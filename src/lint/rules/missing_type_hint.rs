use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct MissingTypeHint;

impl LintRule for MissingTypeHint {
    fn name(&self) -> &'static str {
        "missing-type-hint"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
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
                                "parameter `{}` in function `{}` has no type hint",
                                param_name, func_name
                            ),
                            severity: Severity::Warning,
                            line: child.start_position().row,
                            column: child.start_position().column,
                            fix: None,
                    end_column: None,
                        });
                    }
                    // default_parameter (untyped with default) also has no type
                    if child.kind() == "default_parameter" {
                        // First child is the identifier name
                        if let Some(name_node) = child.child(0)
                            && name_node.kind() == "identifier" {
                                let param_name = &source[name_node.byte_range()];
                                diags.push(LintDiagnostic {
                                    rule: "missing-type-hint",
                                    message: format!(
                                        "parameter `{}` in function `{}` has no type hint",
                                        param_name, func_name
                                    ),
                                    severity: Severity::Warning,
                                    line: name_node.start_position().row,
                                    column: name_node.start_position().column,
                                    fix: None,
                    end_column: None,
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
                    message: format!(
                        "function `{}` has no return type hint",
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
