use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_core::cprintln;

use super::{
    AddSubResourceArgs, clean_double_blanks, find_node, increment_load_steps, next_sub_resource_id,
    read_and_parse_scene, set_property, write_or_dry_run,
};

pub(crate) fn exec_add_sub_resource(args: &AddSubResourceArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    // Validate --key requires --node
    if args.key.is_some() && args.node.is_none() {
        return Err(miette!("--key requires --node"));
    }

    let (source, data) = read_and_parse_scene(&path)?;

    let sub_id = next_sub_resource_id(&data.sub_resources, &args.resource_type);
    let result = insert_sub_resource(&source, &args.resource_type, &sub_id, &args.properties);

    // If --node and --key, also set the property
    let result = if let (Some(node_name), Some(key)) = (&args.node, &args.key) {
        // Re-parse to get updated data for node lookup
        let updated_data = gd_core::scene::parse_scene(&result)?;
        let node = find_node(&updated_data, node_name)?;
        let value = format!("SubResource(\"{sub_id}\")");
        set_property::apply_set_property(&result, &node.name, node.parent.as_deref(), key, &value)?
    } else {
        result
    };

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Added sub_resource '{}' ({}) to {}",
            "✓".green(),
            sub_id.bold(),
            args.resource_type,
            args.scene,
        );
    }

    Ok(())
}

/// Insert a sub_resource section into the scene source.
/// Placed after ext_resources but before the first [node].
pub(crate) fn insert_sub_resource(
    source: &str,
    type_name: &str,
    sub_id: &str,
    properties: &[(String, String)],
) -> String {
    use std::fmt::Write;
    let mut section = format!("[sub_resource type=\"{type_name}\" id=\"{sub_id}\"]");
    for (key, value) in properties {
        let _ = write!(section, "\n{key} = {value}");
    }

    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 5);

    let mut inserted = false;

    for line in &lines {
        let trimmed = line.trim();

        // Increment load_steps
        if trimmed.starts_with("[gd_scene") && trimmed.contains("load_steps=") {
            result.push(increment_load_steps(trimmed));
            continue;
        }

        // Insert before first [node] section
        if !inserted && trimmed.starts_with("[node ") {
            if !result.last().is_some_and(|l| l.trim().is_empty()) {
                result.push(String::new());
            }
            result.push(section.clone());
            result.push(String::new());
            inserted = true;
        }

        result.push((*line).to_string());
    }

    // If no nodes found, append at end
    if !inserted {
        if !result.last().is_some_and(|l| l.trim().is_empty()) {
            result.push(String::new());
        }
        result.push(section);
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    clean_double_blanks(&output)
}
