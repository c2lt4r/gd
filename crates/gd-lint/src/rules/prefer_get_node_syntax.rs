use gd_core::gd_ast::{self, GdExpr, GdFile};

use super::{Fix, LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct PreferGetNodeSyntax;

impl LintRule for PreferGetNodeSyntax {
    fn name(&self) -> &'static str {
        "prefer-get-node-syntax"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::GetNode { node, path } = expr
                && path.starts_with('$')
            {
                let inner = &path[1..]; // Strip leading $
                let inner = if inner.starts_with('"') && inner.ends_with('"') {
                    &inner[1..inner.len() - 1]
                } else {
                    inner
                };

                if inner.is_empty() {
                    return;
                }

                let replacement = format!("get_node(\"{inner}\")");

                diags.push(LintDiagnostic {
                    rule: "prefer-get-node-syntax",
                    message: format!("use `{replacement}` instead of `{path}`"),
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

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        PreferGetNodeSyntax.check(&file, source, &config)
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
    fn detects_simple_dollar() {
        let source = "func f():\n\tvar node = $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node(\"Sprite2D\")"));
    }

    #[test]
    fn detects_quoted_dollar() {
        let source = "func f():\n\tvar node = $\"Player/Camera\"\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("get_node(\"Player/Camera\")"));
    }

    #[test]
    fn no_warning_for_get_node_call() {
        let source = "func f():\n\tvar node = get_node(\"Sprite2D\")\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn fix_simple_dollar() {
        let source = "func f():\n\tvar node = $Sprite2D\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("get_node(\"Sprite2D\")"));
        assert!(!fixed.contains('$'));
    }

    #[test]
    fn fix_quoted_dollar() {
        let source = "func f():\n\tvar node = $\"Player/Camera\"\n";
        let diags = check(source);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("get_node(\"Player/Camera\")"));
    }

    #[test]
    fn fix_dollar_with_path() {
        let source = "func f():\n\tvar node = $UI/HealthBar\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        let fix = diags[0].fix.as_ref().unwrap();
        let fixed = apply_fix(source, fix);
        assert!(fixed.contains("get_node(\"UI/HealthBar\")"));
    }

    #[test]
    fn opt_in_rule() {
        assert!(!PreferGetNodeSyntax.default_enabled());
    }
}
