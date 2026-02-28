use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct RedundantStaticUnload;

impl LintRule for RedundantStaticUnload {
    fn name(&self) -> &'static str {
        "redundant-static-unload"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        if !file.has_static_unload {
            return Vec::new();
        }

        let has_static_var = file.vars().any(|v| v.is_static);
        if has_static_var {
            return Vec::new();
        }

        vec![LintDiagnostic {
            rule: "redundant-static-unload",
            message: "`@static_unload` is redundant — no `static var` declarations found"
                .to_string(),
            severity: Severity::Warning,
            line: 0,
            column: 0,
            end_column: None,
            fix: None,
            context_lines: None,
        }]
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
        RedundantStaticUnload.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn detects_redundant_static_unload() {
        let source = "@static_unload\nextends Node\nvar x: int = 0\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("redundant"));
    }

    #[test]
    fn no_warning_with_static_var() {
        let source = "@static_unload\nextends Node\nstatic var cache: Dictionary = {}\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_without_static_unload() {
        let source = "extends Node\nvar x: int = 0\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!RedundantStaticUnload.default_enabled());
    }
}
