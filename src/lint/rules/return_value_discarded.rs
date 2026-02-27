use crate::core::gd_ast::{self, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{
    InferredType, infer_expression_type, infer_expression_type_with_project,
};
use crate::core::workspace_index::ProjectIndex;

pub struct ReturnValueDiscarded;

impl LintRule for ReturnValueDiscarded {
    fn name(&self) -> &'static str {
        "return-value-discarded"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Suspicious
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_stmts(file, source, symbols, None, &mut diags);
        diags
    }

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_stmts(file, source, symbols, Some(project), &mut diags);
        diags
    }
}

fn check_stmts(
    file: &GdFile,
    source: &str,
    symbols: &SymbolTable,
    project: Option<&ProjectIndex>,
    diags: &mut Vec<LintDiagnostic>,
) {
    gd_ast::visit_stmts(file, &mut |stmt| {
        // Look for expression statements that are function calls with non-void return
        if let GdStmt::Expr { expr, .. } = stmt
            && is_call_expr(expr)
        {
            let expr_node = expr.node();
            let inferred = if let Some(proj) = project {
                infer_expression_type_with_project(&expr_node, source, symbols, proj)
            } else {
                infer_expression_type(&expr_node, source, symbols)
            };

            if matches!(inferred, Some(InferredType::Void) | None) {
                return;
            }

            let call_text = &source[expr_node.byte_range()];
            let display = if call_text.len() > 40 {
                format!("{}...", &call_text[..37])
            } else {
                call_text.to_string()
            };
            diags.push(LintDiagnostic {
                rule: "return-value-discarded",
                message: format!("return value of `{display}` is discarded"),
                severity: Severity::Info,
                line: stmt.line(),
                column: stmt.column(),
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }
    });
}

fn is_call_expr(expr: &GdExpr) -> bool {
    matches!(expr, GdExpr::Call { .. } | GdExpr::MethodCall { .. })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::{parser, symbol_table};

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        ReturnValueDiscarded.check_with_symbols(&file, source, &config, &symbols)
    }

    #[test]
    fn discarded_non_void_call() {
        let source = "\
extends Node
func f():
\tget_child(0)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("discarded"));
    }

    #[test]
    fn void_call_ok() {
        let source = "\
extends Node
func f():
\tadd_child(null)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assigned_call_ok() {
        let source = "\
extends Node
func f():
\tvar child = get_child(0)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn discarded_self_method() {
        let source = "\
func get_value() -> int:
\treturn 42
func f():
\tget_value()
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn self_void_method_ok() {
        let source = "\
func do_thing() -> void:
\tpass
func f():
\tdo_thing()
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!ReturnValueDiscarded.default_enabled());
    }
}
