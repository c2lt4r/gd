use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::type_inference::{InferredType, infer_expression_type_with_project};
use crate::core::workspace_index::ProjectIndex;

pub struct VariantInference;

impl LintRule for VariantInference {
    fn name(&self) -> &'static str {
        "variant-inference"
    }

    fn category(&self) -> LintCategory {
        LintCategory::TypeSafety
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _tree: &Tree, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_project(
        &self,
        tree: &Tree,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        check_node(tree.root_node(), source, symbols, project, &mut diags);
        diags
    }
}

fn check_node(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    if node.kind() == "variable_statement" {
        check_variable(node, source, symbols, project, diags);
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            check_node(cursor.node(), source, symbols, project, diags);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
}

fn check_variable(
    node: Node,
    source: &str,
    symbols: &SymbolTable,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Check if this uses := (inferred type via inferred_type node)
    let is_inferred = node
        .child_by_field_name("type")
        .is_some_and(|t| t.kind() == "inferred_type");
    if !is_inferred {
        return;
    }

    let Some(value) = node.child_by_field_name("value") else {
        return;
    };

    // Godot's parser treats `in`/`not in` as returning Variant at the type
    // level (even though it's always bool at runtime), so `:=` fails.
    if is_in_operator(&value, source) {
        let name_node = node.child_by_field_name("name");
        let var_name = name_node
            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
            .unwrap_or("?");
        diags.push(LintDiagnostic {
            rule: "variant-inference",
            message: format!(
                "`:=` cannot infer type from `in` operator for `{var_name}` — use `var {var_name}: bool = ...`"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
        return;
    }

    // Use the project-aware inference engine for cross-file resolution.
    let inferred = infer_expression_type_with_project(&value, source, symbols, project);
    let is_variant = matches!(inferred, Some(InferredType::Variant) | None);
    if !is_variant {
        return;
    }

    let name_node = node.child_by_field_name("name");
    let var_name = name_node
        .and_then(|n| n.utf8_text(source.as_bytes()).ok())
        .unwrap_or("?");

    diags.push(LintDiagnostic {
        rule: "variant-inference",
        message: format!(
            "`:=` infers `Variant` for `{var_name}` — use an explicit type annotation"
        ),
        severity: Severity::Warning,
        line: node.start_position().row,
        column: node.start_position().column,
        end_column: None,
        fix: None,
        context_lines: None,
    });
}

/// Check if the value expression is (or contains at the top level) an `in` or `not in` operator.
fn is_in_operator(node: &Node, source: &str) -> bool {
    if node.kind() == "binary_operator" {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.is_named() {
                    continue;
                }
                let text = child.utf8_text(source.as_bytes()).unwrap_or("");
                if text == "in" {
                    return true;
                }
            }
        }
    }
    // Handle `not X in Y` which parses as unary(not, binary(in))
    if node.kind() == "unary_operator" {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i)
                && child.kind() == "binary_operator"
                && is_in_operator(&child, source)
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{parser, symbol_table, workspace_index};
    use std::path::PathBuf;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        let root = PathBuf::from("/test_project");
        let project = workspace_index::build_from_sources(&root, &[], &[]);
        VariantInference.check_with_project(&tree, source, &config, &symbols, &project)
    }

    fn check_with_files(source: &str, project_files: &[(&str, &str)]) -> Vec<LintDiagnostic> {
        let root = PathBuf::from("/test_project");
        let file_entries: Vec<(PathBuf, &str)> = project_files
            .iter()
            .map(|(name, src)| (root.join(name), *src))
            .collect();
        let project = workspace_index::build_from_sources(&root, &file_entries, &[]);

        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        VariantInference.check_with_project(&tree, source, &config, &symbols, &project)
    }

    #[test]
    fn detects_dict_subscript() {
        let source = "var dict := {}\nfunc f():\n\tvar x := dict[\"key\"]\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Variant"));
    }

    #[test]
    fn no_warning_explicit_type() {
        let source = "var dict := {}\nfunc f():\n\tvar x: String = dict[\"key\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_simple_assignment() {
        let source = "func f():\n\tvar x := 42\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_regular_equals() {
        let source = "func f():\n\tvar x = dict[\"key\"]\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_constructor() {
        let source = "func f():\n\tvar v := Vector2(1, 2)\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_in_operator() {
        let source = "const ACTIONS: Array[String] = [\"move\"]\nfunc f(action: String):\n\tvar is_movement := action in ACTIONS\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("`in` operator"));
    }

    #[test]
    fn detects_not_in_operator() {
        let source = "func f(x: String, arr: Array):\n\tvar missing := not x in arr\n";
        let diags = check(source);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_in_with_explicit_type() {
        let source = "func f(x: String, arr: Array):\n\tvar found: bool = x in arr\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!VariantInference.default_enabled());
    }

    #[test]
    fn no_warning_preload_tscn() {
        let source = "func f():\n\tvar scene := preload(\"res://scene.tscn\")\n";
        let diags = check(source);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_cross_file_method() {
        let source = "\
extends BaseEnemy
func f():
\tvar h := get_health()
";
        let diags = check_with_files(
            source,
            &[(
                "base.gd",
                "class_name BaseEnemy\nextends Node\nfunc get_health() -> int:\n\treturn 100\n",
            )],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_property_access_on_typed_var() {
        let source = "\
var node: Node2D
func f():
\tvar x := node.position
";
        let diags = check(source);
        assert!(diags.is_empty());
    }
}
