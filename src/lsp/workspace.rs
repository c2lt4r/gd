use std::path::{Path, PathBuf};

use dashmap::DashMap;
use tower_lsp::lsp_types::InitializeParams;

/// Autoload singleton metadata resolved during workspace scan.
pub struct AutoloadInfo {
    /// The `class_name` declared in the autoload script (if any).
    pub class_name: Option<String>,
    /// Absolute filesystem path to the autoload script.
    pub path: PathBuf,
}

/// Index of all GDScript files in the workspace for cross-file operations.
pub struct WorkspaceIndex {
    project_root: PathBuf,
    /// Cached file contents keyed by absolute path.
    files: DashMap<PathBuf, String>,
    /// `class_name` → absolute file path.
    class_names: DashMap<String, PathBuf>,
    /// Autoload name → resolved metadata.
    autoloads: DashMap<String, AutoloadInfo>,
}

impl WorkspaceIndex {
    /// Create a new workspace index by scanning all `.gd` files under `root`.
    pub fn new(root: PathBuf) -> Self {
        let index = Self {
            project_root: root,
            files: DashMap::new(),
            class_names: DashMap::new(),
            autoloads: DashMap::new(),
        };
        index.scan();
        index
    }

    fn scan(&self) {
        if let Ok(entries) = crate::core::fs::collect_gdscript_files(&self.project_root) {
            for path in entries {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    // Extract class_name from source
                    if let Ok(tree) = crate::core::parser::parse(&content)
                        && let Some(cn) = extract_class_name(tree.root_node(), &content)
                    {
                        self.class_names.insert(cn, path.clone());
                    }
                    self.files.insert(path, content);
                }
            }
        }

        // Parse autoloads from project.godot
        let project_file = self.project_root.join("project.godot");
        for (name, res_path) in crate::core::project::parse_autoloads(&project_file) {
            if let Some(abs_path) = self.resolve_res_path(&res_path) {
                let class_name = self.get_content(&abs_path).and_then(|content| {
                    let tree = crate::core::parser::parse(&content).ok()?;
                    extract_class_name(tree.root_node(), &content)
                });
                self.autoloads.insert(
                    name,
                    AutoloadInfo {
                        class_name,
                        path: abs_path,
                    },
                );
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
            // Update class_name index if needed
            if let Ok(tree) = crate::core::parser::parse(&content)
                && let Some(cn) = extract_class_name(tree.root_node(), &content)
            {
                self.class_names.insert(cn, path.to_path_buf());
            }
            self.files.insert(path.to_path_buf(), content);
        }
    }

    /// Look up a file by its `class_name` declaration.
    pub fn lookup_class_name(&self, name: &str) -> Option<PathBuf> {
        self.class_names.get(name).map(|r| r.value().clone())
    }

    /// Look up an autoload singleton by name.
    pub fn lookup_autoload(
        &self,
        name: &str,
    ) -> Option<dashmap::mapref::one::Ref<'_, String, AutoloadInfo>> {
        self.autoloads.get(name)
    }

    /// Get the content of an autoload's script file.
    pub fn autoload_content(&self, name: &str) -> Option<String> {
        let info = self.autoloads.get(name)?;
        self.get_content(&info.path)
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

/// Extract `class_name` from a parsed GDScript file.
fn extract_class_name(root: tree_sitter::Node, source: &str) -> Option<String> {
    let mut cursor = root.walk();
    for child in root.children(&mut cursor) {
        if child.kind() == "class_name_statement"
            && let Some(name_node) = child.child_by_field_name("name")
        {
            return name_node
                .utf8_text(source.as_bytes())
                .ok()
                .map(std::string::ToString::to_string);
        }
    }
    None
}
