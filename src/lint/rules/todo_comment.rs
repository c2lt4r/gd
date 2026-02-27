use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct TodoComment;

/// Markers to detect in comments. Matches Godot's editor highlight keywords.
const MARKERS: &[&str] = &[
    "TODO",
    "FIXME",
    "HACK",
    "XXX",
    "BUG",
    "DEPRECATED",
    "WARNING",
];

impl LintRule for TodoComment {
    fn name(&self) -> &'static str {
        "todo-comment"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn check(&self, _file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        // Scan source lines for comments — tree-sitter not needed since
        // GDScript comments always start with `#` outside of strings.
        // We track string state to avoid false positives in string literals.
        let mut in_multiline_string = false;
        for (line_no, line) in source.lines().enumerate() {
            // Track triple-quoted multiline strings
            let triple_count = line.matches("\"\"\"").count();
            if in_multiline_string {
                if triple_count % 2 == 1 {
                    in_multiline_string = false;
                }
                continue;
            }
            if triple_count % 2 == 1 {
                in_multiline_string = true;
                continue;
            }

            // Find the first `#` that isn't inside a string
            if let Some(comment_col) = find_comment_start(line) {
                let comment_text = &line[comment_col..];
                for marker in MARKERS {
                    if contains_marker_word(comment_text, marker) {
                        diags.push(LintDiagnostic {
                            rule: "todo-comment",
                            message: format!("comment contains {marker} marker"),
                            severity: Severity::Info,
                            line: line_no,
                            column: comment_col,
                            end_column: Some(line.len()),
                            fix: None,
                            context_lines: None,
                        });
                        // Only report the first marker per comment
                        break;
                    }
                }
            }
        }
        diags
    }
}

/// Find the byte offset of the first `#` that starts a comment (not inside a string).
fn find_comment_start(line: &str) -> Option<usize> {
    let mut in_double = false;
    let mut in_single = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let ch = bytes[i];
        match ch {
            b'"' if !in_single => in_double = !in_double,
            b'\'' if !in_double => in_single = !in_single,
            b'\\' if in_double || in_single => {
                i += 1; // skip escaped char
            }
            b'#' if !in_double && !in_single => return Some(i),
            _ => {}
        }
        i += 1;
    }
    None
}

/// Check if a marker appears as a standalone word in the comment text.
/// The marker must be preceded by `#`, whitespace, or start of string,
/// and followed by `:`, whitespace, end of string, or non-alphanumeric.
fn contains_marker_word(text: &str, marker: &str) -> bool {
    let upper = text.to_ascii_uppercase();
    let bytes = upper.as_bytes();
    let mut start = 0;
    while let Some(pos) = upper[start..].find(marker) {
        let abs = start + pos;
        let before_ok = abs == 0 || matches!(bytes[abs - 1], b'#' | b' ' | b'\t');
        let after = abs + marker.len();
        let after_ok = after >= bytes.len() || !bytes[after].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
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
        TodoComment.check(&file, source, &config)
    }

    // ── Detects markers ───────────────────────────────────────────────

    #[test]
    fn detects_todo() {
        let source = "# TODO: implement this\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("TODO"));
    }

    #[test]
    fn detects_fixme() {
        let source = "# FIXME: broken logic\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("FIXME"));
    }

    #[test]
    fn detects_hack() {
        let source = "# HACK: workaround for bug\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("HACK"));
    }

    #[test]
    fn detects_xxx() {
        let source = "# XXX: needs review\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("XXX"));
    }

    #[test]
    fn detects_bug() {
        let source = "# BUG: crashes on null input\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("BUG"));
    }

    #[test]
    fn detects_deprecated() {
        let source = "# DEPRECATED: use new_func instead\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("DEPRECATED"));
    }

    #[test]
    fn detects_warning() {
        let source = "# WARNING: not thread-safe\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("WARNING"));
    }

    // ── Case insensitive ──────────────────────────────────────────────

    #[test]
    fn case_insensitive_todo() {
        let source = "# todo: do this later\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("TODO"));
    }

    #[test]
    fn mixed_case() {
        let source = "# Todo: mixed case\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── No false positives ────────────────────────────────────────────

    #[test]
    fn no_warning_for_normal_comment() {
        let source = "# This is a normal comment\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_debug_substring() {
        let source = "# Debug trail for visualizing nav paths\n";
        assert!(check(source).is_empty(), "Debug should not match BUG");
    }

    #[test]
    fn no_warning_xxx_in_string_literal_comment() {
        let source = "# \"xxx-1f\", \"xxx-b2f\" are format codes\n";
        assert!(check(source).is_empty(), "xxx- should not match XXX marker");
    }

    #[test]
    fn no_warning_for_code() {
        let source = "var todo_list = []\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn empty_comment() {
        let source = "#\n";
        assert!(check(source).is_empty());
    }

    // ── Multiple comments ─────────────────────────────────────────────

    #[test]
    fn multiple_comments_multiple_diags() {
        let source = "# TODO: first\n# FIXME: second\n";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("TODO"));
        assert!(diags[1].message.contains("FIXME"));
    }

    #[test]
    fn only_first_marker_reported_per_comment() {
        let source = "# TODO FIXME both in one line\n";
        let diags = check(source);
        // Only the first marker should be reported
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("TODO"));
    }

    // ── Inline comments ───────────────────────────────────────────────

    #[test]
    fn detects_inline_todo() {
        let source = "var x = 5 # TODO: refactor\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Inside functions ──────────────────────────────────────────────

    #[test]
    fn detects_in_function_body() {
        let source = "func foo():\n\t# TODO: implement\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── Opt-in ────────────────────────────────────────────────────────

    #[test]
    fn is_default_enabled() {
        assert!(TodoComment.default_enabled());
    }

    #[test]
    fn severity_is_info() {
        let source = "# TODO: implement this\n";
        let diags = check(source);
        assert_eq!(diags[0].severity, Severity::Info);
    }

    // ── Span correctness ──────────────────────────────────────────────

    #[test]
    fn diagnostic_points_to_comment() {
        let source = "# TODO: something\n";
        let diags = check(source);
        assert_eq!(diags[0].line, 0);
        assert_eq!(diags[0].column, 0);
    }

    #[test]
    fn marker_in_middle_of_comment() {
        let source = "# some text TODO more text\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    // ── String literal exclusion ──────────────────────────────────────

    #[test]
    fn no_warning_hash_in_string() {
        let source = "var s = \"# TODO: not a real comment\"\n";
        assert!(check(source).is_empty());
    }
}
