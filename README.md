# The fuck is this

This is a language server that acts as a (real-time) linter for the Zed editor, where you can configure your own linters with just a few JSON options. Matches are (obviously) handled via regex and will be reported back to the editor as inline diagnostics.

There's currently **one** whole default linter: `annotations`, which is for reporting comments like `// TODO` and `# FIXME`. It's disabled by default though.

## Installation

You can simply install it directly via Zed > `Extensions` > `Regex Linter LSP`.

## Config

```jsonc
{
	"lsp": {
		"regex-linter": {
			"settings": {
				// The key will be used as the LSP's "source", which is the name you see on diagnostics
				"my-cool-linter": {
					// **All** settings are technically optional, but it won't do much without any regex patterns
					// Default is `true` of course
					"enabled": false,

					// For only matching within comments, which is based off a hardcoded list of comment markers for every language we support
					// Note that this is a pretty dumb match, it won't detect multiline comments if the first line has different syntax than the others (like `/*` and `*`)
					// The only thing we look for is any ASCII whitespace preceding the marker (or the start of the line), so common markers like `//` at least won't trigger on URLs
					// The default is `true` regardless
					"comments_only": false,

					// Only apply to these languages, based on Zed's language names (default is all languages the extension knows about, i.e. what's in `extension.toml`)
					"languages": ["Rust"],

					// The default for all of these is just an empty list (obviously)
					// Note that the regexes are case-**sensitive**, but you can make something case-insensitive by using the `(?i)` flag in a separate group
					// Also, you should prolly avoid using named capture groups because they might conflict with our own (you can't really do anything with them anyway)
					"error": ["MY_?ERROR", "((?i)MY_CRITICAL)"],
					"warning": ["MY_?WARNING"],
					"info": ["MY_?INFO"],
				},

				"another-cool-linter": {
					// Same options
				},

				// If you wanna enable built-in linters then you only have to enable them, but you can also extend their default configs here by using the same options as above
				// Note that the simple boolean toggles and `languages` will always be overridden by your own config, but the regex lists will be **merged**
				"annotations": {
					"enabled": true,
				},
			},
		},
	},
}
```

## Development setup

If you want to build the **extension** manually then **you'll need support for the `wasm32-wasip2` Rust target**. This is not necessary for the LSP server itself.

There's a `compilem.sh` script in both the repo root and `lsp-server`, you can use that to generate release builds. Then in your Zed settings, set the path to the local LSP binary if needed:

```jsonc
{
	"lsp": {
		"regex-linter": {
			"binary": {
				"path": "/some/path/to/repo/root/lsp-server/target/release/regex-linter-lsp-server"
			},
			"settings": {
				// ...
			},
		},
	},
}
```
