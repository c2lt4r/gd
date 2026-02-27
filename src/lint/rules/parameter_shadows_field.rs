use std::collections::HashSet;

use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ParameterShadowsField;

impl LintRule for ParameterShadowsField {
    fn name(&self) -> &'static str {
        "parameter-shadows-field"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_scope(&file.declarations, &mut diags);
        diags
    }
}

fn check_scope(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    // Collect field names at this scope level
    let fields: HashSet<&str> = decls
        .iter()
        .filter_map(|d| {
            if let GdDecl::Var(var) = d {
                Some(var.name)
            } else {
                None
            }
        })
        .collect();

    if !fields.is_empty() {
        // Check functions at this scope level
        for decl in decls {
            if let GdDecl::Func(func) = decl {
                // Skip static functions (no instance context)
                if func.is_static {
                    continue;
                }
                for param in &func.params {
                    if fields.contains(param.name)
                        && !body_uses_self_field(&func.body, param.name)
                    {
                        diags.push(LintDiagnostic {
                            rule: "parameter-shadows-field",
                            message: format!(
                                "parameter `{}` shadows an instance variable",
                                param.name
                            ),
                            severity: Severity::Warning,
                            line: param.node.start_position().row,
                            column: param.node.start_position().column,
                            end_column: Some(param.node.end_position().column),
                            fix: None,
                            context_lines: None,
                        });
                    }
                }
            }
        }
    }

    // Recurse into inner classes (separate scope)
    for decl in decls {
        if let GdDecl::Class(class) = decl {
            check_scope(&class.declarations, diags);
        }
    }
}

/// Check if the function body contains `self.<field_name>` access.
fn body_uses_self_field(body: &[GdStmt], field_name: &str) -> bool {
    for stmt in body {
        if stmts_have_self_field(stmt, field_name) {
            return true;
        }
    }
    false
}

fn stmts_have_self_field(stmt: &GdStmt, field_name: &str) -> bool {
    match stmt {
        GdStmt::Expr { expr, .. } => expr_has_self_field(expr, field_name),
        GdStmt::Var(var) => {
            var.value.as_ref().is_some_and(|v| expr_has_self_field(v, field_name))
        }
        GdStmt::Assign { target, value, .. } | GdStmt::AugAssign { target, value, .. } => {
            expr_has_self_field(target, field_name) || expr_has_self_field(value, field_name)
        }
        GdStmt::Return { value: Some(v), .. } => expr_has_self_field(v, field_name),
        GdStmt::If(gif) => {
            expr_has_self_field(&gif.condition, field_name)
                || gif.body.iter().any(|s| stmts_have_self_field(s, field_name))
                || gif.elif_branches.iter().any(|(c, b)| {
                    expr_has_self_field(c, field_name)
                        || b.iter().any(|s| stmts_have_self_field(s, field_name))
                })
                || gif.else_body.as_ref().is_some_and(|b| {
                    b.iter().any(|s| stmts_have_self_field(s, field_name))
                })
        }
        GdStmt::For { iter, body, .. } => {
            expr_has_self_field(iter, field_name)
                || body.iter().any(|s| stmts_have_self_field(s, field_name))
        }
        GdStmt::While { condition, body, .. } => {
            expr_has_self_field(condition, field_name)
                || body.iter().any(|s| stmts_have_self_field(s, field_name))
        }
        GdStmt::Match { value, arms, .. } => {
            expr_has_self_field(value, field_name)
                || arms.iter().any(|a| a.body.iter().any(|s| stmts_have_self_field(s, field_name)))
        }
        _ => false,
    }
}

fn expr_has_self_field(expr: &GdExpr, field_name: &str) -> bool {
    match expr {
        // self.field_name — the target pattern
        GdExpr::PropertyAccess { receiver, property, .. }
            if matches!(receiver.as_ref(), GdExpr::Ident { name: "self", .. })
                && *property == field_name =>
        {
            true
        }
        // self.field_name() — method call on self with matching name
        GdExpr::MethodCall { receiver, method, .. }
            if matches!(receiver.as_ref(), GdExpr::Ident { name: "self", .. })
                && *method == field_name =>
        {
            true
        }
        // Recurse into sub-expressions
        GdExpr::BinOp { left, right, .. } => {
            expr_has_self_field(left, field_name) || expr_has_self_field(right, field_name)
        }
        GdExpr::UnaryOp { operand, .. } | GdExpr::Cast { expr: operand, .. }
        | GdExpr::Is { expr: operand, .. } | GdExpr::Await { expr: operand, .. } => {
            expr_has_self_field(operand, field_name)
        }
        GdExpr::Call { callee, args, .. } => {
            expr_has_self_field(callee, field_name)
                || args.iter().any(|a| expr_has_self_field(a, field_name))
        }
        GdExpr::MethodCall { receiver, args, .. } => {
            expr_has_self_field(receiver, field_name)
                || args.iter().any(|a| expr_has_self_field(a, field_name))
        }
        GdExpr::PropertyAccess { receiver, .. } => expr_has_self_field(receiver, field_name),
        GdExpr::Subscript { receiver, index, .. } => {
            expr_has_self_field(receiver, field_name) || expr_has_self_field(index, field_name)
        }
        GdExpr::Ternary { true_val, condition, false_val, .. } => {
            expr_has_self_field(true_val, field_name)
                || expr_has_self_field(condition, field_name)
                || expr_has_self_field(false_val, field_name)
        }
        GdExpr::Array { elements, .. } => {
            elements.iter().any(|e| expr_has_self_field(e, field_name))
        }
        GdExpr::Dict { pairs, .. } => {
            pairs.iter().any(|(k, v)| expr_has_self_field(k, field_name) || expr_has_self_field(v, field_name))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;
    use crate::core::gd_ast;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        ParameterShadowsField.check(&file, source, &config)
    }

    #[test]
    fn detects_shadowing() {
        let source =
            "var speed: float = 10.0\n\nfunc set_speed(speed: float) -> void:\n\tspeed = speed\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "parameter-shadows-field");
        assert!(diags[0].message.contains("speed"));
    }

    #[test]
    fn no_warning_different_names() {
        let source = "var speed: float = 10.0\n\nfunc set_speed(new_speed: float) -> void:\n\tspeed = new_speed\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_when_self_used_in_constructor() {
        let source =
            "var health: int\n\nfunc _init(health: int) -> void:\n\tself.health = health\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_in_constructor_without_self() {
        let source = "var health: int\n\nfunc _init(health: int) -> void:\n\thealth = health\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("health"));
    }

    #[test]
    fn no_warning_without_fields() {
        let source = "func f(x: int) -> void:\n\tprint(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_multiple_shadows() {
        let source = "var x: int\nvar y: int\n\nfunc f(x: int, y: int) -> void:\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn inner_class_no_warning_with_self() {
        let source = "class Inner:\n\tvar value: int\n\n\tfunc set_value(value: int) -> void:\n\t\tself.value = value\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn inner_class_warns_without_self() {
        let source = "class Inner:\n\tvar value: int\n\n\tfunc set_value(value: int) -> void:\n\t\tvalue = value\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("value"));
    }

    #[test]
    fn no_cross_class_warning() {
        let source =
            "var speed: float\n\nclass Inner:\n\tfunc f(speed: float) -> void:\n\t\tpass\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_static_factory() {
        let source = "var blocker_id: int\nvar tick: int\n\nstatic func from_box(blocker_id: int, tick: int) -> void:\n\tvar record = DynamicBlockerRecord.new()\n\trecord.blocker_id = blocker_id\n\trecord.tick = tick\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(ParameterShadowsField.default_enabled());
    }
}
