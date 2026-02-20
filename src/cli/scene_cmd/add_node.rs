use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::scene::SceneData;

use crate::cprintln;

use super::{
    AddNodeArgs, clean_double_blanks, find_node, parent_attr_for_node, read_and_parse_scene,
    write_or_dry_run,
};

pub(crate) fn exec_add_node(args: &AddNodeArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, data) = read_and_parse_scene(&path)?;

    // Determine parent attribute
    let parent_attr = if let Some(ref parent_name) = args.parent {
        // Validate parent exists
        find_node(&data, parent_name)?;
        parent_attr_for_node(parent_name, &data)?
    } else {
        ".".to_string()
    };

    let result = insert_node(&source, &data, &args.name, &args.node_type, &parent_attr)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Added node '{}' ({}) to {}",
            "✓".green(),
            args.name.bold(),
            args.node_type,
            args.scene,
        );
    }

    Ok(())
}

/// Insert a new node section into the scene source.
pub(crate) fn insert_node(
    source: &str,
    data: &SceneData,
    name: &str,
    node_type: &str,
    parent_attr: &str,
) -> Result<String> {
    // Check for duplicate sibling name
    let has_duplicate = data
        .nodes
        .iter()
        .any(|n| n.name == name && n.parent.as_deref() == Some(parent_attr));
    if has_duplicate {
        return Err(miette!(
            "Node '{}' already exists under parent '{}'",
            name,
            parent_attr
        ));
    }

    // Also check if this name matches root and parent is "."
    if parent_attr == "." && data.nodes.first().is_some_and(|n| n.name == name) {
        return Err(miette!(
            "Node '{}' already exists under parent '{}'",
            name,
            parent_attr
        ));
    }

    let new_section =
        format!("[node name=\"{name}\" type=\"{node_type}\" parent=\"{parent_attr}\"]");

    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + 3);

    // Find insertion point: after last [node] section, before first [connection]
    let mut insert_idx = None;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("[connection") {
            // Insert before connections
            insert_idx = Some(i);
            break;
        }
    }

    if let Some(idx) = insert_idx {
        // Insert before connections
        for line in &lines[..idx] {
            result.push((*line).to_string());
        }
        // Ensure blank line before new node
        if !result.last().is_some_and(|l| l.trim().is_empty()) {
            result.push(String::new());
        }
        result.push(new_section);
        result.push(String::new());
        for line in &lines[idx..] {
            result.push((*line).to_string());
        }
    } else {
        // No connections — append at end
        for line in &lines {
            result.push((*line).to_string());
        }
        // Ensure blank line before new node
        if !result.last().is_some_and(|l| l.trim().is_empty()) {
            result.push(String::new());
        }
        result.push(new_section);
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(clean_double_blanks(&output))
}
