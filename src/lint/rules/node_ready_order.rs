use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NodeReadyOrder;

impl LintRule for NodeReadyOrder {
    fn name(&self) -> &'static str {
        "node-ready-order"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        find_init_functions(root, source, &mut diags);
        diags
    }
}

/// Find `_init()` functions and check for node access inside them.
fn find_init_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            // tree-sitter-gdscript parses _init() as constructor_definition
            if (child.kind() == "function_definition" || child.kind() == "constructor_definition")
                && let Some(body) = child.child_by_field_name("body")
            {
                // For function_definition, check the name field
                // For constructor_definition, the name is implicitly _init
                let is_init = if child.kind() == "constructor_definition" {
                    true
                } else if let Some(name_node) = child.child_by_field_name("name") {
                    name_node.utf8_text(source.as_bytes()).unwrap_or("") == "_init"
                } else {
                    false
                };

                if is_init {
                    find_node_access(body, source, diags);
                }
            }

            // Recurse into class bodies
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                find_init_functions(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Find `$NodePath` (get_node shorthand) and `get_node()` calls.
fn find_node_access(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    match node.kind() {
        // $NodePath syntax is parsed as `get_node` node in tree-sitter-gdscript
        "get_node" => {
            let text = node.utf8_text(source.as_bytes()).unwrap_or("$...");
            diags.push(LintDiagnostic {
                rule: "node-ready-order",
                message: format!(
                    "`{text}` in _init() may fail; nodes are not ready until _ready()"
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: Some(node.end_position().column),
                fix: None,
                context_lines: None,
            });
        }
        "call" => {
            // Check for get_node("...") or get_node_or_null("...") calls
            if let Some(func) = call_function_name(node, source)
                && (func == "get_node"
                    || func == "get_node_or_null"
                    || func.ends_with(".get_node")
                    || func.ends_with(".get_node_or_null"))
            {
                diags.push(LintDiagnostic {
                    rule: "node-ready-order",
                    message: format!(
                        "`{func}(...)` in _init() may fail; nodes are not ready until _ready()"
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
        // Don't recurse into nested function definitions (separate scope)
        _ => {}
    }

    // Recurse into children (unless we already returned for function/lambda)
    if !matches!(
        node.kind(),
        "function_definition" | "constructor_definition" | "lambda"
    ) {
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                find_node_access(cursor.node(), source, diags);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
}

/// Get the function name from a call node, trying field name first, then named_child(0).
fn call_function_name<'a>(node: Node<'a>, source: &'a str) -> Option<&'a str> {
    node.child_by_field_name("function")
        .or_else(|| node.named_child(0))
        .map(|n| &source[n.byte_range()])
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
        NodeReadyOrder.check(&tree, source, &LintConfig::default())
    }

    #[test]
    fn dollar_sign_in_init() {
        let src = "func _init() -> void:\n\tvar child: Node = $Something\n\tprint(child)\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$Something"));
        assert!(diags[0].message.contains("_init()"));
    }

    #[test]
    fn get_node_call_in_init() {
        let src = "func _init() -> void:\n\tvar child = get_node(\"Sprite\")\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node"));
    }

    #[test]
    fn no_warning_in_ready() {
        let src = "func _ready() -> void:\n\tvar child: Node = $Something\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn no_warning_normal_function() {
        let src = "func setup() -> void:\n\tvar child: Node = $Something\n";
        assert!(check(src).is_empty());
    }
}
