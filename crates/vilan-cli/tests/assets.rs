//! End-to-end CLI test for the asset channel (proposal/const-eval.md §3):
//! `vilan build` writes `<output>.<kind>` beside the compiled JS, with the
//! collected lines deduplicated and ordered by the kind-specific rule (css
//! cascade-ordered — lexical except media rules ascend by min-width, B35;
//! every other kind lexical by line — G5), a kind that stops emitting loses
//! its file on the next build (the per-kind prune, G6) — and, per hmr.md §11
//! S0, `vilan run` / `run --watch` write the same sidecar each round so the
//! dev loop serves fresh assets, and SWEEP it when the source stops emitting
//! styles, like `build` does (G8). A kind naming a file the build writes
//! itself never reaches the filesystem at all (G7).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

mod support;

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_assets_cli_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn vilan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .output()
        .expect("run vilan")
}

#[test]
fn build_writes_assets_beside_the_output() {
    let dir = temp_project("emit");
    write(
        &dir,
        "app.vl",
        r#"import std::print;
import std::asset::emit;

fun base(): i32 {
	emit("css", ".pA3{padding:1rem}");
	emit("css", "@media (min-width: 768px){.mX{padding:2rem}}");
	1
}

fun accent(): i32 {
	emit("css", ".pA3{padding:1rem}");
	emit("css", ".bC7{background:blue}");
	2
}

let _a = const base();
let _b = const accent();

fun main() {
	print("styled");
}
main();
"#,
    );
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // The JS runs (the consts folded; no runtime emit calls survive).
    let js = std::fs::read_to_string(dir.join("app.mjs")).unwrap();
    assert!(!js.contains("__emit_asset"), "no runtime emit calls:\n{js}");
    // The stylesheet sits beside it: deduplicated, lexically ordered ('.'
    // before '@', so media blocks take the later cascade position).
    let css = std::fs::read_to_string(dir.join("app.css")).unwrap();
    assert_eq!(
        css,
        ".bC7{background:blue}\n.pA3{padding:1rem}\n@media (min-width: 768px){.mX{padding:2rem}}\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_kind_colliding_with_the_build_namespace_never_reaches_the_filesystem() {
    // G7, the probe inverted end to end. `emit("vl", …)` passed E94's shape
    // fence (one path segment) and `write_assets` then wrote `<leg>.vl` —
    // which in a BARE build is the entry source itself, because a lone
    // package's outputs sit exactly where its entry does. The measured
    // before-state: exit 0, `app.vl` replaced by the emitted line, and the
    // `Emitted  …/app.vl` line printed as if that were a build product.
    //
    // The inference pins hold the diagnostic; this holds the FILESYSTEM,
    // which is the part that cannot be inferred from a green analyzer — the
    // fence has to bite before the flush, not merely before the exit code.
    let dir = temp_project("owned_kind");
    let source = "import std::print;\nimport std::asset::emit;\n\nfun clobber(): i32 {\n\temit(\"vl\", \"CLOBBERED\");\n\t1\n}\n\nlet _c = const clobber();\n\nfun main() {\n\tprint(\"hi\");\n}\nmain();\n";
    write(&dir, "app.vl", source);
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        !output.status.success(),
        "a kind naming the build's own namespace must fail the build"
    );
    let report = String::from_utf8_lossy(&output.stderr);
    assert!(
        report.contains("collides with the entry source"),
        "the refusal should name what the kind collides with; got:\n{report}"
    );
    assert_eq!(
        std::fs::read_to_string(&entry).ok().as_deref(),
        Some(source),
        "the entry source must be byte-identical — the whole defect"
    );
    assert!(
        !dir.join("app.mjs").exists(),
        "a refused build writes no bundle"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A single-file program emitting one `css` line plus one line per named
/// extra kind — the fixture for the per-kind prune pins (G6).
fn kind_program(kinds: &[(&str, &str)]) -> String {
    let extra = kinds
        .iter()
        .map(|(kind, line)| format!("\temit(\"{kind}\", \"{line}\");\n"))
        .collect::<String>();
    format!(
        "import std::print;\nimport std::asset::emit;\n\nfun outputs(): i32 {{\n\temit(\"css\", \".k{{color:red}}\");\n{extra}\t1\n}}\n\nlet _o = const outputs();\n\nfun main() {{\n\tprint(\"kinds\");\n}}\nmain();\n"
    )
}

#[test]
fn a_kind_that_stops_emitting_loses_its_file() {
    // G6 (build-hooks.md §5.2d, probe P8 inverted into a pin): a build whose
    // flush holds no contributions for a kind the previous build wrote must
    // remove that kind's file — a stale accumulator flush SHIPS, which is
    // worse than a missing file under "a built app needs nothing but dist/".
    let dir = temp_project("prune");
    write(&dir, "app.vl", &kind_program(&[("manifest", "name=app")]));
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 1 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("app.manifest"))
            .ok()
            .as_deref(),
        Some("name=app\n")
    );

    write(&dir, "app.vl", &kind_program(&[]));
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 2 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !dir.join("app.manifest").exists(),
        "the stale manifest flush must be pruned"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("app.css")).ok().as_deref(),
        Some(".k{color:red}\n"),
        "the still-emitted css sidecar stays"
    );
    assert!(dir.join("app.mjs").is_file(), "the bundle stays");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_prune_spares_what_the_build_did_not_record() {
    // The prune removes exactly the files the previous build's flush wrote
    // and nothing else: a kind still emitting keeps its (rewritten) file, and
    // user-placed files — including one shaped like a kind file — are not the
    // build's to touch.
    let dir = temp_project("prune_spares");
    write(
        &dir,
        "app.vl",
        &kind_program(&[("manifest", "name=app"), ("routes", "GET /a")]),
    );
    write(&dir, "data.txt", "user data\n");
    write(&dir, "app.notes", "user notes\n");
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 1 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    write(&dir, "app.vl", &kind_program(&[("routes", "GET /b")]));
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 2 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !dir.join("app.manifest").exists(),
        "the dropped kind's file must be pruned"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("app.routes"))
            .ok()
            .as_deref(),
        Some("GET /b\n"),
        "a kind still emitting keeps its rewritten file"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("data.txt"))
            .ok()
            .as_deref(),
        Some("user data\n")
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("app.notes"))
            .ok()
            .as_deref(),
        Some("user notes\n"),
        "a user file shaped like a kind file is not the build's to remove"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stale_kind_file_without_a_record_is_left_alone() {
    // The pruner knows kind files from its own record of the previous flush,
    // never by reading filenames: with no record, a look-alike file predating
    // the build is refused, not guessed at.
    let dir = temp_project("prune_refuses");
    write(&dir, "app.manifest", "not the build's\n");
    write(&dir, "app.vl", &kind_program(&[]));
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("app.manifest"))
            .ok()
            .as_deref(),
        Some("not the build's\n"),
        "an unrecorded file must not be pruned"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_kind_record_removes_itself_with_its_last_kind() {
    // The record of flushed kinds (`.vilan-asset-kinds`, beside the outputs)
    // exists exactly while a non-css kind does: written with the first flush
    // that has one, gone after a build whose flush has none — the record
    // never becomes its own stale artifact.
    let dir = temp_project("prune_record");
    write(&dir, "app.vl", &kind_program(&[("manifest", "name=app")]));
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 1 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let record = std::fs::read_to_string(dir.join(".vilan-asset-kinds"))
        .expect("a record of the flushed kinds");
    assert_eq!(record, "app/manifest\n");

    write(&dir, "app.vl", &kind_program(&[]));
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 2 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!dir.join("app.manifest").exists());
    assert!(
        !dir.join(".vilan-asset-kinds").exists(),
        "an empty record is removed, not left behind"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_workspace_node_leg_kind_that_stops_emitting_is_pruned_from_dist() {
    // Probe P8's literal shape: a NODE leg's `dist/server.routes` outlived
    // the build that stopped emitting it. The prune is per leg — the client
    // leg's namespace and bundle are untouched.
    let dir = temp_project("prune_ws");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", "fun main() {}\n");
    let with_routes = "import std::asset::emit;\n\nfun routes(): i32 {\n\temit(\"routes\", \"GET /health\");\n\t1\n}\n\nlet _r = const routes();\n\nfun main() {}\n";
    write(&dir, "src/server.vl", with_routes);
    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 1 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("dist/server.routes"))
            .ok()
            .as_deref(),
        Some("GET /health\n")
    );

    write(&dir, "src/server.vl", "fun main() {}\n");
    let output = vilan(&["build", dir.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build 2 failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !dir.join("dist/server.routes").exists(),
        "the node leg's stale kind file must be pruned from dist/"
    );
    assert!(dir.join("dist/server.mjs").is_file());
    assert!(dir.join("dist/client.js").is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Polls until `path` no longer exists, up to `deadline`. `Err` carries the
/// path for the assert message.
fn wait_for_gone(path: &Path, deadline: Duration) -> Result<(), String> {
    let start = Instant::now();
    while start.elapsed() < deadline {
        if !path.exists() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(format!("{} still exists", path.display()))
}

#[test]
fn a_watch_round_prunes_a_kind_that_stopped_emitting_from_dist() {
    // The dev loop writes non-css kinds into `dist/` on its own path, not
    // through `write_assets` — the both-pipelines scar: a prune wired into
    // `build` alone would leave a watch session serving the stale flush
    // round after round (kolt.local 007's resurrection shape, on a kind).
    // Round 1's server leg emits a `routes` kind; the edit stops emitting
    // it; the round it triggers must remove `dist/server.routes`.
    let dir = temp_project("watch_prune");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/client.vl", "fun main() {}\n");
    write(
        &dir,
        "src/server.vl",
        "import std::print;\nimport std::asset::emit;\n\nfun routes(): i32 {\n\temit(\"routes\", \"GET /health\");\n\t1\n}\n\nlet _r = const routes();\n\nfun main() {\n\tprint(\"srv\");\n}\n",
    );

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", dir.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let routes = dir.join("dist/server.routes");
    let deadline = support::WATCH_LIVENESS;
    let round_one = wait_for_contents(&routes, "GET /health\n", deadline);

    write(
        &dir,
        "src/server.vl",
        "import std::print;\n\nfun main() {\n\tprint(\"srv\");\n}\n",
    );
    let round_two = wait_for_gone(&routes, deadline);

    support::kill_watcher(&mut watcher);

    round_one.expect("round 1 should have written dist/server.routes");
    round_two.expect("the round after the edit should have pruned the stale kind file");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A quick-exit single-file program whose `const` initializer emits one CSS line.
/// `main` prints and returns, so Node exits on its own — safe to `run` (and to
/// spawn under `--watch` and kill).
fn quick_exit_program(marker: &str) -> String {
    format!(
        "import std::print;\nimport std::asset::emit;\n\nfun styles(): i32 {{\n\temit(\"css\", \".{marker}{{color:red}}\");\n\t1\n}}\n\nlet _s = const styles();\n\nfun main() {{\n\tprint(\"{marker}\");\n}}\nmain();\n"
    )
}

#[test]
fn run_writes_assets_beside_the_output() {
    // Single-package `vilan run` (the blocking path) must write the sidecar beside
    // the *canonical* build output (`app.css`, where `build` puts it) — not beside
    // the temp script Node executes. The G2 tail: `run`'s missing-CSS gap, closed.
    let dir = temp_project("run_single");
    write(&dir, "app.vl", &quick_exit_program("rS"));
    let entry = dir.join("app.vl");
    let output = vilan(&["run", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("rS"),
        "the program should have run to completion"
    );
    let css = std::fs::read_to_string(dir.join("app.css")).expect("app.css beside the entry");
    assert_eq!(css, ".rS{color:red}\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn workspace_run_writes_fresh_dist_css() {
    // Workspace `vilan run` already routes its build through
    // `build_workspace_artifacts` (which calls `write_assets`), so the client
    // leg's CSS lands in `dist/client.css`. Pinned so the shared helper can't
    // regress. The server leg prints and exits, so `run` (which waits) returns.
    let dir = temp_project("run_ws");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(
        &dir,
        "src/client.vl",
        "import std::print;\nimport std::asset::emit;\n\nfun styles(): i32 {\n\temit(\"css\", \".ws{margin:0}\");\n\t1\n}\n\nlet _s = const styles();\n\nfun main() {\n\tprint(\"ui\");\n}\n",
    );
    write(
        &dir,
        "src/server.vl",
        "import std::print;\n\nfun main() {\n\tprint(\"fresh\");\n}\n",
    );
    let output = vilan(&["run", dir.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "workspace run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let css = std::fs::read_to_string(dir.join("dist/client.css")).expect("dist/client.css");
    assert_eq!(css, ".ws{margin:0}\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same program with the styles taken out — `run`'s round after the edit
/// that G8 is about.
fn styleless_program(marker: &str) -> String {
    format!("import std::print;\n\nfun main() {{\n\tprint(\"{marker}\");\n}}\nmain();\n")
}

#[test]
fn run_sweeps_the_stale_sidecar() {
    // G8: `sweep_stale_sidecar` hung off `write_chunks`, which `vilan build`
    // reaches and `vilan run` does not — so `app.css` survived a `run` whose
    // source no longer emitted a single style, and the same tree built with
    // `vilan build` had it removed. Measured before the fix: build → `app.css`
    // present; delete the styles; `run` → `app.css` STILL present with the old
    // bytes; `build` → gone. The sweep now lives in `write_assets`, which every
    // flushing path calls, so `run` answers like `build`.
    let dir = temp_project("run_sweep");
    write(&dir, "app.vl", &quick_exit_program("rSw"));
    // An unrelated user file beside the entry — the sweep names ONE file,
    // `<entry>.css`, and everything else in the directory is not its business.
    write(&dir, "notes.txt", "user notes\n");
    let entry = dir.join("app.vl");
    let output = vilan(&["build", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("app.css")).ok().as_deref(),
        Some(".rSw{color:red}\n")
    );

    write(&dir, "app.vl", &styleless_program("rSw"));
    let output = vilan(&["run", entry.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "run failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !dir.join("app.css").exists(),
        "`vilan run` must sweep the sidecar its source stopped emitting"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("notes.txt"))
            .ok()
            .as_deref(),
        Some("user notes\n"),
        "a user file beside the entry is not the sweep's to remove"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Polls for `path` to hold `expected`, up to a bounded deadline. Returns the last
/// content seen (for a helpful assert message) if it never matches.
fn wait_for_contents(path: &Path, expected: &str, deadline: Duration) -> Result<(), String> {
    let start = Instant::now();
    let mut last = String::from("<never written>");
    while start.elapsed() < deadline {
        if let Ok(contents) = std::fs::read_to_string(path) {
            if contents == expected {
                return Ok(());
            }
            last = contents;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Err(last)
}

#[test]
fn watch_round_refreshes_the_sidecar() {
    // `run --watch` never exits, so this is a bounded end-to-end: spawn it, wait
    // for the round-1 sidecar, edit the source, wait for the round-2 sidecar, then
    // kill and reap the watcher. The program is quick-exit (main prints and
    // returns), so each round's Node child terminates on its own — killing the
    // watcher orphans nothing (the house scar tissue: a long-lived Node grandchild
    // would leak).
    let dir = temp_project("run_watch");
    let entry = dir.join("app.vl");
    write(&dir, "app.vl", &quick_exit_program("v1"));

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", entry.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let css = dir.join("app.css");
    // The deadline exists to catch a HUNG watch loop, not to race a loaded
    // scheduler: under the full parallel suite a round (poll → detect →
    // recompile → node run → sidecar write) legitimately runs long, and 20s
    // lost that race three times in two days — 20.25s local, 20.5s on the
    // ubuntu leg, 20.97s on the windows leg, each a bare overshoot on a round
    // that then completed (backlog E20; the eventual real fix is an injected
    // change event instead of racing the poller). Its replacement, 120 s, was
    // the same bet at a longer odds, and E39 watched a box carrying five
    // overlapping suites eat all of it; this is now the family's shared
    // liveness bound (E40), which is the same conclusion with the number
    // attached to a measurement instead of to an incident.
    let deadline = support::WATCH_LIVENESS;
    let round_one = wait_for_contents(&css, ".v1{color:red}\n", deadline);

    // A watch round must rewrite the sidecar from the edited source.
    std::fs::write(&entry, quick_exit_program("v2")).unwrap();
    let round_two = wait_for_contents(&css, ".v2{color:red}\n", deadline);

    support::kill_watcher(&mut watcher);

    round_one.expect("round 1 should have written the v1 sidecar");
    round_two
        .map_err(|last| format!("watch round did not refresh the sidecar; last saw: {last:?}"))
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_single_file_watch_round_sweeps_the_stale_sidecar() {
    // G8's other half, and the one that hurts most: the single-file watch
    // round writes assets on its own path (not through `build`), so before the
    // fix a session kept `app.css` on disk round after round with the styles
    // long deleted — kolt.local 007's resurrection shape, which is exactly why
    // `sweep_stale_sidecar` exists. Bounded end-to-end, like its sibling
    // above: wait for round 1's sidecar, edit the styles out, wait for the
    // round it triggers to remove the file.
    let dir = temp_project("watch_sweep");
    let entry = dir.join("app.vl");
    write(&dir, "app.vl", &quick_exit_program("wSw"));

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", entry.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn run --watch");

    let css = dir.join("app.css");
    let deadline = support::WATCH_LIVENESS;
    let round_one = wait_for_contents(&css, ".wSw{color:red}\n", deadline);

    std::fs::write(&entry, styleless_program("wSw")).unwrap();
    let round_two = wait_for_gone(&css, deadline);

    support::kill_watcher(&mut watcher);

    round_one.expect("round 1 should have written the sidecar");
    round_two.expect("the round after the edit should have swept the stale sidecar");
    let _ = std::fs::remove_dir_all(&dir);
}
