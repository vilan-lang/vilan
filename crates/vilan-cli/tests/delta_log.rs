//! `std::reactive`'s delta log — the change structure A112 S1 lifted out of
//! `KeyedCell` (`proposal/incremental-collections.md` §4, §5, §10).
//!
//! Three things are gated here, and the first is the reason the other two are
//! cheap:
//!
//! 1. [`the_derivative_law_holds_across_four_hundred_randomized_turns`] — THE
//!    LAW, `f(a (+) da) == f(a) (+) f'(a, da)`, as the paper's randomized
//!    property test. The program is a corpus program (`vilan/test/delta-law.vl`)
//!    so the byte gate compiles it on every run, but the corpus gate compares
//!    BYTES and the differentials compare one node run against another — a
//!    program that panics identically twice passes both. So the law is only
//!    actually asserted by running it and reading the exit code, which is what
//!    this does. Proven non-vacuous by planting the log bug the design is
//!    against: `since` answering a cursor behind `base` with an empty list
//!    instead of `None` (silently lost history) reddens it at turn 12, with the
//!    diverging lists printed.
//! 2. [`the_logs_two_off_by_one_behaviours_are_what_the_doc_says`] — §4.1's two
//!    corrections, which are easy to state wrongly and were both stated wrongly
//!    before the paper measured them: a log with NO consumers holds exactly the
//!    op just recorded (one, not zero — `record` trims and then pushes), and the
//!    log is trimmed at the next WRITE and not at the drain.
//! 3. [`reconciles_index_answers_exactly_what_the_scan_answered`] — M82's
//!    position index inside `reconcile`, held to the scan it replaces over
//!    1,415 cases, including a key whose `==` and hash are both coarser than
//!    value identity (which a first attempt got wrong) and, since A125, a key
//!    whose HASH alone is coarse.
//! 4. [`a_reversal_costs_one_key_comparison_per_row`] — A125's bound, COUNTED:
//!    the key's `==` tallies every call, so a 1,000-row reversal's cost is a
//!    number rather than a clock reading. It lives here and not in the corpus
//!    because the corpus is also the native differential's enumeration, and the
//!    native backend accepts this program and then emits Rust that does not
//!    compile (`same(old_items[found], item)` moves out of a `Vec` of a
//!    non-`Copy` element) — a backend defect reported rather than worked
//!    around.
//! 5. [`a_list_cell_runs_its_derivation_once_per_arriving_element`] — A112 S2:
//!    `ListCell`, the two sequence traits and `map_each`, over the corpus
//!    program `vilan/test/list-cell.vl`. Its own `panic`s are the assertions
//!    and the byte gate does not run them, so — exactly as for the law above —
//!    the program is RUN here and its exit code read.
//! 6. [`std_reactive_imports_nothing_from_std_wire`] — the layering the lift
//!    must not invert. `SeqOp` is a `std::reactive` type, `Delta` a `std::wire`
//!    one, and the edge between them lives in `std::rpc`. A grep over the two
//!    reactive-layer files is honest here and costs microseconds: there is no
//!    import to find, so nothing subtler than the text is needed.
//! 7. [`std_delta_feed_admits_list_sources_and_refuses_everything_else`] —
//!    A112 S3's `DeltaFeed<T>` and its two blankets must make nothing a source
//!    that was not one. Its ruled form, `with Source<List<T>>` beside its own
//!    blanket over `Source<List<T>>`, made every `Source` bound in every
//!    program admit ANY type before solver-40b's fix; held here against std's
//!    REAL trait, whichever spelling it carries, where the solver's own pins
//!    hold a user-written copy.

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// The repository's `vilan/std` directory — what `VILAN_STD` points the built
/// binary at, so a probe compiles against THIS tree's std and not the embedded
/// copy of whatever was last released.
fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

fn std_source(relative: &str) -> String {
    let path = std_dir().join("src").join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path:?}: {error}"))
}

/// Builds `program` in a staging directory of its own and runs it under node,
/// returning its stdout. A failing build or a non-zero exit panics with
/// everything the run said.
///
/// The staging copy is not an optimization: `vilan build vilan/test/x.vl`
/// writes `vilan/test/x.mjs` IN PLACE, which is the committed golden, so a gate
/// that built a corpus program where it lives would rewrite the artifact the
/// byte gate is holding.
fn build_and_run(tag: &str, source: &str, contents: &str) -> String {
    let directory = support::scratch_dir(&format!("vilan_delta_log_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&directory, &error)));
    let program = directory.join(source);
    std::fs::write(&program, contents)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&program, &error)));

    let built = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&program)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        built.status.success(),
        "{source} must build:\n{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let emitted = program.with_extension("mjs");
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

/// The law, over 400 seeded turns of 1–4 ops each, with a cursor drained every
/// third turn (answered with OPS) and one drained every twentieth (which falls
/// past the log's `base` and is answered with a `Reset`).
///
/// Every assertion is INSIDE the program, as a `panic`: the law per turn for
/// the always-current consumer and for whichever mirror drained, one
/// notification per turn whatever the op count, both cursor paths exercised,
/// and the incremental call count strictly below the naive rerun's. The tail it
/// prints is the arithmetic, and it is held here so a run that stops asserting
/// — a generator that no longer reaches `clear`, a log that no longer overflows
/// — reds instead of passing quietly.
#[test]
fn the_derivative_law_holds_across_four_hundred_randomized_turns() {
    let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test/delta-law.vl");
    let contents = std::fs::read_to_string(&program)
        .unwrap_or_else(|error| panic!("read {program:?}: {error}"));
    let stdout = build_and_run("law", "delta-law.vl", &contents);

    let expected = "turns=400 checks=555 failures=0\n\
        lagging cursors: ops drained=845 resets=56\n\
        g calls: incremental=1223 (splice/set_at=806, reset=414) naive-rerun=6651\n\
        final length 11, log held 30 ops, base 1006, version 1036\n";
    assert_eq!(
        stdout, expected,
        "the law run's arithmetic moved. The seeded generator is deterministic, \
         so a different tail means the ops, the derivative or the log changed — \
         read which line moved before regenerating anything. (`checks` counts \
         the law read once per turn plus once per mirror drain; `resets` is the \
         far mirror falling past `base`, which is the lag path and must not \
         reach zero.)"
    );
}

/// §4.1's two corrections, each as its own printed fact.
///
/// The cell here is deliberately the SHIPPED one — `KeyedCell` — because these
/// are properties of the lifted log and the point of the lift is that its
/// consumer sees no change: the same two numbers came off the pre-lift cell
/// when the paper measured it (`probes/probe_keyed_cell_today.out`).
#[test]
fn the_logs_two_off_by_one_behaviours_are_what_the_doc_says() {
    const PROGRAM: &str = r#"import std::io::print;
import std::reactive::{ FlushPolicy, turn };
import std::rpc::KeyedCell;
import std::wire::Keyed;

struct Row {
	id: i32,
}

impl Row with Keyed<i32> {
	fun key(self): i32 {
		self.id
	}
}

fun main() {
	let cell: KeyedCell<i32, Row> = KeyedCell<i32, Row>::new([Row { id = 1 }]);

	// No consumer at all: `record` TRIMS and then pushes, so the log holds
	// exactly the op just recorded. One, not zero.
	cell.insert(Row { id = 2 });
	cell.insert(Row { id = 3 });
	print(i"no cursor: held={cell.log.held()} at={cell.log.at()} oldest={cell.log.oldest()}");

	// One cursor, four writes in ONE turn, one drain: the log still holds four
	// afterwards. The trim happens at the next WRITE, not at the drain.
	let cursor = cell.cursor();
	turn(FlushPolicy::AtSuspension, || {
		cell.insert(Row { id = 4 });
		cell.update(2, |&mut row| {
			row.id = 2;
		});
		cell.remove(1);
		cell.insert(Row { id = 5 });
	});
	let drained = cell.since(cursor).len();
	print(i"after drain: drained={drained} held={cell.log.held()}");

	// And they go on the next one.
	cell.insert(Row { id = 6 });
	print(i"after the next write: held={cell.log.held()}");
}

main();
"#;
    let stdout = build_and_run("offbyone", "offbyone.vl", PROGRAM);
    assert_eq!(
        stdout,
        "no cursor: held=1 at=2 oldest=1\n\
         after drain: drained=4 held=4\n\
         after the next write: held=1\n",
        "the log's two documented off-by-one behaviours moved. Both are \
         harmless and both are WRITTEN DOWN in `DeltaLog::record`/`trim` \
         (incremental-collections.md §4.1) — a change here is a doc change \
         first."
    );
}

/// M82: the indexed `reconcile` answers what the scan answered, plan for plan.
///
/// The program carries the pre-index scan verbatim and compares every case
/// against it, so this gate is a DIFFERENTIAL rather than a golden: it cannot
/// be satisfied by regenerating anything. 1,415 cases — the named shapes, 600
/// randomized pairs over a small key alphabet so duplicates and reorders are
/// dense, 400 randomized pairs of a key whose equality AND hash are both
/// coarser than value identity, and (A125) 400 more of a key whose HASH alone
/// is coarse, so every chain the index walks is full of entries that are not
/// `==` and the walk has to step past them. Non-vacuous by construction: the
/// first version of the index answered the chain's candidate outright and this
/// reddened at `tags 22` with both plans printed.
#[test]
fn reconciles_index_answers_exactly_what_the_scan_answered() {
    let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test/reconcile-index.vl");
    let contents = std::fs::read_to_string(&program)
        .unwrap_or_else(|error| panic!("read {program:?}: {error}"));
    let stdout = build_and_run("reconcile", "reconcile-index.vl", &contents);
    assert_eq!(
        stdout, "cases=1415 differences=0\n",
        "the reconcile differential moved. A difference is printed as a panic \
         naming the case and both plans; a changed COUNT means the shapes \
         moved, which is a deliberate edit to the program and not a number to \
         update blindly."
    );
}

/// A125 — a REORDER is LINEAR, counted rather than timed.
///
/// M82's index made every change that did not reorder one pass; a reorder
/// stayed quadratic, because nothing bound the key's hash to the key's
/// equality and the stretch from the smallest unclaimed index up to the
/// candidate had to be scanned with `==` as well. That stretch is empty when
/// nothing moved and is the WHOLE prefix when a list is reversed — N(N+1)/2 key
/// comparisons, 500,500 at 1,000 rows. `K: Hashable` states the obligation, so
/// the chain holds every candidate and the scan is gone.
///
/// The count is the assertion: `Row`'s `==` tallies every call, so this reads a
/// number the machine cannot make faster and a clock cannot make slower. One
/// comparison per row is the answer and the guard is 4N, so the scan coming
/// back reds at 250 rows rather than being noticed as a hang — proven by
/// planting the scan back, which answers 31,375 at 250 rows (250 * 251 / 2).
/// The plan is checked as well as its cost: a cheaper wrong answer is still
/// wrong.
const REORDER_COST: &str = r#"import std::compare::PartialEq;
import std::hash::{ Hash, Hashable };
import std::io::{ panic, print };
import std::reactive::{ RowStep, reconcile };
import std::shared::Shared;

let comparisons: Shared<i32> = Shared::new(0);

struct Row {
	id: i32,
}

impl Row with PartialEq {
	fun eq(self, other: Row): bool {
		comparisons.write() = comparisons.read() + 1;
		self.id == other.id
	}
}

impl Row with Hashable {
	fun hash(self): Hash {
		self.id.hash()
	}
}

/// Reverse `rows` rows, check the plan is the RIGHT one, and answer what the
/// key comparisons cost.
fun reversal(rows: i32): i32 {
	mut old: List<Row> = [];
	mut at = 0;
	for at < rows {
		old.push(Row { id = at });
		at += 1;
	}
	mut fresh: List<Row> = [];
	at = rows - 1;
	for at >= 0 {
		fresh.push(Row { id = at });
		at -= 1;
	}
	comparisons.write() = 0;
	let plan = reconcile(old, old, fresh, |item| item, |_before, _after| true);
	// A reversal keeps every row and removes none — new position `i` is old
	// position `rows - 1 - i`. A cheaper wrong answer is still wrong.
	if plan.steps.len() != rows {
		panic(i"{rows}: the plan has {plan.steps.len()} steps");
	}
	if plan.removed.len() != 0 {
		panic(i"{rows}: a reversal removes nothing, not {plan.removed.len()}");
	}
	mut index = 0;
	for step in plan.steps {
		match step {
			RowStep::Keep(let held) => {
				if held != rows - 1 - index {
					panic(i"{rows}: step {index} kept {held}");
				}
			},
			_ => panic(i"{rows}: step {index} is not a Keep"),
		}
		index += 1;
	}
	comparisons.read()
}

fun main() {
	for size in [250, 500, 1000] {
		let count = reversal(size);
		if count > 4 * size {
			panic(i"{size} rows reversed cost {count} key comparisons — not linear");
		}
		print(i"reverse {size}: {count} key comparisons");
	}
}

main();
"#;

#[test]
fn a_reversal_costs_one_key_comparison_per_row() {
    let stdout = build_and_run("reorder", "reconcile-reorder.vl", REORDER_COST);
    assert_eq!(
        stdout,
        "reverse 250: 250 key comparisons\n\
         reverse 500: 500 key comparisons\n\
         reverse 1000: 1000 key comparisons\n",
        "a reversal's key-comparison count moved. One per row is the index \
         answering each key from its own chain; anything growing with the \
         square is M82's coarse-equality scan back again."
    );
}

/// A112 S2 — `ListCell`, `SequenceCell`, `Sequence`/`Tracked` and `map_each`.
///
/// The program is `vilan/test/list-cell.vl`, so the byte gate compiles it on
/// every run; what the byte gate cannot do is READ its assertions, which are
/// `panic`s. Ten claims, in the program's own order: the item's original ask
/// (one `ran` for one push), k pushes -> k calls of `g` with a removal and a
/// `clear` costing none, THE LAW after every mutation against a naive rerun,
/// the twelve defaults each reaching the one primitive, four mutations in one
/// `edit` being ONE notification and FOUR ops, a body written against the
/// `Sequence<T>` BOUND running on the recorder, `set(whole)` costing N where
/// `reconcile_to(whole)` costs what changed (and an unchanged
/// `reconcile_to` costing nothing at all), a `Move` costing zero, a cursor
/// past the log's limit answered with a `Reset` whose application the law
/// checks, an out-of-range ask CLAMPED alike by the cell and by the recorder
/// (they run one splice), and THE LAW over a 300-turn random walk of the whole
/// vocabulary with a second `map_each` CHAINED on the first — both laws at
/// every settle, at most one notification per turn, and the two derivations
/// together cheaper than one naive rerun (1,081 + 1,081 calls against 4,196).
///
/// Non-vacuous, proven by planting the bugs the slice is against:
/// `map_each`'s `Splice` arm re-mapping the whole list reds at
/// `3 pushes: 6, expected 3`; `edit` publishing per op instead of once reds
/// at `one edit, one notification: 5, expected 1`; `map_each`'s `Move` arm
/// forwarding a count of one reds ONLY in the walk (the fixed claims move one
/// element), at `turn 7: first[2] is 61, not 107`; and the recorder's splice
/// without the shared clamp reds at the clamped `edit` with
/// `index out of bounds: the length is 1 but the index is 1`.
#[test]
fn a_list_cell_runs_its_derivation_once_per_arriving_element() {
    let program = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test/list-cell.vl");
    let contents = std::fs::read_to_string(&program)
        .unwrap_or_else(|error| panic!("read {program:?}: {error}"));
    let stdout = build_and_run("listcell", "list-cell.vl", &contents);
    assert_eq!(
        stdout,
        "ran\n\
         after one push: 4\n\
         twelve defaults: calls=11 notifications=14\n\
         after edit: lll m \n\
         after fill: lll m from-fill-1 from-fill-2 \n\
         after move: oo qqqq ppp n \n\
         lagged: held=0 calls=9\n\
         total: calls=32 notifications=20\n\
         clamped: front \n\
         walk: turns=300 silent=1 length=5 g=1081 h=1081 rerun=4196\n",
        "A112 S2's counts moved. `ran` printed once is the item's own ask; the \
         call counts are `g` running per ARRIVING element and not per element; \
         `notifications` is one per write and one per `edit`, whatever the \
         `edit` did. A changed number here is a semantics change, not a \
         golden."
    );
}

/// The layering the lift must not invert (§10, and A54's own sentence about
/// where `KeyedCell` lives).
#[test]
fn std_reactive_imports_nothing_from_std_wire() {
    for file in ["reactive.vl", "delta.vl"] {
        let source = std_source(file);
        for line in source.lines() {
            let trimmed = line.trim_start();
            assert!(
                !(trimmed.starts_with("import pkg::wire")
                    || trimmed.starts_with("export import pkg::wire")),
                "{file} imports `std::wire`: {line:?}\n\
                 The reactive layer's op vocabulary is `SeqOp`/`MapOp`/`SetOp`; \
                 `Delta` is the WIRE's, and the edge between them is \
                 `KeyedCell::since` in `std::rpc`, which is the one type that \
                 knows both the keys and the positions. An import here would \
                 make the whole reactive core depend on the `Patch` frame."
            );
        }
    }
}

/// Checks `contents` as a one-file program and answers the compiler's stderr,
/// asserting the check FAILED — the refusal half of the pin below.
fn refused(tag: &str, contents: &str) -> String {
    let directory = support::scratch_dir(&format!("vilan_delta_log_{tag}_{}", std::process::id()));
    std::fs::create_dir_all(&directory)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&directory, &error)));
    let program = directory.join("app.vl");
    std::fs::write(&program, contents)
        .unwrap_or_else(|error| panic!("{}", support::storage_failure(&program, &error)));
    let checked = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("check")
        .arg(&program)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan check");
    let stderr = String::from_utf8_lossy(&checked.stderr).into_owned();
    assert!(
        !checked.status.success(),
        "{tag} must be REFUSED, and checked clean:\n{}{stderr}",
        String::from_utf8_lossy(&checked.stdout)
    );
    stderr
}

/// A112 S3's `DeltaFeed` must answer the question it exists for (a `ListCell`
/// has a feed, a plain `SignalCell<List<T>>` has none) and must NOT make
/// anything else a source. It ships as the two-bound fallback (no supertrait;
/// a consumer writes `S: Source<List<T>> + DeltaFeed<T>`), and this pin is
/// what the switch to the ruled `with Source<List<T>>` form must keep green.
///
/// Non-vacuous by the compiler it was found on: on 3e4e6c51 (before
/// solver-40b's guard in `satisfies_trait_bound`) with the ruled supertrait
/// form in std, all four refusals below checked CLEAN and ran — a plain struct and an `i32` passed as
/// `Source<T>`, a `SignalCell<i32>` as `Source<List<T>>` (printing
/// `undefined`), an `i32` as `DeltaFeed<T>` — because the blanket was a
/// candidate for its own supertrait and the recursion's depth cap answered yes.
/// The control is what keeps the refusals from passing by refusing everything.
#[test]
fn std_delta_feed_admits_list_sources_and_refuses_everything_else() {
    let stdout = build_and_run(
        "feed_control",
        "feed_control.vl",
        r#"import std::io::print;
import std::option::Option::{ self, None, Some };
import std::reactive::{ DeltaFeed, ListCell, Signal, SignalCell, Source };

fun logged<T, S: Source<List<T>> + DeltaFeed<T>>(source: S): str {
	match source.delta_cursor() {
		Some(let _cursor) => i"log over {source.get().len()}",
		None => i"none over {source.get().len()}",
	}
}

fun main() {
	let cell: ListCell<str> = ListCell<str>::of(["a"]);
	let plain: SignalCell<List<str>> = Signal::new(["a", "b"]);
	print(i"{logged(cell)} / {logged(plain)}");
}

main();
"#,
    );
    assert_eq!(
        stdout, "log over 1 / none over 2\n",
        "a `ListCell` answers a feed and a plain cell answers none"
    );

    let cases: [(&str, &str, &str); 4] = [
        (
            "feed_struct_as_source",
            "struct Plain {\n\tn: i32,\n}\n\nfun src<T, S: Source<T>>(source: S): i32 {\n\t1\n}\n\nfun main() {\n\tlet plain = Plain { n = 1 };\n\tprint(i\"{src(plain)}\");\n}\n",
            "'Plain' does not implement trait 'Source<T>'",
        ),
        (
            "feed_integer_as_source",
            "fun src<T, S: Source<T>>(source: S): i32 {\n\t1\n}\n\nfun main() {\n\tprint(i\"{src(5)}\");\n}\n",
            "'i32' does not implement trait 'Source<T>'",
        ),
        (
            "feed_scalar_cell_as_list_source",
            "fun src<T, S: Source<List<T>>>(source: S): List<T> {\n\tsource.get()\n}\n\nfun main() {\n\tlet number: SignalCell<i32> = Signal::new(3);\n\tlet got: List<str> = src(number);\n\tprint(i\"{got.len()}\");\n}\n",
            "'SignalCell<i32>' does not implement trait 'Source<List<str>>'",
        ),
        (
            "feed_integer_as_feed",
            "fun feed<T, S: DeltaFeed<T>>(source: S): i32 {\n\t1\n}\n\nfun main() {\n\tprint(i\"{feed(5)}\");\n}\n",
            "'i32' does not implement trait 'DeltaFeed<T>'",
        ),
    ];
    for (tag, body, wanted) in cases {
        let program = format!(
            "import std::io::print;\nimport std::reactive::{{ DeltaFeed, Signal, SignalCell, Source }};\n\n{body}\nmain();\n"
        );
        let stderr = refused(tag, &program);
        assert!(
            stderr.contains(wanted),
            "{tag}: expected the refusal {wanted:?}, got:\n{stderr}"
        );
    }
}
