use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_mesh::MeshState;

use super::{
    GroupArgs, GroupsArgs, OutputFormat, UngroupArgs, inject_stats, match_part_pattern,
    project_root,
};
use gd_core::cprintln;

pub fn cmd_group(args: &GroupArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    let all_names: Vec<String> = state.parts.keys().cloned().collect();
    let matched = match_part_pattern(&all_names, &args.parts);

    if matched.is_empty() {
        return Err(miette!(
            "No parts match '{}'. Available: {}",
            args.parts,
            all_names.join(", ")
        ));
    }

    let members: Vec<String> = matched.iter().map(|s| (*s).to_string()).collect();
    let count = members.len();
    state.groups.insert(args.name.clone(), members.clone());
    state.save(&root)?;

    let mut result = serde_json::json!({
        "group": args.name,
        "members": members,
        "count": count,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!(
                "Group {}: {} members ({})",
                args.name.green().bold(),
                count.to_string().cyan(),
                matched.join(", ")
            );
        }
    }
    Ok(())
}

pub fn cmd_ungroup(args: &UngroupArgs) -> Result<()> {
    let root = project_root()?;
    let mut state = MeshState::load(&root)?;

    if state.groups.remove(&args.name).is_none() {
        return Err(miette!("Group '{}' not found", args.name));
    }
    state.save(&root)?;

    let mut result = serde_json::json!({
        "group": args.name,
        "removed": true,
    });
    inject_stats(&mut result, &state);

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            cprintln!("Removed group {}", args.name.red().bold());
        }
    }
    Ok(())
}

pub fn cmd_groups(args: &GroupsArgs) -> Result<()> {
    let root = project_root()?;
    let state = MeshState::load(&root)?;

    let result = serde_json::json!({
        "groups": state.groups,
    });

    match args.format {
        OutputFormat::Json => {
            cprintln!("{}", serde_json::to_string_pretty(&result).unwrap());
        }
        OutputFormat::Text => {
            if state.groups.is_empty() {
                cprintln!("No groups defined.");
            } else {
                for (name, members) in &state.groups {
                    cprintln!(
                        "{}: {} ({})",
                        name.green().bold(),
                        members.len().to_string().cyan(),
                        members.join(", ")
                    );
                }
            }
        }
    }
    Ok(())
}
