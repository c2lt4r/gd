use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{OutputFormat, SolidifyArgs, inject_stats, project_root, run_eval};
use crate::cprintln;

pub fn cmd_solidify(args: &SolidifyArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part = state.active_part_mut()?;
    let result = crate::core::mesh::solidify::solidify(&part.mesh, args.thickness);
    let vc = result.vertex_count();
    let fc = result.face_count();
    part.mesh = result;

    state.save(&root)?;

    let active = state.active.clone();
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "thickness": args.thickness,
        "vertex_count": vc,
        "face_count": fc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let t = args.thickness;
            cprintln!(
                "Solidified: thickness {t:.3}, {vc} vertices, {fc} faces",
                vc = vc.to_string().green().bold(),
                fc = fc.to_string().cyan(),
            );
        }
    }
    Ok(())
}
