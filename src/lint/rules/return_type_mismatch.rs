use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct ReturnTypeMismatch;

impl LintRule for ReturnTypeMismatch {
    fn name(&self) -> &'static str {
        "return-type-mismatch"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition"
        && let Some(return_type_node) = node.child_by_field_name("return_type") {
            let return_type = &source[return_type_node.byte_range()];

            if let Some(body) = node.child_by_field_name("body") {
                if return_type == "void" {
                    // void function should not return a value
                    check_void_returns(body, source, diags);
                } else {
                    // non-void function should not have bare returns
                    check_bare_returns(body, source, diags);
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

fn check_void_returns(node: Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "return_statement" {
        // Check if return statement has a child (return value)
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            // Skip the "return" keyword itself
            let child = cursor.node();
            if child.is_named() {
                // Has a return value
                diags.push(LintDiagnostic {
                    rule: "return-type-mismatch",
                    message: "function declares -> void but returns a value".to_string(),
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
            check_void_returns(cursor.node(), _source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_bare_returns(node: Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "return_statement" {
        // Check if return statement has no named children (bare return)
        let mut has_value = false;
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                if cursor.node().is_named() {
                    has_value = true;
                    break;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if !has_value {
            diags.push(LintDiagnostic {
                rule: "return-type-mismatch",
                message: "function declares return type but has bare return".to_string(),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                fix: None,
                    end_column: None,
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_bare_returns(cursor.node(), _source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
