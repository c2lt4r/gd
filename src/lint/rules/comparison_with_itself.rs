use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ComparisonWithItself;

impl LintRule for ComparisonWithItself {
    fn name(&self) -> &'static str {
        "comparison-with-itself"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

const COMPARISON_OPS: &[&str] = &["==", "!=", "<", ">", "<=", ">="];

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "binary_operator"
        && let Some(op_node) = node.child_by_field_name("op")
    {
        let op = &source[op_node.byte_range()];
        if COMPARISON_OPS.contains(&op)
            && let (Some(left), Some(right)) = (
                node.child_by_field_name("left"),
                node.child_by_field_name("right"),
            )
        {
            let left_text = &source[left.byte_range()];
            let right_text = &source[right.byte_range()];

            if left_text == right_text {
                diags.push(LintDiagnostic {
                    rule: "comparison-with-itself",
                    message: format!(
                        "comparing `{}` with itself (`{} {} {}`)",
                        left_text, left_text, op, right_text,
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix: None,
                });
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        ComparisonWithItself.check(&tree, source, &config)
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
