use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{ArrayArgs, OutputFormat, inject_stats, parse_3d, project_root, run_eval};
use crate::cprintln;

pub fn cmd_array(args: &ArrayArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let (x, y, z) = parse_3d(&args.offset)?;

    let part = state.active_part_mut()?;
    let result = crate::core::mesh::array::array(&part.mesh, args.count as usize, [x, y, z]);
    let vc = result.vertex_count();
    let fc = result.face_count();
    part.mesh = result;

    state.save(&root)?;

    let active = state.active.clone();
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "count": args.count,
        "offset": [x, y, z],
        "vertex_count": vc,
        "face_count": fc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let c = args.count;
            cprintln!(
                "Array: {} copies, offset [{x}, {y}, {z}], {} vertices",
                c.to_string().green().bold(),
                vc.to_string().cyan(),
            );
        }
    }
    Ok(())
}
