use tree_sitter::Tree;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;

pub struct NativeMethodOverride;

impl LintRule for NativeMethodOverride {
    fn name(&self) -> &'static str {
        "native-method-override"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
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
    let Some(ref extends) = symbols.extends else {
        return;
    };

    for func in &symbols.functions {
        // Skip virtual methods (prefixed with _) — these are meant to be overridden
        if func.name.starts_with('_') {
            continue;
        }

        // Check engine classes via ClassDB
        if crate::class_db::method_exists(extends, &func.name) {
            diags.push(LintDiagnostic {
                rule: "native-method-override",
                message: format!(
                    "`{}()` overrides a native method from `{extends}` — this may cause unexpected behavior",
                    func.name
                ),
                severity: Severity::Error,
                line: func.line,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
            // User-defined base class methods are NOT flagged — overriding them
            // is normal polymorphism (e.g. State pattern: enter/exit/update).
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
        NativeMethodOverride.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn detects_native_method_override() {
        // add_child is a method on Node
        let source = "extends Node\nfunc add_child(node):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add_child"));
        assert!(diags[0].message.contains("Node"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn detects_inherited_method_override() {
        // add_child is on Node, Node2D extends CanvasItem extends Node
        let source = "extends Node2D\nfunc add_child(node):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("add_child"));
    }

    #[test]
    fn no_warning_for_virtual_methods() {
        // _ready is a virtual method meant to be overridden
        let source = "extends Node\nfunc _ready():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_custom_methods() {
        let source = "extends Node\nfunc my_custom_method():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_without_extends() {
        let source = "func add_child(node):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_non_engine_class() {
        let source = "extends MyCustomClass\nfunc add_child(node):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!NativeMethodOverride.default_enabled());
    }
}
