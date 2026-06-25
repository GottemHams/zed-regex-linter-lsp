#![allow(clippy::needless_return)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::unused_unit)]

mod linter;

use linter::Linter;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::io::{stdin, stdout};
use tokio::task::AbortHandle;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const LANGUAGE_SERVER_ID: &str = "regex-linter";
const DEBOUNCE_MS: Duration = Duration::from_millis(300);

#[derive(Clone)]
struct Document {
	language_id: String,
	text: Arc<str>, // Let's put this in an Arc to avoid ever copying the full text
	version: i32,
}

#[derive(Default)]
struct LintTask {
	document_version: i32,
	handle: Option<AbortHandle>,
}

#[derive(Clone)]
struct RegexLinterServer {
	client: Client,
	linters: Arc<RwLock<HashMap<String, Linter>>>,
	documents: Arc<RwLock<HashMap<Url, Document>>>,
	lint_tasks: Arc<Mutex<HashMap<Url, LintTask>>>,
}

impl RegexLinterServer {
	fn new(client: Client) -> Self {
		// We can just pass a null value to get the default configs =]]
		let linters = linter::parse_config(&serde_json::Value::Null);
		Self::printem_linter_info(&linters);
		return Self {
			client: client,
			linters: Arc::new(RwLock::new(linters)),
			documents: Arc::new(RwLock::new(HashMap::new())),
			lint_tasks: Arc::new(Mutex::new(HashMap::new())),
		};
	}

	fn printem_linter_info(linters: &HashMap<String, Linter>) -> () {
		if linters.is_empty() {
			eprintln!("[WARN] No linters were loaded");
		}
		else {
			eprintln!("Loaded linters:");
			for (source, linter) in linters {
				eprintln!("- {}: {:?}", source, linter);
			}
		}
	}

	fn lintem(&self, url: &Url, document: &Document) -> () {
		let Ok(mut tasks) = self.lint_tasks.lock() else {
			return;
		};

		// Repeated requests for the same (latest) document version are fine, as long as we don't interrupt existing runs
		let task = tasks.entry(url.clone()).or_default();
		if let Some(handle) = &task.handle && !handle.is_finished() {
			if task.document_version >= document.version {
				return;
			}

			handle.abort();
		}

		let this = self.clone();
		let current_url = url.clone();
		let current_doc = document.clone();

		task.document_version = current_doc.version;
		task.handle = Some(tokio::spawn(async move {
			tokio::time::sleep(DEBOUNCE_MS).await;

			// We'll check the exact version here, because that's what we were originally scheduled for
			let expected_version = this.documents.read()
				.ok()
				.and_then(|docs| docs.get(&current_url).map(|doc| doc.version));

			if expected_version != Some(current_doc.version) {
				return;
			}

			let results = if let Ok(linters) = this.linters.read() {
				linter::scan(&current_doc, &linters)
			}
			else {
				vec![]
			};

			this.client.publish_diagnostics(current_url.clone(), results, Some(current_doc.version)).await;
		}).abort_handle());
	}

	async fn clearem_lint(&self, url: &Url) -> () {
		self.client.publish_diagnostics(url.clone(), vec![], None).await;
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
		let parsed_settings = params.settings.as_object().and_then(|settings| settings.get(LANGUAGE_SERVER_ID));
		if let Some(parsed_settings) = parsed_settings && let Ok(mut linters) = self.linters.write() {
			*linters = linter::parse_config(parsed_settings);
			Self::printem_linter_info(&linters);
		}

		if let Ok(docs) = self.documents.read() {
			for (url, doc) in docs.iter() {
				self.lintem(url, doc);
			}
		}
	}

	async fn did_open(&self, params: DidOpenTextDocumentParams) -> () {
		let text_document = params.text_document;
		let uri = text_document.uri;
		let Ok(mut docs) = self.documents.write() else {
			return;
		};

		let new_doc = Document {
			language_id: text_document.language_id,
			text: Arc::from(text_document.text),
			version: text_document.version,
		};

		docs.insert(uri.clone(), new_doc.clone());
		self.lintem(&uri, &new_doc);
	}

	async fn did_change(&self, params: DidChangeTextDocumentParams) -> () {
		let text_document = params.text_document;
		let uri = text_document.uri;
		let Some(change) = params.content_changes.into_iter().next() else {
			return;
		};

		if let Ok(mut docs) = self.documents.write() && let Some(doc) = docs.get_mut(&uri) {
			// The change contains the **full** text
			doc.text = Arc::from(change.text);
			doc.version = text_document.version;
			self.lintem(&uri, &doc);
		}
	}

	async fn did_save(&self, params: DidSaveTextDocumentParams) -> () {
		let uri = params.text_document.uri;
		if let Ok(docs) = self.documents.write() && let Some(doc) = docs.get(&uri) {
			self.lintem(&uri, &doc);
		}
	}

	async fn did_close(&self, params: DidCloseTextDocumentParams) -> () {
		let uri = params.text_document.uri;
		if let Ok(mut tasks) = self.lint_tasks.lock() && let Some(task) = tasks.remove(&uri) && let Some(handle) = task.handle {
			handle.abort();
		}

		if let Ok(mut docs) = self.documents.write() {
			docs.remove(&uri);
		}

		self.clearem_lint(&uri).await;
	}
}

#[tokio::main]
async fn main() -> () {
	let (service, socket) = LspService::new(RegexLinterServer::new);
	Server::new(stdin(), stdout(), socket).serve(service).await;
}
