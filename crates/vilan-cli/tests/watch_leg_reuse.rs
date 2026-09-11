//! `vilan build --watch` recompiles the legs the edit REACHED, not every leg
//! (backlog M22).
//!
//! `run --watch` has skipped a leg whose sources all re-hash to what they were
//! compiled with since E12 half b; `build --watch` never did. The measured
//! consequence on kolt (three legs: client, probe, server): a one-character
//! edit in `views.vl` — a module only the CLIENT leg loads — recompiled all
//! three, every round.
//!
//! The two rows this file pins are exactly the item's: a **client-only**
//! module edit re-emits the client leg only, and a **shared** module edit
//! re-emits every reaching leg. Reuse is decided by CONTENT (the E12 rule), so
//! the fixture changes text, never just an mtime.
//!
//! Timing posture, following the family's (`support::WATCH_LIVENESS`, E39/E40):
//! nothing here asserts how FAST a round is, so every bound is a liveness bound
//! that a green run never pays. The waits are TALKING — a timeout prints every
//! line the watcher produced, so a red run on a loaded box says whether the
//! watcher was slow or silent — and the edit is RE-TOUCHED while waiting,
//! because the watcher polls mtimes and a same-second write can land inside the
//! poll it should have triggered. A re-touch writes new content each time, so
//! it re-triggers without ever weakening the assertion: the question is WHICH
//! legs recompiled, and that answer does not depend on how many rounds it took
//! to ask it.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod support;

/// One leg line of a round: `Compiled <entry> -> <artifact>` when the leg was
/// rebuilt, `Fresh <entry> -> <artifact>` when it was reused.
#[derive(Debug, Clone, PartialEq, Eq)]
struct LegLine {
    rebuilt: bool,
    entry: String,
}

fn parse_leg_line(line: &str) -> Option<LegLine> {
    // The lines are painted; strip the escapes rather than matching them, so
    // the pin does not depend on whether the test's stdout is a terminal.
    let plain: String = strip_ansi(line);
    let (verb, rest) = plain.trim().split_once(' ')?;
    let entry = rest.split(" -> ").next()?.trim().to_string();
    if !rest.contains(" -> ") {
        return None;
    }
    match verb {
        "Compiled" => Some(LegLine {
            rebuilt: true,
            entry,
        }),
        "Fresh" => Some(LegLine {
            rebuilt: false,
            entry,
        }),
        _ => None,
    }
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            for next in chars.by_ref() {
                if next == 'm' {
                    break;
                }
            }
            continue;
        }
        out.push(character);
    }
    out
}

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_watch_reuse_{tag}_{}_{unique}",
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

/// The fixture: two legs over three modules — `shared` (both legs import it),
/// `only_client` (the client alone imports it), and the two entries.
fn build_fixture(dir: &Path) {
    write(
        dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(dir, "src/shared.vl", "fun shared_value(): i32 {\n\t1\n}\n");
    write(
        dir,
        "src/only_client.vl",
        "fun client_value(): i32 {\n\t2\n}\n",
    );
    write(
        dir,
        "src/client.vl",
        "import std::io::print;\nimport pkg::shared::shared_value;\n\
         import pkg::only_client::client_value;\n\n\
         fun main() {\n\tprint(shared_value() + client_value());\n}\n",
    );
    write(
        dir,
        "src/server.vl",
        "import std::io::print;\nimport pkg::shared::shared_value;\n\n\
         fun main() {\n\tprint(shared_value());\n}\n",
    );
}

/// Reads the watcher's stdout on its own thread, so a wait can time out with
/// everything seen so far rather than blocking forever on a silent pipe.
fn spawn_reader(stdout: ChildStdout) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                return;
            }
        }
    });
    receiver
}

/// Collects the next `legs` leg lines, re-touching `retouch` (with fresh
/// content each time) whenever the watcher has been quiet for a while.
///
/// TALKING: on timeout it panics with every line collected so far, because
/// "timed out" alone cannot distinguish a slow box from a watcher that
/// recompiled the wrong set and stopped.
fn next_round(
    receiver: &mpsc::Receiver<String>,
    legs: usize,
    label: &str,
    mut retouch: Option<(&Path, &str, &mut u32)>,
    budget: Duration,
    trace_path: &Path,
) -> Vec<LegLine> {
    let deadline = Instant::now() + budget;
    let mut collected: Vec<String> = Vec::new();
    let mut lines: Vec<LegLine> = Vec::new();
    let mut last_touch = Instant::now();
    while Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(200)) {
            Ok(line) => {
                collected.push(line.clone());
                if let Some(leg) = parse_leg_line(&line) {
                    lines.push(leg);
                    if lines.len() == legs {
                        return lines;
                    }
                }
                last_touch = Instant::now();
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Quiet for a while and the round has not even started: the
                // poll may have missed the write. Re-touch with new content.
                if lines.is_empty()
                    && last_touch.elapsed() > Duration::from_secs(5)
                    && let Some((path, source, counter)) = retouch.as_mut()
                {
                    **counter += 1;
                    let text = format!("{source}// retouch {}\n", **counter);
                    std::fs::write(path, text).expect("re-touch the edited module");
                    last_touch = Instant::now();
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let trace = std::fs::read_to_string(trace_path).unwrap_or_default();
    panic!(
        "timed out waiting for {label} ({} of {legs} leg lines). \
         Watcher stdout so far:\n{}\n\
         --- watch-trace.log (VILAN_WATCH_LOG, B208) ---\n{trace}",
        lines.len(),
        collected.join("\n")
    );
}

/// B203's fixture: two legs where the SERVER's compile reads the CLIENT's
/// `dist/` bundle — the shape the item names, a server leg serving the client
/// bundle it was built beside.
///
/// `root = "."` puts the package's source root at the project directory, so
/// `dist/` is inside it and `const asset::read("dist/client.js")` names the
/// client's artifact by the ordinary rule (const paths resolve against the
/// package root and may not climb out of it). Nothing about the bug needs that
/// layout — it is simply the shortest spelling of "one leg reads another leg's
/// output" that the const channel's own path rule allows.
fn build_artifact_dependent_fixture(dir: &Path) {
    write(
        dir,
        "vilan.toml",
        "[package]\nname = \"app\"\nroot = \".\"\n\n\
         [entry.client]\npath = \"client.vl\"\ntarget = \"browser\"\n\n\
         [entry.server]\npath = \"server.vl\"\n",
    );
    write(dir, "only_client.vl", "fun client_value(): i32 {\n\t2\n}\n");
    write(
        dir,
        "client.vl",
        "import std::io::print;\nimport pkg::only_client::client_value;\n\n\
         fun main() {\n\tprint(client_value());\n}\n",
    );
    write(
        dir,
        "server.vl",
        "import std::asset;\nimport std::io::print;\n\n\
         let BUNDLE = const asset::read(\"dist/client.js\");\n\n\
         fun main() {\n\tprint(BUNDLE);\n}\n",
    );
    // The bundle the server's const read consumes on ROUND 1. A compile-time
    // read of a file that is not there fails the compile, and a project shaped
    // like this one has its last build's `dist/` on disk — so the fixture puts
    // one there rather than pretending the first round can invent it.
    write(dir, "dist/client.js", "// the previous build's bundle\n");
}

/// B203 — the leg-skip set was decided for EVERY leg before ANY leg compiled.
///
/// The server's recorded sources include `dist/client.js`, which the client leg
/// writes. Asked before the round began, the server's freshness question was
/// answered against the bundle the client was about to overwrite: every source
/// re-hashed, the server was judged Fresh, the client then recompiled, and the
/// server's own artifact went on embedding a bundle that no longer existed —
/// until some later edit happened to reach the server's own modules.
///
/// So the pin is the item's: edit a module ONLY THE CLIENT loads, and the
/// server must recompile in the SAME round. It is the exact edit M22's first row
/// uses to prove the opposite ("one leg, not three"), which is what makes the
/// pair honest — reuse is still by content, and the server recompiles here
/// because one of its contents is about to change, not because reuse was
/// weakened.
#[test]
fn a_leg_that_reads_another_legs_artifact_recompiles_in_the_same_round() {
    let dir = temp_project("artifact_dep");
    build_artifact_dependent_fixture(&dir);

    let trace_path = dir.join("watch-trace.log");
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--watch", dir.to_str().unwrap()])
        .env("VILAN_WATCH_LOG", &trace_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the watcher");
    let receiver = spawn_reader(watcher.stdout.take().expect("piped stdout"));

    let started = Instant::now();
    let first = next_round(
        &receiver,
        2,
        "the initial build",
        None,
        support::WATCH_LIVENESS,
        &trace_path,
    );
    let budget = support::round_budget(started.elapsed());
    assert!(
        first.iter().all(|leg| leg.rebuilt),
        "the first round must compile both legs: {first:?}"
    );

    const ONLY_CLIENT: &str = "fun client_value(): i32 {\n\t3\n}\n";
    let only_client = dir.join("only_client.vl");
    let mut retouches = 0;
    std::fs::write(&only_client, ONLY_CLIENT).expect("edit the client-only module");
    let round = next_round(
        &receiver,
        2,
        "the round the client-only edit starts",
        Some((&only_client, ONLY_CLIENT, &mut retouches)),
        budget,
        &trace_path,
    );

    assert!(
        round.iter().all(|leg| leg.rebuilt),
        "the client recompiled and the server READS the client's bundle, so the \
         server recompiles in the same round — a `Fresh` server here is an \
         artifact built from a bundle that no longer exists (B203): {round:?}"
    );
    // And the producer went first, which is what makes the consumer's compile
    // read this round's bundle rather than the last one's.
    assert!(
        round[0].entry.ends_with("client.vl"),
        "the leg whose artifact the other reads compiles first: {round:?}"
    );

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
}

/// M22, both rows: a client-only module edit re-emits the client leg only; a
/// shared module edit re-emits every reaching leg.
#[test]
fn a_watch_round_recompiles_the_legs_the_edit_reached() {
    let dir = temp_project("legs");
    build_fixture(&dir);

    let trace_path = dir.join("watch-trace.log");
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--watch", dir.to_str().unwrap()])
        // The loop's own trace of what each poll SAW (B208). stderr is nulled
        // here — the pin reads leg lines off stdout — so without this a timeout
        // could say only "no leg lines", which is the symptom four different
        // bugs share. The name is not `.vl`, so the watched set is unperturbed.
        .env("VILAN_WATCH_LOG", &trace_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the watcher");
    let receiver = spawn_reader(watcher.stdout.take().expect("piped stdout"));

    // Round 1 is a full build of both legs — and its cost is what every later
    // budget is expressed in (E32's rule: calibrate against this machine's own
    // round rather than guessing a number about the box).
    let started = Instant::now();
    let first = next_round(
        &receiver,
        2,
        "the initial build",
        None,
        support::WATCH_LIVENESS,
        &trace_path,
    );
    let budget = support::round_budget(started.elapsed());
    assert!(
        first.iter().all(|leg| leg.rebuilt),
        "the first round must compile both legs: {first:?}"
    );

    // --- Row 1: a module only the client leg imports. ---
    const ONLY_CLIENT: &str = "fun client_value(): i32 {\n\t3\n}\n";
    let only_client = dir.join("src/only_client.vl");
    let mut retouches = 0;
    std::fs::write(&only_client, ONLY_CLIENT).expect("edit the client-only module");
    let round = next_round(
        &receiver,
        2,
        "the client-only round",
        Some((&only_client, ONLY_CLIENT, &mut retouches)),
        budget,
        &trace_path,
    );
    let rebuilt: Vec<&LegLine> = round.iter().filter(|leg| leg.rebuilt).collect();
    assert_eq!(
        rebuilt.len(),
        1,
        "an edit in a module only the client leg loads must recompile ONE leg \
         (M22); this round recompiled {}: {round:?}",
        rebuilt.len()
    );
    assert!(
        rebuilt[0].entry.ends_with("client.vl"),
        "the one recompiled leg must be the client: {round:?}"
    );

    // --- Row 2: a module BOTH legs import. ---
    const SHARED: &str = "fun shared_value(): i32 {\n\t4\n}\n";
    let shared = dir.join("src/shared.vl");
    let mut shared_retouches = 0;
    std::fs::write(&shared, SHARED).expect("edit the shared module");
    let round = next_round(
        &receiver,
        2,
        "the shared round",
        Some((&shared, SHARED, &mut shared_retouches)),
        budget,
        &trace_path,
    );
    assert!(
        round.iter().all(|leg| leg.rebuilt),
        "an edit in a module BOTH legs load must recompile both — reuse is not \
         allowed to shorten a round that reached every leg: {round:?}"
    );

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
}

/// B276's fixture: a client leg that BUNDLES — one file that is not valid
/// UTF-8 (a `.woff2`, written here as raw bytes) and one that is (a `.txt`) —
/// beside a module only the SERVER leg loads.
///
/// Both kinds are present deliberately. The binary one is what made the bug
/// total on kolt (`read_source` cannot decode a font at all, so the re-hash was
/// `None` and the leg was disqualified outright); the text one is what makes
/// the bug a HASHING bug rather than a decoding one, because `content_hash` of
/// a `str` and `content_hash_bytes` of its bytes disagree on plain ASCII too.
/// A fix that only taught the watch loop to read bytes would pass the first row
/// and fail the second.
fn build_bundling_fixture(dir: &Path) {
    write(
        dir,
        "vilan.toml",
        "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    // A `.woff2` header followed by bytes no UTF-8 decoder accepts — the shape
    // of the thing, without carrying a font into the tree.
    let font = dir.join("src/static/font/body.woff2");
    std::fs::create_dir_all(font.parent().unwrap()).unwrap();
    std::fs::write(&font, [b'w', b'O', b'F', b'2', 0x00, 0xff, 0xfe, 0x80]).unwrap();
    write(dir, "src/static/robots.txt", "User-agent: *\n");
    write(
        dir,
        "src/only_server.vl",
        "fun server_value(): i32 {\n\t1\n}\n",
    );
    write(
        dir,
        "src/client.vl",
        "import std::asset::bundle;\nimport std::io::print;\n\n\
         const {\n\tbundle(\"static/font/body.woff2\");\n\tbundle(\"static/robots.txt\");\n};\n\n\
         fun main() {\n\tprint(\"client\");\n}\n",
    );
    write(
        dir,
        "src/server.vl",
        "import std::io::print;\nimport pkg::only_server::server_value;\n\n\
         fun main() {\n\tprint(server_value());\n}\n",
    );
}

/// B276 — a leg that bundles anything was never `Fresh`, however little
/// changed.
///
/// `asset::bundle` recorded its files under `content_hash_bytes`; the watch
/// loop re-hashed every recorded input with `read_source` + `content_hash`. The
/// two never agreed, so `leg_is_current` failed on the bundled row of every
/// round and kolt — which bundles a favicon, three fonts, an svg and a
/// manifest — reused nothing for the life of a session.
///
/// Both directions are pinned here, because a change detector that never fires
/// and one that always fires are the same bug wearing different clothes:
/// 1. an edit to a module only the SERVER loads leaves the client `Fresh`, and
/// 2. an edit to the BUNDLED BINARY recompiles the client, which is reuse still
///    being decided by content (E12's rule) and the bundled copy in `dist/`
///    still being the bytes on disk.
#[test]
fn a_bundling_leg_is_fresh_on_a_round_that_did_not_touch_it() {
    let dir = temp_project("bundling");
    build_bundling_fixture(&dir);

    let trace_path = dir.join("watch-trace.log");
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--watch", dir.to_str().unwrap()])
        .env("VILAN_WATCH_LOG", &trace_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the watcher");
    let receiver = spawn_reader(watcher.stdout.take().expect("piped stdout"));

    let started = Instant::now();
    let first = next_round(
        &receiver,
        2,
        "the initial build",
        None,
        support::WATCH_LIVENESS,
        &trace_path,
    );
    let budget = support::round_budget(started.elapsed());
    assert!(
        first.iter().all(|leg| leg.rebuilt),
        "the first round must compile both legs: {first:?}"
    );

    // --- Row 1: the edit reaches the server leg alone. ---
    const ONLY_SERVER: &str = "fun server_value(): i32 {\n\t2\n}\n";
    let only_server = dir.join("src/only_server.vl");
    let mut retouches = 0;
    std::fs::write(&only_server, ONLY_SERVER).expect("edit the server-only module");
    let round = next_round(
        &receiver,
        2,
        "the server-only round",
        Some((&only_server, ONLY_SERVER, &mut retouches)),
        budget,
        &trace_path,
    );
    let client = round
        .iter()
        .find(|leg| leg.entry.ends_with("client.vl"))
        .unwrap_or_else(|| panic!("the round must name the client leg: {round:?}"));
    assert!(
        !client.rebuilt,
        "the client leg BUNDLES and nothing it loads changed, so it must be \
         `Fresh` — a rebuilt client here is B276: its bundled files re-hash \
         under a rule the compile did not record them with: {round:?}"
    );
    let server = round
        .iter()
        .find(|leg| leg.entry.ends_with("server.vl"))
        .unwrap_or_else(|| panic!("the round must name the server leg: {round:?}"));
    assert!(
        server.rebuilt,
        "the edited module is the server's, so the server recompiles: {round:?}"
    );

    // --- Row 2: the edit is the bundled BINARY itself. ---
    let font = dir.join("src/static/font/body.woff2");
    std::fs::write(&font, [b'w', b'O', b'F', b'2', 0x01, 0xfd, 0xfc, 0x81])
        .expect("edit the bundled font");
    let deadline = Instant::now() + budget;
    let mut rounds = 0;
    let client_recompiled = loop {
        if Instant::now() >= deadline {
            break false;
        }
        let round = next_round(
            &receiver,
            2,
            "the bundled-asset round",
            None,
            budget,
            &trace_path,
        );
        rounds += 1;
        if round
            .iter()
            .any(|leg| leg.entry.ends_with("client.vl") && leg.rebuilt)
        {
            break true;
        }
    };
    assert!(
        client_recompiled,
        "a changed BUNDLED file must recompile the leg that bundles it — reuse \
         is decided by content, and a `Fresh` client here would serve a font \
         its build no longer matches ({rounds} rounds seen)"
    );
    assert_eq!(
        std::fs::read(dir.join("dist/static/font/body.woff2")).expect("the bundled copy"),
        [b'w', b'O', b'F', b'2', 0x01, 0xfd, 0xfc, 0x81],
        "the recompiled leg re-copies the edited file into `dist/`"
    );

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The invariant under B276's fix, asked of the two functions directly: the
/// hash a tracked build input is RECORDED with and the hash it is RE-VERIFIED
/// with are the same hash, for every kind of file the const channel can touch.
///
/// This is the pin that would have caught the bug at the seam rather than three
/// layers up in a watch round. It goes red if either side grows an arm the
/// other does not have — which is exactly how the two came apart: `bundle`
/// gained a bytes hash and the watch loop was never told.
#[test]
fn hashing_agrees_between_the_recording_side_and_the_watch_side() {
    use vilan_core::const_eval::{tracked_input_hash, tracked_input_hash_of_bytes};

    let dir = temp_project("hash_agreement");
    std::fs::create_dir_all(&dir).unwrap();

    // Plain text (what `asset::read` and every `.vl` module are), text with a
    // BOM (`windows-support.md` §2 — the compiler never sees the marker, so
    // neither may the hash), and bytes no decoder accepts (a font, an image).
    let rows: [(&str, Vec<u8>); 3] = [
        ("plain.txt", b"User-agent: *\n".to_vec()),
        ("bom.txt", "\u{feff}fun main() {}\n".as_bytes().to_vec()),
        (
            "body.woff2",
            vec![b'w', b'O', b'F', b'2', 0x00, 0xff, 0xfe, 0x80],
        ),
    ];
    for (name, bytes) in &rows {
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(
            tracked_input_hash(&path),
            Some(tracked_input_hash_of_bytes(bytes)),
            "`{name}`: the watch side must re-hash to what the compile recorded"
        );
    }

    // A `.vl` module's row in the same map is recorded as `content_hash` of the
    // text the compiler consumed — the text arm's answer for the same file, or
    // a module edit and a bundled-file edit could not share one map.
    let module = dir.join("module.vl");
    let source = "fun main() {\n\tprint(\"hi\")\n}\n";
    std::fs::write(&module, source).unwrap();
    assert_eq!(
        tracked_input_hash(&module),
        Some(vilan_core::content_hash(source)),
        "a module row and a const-input row must agree on what one file hashes to"
    );

    // A BOM'd module hashes as the text the lexer sees, not as the file.
    let bom_module = dir.join("bom.vl");
    std::fs::write(&bom_module, format!("\u{feff}{source}")).unwrap();
    assert_eq!(
        tracked_input_hash(&bom_module),
        Some(vilan_core::content_hash(source)),
        "the BOM is an encoding marker, so it cannot move the hash"
    );

    // Gone is `None`, which is what disqualifies a reuse by construction.
    assert_eq!(tracked_input_hash(&dir.join("absent.txt")), None);

    // A DIRECTORY re-hashes as its listing (`asset::read_dir`), and a file
    // appearing in it moves that hash.
    let listed = tracked_input_hash(&dir).expect("a directory hashes as its listing");
    std::fs::write(dir.join("new.txt"), "x").unwrap();
    assert_ne!(
        tracked_input_hash(&dir),
        Some(listed),
        "a file appearing in a listed directory must fail the compare"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
