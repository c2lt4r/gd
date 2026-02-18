use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{OutputFormat, TaperArgs, project_root, run_eval};

pub fn cmd_taper(args: &TaperArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let axis_idx = args.axis.as_index();
    let range = match (args.from, args.to) {
        (Some(f), Some(t)) => Some((f, t)),
        (Some(f), None) => Some((f, 1.0)),
        (None, Some(t)) => Some((0.0, t)),
        (None, None) => None,
    };

    let part = state.resolve_part_mut(args.part.as_deref())?;
    let count = crate::core::mesh::taper::taper(
        &mut part.mesh,
        axis_idx,
        args.from_scale,
        args.to_scale,
        args.midpoint,
        range,
    );
    let vc = part.mesh.vertex_count();

    state.save(&root)?;

    // Push to Godot
    let active = state.active.clone();
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;

    let result = serde_json::json!({
        "axis": args.axis.as_str(),
        "from_scale": args.from_scale,
        "to_scale": args.to_scale,
        "midpoint": args.midpoint,
        "range": range,
        "vertex_count": vc,
        "vertices_modified": count,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let from = args.from_scale;
            let to = args.to_scale;
            println!(
                "Tapered along {}: {from:.2} -> {to:.2} ({vc} vertices)",
                args.axis.as_str().cyan()
            );
        }
    }
    Ok(())
}
