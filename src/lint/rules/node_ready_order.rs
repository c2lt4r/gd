use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct NodeReadyOrder;

impl LintRule for NodeReadyOrder {
    fn name(&self) -> &'static str {
        "node-ready-order"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        find_init_functions(&file.declarations, source, &mut diags);
        diags
    }
}

fn find_init_functions(decls: &[GdDecl<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl
            && (func.name == "_init" || func.is_constructor)
        {
            scan_stmts_for_get_node(&func.body, source, diags);
        }
        if let GdDecl::Class(class) = decl {
            find_init_functions(&class.declarations, source, diags);
        }
    }
}

fn scan_stmts_for_get_node(stmts: &[GdStmt<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    for stmt in stmts {
        visit_stmt_exprs(stmt, &mut |expr| {
            check_node_access(expr, source, diags);
        });

        // Recurse into nested bodies (but not nested functions/lambdas)
        match stmt {
            GdStmt::If(gif) => {
                scan_stmts_for_get_node(&gif.body, source, diags);
                for (_, branch_body) in &gif.elif_branches {
                    scan_stmts_for_get_node(branch_body, source, diags);
                }
                if let Some(else_body) = &gif.else_body {
                    scan_stmts_for_get_node(else_body, source, diags);
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                scan_stmts_for_get_node(body, source, diags);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    scan_stmts_for_get_node(&arm.body, source, diags);
                }
            }
            _ => {}
        }
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
        GdStmt::Assign { target, value, .. }
        | GdStmt::AugAssign { target, value, .. } => {
            visit_expr(target, f);
            visit_expr(value, f);
        }
        GdStmt::Return { value: Some(val), .. } => visit_expr(val, f),
        GdStmt::If(gif) => visit_expr(&gif.condition, f),
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
            for arg in args { visit_expr(arg, f); }
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            visit_expr(receiver, f);
            for arg in args { visit_expr(arg, f); }
        }
        GdExpr::BinOp { left, right, .. } => {
            visit_expr(left, f);
            visit_expr(right, f);
        }
        GdExpr::UnaryOp { operand, .. } => visit_expr(operand, f),
        GdExpr::PropertyAccess { receiver, .. } => visit_expr(receiver, f),
        GdExpr::Subscript { receiver, index, .. } => {
            visit_expr(receiver, f);
            visit_expr(index, f);
        }
        GdExpr::Array { elements, .. } => {
            for e in elements { visit_expr(e, f); }
        }
        GdExpr::Dict { pairs, .. } => {
            for (k, v) in pairs { visit_expr(k, f); visit_expr(v, f); }
        }
        GdExpr::Ternary { true_val, condition, false_val, .. } => {
            visit_expr(true_val, f);
            visit_expr(condition, f);
            visit_expr(false_val, f);
        }
        GdExpr::Await { expr: inner, .. }
        | GdExpr::Cast { expr: inner, .. }
        | GdExpr::Is { expr: inner, .. } => visit_expr(inner, f),
        GdExpr::SuperCall { args, .. } => {
            for arg in args { visit_expr(arg, f); }
        }
        _ => {}
    }
}

fn check_node_access(expr: &GdExpr<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // $NodePath syntax
    if let GdExpr::GetNode { node, path } = expr {
        diags.push(LintDiagnostic {
            rule: "node-ready-order",
            message: format!(
                "`{path}` in _init() may fail; nodes are not ready until _ready()"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: None,
            context_lines: None,
        });
    }

    // Bare call: get_node(...) or get_node_or_null(...)
    if let GdExpr::Call { node, callee, .. } = expr
        && let GdExpr::Ident { name, .. } = callee.as_ref()
        && matches!(*name, "get_node" | "get_node_or_null")
    {
        diags.push(LintDiagnostic {
            rule: "node-ready-order",
            message: format!(
                "`{name}(...)` in _init() may fail; nodes are not ready until _ready()"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }

    // Method call: obj.get_node(...) or obj.get_node_or_null(...)
    if let GdExpr::MethodCall { node, method, .. } = expr
        && matches!(*method, "get_node" | "get_node_or_null")
    {
        let full_text = &source[node.byte_range()];
        diags.push(LintDiagnostic {
            rule: "node-ready-order",
            message: format!(
                "`{full_text}` in _init() may fail; nodes are not ready until _ready()"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::LintConfig;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = crate::core::parser::parse(source).unwrap();
        let file = crate::core::gd_ast::convert(&tree, source);
        NodeReadyOrder.check(&file, source, &LintConfig::default())
    }

    #[test]
    fn dollar_sign_in_init() {
        let src = "func _init() -> void:\n\tvar child: Node = $Something\n\tprint(child)\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$Something"));
        assert!(diags[0].message.contains("_init()"));
    }

    #[test]
    fn get_node_call_in_init() {
        let src = "func _init() -> void:\n\tvar child = get_node(\"Sprite\")\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node"));
    }

    #[test]
    fn no_warning_in_ready() {
        let src = "func _ready() -> void:\n\tvar child: Node = $Something\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn no_warning_normal_function() {
        let src = "func setup() -> void:\n\tvar child: Node = $Something\n";
        assert!(check(src).is_empty());
    }
}
