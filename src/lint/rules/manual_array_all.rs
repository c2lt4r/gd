use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ManualArrayAll;

impl LintRule for ManualArrayAll {
    fn name(&self) -> &'static str {
        "manual-array-all"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_decls(&file.declarations, source, &mut diags);
        diags
    }
}

fn check_decls(decls: &[GdDecl<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        match decl {
            GdDecl::Func(func) => check_stmts(&func.body, source, diags),
            GdDecl::Class(class) => check_decls(&class.declarations, source, diags),
            _ => {}
        }
    }
}

fn check_stmts(stmts: &[GdStmt<'_>], source: &str, diags: &mut Vec<LintDiagnostic>) {
    for pair in stmts.windows(2) {
        check_all_pattern(&pair[0], &pair[1], source, diags);
    }

    // Recurse into nested blocks
    for stmt in stmts {
        recurse_into(stmt, source, diags);
    }
}

fn recurse_into(stmt: &GdStmt<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    match stmt {
        GdStmt::If(gif) => {
            check_stmts(&gif.body, source, diags);
            for (_, branch) in &gif.elif_branches {
                check_stmts(branch, source, diags);
            }
            if let Some(else_body) = &gif.else_body {
                check_stmts(else_body, source, diags);
            }
        }
        GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
            check_stmts(body, source, diags);
        }
        GdStmt::Match { arms, .. } => {
            for arm in arms {
                check_stmts(&arm.body, source, diags);
            }
        }
        _ => {}
    }
}

/// Detect: `for x in arr: if cond: return false` followed by `return true`.
fn check_all_pattern(
    first: &GdStmt<'_>,
    second: &GdStmt<'_>,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    // second must be `return true`
    let GdStmt::Return {
        value: Some(GdExpr::Bool { value: true, .. }),
        ..
    } = second
    else {
        return;
    };

    // first must be a for loop
    let GdStmt::For {
        node: for_node,
        var,
        iter,
        body,
        ..
    } = first
    else {
        return;
    };

    // for body must be exactly one statement: an if
    if body.len() != 1 {
        return;
    }
    let GdStmt::If(gif) = &body[0] else { return };

    // if must have no elif/else
    if !gif.elif_branches.is_empty() || gif.else_body.is_some() {
        return;
    }

    // if body must be exactly one statement: return false
    if gif.body.len() != 1 {
        return;
    }
    let GdStmt::Return {
        value: Some(GdExpr::Bool { value: false, .. }),
        ..
    } = &gif.body[0]
    else {
        return;
    };

    let iter_text = &source[iter.node().byte_range()];
    let cond_text = &source[gif.condition.node().byte_range()];
    let suggestion = format!("return {iter_text}.all(func({var}): return not ({cond_text}))");

    let fix = generate_fix(for_node, &second.node(), &suggestion, source);

    diags.push(LintDiagnostic {
        rule: "manual-array-all",
        message: format!(
            "this loop can be replaced with `{iter_text}.all(func({var}): return not ({cond_text}))`"
        ),
        severity: Severity::Info,
        line: for_node.start_position().row,
        column: for_node.start_position().column,
        end_column: None,
        fix: Some(fix),
        context_lines: None,
    });
}

fn generate_fix(
    start_node: &tree_sitter::Node<'_>,
    end_node: &tree_sitter::Node<'_>,
    replacement_line: &str,
    source: &str,
) -> Fix {
    let source_bytes = source.as_bytes();

    let mut line_start = start_node.start_byte();
    while line_start > 0 && source_bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let indent = &source[line_start..start_node.start_byte()];

    let mut end = end_node.end_byte();
    if end < source_bytes.len() && source_bytes[end] == b'\n' {
        end += 1;
    }

    Fix {
        byte_start: line_start,
        byte_end: end,
        replacement: format!("{indent}{replacement_line}\n"),
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
        ManualArrayAll.check(&file, source, &config)
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
    fn detects_basic_all_pattern() {
        let source =
            "func f(arr):\n\tfor x in arr:\n\t\tif x <= 0:\n\t\t\treturn false\n\treturn true\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains("arr.all(func(x): return not (x <= 0))")
        );
    }

    #[test]
    fn no_warning_any_pattern() {
        // return true in if, return false after loop -- that's manual-array-any
        let source =
            "func f(arr):\n\tfor x in arr:\n\t\tif x > 0:\n\t\t\treturn true\n\treturn false\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_if_has_else() {
        let source = "func f(arr):\n\tfor x in arr:\n\t\tif x <= 0:\n\t\t\treturn false\n\t\telse:\n\t\t\tpass\n\treturn true\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_applies_correctly() {
        let source =
            "func f(arr):\n\tfor x in arr:\n\t\tif x <= 0:\n\t\t\treturn false\n\treturn true\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(
            fixed,
            "func f(arr):\n\treturn arr.all(func(x): return not (x <= 0))\n"
        );
    }

    #[test]
    fn opt_in_rule() {
        assert!(!ManualArrayAll.default_enabled());
    }
}
