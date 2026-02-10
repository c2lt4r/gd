use tree_sitter::{Node, Tree};

use crate::core::config::LintConfig;
use super::{LintDiagnostic, LintRule, Severity};

pub struct UnreachableCode;

impl LintRule for UnreachableCode {
    fn name(&self) -> &'static str {
        "unreachable-code"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Check body/block nodes for statements after return/break/continue
    if is_body_node(node.kind()) {
        check_body_for_unreachable(node, source, diags);
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

fn is_body_node(kind: &str) -> bool {
    matches!(kind, "body" | "block")
}

fn is_terminator(kind: &str) -> bool {
    matches!(kind, "return_statement" | "break_statement" | "continue_statement")
}

fn check_body_for_unreachable(body: Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut found_terminator: Option<&str> = None;

    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let child = cursor.node();
        if !child.is_named() {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        if let Some(term) = found_terminator {
            diags.push(LintDiagnostic {
                rule: "unreachable-code",
                message: format!("unreachable code after `{}`", term),
                severity: Severity::Warning,
                line: child.start_position().row,
                column: child.start_position().column,
                fix: None,
            });
            // Only report the first unreachable statement per block
            break;
        }

        if is_terminator(child.kind()) {
            found_terminator = Some(match child.kind() {
                "return_statement" => "return",
                "break_statement" => "break",
                "continue_statement" => "continue",
                _ => unreachable!(),
            });
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
