use std::collections::HashSet;
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ParameterShadowsField;

impl LintRule for ParameterShadowsField {
    fn name(&self) -> &'static str {
        "parameter-shadows-field"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let src = source.as_bytes();

        // Collect top-level instance variable names
        let mut fields: HashSet<String> = HashSet::new();
        collect_fields(root, src, &mut fields);

        if !fields.is_empty() {
            check_functions(root, src, &fields, &mut diags);
        }

        // Check inner classes (they have their own fields)
        check_classes(root, src, &mut diags);

        diags
    }
}

/// Collect direct child `variable_statement` names from a scope.
fn collect_fields(scope: Node, src: &[u8], fields: &mut HashSet<String>) {
    let mut cursor = scope.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "variable_statement"
                && let Some(name_node) = child.child_by_field_name("name")
                && let Ok(name) = name_node.utf8_text(src)
            {
                fields.insert(name.to_string());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check direct child functions for parameter-shadows-field.
fn check_functions(
    scope: Node,
    src: &[u8],
    fields: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = scope.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if (child.kind() == "function_definition" || child.kind() == "constructor_definition")
                && let Some(params) = child.child_by_field_name("parameters")
            {
                check_params(params, src, fields, diags);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check inner classes: each class has its own fields + parent fields.
fn check_classes(node: Node, src: &[u8], diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                let mut class_fields = HashSet::new();
                collect_fields(body, src, &mut class_fields);
                if !class_fields.is_empty() {
                    check_functions(body, src, &class_fields, diags);
                }
                // Recurse for nested classes
                check_classes(body, src, diags);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_params(
    params_node: Node,
    src: &[u8],
    fields: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = params_node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let name_node = match child.kind() {
                "identifier" => Some(child),
                "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                    child.child(0)
                }
                _ => None,
            };
            if let Some(name_node) = name_node
                && let Ok(name) = name_node.utf8_text(src)
                && fields.contains(name)
            {
                diags.push(LintDiagnostic {
                    rule: "parameter-shadows-field",
                    message: format!("parameter `{}` shadows an instance variable", name),
                    severity: Severity::Warning,
                    line: name_node.start_position().row,
                    column: name_node.start_position().column,
                    end_column: Some(name_node.end_position().column),
                    fix: None,
                });
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
        ParameterShadowsField.check(&tree, source, &config)
    }

    #[test]
    fn detects_shadowing() {
        let source =
            "var speed: float = 10.0\n\nfunc set_speed(speed: float) -> void:\n\tspeed = speed\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "parameter-shadows-field");
        assert!(diags[0].message.contains("speed"));
    }

    #[test]
    fn no_warning_different_names() {
        let source = "var speed: float = 10.0\n\nfunc set_speed(new_speed: float) -> void:\n\tspeed = new_speed\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_in_constructor() {
        let source =
            "var health: int\n\nfunc _init(health: int) -> void:\n\tself.health = health\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("health"));
    }

    #[test]
    fn no_warning_without_fields() {
        let source = "func f(x: int) -> void:\n\tprint(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_shadows() {
        let source = "var x: int\nvar y: int\n\nfunc f(x: int, y: int) -> void:\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn inner_class_fields() {
        let source = "class Inner:\n\tvar value: int\n\n\tfunc set_value(value: int) -> void:\n\t\tself.value = value\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("value"));
    }

    #[test]
    fn no_cross_class_warning() {
        // Top-level field should not trigger for inner class functions
        let source =
            "var speed: float\n\nclass Inner:\n\tfunc f(speed: float) -> void:\n\t\tpass\n";
        let diags = check(source);
        // Inner class doesn't have 'speed' as its own field, so no warning
        assert!(diags.is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(ParameterShadowsField.default_enabled());
    }
}
