use crate::core::gd_ast::{self, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct LoopVariableName;

impl LintRule for LoopVariableName {
    fn name(&self) -> &'static str {
        "loop-variable-name"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::For { node, var, var_node, .. } = stmt
                && !var.is_empty()
                && !is_snake_case(var)
            {
                let fixed = to_snake_case(var);
                let fix = var_node.map(|vn| Fix {
                    byte_start: vn.start_byte(),
                    byte_end: vn.end_byte(),
                    replacement: fixed.clone(),
                });
                let (line, col, end_col) = var_node.map_or(
                    (node.start_position().row, node.start_position().column, None),
                    |vn| (vn.start_position().row, vn.start_position().column, Some(vn.end_position().column)),
                );

                diags.push(LintDiagnostic {
                    rule: "loop-variable-name",
                    message: format!("loop variable `{var}` should use snake_case: `{fixed}`"),
                    severity: Severity::Warning,
                    line,
                    column: col,
                    end_column: end_col,
                    fix,
                    context_lines: None,
                });
            }
        });
        diags
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
    use crate::core::gd_ast;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        LoopVariableName.check(&file, source, &config)
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
