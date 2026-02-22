use std::collections::HashMap;
use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateVariable;

impl LintRule for DuplicateVariable {
    fn name(&self) -> &'static str {
        "duplicate-variable"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_scope(root, source, &mut diags);
        diags
    }
}

/// Check a single scope (top-level or class body) for duplicate variable names.
fn check_scope(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut variables: HashMap<String, usize> = HashMap::new();

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "variable_statement"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = source[name_node.byte_range()].to_string();
                let line = name_node.start_position().row;

                if let Some(&first_line) = variables.get(&name) {
                    diags.push(LintDiagnostic {
                        rule: "duplicate-variable",
                        message: format!(
                            "variable `{}` already declared on line {}",
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
                    variables.insert(name, line);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        DuplicateVariable.check(&tree, source, &config)
    }

    #[test]
    fn detects_duplicate_class_variable() {
        let source = "extends Control\nvar _ship_index: int = 0\nvar _name: Label\nvar _ship_index: int = 0\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_ship_index"));
        assert!(diags[0].message.contains("line 2"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn detects_multiple_duplicates() {
        let source = "var a: int\nvar b: int\nvar a: int\nvar b: int\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn no_warning_unique_variables() {
        let source = "extends Node\nvar health: int\nvar speed: float\nvar name: String\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_different_scopes() {
        let source = "var x: int\nclass Inner:\n\tvar x: int\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_duplicate_in_inner_class() {
        let source = "class Inner:\n\tvar x: int\n\tvar x: int\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn default_enabled() {
        assert!(DuplicateVariable.default_enabled());
    }
}
