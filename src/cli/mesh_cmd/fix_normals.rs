use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;
use crate::core::mesh::normals;

use super::{FixNormalsArgs, OutputFormat, project_root, run_eval};

pub fn cmd_fix_normals(args: &FixNormalsArgs) -> Result<()> {
    if args.all {
        return cmd_fix_normals_all(args);
    }

    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part_name = args.part.clone().unwrap_or_else(|| state.active.clone());

    let part = state.resolve_part_mut(args.part.as_deref())?;
    let total = part.mesh.face_count();
    let flipped = normals::fix_winding(&mut part.mesh);

    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&part_name)?;
    let _ = run_eval(&push)?;

    let result = serde_json::json!({
        "name": part_name,
        "faces_flipped": flipped,
        "total_faces": total,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            println!(
                "Fixed normals on {}: {}/{} faces corrected",
                part_name.cyan(),
                flipped.to_string().green(),
                total
            );
        }
    }
    Ok(())
}

fn cmd_fix_normals_all(args: &FixNormalsArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let mut results = Vec::new();
    let names: Vec<String> = state.parts.keys().cloned().collect();

    for name in &names {
        let part = state.parts.get_mut(name).unwrap();
        let total = part.mesh.face_count();
        let flipped = normals::fix_winding(&mut part.mesh);
        results.push(serde_json::json!({
            "name": name,
            "faces_flipped": flipped,
            "total_faces": total,
        }));
    }

    state.save(&root)?;

    // Push all parts to Godot (skip parts missing from scene)
    for name in &names {
        let push = state.generate_push_script(name)?;
        if let Err(e) = run_eval(&push) {
            eprintln!("Warning: skipping push for '{name}': {e}");
        }
    }

    let result = serde_json::json!({
        "parts_fixed": names.len(),
        "results": results,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            println!(
                "Fixed normals on {} parts:",
                names.len().to_string().green()
            );
            for r in &results {
                let name = r["name"].as_str().unwrap_or("?");
                let flipped = r["faces_flipped"].as_u64().unwrap_or(0);
                let total = r["total_faces"].as_u64().unwrap_or(0);
                println!(
                    "  {}: {}/{} faces corrected",
                    name.cyan(),
                    flipped.to_string().green(),
                    total
                );
            }
        }
    }
    Ok(())
}
