use crate::core::gd_ast::{self, GdDecl, GdExpr, GdFile, GdStmt, GdVar};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
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

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        source: &str,
        _config: &LintConfig,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();
        // Check class-level var declarations
        gd_ast::visit_decls(file, &mut |decl| {
            if let GdDecl::Var(var) = decl {
                check_var(var, source, file, project, &mut diags);
            }
        });
        // Check function-local var declarations
        gd_ast::visit_stmts(file, &mut |stmt| {
            if let GdStmt::Var(var) = stmt {
                check_var(var, source, file, project, &mut diags);
            }
        });
        diags
    }
}

fn check_var(
    var: &GdVar,
    source: &str,
    file: &GdFile,
    project: &ProjectIndex,
    diags: &mut Vec<LintDiagnostic>,
) {
    // Only check := (inferred type)
    let Some(ref type_ann) = var.type_ann else { return };
    if !type_ann.is_inferred {
        return;
    }

    let Some(ref value) = var.value else { return };

    // Godot's parser treats `in`/`not in` as returning Variant at the type
    // level (even though it's always bool at runtime), so `:=` fails.
    if is_in_operator(value) {
        diags.push(LintDiagnostic {
            rule: "variant-inference",
            message: format!(
                "`:=` cannot infer type from `in` operator for `{}` — use `var {}: bool = ...`",
                var.name, var.name
            ),
            severity: Severity::Warning,
            line: var.node.start_position().row,
            column: var.node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
        return;
    }

    // Use the project-aware inference engine for cross-file resolution.
    let value_node = value.node();
    let inferred = infer_expression_type_with_project(&value_node, source, file, project);
    let is_variant = matches!(inferred, Some(InferredType::Variant) | None);
    if !is_variant {
        return;
    }

    diags.push(LintDiagnostic {
        rule: "variant-inference",
        message: format!(
            "`:=` infers `Variant` for `{}` — use an explicit type annotation",
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

/// Check if the value expression is (or contains at the top level) an `in` or `not in` operator.
fn is_in_operator(expr: &GdExpr) -> bool {
    match expr {
        GdExpr::BinOp { op, .. } if *op == "in" => true,
        GdExpr::UnaryOp { op, operand, .. } if *op == "not" => is_in_operator(operand),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::gd_ast;
    use crate::core::{parser, workspace_index};
    use std::path::PathBuf;

    fn check(source: &str) -> Vec<LintDiagnostic> {
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        let root = PathBuf::from("/test_project");
        let project = workspace_index::build_from_sources(&root, &[], &[]);
        VariantInference.check_with_project(&file, source, &config, &project)
    }

    fn check_with_files(source: &str, project_files: &[(&str, &str)]) -> Vec<LintDiagnostic> {
        let root = PathBuf::from("/test_project");
        let file_entries: Vec<(PathBuf, &str)> = project_files
            .iter()
            .map(|(name, src)| (root.join(name), *src))
            .collect();
        let project = workspace_index::build_from_sources(&root, &file_entries, &[]);

        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        VariantInference.check_with_project(&file, source, &config, &project)
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
