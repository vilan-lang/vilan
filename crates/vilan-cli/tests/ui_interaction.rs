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
//! 3. The list in a real window — a keyed list (`each_by` over a `ListCell`):
//!    three wheel notches, each a frame that moved the offset and ran no
//!    layout; a click on a row (one `set_at`: that row's cell), a frame whose
//!    solver measured that row alone and moved no box; then `a` (a row
//!    inserted at the top: the solver measured the newcomer alone), `d` (the
//!    top row removed: nothing measured) and `m` (the bottom row moved to the
//!    top: nothing measured), each a frame with the row count it left.
//!
//! 4. native-ui.md P4's exit test, "a todo app with the look supplied by the
//!    app": `vilan/examples/ui-todo`, on `std::ui::kit`. Its `--check` drives
//!    the screen with no window — typing, the caret keys, a selection by
//!    Shift and by a drag (off the input and out of the window: the press
//!    captures the pointer), double and triple clicks, a drag past the
//!    field's edge scrolling it, graphemes, Enter, Tab, Space and clicks on
//!    the checkboxes and the remove buttons — held to
//!    `tests/ui/todo-check.txt` line for line (the same regeneration). Then
//!    in a window: a click focuses the input, `m`, `i`, `l`, `k` type, a
//!    press at its left edge dragged out of the window and released there
//!    selects all four (X11's implicit grab delivering the moves and the
//!    release outside, winit passing them on), Return adds the item, a
//!    click checks it off, a click on its remove button removes it; then the
//!    CLIPBOARD (F173) on the private server: Ctrl+C copies, Ctrl+V pastes
//!    it back, another X client (`x11_input clipboard read`) reads the copy
//!    from the program, and a paste takes what a client that owns the
//!    clipboard (`clipboard own`) serves; the close button ends it with exit
//!    0. The listing also records the blink and a held drag on a stopped
//!    clock, and cut, copy and paste on an in-memory clipboard (F176, F173).
//! 5. The virtual list (`vilan/examples/ui-virtual-list`, part P4's
//!    `std::ui::kit::virtual_list_by` over 100,000 rows): its `--check`, with
//!    no window, held to `tests/ui/virtual-list-check.txt` (regenerated the
//!    same way) — what the tree holds after each step: the first layout, the
//!    wheel, a reveal half a list away, hover/press/focus kept across a
//!    change in the window, an insert above it and a programmatic scroll,
//!    thirty Tabs, Tab from the last row built, ops at either end, a reset,
//!    and a plain `SignalCell<List<T>>` — and on every step no more than a
//!    window's rows in the tree. Then in a window: `g` (the middle row
//!    revealed), a wheel notch, a click on the row under the pointer, whose
//!    id the program prints and the test works out from the offset alone.
//!    With `--varied` (F174: notes of differing heights on an ESTIMATED
//!    extent), its `--check` is held to `tests/ui/virtual-list-varied-check.txt`
//!    — the anchoring cases, reveals corrected once measured, random
//!    operations, ten million fixed rows past the old `f32` limit, a plain
//!    signal's whole lists — and in a window, with an estimate eight times too
//!    large: three wheel notches and a click on the row under the pointer,
//!    which must be the row the list's own heights put there, every frame
//!    laid out within the host's eight passes.
//!
//! 6. native-ui.md P5, accessibility publishing: the todo list driven
//!    through AT-SPI on a private session bus (`support::atspi`), as a screen
//!    reader would — the input and the Add button found by role and name, the
//!    input's text set and the button pressed, the new item's check box
//!    pressed — each step proven by the program's own line. It skips, with
//!    the reason, where AT-SPI's bus launcher or `gdbus` is missing, whatever
//!    `VILAN_REQUIRE_GFX` says.
//!
//! 7. native-ui.md P5, text editing: `vilan/examples/ui-notes`, a notes pad on
//!    `std::ui::kit::text_area`. Its `--check`, with no window, held to
//!    `tests/ui/notes-check.txt` (regenerated the same way): typing and
//!    Enter, Up and Down keeping a goal column across lines of different
//!    lengths, Home and End at a soft wrap (the caret's line read from the
//!    accessibility listing's selection), Shift over lines, a drag past the
//!    bottom edge, double and triple clicks, Page Down, undo and redo,
//!    Ctrl+Enter and the lock. Then in a window: a click focuses the area,
//!    `a`, `b`, Return, `c`, `d` type two lines, Up goes to the end of the
//!    first (the goal column), `x` types there — the program's own line
//!    shows `abx\ncd` — and the close button ends it with exit 0.
//! 8. F176, element timers: the counter's `--timers` (no window) on BOTH of
//!    a program's loops — the owned one (no display) and the window system's
//!    (under `xvfb-run`) — a repeating timer that cancels itself on its third
//!    call and then sets a one-shot one, a timer on an element removed before
//!    it is due, and the window's focus told to a tree. Every step is caused by
//!    the one before, so the order is the program's, not the clock's; the
//!    program ends by itself once no timer is left.
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

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

mod support;

use support::xvfb::Running;

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

/// Four device pixels inside the left edge of `box <name> x y width height`,
/// at its middle, at 100%.
fn left_edge(line: &str) -> (String, String) {
    let numbers: Vec<f64> = line
        .split_whitespace()
        .skip(2)
        .take(4)
        .map(|word| word.parse().expect("a number"))
        .collect();
    (
        ((numbers[0] + 4.0).round() as i64).to_string(),
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

/// N198: a run is `xvfb-run`, its `Xvfb` and the program, and the harness
/// holds only the first; a test that failed with a run open left the other
/// two alive (a `--smoke` list was found 73 minutes after a lane's run).
/// `Running` starts the run in a process group of its own and its `Drop` ends
/// the group and returns only when none of it is alive — so a test body that
/// PANICS with a run open leaves nothing, by the harness's own sequencing:
/// the unwinding drop has finished before `catch_unwind` returns.
#[test]
fn a_test_that_panics_with_a_run_open_leaves_no_process_of_it_behind() {
    if !may_run(tools_missing()) {
        return;
    }
    let group = std::sync::atomic::AtomicU32::new(0);
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // `sleep` stands for a program that would never end by itself.
        let running = Running::start(Path::new("sleep"), &["600"], "none");
        group.store(running.group(), std::sync::atomic::Ordering::SeqCst);
        let live = support::xvfb::live_in_group(running.group());
        assert!(
            live.len() >= 2,
            "the run is a group (xvfb-run, Xvfb, the program): {live:?}"
        );
        panic!("a test body failing with the run open");
    }));
    assert!(outcome.is_err(), "the body panicked");
    let group = group.load(std::sync::atomic::Ordering::SeqCst);
    assert_ne!(group, 0, "the run started");
    assert_eq!(
        support::xvfb::live_in_group(group),
        Vec::<u32>::new(),
        "a process of the run outlived the test"
    );
}

#[test]
fn the_counter_and_the_ten_thousand_row_list_run_end_to_end() {
    if !may_run(tools_missing()) {
        return;
    }
    let counter = build_example("ui-counter", "counter");

    // 1. The tree driven with no window: both spellings, and every event.
    let listed = assert_check_listing(&counter, "counter-check.txt");
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
    // The keys change the list: each a frame after its action, which the
    // solver lays out from its cache — measuring a new row's texts alone, and
    // nothing at all for a removal or a move.
    for (key, action, rows, measures) in [
        ("a", "add", "10001", true),
        ("d", "remove", "10000", false),
        ("m", "move", "10000", false),
    ] {
        running.inject(&injector, &["key", key]);
        let frame = running.wait_until(&format!("a frame after {action}"), started, |line| {
            line.starts_with("frame ") && field(line, "action=") == action
        });
        assert_eq!(field(&frame, "rows="), rows, "{frame}");
        assert_eq!(field(&frame, "solved="), "true", "{frame}");
        let measured: u32 = field(&frame, "measured=").parse().expect("a count");
        if measures {
            // The new row's two texts, each asked at the few widths a pass
            // asks (the click's one text answered at most 12): the rows
            // already there are answered from the solver's cache.
            assert!((1..=24).contains(&measured), "{frame}");
        } else {
            assert_eq!(measured, 0, "{frame}");
        }
    }
    running.inject(&injector, &["close"]);
    let (code, seen, stderr) = running.exit(started);
    assert_eq!(
        code,
        Some(0),
        "closing the list ended it with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );
}

/// F176: element timers fire in their causal order on the owned loop and on
/// the window system's, a removed element's timer never fires, and the
/// program ends when no timer is left (an armed timer would keep it running).
#[test]
fn element_timers_fire_in_order_on_both_loops_and_end_with_their_element() {
    if !may_run(tools_missing()) {
        return;
    }
    let counter = build_example("ui-counter", "counter_timers");
    let expected = [
        "removed, elements left 1",
        "focused before false",
        "window focus true, read back true",
        "window focus false, read back false",
        "timers set",
        "tick 1",
        "tick 2",
        "tick 3",
        "after fired",
        "removed element still held, 1 element",
        "done",
    ];
    // The program's own lines (`xvfb-run` may merge stderr in), and any line
    // a wrong timer would print.
    let program = |lines: &[String]| -> Vec<String> {
        lines
            .iter()
            .filter(|line| {
                expected.contains(&line.as_str())
                    || line.starts_with("tick ")
                    || line.contains("timer fired")
            })
            .cloned()
            .collect()
    };

    // The owned loop: no display at all.
    let owned = Command::new(&counter)
        .arg("--timers")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .output()
        .expect("run the timers mode");
    let printed: Vec<String> = String::from_utf8_lossy(&owned.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    assert!(
        owned.status.success(),
        "--timers failed:
{}
{}",
        printed.join(
            "
"
        ),
        String::from_utf8_lossy(&owned.stderr)
    );
    assert_eq!(program(&printed), expected, "on the owned loop");

    // The window system's loop: a display, no window.
    let started = Instant::now();
    let running = Running::start(&counter, &["--timers"], "none");
    let (code, seen, stderr) = running.exit(started);
    assert_eq!(
        code,
        Some(0),
        "--timers under xvfb ended with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );
    assert_eq!(program(&seen), expected, "on the window system's loop");
}

/// The recorded listing of an example's `--check`, compared line for line
/// (regenerated with `VILAN_REGENERATE_UI_CHECK=1`: READ it first).
fn assert_check_listing(binary: &Path, recorded: &str) -> String {
    assert_check_listing_with(binary, &[], recorded)
}

/// [`assert_check_listing`], with more arguments for the check mode.
fn assert_check_listing_with(binary: &Path, arguments: &[&str], recorded: &str) -> String {
    let checked = Command::new(binary)
        .args(arguments)
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
    let recorded = repository_root()
        .join("crates/vilan-cli/tests/ui")
        .join(recorded);
    if std::env::var_os("VILAN_REGENERATE_UI_CHECK").is_some() {
        std::fs::write(&recorded, &listed).expect("write the listing");
    }
    let expected = std::fs::read_to_string(&recorded)
        .expect("the recorded listing")
        .replace("\r\n", "\n");
    assert_eq!(expected, listed, "the tree no longer does what it recorded");
    listed
}

#[test]
fn the_todo_list_on_the_kit_runs_end_to_end() {
    if !may_run(tools_missing()) {
        return;
    }
    let todo = build_example("ui-todo", "todo");

    // 1. With no window: every behaviour of the three components, recorded.
    let listed = assert_check_listing(&todo, "todo-check.txt");
    assert!(
        listed.contains("remove bread: draft=\"\" caret=0 anchor=0 items=[]"),
        "{listed}"
    );

    // 2. In a window, by pointer and keyboard: add an item, check it off,
    // remove it.
    let injector = build_injector();
    let started = Instant::now();
    let mut running = Running::start(&todo, &["--smoke"], "vilan ui todo");
    running.wait_for("frame 1 ", started);
    let input = running.wait_for("box new-item ", started);
    let (x, y) = centre(&input);
    running.inject(&injector, &["move", &x, &y, "press", "release"]);
    running.wait_for("focus=input", started);
    running.inject(&injector, &["key", "m", "key", "i", "key", "l", "key", "k"]);
    running.wait_for("draft=milk items=[] focus=input selection=4..4", started);
    // Pointer capture through the X server: a press at the input's left
    // edge, the pointer taken out of the window to its right, the release
    // there — the moves and the release still reach the input, which
    // selected the whole draft.
    let (left, middle) = left_edge(&input);
    running.inject(&injector, &["move", &left, &middle, "press"]);
    running.wait_for("selection=0..0", started);
    running.inject(&injector, &["move", "700", &middle, "release"]);
    running.wait_for("draft=milk items=[] focus=input selection=0..4", started);
    running.inject(&injector, &["key", "Return"]);
    running.wait_for("draft= items=[[ ] milk]", started);
    let check = running.wait_for("box check-milk ", started);
    let remove = running.wait_for("box remove-milk ", started);
    let (x, y) = centre(&check);
    running.inject(&injector, &["move", &x, &y, "press", "release"]);
    running.wait_for("items=[[x] milk]", started);
    let (x, y) = centre(&remove);
    running.inject(&injector, &["move", &x, &y, "press", "release"]);
    running.wait_for("items=[] ", started);

    // 3. The clipboard (F173) through this private X server: Ctrl+C copies
    // the selection, and a paste of it proves the copy reached the clipboard
    // first (the clipboard keeps the order of its requests); then another X
    // client reads it from the program, and a paste takes the text a client
    // that owns the clipboard serves.
    let (x, y) = centre(&input);
    running.inject(&injector, &["move", &x, &y, "press", "release"]);
    running.inject(&injector, &["key", "m", "key", "i", "key", "l", "key", "k"]);
    running.wait_for("draft=milk items=[] focus=input selection=4..4", started);
    running.inject(&injector, &["key", "ctrl+a", "key", "ctrl+c"]);
    running.wait_for("draft=milk items=[] focus=input selection=0..4", started);
    running.inject(&injector, &["key", "End", "key", "ctrl+v"]);
    running.wait_for(
        "draft=milkmilk items=[] focus=input selection=8..8",
        started,
    );
    let read = running.inject_output(&injector, &["clipboard", "read"]);
    assert_eq!(
        read.trim(),
        "clipboard milk",
        "another client reads the copy"
    );
    let mut owner = running.start_injector(&injector, &["clipboard", "own", "eggs"]);
    let mut owned = String::new();
    std::io::BufRead::read_line(
        &mut std::io::BufReader::new(owner.stdout.as_mut().expect("its output")),
        &mut owned,
    )
    .expect("the owner's first line");
    assert_eq!(owned.trim(), "owned");
    running.inject(&injector, &["key", "ctrl+v"]);
    running.wait_for(
        "draft=milkmilkeggs items=[] focus=input selection=12..12",
        started,
    );
    let served = owner.wait_with_output().expect("the owner ends");
    assert!(
        served.status.success(),
        "the owning client failed: {}",
        String::from_utf8_lossy(&served.stderr)
    );
    running.inject(&injector, &["close"]);
    let (code, seen, stderr) = running.exit(started);
    let report = format!("{}\n--- stderr\n{stderr}", seen.join("\n"));
    for (line, what) in [
        ("add milk", "Return in the input adding the item"),
        ("remove milk", "the click on the remove button"),
    ] {
        assert!(
            seen.iter().any(|seen| seen == line),
            "{what} never ran:\n{report}"
        );
    }
    assert!(
        seen.iter().any(|line| line.starts_with("close requested")),
        "the close request never reached the program:\n{report}"
    );
    assert_eq!(
        code,
        Some(0),
        "closing the todo list ended it with {code:?}:\n{report}"
    );
}

/// F174: the virtual list with rows of differing heights (`--varied`, an
/// estimated extent). No window: the recorded listing, every check in it true,
/// and a window's rows at most in the tree. In a window, with an estimate of
/// 400 dp for rows of 30 to 150 (every row differs from it, by far): the wheel
/// moves the list, and a click lands on the row the list's heights and its
/// offset put under the pointer; every frame converged within the host's
/// passes.
#[test]
fn the_virtual_list_of_differing_heights_anchors_and_a_click_lands_on_its_row() {
    if !may_run(tools_missing()) {
        return;
    }
    let list = build_example("ui-virtual-list", "vlist_varied");

    // 1. With no window.
    let listed = assert_check_listing_with(&list, &["--varied"], "virtual-list-varied-check.txt");
    for refused in [
        ": false",
        "settled=false",
        "consistent=false",
        "meet=false",
        "placed=false",
    ] {
        assert!(!listed.contains(refused), "{refused}:\n{listed}");
    }
    let steps: Vec<(usize, usize)> = listed.lines().filter_map(tree_counts).collect();
    assert!(steps.len() >= 20, "{listed}");
    for (rows, elements) in &steps {
        assert!(*rows <= 30 && *elements <= 2 * 30 + 10, "{listed}");
    }

    // 2. In a window.
    let injector = build_injector();
    let started = Instant::now();
    let mut running = Running::start(
        &list,
        &["--smoke", "--varied", "--estimate", "400"],
        "vilan ui virtual list",
    );
    running.wait_for("frame 1 ", started);
    let viewport = running.wait_for("box viewport ", started);
    let (x, y) = centre(&viewport);
    running.inject(&injector, &["move", &x, &y]);
    running.inject(&injector, &["wheel", "down", "3"]);
    let scrolled = running.wait_until("a frame scrolled by the wheel", started, |line| {
        line.starts_with("frame ")
            && field(line, "offset=").parse::<f64>().expect("an offset") > 0.0
    });
    // The last frame before the click is the layout the click lands on: no
    // frame is drawn while nothing changes.
    running.inject(&injector, &["press", "release"]);
    let clicked = running.wait_for("clicked ", started);
    let before_click = running
        .seen
        .iter()
        .take_while(|line| !line.starts_with("clicked "))
        .filter(|line| line.starts_with("frame "))
        .last()
        .cloned()
        .unwrap_or(scrolled);
    let id: i64 = clicked
        .trim_start_matches("clicked ")
        .trim()
        .parse()
        .expect("an id");
    let middle: i64 = field(&before_click, "middle_row=").parse().expect("a row");
    let margin: f64 = field(&before_click, "middle_margin=")
        .parse()
        .expect("a margin");
    assert!(
        id == middle || (margin < 1.0 && (id - middle).abs() == 1),
        "clicked {id}, but the list's heights put row {middle} under the pointer:\n{}",
        running.seen.join("\n")
    );
    running.wait_until("the clicked row's frame", started, |line| {
        line.starts_with("frame ") && field(line, "clicked=") == format!("{id}")
    });
    // For the report: how many passes each frame's layout took.
    let passes: Vec<&str> = running
        .seen
        .iter()
        .filter(|line| line.starts_with("frame "))
        .map(|frame| field(frame, "passes="))
        .collect();
    eprintln!("layout passes per frame: {}", passes.join(" "));
    for frame in running
        .seen
        .iter()
        .filter(|line| line.starts_with("frame "))
    {
        let passes: usize = field(frame, "passes=").parse().expect("a count");
        assert!(
            passes < 8,
            "a layout used every pass the host allows: {frame}"
        );
        let tree: usize = field(frame, "tree=").parse().expect("a count");
        assert!(
            tree <= 2 * 30 + 10,
            "the tree held more than a window: {frame}"
        );
    }
    running.inject(&injector, &["close"]);
    let (code, seen, stderr) = running.exit(started);
    assert_eq!(
        code,
        Some(0),
        "closing the list ended it with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );
}

#[test]
fn the_notes_pad_on_the_kits_text_area_runs_end_to_end() {
    if !may_run(tools_missing()) {
        return;
    }
    let notes = build_example("ui-notes", "notes");

    // 1. With no window: the text area's behaviour, recorded.
    let listed = assert_check_listing(&notes, "notes-check.txt");
    assert!(
        listed.contains("down again: the goal column comes back"),
        "{listed}"
    );

    // 2. In a window, by pointer and keyboard: two lines, Up, a letter typed
    // at the goal column.
    let injector = build_injector();
    let started = Instant::now();
    let mut running = Running::start(&notes, &["--smoke"], "vilan ui notes");
    running.wait_for("frame 1 ", started);
    let area = running.wait_for("box notes ", started);
    let (x, y) = centre(&area);
    running.inject(&injector, &["move", &x, &y, "press", "release"]);
    running.wait_for("focus=notes", started);
    running.inject(
        &injector,
        &[
            "key", "a", "key", "b", "key", "Return", "key", "c", "key", "d",
        ],
    );
    running.wait_for("note=ab\\ncd selection=5..5 focus=notes", started);
    running.inject(&injector, &["key", "Up"]);
    running.wait_for("note=ab\\ncd selection=2..2 focus=notes", started);
    running.inject(&injector, &["key", "x"]);
    running.wait_for("note=abx\\ncd selection=3..3 focus=notes", started);
    running.inject(&injector, &["close"]);
    let (code, seen, stderr) = running.exit(started);
    let report = format!("{}\n--- stderr\n{stderr}", seen.join("\n"));
    assert!(
        seen.iter().any(|line| line.starts_with("close requested")),
        "the close request never reached the program:\n{report}"
    );
    assert_eq!(
        code,
        Some(0),
        "closing the notes pad ended it with {code:?}:\n{report}"
    );
}

/// native-ui.md P5: the todo list as a screen reader meets it, over AT-SPI on
/// a private session bus (`support::atspi`): its window, its input and its Add
/// button found by role and name, the input's text set and the button
/// pressed through the accessibility API — the program hears a typed value
/// and a click and adds the item — and the new item's check box pressed the
/// same way.
#[test]
fn the_todo_list_is_driven_through_the_accessibility_bus() {
    if !may_run(tools_missing()) {
        return;
    }
    // Not part of what the `gfx` leg requires: a box without AT-SPI's
    // launcher skips with the reason whatever `VILAN_REQUIRE_GFX` says.
    if let Some(reason) = support::atspi::tools_missing() {
        eprintln!("skipped: {reason}");
        return;
    }
    // Its own entry name: the kit test stages `todo` at the same time.
    let todo = build_example("ui-todo", "todo_access");
    let session = support::atspi::Session::start();
    let started = Instant::now();
    let mut running =
        Running::start_in(&todo, &["--smoke"], "vilan ui todo", &session.environment());
    running.wait_for("frame 1 ", started);

    let input = session.wait_for(support::atspi::ENTRY, "new item", "EditableText");
    session.set_text(&input, "milk");
    running.wait_for("draft=milk ", started);
    let add = session.wait_for(support::atspi::PUSH_BUTTON, "add", "Action");
    session.act(&add, 0);
    running.wait_for("add milk", started);
    running.wait_for("draft= items=[[ ] milk]", started);
    let check = session.wait_for(support::atspi::CHECK_BOX, "check milk", "Action");
    session.act(&check, 0);
    running.wait_for("items=[[x] milk]", started);

    let group = session.group();
    drop(running);
    drop(session);
    assert!(
        support::xvfb::live_in_group(group).is_empty(),
        "the private bus outlived its session"
    );
}

/// The numbers of a `--check` line's `rows=a..b (n) elements=m`: the count of
/// rows and of elements in the tree.
fn tree_counts(line: &str) -> Option<(usize, usize)> {
    let rows = line.split(" (").nth(1)?.split(')').next()?.parse().ok()?;
    let elements = field(line, "elements=").parse().ok()?;
    Some((rows, elements))
}

#[test]
fn the_virtual_list_holds_only_its_window_and_a_click_far_down_lands_on_its_row() {
    if !may_run(tools_missing()) {
        return;
    }
    let list = build_example("ui-virtual-list", "vlist");

    // 1. With no window: the recorded steps, and the window's size at each.
    let listed = assert_check_listing(&list, "virtual-list-check.txt");
    // Structure, whatever the recording says: 100,000 rows, and never more
    // than a window of them (a 600 dp viewport of 32 dp rows, two overscan
    // rows each side and one for Tab) in the tree, three elements a row.
    let steps: Vec<(usize, usize)> = listed.lines().filter_map(tree_counts).collect();
    assert!(steps.len() >= 15, "{listed}");
    for (rows, elements) in &steps {
        assert!(*rows <= 26 && *elements <= 3 * 26 + 10, "{listed}");
    }
    assert!(
        listed.contains("count=100000 built=0..21"),
        "the first layout built the rows the viewport shows: {listed}"
    );

    // 2. In a window: the middle revealed, a notch of the wheel, a click.
    let injector = build_injector();
    let started = Instant::now();
    let mut running = Running::start(&list, &["--smoke"], "vilan ui virtual list");
    running.wait_for("frame 1 ", started);
    let viewport = running.wait_for("box viewport ", started);
    let viewport_top: f64 = viewport
        .split_whitespace()
        .nth(3)
        .and_then(|word| word.parse().ok())
        .expect("the viewport's top");
    let (x, y) = centre(&viewport);
    running.inject(&injector, &["move", &x, &y]);
    running.inject(&injector, &["key", "g"]);
    let middle = running.wait_until("a frame at the middle", started, |line| {
        line.starts_with("frame ") && field(line, "action=") == "middle"
    });
    let offset: f64 = field(&middle, "offset=").parse().expect("an offset");
    assert!(offset > 1_000_000.0, "{middle}");
    running.inject(&injector, &["wheel", "down", "1"]);
    let scrolled = running.wait_until("a frame further down", started, |line| {
        line.starts_with("frame ")
            && field(line, "offset=").parse::<f64>().expect("an offset") > offset
    });
    let offset: f64 = field(&scrolled, "offset=").parse().expect("an offset");
    for frame in running
        .seen
        .iter()
        .filter(|line| line.starts_with("frame "))
    {
        let tree: usize = field(frame, "tree=").parse().expect("a count");
        assert!(
            tree <= 3 * 26 + 10,
            "the tree held more than a window: {frame}"
        );
    }
    running.inject(&injector, &["press", "release"]);
    let clicked = running.wait_for("clicked ", started);
    let id: f64 = clicked
        .trim_start_matches("clicked ")
        .trim()
        .parse()
        .expect("an id");
    // The row under the pointer, from the offset alone: the content's y over
    // the 32 dp rows (no row was added, so an id is its index). A pointer
    // within a pixel of a boundary may land on either row.
    let pointer_y: f64 = y.parse().expect("the pointer's y");
    let content = offset + pointer_y - viewport_top;
    let row = (content / 32.0).floor();
    let near_a_boundary = (content - row * 32.0) < 1.0 || ((row + 1.0) * 32.0 - content) < 1.0;
    assert!(
        id == row || (near_a_boundary && (id - row).abs() == 1.0),
        "clicked {id}, but the row under the pointer at offset {offset} is {row}:\n{}",
        running.seen.join("\n")
    );
    running.wait_until("the clicked row's frame", started, |line| {
        line.starts_with("frame ") && field(line, "clicked=") == format!("{id}")
    });
    running.inject(&injector, &["close"]);
    let (code, seen, stderr) = running.exit(started);
    assert_eq!(
        code,
        Some(0),
        "closing the list ended it with {code:?}:\n{}\n--- stderr\n{stderr}",
        seen.join("\n")
    );
}
