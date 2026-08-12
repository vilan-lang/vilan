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

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
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

	// The other direction, which the property must NOT be read as claiming: the
	// escape hatches are raw markup, and raw markup can still be wrong. A
	// `<link>` added by hand to a leg that emits no styles is caught exactly as
	// it would be in a hand-written shell — generation is sugar over the check,
	// not an exemption from it.
	let unstyled = LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = None,
		chunks = [],
		classic_script = false,
	};
	let hatched = Document::of(unstyled).head("<link rel=\"stylesheet\" href=\"/client.css\" />");
	match check_shell(hatched.html(), unstyled, "app") {
		Ok(let _checked) => print("hatch: ok"),
		Err(let found) => {
			for fault in found {
				print(i"hatch: {fault.message()}");
			}
		},
	}
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
    assert!(
        report.contains("documents=1152 faults=0"),
        "the whole option space should have been checked:\n{report}"
    );
    // And the escape hatch is not an exemption.
    assert!(
        report.contains("hatch: this document links /client.css, which this build did not emit"),
        "raw `head` markup is checked like any other markup:\n{report}"
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
