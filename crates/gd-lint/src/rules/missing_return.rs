use gd_core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct MissingReturn;

impl LintRule for MissingReturn {
    fn name(&self) -> &'static str {
        "missing-return"
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
            // Skip abstract functions — they have no body to return from
            if func.annotations.iter().any(|a| a.name == "abstract") {
                continue;
            }
            // Must have a non-void return type
            if let Some(ret_type) = &func.return_type
                && ret_type.name != "void"
                && !body_always_returns(&func.body)
            {
                diags.push(LintDiagnostic {
                    rule: "missing-return",
                    message: format!(
                        "function `{}` has a return type but may not return a value",
                        func.name
                    ),
                    severity: Severity::Warning,
                    line: func.node.start_position().row,
                    column: func.node.start_position().column,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            }
        }
        if let GdDecl::Class(class) = decl {
            check_decls(&class.declarations, diags);
        }
    }
}

/// Check if a statement list always returns on every code path.
fn body_always_returns(stmts: &[GdStmt<'_>]) -> bool {
    let Some(last) = stmts.last() else {
        return false;
    };
    stmt_always_returns(last)
}

/// Check if a statement always returns.
fn stmt_always_returns(stmt: &GdStmt<'_>) -> bool {
    match stmt {
        GdStmt::Return { .. } => true,
        GdStmt::If(gif) => {
            // If body must return
            if !body_always_returns(&gif.body) {
                return false;
            }
            // All elif branches must return
            for (_, branch_body) in &gif.elif_branches {
                if !body_always_returns(branch_body) {
                    return false;
                }
            }
            // Must have an else that returns (otherwise can fall through)
            gif.else_body
                .as_ref()
                .is_some_and(|else_body| body_always_returns(else_body))
        }
        GdStmt::Match { arms, .. } => {
            // Every arm must return
            if arms.iter().any(|arm| !body_always_returns(&arm.body)) {
                return false;
            }
            // Must have a wildcard arm (otherwise can fall through)
            arms.iter().any(|arm| {
                arm.patterns
                    .iter()
                    .any(|pat| matches!(pat, GdExpr::Ident { name: "_", .. }))
            })
        }
        _ => false,
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
        MissingReturn.check(&file, source, &config)
    }

    #[test]
    fn no_warning_on_match_with_all_returns() {
        let source = "func f(x: int) -> String:\n\tmatch x:\n\t\t0:\n\t\t\treturn \"a\"\n\t\t1:\n\t\t\treturn \"b\"\n\t\t_:\n\t\t\treturn \"c\"\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_if_elif_else_all_return() {
        let source = "func f(x: int) -> int:\n\tif x > 10:\n\t\treturn 1\n\telif x > 5:\n\t\treturn 2\n\telse:\n\t\treturn 3\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_on_match_without_wildcard() {
        let source = "func f(x: int) -> String:\n\tmatch x:\n\t\t0:\n\t\t\treturn \"a\"\n\t\t1:\n\t\t\treturn \"b\"\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_if_without_else() {
        let source = "func f(x: int) -> int:\n\tif x > 10:\n\t\treturn 1\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn warns_on_empty_body() {
        let source = "func f() -> int:\n\tpass\n";
        assert_eq!(check(source).len(), 1);
    }

    #[test]
    fn no_warning_on_direct_return() {
        let source = "func f() -> int:\n\treturn 42\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_on_void() {
        let source = "func f() -> void:\n\tpass\n";
        assert!(check(source).is_empty());
    }
}
