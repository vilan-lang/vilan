//! Reactive maps and sets (`proposal/reactive-maps-sets.md`, tracker A138):
//! `MapCell<K, V>` and `SetCell<T>`, whose writes ARE their deltas (`MapOp`,
//! `SetOp`), and the per-key handles — `at(key)`, `contains(x)` — whose
//! subscriptions land on one key's slot.
//!
//! What the pins hold is the per-key COST model as much as the values: a write
//! wakes the slot of the key it names and no other (§4), a `Reset` wakes every
//! live slot and `reconcile_to` only the keys it changed (Q6), a slot is counted
//! and goes with its last subscription (Q4), and `clear()` is one `Reset` (Q9).
//! The values are held against a plain `HashMap`/`HashSet` written the same way
//! under a seeded random walk; `native_differential` runs the map's walk on the
//! native backend too.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

/// The map walk, shared with `native_differential` (which runs it natively).
const MAP_WALK: &str = include_str!("../../../vilan-cli/tests/native/map_walk.vl");

/// The map operators' walk, shared with `native_differential` too.
const MAP_OPERATOR_WALK: &str =
    include_str!("../../../vilan-cli/tests/native/map_operator_walk.vl");

/// The set walk — JS only: the native backend refuses `SetOp`'s variants today
/// (`native_differential` pins the refusal by name).
const SET_WALK: &str = include_str!("../../../vilan-cli/tests/native/set_walk.vl");

/// A `describe` for the ops a cursor drains, prepended to the programs below.
const DESCRIBE_MAP_OPS: &str = r#"
import std::hash_map::HashMap;
import std::option::Option::{ self, None, Some };
import std::reactive::{ DeltaSource, MapCell, MapOp, Signal, Source, comp };

fun opt(value: Option<i32>): str {
    match value {
        Some(let held) => i"Some({held})",
        None => "None",
    }
}

fun describe(ops: List<MapOp<str, i32>>): str {
    mut out = "";
    for op in ops {
        match op {
            MapOp::Put(let key, let was, let now) => out = out + i"put({key},{opt(was)},{now}) ",
            MapOp::Delete(let key, let gone) => out = out + i"delete({key},{gone}) ",
            MapOp::Reset(let map) => out = out + i"reset({map.len()}) ",
        }
    }
    out
}
"#;

// --- S1: the cell's writes are its ops (§3.2) ----------------------------------

/// Every write records exactly the op §3.2's table names, and nothing for a write
/// that changes nothing: an overwrite carries what was there, a removal what left,
/// an absent removal nothing, `update` the value before and after, `get_or_insert`
/// a `Put` on a miss and nothing on a hit, `clear` ONE `Reset` (Q9) and nothing on
/// an empty map, `edit` one op per mutation under ONE notification.
#[test]
fn a138_s1_every_write_records_its_op() {
    assert_compiles_and_runs(
        &format!(
            "{DESCRIBE_MAP_OPS}{}",
            r#"
            fun main() {
                let cell: MapCell<str, i32> = MapCell::new();
                mut notified = 0;
                let watch = cell.on_change(|map| notified += 1);
                let cursor = cell.cursor();
                cell.insert("a", 1);
                cell.insert("a", 2);
                cell.insert("b", 3);
                cell.remove("a");
                cell.remove("a");
                cell.update("b", |&mut held| held *= 10);
                cell.update("zz", |&mut held| held *= 10);
                print(cell.get_or_insert("c", || 7));
                print(cell.get_or_insert("c", || 8));
                print(describe(cell.since(cursor)));
                print(notified);
                cell.clear();
                cell.clear();
                print(describe(cell.since(cursor)));
                print(notified);
                cell.edit(|&mut map| {
                    map.insert("x", 1);
                    map.insert("y", 2);
                    map.remove("x");
                    map.remove("nothing");
                });
                cell.edit(|&mut map| {
                    let _read = map.get("y");
                });
                print(describe(cell.since(cursor)));
                print(notified);
                print(cell.get("y"));
                print(cell.contains_key("x"));
                watch.dispose();
            }
            main();
            "#
        ),
        "7\n7\nput(a,None,1) put(a,Some(1),2) put(b,None,3) delete(a,2) put(b,Some(3),30) put(c,None,7) \n6\nreset(0) \n7\nput(x,None,1) put(y,None,2) delete(x,1) \n8\n[ 0, 2 ]\nfalse\n",
    );
}

/// A cursor that fell behind the log's ceiling is told the whole map as one
/// `Reset`, and a cursor taken after `of(map)` starts from the map, owed nothing.
#[test]
fn a138_s1_a_lagging_cursor_is_told_the_map() {
    assert_compiles_and_runs(
        &format!(
            "{DESCRIBE_MAP_OPS}{}",
            r#"
            fun main() {
                let cell: MapCell<str, i32> = MapCell::with_limit([("a", 1)].to_map(), 2);
                let cursor = cell.cursor();
                print(describe(cell.since(cursor)));
                cell.insert("b", 2);
                cell.insert("c", 3);
                cell.insert("d", 4);
                print(describe(cell.since(cursor)));
                cell.insert("e", 5);
                print(describe(cell.since(cursor)));
                print(cell.logged());
            }
            main();
            "#
        ),
        "\nreset(4) \nput(e,None,5) \n1\n",
    );
}

// --- S1: per-key tracking (§4) -----------------------------------------------

/// THE POINT OF THE CELL: `at(key)` depends on that key only. A derivation over
/// one key runs once per write to it and never for a write to another key, an
/// observer of another key is not called, and a write that changes nothing —
/// removing an absent key — wakes nothing at all. Red when a write wakes every
/// slot: the other key's observer is called and the derivation re-runs.
#[test]
fn a138_s1_at_depends_on_its_key_only() {
    assert_compiles_and_runs(
        &format!(
            "{DESCRIBE_MAP_OPS}{}",
            r#"
            fun main() {
                let cell: MapCell<str, i32> = MapCell::of([("a", 1), ("b", 2)].to_map());
                mut runs = 0;
                mut b_calls = 0;
                let (doubled, scope) = comp(|| cell.at("a").derive(|value| {
                    runs += 1;
                    value.map(|held| held * 2)
                }).memo());
                let watch_b = cell.at("b").on_change(|value| b_calls += 1);
                print(opt(doubled.get()));
                cell.insert("b", 20);
                cell.insert("c", 3);
                cell.remove("c");
                cell.remove("nothing");
                print(i"runs={runs} b={b_calls}");
                cell.insert("a", 5);
                cell.update("a", |&mut held| held += 1);
                print(opt(doubled.get()));
                print(i"runs={runs} b={b_calls}");
                cell.remove("a");
                print(opt(doubled.get()));
                print(i"runs={runs} b={b_calls}");
                watch_b.dispose();
                scope.dispose();
            }
            main();
            "#
        ),
        "Some(2)\nruns=1 b=1\nSome(12)\nruns=3 b=1\nNone\nruns=4 b=1\n",
    );
}

/// A key's slot is COUNTED (Q4): two subscriptions on one key share one slot, the
/// slot outlives the first disposal and goes with the second, and a key nobody
/// watches has none — so the table is the size of what is watched. Red when the
/// first disposal drops the slot: the second subscription stops hearing its key.
#[test]
fn a138_s1_a_key_slot_is_counted_and_goes_with_its_last_subscription() {
    assert_compiles_and_runs(
        &format!(
            "{DESCRIBE_MAP_OPS}{}",
            r#"
            fun main() {
                let cell: MapCell<str, i32> = MapCell::new();
                print(cell.watched());
                mut first = 0;
                mut second = 0;
                let one = cell.at("k").on_change(|value| first += 1);
                let two = cell.at("k").on_change(|value| second += 1);
                let other = cell.at("j").on_change(|value| {});
                print(cell.watched());
                cell.insert("k", 1);
                one.dispose();
                print(cell.watched());
                cell.insert("k", 2);
                print(i"first={first} second={second}");
                two.dispose();
                print(cell.watched());
                other.dispose();
                print(cell.watched());
                // A key re-watched after its slot went gets a fresh one.
                let again = cell.at("k").on_change(|value| second += 1);
                cell.insert("k", 3);
                print(i"second={second} watched={cell.watched()}");
                again.dispose();
                print(cell.watched());
            }
            main();
            "#
        ),
        "0\n2\n2\nfirst=1 second=2\n1\n0\nsecond=3 watched=1\n0\n",
    );
}

/// What a wholesale write wakes (Q6): `set` and `clear` record a `Reset`, which
/// cannot say which keys changed without comparing, so EVERY watched key wakes —
/// even one whose value did not move. `reconcile_to` compares, so it wakes only the
/// keys whose presence or value changed, and an identical map wakes nothing and
/// publishes nothing. Red when `reconcile_to` wakes every slot (`b` is called) or
/// `set` wakes none (`a`'s count stays).
#[test]
fn a138_s1_reset_wakes_every_live_slot_and_reconcile_only_what_changed() {
    assert_compiles_and_runs(
        &format!(
            "{DESCRIBE_MAP_OPS}{}",
            r#"
            fun main() {
                let cell: MapCell<str, i32> = MapCell::of([("a", 1), ("b", 2), ("c", 3)].to_map());
                mut a = 0;
                mut b = 0;
                mut c = 0;
                mut whole = 0;
                let watch_a = cell.at("a").on_change(|value| a += 1);
                let watch_b = cell.at("b").on_change(|value| b += 1);
                let watch_c = cell.at("c").on_change(|value| c += 1);
                let watch = cell.on_change(|map| whole += 1);
                let cursor = cell.cursor();
                cell.reconcile_to([("a", 10), ("b", 2), ("d", 4)].to_map());
                print(i"a={a} b={b} c={c} whole={whole}");
                print(describe(cell.since(cursor)));
                cell.reconcile_to([("a", 10), ("b", 2), ("d", 4)].to_map());
                print(i"a={a} b={b} c={c} whole={whole}");
                let held: HashMap<str, i32> = Source::get(cell);
                print(held.keys());
                cell.set([("a", 10), ("b", 2)].to_map());
                print(i"a={a} b={b} c={c} whole={whole}");
                cell.clear();
                print(i"a={a} b={b} c={c} whole={whole}");
                print(describe(cell.since(cursor)));
                watch.dispose();
                watch_c.dispose();
                watch_b.dispose();
                watch_a.dispose();
            }
            main();
            "#
        ),
        "a=1 b=0 c=1 whole=1\ndelete(c,3) put(a,Some(1),10) put(d,None,4) \na=1 b=0 c=1 whole=1\n[ 'a', 'b', 'd' ]\na=2 b=1 c=2 whole=2\na=3 b=2 c=3 whole=3\nreset(2) reset(0) \n",
    );
}

/// `at(key)` is also a `Signal<Option<V>>`: `set(Some(v))` inserts the key and
/// `set(None)` removes it, each recording its op and waking its slot; `notify`
/// wakes the key and records nothing.
#[test]
fn a138_s1_a_map_entry_writes_its_key() {
    assert_compiles_and_runs(
        &format!(
            "{DESCRIBE_MAP_OPS}{}",
            r#"
            fun main() {
                let cell: MapCell<str, i32> = MapCell::new();
                let entry = cell.at("k");
                mut seen: List<str> = [];
                let watch = entry.sub(|value| seen.push(opt(value)));
                let cursor = cell.cursor();
                entry.set(Some(1));
                entry.set(Some(2));
                entry.set(None);
                entry.set(None);
                entry.notify();
                print(seen);
                print(describe(cell.since(cursor)));
                print(entry.key());
                watch.dispose();
            }
            main();
            "#
        ),
        "[ 'None', 'Some(1)', 'Some(2)', 'None', 'None' ]\nput(k,None,1) put(k,Some(1),2) delete(k,2) \nk\n",
    );
}

/// The handle is a `Source`, so every consumer takes it: an `effect` (through
/// `Flow`'s blanket, which must reach the handle's own `attach_observer`), a
/// tracked read inside another body, a `switch` that re-selects WHICH key it
/// follows, and a whole-map read through a generic `Source` bound or `peek`.
#[test]
fn a138_s1_at_feeds_an_effect_a_tracked_read_and_a_switch() {
    assert_compiles_and_runs(
        &format!(
            "{DESCRIBE_MAP_OPS}{}",
            r#"
            import std::reactive::{ SignalCell, derive };

            fun size_of<S: Source<HashMap<str, i32>>>(source: S): usize {
                source.get().len()
            }

            fun main() {
                let cell: MapCell<str, i32> = MapCell::of([("a", 1), ("b", 2)].to_map());
                let picked: SignalCell<str> = Signal::new("a");
                mut effects: List<str> = [];
                let (outs, scope) = comp(|| {
                    cell.at("b").effect(|value| effects.push(opt(value)));
                    let sum = derive(|| cell.at("a").track().unwrap_or(0) + cell.at("b").track().unwrap_or(0)).memo();
                    let chosen = picked.switch(|key| cell.at(key)).memo();
                    (sum, chosen)
                });
                let (sum, chosen) = outs;
                print(i"{sum.get()} {opt(chosen.get())}");
                cell.insert("b", 20);
                cell.insert("z", 0);
                print(i"{sum.get()} {opt(chosen.get())}");
                picked.set("b");
                print(i"{sum.get()} {opt(chosen.get())}");
                cell.insert("a", 100);
                print(i"{sum.get()} {opt(chosen.get())}");
                print(effects);
                print(size_of(cell));
                print(cell.peek(|map| map.len()));
                scope.dispose();
                print(cell.watched());
            }
            main();
            "#
        ),
        "3 Some(1)\n21 Some(1)\n21 Some(20)\n120 Some(20)\n[ 'Some(2)', 'Some(20)' ]\n3\n3\n0\n",
    );
}

/// The map's walk: every write, checked after every step against a plain
/// `HashMap` — the map, its ORDER (a held key keeps its place, a removed and
/// re-inserted one goes to the end, `reconcile_to` appends in the target's order),
/// a mirror replayed from the drained ops, each watched key's handle, and how many
/// times each watched key woke (exactly once when the step named or changed it,
/// or reset everything; never otherwise).
#[test]
fn a138_s1_the_map_cell_agrees_with_a_hash_map_under_a_random_walk() {
    assert_compiles_and_runs(
        &format!("{MAP_WALK}\nmain();\n"),
        "failures 0\nwatched 4\nwatched 0\n[ 62, 76, 86, 80 ]\n",
    );
}

// --- S1: the set cell (§3.3) ---------------------------------------------------

/// `insert`/`remove` answer whether they changed the set and record only when
/// they did; `contains(x)` wakes for `x` alone; `clear` is one `Reset` and an empty
/// `clear` nothing; `reconcile_to` records the difference.
#[test]
fn a138_s1_a_set_cell_answers_records_and_wakes_per_member() {
    assert_compiles_and_runs(
        r#"
        import std::hash_set::HashSet;
        import std::reactive::{ DeltaSource, SetCell, SetOp, Source };

        fun describe(ops: List<SetOp<str>>): str {
            mut out = "";
            for op in ops {
                match op {
                    SetOp::Add(let value) => out = out + i"add({value}) ",
                    SetOp::Remove(let value) => out = out + i"remove({value}) ",
                    SetOp::Reset(let set) => out = out + i"reset({set.len()}) ",
                }
            }
            out
        }

        fun main() {
            let online: SetCell<str> = SetCell::of(["a", "b"].to_set());
            mut a_seen: List<bool> = [];
            mut c_calls = 0;
            let watch_a = online.contains("a").sub(|held| a_seen.push(held));
            let watch_c = online.contains("c").on_change(|held| c_calls += 1);
            let cursor = online.cursor();
            print(online.insert("c"));
            print(online.insert("c"));
            print(online.remove("a"));
            print(online.remove("a"));
            online.reconcile_to(["a", "c", "d"].to_set());
            online.reconcile_to(["a", "c", "d"].to_set());
            print(describe(online.since(cursor)));
            print(a_seen);
            print(i"c={c_calls} watched={online.watched()}");
            online.clear();
            online.clear();
            print(describe(online.since(cursor)));
            print(a_seen);
            print(i"c={c_calls}");
            watch_c.dispose();
            watch_a.dispose();
            print(online.watched());
        }
        main();
        "#,
        "true\nfalse\ntrue\nfalse\nadd(c) remove(a) remove(b) add(a) add(d) \n[ true, false, true ]\nc=1 watched=2\nreset(0) \n[ true, false, true, false ]\nc=2\n0\n",
    );
}

/// The set's walk, as the map's: the set and its order, a replayed mirror, the
/// answers `insert`/`remove` gave, each watched member's handle and its wakes.
#[test]
fn a138_s1_the_set_cell_agrees_with_a_hash_set_under_a_random_walk() {
    assert_compiles_and_runs(
        &format!("{SET_WALK}\nmain();\n"),
        "failures 0\nwatched 4\nwatched 0\n[ 70, 57, 62, 64 ]\n",
    );
}

// --- S2: the operators (§5) ----------------------------------------------------

/// The positional face and the key set, as the ops a consumer is told. `values()`
/// is a one-element `Splice` at a new key's RANK, a `SetAt` there for an
/// overwrite, a one-element `Splice` out for a removal — a removed and re-inserted
/// key goes to the end, as the map puts it — and a `Reset` for a `set`. `keys()`
/// is an `Add` for a new key, nothing for an overwrite, a `Remove` for a removal,
/// so a sealed key set's `contains(x)` wakes when `x` arrives or leaves and never
/// for a write to `x`'s value. Red with a stale rank (an appended key's Fenwick
/// node built without the slots below it). (`keys()` answering an overwrite with an
/// `Add` would NOT redden it: the seal drops an `Add` of a held member, so a sealed
/// key set is right either way; the stage's own op is what `key_ops` states.)
#[test]
fn a138_s2_values_ranks_its_ops_and_keys_changes_only_on_arrival_and_removal() {
    assert_compiles_and_runs(
        r#"
        import std::hash_map::HashMap;
        import std::hash_set::HashSet;
        import std::option::Option::{ self, None, Some };
        import std::reactive::{
            DeltaSource,
            MapCell,
            SeqOp,
            SetOp,
            Signal,
            SignalCell,
            Source,
            comp,
            on_cleanup,
        };

        fun nums(xs: List<i32>): str {
            mut out = "[";
            for x in xs {
                out = out + i" {x}";
            }
            out + " ]"
        }

        fun seq(ops: List<SeqOp<i32>>): str {
            mut out = "";
            for op in ops {
                match op {
                    SeqOp::Splice(let at, let removed, let inserted) => out = out + i"splice({at},{nums(removed)},{nums(inserted)}) ",
                    SeqOp::SetAt(let at, let was, let now) => out = out + i"set({at},{was},{now}) ",
                    SeqOp::Reset(let items) => out = out + i"reset({nums(items)}) ",
                    SeqOp::Move(let from, let count, let to) => out = out + i"move({from},{count},{to}) ",
                }
            }
            out
        }

        fun set_ops(ops: List<SetOp<str>>): str {
            mut out = "";
            for op in ops {
                match op {
                    SetOp::Add(let value) => out = out + i"add({value}) ",
                    SetOp::Remove(let value) => out = out + i"remove({value}) ",
                    SetOp::Reset(let set) => out = out + i"reset({set.len()}) ",
                }
            }
            out
        }

        fun main() {
            let cell: MapCell<str, i32> = MapCell::of([("a", 1), ("b", 2), ("c", 3)].to_map());
            let ((values, keys), scope) = comp(|| (cell.values().memo(), cell.keys().memo()));
            let vcursor = values.cursor();
            let kcursor = keys.cursor();
            cell.insert("b", 20);
            cell.insert("d", 4);
            cell.remove("a");
            cell.insert("a", 5);
            cell.remove("zz");
            print(seq(values.since(vcursor)));
            print(set_ops(keys.since(kcursor)));
            print(values.get());
            mut d_wakes = 0;
            let watch = keys.contains("d").on_change(|held| d_wakes += 1);
            cell.insert("d", 40);
            cell.insert("e", 1);
            cell.remove("d");
            print(d_wakes);
            cell.set([("q", 1)].to_map());
            print(seq(values.since(vcursor)));
            print(set_ops(keys.since(kcursor)));
            watch.dispose();
            scope.dispose();
        }

        main();
        "#,
        "set(1,2,20) splice(3,[ ],[ 4 ]) splice(0,[ 1 ],[ ]) splice(3,[ ],[ 5 ]) \nadd(d) remove(a) add(a) \n[ 20, 3, 4, 5 ]\n1\nset(2,4,40) splice(4,[ ],[ 1 ]) splice(2,[ 40 ],[ ]) reset([ 1 ]) \nadd(e) remove(d) reset(1) \n",
    );
}

/// The keyed operators' cost model and owners (§5, §6.4): `map_values` runs its
/// closure once per `Put` and never for a `Delete`, and each key's run is
/// released when the key re-runs or leaves and the rest with the consumer; a
/// `filter` whose predicate answers a flow FOLLOWS it per key — a flip is a
/// `Delete` or a `Put` downstream, a key that stays kept is a `Put` only when its
/// input was replaced; a sealed map pipe's `at(key)` wakes for that key alone; and
/// `count`, `sum_by` and `.sample()` read once. Red with the re-run not releasing
/// the last run (`released` short), or a predicate flow not followed.
#[test]
fn a138_s2_keyed_operators_run_per_put_follow_flows_and_release_per_key() {
    assert_compiles_and_runs(
        r#"
        import std::hash_map::HashMap;
        import std::option::Option::{ self, None, Some };
        import std::reactive::{ DeltaSource, MapCell, MapOp, Signal, SignalCell, Source, comp, on_cleanup };

        fun opt(value: Option<i32>): str {
            match value {
                Some(let held) => i"Some({held})",
                None => "None",
            }
        }

        fun describe(ops: List<MapOp<str, i32>>): str {
            mut out = "";
            for op in ops {
                match op {
                    MapOp::Put(let key, let was, let now) => out = out + i"put({key},{opt(was)},{now}) ",
                    MapOp::Delete(let key, let gone) => out = out + i"delete({key},{gone}) ",
                    MapOp::Reset(let map) => out = out + i"reset({map.len()}) ",
                }
            }
            out
        }

        fun main() {
            let cell: MapCell<str, i32> = MapCell::of([("a", 1), ("b", 2)].to_map());
            let online: SignalCell<bool> = Signal::new(true);
            mut runs = 0;
            mut released = 0;
            let ((scaled, shown), scope) = comp(|| (cell
                .map_values(|v| {
                    runs += 1;
                    on_cleanup(|| released += 1);
                    v * 10
                })
                .memo(), cell.filter(|key, v| online.derive(|on| on || v > 1)).memo()));
            let scursor = scaled.cursor();
            let fcursor = shown.cursor();
            print(i"runs={runs} released={released}");
            cell.insert("c", 3);
            cell.insert("a", 5);
            cell.remove("b");
            print(i"runs={runs} released={released}");
            print(describe(scaled.since(scursor)));
            online.set(false);
            print(describe(shown.since(fcursor)));
            cell.insert("a", 0);
            cell.insert("a", 7);
            online.set(true);
            print(describe(shown.since(fcursor)));
            mut a_wakes = 0;
            let watch = scaled.at("a").on_change(|value| a_wakes += 1);
            cell.insert("c", 30);
            cell.insert("a", 6);
            print(i"a_wakes={a_wakes} {opt(scaled.at("a").get())} {opt(scaled.get("c"))}");
            watch.dispose();
            let counted = cell.count().sample();
            let summed = cell.sum_by(|v| v * 2).sample();
            let pairs = cell.map_values(|v| v + 1).sample();
            print(i"{counted} {summed} {pairs.len()}");
            scope.dispose();
            print(i"runs={runs} released={released}");
        }

        main();
        "#,
        "runs=2 released=0\nruns=4 released=2\nput(c,None,30) put(a,Some(10),50) delete(b,20) \nput(c,None,3) put(a,Some(1),5) delete(b,2) \ndelete(a,5) put(a,None,7) \na_wakes=1 Some(60) Some(300)\n2 72 2\nruns=8 released=8\n",
    );
}

/// The operators' walk: every write a `MapCell` has, interleaved with flips of the
/// flows the closures answer, checked after every step against a recomputation
/// from the map — `values`/`entries` (the rank index, through enough churn to
/// compact it), `map_values` plain and following, `filter` plain and following,
/// `count`, `sum_by` plain and following, and two chains (`filter(..).values()`,
/// `filter(..).count()`).
#[test]
fn a138_s2_the_map_operators_agree_with_a_recomputation_under_a_random_walk() {
    assert_compiles_and_runs(&format!("{MAP_OPERATOR_WALK}\nmain();\n"), "failures 0\n");
}

/// A146's identity for the new sources: every handle to one cell (a copy of a
/// `MapCell`, a `SetCell`, the memo a pipe sealed into) names it with one number,
/// and every per-key handle names its KEY — two `at("a")`s agree, `at("b")`
/// differs, and the same key of another map differs — so a tracked read keeps one
/// edge across runs and a connection dedups an exported handle.
#[test]
fn a138_every_handle_to_one_cell_or_one_key_shares_an_identity() {
    assert_compiles_and_runs(
        r#"
        import std::hash_map::HashMap;
        import std::reactive::{ MapCell, SetCell, Source, comp };

        fun main() {
            let cell: MapCell<str, i32> = MapCell::new();
            let copy = cell;
            let other: MapCell<str, i32> = MapCell::new();
            print(cell.identity() == copy.identity());
            print(cell.identity() == other.identity());
            print(cell.at("a").identity() == copy.at("a").identity());
            print(cell.at("a").identity() == cell.at("b").identity());
            print(cell.at("a").identity() == other.at("a").identity());
            let set: SetCell<i32> = SetCell::new();
            print(set.contains(1).identity() == set.contains(1).identity());
            print(set.contains(1).identity() == set.contains(2).identity());
            let (memo, scope) = comp(|| cell.map_values(|value| value + 1).memo());
            print(memo.identity() == memo.identity());
            print(memo.at("a").identity() == memo.at("a").identity());
            print(memo.identity() == cell.identity());
            scope.dispose();
        }
        main();
        "#,
        "true\nfalse\ntrue\nfalse\nfalse\ntrue\nfalse\ntrue\ntrue\nfalse\n",
    );
}
