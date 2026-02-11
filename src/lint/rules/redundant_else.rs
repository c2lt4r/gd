use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct RedundantElse;

impl LintRule for RedundantElse {
    fn name(&self) -> &'static str {
        "redundant-else"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "if_statement" {
        check_if_statement(node, source, diags);
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

fn check_if_statement(node: Node, _source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Get the body of the if branch (first `body` child)
    let body = match node.child_by_field_name("body") {
        Some(b) => b,
        None => return,
    };

    // Check if the body always terminates
    if !body_always_terminates(body) {
        return;
    }

    // Look for an else_clause that is NOT an elif
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let child = cursor.node();
        if child.kind() == "else_clause" {
            // Check if this else clause contains an elif (if_statement as direct child)
            // If so, don't warn - elif chains are fine
            let has_elif = child
                .children(&mut child.walk())
                .any(|c| c.kind() == "if_statement");
            if has_elif {
                return;
            }

            diags.push(LintDiagnostic {
                rule: "redundant-else",
                message: "unnecessary `else` after `return`/`break`/`continue`; remove the `else` and dedent"
                    .to_string(),
                severity: Severity::Warning,
                line: child.start_position().row,
                column: child.start_position().column,
                end_column: None,
                fix: None,
            });
            return;
        }

        // Also skip elif_clause (some grammars use this)
        if child.kind() == "elif_clause" {
            return;
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

/// Check if the last named statement in a body is a terminator.
fn body_always_terminates(body: Node) -> bool {
    let mut last_statement = None;
    let mut cursor = body.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() && child.kind() != "comment" {
                last_statement = Some(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    match last_statement {
        Some(stmt) => is_terminator(stmt.kind()),
        None => false,
    }
}

fn is_terminator(kind: &str) -> bool {
    matches!(
        kind,
        "return_statement" | "break_statement" | "continue_statement"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        RedundantElse.check(&tree, source, &config)
    }

    #[test]
    fn detects_redundant_else_after_return() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\treturn -x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "redundant-else");
        assert!(diags[0].message.contains("unnecessary `else`"));
    }

    #[test]
    fn detects_redundant_else_after_break() {
        let source = "func f() -> void:\n\tfor i in range(10):\n\t\tif i == 5:\n\t\t\tbreak\n\t\telse:\n\t\t\tprint(i)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_redundant_else_after_continue() {
        let source = "func f() -> void:\n\tfor i in range(10):\n\t\tif i == 5:\n\t\t\tcontinue\n\t\telse:\n\t\t\tprint(i)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_when_if_does_not_terminate() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\tprint(x)\n\telse:\n\t\tprint(-x)\n\treturn x\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_elif_chain() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn 1\n\telif x < 0:\n\t\treturn -1\n\telse:\n\t\treturn 0\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_without_else() {
        let source = "func f(x: int) -> void:\n\tif x > 0:\n\t\treturn\n\tprint(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_nested_redundant_else() {
        let source = "func f(x: int, y: int) -> int:\n\tif x > 0:\n\t\tif y > 0:\n\t\t\treturn 1\n\t\telse:\n\t\t\treturn -1\n\treturn 0\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_if_return_not_last() {
        let source = "func f(x: int) -> void:\n\tif x > 0:\n\t\treturn\n\t\tprint(\"unreachable\")\n\telse:\n\t\tprint(x)\n";
        // The return IS the last named non-comment statement?
        // Actually, print("unreachable") comes after return - but it's still a statement.
        // The body contains return_statement then expression_statement, so last is expression_statement.
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_with_multiple_statements_before_return() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\tvar y := x * 2\n\t\tprint(y)\n\t\treturn y\n\telse:\n\t\treturn -x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
