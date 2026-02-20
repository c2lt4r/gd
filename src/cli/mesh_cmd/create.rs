use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{CreateArgs, OutputFormat, import_primitive_mesh, inject_stats, project_root, run_eval};
use crate::cprintln;

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

    // Import primitive mesh arrays (cube/sphere/cylinder) into Rust state
    import_primitive_mesh(&parsed, &mut state);
    state.save(&root)?;
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
