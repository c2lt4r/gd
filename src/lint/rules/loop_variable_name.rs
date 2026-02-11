use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct LoopVariableName;

impl LintRule for LoopVariableName {
    fn name(&self) -> &'static str {
        "loop-variable-name"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "for_statement"
        && let Some(iter_node) = node.child_by_field_name("left")
        && iter_node.kind() == "identifier"
    {
        let name = &source[iter_node.byte_range()];
        if !is_snake_case(name) {
            let fixed = to_snake_case(name);
            diags.push(LintDiagnostic {
                rule: "loop-variable-name",
                message: format!(
                    "loop variable `{}` should use snake_case: `{}`",
                    name, fixed
                ),
                severity: Severity::Warning,
                line: iter_node.start_position().row,
                column: iter_node.start_position().column,
                end_column: Some(iter_node.end_position().column),
                fix: Some(Fix {
                    byte_start: iter_node.start_byte(),
                    byte_end: iter_node.end_byte(),
                    replacement: fixed,
                }),
            });
        }
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

/// Check if a name is valid snake_case.
/// Allows leading underscores (e.g. `_item`, `__internal`).
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
        LoopVariableName.check(&tree, source, &config)
    }

    // ── Valid snake_case loop variables ────────────────────────────────

    #[test]
    fn snake_case_ok() {
        let source = "func foo():\n\tfor item in items:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn single_char_ok() {
        let source = "func foo():\n\tfor i in range(10):\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn underscore_prefix_ok() {
        let source = "func foo():\n\tfor _item in items:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn snake_case_with_digits_ok() {
        let source = "func foo():\n\tfor node2d in nodes:\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn underscore_only_ok() {
        let source = "func foo():\n\tfor _ in range(5):\n\t\tpass\n";
        assert!(check(source).is_empty());
    }

    // ── Invalid loop variable names ───────────────────────────────────

    #[test]
    fn warns_on_camel_case() {
        let source = "func foo():\n\tfor myItem in items:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("snake_case"));
        assert!(diags[0].message.contains("my_item"));
    }

    #[test]
    fn warns_on_pascal_case() {
        let source = "func foo():\n\tfor MyItem in items:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("my_item"));
    }

    #[test]
    fn warns_on_upper_snake_case() {
        let source = "func foo():\n\tfor MAX_VAL in values:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Fix correctness ───────────────────────────────────────────────

    #[test]
    fn fix_camel_to_snake() {
        let source = "func foo():\n\tfor myItem in items:\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "my_item");
    }

    #[test]
    fn fix_pascal_to_snake() {
        let source = "func foo():\n\tfor MyValue in values:\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "my_value");
    }

    #[test]
    fn fix_preserves_underscore_prefix() {
        let source = "func foo():\n\tfor _BadName in items:\n\t\tpass\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(fix.replacement, "_bad_name");
    }

    // ── Multiple loops ────────────────────────────────────────────────

    #[test]
    fn checks_all_loops() {
        let source = "\
func foo():
\tfor badOne in a:
\t\tpass
\tfor badTwo in b:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn mixed_good_and_bad() {
        let source = "\
func foo():
\tfor good_item in a:
\t\tpass
\tfor badItem in b:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("badItem"));
    }

    // ── Nested loops ──────────────────────────────────────────────────

    #[test]
    fn checks_nested_loops() {
        let source = "\
func foo():
\tfor outerBad in a:
\t\tfor innerBad in b:
\t\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    // ── Inner class ───────────────────────────────────────────────────

    #[test]
    fn checks_loops_in_inner_class() {
        let source = "class Inner:\n\tfunc foo():\n\t\tfor badName in items:\n\t\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Span correctness ──────────────────────────────────────────────

    #[test]
    fn diagnostic_points_to_variable() {
        let source = "func foo():\n\tfor badName in items:\n\t\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].column, 5); // "badName" starts after "for "
    }

    // ── Default enabled ───────────────────────────────────────────────

    #[test]
    fn is_default_enabled() {
        assert!(LoopVariableName.default_enabled());
    }
}
