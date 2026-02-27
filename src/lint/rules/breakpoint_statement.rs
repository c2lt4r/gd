use crate::core::gd_ast::{self, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct BreakpointStatement;

impl LintRule for BreakpointStatement {
    fn name(&self) -> &'static str {
        "breakpoint-statement"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Breakpoint { node } = stmt {
                diags.push(LintDiagnostic {
                    rule: "breakpoint-statement",
                    message: "found `breakpoint`; consider removing before release".to_string(),
                    severity: Severity::Info,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix: None,
                    context_lines: None,
                });
            }
        });
        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;
    use crate::core::gd_ast;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        BreakpointStatement.check(&file, source, &config)
    }

    #[test]
    fn detects_breakpoint() {
        let source = "func f():\n\tbreakpoint\n\tprint(\"hi\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "breakpoint-statement");
    }

    #[test]
    fn no_warning_without_breakpoint() {
        let source = "func f():\n\tprint(\"hi\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!BreakpointStatement.default_enabled());
    }
}
