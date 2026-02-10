use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct UnnecessaryPass;

impl LintRule for UnnecessaryPass {
    fn name(&self) -> &'static str {
        "unnecessary-pass"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Check body nodes: if they have more than one named child and one is pass_statement
    if node.kind() == "body" || node.kind() == "block" {
        let named_count = node.named_child_count();
        if named_count > 1 {
            for i in 0..named_count {
                if let Some(child) = node.named_child(i) {
                    if child.kind() == "pass_statement" {
                        diags.push(LintDiagnostic {
                            rule: "unnecessary-pass",
                            message: "`pass` is unnecessary when the body contains other statements".to_string(),
                            severity: Severity::Warning,
                            line: child.start_position().row,
                            column: child.start_position().column,
                            fix: None,
                        });
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), _source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}
