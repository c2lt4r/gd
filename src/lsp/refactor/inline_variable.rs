use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::{
    collision::collect_scope_names, find_declaration_by_name, line_starts, normalize_blank_lines,
};

// ── inline-variable ─────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct InlineVariableOutput {
    pub variable: String,
    pub expression: String,
    pub definition_line: u32,
    pub reference_count: u32,
    pub file: String,
    pub applied: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[allow(clippy::too_many_lines)]
pub fn inline_variable(
    file: &Path,
    line: usize,   // 1-based
    column: usize, // 1-based
    dry_run: bool,
    project_root: &Path,
) -> Result<InlineVariableOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let point = tree_sitter::Point::new(line - 1, column - 1);

    // Find identifier at position
    let ident = root
        .descendant_for_point_range(point, point)
        .ok_or_else(|| miette::miette!("no node found at {line}:{column}"))?;
    let var_name = ident
        .utf8_text(source.as_bytes())
        .map_err(|e| miette::miette!("cannot read identifier: {e}"))?;
    if ident.kind() != "identifier" && ident.kind() != "name" {
        return Err(miette::miette!(
            "expected identifier at {line}:{column}, found '{}'",
            ident.kind()
        ));
    }

    // Find the variable declaration
    let (decl, is_local) = find_var_declaration(root, &source, var_name, point)?;

    // Validate: must be var, not const
    if decl.kind() == "const_statement" {
        return Err(miette::miette!(
            "cannot inline constant '{var_name}' — constants should stay as named references"
        ));
    }
    if decl.kind() != "variable_statement" {
        return Err(miette::miette!(
            "'{var_name}' is not a variable declaration (found {})",
            decl.kind()
        ));
    }

    // Validate: not @onready — value depends on scene tree readiness
    if has_onready_annotation(decl, root, &source) {
        return Err(miette::miette!(
            "cannot inline @onready variable '{var_name}' — its value depends on scene tree readiness"
        ));
    }

    // Must have initializer
    let init_node = decl
        .child_by_field_name("value")
        .ok_or_else(|| miette::miette!("variable '{var_name}' has no initializer"))?;
    let init_text = init_node
        .utf8_text(source.as_bytes())
        .map_err(|e| miette::miette!("cannot read initializer: {e}"))?
        .to_string();
    let init_kind = init_node.kind();

    // Check for type annotation loss
    let type_warning = extract_type_annotation_text(decl, &source);

    // Determine scope for usage search and reassignment check
    let scope_node = if is_local {
        crate::lsp::references::enclosing_function(root, point)
            .ok_or_else(|| miette::miette!("cannot find enclosing function"))?
    } else {
        root
    };

    // Check: not reassigned after declaration
    if is_reassigned(scope_node, &source, var_name, decl.end_byte()) {
        return Err(miette::miette!(
            "variable '{var_name}' is reassigned after declaration — cannot safely inline"
        ));
    }

    // Collect all usages (excluding the declaration itself)
    let usages = collect_usages(scope_node, &source, var_name, decl);

    let mut warnings = Vec::new();
    if usages.is_empty() {
        warnings.push(format!(
            "variable '{var_name}' has no usages — only the declaration will be removed"
        ));
    }
    if let Some(type_text) = &type_warning {
        warnings.push(format!("type annotation ': {type_text}' will be lost after inlining"));
    }

    // Collision check: available for future use to warn about shadowing issues
    // when inlining into different scopes. Currently a no-op since inline-variable
    // operates within a single scope.
    let _scope_names = collect_scope_names(root, &source, decl.start_position());

    let decl_line = decl.start_position().row as u32 + 1;
    let reference_count = usages.len() as u32;
    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if !dry_run {
        let mut new_source = source.clone();

        // Replace usages bottom-to-top by byte offset
        let mut sorted_usages: Vec<_> = usages.iter().collect();
        sorted_usages.sort_by_key(|u| std::cmp::Reverse(u.start_byte()));

        for usage in &sorted_usages {
            let replacement = if needs_parens(init_kind, usage.parent()) {
                format!("({init_text})")
            } else {
                init_text.clone()
            };
            new_source.replace_range(usage.start_byte()..usage.end_byte(), &replacement);
        }

        // Delete the declaration line
        let starts = line_starts(&new_source);
        let tree2 = crate::core::parser::parse(&new_source)?;
        let root2 = tree2.root_node();

        // Find declaration in modified source by name
        let decl2 = if is_local {
            crate::lsp::references::enclosing_function(root2, point)
                .and_then(|f| find_local_var_decl(f, &new_source, var_name))
        } else {
            find_declaration_by_name(root2, &new_source, var_name)
        };

        if let Some(decl_node) = decl2 {
            let decl_start_line = decl_node.start_position().row;
            let decl_end_line = decl_node.end_position().row;
            let remove_start = starts[decl_start_line];
            let remove_end = if decl_end_line + 1 < starts.len() {
                starts[decl_end_line + 1]
            } else {
                new_source.len()
            };
            new_source.replace_range(remove_start..remove_end, "");
        }

        normalize_blank_lines(&mut new_source);
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "inline-variable",
            &format!("inline {var_name}"),
            &snaps,
            project_root,
        );
    }

    Ok(InlineVariableOutput {
        variable: var_name.to_string(),
        expression: init_text,
        definition_line: decl_line,
        reference_count,
        file: relative_file,
        applied: !dry_run,
        warnings,
    })
}

// ── Private helpers ─────────────────────────────────────────────────────────

/// Find variable declaration: first check local scope, then file-level.
/// Returns (declaration_node, is_local).
fn find_var_declaration<'a>(
    root: Node<'a>,
    source: &str,
    name: &str,
    position: tree_sitter::Point,
) -> Result<(Node<'a>, bool)> {
    // Check local scope first
    if let Some(func) = crate::lsp::references::enclosing_function(root, position)
        && let Some(decl) = find_local_var_decl(func, source, name)
    {
        return Ok((decl, true));
    }

    // Check file-level
    if let Some(decl) = find_declaration_by_name(root, source, name) {
        return Ok((decl, false));
    }

    Err(miette::miette!(
        "cannot find declaration of '{name}' in this file"
    ))
}

/// Find a local variable declaration in a function body.
fn find_local_var_decl<'a>(func: Node<'a>, source: &str, name: &str) -> Option<Node<'a>> {
    let body = func.child_by_field_name("body")?;
    find_var_in_body(body, source, name)
}

fn find_var_in_body<'a>(body: Node<'a>, source: &str, name: &str) -> Option<Node<'a>> {
    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if (child.kind() == "variable_statement" || child.kind() == "const_statement")
            && let Some(name_node) = child.child_by_field_name("name")
            && name_node.utf8_text(source.as_bytes()).ok() == Some(name)
        {
            return Some(child);
        }
    }
    None
}

/// Check if `var_name` is reassigned after `after_byte`.
fn is_reassigned(scope: Node, source: &str, var_name: &str, after_byte: usize) -> bool {
    let mut found = false;
    check_reassignment(scope, source, var_name, after_byte, &mut found);
    found
}

fn check_reassignment(
    node: Node,
    source: &str,
    var_name: &str,
    after_byte: usize,
    found: &mut bool,
) {
    if *found {
        return;
    }
    if (node.kind() == "assignment" || node.kind() == "augmented_assignment")
        && node.start_byte() > after_byte
        && let Some(left) = node.child_by_field_name("left")
        && let Ok(text) = left.utf8_text(source.as_bytes())
        && text == var_name
    {
        *found = true;
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_reassignment(child, source, var_name, after_byte, found);
    }
}

/// Collect all identifier usages of `var_name` in `scope`, excluding the declaration node.
fn collect_usages<'a>(scope: Node<'a>, source: &str, var_name: &str, decl: Node) -> Vec<Node<'a>> {
    let mut usages = Vec::new();
    collect_usages_recursive(scope, source, var_name, decl, &mut usages);
    usages
}

fn collect_usages_recursive<'a>(
    node: Node<'a>,
    source: &str,
    var_name: &str,
    decl: Node,
    usages: &mut Vec<Node<'a>>,
) {
    if node.kind() == "identifier"
        && node.utf8_text(source.as_bytes()).ok() == Some(var_name)
        && !is_within(node, decl)
    {
        usages.push(node);
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_usages_recursive(child, source, var_name, decl, usages);
    }
}

/// Check if `node` is within `container` by byte range.
fn is_within(node: Node, container: Node) -> bool {
    node.start_byte() >= container.start_byte() && node.end_byte() <= container.end_byte()
}

/// Determine if the initializer needs parentheses when inlined at a usage site.
fn needs_parens(init_kind: &str, usage_parent: Option<Node>) -> bool {
    let complex_init = matches!(
        init_kind,
        "binary_operator" | "ternary_expression" | "unary_operator"
    );
    if !complex_init {
        return false;
    }
    let Some(parent) = usage_parent else {
        return false;
    };
    matches!(
        parent.kind(),
        "binary_operator" | "subscript" | "attribute" | "unary_operator"
    )
}

/// Check if a `variable_statement` has an `@onready` annotation.
///
/// Annotations can appear in two forms in tree-sitter GDScript:
/// 1. Inline: `@onready var x` — an `annotations` child within the node
/// 2. Preceding sibling: `@onready\nvar x` — a separate `annotation` node before the declaration
fn has_onready_annotation(decl: Node, root: Node, source: &str) -> bool {
    // Check inline annotations (children of the declaration node)
    let mut cursor = decl.walk();
    for child in decl.children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut annot_cursor = child.walk();
            for annot in child.children(&mut annot_cursor) {
                if annot.kind() == "annotation" && annotation_name(&annot, source) == Some("onready")
                {
                    return true;
                }
            }
        }
    }

    // Check preceding siblings at the top level (e.g. `@onready\nvar x`)
    let parent = decl.parent().unwrap_or(root);
    let mut sibling_cursor = parent.walk();
    let children: Vec<_> = parent.children(&mut sibling_cursor).collect();
    if let Some(idx) = children.iter().position(|c| c.id() == decl.id()) {
        let mut i = idx;
        while i > 0 {
            i -= 1;
            let prev = &children[i];
            if prev.kind() == "annotation" {
                if annotation_name(prev, source) == Some("onready") {
                    return true;
                }
            } else if prev.kind() == "comment" {
                // Comments can appear between annotations and declarations
            } else {
                break;
            }
        }
    }

    false
}

/// Extract the annotation identifier name (e.g. "onready" from `@onready`).
fn annotation_name<'a>(node: &Node, source: &'a str) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source.as_bytes()).ok();
        }
    }
    None
}

/// Extract the type annotation text from a variable declaration.
///
/// Returns `Some("int")` for `var x: int = ...`, `None` for `var x = ...` or `var x := ...`.
fn extract_type_annotation_text<'a>(decl: Node, source: &'a str) -> Option<&'a str> {
    let type_node = decl.child_by_field_name("type")?;
    // Skip inferred types (`:=`) — they don't carry explicit type info to lose
    if type_node.kind() == "inferred_type" {
        return None;
    }
    type_node.utf8_text(source.as_bytes()).ok()
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
    fn inline_simple_local_variable() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar speed = 10\n\tprint(speed)\n",
        )]);
        let result = inline_variable(
            &temp.path().join("player.gd"),
            2,
            6, // column of 'speed' in 'var speed = 10'
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.variable, "speed");
        assert_eq!(result.expression, "10");
        assert_eq!(result.reference_count, 1);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(10)"),
            "should replace usage, got: {content}"
        );
        assert!(
            !content.contains("var speed"),
            "should remove declaration, got: {content}"
        );
    }

    #[test]
    fn inline_with_parentheses() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar offset = a + b\n\tprint(offset * c)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print((a + b) * c)"),
            "should wrap in parens, got: {content}"
        );
    }

    #[test]
    fn inline_no_parens_when_simple() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar x = get_value()\n\tprint(x * 2)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(get_value() * 2)"),
            "should not wrap call in parens, got: {content}"
        );
    }

    #[test]
    fn inline_multiple_usages() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar hp = 100\n\tprint(hp)\n\tprocess(hp)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.reference_count, 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(100)") && content.contains("process(100)"),
            "should replace all usages, got: {content}"
        );
        assert!(
            !content.contains("var hp"),
            "should remove declaration, got: {content}"
        );
    }

    #[test]
    fn inline_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar speed = 10\n\tprint(speed)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, true, temp.path()).unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var speed"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn rejects_const() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tconst MAX = 100\n\tprint(MAX)\n",
        )]);
        let result = inline_variable(&temp.path().join("player.gd"), 2, 8, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_reassigned() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar x = 1\n\tx = 2\n\tprint(x)\n",
        )]);
        let result = inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn rejects_no_initializer() {
        let temp = setup_project(&[("player.gd", "func _ready():\n\tvar x\n\tprint(x)\n")]);
        let result = inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn inline_top_level_variable() {
        let temp = setup_project(&[(
            "player.gd",
            "var speed = 10\n\nfunc _ready():\n\tprint(speed)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 1, 5, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(10)"),
            "should replace usage, got: {content}"
        );
        assert!(
            !content.contains("var speed"),
            "should remove declaration, got: {content}"
        );
    }

    #[test]
    fn no_usages_still_removes_declaration() {
        let temp = setup_project(&[("player.gd", "func _ready():\n\tvar unused = 42\n\tpass\n")]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.reference_count, 0);
        assert!(!result.warnings.is_empty());
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("var unused"),
            "should remove declaration, got: {content}"
        );
    }

    #[test]
    fn inline_from_usage_site() {
        // Point cursor at the usage, not the declaration
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar speed = 10\n\tprint(speed)\n",
        )]);
        let result = inline_variable(
            &temp.path().join("player.gd"),
            3,
            8, // column of 'speed' in 'print(speed)'
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.variable, "speed");
    }

    #[test]
    fn rejects_onready_variable() {
        let temp = setup_project(&[(
            "player.gd",
            "@onready var sprite = $Sprite2D\n\nfunc _ready():\n\tprint(sprite)\n",
        )]);
        let result = inline_variable(&temp.path().join("player.gd"), 1, 14, false, temp.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("@onready"),
            "error should mention @onready, got: {msg}"
        );
    }

    #[test]
    fn rejects_onready_with_type() {
        let temp = setup_project(&[(
            "player.gd",
            "@onready var sprite: Sprite2D = $Sprite2D\n\nfunc _ready():\n\tprint(sprite)\n",
        )]);
        let result = inline_variable(&temp.path().join("player.gd"), 1, 14, false, temp.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("@onready"),
            "error should mention @onready, got: {msg}"
        );
    }

    #[test]
    fn rejects_onready_multiline_annotation() {
        // @onready on a separate preceding line
        let temp = setup_project(&[(
            "player.gd",
            "@export\n@onready\nvar sprite = $Sprite2D\n\nfunc _ready():\n\tprint(sprite)\n",
        )]);
        let result = inline_variable(&temp.path().join("player.gd"), 3, 5, false, temp.path());
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("@onready"),
            "error should mention @onready, got: {msg}"
        );
    }

    #[test]
    fn warns_on_type_annotation_loss() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar x: int = get_value()\n\tprint(x)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        assert_eq!(result.reference_count, 1);
        assert!(
            result.warnings.iter().any(|w| w.contains("int")),
            "should warn about lost type annotation, got: {:?}",
            result.warnings
        );
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("print(get_value())"),
            "should still inline, got: {content}"
        );
    }

    #[test]
    fn no_type_warning_without_annotation() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar x = get_value()\n\tprint(x)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        assert!(
            !result.warnings.iter().any(|w| w.contains("type annotation")),
            "should not warn about type for untyped var, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn no_type_warning_for_inferred() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar x := 42\n\tprint(x)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        assert!(
            !result.warnings.iter().any(|w| w.contains("type annotation")),
            "should not warn about inferred type, got: {:?}",
            result.warnings
        );
    }

    #[test]
    fn warns_on_complex_type_annotation() {
        let temp = setup_project(&[(
            "player.gd",
            "func _ready():\n\tvar items: Array[String] = get_items()\n\tprint(items)\n",
        )]);
        let result =
            inline_variable(&temp.path().join("player.gd"), 2, 6, false, temp.path()).unwrap();
        assert!(result.applied);
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("Array[String]")),
            "should warn about lost type annotation, got: {:?}",
            result.warnings
        );
    }
}
