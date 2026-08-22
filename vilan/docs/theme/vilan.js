// vilan syntax highlighting for the rendered docs (proposal/docs-site.md §4).
//
// Two jobs:
// 1. Register a highlight.js grammar for vilan (regex-level — keywords,
//    types, suffixed numbers, strings and i-strings, attributes, comments;
//    lexer-true highlighting is the recorded v2).
// 2. The fence-tag shim: docs fences carry harness tags the highlighter
//    doesn't know (```vilan,browser → class "language-vilan browser" under
//    mdBook 0.5, "language-vilan,browser" under 0.4). Normalize those to
//    plain vilan, then highlight. `text` fences (the
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
		// The built-in attribute markers, mirroring `is_known_attribute_marker`
		// in the parser (and the TextMate grammar's own list). `macro` is NOT
		// one — `[macro]` is a `vilan.toml` section, never a source attribute.
		const ATTRIBUTE = {
			className: "meta",
			begin: "^\\s*\\[(?:derive|service|extern|must_use|rpc|trait_only|doc|expose|platform)\\b",
			end: "\\]",
		};
		// `context` and `sync` are CONTEXTUAL: the lexer hands both back as
		// identifiers, so they only read as keywords in the one position each
		// occupies — `context` after a closure type's `)`, `sync` right after
		// the `(` that opens one. Anchored, so a variable named `context` or a
		// type named `Sync` is untouched.
		const CONTEXT_CLAUSE = {
			className: "keyword",
			begin: "(?<=\\)\\s{0,8})context\\b",
		};
		const SYNC_MARKER = {
			className: "keyword",
			begin: "(?<=\\()sync\\b",
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
				CONTEXT_CLAUSE,
				SYNC_MARKER,
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
	// mdBook 0.4 rendered ```vilan,fragment as the single class token
	// `language-vilan,fragment`; 0.5 splits the info string on the comma into
	// separate classes, `language-vilan fragment`. Capture the harness tags in
	// either form — only the harness's own tags (docs.rs's vocabulary), so a
	// neighbouring class like `hljs` is never mistaken for one — and stash them
	// in the comma form the controls pass below reads.
	var highlight = hljs.highlightElement || hljs.highlightBlock;
	document.querySelectorAll("code[class*='language-vilan']").forEach(function (block) {
		var tagged = block.className.match(/language-(vilan(?:[\s,]+(?:browser|fragment|norun))*)/);
		if (tagged) {
			block.dataset.vilanTag = tagged[1].replace(/[\s,]+/g, ",");
		}
		block.className = "language-vilan";
		highlight.call(hljs, block);
	});
})();

// Every fence that is a complete program gets two controls (the D11 book
// tie-in, then K7 — docs-port.md §3.1 S5):
//
// - "Open in the vilan playground" (▶): a link putting the code itself in the
//   URL fragment - deflate-raw, base64url, the SAME codec the playground's
//   Share writes (playground/codec.js in the website repo — the codec's one
//   home, which the editor bundle imports; the copy below is pinned
//   byte-equal to the committed codec-fixture.js beside this file by
//   crates/vilan-cli/tests/book_mirrors.rs, so it cannot drift silently —
//   K15). A process-leg example carries `&mode=node`, so it
//   opens straight into the server check and the platform story shows
//   itself. A browser without CompressionStream simply sees no links.
// - "Run" (or "Check" for a process-leg example): compiles the same source
//   against the published playground compiler and mounts the result under
//   the fence. The compiler is `/playground/worker.js`, which resolves its
//   wasm entirely from its own URL (so it spawns unchanged from a `/docs/`
//   page), spawned lazily on the first click and shared by every fence on
//   the page; the runner is the playground's own sandboxed srcdoc iframe
//   (editor.mjs:780-835), one per run, torn down and rebuilt. A book served
//   without a `/playground/` beside it says so in the panel on the first
//   click, and a browser without module workers or DecompressionStream sees
//   no buttons.
//
// Neither control ever pins a compiler version (`&v=`, `?v=`): the book is
// built from the released compiler and the playground's manifest names the
// released compiler, so they agree by construction and the playground's wasm
// retention stays a free decision (docs-port.md §2.4, §4 Q6).
//
// Fragments and mainless blocks get neither control.
(function () {
	var PLAYGROUND = "https://vilan-lang.org/playground/";
	var WORKER_URL = "/playground/worker.js";
	var canLink = "CompressionStream" in window;
	var canRun = "Worker" in window && "DecompressionStream" in window;
	if (!canLink && !canRun) {
		return;
	}

	// The docs harness compiles untagged fences on the process leg, but most
	// are platform-neutral and RUN in the browser; only these imports mark a
	// program as genuinely process-bound: the std modules of the process layer
	// (`std/vilan.toml`, `[library.layer.process]`) and `std::ui`'s `render`,
	// the SSR twin's one process-only export. A wrong guess is self-correcting:
	// the reader lands on a diagnostic, and in the playground the mode select
	// is right there. The run button makes the same guess: a process-bound
	// program is CHECKED (the browser has no process host to run it),
	// everything else runs.
	var PROCESS_HINT =
		/\bstd::(build|db|document|fs|http|process|rpc_server|watch)\b|\bstd::ui::(?:\{[^}]*\brender\b|render\b)/;

	// --- the share codec -------------------------------------------------
	//
	// The two functions the links need, copied from the codec's one home
	// (the website repo's playground/codec.js) and held byte-equal to the
	// committed codec-fixture.js beside this file by the suite
	// (crates/vilan-cli/tests/book_mirrors.rs). This file is a classic
	// script on a book that must keep working locally with no served
	// playground, so it copies rather than imports — the gate is what keeps
	// the copy honest. Edit the codec at its home, never here first.

	function encodeBase64Url(bytes) {
		let binary = "";
		for (const byte of bytes) {
			binary += String.fromCharCode(byte);
		}
		return btoa(binary).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
	}

	async function deflate(text) {
		const stream = new Blob([new TextEncoder().encode(text)])
			.stream()
			.pipeThrough(new CompressionStream("deflate-raw"));
		return new Uint8Array(await new Response(stream).arrayBuffer());
	}

	// --- the styles -------------------------------------------------------
	//
	// Written against the role tokens the themed book declares on `html`
	// (`--up-*`, `--down-*`, `--stroke-*`, `--primary`; design-language.md
	// §2.1) with mdBook's own variables as the fallback, so the controls stay
	// legible in a book built without the theme. Everything is one style
	// element; the code face and size ride the `--code-*` palette the
	// playground also reads, falling back to mdBook's mono font.
	var style = document.createElement("style");
	style.textContent = [
		"pre .buttons a.vilan-playground-link { text-decoration: none; cursor: pointer; }",
		"pre > .buttons button.vilan-run {",
		"  font-family: inherit; font-size: 12px; font-weight: 600; line-height: 1.2;",
		"  margin: 0 5px; padding: 3px 9px; vertical-align: middle;",
		"  border: 1px solid transparent; border-radius: 4px;",
		"  background: var(--primary, var(--links, #2b79a2)); color: var(--primary-on, #fff);",
		"  cursor: pointer; transition: filter 80ms ease;",
		"}",
		"pre > .buttons button.vilan-run:hover {",
		"  background: var(--primary, var(--links, #2b79a2)); color: var(--primary-on, #fff);",
		"  border-color: transparent; filter: brightness(1.08);",
		"}",
		"pre > .buttons button.vilan-run:active { filter: brightness(0.94); }",
		"pre > .buttons button.vilan-run:disabled { cursor: progress; opacity: 0.6; }",
		// The top margin is in the inherited em on purpose: it cancels the
		// fence's own bottom margin (the same em) down to a 6px seam, whatever
		// the body size is; the code size lands one level down.
		".vilan-run-panel {",
		"  margin: calc(6px - 1em) 0 1em; overflow: hidden;",
		"  border: 1px solid var(--stroke-soft, var(--table-border-color, #ccc)); border-radius: 6px;",
		"  background: var(--down-normal, var(--quote-bg, #f6f7f6)); color: var(--up-normal, var(--fg));",
		"  font-family: var(--code-face, var(--mono-font, monospace));",
		"}",
		".vilan-run-panel > * { font-size: var(--code-size, var(--code-font-size, 0.875em)); line-height: 18px; }",
		".vilan-run-head { display: flex; align-items: center; gap: 8px; padding: 3px 4px 3px 8px; }",
		".vilan-run-status { flex: 1 1 auto; color: var(--up-dim, var(--fg)); }",
		".vilan-run-close {",
		"  flex: 0 0 auto; padding: 0 6px; border: 0; border-radius: 4px; background: transparent;",
		"  color: var(--up-dim, var(--icons, #747474)); font: inherit; line-height: 18px; cursor: pointer;",
		"}",
		".vilan-run-close:hover { background: var(--down-hover, var(--theme-hover, rgba(127, 127, 127, 0.15))); color: var(--up-bright, var(--fg)); }",
		".vilan-run-result { height: 0; overflow: hidden; }",
		".vilan-run-result.vilan-run-open { border-top: 1px solid var(--stroke-soft, var(--table-border-color, #ccc)); }",
		".vilan-run-result iframe { display: block; width: 100%; border: 0; background: transparent; }",
		".vilan-run-diagnostics:empty, .vilan-run-console:empty { display: none; }",
		".vilan-run-diagnostics, .vilan-run-console {",
		"  border-top: 1px solid var(--stroke-soft, var(--table-border-color, #ccc)); white-space: pre-wrap; overflow-wrap: anywhere;",
		"}",
		".vilan-run-console { padding: 4px 0; max-height: 240px; overflow: auto; }",
		".vilan-run-diagnostic {",
		"  padding: 3px 8px; border-top: 1px solid var(--stroke-soft, var(--table-border-color, #ccc));",
		"  border-left: 2px solid var(--down-danger, var(--blockquote-caution-color, #fb7185));",
		"  background: color-mix(in srgb, var(--down-danger, var(--blockquote-caution-color, #fb7185)) 7%, transparent);",
		"}",
		".vilan-run-diagnostic:first-child { border-top-color: transparent; }",
		".vilan-run-diagnostic.vilan-run-warning {",
		"  border-left-color: var(--down-caution, var(--blockquote-warning-color, #fbbf24));",
		"  background: color-mix(in srgb, var(--down-caution, var(--blockquote-warning-color, #fbbf24)) 6%, transparent);",
		"}",
		".vilan-run-severity { font-weight: 600; color: var(--up-error, var(--blockquote-caution-color, #fb7185)); }",
		".vilan-run-warning .vilan-run-severity { color: var(--up-caution, var(--blockquote-warning-color, #fbbf24)); }",
		".vilan-run-site, .vilan-run-note, .vilan-run-quiet { color: var(--up-dim, var(--icons, #747474)); }",
		".vilan-run-line, .vilan-run-quiet { padding: 1px 8px; }",
		".vilan-run-line.vilan-run-error { color: var(--up-error, var(--blockquote-caution-color, #fb7185)); }",
	].join("\n");
	document.head.appendChild(style);

	// --- the compile worker: one per page, spawned on the first Run, shared ---
	//
	// The playground's lifecycle (editor.mjs:544-726), minus what only an
	// editor needs (live checks, latest-wins, format): the wasm instance leaks
	// per compile by design and a panic poisons its memory, so the worker is
	// disposable - recycled after enough compiles, and immediately after any
	// crash. Jobs queue first-in first-out (each fence is its own program, so
	// nothing is superseded) and a recycle keeps the queue: the next job waits
	// for the fresh worker's ready.
	var RECYCLE_AFTER = 32;
	var LOAD_ATTEMPTS = 3;
	var LOAD_FAILED =
		"The compiler could not be loaded: this page is not served beside the playground (" +
		WORKER_URL +
		").";

	var worker = null;
	var ready = false;
	var compileCount = 0;
	var loadFailures = 0;
	var queue = [];
	var active = null;

	function spawn() {
		ready = false;
		var spawned = new Worker(WORKER_URL, { type: "module" });
		worker = spawned;
		worker.onmessage = function (event) {
			if (spawned !== worker) {
				return; // a recycled worker's late word
			}
			var message = event.data;
			if (message.kind === "ready") {
				ready = true;
				loadFailures = 0;
				pump();
			} else if (message.kind === "result") {
				compileCount += 1;
				finish(message);
				if (compileCount >= RECYCLE_AFTER) {
					recycle();
				} else {
					pump();
				}
			} else if (message.kind === "crash") {
				finish(message);
				recycle();
			}
		};
		// A worker-level error before `ready` is a load failure (no
		// /playground/ beside the book, the wasm fetch refused) - respawning
		// forever would loop, so give up after a few and let the next click
		// try afresh. One after `ready` is a worker dying mid-job, which loses
		// the job the way a crash does and recycles the same way.
		worker.onerror = function () {
			if (spawned !== worker) {
				return;
			}
			if (active) {
				finish({ kind: "crash", error: "the compiler worker died" });
				recycle();
				return;
			}
			loadFailures += 1;
			if (loadFailures >= LOAD_ATTEMPTS) {
				abandon();
				return;
			}
			recycle();
		};
	}

	function recycle() {
		if (worker) {
			worker.terminate();
		}
		worker = null;
		ready = false;
		compileCount = 0;
		spawn();
	}

	function abandon() {
		if (worker) {
			worker.terminate();
		}
		worker = null;
		ready = false;
		loadFailures = 0;
		var stranded = queue;
		queue = [];
		if (active) {
			stranded.unshift(active);
			active = null;
		}
		stranded.forEach(function (job) {
			job.settle({ kind: "unavailable", error: LOAD_FAILED });
		});
	}

	function finish(message) {
		var job = active;
		active = null;
		if (job) {
			job.settle(message);
		}
	}

	function pump() {
		if (!ready || active || queue.length === 0) {
			return;
		}
		active = queue.shift();
		active.start();
		worker.postMessage({ action: "compile", source: active.source, platform: active.platform });
	}

	// Queue one compile; `start` fires when it is handed to the worker,
	// `settle` with the worker's answer (`result`, `crash`) or `unavailable`.
	function compile(job) {
		queue.push(job);
		if (!worker) {
			spawn();
		} else {
			pump();
		}
	}

	// --- the runner: one sandboxed iframe per Run, torn down and rebuilt ---
	//
	// `allow-scripts` only: an opaque origin, no same-origin access. The
	// bootstrap forwards console output and uncaught errors to the parent
	// (the playground's, verbatim) and additionally reports the body's height
	// so the panel can size the result to the program — a program that only
	// prints gets no empty box, a widget gets exactly its room. The frame's
	// own layout height is fixed at the cap, so a `100vh` program measures a
	// fixed number and the report converges instead of chasing itself.
	var RESULT_MAX_HEIGHT = 400;
	var CONSOLE_CAP = 300;

	var BOOTSTRAP = `(function () {
	var send = function (kind, text) { parent.postMessage({ kind: kind, text: text }, "*"); };
	var show = function (value) {
		if (typeof value === "string") return value;
		try { return JSON.stringify(value); } catch (error) { return String(value); }
	};
	var wrap = function (name, kind) {
		var original = console[name].bind(console);
		console[name] = function () {
			original.apply(null, arguments);
			send(kind, Array.prototype.map.call(arguments, show).join(" "));
		};
	};
	wrap("log", "log"); wrap("info", "log"); wrap("warn", "error"); wrap("error", "error");
	window.addEventListener("error", function (event) { send("error", event.message); });
	window.addEventListener("unhandledrejection", function (event) { send("error", String(event.reason)); });
	var measure = function () {
		var body = document.body;
		var rect = body.getBoundingClientRect();
		var style = getComputedStyle(body);
		var height = rect.height > 0
			? Math.ceil(rect.height + parseFloat(style.marginTop) + parseFloat(style.marginBottom))
			: 0;
		parent.postMessage({ kind: "size", height: height }, "*");
	};
	if (window.ResizeObserver) new ResizeObserver(measure).observe(document.body);
	window.addEventListener("load", measure);
})();`;

	// Only a literal "</script" (or "</style") can close its tag early; in
	// valid program text the sequence can only sit inside a string, where
	// the escaped spelling means the same thing.
	function escapeScript(text) {
		return text.replace(/<\/script/gi, "<\\/script");
	}

	function escapeStyle(text) {
		return text.replace(/<\/style/gi, "<\\/style");
	}

	function buildSrcdoc(js, css) {
		return [
			"<!doctype html>",
			'<html><head><meta charset="utf-8">',
			// The program's canvas is light whatever the book's theme: the
			// emitted CSS styles its own elements and assumes nothing about
			// the embedder, and a dark page would otherwise lend its ground
			// to default black text.
			"<style>html{color-scheme:light;background:#fff}</style>",
			"<style>" + escapeStyle(css) + "</style>",
			"</head><body>",
			'<div id="app"></div>',
			"<script>" + BOOTSTRAP + "</scr" + "ipt>",
			'<script type="module">' + escapeScript(js) + "</scr" + "ipt>",
			"</body></html>",
		].join("\n");
	}

	// --- the output panel: one per fence, under it ---------------------------

	var panels = [];

	function element(tag, className, text) {
		var node = document.createElement(tag);
		node.className = className;
		if (text !== undefined) {
			node.textContent = text;
		}
		return node;
	}

	function createPanel(pre) {
		var panel = {
			pre: pre,
			root: element("div", "vilan-run-panel"),
			status: element("span", "vilan-run-status"),
			result: element("div", "vilan-run-result"),
			diagnostics: element("div", "vilan-run-diagnostics"),
			console: element("div", "vilan-run-console"),
			frame: null,
			lines: 0,
		};
		var head = element("div", "vilan-run-head");
		var close = element("button", "vilan-run-close", "×");
		close.type = "button";
		close.title = "Close the output";
		close.setAttribute("aria-label", close.title);
		close.addEventListener("click", function () {
			discard(panel);
		});
		panel.status.setAttribute("aria-live", "polite");
		head.appendChild(panel.status);
		head.appendChild(close);
		panel.root.appendChild(head);
		panel.root.appendChild(panel.result);
		panel.root.appendChild(panel.diagnostics);
		panel.root.appendChild(panel.console);
		pre.parentNode.insertBefore(panel.root, pre.nextSibling);
		panels.push(panel);
		return panel;
	}

	function discard(panel) {
		panel.root.remove();
		panel.frame = null;
		panels.splice(panels.indexOf(panel), 1);
	}

	function setState(panel, state, status) {
		panel.root.dataset.state = state;
		panel.status.textContent = status;
	}

	// A fresh run: the previous result, its frame and its report all go.
	function reset(panel) {
		panel.result.textContent = "";
		panel.result.style.height = "0px";
		panel.result.classList.remove("vilan-run-open");
		panel.diagnostics.textContent = "";
		panel.console.textContent = "";
		panel.frame = null;
		panel.lines = 0;
	}

	function renderDiagnostics(panel, diagnostics) {
		diagnostics.forEach(function (diagnostic) {
			var severity = diagnostic.severity === "error" ? "error" : "warning";
			var row = element("div", "vilan-run-diagnostic" + (severity === "warning" ? " vilan-run-warning" : ""));
			row.appendChild(element("span", "vilan-run-severity", severity));
			row.appendChild(
				element(
					"span",
					"vilan-run-site",
					" " + diagnostic.file + ":" + (diagnostic.line + 1) + ":" + (diagnostic.column + 1) + " ",
				),
			);
			row.appendChild(document.createTextNode(diagnostic.message));
			if (diagnostic.note) {
				row.appendChild(element("div", "vilan-run-note", "  note: " + diagnostic.note));
			}
			panel.diagnostics.appendChild(row);
		});
	}

	function mount(panel, js, css) {
		var frame = document.createElement("iframe");
		frame.setAttribute("sandbox", "allow-scripts");
		frame.setAttribute("title", "Program result");
		frame.style.height = RESULT_MAX_HEIGHT + "px";
		frame.srcdoc = buildSrcdoc(js, css);
		// `load` fires once the module has run; a program that printed
		// nothing by then gets told so, and the first line replaces the note.
		frame.addEventListener("load", function () {
			if (panel.frame === frame && panel.lines === 0) {
				panel.console.appendChild(element("div", "vilan-run-quiet", "No console output."));
			}
		});
		panel.frame = frame;
		panel.result.appendChild(frame);
	}

	// A runaway program can print forever; past this many rows the pane
	// notes the truncation once and drops the rest. Each run starts a fresh
	// count.
	function appendLine(panel, kind, text) {
		if (panel.lines === 0) {
			panel.console.textContent = "";
		}
		if (panel.lines > CONSOLE_CAP) {
			return;
		}
		panel.lines += 1;
		var truncated = panel.lines > CONSOLE_CAP;
		panel.console.appendChild(
			element(
				"div",
				"vilan-run-line" + (kind === "error" || truncated ? " vilan-run-error" : ""),
				truncated ? "[output truncated]" : text,
			),
		);
	}

	function resize(panel, height) {
		var clamped = Math.max(0, Math.min(Math.floor(Number(height) || 0), RESULT_MAX_HEIGHT));
		panel.result.style.height = clamped + "px";
		panel.result.classList.toggle("vilan-run-open", clamped > 0);
	}

	// The frames talk to the page with postMessage; the sender's window
	// names the panel. Anything else that posts here is not ours.
	window.addEventListener("message", function (event) {
		var message = event.data;
		if (!message || typeof message.kind !== "string") {
			return;
		}
		for (var i = 0; i < panels.length; i++) {
			var panel = panels[i];
			if (!panel.frame || panel.frame.contentWindow !== event.source) {
				continue;
			}
			if (message.kind === "log" || message.kind === "error") {
				appendLine(panel, message.kind, String(message.text));
			} else if (message.kind === "size") {
				resize(panel, message.height);
			}
			return;
		}
	});

	function settle(panel, message, node) {
		if (message.kind === "crash") {
			setState(
				panel,
				"crashed",
				"The compiler crashed on this input; it has been restarted. Please report the program that did it.",
			);
			return;
		}
		if (message.kind === "unavailable") {
			setState(panel, "unavailable", message.error);
			return;
		}
		renderDiagnostics(panel, message.diagnostics);
		if (!message.ok) {
			setState(panel, "failed", "Build failed; see the diagnostics.");
			return;
		}
		if (node) {
			// Server results are checks: the browser has no process host, so
			// nothing is mounted.
			setState(
				panel,
				"checked",
				"Server program checks clean (vilan " + message.version + "); the browser has no process host to run it.",
			);
			return;
		}
		setState(panel, "ran", "Compiled (vilan " + message.version + ")");
		mount(panel, message.js, message.css);
	}

	function panelFor(pre) {
		for (var i = 0; i < panels.length; i++) {
			if (panels[i].pre === pre) {
				reset(panels[i]);
				return panels[i];
			}
		}
		return createPanel(pre);
	}

	// One fence, one button, one run at a time: the button is disabled until
	// its latest job settles, and a job whose panel was closed meanwhile
	// settles into nothing (a closed panel's frame is detached, so nothing
	// ever runs in it).
	function run(pre, button, source, node) {
		var panel = panelFor(pre);
		setState(panel, "queued", ready ? "Compiling…" : "Loading the compiler…");
		button.disabled = true;
		compile({
			source: source,
			platform: node ? "node" : "browser",
			start: function () {
				setState(panel, "compiling", "Compiling…");
			},
			settle: function (message) {
				button.disabled = false;
				if (panel.root.isConnected) {
					settle(panel, message, node);
				}
			},
		});
	}

	// --- the controls, per fence ---------------------------------------------

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
		var pre = block.parentElement;
		var buttons = pre.querySelector(".buttons");
		if (!buttons) {
			buttons = document.createElement("div");
			buttons.className = "buttons";
			pre.insertBefore(buttons, pre.firstChild);
		}
		var button = null;
		if (canRun) {
			button = document.createElement("button");
			button.type = "button";
			button.className = "vilan-run";
			button.textContent = node ? "Check" : "Run";
			button.title = node
				? "Check this server program here (the browser cannot run it)"
				: "Run this example here";
			button.setAttribute("aria-label", button.title);
			button.addEventListener("click", function () {
				run(pre, button, source, node);
			});
			buttons.insertBefore(button, buttons.firstChild);
		}
		if (canLink) {
			deflate(source).then(function (bytes) {
				var link = document.createElement("a");
				link.className = "vilan-playground-link";
				link.href =
					PLAYGROUND + "#code=" + encodeBase64Url(bytes) + (node ? "&mode=node" : "");
				link.target = "_blank";
				link.rel = "noopener";
				link.title = node
					? "Open in the vilan playground (server leg)"
					: "Open in the vilan playground";
				link.setAttribute("aria-label", link.title);
				link.textContent = "▶";
				// Run, then the playground, then mdBook's own copy button.
				buttons.insertBefore(link, button ? button.nextSibling : buttons.firstChild);
			});
		}
	});
})();
