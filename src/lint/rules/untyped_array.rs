use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt, GdVar};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UntypedArray;

impl LintRule for UntypedArray {
    fn name(&self) -> &'static str {
        "untyped-array"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Var(var) = decl {
                check_var(var, &mut diags);
            }
        });
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Var(var) = stmt {
                check_var(var, &mut diags);
            }
        });
        diags
    }
}

fn check_var(var: &GdVar<'_>, diags: &mut Vec<LintDiagnostic>) {
    // Skip constants
    if var.is_const {
        return;
    }
    // Skip if already has a type annotation (explicit or inferred via :=)
    if var.type_ann.is_some() {
        return;
    }
    // Value must be an array literal
    if !matches!(&var.value, Some(GdExpr::Array { .. })) {
        return;
    }

    let col = var.name_node.map_or(var.node.start_position().column, |n| {
        n.start_position().column
    });

    diags.push(LintDiagnostic {
        rule: "untyped-array",
        message: "array variable has no type annotation; consider `Array[Type]`".to_string(),
        severity: Severity::Warning,
        line: var.node.start_position().row,
        column: col,
        fix: None,
        end_column: None,
        context_lines: None,
    });
}
