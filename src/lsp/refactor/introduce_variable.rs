use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

use super::collision::{check_collision, collect_scope_names};
use super::extract_method::get_indent;
use super::line_starts;

// ── introduce-variable ─────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct IntroduceVariableOutput {
    pub variable: String,
    pub expression: String,
    pub is_const: bool,
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
    "lambda",
    "await_expression",
];

/// Check that `name` is `UPPER_SNAKE_CASE` (e.g. `MAX_SPEED`, `PI`).
fn is_upper_snake_case(name: &str) -> bool {
    !name.is_empty()
        && name
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        && name.as_bytes()[0].is_ascii_uppercase()
}

#[allow(clippy::too_many_arguments)]
pub fn introduce_variable(
    file: &Path,
    line: usize,       // 1-based
    column: usize,     // 1-based
    end_column: usize, // 1-based
    name: &str,
    as_const: bool,
    replace_all: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<IntroduceVariableOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();
    let gd_file = crate::core::gd_ast::convert(&tree, &source);

    let line_0 = line - 1;
    let col_0 = column - 1;
    let end_col_0 = end_column - 1;

    // Find expression node at the selection
    let start_point = tree_sitter::Point::new(line_0, col_0);
    let end_point = tree_sitter::Point::new(line_0, end_col_0);

    let expr = find_expression_at(root, start_point, end_point)
        .ok_or_else(|| miette::miette!("no expression found at {line}:{column}-{end_column}"))?;

    let expr_text = expr
        .utf8_text(source.as_bytes())
        .map_err(|e| miette::miette!("cannot read expression: {e}"))?
        .to_string();
    let expr_kind = expr.kind().to_string();

    // ── Type inference ──────────────────────────────────────────────────
    let inferred_type =
        crate::core::type_inference::infer_expression_type(&expr, &source, &gd_file)
            .filter(|t| {
                !matches!(
                    t,
                    crate::core::type_inference::InferredType::Void
                        | crate::core::type_inference::InferredType::Variant
                )
            })
            .map(|t| t.display_name());

    // ── Find scope and matching occurrences ─────────────────────────────
    let containing_stmt = find_containing_statement(expr)
        .ok_or_else(|| miette::miette!("cannot find containing statement for the expression"))?;

    let (replacements, earliest_stmt_line) = collect_replacements(
        replace_all,
        root,
        start_point,
        &expr,
        &expr_kind,
        &expr_text,
        &source,
        containing_stmt.start_position().row,
    );
    let replacement_count = u32::try_from(replacements.len()).unwrap_or(u32::MAX);

    let indent = get_indent(&source, earliest_stmt_line);
    let relative_file = crate::core::fs::relative_slash(file, project_root);

    let mut warnings = Vec::new();
    let scope_names = collect_scope_names(root, &source, start_point, &gd_file);
    if let Some(kind) = check_collision(name, &scope_names) {
        warnings.push(format!("'{name}' collides with a {kind}"));
    }
    if as_const && !is_upper_snake_case(name) {
        warnings.push(format!("constant name '{name}' is not UPPER_SNAKE_CASE"));
    }

    if !dry_run {
        apply_introduce(
            file,
            &source,
            name,
            &expr_text,
            as_const,
            inferred_type.as_deref(),
            replacements,
            earliest_stmt_line,
            &indent,
            project_root,
        )?;
    }

    Ok(IntroduceVariableOutput {
        variable: name.to_string(),
        expression: expr_text,
        is_const: as_const,
        file: relative_file,
        applied: !dry_run,
        replacements: replacement_count,
        inferred_type,
        warnings,
    })
}

/// Gather all byte ranges to replace. Returns `(replacements, earliest_stmt_line)`.
#[allow(clippy::too_many_arguments)]
fn collect_replacements(
    replace_all: bool,
    root: Node,
    start_point: tree_sitter::Point,
    expr: &Node,
    expr_kind: &str,
    expr_text: &str,
    source: &str,
    initial_stmt_line: usize,
) -> (Vec<(usize, usize)>, usize) {
    let mut replacements: Vec<(usize, usize)> = vec![(expr.start_byte(), expr.end_byte())];
    let mut earliest_stmt_line = initial_stmt_line;

    if replace_all {
        let scope = crate::lsp::references::enclosing_function(root, start_point)
            .and_then(|f| f.child_by_field_name("body"))
            .unwrap_or(root);

        let mut extra = Vec::new();
        collect_matching_expressions(scope, expr_kind, expr_text, source.as_bytes(), &mut extra);

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

            if let Some(n) = root.descendant_for_byte_range(*s, *e)
                && let Some(stmt) = find_containing_statement(n)
                && stmt.start_position().row < earliest_stmt_line
            {
                earliest_stmt_line = stmt.start_position().row;
            }
        }

        replacements.sort_by_key(|(s, _)| *s);
        replacements.dedup();
    }

    (replacements, earliest_stmt_line)
}

/// Apply the replacements and insert the declaration.
#[allow(clippy::too_many_arguments)]
fn apply_introduce(
    file: &Path,
    source: &str,
    name: &str,
    expr_text: &str,
    as_const: bool,
    inferred_type: Option<&str>,
    mut replacements: Vec<(usize, usize)>,
    earliest_stmt_line: usize,
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

    // 2. Insert variable declaration before the earliest containing statement
    let insert_byte = starts[earliest_stmt_line];
    let keyword = if as_const { "const" } else { "var" };
    let type_suffix = inferred_type.map_or(String::new(), |t| format!(": {t}"));
    let var_line = format!("{indent}{keyword} {name}{type_suffix} = {expr_text}\n");
    new_source.insert_str(insert_byte, &var_line);

    super::validate_no_new_errors(source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let label = if as_const {
        "introduce-constant"
    } else {
        "introduce-variable"
    };
    let _ = stack.record(label, &format!("introduce {name}"), &snaps, project_root);
    Ok(())
}

/// Collect all expression nodes in `scope` that match the given kind and text.
/// Does NOT recurse into nested function definitions.
fn collect_matching_expressions(
    scope: Node,
    kind: &str,
    text: &str,
    source_bytes: &[u8],
    out: &mut Vec<(usize, usize)>,
) {
    let mut cursor = scope.walk();
    for child in scope.children(&mut cursor) {
        // Don't recurse into nested function definitions (different scope)
        if child.kind() == "function_definition" || child.kind() == "constructor_definition" {
            continue;
        }
        if child.kind() == kind
            && let Ok(node_text) = child.utf8_text(source_bytes)
            && node_text == text
        {
            out.push((child.start_byte(), child.end_byte()));
            // Don't recurse into matching nodes — the whole subtree is the match
            continue;
        }
        // Recurse into children
        collect_matching_expressions(child, kind, text, source_bytes, out);
    }
}

/// Check whether a node is an assignment target (left side of `=`, `+=`, etc.,
/// or a var/const declaration name).
fn is_assignment_target(node: Node) -> bool {
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "assignment" | "augmented_assignment" => {
                // Left child of assignment is at field "left" or position 0
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

/// Walk up from an expression node to find the nearest statement-level ancestor.
fn find_containing_statement(node: Node) -> Option<Node> {
    let stmt_kinds = [
        "expression_statement",
        "variable_statement",
        "assignment",
        "augmented_assignment",
        "return_statement",
        "if_statement",
        "for_statement",
        "while_statement",
        "match_statement",
    ];
    let mut current = node;
    loop {
        if stmt_kinds.contains(&current.kind()) {
            return Some(current);
        }
        // If we hit a function body, the expression is the statement itself
        if current.kind() == "body" || current.kind() == "source" {
            return None;
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
    fn introduce_simple_expression() {
        let temp = setup_project(&[(
            "player.gd",
            "func process(delta):\n\tposition.x += speed * delta\n",
        )]);
        // Select "speed * delta" on line 2
        // \tposition.x += speed * delta
        //                 ^            ^ col 17 to 29 (1-based)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            17,
            29,
            "velocity",
            false,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.variable, "velocity");
        assert_eq!(result.expression, "speed * delta");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var velocity = speed * delta"),
            "should insert variable, got: {content}"
        );
        assert!(
            content.contains("position.x += velocity"),
            "should replace expression, got: {content}"
        );
    }

    #[test]
    fn introduce_dry_run() {
        let temp = setup_project(&[(
            "player.gd",
            "func process(delta):\n\tposition.x += speed * delta\n",
        )]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            17,
            29,
            "velocity",
            false,
            false,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            !content.contains("var velocity"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn introduce_as_const_literal() {
        let temp = setup_project(&[("player.gd", "func calc():\n\tvar x = speed * 0.15\n")]);
        // Select "0.15" on line 2, col 18-22 (1-based)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "RATIO",
            true,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(result.is_const);
        assert_eq!(result.variable, "RATIO");
        assert_eq!(result.expression, "0.15");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("const RATIO: float = 0.15"),
            "should insert typed const, got: {content}"
        );
        assert!(
            content.contains("speed * RATIO"),
            "should replace expression, got: {content}"
        );
    }

    #[test]
    fn introduce_as_const_naming_warning() {
        let temp = setup_project(&[("player.gd", "func calc():\n\tvar x = speed * 0.15\n")]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "ratio",
            true,
            false,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.is_const);
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
    fn introduce_as_const_false_no_const() {
        let temp = setup_project(&[("player.gd", "func calc():\n\tvar x = speed * 0.15\n")]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "ratio",
            false,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(!result.is_const);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var ratio: float = 0.15"),
            "should insert typed var not const, got: {content}"
        );
    }

    #[test]
    fn introduce_as_const_output_fields() {
        let temp = setup_project(&[("player.gd", "func calc():\n\tvar x = speed * 0.15\n")]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "RATIO",
            true,
            false,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.is_const);
        assert!(!result.applied);
        assert_eq!(result.variable, "RATIO");
        assert_eq!(result.expression, "0.15");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn introduce_call_expression() {
        let temp = setup_project(&[("player.gd", "func _ready():\n\tprint(get_health())\n")]);
        // Select "get_health()" on line 2, col 8-20
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            8,
            20,
            "hp",
            false,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var hp = get_health()"),
            "should extract call, got: {content}"
        );
        assert!(
            content.contains("print(hp)"),
            "should replace with var, got: {content}"
        );
    }

    // ── Type inference tests ──────────────────────────────────────────────

    #[test]
    fn introduce_infers_int_from_arithmetic() {
        // 1 + 2 → int via binary operator inference
        let temp = setup_project(&[("player.gd", "func calc():\n\tprint(1 + 2)\n")]);
        // Select "1 + 2" on line 2
        // \tprint(1 + 2)
        //        ^    ^ col 8 to 13 (1-based)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            8,
            13,
            "sum",
            false,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.inferred_type.as_deref(), Some("int"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var sum: int = 1 + 2"),
            "should have typed declaration, got: {content}"
        );
    }

    #[test]
    fn introduce_infers_vector2_from_constructor() {
        let temp = setup_project(&[("player.gd", "func calc():\n\tprint(Vector2(1, 2))\n")]);
        // Select "Vector2(1, 2)" on line 2
        // \tprint(Vector2(1, 2))
        //        ^             ^ col 8 to 21 (1-based)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            8,
            21,
            "pos",
            false,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.inferred_type.as_deref(), Some("Vector2"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var pos: Vector2 = Vector2(1, 2)"),
            "should have typed declaration, got: {content}"
        );
    }

    #[test]
    fn introduce_unknown_expression_stays_untyped() {
        let temp = setup_project(&[("player.gd", "func calc():\n\tprint(some_unknown_func())\n")]);
        // Select "some_unknown_func()" on line 2
        // \tprint(some_unknown_func())
        //        ^                   ^ col 8 to 27 (1-based)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            8,
            27,
            "val",
            false,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        // Unknown function — no inferred type (or Variant which we skip)
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var val = some_unknown_func()"),
            "should have untyped declaration, got: {content}"
        );
    }

    #[test]
    fn introduce_const_with_type() {
        let temp = setup_project(&[("player.gd", "func calc():\n\tprint(Vector2(1, 2))\n")]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            8,
            21,
            "POS",
            true,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert!(result.is_const);
        assert_eq!(result.inferred_type.as_deref(), Some("Vector2"));
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("const POS: Vector2 = Vector2(1, 2)"),
            "should have typed const, got: {content}"
        );
    }

    // ── Replace-all tests ────────────────────────────────────────────────

    #[test]
    fn replace_all_replaces_all_occurrences() {
        let temp = setup_project(&[(
            "player.gd",
            "func calc(a, b):\n\tvar x = a + b\n\tvar y = a + b\n\tprint(a + b)\n",
        )]);
        // Select "a + b" on line 2, col 10-15 (1-based)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            10,
            15,
            "sum",
            false,
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.replacements, 3);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var sum = a + b"),
            "should have declaration, got: {content}"
        );
        // All three occurrences should be replaced
        assert_eq!(
            content.matches("sum").count(),
            4, // 1 declaration + 3 replacements
            "should have 4 occurrences of 'sum', got: {content}"
        );
        // Original expression should not appear
        assert_eq!(
            content.matches("a + b").count(),
            1, // Only in the declaration
            "expression should only appear in declaration, got: {content}"
        );
    }

    #[test]
    fn replace_all_false_only_replaces_selected() {
        let temp = setup_project(&[(
            "player.gd",
            "func calc(a, b):\n\tvar x = a + b\n\tvar y = a + b\n\tprint(a + b)\n",
        )]);
        // Same selection but replace_all=false
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            10,
            15,
            "sum",
            false,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.replacements, 1);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        // Only the selected occurrence replaced, others remain
        assert_eq!(
            content.matches("a + b").count(),
            3, // 1 in declaration + 2 untouched occurrences
            "only selected should be replaced, got: {content}"
        );
    }

    #[test]
    fn replace_all_declaration_before_earliest() {
        let temp = setup_project(&[(
            "player.gd",
            "func calc(a, b):\n\tprint(a + b)\n\tvar x = a + b\n\tprint(a + b)\n",
        )]);
        // Select "a + b" on line 3 (the second occurrence)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            3,
            10,
            15,
            "sum",
            false,
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.replacements, 3);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        // Declaration should be before the first occurrence (line 2)
        let decl_pos = content.find("var sum").expect("should have declaration");
        let first_use = content.find("print(sum)").expect("should have first usage");
        assert!(
            decl_pos < first_use,
            "declaration should be before first use, got: {content}"
        );
    }

    #[test]
    fn replace_all_respects_function_scope() {
        let temp = setup_project(&[(
            "player.gd",
            "func calc(a, b):\n\tprint(a + b)\n\nfunc other(a, b):\n\tprint(a + b)\n",
        )]);
        // Select "a + b" on line 2, inside calc()
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            8,
            13,
            "sum",
            false,
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.replacements, 1);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        // The "a + b" in other() should NOT be replaced
        assert!(
            content.contains("func other(a, b):\n\tprint(a + b)"),
            "other function should be untouched, got: {content}"
        );
    }
}
