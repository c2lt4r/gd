//! AST rewriter — pure, immutable tree transformations.
//!
//! Walks owned AST types bottom-up, applying a rule closure to each node.
//! Rewritten nodes get their span cleared (`None`) so the printer knows to
//! emit fresh text.  Unchanged subtrees keep their original spans for
//! verbatim source emission.
//!
//! # Dirty propagation
//!
//! When any child's span becomes `None` (meaning it was rewritten), the
//! parent's span is also cleared.  This ensures the printer recurses into
//! structurally-changed nodes rather than emitting stale original text.

use crate::ast_owned::{
    OwnedClass, OwnedDecl, OwnedEnumMember, OwnedExpr, OwnedFile, OwnedFunc, OwnedIf,
    OwnedMatchArm, OwnedParam, OwnedStmt, OwnedVar,
};

// ═══════════════════════════════════════════════════════════════════════
//  Public API
// ═══════════════════════════════════════════════════════════════════════

/// Rewrite every expression in a file (bottom-up).
pub fn rewrite_file(file: OwnedFile, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedFile {
    let declarations: Vec<OwnedDecl> = file
        .declarations
        .into_iter()
        .map(|d| rewrite_decl(d, rule))
        .collect();
    let dirty = declarations.iter().any(|d| decl_span(d).is_none());
    OwnedFile {
        span: if dirty { None } else { file.span },
        class_name: file.class_name,
        extends: file.extends,
        is_tool: file.is_tool,
        has_static_unload: file.has_static_unload,
        declarations,
    }
}

/// Rewrite a single expression (bottom-up): children first, then this node.
pub fn rewrite_expr(expr: OwnedExpr, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedExpr {
    let with_children = rewrite_expr_children(expr, rule);
    rule(with_children)
}

/// Rewrite all expressions inside a statement (bottom-up).
pub fn rewrite_stmt(stmt: OwnedStmt, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedStmt {
    rewrite_stmt_inner(stmt, rule)
}

// ═══════════════════════════════════════════════════════════════════════
//  Expression rewriting
// ═══════════════════════════════════════════════════════════════════════

/// Recursively rewrite children of an expression, then propagate dirty.
#[allow(clippy::too_many_lines)]
fn rewrite_expr_children(expr: OwnedExpr, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedExpr {
    match expr {
        // ── Leaves (no children to rewrite) ────────────────────────
        OwnedExpr::IntLiteral { .. }
        | OwnedExpr::FloatLiteral { .. }
        | OwnedExpr::StringLiteral { .. }
        | OwnedExpr::StringName { .. }
        | OwnedExpr::Bool { .. }
        | OwnedExpr::Null { .. }
        | OwnedExpr::Ident { .. }
        | OwnedExpr::GetNode { .. }
        | OwnedExpr::Preload { .. }
        | OwnedExpr::Invalid { .. } => expr,

        // ── Compound nodes ─────────────────────────────────────────
        OwnedExpr::Array { span, elements } => {
            let elements: Vec<_> = elements.into_iter().map(|e| rewrite_expr(e, rule)).collect();
            let dirty = elements.iter().any(|e| e.span().is_none());
            OwnedExpr::Array {
                span: if dirty { None } else { span },
                elements,
            }
        }

        OwnedExpr::Dict { span, pairs } => {
            let pairs: Vec<_> = pairs
                .into_iter()
                .map(|(k, v)| (rewrite_expr(k, rule), rewrite_expr(v, rule)))
                .collect();
            let dirty = pairs.iter().any(|(k, v)| k.span().is_none() || v.span().is_none());
            OwnedExpr::Dict {
                span: if dirty { None } else { span },
                pairs,
            }
        }

        OwnedExpr::Call { span, callee, args } => {
            let callee = Box::new(rewrite_expr(*callee, rule));
            let args: Vec<_> = args.into_iter().map(|a| rewrite_expr(a, rule)).collect();
            let dirty = callee.span().is_none() || args.iter().any(|a| a.span().is_none());
            OwnedExpr::Call {
                span: if dirty { None } else { span },
                callee,
                args,
            }
        }

        OwnedExpr::MethodCall { span, receiver, method, args } => {
            let receiver = Box::new(rewrite_expr(*receiver, rule));
            let args: Vec<_> = args.into_iter().map(|a| rewrite_expr(a, rule)).collect();
            let dirty = receiver.span().is_none() || args.iter().any(|a| a.span().is_none());
            OwnedExpr::MethodCall {
                span: if dirty { None } else { span },
                receiver,
                method,
                args,
            }
        }

        OwnedExpr::SuperCall { span, method, args } => {
            let args: Vec<_> = args.into_iter().map(|a| rewrite_expr(a, rule)).collect();
            let dirty = args.iter().any(|a| a.span().is_none());
            OwnedExpr::SuperCall {
                span: if dirty { None } else { span },
                method,
                args,
            }
        }

        OwnedExpr::PropertyAccess { span, receiver, property } => {
            let receiver = Box::new(rewrite_expr(*receiver, rule));
            let dirty = receiver.span().is_none();
            OwnedExpr::PropertyAccess {
                span: if dirty { None } else { span },
                receiver,
                property,
            }
        }

        OwnedExpr::Subscript { span, receiver, index } => {
            let receiver = Box::new(rewrite_expr(*receiver, rule));
            let index = Box::new(rewrite_expr(*index, rule));
            let dirty = receiver.span().is_none() || index.span().is_none();
            OwnedExpr::Subscript {
                span: if dirty { None } else { span },
                receiver,
                index,
            }
        }

        OwnedExpr::BinOp { span, left, op, right } => {
            let left = Box::new(rewrite_expr(*left, rule));
            let right = Box::new(rewrite_expr(*right, rule));
            let dirty = left.span().is_none() || right.span().is_none();
            OwnedExpr::BinOp {
                span: if dirty { None } else { span },
                left,
                op,
                right,
            }
        }

        OwnedExpr::UnaryOp { span, op, operand } => {
            let operand = Box::new(rewrite_expr(*operand, rule));
            let dirty = operand.span().is_none();
            OwnedExpr::UnaryOp {
                span: if dirty { None } else { span },
                operand,
                op,
            }
        }

        OwnedExpr::Cast { span, expr, target_type } => {
            let expr = Box::new(rewrite_expr(*expr, rule));
            let dirty = expr.span().is_none();
            OwnedExpr::Cast {
                span: if dirty { None } else { span },
                expr,
                target_type,
            }
        }

        OwnedExpr::Is { span, expr, type_name } => {
            let expr = Box::new(rewrite_expr(*expr, rule));
            let dirty = expr.span().is_none();
            OwnedExpr::Is {
                span: if dirty { None } else { span },
                expr,
                type_name,
            }
        }

        OwnedExpr::Ternary { span, true_val, condition, false_val } => {
            let true_val = Box::new(rewrite_expr(*true_val, rule));
            let condition = Box::new(rewrite_expr(*condition, rule));
            let false_val = Box::new(rewrite_expr(*false_val, rule));
            let dirty =
                true_val.span().is_none() || condition.span().is_none() || false_val.span().is_none();
            OwnedExpr::Ternary {
                span: if dirty { None } else { span },
                true_val,
                condition,
                false_val,
            }
        }

        OwnedExpr::Await { span, expr } => {
            let expr = Box::new(rewrite_expr(*expr, rule));
            let dirty = expr.span().is_none();
            OwnedExpr::Await {
                span: if dirty { None } else { span },
                expr,
            }
        }

        OwnedExpr::Lambda { span, func } => {
            let func = Box::new(rewrite_func(*func, rule));
            let dirty = func.span.is_none();
            OwnedExpr::Lambda {
                span: if dirty { None } else { span },
                func,
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Statement rewriting
// ═══════════════════════════════════════════════════════════════════════

fn rewrite_stmt_inner(stmt: OwnedStmt, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedStmt {
    match stmt {
        OwnedStmt::Expr { span, expr } => {
            let expr = rewrite_expr(expr, rule);
            let dirty = expr.span().is_none();
            OwnedStmt::Expr {
                span: if dirty { None } else { span },
                expr,
            }
        }

        OwnedStmt::Var(v) => OwnedStmt::Var(rewrite_var(v, rule)),

        OwnedStmt::Assign { span, target, value } => {
            let target = rewrite_expr(target, rule);
            let value = rewrite_expr(value, rule);
            let dirty = target.span().is_none() || value.span().is_none();
            OwnedStmt::Assign {
                span: if dirty { None } else { span },
                target,
                value,
            }
        }

        OwnedStmt::AugAssign { span, target, op, value } => {
            let target = rewrite_expr(target, rule);
            let value = rewrite_expr(value, rule);
            let dirty = target.span().is_none() || value.span().is_none();
            OwnedStmt::AugAssign {
                span: if dirty { None } else { span },
                target,
                op,
                value,
            }
        }

        OwnedStmt::Return { span, value } => {
            let value = value.map(|v| rewrite_expr(v, rule));
            let dirty = value.as_ref().is_some_and(|v| v.span().is_none());
            OwnedStmt::Return {
                span: if dirty { None } else { span },
                value,
            }
        }

        OwnedStmt::If(i) => OwnedStmt::If(rewrite_if(i, rule)),

        OwnedStmt::For { span, var, var_type, iter, body } => {
            let iter = rewrite_expr(iter, rule);
            let body: Vec<_> = body.into_iter().map(|s| rewrite_stmt_inner(s, rule)).collect();
            let dirty =
                iter.span().is_none() || body.iter().any(|s| s.span().is_none());
            OwnedStmt::For {
                span: if dirty { None } else { span },
                var,
                var_type,
                iter,
                body,
            }
        }

        OwnedStmt::While { span, condition, body } => {
            let condition = rewrite_expr(condition, rule);
            let body: Vec<_> = body.into_iter().map(|s| rewrite_stmt_inner(s, rule)).collect();
            let dirty =
                condition.span().is_none() || body.iter().any(|s| s.span().is_none());
            OwnedStmt::While {
                span: if dirty { None } else { span },
                condition,
                body,
            }
        }

        OwnedStmt::Match { span, value, arms } => {
            let value = rewrite_expr(value, rule);
            let arms: Vec<_> = arms.into_iter().map(|a| rewrite_match_arm(a, rule)).collect();
            let dirty =
                value.span().is_none() || arms.iter().any(|a| a.span.is_none());
            OwnedStmt::Match {
                span: if dirty { None } else { span },
                value,
                arms,
            }
        }

        // Simple statements with no expressions to rewrite.
        OwnedStmt::Pass { .. }
        | OwnedStmt::Break { .. }
        | OwnedStmt::Continue { .. }
        | OwnedStmt::Breakpoint { .. }
        | OwnedStmt::Invalid { .. } => stmt,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Declaration / helper rewriting
// ═══════════════════════════════════════════════════════════════════════

fn rewrite_decl(decl: OwnedDecl, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedDecl {
    match decl {
        OwnedDecl::Func(f) => OwnedDecl::Func(rewrite_func(f, rule)),
        OwnedDecl::Var(v) => OwnedDecl::Var(rewrite_var(v, rule)),
        OwnedDecl::Signal(s) => OwnedDecl::Signal(rewrite_signal(s, rule)),
        OwnedDecl::Enum(e) => OwnedDecl::Enum(rewrite_enum(e, rule)),
        OwnedDecl::Class(c) => OwnedDecl::Class(rewrite_class(c, rule)),
        OwnedDecl::Stmt(s) => OwnedDecl::Stmt(rewrite_stmt_inner(s, rule)),
    }
}

fn rewrite_func(f: OwnedFunc, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedFunc {
    let params: Vec<_> = f.params.into_iter().map(|p| rewrite_param(p, rule)).collect();
    let body: Vec<_> = f.body.into_iter().map(|s| rewrite_stmt_inner(s, rule)).collect();
    let annotations: Vec<_> = f
        .annotations
        .into_iter()
        .map(|a| rewrite_annotation(a, rule))
        .collect();
    let dirty = params.iter().any(|p| p.span.is_none())
        || body.iter().any(|s| s.span().is_none())
        || annotations.iter().any(|a| a.span.is_none());
    OwnedFunc {
        span: if dirty { None } else { f.span },
        name: f.name,
        params,
        return_type: f.return_type,
        body,
        is_static: f.is_static,
        is_constructor: f.is_constructor,
        annotations,
        doc: f.doc,
    }
}

fn rewrite_param(p: OwnedParam, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedParam {
    let default = p.default.map(|d| rewrite_expr(d, rule));
    let dirty = default.as_ref().is_some_and(|d| d.span().is_none());
    OwnedParam {
        span: if dirty { None } else { p.span },
        name: p.name,
        type_ann: p.type_ann,
        default,
    }
}

fn rewrite_var(v: OwnedVar, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedVar {
    let value = v.value.map(|val| rewrite_expr(val, rule));
    let annotations: Vec<_> = v
        .annotations
        .into_iter()
        .map(|a| rewrite_annotation(a, rule))
        .collect();
    let dirty = value.as_ref().is_some_and(|val| val.span().is_none())
        || annotations.iter().any(|a| a.span.is_none());
    OwnedVar {
        span: if dirty { None } else { v.span },
        name: v.name,
        type_ann: v.type_ann,
        value,
        is_const: v.is_const,
        is_static: v.is_static,
        annotations,
        setter: v.setter,
        getter: v.getter,
        doc: v.doc,
    }
}

fn rewrite_annotation(
    a: crate::ast_owned::OwnedAnnotation,
    rule: &impl Fn(OwnedExpr) -> OwnedExpr,
) -> crate::ast_owned::OwnedAnnotation {
    let args: Vec<_> = a.args.into_iter().map(|e| rewrite_expr(e, rule)).collect();
    let dirty = args.iter().any(|e| e.span().is_none());
    crate::ast_owned::OwnedAnnotation {
        span: if dirty { None } else { a.span },
        name: a.name,
        args,
    }
}

fn rewrite_signal(
    s: crate::ast_owned::OwnedSignal,
    rule: &impl Fn(OwnedExpr) -> OwnedExpr,
) -> crate::ast_owned::OwnedSignal {
    let params: Vec<_> = s.params.into_iter().map(|p| rewrite_param(p, rule)).collect();
    let dirty = params.iter().any(|p| p.span.is_none());
    crate::ast_owned::OwnedSignal {
        span: if dirty { None } else { s.span },
        name: s.name,
        params,
        doc: s.doc,
    }
}

fn rewrite_enum(
    e: crate::ast_owned::OwnedEnum,
    rule: &impl Fn(OwnedExpr) -> OwnedExpr,
) -> crate::ast_owned::OwnedEnum {
    let members: Vec<_> = e
        .members
        .into_iter()
        .map(|m| rewrite_enum_member(m, rule))
        .collect();
    let dirty = members.iter().any(|m| m.span.is_none());
    crate::ast_owned::OwnedEnum {
        span: if dirty { None } else { e.span },
        name: e.name,
        members,
        doc: e.doc,
    }
}

fn rewrite_enum_member(
    m: OwnedEnumMember,
    rule: &impl Fn(OwnedExpr) -> OwnedExpr,
) -> OwnedEnumMember {
    let value = m.value.map(|v| rewrite_expr(v, rule));
    let dirty = value.as_ref().is_some_and(|v| v.span().is_none());
    OwnedEnumMember {
        span: if dirty { None } else { m.span },
        name: m.name,
        value,
    }
}

fn rewrite_class(c: OwnedClass, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedClass {
    let declarations: Vec<_> = c
        .declarations
        .into_iter()
        .map(|d| rewrite_decl(d, rule))
        .collect();
    let dirty = declarations.iter().any(|d| decl_span(d).is_none());
    OwnedClass {
        span: if dirty { None } else { c.span },
        name: c.name,
        extends: c.extends,
        declarations,
        doc: c.doc,
    }
}

fn rewrite_if(i: OwnedIf, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedIf {
    let condition = rewrite_expr(i.condition, rule);
    let body: Vec<_> = i.body.into_iter().map(|s| rewrite_stmt_inner(s, rule)).collect();
    let elif_branches: Vec<_> = i
        .elif_branches
        .into_iter()
        .map(|(cond, stmts)| {
            let cond = rewrite_expr(cond, rule);
            let stmts: Vec<_> = stmts.into_iter().map(|s| rewrite_stmt_inner(s, rule)).collect();
            (cond, stmts)
        })
        .collect();
    let else_body = i
        .else_body
        .map(|stmts| stmts.into_iter().map(|s| rewrite_stmt_inner(s, rule)).collect());
    let dirty = condition.span().is_none()
        || body.iter().any(|s| s.span().is_none())
        || elif_branches
            .iter()
            .any(|(c, ss)| c.span().is_none() || ss.iter().any(|s| s.span().is_none()))
        || else_body
            .as_ref()
            .is_some_and(|ss: &Vec<OwnedStmt>| ss.iter().any(|s| s.span().is_none()));
    OwnedIf {
        span: if dirty { None } else { i.span },
        condition,
        body,
        elif_branches,
        else_body,
    }
}

fn rewrite_match_arm(arm: OwnedMatchArm, rule: &impl Fn(OwnedExpr) -> OwnedExpr) -> OwnedMatchArm {
    let patterns: Vec<_> = arm.patterns.into_iter().map(|p| rewrite_expr(p, rule)).collect();
    let guard = arm.guard.map(|g| rewrite_expr(g, rule));
    let body: Vec<_> = arm.body.into_iter().map(|s| rewrite_stmt_inner(s, rule)).collect();
    let dirty = patterns.iter().any(|p| p.span().is_none())
        || guard.as_ref().is_some_and(|g| g.span().is_none())
        || body.iter().any(|s| s.span().is_none());
    OwnedMatchArm {
        span: if dirty { None } else { arm.span },
        patterns,
        guard,
        body,
    }
}

// ═══════════════════════════════════════════════════════════════════════
//  Helpers
// ═══════════════════════════════════════════════════════════════════════

fn decl_span(decl: &OwnedDecl) -> Option<crate::ast_owned::Span> {
    match decl {
        OwnedDecl::Func(f) => f.span,
        OwnedDecl::Var(v) => v.span,
        OwnedDecl::Signal(s) => s.span,
        OwnedDecl::Enum(e) => e.span,
        OwnedDecl::Class(c) => c.span,
        OwnedDecl::Stmt(s) => s.span(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast_owned::OwnedFile;
    use crate::gd_ast;
    use crate::parser;
    use crate::printer;

    fn parse_to_owned(src: &str) -> OwnedFile {
        let tree = parser::parse(src).unwrap();
        let file = gd_ast::convert(&tree, src);
        OwnedFile::from_borrowed(&file)
    }

    // ── Round-trip ───────────────────────────────────────────────────

    #[test]
    fn round_trip_simple_function() {
        let src = "func hello():\n\tpass\n";
        let file = parse_to_owned(src);
        let printed = printer::print_file(&file, src);
        assert_eq!(printed, src);
    }

    #[test]
    fn round_trip_with_expressions() {
        let src = "func calc(a, b):\n\treturn a + b\n";
        let file = parse_to_owned(src);
        let printed = printer::print_file(&file, src);
        assert_eq!(printed, src);
    }

    #[test]
    fn round_trip_method_call() {
        let src = "func run():\n\tobj.damage(10)\n";
        let file = parse_to_owned(src);
        let printed = printer::print_file(&file, src);
        assert_eq!(printed, src);
    }

    #[test]
    fn round_trip_class_with_extends() {
        let src = "class_name Player\nextends CharacterBody2D\n\nvar health: int = 100\n\nfunc take_damage(amount: int):\n\thealth -= amount\n";
        let file = parse_to_owned(src);
        let printed = printer::print_file(&file, src);
        assert_eq!(printed, src);
    }

    #[test]
    fn round_trip_if_elif_else() {
        let src = "func check(x):\n\tif x > 10:\n\t\tprint(\"big\")\n\telif x > 5:\n\t\tprint(\"medium\")\n\telse:\n\t\tprint(\"small\")\n";
        let file = parse_to_owned(src);
        let printed = printer::print_file(&file, src);
        assert_eq!(printed, src);
    }

    // ── Identity rewrite (no changes) ───────────────────────────────

    #[test]
    fn identity_rewrite_preserves_spans() {
        let src = "func greet():\n\tprint(\"hello\")\n";
        let file = parse_to_owned(src);
        let rewritten = rewrite_file(file, &|expr| expr);
        // All spans should be preserved when no changes are made.
        assert!(rewritten.span.is_some());
        let printed = printer::print_file(&rewritten, src);
        assert_eq!(printed, src);
    }

    // ── Simple rename rewrite ───────────────────────────────────────

    #[test]
    fn rename_identifier() {
        let src = "func run():\n\tdamage(10)\n";
        let file = parse_to_owned(src);
        let rewritten = rewrite_file(file, &|expr| match expr {
            OwnedExpr::Ident { name, .. } if name == "damage" => OwnedExpr::Ident {
                span: None,
                name: "take_damage".to_string(),
            },
            other => other,
        });

        let printed = printer::print_file(&rewritten, src);
        assert!(printed.contains("take_damage"), "printed: {printed}");
        assert!(!printed.contains("\tdamage("), "printed: {printed}");
    }

    #[test]
    fn rename_method_call() {
        let src = "func run():\n\tplayer.damage(10)\n";
        let file = parse_to_owned(src);
        let rewritten = rewrite_file(file, &|expr| match expr {
            OwnedExpr::MethodCall { receiver, method, args, .. } if method == "damage" => {
                OwnedExpr::MethodCall {
                    span: None,
                    receiver,
                    method: "take_damage".to_string(),
                    args,
                }
            }
            other => other,
        });

        let printed = printer::print_file(&rewritten, src);
        assert!(printed.contains("player.take_damage(10)"), "printed: {printed}");
    }

    // ── Call-chain rewrite (the motivating case) ────────────────────

    #[test]
    fn call_chain_rewrite_insert_sub_object() {
        // obj.method() → obj.sub.method()
        let src = "func run():\n\tobj.damage(10)\n";
        let file = parse_to_owned(src);

        let rewritten = rewrite_file(file, &|expr| match expr {
            OwnedExpr::MethodCall { receiver, method, args, .. } if method == "damage" => {
                OwnedExpr::MethodCall {
                    span: None,
                    receiver: Box::new(OwnedExpr::PropertyAccess {
                        span: None,
                        receiver,
                        property: "combat".to_string(),
                    }),
                    method,
                    args,
                }
            }
            other => other,
        });

        let printed = printer::print_file(&rewritten, src);
        assert!(
            printed.contains("obj.combat.damage(10)"),
            "Expected obj.combat.damage(10), got: {printed}"
        );
    }

    #[test]
    fn call_chain_rewrite_multiple_sites() {
        let src = "func run():\n\tplayer.damage(5)\n\tenemy.heal(10)\n\tplayer.damage(20)\n";
        let file = parse_to_owned(src);

        let rewritten = rewrite_file(file, &|expr| match expr {
            OwnedExpr::MethodCall { receiver, method, args, .. } if method == "damage" => {
                OwnedExpr::MethodCall {
                    span: None,
                    receiver: Box::new(OwnedExpr::PropertyAccess {
                        span: None,
                        receiver,
                        property: "stats".to_string(),
                    }),
                    method,
                    args,
                }
            }
            other => other,
        });

        let printed = printer::print_file(&rewritten, src);
        // Both damage calls should be rewritten.
        assert!(printed.contains("player.stats.damage(5)"), "printed: {printed}");
        assert!(printed.contains("player.stats.damage(20)"), "printed: {printed}");
        // heal should be untouched.
        assert!(printed.contains("enemy.heal(10)"), "printed: {printed}");
    }

    // ── Dirty propagation ───────────────────────────────────────────

    #[test]
    fn dirty_propagates_to_parent() {
        let src = "func run():\n\tvar x = damage()\n";
        let file = parse_to_owned(src);

        let rewritten = rewrite_file(file, &|expr| match expr {
            OwnedExpr::Ident { name, .. } if name == "damage" => OwnedExpr::Ident {
                span: None,
                name: "take_damage".to_string(),
            },
            other => other,
        });

        // The file span should be cleared because a descendant was rewritten.
        assert!(rewritten.span.is_none());
        // But the output should still be valid.
        let printed = printer::print_file(&rewritten, src);
        assert!(printed.contains("take_damage"), "printed: {printed}");
    }
}
