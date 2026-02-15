use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{SetPropertyArgs, read_and_parse_scene, write_or_dry_run};

pub(crate) fn exec_set_property(args: &SetPropertyArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, _data) = read_and_parse_scene(&path)?;
    let result = apply_set_property(&source, &args.node, &args.key, &args.value)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        println!(
            "{} Set {}.{} = {} in {}",
            "✓".green(),
            args.node.bold(),
            args.key,
            args.value,
            args.scene,
        );
    }

    Ok(())
}

/// Set or update a property on a node in the scene source text.
pub(crate) fn apply_set_property(
    source: &str,
    node_name: &str,
    key: &str,
    value: &str,
) -> Result<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 1);

    let node_pattern = format!("name=\"{node_name}\"");
    let prop_prefix = format!("{key} = ");

    let mut in_target_node = false;
    let mut property_set = false;
    let mut node_found = false;
    let mut needs_insert = false;

    for line in &lines {
        let trimmed = line.trim();

        // Detect entering the target node section
        if trimmed.starts_with("[node ") && trimmed.contains(&node_pattern) {
            in_target_node = true;
            node_found = true;
            result.push((*line).to_string());
            needs_insert = true;
            continue;
        }

        // When we need to insert a new property, do it right after the header
        // (before any blank lines or the next section)
        if needs_insert && !property_set && (trimmed.is_empty() || trimmed.starts_with('[')) {
            result.push(format!("{key} = {value}"));
            property_set = true;
            needs_insert = false;
            if trimmed.starts_with('[') {
                in_target_node = false;
            }
            result.push((*line).to_string());
            continue;
        }

        // Detect leaving the target node section (next section header)
        if in_target_node && trimmed.starts_with('[') {
            if !property_set {
                result.push(format!("{key} = {value}"));
                property_set = true;
            }
            in_target_node = false;
        }

        // Replace existing property if we're in the target node
        if in_target_node && trimmed.starts_with(&prop_prefix) {
            result.push(format!("{key} = {value}"));
            property_set = true;
            needs_insert = false;
            continue;
        }

        // If we encounter a non-blank property line, the insert point has passed
        if needs_insert && !trimmed.is_empty() {
            needs_insert = false;
        }

        result.push((*line).to_string());
    }

    // If we were still in the target node at EOF (last section), append property
    if in_target_node && !property_set {
        result.push(format!("{key} = {value}"));
    }

    if !node_found {
        return Err(miette!("Node '{}' not found in scene", node_name));
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(output)
}
