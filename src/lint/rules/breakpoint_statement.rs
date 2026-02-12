use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct BreakpointStatement;

impl LintRule for BreakpointStatement {
    fn name(&self) -> &'static str {
        "breakpoint-statement"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, &mut diags);
        diags
    }
}

fn check_node(node: Node, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "breakpoint_statement" {
        diags.push(LintDiagnostic {
            rule: "breakpoint-statement",
            message: "found `breakpoint`; consider removing before release".to_string(),
            severity: Severity::Info,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: None,
            context_lines: None,
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), diags);
            if !cursor.goto_next_sibling() {
                break;
            }
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
        BreakpointStatement.check(&tree, source, &config)
    }

    #[test]
    fn detects_breakpoint() {
        let source = "func f():\n\tbreakpoint\n\tprint(\"hi\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "breakpoint-statement");
    }

    #[test]
    fn no_warning_without_breakpoint() {
        let source = "func f():\n\tprint(\"hi\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!BreakpointStatement.default_enabled());
    }
}
