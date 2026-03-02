use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PrintStatement;

impl LintRule for PrintStatement {
    fn name(&self) -> &'static str {
        "print-statement"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::Call { node, callee, .. } = expr
                && let GdExpr::Ident { name, .. } = callee.as_ref()
                && PRINT_FUNCTIONS.contains(name)
            {
                diags.push(LintDiagnostic {
                    rule: "print-statement",
                    message: format!("found `{name}()` call; consider removing before release"),
                    severity: Severity::Info,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(callee.node().end_position().column),
                    fix: None,
                    context_lines: None,
                });
            }
        });
        diags
    }
}

/// Debug print function names to detect.
/// Note: push_error() and push_warning() are intentionally excluded — they are
/// Godot's structured logging (appear in debugger with stack traces) and belong
/// in production code for error conditions and graceful degradation.
const PRINT_FUNCTIONS: &[&str] = &[
    "print",
    "prints",
    "printt",
    "printraw",
    "print_debug",
    "print_rich",
    "print_verbose",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        PrintStatement.check(&file, source, &config)
    }

    // ── Detects print functions ───────────────────────────────────────

    #[test]
    fn detects_print() {
        let source = "func foo():\n\tprint(\"hello\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("print()"));
    }

    #[test]
    fn detects_prints() {
        let source = "func foo():\n\tprints(\"a\", \"b\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("prints()"));
    }

    #[test]
    fn detects_printt() {
        let source = "func foo():\n\tprintt(\"a\", \"b\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("printt()"));
    }

    #[test]
    fn detects_printraw() {
        let source = "func foo():\n\tprintraw(\"raw\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("printraw()"));
    }

    #[test]
    fn detects_print_debug() {
        let source = "func foo():\n\tprint_debug(\"debug\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("print_debug()"));
    }

    #[test]
    fn detects_print_rich() {
        let source = "func foo():\n\tprint_rich(\"[b]bold[/b]\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("print_rich()"));
    }

    #[test]
    fn detects_print_verbose() {
        let source = "func foo():\n\tprint_verbose(\"verbose\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("print_verbose()"));
    }

    #[test]
    fn no_warning_push_error() {
        let source = "func foo():\n\tpush_error(\"error\")\n";
        assert!(
            check(source).is_empty(),
            "push_error is structured logging, not debug print"
        );
    }

    #[test]
    fn no_warning_push_warning() {
        let source = "func foo():\n\tpush_warning(\"warning\")\n";
        assert!(
            check(source).is_empty(),
            "push_warning is structured logging, not debug print"
        );
    }

    // ── No false positives ────────────────────────────────────────────

    #[test]
    fn no_warning_for_other_calls() {
        let source = "func foo():\n\tmy_function(\"hello\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_method_calls() {
        // self.print() should NOT trigger since the function name would be "print"
        // accessed through attribute, not a bare call
        let source = "func foo():\n\tlogger.print(\"hello\")\n";
        // The function node for method calls is typically the full attribute expression
        // "logger.print" which won't match our plain function names
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_variable_named_print() {
        let source = "var print = \"test\"\n";
        assert!(check(source).is_empty());
    }

    // ── Multiple calls ────────────────────────────────────────────────

    #[test]
    fn detects_multiple_prints() {
        let source = "\
func foo():
\tprint(\"first\")
\tprint(\"second\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn detects_different_print_types() {
        let source = "\
func foo():
\tprint(\"a\")
\tprints(\"b\", \"c\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    // ── Nested contexts ───────────────────────────────────────────────

    #[test]
    fn detects_in_conditional() {
        let source = "func foo():\n\tif true:\n\t\tprint(\"debug\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_in_loop() {
        let source = "func foo():\n\tfor i in range(10):\n\t\tprint(i)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_in_inner_class() {
        let source = "class Inner:\n\tfunc foo():\n\t\tprint(\"inner\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Top-level call ────────────────────────────────────────────────

    #[test]
    fn detects_top_level_print() {
        let source = "print(\"top level\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Opt-in ────────────────────────────────────────────────────────

    #[test]
    fn is_default_enabled() {
        assert!(PrintStatement.default_enabled());
    }

    #[test]
    fn severity_is_info() {
        let source = "func foo():\n\tprint(\"hello\")\n";
        let diags = check(source);
        assert_eq!(diags[0].severity, Severity::Info);
    }

    // ── Span correctness ──────────────────────────────────────────────

    #[test]
    fn diagnostic_points_to_call() {
        let source = "func foo():\n\tprint(\"hello\")\n";
        let diags = check(source);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].column, 1); // after tab
    }
}
