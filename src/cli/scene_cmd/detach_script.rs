use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{
    DetachScriptArgs, clean_double_blanks, decrement_load_steps, extract_ext_resource_id,
    find_node, is_ext_resource_referenced, read_and_parse_scene, write_or_dry_run,
};

pub(crate) fn exec_detach_script(args: &DetachScriptArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, data) = read_and_parse_scene(&path)?;

    let node_name = if let Some(ref name) = args.node {
        find_node(&data, name)?;
        name.clone()
    } else {
        data.nodes
            .first()
            .ok_or_else(|| miette!("Scene has no nodes"))?
            .name
            .clone()
    };

    let result = apply_detach_script(&source, &node_name)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        println!(
            "{} Detached script from node '{}' in {}",
            "✓".green(),
            node_name.bold(),
            args.scene,
        );
    }

    Ok(())
}

/// Remove the script property from a node and clean up orphaned ext_resources.
pub(crate) fn apply_detach_script(source: &str, node_name: &str) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let node_pattern = format!("name=\"{node_name}\"");

    // First pass: find the script property and its ext_resource ID
    let mut in_target_node = false;
    let mut node_found = false;
    let mut script_ext_id: Option<String> = None;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("[node ") && trimmed.contains(&node_pattern) {
            in_target_node = true;
            node_found = true;
            continue;
        }
        if in_target_node && trimmed.starts_with('[') {
            in_target_node = false;
        }
        if in_target_node && trimmed.starts_with("script = ") {
            let value = &trimmed["script = ".len()..];
            script_ext_id = extract_ext_resource_id(value).map(String::from);
        }
    }

    if !node_found {
        return Err(miette!("Node '{}' not found in scene", node_name));
    }

    let ext_id =
        script_ext_id.ok_or_else(|| miette!("Node '{}' has no script attached", node_name))?;

    // Second pass: remove the script property line
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    in_target_node = false;

    for line in &lines {
        let trimmed = line.trim();
        if trimmed.starts_with("[node ") && trimmed.contains(&node_pattern) {
            in_target_node = true;
        } else if in_target_node && trimmed.starts_with('[') {
            in_target_node = false;
        }

        if in_target_node && trimmed.starts_with("script = ") {
            continue; // Skip the script property
        }

        result.push((*line).to_string());
    }

    let mut intermediate = result.join("\n");
    if !intermediate.ends_with('\n') {
        intermediate.push('\n');
    }

    // Check if the ext_resource is still referenced elsewhere
    if !is_ext_resource_referenced(&intermediate, &ext_id) {
        // Remove the orphaned ext_resource line and decrement load_steps
        let mut final_lines: Vec<String> = Vec::new();
        let ext_pattern = format!("id=\"{ext_id}\"");

        for line in intermediate.lines() {
            let trimmed = line.trim();
            // Skip the orphaned ext_resource line
            if trimmed.starts_with("[ext_resource") && trimmed.contains(&ext_pattern) {
                continue;
            }
            // Decrement load_steps
            if trimmed.starts_with("[gd_scene") && trimmed.contains("load_steps=") {
                final_lines.push(decrement_load_steps(trimmed, 1));
                continue;
            }
            final_lines.push(line.to_string());
        }

        let mut output = final_lines.join("\n");
        if !output.ends_with('\n') {
            output.push('\n');
        }
        return Ok(clean_double_blanks(&output));
    }

    Ok(clean_double_blanks(&intermediate))
}
