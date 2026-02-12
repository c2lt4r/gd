use std::collections::HashMap;
use tree_sitter::{Node, Tree};

use super::{LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;

pub struct DuplicatedLoad;

impl LintRule for DuplicatedLoad {
    fn name(&self) -> &'static str {
        "duplicated-load"
    }

    fn check(&self, tree: &Tree, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = tree.root_node();

        // Map from load path -> list of (line, col, end_col)
        let mut loads: HashMap<String, Vec<(usize, usize, usize)>> = HashMap::new();
        collect_load_calls(root, source, &mut loads);

        for (path, locations) in &loads {
            if locations.len() > 1 {
                // Report all occurrences after the first
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

        // Sort by line for deterministic output
        diags.sort_by_key(|d| (d.line, d.column));
        diags
    }
}

fn collect_load_calls(
    node: Node,
    source: &str,
    loads: &mut HashMap<String, Vec<(usize, usize, usize)>>,
) {
    if node.kind() == "call"
        && let Some(func) = node.named_child(0)
    {
        let func_name = &source[func.byte_range()];
        if func_name == "load" || func_name == "preload" {
            // Extract the string argument from the arguments list
            if let Some(args) = node.child_by_field_name("arguments") {
                for i in 0..args.named_child_count() {
                    if let Some(arg) = args.named_child(i)
                        && arg.kind() == "string"
                    {
                        let text = arg.utf8_text(source.as_bytes()).unwrap_or("");
                        let path = text.trim_matches('"').trim_matches('\'');
                        if !path.is_empty() {
                            let line = node.start_position().row;
                            let col = node.start_position().column;
                            let end_col = node.end_position().column;
                            loads
                                .entry(path.to_string())
                                .or_default()
                                .push((line, col, end_col));
                        }
                        break;
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            collect_load_calls(cursor.node(), source, loads);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::parser;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let config = LintConfig::default();
        DuplicatedLoad.check(&tree, source, &config)
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
