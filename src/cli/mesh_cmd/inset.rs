use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;
use crate::core::mesh::spatial_filter;

use super::{InsetArgs, OutputFormat, inject_stats, project_root, run_eval};
use crate::cprintln;

pub fn cmd_inset(args: &InsetArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let spatial = args
        .where_expr
        .as_deref()
        .map(spatial_filter::parse_where)
        .transpose()?;

    let part = state.active_part_mut()?;
    let result = if let Some(ref sf) = spatial {
        let selected: Vec<usize> = (0..part.mesh.faces.len())
            .filter(|&fi| spatial_filter::face_matches(&part.mesh, fi, sf))
            .collect();
        crate::core::mesh::inset::inset_selected(&part.mesh, args.factor, Some(&selected))
    } else {
        crate::core::mesh::inset::inset(&part.mesh, args.factor)
    };
    let vc = result.vertex_count();
    let fc = result.face_count();
    part.mesh = result;

    state.save(&root)?;

    let active = state.active.clone();
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "factor": args.factor,
        "vertex_count": vc,
        "face_count": fc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            let f = args.factor;
            cprintln!(
                "Inset: factor {f:.3}, {vc} vertices, {fc} faces",
                vc = vc.to_string().green().bold(),
                fc = fc.to_string().cyan(),
            );
        }
    }
    Ok(())
}
