use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct AwaitInReady;

impl LintRule for AwaitInReady {
    fn name(&self) -> &'static str {
        "await-in-ready"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        find_ready_decls(&file.declarations, &mut diags);
        diags
    }
}

fn find_ready_decls(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        match decl {
            GdDecl::Func(func) if func.name == "_ready" => {
                find_awaits_in_stmts(&func.body, diags);
            }
            GdDecl::Class(cls) => {
                find_ready_decls(&cls.declarations, diags);
            }
            _ => {}
        }
    }
}

fn find_awaits_in_stmts(stmts: &[GdStmt<'_>], diags: &mut Vec<LintDiagnostic>) {
    for stmt in stmts {
        find_awaits_in_stmt(stmt, diags);
    }
}

fn find_awaits_in_stmt(stmt: &GdStmt<'_>, diags: &mut Vec<LintDiagnostic>) {
    match stmt {
        GdStmt::Expr { expr, .. } => find_awaits_in_expr(expr, diags),
        GdStmt::Var(var) => {
            if let Some(value) = &var.value {
                find_awaits_in_expr(value, diags);
            }
        }
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            find_awaits_in_expr(target, diags);
            find_awaits_in_expr(value, diags);
        }
        GdStmt::Return { value, .. } => {
            if let Some(v) = value {
                find_awaits_in_expr(v, diags);
            }
        }
        GdStmt::If(if_stmt) => {
            find_awaits_in_expr(&if_stmt.condition, diags);
            find_awaits_in_stmts(&if_stmt.body, diags);
            for (cond, branch) in &if_stmt.elif_branches {
                find_awaits_in_expr(cond, diags);
                find_awaits_in_stmts(branch, diags);
            }
            if let Some(else_body) = &if_stmt.else_body {
                find_awaits_in_stmts(else_body, diags);
            }
        }
        GdStmt::For { iter, body, .. } => {
            find_awaits_in_expr(iter, diags);
            find_awaits_in_stmts(body, diags);
        }
        GdStmt::While { condition, body, .. } => {
            find_awaits_in_expr(condition, diags);
            find_awaits_in_stmts(body, diags);
        }
        GdStmt::Match { value, arms, .. } => {
            find_awaits_in_expr(value, diags);
            for arm in arms {
                find_awaits_in_stmts(&arm.body, diags);
            }
        }
        GdStmt::Pass { .. }
        | GdStmt::Break { .. }
        | GdStmt::Continue { .. }
        | GdStmt::Breakpoint { .. }
        | GdStmt::Invalid { .. } => {}
    }
}

fn find_awaits_in_expr(expr: &GdExpr<'_>, diags: &mut Vec<LintDiagnostic>) {
    match expr {
        GdExpr::Await { node, expr: inner } => {
            diags.push(LintDiagnostic {
                rule: "await-in-ready",
                message: "avoid `await` in _ready(); use call_deferred() or a separate async method"
                    .to_string(),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: None,
                fix: None,
                context_lines: None,
            });
            find_awaits_in_expr(inner, diags);
        }
        GdExpr::BinOp { left, right, .. } => {
            find_awaits_in_expr(left, diags);
            find_awaits_in_expr(right, diags);
        }
        GdExpr::UnaryOp { operand, .. } => find_awaits_in_expr(operand, diags),
        GdExpr::Call { callee, args, .. } => {
            find_awaits_in_expr(callee, diags);
            for arg in args {
                find_awaits_in_expr(arg, diags);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            find_awaits_in_expr(receiver, diags);
            for arg in args {
                find_awaits_in_expr(arg, diags);
            }
        }
        GdExpr::SuperCall { args, .. } => {
            for arg in args {
                find_awaits_in_expr(arg, diags);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => find_awaits_in_expr(receiver, diags),
        GdExpr::Subscript { receiver, index, .. } => {
            find_awaits_in_expr(receiver, diags);
            find_awaits_in_expr(index, diags);
        }
        GdExpr::Cast { expr: inner, .. } | GdExpr::Is { expr: inner, .. } => {
            find_awaits_in_expr(inner, diags);
        }
        GdExpr::Ternary { true_val, condition, false_val, .. } => {
            find_awaits_in_expr(true_val, diags);
            find_awaits_in_expr(condition, diags);
            find_awaits_in_expr(false_val, diags);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                find_awaits_in_expr(e, diags);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                find_awaits_in_expr(k, diags);
                find_awaits_in_expr(v, diags);
            }
        }
        // Don't recurse into lambdas — they're separate scopes
        GdExpr::Lambda { .. }
        | GdExpr::IntLiteral { .. }
        | GdExpr::FloatLiteral { .. }
        | GdExpr::StringLiteral { .. }
        | GdExpr::StringName { .. }
        | GdExpr::Bool { .. }
        | GdExpr::Null { .. }
        | GdExpr::Ident { .. }
        | GdExpr::GetNode { .. }
        | GdExpr::Preload { .. }
        | GdExpr::Invalid { .. } => {}
    }
}
