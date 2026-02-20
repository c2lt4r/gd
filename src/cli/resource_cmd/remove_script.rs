use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{RemoveScriptArgs, read_and_parse_resource, write_or_dry_run};
use crate::cprintln;

pub(crate) fn exec_remove_script(args: &RemoveScriptArgs) -> Result<()> {
    let path = PathBuf::from(&args.file);
    if !path.exists() {
        return Err(miette!("File not found: {}", args.file));
    }

    let (source, data) = read_and_parse_resource(&path)?;

    let script_value = data
        .properties
        .iter()
        .find(|(k, _)| k == "script")
        .map(|(_, v)| v.clone())
        .ok_or_else(|| miette!("Resource has no script attached"))?;

    let ext_id = scene_helpers::extract_ext_resource_id(&script_value)
        .ok_or_else(|| miette!("Cannot parse script reference: {script_value}"))?
        .to_string();

    let result = apply_remove_script(&source, &ext_id);

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!("{} Removed script from {}", "✓".green(), args.file.bold(),);
    }

    Ok(())
}

pub(crate) fn apply_remove_script(source: &str, ext_id: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let ext_pattern = format!("id=\"{ext_id}\"");

    let mut in_resource = false;

    for line in &lines {
        let trimmed = line.trim();

        // Track [resource] section
        if trimmed.starts_with("[resource") {
            in_resource = true;
        } else if in_resource && trimmed.starts_with('[') {
            in_resource = false;
        }

        // Skip the script property in [resource]
        if in_resource && trimmed.starts_with("script = ") {
            continue;
        }

        result.push((*line).to_string());
    }

    let intermediate = result.join("\n");

    // Check if the ext_resource is still referenced elsewhere
    if !scene_helpers::is_ext_resource_referenced(&intermediate, ext_id) {
        // Remove the orphaned ext_resource and decrement load_steps
        let mut final_lines: Vec<String> = Vec::new();

        for line in intermediate.lines() {
            let trimmed = line.trim();

            // Skip the orphaned ext_resource line
            if trimmed.starts_with("[ext_resource") && trimmed.contains(&ext_pattern) {
                continue;
            }

            // Decrement load_steps
            if trimmed.starts_with("[gd_resource") && trimmed.contains("load_steps=") {
                final_lines.push(scene_helpers::decrement_load_steps(trimmed, 1));
                continue;
            }

            final_lines.push(line.to_string());
        }

        let mut output = final_lines.join("\n");
        if !output.ends_with('\n') {
            output.push('\n');
        }
        return scene_helpers::clean_double_blanks(&output);
    }

    let mut output = intermediate;
    if !output.ends_with('\n') {
        output.push('\n');
    }
    scene_helpers::clean_double_blanks(&output)
}

mod scene_helpers {
    pub(super) use crate::cli::scene_cmd::{
        clean_double_blanks, decrement_load_steps, extract_ext_resource_id,
        is_ext_resource_referenced,
    };
}
