//! Two accuracy gains Order 29 shipped with no pin over them (tracker N62).
//!
//! Both are the same shape of blind spot: a change made the compiler MORE
//! accurate, the suite went green, and nothing in the tree would notice the
//! accuracy going away again. A pin that only ever asked "does this still
//! compile?" is green before and after either of them.
//!
//! * **B254's subject narrowing** removed bogus edges from `call_graph` and
//!   `init_order`. `dispatch_candidates` used to answer a known-receiver
//!   `OnType(Some(_), member)` with every same-named member in the program, so
//!   an unrelated type's inherent member was a reachability successor of a call
//!   it could never be selected at. The async pins that closed B254 assert the
//!   REFUSAL is gone; nothing asserted the EDGE is. Removing an edge is the
//!   direction reachability wants — every consumer of it (dead-item paint,
//!   platform coloring's admission walk, emission's binding reachability,
//!   `init_order`) over-approximates — so the loss would show up as a wrong
//!   answer somewhere else entirely, or as nothing at all.
//!
//! * **B244's composing substitution** is the only thing in the transformer
//!   that grows with monomorphization depth: `emit_instance` clones the map in
//!   force and extends it, so a chain n deep clones n times and the innermost
//!   map carries every outer entry the inner ones did not shadow. The item
//!   filed the cost of a deep chain as unmeasured. It is measured here, over the
//!   corpus, with a bound.

use std::path::{Path, PathBuf};

use vilan_core::call_graph::CallGraph;
use vilan_core::id::Id;
use vilan_core::{BuildOptions, Platform, Program, Workspace, analyze_source, transform};

fn std_spec() -> vilan_core::PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test")
}

/// Analyzes `source` and hands `read` the program, on the big stack the
/// pipeline needs. Only what `read` returns travels back, because a `Program`
/// borrows the leaked source.
fn analyzed<T: Send + 'static>(
    source: &'static str,
    root: PathBuf,
    file: &'static str,
    read: fn(&Program) -> T,
) -> T {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &std_spec(),
                &root,
                Path::new(file),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("the probe analyzes");
            assert!(errors.is_empty(), "the probe compiles cleanly: {errors:?}");
            read(&program)
        })
        .expect("spawn the probe thread")
        .join()
        .expect("the probe thread joins")
}

// --- B254: the edge the narrowing removes ------------------------------------

/// Every function the reachability walk from `main` reaches, as
/// `(name, is_async)` — the walk `dead_items`, platform coloring's admission
/// and `init_order` each run over `CallGraph::successors`, which is where
/// `dispatch_candidates` is consulted.
fn reached_functions(program: &Program) -> Vec<(String, bool)> {
    let graph: &CallGraph = program.call_graph();
    let root = program
        .functions
        .iter()
        .find(|(_, function)| function.name == "main")
        .map(|(id, _)| *id)
        .expect("the probe declares a `main`");
    let mut seen: Vec<Id> = vec![root];
    let mut pending: Vec<Id> = vec![root];
    while let Some(node) = pending.pop() {
        for (successor, _) in graph.successors(program, node) {
            if !seen.contains(&successor) {
                seen.push(successor);
                pending.push(successor);
            }
        }
    }
    let mut reached: Vec<(String, bool)> = seen
        .into_iter()
        .filter_map(|id| program.functions.get(&id))
        .map(|function| (function.name.to_string(), function.is_async))
        .collect();
    reached.sort();
    reached
}

/// The a49 shape, read as a GRAPH: `Client::doubled` is an unrelated type's
/// inherent member that happens to share a name with the trait default the
/// call actually dispatches to, and it must not be a successor of the call.
///
/// `async` is what tells the two `doubled`s apart in the answer — the same
/// property B254's own pins observe through the field-escape refusal, asked of
/// the edge instead of the diagnostic. That refusal is not the fact this file
/// is for: `assert_compiles_without_async` cannot see an edge at all (B254's
/// tombstone says so), and a reachability edge to an unrelated member is wrong
/// whether or not anything downstream happens to be colored by it.
const AN_UNRELATED_SAME_NAMED_MEMBER: &str = r#"
import std::io::print;

struct Client { }

impl Client {
    async fun doubled(self): i32 { 1 }
}

trait Peek {
    fun get(self): i32;
    fun doubled(self): i32 { self.get() * 2 }
}

struct Cell { n: i32 }

impl Cell with Peek {
    fun get(self): i32 { self.n }
}

fun main() {
    let cell = Cell { n = 7 };
    print(cell.doubled());
}
"#;

/// The control, and the reason the pin above is a NARROWING and not a
/// deletion: `Slow` implements the dispatching trait and overrides the default
/// with an async member, so it is a member this receiver's dispatch could
/// select at another instantiation. The over-approximation stands, and the
/// edge is there.
const A_SAME_NAMED_MEMBER_ON_ANOTHER_IMPLEMENTOR: &str = r#"
import std::io::print;

trait Peek {
    fun get(self): i32;
    fun doubled(self): i32 { self.get() * 2 }
}

struct Cell { n: i32 }

impl Cell with Peek {
    fun get(self): i32 { self.n }
}

struct Slow { n: i32 }

impl Slow with Peek {
    fun get(self): i32 { self.n }
    async fun doubled(self): i32 { self.n }
}

fun main() {
    let cell = Cell { n = 7 };
    print(cell.doubled());
}
"#;

#[test]
fn b254_an_unrelated_same_named_member_is_not_a_reachability_successor() {
    let reached = analyzed(
        AN_UNRELATED_SAME_NAMED_MEMBER,
        corpus_dir(),
        "b254_probe.vl",
        reached_functions,
    );
    // The edge that used to be here: `Client::doubled`, an inherent member of a
    // type this program never constructs, reached from `main` because it shares
    // a name with the trait default `cell.doubled()` dispatches to.
    assert!(
        !reached.contains(&("doubled".to_string(), true)),
        "an unrelated type's same-named member is not selectable at this \
         receiver, so it is not reachable from `main`: {reached:?}"
    );
    // Non-vacuity, both ways. The dispatch edge itself must be there (otherwise
    // the assertion above is green because nothing was walked at all), and it
    // must be the SYNC `doubled` — the trait default the call really reaches.
    assert!(
        reached.contains(&("doubled".to_string(), false)),
        "the call still dispatches to `Peek`'s default, so that edge is in the \
         graph: {reached:?}"
    );
    assert!(
        reached.contains(&("get".to_string(), false)),
        "the default's own `self.get()` is a further edge — the walk is \
         transitive, not one step: {reached:?}"
    );
}

#[test]
fn b254_a_same_named_member_on_another_implementor_is_still_a_successor() {
    let reached = analyzed(
        A_SAME_NAMED_MEMBER_ON_ANOTHER_IMPLEMENTOR,
        corpus_dir(),
        "b254_control_probe.vl",
        reached_functions,
    );
    assert!(
        reached.contains(&("doubled".to_string(), true)),
        "`Slow` implements the dispatching trait, so its async override is a \
         member the dispatch could select and the edge stands — this is the \
         control that keeps the pin above a narrowing rather than a deletion: \
         {reached:?}"
    );
}

// --- B244: what the composed substitution costs at its deepest ---------------

/// The corpus program whose monomorphization chain composes the most, found by
/// sweeping every `vilan/test/*.vl` through `transform` at this commit.
const DEEPEST_CORPUS_PROGRAM: &str = "iterator-adapters.vl";

/// The bound on that program's peak.
///
/// **Measured: 11 entries.** The whole corpus, deepest first:
/// `iterator-adapters.vl` 11, `crypto.vl` 5, `ssr-render.vl` 4,
/// `reactive-selector.vl` 4, `blanket-impl.vl` 4, and everything else 3 or
/// below — so the deepest program in the corpus is more than twice the next
/// one, which is what makes it the interesting measurement rather than an
/// arbitrary sample.
///
/// 24 is a little over twice the measurement, and it is a bound rather than an
/// equality on purpose: composition entries are a function of how many generic
/// declarations a chain passes through, so an adapter gained or a trait
/// parameter added moves the number by one or two and should not red a pin
/// about the ORDER of the cost. What it does catch is the thing B244's item
/// worried about — a chain whose composed map grows out of proportion to its
/// depth, which is the shape a quadratic would take here (each level cloning a
/// map that already carries every level below it).
const SUBSTITUTION_PEAK_BOUND: usize = 24;

#[test]
fn b244_the_deepest_corpus_instantiation_composes_a_bounded_substitution() {
    let path = corpus_dir().join(DEEPEST_CORPUS_PROGRAM);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let root = corpus_dir();
    let peak = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &root,
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("the corpus program analyzes");
            assert!(errors.is_empty(), "the corpus program compiles: {errors:?}");
            // The instrument is thread-local and this thread is fresh, but the
            // reset is written out anyway: a macro world is a nested analysis
            // on the SAME thread, so "fresh" is a property of the spawn and not
            // of the program.
            vilan_core::transformer::reset_substitution_peak();
            transform(&program, &BuildOptions::default()).expect("it transforms");
            vilan_core::transformer::substitution_peak()
        })
        .expect("spawn the probe thread")
        .join()
        .expect("the probe thread joins");

    // Non-vacuity, measured rather than assumed: with the composition planted
    // back to the pre-B244 REPLACE, this program peaks at 4 — the largest
    // substitution any single instantiation of its own brings. Composed it is
    // 11. So a peak inside the old figure is this pin telling you the
    // composition is gone, and `> 1` would not have said that.
    assert!(
        peak > 4,
        "`{DEEPEST_CORPUS_PROGRAM}` must actually COMPOSE — a peak of {peak} is \
         inside what its own single instantiations bring (4, measured against a \
         planted REPLACE), so this pin is measuring nothing and B244's \
         composition is gone"
    );
    assert!(
        peak <= SUBSTITUTION_PEAK_BOUND,
        "the deepest corpus instantiation composes {peak} substitution \
         entries, over the bound of {SUBSTITUTION_PEAK_BOUND}. It measured 11 \
         when the bound was set (tracker N62). A jump means a monomorphization \
         chain got deeper or the composition stopped shadowing — worth reading \
         before the number is raised."
    );
}
