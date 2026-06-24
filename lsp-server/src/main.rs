#![allow(clippy::needless_return)]
#![allow(clippy::unused_unit)]

mod linter;

use linter::Linter;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::io::{stdin, stdout};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const LANGUAGE_SERVER_ID: &str = "regex-linter";

#[derive(Clone)]
struct Document {
	language_id: String,
	text: String,
	version: i32,
	has_unsaved_changes: bool,
}

struct RegexLinterServer {
	client: Client,
	linters: Arc<RwLock<HashMap<String, Linter>>>,
	documents: Arc<RwLock<HashMap<Url, Document>>>,
}

impl RegexLinterServer {
	fn new(client: Client) -> Self {
		// We can just pass a null value to get the default configs =]]
		let linters = linter::parse_config(&serde_json::Value::Null);
		Self::print_linter_info(&linters);
		return Self {
			client: client,
			linters: Arc::new(RwLock::new(linters)),
			documents: Arc::new(RwLock::new(HashMap::new())),
		};
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

	async fn publish_diagnostics(&self, uri: &Url, document: &Document) -> () {
		let results = if let Ok(linters) = self.linters.read() {
			linter::scan(&document, &linters)
		}
		else {
			vec![]
		};

		self.client.publish_diagnostics(uri.clone(), results, Some(document.version)).await;
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
				text_document_sync: Some(TextDocumentSyncCapability::Options(
					TextDocumentSyncOptions {
						open_close: Some(true),
						change: Some(TextDocumentSyncKind::FULL),
						save: Some(TextDocumentSyncSaveOptions::Supported(true)),
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
		let parsed_settings = params.settings.as_object().and_then(|s| s.get(LANGUAGE_SERVER_ID));
		if let Some(parsed_settings) = parsed_settings && let Ok(mut linters) = self.linters.write() {
			*linters = linter::parse_config(parsed_settings);
			Self::print_linter_info(&linters);
		}
	}

	async fn did_open(&self, params: DidOpenTextDocumentParams) -> () {
		let text_document = params.text_document;
		let uri = text_document.uri;
		let doc = if let Ok(mut docs) = self.documents.write() {
			let new_doc = Document {
				language_id: text_document.language_id,
				text: text_document.text,
				version: text_document.version,
				has_unsaved_changes: false,
			};

			docs.insert(uri.clone(), new_doc.clone());
			Some(&new_doc.clone())
		}
		else {
			None
		};

		if doc.is_some() {
			self.publish_diagnostics(&uri, doc.unwrap()).await;
		}
	}

	async fn did_change(&self, params: DidChangeTextDocumentParams) -> () {
		let text_document = params.text_document;
		let uri = text_document.uri;
		let doc = if let Some(change) = params.content_changes.into_iter().next() && let Ok(mut docs) = self.documents.write() && let Some(doc) = docs.get_mut(&uri) {
			// The change contains the **full** text
			doc.text = change.text;
			doc.version = text_document.version;
			doc.has_unsaved_changes = true;
			Some(&doc.clone())
		}
		else {
			None
		};

		if doc.is_some() {
			self.publish_diagnostics(&uri, doc.unwrap()).await;
		}
	}

	async fn did_save(&self, params: DidSaveTextDocumentParams) -> () {
		let uri = params.text_document.uri;
		let doc = if let Ok(mut docs) = self.documents.write() && let Some(doc) = docs.get_mut(&uri) {
			doc.has_unsaved_changes = false;
			Some(&doc.clone())
		}
		else {
			None
		};

		if doc.is_some() {
			self.publish_diagnostics(&uri, doc.unwrap()).await;
		}
	}

	async fn did_close(&self, params: DidCloseTextDocumentParams) -> () {
		let uri = params.text_document.uri;
		if let Ok(mut docs) = self.documents.write() {
			docs.remove(&uri);
		}

		self.client.publish_diagnostics(uri, vec![], None).await;
	}
}

#[tokio::main]
async fn main() -> () {
	let (service, socket) = LspService::new(RegexLinterServer::new);
	Server::new(stdin(), stdout(), socket).serve(service).await;
}
