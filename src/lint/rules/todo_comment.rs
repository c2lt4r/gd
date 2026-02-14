use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct TodoComment;

impl LintRule for TodoComment {
    fn name(&self) -> &'static str {
        "todo-comment"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

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

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "comment" {
        let text = node.utf8_text(source.as_bytes()).unwrap_or("");
        let upper = text.to_ascii_uppercase();
        for marker in MARKERS {
            if upper.contains(marker) {
                diags.push(LintDiagnostic {
                    rule: "todo-comment",
                    message: format!("comment contains {marker} marker"),
                    severity: Severity::Info,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix: None,
                    context_lines: None,
                });
                // Only report the first marker per comment
                break;
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
        TodoComment.check(&tree, source, &config)
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
}
