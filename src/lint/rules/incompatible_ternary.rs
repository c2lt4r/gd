use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::type_inference::{InferredType, infer_expression_type};

pub struct IncompatibleTernary;

impl LintRule for IncompatibleTernary {
    fn name(&self) -> &'static str {
        "incompatible-ternary"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(
        &self,
        _file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::Ternary {
                node,
                true_val,
                false_val,
                ..
            } = expr
            {
                let Some(true_type) = infer_expression_type(&true_val.node(), source, file) else {
                    return;
                };
                let Some(false_type) = infer_expression_type(&false_val.node(), source, file)
                else {
                    return;
                };

                // Skip if either side is Variant (dynamic, can't tell)
                if matches!(true_type, InferredType::Variant)
                    || matches!(false_type, InferredType::Variant)
                {
                    return;
                }

                // Allow int/float mixing (arithmetic promotion)
                if true_type.is_numeric() && false_type.is_numeric() {
                    return;
                }

                if true_type != false_type {
                    diags.push(LintDiagnostic {
                        rule: "incompatible-ternary",
                        message: format!(
                            "ternary branches have incompatible types: `{}` vs `{}`",
                            true_type.display_name(),
                            false_type.display_name()
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
        });
        diags
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
        IncompatibleTernary.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn incompatible_string_int() {
        let source = "func f():\n\tvar x = \"a\" if true else 1\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("String"));
        assert!(diags[0].message.contains("int"));
    }

    #[test]
    fn compatible_same_type() {
        let source = "func f():\n\tvar x = 1 if true else 2\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn compatible_int_float() {
        let source = "func f():\n\tvar x = 1 if true else 2.0\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn incompatible_bool_string() {
        let source = "func f():\n\tvar x = true if true else \"no\"\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn opt_in_rule() {
        assert!(!IncompatibleTernary.default_enabled());
    }
}
