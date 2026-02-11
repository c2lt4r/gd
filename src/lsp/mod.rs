mod actions;
mod definition;
mod diagnostics;
mod formatting;
mod hover;
mod references;
mod rename;
mod symbols;
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
}

impl Backend {
    /// Parse a document and publish diagnostics to the client.
    async fn publish_diagnostics(&self, uri: Url) {
        let Some(doc) = self.documents.get(&uri) else {
            return;
        };
        let source = doc.content.clone();
        drop(doc); // release lock before expensive work

        let diags = diagnostics::lint_source(&source);

        self.client
            .publish_diagnostics(uri, diags, None)
            .await;
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
        let uri = params.text_document.uri.clone();
        self.documents.remove(&params.text_document.uri);
        // Clear diagnostics for closed file
        self.client
            .publish_diagnostics(uri, vec![], None)
            .await;
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

        Ok(hover::hover_at(&source, params.text_document_position_params.position))
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

        if let Some(ws) = self.workspace.get() {
            Ok(definition::goto_definition_cross_file(
                &source,
                uri,
                params.text_document_position_params.position,
                ws,
            ))
        } else {
            Ok(definition::goto_definition(
                &source,
                uri,
                params.text_document_position_params.position,
            ))
        }
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
}

/// Start the LSP server on stdin/stdout.
pub fn run_server() {
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
            });

            Server::new(stdin, stdout, socket).serve(service).await;
        });
}
