//! Project-wide symbol index for cross-file resolution (Layer 3).
//!
//! Built once at lint time, shared read-only across rayon-parallel file linting.
//! Maps `class_name` declarations to their symbols, resolves `preload()` targets,
//! and parses `project.godot` autoloads.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::fs::collect_gdscript_files;
use crate::core::parser;
use crate::core::project::parse_autoloads;
use crate::core::symbol_table::{self, FuncDecl, SymbolTable, VarDecl};

/// Summary of a function parameter (no AST references).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ParamSummary {
    pub name: String,
    pub type_name: Option<String>,
}

/// Summary of a function declaration (no AST references).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FuncSummary {
    pub name: String,
    pub params: Vec<ParamSummary>,
    pub return_type: Option<String>,
    pub is_static: bool,
    /// `##` doc comment text, if any.
    pub doc: Option<String>,
}

/// Summary of a variable declaration (no AST references).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VarSummary {
    pub name: String,
    pub type_name: Option<String>,
    pub is_static: bool,
    pub is_constant: bool,
    /// `##` doc comment text, if any.
    pub doc: Option<String>,
}

/// All symbols extracted from a single `.gd` file.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct FileSymbols {
    pub path: PathBuf,
    pub class_name: Option<String>,
    pub extends: Option<String>,
    pub has_tool: bool,
    pub functions: Vec<FuncSummary>,
    pub variables: Vec<VarSummary>,
    pub signals: Vec<String>,
    pub enums: Vec<String>,
}

/// Project-wide symbol index. Immutable after construction.
#[derive(Debug)]
#[allow(dead_code)]
pub struct ProjectIndex {
    /// `class_name` → file symbols.
    classes: HashMap<String, FileSymbols>,
    /// Autoload name → file symbols.
    autoloads: HashMap<String, FileSymbols>,
    /// All indexed files (including those without a class_name).
    files: Vec<FileSymbols>,
    /// Project root directory.
    project_root: PathBuf,
}

impl ProjectIndex {
    /// Build a project index by scanning all `.gd` files under `project_root`.
    pub fn build(project_root: &Path) -> Self {
        let gd_files = collect_gdscript_files(project_root).unwrap_or_default();

        let mut files = Vec::with_capacity(gd_files.len());
        let mut classes = HashMap::new();

        for path in &gd_files {
            if let Some(fs) = parse_file_symbols(path) {
                if let Some(ref cn) = fs.class_name {
                    classes.insert(cn.clone(), fs.clone());
                }
                files.push(fs);
            }
        }

        // Parse autoloads from project.godot
        let project_file = project_root.join("project.godot");
        let mut autoloads = HashMap::new();
        for (name, res_path) in parse_autoloads(&project_file) {
            if res_path.starts_with("uid://") {
                // UID-based autoloads: can't resolve path, but register name as known.
                // Try to find a matching class_name in the project files.
                if let Some(fs) = classes.get(&name) {
                    autoloads.insert(name, fs.clone());
                } else {
                    autoloads.insert(
                        name.clone(),
                        FileSymbols {
                            path: PathBuf::new(),
                            class_name: Some(name),
                            extends: None,
                            has_tool: false,
                            functions: Vec::new(),
                            variables: Vec::new(),
                            signals: Vec::new(),
                            enums: Vec::new(),
                        },
                    );
                }
            } else if Path::new(&res_path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("gd"))
                && let Some(real_path) = resolve_res_path(&res_path, project_root)
                && let Some(fs) = files.iter().find(|f| f.path == real_path)
            {
                autoloads.insert(name, fs.clone());
            }
        }

        Self {
            classes,
            autoloads,
            files,
            project_root: project_root.to_path_buf(),
        }
    }

    /// Look up file symbols by `class_name`.
    pub fn lookup_class(&self, name: &str) -> Option<&FileSymbols> {
        self.classes.get(name)
    }

    /// Look up file symbols by autoload name.
    #[allow(dead_code)]
    pub fn lookup_autoload(&self, name: &str) -> Option<&FileSymbols> {
        self.autoloads.get(name)
    }

    /// Resolve a `res://` path to its file symbols.
    #[allow(dead_code)]
    pub fn resolve_preload(&self, res_path: &str) -> Option<&FileSymbols> {
        let real_path = resolve_res_path(res_path, &self.project_root)?;
        self.files.iter().find(|f| f.path == real_path)
    }

    /// Walk the extends chain for a class/extends name, returning user-defined
    /// ancestors until we reach an engine class (or run out of info).
    ///
    /// Does NOT include the starting class itself.
    #[allow(dead_code)]
    pub fn extends_chain(&self, class_or_extends: &str) -> Vec<&str> {
        let mut chain = Vec::new();
        let mut current = class_or_extends;

        // Limit iterations to avoid cycles
        for _ in 0..64 {
            // Try to find this class in our index
            let Some(fs) = self.lookup_class(current) else {
                break; // Not a user class — probably an engine class
            };

            // Walk to parent
            match fs.extends.as_deref() {
                Some(parent) => {
                    chain.push(parent);
                    current = parent;
                }
                None => break,
            }
        }

        chain
    }

    /// Look up the return type of a method on a user-defined class, walking
    /// the extends chain. Falls back to ClassDB.
    pub fn method_return_type(&self, class: &str, method: &str) -> Option<String> {
        // Check user classes first
        let mut current = class;
        for _ in 0..64 {
            if let Some(fs) = self.lookup_class(current) {
                if let Some(func) = fs.functions.iter().find(|f| f.name == method) {
                    return func
                        .return_type
                        .clone()
                        .or_else(|| Some("Variant".to_string()));
                }
                match fs.extends.as_deref() {
                    Some(parent) => current = parent,
                    None => break,
                }
            } else {
                break;
            }
        }
        // Fall back to ClassDB
        crate::class_db::method_return_type(current, method).map(String::from)
    }

    /// Check if a method is static on a user-defined class, walking the extends chain.
    pub fn method_is_static(&self, class: &str, method: &str) -> Option<bool> {
        let mut current = class;
        for _ in 0..64 {
            if let Some(fs) = self.lookup_class(current) {
                if let Some(func) = fs.functions.iter().find(|f| f.name == method) {
                    return Some(func.is_static);
                }
                match fs.extends.as_deref() {
                    Some(parent) => current = parent,
                    None => break,
                }
            } else {
                break;
            }
        }
        None
    }

    /// Check if a method exists on a user-defined class (walking extends chain).
    #[allow(dead_code)]
    pub fn method_exists(&self, class: &str, method: &str) -> bool {
        let mut current = class;
        for _ in 0..64 {
            if let Some(fs) = self.lookup_class(current) {
                if fs.functions.iter().any(|f| f.name == method) {
                    return true;
                }
                match fs.extends.as_deref() {
                    Some(parent) => current = parent,
                    None => return false,
                }
            } else {
                return false;
            }
        }
        false
    }

    /// Look up a variable type on a user-defined class, walking the extends chain.
    #[allow(dead_code)]
    pub fn variable_type(&self, class: &str, var_name: &str) -> Option<String> {
        let mut current = class;
        for _ in 0..64 {
            if let Some(fs) = self.lookup_class(current) {
                if let Some(var) = fs.variables.iter().find(|v| v.name == var_name) {
                    return var.type_name.clone();
                }
                match fs.extends.as_deref() {
                    Some(parent) => current = parent,
                    None => break,
                }
            } else {
                break;
            }
        }
        None
    }

    /// Collect all variable names from a class and its user-defined base classes.
    pub fn all_variables(&self, class: &str) -> Vec<&VarSummary> {
        let mut result = Vec::new();
        let mut current = class;
        for _ in 0..64 {
            if let Some(fs) = self.lookup_class(current) {
                result.extend(fs.variables.iter());
                match fs.extends.as_deref() {
                    Some(parent) => current = parent,
                    None => break,
                }
            } else {
                break;
            }
        }
        result
    }

    /// Check if any class in the extends chain has `@tool`.
    pub fn has_tool_in_chain(&self, class_or_extends: &str) -> bool {
        let mut current = class_or_extends;
        for _ in 0..64 {
            if let Some(fs) = self.lookup_class(current) {
                if fs.has_tool {
                    return true;
                }
                match fs.extends.as_deref() {
                    Some(parent) => current = parent,
                    None => break,
                }
            } else {
                break;
            }
        }
        false
    }

    /// Returns true if the index is empty (no files indexed).
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// Returns a slice of all indexed files.
    pub fn files(&self) -> &[FileSymbols] {
        &self.files
    }

    /// Returns the project root directory.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// Check if a name matches an autoload (by autoload name or class_name).
    pub fn is_autoload(&self, name: &str) -> bool {
        self.autoloads.contains_key(name)
            || self
                .autoloads
                .values()
                .any(|fs| fs.class_name.as_deref() == Some(name))
    }
}

/// Parse a single `.gd` file into `FileSymbols`.
fn parse_file_symbols(path: &Path) -> Option<FileSymbols> {
    let (source, tree) = parser::parse_file(path).ok()?;
    let table = symbol_table::build(&tree, &source);
    Some(symbols_from_table(path.to_path_buf(), &table))
}

/// Convert a `SymbolTable` into the lighter `FileSymbols`.
fn symbols_from_table(path: PathBuf, table: &SymbolTable) -> FileSymbols {
    FileSymbols {
        path,
        class_name: table.class_name.clone(),
        extends: table.extends.clone(),
        has_tool: table.has_tool,
        functions: table.functions.iter().map(func_summary).collect(),
        variables: table.variables.iter().map(var_summary).collect(),
        signals: table.signals.iter().map(|s| s.name.clone()).collect(),
        enums: table.enums.iter().map(|e| e.name.clone()).collect(),
    }
}

fn func_summary(f: &FuncDecl) -> FuncSummary {
    FuncSummary {
        name: f.name.clone(),
        params: f
            .params
            .iter()
            .map(|p| ParamSummary {
                name: p.name.clone(),
                type_name: p
                    .type_ann
                    .as_ref()
                    .filter(|t| !t.is_inferred && !t.name.is_empty())
                    .map(|t| t.name.clone()),
            })
            .collect(),
        return_type: f.return_type.as_ref().map(|t| t.name.clone()),
        is_static: f.is_static,
        doc: f.doc.clone(),
    }
}

fn var_summary(v: &VarDecl) -> VarSummary {
    VarSummary {
        name: v.name.clone(),
        type_name: v
            .type_ann
            .as_ref()
            .filter(|t| !t.is_inferred && !t.name.is_empty())
            .map(|t| t.name.clone()),
        is_static: v.is_static,
        is_constant: v.is_constant,
        doc: v.doc.clone(),
    }
}

/// Resolve a `res://` path to an absolute filesystem path.
fn resolve_res_path(res_path: &str, project_root: &Path) -> Option<PathBuf> {
    let relative = res_path.strip_prefix("res://")?;
    let full = project_root.join(relative);
    if full.exists() { Some(full) } else { None }
}

/// Build a `ProjectIndex` from in-memory file contents (for testing).
#[cfg(test)]
pub fn build_from_sources(
    project_root: &Path,
    files: &[(PathBuf, &str)],
    autoloads: &[(&str, &str)],
) -> ProjectIndex {
    let mut all_files = Vec::with_capacity(files.len());
    let mut classes = HashMap::new();

    for (path, source) in files {
        if let Ok(tree) = parser::parse(source) {
            let table = symbol_table::build(&tree, source);
            let fs = symbols_from_table(path.clone(), &table);
            if let Some(ref cn) = fs.class_name {
                classes.insert(cn.clone(), fs.clone());
            }
            all_files.push(fs);
        }
    }

    let mut autoload_map = HashMap::new();
    for &(name, res_path) in autoloads {
        if let Some(real_path) = resolve_res_path(res_path, project_root)
            && let Some(fs) = all_files.iter().find(|f| f.path == real_path)
        {
            autoload_map.insert(name.to_string(), fs.clone());
        }
    }

    ProjectIndex {
        classes,
        autoloads: autoload_map,
        files: all_files,
        project_root: project_root.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_index(files: &[(&str, &str)]) -> ProjectIndex {
        let root = PathBuf::from("/test_project");
        let file_entries: Vec<(PathBuf, &str)> = files
            .iter()
            .map(|(name, src)| (root.join(name), *src))
            .collect();
        build_from_sources(&root, &file_entries, &[])
    }

    #[test]
    fn lookup_class_by_name() {
        let idx = make_index(&[(
            "player.gd",
            "class_name Player\nextends CharacterBody2D\nvar health: int\nfunc move() -> void:\n\tpass\n",
        )]);
        let fs = idx.lookup_class("Player").unwrap();
        assert_eq!(fs.class_name.as_deref(), Some("Player"));
        assert_eq!(fs.extends.as_deref(), Some("CharacterBody2D"));
        assert_eq!(fs.functions.len(), 1);
        assert_eq!(fs.functions[0].name, "move");
        assert_eq!(fs.variables.len(), 1);
        assert_eq!(fs.variables[0].name, "health");
    }

    #[test]
    fn lookup_class_not_found() {
        let idx = make_index(&[("script.gd", "extends Node\n")]);
        assert!(idx.lookup_class("Player").is_none());
    }

    #[test]
    fn extends_chain_user_classes() {
        let idx = make_index(&[
            (
                "base.gd",
                "class_name BaseEnemy\nextends CharacterBody2D\nfunc take_damage() -> void:\n\tpass\n",
            ),
            (
                "enemy.gd",
                "class_name Enemy\nextends BaseEnemy\nfunc attack() -> void:\n\tpass\n",
            ),
        ]);
        let chain = idx.extends_chain("Enemy");
        assert_eq!(chain, vec!["BaseEnemy", "CharacterBody2D"]);
    }

    #[test]
    fn extends_chain_engine_class() {
        let idx = make_index(&[("node.gd", "class_name MyNode\nextends Node\n")]);
        // "Node" is an engine class, chain stops at it
        let chain = idx.extends_chain("MyNode");
        assert_eq!(chain, vec!["Node"]);
    }

    #[test]
    fn method_return_type_user_class() {
        let idx = make_index(&[(
            "utils.gd",
            "class_name Utils\nfunc compute() -> int:\n\treturn 42\n",
        )]);
        assert_eq!(
            idx.method_return_type("Utils", "compute"),
            Some("int".to_string())
        );
    }

    #[test]
    fn method_return_type_inherited() {
        let idx = make_index(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc get_value() -> float:\n\treturn 1.0\n",
            ),
            ("child.gd", "class_name Child\nextends Base\n"),
        ]);
        assert_eq!(
            idx.method_return_type("Child", "get_value"),
            Some("float".to_string())
        );
    }

    #[test]
    fn method_return_type_classdb_fallback() {
        let idx = make_index(&[("node.gd", "class_name MyNode\nextends Node\n")]);
        // get_child is a ClassDB method on Node
        let ret = idx.method_return_type("MyNode", "get_child");
        assert!(ret.is_some());
    }

    #[test]
    fn method_return_type_no_annotation() {
        let idx = make_index(&[(
            "utils.gd",
            "class_name Utils\nfunc compute():\n\treturn 42\n",
        )]);
        // No return type annotation → Variant
        assert_eq!(
            idx.method_return_type("Utils", "compute"),
            Some("Variant".to_string())
        );
    }

    #[test]
    fn method_is_static_check() {
        let idx = make_index(&[(
            "factory.gd",
            "class_name Factory\nstatic func create() -> Node:\n\treturn Node.new()\nfunc build() -> void:\n\tpass\n",
        )]);
        assert_eq!(idx.method_is_static("Factory", "create"), Some(true));
        assert_eq!(idx.method_is_static("Factory", "build"), Some(false));
        assert_eq!(idx.method_is_static("Factory", "nonexistent"), None);
    }

    #[test]
    fn method_exists_check() {
        let idx = make_index(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nfunc base_method() -> void:\n\tpass\n",
            ),
            (
                "child.gd",
                "class_name Child\nextends Base\nfunc child_method() -> void:\n\tpass\n",
            ),
        ]);
        assert!(idx.method_exists("Child", "child_method"));
        assert!(idx.method_exists("Child", "base_method"));
        assert!(!idx.method_exists("Child", "nonexistent"));
    }

    #[test]
    fn variable_type_lookup() {
        let idx = make_index(&[(
            "player.gd",
            "class_name Player\nextends Node\nvar health: int\nvar speed := 5.0\nvar data\n",
        )]);
        assert_eq!(
            idx.variable_type("Player", "health"),
            Some("int".to_string())
        );
        // Inferred type `:=` is not captured as an explicit type
        assert_eq!(idx.variable_type("Player", "speed"), None);
        assert_eq!(idx.variable_type("Player", "data"), None);
    }

    #[test]
    fn all_variables_includes_base() {
        let idx = make_index(&[
            (
                "base.gd",
                "class_name Base\nextends Node\nvar base_var: int\n",
            ),
            (
                "child.gd",
                "class_name Child\nextends Base\nvar child_var: String\n",
            ),
        ]);
        let vars = idx.all_variables("Child");
        let names: Vec<&str> = vars.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"child_var"));
        assert!(names.contains(&"base_var"));
    }

    #[test]
    fn has_tool_in_chain_direct() {
        let idx = make_index(&[("tool.gd", "@tool\nclass_name ToolScript\nextends Node\n")]);
        assert!(idx.has_tool_in_chain("ToolScript"));
    }

    #[test]
    fn has_tool_in_chain_inherited() {
        let idx = make_index(&[
            ("base.gd", "@tool\nclass_name ToolBase\nextends Node\n"),
            ("child.gd", "class_name ToolChild\nextends ToolBase\n"),
        ]);
        assert!(idx.has_tool_in_chain("ToolChild"));
    }

    #[test]
    fn has_tool_in_chain_none() {
        let idx = make_index(&[("plain.gd", "class_name Plain\nextends Node\n")]);
        assert!(!idx.has_tool_in_chain("Plain"));
    }

    #[test]
    fn resolve_preload_path() {
        // Use real temp dir so files exist
        let dir = tempfile::tempdir().unwrap();
        let gd_path = dir.path().join("player.gd");
        std::fs::write(
            &gd_path,
            "class_name Player\nextends Node\nfunc f() -> void:\n\tpass\n",
        )
        .unwrap();

        let idx = ProjectIndex::build(dir.path());
        let fs = idx.resolve_preload("res://player.gd").unwrap();
        assert_eq!(fs.class_name.as_deref(), Some("Player"));
    }

    #[test]
    fn autoload_lookup() {
        let dir = tempfile::tempdir().unwrap();
        // Create autoload script
        let scripts_dir = dir.path().join("scripts");
        std::fs::create_dir(&scripts_dir).unwrap();
        std::fs::write(
            scripts_dir.join("global.gd"),
            "class_name GameGlobal\nextends Node\nvar score: int\n",
        )
        .unwrap();
        // Create project.godot with autoload
        std::fs::write(
            dir.path().join("project.godot"),
            "[application]\nconfig/name=\"Test\"\n\n[autoload]\nGame=\"*res://scripts/global.gd\"\n",
        )
        .unwrap();

        let idx = ProjectIndex::build(dir.path());
        let fs = idx.lookup_autoload("Game").unwrap();
        assert_eq!(fs.class_name.as_deref(), Some("GameGlobal"));
        assert_eq!(fs.variables[0].name, "score");
    }

    #[test]
    fn empty_project() {
        let dir = tempfile::tempdir().unwrap();
        let idx = ProjectIndex::build(dir.path());
        assert!(idx.is_empty());
        assert!(idx.lookup_class("Anything").is_none());
    }
}
