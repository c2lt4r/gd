use tree_sitter::{Node, Tree};

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::workspace_index::ProjectIndex;

pub struct ShadowedVariableBaseClass;

impl LintRule for ShadowedVariableBaseClass {
    fn name(&self) -> &'static str {
        "shadowed-variable-base-class"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Style
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

        let Some(ref extends) = symbols.extends else {
            return diags;
        };

        // Collect base class variable names (user-defined ancestors)
        let base_vars = project.all_variables(extends);
        if base_vars.is_empty() {
            return diags;
        }

        let base_names: Vec<&str> = base_vars.iter().map(|v| v.name.as_str()).collect();

        // Check function bodies for local variables that shadow base class members
        check_functions(tree.root_node(), source, &base_names, extends, &mut diags);

        diags
    }
}

fn check_functions(
    node: Node,
    source: &str,
    base_names: &[&str],
    extends: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    if matches!(
        node.kind(),
        "function_definition" | "constructor_definition"
    ) {
        check_function_body(&node, source, base_names, extends, diags);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_functions(child, source, base_names, extends, diags);
    }
}

fn check_function_body(
    func_node: &Node,
    source: &str,
    base_names: &[&str],
    extends: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    let Some(body) = func_node.child_by_field_name("body") else {
        return;
    };

    check_body_for_shadows(body, source, base_names, extends, diags);
}

fn check_body_for_shadows(
    node: Node,
    source: &str,
    base_names: &[&str],
    extends: &str,
    diags: &mut Vec<LintDiagnostic>,
) {
    if node.kind() == "variable_statement"
        && let Some(name_node) = node.child_by_field_name("name")
        && let Ok(var_name) = name_node.utf8_text(source.as_bytes())
        && base_names.contains(&var_name)
    {
        diags.push(LintDiagnostic {
            rule: "shadowed-variable-base-class",
            message: format!(
                "local variable `{var_name}` shadows a member of base class `{extends}`"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }

    // Also check for-loop iterators
    if node.kind() == "for_statement"
        && let Some(iter_node) = node.named_child(0)
        && iter_node.kind() == "identifier"
        && let Ok(var_name) = iter_node.utf8_text(source.as_bytes())
        && base_names.contains(&var_name)
    {
        diags.push(LintDiagnostic {
            rule: "shadowed-variable-base-class",
            message: format!(
                "loop variable `{var_name}` shadows a member of base class `{extends}`"
            ),
            severity: Severity::Warning,
            line: node.start_position().row,
            column: node.start_position().column,
            end_column: None,
            fix: None,
            context_lines: None,
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        check_body_for_shadows(child, source, base_names, extends, diags);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::workspace_index;
    use crate::core::{parser, symbol_table};
    use std::path::PathBuf;

    fn check_with_project(source: &str, project_files: &[(&str, &str)]) -> Vec<LintDiagnostic> {
        let root = PathBuf::from("/test_project");
        let file_entries: Vec<(PathBuf, &str)> = project_files
            .iter()
            .map(|(name, src)| (root.join(name), *src))
            .collect();
        let project = workspace_index::build_from_sources(&root, &file_entries, &[]);

        let tree = parser::parse(source).unwrap();
        let symbols = symbol_table::build(&tree, source);
        let config = LintConfig::default();
        ShadowedVariableBaseClass.check_with_project(&tree, source, &config, &symbols, &project)
    }

    #[test]
    fn detects_shadowed_variable() {
        let source = "\
extends BaseEnemy
func f():
\tvar health = 50
";
        let diags = check_with_project(
            source,
            &[(
                "base.gd",
                "class_name BaseEnemy\nextends Node\nvar health: int = 100\n",
            )],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("health"));
        assert!(diags[0].message.contains("BaseEnemy"));
    }

    #[test]
    fn no_warning_for_unique_var() {
        let source = "\
extends BaseEnemy
func f():
\tvar score = 50
";
        let diags = check_with_project(
            source,
            &[(
                "base.gd",
                "class_name BaseEnemy\nextends Node\nvar health: int = 100\n",
            )],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_for_loop_shadow() {
        let source = "\
extends BaseEnemy
func f():
\tfor health in range(10):
\t\tpass
";
        let diags = check_with_project(
            source,
            &[(
                "base.gd",
                "class_name BaseEnemy\nextends Node\nvar health: int = 100\n",
            )],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("health"));
    }

    #[test]
    fn no_warning_without_extends() {
        let source = "\
func f():
\tvar health = 50
";
        let diags = check_with_project(
            source,
            &[(
                "base.gd",
                "class_name BaseEnemy\nextends Node\nvar health: int = 100\n",
            )],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!ShadowedVariableBaseClass.default_enabled());
    }
}
