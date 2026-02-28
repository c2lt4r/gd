use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::workspace_index::ProjectIndex;

pub struct OverrideSignatureMismatch;

impl LintRule for OverrideSignatureMismatch {
    fn name(&self) -> &'static str {
        "override-signature-mismatch"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
    }

    fn check(&self, _file: &GdFile<'_>, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        let Some(extends) = file.extends_class() else {
            return diags;
        };

        // Walk the full ancestor chain: [extends, extends-of-extends, ...]
        // extends_chain starts from a class name, so we need to include the
        // immediate parent plus its ancestors.
        let mut ancestors = vec![extends];
        ancestors.extend(project.extends_chain(extends));

        for func in file.funcs() {
            // Check each ancestor for a method with the same name
            for &ancestor in &ancestors {
                let Some(ancestor_symbols) = project.lookup_class(ancestor) else {
                    continue;
                };

                let Some(parent_func) = ancestor_symbols
                    .functions
                    .iter()
                    .find(|f| f.name == func.name)
                else {
                    continue;
                };

                let child_count = func.params.len();
                let parent_count = parent_func.params.len();

                if child_count != parent_count {
                    diags.push(LintDiagnostic {
                        rule: "override-signature-mismatch",
                        message: format!(
                            "method `{}` overrides `{ancestor}` which has {parent_count} parameter{}, but child defines {child_count}",
                            func.name,
                            if parent_count == 1 { "" } else { "s" },
                        ),
                        severity: Severity::Error,
                        line: func.node.start_position().row,
                        column: 0,
                        end_column: None,
                        fix: None,
                        context_lines: None,
                    });
                }

                // Stop at the first ancestor that defines this method
                break;
            }
        }

        diags
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::workspace_index;
    use crate::core::gd_ast;
    use crate::core::parser;
    use std::path::PathBuf;

    fn check_with_project(source: &str, project_files: &[(&str, &str)]) -> Vec<LintDiagnostic> {
        let root = PathBuf::from("/test_project");
        let file_entries: Vec<(PathBuf, &str)> = project_files
            .iter()
            .map(|(name, src)| (root.join(name), *src))
            .collect();
        let project = workspace_index::build_from_sources(&root, &file_entries, &[]);

        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        OverrideSignatureMismatch.check_with_project(&file, source, &config, &project)
    }

    #[test]
    fn detects_fewer_params() {
        let source = "extends Parent\n\n\nfunc process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[(
                "parent.gd",
                "class_name Parent\nextends Node\nfunc process(delta: float, extra: int) -> void:\n\tpass\n",
            )],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("process"));
        assert!(diags[0].message.contains("2 parameters"));
        assert!(diags[0].message.contains("child defines 1"));
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn detects_more_params() {
        let source =
            "extends Parent\n\n\nfunc process(a: float, b: int, c: String) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[(
                "parent.gd",
                "class_name Parent\nextends Node\nfunc process(delta: float) -> void:\n\tpass\n",
            )],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("process"));
        assert!(diags[0].message.contains("1 parameter,"));
        assert!(diags[0].message.contains("child defines 3"));
    }

    #[test]
    fn no_warning_matching_params() {
        let source = "extends Parent\n\n\nfunc process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[(
                "parent.gd",
                "class_name Parent\nextends Node\nfunc process(delta: float) -> void:\n\tpass\n",
            )],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_without_extends() {
        let source = "func process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[(
                "parent.gd",
                "class_name Parent\nextends Node\nfunc process(delta: float, extra: int) -> void:\n\tpass\n",
            )],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_method_not_in_parent() {
        let source = "extends Parent\n\n\nfunc my_custom_func(a: int) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[(
                "parent.gd",
                "class_name Parent\nextends Node\nfunc process(delta: float) -> void:\n\tpass\n",
            )],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_grandparent_mismatch() {
        let source = "extends Child\n\n\nfunc process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[
                (
                    "grandparent.gd",
                    "class_name Grandparent\nextends Node\nfunc process(d: float, e: int) -> void:\n\tpass\n",
                ),
                ("parent.gd", "class_name Child\nextends Grandparent\n"),
            ],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("process"));
        assert!(diags[0].message.contains("Grandparent"));
    }

    #[test]
    fn stops_at_nearest_ancestor() {
        // Parent overrides Grandparent's method with 1 param.
        // Child also has 1 param — should match Parent, not Grandparent.
        let source = "extends Child\n\n\nfunc process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[
                (
                    "grandparent.gd",
                    "class_name Grandparent\nextends Node\nfunc process(d: float, e: int) -> void:\n\tpass\n",
                ),
                (
                    "parent.gd",
                    "class_name Child\nextends Grandparent\nfunc process(d: float) -> void:\n\tpass\n",
                ),
            ],
        );
        // Child has 1 param, nearest ancestor (Child/parent.gd) also has 1 param → no warning
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_mismatch_in_nearest_ancestor() {
        // Parent overrides Grandparent's method with 2 params.
        // Child has 1 param — should compare against Parent (nearest), not Grandparent.
        let source = "extends Mid\n\n\nfunc process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[
                (
                    "grandparent.gd",
                    "class_name Grandparent\nextends Node\nfunc process(d: float) -> void:\n\tpass\n",
                ),
                (
                    "parent.gd",
                    "class_name Mid\nextends Grandparent\nfunc process(d: float, e: int) -> void:\n\tpass\n",
                ),
            ],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Mid"));
    }

    #[test]
    fn parent_not_in_index() {
        // extends an engine class or unknown class — no user-defined parent to check
        let source = "extends Node\n\n\nfunc process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(source, &[]);
        assert!(diags.is_empty());
    }

    #[test]
    fn default_enabled() {
        assert!(OverrideSignatureMismatch.default_enabled());
    }

    #[test]
    fn severity_is_error() {
        let source = "extends Parent\n\n\nfunc process(delta: float) -> void:\n\tpass\n";
        let diags = check_with_project(
            source,
            &[(
                "parent.gd",
                "class_name Parent\nextends Node\nfunc process(delta: float, extra: int) -> void:\n\tpass\n",
            )],
        );
        assert_eq!(diags[0].severity, Severity::Error);
    }

    #[test]
    fn multiple_mismatches() {
        let source = "\
extends Parent

func foo(a: int) -> void:
\tpass

func bar() -> void:
\tpass
";
        let diags = check_with_project(
            source,
            &[(
                "parent.gd",
                "class_name Parent\nextends Node\nfunc foo(a: int, b: int) -> void:\n\tpass\nfunc bar(x: int) -> void:\n\tpass\n",
            )],
        );
        assert_eq!(diags.len(), 2);
    }
}
