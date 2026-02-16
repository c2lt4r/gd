mod add_connection;
mod add_node;
mod attach_script;
mod create;
mod detach_script;
mod remove_connection;
mod remove_node;
mod set_property;

#[cfg(test)]
mod tests;

use clap::{Args, Subcommand};
use miette::{Result, miette};

use crate::core::scene;

#[derive(Args)]
pub struct SceneArgs {
    #[command(subcommand)]
    pub command: SceneCommand,
}

#[derive(Subcommand)]
pub enum SceneCommand {
    /// Attach a GDScript file to a node in a .tscn scene
    AttachScript(AttachScriptArgs),
    /// Create a new .tscn scene file
    Create(CreateArgs),
    /// Add a node to a scene
    AddNode(AddNodeArgs),
    /// Remove a node (and its descendants) from a scene
    RemoveNode(RemoveNodeArgs),
    /// Detach a script from a node in a scene
    DetachScript(DetachScriptArgs),
    /// Set a property on a node in a scene
    SetProperty(SetPropertyArgs),
    /// Add a signal connection to a scene
    AddConnection(AddConnectionArgs),
    /// Remove a signal connection from a scene
    RemoveConnection(RemoveConnectionArgs),
}

#[derive(Args)]
pub struct AttachScriptArgs {
    /// Path to the .tscn scene file
    pub scene: String,
    /// Path to the .gd script file
    pub script: String,
    /// Node name to attach the script to (defaults to root node)
    #[arg(long)]
    pub node: Option<String>,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Path for the new .tscn scene file
    pub path: String,
    /// Type of the root node (e.g. Node2D, Node3D, Control)
    #[arg(long)]
    pub root_type: String,
    /// Name of the root node (defaults to PascalCase of filename)
    #[arg(long)]
    pub root_name: Option<String>,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct AddNodeArgs {
    /// Path to the .tscn scene file
    pub scene: String,
    /// Name of the new node
    #[arg(long)]
    pub name: String,
    /// Type of the new node (e.g. Sprite2D, CharacterBody2D)
    #[arg(long = "type")]
    pub node_type: String,
    /// Parent node name (defaults to root)
    #[arg(long)]
    pub parent: Option<String>,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct RemoveNodeArgs {
    /// Path to the .tscn scene file
    pub scene: String,
    /// Name of the node to remove
    #[arg(long)]
    pub name: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct DetachScriptArgs {
    /// Path to the .tscn scene file
    pub scene: String,
    /// Node name to detach the script from (defaults to root node)
    #[arg(long)]
    pub node: Option<String>,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct SetPropertyArgs {
    /// Path to the .tscn scene file
    pub scene: String,
    /// Node name to set the property on
    #[arg(long)]
    pub node: String,
    /// Property key
    #[arg(long)]
    pub key: String,
    /// Property value (Godot resource format, e.g. Vector2(100, 200), true, 42)
    #[arg(long)]
    pub value: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct AddConnectionArgs {
    /// Path to the .tscn scene file
    pub scene: String,
    /// Signal name
    #[arg(long)]
    pub signal: String,
    /// Source node name (emitter)
    #[arg(long)]
    pub from: String,
    /// Target node name (receiver)
    #[arg(long)]
    pub to: String,
    /// Method name on the target node
    #[arg(long)]
    pub method: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct RemoveConnectionArgs {
    /// Path to the .tscn scene file
    pub scene: String,
    /// Signal name
    #[arg(long)]
    pub signal: String,
    /// Source node name (emitter)
    #[arg(long)]
    pub from: String,
    /// Target node name (receiver)
    #[arg(long)]
    pub to: String,
    /// Method name on the target node
    #[arg(long)]
    pub method: String,
    /// Preview changes without writing
    #[arg(long)]
    pub dry_run: bool,
}

pub fn exec(args: &SceneArgs) -> Result<()> {
    match &args.command {
        SceneCommand::AttachScript(a) => attach_script::exec_attach_script(a),
        SceneCommand::Create(a) => create::exec_create(a),
        SceneCommand::AddNode(a) => add_node::exec_add_node(a),
        SceneCommand::RemoveNode(a) => remove_node::exec_remove_node(a),
        SceneCommand::DetachScript(a) => detach_script::exec_detach_script(a),
        SceneCommand::SetProperty(a) => set_property::exec_set_property(a),
        SceneCommand::AddConnection(a) => add_connection::exec_add_connection(a),
        SceneCommand::RemoveConnection(a) => remove_connection::exec_remove_connection(a),
    }
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Compute the next ext_resource ID by incrementing the max numeric prefix.
pub(crate) fn next_ext_resource_id(ext_resources: &[scene::ExtResource]) -> String {
    let max_num = ext_resources
        .iter()
        .filter_map(|e| {
            let num_str: String = e.id.chars().take_while(char::is_ascii_digit).collect();
            num_str.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);
    (max_num + 1).to_string()
}

/// Check if a line starts a non-ext_resource section.
pub(crate) fn is_non_ext_section(line: &str) -> bool {
    (line.starts_with("[sub_resource")
        || line.starts_with("[node")
        || line.starts_with("[connection")
        || line.starts_with("[resource"))
        && !line.starts_with("[ext_resource")
}

/// Build a pattern to match the target node's section header.
pub(crate) fn build_node_pattern(node: &scene::SceneNode) -> String {
    format!("name=\"{}\"", node.name)
}

/// Increment the load_steps value in a gd_scene header line.
pub(crate) fn increment_load_steps(line: &str) -> String {
    if let Some(start) = line.find("load_steps=") {
        let after = &line[start + "load_steps=".len()..];
        let num_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        if let Ok(n) = after[..num_end].parse::<u32>() {
            return format!(
                "{}load_steps={}{}",
                &line[..start],
                n + 1,
                &after[num_end..]
            );
        }
    }
    line.to_string()
}

/// Decrement the load_steps value in a gd_scene header line.
pub(crate) fn decrement_load_steps(line: &str, amount: u32) -> String {
    if let Some(start) = line.find("load_steps=") {
        let after = &line[start + "load_steps=".len()..];
        let num_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        if let Ok(n) = after[..num_end].parse::<u32>() {
            let new_val = n.saturating_sub(amount);
            return format!(
                "{}load_steps={}{}",
                &line[..start],
                new_val,
                &after[num_end..]
            );
        }
    }
    line.to_string()
}

/// Read a scene file and parse it, returning the raw text and parsed data.
pub(crate) fn read_and_parse_scene(path: &std::path::Path) -> Result<(String, scene::SceneData)> {
    let source = std::fs::read_to_string(path)
        .map_err(|e| miette!("Failed to read {}: {e}", path.display()))?;
    let data = scene::parse_scene(&source)?;
    Ok((source, data))
}

/// Write content to a file, or print to stdout if dry_run is true.
pub(crate) fn write_or_dry_run(path: &std::path::Path, content: &str, dry_run: bool) -> Result<()> {
    if dry_run {
        print!("{content}");
        return Ok(());
    }
    std::fs::write(path, content).map_err(|e| miette!("Failed to write {}: {e}", path.display()))
}

/// Compute the full node path from the parsed scene data.
/// Root node → ".", direct child → "NodeName", deeper → "Parent/Child".
pub(crate) fn compute_node_path(node: &scene::SceneNode, _data: &scene::SceneData) -> String {
    if node.parent.is_none() {
        // Root node
        return ".".to_string();
    }
    let parent = node.parent.as_deref().unwrap();
    if parent == "." {
        return node.name.clone();
    }
    format!("{}/{}", parent, node.name)
}

/// Compute the `parent=` attribute value for a child of the given target node.
pub(crate) fn parent_attr_for_node(target_name: &str, data: &scene::SceneData) -> Result<String> {
    // Find the target node
    let target = data
        .nodes
        .iter()
        .find(|n| n.name == target_name)
        .ok_or_else(|| miette!("Node '{}' not found in scene", target_name))?;

    // If target is root (no parent), children use parent="."
    if target.parent.is_none() {
        return Ok(".".to_string());
    }

    let parent = target.parent.as_deref().unwrap();
    if parent == "." {
        return Ok(target_name.to_string());
    }
    Ok(format!("{parent}/{target_name}"))
}

/// Find a node by name in the scene data, returning an error if not found.
pub(crate) fn find_node<'a>(
    data: &'a scene::SceneData,
    name: &str,
) -> Result<&'a scene::SceneNode> {
    data.nodes
        .iter()
        .find(|n| n.name == name)
        .ok_or_else(|| miette!("Node '{}' not found in scene", name))
}

/// Extract the ext_resource ID from a value like `ExtResource("1_abc")`.
pub(crate) fn extract_ext_resource_id(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    let inner = trimmed
        .strip_prefix("ExtResource(\"")?
        .strip_suffix("\")")?;
    Some(inner)
}

/// Remove consecutive blank lines, leaving at most one.
pub(crate) fn clean_double_blanks(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut prev_blank = false;
    for line in text.lines() {
        let blank = line.trim().is_empty();
        if blank && prev_blank {
            continue;
        }
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(line);
        prev_blank = blank;
    }
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Check if an ext_resource ID is referenced anywhere in the scene text
/// (in node sections or connection sections, as ExtResource("id")).
pub(crate) fn is_ext_resource_referenced(source: &str, ext_id: &str) -> bool {
    let pattern = format!("ExtResource(\"{ext_id}\")");
    // Check only in node/connection sections (not in the ext_resource declaration itself)
    let mut in_ext_section = false;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[ext_resource") {
            in_ext_section = true;
        } else if trimmed.starts_with('[') {
            in_ext_section = false;
        }
        if !in_ext_section && line.contains(&pattern) {
            return true;
        }
    }
    false
}
