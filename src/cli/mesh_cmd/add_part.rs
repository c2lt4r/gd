use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::{MeshPart, MeshState};

use super::gdscript;
use super::{
    AddPartArgs, OutputFormat, build_primitive_mesh, inject_stats, project_root, run_eval,
};
use gd_core::cprintln;

pub fn cmd_add_part(args: &AddPartArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    // Add new part to Rust state
    state.parts.insert(args.name.clone(), MeshPart::new());
    state.active.clone_from(&args.name);
    state.save(&root)?;

    // Create node in Godot (handles primitive mesh if needed)
    let script = gdscript::generate_add_part(&args.name, args.from.as_str());
    let result = run_eval(&script)?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Build primitive mesh in Rust (CCW winding, no Godot round-trip)
    build_primitive_mesh(args.from.as_str(), &mut state);
    state.save(&root)?;

    // Push Rust-built mesh to Godot for display
    if state.active_part().is_ok_and(|p| p.mesh.face_count() > 0) {
        let push = state.generate_push_script(&state.active.clone())?;
        let _ = run_eval(&push)?;
    }

    let pc = state.parts.len();
    let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
    inject_stats(&mut parsed, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Added part: {} ({vc} vertices, {pc} parts total)",
                args.name.green().bold(),
            );
        }
    }
    Ok(())
}
