use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ManualRangeContains;

impl LintRule for ManualRangeContains {
    fn name(&self) -> &'static str {
        "manual-range-contains"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::BinOp {
                node,
                op,
                left,
                right,
                ..
            } = expr
            {
                match *op {
                    "and" => check_and_range(node, left, right, source, &mut diags),
                    "or" => check_or_range(node, left, right, source, &mut diags),
                    _ => {}
                }
            }
        });
        diags
    }
}

/// A comparison: (variable, operator, bound).
struct CmpParts<'a> {
    variable: &'a str,
    op: &'a str,
    bound: &'a str,
}

/// Extract comparison parts from a BinOp expression.
fn parse_cmp<'a>(expr: &GdExpr<'a>, source: &'a str) -> Option<CmpParts<'a>> {
    if let GdExpr::BinOp {
        op, left, right, ..
    } = expr
    {
        Some(CmpParts {
            variable: &source[left.node().byte_range()],
            op,
            bound: &source[right.node().byte_range()],
        })
    } else {
        None
    }
}

/// Check `x >= a and x < b` pattern.
fn check_and_range(
    node: &tree_sitter::Node<'_>,
    left: &GdExpr<'_>,
    right: &GdExpr<'_>,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    let Some(l) = parse_cmp(left, source) else {
        return;
    };
    let Some(r) = parse_cmp(right, source) else {
        return;
    };

    if let Some((var, lower, upper)) = match_and_pattern(&l, &r) {
        let suggestion = format!("{var} in range({lower}, {upper})");
        diags.push(LintDiagnostic {
            rule: "manual-range-contains",
            message: format!("manual range check can be written as `{suggestion}`"),
            severity: Severity::Info,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: Some(Fix {
                byte_start: node.start_byte(),
                byte_end: node.end_byte(),
                replacement: suggestion,
            }),
            context_lines: None,
        });
    }
}

/// Check `x < a or x >= b` pattern (negated range).
fn check_or_range(
    node: &tree_sitter::Node<'_>,
    left: &GdExpr<'_>,
    right: &GdExpr<'_>,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    let Some(l) = parse_cmp(left, source) else {
        return;
    };
    let Some(r) = parse_cmp(right, source) else {
        return;
    };

    if let Some((var, lower, upper)) = match_or_pattern(&l, &r) {
        let suggestion = format!("{var} not in range({lower}, {upper})");
        diags.push(LintDiagnostic {
            rule: "manual-range-contains",
            message: format!("manual negated range check can be written as `{suggestion}`"),
            severity: Severity::Info,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: Some(Fix {
                byte_start: node.start_byte(),
                byte_end: node.end_byte(),
                replacement: suggestion,
            }),
            context_lines: None,
        });
    }
}

/// Match `(x >= a) and (x < b)` in all normalized forms.
fn match_and_pattern<'a>(
    left: &CmpParts<'a>,
    right: &CmpParts<'a>,
) -> Option<(&'a str, &'a str, &'a str)> {
    if left.op == ">=" && right.op == "<" && left.variable == right.variable {
        return Some((left.variable, left.bound, right.bound));
    }
    if left.op == "<=" && right.op == "<" && left.bound == right.variable {
        return Some((left.bound, left.variable, right.bound));
    }
    if left.op == ">=" && right.op == ">" && left.variable == right.bound {
        return Some((left.variable, left.bound, right.variable));
    }
    if left.op == "<=" && right.op == ">" && left.bound == right.bound {
        return Some((left.bound, left.variable, right.variable));
    }
    None
}

/// Match `(x < a) or (x >= b)` — negated range.
fn match_or_pattern<'a>(
    left: &CmpParts<'a>,
    right: &CmpParts<'a>,
) -> Option<(&'a str, &'a str, &'a str)> {
    if left.op == "<" && right.op == ">=" && left.variable == right.variable {
        return Some((left.variable, left.bound, right.bound));
    }
    if left.op == ">" && right.op == ">=" && left.bound == right.variable {
        return Some((left.bound, left.variable, right.bound));
    }
    None
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
        ManualRangeContains.check(&file, source, &config)
    }

    fn apply_fix(source: &str, fix: &Fix) -> String {
        format!(
            "{}{}{}",
            &source[..fix.byte_start],
            &fix.replacement,
            &source[fix.byte_end..]
        )
    }

    // ── and patterns ──────────────────────────────────────────────

    #[test]
    fn detects_x_gte_and_x_lt() {
        let source = "func f(x):\n\tif x >= 0 and x < 10:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x in range(0, 10)"));
    }

    #[test]
    fn detects_a_lte_x_and_x_lt_b() {
        let source = "func f(x):\n\tif 0 <= x and x < 10:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x in range(0, 10)"));
    }

    #[test]
    fn detects_variable_bounds() {
        let source = "func f(x, start, end):\n\tif x >= start and x < end:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x in range(start, end)"));
    }

    #[test]
    fn no_warning_different_variables() {
        let source = "func f(x, y):\n\tif x >= 0 and y < 10:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_exclusive_lower_bound() {
        let source = "func f(x):\n\tif x > 0 and x < 10:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_inclusive_upper_bound() {
        let source = "func f(x):\n\tif x >= 0 and x <= 10:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    // ── or patterns (negated) ─────────────────────────────────────

    #[test]
    fn detects_negated_range() {
        let source = "func f(x):\n\tif x < 0 or x >= 10:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x not in range(0, 10)"));
    }

    // ── fix correctness ──────────────────────────────────────────

    #[test]
    fn fix_and_pattern() {
        let source = "func f(x):\n\tvar y = x >= 0 and x < 10\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("x in range(0, 10)"));
    }

    #[test]
    fn fix_or_pattern() {
        let source = "func f(x):\n\tvar y = x < 0 or x >= 10\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("x not in range(0, 10)"));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!ManualRangeContains.default_enabled());
    }
}
