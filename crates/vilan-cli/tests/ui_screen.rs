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
//! 3. `--icons` (gfx-path.md S3's exit test): the example's second screen,
//!    icons in rows beside text — in the text's size and colour, at sizes of
//!    their own, coloured and labelled, inside a button, filled — laid out at
//!    100% and 150% and held, with what an assistive tool is told of it (the
//!    labelled icon an image, the decorative ones nothing), to
//!    `tests/ui/icon-rects.txt`; then drawn offscreen and held to
//!    `tests/ui/icons-100.png` and `icons-150.png`. Its own goldens, so the
//!    page's do not move; and its own comparison (tracker N201): a path
//!    mask's blend rounds apart on two Mesa versions, so up to 12 channel
//!    values per image may be two off (none further), where the page's
//!    goldens allow none. `--golden-rule` pins that rule on made-up images.
//! 4. `--smoke`: the screen in a real window under `xvfb-run -a`, laid out at
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
use std::time::Instant;

mod support;

use support::xvfb::Running;

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

/// The icon screen's listing: its boxes at 100% and 150%, and what an
/// assistive tool is told of it.
fn icon_listing(binary: &Path) -> String {
    let output = Command::new(binary)
        .args(["--rects", "--icons", "--font"])
        .arg(test_font())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run the icon rects mode");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(
        output.status.success(),
        "--rects --icons failed:\n{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout
}

/// The icon goldens' comparison (tracker N201): every channel within one,
/// except at most a set number two off and none further, because two
/// versions of the software rasteriser round a path mask's blend apart. The
/// page's goldens allow none. Pinned on made-up images, with no GPU.
fn held_to_golden_rule(binary: &Path) {
    let output = Command::new(binary)
        .arg("--golden-rule")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run the golden rule mode");
    let said = String::from_utf8_lossy(&output.stdout).replace("\r\n", "\n");
    assert!(output.status.success(), "--golden-rule failed:\n{said}");
    assert_eq!(
        said,
        concat!(
            "rule every value one off, none allowed: passes\n",
            "rule one value two off, none allowed: fails\n",
            "rule three values two off, 12 allowed: passes\n",
            "rule twelve values two off, 12 allowed: passes\n",
            "rule thirteen values two off, 12 allowed: fails\n",
            "rule one value three off, 12 allowed: fails\n",
        )
    );
}

/// Holds `listed` to the recorded listing `name` in `tests/ui`, line for line
/// (`VILAN_REGENERATE_UI_RECTS` writes it instead).
fn held_to_listing(name: &str, listed: &str) {
    let recorded = expected_directory().join(name);
    if std::env::var_os("VILAN_REGENERATE_UI_RECTS").is_some() {
        std::fs::write(&recorded, listed).expect("write the listing");
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
            "{name}: the screen moved from its recorded listing (first difference at line {}):\n\
             --- recorded\n{}\n--- laid out\n{}",
            first + 1,
            expected.lines().nth(first).unwrap_or("(end)"),
            listed.lines().nth(first).unwrap_or("(end)"),
        );
    }
}

/// Draws a screen offscreen (the page, or the one `extra` names) and holds it
/// to `tests/ui/<golden>-100.png` and `-150.png`. Answers false when there is
/// no software adapter and the proof may skip.
fn matches_goldens(binary: &Path, golden: &str, extra: &[&str]) -> bool {
    let run = Command::new(binary)
        .arg("--golden")
        .arg(expected_directory().join(golden))
        .args(extra)
        .arg("--font")
        .arg(test_font())
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .env_remove("WAYLAND_SOCKET")
        .output()
        .expect("run the golden mode");
    let said = String::from_utf8_lossy(&run.stdout).to_string();
    if said.contains("no GPU adapter is available")
        && !may_run(Some(format!(
            "no software GPU adapter (install Mesa's lavapipe, mesa-vulkan-drivers): {said}"
        )))
    {
        return false;
    }
    assert!(
        run.status.success(),
        "the offscreen {golden} does not match its goldens:\n{said}{}",
        String::from_utf8_lossy(&run.stderr)
    );
    if std::env::var_os("VILAN_REGENERATE_GFX_GOLDENS").is_none() {
        assert_eq!(
            said.matches(" matches").count(),
            2,
            "both scales are compared:\n{said}"
        );
    }
    true
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
    held_to_listing("screen-rects.txt", &listings(&binary));

    // 2. The golden images, offscreen, with no window system.
    if !matches_goldens(&binary, "screen", &[]) {
        return;
    }

    // 3. The icon screen: its boxes and what a tool is told, then its goldens,
    // compared by a rule the program states first (N201).
    held_to_golden_rule(&binary);
    held_to_listing("icon-rects.txt", &icon_listing(&binary));
    if !matches_goldens(&binary, "icons", &["--icons"]) {
        return;
    }

    // 4. A real window: laid out at its size, one frame, closed. The run is a
    // process group the harness ends whatever happens here (N198).
    let started = Instant::now();
    let running = Running::start(&binary, &["--smoke"], "vilan ui");
    let (code, seen, stderr) = running.exit(started);
    let said = seen.join("\n");
    assert!(
        code == Some(0) && said.contains("frame 960 x 640 at 1"),
        "the window did not lay the screen out at its size ({code:?}):\n{said}\n{stderr}"
    );
}
