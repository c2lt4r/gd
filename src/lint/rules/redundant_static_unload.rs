use tree_sitter::Tree;

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;

pub struct RedundantStaticUnload;

impl LintRule for RedundantStaticUnload {
    fn name(&self) -> &'static str {
        "redundant-static-unload"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _tree: &Tree, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        _tree: &Tree,
        _source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
    ) -> Vec<LintDiagnostic> {
        if !symbols.has_static_unload {
            return Vec::new();
        }

        let has_static_var = symbols.variables.iter().any(|v| v.is_static);
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
    use crate::core::symbol_table;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        RedundantStaticUnload.check_with_symbols(&tree, source, &config, &symbols)
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
