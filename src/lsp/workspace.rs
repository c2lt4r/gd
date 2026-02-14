use std::path::{Path, PathBuf};

use dashmap::DashMap;
use tower_lsp::lsp_types::InitializeParams;

/// Index of all GDScript files in the workspace for cross-file operations.
pub struct WorkspaceIndex {
    project_root: PathBuf,
    /// Cached file contents keyed by absolute path.
    files: DashMap<PathBuf, String>,
}

impl WorkspaceIndex {
    /// Create a new workspace index by scanning all `.gd` files under `root`.
    pub fn new(root: PathBuf) -> Self {
        let index = Self {
            project_root: root,
            files: DashMap::new(),
        };
        index.scan();
        index
    }

    fn scan(&self) {
        if let Ok(entries) = crate::core::fs::collect_gdscript_files(&self.project_root) {
            for path in entries {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    self.files.insert(path, content);
                }
            }
        }
    }

    /// The project root directory.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Convert a `res://` path to an absolute filesystem path.
    pub fn resolve_res_path(&self, res_path: &str) -> Option<PathBuf> {
        let rel = res_path.strip_prefix("res://")?;
        let path = self.project_root.join(rel);
        if path.exists() { Some(path) } else { None }
    }

    /// Return all indexed `(path, content)` pairs.
    pub fn all_files(&self) -> Vec<(PathBuf, String)> {
        self.files
            .iter()
            .map(|r| (r.key().clone(), r.value().clone()))
            .collect()
    }

    /// Get a single file's cached content.
    pub fn get_content(&self, path: &Path) -> Option<String> {
        self.files.get(path).map(|r| r.value().clone())
    }

    /// Re-read a file from disk and update the cache.
    pub fn refresh_file(&self, path: &Path) {
        if let Ok(content) = std::fs::read_to_string(path) {
            self.files.insert(path.to_path_buf(), content);
        }
    }
}

/// Try to discover the Godot project root from LSP initialize params.
pub fn discover_root(params: &InitializeParams) -> Option<PathBuf> {
    // Try workspace folders first
    if let Some(folders) = &params.workspace_folders
        && let Some(folder) = folders.first()
        && let Some(root) = root_from_path(&folder.uri.to_file_path().ok()?)
    {
        return Some(root);
    }

    // Fall back to root_uri
    #[allow(deprecated)]
    if let Some(ref root_uri) = params.root_uri
        && let Some(root) = root_from_path(&root_uri.to_file_path().ok()?)
    {
        return Some(root);
    }

    None
}

fn root_from_path(path: &Path) -> Option<PathBuf> {
    crate::core::project::GodotProject::discover(path)
        .ok()
        .map(|p| p.root)
}
