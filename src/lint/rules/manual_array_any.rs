use crate::core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct ManualArrayAny;

impl LintRule for ManualArrayAny {
    fn name(&self) -> &'static str {
        "manual-array-any"
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
        check_any_pattern(&pair[0], &pair[1], source, diags);
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

/// Detect: `for x in arr: if cond: return true` followed by `return false`.
fn check_any_pattern(
    first: &GdStmt<'_>,
    second: &GdStmt<'_>,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    // second must be `return false`
    let GdStmt::Return { value: Some(GdExpr::Bool { value: false, .. }), .. } = second else {
        return;
    };

    // first must be a for loop
    let GdStmt::For { node: for_node, var, iter, body, .. } = first else {
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

    // if body must be exactly one statement: return true
    if gif.body.len() != 1 {
        return;
    }
    let GdStmt::Return { value: Some(GdExpr::Bool { value: true, .. }), .. } = &gif.body[0]
    else {
        return;
    };

    let iter_text = &source[iter.node().byte_range()];
    let cond_text = &source[gif.condition.node().byte_range()];
    let suggestion = format!("return {iter_text}.any(func({var}): return {cond_text})");

    let fix = generate_fix(for_node, &second.node(), &suggestion, source);

    diags.push(LintDiagnostic {
        rule: "manual-array-any",
        message: format!(
            "this loop can be replaced with `{iter_text}.any(func({var}): return {cond_text})`"
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

    // Find the start of the line containing the for
    let mut line_start = start_node.start_byte();
    while line_start > 0 && source_bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let indent = &source[line_start..start_node.start_byte()];

    // End includes the return statement + trailing newline
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
        ManualArrayAny.check(&file, source, &config)
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
    fn detects_basic_any_pattern() {
        let source = "func f(arr):\n\tfor x in arr:\n\t\tif x > 0:\n\t\t\treturn true\n\treturn false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("arr.any(func(x): return x > 0)"));
    }

    #[test]
    fn detects_complex_condition() {
        let source = "func f(items):\n\tfor item in items:\n\t\tif item.is_valid() and item.health > 0:\n\t\t\treturn true\n\treturn false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("items.any("));
    }

    #[test]
    fn no_warning_extra_statements_in_loop() {
        let source = "func f(arr):\n\tfor x in arr:\n\t\tprint(x)\n\t\tif x > 0:\n\t\t\treturn true\n\treturn false\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_if_has_else() {
        let source = "func f(arr):\n\tfor x in arr:\n\t\tif x > 0:\n\t\t\treturn true\n\t\telse:\n\t\t\tpass\n\treturn false\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_if_has_elif() {
        let source = "func f(arr):\n\tfor x in arr:\n\t\tif x > 0:\n\t\t\treturn true\n\t\telif x == 0:\n\t\t\tpass\n\treturn false\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_all_pattern() {
        // return false in if, return true after loop -- that's manual-array-all
        let source = "func f(arr):\n\tfor x in arr:\n\t\tif x > 0:\n\t\t\treturn false\n\treturn true\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_bare_for_without_return() {
        let source = "func f(arr):\n\tfor x in arr:\n\t\tif x > 0:\n\t\t\treturn true\n\tprint(\"done\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_applies_correctly() {
        let source = "func f(arr):\n\tfor x in arr:\n\t\tif x > 0:\n\t\t\treturn true\n\treturn false\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(
            fixed,
            "func f(arr):\n\treturn arr.any(func(x): return x > 0)\n"
        );
    }

    #[test]
    fn opt_in_rule() {
        assert!(!ManualArrayAny.default_enabled());
    }
}
