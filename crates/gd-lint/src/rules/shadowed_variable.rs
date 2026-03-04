use std::collections::HashSet;

use gd_core::gd_ast::{GdDecl, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct ShadowedVariable;

impl LintRule for ShadowedVariable {
    fn name(&self) -> &'static str {
        "shadowed-variable"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        collect_functions(&file.declarations, &mut diags);
        diags
    }
}

fn collect_functions(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl {
            let outer: HashSet<&str> = func.params.iter().map(|p| p.name).collect();
            check_body(&func.body, &outer, diags);
        }
        if let GdDecl::Class(class) = decl {
            collect_functions(&class.declarations, diags);
        }
    }
}

/// Check a statement list for variable declarations that shadow outer scope names.
fn check_body<'a>(stmts: &[GdStmt<'a>], outer: &HashSet<&'a str>, diags: &mut Vec<LintDiagnostic>) {
    let mut current_scope = outer.clone();

    for stmt in stmts {
        if let GdStmt::Var(var) = stmt {
            if outer.contains(var.name) {
                let name_node = var.name_node.unwrap_or(var.node);
                diags.push(LintDiagnostic {
                    rule: "shadowed-variable",
                    message: format!(
                        "variable `{}` shadows a variable from an outer scope",
                        var.name,
                    ),
                    severity: Severity::Warning,
                    line: name_node.start_position().row,
                    column: name_node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }
            current_scope.insert(var.name);
        }

        // Recurse into inner scopes
        check_inner_scopes(stmt, &current_scope, diags);
    }
}

fn check_inner_scopes<'a>(
    stmt: &GdStmt<'a>,
    outer: &HashSet<&'a str>,
    diags: &mut Vec<LintDiagnostic>,
) {
    match stmt {
        GdStmt::If(gif) => {
            check_body(&gif.body, outer, diags);
            for (_, branch_body) in &gif.elif_branches {
                check_body(branch_body, outer, diags);
            }
            if let Some(else_body) = &gif.else_body {
                check_body(else_body, outer, diags);
            }
        }
        GdStmt::For { var, body, .. } => {
            let mut for_outer = outer.clone();
            for_outer.insert(var);
            check_body(body, &for_outer, diags);
        }
        GdStmt::While { body, .. } => {
            check_body(body, outer, diags);
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                check_body(&arm.body, outer, diags);
            }
        }
        _ => {}
    }
}
