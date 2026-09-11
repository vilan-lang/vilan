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
