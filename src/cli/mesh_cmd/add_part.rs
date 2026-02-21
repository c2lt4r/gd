use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::{MeshPart, MeshState};

use super::gdscript;
use super::{
    AddPartArgs, OutputFormat, import_primitive_mesh, inject_stats, project_root, run_eval,
};
use crate::cprintln;

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

    // Import primitive mesh arrays (cube/sphere/cylinder) into Rust state
    import_primitive_mesh(&parsed, &mut state);
    state.save(&root)?;

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
