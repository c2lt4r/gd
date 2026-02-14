use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct GetNodeInProcess;

impl LintRule for GetNodeInProcess {
    fn name(&self) -> &'static str {
        "get-node-in-process"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        find_process_functions(root, source, &mut diags);
        diags
    }
}

fn find_process_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "function_definition"
                && let Some(name_node) = child.child_by_field_name("name")
            {
                let name = name_node.utf8_text(source.as_bytes()).unwrap_or("");
                if matches!(name, "_process" | "_physics_process")
                    && let Some(body) = child.child_by_field_name("body")
                {
                    find_get_node_calls(body, source, name, diags);
                }
            }

            // Recurse into class bodies
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                find_process_functions(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn find_get_node_calls(node: Node, source: &str, func_name: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();

    // $NodePath syntax → tree-sitter parses as `get_node` node type
    if node.kind() == "get_node" {
        let text = node.utf8_text(src).unwrap_or("$...");
        diags.push(LintDiagnostic {
            rule: "get-node-in-process",
            message: format!(
                "`{text}` in {func_name}() is called every frame; cache it in an @onready var"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: None,
            context_lines: None,
        });
    }

    // Bare call: get_node("path") or get_node_or_null("path")
    if node.kind() == "call" {
        let callee = node
            .children(&mut node.walk())
            .find(|c| c.kind() == "identifier")
            .and_then(|n| n.utf8_text(src).ok())
            .unwrap_or("");
        if matches!(callee, "get_node" | "get_node_or_null") {
            diags.push(LintDiagnostic {
                rule: "get-node-in-process",
                message: format!(
                    "`{callee}()` in {func_name}() is called every frame; cache it in an @onready var"
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

    // Method call: self.get_node("path") → attribute > [identifier, attribute_call]
    if node.kind() == "attribute" {
        check_attribute_get_node(node, source, func_name, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            // Don't recurse into nested function definitions or lambdas
            if child.kind() != "function_definition" && child.kind() != "lambda" {
                find_get_node_calls(child, source, func_name, diags);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_attribute_get_node(
    node: Node,
    source: &str,
    func_name: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    let src = source.as_bytes();

    // Look for attribute_call child whose identifier is get_node or get_node_or_null
    for child in node.children(&mut node.walk()) {
        if child.kind() == "attribute_call" {
            let method = child
                .children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
                .and_then(|n| n.utf8_text(src).ok())
                .unwrap_or("");
            if matches!(method, "get_node" | "get_node_or_null") {
                diags.push(LintDiagnostic {
                    rule: "get-node-in-process",
                    message: format!(
                        "`{method}()` in {func_name}() is called every frame; cache it in an @onready var"
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        GetNodeInProcess.check(&tree, source, &config)
    }

    #[test]
    fn detects_dollar_node_path_in_process() {
        let source = "func _process(delta: float) -> void:\n\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "get-node-in-process");
        assert!(diags[0].message.contains("$Sprite2D"));
        assert!(diags[0].message.contains("_process()"));
    }

    #[test]
    fn detects_get_node_call_in_process() {
        let source = "func _process(delta: float) -> void:\n\tvar n := get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node()"));
    }

    #[test]
    fn detects_get_node_or_null_in_process() {
        let source =
            "func _process(delta: float) -> void:\n\tvar n := get_node_or_null(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node_or_null()"));
    }

    #[test]
    fn detects_in_physics_process() {
        let source = "func _physics_process(delta: float) -> void:\n\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_physics_process()"));
    }

    #[test]
    fn detects_self_get_node_in_process() {
        let source =
            "func _process(delta: float) -> void:\n\tvar n := self.get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node()"));
    }

    #[test]
    fn no_warning_in_ready() {
        let source = "func _ready() -> void:\n\tvar sprite := $Sprite2D\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_in_regular_function() {
        let source = "func setup() -> void:\n\tvar n := get_node(\"Sprite2D\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_in_same_function() {
        let source = "func _process(delta: float) -> void:\n\tvar a := $Sprite2D\n\tvar b := get_node(\"Label\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn no_warning_for_nested_function() {
        let source = "func _process(delta: float) -> void:\n\tpass\n\nfunc helper() -> void:\n\tvar n := get_node(\"Sprite2D\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_in_inner_class() {
        let source =
            "class Inner:\n\tfunc _process(delta: float) -> void:\n\t\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_nested_dollar_in_conditional() {
        let source =
            "func _process(delta: float) -> void:\n\tif true:\n\t\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
