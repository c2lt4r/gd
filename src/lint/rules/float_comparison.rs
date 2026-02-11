use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct FloatComparison;

impl LintRule for FloatComparison {
    fn name(&self) -> &'static str {
        "float-comparison"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "binary_operator"
        && let Some(op_node) = node.child_by_field_name("operator") {
            let op = &source[op_node.byte_range()];
            if op == "==" || op == "!=" {
                let left = node.child_by_field_name("left");
                let right = node.child_by_field_name("right");

                let left_is_float = left.as_ref().is_some_and(|n| n.kind() == "float");
                let right_is_float = right.as_ref().is_some_and(|n| n.kind() == "float");

                if left_is_float || right_is_float {
                    diags.push(LintDiagnostic {
                        rule: "float-comparison",
                        message: "comparing floats with == is unreliable; use is_equal_approx() instead".to_string(),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        fix: None,
                    end_column: None,
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
