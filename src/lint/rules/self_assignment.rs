use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct SelfAssignment;

impl LintRule for SelfAssignment {
    fn name(&self) -> &'static str {
        "self-assignment"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "assignment" {
        // assignment has children: left, "=", right
        let child_count = node.child_count();
        if child_count >= 3 {
            let left = node.child(0);
            let right = node.child(child_count - 1);

            if let (Some(left), Some(right)) = (left, right) {
                let left_text = &source[left.byte_range()];
                let right_text = &source[right.byte_range()];

                if left_text == right_text {
                    diags.push(LintDiagnostic {
                        rule: "self-assignment",
                        message: format!("`{}` is assigned to itself", left_text),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        fix: None,
                    });
                }
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
