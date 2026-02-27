use crate::core::gd_ast::{self, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct RedundantElse;

impl LintRule for RedundantElse {
    fn name(&self) -> &'static str {
        "redundant-else"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_stmts(file, &mut |stmt| {
            check_redundant_else(stmt, source, &mut diags);
        });
        diags
    }
}

fn check_redundant_else(stmt: &GdStmt<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let GdStmt::If(gif) = stmt else { return };

    // Must have an else clause, no elif
    if !gif.elif_branches.is_empty() || gif.else_body.is_none() {
        return;
    }

    // Check if the if body always terminates
    if !body_always_terminates(&gif.body) {
        return;
    }

    // The else clause node is captured during typed AST conversion
    let Some(else_node) = gif.else_node else { return };

    diags.push(LintDiagnostic {
        rule: "redundant-else",
        message: "unnecessary `else` after `return`/`break`/`continue`; remove the `else` and dedent"
            .to_string(),
        severity: Severity::Warning,
        line: else_node.start_position().row,
        column: else_node.start_position().column,
        end_column: None,
        fix: generate_else_fix(&else_node, source),
        context_lines: None,
    });
}

/// Check if the last statement in a body is a terminator.
fn body_always_terminates(body: &[GdStmt<'_>]) -> bool {
    body.last().is_some_and(|stmt| {
        matches!(
            stmt,
            GdStmt::Return { .. } | GdStmt::Break { .. } | GdStmt::Continue { .. }
        )
    })
}

fn generate_else_fix(else_node: &tree_sitter::Node<'_>, source: &str) -> Option<Fix> {
    let source_bytes = source.as_bytes();

    // Get the body of the else clause
    let body = else_node.child_by_field_name("body")?;

    // Find the start of the line containing 'else:'
    let mut else_line_start = else_node.start_byte();
    while else_line_start > 0 && source_bytes[else_line_start - 1] != b'\n' {
        else_line_start -= 1;
    }

    // Get indentation of the else clause
    let else_indent = &source[else_line_start..else_node.start_byte()];

    // Use the first/last child of the body to get the actual statement lines
    let first_stmt = body.child(0)?;
    let last_stmt = body.child(body.child_count().checked_sub(1)?)?;
    let body_first_line = first_stmt.start_position().row;
    let body_last_line = last_stmt.end_position().row;

    // Build line start offsets
    let mut line_starts: Vec<usize> = vec![0];
    for (i, &b) in source_bytes.iter().enumerate() {
        if b == b'\n' {
            line_starts.push(i + 1);
        }
    }

    let body_lines_start = line_starts[body_first_line];
    let body_lines_end = if body_last_line + 1 < line_starts.len() {
        line_starts[body_last_line + 1]
    } else {
        source_bytes.len()
    };

    let body_text = &source[body_lines_start..body_lines_end];

    // Determine how much indent to strip
    let first_line = body_text.lines().next()?;
    let first_line_indent_len = first_line.len() - first_line.trim_start().len();
    let strip_len = first_line_indent_len.saturating_sub(else_indent.len());

    // Dedent each body line
    let mut result = String::new();
    for line in body_text.lines() {
        if line.trim().is_empty() {
            result.push('\n');
        } else if line.len() >= strip_len
            && line.as_bytes()[..strip_len]
                .iter()
                .all(u8::is_ascii_whitespace)
        {
            result.push_str(&line[strip_len..]);
            result.push('\n');
        } else {
            result.push_str(else_indent);
            result.push_str(line.trim_start());
            result.push('\n');
        }
    }

    Some(Fix {
        byte_start: else_line_start,
        byte_end: body_lines_end,
        replacement: result,
    })
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
        RedundantElse.check(&file, source, &config)
    }

    #[test]
    fn detects_redundant_else_after_return() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\treturn -x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "redundant-else");
        assert!(diags[0].message.contains("unnecessary `else`"));
    }

    #[test]
    fn detects_redundant_else_after_break() {
        let source = "func f() -> void:\n\tfor i in range(10):\n\t\tif i == 5:\n\t\t\tbreak\n\t\telse:\n\t\t\tprint(i)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn detects_redundant_else_after_continue() {
        let source = "func f() -> void:\n\tfor i in range(10):\n\t\tif i == 5:\n\t\t\tcontinue\n\t\telse:\n\t\t\tprint(i)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_when_if_does_not_terminate() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\tprint(x)\n\telse:\n\t\tprint(-x)\n\treturn x\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_elif_chain() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn 1\n\telif x < 0:\n\t\treturn -1\n\telse:\n\t\treturn 0\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_without_else() {
        let source = "func f(x: int) -> void:\n\tif x > 0:\n\t\treturn\n\tprint(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_nested_redundant_else() {
        let source = "func f(x: int, y: int) -> int:\n\tif x > 0:\n\t\tif y > 0:\n\t\t\treturn 1\n\t\telse:\n\t\t\treturn -1\n\treturn 0\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_if_return_not_last() {
        let source = "func f(x: int) -> void:\n\tif x > 0:\n\t\treturn\n\t\tprint(\"unreachable\")\n\telse:\n\t\tprint(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn detects_with_multiple_statements_before_return() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\tvar y := x * 2\n\t\tprint(y)\n\t\treturn y\n\telse:\n\t\treturn -x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
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
    fn fix_removes_else_and_dedents() {
        let source = "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\treturn -x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(
            apply_fix(source, fix),
            "func f(x: int) -> int:\n\tif x > 0:\n\t\treturn x\n\treturn -x\n"
        );
    }

    #[test]
    fn fix_dedents_multi_line_body() {
        let source = "func f(x):\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\tvar y = -x\n\t\tprint(y)\n\t\treturn y\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(
            apply_fix(source, fix),
            "func f(x):\n\tif x > 0:\n\t\treturn x\n\tvar y = -x\n\tprint(y)\n\treturn y\n"
        );
    }

    #[test]
    fn fix_nested_preserves_relative_indent() {
        let source = "func f(x):\n\tif x > 0:\n\t\treturn x\n\telse:\n\t\tfor i in range(10):\n\t\t\tprint(i)\n\t\treturn -x\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        assert_eq!(
            apply_fix(source, fix),
            "func f(x):\n\tif x > 0:\n\t\treturn x\n\tfor i in range(10):\n\t\tprint(i)\n\treturn -x\n"
        );
    }
}
