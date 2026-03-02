use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnreachableCode;

impl LintRule for UnreachableCode {
    fn name(&self) -> &'static str {
        "unreachable-code"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_decls(&file.declarations, source, &mut diags);
        diags
    }
}

fn check_decls(decls: &[GdDecl<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Func(func) = decl {
            check_stmt_list(&func.body, source, diags);
        }
        if let GdDecl::Class(class) = decl {
            check_decls(&class.declarations, source, diags);
        }
    }
}

/// Check a list of statements for unreachable code after return/break/continue.
fn check_stmt_list(stmts: &[GdStmt<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    let mut terminator_idx: Option<(usize, &str)> = None;

    for (i, stmt) in stmts.iter().enumerate() {
        if let Some((_, term_name)) = terminator_idx {
            // Found unreachable code: report from this statement to the end
            emit_unreachable(stmts, i, term_name, source, diags);
            break;
        }

        match stmt {
            GdStmt::Return { .. } => {
                // Skip return statements that follow a pending() call (GUT test-skip pattern)
                if !is_after_pending(stmts, i) {
                    terminator_idx = Some((i, "return"));
                }
            }
            GdStmt::Break { .. } => terminator_idx = Some((i, "break")),
            GdStmt::Continue { .. } => terminator_idx = Some((i, "continue")),
            _ => {}
        }

        // Recurse into nested statement bodies
        visit_nested_bodies(stmt, source, diags);
    }
}

/// Check if a return statement at index `idx` is immediately preceded by a `pending()` call.
fn is_after_pending(stmts: &[GdStmt<'_>], idx: usize) -> bool {
    if idx == 0 {
        return false;
    }
    // In the typed AST, comments aren't included, so stmts[idx-1] is the actual previous statement
    let prev = &stmts[idx - 1];
    if let GdStmt::Expr { expr, .. } = prev
        && let GdExpr::Call { callee, .. } = expr
        && let GdExpr::Ident {
            name: "pending", ..
        } = callee.as_ref()
    {
        return true;
    }
    false
}

/// Emit a diagnostic for unreachable code from `start_idx` to end of `stmts`.
fn emit_unreachable(
    stmts: &[GdStmt<'_>],
    start_idx: usize,
    term_name: &str,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    let first = &stmts[start_idx];
    let last = &stmts[stmts.len() - 1];

    let first_node = first.node();
    let last_node = last.node();

    // Extend backward to include leading whitespace on the line
    let source_bytes = source.as_bytes();
    let mut byte_start = first_node.start_byte();
    while byte_start > 0 {
        let prev = byte_start - 1;
        let ch = source_bytes[prev];
        if ch == b' ' || ch == b'\t' {
            byte_start = prev;
        } else {
            break;
        }
    }

    // Extend forward to include trailing newline
    let mut byte_end = last_node.end_byte();
    if byte_end < source_bytes.len() && source_bytes[byte_end] == b'\n' {
        byte_end += 1;
    }

    diags.push(LintDiagnostic {
        rule: "unreachable-code",
        message: format!("unreachable code after `{term_name}`"),
        severity: Severity::Warning,
        line: first_node.start_position().row,
        column: first_node.start_position().column,
        end_column: None,
        fix: Some(Fix {
            byte_start,
            byte_end,
            replacement: String::new(),
        }),
        context_lines: None,
    });
}

/// Recurse into nested statement bodies to check for unreachable code.
fn visit_nested_bodies(stmt: &GdStmt<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    match stmt {
        GdStmt::If(gif) => {
            check_stmt_list(&gif.body, source, diags);
            for (_, branch_body) in &gif.elif_branches {
                check_stmt_list(branch_body, source, diags);
            }
            if let Some(else_body) = &gif.else_body {
                check_stmt_list(else_body, source, diags);
            }
        }
        GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
            check_stmt_list(body, source, diags);
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                check_stmt_list(&arm.body, source, diags);
            }
        }
        _ => {}
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
        UnreachableCode.check(&file, source, &config)
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

    #[test]
    fn no_false_positive_pending_return_pattern() {
        // GUT test-skip pattern: pending() + return + other code
        let source = "func test_thing() -> void:\n\tpending(\"not implemented\")\n\treturn\n\tvar x := 1\n\tassert_eq(x, 1)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_false_positive_pending_return_with_comment() {
        // pending() + comment + return + other code
        let source = "func test_thing() -> void:\n\tpending(\"wip\")\n\t# skipping for now\n\treturn\n\tvar x := 1\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn still_detects_unreachable_after_plain_return() {
        // Plain return without preceding pending() should still warn
        let source = "func f() -> void:\n\tprint(\"done\")\n\treturn\n\tvar x := 1\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "unreachable-code");
    }
}
