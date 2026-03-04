use gd_core::gd_ast::{self, GdExpr, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct PreferInOperator;

impl LintRule for PreferInOperator {
    fn name(&self) -> &'static str {
        "prefer-in-operator"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::BinOp { node, op: "or", .. } = expr {
                // Only trigger on the top-level `or` — skip if parent is also `or`
                if let Some(parent) = node.parent()
                    && parent.kind() == "binary_operator"
                    && parent
                        .child_by_field_name("op")
                        .is_some_and(|o| &source[o.byte_range()] == "or")
                {
                    return;
                }

                let Some(comparisons) = collect_or_chain(expr, source) else {
                    return;
                };

                if comparisons.len() < 2 {
                    return;
                }

                // All left-hand sides must be the same
                let variable = comparisons[0].0;
                if !comparisons.iter().all(|c| c.0 == variable) {
                    return;
                }

                let values: Vec<&str> = comparisons.iter().map(|c| c.1).collect();
                let values_str = values.join(", ");
                let replacement = format!("{variable} in [{values_str}]");

                diags.push(LintDiagnostic {
                    rule: "prefer-in-operator",
                    message: format!(
                        "use `{replacement}` instead of chained `==`/`or` comparisons"
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
        });
        diags
    }
}

/// Collect all `==` comparisons from a chain of `or` BinOps.
/// Returns Vec of (variable, value) pairs.
fn collect_or_chain<'a>(expr: &GdExpr<'a>, source: &'a str) -> Option<Vec<(&'a str, &'a str)>> {
    match expr {
        GdExpr::BinOp {
            op: "or",
            left,
            right,
            ..
        } => {
            let mut comps = collect_or_chain(left, source)?;
            let eq = parse_eq(right, source)?;
            comps.push(eq);
            Some(comps)
        }
        GdExpr::BinOp {
            op: "==",
            left,
            right,
            ..
        } => {
            let lhs = &source[left.node().byte_range()];
            let rhs = &source[right.node().byte_range()];
            Some(vec![(lhs, rhs)])
        }
        _ => None,
    }
}

/// Extract (variable, value) from an `==` BinOp.
fn parse_eq<'a>(expr: &GdExpr<'a>, source: &'a str) -> Option<(&'a str, &'a str)> {
    if let GdExpr::BinOp {
        op: "==",
        left,
        right,
        ..
    } = expr
    {
        let lhs = &source[left.node().byte_range()];
        let rhs = &source[right.node().byte_range()];
        Some((lhs, rhs))
    } else {
        None
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
        PreferInOperator.check(&file, source, &config)
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
    fn detects_two_comparisons() {
        let source = "func f(x):\n\tif x == 1 or x == 2:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x in [1, 2]"));
    }

    #[test]
    fn detects_three_comparisons() {
        let source = "func f(x):\n\tif x == 1 or x == 2 or x == 3:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x in [1, 2, 3]"));
    }

    #[test]
    fn detects_string_comparisons() {
        let source = "func f(x):\n\tif x == \"a\" or x == \"b\" or x == \"c\":\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x in [\"a\", \"b\", \"c\"]"));
    }

    #[test]
    fn no_warning_different_variables() {
        let source = "func f(x, y):\n\tif x == 1 or y == 2:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_single_comparison() {
        let source = "func f(x):\n\tif x == 1:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_and_operator() {
        let source = "func f(x):\n\tif x == 1 and x == 2:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_not_equals() {
        let source = "func f(x):\n\tif x != 1 or x != 2:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_two_comparisons() {
        let source = "func f(x):\n\tif x == 1 or x == 2:\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("x in [1, 2]"));
    }

    #[test]
    fn fix_three_comparisons() {
        let source = "func f(x):\n\tif x == 1 or x == 2 or x == 3:\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("x in [1, 2, 3]"));
    }

    #[test]
    fn only_one_diagnostic_for_chain() {
        let source = "func f(x):\n\tif x == 1 or x == 2 or x == 3:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_with_method_call_values() {
        let source = "func f(x):\n\tif x == foo() or x == bar():\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x in [foo(), bar()]"));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!PreferInOperator.default_enabled());
    }
}
