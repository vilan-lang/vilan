//! native-ui.md P1's end-to-end proof: `vilan/examples/gfx` — a counter in a
//! window `std::gfx` draws on the GPU — built with the Rust backend and RUN in
//! a real X window under `xvfb-run -a`, on Mesa's software Vulkan, driven by
//! real pointer input injected through the X server (XTEST, by
//! `crates/vilan-rt-gfx/examples/x11_input.rs`).
//!
//! What it proves, by the program's own output (`--smoke` prints a line per
//! frame with what the frame showed, and a line per pointer move):
//!
//! 1. the window opens and draws its first frame;
//! 2. the pointer moving over the button draws ONE frame with the hover; a
//!    second move inside the button changes nothing and draws nothing;
//! 3. a press draws the pressed button; the release over it is a click, and
//!    the count's `Signal` going to 1 draws the new count;
//! 4. the pointer leaving draws the plain button, and an idle second after
//!    that draws no frame at all (the program counts them itself);
//! 5. the window's close message (sent through the X server, as a window
//!    manager sends it) reaches the program as an event and ends it, exit 0;
//! 6. SIGTERM with the window open ends it, exit 0;
//! 7. a failure inside a frame callback (another small program) ends it with
//!    exit code 1 and the failure's message, as a failure in `main` does —
//!    the host caught it out of the turn, ended the window loop and raised it
//!    again;
//! 8. the example's scene drawn OFFSCREEN through the same surface
//!    (`--golden`, no display needed: it also runs the program with no window
//!    system) matches its golden images at 100% and 150% within one per
//!    channel.
//!
//! Frames on change only: every frame after the first must show something
//! the frame before it did not (an X server may expose a new window once more,
//! which repeats the FIRST frame's state and nothing else), and the sequence
//! of states is exactly the five the input produces. Ordering comes from the
//! program's lines — each injection waits for the line its predecessor caused
//! — never from a sleep.
//!
//! # When it runs
//!
//! On Linux with `xvfb-run` and `cargo`, and a software Vulkan adapter (Mesa's
//! lavapipe, `mesa-vulkan-drivers`). Anywhere else it is SKIPPED with the
//! reason printed. `VILAN_REQUIRE_GFX=1` (the `gfx` CI job, through
//! `scripts/ci-local.sh gfx`) turns a skip into a failure. Regenerate the
//! goldens with `VILAN_REGENERATE_GFX_GOLDENS=1` and LOOK at them before
//! committing.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long one run may take before it is a hang. A run takes a few seconds
/// warm (one of them the program's own idle second); the bound is for a
/// loaded CI runner.
const RUN_LIMIT: Duration = Duration::from_secs(120);

/// The window's title, by which the injector finds it.
const TITLE: &str = "vilan gfx";

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn required() -> bool {
    std::env::var_os("VILAN_REQUIRE_GFX").is_some_and(|value| value != "0")
}

/// `true` to go on; `false` after printing why not. A required run panics
/// instead of skipping.
fn may_run(reason: Option<String>) -> bool {
    match reason {
        None => true,
        Some(reason) if required() => {
            panic!("VILAN_REQUIRE_GFX is set, and the GPU window proof cannot run: {reason}")
        }
        Some(reason) => {
            eprintln!("skipped: {reason}");
            false
        }
    }
}

/// Why this machine cannot run the proof at all, before anything is built.
fn tools_missing() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some(
            "the end-to-end run is Linux-only (a virtual X display); macOS and Windows are \
             verified by compile and lint"
                .to_string(),
        );
    }
    for tool in ["xvfb-run", "cargo"] {
        let found = Command::new("sh")
            .args(["-c", &format!("command -v {tool}")])
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        if !found {
            return Some(format!(
                "`{tool}` is not on PATH (Debian/Ubuntu: apt install xvfb)"
            ));
        }
    }
    None
}

/// The scratch every build of this test shares: one target directory, so
/// the GPU stack is compiled once for the example, the failing program and
/// the injector.
fn scratch() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("gfx-window")
}

/// Builds the program in `directory` (an entry `entry`.vl and a vilan.toml)
/// with the Rust backend from this checkout's runtime crates; answers the
/// binary.
fn build_native(directory: &Path, entry: &str) -> PathBuf {
    let target = scratch().join("target");
    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--backend", "rust", &format!("{entry}.vl")])
        .current_dir(directory)
        .env("VILAN_RT", repository_root().join("crates/vilan-rt"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run vilan");
    assert!(
        built.status.success(),
        "{} did not build natively:\n{}{}",
        directory.display(),
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    // The project was seeded with the GPU crate's audited lockfile.
    let lock = std::fs::read_to_string(directory.join(format!("dist/native/{entry}/Cargo.lock")))
        .expect("the generated project's lockfile");
    assert!(
        lock.contains("name = \"wgpu\"\nversion = \"30.0.1\""),
        "the generated project resolved a wgpu other than the audited one"
    );
    let binary = target.join("debug").join(entry);
    assert!(binary.is_file(), "no binary at {}", binary.display());
    binary
}

/// Stages the example's tracked sources and builds it.
fn build_example() -> PathBuf {
    let staged = scratch().join("example");
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("stage the example");
    let example = repository_root().join("vilan/examples/gfx");
    for file in ["vilan.toml", "main.vl"] {
        std::fs::copy(example.join(file), staged.join(file)).expect("copy the example");
    }
    build_native(&staged, "main")
}

/// A program whose frame callback fails, built beside the example.
fn build_failing_program() -> PathBuf {
    let staged = scratch().join("failing");
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("stage the failing program");
    std::fs::write(
        staged.join("vilan.toml"),
        "[package]\nname = \"failing\"\nroot = \".\"\n",
    )
    .expect("write its manifest");
    std::fs::write(staged.join("failing.vl"), FAILING_PROGRAM).expect("write it");
    build_native(&staged, "failing")
}

const FAILING_PROGRAM: &str = concat!(
    "import std::gfx::{ Frame, Window, dp };\n",
    "import std::io::{ panic, print };\n",
    "\n",
    "fun main() {\n",
    "\tmatch Window::builder().title(\"failing\").size(dp(120), dp(80)).open() {\n",
    "\t\tOk(let window) => window.on_frame(|frame: Frame| {\n",
    "\t\t\tprint(\"drawing\");\n",
    "\t\t\tpanic(\"the frame callback failed\");\n",
    "\t\t}),\n",
    "\t\tErr(let reason) => print(i\"no window: {reason}\"),\n",
    "\t}\n",
    "}\n",
);

/// The XTEST injector, built from the GPU crate's examples into the shared
/// target directory.
fn build_injector() -> PathBuf {
    let target = scratch().join("target");
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(["build", "--example", "x11_input", "--manifest-path"])
        .arg(repository_root().join("crates/vilan-rt-gfx/Cargo.toml"))
        .arg("--target-dir")
        .arg(&target)
        .status()
        .expect("run cargo");
    assert!(status.success(), "building the injector failed: {status}");
    target.join("debug/examples/x11_input")
}

/// A program in a virtual display, its stdout arriving line by line. The
/// first line is the wrapper's: the display and its authority file, which
/// the injector connects with.
struct Running {
    child: std::process::Child,
    lines: mpsc::Receiver<String>,
    seen: Vec<String>,
    display: String,
    authority: String,
}

impl Running {
    fn start(binary: &Path, arguments: &[&str]) -> Running {
        let mut child = Command::new("xvfb-run")
            .args(["-a", "-s", "-screen 0 1280x1024x24", "sh", "-c"])
            .arg("printf 'display %s %s\\n' \"$DISPLAY\" \"$XAUTHORITY\"; exec \"$0\" \"$@\"")
            .arg(binary)
            .args(arguments)
            // X11 under xvfb whatever session the suite runs in, at 100%.
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env("WINIT_X11_SCALE_FACTOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("start the program under xvfb-run");
        let stdout = child.stdout.take().expect("piped stdout");
        let (sender, lines) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let started = Instant::now();
        let first = lines
            .recv_timeout(RUN_LIMIT)
            .expect("the wrapper names the display");
        let mut words = first.split_whitespace();
        assert_eq!(words.next(), Some("display"), "{first}");
        let display = words.next().expect("a display").to_string();
        let authority = words.next().unwrap_or_default().to_string();
        assert!(started.elapsed() < RUN_LIMIT);
        Running {
            child,
            lines,
            seen: Vec::new(),
            display,
            authority,
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
                    let stderr = self.stderr();
                    panic!(
                        "the program never printed {wanted:?}; it printed:\n{}\n--- stderr\n{stderr}",
                        self.seen.join("\n")
                    );
                }
            }
        }
    }

    fn stderr(&mut self) -> String {
        let mut text = String::new();
        if let Some(mut stderr) = self.child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        }
        text
    }

    /// Moves, presses, releases or closes, through the X server, and returns
    /// once the server has handled it.
    fn inject(&self, injector: &Path, commands: &[&str]) {
        let status = Command::new(injector)
            .args(["--title", TITLE])
            .args(commands)
            .env("DISPLAY", &self.display)
            .env("XAUTHORITY", &self.authority)
            .status()
            .expect("run the injector");
        assert!(status.success(), "injecting {commands:?} failed: {status}");
    }

    /// Waits for the program to exit: its code, every stdout line, stderr.
    fn exit(mut self, started: Instant) -> (Option<i32>, Vec<String>, String) {
        loop {
            if let Some(status) = self.child.try_wait().expect("wait on the program") {
                // The reader thread ends at the pipe's end, after the last
                // line; collect them all.
                while let Ok(line) = self.lines.recv_timeout(Duration::from_secs(5)) {
                    self.seen.push(line);
                }
                let stderr = self.stderr();
                return (status.code(), self.seen, stderr);
            }
            if started.elapsed() > RUN_LIMIT {
                let _ = self.child.kill();
                panic!(
                    "the program did not exit; it printed:\n{}",
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
/// `xvfb-run` and `sh`, which is the process SIGTERM is for.
fn pids_running(binary: &Path) -> Vec<i32> {
    let binary = binary.canonicalize().expect("the binary's path");
    std::fs::read_dir("/proc")
        .expect("read /proc")
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<i32>().ok())
        .filter(|pid| std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| exe == binary))
        .collect()
}

/// `(count, hover, pressed)` of every frame line, in order.
fn frame_states(seen: &[String]) -> Vec<(u32, bool, bool)> {
    seen.iter()
        .filter(|line| line.starts_with("frame "))
        .map(|line| {
            let field = |name: &str| {
                line.split_whitespace()
                    .find_map(|word| word.strip_prefix(name))
                    .unwrap_or_else(|| panic!("{line:?} has no {name}"))
                    .to_string()
            };
            (
                field("count=").parse().expect("a count"),
                field("hover=") == "true",
                field("pressed=") == "true",
            )
        })
        .collect()
}

/// The frames drawn, held to "only on change": the first state repeated
/// (an expose) is allowed, any other repeat is a frame nobody asked for.
fn assert_frames_on_change_only(seen: &[String]) {
    let states = frame_states(seen);
    let report = seen.join("\n");
    let first = *states
        .first()
        .unwrap_or_else(|| panic!("no frame drawn:\n{report}"));
    let after_expose: Vec<_> = states
        .iter()
        .copied()
        .skip_while(|state| *state == first)
        .collect();
    for pair in after_expose.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "a frame drew what the frame before it showed:\n{report}"
        );
    }
    let mut distinct = vec![first];
    distinct.extend(after_expose);
    assert_eq!(
        distinct,
        vec![
            (0, false, false),
            (0, true, false),
            (0, true, true),
            (1, true, false),
            (1, false, false),
        ],
        "the frames are not the five states the input produces:\n{report}"
    );
}

#[test]
fn the_gfx_example_draws_on_change_counts_a_click_and_ends_cleanly() {
    if !may_run(tools_missing()) {
        return;
    }
    let binary = build_example();

    // 8. The goldens first, offscreen, with no window system at all: they
    // need only a software adapter, and say plainly when there is none.
    let golden = Command::new(&binary)
        .arg("--golden")
        .arg(repository_root().join("crates/vilan-cli/tests/gfx/counter"))
        .arg("--font")
        .arg(repository_root().join("crates/vilan-rt-gfx/tests/fonts/NotoMono-Regular.ttf"))
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .output()
        .expect("run the golden mode");
    let said = String::from_utf8_lossy(&golden.stdout).to_string();
    // `GfxError::NoAdapter`'s sentence, as the golden mode prints it.
    let no_adapter = said.contains("no GPU adapter is available");
    if no_adapter
        && !may_run(Some(format!(
            "no software GPU adapter (install Mesa's lavapipe, mesa-vulkan-drivers): {said}"
        )))
    {
        return;
    }
    assert!(
        golden.status.success(),
        "the offscreen scene does not match its goldens:\n{said}{}",
        String::from_utf8_lossy(&golden.stderr)
    );
    if std::env::var_os("VILAN_REGENERATE_GFX_GOLDENS").is_none() {
        assert_eq!(
            said.matches(" matches").count(),
            2,
            "both scales are compared:\n{said}"
        );
    }

    let injector = build_injector();

    // 1-5. Hover, press, click, leave, idle, close — through the X server.
    let started = Instant::now();
    let mut running = Running::start(&binary, &["--smoke"]);
    running.wait_for("frame 1 ", started);
    // The button is 144 x 48 dp at (108, 140); at 100% a dp is a pixel.
    running.inject(&injector, &["move", "180", "164"]);
    running.wait_for("hover=true", started);
    running.inject(&injector, &["move", "186", "170"]);
    // The first move's line came before its frame; this is the second's.
    running.wait_for("moved over=true", started);
    running.inject(&injector, &["press"]);
    running.wait_for("pressed=true", started);
    running.inject(&injector, &["release"]);
    running.wait_for("count=1", started);
    running.inject(&injector, &["move", "10", "10"]);
    running.wait_for("idle frames=", started);
    running.inject(&injector, &["close"]);
    let (code, seen, stderr) = running.exit(started);
    let report = format!("{}\n--- stderr\n{stderr}", seen.join("\n"));
    assert!(
        seen.iter().any(|line| line == "idle frames=0"),
        "an idle second drew frames:\n{report}"
    );
    assert!(
        seen.iter().any(|line| line.starts_with("close requested")),
        "the close request never reached the program:\n{report}"
    );
    assert_frames_on_change_only(&seen);
    assert_eq!(
        code,
        Some(0),
        "closing the window ended it with {code:?}:\n{report}"
    );

    // 6. SIGTERM with the window open.
    let started = Instant::now();
    let mut running = Running::start(&binary, &["--smoke"]);
    running.wait_for("frame 1 ", started);
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
    let (code, seen, stderr) = running.exit(started);
    assert!(
        !seen.iter().any(|line| line.starts_with("close requested")),
        "the SIGTERM run closed through the window instead:\n{}",
        seen.join("\n")
    );
    assert_eq!(
        code,
        Some(0),
        "SIGTERM with a window open ended it with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );

    // 7. A failure inside a frame callback is the program's failure.
    let failing = build_failing_program();
    let started = Instant::now();
    let mut running = Running::start(&failing, &[]);
    running.wait_for("drawing", started);
    let (code, seen, stderr) = running.exit(started);
    assert_eq!(
        code,
        Some(1),
        "a failing frame callback ended the program with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );
    // Where the message arrives depends on the wrapper: older
    // `xvfb-run` scripts run their command with `2>&1`, so the program's
    // stderr comes out of the wrapper's stdout (ubuntu-latest's; CI saw exit
    // 1 with an empty stderr); later ones keep the two apart. Once, wherever it lands.
    let reported = format!("{}\n{stderr}", seen.join("\n"));
    assert_eq!(
        reported.matches("the frame callback failed").count(),
        1,
        "the failure is reported once:\n{reported}"
    );
}
