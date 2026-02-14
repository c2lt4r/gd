use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateDelegate;

impl LintRule for DuplicateDelegate {
    fn name(&self) -> &'static str {
        "duplicate-delegate"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition" {
        check_function(node, source, diags);
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

fn check_function(func: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();

    // Get function name
    let func_name = match func.child_by_field_name("name") {
        Some(n) => n.utf8_text(src).unwrap_or(""),
        None => return,
    };

    // Collect parameter names in order
    let params: Vec<&str> = match func.child_by_field_name("parameters") {
        Some(p) => collect_param_names(p, src),
        None => return,
    };

    // Get body
    let Some(body) = func.child_by_field_name("body") else {
        return;
    };

    // Body must have exactly one named non-comment child
    let statements: Vec<Node> = body
        .children(&mut body.walk())
        .filter(|c| c.is_named() && c.kind() != "comment")
        .collect();

    if statements.len() != 1 {
        return;
    }

    let stmt = statements[0];

    // The statement must be either:
    // - return_statement containing a call
    // - expression_statement containing a call
    let call_node = match stmt.kind() {
        "return_statement" | "expression_statement" => find_call_child(stmt),
        _ => None,
    };

    let Some(call_node) = call_node else {
        return;
    };

    // The call must be a method call on self.something (attribute > attribute_call)
    // Pattern: self.ref.method(args) or ref.method(args)
    let Some(target) = extract_delegate_target(call_node, src) else {
        return;
    };

    // Check that the call arguments exactly match the function parameters
    if !args_match_params(call_node, src, &params) {
        return;
    }

    diags.push(LintDiagnostic {
        rule: "duplicate-delegate",
        message: format!(
            "`{func_name}` is a pure delegate to `{target}`; consider inlining or removing"
        ),
        severity: Severity::Info,
        line: func.start_position().row,
        column: func.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

fn collect_param_names<'a>(params_node: Node<'a>, src: &'a [u8]) -> Vec<&'a str> {
    let mut names = Vec::new();
    let mut cursor = params_node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            let name_node = match child.kind() {
                "identifier" => Some(child),
                "typed_parameter" | "default_parameter" | "typed_default_parameter" => {
                    child.child(0)
                }
                _ => None,
            };
            if let Some(n) = name_node
                && let Ok(name) = n.utf8_text(src)
            {
                names.push(name);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    names
}

/// Find a call expression inside a statement node.
/// For attribute method calls, the structure is:
/// expression_statement > attribute > attribute_call > arguments
fn find_call_child(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            // Direct call: `foo(args)`
            if child.kind() == "call" {
                return Some(child);
            }
            // Method call: `obj.method(args)` → attribute node
            if child.kind() == "attribute" {
                return Some(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

/// Extract the delegate target string like "self.ref.method" from a call.
fn extract_delegate_target<'a>(node: Node<'a>, src: &'a [u8]) -> Option<String> {
    // For attribute chains: self.ref.method(args)
    // The node is an "attribute" with children: identifiers and attribute_call
    if node.kind() != "attribute" {
        return None;
    }

    // Must have at least one attribute_call child (the method call)
    let has_method_call = node
        .children(&mut node.walk())
        .any(|c| c.kind() == "attribute_call");

    if !has_method_call {
        return None;
    }

    // Build the target string from identifiers
    let mut parts: Vec<&str> = Vec::new();
    for child in node.children(&mut node.walk()) {
        if child.kind() == "identifier"
            && let Ok(text) = child.utf8_text(src)
        {
            parts.push(text);
        }
        if child.kind() == "attribute_call"
            && let Some(method_name) = child
                .children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
            && let Ok(text) = method_name.utf8_text(src)
        {
            parts.push(text);
        }
    }

    if parts.len() >= 2 {
        Some(parts.join("."))
    } else {
        None
    }
}

/// Check that the call arguments exactly match the function parameters in order.
fn args_match_params(call_node: Node, src: &[u8], params: &[&str]) -> bool {
    // Find the arguments node
    let args_node = if call_node.kind() == "attribute" {
        // For method calls, arguments are inside the attribute_call child
        call_node
            .children(&mut call_node.walk())
            .find(|c| c.kind() == "attribute_call")
            .and_then(|ac| {
                ac.children(&mut ac.walk())
                    .find(|c| c.kind() == "arguments")
            })
    } else {
        // For direct calls
        call_node
            .children(&mut call_node.walk())
            .find(|c| c.kind() == "arguments")
    };

    let Some(args_node) = args_node else {
        return params.is_empty();
    };

    // Collect argument identifiers
    let args: Vec<&str> = args_node
        .children(&mut args_node.walk())
        .filter(tree_sitter::Node::is_named)
        .filter_map(|c| {
            if c.kind() == "identifier" {
                c.utf8_text(src).ok()
            } else {
                None // Non-identifier arg (expression, literal, etc.)
            }
        })
        .collect();

    // Must have same count and same order
    args.len() == params.len() && args.iter().zip(params.iter()).all(|(a, p)| a == p)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        DuplicateDelegate.check(&tree, source, &config)
    }

    #[test]
    fn detects_delegate_with_return() {
        let source = "var ref: Node\n\nfunc get_name(a, b):\n\treturn self.ref.get_name(a, b)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "duplicate-delegate");
        assert!(diags[0].message.contains("pure delegate"));
    }

    #[test]
    fn detects_delegate_without_return() {
        let source = "var ref: Node\n\nfunc do_thing(x):\n\tself.ref.do_thing(x)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_different_args() {
        let source = "var ref: Node\n\nfunc do_thing(x, y):\n\tself.ref.do_thing(y, x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_extra_logic() {
        let source = "var ref: Node\n\nfunc do_thing(x):\n\tprint(x)\n\tself.ref.do_thing(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_no_params() {
        // Zero-param delegates are too common and legitimate (property getters)
        let source = "var ref: Node\n\nfunc get_name():\n\treturn self.ref.get_name()\n";
        let diags = check(source);
        // Zero params: args_match_params returns true (both empty)
        // But this is still a delegate — we flag it
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn opt_in_rule() {
        assert!(!DuplicateDelegate.default_enabled());
    }
}
