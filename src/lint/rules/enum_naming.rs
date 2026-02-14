use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct EnumNaming;

impl LintRule for EnumNaming {
    fn name(&self) -> &'static str {
        "enum-naming"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "enum_definition" {
        // Check enum name is PascalCase
        if let Some(name_node) = node.child_by_field_name("name") {
            let name = &source[name_node.byte_range()];
            if !is_pascal_case(name) {
                let fixed = to_pascal_case(name);
                diags.push(LintDiagnostic {
                    rule: "enum-naming",
                    message: format!("enum `{name}` should use PascalCase: `{fixed}`"),
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

        // Check enum values are UPPER_SNAKE_CASE
        check_enum_values(node, source, diags);
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

fn check_enum_values(enum_node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = enum_node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        if child.kind() == "enumerator_list" {
            let mut list_cursor = child.walk();
            if list_cursor.goto_first_child() {
                loop {
                    let item = list_cursor.node();
                    if item.kind() == "enumerator" {
                        // First child of enumerator is the name identifier
                        if let Some(name_node) = item.child(0) {
                            let name = &source[name_node.byte_range()];
                            if !is_upper_snake_case(name) {
                                let fixed = to_upper_snake_case(name);
                                diags.push(LintDiagnostic {
                                    rule: "enum-naming",
                                    message: format!(
                                        "enum value `{name}` should use UPPER_SNAKE_CASE: `{fixed}`"
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
                    if !list_cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
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
    !name.contains('_') && name.chars().all(|c| c.is_ascii_alphanumeric())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        EnumNaming.check(&tree, source, &config)
    }

    // ── Enum name checks ────────────────────────────────────────────

    #[test]
    fn pascal_case_enum_name_ok() {
        let source = "enum Direction { UP, DOWN, LEFT, RIGHT }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_snake_case_enum_name() {
        let source = "enum my_direction { UP, DOWN }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("PascalCase"));
        assert!(diags[0].message.contains("MyDirection"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn warns_on_lowercase_enum_name() {
        let source = "enum state { IDLE, RUNNING }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("State"));
    }

    // ── Enum value checks ───────────────────────────────────────────

    #[test]
    fn upper_snake_case_values_ok() {
        let source = "enum State { IDLE, RUNNING, GAME_OVER }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_lowercase_value() {
        let source = "enum State { idle, RUNNING }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("UPPER_SNAKE_CASE"));
        assert!(diags[0].message.contains("IDLE"));
    }

    #[test]
    fn warns_on_pascal_case_value() {
        let source = "enum State { Running, Idle }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn warns_on_camel_case_value() {
        let source = "enum Color { lightRed }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("LIGHT_RED"));
    }

    // ── Enum with assigned values ───────────────────────────────────

    #[test]
    fn values_with_assignments_ok() {
        let source = "enum Speed { SLOW = 1, FAST = 10 }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_bad_value_with_assignment() {
        let source = "enum Speed { slow = 1 }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("SLOW"));
    }

    // ── Anonymous enum ──────────────────────────────────────────────

    #[test]
    fn anonymous_enum_values_ok() {
        let source = "enum { UP, DOWN, LEFT, RIGHT }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_anonymous_enum_bad_values() {
        let source = "enum { up, down }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    // ── Combined checks ─────────────────────────────────────────────

    #[test]
    fn warns_on_both_name_and_values() {
        let source = "enum bad_name { also_bad, AND_GOOD }\n";
        let diags = check(source);
        // 1 for enum name + 1 for "also_bad" value
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn multiple_enums() {
        let source = "enum Good { OK }\nenum bad { bad_val }\n";
        let diags = check(source);
        // 1 for "bad" name + 1 for "bad_val" value
        assert_eq!(diags.len(), 2);
    }

    // ── Fix correctness ─────────────────────────────────────────────

    #[test]
    fn fix_replaces_enum_name() {
        let source = "enum my_state { IDLE }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "MyState");
    }

    #[test]
    fn fix_replaces_enum_value() {
        let source = "enum State { gameOver }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "GAME_OVER");
    }

    // ── Nested enum in class ────────────────────────────────────────

    #[test]
    fn enum_inside_class() {
        let source = "class Player:\n\tenum State { IDLE, RUNNING }\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_bad_enum_inside_class() {
        let source = "class Player:\n\tenum state { idle }\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    // ── Values with digits ──────────────────────────────────────────

    #[test]
    fn upper_snake_case_with_digits_ok() {
        let source = "enum Axis { AXIS_X1, AXIS_Y2 }\n";
        assert!(check(source).is_empty());
    }
}
