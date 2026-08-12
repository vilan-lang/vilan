//! S4 — the validator (proposal/fullstack-dx.md §5.6, §8 S4).
//!
//! `check_shell` holds a hand-authored HTML shell against what a leg's build
//! actually emitted, and §10.7 ruled how loud a fault is: the server **refuses
//! to boot**. So every pin here is a real project, built, whose `node
//! dist/server.mjs` must stop with a message naming the file, the fault and the
//! fix — one pin per `ShellFault` variant, and each planted by BREAKING A REAL
//! SHELL rather than by writing a broken one:
//!
//!   - **F1** `StylesNotLinked` — the owner's own bug, planted in a *scaffolded*
//!     project by deleting the `<link>` line from `src/app.html`;
//!   - **F2** `LinkedStyleMissing` — the template's shell over a leg that
//!     compiles no styles (what deleting the last `const style()` leaves);
//!   - **F3** `ScriptNotEmitted` — a chunk script over a leg that does not
//!     split, and `BundleNotLoaded` — the shell's own `<script>` deleted;
//!   - **F4** `MountMissing` — the mount `<div id>` renamed;
//!   - **F6** `ModuleScriptWithChunks` — `type="module"` over a splitting leg.
//!
//! Every plant is one edit to `templates/fullstack/src/app.html`, read from the
//! tree rather than transcribed, so a pin cannot drift from the shell the
//! language actually ships.
//!
//! Two more: a shell with two problems reports **two** (a boot-time check that
//! reported one would cost a restart per fault), and a `vilan run` probe pins
//! the discriminations a `contains`-based check could not make — a commented-out
//! link links nothing, a `<div id="app">` inside a script body is not a mount
//! element, a font CDN is not this build's business.
//!
//! Assertions are `contains`, so a host's teardown chatter on a killed child
//! (the Windows `uv_handle` assertion line) is tolerated by construction.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

mod support;

/// A browser leg that compiles a `const style()`, so its build emits a sidecar
/// and the manifest names one — the shape the template's own client has.
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

/// The same leg with its styles deleted — the F2 plant, and the one edit a user
/// makes that silently unlinks their stylesheet.
const PLAIN_CLIENT: &str = r#"import std::ui::{ mount_root, view };

fun main() {
	let _root = mount_root("app", || view("main").text("served"));
}
"#;

/// Which client a staged project carries.
#[derive(Clone, Copy)]
enum Client {
    /// Emits a style sidecar.
    Styled,
    /// Emits none.
    Plain,
    /// The split fixture's own router entry — three arms, so `split = true` has
    /// something to chunk.
    Router,
}

/// The shell the `fullstack` template ships, read out of the tree. Every plant
/// below is one edit to THIS markup, which is what makes each pin a broken real
/// shell rather than a straw one.
fn template_shell() -> String {
    std::fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates/fullstack/src/app.html"),
    )
    .expect("the fullstack template's shell")
}

/// The template shell with its every line matching `needle` removed — how the
/// `<link>` and the `<script>` plants are made.
fn without_lines(shell: &str, needle: &str) -> String {
    shell
        .lines()
        .filter(|line| !line.contains(needle))
        .collect::<Vec<_>>()
        .join("\n")
}

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let staged = std::env::temp_dir().join(format!(
        "vilan_shell_check_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&staged);
    staged
}

/// A two-entry project: one browser leg, and a server whose only job is to boot
/// — it reads `src/app.html`, checks it against the build, and serves it.
fn stage(tag: &str, client: Client, split: bool, shell: &str) -> PathBuf {
    let staged = temp_project(tag);
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    let entry = if split {
        "[entry.client]\ntarget = \"browser\"\nsplit = true\n"
    } else {
        "[entry.client]\ntarget = \"browser\"\n"
    };
    std::fs::write(
        staged.join("vilan.toml"),
        format!("[package]\nname = \"checked\"\n\n{entry}\n[entry.server]\n"),
    )
    .expect("write the manifest");
    let source = match client {
        Client::Styled => STYLED_CLIENT.to_string(),
        Client::Plain => PLAIN_CLIENT.to_string(),
        Client::Router => std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/split/project/app.vl"),
        )
        .expect("the split fixture's client"),
    };
    std::fs::write(staged.join("src/client.vl"), source).expect("write the client");
    std::fs::write(staged.join("src/server.vl"), SERVER).expect("write the server");
    std::fs::write(staged.join("src/app.html"), shell).expect("write the shell");
    build(&staged);
    staged
}

/// The rung-0+ server: the shell is read and CHECKED, and only then served.
///
/// Port 0 on purpose — no pin in this file expects a bound socket, so a
/// regression that let one of these servers start must not also collide with
/// another test's port on its way to failing.
const SERVER: &str = r#"import std::build::require_build;
import std::document::require_shell;
import std::http::{ Request, Response, Server };
import std::io::print;

async fun main() {
	let build = require_build("client");
	let page = require_shell("src/app.html", build).html();

	Server::builder()
		.port(0)
		.serve_build(build)
		.on_request(|request| Response::builder().set_header("Content-Type", "text/html").body(page).build())
		.on_start(|server| print("listening"))
		.build()
		.start();
}
"#;

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

fn build(staged: &Path) {
    let output = vilan(&["build", staged.to_str().expect("utf-8 temp path")]);
    assert!(
        output.status.success(),
        "vilan build failed:\n{}",
        combined(&output)
    );
}

/// What a boot did.
struct Boot {
    /// The server was still running when the wait ran out — for every pin here
    /// that IS the failure: the check did not fire and the process took the
    /// port and the event loop with it.
    started: bool,
    /// It exited non-zero, which is what refusing to boot looks like.
    refused: bool,
    /// Everything it printed, stdout and stderr together.
    report: String,
}

/// Boot the built server from the project root and wait for it to STOP.
///
/// A server that refuses to boot exits on its own; one that (wrongly) started
/// holds the event loop for as long as anything lets it, so the wait is bounded
/// and a child still alive at the deadline is killed BY THE HARNESS and reported
/// as a started server rather than left to outlive the suite.
fn boot(staged: &Path) -> Boot {
    let log = staged.join("boot.log");
    let file = std::fs::File::create(&log).expect("create the boot log");
    let mut server: Child = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(staged)
        .stdout(Stdio::from(file.try_clone().expect("clone the log handle")))
        .stderr(Stdio::from(file))
        .spawn()
        .expect("spawn the server");

    let deadline = Instant::now() + support::run_liveness();
    let mut refused = false;
    let mut started = true;
    while Instant::now() < deadline {
        match server.try_wait() {
            Ok(Some(status)) => {
                started = false;
                refused = !status.success();
                break;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => panic!("wait for the server: {error}"),
        }
    }
    if started {
        let _ = server.kill();
        let _ = server.wait();
    }
    let report = std::fs::read_to_string(&log).unwrap_or_default();
    Boot {
        started,
        refused,
        report,
    }
}

/// The claim every fault pin makes: the server stopped, non-zero, saying so.
fn assert_refused(boot: &Boot, expected: &[&str]) {
    assert!(
        !boot.started,
        "the server STARTED over a shell that does not match its build — the check did not fire:\n{}",
        boot.report
    );
    assert!(
        boot.refused,
        "a refused boot must exit non-zero:\n{}",
        boot.report
    );
    for needle in expected {
        assert!(
            boot.report.contains(needle),
            "the refusal should name {needle}:\n{}",
            boot.report
        );
    }
}

fn cleanup(staged: &Path) {
    let _ = std::fs::remove_dir_all(staged);
}

#[test]
fn a_scaffolded_project_that_loses_its_stylesheet_link_refuses_to_boot() {
    // F1, the founding case (§5.1, and the charter's own example: "a shell
    // missing its stylesheet link shipped silently"). Planted in a project
    // scaffolded by the real `vilan init`, because the claim is about the
    // language's opening argument: the file every new user edits first cannot
    // lose its `<link>` quietly.
    let parent = temp_project("scaffold");
    std::fs::create_dir_all(&parent).expect("create the temp directory");
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["init", "app", "--template", "fullstack"])
        .current_dir(&parent)
        .stdin(Stdio::null())
        .output()
        .expect("run vilan init");
    assert!(
        output.status.success(),
        "vilan init --template fullstack failed:\n{}",
        combined(&output)
    );
    let project = parent.join("app");
    build(&project);

    let shell = project.join("src/app.html");
    let scaffolded = std::fs::read_to_string(&shell).expect("the scaffolded shell");
    assert!(
        scaffolded.contains("rel=\"stylesheet\""),
        "the scaffold is supposed to ship a linked stylesheet:\n{scaffolded}"
    );
    std::fs::write(&shell, without_lines(&scaffolded, "rel=\"stylesheet\"")).expect("plant F1");

    let boot = boot(&project);
    assert_refused(
        &boot,
        &["src/app.html", "client", "client.css", "rel=\"stylesheet\""],
    );
    cleanup(&parent);
}

#[test]
fn a_link_to_a_stylesheet_the_build_did_not_emit_refuses_to_boot() {
    // F2 — the inverse, and the one edit that produces it: the leg's last
    // `const style()` is deleted, so the build emits no sidecar while the shell
    // still links `/client.css`. Today that request is answered by the app's
    // catch-all with the HTML document, at 200, and the browser drops it
    // without a word.
    let staged = stage("unlinked", Client::Plain, false, &template_shell());
    let boot = boot(&staged);
    assert_refused(&boot, &["src/app.html", "/client.css", "did not emit"]);
    cleanup(&staged);
}

#[test]
fn a_script_the_build_did_not_emit_refuses_to_boot() {
    // F3, first half: a route chunk the shell loads and the build no longer
    // writes — what dropping `split` leaves behind. It is inside the leg's own
    // namespace (`client.…`), which is exactly the boundary of what this check
    // is allowed to have an opinion about.
    let shell = template_shell().replace(
        "</body>",
        "\t\t<script src=\"/client.Route_Home.js\"></script>\n\t</body>",
    );
    let staged = stage("stalechunk", Client::Styled, false, &shell);
    let boot = boot(&staged);
    assert_refused(
        &boot,
        &["src/app.html", "/client.Route_Home.js", "did not emit"],
    );
    cleanup(&staged);
}

#[test]
fn a_shell_that_loads_no_bundle_refuses_to_boot() {
    // F3, second half: the page ships without its own application. The symptom
    // today is a blank page and no error at all — the server is fine, the HTML
    // is fine, nothing ever asked for the bundle.
    let staged = stage(
        "nobundle",
        Client::Styled,
        false,
        &without_lines(&template_shell(), "<script"),
    );
    let boot = boot(&staged);
    assert_refused(&boot, &["src/app.html", "client.js", "no script"]);
    cleanup(&staged);
}

#[test]
fn a_renamed_mount_element_refuses_to_boot() {
    // F4 — `<div id>` and `mount_root(id, …)` disagree. Today that is a
    // `Cannot read properties of null` in `element.clear()`, one console line
    // into a blank page.
    let staged = stage(
        "mount",
        Client::Styled,
        false,
        &template_shell().replace("id=\"app\"", "id=\"root\""),
    );
    let boot = boot(&staged);
    assert_refused(&boot, &["src/app.html", "id=\"app\"", "mounts"]);
    cleanup(&staged);
}

#[test]
fn a_module_script_over_a_splitting_leg_refuses_to_boot() {
    // F6 — latent in every shell in the tree, because no leg split until one
    // did. A split leg's chunk resolution reads `document.currentScript`, which
    // is null inside a module script, so the eager bundle loads and every
    // nested route then fails to find its chunk. The shell here is the
    // template's, minus the `<link>` its leg has no styles for: the ONLY thing
    // wrong with it is `type="module"`.
    let shell = without_lines(&template_shell(), "rel=\"stylesheet\"");
    let staged = stage("modulesplit", Client::Router, true, &shell);
    let boot = boot(&staged);
    assert_refused(
        &boot,
        &["src/app.html", "client.js", "module", "currentScript"],
    );
    cleanup(&staged);
}

#[test]
fn a_shell_with_two_faults_reports_both() {
    // "Every fault, not the first — a shell with two problems should report
    // two" (§5.6). A check that stopped at the first would cost a restart per
    // fault, which is how a loud check teaches people to stop reading it.
    let shell =
        without_lines(&template_shell(), "rel=\"stylesheet\"").replace("id=\"app\"", "id=\"root\"");
    let staged = stage("twofaults", Client::Styled, false, &shell);
    let boot = boot(&staged);
    assert_refused(&boot, &["client.css", "id=\"app\""]);
    assert_eq!(
        boot.report.matches("  - ").count(),
        2,
        "both faults, on their own lines:\n{}",
        boot.report
    );
    cleanup(&staged);
}

/// The discrimination probe: `check_shell` over shells that differ only in ways
/// a substring search cannot see. It builds `LegBuild` values directly, so it
/// asks about the CHECK and nothing else — no server, no port, no teardown.
const PROBE: &str = r#"import std::build::LegBuild;
import std::document::{ ShellFault, check_shell };
import std::io::print;
import std::option::Option::{ None, Some, self };
import std::result::Result::{ Err, Ok, self };

fun leg(styles: Option<str>): LegBuild {
	LegBuild {
		leg = "client",
		dist = "dist",
		bundle = "client.js",
		styles = styles,
		chunks = [],
		classic_script = false,
	}
}

fun report(label: str, shell: str, build: LegBuild) {
	match check_shell(shell, build, "app") {
		Ok(let _checked) => print(i"{label}: ok"),
		Err(let faults) => {
			for fault in faults {
				print(i"{label}: {fault.message()}");
			}
		},
	}
}

fun main() {
	let head = "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\" />\n<link rel=\"stylesheet\" href=\"/client.css\" />\n</head>\n";
	let body = "<body>\n<div id=\"app\"></div>\n<script type=\"module\" src=\"/client.js\"></script>\n</body>\n</html>\n";
	let styled = leg(Some("client.css"));

	report("correct", head + body, styled);
	report("cdn", head.replace("</head>", "<link rel=\"stylesheet\" href=\"https://cdn.example.com/font.css\" />\n</head>") + body, styled);
	report("buster", head + body.replace("/client.js", "/client.js?v=2"), styled);
	report("quoted", (head + body).replace("\"", "'"), styled);
	report("commented", head.replace("<link rel=\"stylesheet\" href=\"/client.css\" />", "<!-- <link rel=\"stylesheet\" href=\"/client.css\" /> -->") + body, styled);
	report("inscript", head + body.replace("<div id=\"app\"></div>", "<script>const shell = \"<div id='app'></div>\";</script>"), styled);
	report("theme", head.replace("/client.css", "/theme.css") + body, leg(None));
}
"#;

#[test]
fn the_check_reads_the_markup_rather_than_searching_it() {
    // Six discriminations, and each one is the difference between a check that
    // can refuse to boot and one that cannot be trusted to. A `contains` over
    // the shell gets four of them wrong.
    let staged = temp_project("probe");
    std::fs::create_dir_all(staged.join("src")).expect("create the staging directory");
    std::fs::write(
        staged.join("vilan.toml"),
        "[package]\nname = \"probe\"\ntarget = \"node\"\n",
    )
    .expect("write the manifest");
    std::fs::write(staged.join("src/main.vl"), PROBE).expect("write the probe");

    let output = vilan(&["run", staged.to_str().expect("utf-8 temp path")]);
    let report = combined(&output);
    assert!(output.status.success(), "the probe should run:\n{report}");

    // A shell that matches its build passes — including one whose stylesheet is
    // a font CDN's, whose bundle carries a cache-buster, and whose attributes
    // are single-quoted. None of those is this check's business, and a check
    // that refuses to boot must be sound about that.
    for label in ["correct", "cdn", "buster", "quoted"] {
        assert!(
            report.contains(&format!("{label}: ok")),
            "`{label}` is a document that matches its build:\n{report}"
        );
    }
    // A commented-out link links nothing...
    assert!(
        report.contains("commented: the build emitted the stylesheet client.css"),
        "a `<link>` inside a comment is not a link:\n{report}"
    );
    // ...a mount element inside a script body is not markup...
    assert!(
        report.contains("inscript: no element in this document carries id=\"app\""),
        "a `<div id=\"app\">` inside a script's own string is not a mount element:\n{report}"
    );
    // ...and a stylesheet outside this leg's namespace is not this build's to
    // have an opinion about: `/theme.css` may be served by the app itself, and
    // guessing otherwise would refuse to boot over somebody else's file.
    assert!(
        report.contains("theme: ok"),
        "a stylesheet outside the leg's namespace is outside the check:\n{report}"
    );
    cleanup(&staged);
}
