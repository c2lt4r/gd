use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use super::{SetPropertyArgs, read_and_parse_scene, write_or_dry_run};
use crate::cprintln;

pub(crate) fn exec_set_property(args: &SetPropertyArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, _data) = read_and_parse_scene(&path)?;
    let result = apply_set_property(&source, &args.node, &args.key, &args.value)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
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
    let mut consuming_multiline = false;

    for line in &lines {
        let trimmed = line.trim();

        // Detect entering the target node section
        if trimmed.starts_with("[node ") && trimmed.contains(&node_pattern) {
            in_target_node = true;
            node_found = true;
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
            consuming_multiline = false;
        }

        // Skip continuation lines of a replaced multi-line value
        if consuming_multiline {
            if is_scene_continuation_line(trimmed) {
                continue;
            }
            consuming_multiline = false;
        }

        // Replace existing property if we're in the target node
        if in_target_node && trimmed.starts_with(&prop_prefix) {
            if !property_set {
                result.push(format!("{key} = {value}"));
                property_set = true;
            }
            // Check if old value spans multiple lines
            if is_scene_multiline_start(trimmed, &prop_prefix) {
                consuming_multiline = true;
            }
            continue;
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

/// Check if a property value starts a multi-line block (unclosed bracket/brace).
fn is_scene_multiline_start(line: &str, prop_prefix: &str) -> bool {
    let val = &line[prop_prefix.len()..];
    let open_brackets = val.chars().filter(|&c| c == '[').count();
    let close_brackets = val.chars().filter(|&c| c == ']').count();
    let open_braces = val.chars().filter(|&c| c == '{').count();
    let close_braces = val.chars().filter(|&c| c == '}').count();
    (open_brackets > close_brackets) || (open_braces > close_braces)
}

/// Check if a line is a continuation of a multi-line value in a scene file.
fn is_scene_continuation_line(trimmed: &str) -> bool {
    if trimmed.is_empty() || trimmed.starts_with('[') {
        return false;
    }
    // New property lines match `identifier = value`
    !looks_like_scene_property(trimmed)
}

/// Heuristic: does this line look like a scene node property?
fn looks_like_scene_property(trimmed: &str) -> bool {
    if let Some(eq_pos) = trimmed.find(" = ") {
        let key = &trimmed[..eq_pos];
        !key.is_empty()
            && key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/')
    } else {
        false
    }
}
