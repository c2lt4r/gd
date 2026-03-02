use crate::core::gd_ast::{self, GdExpr, GdFile, GdStmt};

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

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            check_if_statement(stmt, source, &mut diags);
        });
        gd_ast::visit_exprs(file, &mut |expr| {
            check_ternary(expr, source, &mut diags);
        });
        diags
    }
}

fn check_if_statement(stmt: &GdStmt<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let GdStmt::If(gif) = stmt else { return };

    // Must have exactly one else clause, no elif
    if !gif.elif_branches.is_empty() || gif.else_body.is_none() {
        return;
    }
    let else_body = gif.else_body.as_ref().unwrap();

    // Each body must have exactly one statement
    if gif.body.len() != 1 || else_body.len() != 1 {
        return;
    }

    let condition = &source[gif.condition.node().byte_range()];

    // Pattern 1: both return boolean literals
    if let (
        GdStmt::Return {
            value: Some(if_expr),
            ..
        },
        GdStmt::Return {
            value: Some(else_expr),
            ..
        },
    ) = (&gif.body[0], &else_body[0])
        && let (
            GdExpr::Bool { value: if_bool, .. },
            GdExpr::Bool {
                value: else_bool, ..
            },
        ) = (if_expr, else_expr)
        && if_bool != else_bool
    {
        let suggestion = if *if_bool {
            format!("return {condition}")
        } else {
            format!("return not {condition}")
        };
        let fix = generate_if_else_fix(gif.node, &suggestion, source);
        diags.push(LintDiagnostic {
            rule: "needless-bool",
            message: format!("this if/else returns booleans; simplify to `{suggestion}`"),
            severity: Severity::Warning,
            line: gif.node.start_position().row,
            column: gif.node.start_position().column,
            end_column: None,
            fix: Some(fix),
            context_lines: None,
        });
        return;
    }

    // Pattern 2: both assign same variable to boolean literals
    if let (
        GdStmt::Assign {
            target: if_target,
            value: if_val,
            ..
        },
        GdStmt::Assign {
            target: else_target,
            value: else_val,
            ..
        },
    ) = (&gif.body[0], &else_body[0])
        && let GdExpr::Bool { value: if_bool, .. } = if_val
        && let GdExpr::Bool {
            value: else_bool, ..
        } = else_val
        && if_bool != else_bool
    {
        let if_var = &source[if_target.node().byte_range()];
        let else_var = &source[else_target.node().byte_range()];
        if if_var != else_var {
            return;
        }
        let suggestion = if *if_bool {
            format!("{if_var} = {condition}")
        } else {
            format!("{if_var} = not {condition}")
        };
        let fix = generate_if_else_fix(gif.node, &suggestion, source);
        diags.push(LintDiagnostic {
            rule: "needless-bool",
            message: format!(
                "this if/else assigns booleans to `{if_var}`; simplify to `{suggestion}`"
            ),
            severity: Severity::Warning,
            line: gif.node.start_position().row,
            column: gif.node.start_position().column,
            end_column: None,
            fix: Some(fix),
            context_lines: None,
        });
    }
}

fn check_ternary(expr: &GdExpr<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let GdExpr::Ternary {
        node,
        true_val,
        condition,
        false_val,
    } = expr
    else {
        return;
    };

    let GdExpr::Bool {
        value: true_bool, ..
    } = true_val.as_ref()
    else {
        return;
    };
    let GdExpr::Bool {
        value: false_bool, ..
    } = false_val.as_ref()
    else {
        return;
    };

    if true_bool == false_bool {
        return;
    }

    let cond_text = &source[condition.node().byte_range()];
    let (suggestion, replacement) = if *true_bool {
        (cond_text.to_string(), cond_text.to_string())
    } else {
        (format!("not {cond_text}"), format!("not {cond_text}"))
    };

    diags.push(LintDiagnostic {
        rule: "needless-bool",
        message: format!("this ternary returns booleans; simplify to `{suggestion}`"),
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

fn generate_if_else_fix(
    if_node: tree_sitter::Node<'_>,
    replacement_line: &str,
    source: &str,
) -> Fix {
    let source_bytes = source.as_bytes();

    // Find the start of the line containing the if
    let mut line_start = if_node.start_byte();
    while line_start > 0 && source_bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }

    // Get indentation
    let indent = &source[line_start..if_node.start_byte()];

    // Include trailing newline in the replaced range
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
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        NeedlessBool.check(&file, source, &config)
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
