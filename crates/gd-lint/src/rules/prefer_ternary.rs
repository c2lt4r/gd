use gd_core::gd_ast::{self, GdExpr, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

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

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            check_if_else(stmt, source, &mut diags);
        });
        diags
    }
}

fn check_if_else(stmt: &GdStmt<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let GdStmt::If(gif) = stmt else { return };

    // Must have an else clause, no elif
    if !gif.elif_branches.is_empty() || gif.else_body.is_none() {
        return;
    }
    let else_body = gif.else_body.as_ref().unwrap();

    // Each body must have exactly one assignment statement
    if gif.body.len() != 1 || else_body.len() != 1 {
        return;
    }

    let (if_var, if_val) = extract_assignment(&gif.body[0], source);
    let (else_var, else_val) = extract_assignment(&else_body[0], source);

    let (Some(if_var), Some(if_val)) = (if_var, if_val) else {
        return;
    };
    let (Some(else_var), Some(else_val)) = (else_var, else_val) else {
        return;
    };

    // Must assign to the same variable
    if if_var != else_var {
        return;
    }

    // Values must be single-line
    if if_val.contains('\n') || else_val.contains('\n') {
        return;
    }

    let condition = &source[gif.condition.node().byte_range()];
    let replacement = format!("{if_var} = {if_val} if {condition} else {else_val}");

    let fix = generate_if_else_fix(gif.node, &replacement, source);

    diags.push(LintDiagnostic {
        rule: "prefer-ternary",
        message: format!(
            "this if/else assigns to `{if_var}` in both branches; use `{replacement}`"
        ),
        severity: Severity::Warning,
        line: gif.node.start_position().row,
        column: gif.node.start_position().column,
        end_column: None,
        fix: Some(fix),
        context_lines: None,
    });
}

/// Extract (variable_text, value_text) from an assignment statement.
fn extract_assignment<'a>(
    stmt: &GdStmt<'a>,
    source: &'a str,
) -> (Option<&'a str>, Option<&'a str>) {
    if let GdStmt::Assign { target, value, .. } = stmt {
        return (
            Some(&source[target.node().byte_range()]),
            Some(&source[value.node().byte_range()]),
        );
    }
    // Also handle Expr wrapping an assignment (expression_statement > assignment in CST)
    if let GdStmt::Expr { expr, .. } = stmt
        && let GdExpr::Ident { .. } = expr
    {
        // Not an assignment
    }
    (None, None)
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
    use gd_core::gd_ast;
    use gd_core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        PreferTernary.check(&file, source, &config)
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
