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
    let return_type = match func.child_by_field_name("return_type") {
        Some(rt) => rt,
        None => return,
    };

    let type_text = return_type.utf8_text(src).unwrap_or("");
    if type_text == "void" {
        return;
    }

    // Get the function body
    let body = match func.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    // Check if the last named child is a return statement
    let child_count = body.named_child_count();
    if child_count == 0 {
        // Empty body with a return type - warn
        emit_warning(func, source, diags);
        return;
    }

    let last_child = body.named_child(child_count - 1).unwrap();
    if last_child.kind() != "return_statement" {
        emit_warning(func, source, diags);
    }
}

fn emit_warning(func: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let name = func
        .child_by_field_name("name")
        .map(|n| n.utf8_text(source.as_bytes()).unwrap_or("?"))
        .unwrap_or("?");

    diags.push(LintDiagnostic {
        rule: "missing-return",
        message: format!(
            "function `{}` has a return type but may not return a value",
            name,
        ),
        severity: Severity::Warning,
        line: func.start_position().row,
        column: func.start_position().column,
        end_column: None,
        fix: None,
    });
}
