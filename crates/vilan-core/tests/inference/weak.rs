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
