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
    panic!(
        "timed out waiting for {label} ({} of {legs} leg lines). \
         Watcher stdout so far:\n{}",
        lines.len(),
        collected.join("\n")
    );
}

/// M22, both rows: a client-only module edit re-emits the client leg only; a
/// shared module edit re-emits every reaching leg.
#[test]
fn a_watch_round_recompiles_the_legs_the_edit_reached() {
    let dir = temp_project("legs");
    build_fixture(&dir);

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--watch", dir.to_str().unwrap()])
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
    );
    assert!(
        round.iter().all(|leg| leg.rebuilt),
        "an edit in a module BOTH legs load must recompile both — reuse is not \
         allowed to shorten a round that reached every leg: {round:?}"
    );

    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
}
