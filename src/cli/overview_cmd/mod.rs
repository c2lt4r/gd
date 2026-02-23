use clap::Args;
use miette::{Result, miette};
use path_slash::PathExt;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tree_sitter::Node;

use crate::cprintln;
use crate::core::project::GodotProject;
use crate::lsp::workspace::WorkspaceIndex;

#[derive(Args)]
pub struct OverviewArgs {
    /// Scope overview to files under these paths
    pub paths: Vec<String>,
    /// Output format: text (default) or json
    #[arg(long, default_value = "text")]
    pub format: String,
}

#[derive(Serialize)]
struct OverviewData {
    project: String,
    godot_version: Option<String>,
    scripts: usize,
    scenes: usize,
    entries: Vec<ScriptEntry>,
    signal_flow: Vec<SceneFlowGroup>,
    autoloads: Vec<AutoloadEntry>,
}

#[derive(Serialize)]
struct ScriptEntry {
    file: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    extends: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    class_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    scene: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    signals: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    exports: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    funcs: Vec<String>,
}

#[derive(Serialize)]
struct SceneFlowGroup {
    scene: String,
    connections: Vec<FlowConnection>,
}

#[derive(Serialize)]
struct FlowConnection {
    from_node: String,
    signal: String,
    to_node: String,
    method: String,
}

#[derive(Serialize)]
struct AutoloadEntry {
    name: String,
    script: String,
}

/// A signal connection found in GDScript code via `.connect()`.
struct CodeConnection {
    signal: String,
    handler: String,
    receiver: String,
}

fn rel_slash(path: &Path, root: &Path) -> String {
    path.strip_prefix(root).map_or_else(
        |_| path.to_string_lossy().to_string(),
        |p| p.to_slash_lossy().to_string(),
    )
}

pub fn exec(args: &OverviewArgs) -> Result<()> {
    let cwd =
        std::env::current_dir().map_err(|e| miette!("Failed to get current directory: {e}"))?;
    let project = GodotProject::discover(&cwd)?;
    let root = &project.root;

    let workspace = WorkspaceIndex::new(root.clone());

    let files = collect_files(&workspace, root, &args.paths);
    let entries = build_entries(&workspace, &files, root);
    let signal_flow = build_signal_flow(&workspace, &files, root, &args.paths);
    let autoloads = build_autoloads(&workspace, root);

    let data = OverviewData {
        project: project.name().unwrap_or_else(|_| "Untitled".to_string()),
        godot_version: parse_godot_version(&project.project_file),
        scripts: entries.len(),
        scenes: workspace.scene_count(),
        entries,
        signal_flow,
        autoloads,
    };

    match args.format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&data)
                .map_err(|e| miette!("Failed to serialize: {e}"))?;
            cprintln!("{json}");
        }
        "text" => output_text(&data),
        _ => return Err(miette!("Invalid format: {}", args.format)),
    }

    Ok(())
}

fn collect_files(
    workspace: &WorkspaceIndex,
    root: &Path,
    scope: &[String],
) -> Vec<(String, PathBuf)> {
    let mut files: Vec<(String, PathBuf)> = Vec::new();
    for (path, _content) in workspace.all_files() {
        if let Ok(rel) = path.strip_prefix(root) {
            files.push((rel.to_slash_lossy().to_string(), path));
        }
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));

    if !scope.is_empty() {
        files.retain(|(rel, _)| {
            scope
                .iter()
                .any(|prefix| rel.starts_with(prefix.trim_end_matches('/')))
        });
    }

    files
}

fn build_entries(
    workspace: &WorkspaceIndex,
    files: &[(String, PathBuf)],
    root: &Path,
) -> Vec<ScriptEntry> {
    let mut entries: Vec<ScriptEntry> = Vec::new();
    for (rel, abs_path) in files {
        let Some(table) = workspace.get_symbols(abs_path) else {
            continue;
        };

        let scenes = workspace.scenes_for_script(abs_path);
        let scene = scenes.first().map(|spn| {
            format!(
                "{} \u{2192} {}",
                rel_slash(&spn.scene_path, root),
                spn.node_name
            )
        });

        let signals: Vec<String> = table.signals.iter().map(|s| s.name.clone()).collect();

        let exports: Vec<String> = table
            .variables
            .iter()
            .filter(|v| v.annotations.iter().any(|a| a == "export"))
            .map(|v| {
                v.type_ann.as_ref().map_or_else(
                    || v.name.clone(),
                    |ty| format!("{}: {}", v.name, ty.name),
                )
            })
            .collect();

        let funcs: Vec<String> = table
            .functions
            .iter()
            .map(|f| {
                let params = f
                    .params
                    .iter()
                    .map(|p| p.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({params})", f.name)
            })
            .collect();

        entries.push(ScriptEntry {
            file: rel.clone(),
            extends: table.extends.clone(),
            class_name: table.class_name.clone(),
            scene,
            signals,
            exports,
            funcs,
        });
    }
    entries
}

fn build_signal_flow(
    workspace: &WorkspaceIndex,
    files: &[(String, PathBuf)],
    root: &Path,
    scope: &[String],
) -> Vec<SceneFlowGroup> {
    let mut flow_map: BTreeMap<String, Vec<FlowConnection>> = BTreeMap::new();

    // Scene-based connections (.tscn)
    collect_scene_connections(workspace, files, root, scope, &mut flow_map);

    // Code-based connections (.connect() calls in GDScript)
    collect_code_connections(workspace, files, root, &mut flow_map);

    flow_map
        .into_iter()
        .map(|(scene, connections)| SceneFlowGroup { scene, connections })
        .collect()
}

fn collect_scene_connections(
    workspace: &WorkspaceIndex,
    files: &[(String, PathBuf)],
    root: &Path,
    scope: &[String],
    flow_map: &mut BTreeMap<String, Vec<FlowConnection>>,
) {
    for entry in workspace.iter_scene_connections() {
        let scene_path: &PathBuf = entry.key();
        let connections = entry.value();
        let scene_rel = rel_slash(scene_path, root);

        if !scope.is_empty() {
            let scene_in_scope = scope
                .iter()
                .any(|prefix| scene_rel.starts_with(prefix.trim_end_matches('/')));
            let scripts_in_scope = files.iter().any(|(_, abs)| {
                workspace
                    .scenes_for_script(abs)
                    .iter()
                    .any(|spn| spn.scene_path == *scene_path)
            });
            if !scene_in_scope && !scripts_in_scope {
                continue;
            }
        }

        let flow_connections: Vec<FlowConnection> = connections
            .iter()
            .map(|conn| FlowConnection {
                from_node: conn.from_node.clone(),
                signal: conn.signal.clone(),
                to_node: conn.to_node.clone(),
                method: conn.method.clone(),
            })
            .collect();
        if !flow_connections.is_empty() {
            flow_map
                .entry(scene_rel)
                .or_default()
                .extend(flow_connections);
        }
    }
}

fn collect_code_connections(
    workspace: &WorkspaceIndex,
    files: &[(String, PathBuf)],
    root: &Path,
    flow_map: &mut BTreeMap<String, Vec<FlowConnection>>,
) {
    for (rel, abs_path) in files {
        let Some(content) = workspace.get_content(abs_path) else {
            continue;
        };
        let Ok(tree) = crate::core::parser::parse(&content) else {
            continue;
        };

        let connections = find_connect_calls(tree.root_node(), &content);
        if connections.is_empty() {
            continue;
        }

        // Determine the label — use the scene name if this script has one, else the file path
        let label = workspace
            .scenes_for_script(abs_path)
            .first()
            .map_or_else(
                || rel.clone(),
                |spn| rel_slash(&spn.scene_path, root),
            );

        let flow_connections: Vec<FlowConnection> = connections
            .into_iter()
            .map(|cc| FlowConnection {
                from_node: cc.receiver,
                signal: cc.signal,
                to_node: String::new(),
                method: cc.handler,
            })
            .collect();
        flow_map
            .entry(label)
            .or_default()
            .extend(flow_connections);
    }
}

/// Walk the AST to find `signal.connect(handler)` calls.
///
/// Handles patterns:
///   `my_signal.connect(handler)`
///   `$Node.signal.connect(handler)`
///   `EventBus.signal.connect(handler)`
fn find_connect_calls(root: Node, source: &str) -> Vec<CodeConnection> {
    let mut results = Vec::new();
    walk_for_connects(root, source, &mut results);
    results
}

fn walk_for_connects(node: Node, source: &str, out: &mut Vec<CodeConnection>) {
    // Look for `attribute` nodes that contain an `attribute_call` with method "connect"
    if node.kind() == "attribute"
        && let Some(cc) = try_parse_connect(node, source)
    {
        out.push(cc);
        return; // Don't recurse into children we already processed
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_for_connects(child, source, out);
    }
}

/// Try to extract a `CodeConnection` from an `attribute` node like `X.connect(Y)`.
///
/// tree-sitter flattens `A.B.connect(C)` into a single `attribute` node with children:
///   `identifier("A")`, `identifier("B")`, `attribute_call` { `identifier("connect")`, `arguments` { `identifier("C")` } }
///
/// For `signal.connect(C)`: `identifier("signal")`, `attribute_call` { ... }
fn try_parse_connect(node: Node, source: &str) -> Option<CodeConnection> {
    let bytes = source.as_bytes();

    // Collect identifiers and find the attribute_call for "connect"
    let mut idents: Vec<String> = Vec::new();
    let mut call_node = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" | "get_node" => {
                if let Ok(text) = child.utf8_text(bytes) {
                    idents.push(text.to_string());
                }
            }
            "attribute_call"
                if child
                    .named_child(0)
                    .and_then(|n| n.utf8_text(bytes).ok())
                    == Some("connect") =>
            {
                call_node = Some(child);
            }
            _ => {}
        }
    }
    let call = call_node?;

    // Need at least one identifier before .connect() (the signal name)
    if idents.is_empty() {
        return None;
    }

    // Extract handler name from first argument
    let args = call
        .children(&mut call.walk())
        .find(|c| c.kind() == "arguments")?;
    let first_arg = args.named_child(0)?;
    let handler = first_arg.utf8_text(bytes).ok()?.to_string();

    // Last identifier is the signal name; preceding identifiers form the receiver
    let signal = idents.pop()?;
    let receiver = if idents.is_empty() {
        "self".to_string()
    } else {
        idents.join(".")
    };

    Some(CodeConnection {
        signal,
        handler,
        receiver,
    })
}

fn build_autoloads(workspace: &WorkspaceIndex, root: &Path) -> Vec<AutoloadEntry> {
    let mut autoloads: Vec<AutoloadEntry> = Vec::new();
    for (name, path) in workspace.iter_autoloads() {
        autoloads.push(AutoloadEntry {
            name,
            script: rel_slash(&path, root),
        });
    }
    autoloads.sort_by(|a, b| a.name.cmp(&b.name));
    autoloads
}

fn output_text(data: &OverviewData) {
    let version_str = data
        .godot_version
        .as_deref()
        .map_or(String::new(), |v| format!("Godot {v} | "));
    cprintln!("# {}", data.project);
    cprintln!(
        "# {}{} scripts | {} scenes",
        version_str,
        data.scripts,
        data.scenes
    );

    if !data.entries.is_empty() {
        cprintln!();
        cprintln!("## Scripts");
        cprintln!();
        for entry in &data.entries {
            let extends = entry
                .extends
                .as_deref()
                .map_or(String::new(), |e| format!(" (extends {e})"));
            let class = entry
                .class_name
                .as_deref()
                .map_or(String::new(), |c| format!(" [{c}]"));
            cprintln!("{}{extends}{class}", entry.file);
            if let Some(ref scene) = entry.scene {
                cprintln!("  scene: {scene}");
            }
            if !entry.signals.is_empty() {
                cprintln!("  signals: {}", entry.signals.join(", "));
            }
            if !entry.exports.is_empty() {
                cprintln!("  exports: {}", entry.exports.join(", "));
            }
            if !entry.funcs.is_empty() {
                cprintln!("  funcs: {}", entry.funcs.join(", "));
            }
            cprintln!();
        }
    }

    if !data.signal_flow.is_empty() {
        cprintln!("## Signal Flow");
        cprintln!();
        for group in &data.signal_flow {
            cprintln!("{}:", group.scene);
            for conn in &group.connections {
                if conn.from_node.is_empty() && conn.to_node.is_empty() {
                    // Code connection with no receiver context
                    cprintln!("  {} \u{2192} {}", conn.signal, conn.method);
                } else if conn.to_node.is_empty() {
                    // Code connection: receiver.signal → handler
                    cprintln!(
                        "  {}.{} \u{2192} {}",
                        conn.from_node,
                        conn.signal,
                        conn.method
                    );
                } else {
                    // Scene connection: from.signal → to.method
                    cprintln!(
                        "  {}.{} \u{2192} {}.{}",
                        conn.from_node,
                        conn.signal,
                        conn.to_node,
                        conn.method
                    );
                }
            }
            cprintln!();
        }
    }

    cprintln!("## Autoloads");
    cprintln!();
    if data.autoloads.is_empty() {
        cprintln!("(none)");
    } else {
        for al in &data.autoloads {
            cprintln!("{}: {}", al.name, al.script);
        }
    }
}

/// Parse the Godot version from `config/features=PackedStringArray("4.4", ...)`.
fn parse_godot_version(project_file: &Path) -> Option<String> {
    let content = std::fs::read_to_string(project_file).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("config/features=PackedStringArray(") {
            let start = rest.find('"')? + 1;
            let end = start + rest[start..].find('"')?;
            return Some(rest[start..end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests;
