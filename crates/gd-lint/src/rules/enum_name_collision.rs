use gd_core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;
use gd_core::workspace_index::ProjectIndex;

pub struct EnumNameCollision;

impl LintRule for EnumNameCollision {
    fn name(&self) -> &'static str {
        "enum-name-collision"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Correctness
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
        let mut diags = Vec::new();

        for enum_decl in file.enums() {
            // Check if any file in the project has a class_name matching this enum
            if project.lookup_class(enum_decl.name).is_some() {
                diags.push(LintDiagnostic {
                    rule: "enum-name-collision",
                    message: format!(
                        "enum `{}` collides with global class_name `{}` — \
                         Godot resolves the type annotation to the class, not the enum. \
                         Rename the enum or use the fully qualified name",
                        enum_decl.name, enum_decl.name,
                    ),
                    severity: Severity::Error,
                    line: enum_decl.node.start_position().row,
                    column: 0,
                    end_column: None,
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
    use gd_core::gd_ast;
    use gd_core::{parser, workspace_index};
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
        EnumNameCollision.check_with_project(&file, source, &config, &project)
    }

    #[test]
    fn detects_enum_colliding_with_class_name() {
        let source = "\
class_name LobbyManager
extends Node
enum LobbyState { WAITING, FULL }
var lobby_state: LobbyState = LobbyState.WAITING
";
        let diags = check_with_project(
            source,
            &[("lobby_state.gd", "class_name LobbyState\nextends Node\n")],
        );
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("collides with global class_name"));
    }

    #[test]
    fn no_warning_when_no_collision() {
        let source = "\
class_name LobbyManager
extends Node
enum State { WAITING, FULL }
var state: State = State.WAITING
";
        let diags = check_with_project(
            source,
            &[("lobby_state.gd", "class_name LobbyState\nextends Node\n")],
        );
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_without_project_class() {
        let source = "\
class_name LobbyManager
extends Node
enum LobbyState { WAITING, FULL }
";
        let diags = check_with_project(source, &[("other.gd", "class_name Other\nextends Node\n")]);
        assert!(diags.is_empty());
    }
}
