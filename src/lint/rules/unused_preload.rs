use std::collections::{HashMap, HashSet};
use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct UnusedPreload;

impl LintRule for UnusedPreload {
    fn name(&self) -> &'static str {
        "unused-preload"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Collect all `var X = preload(...)` or `var X = load(...)` declarations
        let mut preloads: HashMap<&str, (usize, usize, usize)> = HashMap::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Var(var) = decl {
                let is_preload = matches!(&var.value, Some(GdExpr::Preload { .. }));
                let is_load = matches!(
                    &var.value,
                    Some(GdExpr::Call { callee, .. })
                        if matches!(callee.as_ref(), GdExpr::Ident { name: "load", .. })
                );
                if (is_preload || is_load) && !var.name.is_empty() && !var.name.starts_with('_') {
                    let col = var.name_node.map_or(var.node.start_position().column, |n| {
                        n.start_position().column
                    });
                    let line = var.name_node.map_or(var.node.start_position().row, |n| {
                        n.start_position().row
                    });
                    preloads
                        .entry(var.name)
                        .or_insert((line, col, col + var.name.len()));
                }
            }
        });

        if preloads.is_empty() {
            return diags;
        }

        // Collect all identifier references in expression context
        let mut references: HashSet<&str> = HashSet::new();
        gd_ast::visit_exprs(file, &mut |expr| {
            if let GdExpr::Ident { name, .. } = expr {
                references.insert(name);
            }
        });

        // Report preloaded vars that are never referenced elsewhere
        for (name, (line, col, end_col)) in &preloads {
            if !references.contains(name) {
                diags.push(LintDiagnostic {
                    rule: "unused-preload",
                    message: format!("preloaded variable `{name}` is never used"),
                    severity: Severity::Warning,
                    line: *line,
                    column: *col,
                    end_column: Some(*end_col),
                    fix: None,
                    context_lines: None,
                });
            }
        }

        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::LintConfig;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = crate::core::parser::parse(source).unwrap();
        let file = crate::core::gd_ast::convert(&tree, source);
        UnusedPreload.check(&file, source, &LintConfig::default())
    }

    #[test]
    fn unused_preload_detected() {
        let src =
            "var unused_res = preload(\"res://unused.tscn\")\nfunc _ready() -> void:\n\tpass\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "unused-preload");
        assert!(diags[0].message.contains("unused_res"));
    }

    #[test]
    fn used_preload_no_warning() {
        let src =
            "var scene = preload(\"res://scene.tscn\")\nfunc _ready() -> void:\n\tprint(scene)\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn underscore_prefix_skipped() {
        let src = "var _cached = preload(\"res://cached.tscn\")\n";
        assert!(check(src).is_empty());
    }

    #[test]
    fn unused_load_detected() {
        let src = "var unused_script = load(\"res://script.gd\")\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("unused_script"));
    }
}
