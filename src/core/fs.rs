use miette::{miette, Result};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Collect all .gd files under `root`, respecting .gdignore.
pub fn collect_gdscript_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|e| !is_hidden_or_ignored(e))
    {
        let entry = entry.map_err(|e| miette!("Error walking directory: {e}"))?;
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if ext == "gd" {
                    files.push(entry.into_path());
                }
            }
        }
    }
    files.sort();
    Ok(files)
}

/// Skip hidden dirs, .godot/, addons/ build dirs, etc.
fn is_hidden_or_ignored(entry: &walkdir::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy();
    name.starts_with('.')
        || name == "build"
        || name == ".godot"
        || name == ".import"
}
