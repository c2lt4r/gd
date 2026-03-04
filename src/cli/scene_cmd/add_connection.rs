use std::path::PathBuf;

use miette::{Result, miette};
use owo_colors::OwoColorize;

use gd_core::scene::SceneData;

use super::{AddConnectionArgs, read_and_parse_scene, write_or_dry_run};
use gd_core::cprintln;

pub(crate) fn exec_add_connection(args: &AddConnectionArgs) -> Result<()> {
    let path = PathBuf::from(&args.scene);
    if !path.exists() {
        return Err(miette!("Scene file not found: {}", args.scene));
    }

    let (source, data) = read_and_parse_scene(&path)?;
    let result = insert_connection(
        &source,
        &data,
        &args.signal,
        &args.from,
        &args.to,
        &args.method,
    )?;

    write_or_dry_run(&path, &result, args.dry_run)?;

    if !args.dry_run {
        cprintln!(
            "{} Added connection {}.{} → {}.{} in {}",
            "✓".green(),
            args.from,
            args.signal,
            args.to,
            args.method.bold(),
            args.scene,
        );
    }

    Ok(())
}

/// Insert a new connection line into the scene source.
pub(crate) fn insert_connection(
    source: &str,
    data: &SceneData,
    signal: &str,
    from: &str,
    to: &str,
    method: &str,
) -> Result<String> {
    // Validate from node exists (allow "." for root)
    validate_node_ref(from, data)?;
    validate_node_ref(to, data)?;

    // Check for duplicate connection
    let is_duplicate = data
        .connections
        .iter()
        .any(|c| c.signal == signal && c.from == from && c.to == to && c.method == method);
    if is_duplicate {
        return Err(miette!(
            "Connection already exists: {}.{} → {}.{}",
            from,
            signal,
            to,
            method
        ));
    }

    let conn_line =
        format!("[connection signal=\"{signal}\" from=\"{from}\" to=\"{to}\" method=\"{method}\"]");

    let mut output = source.trim_end().to_string();

    // Add blank line separator if there are no existing connections
    if data.connections.is_empty() {
        output.push_str("\n\n");
    } else {
        output.push('\n');
    }
    output.push_str(&conn_line);
    output.push('\n');

    Ok(output)
}

/// Validate that a node reference exists in the scene.
/// "." refers to the root node, node names refer to direct children or paths.
fn validate_node_ref(name: &str, data: &SceneData) -> Result<()> {
    if name == "." {
        if data.nodes.is_empty() {
            return Err(miette!("Scene has no root node"));
        }
        return Ok(());
    }
    // Check if any node's computed path matches
    let found = data.nodes.iter().any(|n| {
        if n.parent.is_none() {
            // Root node — its "path" in connections is its name
            n.name == name
        } else if n.parent.as_deref() == Some(".") {
            // Direct child of root
            n.name == name
        } else {
            // Deeper node: parent/name
            let path = format!("{}/{}", n.parent.as_deref().unwrap_or("."), n.name);
            path == name || n.name == name
        }
    });
    if !found {
        return Err(miette!("Node '{}' not found in scene", name));
    }
    Ok(())
}
