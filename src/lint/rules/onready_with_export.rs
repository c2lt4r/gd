use crate::core::gd_ast::{GdClass, GdFile, GdVar};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct OnreadyWithExport;

impl LintRule for OnreadyWithExport {
    fn name(&self) -> &'static str {
        "onready-with-export"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(
        &self,
        _file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_symbols(
        &self,
        file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_vars(file.vars(), &mut diags);
        for inner in file.inner_classes() {
            check_inner_class(inner, &mut diags);
        }
        diags
    }
}

fn has_warning_ignore(var: &GdVar, warning: &str) -> bool {
    var.annotations.iter().any(|a| {
        a.name == "warning_ignore"
            && a.args.iter().any(|arg| {
                if let crate::core::gd_ast::GdExpr::StringLiteral { value, .. } = arg {
                    // value includes quotes, e.g. "\"onready_with_export\""
                    value.trim_matches('"') == warning
                } else {
                    false
                }
            })
    })
}

fn check_var(var: &GdVar, diags: &mut Vec<LintDiagnostic>) {
    let has_onready = var.annotations.iter().any(|a| a.name == "onready");
    let has_export = var.annotations.iter().any(|a| a.name == "export");
    if has_onready && has_export && !has_warning_ignore(var, "onready_with_export") {
        diags.push(LintDiagnostic {
            rule: "onready-with-export",
            message: format!(
                "`{}` has both `@onready` and `@export` — `@onready` sets the value after `@export`, making the export useless",
                var.name
            ),
            severity: Severity::Error,
            line: var.node.start_position().row,
            column: 0,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }
}

fn check_vars<'a>(vars: impl Iterator<Item = &'a GdVar<'a>>, diags: &mut Vec<LintDiagnostic>) {
    for var in vars {
        check_var(var, diags);
    }
}

fn check_inner_class(class: &GdClass, diags: &mut Vec<LintDiagnostic>) {
    for decl in &class.declarations {
        if let crate::core::gd_ast::GdDecl::Var(var) = decl {
            check_var(var, diags);
        }
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
        OnreadyWithExport.check_with_symbols(&file, source, &config)
    }

    #[test]
    fn detects_onready_with_export() {
        let source = "@export\n@onready var sprite = $Sprite2D\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("sprite"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn no_warning_export_only() {
        let source = "@export var health: int = 100\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_onready_only() {
        let source = "@onready var sprite = $Sprite2D\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn no_warning_plain_var() {
        let source = "var x: int = 0\n";
        assert!(check(source).is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!OnreadyWithExport.default_enabled());
    }
}
