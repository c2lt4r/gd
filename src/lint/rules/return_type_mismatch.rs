use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ReturnTypeMismatch;

impl LintRule for ReturnTypeMismatch {
    fn name(&self) -> &'static str {
        "return-type-mismatch"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_decls(&file.declarations, &mut diags);
        diags
    }
}

fn check_decls(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl {
            let Some(ret_type) = &func.return_type else {
                continue;
            };
            let is_void = ret_type.name == "void";
            check_returns(&func.body, is_void, diags);
        }
        if let GdDecl::Class(class) = decl {
            check_decls(&class.declarations, diags);
        }
    }
}

fn check_returns(stmts: &[GdStmt<'_>], is_void: bool, diags: &mut Vec<LintDiagnostic>) {
    for stmt in stmts {
        if let GdStmt::Return { node, value } = stmt {
            if is_void && value.is_some() {
                // Allow `return <call>()` in void functions — Godot permits this
                // pattern for side-effect calls (e.g. `return print("x")`)
                let is_call = value.as_ref().is_some_and(|v| {
                    matches!(
                        v,
                        GdExpr::Call { .. } | GdExpr::MethodCall { .. } | GdExpr::SuperCall { .. }
                    )
                });
                if !is_call {
                    diags.push(LintDiagnostic {
                        rule: "return-type-mismatch",
                        message: "function declares -> void but returns a value".to_string(),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        fix: None,
                        end_column: None,
                        context_lines: None,
                    });
                }
            } else if !is_void && value.is_none() {
                diags.push(LintDiagnostic {
                    rule: "return-type-mismatch",
                    message: "function declares return type but has bare return".to_string(),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }
        }

        // Recurse into nested blocks
        match stmt {
            GdStmt::If(gif) => {
                check_returns(&gif.body, is_void, diags);
                for (_, branch) in &gif.elif_branches {
                    check_returns(branch, is_void, diags);
                }
                if let Some(else_body) = &gif.else_body {
                    check_returns(else_body, is_void, diags);
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                check_returns(body, is_void, diags);
            }
            GdStmt::Match { arms, .. } => {
                for arm in arms {
                    check_returns(&arm.body, is_void, diags);
                }
            }
            _ => {}
        }
    }
}
