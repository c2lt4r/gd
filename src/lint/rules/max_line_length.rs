use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct MaxLineLength;

impl LintRule for MaxLineLength {
    fn name(&self) -> &'static str {
        "max-line-length"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _file: &GdFile<'_>, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let max_len = config.max_line_length;
        let mut diags = Vec::new();

        for (i, line) in source.lines().enumerate() {
            if line.len() > max_len {
                // Skip comment lines containing URLs
                let trimmed = line.trim_start();
                if trimmed.starts_with('#')
                    && (trimmed.contains("http://") || trimmed.contains("https://"))
                {
                    continue;
                }

                diags.push(LintDiagnostic {
                    rule: "max-line-length",
                    message: format!("line is {} characters long (max {})", line.len(), max_len),
                    severity: Severity::Warning,
                    line: i,
                    column: max_len,
                    fix: None,
                    end_column: Some(line.len()),
                    context_lines: None,
                });
            }
        }

        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::parser;

    // Default from LintConfig::default().max_line_length
    const MAX_LEN: usize = 120;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        MaxLineLength.check(&file, source, &config)
    }

    #[test]
    fn no_warning_short_line() {
        let source = "var x = 1\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_at_limit() {
        let source = &format!("var x = {}\n", "0".repeat(MAX_LEN - 8));
        assert_eq!(source.lines().next().unwrap().len(), MAX_LEN);
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_over_limit() {
        let long_line = format!("var x = \"{}\"", "a".repeat(MAX_LEN));
        let source = format!("{long_line}\n");
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "max-line-length");
        assert_eq!(diags[0].line, 0);
        assert_eq!(diags[0].column, MAX_LEN);
        assert!(diags[0].message.contains(&long_line.len().to_string()));
    }

    #[test]
    fn reports_correct_line_number() {
        let short = "var a = 1\n";
        let long_line = format!("var b = \"{}\"", "x".repeat(MAX_LEN));
        let source = format!("{short}{long_line}\n");
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 1);
    }

    #[test]
    fn multiple_long_lines() {
        let long = format!("var x = \"{}\"", "a".repeat(MAX_LEN));
        let source = format!("{long}\n{long}\nvar y = 1\n{long}\n");
        let diags = check(&source);
        assert_eq!(diags.len(), 3);
        assert_eq!(diags[0].line, 0);
        assert_eq!(diags[1].line, 1);
        assert_eq!(diags[2].line, 3);
    }

    #[test]
    fn skips_comment_with_url() {
        let long_comment = format!(
            "# See https://docs.godotengine.org/en/stable/classes/class_node.html{}",
            "x".repeat(MAX_LEN)
        );
        assert!(long_comment.len() > MAX_LEN);
        let source = format!("{long_comment}\n");
        assert!(check(&source).is_empty());
    }

    #[test]
    fn skips_comment_with_http_url() {
        let long_comment = format!(
            "# See http://example.com/very/long/path/{}",
            "x".repeat(MAX_LEN)
        );
        assert!(long_comment.len() > MAX_LEN);
        let source = format!("{long_comment}\n");
        assert!(check(&source).is_empty());
    }

    #[test]
    fn does_not_skip_comment_without_url() {
        let long_comment = format!("# {}", "x".repeat(MAX_LEN));
        let source = format!("{long_comment}\n");
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_skip_code_with_url() {
        let long_code = format!("var url = \"https://example.com/{}\"", "x".repeat(MAX_LEN));
        assert!(long_code.len() > MAX_LEN);
        let source = format!("{long_code}\n");
        let diags = check(&source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn skips_indented_comment_with_url() {
        let long_comment = format!(
            "\t# https://docs.godotengine.org/en/stable/classes/{}",
            "x".repeat(MAX_LEN)
        );
        assert!(long_comment.len() > MAX_LEN);
        let source = format!("{long_comment}\n");
        assert!(check(&source).is_empty());
    }

    #[test]
    fn empty_source() {
        assert!(check("").is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!MaxLineLength.default_enabled());
    }

    #[test]
    fn end_column_set() {
        let long_line = format!("var x = \"{}\"", "a".repeat(MAX_LEN));
        let source = format!("{long_line}\n");
        let diags = check(&source);
        assert_eq!(diags[0].end_column, Some(long_line.len()));
    }
}
