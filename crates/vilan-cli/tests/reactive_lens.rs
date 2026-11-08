//! `std::reactive::lens` — the reactive lens (tracker A185 S1,
//! `proposal/reactive-lens.md`), its laws RUN.
//!
//! The pins are five corpus programs, `vilan/test/lens-{laws,fuzz,nested,keyed,
//! tracked}.vl`, so the byte gate compiles them on every run and every corpus
//! differential runs them. But the byte gate compares BYTES and the
//! differentials compare one run against another: a program that panics
//! identically twice passes both. The assertions are the programs' own
//! `panic`s, so each is built here against THIS tree's std, run under node, and
//! its exit code and its whole stdout read — exactly as `delta_log.rs` does for
//! `delta-law.vl` and `list-cell.vl`.
//!
//! Every program was proven non-vacuous by planting the bug it is against in
//! `std/src/reactive/lens.vl` and watching it go red:
//!
//! - `lens-fuzz.vl`: the positional table's `renumber` dropped after a widen
//!   reds at `step 22 (insert)`; a `Splice` that forgets its arrivals at
//!   `step 10`; the index mapping a key to the slot one below at `step 575`.
//! - `lens-laws.vl`: `landed` notifying through `batch` instead of
//!   `settle_together` reds at "one run for two writes"; the value feed or the
//!   row feed recording a replacing splice as two ops at "a replacing splice is
//!   one op" / "one row op"; the length woken by an equal-count splice; the
//!   keyed handler ignoring a `Splice`; an index whose release keeps its slots.
//! - `lens-keyed.vl`: `explain_sequence` ignoring a `Splice`'s counts reds at
//!   "an honest splice passes".
//! - `lens-nested.vl`: an inner write landing as a `Replace` of the outer item
//!   instead of an `Edit` of its part reds at "only the items part of that
//!   section woke".
//! - `lens-tracked.vl`: a deletion that wakes no slot reds at "the row's
//!   deletion re-runs it once"; an edit addressed one row low at "another row's
//!   edit and an insert above do not re-run it".

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// The repository's `vilan/std` directory — what `VILAN_STD` points the built
/// binary at, so a program compiles against THIS tree's std and not the
/// embedded copy of whatever was last released.
fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// Builds the corpus program `source` in a staging directory of its own and
/// runs it under node, returning its stdout. A failing build or a non-zero exit
/// panics with everything the run said.
///
/// The staging copy keeps the build from writing `vilan/test/<name>.mjs` in
/// place, which is the committed golden the byte gate holds.
fn build_and_run(source: &str) -> String {
    let program = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../vilan/test")
        .join(source);
    let contents = std::fs::read_to_string(&program)
        .unwrap_or_else(|error| panic!("read {program:?}: {error}"));
    let tag = source.trim_end_matches(".vl");
    let directory =
        support::scratch_dir(&format!("vilan_reactive_lens_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&directory, &error)));
    let staged = directory.join(source);
    std::fs::write(&staged, contents)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&staged, &error)));

    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&staged)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        built.status.success(),
        "{source} must build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let emitted = staged.with_extension("mjs");
    assert!(
        emitted.is_file(),
        "{source} built without leaving {} behind — the run below would have \
         passed vacuously",
        emitted.display()
    );

    let run = Command::new("node")
        .arg(&emitted)
        .current_dir(&directory)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "{source} exited {:?}:\n{stdout}--- stderr ---\n{}",
        run.status.code(),
        String::from_utf8_lossy(&run.stderr)
    );
    stdout
}

/// L1–L4, L6, L7 per handler, a move, the diff door, std's operators over a
/// lens, the index, lifetime under counting and under an owner; then S1's own:
/// one `Splice` per splice to a tap, to the handler, to the value feed and to
/// the row feed (R16), the length asleep for an equal-count splice, a write from
/// an observer joining the drained turn, the keyed handler's `Splice`, an
/// index's release, and the wrapper types (R10).
#[test]
fn the_lens_laws_hold() {
    assert_eq!(build_and_run("lens-laws.vl"), "laws: all passed\n");
}

/// L1 and L8 under 9,000 seeded random writes (three seeds): every followed
/// slot checked after every write, std's `map` and `filter` over the value
/// feed, the row feed's ops replayed onto a mirror, and the index. The tail is
/// held so a generator that stops reaching its writes reds instead of passing
/// quietly.
#[test]
fn the_positional_table_survives_random_writes() {
    assert_eq!(
        build_and_run("lens-fuzz.vl"),
        "fuzz seed 1: 3000 writes, 343 slots, 1234 items held at the end\n\
         fuzz seed 7: 3000 writes, 423 slots, 1012 items held at the end\n\
         fuzz seed 42: 3000 writes, 501 slots, 859 items held at the end\n\
         fuzz: all passed\n",
        "the walk's arithmetic moved: the generator is seeded, so a different \
         count is a different walk, not a golden"
    );
}

/// A lens inside a lens: an inner write is an `Edit` of one part of the outer
/// item, the inner lens follows its item across outer shifts, one turn spans
/// both levels, and an inner lens whose item is gone reads empty.
#[test]
fn a_nested_lens_writes_one_part_of_its_item() {
    assert_eq!(build_and_run("lens-nested.vl"), "nested: all passed\n");
}

/// The keyed side (std's map operators, the length, the diff door) and the
/// planned write with its truth check, `Splice` included.
#[test]
fn the_keyed_lens_and_the_truth_check_hold() {
    assert_eq!(build_and_run("lens-keyed.vl"), "keyed: all passed\n");
}

/// A tracked body over two parts of one slot re-runs for exactly those parts
/// and for the row's deletion, and for nothing else.
#[test]
fn tracked_reads_depend_on_exactly_the_parts_read() {
    assert_eq!(build_and_run("lens-tracked.vl"), "tracked: all passed\n");
}

/// R17: the lens is reached by name (`import std::reactive::lens::{ .. }`) and
/// NOT re-exported from `std::reactive`, until its analysis cost is paid down —
/// a program that never names it must not pay for it (T4 measured 13–18% more
/// instructions to check every reactive example with the module in the
/// reactive closure). The perf gate's rows are the measurement; this is the
/// cheap guard on the one line that would undo it. No other std module may
/// import it either, since any import puts it in that module's closure.
#[test]
fn std_reaches_the_lens_only_by_name() {
    let source_root = std_dir().join("src");
    let mut pending = vec![source_root.clone()];
    let mut offenders = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory)
            .unwrap_or_else(|error| panic!("read {directory:?}: {error}"))
        {
            let path = entry.expect("read a std entry").path();
            if path.is_dir() {
                pending.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "vl")
                || path.ends_with("reactive/lens.vl")
            {
                continue;
            }
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("read {path:?}: {error}"));
            if text.contains("reactive::lens") {
                offenders.push(
                    path.strip_prefix(&source_root)
                        .unwrap()
                        .display()
                        .to_string(),
                );
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{offenders:?} name `reactive::lens`: R17 keeps the lens out of every \
         other std module's closure (and out of `std::reactive`'s re-exports) \
         until its analysis cost is paid down"
    );
}
