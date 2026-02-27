use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct AssertAlwaysFalse;

impl LintRule for AssertAlwaysFalse {
    fn name(&self) -> &'static str {
        "assert-always-false"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(file.node, source, &mut diags);
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
        && is_always_falsy(&first_arg, source)
    {
        let arg_text = first_arg.utf8_text(source.as_bytes()).ok().unwrap_or("?");
        let fix = node.parent().map(|stmt| {
            let mut start = stmt.start_byte();
            let mut end = stmt.end_byte();
            while start > 0 && source.as_bytes()[start - 1] == b'\t' {
                start -= 1;
            }
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
            rule: "assert-always-false",
            message: format!("assertion is always false: `assert({arg_text})`"),
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

fn is_always_falsy(node: &Node, source: &str) -> bool {
    match node.kind() {
        "false" | "null" => true,
        "integer" => {
            let text = node.utf8_text(source.as_bytes()).ok().unwrap_or("1");
            text == "0"
        }
        "float" => {
            let text = node.utf8_text(source.as_bytes()).ok().unwrap_or("1.0");
            text == "0.0" || text == "0." || text == ".0"
        }
        "string" => {
            // Empty string `""` or `''` is falsy
            let text = node.utf8_text(source.as_bytes()).ok().unwrap_or("\"x\"");
            text == "\"\"" || text == "''"
        }
        _ => false,
    }
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
        AssertAlwaysFalse.check(&file, source, &config)
    }

    #[test]
    fn assert_false() {
        let source = "func f():\n\tassert(false)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("always false"));
    }

    #[test]
    fn assert_null() {
        let source = "func f():\n\tassert(null)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_zero() {
        let source = "func f():\n\tassert(0)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_empty_string() {
        let source = "func f():\n\tassert(\"\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn assert_true_ok() {
        let source = "func f():\n\tassert(true)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assert_variable_ok() {
        let source = "func f():\n\tassert(x)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn assert_call_ok() {
        let source = "func f():\n\tassert(is_valid())\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn autofix_removes_line() {
        let source = "func f():\n\tassert(false)\n\tprint(\"hi\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().expect("should have auto-fix");
        let fixed = format!("{}{}", &source[..fix.byte_start], &source[fix.byte_end..]);
        assert_eq!(fixed, "func f():\n\tprint(\"hi\")\n");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!AssertAlwaysFalse.default_enabled());
    }
}
