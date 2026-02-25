use tree_sitter::{Node, Tree};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PreferTernary;

impl LintRule for PreferTernary {
    fn name(&self) -> &'static str {
        "prefer-ternary"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "if_statement" {
        check_if_else(node, source, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_if_else(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Must have an else clause, no elif
    if has_elif(node) {
        return;
    }

    let Some(condition) = node.child_by_field_name("condition") else {
        return;
    };
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };
    let Some(else_node) = find_else_clause(node) else {
        return;
    };
    let Some(else_body) = else_node.child_by_field_name("body") else {
        return;
    };

    // Each body must have exactly one statement that is an assignment
    let Some(if_stmt) = single_named_non_comment_child(body) else {
        return;
    };
    let Some(else_stmt) = single_named_non_comment_child(else_body) else {
        return;
    };

    let Some((if_var, if_val)) = extract_assignment(if_stmt, source) else {
        return;
    };
    let Some((else_var, else_val)) = extract_assignment(else_stmt, source) else {
        return;
    };

    // Must assign to the same variable
    if if_var != else_var {
        return;
    }

    // Values must be single-line (no newlines)
    if if_val.contains('\n') || else_val.contains('\n') {
        return;
    }

    let cond_text = &source[condition.byte_range()];
    let replacement = format!("{if_var} = {if_val} if {cond_text} else {else_val}");

    let fix = generate_if_else_fix(node, &replacement, source);

    diags.push(LintDiagnostic {
        rule: "prefer-ternary",
        message: format!(
            "this if/else assigns to `{if_var}` in both branches; use `{replacement}`"
        ),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix: Some(fix),
        context_lines: None,
    });
}

fn find_else_clause(if_node: Node) -> Option<Node> {
    let mut cursor = if_node.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "else_clause" {
            return Some(child);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    None
}

fn has_elif(if_node: Node) -> bool {
    let mut cursor = if_node.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    loop {
        let kind = cursor.node().kind();
        if kind == "elif_clause" {
            return true;
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    false
}

fn single_named_non_comment_child(body: Node) -> Option<Node> {
    let mut result = None;
    let mut count = 0;
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.is_named() && child.kind() != "comment" {
            count += 1;
            if count > 1 {
                return None;
            }
            result = Some(child);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    result
}

/// Extract (variable, value) from an assignment statement.
/// Handles both `expression_statement > assignment` and bare `assignment`.
fn extract_assignment<'a>(stmt: Node<'a>, source: &'a str) -> Option<(&'a str, &'a str)> {
    let assign = if stmt.kind() == "expression_statement" {
        let inner = stmt.named_child(0)?;
        if inner.kind() == "assignment" {
            inner
        } else {
            return None;
        }
    } else if stmt.kind() == "assignment" {
        stmt
    } else {
        return None;
    };

    let left = assign.child_by_field_name("left")?;
    let right = assign.child_by_field_name("right")?;
    Some((&source[left.byte_range()], &source[right.byte_range()]))
}

fn generate_if_else_fix(if_node: Node, replacement_line: &str, source: &str) -> Fix {
    let source_bytes = source.as_bytes();

    // Find the start of the line containing the if
    let mut line_start = if_node.start_byte();
    while line_start > 0 && source_bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }

    // Get indentation
    let indent = &source[line_start..if_node.start_byte()];

    // Include trailing newline in the replaced range if present
    let mut end = if_node.end_byte();
    if end < source_bytes.len() && source_bytes[end] == b'\n' {
        end += 1;
    }

    Fix {
        byte_start: line_start,
        byte_end: end,
        replacement: format!("{indent}{replacement_line}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        PreferTernary.check(&tree, source, &config)
    }

    fn apply_fix(source: &str, fix: &Fix) -> String {
        format!(
            "{}{}{}",
            &source[..fix.byte_start],
            &fix.replacement,
            &source[fix.byte_end..]
        )
    }

    #[test]
    fn detects_simple_if_else_assignment() {
        let source = "func f(cond):\n\tif cond:\n\t\tx = 1\n\telse:\n\t\tx = 2\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x = 1 if cond else 2"));
    }

    #[test]
    fn detects_string_values() {
        let source = "func f(cond):\n\tif cond:\n\t\tname = \"yes\"\n\telse:\n\t\tname = \"no\"\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains("name = \"yes\" if cond else \"no\"")
        );
    }

    #[test]
    fn no_warning_different_variables() {
        let source = "func f(cond):\n\tif cond:\n\t\tx = 1\n\telse:\n\t\ty = 2\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_elif_present() {
        let source =
            "func f(cond):\n\tif cond:\n\t\tx = 1\n\telif other:\n\t\tx = 2\n\telse:\n\t\tx = 3\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_multiple_statements_in_body() {
        let source =
            "func f(cond):\n\tif cond:\n\t\tprint(\"yes\")\n\t\tx = 1\n\telse:\n\t\tx = 2\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_no_else() {
        let source = "func f(cond):\n\tif cond:\n\t\tx = 1\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_assignment() {
        let source = "func f(cond):\n\tif cond:\n\t\treturn 1\n\telse:\n\t\treturn 2\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_simple_ternary() {
        let source = "func f(cond):\n\tif cond:\n\t\tx = 1\n\telse:\n\t\tx = 2\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(fixed, "func f(cond):\n\tx = 1 if cond else 2\n");
    }

    #[test]
    fn fix_preserves_indent() {
        let source = "func f(cond):\n\t\tif cond:\n\t\t\tx = 1\n\t\telse:\n\t\t\tx = 2\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("\t\tx = 1 if cond else 2"));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!PreferTernary.default_enabled());
    }
}
