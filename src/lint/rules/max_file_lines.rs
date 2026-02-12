use tree_sitter::Tree;

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MaxFileLines;

impl LintRule for MaxFileLines {
    fn name(&self) -> &'static str {
        "max-file-lines"
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let max_lines = config.max_file_lines;
        let line_count = source.lines().count();

        if line_count > max_lines {
            vec![LintDiagnostic {
                rule: "max-file-lines",
                message: format!("file is {} lines long (max {})", line_count, max_lines),
                severity: Severity::Warning,
                line: 0,
                column: 0,
                fix: None,
                end_column: None,
                context_lines: None,
            }]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    const DEFAULT_MAX_FILE_LINES: usize = 500;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        MaxFileLines.check(&tree, source, &config)
    }

    #[test]
    fn no_warning_short_file() {
        let source = "var x = 1\nvar y = 2\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_at_limit() {
        let source = "var x = 1\n".repeat(DEFAULT_MAX_FILE_LINES);
        assert!(check(&source).is_empty());
    }

    #[test]
    fn warns_over_limit() {
        let source = "var x = 1\n".repeat(DEFAULT_MAX_FILE_LINES + 1);
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "max-file-lines");
        assert_eq!(diags[0].line, 0);
        assert_eq!(diags[0].column, 0);
        assert!(
            diags[0]
                .message
                .contains(&(DEFAULT_MAX_FILE_LINES + 1).to_string())
        );
    }

    #[test]
    fn reports_correct_count() {
        let count = DEFAULT_MAX_FILE_LINES + 50;
        let source = "pass\n".repeat(count);
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains(&count.to_string()));
    }

    #[test]
    fn empty_file() {
        assert!(check("").is_empty());
    }

    #[test]
    fn single_line() {
        assert!(check("var x = 1").is_empty());
    }

    #[test]
    fn just_over_limit() {
        let source = "pass\n".repeat(DEFAULT_MAX_FILE_LINES + 1);
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains(&(DEFAULT_MAX_FILE_LINES + 1).to_string())
        );
    }

    #[test]
    fn opt_in_rule() {
        assert!(!MaxFileLines.default_enabled());
    }

    #[test]
    fn severity_is_warning() {
        let source = "pass\n".repeat(DEFAULT_MAX_FILE_LINES + 1);
        let diags = check(&source);
        assert_eq!(diags[0].severity, Severity::Warning);
    }
}
