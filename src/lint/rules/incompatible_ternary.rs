use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct IncompatibleTernary;

impl LintRule for IncompatibleTernary {
    fn name(&self) -> &'static str {
        "incompatible-ternary"
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
    if node.kind() == "conditional_expression" || node.kind() == "ternary_expression" {
        check_ternary(node, source, symbols, diags);
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

fn check_ternary(node: Node, source: &str, symbols: &SymbolTable, diags: &mut Vec<LintDiagnostic>) {
    // conditional_expression: [0] = true branch, [1] = condition, [2] = false branch
    let Some(true_branch) = node.named_child(0) else {
        return;
    };
    let Some(false_branch) = node.named_child(2) else {
        return;
    };

    let Some(true_type) = infer_expression_type(&true_branch, source, symbols) else {
        return;
    };
    let Some(false_type) = infer_expression_type(&false_branch, source, symbols) else {
        return;
    };

    // Skip if either side is Variant (dynamic, can't tell)
    if matches!(true_type, InferredType::Variant) || matches!(false_type, InferredType::Variant) {
        return;
    }

    // Allow int/float mixing (arithmetic promotion)
    if true_type.is_numeric() && false_type.is_numeric() {
        return;
    }

    if true_type != false_type {
        diags.push(LintDiagnostic {
            rule: "incompatible-ternary",
            message: format!(
                "ternary branches have incompatible types: `{}` vs `{}`",
                true_type.display_name(),
                false_type.display_name()
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
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
        IncompatibleTernary.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn incompatible_string_int() {
        let source = "func f():\n\tvar x = \"a\" if true else 1\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("String"));
        assert!(diags[0].message.contains("int"));
    }

    #[test]
    fn compatible_same_type() {
        let source = "func f():\n\tvar x = 1 if true else 2\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn compatible_int_float() {
        let source = "func f():\n\tvar x = 1 if true else 2.0\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn incompatible_bool_string() {
        let source = "func f():\n\tvar x = true if true else \"no\"\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn opt_in_rule() {
        assert!(!IncompatibleTernary.default_enabled());
    }
}
