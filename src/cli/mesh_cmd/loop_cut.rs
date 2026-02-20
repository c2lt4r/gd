use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{LoopCutArgs, OutputFormat, project_root, run_eval};
use crate::cprintln;

pub fn cmd_loop_cut(args: &LoopCutArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let axis_idx = args.axis.as_index();

    let part_name = args.part.clone().unwrap_or_else(|| state.active.clone());

    let part = state.resolve_part_mut(args.part.as_deref())?;
    let (result_mesh, splits) =
        crate::core::mesh::loop_cut::loop_cut(&part.mesh, axis_idx, args.at);

    let vc = result_mesh.vertex_count();
    part.mesh = result_mesh;

    state.save(&root)?;

    // Push to Godot
    let push = state.generate_push_script(&part_name)?;
    let _ = run_eval(&push)?;

    let result = serde_json::json!({
        "axis": args.axis.as_str(),
        "at": args.at,
        "triangles_split": splits,
        "vertex_count": vc,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let at = args.at;
            cprintln!(
                "Loop cut at {}={at:.2}: {splits} triangles split, {vc} vertices",
                args.axis.as_str().cyan()
            );
        }
    }
    Ok(())
}
