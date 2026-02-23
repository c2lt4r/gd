use tree_sitter::{Node, Tree};

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

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "binary_operator"
        && let Some(op_node) = node.child_by_field_name("op")
    {
        let op = &source[op_node.byte_range()];
        match op {
            "and" => check_and_range(node, source, diags),
            "or" => check_or_range(node, source, diags),
            _ => {}
        }
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

/// A comparison half: variable, operator, bound expression.
struct ComparisonParts<'a> {
    variable: &'a str,
    op: &'a str,
    bound: &'a str,
}

/// Try to extract a comparison of the form `x >= a` or `a <= x` (lower bound)
/// or `x < b` or `b > x` (upper bound) from a binary_operator node.
fn parse_comparison<'a>(node: Node<'a>, source: &'a str) -> Option<ComparisonParts<'a>> {
    if node.kind() != "binary_operator" {
        return None;
    }
    let op_node = node.child_by_field_name("op")?;
    let op = &source[op_node.byte_range()];
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;
    let left_text = &source[left.byte_range()];
    let right_text = &source[right.byte_range()];

    Some(ComparisonParts {
        variable: left_text,
        op,
        bound: right_text,
    })
}

/// Check `x >= a and x < b` pattern (inclusive lower, exclusive upper).
fn check_and_range(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };

    let Some(left_cmp) = parse_comparison(left, source) else {
        return;
    };
    let Some(right_cmp) = parse_comparison(right, source) else {
        return;
    };

    // Try to match: (x >= a) and (x < b)
    // Normalized forms:
    //   left: x >= a  or  a <= x
    //   right: x < b  or  b > x
    if let Some((var, lower, upper)) = match_and_pattern(&left_cmp, &right_cmp) {
        let suggestion = format!("{var} in range({lower}, {upper})");
        diags.push(LintDiagnostic {
            rule: "manual-range-contains",
            message: format!(
                "manual range check can be written as `{suggestion}`"
            ),
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
fn check_or_range(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let Some(left) = node.child_by_field_name("left") else {
        return;
    };
    let Some(right) = node.child_by_field_name("right") else {
        return;
    };

    let Some(left_cmp) = parse_comparison(left, source) else {
        return;
    };
    let Some(right_cmp) = parse_comparison(right, source) else {
        return;
    };

    // Try to match: (x < a) or (x >= b)  — negation of `x in range(a, b)`
    if let Some((var, lower, upper)) = match_or_pattern(&left_cmp, &right_cmp) {
        let suggestion = format!("{var} not in range({lower}, {upper})");
        diags.push(LintDiagnostic {
            rule: "manual-range-contains",
            message: format!(
                "manual negated range check can be written as `{suggestion}`"
            ),
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
/// Returns (variable, lower_bound, upper_bound).
fn match_and_pattern<'a>(
    left: &ComparisonParts<'a>,
    right: &ComparisonParts<'a>,
) -> Option<(&'a str, &'a str, &'a str)> {
    // Form 1: x >= a and x < b
    if (left.op == ">=" && right.op == "<") && left.variable == right.variable {
        return Some((left.variable, left.bound, right.bound));
    }
    // Form 2: a <= x and x < b
    if (left.op == "<=" && right.op == "<") && left.bound == right.variable {
        return Some((left.bound, left.variable, right.bound));
    }
    // Form 3: x >= a and b > x
    if (left.op == ">=" && right.op == ">") && left.variable == right.bound {
        return Some((left.variable, left.bound, right.variable));
    }
    // Form 4: a <= x and b > x
    if (left.op == "<=" && right.op == ">") && left.bound == right.bound {
        return Some((left.bound, left.variable, right.variable));
    }
    None
}

/// Match `(x < a) or (x >= b)` — negated range pattern.
/// Returns (variable, lower_bound, upper_bound).
fn match_or_pattern<'a>(
    left: &ComparisonParts<'a>,
    right: &ComparisonParts<'a>,
) -> Option<(&'a str, &'a str, &'a str)> {
    // Form 1: x < a or x >= b
    if (left.op == "<" && right.op == ">=") && left.variable == right.variable {
        return Some((left.variable, left.bound, right.bound));
    }
    // Form 2: a > x or x >= b
    if (left.op == ">" && right.op == ">=") && left.bound == right.variable {
        return Some((left.bound, left.variable, right.bound));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        ManualRangeContains.check(&tree, source, &config)
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
        // x > 0 and x < 10 is not the standard range pattern (> not >=)
        let source = "func f(x):\n\tif x > 0 and x < 10:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_inclusive_upper_bound() {
        // x >= 0 and x <= 10 is not the standard range pattern (<= not <)
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
