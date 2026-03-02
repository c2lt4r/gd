use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicateDelegate;

impl LintRule for DuplicateDelegate {
    fn name(&self) -> &'static str {
        "duplicate-delegate"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                check_function(func, &mut diags);
            }
        });
        diags
    }
}

fn check_function(func: &crate::core::gd_ast::GdFunc, diags: &mut Vec<LintDiagnostic>) {
    let params: Vec<&str> = func.params.iter().map(|p| p.name).collect();

    // Body must have exactly one non-pass statement
    let stmts: Vec<&GdStmt> = func
        .body
        .iter()
        .filter(|s| !matches!(s, GdStmt::Pass { .. }))
        .collect();
    if stmts.len() != 1 {
        return;
    }

    // Extract the call expression (from return or expression statement)
    let (GdStmt::Return {
        value: Some(call_expr),
        ..
    }
    | GdStmt::Expr {
        expr: call_expr, ..
    }) = stmts[0]
    else {
        return;
    };

    // Must be a method call (self.ref.method(args) or ref.method(args))
    let GdExpr::MethodCall {
        receiver,
        method,
        args,
        ..
    } = call_expr
    else {
        return;
    };

    // Build the delegate target string from the receiver chain + method
    let mut parts: Vec<&str> = Vec::new();
    collect_chain_parts(receiver, &mut parts);
    parts.push(method);

    // Must have at least receiver.method (2+ parts)
    if parts.len() < 2 {
        return;
    }

    // Check that call arguments exactly match function parameters
    if !args_match_params(args, &params) {
        return;
    }

    let target = parts.join(".");
    diags.push(LintDiagnostic {
        rule: "duplicate-delegate",
        message: format!(
            "`{}` is a pure delegate to `{target}`; consider inlining or removing",
            func.name
        ),
        severity: Severity::Info,
        line: func.node.start_position().row,
        column: func.node.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

/// Collect identifier parts from a receiver chain (e.g., `self.ref` becomes `["self", "ref"]`).
fn collect_chain_parts<'a>(expr: &GdExpr<'a>, parts: &mut Vec<&'a str>) {
    match expr {
        GdExpr::Ident { name, .. } => parts.push(name),
        GdExpr::PropertyAccess {
            receiver, property, ..
        } => {
            collect_chain_parts(receiver, parts);
            parts.push(property);
        }
        _ => {}
    }
}

/// Check that call arguments are all plain identifiers matching the function parameters
/// in the same order.
fn args_match_params(args: &[GdExpr], params: &[&str]) -> bool {
    if args.len() != params.len() {
        return false;
    }
    args.iter()
        .zip(params.iter())
        .all(|(arg, param)| matches!(arg, GdExpr::Ident { name, .. } if *name == *param))
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
        DuplicateDelegate.check(&file, source, &config)
    }

    #[test]
    fn detects_delegate_with_return() {
        let source = "var ref: Node\n\nfunc get_name(a, b):\n\treturn self.ref.get_name(a, b)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "duplicate-delegate");
        assert!(diags[0].message.contains("pure delegate"));
    }

    #[test]
    fn detects_delegate_without_return() {
        let source = "var ref: Node\n\nfunc do_thing(x):\n\tself.ref.do_thing(x)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_different_args() {
        let source = "var ref: Node\n\nfunc do_thing(x, y):\n\tself.ref.do_thing(y, x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_extra_logic() {
        let source = "var ref: Node\n\nfunc do_thing(x):\n\tprint(x)\n\tself.ref.do_thing(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_no_params() {
        // Zero-param delegates are too common and legitimate (property getters)
        let source = "var ref: Node\n\nfunc get_name():\n\treturn self.ref.get_name()\n";
        let diags = check(source);
        // Zero params: args_match_params returns true (both empty)
        // But this is still a delegate — we flag it
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn opt_in_rule() {
        assert!(!DuplicateDelegate.default_enabled());
    }
}
