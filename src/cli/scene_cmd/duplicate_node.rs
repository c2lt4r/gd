use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use crate::core::scene;
use crate::cprintln;

use super::{
    DuplicateNodeArgs, clean_double_blanks, compute_node_path, find_node, parent_attr_for_node,
    read_and_parse_scene, write_or_dry_run,
};

pub(crate) fn exec_duplicate_node(args: &DuplicateNodeArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, data) = read_and_parse_scene(&path)?;
    let result = apply_duplicate_node(
        &source,
        &data,
        &args.source_node,
        &args.name,
        args.parent.as_deref(),
    )?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Duplicated '{}' as '{}' in {}",
            "✓".green(),
            args.source_node,
            args.name.bold(),
            args.scene,
        );
    }

    Ok(())
}

/// Duplicate a node section in the scene source, replacing name and optionally parent.
pub(crate) fn apply_duplicate_node(
    source: &str,
    data: &scene::SceneData,
    source_node_ref: &str,
    new_name: &str,
    new_parent: Option<&str>,
) -> Result<String> {
    let source_node = find_node(data, source_node_ref)?;
    if source_node.parent.is_none() {
        return Err(miette!("Cannot duplicate root node"));
    }

    let source_path = compute_node_path(source_node, data);

    // Determine target parent attribute
    let target_parent_attr = if let Some(parent_ref) = new_parent {
        find_node(data, parent_ref)?;
        parent_attr_for_node(parent_ref, data)?
    } else {
        // Same parent as source
        source_node
            .parent
            .clone()
            .unwrap_or_else(|| ".".to_string())
    };

    // Check for duplicate sibling
    let has_duplicate = data
        .nodes
        .iter()
        .any(|n| n.name == new_name && n.parent.as_deref() == Some(&target_parent_attr));
    if target_parent_attr == "." && data.nodes.first().is_some_and(|n| n.name == new_name) {
        return Err(miette!(
            "Node '{}' already exists under parent '{}'",
            new_name,
            target_parent_attr
        ));
    }
    if has_duplicate {
        return Err(miette!(
            "Node '{}' already exists under parent '{}'",
            new_name,
            target_parent_attr
        ));
    }

    // Extract the source node's section from raw text
    let section_lines = extract_node_section(source, &source_path, source_node)?;

    // Rewrite the section with new name and parent
    let new_section = rewrite_node_section(
        &section_lines,
        &source_node.name,
        new_name,
        source_node.parent.as_deref(),
        &target_parent_attr,
    );

    // Insert after source node's section
    let lines: Vec<&str> = source.lines().collect();
    let mut result: Vec<String> = Vec::with_capacity(lines.len() + section_lines.len() + 2);

    let insert_after = find_section_end(&lines, &source_path, source_node);

    for (i, line) in lines.iter().enumerate() {
        result.push((*line).to_string());
        if i == insert_after {
            result.push(String::new());
            for section_line in &new_section {
                result.push(section_line.clone());
            }
        }
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }
    Ok(clean_double_blanks(&output))
}

/// Extract a node section (header + property lines) from the raw source.
fn extract_node_section(
    source: &str,
    source_path: &str,
    node: &scene::SceneNode,
) -> Result<Vec<String>> {
    let lines: Vec<&str> = source.lines().collect();
    let mut section = Vec::new();
    let mut found = false;
    let mut in_section = false;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.starts_with("[node ") && is_matching_node_header(trimmed, source_path, node) {
            in_section = true;
            found = true;
            section.push((*line).to_string());
            continue;
        }

        if in_section {
            if trimmed.starts_with('[') {
                break;
            }
            section.push((*line).to_string());
        }
    }

    if !found {
        return Err(miette!(
            "Could not find node '{}' section in scene text",
            node.name
        ));
    }

    // Trim trailing blank lines from section
    while section.last().is_some_and(|l| l.trim().is_empty()) {
        section.pop();
    }

    Ok(section)
}

/// Find the last line index of a node's section.
fn find_section_end(lines: &[&str], source_path: &str, node: &scene::SceneNode) -> usize {
    let mut in_section = false;
    let mut last_content = 0;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();

        if trimmed.starts_with("[node ") && is_matching_node_header(trimmed, source_path, node) {
            in_section = true;
            last_content = i;
            continue;
        }

        if in_section {
            if trimmed.starts_with('[') {
                break;
            }
            if !trimmed.is_empty() {
                last_content = i;
            }
        }
    }

    last_content
}

/// Check if a `[node ...]` header matches the source node.
fn is_matching_node_header(header: &str, _source_path: &str, node: &scene::SceneNode) -> bool {
    let name_pat = format!("name=\"{}\"", node.name);
    if !header.contains(&name_pat) {
        return false;
    }

    match &node.parent {
        None => !header.contains("parent="),
        Some(parent) => {
            // For root children, source_path == node.name, parent == "."
            let parent_pat = format!("parent=\"{parent}\"");
            header.contains(&parent_pat)
        }
    }
}

/// Rewrite extracted section lines with new name and parent.
fn rewrite_node_section(
    section: &[String],
    old_name: &str,
    new_name: &str,
    old_parent: Option<&str>,
    new_parent: &str,
) -> Vec<String> {
    let mut result = Vec::with_capacity(section.len());

    for (i, line) in section.iter().enumerate() {
        if i == 0 {
            // Rewrite the header line
            let mut header = line.replace(
                &format!("name=\"{old_name}\""),
                &format!("name=\"{new_name}\""),
            );
            if let Some(old_p) = old_parent {
                header = header.replace(
                    &format!("parent=\"{old_p}\""),
                    &format!("parent=\"{new_parent}\""),
                );
            }
            result.push(header);
        } else {
            result.push(line.clone());
        }
    }

    result
}
