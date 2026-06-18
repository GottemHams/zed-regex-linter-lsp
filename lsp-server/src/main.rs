#![allow(clippy::needless_return)]
#![allow(clippy::unused_unit)]

mod linter;

use linter::Linter;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::io::{stdin, stdout};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{LanguageServer, LspService, Server};
use uuid::Uuid;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const LANGUAGE_SERVER_ID: &str = "regex-linter";

#[derive(Clone)]
struct Document {
	language_id: String,
	text: String,
	has_unsaved_changes: bool,
}

struct RegexLinterServer {
	linters: Arc<RwLock<HashMap<String, Linter>>>,
	documents: Arc<RwLock<HashMap<Url, Document>>>,
}

impl RegexLinterServer {
	fn new() -> Self {
		// We can just pass a null value to get the default configs =]]
		let linters = linter::parse_config(&serde_json::Value::Null);
		print_linter_info(&linters);
		return Self {
			linters: Arc::new(RwLock::new(linters)),
			documents: Arc::new(RwLock::new(HashMap::new())),
		};
	}
}

fn print_linter_info(linters: &HashMap<String, Linter>) -> () {
	if linters.is_empty() {
		eprintln!("[WARN] No linters were loaded");
	}
	else {
		eprintln!("Loaded linters:");
		for (source, linter) in linters {//.iter() {
			eprintln!("- {}: {:?}", source, linter);
		}
	}
}

#[tower_lsp::async_trait]
impl LanguageServer for RegexLinterServer {
	async fn initialize(&self, _params: InitializeParams) -> LspResult<InitializeResult> {
		return Ok(InitializeResult {
			server_info: Some(ServerInfo {
				name: "Regex Linter".to_string(),
				version: Some(VERSION.to_string()),
			}),
			capabilities: ServerCapabilities {
				text_document_sync: Some(TextDocumentSyncCapability::Options(TextDocumentSyncOptions {
					open_close: Some(true),
					change: Some(TextDocumentSyncKind::FULL),
					save: Some(TextDocumentSyncSaveOptions::Supported(true)),
					..Default::default()
				})),
				diagnostic_provider: Some(DiagnosticServerCapabilities::Options(
					DiagnosticOptions {
						identifier: Some(LANGUAGE_SERVER_ID.to_string()),
						inter_file_dependencies: false,
						workspace_diagnostics: false,
						..Default::default()
					},
				)),
				..Default::default()
			},
		});
	}

	async fn initialized(&self, _params: InitializedParams) -> () {
		eprintln!("Initialised {} {}", PACKAGE_NAME, VERSION);
	}

	async fn shutdown(&self) -> LspResult<()> {
		// Prolly not really necessary but let's bnice =]
		if let Ok(mut linters) = self.linters.write() {
			linters.clear();
		}

		if let Ok(mut docs) = self.documents.write() {
			docs.clear();
		}

		eprintln!("Shutdown complete");
		return Ok(());
	}

	async fn did_change_configuration(&self, params: DidChangeConfigurationParams) -> () {
		eprintln!("Configuration changed, reloading linters...");
		let parsed_settings = params.settings.as_object().and_then(|s| s.get("regex-linter"));
		if let Some(parsed_settings) = parsed_settings && let Ok(mut linters) = self.linters.write() {
			*linters = linter::parse_config(parsed_settings);
			print_linter_info(&linters);
		}
	}

	async fn did_open(&self, params: DidOpenTextDocumentParams) -> () {
		if let Ok(mut docs) = self.documents.write() {
			docs.insert(params.text_document.uri, Document {
				language_id: params.text_document.language_id,
				text: params.text_document.text,
				has_unsaved_changes: false,
			});
		}
	}

	async fn did_change(&self, params: DidChangeTextDocumentParams) -> () {
		let uri = params.text_document.uri;
		if let Some(change) = params.content_changes.into_iter().next() {
			if let Ok(mut docs) = self.documents.write() && let Some(doc) = docs.get_mut(&uri) {
				// The change contains the **full** text
				doc.text = change.text;
				doc.has_unsaved_changes = true;
			}
		}
	}

	async fn did_save(&self, params: DidSaveTextDocumentParams) -> () {
		let uri = params.text_document.uri;
		if let Ok(mut docs) = self.documents.write() && let Some(doc) = docs.get_mut(&uri) {
			doc.has_unsaved_changes = false;
		}
	}

	async fn did_close(&self, params: DidCloseTextDocumentParams) -> () {
		let uri = params.text_document.uri;
		if let Ok(mut docs) = self.documents.write() {
			docs.remove(&uri);
		}
	}

	async fn diagnostic(&self, params: DocumentDiagnosticParams) -> LspResult<DocumentDiagnosticReportResult> {
		let uri = params.text_document.uri;
		let results = if let Some(doc) = self.documents.read().ok().and_then(|docs| docs.get(&uri).cloned()) {
			if let Ok(linters) = self.linters.read() {
				linter::scan(&doc, &linters)
			}
			else {
				vec![]
			}
		}
		else {
			vec![]
		};

		// We still need **some** ID even though we don't send `Unchanged` reports, otherwise the LSP client tends to get confused
		let result_id = Uuid::new_v4().to_string();
		return Ok(DocumentDiagnosticReportResult::Report(
			DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
				full_document_diagnostic_report: FullDocumentDiagnosticReport {
					result_id: Some(result_id),
					items: results,
				},
				related_documents: None,
			}),
		));
	}
}

#[tokio::main]
async fn main() -> () {
	let (service, socket) = LspService::new(|_client| RegexLinterServer::new());
	Server::new(stdin(), stdout(), socket).serve(service).await;
}
