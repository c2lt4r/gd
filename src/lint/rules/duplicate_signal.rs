use crate::core::gd_ast::{GdDecl, GdFile};
use std::collections::HashMap;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateSignal;

impl LintRule for DuplicateSignal {
    fn name(&self) -> &'static str {
        "duplicate-signal"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_scope(&file.declarations, &mut diags);
        diags
    }
}

/// Check a single scope for duplicate signal names.
fn check_scope(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    let mut signals: HashMap<&str, usize> = HashMap::new();

    for decl in decls {
        if let GdDecl::Signal(sig) = decl {
            let line = sig.node.start_position().row;
            let col = sig.name_node.map_or(sig.node.start_position().column, |n| {
                n.start_position().column
            });

            if let Some(&first_line) = signals.get(sig.name) {
                diags.push(LintDiagnostic {
                    rule: "duplicate-signal",
                    message: format!(
                        "signal `{}` already declared on line {}",
                        sig.name,
                        first_line + 1,
                    ),
                    severity: Severity::Error,
                    line,
                    column: col,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            } else {
                signals.insert(sig.name, line);
            }
        }

        // Recurse into inner classes (separate scope)
        if let GdDecl::Class(class) = decl {
            check_scope(&class.declarations, diags);
        }
    }
}
