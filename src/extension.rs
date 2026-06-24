#![allow(clippy::needless_return)]
#![allow(clippy::unused_unit)]

use std::fs;
use zed_extension_api::{self as zed, Result, settings::LspSettings};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const RELEASE_BASE_URL: &str = "https://github.com/GottemHams/zed-regex-linter-lsp/releases/download";

struct RegexLinterLspExtension {
	cached_lsp_server_path: Option<String>,
}

impl zed::Extension for RegexLinterLspExtension {
	fn new() -> Self {
		return Self {
			cached_lsp_server_path: None,
		};
	}

	fn language_server_command(&mut self, language_server_id: &zed::LanguageServerId, _worktree: &zed::Worktree) -> Result<zed::Command> {
		return match language_server_id.as_ref() {
			Self::LANGUAGE_SERVER_ID => Ok(zed::Command {
				command: self.lsp_server_path()?,
				args: vec![],
				env: Default::default(),
			}),

			language_server_id => Err(format!("Unknown language server: {}", language_server_id)),
		};
	}

	fn language_server_workspace_configuration(&mut self, language_server_id: &zed::LanguageServerId, worktree: &zed::Worktree) -> Result<Option<serde_json::Value>> {
		return match language_server_id.as_ref() {
			Self::LANGUAGE_SERVER_ID => {
				let settings = LspSettings::for_worktree(Self::LANGUAGE_SERVER_ID, worktree)
					.ok()
					.and_then(|lsp_settings| lsp_settings.settings)
					.unwrap_or_default();

				Ok(Some(serde_json::json!({
					Self::LANGUAGE_SERVER_ID: settings,
				})))
			},

			_ => Ok(None),
		};
	}
}

impl RegexLinterLspExtension {
	const LANGUAGE_SERVER_ID: &str = "regex-linter";

	fn lsp_server_path(&mut self) -> Result<String> {
		if let Some(cached_path) = &self.cached_lsp_server_path && fs::metadata(cached_path).is_ok() {
			return Ok(cached_path.clone());
		}

		let path = Self::find_or_download_binary()?;
		self.cached_lsp_server_path = Some(path.clone());
		return Ok(path);
	}

	fn find_or_download_binary() -> Result<String> {
		let (os, arch) = zed::current_platform();
		let platform_suffix = match (os, arch) {
			(zed::Os::Windows, zed::Architecture::X8664) => "-windows-x64.exe",
			(zed::Os::Windows, zed::Architecture::Aarch64) => "-windows-arm64.exe",
			(zed::Os::Windows, _) => ".exe",

			(zed::Os::Mac, zed::Architecture::Aarch64) => "-macos-arm64",
			(zed::Os::Mac, zed::Architecture::X8664) => "-macos-x64",

			(zed::Os::Linux, zed::Architecture::X8664) => "-linux-x64",
			(zed::Os::Linux, zed::Architecture::Aarch64) => "-linux-arm64",

			_ => "",
		};

		let working_dir = format!("{}-{}", PACKAGE_NAME, VERSION);
		let binary_name = format!("regex-linter-lsp-server{}", platform_suffix);
		let download_path = format!("{}/{}", working_dir, binary_name);
		if fs::metadata(&download_path).is_ok() {
			return Ok(download_path);
		}

		let (archive_ext, archive_type) = match os {
			zed::Os::Windows => ("zip", zed::DownloadedFileType::Zip),
			_ => ("tar.gz", zed::DownloadedFileType::GzipTar),
		};

		let archive_name = format!("{}.{}", binary_name, archive_ext);
		let release_url = format!("{}/v{}/{}", RELEASE_BASE_URL, VERSION, archive_name);
		zed::download_file(&release_url, &working_dir, archive_type)
			.map_err(|e| format!("Failed to download binary from {}: {}", release_url, e))?;

		fs::metadata(&download_path)
			.map_err(|_| format!("Binary not found after extraction, which was expected at: {}", download_path))?;

		return Ok(download_path);
	}
}

zed::register_extension!(RegexLinterLspExtension);
