use crate::core::gd_ast::{GdDecl, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct PreloadTypeHint;

impl LintRule for PreloadTypeHint {
    fn name(&self) -> &'static str {
        "preload-type-hint"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Performance
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
            // Constants are fine
            if var.is_const {
                continue;
            }
            // Already has type annotation
            if var.type_ann.is_some() {
                continue;
            }
            // Check if the value is a preload() or load() call
            if let Some(value) = &var.value
                && is_preload_or_load(value)
            {
                diags.push(LintDiagnostic {
                    rule: "preload-type-hint",
                    message: format!(
                        "variable `{}` uses preload/load but has no type hint; consider adding a type annotation",
                        var.name
                    ),
                    severity: Severity::Warning,
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

fn is_preload_or_load(expr: &GdExpr<'_>) -> bool {
    match expr {
        GdExpr::Preload { .. } => true,
        GdExpr::Call { callee, .. } => {
            matches!(callee.as_ref(), GdExpr::Ident { name: "load", .. })
        }
        _ => false,
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
        PreloadTypeHint.check(&file, source, &LintConfig::default())
    }

    #[test]
    fn untyped_preload() {
        let diags = check("var my_scene = preload(\"res://scene.tscn\")\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "preload-type-hint");
        assert!(diags[0].message.contains("my_scene"));
    }

    #[test]
    fn untyped_load() {
        let diags = check("var my_script = load(\"res://script.gd\")\n");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("my_script"));
    }

    #[test]
    fn typed_preload_no_warning() {
        assert!(check("var my_scene: PackedScene = preload(\"res://scene.tscn\")\n").is_empty());
    }

    #[test]
    fn const_preload_no_warning() {
        assert!(check("const Scene: PackedScene = preload(\"res://scene.tscn\")\n").is_empty());
    }

    #[test]
    fn regular_var_no_warning() {
        assert!(check("var x = 42\n").is_empty());
    }
}
