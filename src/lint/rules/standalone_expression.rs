use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct StandaloneExpression;

impl LintRule for StandaloneExpression {
    fn name(&self) -> &'static str {
        "standalone-expression"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "expression_statement" {
        // The expression_statement wraps an expression child
        if let Some(expr) = node.named_child(0)
            && is_pure_expression(&expr, source)
        {
            let text = &source[expr.byte_range()];
            // Truncate for display
            let display = if text.len() > 40 {
                format!("{}...", &text[..37])
            } else {
                text.to_string()
            };
            diags.push(LintDiagnostic {
                rule: "standalone-expression",
                message: format!("expression `{display}` has no effect as a statement",),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: Some(node.end_position().column),
                fix: None,
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

/// Returns true if the expression has no side effects.
/// We flag: identifiers, literals, arithmetic/comparison operators, attribute access,
///          subscript, parenthesized, unary operators, array/dictionary literals.
/// We do NOT flag: calls, assignments, augmented assignments, await, yield.
#[allow(clippy::only_used_in_recursion)]
fn is_pure_expression(node: &Node, source: &str) -> bool {
    match node.kind() {
        // Pure value references, literals, binary/unary operators,
        // subscript, array/dictionary constructors, ternary expressions
        "identifier"
        | "integer"
        | "float"
        | "string"
        | "true"
        | "false"
        | "null"
        | "get_node"
        | "node_path"
        | "binary_operator"
        | "unary_operator"
        | "subscript"
        | "array"
        | "dictionary"
        | "conditional_expression" => true,

        // Parenthesized expression - check inner
        "parenthesized_expression" => {
            if let Some(inner) = node.named_child(0) {
                is_pure_expression(&inner, source)
            } else {
                true
            }
        }

        // Attribute access like `obj.prop` — but NOT if it contains a call (obj.method())
        "attribute" => {
            // In tree-sitter-gdscript, obj.method() parses as attribute > [identifier, attribute_call]
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    if cursor.node().kind() == "attribute_call" {
                        return false; // It's a method call, has side effects
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
            true
        }

        // Everything else (call, assignment, augmented_assignment, await_expression,
        // yield_expression, etc.) is considered to have side effects
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        StandaloneExpression.check(&tree, source, &config)
    }

    // ── Should warn ─────────────────────────────────────────────────

    #[test]
    fn standalone_identifier() {
        let source = "\
func foo():
\tx
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains('x'));
        assert!(diags[0].message.contains("no effect"));
    }

    #[test]
    fn standalone_integer() {
        let source = "\
func foo():
\t42
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("42"));
    }

    #[test]
    fn standalone_string() {
        let source = "\
func foo():
\t\"hello\"
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn standalone_arithmetic() {
        let source = "\
func foo():
\tx + y
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("x + y"));
    }

    #[test]
    fn standalone_comparison() {
        let source = "\
func foo():
\tx == y
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn standalone_attribute() {
        let source = "\
func foo():
\tobj.prop
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn standalone_boolean() {
        let source = "\
func foo():
\ttrue
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn standalone_unary() {
        let source = "\
func foo():
\t-x
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Should NOT warn (side-effect expressions) ───────────────────

    #[test]
    fn function_call_ok() {
        let source = "\
func foo():
\tbar()
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn method_call_ok() {
        let source = "\
func foo():
\tobj.method()
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn assignment_ok() {
        let source = "\
func foo():
\tx = 5
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn augmented_assignment_ok() {
        let source = "\
func foo():
\tx += 1
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn await_expression_ok() {
        let source = "\
func foo():
\tawait some_signal
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn yield_expression_ok() {
        let source = "\
func foo():
\tyield()
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn var_declaration_ok() {
        let source = "\
func foo():
\tvar x = 10
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn return_statement_ok() {
        let source = "\
func foo():
\treturn 42
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn multiple_standalone_expressions() {
        let source = "\
func foo():
\t1
\t\"unused\"
\tx
\tbar()
";
        let diags = check(source);
        assert_eq!(diags.len(), 3);
    }
}
