use tree_sitter::Node;
use crate::core::gd_ast::{self, GdDecl, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DeeplyNestedCode;

impl LintRule for DeeplyNestedCode {
    fn name(&self) -> &'static str {
        "deeply-nested-code"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn check(&self, file: &GdFile<'_>, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let max_depth = config
            .rules
            .get("deeply-nested-code")
            .and_then(|r| r.max_depth)
            .unwrap_or(config.max_nesting_depth);
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Func(func) = decl
                && let Some(body) = func.node.child_by_field_name("body")
            {
                let _ = check_depth(body, source, func.name, 0, max_depth, &mut diags);
            }
        });
        diags
    }
}

/// Returns true if a node kind increases nesting depth.
fn is_nesting_node(kind: &str) -> bool {
    matches!(
        kind,
        "if_statement"
            | "elif_clause"
            | "else_clause"
            | "for_statement"
            | "while_statement"
            | "match_statement"
            | "pattern_section"
    )
}

/// Walk the AST tracking nesting depth. Warn on the first statement exceeding max_depth.
/// Only reports one diagnostic per function to avoid noise. Returns true if a diagnostic
/// was emitted so callers can stop recursing.
#[allow(clippy::only_used_in_recursion)]
fn check_depth(
    node: Node,
    source: &str,
    func_name: &str,
    current_depth: usize,
    max_depth: usize,
    diags: &mut Vec<LintDiagnostic>,
) -> bool {
    let mut cursor = node.walk();
    if !cursor.goto_first_child() {
        return false;
    }

    loop {
        let child = cursor.node();

        // Don't recurse into nested function definitions
        if child.kind() == "function_definition" {
            if !cursor.goto_next_sibling() {
                break;
            }
            continue;
        }

        if is_nesting_node(child.kind()) {
            let new_depth = current_depth + 1;
            if new_depth > max_depth {
                diags.push(LintDiagnostic {
                    rule: "deeply-nested-code",
                    message: format!(
                        "function `{func_name}` has code nested {new_depth} levels deep (max {max_depth})"
                    ),
                    severity: Severity::Warning,
                    line: child.start_position().row,
                    column: child.start_position().column,
                    fix: None,
                    end_column: None,
                    context_lines: None,
                });
                return true;
            }
            if check_depth(child, source, func_name, new_depth, max_depth, diags) {
                return true;
            }
        } else if check_depth(child, source, func_name, current_depth, max_depth, diags) {
            return true;
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
    false
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
