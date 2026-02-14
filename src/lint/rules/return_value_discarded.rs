use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct ReturnValueDiscarded;

impl LintRule for ReturnValueDiscarded {
    fn name(&self) -> &'static str {
        "return-value-discarded"
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
        check_node(tree.root_node(), source, symbols, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, symbols: &SymbolTable, diags: &mut Vec<LintDiagnostic>) {
    // Look for expression statements that are function calls with non-void return
    if node.kind() == "expression_statement"
        && let Some(expr) = node.named_child(0)
        && is_discarded_non_void_call(&expr, source, symbols)
    {
        let call_text = expr.utf8_text(source.as_bytes()).ok().unwrap_or("?");
        let display = if call_text.len() > 40 {
            format!("{}...", &call_text[..37])
        } else {
            call_text.to_string()
        };
        diags.push(LintDiagnostic {
            rule: "return-value-discarded",
            message: format!("return value of `{display}` is discarded"),
            severity: Severity::Info,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, symbols, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_discarded_non_void_call(node: &Node, source: &str, symbols: &SymbolTable) -> bool {
    // Only check actual call expressions
    let is_call = node.kind() == "call"
        || (node.kind() == "attribute" && {
            let mut cursor = node.walk();
            node.children(&mut cursor)
                .any(|c| c.kind() == "attribute_call")
        });
    if !is_call {
        return false;
    }

    match infer_expression_type(node, source, symbols) {
        Some(InferredType::Void) | None => false,
        Some(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        ReturnValueDiscarded.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn discarded_non_void_call() {
        let source = "\
extends Node
func f():
\tget_child(0)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("discarded"));
    }

    #[test]
    fn void_call_ok() {
        let source = "\
extends Node
func f():
\tadd_child(null)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assigned_call_ok() {
        let source = "\
extends Node
func f():
\tvar child = get_child(0)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn discarded_self_method() {
        let source = "\
func get_value() -> int:
\treturn 42
func f():
\tget_value()
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn self_void_method_ok() {
        let source = "\
func do_thing() -> void:
\tpass
func f():
\tdo_thing()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!ReturnValueDiscarded.default_enabled());
    }
}
