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
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

mod support;

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

/// How many times a source reads a registry slot AT A USE —
/// `__vilan_chunks.fn.docs_nav(…)` rather than the preamble's
/// `const docs_nav = __vilan_chunks.fn.docs_nav;` or the tail's
/// `__vilan_chunks.fn.docs_nav = docs_nav;`. That is the form a reference to
/// another CHUNK's function takes (M20, `bundle-boundaries.md` §4.1), and this
/// counts its cost: one property lookup per occurrence.
fn call_site_registry_reads(source: &str) -> usize {
    source
        .split("__vilan_chunks.fn.")
        .skip(1)
        .filter(|tail| {
            let end = tail
                .find(|character: char| {
                    !character.is_alphanumeric() && character != '_' && character != '$'
                })
                .unwrap_or(tail.len());
            tail[end..].starts_with('(')
        })
        .count()
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

    // M20 (`bundle-boundaries.md` §1.6 fact 2, D5): a chunk's every non-std
    // dependency is EAGER under the route partition, so its snapshot is sound
    // and it pays no property read at a call. The emitter reads a name at the
    // USE only when a sibling CHUNK owns it, which this partition cannot
    // produce — so the count here is 0, and that zero is why the reference-form
    // rule is latent on every plan v1 can make. The eager bundle's forwarders
    // are the same read and are counted in `app.js`, deliberately not here.
    for artifact in ARTIFACTS
        .iter()
        .filter(|name| name.starts_with("app.Route_"))
    {
        let chunk = read(&staged, artifact);
        assert_eq!(
            call_site_registry_reads(&chunk),
            0,
            "{artifact} must reach nothing but the eager scope, which it \
             snapshots once: {chunk}"
        );
    }
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn splitting_moves_the_route_chunks_and_nothing_else() {
    let split = stage("moved", true);
    build(&split, &[]);
    let single = stage("whole", false);
    build(&single, &[]);

    // Without the flag there is one bundle, and it is whole: no chunk FILES.
    // The manifest still lands — since `fullstack-dx.md` §10.3 it is the leg's
    // BUILD manifest, written on every build of a browser leg — and says so
    // positively, with an empty chunk list.
    assert!(
        !single.join("app.Route_Home.js").exists(),
        "single-file emission is the default: no flag, no chunk files"
    );
    let manifest = read(&single, "app.chunks.json");
    assert!(
        manifest.contains("\"chunks\": []") && manifest.contains("\"classic_script\": false"),
        "a leg that does not split writes a manifest that says so: {manifest}"
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

/// Writes the shared DOM stub plus `driver` beside a staged build and runs it
/// under node, returning its stdout. Node resolves the chunks' relative
/// `import()` against the importing file, so they load off disk exactly as a
/// browser would load them off the origin.
fn run_under_node(staged: &Path, driver: &str) -> String {
    std::fs::write(staged.join("stub.js"), STUB).expect("write the DOM stub");
    std::fs::write(staged.join("harness.js"), driver).expect("write the harness");
    let output = Command::new("node")
        .arg("harness.js")
        .current_dir(staged)
        .output()
        .expect("run the node harness");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(
        output.status.success(),
        "the split harness failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

#[test]
fn a_split_bundle_runs_its_routes_and_fetches_one_chunk_at_a_time() {
    let staged = stage("run", true);
    build(&staged, &[]);
    let stdout = run_under_node(
        &staged,
        r#"const stub = require("./stub.js");
require("./app.js");
const chunks = globalThis.__vilan_chunks;

(async () => {
	// The BOOT PRELOAD (bundle-splitting.md §S3): the emitter plants
	// `chunk_preload(route)` ahead of the statement that mounts the swap, so the
	// boot route's chunk is on the wire before the first element of the shell is
	// created. Without it the whole shell is built first and the fetch trails it.
	console.log("preloaded-before-the-shell", stub.first_element_saw_a_fetch());
	console.log("boot", stub.page());
	// Anchored on the boot chunk's own render: `import()` cannot resolve within
	// this turn, so the line above always sees the placeholder, and the line
	// below always sees the section — however long the fetch takes.
	await stub.rendered("scale 6");
	console.log("home", stub.page());

	// Navigating: the signal does not advance until the chunk does, so the
	// PREVIOUS page is what is on screen — the whole loading story.
	stub.go("/docs/3");
	console.log("fetching", stub.page());
	await stub.rendered("page 3");
	console.log("docs", stub.page());

	// Only what was navigated to was ever fetched.
	const fetched = Object.keys(chunks.loaded)
		.map((arm) => chunks.url[arm].replace("app.", "").replace(".js", ""))
		.sort()
		.join(",");
	console.log("fetched", fetched);
})();
"#,
    );
    assert_eq!(
        stdout,
        // B33 (b33-emission-order.md §1): `BASE` before `SCALED`, though
        // `app.vl` declares them the other way round. Then: the boot chunk
        // already fetching before the shell exists; nothing rendered before it
        // lands; the home page once it does; the PREVIOUS view still on screen
        // while the docs chunk is in flight (bundle-splitting.md §2); the docs
        // page once it lands. `NotFound` is never navigated to, so it is never
        // fetched. The first `<p>...</p>` is `router::pending()`, live through
        // both fetches; the second `<p>` is `router::chunk_error()`, empty
        // throughout because nothing failed.
        "init BASE=2\n\
         init SCALED=6\n\
         preloaded-before-the-shell true\n\
         boot <main><nav><a>Home</a><a>Docs</a></nav><p>...</p><p></p></main>\n\
         home <main><nav><a>Home</a><a>Docs</a></nav><p></p><p></p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         fetching <main><nav><a>Home</a><a>Docs</a></nav><p>...</p><p></p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         docs <main><nav><a>Home</a><a>Docs</a></nav><p></p><p></p><article><section><h2>Docs</h2><p>page 3</p></section><nav><a>Next</a></nav></article></main>\n\
         fetched Route_Docs,Route_Home\n",
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn a_failed_chunk_fetch_surfaces_and_the_next_navigation_retries() {
    // The error hook (bundle-splitting.md §S3). The fetch is made controllable
    // by pointing an arm's entry in the embedded map at a file that is not
    // there — the same failure a 404 on the origin produces, and the only knob
    // needed to drive it.
    let staged = stage("failure", true);
    build(&staged, &[]);
    let stdout = run_under_node(
        &staged,
        r#"const stub = require("./stub.js");
require("./app.js");
const chunks = globalThis.__vilan_chunks;

(async () => {
	await stub.rendered("scale 6");
	console.log("home", stub.page());

	const real = chunks.url[2];
	chunks.url[2] = "app.Route_Missing.js";
	stub.go("/nope");
	// The failure's own render is the anchor: `chunk_error()` is written on the
	// rejection path, strictly after the registry has dropped the in-flight
	// entry, so observing `!` also puts the `still-pending` read behind the
	// delete rather than beside it.
	await stub.rendered("<p>!</p>");
	// The navigation did not happen: the previous page is still on screen, the
	// pending flag is back down (it must not stick), and the reason reached the
	// app — `!` renders only for a non-empty message.
	console.log("failed", stub.page());
	console.log("still-pending", Object.keys(chunks.pending).length > 0);

	// A failed fetch is not remembered as in flight, so the next navigation to
	// the same arm refetches — a retry is a link click, not an API.
	chunks.url[2] = real;
	stub.go("/nope");
	await stub.rendered("Nothing here");
	console.log("retried", stub.page());
})();
"#,
    );
    assert_eq!(
        stdout,
        "init BASE=2\n\
         init SCALED=6\n\
         home <main><nav><a>Home</a><a>Docs</a></nav><p></p><p></p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         failed <main><nav><a>Home</a><a>Docs</a></nav><p></p><p>!</p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         still-pending false\n\
         retried <main><nav><a>Home</a><a>Docs</a></nav><p></p><p></p><section><h2>Nothing here</h2><p>try /docs/1</p></section></main>\n",
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn a_chunk_that_lands_after_a_later_navigation_does_not_swap() {
    // Overlapping navigations resolve by GENERATION, not by arrival
    // (bundle-splitting.md §S3, `Draft::push`'s guard). The in-flight fetch is
    // made controllable by seeding the registry's pending slot with a promise
    // the harness resolves by hand — `__chunk_load` joins an existing one
    // rather than opening a second, so this IS the arm's fetch.
    let staged = stage("generation", true);
    build(&staged, &[]);
    let stdout = run_under_node(
        &staged,
        r#"const stub = require("./stub.js");
require("./app.js");
const chunks = globalThis.__vilan_chunks;

(async () => {
	await stub.rendered("scale 6");
	stub.go("/docs/1");
	await stub.rendered("page 1");
	stub.go("/");
	await stub.rendered("scale 6");
	console.log("home", stub.page());

	// NotFound's chunk is the slow one, and its arrival is the harness's to
	// choose. Landing it registers the arm's functions for real, so the stale
	// arrival below could genuinely render if nothing stopped it.
	let land;
	chunks.pending[2] = new Promise((resolve) => {
		land = async () => {
			await import("./app.Route_NotFound.js");
			chunks.loaded[2] = true;
			resolve();
		};
	});

	stub.go("/nope");
	console.log("in-flight", stub.page());

	// A second navigation, to code that is already here, wins immediately —
	// and ends the wait, since the fetch it supersedes can no longer land.
	stub.go("/docs/2");
	await stub.rendered("page 2");
	console.log("superseded", stub.page());

	// …and when the superseded chunk finally arrives, it must NOT swap. The
	// window this negative needs is opened by an event the harness itself
	// causes — `land()` resolving the arm's in-flight promise, which queues the
	// app's continuation — and closed by draining turns until the DOM stops
	// changing. Nothing here waits for a duration, so load cannot shorten it.
	await land();
	await stub.quiet();
	console.log("stale-arrival", stub.page());
})();
"#,
    );
    assert_eq!(
        stdout,
        "init BASE=2\n\
         init SCALED=6\n\
         home <main><nav><a>Home</a><a>Docs</a></nav><p></p><p></p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         in-flight <main><nav><a>Home</a><a>Docs</a></nav><p>...</p><p></p><section><h2>Home</h2><p>scale 6</p></section></main>\n\
         superseded <main><nav><a>Home</a><a>Docs</a></nav><p></p><p></p><article><section><h2>Docs</h2><p>page 2</p></section><nav><a>Next</a></nav></article></main>\n\
         stale-arrival <main><nav><a>Home</a><a>Docs</a></nav><p></p><p></p><article><section><h2>Docs</h2><p>page 2</p></section><nav><a>Next</a></nav></article></main>\n",
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
fn a_build_owns_its_legs_chunk_namespace() {
    // `bundle-splitting.md` §S3, item 4. A leg's chunk files and its manifest
    // belong to its LAST build: a renamed route arm must not leave the old
    // arm's file beside the new one, and dropping `split` must not leave a
    // manifest describing chunks the bundle no longer names. Since
    // `fullstack-dx.md` §10.3 the second half is stronger, not weaker: the
    // manifest is REWRITTEN with an empty chunk list rather than removed, which
    // is a positive statement where an absent file was an ambiguity between
    // "did not split" and "was never built".
    let staged = stage("namespace", true);
    build(&staged, &[]);
    assert!(
        staged.join("app.Route_Docs.js").is_file() && staged.join("app.chunks.json").is_file(),
        "the split build writes the docs chunk and the manifest"
    );

    // Rename the arm. The chunk file is named after the arm, so the old one is
    // now a stray — inert, but a stray the manifest no longer lists.
    let source = read(&staged, "app.vl")
        .replace("Docs(i32)", "Guide(i32)")
        .replace("Route::Docs", "Route::Guide")
        .replace("docs_page", "guide_page")
        .replace("docs_nav", "guide_nav");
    std::fs::write(staged.join("app.vl"), source).expect("rewrite the fixture");
    build(&staged, &[]);
    assert!(
        staged.join("app.Route_Guide.js").is_file(),
        "the renamed arm gets its own chunk"
    );
    assert!(
        !staged.join("app.Route_Docs.js").exists(),
        "the renamed arm's previous chunk file must be swept"
    );
    // …and the bundle itself is never mistaken for one of its chunks.
    assert!(staged.join("app.js").is_file(), "the eager bundle survives");

    // Dropping `split` takes the whole namespace with it.
    let manifest = read(&staged, "vilan.toml")
        .lines()
        .filter(|line| !line.starts_with("split"))
        .map(|line| format!("{line}\n"))
        .collect::<String>();
    std::fs::write(staged.join("vilan.toml"), manifest).expect("rewrite the manifest");
    build(&staged, &[]);
    let left: Vec<String> = std::fs::read_dir(&staged)
        .expect("read the staged directory")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with("app.")
                && name != "app.js"
                && name != "app.vl"
                && name != "app.chunks.json"
        })
        .collect();
    assert!(
        left.is_empty(),
        "a build with no chunks must leave no chunk FILE behind: {left:?}"
    );
    let manifest = read(&staged, "app.chunks.json");
    assert!(
        manifest.contains("\"chunks\": []") && !manifest.contains("Route_"),
        "the manifest survives the drop, describing a leg that no longer splits: {manifest}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
fn a_split_build_warns_when_the_gate_costs_more_than_it_defers() {
    // `bundle-splitting.md` §S3, item 5. Splitting is not free — below a few KB
    // of per-route code the gate, the forwarders and the chunk map cost more
    // than the deferred mass saves — and the build says so with THIS leg's
    // numbers, measured against the same entry emitted whole.
    let staged = stage("cost", true);
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", staged.to_str().expect("utf-8 temp path")])
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan build");
    assert!(
        output.status.success(),
        "the warning must not fail the build"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("warning:") && stderr.contains("`split` on `app`"),
        "the fixture's lazy mass is far below the gate's cost; the build must warn:\n{stderr}"
    );
    assert!(
        stderr.contains("adds") && stderr.contains("defers only"),
        "the warning must name the numbers, not just complain:\n{stderr}"
    );

    // The numbers are this build's own: the eager bundle it just wrote, and the
    // chunk files it just wrote.
    let eager = std::fs::metadata(staged.join("app.js"))
        .expect("the eager bundle")
        .len();
    let deferred: u64 = [
        "app.Route_Home.js",
        "app.Route_Docs.js",
        "app.Route_NotFound.js",
    ]
    .iter()
    .map(|name| std::fs::metadata(staged.join(name)).expect("a chunk").len())
    .sum();
    assert!(
        stderr.contains(&format!("defers only {deferred}"))
            && stderr.contains(&format!("{eager} bytes split")),
        "the warning must be measured, not estimated (eager {eager}, deferred {deferred}):\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

/// A RUNNABLE split project: the fixture's client beside a node server that
/// prints and returns. `vilan run` needs a node leg to launch, and the fixture
/// package (browser-only) has none.
fn stage_workspace(tag: &str) -> PathBuf {
    let staged = std::env::temp_dir().join(format!("vilan_split_run_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    std::fs::write(
        staged.join("vilan.toml"),
        "[package]\nname = \"split_run\"\n\n[entry.client]\ntarget = \"browser\"\nsplit = true\n\n[entry.server]\n",
    )
    .expect("write the manifest");
    let client = std::fs::read_to_string(fixture("project").join("app.vl")).expect("the client");
    std::fs::write(staged.join("src/client.vl"), client).expect("write the client");
    std::fs::write(
        staged.join("src/server.vl"),
        "import std::io::print;\n\nfun main() {\n\tprint(\"server-booted\");\n}\n",
    )
    .expect("write the server");
    staged
}

/// The leg's chunk FILES currently on disk. The build manifest is deliberately
/// not one of them: since `fullstack-dx.md` §10.3 `client.chunks.json` is
/// written on every build of the leg, so it can no longer stand in for "this
/// build split" — the manifest's own `chunks` list is what says that, and
/// [`manifest_of`] reads it.
fn chunk_artifacts(dist: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(dist) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| {
            name.starts_with("client.") && name != "client.js" && name != "client.chunks.json"
        })
        .collect();
    names.sort();
    names
}

/// The leg's build manifest, or `None` when this build wrote none.
fn manifest_of(dist: &Path) -> Option<String> {
    std::fs::read_to_string(dist.join("client.chunks.json")).ok()
}

#[test]
fn vilan_run_emits_the_leg_whole_and_clears_the_chunks_a_build_left() {
    // `bundle-splitting.md` §S4, item 6. Splitting is a BUILD optimization and
    // single-file emission is first-class forever: every `run` form emits one
    // file per leg, whatever the manifest declares, because that is the only
    // shape the dev loop's whole-bundle diff-and-swap can classify. Refusing
    // the combination instead would mean a project that ships split could not
    // be developed without editing its manifest.
    let staged = stage_workspace("whole");
    build(&staged, &[]);
    let dist = staged.join("dist");
    assert_eq!(
        chunk_artifacts(&dist),
        vec![
            "client.Route_Docs.js",
            "client.Route_Home.js",
            "client.Route_NotFound.js",
        ],
        "`vilan build` honours `split`"
    );
    assert!(
        manifest_of(&dist)
            .expect("the split build's manifest")
            .contains("client.Route_Home.js"),
        "and the manifest names what it wrote"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", staged.to_str().expect("utf-8 temp path")])
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan run");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() && stdout.contains("server-booted"),
        "the run must build and boot:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("run: `client` emits as one file"),
        "passing over a leg's `split` must be said out loud, once:\n{stderr}"
    );

    // The bundle is whole — the pages are declarations, not forwarders…
    let bundle = read(&dist, "client.js");
    assert!(
        bundle.contains("function home_page(") && !bundle.contains("__vilan_chunks"),
        "a run's bundle carries every route and names no chunk"
    );
    // …and nothing of the previous split build is left describing it. This is
    // what moots the `--watch` stray-chunk residue S2 recorded: a leg's chunk
    // namespace belongs to its last build, and this build had none.
    assert_eq!(
        chunk_artifacts(&dist),
        Vec::<String>::new(),
        "a whole-bundle build must sweep the leg's chunk namespace"
    );
    assert!(
        manifest_of(&dist)
            .expect("the run's manifest")
            .contains("\"chunks\": []"),
        "and its manifest must say the leg emitted none — a server reading it \
         during the dev loop needs the description, not its absence"
    );
    let _ = std::fs::remove_dir_all(&staged);
}

#[test]
#[cfg(unix)]
fn a_watch_round_clears_the_chunks_a_build_left() {
    // The same rule on the HMR path, which writes `dist/` itself rather than
    // going through `build_workspace_artifacts`.
    let staged = stage_workspace("watch");
    build(&staged, &[]);
    let dist = staged.join("dist");
    assert!(
        !chunk_artifacts(&dist).is_empty(),
        "the seed build must leave chunks for the round to clear"
    );

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args([
            "run",
            "--watch",
            "--hmr-port",
            "0",
            staged.to_str().expect("utf-8 temp path"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    // A watch round is a full compile, so this is `support::WATCH_LIVENESS` and
    // not a literal (E40, following E39's sweep of `hmr.rs`): the claim is that
    // the round CLEARS the seed build's chunks, never that it clears them
    // quickly, and the 120 s that stood here was consumed outright on a box
    // running several overlapping suites.
    let deadline = Instant::now() + support::WATCH_LIVENESS;
    let mut cleared = false;
    while Instant::now() < deadline {
        // The manifest is REWRITTEN, not swept: a watch round is exactly when a
        // running server asks what its client leg emitted, so the round that
        // takes the chunks away must leave the description behind saying so.
        let described_no_chunks =
            manifest_of(&dist).is_some_and(|manifest| manifest.contains("\"chunks\": []"));
        if chunk_artifacts(&dist).is_empty()
            && dist.join("client.js").is_file()
            && described_no_chunks
        {
            cleared = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let left = chunk_artifacts(&dist);
    let manifest = manifest_of(&dist).unwrap_or_else(|| "<no manifest>".to_string());
    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&staged);
    assert!(
        cleared,
        "a watch round emits the leg whole, so the previous build's chunks must go \
         and its manifest must say so: {left:?}\n{manifest}"
    );
}

/// Bind an ephemeral port and release it — a free port for the served pin (the
/// standard small TOCTOU window this suite's server tests all take).
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read the bound address")
        .port()
}

fn wait_for_port(port: u16, deadline: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// A plain HTTP GET, returning the response body bytes.
fn http_get(port: u16, path: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect for GET");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set a read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .expect("send GET");
    let mut response = Vec::new();
    let _ = stream.read_to_end(&mut response);
    let separator = b"\r\n\r\n";
    match response
        .windows(separator.len())
        .position(|window| window == separator)
    {
        Some(index) => response[index + separator.len()..].to_vec(),
        None => response,
    }
}

#[test]
fn a_split_builds_chunks_are_servable_through_the_manifest() {
    // `bundle-splitting.md` §S4, item 7. `chunks.json` exists so a server can
    // serve the chunk files without hard-coding a route per file. The blessed
    // way to do that is now `serve_build` (`fullstack-dx.md` §5.4, pinned in
    // `tests/serve_build.rs`, and adopted by `examples/fullstack`); what THIS
    // pins is that the manifest stays a plain, hand-readable JSON contract —
    // rung 0 is not deprecated, and a server that iterates the file itself must
    // keep working (§5.7).
    let port = free_port();
    let staged = stage_workspace("served");
    std::fs::write(
        staged.join("src/server.vl"),
        format!(
            "import std::fs;\n\
             import std::http::{{ Request, Response, Server }};\n\
             import std::json::{{ coerce_str, parse_json_value }};\n\
             import std::option::Option::{{ None, Some, self }};\n\
             import std::io::print;\n\
             \n\
             struct ChunkFile {{\n\
             \tpath: str,\n\
             \tsource: str,\n\
             }}\n\
             \n\
             async fun main() {{\n\
             \tlet client_js = fs::read_file_to_str(\"dist/client.js\");\n\
             \tlet chunks = route_chunks(\"client\");\n\
             \tServer::builder()\n\
             \t\t.port({port})\n\
             \t\t.on_request(|request| match request.path() {{\n\
             \t\t\t\"/client.js\" => Response::builder().set_header(\"Content-Type\", \"text/javascript\").body(client_js).build(),\n\
             \t\t\t_ => match find_chunk(chunks, request.path()) {{\n\
             \t\t\t\tSome(let source) => Response::builder().set_header(\"Content-Type\", \"text/javascript\").body(source).build(),\n\
             \t\t\t\tNone => Response::builder().set_header(\"Content-Type\", \"text/html\").body(\"<div id=\\\"app\\\"></div>\").build(),\n\
             \t\t\t}},\n\
             \t\t}})\n\
             \t\t.on_start(|server| print(\"listening\"))\n\
             \t\t.build()\n\
             \t\t.start();\n\
             }}\n\
             \n\
             fun route_chunks(leg: str): List<ChunkFile> {{\n\
             \tmut files: List<ChunkFile> = [];\n\
             \tlet manifest_path = i\"dist/{{leg}}.chunks.json\";\n\
             \tif fs::stat(manifest_path).is_none() {{\n\
             \t\tret files;\n\
             \t}}\n\
             \tlet manifest = parse_json_value(fs::read_file_to_str(manifest_path));\n\
             \tfor chunk in manifest.field(\"chunks\").elements() {{\n\
             \t\tlet name = coerce_str(chunk.field(\"file\"));\n\
             \t\tfiles.push(ChunkFile {{ path = i\"/{{name}}\", source = fs::read_file_to_str(i\"dist/{{name}}\") }});\n\
             \t}}\n\
             \tfiles\n\
             }}\n\
             \n\
             fun find_chunk(chunks: List<ChunkFile>, path: str): Option<str> {{\n\
             \tfor chunk in chunks {{\n\
             \t\tif chunk.path == path {{\n\
             \t\t\tret Some(chunk.source);\n\
             \t\t}}\n\
             \t}}\n\
             \tNone\n\
             }}\n"
        ),
    )
    .expect("write the manifest-driven server");
    build(&staged, &[]);

    let dist = staged.join("dist");
    let mut server = Command::new("node")
        .arg(Path::new("dist").join("server.mjs"))
        .current_dir(&staged)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the server");

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(
            wait_for_port(port, Duration::from_secs(30)),
            "the server should listen on {port}"
        );
        // Every chunk the build wrote is served, byte for byte, at the path the
        // embedded map will ask for — and the server was told none of their
        // names.
        for arm in ["Route_Home", "Route_Docs", "Route_NotFound"] {
            let file = format!("client.{arm}.js");
            let on_disk = std::fs::read(dist.join(&file)).expect("the chunk on disk");
            let served = http_get(port, &format!("/{file}"));
            assert_eq!(
                served, on_disk,
                "GET /{file} must serve the chunk the build wrote"
            );
        }
        // …and an ordinary path still gets the app shell, not a chunk.
        let shell = http_get(port, "/docs/3");
        assert!(
            String::from_utf8_lossy(&shell).contains("id=\"app\""),
            "a route path still serves the shell"
        );
    }));

    let _ = server.kill();
    let _ = server.wait();
    let _ = std::fs::remove_dir_all(&staged);
    outcome.unwrap();
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
/// `location`/`popstate` driver, one instrument (whether a chunk fetch was
/// already in flight when the FIRST element was created, which is how the boot
/// preload is observed), and the two waits every content assertion here is
/// anchored on.
///
/// The waits are EVENTS, not a clock (E45, E41's shape applied here). A chunk
/// arrives asynchronously — a real `import()` off disk, then a microtask segment
/// that swaps the view — and the wait this replaces was `setTimeout(50)`: on a
/// loaded box the fetch outran it and the assertion sampled the *pending
/// placeholder* where the resolved section belongs, which is exactly the failure
/// E45 recorded across three lanes. `rendered(needle)` instead resolves on the
/// DOM MUTATION that puts `needle` on the page, so a slow box only waits longer,
/// and a render that never comes fails loudly with the page it was left with —
/// it can neither pass vacuously nor fail spuriously.
const STUB: &str = r#"class StubElement {
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
	set textContent(value) { this._text = value; this.children = []; touched(); }
	setAttribute(name, value) { this.attributes[name] = value; touched(); }
	appendChild(child) {
		if (child.parent) {
			child.parent.children = child.parent.children.filter((c) => c !== child);
		}
		child.parent = this;
		this.children.push(child);
		touched();
		return child;
	}
	remove() {
		if (this.parent) {
			this.parent.children = this.parent.children.filter((c) => c !== this);
		}
		this.parent = null;
		touched();
	}
	replaceChildren() { this.children = []; touched(); }
	addEventListener(name, handler) { (this.listeners[name] ||= []).push(handler); }
	render() {
		const inner = this.children.map((child) => child.render()).join("");
		return `<${this.tagName}>${this._text}${inner}</${this.tagName}>`;
	}
}

// Every write to the tree bumps `mutations` and wakes whoever is waiting on the
// next render. This is the observable event a chunk's arrival ends in.
let mutations = 0;
const watchers = [];
const touched = () => {
	mutations += 1;
	for (const watcher of watchers.splice(0)) watcher();
};

const root = new StubElement("div");
let first_element_saw_a_fetch = null;
const fetching = () =>
	globalThis.__vilan_chunks !== undefined &&
	Object.keys(globalThis.__vilan_chunks.pending).length > 0;
global.document = {
	createElement: (tag) => {
		if (first_element_saw_a_fetch === null) first_element_saw_a_fetch = fetching();
		return new StubElement(tag);
	},
	createElementNS: (namespace, tag) => new StubElement(tag),
	getElementById: (id) => (id === "app" ? root : null),
	querySelector: () => null,
	querySelectorAll: () => [],
};
global.location = { pathname: "/" };
global.history = { pushState(state, title, path) { global.location.pathname = path; } };
const popstate = [];
global.window = { addEventListener: (event, handler) => { if (event === "popstate") popstate.push(handler); } };

const page = () => root.children.map((child) => child.render()).join("");
// One turn of the loop: `setImmediate` runs after every microtask queued so
// far, and reactive's continuation segments settle on microtasks
// (`std/reactive.vl`), so a turn boundary drains the whole render a resolved
// chunk schedules. A turn is not a duration — this waits for the queue, not for
// the clock.
const turn = () => new Promise((resolve) => setImmediate(resolve));

module.exports = {
	page,
	// Waits for the render that puts `needle` on the page. Returns after the
	// mutation that lands it PLUS one turn, so the surrounding synchronous
	// render batch and any microtask that follows it are complete before the
	// page is sampled. The deadline is a failure mode, not the wait: nothing
	// here passes because it expired.
	rendered: (needle, deadline_ms = 30000) =>
		new Promise((resolve, reject) => {
			const done = () => turn().then(resolve);
			if (page().includes(needle)) return done();
			const timer = setTimeout(() => {
				reject(new Error(
					`the render carrying ${JSON.stringify(needle)} never arrived within ` +
					`${deadline_ms}ms; the page is ${JSON.stringify(page())}`,
				));
			}, deadline_ms);
			const watcher = () => {
				if (!page().includes(needle)) return watchers.push(watcher);
				clearTimeout(timer);
				done();
			};
			watchers.push(watcher);
		}),
	// For an assertion that the page must NOT change: drains turns until one
	// passes with no mutation at all. Used only after the event whose effect is
	// being denied has already been observed (a chunk that landed by the
	// harness's own hand), so this closes a window that is already open rather
	// than standing in for the arrival itself.
	quiet: async (turns = 3) => {
		for (let index = 0; index < turns; index += 1) {
			const before = mutations;
			await turn();
			if (mutations === before) return;
		}
	},
	go: (path) => {
		global.location.pathname = path;
		for (const handler of popstate) handler({});
	},
	first_element_saw_a_fetch: () => first_element_saw_a_fetch,
};
"#;
