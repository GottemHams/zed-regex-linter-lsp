#![allow(clippy::bind_instead_of_map)]
#![allow(clippy::needless_return)]
#![allow(clippy::redundant_field_names)]
#![allow(clippy::unused_unit)]

use std::fs;
use zed_extension_api as zed;
use zed_extension_api::Result;
use zed_extension_api::settings::LspSettings;

const PACKAGE_NAME: &str = env!("CARGO_PKG_NAME");
const VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASE_GITHUB_REPO: &str = "GottemHams/zed-regex-linter-lsp";

struct RegexLinterLspExtension {
	cached_lsp_server_path: Option<String>,
}

impl RegexLinterLspExtension {
	const LANGUAGE_SERVER_ID: &str = "regex-linter";

	fn lsp_server_path(&mut self, language_server_id: &zed::LanguageServerId) -> Result<String> {
		if let Some(cached_path) = &self.cached_lsp_server_path && fs::metadata(cached_path).is_ok_and(|stat| stat.is_file()) {
			return Ok(cached_path.clone());
		}

		let path = Self::find_or_download_binary(language_server_id)?;
		self.cached_lsp_server_path = Some(path.clone());
		return Ok(path);
	}

	fn find_or_download_binary(language_server_id: &zed::LanguageServerId) -> Result<String> {
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

		let version_dir = format!("{}-server-{}", PACKAGE_NAME, VERSION);
		let binary_name = format!("{}-server{}", PACKAGE_NAME, platform_suffix);
		let binary_path = format!("{}/{}", version_dir, binary_name);
		if fs::metadata(&binary_path).is_ok_and(|stat| stat.is_file()) {
			return Ok(binary_path);
		}

		zed::set_language_server_installation_status(language_server_id, &zed::LanguageServerInstallationStatus::CheckingForUpdate);

		let (asset_ext, download_type) = match os {
			zed::Os::Windows => ("zip", zed::DownloadedFileType::Zip),
			_ => ("tar.gz", zed::DownloadedFileType::GzipTar),
		};

		let asset_name = format!("{}.{}", binary_name, asset_ext);
		let release_tag = format!("v{}", VERSION);
		let release = zed::github_release_by_tag_name(RELEASE_GITHUB_REPO, &release_tag)
			.map_err(|e| format!("[ERROR] Failed to get GitHub release '{}': {}", release_tag, e))?;

		let asset = release.assets.iter().find(|asset| asset.name == asset_name)
			.ok_or_else(|| format!("No GitHub release asset '{}' found for {}", asset_name, release_tag))?;

		zed::set_language_server_installation_status(language_server_id, &zed::LanguageServerInstallationStatus::Downloading);
		zed::download_file(&asset.download_url, &version_dir, download_type)?;
		zed::make_file_executable(&binary_path)?;
		zed::set_language_server_installation_status(language_server_id, &zed::LanguageServerInstallationStatus::None);

		Self::remove_other_versions(&version_dir)?;

		return Ok(binary_path);
	}

	fn remove_other_versions(current_version_dir: &str) -> Result<()> {
		// This shit is loosely based on https://github.com/zed-industries/zed/blob/main/extensions/proto/src/language_servers/util.rs =]]]
		let entries = fs::read_dir(".")
			.map_err(|e| format!("[ERROR] Failed to list working directory: {}", e))?;

		for entry in entries {
			let entry = entry.map_err(|e| format!("[ERROR] Failed to get directory entry: {}", e))?;
			if let Some(filename) = entry.file_name().to_str() && filename.starts_with(Self::LANGUAGE_SERVER_ID) && filename != current_version_dir {
				fs::remove_dir_all(entry.path())
					.inspect_err(|e| eprintln!("[WARN] Failed to remove '{}': {}", filename, e))
					.ok();
			}
		}

		return Ok(());
	}
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
				command: self.lsp_server_path(language_server_id)?,
				args: Default::default(),
				env: Default::default(),
			}),

			language_server_id => Err(format!("[ERROR] Unknown language server: {}", language_server_id)),
		};
	}

	fn language_server_workspace_configuration(&mut self, language_server_id: &zed::LanguageServerId, worktree: &zed::Worktree) -> Result<Option<serde_json::Value>> {
		return match language_server_id.as_ref() {
			Self::LANGUAGE_SERVER_ID => {
				let settings = LspSettings::for_worktree(Self::LANGUAGE_SERVER_ID, worktree)
					.ok()
					.and_then(|lsp_settings| lsp_settings.settings);

				Ok(Some(serde_json::json!({
					Self::LANGUAGE_SERVER_ID: settings,
				})))
			},

			_ => Ok(None),
		};
	}
}

zed::register_extension!(RegexLinterLspExtension);
