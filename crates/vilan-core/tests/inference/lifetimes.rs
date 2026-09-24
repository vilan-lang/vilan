//! The reactive graph's lifetimes (proposal/lifetimes.md §5): which std APIs
//! hand their subscription to the ambient owner, and which teardown paths let
//! the thing they were attached to forget them.
//!
//! §5 named five back edges, each measured by SCC analysis of a live heap
//! snapshot. The two that are LEAKS rather than incidental cycles are pinned
//! here — A28 (`map`/`combine`/`flatten` could never be detached) and A29
//! (`DuplexEnd.me` was never cleared) — and both are pinned STRUCTURALLY, by
//! reading the count or the slot the leak sat in, because a value assertion
//! cannot tell a live derivation from a dead one that still fires.
//!
//! The cycles themselves (V1, V3, V5) are held by the heap-snapshot walk in
//! `crates/vilan-cli/tests/reactive_lifetimes.rs`, which is the only instrument
//! that can see them.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- A28: the derivation combinators register with the ambient owner ---------
//
// `map`/`combine`/`flatten` pushed a `Subscriber` and handed back only the
// derived signal — no id, no `Subscription`, nothing that could ever detach
// them (proposal/lifetimes.md §5, V2). Measured on the documented router idiom,
// that leaked 256 objects PERMANENTLY per mount/dispose round plus a time leak:
// every write notified every dead derivation ever made. Each pin below reads
// the SOURCE's subscriber count after disposal, because that is where the dead
// subscriber sat; the value assertions around it hold the behavior unchanged.
//
// Unlike `effect`, these read the owner SAFELY: a derivation built outside every
// boundary is a documented idiom (a module-level `current_path().map(parse)`,
// `RemoteSource::status` above every boundary), so it must keep compiling —
// `a_derivation_outside_every_owner_still_tracks_its_source` pins that.

#[test]
fn map_registers_its_subscription_into_the_ambient_owner() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope };

        fun main() {
            let count = Signal::new(1);
            let owner = Owner::new();
            owner_scope.run(owner, || {
                let doubled = count.map(|n| n * 2);
                doubled.effect(|value| print(value));
            });
            count.set(2);
            owner.dispose();
            count.set(3);
            print(count.subscribers.read().len());
        }

        main();
        "#,
        "2\n4\n0\n",
    );
}

#[test]
fn combine_registers_every_input_subscription_into_the_ambient_owner() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, combine, owner_scope };

        fun main() {
            let left = Signal::new(1);
            let right = Signal::new(2);
            let owner = Owner::new();
            owner_scope.run(owner, || {
                let both = combine((left, right));
                both.effect(|pair| print(pair.0 + pair.1));
            });
            left.set(10);
            owner.dispose();
            left.set(100);
            print(left.subscribers.read().len());
            print(right.subscribers.read().len());
        }

        main();
        "#,
        "3\n12\n0\n0\n",
    );
}

// `flatten` owes TWO handles: the outer subscription, and whichever inner one
// happens to be live at disposal (the rolling one it disposes on every switch
// has no other owner).
#[test]
fn flatten_registers_its_outer_and_live_inner_subscriptions() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope };

        fun main() {
            let first = Signal::new(1);
            let second = Signal::new(10);
            let outer = Signal::new(first);
            let owner = Owner::new();
            owner_scope.run(owner, || {
                let joined = outer.flatten();
                joined.effect(|value| print(value));
            });
            outer.set(second);
            owner.dispose();
            print(outer.subscribers.read().len());
            print(first.subscribers.read().len());
            print(second.subscribers.read().len());
        }

        main();
        "#,
        "1\n10\n0\n0\n0\n",
    );
}

// --- A86: `flatten` is a BLANKET over `Source`, not a member of the cell -----
//
// The read contract is the trait, so an outer that is a `map` result or a
// derived cell holds an inner signal exactly as a `SignalCell` does. The pin
// above (`flatten_registers_its_outer_and_live_inner_subscriptions`) is the
// control for the cell receiver and for the ownership story; these are the
// receivers the inherent impl could not reach.

#[test]
fn flatten_joins_a_derived_outer_not_only_a_cell() {
    // The outer is a `map` RESULT — a `SignalCell` the user never spelled, and
    // the shape `outer.map(..).flatten()` has wanted since A4. Switching the
    // outer detaches the replaced inner, exactly as it does for a cell.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let first = Signal::new(1);
            let second = Signal::new(10);
            let which = Signal::new(true);
            let picked = which.map(|flag| if flag { first } else { second });
            let joined = picked.flatten();
            print(joined.get());
            which.set(false);
            print(joined.get());
            second.set(11);
            print(joined.get());
            // The replaced inner no longer drives the result.
            first.set(99);
            print(joined.get());
        }

        main();
        "#,
        "1\n10\n11\n11\n",
    );
}

#[test]
fn flatten_over_an_optional_inner_follows_some_and_detaches_on_none() {
    // The `Option` form: an outer of `Option<inner source>` — a lazily-created
    // signal. `None` is `None` and DETACHES (the last line proves it: a set on
    // the dropped inner does not reach the result), `Some(inner)` follows that
    // inner from its current value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let inner = Signal::new(1);
            let outer: SignalCell<Option<SignalCell<i32>>> = Signal::new(None);
            let joined = outer.flatten();
            print(joined.get().unwrap_or(0));
            outer.set(Some(inner));
            print(joined.get().unwrap_or(0));
            inner.set(5);
            print(joined.get().unwrap_or(0));
            outer.set(None);
            print(joined.get().unwrap_or(0));
            inner.set(9);
            print(joined.get().unwrap_or(0));
        }

        main();
        "#,
        "0\n1\n5\n0\n0\n",
    );
}

#[test]
fn the_optional_flatten_registers_its_subscriptions_with_the_ambient_owner() {
    // A28's story, unchanged by the second blanket: the outer subscription is
    // registered and whichever inner is live at disposal is deferred, so a
    // disposed boundary leaves no subscriber behind on either signal.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope };

        fun main() {
            let inner = Signal::new(1);
            let outer: SignalCell<Option<SignalCell<i32>>> = Signal::new(Some(inner));
            let owner = Owner::new();
            owner_scope.run(owner, || {
                let joined = outer.flatten();
                joined.effect(|value| print(value.unwrap_or(0)));
            });
            inner.set(5);
            owner.dispose();
            print(outer.subscribers.read().len());
            print(inner.subscribers.read().len());
        }

        main();
        "#,
        "1\n5\n0\n0\n",
    );
}

#[test]
fn flatten_still_joins_a_plain_cell_of_cells() {
    // The control the blanket has to subsume: the receiver the retired
    // inherent `impl SignalCell<SignalCell<type U>>` served, un-annotated, with
    // derived work stacked on the result — `vilan/test/reactive-flatten.vl`'s
    // shape in one pin.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let first = Signal::new(1);
            let second = Signal::new(10);
            let outer = Signal::new(first);
            let joined = outer.flatten();
            let doubled = joined.map(|value| value * 2);
            print(joined.get());
            outer.set(second);
            print(joined.get());
            second.set(21);
            print(doubled.get());
        }

        main();
        "#,
        "1\n10\n42\n",
    );
}

// The ownerless case is leak-as-today, NOT a refusal: a derivation made where no
// `owner_scope.run` encloses still compiles and still tracks its source, which
// is what a module-level `current_path().map(parse)` needs. Making this an
// error is the stronger law and a breaking change — the owner's call, not std's.
#[test]
fn a_derivation_outside_every_owner_still_tracks_its_source() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        let count: SignalCell<i32> = Signal::new(1);
        let doubled: SignalCell<i32> = count.map(|n| n * 2);

        fun main() {
            print(doubled.get());
            count.set(5);
            print(doubled.get());
            print(count.subscribers.read().len());
        }

        main();
        "#,
        "2\n10\n1\n",
    );
}

// --- A123: the two DYNAMIC-DEPENDENCY combinators over `Source` --------------
//
// `map`/`combine` are static dependencies; `switch` and `and_then` are the
// dynamic pair — WHICH source the result follows is decided by the current
// value. Both are blankets written directly over `on_change` rather than as
// `self.map(select).flatten()`: one derived cell instead of two, and the
// selector called exactly once per value of the source. The `flatten` pins
// above are their control — the ownership and detach stories are the same
// ones, reached through a selector instead of through a held inner.

#[test]
fn switch_follows_the_source_its_selector_answers_and_detaches_from_the_last() {
    // The four claims in one run: the initial follow, an inner update reaching
    // the result, a switch of inner, and the ABANDONED inner no longer driving
    // it (line four) — then the new inner still does (line five).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let which = Signal::new(0);
            let first = Signal::new(10);
            let second = Signal::new(20);
            let picked = which.switch(|n| if n == 0 { first } else { second });
            print(picked.get());
            first.set(11);
            print(picked.get());
            which.set(1);
            print(picked.get());
            first.set(99);
            print(picked.get());
            second.set(21);
            print(picked.get());
        }

        main();
        "#,
        "10\n11\n20\n20\n21\n",
    );
}

#[test]
fn switch_calls_its_selector_once_per_value_of_the_source() {
    // The reason this is not `self.map(select).flatten()`: that form evaluates
    // the selector a SECOND time for the value the derived cell was built from
    // and orphans the node it answered. One call per value, the initial one
    // included — three sets, four calls.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let which = Signal::new(0);
            let first = Signal::new(10);
            let second = Signal::new(20);
            let calls: SignalCell<i32> = Signal::new(0);
            let picked = which.switch(|n| {
                calls.set(calls.get() + 1);
                if n == 0 { first } else { second }
            });
            print(calls.get());
            which.set(1);
            which.set(0);
            which.set(1);
            print(calls.get());
            print(picked.get());
        }

        main();
        "#,
        "1\n4\n20\n",
    );
}

#[test]
fn switch_registers_its_outer_and_live_inner_subscriptions() {
    // A28's story through a selector: the outer subscription is registered and
    // whichever inner the selector last answered is deferred, so a disposed
    // boundary leaves no subscriber behind on the source or on either inner.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope };

        fun main() {
            let which = Signal::new(0);
            let first = Signal::new(10);
            let second = Signal::new(20);
            let owner = Owner::new();
            owner_scope.run(owner, || {
                let picked = which.switch(|n| if n == 0 { first } else { second });
                picked.effect(|value| print(value));
            });
            which.set(1);
            owner.dispose();
            print(which.subscribers.read().len());
            print(first.subscribers.read().len());
            print(second.subscribers.read().len());
        }

        main();
        "#,
        "10\n20\n0\n0\n0\n",
    );
}

#[test]
fn and_then_follows_the_selected_source_and_an_outer_none_detaches() {
    // The Kleisli composition of `Source<Option<T>>`, which is what a model
    // layer of `SignalCell<Option<T>>` cells composes with: the outer `None`
    // (nothing selected) and the inner `None` (the followed source has nothing
    // yet) collapse into one `None`. Line four is the detach — the abandoned
    // inner's later value does not reach the result — and line six re-follows.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let outer: SignalCell<Option<i32>> = Signal::new(Some(1));
            let one: SignalCell<Option<i32>> = Signal::new(Some(10));
            let two: SignalCell<Option<i32>> = Signal::new(None);
            let followed = outer.and_then(|id| if id == 1 { one } else { two });
            print(followed.get().unwrap_or(0));
            outer.set(Some(2));
            print(followed.get().unwrap_or(0));
            two.set(Some(20));
            print(followed.get().unwrap_or(0));
            outer.set(None);
            print(followed.get().unwrap_or(0));
            two.set(Some(99));
            print(followed.get().unwrap_or(0));
            outer.set(Some(1));
            print(followed.get().unwrap_or(0));
        }

        main();
        "#,
        "10\n0\n20\n0\n0\n10\n",
    );
}

#[test]
fn and_then_registers_its_subscriptions_with_the_ambient_owner() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell, Owner, Disposable, owner_scope };

        fun main() {
            let outer: SignalCell<Option<i32>> = Signal::new(Some(1));
            let one: SignalCell<Option<i32>> = Signal::new(Some(10));
            let owner = Owner::new();
            owner_scope.run(owner, || {
                let followed = outer.and_then(|id| one);
                followed.effect(|value| print(value.unwrap_or(0)));
            });
            owner.dispose();
            print(outer.subscribers.read().len());
            print(one.subscribers.read().len());
        }

        main();
        "#,
        "10\n0\n0\n",
    );
}

#[test]
fn a_switch_is_a_derivation_so_an_effect_reads_its_chain_settled() {
    // A110 door 2, as a diamond: an effect standing on the ROOT reads a value
    // two hops away through the switch. Both of `switch`'s attaches are marked
    // `as_derivation`, so phase 1 pulls the chain to a fixpoint and the effect
    // reads 21. Drop either mark and the switch's update becomes effect-class:
    // it runs in the same wave as the watcher, the `map` below it is enqueued
    // for the NEXT wave, and the effect reads the stale `11`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{
            FlushPolicy, Owner, Signal, SignalCell, Source, run_with_owner, turn,
        };

        fun main() {
            let which: SignalCell<i32> = Signal::new(0);
            let first: SignalCell<i32> = Signal::new(10);
            let second: SignalCell<i32> = Signal::new(20);
            let picked: SignalCell<i32> = which.switch(|n| if n == 0 { first } else { second });
            let plus: SignalCell<i32> = picked.map(|value| value + 1);
            let seen: SignalCell<str> = Signal::new("");
            let watcher = Owner::new();
            run_with_owner(watcher, || {
                which.effect_on_change(|value: i32| {
                    seen.set_with(|log| i"{log}{value}/{plus.get()},");
                });
            });
            turn(FlushPolicy::AtEnd, || {
                which.set(1);
            });
            print(seen.get());
        }

        main();
        "#,
        "1/21,\n",
    );
}

#[test]
fn an_and_then_is_a_derivation_so_an_effect_reads_its_chain_settled() {
    // The same claim for the `Option` half, same shape, same red when the
    // marks come off.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{
            FlushPolicy, Owner, Signal, SignalCell, Source, run_with_owner, turn,
        };

        fun main() {
            let outer: SignalCell<Option<i32>> = Signal::new(Some(1));
            let one: SignalCell<Option<i32>> = Signal::new(Some(10));
            let two: SignalCell<Option<i32>> = Signal::new(Some(20));
            let followed: SignalCell<Option<i32>> = outer.and_then(|id| if id == 1 { one } else { two });
            let plus: SignalCell<i32> = followed.map(|value| value.unwrap_or(0) + 1);
            let seen: SignalCell<str> = Signal::new("");
            let watcher = Owner::new();
            run_with_owner(watcher, || {
                outer.effect_on_change(|value: Option<i32>| {
                    seen.set_with(|log| i"{log}{value.unwrap_or(0)}/{plus.get()},");
                });
            });
            turn(FlushPolicy::AtEnd, || {
                outer.set(Some(2));
            });
            print(seen.get());
        }

        main();
        "#,
        "2/21,\n",
    );
}

#[test]
fn switch_and_and_then_mint_two_subscribers_where_the_composed_form_mints_three() {
    // The "one derived cell" claim, counted. `fresh_id` is the program's
    // subscriber counter, so the delta across a construction is the number of
    // subscribers it minted — two `fresh_id()` calls of its own included, hence
    // the `- 1`.
    //
    // The pin carries its OWN control: the third line builds the same dynamic
    // dependency the composed way (`map(select).flatten()`, spelled at a
    // concrete type because the generic body hits B371) in the same program.
    // Two subscribers and one cell against three subscribers and two cells is
    // the whole reason A123 is written over `on_change`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell, fresh_id };

        fun main() {
            let which = Signal::new(0);
            let first = Signal::new(10);
            let second = Signal::new(20);

            let before_switch = fresh_id();
            let picked = which.switch(|n| if n == 0 { first } else { second });
            print(fresh_id() - before_switch - 1);

            let outer: SignalCell<Option<i32>> = Signal::new(Some(1));
            let inner: SignalCell<Option<i32>> = Signal::new(Some(5));
            let before_and_then = fresh_id();
            let followed = outer.and_then(|id| inner);
            print(fresh_id() - before_and_then - 1);

            let before_composed = fresh_id();
            let composed: SignalCell<i32> =
                which.map(|n| if n == 0 { first } else { second }).flatten();
            print(fresh_id() - before_composed - 1);

            // Every one of the three is live, so none of the counts is the
            // count of a chain that failed to attach.
            which.set(1);
            print(picked.get());
            print(composed.get());
            print(followed.get().unwrap_or(0));
        }

        main();
        "#,
        "2\n2\n3\n20\n20\n5\n",
    );
}

// --- A124 S1: the push-pull pipeline as EVIDENCE ----------------------------
//
// The paper's S1 probe (`proposal/reactive-pipeline.md` §7) measured the
// cold-node model over today's `Source` in a module of its own. Its nodes are
// `std::reactive`'s since S2b and the probe file is gone; these four pins are
// its numbers, now held against the real nodes (built through the `[internal]`
// `_node` spellings until the flip, S2c).

#[test]
fn a124_s1_a_cold_chain_with_no_subscriber_evaluates_nothing() {
    // Claim 1: a node allocates no cell and registers nothing, so three writes
    // to the root of a five-deep chain do no work at all. Today's five `map`
    // cells would have evaluated fifteen times. One `get()` then pulls the
    // whole chain — five evaluations, paid by the reader.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let evals: Shared<i32> = Shared::new(0);
            let chain = root
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 });
            print(evals.read());
            root.set(2);
            root.set(3);
            root.set(4);
            print(evals.read());
            print(chain.get());
            print(evals.read());
        }

        main();
        "#,
        "0\n0\n9\n5\n",
    );
}

#[test]
fn a124_s1_a_cold_chain_evaluates_once_per_leaf_subscriber() {
    // Claim 2: N leaves means N evaluations, by design — the cold contract.
    // One leaf: five. Two leaves on the same chain: ten. The notification
    // carries no payload, so the hops themselves compute nothing; the count is
    // exactly the leaves' pulls.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let evals: Shared<i32> = Shared::new(0);
            let chain = root
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 })
                .map_node(|x| { evals.write() = evals.read() + 1; x + 1 });
            let _one = chain.on_change(|_value| {});
            evals.write() = 0;
            root.set(2);
            print(evals.read());
            let _two = chain.on_change(|_value| {});
            evals.write() = 0;
            root.set(3);
            print(evals.read());
        }

        main();
        "#,
        "5\n10\n",
    );
}

#[test]
fn a124_s1_a_cell_between_runs_the_segment_above_it_once() {
    // Claim 3: `.cell()` is one more node, composable anywhere, and it is where
    // sharing is bought. Two leaves below a cell: the two nodes ABOVE it run
    // once (2), the three below it run per leaf (3 x 2 = 6).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let upper: Shared<i32> = Shared::new(0);
            let lower: Shared<i32> = Shared::new(0);
            let cached = root
                .map_node(|x| { upper.write() = upper.read() + 1; x + 1 })
                .map_node(|x| { upper.write() = upper.read() + 1; x + 1 })
                .cell();
            let below = cached
                .map_node(|x| { lower.write() = lower.read() + 1; x + 1 })
                .map_node(|x| { lower.write() = lower.read() + 1; x + 1 })
                .map_node(|x| { lower.write() = lower.read() + 1; x + 1 });
            let _a = below.on_change(|_value| {});
            let _b = below.on_change(|_value| {});
            upper.write() = 0;
            lower.write() = 0;
            root.set(2);
            print(upper.read());
            print(lower.read());
        }

        main();
        "#,
        "2\n6\n",
    );
}

#[test]
fn a124_s1_a_diamond_pulls_a_settled_pair_and_fires_twice() {
    // Claim 4, and the honest half of R2. Two arms over one root, joined: with
    // NO turn the leaf is told twice per settle. Since S2a both arms carry the
    // SAME record (the leaf's id), but an inline notification walks the root's
    // list and calls what it finds — there is no queue to dedup in, which is
    // what "inline, eager, depth-first" has always meant. It is a duplicate
    // CALL and never a torn pair: both calls pull, so both read the settled
    // `(20, 102)`. The turn is where the one id pays — the S2a pin below.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, combine_node };
        import std::shared::Shared;

        fun main() {
            let source: SignalCell<i32> = Signal::new(1);
            let seen: Shared<str> = Shared::new("");
            let arms: (dyn Source<i32>, dyn Source<i32>) = (
                source.map_node(|x| x * 10),
                source.map_node(|x| x + 100)
            );
            let diamond = combine_node(arms);
            let _leaf = diamond.on_change(|pair| {
                let (left, right) = pair;
                seen.write() = i"{seen.read()}({left},{right})";
            });
            seen.write() = "";
            source.set(2);
            print(seen.read());
        }

        main();
        "#,
        "(20,102)(20,102)\n",
    );
}

// --- A124 S2a: the no-payload protocol on the read trait ---------------------
//
// `Source::on_settle` is the protocol now (the S1 probe carried it as a twin
// trait), and a leaf's ONE subscriber id is threaded down its whole chain: the
// nodes forward the record they are handed, `SignalCell::on_settle` pushes it as
// given, and a turn's dedup — keyed on that id — collapses a diamond's
// duplicate.

#[test]
fn a124_s2a_a_diamond_in_a_turn_fires_once_with_the_settled_pair() {
    // The COUNT claim S1 could not make. Red when `SignalCell::on_settle`
    // mints a fresh id per registration instead of pushing the leaf's record:
    // `(30,103)(30,103)`. The leaf is the node's own `on_change`, called
    // DIRECTLY: through a generic bound a `Combine`'s override is not selected
    // — its trait argument is a tuple — and the call takes the default
    // bridge, whose own dedup would hide the root's.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ FlushPolicy, Signal, SignalCell, Source, combine_node, turn };
        import std::shared::Shared;

        fun main() {
            let source: SignalCell<i32> = Signal::new(1);
            let seen: Shared<str> = Shared::new("");
            let arms: (dyn Source<i32>, dyn Source<i32>) = (
                source.map_node(|x| x * 10),
                source.map_node(|x| x + 100)
            );
            let diamond = combine_node(arms);
            let _leaf = diamond.on_change(|pair| {
                let (left, right) = pair;
                seen.write() = i"{seen.read()}({left},{right})";
            });
            turn(FlushPolicy::AtEnd, || {
                source.set(3);
            });
            print(seen.read());
        }

        main();
        "#,
        "(30,103)\n",
    );
}

#[test]
fn a124_s2a_a_leaf_disposed_by_an_earlier_effect_in_its_wave_does_not_fire() {
    // Door 1 through the protocol. Both observers are queued in one wave; the
    // first (lower id) disposes the cold chain's leaf, whose record is already
    // OUT of the queue — only its liveness cell can stop it now, and that cell
    // is the one `attach` shares with the handle. Red when the root's handle
    // carries a liveness cell of its own: `leaf saw 20`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{
            FlushPolicy, Signal, SignalCell, Source, Subscription, turn,
        };
        import std::shared::Shared;

        fun main() {
            let source: SignalCell<i32> = Signal::new(1);
            let held: Shared<Option<Subscription>> = Shared::new(None);
            let _first = source.on_change(|_value| {
                held.read()?.dispose();
                print("first disposed the leaf");
            });
            held.write() = Some(source.map_node(|x| x * 10).on_change(|value| {
                print(i"leaf saw {value}");
            }));
            turn(FlushPolicy::AtEnd, || {
                source.set(2);
            });
            print("done");
        }

        main();
        "#,
        "first disposed the leaf\ndone\n",
    );
}

#[test]
fn a124_s2a_the_default_bridge_dedups_a_diamond_over_a_root_that_only_has_on_change() {
    // A root that is not a `SignalCell` — `get` and `on_change` and nothing
    // else, the `Stored` of the reference page — takes `Source::on_settle`'s
    // DEFAULT, the payload bridge. Each arm's forward reaches the root
    // separately and mints a bridge of its own, and each bridge WAKES the leaf
    // through the turn rather than calling it, so the leaf's id is still the
    // one the dedup sees. Red when the bridge calls the leaf directly:
    // `(30,103)(30,103)`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{
            FlushPolicy, Signal, SignalCell, Source, Subscription, combine_node, turn,
        };
        import std::shared::Shared;

        struct Stored<T> {
            inner: SignalCell<T>,
        }

        impl Stored<type T> with Source<T> {
            fun get(self): T {
                self.inner.get()
            }

            fun on_change(self, observer: |T| void): Subscription {
                self.inner.on_change(observer)
            }
        }

        fun main() {
            let cell: SignalCell<i32> = Signal::new(1);
            let root = Stored { inner = cell };
            let seen: Shared<str> = Shared::new("");
            let arms: (dyn Source<i32>, dyn Source<i32>) = (
                root.map_node(|x| x * 10),
                root.map_node(|x| x + 100)
            );
            let diamond = combine_node(arms);
            let _leaf = diamond.on_change(|pair| {
                let (left, right) = pair;
                seen.write() = i"{seen.read()}({left},{right})";
            });
            turn(FlushPolicy::AtEnd, || {
                cell.set(3);
            });
            print(seen.read());
            seen.write() = "";
            cell.set(4);
            print(seen.read());
        }

        main();
        "#,
        "(30,103)\n(40,104)(40,104)\n",
    );
}

// --- A124 S2b: the nodes, `.cell()`, `.distinct()` and `Resource<T>` ---------
//
// The node types are `std::reactive`'s; `map`/`switch`/`combine` still return
// cells until the flip (S2c), so the nodes are built through the `[internal]`
// `map_node`/`switch_node`/`combine_node` spellings, and `.cell()` and
// `.distinct()` are public.

#[test]
fn a124_s2b_a_cell_does_not_compare_and_a_distinct_does() {
    // Q3. Three settles of the root, the second and third to values the
    // derivation maps to the SAME answer: below `.cell()` the leaf fires every
    // time (a `set` never compares); below `.distinct()` only for the change.
    // Red when `Distinct`'s relay wakes without comparing: `distinct=0,1,1,`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let cell_log: Shared<str> = Shared::new("");
            let distinct_log: Shared<str> = Shared::new("");
            let cached = root.map_node(|x| x / 10).cell();
            let _c = cached.on_change(|value| {
                cell_log.write() = i"{cell_log.read()}{value},";
            });
            let _d = root.map_node(|x| x / 10).distinct().on_change(|value| {
                distinct_log.write() = i"{distinct_log.read()}{value},";
            });
            root.set(2);
            root.set(15);
            root.set(19);
            print(i"cell={cell_log.read()}");
            print(i"distinct={distinct_log.read()}");
        }

        main();
        "#,
        "cell=0,1,1,\ndistinct=1,\n",
    );
}

#[test]
fn a124_s2b_a_cell_is_a_derivation_an_effect_reads_settled() {
    // `.cell()` is marked `as_derivation()` (A110 door 2): an effect standing
    // on the root, lower in id than the cell, reads the cell SETTLED in the
    // same wave. Red with the mark removed: `seen 5/3`, the cell's value from
    // before the write.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ FlushPolicy, Signal, SignalCell, Source, turn };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let seen: Shared<str> = Shared::new("");
            let holder: Shared<Option<SignalCell<i32>>> = Shared::new(None);
            let _effect = root.on_change(|value| {
                match holder.read() {
                    Some(let cached) => {
                        seen.write() = i"{seen.read()}{value}/{cached.get()}";
                    },
                    None => {},
                }
            });
            holder.write() = Some(root.map_node(|x| x * 2 + 1).cell());
            turn(FlushPolicy::AtEnd, || {
                root.set(5);
            });
            print(i"seen {seen.read()}");
        }

        main();
        "#,
        "seen 5/11\n",
    );
}

#[test]
fn a124_s2b_a_cell_made_under_an_owner_releases_its_upstream_with_it() {
    // Owner-tied like every derivation (A28): the cell's registration on the
    // root is the ambient owner's, so disposing the owner leaves the root with
    // no subscriber and the cell frozen at its last value. Red when `.cell()`
    // does not register with the owner: `after=1 cell=30`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            let cached = run_with_owner(owner, || root.map_node(|x| x * 10).cell());
            root.set(2);
            print(i"before={root.subscribers.read().len()} cell={cached.get()}");
            owner.dispose();
            root.set(3);
            print(i"after={root.subscribers.read().len()} cell={cached.get()}");
        }

        main();
        "#,
        "before=1 cell=20\nafter=0 cell=20\n",
    );
}

#[test]
fn a124_s2b_the_owners_disposal_releases_a_switchs_rolling_inner_registration() {
    // A28's story for the cold `Switch`: an effect on the node, made under an
    // owner, follows the current inner and re-follows at every switch without
    // retiring itself (the rolling registration is the node's own relay, not
    // the effect's record). Disposing the owner leaves NO subscriber on the
    // outer or on either inner. Red when the handle forgets the live inner:
    // `after 0/1/0`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };
        import std::shared::Shared;

        fun main() {
            let which: SignalCell<i32> = Signal::new(0);
            let first: SignalCell<i32> = Signal::new(10);
            let second: SignalCell<i32> = Signal::new(20);
            let log: Shared<str> = Shared::new("");
            let owner = Owner::new();
            run_with_owner(owner, || {
                which
                    .switch_node(|n| if n == 0 { first } else { second })
                    .effect_on_change(|value| {
                        log.write() = i"{log.read()}{value},";
                    });
            });
            first.set(11);
            which.set(1);
            first.set(99);
            second.set(21);
            which.set(0);
            first.set(100);
            print(log.read());
            let counts = || i"{which.subscribers.read().len()}/{first.subscribers.read().len()}/{second.subscribers.read().len()}";
            print(i"before {counts()}");
            owner.dispose();
            first.set(1);
            second.set(2);
            which.set(1);
            print(i"after {counts()}");
            print(log.read());
        }

        main();
        "#,
        "11,20,21,99,100,\nbefore 1/1/0\nafter 0/0/0\n11,20,21,99,100,\n",
    );
}

#[test]
fn a124_s2b_a_default_inherited_by_a_node_types_its_observer_from_the_node() {
    // `sub`, `effect` and `map` are `Source` DEFAULTS, reached on a node through
    // its impl. Each node's value type is a DIRECT binder of its impl subject
    // (`Switch<.., type I: Source<U>, type U>`, `Distinct<.., type T>`), so the
    // observer's parameter is the node's `i32` and `value + 1` checks. Red with
    // `Switch` binding `U` out of `I`'s bound (`type I: Source<type U>`): `+` on
    // an unbounded `T`. And each node's upstream is bound on `Upstream`, not on
    // `Source`: bound on `Source`, the default on a type-changing `map_node` is
    // refused as "`SignalCell<i32>` does not implement `Source<str>`".
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        fun main() {
            let which: SignalCell<i32> = Signal::new(0);
            let first: SignalCell<i32> = Signal::new(10);
            let _switched = which.switch_node(|n| first).sub(|value| print(value + 1));
            let _distinct = which.distinct().sub(|value| print(value + 2));
            let labelled = which.map_node(|n| i"n{n}").map(|label| label.len());
            print(labelled.get());
        }

        main();
        "#,
        "11\n2\n2\n",
    );
}

#[test]
fn a124_s2b_combine_over_the_tuple_bound_joins_three_kinds_of_source() {
    // The n-ary `Combine` over `(U in T: dyn Source<U>)`: a root cell, a cold
    // node and a `.cell()`, three element types, one node — read by pull and
    // notified once per settle in a turn however many of its inputs stand on
    // the written root.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ FlushPolicy, Signal, SignalCell, Source, combine_node, turn };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let arms: (dyn Source<i32>, dyn Source<str>, dyn Source<bool>) = (
                count,
                count.map_node(|n| i"n{n}"),
                count.map_node(|n| n > 2).cell()
            );
            let all = combine_node(arms);
            let _leaf = all.on_change(|triple| {
                let (n, label, big) = triple;
                print(i"{n} {label} {big}");
            });
            turn(FlushPolicy::AtEnd, || {
                count.set(3);
            });
            let (n, label, big) = all.get();
            print(i"get {n} {label} {big}");
        }

        main();
        "#,
        "3 n3 true\nget 3 n3 true\n",
    );
}

#[test]
fn a124_s2b_pending_or_zero_reads_zero_before_completion_and_the_value_after() {
    // The item's pin, with a REAL load: a resource over `id` loads on a timer,
    // `.or(0)` reads 0 while it is out, and the `map` below the fallback is
    // written on `i32` — no `Option` anywhere downstream. A change of `id`
    // re-pends it and `.or` RESETS to the default until the new load lands
    // (Q2). Red when `.or` holds the last settled value instead: the re-pend
    // is invisible, `shown 11` repeats and `shown 1` never comes.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Resource, Signal, SignalCell, Source };
        import std::result::Result::{ self, Ok };
        import std::time::sleep;

        fun main() {
            let id: SignalCell<i32> = Signal::new(1);
            let user: Resource<i32> = id.resource_node(|n| {
                sleep(5);
                Ok(n * 10)
            });
            let shown = user.or(0).map(|n| n + 1);
            print(i"before {shown.get()}");
            let _log = shown.on_change(|value| {
                print(i"shown {value}");
                if value == 11 {
                    id.set(2);
                }
            });
        }

        main();
        "#,
        "before 1\nshown 11\nshown 1\nshown 21\n",
    );
}

#[test]
fn a124_s2b_a_resource_state_machine_and_its_four_fallbacks() {
    // R5's machine by hand — pending, settled, pending again, failed — read
    // through all four fallbacks. `.or` resets on every re-pend and on the
    // failure; `.latest` holds the last settled value across both and shows
    // its default only before the first settle; `.optional` is `None` except
    // while settled; `.is_pending` is true only while pending. Red when
    // `.latest` resets like `.or`: `latest=-1` on the third and fourth lines.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Resource, Source };

        fun line(r: Resource<i32>): str {
            let optional = match r.optional().get() {
                Some(let value) => i"{value}",
                None => "none",
            };
            i"or={r.or(0).get()} latest={r.latest(-1).get()} optional={optional} pending={r.is_pending().get()}"
        }

        fun main() {
            let r: Resource<i32> = Resource::pending();
            print(line(r));
            r.settle(41);
            print(line(r));
            r.pend();
            print(line(r));
            r.fail("offline");
            print(line(r));
        }

        main();
        "#,
        "or=0 latest=-1 optional=none pending=true\nor=41 latest=41 optional=41 pending=false\nor=0 latest=41 optional=none pending=true\nor=0 latest=41 optional=none pending=false\n",
    );
}

// --- A29: a disposed session lets its transport forget it --------------------

// `ReactiveClient::new`/`ReactiveServer::new` install an inbound handler that
// captures the whole client/server, and nothing ever cleared it: `dispose`
// emptied `sources`/`live` and left the wire holding the closure, so a closed
// connection's 70-node component stayed reachable from its transport — a leak
// per disconnect under any counted backend. The pin is structural on purpose:
// the observable behavior of a disposed session is "nothing happens" either
// way, and only the slot says whether the wire still reaches it.
#[test]
fn disposing_a_reactive_server_clears_its_transports_inbound_handler() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::json_codec;
        import std::reactive::{ Disposable, Signal, SignalCell };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let status = Signal::new("idle");
            let (client_end, server_end) = duplex_pair();
            let server = ReactiveServer::new(server_end, json_codec());
            let channel = server.expose(status);
            let mirror: RemoteSource<str> = ReactiveClient::new(client_end, json_codec()).source(channel);
            let watching = mirror.sub(|value| print(i"status = {value}"));
            status.set("busy");
            print(server_end.me.read().is_some());
            server.dispose();
            print(server_end.me.read().is_some());
            watching.dispose();
        }

        main();
        "#,
        "status = idle\nstatus = busy\ntrue\nfalse\n",
    );
}

// The client half is symmetric, and `drop_session` needs no line of its own: it
// disposes the session it drops.
#[test]
fn disposing_a_reactive_client_clears_its_transports_inbound_handler() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::json_codec;
        import std::reactive::{ Disposable, Signal, SignalCell };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let status = Signal::new("idle");
            let (client_end, server_end) = duplex_pair();
            let server = ReactiveServer::new(server_end, json_codec());
            let channel = server.expose(status);
            let client = ReactiveClient::new(client_end, json_codec());
            let mirror: RemoteSource<str> = client.source(channel);
            let watching = mirror.sub(|value| print(i"status = {value}"));
            print(client_end.me.read().is_some());
            client.dispose();
            print(client_end.me.read().is_some());
            watching.dispose();
        }

        main();
        "#,
        "status = idle\ntrue\nfalse\n",
    );
}

// A30 wires `dispose` to the terminal `Closed`, so std now calls it on a client
// an app may already have disposed — which makes idempotence load-bearing
// rather than incidental. Both halves are read: the handler slot (already
// `None` on the second pass) and the ROUTES, which is the half a second pass
// could plausibly disturb and the half `close_for_good` is really there for.
#[test]
fn disposing_a_reactive_client_twice_is_harmless() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::json_codec;
        import std::reactive::{ Disposable, Signal, SignalCell };
        import std::rpc::{ ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };

        fun main() {
            let status = Signal::new("idle");
            let (client_end, server_end) = duplex_pair();
            let server = ReactiveServer::new(server_end, json_codec());
            let channel = server.expose(status);
            let client = ReactiveClient::new(client_end, json_codec());
            let mirror: RemoteSource<str> = client.source(channel);
            let watching = mirror.sub(|value| print(i"status = {value}"));
            print(client.routes.read().len());
            client.dispose();
            print(client.routes.read().len());
            print(client_end.me.read().is_some());
            client.dispose();
            print(client.routes.read().len());
            print(client_end.me.read().is_some());
            watching.dispose();
        }

        main();
        "#,
        "status = idle\n1\n0\nfalse\n0\nfalse\n",
    );
}
