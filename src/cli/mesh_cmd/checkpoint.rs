use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::{CheckpointArgs, OutputFormat, RestoreArgs, inject_stats, project_root, run_eval};
use gd_core::{ceprintln, cprintln};

pub fn cmd_checkpoint(args: &CheckpointArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let label = args.name.clone().unwrap_or_else(|| "default".to_string());

    let parts_saved = state.parts.len();
    state.checkpoints.insert(label.clone(), state.parts.clone());
    state
        .group_checkpoints
        .insert(label.clone(), state.groups.clone());
    state.save(&root)?;

    let mut result = serde_json::json!({
        "parts_saved": parts_saved,
        "name": label,
        "checkpoints": state.checkpoints.keys().collect::<Vec<_>>(),
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Checkpoint {} saved: {} parts",
                label.cyan(),
                parts_saved.to_string().green()
            );
        }
    }
    Ok(())
}

pub fn cmd_restore(args: &RestoreArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let label = args.name.clone().unwrap_or_else(|| "default".to_string());

    let saved = state
        .checkpoints
        .get(&label)
        .ok_or_else(|| miette!("Checkpoint '{label}' not found"))?
        .clone();

    let parts_restored = saved.len();

    // Remove Godot nodes for parts that exist now but not in the checkpoint
    let extra_parts: Vec<String> = state
        .parts
        .keys()
        .filter(|k| !saved.contains_key(k.as_str()))
        .cloned()
        .collect();
    for name in &extra_parts {
        let script = super::gdscript::generate_remove_part(name);
        let _ = run_eval(&script);
    }

    state.parts = saved;

    // Restore groups if available
    if let Some(saved_groups) = state.group_checkpoints.get(&label).cloned() {
        state.groups = saved_groups;
    }

    // Ensure active part still exists
    if !state.parts.contains_key(&state.active)
        && let Some(first) = state.parts.keys().next()
    {
        state.active = first.clone();
    }

    state.save(&root)?;

    // Push all parts to Godot (skip parts missing from scene)
    let names: Vec<String> = state.parts.keys().cloned().collect();
    for name in &names {
        let push = state.generate_push_script(name)?;
        if let Err(e) = run_eval(&push) {
            ceprintln!("Warning: skipping push for '{name}': {e}");
        }
    }

    let mut result = serde_json::json!({
        "parts_restored": parts_restored,
        "name": label,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Restored {} parts from checkpoint {}",
                parts_restored.to_string().green(),
                label.cyan()
            );
        }
    }
    Ok(())
}
