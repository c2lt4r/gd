use gd_core::gd_ast::{GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct GodObject;

impl LintRule for GodObject {
    fn name(&self) -> &'static str {
        "god-object"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let max_functions = config.max_god_object_functions;
        let max_members = config.max_god_object_members;
        let max_lines = config.max_god_object_lines;

        let mut diags = Vec::new();

        // Check the top-level script as a class
        let (funcs, members) = count_definitions(&file.declarations);
        let lines = source.lines().count();
        let mut reasons = Vec::new();

        if funcs > max_functions {
            reasons.push(format!("{funcs} functions (max {max_functions})"));
        }
        if members > max_members {
            reasons.push(format!("{members} member variables (max {max_members})"));
        }
        if lines > max_lines {
            reasons.push(format!("{lines} lines (max {max_lines})"));
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
                context_lines: None,
            });
        }

        // Check inner classes
        check_classes(
            &file.declarations,
            max_functions,
            max_members,
            max_lines,
            &mut diags,
        );

        diags
    }
}

/// Count direct child function definitions and variable statements in a scope.
fn count_definitions(decls: &[GdDecl<'_>]) -> (usize, usize) {
    let mut functions = 0;
    let mut members = 0;
    for decl in decls {
        match decl {
            GdDecl::Func(_) => functions += 1,
            GdDecl::Var(_) => members += 1,
            GdDecl::Enum(e) => members += e.members.len(),
            _ => {}
        }
    }
    (functions, members)
}

/// Recursively check inner classes for god-object violations.
fn check_classes(
    decls: &[GdDecl<'_>],
    max_functions: usize,
    max_members: usize,
    max_lines: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    for decl in decls {
        if let GdDecl::Class(class) = decl {
            let (funcs, members) = count_definitions(&class.declarations);
            let class_lines = class.node.end_position().row - class.node.start_position().row + 1;
            let mut reasons = Vec::new();

            if funcs > max_functions {
                reasons.push(format!("{funcs} functions (max {max_functions})"));
            }
            if members > max_members {
                reasons.push(format!("{members} member variables (max {max_members})"));
            }
            if class_lines > max_lines {
                reasons.push(format!("{class_lines} lines (max {max_lines})"));
            }

            if !reasons.is_empty() {
                diags.push(LintDiagnostic {
                    rule: "god-object",
                    message: format!(
                        "class `{}` is too large: {}",
                        class.name,
                        reasons.join(", ")
                    ),
                    severity: Severity::Warning,
                    line: class.node.start_position().row,
                    column: class.node.start_position().column,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            }

            // Recurse for nested classes
            check_classes(
                &class.declarations,
                max_functions,
                max_members,
                max_lines,
                diags,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        GodObject.check(&file, source, &config)
    }

    fn make_functions(count: usize) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        for i in 0..count {
            write!(s, "func f_{i}():\n\tpass\n\n").unwrap();
        }
        s
    }

    fn make_members(count: usize) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        for i in 0..count {
            writeln!(s, "var m_{i}: int").unwrap();
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
