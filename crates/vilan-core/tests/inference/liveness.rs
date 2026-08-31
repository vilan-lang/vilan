//! The last-use liveness dataflow and the copy elision it drives
//! (`lifetimes.md` §6, slice S2 — `analyzer/liveness.rs`).
//!
//! Each pin states the emitted-bytes consequence, because that is what the
//! change is: **no semantic change at all**, only which rule-2 copies survive.
//! A shape that must elide asserts the output carries no `__clone` (the helper
//! is emitted only when something calls it, so its absence is the whole
//! program's proof); a shape that must still copy asserts a `__clone(` call is
//! there. Where the copy is load-bearing the pin also RUNS the program and
//! reads the values back, since "still copies" is only interesting because
//! eliding it would change an answer.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- The win: a use that is the last one on every path ----------------------

#[test]
fn a_read_then_move_elides_the_move() {
    // The census's headline shape (§3 fact 3): two uses, the second of them the
    // last. `reference_count == 1` refused it for the count alone; the dataflow
    // sees that nothing reads `items` after the store and lets the enum payload
    // take the list's storage.
    let js = compile(
        r#"
        import std::io::print;
        import std::option::{ Option, Some };
        fun total(xs: List<i32>): i32 {
            mut sum = 0;
            for x in xs {
                sum += x;
            }
            sum
        }
        fun main() {
            mut items: List<i32> = [1, 2, 3];
            print(total(items));
            let boxed: Option<List<i32>> = Some(items);
            print(boxed.is_some());
        }
        "#,
    )
    .expect("the read-then-move shape compiles");
    assert!(
        !js.contains("__clone"),
        "the last use of `items` still copies:\n{js}"
    );
}

#[test]
fn a_read_then_move_still_prints_the_same_values() {
    // The elision is only sound because the source is dead: the same program,
    // run, must answer exactly as it did when the store deep-copied.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{ Option, Some };
        fun total(xs: List<i32>): i32 {
            mut sum = 0;
            for x in xs {
                sum += x;
            }
            sum
        }
        fun main() {
            mut items: List<i32> = [1, 2, 3];
            print(total(items));
            let boxed: Option<List<i32>> = Some(items);
            print(boxed.is_some());
        }
        "#,
        "6\ntrue\n",
    );
}

#[test]
fn a_binding_the_loop_body_declares_elides_at_its_last_use() {
    // The half `collect_repeatable_interiors` could not see: the store IS
    // lexically inside a loop, but `row` is declared inside the body too, so
    // every iteration builds a fresh list and the elided store hands each one
    // away whole. The loop rule asks the question relative to the DECLARATION.
    let js = compile(
        r#"
        import std::io::print;
        fun main() {
            mut fresh: List<List<i32>> = [];
            let rounds: List<i32> = [0, 1, 2];
            for round in rounds {
                mut row: List<i32> = [round];
                fresh.push(row);
            }
            print(fresh.len());
        }
        "#,
    )
    .expect("the loop-local shape compiles");
    assert!(
        !js.contains("__clone"),
        "a binding the loop body declares still copies at its last use:\n{js}"
    );
}

#[test]
fn loop_local_rows_stay_independent_when_the_copy_is_elided() {
    // The elision must not make the three pushed rows one shared list: `row` is
    // re-declared per iteration, so each `push` donates a different array.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut fresh: List<List<i32>> = [];
            let rounds: List<i32> = [0, 1, 2];
            for round in rounds {
                mut row: List<i32> = [round];
                row.push(round + 10);
                fresh.push(row);
            }
            for stored in fresh {
                print(stored.len());
            }
            print(fresh[0][0]);
            print(fresh[2][0]);
        }
        "#,
        "2\n2\n2\n0\n2\n",
    );
}

#[test]
fn a_generic_own_store_elides_at_the_last_use() {
    // The `UnlessResource` machinery's own path: `keep`'s `own item: T` is a
    // store position whose copy is decided per instantiation. At the argument's
    // last use there is nothing to decide — the caller's storage is donated,
    // exactly as `List::push` already did for a once-read local. The callee's
    // own body still copies its PARAMETER into `held` (a parameter aliases the
    // caller's value and is never elided), so the assertion names the argument
    // rather than sweeping the whole program.
    let js = compile(
        r#"
        import std::io::print;
        struct Vault<T> { held: List<T> }
        impl Vault<type T> {
            fun keep(&mut self, own item: T) {
                self.held.push(item);
            }
        }
        fun main() {
            mut vault: Vault<List<i32>> = Vault { held = [] };
            mut payload: List<i32> = [1, 2];
            print(payload.len());
            vault.keep(payload);
            print(vault.held.len());
        }
        "#,
    )
    .expect("the generic store shape compiles");
    assert!(
        !js.contains("__clone(payload)"),
        "a generic `own` store still copies at the argument's last use:\n{js}"
    );
}

// --- The refusals: shapes that must still copy ------------------------------

#[test]
fn a_read_inside_a_loop_of_an_outer_binding_still_copies() {
    // Live across the back edge: `base` is read on every iteration, so the
    // store can never be its last use and the alias would survive into the
    // next one. This is what `collect_repeatable_interiors` was approximating.
    let js = compile(
        r#"
        import std::io::print;
        fun main() {
            mut base: List<i32> = [1, 2];
            mut collected: List<List<i32>> = [];
            let rounds: List<i32> = [0, 1, 2];
            for round in rounds {
                collected.push(base);
            }
            print(collected.len());
        }
        "#,
    )
    .expect("the loop shape compiles");
    assert!(
        js.contains("__clone("),
        "a read carried across the back edge was elided:\n{js}"
    );
}

#[test]
fn loop_carried_rows_stay_independent_of_the_source() {
    // Why the refusal above is load-bearing: with the copy gone, all three
    // stored rows would BE `base`, and the write after the loop would show
    // through every one of them.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            mut base: List<i32> = [1, 2];
            mut collected: List<List<i32>> = [];
            let rounds: List<i32> = [0, 1, 2];
            for round in rounds {
                collected.push(base);
            }
            base.push(9);
            print(base.len());
            print(collected[0].len());
            print(collected[2].len());
        }
        "#,
        "3\n2\n2\n",
    );
}

#[test]
fn a_captured_binding_never_elides() {
    // §4's capture rule: a closure captures the BINDING, so the read inside it
    // is a read of the same binding — from another region, which the dataflow
    // refuses wholesale rather than trying to time.
    let js = compile(
        r#"
        import std::io::print;
        import std::option::{ Option, Some };
        fun main() {
            mut captured: List<i32> = [7];
            let show = || print(captured.len());
            let held: Option<List<i32>> = Some(captured);
            show();
            print(held.is_some());
        }
        "#,
    )
    .expect("the capture shape compiles");
    assert!(
        js.contains("__clone("),
        "a binding a closure captures was elided:\n{js}"
    );
}

#[test]
fn a_captured_binding_and_its_stored_copy_stay_independent() {
    // The refusal's observable half: the closure keeps reading the binding, so
    // the stored value must not be the binding's own list.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{ Option, Some, None };
        fun main() {
            mut captured: List<i32> = [7];
            let show = || print(captured.len());
            let held: Option<List<i32>> = Some(captured);
            captured.push(8);
            show();
            match held {
                Some(let inner) => print(inner.len()),
                None => print(0),
            }
        }
        "#,
        "2\n1\n",
    );
}

#[test]
fn a_module_level_binding_read_by_a_function_never_elides() {
    // A module body is its own region. A function reading a module-level
    // binding reads it from a region that is not the declaring one, and the
    // module body's own last read is therefore not the program's last read.
    let js = compile(
        r#"
        import std::io::print;
        import std::option::{ Option, Some };
        let table: List<i32> = [1, 2, 3];
        fun size(): i32 { table.len() }
        let snapshot: Option<List<i32>> = Some(table);
        fun main() {
            print(size());
            print(snapshot.is_some());
        }
        "#,
    )
    .expect("the module-level shape compiles");
    assert!(
        js.contains("__clone("),
        "a module-level binding a function reads was elided:\n{js}"
    );
}

// --- The loan-extension rule (§6.1) -----------------------------------------

#[test]
fn a_move_past_a_dead_view_elides() {
    // The view's last read comes BEFORE the store, so the extension rule adds
    // nothing after it: the owner is dead at the store and donates its storage.
    let js = compile(
        r#"
        import std::io::print;
        import std::option::{ Option, Some };
        struct Box { items: List<i32> }
        fun main() {
            mut early = Box { items = [1] };
            let seen = &early;
            print(seen.items.len());
            let kept: Option<Box> = Some(early);
            print(kept.is_some());
        }
        "#,
    )
    .expect("the dead-view shape compiles");
    assert!(
        !js.contains("__clone"),
        "the owner still copies although its view was already dead:\n{js}"
    );
}

#[test]
fn a_move_under_a_live_view_still_copies() {
    // §6.1: a `borrows` projection extends its owner's last use to the last use
    // of the VIEW. `watch` is read after the store, so `late` is live there and
    // the copy stands — the one shape a value-only last-use rule gets wrong.
    let js = compile(
        r#"
        import std::io::print;
        import std::option::{ Option, Some };
        struct Box { items: List<i32> }
        fun main() {
            mut late = Box { items = [2] };
            let watch = &late;
            let stored: Option<Box> = Some(late);
            print(watch.items.len());
            print(stored.is_some());
        }
        "#,
    )
    .expect("the live-view shape compiles");
    assert!(
        js.contains("__clone("),
        "the owner was elided out from under a still-live view:\n{js}"
    );
}

#[test]
fn a_loan_to_a_resolved_callee_is_call_bounded() {
    // §6.4's rule as it stands before S4: an ordinary `&` argument to a callee
    // whose signature the analyzer read is a loan for the duration of the call
    // and nothing more, so it does not keep the owner alive past it.
    let js = compile(
        r#"
        import std::io::print;
        import std::option::{ Option, Some };
        struct Box { items: List<i32> }
        fun peek(view: &Box): i32 { view.items.len() }
        fun main() {
            mut subject = Box { items = [1, 2] };
            print(peek(&subject));
            let kept: Option<Box> = Some(subject);
            print(kept.is_some());
        }
        "#,
    )
    .expect("the call-bounded loan shape compiles");
    assert!(
        !js.contains("__clone"),
        "a call-bounded loan kept its owner alive past the call:\n{js}"
    );
}

// --- The move checker sees an elided copy as the move it is -----------------

#[test]
fn an_elided_own_argument_is_still_a_move_to_the_checker() {
    // Elision and the affine rules are answered by different passes, and only
    // one of them moved: `compute_clone_sites` never made a resource a copy
    // site in the first place (R1), so widening rule 2 cannot loosen R3. The
    // use after the `own` argument is still rejected, with the same message.
    assert_fails_with(
        r#"
        import std::io::print;
        resource struct Session { id: i32 }
        fun take(own s: Session): i32 { s.id }
        fun main() {
            let session = Session { id = 3 };
            print(take(session));
            print(session.id);
        }
        "#,
        "use of `session` after it was moved",
    );
}

#[test]
fn a_resource_beside_an_elided_copy_still_runs_its_single_owner() {
    // The mixed shape: a resource moved at its last use, and an ordinary
    // aggregate elided at its own, in one body. Both are moves; only the second
    // one ever emitted a `__clone` to remove.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        resource struct Session { id: i32 }
        fun take(own s: Session): i32 { s.id }
        struct Vault { held: List<List<i32>> }
        fun main() {
            let session = Session { id = 3 };
            mut vault = Vault { held = [] };
            mut payload: List<i32> = [1, 2];
            print(payload.len());
            vault.held.push(payload);
            print(take(session));
            print(vault.held.len());
        }
        "#,
        "2\n3\n1\n",
    );
}
