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
        check_scope(root, source, &mut diags);
        diags
    }
}

/// Check all functions within a scope (file top-level or class body).
/// Two-pass: first detect if the scope contains virtual stubs (empty functions
/// with all-`_`-prefixed params), then emit warnings skipping zero-param
/// empty functions that are siblings of virtual stubs.
fn check_scope(scope: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Pass 1: find empty functions and check for virtual stubs in this scope
    let mut empty_funcs: Vec<Node> = Vec::new();
    let mut has_virtual_stubs = false;

    let mut cursor = scope.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            if child.kind() == "function_definition"
                && is_empty_body(child)
                && !has_annotation(child, source, "abstract")
                && !prev_sibling_has_annotation(child, source, "abstract")
            {
                empty_funcs.push(child);
                if is_virtual_stub(child, source) {
                    has_virtual_stubs = true;
                }
            }

            // Recurse into class bodies (separate scope)
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                check_scope(body, source, diags);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    // Pass 2: emit warnings, skipping zero-param functions in virtual stub scopes
    for func in empty_funcs {
        if is_virtual_stub(func, source) {
            continue;
        }

        // Zero-param private function alongside virtual stubs → likely a virtual stub too
        // (e.g. _on_exit() next to _on_enter(_msg), _on_update(_delta))
        if has_virtual_stubs && param_count(func) == 0 {
            let name = func
                .child_by_field_name("name")
                .map(|n| &source[n.byte_range()])
                .unwrap_or("");
            if name.starts_with('_') {
                continue;
            }
        }

        let func_name = func
            .child_by_field_name("name")
            .map(|n| &source[n.byte_range()])
            .unwrap_or("<unknown>");
        diags.push(LintDiagnostic {
            rule: "empty-function",
            message: format!("function `{}` has an empty body (only `pass`)", func_name),
            severity: Severity::Warning,
            line: func.start_position().row,
            column: func.start_position().column,
            fix: None,
            end_column: None,
            context_lines: None,
        });
    }
}

/// Check if a node has a specific annotation (e.g. `@abstract`).
fn has_annotation(node: Node, source: &str, annotation_name: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "annotations" {
            let mut annot_cursor = child.walk();
            for annot in child.children(&mut annot_cursor) {
                if annot.kind() == "annotation" {
                    let mut ident_cursor = annot.walk();
                    for ident_child in annot.children(&mut ident_cursor) {
                        if ident_child.kind() == "identifier"
                            && ident_child.utf8_text(source.as_bytes()).ok()
                                == Some(annotation_name)
                        {
                            return true;
                        }
                    }
                }
            }
        }
    }
    false
}

/// Check if the previous sibling is an annotation node containing a specific annotation.
/// Tree-sitter puts annotations on separate lines as sibling `annotation` (singular) or
/// `annotations` (plural) nodes rather than children of the function.
fn prev_sibling_has_annotation(node: Node, source: &str, annotation_name: &str) -> bool {
    let Some(prev) = node.prev_named_sibling() else {
        return false;
    };
    match prev.kind() {
        // Single annotation on its own line: annotation > identifier
        "annotation" => {
            let mut cursor = prev.walk();
            for child in prev.children(&mut cursor) {
                if child.kind() == "identifier"
                    && child.utf8_text(source.as_bytes()).ok() == Some(annotation_name)
                {
                    return true;
                }
            }
        }
        // Multiple annotations block: annotations > annotation > identifier
        "annotations" => {
            let mut cursor = prev.walk();
            for annot in prev.children(&mut cursor) {
                if annot.kind() == "annotation" {
                    let mut ident_cursor = annot.walk();
                    for child in annot.children(&mut ident_cursor) {
                        if child.kind() == "identifier"
                            && child.utf8_text(source.as_bytes()).ok() == Some(annotation_name)
                        {
                            return true;
                        }
                    }
                }
            }
        }
        _ => {}
    }
    false
}

/// Check if a function has only `pass` in its body.
fn is_empty_body(func: Node) -> bool {
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    // Count non-comment named children
    let stmts: Vec<_> = (0..body.named_child_count())
        .filter_map(|i| body.named_child(i))
        .filter(|c| c.kind() != "comment")
        .collect();
    stmts.len() == 1 && stmts[0].kind() == "pass_statement"
}

/// A function is a virtual stub if it has at least one parameter and every
/// parameter name starts with `_`.
fn is_virtual_stub(func: Node, source: &str) -> bool {
    let Some(params) = func.child_by_field_name("parameters") else {
        return false;
    };

    let src = source.as_bytes();
    let mut count = 0;

    let mut cursor = params.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if matches!(
                child.kind(),
                "identifier" | "typed_parameter" | "default_parameter" | "typed_default_parameter"
            ) {
                count += 1;
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

    count > 0
}

/// Count the number of parameters in a function.
fn param_count(func: Node) -> usize {
    let Some(params) = func.child_by_field_name("parameters") else {
        return 0;
    };

    let mut count = 0;
    let mut cursor = params.walk();
    if cursor.goto_first_child() {
        loop {
            if matches!(
                cursor.node().kind(),
                "identifier" | "typed_parameter" | "default_parameter" | "typed_default_parameter"
            ) {
                count += 1;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    count
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
    fn no_warning_on_zero_param_sibling_of_virtual_stub() {
        // _on_exit has no params but is alongside _on_enter which is a virtual stub
        let source = "func _on_enter(_msg: Dictionary) -> void:\n\tpass\n\nfunc _on_exit() -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_isolated_zero_param_empty() {
        // No virtual stub siblings → should warn
        let source = "func _on_exit() -> void:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_standalone_empty_handler() {
        // Empty signal handler with no virtual stub context
        let source = "func _on_button_pressed() -> void:\n\tpass\n\nfunc do_stuff() -> void:\n\tprint(\"hi\")\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn no_warning_on_abstract_function() {
        // @abstract on same line as func
        let source = "@abstract func draw():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_abstract_function_separate_line() {
        // @abstract on separate line from func
        let source = "@abstract\nfunc draw():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_non_abstract_empty() {
        // @abstract on the class, not the function — function should still warn
        let source = "func draw():\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_public_empty_alongside_stubs() {
        // do_nothing is public (no _ prefix) so should still warn even with virtual stubs nearby
        let source = "func _on_enter(_msg: Dictionary) -> void:\n\tpass\n\nfunc do_nothing() -> void:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
        assert_eq!(
            check(source)[0].message,
            "function `do_nothing` has an empty body (only `pass`)"
        );
    }
}
