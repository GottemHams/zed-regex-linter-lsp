mod annotations;

use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use tower_lsp::lsp_types::*;

use crate::Document;

type LanguageName = &'static str;
type LanguageId = &'static str;
type CommentStart = &'static str;
type BlockCommentEnd = Option<&'static str>;
type CommentMarkers = (CommentStart, BlockCommentEnd);

// This is for mapping Zed languages in `settings.json` to LSP language IDs, so the config is a bit more intuitive
const LANGUAGE_ID_MAP: &[(LanguageName, LanguageId)] = &[
	("C", "c"),
	("C++", "cpp"),
	("C#", "csharp"),
	("CSS", "css"),
	("Go", "go"),
	("HTML", "html"),
	("Java", "java"),
	("JavaScript", "javascript"),
	("Kotlin", "kotlin"),
	("Less", "less"),
	("Lua", "lua"),
	("Perl", "perl"),
	("PHP", "php"),
	("PowerShell", "powershell"),
	("Python", "python"),
	("Ruby", "ruby"),
	("Rust", "rust"),
	("SASS", "sass"),
	("SCSS", "scss"),
	("Shell Script", "shell script"), // Apparently the ID contains a space too??¿¿¿÷¿??//
	("Swift", "swift"),
	("TSX", "tsx"),
	("TypeScript", "typescript"),
	("YAML", "yaml"),
];

// These should always be preceded by some ASCII whitespace (or the start of the line) to be detected as a comment, so we can avoid (some) false positives in strings etc
const COMMENT_MARKER_MAP: &[(LanguageId, &[CommentMarkers])] = &[
	("c", &[("//", None), ("/*", Some("*/"))]),
	("cpp", &[("//", None), ("/*", Some("*/"))]),
	("csharp", &[("//", None), ("/*", Some("*/"))]),
	("css", &[("/*", Some("*/"))]),
	("go", &[("//", None), ("/*", Some("*/"))]),
	("html", &[("<!--", Some("-->"))]),
	("java", &[("//", None), ("/*", Some("*/"))]),
	("javascript", &[("//", None), ("/*", Some("*/"))]),
	("kotlin", &[("//", None), ("/*", Some("*/"))]),
	("less", &[("//", None), ("/*", Some("*/"))]),
	("lua", &[("--", None)]),
	("perl", &[("#", None)]),
	("php", &[("//", None), ("/*", Some("*/"))]),
	("powershell", &[("#", None), ("<#", Some("#>"))]),
	("python", &[("#", None)]),
	("ruby", &[("#", None)]),
	("rust", &[("//", None), ("/*", Some("*/"))]),
	("sass", &[("//", None), ("/*", Some("*/"))]),
	("scss", &[("//", None), ("/*", Some("*/"))]),
	("shell script", &[("#", None)]),
	("swift", &[("//", None), ("/*", Some("*/"))]),
	("tsx", &[("//", None), ("/*", Some("*/"))]),
	("typescript", &[("//", None), ("/*", Some("*/"))]),
	("yaml", &[("#", None)]),
];

#[derive(Debug)]
pub struct Linter {
	enabled: bool,
	comments_only: bool,
	languages: Option<Vec<String>>,

	// Any of these can be `None` if regex compilation fails
	error_regex: Option<Regex>,
	warning_regex: Option<Regex>,
	info_regex: Option<Regex>,
}

#[derive(Default, Deserialize)]
struct LinterConfig {
	enabled: Option<bool>,
	comments_only: Option<bool>,
	languages: Option<Vec<String>>,
	error: Option<Vec<String>>,
	warning: Option<Vec<String>>,
	info: Option<Vec<String>>,
}

impl LinterConfig {
	fn merge(&mut self, other: LinterConfig) -> () {
		if let Some(enabled) = other.enabled {
			self.enabled = Some(enabled);
		}

		if let Some(comments_only) = other.comments_only {
			self.comments_only = Some(comments_only);
		}

		if let Some(languages) = other.languages {
			self.languages = Some(languages);
		}

		self.error.get_or_insert_with(Vec::new).extend(other.error.unwrap_or_default());
		self.warning.get_or_insert_with(Vec::new).extend(other.warning.unwrap_or_default());
		self.info.get_or_insert_with(Vec::new).extend(other.info.unwrap_or_default());
	}
}

pub fn parse_config(settings: &serde_json::Value) -> HashMap<String, Linter> {
	let mut configs = HashMap::new();
	configs.insert(annotations::SOURCE.to_string(), annotations::config());

	if let Some(obj) = settings.as_object() {
		for (key, value) in obj {
			let linter_config = serde_json::from_value::<LinterConfig>(value.clone())
				.inspect_err(|e| eprintln!("Failed to parse config for linter '{}': {}", key, e));

			if let Ok(linter_config) = linter_config {
				configs.entry(key.to_string()).or_default().merge(linter_config);
			}
		}
	}

	return configs.into_iter()
		.map(|(name, config)| (name, Linter {
			enabled: config.enabled.unwrap_or(true),
			comments_only: config.comments_only.unwrap_or(true),
			languages: config.languages.map(|langs| {
				// Languages found in `LANGUAGE_ID_MAP` will be mapped to their LSP language ID counterparts, otherwise we'll retain the original values (so you could also specify IDs directly)
				langs.into_iter().map(|lang| {
					LANGUAGE_ID_MAP.iter()
						.find(|(zed_lang, _)| *zed_lang == lang)
						.map(|(_, lsp_lang)| lsp_lang.to_string())
						.unwrap_or(lang)
				})
				.collect()
			}),
			error_regex: compilem_regexes(&config.error.unwrap_or_default()),
			warning_regex: compilem_regexes(&config.warning.unwrap_or_default()),
			info_regex: compilem_regexes(&config.info.unwrap_or_default()),
		}))
		.collect();
}

fn compilem_regexes(patterns: &[String]) -> Option<Regex> {
	if patterns.is_empty() {
		return None;
	}

	// Sort by longest patterns first so more specific matches take precedence
	let mut sorted: Vec<&str> = patterns.iter().map(|pattern| pattern.as_ref()).collect();
	sorted.sort_unstable_by_key(|pattern| std::cmp::Reverse(pattern.len()));

	let full_pattern = format!(r"(?P<word>{})(?::|\s+-+)?\s*(?P<message>.*)", sorted.join("|"));
	return Regex::new(&full_pattern)
		.inspect_err(|e| eprintln!("Failed to compile pattern: {}\n{}", full_pattern, e))
		.ok();
}

fn find_comment_text(line: &str, comment_markers: &CommentMarkers) -> Option<(usize, usize)> {
	let line_butts = line.as_bytes();
	let (start_marker, end_marker) = comment_markers;

	let mut slice_start = 0;
	while let Some(start_pos) = line[slice_start..].find(start_marker) {
		let abs_start_pos = slice_start + start_pos;
		slice_start = abs_start_pos + start_marker.len();
		if abs_start_pos > 0 && !line_butts[abs_start_pos - 1].is_ascii_whitespace() {
			continue;
		}

		let slice_end = if let Some(end_marker) = end_marker && let Some(end_pos) = line[slice_start..].find(end_marker) {
			slice_start + end_pos
		}
		else {
			line.len()
		};

		return Some((slice_start, slice_end));
	}

	return None;
}

pub fn scan(document: &Document, linters: &HashMap<String, Linter>) -> Vec<Diagnostic> {
	let text = &document.text;
	let language_id = &document.language_id;

	let comment_markers = COMMENT_MARKER_MAP.iter()
		.find(|(lang_id, _)| *lang_id == language_id)
		.map(|(_, markers)| *markers)
		.unwrap_or_default();

	let mut diagnostics = Vec::new();
	for (source, linter) in linters {
		if !linter.enabled {
			continue;
		}

		if linter.comments_only && comment_markers.is_empty() {
			continue;
		}

		if linter.languages.as_ref().is_some_and(|langs| !langs.iter().any(|lang_id| lang_id == language_id)) {
			continue;
		}

		let severity_groups = [
			(&linter.error_regex, DiagnosticSeverity::ERROR),
			(&linter.warning_regex, DiagnosticSeverity::WARNING),
			(&linter.info_regex, DiagnosticSeverity::INFORMATION),
		];

		for (line_num, line) in text.lines().enumerate() {
			let scannable_range = if linter.comments_only {
				let Some(comment_range) = comment_markers.iter().filter_map(|markers| find_comment_text(line, markers)).min() else {
					continue;
				};

				comment_range
			}
			else {
				(0, line.len())
			};

			let (scan_start, scan_end) = scannable_range;
			let scannable_text = &line[scan_start..scan_end];
			for (regex, severity) in severity_groups {
				let Some(regex) = regex else {
					continue;
				};

				let Some(matches) = regex.captures(scannable_text) else {
					continue;
				};

				let Some(word_match) = matches.name("word") else {
					continue;
				};

				let word = word_match.as_str();
				let message = matches.name("message")
					.map(|message| message.as_str().trim())
					.unwrap_or("");

				// LSP positions are based on UTF-16 code units by default
				let start_char = line[..scan_start + word_match.start()].encode_utf16().count();
				let end_char = start_char + word.encode_utf16().count();
				let display_msg = if message.is_empty() {
					word.to_string()
				}
				else {
					format!("{} — {}", word, message)
				};

				diagnostics.push(Diagnostic {
					range: Range {
						start: Position {
							line: line_num as u32,
							character: start_char as u32,
						},
						end: Position {
							line: line_num as u32,
							character: end_char as u32,
						},
					},
					severity: Some(severity),
					code: None,
					code_description: None,
					source: Some(source.clone()),
					message: display_msg,
					related_information: None,
					tags: None,
					data: None,
				});
			}
		}
	}

	return diagnostics;
}
