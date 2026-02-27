use crate::core::gd_ast::{self, GdExpr, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct SelfAssignment;

impl LintRule for SelfAssignment {
    fn name(&self) -> &'static str {
        "self-assignment"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let source_bytes = source.as_bytes();
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Assign { node, target, value } = stmt {
                let left_text = &source[target.node().byte_range()];
                let right_text = &source[value.node().byte_range()];

                if left_text == right_text {
                    // For simple identifiers like `speed = speed`, the fix is to
                    // prepend `self.` to the LHS (parameter shadows instance var).
                    // For attribute access like `self.x = self.x`, it's a true
                    // no-op so we delete the entire line.
                    let fix = if matches!(target, GdExpr::Ident { .. }) {
                        Fix {
                            byte_start: target.node().start_byte(),
                            byte_end: target.node().end_byte(),
                            replacement: format!("self.{left_text}"),
                        }
                    } else {
                        generate_line_delete(node, source_bytes)
                    };

                    diags.push(LintDiagnostic {
                        rule: "self-assignment",
                        message: format!("`{left_text}` is assigned to itself"),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: Some(node.end_position().column),
                        fix: Some(fix),
                        context_lines: None,
                    });
                }
            }
        });
        diags
    }
}

fn generate_line_delete(node: &tree_sitter::Node, source_bytes: &[u8]) -> Fix {
    // For self-assignment, we want to remove the entire line.
    // The node is expression_statement (the Assign's backing stmt node).
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
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        SelfAssignment.check(&file, source, &config)
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
