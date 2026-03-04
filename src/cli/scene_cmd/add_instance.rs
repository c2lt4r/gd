use std::env;
use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_core::config::find_project_root;
use gd_core::cprintln;
use gd_core::scene;

use super::{
    AddInstanceArgs, clean_double_blanks, find_node, increment_load_steps, is_non_ext_section,
    next_ext_resource_id, parent_attr_for_node,
};

pub(crate) fn exec_add_instance(args: &AddInstanceArgs) -> Result<()> {
    let cwd = env::current_dir().unwrap_or_default();
    let project_root = find_project_root(&cwd)
        .ok_or_else(|| miette!("No project.godot found — run from a Godot project directory"))?;

    let scene_path = resolve_scene_path(&args.scene, &cwd, &project_root)?;
    let (source, data) = super::read_and_parse_scene(&scene_path)?;

    // Resolve instance path to res:// format
    let res_path = resolve_instance_res_path(&args.instance, &cwd, &project_root)?;

    // Determine node name
    let node_name = args.name.clone().unwrap_or_else(|| {
        let stem = std::path::Path::new(&args.instance)
            .file_stem()
            .map_or("Instance".to_string(), |s| s.to_string_lossy().to_string());
        super::create::to_pascal_case(&stem)
    });

    // Resolve parent
    let parent_attr = if let Some(ref parent_name) = args.parent {
        find_node(&data, parent_name)?;
        parent_attr_for_node(parent_name, &data)?
    } else {
        ".".to_string()
    };

    // Check for existing ext_resource with same path, or create new
    let ext_id = if let Some(existing) = data.ext_resources.iter().find(|e| e.path == res_path) {
        existing.id.clone()
    } else {
        next_ext_resource_id(&data.ext_resources)
    };

    let needs_new_ext = !data.ext_resources.iter().any(|e| e.path == res_path);

    let result = insert_instance(
        &source,
        &data,
        &node_name,
        &parent_attr,
        &res_path,
        &ext_id,
        needs_new_ext,
    )?;

    super::write_or_dry_run(&scene_path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Added instance '{}' ({}) to {}",
            "✓".green(),
            node_name.bold(),
            res_path,
            args.scene,
        );
    }

    Ok(())
}

/// Insert a scene instance node into the scene source.
pub(crate) fn insert_instance(
    source: &str,
    data: &scene::SceneData,
    name: &str,
    parent_attr: &str,
    res_path: &str,
    ext_id: &str,
    needs_new_ext: bool,
) -> Result<String> {
    // Check for duplicate sibling
    let has_duplicate = data
        .nodes
        .iter()
        .any(|n| n.name == name && n.parent.as_deref() == Some(parent_attr));
    if parent_attr == "." && data.nodes.first().is_some_and(|n| n.name == name) {
        return Err(miette!(
            "Node '{}' already exists under parent '{}'",
            name,
            parent_attr
        ));
    }
    if has_duplicate {
        return Err(miette!(
            "Node '{}' already exists under parent '{}'",
            name,
            parent_attr
        ));
    }

    let node_line = format!(
        "[node name=\"{name}\" parent=\"{parent_attr}\" instance=ExtResource(\"{ext_id}\")]"
    );

    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 5);

    let ext_line =
        format!("[ext_resource type=\"PackedScene\" path=\"{res_path}\" id=\"{ext_id}\"]");

    let mut ext_inserted = !needs_new_ext;
    let mut node_inserted = false;

    for line in &lines {
        let trimmed = line.trim();

        // Insert ext_resource before the first non-ext section
        if !ext_inserted && is_non_ext_section(trimmed) {
            result.push(ext_line.clone());
            result.push(String::new());
            ext_inserted = true;
        }

        // Increment load_steps if we're adding a new ext_resource
        if needs_new_ext && trimmed.starts_with("[gd_scene") && trimmed.contains("load_steps=") {
            result.push(increment_load_steps(trimmed));
            continue;
        }

        // Insert node before connections
        if !node_inserted && trimmed.starts_with("[connection") {
            if !result.last().is_some_and(|l| l.trim().is_empty()) {
                result.push(String::new());
            }
            result.push(node_line.clone());
            result.push(String::new());
            node_inserted = true;
        }

        result.push((*line).to_string());
    }

    // If no connections, append node at end
    if !node_inserted {
        if !result.last().is_some_and(|l| l.trim().is_empty()) {
            result.push(String::new());
        }
        result.push(node_line);
    }

    // If we never found a non-ext section for the ext_resource
    if !ext_inserted {
        result.push(String::new());
        result.push(ext_line);
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(clean_double_blanks(&output))
}

fn resolve_scene_path(
    scene_arg: &str,
    cwd: &std::path::Path,
    project_root: &std::path::Path,
) -> Result<PathBuf> {
    let p = PathBuf::from(scene_arg);
    if p.is_absolute() && p.exists() {
        return Ok(p);
    }
    let from_cwd = cwd.join(&p);
    if from_cwd.exists() {
        return Ok(from_cwd);
    }
    let from_root = project_root.join(&p);
    if from_root.exists() {
        return Ok(from_root);
    }
    Err(miette!("Scene file not found: {}", scene_arg))
}

fn resolve_instance_res_path(
    instance_arg: &str,
    cwd: &std::path::Path,
    project_root: &std::path::Path,
) -> Result<String> {
    // Already in res:// format
    if instance_arg.starts_with("res://") {
        return Ok(instance_arg.to_string());
    }

    let p = PathBuf::from(instance_arg);
    let abs = if p.is_absolute() {
        p
    } else {
        let from_cwd = cwd.join(&p);
        if from_cwd.exists() {
            from_cwd
        } else {
            project_root.join(&p)
        }
    };

    let rel = abs
        .strip_prefix(project_root)
        .map_err(|_| miette!("Instance scene is not inside the project root"))?;
    Ok(format!(
        "res://{}",
        path_slash::PathBufExt::to_slash_lossy(&rel.to_path_buf())
    ))
}
