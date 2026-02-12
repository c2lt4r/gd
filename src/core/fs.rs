use miette::{Result, miette};
use path_slash::PathExt;
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
        if entry.file_type().is_file()
            && let Some(ext) = entry.path().extension()
            && ext == "gd"
        {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

/// Skip hidden dirs, .godot/, addons/ build dirs, etc.
fn is_hidden_or_ignored(entry: &walkdir::DirEntry) -> bool {
    // Never filter the root entry (e.g. "." passed as the walk root)
    if entry.depth() == 0 {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    name.starts_with('.') || name == "build" || name == ".godot" || name == ".import"
}

/// Make `path` relative to `base` and return a forward-slash string.
///
/// Uses `strip_prefix` (no canonicalization) to avoid Windows `\\?\` issues.
/// Falls back to the full path if stripping fails.
pub fn relative_slash(path: &Path, base: &Path) -> String {
    let rel = path.strip_prefix(base).unwrap_or(path);
    rel.to_slash_lossy().into_owned()
}
