use gd_core::gd_ast::{self, GdExpr, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct AssertAlwaysFalse;

impl LintRule for AssertAlwaysFalse {
    fn name(&self) -> &'static str {
        "assert-always-false"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::Call {
                node, callee, args, ..
            } = expr
                && let GdExpr::Ident { name: "assert", .. } = callee.as_ref()
                && let Some(first_arg) = args.first()
                && is_always_falsy(first_arg, source)
            {
                let arg_text = &source[first_arg.node().byte_range()];
                let fix = node.parent().map(|stmt| {
                    let source_bytes = source.as_bytes();
                    let mut start = stmt.start_byte();
                    let mut end = stmt.end_byte();
                    while start > 0 && source_bytes[start - 1] == b'\t' {
                        start -= 1;
                    }
                    if end < source.len() && source_bytes[end] == b'\n' {
                        end += 1;
                    }
                    Fix {
                        byte_start: start,
                        byte_end: end,
                        replacement: String::new(),
                    }
                });
                diags.push(LintDiagnostic {
                    rule: "assert-always-false",
                    message: format!("assertion is always false: `assert({arg_text})`"),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: None,
                    fix,
                    context_lines: None,
                });
            }
        });
        diags
    }
}

fn is_always_falsy(expr: &GdExpr<'_>, source: &str) -> bool {
    match expr {
        GdExpr::Bool { value: false, .. } | GdExpr::Null { .. } => true,
        GdExpr::IntLiteral { value, .. } => *value == "0",
        GdExpr::FloatLiteral { value, .. } => *value == "0.0" || *value == "0." || *value == ".0",
        GdExpr::StringLiteral { .. } => {
            let text = &source[expr.node().byte_range()];
            text == "\"\"" || text == "''"
        }
        _ => false,
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
        AssertAlwaysFalse.check(&file, source, &config)
    }

    #[test]
    fn assert_false() {
        let source = "func f():\n\tassert(false)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("always false"));
    }

    #[test]
    fn assert_null() {
        let source = "func f():\n\tassert(null)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_zero() {
        let source = "func f():\n\tassert(0)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_empty_string() {
        let source = "func f():\n\tassert(\"\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_true_ok() {
        let source = "func f():\n\tassert(true)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assert_variable_ok() {
        let source = "func f():\n\tassert(x)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assert_call_ok() {
        let source = "func f():\n\tassert(is_valid())\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_removes_line() {
        let source = "func f():\n\tassert(false)\n\tprint(\"hi\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!("{}{}", &source[..fix.byte_start], &source[fix.byte_end..]);
        assert_eq!(fixed, "func f():\n\tprint(\"hi\")\n");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!AssertAlwaysFalse.default_enabled());
    }
}
