//! native-ui.md P2's exit test: "a fixed screen lays out to recorded
//! rectangles". `vilan/examples/ui` — a documentation page built from
//! `std::ui`'s rows, columns, stacks, texts and an image — is built with the
//! Rust backend and run:
//!
//! 1. `--rects`: laid out with the test font (Noto Mono) at two window sizes,
//!    each at 100% and 150%, every element's box printed in device pixels, and
//!    the four listings held to `tests/ui/screen-rects.txt` line for line. No
//!    GPU and no display: layout is the text system and the solver.
//! 2. `--golden`: the same screen drawn OFFSCREEN with a software adapter at
//!    100% and 150%, held to `tests/ui/screen-100.png` and `screen-150.png`
//!    within one per channel (`gfx_window.rs`'s tolerance).
//! 3. `--smoke`: the screen in a real window under `xvfb-run -a`, laid out at
//!    the window's size, one frame drawn, the window closed; exit 0.
//!
//! # When it runs
//!
//! Where `gfx_window.rs` runs, and skipped where it skips, by the same rule:
//! Linux with `xvfb-run` and `cargo`, and (for the images and the window) a
//! software Vulkan adapter. `VILAN_REQUIRE_GFX=1` (the `gfx` CI job, through
//! `scripts/ci-local.sh gfx`) turns a skip into a failure. Regenerate with
//! `VILAN_REGENERATE_UI_RECTS=1` (the listing) and
//! `VILAN_REGENERATE_GFX_GOLDENS=1` (the images), and LOOK at both before
//! committing.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// How long the window may take to open, draw and close: a few seconds warm,
/// the bound is for a loaded CI runner.
const RUN_LIMIT: Duration = Duration::from_secs(120);

/// The two window sizes and two scales the listing records, in dp.
const LAYOUTS: [(u32, u32, f64); 4] = [
    (960, 640, 1.0),
    (960, 640, 1.5),
    (720, 900, 1.0),
    (720, 900, 1.5),
];

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
            panic!("VILAN_REQUIRE_GFX is set, and the std::ui screen proof cannot run: {reason}")
        }
        Some(reason) => {
            eprintln!("skipped: {reason}");
            false
        }
    }
}

/// `gfx_window.rs`'s rule, so the two run (and cost a GPU stack build) in the
/// same places.
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

/// Its own scratch and target directory: `gfx_window.rs` builds a binary of
/// the same name (`main`) into its own.
fn scratch() -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join("ui-screen")
}

fn test_font() -> PathBuf {
    repository_root().join("crates/vilan-rt-gfx/tests/fonts/NotoMono-Regular.ttf")
}

fn expected_directory() -> PathBuf {
    repository_root().join("crates/vilan-cli/tests/ui")
}

/// Stages the example's tracked sources and builds it with the Rust backend;
/// answers the binary.
fn build_example() -> PathBuf {
    let staged = scratch().join("example");
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("stage the example");
    let example = repository_root().join("vilan/examples/ui");
    for file in ["vilan.toml", "main.vl"] {
        std::fs::copy(example.join(file), staged.join(file)).expect("copy the example");
    }
    let target = scratch().join("target");
    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(["build", "--backend", "rust", "main.vl"])
        .current_dir(&staged)
        .env("VILAN_RT", repository_root().join("crates/vilan-rt"))
        .env("CARGO_TARGET_DIR", &target)
        .output()
        .expect("run vilan");
    assert!(
        built.status.success(),
        "the ui example did not build natively:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let binary = target.join("debug").join("main");
    assert!(binary.is_file(), "no binary at {}", binary.display());
    binary
}

/// The four listings, as the program prints them.
fn listings(binary: &Path) -> String {
    let mut all = String::new();
    for (width, height, scale) in LAYOUTS {
        let output = Command::new(binary)
            .arg("--rects")
            .args([width.to_string(), height.to_string(), scale.to_string()])
            .arg("--font")
            .arg(test_font())
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .output()
            .expect("run the rects mode");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success(),
            "--rects {width} {height} {scale} failed:\n{stdout}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        all.push_str(&stdout);
    }
    all
}

#[test]
fn the_ui_example_lays_out_to_its_recorded_rectangles_and_draws_its_goldens() {
    if !may_run(tools_missing()) {
        return;
    }
    let binary = build_example();

    // 1. The recorded rectangles.
    let recorded = expected_directory().join("screen-rects.txt");
    let listed = listings(&binary);
    if std::env::var_os("VILAN_REGENERATE_UI_RECTS").is_some() {
        std::fs::write(&recorded, &listed).expect("write the listing");
    }
    let expected = std::fs::read_to_string(&recorded)
        .expect("the recorded listing")
        .replace("\r\n", "\n");
    if expected != listed {
        let first = expected
            .lines()
            .zip(listed.lines())
            .position(|(left, right)| left != right)
            .unwrap_or(expected.lines().count().min(listed.lines().count()));
        panic!(
            "the screen's boxes moved from the recorded rectangles (first difference at line \
             {}):\n--- recorded\n{}\n--- laid out\n{}",
            first + 1,
            expected.lines().nth(first).unwrap_or("(end)"),
            listed.lines().nth(first).unwrap_or("(end)"),
        );
    }

    // 2. The golden images, offscreen, with no window system.
    let golden = Command::new(&binary)
        .arg("--golden")
        .arg(expected_directory().join("screen"))
        .arg("--font")
        .arg(test_font())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .output()
        .expect("run the golden mode");
    let said = String::from_utf8_lossy(&golden.stdout).to_string();
    if said.contains("no GPU adapter is available")
        && !may_run(Some(format!(
            "no software GPU adapter (install Mesa's lavapipe, mesa-vulkan-drivers): {said}"
        )))
    {
        return;
    }
    assert!(
        golden.status.success(),
        "the offscreen screen does not match its goldens:\n{said}{}",
        String::from_utf8_lossy(&golden.stderr)
    );
    if std::env::var_os("VILAN_REGENERATE_GFX_GOLDENS").is_none() {
        assert_eq!(
            said.matches(" matches").count(),
            2,
            "both scales are compared:\n{said}"
        );
    }

    // 3. A real window: laid out at its size, one frame, closed.
    let log = scratch().join("smoke.log");
    let errors = scratch().join("smoke-errors.log");
    let mut child = Command::new("xvfb-run")
        .args(["-a", "-s", "-screen 0 1280x1024x24"])
        .arg(&binary)
        .arg("--smoke")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .env("WINIT_X11_SCALE_FACTOR", "1")
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&log).expect("the smoke log"))
        .stderr(std::fs::File::create(&errors).expect("the smoke's errors"))
        .spawn()
        .expect("run the window under xvfb-run");
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("wait on the window") {
            break status;
        }
        if started.elapsed() > RUN_LIMIT {
            let _ = child.kill();
            panic!(
                "the window did not close itself after its first frame:\n{}",
                std::fs::read_to_string(&log).unwrap_or_default()
            );
        }
        // A bounded poll of a program that ends by itself after one frame:
        // the interval only sets how soon its exit is noticed.
        std::thread::sleep(Duration::from_millis(50));
    };
    let stderr = std::fs::read_to_string(&errors).unwrap_or_default();
    let said = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        status.success() && said.contains("frame 960 x 640 at 1"),
        "the window did not lay the screen out at its size ({status}):\n{said}{stderr}"
    );
}
