use std::collections::HashSet;

use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NullAfterAwait;

impl LintRule for NullAfterAwait {
    fn name(&self) -> &'static str {
        "null-after-await"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Step 1: Collect nullable member vars (no init or init to null)
        let mut nullable_vars = HashSet::new();
        for decl in &file.declarations {
            if let GdDecl::Var(var) = decl
                && matches!(&var.value, None | Some(GdExpr::Null { .. }))
            {
                nullable_vars.insert(var.name);
            }
        }
        if nullable_vars.is_empty() {
            return diags;
        }

        // Step 2: Find vars assigned after await in any function
        let mut vars_assigned_after_await: HashSet<&str> = HashSet::new();
        for decl in &file.declarations {
            if let GdDecl::Func(func) = decl
                && body_contains_await(&func.body)
            {
                collect_assignments_after_await(
                    &func.body,
                    &nullable_vars,
                    &mut vars_assigned_after_await,
                );
            }
        }
        if vars_assigned_after_await.is_empty() {
            return diags;
        }

        // Step 3: Check _process/_physics_process for unguarded access
        for decl in &file.declarations {
            if let GdDecl::Func(func) = decl
                && matches!(func.name, "_process" | "_physics_process")
            {
                check_unguarded_access(&func.body, &vars_assigned_after_await, &mut diags);
            }
        }

        diags
    }
}

/// Check if any expression in the body contains an Await.
fn body_contains_await(stmts: &[GdStmt]) -> bool {
    let mut found = false;
    gd_ast::visit_body_exprs(stmts, &mut |expr| {
        if matches!(expr, GdExpr::Await { .. }) {
            found = true;
        }
    });
    found
}

/// Scan top-level statements in a function body. After seeing a statement
/// containing `await`, collect any assignments to nullable vars.
fn collect_assignments_after_await<'a>(
    stmts: &[GdStmt<'a>],
    nullable_vars: &HashSet<&str>,
    result: &mut HashSet<&'a str>,
) {
    let mut seen_await = false;
    for stmt in stmts {
        if !seen_await {
            if stmt_contains_await(stmt) {
                seen_await = true;
            }
            continue;
        }
        // After await: check for assignments to nullable vars
        collect_assign_targets(stmt, nullable_vars, result);
    }
}

/// Check if a single statement (or its nested expressions) contains an await.
fn stmt_contains_await(stmt: &GdStmt) -> bool {
    let mut found = false;
    gd_ast::visit_body_exprs(std::slice::from_ref(stmt), &mut |expr| {
        if matches!(expr, GdExpr::Await { .. }) {
            found = true;
        }
    });
    found
}

/// Collect assignment targets from a statement (recursing into if/match blocks).
fn collect_assign_targets<'a>(
    stmt: &GdStmt<'a>,
    nullable_vars: &HashSet<&str>,
    result: &mut HashSet<&'a str>,
) {
    match stmt {
        GdStmt::Assign { target, .. } | GdStmt::AugAssign { target, .. } => {
            if let GdExpr::Ident { name, .. } = target
                && nullable_vars.contains(name)
            {
                result.insert(name);
            }
        }
        GdStmt::If(if_stmt) => {
            for s in &if_stmt.body {
                collect_assign_targets(s, nullable_vars, result);
            }
            for (_, branch) in &if_stmt.elif_branches {
                for s in branch {
                    collect_assign_targets(s, nullable_vars, result);
                }
            }
            if let Some(else_body) = &if_stmt.else_body {
                for s in else_body {
                    collect_assign_targets(s, nullable_vars, result);
                }
            }
        }
        GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
            for s in body {
                collect_assign_targets(s, nullable_vars, result);
            }
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                for s in &arm.body {
                    collect_assign_targets(s, nullable_vars, result);
                }
            }
        }
        _ => {}
    }
}

/// Check a function body for unguarded access to risky variables.
fn check_unguarded_access(
    stmts: &[GdStmt],
    risky_vars: &HashSet<&str>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Collect variables guarded by if-statements at the top level
    let mut guarded: HashSet<&str> = HashSet::new();
    for stmt in stmts {
        if let GdStmt::If(if_stmt) = stmt {
            collect_idents_from_expr(&if_stmt.condition, &mut guarded);
        }
    }

    // Find unguarded identifier access (skip if-statement bodies which are guarded)
    for stmt in stmts {
        if matches!(stmt, GdStmt::If(_)) {
            continue; // Skip if-blocks (guarded)
        }
        find_unguarded_idents(stmt, risky_vars, &guarded, diags);
    }
}

/// Collect all identifiers from an expression.
fn collect_idents_from_expr<'a>(expr: &GdExpr<'a>, out: &mut HashSet<&'a str>) {
    if let GdExpr::Ident { name, .. } = expr {
        out.insert(name);
    }
    // Recurse into children
    match expr {
        GdExpr::BinOp { left, right, .. } => {
            collect_idents_from_expr(left, out);
            collect_idents_from_expr(right, out);
        }
        GdExpr::UnaryOp { operand, .. } => collect_idents_from_expr(operand, out),
        GdExpr::Call { callee, args, .. } => {
            collect_idents_from_expr(callee, out);
            for a in args {
                collect_idents_from_expr(a, out);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            collect_idents_from_expr(receiver, out);
            for a in args {
                collect_idents_from_expr(a, out);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => collect_idents_from_expr(receiver, out),
        _ => {}
    }
}

/// Find identifiers in a statement that reference risky vars without a guard.
fn find_unguarded_idents(
    stmt: &GdStmt,
    risky_vars: &HashSet<&str>,
    guarded: &HashSet<&str>,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Visit all expressions in this statement
    let mut check_expr = |expr: &GdExpr| {
        if let GdExpr::Ident { name, node, .. } = expr
            && risky_vars.contains(name)
            && !guarded.contains(name)
        {
            diags.push(LintDiagnostic {
                rule: "null-after-await",
                message: format!(
                    "`{name}` may be null after `await` — add a null check before using"
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: Some(node.end_position().column),
                fix: None,
                context_lines: None,
            });
        }
    };

    // Walk expressions in the statement (but NOT recursing into nested if/for/while bodies
    // from here — that would be handled by the top-level loop)
    match stmt {
        GdStmt::Expr { expr, .. } => visit_expr_flat(expr, &mut check_expr),
        GdStmt::Var(var) => {
            if let Some(value) = &var.value {
                visit_expr_flat(value, &mut check_expr);
            }
        }
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            visit_expr_flat(target, &mut check_expr);
            visit_expr_flat(value, &mut check_expr);
        }
        GdStmt::Return { value: Some(v), .. } => visit_expr_flat(v, &mut check_expr),
        _ => {}
    }
}

/// Visit an expression and all its children, calling f for each.
fn visit_expr_flat(expr: &GdExpr, f: &mut impl FnMut(&GdExpr)) {
    f(expr);
    match expr {
        GdExpr::BinOp { left, right, .. } => {
            visit_expr_flat(left, f);
            visit_expr_flat(right, f);
        }
        GdExpr::UnaryOp { operand, .. }
        | GdExpr::Cast { expr: operand, .. }
        | GdExpr::Is { expr: operand, .. }
        | GdExpr::Await { expr: operand, .. } => {
            visit_expr_flat(operand, f);
        }
        GdExpr::Call { callee, args, .. } => {
            visit_expr_flat(callee, f);
            for a in args {
                visit_expr_flat(a, f);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            visit_expr_flat(receiver, f);
            for a in args {
                visit_expr_flat(a, f);
            }
        }
        GdExpr::SuperCall { args, .. } => {
            for a in args {
                visit_expr_flat(a, f);
            }
        }
        GdExpr::PropertyAccess { receiver, .. } => visit_expr_flat(receiver, f),
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            visit_expr_flat(receiver, f);
            visit_expr_flat(index, f);
        }
        GdExpr::Ternary {
            true_val,
            condition,
            false_val,
            ..
        } => {
            visit_expr_flat(true_val, f);
            visit_expr_flat(condition, f);
            visit_expr_flat(false_val, f);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                visit_expr_flat(e, f);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                visit_expr_flat(k, f);
                visit_expr_flat(v, f);
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
        NullAfterAwait.check(&file, source, &config)
    }

    #[test]
    fn detects_unguarded_access_after_await() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(!diags.is_empty());
        assert!(diags[0].message.contains("enemy"));
        assert!(diags[0].message.contains("null"));
    }

    #[test]
    fn no_warning_with_null_guard() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tif enemy:
\t\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_var_with_initializer() {
        let source = "\
var enemy = Node2D.new()

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_without_await() {
        let source = "\
var enemy = null

func load_enemy():
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_no_process_function() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func update():
\tenemy.move()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_in_physics_process() {
        let source = "\
var enemy = null

func load_enemy():
\tawait get_tree().create_timer(1.0).timeout
\tenemy = preload(\"res://enemy.tscn\").instantiate()

func _physics_process(delta):
\tenemy.move()
";
        let diags = check(source);
        assert!(!diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!NullAfterAwait.default_enabled());
    }
}
