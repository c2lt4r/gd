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
    dry_run: bool,
    project_root: &Path,
) -> Result<IntroduceVariableOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

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

    // Find the containing statement to insert before
    let containing_stmt = find_containing_statement(expr)
        .ok_or_else(|| miette::miette!("cannot find containing statement for the expression"))?;

    let stmt_line = containing_stmt.start_position().row;
    let indent = get_indent(&source, stmt_line);

    let relative_file = crate::core::fs::relative_slash(file, project_root);

    let mut warnings = Vec::new();
    let scope_names = collect_scope_names(root, &source, start_point);
    if let Some(kind) = check_collision(name, &scope_names) {
        warnings.push(format!("'{name}' collides with a {kind}"));
    }
    if as_const && !is_upper_snake_case(name) {
        warnings.push(format!("constant name '{name}' is not UPPER_SNAKE_CASE"));
    }

    if !dry_run {
        let starts = line_starts(&source);
        let mut new_source = source.clone();

        // 1. Replace expression with variable name
        let expr_start = expr.start_byte();
        let expr_end = expr.end_byte();
        new_source.replace_range(expr_start..expr_end, name);

        // 2. Insert variable declaration before the containing statement
        // After the replacement, byte offsets above expr_start have shifted.
        // The statement starts at starts[stmt_line], which is before expr_start,
        // so it's still valid.
        let insert_byte = starts[stmt_line];
        let keyword = if as_const { "const" } else { "var" };
        let var_line = format!("{indent}{keyword} {name} = {expr_text}\n");
        new_source.insert_str(insert_byte, &var_line);

        super::validate_no_new_errors(&source, &new_source)?;
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let label = if as_const {
            "introduce-constant"
        } else {
            "introduce-variable"
        };
        let _ = stack.record(
            label,
            &format!("introduce {name}"),
            &snaps,
            project_root,
        );
    }

    Ok(IntroduceVariableOutput {
        variable: name.to_string(),
        expression: expr_text,
        is_const: as_const,
        file: relative_file,
        applied: !dry_run,
        warnings,
    })
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
        let temp = setup_project(&[(
            "player.gd",
            "func calc():\n\tvar x = speed * 0.15\n",
        )]);
        // Select "0.15" on line 2, col 18-22 (1-based)
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "RATIO",
            true,
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
            content.contains("const RATIO = 0.15"),
            "should insert const, got: {content}"
        );
        assert!(
            content.contains("speed * RATIO"),
            "should replace expression, got: {content}"
        );
    }

    #[test]
    fn introduce_as_const_naming_warning() {
        let temp = setup_project(&[(
            "player.gd",
            "func calc():\n\tvar x = speed * 0.15\n",
        )]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "ratio",
            true,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(result.is_const);
        assert!(
            result.warnings.iter().any(|w| w.contains("UPPER_SNAKE_CASE")),
            "should warn about naming: {:?}",
            result.warnings
        );
    }

    #[test]
    fn introduce_as_const_false_no_const() {
        let temp = setup_project(&[(
            "player.gd",
            "func calc():\n\tvar x = speed * 0.15\n",
        )]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "ratio",
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(!result.is_const);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(
            content.contains("var ratio = 0.15"),
            "should insert var not const, got: {content}"
        );
    }

    #[test]
    fn introduce_as_const_output_fields() {
        let temp = setup_project(&[(
            "player.gd",
            "func calc():\n\tvar x = speed * 0.15\n",
        )]);
        let result = introduce_variable(
            &temp.path().join("player.gd"),
            2,
            18,
            22,
            "RATIO",
            true,
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
}
