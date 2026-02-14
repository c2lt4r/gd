use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MissingReturn;

impl LintRule for MissingReturn {
    fn name(&self) -> &'static str {
        "missing-return"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        find_typed_functions(root, source, &mut diags);
        diags
    }
}

fn find_typed_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "function_definition" {
                check_function(child, source, diags);
            }

            // Recurse into class bodies
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                find_typed_functions(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_function(func: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();

    // Must have a return type annotation
    let Some(return_type) = func.child_by_field_name("return_type") else {
        return;
    };

    let type_text = return_type.utf8_text(src).unwrap_or("");
    if type_text == "void" {
        return;
    }

    // Get the function body
    let Some(body) = func.child_by_field_name("body") else {
        return;
    };

    if body_always_returns(body, source) {
        return;
    }

    emit_warning(func, source, diags);
}

/// Check if a body node always returns on every code path.
fn body_always_returns(body: Node, source: &str) -> bool {
    let Some(last) = last_statement(body) else {
        return false;
    };

    node_always_returns(last, source)
}

/// Check if a node (the last statement in a body) always returns.
fn node_always_returns(node: Node, source: &str) -> bool {
    match node.kind() {
        "return_statement" => true,
        "if_statement" => if_always_returns(node, source),
        "match_statement" => match_always_returns(node, source),
        _ => false,
    }
}

/// Check if an if/elif/else chain always returns (needs an else branch
/// and every branch must return).
fn if_always_returns(node: Node, source: &str) -> bool {
    // The if body
    let Some(if_body) = node.child_by_field_name("body") else {
        return false;
    };
    if !body_always_returns(if_body, source) {
        return false;
    }

    // Walk children looking for elif_clause and else_clause
    let mut has_else = false;
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            match child.kind() {
                "elif_clause" => {
                    if let Some(body) = child.child_by_field_name("body") {
                        if !body_always_returns(body, source) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                "else_clause" => {
                    has_else = true;
                    if let Some(body) = child.child_by_field_name("body") {
                        if !body_always_returns(body, source) {
                            return false;
                        }
                    } else {
                        return false;
                    }
                }
                _ => {}
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Without an else branch, the if can fall through
    has_else
}

/// Check if a match statement always returns (needs a wildcard arm
/// and every arm must return).
fn match_always_returns(node: Node, source: &str) -> bool {
    let match_body = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "match_body");
    let Some(match_body) = match_body else {
        return false;
    };

    let mut has_wildcard = false;
    let mut cursor = match_body.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "pattern_section" {
                // Check for wildcard pattern (_)
                if has_wildcard_pattern(child, source) {
                    has_wildcard = true;
                }
                // Check the arm's body returns
                if let Some(body) = child.child_by_field_name("body") {
                    if !body_always_returns(body, source) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Without a wildcard, the match can fall through
    has_wildcard
}

/// Check if a pattern_section contains a wildcard (_) pattern.
fn has_wildcard_pattern(section: Node, source: &str) -> bool {
    let mut cursor = section.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            // The wildcard is an identifier node with text "_"
            if child.kind() == "identifier"
                && child.utf8_text(source.as_bytes()).unwrap_or("") == "_"
            {
                return true;
            }
            // Stop at the body — patterns come before it
            if child.kind() == "body" {
                break;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

/// Get the last non-comment named child of a body node.
fn last_statement(body: Node) -> Option<Node> {
    let count = body.named_child_count();
    for i in (0..count).rev() {
        if let Some(child) = body.named_child(i)
            && child.kind() != "comment"
        {
            return Some(child);
        }
    }
    None
}

fn emit_warning(func: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let name = func
        .child_by_field_name("name")
        .map_or("?", |n| n.utf8_text(source.as_bytes()).unwrap_or("?"));

    diags.push(LintDiagnostic {
        rule: "missing-return",
        message: format!(
            "function `{name}` has a return type but may not return a value",
        ),
        severity: Severity::Warning,
        line: func.start_position().row,
        column: func.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        MissingReturn.check(&tree, source, &config)
    }

    #[test]
    fn no_warning_on_match_with_all_returns() {
        let source = "func f(x: int) -> String:\n\tmatch x:\n\t\t0:\n\t\t\treturn \"a\"\n\t\t1:\n\t\t\treturn \"b\"\n\t\t_:\n\t\t\treturn \"c\"\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_if_elif_else_all_return() {
        let source = "func f(x: int) -> int:\n\tif x > 10:\n\t\treturn 1\n\telif x > 5:\n\t\treturn 2\n\telse:\n\t\treturn 3\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_match_without_wildcard() {
        let source = "func f(x: int) -> String:\n\tmatch x:\n\t\t0:\n\t\t\treturn \"a\"\n\t\t1:\n\t\t\treturn \"b\"\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_if_without_else() {
        let source = "func f(x: int) -> int:\n\tif x > 10:\n\t\treturn 1\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_empty_body() {
        let source = "func f() -> int:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn no_warning_on_direct_return() {
        let source = "func f() -> int:\n\treturn 42\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_void() {
        let source = "func f() -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }
}
