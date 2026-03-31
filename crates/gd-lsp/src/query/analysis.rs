use miette::Result;
use tower_lsp::lsp_types::{CodeActionOrCommand, DocumentSymbolResponse, SymbolKind};

use gd_core::gd_ast::{self, GdDecl, GdExtends};

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

    let project_root = gd_core::config::find_project_root(&path)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Range covering the specified line
    let lsp_line = (line - 1) as u32;
    let range = Range::new(
        Position::new(lsp_line, (column - 1) as u32),
        Position::new(lsp_line, u32::MAX),
    );

    let actions = crate::actions::provide_code_actions(&uri, &source, &range).unwrap_or_default();

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

pub fn query_symbols(file: &str) -> Result<Vec<SymbolOutput>> {
    let path = resolve_file(file)?;
    let source =
        std::fs::read_to_string(&path).map_err(|e| miette::miette!("cannot read file: {e}"))?;

    let response = crate::symbols::document_symbols(&source)
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
    let rel = gd_core::fs::relative_slash(&path, &project_root);
    let res_path = format!("res://{rel}");

    let workspace = crate::workspace::WorkspaceIndex::new(project_root.clone());

    let mut references = Vec::new();

    // Find preload()/load() references
    for (fpath, content) in workspace.all_files() {
        if let Ok(tree) = gd_core::parser::parse(&content) {
            find_preloads_in_tree(
                tree.root_node(),
                &content,
                &res_path,
                &gd_core::fs::relative_slash(&fpath, &project_root),
                &mut references,
            );
        }
    }

    // Find extends "res://..." references
    for (fpath, content) in workspace.all_files() {
        if fpath == path {
            continue;
        }
        if let Ok(tree) = gd_core::parser::parse(&content) {
            let file = gd_ast::convert(&tree, &content);
            if let Some(GdExtends::Path(ext_path)) = file.extends
                && ext_path == res_path
            {
                let ext_line = file
                    .extends_node
                    .map_or(0, |n| n.start_position().row as u32);
                references.push(FileReference {
                    file: gd_core::fs::relative_slash(&fpath, &project_root),
                    line: ext_line + 1,
                    kind: "extends".to_string(),
                    text: ext_path.to_string(),
                });
            }
        }
    }

    // Only delete when --force is explicitly passed (never auto-delete)
    let deleted = if force && !dry_run {
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

// ── Preload/load reference scanning ─────────────────────────────────────────

fn find_preloads_in_tree(
    node: tree_sitter::Node,
    source: &str,
    target_path: &str,
    file: &str,
    refs: &mut Vec<FileReference>,
) {
    if node.kind() == "call" {
        let func_name = node
            .child_by_field_name("function")
            .or_else(|| node.named_child(0));
        if let Some(func) = func_name
            && let Ok(name) = func.utf8_text(source.as_bytes())
            && (name == "preload" || name == "load")
            && let Some(args) = node.child_by_field_name("arguments")
        {
            let mut arg_cursor = args.walk();
            for arg in args.children(&mut arg_cursor) {
                if arg.kind() == "string"
                    && let Ok(text) = arg.utf8_text(source.as_bytes())
                {
                    let unquoted = text.trim_matches('"').trim_matches('\'');
                    if unquoted == target_path {
                        refs.push(FileReference {
                            file: file.to_string(),
                            line: node.start_position().row as u32 + 1,
                            kind: "preload".to_string(),
                            text: unquoted.to_string(),
                        });
                    }
                }
            }
        }
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        find_preloads_in_tree(child, source, target_path, file, refs);
    }
}

// ── Find implementations ─────────────────────────────────────────────────────

pub fn query_find_implementations(name: &str, base: Option<&str>) -> Result<ImplementationsOutput> {
    let cwd = std::env::current_dir()
        .map_err(|e| miette::miette!("cannot get current directory: {e}"))?;
    let project_root = find_root(&cwd)?;
    let workspace = crate::workspace::WorkspaceIndex::new(project_root.clone());

    let mut implementations = Vec::new();

    for (fpath, content) in workspace.all_files() {
        if let Ok(tree) = gd_core::parser::parse(&content) {
            let file = gd_ast::convert(&tree, &content);

            // Extract extends info
            let extends_value = match file.extends {
                Some(GdExtends::Class(cls)) => Some(cls.to_string()),
                Some(GdExtends::Path(p)) => Some(p.to_string()),
                None => None,
            };
            let class_name_value = file.class_name.map(String::from);

            // Filter by base if specified
            if let Some(base_filter) = base {
                match &extends_value {
                    Some(ext) if ext == base_filter => {}
                    _ => continue,
                }
            }

            // Search for matching function definitions
            for decl in &file.declarations {
                if let GdDecl::Func(f) = decl
                    && f.name == name
                {
                    implementations.push(ImplementationEntry {
                        file: gd_core::fs::relative_slash(&fpath, &project_root),
                        line: f.node.start_position().row as u32 + 1,
                        end_line: f.node.end_position().row as u32 + 1,
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
