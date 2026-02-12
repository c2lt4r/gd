use tree_sitter::Tree;

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NamingConvention;

impl LintRule for NamingConvention {
    fn name(&self) -> &'static str {
        "naming-convention"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let mut cursor = root.walk();

        check_node(root, source, &mut diags, &mut cursor);

        diags
    }
}

fn check_node(
    node: tree_sitter::Node,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
    cursor: &mut tree_sitter::TreeCursor,
) {
    match node.kind() {
        "function_definition" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                // Skip Godot built-in overridden methods
                if !is_godot_builtin(name) && !is_snake_case(name) {
                    let fixed = to_snake_case(name);
                    diags.push(LintDiagnostic {
                        rule: "naming-convention",
                        message: format!("function `{}` should use snake_case: `{}`", name, fixed),
                        severity: Severity::Warning,
                        line: name_node.start_position().row,
                        column: name_node.start_position().column,
                        end_column: Some(name_node.end_position().column),
                        fix: Some(Fix {
                            byte_start: name_node.start_byte(),
                            byte_end: name_node.end_byte(),
                            replacement: fixed,
                        }),
                        context_lines: None,
                    });
                }
            }
        }
        "const_statement" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                if !is_upper_snake_case(name) {
                    let fixed = to_upper_snake_case(name);
                    diags.push(LintDiagnostic {
                        rule: "naming-convention",
                        message: format!(
                            "constant `{}` should use UPPER_SNAKE_CASE: `{}`",
                            name, fixed
                        ),
                        severity: Severity::Warning,
                        line: name_node.start_position().row,
                        column: name_node.start_position().column,
                        end_column: Some(name_node.end_position().column),
                        fix: Some(Fix {
                            byte_start: name_node.start_byte(),
                            byte_end: name_node.end_byte(),
                            replacement: fixed,
                        }),
                        context_lines: None,
                    });
                }
            }
        }
        "variable_statement" => {
            // Only check local variables (inside a body) and class-level vars
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                if !is_snake_case(name) {
                    let fixed = to_snake_case(name);
                    diags.push(LintDiagnostic {
                        rule: "naming-convention",
                        message: format!("variable `{}` should use snake_case: `{}`", name, fixed),
                        severity: Severity::Warning,
                        line: name_node.start_position().row,
                        column: name_node.start_position().column,
                        end_column: Some(name_node.end_position().column),
                        fix: Some(Fix {
                            byte_start: name_node.start_byte(),
                            byte_end: name_node.end_byte(),
                            replacement: fixed,
                        }),
                        context_lines: None,
                    });
                }
            }
        }
        "class_name_statement" => {
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                if !is_pascal_case(name) {
                    let fixed = to_pascal_case(name);
                    diags.push(LintDiagnostic {
                        rule: "naming-convention",
                        message: format!(
                            "class_name `{}` should use PascalCase: `{}`",
                            name, fixed
                        ),
                        severity: Severity::Warning,
                        line: name_node.start_position().row,
                        column: name_node.start_position().column,
                        end_column: Some(name_node.end_position().column),
                        fix: Some(Fix {
                            byte_start: name_node.start_byte(),
                            byte_end: name_node.end_byte(),
                            replacement: fixed,
                        }),
                        context_lines: None,
                    });
                }
            }
        }
        "class_definition" => {
            // Inner class: `class Foo:` - the name child
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = &source[name_node.byte_range()];
                if !is_pascal_case(name) {
                    let fixed = to_pascal_case(name);
                    diags.push(LintDiagnostic {
                        rule: "naming-convention",
                        message: format!("class `{}` should use PascalCase: `{}`", name, fixed),
                        severity: Severity::Warning,
                        line: name_node.start_position().row,
                        column: name_node.start_position().column,
                        end_column: Some(name_node.end_position().column),
                        fix: Some(Fix {
                            byte_start: name_node.start_byte(),
                            byte_end: name_node.end_byte(),
                            replacement: fixed,
                        }),
                        context_lines: None,
                    });
                }
            }
        }
        _ => {}
    }

    // Recurse into children
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            // Use a temporary cursor for the subtree
            let mut child_cursor = child.walk();
            check_node(child, source, diags, &mut child_cursor);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Godot built-in methods that are commonly overridden and use _prefix naming.
const GODOT_BUILTINS: &[&str] = &[
    "_ready",
    "_process",
    "_physics_process",
    "_input",
    "_unhandled_input",
    "_enter_tree",
    "_exit_tree",
    "_draw",
    "_notification",
    "_to_string",
    "_init",
    "_get",
    "_set",
    "_get_property_list",
];

fn is_godot_builtin(name: &str) -> bool {
    GODOT_BUILTINS.contains(&name)
}

/// Check if a name is valid UPPER_SNAKE_CASE.
fn is_upper_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    name.chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !name.contains("__")
        && !name.starts_with('_')
        && !name.ends_with('_')
}

/// Convert a name to UPPER_SNAKE_CASE.
fn to_upper_snake_case(name: &str) -> String {
    let mut result = String::new();
    let mut prev_was_upper = false;
    for (i, ch) in name.chars().enumerate() {
        if ch == '_' {
            result.push('_');
            prev_was_upper = false;
        } else if ch.is_ascii_uppercase() {
            if i > 0 && !prev_was_upper && name.as_bytes()[i - 1] != b'_' {
                result.push('_');
            }
            result.push(ch);
            prev_was_upper = true;
        } else {
            prev_was_upper = false;
            result.push(ch.to_ascii_uppercase());
        }
    }
    result
}

/// Check if a name is valid snake_case.
/// Allows leading underscores (e.g. `_ready`, `__init`).
fn is_snake_case(name: &str) -> bool {
    let trimmed = name.trim_start_matches('_');
    if trimmed.is_empty() {
        return true; // `_` or `__` are fine
    }
    // Must be lowercase alphanumeric + underscores, no consecutive underscores in the body
    trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !trimmed.contains("__")
}

/// Check if a name is valid PascalCase.
fn is_pascal_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let first = name.chars().next().unwrap();
    if !first.is_ascii_uppercase() {
        return false;
    }
    // No underscores allowed in PascalCase
    !name.contains('_') && name.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Convert a name to snake_case.
fn to_snake_case(name: &str) -> String {
    let prefix_underscores: String = name.chars().take_while(|&c| c == '_').collect();
    let rest = &name[prefix_underscores.len()..];

    let mut result = prefix_underscores;
    let mut prev_was_upper = false;
    for (i, ch) in rest.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 && !prev_was_upper {
                result.push('_');
            }
            result.push(ch.to_ascii_lowercase());
            prev_was_upper = true;
        } else {
            prev_was_upper = false;
            result.push(ch);
        }
    }
    result
}

/// Convert a name to PascalCase.
fn to_pascal_case(name: &str) -> String {
    let mut result = String::new();
    let mut capitalize_next = true;
    for ch in name.chars() {
        if ch == '_' {
            capitalize_next = true;
        } else if capitalize_next {
            result.push(ch.to_ascii_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}
