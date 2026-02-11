use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PrivateMethodAccess;

const ALLOWED_CALLBACKS: &[&str] = &["_to_string"];

impl LintRule for PrivateMethodAccess {
    fn name(&self) -> &'static str {
        "private-method-access"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "call"
        && let Some(func_node) = node.child_by_field_name("function")
        && func_node.kind() == "attribute"
    {
        // This is a method call: obj.method()
        // First child is the object, last named child is the method name
        let mut cursor = func_node.walk();
        let mut object_text = String::new();

        if cursor.goto_first_child() {
            object_text = source[cursor.node().byte_range()].to_string();
        }

        // Get last named child (the method name)
        let last_child = func_node.named_child(func_node.named_child_count().saturating_sub(1));
        if let Some(method_node) = last_child {
            let method_text = source[method_node.byte_range()].to_string();

            if method_text.starts_with('_')
                && object_text != "self"
                && !ALLOWED_CALLBACKS.contains(&method_text.as_str())
            {
                diags.push(LintDiagnostic {
                    rule: "private-method-access",
                    message: format!(
                        "accessing private method `{}` on external object",
                        method_text
                    ),
                    severity: Severity::Warning,
                    line: method_node.start_position().row,
                    column: method_node.start_position().column,
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
