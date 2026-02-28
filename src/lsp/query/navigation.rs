use std::collections::HashMap;
use std::path::Path;

use miette::Result;
use serde::Serialize;
use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, GotoDefinitionResponse, HoverContents, Location,
    MarkedString, Position, Range, TextEdit, Url, WorkspaceEdit,
};

use super::{
    CompletionOutput, DefinitionOutput, HoverOutput, ReferenceOutput, ReferencesOutput,
    RenameOutput, convert_workspace_edit, find_root, make_uri, resolve_file, url_to_relative,
};

// ── Scene query output structs ───────────────────────────────────────────────

#[derive(Serialize)]
pub struct SceneRefOutput {
    pub scene: String,
    pub node: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
}

#[derive(Serialize)]
pub struct SignalConnectionOutput {
    pub signal: String,
    pub from_node: String,
    pub to_node: String,
    pub method: String,
    pub scene: String,
}

// ── Private helpers ──────────────────────────────────────────────────────────

fn get_symbol_name(source: &str, position: Position) -> Result<String> {
    let tree =
        crate::core::parser::parse(source).map_err(|e| miette::miette!("parse error: {e}"))?;
    let root = tree.root_node();
    let point = tree_sitter::Point::new(position.line as usize, position.character as usize);
    let node = root
        .descendant_for_point_range(point, point)
        .ok_or_else(|| miette::miette!("no symbol at that position"))?;
    let text = node
        .utf8_text(source.as_bytes())
        .map_err(|e| miette::miette!("cannot read symbol text: {e}"))?;
    if text.is_empty() {
        return Err(miette::miette!("no symbol at that position"));
    }
    Ok(text.to_string())
}

fn to_position(line: usize, column: usize) -> Position {
    Position::new((line - 1) as u32, (column - 1) as u32)
}

fn range_to_reference(
    range: &Range,
    uri: &Url,
    base: &Path,
    file_cache: &mut HashMap<String, String>,
) -> ReferenceOutput {
    let file = url_to_relative(uri, base);
    let line_num = range.start.line + 1;
    let context = if let Ok(path) = uri.to_file_path() {
        let key = path.to_string_lossy().to_string();
        let source = file_cache
            .entry(key)
            .or_insert_with(|| std::fs::read_to_string(&path).unwrap_or_default());
        source
            .lines()
            .nth(range.start.line as usize)
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        String::new()
    };
    ReferenceOutput {
        file,
        line: line_num,
        column: range.start.character + 1,
        end_line: range.end.line + 1,
        end_column: range.end.character + 1,
        context,
    }
}

fn completion_kind_str(kind: Option<CompletionItemKind>) -> String {
    match kind {
        Some(CompletionItemKind::FUNCTION) => "function",
        Some(CompletionItemKind::METHOD) => "method",
        Some(CompletionItemKind::VARIABLE) => "variable",
        Some(CompletionItemKind::PROPERTY) => "property",
        Some(CompletionItemKind::FIELD) => "field",
        Some(CompletionItemKind::CLASS) => "class",
        Some(CompletionItemKind::CONSTANT) => "constant",
        Some(CompletionItemKind::ENUM) => "enum",
        Some(CompletionItemKind::ENUM_MEMBER) => "enum_member",
        Some(CompletionItemKind::KEYWORD) => "keyword",
        Some(CompletionItemKind::EVENT) => "event",
        _ => "unknown",
    }
    .to_string()
}

fn extract_godot_definition(val: &serde_json::Value) -> Option<GotoDefinitionResponse> {
    // Godot returns either a single location or an array
    let parse_loc = |obj: &serde_json::Value| -> Option<Location> {
        let uri_str = obj.get("uri")?.as_str()?;
        let uri = Url::parse(uri_str).ok()?;
        let range = obj.get("range")?;
        let start = range.get("start")?;
        let end = range.get("end")?;
        Some(Location {
            uri,
            range: Range {
                start: Position::new(
                    start.get("line")?.as_u64()? as u32,
                    start.get("character")?.as_u64()? as u32,
                ),
                end: Position::new(
                    end.get("line")?.as_u64()? as u32,
                    end.get("character")?.as_u64()? as u32,
                ),
            },
        })
    };

    if let Some(arr) = val.as_array() {
        let locs: Vec<Location> = arr.iter().filter_map(parse_loc).collect();
        if locs.is_empty() {
            return None;
        }
        Some(GotoDefinitionResponse::Array(locs))
    } else {
        Some(GotoDefinitionResponse::Scalar(parse_loc(val)?))
    }
}

fn extract_godot_hover_text(val: &serde_json::Value) -> Option<String> {
    let contents = val.get("contents")?;
    let text = if let Some(s) = contents.as_str() {
        s.to_string()
    } else if let Some(obj) = contents.as_object() {
        obj.get("value")?.as_str()?.to_string()
    } else {
        return None;
    };
    if text.is_empty() { None } else { Some(text) }
}

fn extract_godot_completions(val: &serde_json::Value) -> Option<Vec<CompletionItem>> {
    let items = val
        .as_array()
        .or_else(|| val.get("items").and_then(|i| i.as_array()))?;
    let mut result = Vec::new();
    for item in items {
        let label = item.get("label")?.as_str()?.to_string();
        let kind = item
            .get("kind")
            .and_then(serde_json::Value::as_u64)
            .and_then(|k| serde_json::from_value(serde_json::Value::Number(k.into())).ok());
        let detail = item
            .get("detail")
            .and_then(|d| d.as_str())
            .map(std::string::ToString::to_string);
        result.push(CompletionItem {
            label,
            kind,
            detail,
            ..Default::default()
        });
    }
    Some(result)
}

// ── Query functions ──────────────────────────────────────────────────────────

pub fn query_rename(
    file: &str,
    line: usize,
    column: usize,
    new_name: &str,
) -> Result<RenameOutput> {
    let path = resolve_file(file)?;
    let uri = make_uri(&path)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);
    let symbol = get_symbol_name(&source, position)?;

    let project_root = find_root(&path)?;
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.clone());

    let edit = crate::lsp::rename::rename_cross_file(&source, &uri, position, new_name, &workspace)
        .ok_or_else(|| miette::miette!("no renameable symbol at {file}:{line}:{column}"))?;

    let mut warnings = Vec::new();
    if let Ok(tree) = crate::core::parser::parse(&source) {
        let point = tree_sitter::Point::new(line - 1, column - 1);
        let gd_file = crate::core::gd_ast::convert(&tree, &source);
        let scope_names = crate::lsp::refactor::collision::collect_scope_names(
            tree.root_node(),
            &source,
            point,
            &gd_file,
        );
        if let Some(kind) = crate::lsp::refactor::collision::check_collision(new_name, &scope_names)
        {
            warnings.push(format!("'{new_name}' collides with a {kind}"));
        }
    }

    let changes = convert_workspace_edit(&edit, &project_root);
    Ok(RenameOutput {
        symbol,
        new_name: new_name.to_string(),
        changes,
        summary: None,
        warnings,
    })
}

pub fn query_rename_by_name(
    name: &str,
    new_name: &str,
    file: Option<&str>,
) -> Result<RenameOutput> {
    let (project_root, file_path) = if let Some(f) = file {
        let path = resolve_file(f)?;
        let root = find_root(&path)?;
        (root, Some(path))
    } else {
        let cwd = std::env::current_dir()
            .map_err(|e| miette::miette!("cannot get current directory: {e}"))?;
        let root = find_root(&cwd)?;
        (root, None)
    };

    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.clone());
    let locations = crate::lsp::references::find_references_by_name(
        name,
        &workspace,
        file_path.as_deref(),
        None,
    );

    if locations.is_empty() {
        return Err(miette::miette!("no references found for '{name}'"));
    }

    // Build WorkspaceEdit from locations
    let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
    for loc in &locations {
        changes.entry(loc.uri.clone()).or_default().push(TextEdit {
            range: loc.range,
            new_text: new_name.to_string(),
        });
    }

    let edit = WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    };

    let mut warnings = Vec::new();
    // Check collision at the first reference location
    if let Some(first) = locations.first()
        && let Ok(first_path) = first.uri.to_file_path()
        && let Ok(first_source) = std::fs::read_to_string(&first_path)
        && let Ok(tree) = crate::core::parser::parse(&first_source)
    {
        let point = tree_sitter::Point::new(
            first.range.start.line as usize,
            first.range.start.character as usize,
        );
        let gd_file = crate::core::gd_ast::convert(&tree, &first_source);
        let scope_names = crate::lsp::refactor::collision::collect_scope_names(
            tree.root_node(),
            &first_source,
            point,
            &gd_file,
        );
        if let Some(kind) = crate::lsp::refactor::collision::check_collision(new_name, &scope_names)
        {
            warnings.push(format!("'{new_name}' collides with a {kind}"));
        }
    }

    let file_edits = convert_workspace_edit(&edit, &project_root);
    Ok(RenameOutput {
        symbol: name.to_string(),
        new_name: new_name.to_string(),
        changes: file_edits,
        summary: None,
        warnings,
    })
}

pub fn query_references(file: &str, line: usize, column: usize) -> Result<ReferencesOutput> {
    let path = resolve_file(file)?;
    let uri = make_uri(&path)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);
    let symbol = get_symbol_name(&source, position)?;

    let project_root = find_root(&path)?;
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.clone());

    let locations = crate::lsp::references::find_references_cross_file(
        &source, &uri, position, true, &workspace,
    )
    .unwrap_or_default();

    let mut file_cache = HashMap::new();
    let references = locations
        .iter()
        .map(|loc| range_to_reference(&loc.range, &loc.uri, &project_root, &mut file_cache))
        .collect();

    Ok(ReferencesOutput { symbol, references })
}

pub fn query_references_by_name(
    name: &str,
    file: Option<&str>,
    class: Option<&str>,
) -> Result<ReferencesOutput> {
    let (project_root, file_path) = if let Some(f) = file {
        let path = resolve_file(f)?;
        let root = find_root(&path)?;
        (root, Some(path))
    } else {
        let cwd = std::env::current_dir()
            .map_err(|e| miette::miette!("cannot get current directory: {e}"))?;
        let root = find_root(&cwd)?;
        (root, None)
    };

    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.clone());

    let locations = crate::lsp::references::find_references_by_name(
        name,
        &workspace,
        file_path.as_deref(),
        class,
    );

    let mut file_cache = HashMap::new();
    let references = locations
        .iter()
        .map(|loc| range_to_reference(&loc.range, &loc.uri, &project_root, &mut file_cache))
        .collect();

    Ok(ReferencesOutput {
        symbol: name.to_string(),
        references,
    })
}

pub fn query_definition(
    file: &str,
    line: usize,
    column: usize,
    godot: Option<&crate::lsp::godot_client::GodotClient>,
) -> Result<DefinitionOutput> {
    let path = resolve_file(file)?;
    let uri = make_uri(&path)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);
    let symbol = get_symbol_name(&source, position)?;

    let project_root = find_root(&path)?;

    // Godot-first: if Godot returns a definition, use it exclusively
    if let Some(proxy) = godot {
        let godot_uri = proxy.to_godot_uri(uri.as_str());
        if let Some(godot_val) = proxy.definition(&godot_uri, position.line, position.character)
            && let Some(response) = extract_godot_definition(&godot_val)
        {
            let location = match response {
                GotoDefinitionResponse::Scalar(loc) => loc,
                GotoDefinitionResponse::Array(locs) => {
                    if let Some(loc) = locs.into_iter().next() {
                        loc
                    } else {
                        return Err(miette::miette!("no definition found for '{symbol}'"));
                    }
                }
                GotoDefinitionResponse::Link(links) => {
                    if let Some(link) = links.into_iter().next() {
                        Location {
                            uri: link.target_uri,
                            range: link.target_selection_range,
                        }
                    } else {
                        return Err(miette::miette!("no definition found for '{symbol}'"));
                    }
                }
            };
            return Ok(DefinitionOutput {
                symbol,
                file: url_to_relative(&location.uri, &project_root),
                line: location.range.start.line + 1,
                column: location.range.start.character + 1,
                end_line: location.range.end.line + 1,
                end_column: location.range.end.character + 1,
            });
        }
    }

    // Fallback: static analysis
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.clone());
    let response =
        crate::lsp::definition::goto_definition_cross_file(&source, &uri, position, &workspace)
            .ok_or_else(|| miette::miette!("no definition found for '{symbol}'"))?;

    let location = match response {
        GotoDefinitionResponse::Scalar(loc) => loc,
        GotoDefinitionResponse::Array(locs) => locs
            .into_iter()
            .next()
            .ok_or_else(|| miette::miette!("no definition found"))?,
        GotoDefinitionResponse::Link(links) => {
            let link = links
                .into_iter()
                .next()
                .ok_or_else(|| miette::miette!("no definition found"))?;
            Location {
                uri: link.target_uri,
                range: link.target_selection_range,
            }
        }
    };

    Ok(DefinitionOutput {
        symbol,
        file: url_to_relative(&location.uri, &project_root),
        line: location.range.start.line + 1,
        column: location.range.start.character + 1,
        end_line: location.range.end.line + 1,
        end_column: location.range.end.character + 1,
    })
}

pub fn query_hover(
    file: &str,
    line: usize,
    column: usize,
    godot: Option<&crate::lsp::godot_client::GodotClient>,
) -> Result<HoverOutput> {
    let path = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);

    // Godot-first: if Godot proxy returns data, use it exclusively
    if let Some(proxy) = godot
        && let Some(text) = (|| {
            let uri = make_uri(&path).ok()?;
            let godot_uri = proxy.to_godot_uri(uri.as_str());
            let val = proxy.hover(&godot_uri, position.line, position.character)?;
            extract_godot_hover_text(&val)
        })()
    {
        return Ok(HoverOutput {
            content: text,
            line: line as u32,
            column: column as u32,
        });
    }

    // Build workspace index for cross-file resolution
    let workspace = crate::core::config::find_project_root(&path)
        .map(crate::lsp::workspace::WorkspaceIndex::new);

    // Fallback: static analysis
    let content = crate::lsp::hover::hover_at(&source, position, workspace.as_ref(), Some(&path))
        .map(|h| match h.contents {
            HoverContents::Markup(markup) => markup.value,
            HoverContents::Scalar(MarkedString::String(s)) => s,
            HoverContents::Scalar(MarkedString::LanguageString(ls)) => ls.value,
            HoverContents::Array(arr) => arr
                .into_iter()
                .map(|ms| match ms {
                    MarkedString::String(s) => s,
                    MarkedString::LanguageString(ls) => ls.value,
                })
                .collect::<Vec<_>>()
                .join("\n"),
        })
        .ok_or_else(|| miette::miette!("no hover information at {file}:{line}:{column}"))?;

    Ok(HoverOutput {
        content,
        line: line as u32,
        column: column as u32,
    })
}

pub fn query_completions(
    file: &str,
    line: usize,
    column: usize,
    godot: Option<&crate::lsp::godot_client::GodotClient>,
) -> Result<Vec<CompletionOutput>> {
    let path = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);

    // Our dot-completions first (handles class_name refs, chains, typed vars)
    let workspace = crate::core::config::find_project_root(&path)
        .map(crate::lsp::workspace::WorkspaceIndex::new);
    if let Some(dot_items) =
        crate::lsp::completion::try_dot_completions(&source, position, workspace.as_ref())
    {
        return Ok(dot_items
            .into_iter()
            .map(|item| CompletionOutput {
                label: item.label,
                kind: completion_kind_str(item.kind),
                detail: item.detail,
            })
            .collect());
    }

    // Godot proxy for non-dot contexts (or unresolved dot receivers)
    if let Some(proxy) = godot
        && let Ok(uri) = make_uri(&path)
    {
        let godot_uri = proxy.to_godot_uri(uri.as_str());
        if let Some(val) = proxy.completion(&godot_uri, position.line, position.character)
            && let Some(godot_items) = extract_godot_completions(&val)
            && !godot_items.is_empty()
        {
            return Ok(godot_items
                .into_iter()
                .map(|item| CompletionOutput {
                    label: item.label,
                    kind: completion_kind_str(item.kind),
                    detail: item.detail,
                })
                .collect());
        }
    }

    // Fallback: global completions (workspace already built above)
    let items = crate::lsp::completion::provide_completions(
        &source,
        position,
        workspace.as_ref(),
        Some(&path),
    );

    Ok(items
        .into_iter()
        .map(|item| CompletionOutput {
            label: item.label,
            kind: completion_kind_str(item.kind),
            detail: item.detail,
        })
        .collect())
}

// ── Scene-aware queries ──────────────────────────────────────────────────────

pub fn query_scene_refs(file: &str) -> Result<Vec<SceneRefOutput>> {
    let path = resolve_file(file)?;
    let root = find_root(&path)?;
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(root.clone());

    let entries = workspace.scenes_for_script(&path);
    Ok(entries
        .into_iter()
        .map(|spn| SceneRefOutput {
            scene: crate::core::fs::relative_slash(&spn.scene_path, &root),
            node: spn.node_name,
            node_type: spn.node_type,
            parent: spn.node_parent,
        })
        .collect())
}

pub fn query_signal_connections(file: &str) -> Result<Vec<SignalConnectionOutput>> {
    let path = resolve_file(file)?;
    let root = find_root(&path)?;
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(root.clone());

    // Collect all signal connections from scenes that use this script
    let mut results = Vec::new();
    let scene_nodes = workspace.scenes_for_script(&path);
    for spn in &scene_nodes {
        for entry in workspace.iter_scene_connections() {
            if *entry.key() != spn.scene_path {
                continue;
            }
            for conn in entry.value() {
                results.push(SignalConnectionOutput {
                    signal: conn.signal.clone(),
                    from_node: conn.from_node.clone(),
                    to_node: conn.to_node.clone(),
                    method: conn.method.clone(),
                    scene: crate::core::fs::relative_slash(&conn.scene_path, &root),
                });
            }
        }
    }
    Ok(results)
}
