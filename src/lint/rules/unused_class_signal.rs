use crate::core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::workspace_index::ProjectIndex;

pub struct UnusedClassSignal;

impl LintRule for UnusedClassSignal {
    fn name(&self) -> &'static str {
        "unused-class-signal"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
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

        for signal in file.signals() {
            // Skip private signal convention (starting with _)
            if signal.name.starts_with('_') {
                continue;
            }

            // Check if the signal is connected/emitted in any other project file
            if is_signal_referenced(signal.name, source, project) {
                continue;
            }

            diags.push(LintDiagnostic {
                rule: "unused-class-signal",
                message: format!(
                    "signal `{}` has no cross-file connections or emissions",
                    signal.name
                ),
                severity: Severity::Warning,
                line: signal.node.start_position().row,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }

        diags
    }
}

/// Check if a signal name is referenced in any other project file.
fn is_signal_referenced(name: &str, current_source: &str, project: &ProjectIndex) -> bool {
    for file in project.files() {
        let Ok(file_source) = std::fs::read_to_string(&file.path) else {
            continue;
        };

        // Skip the current file
        if file_source == current_source {
            continue;
        }

        // Fast text search — signal must appear as identifier or string
        if !file_source.contains(name) {
            continue;
        }

        // Check for actual identifier or string reference
        if let Ok(tree) = crate::core::parser::parse(&file_source)
            && has_signal_reference(tree.root_node(), file_source.as_bytes(), name)
        {
            return true;
        }
    }

    // Check .tscn files for [connection signal="name" ...]
    is_signal_in_tscn(name, project)
}

/// Check .tscn files in the project root for signal connections.
fn is_signal_in_tscn(name: &str, project: &ProjectIndex) -> bool {
    let root = project.project_root();
    let pattern = format!("signal=\"{name}\"");
    walk_tscn(root, &pattern)
}

fn walk_tscn(dir: &std::path::Path, pattern: &str) -> bool {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            // Skip hidden directories
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if walk_tscn(&path, pattern) {
                return true;
            }
        } else if path.extension().is_some_and(|e| e == "tscn")
            && let Ok(content) = std::fs::read_to_string(&path)
            && content.contains(pattern)
        {
            return true;
        }
    }
    false
}

/// Search the AST for identifier or string references to a signal name.
fn has_signal_reference(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    match node.kind() {
        "identifier" => {
            if node.utf8_text(source).ok() == Some(name) {
                return true;
            }
        }
        "string" => {
            // Check for "signal_name" in connect("signal_name", ...) etc.
            if let Ok(text) = node.utf8_text(source) {
                let stripped = text
                    .trim_start_matches('"')
                    .trim_end_matches('"')
                    .trim_start_matches('\'')
                    .trim_end_matches('\'');
                if stripped == name {
                    return true;
                }
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if has_signal_reference(cursor.node(), source, name) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
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
        UnusedClassSignal.check_with_project(&file, source, &config, &project)
    }

    #[test]
    fn detects_unused_signal() {
        let source = "\
extends Node
signal health_changed(value: int)
";
        let diags = check_with_project(source, &[]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("health_changed"));
    }

    #[test]
    fn no_warning_connected_from_other_file() {
        let dir = tempfile::tempdir().unwrap();
        let emitter = "class_name Emitter\nsignal health_changed(value: int)\n";
        let emitter_path = dir.path().join("emitter.gd");
        std::fs::write(&emitter_path, emitter).unwrap();
        let listener_path = dir.path().join("listener.gd");
        std::fs::write(
            &listener_path,
            "extends Node\nfunc _ready():\n\temitter.health_changed.connect(_on_health)\n",
        )
        .unwrap();

        let project = crate::core::workspace_index::ProjectIndex::build(dir.path());
        let tree = parser::parse(emitter).unwrap();
        let file = gd_ast::convert(&tree, emitter);
        let config = LintConfig::default();
        let diags =
            UnusedClassSignal.check_with_project(&file, emitter, &config, &project);
        assert!(diags.is_empty(), "signal connected from listener.gd");
    }

    #[test]
    fn no_warning_private_signal() {
        let source = "\
extends Node
signal _internal_signal
";
        let diags = check_with_project(source, &[]);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_multiple_unused() {
        let source = "\
extends Node
signal health_changed
signal mana_changed
signal _private_signal
";
        let diags = check_with_project(source, &[]);
        assert_eq!(diags.len(), 2); // health_changed and mana_changed, not _private_signal
    }

    #[test]
    fn no_warning_signal_in_tscn() {
        let dir = tempfile::tempdir().unwrap();
        let source = "class_name MyNode\nsignal pressed\n";
        let gd_path = dir.path().join("my_node.gd");
        std::fs::write(&gd_path, source).unwrap();
        let tscn_path = dir.path().join("scene.tscn");
        std::fs::write(
            &tscn_path,
            "[connection signal=\"pressed\" from=\"Button\" to=\".\" method=\"_on_pressed\"]\n",
        )
        .unwrap();

        let project = crate::core::workspace_index::ProjectIndex::build(dir.path());
        let tree = parser::parse(source).unwrap();
        let file = gd_ast::convert(&tree, source);
        let config = LintConfig::default();
        let diags =
            UnusedClassSignal.check_with_project(&file, source, &config, &project);
        assert!(diags.is_empty(), "signal connected in .tscn");
    }

    #[test]
    fn opt_in_rule() {
        assert!(!UnusedClassSignal.default_enabled());
    }
}
