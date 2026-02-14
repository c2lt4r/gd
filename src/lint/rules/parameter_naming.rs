use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ParameterNaming;

impl LintRule for ParameterNaming {
    fn name(&self) -> &'static str {
        "parameter-naming"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition"
        && let Some(params) = node.child_by_field_name("parameters")
    {
        check_parameters(params, source, diags);
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

fn check_parameters(params_node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = params_node.walk();
    if !cursor.goto_first_child() {
        return;
    }
    loop {
        let child = cursor.node();
        match child.kind() {
            // Plain parameter: `func foo(myParam):`
            "identifier" => {
                check_param_name(child, source, diags);
            }
            // Typed: `func foo(myParam: int):`
            // Default: `func foo(myParam = 5):`
            // Typed default: `func foo(myParam: int = 5):`
            "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                if let Some(name_node) = child.child(0)
                    && name_node.kind() == "identifier"
                {
                    check_param_name(name_node, source, diags);
                }
            }
            _ => {}
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn check_param_name(name_node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let name = &source[name_node.byte_range()];
    // Allow leading underscores for unused params
    if !is_snake_case(name) {
        let fixed = to_snake_case(name);
        diags.push(LintDiagnostic {
            rule: "parameter-naming",
            message: format!("parameter `{name}` should use snake_case: `{fixed}`"),
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

/// Check if a name is valid snake_case.
/// Allows leading underscores (e.g. `_delta`, `__internal`).
fn is_snake_case(name: &str) -> bool {
    let trimmed = name.trim_start_matches('_');
    if trimmed.is_empty() {
        return true; // `_` or `__` are fine
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && !trimmed.contains("__")
}

/// Convert a name to snake_case, preserving leading underscores.
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        ParameterNaming.check(&tree, source, &config)
    }

    // ── Valid snake_case parameters ─────────────────────────────────

    #[test]
    fn snake_case_params_ok() {
        let source = "func move(speed: float, direction: Vector2):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn single_word_param_ok() {
        let source = "func jump(height):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn underscore_prefix_ok() {
        let source = "func _ready(_delta: float):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn plain_underscore_ok() {
        let source = "func foo(_):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn param_with_digits_ok() {
        let source = "func foo(point2d: Vector2):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    // ── Invalid parameter names ─────────────────────────────────────

    #[test]
    fn warns_on_camel_case() {
        let source = "func move(moveSpeed: float):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("snake_case"));
        assert!(diags[0].message.contains("move_speed"));
    }

    #[test]
    fn warns_on_pascal_case() {
        let source = "func foo(MyParam: int):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("my_param"));
    }

    #[test]
    fn warns_on_upper_snake_case() {
        let source = "func foo(MAX_SPEED: float):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Multiple parameters ─────────────────────────────────────────

    #[test]
    fn warns_on_multiple_bad_params() {
        let source = "func foo(myParam: int, anotherBad: float):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn mixed_good_and_bad() {
        let source = "func foo(good_param: int, badParam: float):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("badParam"));
    }

    // ── Default parameters ──────────────────────────────────────────

    #[test]
    fn default_param_ok() {
        let source = "func foo(speed = 10):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_bad_default_param() {
        let source = "func foo(moveSpeed = 10):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("move_speed"));
    }

    // ── Typed default parameters ────────────────────────────────────

    #[test]
    fn typed_default_param_ok() {
        let source = "func foo(speed: float = 1.0):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_bad_typed_default_param() {
        let source = "func foo(moveSpeed: float = 1.0):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("move_speed"));
    }

    // ── Fix correctness ─────────────────────────────────────────────

    #[test]
    fn fix_camel_to_snake() {
        let source = "func foo(moveSpeed: float):\n\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "move_speed");
    }

    #[test]
    fn fix_pascal_to_snake() {
        let source = "func foo(MyValue: int):\n\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "my_value");
    }

    #[test]
    fn fix_preserves_underscore_prefix() {
        let source = "func foo(_BadName: int):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "_bad_name");
    }

    // ── Nested functions / inner classes ─────────────────────────────

    #[test]
    fn checks_params_in_inner_class() {
        let source = "class Inner:\n\tfunc foo(badParam: int):\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Multiple functions ──────────────────────────────────────────

    #[test]
    fn checks_all_functions() {
        let source = "func a(goodOne: int):\n\tpass\n\nfunc b(badParam: float):\n\tpass\n";
        let diags = check(source);
        // goodOne -> good_one, badParam -> bad_param
        assert_eq!(diags.len(), 2);
    }

    // ── No parameters ───────────────────────────────────────────────

    #[test]
    fn no_params_ok() {
        let source = "func ready():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    // ── Span correctness ────────────────────────────────────────────

    #[test]
    fn diagnostic_points_to_param_name() {
        let source = "func foo(badName: int):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 0);
        assert_eq!(diags[0].column, 9); // "badName" starts at col 9
        assert_eq!(diags[0].end_column, Some(16)); // ends at col 16
    }
}
