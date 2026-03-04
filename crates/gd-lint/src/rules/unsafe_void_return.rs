use gd_core::gd_ast::{self, GdExpr, GdFile, GdStmt, GdVar};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;
use gd_core::type_inference::{
    InferredType, infer_expression_type, infer_expression_type_with_project,
};
use gd_core::workspace_index::ProjectIndex;

pub struct UnsafeVoidReturn;

impl LintRule for UnsafeVoidReturn {
    fn name(&self) -> &'static str {
        "unsafe-void-return"
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
        check_stmts(file, source, None, &mut diags);
        diags
    }

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_stmts(file, source, Some(project), &mut diags);
        diags
    }
}

fn check_stmts(
    file: &GdFile,
    source: &str,
    project: Option<&ProjectIndex>,
    diags: &mut Vec<LintDiagnostic>,
) {
    gd_ast::visit_stmts(file, &mut |stmt| match stmt {
        GdStmt::Return {
            value: Some(expr), ..
        } => {
            check_return_void(stmt, expr, source, file, project, diags);
        }
        GdStmt::Var(var) => {
            check_assign_void(var, source, file, project, diags);
        }
        _ => {}
    });
}

fn check_return_void(
    stmt: &GdStmt,
    expr: &GdExpr,
    source: &str,
    file: &GdFile,
    project: Option<&ProjectIndex>,
    diags: &mut Vec<LintDiagnostic>,
) {
    if !is_call_expr(expr) {
        return;
    }
    let expr_node = expr.node();
    if !is_void_call(&expr_node, source, file, project) {
        return;
    }

    let call_text = &source[expr_node.byte_range()];
    let display = if call_text.len() > 40 {
        format!("{}...", &call_text[..37])
    } else {
        call_text.to_string()
    };

    let stmt_node = stmt.node();
    // Fix: `return void_call()` → `void_call()\n<indent>return`
    let indent = if stmt_node.start_position().column > 0 {
        &source[stmt_node.start_byte() - stmt_node.start_position().column..stmt_node.start_byte()]
    } else {
        "\t"
    };
    let fix = Some(Fix {
        byte_start: stmt_node.start_byte(),
        byte_end: stmt_node.end_byte(),
        replacement: format!("{call_text}\n{indent}return"),
    });

    diags.push(LintDiagnostic {
        rule: "unsafe-void-return",
        message: format!("returning void call `{display}` as a value"),
        severity: Severity::Warning,
        line: stmt.line(),
        column: stmt.column(),
        end_column: None,
        fix,
        context_lines: None,
    });
}

fn check_assign_void(
    var: &GdVar,
    source: &str,
    file: &GdFile,
    project: Option<&ProjectIndex>,
    diags: &mut Vec<LintDiagnostic>,
) {
    let Some(ref value) = var.value else { return };
    if !is_call_expr(value) {
        return;
    }
    let value_node = value.node();
    if !is_void_call(&value_node, source, file, project) {
        return;
    }

    // Fix: `var x = void_call()` → `void_call()`
    let call_text = &source[value_node.byte_range()];
    let fix = if call_text.is_empty() {
        None
    } else {
        Some(Fix {
            byte_start: var.node.start_byte(),
            byte_end: var.node.end_byte(),
            replacement: call_text.to_string(),
        })
    };

    diags.push(LintDiagnostic {
        rule: "unsafe-void-return",
        message: format!("assigning void call result to `{}`", var.name),
        severity: Severity::Warning,
        line: var.node.start_position().row,
        column: var.node.start_position().column,
        end_column: None,
        fix,
        context_lines: None,
    });
}

fn is_call_expr(expr: &GdExpr) -> bool {
    matches!(expr, GdExpr::Call { .. } | GdExpr::MethodCall { .. })
}

fn is_void_call(
    node: &tree_sitter::Node,
    source: &str,
    file: &GdFile,
    project: Option<&ProjectIndex>,
) -> bool {
    let inferred = if let Some(proj) = project {
        infer_expression_type_with_project(node, source, file, proj)
    } else {
        infer_expression_type(node, source, file)
    };
    matches!(inferred, Some(InferredType::Void))
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
        UnsafeVoidReturn.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn return_void_call() {
        let source = "\
extends Node
func f():
\treturn add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("void"));
    }

    #[test]
    fn return_non_void_call() {
        let source = "\
extends Node
func f():
\treturn get_child(0)
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assign_void_call() {
        let source = "\
extends Node
func f():
\tvar x = add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("void"));
    }

    #[test]
    fn return_literal() {
        let source = "\
func f():
\treturn 42
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn return_self_void() {
        let source = "\
func do_thing() -> void:
\tpass
func f():
\treturn do_thing()
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn autofix_return_splits() {
        let source = "\
extends Node
func f():
\treturn add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        assert!(
            fixed.contains("add_child(null)\n\treturn"),
            "fixed was: {fixed}"
        );
    }

    #[test]
    fn autofix_assign_removes_var() {
        let source = "\
extends Node
func f():
\tvar x = add_child(null)
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!(
            "{}{}{}",
            &source[..fix.byte_start],
            fix.replacement,
            &source[fix.byte_end..]
        );
        assert!(fixed.contains("\tadd_child(null)"), "fixed was: {fixed}");
        assert!(!fixed.contains("var x"), "should remove var: {fixed}");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!UnsafeVoidReturn.default_enabled());
    }
}
