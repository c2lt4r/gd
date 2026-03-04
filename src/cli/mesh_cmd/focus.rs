use miette::Result;
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::gdscript;
use super::{FocusArgs, OutputFormat, project_root, run_eval};
use gd_core::cprintln;

pub fn cmd_focus(args: &FocusArgs) -> Result<()> {
    if !args.all && args.part.is_none() {
        return Err(miette::miette!(
            "Provide a part name or use --all to show all parts"
        ));
    }

    let script = if args.all {
        gdscript::generate_focus_all()
    } else {
        gdscript::generate_focus(args.part.as_deref().unwrap())
    };

    let result = run_eval(&script)?;

    // Update Rust-side active part to match Godot-side focus
    if let Some(name) = &args.part {
        let root = project_root()?;
        let mut state = MeshState::load(&root)?;
        if state.parts.contains_key(name) {
            state.active.clone_from(name);
            state.save(&root)?;
        }
    }
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let active = parsed["active"].as_str().unwrap_or("?");
            if args.all {
                let pc = parsed["part_count"].as_u64().unwrap_or(0);
                cprintln!(
                    "Showing {} parts (active: {})",
                    pc.to_string().green().bold(),
                    active.cyan()
                );
            } else {
                let vc = parsed["vertex_count"].as_u64().unwrap_or(0);
                cprintln!("Focused: {} ({vc} vertices)", active.green().bold(),);
            }
        }
    }
    Ok(())
}
