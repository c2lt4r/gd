use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct VariantInference;

impl LintRule for VariantInference {
    fn name(&self) -> &'static str {
        "variant-inference"
    }

    fn default_enabled(&self) -> bool {
        false
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

    let Some(value) = node.child_by_field_name("value") else {
        return;
    };

    if is_variant_producing(value, source) {
        let name_node = node.child_by_field_name("name");
        let var_name = name_node
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .unwrap_or("?");

        diags.push(LintDiagnostic {
            rule: "variant-inference",
            message: format!(
                "`:=` infers `Variant` for `{var_name}` — use an explicit type annotation"
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

fn is_variant_producing(node: Node, source: &str) -> bool {
    match node.kind() {
        // dict["key"] or arr[idx]
        "subscript" => true,
        // method calls: attribute > attribute_call (tree-sitter pattern)
        // e.g. dict.get("key"), dict.values(), dict.keys()
        "attribute" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "attribute_call"
                    && let Some(name_node) = child.named_child(0)
                    && let Ok(method_name) = name_node.utf8_text(source.as_bytes())
                {
                    return matches!(method_name, "get" | "get_or_add" | "values" | "keys");
                }
            }
            false
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        VariantInference.check(&tree, source, &config)
    }

    #[test]
    fn detects_dict_subscript() {
        let source = "var dict := {}\nfunc f():\n\tvar x := dict[\"key\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Variant"));
    }

    #[test]
    fn detects_dict_get() {
        let source = "var dict := {}\nfunc f():\n\tvar x := dict.get(\"key\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_dict_values() {
        let source = "var dict := {}\nfunc f():\n\tvar x := dict.values()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_dict_keys() {
        let source = "var dict := {}\nfunc f():\n\tvar x := dict.keys()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_explicit_type() {
        let source = "var dict := {}\nfunc f():\n\tvar x: String = dict[\"key\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_simple_assignment() {
        let source = "func f():\n\tvar x := 42\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_regular_equals() {
        let source = "func f():\n\tvar x = dict[\"key\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!VariantInference.default_enabled());
    }
}
