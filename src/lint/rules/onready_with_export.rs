use tree_sitter::Tree;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;

pub struct OnreadyWithExport;

impl LintRule for OnreadyWithExport {
    fn name(&self) -> &'static str {
        "onready-with-export"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
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
        let mut diags = Vec::new();
        check_table(symbols, &mut diags);
        for (_, inner) in &symbols.inner_classes {
            check_table(inner, &mut diags);
        }
        diags
    }
}

fn check_table(symbols: &SymbolTable, diags: &mut Vec<LintDiagnostic>) {
    for var in &symbols.variables {
        let has_onready = var.annotations.iter().any(|a| a == "onready");
        let has_export = var.annotations.iter().any(|a| a == "export");
        if has_onready && has_export {
            diags.push(LintDiagnostic {
                rule: "onready-with-export",
                message: format!(
                    "`{}` has both `@onready` and `@export` — `@onready` sets the value after `@export`, making the export useless",
                    var.name
                ),
                severity: Severity::Error,
                line: var.line,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }
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
        OnreadyWithExport.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn detects_onready_with_export() {
        let source = "@export\n@onready var sprite = $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("sprite"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn no_warning_export_only() {
        let source = "@export var health: int = 100\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_onready_only() {
        let source = "@onready var sprite = $Sprite2D\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_plain_var() {
        let source = "var x: int = 0\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!OnreadyWithExport.default_enabled());
    }
}
