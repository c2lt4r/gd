use std::path::Path;

use miette::Result;

use super::{MoveFileOutput, UpdatedReference};

/// Move/rename a file and update all references across the project.
///
/// Scans .gd files for `preload()`/`load()` and `extends "res://..."` references,
/// .tscn/.tres files for `ext_resource` path entries, and project.godot for autoloads.
#[allow(clippy::too_many_lines)]
pub fn move_file(
    from: &Path,
    to: &Path,
    dry_run: bool,
    project_root: &Path,
) -> Result<MoveFileOutput> {
    if !from.exists() {
        return Err(miette::miette!("source file not found: {}", from.display()));
    }
    if to.exists() {
        return Err(miette::miette!(
            "destination already exists: {}",
            to.display()
        ));
    }

    let from_rel = crate::core::fs::relative_slash(from, project_root);
    let to_rel = crate::core::fs::relative_slash(to, project_root);
    let old_res = format!("res://{from_rel}");
    let new_res = format!("res://{to_rel}");

    let mut updated_scripts = Vec::new();
    let mut updated_resources = Vec::new();
    let mut updated_autoload: Option<String> = None;
    let mut warnings = Vec::new();
    let mut tx = super::transaction::RefactorTransaction::new();

    // ── Scan .gd files for preload/load/extends references ──────────────
    let gd_files = crate::core::fs::collect_gdscript_files(project_root)?;
    for gd_path in &gd_files {
        let Ok(content) = std::fs::read_to_string(gd_path) else {
            continue;
        };
        let refs = find_res_path_references(&content, &old_res);
        if refs.is_empty() {
            continue;
        }
        let file_rel = crate::core::fs::relative_slash(gd_path, project_root);
        for line_num in &refs {
            updated_scripts.push(UpdatedReference {
                file: file_rel.clone(),
                line: *line_num,
                old_path: old_res.clone(),
                new_path: new_res.clone(),
            });
        }
        if !dry_run {
            let new_content = content.replace(&old_res, &new_res);
            tx.write_file(gd_path, &new_content)?;
        }
    }

    // ── Scan .tscn/.tres files for ext_resource / load/preload paths ────
    let resource_files = crate::core::fs::collect_resource_files(project_root)?;
    for res_path in &resource_files {
        let Ok(content) = std::fs::read_to_string(res_path) else {
            continue;
        };
        if !content.contains(&old_res) {
            continue;
        }
        let file_rel = crate::core::fs::relative_slash(res_path, project_root);
        let refs = find_res_path_line_numbers(&content, &old_res);
        for line_num in &refs {
            updated_resources.push(UpdatedReference {
                file: file_rel.clone(),
                line: *line_num,
                old_path: old_res.clone(),
                new_path: new_res.clone(),
            });
        }
        if !dry_run {
            let new_content = content.replace(&old_res, &new_res);
            tx.write_file(res_path, &new_content)?;
        }
    }

    // ── Check project.godot autoloads ───────────────────────────────────
    let project_file = project_root.join("project.godot");
    if project_file.exists() {
        let autoloads = crate::core::project::parse_autoloads(&project_file);
        for (name, path) in &autoloads {
            if path == &old_res {
                updated_autoload = Some(name.clone());
                if !dry_run {
                    let content = std::fs::read_to_string(&project_file)
                        .map_err(|e| miette::miette!("cannot read project.godot: {e}"))?;
                    let new_content = content.replace(&old_res, &new_res);
                    tx.write_file(&project_file, &new_content)?;
                }
                break;
            }
        }
    }

    // ── Move the actual file ────────────────────────────────────────────
    if !dry_run {
        if let Some(parent) = to.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| miette::miette!("cannot create directories: {e}"))?;
        }
        // Try rename through transaction (snapshots both paths automatically);
        // fall back to copy+delete for cross-device moves.
        if tx.rename_file(from, to).is_err() {
            let content =
                std::fs::read(from).map_err(|e| miette::miette!("cannot read source file: {e}"))?;
            std::fs::write(to, &content)
                .map_err(|e| miette::miette!("cannot write destination file: {e}"))?;
            std::fs::remove_file(from)
                .map_err(|e| miette::miette!("cannot remove source file: {e}"))?;
        }

        let snapshots = tx.into_snapshots();
        let stack = super::undo::UndoStack::open(project_root);
        let _ = stack.record(
            "move-file",
            &format!("{from_rel} → {to_rel}"),
            &snapshots,
            project_root,
        );
    }

    // ── Warn about potential references we can't statically detect ───────
    if std::path::Path::new(&from_rel)
        .extension()
        .is_some_and(|ext| ext == "gd")
    {
        // Check for string-based load() calls we might miss
        let total_refs = updated_scripts.len() + updated_resources.len();
        if total_refs == 0 {
            // No references found — maybe it's only referenced dynamically
            warnings.push("no static references found; check for dynamic load() calls".into());
        }
    }

    Ok(MoveFileOutput {
        from: from_rel,
        to: to_rel,
        applied: !dry_run,
        updated_scripts,
        updated_resources,
        updated_autoload,
        warnings,
    })
}

/// Find line numbers (1-based) where `res_path` appears in GDScript source
/// (preload, load, or extends "res://..." references).
fn find_res_path_references(source: &str, res_path: &str) -> Vec<u32> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            if line.contains(res_path) {
                Some(i as u32 + 1)
            } else {
                None
            }
        })
        .collect()
}

/// Find line numbers (1-based) where `res_path` appears in a resource file.
fn find_res_path_line_numbers(source: &str, res_path: &str) -> Vec<u32> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, line)| {
            if line.contains(res_path) {
                Some(i as u32 + 1)
            } else {
                None
            }
        })
        .collect()
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
            let path = temp.path().join(name);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create dirs");
            }
            fs::write(path, content).expect("write file");
        }
        temp
    }

    #[test]
    fn move_gd_updates_preload_references() {
        let temp = setup_project(&[
            ("scripts/player.gd", "extends CharacterBody2D\n"),
            (
                "scripts/main.gd",
                "var Player = preload(\"res://scripts/player.gd\")\n",
            ),
        ]);
        let result = move_file(
            &temp.path().join("scripts/player.gd"),
            &temp.path().join("entities/player.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.from, "scripts/player.gd");
        assert_eq!(result.to, "entities/player.gd");
        assert_eq!(result.updated_scripts.len(), 1);
        assert_eq!(result.updated_scripts[0].file, "scripts/main.gd");

        // Verify file was actually moved
        assert!(!temp.path().join("scripts/player.gd").exists());
        assert!(temp.path().join("entities/player.gd").exists());

        // Verify reference was updated
        let main_content = fs::read_to_string(temp.path().join("scripts/main.gd")).unwrap();
        assert!(main_content.contains("res://entities/player.gd"));
        assert!(!main_content.contains("res://scripts/player.gd"));
    }

    #[test]
    fn move_gd_updates_tscn_ext_resource() {
        let temp = setup_project(&[
            ("scripts/enemy.gd", "extends Node2D\n"),
            (
                "scenes/level.tscn",
                "[gd_scene load_steps=2 format=3]\n\n\
                 [ext_resource type=\"Script\" path=\"res://scripts/enemy.gd\" id=\"1\"]\n\n\
                 [node name=\"Root\" type=\"Node2D\"]\n",
            ),
        ]);
        let result = move_file(
            &temp.path().join("scripts/enemy.gd"),
            &temp.path().join("entities/enemy.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.updated_resources.len(), 1);
        assert_eq!(result.updated_resources[0].file, "scenes/level.tscn");

        let tscn = fs::read_to_string(temp.path().join("scenes/level.tscn")).unwrap();
        assert!(tscn.contains("res://entities/enemy.gd"));
        assert!(!tscn.contains("res://scripts/enemy.gd"));
    }

    #[test]
    fn move_tscn_updates_other_tscn() {
        let temp = setup_project(&[
            (
                "scenes/hud.tscn",
                "[gd_scene format=3]\n\n[node name=\"HUD\" type=\"Control\"]\n",
            ),
            (
                "scenes/main.tscn",
                "[gd_scene load_steps=2 format=3]\n\n\
                 [ext_resource type=\"PackedScene\" path=\"res://scenes/hud.tscn\" id=\"1\"]\n\n\
                 [node name=\"Root\" type=\"Node2D\"]\n",
            ),
        ]);
        let result = move_file(
            &temp.path().join("scenes/hud.tscn"),
            &temp.path().join("ui/hud.tscn"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.updated_resources.len(), 1);

        let main_tscn = fs::read_to_string(temp.path().join("scenes/main.tscn")).unwrap();
        assert!(main_tscn.contains("res://ui/hud.tscn"));
    }

    #[test]
    fn move_autoloaded_file_updates_project_godot() {
        let temp = tempfile::Builder::new()
            .prefix("gdtest")
            .tempdir()
            .expect("create temp dir");
        fs::write(
            temp.path().join("project.godot"),
            "[application]\nconfig/name=\"test\"\n\n\
             [autoload]\nGame=\"*res://scripts/global.gd\"\n",
        )
        .expect("write project.godot");
        fs::create_dir_all(temp.path().join("scripts")).unwrap();
        fs::write(temp.path().join("scripts/global.gd"), "extends Node\n").unwrap();

        let result = move_file(
            &temp.path().join("scripts/global.gd"),
            &temp.path().join("autoload/global.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.updated_autoload, Some("Game".to_string()));

        let project = fs::read_to_string(temp.path().join("project.godot")).unwrap();
        assert!(project.contains("res://autoload/global.gd"));
        assert!(!project.contains("res://scripts/global.gd"));
    }

    #[test]
    fn move_dry_run_no_changes() {
        let temp = setup_project(&[
            ("scripts/player.gd", "extends CharacterBody2D\n"),
            (
                "scripts/main.gd",
                "var Player = preload(\"res://scripts/player.gd\")\n",
            ),
        ]);
        let result = move_file(
            &temp.path().join("scripts/player.gd"),
            &temp.path().join("entities/player.gd"),
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        assert!(!result.updated_scripts.is_empty());

        // File should NOT be moved
        assert!(temp.path().join("scripts/player.gd").exists());
        assert!(!temp.path().join("entities/player.gd").exists());

        // Reference should NOT be changed
        let main_content = fs::read_to_string(temp.path().join("scripts/main.gd")).unwrap();
        assert!(main_content.contains("res://scripts/player.gd"));
    }

    #[test]
    fn move_source_not_found() {
        let temp = setup_project(&[]);
        let result = move_file(
            &temp.path().join("nonexistent.gd"),
            &temp.path().join("dest.gd"),
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn move_destination_already_exists() {
        let temp = setup_project(&[
            ("source.gd", "extends Node\n"),
            ("dest.gd", "extends Node2D\n"),
        ]);
        let result = move_file(
            &temp.path().join("source.gd"),
            &temp.path().join("dest.gd"),
            false,
            temp.path(),
        );
        assert!(result.is_err());
    }
}
