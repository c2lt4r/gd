use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tower_lsp::lsp_types::Position;

use super::find_declaration_by_name;
use gd_core::gd_ast;

#[derive(Serialize, Debug)]
pub struct BulkRenameOutput {
    pub renames: Vec<BulkRenameEntry>,
    pub skipped: Vec<BulkRenameSkipped>,
    pub files_modified: u32,
    pub file: String,
    pub applied: bool,
}

#[derive(Serialize, Debug)]
pub struct BulkRenameEntry {
    pub old_name: String,
    pub new_name: String,
    pub occurrences: u32,
}

#[derive(Serialize, Debug)]
pub struct BulkRenameSkipped {
    pub old_name: String,
    pub new_name: String,
    pub reason: String,
}

/// Rename multiple symbols atomically. Applies renames sequentially,
/// re-parsing between each to handle position shifts correctly.
/// When `file_only` is true, restrict renames to only modify the target file.
#[allow(clippy::too_many_lines)]
pub fn bulk_rename(
    file: &Path,
    renames: &[(String, String)],
    dry_run: bool,
    file_only: bool,
    project_root: &Path,
) -> Result<BulkRenameOutput> {
    let relative_file = gd_core::fs::relative_slash(file, project_root);

    let mut results = Vec::new();
    let mut skipped = Vec::new();
    let mut files_modified = HashSet::new();
    let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();

    for (old_name, new_name) in renames {
        // Re-read and re-parse to get current positions after previous renames
        let source =
            std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
        let tree = gd_core::parser::parse(&source)?;
        let gd_file = gd_ast::convert(&tree, &source);

        let Some(decl) = find_declaration_by_name(&gd_file, old_name) else {
            skipped.push(BulkRenameSkipped {
                old_name: old_name.clone(),
                new_name: new_name.clone(),
                reason: format!("symbol '{old_name}' not found"),
            });
            continue;
        };

        if decl.kind() == "constructor_definition" {
            skipped.push(BulkRenameSkipped {
                old_name: old_name.clone(),
                new_name: new_name.clone(),
                reason: "cannot rename constructor (_init)".to_string(),
            });
            continue;
        }

        // Use typed AST name_node when available, fall back to class_name_node
        let name_node = gd_file
            .find_decl_by_name(old_name)
            .and_then(gd_ast::GdDecl::name_node)
            .or(gd_file.class_name_node);

        let Some(name_node) = name_node else {
            skipped.push(BulkRenameSkipped {
                old_name: old_name.clone(),
                new_name: new_name.clone(),
                reason: "cannot determine symbol position".to_string(),
            });
            continue;
        };

        let position = Position::new(
            name_node.start_position().row as u32,
            name_node.start_position().column as u32,
        );

        let uri = tower_lsp::lsp_types::Url::from_file_path(file)
            .map_err(|()| miette::miette!("invalid path: {}", file.display()))?;
        let workspace = crate::workspace::WorkspaceIndex::new(project_root.to_path_buf());

        let rename_edit =
            crate::rename::rename_cross_file(&source, &uri, position, new_name, &workspace);

        match rename_edit {
            Some(edit) => {
                let mut rename_output =
                    crate::query::convert_rename_edit(&edit, project_root, old_name, new_name);

                // When file_only, restrict edits to only the target file
                if file_only {
                    rename_output.changes.retain(|fe| {
                        let fe_path = project_root.join(&fe.file);
                        fe_path == file
                    });
                }

                let occurrences: u32 = rename_output
                    .changes
                    .iter()
                    .map(|fe| fe.edits.len() as u32)
                    .sum();

                if !dry_run {
                    // Snapshot files before modification (only first time for each file)
                    for fe in &rename_output.changes {
                        let p = project_root.join(&fe.file);
                        snaps
                            .entry(p.clone())
                            .or_insert_with(|| std::fs::read(&p).ok());
                    }
                    crate::query::apply_rename(&rename_output, project_root)?;
                    for fe in &rename_output.changes {
                        files_modified.insert(fe.file.clone());
                    }
                }

                results.push(BulkRenameEntry {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                    occurrences,
                });
            }
            None => {
                skipped.push(BulkRenameSkipped {
                    old_name: old_name.clone(),
                    new_name: new_name.clone(),
                    reason: format!("no renameable symbol '{old_name}' found"),
                });
            }
        }
    }

    // Record undo if any renames were applied
    if !dry_run && !snaps.is_empty() {
        let rename_desc: Vec<String> = results
            .iter()
            .map(|r| format!("{}→{}", r.old_name, r.new_name))
            .collect();
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "bulk-rename",
            &format!("bulk rename {}", rename_desc.join(", ")),
            &snaps,
            project_root,
        );
    }

    Ok(BulkRenameOutput {
        renames: results,
        skipped,
        files_modified: files_modified.len() as u32,
        file: relative_file,
        applied: !dry_run,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_project(files: &[(&str, &str)]) -> TempDir {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n",
        )
        .expect("write project.godot");
        for (name, content) in files {
            fs::write(temp.path().join(name), content).expect("write file");
        }
        temp
    }

    #[test]
    fn bulk_rename_multiple() {
        let temp = setup_project(&[(
            "player.gd",
            "var speed = 10\nvar health = 100\n\n\nfunc _ready():\n\tprint(speed)\n\tprint(health)\n",
        )]);
        let renames = vec![
            ("speed".to_string(), "velocity".to_string()),
            ("health".to_string(), "hp".to_string()),
        ];
        let result = bulk_rename(
            &temp.path().join("player.gd"),
            &renames,
            false,
            false,
            temp.path(),
        )
        .unwrap();

        assert!(result.applied);
        assert_eq!(result.renames.len(), 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(content.contains("velocity"), "speed should be renamed");
        assert!(content.contains("hp"), "health should be renamed");
        assert!(!content.contains("var speed"), "old name should be gone");
        assert!(!content.contains("var health"), "old name should be gone");
    }

    #[test]
    fn bulk_rename_dry_run() {
        let temp = setup_project(&[("player.gd", "var speed = 10\nvar health = 100\n")]);
        let renames = vec![
            ("speed".to_string(), "velocity".to_string()),
            ("health".to_string(), "hp".to_string()),
        ];
        let result = bulk_rename(
            &temp.path().join("player.gd"),
            &renames,
            true,
            false,
            temp.path(),
        )
        .unwrap();

        assert!(!result.applied);
        assert_eq!(result.renames.len(), 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(content.contains("speed"), "dry run should not modify");
        assert!(content.contains("health"), "dry run should not modify");
    }

    #[test]
    fn bulk_rename_some_not_found() {
        let temp = setup_project(&[("player.gd", "var speed = 10\n")]);
        let renames = vec![
            ("speed".to_string(), "velocity".to_string()),
            ("nonexistent".to_string(), "whatever".to_string()),
        ];
        let result = bulk_rename(
            &temp.path().join("player.gd"),
            &renames,
            false,
            false,
            temp.path(),
        )
        .unwrap();

        assert_eq!(result.renames.len(), 1);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].old_name, "nonexistent");
    }
}
