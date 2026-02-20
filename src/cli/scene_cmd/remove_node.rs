use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::scene;

use crate::cprintln;

use super::{
    RemoveNodeArgs, clean_double_blanks, compute_node_path, decrement_load_steps,
    extract_ext_resource_id, is_ext_resource_referenced, read_and_parse_scene, write_or_dry_run,
};

pub(crate) fn exec_remove_node(args: &RemoveNodeArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, data) = read_and_parse_scene(&path)?;

    let target = data
        .nodes
        .iter()
        .find(|n| n.name == args.name)
        .ok_or_else(|| miette!("Node '{}' not found in scene", args.name))?;
    if target.parent.is_none() {
        return Err(miette!("Cannot remove root node '{}'", args.name));
    }

    let result = apply_remove_node(&source, &args.name)?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Removed node '{}' from {}",
            "✓".green(),
            args.name.bold(),
            args.scene,
        );
    }

    Ok(())
}

/// Remove a node and all its descendants from the scene source, cleaning up
/// connections and orphaned ext_resources.
pub(crate) fn apply_remove_node(source: &str, node_name: &str) -> Result<String> {
    let data = scene::parse_scene(source)?;

    let target = data
        .nodes
        .iter()
        .find(|n| n.name == node_name)
        .ok_or_else(|| miette!("Node '{}' not found in scene", node_name))?;

    if target.parent.is_none() {
        return Err(miette!("Cannot remove root node '{}'", node_name));
    }

    let target_path = compute_node_path(target, &data);

    let (removed_names, removed_paths) = collect_removed_nodes(&data, node_name, &target_path);
    let orphan_candidates = collect_orphan_candidates(&data, &removed_names);
    let removed_conn_refs: Vec<&str> = removed_paths
        .iter()
        .map(String::as_str)
        .chain(removed_names.iter().map(String::as_str))
        .collect();

    let mut intermediate =
        rewrite_without_nodes(source, &removed_names, &target_path, &removed_conn_refs);
    let orphan_count = remove_orphaned_ext_resources(&mut intermediate, &orphan_candidates);

    if orphan_count > 0 {
        apply_load_steps_decrement(&mut intermediate, orphan_count);
    }

    Ok(clean_double_blanks(&intermediate))
}

/// Collect the target node and all its descendants.
fn collect_removed_nodes(
    data: &scene::SceneData,
    node_name: &str,
    target_path: &str,
) -> (Vec<String>, Vec<String>) {
    let mut names = vec![node_name.to_string()];
    let mut paths = vec![target_path.to_string()];

    for node in &data.nodes {
        if let Some(ref parent) = node.parent
            && (*parent == target_path || parent.starts_with(&format!("{target_path}/")))
        {
            let path = compute_node_path(node, data);
            names.push(node.name.clone());
            paths.push(path);
        }
    }

    (names, paths)
}

/// Collect ext_resource IDs referenced by removed nodes.
fn collect_orphan_candidates(data: &scene::SceneData, removed_names: &[String]) -> Vec<String> {
    let mut ids = Vec::new();

    for node in &data.nodes {
        if !removed_names.contains(&node.name) {
            continue;
        }
        if let Some(ref script_val) = node.script
            && let Some(id) = extract_ext_resource_id(script_val)
        {
            ids.push(id.to_string());
        }
        if let Some(ref instance) = node.instance
            && let Some(id) = extract_ext_resource_id(instance)
        {
            ids.push(id.to_string());
        }
        for (key, value) in &node.properties {
            // Skip script/instance — already handled above
            if key == "script" || key == "instance" {
                continue;
            }
            if let Some(id) = extract_ext_resource_id(value) {
                ids.push(id.to_string());
            }
        }
    }

    ids
}

/// Rewrite the source, removing matching node and connection sections.
fn rewrite_without_nodes(
    source: &str,
    removed_names: &[String],
    target_path: &str,
    removed_conn_refs: &[&str],
) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len());
    let mut skip_section = false;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.starts_with('[') {
            skip_section = false;

            if trimmed.starts_with("[node ")
                && should_remove_node_line(trimmed, removed_names, target_path)
            {
                skip_section = true;
                continue;
            }

            if trimmed.starts_with("[connection ")
                && should_remove_connection_line(trimmed, removed_conn_refs)
            {
                skip_section = true;
                continue;
            }
        }

        if skip_section {
            continue;
        }

        result.push((*line).to_string());
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    output
}

/// Remove orphaned ext_resources from the intermediate text.
/// Returns the count of removed resources.
fn remove_orphaned_ext_resources(intermediate: &mut String, candidates: &[String]) -> u32 {
    let mut count = 0;
    for ext_id in candidates {
        if !is_ext_resource_referenced(intermediate, ext_id) {
            let ext_pattern = format!("id=\"{ext_id}\"");
            let cleaned: String = intermediate
                .lines()
                .filter(|line| {
                    let t = line.trim();
                    !(t.starts_with("[ext_resource") && t.contains(&ext_pattern))
                })
                .collect::<Vec<_>>()
                .join("\n");
            *intermediate = cleaned;
            if !intermediate.ends_with('\n') {
                intermediate.push('\n');
            }
            count += 1;
        }
    }
    count
}

/// Decrement load_steps in the gd_scene header.
fn apply_load_steps_decrement(intermediate: &mut String, amount: u32) {
    let updated: String = intermediate
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with("[gd_scene") && trimmed.contains("load_steps=") {
                decrement_load_steps(trimmed, amount)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    *intermediate = updated;
    if !intermediate.ends_with('\n') {
        intermediate.push('\n');
    }
}

/// Check if a [node ...] line matches a node to remove.
fn should_remove_node_line(line: &str, removed_names: &[String], target_path: &str) -> bool {
    let name = extract_attr(line, "name");
    let parent = extract_attr(line, "parent");

    let Some(name) = name else {
        return false;
    };

    if removed_names.contains(&name) {
        return true;
    }

    if let Some(ref p) = parent
        && (*p == target_path || p.starts_with(&format!("{target_path}/")))
    {
        return true;
    }

    false
}

/// Check if a [connection ...] line references any removed node.
fn should_remove_connection_line(line: &str, removed_refs: &[&str]) -> bool {
    let from = extract_attr(line, "from");
    let to = extract_attr(line, "to");

    if let Some(ref f) = from
        && removed_refs.contains(&f.as_str())
    {
        return true;
    }
    if let Some(ref t) = to
        && removed_refs.contains(&t.as_str())
    {
        return true;
    }
    false
}

/// Extract an attribute value from a section header line.
fn extract_attr(line: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    let start = line.find(&pattern)?;
    let after = &line[start + pattern.len()..];
    let end = after.find('"')?;
    Some(after[..end].to_string())
}
