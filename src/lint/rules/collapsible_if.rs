use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct CollapsibleIf;

impl LintRule for CollapsibleIf {
    fn name(&self) -> &'static str {
        "collapsible-if"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(file.node, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "if_statement" {
        check_if(node, source, diags);
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

fn check_if(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    // Outer if must have no else/elif
    if has_else_or_elif(node) {
        return;
    }

    let Some(body) = node.child_by_field_name("body") else {
        return;
    };

    // Body must contain exactly one named non-comment child that is an if_statement
    let Some(inner_if) = single_named_non_comment_child(body) else {
        return;
    };
    if inner_if.kind() != "if_statement" {
        return;
    }

    // Inner if must also have no else/elif
    if has_else_or_elif(inner_if) {
        return;
    }

    let Some(outer_cond) = node.child_by_field_name("condition") else {
        return;
    };
    let Some(inner_cond) = inner_if.child_by_field_name("condition") else {
        return;
    };

    let outer_text = &source[outer_cond.byte_range()];
    let inner_text = &source[inner_cond.byte_range()];

    let fix = generate_fix(node, inner_if, outer_text, inner_text, source);

    diags.push(LintDiagnostic {
        rule: "collapsible-if",
        message: format!(
            "this `if` can be collapsed into the outer one: `if {outer_text} and {inner_text}:`"
        ),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix,
        context_lines: None,
    });
}

fn has_else_or_elif(if_node: Node) -> bool {
    let mut cursor = if_node.walk();
    if !cursor.goto_first_child() {
        return false;
    }
    loop {
        let kind = cursor.node().kind();
        if kind == "else_clause" || kind == "elif_clause" {
            return true;
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    false
}

fn single_named_non_comment_child(body: Node) -> Option<Node> {
    let mut result = None;
    let mut count = 0;
    let mut cursor = body.walk();
    if !cursor.goto_first_child() {
        return None;
    }
    loop {
        let child = cursor.node();
        if child.is_named() && child.kind() != "comment" {
            count += 1;
            if count > 1 {
                return None;
            }
            result = Some(child);
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
    result
}

fn generate_fix(
    outer_if: Node,
    inner_if: Node,
    outer_cond: &str,
    inner_cond: &str,
    source: &str,
) -> Option<Fix> {
    let source_bytes = source.as_bytes();

    // Find start of the line containing the outer if
    let mut line_start = outer_if.start_byte();
    while line_start > 0 && source_bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let indent = &source[line_start..outer_if.start_byte()];

    // Get the inner if's body
    let inner_body = inner_if.child_by_field_name("body")?;

    // Collect all named children of the inner body
    let mut body_lines = String::new();
    let mut cursor = inner_body.walk();
    if cursor.goto_first_child() {
        // Get the raw text of the inner body's children
        let first_child = cursor.node();
        // Find the first named child to get the start
        let mut first_named_start = None;
        let mut last_end = inner_body.end_byte();
        {
            let mut c2 = inner_body.walk();
            if c2.goto_first_child() {
                loop {
                    let child = c2.node();
                    if child.is_named() && child.kind() != "comment" {
                        if first_named_start.is_none() {
                            // Find start of this child's line
                            let mut ls = child.start_byte();
                            while ls > 0 && source_bytes[ls - 1] != b'\n' {
                                ls -= 1;
                            }
                            first_named_start = Some(ls);
                        }
                        last_end = child.end_byte();
                    }
                    if !c2.goto_next_sibling() {
                        break;
                    }
                }
            }
        }

        let body_start = first_named_start.unwrap_or(first_child.start_byte());
        // Include trailing newline if present
        let body_end = if last_end < source_bytes.len() && source_bytes[last_end] == b'\n' {
            last_end + 1
        } else {
            last_end
        };

        let raw_body = &source[body_start..body_end];

        // Determine outer body indent (one level deeper than outer if)
        let outer_body_indent = format!("{indent}\t");
        // Determine inner body indent (current indent of the inner body content)
        let first_line = raw_body.lines().next().unwrap_or("");
        let current_indent_len = first_line.len() - first_line.trim_start().len();

        // Re-indent: replace current indent with outer_body_indent
        for line in raw_body.lines() {
            if line.trim().is_empty() {
                body_lines.push('\n');
            } else {
                let stripped = if line.len() >= current_indent_len {
                    &line[current_indent_len..]
                } else {
                    line.trim_start()
                };
                body_lines.push_str(&outer_body_indent);
                body_lines.push_str(stripped);
                body_lines.push('\n');
            }
        }
    }

    // Include trailing newline in replaced range if present
    let mut end = outer_if.end_byte();
    if end < source_bytes.len() && source_bytes[end] == b'\n' {
        end += 1;
    }

    Some(Fix {
        byte_start: line_start,
        byte_end: end,
        replacement: format!("{indent}if {outer_cond} and {inner_cond}:\n{body_lines}"),
    })
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
        CollapsibleIf.check(&file, source, &config)
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
    fn detects_collapsible_if() {
        let source = "func f():\n\tif a:\n\t\tif b:\n\t\t\tdo_something()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("if a and b:"));
    }

    #[test]
    fn no_warning_inner_else() {
        let source =
            "func f():\n\tif a:\n\t\tif b:\n\t\t\tdo_something()\n\t\telse:\n\t\t\tother()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_outer_else() {
        let source = "func f():\n\tif a:\n\t\tif b:\n\t\t\tdo_something()\n\telse:\n\t\tother()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_extra_statements() {
        let source = "func f():\n\tif a:\n\t\tsetup()\n\t\tif b:\n\t\t\tdo_something()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_inner_elif() {
        let source =
            "func f():\n\tif a:\n\t\tif b:\n\t\t\tdo_something()\n\t\telif c:\n\t\t\tother()\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_collapses_conditions() {
        let source = "func f():\n\tif a:\n\t\tif b:\n\t\t\tdo_something()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(fixed, "func f():\n\tif a and b:\n\t\tdo_something()\n");
    }

    #[test]
    fn fix_preserves_multi_line_body() {
        let source = "func f():\n\tif a:\n\t\tif b:\n\t\t\tdo_something()\n\t\t\tdo_more()\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert_eq!(
            fixed,
            "func f():\n\tif a and b:\n\t\tdo_something()\n\t\tdo_more()\n"
        );
    }

    #[test]
    fn default_enabled() {
        assert!(CollapsibleIf.default_enabled());
    }
}
