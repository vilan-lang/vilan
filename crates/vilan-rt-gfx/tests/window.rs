//! The window half, end to end: `examples/window.rs --self-test` in a real X
//! window under `xvfb-run -a`, at 100% and 150%.
//!
//! What it proves, from the example's own per-frame lines: a frame is drawn
//! when the window opens, at the window's device size for its scale factor; a
//! resize draws one frame at the new size; an idle second draws NONE (frames
//! on demand only); and the window's close message (WM_DELETE_WINDOW, sent
//! through the X server as a window manager sends it) ends the program with
//! exit code 0. The ordering is the example's own sequencing (each step waits
//! for the frame before it), not a sleep in this test.
//!
//! Skipped with a printed reason where there is no `xvfb-run` or no software
//! adapter; required under `VILAN_REQUIRE_GFX=1` (CI's `gfx` job). Not
//! verified here: a scale factor CHANGING while the window is open (X11 reads
//! it once, at start), Wayland, Windows and macOS.

#![cfg(target_os = "linux")]

mod support;

use std::path::PathBuf;
use std::process::Command;

use support::{skip_or_fail, software_context};

/// The example binary, built now by the cargo running this test: a filtered
/// `cargo test --test window` does not build examples, and an example left
/// over from an earlier build would be tested stale. The build lands in the
/// same target directory and profile, so it is a no-op when nothing changed.
fn example_binary() -> PathBuf {
    let test_binary = std::env::current_exe().expect("the test binary's path");
    let profile_directory = test_binary
        .parent()
        .and_then(|deps| deps.parent())
        .expect("the test binary lives in <target>/<profile>/deps");
    let target_directory = profile_directory.parent().expect("<target>/<profile>");
    let release = profile_directory
        .file_name()
        .is_some_and(|name| name == "release");
    let mut build = Command::new(env!("CARGO"));
    build
        .args(["build", "--example", "window", "--manifest-path"])
        .arg(support::crate_directory().join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target_directory);
    if release {
        build.arg("--release");
    }
    let status = build.status().expect("cargo runs");
    assert!(
        status.success(),
        "building the window example failed: {status}"
    );
    profile_directory.join("examples").join("window")
}

fn has_xvfb_run() -> bool {
    Command::new("sh")
        .args(["-c", "command -v xvfb-run"])
        .output()
        .is_ok_and(|output| output.status.success())
}

/// `(width, height)` of each frame the example printed, in order.
fn frame_sizes(stdout: &str) -> Vec<(u32, u32)> {
    stdout
        .lines()
        .filter(|line| line.starts_with("frame="))
        .map(|line| {
            let size = line
                .split_whitespace()
                .find_map(|field| field.strip_prefix("size="))
                .expect("a frame line names its size");
            let (width, height) = size.split_once('x').expect("WxH");
            (
                width.parse().expect("a width"),
                height.parse().expect("a height"),
            )
        })
        .collect()
}

#[test]
fn a_window_draws_on_demand_follows_a_resize_and_exits_when_closed() {
    if !has_xvfb_run() {
        skip_or_fail("no `xvfb-run` on PATH (Debian/Ubuntu: apt install xvfb)");
        return;
    }
    if software_context().is_none() {
        return;
    }
    let example = example_binary();
    for (scale_factor, scale_text) in [(1.0, "1"), (1.5, "1.5")] {
        let output = Command::new("xvfb-run")
            .args(["-a", "-s", "-screen 0 1600x1200x24"])
            .arg(&example)
            .arg("--self-test")
            // X11 under xvfb, whatever session this test runs in (winit picks
            // Wayland whenever WAYLAND_DISPLAY is set).
            .env_remove("WAYLAND_DISPLAY")
            .env_remove("WAYLAND_SOCKET")
            .env("WINIT_X11_SCALE_FACTOR", scale_text)
            .output()
            .expect("xvfb-run starts");
        let stdout = String::from_utf8_lossy(&output.stdout);
        let report = format!(
            "at {scale_factor}: status {:?}\n--- stdout\n{stdout}--- stderr\n{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            output.status.success(),
            "the window example failed {report}"
        );
        let device = |logical: f64| (logical * scale_factor).round() as u32;
        let sizes = frame_sizes(&stdout);
        assert_eq!(
            sizes.first(),
            Some(&(device(480.0), device(300.0))),
            "the first frame is the opening size in device pixels {report}"
        );
        assert_eq!(
            sizes.last(),
            Some(&(device(640.0), device(400.0))),
            "the last frame is the resized size {report}"
        );
        // An X server may expose the window once more as it maps or resizes,
        // which is a legitimate redraw; anything beyond that is drawing
        // without being asked.
        assert!(
            sizes.len() <= 4,
            "{} frames for one open and one resize {report}",
            sizes.len()
        );
        assert!(
            stdout.contains("idle frames=0 "),
            "an idle second drew a frame {report}"
        );
        assert!(
            stdout.contains("close-requested"),
            "the close message did not arrive {report}"
        );
    }
}
