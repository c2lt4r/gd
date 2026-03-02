use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdIf, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct CyclomaticComplexity;

impl LintRule for CyclomaticComplexity {
    fn name(&self) -> &'static str {
        "cyclomatic-complexity"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let max_complexity = config
            .rules
            .get("cyclomatic-complexity")
            .and_then(|r| r.max_complexity)
            .unwrap_or(config.max_cyclomatic_complexity);
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                let complexity = compute_complexity(&func.body);
                if complexity > max_complexity {
                    diags.push(LintDiagnostic {
                        rule: "cyclomatic-complexity",
                        message: format!(
                            "function `{}` has cyclomatic complexity of {complexity} (max {max_complexity})",
                            func.name
                        ),
                        severity: Severity::Warning,
                        line: func.node.start_position().row,
                        column: func.node.start_position().column,
                        fix: None,
                        end_column: None,
                        context_lines: None,
                    });
                }
            }
        });
        diags
    }
}

/// Compute cyclomatic complexity of a function body.
/// Starts at 1 and increments for each branching construct.
fn compute_complexity(body: &[GdStmt]) -> usize {
    let mut complexity = 1;
    count_branches(body, &mut complexity);
    complexity
}

fn count_branches(stmts: &[GdStmt], complexity: &mut usize) {
    for stmt in stmts {
        match stmt {
            GdStmt::If(if_stmt) => {
                *complexity += 1;
                if is_guard_clause(if_stmt) {
                    // Guard clause: don't count and/or in its condition.
                    // Only recurse into body/elif/else (not condition).
                    count_branches(&if_stmt.body, complexity);
                    for (_, branch) in &if_stmt.elif_branches {
                        *complexity += 1;
                        count_branches(branch, complexity);
                    }
                    if let Some(else_body) = &if_stmt.else_body {
                        count_branches(else_body, complexity);
                    }
                } else {
                    count_expr_branches(&if_stmt.condition, complexity);
                    count_branches(&if_stmt.body, complexity);
                    for (cond, branch) in &if_stmt.elif_branches {
                        *complexity += 1;
                        count_expr_branches(cond, complexity);
                        count_branches(branch, complexity);
                    }
                    if let Some(else_body) = &if_stmt.else_body {
                        count_branches(else_body, complexity);
                    }
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                *complexity += 1;
                count_branches(body, complexity);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    *complexity += 1;
                    count_branches(&arm.body, complexity);
                }
            }
            // Other statements: check for and/or in expressions
            GdStmt::Expr { expr, .. } => count_expr_branches(expr, complexity),
            GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
                count_expr_branches(target, complexity);
                count_expr_branches(value, complexity);
            }
            GdStmt::Var(var) => {
                if let Some(value) = &var.value {
                    count_expr_branches(value, complexity);
                }
            }
            GdStmt::Return { value: Some(v), .. } => {
                count_expr_branches(v, complexity);
            }
            _ => {}
        }
    }
}

/// Count `and`/`or` operators in expressions.
fn count_expr_branches(expr: &GdExpr, complexity: &mut usize) {
    match expr {
        GdExpr::BinOp {
            op, left, right, ..
        } => {
            if matches!(*op, "and" | "or") {
                *complexity += 1;
            }
            count_expr_branches(left, complexity);
            count_expr_branches(right, complexity);
        }
        GdExpr::UnaryOp { operand, .. }
        | GdExpr::Cast { expr: operand, .. }
        | GdExpr::Is { expr: operand, .. }
        | GdExpr::Await { expr: operand, .. } => {
            count_expr_branches(operand, complexity);
        }
        GdExpr::Call { callee, args, .. } => {
            count_expr_branches(callee, complexity);
            for a in args {
                count_expr_branches(a, complexity);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            count_expr_branches(receiver, complexity);
            for a in args {
                count_expr_branches(a, complexity);
            }
        }
        GdExpr::SuperCall { args, .. } => {
            for a in args {
                count_expr_branches(a, complexity);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => count_expr_branches(receiver, complexity),
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            count_expr_branches(receiver, complexity);
            count_expr_branches(index, complexity);
        }
        GdExpr::Ternary {
            true_val,
            condition,
            false_val,
            ..
        } => {
            count_expr_branches(true_val, complexity);
            count_expr_branches(condition, complexity);
            count_expr_branches(false_val, complexity);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                count_expr_branches(e, complexity);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                count_expr_branches(k, complexity);
                count_expr_branches(v, complexity);
            }
        }
        // Don't recurse into lambdas — separate complexity scope
        _ => {}
    }
}

/// Check if an if_statement is a guard clause: no elif/else branches, body is a single
/// return/continue/break.
fn is_guard_clause(if_stmt: &GdIf) -> bool {
    if !if_stmt.elif_branches.is_empty() || if_stmt.else_body.is_some() {
        return false;
    }
    if if_stmt.body.len() != 1 {
        return false;
    }
    matches!(
        &if_stmt.body[0],
        GdStmt::Return { .. } | GdStmt::Break { .. } | GdStmt::Continue { .. }
    )
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
        CyclomaticComplexity.check(&file, source, &config)
    }

    fn complexity_of(source: &str) -> usize {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        // Find the first function
        for decl in &file.declarations {
            if let GdDecl::Func(func) = decl {
                return compute_complexity(&func.body);
            }
        }
        panic!("no function found");
    }

    #[test]
    fn simple_function_complexity_1() {
        let source = "func foo():\n\tpass\n";
        assert_eq!(complexity_of(source), 1);
    }

    #[test]
    fn single_if() {
        let source = "\
func foo(x):
\tif x > 0:
\t\tprint(x)
";
        assert_eq!(complexity_of(source), 2);
    }

    #[test]
    fn if_elif_else() {
        let source = "\
func foo(x):
\tif x > 0:
\t\tprint(\"pos\")
\telif x < 0:
\t\tprint(\"neg\")
\telse:
\t\tprint(\"zero\")
";
        // 1 base + 1 if + 1 elif = 3
        assert_eq!(complexity_of(source), 3);
    }

    #[test]
    fn for_loop() {
        let source = "\
func foo(arr):
\tfor item in arr:
\t\tprint(item)
";
        assert_eq!(complexity_of(source), 2);
    }

    #[test]
    fn while_loop() {
        let source = "\
func foo():
\tvar i = 0
\twhile i < 10:
\t\ti += 1
";
        assert_eq!(complexity_of(source), 2);
    }

    #[test]
    fn boolean_and_or() {
        let source = "\
func foo(a, b, c):
\tif a and b or c:
\t\tprint(\"yes\")
";
        // 1 base + 1 if + 1 and + 1 or = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn match_statement() {
        let source = "\
func foo(x):
\tmatch x:
\t\t1:
\t\t\tprint(\"one\")
\t\t2:
\t\t\tprint(\"two\")
\t\t_:
\t\t\tprint(\"other\")
";
        // 1 base + 3 pattern_section = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn nested_control_flow() {
        let source = "\
func foo(items):
\tfor item in items:
\t\tif item > 0:
\t\t\twhile item > 10:
\t\t\t\titem -= 1
";
        // 1 base + 1 for + 1 if + 1 while = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn no_warning_under_threshold() {
        let source = "\
func simple(x):
\tif x:
\t\tprint(x)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_over_threshold() {
        // Build a function with complexity > 10
        let source = "\
func complex(a, b, c):
\tif a:
\t\tpass
\tif b:
\t\tpass
\tif c:
\t\tpass
\tfor i in a:
\t\tpass
\tfor j in b:
\t\tpass
\twhile a:
\t\tpass
\twhile b:
\t\tpass
\twhile c:
\t\tpass
\tif a and b:
\t\tpass
\tif a or c:
\t\tpass
";
        // 1 base + 10 (if/for/while) + 2 (and/or) = 13
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "cyclomatic-complexity");
        assert!(diags[0].message.contains("complex"));
        assert!(diags[0].message.contains("13"));
    }

    #[test]
    fn does_not_count_nested_function() {
        // Inner function's complexity should not be added to outer
        let source = "\
func outer():
\tif true:
\t\tpass

func inner():
\tif true:
\t\tif true:
\t\t\tpass
";
        // outer: 1+1=2, inner: 1+1+1=3 — both under threshold
        assert!(check(source).is_empty());
    }

    #[test]
    fn checks_inner_class_functions() {
        // Create a complex function inside an inner class
        let source = "\
class Inner:
\tfunc complex(a, b, c):
\t\tif a:
\t\t\tpass
\t\tif b:
\t\t\tpass
\t\tif c:
\t\t\tpass
\t\tfor i in a:
\t\t\tpass
\t\tfor j in b:
\t\t\tpass
\t\twhile a:
\t\t\tpass
\t\twhile b:
\t\t\tpass
\t\twhile c:
\t\t\tpass
\t\tif a and b:
\t\t\tpass
\t\tif a or c:
\t\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("complex"));
    }

    #[test]
    fn guard_clause_not_penalized() {
        // Guard clause with and/or should not count the boolean ops
        let guard = "\
func guard(event):
\tif not (event is InputEventKey and event.pressed and not event.echo):
\t\treturn
\thandle()
";
        // 1 base + 1 if (guard) = 2 — and/or in guard not counted
        assert_eq!(complexity_of(guard), 2);
    }

    #[test]
    fn guard_clause_same_as_nested() {
        let nested = "\
func nested(event):
\tif event is InputEventKey:
\t\tif event.pressed:
\t\t\tif not event.echo:
\t\t\t\thandle()
";
        let guard = "\
func guard(event):
\tif not (event is InputEventKey and event.pressed and not event.echo):
\t\treturn
\thandle()
";
        // Guard should be <= nested, not more
        assert!(complexity_of(guard) <= complexity_of(nested));
    }

    #[test]
    fn non_guard_if_still_counts_and_or() {
        // Not a guard clause — body has more than just return
        let source = "\
func f(a, b, c):
\tif a and b and c:
\t\tprint(\"yes\")
";
        // 1 base + 1 if + 2 and = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn reports_correct_location() {
        let source = "\
func ok():
\tpass

func complex(a, b, c):
\tif a:
\t\tpass
\tif b:
\t\tpass
\tif c:
\t\tpass
\tfor i in a:
\t\tpass
\tfor j in b:
\t\tpass
\twhile a:
\t\tpass
\twhile b:
\t\tpass
\twhile c:
\t\tpass
\tif a and b:
\t\tpass
\tif a or c:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3); // 0-indexed, 4th line
    }
}
