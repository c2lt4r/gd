use miette::Result;
use tower_lsp::lsp_types::{CodeActionOrCommand, DocumentSymbolResponse, SymbolKind};

use super::{
    CodeActionOutput, FileEditEntry, FileReference, ImplementationEntry, ImplementationsOutput,
    SafeDeleteFileOutput, SymbolOutput, find_root, resolve_file, url_to_relative,
};

// ── Private helpers ──────────────────────────────────────────────────────────

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
fn flatten_symbols(symbols: &[tower_lsp::lsp_types::DocumentSymbol]) -> Vec<SymbolOutput> {
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

// ── Query functions ──────────────────────────────────────────────────────────

pub fn query_code_actions(file: &str, line: usize, column: usize) -> Result<Vec<CodeActionOutput>> {
    use super::make_uri;
    use tower_lsp::lsp_types::{Position, Range};

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

    let actions =
        crate::lsp::actions::provide_code_actions(&uri, &source, &range).unwrap_or_default();

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
            CodeActionOrCommand::Command(_) => None,
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

    let response = crate::lsp::symbols::document_symbols(&source)
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

// ── Safe delete file ─────────────────────────────────────────────────────────

pub fn query_safe_delete_file(
    file: &str,
    force: bool,
    dry_run: bool,
) -> Result<SafeDeleteFileOutput> {
    let path = resolve_file(file)?;
    let project_root = find_root(&path)?;
    let rel = crate::core::fs::relative_slash(&path, &project_root);
    let res_path = format!("res://{rel}");

    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.clone());

    let mut references = Vec::new();

    // Find preload()/load() references
    let preloads =
        crate::lsp::refactor::find_preloads_to_file(&res_path, &workspace, &project_root);
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

// ── Find implementations ─────────────────────────────────────────────────────

pub fn query_find_implementations(name: &str, base: Option<&str>) -> Result<ImplementationsOutput> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette::miette!("cannot get current directory: {e}"))?;
    let project_root = find_root(&cwd)?;
    let workspace = crate::lsp::workspace::WorkspaceIndex::new(project_root.clone());

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
