use gd_core::gd_ast::{self, GdExpr, GdFile};
use std::collections::HashMap;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;

pub struct DuplicatedLoad;

impl LintRule for DuplicatedLoad {
    fn name(&self) -> &'static str {
        "duplicated-load"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Performance
    }

    fn check(&self, file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Map from load path -> list of (line, col, end_col)
        let mut loads: HashMap<String, Vec<(usize, usize, usize)>> = HashMap::new();

        gd_ast::visit_exprs(file, &mut |expr| {
            let (trimmed, node) = match expr {
                GdExpr::Preload { node, path } => {
                    let t = path.trim_matches('"').trim_matches('\'');
                    (t, *node)
                }
                GdExpr::Call {
                    node, callee, args, ..
                } if matches!(callee.as_ref(), GdExpr::Ident { name: "load", .. }) => {
                    if let Some(GdExpr::StringLiteral { value, .. }) = args.first() {
                        (value.trim_matches('"').trim_matches('\''), *node)
                    } else {
                        return;
                    }
                }
                _ => return,
            };
            if !trimmed.is_empty() {
                loads.entry(trimmed.to_string()).or_default().push((
                    node.start_position().row,
                    node.start_position().column,
                    node.end_position().column,
                ));
            }
        });

        for (path, locations) in &loads {
            if locations.len() > 1 {
                for &(line, col, end_col) in &locations[1..] {
                    diags.push(LintDiagnostic {
                        rule: "duplicated-load",
                        message: format!(
                            "path `{}` is already loaded on line {}",
                            path,
                            locations[0].0 + 1,
                        ),
                        severity: Severity::Warning,
                        line,
                        column: col,
                        end_column: Some(end_col),
                        fix: None,
                        context_lines: None,
                    });
                }
            }
        }

        diags.sort_by_key(|d| (d.line, d.column));
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
        DuplicatedLoad.check(&file, source, &config)
    }

    #[test]
    fn no_duplicates() {
        let source = "\
var a = preload(\"res://a.tscn\")
var b = preload(\"res://b.tscn\")
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn duplicate_preload() {
        let source = "\
var a = preload(\"res://player.tscn\")
var b = preload(\"res://player.tscn\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "duplicated-load");
        assert!(diags[0].message.contains("res://player.tscn"));
        assert!(diags[0].message.contains("line 1"));
        assert_eq!(diags[0].line, 1);
    }

    #[test]
    fn duplicate_load() {
        let source = "\
var a = load(\"res://enemy.gd\")
var b = load(\"res://enemy.gd\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("res://enemy.gd"));
    }

    #[test]
    fn mixed_load_and_preload_same_path() {
        let source = "\
var a = preload(\"res://item.tscn\")
var b = load(\"res://item.tscn\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("res://item.tscn"));
    }

    #[test]
    fn three_duplicates_reports_two() {
        let source = "\
var a = preload(\"res://x.gd\")
var b = preload(\"res://x.gd\")
var c = preload(\"res://x.gd\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn different_paths_no_warning() {
        let source = "\
var a = preload(\"res://a.gd\")
var b = preload(\"res://b.gd\")
var c = load(\"res://c.gd\")
";
        assert!(check(source).is_empty());
    }

    #[test]
    fn load_in_function_body() {
        let source = "\
func setup():
\tvar a = load(\"res://item.gd\")
\tvar b = load(\"res://item.gd\")
";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn single_load_no_warning() {
        let source = "var scene = preload(\"res://main.tscn\")\n";
        assert!(check(source).is_empty());
    }
}
