use crate::core::gd_ast::{GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MaxPublicMethods;

impl LintRule for MaxPublicMethods {
    fn name(&self) -> &'static str {
        "max-public-methods"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let max_methods = config.max_public_methods;
        let mut diags = Vec::new();

        // Check top-level scope (the script itself acts as a class)
        let top_level_count = count_public_methods(&file.declarations);
        if top_level_count > max_methods {
            diags.push(LintDiagnostic {
                rule: "max-public-methods",
                message: format!("script has {top_level_count} public methods (max {max_methods})"),
                severity: Severity::Warning,
                line: 0,
                column: 0,
                fix: None,
                end_column: None,
                context_lines: None,
            });
        }

        // Check inner classes
        check_classes(&file.declarations, max_methods, &mut diags);

        diags
    }
}

/// Count functions whose name doesn't start with "_" in a scope.
fn count_public_methods(decls: &[GdDecl<'_>]) -> usize {
    decls
        .iter()
        .filter(|d| matches!(d, GdDecl::Func(f) if !f.name.starts_with('_')))
        .count()
}

/// Recursively find inner classes and check their public method count.
fn check_classes(decls: &[GdDecl<'_>], max_methods: usize, diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Class(class) = decl {
            let count = count_public_methods(&class.declarations);
            if count > max_methods {
                diags.push(LintDiagnostic {
                    rule: "max-public-methods",
                    message: format!(
                        "class `{}` has {count} public methods (max {max_methods})",
                        class.name,
                    ),
                    severity: Severity::Warning,
                    line: class.node.start_position().row,
                    column: class.node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }

            // Check nested classes
            check_classes(&class.declarations, max_methods, diags);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;
    use crate::core::gd_ast;

    const DEFAULT_MAX_PUBLIC_METHODS: usize = 20;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        MaxPublicMethods.check(&file, source, &config)
    }

    fn make_methods(public: usize, private: usize) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        for i in 0..public {
            write!(s, "func method_{i}():\n\tpass\n\n").unwrap();
        }
        for i in 0..private {
            write!(s, "func _private_{i}():\n\tpass\n\n").unwrap();
        }
        s
    }

    fn make_class_methods(class_name: &str, public: usize, private: usize) -> String {
        use std::fmt::Write;
        let mut s = format!("class {class_name}:\n");
        for i in 0..public {
            write!(s, "\tfunc method_{i}():\n\t\tpass\n\n").unwrap();
        }
        for i in 0..private {
            write!(s, "\tfunc _private_{i}():\n\t\tpass\n\n").unwrap();
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
