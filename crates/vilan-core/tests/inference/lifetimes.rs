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
                let doubled = count.derive(|n| n * 2);
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

// The total join (`switch(|inner| inner)` since A142 — `flatten` is the `Option`
// join) owes TWO handles: the outer subscription, and whichever inner one
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
                let joined = outer.switch(|inner| inner);
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

// --- A86: the join is a BLANKET over `Flow`, not a member of the cell --------
//
// The read contract is the trait, so an outer that is a pipe or a sealed
// derivation holds an inner signal exactly as a `SignalCell` does. The pin
// above (`flatten_registers_its_outer_and_live_inner_subscriptions`) is the
// control for the cell receiver and for the ownership story; these are the
// receivers the inherent impl could not reach.

#[test]
fn flatten_joins_a_derived_outer_not_only_a_cell() {
    // The outer is a PIPE — the shape `outer.derive(..)` then a join has wanted
    // since A4; over a pipe the total join is `switch(|inner| inner)` (one
    // stage), sealed here because it is read four times. Switching the outer
    // detaches the replaced inner, exactly as it does for a cell.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let first = Signal::new(1);
            let second = Signal::new(10);
            let which = Signal::new(true);
            let picked = which.derive(|flag| if flag { first } else { second });
            let joined = picked.switch(|inner| inner).memo();
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
            let joined = outer.flatten().memo();
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
    // shape in one pin. The total join is `switch(|inner| inner)` since A142.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let first = Signal::new(1);
            let second = Signal::new(10);
            let outer = Signal::new(first);
            let joined = outer.switch(|inner| inner).memo();
            let doubled = joined.derive(|value| value * 2).memo();
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
// is what a module-level `current_path().derive(parse)` needs. Making this an
// error is the stronger law and a breaking change — the owner's call, not std's.
// Since A124 S2c a module-level derivation that CACHES is spelled
// `.cell_global()` (A130 refuses `.cell()` and, since A142, `.memo()` here): the
// lifetime this pin holds is the one that name says.
#[test]
fn a_derivation_outside_every_owner_still_tracks_its_source() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell };

        let count: SignalCell<i32> = Signal::new(1);
        let doubled: SignalCell<i32> = count.derive(|n| n * 2).cell_global();

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
// `derive`/`combine` are static dependencies; `switch` and `and_then` are the
// dynamic pair — WHICH source the result follows is decided by the current
// value. Each is one pipe stage, and its selector is called exactly once per
// value of the source, inside the one instance its consumer started. The `flatten` pins
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
            let picked = which.switch(|n| if n == 0 { first } else { second }).memo();
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
fn switch_calls_its_selector_once_per_change_inside_its_one_instance() {
    // Re-derived at A142 S1 (the pipe model) from A124 S2c's cold-node count.
    // Building a switch calls `select` NOWHERE (`built 0`), and writes nobody
    // consumes call it nowhere (`unread 0`): a pipe runs only inside the
    // instance its consumer starts. Sealed, the instance selects ONCE at start
    // (`sealed 1`), and every value of the source costs ONE call — the
    // re-selection — because the consumer's pull reads the followed inner, not
    // the selector (three writes, `4`). A124's cold node read `cell 3` and `9`
    // (a select per pull plus one per re-follow). Red when the stage's pull
    // re-selects: `cell-after 10 7`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        fun main() {
            let which = Signal::new(0);
            let first = Signal::new(10);
            let second = Signal::new(20);
            let calls: SignalCell<i32> = Signal::new(0);
            let picked = which.switch(|n| {
                calls.set(calls.get() + 1);
                if n == 0 { first } else { second }
            });
            print(i"built {calls.get()}");
            which.set(1);
            which.set(0);
            which.set(1);
            print(i"unread {calls.get()}");
            let cached = picked.memo();
            print(i"sealed {cached.get()} {calls.get()}");
            which.set(0);
            which.set(1);
            which.set(0);
            print(i"cell-after {cached.get()} {calls.get()}");
        }

        main();
        "#,
        "built 0\nunread 0\nsealed 20 1\ncell-after 10 4\n",
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
            let followed = outer.and_then(|id| if id == 1 { one } else { two }).memo();
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
            let picked: SignalCell<i32> = which.switch(|n| if n == 0 { first } else { second }).cell();
            let plus: SignalCell<i32> = picked.derive(|value| value + 1).cell();
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
            let followed: SignalCell<Option<i32>> = outer.and_then(|id| if id == 1 { one } else { two }).cell();
            let plus: SignalCell<i32> = followed.derive(|value| value.unwrap_or(0) + 1).cell();
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
fn switch_and_and_then_mint_nothing_until_a_leaf_and_the_composed_form_costs_the_same() {
    // Re-derived at A124 S2c, and kept by A142 S1. `fresh_id` is the program's
    // subscriber counter, so the delta across a step is the number of
    // subscribers it minted — two `fresh_id()` calls of its own included, hence
    // the `- 1`.
    //
    // All three pipes mint NOTHING when built (a pipe runs only once consumed),
    // and a consumer on each mints the same THREE: the consumer's own record, the
    // stage's relay on the outer, and its relay on whichever inner is current
    // (the one place a stage keeps a registration of its OWN). The composed
    // spelling is `derive(select).switch(|inner| inner)` — the total join is a
    // switch since A142 — and costs what `switch(select)` does. The values are
    // read from what each consumer was handed: a consumed pipe has no `get`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        import std::reactive::{ Signal, SignalCell, Source, fresh_id };
        import std::shared::Shared;

        fun main() {
            let which = Signal::new(0);
            let first = Signal::new(10);
            let second = Signal::new(20);
            let seen_a: Shared<i32> = Shared::new(0);
            let seen_b: Shared<i32> = Shared::new(0);
            let seen_c: Shared<i32> = Shared::new(0);

            let before_switch = fresh_id();
            let picked = which.switch(|n| if n == 0 { first } else { second });
            print(fresh_id() - before_switch - 1);

            let outer: SignalCell<Option<i32>> = Signal::new(Some(1));
            let inner: SignalCell<Option<i32>> = Signal::new(Some(5));
            let before_and_then = fresh_id();
            let followed = outer.and_then(|id| inner);
            print(fresh_id() - before_and_then - 1);

            let before_composed = fresh_id();
            let composed = which
                .derive(|n| if n == 0 { first } else { second })
                .switch(|chosen| chosen);
            print(fresh_id() - before_composed - 1);

            let before_a = fresh_id();
            let _a = picked.on_change(|value| { seen_a.write() = value; });
            print(fresh_id() - before_a - 1);
            let before_b = fresh_id();
            let _b = composed.on_change(|value| { seen_b.write() = value; });
            print(fresh_id() - before_b - 1);
            let before_c = fresh_id();
            let _c = followed.on_change(|value| { seen_c.write() = value.unwrap_or(0); });
            print(fresh_id() - before_c - 1);

            // Every one of the three is live, so none of the counts is the
            // count of a chain that failed to attach.
            which.set(1);
            inner.set(Some(7));
            print(seen_a.read());
            print(seen_b.read());
            print(seen_c.read());
        }

        main();
        "#,
        "0\n0\n0\n3\n3\n3\n20\n20\n7\n",
    );
}

// --- A142 S1: pipes are move-only, and a sealed chain is ONE instance ----------
//
// `proposal/reactive-layers.md` §3 (R29): a transformation is a PIPE — a
// `[resource]` description with no `get()` — consumed exactly once, by sealing
// (`.memo()`, `.cell()`, the `_global` twins, `.sample()`) or by a consumer. These
// pins replace A124 S1's cold-node evidence (N readers = N evaluations, a node
// read on every `get()`), which the pipe model retires: a pipe has one consumer,
// so every stage runs once per change inside the one instance it started.

#[test]
fn a142_s1_a_pipe_that_is_never_consumed_runs_nothing_and_sample_runs_it_once() {
    // An unconsumed pipe registers nothing and computes nothing: three writes to
    // the root of a five-stage pipe do no work. `.sample()` starts it, reads it
    // once — five evaluations, paid at the call — and releases it: the writes
    // after it run nothing either. Red when `sample` attaches and leaves its
    // subscriber behind: `5` becomes `10` after the last write.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let evals: Shared<i32> = Shared::new(0);
            let chain = root
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 });
            print(evals.read());
            root.set(2);
            root.set(3);
            root.set(4);
            print(evals.read());
            print(chain.sample());
            print(evals.read());
            root.set(5);
            print(evals.read());
            print(root.subscribers.read().len());
        }

        main();
        "#,
        "0\n0\n9\n5\n5\n0\n",
    );
}

#[test]
fn a142_s1_a_five_stage_sealed_chain_puts_one_subscriber_on_its_root_and_runs_once_per_change() {
    // The fused instance: sealing a five-stage chain mints ONE subscriber (the
    // memo's) and puts it on the root — every stage forwards it — and a change
    // runs the five stages once, in one pass. Reading the memo three times runs
    // nothing. Red when a stage mints a relay of its own instead of forwarding:
    // `minted=6 on_root=1`, or when the memo pulls twice per change: `evals=10`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, fresh_id };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let evals: Shared<i32> = Shared::new(0);
            let before = fresh_id();
            let sealed = root
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .derive(|x| { evals.write() = evals.read() + 1; x + 1 })
                .memo();
            print(i"minted={fresh_id() - before - 1} on_root={root.subscribers.read().len()}");
            evals.write() = 0;
            root.set(10);
            print(i"evals={evals.read()}");
            print(i"{sealed.get()} {sealed.get()} {sealed.get()} evals={evals.read()}");
        }

        main();
        "#,
        "minted=1 on_root=1\nevals=5\n15 15 15 evals=5\n",
    );
}

#[test]
fn a142_s1_sharing_is_sealing_the_segment_above_a_memo_runs_once() {
    // Two consumers of one derived value need a source between them. Above the
    // `.memo()` the two stages run once per change (2); below it each consumer
    // built its OWN three-stage pipe, which runs once per change in its own
    // instance (3 x 2 = 6).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let upper: Shared<i32> = Shared::new(0);
            let lower: Shared<i32> = Shared::new(0);
            let shared = root
                .derive(|x| { upper.write() = upper.read() + 1; x + 1 })
                .derive(|x| { upper.write() = upper.read() + 1; x + 1 })
                .memo();
            let _a = shared
                .derive(|x| { lower.write() = lower.read() + 1; x + 1 })
                .derive(|x| { lower.write() = lower.read() + 1; x + 1 })
                .derive(|x| { lower.write() = lower.read() + 1; x + 1 })
                .on_change(|_value| {});
            let _b = shared
                .derive(|x| { lower.write() = lower.read() + 1; x + 1 })
                .derive(|x| { lower.write() = lower.read() + 1; x + 1 })
                .derive(|x| { lower.write() = lower.read() + 1; x + 1 })
                .on_change(|_value| {});
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
fn a142_s1_sealing_a_pipe_twice_is_refused() {
    // R29: a second consumer is a compile error, not silent duplicate work.
    assert_fails_with(
        r#"
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let count = Signal::new(1);
            let doubled = count.derive(|n| n * 2);
            let first = doubled.memo();
            let second = doubled.memo();
        }
        "#,
        "use of `doubled` after it was moved: a resource has a single owner",
    );
}

#[test]
fn a142_s1_branching_a_pipe_is_refused() {
    // The same refusal reached by branching: a stage built over `doubled`
    // consumed it, so sealing it afterwards is its second use.
    assert_fails_with(
        r#"
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let count = Signal::new(1);
            let doubled = count.derive(|n| n * 2);
            let plus = doubled.derive(|n| n + 1);
            let sealed = doubled.memo();
        }
        "#,
        "use of `doubled` after it was moved: a resource has a single owner",
    );
}

#[test]
fn a142_s1_handing_one_pipe_to_two_consumers_is_refused() {
    // A consumer (`sub`) takes `own self` too: the second consumer is refused
    // wherever it is — a binding, an effect, a seal.
    assert_fails_with(
        r#"
        import std::reactive::{ Signal, SignalCell, Disposable };

        fun main() {
            let count = Signal::new(1);
            let doubled = count.derive(|n| n * 2);
            let one = doubled.sub(|n| print(n));
            let two = doubled.sub(|n| print(n));
        }
        "#,
        "use of `doubled` after it was moved: a resource has a single owner",
    );
}

#[test]
fn a142_s1_a_root_is_copied_freely() {
    // Roots are data: the same cell handed to two `own` flow parameters is two
    // copies of one handle, and both consumers follow the one cell.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Flow, Signal, SignalCell };

        fun show(label: str, own value: Flow<i32>) {
            let _watch = value.sub(|n| print(i"{label} {n}"));
        }

        fun main() {
            let count = Signal::new(1);
            show("a", count);
            show("b", count);
            show("pipe", count.derive(|n| n * 10));
            count.set(2);
        }

        main();
        "#,
        "a 1\nb 1\npipe 10\na 2\nb 2\npipe 20\n",
    );
}

#[test]
fn a142_s1_a_root_has_no_memo() {
    // R29: the sealing operations exist only on pipes. A cell is already a
    // source, so `cell.memo()` has no method.
    assert_fails_with(
        r#"
        import std::reactive::{ Signal, SignalCell };

        fun main() {
            let count = Signal::new(1);
            let sealed = count.memo();
        }
        "#,
        "SignalCell<i32> has no method 'memo'",
    );
}

#[test]
fn a142_s1_a_pipe_has_no_get() {
    // A pipe is a description: it has no `get`. The steer to `.memo()` /
    // `.sample()` is solver-44's (A142 §3.3); this pins the refusal.
    assert_fails_with(
        r#"
        import std::reactive::{ Signal, SignalCell, Source };

        fun main() {
            let count = Signal::new(1);
            print(count.derive(|n| n * 2).get());
        }
        "#,
        "has no method 'get'",
    );
}

#[test]
fn a142_s1_a_diamond_of_sealed_arms_pulls_a_settled_pair_in_a_turn() {
    // `combine` takes sources, so a diamond over one root is two sealed arms and
    // a combined leaf. In a turn the arms are derivations (phase 1) and the leaf
    // an effect (phase 2): the leaf runs once, on the settled pair.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ FlushPolicy, Signal, SignalCell, Source, combine, turn };
        import std::shared::Shared;

        fun main() {
            let source: SignalCell<i32> = Signal::new(1);
            let seen: Shared<str> = Shared::new("");
            let diamond = combine((source.derive(|x| x * 10).memo(), source.derive(|x| x + 100).memo()));
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
fn a142_s1_one_consumer_over_a_root_twice_is_one_subscriber_deduped_in_a_turn() {
    // A124 R2's dedup, through the pipe protocol: `combine` forwards the
    // consumer's ONE record to both of its arms, and both arms are the same root,
    // so the root holds that record twice. In a turn the two notifications are one
    // call; inline, the root's list walk calls it twice — a duplicate CALL, never
    // a torn pair, because both calls pull. Red when `SignalCell::on_settle` mints
    // a fresh id per registration: `(3,3)(3,3)` in the turn.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ FlushPolicy, Signal, SignalCell, Source, combine, turn };
        import std::shared::Shared;

        fun main() {
            let source: SignalCell<i32> = Signal::new(1);
            let seen: Shared<str> = Shared::new("");
            let _leaf = combine((source, source)).on_change(|pair| {
                let (left, right) = pair;
                seen.write() = i"{seen.read()}({left},{right})";
            });
            turn(FlushPolicy::AtEnd, || {
                source.set(3);
            });
            print(seen.read());
            seen.write() = "";
            source.set(4);
            print(seen.read());
        }

        main();
        "#,
        "(3,3)\n(4,4)(4,4)\n",
    );
}

#[test]
fn a124_s2a_a_leaf_disposed_by_an_earlier_effect_in_its_wave_does_not_fire() {
    // Door 1 through the protocol. Both observers are queued in one wave; the
    // first (lower id) disposes the pipe's consumer, whose record is already
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
            held.write() = Some(source.derive(|x| x * 10).on_change(|value| {
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
fn a142_s1_a_root_of_ones_own_forwards_the_consumers_record() {
    // A root that is not a `SignalCell` — `get` and `on_settle` and nothing else,
    // the `Stored` of the reference page — hands the consumer's record to the
    // cell it wraps, so the dedup is the cell's: once in a turn, and a duplicate
    // call (never a torn pair) inline. Red when a root mints a relay of its own
    // and wakes the consumer directly: `(30,30)(30,30)` in the turn.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{
            FlushPolicy, Signal, SignalCell, Source, Subscriber, Subscription, combine, turn,
        };
        import std::shared::Shared;

        struct Stored<T> {
            inner: SignalCell<T>,
        }

        impl Stored<type T> with Source<T> {
            fun get(self): T {
                self.inner.get()
            }

            fun on_settle(self, subscriber: Subscriber): Subscription {
                self.inner.on_settle(subscriber)
            }
        }

        fun main() {
            let cell: SignalCell<i32> = Signal::new(1);
            let root = Stored { inner = cell };
            let seen: Shared<str> = Shared::new("");
            let _leaf = combine((root, root)).derive(|pair| {
                let (left, right) = pair;
                (left * 10, right + 100)
            }).on_change(|pair| {
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
// The node types are `std::reactive`'s; since the flip (S2c) `map`, `switch`,
// both `flatten`s, `and_then` and `combine` build them, and `.cell()` and
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
            let cached = root.derive(|x| x / 10).cell();
            let _c = cached.on_change(|value| {
                cell_log.write() = i"{cell_log.read()}{value},";
            });
            let _d = root.derive(|x| x / 10).distinct().on_change(|value| {
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
            holder.write() = Some(root.derive(|x| x * 2 + 1).cell());
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
            let cached = run_with_owner(owner, || root.derive(|x| x * 10).cell());
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
                    .switch(|n| if n == 0 { first } else { second })
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
    // `sub`, `effect` and `derive` are `Flow` members, reached on a stage through
    // its impl. Each stage's value type is a DIRECT binder of its impl subject
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
            let _switched = which.switch(|n| first).sub(|value| print(value + 1));
            let _distinct = which.distinct().sub(|value| print(value + 2));
            let labelled = which.derive(|n| i"n{n}").derive(|label| label.len());
            print(labelled.sample());
        }

        main();
        "#,
        "11\n2\n2\n",
    );
}

#[test]
fn a124_s2b_combine_over_the_tuple_bound_joins_three_kinds_of_source() {
    // The n-ary `Combine` over `(U in T: dyn Source<U>)`: a root cell, a sealed
    // `.memo()` and a `.cell()`, three element types, one stage — notified once
    // per settle in a turn however many of its inputs stand on the written root.
    // Sealed, because it is read after it is subscribed (a pipe has one
    // consumer); its arms are sources, since a pipe cannot become a `dyn`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ FlushPolicy, Signal, SignalCell, Source, combine, turn };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let arms: (dyn Source<i32>, dyn Source<str>, dyn Source<bool>) = (
                count,
                count.derive(|n| i"n{n}").memo(),
                count.derive(|n| n > 2).cell()
            );
            let all = combine(arms).memo();
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
    // `.or(0)` reads 0 while it is out, and the `derive` below the fallback is
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
            let user: Resource<i32> = id.load(|n| {
                sleep(5);
                Ok(n * 10)
            });
            let shown = user.or(0).derive(|n| n + 1).memo();
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
            let optional = match r.optional().sample() {
                Some(let value) => i"{value}",
                None => "none",
            };
            i"or={r.or(0).sample()} latest={r.latest(-1).sample()} optional={optional} pending={r.is_pending().sample()}"
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

// --- A130: a module binding's `.cell()` is refused; `.cell_global()` says it --
//
// A `.cell()` is owner-tied, and a module binding's initializer has no owner, so
// its registration stays on the upstream for the life of the program. The
// direct spelling is refused with a steer; `.cell_global()` is the spelling that
// names the lifetime. Since A142 a seal exists only on a PIPE (a root has no
// `.cell()`), so these pins seal `path.derive(|p| p)`, and `.memo()` — the
// read-only seal — is refused the same way, steering to `.memo_global()`. STATIC-ONLY (R-e): the initializer's own calls, not a
// closure it creates and not a call it makes into a function that builds one.

const A130_REFUSAL: &str = "`.cell()` in the initializer of the module binding";

#[test]
fn a130_a_cell_in_a_module_bindings_initializer_is_refused_at_the_method() {
    // Red before A130: the program compiled, and `route`'s registration stayed
    // on `path` for the life of the program.
    let source = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");
        let route: SignalCell<str> = path.derive(|p| p).cell();

        fun main() {
            print(route.get());
        }
        "#;
    assert_fails_spanning(source, "cell", A130_REFUSAL);
    assert_fails_with(source, "the module binding `route`");
    assert_fails_with(
        source,
        "or write `.cell_global()`, which says that lifetime",
    );
}

#[test]
fn rk_a_transient_in_a_module_bindings_initializer_is_refused_with_its_own_twin() {
    // R-k: `.transient()` registers with the ambient owner exactly as `.cell()`
    // does, so in a module binding's initializer it is refused the same way —
    // naming its own seal and steering to `.transient_global()`. Both arms: a flow
    // of `Result` tasks and a flow of bare tasks. Red before R-k: the program
    // compiled, and the seal's registration stayed on `id` for the program.
    let source = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::result::Result::{ self, Ok, Err };
        import std::reactive::transient::Transient;

        let id: SignalCell<i32> = Signal::new(1);
        let fetched: Transient<i32, str> = id.derive(|x| async Ok(x * 10)).transient();
        let bare: Transient<i32, str> = id.derive(|x| async { x * 100 }).transient();

        fun main() {
            print(fetched.get().is_some());
        }
        "#;
    // Occurrence 0 is the import's `std::reactive::transient`; 1 and 2 are the two seals.
    for occurrence in [1, 2] {
        assert_fails_spanning_nth(
            source,
            "transient",
            occurrence,
            "`.transient()` in the initializer of the module binding",
        );
    }
    assert_fails_with(source, "the module binding `fetched`");
    assert_fails_with(source, "the module binding `bare`");
    assert_fails_with(
        source,
        "or write `.transient_global()`, which says that lifetime",
    );
}

#[test]
fn rk_transient_global_at_module_level_compiles_and_follows_its_source() {
    // The program-lifetime spelling: both arms compile in a module binding's
    // initializer and follow their source to the latest task's answer.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };
        import std::result::Result::{ self, Ok, Err };
        import std::time::sleep;
        import std::reactive::transient::Transient;

        let id: SignalCell<i32> = Signal::new(1);
        let fetched: Transient<i32, str> = id.derive(|x| async Ok(x * 10)).transient_global();
        let bare: Transient<i32, str> = id.derive(|x| async { x * 100 }).transient_global();

        fun main() {
            sleep(10);
            print(i"{fetched.get().unwrap_or(0)} {bare.get().unwrap_or(0)}");
            id.set(2);
            sleep(10);
            print(i"{fetched.get().unwrap_or(0)} {bare.get().unwrap_or(0)}");
        }

        main();
        "#,
        "10 100\n20 200\n",
    );
}

#[test]
fn rk_transient_global_ignores_an_ambient_owner_where_transient_is_released_with_it() {
    // The lifetime is in the name: made under an owner that is then disposed,
    // `.transient_global()` keeps following its source; `.transient()` made the
    // same way stops at the disposal and keeps its last answer. Red when the
    // global twin registers with the owner: `global=10`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };
        import std::result::Result::{ self, Ok, Err };
        import std::time::sleep;
        import std::reactive::transient::Transient;

        fun main() {
            let id: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            let scoped: Transient<i32, str> = run_with_owner(owner, || id.derive(|x| async Ok(x * 10)).transient());
            let global: Transient<i32, str> = run_with_owner(owner, || id.derive(|x| async Ok(x * 10)).transient_global());
            sleep(10);
            owner.dispose();
            id.set(2);
            sleep(10);
            print(i"scoped={scoped.get().unwrap_or(0)} global={global.get().unwrap_or(0)}");
        }

        main();
        "#,
        "scoped=10 global=20\n",
    );
}

#[test]
fn a142_a130_a_memo_in_a_module_bindings_initializer_is_refused_with_its_own_twin() {
    // `.memo()` is owner-tied exactly as `.cell()` is, and it is refused the same
    // way — naming its own seal and steering to its own `_global` twin. Red when
    // the check keys on `cell` alone: the program compiles.
    let source = r#"
        import std::io::print;
        import std::reactive::{ MemoCell, Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");
        let route: MemoCell<str> = path.derive(|p| p).memo();

        fun main() {
            print(route.get());
        }
        "#;
    assert_fails_with(
        source,
        "`.memo()` in the initializer of the module binding `route`",
    );
    assert_fails_with(
        source,
        "or write `.memo_global()`, which says that lifetime",
    );
}

#[test]
fn a142_a130_memo_global_at_module_level_compiles_and_follows_its_source() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ MemoCell, Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");
        let depth: MemoCell<usize> = path.derive(|p| p.len()).memo_global();

        fun main() {
            print(depth.get());
            path.set("/docs");
            print(depth.get());
        }

        main();
        "#,
        "1\n5\n",
    );
}

#[test]
fn a130_the_refusal_reaches_a_cell_through_a_dyn_receiver() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");
        let erased: dyn Source<str> = path;
        let cached: SignalCell<str> = erased.derive(|p| p).cell();

        fun main() {
            print(cached.get());
        }
        "#,
        "the module binding `cached`",
    );
}

#[test]
fn a130_the_refusal_reaches_a_cell_inside_a_struct_literal_field() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        struct Holder {
            value: SignalCell<str>,
        }

        let path: SignalCell<str> = Signal::new("/");
        let held: Holder = Holder { value = path.derive(|p| p).cell() };

        fun main() {
            print(held.value.get());
        }
        "#,
        "the module binding `held`",
    );
}

#[test]
fn a130_the_refusal_reaches_a_cell_passed_as_an_argument() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        fun keep(value: SignalCell<str>): SignalCell<str> {
            value
        }

        let path: SignalCell<str> = Signal::new("/");
        let kept: SignalCell<str> = keep(path.derive(|p| p).cell());

        fun main() {
            print(kept.get());
        }
        "#,
        "the module binding `kept`",
    );
}

#[test]
fn a130_the_refusal_reaches_a_cell_in_a_value_if_branch() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");
        let chosen: SignalCell<str> = if true { path.derive(|p| p).cell() } else { path };

        fun main() {
            print(chosen.get());
        }
        "#,
        "the module binding `chosen`",
    );
}

#[test]
fn a130_one_diagnostic_per_refused_binding_and_none_for_the_others() {
    // Two refused bindings, two diagnostics; the global, the closure and the
    // root beside them add none.
    let source = r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");
        let first: SignalCell<str> = path.derive(|p| p).cell();
        let global: SignalCell<str> = path.derive(|p| p).cell_global();
        let second: SignalCell<str> = path.derive(|p| p).cell();

        fun main() {
            print(i"{first.get()}{global.get()}{second.get()}");
        }
        "#;
    let refusals: Vec<String> = failure_diagnostics(source)
        .into_iter()
        .map(|(message, _span)| message)
        .filter(|message| message.contains(A130_REFUSAL))
        .collect();
    assert_eq!(
        refusals.len(),
        2,
        "one refusal per binding; got {refusals:?}"
    );
    assert!(
        refusals[0].contains("`first`") && refusals[1].contains("`second`"),
        "{refusals:?}"
    );
}

#[test]
fn a130_a_closure_the_initializer_creates_is_not_refused() {
    // Creating a closure is inert, so the check does not enter it (the load-time
    // rule `init_order` keeps). It is ALSO the second half of the documented
    // remainder: a closure captures its context at CREATION (spec §8.4), and a
    // module-level closure was created with no owner ambient, so calling it
    // under one does not tie the cell to that owner — the registration outlives
    // the dispose (`1`), exactly as a `.cell()` reached through a call does.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };

        let path: SignalCell<str> = Signal::new("/");
        let cached_path = || path.derive(|p| p).cell();

        fun main() {
            let owner = Owner::new();
            let cached = run_with_owner(owner, || cached_path());
            print(i"{cached.get()} {path.subscribers.read().len()}");
            owner.dispose();
            print(path.subscribers.read().len());
        }

        main();
        "#,
        "/ 1\n1\n",
    );
}

#[test]
fn a130_a_cell_reached_through_a_call_from_module_init_is_the_documented_remainder() {
    // STATIC-ONLY by ruling (R-e): a `.cell()` one call down from the
    // initializer is not caught, and leaks exactly as the refused spelling
    // would. Pinned so that a widening of the check is a decision, not a drift.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");

        fun cached(): SignalCell<str> {
            path.derive(|p| p).cell()
        }

        let through: SignalCell<str> = cached();

        fun main() {
            path.set("/a");
            print(i"{through.get()} {path.subscribers.read().len()}");
        }

        main();
        "#,
        "/a 1\n",
    );
}

#[test]
fn a130_a_cell_in_a_function_body_is_not_refused() {
    // Only a MODULE binding's initializer: the same spelling in `main` has an
    // owner to meet when one is ambient, and is not this check's.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        fun main() {
            let path: SignalCell<str> = Signal::new("/");
            let cached = path.derive(|p| p).cell();
            path.set("/b");
            print(cached.get());
        }

        main();
        "#,
        "/b\n",
    );
}

#[test]
fn a130_a_users_own_cell_method_is_not_refused() {
    // The check names std's `.cell()`, not a spelling: a user's inherent `cell`
    // at module level is an ordinary call.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Box {
            n: i32,
        }

        impl Box {
            fun cell(self): i32 {
                self.n + 1
            }
        }

        let answer: i32 = Box { n = 41 }.cell();

        fun main() {
            print(answer);
        }

        main();
        "#,
        "42\n",
    );
}

#[test]
fn a130_cell_global_at_module_level_compiles_and_follows_its_source() {
    // Red before A130 on the name alone (`SignalCell<str> has no method
    // 'cell_global'`).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        let path: SignalCell<str> = Signal::new("/");
        let route: SignalCell<str> = path.derive(|value| "route" + value).cell_global();

        fun main() {
            print(route.get());
            path.set("/docs");
            print(route.get());
        }

        main();
        "#,
        "route/\nroute/docs\n",
    );
}

#[test]
fn a130_cell_global_ignores_an_ambient_owner_where_cell_is_released_with_it() {
    // The lifetime is in the name: made under an owner, `.cell_global()` still
    // follows its source after the owner is disposed, and `.cell()` beside it
    // does not — and the same for A142's read-only twins, `.memo_global()` and
    // `.memo()`. Two subscribers are left on the root — the two globals'.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            let global = run_with_owner(owner, || root.derive(|n| n).cell_global());
            let owned = run_with_owner(owner, || root.derive(|n| n).cell());
            let memo_global = run_with_owner(owner, || root.derive(|n| n).memo_global());
            let memo = run_with_owner(owner, || root.derive(|n| n).memo());
            owner.dispose();
            root.set(2);
            print(i"global={global.get()} owned={owned.get()} left={root.subscribers.read().len()}");
            print(i"memo_global={memo_global.get()} memo={memo.get()}");
        }

        main();
        "#,
        "global=2 owned=1 left=2\nmemo_global=2 memo=1\n",
    );
}

#[test]
fn a124_s2c_combine_erases_a_cell_and_a_node_at_the_call() {
    // The flip's `combine`: its mapped-tuple parameter `(U in T: dyn Source<U>)`
    // erases each element at the call, so a root cell and a derivation mix in one
    // literal with NO annotated tuple of `dyn`s (B398 made the position coerce
    // its elements). Since A142 a derived arm is SEALED — a pipe is a
    // `[resource]` and cannot become a `dyn` — so the root holds the memo's one
    // record from the start (`built=1`). The combined pipe registers nothing
    // until consumed; its consumer puts one record per arm on what each arm is
    // (`subscribed=2` on the root: the memo's and the consumer's), and one write
    // fires it once in a turn with the settled pair.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ FlushPolicy, Signal, SignalCell, Source, combine, turn };

        fun main() {
            let name: SignalCell<str> = Signal::new("ada");
            let length = name.derive(|value| value.len()).memo();
            let both = combine((name, length));
            print(i"built={name.subscribers.read().len()}");
            let _leaf = both.on_change(|pair| {
                let (value, size) = pair;
                print(i"changed {value} {size}");
            });
            print(i"subscribed={name.subscribers.read().len()}");
            turn(FlushPolicy::AtEnd, || {
                name.set("grace");
            });
            let (first, size) = combine((name, length)).sample();
            print(i"{first} {size}");
        }

        main();
        "#,
        "built=1\nsubscribed=2\nchanged grace 5\ngrace 5\n",
    );
}

#[test]
fn a124_s2c_a_node_over_a_tuple_upstream_forwards_through_its_bound() {
    // B410 retired at S2c: a `map` over a `combine` reaches the `Combine`'s
    // `on_settle` through its bound at a TUPLE trait argument, and the override
    // answers — it forwards the leaf's record, so subscribing the leaf mints
    // exactly ONE subscriber (the leaf's own). Before B410 the bound selected
    // `Source::on_settle`'s payload bridge here instead, which minted a relay of
    // its own (S2b's note: "costs one relay"). Planted — `Combine` without its
    // `on_settle` override, so the bridge answers — this reds: the bridge calls
    // back into `Combine::on_change`, which calls `on_settle`, and the two recurse
    // (`RangeError: Maximum call stack size exceeded`), the failure the S2b-era
    // "call your OWN `on_settle`" rule existed to avoid.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, combine, fresh_id };

        fun main() {
            let a: SignalCell<i32> = Signal::new(1);
            let b: SignalCell<i32> = Signal::new(2);
            let sum = combine((a, b)).derive(|(x, y)| x + y);
            let before = fresh_id();
            let _leaf = sum.on_change(|value| print(i"sum {value}"));
            print(i"minted {fresh_id() - before - 1}");
            a.set(10);
        }

        main();
        "#,
        "minted 1\nsum 12\n",
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

// --- A135 door (c): a handle-returning `[rpc]` whose tail is `.cell()` --------

/// The `[service]` both A135 pins share: one handle method per shape, and the
/// controls beside them. `{BODY}` is replaced per pin.
const A135_SERVICE: &str = r#"
    import std::reactive::{ Signal, SignalCell };
    import std::hash_map::HashMap;

    [service(StoreClient)]
    struct Store {
        names: SignalCell<HashMap<i32, str>>,
        kept: SignalCell<str>,
    }

    impl Store {
        {BODY}
    }

    fun main() {}
"#;

/// The warnings a `Store` body raises that name A135's hazard, as
/// `(method named, the text the warning spans)`.
fn a135_warnings(body: &str) -> Vec<(String, String)> {
    let source = A135_SERVICE.replace("{BODY}", body);
    warning_diagnostics(&source)
        .into_iter()
        .filter(|(message, _)| message.contains("returns a signal handle it builds with `.cell()`"))
        .map(|(message, range)| {
            let method = message.split('`').nth(1).unwrap_or_default().to_string();
            (method, source[range].to_string())
        })
        .collect()
}

/// A135 (c), the warning's cases: the plain handle, a block-bodied one whose
/// TAIL is the `.cell()`, and the `Option` form's `Some(…cell())`. Each is
/// anchored at the `.cell` name of the call that mints per call, and names the
/// method. (Kolt's `store.vl` wrote the first shape at three sites after the
/// v0.41.0 flip.)
#[test]
fn a135_a_handle_method_whose_tail_is_cell_warns_with_the_memo_steer() {
    let found = a135_warnings(
        r#"
        [rpc]
        fun first_name(self): SignalCell<Option<str>> {
            self.names.derive(|names| names.get(1)).cell()
        }

        [rpc]
        fun count(self): SignalCell<usize> {
            let unused = 0;
            self.names.derive(|names| names.len()).cell()
        }

        [rpc]
        fun maybe(self, id: i32): Option<SignalCell<str>> {
            Some(self.names.derive(|names| names.get(id).unwrap_or_default()).cell())
        }
        "#,
    );
    let mut methods: Vec<&str> = found.iter().map(|(method, _)| method.as_str()).collect();
    methods.sort_unstable();
    assert_eq!(methods, vec!["count", "first_name", "maybe"], "{found:#?}");
    assert!(
        found.iter().all(|(_, spanned)| spanned == "cell"),
        "every warning spans the `.cell` name: {found:#?}"
    );
    let steered = warning_diagnostics(&A135_SERVICE.replace(
        "{BODY}",
        "[rpc]\n        fun count(self): SignalCell<usize> {\n            self.names.derive(|names| names.len()).cell()\n        }",
    ));
    assert!(
        steered
            .iter()
            .any(|(message, _)| message.contains("a `Memo` whose maker writes `.cell_global()`")),
        "a `.cell()` tail is steered to `.cell_global()`: {steered:#?}"
    );
}

/// A135 (c), the controls: a handle returning a cell that OUTLIVES the call (a
/// field, which is what the dedup keys on), a `.cell_global()` (not this
/// hazard's shape), an `Option` built from a field, a plain value method whose
/// body merely USES a `.cell()`, and a free function that is no `[rpc]` at all.
/// None warns.
#[test]
fn a135_a_handle_method_returning_what_outlives_the_call_does_not_warn() {
    let found = a135_warnings(
        r#"
        [rpc]
        fun kept(self): SignalCell<str> {
            self.kept
        }

        [rpc]
        fun global(self): SignalCell<usize> {
            self.names.derive(|names| names.len()).cell_global()
        }

        [rpc]
        fun kept_if(self, present: bool): Option<SignalCell<str>> {
            if present { Some(self.kept) } else { None }
        }

        [rpc]
        fun size(self): usize {
            let derived = self.names.derive(|names| names.len()).cell();
            derived.get()
        }
        "#,
    );
    assert!(found.is_empty(), "{found:#?}");
}

// --- A141 (R-f, door a's warning): a `.cell()` a handler STORES -------------

/// The `[service]` both A141 pins share: a shared store's caches (a
/// `Shared<Option<..>>` slot, a list, a map) and the cell they derive from.
/// `{BODY}` is replaced per pin; `{FREE}` is code outside the service.
const A141_SERVICE: &str = r#"
    import std::reactive::{ MemoCell, Signal, SignalCell };
    import std::hash_map::HashMap;
    import std::shared::Shared;

    [service(StoreClient)]
    struct Store {
        names: SignalCell<HashMap<i32, str>>,
        slot: Shared<Option<SignalCell<usize>>>,
        kept: Shared<List<SignalCell<usize>>>,
        by_id: Shared<HashMap<i32, SignalCell<usize>>>,
        memos: Shared<List<MemoCell<usize>>>,
    }

    impl Store {
        {BODY}
    }

    {FREE}

    fun main() {}
"#;

/// The warnings a program raises that name A141's hazard, as the text each
/// spans.
fn a141_warnings(body: &str, free: &str) -> Vec<String> {
    let source = A141_SERVICE.replace("{BODY}", body).replace("{FREE}", free);
    warning_diagnostics(&source)
        .into_iter()
        .filter(|(message, _)| message.contains("stored on a structure that outlives the call"))
        .map(|(_, range)| source[range].to_string())
        .collect()
}

/// A141 (R-f: door b, plus door a's warning), the warning's cases: inside an
/// `[rpc]` handler, a `.cell()` STORED through `Shared::write` — assigned to
/// the slot (bare and as `Some(..)`), pushed onto a list, inserted into a map
/// — a `.memo()` pushed the same way, and one built in a `Shared<Option<T>>::get_or_insert` maker (I8's). A
/// handler's `.cell()` is owned by the connection under `Service::factory`, so
/// a store every connection shares hands it out dead after that connection
/// closes. (A `Memo` maker is A136's warning, raised everywhere.)
#[test]
fn a141_a_cell_a_handler_stores_through_shared_write_or_a_maker_warns() {
    let found = a141_warnings(
        r#"
        [rpc]
        fun fill(self): usize {
            self.slot.write() = Some(self.names.derive(|names| names.len()).cell());
            self.kept.write().push(self.names.derive(|names| names.len()).cell());
            self.by_id.write().insert(1, self.names.derive(|names| names.len()).cell());
            self.memos.write().push(self.names.derive(|names| names.len()).memo());
            0
        }

        [rpc]
        fun count(self): SignalCell<usize> {
            self.slot.get_or_insert(|| self.names.derive(|names| names.len()).cell())
        }
        "#,
        "",
    );
    assert_eq!(
        found,
        vec!["cell", "cell", "cell", "memo", "cell"],
        "{found:#?}"
    );
}

/// A141, the controls: the same stores written `.cell_global()` (the rule's
/// spelling), a `.cell()` kept in a LOCAL (A135's per-call shape, not a
/// store), and the identical store through `Shared::write` in a free function
/// that is no handler — the ruling scopes the warning to handlers. None warns.
#[test]
fn a141_a_handler_storing_what_outlives_the_connection_does_not_warn() {
    let found = a141_warnings(
        r#"
        [rpc]
        fun fill(self): usize {
            self.slot.write() = Some(self.names.derive(|names| names.len()).cell_global());
            self.kept.write().push(self.names.derive(|names| names.len()).cell_global());
            let local = self.names.derive(|names| names.len()).cell();
            local.get()
        }

        [rpc]
        fun count(self): SignalCell<usize> {
            self.slot.get_or_insert(|| self.names.derive(|names| names.len()).cell_global())
        }
        "#,
        r#"
        fun outside(store: Store) {
            store.slot.write() = Some(store.names.derive(|names| names.len()).cell());
        }
        "#,
    );
    assert!(found.is_empty(), "{found:#?}");
}

/// A145 + A135 (c): a handle method written as the read-only seal is found
/// through its `reply_source_memo` route like any other, so a tail `.memo()` —
/// a fresh memo per call — warns with the same steer; a stored memo does not.
#[test]
fn a145_a_memo_handle_method_whose_tail_is_memo_warns() {
    let source = r#"
    import std::reactive::{ MemoCell, Signal, SignalCell };

    [service(StoreClient)]
    struct Store {
        count: SignalCell<i32>,
        kept: MemoCell<i32>,
    }

    impl Store {
        [rpc]
        fun fresh(self): MemoCell<i32> {
            self.count.derive(|n| n * 2).memo()
        }

        [rpc]
        fun maybe(self, present: bool): Option<MemoCell<i32>> {
            Some(self.count.derive(|n| n + 1).memo())
        }

        [rpc]
        fun kept(self): MemoCell<i32> {
            self.kept
        }
    }

    fun main() {}
"#;
    let mut methods: Vec<String> = warning_diagnostics(source)
        .into_iter()
        .filter(|(message, _)| message.contains("returns a signal handle it builds with"))
        .map(|(message, range)| {
            assert_eq!(&source[range], "memo", "{message}");
            // The warning names the seal the body wrote, and its program-lifetime
            // twin (A135's tail, transient-44's find): red when it was hard-coded
            // to `.cell()` / `.cell_global()`.
            assert!(
                message.contains("builds with `.memo()` on every call")
                    && message.contains("a `Memo` whose maker writes `.memo_global()`"),
                "{message}"
            );
            message.split('`').nth(1).unwrap_or_default().to_string()
        })
        .collect();
    methods.sort_unstable();
    assert_eq!(methods, vec!["fresh", "maybe"]);
}

// --- A136 door (b): an owner-taking call inside a `Memo` maker ---------------

/// The warnings a program raises that name A136's hazard, as the text each
/// spans.
fn a136_warnings(source: &str) -> Vec<String> {
    warning_diagnostics(source)
        .into_iter()
        .filter(|(message, _)| message.contains("inside a `Memo` maker ties what it builds"))
        .map(|(_, range)| source[range].to_string())
        .collect()
}

/// A136 (b), the warning's cases: a `.cell()` and an `effect` written directly
/// in a maker — through `get_or_insert` and through the deprecated `get_or`
/// alias alike (I7). Kolt's `model.vl` wrote the first at four sites: the
/// memo is program-lifetime, the `.cell()` dies with the FIRST caller's owner,
/// and every later ask got the dead cell.
#[test]
fn a136_a_cell_or_an_effect_inside_a_memo_maker_warns() {
    let found = a136_warnings(
        r#"
        import std::memo::Memo;
        import std::reactive::{ Owner, Signal, SignalCell, owner_scope };

        let source: SignalCell<i32> = Signal::new(1);
        let doubled: Memo<i32, SignalCell<i32>> = Memo::new();
        let watched: Memo<i32, bool> = Memo::new();

        fun main() {
            owner_scope.run(Owner::new(), || {
                let a = doubled.get_or_insert(1, || source.derive(|n| n * 2).cell());
                let b = doubled.get_or(2, || source.derive(|n| n * 3).cell());
                let c = watched.get_or_insert(1, || {
                    source.effect(|n| {});
                    true
                });
            });
        }
        "#,
    );
    assert_eq!(found, vec!["cell", "cell", "effect"], "{found:#?}");
}

/// A136 (b), the controls: the maker that builds `.cell_global()` (the rule's
/// own spelling), the lease taken at the CALL SITE on what the memo answers
/// (the fixed shape), a `.cell()` inside a closure the maker merely CREATES
/// (inert until something runs it — the ruling's static line), and a
/// `.cell()` in an ordinary closure argument that is no memo's. None warns.
#[test]
fn a136_a_maker_that_builds_what_outlives_the_caller_does_not_warn() {
    let found = a136_warnings(
        r#"
        import std::memo::Memo;
        import std::reactive::{ Owner, Signal, SignalCell, owner_scope };

        let source: SignalCell<i32> = Signal::new(1);
        let global: Memo<i32, SignalCell<i32>> = Memo::new();
        let later: Memo<i32, || SignalCell<i32>> = Memo::new();

        fun apply(make: || SignalCell<i32>): SignalCell<i32> {
            make()
        }

        fun main() {
            owner_scope.run(Owner::new(), || {
                let kept = global.get_or_insert(1, || source.derive(|n| n * 2).cell_global());
                let leased = global.get_or_insert(2, || source.derive(|n| n + 1).cell_global()).derive(|n| n).cell();
                let deferred = later.get_or_insert(1, || || source.derive(|n| n).cell());
                let plain = apply(|| source.derive(|n| n).cell());
            });
        }
        "#,
    );
    assert!(found.is_empty(), "{found:#?}");
}
// --- A142 S2: every body a pipe runs has an owner per run --------------------
//
// `proposal/reactive-layers.md` §4 (R2, R14): because a pipe has exactly one
// consumer, every body in it runs once per change inside one instance, so every
// run of a body — a `derive` transform, a `switch` selector, an `effect` — gets
// an owner of its own, released when the body runs again and when the instance
// is released. The owner is an epoch of one cell per stage, and its cleanup list
// is allocated at the run's first registration (§4.1): a run that registers
// nothing costs a read and a write. A task a run starts belongs to the run's
// nursery and is cancelled at its release.

#[test]
fn a142_s2_a_body_that_registers_nothing_allocates_no_owner() {
    // `owner_lists_allocated` counts every cleanup list an owner ever made, and
    // `run_nurseries_allocated` every nursery a pipe run made (M92). Ten changes
    // through a sealed two-stage chain and a plain effect allocate NEITHER
    // (`quiet=0 nurseries=0`); the same ten through an effect whose body
    // registers one cleanup per run allocate one list per run (`busy=10`) and
    // still no nursery; ten through an effect whose body starts a task allocate
    // one nursery per run (`spawning=10`): a nursery a task registered with is
    // cancelled with its run. Red when the run's owner allocates its list up
    // front: `quiet=30`; red when every run makes a nursery (before M92):
    // `nurseries=30` and `busy_nurseries=10`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{
            Owner, Signal, SignalCell, Source, on_cleanup, owner_lists_allocated, run_nurseries_allocated,
            run_with_owner,
        };

        fun main() {
            let root: SignalCell<i32> = Signal::new(0);
            let boundary = Owner::new();
            let sealed = run_with_owner(boundary, || {
                root.effect(|value: i32| {
                    let _ignored = value + 1;
                });
                root.derive(|value| value + 1).derive(|value| value * 2).memo()
            });
            let before = owner_lists_allocated();
            let before_nurseries = run_nurseries_allocated();
            mut step = 1;
            for step <= 10 {
                root.set(step);
                step += 1;
            }
            print(i"quiet={owner_lists_allocated() - before} nurseries={run_nurseries_allocated() - before_nurseries} value={sealed.get()}");
            let noisy = Owner::new();
            run_with_owner(noisy, || {
                root.effect(|_value: i32| on_cleanup(|| {}));
            });
            let before_busy = owner_lists_allocated();
            let before_busy_nurseries = run_nurseries_allocated();
            step = 1;
            for step <= 10 {
                root.set(step);
                step += 1;
            }
            print(i"busy={owner_lists_allocated() - before_busy} busy_nurseries={run_nurseries_allocated() - before_busy_nurseries}");
            noisy.dispose();
            let spawning = Owner::new();
            run_with_owner(spawning, || {
                root.effect(|value: i32| {
                    let _task = async value;
                });
            });
            let before_spawning = run_nurseries_allocated();
            step = 1;
            for step <= 10 {
                root.set(step);
                step += 1;
            }
            print(i"spawning={run_nurseries_allocated() - before_spawning}");
            spawning.dispose();
            boundary.dispose();
        }

        main();
        "#,
        "quiet=0 nurseries=0 value=22\nbusy=10 busy_nurseries=0\nspawning=10\n",
    );
}

#[test]
fn a142_s2_a_derive_body_releases_each_run_before_the_next_and_the_last_with_its_consumer() {
    // R14: a `derive` body's registrations belong to its run. The cleanup of run
    // `n` runs before run `n + 1` builds, and the last run's is released with the
    // memo's boundary. Red when `derive` runs its body under the boundary's owner:
    // the three cleanups all print at `dispose`, after `value 6`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, on_cleanup, run_with_owner };

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let boundary = Owner::new();
            let doubled = run_with_owner(boundary, || {
                root.derive(|value: i32| {
                    print(i"run {value}");
                    on_cleanup(|| print(i"release {value}"));
                    value * 2
                }).memo()
            });
            root.set(2);
            root.set(3);
            print(i"value {doubled.get()}");
            boundary.dispose();
            print("disposed");
        }

        main();
        "#,
        "run 1\nrelease 1\nrun 2\nrelease 2\nrun 3\nvalue 6\nrelease 3\ndisposed\n",
    );
}

#[test]
fn a142_s2_a_switch_selectors_creations_are_released_on_reselection() {
    // A selector runs once per change of its outer (the prototype's `made=1`
    // shape: reads run nothing), and what a selection built is released when
    // the next selection replaces it — before the new one builds.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, on_cleanup };
        import std::shared::Shared;

        fun main() {
            let made: Shared<i32> = Shared::new(0);
            let flag = Signal::new(true);
            let count = Signal::new(1);
            let picked = flag
                .switch(|on: bool| {
                    made.write() = made.read() + 1;
                    on_cleanup(|| print(i"released {on}"));
                    count.derive(|value| if on { value * 100 } else { 0 - value })
                })
                .memo();
            print(i"picked={picked.get()} made={made.read()}");
            count.set(2);
            print(i"picked={picked.get()} {picked.get()} made={made.read()}");
            flag.set(false);
            count.set(3);
            print(i"picked={picked.get()} made={made.read()}");
        }

        main();
        "#,
        "picked=100 made=1\npicked=200 200 made=1\nreleased true\npicked=-3 made=2\n",
    );
}

#[test]
fn a142_s2_sample_releases_what_its_bodies_created() {
    // R32: `.sample()` starts the pipe, reads it and releases it — the runs its
    // bodies made included. The cleanup runs inside the call, before the value
    // is printed; and nothing is left subscribed on the root.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, on_cleanup };

        fun main() {
            let root: SignalCell<i32> = Signal::new(4);
            let read = root
                .derive(|value: i32| {
                    on_cleanup(|| print("released the run"));
                    value + 1
                })
                .sample();
            print(i"sampled {read} subscribers={root.subscribers.read().len()}");
        }

        main();
        "#,
        "released the run\nsampled 5 subscribers=0\n",
    );
}

#[test]
fn a142_s2_a_superseded_task_is_cancelled_with_its_run() {
    // §4.1: a task started during a run belongs to the run's nursery and is
    // cancelled when the run is released. Two changes before the first task's
    // sleep ends: only the LAST run's task finishes. Red when the run owns no
    // nursery: `fetched 1` and `fetched 2` print too.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };
        import std::time::sleep;

        fun main() {
            let id = Signal::new(1);
            let boundary = Owner::new();
            run_with_owner(boundary, || {
                id.effect(|value: i32| {
                    let _pending = async {
                        sleep(30);
                        print(i"fetched {value}");
                    };
                });
            });
            id.set(2);
            id.set(3);
            sleep(120);
            boundary.dispose();
            print("done");
        }

        main();
        "#,
        "fetched 3\ndone\n",
    );
}

#[test]
fn a142_s2_a_derive_bodys_superseded_task_is_cancelled() {
    // The paper's own example: `src.derive(|x| async ..)` cancels the
    // superseded fetch when `src` changes, because the body runs once per change
    // inside the one instance its consumer started (§4.1).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };
        import std::task::Task;
        import std::time::sleep;

        fun main() {
            let id = Signal::new(1);
            let boundary = Owner::new();
            let _fetches = run_with_owner(boundary, || {
                id.derive(|value: i32| async {
                    sleep(30);
                    print(i"fetched {value}");
                    value
                }).memo()
            });
            id.set(2);
            sleep(120);
            boundary.dispose();
            print("done");
        }

        main();
        "#,
        "fetched 2\ndone\n",
    );
}

#[test]
fn a142_s2_a_cancelled_runs_task_is_owned_and_reports_nothing() {
    // The ownership half of the pin above (J7): the run's task is REGISTERED with
    // the run's nursery, so its cancellation is absorbed like every owned task's.
    // Red when a spawn in a literal born under `context ambient_nursery` is not
    // connected to the nursery its caller injects (the program calls no
    // `nursery`): the task is cancelled through the ambient signal (its sleep
    // aborts) but it is unowned, so its AbortError is reported on stderr.
    let (stdout, stderr) = compile_and_run_capturing_stderr(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };
        import std::time::sleep;

        fun main() {
            let id = Signal::new(1);
            let boundary = Owner::new();
            run_with_owner(boundary, || {
                id.effect(|value: i32| {
                    let _pending = async {
                        sleep(30);
                        print(i"fetched {value}");
                    };
                });
            });
            id.set(2);
            sleep(120);
            boundary.dispose();
            print("done");
        }

        main();
        "#,
    )
    .expect("compiles and runs");
    assert_eq!(stdout, "fetched 2\ndone\n");
    assert!(
        stderr.trim().is_empty(),
        "a cancelled owned task reports nothing: {stderr}"
    );
}

#[test]
fn j7_a_spawn_in_a_user_written_ambient_nursery_closure_is_owned_by_the_injected_nursery() {
    // J7 (native-44's divergence): a function whose body parameter is typed
    // `context ambient_nursery` establishes a detached nursery for it and cancels
    // the nursery after. The task the literal spawned belongs to that nursery, so
    // it is cancelled (neither `survived` prints) and its cancellation is
    // absorbed. Red before J7 — the program calls no `nursery`, so the spawn was
    // never connected: two "unhandled task error … AbortError" lines on stderr.
    // Both shapes: the clause alone, and beside `owner_scope` (a pipe body's).
    let (stdout, stderr) = compile_and_run_capturing_stderr(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, owner_scope };
        import std::task::{ ambient_nursery, detached_nursery };
        import std::time::sleep;

        fun single(body: (|| void) context ambient_nursery) {
            let held = detached_nursery();
            ambient_nursery.run(held, body);
            held.cancel();
        }

        fun double(body: (|| void) context (owner_scope, ambient_nursery)) {
            let held = detached_nursery();
            owner_scope.run(Owner::new(), || ambient_nursery.run(held, || body()));
            held.cancel();
        }

        fun main() {
            single(|| {
                let _task = async {
                    sleep(20);
                    print("single survived");
                };
            });
            double(|| {
                let _task = async {
                    sleep(20);
                    print("double survived");
                };
            });
            sleep(60);
            print("done");
        }

        main();
        "#,
    )
    .expect("compiles and runs");
    assert_eq!(stdout, "done\n");
    assert!(
        stderr.trim().is_empty(),
        "a cancelled owned task reports nothing: {stderr}"
    );
}

#[test]
fn j7_a_spawn_in_a_function_an_effect_body_calls_is_owned_by_the_run() {
    // The dynamic extent, not the literal: a spawn inside a helper the body CALLS
    // registers with the run's nursery too (the helper is threaded the nursery
    // like any function a `nursery` body calls), so a superseded run's task is
    // cancelled AND absorbed. Red when only a spawn written in the literal itself
    // registers: `fetched 1` stays cancelled through the signal but its
    // AbortError reaches stderr.
    let (stdout, stderr) = compile_and_run_capturing_stderr(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };
        import std::time::sleep;

        fun fetch_later(value: i32) {
            let _pending = async {
                sleep(30);
                print(i"fetched {value}");
            };
        }

        fun main() {
            let id = Signal::new(1);
            let boundary = Owner::new();
            run_with_owner(boundary, || {
                id.effect(|value: i32| fetch_later(value));
            });
            id.set(2);
            sleep(120);
            boundary.dispose();
            print("done");
        }

        main();
        "#,
    )
    .expect("compiles and runs");
    assert_eq!(stdout, "fetched 2\ndone\n");
    assert!(
        stderr.trim().is_empty(),
        "a cancelled owned task reports nothing: {stderr}"
    );
}

#[test]
fn j7_a_spawn_with_no_nursery_establishing_site_stays_free_floating() {
    // The engagement is still gated: a program that only loads `std::task` and
    // spawns (no `nursery`, no `enter`, no literal at a `context ambient_nursery`
    // position) keeps the unstructured behaviour — its failing task, never
    // awaited, reports on stderr with its origin.
    let (stdout, stderr) = compile_and_run_capturing_stderr(
        r#"
        import std::io::{ panic, print };
        import std::task::Task;
        import std::time::sleep;

        fun fail(): i32 {
            panic("boom")
        }

        fun main() {
            let _task: Task<i32> = async fail();
            sleep(20);
            print("done");
        }

        main();
        "#,
    )
    .expect("compiles and runs");
    assert_eq!(stdout, "done\n");
    assert!(
        stderr.contains("unhandled task error") && stderr.contains("boom"),
        "a free task's failure reports: {stderr}"
    );
}

#[test]
fn a142_s2_scoped_effect_is_a_deprecated_alias_of_effect() {
    // R2 merged the pair: `scoped_effect` still compiles, warns, and behaves as
    // `effect` does.
    let source = r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, on_cleanup, run_with_owner };

        fun main() {
            let id = Signal::new(1);
            let boundary = Owner::new();
            run_with_owner(boundary, || {
                id.scoped_effect(|value: i32| {
                    on_cleanup(|| print(i"release {value}"));
                });
            });
            id.set(2);
            boundary.dispose();
        }

        main();
        "#;
    assert_compiles_and_runs(source, "release 1\nrelease 2\n");
    assert!(
        warning_diagnostics(source)
            .iter()
            .any(|(message, _)| message.contains("scoped_effect")),
        "scoped_effect warns as deprecated"
    );
}

// --- F60 (R-d door (a)): a pipe dropped unconsumed WARNS ----------------------
//
// A pipe node runs nothing until a consumer starts it, so a pipe built as a bare
// statement is a mistake the type can name: every pipe node type (anything std's
// `Pipe`/`CollPipe` is implemented for, and a `dyn Pipe`) is must-use. It warns —
// a dropped value is not an error anywhere else in the language — and still
// builds on both backends (`native_differential`'s dropped-pipes probe).

const F60_UNUSED_PIPE: &str = "unused pipe: a pipe runs nothing until it is consumed";

#[test]
fn f60_a_dropped_pipe_warns_whichever_function_built_it() {
    // One warning per dropped pipe statement: a `derive`, a `distinct_by`, a
    // chain, the free `derive`, a `switch`, an `Option` `flatten`, the
    // collection pipes (`coll`, `coll().map`), a transient's `latest()` (a
    // `dyn Pipe`), and an application's own function that answers a pipe. Red
    // before F60: none of them said anything.
    let found = warnings(
        r#"
        import std::option::Option::{ self, None, Some };
        import std::reactive::{ Derive, Signal, SignalCell, Source, derive };
        import std::reactive::transient::{ Transient, TransientSource };

        fun doubled(source: SignalCell<i32>): Derive<SignalCell<i32>, i32, i32> {
            source.derive(|value| value * 2)
        }

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            count.derive(|value| value * 2);
            count.distinct_by(|value| value);
            count.derive(|value| value * 2).derive(|value| i"{value}");
            derive(|| count.track() + 1);
            let first: SignalCell<i32> = Signal::new(1);
            let outer: SignalCell<SignalCell<i32>> = Signal::new(first);
            outer.switch(|inner| inner);
            let maybe: SignalCell<Option<SignalCell<i32>>> = Signal::new(Some(first));
            maybe.flatten();
            let items: SignalCell<List<i32>> = Signal::new([1, 2, 3]);
            items.coll();
            items.coll().map(|x| x * 2);
            let fetched: Transient<i32, str> = count.derive(|x| async x).transient_global();
            fetched.latest();
            doubled(count);
        }

        main();
        "#,
    );
    let dropped = found
        .iter()
        .filter(|message| message.starts_with(F60_UNUSED_PIPE))
        .count();
    assert_eq!(dropped, 10, "{found:#?}");
}

#[test]
fn f60_a_consumed_sealed_or_discarded_pipe_does_not_warn() {
    // The controls: a sealed pipe (`.memo()` answers a cell, not a pipe), one
    // consumed by an `effect`, one bound with `let _`, one returned, a source
    // (`Signal::new`), and an application's OWN trait named `Pipe` — none warns.
    let found = warnings(
        r#"
        import std::reactive::{ Derive, Owner, Signal, SignalCell, Source, run_with_owner };

        trait Pipe {
            fun go(self): i32;
        }

        struct Mine {}

        impl Mine with Pipe {
            fun go(self): i32 { 1 }
        }

        fun make(): Mine {
            Mine {}
        }

        fun doubled(source: SignalCell<i32>): Derive<SignalCell<i32>, i32, i32> {
            source.derive(|value| value * 2)
        }

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            run_with_owner(owner, || {
                count.derive(|value| value + 1).memo();
                count.derive(|value| value + 1).effect(|value: i32| print(value));
            });
            let _ = count.derive(|value| value * 2);
            Signal::new(3);
            make();
            let _kept = doubled(count);
        }

        main();
        "#,
    );
    assert!(
        !found
            .iter()
            .any(|message| message.starts_with(F60_UNUSED_PIPE)),
        "{found:#?}"
    );
}
