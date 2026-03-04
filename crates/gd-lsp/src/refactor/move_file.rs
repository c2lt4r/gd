use std::path::Path;

use miette::Result;
use tree_sitter::Node;

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

    let from_rel = gd_core::fs::relative_slash(from, project_root);
    let to_rel = gd_core::fs::relative_slash(to, project_root);
    let old_res = format!("res://{from_rel}");
    let new_res = format!("res://{to_rel}");

    let mut updated_scripts = Vec::new();
    let mut updated_resources = Vec::new();
    let mut updated_autoload: Option<String> = None;
    let mut warnings = Vec::new();
    let mut tx = super::transaction::RefactorTransaction::new();

    // ── Scan .gd files for preload/load/extends references ──────────────
    let gd_files = gd_core::fs::collect_gdscript_files(project_root)?;
    for gd_path in &gd_files {
        let Ok(content) = std::fs::read_to_string(gd_path) else {
            continue;
        };
        if !content.contains(&old_res) {
            continue;
        }
        let replacements = find_ast_replacements(&content, &old_res);
        if replacements.is_empty() {
            continue;
        }
        let file_rel = gd_core::fs::relative_slash(gd_path, project_root);
        for r in &replacements {
            updated_scripts.push(UpdatedReference {
                file: file_rel.clone(),
                line: r.line,
                old_path: old_res.clone(),
                new_path: new_res.clone(),
            });
        }
        if !dry_run {
            let new_content = apply_replacements(&content, &replacements, &old_res, &new_res);
            tx.write_file(gd_path, &new_content)?;
        }
    }

    // ── Scan .tscn/.tres files for ext_resource / load/preload paths ────
    let resource_files = gd_core::fs::collect_resource_files(project_root)?;
    for res_path in &resource_files {
        let Ok(content) = std::fs::read_to_string(res_path) else {
            continue;
        };
        if !content.contains(&old_res) {
            continue;
        }
        let file_rel = gd_core::fs::relative_slash(res_path, project_root);
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
        let autoloads = gd_core::project::parse_autoloads(&project_file);
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

/// A replacement target: the byte range of the `res://` path inside a string node
/// and its 1-based line number.
struct Replacement {
    /// Byte offset of the `res://...` path within the source (inside the quotes).
    start: usize,
    /// Byte offset past the end of the `res://...` path.
    end: usize,
    /// 1-based line number for reporting.
    line: u32,
}

/// Parse a `.gd` file with tree-sitter and find `res://` path occurrences that are
/// arguments to `preload()`/`load()` calls or appear in `extends "res://..."` statements.
///
/// Falls back to line-based search if tree-sitter parsing fails.
fn find_ast_replacements(source: &str, res_path: &str) -> Vec<Replacement> {
    let Ok(tree) = gd_core::parser::parse(source) else {
        // Fallback: treat every occurrence as replaceable (old behaviour).
        return find_all_occurrences(source, res_path);
    };
    let root = tree.root_node();
    let mut results = Vec::new();
    collect_replaceable_strings(root, source.as_bytes(), res_path, &mut results);
    results
}

/// Recursively walk the AST collecting string nodes that contain `res_path` and
/// appear in a context where a path replacement is valid (preload/load call or
/// extends statement).
fn collect_replaceable_strings(
    node: Node,
    source: &[u8],
    res_path: &str,
    out: &mut Vec<Replacement>,
) {
    match node.kind() {
        "call" => {
            // tree-sitter-gdscript: the function name is a child (not a named field),
            // so fall back to `named_child(0)` when `child_by_field_name` returns None.
            if let Some(func_node) = node
                .child_by_field_name("function")
                .or_else(|| node.named_child(0))
                && let Ok(func_name) = func_node.utf8_text(source)
                && (func_name == "preload" || func_name == "load")
                && let Some(args_node) = node.child_by_field_name("arguments")
            {
                for i in 0..args_node.named_child_count() {
                    if let Some(arg) = args_node.named_child(i)
                        && arg.kind() == "string"
                        && let Ok(text) = arg.utf8_text(source)
                        && text.contains(res_path)
                        && let Some(r) = replacement_for_string_node(&arg, source, res_path)
                    {
                        out.push(r);
                    }
                }
            }
        }
        "extends_statement" => {
            for i in 0..node.named_child_count() {
                if let Some(child) = node.named_child(i)
                    && child.kind() == "string"
                    && let Ok(text) = child.utf8_text(source)
                    && text.contains(res_path)
                    && let Some(r) = replacement_for_string_node(&child, source, res_path)
                {
                    out.push(r);
                }
            }
        }
        _ => {}
    }

    // Recurse into children.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_replaceable_strings(child, source, res_path, out);
    }
}

/// Given a `string` AST node whose text contains `res_path`, compute the byte
/// range of `res_path` within the source for replacement.
fn replacement_for_string_node(
    string_node: &Node,
    source: &[u8],
    res_path: &str,
) -> Option<Replacement> {
    let text = string_node.utf8_text(source).ok()?;
    // The string node text includes quotes, e.g. `"res://foo.gd"`.
    // Find the res_path within the full node text.
    let offset_in_node = text.find(res_path)?;
    let abs_start = string_node.start_byte() + offset_in_node;
    let abs_end = abs_start + res_path.len();
    let line = string_node.start_position().row as u32 + 1;
    Some(Replacement {
        start: abs_start,
        end: abs_end,
        line,
    })
}

/// Apply byte-range replacements to source, replacing `old_res` with `new_res`
/// at each `Replacement` location. Processes in reverse order to keep offsets valid.
fn apply_replacements(
    source: &str,
    replacements: &[Replacement],
    old_res: &str,
    new_res: &str,
) -> String {
    let mut result = source.to_string();
    // Sort by start descending so later replacements don't shift earlier offsets.
    let mut sorted: Vec<&Replacement> = replacements.iter().collect();
    sorted.sort_by(|a, b| b.start.cmp(&a.start));
    for r in sorted {
        // Safety check: verify the slice still matches.
        if result.get(r.start..r.end) == Some(old_res) {
            result.replace_range(r.start..r.end, new_res);
        }
    }
    result
}

/// Fallback: find every occurrence of `res_path` in `source` and return a
/// `Replacement` for each. Used when tree-sitter parsing fails.
fn find_all_occurrences(source: &str, res_path: &str) -> Vec<Replacement> {
    let mut results = Vec::new();
    let mut search_start = 0;
    while let Some(pos) = source[search_start..].find(res_path) {
        let abs_start = search_start + pos;
        let abs_end = abs_start + res_path.len();
        let line = source[..abs_start].matches('\n').count() as u32 + 1;
        results.push(Replacement {
            start: abs_start,
            end: abs_end,
            line,
        });
        search_start = abs_end;
    }
    results
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

    #[test]
    fn move_skips_path_in_comment_and_print() {
        let temp = setup_project(&[
            ("scripts/player.gd", "extends CharacterBody2D\n"),
            (
                "scripts/main.gd",
                "extends Node\n\
                 # See res://scripts/player.gd for details\n\
                 var Player = preload(\"res://scripts/player.gd\")\n\
                 func _ready():\n\
                 \tprint(\"res://scripts/player.gd\")\n\
                 \tvar msg = \"moved from res://scripts/player.gd\"\n",
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
        // Only the preload reference should be reported
        assert_eq!(result.updated_scripts.len(), 1);
        assert_eq!(result.updated_scripts[0].line, 3);

        let content = fs::read_to_string(temp.path().join("scripts/main.gd")).unwrap();
        // preload should be updated
        assert!(content.contains("preload(\"res://entities/player.gd\")"));
        // comment should NOT be updated
        assert!(content.contains("# See res://scripts/player.gd"));
        // print argument should NOT be updated
        assert!(content.contains("print(\"res://scripts/player.gd\")"));
        // data string should NOT be updated
        assert!(content.contains("\"moved from res://scripts/player.gd\""));
    }

    #[test]
    fn move_updates_load_and_extends_path() {
        let temp = setup_project(&[
            ("scripts/base.gd", "extends Node\n"),
            (
                "scripts/child.gd",
                "extends \"res://scripts/base.gd\"\n\
                 var base_scene = load(\"res://scripts/base.gd\")\n",
            ),
        ]);
        let result = move_file(
            &temp.path().join("scripts/base.gd"),
            &temp.path().join("core/base.gd"),
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.updated_scripts.len(), 2);

        let content = fs::read_to_string(temp.path().join("scripts/child.gd")).unwrap();
        assert!(content.contains("extends \"res://core/base.gd\""));
        assert!(content.contains("load(\"res://core/base.gd\")"));
        assert!(!content.contains("res://scripts/base.gd"));
    }

    #[test]
    fn find_ast_replacements_only_matches_load_contexts() {
        let source = "extends Node\n\
                      const A = preload(\"res://old/path.gd\")\n\
                      var b = load(\"res://old/path.gd\")\n\
                      # res://old/path.gd\n\
                      var msg = \"res://old/path.gd\"\n";
        let replacements = find_ast_replacements(source, "res://old/path.gd");
        // Should find preload + load, but NOT comment or data string
        assert_eq!(replacements.len(), 2);
        assert_eq!(replacements[0].line, 2); // preload line
        assert_eq!(replacements[1].line, 3); // load line
    }

    #[test]
    fn find_ast_replacements_extends_path() {
        let source = "extends \"res://old/base.gd\"\n";
        let replacements = find_ast_replacements(source, "res://old/base.gd");
        assert_eq!(replacements.len(), 1);
        assert_eq!(replacements[0].line, 1);
    }
}
