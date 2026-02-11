use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnreachableCode;

impl LintRule for UnreachableCode {
    fn name(&self) -> &'static str {
        "unreachable-code"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Check body/block nodes for statements after return/break/continue
    if is_body_node(node.kind()) {
        check_body_for_unreachable(node, source, diags);
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

fn is_body_node(kind: &str) -> bool {
    matches!(kind, "body" | "block")
}

fn is_terminator(kind: &str) -> bool {
    matches!(
        kind,
        "return_statement" | "break_statement" | "continue_statement"
    )
}

fn check_body_for_unreachable(body: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let source_bytes = source.as_bytes();
    let mut found_terminator: Option<&str> = None;
    let mut first_unreachable_start: Option<usize> = None;
    let mut last_unreachable_end: usize = 0;
    let mut first_unreachable_pos: Option<(usize, usize)> = None;

    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return;
    }

    loop {
        let child = cursor.node();
        if !child.is_named() || child.kind() == "comment" {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        if found_terminator.is_some() {
            if first_unreachable_start.is_none() {
                // Extend backward to include leading whitespace on the line
                let mut start = child.start_byte();
                while start > 0 {
                    let prev = start - 1;
                    let ch = source_bytes[prev];
                    if ch == b' ' || ch == b'\t' {
                        start = prev;
                    } else {
                        break;
                    }
                }
                first_unreachable_start = Some(start);
                first_unreachable_pos =
                    Some((child.start_position().row, child.start_position().column));
            }
            last_unreachable_end = child.end_byte();
        }

        if is_terminator(child.kind()) && found_terminator.is_none() {
            found_terminator = Some(match child.kind() {
                "return_statement" => "return",
                "break_statement" => "break",
                "continue_statement" => "continue",
                _ => unreachable!(),
            });
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }

    if let (Some(term), Some(byte_start), Some((line, col))) = (
        found_terminator,
        first_unreachable_start,
        first_unreachable_pos,
    ) {
        // Extend to include trailing newline
        let mut byte_end = last_unreachable_end;
        if byte_end < source_bytes.len() && source_bytes[byte_end] == b'\n' {
            byte_end += 1;
        }

        diags.push(LintDiagnostic {
            rule: "unreachable-code",
            message: format!("unreachable code after `{}`", term),
            severity: Severity::Warning,
            line,
            column: col,
            end_column: None,
            fix: Some(Fix {
                byte_start,
                byte_end,
                replacement: String::new(),
            }),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        UnreachableCode.check(&tree, source, &config)
    }

    #[test]
    fn no_false_positive_on_comments_after_return() {
        let source = "func f() -> void:\n\treturn  # done\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_false_positive_on_match_arms_with_comments() {
        let source = "func f(x: int) -> String:\n\tmatch x:\n\t\t0:\n\t\t\treturn \"a\"  # first\n\t\t1:\n\t\t\treturn \"b\"  # second\n\t\t_:\n\t\t\treturn \"c\"\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn still_detects_real_unreachable_code() {
        let source = "func f() -> void:\n\treturn\n\tvar x := 1\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "unreachable-code");
    }
}
