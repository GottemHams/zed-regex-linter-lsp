#![allow(clippy::bind_instead_of_map)]
#![allow(clippy::needless_return)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::unused_unit)]

mod linter;

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::io::{stdin, stdout};
use tokio::task::AbortHandle;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::*;

use linter::Linter;

const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const LANGUAGE_SERVER_ID: &str = "regex-linter";
const DEBOUNCE_MS: Duration = Duration::from_millis(300);

#[derive(Clone)]
struct DocumentContent {
	// Let's put the strings in an `Arc` to avoid ever copying them, as we do need to clone this struct pretty often (and possibly in very quick succession)
	language_id: Arc<str>,
	text: Arc<str>,
	version: i32,
}

#[derive(Default)]
struct LintTask {
	document_version: i32,
	handle: Option<AbortHandle>,
}

struct Document {
	content: DocumentContent,
	lint_task: Option<LintTask>,
}

impl Document {
	fn new(content: DocumentContent) -> Self {
		return Self {
			content: content,
			lint_task: None,
		};
	}
}

#[derive(Clone)]
struct RegexLinterServer {
	client: Client,
	linters: Arc<RwLock<HashMap<String, Linter>>>,
	documents: Arc<RwLock<HashMap<Url, Document>>>,
}

impl RegexLinterServer {
	fn new(client: Client) -> Self {
		// We can just pass a null value to get the default configs =]]
		let linters = linter::parsem_config(&serde_json::Value::Null);
		let this = Self {
			client: client,
			linters: Arc::new(RwLock::new(linters)),
			documents: Arc::new(RwLock::new(HashMap::new())),
		};

		this.printem_linter_info();
		return this;
	}

	fn printem_linter_info(&self) -> () {
		let linters = self.linters.read().unwrap();
		if linters.is_empty() {
			eprintln!("[WARN] No linters were loaded");
		}
		else {
			eprintln!("Loaded linters:");
			for (source, linter) in linters.iter() {
				eprintln!("- {}: {:?}", source, linter);
			}
		}
	}

	fn lintem(&self, url: &Url) -> () {
		// Lock poisoning really shouldn't happen, so in that case we'll just shit ourselves =]]
		let mut docs = self.documents.write().unwrap();
		let Some(doc) = docs.get_mut(url) else {
			return;
		};

		let task = doc.lint_task.get_or_insert_default();
		if let Some(handle) = &task.handle {
			// Although it shouldn't really happen in practice, we'll drop out-of-order requests to avoid even scheduling a task that would return early anyway
			if task.document_version > doc.content.version {
				return;
			}

			// Repeated requests for the same (latest) document version are fine, as long as we don't interrupt existing runs
			if task.document_version == doc.content.version && !handle.is_finished() {
				return;
			}

			// Otherwise we should always abort, which has no effect if the task is already done
			handle.abort();
		}

		let this = self.clone();
		let current_url = url.clone();
		let current_content = doc.content.clone();
		task.document_version = current_content.version;
		task.handle = Some(tokio::spawn(async move {
			// We don't have any special semantics for change vs save, we always run on the latest document version
			// This means we don't have to come up with a complex debouncing scheme and can simply treat all events the same
			tokio::time::sleep(DEBOUNCE_MS).await;

			// We'll check the exact version here, because that's what we were originally scheduled for
			let expected_version = this.documents.read().unwrap().get(&current_url).and_then(|doc| Some(doc.content.version));
			if expected_version != Some(current_content.version) {
				return;
			}

			let results = linter::scannem(&current_content, &this.linters.read().unwrap());
			this.client.publish_diagnostics(current_url.clone(), results, Some(current_content.version)).await;
		}).abort_handle());
	}

	async fn clearem_lint(&self, url: &Url) -> () {
		if let Some(doc) = self.documents.write().unwrap().remove(url) && let Some(task) = doc.lint_task && let Some(handle) = task.handle {
			handle.abort();
		}

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
		// All this cleanup is prolly not really necessary but let's bnice =]
		let mut docs = self.documents.write().unwrap();
		for doc in docs.values() {
			if let Some(task) = &doc.lint_task && let Some(handle) = &task.handle {
				handle.abort();
			}
		}

		docs.clear();
		self.linters.write().unwrap().clear();

		eprintln!("Shutdown complete");
		return Ok(());
	}

	async fn did_change_configuration(&self, params: DidChangeConfigurationParams) -> () {
		eprintln!("Configuration change detected, reloading linters...");

		if let Some(lsp_settings) = params.settings.get(LANGUAGE_SERVER_ID) {
			*self.linters.write().unwrap() = linter::parsem_config(lsp_settings);
			self.printem_linter_info();
		}

		let urls: Vec<Url> = self.documents.read().unwrap().keys().cloned().collect();
		for url in &urls {
			self.lintem(url);
		}
	}

	async fn did_open(&self, params: DidOpenTextDocumentParams) -> () {
		let text_document = params.text_document;
		let uri = text_document.uri;
		let new_content = DocumentContent {
			language_id: Arc::from(text_document.language_id),
			text: Arc::from(text_document.text),
			version: text_document.version,
		};

		let prev_doc = self.documents.write().unwrap().insert(uri.clone(), Document::new(new_content));
		if let Some(prev_doc) = prev_doc && let Some(task) = prev_doc.lint_task && let Some(handle) = task.handle {
			// Shouldn't really be possible because `did_close()` should already have removed the entry, but we may not **necessarily** run in that exact order
			handle.abort();
		}

		self.lintem(&uri);
	}

	async fn did_change(&self, params: DidChangeTextDocumentParams) -> () {
		let text_document = params.text_document;
		let uri = text_document.uri;

		// The change contains the **full** text, so there should be only one
		let Some(change) = params.content_changes.into_iter().next() else {
			return;
		};

		// Let's ensure any out-of-order requests don't revert that shit to an older version
		// We also won't re-lint as the results will most likely be incorrect anyway
		let gucci = if let Some(doc) = self.documents.write().unwrap().get_mut(&uri) && text_document.version > doc.content.version {
			let content = &mut doc.content;
			content.text = Arc::from(change.text);
			content.version = text_document.version;
			true
		}
		else {
			false
		};

		if gucci {
			self.lintem(&uri);
		}
	}

	async fn did_save(&self, params: DidSaveTextDocumentParams) -> () {
		self.lintem(&params.text_document.uri);
	}

	async fn did_close(&self, params: DidCloseTextDocumentParams) -> () {
		self.clearem_lint(&params.text_document.uri).await;
	}
}

#[tokio::main]
async fn main() -> () {
	let (service, socket) = LspService::new(RegexLinterServer::new);
	Server::new(stdin(), stdout(), socket).serve(service).await;
}
