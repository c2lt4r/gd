use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;
use crate::cprintln;

use super::{OutputFormat, SelectArgs, inject_stats, project_root};

pub fn cmd_select(args: &SelectArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    if !state.parts.contains_key(&args.name) {
        return Err(miette!("Part '{}' not found", args.name));
    }

    state.active.clone_from(&args.name);
    state.save(&root)?;

    let mut result = serde_json::json!({
        "active": args.name,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!("Active part: {}", args.name.green().bold());
        }
    }
    Ok(())
}
