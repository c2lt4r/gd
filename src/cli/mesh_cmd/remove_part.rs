use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{OutputFormat, RemovePartArgs, project_root, run_eval};

pub fn cmd_remove_part(args: &RemovePartArgs) -> Result<()> {
    let script = gdscript::generate_remove_part(&args.name);
    let result = run_eval(&script)?;
    let parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Remove from Rust state so --all iterators don't reference stale parts
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;
    state.parts.shift_remove(&args.name);
    if state.active == args.name {
        state.active = state
            .parts
            .keys()
            .next()
            .cloned()
            .unwrap_or_default();
    }
    state.save(&root)?;

    match args.format {
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let removed = parsed["removed"].as_str().unwrap_or("?");
            let active = parsed["active"].as_str().unwrap_or("none");
            let pc = parsed["part_count"].as_u64().unwrap_or(0);
            println!(
                "Removed: {} (active: {}, {pc} remaining)",
                removed.red().bold(),
                active.cyan(),
            );
        }
    }
    Ok(())
}
