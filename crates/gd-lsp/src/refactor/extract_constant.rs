use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::line_starts;

// ── extract-constant ─────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct ExtractConstantOutput {
    pub constant: String,
    pub expression: String,
    pub file: String,
    pub applied: bool,
    pub replacements: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inferred_type: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

/// Expression node types we accept for extraction.
const EXPRESSION_KINDS: &[&str] = &[
    "binary_operator",
    "unary_operator",
    "call",
    "attribute",
    "subscript",
    "identifier",
    "string",
    "integer",
    "float",
    "true",
    "false",
    "null",
    "ternary_expression",
    "parenthesized_expression",
    "array",
    "dictionary",
];

/// Check that `name` is `UPPER_SNAKE_CASE` (e.g. `MAX_SPEED`, `PI`).
fn is_upper_snake_case(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && name.as_bytes()[0].is_ascii_uppercase()
}

#[allow(clippy::too_many_arguments, clippy::fn_params_excessive_bools)]
pub fn extract_constant(
    file: &Path,
    line: usize,       // 1-based
    column: usize,     // 1-based
    end_column: usize, // 1-based
    name: &str,
    replace_all: bool,
    dry_run: bool,
    project_root: &Path,
    class: Option<&str>,
) -> Result<ExtractConstantOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let root = tree.root_node();
    let gd_file = gd_core::gd_ast::convert(&tree, &source);

    let line_0 = line - 1;
    let col_0 = column - 1;
    let end_col_0 = end_column - 1;

    let start_point = tree_sitter::Point::new(line_0, col_0);
    let end_point = tree_sitter::Point::new(line_0, end_col_0);

    let expr = find_expression_at(root, start_point, end_point)
        .ok_or_else(|| miette::miette!("no expression found at {line}:{column}-{end_column}"))?;

    let expr_text = expr
        .utf8_text(source.as_bytes())
        .map_err(|e| miette::miette!("cannot read expression: {e}"))?
        .to_string();

    // ── Type inference ──────────────────────────────────────────────────
    let inferred_type = gd_core::type_inference::infer_expression_type(&expr, &source, &gd_file)
        .filter(|t| {
            !matches!(
                t,
                gd_core::type_inference::InferredType::Void
                    | gd_core::type_inference::InferredType::Variant
            )
        })
        .map(|t| t.display_name());

    // ── Determine scope (file top-level or inner class) ────────────────
    let (scope, indent) = if let Some(class_name) = class {
        let class_node = find_class_body(root, class_name, &source)
            .ok_or_else(|| miette::miette!("no inner class named '{class_name}' found"))?;
        (class_node, "\t".to_string())
    } else {
        (root, String::new())
    };

    // ── Collect replacements ───────────────────────────────────────────
    let mut replacements: Vec<(usize, usize)> = vec![(expr.start_byte(), expr.end_byte())];

    if replace_all {
        let expr_kind = expr.kind();
        let mut extra = Vec::new();
        collect_matching_expressions(scope, expr_kind, &expr_text, source.as_bytes(), &mut extra);

        for (s, e) in &extra {
            if *s == expr.start_byte() && *e == expr.end_byte() {
                continue;
            }
            if let Some(n) = root.descendant_for_byte_range(*s, *e)
                && is_assignment_target(n)
            {
                continue;
            }
            replacements.push((*s, *e));
        }
        replacements.sort_by_key(|(s, _)| *s);
        replacements.dedup();
    }

    let replacement_count = u32::try_from(replacements.len()).unwrap_or(u32::MAX);
    let relative_file = gd_core::fs::relative_slash(file, project_root);

    let mut warnings = Vec::new();
    if !is_upper_snake_case(name) {
        warnings.push(format!("constant name '{name}' is not UPPER_SNAKE_CASE"));
    }

    // Check for name collision at the target scope
    let scope_names = collect_top_level_names(scope, &source);
    if scope_names.contains(&name.to_string()) {
        warnings.push(format!("'{name}' already exists in this scope"));
    }

    if !dry_run {
        apply_extract(
            file,
            &source,
            name,
            &expr_text,
            inferred_type.as_deref(),
            replacements,
            scope,
            &indent,
            project_root,
        )?;
    }

    Ok(ExtractConstantOutput {
        constant: name.to_string(),
        expression: expr_text,
        file: relative_file,
        applied: !dry_run,
        replacements: replacement_count,
        inferred_type,
        warnings,
    })
}

/// Find the insertion point for a new constant at file/class scope.
/// Strategy: insert after the last existing const/var at top level, or before the
/// first function if no const/var exists. Falls back to after extends/class_name.
fn find_const_insertion_line(scope: Node) -> usize {
    let mut last_decl_end: Option<usize> = None;
    let mut first_func_line: Option<usize> = None;
    let mut after_header_line: usize = 0;

    let mut cursor = scope.walk();
    for child in scope.children(&mut cursor) {
        match child.kind() {
            "const_statement" | "variable_statement" => {
                last_decl_end = Some(child.end_position().row + 1);
            }
            "function_definition" | "constructor_definition" => {
                if first_func_line.is_none() {
                    first_func_line = Some(child.start_position().row);
                    // Include preceding annotations/doc comments
                    if let Some(prev) = child.prev_sibling()
                        && (prev.kind() == "decorator" || prev.kind() == "comment")
                    {
                        first_func_line = Some(prev.start_position().row);
                    }
                }
            }
            "extends_statement" | "class_name_statement" | "tool_statement" => {
                after_header_line = child.end_position().row + 1;
            }
            "comment" => {
                // Track comments that follow header declarations
                if last_decl_end.is_none() && first_func_line.is_none() {
                    after_header_line = child.end_position().row + 1;
                }
            }
            _ => {}
        }
    }

    // Prefer: after last const/var > before first func > after header
    if let Some(line) = last_decl_end {
        line
    } else if let Some(line) = first_func_line {
        // Add blank line before functions
        line
    } else {
        after_header_line
    }
}

/// Apply the replacements and insert the const declaration at file/class scope.
#[allow(clippy::too_many_arguments)]
fn apply_extract(
    file: &Path,
    source: &str,
    name: &str,
    expr_text: &str,
    inferred_type: Option<&str>,
    mut replacements: Vec<(usize, usize)>,
    scope: Node,
    indent: &str,
    project_root: &Path,
) -> Result<()> {
    let starts = line_starts(source);
    let mut new_source = source.to_string();

    // 1. Replace occurrences in reverse byte order to preserve offsets
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    for (s, e) in &replacements {
        new_source.replace_range(*s..*e, name);
    }

    // 2. Insert const declaration at file/class scope
    let insert_line = find_const_insertion_line(scope);
    let insert_byte = if insert_line < starts.len() {
        starts[insert_line]
    } else {
        new_source.len()
    };
    let type_suffix = inferred_type.map_or(String::new(), |t| format!(": {t}"));
    let const_line = format!("{indent}const {name}{type_suffix} = {expr_text}\n");
    new_source.insert_str(insert_byte, &const_line);

    super::validate_no_new_errors(source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "extract-constant",
        &format!("extract constant {name}"),
        &snaps,
        project_root,
    );
    Ok(())
}

/// Find the body node of an inner class by name.
fn find_class_body<'a>(root: Node<'a>, class_name: &str, source: &str) -> Option<Node<'a>> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "class_definition"
            && let Some(name_node) = child.child_by_field_name("name")
            && let Ok(text) = name_node.utf8_text(source.as_bytes())
            && text == class_name
        {
            return child.child_by_field_name("body");
        }
    }
    None
}

/// Collect all top-level declaration names in a scope.
fn collect_top_level_names(scope: Node, source: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = scope.walk();
    for child in scope.children(&mut cursor) {
        match child.kind() {
            "const_statement"
            | "variable_statement"
            | "function_definition"
            | "constructor_definition"
            | "class_definition"
            | "signal_statement"
            | "enum_definition" => {
                if let Some(name_node) = child.child_by_field_name("name")
                    && let Ok(text) = name_node.utf8_text(source.as_bytes())
                {
                    names.push(text.to_string());
                }
            }
            _ => {}
        }
    }
    names
}

/// Collect all expression nodes in `scope` that match the given kind and text.
/// Recurses into function bodies (unlike introduce-variable which stays local).
fn collect_matching_expressions(
    scope: Node,
    kind: &str,
    text: &str,
    source_bytes: &[u8],
    out: &mut Vec<(usize, usize)>,
) {
    let mut cursor = scope.walk();
    for child in scope.children(&mut cursor) {
        if child.kind() == kind
            && let Ok(node_text) = child.utf8_text(source_bytes)
            && node_text == text
        {
            out.push((child.start_byte(), child.end_byte()));
            continue;
        }
        collect_matching_expressions(child, kind, text, source_bytes, out);
    }
}

/// Check whether a node is an assignment target.
fn is_assignment_target(node: Node) -> bool {
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "assignment" | "augmented_assignment" => {
                if let Some(left) = parent.child_by_field_name("left") {
                    return left.byte_range() == node.byte_range();
                }
                if let Some(first) = parent.child(0) {
                    return first.byte_range() == node.byte_range();
                }
            }
            "variable_statement" | "const_statement" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    return name_node.byte_range() == node.byte_range();
                }
            }
            _ => {}
        }
    }
    false
}

/// Walk up from the deepest node at the given range to find the first expression node.
fn find_expression_at(
    root: Node<'_>,
    start: tree_sitter::Point,
    end: tree_sitter::Point,
) -> Option<Node<'_>> {
    let node = root.descendant_for_point_range(start, end)?;
    let mut current = node;
    loop {
        if EXPRESSION_KINDS.contains(&current.kind()) {
            return Some(current);
        }
        current = current.parent()?;
    }
}

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
    fn extract_literal_to_file_scope() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node\n\nfunc calc():\n\tvar x = speed * 0.15\n",
        )]);
        let result = extract_constant(
            &temp.path().join("player.gd"),
            4,
            18,
            22, // "0.15" on line 4
            "RATIO",
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.constant, "RATIO");
        assert_eq!(result.expression, "0.15");
        assert_eq!(result.replacements, 1);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("const RATIO: float = 0.15"),
            "should insert const at top level, got:\n{content}"
        );
        assert!(
            content.contains("speed * RATIO"),
            "should replace expression, got:\n{content}"
        );
        // Const should be before the function, not inside it
        let const_line = content
            .lines()
            .position(|l| l.contains("const RATIO"))
            .unwrap();
        let func_line = content
            .lines()
            .position(|l| l.contains("func calc"))
            .unwrap();
        assert!(
            const_line < func_line,
            "const should be before func, const at {const_line}, func at {func_line}"
        );
    }

    #[test]
    fn extract_replace_all_across_functions() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node\n\nfunc foo():\n\tvar x = 42\n\nfunc bar():\n\tvar y = 42\n",
        )]);
        let result = extract_constant(
            &temp.path().join("player.gd"),
            4,
            10,
            12, // "42" on line 4
            "ANSWER",
            true,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.replacements, 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("const ANSWER: int = 42"),
            "should insert const, got:\n{content}"
        );
        assert!(
            !content.contains("= 42"),
            "should replace all occurrences, got:\n{content}"
        );
    }

    #[test]
    fn extract_naming_warning() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node\n\nfunc calc():\n\tvar x = 0.15\n",
        )]);
        let result = extract_constant(
            &temp.path().join("player.gd"),
            4,
            10,
            14,
            "ratio", // not UPPER_SNAKE_CASE
            false,
            true,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(
            result
                .warnings
                .iter()
                .any(|w| w.contains("UPPER_SNAKE_CASE")),
            "should warn about naming: {:?}",
            result.warnings
        );
    }

    #[test]
    fn extract_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node\n\nfunc calc():\n\tvar x = 0.15\n",
        )]);
        let result = extract_constant(
            &temp.path().join("player.gd"),
            4,
            10,
            14,
            "RATIO",
            false,
            true,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("const RATIO"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn extract_inserts_after_existing_consts() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node\n\nconst SPEED = 200\n\nfunc calc():\n\tvar x = 0.15\n",
        )]);
        let result = extract_constant(
            &temp.path().join("player.gd"),
            6,
            10,
            14,
            "RATIO",
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        let speed_line = content
            .lines()
            .position(|l| l.contains("const SPEED"))
            .unwrap();
        let ratio_line = content
            .lines()
            .position(|l| l.contains("const RATIO"))
            .unwrap();
        let func_line = content
            .lines()
            .position(|l| l.contains("func calc"))
            .unwrap();
        assert!(
            ratio_line > speed_line && ratio_line < func_line,
            "RATIO should be after SPEED and before func, got speed={speed_line} ratio={ratio_line} func={func_line}"
        );
    }

    #[test]
    fn extract_collision_warning() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node\n\nconst SPEED = 200\n\nfunc calc():\n\tvar x = 0.15\n",
        )]);
        let result = extract_constant(
            &temp.path().join("player.gd"),
            6,
            10,
            14,
            "SPEED", // already exists
            false,
            true,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(
            result.warnings.iter().any(|w| w.contains("already exists")),
            "should warn about collision: {:?}",
            result.warnings
        );
    }

    #[test]
    fn extract_string_literal() {
        let temp = setup_project(&[(
            "player.gd",
            "extends Node\n\nfunc greet():\n\tprint(\"hello world\")\n",
        )]);
        let result = extract_constant(
            &temp.path().join("player.gd"),
            4,
            8,
            21, // "hello world" including quotes
            "GREETING",
            false,
            false,
            temp.path(),
            None,
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("const GREETING: String = \"hello world\""),
            "should extract string const, got:\n{content}"
        );
    }
}
