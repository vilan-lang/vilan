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
//!    channel — the button's plus icon is a stroked `Path` (gfx-path.md S2) —
//!    and malformed path data is an `Err` naming the byte;
//! 9. the system clipboard's text (F173), with no window: the example's
//!    `--clipboard` copies a text and reads it back; another X client (the
//!    injector's `clipboard read`) reads it from the program, through
//!    `arboard`'s own connection and serving thread; that client then takes
//!    the clipboard (`clipboard own`) and the program reads ITS text; and with
//!    no display at all the program says so and ends. Only under `xvfb-run`:
//!    the clipboard is that private server's, never the session's.
//!
//! Frames on change only: every frame after the first must show something
//! the frame before it did not (an X server may expose a new window once more,
//! which repeats the FIRST frame's state and nothing else), and the sequence
//! of states is exactly the five the input produces. Ordering comes from the
//! program's lines — each injection waits for the line its predecessor caused
//! — never from a sleep.
//!
//! 10. `VILAN_GFX_REPORT=1` (F187): the host prints its start report (the
//!     adapter, the surface, the window) and, when the window closes, its exit
//!     report (frames and their times) to stderr; with `VILAN_GFX_REPORT=0`
//!     it prints neither. The report's statistics and wording are pinned by
//!     the GPU crate's unit tests (`src/host/report.rs`); this proves the
//!     host prints them, from a real adapter.
//!
//! # When it runs
//!
//! On Linux with `xvfb-run` and `cargo`, and a software Vulkan adapter (Mesa's
//! lavapipe, `mesa-vulkan-drivers`). Anywhere else it is SKIPPED with the
//! reason printed. `VILAN_REQUIRE_GFX=1` (the `gfx` CI job, through
//! `scripts/ci-local.sh gfx`) turns a skip into a failure. Regenerate the
//! goldens with `VILAN_REGENERATE_GFX_GOLDENS=1` and LOOK at them before
//! committing.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

mod support;

use support::xvfb::Running;

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

/// The example staged under its own name, so a second test builds it beside
/// [`build_example`]'s without sharing a staging directory.
fn build_example_as(entry: &str) -> PathBuf {
    let staged = scratch().join(entry);
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("stage the example");
    let example = repository_root().join("vilan/examples/gfx");
    std::fs::copy(example.join("vilan.toml"), staged.join("vilan.toml")).expect("its manifest");
    std::fs::copy(example.join("main.vl"), staged.join(format!("{entry}.vl"))).expect("its source");
    build_native(&staged, entry)
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
    // gfx-path.md S2: `Path::from_svg("M10 10 L")` names byte 8, the end,
    // where a number was due.
    assert!(
        said.lines()
            .any(|line| line == "path: invalid path data at byte 8: expected a number: M10 10 L>"),
        "malformed path data is not an `Err` naming the byte:\n{said}"
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
    let mut running = Running::start(&binary, &["--smoke"], TITLE);
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
    let mut running = Running::start(&binary, &["--smoke"], TITLE);
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
    let mut running = Running::start(&failing, &[], TITLE);
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

/// F173 (item 9 above): the system clipboard's text, for real, between the program and
/// another X client on a private X server; and cleanly nothing with no
/// display. Each step waits for the line its predecessor caused.
#[test]
fn the_clipboard_round_trips_with_another_x_client_and_is_absent_without_a_display() {
    if !may_run(tools_missing()) {
        return;
    }
    let binary = build_example_as("clipboard");
    let injector = build_injector();

    // No display: no clipboard, no panic, no wait.
    let alone = Command::new(&binary)
        .arg("--clipboard")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .output()
        .expect("run the clipboard mode");
    let said = String::from_utf8_lossy(&alone.stdout).to_string();
    assert!(
        alone.status.success() && said.lines().any(|line| line == "no clipboard"),
        "with no display:\n{said}{}",
        String::from_utf8_lossy(&alone.stderr)
    );

    let started = Instant::now();
    let mut running = Running::start(&binary, &["--clipboard"], "none");
    running.wait_for("clipboard own: copied by vilan", started);
    running.wait_for("clipboard waiting", started);
    // Another client reads what the program copied: served by the program.
    let read = running.inject_output(&injector, &["clipboard", "read"]);
    assert_eq!(read.trim(), "clipboard copied by vilan");
    // That client copies: the program reads its text.
    let owned = running.inject_output(&injector, &["clipboard", "own", "copied by the injector"]);
    assert_eq!(owned.trim(), "owned");
    running.wait_for("clipboard pasted: copied by the injector", started);
    let (code, seen, stderr) = running.exit(started);
    assert_eq!(
        code,
        Some(0),
        "the clipboard mode ended with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );
}

/// F187 (item 10 above): the report switch, on and off, around one hover and
/// a close. Each step waits for the line its predecessor caused.
#[test]
fn the_report_switch_prints_the_adapter_at_start_and_the_frames_at_exit() {
    if !may_run(tools_missing()) {
        return;
    }
    let binary = build_example_as("report");
    let injector = build_injector();
    let run = |value: &str| {
        let started = Instant::now();
        let mut running = Running::start_in(
            &binary,
            &["--smoke"],
            TITLE,
            &[("VILAN_GFX_REPORT".to_string(), value.to_string())],
        );
        running.wait_for("frame 1 ", started);
        // The button is 144 x 48 dp at (108, 140); at 100% a dp is a pixel.
        running.inject(&injector, &["move", "180", "164"]);
        running.wait_for("hover=true", started);
        running.inject(&injector, &["close"]);
        let (code, seen, stderr) = running.exit(started);
        // Older `xvfb-run` scripts merge the program's stderr into stdout.
        let said = format!("{}\n--- stderr\n{stderr}", seen.join("\n"));
        assert_eq!(code, Some(0), "the run ended with {code:?}:\n{said}");
        said
    };

    let off = run("0");
    assert!(
        !off.contains("vilan gfx report"),
        "VILAN_GFX_REPORT=0 printed a report:\n{off}"
    );

    let on = run("1");
    let lines: Vec<&str> = on.lines().collect();
    let has = |wanted: &str| lines.iter().any(|line| line.starts_with(wanted));
    for wanted in [
        "--- vilan gfx report: start ---",
        "adapter: ",
        "adapter type: ",
        "backend: ",
        "surface format: ",
        "present mode: ",
        "scale factor: 1",
        "--- vilan gfx report: exit ---",
        "encode: count ",
        "interval: ",
        "glyph atlas: ",
    ] {
        assert!(has(wanted), "the report has no line `{wanted}..`:\n{on}");
    }
    // The window opened at its asked size, 360 x 240 dp at 100%.
    assert!(
        has("window inner size: 360x240 px"),
        "the start report's window size:\n{on}"
    );
    // The first frame and the hover's: at least two presented.
    let presented: u64 = lines
        .iter()
        .find_map(|line| line.strip_prefix("frames presented: "))
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("no frame count:\n{on}"));
    assert!(presented >= 2, "{presented} frames presented:\n{on}");
    // Both sections once, the start before the exit.
    let start = on.find("--- vilan gfx report: start ---").expect("start");
    let exit = on.find("--- vilan gfx report: exit ---").expect("exit");
    assert!(start < exit, "the exit report came first:\n{on}");
    assert_eq!(on.matches("--- vilan gfx report: start ---").count(), 1);
    assert_eq!(on.matches("--- vilan gfx report: exit ---").count(), 1);
}
