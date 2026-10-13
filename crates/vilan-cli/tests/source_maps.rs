//! E257 S5 end to end (debugging.md §5.1–§5.2): what `vilan build` writes
//! beside a script, which builds write it, the stack node prints for an
//! uncaught panic under `vilan run`, and a breakpoint on a `.vl` line set
//! through node's inspector.
//!
//! The map's own validity — every segment inside its source's text, a decoder
//! round trip, the JavaScript unchanged by marking it, per construct — is
//! `vilan-core`'s `tests/source_maps.rs`. This file pins the CLI's half: the
//! files, the comment that names the map, the presets, and node reading it.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;

mod support;

fn vilan() -> Command {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
}

fn project(tag: &str) -> PathBuf {
    let directory =
        support::scratch_dir(&format!("vilan_source_maps_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("scratch directory");
    directory
}

fn write(directory: &Path, relative: &str, contents: &str) {
    let path = directory.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
    std::fs::write(&path, contents).expect("write");
}

fn succeeded(output: &Output, what: &str) {
    assert!(
        output.status.success(),
        "{what} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

const PROGRAM: &str = r#"import std::io::print;

fun area(width: i32, height: i32): i32 {
    let product = width * height;
    product
}

fun main() {
    let first = area(3, 4);
    print(first);
}
"#;

fn map_json(path: &Path) -> serde_json::Value {
    let text =
        std::fs::read_to_string(path).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    serde_json::from_str(&text).expect("the map is JSON")
}

#[test]
fn a_debug_build_writes_its_map_beside_the_script_and_names_it() {
    let directory = project("debug");
    write(&directory, "main.vl", PROGRAM);
    let output = vilan()
        .args(["build", "main.vl"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build");
    let script = std::fs::read_to_string(directory.join("main.mjs")).expect("main.mjs");
    assert!(
        script.ends_with("\n//# sourceMappingURL=main.mjs.map\n"),
        "the script's last line names its map:\n{script}"
    );
    // The script is `--stdout`'s bundle plus that one line: the map changes
    // no byte of the program.
    let printed = vilan()
        .args(["build", "main.vl", "--stdout"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&printed, "vilan build --stdout");
    let printed = String::from_utf8(printed.stdout).expect("utf-8");
    assert!(
        !printed.contains("sourceMappingURL"),
        "--stdout prints a bundle, not a build"
    );
    assert_eq!(
        script.strip_suffix("//# sourceMappingURL=main.mjs.map\n"),
        Some(printed.as_str())
    );
    let map = map_json(&directory.join("main.mjs.map"));
    assert_eq!(map["version"], 3);
    assert_eq!(map["file"], "main.mjs");
    // Beside its source, the user's file is spelled relative to the map and
    // carried whole.
    assert_eq!(map["sources"], serde_json::json!(["main.vl"]));
    assert_eq!(map["sourcesContent"], serde_json::json!([PROGRAM]));
    assert_eq!(map["names"], serde_json::json!(["area"]));
    assert!(
        !map["mappings"].as_str().expect("mappings").is_empty(),
        "a program maps somewhere"
    );
}

#[test]
fn a_release_build_writes_none_unless_asked_and_removes_a_stale_one() {
    let directory = project("release");
    write(&directory, "src/main.vl", PROGRAM);
    write(
        &directory,
        "vilan.toml",
        "[package]\nname = \"maps\"\n\n[build]\npreset = \"release\"\n",
    );
    // A map an earlier debug build left behind.
    write(&directory, "src/main.mjs.map", "{}");
    let output = vilan()
        .arg("build")
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build (release)");
    let script = std::fs::read_to_string(directory.join("src/main.mjs")).expect("main.mjs");
    assert!(!script.contains("sourceMappingURL"), "{script}");
    assert!(
        !directory.join("src/main.mjs.map").exists(),
        "a build without a map removes the stale one"
    );

    // `--source-maps` asks for one under release; the minified script then
    // names it on a line of its own.
    let output = vilan()
        .args(["build", "--source-maps"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build --source-maps (release)");
    let mapped = std::fs::read_to_string(directory.join("src/main.mjs")).expect("main.mjs");
    assert_eq!(
        mapped,
        format!("{script}\n//# sourceMappingURL=main.mjs.map\n"),
        "the release script, unchanged, plus the comment"
    );
    let map = map_json(&directory.join("src/main.mjs.map"));
    assert_eq!(map["sources"], serde_json::json!(["main.vl"]));
}

#[test]
fn the_manifest_turns_the_debug_presets_map_off() {
    let directory = project("off");
    write(&directory, "src/main.vl", PROGRAM);
    write(
        &directory,
        "vilan.toml",
        "[package]\nname = \"maps\"\n\n[build]\nsource-maps = false\n",
    );
    let output = vilan()
        .arg("build")
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build");
    let script = std::fs::read_to_string(directory.join("src/main.mjs")).expect("main.mjs");
    assert!(!script.contains("sourceMappingURL"));
    assert!(!directory.join("src/main.mjs.map").exists());
}

#[test]
fn an_uncaught_panic_under_run_names_vl_lines() {
    let directory = project("panic");
    write(
        &directory,
        "main.vl",
        r#"import std::io::print;

fun pick(items: List<i32>, index: usize): i32 {
    items[index]
}

fun main() {
    let items = [1, 2];
    print(pick(items, 5));
}
"#,
    );
    let output = vilan()
        .args(["run", "main.vl"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    assert!(!output.status.success(), "the panic exits non-zero");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("panicked at main.vl:4:"),
        "the panic's own location:\n{stderr}"
    );
    // `--enable-source-maps`: the frames are `.vl` places, the function
    // named by its source name.
    let frame = stderr
        .lines()
        .find(|line| line.trim_start().starts_with("at pick ("))
        .unwrap_or_else(|| panic!("a frame for `pick`:\n{stderr}"));
    assert!(frame.contains("main.vl:4:"), "{frame}");
    assert!(
        stderr
            .lines()
            .any(|line| line.trim_start().starts_with("at ") && line.contains("main.vl:9:")),
        "the caller's frame, at `main`'s call:\n{stderr}"
    );
}

/// N173: `vilan test`'s failing test reports its stack in `.vl` lines, as `vilan run` does — it used
/// to print `file:///tmp/vilan-test-<pid>.mjs:18:2`, a place that is gone when the run ends.
#[test]
fn a_failing_test_names_vl_lines_in_its_stack() {
    let directory = project("test_stack");
    write(&directory, "vilan.toml", "[package]\nname = \"maps\"\n");
    write(
        &directory,
        "main_test.vl",
        "import std::io::assert;\n\nfun check(value: i32) {\n\tassert(value == 3, \"check failed\");\n}\n\nfun main() {\n\tcheck(4);\n}\n",
    );
    let output = vilan()
        .args(["test", "main_test.vl"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    assert!(!output.status.success(), "the failing test exits non-zero");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.lines()
            .any(|line| line.trim_start().starts_with("at check (")
                && line.contains("main_test.vl:4:")),
        "a frame for `check` at its `.vl` line:\n{text}"
    );
    assert!(
        text.lines()
            .any(|line| line.trim_start().starts_with("at ") && line.contains("main_test.vl:8:")),
        "the caller's frame at `main`'s call:\n{text}"
    );
    assert!(
        !text.contains("vilan-test-"),
        "no temp script place is named:\n{text}"
    );
}

/// `--inspect` starts node's inspector, so `--backend rust` (no node) refuses
/// it. Under `--watch` it is accepted since E257 S6 — every round's server
/// starts under the inspector again (`watch_lifecycle`'s
/// `inspect_under_watch_reopens_the_inspector_each_round`).
#[test]
fn inspect_is_refused_with_the_native_backend() {
    let directory = project("refusals");
    write(&directory, "main.vl", PROGRAM);
    let native = vilan()
        .args(["run", "--backend", "rust", "--inspect", "main.vl"])
        .current_dir(&directory)
        .output()
        .expect("run vilan");
    assert!(!native.status.success());
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("`--backend rust` runs no node"),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
}

/// The inspector client: node's inspector protocol over its WebSocket. It
/// places ONE breakpoint on a `.vl` line by reading the program's own map
/// (the `sourceMappingURL` the script names, exactly as a debugger finds
/// it), resumes, and when the program stops reads the stop back through
/// node's own map reader (`module.SourceMap`) and evaluates one expression
/// in the paused frame. One JSON line out.
const INSPECTOR_CLIENT: &str = r#"
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { SourceMap } from "node:module";

const [, , endpoint, vlFile, vlLineText, expression, budgetText] = process.argv;
const vlLine = Number(vlLineText) - 1;

const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
function decode(mappings) {
	const segments = [];
	let source = 0, line = 0, column = 0;
	mappings.split(";").forEach((text, generatedLine) => {
		let generatedColumn = 0;
		for (const segment of text.split(",")) {
			if (!segment) continue;
			const fields = [];
			let value = 0, shift = 0;
			for (const character of segment) {
				const digit = BASE64.indexOf(character);
				value += (digit & 31) << shift;
				if (digit & 32) { shift += 5; continue; }
				fields.push(value & 1 ? -(value >> 1) : value >> 1);
				value = 0; shift = 0;
			}
			generatedColumn += fields[0];
			if (fields.length >= 4) {
				source += fields[1]; line += fields[2]; column += fields[3];
				segments.push({ generatedLine, generatedColumn, source, line, column });
			}
		}
	});
	return segments;
}

const fail = (why) => { console.log(JSON.stringify({ error: why })); process.exit(1); };
setTimeout(() => fail("timed out"), Number(budgetText)).unref();

const socket = new WebSocket(endpoint);
let nextId = 1;
const pending = new Map();
const scripts = [];
const pauses = [];
let pauseWaiter = null;
function send(method, params = {}) {
	const id = nextId++;
	socket.send(JSON.stringify({ id, method, params }));
	return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
}
function nextPause() {
	if (pauses.length) return Promise.resolve(pauses.shift());
	return new Promise((resolve) => (pauseWaiter = resolve));
}
socket.onmessage = (event) => {
	const message = JSON.parse(event.data);
	if (message.id && pending.has(message.id)) {
		const { resolve, reject } = pending.get(message.id);
		pending.delete(message.id);
		message.error ? reject(new Error(JSON.stringify(message.error))) : resolve(message.result);
	} else if (message.method === "Debugger.scriptParsed") {
		scripts.push(message.params);
	} else if (message.method === "Debugger.paused") {
		if (pauseWaiter) { const waiter = pauseWaiter; pauseWaiter = null; waiter(message.params); }
		else pauses.push(message.params);
	}
};
socket.onerror = (event) => fail("socket error: " + (event.message || event.type));
socket.onopen = async () => {
	try {
		await send("Runtime.enable");
		await send("Debugger.enable");
		await send("Runtime.runIfWaitingForDebugger");
		// `--inspect-brk` holds the program before its first line, and every
		// script it loads has been parsed by then.
		const start = await nextPause();
		const script = scripts.find((parsed) => parsed.sourceMapURL && parsed.url.endsWith(".mjs"));
		if (!script) fail("no script names a map: " + JSON.stringify(scripts.map((parsed) => [parsed.url, parsed.sourceMapURL])));
		const payload = JSON.parse(readFileSync(fileURLToPath(new URL(script.sourceMapURL, script.url)), "utf8"));
		const sourceIndex = payload.sources.findIndex((spelled) => spelled.startsWith("file:") && fileURLToPath(spelled) === vlFile);
		if (sourceIndex < 0) fail("the map does not name " + vlFile + ": " + JSON.stringify(payload.sources));
		const target = decode(payload.mappings).find((segment) => segment.source === sourceIndex && segment.line === vlLine);
		if (!target) fail("no mapping on line " + (vlLine + 1));
		await send("Debugger.setBreakpointByUrl", { url: script.url, lineNumber: target.generatedLine, columnNumber: target.generatedColumn });
		await send("Debugger.resume");
		const paused = await nextPause();
		const frame = paused.callFrames[0];
		const entry = new SourceMap(payload).findEntry(frame.location.lineNumber, frame.location.columnNumber);
		const evaluated = await send("Debugger.evaluateOnCallFrame", { callFrameId: frame.callFrameId, expression });
		console.log(JSON.stringify({
			startReason: start.reason,
			hitBreakpoints: (paused.hitBreakpoints || []).length,
			originalSource: entry.originalSource,
			originalLine: entry.originalLine + 1,
			functionName: frame.functionName,
			value: evaluated.result.value,
		}));
		await send("Debugger.resume");
		socket.close();
	} catch (error) {
		fail(String((error && error.stack) || error));
	}
};
"#;

/// Why this node cannot take part, when it cannot: built without the
/// inspector, or without the WebSocket client the client script speaks.
fn inspector_unavailable() -> Option<String> {
    let probe = Command::new("node")
        .args([
            "-e",
            "process.stdout.write(String(process.features.inspector && typeof WebSocket === 'function'))",
        ])
        .output();
    match probe {
        Ok(output) if output.status.success() && output.stdout == b"true" => None,
        Ok(output) => Some(format!(
            "node answered {:?} for the inspector and a WebSocket client",
            String::from_utf8_lossy(&output.stdout)
        )),
        Err(error) => Some(format!("no node to ask: {error}")),
    }
}

#[test]
fn run_inspect_brk_stops_at_a_breakpoint_on_a_vl_line() {
    if let Some(why) = inspector_unavailable() {
        eprintln!(
            "E257 S5b: skipped — the inspector smoke test needs node's inspector and a \
             WebSocket client ({why})"
        );
        return;
    }
    let directory = project("inspect");
    write(&directory, "main.vl", PROGRAM);
    write(&directory, "client.mjs", INSPECTOR_CLIENT);
    let budget = support::run_liveness();

    // Port 0: node picks a free one and says which on stderr, before the
    // program runs a line — that line is the harness's sequencing.
    let mut program = vilan()
        .args(["run", "--inspect-brk=127.0.0.1:0", "main.vl"])
        .current_dir(&directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run vilan");
    let stderr = program.stderr.take().expect("stderr");
    let (lines, received) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if lines.send(line).is_err() {
                break;
            }
        }
    });
    let deadline = std::time::Instant::now() + budget;
    let mut seen = Vec::new();
    let endpoint = loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        match received.recv_timeout(remaining) {
            Ok(line) => {
                if let Some(at) = line.find("ws://") {
                    break line[at..].trim().to_string();
                }
                seen.push(line);
            }
            Err(_) => {
                let _ = program.kill();
                panic!(
                    "node never said where its inspector listens:\n{}",
                    seen.join("\n")
                );
            }
        }
    };

    let entry = vilan_core::util::canonical_path(directory.join("main.vl"));
    let client = Command::new("node")
        .arg(directory.join("client.mjs"))
        .arg(&endpoint)
        .arg(&entry)
        // `    product` — the tail of `area`, after `product` is bound.
        .arg("5")
        .arg("product")
        .arg(budget.as_millis().to_string())
        .output()
        .expect("run the inspector client");
    let verdict = String::from_utf8_lossy(&client.stdout).into_owned();
    let program_output = program.wait_with_output().expect("the program exits");
    drop(received);
    let _ = reader.join();
    assert!(
        client.status.success(),
        "the inspector client failed: {verdict}\n{}",
        String::from_utf8_lossy(&client.stderr)
    );
    let verdict: serde_json::Value = serde_json::from_str(verdict.trim()).expect("one JSON line");
    assert_eq!(verdict["startReason"], "Break on start", "{verdict}");
    assert_eq!(verdict["hitBreakpoints"], 1, "{verdict}");
    assert_eq!(
        verdict["originalLine"], 5,
        "stopped on the `.vl` line: {verdict}"
    );
    assert!(
        verdict["originalSource"]
            .as_str()
            .is_some_and(|source| source.ends_with("/main.vl")),
        "{verdict}"
    );
    assert_eq!(verdict["functionName"], "area", "{verdict}");
    assert_eq!(
        verdict["value"], 12,
        "the paused frame's `product`: {verdict}"
    );
    assert!(
        program_output.status.success(),
        "the program ran to its end"
    );
    assert_eq!(String::from_utf8_lossy(&program_output.stdout), "12\n");
}

/// E257 S6: a `split = true` leg maps its eager bundle and each route chunk —
/// each file printed apart, so stripped apart, each beside its own map and
/// naming it — and the maps change no byte of the program: the same build with
/// `source-maps = false` writes every file without its last line.
#[test]
fn a_split_leg_maps_its_eager_bundle_and_each_chunk() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/split/project");
    let stage = |tag: &str, maps: bool| {
        let directory = project(tag);
        for entry in std::fs::read_dir(&fixture).expect("the split fixture") {
            let path = entry.expect("an entry").path();
            if path.is_file() {
                std::fs::copy(&path, directory.join(path.file_name().expect("a name")))
                    .expect("copy");
            }
        }
        if !maps {
            let manifest = directory.join("vilan.toml");
            let text = std::fs::read_to_string(&manifest).expect("vilan.toml");
            std::fs::write(&manifest, format!("{text}\n[build]\nsource-maps = false\n"))
                .expect("write vilan.toml");
        }
        let output = vilan()
            .arg("build")
            .current_dir(&directory)
            .output()
            .expect("run vilan");
        succeeded(&output, "vilan build (split)");
        directory
    };
    let mapped = stage("split_mapped", true);
    let plain = stage("split_plain", false);
    let scripts = [
        "app.js",
        "app.Route_Home.js",
        "app.Route_Docs.js",
        "app.Route_NotFound.js",
    ];
    for script in scripts {
        let with_map = std::fs::read_to_string(mapped.join(script)).expect(script);
        let without = std::fs::read_to_string(plain.join(script)).expect(script);
        assert_eq!(
            with_map,
            format!("{without}//# sourceMappingURL={script}.map\n"),
            "{script}: the plain bytes plus the comment naming its own map"
        );
        assert!(!plain.join(format!("{script}.map")).exists());
        let map = map_json(&mapped.join(format!("{script}.map")));
        assert_eq!(map["version"], 3);
        assert_eq!(map["file"], script);
        assert!(
            !map["mappings"].as_str().expect("mappings").is_empty(),
            "{script} maps somewhere"
        );
        let sources: Vec<&str> = map["sources"]
            .as_array()
            .expect("sources")
            .iter()
            .map(|source| source.as_str().expect("a string"))
            .collect();
        assert!(
            sources
                .iter()
                .any(|source| source.ends_with(".vl") && !source.starts_with("file:")),
            "{script}: the fixture's own sources, spelled relative: {sources:?}"
        );
    }
    // A build that no longer splits sweeps the chunks' maps with the chunks.
    let manifest = mapped.join("vilan.toml");
    let text = std::fs::read_to_string(&manifest).expect("vilan.toml");
    std::fs::write(&manifest, text.replace("split = true", "split = false")).expect("write");
    let output = vilan()
        .arg("build")
        .current_dir(&mapped)
        .output()
        .expect("run vilan");
    succeeded(&output, "vilan build (unsplit)");
    for chunk in &scripts[1..] {
        assert!(!mapped.join(chunk).exists(), "{chunk} swept");
        assert!(
            !mapped.join(format!("{chunk}.map")).exists(),
            "{chunk}.map swept with its chunk"
        );
    }
    assert!(
        mapped.join("app.js.map").exists(),
        "the bundle keeps its map"
    );
}
