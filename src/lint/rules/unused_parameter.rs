use std::collections::HashSet;

use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedParameter;

impl LintRule for UnusedParameter {
    fn name(&self) -> &'static str {
        "unused-parameter"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_decls(&file.declarations, &mut diags);
        diags
    }
}

fn check_decls(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl {
            if func.params.is_empty() {
                continue;
            }

            // Collect all identifier references in the function body
            let mut references: HashSet<&str> = HashSet::new();
            collect_refs_from_stmts(&func.body, &mut references);

            // Report unused parameters
            let mut unused: Vec<_> = func
                .params
                .iter()
                .filter(|p| !p.name.starts_with('_') && !references.contains(p.name))
                .collect();
            unused.sort_by_key(|p| (p.node.start_position().row, p.node.start_position().column));

            for param in unused {
                diags.push(LintDiagnostic {
                    rule: "unused-parameter",
                    message: format!(
                        "parameter `{}` is never used; prefix with `_` if intentional",
                        param.name
                    ),
                    severity: Severity::Warning,
                    line: param.node.start_position().row,
                    column: param.node.start_position().column,
                    end_column: Some(param.node.start_position().column + param.name.len()),
                    fix: None,
                    context_lines: None,
                });
            }
        }
        if let GdDecl::Class(class) = decl {
            check_decls(&class.declarations, diags);
        }
    }
}

/// Collect all identifier references from statements. Stops at lambda boundaries.
fn collect_refs_from_stmts<'a>(stmts: &[GdStmt<'a>], refs: &mut HashSet<&'a str>) {
    for stmt in stmts {
        match stmt {
            GdStmt::Var(var) => {
                if let Some(value) = &var.value {
                    collect_refs_from_expr(value, refs);
                }
            }
            GdStmt::Expr { expr, .. } => collect_refs_from_expr(expr, refs),
            GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
                collect_refs_from_expr(target, refs);
                collect_refs_from_expr(value, refs);
            }
            GdStmt::Return { value: Some(v), .. } => {
                collect_refs_from_expr(v, refs);
            }
            GdStmt::If(gif) => {
                collect_refs_from_expr(&gif.condition, refs);
                collect_refs_from_stmts(&gif.body, refs);
                for (cond, branch) in &gif.elif_branches {
                    collect_refs_from_expr(cond, refs);
                    collect_refs_from_stmts(branch, refs);
                }
                if let Some(else_body) = &gif.else_body {
                    collect_refs_from_stmts(else_body, refs);
                }
            }
            GdStmt::For { iter, body, .. } => {
                collect_refs_from_expr(iter, refs);
                collect_refs_from_stmts(body, refs);
            }
            GdStmt::While {
                condition, body, ..
            } => {
                collect_refs_from_expr(condition, refs);
                collect_refs_from_stmts(body, refs);
            }
            GdStmt::Match { value, arms, .. } => {
                collect_refs_from_expr(value, refs);
                for arm in arms {
                    for pat in &arm.patterns {
                        collect_refs_from_expr(pat, refs);
                    }
                    if let Some(guard) = &arm.guard {
                        collect_refs_from_expr(guard, refs);
                    }
                    collect_refs_from_stmts(&arm.body, refs);
                }
            }
            _ => {}
        }
    }
}

/// Collect all identifier references from an expression tree.
/// Stops at lambda boundaries (separate scope).
fn collect_refs_from_expr<'a>(expr: &GdExpr<'a>, refs: &mut HashSet<&'a str>) {
    if matches!(expr, GdExpr::Lambda { .. }) {
        return;
    }
    match expr {
        GdExpr::Ident { name, .. } => {
            refs.insert(name);
        }
        GdExpr::BinOp { left, right, .. } => {
            collect_refs_from_expr(left, refs);
            collect_refs_from_expr(right, refs);
        }
        GdExpr::UnaryOp { operand, .. }
        | GdExpr::Cast { expr: operand, .. }
        | GdExpr::Is { expr: operand, .. }
        | GdExpr::Await { expr: operand, .. } => {
            collect_refs_from_expr(operand, refs);
        }
        GdExpr::Call { callee, args, .. } => {
            collect_refs_from_expr(callee, refs);
            for a in args {
                collect_refs_from_expr(a, refs);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            collect_refs_from_expr(receiver, refs);
            for a in args {
                collect_refs_from_expr(a, refs);
            }
        }
        GdExpr::SuperCall { args, .. } => {
            for a in args {
                collect_refs_from_expr(a, refs);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => collect_refs_from_expr(receiver, refs),
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            collect_refs_from_expr(receiver, refs);
            collect_refs_from_expr(index, refs);
        }
        GdExpr::Ternary {
            true_val,
            condition,
            false_val,
            ..
        } => {
            collect_refs_from_expr(true_val, refs);
            collect_refs_from_expr(condition, refs);
            collect_refs_from_expr(false_val, refs);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                collect_refs_from_expr(e, refs);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                collect_refs_from_expr(k, refs);
                collect_refs_from_expr(v, refs);
            }
        }
        _ => {}
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
        UnusedParameter.check(&file, source, &config)
    }

    #[test]
    fn detects_unused_parameter() {
        let source = "func f(x: int, y: int) -> int:\n\treturn x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "unused-parameter");
        assert!(diags[0].message.contains("`y`"));
    }

    #[test]
    fn no_warning_when_all_used() {
        let source = "func add(x: int, y: int) -> int:\n\treturn x + y\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn skips_underscore_prefixed() {
        let source = "func f(_unused: int) -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_unused() {
        let source = "func f(a: int, b: int, c: int) -> void:\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 3);
    }

    #[test]
    fn no_warning_for_no_params() {
        let source = "func f() -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn parameter_used_in_nested_expression() {
        let source = "func f(x: int) -> int:\n\tvar result := x * 2 + 1\n\treturn result\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn parameter_used_in_method_call() {
        let source = "func f(msg: String) -> void:\n\tprint(msg)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn is_opt_in_rule() {
        assert!(!UnusedParameter.default_enabled());
    }

    #[test]
    fn lambda_capture_flagged_as_unused() {
        let source = "func f(x: int) -> void:\n\tvar fn := func(): return x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_unused_delta_in_process() {
        let source = "func _process(delta: float) -> void:\n\tprint(\"hello\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`delta`"));
    }

    #[test]
    fn no_warning_delta_used() {
        let source = "func _process(delta: float) -> void:\n\tposition.x += 100 * delta\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn parameter_used_in_conditional() {
        let source = "func f(x: int) -> String:\n\tif x > 0:\n\t\treturn \"positive\"\n\treturn \"non-positive\"\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn end_column_set_correctly() {
        let source = "func f(x: int, y: int) -> int:\n\treturn x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].end_column, Some(diags[0].column + 1)); // "y" is 1 char
    }
}
