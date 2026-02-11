use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct MagicNumber;

impl LintRule for MagicNumber {
    fn name(&self) -> &'static str {
        "magic-number"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags, false);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>, in_function_body: bool) {
    let inside_body = in_function_body;

    // If we're entering a function body, mark it
    if node.kind() == "function_definition"
        && let Some(body_node) = node.child_by_field_name("body") {
            check_node(body_node, source, diags, true);
            // Don't recurse normally for function_definition since we handled body explicitly
            return;
        }

    // Only check numeric literals inside function bodies
    if inside_body && (node.kind() == "integer" || node.kind() == "float") {
        // Skip if this literal is part of a variable or const statement
        if !is_in_variable_or_const_definition(&node) {
            let value_text = &source[node.byte_range()];

            if !is_allowed_value(value_text) {
                diags.push(LintDiagnostic {
                    rule: "magic-number",
                    message: format!(
                        "consider extracting magic number {} to a named constant",
                        value_text,
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    fix: None,
                    end_column: None,
                });
            }
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags, inside_body);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if a node is inside a variable_statement or const_statement.
fn is_in_variable_or_const_definition(node: &Node) -> bool {
    let mut current = node.parent();
    while let Some(parent) = current {
        let kind = parent.kind();
        if kind == "variable_statement" || kind == "const_statement" {
            return true;
        }
        // Stop if we hit a function definition (top-level const/var are ok)
        if kind == "function_definition" {
            return false;
        }
        current = parent.parent();
    }
    false
}

/// Allowed numeric values that are commonly used and don't need extraction.
fn is_allowed_value(text: &str) -> bool {
    matches!(
        text,
        "0" | "1" | "-1" | "2" | "0.0" | "1.0" | "0.5" | "2.0" |
        "10" | "10.0" | "100" | "255" | "256" | "360"
    )
}
