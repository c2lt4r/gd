use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tree_sitter::Node;

// ── invert-if ────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug)]
pub struct InvertIfOutput {
    pub file: String,
    pub line: u32,
    pub original_condition: String,
    pub inverted_condition: String,
    pub applied: bool,
}

/// Invert the if/else at the given line: negate the condition and swap the
/// if-body with the else-body.  For elif chains, rotate: the first elif
/// becomes the new `if`, and the original `if` becomes the last `elif`.
pub fn invert_if(
    file: &Path,
    line: usize, // 1-based
    dry_run: bool,
    project_root: &Path,
) -> Result<InvertIfOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = crate::core::parser::parse(&source)?;
    let root = tree.root_node();

    let line_0 = line - 1;

    let if_node = find_if_at_line(root, line_0)
        .ok_or_else(|| miette::miette!("no if statement found at line {line}"))?;

    let mut cursor = if_node.walk();
    let children: Vec<Node> = if_node.children(&mut cursor).collect();

    // Condition = first named child that isn't body/elif_clause/else_clause/comment
    let condition_node = children
        .iter()
        .find(|c| {
            c.is_named() && !matches!(c.kind(), "body" | "elif_clause" | "else_clause" | "comment")
        })
        .ok_or_else(|| miette::miette!("cannot find condition in if statement"))?;

    let condition_text = node_text(condition_node, &source);
    let if_body = children
        .iter()
        .find(|c| c.kind() == "body")
        .ok_or_else(|| miette::miette!("cannot find if body"))?;

    let elif_clauses: Vec<&Node> = children
        .iter()
        .filter(|c| c.kind() == "elif_clause")
        .collect();
    let else_clause = children.iter().find(|c| c.kind() == "else_clause");

    if elif_clauses.is_empty() && else_clause.is_none() {
        return Err(miette::miette!(
            "cannot invert: if statement has no else/elif branch"
        ));
    }

    let inverted_condition = negate_condition(condition_node, &source);
    let relative_file = crate::core::fs::relative_slash(file, project_root);

    if dry_run {
        return Ok(InvertIfOutput {
            file: relative_file,
            line: line as u32,
            original_condition: condition_text,
            inverted_condition,
            applied: false,
        });
    }

    let new_source = if elif_clauses.is_empty() {
        build_simple_inversion(
            &source,
            if_node,
            condition_node,
            if_body,
            else_clause.unwrap(),
        )?
    } else {
        build_elif_inversion(
            &source,
            if_node,
            condition_node,
            if_body,
            &elif_clauses,
            else_clause,
        )?
    };

    super::validate_no_new_errors(&source, &new_source)?;
    std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
    snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
    let stack = super::undo::UndoStack::open(project_root);
    let _ = stack.record(
        "invert-if",
        &format!("invert if at line {line}"),
        &snaps,
        project_root,
    );

    Ok(InvertIfOutput {
        file: relative_file,
        line: line as u32,
        original_condition: condition_text,
        inverted_condition,
        applied: true,
    })
}

// ── Inversion builders ──────────────────────────────────────────────────────

/// Simple if/else swap: negate condition, swap bodies.
fn build_simple_inversion(
    source: &str,
    if_node: Node,
    condition_node: &Node,
    if_body: &Node,
    else_clause: &Node,
) -> Result<String> {
    let inverted = negate_condition(condition_node, source);
    let if_body_text = body_lines(if_body, source);
    let else_body_text = get_clause_body_lines(else_clause, source)
        .ok_or_else(|| miette::miette!("cannot find else body"))?;

    let indent = get_line_indent(source, if_node.start_position().row);

    let mut rebuilt = String::new();
    rebuilt.push_str(&indent);
    rebuilt.push_str("if ");
    rebuilt.push_str(&inverted);
    rebuilt.push_str(":\n");
    rebuilt.push_str(&else_body_text);
    rebuilt.push('\n');
    rebuilt.push_str(&indent);
    rebuilt.push_str("else:\n");
    rebuilt.push_str(&if_body_text);

    Ok(splice(source, if_node, &rebuilt))
}

/// Elif rotation: first elif → new if, original if → last elif.
fn build_elif_inversion(
    source: &str,
    if_node: Node,
    condition_node: &Node,
    if_body: &Node,
    elif_clauses: &[&Node],
    else_clause: Option<&Node>,
) -> Result<String> {
    let indent = get_line_indent(source, if_node.start_position().row);
    let original_condition = node_text(condition_node, source);
    let if_body_text = body_lines(if_body, source);

    let first_elif = elif_clauses[0];
    let first_elif_cond = get_elif_condition(first_elif, source)
        .ok_or_else(|| miette::miette!("cannot find elif condition"))?;
    let first_elif_body = get_clause_body_lines(first_elif, source)
        .ok_or_else(|| miette::miette!("cannot find elif body"))?;

    let mut rebuilt = String::new();

    // First elif becomes the new if
    rebuilt.push_str(&indent);
    rebuilt.push_str("if ");
    rebuilt.push_str(&first_elif_cond);
    rebuilt.push_str(":\n");
    rebuilt.push_str(&first_elif_body);

    // Remaining elifs stay
    for elif in &elif_clauses[1..] {
        let cond = get_elif_condition(elif, source)
            .ok_or_else(|| miette::miette!("cannot find elif condition"))?;
        let body_text = get_clause_body_lines(elif, source)
            .ok_or_else(|| miette::miette!("cannot find elif body"))?;
        rebuilt.push('\n');
        rebuilt.push_str(&indent);
        rebuilt.push_str("elif ");
        rebuilt.push_str(&cond);
        rebuilt.push_str(":\n");
        rebuilt.push_str(&body_text);
    }

    // Original if becomes a new elif
    rebuilt.push('\n');
    rebuilt.push_str(&indent);
    rebuilt.push_str("elif ");
    rebuilt.push_str(&original_condition);
    rebuilt.push_str(":\n");
    rebuilt.push_str(&if_body_text);

    // else clause stays
    if let Some(else_node) = else_clause {
        let body_text = get_clause_body_lines(else_node, source)
            .ok_or_else(|| miette::miette!("cannot find else body"))?;
        rebuilt.push('\n');
        rebuilt.push_str(&indent);
        rebuilt.push_str("else:\n");
        rebuilt.push_str(&body_text);
    }

    Ok(splice(source, if_node, &rebuilt))
}

// ── Condition negation ──────────────────────────────────────────────────────

/// Negate a GDScript condition, applying De Morgan's law where appropriate.
/// Uses AST node kind for dispatch, text-based helpers for the transform.
pub(super) fn negate_condition(node: &Node, source: &str) -> String {
    match node.kind() {
        "unary_operator" => {
            let ct = node_text(node, source);
            if let Some(rest) = ct.strip_prefix("not ") {
                return rest.to_string();
            }
            if let Some(rest) = ct.strip_prefix('!') {
                return rest.to_string();
            }
            format!("not {ct}")
        }
        "binary_operator" => {
            let ct = node_text(node, source);
            if let Some(r) = apply_de_morgan(&ct) {
                return r;
            }
            if let Some(r) = flip_comparison(&ct) {
                return r;
            }
            format!("not ({ct})")
        }
        "parenthesized_expression" => {
            if node.named_child_count() == 1
                && let Some(inner) = node.named_child(0)
            {
                return negate_condition(&inner, source);
            }
            let ct = node_text(node, source);
            format!("not {ct}")
        }
        "true" => "false".to_string(),
        "false" => "true".to_string(),
        _ => {
            let ct = node_text(node, source);
            format!("not {ct}")
        }
    }
}

/// Apply De Morgan's law: `a and b` → `not a or not b`, `a or b` → `not a and not b`.
/// Only splits at the top-level operator (not inside parentheses).
pub(super) fn apply_de_morgan(text: &str) -> Option<String> {
    let (pos, op_kind) = find_top_level_bool_op(text)?;
    let lhs = text[..pos].trim();
    let rhs = text[pos + op_kind.len() + 2..].trim();
    let new_op = if op_kind == "and" { "or" } else { "and" };
    Some(format!(
        "{} {new_op} {}",
        negate_simple(lhs),
        negate_simple(rhs)
    ))
}

/// Find top-level `and` / `or` (not inside parentheses).
fn find_top_level_bool_op(text: &str) -> Option<(usize, &'static str)> {
    let mut depth = 0i32;
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => depth -= 1,
            _ => {}
        }
        if depth == 0 {
            if text[i..].starts_with(" and ") {
                return Some((i, "and"));
            }
            if text[i..].starts_with(" or ") {
                return Some((i, "or"));
            }
        }
    }
    None
}

/// Negate a simple sub-expression (text-based, for De Morgan operands).
pub(super) fn negate_simple(text: &str) -> String {
    if let Some(rest) = text.strip_prefix("not ") {
        return rest.to_string();
    }
    if text == "true" {
        return "false".to_string();
    }
    if text == "false" {
        return "true".to_string();
    }
    if let Some(flipped) = flip_comparison(text) {
        return flipped;
    }
    format!("not {text}")
}

/// Flip comparison: `x > 0` → `x <= 0`, `x == y` → `x != y`, etc.
pub(super) fn flip_comparison(text: &str) -> Option<String> {
    // Check longer operators first to avoid partial matches
    let ops: &[(&str, &str)] = &[
        ("!=", "=="),
        ("<=", ">"),
        (">=", "<"),
        ("==", "!="),
        ("<", ">="),
        (">", "<="),
    ];
    for (op, flipped) in ops {
        let pattern = format!(" {op} ");
        if let Some(pos) = text.find(&pattern) {
            let lhs = &text[..pos];
            let rhs = &text[pos + pattern.len()..];
            return Some(format!("{lhs} {flipped} {rhs}"));
        }
    }
    None
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Find an `if_statement` at the given line (0-based), searching recursively.
fn find_if_at_line(root: Node, line: usize) -> Option<Node> {
    find_if_recursive(root, line)
}

fn find_if_recursive(node: Node, line: usize) -> Option<Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "if_statement" && child.start_position().row == line {
            return Some(child);
        }
        if let Some(found) = find_if_recursive(child, line) {
            return Some(found);
        }
    }
    None
}

pub(super) fn node_text(node: &Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

pub(super) fn get_line_indent(source: &str, line: usize) -> String {
    if let Some(line_str) = source.lines().nth(line) {
        let trimmed = line_str.trim_start();
        line_str[..line_str.len() - trimmed.len()].to_string()
    } else {
        String::new()
    }
}

pub(super) fn line_start_offset(source: &str, line: usize) -> usize {
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

/// Replace an AST node's range in the source with new text.
pub(super) fn splice(source: &str, node: Node, replacement: &str) -> String {
    let start = line_start_offset(source, node.start_position().row);
    let end = node.end_byte();
    let mut out = String::with_capacity(source.len());
    out.push_str(&source[..start]);
    out.push_str(replacement);
    out.push_str(&source[end..]);
    out
}

/// Get body text from a body node, stripping the leading newline that
/// tree-sitter includes (the newline between `:` and the body content).
pub(super) fn body_lines(body_node: &Node, source: &str) -> String {
    let raw = node_text(body_node, source);
    raw.strip_prefix('\n').unwrap_or(&raw).to_string()
}

/// Get the body lines from an elif_clause or else_clause.
fn get_clause_body_lines(clause: &Node, source: &str) -> Option<String> {
    let mut cursor = clause.walk();
    for child in clause.children(&mut cursor) {
        if child.kind() == "body" {
            return Some(body_lines(&child, source));
        }
    }
    None
}

fn get_elif_condition(elif: &Node, source: &str) -> Option<String> {
    let mut cursor = elif.walk();
    for child in elif.children(&mut cursor) {
        if child.is_named()
            && !matches!(
                child.kind(),
                "body" | "elif_clause" | "else_clause" | "comment"
            )
        {
            return Some(node_text(&child, source));
        }
    }
    None
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
    fn invert_simple_if_else() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tif not is_alive:\n\t\treturn\n\telse:\n\t\ttake_damage(10)\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if is_alive:"),
            "should negate `not is_alive` to `is_alive`, got:\n{content}"
        );
    }

    #[test]
    fn invert_comparison() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo(x):\n\tif x > 0:\n\t\tprint(\"positive\")\n\telse:\n\t\tprint(\"non-positive\")\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if x <= 0:"),
            "should flip > to <=, got:\n{content}"
        );
        let lines: Vec<&str> = content.lines().collect();
        let if_idx = lines.iter().position(|l| l.contains("if x <= 0:")).unwrap();
        assert!(
            lines[if_idx + 1].contains("non-positive"),
            "if body should now have old else content, got:\n{content}"
        );
    }

    #[test]
    fn invert_de_morgan_and() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo(a, b):\n\tif a and b:\n\t\tprint(\"both\")\n\telse:\n\t\tprint(\"not both\")\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if not a or not b:"),
            "should apply De Morgan's law, got:\n{content}"
        );
    }

    #[test]
    fn invert_de_morgan_or() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo(a, b):\n\tif a or b:\n\t\tprint(\"either\")\n\telse:\n\t\tprint(\"neither\")\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if not a and not b:"),
            "should apply De Morgan for or, got:\n{content}"
        );
    }

    #[test]
    fn invert_dry_run() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tif true:\n\t\tpass\n\telse:\n\t\tpass\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, true, temp.path()).unwrap();
        assert!(!result.applied);
        assert_eq!(result.original_condition, "true");
        assert_eq!(result.inverted_condition, "false");
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if true:"),
            "dry run should not modify file"
        );
    }

    #[test]
    fn invert_no_else_errors() {
        let temp = setup_project(&[("test.gd", "func foo():\n\tif true:\n\t\tpass\n")]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path());
        assert!(result.is_err());
    }

    #[test]
    fn invert_elif_chain() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo(x):\n\tif x == 1:\n\t\tprint(\"one\")\n\telif x == 2:\n\t\tprint(\"two\")\n\telse:\n\t\tprint(\"other\")\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if x == 2:"),
            "first elif should become new if, got:\n{content}"
        );
        assert!(
            content.contains("elif x == 1:"),
            "original if should become elif, got:\n{content}"
        );
    }

    #[test]
    fn invert_boolean_literals() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo():\n\tif true:\n\t\tprint(\"yes\")\n\telse:\n\t\tprint(\"no\")\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if false:"),
            "true should become false, got:\n{content}"
        );
    }

    #[test]
    fn invert_not_equals() {
        let temp = setup_project(&[(
            "test.gd",
            "func foo(x):\n\tif x != null:\n\t\tuse(x)\n\telse:\n\t\tprint(\"null\")\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 2, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if x == null:"),
            "!= should become ==, got:\n{content}"
        );
    }

    #[test]
    fn invert_top_level_if() {
        let temp = setup_project(&[(
            "test.gd",
            "if Engine.is_editor_hint():\n\tpass\nelse:\n\trun_game()\n",
        )]);
        let result = invert_if(&temp.path().join("test.gd"), 1, false, temp.path()).unwrap();
        assert!(result.applied);
        let content = fs::read_to_string(temp.path().join("test.gd")).unwrap();
        assert!(
            content.contains("if not Engine.is_editor_hint():"),
            "should negate call, got:\n{content}"
        );
    }

    #[test]
    fn negate_double_not() {
        // `not not x` → `x` (strip one not, leaving `not x`, then... actually
        // the AST for `not not x` is unary_operator(not, unary_operator(not, x)).
        // Our function sees "not not x", strips "not " → "not x". That's correct:
        // the negation of `not not x` is `not x`.
        let source = "not x";
        let tree =
            crate::core::parser::parse(&format!("if {source}:\n\tpass\nelse:\n\tpass\n")).unwrap();
        let if_node = tree.root_node().child(0).unwrap();
        let mut cursor = if_node.walk();
        let condition = if_node
            .children(&mut cursor)
            .find(|c| {
                c.is_named()
                    && !matches!(c.kind(), "body" | "elif_clause" | "else_clause" | "comment")
            })
            .unwrap();
        let full_source = format!("if {source}:\n\tpass\nelse:\n\tpass\n");
        let result = negate_condition(&condition, &full_source);
        assert_eq!(result, "x");
    }

    #[test]
    fn negate_comparison_operators() {
        assert_eq!(flip_comparison("x == 1"), Some("x != 1".to_string()));
        assert_eq!(flip_comparison("x != 1"), Some("x == 1".to_string()));
        assert_eq!(flip_comparison("x < 10"), Some("x >= 10".to_string()));
        assert_eq!(flip_comparison("x > 10"), Some("x <= 10".to_string()));
        assert_eq!(flip_comparison("x <= 10"), Some("x > 10".to_string()));
        assert_eq!(flip_comparison("x >= 10"), Some("x < 10".to_string()));
        assert_eq!(flip_comparison("is_alive"), None);
    }

    #[test]
    fn negate_de_morgan_text() {
        assert_eq!(
            apply_de_morgan("a and b"),
            Some("not a or not b".to_string())
        );
        assert_eq!(
            apply_de_morgan("a or b"),
            Some("not a and not b".to_string())
        );
        assert_eq!(
            apply_de_morgan("not a and b"),
            Some("a or not b".to_string())
        );
        // Nested parens should not split
        assert_eq!(
            apply_de_morgan("(a and b) or c"),
            Some("not (a and b) and not c".to_string())
        );
    }
}
