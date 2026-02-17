use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use tower_lsp::lsp_types::InitializeParams;

use crate::core::symbol_table::{self, SymbolTable};

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
    files: DashMap<PathBuf, Arc<String>>,
    /// `class_name` → absolute file path.
    class_names: DashMap<String, PathBuf>,
    /// Autoload name → resolved metadata.
    autoloads: DashMap<String, AutoloadInfo>,
    /// Per-file cached `SymbolTable`.
    symbols: DashMap<PathBuf, SymbolTable>,
    /// Symbol name → files that declare it.
    declarations: DashMap<String, Vec<PathBuf>>,
    /// File → extends class name.
    extends_map: DashMap<PathBuf, Option<String>>,
}

impl WorkspaceIndex {
    /// Create a new workspace index by scanning all `.gd` files under `root`.
    pub fn new(root: PathBuf) -> Self {
        let index = Self {
            project_root: root,
            files: DashMap::new(),
            class_names: DashMap::new(),
            autoloads: DashMap::new(),
            symbols: DashMap::new(),
            declarations: DashMap::new(),
            extends_map: DashMap::new(),
        };
        index.scan();
        index
    }

    fn scan(&self) {
        if let Ok(entries) = crate::core::fs::collect_gdscript_files(&self.project_root) {
            for path in entries {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Ok(tree) = crate::core::parser::parse(&content) {
                        // Extract class_name
                        if let Some(cn) = extract_class_name(tree.root_node(), &content) {
                            self.class_names.insert(cn, path.clone());
                        }
                        // Build and cache SymbolTable
                        let table = symbol_table::build(&tree, &content);
                        self.extends_map.insert(path.clone(), table.extends.clone());
                        self.insert_declarations_from_table(&path, &table);
                        self.symbols.insert(path.clone(), table);
                    }
                    self.files.insert(path, Arc::new(content));
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

    /// Insert declaration index entries from a `SymbolTable`.
    fn insert_declarations_from_table(&self, path: &Path, table: &SymbolTable) {
        let pb = path.to_path_buf();
        for f in &table.functions {
            self.declarations
                .entry(f.name.clone())
                .or_default()
                .push(pb.clone());
        }
        for v in &table.variables {
            self.declarations
                .entry(v.name.clone())
                .or_default()
                .push(pb.clone());
        }
        for s in &table.signals {
            self.declarations
                .entry(s.name.clone())
                .or_default()
                .push(pb.clone());
        }
        for e in &table.enums {
            self.declarations
                .entry(e.name.clone())
                .or_default()
                .push(pb.clone());
        }
        if let Some(cn) = &table.class_name {
            self.declarations.entry(cn.clone()).or_default().push(pb);
        }
    }

    /// Remove all declaration index entries for a file.
    fn remove_declarations_for_file(&self, path: &Path) {
        // Iterate all entries and remove this path from each Vec
        self.declarations.retain(|_, paths| {
            paths.retain(|p| p != path);
            !paths.is_empty()
        });
    }

    /// Rebuild declarations for a single file.
    fn rebuild_declarations_for_file(&self, path: &Path) {
        self.remove_declarations_for_file(path);
        if let Some(table) = self.symbols.get(path) {
            self.insert_declarations_from_table(path, &table);
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
    pub fn all_files(&self) -> Vec<(PathBuf, Arc<String>)> {
        self.files
            .iter()
            .map(|r| (r.key().clone(), Arc::clone(r.value())))
            .collect()
    }

    /// Get a single file's cached content.
    pub fn get_content(&self, path: &Path) -> Option<Arc<String>> {
        self.files.get(path).map(|r| Arc::clone(r.value()))
    }

    /// Get a cached `SymbolTable` for a file.
    pub fn get_symbols(
        &self,
        path: &Path,
    ) -> Option<dashmap::mapref::one::Ref<'_, PathBuf, SymbolTable>> {
        self.symbols.get(path)
    }

    /// Re-read a file from disk and update the cache.
    pub fn refresh_file(&self, path: &Path) {
        if let Ok(content) = std::fs::read_to_string(path) {
            self.update_content(path, &content);
        }
    }

    /// Update workspace state from in-memory content (for unsaved files).
    pub fn update_in_memory(&self, path: &Path, content: &str) {
        self.update_content(path, content);
    }

    /// Shared logic for refresh_file and update_in_memory.
    fn update_content(&self, path: &Path, content: &str) {
        if let Ok(tree) = crate::core::parser::parse(content) {
            // Update class_name index
            if let Some(cn) = extract_class_name(tree.root_node(), content) {
                self.class_names.insert(cn, path.to_path_buf());
            }
            // Rebuild SymbolTable
            let table = symbol_table::build(&tree, content);
            self.extends_map
                .insert(path.to_path_buf(), table.extends.clone());
            self.symbols.insert(path.to_path_buf(), table);
            self.rebuild_declarations_for_file(path);
        }
        self.files
            .insert(path.to_path_buf(), Arc::new(content.to_string()));
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
    pub fn autoload_content(&self, name: &str) -> Option<Arc<String>> {
        let info = self.autoloads.get(name)?;
        self.get_content(&info.path)
    }

    /// Look up files that declare a symbol with the given name.
    pub fn lookup_declaration(&self, name: &str) -> Vec<PathBuf> {
        self.declarations
            .get(name)
            .map(|r| r.value().clone())
            .unwrap_or_default()
    }

    /// Get the extends class name for a file.
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn file_extends(&self, path: &Path) -> Option<String> {
        self.extends_map.get(path)?.value().clone()
    }

    /// Walk the extends chain for a class: class_name → file → extends → class_name → ...
    #[cfg(test)]
    #[allow(dead_code)]
    pub fn extends_chain(&self, class: &str) -> Vec<String> {
        let mut chain = Vec::new();
        let mut current = class.to_string();
        let mut seen = std::collections::HashSet::new();
        loop {
            if !seen.insert(current.clone()) {
                break; // Cycle detection
            }
            let Some(path) = self.lookup_class_name(&current) else {
                break; // Hit a builtin or unknown class
            };
            let Some(extends) = self.file_extends(&path) else {
                break;
            };
            chain.push(extends.clone());
            current = extends;
        }
        chain
    }

    /// Create an empty workspace index for testing (no scanning).
    #[cfg(test)]
    pub fn new_empty() -> Self {
        Self {
            project_root: PathBuf::new(),
            files: DashMap::new(),
            class_names: DashMap::new(),
            autoloads: DashMap::new(),
            symbols: DashMap::new(),
            declarations: DashMap::new(),
            extends_map: DashMap::new(),
        }
    }

    /// Find all classes that directly extend the given class.
    pub fn subtypes(&self, class: &str) -> Vec<String> {
        let mut result = Vec::new();
        for entry in &self.extends_map {
            if let Some(ext) = entry.value()
                && ext == class
                && let Some(table) = self.symbols.get(entry.key())
                && let Some(cn) = &table.class_name
            {
                result.push(cn.clone());
            }
        }
        result
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
