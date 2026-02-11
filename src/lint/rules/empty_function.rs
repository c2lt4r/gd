use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct EmptyFunction;

impl LintRule for EmptyFunction {
    fn name(&self) -> &'static str {
        "empty-function"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition"
        && let Some(body) = node.child_by_field_name("body")
    {
        // An empty function body has exactly one named child: a pass_statement
        let named_count = body.named_child_count();
        if named_count == 1
            && let Some(first) = body.named_child(0)
            && first.kind() == "pass_statement"
        {
            // Skip virtual method stubs: functions where all parameters
            // are prefixed with _ (the GDScript convention for intentionally
            // unused parameters). This pattern indicates the function is a
            // base class virtual method meant to be overridden by subclasses.
            if is_virtual_stub(node, source) {
                // Still recurse into children below
            } else {
                let func_name = node
                    .child_by_field_name("name")
                    .map(|n| &source[n.byte_range()])
                    .unwrap_or("<unknown>");
                diags.push(LintDiagnostic {
                    rule: "empty-function",
                    message: format!("function `{}` has an empty body (only `pass`)", func_name),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
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

/// A function is a virtual stub if it has at least one parameter and every
/// parameter name starts with `_`. This is the standard GDScript convention
/// for base class methods meant to be overridden.
fn is_virtual_stub(func: Node, source: &str) -> bool {
    let params = match func.child_by_field_name("parameters") {
        Some(p) => p,
        None => return false,
    };

    let src = source.as_bytes();
    let mut param_count = 0;

    let mut cursor = params.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let is_param = matches!(
                child.kind(),
                "identifier" | "typed_parameter" | "default_parameter" | "typed_default_parameter"
            );
            if is_param {
                param_count += 1;
                let name = match child.kind() {
                    "identifier" => child.utf8_text(src).unwrap_or(""),
                    _ => child
                        .child(0)
                        .and_then(|n| n.utf8_text(src).ok())
                        .unwrap_or(""),
                };
                if !name.starts_with('_') {
                    return false;
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    param_count > 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        EmptyFunction.check(&tree, source, &config)
    }

    #[test]
    fn warns_on_empty_function() {
        let source = "func do_nothing():\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn no_warning_on_virtual_stub() {
        let source = "func _on_enter(_msg: Dictionary) -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_multi_param_virtual_stub() {
        let source = "func _on_update(_delta: float, _state: int) -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_when_not_all_params_prefixed() {
        let source = "func process(delta: float) -> void:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_zero_param_empty_function() {
        let source = "func _on_exit() -> void:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }
}
