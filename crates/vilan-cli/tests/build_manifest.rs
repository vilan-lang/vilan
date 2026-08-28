//! The leg's build manifest and the channel that reads it
//! (proposal/fullstack-dx.md §5.2, §10.2, §10.3 — S2).
//!
//! `dist/<leg>.chunks.json` began as a list of chunk files a hand-written
//! server could iterate (`bundle-splitting.md` §3), written only when the leg
//! split. §10.3 made it the leg's BUILD MANIFEST — *what this leg's build
//! emitted* — so it is written on every build of a browser leg, carrying `leg`,
//! `entry`, `styles`, `classic_script` and `chunks`. What that buys is the
//! thing `std::build::build_of` needs and a filesystem probe cannot give: the
//! difference between "this leg emitted no stylesheet" and "this leg was never
//! built".
//!
//! The byte-level shape is pinned by the split fixture's golden
//! (`tests/split/golden/app.chunks.json`); what is pinned here is the FIELDS'
//! meaning, over legs the golden cannot show — one with styles, one without,
//! one that does not split, and a node leg that gets no manifest at all.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "vilan_build_manifest_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&directory);
    directory
}

fn write(directory: &Path, relative: &str, contents: &str) {
    let path = directory.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent directory")).expect("create the parent");
    std::fs::write(path, contents).expect("write the file");
}

fn vilan(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .current_dir(directory)
        .env("NO_COLOR", "1")
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

/// A two-entry project: a browser client and the node server that would serve
/// it. `styles` decides whether the client compiles a `const style()`, which is
/// the one thing the manifest's `styles` field reports.
fn two_leg_project(tag: &str, styles: bool) -> PathBuf {
    let directory = temp_project(tag);
    write(
        &directory,
        "vilan.toml",
        "[package]\nname = \"legs\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    let client = if styles {
        r#"import std::style::{ Display, Style, style };
import std::ui::{ mount_root, view };

fun panel(): Style {
	style().display(Display::Flex)
}

fun main() {
	let card = const panel();
	let _root = mount_root("app", || view("main").styled(card).text("styled"));
}
"#
    } else {
        r#"import std::ui::{ mount_root, view };

fun main() {
	let _root = mount_root("app", || view("main").text("plain"));
}
"#
    };
    write(&directory, "src/client.vl", client);
    write(
        &directory,
        "src/server.vl",
        "import std::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
    );
    directory
}

fn manifest_of(directory: &Path, leg: &str) -> Option<String> {
    std::fs::read_to_string(directory.join("dist").join(format!("{leg}.chunks.json"))).ok()
}

#[test]
fn a_browser_leg_that_does_not_split_still_writes_its_build_manifest() {
    // The S2 gate, and the reversal of `bundle-splitting.md` §9's "dropping
    // `split` takes the manifest with it": no example in this tree splits (§9
    // measured that none should), so under the old rule the one artifact that
    // describes a leg's output was absent for every leg in the repository.
    let directory = two_leg_project("plain", false);
    let built = vilan(&directory, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );

    let manifest = manifest_of(&directory, "client").expect("the client leg's build manifest");
    assert!(
        manifest.contains("\"leg\": \"client\"") && manifest.contains("\"entry\": \"client.js\""),
        "the manifest names the leg and its bundle: {manifest}"
    );
    assert!(
        manifest.contains("\"chunks\": []"),
        "a leg that does not split says so with an empty list, not an absent file: {manifest}"
    );
    assert!(
        manifest.contains("\"classic_script\": false"),
        "only a splitting leg must be loaded as a classic script: {manifest}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn the_manifest_names_the_style_sidecar_exactly_when_the_build_emitted_one() {
    // F1/F2, answered by the build instead of by a filesystem probe: `styles`
    // is `null` for a leg with no `const style()` and names the sidecar for one
    // that has it. A `fs::stat("dist/client.css")` probe — what the `vilan
    // init` template did before serve_build — cannot tell either case from a
    // stale file a previous build left behind.
    let without = two_leg_project("nostyles", false);
    let built = vilan(&without, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );
    let manifest = manifest_of(&without, "client").expect("a manifest");
    assert!(
        manifest.contains("\"styles\": null"),
        "a leg with no styles emits no sidecar and says so: {manifest}"
    );
    assert!(
        !without.join("dist/client.css").exists(),
        "and there is no sidecar on disk to probe for"
    );
    let _ = std::fs::remove_dir_all(&without);

    let with = two_leg_project("styles", true);
    let built = vilan(&with, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );
    let manifest = manifest_of(&with, "client").expect("a manifest");
    assert!(
        manifest.contains("\"styles\": \"client.css\""),
        "a leg with styles names the sidecar it wrote: {manifest}"
    );
    assert!(
        with.join("dist/client.css").is_file(),
        "and the sidecar it names is on disk"
    );
    let _ = std::fs::remove_dir_all(&with);
}

#[test]
fn a_node_leg_writes_no_build_manifest() {
    // The manifest describes what a BROWSER loads: `classic_script` has no
    // meaning off the browser and a node leg has no chunks and no shell.
    let directory = two_leg_project("node", false);
    let built = vilan(&directory, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );
    assert!(
        directory.join("dist/server.mjs").is_file(),
        "the node leg builds"
    );
    assert!(
        manifest_of(&directory, "server").is_none(),
        "and describes nothing a browser could load"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// A server leg that reports what `build_of` said, so the channel is pinned
/// through the same surface a real server uses.
const PROBE_SERVER: &str = r#"import std::build::build_of;
import std::print;
import std::result::Result::{ Err, Ok };

async fun main() {
	match build_of("client") {
		Ok(let build) => {
			print(i"leg={build.leg}");
			print(i"bundle={build.bundle}");
			print(i"classic={build.classic_script}");
			print(i"chunks={build.chunks.len()}");
			for artifact in build.artifacts() {
				let (url, file) = artifact;
				print(i"serve {url} <- {file}");
			}
		},
		Err(let error) => print(i"error: {error.message()}"),
	}
}
"#;

#[test]
fn build_of_describes_the_leg_the_build_wrote() {
    let directory = two_leg_project("channel", true);
    write(&directory, "src/server.vl", PROBE_SERVER);
    let built = vilan(&directory, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );

    let ran = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(&directory)
        .output()
        .expect("run the probe server");
    let report = combined(&ran);
    for expected in [
        "leg=client",
        "bundle=client.js",
        "classic=false",
        "chunks=0",
        "serve /client.js <- dist/client.js",
        "serve /client.css <- dist/client.css",
    ] {
        assert!(
            report.contains(expected),
            "`build_of` should report {expected:?}:\n{report}"
        );
    }
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn build_of_on_a_leg_that_was_never_built_is_a_named_error() {
    // The S2 gate. Not a panic, not an empty build, and not `ENOENT` from a
    // read of a path the user typed: a named error whose message says what was
    // looked for and what would produce it.
    let directory = two_leg_project("unbuilt", false);
    write(
        &directory,
        "src/server.vl",
        r#"import std::build::build_of;
import std::print;
import std::result::Result::{ Err, Ok };

async fun main() {
	match build_of("nosuchleg") {
		Ok(let build) => print(i"unexpectedly described {build.leg}"),
		Err(let error) => print(i"error: {error.message()}"),
	}
	print("still running");
}
"#,
    );
    let built = vilan(&directory, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );

    let ran = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(&directory)
        .output()
        .expect("run the probe server");
    let report = combined(&ran);
    assert!(
        ran.status.success(),
        "an unbuilt leg is a value to handle, not a crash:\n{report}"
    );
    assert!(
        report.contains("dist/nosuchleg.chunks.json") && report.contains("vilan build"),
        "the error names the manifest it wanted and the build that writes it:\n{report}"
    );
    assert!(
        report.contains("still running"),
        "and the program keeps going, because the error was returned:\n{report}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

/// The corrupt-manifest arm, shared by the two `Unreadable` tests below: build
/// the real project, then overwrite `dist/client.chunks.json` with `contents`
/// and assert `build_of` reports `BuildError::Unreadable`'s own sentence.
fn an_overwritten_manifest_is_unreadable(tag: &str, contents: &str) {
    let directory = two_leg_project(tag, false);
    write(&directory, "src/server.vl", PROBE_SERVER);
    let built = vilan(&directory, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );
    write(&directory, "dist/client.chunks.json", contents);

    let ran = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(&directory)
        .output()
        .expect("run the probe server");
    let report = combined(&ran);
    assert!(
        ran.status.success(),
        "a corrupt manifest is a value to handle, not a crash:\n{report}"
    );
    assert!(
        report.contains(
            "dist/client.chunks.json is not a build manifest this toolchain wrote \
             — rebuild the leg (`vilan build .`)"
        ),
        "the error disowns the manifest and names the rebuild:\n{report}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}

#[test]
fn a_manifest_that_is_not_json_is_the_unreadable_error() {
    // Ledger row 231's flagged gap (diagnostics-ledger.md batch 8): nothing
    // drove the corrupt-manifest arm. A manifest that does not parse at all —
    // a half-written or hand-mangled file — is `Unreadable`, named, not a
    // JSON exception from inside std.
    an_overwritten_manifest_is_unreadable("notjson", "not a manifest");
}

#[test]
fn a_manifest_missing_this_toolchains_fields_is_the_unreadable_error() {
    // The checked-shape arm of the same row: valid JSON that does not carry
    // the fields this toolchain writes. Checked rather than coerced, because
    // `coerce_str` over an absent field yields `"undefined"` — which would
    // become a route.
    an_overwritten_manifest_is_unreadable("wrongshape", "{\"leg\": \"client\"}");
}

#[test]
fn require_build_stops_the_boot_naming_the_missing_manifest() {
    // Batch 8's other flagged gap: no test drove `require_build` to its own
    // panic (its message was pinned only through `build_of`'s returned
    // error). The server-boot idiom end-to-end: a server whose manifest is
    // gone must STOP, with `BuildError`'s own sentence, before it serves
    // anything.
    let directory = two_leg_project("requireboot", false);
    write(
        &directory,
        "src/server.vl",
        r#"import std::build::require_build;
import std::print;

async fun main() {
	let build = require_build("client");
	print(i"described {build.leg}");
}
"#,
    );
    let built = vilan(&directory, &["build", "."]);
    assert!(
        built.status.success(),
        "build failed:\n{}",
        combined(&built)
    );
    std::fs::remove_file(directory.join("dist/client.chunks.json")).expect("remove the manifest");

    let ran = Command::new("node")
        .arg("dist/server.mjs")
        .current_dir(&directory)
        .output()
        .expect("run the server");
    let report = combined(&ran);
    assert!(
        !ran.status.success(),
        "a server that cannot describe its own build must not start:\n{report}"
    );
    assert!(
        report.contains(
            "no build manifest at dist/client.chunks.json — build the leg first \
             (`vilan build .`), and run the server from the project root"
        ),
        "the panic carries the named error's own sentence:\n{report}"
    );
    assert!(
        !report.contains("described"),
        "and nothing after the boot line ran:\n{report}"
    );
    let _ = std::fs::remove_dir_all(&directory);
}
