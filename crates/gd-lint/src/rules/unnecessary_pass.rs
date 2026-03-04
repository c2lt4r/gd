use gd_core::gd_ast::{GdClass, GdDecl, GdFile, GdFunc, GdIf, GdMatchArm, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct UnnecessaryPass;

impl LintRule for UnnecessaryPass {
    fn name(&self) -> &'static str {
        "unnecessary-pass"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        for decl in &file.declarations {
            check_decl(decl, source, &mut diags);
        }
        diags
    }
}

fn check_decl(decl: &GdDecl, source: &str, diags: &mut Vec<LintDiagnostic>) {
    match decl {
        GdDecl::Func(func) => check_func(func, source, diags),
        GdDecl::Class(cls) => check_class(cls, source, diags),
        _ => {}
    }
}

fn check_func(func: &GdFunc, source: &str, diags: &mut Vec<LintDiagnostic>) {
    check_body(&func.body, source, diags);
}

fn check_class(cls: &GdClass, source: &str, diags: &mut Vec<LintDiagnostic>) {
    for decl in &cls.declarations {
        check_decl(decl, source, diags);
    }
}

/// Check a statement list for unnecessary `pass` — if the body has more than one statement
/// and one of them is `pass`, the `pass` is redundant.
fn check_body(stmts: &[GdStmt], source: &str, diags: &mut Vec<LintDiagnostic>) {
    if stmts.len() > 1 {
        for stmt in stmts {
            if let GdStmt::Pass { node } = stmt {
                let fix = generate_fix(node.start_byte(), node.end_byte(), source.as_bytes());
                diags.push(LintDiagnostic {
                    rule: "unnecessary-pass",
                    message: "`pass` is unnecessary when the body contains other statements"
                        .to_string(),
                    severity: Severity::Warning,
                    line: node.start_position().row,
                    column: node.start_position().column,
                    end_column: Some(node.end_position().column),
                    fix: Some(fix),
                    context_lines: None,
                });
            }
        }
    }

    // Recurse into nested blocks
    for stmt in stmts {
        check_stmt(stmt, source, diags);
    }
}

fn check_stmt(stmt: &GdStmt, source: &str, diags: &mut Vec<LintDiagnostic>) {
    match stmt {
        GdStmt::If(if_stmt) => check_if(if_stmt, source, diags),
        GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
            check_body(body, source, diags);
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                check_match_arm(arm, source, diags);
            }
        }
        _ => {}
    }
}

fn check_if(if_stmt: &GdIf, source: &str, diags: &mut Vec<LintDiagnostic>) {
    check_body(&if_stmt.body, source, diags);
    for (_, branch) in &if_stmt.elif_branches {
        check_body(branch, source, diags);
    }
    if let Some(else_body) = &if_stmt.else_body {
        check_body(else_body, source, diags);
    }
}

fn check_match_arm(arm: &GdMatchArm, source: &str, diags: &mut Vec<LintDiagnostic>) {
    check_body(&arm.body, source, diags);
}

fn generate_fix(start_byte: usize, end_byte: usize, source_bytes: &[u8]) -> Fix {
    let mut byte_start = start_byte;
    let mut byte_end = end_byte;

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
    use gd_core::config::LintConfig;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = gd_core::parser::parse(source).unwrap();
        let file = gd_core::gd_ast::convert(&tree, source);
        UnnecessaryPass.check(&file, source, &LintConfig::default())
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
        // Note: typed AST strips comments from body, so body.len() == 1 (just pass)
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
