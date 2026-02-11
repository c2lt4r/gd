use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NodeReadyOrder;

impl LintRule for NodeReadyOrder {
    fn name(&self) -> &'static str {
        "node-ready-order"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        find_init_functions(root, source, &mut diags);
        diags
    }
}

/// Find `_init()` functions and check for node access inside them.
fn find_init_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            if child.kind() == "function_definition"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                if name == "_init"
                    && let Some(body) = child.child_by_field_name("body")
                {
                    find_node_access(body, source, diags);
                }
            }

            // Recurse into class bodies
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                find_init_functions(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Find `$NodePath` (get_node shorthand) and `get_node()` calls.
fn find_node_access(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    match node.kind() {
        // $NodePath syntax is parsed as `get_node` node in tree-sitter-gdscript
        "get_node" => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("$...");
            diags.push(LintDiagnostic {
                rule: "node-ready-order",
                message: format!(
                    "`{}` in _init() may fail; nodes are not ready until _ready()",
                    text
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: Some(node.end_position().column),
                fix: None,
            });
        }
        "call" => {
            // Check for get_node("...") or get_node_or_null("...") calls
            if let Some(func) = node.child_by_field_name("function") {
                let func_text = &source[func.byte_range()];
                if func_text == "get_node"
                    || func_text == "get_node_or_null"
                    || func_text.ends_with(".get_node")
                    || func_text.ends_with(".get_node_or_null")
                {
                    diags.push(LintDiagnostic {
                        rule: "node-ready-order",
                        message: format!(
                            "`{}(...)` in _init() may fail; nodes are not ready until _ready()",
                            func_text
                        ),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: Some(func.end_position().column),
                        fix: None,
                    });
                }
            }
        }
        // Don't recurse into nested function definitions (separate scope)
        "function_definition" | "lambda" => {}
        _ => {}
    }

    // Recurse into children (unless we already returned for function_definition/lambda)
    if node.kind() != "function_definition" && node.kind() != "lambda" {
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                find_node_access(cursor.node(), source, diags);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}
