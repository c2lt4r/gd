use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct MissingSuperCall;

const LIFECYCLE_METHODS: &[&str] = &[
    "_ready",
    "_process",
    "_physics_process",
    "_enter_tree",
    "_exit_tree",
    "_input",
    "_unhandled_input",
];

impl LintRule for MissingSuperCall {
    fn name(&self) -> &'static str {
        "missing-super-call"
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
        && let Some(name_node) = node.child_by_field_name("name") {
            let func_name = &source[name_node.byte_range()];

            if LIFECYCLE_METHODS.contains(&func_name) {
                // Check if the function body contains a super call
                if let Some(body) = node.child_by_field_name("body")
                    && !has_super_call(body, source) {
                        diags.push(LintDiagnostic {
                            rule: "missing-super-call",
                            message: format!(
                                "function `{}` overrides a built-in; consider calling super()",
                                func_name
                            ),
                            severity: Severity::Warning,
                            line: name_node.start_position().row,
                            column: name_node.start_position().column,
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

fn has_super_call(node: Node, source: &str) -> bool {
    if node.kind() == "call"
        && let Some(func_node) = node.child_by_field_name("function") {
            let text = &source[func_node.byte_range()];
            if text.starts_with("super") {
                return true;
            }
        }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if has_super_call(cursor.node(), source) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    false
}
