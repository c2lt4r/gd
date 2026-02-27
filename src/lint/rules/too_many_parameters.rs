use crate::core::gd_ast::{self, GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct TooManyParameters;

impl LintRule for TooManyParameters {
    fn name(&self) -> &'static str {
        "too-many-parameters"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let max_params = config
            .rules
            .get("too-many-parameters")
            .and_then(|r| r.max_params)
            .unwrap_or(config.max_function_params);
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                let count = func.params.len();
                if count > max_params {
                    diags.push(LintDiagnostic {
                        rule: "too-many-parameters",
                        message: format!(
                            "function `{}` has {count} parameters (max {max_params})",
                            func.name,
                        ),
                        severity: Severity::Warning,
                        line: func.node.start_position().row,
                        column: func.node.start_position().column,
                        fix: None,
                        end_column: None,
                        context_lines: None,
                    });
                }
            }
        });
        diags
    }
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
        TooManyParameters.check(&file, source, &config)
    }

    #[test]
    fn no_warning_under_limit() {
        let source = "func foo(a, b, c):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_at_limit() {
        let source = "func foo(a, b, c, d, e):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_over_limit() {
        let source = "func foo(a, b, c, d, e, f):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "too-many-parameters");
        assert!(diags[0].message.contains("6 parameters"));
    }

    #[test]
    fn warns_with_typed_params() {
        let source =
            "func setup(a: int, b: float, c: String, d: bool, e: Array, f: Dictionary):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("6 parameters"));
    }

    #[test]
    fn warns_with_default_params() {
        let source = "func init(a, b, c, d = 1, e = 2, f = 3):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn warns_with_typed_default_params() {
        let source =
            "func create(a: int, b: int, c: int, d: int = 0, e: int = 0, f: int = 0):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_zero_params() {
        let source = "func foo():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn checks_inner_class_functions() {
        let source = "\
class Inner:
\tfunc method(a, b, c, d, e, f, g):
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("method"));
    }

    #[test]
    fn multiple_functions() {
        let source = "\
func ok(a, b):
\tpass

func bad(a, b, c, d, e, f):
\tpass

func also_ok(x):
\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bad"));
    }

    #[test]
    fn reports_correct_function_name() {
        let source = "func my_complex_function(a, b, c, d, e, f):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("my_complex_function"));
    }
}
