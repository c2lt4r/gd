use tree_sitter::Tree;

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;

pub struct EnumVariableWithoutDefault;

impl LintRule for EnumVariableWithoutDefault {
    fn name(&self) -> &'static str {
        "enum-variable-without-default"
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
    let enum_names: Vec<&str> = symbols.enums.iter().map(|e| e.name.as_str()).collect();

    for var in &symbols.variables {
        if var.has_default || var.is_constant {
            continue;
        }
        if let Some(ref ann) = var.type_ann
            && !ann.is_inferred
            && enum_names.contains(&ann.name.as_str())
        {
            diags.push(LintDiagnostic {
                rule: "enum-variable-without-default",
                message: format!(
                    "`{}` is typed as enum `{}` but has no default value — it will be `0`, not the first enum member",
                    var.name, ann.name
                ),
                severity: Severity::Warning,
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
        EnumVariableWithoutDefault.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn detects_enum_var_without_default() {
        let source = "enum State { IDLE, RUN }\nvar state: State\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("state"));
        assert!(diags[0].message.contains("State"));
    }

    #[test]
    fn no_warning_with_default() {
        let source = "enum State { IDLE, RUN }\nvar state: State = State.IDLE\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_enum_type() {
        let source = "enum State { IDLE, RUN }\nvar health: int\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_constant() {
        let source = "enum State { IDLE, RUN }\nconst DEFAULT: State = State.IDLE\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_untyped_var() {
        let source = "enum State { IDLE, RUN }\nvar state\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!EnumVariableWithoutDefault.default_enabled());
    }
}
