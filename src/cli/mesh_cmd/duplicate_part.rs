use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::gdscript;
use super::{DuplicatePartArgs, OutputFormat, inject_stats, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_duplicate_part(args: &DuplicatePartArgs) -> Result<()> {
    if args.group.is_some() {
        return cmd_duplicate_group(args);
    }

    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let src_name = args
        .name
        .as_deref()
        .ok_or_else(|| miette!("--name is required when not using --group"))?;

    // Clone the source part
    let src_part = state
        .parts
        .get(src_name)
        .ok_or_else(|| miette!("Part '{src_name}' not found"))?
        .clone();

    let mut new_part = src_part;

    // Apply mirror if requested
    let mirror_axis = args.symmetric.as_ref().or(args.mirror.as_ref());
    if let Some(axis) = mirror_axis {
        gd_mesh::mirror::mirror(&mut new_part.mesh, axis.as_index());
        new_part.transform.position[axis.as_index()] =
            -new_part.transform.position[axis.as_index()];
    }

    state.parts.insert(args.as_name.clone(), new_part);
    state.active.clone_from(&args.as_name);
    state.save(&root)?;

    // Create the Godot node via GDScript
    let symmetric = args.symmetric.is_some();
    let script = if let Some(axis) = mirror_axis {
        gdscript::generate_mirror_part(src_name, &args.as_name, axis.as_str(), symmetric)
    } else {
        gdscript::generate_duplicate_part(src_name, &args.as_name)
    };
    let result = run_eval(&script)?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Push the new part's mesh
    let push = state.generate_push_script(&args.as_name)?;
    let _ = run_eval(&push)?;

    // Fix vertex_count: GDScript reports 0 because mesh is pushed after node creation.
    if let Some(part) = state.parts.get(&args.as_name) {
        parsed["vertex_count"] = serde_json::json!(part.mesh.vertices.len());
    }
    inject_stats(&mut parsed, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let dst = &args.as_name;
            let pc = state.parts.len();
            if let Some(axis) = mirror_axis {
                cprintln!(
                    "Mirrored {} -> {} (axis={}, {pc} parts total)",
                    src_name.cyan(),
                    dst.green().bold(),
                    axis.as_str().yellow(),
                );
            } else {
                cprintln!(
                    "Duplicated {} -> {} ({pc} parts total)",
                    src_name.cyan(),
                    dst.green().bold(),
                );
            }
        }
    }
    Ok(())
}

fn cmd_duplicate_group(args: &DuplicatePartArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let group_name = args.group.as_deref().unwrap();
    let members = state
        .groups
        .get(group_name)
        .ok_or_else(|| miette!("Group '{group_name}' not found"))?
        .clone();

    let mirror_axis = args.symmetric.as_ref().or(args.mirror.as_ref());
    let mut new_members: Vec<String> = Vec::new();

    for src_name in &members {
        let new_name =
            if let (Some(find), Some(replace)) = (args.replace.as_deref(), args.with.as_deref()) {
                src_name.replace(find, replace)
            } else {
                format!("{src_name}-{}", args.as_name)
            };

        let src_part = state
            .parts
            .get(src_name)
            .ok_or_else(|| miette!("Part '{src_name}' not found (member of group '{group_name}')"))?
            .clone();

        let mut new_part = src_part;
        if let Some(axis) = mirror_axis {
            gd_mesh::mirror::mirror(&mut new_part.mesh, axis.as_index());
            new_part.transform.position[axis.as_index()] =
                -new_part.transform.position[axis.as_index()];
        }

        state.parts.insert(new_name.clone(), new_part);

        // Create Godot node + push mesh
        let symmetric = args.symmetric.is_some();
        let script = if let Some(axis) = mirror_axis {
            gdscript::generate_mirror_part(src_name, &new_name, axis.as_str(), symmetric)
        } else {
            gdscript::generate_duplicate_part(src_name, &new_name)
        };
        let _ = run_eval(&script);
        let push = state.generate_push_script(&new_name)?;
        let _ = run_eval(&push);

        new_members.push(new_name);
    }

    // Create new group
    state
        .groups
        .insert(args.as_name.clone(), new_members.clone());
    state.save(&root)?;

    let mut result = serde_json::json!({
        "source_group": group_name,
        "new_group": args.as_name,
        "members": new_members,
        "count": new_members.len(),
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Duplicated group {} -> {} ({} parts)",
                group_name.cyan(),
                args.as_name.green().bold(),
                new_members.len().to_string().cyan()
            );
        }
    }
    Ok(())
}
