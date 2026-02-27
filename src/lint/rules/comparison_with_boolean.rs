use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ComparisonWithBoolean;

impl LintRule for ComparisonWithBoolean {
    fn name(&self) -> &'static str {
        "comparison-with-boolean"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            let GdExpr::BinOp { node, op, left, right, .. } = expr else { return };
            if *op != "==" && *op != "!=" {
                return;
            }

            let left_is_bool = matches!(left.as_ref(), GdExpr::Bool { .. });
            let right_is_bool = matches!(right.as_ref(), GdExpr::Bool { .. });

            if !left_is_bool && !right_is_bool {
                return;
            }

            let suggestion = if *op == "==" {
                "use the value directly (e.g. `if x:` instead of `if x == true:`)"
            } else {
                "use `not` (e.g. `if not x:` instead of `if x != true:`)"
            };

            let (bool_expr, other_expr) = if left_is_bool {
                (left.as_ref(), right.as_ref())
            } else {
                (right.as_ref(), left.as_ref())
            };

            let bool_text = &source[bool_expr.node().byte_range()];
            let other_text = &source[other_expr.node().byte_range()];

            let fix = match (*op, bool_text) {
                ("==", "true") | ("!=", "false") => Some(Fix {
                    byte_start: node.start_byte(),
                    byte_end: node.end_byte(),
                    replacement: other_text.to_string(),
                }),
                ("==", "false") | ("!=", "true") => Some(Fix {
                    byte_start: node.start_byte(),
                    byte_end: node.end_byte(),
                    replacement: format!("not {other_text}"),
                }),
                _ => None,
            };

            diags.push(LintDiagnostic {
                rule: "comparison-with-boolean",
                message: format!(
                    "comparison `{}` with boolean literal is redundant; {}",
                    &source[node.byte_range()],
                    suggestion,
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: Some(node.end_position().column),
                fix,
                context_lines: None,
            });
        });
        diags
    }
}
