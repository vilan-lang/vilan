//! S5 — rung 2, the document (proposal/fullstack-dx.md §5.5, §5.8, §8 S5).
//!
//! `Document::of(build)` writes the page the build implies: the doctype, the
//! `<html lang>`, the charset and viewport, the `<title>`, the stylesheet link
//! IF AND ONLY IF the build emitted styles, the mount element, and the bundle's
//! script tag in the form the build requires. The claim that makes it one design
//! with S4's validator rather than a second implementation of the same rules is
//! a property:
//!
//!   **every document `Document::of` can produce passes `check_shell`.**
//!
//! It is pinned here over the builder's whole option space — 1152 documents,
//! every combination of styles/no styles, splitting/not, three titles (one
//! hostile), two languages, two mount ids, four `head` escape hatches, three
//! `body` ones, and rendered/unrendered. That is `ssr.md` §4's
//! cross-implementation differential in a different key: two things that must
//! agree, held against each other by construction rather than by review.
//!
//! Two more pins. `render(view)` splices INSIDE the mount element at both rungs
//! — a generated document and a hand-authored shell — which is what retires the
//! `<!--ssr-->` marker (§5.8), and it leaves the document it was called on
//! unchanged, because a handler renders per request from one boot-time value.
//! And a generated document is served over a REAL build end to end: the page's
//! `<link>` and `<script>` resolve to routes `serve_build` actually answers,
//! which is the whole F1 loop — emitted, linked, served — closed in one test.
//!
//! The escape hatches are the property's one hole, and E70 closed it (§16.10):
//! `head`/`body` take raw markup, raw markup can be wrong, and `html()` now
//! holds the assembled page to the same `check_shell` rules whenever either
//! hatch was used — a fault stops the boot with the same report a hand-written
//! shell gets. One refusal pin per fault family the hatches can trip (script,
//! stylesheet, mount id), a valid hatch passing, and the other side of the
//! ruling: a document with NO hatch markup is derived from the build alone, so
//! it runs no check at all and its bytes are exactly what they were.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let staged = std::env::temp_dir().join(format!(
        "vilan_document_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&staged);
    staged
}

fn vilan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .output()
        .expect("run vilan")
}

fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Run a one-file node program against the real `std::document` and hand back
/// what it printed. The probes below build `LegBuild` values directly, so they
/// ask about the document and nothing else — no build, no server, no port.
fn run_probe(tag: &str, source: &str) -> String {
    let staged = temp_project(tag);
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    std::fs::write(
        staged.join("vilan.toml"),
        format!("[package]\nname = \"{tag}\"\ntarget = \"node\"\n"),
    )
    .expect("write the manifest");
    std::fs::write(staged.join("src/main.vl"), source).expect("write the probe");
    let output = vilan(&["run", staged.to_str().expect("utf-8 temp path")]);
    let report = combined(&output);
    assert!(output.status.success(), "the probe should run:\n{report}");
    let _ = std::fs::remove_dir_all(&staged);
    report
}

/// The same one-file probe, expected to REFUSE: the program must exit
/// non-zero (a `ShellFault` refusal is a panic at the `html()` call), and the
/// report is handed back for the message assertions. Each caller asserts the
/// fault's own recorded head, which is what discriminates a genuine refusal
/// from a probe that merely failed to compile.
fn refused_probe(tag: &str, source: &str) -> String {
    let staged = temp_project(tag);
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    std::fs::write(
        staged.join("vilan.toml"),
        format!("[package]\nname = \"{tag}\"\ntarget = \"node\"\n"),
    )
    .expect("write the manifest");
    std::fs::write(staged.join("src/main.vl"), source).expect("write the probe");
    let output = vilan(&["run", staged.to_str().expect("utf-8 temp path")]);
    let report = combined(&output);
    assert!(
        !output.status.success(),
        "the probe should refuse rather than hand the page back:\n{report}"
    );
    let _ = std::fs::remove_dir_all(&staged);
    report
}

/// The property, as a program: every document the builder can produce, checked.
///
/// The option space is written out rather than sampled — the whole point is
/// that no combination of the generator's own decisions can produce a document
/// its own checker rejects, and a sampled space cannot say that. Every `head`
/// and `body` value here is markup a real page carries (a favicon, an `og:`
/// tag, a page-frame `<style>`, a `<noscript>`).
const PROPERTY: &str = r#"import std::build::LegBuild;
import std::document::{ Document, ShellFault, check_shell };
import std::io::print;
import std::option::Option::{ None, Some, self };
import std::result::Result::{ Err, Ok, self };
import std::ui::{ View, view };

fun app(): View {
	view("main").class("app").text("rendered")
}

/// Every document `Document::of` can produce over one build, checked against
/// that build. Returns `(documents, faults)`.
fun check_every_document(build: LegBuild): (i32, i32) {
	let titles = ["", "Notes", "Tasks & <notes> \"quoted\""];
	let languages = ["en", "de-CH"];
	let mounts = ["app", "root"];
	let heads = [
		"",
		"<link rel=\"icon\" href=\"/favicon.ico\" />",
		"<style>body { margin: 0 }</style>",
		"<meta property=\"og:title\" content=\"a & b\" />",
	];
	let bodies = ["", "<noscript>needs JavaScript</noscript>", "<footer>&copy; 2026</footer>"];
	let renders = [false, true];

	mut documents = 0;
	mut faults = 0;
	for title in titles {
		for language in languages {
			for mount in mounts {
				for head in heads {
					for body in bodies {
						for rendered in renders {
							let base = Document::of(build).title(title).lang(language).mount(mount).head(head).body(body);
							let document = if rendered { base.render(app()) } else { base };
							documents = documents + 1;
							match check_shell(document.html(), build, mount) {
								Ok(let _checked) => {},
								Err(let found) => {
									for fault in found {
										faults = faults + 1;
										print(i"FAULT [{title}|{language}|{mount}|{head}|{body}|{rendered}] {fault.message()}");
									}
								},
							}
						}
					}
				}
			}
		}
	}
	(documents, faults)
}

fun main() {
	mut documents = 0;
	mut faults = 0;
	// The build's own axes: a leg with styles and one without, a leg that
	// splits and one that does not.
	for styles in [None, Some("client.css")] {
		for splits in [false, true] {
			let build = LegBuild {
				leg = "client",
				dist = "dist",
				bundle = "client.js",
				styles = styles,
				chunks = if splits { ["client.Route_Home.js"] } else { [] },
				classic_script = splits,
			};
			let (checked, found) = check_every_document(build);
			documents = documents + checked;
			faults = faults + found;
		}
	}
	print(i"documents={documents} faults={faults}");
}
"#;

#[test]
fn every_document_of_can_produce_passes_check_shell() {
    let report = run_probe("property", PROPERTY);
    assert!(
        !report.contains("FAULT"),
        "a document the generator wrote failed its own checker:\n{report}"
    );
    // The count is asserted, not just the absence of faults: a probe that
    // silently checked nothing would otherwise pass this test forever.
    //
    // Since E70, most of this option space (every combination with hatch
    // markup in it) also passes through `html()`'s own internal check on the
    // way — a fault there would panic the probe, so the count doubles as 1056
    // hatched documents surviving the boot-time check. The other direction —
    // hatch markup that is genuinely wrong REFUSES — has its own pins below.
    assert!(
        report.contains("documents=1152 faults=0"),
        "the whole option space should have been checked:\n{report}"
    );
}

/// `render` at both rungs, and what it does to the value it was called on.
const RENDER: &str = r#"import std::build::LegBuild;
import std::document::{ Document, ShellFault, check_shell };
import std::io::print;
import std::option::Option::{ None, Some, self };
import std::result::Result::{ Err, Ok, self };
import std::ui::{ View, view };

fun app(): View {
	view("main").class("app").text("rendered")
}

fun main() {
	let build = LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = Some("client.css"),
		chunks = [],
		classic_script = false,
	};

	let generated = Document::of(build).title("Notes");
	print(i"generated: {generated.render(app()).html().replace("\n", "")}");
	print(i"unrendered: {generated.html().replace("\n", "")}");

	// The rung-0 shell: hand-authored, checked, and spliced into the element
	// the check located — no marker anywhere in it.
	let shell = "<!doctype html><html><head><link rel=\"stylesheet\" href=\"/client.css\"></head><body><div id=\"app\"></div><script type=\"module\" src=\"/client.js\"></script></body></html>";
	match Document::from_shell(shell, build) {
		Ok(let supplied) => {
			print(i"supplied: {supplied.render(app()).html()}");
			print(i"again: {supplied.render(app()).html()}");
		},
		Err(let faults) => {
			for fault in faults {
				print(i"supplied: {fault.message()}");
			}
		},
	}
}
"#;

#[test]
fn render_splices_inside_the_mount_element_at_both_rungs() {
    // §5.8: the document knows where the mount element is, so the markup goes
    // inside it BY CONSTRUCTION and there is no marker string to spell wrong.
    let report = run_probe("render", RENDER);
    for line in ["generated", "supplied", "again"] {
        let rendered = report
            .lines()
            .find(|text| text.starts_with(&format!("{line}: ")))
            .unwrap_or_else(|| panic!("the probe should print `{line}`:\n{report}"));
        assert!(
            rendered.contains("<div id=\"app\"><main class=\"app\">rendered</main></div>"),
            "`{line}` should carry the render inside the mount element:\n{rendered}"
        );
        assert!(
            !rendered.contains("<!--"),
            "`{line}` should carry no marker comment at all:\n{rendered}"
        );
    }
    // A handler renders per request from one boot-time document, so rendering
    // must derive rather than mutate — the same reason `render` takes `self`.
    let unrendered = report
        .lines()
        .find(|text| text.starts_with("unrendered: "))
        .expect("the probe should print `unrendered`");
    assert!(
        unrendered.contains("<div id=\"app\"></div>"),
        "the document that was rendered FROM must be unchanged:\n{unrendered}"
    );
}

/// A browser leg with styles, so the generated document has a `<link>` to be
/// right or wrong about.
const STYLED_CLIENT: &str = r#"import std::style::{ Display, Style, style };
import std::ui::{ mount_root, view };

fun panel(): Style {
	style().display(Display::Flex)
}

fun main() {
	let card = const panel();
	let _root = mount_root("app", || view("main").styled(card).text("served"));
}
"#;

/// A server with no `src/app.html` at all: the document is the build's.
fn generated_server(port: u16) -> String {
    format!(
        "import std::build::require_build;\n\
         import std::document::Document;\n\
         import std::http::{{ Request, Response, Server }};\n\
         import std::io::print;\n\
         import std::process;\n\
         \n\
         async fun main() {{\n\
         \tlet build = require_build(\"client\");\n\
         \tlet page = Document::of(build).title(\"Generated\").html();\n\
         \n\
         \tServer::builder()\n\
         \t\t.port({port})\n\
         \t\t.serve_build(build)\n\
         \t\t.on_request(|request| match request.path() {{\n\
         \t\t\t\"/shutdown\" => {{\n\
         \t\t\t\tprocess::exit(0);\n\
         \t\t\t\tResponse::builder().body(\"\").build()\n\
         \t\t\t}},\n\
         \t\t\t_ => Response::builder().set_header(\"Content-Type\", \"text/html\").body(page).build(),\n\
         \t\t}})\n\
         \t\t.on_start(|server| print(\"listening\"))\n\
         \t\t.build()\n\
         \t\t.start();\n\
         }}\n"
    )
}

/// Bind an ephemeral port and release it — the standard small TOCTOU window
/// this suite's server tests all take.
fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("bind an ephemeral port")
        .local_addr()
        .expect("read the bound address")
        .port()
}

fn wait_for_port(port: u16) -> bool {
    let deadline = Instant::now() + support::run_liveness();
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn http_get(port: u16, path: &str) -> String {
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
    let text = String::from_utf8_lossy(&response).into_owned();
    match text.split_once("\r\n\r\n") {
        Some((_, body)) => body.to_string(),
        None => text,
    }
}

/// Ask the server to exit and wait until the port stops accepting — the
/// `/shutdown` + connect-poll idiom. A server this suite spawns dies by the
/// harness's hand and is asserted dead, so a failed assertion cannot leave a
/// listener behind.
fn shutdown(port: u16) -> bool {
    let start = Instant::now();
    loop {
        match TcpStream::connect(("127.0.0.1", port)) {
            Err(_) => return true,
            Ok(mut stream) => {
                let _ = stream.write_all(
                    b"GET /shutdown HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
                );
                if start.elapsed() > support::run_liveness() {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[test]
fn a_generated_document_serves_a_real_build() {
    // The property above proves the generator agrees with the checker. This
    // proves both agree with the BUILD: the `<link>` and the `<script>` the
    // document wrote are routes `serve_build` actually answers, over artifacts
    // `vilan build` actually wrote. No `src/app.html` exists in this project at
    // all — rung 2, which §6.3 keeps out of the scaffold and puts here.
    let port = free_port();
    let staged = temp_project("generated");
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    std::fs::write(
        staged.join("vilan.toml"),
        "[package]\nname = \"generated\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    )
    .expect("write the manifest");
    std::fs::write(staged.join("src/client.vl"), STYLED_CLIENT).expect("write the client");
    std::fs::write(staged.join("src/server.vl"), generated_server(port)).expect("write the server");
    let build = vilan(&["build", staged.to_str().expect("utf-8 temp path")]);
    assert!(
        build.status.success(),
        "vilan build failed:\n{}",
        combined(&build)
    );
    assert!(
        !staged.join("src/app.html").exists(),
        "rung 2 ships no shell — that is the whole point of it"
    );

    let mut server = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(&staged)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the server");

    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        assert!(wait_for_port(port), "the server should bind {port}");
        let page = http_get(port, "/");
        for expected in [
            "<!doctype html>",
            "<title>Generated</title>",
            "<link rel=\"stylesheet\" href=\"/client.css\" />",
            "<div id=\"app\"></div>",
            "<script type=\"module\" src=\"/client.js\"></script>",
        ] {
            assert!(
                page.contains(expected),
                "the generated document should carry {expected}:\n{page}"
            );
        }
        // The link resolves: the stylesheet the build emitted, at the url the
        // document wrote, served by the route the build installed. That loop —
        // emitted, linked, served — is the one the founding bug broke.
        let stylesheet = http_get(port, "/client.css");
        assert!(
            stylesheet.contains("{display:flex}"),
            "the document's own `<link>` should reach the compiled styles:\n{stylesheet}"
        );
        let bundle = http_get(port, "/client.js");
        assert!(
            bundle.contains("mount_root") || bundle.contains("replaceChildren"),
            "and its `<script>` should reach the bundle"
        );
    }));

    let dead = shutdown(port);
    let _ = server.kill();
    let _ = server.wait();
    if outcome.is_ok() {
        assert!(
            dead,
            "the generated-document server must exit on /shutdown — an orphan here holds a port"
        );
        let _ = std::fs::remove_dir_all(&staged);
    }
    outcome.unwrap();
}

// --- E70: the escape hatches meet the rules at `html()` (§16.10) -----------
//
// The envelope every refusal below carries — `fault_report`'s form with the
// `{path}` slot filled by `html()`'s name for where the markup came from,
// because a generated page has no file to point at and its derived parts are
// proven fault-free by the property above: any fault IS the hatches'.
const HATCH_ENVELOPE: &str =
    "this generated document's `head`/`body` markup does not match the `client` build:";

/// §16.7's found-in-passing probe, now a boot refusal: the server writes its
/// page from a REAL build and adds, through `head()`, a module script in the
/// leg's own namespace (`client.…`) that the build never emitted — F3,
/// `ScriptNotEmitted`. Probed on 2026-08-19 this page built, booted, and was
/// served; it is the finding E70 was filed on, and this pin is the ruling.
const HATCHED_SERVER: &str = r#"import std::build::require_build;
import std::document::Document;
import std::http::{ Request, Response, Server };
import std::io::print;

async fun main() {
	let build = require_build("client");
	let page = Document::of(build).head("<script type=\"module\" src=\"/client.Nope.js\"></script>").html();

	Server::builder()
		.port(0)
		.serve_build(build)
		.on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(page).build())
		.on_start(|server| print("listening"))
		.build()
		.start();
}
"#;

#[test]
fn a_hatch_loading_a_script_the_build_did_not_emit_refuses_the_boot() {
    // Port 0 on purpose, like every shell_check refusal pin: no assertion here
    // expects a bound socket, so a regression that lets this server start must
    // not also collide with another test's port on its way to failing.
    let staged = temp_project("hatchrefusal");
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    std::fs::write(
        staged.join("vilan.toml"),
        "[package]\nname = \"hatched\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    )
    .expect("write the manifest");
    std::fs::write(staged.join("src/client.vl"), STYLED_CLIENT).expect("write the client");
    std::fs::write(staged.join("src/server.vl"), HATCHED_SERVER).expect("write the server");
    let build = vilan(&["build", staged.to_str().expect("utf-8 temp path")]);
    assert!(
        build.status.success(),
        "vilan build failed:\n{}",
        combined(&build)
    );

    let outcome = support::boot::boot(&staged);
    support::boot::assert_refused(
        &outcome,
        &[
            HATCH_ENVELOPE,
            "this document loads /client.Nope.js, which this build did not emit",
        ],
    );
    let _ = std::fs::remove_dir_all(&staged);
}

/// The stylesheet family (F2, `LinkedStyleMissing`) at the check's new site: a
/// `<link>` added through `head()` into the leg's namespace, over a build that
/// emitted no styles. The same plant `shell_check.rs` makes in a hand-written
/// shell, and the exact page the property probe used to demonstrate that
/// `check_shell` CAUGHT it when called by hand — `html()` now refuses to hand
/// it back at all.
const STYLE_HATCH: &str = r#"import std::build::LegBuild;
import std::document::Document;
import std::option::Option::{ None, Some, self };

fun main() {
	let unstyled = LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = None,
		chunks = [],
		classic_script = false,
	};
	let _page = Document::of(unstyled).head("<link rel=\"stylesheet\" href=\"/client.css\" />").html();
}
"#;

#[test]
fn a_hatch_linking_a_stylesheet_the_build_did_not_emit_refuses() {
    let report = refused_probe("stylehatch", STYLE_HATCH);
    for needle in [
        HATCH_ENVELOPE,
        "this document links /client.css, which this build did not emit",
    ] {
        assert!(
            report.contains(needle),
            "the refusal should name {needle}:\n{report}"
        );
    }
}

/// The mount family (F4, `MountMissing`): hatch markup that HIDES the mount
/// element. An unclosed comment in `head()` comments out everything after it —
/// for the scanner and for a real browser alike, which is what makes refusing
/// sound: the served page would genuinely lose its mount element and its
/// script. Both faults are expected, because a check that reported one costs a
/// restart per problem (§5.6).
const MOUNT_HATCH: &str = r#"import std::build::LegBuild;
import std::document::Document;
import std::option::Option::{ None, Some, self };

fun main() {
	let build = LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = None,
		chunks = [],
		classic_script = false,
	};
	let _page = Document::of(build).head("<!--").html();
}
"#;

#[test]
fn a_hatch_that_hides_the_mount_element_refuses() {
    let report = refused_probe("mounthatch", MOUNT_HATCH);
    for needle in [
        HATCH_ENVELOPE,
        "no element in this document carries id=\"app\", which is where the client mounts",
        "this build's bundle client.js is loaded by no script in this document",
    ] {
        assert!(
            report.contains(needle),
            "the refusal should name {needle}:\n{report}"
        );
    }
}

/// The script tag's form (F6, `ModuleScriptWithChunks`): a second script tag
/// for the bundle added through `body()` as a module, over a leg that SPLITS —
/// the generated tag is classic precisely because chunk resolution reads
/// `document.currentScript`, and the hatch's module copy would race it.
const MODULE_HATCH: &str = r#"import std::build::LegBuild;
import std::document::Document;
import std::option::Option::{ None, Some, self };

fun main() {
	let split = LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = None,
		chunks = ["client.Route_Home.js"],
		classic_script = true,
	};
	let _page = Document::of(split).body("<script type=\"module\" src=\"/client.js\"></script>").html();
}
"#;

#[test]
fn a_module_script_hatch_over_a_splitting_leg_refuses() {
    let report = refused_probe("modulehatch", MODULE_HATCH);
    for needle in [
        HATCH_ENVELOPE,
        "client.js is loaded as a module script and this leg SPLITS",
    ] {
        assert!(
            report.contains(needle),
            "the refusal should name {needle}:\n{report}"
        );
    }
}

/// The affirmative half: hatch markup that matches the build passes the check
/// and rides the page. (The property probe passes 1056 hatched documents
/// through the same internal check; this pin is the named, minimal case.)
const VALID_HATCH: &str = r#"import std::build::LegBuild;
import std::document::Document;
import std::io::print;
import std::option::Option::{ None, Some, self };

fun main() {
	let build = LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = Some("client.css"),
		chunks = [],
		classic_script = false,
	};
	print(Document::of(build).title("Notes").head("<link rel=\"icon\" href=\"/favicon.ico\" />").body("<noscript>needs JavaScript</noscript>").html());
}
"#;

#[test]
fn a_valid_hatch_passes_the_check_and_rides_the_page() {
    let report = run_probe("validhatch", VALID_HATCH);
    for needle in [
        "<link rel=\"icon\" href=\"/favicon.ico\" />",
        "<noscript>needs JavaScript</noscript>",
    ] {
        assert!(
            report.contains(needle),
            "the checked page should carry the hatch markup {needle}:\n{report}"
        );
    }
}

/// The other side of the ruling: NO hatch markup means no check at all — the
/// page is derived from the build alone, proven to pass by construction, so
/// `html()` decides by a cheap emptiness flag and never re-reads its own
/// output. Two observables: the bytes are identical to what `html()` produced
/// before the check existed (recorded 2026-08-20 from the pre-E70 std), and a
/// hatch-less page `check_shell` WOULD refuse — an empty mount id, which no
/// element can carry — is still handed back, which is the absence of the
/// check, observed. (`.mount("")` is not endorsed; it is the observable.)
const HATCHLESS: &str = r#"import std::build::LegBuild;
import std::document::Document;
import std::io::print;
import std::option::Option::{ None, Some, self };

fun main() {
	let build = LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = Some("client.css"),
		chunks = [],
		classic_script = false,
	};
	print("===PAGE===");
	print(Document::of(build).title("Notes").html());
	print("===END===");

	let _unchecked = Document::of(build).title("Notes").mount("").html();
	print("unchecked: ok");
}
"#;

#[test]
fn a_hatchless_document_is_byte_identical_and_runs_no_check() {
    let report = run_probe("hatchless", HATCHLESS);
    let expected = concat!(
        "<!doctype html>\n",
        "<html lang=\"en\">\n",
        "\t<head>\n",
        "\t\t<meta charset=\"utf-8\" />\n",
        "\t\t<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\" />\n",
        "\t\t<title>Notes</title>\n",
        "\t\t<link rel=\"stylesheet\" href=\"/client.css\" />\n",
        "\t</head>\n",
        "\t<body>\n",
        "\t\t<div id=\"app\"></div>\n",
        "\t\t<script type=\"module\" src=\"/client.js\"></script>\n",
        "\t</body>\n",
        "</html>\n",
        // `print`'s own trailing newline.
        "\n",
    );
    let page = report
        .split("===PAGE===\n")
        .nth(1)
        .and_then(|rest| rest.split("===END===").next())
        .expect("the probe should print the page between its markers");
    assert_eq!(
        page, expected,
        "the hatch-less page must be byte-identical to the pre-E70 output"
    );
    assert!(
        report.contains("unchecked: ok"),
        "a hatch-less document must run no check at all:\n{report}"
    );
}
