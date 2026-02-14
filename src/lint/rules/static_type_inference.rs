use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct StaticTypeInference;

impl LintRule for StaticTypeInference {
    fn name(&self) -> &'static str {
        "static-type-inference"
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
        let root = tree.root_node();
        check_node(root, source, symbols, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, symbols: &SymbolTable, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "variable_statement" {
        // Skip if already has a type annotation
        if node.child_by_field_name("type").is_none()
            && let Some(value_node) = node.child_by_field_name("value")
            && let Some(inferred) = infer_expression_type(&value_node, source, symbols)
            // Only suggest for concrete builtin types (not Void, Variant, or Class)
            && matches!(inferred, InferredType::Builtin(_))
        {
            let var_name = node
                .child_by_field_name("name")
                .map_or("variable", |n| &source[n.byte_range()]);
            let name_node = node.child_by_field_name("name");
            let (line, col) = if let Some(n) = name_node {
                (n.start_position().row, n.start_position().column)
            } else {
                (node.start_position().row, node.start_position().column)
            };

            diags.push(LintDiagnostic {
                rule: "static-type-inference",
                message: format!(
                    "variable `{var_name}` could have an explicit type: `{}`",
                    inferred.display_name()
                ),
                severity: Severity::Warning,
                line,
                column: col,
                end_column: None,
                fix: None,
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

// Tests remain the same — same diagnostics, consolidated implementation.
// Now uses check_with_symbols instead of check.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        StaticTypeInference.check_with_symbols(&tree, source, &config, &symbols)
    }

    #[test]
    fn suggests_int_type() {
        let diags = check("var x = 42\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`int`"));
    }

    #[test]
    fn suggests_float_type() {
        let diags = check("var x = 3.14\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`float`"));
    }

    #[test]
    fn suggests_string_type() {
        let diags = check("var x = \"hello\"\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`String`"));
    }

    #[test]
    fn suggests_bool_type() {
        let diags = check("var x = true\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`bool`"));
    }

    #[test]
    fn suggests_array_type() {
        let diags = check("var x = [1, 2, 3]\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Array`"));
    }

    #[test]
    fn suggests_dictionary_type() {
        let diags = check("var x = {\"a\": 1}\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Dictionary`"));
    }

    #[test]
    fn suggests_vector2_type() {
        let diags = check("var x = Vector2(1, 2)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Vector2`"));
    }

    #[test]
    fn suggests_color_type() {
        let diags = check("var x = Color(1, 0, 0)\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`Color`"));
    }

    #[test]
    fn no_warning_typed_var() {
        let diags = check("var x: int = 42\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_inferred_var() {
        let diags = check("var x := 42\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn suggests_negative_int() {
        let diags = check("var x = -5\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`int`"));
    }

    #[test]
    fn suggests_negative_float() {
        let diags = check("var x = -3.14\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`float`"));
    }

    #[test]
    fn no_warning_unresolvable() {
        // Variable assigned from a function call that doesn't return a builtin
        let diags = check("var x = get_node(\"path\")\n");
        assert!(diags.is_empty());
    }
}
