use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MaxPublicMethods;

impl LintRule for MaxPublicMethods {
    fn name(&self) -> &'static str {
        "max-public-methods"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let max_methods = config.max_public_methods;
        let mut diags = Vec::new();
        let root = tree.root_node();

        // Check top-level scope (the script itself acts as a class)
        let top_level_count = count_public_methods(root, source);
        if top_level_count > max_methods {
            diags.push(LintDiagnostic {
                rule: "max-public-methods",
                message: format!(
                    "script has {} public methods (max {})",
                    top_level_count, max_methods
                ),
                severity: Severity::Warning,
                line: 0,
                column: 0,
                fix: None,
                end_column: None,
            });
        }

        // Check inner class_definition nodes
        check_classes(root, source, max_methods, &mut diags);

        diags
    }
}

/// Count direct child function_definition nodes whose name doesn't start with "_".
fn count_public_methods(node: Node, source: &str) -> usize {
    let mut count = 0;
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "function_definition"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = &source[name_node.byte_range()];
                if !name.starts_with('_') {
                    count += 1;
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    count
}

/// Recursively find class_definition nodes and check their public method count.
fn check_classes(node: Node, source: &str, max_methods: usize, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "class_definition" {
                let class_name = child
                    .child_by_field_name("name")
                    .map(|n| &source[n.byte_range()])
                    .unwrap_or("<unknown>");

                // Count public methods in the class body
                if let Some(body) = child.child_by_field_name("body") {
                    let count = count_public_methods(body, source);
                    if count > max_methods {
                        diags.push(LintDiagnostic {
                            rule: "max-public-methods",
                            message: format!(
                                "class `{}` has {} public methods (max {})",
                                class_name, count, max_methods
                            ),
                            severity: Severity::Warning,
                            line: child.start_position().row,
                            column: child.start_position().column,
                            fix: None,
                            end_column: None,
                        });
                    }
                }

                // Check nested classes
                if let Some(body) = child.child_by_field_name("body") {
                    check_classes(body, source, max_methods, diags);
                }
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

    const DEFAULT_MAX_PUBLIC_METHODS: usize = 20;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        MaxPublicMethods.check(&tree, source, &config)
    }

    fn make_methods(public: usize, private: usize) -> String {
        let mut s = String::new();
        for i in 0..public {
            s.push_str(&format!("func method_{}():\n\tpass\n\n", i));
        }
        for i in 0..private {
            s.push_str(&format!("func _private_{}():\n\tpass\n\n", i));
        }
        s
    }

    fn make_class_methods(class_name: &str, public: usize, private: usize) -> String {
        let mut s = format!("class {class_name}:\n");
        for i in 0..public {
            s.push_str(&format!("\tfunc method_{}():\n\t\tpass\n\n", i));
        }
        for i in 0..private {
            s.push_str(&format!("\tfunc _private_{}():\n\t\tpass\n\n", i));
        }
        s
    }

    #[test]
    fn no_warning_under_limit() {
        let source = make_methods(5, 3);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn no_warning_at_limit() {
        let source = make_methods(DEFAULT_MAX_PUBLIC_METHODS, 10);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn warns_over_limit() {
        let source = make_methods(DEFAULT_MAX_PUBLIC_METHODS + 1, 0);
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "max-public-methods");
        assert_eq!(diags[0].line, 0);
        assert!(diags[0].message.contains("script"));
        assert!(
            diags[0]
                .message
                .contains(&(DEFAULT_MAX_PUBLIC_METHODS + 1).to_string())
        );
    }

    #[test]
    fn private_methods_not_counted() {
        let source = make_methods(5, 30);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn inner_class_warns() {
        let source = make_class_methods("MyClass", DEFAULT_MAX_PUBLIC_METHODS + 1, 0);
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("MyClass"));
        assert!(
            diags[0]
                .message
                .contains(&(DEFAULT_MAX_PUBLIC_METHODS + 1).to_string())
        );
    }

    #[test]
    fn inner_class_under_limit() {
        let source = make_class_methods("MyClass", DEFAULT_MAX_PUBLIC_METHODS, 5);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn both_script_and_class_warn() {
        let mut source = make_methods(DEFAULT_MAX_PUBLIC_METHODS + 1, 0);
        source.push_str(&make_class_methods(
            "BigClass",
            DEFAULT_MAX_PUBLIC_METHODS + 2,
            0,
        ));
        let diags = check(&source);
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("script"));
        assert!(diags[1].message.contains("BigClass"));
    }

    #[test]
    fn empty_file() {
        assert!(check("").is_empty());
    }

    #[test]
    fn only_private_methods() {
        let source = make_methods(0, 25);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!MaxPublicMethods.default_enabled());
    }

    #[test]
    fn severity_is_warning() {
        let source = make_methods(DEFAULT_MAX_PUBLIC_METHODS + 1, 0);
        let diags = check(&source);
        assert_eq!(diags[0].severity, Severity::Warning);
    }

    #[test]
    fn mixed_public_private_at_boundary() {
        // Exactly at limit with many private — should not warn
        let source = make_methods(DEFAULT_MAX_PUBLIC_METHODS, 20);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn class_reports_correct_line() {
        let mut source = String::from("var x = 1\n\n");
        source.push_str(&make_class_methods(
            "Late",
            DEFAULT_MAX_PUBLIC_METHODS + 1,
            0,
        ));
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 2); // class starts on line 2
    }
}
