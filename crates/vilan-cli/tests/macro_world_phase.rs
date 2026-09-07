//! The macro worlds get a ROW on the phase line (tracker M33).
//!
//! A cold `vilan check` of kolt's client compiles four macro worlds —
//! `analyze_source` ×4 inside `macros::expand_source`, 18.2% of that entry's
//! instructions, comparable to the entry's own whole-program checks — and the
//! phase line could not see them. A world is a nested analysis (macro-engine.md
//! §3), its own `[vilan phase] load+walk …` line is suppressed because its
//! numbers inside the outer analysis's would read as noise, and so its cost
//! folded into the outer entry's `load+walk` with nothing saying so. The only
//! trace the four left was four unheaded `post-passes` lines ahead of the
//! entry's, which reads as the compiler having run its post-passes five times.
//!
//! Two properties, and the second is the one that had to change:
//!
//! 1. **The row is always printed**, count included, zero included. A row that
//!    appeared only when there was something to report could not state the fact
//!    a warm run exists to state — "this analysis compiled no macro worlds" —
//!    which is precisely the reading M33's cross-process cache is pinned on.
//! 2. **A world no longer prints a `post-passes` line of its own.** Its
//!    post-passes are tallied into the row's own field instead, so the line
//!    beginning `[vilan phase] post-passes` appears exactly once per analysis
//!    and means the entry's.
//!
//! The row's numbers are a SLICE through the line above it, not a disjoint
//! bucket: a world runs inside the outer entry's `load+walk`, because that is
//! when the expansion needing it runs. The two do not sum, exactly as
//! `dispatch-refine` does not sum with the buckets it explains — subtracting
//! the worlds out would make `load+walk` stop being the wall the phase took,
//! which is what a reader uses it for.
//!
//! Its own file: this is a new module, and `diagnostics.rs`'s phase-line tests
//! are about the instrument's other halves.

use std::path::PathBuf;
use std::process::Command;

/// A package whose entry `[derive]`s, so the expander dispatches into std's
/// derive world and one world is compiled.
const DERIVING: &str = "[derive(Debug)]\nstruct Point { x: i32, y: i32 }\n\n\
     fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tlet shown = p.debug();\n}\n";

/// The same package with no macro use anywhere — the control that makes the
/// count above a measurement rather than a constant, and the shape E121's
/// generated exhibit has (1,791 mechanical functions, not one derive among
/// them: its row reads 0).
const PLAIN: &str = "struct Point { x: i32, y: i32 }\n\n\
     fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tlet moved = p.x;\n}\n";

fn temp_package(name: &str, entry: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan-m33-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id(),
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).expect("a scratch package");
    std::fs::write(
        dir.join("vilan.toml"),
        format!("[package]\nname = \"{name}\"\ndefault-entry = \"main\"\n\n[entry.main]\n"),
    )
    .expect("the manifest");
    std::fs::write(dir.join("src/main.vl"), entry).expect("the entry");
    dir
}

/// The phase line of a `vilan check` of `dir`, with the instrument on.
fn phase_lines(dir: &PathBuf) -> Vec<String> {
    let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .args(["check", "."])
        .env("VILAN_PHASE_TIMING", "1")
        .output()
        .expect("run vilan");
    assert!(
        output.status.success(),
        "the fixture must check cleanly; stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stderr)
        .lines()
        .filter(|line| line.starts_with("[vilan phase]"))
        .map(str::to_string)
        .collect()
}

/// The count on the `macro-worlds` row, and a failure that shows the whole
/// line when the shape is not what it should be.
fn macro_world_count(lines: &[String]) -> usize {
    let rows: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("macro-worlds"))
        .collect();
    assert_eq!(
        rows.len(),
        1,
        "exactly one `macro-worlds` row per analysis; the phase lines were: {lines:#?}"
    );
    let row = rows[0];
    for field in [
        "macro-worlds",
        "load+walk",
        "base",
        "build",
        "checks",
        "post-passes",
    ] {
        assert!(
            row.contains(field),
            "the row must carry `{field}` — a world's cost splits the same way \
             the entry's does, and a row that named only a total would send the \
             next reader back to a profiler: {row}"
        );
    }
    let words: Vec<&str> = row.split_whitespace().collect();
    let index = words
        .iter()
        .position(|word| *word == "macro-worlds")
        .expect("just asserted the label is there");
    words[index + 1]
        .parse()
        .unwrap_or_else(|_| panic!("the count must be a number: {row}"))
}

#[test]
fn the_macro_worlds_row_is_printed_with_its_count() {
    let dir = temp_package("deriving", DERIVING);
    let lines = phase_lines(&dir);
    let count = macro_world_count(&lines);
    assert!(
        count >= 1,
        "a `[derive]` dispatches into std's derive world, so a cold check must \
         report at least one compiled world; the phase lines were: {lines:#?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_package_with_no_macro_use_reports_zero_rather_than_no_row() {
    let dir = temp_package("plain", PLAIN);
    let lines = phase_lines(&dir);
    assert_eq!(
        macro_world_count(&lines),
        0,
        "the row must be printed with a zero rather than omitted — the reading \
         a warm run exists to produce is exactly this one; the phase lines \
         were: {lines:#?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_macro_world_no_longer_prints_a_post_passes_line_of_its_own() {
    // The regression this row replaces: four unheaded `post-passes` lines ahead
    // of the entry's, which read as five runs of the post-passes rather than as
    // one entry and its four worlds.
    let dir = temp_package("postpasses", DERIVING);
    let lines = phase_lines(&dir);
    let entry_post_passes = lines
        .iter()
        .filter(|line| line.starts_with("[vilan phase] post-passes"))
        .count();
    assert_eq!(
        entry_post_passes, 1,
        "exactly one `post-passes` line per analysis, and it is the ENTRY's: a \
         world's post-passes belong in the `macro-worlds` row's own field. The \
         phase lines were: {lines:#?}"
    );
    // Non-vacuity: the fixture really did compile a world, so the assertion
    // above is about where its line went and not about a program that has none.
    assert!(
        macro_world_count(&lines) >= 1,
        "the fixture must compile a world for the assertion above to mean \
         anything; the phase lines were: {lines:#?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
