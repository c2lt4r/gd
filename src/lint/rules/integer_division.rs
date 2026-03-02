use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct IntegerDivision;

impl LintRule for IntegerDivision {
    fn name(&self) -> &'static str {
        "integer-division"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::BinOp {
                node,
                op: "/",
                left,
                right,
                ..
            } = expr
                && let GdExpr::IntLiteral { value: l, .. } = left.as_ref()
                && let GdExpr::IntLiteral { value: r, .. } = right.as_ref()
            {
                diags.push(LintDiagnostic {
                    rule: "integer-division",
                    message: format!(
                        "integer division truncates to integer, use {l}.0 / {r} for float result",
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }
        });
        diags
    }
}
