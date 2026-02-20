use std::collections::HashSet;
use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct CallableNullCheck;

impl LintRule for CallableNullCheck {
    fn name(&self) -> &'static str {
        "callable-null-check"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let src = source.as_bytes();
        check_functions(root, src, &mut diags);
        diags
    }
}

fn check_functions(node: Node, src: &[u8], diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition" || node.kind() == "constructor_definition" {
        check_function_body(node, src, diags);
        return;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_functions(cursor.node(), src, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_function_body(func: Node, src: &[u8], diags: &mut Vec<LintDiagnostic>) {
    let Some(body) = func.child_by_field_name("body") else {
        return;
    };

    // First pass: collect identifiers that have .is_valid() checks
    let mut validated: HashSet<String> = HashSet::new();
    collect_validated(body, src, &mut validated);

    // Second pass: find .call() / .call_deferred() / .callv() without validation
    find_unvalidated_calls(body, src, &validated, diags);
}

/// Collect identifiers that appear as `foo.is_valid()` or `foo != null` or `foo == null`.
fn collect_validated(node: Node, src: &[u8], validated: &mut HashSet<String>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            // Pattern: foo.is_valid() → attribute > [identifier "foo", attribute_call > identifier "is_valid"]
            if child.kind() == "attribute" {
                check_is_valid(child, src, validated);
            }

            // Pattern: foo != null / foo == null → binary_operator
            if child.kind() == "binary_operator" {
                check_null_compare(child, src, validated);
            }

            // Pattern: if foo: (truthiness check on the callable)
            if child.kind() == "if_statement" {
                check_truthiness(child, src, validated);
            }

            collect_validated(child, src, validated);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Recursively collect all identifiers within a node.
fn collect_all_identifiers<'a>(node: Node<'a>, src: &[u8], out: &mut Vec<(String, Node<'a>)>) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "identifier"
                && let Ok(text) = child.utf8_text(src)
            {
                out.push((text.to_string(), child));
            } else if child.kind() != "attribute_call" {
                // Recurse into everything except attribute_call (don't collect method name)
                collect_all_identifiers(child, src, out);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_is_valid(node: Node, src: &[u8], validated: &mut HashSet<String>) {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }

    let mut has_is_valid = false;

    loop {
        let child = cursor.node();

        if child.kind() == "attribute_call"
            && let Some(method) = child
                .children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
            && let Ok(name) = method.utf8_text(src)
            && (name == "is_valid" || name == "is_null")
        {
            has_is_valid = true;
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    if has_is_valid {
        let mut ids = Vec::new();
        collect_all_identifiers(node, src, &mut ids);
        if let Some((name, _)) = ids.last() {
            validated.insert(name.clone());
        }
    }
}

fn check_null_compare(node: Node, src: &[u8], validated: &mut HashSet<String>) {
    // binary_operator: left op right
    // Look for `foo != null` or `foo == null`
    let child_count = node.child_count();
    if child_count < 3 {
        return;
    }

    let op = node
        .child_by_field_name("op")
        .and_then(|n| n.utf8_text(src).ok())
        .unwrap_or("");

    if op != "!=" && op != "==" {
        return;
    }

    let left = node.child(0);
    let right = node.child(child_count - 1);

    if let (Some(left), Some(right)) = (left, right) {
        let left_text = left.utf8_text(src).unwrap_or("");
        let right_text = right.utf8_text(src).unwrap_or("");

        if right_text == "null" && left.kind() == "identifier" {
            validated.insert(left_text.to_string());
        } else if left_text == "null" && right.kind() == "identifier" {
            validated.insert(right_text.to_string());
        }
    }
}

fn check_truthiness(node: Node, src: &[u8], validated: &mut HashSet<String>) {
    // if_statement > condition (first named child after "if")
    // If the condition is just an identifier, it's a truthiness check
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "identifier"
                && child.start_position().row == node.start_position().row
                && let Ok(name) = child.utf8_text(src)
            {
                validated.insert(name.to_string());
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Find .call() / .call_deferred() / .callv() on identifiers not in the validated set.
fn find_unvalidated_calls(
    node: Node,
    src: &[u8],
    validated: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            if child.kind() == "attribute" {
                check_callable_call(child, src, validated, diags);
            }

            find_unvalidated_calls(child, src, validated, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_callable_call(
    node: Node,
    src: &[u8],
    validated: &HashSet<String>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }

    let mut call_method = None;

    loop {
        let child = cursor.node();

        if child.kind() == "attribute_call"
            && let Some(method) = child
                .children(&mut child.walk())
                .find(|c| c.kind() == "identifier")
            && let Ok(name) = method.utf8_text(src)
            && matches!(name, "call" | "call_deferred" | "callv")
        {
            call_method = Some(name.to_string());
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    if let Some(method) = call_method {
        let mut ids = Vec::new();
        collect_all_identifiers(node, src, &mut ids);
        if let Some((obj_name, obj_node)) = ids.last()
            && obj_name != "self"
            && !validated.contains(obj_name)
        {
            diags.push(LintDiagnostic {
                rule: "callable-null-check",
                message: format!(
                    "`{obj_name}.{method}()` called without `{obj_name}.is_valid()` check"
                ),
                severity: Severity::Warning,
                line: obj_node.start_position().row,
                column: obj_node.start_position().column,
                end_column: None,
                fix: None,
                context_lines: None,
            });
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
        CallableNullCheck.check(&tree, source, &config)
    }

    #[test]
    fn detects_call_without_check() {
        let source = "func f(callback: Callable) -> void:\n\tcallback.call()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "callable-null-check");
        assert!(diags[0].message.contains("callback.call()"));
        assert!(diags[0].message.contains("is_valid"));
    }

    #[test]
    fn no_warning_with_is_valid() {
        let source =
            "func f(callback: Callable) -> void:\n\tif callback.is_valid():\n\t\tcallback.call()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_with_null_check() {
        let source =
            "func f(callback: Callable) -> void:\n\tif callback != null:\n\t\tcallback.call()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_with_truthiness_check() {
        let source = "func f(callback: Callable) -> void:\n\tif callback:\n\t\tcallback.call()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_call_deferred() {
        let source = "func f(cb: Callable) -> void:\n\tcb.call_deferred()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("call_deferred"));
    }

    #[test]
    fn no_warning_on_self_call() {
        // self.call() is a legitimate pattern (calling own method)
        let source = "func f() -> void:\n\tself.call(\"method\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_callv() {
        let source = "func f(cb: Callable) -> void:\n\tcb.callv([])\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_without_callable_call() {
        let source = "func f(node: Node) -> void:\n\tnode.process()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn chained_is_valid_guards_chained_call() {
        // Pattern from user report: server.hitscan_validator.is_valid() guards
        // server.hitscan_validator.call() across if/body boundary
        let source = "func f(server) -> void:\n\tif server and server.hitscan_validator.is_valid():\n\t\tserver.hitscan_validator.call(1, 2)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn chained_call_without_is_valid_warns() {
        // Same chained pattern but WITHOUT the is_valid guard — should warn
        let source = "func f(server) -> void:\n\tserver.hitscan_validator.call(1, 2)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("hitscan_validator"));
    }

    #[test]
    fn default_enabled() {
        assert!(CallableNullCheck.default_enabled());
    }
}
