//! A142 S6 — tracked reads (`proposal/reactive-layers.md` §7).
//!
//! Tracking is a context, `std::reactive::tracking`. `Source::track()` reads
//! the source AND registers it with the scope of the body that is running,
//! through the STRICT `Context::get`: a `track()` no enclosing body establishes
//! is a compile-time coverage error, never a runtime absence. Every body a pipe
//! runs opens a scope — a `derive` transform, a `switch`/`switch_some`/
//! `and_then` selector, an `effect` body, and the free `derive(|| body)`, a pipe
//! whose only dependencies are the ones it tracks. After each run the stage
//! compares the run's reads with the last run's (reusing edges in order),
//! subscribes to what is new, releases what was dropped, and wakes through a
//! DERIVATION relay, so the turn's two phases keep it glitch-free.

use crate::support::*;

#[test]
fn a142_s6_a_dynamic_dependency_follows_the_branch_it_read_last() {
    // A free `derive` whose body tracks `a` or `b` depending on `flag`: it follows
    // whichever it read on its LAST run. While the flag is up a write to `a`
    // re-runs it and a write to `b` does not; flipped, the other way round.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, derive, run_with_owner };

        fun main() {
            let flag: SignalCell<bool> = Signal::new(true);
            let a: SignalCell<i32> = Signal::new(1);
            let b: SignalCell<i32> = Signal::new(100);
            let boundary = Owner::new();
            run_with_owner(boundary, || {
                derive(|| if flag.track() { a.track() } else { b.track() })
                    .effect(|value: i32| print(i"saw {value}"));
            });
            a.set(2);
            b.set(200);
            flag.set(false);
            a.set(3);
            b.set(300);
            boundary.dispose();
            b.set(400);
        }

        main();
        "#,
        "saw 1\nsaw 2\nsaw 200\nsaw 300\n",
    );
}

#[test]
fn a142_s6_a_dependency_a_branch_dropped_stops_waking_its_body() {
    // The body's run count is the instrument: once the branch that read `a` is
    // not taken, ten writes to `a` run the body zero times. Red when a dropped
    // dependency keeps its edge: `runs=13`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, derive };
        import std::shared::Shared;

        fun main() {
            let flag: SignalCell<bool> = Signal::new(true);
            let a: SignalCell<i32> = Signal::new(1);
            let runs: Shared<i32> = Shared::new(0);
            let sealed = derive(|| {
                runs.write() = runs.read() + 1;
                if flag.track() { a.track() } else { 0 }
            }).memo();
            a.set(2);
            flag.set(false);
            mut step = 0;
            for step < 10 {
                a.set(step);
                step += 1;
            }
            print(i"runs={runs.read()} value={sealed.get()}");
        }

        main();
        "#,
        "runs=3 value=0\n",
    );
}

#[test]
fn a142_s6_a_derive_stage_follows_a_tracked_read_beside_its_input() {
    // §7.2: `count.derive(|c| c + other.track())` follows `other` too — the
    // stage's input structurally, and the tracked read through its scope.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let other: SignalCell<i32> = Signal::new(10);
            let total = count.derive(|c: i32| c + other.track()).memo();
            print(total.get());
            other.set(20);
            print(total.get());
            count.set(2);
            print(total.get());
        }

        main();
        "#,
        "11\n21\n22\n",
    );
}

#[test]
fn a142_s6_a_switch_selector_reselects_when_a_tracked_read_changes() {
    // A selector's tracked read is a dependency of the SELECTION: when it
    // changes, the stage re-selects (and releases what the last selection
    // built), exactly as it does when its outer changes.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, on_cleanup };
        import std::shared::Shared;

        fun main() {
            let outer: SignalCell<i32> = Signal::new(1);
            let scale: SignalCell<i32> = Signal::new(10);
            let inner: SignalCell<i32> = Signal::new(5);
            let made: Shared<i32> = Shared::new(0);
            let picked = outer
                .switch(|base: i32| {
                    made.write() = made.read() + 1;
                    let factor = scale.track();
                    on_cleanup(|| print(i"released {base}x{factor}"));
                    inner.derive(|value| base + value * factor)
                })
                .memo();
            print(i"{picked.get()} made={made.read()}");
            inner.set(6);
            print(i"{picked.get()} made={made.read()}");
            scale.set(100);
            print(i"{picked.get()} made={made.read()}");
        }

        main();
        "#,
        "51 made=1\n61 made=1\nreleased 1x10\n601 made=2\n",
    );
}

#[test]
fn a142_s6_an_effect_reruns_when_a_tracked_read_changes_with_its_latest_input() {
    // An `effect` body opens a scope: a change to what it tracked re-runs it,
    // with the value its input delivered last. Its per-run owner still releases
    // the previous run first.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, on_cleanup, run_with_owner };

        fun main() {
            let input: SignalCell<i32> = Signal::new(1);
            let label: SignalCell<str> = Signal::new("a");
            let boundary = Owner::new();
            run_with_owner(boundary, || {
                input.effect(|value: i32| {
                    let name = label.track();
                    print(i"run {name}{value}");
                    on_cleanup(|| print(i"release {name}{value}"));
                });
            });
            label.set("b");
            input.set(2);
            boundary.dispose();
            label.set("c");
            print("done");
        }

        main();
        "#,
        "run a1\nrelease a1\nrun b1\nrelease b1\nrun b2\nrelease b2\ndone\n",
    );
}

#[test]
fn a142_s6_a_glitch_free_diamond_runs_once_per_change() {
    // Two sealed arms over one root, read by a tracked body. In a turn the arms
    // and the tracked edges are derivations (phase 1) and the effect runs once,
    // on the settled pair — for the free `derive`, for a `derive` stage whose
    // input is the root itself, and for an effect whose body tracks an arm. Red
    // when a tracked edge wakes its consumer as an effect, or the effect's
    // re-run and its input's wake are two entries: two runs, the first torn.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{
            FlushPolicy, Owner, Signal, SignalCell, Source, derive, run_with_owner, turn,
        };
        import std::shared::Shared;

        fun main() {
            let root: SignalCell<i32> = Signal::new(1);
            let left = root.derive(|x| x * 10).memo();
            let right = root.derive(|x| x + 100).memo();
            let seen: Shared<str> = Shared::new("");
            let boundary = Owner::new();
            run_with_owner(boundary, || {
                derive(|| (left.track(), right.track())).effect(|pair: (i32, i32)| {
                    let (l, r) = pair;
                    seen.write() = i"{seen.read()} free({l},{r})";
                });
                root.derive(|x: i32| x + right.track()).effect(|sum: i32| {
                    seen.write() = i"{seen.read()} stage({sum})";
                });
                root.effect(|x: i32| {
                    let l = left.track();
                    seen.write() = i"{seen.read()} effect({x},{l})";
                });
            });
            seen.write() = "";
            turn(FlushPolicy::AtEnd, || {
                root.set(3);
            });
            print(seen.read());
            boundary.dispose();
        }

        main();
        "#,
        " free(30,103) stage(106) effect(3,30)\n",
    );
}

#[test]
fn a142_s6_a_callback_minted_in_a_scope_does_not_register_later() {
    // A closure created inside a run captures that run's scope (spec §8.4). The
    // scope closes when the run returns, so a `track()` through it later —
    // here from inside the body's NEXT run — registers nothing: writes to
    // `late` never re-run the body. Red when a scope registers into whatever run
    // its stage is in: `runs=4` (the stale read became a dependency).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, derive };
        import std::shared::Shared;

        fun main() {
            let trigger: SignalCell<i32> = Signal::new(0);
            let late: SignalCell<i32> = Signal::new(0);
            let runs: Shared<i32> = Shared::new(0);
            let stash: Shared<List<|| i32>> = Shared::new([]);
            let sealed = derive(|| {
                runs.write() = runs.read() + 1;
                for callback in stash.read() {
                    let _ignored = callback();
                }
                stash.write().push(|| late.track());
                trigger.track()
            }).memo();
            trigger.set(1);
            late.set(1);
            late.set(2);
            print(i"runs={runs.read()} value={sealed.get()}");
        }

        main();
        "#,
        "runs=2 value=1\n",
    );
}

#[test]
fn a142_s6_clear_inside_a_free_derive_is_untrack() {
    // `tracking.clear(..)` is the documented `untrack` (B458): a closure minted
    // inside it holds no scope, so a helper it calls later reads the context as
    // absent, and a read inside it registers nothing.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ None, Some };
        import std::reactive::{ Signal, SignalCell, Source, derive, tracking };
        import std::shared::Shared;

        fun scoped(): str {
            match tracking.get_safe() {
                Some(let _scope) => "scoped",
                None => "unscoped",
            }
        }

        fun main() {
            let followed: SignalCell<i32> = Signal::new(1);
            let ignored: SignalCell<i32> = Signal::new(10);
            let runs: Shared<i32> = Shared::new(0);
            let stash: Shared<List<|| str>> = Shared::new([]);
            let sealed = derive(|| {
                runs.write() = runs.read() + 1;
                let inside = scoped();
                let cleared = tracking.clear(|| {
                    stash.write().push(|| scoped());
                    ignored.get()
                });
                print(i"{inside} {stash.read()[0]()}");
                followed.track() + cleared
            }).memo();
            ignored.set(20);
            followed.set(2);
            print(i"runs={runs.read()} value={sealed.get()}");
        }

        main();
        "#,
        "scoped unscoped\nscoped unscoped\nruns=2 value=22\n",
    );
}

#[test]
fn a142_s6_a_track_inside_clear_is_the_coverage_refusal_naming_the_clear() {
    // Inside a free `derive`, `clear` takes the scope away for its body: a
    // `track()` there is refused exactly as one outside every body is, and the
    // refusal's trace names the `clear` that made the body uncovered.
    let diagnostics = failure_diagnostics_with_trace(
        r#"
        import std::reactive::{ Signal, SignalCell, Source, derive, tracking };

        fun main() {
            let hidden: SignalCell<i32> = Signal::new(1);
            let _sealed = derive(|| tracking.clear(|| hidden.track())).memo();
        }

        main();
        "#,
    );
    assert!(
        diagnostics.iter().any(|(message, _, trace)| {
            message.contains("context `tracking` is read here")
                && trace.iter().any(|(label, _, _)| {
                    label.contains(
                        "`tracking.clear(..)` runs this body with the context not established",
                    )
                })
        }),
        "{diagnostics:#?}"
    );
}

#[test]
fn a142_s6_a_track_outside_every_scope_is_the_coverage_refusal() {
    // R6: `track()` reads the scope through the STRICT `get`, so a read no body
    // establishes is refused at compile time, never answered at run time.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            print(count.track());
        }

        main();
        "#,
        "can be reached without an enclosing `run`",
    );
}

#[test]
fn a142_s6_a_callback_positions_track_is_refused_when_minted_outside_a_scope() {
    // An `on_change` observer is a callback, not a body: it opens no scope, and
    // since B482's std half it is CALLED with `tracking` cleared, so a `track()`
    // in it is refused as a read of a cleared context — wherever the literal is
    // written (the coverage refusal said "reached without an enclosing `run`").
    assert_fails_with(
        r#"
        import std::reactive::{ Owner, Signal, SignalCell, Source };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let other: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            owner.take(count.on_change(|_value| {
                let _read = other.track();
            }));
        }

        main();
        "#,
        "this closure is called with `tracking` CLEARED: it lands at the parameter `observer` of `on_change`",
    );
}

#[test]
fn a142_s6_the_free_derive_is_a_pipe_sealed_once() {
    assert_fails_with(
        r#"
        import std::reactive::{ Signal, SignalCell, Source, derive };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let doubled = derive(|| count.track() * 2);
            let _first = doubled.memo();
            let _second = doubled.memo();
        }

        main();
        "#,
        "after it was moved",
    );
}

#[test]
fn a142_s6_the_free_derive_has_no_get() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Source, derive };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            print(derive(|| count.track() * 2).get());
        }

        main();
        "#,
        "has no method 'get'",
    );
}

#[test]
fn a142_s6_on_change_over_a_free_derive_hears_its_first_change() {
    // `on_change` runs its observer only on a change — but the body has to have
    // run for a tracked read to be a dependency at all, so a consumer that does
    // not read at once PRIMES the pipe (runs its bodies, discards the value).
    // Red without the prime: nothing is ever heard.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, derive };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            owner.take(derive(|| count.track() * 2).on_change(|value: i32| print(i"heard {value}")));
            count.set(2);
            count.set(3);
            owner.dispose();
            count.set(4);
        }

        main();
        "#,
        "heard 4\nheard 6\n",
    );
}

#[test]
fn a142_s6_an_edge_to_the_same_source_is_reused_across_runs() {
    // "Reusing edges in order": a body that reads the same cell on every run
    // keeps ONE registration on it, however many times it runs. The instrument
    // is a source of the application's own that counts its attaches and live
    // registrations. A source that cannot name its identity is re-attached per
    // run instead, the previous edge released — still one live registration.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, None, Some };
        import std::reactive::{
            Signal, SignalCell, Source, Subscriber, Subscription, derive,
        };
        import std::shared::Shared;

        struct Counted {
            cell: SignalCell<i32>,
            attaches: Shared<i32>,
            live: Shared<i32>,
            named: bool,
        }

        impl Counted with Source<i32> {
            fun get(self): i32 {
                self.cell.get()
            }

            fun on_settle(self, subscriber: Subscriber): Subscription {
                self.attaches.write() = self.attaches.read() + 1;
                self.live.write() = self.live.read() + 1;
                let handle = self.cell.on_settle(subscriber);
                let live = self.live;
                let previous = handle.release.read();
                handle.release.write() = Some(|| {
                    live.write() = live.read() - 1;
                    match previous {
                        Some(let release) => release(),
                        None => {},
                    }
                });
                handle
            }

            fun identity(self): Option<i32> {
                if self.named { self.cell.identity() } else { None }
            }
        }

        fun counted(named: bool): Counted {
            Counted {
                cell = Signal::new(0),
                attaches = Shared::new(0),
                live = Shared::new(0),
                named,
            }
        }

        fun main() {
            let named = counted(true);
            let anonymous = counted(false);
            let trigger: SignalCell<i32> = Signal::new(0);
            let sealed = derive(|| trigger.track() + named.track() + anonymous.track()).memo();
            mut step = 1;
            for step <= 5 {
                trigger.set(step);
                step += 1;
            }
            print(i"named attaches={named.attaches.read()} live={named.live.read()}");
            print(i"anonymous attaches={anonymous.attaches.read()} live={anonymous.live.read()}");
            print(sealed.get());
        }

        main();
        "#,
        "named attaches=1 live=1\nanonymous attaches=6 live=1\n5\n",
    );
}

#[test]
fn a142_s6_a_dependency_that_notifies_as_it_is_attached_is_read_again() {
    // A source may deliver a value INSIDE its attach — a mirror's seed over an
    // in-process transport. When the edge a run's read makes does that, the
    // value the run computed is already stale: the stage runs again and hands
    // back the fresh one, rather than waking its consumer inline and then
    // returning the stale value on top of it. Red when the wake goes out
    // inline: the effect's last word is the stale `saw 0`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, None, Some };
        import std::reactive::{
            Owner, Signal, SignalCell, Source, Subscriber, Subscription, derive, run_with_owner,
        };
        import std::shared::Shared;

        struct Seeding {
            cell: SignalCell<i32>,
            seeded: Shared<bool>,
        }

        impl Seeding with Source<i32> {
            fun get(self): i32 {
                self.cell.get()
            }

            fun on_settle(self, subscriber: Subscriber): Subscription {
                let handle = self.cell.on_settle(subscriber);
                if !self.seeded.read() {
                    self.seeded.write() = true;
                    self.cell.set(42);
                }
                handle
            }
        }

        fun main() {
            let source = Seeding { cell = Signal::new(0), seeded = Shared::new(false) };
            let owner = Owner::new();
            run_with_owner(owner, || {
                derive(|| source.track()).effect(|value: i32| print(i"saw {value}"));
            });
            owner.dispose();
        }

        main();
        "#,
        "saw 42\n",
    );
}

#[test]
fn m93_a_body_that_never_tracks_allocates_no_tracker() {
    // `trackers_allocated` counts every stage that made its dependency lists.
    // A sealed two-stage chain, a switch and an effect whose bodies never call
    // `track()` — built, then changed ten times — make NONE (`quiet=0`); a
    // `derive` stage, an effect and a free `derive` that track make one each
    // when they first track, and ten more changes make no more (`tracking=3
    // after=0`). The effect's tracked re-run still runs with its latest input.
    // Red when every stage instance makes its lists at `start` (before M93):
    // `quiet=5` (the effect, the selector, the stage it builds, the two derives).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{
            Owner, Signal, SignalCell, Source, derive, run_with_owner, trackers_allocated,
        };

        fun main() {
            let root: SignalCell<i32> = Signal::new(0);
            let other: SignalCell<i32> = Signal::new(100);
            let flag: SignalCell<bool> = Signal::new(true);
            let before = trackers_allocated();
            let boundary = Owner::new();
            let sealed = run_with_owner(boundary, || {
                root.effect(|value: i32| {
                    let _ignored = value + 1;
                });
                flag.switch(|on: bool| root.derive(|value| if on { value } else { 0 })).memo();
                root.derive(|value| value + 1).derive(|value| value * 2).memo()
            });
            mut step = 1;
            for step <= 10 {
                root.set(step);
                step += 1;
            }
            print(i"quiet={trackers_allocated() - before} value={sealed.get()}");
            let tracked_before = trackers_allocated();
            let tracking = Owner::new();
            let summed = run_with_owner(tracking, || {
                root.effect(|value: i32| print(i"effect {value} {other.track()}"));
                let _free = derive(|| root.track() + other.track()).memo();
                root.derive(|value| value + other.track()).memo()
            });
            let tracked_after = trackers_allocated();
            other.set(200);
            root.set(11);
            print(i"tracking={tracked_after - tracked_before} after={trackers_allocated() - tracked_after} summed={summed.get()}");
            tracking.dispose();
            boundary.dispose();
        }

        main();
        "#,
        "quiet=0 value=22\neffect 10 100\neffect 10 200\neffect 11 200\ntracking=3 after=0 summed=211\n",
    );
}

#[test]
fn a146_a_tracked_list_keyed_or_mirror_read_keeps_one_edge_across_runs() {
    // A146: `ListCell`, `KeyedCell`, `RemoteSource` and `KeyedSource` name their
    // identity (their value cell's), so a body that tracks one on every run keeps
    // ONE edge on it — for a mirror, one lease, no re-attach per run. The
    // instrument wraps each source, counting the edges a tracked read attaches
    // and answering the wrapped source's identity. Red before A146 (the default
    // `None`): every wrapper reads `attaches=7`, one per run. The same program
    // runs natively in `native_differential`'s
    // `an_in_process_mirror_program_is_identical_on_both_backends` (F74).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, None, Some };
        import std::reactive::{ Signal, SignalCell, Source, Subscriber, Subscription, derive };
        import std::reactive::delta::ListCell;
        import std::rpc::{ KeyedCell, Keyed, KeyedSource, ReactiveClient, ReactiveServer, RemoteSource, duplex_pair };
        import std::json::json_codec;
        import std::shared::Shared;

        // Counts the edges a tracked read attaches to the wrapped source, and answers the
        // wrapped source's identity.
        struct Counted<S> {
            inner: S,
            attaches: Shared<i32>,
        }

        impl Counted<type S: Source<type T>> with Source<T> {
            fun get(self): T {
                self.inner.get()
            }

            fun on_settle(self, subscriber: Subscriber): Subscription {
                self.attaches.write() = self.attaches.read() + 1;
                self.inner.on_settle(subscriber)
            }

            fun identity(self): Option<i32> {
                self.inner.identity()
            }
        }

        fun counted<S>(inner: S): Counted<S> {
            Counted { inner, attaches = Shared::new(0) }
        }

        [derive(Wire)]
        struct Row {
            id: str,
            n: i32,
        }

        impl Row with Keyed<str> {
            fun key(self): str {
                self.id
            }
        }

        fun main() {
            let list = counted(ListCell::of([1, 2]));
            let keyed: Counted<KeyedCell<str, Row>> = counted(KeyedCell::new([Row { id = "a", n = 1 }]));
            let status = Signal::new(7);
            let (client_end, server_end) = duplex_pair();
            let server = ReactiveServer::new(server_end, json_codec());
            let channel = server.expose(status);
            let rows: KeyedCell<str, Row> = KeyedCell::new([Row { id = "b", n = 2 }]);
            let keyed_channel = server.expose_keyed_cell(rows);
            let client = ReactiveClient::new(client_end, json_codec());
            let remote: RemoteSource<i32> = client.source(channel);
            let mirror = counted(remote);
            let remote_rows: KeyedSource<str, Row> = client.attached_keyed_source(keyed_channel);
            let keyed_mirror = counted(remote_rows);
            let trigger: SignalCell<i32> = Signal::new(0);
            let sealed = derive(|| {
                let value = match mirror.track() {
                    Some(let n) => n,
                    None => 0,
                };
                let remote_count = match keyed_mirror.track() {
                    Some(let found) => found.len(),
                    None => 0usize,
                };
                i"{trigger.track()} {list.track().len()} {keyed.track().len()} {value} {remote_count}"
            }).memo();
            mut step = 1;
            for step <= 5 {
                trigger.set(step);
                step += 1;
            }
            print(i"list attaches={list.attaches.read()}");
            print(i"keyed cell attaches={keyed.attaches.read()}");
            print(i"mirror attaches={mirror.attaches.read()}");
            print(i"keyed mirror attaches={keyed_mirror.attaches.read()}");
            print(sealed.get());
        }

        main();
        "#,
        "list attaches=1\nkeyed cell attaches=1\nmirror attaches=1\nkeyed mirror attaches=1\n5 2 1 7 1\n",
    );
}

// --- B482's std half (R-g): the base callback positions run CLEARED ----------
//
// `on_change`, `sub`, `effect_on_change` and the UI event handlers take their
// callback `context tracking` and call it under `tracking.clear(..)` (A142 §7.3).
// A callback minted INSIDE a body — where `tracking` is in scope — therefore
// reads it as absent, and a `track()` in it is refused at compile time.

#[test]
fn b482_a_track_in_a_callback_written_inside_a_body_is_refused() {
    // Each position, written inside an effect body (the scope a `track()` there
    // would otherwise have seen). Red before the std half: all three compiled,
    // and the read was a `get()` registered with nothing (the epoch guard).
    for (position, call) in [
        (
            "on_change",
            "let _held = other.on_change(|_value| { let _read = late.track(); });",
        ),
        (
            "sub",
            "let _held = other.sub(|_value| { let _read = late.track(); });",
        ),
        (
            "effect_on_change",
            "other.effect_on_change(|_value| { let _read = late.track(); });",
        ),
    ] {
        let source = format!(
            r#"
        import std::reactive::{{ Owner, Signal, SignalCell, Source, run_with_owner }};

        fun main() {{
            let count: SignalCell<i32> = Signal::new(1);
            let other: SignalCell<i32> = Signal::new(1);
            let late: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            run_with_owner(owner, || {{
                count.effect(|_value: i32| {{
                    {call}
                }});
            }});
        }}

        main();
        "#
        );
        let parameter = if position == "effect_on_change" {
            "body"
        } else {
            "observer"
        };
        assert_fails_with(
            &source,
            &format!(
                "this closure is called with `tracking` CLEARED: it lands at the parameter `{parameter}` of `{position}`"
            ),
        );
    }
}

#[test]
fn b482_a_callbacks_safe_read_of_tracking_is_none() {
    // The same three positions, minted inside an effect body, read `tracking`
    // safely: `None` in the callback, `Some` in the body that minted it. Red
    // before the std half: the callbacks read the body's captured scope,
    // `Some` (`true` printed as `false`).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner, tracking };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let other: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            run_with_owner(owner, || {
                count.effect(|_value: i32| {
                    print(i"body {tracking.get_safe().is_some()}");
                    let _changed = other.on_change(|_value| print(i"on_change {tracking.get_safe().is_none()}"));
                    let _subbed = other.sub(|_value| print(i"sub {tracking.get_safe().is_none()}"));
                    other.effect_on_change(|_value| print(i"effect_on_change {tracking.get_safe().is_none()}"));
                });
            });
            other.set(2);
            owner.dispose();
        }

        main();
        "#,
        "body true\nsub true\non_change true\nsub true\neffect_on_change true\n",
    );
}

#[test]
fn b482_a_callback_value_typed_without_the_clause_is_refused_and_both_migrations_compile() {
    // BREAKING (R-g): a callback VALUE whose type carries no `context tracking`
    // cannot land at a callback position — the threading cannot clear it. The
    // two spellings that do: wrap it in a literal, or type it with the clause.
    let refused = r#"
        import std::reactive::{ Owner, Signal, SignalCell, Source };

        fun watch(cell: SignalCell<i32>, react: |i32| void) {
            let owner = Owner::new();
            owner.take(cell.on_change(react));
        }

        fun main() {
            watch(Signal::new(1), |n| print(n));
        }

        main();
        "#;
    assert_fails_with(
        refused,
        "a `context`-typed parameter takes a closure literal, a named function or variant, a value with the same `context` clause",
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Owner, Signal, SignalCell, Source, tracking };

        fun wrapped(cell: SignalCell<i32>, react: |i32| void): Owner {
            let owner = Owner::new();
            owner.take(cell.on_change(|value| react(value)));
            owner
        }

        fun typed(cell: SignalCell<i32>, react: (|i32| void) context tracking): Owner {
            let owner = Owner::new();
            owner.take(cell.on_change(react));
            owner
        }

        fun main() {
            let cell: SignalCell<i32> = Signal::new(1);
            let first = wrapped(cell, |n| print(i"wrapped {n}"));
            let second = typed(cell, |n| print(i"typed {n}"));
            cell.set(2);
            first.dispose();
            second.dispose();
        }

        main();
        "#,
        "wrapped 2\ntyped 2\n",
    );
}

#[test]
fn b482_a_track_in_a_ui_event_handler_is_refused() {
    // The UI event handlers are callback positions too (`on`, `on_event`): a
    // handler minted inside a body runs CLEARED in the browser.
    let source = r#"
        import std::reactive::{ Owner, Signal, SignalCell, Source, run_with_owner };
        import std::web::ui::{ View, view };

        fun main() {
            let count: SignalCell<i32> = Signal::new(1);
            let late: SignalCell<i32> = Signal::new(1);
            let owner = Owner::new();
            run_with_owner(owner, || {
                count.effect(|_value: i32| {
                    let _view: View = view("button").on("click", || {
                        let _read = late.track();
                    });
                });
            });
        }

        main();
        "#;
    let message = "this closure is called with `tracking` CLEARED: it lands at the parameter `handler` of `on`";
    assert_fails_browser_with(source, message);
    // The server's `std::web::ui` DISCARDS a handler (it is never called), so its
    // clause is only the signature's: the process leg compiles, and a fullstack
    // build is refused by its browser leg.
    assert_compiles(source);
}
