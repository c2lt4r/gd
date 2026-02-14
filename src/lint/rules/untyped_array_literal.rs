use tree_sitter::{Node, Tree};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct UntypedArrayLiteral;

impl LintRule for UntypedArrayLiteral {
    fn name(&self) -> &'static str {
        "untyped-array-literal"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
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
    if node.kind() == "variable_statement" {
        check_variable(node, source, symbols, diags);
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

fn check_variable(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Check if this uses := (inferred type via inferred_type node)
    let type_node = node.child_by_field_name("type");
    let is_inferred = type_node.is_some_and(|t| t.kind() == "inferred_type");
    if !is_inferred {
        return;
    }

    // Skip const declarations
    let first_token = &source[node.start_byte()..].trim_start();
    if first_token.starts_with("const") {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else {
        return;
    };

    if value.kind() != "array" {
        return;
    }

    // Only fire for non-empty array literals
    let element_count = value.named_child_count();
    if element_count == 0 {
        return;
    }

    // Try to infer element type using the centralized engine
    let suggested_type = infer_array_element_type(value, source, symbols);

    let var_name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("?");

    // Build auto-fix when type is inferable: replace ` :=` with `: Array[T] =`
    let fix = suggested_type.as_ref().and_then(|elem_type| {
        let inferred = type_node?;
        let mut start = inferred.start_byte();
        // Consume preceding whitespace so we get `var x: Array[T]` not `var x : Array[T]`
        while start > 0 && source.as_bytes()[start - 1] == b' ' {
            start -= 1;
        }
        Some(Fix {
            byte_start: start,
            byte_end: inferred.end_byte(),
            replacement: format!(": Array[{}] =", elem_type.display_name()),
        })
    });

    let message = if let Some(ref elem_type) = suggested_type {
        format!(
            "array literal infers `Variant` with `:=`; consider `var {var_name}: Array[{}] = [...]`",
            elem_type.display_name()
        )
    } else {
        "array literal infers `Variant` with `:=`; consider adding an explicit `Array[T]` type"
            .to_string()
    };

    diags.push(LintDiagnostic {
        rule: "untyped-array-literal",
        message,
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix,
        context_lines: None,
    });
}

/// Infer the common element type of an array literal using the centralized engine.
fn infer_array_element_type(
    array_node: Node,
    source: &str,
    symbols: &SymbolTable,
) -> Option<InferredType> {
    let count = array_node.named_child_count();
    if count == 0 {
        return None;
    }

    let first_type = infer_expression_type(&array_node.named_child(0)?, source, symbols)?;

    // Skip Variant — can't determine a concrete element type
    if matches!(first_type, InferredType::Variant | InferredType::Void) {
        return None;
    }

    // Check that all elements have the same type
    for i in 1..count {
        let child = array_node.named_child(i)?;
        let child_type = infer_expression_type(&child, source, symbols)?;
        if child_type != first_type {
            return None;
        }
    }

    Some(first_type)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        UntypedArrayLiteral.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn detects_string_array() {
        let source = "func f():\n\tvar x := [\"a\", \"b\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[String]"));
    }

    #[test]
    fn detects_int_array() {
        let source = "func f():\n\tvar x := [1, 2, 3]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[int]"));
    }

    #[test]
    fn detects_float_array() {
        let source = "func f():\n\tvar x := [1.0, 2.5]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[float]"));
    }

    #[test]
    fn detects_bool_array() {
        let source = "func f():\n\tvar x := [true, false]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[bool]"));
    }

    #[test]
    fn mixed_array_no_type_suggestion() {
        let source = "func f():\n\tvar x := [1, \"a\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[T]"));
    }

    #[test]
    fn no_warning_empty_array() {
        let source = "func f():\n\tvar x := []\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_explicit_type() {
        let source = "func f():\n\tvar x: Array[String] = [\"a\", \"b\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_regular_equals() {
        let source = "func f():\n\tvar x = [\"a\", \"b\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_const() {
        let source = "const ITEMS := [\"a\", \"b\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_string_array() {
        let source = "func f():\n\tvar x := [\"a\", \"b\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        // `:=` replaced with `: Array[String] =`
        assert!(
            fixed.contains("var x: Array[String] ="),
            "fixed was: {fixed}"
        );
    }

    #[test]
    fn autofix_int_array() {
        let source = "func f():\n\tvar nums := [1, 2, 3]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        assert_eq!(fix.replacement, ": Array[int] =");
    }

    #[test]
    fn no_autofix_mixed_array() {
        let source = "func f():\n\tvar x := [1, \"a\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].fix.is_none());
    }

    #[test]
    fn detects_constructor_array() {
        let source = "func f():\n\tvar pts := [Vector2(0, 0), Vector2(1, 1)]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Array[Vector2]"));
        assert!(diags[0].fix.is_some());
    }

    #[test]
    fn default_enabled() {
        assert!(UntypedArrayLiteral.default_enabled());
    }
}
