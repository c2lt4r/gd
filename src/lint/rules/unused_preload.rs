use std::collections::{HashMap, HashSet};
use tree_sitter::Node;
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

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Collect all `var X = preload(...)` or `var X = load(...)` declarations via typed AST
        let mut preloads: HashMap<String, (usize, usize, usize)> = HashMap::new();
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Var(var) = decl {
                let is_preload = matches!(&var.value, Some(GdExpr::Preload { .. }));
                let is_load = matches!(
                    &var.value,
                    Some(GdExpr::Call { callee, .. })
                        if matches!(callee.as_ref(), GdExpr::Ident { name: "load", .. })
                );
                if (is_preload || is_load) && !var.name.is_empty() && !var.name.starts_with('_') {
                    let name_pos = var.node.child_by_field_name("name");
                    let col = name_pos.map_or(var.node.start_position().column, |n| {
                        n.start_position().column
                    });
                    let line = name_pos.map_or(var.node.start_position().row, |n| {
                        n.start_position().row
                    });
                    preloads.insert(
                        var.name.to_string(),
                        (line, col, col + var.name.len()),
                    );
                }
            }
        });

        if preloads.is_empty() {
            return diags;
        }

        // Collect all identifier references across the entire file (CST for full tree scan)
        let mut references: HashSet<String> = HashSet::new();
        collect_all_references(file.node, source.as_bytes(), &mut references);

        // Report preloaded vars that are never referenced elsewhere
        for (name, (line, col, end_col)) in &preloads {
            if !references.contains(name.as_str()) {
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

/// Collect all identifier references, skipping declaration name positions.
fn collect_all_references(node: Node, src: &[u8], refs: &mut HashSet<String>) {
    match node.kind() {
        "variable_statement" => {
            // Skip the name field (that's the declaration), but check value and type
            if let Some(value) = node.child_by_field_name("value") {
                collect_all_references(value, src, refs);
            }
            if let Some(ty) = node.child_by_field_name("type") {
                collect_all_references(ty, src, refs);
            }
        }
        "identifier" => {
            let name = node.utf8_text(src).unwrap_or("");
            if !name.is_empty() {
                refs.insert(name.to_string());
            }
        }
        _ => {
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    collect_all_references(cursor.node(), src, refs);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
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
