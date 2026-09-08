//! `vilan build` compiles a tier of its legs at once (M51).
//!
//! `build`'s loop was one thread walking [`leg_schedule`]'s order, and every
//! obstacle to overlapping it is an ORDER-dependent accumulator rather than the
//! compile itself: one `dist/` three writers share, a bundled-name map each leg
//! writes and every later leg reads, a `--explain` log appended to as files are
//! written, a watch record, and `?`'s abort at the first failing leg. So the
//! step is narrow — the COMPILES of one tier overlap, every WRITER stays serial
//! and in schedule order — and this file is the claim that nothing about the
//! scheduler reaches `dist/` or the terminal.
//!
//! Every pin is a comparison against the same round run with
//! `VILAN_SEQUENTIAL_BUILD=1`, which is the build this replaces: the bytes in
//! `dist/`, the leg a collision is blamed on, the leg an abort names, and the
//! legs a watch round reuses.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{ChildStdout, Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod support;

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("vilan_m51_{tag}_{}_{unique}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

/// `vilan build <dir>`, with the legs compiled one after another as they were
/// before M51 when `sequential` — the reference every pin here compares to.
fn build(dir: &Path, sequential: bool) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .args(["build", dir.to_str().unwrap()])
        .env("NO_COLOR", "1");
    if sequential {
        command.env("VILAN_SEQUENTIAL_BUILD", "1");
    }
    command.output().expect("run vilan build")
}

/// Every file under `dir` by its path relative to `dir`, with its bytes.
fn tree(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&next) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let relative = path
                .strip_prefix(dir)
                .expect("walked from `dir`")
                .to_string_lossy()
                .into_owned();
            files.insert(relative, std::fs::read(&path).unwrap_or_default());
        }
    }
    files
}

/// A three-leg package in kolt's shape — a browser `client`, and `probe` and
/// `server` on node — sharing a module all three reach. Three legs that read
/// none of each other's artifacts, which is the widest tier a workspace has and
/// so the shape the overlap is for.
///
/// `bundled` gives each leg a `const asset::bundle_as` resource of its own, so
/// the round exercises the collision map and `write_bundled`'s copies. It is
/// OFF for the watch pin: a leg that bundles anything recompiles every round
/// today, whatever the schedule (the bundled input is recorded with a
/// byte-hash and re-hashed as text), which would make a reuse comparison
/// compare nothing.
fn three_leg_workspace(dir: &Path, bundled: bool) {
    write(
        dir,
        "vilan.toml",
        "[package]\nname = \"legs\"\n\n\
         [entry.client]\ntarget = \"browser\"\n\n\
         [entry.probe]\n\n\
         [entry.server]\n",
    );
    write(
        dir,
        "src/shared.vl",
        "fun greeting(who: str): str {\n\t\"hello, \" + who\n}\n",
    );
    let carry = |leg: &str| {
        if bundled {
            format!(
                "let RESOURCE = const asset::bundle_as(\"{leg}.txt\", \"/{leg}-resource.txt\");\n"
            )
        } else {
            String::new()
        }
    };
    let import_asset = if bundled { "import std::asset;\n" } else { "" };
    if bundled {
        for leg in ["client", "probe", "server"] {
            write(
                dir,
                &format!("src/{leg}.txt"),
                &format!("the {leg}'s resource\n"),
            );
        }
    }
    write(
        dir,
        "src/client.vl",
        &format!(
            "{import_asset}\
             import std::ui::{{ mount_root, view }};\n\
             import pkg::shared::greeting;\n\
             \n\
             {}\
             fun main() {{\n\
             \tlet _root = mount_root(\"app\", || view(\"a\").text(greeting(\"client\")){});\n\
             }}\n",
            carry("client"),
            if bundled {
                ".attr(\"href\", RESOURCE)"
            } else {
                ""
            }
        ),
    );
    for leg in ["probe", "server"] {
        write(
            dir,
            &format!("src/{leg}.vl"),
            &format!(
                "{import_asset}\
                 import std::io::print;\n\
                 import pkg::shared::greeting;\n\
                 \n\
                 {}\
                 fun main() {{\n\
                 \tprint(greeting(\"{leg}\"));\n\
                 {}\
                 }}\n",
                carry(leg),
                if bundled { "\tprint(RESOURCE);\n" } else { "" }
            ),
        );
    }
}

/// **The corpus's bar, for a directory.** The round that compiles its legs at
/// once writes the bytes the round that compiled them one after another wrote —
/// the same files, with the same contents.
///
/// Built into ONE tree, twice, rather than into two trees: a build's output can
/// mention the paths it was given, and two directories would compare a
/// difference this pin is not about.
#[test]
fn a_three_leg_workspace_writes_the_same_dist_either_way() {
    let dir = temp_project("dist_identical");
    three_leg_workspace(&dir, true);

    let serial = build(&dir, true);
    let serial_stderr = String::from_utf8_lossy(&serial.stderr).into_owned();
    assert!(
        serial.status.success(),
        "the sequential build is the reference and must be green:\n{serial_stderr}"
    );
    let reference = tree(&dir.join("dist"));
    assert!(
        reference.contains_key("client.js")
            && reference.contains_key("server.mjs")
            && reference.contains_key("probe.mjs"),
        "the fixture must actually build three legs: {:?}",
        reference.keys().collect::<Vec<_>>()
    );

    std::fs::remove_dir_all(dir.join("dist")).expect("clear dist");
    let parallel = build(&dir, false);
    let parallel_stderr = String::from_utf8_lossy(&parallel.stderr).into_owned();
    let observed = tree(&dir.join("dist"));
    let serial_stdout = String::from_utf8_lossy(&serial.stdout).into_owned();
    let parallel_stdout = String::from_utf8_lossy(&parallel.stdout).into_owned();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        parallel.status.success(),
        "and so must the parallel one:\n{parallel_stderr}"
    );
    assert_eq!(
        observed.keys().collect::<Vec<_>>(),
        reference.keys().collect::<Vec<_>>(),
        "the same files"
    );
    for (name, bytes) in &reference {
        assert_eq!(
            observed.get(name).map(Vec::as_slice),
            Some(bytes.as_slice()),
            "`dist/{name}` differs between a parallel round and a sequential one"
        );
    }
    // And the round's own account of itself, which is written by the serial
    // half of the loop and so is a claim about the schedule, not the scheduler.
    assert_eq!(
        parallel_stdout, serial_stdout,
        "the build's lines are the sequential round's, in the sequential order"
    );
}

/// **The collision blame.** `bundled_names` is written by each leg and read by
/// every later one, so the leg blamed for a two-files-one-url collision is
/// whichever leg reached the copy FIRST. Moving the copy onto the compile
/// threads would make that the scheduler's answer; keeping every writer serial
/// and in schedule order makes it the schedule's.
///
/// The plant is the fixture's own shape rather than a contrivance: `client`
/// comes first in the schedule and is by far the slower leg (a browser bundle
/// over `std::ui` against a node leg that imports `std::io`), so a round that
/// blamed in completion order would name the two files the other way round
/// essentially every time.
#[test]
fn a_bundled_name_collision_names_the_same_leg_either_way() {
    let dir = temp_project("collision_blame");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"collide\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n",
    );
    write(&dir, "src/first.txt", "the client's\n");
    write(&dir, "src/second.txt", "the server's\n");
    write(
        &dir,
        "src/client.vl",
        "import std::asset;\n\
         import std::ui::{ mount_root, view };\n\
         \n\
         let ONE = const asset::bundle_as(\"first.txt\", \"/pinned.txt\");\n\
         \n\
         fun main() {\n\
         \tlet _root = mount_root(\"app\", || view(\"a\").attr(\"href\", ONE));\n\
         }\n",
    );
    write(
        &dir,
        "src/server.vl",
        "import std::asset;\n\
         import std::io::print;\n\
         \n\
         let TWO = const asset::bundle_as(\"second.txt\", \"/pinned.txt\");\n\
         \n\
         fun main() {\n\
         \tprint(TWO);\n\
         }\n",
    );

    let serial = build(&dir, true);
    let serial_stderr = String::from_utf8_lossy(&serial.stderr).into_owned();
    let _ = std::fs::remove_dir_all(dir.join("dist"));
    let parallel = build(&dir, false);
    let parallel_stderr = String::from_utf8_lossy(&parallel.stderr).into_owned();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        !serial.status.success() && !parallel.status.success(),
        "one `dist/` cannot serve two files on one url, either way:\n\
         {serial_stderr}\n---\n{parallel_stderr}"
    );
    let first_at = serial_stderr.find("first.txt");
    let second_at = serial_stderr.find("second.txt");
    assert!(
        serial_stderr.contains("both bundle to `pinned.txt`") && first_at < second_at,
        "the sequential round blames the leg the schedule put FIRST — `first.txt` is \
         the name already taken and `second.txt` the one refused; got:\n{serial_stderr}"
    );
    assert_eq!(
        parallel_stderr, serial_stderr,
        "and so does the parallel round — the blame is the schedule's, not the \
         scheduler's"
    );
}

/// **The abort.** A serial round stops at the first leg that fails, so the legs
/// after it never compile and never say anything. A parallel round compiles a
/// whole tier before it can know which leg failed, so it has diagnostics from
/// legs a serial round never reached — and must throw them away. Reported in
/// SCHEDULE order, stopping at the first failure, is what makes the terminal
/// the serial round's.
#[test]
fn an_aborting_round_names_the_same_leg_either_way() {
    let dir = temp_project("abort_order");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"aborts\"\n\n\
         [entry.client]\ntarget = \"browser\"\n\n\
         [entry.probe]\n\n\
         [entry.server]\n",
    );
    write(
        &dir,
        "src/client.vl",
        "import std::ui::{ mount_root, view };\n\n\
         fun main() {\n\tlet _root = mount_root(\"app\", || view(\"a\"));\n}\n",
    );
    // `probe` fails, and `server` — after it in the schedule — fails too. A
    // serial round never reaches the server's mistake, so neither may this one.
    write(
        &dir,
        "src/probe.vl",
        "import std::io::print;\n\n\
         fun probe_oops(): i32 {\n\t\"the probe's mistake\"\n}\n\n\
         fun main() {\n\tprint(\"probe\");\n}\n",
    );
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\n\
         fun server_oops(): i32 {\n\t\"the server's mistake\"\n}\n\n\
         fun main() {\n\tprint(\"server\");\n}\n",
    );

    let serial = build(&dir, true);
    let serial_stderr = String::from_utf8_lossy(&serial.stderr).into_owned();
    let _ = std::fs::remove_dir_all(dir.join("dist"));
    let parallel = build(&dir, false);
    let parallel_stderr = String::from_utf8_lossy(&parallel.stderr).into_owned();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(
        !serial.status.success() && !parallel.status.success(),
        "both rounds fail"
    );
    assert!(
        serial_stderr.contains("the probe's mistake"),
        "the sequential round aborts at the probe; got:\n{serial_stderr}"
    );
    assert!(
        !serial_stderr.contains("the server's mistake"),
        "and never reaches the server; got:\n{serial_stderr}"
    );
    assert_eq!(
        parallel_stderr, serial_stderr,
        "the parallel round compiled the server too, and must report exactly \
         what the sequential round reported"
    );
}

/// **The artifact edge, on a round that cannot know about it.** A leg's sources
/// can include another leg's `dist/` file, and the schedule's edges come from
/// the PREVIOUS round's record — which a plain `vilan build` never has. So a
/// tier's compiles are speculative: they read `dist/` as the tier found it, and
/// the guard is the check the compile makes possible, against the sources it
/// actually loaded. A leg that read a file an earlier leg of its own tier has
/// just written is compiled AGAIN, where a serial round compiled it.
///
/// The pin is what the server's artifact EMBEDS: build, change the marker only
/// the client's bundle carries, build again. Without the guard the server's
/// second build embeds the FIRST build's client bundle — the marker the round
/// just replaced.
#[test]
fn a_leg_that_reads_another_legs_artifact_gets_this_rounds_bytes() {
    let dir = temp_project("artifact_edge");
    write(
        &dir,
        "vilan.toml",
        "[package]\nname = \"edged\"\nroot = \".\"\n\n\
         [entry.client]\npath = \"client.vl\"\ntarget = \"browser\"\n\n\
         [entry.server]\npath = \"server.vl\"\n",
    );
    let client = |marker: &str| {
        format!("import std::io::print;\n\nfun main() {{\n\tprint(\"{marker}\");\n}}\n")
    };
    write(&dir, "client.vl", &client("marker-ALPHA"));
    write(
        &dir,
        "server.vl",
        "import std::asset;\nimport std::io::print;\n\n\
         let BUNDLE = const asset::read(\"dist/client.js\");\n\n\
         fun main() {\n\tprint(BUNDLE);\n}\n",
    );
    // A compile-time read of a file that is not there fails the compile, and a
    // project shaped like this one has its last build's `dist/` on disk.
    write(&dir, "dist/client.js", "// the previous build's bundle\n");

    let first = build(&dir, false);
    assert!(
        first.status.success(),
        "the first build is green:\n{}",
        String::from_utf8_lossy(&first.stderr)
    );
    write(&dir, "client.vl", &client("marker-BETA"));
    let second = build(&dir, false);
    let stderr = String::from_utf8_lossy(&second.stderr).into_owned();
    let server = std::fs::read_to_string(dir.join("dist/server.mjs")).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);

    assert!(second.status.success(), "and so is the second:\n{stderr}");
    assert!(
        server.contains("marker-BETA"),
        "the server embeds THIS round's client bundle"
    );
    assert!(
        !server.contains("marker-ALPHA"),
        "and not the one the round replaced"
    );
}

// --- a watch round's state -------------------------------------------------

/// One leg line of a round: `Compiled <entry> -> <artifact>` when the leg was
/// rebuilt, `Fresh <entry> -> <artifact>` when it was reused — the only window
/// onto the per-leg watch record the round merges after its join.
fn leg_line(line: &str) -> Option<String> {
    let mut plain = String::with_capacity(line.len());
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            for next in characters.by_ref() {
                if next == 'm' {
                    break;
                }
            }
            continue;
        }
        plain.push(character);
    }
    let (verb, rest) = plain.trim().split_once(' ')?;
    if !matches!(verb, "Compiled" | "Fresh") || !rest.contains(" -> ") {
        return None;
    }
    let entry = rest.split(" -> ").next()?.trim();
    let name = Path::new(entry)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())?;
    Some(format!("{verb} {name}"))
}

fn spawn_reader(stdout: ChildStdout) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    receiver
}

/// The next `legs` leg lines, or a description of what arrived instead.
fn next_round(
    receiver: &mpsc::Receiver<String>,
    legs: usize,
    deadline: Duration,
    what: &str,
) -> Vec<String> {
    let started = Instant::now();
    let mut round = Vec::new();
    while round.len() < legs {
        let left = deadline
            .checked_sub(started.elapsed())
            .unwrap_or(Duration::ZERO);
        match receiver.recv_timeout(left) {
            Ok(line) => {
                if let Some(leg) = leg_line(&line) {
                    round.push(leg);
                }
            }
            Err(_) => panic!("timed out waiting for {what}; got {round:?}"),
        }
    }
    round
}

/// A `build --watch` round's per-leg record decides which legs the NEXT round
/// reuses, and the compiles that fill it now run on threads of their own. The
/// record is therefore built into a per-leg slot and merged after the join, in
/// DECLARATION order — never held by a worker. The pin is the record's only
/// observable: the legs round 2 reuses after an edit that reaches exactly one
/// of them, and that they are the legs a sequential watcher reused.
#[test]
fn a_watch_round_reuses_the_same_legs_either_way() {
    let rounds: Vec<(Vec<String>, Vec<String>)> = [true, false]
        .into_iter()
        .map(|sequential| {
            let dir = temp_project(if sequential {
                "watch_serial"
            } else {
                "watch_par"
            });
            three_leg_workspace(&dir, false);
            // A module only the client reaches, so round 2 has one leg to
            // recompile and two to reuse.
            write(&dir, "src/only_client.vl", "fun tint(): i32 {\n\t2\n}\n");
            write(
                &dir,
                "src/client.vl",
                "import std::ui::{ mount_root, view };\n\
                 import pkg::only_client::tint;\n\
                 \n\
                 fun main() {\n\
                 \tlet _root = mount_root(\"app\", || view(\"a\").text(i\"{tint()}\"));\n\
                 }\n",
            );
            let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
            command
                .args(["build", "--watch", dir.to_str().unwrap()])
                .env("NO_COLOR", "1")
                .stdout(Stdio::piped())
                .stderr(Stdio::null());
            if sequential {
                command.env("VILAN_SEQUENTIAL_BUILD", "1");
            }
            let mut watcher = command.spawn().expect("spawn the watcher");
            let receiver = spawn_reader(watcher.stdout.take().expect("piped stdout"));
            let started = Instant::now();
            let first = next_round(&receiver, 3, support::WATCH_LIVENESS, "the initial build");
            let budget = support::round_budget(started.elapsed());

            let module = dir.join("src/only_client.vl");
            let mut round = Vec::new();
            for tick in 0..6 {
                std::fs::write(&module, format!("fun tint(): i32 {{\n\t{}\n}}\n", tick + 3))
                    .expect("edit the client-only module");
                let left = budget.checked_div(3).unwrap_or(budget);
                let started = Instant::now();
                let mut collected = Vec::new();
                while collected.len() < 3 && started.elapsed() < left {
                    let wait = left
                        .checked_sub(started.elapsed())
                        .unwrap_or(Duration::ZERO);
                    if let Ok(line) = receiver.recv_timeout(wait)
                        && let Some(leg) = leg_line(&line)
                    {
                        collected.push(leg);
                    }
                }
                if collected.len() == 3 {
                    round = collected;
                    break;
                }
            }
            support::kill_watcher(&mut watcher);
            let _ = std::fs::remove_dir_all(&dir);
            assert_eq!(first.len(), 3, "round 1 compiles three legs: {first:?}");
            assert_eq!(round.len(), 3, "round 2 reports three legs: {round:?}");
            (first, round)
        })
        .collect();

    let (serial_first, serial_round) = &rounds[0];
    let (parallel_first, parallel_round) = &rounds[1];
    assert!(
        serial_first.iter().all(|leg| leg.starts_with("Compiled")),
        "the first round compiles every leg: {serial_first:?}"
    );
    assert_eq!(
        serial_round
            .iter()
            .filter(|leg| leg.starts_with("Fresh"))
            .count(),
        2,
        "the edit reaches one leg, so two are reused: {serial_round:?}"
    );
    assert_eq!(
        parallel_first, serial_first,
        "round 1 is the sequential round's"
    );
    assert_eq!(
        parallel_round, serial_round,
        "and so is the record round 2 read — the same legs reused, in the same \
         order, whatever thread each leg compiled on"
    );
}
