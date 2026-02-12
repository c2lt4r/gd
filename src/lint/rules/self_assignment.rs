use tree_sitter::{Node, Tree};

use super::{Fix, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct SelfAssignment;

impl LintRule for SelfAssignment {
    fn name(&self) -> &'static str {
        "self-assignment"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        check_node(root, source.as_bytes(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source_bytes: &[u8], source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "assignment" {
        // assignment has children: left, "=", right
        let child_count = node.child_count();
        if child_count >= 3 {
            let left = node.child(0);
            let right = node.child(child_count - 1);

            if let (Some(left), Some(right)) = (left, right) {
                let left_text = &source[left.byte_range()];
                let right_text = &source[right.byte_range()];

                if left_text == right_text {
                    // For simple identifiers like `speed = speed`, the fix is to
                    // prepend `self.` to the LHS (parameter shadows instance var).
                    // For attribute access like `self.x = self.x`, it's a true
                    // no-op so we delete the entire line.
                    let fix = if left.kind() == "identifier" {
                        Fix {
                            byte_start: left.start_byte(),
                            byte_end: left.end_byte(),
                            replacement: format!("self.{}", left_text),
                        }
                    } else {
                        generate_fix(&node, source_bytes)
                    };

                    diags.push(LintDiagnostic {
                        rule: "self-assignment",
                        message: format!("`{}` is assigned to itself", left_text),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: Some(node.end_position().column),
                        fix: Some(fix),
                    });
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source_bytes, source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn generate_fix(node: &Node, source_bytes: &[u8]) -> Fix {
    // For self-assignment, we want to remove the entire line
    // This could be an assignment node or its parent (expression_statement)
    let target_node = if let Some(parent) = node.parent() {
        if parent.kind() == "expression_statement" {
            parent
        } else {
            *node
        }
    } else {
        *node
    };

    let mut byte_start = target_node.start_byte();
    let mut byte_end = target_node.end_byte();

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
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        SelfAssignment.check(&tree, source, &config)
    }

    fn apply_fix(source: &str, fix: &Fix) -> String {
        format!(
            "{}{}{}",
            &source[..fix.byte_start],
            &fix.replacement,
            &source[fix.byte_end..]
        )
    }

    #[test]
    fn fix_prepends_self_for_identifier() {
        let source = "func f(speed):\n\tspeed = speed\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(
            apply_fix(source, fix),
            "func f(speed):\n\tself.speed = speed\n"
        );
    }

    #[test]
    fn fix_deletes_self_dot_assignment() {
        let source = "func f():\n\tself.x = self.x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(apply_fix(source, fix), "func f():\n");
    }

    #[test]
    fn detects_simple_self_assignment() {
        let source = "func f():\n\tx = x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "self-assignment");
    }

    #[test]
    fn no_warning_different_values() {
        let source = "func f():\n\tx = y\n";
        assert!(check(source).is_empty());
    }
}
