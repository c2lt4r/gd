use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct NarrowingConversion;

impl LintRule for NarrowingConversion {
    fn name(&self) -> &'static str {
        "narrowing-conversion"
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
    // Check variable declarations with explicit int type and float value
    if node.kind() == "variable_statement"
        && let Some(type_node) = node.child_by_field_name("type")
        && type_node.kind() != "inferred_type"
        && let Ok(type_name) = type_node.utf8_text(source.as_bytes())
        && type_name == "int"
        && let Some(value) = node.child_by_field_name("value")
        && matches!(
            infer_expression_type(&value, source, symbols),
            Some(InferredType::Builtin("float"))
        )
    {
        let var_name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .unwrap_or("?");
        // Fix: wrap the value with int()
        let value_text = value.utf8_text(source.as_bytes()).unwrap_or("");
        let fix = Some(Fix {
            byte_start: value.start_byte(),
            byte_end: value.end_byte(),
            replacement: format!("int({value_text})"),
        });
        diags.push(LintDiagnostic {
            rule: "narrowing-conversion",
            message: format!("narrowing conversion: float value assigned to `{var_name}: int`"),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix,
            context_lines: None,
        });
    }

    // Check assignments to int-typed variables
    if node.kind() == "assignment"
        && let Some(left) = node.child_by_field_name("left")
        && let Some(right) = node.child_by_field_name("right")
    {
        let var_name = left.utf8_text(source.as_bytes()).ok().unwrap_or("?");
        let is_int_var = symbols
            .variables
            .iter()
            .any(|v| v.name == var_name && v.type_ann.as_ref().is_some_and(|t| t.name == "int"));
        if is_int_var
            && matches!(
                infer_expression_type(&right, source, symbols),
                Some(InferredType::Builtin("float"))
            )
        {
            let value_text = right.utf8_text(source.as_bytes()).unwrap_or("");
            let fix = Some(Fix {
                byte_start: right.start_byte(),
                byte_end: right.end_byte(),
                replacement: format!("int({value_text})"),
            });
            diags.push(LintDiagnostic {
                rule: "narrowing-conversion",
                message: format!("narrowing conversion: float value assigned to `{var_name}: int`"),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: None,
                fix,
                context_lines: None,
            });
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        NarrowingConversion.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn float_literal_to_int_var() {
        let source = "var x: int = 3.14\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("narrowing"));
    }

    #[test]
    fn float_expr_to_int_var() {
        let source = "var x: int = 1.0 + 2.0\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn int_literal_to_int_var() {
        let source = "var x: int = 42\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_type_annotation() {
        let source = "var x = 3.14\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn float_type_var() {
        let source = "var x: float = 3.14\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_wraps_with_int() {
        let source = "var x: int = 3.14\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        assert!(fixed.contains("int(3.14)"), "fixed was: {fixed}");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!NarrowingConversion.default_enabled());
    }
}
