use crate::core::gd_ast::{self, GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct LongFunction;

impl LintRule for LongFunction {
    fn name(&self) -> &'static str {
        "long-function"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let max_lines = config
            .rules
            .get("long-function")
            .and_then(|r| r.max_lines)
            .unwrap_or(config.max_function_length);
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                let start_line = func.node.start_position().row;
                let end_line = func.node.end_position().row;
                let line_count = end_line - start_line + 1;

                if line_count > max_lines {
                    diags.push(LintDiagnostic {
                        rule: "long-function",
                        message: format!(
                            "function `{}` is {line_count} lines long (max {max_lines})",
                            func.name,
                        ),
                        severity: Severity::Warning,
                        line: start_line,
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
