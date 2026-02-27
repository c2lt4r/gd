use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ComparisonWithItself;

impl LintRule for ComparisonWithItself {
    fn name(&self) -> &'static str {
        "comparison-with-itself"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::BinOp { node, op, left, right, .. } = expr
                && matches!(*op, "==" | "!=" | "<" | ">" | "<=" | ">=")
            {
                let left_text = &source[left.node().byte_range()];
                let right_text = &source[right.node().byte_range()];

                if left_text == right_text {
                    diags.push(LintDiagnostic {
                        rule: "comparison-with-itself",
                        message: format!(
                            "comparing `{left_text}` with itself (`{left_text} {op} {right_text}`)",
                        ),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: Some(node.end_position().column),
                        fix: None,
                        context_lines: None,
                    });
                }
            }
        });
        diags
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
        ComparisonWithItself.check(&file, source, &config)
    }

    // ── Should warn ─────────────────────────────────────────────────

    #[test]
    fn equal_to_itself() {
        let source = "\
func foo():
\tif x == x:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "comparison-with-itself");
        assert!(diags[0].message.contains("x == x"));
    }

    #[test]
    fn not_equal_to_itself() {
        let source = "\
func foo():
\tif x != x:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x != x"));
    }

    #[test]
    fn less_than_itself() {
        let source = "\
func foo():
\tif x < x:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x < x"));
    }

    #[test]
    fn greater_than_itself() {
        let source = "\
func foo():
\tif x > x:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn less_equal_itself() {
        let source = "\
func foo():
\tvar r = x <= x
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn greater_equal_itself() {
        let source = "\
func foo():
\tvar r = x >= x
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn complex_expression_same() {
        let source = "\
func foo():
\tif obj.value == obj.value:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("obj.value"));
    }

    #[test]
    fn self_comparison_in_assignment() {
        let source = "\
func foo():
\tvar b = score == score
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("score"));
    }

    // ── Should NOT warn ─────────────────────────────────────────────

    #[test]
    fn different_identifiers() {
        let source = "\
func foo():
\tif x == y:
\t\tpass
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn different_attributes() {
        let source = "\
func foo():
\tif obj.a == obj.b:
\t\tpass
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn comparison_with_literal() {
        let source = "\
func foo():
\tif x == 0:
\t\tpass
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn arithmetic_not_comparison() {
        let source = "\
func foo():
\tvar r = x + x
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn different_complex_expressions() {
        let source = "\
func foo():
\tif a.x == b.x:
\t\tpass
";
        assert!(check(source).is_empty());
    }
}
