//! Collections per shape (`proposal/reactive-layers.md` §6, tracker A142 S4):
//! the collection pipe (`CollPipe`, R33) and its operators, which follow the
//! reactive value a closure returns (R9, `IntoFlow` R31, `map`'s R35).
//!
//! What the pins hold is the COST model as much as the values: a plain closure
//! allocates nothing per element, a following closure holds one subscription
//! per element and an element's change is one op, and a removed element releases
//! what its run built. The values are held against an oracle recomputed from scratch
//! after every edit of a seeded random walk, on both backends
//! (`native_differential`'s collection probe runs the same walk natively).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- S4: the operators -------------------------------------------------------

/// The operators over a `ListCell`, through a chain and a seal: the closures run
/// for what changed, and the sealed collection is what a whole recomputation
/// would say.
#[test]
fn a142_s4_operators_chain_and_seal_into_a_granular_source() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ ListCell, SequenceCell, comp };

        fun main() {
            let numbers: ListCell<i32> = ListCell::of([1, 2, 3, 4]);
            mut runs = 0;
            let (big, scope) = comp(|| numbers.filter(|n| n > 2).map(|n| {
                runs += 1;
                n * 10
            }).memo());
            print(big.get());
            print(runs);
            numbers.push(7);
            print(runs);
            numbers.remove_at(0);
            print(runs);
            // A sealed collection is a granular source again.
            let (again, scope2) = comp(|| big.map(|n| n + 1).memo());
            numbers.set_at(0, 9);
            print(big.get());
            print(again.get());
            scope2.dispose();
            scope.dispose();
        }
        main();
        "#,
        "[ 30, 40 ]\n2\n3\n3\n[ 90, 30, 40, 70 ]\n[ 91, 31, 41, 71 ]\n",
    );
}

/// "A plain `filter` allocates nothing per element" (§6.1): after the boundary
/// is set up, a hundred arrivals and changes through a plain predicate make no
/// element record and no owner list. The instruments are std's own
/// (`element_holds_allocated`, `owner_lists_allocated`, both `[internal]`).
#[test]
fn a142_s4_a_plain_filter_allocates_nothing_per_element() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ ListCell, SequenceCell, comp, element_holds_allocated, owner_lists_allocated };

        fun main() {
            let numbers: ListCell<i32> = ListCell::of([1, 2, 3, 4]);
            let (evens, scope) = comp(|| numbers.filter(|n| n % 2 == 0).memo());
            let holds = element_holds_allocated();
            let owners = owner_lists_allocated();
            mut at = 0;
            for at < 100 {
                numbers.push(at);
                numbers.set_at(0, at);
                at += 1;
            }
            print(evens.get().len());
            print(element_holds_allocated() - holds);
            print(owner_lists_allocated() - owners);
            scope.dispose();
        }
        main();
        "#,
        "52\n0\n0\n",
    );
}

/// The transient stand-in the S4 pins run against: `TransientSource` is
/// transient-44's (S3) and not on `next` yet, so a `Mirror` is a
/// `Source<Option<str>>` that is `None` while pending, answers `is_pending()` as a
/// fresh pipe per call, and counts the subscriptions it holds.
const MIRROR: &str = r#"
import std::option::Option::{ self, None, Some };
import std::reactive::{ Derive, ListCell, Signal, SignalCell, Source, Subscriber, Subscription, comp };
import std::shared::Shared;

struct Mirror {
    state: SignalCell<Option<str>>,
    live: Shared<i32>,
}

impl Mirror with Source<Option<str>> {
    fun get(self): Option<str> {
        self.state.get()
    }

    [must_use]
    fun on_settle(self, subscriber: Subscriber): Subscription {
        self.live.write() = self.live.read() + 1;
        let handle = self.state.on_settle(subscriber);
        let live = self.live;
        let previous = handle.release.read();
        handle.release.write() = Some(|| {
            live.write() = live.read() - 1;
            match previous {
                Some(let earlier) => earlier(),
                None => {},
            }
        });
        handle
    }
}

impl Mirror {
    fun is_pending(self): Derive<SignalCell<Option<str>>, Option<str>, bool> {
        self.state.derive(|value| value.is_none())
    }
}

fun mirror(value: Option<str>, live: Shared<i32>): Mirror {
    Mirror { state = Signal::new(value), live }
}
"#;

/// The paper's first rough edge (§6.1): `filter_map(|m| m)` over a collection of
/// `Option`-valued sources keeps the loaded payloads and follows each source, and
/// a removed element RELEASES its source — the subscription count drops with
/// it. (B434's shape, on `next`; the transient is the local stand-in above.)
#[test]
fn a142_s4_filter_map_over_transients_follows_and_releases() {
    assert_compiles_and_runs(
        &format!(
            "{MIRROR}\n{}",
            r#"
            fun main() {
                let live = Shared::new(0);
                let a = mirror(Some("a"), live);
                let b = mirror(None, live);
                let c = mirror(Some("c"), live);
                let mirrors: ListCell<Mirror> = ListCell::of([a, b, c]);
                let (loaded, scope) = comp(|| mirrors.filter_map(|m| m).memo());
                print(loaded.get());
                print(live.read());
                b.state.set(Some("b"));
                print(loaded.get());
                mirrors.remove_at(0);
                print(live.read());
                print(loaded.get());
                scope.dispose();
                print(live.read());
            }
            main();
            "#
        ),
        "[ 'a', 'c' ]\n3\n[ 'a', 'b', 'c' ]\n2\n[ 'b', 'c' ]\n0\n",
    );
}

/// `any` over `is_pending()` pipes (§6.1's `gns_is_loading`) is a counter: the
/// sealed answer flips EXACTLY ONCE per element change that flips it, and an
/// element change that does not flip it moves nothing out.
#[test]
fn a142_s4_any_over_pending_pipes_flips_once_per_element_change() {
    assert_compiles_and_runs(
        &format!(
            "{MIRROR}\n{}",
            r#"
            fun main() {
                let live = Shared::new(0);
                let a = mirror(Some("a"), live);
                let b = mirror(None, live);
                let mirrors: ListCell<Mirror> = ListCell::of([a, b]);
                let (pending, scope) = comp(|| mirrors.any(|m| m.is_pending()).memo());
                mut flips = 0;
                let watch = pending.on_change(|now| {
                    flips += 1;
                });
                print(pending.get());
                b.state.set(Some("b"));
                print(pending.get());
                print(flips);
                a.state.set(None);
                print(pending.get());
                print(flips);
                watch.dispose();
                scope.dispose();
            }
            main();
            "#
        ),
        "true\nfalse\n1\ntrue\n2\n",
    );
}

/// A `dyn` element — the paper's own spelling, `CollSource<dyn TransientSource<..>>`
/// — is held by B475: a `dyn Source`'s table has no slot for `Flow::start`, so
/// starting one per element fails at run time ("start is not a function"). The
/// concrete-element pins above are the shape until B475 lands.
#[test]
#[ignore = "B475: a dyn Source's table has no slot for Flow::start (reactive-44's find 3)"]
fn a142_s4_filter_map_over_source_objects() {
    assert_compiles_and_runs(
        r#"
        import std::option::Option::{ self, None, Some };
        import std::reactive::{ ListCell, Signal, SignalCell, Source, comp };

        fun main() {
            let a: SignalCell<Option<str>> = Signal::new(Some("a"));
            let b: SignalCell<Option<str>> = Signal::new(None);
            let first: dyn Source<Option<str>> = a;
            let second: dyn Source<Option<str>> = b;
            let sources: ListCell<dyn Source<Option<str>>> = ListCell::of([first, second]);
            let (loaded, scope) = comp(|| sources.filter_map(|source| source).memo());
            b.set(Some("b"));
            print(loaded.get());
            scope.dispose();
        }
        main();
        "#,
        "[ 'a', 'b' ]\n",
    );
}

/// R35: `map` STARTS a returned pipe per element and carries its value, and
/// never follows a returned `Source` — that is the element; `.flatten()` is the
/// explicit follow.
#[test]
fn a142_s4_map_starts_a_pipe_and_keeps_a_source() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ ListCell, Signal, SignalCell, SequenceCell, comp };

        fun main() {
            let cells: List<SignalCell<i32>> = [Signal::new(1), Signal::new(2), Signal::new(3)];
            let ids: ListCell<usize> = ListCell::of([0usize, 1usize]);
            let ((sources, values, doubled), scope) = comp(|| (
                ids.map(|id| cells[id]).memo(),
                ids.map(|id| cells[id]).flatten().memo(),
                ids.map(|id| cells[id].derive(|value| value * 2)).memo()
            ));
            print(sources.get().len());
            cells[0].set(10);
            print(values.get());
            print(doubled.get());
            ids.push(2usize);
            cells[2].set(30);
            print(values.get());
            print(doubled.get());
            print(sources.get()[2].get());
            scope.dispose();
        }
        main();
        "#,
        "2\n[ 10, 2 ]\n[ 20, 4 ]\n[ 10, 2, 30 ]\n[ 20, 4, 60 ]\n30\n",
    );
}

/// §6.4: every element run has an owner — what it registers is released when
/// the element LEAVES or RE-RUNS, and the rest with the consumer.
#[test]
fn a142_s4_an_element_run_is_released_when_it_leaves_or_reruns() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ ListCell, SequenceCell, comp, on_cleanup };

        fun main() {
            let numbers: ListCell<i32> = ListCell::of([1, 2, 3]);
            mut released: List<i32> = [];
            let (tagged, scope) = comp(|| numbers.map(|n| {
                on_cleanup(|| released.push(n));
                n * 100
            }).memo());
            numbers.remove_at(1);
            numbers.set_at(0, 7);
            print(released);
            print(tagged.get());
            scope.dispose();
            print(released);
        }
        main();
        "#,
        "[ 2, 1 ]\n[ 700, 300 ]\n[ 2, 1, 7, 3 ]\n",
    );
}

/// A collection pipe is move-only (R33 under R29): a second consumer is "use
/// after move", exactly as for a scalar pipe.
#[test]
fn a142_s4_a_collection_pipe_has_one_consumer() {
    assert_fails_with(
        r#"
        import std::reactive::ListCell;

        fun main() {
            let numbers: ListCell<i32> = ListCell::of([1, 2]);
            let evens = numbers.filter(|n| n % 2 == 0);
            let first = evens.sample();
            let again = evens.sample();
        }
        main();
        "#,
        "after it was moved",
    );
}

/// The operators start from a GRANULAR source; a coarse `Source<List<T>>`
/// converts at the boundary (R10), so it has no `filter` of its own.
#[test]
fn a142_s4_a_coarse_list_source_has_no_operators() {
    assert_fails_with(
        r#"
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let rows: SignalCell<List<i32>> = Signal::new([1, 2]);
            let big = rows.filter(|n| n > 1);
        }
        main();
        "#,
        "has no method 'filter'",
    );
}

/// `.sample()` reads a collection pipe once and releases what its runs built;
/// `.memo_global()` seals for the program (A130), outside every owner.
#[test]
fn a142_s4_sample_and_memo_global() {
    assert_compiles_and_runs(
        r#"
        import std::reactive::{ ListCell, SequenceCell, on_cleanup };

        let numbers: ListCell<i32> = ListCell::of([1, 2, 3]);
        let doubled = numbers.map(|n| n * 2).memo_global();

        fun main() {
            mut released = 0;
            print(numbers.map(|n| {
                on_cleanup(|| released += 1);
                n + 1
            }).sample());
            print(released);
            numbers.push(4);
            print(doubled.get());
        }
        main();
        "#,
        "[ 2, 3, 4 ]\n3\n[ 2, 4, 6, 8 ]\n",
    );
}

/// Positions under interleaved splices, and the counter folds: a seeded random
/// walk of every op the cell has (push, remove, set, insert-two, move, splice,
/// reset) interleaved with flips of the flows the closures answer, checked
/// against an oracle recomputed from scratch after EVERY step. A wrong Fenwick
/// position, a stale slot, a lost flip or a counter that missed what left shows
/// up as a count of failed steps.
#[test]
fn a142_s4_the_operators_agree_with_a_recomputation_under_a_random_walk() {
    assert_compiles_and_runs(COLLECTION_WALK, "failures 0\n");
}

pub const COLLECTION_WALK: &str = r#"
import std::option::Option::{ self, None, Some };
import std::reactive::{ ListCell, Signal, SignalCell, SequenceCell, comp };

mut seed = 4242;

fun next(bound: usize): usize {
    seed = (seed * 75 + 74) % 65537;
    (seed % bound.as_i32()).as_usize()
}

fun flag_for(flags: List<SignalCell<bool>>, x: i32): SignalCell<bool> {
    flags[x.as_usize() % 4]
}

fun main() {
    let flags: List<SignalCell<bool>> = [Signal::new(false), Signal::new(true), Signal::new(false), Signal::new(false)];
    let cell: ListCell<i32> = ListCell::of([1, 2, 3, 4, 5, 6]);
    let (outs, scope) = comp(|| {
        let plain = cell.filter(|x| x % 2 == 0).memo();
        let followed = cell.filter(|x| flag_for(flags, x).derive(|on| on || x % 3 == 0)).memo();
        let mapped = cell.map(|x| x * 10).memo();
        let picked = cell.filter_map(|x| if x > 4 { Some(x + 100) } else { None }).memo();
        let counted = cell.count(|x| flag_for(flags, x)).memo();
        let everything = cell.map(|x| x + 1).filter(|x| x % 2 == 0).all(|x| x > 1).memo();
        let anything = cell.any(|x| flag_for(flags, x)).memo();
        (plain, followed, mapped, picked, counted, everything, anything)
    });
    let (plain, followed, mapped, picked, counted, everything, anything) = outs;
    mut failures = 0;
    mut step = 0;
    for step < 600 {
        let size = cell.size();
        let choice = next(8);
        if choice == 0 {
            cell.push(next(50).as_i32());
        } else if choice == 1 && size > 0 {
            cell.remove_at(next(size));
        } else if choice == 2 && size > 0 {
            cell.set_at(next(size), next(50).as_i32());
        } else if choice == 3 {
            cell.insert_all(next(size + 1), [next(50).as_i32(), next(50).as_i32()]);
        } else if choice == 4 && size > 2 {
            cell.move_range(0, 2, next(size - 1));
        } else if choice == 5 {
            let flag = flags[next(4)];
            flag.set(!flag.get());
        } else if choice == 6 && size > 3 {
            cell.splice(next(size - 2), 2, [next(50).as_i32()]);
        } else if choice == 7 && next(10) == 0 {
            cell.set([3, 9, 12, 1]);
        }
        let xs = cell.get();
        mut evens: List<i32> = [];
        mut kept: List<i32> = [];
        mut tens: List<i32> = [];
        mut big: List<i32> = [];
        mut flagged: usize = 0;
        mut all_positive = true;
        for x in xs {
            if x % 2 == 0 { evens.push(x); }
            if x % 3 == 0 || flags[x.as_usize() % 4].get() { kept.push(x); }
            tens.push(x * 10);
            if x > 4 { big.push(x + 100); }
            if flags[x.as_usize() % 4].get() { flagged += 1; }
            if (x + 1) % 2 == 0 && x + 1 <= 1 { all_positive = false; }
        }
        if plain.get() != evens { failures += 1; }
        if followed.get() != kept { failures += 1; }
        if mapped.get() != tens { failures += 1; }
        if picked.get() != big { failures += 1; }
        if counted.get() != flagged { failures += 1; }
        if everything.get() != all_positive { failures += 1; }
        if anything.get() != (flagged > 0) { failures += 1; }
        step += 1;
    }
    print(i"failures {failures}");
    scope.dispose();
}
main();
"#;
