mod actions;
mod builtins;
mod completion;
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
    godot_proxy: std::sync::OnceLock<Option<godot_client::GodotClient>>,
    godot_port: u16,
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

    /// Get the Godot proxy client (lazily connects on first use).
    fn godot_proxy(&self) -> Option<&godot_client::GodotClient> {
        self.godot_proxy
            .get_or_init(|| {
                if self.godot_port == 0 {
                    return None;
                }
                let client = godot_client::GodotClient::connect("127.0.0.1", self.godot_port)?;
                client.initialize()?;
                Some(client)
            })
            .as_ref()
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

        let our_hover = hover::hover_at(&source, params.text_document_position_params.position);

        // Try Godot proxy for additional hover info
        if let Some(proxy) = self.godot_proxy() {
            let pos = params.text_document_position_params.position;
            if let Some(godot_val) = proxy.hover(uri.as_str(), pos.line, pos.character)
                && let Some(godot_hover) = parse_godot_hover(&godot_val)
            {
                return Ok(Some(merge_hovers(our_hover, godot_hover)));
            }
        }

        Ok(our_hover)
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

        let our_result = if let Some(ws) = self.workspace.get() {
            definition::goto_definition_cross_file(
                &source,
                uri,
                params.text_document_position_params.position,
                ws,
            )
        } else {
            definition::goto_definition(&source, uri, params.text_document_position_params.position)
        };

        // If we didn't find a definition, try Godot proxy
        if our_result.is_none()
            && let Some(proxy) = self.godot_proxy()
        {
            let pos = params.text_document_position_params.position;
            if let Some(godot_val) = proxy.definition(uri.as_str(), pos.line, pos.character)
                && let Some(def) = parse_godot_definition(&godot_val)
            {
                return Ok(Some(def));
            }
        }

        Ok(our_result)
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

        let mut items = completion::provide_completions(
            &source,
            params.text_document_position.position,
            self.workspace.get(),
        );

        // Try Godot proxy for additional completions
        if let Some(proxy) = self.godot_proxy() {
            let pos = params.text_document_position.position;
            if let Some(godot_val) = proxy.completion(uri.as_str(), pos.line, pos.character)
                && let Some(godot_items) = parse_godot_completions(&godot_val)
            {
                items.extend(godot_items);
            }
        }

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
                godot_proxy: std::sync::OnceLock::new(),
                godot_port,
            });

            Server::new(stdin, stdout, socket).serve(service).await;
        });
}

// ---------------------------------------------------------------------------
// Godot proxy response helpers
// ---------------------------------------------------------------------------

/// Parse Godot's hover response into an LSP Hover.
fn parse_godot_hover(val: &serde_json::Value) -> Option<Hover> {
    let contents = val.get("contents")?;
    let text = if let Some(s) = contents.as_str() {
        s.to_string()
    } else if let Some(obj) = contents.as_object() {
        obj.get("value")?.as_str()?.to_string()
    } else {
        return None;
    };

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: text,
        }),
        range: None,
    })
}

/// Merge our hover with Godot's hover.
fn merge_hovers(ours: Option<Hover>, godot: Hover) -> Hover {
    let Some(our_hover) = ours else {
        return godot;
    };

    let our_text = match &our_hover.contents {
        HoverContents::Markup(m) => m.value.clone(),
        _ => String::new(),
    };

    let godot_text = match &godot.contents {
        HoverContents::Markup(m) => m.value.clone(),
        _ => String::new(),
    };

    if our_text.is_empty() {
        return godot;
    }
    if godot_text.is_empty() {
        return our_hover;
    }

    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("{our_text}\n\n---\n\n{godot_text}"),
        }),
        range: our_hover.range.or(godot.range),
    }
}

/// Parse Godot's completion response into CompletionItems.
fn parse_godot_completions(val: &serde_json::Value) -> Option<Vec<CompletionItem>> {
    let items = val
        .as_array()
        .or_else(|| val.get("items").and_then(|i| i.as_array()))?;

    let mut result = Vec::new();
    for item in items {
        let label = item.get("label")?.as_str()?.to_string();
        let kind = item
            .get("kind")
            .and_then(|k| k.as_u64())
            .and_then(|k| serde_json::from_value(serde_json::Value::Number(k.into())).ok());
        let detail = item
            .get("detail")
            .and_then(|d| d.as_str())
            .map(|s| s.to_string());

        result.push(CompletionItem {
            label,
            kind,
            detail,
            // Mark as coming from Godot engine
            label_details: Some(CompletionItemLabelDetails {
                detail: Some(" (Godot)".to_string()),
                description: None,
            }),
            ..Default::default()
        });
    }

    if result.is_empty() {
        None
    } else {
        Some(result)
    }
}

/// Parse Godot's definition response into a GotoDefinitionResponse.
fn parse_godot_definition(val: &serde_json::Value) -> Option<GotoDefinitionResponse> {
    // Godot may return a single Location or an array
    if let Some(uri_str) = val.get("uri").and_then(|u| u.as_str()) {
        let uri = Url::parse(uri_str).ok()?;
        let range = parse_godot_range(val.get("range")?)?;
        return Some(GotoDefinitionResponse::Scalar(Location { uri, range }));
    }

    if let Some(arr) = val.as_array() {
        let mut locations = Vec::new();
        for item in arr {
            let uri = Url::parse(item.get("uri")?.as_str()?).ok()?;
            let range = parse_godot_range(item.get("range")?)?;
            locations.push(Location { uri, range });
        }
        if !locations.is_empty() {
            return Some(GotoDefinitionResponse::Array(locations));
        }
    }

    None
}

fn parse_godot_range(val: &serde_json::Value) -> Option<Range> {
    let start = val.get("start")?;
    let end = val.get("end")?;
    Some(Range {
        start: Position {
            line: start.get("line")?.as_u64()? as u32,
            character: start.get("character")?.as_u64()? as u32,
        },
        end: Position {
            line: end.get("line")?.as_u64()? as u32,
            character: end.get("character")?.as_u64()? as u32,
        },
    })
}
