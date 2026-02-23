use std::path::{Path, PathBuf};
use std::sync::Arc;

use dashmap::DashMap;
use serde::Serialize;
use tower_lsp::lsp_types::InitializeParams;

use crate::core::scene::SceneData;
use crate::core::symbol_table::{self, SymbolTable};

/// Autoload singleton metadata resolved during workspace scan.
pub struct AutoloadInfo {
    /// The `class_name` declared in the autoload script (if any).
    pub class_name: Option<String>,
    /// Absolute filesystem path to the autoload script.
    pub path: PathBuf,
}

/// A node in a scene that has a script attached.
#[derive(Debug, Clone, Serialize)]
pub struct ScenePathNode {
    /// Absolute path to the .tscn file.
    pub scene_path: PathBuf,
    /// Node name in the scene.
    pub node_name: String,
    /// Node type (e.g., `CharacterBody3D`).
    pub node_type: Option<String>,
    /// Node parent path (e.g., `.` for root children).
    pub node_parent: Option<String>,
    /// Line number (0-based) of the `script = ExtResource(...)` in the .tscn.
    pub script_line: Option<usize>,
}

/// A signal connection found in a .tscn file.
#[derive(Debug, Clone, Serialize)]
pub struct SceneConnection {
    /// Signal name (e.g., `body_entered`).
    pub signal: String,
    /// Node path the signal comes from.
    pub from_node: String,
    /// Node path the signal connects to.
    pub to_node: String,
    /// Handler function name (e.g., `_on_body_entered`).
    pub method: String,
    /// Absolute path to the .tscn file.
    pub scene_path: PathBuf,
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
    /// .tscn path → parsed scene data.
    scenes: DashMap<PathBuf, Arc<SceneData>>,
    /// `res://` script path → list of .tscn files that attach it to a node.
    script_to_scenes: DashMap<String, Vec<ScenePathNode>>,
    /// .tscn path → list of signal connections.
    scene_connections: DashMap<PathBuf, Vec<SceneConnection>>,
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
            scenes: DashMap::new(),
            script_to_scenes: DashMap::new(),
            scene_connections: DashMap::new(),
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

        // Scan .tscn files for scene data
        if let Ok(scene_files) = crate::core::fs::collect_resource_files(&self.project_root) {
            for path in scene_files {
                if path.extension().is_some_and(|e| e == "tscn")
                    && let Ok(content) = std::fs::read_to_string(&path)
                {
                    self.index_scene(&path, &content);
                }
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

    // ── Scene indexing ─────────────────────────────────────────────────────

    /// Parse a .tscn file and populate scene-related DashMaps.
    fn index_scene(&self, path: &Path, content: &str) {
        let Ok(scene_data) = crate::core::scene::parse_scene(content) else {
            return;
        };

        // Build ext_resource id → res:// path lookup
        let ext_map: std::collections::HashMap<&str, &str> = scene_data
            .ext_resources
            .iter()
            .map(|ext| (ext.id.as_str(), ext.path.as_str()))
            .collect();

        // Index nodes with scripts → script_to_scenes reverse map
        for node in &scene_data.nodes {
            if let Some(ref script_val) = node.script {
                // Extract ExtResource id: `ExtResource("1_abc")` → `1_abc`
                if let Some(ext_id) = extract_ext_resource_id(script_val)
                    && let Some(&res_path) = ext_map.get(ext_id)
                {
                    // Find line number of the script property in the source
                    let script_line = find_script_line(content, &node.name);
                    let entry = ScenePathNode {
                        scene_path: path.to_path_buf(),
                        node_name: node.name.clone(),
                        node_type: node.type_name.clone(),
                        node_parent: node.parent.clone(),
                        script_line,
                    };
                    self.script_to_scenes
                        .entry(res_path.to_string())
                        .or_default()
                        .push(entry);
                }
            }
        }

        // Index signal connections
        let connections: Vec<SceneConnection> = scene_data
            .connections
            .iter()
            .map(|conn| SceneConnection {
                signal: conn.signal.clone(),
                from_node: conn.from.clone(),
                to_node: conn.to.clone(),
                method: conn.method.clone(),
                scene_path: path.to_path_buf(),
            })
            .collect();
        if !connections.is_empty() {
            self.scene_connections
                .insert(path.to_path_buf(), connections);
        }

        self.scenes
            .insert(path.to_path_buf(), Arc::new(scene_data));
    }

    /// Re-index a .tscn file from new content (for did_change/did_save).
    pub fn update_scene(&self, path: &Path, content: &str) {
        // Remove old entries for this scene
        self.remove_scene_entries(path);
        // Re-index
        self.index_scene(path, content);
    }

    /// Remove all reverse-map entries for a scene file.
    fn remove_scene_entries(&self, path: &Path) {
        self.scenes.remove(path);
        self.scene_connections.remove(path);

        // Remove script_to_scenes entries that reference this scene
        self.script_to_scenes.retain(|_, entries| {
            entries.retain(|e| e.scene_path != path);
            !entries.is_empty()
        });
    }

    // ── Scene query API ─────────────────────────────────────────────────────

    /// Find all scenes that use a given script (by `res://` path or absolute path).
    pub fn scenes_for_script(&self, script_path: &Path) -> Vec<ScenePathNode> {
        // Try as res:// path
        let res_path = self.to_res_path(script_path);
        if let Some(entries) = res_path.as_ref().and_then(|rp| self.script_to_scenes.get(rp)) {
            return entries.clone();
        }
        Vec::new()
    }

    /// Get parsed scene data for a .tscn file.
    pub fn get_scene(&self, scene_path: &Path) -> Option<Arc<SceneData>> {
        self.scenes.get(scene_path).map(|r| Arc::clone(r.value()))
    }

    /// Find signal connections where `method == handler_name` across scenes
    /// that use the given script.
    pub fn signal_connections_for_handler(
        &self,
        script_path: &Path,
        handler_name: &str,
    ) -> Vec<SceneConnection> {
        let mut results = Vec::new();
        let scene_nodes = self.scenes_for_script(script_path);
        for spn in &scene_nodes {
            if let Some(connections) = self.scene_connections.get(&spn.scene_path) {
                for conn in connections.iter() {
                    if conn.method == handler_name {
                        results.push(conn.clone());
                    }
                }
            }
        }
        results
    }

    /// Find signal connections where `signal == signal_name` across scenes
    /// that use the given script.
    pub fn signal_connections_for_signal(
        &self,
        script_path: &Path,
        signal_name: &str,
    ) -> Vec<SceneConnection> {
        let mut results = Vec::new();
        let scene_nodes = self.scenes_for_script(script_path);
        for spn in &scene_nodes {
            if let Some(connections) = self.scene_connections.get(&spn.scene_path) {
                for conn in connections.iter() {
                    if conn.signal == signal_name {
                        results.push(conn.clone());
                    }
                }
            }
        }
        results
    }

    /// Resolve a node path in a scene to its (name, type).
    #[allow(dead_code)]
    pub fn resolve_node_path(
        &self,
        scene_path: &Path,
        node_path: &str,
    ) -> Option<(String, Option<String>)> {
        let scene = self.get_scene(scene_path)?;
        resolve_node_in_scene(&scene, node_path)
    }

    /// Get all nodes in a scene: (name, type, full_path).
    pub fn scene_nodes(&self, scene_path: &Path) -> Vec<(String, Option<String>, String)> {
        let Some(scene) = self.get_scene(scene_path) else {
            return Vec::new();
        };
        scene
            .nodes
            .iter()
            .map(|n| {
                let full_path = build_node_full_path(&n.name, n.parent.as_deref());
                (n.name.clone(), n.type_name.clone(), full_path)
            })
            .collect()
    }

    /// Find the primary scene for a script (heuristic: single scene, or same base name).
    pub fn primary_scene_for_script(&self, script_path: &Path) -> Option<PathBuf> {
        let scenes = self.scenes_for_script(script_path);
        if scenes.is_empty() {
            return None;
        }
        if scenes.len() == 1 {
            return Some(scenes[0].scene_path.clone());
        }
        // Prefer scene with same base name (player.gd → player.tscn)
        let script_stem = script_path.file_stem()?.to_str()?;
        for spn in &scenes {
            if let Some(stem) = spn.scene_path.file_stem()
                && stem.to_str() == Some(script_stem)
            {
                return Some(spn.scene_path.clone());
            }
        }
        // Fallback: first scene
        Some(scenes[0].scene_path.clone())
    }

    /// Iterate over all scene connections (used by hover to find signal handlers).
    pub fn iter_scene_connections(
        &self,
    ) -> dashmap::iter::Iter<'_, PathBuf, Vec<SceneConnection>> {
        self.scene_connections.iter()
    }

    /// Iterate over all indexed scenes (used by hover for node path lookup).
    pub fn iter_scenes(&self) -> dashmap::iter::Iter<'_, PathBuf, Arc<SceneData>> {
        self.scenes.iter()
    }

    /// Convert an absolute path to a `res://` path relative to the project root.
    fn to_res_path(&self, path: &Path) -> Option<String> {
        let rel = path.strip_prefix(&self.project_root).ok()?;
        Some(format!("res://{}", rel.to_string_lossy().replace('\\', "/")))
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
            scenes: DashMap::new(),
            script_to_scenes: DashMap::new(),
            scene_connections: DashMap::new(),
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

/// Extract the id from `ExtResource("id")`.
fn extract_ext_resource_id(value: &str) -> Option<&str> {
    let inner = value
        .strip_prefix("ExtResource(\"")
        .or_else(|| value.strip_prefix("ExtResource( \""))?;
    let end = inner.find('"')?;
    Some(&inner[..end])
}

/// Find the 0-based line number of the `script = ExtResource(...)` property
/// in a .tscn source, searching after the `[node name="name"` header.
fn find_script_line(source: &str, node_name: &str) -> Option<usize> {
    let node_header = format!("[node name=\"{node_name}\"");
    let mut in_target_node = false;
    for (line_num, line) in source.lines().enumerate() {
        if line.contains(&node_header) {
            in_target_node = true;
            continue;
        }
        if in_target_node {
            if line.starts_with('[') {
                // Next section started — no script property found
                return None;
            }
            if line.starts_with("script = ") {
                return Some(line_num);
            }
        }
    }
    None
}

/// Resolve a node path like `"Player/Sprite2D"` in a parsed scene.
pub(super) fn resolve_node_in_scene(
    scene: &SceneData,
    node_path: &str,
) -> Option<(String, Option<String>)> {
    // Build full paths for each node
    for node in &scene.nodes {
        let full = build_node_full_path(&node.name, node.parent.as_deref());
        if full == node_path || node.name == node_path {
            return Some((node.name.clone(), node.type_name.clone()));
        }
    }
    None
}

/// Build the full scene-tree path for a node given its name and parent.
fn build_node_full_path(name: &str, parent: Option<&str>) -> String {
    match parent {
        // Root node or direct child of root
        None | Some(".") => name.to_string(),
        Some(p) => format!("{p}/{name}"),      // Deeper nesting
    }
}
