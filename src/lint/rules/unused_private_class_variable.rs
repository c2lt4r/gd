use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;

pub struct UnusedPrivateClassVariable;

impl LintRule for UnusedPrivateClassVariable {
    fn name(&self) -> &'static str {
        "unused-private-class-variable"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _tree: &Tree, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        tree: &Tree,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        for var in &symbols.variables {
            if !var.name.starts_with('_') || var.is_constant {
                continue;
            }
            // Check if the variable name appears anywhere else in the source
            // besides its declaration
            if !is_used_elsewhere(tree.root_node(), source, &var.name, var.line) {
                diags.push(LintDiagnostic {
                    rule: "unused-private-class-variable",
                    message: format!(
                        "private variable `{}` is declared but never used in this file",
                        var.name
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

        diags
    }
}

/// Search the AST for any `identifier` node matching `name` that is NOT
/// on the declaration line itself.
fn is_used_elsewhere(root: Node, source: &str, name: &str, decl_line: usize) -> bool {
    let bytes = source.as_bytes();
    search_node(root, bytes, name, decl_line)
}

fn search_node(node: Node, source: &[u8], name: &str, decl_line: usize) -> bool {
    if node.kind() == "identifier"
        && node.start_position().row != decl_line
        && node.utf8_text(source).ok() == Some(name)
    {
        return true;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if search_node(cursor.node(), source, name, decl_line) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
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
        UnusedPrivateClassVariable.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn detects_unused_private_var() {
        let source = "var _unused: int = 0\nfunc f():\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_unused"));
    }

    #[test]
    fn no_warning_when_used() {
        let source = "var _count: int = 0\nfunc f():\n\t_count += 1\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_public_var() {
        let source = "var unused_public: int = 0\nfunc f():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_constant() {
        let source = "const _CONST: int = 42\nfunc f():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_used_in_different_function() {
        let source = "var _hp: int = 100\nfunc take_damage(n: int):\n\t_hp -= n\nfunc get_hp() -> int:\n\treturn _hp\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!UnusedPrivateClassVariable.default_enabled());
    }
}
