use std::collections::HashMap;
use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;

use gd_core::gd_ast;

use super::{
    declaration_full_range, declaration_kind_str, find_declaration_by_name, normalize_blank_lines,
};

#[derive(Serialize, Debug)]
pub struct BulkDeleteSymbolOutput {
    pub deleted: Vec<BulkDeletedEntry>,
    pub skipped: Vec<BulkSkippedEntry>,
    pub file: String,
    pub applied: bool,
}

#[derive(Serialize, Debug)]
pub struct BulkDeletedEntry {
    pub name: String,
    pub kind: String,
}

#[derive(Serialize, Debug)]
pub struct BulkSkippedEntry {
    pub name: String,
    pub reason: String,
}

pub fn bulk_delete_symbol(
    file: &Path,
    names: &[String],
    force: bool,
    dry_run: bool,
    project_root: &Path,
) -> Result<BulkDeleteSymbolOutput> {
    let source =
        std::fs::read_to_string(file).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let tree = gd_core::parser::parse(&source)?;
    let gd_file = gd_ast::convert(&tree, &source);

    let workspace = crate::workspace::WorkspaceIndex::new(project_root.to_path_buf());
    let file_uri = tower_lsp::lsp_types::Url::from_file_path(file).ok();
    let relative_file = gd_core::fs::relative_slash(file, project_root);

    // Collect declarations to delete and their byte ranges
    // (name, kind, start_byte, end_byte, decl_start_row, decl_end_row)
    let mut deletions: Vec<(String, String, usize, usize)> = Vec::new();
    let mut skipped = Vec::new();

    for name in names {
        // Skip enum member syntax in bulk mode
        if name.contains('.') {
            skipped.push(BulkSkippedEntry {
                name: name.clone(),
                reason: "enum members not supported in bulk mode; use delete-symbol".to_string(),
            });
            continue;
        }

        let Some(decl) = find_declaration_by_name(&gd_file, name) else {
            skipped.push(BulkSkippedEntry {
                name: name.clone(),
                reason: "declaration not found".to_string(),
            });
            continue;
        };

        let kind = declaration_kind_str(decl.kind()).to_string();
        let (start_byte, end_byte) = declaration_full_range(decl, &source);
        let decl_start = decl.start_position().row as u32;
        let decl_end = decl.end_position().row as u32;

        // Check for external references unless forcing
        if !force {
            let refs = crate::references::find_references_by_name(name, &workspace, None, None);
            let external_count = refs
                .iter()
                .filter(|loc| {
                    if let Some(ref uri) = file_uri
                        && &loc.uri == uri
                    {
                        let ref_line = loc.range.start.line;
                        if ref_line >= decl_start && ref_line <= decl_end {
                            return false;
                        }
                    }
                    true
                })
                .count();

            if external_count > 0 {
                skipped.push(BulkSkippedEntry {
                    name: name.clone(),
                    reason: format!(
                        "{external_count} external reference(s); use --force to override"
                    ),
                });
                continue;
            }
        }

        deletions.push((name.clone(), kind, start_byte, end_byte));
    }

    // Sort by start_byte descending so removals don't shift earlier offsets
    deletions.sort_by(|a, b| b.2.cmp(&a.2));

    let mut deleted = Vec::new();

    if dry_run {
        for (name, kind, _, _) in &deletions {
            deleted.push(BulkDeletedEntry {
                name: name.clone(),
                kind: kind.clone(),
            });
        }
    } else {
        let mut new_source = source.clone();
        for (name, kind, start_byte, end_byte) in &deletions {
            new_source.replace_range(*start_byte..*end_byte, "");
            deleted.push(BulkDeletedEntry {
                name: name.clone(),
                kind: kind.clone(),
            });
        }
        normalize_blank_lines(&mut new_source);
        super::validate_no_new_errors(&source, &new_source)?;
        std::fs::write(file, &new_source).map_err(|e| miette::miette!("cannot write file: {e}"))?;

        // Record undo
        let mut snaps: HashMap<PathBuf, Option<Vec<u8>>> = HashMap::new();
        snaps.insert(file.to_path_buf(), Some(source.as_bytes().to_vec()));
        let stack = super::undo::UndoStack::open(project_root);
        let deleted_names: Vec<&str> = deletions.iter().map(|(n, _, _, _)| n.as_str()).collect();
        let _ = stack.record(
            "delete-symbol",
            &format!("bulk delete {}", deleted_names.join(", ")),
            &snaps,
            project_root,
        );
    }

    // Reverse to report in input order (they were sorted descending)
    deleted.reverse();

    Ok(BulkDeleteSymbolOutput {
        deleted,
        skipped,
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
    fn bulk_delete_multiple() {
        let temp = setup_project(&[(
            "player.gd",
            "var a = 1\nvar b = 2\nvar c = 3\n\n\nfunc keep():\n\tpass\n",
        )]);
        let names = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let result = bulk_delete_symbol(
            &temp.path().join("player.gd"),
            &names,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.deleted.len(), 3);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("var a"));
        assert!(!content.contains("var b"));
        assert!(!content.contains("var c"));
        assert!(content.contains("func keep()"));
    }

    #[test]
    fn bulk_delete_some_not_found() {
        let temp = setup_project(&[("player.gd", "var a = 1\nvar b = 2\n")]);
        let names = vec!["a".to_string(), "nonexistent".to_string(), "b".to_string()];
        let result = bulk_delete_symbol(
            &temp.path().join("player.gd"),
            &names,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert!(result.applied);
        assert_eq!(result.deleted.len(), 2);
        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].name, "nonexistent");
    }

    #[test]
    fn bulk_delete_dry_run() {
        let temp = setup_project(&[("player.gd", "var a = 1\nvar b = 2\n")]);
        let names = vec!["a".to_string(), "b".to_string()];
        let result = bulk_delete_symbol(
            &temp.path().join("player.gd"),
            &names,
            false,
            true,
            temp.path(),
        )
        .unwrap();
        assert!(!result.applied);
        assert_eq!(result.deleted.len(), 2);
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(content.contains("var a"));
        assert!(content.contains("var b"));
    }

    #[test]
    fn bulk_delete_skips_referenced() {
        let temp = setup_project(&[(
            "player.gd",
            "var speed = 10\nvar unused = 0\n\n\nfunc run():\n\tprint(speed)\n",
        )]);
        let names = vec!["speed".to_string(), "unused".to_string()];
        let result = bulk_delete_symbol(
            &temp.path().join("player.gd"),
            &names,
            false,
            false,
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.deleted.len(), 1, "only unused should be deleted");
        assert_eq!(result.deleted[0].name, "unused");
        assert_eq!(result.skipped.len(), 1, "speed should be skipped");
        assert_eq!(result.skipped[0].name, "speed");
    }

    #[test]
    fn bulk_delete_force() {
        let temp = setup_project(&[(
            "player.gd",
            "var speed = 10\nvar health = 100\n\n\nfunc run():\n\tprint(speed)\n\tprint(health)\n",
        )]);
        let names = vec!["speed".to_string(), "health".to_string()];
        let result = bulk_delete_symbol(
            &temp.path().join("player.gd"),
            &names,
            true,
            false,
            temp.path(),
        )
        .unwrap();
        assert_eq!(result.deleted.len(), 2, "force should delete both");
        let content = fs::read_to_string(temp.path().join("player.gd")).unwrap();
        assert!(!content.contains("var speed"));
        assert!(!content.contains("var health"));
    }
}
