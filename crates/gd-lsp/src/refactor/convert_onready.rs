use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::invert_if::node_text;
use gd_core::gd_ast;

// ── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ConvertOnreadyOutput {
    pub variable: String,
    pub direction: String, // "to-ready" or "to-onready"
    pub file: String,
    pub line: u32,
    pub applied: bool,
}

// ── Public entry point ──────────────────────────────────────────────────────

pub fn convert_onready(
    file: &Path,
    name: &str,
    to_ready: bool, // true = @onready → _ready(), false = _ready() → @onready
    dry_run: bool,
    project_root: &Path,
) -> Result<ConvertOnreadyOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let gd_file = gd_ast::convert(&tree, &source);
    let relative_file = gd_core::fs::relative_slash(file, project_root);

    if to_ready {
        convert_to_ready(
            file,
            name,
            &source,
            &gd_file,
            dry_run,
            &relative_file,
            project_root,
        )
    } else {
        convert_to_onready(
            file,
            name,
            &source,
            &gd_file,
            dry_run,
            &relative_file,
            project_root,
        )
    }
}

// ── @onready var → _ready() assignment ──────────────────────────────────────

fn convert_to_ready(
    file: &Path,
    name: &str,
    source: &str,
    gd_file: &gd_ast::GdFile,
    dry_run: bool,
    relative_file: &str,
    project_root: &Path,
) -> Result<ConvertOnreadyOutput> {
    // Find the variable declaration
    let var_node = super::find_declaration_by_name(gd_file, name)
        .ok_or_else(|| miette::miette!("variable '{name}' not found"))?;

    if var_node.kind() != "variable_statement" {
        return Err(miette::miette!("'{name}' is not a variable declaration"));
    }

    // Verify it has @onready
    if !has_annotation(&var_node, source, "onready") {
        return Err(miette::miette!(
            "'{name}' does not have @onready annotation"
        ));
    }

    // Extract the value expression
    let value_node = var_node
        .child_by_field_name("value")
        .ok_or_else(|| miette::miette!("@onready var '{name}' has no default value"))?;
    let value_text = node_text(&value_node, source);

    // Extract type annotation if present
    let type_text = extract_type_text(&var_node, source);

    let line = var_node.start_position().row + 1;

    if dry_run {
        return Ok(ConvertOnreadyOutput {
            variable: name.to_string(),
            direction: "to-ready".to_string(),
            file: relative_file.to_string(),
            line: line as u32,
            applied: false,
        });
    }

    // Build the new var declaration (without @onready, without default value)
    let new_var = if let Some(ref type_ann) = type_text {
        format!("var {name}{type_ann}")
    } else {
        format!("var {name}")
    };

    // Build the assignment line for _ready()
    let assignment = format!("{name} = {value_text}");

    // Replace the variable declaration (strip @onready + default)
    let var_line_start = line_byte_offset(source, var_node.start_position().row);
    let var_line_end = node_end_with_newline(source, &var_node);
    let indent = get_var_indent(source, var_node.start_position().row);

    let mut new_source = String::with_capacity(source.len());
    new_source.push_str(&source[..var_line_start]);
    new_source.push_str(&indent);
    new_source.push_str(&new_var);
    new_source.push('\n');
    new_source.push_str(&source[var_line_end..]);

    // Now add assignment to _ready()
    new_source = add_to_ready(&new_source, &assignment)?;

    super::validate_no_new_errors(source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    record_undo(file, source, project_root, name, "to-ready");

    Ok(ConvertOnreadyOutput {
        variable: name.to_string(),
        direction: "to-ready".to_string(),
        file: relative_file.to_string(),
        line: line as u32,
        applied: true,
    })
}

// ── _ready() assignment → @onready var ──────────────────────────────────────

fn convert_to_onready(
    file: &Path,
    name: &str,
    source: &str,
    gd_file: &gd_ast::GdFile,
    dry_run: bool,
    relative_file: &str,
    project_root: &Path,
) -> Result<ConvertOnreadyOutput> {
    // Find the variable declaration (must exist as bare `var name`)
    let var_node = super::find_declaration_by_name(gd_file, name)
        .ok_or_else(|| miette::miette!("variable '{name}' not found"))?;

    if var_node.kind() != "variable_statement" {
        return Err(miette::miette!("'{name}' is not a variable declaration"));
    }

    if has_annotation(&var_node, source, "onready") {
        return Err(miette::miette!("'{name}' already has @onready"));
    }

    // Find _ready() and the assignment for this variable
    let ready_func = super::find_declaration_by_name(gd_file, "_ready")
        .ok_or_else(|| miette::miette!("no _ready() function found"))?;

    let (assignment_value, assignment_stmt) = find_assignment_in_func(&ready_func, source, name)?;

    let line = var_node.start_position().row + 1;

    if dry_run {
        return Ok(ConvertOnreadyOutput {
            variable: name.to_string(),
            direction: "to-onready".to_string(),
            file: relative_file.to_string(),
            line: line as u32,
            applied: false,
        });
    }

    // Extract type annotation
    let type_text = extract_type_text(&var_node, source);

    // Build @onready var with the value from _ready()
    let new_var = if let Some(ref type_ann) = type_text {
        format!("@onready var {name}{type_ann} = {assignment_value}")
    } else {
        format!("@onready var {name} = {assignment_value}")
    };

    // We need to:
    // 1. Replace the var declaration with @onready version
    // 2. Remove the assignment from _ready()
    // Process from bottom to top to keep byte offsets valid

    // Remove assignment from _ready() first (it's lower in the file)
    let assign_line_start = line_byte_offset(source, assignment_stmt.start_position().row);
    let assign_line_end = node_end_with_newline(source, &assignment_stmt);

    let mut new_source = String::with_capacity(source.len());
    new_source.push_str(&source[..assign_line_start]);
    new_source.push_str(&source[assign_line_end..]);

    // Check if _ready() is now empty (only has pass or nothing) — if so, check
    // but don't remove (user can do that separately)

    // Now replace the var declaration (use updated source offsets)
    let updated_tree = gd_core::parser::parse(&new_source)?;
    let updated_file = gd_ast::convert(&updated_tree, &new_source);
    let updated_var = super::find_declaration_by_name(&updated_file, name)
        .ok_or_else(|| miette::miette!("lost variable '{name}' during transform"))?;

    let var_line_start = line_byte_offset(&new_source, updated_var.start_position().row);
    let var_line_end = node_end_with_newline(&new_source, &updated_var);
    let indent = get_var_indent(&new_source, updated_var.start_position().row);

    let mut final_source = String::with_capacity(new_source.len());
    final_source.push_str(&new_source[..var_line_start]);
    final_source.push_str(&indent);
    final_source.push_str(&new_var);
    final_source.push('\n');
    final_source.push_str(&new_source[var_line_end..]);

    // If _ready() body is now empty, insert `pass`
    final_source = ensure_ready_not_empty(&final_source)?;

    super::validate_no_new_errors(source, &final_source)?;
    std::fs::write(file, &final_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    record_undo(file, source, project_root, name, "to-onready");

    Ok(ConvertOnreadyOutput {
        variable: name.to_string(),
        direction: "to-onready".to_string(),
        file: relative_file.to_string(),
        line: line as u32,
        applied: true,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Check if a node has a specific annotation (e.g., "onready").
fn has_annotation(node: &Node, source: &str, annotation: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut annot_cursor = child.walk();
            for annot in child.children(&mut annot_cursor) {
                if annot.kind() == "annotation" {
                    let text = node_text(&annot, source);
                    if text.strip_prefix('@').unwrap_or(&text) == annotation {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Extract the type annotation text (e.g., ": int", ": Node2D") if present.
fn extract_type_text(var_node: &Node, source: &str) -> Option<String> {
    let type_node = var_node.child_by_field_name("type")?;
    let text = node_text(&type_node, source);
    if type_node.kind() == "inferred_type" {
        Some(format!(" {text}"))
    } else {
        Some(format!(": {text}"))
    }
}

/// Get the indentation of a given line.
fn get_var_indent(source: &str, line: usize) -> String {
    if let Some(line_str) = source.lines().nth(line) {
        let trimmed = line_str.trim_start();
        line_str[..line_str.len() - trimmed.len()].to_string()
    } else {
        String::new()
    }
}

/// Get byte offset of a line start.
fn line_byte_offset(source: &str, line: usize) -> usize {
    let mut current = 0;
    for (i, ch) in source.char_indices() {
        if current == line {
            return i;
        }
        if ch == '\n' {
            current += 1;
        }
    }
    source.len()
}

/// Get the byte offset past the node, including its trailing newline.
fn node_end_with_newline(source: &str, node: &Node) -> usize {
    let mut end = node.end_byte();
    if end < source.len() && source.as_bytes()[end] == b'\n' {
        end += 1;
    }
    end
}

/// Find an assignment `name = <expr>` inside a function body.
/// Returns (value_text, statement_node).
fn find_assignment_in_func<'a>(
    func_node: &Node<'a>,
    source: &str,
    name: &str,
) -> Result<(String, Node<'a>)> {
    let body = func_node
        .child_by_field_name("body")
        .ok_or_else(|| miette::miette!("_ready() has no body"))?;

    let mut cursor = body.walk();
    for stmt in body.children(&mut cursor) {
        if stmt.kind() == "comment" {
            continue;
        }

        // Assignments can be `expression_statement > assignment` or direct
        let assign = if stmt.kind() == "expression_statement" {
            let mut inner_cursor = stmt.walk();
            stmt.children(&mut inner_cursor)
                .find(|c| c.kind() == "assignment")
        } else if stmt.kind() == "assignment" {
            Some(stmt)
        } else {
            None
        };

        if let Some(assign_node) = assign {
            // Check left side is our variable
            let mut ac = assign_node.walk();
            let children: Vec<Node> = assign_node.children(&mut ac).collect();
            if children.len() >= 3 {
                let left = &children[0];
                if left.kind() == "identifier" && node_text(left, source) == name {
                    // Check it's a simple `=` assignment (not +=, -= etc.)
                    let op = &children[1];
                    let op_text = node_text(op, source);
                    if op_text != "=" {
                        return Err(miette::miette!(
                            "assignment to '{name}' uses compound operator '{op_text}', cannot convert"
                        ));
                    }
                    let right = &children[2];
                    let value_text = node_text(right, source);
                    // Return the expression_statement node (or assignment if top-level)
                    // so we remove the entire statement line
                    let remove_node = if stmt.kind() == "expression_statement" {
                        stmt
                    } else {
                        assign_node
                    };
                    return Ok((value_text, remove_node));
                }
            }
        }
    }

    Err(miette::miette!(
        "no assignment to '{name}' found in _ready()"
    ))
}

/// Add an assignment line to _ready(), creating the function if it doesn't exist.
fn add_to_ready(source: &str, assignment: &str) -> Result<String> {
    let tree = gd_core::parser::parse(source)?;
    let file = gd_ast::convert(&tree, source);

    if let Some(ready_func) = super::find_declaration_by_name(&file, "_ready") {
        // _ready() exists — append to its body
        let body = ready_func
            .child_by_field_name("body")
            .ok_or_else(|| miette::miette!("_ready() has no body"))?;

        // Check if body only has `pass` — if so, replace it
        let body_text = node_text(&body, source);
        let body_trimmed = body_text.trim();

        if body_trimmed == "pass" {
            // Replace body contents using byte offsets (not splice, which
            // uses line_start_offset and would clobber the func header)
            let mut new_source = String::with_capacity(source.len());
            new_source.push_str(&source[..body.start_byte()]);
            write!(new_source, "\n\t{assignment}").unwrap();
            new_source.push_str(&source[body.end_byte()..]);
            return Ok(new_source);
        }

        // Append to end of body
        let body_end = body.end_byte();
        let mut new_source = String::with_capacity(source.len() + assignment.len() + 2);
        new_source.push_str(&source[..body_end]);
        write!(new_source, "\n\t{assignment}").unwrap();
        new_source.push_str(&source[body_end..]);
        Ok(new_source)
    } else {
        // _ready() doesn't exist — create it at end of top-level declarations
        let ready_func = format!("\nfunc _ready():\n\t{assignment}\n");
        let mut new_source = source.trim_end().to_string();
        new_source.push_str(&ready_func);
        Ok(new_source)
    }
}

/// If _ready() exists and its body is empty or only whitespace, insert `pass`.
fn ensure_ready_not_empty(source: &str) -> Result<String> {
    let tree = gd_core::parser::parse(source)?;
    let file = gd_ast::convert(&tree, source);

    let Some(ready_func) = super::find_declaration_by_name(&file, "_ready") else {
        return Ok(source.to_string());
    };

    let Some(body) = ready_func.child_by_field_name("body") else {
        return Ok(source.to_string());
    };

    // Check if body has any non-comment statements
    let mut cursor = body.walk();
    let has_stmts = body
        .children(&mut cursor)
        .any(|c| c.is_named() && c.kind() != "comment" && c.kind() != "pass_statement");

    if !has_stmts {
        // Body is empty — check if it even has pass
        let body_text = node_text(&body, source);
        if !body_text.trim().contains("pass") {
            let mut new_source = String::with_capacity(source.len());
            new_source.push_str(&source[..body.start_byte()]);
            new_source.push_str("\n\tpass");
            new_source.push_str(&source[body.end_byte()..]);
            return Ok(new_source);
        }
    }

    Ok(source.to_string())
}

fn record_undo(file: &Path, source: &str, project_root: &Path, name: &str, direction: &str) {
    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "convert-onready",
        &format!("convert {name} {direction}"),
        &snaps,
        project_root,
    );
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .expect("write project.godot");
        for (name, content) in files {
            fs::write(temp.path().join(name), content).expect("write file");
        }
        temp
    }

    #[test]
    fn onready_to_ready_simple() {
        let temp = setup_project(&[(
            "test.gd",
            "@onready var sprite = $Sprite2D\n\nfunc _ready():\n\tpass\n",
        )]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-ready");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var sprite"),
            "should have bare var, got:\n{content}"
        );
        assert!(
            !content.contains("@onready"),
            "should remove @onready, got:\n{content}"
        );
        assert!(
            content.contains("sprite = $Sprite2D"),
            "should add assignment to _ready(), got:\n{content}"
        );
    }

    #[test]
    fn onready_to_ready_creates_ready() {
        let temp = setup_project(&[("test.gd", "@onready var sprite = $Sprite2D\n")]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("func _ready():"),
            "should create _ready(), got:\n{content}"
        );
        assert!(
            content.contains("sprite = $Sprite2D"),
            "should have assignment in _ready(), got:\n{content}"
        );
    }

    #[test]
    fn onready_to_ready_with_type() {
        let temp = setup_project(&[(
            "test.gd",
            "@onready var sprite: Sprite2D = $Sprite2D\n\nfunc _ready():\n\tpass\n",
        )]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var sprite: Sprite2D"),
            "should preserve type annotation, got:\n{content}"
        );
    }

    #[test]
    fn ready_to_onready_simple() {
        let temp = setup_project(&[(
            "test.gd",
            "var sprite\n\nfunc _ready():\n\tsprite = $Sprite2D\n",
        )]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.direction, "to-onready");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("@onready var sprite = $Sprite2D"),
            "should have @onready, got:\n{content}"
        );
    }

    #[test]
    fn ready_to_onready_with_type() {
        let temp = setup_project(&[(
            "test.gd",
            "var sprite: Sprite2D\n\nfunc _ready():\n\tsprite = $Sprite2D\n",
        )]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("@onready var sprite: Sprite2D = $Sprite2D"),
            "should preserve type and add @onready, got:\n{content}"
        );
    }

    #[test]
    fn dry_run_no_modify() {
        let original = "@onready var sprite = $Sprite2D\n\nfunc _ready():\n\tpass\n";
        let temp = setup_project(&[("test.gd", original)]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            true,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert_eq!(content, original, "dry run should not modify file");
    }

    #[test]
    fn ready_to_onready_preserves_other_ready_code() {
        let temp = setup_project(&[(
            "test.gd",
            "var sprite\n\nfunc _ready():\n\tsprite = $Sprite2D\n\tprint(\"ready\")\n",
        )]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("@onready var sprite = $Sprite2D"),
            "should have @onready, got:\n{content}"
        );
        assert!(
            content.contains("print(\"ready\")"),
            "should preserve other _ready() code, got:\n{content}"
        );
    }

    #[test]
    fn no_onready_errors() {
        let temp = setup_project(&[("test.gd", "var sprite = null\n")]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            true,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn already_onready_errors() {
        let temp = setup_project(&[(
            "test.gd",
            "@onready var sprite = $Sprite2D\n\nfunc _ready():\n\tpass\n",
        )]);
        let result = convert_onready(
            &temp.path().join("test.gd"),
            "sprite",
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }
}
