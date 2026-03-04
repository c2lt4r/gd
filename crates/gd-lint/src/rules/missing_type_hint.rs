use gd_core::gd_ast::{self, GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct MissingTypeHint;

impl LintRule for MissingTypeHint {
    fn name(&self) -> &'static str {
        "missing-type-hint"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                // Check parameters for missing type hints
                for param in &func.params {
                    if param.type_ann.is_none() {
                        diags.push(LintDiagnostic {
                            rule: "missing-type-hint",
                            message: format!(
                                "parameter `{}` in function `{}` has no type hint",
                                param.name, func.name,
                            ),
                            severity: Severity::Warning,
                            line: param.node.start_position().row,
                            column: param.node.start_position().column,
                            fix: None,
                            end_column: None,
                            context_lines: None,
                        });
                    }
                }

                // Check for missing return type
                if func.return_type.is_none() {
                    diags.push(LintDiagnostic {
                        rule: "missing-type-hint",
                        message: format!("function `{}` has no return type hint", func.name),
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
