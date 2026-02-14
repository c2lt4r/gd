use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct IntegerDivision;

impl LintRule for IntegerDivision {
    fn name(&self) -> &'static str {
        "integer-division"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
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
        && let Some(op_node) = node.child_by_field_name("op")
    {
        let op = &source[op_node.byte_range()];
        if op == "/" {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");

            let left_is_int = left.as_ref().is_some_and(|n| n.kind() == "integer");
            let right_is_int = right.as_ref().is_some_and(|n| n.kind() == "integer");

            if left_is_int && right_is_int {
                let left_node = left.unwrap();
                let left_text = &source[left_node.byte_range()];

                diags.push(LintDiagnostic {
                    rule: "integer-division",
                    message: format!(
                        "integer division truncates to integer, use {}.0 / {} for float result",
                        left_text,
                        &source[right.unwrap().byte_range()],
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
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
