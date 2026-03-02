use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct FloatComparison;

impl LintRule for FloatComparison {
    fn name(&self) -> &'static str {
        "float-comparison"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::BinOp {
                node,
                op,
                left,
                right,
                ..
            } = expr
                && (*op == "==" || *op == "!=")
            {
                let left_is_float = matches!(left.as_ref(), GdExpr::FloatLiteral { .. });
                let right_is_float = matches!(right.as_ref(), GdExpr::FloatLiteral { .. });

                if left_is_float || right_is_float {
                    let left_text = &source[left.node().byte_range()];
                    let right_text = &source[right.node().byte_range()];

                    let replacement = if *op == "==" {
                        format!("is_equal_approx({left_text}, {right_text})")
                    } else {
                        format!("!is_equal_approx({left_text}, {right_text})")
                    };

                    diags.push(LintDiagnostic {
                        rule: "float-comparison",
                        message:
                            "comparing floats with == is unreliable; use is_equal_approx() instead"
                                .to_string(),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: Some(node.end_position().column),
                        fix: Some(Fix {
                            byte_start: node.start_byte(),
                            byte_end: node.end_byte(),
                            replacement,
                        }),
                        context_lines: None,
                    });
                }
            }
        });
        diags
    }
}
