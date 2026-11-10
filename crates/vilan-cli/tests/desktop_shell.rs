//! F1 M2's end-to-end proof: `vilan/examples/desktop` — an in-process
//! `std::http` server and a `std::window` showing its page — built with the
//! Rust backend and RUN in a real window, under a virtual X display.
//!
//! What it proves, by the program's own output, in order:
//!
//! 1. the window opened (`window open on <url>`),
//! 2. the webview asked the in-process server for the page (`served the
//!    page`),
//! 3. the page's script RAN in the webview and reached the server again over
//!    the loopback socket (`the page loaded`),
//!
//! and then that the program ENDS CLEANLY, exit code 0, by each of the two
//! roads a desktop program ends by:
//!
//! - the window closes (`--smoke`: the page presses its own Quit button, the
//!   server's `/quit` closes the window, and the last window closing stops the
//!   program as SIGTERM does);
//! - SIGTERM arrives with the window still open (the graceful stop drains the
//!   server, `main` returns, the platform loop exits).
//!
//! # When it runs
//!
//! On Linux, when `xvfb-run` and `dbus-run-session` are on PATH and
//! `pkg-config` finds `webkit2gtk-4.1` and `gtk+-3.0` (the development
//! packages the webview crate builds against). Anywhere else it is SKIPPED
//! with the reason printed — the Windows shards, a machine without WebKitGTK.
//! `VILAN_REQUIRE_WEBVIEW_E2E=1` turns a skip into a failure: the `webview` CI
//! leg sets it (`scripts/ci-local.sh webview`), so the job that installs the
//! packages cannot go green on a skip.
//!
//! The program runs inside `dbus-run-session`: GTK registers its application
//! on the session bus, and a machine's own bus can stall that (this lane's
//! WSL box did, inside `g_application_register`), while a CI runner has none.
//! `NO_AT_BRIDGE=1` keeps the accessibility bridge off the same bus.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long one run may take before it is a hang. A run takes about a second
/// warm; the bound is for a loaded CI runner.
const RUN_LIMIT: Duration = Duration::from_secs(120);

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Why this machine cannot run the proof, or `Ok` when it can.
fn prerequisites() -> Result<(), String> {
    if !cfg!(target_os = "linux") {
        return Err(
            "the end-to-end run is Linux-only (a virtual X display); macOS and \
                    Windows are verified by compile and lint"
                .to_string(),
        );
    }
    for tool in ["xvfb-run", "dbus-run-session", "pkg-config", "cargo"] {
        let found = Command::new("sh")
            .args(["-c", &format!("command -v {tool}")])
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !found {
            return Err(format!("`{tool}` is not on PATH"));
        }
    }
    let webkit = Command::new("pkg-config")
        .args(["--exists", "webkit2gtk-4.1", "gtk+-3.0"])
        .status()
        .is_ok_and(|status| status.success());
    if !webkit {
        return Err(
            "pkg-config finds no webkit2gtk-4.1 / gtk+-3.0 (install libwebkit2gtk-4.1-dev \
             and libgtk-3-dev)"
                .to_string(),
        );
    }
    Ok(())
}

/// `true` to run; `false` after printing why not. A required run panics
/// instead of skipping.
fn may_run() -> bool {
    match prerequisites() {
        Ok(()) => true,
        Err(reason) if std::env::var_os("VILAN_REQUIRE_WEBVIEW_E2E").is_some() => {
            panic!("VILAN_REQUIRE_WEBVIEW_E2E is set, and the desktop proof cannot run: {reason}")
        }
        Err(reason) => {
            eprintln!("skipped: {reason}");
            false
        }
    }
}

/// Stages the example's tracked sources and builds it with the Rust backend,
/// answering the binary. The webview crate builds from this checkout
/// (`VILAN_RT`), into a target directory of this test's own.
fn build_example() -> PathBuf {
    let scratch = Path::new(env!("CARGO_TARGET_TMPDIR")).join("desktop-shell");
    let staged = scratch.join("example");
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("stage the example");
    let example = repository_root().join("vilan/examples/desktop");
    for file in ["vilan.toml", "main.vl"] {
        std::fs::copy(example.join(file), staged.join(file)).expect("copy the example");
    }
    let target = scratch.join("target");
    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--backend", "rust", "."])
        .current_dir(&staged)
        .env("VILAN_RT", repository_root().join("crates/vilan-rt"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run vilan");
    assert!(
        built.status.success(),
        "the desktop example did not build natively:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    // The project was seeded with the webview crate's audited lockfile.
    let lock = std::fs::read_to_string(staged.join("dist/native/main/Cargo.lock"))
        .expect("the generated project's lockfile");
    assert!(
        lock.contains("name = \"wry\"\nversion = \"0.57.2\""),
        "the generated project resolved a wry other than the audited one"
    );
    let binary = target.join("debug/main");
    assert!(binary.is_file(), "no binary at {}", binary.display());
    binary
}

/// The program, in a private session bus and a virtual display, its stdout
/// arriving line by line on the receiver.
struct Running {
    child: std::process::Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
}

impl Running {
    fn start(binary: &Path, arguments: &[&str]) -> Running {
        let mut child = Command::new("dbus-run-session")
            .arg("--")
            .args(["xvfb-run", "-a"])
            .arg(binary)
            .args(arguments)
            .env("NO_AT_BRIDGE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the desktop program");
        let stdout = child.stdout.take().expect("piped stdout");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Running {
            child,
            lines,
            seen: Vec::new(),
        }
    }

    /// Waits for a line containing `wanted`, recording every line on the way.
    fn wait_for(&mut self, wanted: &str, started: Instant) {
        loop {
            let left = RUN_LIMIT.saturating_sub(started.elapsed());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let found = line.contains(wanted);
                    self.seen.push(line);
                    if found {
                        return;
                    }
                }
                Err(_) => {
                    let _ = self.child.kill();
                    panic!(
                        "the desktop program never printed {wanted:?}; it printed:\n{}",
                        self.seen.join("\n")
                    );
                }
            }
        }
    }

    /// Waits for the program to exit and answers its code.
    fn exit_code(mut self, started: Instant) -> (Option<i32>, Vec<String>) {
        loop {
            if let Some(status) = self.child.try_wait().expect("wait on the program") {
                self.seen.extend(self.lines.try_iter());
                return (status.code(), self.seen);
            }
            if started.elapsed() > RUN_LIMIT {
                let _ = self.child.kill();
                panic!(
                    "the desktop program did not exit; it printed:\n{}",
                    self.seen.join("\n")
                );
            }
            // A bounded poll of a process that has ALREADY been asked to end
            // (the ordering is the output's, above): the interval only sets
            // how soon the exit is noticed.
            std::thread::sleep(Duration::from_millis(50));
        }
    }
}

/// The pids running `binary`, read from `/proc` — the program under
/// `dbus-run-session` and `xvfb-run`, which is the process SIGTERM is for.
fn pids_running(binary: &Path) -> Vec<i32> {
    let binary = binary.canonicalize().expect("the binary's path");
    std::fs::read_dir("/proc")
        .expect("read /proc")
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| exe == binary))
        .collect()
}

fn assert_served_and_loaded(seen: &[String]) {
    let position = |wanted: &str| {
        seen.iter()
            .position(|line| line.contains(wanted))
            .unwrap_or_else(|| panic!("no {wanted:?} line in:\n{}", seen.join("\n")))
    };
    let opened = position("window open on http://127.0.0.1:");
    let served = position("served the page");
    let loaded = position("the page loaded");
    assert!(
        opened < served && served < loaded,
        "the lines came out of order:\n{}",
        seen.join("\n")
    );
}

#[test]
fn the_desktop_example_shows_its_own_page_and_ends_cleanly_both_ways() {
    if !may_run() {
        return;
    }
    let binary = build_example();

    // Road 1: the window closes — the page's Quit, through the server's
    // `/quit` and `Window::close`; the last window closing stops the program.
    let started = Instant::now();
    let mut running = Running::start(&binary, &["--smoke"]);
    running.wait_for("quit", started);
    let (code, seen) = running.exit_code(started);
    assert_served_and_loaded(&seen);
    assert_eq!(
        code,
        Some(0),
        "closing the last window ended the program with {code:?}:\n{}",
        seen.join("\n")
    );

    // Road 2: SIGTERM with the window open and the page loaded.
    let started = Instant::now();
    let mut running = Running::start(&binary, &[]);
    running.wait_for("the page loaded", started);
    let pids = pids_running(&binary);
    assert_eq!(
        pids.len(),
        1,
        "expected one running program, found {pids:?}"
    );
    let signalled = Command::new("kill")
        .args(["-TERM", &pids[0].to_string()])
        .status()
        .expect("run kill");
    assert!(signalled.success(), "kill -TERM {} failed", pids[0]);
    let (code, seen) = running.exit_code(started);
    assert_served_and_loaded(&seen);
    assert!(
        !seen.iter().any(|line| line == "quit"),
        "the SIGTERM run quit through the page instead:\n{}",
        seen.join("\n")
    );
    assert_eq!(
        code,
        Some(0),
        "SIGTERM with a window open ended the program with {code:?}:\n{}",
        seen.join("\n")
    );
}
