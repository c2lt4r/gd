use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::gdscript;
use super::{CreateArgs, OutputFormat, build_primitive_mesh, inject_stats, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_create(args: &CreateArgs) -> Result<()> {
    let root = project_root()?;

    // Initialize Rust mesh state
    let mut state = MeshState::new(&args.name);
    state.save(&root)?;

    // Create Godot scene infrastructure (cameras, lights, HUD, mesh node)
    let script = gdscript::generate_create(&args.name, args.from.as_str());
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

    inject_stats(&mut parsed, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let name = parsed["name"].as_str().unwrap_or("?");
            let prim = parsed["primitive"].as_str().unwrap_or("?");
            let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
            cprintln!(
                "Mesh session started: {} (primitive: {}, vertices: {vc})",
                name.green().bold(),
                prim.cyan()
            );
        }
    }
    Ok(())
}
