use tree_sitter::{Node, Tree};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct FloatComparison;

impl LintRule for FloatComparison {
    fn name(&self) -> &'static str {
        "float-comparison"
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
        if op == "==" || op == "!=" {
            let left = node.child_by_field_name("left");
            let right = node.child_by_field_name("right");

            let left_is_float = left.as_ref().is_some_and(|n| n.kind() == "float");
            let right_is_float = right.as_ref().is_some_and(|n| n.kind() == "float");

            if left_is_float || right_is_float {
                let fix = generate_fix(&node, left, right, op, source);

                diags.push(LintDiagnostic {
                    rule: "float-comparison",
                    message:
                        "comparing floats with == is unreliable; use is_equal_approx() instead"
                            .to_string(),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix,
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

fn generate_fix(
    node: &Node,
    left: Option<Node>,
    right: Option<Node>,
    op: &str,
    source: &str,
) -> Option<Fix> {
    let left_text = &source[left?.byte_range()];
    let right_text = &source[right?.byte_range()];

    let replacement = if op == "==" {
        format!("is_equal_approx({left_text}, {right_text})")
    } else {
        format!("!is_equal_approx({left_text}, {right_text})")
    };

    Some(Fix {
        byte_start: node.start_byte(),
        byte_end: node.end_byte(),
        replacement,
    })
}
