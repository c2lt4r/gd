use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

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
        check_node(file.node, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "binary_operator" {
        check_or_chain(node, source, diags);
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

/// Represents one equality comparison: `x == value`.
struct EqComparison<'a> {
    variable: &'a str,
    value: &'a str,
}

/// Try to extract `x == value` from a binary_operator node.
fn parse_eq<'a>(node: Node<'a>, source: &'a str) -> Option<EqComparison<'a>> {
    if node.kind() != "binary_operator" {
        return None;
    }
    let op_node = node.child_by_field_name("op")?;
    let op = &source[op_node.byte_range()];
    if op != "==" {
        return None;
    }
    let left = node.child_by_field_name("left")?;
    let right = node.child_by_field_name("right")?;
    Some(EqComparison {
        variable: &source[left.byte_range()],
        value: &source[right.byte_range()],
    })
}

/// Collect all `==` comparisons from a chain of `or` binary operators.
/// Returns None if the chain contains non-`or` operators or non-`==` leaves.
fn collect_or_chain<'a>(node: Node<'a>, source: &'a str) -> Option<Vec<EqComparison<'a>>> {
    if node.kind() != "binary_operator" {
        return None;
    }

    let op_node = node.child_by_field_name("op")?;
    let op = &source[op_node.byte_range()];

    if op == "or" {
        let left = node.child_by_field_name("left")?;
        let right = node.child_by_field_name("right")?;

        let mut comparisons = collect_or_chain(left, source)?;
        // Right side should be a single == comparison
        let right_eq = parse_eq(right, source)?;
        comparisons.push(right_eq);
        Some(comparisons)
    } else if op == "==" {
        let eq = parse_eq(node, source)?;
        Some(vec![eq])
    } else {
        None
    }
}

fn check_or_chain(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let Some(op_node) = node.child_by_field_name("op") else {
        return;
    };
    let op = &source[op_node.byte_range()];
    if op != "or" {
        return;
    }

    // Only trigger on the top-level `or` — avoid re-checking inner `or` nodes.
    // We detect this by checking if the parent is also an `or` binary_operator.
    if let Some(parent) = node.parent()
        && parent.kind() == "binary_operator"
        && let Some(parent_op) = parent.child_by_field_name("op")
        && &source[parent_op.byte_range()] == "or"
    {
        return;
    }

    let Some(comparisons) = collect_or_chain(node, source) else {
        return;
    };

    // Need at least 2 comparisons
    if comparisons.len() < 2 {
        return;
    }

    // All left-hand sides must be the same
    let variable = comparisons[0].variable;
    if !comparisons.iter().all(|c| c.variable == variable) {
        return;
    }

    let values: Vec<&str> = comparisons.iter().map(|c| c.value).collect();
    let values_str = values.join(", ");
    let replacement = format!("{variable} in [{values_str}]");

    diags.push(LintDiagnostic {
        rule: "prefer-in-operator",
        message: format!("use `{replacement}` instead of chained `==`/`or` comparisons"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;
    use crate::core::gd_ast;

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
        // Should not emit one diagnostic per inner `or` node
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
