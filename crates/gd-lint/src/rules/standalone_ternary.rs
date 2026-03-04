use gd_core::gd_ast::{self, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct StandaloneTernary;

impl LintRule for StandaloneTernary {
    fn name(&self) -> &'static str {
        "standalone-ternary"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Expr { node, expr } = stmt
                && matches!(expr, GdExpr::Ternary { .. })
            {
                let text = &source[expr.node().byte_range()];
                let display = if text.len() > 40 {
                    format!("{}...", &text[..37])
                } else {
                    text.to_string()
                };
                diags.push(LintDiagnostic {
                    rule: "standalone-ternary",
                    message: format!(
                        "ternary expression `{display}` used as statement; result is unused"
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix: None,
                    context_lines: None,
                });
            }
        });
        diags
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
        StandaloneTernary.check(&file, source, &config)
    }

    #[test]
    fn ternary_as_statement() {
        let source = "func f():\n\t1 if true else 2\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("ternary"));
        assert!(diags[0].message.contains("unused"));
    }

    #[test]
    fn ternary_in_assignment_ok() {
        let source = "func f():\n\tvar x = 1 if true else 2\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn ternary_in_return_ok() {
        let source = "func f():\n\treturn 1 if true else 2\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn normal_call_ok() {
        let source = "func f():\n\tprint(\"hello\")\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(StandaloneTernary.default_enabled());
    }
}
