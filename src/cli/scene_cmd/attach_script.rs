use std::env;
use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::config::find_project_root;
use crate::core::scene;

use crate::cprintln;

use super::{
    AttachScriptArgs, build_node_pattern, increment_load_steps, is_non_ext_section,
    next_ext_resource_id,
};

pub(crate) fn exec_attach_script(args: &AttachScriptArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let project_root = find_project_root(&cwd)
        .ok_or_else(|| miette!("No project.godot found — run from a Godot project directory"))?;

    // Resolve scene path: try CWD first, then project root
    let scene_path = resolve_path(&args.scene, &cwd, &project_root)
        .ok_or_else(|| miette!("Scene file not found: {}", args.scene))?;

    // Resolve script path: strip res:// prefix, try CWD first, then project root
    let script_arg = args.script.strip_prefix("res://").unwrap_or(&args.script);
    let script_path = resolve_path(script_arg, &cwd, &project_root)
        .ok_or_else(|| miette!("Script file not found: {}", args.script))?;

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
    let next_id = next_ext_resource_id(&data.ext_resources);

    // Build the modified file content
    let result = insert_script_attachment(&source, &res_path, &next_id, target_node)?;

    if args.dry_run {
        cprintln!("{result}");
        return Ok(());
    }

    std::fs::write(&scene_path, &result)
        .map_err(|e| miette!("Failed to write {}: {e}", args.scene))?;

    cprintln!(
        "{} Attached {} to node '{}' in {}",
        "✓".green(),
        script_path.display().bold(),
        target_node.name.bold(),
        args.scene,
    );

    Ok(())
}

/// Insert the ext_resource line and script property into the .tscn source.
pub(crate) fn insert_script_attachment(
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

/// Resolve a file path by trying CWD first, then project root.
fn resolve_path(
    path: &str,
    cwd: &std::path::Path,
    project_root: &std::path::Path,
) -> Option<PathBuf> {
    let p = PathBuf::from(path);
    // Absolute path
    if p.is_absolute() {
        return p.exists().then_some(p);
    }
    // Relative to CWD
    let from_cwd = cwd.join(&p);
    if from_cwd.exists() {
        return Some(from_cwd);
    }
    // Relative to project root
    let from_root = project_root.join(&p);
    if from_root.exists() {
        return Some(from_root);
    }
    None
}
