//! Route-chunk emission (proposal/bundle-splitting.md S2) — the gates for
//! `[entry.<name>] split = true`.
//!
//! `tests/split/project` is a router-shaped browser entry that opts in, and
//! `tests/split/golden` holds its emitted artifacts byte-for-byte: the eager
//! bundle, one file per route arm, and the `chunks.json` manifest. Four things
//! are pinned here, each a rule the partition makes rather than a shape of the
//! output:
//!
//!   1. the artifacts are byte-identical to the goldens (the corpus's bar, over
//!      a multi-file emission the corpus gate cannot reach);
//!   2. splitting moves the route-exclusive functions and NOTHING else — the
//!      shared page helper and every module binding stay eager, which is what
//!      keeps B33's dependency-ordered initialization whole;
//!   3. the split bundle RUNS: initializers fire in dependency order, the
//!      previous view holds while a chunk is in flight, and an arm never
//!      navigated to is never fetched;
//!   4. `--print-chunks` names exactly the chunks the emitter wrote — the plan
//!      and the artifacts pinned against each other, not both against prose.
//!
//! Regenerating a golden is the corpus ritual (AGENTS.md): rebuild the debug
//! binary, build `tests/split/project` by hand, and copy the artifacts over
//! after reading the diff.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The emitted artifacts, in the order the golden directory holds them.
const ARTIFACTS: &[&str] = &[
    "app.js",
    "app.Route_Home.js",
    "app.Route_Docs.js",
    "app.Route_NotFound.js",
    "app.chunks.json",
];

/// The fixture's module bindings, in the order their DEPENDENCIES require —
/// which is the reverse of the order `app.vl` declares them in.
const BINDINGS_IN_INITIALIZATION_ORDER: &[&str] = &["BASE", "SCALED", "LABEL"];

fn fixture(part: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/split")
        .join(part)
}

/// Copies the fixture project into a fresh temp directory. `split` decides
/// whether the manifest keeps its `split = true` line, so the same sources can
/// be built both ways and compared.
fn stage(tag: &str, split: bool) -> PathBuf {
    let staged = std::env::temp_dir().join(format!("vilan_split_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("create the staging directory");
    for entry in std::fs::read_dir(fixture("project")).expect("read the fixture") {
        let entry = entry.expect("a fixture entry");
        let mut text = std::fs::read_to_string(entry.path()).expect("read a fixture file");
        if !split && entry.file_name() == "vilan.toml" {
            text = text
                .lines()
                .filter(|line| !line.starts_with("split"))
                .map(|line| format!("{line}\n"))
                .collect();
        }
        std::fs::write(staged.join(entry.file_name()), text).expect("stage a fixture file");
    }
    staged
}

fn build(staged: &Path, extra: &[&str]) -> String {
    let mut arguments = vec!["build", staged.to_str().expect("utf-8 temp path")];
    arguments.extend_from_slice(extra);
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(&arguments)
        .output()
        .expect("run vilan build");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "vilan build failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

fn read(staged: &Path, name: &str) -> String {
    std::fs::read_to_string(staged.join(name))
        .unwrap_or_else(|error| panic!("read {name}: {error}"))
}

/// Byte-granular, like the corpus gate's — a line-wise diff hides a CRLF drift
/// (`windows-support.md` §3), which is the whole point of a byte pin.
fn first_difference(golden: &str, rebuilt: &str) -> String {
    let (golden_bytes, rebuilt_bytes) = (golden.as_bytes(), rebuilt.as_bytes());
    let at = golden_bytes
        .iter()
        .zip(rebuilt_bytes)
        .position(|(a, b)| a != b);
    match at {
        None => format!(
            "lengths differ (golden {} bytes, rebuilt {})",
            golden_bytes.len(),
            rebuilt_bytes.len()
        ),
        Some(at) => {
            let line = golden[..at].matches('\n').count() + 1;
            let window = |text: &str| {
                let start = at.saturating_sub(40);
                let end = (at + 40).min(text.len());
                text.get(start..end).unwrap_or("").to_string()
            };
            format!(
                "byte {at} (line {line}) differs\n  golden:  …{}…\n  rebuilt: …{}…",
                window(golden),
                window(rebuilt)
            )
        }
    }
}

/// The top-level declarations of an emitted file, in order — the seam the B33
/// invariant is read off. A `const X = …` at column 0 is a module binding (or a
/// chunk's registry read, which is why the chunk side asserts on absence).
fn top_level_consts(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| line.strip_prefix("const "))
        .filter_map(|rest| rest.split_once(" =").map(|(name, _)| name.to_string()))
        .collect()
}

#[test]
fn the_split_fixture_emits_its_pinned_artifacts() {
    let staged = stage("golden", true);
    build(&staged, &[]);

    let mut failures = Vec::new();
    for artifact in ARTIFACTS {
        let golden = std::fs::read_to_string(fixture("golden").join(artifact))
            .unwrap_or_else(|error| panic!("read the golden {artifact}: {error}"));
        let rebuilt = read(&staged, artifact);
        if golden != rebuilt {
            failures.push(format!(
                "{artifact}: {}",
                first_difference(&golden, &rebuilt)
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "the split emission drifted from its goldens:\n{}",
        failures.join("\n")
    );

    // A golden that stopped covering the interesting artifacts would pass
    // silently, so the fixture's own shape is asserted too.
    let home = read(&staged, "app.Route_Home.js");
    assert!(
        home.contains("function home_page(") && home.contains("__vilan_chunks.fn.home_page ="),
        "a chunk declares its arm's functions and registers them: {home}"
    );
    assert!(
        home.contains("const LABEL = __vilan_chunks.fn.LABEL;"),
        "a chunk reads the eager scope — a module binding included — \
         through the registry: {home}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn splitting_moves_the_route_chunks_and_nothing_else() {
    let split = stage("moved", true);
    build(&split, &[]);
    let single = stage("whole", false);
    build(&single, &[]);

    // Without the flag there is one artifact, and it is whole.
    assert!(
        !single.join("app.Route_Home.js").exists() && !single.join("app.chunks.json").exists(),
        "single-file emission is the default: no flag, no chunk artifacts"
    );
    let whole = read(&single, "app.js");
    for page in ["home_page", "docs_page", "docs_nav", "not_found_page"] {
        assert!(
            whole.contains(&format!("function {page}(")),
            "{page} belongs in the single-file bundle"
        );
    }

    // With it, exactly the route-exclusive pages leave — as declarations. The
    // eager bundle keeps a forwarder of the same name, so every call site the
    // route match emitted is unchanged.
    let eager = read(&split, "app.js");
    let chunks: String = [
        "app.Route_Home.js",
        "app.Route_Docs.js",
        "app.Route_NotFound.js",
    ]
    .iter()
    .map(|name| read(&split, name))
    .collect();
    for page in ["home_page", "docs_page", "docs_nav", "not_found_page"] {
        assert!(
            chunks.contains(&format!("function {page}(")),
            "{page} is reachable from exactly one arm, so it is chunked"
        );
    }
    // What the eager bundle keeps of an arm's entry point is a FORWARDER — the
    // same name and parameters, one hop through the registry — so the route
    // match's call sites are emitted exactly as they always were. A page body
    // left behind here is a leak, and this is what catches it.
    for page in ["home_page", "docs_page", "not_found_page"] {
        let start = eager
            .find(&format!("function {page}("))
            .unwrap_or_else(|| panic!("{page} keeps its name in the eager bundle"));
        let rest = &eager[start..];
        let body = &rest[..rest.find("\n}").map(|end| end + 2).unwrap_or(rest.len())];
        assert_eq!(
            body.lines().count(),
            3,
            "the eager `{page}` must be a one-hop forwarder, not the page:\n{body}"
        );
        assert!(
            body.contains(&format!("return __vilan_chunks.fn.{page}(")),
            "the eager `{page}` must forward to the registry:\n{body}"
        );
    }
    // A helper only its own chunk calls is named nowhere eager, so it gets no
    // forwarder at all.
    assert!(
        !eager.contains("function docs_nav("),
        "a chunk-private helper needs no eager stand-in"
    );

    // Shared code goes eager (v1 extracts no sibling chunk).
    assert!(
        eager.contains("function panel("),
        "a helper all three arms reach stays eager"
    );
    assert!(
        !chunks.contains("function panel("),
        "and is not duplicated into any chunk"
    );

    // B33: every module binding stays in the eager bundle, in the order its
    // dependencies force — the same order, and the same place, as the
    // single-file build (b33-emission-order.md §1).
    let eager_consts = top_level_consts(&eager);
    let whole_consts = top_level_consts(&whole);
    let bindings = |consts: &[String]| -> Vec<String> {
        consts
            .iter()
            .filter(|name| BINDINGS_IN_INITIALIZATION_ORDER.contains(&name.as_str()))
            .cloned()
            .collect()
    };
    assert_eq!(
        bindings(&eager_consts),
        BINDINGS_IN_INITIALIZATION_ORDER,
        "the split bundle's module bindings must initialize in dependency order"
    );
    assert_eq!(
        bindings(&eager_consts),
        bindings(&whole_consts),
        "splitting must not reorder module initialization"
    );
    for binding in BINDINGS_IN_INITIALIZATION_ORDER {
        assert!(
            !chunks.contains(&format!("const {binding} = announce"))
                && !chunks.contains(&format!("const {binding} = \"scale")),
            "{binding}'s initializer must not move into a chunk"
        );
    }

    let _ = std::fs::remove_dir_all(&split);
    let _ = std::fs::remove_dir_all(&single);
}

#[test]
fn a_split_bundle_runs_its_routes_and_fetches_one_chunk_at_a_time() {
    let staged = stage("run", true);
    build(&staged, &[]);
    std::fs::write(staged.join("harness.js"), HARNESS).expect("write the harness");

    let output = Command::new("node")
        .arg("harness.js")
        .current_dir(&staged)
        .output()
        .expect("run the node harness");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the split harness failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        stdout,
        // B33 (b33-emission-order.md §1): `BASE` before `SCALED`, though
        // `app.vl` declares them the other way round. Then: nothing rendered
        // before the boot route's chunk lands; the home page once it does; the
        // PREVIOUS view still on screen while the docs chunk is in flight
        // (bundle-splitting.md §2); the docs page once it lands. `NotFound` is
        // never navigated to, so it is never fetched. `<p>...</p>` is
        // `router::pending()`, live through both fetches.
        "init BASE=2\n\
         init SCALED=6\n\
         boot <main><nav><a>Home</a><a>Docs</a></nav><p>...</p></main>\n\
         home <main><nav><a>Home</a><a>Docs</a></nav><p></p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         fetching <main><nav><a>Home</a><a>Docs</a></nav><p>...</p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         docs <main><nav><a>Home</a><a>Docs</a></nav><p></p><article><section><h2>Docs</h2><p>page 3</p></section><nav><a>Next</a></nav></article></main>\n\
         fetched Route_Docs,Route_Home\n",
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn the_plan_and_the_emitted_chunks_agree() {
    let staged = stage("agree", true);
    let report = build(&staged, &["--print-chunks"]);

    // The plan's arms, straight out of the report.
    let planned: BTreeSet<String> = report
        .lines()
        .filter_map(|line| line.trim().strip_prefix("chunk `"))
        .filter_map(|rest| rest.split_once('`').map(|(arm, _)| arm.to_string()))
        .collect();
    assert!(
        !planned.is_empty(),
        "the fixture must plan chunks: {report}"
    );

    // The artifacts' arms, straight out of the manifest the emitter wrote.
    let manifest = read(&staged, "app.chunks.json");
    let emitted: BTreeSet<String> = manifest
        .lines()
        .filter_map(|line| line.split_once("\"arm\": \""))
        .filter_map(|(_, rest)| rest.split_once('"').map(|(arm, _)| arm.to_string()))
        .collect();
    assert_eq!(
        planned, emitted,
        "`--print-chunks` and the emitted chunk map must name the same arms\n\
         report:\n{report}\nmanifest:\n{manifest}"
    );

    // …and every named file is on disk, per arm.
    for arm in &emitted {
        let file = manifest
            .lines()
            .find(|line| line.contains(&format!("\"arm\": \"{arm}\"")))
            .and_then(|line| line.split_once("\"file\": \""))
            .and_then(|(_, rest)| rest.split_once('"').map(|(file, _)| file.to_string()))
            .unwrap_or_else(|| panic!("{arm} has no file in {manifest}"));
        assert!(
            staged.join(&file).is_file(),
            "the manifest names {file} for {arm}, which was not emitted"
        );
    }

    // The per-function memberships agree too, so a chunk that planned three
    // functions and emitted two would not slip through.
    let functions_of = |line: &str| -> BTreeSet<String> {
        line.rsplit_once('(')
            .map(|(_, list)| {
                list.trim_end_matches(&[')', '\n'][..])
                    .split(", ")
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default()
    };
    for line in report.lines().filter(|line| line.contains("chunk `")) {
        let arm = line
            .trim()
            .strip_prefix("chunk `")
            .and_then(|rest| rest.split_once('`').map(|(arm, _)| arm))
            .expect("an arm");
        let file = vilan_core::chunks::chunk_file_name("app", arm);
        let source = read(&staged, &file);
        for function in functions_of(line) {
            assert!(
                source.contains(&format!("function {function}(")),
                "the plan puts {function} in {arm}, but {file} does not declare it"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn split_off_a_browser_leg_stops_the_build() {
    // The manifest's refusal reaching a real `vilan build`: a key that cannot
    // apply is a build error naming the leg, not a silently ignored line.
    let staged = stage("refused", true);
    std::fs::write(
        staged.join("vilan.toml"),
        "[package]\nname = \"split_fixture\"\nroot = \".\"\nentry = \"app.vl\"\nsplit = true\n",
    )
    .expect("write the manifest");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", staged.to_str().expect("utf-8 temp path")])
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan build");
    assert!(
        !output.status.success(),
        "a `split` outside a browser leg must not build"
    );
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("`[package] split`")
            && text.contains("`browser` leg only")
            && text.contains("targets `node`"),
        "the refusal must name the key and the leg it found: {text}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

/// The DOM/history stub the split bundle runs against — `router.rs`'s, plus a
/// settle step, because a chunk arrives on a microtask rather than inline.
/// Node resolves the chunks' relative `import()` against the importing file, so
/// they load off disk exactly as a browser would load them off the origin.
const HARNESS: &str = r#"class StubElement {
	constructor(tagName) {
		this.tagName = tagName;
		this.children = [];
		this.parent = null;
		this.listeners = {};
		this._text = "";
		this.className = "";
		this.attributes = {};
		this.style = { setProperty() {} };
	}
	get textContent() { return this._text; }
	set textContent(value) { this._text = value; this.children = []; }
	setAttribute(name, value) { this.attributes[name] = value; }
	appendChild(child) {
		if (child.parent) {
			child.parent.children = child.parent.children.filter((c) => c !== child);
		}
		child.parent = this;
		this.children.push(child);
		return child;
	}
	remove() {
		if (this.parent) {
			this.parent.children = this.parent.children.filter((c) => c !== this);
		}
		this.parent = null;
	}
	replaceChildren() { this.children = []; }
	addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler); }
	render() {
		const inner = this.children.map((child) => child.render()).join("");
		return `<${this.tagName}>${this._text}${inner}</${this.tagName}>`;
	}
}

const root = new StubElement("div");
global.document = {
	createElement: (tag) => new StubElement(tag),
	createElementNS: (namespace, tag) => new StubElement(tag),
	getElementById: (id) => (id === "app" ? root : null),
	querySelector: () => null,
	querySelectorAll: () => [],
};
global.location = { pathname: "/" };
global.history = { pushState(state, title, path) { global.location.pathname = path; } };
const popstate = [];
global.window = { addEventListener: (event, handler) => { if (event === "popstate") popstate.push(handler); } };

require("./app.js");

const page = () => root.children.map((child) => child.render()).join("");
const settle = () => new Promise((resolve) => setTimeout(resolve, 50));
const chunks = globalThis.__vilan_chunks;

(async () => {
	// Nothing is rendered before the boot route's own code has landed, and
	// `router::pending()` — bound to the `<p>` — reads busy meanwhile.
	console.log("boot", page());
	await settle();
	console.log("home", page());

	// Navigating: the signal does not advance until the chunk does, so the
	// PREVIOUS page is what is on screen — the whole loading story.
	global.location.pathname = "/docs/3";
	for (const handler of popstate) handler({});
	console.log("fetching", page());
	await settle();
	console.log("docs", page());

	// Only what was navigated to was ever fetched.
	const fetched = Object.keys(chunks.loaded)
		.map((arm) => chunks.url[arm].replace("app.", "").replace(".js", ""))
		.sort()
		.join(",");
	console.log("fetched", fetched);
})();
"#;
