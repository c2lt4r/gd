use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct StandaloneTernary;

impl LintRule for StandaloneTernary {
    fn name(&self) -> &'static str {
        "standalone-ternary"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // A ternary used as a statement (expression_statement > conditional_expression)
    if node.kind() == "expression_statement"
        && let Some(expr) = node.named_child(0)
        && (expr.kind() == "conditional_expression" || expr.kind() == "ternary_expression")
    {
        let text = expr.utf8_text(source.as_bytes()).ok().unwrap_or("?");
        let display = if text.len() > 40 {
            format!("{}...", &text[..37])
        } else {
            text.to_string()
        };
        diags.push(LintDiagnostic {
            rule: "standalone-ternary",
            message: format!("ternary expression `{display}` used as statement; result is unused"),
            severity: Severity::Warning,
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
            check_node(cursor.node(), source, diags);
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
        StandaloneTernary.check(&tree, source, &config)
    }

    #[test]
    fn ternary_as_statement() {
        let source = "func f():\n\t1 if true else 2\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("ternary"));
        assert!(diags[0].message.contains("unused"));
    }

    #[test]
    fn ternary_in_assignment_ok() {
        let source = "func f():\n\tvar x = 1 if true else 2\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn ternary_in_return_ok() {
        let source = "func f():\n\treturn 1 if true else 2\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn normal_call_ok() {
        let source = "func f():\n\tprint(\"hello\")\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(StandaloneTernary.default_enabled());
    }
}
