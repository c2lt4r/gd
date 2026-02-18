use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{MergeArgs, OutputFormat, project_root, run_eval};

pub fn cmd_merge(args: &MergeArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part = state.active_part_mut()?;
    let (result, merged) =
        crate::core::mesh::merge::merge_by_distance(&part.mesh, args.distance);
    let vc = result.vertex_count();
    let fc = result.face_count();
    part.mesh = result;

    state.save(&root)?;

    let active = state.active.clone();
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;

    let result = serde_json::json!({
        "distance": args.distance,
        "merged": merged,
        "vertex_count": vc,
        "face_count": fc,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let d = args.distance;
            println!(
                "Merged {} vertices (distance {d:.6}), {} vertices remaining",
                merged.to_string().green().bold(),
                vc.to_string().cyan(),
            );
        }
    }
    Ok(())
}
