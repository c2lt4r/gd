use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PrintStatement;

impl LintRule for PrintStatement {
    fn name(&self) -> &'static str {
        "print-statement"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

/// Debug print function names to detect.
const PRINT_FUNCTIONS: &[&str] = &[
    "print",
    "prints",
    "printt",
    "printraw",
    "print_debug",
    "print_rich",
    "print_verbose",
    "push_error",
    "push_warning",
];

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "call" {
        // Try field name first, fall back to finding first identifier child
        let func_name_opt = node
            .child_by_field_name("function")
            .or_else(|| {
                node.children(&mut node.walk())
                    .find(|c| c.kind() == "identifier")
            });
        if let Some(func_node) = func_name_opt {
            let func_name = &source[func_node.byte_range()];
            if PRINT_FUNCTIONS.contains(&func_name) {
                diags.push(LintDiagnostic {
                    rule: "print-statement",
                    message: format!(
                        "found `{}()` call; consider removing before release",
                        func_name
                    ),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(func_node.end_position().column),
                    fix: None,
                });
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        PrintStatement.check(&tree, source, &config)
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
    fn detects_push_error() {
        let source = "func foo():\n\tpush_error(\"error\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("push_error()"));
    }

    #[test]
    fn detects_push_warning() {
        let source = "func foo():\n\tpush_warning(\"warning\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("push_warning()"));
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
\tpush_error(\"b\")
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
    fn is_opt_in() {
        assert!(!PrintStatement.default_enabled());
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
