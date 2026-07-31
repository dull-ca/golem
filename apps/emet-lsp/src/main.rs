// The LSP server binary: a stdio JSON-RPC loop that owns the open-document
// store and routes requests to the feature functions in `emet_lsp`. Full-sync
// text: each change replaces the whole document. All language understanding is
// in `emet` behind those functions; this file is transport and dispatch only.

use std::collections::HashMap;
use std::error::Error;

use emet_lsp::{completion_at, definition_at, diagnostics_for, hover_at};
use lsp_server::{Connection, Message, Notification, Request, Response};
use lsp_types::notification::{
    DidChangeTextDocument, DidOpenTextDocument, Notification as _, PublishDiagnostics,
};
use lsp_types::request::{Completion, GotoDefinition, HoverRequest, Request as _};
use lsp_types::{
    CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
    DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, HoverParams, OneOf,
    PublishDiagnosticsParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    Uri,
};

/// The latest full text of every open document, keyed by URI. Full-sync
/// (`TextDocumentSyncKind::FULL`) means each edit notification carries the
/// complete new text, which simply overwrites the stored copy — no incremental
/// patching. Every request re-reads from here and re-analyzes.
#[derive(Default)]
struct Documents(HashMap<String, String>);

impl Documents {
    fn set(&mut self, uri: &Uri, text: String) {
        self.0.insert(uri.as_str().to_string(), text);
    }
    fn get(&self, uri: &Uri) -> Option<&String> {
        self.0.get(uri.as_str())
    }
}

fn main() -> Result<(), Box<dyn Error + Sync + Send>> {
    let (connection, io_threads) = Connection::stdio();

    let capabilities = ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        hover_provider: Some(true.into()),
        completion_provider: Some(CompletionOptions::default()),
        definition_provider: Some(OneOf::Left(true)),
        ..Default::default()
    };
    connection.initialize(serde_json::to_value(capabilities)?)?;

    serve(connection)?;

    io_threads.join()?;
    Ok(())
}

fn serve(connection: Connection) -> Result<(), Box<dyn Error + Sync + Send>> {
    let mut documents = Documents::default();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    break;
                }
                let response = handle_request(&documents, request);
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(notification) => {
                handle_notification(&connection, &mut documents, notification)?;
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// Dispatch one request to its feature function by LSP method: hover,
/// completion, or go-to-definition. Each looks up the document's current text
/// and delegates to `emet_lsp`; an unrecognized method returns a null result.
fn handle_request(documents: &Documents, request: Request) -> Response {
    match request.method.as_str() {
        HoverRequest::METHOD => {
            let params: HoverParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(error) => return error_response(request.id, error),
            };
            let position = params.text_document_position_params;
            let result = documents
                .get(&position.text_document.uri)
                .and_then(|source| {
                    hover_at(&position.text_document.uri, source, position.position)
                });
            ok_response(
                request.id,
                serde_json::to_value(result).unwrap_or(serde_json::Value::Null),
            )
        }
        Completion::METHOD => {
            let params: CompletionParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(error) => return error_response(request.id, error),
            };
            let position = params.text_document_position;
            let items = documents
                .get(&position.text_document.uri)
                .map(|source| completion_at(&position.text_document.uri, source, position.position))
                .unwrap_or_default();
            ok_response(
                request.id,
                serde_json::to_value(CompletionResponse::Array(items))
                    .unwrap_or(serde_json::Value::Null),
            )
        }
        GotoDefinition::METHOD => {
            let params: GotoDefinitionParams = match serde_json::from_value(request.params) {
                Ok(params) => params,
                Err(error) => return error_response(request.id, error),
            };
            let position = params.text_document_position_params;
            let location = documents
                .get(&position.text_document.uri)
                .and_then(|source| {
                    definition_at(&position.text_document.uri, source, position.position)
                });
            ok_response(
                request.id,
                serde_json::to_value(location.map(GotoDefinitionResponse::Scalar))
                    .unwrap_or(serde_json::Value::Null),
            )
        }
        _ => ok_response(request.id, serde_json::Value::Null),
    }
}

fn ok_response(id: lsp_server::RequestId, result: serde_json::Value) -> Response {
    Response::new_ok(id, result)
}

fn error_response(id: lsp_server::RequestId, error: serde_json::Error) -> Response {
    Response::new_err(
        id,
        lsp_server::ErrorCode::InvalidParams as i32,
        error.to_string(),
    )
}

fn handle_notification(
    connection: &Connection,
    documents: &mut Documents,
    notification: Notification,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let params: DidOpenTextDocumentParams = serde_json::from_value(notification.params)?;
            documents.set(&params.text_document.uri, params.text_document.text.clone());
            publish(
                connection,
                params.text_document.uri,
                &params.text_document.text,
            )?;
        }
        DidChangeTextDocument::METHOD => {
            let params: DidChangeTextDocumentParams = serde_json::from_value(notification.params)?;
            if let Some(change) = params.content_changes.into_iter().last() {
                documents.set(&params.text_document.uri, change.text.clone());
                publish(connection, params.text_document.uri, &change.text)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn publish(
    connection: &Connection,
    uri: Uri,
    source: &str,
) -> Result<(), Box<dyn Error + Sync + Send>> {
    let diagnostics = diagnostics_for(&uri, source);
    let params = PublishDiagnosticsParams {
        uri,
        diagnostics,
        version: None,
    };
    let notification = Notification {
        method: PublishDiagnostics::METHOD.to_owned(),
        params: serde_json::to_value(params)?,
    };
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}
