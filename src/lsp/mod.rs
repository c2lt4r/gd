mod actions;
mod builtins;
mod completion;
pub mod daemon;
pub mod daemon_client;
mod definition;
mod diagnostics;
mod formatting;
pub mod godot_client;
mod hover;
pub mod query;
pub mod refactor;
mod references;
pub(crate) mod rename;
mod symbols;
mod util;
mod workspace;

use dashmap::DashMap;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

/// In-memory state for a single open document.
struct DocumentState {
    content: String,
}

/// The LSP server backend.
struct Backend {
    client: Client,
    documents: DashMap<Url, DocumentState>,
    workspace: std::sync::OnceLock<workspace::WorkspaceIndex>,
    /// Whether daemon-backed Godot proxy is enabled (non-zero port).
    use_godot_proxy: bool,
}

impl Backend {
    /// Parse a document and publish diagnostics to the client.
    async fn publish_diagnostics(&self, uri: Url) {
        let Some(doc) = self.documents.get(&uri) else {
            return;
        };
        let source = doc.content.clone();
        drop(doc); // release lock before expensive work

        let diags = diagnostics::lint_source(&source, &uri);

        self.client.publish_diagnostics(uri, diags, None).await;
    }

    /// Lint all workspace files and publish diagnostics for the entire project.
    async fn lint_workspace(&self) {
        let Some(ws) = self.workspace.get() else {
            return;
        };

        // Load config once for the whole workspace instead of per-file
        let config = crate::core::config::Config::load(ws.project_root()).unwrap_or_default();
        let ignore_base = ws.project_root().to_path_buf();

        for (path, content) in ws.all_files() {
            // Skip files matching ignore_patterns before any linting
            if crate::lint::matches_ignore_pattern(
                &path,
                &ignore_base,
                &config.lint.ignore_patterns,
            ) {
                continue;
            }

            let Ok(uri) = Url::from_file_path(&path) else {
                continue;
            };
            // Skip files already open — they have fresh diagnostics from did_open
            if self.documents.contains_key(&uri) {
                continue;
            }
            let diags = diagnostics::lint_source(&content, &uri);
            self.client.publish_diagnostics(uri, diags, None).await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(root) = workspace::discover_root(&params) {
            let _ = self.workspace.set(workspace::WorkspaceIndex::new(root));
        }

        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_formatting_provider: Some(OneOf::Left(true)),
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![".".to_string()]),
                    ..Default::default()
                }),
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "gd-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "gd language server initialized")
            .await;

        // Warm up the daemon (auto-spawns if not running) so first hover is fast
        if self.use_godot_proxy {
            let connected = tokio::task::spawn_blocking(|| {
                daemon_client::query_daemon(
                    "hover",
                    serde_json::json!({"file": "__warmup__", "line": 1, "column": 1}),
                    None,
                )
                .is_some()
            })
            .await
            .unwrap_or(false);

            if connected {
                self.client
                    .log_message(MessageType::INFO, "Daemon connected (Godot proxy available)")
                    .await;
            }
        }

        self.lint_workspace().await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        self.documents.insert(
            params.text_document.uri,
            DocumentState {
                content: params.text_document.text,
            },
        );
        self.publish_diagnostics(uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri.clone();
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents.insert(
                params.text_document.uri,
                DocumentState {
                    content: change.text,
                },
            );
        }
        self.publish_diagnostics(uri).await;
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        if let Some(ws) = self.workspace.get()
            && let Ok(path) = uri.to_file_path()
        {
            ws.refresh_file(&path);
        }
        self.publish_diagnostics(uri).await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        // Re-lint from disk content so Problems panel reflects saved state
        if let Some(ws) = self.workspace.get()
            && let Ok(path) = uri.to_file_path()
            && let Some(content) = ws.get_content(&path)
        {
            let diags = diagnostics::lint_source(&content, &uri);
            self.client.publish_diagnostics(uri, diags, None).await;
            return;
        }
        // Fallback: file not in workspace index, clear diagnostics
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = &params.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        Ok(formatting::format_document(&source, &params.options))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = &params.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        Ok(actions::provide_code_actions(
            &params.text_document.uri,
            &source,
            &params.range,
        ))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = &params.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        Ok(symbols::document_symbols(&source))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        // Godot-first via daemon: if daemon returns hover data, use it exclusively
        if self.use_godot_proxy
            && let Some(path) = uri.to_file_path().ok()
            && let Some(file_str) = path.to_str()
        {
            let pos = params.text_document_position_params.position;
            let file = file_str.to_string();
            let line = pos.line as usize + 1; // LSP 0-based → query 1-based
            let col = pos.character as usize + 1;
            if let Some(result) = tokio::task::spawn_blocking(move || {
                daemon_client::query_daemon(
                    "hover",
                    serde_json::json!({"file": file, "line": line, "column": col}),
                    None,
                )
            })
            .await
            .ok()
            .flatten()
                && let Some(content) = result.get("content").and_then(|c| c.as_str())
            {
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: content.to_string(),
                    }),
                    range: None,
                }));
            }
        }

        // Fallback: static tree-sitter analysis
        Ok(hover::hover_at(
            &source,
            params.text_document_position_params.position,
        ))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = &params.text_document_position_params.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        // Godot-first via daemon
        if self.use_godot_proxy
            && let Some(path) = uri.to_file_path().ok()
            && let Some(file_str) = path.to_str()
        {
            let pos = params.text_document_position_params.position;
            let file = file_str.to_string();
            let line = pos.line as usize + 1;
            let col = pos.character as usize + 1;
            if let Some(result) = tokio::task::spawn_blocking(move || {
                daemon_client::query_daemon(
                    "definition",
                    serde_json::json!({"file": file, "line": line, "column": col}),
                    None,
                )
            })
            .await
            .ok()
            .flatten()
                && let Some(def_file) = result.get("file").and_then(|f| f.as_str())
                && let Some(def_line) = result.get("line").and_then(|l| l.as_u64())
                && let Some(def_col) = result.get("column").and_then(|c| c.as_u64())
            {
                // Convert back to LSP 0-based positions
                let def_line = def_line.saturating_sub(1) as u32;
                let def_col = def_col.saturating_sub(1) as u32;
                // Resolve the file path to a URI
                let def_path = if std::path::Path::new(def_file).is_absolute() {
                    std::path::PathBuf::from(def_file)
                } else if let Some(ws) = self.workspace.get() {
                    ws.project_root().join(def_file)
                } else {
                    std::path::PathBuf::from(def_file)
                };
                if let Ok(def_uri) = Url::from_file_path(&def_path) {
                    let end_col = result
                        .get("end_column")
                        .and_then(|c| c.as_u64())
                        .map(|c| c.saturating_sub(1) as u32)
                        .unwrap_or(def_col);
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: def_uri,
                        range: Range {
                            start: Position {
                                line: def_line,
                                character: def_col,
                            },
                            end: Position {
                                line: def_line,
                                character: end_col,
                            },
                        },
                    })));
                }
            }
        }

        // Fallback: static tree-sitter analysis
        let result = if let Some(ws) = self.workspace.get() {
            definition::goto_definition_cross_file(
                &source,
                uri,
                params.text_document_position_params.position,
                ws,
            )
        } else {
            definition::goto_definition(&source, uri, params.text_document_position_params.position)
        };

        Ok(result)
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = &params.text_document_position.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        if let Some(ws) = self.workspace.get() {
            Ok(references::find_references_cross_file(
                &source,
                uri,
                params.text_document_position.position,
                params.context.include_declaration,
                ws,
            ))
        } else {
            Ok(references::find_references(
                &source,
                uri,
                params.text_document_position.position,
                params.context.include_declaration,
            ))
        }
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = &params.text_document_position.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        if let Some(ws) = self.workspace.get() {
            Ok(rename::rename_cross_file(
                &source,
                uri,
                params.text_document_position.position,
                &params.new_name,
                ws,
            ))
        } else {
            Ok(rename::rename_symbol(
                &source,
                uri,
                params.text_document_position.position,
                &params.new_name,
            ))
        }
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = &params.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        Ok(rename::prepare_rename(&source, params.position))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = &params.text_document_position.text_document.uri;
        let Some(doc) = self.documents.get(uri) else {
            return Ok(None);
        };
        let source = doc.content.clone();
        drop(doc);

        // Godot-first via daemon
        if self.use_godot_proxy
            && let Some(path) = uri.to_file_path().ok()
            && let Some(file_str) = path.to_str()
        {
            let pos = params.text_document_position.position;
            let file = file_str.to_string();
            let line = pos.line as usize + 1;
            let col = pos.character as usize + 1;
            if let Some(result) = tokio::task::spawn_blocking(move || {
                daemon_client::query_daemon(
                    "completion",
                    serde_json::json!({"file": file, "line": line, "column": col}),
                    None,
                )
            })
            .await
            .ok()
            .flatten()
                && let Some(items) = result.as_array()
                && !items.is_empty()
            {
                let completion_items: Vec<CompletionItem> = items
                    .iter()
                    .filter_map(|item| {
                        let label = item.get("label")?.as_str()?.to_string();
                        let kind_str = item.get("kind").and_then(|k| k.as_str());
                        let kind = kind_str.map(|s| match s {
                            "keyword" => CompletionItemKind::KEYWORD,
                            "function" | "method" => CompletionItemKind::FUNCTION,
                            "variable" => CompletionItemKind::VARIABLE,
                            "property" => CompletionItemKind::PROPERTY,
                            "class" => CompletionItemKind::CLASS,
                            "constant" => CompletionItemKind::CONSTANT,
                            "signal" => CompletionItemKind::EVENT,
                            "enum" => CompletionItemKind::ENUM,
                            _ => CompletionItemKind::TEXT,
                        });
                        let detail = item
                            .get("detail")
                            .and_then(|d| d.as_str())
                            .map(|s| s.to_string());
                        Some(CompletionItem {
                            label,
                            kind,
                            detail,
                            ..Default::default()
                        })
                    })
                    .collect();
                if !completion_items.is_empty() {
                    return Ok(Some(CompletionResponse::Array(completion_items)));
                }
            }
        }

        // Fallback: static tree-sitter analysis
        let items = completion::provide_completions(
            &source,
            params.text_document_position.position,
            self.workspace.get(),
        );

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }
}

/// Start the LSP server with configurable Godot proxy port.
/// Pass 0 to disable the proxy.
pub fn run_server_with_options(godot_port: u16) {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to create tokio runtime")
        .block_on(async {
            let stdin = tokio::io::stdin();
            let stdout = tokio::io::stdout();

            let (service, socket) = LspService::new(|client| Backend {
                client,
                documents: DashMap::new(),
                workspace: std::sync::OnceLock::new(),
                use_godot_proxy: godot_port != 0,
            });

            Server::new(stdin, stdout, socket).serve(service).await;
        });
}

