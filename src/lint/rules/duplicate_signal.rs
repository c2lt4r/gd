use std::collections::HashMap;
use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateSignal;

impl LintRule for DuplicateSignal {
    fn name(&self) -> &'static str {
        "duplicate-signal"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = file.node;
        check_scope(root, source, &mut diags);
        diags
    }
}

/// Check a single scope (top-level or class body) for duplicate signal names.
fn check_scope(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Map signal name -> first occurrence line
    let mut signals: HashMap<String, usize> = HashMap::new();

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "signal_statement"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = source[name_node.byte_range()].to_string();
                let line = name_node.start_position().row;

                if let Some(&first_line) = signals.get(&name) {
                    diags.push(LintDiagnostic {
                        rule: "duplicate-signal",
                        message: format!(
                            "signal `{}` already declared on line {}",
                            name,
                            first_line + 1,
                        ),
                        severity: Severity::Error,
                        line,
                        column: name_node.start_position().column,
                        fix: None,
                        end_column: None,
                        context_lines: None,
                    });
                } else {
                    signals.insert(name, line);
                }
            }

            // Recurse into class definitions to check nested scopes
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                check_scope(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
