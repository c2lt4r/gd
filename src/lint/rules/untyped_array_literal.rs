use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UntypedArrayLiteral;

impl LintRule for UntypedArrayLiteral {
    fn name(&self) -> &'static str {
        "untyped-array-literal"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "variable_statement" {
        check_variable(node, source, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_variable(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Check if this uses := (inferred type via inferred_type node)
    let is_inferred = node
        .child_by_field_name("type")
        .is_some_and(|t| t.kind() == "inferred_type");
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

    // Try to infer element type
    let suggested_type = infer_array_element_type(value, source);

    let var_name = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("?");

    let message = if let Some(elem_type) = suggested_type {
        format!(
            "array literal infers `Variant` with `:=`; consider `var {var_name}: Array[{elem_type}] = [...]`"
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
        fix: None,
        context_lines: None,
    });
}

fn infer_array_element_type(array_node: Node, _source: &str) -> Option<&'static str> {
    let count = array_node.named_child_count();
    if count == 0 {
        return None;
    }

    let mut all_string = true;
    let mut all_int = true;
    let mut all_float = true;
    let mut all_bool = true;

    for i in 0..count {
        let child = array_node.named_child(i)?;
        match child.kind() {
            "string" => {
                all_int = false;
                all_float = false;
                all_bool = false;
            }
            "integer" => {
                all_string = false;
                all_float = false;
                all_bool = false;
            }
            "float" => {
                all_string = false;
                all_int = false;
                all_bool = false;
            }
            "true" | "false" => {
                all_string = false;
                all_int = false;
                all_float = false;
            }
            _ => return None,
        }
    }

    if all_string {
        Some("String")
    } else if all_int {
        Some("int")
    } else if all_float {
        Some("float")
    } else if all_bool {
        Some("bool")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        UntypedArrayLiteral.check(&tree, source, &config)
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
    fn default_enabled() {
        assert!(UntypedArrayLiteral.default_enabled());
    }
}
