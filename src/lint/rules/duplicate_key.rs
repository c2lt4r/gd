use std::collections::HashSet;
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateKey;

impl LintRule for DuplicateKey {
    fn name(&self) -> &'static str {
        "duplicate-key"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        walk_for_dicts(root, source, &mut diags);
        diags
    }
}

fn walk_for_dicts(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "dictionary" {
        check_dict(node, source, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            walk_for_dicts(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_dict(dict_node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();
    let mut seen: HashSet<String> = HashSet::new();

    let mut cursor = dict_node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.kind() == "pair"
                && let Some(key_node) = child.named_child(0)
            {
                let key_text = key_node.utf8_text(src).unwrap_or("").to_string();
                if !seen.insert(key_text.clone()) {
                    diags.push(LintDiagnostic {
                        rule: "duplicate-key",
                        message: format!("duplicate dictionary key {key_text}"),
                        severity: Severity::Warning,
                        line: key_node.start_position().row,
                        column: key_node.start_position().column,
                        end_column: None,
                        fix: None,
                        context_lines: None,
                    });
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
