use std::env;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::config::find_project_root;
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

pub fn exec(args: &SceneArgs) -> Result<()> {
    match &args.command {
        SceneCommand::AttachScript(a) => exec_attach_script(a),
    }
}

fn exec_attach_script(args: &AttachScriptArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let scene_path = PathBuf::from(&args.scene);
    let script_path = PathBuf::from(&args.script);

    if !scene_path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }
    if !script_path.exists() {
        return Err(miette!("Script file not found: {}", args.script));
    }

    let project_root = find_project_root(&cwd)
        .ok_or_else(|| miette!("No project.godot found — run from a Godot project directory"))?;

    // Convert script path to res:// format
    let abs_script = if script_path.is_absolute() {
        script_path.clone()
    } else {
        cwd.join(&script_path)
    };
    let rel = abs_script
        .strip_prefix(&project_root)
        .map_err(|_| miette!("Script is not inside the project root"))?;
    let res_path = format!(
        "res://{}",
        path_slash::PathBufExt::to_slash_lossy(&rel.to_path_buf())
    );

    // Read and parse the scene
    let source = std::fs::read_to_string(&scene_path)
        .map_err(|e| miette!("Failed to read {}: {e}", args.scene))?;
    let data = scene::parse_scene(&source)?;

    // Find target node
    let target_node = if let Some(ref name) = args.node {
        data.nodes
            .iter()
            .find(|n| n.name == *name)
            .ok_or_else(|| miette!("Node '{}' not found in scene", name))?
    } else {
        data.nodes
            .first()
            .ok_or_else(|| miette!("Scene has no nodes"))?
    };

    // Check if node already has a script
    if target_node.script.is_some() {
        return Err(miette!(
            "Node '{}' already has a script attached",
            target_node.name
        ));
    }

    // Check if this script is already an ext_resource
    if data.ext_resources.iter().any(|e| e.path == res_path) {
        return Err(miette!(
            "Script '{}' is already an ext_resource in this scene",
            res_path
        ));
    }

    // Compute next ext_resource ID
    let next_id = next_ext_resource_id(&data);

    // Build the modified file content
    let result = insert_script_attachment(&source, &res_path, &next_id, target_node)?;

    if args.dry_run {
        println!("{result}");
        return Ok(());
    }

    std::fs::write(&scene_path, &result)
        .map_err(|e| miette!("Failed to write {}: {e}", args.scene))?;

    println!(
        "{} Attached {} to node '{}' in {}",
        "✓".green(),
        script_path.display().bold(),
        target_node.name.bold(),
        args.scene,
    );

    Ok(())
}

/// Compute the next ext_resource ID by incrementing the max numeric prefix.
fn next_ext_resource_id(data: &scene::SceneData) -> String {
    let max_num = data
        .ext_resources
        .iter()
        .filter_map(|e| {
            // IDs are like "1", "2_abc", "3_loading" — extract leading number
            let num_str: String = e.id.chars().take_while(char::is_ascii_digit).collect();
            num_str.parse::<u32>().ok()
        })
        .max()
        .unwrap_or(0);
    (max_num + 1).to_string()
}

/// Insert the ext_resource line and script property into the .tscn source.
fn insert_script_attachment(
    source: &str,
    res_path: &str,
    ext_id: &str,
    target_node: &scene::SceneNode,
) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 3);

    let ext_line = format!("[ext_resource type=\"Script\" path=\"{res_path}\" id=\"{ext_id}\"]");
    let script_prop = format!("script = ExtResource(\"{ext_id}\")");

    let target_pattern = build_node_pattern(target_node);

    let mut ext_inserted = false;
    let mut script_inserted = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Insert ext_resource before the first non-ext_resource section
        if !ext_inserted && is_non_ext_section(trimmed) {
            result.push(ext_line.clone());
            result.push(String::new());
            ext_inserted = true;
        }

        // Update load_steps in the gd_scene header
        if trimmed.starts_with("[gd_scene") && trimmed.contains("load_steps=") {
            result.push(increment_load_steps(trimmed));
            i += 1;
            continue;
        }

        result.push(line.to_string());

        // After the target node header, insert the script property
        if !script_inserted && trimmed.starts_with("[node ") && trimmed.contains(&target_pattern) {
            result.push(script_prop.clone());
            script_inserted = true;
        }

        i += 1;
    }

    // If we never found a non-ext section (scene has only ext_resources or is empty)
    if !ext_inserted {
        result.push(String::new());
        result.push(ext_line);
    }

    if !script_inserted {
        return Err(miette!("Could not find target node in scene text"));
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}

/// Check if a line starts a non-ext_resource section.
fn is_non_ext_section(line: &str) -> bool {
    (line.starts_with("[sub_resource")
        || line.starts_with("[node")
        || line.starts_with("[connection"))
        && !line.starts_with("[ext_resource")
}

/// Build a pattern to match the target node's section header.
fn build_node_pattern(node: &scene::SceneNode) -> String {
    format!("name=\"{}\"", node.name)
}

/// Increment the load_steps value in a gd_scene header line.
fn increment_load_steps(line: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::{ExtResource, SceneData, SceneNode};

    fn make_scene_data(ext_ids: &[&str], node_name: &str) -> SceneData {
        SceneData {
            ext_resources: ext_ids
                .iter()
                .map(|id| ExtResource {
                    id: (*id).to_string(),
                    type_name: "Script".to_string(),
                    path: format!("res://script_{id}.gd"),
                    uid: None,
                })
                .collect(),
            sub_resources: Vec::new(),
            nodes: vec![SceneNode {
                name: node_name.to_string(),
                type_name: Some("Node2D".to_string()),
                parent: None,
                instance: None,
                script: None,
                groups: Vec::new(),
                properties: Vec::new(),
            }],
            connections: Vec::new(),
        }
    }

    #[test]
    fn next_id_from_numeric_ids() {
        let data = make_scene_data(&["1", "2", "3"], "Root");
        assert_eq!(next_ext_resource_id(&data), "4");
    }

    #[test]
    fn next_id_from_suffixed_ids() {
        let data = make_scene_data(&["1_abc", "2_def", "3_loading"], "Root");
        assert_eq!(next_ext_resource_id(&data), "4");
    }

    #[test]
    fn next_id_empty_scene() {
        let data = make_scene_data(&[], "Root");
        assert_eq!(next_ext_resource_id(&data), "1");
    }

    #[test]
    fn increment_load_steps_basic() {
        let line = r#"[gd_scene load_steps=3 format=3 uid="uid://abc"]"#;
        let result = increment_load_steps(line);
        assert!(result.contains("load_steps=4"));
    }

    #[test]
    fn increment_load_steps_no_steps() {
        let line = r"[gd_scene format=3]";
        let result = increment_load_steps(line);
        assert_eq!(result, line);
    }

    #[test]
    fn attach_script_to_root() {
        let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Texture2D" path="res://icon.png" id="1"]

[node name="Root" type="Node2D"]

[node name="Child" type="Sprite2D" parent="."]
"#;
        let data = scene::parse_scene(source).unwrap();
        let result =
            insert_script_attachment(source, "res://root.gd", "2", &data.nodes[0]).unwrap();

        assert!(result.contains(r#"[ext_resource type="Script" path="res://root.gd" id="2"]"#));
        assert!(result.contains("load_steps=3"));
        // Script property should appear right after the root node header
        let lines: Vec<&str> = result.lines().collect();
        let node_idx = lines
            .iter()
            .position(|l| l.contains("name=\"Root\""))
            .unwrap();
        assert_eq!(lines[node_idx + 1], r#"script = ExtResource("2")"#);
    }

    #[test]
    fn attach_script_to_named_child() {
        let source = r#"[gd_scene format=3]

[node name="Root" type="Node2D"]

[node name="Player" type="CharacterBody2D" parent="."]
"#;
        let data = scene::parse_scene(source).unwrap();
        let result =
            insert_script_attachment(source, "res://player.gd", "1", &data.nodes[1]).unwrap();

        assert!(result.contains(r#"[ext_resource type="Script" path="res://player.gd" id="1"]"#));
        let lines: Vec<&str> = result.lines().collect();
        let node_idx = lines
            .iter()
            .position(|l| l.contains("name=\"Player\""))
            .unwrap();
        assert_eq!(lines[node_idx + 1], r#"script = ExtResource("1")"#);
    }

    #[test]
    fn attach_preserves_existing_ext_resources() {
        let source = r#"[gd_scene load_steps=2 format=3]

[ext_resource type="Script" path="res://existing.gd" id="1"]

[node name="Root" type="Node2D"]
script = ExtResource("1")

[node name="Enemy" type="CharacterBody2D" parent="."]
"#;
        let data = scene::parse_scene(source).unwrap();
        let result =
            insert_script_attachment(source, "res://enemy.gd", "2", &data.nodes[1]).unwrap();

        // Both ext_resources should be present
        assert!(result.contains(r#"path="res://existing.gd" id="1""#));
        assert!(result.contains(r#"path="res://enemy.gd" id="2""#));
        assert!(result.contains("load_steps=3"));
    }
}
