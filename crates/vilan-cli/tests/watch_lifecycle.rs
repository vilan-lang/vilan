//! End-to-end pins for how a `--watch` session *ends* (`windows-support.md` §6).
//!
//! `watch_loop` never returns from its loop, so `Ctrl-C` is the only way a watch
//! session finishes — and before this slice that path ran no cleanup at all,
//! leaking one `vilan-watch-<pid>.mjs` per session into the temp directory (S3
//! delivered only the per-*round* delete, which covers restarts).
//!
//! Unix-gated because the pin has to deliver a real interrupt: `SIGINT` via
//! `kill(1)`. The Windows leg of the same handler (`SetConsoleCtrlHandler`, via
//! `ctrlc`) is exercised by a live `Ctrl-C` in S6's Windows pass — a console
//! control event cannot be raised at another process from a test.

#![cfg(unix)]

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

mod support;

/// A single-package project whose program prints once and exits, so the round's
/// `node` child is gone before the interrupt lands: what remains on disk is
/// exactly the temp script, with no orphan to clean up after the assertion.
fn temp_package(tag: &str) -> PathBuf {
    let dir = support::scratch_root().join(format!("vilan_watch_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("vilan.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(
        dir.join("src/main.vl"),
        "import std::io::print;\n\nfun main() {\n\tprint(\"round\");\n}\n",
    )
    .unwrap();
    dir
}

/// Waits (bounded) for `condition`, polling — the watcher compiles before it
/// writes anything, and how long that takes is not this test's business.
///
/// This file reached that conclusion before the rest of the family did, and E39
/// cited its comment while setting `WATCH_LIVENESS`; E40 finishes the trade by
/// giving it the shared bound, because 60 s is still a number about the machine
/// sitting in a test that is about `Ctrl-C`.
fn wait_for(label: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + support::WATCH_LIVENESS;
    while Instant::now() < deadline {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timed out waiting for {label}");
}

#[test]
fn ctrl_c_removes_the_watch_script_and_exits_130() {
    let dir = temp_package("ctrlc");
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", dir.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the watcher");

    // The round's temp script is keyed by the watcher's pid.
    // NOT the harness's scratch root (N86): this path is the BINARY's.
    // `vilan run --watch` writes its script to `env::temp_dir()`
    // (`main.rs`'s `watch_script_path`), and a test that looks anywhere else
    // waits five minutes for a file nothing will write.
    let script = std::env::temp_dir().join(format!("vilan-watch-{}.mjs", watcher.id()));
    wait_for("the watch script to be written", || script.exists());

    let signalled = Command::new("kill")
        .args(["-INT", &watcher.id().to_string()])
        .status()
        .expect("send SIGINT");
    assert!(signalled.success(), "kill -INT must succeed");

    let status = watcher.wait().expect("reap the watcher");
    let _ = std::fs::remove_dir_all(&dir);
    let map = std::env::temp_dir().join(format!("vilan-watch-{}.mjs.map", watcher.id()));

    // Exiting *with a code* is itself the discriminator: without the handler
    // the default SIGINT disposition terminates the process by signal, so
    // `code()` would be `None` — and the script would still be on disk.
    assert_eq!(
        status.code(),
        Some(130),
        "a Ctrl-C'd watch session exits 128 + SIGINT"
    );
    assert!(
        !script.exists(),
        "the session's temp script must not outlive it: {}",
        script.display()
    );
    // E257 S6: nor its source map.
    assert!(!map.exists(), "nor its map: {}", map.display());
    // S7: nor its sidecar.
    let sidecar = std::env::temp_dir().join(format!("vilan-watch-{}.mjs.vdbg.json", watcher.id()));
    assert!(!sidecar.exists(), "nor its sidecar: {}", sidecar.display());
}

/// debug-53's orphan sidecars: a plain `vilan run` had no interrupt hook at
/// all, so a `Ctrl-C` (or a debugger's `SIGTERM`) while its program ran left
/// `vilan-run-<pid>.mjs`, its map and its sidecar in the temp directory.
///
/// `SIGTERM` and `SIGHUP` go to `vilan` ALONE, as a debugger ending a session
/// sends them: since E335 the run forwards them to the program and waits, so
/// the program stops, the run removes the three files on its way out, and it
/// exits with the program's own end (128 + the signal: 143 and 129; the
/// forwarding itself is pinned in `run_signals.rs`). `SIGINT` goes to the
/// run's process GROUP, as a terminal's `Ctrl-C` does: the program stops with
/// it and the run exits with a code either way. Without the hook, every
/// signal killed `vilan` outright: no code, and every file stayed.
#[test]
fn an_interrupted_run_removes_its_script_map_and_sidecar() {
    use std::os::unix::process::CommandExt;
    for (signal, to_the_group) in [("INT", true), ("TERM", false), ("HUP", false)] {
        let dir = temp_package(&format!("interrupted_{signal}"));
        std::fs::write(
            dir.join("src/main.vl"),
            "import std::io::print;\nimport std::time::sleep;\n\nfun main() {\n\tprint(\"started\");\n\tsleep(20000);\n}\n",
        )
        .unwrap();
        let mut run = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["run", dir.to_str().unwrap()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .process_group(0)
            .spawn()
            .expect("spawn the run");
        // The program printed: its script, map and sidecar are on disk and
        // node is running it.
        let lines = lines_of(&mut run);
        let mut seen = Vec::new();
        wait_for_line(&lines, &mut seen, "the program to start", |line| {
            line == "started"
        });
        let script = std::env::temp_dir().join(format!("vilan-run-{}.mjs", run.id()));
        let files = [
            script.clone(),
            std::env::temp_dir().join(format!("vilan-run-{}.mjs.map", run.id())),
            std::env::temp_dir().join(format!("vilan-run-{}.mjs.vdbg.json", run.id())),
        ];
        assert!(
            files.iter().all(|file| file.exists()),
            "SIG{signal}: the run's files exist while it runs: {files:?}"
        );
        let target = if to_the_group {
            format!("-{}", run.id())
        } else {
            run.id().to_string()
        };
        let signalled = Command::new("kill")
            .args([format!("-{signal}"), "--".to_string(), target])
            .status()
            .expect("signal the run");
        assert!(signalled.success(), "kill -{signal} must succeed");
        let status = run.wait().expect("reap the run");
        // The program a lone signal to `vilan` left running.
        let _ = Command::new("kill")
            .args(["-KILL", "--", &format!("-{}", run.id())])
            .status();
        let _ = std::fs::remove_dir_all(&dir);
        if to_the_group {
            assert!(
                status.code().is_some(),
                "SIG{signal}: the run exits, not killed: {status:?}"
            );
        } else {
            let number = if signal == "TERM" { 15 } else { 1 };
            assert_eq!(
                status.code(),
                Some(128 + number),
                "SIG{signal}: the run reports the program's death by the signal"
            );
        }
        for file in &files {
            assert!(
                !file.exists(),
                "SIG{signal}: {} outlived the run",
                file.display()
            );
        }
    }
}

/// A9: the `[build] run` hooks belong to the *round*, not to the session — a
/// watch that rebuilds re-runs them, so a Tailwind bridge regenerates its CSS on
/// every edit rather than once at startup. The hook appends a line per round, so
/// the count is the observation.
#[test]
fn build_hooks_run_once_per_watch_round() {
    let dir = temp_package("hooks");
    std::fs::write(
        dir.join("vilan.toml"),
        "[package]\nname = \"app\"\n\n[build]\nrun = [\"echo round >> rounds.txt\"]\n",
    )
    .unwrap();
    let rounds = dir.join("rounds.txt");

    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["run", "--watch", dir.to_str().unwrap()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the watcher");

    let lines = || {
        std::fs::read_to_string(&rounds)
            .map(|text| text.lines().count())
            .unwrap_or(0)
    };
    wait_for("the first round's hook", || lines() >= 1);

    // A source edit starts a second round, which must run the hook again.
    std::fs::write(
        dir.join("src/main.vl"),
        "import std::io::print;\n\nfun main() {\n\tprint(\"round two\");\n}\n",
    )
    .unwrap();
    wait_for("the second round's hook", || lines() >= 2);

    let _ = Command::new("kill")
        .args(["-INT", &watcher.id().to_string()])
        .status();
    let _ = watcher.wait();
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every line the watcher writes, stdout and stderr together, as it writes it.
fn lines_of(watcher: &mut std::process::Child) -> mpsc::Receiver<String> {
    let (sender, receiver) = mpsc::channel();
    let stdout = watcher.stdout.take().expect("piped stdout");
    let stderr = watcher.stderr.take().expect("piped stderr");
    for stream in [
        Box::new(stdout) as Box<dyn std::io::Read + Send>,
        Box::new(stderr),
    ] {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
    }
    receiver
}

/// Waits (bounded) for a line matching `wanted`, keeping every line read.
fn wait_for_line(
    lines: &mpsc::Receiver<String>,
    seen: &mut Vec<String>,
    label: &str,
    wanted: impl Fn(&str) -> bool,
) {
    let deadline = Instant::now() + support::WATCH_LIVENESS;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        match lines.recv_timeout(left) {
            Ok(line) => {
                let matched = wanted(&line);
                seen.push(line);
                if matched {
                    return;
                }
            }
            Err(_) => panic!("timed out waiting for {label}:\n{}", seen.join("\n")),
        }
    }
}

/// E257 S6: `vilan run --watch --inspect` — every round's program starts
/// under node's inspector again (node announces it once per process), with
/// the round's source map beside its script, so a panic in a later round
/// reports `.vl` lines as `vilan run`'s does.
#[test]
fn inspect_under_watch_reopens_the_inspector_each_round() {
    let dir = temp_package("inspect");
    let mut watcher = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args([
            "run",
            "--watch",
            "--inspect=127.0.0.1:0",
            dir.to_str().unwrap(),
        ])
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the watcher");
    let lines = lines_of(&mut watcher);
    let mut seen = Vec::new();
    let listening = |line: &str| line.starts_with("Debugger listening on ws://127.0.0.1:");
    wait_for_line(&lines, &mut seen, "round one's inspector", listening);
    wait_for_line(&lines, &mut seen, "round one's output", |line| {
        line == "round"
    });
    let script = std::env::temp_dir().join(format!("vilan-watch-{}.mjs", watcher.id()));
    let map = std::env::temp_dir().join(format!("vilan-watch-{}.mjs.map", watcher.id()));
    assert!(
        map.exists(),
        "the round's script has its map: {}",
        map.display()
    );
    let text = std::fs::read_to_string(&script).expect("the round's script");
    assert!(
        text.ends_with(&format!(
            "//# sourceMappingURL=vilan-watch-{}.mjs.map\n",
            watcher.id()
        )),
        "{text}"
    );

    // Round two panics: the inspector opens again, and the stack is `.vl`.
    std::fs::write(
        dir.join("src/main.vl"),
        "import std::io::print;\n\nfun pick(rows: List<i32>, at: usize): i32 {\n\trows[at]\n}\n\nfun main() {\n\tprint(pick([1, 2], 5));\n}\n",
    )
    .unwrap();
    wait_for_line(&lines, &mut seen, "round two's inspector", listening);
    wait_for_line(&lines, &mut seen, "round two's frame", |line| {
        line.trim_start().starts_with("at pick (") && line.contains("main.vl:4:")
    });
    assert_eq!(
        seen.iter().filter(|line| listening(line)).count(),
        2,
        "one inspector per round:\n{}",
        seen.join("\n")
    );
    support::kill_watcher(&mut watcher);
    let _ = std::fs::remove_dir_all(&dir);
}
