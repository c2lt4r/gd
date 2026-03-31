use gd_core::gd_ast::GdFile;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use gd_core::config::LintConfig;
use gd_core::workspace_index::ProjectIndex;

pub struct UnusedPrivateFunction;

impl LintRule for UnusedPrivateFunction {
    fn name(&self) -> &'static str {
        "unused-private-function"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
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
        source: &str,
        _config: &LintConfig,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        let extends = file.extends_class().unwrap_or("RefCounted");

        for func in file.funcs() {
            // Skip Godot virtual methods
            if gd_class_db::is_godot_virtual_method(func.name) {
                continue;
            }

            // Skip methods that exist on the engine base class (overrides)
            if gd_class_db::method_exists(extends, func.name) {
                continue;
            }

            // Check if any other file in the project references this function name
            if is_referenced_in_project(func.name, source, project) {
                continue;
            }

            diags.push(LintDiagnostic {
                rule: "unused-private-function",
                message: format!("function `{}` has no cross-file callers", func.name),
                severity: Severity::Warning,
                line: func.node.start_position().row,
                column: 0,
                end_column: None,
                fix: None,
                context_lines: None,
            });
        }

        diags
    }
}

/// Check if `name` is referenced in any other project file's source.
fn is_referenced_in_project(name: &str, current_source: &str, project: &ProjectIndex) -> bool {
    for file in project.files() {
        let Ok(file_source) = std::fs::read_to_string(&file.path) else {
            continue;
        };

        // Skip the current file
        if file_source == current_source {
            continue;
        }

        // Fast text search
        if !file_source.contains(name) {
            continue;
        }

        // Confirm actual identifier reference (not comment/string)
        if let Ok(tree) = gd_core::parser::parse(&file_source)
            && has_identifier_reference(tree.root_node(), file_source.as_bytes(), name)
        {
            return true;
        }
    }
    false
}

/// Search the AST for any `identifier` node matching `name`.
fn has_identifier_reference(node: tree_sitter::Node, source: &[u8], name: &str) -> bool {
    if node.kind() == "identifier" && node.utf8_text(source).ok() == Some(name) {
        return true;
    }

    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if has_identifier_reference(cursor.node(), source, name) {
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
        UnusedPrivateFunction.check_with_project(&file, source, &config, &project)
    }

    #[test]
    fn detects_unused_function() {
        let source = "\
extends Node
func helper() -> void:
\tpass
";
        let diags = check_with_project(source, &[]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("helper"));
    }

    #[test]
    fn no_warning_called_from_other_file() {
        // In production this reads files from disk; in tests, use a temp dir
        let dir = tempfile::tempdir().unwrap();
        let main_path = dir.path().join("main.gd");
        std::fs::write(
            &main_path,
            "extends Node\nfunc _ready():\n\tUtils.helper()\n",
        )
        .unwrap();
        let utils_path = dir.path().join("utils.gd");
        let utils_source = "class_name Utils\nfunc helper() -> void:\n\tpass\n";
        std::fs::write(&utils_path, utils_source).unwrap();

        let project = gd_core::workspace_index::ProjectIndex::build(dir.path());
        let tree = parser::parse(utils_source).unwrap();
        let file = gd_ast::convert(&tree, utils_source);
        let config = LintConfig::default();
        let diags =
            UnusedPrivateFunction.check_with_project(&file, utils_source, &config, &project);
        assert!(diags.is_empty(), "helper is called from main.gd");
    }

    #[test]
    fn no_warning_godot_virtual() {
        let source = "\
extends Node
func _ready() -> void:
\tpass
func _process(delta: float) -> void:
\tpass
";
        let diags = check_with_project(source, &[]);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_engine_override() {
        let source = "\
extends Node
func get_class() -> String:
\treturn \"MyNode\"
";
        let diags = check_with_project(source, &[]);
        assert!(diags.is_empty());
    }

    #[test]
    fn detects_multiple_unused() {
        let source = "\
extends Node
func a() -> void:
\tpass
func b() -> void:
\tpass
func _ready() -> void:
\tpass
";
        let diags = check_with_project(source, &[]);
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn opt_in_rule() {
        assert!(!UnusedPrivateFunction.default_enabled());
    }
}
