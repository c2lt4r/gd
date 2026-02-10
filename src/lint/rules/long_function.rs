use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct LongFunction;

impl LintRule for LongFunction {
    fn name(&self) -> &'static str {
        "long-function"
    }

    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let max_lines = config.max_function_length;
        check_node(root, source, max_lines, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, max_lines: usize, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition" {
        let start_line = node.start_position().row;
        let end_line = node.end_position().row;
        let line_count = end_line - start_line + 1;

        if line_count > max_lines {
            let func_name = node
                .child_by_field_name("name")
                .map(|n| &source[n.byte_range()])
                .unwrap_or("<unknown>");
            diags.push(LintDiagnostic {
                rule: "long-function",
                message: format!(
                    "function `{}` is {} lines long (max {})",
                    func_name, line_count, max_lines
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                fix: None,
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, max_lines, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
