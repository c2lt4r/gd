use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;

use crate::core::gd_ast;
use super::invert_if::{get_line_indent, node_text};

// ── Output structs ──────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct SplitDeclarationOutput {
    pub variable: String,
    pub file: String,
    pub line: u32,
    pub applied: bool,
}

#[derive(Serialize, Debug)]
pub struct JoinDeclarationOutput {
    pub variable: String,
    pub file: String,
    pub line: u32,
    pub applied: bool,
}

// ── split-declaration ───────────────────────────────────────────────────────

pub fn split_declaration(
    file: &Path,
    line: usize, // 1-based
    dry_run: bool,
    project_root: &Path,
) -> Result<SplitDeclarationOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let file_ast = gd_ast::convert(&tree, &source);

    let line_0 = line - 1;

    let node = super::find_declaration_by_line(&file_ast, line_0)
        .ok_or_else(|| miette::miette!("no declaration found at line {line}"))?;

    if node.kind() == "const_statement" {
        return Err(miette::miette!("cannot split a const declaration"));
    }

    if node.kind() != "variable_statement" {
        return Err(miette::miette!("line {line} is not a variable declaration"));
    }

    // Must have an initializer (value field)
    let value_node = node
        .child_by_field_name("value")
        .ok_or_else(|| miette::miette!("variable has no initializer to split"))?;

    let name_node = node
        .child_by_field_name("name")
        .ok_or_else(|| miette::miette!("cannot find variable name"))?;
    let var_name = node_text(&name_node, &source);

    let value_text = node_text(&value_node, &source);
    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if dry_run {
        return Ok(SplitDeclarationOutput {
            variable: var_name,
            file: relative_file,
            line: line as u32,
            applied: false,
        });
    }

    // Extract type annotation (if any), handling inferred type `:=`
    let type_text = get_explicit_type(&node, &source);

    // Check for `static` keyword
    let is_static = has_keyword(&node, "static");

    // Collect annotations before the var keyword (e.g., @export, @onready)
    let annotation_prefix = get_annotation_prefix(&node, &source);

    let indent = get_line_indent(&source, line_0);

    // Build declaration line: [annotations] [static] var name[: Type]
    let mut decl_line = String::new();
    decl_line.push_str(&indent);
    if !annotation_prefix.is_empty() {
        decl_line.push_str(&annotation_prefix);
        decl_line.push(' ');
    }
    if is_static {
        decl_line.push_str("static ");
    }
    decl_line.push_str("var ");
    decl_line.push_str(&var_name);
    if let Some(ref ty) = type_text {
        decl_line.push_str(": ");
        decl_line.push_str(ty);
    }

    // Build assignment line
    let assign_line = format!("{indent}{var_name} = {value_text}");

    let new_text = format!("{decl_line}\n{assign_line}");

    // Replace the original node range
    let new_source = splice_line_range(&source, &node, &new_text);

    super::validate_no_new_errors(&source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "split-declaration",
        &format!("split {var_name} at line {line}"),
        &snaps,
        project_root,
    );

    Ok(SplitDeclarationOutput {
        variable: var_name,
        file: relative_file,
        line: line as u32,
        applied: true,
    })
}

// ── join-declaration ────────────────────────────────────────────────────────

pub fn join_declaration(
    file: &Path,
    line: usize, // 1-based
    dry_run: bool,
    project_root: &Path,
) -> Result<JoinDeclarationOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();
    let file_ast = gd_ast::convert(&tree, &source);

    let line_0 = line - 1;

    let node = super::find_declaration_by_line(&file_ast, line_0)
        .ok_or_else(|| miette::miette!("no declaration found at line {line}"))?;

    if node.kind() == "const_statement" {
        return Err(miette::miette!("cannot join a const declaration"));
    }

    if node.kind() != "variable_statement" {
        return Err(miette::miette!("line {line} is not a variable declaration"));
    }

    // Must NOT have an initializer
    if node.child_by_field_name("value").is_some() {
        return Err(miette::miette!(
            "variable already has an initializer; use split-declaration instead"
        ));
    }

    let name_node = node
        .child_by_field_name("name")
        .ok_or_else(|| miette::miette!("cannot find variable name"))?;
    let var_name = node_text(&name_node, &source);

    // Find the next non-comment sibling statement — must be an assignment to the same var
    let match_result = find_next_assignment(root, &node, &var_name, &source)?;

    let right_node = match_result
        .assignment
        .child_by_field_name("right")
        .ok_or_else(|| miette::miette!("cannot find assignment value"))?;
    let value_text = node_text(&right_node, &source);

    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if dry_run {
        return Ok(JoinDeclarationOutput {
            variable: var_name,
            file: relative_file,
            line: line as u32,
            applied: false,
        });
    }

    // Extract type annotation from the declaration
    let type_text = get_explicit_type(&node, &source);
    let is_static = has_keyword(&node, "static");
    let annotation_prefix = get_annotation_prefix(&node, &source);

    let indent = get_line_indent(&source, line_0);

    // Build joined line: [annotations] [static] var name[: Type] = value
    let mut joined = String::new();
    joined.push_str(&indent);
    if !annotation_prefix.is_empty() {
        joined.push_str(&annotation_prefix);
        joined.push(' ');
    }
    if is_static {
        joined.push_str("static ");
    }
    joined.push_str("var ");
    joined.push_str(&var_name);
    if let Some(ref ty) = type_text {
        joined.push_str(": ");
        joined.push_str(ty);
    }
    joined.push_str(" = ");
    joined.push_str(&value_text);

    // Replace from start of declaration to end of assignment statement
    let new_source = splice_two_nodes(&source, &node, &match_result.statement, &joined);

    super::validate_no_new_errors(&source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "join-declaration",
        &format!("join {var_name} at line {line}"),
        &snaps,
        project_root,
    );

    Ok(JoinDeclarationOutput {
        variable: var_name,
        file: relative_file,
        line: line as u32,
        applied: true,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Get the explicit type annotation text (not inferred `:=`).
fn get_explicit_type(node: &tree_sitter::Node, source: &str) -> Option<String> {
    let type_node = node.child_by_field_name("type")?;
    if type_node.kind() == "inferred_type" {
        return None; // `:=` — drop it on split
    }
    Some(node_text(&type_node, source))
}

/// Check if a variable_statement has a `static` keyword.
fn has_keyword(node: &tree_sitter::Node, keyword: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "static_keyword" || child.kind() == keyword {
            return true;
        }
    }
    false
}

/// Collect annotation text before `var`/`static` in the node.
/// e.g., `@export var x = 1` → `@export`
fn get_annotation_prefix(node: &tree_sitter::Node, source: &str) -> String {
    // Annotations in tree-sitter-gdscript are typically part of the node's
    // children or preceding siblings. Check for annotation children first.
    let full_text = node_text(node, source);

    // Find where `var` or `static var` starts in the text
    if let Some(pos) = full_text.find("static var ") {
        let prefix = full_text[..pos].trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    } else if let Some(pos) = full_text.find("var ") {
        let prefix = full_text[..pos].trim();
        if !prefix.is_empty() {
            return prefix.to_string();
        }
    }

    String::new()
}

/// Result of finding the next assignment: the wrapping statement node and the assignment node.
struct AssignmentMatch<'a> {
    /// The outer statement (expression_statement or assignment) — for byte range replacement
    statement: tree_sitter::Node<'a>,
    /// The actual assignment node — for extracting left/right fields
    assignment: tree_sitter::Node<'a>,
}

/// Find the next sibling statement that is an assignment to the given variable.
fn find_next_assignment<'a>(
    root: tree_sitter::Node<'a>,
    decl: &tree_sitter::Node,
    var_name: &str,
    source: &str,
) -> Result<AssignmentMatch<'a>> {
    let decl_line = decl.start_position().row;

    // Walk all top-level children to find the one after our declaration
    let mut cursor = root.walk();
    let mut found_decl = false;
    for child in root.children(&mut cursor) {
        if found_decl {
            // Skip comments
            if child.kind() == "comment" {
                continue;
            }

            // Must be an expression_statement containing an assignment,
            // or a direct assignment node
            if child.kind() == "expression_statement" || child.kind() == "assignment" {
                let assign = if child.kind() == "expression_statement" {
                    let mut inner_cursor = child.walk();
                    child
                        .children(&mut inner_cursor)
                        .find(|c| c.kind() == "assignment")
                } else {
                    Some(child)
                };

                if let Some(assign) = assign {
                    let left = assign
                        .child_by_field_name("left")
                        .ok_or_else(|| miette::miette!("malformed assignment"))?;
                    let left_text = node_text(&left, source);
                    if left_text == var_name {
                        return Ok(AssignmentMatch {
                            statement: child,
                            assignment: assign,
                        });
                    }
                    return Err(miette::miette!(
                        "next statement assigns to '{left_text}', not '{var_name}'"
                    ));
                }
            }

            return Err(miette::miette!(
                "next statement after 'var {var_name}' is not an assignment"
            ));
        }

        if child.start_position().row == decl_line
            && super::DECLARATION_KINDS.contains(&child.kind())
        {
            found_decl = true;
        }
    }

    Err(miette::miette!("no statement found after 'var {var_name}'"))
}

/// Replace a node's line range with new text.
fn splice_line_range(source: &str, node: &tree_sitter::Node, replacement: &str) -> String {
    let start = super::invert_if::line_start_offset(source, node.start_position().row);
    let end = node.end_byte();
    let mut out = String::with_capacity(source.len());
    out.push_str(&source[..start]);
    out.push_str(replacement);
    out.push_str(&source[end..]);
    out
}

/// Replace two nodes (and everything between them) with new text.
fn splice_two_nodes(
    source: &str,
    first: &tree_sitter::Node,
    second: &tree_sitter::Node,
    replacement: &str,
) -> String {
    let start = super::invert_if::line_start_offset(source, first.start_position().row);
    let end = second.end_byte();
    let mut out = String::with_capacity(source.len());
    out.push_str(&source[..start]);
    out.push_str(replacement);
    out.push_str(&source[end..]);
    out
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

    // ── split-declaration tests ─────────────────────────────────────────

    #[test]
    fn split_basic_var() {
        let temp = setup_project(&[("test.gd", "var speed = 10\n")]);
        let result =
            split_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.variable, "speed");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var speed\n"),
            "should have bare declaration, got:\n{content}"
        );
        assert!(
            content.contains("speed = 10"),
            "should have assignment, got:\n{content}"
        );
    }

    #[test]
    fn split_with_type_annotation() {
        let temp = setup_project(&[("test.gd", "var speed: int = 5\n")]);
        let result =
            split_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var speed: int\n"),
            "should preserve type, got:\n{content}"
        );
        assert!(
            content.contains("speed = 5"),
            "should have assignment, got:\n{content}"
        );
    }

    #[test]
    fn split_inferred_type() {
        let temp = setup_project(&[("test.gd", "var speed := 5\n")]);
        let result =
            split_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var speed\n"),
            "should drop inferred type, got:\n{content}"
        );
        assert!(
            content.contains("speed = 5"),
            "should have assignment, got:\n{content}"
        );
        // Must NOT contain `:=` or `: ` in the declaration
        let decl_line = content.lines().next().unwrap();
        assert!(
            !decl_line.contains(':'),
            "declaration should not have colon, got: {decl_line}"
        );
    }

    #[test]
    fn split_dry_run() {
        let original = "var speed = 10\n";
        let temp = setup_project(&[("test.gd", original)]);
        let result = split_declaration(&temp.path().join("test.gd"), 1, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert_eq!(result.variable, "speed");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert_eq!(content, original, "dry run should not modify file");
    }

    #[test]
    fn split_no_initializer_errors() {
        let temp = setup_project(&[("test.gd", "var speed\n")]);
        let result = split_declaration(&temp.path().join("test.gd"), 1, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn split_const_errors() {
        let temp = setup_project(&[("test.gd", "const MAX = 100\n")]);
        let result = split_declaration(&temp.path().join("test.gd"), 1, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn split_static_var() {
        let temp = setup_project(&[("test.gd", "static var count = 0\n")]);
        let result =
            split_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("static var count\n"),
            "should preserve static, got:\n{content}"
        );
        assert!(
            content.contains("count = 0"),
            "should have assignment, got:\n{content}"
        );
    }

    #[test]
    fn split_preserves_indent() {
        let temp = setup_project(&[("test.gd", "func foo():\n\tvar speed = 10\n\treturn speed\n")]);
        // var speed is on line 2 (1-based) but it's inside a func body, not top-level
        // find_declaration_by_line only finds top-level declarations, so this should error
        let result = split_declaration(&temp.path().join("test.gd"), 2, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn split_complex_expression() {
        let temp = setup_project(&[("test.gd", "var velocity = speed * delta\n")]);
        let result =
            split_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var velocity\n"),
            "should have bare declaration, got:\n{content}"
        );
        assert!(
            content.contains("velocity = speed * delta"),
            "should have full expression, got:\n{content}"
        );
    }

    // ── join-declaration tests ──────────────────────────────────────────

    #[test]
    fn join_basic_var() {
        let temp = setup_project(&[("test.gd", "var speed\nspeed = 10\n")]);
        let result = join_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.variable, "speed");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var speed = 10"),
            "should have joined declaration, got:\n{content}"
        );
        // Should be a single line
        let non_empty: Vec<&str> = content.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(non_empty.len(), 1, "should be one line, got:\n{content}");
    }

    #[test]
    fn join_with_type() {
        let temp = setup_project(&[("test.gd", "var speed: int\nspeed = 5\n")]);
        let result = join_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var speed: int = 5"),
            "should have type + value, got:\n{content}"
        );
    }

    #[test]
    fn join_dry_run() {
        let original = "var speed\nspeed = 10\n";
        let temp = setup_project(&[("test.gd", original)]);
        let result = join_declaration(&temp.path().join("test.gd"), 1, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert_eq!(result.variable, "speed");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert_eq!(content, original, "dry run should not modify file");
    }

    #[test]
    fn join_next_not_assignment_errors() {
        let temp = setup_project(&[("test.gd", "var speed\nprint(speed)\n")]);
        let result = join_declaration(&temp.path().join("test.gd"), 1, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn join_different_variable_errors() {
        let temp = setup_project(&[("test.gd", "var speed\nhealth = 100\n")]);
        let result = join_declaration(&temp.path().join("test.gd"), 1, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn join_has_initializer_errors() {
        let temp = setup_project(&[("test.gd", "var speed = 10\nspeed = 20\n")]);
        let result = join_declaration(&temp.path().join("test.gd"), 1, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn round_trip_split_then_join() {
        let original = "var speed = 10\n";
        let temp = setup_project(&[("test.gd", original)]);
        let file = temp.path().join("test.gd");

        // Split
        let split_result = split_declaration(&file, 1, false, temp.path()).unwrap();
        assert!(split_result.applied);
        let after_split = fs::read_to_string(&file).unwrap();
        assert_ne!(after_split, original);

        // Join
        let join_result = join_declaration(&file, 1, false, temp.path()).unwrap();
        assert!(join_result.applied);
        let after_join = fs::read_to_string(&file).unwrap();
        assert_eq!(
            after_join, original,
            "round-trip should return to original, got:\n{after_join}"
        );
    }

    #[test]
    fn round_trip_with_type() {
        let original = "var speed: int = 5\n";
        let temp = setup_project(&[("test.gd", original)]);
        let file = temp.path().join("test.gd");

        split_declaration(&file, 1, false, temp.path()).unwrap();
        join_declaration(&file, 1, false, temp.path()).unwrap();
        let after = fs::read_to_string(&file).unwrap();
        assert_eq!(
            after, original,
            "round-trip should return to original, got:\n{after}"
        );
    }

    #[test]
    fn join_skips_comments() {
        let temp = setup_project(&[("test.gd", "var speed\n# initialize speed\nspeed = 10\n")]);
        let result = join_declaration(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var speed = 10"),
            "should join despite comment, got:\n{content}"
        );
    }
}
