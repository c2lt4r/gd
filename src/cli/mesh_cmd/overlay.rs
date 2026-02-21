use miette::Result;
use owo_colors::OwoColorize;

use super::{OutputFormat, OverlayArgs, OverlayMode, gdscript, project_root, run_eval};
use crate::core::mesh::MeshState;
use crate::cprintln;

/// Data for the edge overlay GDScript generator.
pub struct EdgeOverlayData {
    pub positions: Vec<[f64; 3]>,
    pub boundary: Vec<(usize, usize)>,
    pub sharp: Vec<(usize, usize)>,
    pub interior: Vec<(usize, usize)>,
}

pub fn cmd_overlay(args: &OverlayArgs) -> Result<()> {
    match args.mode {
        OverlayMode::Edges => overlay_edges(&args.format),
        OverlayMode::Off => overlay_off(&args.format),
    }
}

fn overlay_edges(format: &OutputFormat) -> Result<()> {
    let root = project_root()?;
    let state = MeshState::load(&root)?;
    let part = state.active_part()?;
    let mesh = &part.mesh;

    let classified = mesh.classified_edges();
    let positions: Vec<[f64; 3]> = mesh.vertices.iter().map(|v| v.position).collect();

    let data = EdgeOverlayData {
        positions,
        boundary: classified.boundary,
        sharp: classified.sharp,
        interior: classified.interior,
    };

    let script = gdscript::generate_edge_overlay(&data);
    run_eval(&script)?;

    let mut result = serde_json::json!({
        "overlay": "edges",
        "boundary_edges": data.boundary.len(),
        "sharp_edges": data.sharp.len(),
        "interior_edges": data.interior.len(),
    });
    super::inject_stats(&mut result, &state);

    match format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Edge overlay: {} boundary, {} sharp, {} interior",
                data.boundary.len().to_string().red().bold(),
                data.sharp.len().to_string().yellow().bold(),
                data.interior.len().to_string().green()
            );
        }
    }

    Ok(())
}

fn overlay_off(format: &OutputFormat) -> Result<()> {
    let script = gdscript::generate_remove_edge_overlay();
    run_eval(&script)?;

    match format {
        OutputFormat::Json => {
            cprintln!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "overlay": "off",
                }))
                .unwrap()
            );
        }
        OutputFormat::Text => {
            cprintln!("Edge overlay {}", "removed".green());
        }
    }

    Ok(())
}
