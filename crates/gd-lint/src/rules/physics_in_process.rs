use gd_core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct PhysicsInProcess;

impl LintRule for PhysicsInProcess {
    fn name(&self) -> &'static str {
        "physics-in-process"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Performance
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        find_process_functions(&file.declarations, &mut diags);
        diags
    }
}

fn find_process_functions(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl
            && func.name == "_process"
        {
            find_physics_in_stmts(&func.body, diags);
        }
        if let GdDecl::Class(class) = decl {
            find_process_functions(&class.declarations, diags);
        }
    }
}

fn find_physics_in_stmts(stmts: &[GdStmt<'_>], diags: &mut Vec<LintDiagnostic>) {
    for stmt in stmts {
        find_physics_in_stmt(stmt, diags);
    }
}

fn find_physics_in_stmt(stmt: &GdStmt<'_>, diags: &mut Vec<LintDiagnostic>) {
    // Check expressions in this statement for physics calls
    visit_stmt_exprs(stmt, &mut |expr| {
        check_physics_expr(expr, diags);
    });

    // Recurse into nested statement bodies (but not into nested functions)
    match stmt {
        GdStmt::If(gif) => {
            find_physics_in_stmts(&gif.body, diags);
            for (_, branch_body) in &gif.elif_branches {
                find_physics_in_stmts(branch_body, diags);
            }
            if let Some(else_body) = &gif.else_body {
                find_physics_in_stmts(else_body, diags);
            }
        }
        GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
            find_physics_in_stmts(body, diags);
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                find_physics_in_stmts(&arm.body, diags);
            }
        }
        _ => {}
    }
}

fn visit_stmt_exprs<'a>(stmt: &GdStmt<'a>, f: &mut impl FnMut(&GdExpr<'a>)) {
    match stmt {
        GdStmt::Expr { expr, .. } => visit_expr(expr, f),
        GdStmt::Var(var) => {
            if let Some(val) = &var.value {
                visit_expr(val, f);
            }
        }
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            visit_expr(target, f);
            visit_expr(value, f);
        }
        GdStmt::Return {
            value: Some(val), ..
        } => visit_expr(val, f),
        GdStmt::If(gif) => {
            visit_expr(&gif.condition, f);
        }
        GdStmt::For { iter, .. } => visit_expr(iter, f),
        GdStmt::While { condition, .. } => visit_expr(condition, f),
        _ => {}
    }
}

fn visit_expr<'a>(expr: &GdExpr<'a>, f: &mut impl FnMut(&GdExpr<'a>)) {
    f(expr);
    match expr {
        GdExpr::Call { callee, args, .. } => {
            visit_expr(callee, f);
            for arg in args {
                visit_expr(arg, f);
            }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            visit_expr(receiver, f);
            for arg in args {
                visit_expr(arg, f);
            }
        }
        GdExpr::BinOp { left, right, .. } => {
            visit_expr(left, f);
            visit_expr(right, f);
        }
        GdExpr::UnaryOp { operand, .. } => visit_expr(operand, f),
        GdExpr::PropertyAccess { receiver, .. } => visit_expr(receiver, f),
        GdExpr::Subscript {
            receiver, index, ..
        } => {
            visit_expr(receiver, f);
            visit_expr(index, f);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements {
                visit_expr(e, f);
            }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs {
                visit_expr(k, f);
                visit_expr(v, f);
            }
        }
        GdExpr::Ternary {
            true_val,
            condition,
            false_val,
            ..
        } => {
            visit_expr(true_val, f);
            visit_expr(condition, f);
            visit_expr(false_val, f);
        }
        GdExpr::Await { expr: inner, .. }
        | GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. } => visit_expr(inner, f),
        GdExpr::SuperCall { args, .. } => {
            for arg in args {
                visit_expr(arg, f);
            }
        }
        _ => {}
    }
}

fn check_physics_expr(expr: &GdExpr<'_>, diags: &mut Vec<LintDiagnostic>) {
    // Bare call: move_and_slide() (implicit self)
    if let GdExpr::Call { node, callee, .. } = expr
        && let GdExpr::Ident { name, .. } = callee.as_ref()
        && gd_class_db::curated::is_physics_method(name)
    {
        diags.push(make_diagnostic(name, node));
    }

    // Method call: self.move_and_slide() or body.apply_force(...)
    if let GdExpr::MethodCall { node, method, .. } = expr
        && gd_class_db::curated::is_physics_method(method)
    {
        diags.push(make_diagnostic(method, node));
    }
}

fn make_diagnostic(method: &str, node: &tree_sitter::Node<'_>) -> LintDiagnostic {
    LintDiagnostic {
        rule: "physics-in-process",
        message: format!("`{method}()` should be called in _physics_process(), not _process()"),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        PhysicsInProcess.check(&file, source, &config)
    }

    #[test]
    fn detects_move_and_slide_in_process() {
        let source = "func _process(delta: float) -> void:\n\tmove_and_slide()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "physics-in-process");
        assert!(diags[0].message.contains("move_and_slide()"));
    }

    #[test]
    fn detects_apply_force_in_process() {
        let source = "func _process(delta: float) -> void:\n\tapply_force(Vector2(0, 10))\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("apply_force()"));
    }

    #[test]
    fn detects_self_move_and_slide() {
        let source = "func _process(delta: float) -> void:\n\tself.move_and_slide()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("move_and_slide()"));
    }

    #[test]
    fn detects_apply_impulse_on_object() {
        let source = "func _process(delta: float) -> void:\n\tbody.apply_impulse(Vector2.UP)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("apply_impulse()"));
    }

    #[test]
    fn no_warning_in_physics_process() {
        let source = "func _physics_process(delta: float) -> void:\n\tmove_and_slide()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_in_regular_function() {
        let source = "func helper() -> void:\n\tmove_and_slide()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_physics_calls() {
        let source = "func _process(delta: float) -> void:\n\tmove_and_slide()\n\tapply_force(Vector2.ZERO)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn detects_move_and_collide() {
        let source = "func _process(delta: float) -> void:\n\tvar col := move_and_collide(velocity * delta)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("move_and_collide()"));
    }

    #[test]
    fn detects_set_velocity() {
        let source = "func _process(delta: float) -> void:\n\tset_velocity(Vector2(100, 0))\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("set_velocity()"));
    }

    #[test]
    fn detects_in_inner_class() {
        let source = "class Inner:\n\tfunc _process(delta: float) -> void:\n\t\tmove_and_slide()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_for_nested_function() {
        let source = "func _process(delta: float) -> void:\n\tpass\n\nfunc helper() -> void:\n\tmove_and_slide()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_in_conditional() {
        let source = "func _process(delta: float) -> void:\n\tif is_on_floor():\n\t\tapply_force(Vector2.UP * 100)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
