use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{Fix, LintDiagnostic, LintRule, Severity};

pub struct UnnecessaryPass;

impl LintRule for UnnecessaryPass {
    fn name(&self) -> &'static str {
        "unnecessary-pass"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source.as_bytes(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source_bytes: &[u8], _source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Check body nodes: if they have more than one named child and one is pass_statement
    if node.kind() == "body" || node.kind() == "block" {
        let named_count = node.named_child_count();
        if named_count > 1 {
            for i in 0..named_count {
                if let Some(child) = node.named_child(i)
                    && child.kind() == "pass_statement" {
                        let fix = generate_fix(&child, source_bytes);

                        diags.push(LintDiagnostic {
                            rule: "unnecessary-pass",
                            message: "`pass` is unnecessary when the body contains other statements".to_string(),
                            severity: Severity::Warning,
                            line: child.start_position().row,
                            column: child.start_position().column,
                            end_column: Some(child.end_position().column),
                            fix,
                        });
                    }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source_bytes, _source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn generate_fix(node: &Node, source_bytes: &[u8]) -> Option<Fix> {
    let mut byte_start = node.start_byte();
    let mut byte_end = node.end_byte();

    // Extend to include trailing newline if present
    if byte_end < source_bytes.len() && source_bytes[byte_end] == b'\n' {
        byte_end += 1;
    }

    // Extend backward to include leading whitespace on the line
    while byte_start > 0 {
        let prev = byte_start - 1;
        let ch = source_bytes[prev];
        if ch == b' ' || ch == b'\t' {
            byte_start = prev;
        } else if ch == b'\n' {
            // Don't include the previous newline, just stop here
            break;
        } else {
            break;
        }
    }

    Some(Fix {
        byte_start,
        byte_end,
        replacement: String::new(),
    })
}
