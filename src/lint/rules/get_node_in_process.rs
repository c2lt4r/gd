use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct GetNodeInProcess;

impl LintRule for GetNodeInProcess {
    fn name(&self) -> &'static str {
        "get-node-in-process"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Performance
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        find_process_functions(&file.declarations, source, &mut diags);
        diags
    }
}

fn find_process_functions(decls: &[GdDecl<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl
            && matches!(func.name, "_process" | "_physics_process")
        {
            find_get_node_in_stmts(&func.body, source, func.name, diags);
        }
        if let GdDecl::Class(class) = decl {
            find_process_functions(&class.declarations, source, diags);
        }
    }
}

fn find_get_node_in_stmts(
    stmts: &[GdStmt<'_>],
    source: &str,
    func_name: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    for stmt in stmts {
        visit_stmt_exprs(stmt, &mut |expr| {
            check_get_node_expr(expr, source, func_name, diags);
        });

        // Recurse into nested bodies
        match stmt {
            GdStmt::If(gif) => {
                find_get_node_in_stmts(&gif.body, source, func_name, diags);
                for (_, branch_body) in &gif.elif_branches {
                    find_get_node_in_stmts(branch_body, source, func_name, diags);
                }
                if let Some(else_body) = &gif.else_body {
                    find_get_node_in_stmts(else_body, source, func_name, diags);
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                find_get_node_in_stmts(body, source, func_name, diags);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    find_get_node_in_stmts(&arm.body, source, func_name, diags);
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
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            visit_expr(target, f);
            visit_expr(value, f);
        }
        GdStmt::Return {
            value: Some(val), ..
        } => visit_expr(val, f),
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

fn check_get_node_expr(
    expr: &GdExpr<'_>,
    _source: &str,
    func_name: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    // $NodePath syntax
    if let GdExpr::GetNode { node, path } = expr {
        diags.push(LintDiagnostic {
            rule: "get-node-in-process",
            message: format!(
                "`{path}` in {func_name}() is called every frame; cache it in an @onready var"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: Some(node.end_position().column),
            fix: None,
            context_lines: None,
        });
    }

    // Bare call: get_node("path") or get_node_or_null("path")
    if let GdExpr::Call { node, callee, .. } = expr
        && let GdExpr::Ident { name, .. } = callee.as_ref()
        && matches!(*name, "get_node" | "get_node_or_null")
    {
        diags.push(LintDiagnostic {
            rule: "get-node-in-process",
            message: format!(
                "`{name}()` in {func_name}() is called every frame; cache it in an @onready var"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }

    // Method call: self.get_node("path") or obj.get_node("path")
    if let GdExpr::MethodCall { node, method, .. } = expr
        && matches!(*method, "get_node" | "get_node_or_null")
    {
        diags.push(LintDiagnostic {
            rule: "get-node-in-process",
            message: format!(
                "`{method}()` in {func_name}() is called every frame; cache it in an @onready var"
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
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        GetNodeInProcess.check(&file, source, &config)
    }

    #[test]
    fn detects_dollar_node_path_in_process() {
        let source = "func _process(delta: float) -> void:\n\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "get-node-in-process");
        assert!(diags[0].message.contains("$Sprite2D"));
        assert!(diags[0].message.contains("_process()"));
    }

    #[test]
    fn detects_get_node_call_in_process() {
        let source = "func _process(delta: float) -> void:\n\tvar n := get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node()"));
    }

    #[test]
    fn detects_get_node_or_null_in_process() {
        let source =
            "func _process(delta: float) -> void:\n\tvar n := get_node_or_null(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node_or_null()"));
    }

    #[test]
    fn detects_in_physics_process() {
        let source = "func _physics_process(delta: float) -> void:\n\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_physics_process()"));
    }

    #[test]
    fn detects_self_get_node_in_process() {
        let source =
            "func _process(delta: float) -> void:\n\tvar n := self.get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node()"));
    }

    #[test]
    fn no_warning_in_ready() {
        let source = "func _ready() -> void:\n\tvar sprite := $Sprite2D\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_in_regular_function() {
        let source = "func setup() -> void:\n\tvar n := get_node(\"Sprite2D\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_in_same_function() {
        let source = "func _process(delta: float) -> void:\n\tvar a := $Sprite2D\n\tvar b := get_node(\"Label\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn no_warning_for_nested_function() {
        let source = "func _process(delta: float) -> void:\n\tpass\n\nfunc helper() -> void:\n\tvar n := get_node(\"Sprite2D\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_in_inner_class() {
        let source =
            "class Inner:\n\tfunc _process(delta: float) -> void:\n\t\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_nested_dollar_in_conditional() {
        let source =
            "func _process(delta: float) -> void:\n\tif true:\n\t\tvar sprite := $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
