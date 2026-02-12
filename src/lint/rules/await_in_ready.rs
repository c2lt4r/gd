use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct AwaitInReady;

impl LintRule for AwaitInReady {
    fn name(&self) -> &'static str {
        "await-in-ready"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        find_ready_functions(root, source, &mut diags);
        diags
    }
}

fn find_ready_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "function_definition"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                if name == "_ready"
                    && let Some(body) = child.child_by_field_name("body")
                {
                    find_awaits(body, source, diags);
                }
            }

            // Recurse into class bodies
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                find_ready_functions(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn find_awaits(node: Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "await" {
        diags.push(LintDiagnostic {
            rule: "await-in-ready",
            message: "avoid `await` in _ready(); use call_deferred() or a separate async method"
                .to_string(),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            // Don't recurse into nested function definitions
            let child = cursor.node();
            if child.kind() != "function_definition" {
                find_awaits(child, _source, diags);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
