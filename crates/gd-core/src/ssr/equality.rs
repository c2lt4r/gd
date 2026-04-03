//! Structural equality for GDScript AST nodes.
//!
//! Used by the SSR matcher to verify repeated placeholders: when `$a`
//! appears twice in a pattern, both captures must be structurally
//! identical — same AST shape, same leaf values, ignoring whitespace
//! and parenthesization.

use crate::gd_ast::{GdExpr, GdStmt};

/// Returns `true` if two expressions are structurally identical.
#[allow(clippy::too_many_lines)]
pub fn structurally_equal_expr(a: &GdExpr, b: &GdExpr) -> bool {
    match (a, b) {
        // ── Value leaves (same body: va == vb) ───────────────────
        (GdExpr::IntLiteral { value: va, .. }, GdExpr::IntLiteral { value: vb, .. })
        | (GdExpr::FloatLiteral { value: va, .. }, GdExpr::FloatLiteral { value: vb, .. })
        | (GdExpr::StringLiteral { value: va, .. }, GdExpr::StringLiteral { value: vb, .. })
        | (GdExpr::StringName { value: va, .. }, GdExpr::StringName { value: vb, .. }) => va == vb,

        (GdExpr::Bool { value: va, .. }, GdExpr::Bool { value: vb, .. }) => va == vb,
        (GdExpr::Null { .. }, GdExpr::Null { .. }) => true,

        // ── Identifiers ──────────────────────────────────────────
        (GdExpr::Ident { name: na, .. }, GdExpr::Ident { name: nb, .. }) => na == nb,

        // ── Collections ──────────────────────────────────────────
        (GdExpr::Array { elements: ea, .. }, GdExpr::Array { elements: eb, .. }) => {
            ea.len() == eb.len()
                && ea
                    .iter()
                    .zip(eb)
                    .all(|(a, b)| structurally_equal_expr(a, b))
        }
        (GdExpr::Dict { pairs: pa, .. }, GdExpr::Dict { pairs: pb, .. }) => {
            pa.len() == pb.len()
                && pa.iter().zip(pb).all(|((ka, va), (kb, vb))| {
                    structurally_equal_expr(ka, kb) && structurally_equal_expr(va, vb)
                })
        }

        // ── Calls ────────────────────────────────────────────────
        (
            GdExpr::Call {
                callee: ca,
                args: aa,
                ..
            },
            GdExpr::Call {
                callee: cb,
                args: ab,
                ..
            },
        ) => {
            structurally_equal_expr(ca, cb)
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab)
                    .all(|(a, b)| structurally_equal_expr(a, b))
        }
        (
            GdExpr::MethodCall {
                receiver: ra,
                method: ma,
                args: aa,
                ..
            },
            GdExpr::MethodCall {
                receiver: rb,
                method: mb,
                args: ab,
                ..
            },
        ) => {
            ma == mb
                && structurally_equal_expr(ra, rb)
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab)
                    .all(|(a, b)| structurally_equal_expr(a, b))
        }
        (
            GdExpr::SuperCall {
                method: ma,
                args: aa,
                ..
            },
            GdExpr::SuperCall {
                method: mb,
                args: ab,
                ..
            },
        ) => {
            ma == mb
                && aa.len() == ab.len()
                && aa
                    .iter()
                    .zip(ab)
                    .all(|(a, b)| structurally_equal_expr(a, b))
        }

        // ── Access ───────────────────────────────────────────────
        (
            GdExpr::PropertyAccess {
                receiver: ra,
                property: pa,
                ..
            },
            GdExpr::PropertyAccess {
                receiver: rb,
                property: pb,
                ..
            },
        ) => pa == pb && structurally_equal_expr(ra, rb),

        (
            GdExpr::Subscript {
                receiver: ra,
                index: ia,
                ..
            },
            GdExpr::Subscript {
                receiver: rb,
                index: ib,
                ..
            },
        ) => structurally_equal_expr(ra, rb) && structurally_equal_expr(ia, ib),

        // ── Path leaves (same body: pa == pb) ────────────────────
        (GdExpr::GetNode { path: pa, .. }, GdExpr::GetNode { path: pb, .. })
        | (GdExpr::Preload { path: pa, .. }, GdExpr::Preload { path: pb, .. }) => pa == pb,

        // ── Operators ────────────────────────────────────────────
        (
            GdExpr::BinOp {
                op: oa,
                left: la,
                right: ra,
                ..
            },
            GdExpr::BinOp {
                op: ob,
                left: lb,
                right: rb,
                ..
            },
        ) => oa == ob && structurally_equal_expr(la, lb) && structurally_equal_expr(ra, rb),
        (
            GdExpr::UnaryOp {
                op: oa,
                operand: xa,
                ..
            },
            GdExpr::UnaryOp {
                op: ob,
                operand: xb,
                ..
            },
        ) => oa == ob && structurally_equal_expr(xa, xb),

        // Cast + Is share shape: type-tag + child expr
        (
            GdExpr::Cast {
                expr: ea,
                target_type: ta,
                ..
            },
            GdExpr::Cast {
                expr: eb,
                target_type: tb,
                ..
            },
        )
        | (
            GdExpr::Is {
                expr: ea,
                type_name: ta,
                ..
            },
            GdExpr::Is {
                expr: eb,
                type_name: tb,
                ..
            },
        ) => ta == tb && structurally_equal_expr(ea, eb),

        (
            GdExpr::Ternary {
                true_val: ta,
                condition: ca,
                false_val: fa,
                ..
            },
            GdExpr::Ternary {
                true_val: tb,
                condition: cb,
                false_val: fb,
                ..
            },
        ) => {
            structurally_equal_expr(ta, tb)
                && structurally_equal_expr(ca, cb)
                && structurally_equal_expr(fa, fb)
        }

        // ── Misc ─────────────────────────────────────────────────
        (GdExpr::Await { expr: ea, .. }, GdExpr::Await { expr: eb, .. }) => {
            structurally_equal_expr(ea, eb)
        }

        (GdExpr::Lambda { func: fa, .. }, GdExpr::Lambda { func: fb, .. }) => {
            fa.params.len() == fb.params.len()
                && fa
                    .params
                    .iter()
                    .zip(&fb.params)
                    .all(|(a, b)| a.name == b.name)
                && fa.body.len() == fb.body.len()
                && fa
                    .body
                    .iter()
                    .zip(&fb.body)
                    .all(|(a, b)| structurally_equal_stmt(a, b))
        }

        // ── Mismatched variants or Invalid ───────────────────────
        _ => false,
    }
}

/// Returns `true` if two statements are structurally identical.
pub fn structurally_equal_stmt(a: &GdStmt, b: &GdStmt) -> bool {
    match (a, b) {
        (GdStmt::Expr { expr: ea, .. }, GdStmt::Expr { expr: eb, .. }) => {
            structurally_equal_expr(ea, eb)
        }
        (GdStmt::Var(va), GdStmt::Var(vb)) => {
            va.name == vb.name
                && va.is_const == vb.is_const
                && match (&va.value, &vb.value) {
                    (Some(a), Some(b)) => structurally_equal_expr(a, b),
                    (None, None) => true,
                    _ => false,
                }
        }
        (
            GdStmt::Assign {
                target: ta,
                value: va,
                ..
            },
            GdStmt::Assign {
                target: tb,
                value: vb,
                ..
            },
        ) => structurally_equal_expr(ta, tb) && structurally_equal_expr(va, vb),
        (
            GdStmt::AugAssign {
                target: ta,
                op: oa,
                value: va,
                ..
            },
            GdStmt::AugAssign {
                target: tb,
                op: ob,
                value: vb,
                ..
            },
        ) => oa == ob && structurally_equal_expr(ta, tb) && structurally_equal_expr(va, vb),
        (GdStmt::Return { value: va, .. }, GdStmt::Return { value: vb, .. }) => match (va, vb) {
            (Some(a), Some(b)) => structurally_equal_expr(a, b),
            (None, None) => true,
            _ => false,
        },
        (GdStmt::Pass { .. }, GdStmt::Pass { .. })
        | (GdStmt::Break { .. }, GdStmt::Break { .. })
        | (GdStmt::Continue { .. }, GdStmt::Continue { .. })
        | (GdStmt::Breakpoint { .. }, GdStmt::Breakpoint { .. }) => true,
        _ => false,
    }
}
