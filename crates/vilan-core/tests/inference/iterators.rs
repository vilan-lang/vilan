//! The iterator protocol and its adapters (I3/I5), the enum discriminant
//! family, and the monomorphization instance key (B95/B102).
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- I5: `Iterator::next` takes `&mut self` (proposal/iterator-adapters.md P1,
// --- slice S1) --------------------------------------------------------------
// The trait shipped declaring `fun next(self): Option<T>`, and B29's receiver-
// convention conformance rightly rejected the `&mut self` every stateful
// iterator needs — so the trait was unconformable as declared, and `Range`, the
// one real lazy iterator in std, deliberately did not implement it. `next` is
// now `&mut self`; these pin that the trait is implementable, that std's own
// conformers work, and that the by-value declaration is gone for good.

#[test]
fn i5_a_stateful_iterator_conforms_to_the_repaired_trait() {
    // The shape the by-value receiver made impossible: state on the ITERATOR,
    // advanced by `next`. Pre-repair this was
    // "`Counting`'s `next` receives `&mut self`, but `Iterator` declares `self`".
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self, Some, None };

        struct Counting { at: i32, limit: i32 }

        impl Counting with Iterator<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit {
                    self.at = self.at + 1;
                    Some(self.at)
                } else {
                    None
                }
            }
        }

        fun main() {
            mut counting = Counting { at = 0, limit = 3 };
            for value in counting {
                print(value);
            }
        }
        "#,
        "1\n2\n3\n",
    );
}

#[test]
fn i5_a_by_value_next_no_longer_conforms() {
    // The other direction, and the pin that keeps the repair honest: a conformer
    // declaring the OLD by-value receiver is now the conformance error. Without
    // it, re-widening `next` back to `self` would go unnoticed.
    assert_fails_with(
        r#"
        import std::iterator::Iterator;
        import std::option::Option::{ self, Some, None };

        struct Fixed { value: i32 }

        impl Fixed with Iterator<i32> {
            fun next(self): Option<i32> {
                Some(self.value)
            }
        }

        fun main() {}
        "#,
        "match the receiver convention",
    );
}

#[test]
fn i5_range_implements_the_iterator_trait() {
    // `Range` gains the `with Iterator<i32>` clause it has always deserved, so
    // the docs' "`Range` is one such type" is true for the first time. The
    // `for` protocol is name-resolved and worked either way — what is new is
    // that a bound `I: Iterator<i32>` accepts a `Range`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self };
        import std::range::Range;

        fun total<I: Iterator<i32>>(mut source: I): i32 {
            mut sum = 0;
            for value in source {
                sum = sum + value;
            }
            sum
        }

        fun main() {
            print(total(Range::new(1, 5)));
            mut range = Range::new(0, 3);
            print(range.next().unwrap_or(-1));
            print(range.next().unwrap_or(-1));
        }
        "#,
        "10\n0\n1\n",
    );
}

#[test]
fn i5_iterator_from_fn_follows_the_repaired_receiver() {
    // std's other conformer. `from_fn`'s closure carries the state, so the
    // receiver change is invisible at the call site EXCEPT that the binding must
    // now be `mut` — which is the honest reading: pulling advances it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut produced = 0;
            mut naturals = Iterator::from_fn(|| {
                produced = produced + 1;
                if produced <= 2 { Some(produced) } else { None }
            });
            print(naturals.next().unwrap_or(-1));
            print(naturals.next().unwrap_or(-1));
            print(naturals.next().unwrap_or(-1));
        }
        "#,
        "1\n2\n-1\n",
    );
}

// --- I3 S3: `ListIterator` + `List::iter` (proposal/iterator-adapters.md P0) --
// `List` reached none of the protocol: it implemented neither trait and had no
// cursor, so the headline chain was blocked before any adapter existed.
// `List::iter()` returns a concrete `ListIterator<T>` — concrete because a bare
// trait type is not a value (B4), so an adapter chain's type has to stay
// concrete end to end.

#[test]
fn list_iter_walks_the_elements_in_order() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut cursor = [1, 2, 3].iter();
            for value in cursor {
                print(value);
            }
        }
        "#,
        "1\n2\n3\n",
    );
}

#[test]
fn list_iter_over_an_empty_list_is_immediately_exhausted() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self };

        fun main() {
            let empty: List<i32> = [];
            mut cursor = empty.iter();
            print(cursor.next().is_none());
            mut count = 0;
            for _value in cursor {
                count = count + 1;
            }
            print(count);
        }
        "#,
        "true\n0\n",
    );
}

#[test]
fn list_iter_stays_exhausted_past_the_last_element() {
    // Cursor exhaustion: `next` past the end keeps answering `None` rather than
    // running off the array (the index guard, not a panic from `[]`).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self };

        fun main() {
            mut cursor = [7].iter();
            print(cursor.next().unwrap_or(-1));
            print(cursor.next().unwrap_or(-1));
            print(cursor.next().unwrap_or(-1));
        }
        "#,
        "7\n-1\n-1\n",
    );
}

#[test]
fn list_iter_holds_a_snapshot_so_a_later_push_is_not_walked() {
    // The rule-1 interaction, and the reason `iter` costs a copy: the cursor
    // stores the list in a slot that outlives the call, so it snapshots.
    // Mutating the list mid-walk cannot lengthen the walk — which is also what
    // keeps rule 4 out of it, since the cursor shares no storage with `live`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut live = [1, 2];
            mut cursor = live.iter();
            live.push(3);
            mut total = 0;
            for value in cursor {
                total = total + value;
            }
            print(total);
            print(live.len());
        }
        "#,
        "3\n3\n",
    );
}

#[test]
fn list_iter_satisfies_an_iterator_bound() {
    // `ListIterator` declares the trait, so it is accepted where the protocol is
    // asked for by BOUND rather than by method name.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;

        fun count_of<I: Iterator<i32>>(mut source: I): i32 {
            mut seen = 0;
            for _value in source {
                seen = seen + 1;
            }
            seen
        }

        fun main() {
            print(count_of([4, 5, 6].iter()));
        }
        "#,
        "3\n",
    );
}

// --- B77: a user `impl List<type T>` made std's own `List` methods report,
// --- nondeterministically (FIXED) ---------------------------------------------
//
// A constraint id does NOT identify one declaring file. `impl Subject<type T>`
// deliberately inherits the SUBJECT's constraint id (`register_subject_binders`),
// so a user's `impl List<type T>` is the same id as `list.vl`'s `struct List<T>`
// — by design, so the binder means exactly what writing the subject's bound out
// would mean. The residual-generic leak check collapsed that many-to-one
// relation into a `HashMap<TypeId, SourceId>` via `.collect()`, i.e. last write
// wins over a randomly-seeded `HashMap` iteration. Whichever entity came last
// became "the" declaring file, so on ~half of cold compiles the user's file won
// and every `List<T>` residual inside `list.vl` read as declared elsewhere. The
// check now keeps the SET of declaring files and asks whether the binding's own
// file is among them, which is the question the rule always meant to ask and is
// order-independent.

/// A user-declared `impl List<type T>` block must not make the entry-scoped
/// checks report against std's OWN `List::map` / `List::filter` bodies — "the
/// type of 'result' is never fully determined" against `mut result =
/// List::new()` in `list.vl`. Two things were wrong at once:
///
/// 1. the diagnostic was **spurious**: `result` is fixed by the `result.push(..)`
///    below it and by the declared return type, which is why the other half of
///    the runs were clean; and
/// 2. it pointed into **std**, whose definition-site diagnostics are meant to be
///    frozen (`analysis-reuse.md` §6) — so a user's impl block was un-freezing
///    entities it does not own.
///
/// The order-dependence is what makes it worth its own pin: a single compile
/// proves nothing, so this counts. It is also **cold-path only** — the base
/// cache (`analysis-reuse.md` §6.10) serves every compile after the first in a
/// process, and a served world is always the clean one, which is why a naive
/// loop passed 300/300 and proved nothing either. So each attempt clears the
/// cache first. Measured that way it failed roughly half the time, here and on
/// the pre-arc tree (39c951c, 14/30 through the CLI), so it predated the I3 arc
/// that found it. It is 30/30 clean now, and goes red at ~15/30 if the leak
/// check is put back on a single declaring source.
///
/// `base_cache_clear` is process-global. Under nextest each test is its own
/// process; under plain `cargo test` the worst it does to a concurrent test is
/// cost it a cold analysis, since the base cache is a reuse cache and no answer
/// may depend on a hit.
#[test]
fn a_user_impl_on_list_does_not_report_against_stds_own_list_methods() {
    let source = r#"
        import std::io::print;

        impl List<type T> {
            fun second_len(self): i32 {
                self.len()
            }
        }

        fun main() {
            print([1, 2, 3].second_len());
        }
        "#;
    let mut spurious = 0;
    let mut first_report = None;
    for _attempt in 0..30 {
        vilan_core::analyzer::base_cache_clear();
        if let Err(errors) = compile(source) {
            spurious += 1;
            first_report.get_or_insert(errors);
        }
    }
    assert_eq!(
        spurious, 0,
        "{spurious}/30 cold compiles reported against std; the first said: {first_report:#?}"
    );
}

/// The other half of B77's rule, and the guard that the fix is not a
/// suppression: sharing a constraint id across files makes a residual legal in
/// EVERY file that declares it, not in none of them. The user's own impl body
/// binds `T`, so a `List<T>` binding inside it is as legitimate as the identical
/// binding inside `list.vl` — and both must stay clean on the same cold compile.
/// This went red on ~half of cold attempts before the fix, in the runs where
/// `list.vl` rather than the entry won the collapse.
#[test]
fn a_generic_residual_is_legal_in_every_file_that_declares_its_parameter() {
    let source = r#"
        import std::io::print;

        impl List<type T> {
            fun doubled(self): List<T> {
                mut copy = List::new();
                for item in self {
                    copy.push(item);
                    copy.push(item);
                }
                copy
            }
        }

        fun main() {
            print([1, 2].doubled().len());
        }
        "#;
    let mut spurious = 0;
    let mut first_report = None;
    for _attempt in 0..30 {
        vilan_core::analyzer::base_cache_clear();
        if let Err(errors) = compile(source) {
            spurious += 1;
            first_report.get_or_insert(errors);
        }
    }
    assert_eq!(
        spurious, 0,
        "{spurious}/30 cold compiles rejected a legitimate residual; \
         the first said: {first_report:#?}"
    );
}

/// B77's fix must not soften the B16 rule it lives inside: a parameter declared
/// ONLY in another file still leaks. `Map::new`'s `K`/`V` are declared in
/// `map.vl` and nowhere in the entry, so the set never contains the entry and
/// the annotate steer still lands — on the cold path, where B77 lived.
#[test]
fn a_leaked_generic_still_reports_on_the_cold_path() {
    let source = r#"
        import std::map::Map;
        fun main() {
            mut table = Map::new();
            table.insert("k", 1);
        }
        "#;
    for attempt in 0..5 {
        vilan_core::analyzer::base_cache_clear();
        match compile(source) {
            Ok(_) => panic!("attempt {attempt}: the leaked `Map::new` residual went unreported"),
            Err(errors) => assert!(
                errors
                    .iter()
                    .any(|error| error.contains("never fully determined")),
                "attempt {attempt}: wrong diagnostic: {errors:#?}"
            ),
        }
    }
}

// --- I3 S4: the adapters (proposal/iterator-adapters.md §3) -------------------
// Trait defaults on the repaired `Iterator<T>`, each returning a named concrete
// struct that holds its upstream by value. Terminals arrive in S5, so these
// drive the chains with a `for` loop — which also pins that the loop protocol
// reaches an adapter, per instantiation.

/// The source every adapter pin composes over: a stateful conformer that only
/// works because `next` takes `&mut self`, and an UNBOUNDED one, so a pin that
/// forgets to short-circuit hangs rather than passing.
fn adapter_program(body: &str) -> String {
    format!(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{{ self, Some, None }};
        import std::range::Range;

        struct Naturals {{ at: i32 }}

        impl Naturals with Iterator<i32> {{
            fun next(&mut self): Option<i32> {{
                self.at = self.at + 1;
                Some(self.at)
            }}
        }}
        {body}
        "#
    )
}

#[test]
fn map_applies_its_closure_to_every_value() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut doubled = [1, 2, 3].iter().map(|n| n * 2);
                for value in doubled {
                    print(value);
                }
                mut single = [7].iter().map(|n| n + 1);
                for value in single {
                    print(value);
                }
            }
            "#,
        ),
        "2\n4\n6\n8\n",
    );
}

#[test]
fn map_over_an_empty_source_yields_nothing() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                let empty: List<i32> = [];
                mut mapped = empty.iter().map(|n| n * 2);
                mut seen = 0;
                for _value in mapped {
                    seen = seen + 1;
                }
                print(seen);
            }
            "#,
        ),
        "0\n",
    );
}

#[test]
fn map_changes_the_element_type() {
    // `Mapped<Self, T, U>` carries both, so a chain may change type mid-way and
    // stay concrete — the property B4 makes load-bearing.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut labelled = [1, 2].iter().map(|n| i"n={n}");
                for label in labelled {
                    print(label);
                }
            }
            "#,
        ),
        "n=1\nn=2\n",
    );
}

#[test]
fn an_adapter_pulls_nothing_until_it_is_pulled() {
    // Laziness, stated as an observation rather than a claim: constructing the
    // chain runs no closure, and one `next` runs exactly one.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut pulled = 0;
                mut mapped = [1, 2, 3].iter().map(|n| {
                    pulled = pulled + 1;
                    n * 2
                });
                print(pulled);
                print(mapped.next().unwrap_or(-1));
                print(pulled);
            }
            "#,
        ),
        "0\n2\n1\n",
    );
}

#[test]
fn filter_keeps_only_what_the_predicate_holds_for() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut evens = [1, 2, 3, 4, 5].iter().filter(|n| n % 2 == 0);
                for value in evens {
                    print(value);
                }
            }
            "#,
        ),
        "2\n4\n",
    );
}

#[test]
fn a_filter_that_rejects_everything_is_exhausted_not_stuck() {
    // The loop inside `filter`'s `next` has to end on the upstream's `None`,
    // not on a match — a predicate nothing satisfies is the case that proves it.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut none = [1, 2, 3].iter().filter(|n| n > 100);
                mut seen = 0;
                for _value in none {
                    seen = seen + 1;
                }
                print(seen);
                let empty: List<i32> = [];
                print(empty.iter().filter(|n| n > 0).next().is_none());
            }
            "#,
        ),
        "0\ntrue\n",
    );
}

#[test]
fn take_stops_at_its_budget() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut first2 = [1, 2, 3, 4].iter().take(2);
                for value in first2 {
                    print(value);
                }
            }
            "#,
        ),
        "1\n2\n",
    );
}

#[test]
fn take_of_zero_yields_nothing_and_take_past_the_end_stops_early() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut nothing = [1, 2, 3].iter().take(0);
                print(nothing.next().is_none());
                mut negative = [1, 2, 3].iter().take(-4);
                print(negative.next().is_none());
                mut over = [1, 2].iter().take(9);
                mut seen = 0;
                for _value in over {
                    seen = seen + 1;
                }
                print(seen);
            }
            "#,
        ),
        "true\ntrue\n2\n",
    );
}

#[test]
fn take_short_circuits_an_unbounded_source() {
    // The pin the whole laziness argument rests on: `Naturals` never answers
    // `None`, so this terminates only because `take` stops pulling. A `take`
    // that consumed its upstream eagerly would hang the test binary.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut first3 = Naturals { at = 0 }.take(3);
                for value in first3 {
                    print(value);
                }
            }
            "#,
        ),
        "1\n2\n3\n",
    );
}

#[test]
fn skip_drops_the_first_values() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut rest = [1, 2, 3, 4].iter().skip(2);
                for value in rest {
                    print(value);
                }
                mut all = [1, 2].iter().skip(0);
                for value in all {
                    print(value);
                }
            }
            "#,
        ),
        "3\n4\n1\n2\n",
    );
}

#[test]
fn skipping_past_the_end_leaves_an_exhausted_iterator() {
    // The upstream runs out DURING the skip, which is the case a naive `skip`
    // gets wrong by then pulling once more and answering with a stale value.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut gone = [1, 2].iter().skip(5);
                print(gone.next().is_none());
                print(gone.next().is_none());
                let empty: List<i32> = [];
                print(empty.iter().skip(3).next().is_none());
            }
            "#,
        ),
        "true\ntrue\ntrue\n",
    );
}

#[test]
fn skip_pays_its_cost_on_the_first_pull_not_at_construction() {
    // An adapter that consumed its upstream when it was BUILT would not be
    // lazy — and over `Naturals` it would not return at all.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut later = Naturals { at = 0 }.skip(2).take(2);
                for value in later {
                    print(value);
                }
            }
            "#,
        ),
        "3\n4\n",
    );
}

#[test]
fn enumerate_pairs_each_value_with_its_position() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut indexed = ["a", "b", "c"].iter().enumerate();
                for pair in indexed {
                    print(i"{pair.0}:{pair.1}");
                }
                let empty: List<str> = [];
                print(empty.iter().enumerate().next().is_none());
            }
            "#,
        ),
        "0:a\n1:b\n2:c\ntrue\n",
    );
}

#[test]
fn enumerate_counts_its_own_output_not_the_upstreams() {
    // Over a `filter`, the index must number what SURVIVES — the classic
    // off-by-provenance bug, where the position comes from the source instead.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut indexed = [1, 2, 3, 4].iter().filter(|n| n % 2 == 0).enumerate();
                for pair in indexed {
                    print(i"{pair.0}:{pair.1}");
                }
            }
            "#,
        ),
        "0:2\n1:4\n",
    );
}

#[test]
fn zip_stops_with_the_shorter_side() {
    // Both directions, because they take different paths through `next`: a
    // short LEFT never asks the right side, a short RIGHT drops a left value.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut short_right = [1, 2, 3].iter().zip(["x", "y"].iter());
                for pair in short_right {
                    print(i"{pair.0}{pair.1}");
                }
                mut short_left = [1].iter().zip(["x", "y", "z"].iter());
                for pair in short_left {
                    print(i"{pair.0}{pair.1}");
                }
                let empty: List<i32> = [];
                print(empty.iter().zip([1, 2].iter()).next().is_none());
                print([1, 2].iter().zip(empty.iter()).next().is_none());
            }
            "#,
        ),
        "1x\n2y\n1x\ntrue\ntrue\n",
    );
}

#[test]
fn zip_bounds_an_unbounded_side() {
    // `zip` over `Naturals` terminates because the finite side ends it —
    // the same short-circuit property as `take`, reached differently.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut numbered = Naturals { at = 0 }.zip(["p", "q"].iter());
                for pair in numbered {
                    print(i"{pair.0}{pair.1}");
                }
            }
            "#,
        ),
        "1p\n2q\n",
    );
}

#[test]
fn chain_yields_the_left_side_then_the_right() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut both = [1, 2].iter().chain([3, 4].iter());
                for value in both {
                    print(value);
                }
            }
            "#,
        ),
        "1\n2\n3\n4\n",
    );
}

#[test]
fn chain_handles_an_empty_side_on_either_end() {
    // And, once the left is exhausted, it is never asked again — the latch.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                let empty: List<i32> = [];
                mut right_only = empty.iter().chain([5, 6].iter());
                for value in right_only {
                    print(value);
                }
                mut left_only = [7].iter().chain(empty.iter());
                for value in left_only {
                    print(value);
                }
                print(empty.iter().chain(empty.iter()).next().is_none());
            }
            "#,
        ),
        "5\n6\n7\ntrue\n",
    );
}

#[test]
fn adapters_of_different_kinds_compose_into_one_chain() {
    // Four stages over a `List`, each a different adapter, each holding the
    // previous BY VALUE — the "adapter over adapter over adapter" case, where a
    // dispatch that lost its instantiation used to emit an empty body (B55).
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut pipeline = [1, 2, 3, 4, 5, 6]
                    .iter()
                    .filter(|n| n % 2 == 0)
                    .map(|n| n * 10)
                    .skip(1)
                    .take(2);
                for value in pipeline {
                    print(value);
                }
            }
            "#,
        ),
        "40\n60\n",
    );
}

#[test]
fn a_stateful_custom_conformer_drives_the_whole_adapter_set() {
    // The arc's point, end to end: a user type whose `next` mutates its OWN
    // state — impossible before I5 — reaching every adapter through the trait's
    // defaults, with `Range` zipped in as a second std source.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut squares = Naturals { at = 0 }
                    .map(|n| n * n)
                    .filter(|n| n % 2 == 1)
                    .zip(Range::new(10, 13))
                    .take(2)
                    .map(|pair| pair.0 + pair.1);
                for value in squares {
                    print(value);
                }
            }
            "#,
        ),
        "11\n20\n",
    );
}

#[test]
fn an_adapter_chain_leaves_its_source_list_alone() {
    // The memory-model half: `iter()` snapshots (rule 1), and every adapter
    // holds its upstream by value, so the whole pipeline is pure with respect
    // to the list it started from — including under a `map` that would
    // otherwise write through a shared element.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut source = [1, 2, 3];
                mut doubled = source.iter().map(|n| n * 2);
                mut total = 0;
                for value in doubled {
                    total = total + value;
                }
                print(total);
                print(source.len());
                print(source[0]);
                source.push(4);
                print(source.len());
            }
            "#,
        ),
        "12\n3\n1\n4\n",
    );
}

// --- B78: the protocol loop dropped the element's type when the iterator's
// --- element IS its own generic parameter (FIXED) ----------------------------
//
// `iterable_element_type` reads the element off the DECLARED return type of the
// subject's `next` — `impl ListIterator<type T> { fun next(..): Option<T> }` —
// and took that payload verbatim. The payload is written in the SUBJECT's own
// parameters, so it is abstract until instantiated against the receiver's
// arguments, exactly like the `Trait` arm one match-arm below (which does
// substitute, and is why a bounded-generic loop always worked). Untouched, the
// binding got the bare parameter `T`, and a `T` admits nothing: field access,
// method call and call-as-a-function all refused it.
//
// `enumerate` and `zip` hid the defect for the whole I3 arc because their
// payloads are STRUCTURAL — `Option<(i32, T)>`, `Option<(T, U)>` — so the loop
// saw a tuple whose PARTS were abstract, which projects fine, rather than a
// whole that was. The subject arm now builds the same instantiation context the
// trait arm does, from the struct's or enum's declared parameters.

/// The filed shape: `List::iter()` is std's first generically-elemented
/// iterator, so `for pair in [(1, "a")].iter()` was the first thing to reach it
/// — "cannot access field '0' on type T". The same iterator pulled BY HAND was
/// always fine, which is what places the defect in the loop's binding rather
/// than in the iterator.
#[test]
fn a_protocol_loop_keeps_a_tuple_elements_type_through_a_generic() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut pulled = [(1, "a"), (2, "b")].iter();
            if pulled.next() is Some(let pair) {
                print(pair.0);
            }
            for pair in [(1, "a"), (2, "b")].iter() {
                print(pair.1);
            }
        }
        "#,
        "1\na\nb\n",
    );
}

/// The same defect with no std beyond the loop protocol itself: a user's own
/// generically-elemented cursor. Neither std's nor the arc's — what the arc did
/// was make the shape reachable.
#[test]
fn a_protocol_loop_over_a_user_iterator_keeps_its_tuple_element() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Cursor<T> {
            items: List<T>,
            index: i32,
        }

        impl Cursor<type T> {
            fun next(&mut self): Option<T> {
                if self.index < self.items.len() {
                    let value = self.items[self.index];
                    self.index = self.index + 1;
                    Some(value)
                } else {
                    None
                }
            }
        }

        fun main() {
            mut pulled = Cursor { items = [(1, "a"), (2, "b")], index = 0 };
            if pulled.next() is Some(let pair) {
                print(pair.0);
            }
            mut looped = Cursor { items = [(1, "a"), (2, "b")], index = 0 };
            for pair in looped {
                print(pair.1);
            }
        }
        "#,
        "1\na\nb\n",
    );
}

/// Not a tuple defect — a BARE-PARAMETER defect. Every element form that a `T`
/// refuses went red the same way, so each gets a leg: a struct (field access),
/// a nested container (method call), an `Option` (method call on an enum), and
/// a closure (call-as-a-function, "cannot call this as a function: it is T").
#[test]
fn a_protocol_loop_keeps_every_element_form_through_a_generic() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Point { x: i32, y: i32 }

        fun main() {
            for point in [Point { x = 1, y = 2 }, Point { x = 3, y = 4 }].iter() {
                print(point.x);
            }
            for inner in [[1, 2, 3], [4]].iter() {
                print(inner.len());
            }
            for maybe in [Some(5), None].iter() {
                print(maybe.unwrap_or(-1));
            }
            for fn in [|n: i32| n + 1].iter() {
                print(fn(41));
            }
        }
        "#,
        "1\n3\n3\n1\n5\n-1\n42\n",
    );
}

/// A pattern match on the element never went red — an `is` against a bare `T`
/// checked VACUOUSLY and still ran, which is why an enum element looked
/// unaffected until a method was called on it. The pin is here so that
/// leniency, whatever it is worth, stays a decision and not an accident: the
/// element is a real `Shade` now and both arms still match.
#[test]
fn a_protocol_loop_element_matches_its_enum_variants() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Shade { Light, Dark(i32) }

        fun main() {
            for shade in [Shade::Dark(9), Shade::Light].iter() {
                if shade is Shade::Dark(let depth) { print(depth); }
                if shade is Shade::Light { print(0); }
            }
        }
        "#,
        "9\n0\n",
    );
}

/// The subject arm covers ENUMS as well as structs, and an enum-shaped iterator
/// reaches it by the same road (`Type::Enum(id, arguments)` -> the declared
/// parameters of `enums[id]`). Same "cannot access field '1' on type T" before.
#[test]
fn a_protocol_loop_keeps_an_enum_subjects_element_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        enum Feed<T> { Ready(List<T>, i32), Done }

        impl Feed<type T> {
            fun next(&mut self): Option<T> {
                if self is Feed::Ready(let items, let at) {
                    let pulled = if at < items.len() { Some(items[at]) } else { None };
                    let advanced = if at < items.len() {
                        Feed::Ready(items, at + 1)
                    } else {
                        Feed::Done
                    };
                    self = advanced;
                    pulled
                } else {
                    None
                }
            }
        }

        fun main() {
            mut feed = Feed::Ready([(1, "a"), (2, "b")], 0);
            for pair in feed {
                print(pair.0);
                print(pair.1);
            }
        }
        "#,
        "1\na\n2\nb\n",
    );
}

/// The `&mut` lending form drives `next_mut(&mut self): Option<&mut T>`
/// (`iterator-adapters.md` §7) through the same arm, so a GENERIC container
/// lent by view had the identical defect — the standing `next_mut` pins use a
/// concrete `Bag` and could not see it.
#[test]
fn a_mut_view_loop_keeps_its_element_type_through_a_generic() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Bag<T> { items: List<T>, cursor: i32 }

        impl Bag<type T> {
            fun next_mut(&mut self): Option<&mut T> {
                if self.cursor < self.items.len() {
                    let index = self.cursor;
                    self.cursor = self.cursor + 1;
                    Some(&mut self.items[index])
                } else {
                    None
                }
            }
        }

        fun main() {
            mut bag = Bag { items = [(1, "a"), (2, "b")], cursor = 0 };
            for pair in &mut bag {
                print(pair.1);
            }
        }
        "#,
        "a\nb\n",
    );
}

/// The element survives an adapter chain, where the subject is a `Filtered<..>`
/// whose own parameter is bound to the upstream's — one more instantiation hop
/// than the bare `.iter()` legs above.
#[test]
fn a_protocol_loop_keeps_a_tuple_element_through_an_adapter_chain() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            for pair in [(1, "a"), (2, "b"), (3, "c")].iter().filter(|p| p.0 > 1) {
                print(pair.1);
            }
            for pair in [(1, "a"), (2, "b")].iter().take(1) {
                print(pair.0);
            }
        }
        "#,
        "b\nc\n1\n",
    );
}

/// The regression guard on the forms that always worked, and the reason the
/// defect survived the whole arc: a STRUCTURAL payload projects even without
/// the instantiation, because only its parts are abstract.
#[test]
fn a_structurally_named_tuple_element_still_projects() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            for pair in ["a", "b"].iter().enumerate() {
                print(pair.0);
                print(pair.1);
            }
            for pair in [1, 2].iter().zip(["x", "y"].iter()) {
                print(pair.1);
            }
            for pair in [(9, "z")] {
                print(pair.0);
            }
        }
        "#,
        "0\na\n1\nb\nx\ny\n9\n",
    );
}

// --- B80: a concrete subject with no protocol member ------------------------
// Found while repairing B78's own pin, which named its protocol method `step`
// and therefore never reached the protocol at all. `for x in subject` over a
// CONCRETE struct or enum that has no `next` was not diagnosed — it lowered to
// a native `for...of`, which in JavaScript walks the receiver's flat FIELD
// array (a struct) or its `[tag, ..payload]` array (an enum), so the program
// printed the representation and exited 0.
//
// This is P3/B56's defect one type-shape over: B56 closed it for a GENERIC
// subject (`report_uniterable_for_each`), and the same reasoning applies
// verbatim to a concrete one. The rule the fix states is name AND shape — the
// protocol stays duck-typed on the METHOD NAME (`iterator-adapters.md` §1), but
// the subject must actually carry that method, and a `next` annotated with
// something other than `Option<T>` is rejected rather than driven.
//
// The deliberate native forms are exempt: an `external struct` (a host handle
// whose runtime shape is JavaScript's — `List`, `str`, `Bytes`, `NativeMap`)
// and `Set` (the one ordinary vilan struct with a lowering of its own,
// `__set_iter`). `[T; n]` and tuples never reach the arm.

/// B80's own repro, verbatim from the `#[ignore]`d pin B78's arc filed: before
/// the fix this printed `[ 1, 2 ]` and `0` — the two fields of `Cursor` — and
/// exited 0.
#[test]
fn a_for_loop_over_a_struct_without_next_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Cursor { items: List<i32>, index: i32 }

        fun main() {
            mut walked = Cursor { items = [1, 2], index = 0 };
            for item in walked {
                print(item);
            }
        }
        "#,
        "cannot iterate",
    );
}

/// The enum twin of the same hole, both shapes. A data-less `enum Color` lowers
/// to `[0]` (the representation rule is a CONJUNCTION — all-data-less AND
/// any-explicit-discriminant — so a bare data-less enum is still a tagged
/// array), and printed `0`; a payload variant `Shape::Circle(3)` is `[0, 3]`
/// and printed `0` then `3`.
#[test]
fn a_for_loop_over_an_enum_without_next_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        enum Color { Red, Green, Blue }

        fun main() {
            let shade = Color::Red;
            for value in shade {
                print(value);
            }
        }
        "#,
        "cannot iterate",
    );
    assert_fails_with(
        r#"
        import std::io::print;

        enum Shape { Circle(i32), Square(i32) }

        fun main() {
            let shape = Shape::Circle(3);
            for value in shape {
                print(value);
            }
        }
        "#,
        "cannot iterate",
    );
}

/// The protocol resolves on a MEMBER, and a field is not a member: a struct
/// with a field literally named `next` provides nothing to drive. Before the
/// fix this printed `5` then `7` — the field array, `next` included.
#[test]
fn a_field_named_next_does_not_satisfy_the_for_protocol() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Link { next: i32, value: i32 }

        fun main() {
            mut link = Link { next = 5, value = 7 };
            for item in link {
                print(item);
            }
        }
        "#,
        "cannot iterate",
    );
}

/// A fieldless struct is `[]` at runtime, so the fallback loop ran zero times
/// and exited 0 — silently doing nothing rather than silently doing the wrong
/// thing. It is the same missing member and gets the same diagnostic. (This is
/// also why the exemption reads the declared `external` modifier and not
/// "has no fields": a bodyless `external struct` and `struct Marker {}` are
/// indistinguishable by field count.)
#[test]
fn a_for_loop_over_a_field_less_struct_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Marker {}

        fun main() {
            mut marker = Marker {};
            for item in marker {
                print(item);
            }
        }
        "#,
        "cannot iterate",
    );
}

/// The name alone used to resolve the protocol, so a `next` DECLARED to return
/// anything else was driven anyway: the lowering reads the `Option` tag off the
/// result, `(5)[0]` is `undefined`, `undefined !== 0` breaks, and the loop ran
/// ZERO times and exited 0. The diagnostic names the return type it found.
#[test]
fn a_next_that_does_not_return_option_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Odd { count: i32 }

        impl Odd {
            fun next(&mut self): i32 {
                self.count += 1;
                self.count
            }
        }

        fun main() {
            mut odd = Odd { count = 0 };
            for item in odd {
                print(item);
            }
        }
        "#,
        "its `next` returns `i32`",
    );
}

/// The `&mut` form drives `next_mut`, so it is a separate member and a separate
/// diagnostic — including for a subject that has `next` and only `next`, which
/// is the sharper case: `for item in down` is legal and `for item in &mut down`
/// is not, and the message must name `next_mut` rather than `next`.
#[test]
fn a_mut_for_loop_over_a_subject_without_next_mut_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Cursor { items: List<i32>, index: i32 }

        fun main() {
            mut walked = Cursor { items = [1, 2], index = 0 };
            for item in &mut walked {
                print(item);
            }
        }
        "#,
        "it has no `next_mut(&mut self): Option<T>`",
    );
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        struct Down { left: i32 }

        impl Down {
            fun next(&mut self): Option<i32> {
                if self.left <= 0 { None } else { self.left = self.left - 1; Some(self.left) }
            }
        }

        fun main() {
            mut down = Down { left = 2 };
            for item in &mut down {
                print(item);
            }
        }
        "#,
        "it has no `next_mut(&mut self): Option<T>`",
    );
}

/// A `Map` is an ordinary vilan struct over a `NativeMap`, and it has no `next`
/// — so the fallback walked its one field and printed the backing map itself
/// (`Map(1) { 'a' => [ 'a', 1 ] }`), exit 0. It is now diagnosed, and the
/// message names the three documented ways to walk one.
#[test]
fn a_for_loop_over_a_map_is_diagnosed_and_names_its_accessors() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::map::Map;

        fun main() {
            mut scores: Map<str, i32> = Map::new();
            scores.insert("alice", 1);
            for entry in scores {
                print(entry);
            }
        }
        "#,
        "`entries()`, `keys()` or `values()`",
    );
}

/// The exemption set, end to end and at runtime: an `external struct` whose
/// runtime shape is the host's (`List` — a JS array; `str` — a JS string,
/// yielding characters; `Bytes` — a `Uint8Array`), `Set` (the `__set_iter`
/// lowering over the backing map's stored originals), and the two shapes that
/// never reach the struct/enum arm at all (`[T; n]` and a tuple). None of these
/// declares a `next`, and every one of them must keep iterating.
#[test]
fn the_deliberate_native_iteration_forms_still_iterate() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::set::Set;
        import std::bytes::{ Bytes, encode_utf8 };

        fun main() {
            for item in [1, 2] { print(item); }
            for character in "ab" { print(character); }
            let fixed: [i32; 2] = [3, 4];
            for item in fixed { print(item); }
            let pair = (5, 6);
            for item in pair { print(item); }
            mut seen: Set<i32> = Set::new();
            seen.insert(7);
            for item in seen { print(item); }
            for byte in encode_utf8("h") { print(byte); }
        }
        "#,
        "1\n2\na\nb\n3\n4\n5\n6\n7\n104\n",
    );
}

/// The positive control the whole check is measured against: a struct that DOES
/// declare `next(&mut self): Option<T>` drives the loop exactly as before,
/// whether the method is inherent or comes with an `Iterator` clause, and so
/// does `Range` (std's own). An unannotated `next` is not judged on its return
/// type — `IteratorFromFn::next` is written that way in std and infers its
/// `Option<T>` from its body — so `Iterator::from_fn` must keep working too.
#[test]
fn a_subject_that_declares_next_still_drives_the_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::range::Range;
        import std::iterator::Iterator;
        import std::option::{Option, Some, None};

        struct Countdown { remaining: i32 }
        impl Countdown with Iterator<i32> {
            fun next(&mut self): Option<i32> {
                if self.remaining <= 0 { None } else { self.remaining -= 1; Some(self.remaining) }
            }
        }

        struct Inherent { at: i32 }
        impl Inherent {
            fun next(&mut self): Option<i32> {
                if self.at >= 2 { None } else { self.at += 1; Some(self.at) }
            }
        }

        fun main() {
            mut countdown = Countdown { remaining = 2 };
            for value in countdown { print(value); }
            mut inherent = Inherent { at = 0 };
            for value in inherent { print(value); }
            for value in Range::new(0, 2) { print(value); }
            mut counted = 0;
            mut produced = Iterator::from_fn(|| {
                counted += 1;
                if counted > 2 { None } else { Some(counted) }
            });
            for value in produced { print(value); }
        }
        "#,
        "1\n0\n1\n2\n0\n1\n1\n2\n",
    );
}

// --- B91: an inherited trait default drives the loop (Gap E, at the `for`) ---
// A gap B80's check exposed rather than caused. A `next` INHERITED from a trait
// default is the receiver's `next` everywhere else — `empty.next()` resolves and
// returns `None`, and Gap E's `method_member_in_inherited_defaults` is what makes
// it resolve — but the loop asked `method_member_candidates`, which reads each
// impl's own `declarations`, and `impl Empty with Fixed<i32> {}` declares
// nothing. B80 turned the silent native `for...of` into a clean "cannot iterate",
// which was the right interim answer and the wrong final one: a protocol
// duck-typed on a method NAME cannot mean one thing at a call and another at a
// loop. The loop now falls back to the same tier the call does, dispatched to the
// concrete receiver at codegen through the same `GenericDispatch::OnType`.

/// B91's headline case, `#[ignore]`d until the fix: an impl that declares nothing
/// still iterates, through the default it inherited.
#[test]
fn a_next_inherited_from_a_trait_default_drives_the_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Fixed<T> { fun next(&mut self): Option<T> { None } }

        struct Empty { unused: i32 }
        impl Empty with Fixed<i32> {}

        fun main() {
            mut empty = Empty { unused = 0 };
            for item in empty { print(item); }
            print(99);
        }
        "#,
        "99\n",
    );
}

/// The default that actually YIELDS, which the empty one above cannot prove: the
/// inherited body calls back into the impl's own required member, so the loop has
/// to reach the default AND the default has to dispatch to this receiver. The
/// element type has to be right too — it is written in the TRAIT's `T`, bound by
/// the arguments the impl wrote in its `with` clause.
#[test]
fn an_inherited_default_that_yields_drives_the_loop_to_its_end() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Countdown<T> {
            fun tick(&mut self): Option<T>;
            fun next(&mut self): Option<T> { self.tick() }
        }

        struct Down { at: i32 }
        impl Down with Countdown<i32> {
            fun tick(&mut self): Option<i32> {
                if self.at <= 0 { None } else { self.at -= 1; Some(self.at) }
            }
        }

        fun main() {
            mut down = Down { at = 3 };
            for item in down { print(item + 1); }
            print(99);
        }
        "#,
        "3\n2\n1\n99\n",
    );
}

/// A GENERIC subject, where one substitution step is not enough: `impl Bag<type
/// T> with Feed<T>` maps the trait's `T` onto the impl's BINDER, which is still
/// abstract, so the element instantiates only after the receiver binds the binder.
/// `pair.1` is the probe — with the trait's arguments alone it was "cannot access
/// field '1' on type T", the B78 shape one tier over.
#[test]
fn an_inherited_default_on_a_generic_subject_keeps_its_element_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Feed<T> {
            fun take(&mut self): Option<T>;
            fun next(&mut self): Option<T> { self.take() }
        }

        struct Bag<T> { items: List<T>, cursor: i32 }
        impl Bag<type T> with Feed<T> {
            fun take(&mut self): Option<T> {
                if self.cursor < self.items.len() {
                    let index = self.cursor;
                    self.cursor += 1;
                    Some(self.items[index])
                } else {
                    None
                }
            }
        }

        fun main() {
            mut bag = Bag { items = [(1, "a"), (2, "b")], cursor = 0 };
            for pair in bag { print(pair.1); }
        }
        "#,
        "a\nb\n",
    );
}

/// The `next_mut` twin, lending each element by writable view — a separate member
/// and a separate lookup, so it gets its own pin (the `a_mut_for_loop_over_a_
/// subject_without_next_mut_is_diagnosed` precedent). The writes land in the
/// receiver, which is what makes it the `next_mut` protocol rather than `next`.
#[test]
fn a_next_mut_inherited_from_a_trait_default_drives_a_mut_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Walk<T> {
            fun step(&mut self): Option<&mut T>;
            fun next_mut(&mut self): Option<&mut T> { self.step() }
        }

        struct Bag2 { items: List<i32>, cursor: i32 }
        impl Bag2 with Walk<i32> {
            fun step(&mut self): Option<&mut i32> {
                if self.cursor < self.items.len() {
                    let index = self.cursor;
                    self.cursor += 1;
                    Some(&mut self.items[index])
                } else {
                    None
                }
            }
        }

        fun main() {
            mut bag = Bag2 { items = [1, 2, 3], cursor = 0 };
            for item in &mut bag { item = *item * 10; }
            print(bag.items[0]);
            print(bag.items[2]);
        }
        "#,
        "10\n30\n",
    );
}

/// B57's tiering, unchanged by the new tier: an inherent `next` beside an
/// inherited default one is not ambiguous — inherent wins, unconditionally
/// (`method-resolution.md` §3). The loop consults the inherited tier only when
/// the declared search comes back empty, which is the same order the call path
/// uses, so the two cannot disagree about which body runs.
#[test]
fn an_inherent_next_beats_an_inherited_default_at_the_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Fixed<T> { fun next(&mut self): Option<T> { None } }

        struct Two { at: i32 }
        impl Two {
            fun next(&mut self): Option<i32> {
                if self.at >= 2 { None } else { self.at += 1; Some(self.at) }
            }
        }
        impl Two with Fixed<i32> {}

        fun main() {
            mut two = Two { at = 0 };
            for item in two { print(item); }
            print(99);
        }
        "#,
        "1\n2\n99\n",
    );
}

/// Two traits offering same-named DEFAULTS are as ambiguous as two declaring the
/// name outright (§3, one tier down), and the loop must not silently pick one.
/// The call form resolves this by naming a provider — `Trait::next(receiver)` —
/// and a `for` has no such spelling, so the steer is the edit that works: declare
/// it inherently, the tier that beats both (B65's lesson, as B83 applied it).
#[test]
fn two_inherited_default_nexts_are_ambiguous_at_the_loop() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Early<T> { fun next(&mut self): Option<T> { None } }
        trait Late<T> { fun next(&mut self): Option<T> { None } }

        struct Both { at: i32 }
        impl Both with Early<i32> {}
        impl Both with Late<i32> {}

        fun main() {
            mut both = Both { at = 0 };
            for item in both { print(item); }
        }
        "#,
        "`next` is ambiguous on `Both`",
    );
}

// --- B96: two traits DECLARING `next` are ambiguous at the loop too ---
// The same ambiguity one tier UP from B91's. `method_member_impl_subject`
// collapses `AmbiguousTraits` into `None`, so the declared tier fell through to
// the inherited one and out the "it has no `next`" exit — of a type that has
// two. The loop now asks `resolve_impl_member`, which keeps the two failures
// apart, and reports at whichever tier the competition is in.

/// Two traits DECLARING `next`, with an impl of each providing a body: the
/// call path already reports this ("both 'A' and 'B' provide it; call
/// 'A::next(receiver)' …"), and the loop must too — with the steer that works
/// for a `for`, which has no spelling that names a provider.
#[test]
fn two_declared_nexts_are_ambiguous_at_the_loop() {
    let two_declared = r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Early<T> { fun next(&mut self): Option<T>; }
        trait Late<T> { fun next(&mut self): Option<T>; }

        struct Twin { at: i32 }
        impl Twin with Early<i32> { fun next(&mut self): Option<i32> { None } }
        impl Twin with Late<i32> { fun next(&mut self): Option<i32> { None } }

        fun main() {
            mut twin = Twin { at = 0 };
            for item in twin { print(item); }
        }
        "#;
    // Half one: both providers named, at the DECLARED tier's wording.
    assert_fails_with(
        two_declared,
        "`next` is ambiguous on `Twin`: both 'Early<i32>' and 'Late<i32>' provide it, and a",
    );
    // Half two: the inherent-declaration steer — the edit that resolves it.
    assert_fails_with(
        two_declared,
        "Declare `next` on `Twin` itself — an inherent member beats every trait-provided one",
    );
    // And NOT the misleading message it used to get (B96's whole point).
    assert_fails_without(two_declared, "it has no `next");
}

/// The two tiers say the same thing about the same problem, differing only in
/// how the providers give the member: B91's ambiguity and B96's share the
/// providers clause and the steer verbatim, over the same struct name. An
/// inconsistency here is exactly what made B96 findable.
#[test]
fn both_ambiguous_for_each_tiers_share_one_diagnostic_shape() {
    let declared = r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Early<T> { fun next(&mut self): Option<T>; }
        trait Late<T> { fun next(&mut self): Option<T>; }

        struct Twin { at: i32 }
        impl Twin with Early<i32> { fun next(&mut self): Option<i32> { None } }
        impl Twin with Late<i32> { fun next(&mut self): Option<i32> { None } }

        fun main() {
            mut twin = Twin { at = 0 };
            for item in twin { print(item); }
        }
        "#;
    let inherited = r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Early<T> { fun next(&mut self): Option<T> { None } }
        trait Late<T> { fun next(&mut self): Option<T> { None } }

        struct Twin { at: i32 }
        impl Twin with Early<i32> {}
        impl Twin with Late<i32> {}

        fun main() {
            mut twin = Twin { at = 0 };
            for item in twin { print(item); }
        }
        "#;
    let shared_providers = "`next` is ambiguous on `Twin`: both 'Early<i32>' and 'Late<i32>' \
                            provide it";
    let shared_steer = ", and a `for` loop has no spelling that names one. Declare `next` on \
                        `Twin` itself — an inherent member beats every trait-provided one";
    for source in [declared, inherited] {
        assert_fails_with(source, shared_providers);
        assert_fails_with(source, shared_steer);
    }
    // The one difference: the inherited tier names its tier, because "provide"
    // alone would send the reader looking for a declaration that is not there.
    assert_fails_with(inherited, "provide it as an inherited default, and a");
    assert_fails_without(declared, "as an inherited default");
}

/// B57's tiering, unchanged one tier up: an inherent `next` beside TWO traits
/// declaring the name is not ambiguous — inherent wins, unconditionally
/// (`method-resolution.md` §3), and the loop stays silent about the traits.
#[test]
fn an_inherent_next_beats_two_declared_trait_nexts_at_the_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Early<T> { fun next(&mut self): Option<T>; }
        trait Late<T> { fun next(&mut self): Option<T>; }

        struct Three { at: i32 }
        impl Three {
            fun next(&mut self): Option<i32> {
                if self.at >= 2 { None } else { self.at += 1; Some(self.at) }
            }
        }
        impl Three with Early<i32> { fun next(&mut self): Option<i32> { None } }
        impl Three with Late<i32> { fun next(&mut self): Option<i32> { None } }

        fun main() {
            mut three = Three { at = 0 };
            for item in three { print(item); }
            print(99);
        }
        "#,
        "1\n2\n99\n",
    );
}

/// ONE trait declaring `next` is not made ambiguous by a second trait the type
/// also implements: the ambiguity is over the NAME's providers, not over the
/// impl count. The lower boundary of the new report.
#[test]
fn one_declared_next_beside_an_unrelated_trait_still_drives_the_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Only<T> { fun next(&mut self): Option<T>; }
        trait Named { fun label(&self): str; }

        struct One { at: i32 }
        impl One with Only<i32> {
            fun next(&mut self): Option<i32> {
                if self.at >= 2 { None } else { self.at += 1; Some(self.at) }
            }
        }
        impl One with Named { fun label(&self): str { "one" } }

        fun main() {
            mut one = One { at = 0 };
            for item in one { print(item); }
            print(one.label());
        }
        "#,
        "1\n2\none\n",
    );
}

/// The `next_mut` twin: `for x in &mut subject` looks up a different member
/// through the same tiering, so the ambiguity — and the steer naming the right
/// member — has to reach it as well (the B91 `next_mut` pin's precedent).
#[test]
fn two_declared_next_muts_are_ambiguous_at_the_mut_loop() {
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait WalkA<T> { fun next_mut(&mut self): Option<&mut T>; }
        trait WalkB<T> { fun next_mut(&mut self): Option<&mut T>; }

        struct Bag3 { items: List<i32>, cursor: i32 }
        impl Bag3 with WalkA<i32> { fun next_mut(&mut self): Option<&mut i32> { None } }
        impl Bag3 with WalkB<i32> { fun next_mut(&mut self): Option<&mut i32> { None } }

        fun main() {
            mut bag = Bag3 { items = [1, 2], cursor = 0 };
            for item in &mut bag { item = *item * 10; }
        }
        "#,
        "`next_mut` is ambiguous on `Bag3`: both 'WalkA<i32>' and 'WalkB<i32>' provide it, \
         and a `for` loop has no spelling that names one. Declare `next_mut` on `Bag3` \
         itself — an inherent member beats every trait-provided one",
    );
}

/// The tier's boundaries, both inherited from Gap E's own rules rather than
/// re-decided here. B80's declared-shape check applies to an inherited default
/// exactly as to a declared member — the lowering reads an `Option` tag off
/// whatever comes back, whoever wrote it. And a `[trait_only]` default is not
/// inherited onto the concrete surface at all (§3.2), so it does not make the
/// receiver iterable.
#[test]
fn an_inherited_default_next_keeps_the_tiers_own_boundaries() {
    assert_fails_with(
        r#"
        import std::io::print;

        trait Bad { fun next(&mut self): i32 { 0 } }

        struct Nope { at: i32 }
        impl Nope with Bad {}

        fun main() {
            mut nope = Nope { at = 0 };
            for item in nope { print(item); }
        }
        "#,
        "its `next` returns `i32`",
    );
    assert_fails_with(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Hidden<T> { [trait_only] fun next(&mut self): Option<T> { None } }

        struct H { at: i32 }
        impl H with Hidden<i32> {}

        fun main() {
            mut h = H { at = 0 };
            for item in h { print(item); }
        }
        "#,
        "it has no `next(&mut self): Option<T>`",
    );
}

/// A default inherited through a SUPERTRAIT, which is the same tier reached one
/// hop further out: `impl Q with Derived<i32>` never names `Base` at all, and
/// `method_member_in_trait` walks the supertrait closure to find the body.
#[test]
fn a_supertraits_default_next_drives_the_loop_too() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        trait Base<T> { fun next(&mut self): Option<T> { None } }
        trait Derived<T> with Base<T> {}

        struct Q { at: i32 }
        impl Q with Derived<i32> {}

        fun main() {
            mut q = Q { at = 0 };
            for item in q { print(item); }
            print(9);
        }
        "#,
        "9\n",
    );
}

// --- B92: an unannotated `next` is read from its body, not waved through -----
// B80 judged a `next` by its ANNOTATION and deliberately left the unannotated
// half alone, because `IteratorFromFn::next` in std is written
// `fun next(&mut self) { (self.fn)() }` and had to stay legal. But "unannotated"
// was never the reason it is legal — its BODY yields an `Option<T>`, which is
// what reading the body says. A body that yields nothing infers `void`, reached
// the lowering, and `undefined[0]` threw `TypeError` at runtime: loud rather
// than silent, so not B80's class, but still the compiler's job. One rule now
// covers both spellings; the annotation only decides where the answer is read.

/// B92's headline case, `#[ignore]`d until the fix: a `next` whose body yields
/// nothing at all.
#[test]
fn an_unannotated_next_that_yields_nothing_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Odd { count: i32 }

        impl Odd {
            fun next(&mut self) {
                self.count += 1;
            }
        }

        fun main() {
            mut odd = Odd { count = 0 };
            for item in odd {
                print(item);
            }
        }
        "#,
        "its `next` is unannotated and its body yields `void`",
    );
}

/// The carve-out, proven rather than asserted: `Iterator::from_fn`'s `next` is
/// unannotated BY DESIGN and stays legal, because its body yields an `Option`.
/// This is the pin that would go red if the rule were "unannotated is an error"
/// instead of "read the body", so it is the one that keeps std compiling.
#[test]
fn an_unannotated_next_that_yields_an_option_stays_legal() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut counted = 0;
            let produced = Iterator::from_fn(|| {
                counted += 1;
                if counted > 3 { None } else { Some(counted) }
            });
            for value in produced { print(value); }
        }
        "#,
        "1\n2\n3\n",
    );
    // The same shape written by hand, so the carve-out is the RULE and not a
    // std-shaped exemption: a user `next` with no annotation whose body yields
    // an `Option` drives the loop exactly as an annotated one does.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Two { at: i32 }
        impl Two {
            fun next(&mut self) {
                if self.at >= 2 { None } else { self.at += 1; Some(self.at) }
            }
        }

        fun main() {
            mut two = Two { at = 0 };
            for item in two { print(item); }
            print(9);
        }
        "#,
        "1\n2\n9\n",
    );
}

/// The rule is the SHAPE, not the emptiness: an unannotated `next` that yields a
/// perfectly good `i32` is the same error, and it is B80's silent case rather
/// than B92's loud one — `number[0]` is `undefined`, `undefined !== 0` breaks, so
/// the loop ran zero times and exited 0. Reading the body closes both at once.
#[test]
fn an_unannotated_next_that_yields_a_non_option_is_diagnosed_too() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Num { at: i32 }
        impl Num {
            fun next(&mut self) { self.at += 1; self.at }
        }

        fun main() {
            mut num = Num { at = 0 };
            for item in num { print(item); }
        }
        "#,
        "its `next` is unannotated and its body yields `i32`",
    );
}

/// The check reaches an INHERITED default too (B91's new tier), which it must:
/// the loop drives whatever body it resolved to, and where that body came from
/// cannot change what the lowering reads off the result.
#[test]
fn an_unannotated_inherited_default_next_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        trait Blank { fun next(&mut self) { } }

        struct Empty3 { at: i32 }
        impl Empty3 with Blank {}

        fun main() {
            mut empty = Empty3 { at = 0 };
            for item in empty { print(item); }
        }
        "#,
        "its `next` is unannotated and its body yields `void`",
    );
}

/// The ANNOTATED half keeps its own wording, so the two diagnostics stay
/// distinguishable: one sends you to the annotation, the other to the body it
/// was read from. B80's pins cover the behaviour; this pins the split.
#[test]
fn an_annotated_non_option_next_keeps_its_own_diagnostic() {
    assert_fails_with(
        r#"
        import std::io::print;

        struct Num2 { at: i32 }
        impl Num2 {
            fun next(&mut self): i32 { self.at += 1; self.at }
        }

        fun main() {
            mut num = Num2 { at = 0 };
            for item in num { print(item); }
        }
        "#,
        "its `next` returns `i32`",
    );
}

// --- B82: an enum-variant pattern against a bare generic parameter ----------
// `resolve_pattern` waved a `Type::Generic` scrutinee through beside `Unknown`
// and `Any`, so NOTHING was checked and the tag test was emitted anyway. The
// pattern then matched by coincidence of representation, which is silent wrong
// code, not a missing diagnostic: a `List<i32>` `[0, 7]` "matched"
// `Shape::Circle(let radius)` and bound `radius = 7`, and a trait default
// written over its own `T` matched `Colour::Red` against a `Fruit::Apple(9)`.
//
// Whether it is fixable AT the pattern turns on WHICH parameter it is, and the
// evidence for the split is `std::ui::View::swap` — see the `#[ignore]`d pin at
// the end of this block. A parameter declared by a scope ENCLOSING the pattern
// is abstract by construction (the declaration is checked once for all of its
// instantiations), so it is an error. One that arrived from elsewhere means
// only "not substituted yet", and the free-function twin already substitutes.

/// B82's core case: a generic function matching on its own `T`-typed parameter.
/// Every arm of the family is silent wrong code at runtime, not merely
/// unchecked — `probe([0, 7])` returned `7` and `probe(Fruit::Apple("x"))`
/// returned the *string* `"x"` from a function declared `: i32`.
#[test]
fn an_enum_pattern_on_a_functions_own_type_parameter_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        enum Shape { Circle(i32), Square(i32) }

        fun probe<T>(value: T): i32 {
            if value is Shape::Circle(let radius) { radius } else { -1 }
        }

        fun main() { print(probe(Shape::Circle(5))); }
        "#,
        "cannot match an enum variant against the generic parameter `T`",
    );
}

/// The `match` form of the same thing — the pattern resolver is shared, and the
/// pin exists so a future change to one form cannot quietly leave the other.
#[test]
fn an_enum_match_arm_on_a_functions_own_type_parameter_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }

        fun probe<T>(value: T): i32 {
            match value {
                Route::Home => 0,
                Route::Away(let id) => id,
            }
        }

        fun main() { print(probe(Route::Away(4))); }
        "#,
        "cannot match an enum variant against the generic parameter `T`",
    );
}

/// A trait's own parameter inside a DEFAULT body, which is the sharpest proof
/// that the abstract check can never be right: the body below is written once
/// and instantiated at two different enums, so `value is Colour::Red` cannot
/// have one answer. It had one anyway — `Fruit::Apple(9)` is `[0, 9]`, the tag
/// test `[0] === 0` passed, and `Basket.describe()` returned 1 where every
/// reading of the source says 0.
#[test]
fn an_enum_pattern_on_a_traits_own_parameter_in_a_default_is_diagnosed() {
    assert_fails_with(
        r#"
        import std::io::print;

        enum Colour { Red, Green }
        enum Fruit { Apple(i32), Pear(i32) }

        trait Tell<T> {
            fun payload(self): T;
            fun describe(self): i32 {
                let value = self.payload();
                if value is Colour::Red { 1 } else { 0 }
            }
        }

        struct Holder { at: i32 }
        impl Holder with Tell<Colour> { fun payload(self): Colour { Colour::Red } }

        struct Basket { at: i32 }
        impl Basket with Tell<Fruit> { fun payload(self): Fruit { Fruit::Apple(9) } }

        fun main() {
            print(Holder { at = 0 }.describe());
            print(Basket { at = 0 }.describe());
        }
        "#,
        "cannot match an enum variant against the generic parameter `T`",
    );
}

/// A BOUND does not rescue it, which is why the message does not suggest adding
/// one: a trait bound cannot make a parameter be one particular enum, and the
/// bounded `T` below is exactly as instantiable at some other implementor.
#[test]
fn a_bound_does_not_make_a_type_parameter_matchable() {
    assert_fails_with(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        trait Tag { fun tag(self): i32; }
        impl Route with Tag { fun tag(self): i32 { 1 } }

        fun probe<T: Tag>(value: T): i32 {
            if value is Route::Away(let id) { id } else { 0 }
        }

        fun main() { print(probe(Route::Away(4))); }
        "#,
        "cannot match an enum variant against the generic parameter `T`",
    );
}

/// The two concrete diagnostics the generic one now sits beside, neither of
/// which had a pin: a scrutinee that is a different enum, and one that is not
/// an enum at all. They are the reason the generic case reads as a hole rather
/// than a policy — the check exists, it just had nothing to run against.
#[test]
fn a_concrete_mismatched_subject_keeps_its_own_pattern_diagnostics() {
    assert_fails_with(
        r#"
        import std::io::print;

        enum Colour { Red, Green }
        enum Fruit { Apple, Pear }

        fun main() {
            let value = Fruit::Apple;
            if value is Colour::Red { print(1); } else { print(0); }
        }
        "#,
        "variant 'Colour::Red' does not belong to the matched enum",
    );
    assert_fails_with(
        r#"
        import std::io::print;

        enum Colour { Red, Green }

        fun main() {
            let value: i32 = 5;
            if value is Colour::Red { print(1); } else { print(0); }
        }
        "#,
        "cannot match an enum variant against type i32",
    );
}

/// What must keep working, and does: an enum PARAMETERIZED by a type parameter
/// is a `Type::Enum`, not a `Type::Generic`, so `Some(let value)` on an
/// `Option<T>` inside a generic function is untouched — std's own bodies are
/// full of it (`Set::to_set`, `View::swap`'s `last_value.read()`). So is an
/// enum matching its own variants inside its own `impl`, where `self` is the
/// abstract enum rather than a parameter. And a LITERAL pattern against a `T`
/// is a different arm entirely, checked by `compare_type` and sound at runtime
/// (a JS `===` against a number cannot be fooled by a string).
#[test]
fn matching_through_a_type_parameter_still_works_where_it_is_sound() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::{Option, Some, None};

        enum Route { Home, Away(i32) }

        impl Route {
            fun code(self): i32 {
                if self is Route::Away(let id) { id } else { 0 }
            }
        }

        fun first_or<T>(values: List<T>, fallback: T): T {
            match values.get(0) {
                Some(let value) => value,
                None => fallback,
            }
        }

        fun literal<T>(value: T): i32 {
            match value {
                1 => 10,
                _ => 30,
            }
        }

        fun main() {
            print(first_or([5, 6], 0));
            print(first_or(["a"], "z"));
            print(Route::Away(3).code());
            print(Route::Home.code());
            print(literal(1));
            print(literal("x"));
        }
        "#,
        "5\na\n3\n0\n10\n30\n",
    );
}

/// The evidence that decided B82's shape. A closure argument to a FREE
/// function's generic reaches its body with the parameter already substituted,
/// so the match inside is checked for real — the wrong enum is rejected by the
/// pre-existing concrete diagnostic, with no help from B82's arm.
#[test]
fn a_closure_argument_to_a_free_functions_generic_gets_the_real_check() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }

        fun apply<T>(value: T, render: |T| i32): i32 { render(value) }

        fun main() {
            print(apply(Route::Away(7), |current| match current {
                Route::Home => 0,
                Route::Away(let id) => id,
            }));
        }
        "#,
        "7\n",
    );
    assert_fails_with(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        enum Other { First, Second(i32) }

        fun apply<T>(value: T, render: |T| i32): i32 { render(value) }

        fun main() {
            print(apply(Route::Away(7), |current| match current {
                Other::First => 0,
                Other::Second(let id) => id,
            }));
        }
        "#,
        "variant 'Other::First' does not belong to the matched enum",
    );
}

// --- B90: a closure argument to a generic's parameter, substituted ----------
// B82's residual half, and NOT a pattern-checker gap: the closure's parameter
// reached its body still typed as the abstract `T`, so the match inside was
// checked against nothing and `Other::Second`'s payload came out of a
// `Route::Away`, silently. The root cause is one-shot-ness. An unannotated
// closure parameter's type slot is filled only while it is `Unknown`, so
// whoever writes it first wins forever — and both call paths were willing to
// write it from a substitution they knew to be incomplete:
//
//   * the METHOD path bound the callee's own generics from the non-closure
//     arguments, typed the closure arguments, and only THEN decided to defer
//     because an argument's type had not landed. On the attempt where
//     `Route::Away(7)` was still `Unresolved`, `T` was unbound, `render: |T|
//     i32` typed `current` as the abstract `T`, and the retry that finally knew
//     `T = Route` found the slot already taken.
//   * the FREE path walks its parameters positionally and defers at the first
//     `Unresolved` argument, which is why `apply(value, render)` was right —
//     but `apply(render, value)`, with the closure standing FIRST, hit the same
//     wall for the same reason.
//
// So the fix is one rule in both places: bind the own generics from the
// non-closure arguments, and defer BEFORE typing any closure while an
// argument's type has not landed. `bind_callee_own_generics` is now shared,
// differing only by whether the parameter list starts with `self`.

/// B90's headline case, `#[ignore]`d until the fix: the wrong enum inside a
/// closure passed to a METHOD's own generic. `std::ui::View::swap` is exactly
/// this shape, and the routing guide's `swap(route, |current| match current {
/// .. })` is the documented, shipped use — which is why B82 refused to make a
/// `Type::Generic` scrutinee a blanket error (probed then: 3 diagnostics in
/// `docs/guide/routing.md`, 2 in `docs/guide/dev-loop.md`) and left this to
/// instantiation instead.
#[test]
fn a_closure_argument_to_a_methods_generic_gets_the_real_check() {
    assert_fails_with(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        enum Other { First, Second(i32) }

        struct Holder { tag: i32 }
        impl Holder {
            fun apply<T>(self, value: T, render: |T| i32): i32 { render(value) }
        }

        fun main() {
            print(Holder { tag = 0 }.apply(Route::Away(7), |current| match current {
                Other::First => 0,
                Other::Second(let id) => id,
            }));
        }
        "#,
        "does not belong to the matched enum",
    );
    // The half that must keep working — the RIGHT enum, through the same
    // method. A diagnostic that also rejects this would be no fix at all.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }

        struct Holder { tag: i32 }
        impl Holder {
            fun apply<T>(self, value: T, render: |T| i32): i32 { render(value) }
        }

        fun main() {
            print(Holder { tag = 0 }.apply(Route::Away(7), |current| match current {
                Route::Home => 0,
                Route::Away(let id) => id,
            }));
        }
        "#,
        "7\n",
    );
}

/// The substitution reaches the closure parameter as a TYPE, not just as a
/// pattern scrutinee: a method call on it resolves against the binding. This is
/// the sharper probe of the two — a pattern can match by coincidence of
/// representation, but `current.code()` either resolves or does not, and before
/// the fix it did not ("cannot call method 'code' on T").
#[test]
fn a_closure_parameter_typed_by_a_methods_generic_reaches_its_methods() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        impl Route {
            fun code(self): i32 { if self is Route::Away(let id) { id } else { 0 } }
        }

        struct Holder { tag: i32 }
        impl Holder {
            fun apply<T>(self, value: T, render: |T| i32): i32 { render(value) }
        }

        fun main() {
            print(Holder { tag = 0 }.apply(Route::Away(7), |current| current.code()));
            print(Holder { tag = 0 }.apply(Route::Home, |current| current.code()));
        }
        "#,
        "7\n0\n",
    );
}

/// The ordering edge, both paths. The argument that FIXES the generic used to
/// have to stand before the closure that consumes it, because the free path
/// walks parameters positionally; the method path's two-phase binding never had
/// that constraint and now the free path shares it. `apply(render, value)` is
/// the same call as `apply(value, render)`.
#[test]
fn a_closure_argument_is_substituted_before_the_argument_that_fixes_the_generic() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        impl Route {
            fun code(self): i32 { if self is Route::Away(let id) { id } else { 0 } }
        }

        fun free<T>(render: |T| i32, value: T): i32 { render(value) }

        struct Holder { tag: i32 }
        impl Holder {
            fun apply<T>(self, render: |T| i32, value: T): i32 { render(value) }
        }

        fun main() {
            print(free(|current| current.code(), Route::Away(4)));
            print(Holder { tag = 0 }.apply(|current| match current {
                Route::Home => 0,
                Route::Away(let id) => id,
            }, Route::Away(7)));
        }
        "#,
        "4\n7\n",
    );
    // And the wrong enum is still caught with the arguments in that order.
    assert_fails_with(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        enum Other { First, Second(i32) }

        fun free<T>(render: |T| i32, value: T): i32 { render(value) }

        fun main() {
            print(free(|current| match current {
                Other::First => 0,
                Other::Second(let id) => id,
            }, Route::Away(7)));
        }
        "#,
        "does not belong to the matched enum",
    );
}

/// The callee shapes that share the two-phase rule: a STATIC (no `self`, so the
/// free path's offset), a trait DEFAULT reached through an impl that declares
/// nothing (Gap E's dispatch, whose parameters are written in the trait's terms),
/// and a method whose generic comes from the impl's binder rather than its own
/// list. Each was a separate route to the same abstract-`T` parameter.
#[test]
fn every_callee_shape_substitutes_its_closure_arguments_parameter() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        impl Route {
            fun code(self): i32 { if self is Route::Away(let id) { id } else { 0 } }
        }

        struct Holder { tag: i32 }
        impl Holder {
            fun statically<T>(value: T, render: |T| i32): i32 { render(value) }
        }

        trait Applier {
            fun apply<T>(self, value: T, render: |T| i32): i32 { render(value) }
        }
        impl Holder with Applier {}

        struct Boxed<T> { held: T }
        impl Boxed<type T> {
            fun mapped(self, render: |T| i32): i32 { render(self.held) }
        }

        fun main() {
            print(Holder::statically(Route::Away(1), |current| current.code()));
            print(Holder { tag = 0 }.apply(Route::Away(2), |current| current.code()));
            print(Boxed { held = Route::Away(3) }.mapped(|current| current.code()));
        }
        "#,
        "1\n2\n3\n",
    );
}

/// Nested closures: the inner call's own generic binds from a value whose type
/// is itself the outer closure's parameter, so the outer substitution has to
/// have landed before the inner one is attempted. Both parameters used to reach
/// their bodies abstract.
#[test]
fn a_closure_nested_in_a_closure_argument_is_substituted_too() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        impl Route {
            fun code(self): i32 { if self is Route::Away(let id) { id } else { 0 } }
        }

        struct Holder { tag: i32 }
        impl Holder {
            fun apply<T>(self, value: T, render: |T| i32): i32 { render(value) }
        }

        fun main() {
            print(Holder { tag = 0 }.apply(Route::Away(7), |current|
                Holder { tag = 1 }.apply(current, |inner| inner.code())));
        }
        "#,
        "7\n",
    );
}

/// The closure's RETURN type is substituted by the same binding, and checked.
/// `step: |T| T` under `T = Route` is BOTH halves at once — the parameter the
/// body reads through and the value it must hand back — and they travel through
/// one `substitute_type` `Type::Closure` arm, so this pins that they cannot
/// drift apart. (The one-sided `|i32| T` shape was never broken: its parameter
/// is concrete, so the one-shot slot had nothing abstract to freeze at.)
#[test]
fn a_closure_arguments_return_type_is_checked_against_the_generics_binding() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        impl Route {
            fun code(self): i32 { if self is Route::Away(let id) { id } else { 0 } }
        }

        struct Holder { tag: i32 }
        impl Holder {
            fun twice<T>(self, seed: T, step: |T| T): T { step(step(seed)) }
        }

        fun main() {
            print(Holder { tag = 0 }
                .twice(Route::Away(1), |current| Route::Away(current.code() + 1))
                .code());
        }
        "#,
        "3\n",
    );
    assert_fails_with(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        enum Other { First, Second(i32) }

        struct Holder { tag: i32 }
        impl Holder {
            fun twice<T>(self, seed: T, step: |T| T): T { step(step(seed)) }
        }

        fun main() {
            let made = Holder { tag = 0 }.twice(Route::Away(1), |current| Other::Second(3));
            print(1);
        }
        "#,
        // B132 narrowed the anchor: the bound target (`T = Route`) is ground,
        // so the bare body takes S3's return-position route and the mismatch
        // reports ON the expression in the binding's terms, no longer as the
        // whole closure value (`Expected |Route| Route, but got |Route| Other`).
        "Expected Route, but got Other instead.",
    );
    assert_fails_spanning(
        r#"
        import std::io::print;

        enum Route { Home, Away(i32) }
        enum Other { First, Second(i32) }

        struct Holder { tag: i32 }
        impl Holder {
            fun twice<T>(self, seed: T, step: |T| T): T { step(step(seed)) }
        }

        fun main() {
            let made = Holder { tag = 0 }.twice(Route::Away(1), |current| Other::Second(3));
            print(1);
        }
        "#,
        "Other::Second(3)",
        "Expected Route, but got Other instead.",
    );
}

// --- I3 S5: the terminations (proposal/iterator-adapters.md §5, §6) ----------
// The EXPLICIT family is the primary termination API and there is no `collect`:
// a method that names what it builds needs no annotation, reads at the call
// site, and composes with a chain. `to_list`/`fold`/`for_each`/`count`/`any`/
// `all`/`rev` are trait defaults; `to_set`/`to_map` are bounded `List` methods
// (see `to_set_and_to_map_live_on_list_because_a_default_cannot_carry_a_bound`).

#[test]
fn to_list_pulls_a_chain_into_a_list() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                let doubled = [1, 2, 3].iter().map(|n| n * 2).to_list();
                print(doubled.len());
                print(doubled[0]);
                print(doubled[2]);
                let empty: List<i32> = [];
                print(empty.iter().to_list().len());
                print([7].iter().to_list()[0]);
            }
            "#,
        ),
        "3\n2\n6\n0\n7\n",
    );
}

#[test]
fn to_list_composes_out_of_a_chain_where_an_inferred_collect_would_not() {
    // §5's argument, made executable: the explicit terminal is usable in the
    // middle of an expression, which is the shape a pipeline invites and the
    // one `it.collect().len()` cannot resolve.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                print([1, 2, 3, 4].iter().filter(|n| n % 2 == 0).to_list().len());
            }
            "#,
        ),
        "2\n",
    );
}

#[test]
fn to_list_over_an_unbounded_source_is_bounded_by_take() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                let first = Naturals { at = 0 }.take(4).to_list();
                print(first.len());
                print(first[3]);
            }
            "#,
        ),
        "4\n4\n",
    );
}

#[test]
fn fold_combines_left_to_right_from_its_seed() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                print([1, 2, 3].iter().fold(0, |total, n| total + n));
                print([1, 2, 3].iter().fold(100, |total, n| total - n));
                let empty: List<i32> = [];
                print(empty.iter().fold(42, |total, n| total + n));
                print([1, 2, 3].iter().fold("", |text, n| text + i"{n}"));
            }
            "#,
        ),
        "6\n94\n42\n123\n",
    );
}

#[test]
fn for_each_runs_its_closure_once_per_value_in_order() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                [1, 2, 3].iter().filter(|n| n != 2).for_each(|n| print(n));
                let empty: List<i32> = [];
                empty.iter().for_each(|n| print(n));
                print("done");
            }
            "#,
        ),
        "1\n3\ndone\n",
    );
}

#[test]
fn count_counts_what_reaches_it() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                print([1, 2, 3, 4].iter().filter(|n| n > 2).count());
                let empty: List<i32> = [];
                print(empty.iter().count());
                print([7].iter().count());
                print(Naturals { at = 0 }.take(5).count());
            }
            "#,
        ),
        "2\n0\n1\n5\n",
    );
}

#[test]
fn any_short_circuits_on_the_first_hit() {
    // The short-circuit is what lets `any` answer over an unbounded source —
    // a version that drained first would not return at all.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                print([1, 2, 3].iter().any(|n| n == 2));
                print([1, 2, 3].iter().any(|n| n == 9));
                let empty: List<i32> = [];
                print(empty.iter().any(|n| n == 1));
                print(Naturals { at = 0 }.any(|n| n == 4));
            }
            "#,
        ),
        "true\nfalse\nfalse\ntrue\n",
    );
}

#[test]
fn all_short_circuits_on_the_first_miss_and_is_vacuously_true_when_empty() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                print([1, 2, 3].iter().all(|n| n > 0));
                print([1, 2, 3].iter().all(|n| n > 1));
                let empty: List<i32> = [];
                print(empty.iter().all(|n| n > 100));
                print(Naturals { at = 0 }.all(|n| n < 3));
            }
            "#,
        ),
        "true\nfalse\ntrue\nfalse\n",
    );
}

#[test]
fn rev_walks_the_values_backwards() {
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut backwards = [1, 2, 3].iter().rev();
                for value in backwards {
                    print(value);
                }
                let empty: List<i32> = [];
                print(empty.iter().rev().count());
                print([7].iter().rev().to_list()[0]);
            }
            "#,
        ),
        "3\n2\n1\n0\n7\n",
    );
}

#[test]
fn rev_is_a_barrier_that_still_composes_with_adapters_on_both_sides() {
    // It hands back a `ListIterator`, so the chain continues — but it drained
    // the upstream to do it, which is why the doc calls it a barrier and why
    // the unbounded source has to be bounded BEFORE it.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                let out = Naturals { at = 0 }.take(4).map(|n| n * 10).rev().take(2).to_list();
                print(out[0]);
                print(out[1]);
            }
            "#,
        ),
        "40\n30\n",
    );
}

#[test]
fn to_set_collapses_duplicates_at_the_end_of_a_chain() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::set::Set;

        fun main() {
            let unique = [1, 2, 2, 3, 3, 3].iter().filter(|n| n > 1).to_list().to_set();
            print(unique.len());
            print(unique.contains(2));
            print(unique.contains(1));
            let empty: List<str> = [];
            print(empty.to_set().len());
            print(["only"].to_set().len());
        }
        "#,
        "2\ntrue\nfalse\n0\n1\n",
    );
}

#[test]
fn to_map_builds_a_map_out_of_pairs_and_the_last_key_wins() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::map::Map;
        import std::option::Option::{ self, Some, None };

        fun main() {
            let lengths = ["aa", "b"].iter().map(|word| (word, word.len())).to_list().to_map();
            print(lengths.len());
            print(lengths.get("aa").unwrap_or(-1));
            print(lengths.get("zz").unwrap_or(-1));
            let repeated = [(1, "first"), (1, "second")].to_map();
            print(repeated.get(1).unwrap_or("miss"));
            let empty: List<(i32, str)> = [];
            print(empty.to_map().len());
        }
        "#,
        "2\n2\n-1\nsecond\n0\n",
    );
}

#[test]
fn to_set_and_to_map_live_on_list_because_a_default_cannot_carry_a_bound() {
    // The deviation from §5 ("as trait defaults"), pinned as the compiler fact
    // that forced it: `Iterator<T>` does not bound `T`, a default body may not
    // require a bound its trait does not declare, and a member cannot carry its
    // own that unifies with `T`. So a `to_set` written as a default is an error
    // AT ITS OWN DEFINITION, before any call — which is what this asserts. When
    // per-member bounds arrive, moving `to_set` onto the trait is additive and
    // this pin is what says why it could not be there first.
    assert_fails_with(
        r#"
        import std::hash::Hashable;
        import std::option::Option::{ self, Some, None };
        import std::set::Set;

        trait Walk<T> {
            fun step(&mut self): Option<T>;

            fun to_set(mut self): Set<T> {
                mut result: Set<T> = Set::new();
                for value in self {
                    result.insert(value);
                }
                result
            }
        }

        fun main() {}
        "#,
        "generic parameter 'T' is missing the bound ': Hashable'",
    );
}

#[test]
fn a_terminal_consumes_the_iterator_and_leaves_its_source_list_alone() {
    // The affine half of the terminations: `mut self` consumes the chain, and
    // because `iter()` snapshotted, the list it came from is untouched — its
    // length, its elements, and its ability to be walked again.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                mut source = [1, 2, 3];
                print(source.iter().map(|n| n * 2).to_list().len());
                print(source.iter().count());
                print(source.len());
                print(source[0]);
                source.push(4);
                print(source.iter().count());
            }
            "#,
        ),
        "3\n3\n3\n1\n4\n",
    );
}

#[test]
fn a_custom_conformer_gets_every_termination_for_free() {
    // The trait-default payoff, stated once over a user type: `next` is the
    // only thing `Naturals` implements.
    assert_compiles_and_runs(
        &adapter_program(
            r#"
            fun main() {
                print(Naturals { at = 0 }.take(4).to_list().len());
                print(Naturals { at = 0 }.take(4).fold(0, |total, n| total + n));
                print(Naturals { at = 0 }.take(4).count());
                print(Naturals { at = 0 }.take(4).all(|n| n < 5));
                print(Naturals { at = 0 }.take(4).rev().to_list()[0]);
                Naturals { at = 0 }.take(2).for_each(|n| print(n));
            }
            "#,
        ),
        "4\n10\n4\ntrue\n4\n1\n2\n",
    );
}

// --- I3 S7: the eager `List` forms are NOT re-expressed over the adapters,
// --- permanently (proposal/iterator-adapters.md §4 option ii, §8) ------------
// The owner REFUSED option (ii) on 2026-08-06: an async closure cannot adapt
// through an adapter chain (the adapter stores it in a struct field and calls
// it from a trait-dispatched `next`, which adaptation cannot follow), and the
// adapter path measured ~5.5x slower than the eager loop on a `List` source.
// The `#[ignore]`d pin that waited on that ruling is retired — it asserted an
// outcome the project has decided never to want. The eager four keep their
// eager bodies; `an_async_closure_adapts_map_and_runs_sequentially` above is
// the standing guard that the eager path still adapts.

// --- Found while building I3 S5: a dispatched method call was colored by a
// --- same-named STATIC (async_infer's candidate set) --------------------------
// `dispatch_candidates` over-approximates on purpose — an `OnType` re-dispatch
// does not carry its trait, so it falls back to every member with the call's
// name. Statics were in that set, and they cannot be: `receiver.name()` never
// selects a member with no receiver. Leaving them in was not merely imprecise.
// `std::promise::Promise::all` is an `async external` STATIC and `promise` is a
// force-loaded core module, so the moment std grew an `Iterator::all` trait
// default, every `xs.iter().all(p)` colored its whole caller async — down to an
// `async` `main`, which the const-eval interpreter then refuses outright
// ("async (macro bodies are synchronous)"). The candidate scan now keeps only
// members whose first parameter is `self`.

/// Compile and assert the emitted JS contains no `async` — the shape a
/// miscoloring produces, and one no assertion on stdout can see.
#[track_caller]
fn assert_compiles_without_async(source: &str) {
    match compile(source) {
        Ok(javascript) => assert!(
            !javascript.contains("async"),
            "the program was colored async:\n{javascript}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
}

#[test]
fn a_dispatched_call_is_not_colored_by_a_same_named_async_static() {
    // The user-level shape, with no std collision involved: an async STATIC and
    // a sync trait DEFAULT sharing one name. The call is a method call, so the
    // static is not reachable from it and must not color the caller.
    assert_compiles_without_async(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        struct Gate {}

        impl Gate {
            async fun scan(items: List<i32>): i32 {
                items.len()
            }
        }

        trait Walk<T> {
            fun next(&mut self): Option<T>;

            fun scan(mut self, predicate: |T| bool): bool {
                for value in self {
                    if predicate(value) {
                        ret true;
                    }
                }
                false
            }
        }

        struct Counting { at: i32, limit: i32 }

        impl Counting with Walk<i32> {
            fun next(&mut self): Option<i32> {
                if self.at < self.limit {
                    self.at = self.at + 1;
                    Some(self.at)
                } else {
                    None
                }
            }
        }

        fun main() {
            print(Counting { at = 0, limit = 3 }.scan(|n| n == 2));
        }
        "#,
    );
}

#[test]
fn iterator_all_does_not_color_its_caller_async_through_promise_all() {
    // The std instance, and the one users actually meet: `Iterator::all` shares
    // its name with `Promise::all`, an `async external` static in a force-loaded
    // module. Before the narrowing this emitted `(async () => { … })()` for a
    // program with nothing async in it.
    assert_compiles_without_async(
        r#"
        import std::io::print;

        fun main() {
            print([1, 2, 3].iter().all(|n| n > 0));
        }
        "#,
    );
}

#[test]
fn a_genuinely_async_dispatched_member_still_colors_its_caller() {
    // The other direction, so the narrowing cannot go too far. `describe` is a
    // trait default calling `self.label()` — an `OnType` re-dispatch, which is
    // exactly the path that falls back to the same-named scan — and `label` is
    // inferred async on the one impl. The caller must still be colored, and
    // its output settled rather than a promise. Dropping every candidate takes
    // this red, which is what makes the narrowing above a narrowing and not a
    // deletion.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::sleep;
        import std::option::Option::{ self, Some, None };

        trait Walk<T> {
            fun next(&mut self): Option<T>;
            fun label(self): str;

            fun describe(mut self): str {
                self.label()
            }
        }

        struct Slow { at: i32 }

        impl Slow with Walk<i32> {
            fun next(&mut self): Option<i32> {
                None
            }

            fun label(self): str {
                sleep(1);
                "slow"
            }
        }

        fun main() {
            print(Slow { at = 0 }.describe());
        }
        "#,
        "slow\n",
    );
}

// --- B79: the enum discriminant family ---------------------------------------
//
// `proposal/backed-enums.md` §1.7 surveyed the existing integer discriminant
// and found it validates nothing: a duplicate MISCOMPILES (two variants become
// one runtime value and the second `match` arm is dead), a fraction truncates,
// an overflowing magnitude became `0` — an ordinary discriminant a sibling may
// hold, which routes an overflow straight into the duplicate hole — and a
// discriminant that cannot reach the runtime at all is silently discarded.
//
// The messages state the rule AS IT STANDS. `backed-enums.md` is a live
// proposal to widen the production to string backings, and §3.3/§3.7 design
// exactly these rules for that world too, so none of them foreclose it.

#[test]
fn b79_two_variants_cannot_share_a_discriminant() {
    // P5, the live miscompile: `Dup::B` matched `Dup::A`'s arm and the program
    // printed "a" with exit 0.
    assert_fails_noting(
        r#"
        enum Dup { A = 1, B = 1, C = 2 }
        fun main() { }
        "#,
        "variant 'B' has discriminant 1, which 'A' already uses",
        "A = 1",
        "'A' has discriminant 1",
    );
}

#[test]
fn b79_an_implicit_discriminant_collides_just_as_loudly() {
    // The C-style continuation is part of the value set, so a collision needs
    // no second `=` to happen: `C` walks onto 1 behind `B = 0`.
    assert_fails_with(
        r#"
        enum Walked { A = 1, B = 0, C }
        fun main() { }
        "#,
        "variant 'C' has discriminant 1, which 'A' already uses",
    );
}

#[test]
fn b79_a_fractional_discriminant_is_rejected_rather_than_truncated() {
    // P7's first half: `= 1.5` silently became `1`.
    assert_fails_with(
        r#"
        enum Fraction { X = 1.5, Y = 7 }
        fun main() { }
        "#,
        "an enum backing value must be an integer or a string, and `1.5` is neither",
    );
}

#[test]
fn b79_a_suffixed_discriminant_is_rejected_rather_than_dropped() {
    // The same hole one token over, and the reason `1_000` is in it: the
    // number token's suffix is `_000`, and reducing the WHOLE part alone read
    // the literal as `1`. `= 1u32` was `1` with the type annotation discarded.
    assert_fails_with(
        r#"
        enum Grouped { A = 1_000, B = 1 }
        fun main() { }
        "#,
        "an enum backing value must be an integer or a string, and `1_000` carries the trailer `_000`",
    );
    assert_fails_with(
        r#"
        enum Suffixed { A = 1u32, B = 2 }
        fun main() { }
        "#,
        "an enum backing value must be an integer or a string, and `1u32` carries the trailer `u32`",
    );
}

#[test]
fn b79_an_overflowing_discriminant_is_rejected_rather_than_zeroed() {
    // P7's second half, and the worse one: `unwrap_or(0)` (`parsing.rs:3315`,
    // inherited from chumsky and never revisited) turned this into `0`.
    assert_fails_with(
        r#"
        enum Overflow { X = 99999999999999999999, Y = 1 }
        fun main() { }
        "#,
        "the enum discriminant `99999999999999999999` is out of range",
    );
    // Both directions of the bound, one past each end.
    assert_fails_with(
        r#"
        enum Under { X = -9223372036854775809 }
        fun main() { }
        "#,
        "the enum discriminant `-9223372036854775809` is out of range",
    );
}

#[test]
fn b79_the_bounds_themselves_are_legal() {
    // Off-by-one here would reject a legal program. (B106 moved the bound from
    // `i64` to `i53` and made it SYMMETRIC: `i64`'s negative end reaches one
    // further only because two's complement does, and a JS number has no such
    // asymmetry.)
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        enum Edge { Min = -9007199254740991, Max = 9007199254740991 }
        fun main() {
            print(match Edge::Min { Edge::Min => "min", Edge::Max => "max" });
        }
        "#,
        "min\n",
    );
}

#[test]
fn b79_a_discriminant_sequence_cannot_run_past_the_bound() {
    // The continuation has nowhere to go. `discriminant + 1` panicked the
    // debug compiler here and wrapped the release one.
    assert_fails_with(
        r#"
        enum Edge { A = 9007199254740991, B }
        fun main() { }
        "#,
        "variant 'B' continues the discriminant sequence past 9007199254740991",
    );
}

#[test]
fn b79_a_hex_discriminant_is_read_as_hex() {
    // Not a new spelling — `0xFF` is one integer token everywhere else in the
    // language, and the analyzer's own range check already reads it as radix
    // 16. The discriminant path re-implemented literal reading with
    // `parse::<i64>()`, which FAILS on `0xFF` and fell to `unwrap_or(0)`. The
    // silent `0` is the bug; reading it is the fix, not a feature.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        enum Mask { Low = 0x0F, High = 0xF0 }
        fun main() {
            print(match Mask::High { Mask::Low => "low", Mask::High => "high" });
        }
        "#,
        "high\n",
    );
}

#[test]
fn b79_a_payload_variant_cannot_carry_a_discriminant() {
    // The direct half of §3.3's rule: a bare backing value has nowhere to put
    // a payload.
    assert_fails_with(
        r#"
        enum Pay { A(str) = 1, B }
        fun main() { }
        "#,
        "variant 'A' carries a payload, so it cannot have an explicit backing value",
    );
}

#[test]
fn b79_a_discriminant_beside_a_payload_variant_is_rejected() {
    // P6, and the shape the rule is really about: `backing` is a CONJUNCTION —
    // all-data-less AND any-explicit-value — so `B`'s payload flips the whole
    // enum to the tagged form and `A = 1` is inert. It parsed, it was stored in
    // `EnumVariantDeclaration::backing_value`, and nothing would ever read it.
    assert_fails_noting(
        r#"
        enum Mixed { A = 1, B(str) }
        fun main() { }
        "#,
        "an explicit backing value is only meaningful when every variant is data-less, and 'B' \
         carries a payload",
        "B(str)",
        "'B' carries a payload here",
    );
}

#[test]
fn b79_the_still_legal_discriminant_forms_all_compile() {
    // The negative space, so the family cannot creep: negatives, gaps, a
    // mixture of explicit and continued values, a payload enum with no
    // discriminants at all, and a plain enum with none.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Signed { A = -3, B = -1, C = 5, D }
        enum Gapped { X = 10, Y = 20, Z = 30 }
        enum Plain { P, Q, R }
        enum Carried { S(str), T(i32) }

        fun main() {
            print(match Signed::D { Signed::A => "a", Signed::B => "b", Signed::C => "c", Signed::D => "d" });
            print(match Gapped::Y { Gapped::X => 1, Gapped::Y => 2, Gapped::Z => 3 });
            print(match Plain::R { Plain::P => "p", Plain::Q => "q", Plain::R => "r" });
            print(match Carried::T(7) { Carried::S(let s) => s, Carried::T(let n) => "t" });
        }
        "#,
        "d\n2\nr\nt\n",
    );
}

#[test]
fn b79_a_rejected_discriminant_does_not_also_read_as_a_duplicate() {
    // The cascade guard. An overflowing magnitude used to BECOME `0`, so the
    // one mistake reported twice — once as itself and once as a collision with
    // whatever legitimately holds `0`. A variant with no usable value takes no
    // part in the uniqueness check.
    let diagnostics = failure_diagnostics(
        r#"
        enum Both { A = 0, B = 99999999999999999999 }
        fun main() { }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic; got: {diagnostics:#?}"
    );
    assert!(
        diagnostics[0].0.contains("is out of range"),
        "got: {diagnostics:#?}"
    );
}

#[test]
fn b79_std_ordering_still_lowers_to_its_bare_discriminant() {
    // The load-bearing negative for the whole family: `std/src/compare.vl`'s
    // `Ordering { Less = -1, Equal = 0, Greater = 1 }` is the one enum in the
    // tree that uses the feature, and the representation rule says it lowers
    // to the bare integer rather than the `[index]` array.
    let javascript = compile(
        r#"
        import std::io::print;
        import std::compare::Ordering;
        fun main() {
            print(Ordering::Greater);
        }
        "#,
    )
    .expect("a clean compile");
    assert!(
        javascript.contains("console.log(1)"),
        "Ordering::Greater should lower to the bare `1`, got:\n{javascript}"
    );
}

// --- B84: two same-named members in ONE block --------------------------------
//
// `impl Bag { fun which(self) … "first"  fun which(self) … "second" }` compiled
// clean and ran "second". The cross-block shape — the same two declarations in
// two `impl` blocks — was already a hard error (B57, widened to statics by
// B74); nothing about the RULE differed, only whether the second declaration
// survived to be counted. A scope map holds one entry per name, so the second
// declaration overwrote the first before `collect_declarations` read the map
// back, and the check had no pair to compare.

#[test]
fn b84_two_methods_of_one_name_in_one_block_collide() {
    assert_fails_noting(
        r#"
        struct Bag { n: i32 }
        impl Bag {
            fun which(self): str { "first" }
            fun which(self): str { "second" }
        }
        fun main() { }
        "#,
        "'which' is already defined for 'Bag'; remove or rename this one",
        "which",
        "'which' is already defined here",
    );
}

#[test]
fn b84_two_statics_of_one_name_in_one_block_collide() {
    assert_fails_with(
        r#"
        struct Bag { n: i32 }
        impl Bag {
            fun make(): Bag { Bag { n = 1 } }
            fun make(): Bag { Bag { n = 2 } }
        }
        fun main() { }
        "#,
        "'make' is already defined for 'Bag'",
    );
}

#[test]
fn b84_a_static_and_a_method_of_one_name_in_one_block_collide() {
    // The mixed pair, which B74 established shares ONE namespace: receiver
    // position is not part of a member's identity, so a `fun tag()` and a
    // `fun tag(self)` cannot both be reached whether they sit in one block or
    // two.
    assert_fails_with(
        r#"
        import std::io::print;
        struct Bag { n: i32 }
        impl Bag {
            fun tag(): str { "static" }
            fun tag(self): str { "method" }
        }
        fun main() { print(Bag::tag()); }
        "#,
        "'tag' is already defined for 'Bag'",
    );
}

#[test]
fn b84_two_externals_of_one_name_in_one_block_collide() {
    // The shape bindgen's name table exists to prevent, written by hand: a
    // constructor object's static binding beside the instance method of the
    // same name.
    assert_fails_with(
        r#"
        external struct Reply;
        impl Reply {
            [extern(method, "json")]
            external fun json(self): str;
            [extern("Reply.json")]
            external fun json(data: str): Reply;
        }
        fun main() { }
        "#,
        "'json' is already defined for 'Reply'",
    );
}

#[test]
fn b84_a_trait_provided_name_declared_twice_in_one_block_collides() {
    // The block rule is NOT the inherent rule with a wider input. The inherent
    // rule exempts a trait-provided name so that two impls of one trait — the
    // platform twins — stay legal (method-resolution.md §9(6)); inside ONE
    // block there is no twin to protect, and a name written twice is a mistake
    // whatever trait homes it.
    assert_fails_with(
        r#"
        struct Bag { n: i32 }
        trait Marker { fun mark(self): str; }
        impl Bag with Marker {
            fun mark(self): str { "a" }
            fun mark(self): str { "b" }
        }
        fun main() { }
        "#,
        "'mark' is already defined for 'Bag'",
    );
}

#[test]
fn b84_a_trait_declaring_one_name_twice_collides() {
    // A trait body is a block too, and its declarations went through the same
    // scope map.
    assert_fails_with(
        r#"
        trait Twice {
            fun a(self): str;
            fun a(self): str;
        }
        fun main() { }
        "#,
        "'a' is already defined for 'trait Twice'",
    );
}

#[test]
fn b84_three_copies_in_one_block_report_twice() {
    // Each later declaration is reported against the FIRST, matching the
    // cross-block rule's shape rather than chaining pairwise.
    let diagnostics = failure_diagnostics(
        r#"
        struct Bag { n: i32 }
        impl Bag {
            fun which(self): str { "1" }
            fun which(self): str { "2" }
            fun which(self): str { "3" }
        }
        fun main() { }
        "#,
    );
    assert_eq!(
        diagnostics
            .iter()
            .filter(|(message, _)| message.contains("'which' is already defined for 'Bag'"))
            .count(),
        2,
        "got: {diagnostics:#?}"
    );
}

#[test]
fn b84_a_same_block_duplicate_is_reported_once_not_twice() {
    // The two rules overlap on an inherent same-block pair. The inherent check
    // skips a pair from one block precisely so this stays a single report.
    let diagnostics = failure_diagnostics(
        r#"
        struct Bag { n: i32 }
        impl Bag {
            fun which(self): str { "first" }
            fun which(self): str { "second" }
        }
        fun main() { }
        "#,
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic; got: {diagnostics:#?}"
    );
}

#[test]
fn b84_one_name_per_block_across_two_blocks_still_compiles() {
    // The negative space: the block rule must not reach across blocks, or the
    // platform twins and every ordinary two-impl type go red. `describe` is
    // declared once per block, in three blocks, two of them trait impls.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Bag { n: i32 }
        trait Marker { fun mark(self): str; }
        trait Label { fun label(self): str; }

        impl Bag { fun describe(self): str { "bag" } }
        impl Bag with Marker { fun mark(self): str { "m" } }
        impl Bag with Label { fun label(self): str { "l" } }

        fun main() {
            let bag = Bag { n = 1 };
            print(bag.describe());
            print(bag.mark());
            print(bag.label());
        }
        "#,
        "bag\nm\nl\n",
    );
}

#[test]
fn b84_two_impls_of_one_trait_are_still_not_a_duplicate() {
    // §9(6), kept load-bearing: the trait tier dedups by trait, so the name
    // still has one home. Std's `Into<T>` blanket was the live instance until
    // B127 deleted it (method-resolution.md §14); two user impls of `Into` at
    // different arguments on one subject keep the shape — two blocks, one
    // trait, both declaring `into`, and legal.
    assert_compiles(
        r#"
        import std::into::Into;

        struct Celsius { degrees: i32 }
        struct Fahrenheit { degrees: i32 }

        impl Celsius with Into<Fahrenheit> {
            fun into(self): Fahrenheit { Fahrenheit { degrees = self.degrees * 2 } }
        }

        impl Celsius with Into<Celsius> {
            fun into(self): Celsius { self }
        }

        fun main() { }
        "#,
    );
}

// --- B83: `Type::static()` gets B57's tiering --------------------------------
//
// `prepped_static_accessors` was a flat `find_map` in impl-registration order,
// so a trait-provided static BEAT an inherent one that happened to register
// later — inherent-over-trait inverted on the one path B57 did not reach
// (method-resolution.md §S2's residue). The candidate set is unchanged; only
// the ranking is new.

#[test]
fn b83_an_inherent_static_outranks_a_trait_provided_one() {
    // The registration order that used to decide it: the trait impl FIRST.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::default::Default;

        struct Bag { n: i32 }

        impl Bag with Default { fun default(): Bag { Bag { n = 7 } } }
        impl Bag { fun default(): Bag { Bag { n = 1 } } }

        fun main() { print(Bag::default().n); }
        "#,
        "1\n",
    );
}

#[test]
fn b83_the_inherent_static_wins_from_either_declaration_order() {
    // The other order, which happened to be right before — the pair is what
    // makes the rule a rule rather than a coincidence.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::default::Default;

        struct Bag { n: i32 }

        impl Bag { fun default(): Bag { Bag { n = 1 } } }
        impl Bag with Default { fun default(): Bag { Bag { n = 7 } } }

        fun main() { print(Bag::default().n); }
        "#,
        "1\n",
    );
}

#[test]
fn b83_two_traits_providing_one_static_are_ambiguous() {
    // B57 §4's ambiguity, on the static path. The steer §4 gives a method —
    // `Trait::member(receiver)` — does NOT exist here: the qualified form
    // selects an impl THROUGH the receiver, and a static has no receiver. So
    // the diagnostic names the fix that always works and says outright that
    // the qualified path is not available, rather than steering at a spelling
    // that does not resolve.
    assert_fails_with(
        r#"
        import std::io::print;

        struct Bag { n: i32 }
        trait Alpha { fun spawn(): Bag; }
        trait Beta { fun spawn(): Bag; }

        impl Bag with Alpha { fun spawn(): Bag { Bag { n = 1 } } }
        impl Bag with Beta { fun spawn(): Bag { Bag { n = 2 } } }

        fun main() { print(Bag::spawn().n); }
        "#,
        "'spawn' is ambiguous on 'Bag': both 'Alpha' and 'Beta' provide it as a static, and a \
         static has no receiver for a `Trait::spawn` path to select through; declare 'Bag''s own \
         'spawn', which outranks every trait-provided one",
    );
}

#[test]
fn b83_an_inherent_static_resolves_the_two_trait_ambiguity() {
    // The fix the diagnostic names, proven to work rather than asserted. An
    // impossible steer is worse than no steer (the B65 lesson).
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Bag { n: i32 }
        trait Alpha { fun spawn(): Bag; }
        trait Beta { fun spawn(): Bag; }

        impl Bag with Alpha { fun spawn(): Bag { Bag { n = 1 } } }
        impl Bag with Beta { fun spawn(): Bag { Bag { n = 2 } } }
        impl Bag { fun spawn(): Bag { Bag { n = 9 } } }

        fun main() { print(Bag::spawn().n); }
        "#,
        "9\n",
    );
}

#[test]
fn b83_a_lone_trait_provided_static_still_resolves() {
    // The load-bearing negative. `Type::method` refuses when only a trait
    // provides it (§3.1) because `Trait::method(receiver)` is the sanctioned
    // spelling; for a STATIC there is no other spelling at all, so the trait
    // tier must stay reachable. Tightening this to match the method path would
    // make every trait-provided static uncallable.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::default::Default;

        struct Bag { n: i32 }
        impl Bag with Default { fun default(): Bag { Bag { n = 7 } } }

        fun main() { print(Bag::default().n); }
        "#,
        "7\n",
    );
}

#[test]
fn b83_a_static_on_a_trait_subject_impl_still_resolves() {
    // The other shape the static path carries: an impl whose SUBJECT is a
    // trait (`impl Iterator<type T> { fun from_fn(..) }`), which is how
    // `Iterator::from_fn` is reached. The tiering runs over the same candidate
    // set, so a trait-subject impl must keep resolving.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::iterator::Iterator;
        import std::option::Option::{ self, Some, None };

        fun main() {
            mut n = 0;
            let it = Iterator::from_fn(|| { n = n + 1; if n <= 3 { Some(n) } else { None } });
            print(it.count());
        }
        "#,
        "3\n",
    );
}

#[test]
fn b83_two_impls_of_one_trait_do_not_make_a_static_ambiguous() {
    // §9(6) on the static path: the trait tier dedups by TRAIT, so two impls
    // of one trait leave the name one home. The subjects differ here, so both
    // impls are live at once — which is the case a same-subject pair could
    // never reach.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::default::Default;

        struct Bag { n: i32 }
        struct Box_ { n: i32 }

        impl Bag with Default { fun default(): Bag { Bag { n = 1 } } }
        impl Box_ with Default { fun default(): Box_ { Box_ { n = 2 } } }

        fun main() {
            print(Bag::default().n);
            print(Box_::default().n);
        }
        "#,
        "1\n2\n",
    );
}

#[test]
fn b83_a_trait_declared_static_is_reachable_through_the_trait_when_it_has_a_body() {
    // SUPERSEDED BY B162 (was
    // `b83_a_trait_declared_static_is_not_reachable_through_the_trait`, which
    // RECORDED the probe rather than building it). B83 read the gap right —
    // `Trait::method(receiver)` picks an impl through the receiver, and a
    // static offers nothing to pick with — and drew the wrong conclusion from
    // it: with nothing to pick, there is nothing to pick BETWEEN, so the
    // trait's own default body is the answer, not an ambiguity. That is what
    // B162 ships.
    //
    // What B83 got right survives on the half without a body: a requirement
    // has no body to be the answer, and the call is refused — by a message
    // that now names both spellings rather than the bare "cannot find".
    assert_fails_with(
        r#"
        import std::io::print;

        struct Bag { n: i32 }
        trait Alpha { fun spawn(): Bag; }
        impl Bag with Alpha { fun spawn(): Bag { Bag { n = 1 } } }

        fun main() { print(Alpha::spawn().n); }
        "#,
        "'Alpha::spawn' has no default body",
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Bag { n: i32 }
        trait Alpha { fun spawn(): Bag { Bag { n = 3 } } }

        fun main() { print(Alpha::spawn().n); }
        main();
        "#,
        "3\n",
    );
}

#[test]
fn b83_a_trait_provided_method_is_still_refused_on_the_type_path() {
    // §3.1 is untouched: `Type::method` still refuses a trait-only method and
    // steers to `Trait::method(..)`, which for a METHOD does exist. The static
    // path's reachable trait tier must not leak into it.
    assert_fails_with(
        r#"
        struct Bag { n: i32 }
        trait Alpha { fun show(self): str; }
        impl Bag with Alpha { fun show(self): str { "a" } }

        fun main() { let s = Bag::show(Bag { n = 1 }); }
        "#,
        "'show' is not an inherent member of 'Bag'",
    );
}

/// The number of interned types left behind by analyzing `source` — a
/// deterministic, load-independent measure of how much work the constraint
/// fixpoint did. Every attempt mints fresh type ids unconditionally, so a
/// fixpoint that keeps retrying a stuck constraint set grows this table in
/// direct proportion to the passes it ran.
fn interned_type_count(source: &str) -> usize {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("the probe programs must analyze");
            assert!(
                errors.is_empty(),
                "the probe programs must be clean: {errors:?}"
            );
            program.type_id_to_type_map.len()
        })
        .expect("spawn worker")
        .join()
        .expect("the probe worker must not panic")
}

#[test]
fn the_constraint_fixpoint_stops_when_it_settles() {
    // E43 (`suite-speed.md` §8). `import std::set` leaves ten constraints
    // permanently deferred — legitimately unresolvable, committed to defaults
    // by `finalize_build`. The solving loop is supposed to notice it has
    // settled and stop; instead it counted every attempt's unconditional
    // type-id MINTING as progress, so its quiescence test could never pass and
    // it ran to `max_iterations` — ~14 000 passes over those ten constraints,
    // ~2.2 s of a ~2.4 s import, and the same bill again inside every macro
    // world (`macro_std` re-exports `std::set`, so `[derive(Debug)]` paid it
    // twice over).
    //
    // `std::map` is the control: the same 111 lines, the same shape, no stuck
    // constraints — and it always converged. Pinning `set` AGAINST `map`
    // states the property that actually matters ("set costs what map costs")
    // and stays honest as std grows, where an absolute bound would rot.
    let set = interned_type_count(
        "import std::set::Set;\n\nfun main() {\n\tmut s: Set<i32> = Set::new();\n\ts.insert(1);\n}\n",
    );
    let map = interned_type_count(
        "import std::map::Map;\n\nfun main() {\n\tmut m: Map<str, i32> = Map::new();\n\tm.insert(\"a\", 1);\n}\n",
    );
    assert!(
        set < map * 2,
        "a settled fixpoint must stop: `import std::set` interned {set} types \
         against `import std::map`'s {map}. A ratio this large means the loop is \
         spinning on permanently deferred constraints again (E43)."
    );
}

// --- B95: a monomorphized instance is keyed on what its type arguments ARE,
// --- not on which type ids happen to spell them. Types are deliberately not
// --- interned (`type_id_for_type`: "each call mints a fresh id"), so writing
// --- `List<i32>` twice mints two ids for the `i32` inside — and the instance
// --- key, which used to be `format!("{:?}", type_)`, spells its arguments as
// --- raw `TypeId`s one level down. Two structurally-equal instantiations
// --- therefore keyed apart and the SAME body was emitted twice under two
// --- names. Found by B90's arc (an unconditional hoist re-inferred an argument
// --- earlier, minted its id earlier, and produced a byte-identical duplicate
// --- `Signal::new`); the corpus carried 19 such duplicates across five
// --- programs before the key went structural.
// ---
// --- A count, not a run: a duplicate instance is behaviour-identical, so
// --- `assert_compiles_and_runs` cannot see it. Each shape of nested type id
// --- gets its own pin, and each merging pin is twinned with a splitting one —
// --- the key must be COARSER, never wrong.

/// The B95 probe body: a distinctive expression the instance emits once.
fn b95_program(annotation: &str, first: &str, second: &str) -> String {
    format!(
        r#"
        import std::io::print;
        fun through<T>(value: T): T {{
            print(4242);
            value
        }}
        fun main() {{
            let a: {annotation} = {first};
            let b: {annotation} = {second};
            through(a);
            through(b);
        }}
        "#
    )
}

/// The filed shape: one nominal type argument, written twice.
#[test]
fn b95_two_spellings_of_one_nominal_type_share_an_instance() {
    assert_eq!(
        emitted_occurrences(
            &b95_program("List<i32>", "List::new()", "List::new()"),
            "console.log(4242)",
        ),
        1,
    );
}

/// Nested one level deeper — the key has to RECURSE, not just resolve the
/// outermost argument. `List<List<i32>>`'s duplicate id is the inner one.
#[test]
fn b95_a_nested_nominal_argument_shares_an_instance() {
    assert_eq!(
        emitted_occurrences(
            &b95_program("List<List<i32>>", "List::new()", "List::new()"),
            "console.log(4242)",
        ),
        1,
    );
}

/// A tuple's elements are type ids too.
#[test]
fn b95_two_spellings_of_one_tuple_type_share_an_instance() {
    assert_eq!(
        emitted_occurrences(
            &b95_program("(i32, str)", r#"(1, "x")"#, r#"(2, "y")"#),
            "console.log(4242)",
        ),
        1,
    );
}

/// An array's element type is a type id; its LENGTH is not (it is a `usize`),
/// which the splitting twin below pins.
#[test]
fn b95_two_spellings_of_one_array_type_share_an_instance() {
    assert_eq!(
        emitted_occurrences(
            &b95_program("[i32; 3]", "[1, 2, 3]", "[4, 5, 6]"),
            "console.log(4242)",
        ),
        1,
    );
}

/// A closure type's parameters and return are type ids.
#[test]
fn b95_two_spellings_of_one_closure_type_share_an_instance() {
    assert_eq!(
        emitted_occurrences(
            &b95_program("|i32| i32", "|n| n + 1", "|n| n + 2"),
            "console.log(4242)",
        ),
        1,
    );
}

/// The multi-parameter form: two generics, each re-spelled, one instance.
#[test]
fn b95_a_two_generic_instantiation_shares_an_instance() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            fun pair<T, U>(left: T, right: U): T {
                print(4242);
                left
            }
            fun main() {
                let a: List<i32> = List::new();
                let b: List<str> = List::new();
                let c: List<i32> = List::new();
                let d: List<str> = List::new();
                pair(a, b);
                pair(c, d);
            }
            "#,
            "console.log(4242)",
        ),
        1,
    );
}

/// A METHOD instance, not a free call: the same key is what
/// `method_call_substitution` flows into.
#[test]
fn b95_two_spellings_of_a_method_receiver_share_an_instance() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            struct Holder<T> { value: T }
            impl Holder<type T> {
                fun show(self): T {
                    print(4242);
                    self.value
                }
            }
            fun main() {
                let a: Holder<List<i32>> = Holder { value = List::new() };
                let b: Holder<List<i32>> = Holder { value = List::new() };
                a.show();
                b.show();
            }
            "#,
            "console.log(4242)",
        ),
        1,
    );
}

/// The splitting twin: DIFFERENT nominal arguments still get their own
/// instance. Without this the merging pins above are satisfied by a key that
/// collapses everything.
#[test]
fn b95_different_nominal_arguments_still_split_the_instance() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            fun through<T>(value: T): T {
                print(4242);
                value
            }
            fun main() {
                let a: List<i32> = List::new();
                let b: List<str> = List::new();
                through(a);
                through(b);
            }
            "#,
            "console.log(4242)",
        ),
        2,
    );
}

/// The splitting twin for a length that is not a type id: `[i32; 3]` and
/// `[i32; 4]` are distinct types and must stay distinct instances.
#[test]
fn b95_arrays_of_different_lengths_still_split_the_instance() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            fun through<T>(value: T): T {
                print(4242);
                value
            }
            fun main() {
                let a: [i32; 3] = [1, 2, 3];
                let b: [i32; 4] = [1, 2, 3, 4];
                through(a);
                through(b);
            }
            "#,
            "console.log(4242)",
        ),
        2,
    );
}

/// The splitting twin for nesting: the recursion must reach the inner
/// argument in the OTHER direction too — `List<List<i32>>` and
/// `List<List<str>>` differ only two levels down.
#[test]
fn b95_nested_arguments_that_differ_deep_still_split_the_instance() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            fun through<T>(value: T): T {
                print(4242);
                value
            }
            fun main() {
                let a: List<List<i32>> = List::new();
                let b: List<List<str>> = List::new();
                through(a);
                through(b);
            }
            "#,
            "console.log(4242)",
        ),
        2,
    );
}

// --- B102: a call's recorded substitution is about the CALLEE, and says
// --- something. Every entry joins the monomorphization instance key
// --- (`transformer.rs`'s `emit_instance_with_bits`), so an entry that names a
// --- constraint the callee's body cannot mention — or that "binds" a
// --- constraint to itself — splits an instance off from the identical ones it
// --- belongs with. Between them those two entries were the whole reason B90's
// --- hoisted pre-binding pass stayed gated on argument order for two cycles;
// --- with both refused at the record, the hoist is unconditional and all 112
// --- corpus goldens are byte-identical.

/// The filed shape, minimised. `through` is called at the top level AND from
/// inside another generic body. At the nested call the pre-binding pass fixes
/// the parameter's `T` before the positional loop reaches it, so the loop
/// reconciles the already-substituted parameter against the argument and
/// reports the CALLER's `U` bound to itself as well — `{T: U, U: U}` against
/// the outer call's `{T: i32}`. `through`'s body cannot mention `U`; one
/// instance, not two.
#[test]
fn b102_a_callers_generic_does_not_split_the_callees_instance() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            fun through<T>(value: T): T {
                print(4242);
                value
            }
            fun forward<U>(value: U): U {
                through(value)
            }
            fun main() {
                print(through(1));
                print(forward(2));
            }
            "#,
            "console.log(4242)",
        ),
        1,
    );
}

/// The splitting twin: the filter drops what the callee cannot mention, and
/// nothing else. Two genuinely different instantiations still get their own
/// instance — without this the pin above is satisfied by a key that collapses
/// every call to one body.
#[test]
fn b102_a_different_instantiation_through_a_forwarder_still_splits() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            fun through<T>(value: T): T {
                print(4242);
                value
            }
            fun forward<U>(value: U): U {
                through(value)
            }
            fun main() {
                print(through(1));
                print(forward("x"));
            }
            "#,
            "console.log(4242)",
        ),
        2,
    );
}

/// The bound on the filter, and the reason it is written as "the callee's own
/// constraints" rather than the tempting "anything that binds a generic to
/// ITSELF". A self-recursive call binds `T` to the enclosing `T`, which IS the
/// identity — and is real: it says this call instantiates at whatever the
/// enclosing instance does. `wrap`'s `T` is `wrap`'s own constraint, so the
/// filter keeps it. The blanket identity filter was tried, and this shape is
/// what refuted it: the bound went unrecorded and the call reported "cannot
/// infer 'T' for this call; its bound ': Slot' cannot be checked".
#[test]
fn b102_a_self_recursive_call_keeps_its_own_generic_bound() {
    compile_browser(
        r#"
        import std::ui::{ Slot, mount, view, View };
        fun wrap<T: Slot>(content: T, depth: i32): View {
            if depth > 0 {
                wrap(content, depth - 1)
            } else {
                view("p").child(content)
            }
        }
        fun main() {
            mount("app", wrap("static", 3));
        }
        main();
        "#,
    )
    .expect("a self-recursive call still records its own generic");
}

/// The second writer: return-type-only inference. `Map::new()`'s binders are
/// fixed by nothing but the expectation, and under an expectation substitution
/// has already made abstract it unifies `Map<K, V>` with `Map<K, V>` and
/// "infers" `{K: K, V: V}`. Recording that put a whole instance key's worth of
/// nothing on the call: the generic body was re-emitted under a generated
/// instance name instead of staying the one shared source-named declaration
/// `inherited_substitution` gives it.
#[test]
fn b102_a_static_the_call_cannot_instantiate_stays_one_declaration() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            import std::map::Map;
            import std::reactive::{ Signal, SignalCell };
            fun main() {
                let scores: SignalCell<Map<str, i32>> = Signal::new(Map::new());
                print(scores.get().len());
            }
            "#,
            "function new",
        ),
        1,
    );
}

/// An impl hung off a TRAIT binds its binder there — `impl Iterator<type T> {
/// fun from_fn(fn: || T): FromFn<T> }` — and `impl_binder_generics` read only
/// `Struct`/`Enum` subjects, so `T` was not among the constraints the call may
/// bind. Once the record keys on that set, the missing binder took the whole
/// substitution with it and the static stopped monomorphizing: the emitted
/// program grew a plain `from_fn` declaration where the instance had been.
#[test]
fn b102_a_static_on_a_trait_monomorphizes_through_its_binder() {
    assert_eq!(
        emitted_occurrences(
            r#"
            import std::io::print;
            trait Iterator<T> {
                fun next(self): T;
            }
            struct FromFn<T> {
                fn: || T,
            }
            impl FromFn<type T> with Iterator<T> {
                fun next(self) {
                    (self.fn)()
                }
            }
            impl Iterator<type T> {
                fun from_fn(fn: || T): FromFn<T> {
                    FromFn { fn }
                }
            }
            fun main() {
                mut i = 0;
                let naturals = Iterator::from_fn(|| {
                    i += 1;
                    i
                });
                print(naturals.next());
            }
            "#,
            "function from_fn",
        ),
        0,
    );
}

/// The whole arc, behaviourally: the shapes above run, and run the same. The
/// counts are the point (a duplicate instance is behaviour-identical, which is
/// why the pins above count), but a schedule change that makes every call take
/// the two-phase order has to keep the programs correct too.
#[test]
fn b102_the_unconditional_hoist_keeps_both_argument_orders_running() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun apply<T>(render: |T| i32, value: T): i32 {
            render(value)
        }
        fun apply_last<T>(value: T, render: |T| i32): i32 {
            render(value)
        }
        fun main() {
            print(apply(|n| n * 2, 21));
            print(apply_last(21, |n| n * 2));
        }
        "#,
        "42\n42\n",
    );
}
