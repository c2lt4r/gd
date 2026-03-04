use gd_core::gd_ast::{self, GdDecl, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct SignalNameConvention;

impl LintRule for SignalNameConvention {
    fn name(&self) -> &'static str {
        "signal-name-convention"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Signal(sig) = decl
                && let Some(fixed) = sig.name.strip_prefix("on_")
            {
                let (line, col, end_col) = sig.name_node.map_or(
                    (
                        sig.node.start_position().row,
                        sig.node.start_position().column,
                        None,
                    ),
                    |n| {
                        (
                            n.start_position().row,
                            n.start_position().column,
                            Some(n.end_position().column),
                        )
                    },
                );
                let fix = sig.name_node.map(|n| Fix {
                    byte_start: n.start_byte(),
                    byte_end: n.end_byte(),
                    replacement: fixed.to_string(),
                });
                diags.push(LintDiagnostic {
                    rule: "signal-name-convention",
                    message: format!(
                        "signal names shouldn't use \"on_\" prefix, use \"{fixed}\" instead",
                    ),
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
