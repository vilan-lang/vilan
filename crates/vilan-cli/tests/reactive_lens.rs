//! `std::reactive::lens` — the reactive lens (tracker A185 S1, S2, S5,
//! `proposal/reactive-lens.md`), its laws RUN.
//!
//! The pins are eleven corpus programs, `vilan/test/lens-{laws,fuzz,nested,keyed,
//! tracked,sorted,set,rope}.vl` (the last three S2's) and S5's
//! `lens-{loaded,overlay,loaded-tasks}.vl`, so the byte gate
//! compiles them on every run and every corpus differential runs them. But the
//! byte gate compares BYTES and the differentials compare one run against
//! another: a program that panics identically twice passes both. The
//! assertions are the programs' own `panic`s, so each is built here against
//! THIS tree's std, run under node, and its exit code and its whole stdout read
//! — exactly as `delta_log.rs` does for `delta-law.vl` and `list-cell.vl`.
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
//!   S3's slim path, each proven on the S3 section alone: a slot cell not kept
//!   on its first ask reds at "one slot, one cell"; the length's cell not kept
//!   at "a length asked through one handle"; `landed` deciding ONCE per write
//!   whether a slot exists at "the second change moved the slot the first
//!   change's tap made"; `Resizable::shift`'s default landing the item back at
//!   `from` at "the default shift moved the item".
//! - `lens-keyed.vl`: `explain_sequence` ignoring a `Splice`'s counts reds at
//!   "an honest splice passes".
//! - `lens-nested.vl`: an inner write landing as a `Replace` of the outer item
//!   instead of an `Edit` of its part reds at "only the items part of that
//!   section woke".
//! - `lens-tracked.vl`: a deletion that wakes no slot reds at "the row's
//!   deletion re-runs it once"; an edit addressed one row low at "another row's
//!   edit and an insert above do not re-run it".
//! - `lens-sorted.vl` (S2, the read-only shape): `index_by`'s tap ignoring a
//!   planned `Delete` reds at "the index let 20 go".
//! - `lens-set.vl` (S2, the set shape): the members feed recording a `Delete` as
//!   an `Add` reds at "one op per change, in order"; `SetLens::add` naming an
//!   `Insert` for a member already held at "a held member writes nothing".
//! - `lens-rope.vl` (S2, the exhibit): `Lens::splice` recording a pure insert
//!   as an `Insert` rather than its `Splice` (R16) reds at "the paste is ONE
//!   change".
//! - `a_read_only_sequence_has_no_writing_surface` (S2): `SequenceLens::replace`
//!   moved into the wrapper's READING impl (bounded on `Indexed`) reds at "this
//!   program must NOT build".
//!
//! S5's three are against `std/src/reactive/lens/loaded.vl`:
//!
//! - `lens-loaded.vl`: `settle` ignoring the token reds at "2: the superseded
//!   reply is dropped"; a confirmation landing whether or not a newer one was
//!   confirmed at "5: the late, older ack does not walk the row back to A";
//!   `view` without its `distinct` at "1: a confirm of the value shown woke the
//!   flag and the lens's slot, not the view"; `propose_delete` writing the lens
//!   (the lab's wrong turn, which R12 rules out) at "4: the lens keeps the row".
//! - `lens-overlay.vl`: a confirmed insert landing at the top reds at "it landed
//!   in the lens where it was shown"; a provisional row still listed once the
//!   lens holds its key at "the pushed row IS the row".
//! - `lens-loaded-tasks.vl`: `fetch` not cancelling the load it supersedes reds
//!   at "the superseded call stopped polling: it was cancelled"; `retire` not
//!   cancelling at "the owner's end cancelled the load and dropped its reply".

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
/// index's release, and the wrapper types (R10); then S3's: cells made on the
/// first ask and shared by every handle, a slot a tap makes mid-write, and
/// `Resizable::shift`'s default.
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

/// S5's eight scenarios (`proposal/reactive-lens.md` §16.2) with the server
/// driven by hand: phases beside the rows, latest-wins by token, a refresh that
/// keeps slots, pages with no duplicate rows, the keyed collection, and the
/// overlay on top, R12's hidden delete included.
#[test]
fn a_loaded_list_holds_the_eight_scenarios() {
    assert_eq!(build_and_run("lens-loaded.vl"), "loaded: all passed\n");
}

/// The overlay beside a bare lens: provisional rows and their places, a
/// confirmed insert matched by the server's key, a server removal outranking a
/// local edit, and two changes on one row answered in either order.
#[test]
fn the_overlay_never_writes_the_lens() {
    assert_eq!(build_and_run("lens-overlay.vl"), "overlay: all passed\n");
}

/// The loaded list on real tasks: a superseded fetch and page cancelled, an
/// owner's end dropping the reply, and std's `.transient()` and `TaskSource`
/// followed. Ordered by gates the program opens, never by sleep lengths.
#[test]
fn a_loaded_list_cancels_what_it_supersedes() {
    assert_eq!(
        build_and_run("lens-loaded-tasks.vl"),
        "loaded tasks: all passed\n"
    );
}

/// S2's read-only shape: a sorted index under `indexed(..)` keeps slots, parts,
/// rows, the value feed, the length and the index through `update` and `plan`.
#[test]
fn a_read_only_sequence_reads_like_any_other() {
    assert_eq!(build_and_run("lens-sorted.vl"), "sorted: all passed\n");
}

/// The other half of the read-only shape: a lens over a structure that is
/// `Indexed` and not `Resizable` has NO writing surface, so nothing can splice
/// into a sorted index's order. Each write is its own program, so each refusal
/// is its own: a single program would stop at the first.
#[test]
fn a_read_only_sequence_has_no_writing_surface() {
    for write in [
        "index.splice(0, 0, [1]);",
        "index.push(1);",
        "index.move_item(0, 1);",
        "wrapped.splice(0, 0, [1]);",
        "wrapped.replace(0, 1);",
        "wrapped.move_item(0, 1);",
    ] {
        let source = format!(
            "import std::reactive::SequenceCell;\n\
             import std::reactive::lens::{{ Indexed, SequenceLens, indexed }};\n\
             \n\
             struct Fixed {{\n\titems: List<i32>,\n}}\n\
             \n\
             impl Fixed with Indexed<i32> {{\n\
             \tfun count(self): usize {{\n\t\tself.items.len()\n\t}}\n\
             \n\
             \tfun item(self, at: usize): Option<i32> {{\n\t\tself.items.get(at)\n\t}}\n\
             }}\n\
             \n\
             fun main() {{\n\
             \tlet index = indexed(Fixed {{ items = [1, 2] }});\n\
             \tlet wrapped = SequenceLens::indexed(Fixed {{ items = [1, 2] }});\n\
             \t{write}\n\
             \tprint(index.all());\n\
             \tprint(wrapped.all());\n\
             }}\n"
        );
        let refused = build_refused(&source);
        assert!(
            refused.contains("does not implement trait 'Resizable<i32>'"),
            "`{write}` on a read-only lens must be refused for want of `Resizable`, \
             not for anything else:\n{refused}"
        );
    }
}

/// The set shape: a member's slot is its presence, the length wakes for joins
/// and leaves, `members()` is std's `SetOp` feed (and a `SetFlow`), and
/// `reconcile_set` is one turn.
#[test]
fn the_set_lens_follows_members() {
    assert_eq!(build_and_run("lens-set.vl"), "set: all passed\n");
}

/// The exhibit: a chunked rope under the lens. Every line is read exactly but
/// the two timing lines, which are the measurement (the report carries them)
/// and are held to their shape: a number of milliseconds.
#[test]
fn a_real_rope_keeps_its_slots_and_pastes_in_one_change() {
    let stdout = build_and_run("lens-rope.vl");
    let mut exact = Vec::new();
    let mut timings = 0;
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("rope timing: ") {
            let milliseconds = rest
                .rsplit_once(": ")
                .and_then(|(_, number)| number.strip_suffix(" ms"))
                .and_then(|number| number.parse::<f64>().ok());
            assert!(
                milliseconds.is_some_and(|value| value >= 0.0),
                "a timing line must end in a number of milliseconds: {line:?}"
            );
            timings += 1;
        } else {
            exact.push(line);
        }
    }
    assert_eq!(timings, 2, "the paste and the edits are each measured once");
    assert_eq!(
        exact,
        [
            "rope: 600 random bulk splices kept the rope whole",
            "rope: splits and merges kept every slot",
            "rope: a 5000-line paste is one change",
            "rope: all passed",
        ]
    );
}

/// Builds `source` (a program written here, not a corpus file) and answers
/// what the refused build said. A build that SUCCEEDS panics: the pin claims a
/// refusal.
fn build_refused(source: &str) -> String {
    let directory = support::scratch_dir(&format!(
        "vilan_reactive_lens_refused_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&directory, &error)));
    let staged = directory.join("refused.vl");
    std::fs::write(&staged, source)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&staged, &error)));
    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&staged)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    assert!(
        !built.status.success(),
        "this program must NOT build:\n{source}\n--- it said ---\n{said}"
    );
    said
}

/// R17: the lens is reached by name (`import std::reactive::lens::{ .. }`) and
/// NOT re-exported from `std::reactive`, until its analysis cost is paid down —
/// a program that never names it must not pay for it (T4 measured 13–18% more
/// instructions to check every reactive example with the module in the
/// reactive closure). The perf gate's rows are the measurement; this is the
/// cheap guard on the one line that would undo it. No other std module may
/// import it either, since any import puts it in that module's closure — except
/// the lens's OWN submodules under `reactive/lens/` (S5's `loaded`), which a
/// program reaches only by naming the lens's path, so their closure is paid by
/// exactly the programs that already pay for the lens. Those submodules are
/// held to the same rule in turn: no file outside the lens's tree may name them.
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
                || path.starts_with(source_root.join("reactive/lens"))
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
