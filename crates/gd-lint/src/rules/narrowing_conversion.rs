use gd_core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt, GdVar};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;
use gd_core::type_inference::{InferredType, infer_expression_type};

pub struct NarrowingConversion;

impl LintRule for NarrowingConversion {
    fn name(&self) -> &'static str {
        "narrowing-conversion"
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
        // Check top-level variable declarations (GdDecl::Var)
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Var(var) = decl {
                check_var_decl(var, source, file, &mut diags);
            }
        });
        // Check in-function variable declarations and assignments (GdStmt)
        gd_ast::visit_stmts(file, &mut |stmt| {
            check_stmt(stmt, source, file, &mut diags);
        });
        diags
    }
}

fn check_var_decl(var: &GdVar<'_>, source: &str, file: &GdFile, diags: &mut Vec<LintDiagnostic>) {
    if let Some(type_ann) = &var.type_ann
        && type_ann.name == "int"
        && let Some(value) = &var.value
        && matches!(
            infer_expression_type(&value.node(), source, file),
            Some(InferredType::Builtin("float"))
        )
    {
        let value_text = &source[value.node().byte_range()];
        diags.push(LintDiagnostic {
            rule: "narrowing-conversion",
            message: format!(
                "narrowing conversion: float value assigned to `{}: int`",
                var.name
            ),
            severity: Severity::Warning,
            line: var.node.start_position().row,
            column: var.node.start_position().column,
            end_column: None,
            fix: Some(Fix {
                byte_start: value.node().start_byte(),
                byte_end: value.node().end_byte(),
                replacement: format!("int({value_text})"),
            }),
            context_lines: None,
        });
    }
}

fn check_stmt(stmt: &GdStmt<'_>, source: &str, file: &GdFile, diags: &mut Vec<LintDiagnostic>) {
    // Variable declarations inside functions
    if let GdStmt::Var(var) = stmt {
        check_var_decl(var, source, file, diags);
    }

    // Assignments to int-typed variables
    if let GdStmt::Assign {
        node,
        target,
        value,
        ..
    } = stmt
        && let GdExpr::Ident { name: var_name, .. } = target
    {
        let is_int_var = file
            .vars()
            .any(|v| v.name == *var_name && v.type_ann.as_ref().is_some_and(|t| t.name == "int"));
        if is_int_var
            && matches!(
                infer_expression_type(&value.node(), source, file),
                Some(InferredType::Builtin("float"))
            )
        {
            let value_text = &source[value.node().byte_range()];
            diags.push(LintDiagnostic {
                rule: "narrowing-conversion",
                message: format!("narrowing conversion: float value assigned to `{var_name}: int`"),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                end_column: None,
                fix: Some(Fix {
                    byte_start: value.node().start_byte(),
                    byte_end: value.node().end_byte(),
                    replacement: format!("int({value_text})"),
                }),
                context_lines: None,
            });
        }
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
        NarrowingConversion.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn float_literal_to_int_var() {
        let source = "var x: int = 3.14\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("narrowing"));
    }

    #[test]
    fn float_expr_to_int_var() {
        let source = "var x: int = 1.0 + 2.0\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn int_literal_to_int_var() {
        let source = "var x: int = 42\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_type_annotation() {
        let source = "var x = 3.14\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn float_type_var() {
        let source = "var x: float = 3.14\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_wraps_with_int() {
        let source = "var x: int = 3.14\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        assert!(fixed.contains("int(3.14)"), "fixed was: {fixed}");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!NarrowingConversion.default_enabled());
    }
}
