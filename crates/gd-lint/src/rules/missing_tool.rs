use gd_core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;
use gd_core::workspace_index::ProjectIndex;

pub struct MissingTool;

impl LintRule for MissingTool {
    fn name(&self) -> &'static str {
        "missing-tool"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Godot
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

    fn check_with_project(
        &self,
        file: &GdFile<'_>,
        _source: &str,
        _config: &LintConfig,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        // Only warn if this script doesn't have @tool but a base class does
        if file.is_tool {
            return Vec::new();
        }

        let Some(extends) = file.extends_class() else {
            return Vec::new();
        };

        if project.has_tool_in_chain(extends) {
            vec![LintDiagnostic {
                rule: "missing-tool",
                message: format!(
                    "base class `{extends}` has `@tool` but this script does not — add `@tool` to run in the editor"
                ),
                severity: Severity::Warning,
                line: 0,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            }]
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gd_core::gd_ast;
    use gd_core::parser;
    use gd_core::workspace_index;
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
        MissingTool.check_with_project(&file, source, &config, &project)
    }

    #[test]
    fn detects_missing_tool() {
        let source = "extends ToolBase\nfunc f():\n\tpass\n";
        let diags = check_with_project(
            source,
            &[("base.gd", "@tool\nclass_name ToolBase\nextends Node\n")],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("@tool"));
        assert!(diags[0].message.contains("ToolBase"));
    }

    #[test]
    fn no_warning_when_has_tool() {
        let source = "@tool\nextends ToolBase\nfunc f():\n\tpass\n";
        let diags = check_with_project(
            source,
            &[("base.gd", "@tool\nclass_name ToolBase\nextends Node\n")],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_when_base_not_tool() {
        let source = "extends PlainBase\nfunc f():\n\tpass\n";
        let diags = check_with_project(
            source,
            &[("base.gd", "class_name PlainBase\nextends Node\n")],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_indirect_tool_chain() {
        let source = "extends Child\nfunc f():\n\tpass\n";
        let diags = check_with_project(
            source,
            &[
                ("base.gd", "@tool\nclass_name ToolRoot\nextends Node\n"),
                ("child.gd", "class_name Child\nextends ToolRoot\n"),
            ],
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn no_warning_without_extends() {
        let source = "func f():\n\tpass\n";
        let diags = check_with_project(
            source,
            &[("base.gd", "@tool\nclass_name ToolBase\nextends Node\n")],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn opt_in_rule() {
        assert!(!MissingTool.default_enabled());
    }
}
