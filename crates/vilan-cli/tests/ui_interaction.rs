//! native-ui.md P3's exit test: "the counter and a 10,000-row list". The two
//! examples — `vilan/examples/ui-counter` and `vilan/examples/ui-list` — are
//! built with the Rust backend and run:
//!
//! 1. The counter's `--check`: no window and no GPU. It builds its screen in
//!    both spellings (element tags, and the builder calls they lower to) and
//!    compares their boxes, then drives the tree with `std::ui::deliver` —
//!    hover, press, release, a press released elsewhere, Enter, a plain key,
//!    Tab, Space, Tab, Shift+Tab, the pointer leaving, a narrow window — and
//!    prints the interaction state and the handlers each event reached. The
//!    listing is held to `tests/ui/counter-check.txt` line for line
//!    (regenerate with `VILAN_REGENERATE_UI_CHECK=1`, and READ it first).
//! 2. The counter in a real window under `xvfb-run -a`, on Mesa's software
//!    Vulkan, driven through the X server (XTEST, by
//!    `crates/vilan-rt-gfx/examples/x11_input.rs`): the pointer over "Add
//!    one" (its hover look), a press (pressed and focused), the release (a
//!    click: the count is 1), Tab (the focus on "Reset"), Return (the count
//!    is 0), Shift+Tab (back on "Add one"), Space (1 again), the pointer away,
//!    an idle second that draws no frame, the close button — exit 0. Every
//!    frame after the first shows a state the frame before it did not (an X
//!    server may expose a new window once more, repeating the first).
//! 3. The list in a real window: three wheel notches, each a frame that moved
//!    the offset and ran no layout; then a click on a row, a frame whose solver
//!    measured that row alone and moved no box.
//!
//! Ordering comes from the programs' lines — each injection waits for the
//! line its predecessor caused — never from a sleep. No assertion reads a
//! time: the frame times the programs print are for a person (the report's
//! table), not for a pin a fast or slow machine would break.
//!
//! # When it runs
//!
//! Where `gfx_window.rs` runs, and skipped where it skips, by the same rule:
//! Linux with `xvfb-run` and `cargo`, and a software Vulkan adapter for the
//! windows. `VILAN_REQUIRE_GFX=1` (the `gfx` CI job, through
//! `scripts/ci-local.sh gfx`) turns a skip into a failure.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long one run may take before it is a hang: a few seconds warm (one
/// of them the counter's own idle second); the bound is for a loaded runner.
const RUN_LIMIT: Duration = Duration::from_secs(120);

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn required() -> bool {
    std::env::var_os("VILAN_REQUIRE_GFX").is_some_and(|value| value != "0")
}

fn may_run(reason: Option<String>) -> bool {
    match reason {
        None => true,
        Some(reason) if required() => {
            panic!(
                "VILAN_REQUIRE_GFX is set, and the std::ui interaction proof cannot run: {reason}"
            )
        }
        Some(reason) => {
            eprintln!("skipped: {reason}");
            false
        }
    }
}

/// `gfx_window.rs`'s rule, so the three run (and cost a GPU stack build) in
/// the same places.
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

/// Its own scratch and target directory: `gfx_window.rs` and `ui_screen.rs`
/// build binaries named `main` into theirs.
fn scratch() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("ui-interaction")
}

fn test_font() -> PathBuf {
    repository_root().join("crates/vilan-rt-gfx/tests/fonts/NotoMono-Regular.ttf")
}

/// Stages an example's tracked sources as `<entry>.vl` (so the two binaries
/// have their own names in the one target directory) and builds it with the
/// Rust backend; answers the binary.
fn build_example(example: &str, entry: &str) -> PathBuf {
    let staged = scratch().join(entry);
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("stage the example");
    let source = repository_root().join("vilan/examples").join(example);
    std::fs::copy(source.join("vilan.toml"), staged.join("vilan.toml")).expect("its manifest");
    std::fs::copy(source.join("main.vl"), staged.join(format!("{entry}.vl"))).expect("its source");
    let target = scratch().join("target");
    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--backend", "rust", &format!("{entry}.vl")])
        .current_dir(&staged)
        .env("VILAN_RT", repository_root().join("crates/vilan-rt"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run vilan");
    assert!(
        built.status.success(),
        "{example} did not build natively:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let binary = target.join("debug").join(entry);
    assert!(binary.is_file(), "no binary at {}", binary.display());
    binary
}

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
    title: &'static str,
}

impl Running {
    fn start(binary: &Path, arguments: &[&str], title: &'static str) -> Running {
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
        let first = lines
            .recv_timeout(RUN_LIMIT)
            .expect("the wrapper names the display");
        let mut words = first.split_whitespace();
        assert_eq!(words.next(), Some("display"), "{first}");
        let display = words.next().expect("a display").to_string();
        let authority = words.next().unwrap_or_default().to_string();
        Running {
            child,
            lines,
            seen: Vec::new(),
            display,
            authority,
            title,
        }
    }

    /// Waits for a line `wanted` accepts, recording every line on the way;
    /// answers it.
    fn wait_until(
        &mut self,
        what: &str,
        started: Instant,
        wanted: impl Fn(&str) -> bool,
    ) -> String {
        loop {
            let left = RUN_LIMIT.saturating_sub(started.elapsed());
            match self.lines.recv_timeout(left) {
                Ok(line) => {
                    let found = wanted(&line);
                    self.seen.push(line.clone());
                    if found {
                        return line;
                    }
                }
                Err(_) => {
                    let _ = self.child.kill();
                    let stderr = self.stderr();
                    panic!(
                        "the program never printed {what}; it printed:\n{}\n--- stderr\n{stderr}",
                        self.seen.join("\n")
                    );
                }
            }
        }
    }

    fn wait_for(&mut self, wanted: &str, started: Instant) -> String {
        self.wait_until(&format!("{wanted:?}"), started, |line| {
            line.contains(wanted)
        })
    }

    fn stderr(&mut self) -> String {
        let mut text = String::new();
        if let Some(mut stderr) = self.child.stderr.take() {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut text);
        }
        text
    }

    /// Moves, presses, releases, turns the wheel, types or closes, through the
    /// X server, and returns once the server has handled it.
    fn inject(&self, injector: &Path, commands: &[&str]) {
        let status = Command::new(injector)
            .args(["--title", self.title])
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

/// The value of `name=` in a line.
fn field<'line>(line: &'line str, name: &str) -> &'line str {
    line.split_whitespace()
        .find_map(|word| word.strip_prefix(name))
        .unwrap_or_else(|| panic!("{line:?} has no {name}"))
}

/// The centre of `box <name> x y width height`, in device pixels at 100%.
fn centre(line: &str) -> (String, String) {
    let numbers: Vec<f64> = line
        .split_whitespace()
        .skip(2)
        .take(4)
        .map(|word| word.parse().expect("a number"))
        .collect();
    (
        ((numbers[0] + numbers[2] / 2.0).round() as i64).to_string(),
        ((numbers[1] + numbers[3] / 2.0).round() as i64).to_string(),
    )
}

/// The counter's frames, held to "only on change": its first state repeated
/// (an expose) is allowed, any other repeat is a frame nobody asked for; and
/// the states are exactly the input's.
fn assert_counter_frames(seen: &[String]) {
    let report = seen.join("\n");
    let states: Vec<(String, String, String)> = seen
        .iter()
        .filter(|line| line.starts_with("frame "))
        .map(|line| {
            (
                field(line, "count=").to_string(),
                field(line, "add=").to_string(),
                field(line, "reset=").to_string(),
            )
        })
        .collect();
    let first = states
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("no frame drawn:\n{report}"));
    let after_expose: Vec<_> = states
        .iter()
        .skip_while(|state| **state == first)
        .cloned()
        .collect();
    for pair in after_expose.windows(2) {
        assert_ne!(
            pair[0], pair[1],
            "a frame drew what the frame before it showed:\n{report}"
        );
    }
    let mut distinct = vec![first];
    distinct.extend(after_expose);
    let expected: Vec<(String, String, String)> = [
        ("0", "-", "-"),
        ("0", "h", "-"),
        ("0", "hpf", "-"),
        ("1", "hf", "-"),
        ("1", "h", "f"),
        ("0", "h", "f"),
        ("0", "hf", "-"),
        ("1", "hf", "-"),
        ("1", "f", "-"),
    ]
    .iter()
    .map(|(count, add, reset)| (count.to_string(), add.to_string(), reset.to_string()))
    .collect();
    assert_eq!(
        distinct, expected,
        "the frames are not the states the input produces:\n{report}"
    );
}

#[test]
fn the_counter_and_the_ten_thousand_row_list_run_end_to_end() {
    if !may_run(tools_missing()) {
        return;
    }
    let counter = build_example("ui-counter", "counter");

    // 1. The tree driven with no window: both spellings, and every event.
    let checked = Command::new(&counter)
        .arg("--check")
        .arg("--font")
        .arg(test_font())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run the check mode");
    let listed = String::from_utf8_lossy(&checked.stdout).to_string();
    assert!(
        checked.status.success(),
        "--check failed:\n{listed}{}",
        String::from_utf8_lossy(&checked.stderr)
    );
    let recorded = repository_root().join("crates/vilan-cli/tests/ui/counter-check.txt");
    if std::env::var_os("VILAN_REGENERATE_UI_CHECK").is_some() {
        std::fs::write(&recorded, &listed).expect("write the listing");
    }
    let expected = std::fs::read_to_string(&recorded)
        .expect("the recorded listing")
        .replace("\r\n", "\n");
    assert_eq!(
        expected, listed,
        "the counter's tree no longer does what it recorded"
    );
    assert!(listed.starts_with("spellings agree=true"), "{listed}");

    let list = build_example("ui-list", "list");
    let injector = build_injector();

    // 2. The counter, in a window, by pointer and keyboard.
    let started = Instant::now();
    let mut running = Running::start(&counter, &["--smoke"], "vilan ui counter");
    running.wait_for("frame 1 ", started);
    let add = running.wait_for("box add ", started);
    let (add_x, add_y) = centre(&add);
    running.inject(&injector, &["move", &add_x, &add_y]);
    running.wait_until("a frame with add hovered", started, |line| {
        line.starts_with("frame ") && line.contains(" add=h ")
    });
    running.inject(&injector, &["press"]);
    running.wait_for("add=hpf", started);
    running.inject(&injector, &["release"]);
    running.wait_for("count=1 add=hf", started);
    running.inject(&injector, &["key", "Tab"]);
    running.wait_for("reset=f", started);
    running.inject(&injector, &["key", "Return"]);
    running.wait_for("count=0 add=h reset=f", started);
    running.inject(&injector, &["key", "shift+Tab"]);
    running.wait_for("count=0 add=hf", started);
    running.inject(&injector, &["key", "space"]);
    running.wait_for("count=1 add=hf", started);
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
    for (line, handler) in [
        ("click Add one", "the click on the button"),
        ("click counter", "the click bubbling to the screen"),
        ("key Reset", "Return on the focused Reset"),
        ("key Add one", "Space on the focused Add one"),
    ] {
        assert!(
            seen.iter().any(|seen| seen == line),
            "{handler} never ran:\n{report}"
        );
    }
    assert_counter_frames(&seen);
    assert_eq!(
        code,
        Some(0),
        "closing the window ended it with {code:?}:\n{report}"
    );

    // 3. The list: three wheel notches, then a click on a row.
    let started = Instant::now();
    let mut running = Running::start(&list, &["--smoke"], "vilan ui list");
    running.wait_for("frame 1 ", started);
    let viewport = running.wait_for("box viewport ", started);
    let (x, y) = centre(&viewport);
    running.inject(&injector, &["move", &x, &y]);
    let mut offset = 0.0;
    for _ in 0..3 {
        running.inject(&injector, &["wheel", "down", "1"]);
        let frame = running.wait_until("a frame further down", started, |line| {
            line.starts_with("frame ")
                && field(line, "offset=").parse::<f64>().expect("an offset") > offset
        });
        offset = field(&frame, "offset=").parse().expect("an offset");
        // The scroll moved its child: no layout at all — nothing measured,
        // no box moved. (How many scenes the frame rebuilt depends on which
        // rows the pointer left and came onto before the frame was drawn,
        // which is the machine's timing; `src/ui/tests.rs` pins that a scroll
        // alone rebuilds the scroll's scene and its chain.)
        assert_eq!(field(&frame, "solved="), "false", "{frame}");
        assert_eq!(field(&frame, "measured="), "0", "{frame}");
        assert_eq!(field(&frame, "moved="), "0", "{frame}");
    }
    running.inject(&injector, &["press", "release"]);
    let clicked = running.wait_until("the clicked row's frame", started, |line| {
        line.starts_with("frame ") && !line.contains("clicked=-1")
    });
    // One row's text changed: the solver measured that row's texts alone and
    // no box moved ("0 clicks" and "1 clicks" are one width).
    assert_eq!(field(&clicked, "solved="), "true", "{clicked}");
    assert!(
        field(&clicked, "measured=")
            .parse::<u32>()
            .expect("a count")
            <= 12,
        "{clicked}"
    );
    assert_eq!(field(&clicked, "moved="), "0", "{clicked}");
    running.inject(&injector, &["close"]);
    let (code, seen, stderr) = running.exit(started);
    assert_eq!(
        code,
        Some(0),
        "closing the list ended it with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );
}
