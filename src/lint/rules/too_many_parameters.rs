use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct TooManyParameters;

impl LintRule for TooManyParameters {
    fn name(&self) -> &'static str {
        "too-many-parameters"
    }

    fn check(&self, tree: &Tree, source: &str, config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();
        let max_params = config
            .rules
            .get("too-many-parameters")
            .and_then(|r| r.max_params)
            .unwrap_or(config.max_function_params);
        check_node(root, source, max_params, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, max_params: usize, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "function_definition" {
        let count = param_count(node);
        if count > max_params {
            let func_name = node
                .child_by_field_name("name")
                .map(|n| &source[n.byte_range()])
                .unwrap_or("<unknown>");
            diags.push(LintDiagnostic {
                rule: "too-many-parameters",
                message: format!(
                    "function `{}` has {} parameters (max {})",
                    func_name, count, max_params
                ),
                severity: Severity::Warning,
                line: node.start_position().row,
                column: node.start_position().column,
                fix: None,
                end_column: None,
            });
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, max_params, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn param_count(func: Node) -> usize {
    let Some(params) = func.child_by_field_name("parameters") else {
        return 0;
    };

    let mut count = 0;
    let mut cursor = params.walk();
    if cursor.goto_first_child() {
        loop {
            if matches!(
                cursor.node().kind(),
                "identifier" | "typed_parameter" | "default_parameter" | "typed_default_parameter"
            ) {
                count += 1;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        TooManyParameters.check(&tree, source, &config)
    }

    #[test]
    fn no_warning_under_limit() {
        let source = "func foo(a, b, c):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_at_limit() {
        let source = "func foo(a, b, c, d, e):\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn warns_over_limit() {
        let source = "func foo(a, b, c, d, e, f):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "too-many-parameters");
        assert!(diags[0].message.contains("6 parameters"));
    }

    #[test]
    fn warns_with_typed_params() {
        let source =
            "func setup(a: int, b: float, c: String, d: bool, e: Array, f: Dictionary):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("6 parameters"));
    }

    #[test]
    fn warns_with_default_params() {
        let source = "func init(a, b, c, d = 1, e = 2, f = 3):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn warns_with_typed_default_params() {
        let source =
            "func create(a: int, b: int, c: int, d: int = 0, e: int = 0, f: int = 0):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_zero_params() {
        let source = "func foo():\n\tpass\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn checks_inner_class_functions() {
        let source = "\
class Inner:
\tfunc method(a, b, c, d, e, f, g):
\t\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("method"));
    }

    #[test]
    fn multiple_functions() {
        let source = "\
func ok(a, b):
\tpass

func bad(a, b, c, d, e, f):
\tpass

func also_ok(x):
\tpass
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("bad"));
    }

    #[test]
    fn reports_correct_function_name() {
        let source = "func my_complex_function(a, b, c, d, e, f):\n\tpass\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("my_complex_function"));
    }
}
