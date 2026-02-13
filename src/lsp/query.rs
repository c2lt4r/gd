use std::path::{Path, PathBuf};

use miette::Result;
use serde::Serialize;
use tower_lsp::lsp_types::*;

// ── Output structs ──────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RenameOutput {
    pub symbol: String,
    pub new_name: String,
    pub changes: Vec<FileEdits>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

#[derive(Serialize)]
pub struct FileEdits {
    pub file: String,
    pub edits: Vec<TextEditOutput>,
}

#[derive(Serialize)]
pub struct TextEditOutput {
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub new_text: String,
}

#[derive(Serialize)]
pub struct ReferencesOutput {
    pub symbol: String,
    pub references: Vec<ReferenceOutput>,
}

#[derive(Serialize)]
pub struct ReferenceOutput {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub context: String,
}

#[derive(Serialize)]
pub struct DefinitionOutput {
    pub symbol: String,
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
}

#[derive(Serialize)]
pub struct HoverOutput {
    pub content: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Serialize)]
pub struct CompletionOutput {
    pub label: String,
    pub kind: String,
    pub detail: Option<String>,
}

#[derive(Serialize)]
pub struct CodeActionOutput {
    pub title: String,
    pub edits: Vec<FileEditEntry>,
}

#[derive(Serialize)]
pub struct FileEditEntry {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub new_text: String,
}

#[derive(Serialize)]
pub struct SymbolOutput {
    pub name: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    pub line: u32,
    pub column: u32,
}

#[derive(Serialize)]
pub struct SafeDeleteFileOutput {
    pub file: String,
    pub references: Vec<FileReference>,
    pub deleted: bool,
}

#[derive(Serialize)]
pub struct FileReference {
    pub file: String,
    pub line: u32,
    pub kind: String,
    pub text: String,
}

#[derive(Serialize)]
pub struct ImplementationsOutput {
    pub method: String,
    pub implementations: Vec<ImplementationEntry>,
}

#[derive(Serialize)]
pub struct ImplementationEntry {
    pub file: String,
    pub line: u32,
    pub end_line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extends: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
}

#[derive(Serialize)]
pub struct CreateFileOutput {
    pub file: String,
    pub extends: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_name: Option<String>,
    pub applied: bool,
    pub lines: u32,
}

#[derive(Serialize)]
pub struct ViewOutput {
    pub file: String,
    pub start_line: u32,
    pub end_line: u32,
    pub total_lines: u32,
    pub content: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

pub(super) fn resolve_file(file: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette::miette!("cannot get current directory: {e}"))?;
    let path = cwd.join(file);
    if !path.exists() {
        return Err(miette::miette!("file not found: {file}"));
    }
    Ok(path)
}

pub(super) fn make_uri(path: &Path) -> Result<Url> {
    Url::from_file_path(path).map_err(|_| miette::miette!("invalid path: {}", path.display()))
}

pub(super) fn find_root(path: &Path) -> Result<PathBuf> {
    crate::core::config::find_project_root(path)
        .ok_or_else(|| miette::miette!("no project.godot found above {}", path.display()))
}

pub(super) fn url_to_relative(url: &Url, base: &Path) -> String {
    if let Ok(path) = url.to_file_path() {
        return crate::core::fs::relative_slash(&path, base);
    }
    url.to_string()
}

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
    file_cache: &mut std::collections::HashMap<String, String>,
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
        Some(CompletionItemKind::FIELD) => "field",
        Some(CompletionItemKind::CLASS) => "class",
        Some(CompletionItemKind::CONSTANT) => "constant",
        Some(CompletionItemKind::ENUM) => "enum",
        Some(CompletionItemKind::KEYWORD) => "keyword",
        Some(CompletionItemKind::EVENT) => "event",
        _ => "unknown",
    }
    .to_string()
}

#[allow(deprecated)] // DocumentSymbol::deprecated field
fn symbol_kind_str(kind: SymbolKind) -> String {
    match kind {
        SymbolKind::FUNCTION => "function",
        SymbolKind::METHOD => "method",
        SymbolKind::VARIABLE => "variable",
        SymbolKind::FIELD => "field",
        SymbolKind::CLASS => "class",
        SymbolKind::CONSTANT => "constant",
        SymbolKind::ENUM => "enum",
        SymbolKind::EVENT => "event",
        _ => "unknown",
    }
    .to_string()
}

#[allow(deprecated)]
fn flatten_symbols(symbols: &[DocumentSymbol]) -> Vec<SymbolOutput> {
    let mut out = Vec::new();
    for s in symbols {
        out.push(SymbolOutput {
            name: s.name.clone(),
            kind: symbol_kind_str(s.kind),
            detail: s.detail.clone(),
            line: s.selection_range.start.line + 1,
            column: s.selection_range.start.character + 1,
        });
        if let Some(children) = &s.children {
            out.extend(flatten_symbols(children));
        }
    }
    out
}

fn position_to_byte_offset(source: &str, pos: Position) -> usize {
    let mut line = 0u32;
    let mut col = 0u32;
    for (i, ch) in source.char_indices() {
        if line == pos.line && col == pos.character {
            return i;
        }
        if ch == '\n' {
            if line == pos.line {
                return i;
            }
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    source.len()
}

// ── Query functions ─────────────────────────────────────────────────────────

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
    let workspace = super::workspace::WorkspaceIndex::new(project_root.clone());

    let edit = super::rename::rename_cross_file(&source, &uri, position, new_name, &workspace)
        .ok_or_else(|| miette::miette!("no renameable symbol at {file}:{line}:{column}"))?;

    let changes = convert_workspace_edit(&edit, &project_root);
    Ok(RenameOutput {
        symbol,
        new_name: new_name.to_string(),
        changes,
        summary: None,
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

    let workspace = super::workspace::WorkspaceIndex::new(project_root.clone());
    let locations =
        super::references::find_references_by_name(name, &workspace, file_path.as_deref(), None);

    if locations.is_empty() {
        return Err(miette::miette!("no references found for '{name}'"));
    }

    // Build WorkspaceEdit from locations
    let mut changes: std::collections::HashMap<Url, Vec<TextEdit>> =
        std::collections::HashMap::new();
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

    let file_edits = convert_workspace_edit(&edit, &project_root);
    Ok(RenameOutput {
        symbol: name.to_string(),
        new_name: new_name.to_string(),
        changes: file_edits,
        summary: None,
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
    let workspace = super::workspace::WorkspaceIndex::new(project_root.clone());

    let locations =
        super::references::find_references_cross_file(&source, &uri, position, true, &workspace)
            .unwrap_or_default();

    let mut file_cache = std::collections::HashMap::new();
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

    let workspace = super::workspace::WorkspaceIndex::new(project_root.clone());

    let locations =
        super::references::find_references_by_name(name, &workspace, file_path.as_deref(), class);

    let mut file_cache = std::collections::HashMap::new();
    let references = locations
        .iter()
        .map(|loc| range_to_reference(&loc.range, &loc.uri, &project_root, &mut file_cache))
        .collect();

    Ok(ReferencesOutput {
        symbol: name.to_string(),
        references,
    })
}

pub fn query_definition(file: &str, line: usize, column: usize) -> Result<DefinitionOutput> {
    let path = resolve_file(file)?;
    let uri = make_uri(&path)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);
    let symbol = get_symbol_name(&source, position)?;

    let project_root = find_root(&path)?;
    let workspace = super::workspace::WorkspaceIndex::new(project_root.clone());

    let response =
        super::definition::goto_definition_cross_file(&source, &uri, position, &workspace)
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

pub fn query_hover(file: &str, line: usize, column: usize) -> Result<HoverOutput> {
    let path = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);

    let hover = super::hover::hover_at(&source, position)
        .ok_or_else(|| miette::miette!("no hover information at {file}:{line}:{column}"))?;

    let content = match hover.contents {
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
    };

    Ok(HoverOutput {
        content,
        line: line as u32,
        column: column as u32,
    })
}

pub fn query_completions(file: &str, line: usize, column: usize) -> Result<Vec<CompletionOutput>> {
    let path = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let position = to_position(line, column);

    let workspace =
        crate::core::config::find_project_root(&path).map(super::workspace::WorkspaceIndex::new);

    let items = super::completion::provide_completions(&source, position, workspace.as_ref());

    Ok(items
        .into_iter()
        .map(|item| CompletionOutput {
            label: item.label,
            kind: completion_kind_str(item.kind),
            detail: item.detail,
        })
        .collect())
}

pub fn query_code_actions(file: &str, line: usize, column: usize) -> Result<Vec<CodeActionOutput>> {
    let path = resolve_file(file)?;
    let uri = make_uri(&path)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;

    let project_root = crate::core::config::find_project_root(&path)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Range covering the specified line
    let lsp_line = (line - 1) as u32;
    let range = Range::new(
        Position::new(lsp_line, (column - 1) as u32),
        Position::new(lsp_line, u32::MAX),
    );

    let actions = super::actions::provide_code_actions(&uri, &source, &range).unwrap_or_default();

    Ok(actions
        .into_iter()
        .filter_map(|action| match action {
            CodeActionOrCommand::CodeAction(ca) => {
                let edits = ca
                    .edit
                    .and_then(|we| {
                        we.changes.map(|changes| {
                            changes
                                .into_iter()
                                .flat_map(|(url, edits)| {
                                    let file = url_to_relative(&url, &project_root);
                                    edits.into_iter().map(move |e| FileEditEntry {
                                        file: file.clone(),
                                        line: e.range.start.line + 1,
                                        column: e.range.start.character + 1,
                                        end_line: e.range.end.line + 1,
                                        end_column: e.range.end.character + 1,
                                        new_text: e.new_text,
                                    })
                                })
                                .collect()
                        })
                    })
                    .unwrap_or_default();
                Some(CodeActionOutput {
                    title: ca.title,
                    edits,
                })
            }
            _ => None,
        })
        .collect())
}

pub fn query_diagnostics(paths: &[String]) -> Result<()> {
    // Delegate to the lint system with JSON output
    let opts = crate::lint::LintOptions {
        format: "json".to_string(),
        ..Default::default()
    };
    crate::lint::run_lint(paths, &opts)
}

pub fn query_symbols(file: &str) -> Result<Vec<SymbolOutput>> {
    let path = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;

    let response = super::symbols::document_symbols(&source)
        .ok_or_else(|| miette::miette!("no symbols found in {file}"))?;

    match response {
        DocumentSymbolResponse::Nested(symbols) => Ok(flatten_symbols(&symbols)),
        DocumentSymbolResponse::Flat(symbols) => Ok(symbols
            .into_iter()
            .map(|s| SymbolOutput {
                name: s.name,
                kind: symbol_kind_str(s.kind),
                detail: None,
                line: s.location.range.start.line + 1,
                column: s.location.range.start.character + 1,
            })
            .collect()),
    }
}

// ── Safe delete file ────────────────────────────────────────────────────────

pub fn query_safe_delete_file(
    file: &str,
    force: bool,
    dry_run: bool,
) -> Result<SafeDeleteFileOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let rel = crate::core::fs::relative_slash(&path, &project_root);
    let res_path = format!("res://{rel}");

    let workspace = super::workspace::WorkspaceIndex::new(project_root.clone());

    let mut references = Vec::new();

    // Find preload()/load() references
    let preloads = super::refactor::find_preloads_to_file(&res_path, &workspace, &project_root);
    for p in preloads {
        references.push(FileReference {
            file: p.file,
            line: p.line,
            kind: "preload".to_string(),
            text: p.path,
        });
    }

    // Find extends "res://..." references
    for (fpath, content) in workspace.all_files() {
        if fpath == path {
            continue;
        }
        if let Ok(tree) = crate::core::parser::parse(&content) {
            let root = tree.root_node();
            let mut cursor = root.walk();
            for child in root.children(&mut cursor) {
                if child.kind() == "extends_statement" {
                    for i in 0..child.named_child_count() {
                        if let Some(str_node) = child.named_child(i)
                            && str_node.kind() == "string"
                            && let Ok(text) = str_node.utf8_text(content.as_bytes())
                        {
                            let unquoted = text.trim_matches('"').trim_matches('\'');
                            if unquoted == res_path {
                                references.push(FileReference {
                                    file: crate::core::fs::relative_slash(&fpath, &project_root),
                                    line: child.start_position().row as u32 + 1,
                                    kind: "extends".to_string(),
                                    text: unquoted.to_string(),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    let deleted = if !dry_run && (force || references.is_empty()) {
        std::fs::remove_file(&path).map_err(|e| miette::miette!("cannot delete {file}: {e}"))?;
        true
    } else {
        false
    };

    Ok(SafeDeleteFileOutput {
        file: rel,
        references,
        deleted,
    })
}

// ── Find implementations ────────────────────────────────────────────────────

pub fn query_find_implementations(name: &str, base: Option<&str>) -> Result<ImplementationsOutput> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette::miette!("cannot get current directory: {e}"))?;
    let project_root = find_root(&cwd)?;
    let workspace = super::workspace::WorkspaceIndex::new(project_root.clone());

    let mut implementations = Vec::new();

    for (fpath, content) in workspace.all_files() {
        if let Ok(tree) = crate::core::parser::parse(&content) {
            let root = tree.root_node();

            // Extract extends info
            let mut extends_value = None;
            let mut class_name_value = None;
            let mut cursor = root.walk();
            for child in root.children(&mut cursor) {
                match child.kind() {
                    "extends_statement" => {
                        for i in 0..child.named_child_count() {
                            if let Some(c) = child.named_child(i)
                                && let Ok(text) = c.utf8_text(content.as_bytes())
                            {
                                let val = text.trim_matches('"').trim_matches('\'');
                                extends_value = Some(val.to_string());
                                break;
                            }
                        }
                    }
                    "class_name_statement" => {
                        if let Some(c) = child.child(1)
                            && let Ok(text) = c.utf8_text(content.as_bytes())
                        {
                            class_name_value = Some(text.to_string());
                        }
                    }
                    _ => {}
                }
            }

            // Filter by base if specified
            if let Some(base_filter) = base {
                match &extends_value {
                    Some(ext) if ext == base_filter => {}
                    _ => continue,
                }
            }

            // Search for matching function definitions
            let mut cursor2 = root.walk();
            for child in root.children(&mut cursor2) {
                let is_match = match child.kind() {
                    "function_definition" => child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(content.as_bytes()).ok())
                        .is_some_and(|n| n == name),
                    "constructor_definition" => name == "_init",
                    _ => false,
                };

                if is_match {
                    implementations.push(ImplementationEntry {
                        file: crate::core::fs::relative_slash(&fpath, &project_root),
                        line: child.start_position().row as u32 + 1,
                        end_line: child.end_position().row as u32 + 1,
                        extends: extends_value.clone(),
                        class_name: class_name_value.clone(),
                    });
                }
            }
        }
    }

    // Sort by file path for deterministic output
    implementations.sort_by(|a, b| a.file.cmp(&b.file));

    Ok(ImplementationsOutput {
        method: name.to_string(),
        implementations,
    })
}

// ── Refactoring queries ──────────────────────────────────────────────────────

pub fn query_delete_symbol(
    file: &str,
    name: Option<&str>,
    line: Option<usize>,
    force: bool,
    dry_run: bool,
    class: Option<&str>,
) -> Result<super::refactor::DeleteSymbolOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::delete_symbol(&path, name, line, force, dry_run, &project_root, class)
}

pub fn query_move_symbol(
    name: &str,
    from: &str,
    to: &str,
    dry_run: bool,
    class: Option<&str>,
    target_class: Option<&str>,
    update_callers: bool,
) -> Result<super::refactor::MoveSymbolOutput> {
    let from_path = resolve_file(from)?;
    let project_root = find_root(&from_path)?;
    let to_path = project_root.join(to);
    let mut result = super::refactor::move_symbol(
        name,
        &from_path,
        &to_path,
        dry_run,
        &project_root,
        class,
        target_class,
    )?;

    // Update callers after successful move
    if update_callers && !dry_run && result.applied && !result.preloads.is_empty() {
        let from_relative = crate::core::fs::relative_slash(&from_path, &project_root);
        let to_relative = crate::core::fs::relative_slash(&to_path, &project_root);
        let source_res = format!("res://{from_relative}");
        let dest_res = format!("res://{to_relative}");

        match super::refactor::update_callers_after_move(
            &source_res,
            &dest_res,
            &result.preloads,
            &project_root,
        ) {
            Ok(updates) => {
                for update in &updates {
                    result.warnings.push(format!(
                        "updated {}: added {}",
                        update.file, update.added_preload
                    ));
                }
            }
            Err(e) => {
                result.warnings.push(format!("caller update error: {e}"));
            }
        }
    }

    Ok(result)
}

pub fn query_extract_method(
    file: &str,
    start_line: usize,
    end_line: usize,
    name: &str,
    dry_run: bool,
) -> Result<super::refactor::ExtractMethodOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::extract_method(&path, start_line, end_line, name, dry_run, &project_root)
}

pub fn query_inline_method(
    file: &str,
    line: usize,
    column: usize,
    dry_run: bool,
) -> Result<super::refactor::InlineMethodOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::inline_method(&path, line, column, dry_run, &project_root)
}

pub fn query_inline_method_by_name(
    file: &str,
    name: &str,
    all: bool,
    dry_run: bool,
) -> Result<super::refactor::InlineMethodByNameOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::inline_method_by_name(&path, name, all, dry_run, &project_root)
}

#[allow(clippy::too_many_arguments)]
pub fn query_change_signature(
    file: &str,
    name: &str,
    add_params: &[String],
    remove_params: &[String],
    rename_params: &[String],
    reorder: Option<&str>,
    class: Option<&str>,
    dry_run: bool,
) -> Result<super::refactor::ChangeSignatureOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::change_signature(
        &path,
        name,
        add_params,
        remove_params,
        rename_params,
        reorder,
        class,
        dry_run,
        &project_root,
    )
}

pub fn query_introduce_variable(
    file: &str,
    line: usize,
    column: usize,
    end_column: usize,
    name: &str,
    dry_run: bool,
) -> Result<super::refactor::IntroduceVariableOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::introduce_variable(
        &path,
        line,
        column,
        end_column,
        name,
        dry_run,
        &project_root,
    )
}

pub fn query_introduce_parameter(
    file: &str,
    line: usize,
    column: usize,
    end_column: usize,
    name: &str,
    type_hint: Option<&str>,
    dry_run: bool,
) -> Result<super::refactor::IntroduceParameterOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::introduce_parameter(
        &path,
        line,
        column,
        end_column,
        name,
        type_hint,
        dry_run,
        &project_root,
    )
}

// ── Bulk operations ──────────────────────────────────────────────────────

pub fn query_bulk_delete_symbol(
    file: &str,
    names_str: &str,
    force: bool,
    dry_run: bool,
) -> Result<super::refactor::BulkDeleteSymbolOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let names: Vec<String> = names_str.split(',').map(|s| s.trim().to_string()).collect();
    super::refactor::bulk_delete_symbol(&path, &names, force, dry_run, &project_root)
}

pub fn query_bulk_rename(
    file: &str,
    renames_str: &str,
    dry_run: bool,
) -> Result<super::refactor::BulkRenameOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let mut renames = Vec::new();
    for pair in renames_str.split(',') {
        let pair = pair.trim();
        let parts: Vec<&str> = pair.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(miette::miette!(
                "invalid rename pair '{pair}': expected 'old:new'"
            ));
        }
        renames.push((parts[0].trim().to_string(), parts[1].trim().to_string()));
    }
    super::refactor::bulk_rename(&path, &renames, dry_run, &project_root)
}

pub fn query_inline_delegate(
    file: &str,
    name: &str,
    dry_run: bool,
) -> Result<super::refactor::InlineDelegateOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::inline_delegate(&path, name, dry_run, &project_root)
}

pub fn query_extract_class(
    file: &str,
    symbols_str: &str,
    to: &str,
    dry_run: bool,
) -> Result<super::refactor::ExtractClassOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let to_path = project_root.join(to);
    let names: Vec<String> = symbols_str
        .split(',')
        .map(|s| s.trim().to_string())
        .collect();
    super::refactor::extract_class(&path, &names, &to_path, dry_run, &project_root)
}

// ── AST-aware edit commands ──────────────────────────────────────────────────

pub fn query_replace_body(
    file: &str,
    name: &str,
    class: Option<&str>,
    content: &str,
    no_format: bool,
    dry_run: bool,
) -> Result<super::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::replace_body(
        &path,
        name,
        class,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

pub fn query_insert(
    file: &str,
    anchor: &str,
    after: bool,
    class: Option<&str>,
    content: &str,
    no_format: bool,
    dry_run: bool,
) -> Result<super::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::insert(
        &path,
        anchor,
        after,
        class,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

pub fn query_replace_symbol(
    file: &str,
    name: &str,
    class: Option<&str>,
    content: &str,
    no_format: bool,
    dry_run: bool,
) -> Result<super::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::replace_symbol(
        &path,
        name,
        class,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

pub fn query_edit_range(
    file: &str,
    start_line: usize,
    end_line: usize,
    content: &str,
    no_format: bool,
    dry_run: bool,
) -> Result<super::refactor::EditOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    super::refactor::edit_range(
        &path,
        start_line,
        end_line,
        content,
        no_format,
        dry_run,
        &project_root,
    )
}

// ── Create file ─────────────────────────────────────────────────────────────

pub fn query_create_file(
    file: &str,
    extends: &str,
    class_name: Option<&str>,
    custom_content: Option<&str>,
    dry_run: bool,
) -> Result<CreateFileOutput> {
    let path = std::path::Path::new(file);

    let full_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| miette::miette!("cannot get current directory: {e}"))?
            .join(path)
    };

    if full_path.exists() {
        return Err(miette::miette!("file already exists: {file}"));
    }

    let content = if let Some(custom) = custom_content {
        custom.to_string()
    } else {
        let mut buf = String::new();
        buf.push_str(&format!("extends {extends}\n"));
        if let Some(cn) = class_name {
            buf.push_str(&format!("class_name {cn}\n"));
        }
        buf.push_str("\n\n");
        buf.push_str("func _ready() -> void:\n\tpass\n\n\n");
        buf.push_str("func _process(delta: float) -> void:\n\tpass\n");
        buf
    };

    let lines = content.lines().count() as u32;

    if !dry_run {
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| miette::miette!("cannot create directories: {e}"))?;
        }
        std::fs::write(&full_path, &content)
            .map_err(|e| miette::miette!("cannot write file: {e}"))?;
    }

    Ok(CreateFileOutput {
        file: file.to_string(),
        extends: extends.to_string(),
        class_name: class_name.map(|s| s.to_string()),
        applied: !dry_run,
        lines,
    })
}

// ── View ────────────────────────────────────────────────────────────────────

pub fn query_view(
    file: &str,
    start_line: Option<usize>,
    end_line: Option<usize>,
    context: Option<usize>,
) -> Result<ViewOutput> {
    let resolved = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&resolved).map_err(|e| miette::miette!("cannot read file: {e}"))?;
    let all_lines: Vec<&str> = source.lines().collect();
    let total = all_lines.len();

    let mut start = start_line.unwrap_or(1).max(1);
    let mut end = end_line.unwrap_or(total).min(total);

    if let Some(ctx) = context {
        start = start.saturating_sub(ctx).max(1);
        end = (end + ctx).min(total);
    }

    let content = all_lines[start - 1..end].join("\n");

    let project_root = find_root(&resolved)?;
    let rel = crate::core::fs::relative_slash(&resolved, &project_root);

    Ok(ViewOutput {
        file: rel,
        start_line: start as u32,
        end_line: end as u32,
        total_lines: total as u32,
        content,
    })
}

// ── Apply rename ────────────────────────────────────────────────────────────

pub fn apply_rename(output: &RenameOutput, project_root: &Path) -> Result<usize> {
    let mut files_changed = 0;
    for file_edits in &output.changes {
        let path = project_root.join(&file_edits.file);
        let mut content = std::fs::read_to_string(&path)
            .map_err(|e| miette::miette!("cannot read {}: {e}", file_edits.file))?;

        // Sort edits in reverse order to preserve byte offsets
        let mut edits: Vec<&TextEditOutput> = file_edits.edits.iter().collect();
        edits.sort_by(|a, b| b.line.cmp(&a.line).then(b.column.cmp(&a.column)));

        for edit in edits {
            let start =
                position_to_byte_offset(&content, Position::new(edit.line - 1, edit.column - 1));
            let end = position_to_byte_offset(
                &content,
                Position::new(edit.end_line - 1, edit.end_column - 1),
            );
            content.replace_range(start..end, &edit.new_text);
        }

        std::fs::write(&path, &content)
            .map_err(|e| miette::miette!("cannot write {}: {e}", file_edits.file))?;
        files_changed += 1;
    }
    Ok(files_changed)
}

/// Convert a `WorkspaceEdit` from `rename_cross_file` into a `RenameOutput`.
/// Used by `bulk_rename` to avoid going through `resolve_file` / `find_root`.
pub fn convert_rename_edit(
    edit: &WorkspaceEdit,
    project_root: &Path,
    old_name: &str,
    new_name: &str,
) -> RenameOutput {
    let changes = convert_workspace_edit(edit, project_root);
    RenameOutput {
        symbol: old_name.to_string(),
        new_name: new_name.to_string(),
        changes,
        summary: None,
    }
}

// ── Internal converters ─────────────────────────────────────────────────────

fn convert_workspace_edit(edit: &WorkspaceEdit, base: &Path) -> Vec<FileEdits> {
    let Some(changes) = &edit.changes else {
        return vec![];
    };

    changes
        .iter()
        .map(|(url, edits)| {
            let file = url_to_relative(url, base);
            let edits = edits
                .iter()
                .map(|e| TextEditOutput {
                    line: e.range.start.line + 1,
                    column: e.range.start.character + 1,
                    end_line: e.range.end.line + 1,
                    end_column: e.range.end.character + 1,
                    new_text: e.new_text.clone(),
                })
                .collect();
            FileEdits { file, edits }
        })
        .collect()
}
