use gd_core::gd_ast::{self, GdDecl, GdFile, GdStmt};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct DeeplyNestedCode;

impl LintRule for DeeplyNestedCode {
    fn name(&self) -> &'static str {
        "deeply-nested-code"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let max_depth = config
            .rules
            .get("deeply-nested-code")
            .and_then(|r| r.max_depth)
            .unwrap_or(config.max_nesting_depth);
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl {
                check_stmts_depth(&func.body, func.name, 0, max_depth, &mut diags);
            }
        });
        diags
    }
}

/// Walk the typed AST tracking nesting depth. Returns true if a diagnostic
/// was emitted (stop-at-first per function).
fn check_stmts_depth(
    stmts: &[GdStmt],
    func_name: &str,
    depth: usize,
    max_depth: usize,
    diags: &mut Vec<LintDiagnostic>,
) -> bool {
    for stmt in stmts {
        match stmt {
            GdStmt::If(if_stmt) => {
                let new_depth = depth + 1;
                if new_depth > max_depth {
                    emit(stmt, func_name, new_depth, max_depth, diags);
                    return true;
                }
                if check_stmts_depth(&if_stmt.body, func_name, new_depth, max_depth, diags) {
                    return true;
                }
                // elif adds another nesting level (same as tree-sitter elif_clause)
                for (cond, branch) in &if_stmt.elif_branches {
                    let elif_depth = new_depth + 1;
                    if elif_depth > max_depth {
                        let pos = cond.node().start_position();
                        emit_at(pos.row, pos.column, func_name, elif_depth, max_depth, diags);
                        return true;
                    }
                    if check_stmts_depth(branch, func_name, elif_depth, max_depth, diags) {
                        return true;
                    }
                }
                // else adds another nesting level (same as tree-sitter else_clause)
                if let Some(else_body) = &if_stmt.else_body {
                    let else_depth = new_depth + 1;
                    if else_depth > max_depth {
                        let pos = else_body.first().map_or_else(
                            || if_stmt.node.start_position(),
                            |s| s.node().start_position(),
                        );
                        emit_at(pos.row, pos.column, func_name, else_depth, max_depth, diags);
                        return true;
                    }
                    if check_stmts_depth(else_body, func_name, else_depth, max_depth, diags) {
                        return true;
                    }
                }
            }
            GdStmt::For { body, .. } | GdStmt::While { body, .. } => {
                let new_depth = depth + 1;
                if new_depth > max_depth {
                    emit(stmt, func_name, new_depth, max_depth, diags);
                    return true;
                }
                if check_stmts_depth(body, func_name, new_depth, max_depth, diags) {
                    return true;
                }
            }
            GdStmt::Match { arms, .. } => {
                let new_depth = depth + 1;
                if new_depth > max_depth {
                    emit(stmt, func_name, new_depth, max_depth, diags);
                    return true;
                }
                // Each match arm (pattern_section) adds another level
                for arm in arms {
                    let arm_depth = new_depth + 1;
                    if arm_depth > max_depth {
                        let pos = arm.node.start_position();
                        emit_at(pos.row, pos.column, func_name, arm_depth, max_depth, diags);
                        return true;
                    }
                    if check_stmts_depth(&arm.body, func_name, arm_depth, max_depth, diags) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

fn emit(
    stmt: &GdStmt,
    func_name: &str,
    depth: usize,
    max_depth: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    let pos = stmt.node().start_position();
    emit_at(pos.row, pos.column, func_name, depth, max_depth, diags);
}

fn emit_at(
    row: usize,
    column: usize,
    func_name: &str,
    depth: usize,
    max_depth: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    diags.push(LintDiagnostic {
        rule: "deeply-nested-code",
        message: format!(
            "function `{func_name}` has code nested {depth} levels deep (max {max_depth})"
        ),
        severity: Severity::Warning,
        line: row,
        column,
        fix: None,
        end_column: None,
        context_lines: None,
    });
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
        DeeplyNestedCode.check(&file, source, &config)
    }

    #[test]
    fn no_warning_shallow() {
        let source = "\
func foo(x):
\tif x:
\t\tprint(x)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_at_max_depth() {
        // 4 levels: if -> for -> if -> while
        let source = "\
func foo(items):
\tif items:
\t\tfor item in items:
\t\t\tif item > 0:
\t\t\t\twhile item > 10:
\t\t\t\t\titem -= 1
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_over_max_depth() {
        // 5 levels: if -> for -> if -> while -> if
        let source = "\
func foo(items):
\tif items:
\t\tfor item in items:
\t\t\tif item > 0:
\t\t\t\twhile item > 10:
\t\t\t\t\tif item == 5:
\t\t\t\t\t\tprint(\"deep\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "deeply-nested-code");
        assert!(diags[0].message.contains("foo"));
        assert!(diags[0].message.contains("5 levels"));
    }

    #[test]
    fn elif_counts_as_nesting() {
        // if -> elif -> for -> while -> if = 5 levels
        let source = "\
func foo(x, items):
\tif x > 0:
\t\tpass
\telif x < 0:
\t\tfor item in items:
\t\t\twhile item:
\t\t\t\tif item == 1:
\t\t\t\t\tprint(\"deep\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn else_counts_as_nesting() {
        // if -> else -> for -> while -> if = 5 levels
        let source = "\
func foo(x, items):
\tif x > 0:
\t\tpass
\telse:
\t\tfor item in items:
\t\t\twhile item:
\t\t\t\tif item == 1:
\t\t\t\t\tprint(\"deep\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn match_counts_as_nesting() {
        // match -> pattern_section -> if -> for -> while = 5
        let source = "\
func foo(x, items):
\tmatch x:
\t\t1:
\t\t\tif true:
\t\t\t\tfor item in items:
\t\t\t\t\twhile item:
\t\t\t\t\t\tprint(\"deep\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn one_diagnostic_per_function() {
        // Multiple deep paths in same function — only one warning
        let source = "\
func foo(items):
\tif items:
\t\tfor item in items:
\t\t\tif item > 0:
\t\t\t\twhile item > 10:
\t\t\t\t\tif true:
\t\t\t\t\t\tpass
\tif items:
\t\tfor item in items:
\t\t\tif item > 0:
\t\t\t\twhile item > 10:
\t\t\t\t\tif true:
\t\t\t\t\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn separate_functions_get_separate_warnings() {
        let source = "\
func foo(items):
\tif items:
\t\tfor item in items:
\t\t\tif item > 0:
\t\t\t\twhile item > 10:
\t\t\t\t\tif true:
\t\t\t\t\t\tpass

func bar(items):
\tif items:
\t\tfor item in items:
\t\t\tif item > 0:
\t\t\t\twhile item > 10:
\t\t\t\t\tif true:
\t\t\t\t\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
        assert!(diags[0].message.contains("foo"));
        assert!(diags[1].message.contains("bar"));
    }

    #[test]
    fn checks_inner_class_functions() {
        let source = "\
class Inner:
\tfunc deep(items):
\t\tif items:
\t\t\tfor item in items:
\t\t\t\tif item > 0:
\t\t\t\t\twhile item > 10:
\t\t\t\t\t\tif true:
\t\t\t\t\t\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("deep"));
    }

    #[test]
    fn no_warning_flat_code() {
        let source = "\
func foo():
\tvar a = 1
\tvar b = 2
\tprint(a + b)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn does_not_recurse_into_nested_functions() {
        // Lambda/nested func depth should be independent
        let source = "\
func outer():
\tif true:
\t\tfor i in [1]:
\t\t\tif true:
\t\t\t\tpass
";
        // depth 3, under threshold
        assert!(check(source).is_empty());
    }
}
