use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;

pub struct GetNodeDefaultWithoutOnready;

impl LintRule for GetNodeDefaultWithoutOnready {
    fn name(&self) -> &'static str {
        "get-node-default-without-onready"
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
        check_vars(tree.root_node(), source, symbols, &mut diags);
        diags
    }
}

/// Walk top-level `variable_statement` nodes and check if the default value
/// uses `$Path` or `get_node()` without an `@onready` annotation.
fn check_vars(root: Node, source: &str, symbols: &SymbolTable, diags: &mut Vec<LintDiagnostic>) {
    let bytes = source.as_bytes();
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() != "variable_statement" {
            continue;
        }
        let Some(value) = child.child_by_field_name("value") else {
            continue;
        };
        if !uses_get_node(&value, bytes) {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(var_name) = name_node.utf8_text(bytes) else {
            continue;
        };

        // Check if this variable has @onready in the symbol table
        let has_onready = symbols
            .variables
            .iter()
            .any(|v| v.name == var_name && v.annotations.iter().any(|a| a == "onready"));

        if !has_onready {
            diags.push(LintDiagnostic {
                rule: "get-node-default-without-onready",
                message: format!(
                    "`{var_name}` uses `$`/`get_node()` as default but lacks `@onready` — node tree isn't ready at variable initialization time"
                ),
                severity: Severity::Error,
                line: child.start_position().row,
                column: child.start_position().column,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }
    }
}

/// Check if a value expression uses `$Path` (`get_node`) or `get_node()`.
fn uses_get_node(node: &Node, source: &[u8]) -> bool {
    if node.kind() == "get_node" {
        return true;
    }
    // get_node("Path") as a direct call
    if node.kind() == "call"
        && let Some(id) = node.named_child(0)
        && id.kind() == "identifier"
        && id.utf8_text(source).ok() == Some("get_node")
    {
        return true;
    }
    // Recurse into children (e.g. for expressions like $Sprite.texture)
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if uses_get_node(&cursor.node(), source) {
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
        GetNodeDefaultWithoutOnready.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn detects_dollar_without_onready() {
        let source = "var sprite = $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("sprite"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn detects_get_node_call_without_onready() {
        let source = "var sprite = get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("sprite"));
    }

    #[test]
    fn no_warning_with_onready() {
        let source = "@onready var sprite = $Sprite2D\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_normal_default() {
        let source = "var health: int = 100\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_no_default() {
        let source = "var sprite: Node\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!GetNodeDefaultWithoutOnready.default_enabled());
    }
}
