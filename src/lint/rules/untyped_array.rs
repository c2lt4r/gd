use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UntypedArray;

impl LintRule for UntypedArray {
    fn name(&self) -> &'static str {
        "untyped-array"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "variable_statement" {
        // Check if this is a const (skip constants)
        if let Some(first_child) = node.named_child(0) {
            let text = &source[first_child.byte_range()];
            if text == "const" {
                // Skip constants
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        check_node(cursor.node(), source, diags);
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                return;
            }
        }

        // Check if value is an array
        let has_array_value = node
            .child_by_field_name("value")
            .is_some_and(|n| n.kind() == "array");

        // Check if there's a type annotation
        let has_type = node.child_by_field_name("type").is_some();

        if has_array_value
            && !has_type
            && let Some(name_node) = node.child_by_field_name("name")
        {
            diags.push(LintDiagnostic {
                rule: "untyped-array",
                message: "array variable has no type annotation; consider `Array[Type]`"
                    .to_string(),
                severity: Severity::Warning,
                line: name_node.start_position().row,
                column: name_node.start_position().column,
                fix: None,
                end_column: None,
                context_lines: None,
            });
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
