use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::{InsetArgs, OutputFormat, project_root, run_eval};

pub fn cmd_inset(args: &InsetArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let part = state.active_part_mut()?;
    let result = crate::core::mesh::inset::inset(&part.mesh, args.factor);
    let vc = result.vertex_count();
    let fc = result.face_count();
    part.mesh = result;

    state.save(&root)?;

    let active = state.active.clone();
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;

    let result = serde_json::json!({
        "factor": args.factor,
        "vertex_count": vc,
        "face_count": fc,
    });

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let f = args.factor;
            println!(
                "Inset: factor {f:.3}, {vc} vertices, {fc} faces",
                vc = vc.to_string().green().bold(),
                fc = fc.to_string().cyan(),
            );
        }
    }
    Ok(())
}
