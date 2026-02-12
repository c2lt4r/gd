use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnnecessaryPass;

impl LintRule for UnnecessaryPass {
    fn name(&self) -> &'static str {
        "unnecessary-pass"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source.as_bytes(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source_bytes: &[u8], _source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Check body nodes: if they have more than one non-comment named child and one is pass_statement
    if node.kind() == "body" || node.kind() == "block" {
        let named_count = node.named_child_count();
        // Don't count comments as "other statements" — a setter with just `pass  # Read-only`
        // should not trigger this rule since pass is the only real statement.
        let statement_count = (0..named_count)
            .filter(|&i| node.named_child(i).is_some_and(|c| c.kind() != "comment"))
            .count();
        if statement_count > 1 {
            for i in 0..named_count {
                if let Some(child) = node.named_child(i)
                    && child.kind() == "pass_statement"
                {
                    let fix = generate_fix(&child, source_bytes);

                    diags.push(LintDiagnostic {
                        rule: "unnecessary-pass",
                        message: "`pass` is unnecessary when the body contains other statements"
                            .to_string(),
                        severity: Severity::Warning,
                        line: child.start_position().row,
                        column: child.start_position().column,
                        end_column: Some(child.end_position().column),
                        fix: Some(fix),
                        context_lines: None,
                    });
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source_bytes, _source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn generate_fix(node: &Node, source_bytes: &[u8]) -> Fix {
    let mut byte_start = node.start_byte();
    let mut byte_end = node.end_byte();

    // Extend to include trailing newline if present
    if byte_end < source_bytes.len() && source_bytes[byte_end] == b'\n' {
        byte_end += 1;
    }

    // Extend backward to include leading whitespace on the line
    while byte_start > 0 {
        let prev = byte_start - 1;
        let ch = source_bytes[prev];
        if ch == b' ' || ch == b'\t' {
            byte_start = prev;
        } else if ch == b'\n' {
            // Don't include the previous newline, just stop here
            break;
        } else {
            break;
        }
    }

    Fix {
        byte_start,
        byte_end,
        replacement: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::LintConfig;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        UnnecessaryPass.check(&tree, source, &LintConfig::default())
    }

    #[test]
    fn warns_on_pass_with_other_statements() {
        let diags = check("func foo():\n\tvar x = 1\n\tpass\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "unnecessary-pass");
    }

    #[test]
    fn no_warning_on_pass_only() {
        let diags = check("func foo():\n\tpass\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_on_pass_with_comment() {
        // Read-only setter pattern: pass + comment should not trigger
        let source = "\
var scores: Dictionary:
\tset(value):
\t\tpass  # Read-only
";
        let diags = check(source);
        assert!(
            diags.is_empty(),
            "pass with only a comment should not trigger unnecessary-pass, got: {:?}",
            diags.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }

    #[test]
    fn no_warning_on_standalone_comment_before_pass() {
        let source = "\
func foo():
\t# This function intentionally does nothing
\tpass
";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn still_warns_pass_with_real_statement_and_comment() {
        let source = "\
func foo():
\tvar x = 1  # important
\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }
}
