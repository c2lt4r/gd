use gd_core::gd_ast::{self, GdDecl, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct NamingConvention;

impl LintRule for NamingConvention {
    fn name(&self) -> &'static str {
        "naming-convention"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // class_name check
        if let Some(class_name) = file.class_name
            && !is_pascal_case(class_name)
        {
            let fixed = to_pascal_case(class_name);
            let (line, col, end_col, fix) = name_fix(file.class_name_node, file.node, &fixed);
            diags.push(LintDiagnostic {
                rule: "naming-convention",
                message: format!("class_name `{class_name}` should use PascalCase: `{fixed}`"),
                severity: Severity::Warning,
                line,
                column: col,
                end_column: end_col,
                fix,
                context_lines: None,
            });
        }

        // Declarations: functions, vars, consts, classes
        gd_ast::visit_decls(file, &mut |decl| {
            check_decl(decl, &mut diags);
        });

        // Local variables inside function bodies
        gd_ast::visit_stmts(file, &mut |stmt| {
            check_local_var(stmt, &mut diags);
        });

        diags
    }
}

fn check_decl(decl: &GdDecl<'_>, diags: &mut Vec<LintDiagnostic>) {
    match decl {
        GdDecl::Func(func) => {
            if !is_godot_builtin(func.name) && !is_snake_case(func.name) {
                let fixed = to_snake_case(func.name);
                let (line, col, end_col, fix) = name_fix(func.name_node, func.node, &fixed);
                diags.push(LintDiagnostic {
                    rule: "naming-convention",
                    message: format!("function `{}` should use snake_case: `{fixed}`", func.name),
                    severity: Severity::Warning,
                    line,
                    column: col,
                    end_column: end_col,
                    fix,
                    context_lines: None,
                });
            }
        }
        GdDecl::Var(var) => {
            if var.is_const {
                if !is_upper_snake_case(var.name) {
                    let fixed = to_upper_snake_case(var.name);
                    let (line, col, end_col, fix) = name_fix(var.name_node, var.node, &fixed);
                    diags.push(LintDiagnostic {
                        rule: "naming-convention",
                        message: format!(
                            "constant `{}` should use UPPER_SNAKE_CASE: `{fixed}`",
                            var.name
                        ),
                        severity: Severity::Warning,
                        line,
                        column: col,
                        end_column: end_col,
                        fix,
                        context_lines: None,
                    });
                }
            } else if !is_snake_case(var.name) {
                let fixed = to_snake_case(var.name);
                let (line, col, end_col, fix) = name_fix(var.name_node, var.node, &fixed);
                diags.push(LintDiagnostic {
                    rule: "naming-convention",
                    message: format!("variable `{}` should use snake_case: `{fixed}`", var.name),
                    severity: Severity::Warning,
                    line,
                    column: col,
                    end_column: end_col,
                    fix,
                    context_lines: None,
                });
            }
        }
        GdDecl::Class(class) => {
            if !is_pascal_case(class.name) {
                let fixed = to_pascal_case(class.name);
                let (line, col, end_col, fix) = name_fix(class.name_node, class.node, &fixed);
                diags.push(LintDiagnostic {
                    rule: "naming-convention",
                    message: format!("class `{}` should use PascalCase: `{fixed}`", class.name),
                    severity: Severity::Warning,
                    line,
                    column: col,
                    end_column: end_col,
                    fix,
                    context_lines: None,
                });
            }
        }
        _ => {}
    }
}

fn check_local_var(stmt: &GdStmt<'_>, diags: &mut Vec<LintDiagnostic>) {
    let GdStmt::Var(var) = stmt else { return };
    if var.is_const {
        if !is_upper_snake_case(var.name) {
            let fixed = to_upper_snake_case(var.name);
            let (line, col, end_col, fix) = name_fix(var.name_node, var.node, &fixed);
            diags.push(LintDiagnostic {
                rule: "naming-convention",
                message: format!(
                    "constant `{}` should use UPPER_SNAKE_CASE: `{fixed}`",
                    var.name
                ),
                severity: Severity::Warning,
                line,
                column: col,
                end_column: end_col,
                fix,
                context_lines: None,
            });
        }
    } else if !is_snake_case(var.name) {
        let fixed = to_snake_case(var.name);
        let (line, col, end_col, fix) = name_fix(var.name_node, var.node, &fixed);
        diags.push(LintDiagnostic {
            rule: "naming-convention",
            message: format!("variable `{}` should use snake_case: `{fixed}`", var.name),
            severity: Severity::Warning,
            line,
            column: col,
            end_column: end_col,
            fix,
            context_lines: None,
        });
    }
}

/// Build position + fix from an optional name node, falling back to the decl node.
fn name_fix(
    name_node: Option<tree_sitter::Node<'_>>,
    decl_node: tree_sitter::Node<'_>,
    replacement: &str,
) -> (usize, usize, Option<usize>, Option<Fix>) {
    match name_node {
        Some(n) => (
            n.start_position().row,
            n.start_position().column,
            Some(n.end_position().column),
            Some(Fix {
                byte_start: n.start_byte(),
                byte_end: n.end_byte(),
                replacement: replacement.to_string(),
            }),
        ),
        None => (
            decl_node.start_position().row,
            decl_node.start_position().column,
            None,
            None,
        ),
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
/// Allows leading underscores for private constants (e.g. `_MAX_HP`).
fn is_upper_snake_case(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    let trimmed = name.trim_start_matches('_');
    if trimmed.is_empty() {
        return true;
    }
    trimmed
        .chars()
        .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && !trimmed.contains("__")
        && !trimmed.ends_with('_')
}

/// Convert a name to UPPER_SNAKE_CASE.
/// Preserves leading underscores for private constants.
fn to_upper_snake_case(name: &str) -> String {
    let prefix_underscores: String = name.chars().take_while(|&c| c == '_').collect();
    let rest = &name[prefix_underscores.len()..];

    let mut result = prefix_underscores;
    let mut prev_was_upper = false;
    for (i, ch) in rest.chars().enumerate() {
        if ch == '_' {
            result.push('_');
            prev_was_upper = false;
        } else if ch.is_ascii_uppercase() {
            if i > 0 && !prev_was_upper && rest.as_bytes()[i - 1] != b'_' {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upper_snake_case_valid() {
        assert!(is_upper_snake_case("MAX_SPEED"));
        assert!(is_upper_snake_case("X"));
        assert!(is_upper_snake_case("A1"));
    }

    #[test]
    fn upper_snake_case_leading_underscore() {
        assert!(is_upper_snake_case("_DIALOG_BOX_SCRIPT"));
        assert!(is_upper_snake_case("_MAX_HP"));
        assert!(is_upper_snake_case("_X"));
    }

    #[test]
    fn upper_snake_case_invalid() {
        assert!(!is_upper_snake_case("maxSpeed"));
        assert!(!is_upper_snake_case("MAX__SPEED"));
        assert!(!is_upper_snake_case("MAX_SPEED_"));
    }

    #[test]
    fn to_upper_snake_preserves_leading_underscore() {
        assert_eq!(
            to_upper_snake_case("_dialogBoxScript"),
            "_DIALOG_BOX_SCRIPT"
        );
    }

    #[test]
    fn snake_case_allows_leading_underscore() {
        assert!(is_snake_case("_ready"));
        assert!(is_snake_case("__init"));
        assert!(is_snake_case("my_var"));
    }
}
