use crate::core::gd_ast::{self, GdExpr, GdFile};
use std::collections::HashSet;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateKey;

impl LintRule for DuplicateKey {
    fn name(&self) -> &'static str {
        "duplicate-key"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::Dict { pairs, .. } = expr {
                let mut seen: HashSet<String> = HashSet::new();
                for (key, _) in pairs {
                    let key_text = &source[key.node().byte_range()];
                    if !seen.insert(key_text.to_string()) {
                        diags.push(LintDiagnostic {
                            rule: "duplicate-key",
                            message: format!("duplicate dictionary key {key_text}"),
                            severity: Severity::Warning,
                            line: key.node().start_position().row,
                            column: key.node().start_position().column,
                            end_column: None,
                            fix: None,
                            context_lines: None,
                        });
                    }
                }
            }
        });
        diags
    }
}
