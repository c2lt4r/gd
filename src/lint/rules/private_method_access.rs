use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PrivateMethodAccess;

const ALLOWED_CALLBACKS: &[&str] = &["_to_string"];

impl LintRule for PrivateMethodAccess {
    fn name(&self) -> &'static str {
        "private-method-access"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // tree-sitter-gdscript 6.1 parses `obj.method()` as:
    //   attribute
    //     identifier (object)
    //     .
    //     attribute_call
    //       identifier (method name)
    //       arguments
    if node.kind() == "attribute" {
        // Look for attribute_call child (indicates this is a method call)
        let mut has_call = false;
        let mut method_name = String::new();
        let mut method_row = 0;
        let mut method_col = 0;
        let mut object_text = String::new();

        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            // First named child is the object
            let first = cursor.node();
            if first.kind() == "identifier" || first.is_named() {
                object_text = source[first.byte_range()].to_string();
            }

            loop {
                let child = cursor.node();
                if child.kind() == "attribute_call" {
                    has_call = true;
                    // First named child of attribute_call is the method name
                    if let Some(name_node) = child.named_child(0) {
                        method_name = source[name_node.byte_range()].to_string();
                        method_row = name_node.start_position().row;
                        method_col = name_node.start_position().column;
                    }
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }

        if has_call
            && method_name.starts_with('_')
            && object_text != "self"
            && object_text != "super"
            && !ALLOWED_CALLBACKS.contains(&method_name.as_str())
        {
            diags.push(LintDiagnostic {
                rule: "private-method-access",
                message: format!(
                    "accessing private method `{}` on external object",
                    method_name
                ),
                severity: Severity::Warning,
                line: method_row,
                column: method_col,
                fix: None,
                end_column: None,
            });
        }
    }

    // Also check old-style `call` nodes with `attribute` function (for compatibility)
    if node.kind() == "call"
        && let Some(func_node) = node.child_by_field_name("function")
        && func_node.kind() == "attribute"
    {
        let mut object_text = String::new();
        let mut cursor = func_node.walk();
        if cursor.goto_first_child() {
            object_text = source[cursor.node().byte_range()].to_string();
        }

        let last_child = func_node.named_child(func_node.named_child_count().saturating_sub(1));
        if let Some(method_node) = last_child {
            let method_text = source[method_node.byte_range()].to_string();

            if method_text.starts_with('_')
                && object_text != "self"
                && object_text != "super"
                && !ALLOWED_CALLBACKS.contains(&method_text.as_str())
            {
                diags.push(LintDiagnostic {
                    rule: "private-method-access",
                    message: format!(
                        "accessing private method `{}` on external object",
                        method_text
                    ),
                    severity: Severity::Warning,
                    line: method_node.start_position().row,
                    column: method_node.start_position().column,
                    fix: None,
                    end_column: None,
                });
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::LintConfig;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        PrivateMethodAccess.check(&tree, source, &LintConfig::default())
    }

    #[test]
    fn private_method_on_external() {
        let src = "var other: Node = null\nfunc test() -> void:\n\tother._private_method()\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_private_method"));
    }

    #[test]
    fn self_private_no_warning() {
        let src = "func test() -> void:\n\tself._internal()\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn public_method_no_warning() {
        let src = "var other: Node = null\nfunc test() -> void:\n\tother.public_method()\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn allowed_callback_no_warning() {
        let src = "var obj: Object = null\nfunc test() -> void:\n\tobj._to_string()\n";
        assert!(check(src).is_empty());
    }
}
