use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;
use crate::core::mesh::spatial_filter;

use super::{ExtrudeFaceArgs, OutputFormat, inject_stats, project_root, run_eval};
use crate::cprintln;

pub fn cmd_extrude_face(args: &ExtrudeFaceArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let sf = spatial_filter::parse_where(&args.where_expr)?;

    let part = state.active_part_mut()?;
    let selected: Vec<usize> = (0..part.mesh.faces.len())
        .filter(|&fi| spatial_filter::face_matches(&part.mesh, fi, &sf))
        .collect();

    if selected.is_empty() {
        return Err(miette!(
            "No faces match --where '{}'. Check the expression.",
            args.where_expr
        ));
    }

    let result =
        crate::core::mesh::extrude_face::extrude_faces(&part.mesh, args.depth, &selected);

    let vc = result.vertex_count();
    let fc = result.face_count();
    let selected_count = selected.len();
    part.mesh = result;

    state.save(&root)?;

    let active = state.active.clone();
    let push = state.generate_push_script(&active)?;
    let _ = run_eval(&push)?;

    let mut result = serde_json::json!({
        "depth": args.depth,
        "where": args.where_expr,
        "faces_selected": selected_count,
        "vertex_count": vc,
        "face_count": fc,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Extrude-face: depth={}, {} faces extruded, {vc} vertices, {fc} faces",
                format!("{}", args.depth).green().bold(),
                selected_count.to_string().cyan(),
            );
        }
    }
    Ok(())
}
