// vilan syntax highlighting for the rendered docs (proposal/docs-site.md §4).
//
// Two jobs:
// 1. Register a highlight.js grammar for vilan (regex-level — keywords,
//    types, suffixed numbers, strings and i-strings, attributes, comments;
//    lexer-true highlighting is the recorded v2).
// 2. The fence-tag shim: docs fences carry harness tags the highlighter
//    doesn't know (```vilan,browser → class "language-vilan,browser").
//    Normalize those to plain vilan, then highlight. `text` fences (the
//    ASCII diagrams) and `fragment`-tagged blocks highlight as vilan too —
//    fragments are still vilan syntax, just not compilable standalone.
(function () {
	if (typeof hljs === "undefined") {
		return;
	}

	hljs.registerLanguage("vilan", function (hljs) {
		const KEYWORDS = {
			keyword:
				"async await borrows const else enum export external for fun if impl " +
				"import in is jump let macro match mod mut own resource ret struct trait " +
				"type use with",
			literal: "true false null void self Self",
			built_in: "print panic assert",
		};
		const NUMBER = {
			className: "number",
			variants: [
				{ begin: "\\b0x[0-9a-fA-F]+(?:[iu](?:8|16|32|53)|f32|f64|[fn])?" },
				{ begin: "\\b\\d+(?:\\.\\d+)?(?:[iu](?:8|16|32|53)|f32|f64|[fn])?" },
			],
		};
		const PLAIN_STRING = {
			className: "string",
			begin: '"',
			end: /"|$/,
			contains: [{ begin: "\\\\." }],
		};
		// i"…{hole}…" — the holes carry expressions; render them as substitutions.
		const INTERPOLATED = {
			className: "string",
			begin: 'i"',
			end: /"|$/,
			contains: [
				{ begin: "\\\\." },
				{ className: "subst", begin: "\\{", end: "\\}" },
			],
		};
		const MULTILINE = {
			className: "string",
			begin: '"""',
			end: '"""',
		};
		// i"""…{hole}…""" — the multiline form with holes (H7). Listed before both
		// MULTILINE (which would not match at the `i`) and INTERPOLATED (which
		// would end at the second of the three quotes). Only `\{` and `\}` are
		// escapes here.
		const MULTILINE_INTERPOLATED = {
			className: "string",
			begin: 'i"""',
			end: '"""',
			contains: [
				{ begin: "\\\\[{}]" },
				{ className: "subst", begin: "\\{", end: "\\}" },
			],
		};
		const ATTRIBUTE = {
			className: "meta",
			begin: "^\\s*\\[(?:derive|service|extern|must_use|rpc|trait_only|doc|expose|macro)\\b",
			end: "\\]",
		};
		const TYPE = {
			className: "type",
			begin: "\\b[A-Z][a-zA-Z0-9_]*",
		};
		const FUNCTION = {
			className: "title",
			begin: "(?<=\\bfun\\s)[a-z_][a-zA-Z0-9_]*",
		};
		// Element syntax: tag names after `<`/`</` and the `on:` event form.
		// Regex-level like the rest — `<` glued to a name reads as markup,
		// which is the grammar's own atom-position rule; a spaced comparison
		// (`a < b`) never matches.
		const ELEMENT_TAG = {
			className: "name",
			begin: "(?<=</?)[a-z][a-zA-Z0-9_]*(?:-[a-zA-Z0-9_]+)*",
		};
		const ELEMENT_EVENT = {
			className: "attr",
			begin: "\\bon:[a-z][a-zA-Z0-9_]*",
		};
		return {
			name: "vilan",
			keywords: KEYWORDS,
			contains: [
				hljs.COMMENT("//", "$"),
				ATTRIBUTE,
				MULTILINE_INTERPOLATED,
				MULTILINE,
				INTERPOLATED,
				PLAIN_STRING,
				NUMBER,
				ELEMENT_TAG,
				ELEMENT_EVENT,
				FUNCTION,
				TYPE,
			],
		};
	});

	// The shim: mdBook already ran highlight.js over the page before
	// additional-js loads, so re-highlight every vilan-tagged block with the
	// grammar registered above. (mdBook bundles highlight.js v10, whose
	// entry point is highlightBlock; prefer highlightElement when a newer
	// bundle provides it.) The harness tag is stashed on the element first:
	// normalizing destroys it, and the playground-link pass below reads it.
	var highlight = hljs.highlightElement || hljs.highlightBlock;
	document.querySelectorAll("code[class*='language-vilan']").forEach(function (block) {
		var tagged = block.className.match(/language-(vilan[^\s]*)/);
		if (tagged) {
			block.dataset.vilanTag = tagged[1];
		}
		block.className = "language-vilan";
		highlight.call(hljs, block);
	});
})();

// "Open in the vilan playground" links (the D11 book tie-in): every fence
// that is a complete program gets one, putting the code itself in the URL
// fragment - deflate-raw, base64url, the SAME codec the playground's Share
// writes (playground/editor-src/editor.mjs in the website repo; resync both
// when either moves). A process-leg example carries `&mode=node`, so it
// opens straight into the server check and the platform story shows itself.
// Fragments and mainless blocks are skipped; a browser without
// CompressionStream simply sees no links.
(function () {
	if (!("CompressionStream" in window)) {
		return;
	}

	var PLAYGROUND = "https://vilan-lang.org/playground/";
	// The docs harness compiles untagged fences on the process leg, but most
	// are platform-neutral and RUN in the browser; only these imports mark a
	// program as genuinely process-bound. A wrong guess is self-correcting:
	// the reader lands on a diagnostic and the mode select is right there.
	var PROCESS_HINT = /\bstd::(http|fs|db|process|rpc_server|ws)\b/;

	function encodeBase64Url(bytes) {
		var binary = "";
		for (var i = 0; i < bytes.length; i++) {
			binary += String.fromCharCode(bytes[i]);
		}
		return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
	}

	function deflate(text) {
		var stream = new Blob([new TextEncoder().encode(text)])
			.stream()
			.pipeThrough(new CompressionStream("deflate-raw"));
		return new Response(stream).arrayBuffer();
	}

	var style = document.createElement("style");
	style.textContent =
		"pre .buttons a.vilan-playground-link { text-decoration: none; cursor: pointer; }";
	document.head.appendChild(style);

	document.querySelectorAll("code[data-vilan-tag]").forEach(function (block) {
		var tag = block.dataset.vilanTag;
		if (tag.indexOf("fragment") !== -1) {
			return;
		}
		var source = block.textContent;
		if (!/\bfun main\b/.test(source)) {
			return;
		}
		var node = tag.indexOf("browser") === -1 && PROCESS_HINT.test(source);
		deflate(source).then(function (buffer) {
			var link = document.createElement("a");
			link.className = "vilan-playground-link";
			link.href =
				PLAYGROUND + "#code=" + encodeBase64Url(new Uint8Array(buffer)) + (node ? "&mode=node" : "");
			link.target = "_blank";
			link.rel = "noopener";
			link.title = node
				? "Check in the vilan playground (server leg)"
				: "Run in the vilan playground";
			link.setAttribute("aria-label", link.title);
			link.textContent = "▶";
			var pre = block.parentElement;
			var buttons = pre.querySelector(".buttons");
			if (!buttons) {
				buttons = document.createElement("div");
				buttons.className = "buttons";
				pre.insertBefore(buttons, pre.firstChild);
			}
			buttons.insertBefore(link, buttons.firstChild);
		});
	});
})();
