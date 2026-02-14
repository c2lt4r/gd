use clap::Args;
use miette::{Result, miette};
use owo_colors::OwoColorize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::scene;

#[derive(Args)]
pub struct TreeArgs {
    /// Files or directories to analyze (defaults to current directory)
    pub paths: Vec<String>,
    /// Show only class names, skip signals and methods
    #[arg(long)]
    pub classes_only: bool,
    /// Output format
    #[arg(long, default_value = "tree")]
    pub format: String,
    /// Show scene node hierarchy from a .tscn file (or directory of .tscn files)
    #[arg(long)]
    pub scene: Option<String>,
}

#[derive(Debug, Serialize)]
struct ClassInfo {
    name: String,
    file: String,
    extends: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    signals: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    methods: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    properties: Vec<String>,
}

#[derive(Serialize)]
struct TreeOutput {
    files: usize,
    classes: Vec<ClassInfo>,
}

pub fn exec(args: &TreeArgs) -> Result<()> {
    // Scene hierarchy mode
    if let Some(ref scene_path) = args.scene {
        return exec_scene(scene_path, &args.format);
    }

    // Determine root directory
    let root = if args.paths.is_empty() {
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?
    } else {
        PathBuf::from(&args.paths[0])
    };

    // Collect all .gd files
    let files = crate::core::fs::collect_gdscript_files(&root)?;

    if files.is_empty() {
        println!("No GDScript files found in {}", root.display());
        return Ok(());
    }

    // Parse each file and extract class info
    let mut classes = Vec::new();
    for file_path in &files {
        if let Ok(class_info) = extract_class_info(file_path) {
            classes.push(class_info);
        }
    }

    // Render output
    if args.format == "json" {
        let output = TreeOutput {
            files: files.len(),
            classes,
        };
        let json = serde_json::to_string_pretty(&output)
            .map_err(|e| miette!("Failed to serialize JSON: {e}"))?;
        println!("{json}");
    } else {
        render_tree(&root, files.len(), &classes, args.classes_only);
    }

    Ok(())
}

fn extract_class_info(path: &Path) -> Result<ClassInfo> {
    let (source, tree) = crate::core::parser::parse_file(path)?;
    let root = tree.root_node();

    let mut class_name = None;
    let mut extends = None;
    let mut signals = Vec::new();
    let mut methods = Vec::new();
    let mut properties = Vec::new();

    // Walk the root node's named children
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        match child.kind() {
            "class_name_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    class_name = Some(name_node.utf8_text(source.as_bytes()).unwrap().to_string());
                }
            }
            "extends_statement" => {
                // Get the type being extended
                for i in 0..child.named_child_count() {
                    if let Some(type_node) = child.named_child(i)
                        && (type_node.kind() == "type" || type_node.kind() == "identifier")
                    {
                        extends = Some(type_node.utf8_text(source.as_bytes()).unwrap().to_string());
                        break;
                    }
                }
            }
            "function_definition" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    let name = name_node.utf8_text(source.as_bytes()).unwrap();
                    // Skip private methods (starting with _) except built-ins
                    if !name.starts_with('_') || is_builtin_method(name) {
                        methods.push(name.to_string());
                    }
                }
            }
            "signal_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    signals.push(name_node.utf8_text(source.as_bytes()).unwrap().to_string());
                }
            }
            "variable_statement" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    properties.push(name_node.utf8_text(source.as_bytes()).unwrap().to_string());
                }
            }
            _ => {}
        }
    }

    // Fallback: use filename as class name if not explicitly defined
    let final_class_name = class_name.unwrap_or_else(|| {
        path.file_stem()
            .and_then(|s| s.to_str())
            .map_or_else(|| "Unknown".to_string(), to_pascal_case)
    });

    let file_name = path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown.gd")
        .to_string();

    Ok(ClassInfo {
        name: final_class_name,
        file: file_name,
        extends,
        signals,
        methods,
        properties,
    })
}

fn is_builtin_method(name: &str) -> bool {
    matches!(
        name,
        "_ready"
            | "_process"
            | "_physics_process"
            | "_input"
            | "_unhandled_input"
            | "_enter_tree"
            | "_exit_tree"
            | "_init"
            | "_notification"
    )
}

fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

fn render_tree(root: &Path, file_count: usize, classes: &[ClassInfo], classes_only: bool) {
    // Print header
    let project_name = root
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("Project");
    println!(
        "{} {} ({} {})",
        "Project:".bold(),
        project_name.cyan().bold(),
        file_count,
        if file_count == 1 { "file" } else { "files" }
    );
    println!();

    // Group classes by their base class
    let mut hierarchy: HashMap<String, Vec<&ClassInfo>> = HashMap::new();
    for class in classes {
        let base = class.extends.as_deref().unwrap_or("(no extends)");
        hierarchy.entry(base.to_string()).or_default().push(class);
    }

    // Sort base classes
    let mut bases: Vec<_> = hierarchy.keys().collect();
    bases.sort();

    // Render each base class group
    for (i, base) in bases.iter().enumerate() {
        let is_last_base = i == bases.len() - 1;

        // Print base class name
        println!("  {}", base.green().bold());

        if let Some(children) = hierarchy.get(*base) {
            let child_count = children.len();
            for (j, class) in children.iter().enumerate() {
                let is_last_child = j == child_count - 1;
                let prefix = if is_last_child {
                    "└──"
                } else {
                    "├──"
                };
                let continuation = if is_last_child { "    " } else { "│   " };

                // Print class name and file
                println!(
                    "  {} {} ({})",
                    prefix,
                    class.name.cyan(),
                    class.file.dimmed()
                );

                if !classes_only {
                    // Print signals
                    if !class.signals.is_empty() {
                        println!(
                            "  {}   {}: {}",
                            continuation,
                            "signals".dimmed(),
                            class.signals.join(", ")
                        );
                    }

                    // Print methods
                    if !class.methods.is_empty() {
                        println!(
                            "  {}   {}: {}",
                            continuation,
                            "methods".dimmed(),
                            class.methods.join(", ")
                        );
                    }

                    // Print properties
                    if !class.properties.is_empty() {
                        println!(
                            "  {}   {}: {}",
                            continuation,
                            "properties".dimmed(),
                            class.properties.join(", ")
                        );
                    }
                }
            }
        }

        // Add spacing between base class groups
        if !is_last_base {
            println!();
        }
    }
}

// ---------------------------------------------------------------------------
// Scene hierarchy mode (--scene)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct SceneTreeNode {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    script: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<SceneTreeNode>,
}

fn exec_scene(scene_path: &str, format: &str) -> Result<()> {
    let path = PathBuf::from(scene_path);

    if path.is_dir() {
        // Show all scenes in the directory
        let files = crate::core::fs::collect_resource_files(&path)?;
        let tscn_files: Vec<_> = files
            .iter()
            .filter(|f| f.extension().is_some_and(|e| e == "tscn"))
            .collect();

        if tscn_files.is_empty() {
            println!("No .tscn files found in {}", path.display());
            return Ok(());
        }

        if format == "json" {
            let mut scenes = Vec::new();
            for file in &tscn_files {
                if let Ok(data) = scene::parse_scene_file(file) {
                    let rel = crate::core::fs::relative_slash(file, &path);
                    let tree = build_scene_tree(&data);
                    scenes.push(serde_json::json!({
                        "file": rel,
                        "root": tree,
                    }));
                }
            }
            let json = serde_json::to_string_pretty(&scenes)
                .map_err(|e| miette!("Failed to serialize JSON: {e}"))?;
            println!("{json}");
        } else {
            for (i, file) in tscn_files.iter().enumerate() {
                let rel = crate::core::fs::relative_slash(file, &path);
                if let Ok(data) = scene::parse_scene_file(file) {
                    let tree = build_scene_tree(&data);
                    println!("{}", rel.cyan().bold());
                    render_scene_node(&tree, "", true);
                    if i < tscn_files.len() - 1 {
                        println!();
                    }
                }
            }
        }
    } else {
        // Single .tscn file
        let data = scene::parse_scene_file(&path)?;

        if format == "json" {
            let tree = build_scene_tree(&data);
            let json = serde_json::to_string_pretty(&tree)
                .map_err(|e| miette!("Failed to serialize JSON: {e}"))?;
            println!("{json}");
        } else {
            let tree = build_scene_tree(&data);
            println!("{} {}", "Scene:".bold(), scene_path.cyan().bold());
            render_scene_node(&tree, "", true);
        }
    }

    Ok(())
}

/// Build a nested tree from the flat list of scene nodes.
fn build_scene_tree(data: &scene::SceneData) -> SceneTreeNode {
    if data.nodes.is_empty() {
        return SceneTreeNode {
            name: "(empty)".to_string(),
            r#type: None,
            script: None,
            children: Vec::new(),
        };
    }

    // The root node is the one without a parent
    let root_node = &data.nodes[0];
    let mut root = SceneTreeNode {
        name: root_node.name.clone(),
        r#type: root_node.type_name.clone(),
        script: script_display(root_node.script.as_ref()),
        children: Vec::new(),
    };

    // Build children by parent path
    // parent="." means direct child of root
    // parent="NodeA" means child of NodeA
    // parent="NodeA/NodeB" means child of NodeA/NodeB
    add_children(&mut root, "", &data.nodes[1..]);

    root
}

/// Recursively add children to their parent nodes.
fn add_children(parent: &mut SceneTreeNode, parent_path: &str, remaining: &[scene::SceneNode]) {
    for node in remaining {
        let node_parent = node.parent.as_deref().unwrap_or("");
        if node_parent == "." && parent_path.is_empty()
            || node_parent == parent_path && !parent_path.is_empty()
        {
            let child_path = if parent_path.is_empty() {
                node.name.clone()
            } else {
                format!("{}/{}", parent_path, node.name)
            };

            let mut child = SceneTreeNode {
                name: node.name.clone(),
                r#type: node.type_name.clone(),
                script: script_display(node.script.as_ref()),
                children: Vec::new(),
            };
            add_children(&mut child, &child_path, remaining);
            parent.children.push(child);
        }
    }
}

/// Extract a human-readable script path from the script property value.
fn script_display(script: Option<&String>) -> Option<String> {
    script.cloned()
}

fn render_scene_node(node: &SceneTreeNode, prefix: &str, is_last: bool) {
    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let type_str = node
        .r#type
        .as_deref()
        .map(|t| format!(" ({})", t.green()))
        .unwrap_or_default();

    let script_str = node
        .script
        .as_deref()
        .map(|s| format!(" {}", s.dimmed()))
        .unwrap_or_default();

    println!(
        "{}{}{}{type_str}{script_str}",
        prefix,
        connector,
        node.name.cyan(),
    );

    let child_prefix = if prefix.is_empty() {
        "  ".to_string()
    } else if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    for (i, child) in node.children.iter().enumerate() {
        render_scene_node(child, &child_prefix, i == node.children.len() - 1);
    }
}
