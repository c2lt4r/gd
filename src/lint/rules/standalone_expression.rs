use crate::core::gd_ast::{self, GdExpr, GdFile, GdStmt};

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

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Expr { node, expr } = stmt
                && is_pure_expression(expr)
            {
                let text = &source[expr.node().byte_range()];
                let display = if text.len() > 40 {
                    format!("{}...", &text[..37])
                } else {
                    text.to_string()
                };
                diags.push(LintDiagnostic {
                    rule: "standalone-expression",
                    message: format!("expression `{display}` has no effect as a statement"),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix: None,
                    context_lines: None,
                });
            }
        });
        diags
    }
}

/// Returns true if the expression has no side effects.
/// We flag: identifiers, literals, arithmetic/comparison operators, attribute access,
///          subscript, parenthesized, unary operators, array/dictionary literals.
/// We do NOT flag: calls, assignments, augmented assignments, await, yield.
fn is_pure_expression(expr: &GdExpr<'_>) -> bool {
    match expr {
        // Pure value references, literals, binary/unary operators,
        // subscript, array/dictionary constructors, ternary expressions
        GdExpr::Ident { .. }
        | GdExpr::IntLiteral { .. }
        | GdExpr::FloatLiteral { .. }
        | GdExpr::StringLiteral { .. }
        | GdExpr::StringName { .. }
        | GdExpr::Bool { .. }
        | GdExpr::Null { .. }
        | GdExpr::GetNode { .. }
        | GdExpr::BinOp { .. }
        | GdExpr::UnaryOp { .. }
        | GdExpr::Subscript { .. }
        | GdExpr::Array { .. }
        | GdExpr::Dict { .. }
        | GdExpr::Ternary { .. }
        | GdExpr::Cast { .. }
        | GdExpr::Is { .. }
        | GdExpr::PropertyAccess { .. } => true,

        // Side-effect expressions: calls, await, lambda, preload
        GdExpr::Call { .. }
        | GdExpr::MethodCall { .. }
        | GdExpr::SuperCall { .. }
        | GdExpr::Await { .. }
        | GdExpr::Lambda { .. }
        | GdExpr::Preload { .. }
        | GdExpr::Invalid { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        StandaloneExpression.check(&file, source, &config)
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
