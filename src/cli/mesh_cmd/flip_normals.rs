use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;
use crate::core::mesh::normals;

use super::{FlipNormalsArgs, OutputFormat, match_part_pattern, project_root, run_eval};
use crate::{ceprintln, cprintln};

pub fn cmd_flip_normals(args: &FlipNormalsArgs) -> Result<()> {
    if args.all {
        return cmd_flip_normals_all(args);
    }
    if args.parts.is_some() {
        return cmd_flip_normals_pattern(args);
    }

    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part_name = args.part.clone().unwrap_or_else(|| state.active.clone());

    let part = state.resolve_part_mut(args.part.as_deref())?;
    let fc = part.mesh.face_count();

    let flipped = if let Some(ref caps_axis) = args.caps {
        normals::flip_caps(&mut part.mesh, caps_axis.as_index())
    } else {
        normals::flip_all(&mut part.mesh);
        fc
    };

    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&part_name)?;
    let _ = run_eval(&push)?;

    let result = serde_json::json!({
        "name": part_name,
        "flipped_faces": flipped,
        "face_count": fc,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            if args.caps.is_some() {
                cprintln!(
                    "Flipped {flipped}/{fc} cap faces on {}",
                    part_name.green().bold()
                );
            } else {
                cprintln!(
                    "Flipped normals on {}: {fc} faces",
                    part_name.green().bold()
                );
            }
        }
    }
    Ok(())
}

fn cmd_flip_normals_pattern(args: &FlipNormalsArgs) -> Result<()> {
    let pattern = args.parts.as_deref().unwrap_or("");
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let all_names: Vec<String> = state.parts.keys().cloned().collect();
    let matched = match_part_pattern(&all_names, pattern);

    if matched.is_empty() {
        return Err(miette::miette!("No parts match pattern '{pattern}'"));
    }

    let mut results = Vec::new();
    for name in &matched {
        let part = state.parts.get_mut(*name).unwrap();
        let fc = part.mesh.face_count();
        let flipped = if let Some(ref caps_axis) = args.caps {
            normals::flip_caps(&mut part.mesh, caps_axis.as_index())
        } else {
            normals::flip_all(&mut part.mesh);
            fc
        };
        results.push(serde_json::json!({
            "name": name,
            "flipped_faces": flipped,
            "face_count": fc,
        }));
    }

    state.save(&root)?;

    for name in &matched {
        let push = state.generate_push_script(name)?;
        if let Err(e) = run_eval(&push) {
            ceprintln!("Warning: skipping push for '{name}': {e}");
        }
    }

    let result = serde_json::json!({
        "pattern": pattern,
        "parts_flipped": matched.len(),
        "results": results,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Flipped normals on {} parts matching {}:",
                matched.len().to_string().green(),
                pattern.cyan()
            );
            for r in &results {
                let name = r["name"].as_str().unwrap_or("?");
                let faces = r["face_count"].as_u64().unwrap_or(0);
                cprintln!("  {}: {faces} faces", name.cyan());
            }
        }
    }
    Ok(())
}

fn cmd_flip_normals_all(args: &FlipNormalsArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let mut results = Vec::new();
    let names: Vec<String> = state.parts.keys().cloned().collect();

    for name in &names {
        let part = state.parts.get_mut(name).unwrap();
        let fc = part.mesh.face_count();
        normals::flip_all(&mut part.mesh);
        results.push(serde_json::json!({
            "name": name,
            "face_count": fc,
        }));
    }

    state.save(&root)?;

    // Push all parts to Godot (skip parts missing from scene)
    for name in &names {
        let push = state.generate_push_script(name)?;
        if let Err(e) = run_eval(&push) {
            ceprintln!("Warning: skipping push for '{name}': {e}");
        }
    }

    let result = serde_json::json!({
        "parts_flipped": names.len(),
        "results": results,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Flipped normals on {} parts:",
                names.len().to_string().green()
            );
            for r in &results {
                let name = r["name"].as_str().unwrap_or("?");
                let faces = r["face_count"].as_u64().unwrap_or(0);
                cprintln!("  {}: {faces} faces", name.cyan());
            }
        }
    }
    Ok(())
}
