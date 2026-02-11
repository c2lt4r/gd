use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ComparisonWithBoolean;

impl LintRule for ComparisonWithBoolean {
    fn name(&self) -> &'static str {
        "comparison-with-boolean"
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

            let left_is_bool = left.as_ref().is_some_and(|n| is_boolean_literal(n, source));
            let right_is_bool = right
                .as_ref()
                .is_some_and(|n| is_boolean_literal(n, source));

            if left_is_bool || right_is_bool {
                let suggestion = if op == "==" {
                    "use the value directly (e.g. `if x:` instead of `if x == true:`)"
                } else {
                    "use `not` (e.g. `if not x:` instead of `if x != true:`)"
                };

                // Generate fix
                let fix = generate_fix(&node, left, right, op, left_is_bool, right_is_bool, source);

                diags.push(LintDiagnostic {
                    rule: "comparison-with-boolean",
                    message: format!(
                        "comparison `{}` with boolean literal is redundant; {}",
                        &source[node.byte_range()],
                        suggestion,
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix,
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

fn is_boolean_literal(node: &Node, source: &str) -> bool {
    let text = &source[node.byte_range()];
    text == "true" || text == "false"
}

fn generate_fix(
    node: &Node,
    left: Option<Node>,
    right: Option<Node>,
    op: &str,
    left_is_bool: bool,
    _right_is_bool: bool,
    source: &str,
) -> Option<Fix> {
    let (bool_node, other_node) = if left_is_bool {
        (left?, right?)
    } else {
        (right?, left?)
    };

    let bool_text = &source[bool_node.byte_range()];
    let other_text = source[other_node.byte_range()].to_string();

    let replacement = match (op, bool_text) {
        ("==", "true") => other_text,
        ("==", "false") => format!("not {}", other_text),
        ("!=", "true") => format!("not {}", other_text),
        ("!=", "false") => other_text,
        _ => return None,
    };

    Some(Fix {
        byte_start: node.start_byte(),
        byte_end: node.end_byte(),
        replacement,
    })
}
