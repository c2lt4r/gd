use tree_sitter::{Node, Tree};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NeedlessBool;

impl LintRule for NeedlessBool {
    fn name(&self) -> &'static str {
        "needless-bool"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    match node.kind() {
        "if_statement" => check_if_statement(node, source, diags),
        "conditional_expression" | "ternary_expression" => {
            check_ternary(node, source, diags);
        }
        _ => {}
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

fn check_if_statement(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Must have exactly one else clause, no elif
    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    let else_clause = find_else_clause(node);
    let Some(else_node) = else_clause else {
        return;
    };

    // Reject if there's an elif
    if has_elif(node) {
        return;
    }

    let Some(else_body) = else_node.child_by_field_name("body") else {
        return;
    };

    // Get the single statement from each body
    let Some(if_stmt) = single_named_non_comment_child(body) else {
        return;
    };
    let Some(else_stmt) = single_named_non_comment_child(else_body) else {
        return;
    };

    let Some(condition_node) = node.child_by_field_name("condition") else {
        return;
    };
    let condition = &source[condition_node.byte_range()];

    // Pattern 1: both return boolean literals
    if if_stmt.kind() == "return_statement"
        && else_stmt.kind() == "return_statement"
        && let (Some(if_bool), Some(else_bool)) =
            (return_bool_value(if_stmt, source), return_bool_value(else_stmt, source))
        && if_bool != else_bool
    {
        let suggestion = if if_bool {
            format!("return {condition}")
        } else {
            format!("return not {condition}")
        };
        let fix = generate_if_else_fix(node, &suggestion, source);
        diags.push(LintDiagnostic {
            rule: "needless-bool",
            message: format!(
                "this if/else returns booleans; simplify to `{suggestion}`"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: Some(fix),
            context_lines: None,
        });
        return;
    }

    // Pattern 2: both assign same variable to boolean literals
    if let (Some((if_var, if_bool)), Some((else_var, else_bool))) = (
        assignment_bool_value(if_stmt, source),
        assignment_bool_value(else_stmt, source),
    )
        && if_var == else_var
        && if_bool != else_bool
    {
        let suggestion = if if_bool {
            format!("{if_var} = {condition}")
        } else {
            format!("{if_var} = not {condition}")
        };
        let fix = generate_if_else_fix(node, &suggestion, source);
        diags.push(LintDiagnostic {
            rule: "needless-bool",
            message: format!(
                "this if/else assigns booleans to `{if_var}`; simplify to `{suggestion}`"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: Some(fix),
            context_lines: None,
        });
    }
}

fn check_ternary(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // conditional_expression: [0] = true branch, [1] = condition, [2] = false branch
    let Some(true_branch) = node.named_child(0) else {
        return;
    };
    let Some(condition) = node.named_child(1) else {
        return;
    };
    let Some(false_branch) = node.named_child(2) else {
        return;
    };

    let true_text = &source[true_branch.byte_range()];
    let false_text = &source[false_branch.byte_range()];

    let true_is_bool = true_text == "true" || true_text == "false";
    let false_is_bool = false_text == "true" || false_text == "false";

    if !true_is_bool || !false_is_bool || true_text == false_text {
        return;
    }

    let cond_text = &source[condition.byte_range()];
    let (suggestion, replacement) = if true_text == "true" {
        (cond_text.to_string(), cond_text.to_string())
    } else {
        (format!("not {cond_text}"), format!("not {cond_text}"))
    };

    diags.push(LintDiagnostic {
        rule: "needless-bool",
        message: format!(
            "this ternary returns booleans; simplify to `{suggestion}`"
        ),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: Some(node.end_position().column),
        fix: Some(Fix {
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            replacement,
        }),
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
        let child = cursor.node();
        if child.kind() == "elif_clause" {
            return true;
        }
        // Also check for elif inside else_clause
        if child.kind() == "else_clause" {
            let mut inner = child.walk();
            if inner.goto_first_child() {
                loop {
                    if inner.node().kind() == "if_statement" {
                        return true;
                    }
                    if !inner.goto_next_sibling() {
                        break;
                    }
                }
            }
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

fn return_bool_value(return_node: Node, source: &str) -> Option<bool> {
    let expr = return_node.named_child(0)?;
    let text = &source[expr.byte_range()];
    match text {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn assignment_bool_value<'a>(stmt: Node<'a>, source: &'a str) -> Option<(&'a str, bool)> {
    // Unwrap expression_statement > assignment
    let assign = if stmt.kind() == "expression_statement" {
        let inner = stmt.named_child(0)?;
        if inner.kind() == "assignment" { inner } else { return None }
    } else if stmt.kind() == "assignment" {
        stmt
    } else {
        return None;
    };
    let left = assign.child_by_field_name("left")?;
    let right = assign.child_by_field_name("right")?;
    let var_name = &source[left.byte_range()];
    let val_text = &source[right.byte_range()];
    let val = match val_text {
        "true" => true,
        "false" => false,
        _ => return None,
    };
    Some((var_name, val))
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
        NeedlessBool.check(&tree, source, &config)
    }

    fn apply_fix(source: &str, fix: &Fix) -> String {
        format!(
            "{}{}{}",
            &source[..fix.byte_start],
            &fix.replacement,
            &source[fix.byte_end..]
        )
    }

    // ── if/else return ────────────────────────────────────────────

    #[test]
    fn detects_return_true_else_false() {
        let source = "func f(x):\n\tif x > 0:\n\t\treturn true\n\telse:\n\t\treturn false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("return x > 0"));
    }

    #[test]
    fn detects_return_false_else_true() {
        let source = "func f(x):\n\tif x > 0:\n\t\treturn false\n\telse:\n\t\treturn true\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("return not x > 0"));
    }

    #[test]
    fn no_warning_non_bool_return() {
        let source = "func f(x):\n\tif x > 0:\n\t\treturn 1\n\telse:\n\t\treturn 0\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_elif_present() {
        let source = "func f(x):\n\tif x > 0:\n\t\treturn true\n\telif x == 0:\n\t\treturn false\n\telse:\n\t\treturn true\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_extra_statements_in_body() {
        let source =
            "func f(x):\n\tif x > 0:\n\t\tprint(x)\n\t\treturn true\n\telse:\n\t\treturn false\n";
        assert!(check(source).is_empty());
    }

    // ── if/else assignment ────────────────────────────────────────

    #[test]
    fn detects_assignment_true_else_false() {
        let source = "func f(x):\n\tif x > 0:\n\t\ty = true\n\telse:\n\t\ty = false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("y = x > 0"));
    }

    #[test]
    fn no_warning_different_variables() {
        let source = "func f(x):\n\tif x > 0:\n\t\ty = true\n\telse:\n\t\tz = false\n";
        assert!(check(source).is_empty());
    }

    // ── ternary ───────────────────────────────────────────────────

    #[test]
    fn detects_ternary_true_false() {
        let source = "func f(x):\n\tvar y = true if x > 0 else false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("simplify"));
    }

    #[test]
    fn detects_ternary_false_true() {
        let source = "func f(x):\n\tvar y = false if x > 0 else true\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("not"));
    }

    #[test]
    fn no_warning_ternary_non_bool() {
        let source = "func f(x):\n\tvar y = 1 if x > 0 else 0\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_ternary_same_bool() {
        let source = "func f(x):\n\tvar y = true if x > 0 else true\n";
        assert!(check(source).is_empty());
    }

    // ── fix correctness ──────────────────────────────────────────

    #[test]
    fn fix_ternary_true_false() {
        let source = "func f(x):\n\tvar y = true if x > 0 else false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "x > 0");
    }

    #[test]
    fn fix_ternary_false_true() {
        let source = "func f(x):\n\tvar y = false if x > 0 else true\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "not x > 0");
    }

    #[test]
    fn fix_if_else_return() {
        let source = "func f(x):\n\tif x > 0:\n\t\treturn true\n\telse:\n\t\treturn false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(fixed, "func f(x):\n\treturn x > 0\n");
    }

    #[test]
    fn fix_if_else_assignment() {
        let source = "func f(x):\n\tif x > 0:\n\t\ty = true\n\telse:\n\t\ty = false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(fixed, "func f(x):\n\ty = x > 0\n");
    }

    #[test]
    fn default_enabled() {
        assert!(NeedlessBool.default_enabled());
    }
}
