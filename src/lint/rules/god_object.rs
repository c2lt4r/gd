use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct GodObject;

impl LintRule for GodObject {
    fn name(&self) -> &'static str {
        "god-object"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let max_functions = config.max_god_object_functions;
        let max_members = config.max_god_object_members;
        let max_lines = config.max_god_object_lines;

        let mut diags = Vec::new();
        let root = tree.root_node();

        // Check the top-level script as a class
        let (funcs, members) = count_definitions(root, source);
        let lines = source.lines().count();
        let mut reasons = Vec::new();

        if funcs > max_functions {
            reasons.push(format!("{} functions (max {})", funcs, max_functions));
        }
        if members > max_members {
            reasons.push(format!(
                "{} member variables (max {})",
                members, max_members
            ));
        }
        if lines > max_lines {
            reasons.push(format!("{} lines (max {})", lines, max_lines));
        }

        if !reasons.is_empty() {
            diags.push(LintDiagnostic {
                rule: "god-object",
                message: format!("script is too large: {}", reasons.join(", ")),
                severity: Severity::Warning,
                line: 0,
                column: 0,
                end_column: None,
                fix: None,
            });
        }

        // Check inner classes
        check_classes(
            root,
            source,
            max_functions,
            max_members,
            max_lines,
            &mut diags,
        );

        diags
    }
}

/// Count direct child function definitions and variable statements.
fn count_definitions(scope: Node, source: &str) -> (usize, usize) {
    let mut functions = 0;
    let mut members = 0;
    let mut cursor = scope.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match child.kind() {
                "function_definition" | "constructor_definition" => functions += 1,
                "variable_statement" => members += 1,
                // Count enum members as part of the class complexity
                "enum_definition" => {
                    if let Some(body) = child.child_by_field_name("body") {
                        members += body
                            .children(&mut body.walk())
                            .filter(|c| c.kind() == "enumerator")
                            .count();
                    }
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    // Ignore source in signature to avoid warning
    let _ = source;
    (functions, members)
}

/// Recursively check inner classes for god-object violations.
fn check_classes(
    node: Node,
    source: &str,
    max_functions: usize,
    max_members: usize,
    max_lines: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "class_definition" {
                let class_name = child
                    .child_by_field_name("name")
                    .map(|n| &source[n.byte_range()])
                    .unwrap_or("<unknown>");

                if let Some(body) = child.child_by_field_name("body") {
                    let (funcs, members) = count_definitions(body, source);
                    let class_lines = child.end_position().row - child.start_position().row + 1;
                    let mut reasons = Vec::new();

                    if funcs > max_functions {
                        reasons.push(format!("{} functions (max {})", funcs, max_functions));
                    }
                    if members > max_members {
                        reasons.push(format!(
                            "{} member variables (max {})",
                            members, max_members
                        ));
                    }
                    if class_lines > max_lines {
                        reasons.push(format!("{} lines (max {})", class_lines, max_lines));
                    }

                    if !reasons.is_empty() {
                        diags.push(LintDiagnostic {
                            rule: "god-object",
                            message: format!(
                                "class `{}` is too large: {}",
                                class_name,
                                reasons.join(", ")
                            ),
                            severity: Severity::Warning,
                            line: child.start_position().row,
                            column: child.start_position().column,
                            end_column: None,
                            fix: None,
                        });
                    }

                    // Recurse for nested classes
                    check_classes(body, source, max_functions, max_members, max_lines, diags);
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

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        GodObject.check(&tree, source, &config)
    }

    fn make_functions(count: usize) -> String {
        let mut s = String::new();
        for i in 0..count {
            s.push_str(&format!("func f_{}():\n\tpass\n\n", i));
        }
        s
    }

    fn make_members(count: usize) -> String {
        let mut s = String::new();
        for i in 0..count {
            s.push_str(&format!("var m_{}: int\n", i));
        }
        s
    }

    #[test]
    fn no_warning_under_limits() {
        let source = make_functions(5);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn warns_too_many_functions() {
        let source = make_functions(21);
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("functions"));
    }

    #[test]
    fn warns_too_many_members() {
        let source = make_members(16);
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("member variables"));
    }

    #[test]
    fn warns_multiple_reasons() {
        let mut source = make_functions(21);
        source.push_str(&make_members(16));
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("functions"));
        assert!(diags[0].message.contains("member variables"));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!GodObject.default_enabled());
    }

    #[test]
    fn empty_file() {
        assert!(check("").is_empty());
    }
}
