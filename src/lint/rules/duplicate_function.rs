use std::collections::HashMap;
use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateFunction;

impl LintRule for DuplicateFunction {
    fn name(&self) -> &'static str {
        "duplicate-function"
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

fn check_scope(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut functions: HashMap<String, usize> = HashMap::new();

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "function_definition"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = name_node
                    .utf8_text(source.as_bytes())
                    .unwrap_or("")
                    .to_string();
                let line = name_node.start_position().row;

                if let Some(&first_line) = functions.get(&name) {
                    diags.push(LintDiagnostic {
                        rule: "duplicate-function",
                        message: format!(
                            "function `{}` already defined on line {}",
                            name,
                            first_line + 1,
                        ),
                        severity: Severity::Error,
                        line,
                        column: name_node.start_position().column,
                        end_column: None,
                        fix: None,
                        context_lines: None,
                    });
                } else {
                    functions.insert(name, line);
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
