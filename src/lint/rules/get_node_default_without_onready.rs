use crate::core::gd_ast::{GdDecl, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct GetNodeDefaultWithoutOnready;

impl LintRule for GetNodeDefaultWithoutOnready {
    fn name(&self) -> &'static str {
        "get-node-default-without-onready"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_decls(&file.declarations, &mut diags);
        diags
    }
}

fn check_decls(decls: &[GdDecl<'_>], diags: &mut Vec<LintDiagnostic>) {
    for decl in decls {
        if let GdDecl::Var(var) = decl {
            let Some(value) = &var.value else { continue };
            if !uses_get_node(value) {
                continue;
            }
            let has_onready = var.annotations.iter().any(|a| a.name == "onready");
            if !has_onready {
                diags.push(LintDiagnostic {
                    rule: "get-node-default-without-onready",
                    message: format!(
                        "`{}` uses `$`/`get_node()` as default but lacks `@onready` — node tree isn't ready at variable initialization time",
                        var.name
                    ),
                    severity: Severity::Error,
                    line: var.node.start_position().row,
                    column: var.node.start_position().column,
                    end_column: None,
                    fix: None,
                    context_lines: None,
                });
            }
        }
        if let GdDecl::Class(class) = decl {
            check_decls(&class.declarations, diags);
        }
    }
}

fn uses_get_node(expr: &GdExpr<'_>) -> bool {
    match expr {
        GdExpr::GetNode { .. } => true,
        GdExpr::Call { callee, .. } => {
            matches!(callee.as_ref(), GdExpr::Ident { name: "get_node", .. })
        }
        // Recurse into property access (e.g. $Sprite.texture)
        GdExpr::PropertyAccess { receiver, .. }
        | GdExpr::MethodCall { receiver, .. } => uses_get_node(receiver),
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
        GetNodeDefaultWithoutOnready.check(&file, source, &config)
    }

    #[test]
    fn detects_dollar_without_onready() {
        let source = "var sprite = $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("sprite"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn detects_get_node_call_without_onready() {
        let source = "var sprite = get_node(\"Sprite2D\")\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("sprite"));
    }

    #[test]
    fn no_warning_with_onready() {
        let source = "@onready var sprite = $Sprite2D\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_normal_default() {
        let source = "var health: int = 100\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_no_default() {
        let source = "var sprite: Node\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!GetNodeDefaultWithoutOnready.default_enabled());
    }
}
