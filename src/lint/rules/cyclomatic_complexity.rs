use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct CyclomaticComplexity;

impl LintRule for CyclomaticComplexity {
    fn name(&self) -> &'static str {
        "cyclomatic-complexity"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Complexity
    }

    fn check(&self, file: &GdFile<'_>, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = file.node;
        let max_complexity = config
            .rules
            .get("cyclomatic-complexity")
            .and_then(|r| r.max_complexity)
            .unwrap_or(config.max_cyclomatic_complexity);
        collect_functions(root, source, max_complexity, &mut diags);
        diags
    }
}

fn collect_functions(
    node: Node,
    source: &str,
    max_complexity: usize,
    diags: &mut Vec<LintDiagnostic>,
) {
    if node.kind() == "function_definition" {
        let complexity = compute_complexity(node, source);
        if complexity > max_complexity {
            let func_name = node
                .child_by_field_name("name")
                .map_or("<unknown>", |n| &source[n.byte_range()]);
            diags.push(LintDiagnostic {
                rule: "cyclomatic-complexity",
                message: format!(
                    "function `{func_name}` has cyclomatic complexity of {complexity} (max {max_complexity})"
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                fix: None,
                end_column: None,
                context_lines: None,
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_functions(cursor.node(), source, max_complexity, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Compute cyclomatic complexity of a function.
/// Starts at 1 and increments for each branching construct.
fn compute_complexity(func: Node, source: &str) -> usize {
    let Some(body) = func.child_by_field_name("body") else {
        return 1;
    };
    let mut complexity = 1;
    count_branches(body, source, &mut complexity, false);
    complexity
}

fn count_branches(node: Node, source: &str, complexity: &mut usize, in_guard: bool) {
    match node.kind() {
        "if_statement" => {
            *complexity += 1;
            // If this is a guard clause (body is just `return`/`continue`/`break`),
            // don't count and/or in its condition — the guard is one decision point.
            if is_guard_clause(&node) {
                // Skip condition (don't count and/or), only recurse into non-condition children
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child = cursor.node();
                        // The condition is typically the first named child before "body"
                        if child.kind() == "body"
                            || child.kind() == "elif_branch"
                            || child.kind() == "else_branch"
                        {
                            count_branches(child, source, complexity, false);
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
                return;
            }
        }
        "elif_clause" | "for_statement" | "while_statement" | "pattern_section" => {
            *complexity += 1;
        }
        "binary_operator" => {
            if !in_guard && let Some(op_node) = node.child_by_field_name("op") {
                let op_text = &source[op_node.byte_range()];
                if op_text == "and" || op_text == "or" {
                    *complexity += 1;
                }
            }
        }
        "function_definition" => return,
        _ => {}
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            count_branches(cursor.node(), source, complexity, in_guard);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

/// Check if an if_statement is a guard clause: body contains only `return`, `continue`, or `break`.
fn is_guard_clause(if_node: &Node) -> bool {
    // Must have no elif/else branches
    let mut cursor = if_node.walk();
    for child in if_node.children(&mut cursor) {
        if child.kind() == "elif_branch" || child.kind() == "else_branch" {
            return false;
        }
    }

    let Some(body) = if_node.child_by_field_name("body") else {
        return false;
    };

    // Body must contain exactly one statement that is return/continue/break
    let mut stmt_count = 0;
    let mut is_early_exit = false;
    let mut c = body.walk();
    for child in body.children(&mut c) {
        if !child.is_named() || child.kind() == "comment" {
            continue;
        }
        stmt_count += 1;
        if stmt_count > 1 {
            return false;
        }
        is_early_exit = matches!(
            child.kind(),
            "return_statement" | "continue_statement" | "break_statement"
        );
    }
    stmt_count == 1 && is_early_exit
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
        CyclomaticComplexity.check(&file, source, &config)
    }

    fn complexity_of(source: &str) -> usize {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let root = file.node;
        let mut cursor = root.walk();
        cursor.goto_first_child();
        loop {
            if cursor.node().kind() == "function_definition" {
                return compute_complexity(cursor.node(), source);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        panic!("no function found");
    }

    #[test]
    fn simple_function_complexity_1() {
        let source = "func foo():\n\tpass\n";
        assert_eq!(complexity_of(source), 1);
    }

    #[test]
    fn single_if() {
        let source = "\
func foo(x):
\tif x > 0:
\t\tprint(x)
";
        assert_eq!(complexity_of(source), 2);
    }

    #[test]
    fn if_elif_else() {
        let source = "\
func foo(x):
\tif x > 0:
\t\tprint(\"pos\")
\telif x < 0:
\t\tprint(\"neg\")
\telse:
\t\tprint(\"zero\")
";
        // 1 base + 1 if + 1 elif = 3
        assert_eq!(complexity_of(source), 3);
    }

    #[test]
    fn for_loop() {
        let source = "\
func foo(arr):
\tfor item in arr:
\t\tprint(item)
";
        assert_eq!(complexity_of(source), 2);
    }

    #[test]
    fn while_loop() {
        let source = "\
func foo():
\tvar i = 0
\twhile i < 10:
\t\ti += 1
";
        assert_eq!(complexity_of(source), 2);
    }

    #[test]
    fn boolean_and_or() {
        let source = "\
func foo(a, b, c):
\tif a and b or c:
\t\tprint(\"yes\")
";
        // 1 base + 1 if + 1 and + 1 or = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn match_statement() {
        let source = "\
func foo(x):
\tmatch x:
\t\t1:
\t\t\tprint(\"one\")
\t\t2:
\t\t\tprint(\"two\")
\t\t_:
\t\t\tprint(\"other\")
";
        // 1 base + 3 pattern_section = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn nested_control_flow() {
        let source = "\
func foo(items):
\tfor item in items:
\t\tif item > 0:
\t\t\twhile item > 10:
\t\t\t\titem -= 1
";
        // 1 base + 1 for + 1 if + 1 while = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn no_warning_under_threshold() {
        let source = "\
func simple(x):
\tif x:
\t\tprint(x)
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_over_threshold() {
        // Build a function with complexity > 10
        let source = "\
func complex(a, b, c):
\tif a:
\t\tpass
\tif b:
\t\tpass
\tif c:
\t\tpass
\tfor i in a:
\t\tpass
\tfor j in b:
\t\tpass
\twhile a:
\t\tpass
\twhile b:
\t\tpass
\twhile c:
\t\tpass
\tif a and b:
\t\tpass
\tif a or c:
\t\tpass
";
        // 1 base + 10 (if/for/while) + 2 (and/or) = 13
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "cyclomatic-complexity");
        assert!(diags[0].message.contains("complex"));
        assert!(diags[0].message.contains("13"));
    }

    #[test]
    fn does_not_count_nested_function() {
        // Inner function's complexity should not be added to outer
        let source = "\
func outer():
\tif true:
\t\tpass

func inner():
\tif true:
\t\tif true:
\t\t\tpass
";
        // outer: 1+1=2, inner: 1+1+1=3 — both under threshold
        assert!(check(source).is_empty());
    }

    #[test]
    fn checks_inner_class_functions() {
        // Create a complex function inside an inner class
        let source = "\
class Inner:
\tfunc complex(a, b, c):
\t\tif a:
\t\t\tpass
\t\tif b:
\t\t\tpass
\t\tif c:
\t\t\tpass
\t\tfor i in a:
\t\t\tpass
\t\tfor j in b:
\t\t\tpass
\t\twhile a:
\t\t\tpass
\t\twhile b:
\t\t\tpass
\t\twhile c:
\t\t\tpass
\t\tif a and b:
\t\t\tpass
\t\tif a or c:
\t\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("complex"));
    }

    #[test]
    fn guard_clause_not_penalized() {
        // Guard clause with and/or should not count the boolean ops
        let guard = "\
func guard(event):
\tif not (event is InputEventKey and event.pressed and not event.echo):
\t\treturn
\thandle()
";
        // 1 base + 1 if (guard) = 2 — and/or in guard not counted
        assert_eq!(complexity_of(guard), 2);
    }

    #[test]
    fn guard_clause_same_as_nested() {
        let nested = "\
func nested(event):
\tif event is InputEventKey:
\t\tif event.pressed:
\t\t\tif not event.echo:
\t\t\t\thandle()
";
        let guard = "\
func guard(event):
\tif not (event is InputEventKey and event.pressed and not event.echo):
\t\treturn
\thandle()
";
        // Guard should be <= nested, not more
        assert!(complexity_of(guard) <= complexity_of(nested));
    }

    #[test]
    fn non_guard_if_still_counts_and_or() {
        // Not a guard clause — body has more than just return
        let source = "\
func f(a, b, c):
\tif a and b and c:
\t\tprint(\"yes\")
";
        // 1 base + 1 if + 2 and = 4
        assert_eq!(complexity_of(source), 4);
    }

    #[test]
    fn reports_correct_location() {
        let source = "\
func ok():
\tpass

func complex(a, b, c):
\tif a:
\t\tpass
\tif b:
\t\tpass
\tif c:
\t\tpass
\tfor i in a:
\t\tpass
\tfor j in b:
\t\tpass
\twhile a:
\t\tpass
\twhile b:
\t\tpass
\twhile c:
\t\tpass
\tif a and b:
\t\tpass
\tif a or c:
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3); // 0-indexed, 4th line
    }
}
