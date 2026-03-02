use crate::core::gd_ast::{self, GdExpr, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PreferStringFormat;

impl LintRule for PreferStringFormat {
    fn name(&self) -> &'static str {
        "prefer-string-format"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::BinOp { node, op: "+", .. } = expr {
                // Only trigger on the top-level `+` — skip if parent is also a `+` binary_operator
                if let Some(parent) = node.parent()
                    && parent.kind() == "binary_operator"
                    && parent
                        .child_by_field_name("op")
                        .is_some_and(|o| &source[o.byte_range()] == "+")
                {
                    return;
                }

                let Some(segments) = collect_concat_segments(expr, source) else {
                    return;
                };

                // Must have at least one str() call
                if !segments.iter().any(|s| matches!(s, Segment::StrCall(_))) {
                    return;
                }

                // Build the format string and argument array
                let mut format_parts = String::new();
                let mut args = Vec::new();

                for segment in &segments {
                    match segment {
                        Segment::Literal(text) => {
                            format_parts.push_str(&text.replace('%', "%%"));
                        }
                        Segment::StrCall(expr_text) => {
                            format_parts.push_str("%s");
                            args.push(*expr_text);
                        }
                    }
                }

                let replacement = if args.len() == 1 {
                    format!("\"{format_parts}\" % {}", args[0])
                } else {
                    let args_str = args.join(", ");
                    format!("\"{format_parts}\" % [{args_str}]")
                };

                diags.push(LintDiagnostic {
                    rule: "prefer-string-format",
                    message: format!(
                        "use format string `{replacement}` instead of string concatenation"
                    ),
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
        });
        diags
    }
}

/// A segment of a string concatenation chain.
enum Segment<'a> {
    /// A string literal (content without quotes)
    Literal(&'a str),
    /// A `str(expr)` call — store the inner expression text
    StrCall(&'a str),
}

/// Recursively collect segments from a `+` concatenation chain.
fn collect_concat_segments<'a>(expr: &GdExpr<'a>, source: &'a str) -> Option<Vec<Segment<'a>>> {
    if let GdExpr::BinOp {
        op: "+",
        left,
        right,
        ..
    } = expr
    {
        let mut segments = collect_concat_segments(left, source)?;
        let right_segments = collect_concat_segments(right, source)?;
        segments.extend(right_segments);
        return Some(segments);
    }

    // Single node: must be a string literal or str() call
    parse_single_segment(expr, source).map(|s| vec![s])
}

/// Parse a single expression as either a string literal or a str() call.
fn parse_single_segment<'a>(expr: &GdExpr<'a>, source: &'a str) -> Option<Segment<'a>> {
    // String literal
    if let GdExpr::StringLiteral { value, .. } = expr {
        let content = extract_string_content(value)?;
        return Some(Segment::Literal(content));
    }

    // str() call
    if let GdExpr::Call { callee, args, .. } = expr
        && let GdExpr::Ident { name: "str", .. } = callee.as_ref()
        && args.len() == 1
    {
        let arg_text = &source[args[0].node().byte_range()];
        return Some(Segment::StrCall(arg_text));
    }

    None
}

/// Extract string content from a quoted string like `"text"`.
fn extract_string_content(s: &str) -> Option<&str> {
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        PreferStringFormat.check(&file, source, &config)
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
    fn detects_simple_str_concat() {
        let source = "func f(hp, max_hp):\n\tvar msg = \"HP: \" + str(hp) + \"/\" + str(max_hp)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("%s"));
    }

    #[test]
    fn detects_single_str_call() {
        let source = "func f(name):\n\tvar msg = \"Hello \" + str(name)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("\"Hello %s\" % name"));
    }

    #[test]
    fn no_warning_no_str_call() {
        let source = "func f():\n\tvar msg = \"Hello\" + \" world\"\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_non_string_concat() {
        let source = "func f(a, b):\n\tvar c = a + b\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_mixed_non_str_expressions() {
        // x + str(y) where x is not a string literal
        let source = "func f(x, y):\n\tvar msg = x + str(y)\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_simple_concat() {
        let source = "func f(name):\n\tvar msg = \"Hello \" + str(name)\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("\"Hello %s\" % name"));
    }

    #[test]
    fn fix_multi_part_concat() {
        let source = "func f(hp, max_hp):\n\tvar msg = \"HP: \" + str(hp) + \"/\" + str(max_hp)\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("\"HP: %s/%s\" % [hp, max_hp]"));
    }

    #[test]
    fn only_one_diagnostic_for_chain() {
        let source = "func f(a, b, c):\n\tvar msg = str(a) + \"/\" + str(b) + \"/\" + str(c)\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn single_str_uses_bare_format() {
        // With one argument, use `%` without array brackets
        let source = "func f(name):\n\tvar msg = \"Hello \" + str(name)\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        assert!(!fix.replacement.contains('['));
    }

    #[test]
    fn multiple_str_uses_array_format() {
        let source = "func f(a, b):\n\tvar msg = str(a) + \"/\" + str(b)\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        assert!(fix.replacement.contains('['));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!PreferStringFormat.default_enabled());
    }
}
