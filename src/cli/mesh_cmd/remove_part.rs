use miette::Result;
use owo_colors::OwoColorize;

use crate::core::mesh::MeshState;

use super::gdscript;
use super::{OutputFormat, RemovePartArgs, inject_stats, project_root, run_eval};
use crate::cprintln;

pub fn cmd_remove_part(args: &RemovePartArgs) -> Result<()> {
    if let Some(ref group_name) = args.group {
        return cmd_remove_group(args, group_name);
    }

    let name = args
        .name
        .as_deref()
        .ok_or_else(|| miette::miette!("--name is required when not using --group"))?;

    let script = gdscript::generate_remove_part(name);
    let result = run_eval(&script)?;
    let mut parsed: serde_json::Value = serde_json::from_str(&result)
        .map_err(|e| miette::miette!("Failed to parse result: {e}"))?;

    // Remove from Rust state so --all iterators don't reference stale parts
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;
    state.parts.shift_remove(name);
    if state.active == name {
        state.active = state.parts.keys().next().cloned().unwrap_or_default();
    }
    state.save(&root)?;
    inject_stats(&mut parsed, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&parsed).unwrap());
        }
        OutputFormat::Text => {
            let removed = parsed["removed"].as_str().unwrap_or("?");
            let active = parsed["active"].as_str().unwrap_or("none");
            let pc = parsed["part_count"].as_u64().unwrap_or(0);
            cprintln!(
                "Removed: {} (active: {}, {pc} remaining)",
                removed.red().bold(),
                active.cyan(),
            );
        }
    }
    Ok(())
}

fn cmd_remove_group(args: &RemovePartArgs, group_name: &str) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let members = state
        .groups
        .get(group_name)
        .ok_or_else(|| miette::miette!("Group '{group_name}' not found"))?
        .clone();

    for name in &members {
        let script = gdscript::generate_remove_part(name);
        let _ = run_eval(&script);
        state.parts.shift_remove(name);
    }

    // Remove group definition
    state.groups.remove(group_name);

    // Fix active part if it was in the removed group
    if members.contains(&state.active) {
        state.active = state.parts.keys().next().cloned().unwrap_or_default();
    }
    state.save(&root)?;

    let mut result = serde_json::json!({
        "group": group_name,
        "removed": members,
        "count": members.len(),
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Removed group {} ({} parts)",
                group_name.red().bold(),
                members.len().to_string().cyan()
            );
        }
    }
    Ok(())
}
