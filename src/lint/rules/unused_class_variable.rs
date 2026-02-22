use tree_sitter::Tree;

use super::{LintCategory, LintDiagnostic, LintRule, Severity};
use crate::core::config::LintConfig;
use crate::core::symbol_table::SymbolTable;
use crate::core::workspace_index::ProjectIndex;

pub struct UnusedClassVariable;

impl LintRule for UnusedClassVariable {
    fn name(&self) -> &'static str {
        "unused-class-variable"
    }

    fn category(&self) -> LintCategory {
        LintCategory::Maintenance
    }

    fn default_enabled(&self) -> bool {
        false
    }

    fn check(&self, _tree: &Tree, _source: &str, _config: &LintConfig) -> Vec<LintDiagnostic> {
        Vec::new()
    }

    fn check_with_project(
        &self,
        _tree: &Tree,
        source: &str,
        _config: &LintConfig,
        symbols: &SymbolTable,
        project: &ProjectIndex,
    ) -> Vec<LintDiagnostic> {
        let mut diags = Vec::new();

        // Check if this class is an autoload (all members globally accessible)
        if let Some(ref cn) = symbols.class_name
            && project.is_autoload(cn)
        {
            return diags;
        }

        for var in &symbols.variables {
            // Skip @export / @onready variables — editor/scene-referenced
            if var
                .annotations
                .iter()
                .any(|a| a == "export" || a == "onready")
            {
                continue;
            }

            // Skip constants — they're often used as class-level configuration
            if var.is_constant {
                continue;
            }

            // Check if any other file references this variable name
            if is_referenced_in_project(&var.name, source, project) {
                continue;
            }

            diags.push(LintDiagnostic {
                rule: "unused-class-variable",
                message: format!("class variable `{}` has no cross-file references", var.name),
                severity: Severity::Warning,
                line: var.line,
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

        // Confirm actual identifier reference
        if let Ok(tree) = crate::core::parser::parse(&file_source)
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
        UnusedClassVariable.check_with_project(&tree, source, &config, &symbols, &project)
    }

    #[test]
    fn detects_unused_variable() {
        let source = "\
extends Node
var health: int = 100
";
        let diags = check_with_project(source, &[]);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("health"));
    }

    #[test]
    fn no_warning_referenced_from_other_file() {
        let dir = tempfile::tempdir().unwrap();
        let player_source = "class_name Player\nvar health: int = 100\n";
        std::fs::write(dir.path().join("player.gd"), player_source).unwrap();
        std::fs::write(
            dir.path().join("hud.gd"),
            "extends Control\nfunc update():\n\tvar p = Player.new()\n\tprint(p.health)\n",
        )
        .unwrap();

        let project = crate::core::workspace_index::ProjectIndex::build(dir.path());
        let tree = parser::parse(player_source).unwrap();
        let symbols = symbol_table::build(&tree, player_source);
        let config = LintConfig::default();
        let diags = UnusedClassVariable.check_with_project(
            &tree,
            player_source,
            &config,
            &symbols,
            &project,
        );
        assert!(diags.is_empty(), "health is referenced from hud.gd");
    }

    #[test]
    fn no_warning_export_var() {
        let source = "\
extends Node
@export var health: int = 100
";
        let diags = check_with_project(source, &[]);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_onready_var() {
        let source = "\
extends Node
@onready var sprite = $Sprite2D
";
        let diags = check_with_project(source, &[]);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_constant() {
        let source = "\
extends Node
const MAX_SPEED: float = 300.0
";
        let diags = check_with_project(source, &[]);
        assert!(diags.is_empty());
    }

    #[test]
    fn no_warning_autoload_member() {
        let dir = tempfile::tempdir().unwrap();
        let global_source = "class_name GameGlobal\nextends Node\nvar score: int = 0\n";
        std::fs::write(dir.path().join("global.gd"), global_source).unwrap();
        std::fs::write(
            dir.path().join("project.godot"),
            "[application]\nconfig/name=\"Test\"\n\n[autoload]\nGame=\"*res://global.gd\"\n",
        )
        .unwrap();

        let project = crate::core::workspace_index::ProjectIndex::build(dir.path());
        let tree = parser::parse(global_source).unwrap();
        let symbols = symbol_table::build(&tree, global_source);
        let config = LintConfig::default();
        let diags = UnusedClassVariable.check_with_project(
            &tree,
            global_source,
            &config,
            &symbols,
            &project,
        );
        assert!(diags.is_empty(), "autoload members are globally accessible");
    }

    #[test]
    fn detects_multiple_unused() {
        let source = "\
extends Node
var health: int = 100
var mana: int = 50
@export var speed: float = 200.0
const MAX_HP = 999
";
        let diags = check_with_project(source, &[]);
        assert_eq!(diags.len(), 2); // health and mana, not speed (export) or MAX_HP (const)
    }

    #[test]
    fn opt_in_rule() {
        assert!(!UnusedClassVariable.default_enabled());
    }
}
