//! Behavioural gates for the reactive core's SELECTION surface — the lazy
//! subscribe (`Source::on_change`, backlog A43) and the per-key selector
//! (`selector`, backlog A44).
//!
//! The corpus gate next door pins EMISSION (bytes) and the interpreter gate
//! pins node-vs-interpreter agreement; neither pins what a program actually
//! prints against an expectation. These do: each builds a real program with the
//! real CLI and asserts on its stdout, in the shape `reactive_lifetimes.rs`
//! established.
//!
//! One gate here is a DECLARATION gate rather than a behavioural one, and says
//! so at its own site: `combine`'s redundant construction-time writes are not
//! observable from vilan code at all (nothing is subscribed to the derived cell
//! while they happen), exactly as `dom_events.rs`'s `retains` gate is over
//! declarations because retention is not observable through a listener.

use std::path::{Path, PathBuf};
use std::process::Command;

/// A fresh temp directory for one test's program.
fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "vilan_reactive_selection_{tag}_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// Builds a bare program with the real CLI and runs it under node, returning
/// its stdout. Fails loudly with both streams.
fn build_and_run(tag: &str, program: &str) -> String {
    let dir = temp_project(tag);
    std::fs::create_dir_all(&dir).expect("create the program directory");
    let source = dir.join("app.vl");
    std::fs::write(&source, program).expect("write the program");

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&source)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new("node")
        .arg("app.mjs")
        .current_dir(&dir)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "the program failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

// --- A43: the lazy subscribe -------------------------------------------------

const EAGER_AND_LAZY: &str = r#"import std::reactive::{ Disposable, Signal, SignalCell, comp };

fun main() {
	let count: SignalCell<i32> = Signal::new(1);
	let eager = count.sub(|value| print(i"sub {value}"));
	let lazy = count.on_change(|value| print(i"on_change {value}"));
	print("attached");
	count.set(2);
	eager.dispose();
	lazy.dispose();
	count.set(3);
	let (_built, scope) = comp(|| {
		count.effect_on_change(|value| print(i"effect_on_change {value}"));
	});
	count.set(4);
	scope.dispose();
	count.set(5);
}

main();
"#;

/// The pair on one signal: `sub` fires for the value already held, `on_change`
/// does not, and both then fire on every change. Red with `on_change` spelled
/// `sub` — an extra `on_change 1` lands before `attached`.
#[test]
fn on_change_skips_the_value_the_source_already_holds() {
    let stdout = build_and_run("eager_and_lazy", EAGER_AND_LAZY);
    assert_eq!(
        stdout, "sub 1\nattached\nsub 2\non_change 2\neffect_on_change 4\n",
        "the eager/lazy pair did not print as documented"
    );
}

const HAND_WRITTEN_SOURCE: &str = r#"import std::reactive::{ Disposable, Signal, SignalCell, Source, Subscription };

struct Stored<T> {
	inner: SignalCell<T>,
}

impl Stored<type T> with Source<T> {
	fun get(self): T {
		self.inner.get()
	}

	[must_use]
	fun on_change(self, observer: |T| void): Subscription {
		self.inner.on_change(observer)
	}
}

fun main() {
	let stored = Stored { inner = Signal::new(10) };
	let watched = stored.on_change(|value| print(i"stored {value}"));
	print("attached");
	stored.inner.set(11);
	stored.inner.set(12);
	watched.dispose();
	stored.inner.set(13);
	// Q5's widening: `map` is a `Source` member, so it derives off `Stored`.
	let labelled = stored.map(|value| i"n={value}");
	print(labelled.get());
	let shown = labelled.sub(|value| print(i"label {value}"));
	stored.inner.set(14);
	shown.dispose();
}

main();
"#;

/// A `Source` that implements only `get`/`on_change` — the two A49 made the
/// trait's requirements — gets `map` (reactive-traits Q5's widening) and the
/// whole eager family from the trait defaults. Here the LAZY member is the
/// impl's own; `an_on_change_only_source_gets_a_working_eager_sub` below is the
/// derived half.
#[test]
fn the_trait_defaults_serve_a_hand_written_source() {
    let stdout = build_and_run("hand_written_source", HAND_WRITTEN_SOURCE);
    assert_eq!(
        stdout, "attached\nstored 11\nstored 12\nn=13\nlabel n=13\nlabel n=14\n",
        "the trait defaults did not serve a hand-written Source as documented"
    );
}

// --- A49: `on_change` is the requirement, `sub` the derived eager form -------

const ON_CHANGE_ONLY_SOURCE: &str = r#"import std::reactive::{ Disposable, Signal, SignalCell, Source, Subscription, comp };

struct Stored<T> {
	inner: SignalCell<T>,
}

impl Stored<type T> with Source<T> {
	fun get(self): T {
		self.inner.get()
	}

	[must_use]
	fun on_change(self, observer: |T| void): Subscription {
		self.inner.on_change(observer)
	}
}

fun main() {
	let stored = Stored { inner = Signal::new(10) };
	// The trait's DEFAULT `sub`, over an impl that never wrote one.
	let eager = stored.sub(|value| print(i"eager {value}"));
	print("attached");
	stored.inner.set(11);
	eager.dispose();
	stored.inner.set(12);
	// And the owner-registered eager form, built the same way.
	let (_built, scope) = comp(|| {
		stored.effect(|value| print(i"effect {value}"));
	});
	stored.inner.set(13);
	scope.dispose();
	stored.inner.set(14);
}

main();
"#;

/// The A49 inversion, from the outside: an implementation that writes `get` and
/// `on_change` and NOTHING else gets a working eager `sub` and a working eager
/// `effect`, each firing exactly once with the value the source already holds
/// and then once per change.
///
/// `eager 10` before `attached` is the immediate call; that it appears ONCE is
/// the claim the old arrangement could not make — a default built the other way
/// round subscribes eagerly and then discards a call, so the number of
/// immediate calls is decided by the impl rather than by the trait. `effect 13`
/// with no `effect 14` after it is the owner registration, which rides
/// `effect_on_change`.
///
/// Proven red first by planting a default `sub` that only forwards to
/// `on_change` and drops the immediate call: the run comes back
/// `attached\neager 11\n…` — the `eager 10` before `attached` is gone, which is
/// the whole of what "derived" has to mean here.
#[test]
fn an_on_change_only_source_gets_a_working_eager_sub() {
    let stdout = build_and_run("on_change_only_source", ON_CHANGE_ONLY_SOURCE);
    assert_eq!(
        stdout, "eager 10\nattached\neager 11\neffect 12\neffect 13\n",
        "the derived eager members did not serve an on_change-only Source"
    );
}

/// `combine` seeds its derived signal from the snapshot and then attaches
/// LAZILY, instead of letting each input's eager `sub` re-set that same value
/// once per input.
///
/// A DECLARATION gate, deliberately. The redundant writes happen inside
/// `combine`'s own body, before the derived cell it just created can have a
/// single subscriber, and `SignalCell::set` over an empty subscriber list is a
/// no-op in every observable respect — no notification, no id, no allocation a
/// vilan program can count. There is therefore no program whose output moves,
/// and the honest pin is over the line that carries the claim. (The corpus byte
/// gate is the second half: `combine`'s emitted body is in a golden, so a
/// revert to `sub` diverges it.)
#[test]
fn combine_attaches_to_its_inputs_without_the_first_call() {
    let source = std::fs::read_to_string(std_dir().join("src/reactive.vl"))
        .expect("read std::reactive's source");
    let body = source
        .split_once("fun combine<T: (2..)>")
        .expect("combine is declared in std::reactive")
        .1;
    let body = body
        .split_once("\n}\n")
        .expect("combine's body is delimited")
        .0;
    assert!(
        body.contains("source.on_change(|_| {"),
        "combine must attach to each input with on_change; its body is:\n{body}"
    );
    assert!(
        !body.contains("source.sub("),
        "combine must not attach eagerly — each eager attach re-sets the \
         derived signal to the value it was just seeded with; its body is:\n{body}"
    );
}

// --- A44: the per-key selector ----------------------------------------------

/// A thousand rows, each holding the selector's cell for its own key and
/// counting the notifications that cell delivers.
const THOUSAND_ROWS: &str = r#"import std::reactive::{ Disposable, Owner, Signal, SignalCell, Subscription, run_with_owner, selector };

fun main() {
	let current: SignalCell<i32> = Signal::new(0);
	let selected = selector(current);
	let rows = Owner::new();
	mut watches: List<Subscription> = [];
	mut notifications = 0;
	mut key = 0;
	for key < 1000 {
		let cell = run_with_owner(rows, || selected.of(key));
		watches.push(cell.on_change(|_| {
			notifications += 1;
		}));
		key += 1;
	}
	print(i"wired={notifications} entries={selected.cells.read().len()}");
	current.set(500);
	print(i"one change={notifications}");
	current.set(900);
	print(i"two changes={notifications}");
	// A key nobody holds a cell for writes nothing at all on the way in.
	current.set(4000);
	print(i"absent key={notifications}");
	rows.dispose();
	print(i"after dispose={selected.cells.read().len()}");
	for watch in watches {
		watch.dispose();
	}
}

main();
"#;

/// The whole point of A44, measured: a selection change over a thousand live
/// rows writes exactly TWO cells — the key that left and the key that arrived —
/// so the notification count moves by 2 per change and not by 1000.
///
/// Red-first: spelling the row's cell `current.map(|value| value == key)`
/// instead — the derivation this exists to replace — reports 1000 per change.
/// The last two lines are the other half of the design: an incoming key nobody
/// holds a cell for costs ONE write (the outgoing one) and nothing more, and
/// the per-key entries are the ROWS' — the map empties when their scope goes.
#[test]
fn a_selection_change_over_a_thousand_rows_writes_two_cells() {
    let stdout = build_and_run("thousand_rows", THOUSAND_ROWS);
    assert_eq!(
        stdout,
        "wired=0 entries=1000\none change=2\ntwo changes=4\nabsent key=5\n\
         after dispose=0\n",
        "the selector did not notify O(2) per change over a thousand rows"
    );
}

/// `enqueue`'s dedup at scale, as a RATIO between two measurements taken in one
/// process — the shape `perf_baseline.rs`'s const-pass pin uses, because a
/// number of milliseconds is a claim about the machine and a ratio is a claim
/// about the algorithm.
///
/// Four times the observers on one signal is four times the work for a linear
/// dedup and sixteen times for the scan of the pending queue this replaced.
/// Measured on the development machine: 80 ms → 229 ms (ratio 2.9) keyed,
/// 696 ms → 9630 ms (ratio 13.8) scanning. The threshold sits between them.
const ENQUEUE_SCALING: &str = r#"import std::reactive::{ Signal, SignalCell, Subscription, batch };
import std::time::now_millis;

fun wave_millis(count: i32): f64 {
	let source: SignalCell<i32> = Signal::new(0);
	mut watches: List<Subscription> = [];
	mut hits = 0;
	mut index = 0;
	for index < count {
		watches.push(source.on_change(|_| {
			hits += 1;
		}));
		index += 1;
	}
	let start = now_millis();
	mut round = 0;
	for round < 20 {
		batch(|| {
			source.set(round + 1);
		});
		round += 1;
	}
	now_millis() - start
}

fun main() {
	// Warm the engine so the first measurement is not the one that pays for it.
	let _warm = wave_millis(500);
	let small = wave_millis(2000);
	let large = wave_millis(8000);
	print(i"small={small} large={large}");
}

main();
"#;

#[test]
fn one_turns_dedup_scales_with_its_queue_and_not_with_its_square() {
    let stdout = build_and_run("enqueue_scaling", ENQUEUE_SCALING);
    let numbers: Vec<f64> = stdout
        .split_whitespace()
        .filter_map(|field| field.split_once('=').map(|(_, value)| value))
        .map(|value| value.parse::<f64>().expect("a millisecond measurement"))
        .collect();
    assert_eq!(
        numbers.len(),
        2,
        "expected two measurements; got:\n{stdout}"
    );
    let (small, large) = (numbers[0], numbers[1]);
    // A measurement at or below the clock's resolution cannot carry a ratio.
    assert!(
        small >= 4.0,
        "the small wave was too fast to measure ({small} ms); got:\n{stdout}"
    );
    assert!(
        large < small * 8.0,
        "four times the observers cost {large} ms against {small} ms — more \
         than the 8x ceiling, so the dedup is scanning its queue again rather \
         than keying it (linear is ~4x, quadratic ~16x)"
    );
}
