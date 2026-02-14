use tree_sitter::{Node, Tree};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct AssertAlwaysTrue;

impl LintRule for AssertAlwaysTrue {
    fn name(&self) -> &'static str {
        "assert-always-true"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "call"
        && let Some(func) = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0))
        && func.utf8_text(source.as_bytes()).ok() == Some("assert")
        && let Some(args) = node.child_by_field_name("arguments")
        && let Some(first_arg) = args.named_child(0)
        && is_always_truthy(&first_arg, source)
    {
        let arg_text = first_arg.utf8_text(source.as_bytes()).ok().unwrap_or("?");
        // Fix: delete the entire expression_statement line containing the assert
        let fix = node.parent().map(|stmt| {
            let mut start = stmt.start_byte();
            let mut end = stmt.end_byte();
            // Include leading whitespace on the line
            while start > 0 && source.as_bytes()[start - 1] == b'\t' {
                start -= 1;
            }
            // Include trailing newline
            if end < source.len() && source.as_bytes()[end] == b'\n' {
                end += 1;
            }
            Fix {
                byte_start: start,
                byte_end: end,
                replacement: String::new(),
            }
        });
        diags.push(LintDiagnostic {
            rule: "assert-always-true",
            message: format!("assertion is always true: `assert({arg_text})`"),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix,
            context_lines: None,
        });
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_always_truthy(node: &Node, source: &str) -> bool {
    match node.kind() {
        "true" => true,
        "string" => {
            // Non-empty string is truthy
            let text = node.utf8_text(source.as_bytes()).ok().unwrap_or("");
            // Quoted strings: `"x"` has len >= 3 for non-empty
            text.len() > 2
        }
        "integer" => {
            let text = node.utf8_text(source.as_bytes()).ok().unwrap_or("0");
            text != "0"
        }
        "float" => {
            let text = node.utf8_text(source.as_bytes()).ok().unwrap_or("0.0");
            text != "0.0" && text != "0." && text != ".0"
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        AssertAlwaysTrue.check(&tree, source, &config)
    }

    #[test]
    fn assert_true() {
        let source = "func f():\n\tassert(true)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("always true"));
    }

    #[test]
    fn assert_nonzero_int() {
        let source = "func f():\n\tassert(1)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_nonempty_string() {
        let source = "func f():\n\tassert(\"hello\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_variable_ok() {
        let source = "func f():\n\tassert(x)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assert_false_ok() {
        let source = "func f():\n\tassert(false)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assert_zero_ok() {
        let source = "func f():\n\tassert(0)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_removes_line() {
        let source = "func f():\n\tassert(true)\n\tprint(\"hi\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!("{}{}", &source[..fix.byte_start], &source[fix.byte_end..]);
        assert_eq!(fixed, "func f():\n\tprint(\"hi\")\n");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!AssertAlwaysTrue.default_enabled());
    }
}
