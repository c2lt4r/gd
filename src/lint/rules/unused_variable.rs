use std::collections::HashMap;
use tree_sitter::{Node, Tree};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedVariable;

impl LintRule for UnusedVariable {
    fn name(&self) -> &'static str {
        "unused-variable"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();

        // Find all function definitions and check each scope
        collect_functions(root, source, &mut diags);

        diags
    }
}

fn collect_functions(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition"
        && let Some(body) = node.child_by_field_name("body")
    {
        check_function_body(body, source, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_functions(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Track variable declarations and references within a function body.
fn check_function_body(body: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Collect local variable declarations: name -> (line, col, name_byte_start)
    let mut declarations: HashMap<String, (usize, usize, usize)> = HashMap::new();
    // Collect all identifier references (not declarations)
    let mut references: std::collections::HashSet<String> = std::collections::HashSet::new();

    collect_declarations_and_refs(body, source, &mut declarations, &mut references);

    for (name, (line, col, name_byte_start)) in &declarations {
        // Skip variables starting with _ (intentionally unused)
        if name.starts_with('_') {
            continue;
        }
        if !references.contains(name.as_str()) {
            diags.push(LintDiagnostic {
                rule: "unused-variable",
                message: format!("variable `{name}` is assigned but never used"),
                severity: Severity::Warning,
                line: *line,
                column: *col,
                end_column: Some(*col + name.len()),
                fix: Some(Fix {
                    byte_start: *name_byte_start,
                    byte_end: *name_byte_start,
                    replacement: "_".to_string(),
                }),
                context_lines: None,
            });
        }
    }
}

fn collect_declarations_and_refs(
    node: Node,
    source: &str,
    declarations: &mut HashMap<String, (usize, usize, usize)>,
    references: &mut std::collections::HashSet<String>,
) {
    match node.kind() {
        "variable_statement" => {
            // This is a local var declaration
            if let Some(name_node) = node.child_by_field_name("name") {
                let name = source[name_node.byte_range()].to_string();
                declarations.insert(
                    name,
                    (
                        name_node.start_position().row,
                        name_node.start_position().column,
                        name_node.start_byte(),
                    ),
                );
            }
            // The value expression may reference other vars
            if let Some(value) = node.child_by_field_name("value") {
                collect_refs_only(value, source, references);
            }
        }
        "assignment" | "augmented_assignment" => {
            // The right side is a reference, but the left side identifier is a write
            // For assignments to local vars, we count this as a use of the RHS identifiers
            // The LHS is not a "reference" for unused detection unless it's complex
            // (subscript, attribute, etc.)
            let child_count = node.child_count();
            if child_count >= 3 {
                // left = node.child(0), right = last named child
                let left = node.child(0);
                let right = node.child(child_count - 1);
                if let Some(right) = right {
                    collect_refs_only(right, source, references);
                }
                // If left is a complex expression (attribute, subscript), count identifiers as refs
                if let Some(left) = left
                    && left.kind() != "identifier"
                {
                    collect_refs_only(left, source, references);
                }
            }
        }
        "identifier" => {
            let name = source[node.byte_range()].to_string();
            references.insert(name);
        }
        // Don't descend into nested function definitions (separate scope)
        "function_definition" | "lambda" => {}
        _ => {
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    collect_declarations_and_refs(cursor.node(), source, declarations, references);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
    }
}

fn collect_refs_only(node: Node, source: &str, references: &mut std::collections::HashSet<String>) {
    if node.kind() == "identifier" {
        let name = source[node.byte_range()].to_string();
        references.insert(name);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_refs_only(cursor.node(), source, references);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
