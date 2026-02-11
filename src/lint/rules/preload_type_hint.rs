use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct PreloadTypeHint;

impl LintRule for PreloadTypeHint {
    fn name(&self) -> &'static str {
        "preload-type-hint"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "variable_statement" {
        // Check if it's a const (constants are fine)
        let first_child = node.child(0);
        if let Some(first) = first_child
            && &source[first.byte_range()] == "const" {
                // Constants are ok, skip
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        check_node(cursor.node(), source, diags);
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                return;
            }

        // Check if there's a type annotation
        let has_type = node.child_by_field_name("type").is_some();

        if !has_type {
            // Check if the value is a preload() or load() call
            if let Some(value_node) = node.child_by_field_name("value")
                && is_preload_or_load_call(&value_node, source) {
                    // Get the variable name for the diagnostic message
                    let var_name = if let Some(name_node) = node.child_by_field_name("name") {
                        source[name_node.byte_range()].to_string()
                    } else {
                        "variable".to_string()
                    };

                    diags.push(LintDiagnostic {
                        rule: "preload-type-hint",
                        message: format!(
                            "variable `{}` uses preload/load but has no type hint; consider adding a type annotation",
                            var_name
                        ),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
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

fn is_preload_or_load_call(node: &Node, source: &str) -> bool {
    if node.kind() == "call"
        && let Some(func_node) = node.child_by_field_name("function") {
            let func_name = &source[func_node.byte_range()];
            return func_name == "preload" || func_name == "load";
        }
    false
}
