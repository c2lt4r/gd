use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;

use super::invert_if::node_text;

// ── Output ──────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct EncapsulateFieldOutput {
    pub variable: String,
    pub style: String, // "inline" or "backing-field"
    pub file: String,
    pub line: u32,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

// ── Public entry point ──────────────────────────────────────────────────────

pub fn encapsulate_field(
    file: &Path,
    name: &str,
    backing_field: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<EncapsulateFieldOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();
    let relative_file = crate::core::fs::relative_slash(file, project_root);

    let var_node = super::find_declaration_by_name(root, &source, name)
        .ok_or_else(|| miette::miette!("variable '{name}' not found"))?;

    if var_node.kind() != "variable_statement" {
        return Err(miette::miette!("'{name}' is not a variable declaration"));
    }

    // Check it's not a const
    if has_const_keyword(&var_node) {
        return Err(miette::miette!("cannot encapsulate a constant"));
    }

    let line = var_node.start_position().row + 1;
    let style = if backing_field {
        "backing-field"
    } else {
        "inline"
    };

    if dry_run {
        return Ok(EncapsulateFieldOutput {
            variable: name.to_string(),
            style: style.to_string(),
            file: relative_file,
            line: line as u32,
            applied: false,
            warnings: Vec::new(),
        });
    }

    let mut warnings = Vec::new();

    // Detect existing set/get on the property (already encapsulated)
    if has_setget(&var_node, &source) {
        return Err(miette::miette!("'{name}' already has set/get accessors"));
    }

    // Detect collision with getter/setter names
    if backing_field {
        let getter = format!("_get_{name}");
        let setter = format!("_set_{name}");
        let backing = format!("_{name}");

        if super::find_declaration_by_name(root, &source, &getter).is_some() {
            warnings.push(format!(
                "function '{getter}' already exists — will be overwritten"
            ));
        }
        if super::find_declaration_by_name(root, &source, &setter).is_some() {
            warnings.push(format!(
                "function '{setter}' already exists — will be overwritten"
            ));
        }
        if super::find_declaration_by_name(root, &source, &backing).is_some() {
            return Err(miette::miette!(
                "variable '{backing}' already exists — cannot create backing field"
            ));
        }
    }

    let new_source = if backing_field {
        generate_backing_field(&source, &var_node, name)
    } else {
        generate_inline_accessors(&source, &var_node, name)
    };

    super::validate_no_new_errors(&source, &new_source)?;

    let mut tx = super::transaction::RefactorTransaction::new();
    tx.write_file(file, &new_source)?;
    let snapshots = tx.into_snapshots();

    // Record undo
    let mut undo_snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    undo_snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "encapsulate-field",
        &format!("encapsulate {name} ({style})"),
        &undo_snaps,
        project_root,
    );
    drop(snapshots);

    Ok(EncapsulateFieldOutput {
        variable: name.to_string(),
        style: style.to_string(),
        file: relative_file,
        line: line as u32,
        applied: true,
        warnings,
    })
}

// ── Inline accessor generation (GDScript 4 property syntax) ─────────────────

fn generate_inline_accessors(source: &str, var_node: &tree_sitter::Node, name: &str) -> String {
    let type_text = extract_type_text(var_node, source);
    let value_text = var_node
        .child_by_field_name("value")
        .map(|v| node_text(&v, source));
    let annotation_prefix = get_annotation_prefix(var_node, source);
    let indent = get_var_indent(source, var_node.start_position().row);

    // Build: var name[: Type][ = value]:
    let mut decl = String::new();
    decl.push_str(&indent);
    if !annotation_prefix.is_empty() {
        decl.push_str(&annotation_prefix);
        decl.push(' ');
    }
    decl.push_str("var ");
    decl.push_str(name);
    if let Some(ref ty) = type_text {
        decl.push_str(": ");
        decl.push_str(ty);
    }
    if let Some(ref val) = value_text {
        decl.push_str(" = ");
        decl.push_str(val);
    }
    decl.push(':');

    // set(value):
    let body_indent = format!("{indent}\t");
    let inner_indent = format!("{indent}\t\t");
    let mut setter = String::new();
    setter.push_str(&body_indent);
    setter.push_str("set(value):");
    setter.push('\n');
    setter.push_str(&inner_indent);
    setter.push_str(name);
    setter.push_str(" = value");

    // get:
    let mut getter = String::new();
    getter.push_str(&body_indent);
    getter.push_str("get:");
    getter.push('\n');
    getter.push_str(&inner_indent);
    getter.push_str("return ");
    getter.push_str(name);

    let replacement = format!("{decl}\n{setter}\n{getter}");

    // Replace the original variable declaration
    let start = line_byte_offset(source, var_node.start_position().row);
    let end = node_end_with_newline(source, var_node);

    let mut new_source = String::with_capacity(source.len() + replacement.len());
    new_source.push_str(&source[..start]);
    new_source.push_str(&replacement);
    new_source.push('\n');
    new_source.push_str(&source[end..]);

    new_source
}

// ── Backing field generation (_name + getter/setter functions) ───────────────

fn generate_backing_field(source: &str, var_node: &tree_sitter::Node, name: &str) -> String {
    let type_text = extract_type_text(var_node, source);
    let value_text = var_node
        .child_by_field_name("value")
        .map(|v| node_text(&v, source));
    let annotation_prefix = get_annotation_prefix(var_node, source);
    let indent = get_var_indent(source, var_node.start_position().row);

    let backing_name = format!("_{name}");

    // Build: [annotations] var _name[: Type][ = value]
    let mut decl = String::new();
    decl.push_str(&indent);
    if !annotation_prefix.is_empty() {
        decl.push_str(&annotation_prefix);
        decl.push(' ');
    }
    decl.push_str("var ");
    decl.push_str(&backing_name);
    if let Some(ref ty) = type_text {
        decl.push_str(": ");
        decl.push_str(ty);
    }
    if let Some(ref val) = value_text {
        decl.push_str(" = ");
        decl.push_str(val);
    }

    // Build getter function
    let return_type = type_text
        .as_ref()
        .map_or(String::new(), |ty| format!(" -> {ty}"));
    let getter =
        format!("\n\n{indent}func _get_{name}(){return_type}:\n{indent}\treturn {backing_name}");

    // Build setter function
    let param_type = type_text
        .as_ref()
        .map_or(String::new(), |ty| format!(": {ty}"));
    let setter = format!(
        "\n\n{indent}func _set_{name}(value{param_type}) -> void:\n{indent}\t{backing_name} = value"
    );

    let replacement = format!("{decl}{getter}{setter}");

    // Replace the original variable declaration
    let start = line_byte_offset(source, var_node.start_position().row);
    let end = node_end_with_newline(source, var_node);

    let mut new_source = String::with_capacity(source.len() + replacement.len());
    new_source.push_str(&source[..start]);
    new_source.push_str(&replacement);
    new_source.push('\n');
    new_source.push_str(&source[end..]);

    new_source
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Check if a variable_statement has a `const` keyword (should be const_statement, but be safe).
fn has_const_keyword(node: &tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "const" {
            return true;
        }
    }
    false
}

/// Check if a variable_statement already has set/get accessors (property syntax).
fn has_setget(node: &tree_sitter::Node, source: &str) -> bool {
    let text = node_text(node, source);
    // GDScript 4 property syntax uses a colon after the declaration,
    // followed by set()/get: blocks. Check for setget children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "setget" || child.kind() == "setter" || child.kind() == "getter" {
            return true;
        }
    }
    // Also check the text for `set(` or `get:` patterns after the value
    // In case tree-sitter represents it differently
    if text.contains("\n\tset(")
        || text.contains("\n\tget:")
        || text.contains("\n    set(")
        || text.contains("\n    get:")
    {
        return true;
    }
    false
}

/// Extract the type annotation text (e.g., "int", "Node2D") if present.
fn extract_type_text(var_node: &tree_sitter::Node, source: &str) -> Option<String> {
    let type_node = var_node.child_by_field_name("type")?;
    if type_node.kind() == "inferred_type" {
        return None;
    }
    Some(node_text(&type_node, source))
}

/// Collect annotation text before `var`/`static` in the node.
fn get_annotation_prefix(node: &tree_sitter::Node, source: &str) -> String {
    let full_text = node_text(node, source);
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
fn node_end_with_newline(source: &str, node: &tree_sitter::Node) -> usize {
    let mut end = node.end_byte();
    if end < source.len() && source.as_bytes()[end] == b'\n' {
        end += 1;
    }
    end
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

    // ── Inline accessor tests ───────────────────────────────────────────

    #[test]
    fn inline_simple_var() {
        let temp = setup_project(&[("test.gd", "var health = 100\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.style, "inline");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var health = 100:"),
            "should have property declaration, got:\n{content}"
        );
        assert!(
            content.contains("set(value):"),
            "should have setter, got:\n{content}"
        );
        assert!(
            content.contains("health = value"),
            "setter should assign, got:\n{content}"
        );
        assert!(
            content.contains("get:"),
            "should have getter, got:\n{content}"
        );
        assert!(
            content.contains("return health"),
            "getter should return, got:\n{content}"
        );
    }

    #[test]
    fn inline_typed_var() {
        let temp = setup_project(&[("test.gd", "var health: int = 100\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var health: int = 100:"),
            "should preserve type, got:\n{content}"
        );
    }

    #[test]
    fn inline_no_default_value() {
        let temp = setup_project(&[("test.gd", "var health: int\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var health: int:"),
            "should have property without default, got:\n{content}"
        );
        assert!(
            content.contains("set(value):"),
            "should have setter, got:\n{content}"
        );
    }

    #[test]
    fn inline_with_export() {
        let temp = setup_project(&[("test.gd", "@export var health: int = 100\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("@export var health: int = 100:"),
            "should preserve @export, got:\n{content}"
        );
    }

    // ── Backing field tests ─────────────────────────────────────────────

    #[test]
    fn backing_field_simple() {
        let temp = setup_project(&[("test.gd", "var health = 100\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.style, "backing-field");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var _health = 100"),
            "should have backing field, got:\n{content}"
        );
        assert!(
            content.contains("func _get_health()"),
            "should have getter func, got:\n{content}"
        );
        assert!(
            content.contains("return _health"),
            "getter should return backing, got:\n{content}"
        );
        assert!(
            content.contains("func _set_health(value)"),
            "should have setter func, got:\n{content}"
        );
        assert!(
            content.contains("_health = value"),
            "setter should assign to backing, got:\n{content}"
        );
    }

    #[test]
    fn backing_field_typed() {
        let temp = setup_project(&[("test.gd", "var health: int = 100\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("var _health: int = 100"),
            "should preserve type on backing field, got:\n{content}"
        );
        assert!(
            content.contains("func _get_health() -> int:"),
            "getter should have return type, got:\n{content}"
        );
        assert!(
            content.contains("func _set_health(value: int) -> void:"),
            "setter should have param type, got:\n{content}"
        );
    }

    #[test]
    fn backing_field_with_export() {
        let temp = setup_project(&[("test.gd", "@export var health: int = 100\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("@export var _health: int = 100"),
            "should preserve @export, got:\n{content}"
        );
    }

    #[test]
    fn backing_field_collision_errors() {
        let temp = setup_project(&[("test.gd", "var health = 100\nvar _health = 50\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            true,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    // ── Dry run tests ───────────────────────────────────────────────────

    #[test]
    fn dry_run_no_modify() {
        let original = "var health = 100\n";
        let temp = setup_project(&[("test.gd", original)]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            false,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert_eq!(content, original, "dry run should not modify file");
    }

    // ── Error cases ─────────────────────────────────────────────────────

    #[test]
    fn not_a_variable_errors() {
        let temp = setup_project(&[("test.gd", "func health():\n\tpass\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn not_found_errors() {
        let temp = setup_project(&[("test.gd", "var speed = 10\n")]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "nonexistent",
            false,
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn inline_preserves_surrounding_code() {
        let temp = setup_project(&[(
            "test.gd",
            "extends Node\n\nvar health = 100\n\nfunc _ready():\n\tpass\n",
        )]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("extends Node"),
            "should preserve extends, got:\n{content}"
        );
        assert!(
            content.contains("func _ready():"),
            "should preserve other code, got:\n{content}"
        );
    }

    #[test]
    fn backing_field_preserves_surrounding_code() {
        let temp = setup_project(&[(
            "test.gd",
            "extends Node\n\nvar health = 100\n\nfunc _ready():\n\tpass\n",
        )]);
        let result = encapsulate_field(
            &temp.path().join("test.gd"),
            "health",
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("extends Node"),
            "should preserve extends, got:\n{content}"
        );
        assert!(
            content.contains("func _ready():"),
            "should preserve other code, got:\n{content}"
        );
    }
}
