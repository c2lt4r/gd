use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{Fix, LintDiagnostic, LintRule, Severity};

pub struct SignalNameConvention;

impl LintRule for SignalNameConvention {
    fn name(&self) -> &'static str {
        "signal-name-convention"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "signal_statement"
        && let Some(name_node) = node.child_by_field_name("name") {
            let name = &source[name_node.byte_range()];
            if let Some(fixed) = name.strip_prefix("on_") {
                // Remove "on_" prefix

                diags.push(LintDiagnostic {
                    rule: "signal-name-convention",
                    message: format!(
                        "signal names shouldn't use \"on_\" prefix, use \"{}\" instead",
                        fixed,
                    ),
                    severity: Severity::Warning,
                    line: name_node.start_position().row,
                    column: name_node.start_position().column,
                    end_column: Some(name_node.end_position().column),
                    fix: Some(Fix {
                        byte_start: name_node.start_byte(),
                        byte_end: name_node.end_byte(),
                        replacement: fixed.to_string(),
                    }),
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
