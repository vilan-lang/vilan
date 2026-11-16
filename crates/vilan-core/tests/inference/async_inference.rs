//! The async inference (`async_infer.rs`): the implicit-await fixpoint and the
//! adaptation worklist (async-polymorphism.md A.1). The diagnostics it reports
//! are pinned where their subject lives (`bounds.rs`, `generics.rs`, ...);
//! these pins own the pass's SHAPE — what it computes, and how often.

use super::support::*;

/// `(passes, worklist rounds, decisions passes)` one analysis of `source` ran
/// (a pass per analysis, the macro worlds it builds included).
fn adaptation_counts(source: &str) -> (usize, usize, usize) {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let source: &'static str = Box::leak(source.into_boxed_str());
            vilan_core::async_infer::reset_adaptation_counts();
            let (program, errors) = analyze_source(
                source,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            drop(program);
            vilan_core::async_infer::adaptation_counts()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// M138: the worklist re-runs while a round grows the async set, but the
/// DECISIONS pass — each instance's emission data and diagnostics, which reads
/// the set and moves nothing — runs once, over the last round. `caller` passes
/// an awaiting closure to `apply`, whose adapted instance is async, so the
/// first round flips `caller` itself async and a second round must run; the
/// first round's decisions were computed and thrown away.
#[test]
fn m138_the_adaptation_decisions_run_once_however_many_rounds() {
    let (passes, rounds, decisions) = adaptation_counts(
        r#"
        import std::io::print;
        import std::display::Display;
        import std::time::sleep;

        fun apply(f: || i32): i32 {
            f()
        }

        fun caller(): i32 {
            apply(|| {
                sleep(1);
                3
            })
        }

        fun main() {
            print(caller().to_string());
        }
        "#,
    );
    assert!(
        rounds > passes,
        "the probe must need a second worklist round, or the count below proves nothing: \
         {rounds} round(s) over {passes} pass(es)"
    );
    assert_eq!(
        decisions, passes,
        "{rounds} worklist rounds over {passes} pass(es) ran {decisions} decisions passes: the \
         decisions read each pass's last round only (M138)"
    );
}

/// B743: a trait default is emitted once per receiver type, from whichever
/// body reaches it first, and it must be emitted under its OWN instance
/// record. It took the caller's, so a default first reached inside an async
/// body (here an awaiting closure, inside an async `main`) came out an
/// `async function` while its synchronous call sites did not await it: the
/// second line printed `direct [object Promise]`.
#[test]
fn b743_a_trait_default_first_reached_from_an_async_body_stays_synchronous() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::sleep;

        trait Sized2 {
            fun base(self): i32;

            fun doubled(self): i32 {
                self.base() * 2
            }
        }

        struct Box2 {
            value: i32,
        }

        impl Box2 with Sized2 {
            fun base(self): i32 {
                self.value
            }
        }

        fun apply(f: || i32): i32 {
            f() + 1
        }

        fun main() {
            let slow = apply(|| {
                sleep(1);
                Box2 { value = 3 }.doubled()
            });
            print(i"slow {slow}");
            let four = Box2 { value = 4 };
            print(i"direct {four.doubled()}");
        }
        "#,
        "slow 7\ndirect 8\n",
    );
}
