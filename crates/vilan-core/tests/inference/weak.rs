//! `Weak<T>` (C1, built as C14 S2): the cell's other handle — one that names a
//! `Shared` cell without keeping it alive.
//!
//! Three signatures, specified in `destruction.md` §10 and
//! `claims-and-epochs.md` §5a: `Shared::downgrade(&self): Weak<T>`,
//! `Weak::upgrade(self): Option<Shared<T>>`, and
//! `Weak::get(&self): Option<&T> borrows self` — the scoped, non-retaining twin
//! of `upgrade`, and the same verb and shape as `Arena::get`.
//!
//! **What lands here is the SHAPE, not the guarantee.** On the JS backend
//! nothing counts: `Shared` is a `{ v }` object the host collector reclaims, so
//! there is no release event and nothing can say a cell is gone. `downgrade` is
//! the identity, `upgrade` is always `Some`, `get` is always `Some`. The
//! deterministic `None` arrives with the counted representation on the native
//! backend (C14 S4). What these pins hold is everything that is decidable
//! today: the types, the `Option` on the way back, that a weak handle names the
//! SAME cell rather than a copy of it, and that `get`'s payload is a real
//! second-class view rather than a value wearing an ampersand.
//!
//! One subject module of the `inference` test binary; the harness it is written
//! against lives in `support.rs`.

use crate::support::*;

// --- The three signatures ----------------------------------------------------

#[test]
fn downgrade_hands_back_a_weak_handle_to_the_same_cell() {
    // The annotation is the pin: `downgrade` on a `Shared<i32>` types as
    // `Weak<i32>`, and the cell it names is the one the handle came from — a
    // write through the strong handle is read back through the weak one.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let cell: Shared<i32> = Shared::new(1);
            let weak: Weak<i32> = cell.downgrade();
            cell.write() = 9;
            match weak.get() {
                Some(let view) => print(i"{*view}"),
                None => print("gone"),
            }
        }
        "#,
        "9\n",
    );
}

#[test]
fn downgrade_does_not_consume_the_strong_handle() {
    // `downgrade(&self)` takes a view of the handle and adds no holder, so the
    // handle it was taken from is untouched and every later use of it stands.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };

        fun main() {
            let cell: Shared<i32> = Shared::new(4);
            let first: Weak<i32> = cell.downgrade();
            let second: Weak<i32> = cell.downgrade();
            cell.write() = cell.read() + 1;
            print(i"{cell.read()}");
        }
        "#,
        "5\n",
    );
}

#[test]
fn upgrade_hands_back_an_option_of_a_strong_handle() {
    // `Option<Shared<T>>`, and the `Some` arm's handle is the SAME cell: a write
    // through the upgraded handle is visible through the original.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let cell: Shared<i32> = Shared::new(2);
            let weak: Weak<i32> = cell.downgrade();
            match weak.upgrade() {
                Some(let strong) => strong.write() = strong.read() * 10,
                None => print("gone"),
            }
            print(i"{cell.read()}");
        }
        "#,
        "20\n",
    );
}

#[test]
fn upgrade_is_some_on_this_backend_and_the_none_arm_still_compiles() {
    // The `None` arm is unreachable here and is the whole reason the surface is
    // an `Option`: a program that handles it today is a program that keeps
    // working when the count is real (C14 S4). It must therefore type-check and
    // be writable without a warning.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun reachable(weak: Weak<str>): str {
            match weak.upgrade() {
                Some(let strong) => strong.read(),
                None => "gone",
            }
        }

        fun main() {
            let cell: Shared<str> = Shared::new("here");
            print(reachable(cell.downgrade()));
        }
        "#,
        "here\n",
    );
}

#[test]
fn get_hands_back_an_option_of_a_view_over_a_scalar_and_an_aggregate() {
    // The scoped twin, at both pointee shapes: a scalar `T` and an aggregate
    // one. `*view` is the only way across, in both.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let number: Shared<i32> = Shared::new(7);
            match number.downgrade().get() {
                Some(let view) => print(i"{*view}"),
                None => print("gone"),
            }
            let items: Shared<List<i32>> = Shared::new([1, 2, 3]);
            match items.downgrade().get() {
                Some(let view) => print(i"{(*view).len()}"),
                None => print("gone"),
            }
        }
        "#,
        "7\n3\n",
    );
}

#[test]
fn get_reads_the_cell_live_rather_than_a_snapshot() {
    // The view names the cell's slot, so a write between two `get`s is visible
    // through the second — which is what separates a view from the copy
    // `read()` hands back.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let cell: Shared<i32> = Shared::new(1);
            let weak: Weak<i32> = cell.downgrade();
            match weak.get() {
                Some(let view) => print(i"{*view}"),
                None => print("gone"),
            }
            cell.write() = 42;
            match weak.get() {
                Some(let view) => print(i"{*view}"),
                None => print("gone"),
            }
        }
        "#,
        "1\n42\n",
    );
}

// --- `get`'s payload is SECOND-CLASS (claims-and-epochs.md §5a) ---------------

#[test]
fn a_get_view_may_not_be_read_as_a_value() {
    // Rule 3: a view's value is explicit. Handing the capture to something that
    // wants a value is the same refusal `Arena::get`'s capture earns, and it is
    // the pin that says the payload is a view rather than a value the signature
    // merely spelled with an ampersand.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun escape(weak: Weak<i32>): List<&i32> {
            mut out: List<&i32> = [];
            match weak.get() {
                Some(let view) => out.push(view),
                None => {},
            }
            out
        }

        fun main() {
            let cell: Shared<i32> = Shared::new(1);
            print(i"{escape(cell.downgrade()).len()}");
        }
        "#,
        "a view can't be read as a value here",
    );
}

#[test]
fn a_get_view_is_readonly() {
    // `Option<&T>`, never `&mut T`: `get` is for TOUCHING the cell, and
    // `upgrade` is for keeping it alive and writing through it.
    assert_fails_with(
        r#"
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let cell: Shared<List<i32>> = Shared::new([1]);
            match cell.downgrade().get() {
                Some(let view) => view.push(2),
                None => {},
            }
        }
        "#,
        "cannot mutate immutable 'view'",
    );
}

#[test]
fn a_get_view_binding_is_a_view_binding() {
    // The other direction of the same fact: the capture may be re-annotated as
    // a view, which an ordinary value binding cannot be.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let cell: Shared<i32> = Shared::new(3);
            match cell.downgrade().get() {
                Some(let view) => {
                    let alias: &i32 = view;
                    print(i"{*alias}");
                },
                None => {},
            }
        }
        "#,
        "3\n",
    );
}

// --- What a `Weak` may hold, and what it may cross ---------------------------

#[test]
fn weak_cannot_hold_a_resource() {
    // The same fence `Shared`, `List`, `Map`, `Set` and `Context` carry: a
    // native container's internals are host code the move checker cannot see.
    assert_fails_with(
        r#"
        import std::drop::Drop;
        import std::io::print;
        import std::shared::{ Shared, Weak };

        resource struct Guard { label: str }

        impl Guard with Drop {
            fun drop(&mut self) { print(self.label); }
        }

        fun main() {
            let held: Weak<Guard> = Shared::new(Guard { label = "g" }).downgrade();
            print("done");
        }
        "#,
        "`Weak` cannot hold the resource `Guard`",
    );
}

#[test]
fn a_weak_module_binding_is_excluded_from_the_hmr_transfer() {
    // A `Shared<T>` module binding transfers its PAYLOAD across a swap — the
    // new module's binding gets a fresh cell holding the old value. A `Weak<T>`
    // has no payload it owns: transferring one would carry the OLD module's
    // cell object across, which is the one thing a handle that does not decide
    // a lifetime must not do.
    //
    // It is Excluded by a rule that already shipped rather than by an arm added
    // for it — `compute_transferable` excludes a bodyless `external struct`
    // conservatively, and `Weak` is one — so this is a pin on the BEHAVIOUR
    // rather than on a line of code. It reds if that conservatism is ever
    // relaxed into "an empty struct is empty plain data", which is exactly the
    // day a `Weak` binding would start being transferred.
    let js = compile_hmr(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };
        let cell: Shared<i32> = Shared::new(0);
        let weak: Weak<i32> = cell.downgrade();
        fun main() {
            match weak.get() {
                Some(let view) => print(i"{*view}"),
                None => print("gone"),
            }
        }
        "#,
    );
    assert!(
        !js.contains("pkg::weak"),
        "a Weak binding is neither adopted nor exposed: {js}"
    );
    assert!(
        js.contains(r#"__hmr_adopt_shared("pkg::cell", "#),
        "the Shared binding beside it still uses the payload form: {js}"
    );
}

// --- The lowering (C14 S2: the shape lands, the guarantee waits) --------------

#[test]
fn downgrade_lowers_to_the_cell_itself_and_upgrade_to_some_of_it() {
    // `downgrade` is `SharedClone`'s twin — the receiver, unchanged — and
    // `upgrade` is the Option's `Some` arm around it. Nothing counts on this
    // backend, so there is nothing for a weak handle to be weaker THAN; what
    // the emission must not do is allocate a wrapper, because the native
    // representation (C14 S4) is a second word on the cell, not a box around it.
    assert_emits_containing(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let cell: Shared<i32> = Shared::new(1);
            let weak: Weak<i32> = cell.downgrade();
            match weak.upgrade() {
                Some(let strong) => print(i"{strong.read()}"),
                None => print("gone"),
            }
        }
        "#,
        "const weak = cell;",
    );
}

#[test]
fn get_lowers_to_some_of_the_cell_slot() {
    // `[ 0, cell.v ]` — the same slot `read()` names, in the `Some` arm, and
    // never `__clone`d: the payload is a view, which is the whole difference
    // between this and a `read()` standing in a storing position.
    assert_emits_containing(
        r#"
        import std::io::print;
        import std::shared::{ Shared, Weak };
        import std::option::Option::{ None, Some };

        fun main() {
            let cell: Shared<List<i32>> = Shared::new([1]);
            let weak: Weak<List<i32>> = cell.downgrade();
            match weak.get() {
                Some(let view) => print(i"{(*view).len()}"),
                None => print("gone"),
            }
        }
        "#,
        "[ 0, weak.v ]",
    );
}

// --- C14 S3: std's two back edges are weak, and nothing above them moves -----
//
// `observe`'s notify closure captures the signal's VALUE CELL, and
// `Subscription` aliases the signal's SUBSCRIBER LIST. Both are edges that must
// be followed and must not decide a lifetime — a cell reached from its own
// subscriber's closure, and a list aliased by a handle nobody owns it through —
// so both are weak now.
//
// On this backend the change is invisible by construction: `downgrade` is the
// identity, so the emitted graph is what it was and the heap-snapshot gate's
// mounted SCC count does not move (243 reachable / 2 cycles, before and after).
// What these pins hold is that the edge was rewritten without moving anything
// standing on it — the notification still fires with the current value, the
// detach still detaches, and the signal-less `teardown` shape still runs its
// release. Every one was run against the pre-change std and prints the same.

#[test]
fn an_observer_fires_with_the_current_value_through_the_weak_capture() {
    // `observe` upgrades on every notify and reads the cell through the strong
    // handle, so the observer sees each value exactly as it did when the
    // capture was strong — and the detach still empties the subscriber list.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Disposable, Signal, SignalCell };

        fun main() {
            let count: SignalCell<i32> = Signal::new(0);
            mut seen: List<i32> = [];
            let watch = count.on_change(|value: i32| seen.push(value));
            count.set(1);
            count.set(2);
            print(i"seen={seen.len()} subscribers={count.subscribers.read().len()}");
            watch.dispose();
            count.set(3);
            print(i"seen={seen.len()} subscribers={count.subscribers.read().len()}");
        }
        "#,
        "seen=2 subscribers=1\nseen=2 subscribers=0\n",
    );
}

#[test]
fn a_teardown_subscription_over_no_signal_still_releases_exactly_once() {
    // `Subscription::teardown` is the registration shape for a source outside
    // the signal graph (`std::dom`'s `listen`): there is no signal, so there is
    // no subscriber list to alias, and the cell it mints dies with the call.
    // Under counting its weak alias answers `None` — which is the right answer,
    // because the whole of this shape's teardown is the `release` one-shot, and
    // that is a cell of its own. The `None` arm is UNREACHABLE on this backend
    // (nothing counts, so every upgrade is `Some`), so what this pin holds is
    // the behaviour the arm has to preserve rather than the arm itself:
    // planting a `ret` into that arm leaves it green, and only C14 S4's real
    // count can make it red.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Disposable, Subscription };

        fun main() {
            mut released = 0;
            let bare = Subscription::teardown(|| released += 1);
            bare.dispose();
            bare.dispose();
            print(i"released={released}");
        }
        "#,
        "released=1\n",
    );
}

#[test]
fn a_set_after_the_owner_was_disposed_still_commits() {
    // `signal-cell-representation.md` §5.2, and the law C14 S4 must preserve:
    // disposal runs the owner's cleanups, which detach SUBSCRIBERS. It does
    // nothing whatever to the cell. So a write after it commits and the read
    // after that sees the new value — the cell is a fully live object that has
    // merely lost its audience, not a stale one. Pinned here because the weak
    // edges are the first change that could have made a disposed signal's cell
    // unreadable, and it must not have.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Disposable, Owner, Signal, SignalCell };

        fun main() {
            let owner = Owner::new();
            let tracked: SignalCell<i32> = Signal::new(10);
            owner.dispose();
            tracked.set(99);
            print(i"after-dispose={tracked.get()}");
        }
        "#,
        "after-dispose=99\n",
    );
}

// --- The hole `Weak::upgrade` opened in B267's cell walk ---------------------

#[test]
fn a_read_through_an_upgraded_handle_copies_like_every_other_read() {
    // B267's walk approximates cell identity by following handles between four
    // slot forms, and joins everything it cannot follow to `Unknown`. A MATCH
    // CAPTURE has no initializer to follow, and `Weak::upgrade` is the first
    // API in the language that binds a handle through one — so before the
    // capture was joined to `Unknown`, `Binding(strong)` was a singleton
    // component nothing ever mutated and the elision handed out the cell's own
    // storage. Measured: this program printed `2` where the same program
    // written without the upgrade printed `1`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ None, Some };
        import std::shared::{ Shared, Weak };

        fun main() {
            let cell: Shared<List<i32>> = Shared::new([1]);
            let weak: Weak<List<i32>> = cell.downgrade();
            match weak.upgrade() {
                Some(let strong) => {
                    mut copy = strong.read();
                    cell.write().push(9);
                    print(i"upgraded={copy.len()}");
                },
                None => {},
            }
            let direct: Shared<List<i32>> = Shared::new([1]);
            mut copy = direct.read();
            direct.write().push(9);
            print(i"direct={copy.len()}");
        }
        "#,
        "upgraded=1\ndirect=1\n",
    );
}

// --- The const channel needs no arm of its own -------------------------------

#[test]
fn a_weak_handle_works_in_the_const_interpreter() {
    // The const-eval interpreter evaluates the EMITTED JavaScript, and `Weak`'s
    // three intrinsics lower to shapes it already knows — an array literal
    // (`[ 0, cell ]`) and a property read (`cell.v`) — so unlike `Shared::new`
    // and `Shared::identity`, neither needs a runtime helper or an interpreter
    // arm. Pinned in both directions, because "no arm was needed" is a claim
    // about behaviour and not about a line of code that is not there.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ None, Some };
        import std::shared::{ Shared, Weak };

        const fun through_upgrade(): i32 {
            let cell: Shared<i32> = Shared::new(7);
            let weak: Weak<i32> = cell.downgrade();
            match weak.upgrade() {
                Some(let live) => live.read() + 1,
                None => 0,
            }
        }

        const fun through_get(): i32 {
            let cell: Shared<List<i32>> = Shared::new([1, 2, 3]);
            let weak: Weak<List<i32>> = cell.downgrade();
            match weak.get() {
                Some(let view) => (*view).len(),
                None => 0,
            }
        }

        const let upgraded: i32 = through_upgrade();
        const let counted: i32 = through_get();

        fun main() {
            print(i"{upgraded} {counted}");
        }
        "#,
        "8 3\n",
    );
}
