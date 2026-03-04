use gd_core::gd_ast::{self, GdFile, GdStmt};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

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
        gd_ast::visit_stmts(file, &mut |stmt| {
            check_collapsible_if(stmt, source, &mut diags);
        });
        diags
    }
}

fn check_collapsible_if(stmt: &GdStmt<'_>, source: &str, diags: &mut Vec<LintDiagnostic>) {
    let GdStmt::If(gif) = stmt else { return };

    // Outer if must have no else/elif
    if !gif.elif_branches.is_empty() || gif.else_body.is_some() {
        return;
    }

    // Body must contain exactly one statement that is also an if
    if gif.body.len() != 1 {
        return;
    }
    let GdStmt::If(inner) = &gif.body[0] else {
        return;
    };

    // Inner if must also have no else/elif
    if !inner.elif_branches.is_empty() || inner.else_body.is_some() {
        return;
    }

    let outer_cond = &source[gif.condition.node().byte_range()];
    let inner_cond = &source[inner.condition.node().byte_range()];

    let fix = generate_fix(gif.node, &inner.body, outer_cond, inner_cond, source);

    diags.push(LintDiagnostic {
        rule: "collapsible-if",
        message: format!(
            "this `if` can be collapsed into the outer one: `if {outer_cond} and {inner_cond}:`"
        ),
        severity: Severity::Warning,
        line: gif.node.start_position().row,
        column: gif.node.start_position().column,
        end_column: None,
        fix,
        context_lines: None,
    });
}

fn generate_fix(
    outer_if: tree_sitter::Node<'_>,
    inner_body: &[GdStmt<'_>],
    outer_cond: &str,
    inner_cond: &str,
    source: &str,
) -> Option<Fix> {
    let source_bytes = source.as_bytes();
    let first_stmt = inner_body.first()?;
    let last_stmt = inner_body.last()?;

    // Find start of the line containing the outer if
    let mut line_start = outer_if.start_byte();
    while line_start > 0 && source_bytes[line_start - 1] != b'\n' {
        line_start -= 1;
    }
    let indent = &source[line_start..outer_if.start_byte()];

    // Get byte range of body statements (line-start of first to end of last)
    let mut body_start = first_stmt.node().start_byte();
    while body_start > 0 && source_bytes[body_start - 1] != b'\n' {
        body_start -= 1;
    }
    let last_end = last_stmt.node().end_byte();
    let body_end = if last_end < source_bytes.len() && source_bytes[last_end] == b'\n' {
        last_end + 1
    } else {
        last_end
    };

    let raw_body = &source[body_start..body_end];
    let outer_body_indent = format!("{indent}\t");
    let first_line = raw_body.lines().next().unwrap_or("");
    let current_indent_len = first_line.len() - first_line.trim_start().len();

    let mut body_lines = String::new();
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
    use gd_core::gd_ast;
    use gd_core::parser;

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
