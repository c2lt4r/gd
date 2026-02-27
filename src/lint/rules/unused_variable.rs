use std::collections::{HashMap, HashSet};
use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedVariable;

impl LintRule for UnusedVariable {
    fn name(&self) -> &'static str {
        "unused-variable"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Check each function body independently
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                check_function_body(&func.body, &mut diags);
            }
        });

        diags
    }
}

/// Track variable declarations and references within a function body.
fn check_function_body(body: &[GdStmt], diags: &mut Vec<LintDiagnostic>) {
    // name -> (line, col, name_byte_start)
    let mut declarations: HashMap<&str, (usize, usize, usize)> = HashMap::new();
    let mut references: HashSet<&str> = HashSet::new();

    collect_decls_and_refs(body, &mut declarations, &mut references);

    for (name, (line, col, name_byte_start)) in &declarations {
        if name.starts_with('_') {
            continue;
        }
        if !references.contains(name) {
            diags.push(LintDiagnostic {
                rule: "unused-variable",
                message: format!("variable `{name}` is assigned but never used"),
                severity: Severity::Warning,
                line: *line,
                column: *col,
                end_column: Some(*col + name.len()),
                fix: Some(Fix {
                    byte_start: *name_byte_start,
                    byte_end: *name_byte_start,
                    replacement: "_".to_string(),
                }),
                context_lines: None,
            });
        }
    }
}

/// Walk function body statements, collecting local variable declarations and
/// identifier references. Does not recurse into nested lambdas (separate scope).
fn collect_decls_and_refs<'a>(
    stmts: &[GdStmt<'a>],
    declarations: &mut HashMap<&'a str, (usize, usize, usize)>,
    references: &mut HashSet<&'a str>,
) {
    for stmt in stmts {
        match stmt {
            GdStmt::Var(var) => {
                // Record local variable declaration
                let (line, col, byte_start) = if let Some(n) = var.name_node {
                    (n.start_position().row, n.start_position().column, n.start_byte())
                } else {
                    let pos = var.node.start_position();
                    (pos.row, pos.column, var.node.start_byte())
                };
                declarations.insert(var.name, (line, col, byte_start));
                // Value expression may reference other vars
                if let Some(value) = &var.value {
                    collect_refs_from_expr(value, references);
                }
            }
            GdStmt::Assign { target, value, .. } => {
                // RHS: always collect references
                collect_refs_from_expr(value, references);
                // LHS: only collect refs if it's complex (not a simple identifier write)
                if !matches!(target, GdExpr::Ident { .. }) {
                    collect_refs_from_expr(target, references);
                }
            }
            GdStmt::AugAssign { target, value, .. } => {
                collect_refs_from_expr(value, references);
                // LHS of augmented assignment: same treatment as regular assignment
                if !matches!(target, GdExpr::Ident { .. }) {
                    collect_refs_from_expr(target, references);
                }
            }
            GdStmt::Expr { expr, .. } => {
                collect_refs_from_expr(expr, references);
            }
            GdStmt::Return { value, .. } => {
                if let Some(v) = value {
                    collect_refs_from_expr(v, references);
                }
            }
            GdStmt::If(if_stmt) => {
                collect_refs_from_expr(&if_stmt.condition, references);
                collect_decls_and_refs(&if_stmt.body, declarations, references);
                for (cond, branch) in &if_stmt.elif_branches {
                    collect_refs_from_expr(cond, references);
                    collect_decls_and_refs(branch, declarations, references);
                }
                if let Some(else_body) = &if_stmt.else_body {
                    collect_decls_and_refs(else_body, declarations, references);
                }
            }
            GdStmt::For { iter, body, .. } => {
                collect_refs_from_expr(iter, references);
                collect_decls_and_refs(body, declarations, references);
            }
            GdStmt::While { condition, body, .. } => {
                collect_refs_from_expr(condition, references);
                collect_decls_and_refs(body, declarations, references);
            }
            GdStmt::Match { value, arms, .. } => {
                collect_refs_from_expr(value, references);
                for arm in arms {
                    for pat in &arm.patterns {
                        collect_refs_from_expr(pat, references);
                    }
                    if let Some(guard) = &arm.guard {
                        collect_refs_from_expr(guard, references);
                    }
                    collect_decls_and_refs(&arm.body, declarations, references);
                }
            }
            GdStmt::Pass { .. } | GdStmt::Break { .. } | GdStmt::Continue { .. }
            | GdStmt::Breakpoint { .. } | GdStmt::Invalid { .. } => {}
        }
    }
}

/// Collect all identifier references from an expression tree.
/// Stops at lambda boundaries (separate scope).
fn collect_refs_from_expr<'a>(expr: &GdExpr<'a>, refs: &mut HashSet<&'a str>) {
    // Don't recurse into lambdas — they have their own scope
    if matches!(expr, GdExpr::Lambda { .. }) {
        return;
    }
    match expr {
        GdExpr::Ident { name, .. } => { refs.insert(name); }
        GdExpr::BinOp { left, right, .. } => {
            collect_refs_from_expr(left, refs);
            collect_refs_from_expr(right, refs);
        }
        GdExpr::UnaryOp { operand, .. } | GdExpr::Cast { expr: operand, .. }
        | GdExpr::Is { expr: operand, .. } | GdExpr::Await { expr: operand, .. } => {
            collect_refs_from_expr(operand, refs);
        }
        GdExpr::Call { callee, args, .. } => {
            collect_refs_from_expr(callee, refs);
            for a in args { collect_refs_from_expr(a, refs); }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            collect_refs_from_expr(receiver, refs);
            for a in args { collect_refs_from_expr(a, refs); }
        }
        GdExpr::SuperCall { args, .. } => {
            for a in args { collect_refs_from_expr(a, refs); }
        }
        GdExpr::PropertyAccess { receiver, .. } => {
            collect_refs_from_expr(receiver, refs);
        }
        GdExpr::Subscript { receiver, index, .. } => {
            collect_refs_from_expr(receiver, refs);
            collect_refs_from_expr(index, refs);
        }
        GdExpr::Ternary { true_val, condition, false_val, .. } => {
            collect_refs_from_expr(true_val, refs);
            collect_refs_from_expr(condition, refs);
            collect_refs_from_expr(false_val, refs);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements { collect_refs_from_expr(e, refs); }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                collect_refs_from_expr(k, refs);
                collect_refs_from_expr(v, refs);
            }
        }
        // Literals, GetNode, Preload, StringName, Invalid — no identifier refs
        _ => {}
    }
}
