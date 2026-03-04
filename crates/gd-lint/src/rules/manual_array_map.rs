use gd_core::gd_ast::{GdDecl, GdExpr, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct ManualArrayMap;

impl LintRule for ManualArrayMap {
    fn name(&self) -> &'static str {
        "manual-array-map"
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
        check_map_pattern(&pair[0], &pair[1], source, diags);
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

/// Detect: `var result = []` followed by `for x in arr: result.append(transform(x))`.
fn check_map_pattern(
    first: &GdStmt<'_>,
    second: &GdStmt<'_>,
    source: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    // first must be `var result = []`
    let GdStmt::Var(var_decl) = first else { return };
    let Some(GdExpr::Array { elements, .. }) = &var_decl.value else {
        return;
    };
    if !elements.is_empty() {
        return;
    }
    let result_name = var_decl.name;

    // second must be a for loop
    let GdStmt::For {
        node: for_node,
        var: loop_var,
        iter,
        body,
        ..
    } = second
    else {
        return;
    };

    // for body must be exactly one statement: result.append(expr)
    if body.len() != 1 {
        return;
    }
    let GdStmt::Expr {
        expr:
            GdExpr::MethodCall {
                receiver,
                method,
                args,
                ..
            },
        ..
    } = &body[0]
    else {
        return;
    };

    if *method != "append" {
        return;
    }
    if args.len() != 1 {
        return;
    }

    // Receiver must be the result variable
    let GdExpr::Ident {
        name: recv_name, ..
    } = receiver.as_ref()
    else {
        return;
    };
    if *recv_name != result_name {
        return;
    }

    // The appended expression must NOT be just the loop variable
    // (that would be a plain copy, not a map)
    if let GdExpr::Ident { name, .. } = &args[0]
        && *name == *loop_var
    {
        return;
    }

    let iter_text = &source[iter.node().byte_range()];
    let transform_text = &source[args[0].node().byte_range()];
    let suggestion =
        format!("var {result_name} = {iter_text}.map(func({loop_var}): return {transform_text})");

    let fix = generate_fix(&var_decl.node, for_node, &suggestion, source);

    diags.push(LintDiagnostic {
        rule: "manual-array-map",
        message: format!(
            "this loop can be replaced with `{iter_text}.map(func({loop_var}): return {transform_text})`"
        ),
        severity: Severity::Info,
        line: var_decl.node.start_position().row,
        column: var_decl.node.start_position().column,
        end_column: None,
        fix: Some(fix),
        context_lines: None,
    });
}

fn generate_fix(
    var_node: &tree_sitter::Node<'_>,
    for_node: &tree_sitter::Node<'_>,
    replacement_line: &str,
    source: &str,
) -> Fix {
    let source_bytes = source.as_bytes();

    // Start of the line containing the var declaration
    let mut line_start = var_node.start_byte();
    while line_start > 0 && source_bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let indent = &source[line_start..var_node.start_byte()];

    // End of the for loop + trailing newline
    let mut end = for_node.end_byte();
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
    use gd_core::gd_ast;
    use gd_core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        ManualArrayMap.check(&file, source, &config)
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
    fn detects_basic_map_pattern() {
        let source =
            "func f(arr):\n\tvar result = []\n\tfor x in arr:\n\t\tresult.append(transform(x))\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(
            diags[0]
                .message
                .contains("arr.map(func(x): return transform(x))")
        );
    }

    #[test]
    fn no_warning_appended_is_loop_var() {
        // Just appending the loop var is a copy, not a map
        let source = "func f(arr):\n\tvar result = []\n\tfor x in arr:\n\t\tresult.append(x)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_append_target_mismatch() {
        let source =
            "func f(arr):\n\tvar result = []\n\tfor x in arr:\n\t\tother.append(transform(x))\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_extra_statements_in_for() {
        let source = "func f(arr):\n\tvar result = []\n\tfor x in arr:\n\t\tprint(x)\n\t\tresult.append(transform(x))\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_applies_correctly() {
        let source =
            "func f(arr):\n\tvar result = []\n\tfor x in arr:\n\t\tresult.append(transform(x))\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(
            fixed,
            "func f(arr):\n\tvar result = arr.map(func(x): return transform(x))\n"
        );
    }

    #[test]
    fn opt_in_rule() {
        assert!(!ManualArrayMap.default_enabled());
    }
}
