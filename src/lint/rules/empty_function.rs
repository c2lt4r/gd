use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct EmptyFunction;

impl LintRule for EmptyFunction {
    fn name(&self) -> &'static str {
        "empty-function"
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
        && let Some(body) = node.child_by_field_name("body")
    {
        // An empty function body has exactly one named child: a pass_statement
        let named_count = body.named_child_count();
        if named_count == 1
            && let Some(first) = body.named_child(0)
            && first.kind() == "pass_statement"
        {
            let func_name = node
                .child_by_field_name("name")
                .map(|n| &source[n.byte_range()])
                .unwrap_or("<unknown>");
            diags.push(LintDiagnostic {
                rule: "empty-function",
                message: format!("function `{}` has an empty body (only `pass`)", func_name),
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
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
