use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PreferDollarSyntax;

impl LintRule for PreferDollarSyntax {
    fn name(&self) -> &'static str {
        "prefer-dollar-syntax"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
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
    // Bare call: get_node("literal")
    if node.kind() == "call" {
        check_bare_call(node, source, diags);
    }

    // Method call: self.get_node("literal") → attribute node
    if node.kind() == "attribute" {
        check_attribute_call(node, source, diags);
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

fn check_bare_call(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();

    // First child should be identifier "get_node"
    let callee = node
        .children(&mut node.walk())
        .find(|c| c.kind() == "identifier")
        .and_then(|n| n.utf8_text(src).ok());

    if callee != Some("get_node") {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };

    // Must have exactly one argument that is a string literal
    let mut named_children = Vec::new();
    let mut cursor = args.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
                named_children.push(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if named_children.len() != 1 || named_children[0].kind() != "string" {
        return;
    }

    let string_node = named_children[0];
    let string_text = &source[string_node.byte_range()];

    // Extract the path from the quoted string
    let Some(path) = extract_string_content(string_text) else {
        return;
    };

    if path.is_empty() {
        return;
    }

    let replacement = dollar_syntax_for(path);

    diags.push(LintDiagnostic {
        rule: "prefer-dollar-syntax",
        message: format!("use `{replacement}` instead of `get_node({string_text})`"),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: Some(node.end_position().column),
        fix: Some(Fix {
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            replacement,
        }),
        context_lines: None,
    });
}

fn check_attribute_call(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let src = source.as_bytes();

    // Check for self.get_node("literal") pattern
    // attribute node has children: [identifier("self"), attribute_call]
    let mut has_self = false;
    let mut attr_call = None;

    for child in node.children(&mut node.walk()) {
        if child.kind() == "identifier" && child.utf8_text(src).ok() == Some("self") {
            has_self = true;
        }
        if child.kind() == "attribute_call" {
            attr_call = Some(child);
        }
    }

    if !has_self {
        return;
    }

    let Some(call_node) = attr_call else {
        return;
    };

    // Check if the method name is get_node
    let method = call_node
        .children(&mut call_node.walk())
        .find(|c| c.kind() == "identifier")
        .and_then(|n| n.utf8_text(src).ok());

    if method != Some("get_node") {
        return;
    }

    let Some(args) = call_node.child_by_field_name("arguments") else {
        return;
    };

    // Must have exactly one argument that is a string literal
    let mut named_children = Vec::new();
    let mut cursor = args.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
                named_children.push(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if named_children.len() != 1 || named_children[0].kind() != "string" {
        return;
    }

    let string_node = named_children[0];
    let string_text = &source[string_node.byte_range()];

    let Some(path) = extract_string_content(string_text) else {
        return;
    };

    if path.is_empty() {
        return;
    }

    let replacement = dollar_syntax_for(path);
    let full_text = &source[node.byte_range()];

    diags.push(LintDiagnostic {
        rule: "prefer-dollar-syntax",
        message: format!("use `{replacement}` instead of `{full_text}`"),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: Some(node.end_position().column),
        fix: Some(Fix {
            byte_start: node.start_byte(),
            byte_end: node.end_byte(),
            replacement,
        }),
        context_lines: None,
    });
}

/// Extract string content from a quoted string like `"Sprite2D"` or `'Sprite2D'`.
fn extract_string_content(s: &str) -> Option<&str> {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

/// Build the `$` syntax for a given path.
/// Simple identifiers: `$Sprite2D`
/// Paths with `/` or spaces or special chars: `$"Player/Camera"`
fn dollar_syntax_for(path: &str) -> String {
    let needs_quotes = path.contains('/')
        || path.contains(' ')
        || path.contains('.')
        || path.contains(':')
        || path.contains('%');
    if needs_quotes {
        format!("$\"{path}\"")
    } else {
        format!("${path}")
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
        PreferDollarSyntax.check(&file, source, &config)
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
    fn detects_bare_get_node() {
        let source = "func f():\n\tvar node = get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$Sprite2D"));
    }

    #[test]
    fn detects_self_get_node() {
        let source = "func f():\n\tvar node = self.get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$Sprite2D"));
    }

    #[test]
    fn uses_quoted_dollar_for_paths() {
        let source = "func f():\n\tvar node = get_node(\"Player/Camera\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$\"Player/Camera\""));
    }

    #[test]
    fn uses_quoted_dollar_for_spaces() {
        let source = "func f():\n\tvar node = get_node(\"My Node\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("$\"My Node\""));
    }

    #[test]
    fn no_warning_for_variable_argument() {
        let source = "func f(path):\n\tvar node = get_node(path)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_expression_argument() {
        let source = "func f():\n\tvar node = get_node(\"a\" + \"b\")\n";
        // The argument is a binary_operator, not a string
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_for_get_node_or_null() {
        let source = "func f():\n\tvar node = get_node_or_null(\"Sprite2D\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_bare_get_node() {
        let source = "func f():\n\tvar node = get_node(\"Sprite2D\")\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("$Sprite2D"));
        assert!(!fixed.contains("get_node"));
    }

    #[test]
    fn fix_self_get_node() {
        let source = "func f():\n\tvar node = self.get_node(\"Sprite2D\")\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("$Sprite2D"));
        assert!(!fixed.contains("self.get_node"));
    }

    #[test]
    fn fix_path_with_slash() {
        let source = "func f():\n\tvar node = get_node(\"Player/Camera\")\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("$\"Player/Camera\""));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!PreferDollarSyntax.default_enabled());
    }
}
