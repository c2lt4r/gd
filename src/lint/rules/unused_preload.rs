use std::collections::{HashMap, HashSet};
use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct UnusedPreload;

impl LintRule for UnusedPreload {
    fn name(&self) -> &'static str {
        "unused-preload"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let src = source.as_bytes();

        // Collect all `var X = preload(...)` declarations
        let mut preloads: HashMap<String, (usize, usize, usize)> = HashMap::new(); // name -> (line, col, end_col)
        collect_preload_vars(root, src, source, &mut preloads);

        if preloads.is_empty() {
            return diags;
        }

        // Collect all identifier references across the entire file
        let mut references: HashSet<String> = HashSet::new();
        collect_all_references(root, src, &mut references);

        // Report preloaded vars that are never referenced elsewhere
        for (name, (line, col, end_col)) in &preloads {
            // The declaration itself counts as one reference (the assignment),
            // so we check if the name appears as an identifier reference anywhere else.
            // Since collect_all_references skips variable_statement name fields,
            // any reference found means it's actually used.
            if !references.contains(name.as_str()) {
                diags.push(LintDiagnostic {
                    rule: "unused-preload",
                    message: format!(
                        "preloaded variable `{}` is never used",
                        name
                    ),
                    severity: Severity::Warning,
                    line: *line,
                    column: *col,
                    end_column: Some(*end_col),
                    fix: None,
                });
            }
        }

        diags
    }
}

/// Find all `var X = preload(...)` or `var X = load(...)` at module scope.
fn collect_preload_vars(
    node: Node,
    src: &[u8],
    source: &str,
    preloads: &mut HashMap<String, (usize, usize, usize)>,
) {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();

            if child.kind() == "variable_statement" {
                // Check if the value is a preload() or load() call
                if let Some(value) = child.child_by_field_name("value")
                    && is_preload_call(&value, source)
                    && let Some(name_node) = child.child_by_field_name("name")
                {
                    let name = name_node.utf8_text(src).unwrap_or("").to_string();
                    if !name.is_empty() && !name.starts_with('_') {
                        let line = name_node.start_position().row;
                        let col = name_node.start_position().column;
                        let end_col = col + name.len();
                        preloads.insert(name, (line, col, end_col));
                    }
                }
            }

            // Recurse into class bodies
            if child.kind() == "class_definition"
                && let Some(body) = child.child_by_field_name("body")
            {
                collect_preload_vars(body, src, source, preloads);
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_preload_call(node: &Node, source: &str) -> bool {
    if node.kind() == "call"
        && let Some(func) = node.child_by_field_name("function")
    {
        let name = &source[func.byte_range()];
        return name == "preload" || name == "load";
    }
    false
}

/// Collect all identifier references, skipping declaration name positions.
fn collect_all_references(node: Node, src: &[u8], refs: &mut HashSet<String>) {
    match node.kind() {
        "variable_statement" => {
            // Skip the name field (that's the declaration), but check value and type
            if let Some(value) = node.child_by_field_name("value") {
                collect_all_references(value, src, refs);
            }
            if let Some(ty) = node.child_by_field_name("type") {
                collect_all_references(ty, src, refs);
            }
        }
        "identifier" => {
            let name = node.utf8_text(src).unwrap_or("");
            if !name.is_empty() {
                refs.insert(name.to_string());
            }
        }
        _ => {
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    collect_all_references(cursor.node(), src, refs);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
    }
}
