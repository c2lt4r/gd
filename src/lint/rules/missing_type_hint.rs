use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MissingTypeHint;

impl LintRule for MissingTypeHint {
    fn name(&self) -> &'static str {
        "missing-type-hint"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = file.node;
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition" {
        let func_name = node
            .child_by_field_name("name")
            .map(|n| source[n.byte_range()].to_string())
            .unwrap_or_default();

        // Check parameters for missing type hints
        if let Some(params) = node.child_by_field_name("parameters") {
            let mut cursor = params.walk();
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    // Plain identifier = untyped parameter
                    if child.kind() == "identifier" {
                        let param_name = &source[child.byte_range()];
                        diags.push(LintDiagnostic {
                            rule: "missing-type-hint",
                            message: format!(
                                "parameter `{param_name}` in function `{func_name}` has no type hint"
                            ),
                            severity: Severity::Warning,
                            line: child.start_position().row,
                            column: child.start_position().column,
                            fix: None,
                            end_column: None,
                            context_lines: None,
                        });
                    }
                    // default_parameter (untyped with default) also has no type
                    if child.kind() == "default_parameter" {
                        // First child is the identifier name
                        if let Some(name_node) = child.child(0)
                            && name_node.kind() == "identifier"
                        {
                            let param_name = &source[name_node.byte_range()];
                            diags.push(LintDiagnostic {
                                rule: "missing-type-hint",
                                message: format!(
                                    "parameter `{param_name}` in function `{func_name}` has no type hint"
                                ),
                                severity: Severity::Warning,
                                line: name_node.start_position().row,
                                column: name_node.start_position().column,
                                fix: None,
                                end_column: None,
                                context_lines: None,
                            });
                        }
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }

        // Check for missing return type
        if node.child_by_field_name("return_type").is_none() {
            let name_node = node.child_by_field_name("name");
            if let Some(name_node) = name_node {
                diags.push(LintDiagnostic {
                    rule: "missing-type-hint",
                    message: format!("function `{func_name}` has no return type hint"),
                    severity: Severity::Warning,
                    line: name_node.start_position().row,
                    column: name_node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }
        }
    }

    // Recurse
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
