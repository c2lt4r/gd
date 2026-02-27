use tree_sitter::Node;
use crate::core::gd_ast::GdFile;

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

    fn check(&self, file: &GdFile<'_>, source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        let root = file.node;
        check_node(root, source, &mut diags);
        diags
    }
}

fn check_node(node: Node, source: &str, diags: &mut Vec<LintDiagnostic>) {
    if node.kind() == "variable_statement" {
        // Check if it's a const (constants are fine)
        if let Some(first) = node.child(0)
            && &source[first.byte_range()] == "const"
        {
            // Constants are ok, skip — but recurse into children
        } else {
            // Check if there's a type annotation
            let has_type = node.child_by_field_name("type").is_some();

            if !has_type {
                // Check if the value is a preload() or load() call
                if let Some(value_node) = node.child_by_field_name("value")
                    && is_preload_or_load_call(&value_node, source)
                {
                    let var_name = if let Some(name_node) = node.child_by_field_name("name") {
                        source[name_node.byte_range()].to_string()
                    } else {
                        "variable".to_string()
                    };

                    diags.push(LintDiagnostic {
                        rule: "preload-type-hint",
                        message: format!(
                            "variable `{var_name}` uses preload/load but has no type hint; consider adding a type annotation"
                        ),
                        severity: Severity::Warning,
                        line: node.start_position().row,
                        column: node.start_position().column,
                        end_column: None,
                        fix: None,
                        context_lines: None,
                    });
                }
            }
        }
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn is_preload_or_load_call(node: &Node, source: &str) -> bool {
    if node.kind() == "call" {
        // Try field name first, fall back to first named child
        // (tree-sitter-gdscript doesn't always set the "function" field for builtins)
        let func_node = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(func) = func_node {
            let func_name = &source[func.byte_range()];
            return func_name == "preload" || func_name == "load";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::LintConfig;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_gdscript::LANGUAGE.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let file = crate::core::gd_ast::convert(&tree, source);
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
