use gd_core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};
use std::collections::HashSet;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct CallableNullCheck;

impl LintRule for CallableNullCheck {
    fn name(&self) -> &'static str {
        "callable-null-check"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                check_function_body(&func.body, &mut diags);
            }
        });
        diags
    }
}

fn check_function_body(body: &[GdStmt], diags: &mut Vec<LintDiagnostic>) {
    // Pass 1: collect identifiers with .is_valid()/.is_null()/null checks/truthiness guards
    let mut validated: HashSet<&str> = HashSet::new();
    gd_ast::visit_body_exprs(body, &mut |expr| {
        collect_validated(expr, &mut validated);
    });
    // Also check if-statement conditions for truthiness guards
    gd_ast::visit_body_stmts(body, &mut |stmt| {
        if let GdStmt::If(if_stmt) = stmt
            && let GdExpr::Ident { name, .. } = &if_stmt.condition
        {
            validated.insert(name);
        }
    });

    // Pass 2: find .call()/.call_deferred()/.callv() on unvalidated identifiers
    gd_ast::visit_body_exprs(body, &mut |expr| {
        check_callable_call(expr, &validated, diags);
    });
}

/// Collect identifiers that have been validated via .is_valid()/.is_null() or null comparison.
fn collect_validated<'a>(expr: &GdExpr<'a>, validated: &mut HashSet<&'a str>) {
    match expr {
        // foo.is_valid() or foo.is_null()
        GdExpr::MethodCall {
            receiver, method, ..
        } if matches!(*method, "is_valid" | "is_null") => {
            if let Some(name) = terminal_name(receiver) {
                validated.insert(name);
            }
        }
        // foo != null / foo == null / null != foo / null == foo
        GdExpr::BinOp {
            left, op, right, ..
        } if matches!(*op, "!=" | "==") => {
            if matches!(right.as_ref(), GdExpr::Null { .. })
                && let GdExpr::Ident { name, .. } = left.as_ref()
            {
                validated.insert(name);
            } else if matches!(left.as_ref(), GdExpr::Null { .. })
                && let GdExpr::Ident { name, .. } = right.as_ref()
            {
                validated.insert(name);
            }
        }
        _ => {}
    }
}

/// Check for .call()/.call_deferred()/.callv() on unvalidated callable identifiers.
fn check_callable_call(expr: &GdExpr, validated: &HashSet<&str>, diags: &mut Vec<LintDiagnostic>) {
    let GdExpr::MethodCall {
        receiver,
        method,
        args,
        ..
    } = expr
    else {
        return;
    };
    if !matches!(*method, "call" | "call_deferred" | "callv") {
        return;
    }

    // `obj.call_deferred("method_name")` is Object.call_deferred, not Callable
    if *method == "call_deferred" && matches!(args.first(), Some(GdExpr::StringLiteral { .. })) {
        return;
    }

    let Some(obj_name) = terminal_name(receiver) else {
        return;
    };
    if obj_name == "self" || validated.contains(obj_name) {
        return;
    }

    diags.push(LintDiagnostic {
        rule: "callable-null-check",
        message: format!("`{obj_name}.{method}()` called without `{obj_name}.is_valid()` check"),
        severity: Severity::Warning,
        line: expr.line(),
        column: expr.column(),
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

/// Get the terminal identifier name from an expression chain.
/// `Ident("foo")` → "foo", `PropertyAccess { property: "bar" }` → "bar"
/// Also handles tree-sitter precedence quirk where `x and y.prop` parses as
/// `BinOp { left: x, right: PropertyAccess { prop } }` inside an attribute chain.
fn terminal_name<'a>(expr: &GdExpr<'a>) -> Option<&'a str> {
    match expr {
        GdExpr::Ident { name, .. } => Some(name),
        GdExpr::PropertyAccess { property, .. } => Some(property),
        GdExpr::BinOp { right, .. } => terminal_name(right),
        _ => None,
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
        CallableNullCheck.check(&file, source, &config)
    }

    #[test]
    fn detects_call_without_check() {
        let source = "func f(callback: Callable) -> void:\n\tcallback.call()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "callable-null-check");
        assert!(diags[0].message.contains("callback.call()"));
        assert!(diags[0].message.contains("is_valid"));
    }

    #[test]
    fn no_warning_with_is_valid() {
        let source = "func f(callback) -> void:\n\tif callback.is_valid():\n\t\tcallback.call()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_with_null_check() {
        let source = "func f(callback) -> void:\n\tif callback != null:\n\t\tcallback.call()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_with_truthiness_check() {
        let source = "func f(callback) -> void:\n\tif callback:\n\t\tcallback.call()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_call_deferred_on_callable() {
        let source = "func f(cb) -> void:\n\tcb.call_deferred()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("call_deferred"));
    }

    #[test]
    fn no_warning_on_self_call() {
        let source = "func f() -> void:\n\tself.call(\"method\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_callv() {
        let source = "func f(cb: Callable) -> void:\n\tcb.callv([])\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_without_callable_call() {
        let source = "func f(node: Node) -> void:\n\tnode.process()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn chained_is_valid_guards_chained_call() {
        let source = "func f(server) -> void:\n\tif server and server.hitscan_validator.is_valid():\n\t\tserver.hitscan_validator.call(1, 2)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn chained_call_without_is_valid_warns() {
        let source = "func f(server) -> void:\n\tserver.hitscan_validator.call(1, 2)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("hitscan_validator"));
    }

    #[test]
    fn default_enabled() {
        assert!(CallableNullCheck.default_enabled());
    }

    // ── call_deferred with string arg (Object method, not Callable) ──

    #[test]
    fn no_warning_call_deferred_string_arg() {
        let source = "func f(node) -> void:\n\tnode.call_deferred(\"method_name\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_call_deferred_string_arg_extra_args() {
        let source = "func f(node) -> void:\n\tnode.call_deferred(\"method_name\", 1, \"hello\")\n";
        assert!(check(source).is_empty());
    }
}
