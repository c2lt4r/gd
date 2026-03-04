use gd_core::gd_ast::{self, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct PrivateMethodAccess;

const ALLOWED_CALLBACKS: &[&str] = &["_to_string"];

impl LintRule for PrivateMethodAccess {
    fn name(&self) -> &'static str {
        "private-method-access"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::MethodCall {
                receiver, method, ..
            } = expr
                && method.starts_with('_')
                && !is_self_or_super(receiver)
                && !ALLOWED_CALLBACKS.contains(method)
            {
                diags.push(LintDiagnostic {
                    rule: "private-method-access",
                    message: format!("accessing private method `{method}` on external object"),
                    severity: Severity::Warning,
                    line: expr.line(),
                    column: expr.column(),
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
            }
        });
        diags
    }
}

fn is_self_or_super(expr: &GdExpr) -> bool {
    matches!(
        expr,
        GdExpr::Ident {
            name: "self" | "super",
            ..
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::config::LintConfig;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = gd_core::parser::parse(source).unwrap();
        let file = gd_core::gd_ast::convert(&tree, source);
        PrivateMethodAccess.check(&file, source, &LintConfig::default())
    }

    #[test]
    fn private_method_on_external() {
        let src = "var other: Node = null\nfunc test() -> void:\n\tother._private_method()\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("_private_method"));
    }

    #[test]
    fn self_private_no_warning() {
        let src = "func test() -> void:\n\tself._internal()\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn public_method_no_warning() {
        let src = "var other: Node = null\nfunc test() -> void:\n\tother.public_method()\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn allowed_callback_no_warning() {
        let src = "var obj: Object = null\nfunc test() -> void:\n\tobj._to_string()\n";
        assert!(check(src).is_empty());
    }
}
