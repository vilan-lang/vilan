//! Platform coloring and its fences, fn coercion, `std::time`, `Draft<T>`, the
//! diagnostics audit batches, and the fixed-bug regression guards.
//!
//! One subject module of the `inference` test binary; the harness it is
//! written against lives in `support.rs`.

use crate::support::*;

// --- §3.7: declared platform fences ------------------------------------------
//
// `[platform("…")]` declares the platforms a function promises to run on;
// the inferred requirement is checked against every matching host on EVERY
// compile — no entry needed, independent of the build target. Violations
// hang their chain from the fence.

#[test]
fn a_platform_fence_rejects_an_off_platform_reach() {
    // Checked on a NODE build (which itself admits `stat`) and with main
    // never calling the fenced function — the fence alone carries the check.
    assert_fails_spanning(
        r#"
        import std::fs::stat;

        [platform("browser")]
        fun probe_cache(): bool {
            stat("cache").is_some()
        }

        fun main() {}
        "#,
        r#"stat("cache")"#,
        "reachable from `probe_cache`, fenced `[platform(\"browser\")]`",
    );
}

#[test]
fn a_satisfied_fence_compiles_on_every_build_target() {
    let source = r#"
        import std::fs::stat;

        [platform("@process")]
        fun probe_cache(): bool {
            stat("cache").is_some()
        }

        fun main() {}
        "#;
    assert_compiles(source);
    assert_compiles_browser(source);
}

#[test]
fn a_neutral_fence_spanning_families_holds_for_base_code() {
    assert_compiles(
        r#"
        import std::io::print;

        [platform("@process", "browser")]
        fun shared_label(): str {
            "everywhere"
        }

        fun main() {
            print(shared_label());
        }
        "#,
    );
}

#[test]
fn an_unknown_fence_pattern_errors() {
    assert_fails(
        r#"
        [platform("wat")]
        fun probe(): i32 { 1 }

        fun main() {}
        "#,
    );
}

#[test]
fn a_fence_on_a_generic_promises_every_instantiation() {
    // Fences walk unbound, so dispatch considers every candidate: the
    // colored impl's existence alone breaks a browser fence on the generic —
    // deliberate conservatism (the fence promises for every possible T).
    assert_fails_browser_with(
        r#"
        import std::fs::stat;

        trait Check {
            fun check(self): bool;
        }

        struct DiskProbe { path: str }

        impl DiskProbe with Check {
            fun check(self): bool {
                stat(self.path).is_some()
            }
        }

        [platform("browser")]
        fun run_check<T: Check>(subject: T): bool {
            subject.check()
        }

        fun main() {}
        "#,
        "reachable from `run_check`, fenced `[platform(\"browser\")]`",
    );
}

#[test]
fn a_fence_on_a_method_checks_like_a_functions() {
    assert_fails_browser_with(
        r#"
        import std::fs::stat;

        struct Store { path: str }

        impl Store {
            [platform("browser")]
            fun probe(self): bool {
                stat(self.path).is_some()
            }
        }

        fun main() {}
        "#,
        "reachable from `probe`, fenced `[platform(\"browser\")]`",
    );
}

#[test]
fn a_colored_instantiation_still_rejects_beside_a_neutral_one() {
    // The refinement is not a hole: when the SAME generic is instantiated
    // both ways, the colored instantiation's path still rejects — chained
    // through the impl that instantiation actually selects.
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;

        trait Save {
            fun save(self): bool;
        }

        struct MemStore { last: str }
        struct DiskStore { path: str }

        impl MemStore with Save {
            fun save(self): bool { true }
        }

        impl DiskStore with Save {
            fun save(self): bool {
                write_file(self.path, "state");
                true
            }
        }

        fun save_it<T: Save>(store: T): bool {
            store.save()
        }

        fun main() {
            save_it(MemStore { last = "" });
            save_it(DiskStore { path = "s.txt" });
        }
        "#,
        "reachable from the entry: main → save_it → save → write_file (std::fs)",
    );
}

#[test]
fn instantiation_bindings_compose_through_nested_generics() {
    // `route<T>` forwards to `commit<U>` — the binding threads two frames
    // deep, so the neutral instantiation stays admitted even though the
    // dispatch happens in the inner generic.
    assert_compiles_browser(
        r#"
        import std::fs::write_file;

        trait Save {
            fun save(self): bool;
        }

        struct MemStore { last: str }
        struct DiskStore { path: str }

        impl MemStore with Save {
            fun save(self): bool { true }
        }

        impl DiskStore with Save {
            fun save(self): bool {
                write_file(self.path, "state");
                true
            }
        }

        fun commit<U: Save>(store: U): bool {
            store.save()
        }

        fun route<T: Save>(store: T): bool {
            commit(store)
        }

        fun main() {
            route(MemStore { last = "" });
        }
        "#,
    );
}

#[test]
fn a_never_instantiated_impls_globals_leave_no_residue() {
    // The emission side moves with the refinement (emitted ⊆ admitted): a
    // binding referenced only by the impl no instantiation selects is
    // dropped, its callees — and their `node:` imports — with it. The global
    // must be a SYNCHRONOUS `@process` call carrying a `node:` import, so a
    // module-level `Database` stands in for the deleted `fs::exists`
    // (kolt.local 031 Q3): `Database::open` is sync and statically imports
    // `node:sqlite`.
    let source = r#"
        import std::db::Database;

        trait Save {
            fun save(self): bool;
        }

        struct MemStore { last: str }
        struct DiskStore { path: str }

        let disk_db: Database = Database::open("state");

        impl MemStore with Save {
            fun save(self): bool { true }
        }

        impl DiskStore with Save {
            fun save(self): bool {
                disk_db.exec("SELECT 1");
                true
            }
        }

        fun save_it<T: Save>(store: T): bool {
            store.save()
        }

        fun main() {
            save_it(MemStore { last = "" });
        }
        "#;
    let browser = compile_browser(source).expect("the neutral instantiation compiles");
    assert!(
        !browser.contains("node:") && !browser.contains("\"state\""),
        "the unselected impl's binding leaked into the bundle:\n{browser}"
    );
}

#[test]
fn the_router_is_browser_only() {
    // `std::router` lives in the browser layer. Under platform coloring the
    // import is fine — REACHING `navigate` from a node build's entry is the
    // violation, anchored at the user call site with the chain
    // (proposal/platform-coloring.md §3.6).
    assert_fails_spanning(
        r#"
        import std::router::navigate;

        fun main() {
            navigate("/home");
        }
        "#,
        r#"navigate("/home")"#,
        "requires the `browser` layer of `std` and cannot run on `node",
    );
}

// --- E98: one coloring mistake draws one diagnostic --------------------------
//
// The admission walk reaches a layer by every edge the program has — the call
// the user wrote, the synthetic teardown the transformer will insert, and (for
// a fence over a FAMILY) one walk per host in it — so one mistake used to be
// reported two or three times. Two fixes hold the line: a teardown's edge
// carries the CONSTRUCTION as its site, so its diagnostic anchors in the user's
// code rather than inside the library's own `drop`, and violations dedupe on
// `(anchor, layer, what the chain hangs from)`. The negatives below are the
// other half of the claim: genuinely distinct mistakes still each report.

#[test]
fn a_library_resource_in_a_browser_build_draws_one_diagnostic() {
    // Constructing a `@process` resource in a browser build is ONE mistake. It
    // used to draw two: the construction's, plus the scope-end teardown's —
    // that one anchored inside `std`'s own `File::drop`, where the user has
    // nothing to change.
    assert_fails_browser_once_with(
        r#"
        import std::fs::File;

        fun main() {
            let file = File::open("data.txt");
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

#[test]
fn the_doubling_was_the_resource_class_not_one_type() {
    // `Database` shows it identically — the teardown edge, not `File`.
    assert_fails_browser_once_with(
        r#"
        import std::db::Database;

        fun main() {
            let db = Database::open("app.db");
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

#[test]
fn an_early_drop_sink_does_not_add_a_second_diagnostic() {
    // The `drop(x)` sink seeds its own §8 edge, from the SINK rather than the
    // scope exit — and it resolves through the binding to the same
    // construction, so the count is still one.
    assert_fails_browser_once_with(
        r#"
        import std::fs::File;
        import std::drop::drop;

        fun main() {
            let file = File::open("data.txt");
            drop(file);
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

#[test]
fn a_family_fence_draws_one_diagnostic_not_one_per_host() {
    // `@process` enumerates node, deno and bun, and the fence is checked
    // against each — three walks, one broken promise. The diagnostic names the
    // first host that rejects it; the fence itself is quoted, so the family is
    // not lost.
    let source = r#"
        import std::router::navigate;

        [platform("@process")]
        fun go() {
            navigate("/home");
        }

        fun main() {}
        "#;
    let errors = compile(source).expect_err("the fence is broken");
    let coloring: Vec<_> = errors
        .iter()
        .filter(|error| error.contains("requires the `browser` layer of `std`"))
        .collect();
    assert_eq!(
        coloring.len(),
        1,
        "expected one fence diagnostic: {errors:#?}"
    );
    assert!(
        coloring[0].contains(r#"fenced `[platform("@process")]`"#),
        "the fence must still be quoted: {:?}",
        coloring[0]
    );
}

#[test]
fn two_fences_broken_the_same_way_each_report() {
    // The negative for the fence half: the origin is part of the cause, so two
    // distinct promises are two distinct mistakes even reaching the same callee.
    let source = r#"
        import std::router::navigate;

        [platform("@process")]
        fun go_home() {
            navigate("/home");
        }

        [platform("@process")]
        fun go_away() {
            navigate("/away");
        }

        fun main() {}
        "#;
    let errors = compile(source).expect_err("both fences are broken");
    let coloring = errors
        .iter()
        .filter(|error| error.contains("requires the `browser` layer of `std`"))
        .count();
    assert_eq!(coloring, 2, "expected one per fence: {errors:#?}");
}

#[test]
fn two_distinct_off_platform_calls_each_report() {
    // The negative for the entry half: same layer, same function, two sites —
    // two things the user must change, so two diagnostics.
    let source = r#"
        import std::fs::{ stat, write_file };

        fun main() {
            let _probe = stat("cache");
            write_file("out.txt", "data");
        }
        "#;
    let errors = compile_browser(source).expect_err("both calls are off platform");
    let coloring = errors
        .iter()
        .filter(|error| error.contains("requires the `process` layer of `std`"))
        .count();
    assert_eq!(coloring, 2, "expected one per call site: {errors:#?}");
}

#[test]
fn a_drop_only_mistake_still_reports_beside_an_unrelated_one() {
    // The negative for the teardown half: a user resource whose ONLY off-platform
    // surface is its `Drop` is a mistake the construction says nothing about, so
    // it must keep its own diagnostic beside an unrelated off-platform call.
    let source = r#"
        import std::fs::{ stat, write_file };
        import std::drop::Drop;

        resource struct Logger { path: str }
        impl Logger with Drop {
            fun drop(&mut self) { write_file(self.path, "closing"); }
        }

        fun main() {
            let logger = Logger { path = "log.txt" };
            let _probe = stat("cache");
        }
        "#;
    let errors = compile_browser(source).expect_err("both are off platform");
    let coloring = errors
        .iter()
        .filter(|error| error.contains("requires the `process` layer of `std`"))
        .count();
    assert_eq!(
        coloring, 2,
        "the teardown and the call are two mistakes: {errors:#?}"
    );
}

#[test]
fn nothing_is_left_anchored_inside_the_library() {
    // The whole diagnostic list, not just the coloring share: the teardown's
    // second error used to land at `std`'s own `File::drop` — a file the user
    // cannot edit, at a position that meant nothing to them. Exactly one
    // diagnostic remains, and it spans the construction they wrote.
    let source = r#"
        import std::fs::File;

        fun main() {
            let file = File::open("data.txt");
        }
        "#;
    let diagnostics = failure_diagnostics_on(source, Platform::Browser);
    assert_eq!(
        diagnostics.len(),
        1,
        "one mistake, one diagnostic: {diagnostics:#?}"
    );
    let construction = source
        .find(r#"File::open("data.txt")"#)
        .expect("the construction is in the source");
    assert_eq!(
        diagnostics[0].1,
        construction..construction + r#"File::open("data.txt")"#.len(),
        "the diagnostic must span the construction: {diagnostics:#?}"
    );
}

#[test]
fn a_user_written_drop_anchors_at_its_own_off_platform_call() {
    // The complement: when the `drop` impl is the user's own, the deepest
    // user-code site on the chain is inside that body, and that is where the
    // fix goes — the teardown's construction site is a FALLBACK for a library
    // destructor, not a replacement for a real call site.
    assert_fails_browser_spanning(
        r#"
        import std::fs::write_file;
        import std::drop::Drop;

        resource struct Logger { path: str }
        impl Logger with Drop {
            fun drop(&mut self) { write_file(self.path, "closing"); }
        }

        fun main() {
            let logger = Logger { path = "log.txt" };
        }
        "#,
        r#"write_file(self.path, "closing")"#,
        "`write_file` requires the `process` layer of `std`",
    );
}

#[test]
fn an_optional_library_resource_draws_one_diagnostic() {
    // `Option<File>` is the sanctioned container (`filesystem.md`), and its
    // teardown reaches `File::drop` through the drop GLUE's members rather than
    // directly — a member inherits the owner's construction site, so the count
    // is still one.
    assert_fails_browser_once_with(
        r#"
        import std::fs::File;
        import std::option::Option;

        fun main() {
            let held = Option::Some(File::open("data.txt"));
        }
        "#,
        "requires the `process` layer of `std`",
    );
}

// --- platform coloring: per-function requirement lines (hover's data) --------
//
// `platform_color::requirements` renders what the admission walk knows into an
// entry-independent per-function map — the language server appends these lines
// to hover (proposal/platform-coloring.md phase 2). The pins fix the exact
// vocabulary: the layer label, a SHORTEST via-chain, library frames labeled
// with their module, user frames bare.

#[test]
fn a_requirement_line_names_the_layer_and_the_via_chain() {
    let line = requirement_line_of(
        r#"
        import std::fs;

        fun save() {
            fs::write_file("state", "data");
        }

        fun main() {
            save();
        }
        "#,
        "save",
    )
    .expect("`save` reaches `std::fs` and should carry a requirement");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `write_file (std::fs)`)"
    );
}

#[test]
fn a_requirement_line_propagates_to_callers_growing_the_chain() {
    // `main` acquires the same label one hop later; its own frame is implicit,
    // the user frame `save` renders bare, the library frame keeps its module.
    let line = requirement_line_of(
        r#"
        import std::fs;

        fun save() {
            fs::write_file("state", "data");
        }

        fun main() {
            save();
        }
        "#,
        "main",
    )
    .expect("`main` reaches `std::fs` through `save`");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `save → write_file (std::fs)`)"
    );
}

#[test]
fn a_seeded_library_functions_line_has_no_chain() {
    // The std function itself is seeded at its definition site — its line is
    // the bare requirement, no `via`.
    let line = requirement_line_of(
        r#"
        import std::fs;

        fun main() {
            fs::write_file("state", "data");
        }
        "#,
        "write_file",
    )
    .expect("`write_file` is defined in the layer");
    assert_eq!(line, "requires the `process` layer of `std`");
}

#[test]
fn the_via_chain_is_a_shortest_path_to_the_layer() {
    // `main` reaches the layer both through `relay → save` and through `save`
    // directly; the witness chain takes the short way.
    let line = requirement_line_of(
        r#"
        import std::fs;

        fun save() {
            fs::write_file("state", "data");
        }

        fun relay() {
            save();
        }

        fun main() {
            relay();
            save();
        }
        "#,
        "main",
    )
    .expect("`main` reaches the layer");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `save → write_file (std::fs)`)"
    );
}

#[test]
fn a_created_closures_requirement_lands_on_its_creator_line() {
    // The v1 creator rule, rendered: the closure's body charges its creator,
    // and the chain shows the closure frame it traveled through.
    let line = requirement_line_of(
        r#"
        import std::fs::write_file;

        fun make_saver(path: str): |str| void {
            |content: str| {
                write_file(path, content);
            }
        }

        fun main() {
            let _saver = make_saver("s.txt");
        }
        "#,
        "make_saver",
    )
    .expect("`make_saver` creates the colored closure");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `closure → write_file (std::fs)`)"
    );
}

#[test]
fn a_dispatch_candidates_requirement_reaches_the_bounded_caller_line() {
    // Candidate descent (async_infer's rule): the bounded call charges the
    // colored impl's method, and the line says which one — even though this
    // node build ADMITS the layer (the map is platform-independent).
    let line = requirement_line_of(
        r#"
        import std::fs::write_file;

        trait Save {
            fun save(self): bool;
        }

        struct DiskStore { path: str }

        impl DiskStore with Save {
            fun save(self): bool {
                write_file(self.path, "state");
                true
            }
        }

        fun save_it<T: Save>(store: T): bool {
            store.save()
        }

        fun main() {
            save_it(DiskStore { path = "s.txt" });
        }
        "#,
        "save_it",
    )
    .expect("`save_it`'s bound admits the colored impl");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `save → write_file (std::fs)`)"
    );
}

#[test]
fn a_base_only_function_is_colorless() {
    assert_eq!(
        requirement_line_of(
            r#"
        import std::io::print;

        fun greet() {
            print("hi");
        }

        fun main() {
            greet();
        }
        "#,
            "greet",
        ),
        None
    );
}

#[test]
fn an_unreached_function_still_knows_its_requirement() {
    // Entry-independence: nothing calls `orphan`, but its line exists — the
    // fixpoint serves the editor, not just the entry walk.
    let line = requirement_line_of(
        r#"
        import std::fs;

        fun orphan() {
            fs::write_file("state", "data");
        }

        fun main() {}
        "#,
        "orphan",
    )
    .expect("`orphan` should be colored without being reachable");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `write_file (std::fs)`)"
    );
}

// --- platform coloring: module-level initializers ----------------------------
//
// A module-level binding's initializer runs iff something reachable
// references it (F6 — emission's rule), so a REFERENCE is an edge and the
// initializer's calls color like any body. Previously initializers were not
// graph nodes at all: a browser build could reference a binding whose
// initializer called `std::fs` and compile clean, shipping a load-time crash.

#[test]
fn a_module_initializers_call_colors_the_referencing_entry() {
    // `std::process::env` — synchronous, so the initializer is legal on the
    // node build and ONLY the coloring is under test. (`fs::exists`, this
    // pin's original subject, was deleted by kolt.local 031's Q3 ruling; the
    // fs module now has no synchronous entry a module initializer could call.)
    assert_fails_browser_with(
        r#"
        import std::process::env;

        let cache = env("CACHE");

        fun main() {
            let content = cache;
        }
        "#,
        "`env` requires the `process` layer of `std` and cannot run on `browser`\n  reachable from the entry: main → cache → env (std::process)",
    );
}

#[test]
fn an_initializer_violation_anchors_at_the_initializer_call() {
    // The deepest user-code call site on the path is the initializer's own
    // call — the squiggle lands on the code that would run off-platform.
    // (Span-pinned on the node build via a browser-layer binding, the
    // `navigate` precedent.)
    assert_fails_spanning(
        r#"
        import std::storage::get;

        let token = get("notes-token");

        fun main() {
            let t = token;
        }
        "#,
        r#"get("notes-token")"#,
        "requires the `browser` layer of `std` and cannot run on `node",
    );
}

#[test]
fn an_initializer_reaching_a_user_function_colors_through_it() {
    assert_fails_browser_with(
        r#"
        import std::process::env;

        fun boot_check(): bool {
            env("STATE").is_some()
        }

        let ready = boot_check();

        fun main() {
            let r = ready;
        }
        "#,
        "reachable from the entry: main → ready → boot_check → env (std::process)",
    );
}

#[test]
fn a_global_referencing_a_colored_global_chains_through_both() {
    assert_fails_browser_with(
        r#"
        import std::process::env;

        let raw = env("DATA");
        let copy = raw;

        fun main() {
            let c = copy;
        }
        "#,
        "reachable from the entry: main → copy → raw → env (std::process)",
    );
}

#[test]
fn a_global_closures_body_charges_the_binding_that_creates_it() {
    // The creator rule, at module level: the initializer creates the closure,
    // so referencing the binding is what admits (or rejects) the body.
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;

        let saver = |content: str| write_file("state", content);

        fun main() {
            let s = saver;
        }
        "#,
        "reachable from the entry: main → saver → closure → write_file (std::fs)",
    );
}

#[test]
fn calling_a_global_closure_colors_via_its_binding() {
    // Before initializer edges, a global closure's body was charged to
    // NOBODY: the call is value-indirect (skipped) and it has no lexical
    // parent. The call's subject is a reference to the binding, so the
    // reference edge now carries the charge.
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;

        let saver = |content: str| write_file("state", content);

        fun main() {
            saver("boot");
        }
        "#,
        "requires the `process` layer of `std` and cannot run on `browser`",
    );
}

#[test]
fn an_unreferenced_colored_global_is_elided_not_rejected() {
    // F6: a dropped binding's initializer does not run — referencing it only
    // from unreached code keeps the browser build clean.
    assert_compiles_browser(
        r#"
        import std::fs::read_file_to_str;

        let cache = read_file_to_str("cache.txt");

        fun server_only(): str {
            cache
        }

        fun main() {}
        "#,
    );
}

#[test]
fn a_neutral_global_is_colorless_everywhere() {
    assert_compiles_browser(
        r#"
        import std::io::print;

        let greeting = "hello";

        fun main() {
            print(greeting);
        }
        "#,
    );
}

#[test]
fn a_const_bindings_initializer_is_compile_time_data() {
    // `const` initializers run in the compile-time interpreter and ship as
    // serialized values — nothing runs on the build platform, so the binding
    // seeds nothing and carries no requirement line.
    assert_compiles_browser(
        r#"
        import std::io::print;

        let width = const 2 + 2;

        fun main() {
            print(width);
        }
        "#,
    );
    assert_eq!(
        requirement_line_of(
            r#"
        import std::io::print;

        let width = const 2 + 2;

        fun main() {
            print(width);
        }
        "#,
            "width",
        ),
        None
    );
}

#[test]
fn a_coerced_functions_body_charges_the_reference_site() {
    // fn-to-closure coercion (proposal/fn-coercion.md): a named function
    // passed as a value has no closure-creation event for the creator rule,
    // so the REFERENCE is the charge — every later call through the value is
    // deliberately uncharged (`Indirect(Value)`).
    assert_fails_browser_with(
        r#"
        import std::fs::write_file;

        fun save(content: str) {
            write_file("state", content);
        }

        fun apply(action: |str| void) {
            action("x");
        }

        fun main() {
            apply(save);
        }
        "#,
        "reachable from the entry: main → save → write_file (std::fs)",
    );
}

#[test]
fn an_index_expressions_subject_reference_colors() {
    // The `Index` collector blind spot: `cache[0]` never walked its subject,
    // so the reference — and the initializer behind it — went unseen (it also
    // dropped load-bearing bindings from emission; `const.vl`'s golden pins
    // that side).
    assert_fails_browser_with(
        r#"
        import std::io::print;
        import std::fs::read_file_to_str;

        let cache = [read_file_to_str("cache.txt")];

        fun main() {
            print(cache[0]);
        }
        "#,
        "requires the `process` layer of `std` and cannot run on `browser`",
    );
}

#[test]
fn an_iterator_protocols_next_call_colors_the_loop() {
    // `for x in iterable` calls the resolved protocol `next()` every pass —
    // an edge anchored at the loop (previously invisible: the desugar happened
    // at emission, after the graph was built).
    assert_fails_browser_with(
        r#"
        import std::option::Option::{ self, Some, None };
        import std::iterator::Iterator;
        import std::fs::write_file;

        mut produced = 0;

        struct Audited { limit: i32 }

        impl Audited with Iterator<i32> {
            fun next(&mut self): Option<i32> {
                write_file("audit.log", "tick");
                produced = produced + 1;
                if produced <= self.limit {
                    Some(produced)
                } else {
                    None
                }
            }
        }

        fun main() {
            // The struct-literal iterable is parenthesized: a `for .. in`
            // iterable is a condition position, which excludes bare struct
            // literals (§H.1).
            for n in (Audited { limit = 3 }) {
                let _n = n;
            }
        }
        "#,
        "requires the `process` layer of `std` and cannot run on `browser`",
    );
}

#[test]
fn a_dropped_bindings_initializer_leaves_no_residue_in_the_bundle() {
    // Emission's half of F6 (the phantom-retention fix): a binding referenced
    // only by unreached code must not drag its callees — nor their host
    // `import ... from "node:..."` lines — into the bundle. A browser bundle
    // with a `node:` import fails at module parse, before any code runs.
    let source = r#"
        import std::fs::read_file_to_str;

        let cache = read_file_to_str("cache.txt");

        fun server_only(): str {
            cache
        }

        fun main() {}
        "#;
    let browser = compile_browser(source).expect("the elided reach compiles for the browser");
    assert!(
        !browser.contains("node:"),
        "phantom host import in the browser bundle:\n{browser}"
    );
    assert!(
        !browser.contains("cache.txt"),
        "dropped initializer emitted:\n{browser}"
    );
    // The same binding still emits where the reference is load-bearing. (A
    // reference inside an ELIDED unused local doesn't count as running the
    // initializer — emission drops both, and admission merely
    // over-approximates in the safe direction by still checking it.)
    // `env` rather than an fs read: a module initializer cannot await, and
    // the fs module's last synchronous entry was deleted (kolt.local 031 Q3).
    let node = compile(
        r#"
        import std::io::print;
        import std::process::env;

        let cache = env("cache.txt");

        fun main() {
            print(cache.is_some());
        }
        "#,
    )
    .expect("the node build admits the reach");
    assert!(node.contains("cache.txt"), "reached initializer must emit");
}

#[test]
fn a_globals_requirement_line_serves_hover_like_a_functions() {
    let line = requirement_line_of(
        r#"
        import std::fs::read_file_to_str;

        let cache = read_file_to_str("cache.txt");

        fun main() {}
        "#,
        "cache",
    )
    .expect("`cache`'s initializer reaches the layer");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `read_file_to_str (std::fs)`)"
    );
}

#[test]
fn a_function_referencing_a_colored_global_inherits_its_line() {
    let line = requirement_line_of(
        r#"
        import std::fs::read_file_to_str;

        let cache = read_file_to_str("cache.txt");

        fun peek(): str {
            cache
        }

        fun main() {}
        "#,
        "peek",
    )
    .expect("`peek` runs the initializer by referencing the binding");
    assert_eq!(
        line,
        "requires the `process` layer of `std` (via `cache → read_file_to_str (std::fs)`)"
    );
}

#[test]
fn a_function_requiring_two_layers_renders_one_line_each_in_label_order() {
    // The mixed form: one function reaching two different layers gets one
    // line per label, label-sorted. (`torn` is unreached, so the node build
    // stays admissible while the browser requirement is still computed.)
    let line = requirement_line_of(
        r#"
        import std::fs;
        import std::router::navigate;

        fun torn() {
            fs::write_file("state", "data");
            navigate("/home");
        }

        fun main() {}
        "#,
        "torn",
    )
    .expect("`torn` requires both layers");
    assert_eq!(
        line,
        "requires the `browser` layer of `std` (via `navigate (std::router)`)\n\
         requires the `process` layer of `std` (via `write_file (std::fs)`)"
    );
}

// --- B19: closure-return-grounded method generics (backlog.md §B.19) ---------
//
// A method's own generic fixed ONLY by a closure argument's return
// (`map<U>(self, transform: |V| U)`) used to freeze abstract when the call
// resolved before the closure's body typed: the substitution — and the call's
// return type — kept `Generic(U)`, so a later bounded call rejected 'U', and
// monomorphization through the value dispatched abstractly. The resolution now
// defers (the same retry the non-closure path always had) until the closure's
// type lands. The browser-side shape is pinned above
// (`a_mapped_signal_meets_a_bound_without_annotation`).

#[test]
fn a_closure_grounded_generic_dispatches_through_its_bound() {
    // The runtime half: the grounded `U` must reach monomorphization, so the
    // consumer's `==` dispatches to the REAL PartialEq — both outcomes, so an
    // empty abstract method (undefined ~ falsy) cannot pass.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        struct Wrap<V> {
            value: V,
        }

        impl Wrap<type V> {
            fun map<U>(self, transform: |V| U): Wrap<U> {
                Wrap { value = transform(self.value) }
            }
        }

        [derive(PartialEq)]
        struct Label {
            text: str,
        }

        fun same<T: PartialEq>(a: T, b: T): bool {
            a == b
        }

        fun tag(n: i32): Label {
            Label { text = i"tag-{n}" }
        }

        fun main() {
            let a = Wrap { value = 3 }.map(|n| tag(n));
            let b = Wrap { value = 3 }.map(|n| tag(n));
            let c = Wrap { value = 4 }.map(|n| tag(n));
            print(same(a.value, b.value));
            print(same(a.value, c.value));
        }
        main();
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn a_closure_grounded_generic_still_fails_an_unmet_bound() {
    // The other direction: once `U` grounds to a type WITHOUT the impl, the
    // bound check must reject it — deferral must not soften the gate.
    assert_fails_spanning(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        struct Wrap<V> {
            value: V,
        }

        impl Wrap<type V> {
            fun map<U>(self, transform: |V| U): Wrap<U> {
                Wrap { value = transform(self.value) }
            }
        }

        struct Opaque {
            tag: str,
        }

        fun needs_eq<T: PartialEq>(wrapped: Wrap<T>): bool {
            wrapped.value == wrapped.value
        }

        fun cloak(n: i32): Opaque {
            Opaque { tag = i"{n}" }
        }

        fun main() {
            let wrapped = Wrap { value = 3 }.map(|n| cloak(n));
            print(needs_eq(wrapped));
        }
        "#,
        "needs_eq(wrapped)",
        "does not implement trait 'PartialEq'",
    );
}

#[test]
fn chained_maps_ground_each_link() {
    // Two chained closure-grounded links: the outer receiver is itself a
    // deferred call result, so the retries must converge inside-out.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        struct Wrap<V> {
            value: V,
        }

        impl Wrap<type V> {
            fun map<U>(self, transform: |V| U): Wrap<U> {
                Wrap { value = transform(self.value) }
            }
        }

        fun same<T: PartialEq>(a: T, b: T): bool {
            a == b
        }

        fun stringify(n: i32): str {
            i"{n}"
        }

        fun measure(text: str): i32 {
            text.len()
        }

        fun main() {
            let wrapped = Wrap { value = 41 }.map(|n| stringify(n)).map(|text| measure(text));
            print(same(wrapped.value, 2));
            print(wrapped.value);
        }
        main();
        "#,
        "true\n2\n",
    );
}

#[test]
fn a_closure_grounded_generic_meets_a_method_bound() {
    // The consumer as a METHOD with its own bounded generic (the `swap` shape)
    // rather than a free function.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        struct Wrap<V> {
            value: V,
        }

        impl Wrap<type V> {
            fun map<U>(self, transform: |V| U): Wrap<U> {
                Wrap { value = transform(self.value) }
            }
        }

        struct Gate {
            open: bool,
        }

        impl Gate {
            fun admits<T: PartialEq>(self, wrapped: Wrap<T>): bool {
                self.open && wrapped.value == wrapped.value
            }
        }

        fun parse(text: str): i32 {
            text.len()
        }

        fun main() {
            let gate = Gate { open = true };
            let wrapped = Wrap { value = "hi" }.map(|text| parse(text));
            print(gate.admits(wrapped));
        }
        main();
        "#,
        "true\n",
    );
}

// --- B20: named functions as closure values (proposal/fn-coercion.md) --------
//
// A reference to a plain (non-generic, non-method, non-async, non-extern)
// named function coerces to a matching closure type — `map(parse)` instead of
// `map(|path| parse(path))`. On JS the named function IS the value, so the
// whole feature is type-layer.

#[test]
fn a_named_function_passes_as_a_method_closure_argument() {
    // The motivating shape: a method's closure parameter whose return binds
    // the method's own generic (`map<U>`'s `U = Route`) from the FUNCTION's
    // declared return.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Wrap<V> {
            value: V,
        }

        impl Wrap<type V> {
            fun map<U>(self, transform: |V| U): Wrap<U> {
                Wrap { value = transform(self.value) }
            }
        }

        fun measure(text: str): i32 {
            text.len()
        }

        fun main() {
            let wrapped = Wrap { value = "abcd" }.map(measure);
            print(wrapped.value);
        }
        main();
        "#,
        "4\n",
    );
}

#[test]
fn a_named_function_passes_as_a_free_closure_argument() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun apply(seed: i32, transform: |i32| i32): i32 {
            transform(seed)
        }

        fun double(n: i32): i32 {
            n * 2
        }

        fun main() {
            print(apply(21, double));
        }
        main();
        "#,
        "42\n",
    );
}

#[test]
fn a_named_function_binds_to_an_annotated_let_and_field() {
    // The two storage positions: a closure-annotated binding, and a
    // closure-typed struct field (the Kolt server-hook shape).
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Holder {
            hook: |str| i32,
        }

        fun measure(text: str): i32 {
            text.len()
        }

        fun main() {
            let bound: |str| i32 = measure;
            print(bound("abc"));
            let holder = Holder { hook = measure };
            let hook = holder.hook;
            print(hook("abcde"));
        }
        main();
        "#,
        "3\n5\n",
    );
}

#[test]
fn a_named_function_returns_as_a_closure() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun double(n: i32): i32 {
            n * 2
        }

        fun pick(): |i32| i32 {
            double
        }

        fun main() {
            let f = pick();
            print(f(8));
        }
        main();
        "#,
        "16\n",
    );
}

#[test]
fn a_void_function_without_annotation_coerces() {
    // An unannotated-return (void) function into a `|| void` slot — the
    // handler shape; the return type comes from the body's inferred type.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun run_twice(action: || void) {
            action();
            action();
        }

        fun say_hi() {
            print("hi");
        }

        fun main() {
            run_twice(say_hi);
        }
        main();
        "#,
        "hi\nhi\n",
    );
}

#[test]
fn a_stored_function_value_survives_shared_storage() {
    // Through `Shared<|str| i32>` — stored as a value, read back, called
    // indirectly (the pilot's hook pattern, without the eta-expansion).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;

        fun measure(text: str): i32 {
            text.len()
        }

        fun main() {
            let hook: Shared<|str| i32> = Shared::new(measure);
            let stored = hook.read();
            print(stored("abcd"));
        }
        main();
        "#,
        "4\n",
    );
}

#[test]
fn a_mismatched_function_still_fails_closure_positions() {
    // Wrong parameter type: no coercion, the mismatch error stays.
    assert_fails(
        r#"
        fun apply(seed: i32, transform: |i32| i32): i32 {
            transform(seed)
        }

        fun shout(text: str): str {
            text + "!"
        }

        fun main() {
            apply(3, shout);
        }
        "#,
    );
}

#[test]
fn a_generic_function_does_not_coerce() {
    // Rule 2: no single value exists for a generic function (which
    // instantiation?) — deferred, still the mismatch error.
    assert_fails(
        r#"
        fun apply(seed: i32, transform: |i32| i32): i32 {
            transform(seed)
        }

        fun identity<T>(value: T): T {
            value
        }

        fun main() {
            apply(3, identity);
        }
        "#,
    );
}

#[test]
fn an_async_function_does_not_coerce() {
    // Rule 4: a call through a plain closure value is not awaited, so the
    // coerced value would leak a raw promise — rejected.
    assert_fails(
        r#"
        fun apply(seed: i32, transform: |i32| i32): i32 {
            transform(seed)
        }

        async fun slow_double(n: i32): i32 {
            n * 2
        }

        fun main() {
            apply(3, slow_double);
        }
        "#,
    );
}

#[test]
fn a_context_reading_function_still_cannot_be_a_value() {
    // Rule 5: coercion doesn't bypass the context pass — a needs-context
    // function used as a value keeps its value-use rejection (its hidden
    // parameter can't thread through an indirect call).
    let source = r#"
        import std::context::Context;

        let scope: Context<i32> = Context::new();

        fun reads_scope(): i32 {
            scope.get()
        }

        fun apply(transform: || i32): i32 {
            transform()
        }

        fun main() {
            let result = scope.run(7, || apply(reads_scope));
        }
        main();
        "#;
    match compile(source) {
        Ok(_) => panic!("expected the context value-use rejection, but it compiled"),
        Err(errors) => assert!(
            errors
                .iter()
                .any(|error| error.contains("can't be used as a value")),
            "no diagnostic mentions the value-use rule; got: {errors:#?}"
        ),
    }
}

#[test]
fn an_imported_function_coerces_across_modules() {
    // The reference resolves through an import binding (browser layer:
    // `std::router::segments` is a plain vilan fn) — the coercion and the
    // emitted value must both follow the alias to the defining function.
    assert_compiles_browser(
        r#"
        import std::router::segments;

        fun apply(path: str, transform: |str| List<str>): List<str> {
            transform(path)
        }

        fun main() {
            let parts = apply("/a/b", segments);
        }
        "#,
    );
}

// --- B75: calling a fn-typed binding (fn-coercion.md §4) --------------------
//
// `let f = helper; f(1)` used to fail with "cannot call this as a function: it
// is fn helper(i32): i32". `fn-coercion.md` §4 recorded the opposite ("calling
// such a binding works as before"), so this was a hole, not a refusal: the call
// resolver dispatched on the subject's ENTITY (a binding, not a declaration) and
// never read its TYPE. It reads it now, through the same eligibility predicate
// B20's coercion uses — one rule for what a `fun` value is, so the two can never
// disagree. Emission needed nothing: a fn reference already emits as its own
// (mangled) name, which is why the ANNOTATED form already worked.

#[test]
fn a_fn_typed_binding_calls() {
    // The filed shape, end to end.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun helper(i: i32): i32 {
            i + 1
        }

        fun main() {
            let f = helper;
            print(f(1));
        }
        main();
        "#,
        "2\n",
    );
}

#[test]
fn a_nested_fn_typed_binding_calls() {
    // A `fun` declared inside another function (B71's neighbourhood, where this
    // was found) is the same value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            fun helper(i: i32): i32 {
                i * 3
            }
            let f = helper;
            print(f(4));
        }
        main();
        "#,
        "12\n",
    );
}

#[test]
fn a_fn_typed_binding_calls_at_every_arity() {
    // Arity is the parameter list of the DECLARATION, so zero-, one- and
    // multi-parameter forms all have to come through the one path — and a
    // void-returning one has no declared return to read.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun nothing(): i32 {
            7
        }

        fun two(a: i32, b: i32): i32 {
            a * b
        }

        fun shout(text: str) {
            print(text);
        }

        fun main() {
            let n = nothing;
            let t = two;
            let s = shout;
            print(n());
            print(t(3, 4));
            s("hi");
        }
        main();
        "#,
        "7\n12\nhi\n",
    );
}

#[test]
fn a_rebound_fn_typed_binding_still_calls() {
    // The type rides through a chain of bindings — each `let` copies
    // `Type::Function(id)`, so the last one resolves the same declaration.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun helper(i: i32): i32 {
            i + 10
        }

        fun main() {
            let f = helper;
            let g = f;
            let h = g;
            print(h(5));
        }
        main();
        "#,
        "15\n",
    );
}

#[test]
fn a_fn_typed_binding_composes_with_the_b20_coercion() {
    // The two directions must compose: bind a `fun` unannotated, CALL it, and
    // also hand the same binding to a closure-typed parameter (where B20's
    // coercion converts it). One value, both uses, in one program.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun double(i: i32): i32 {
            i * 2
        }

        fun apply(transform: |i32| i32, value: i32): i32 {
            transform(value)
        }

        fun main() {
            let f = double;
            print(f(4));
            print(apply(f, 5));
        }
        main();
        "#,
        "8\n10\n",
    );
}

#[test]
fn a_closure_typed_parameter_rebinds_and_calls() {
    // The receiving end: a closure-typed parameter rebound to a plain `let` and
    // called through the copy. This one always worked (the parameter's declared
    // type is `Type::Closure`); it pins that widening the call operator did not
    // disturb it.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun double(i: i32): i32 {
            i * 2
        }

        fun apply(transform: |i32| i32, value: i32): i32 {
            let inner = transform;
            inner(value)
        }

        fun main() {
            print(apply(double, 6));
        }
        main();
        "#,
        "12\n",
    );
}

#[test]
fn a_fn_typed_binding_checks_its_arguments() {
    // Resolving through the declaration is what buys the ordinary checks: a
    // wrong argument TYPE through a binding reports like any other call.
    assert_fails_with(
        r#"
        fun helper(i: i32): i32 {
            i + 1
        }

        fun main() {
            let f = helper;
            let bad = f("text");
        }
        "#,
        "Expected i32, but got str instead.",
    );
}

#[test]
fn a_fn_typed_binding_checks_its_arity() {
    // The arity check comes from the same path, so it reports the declaration's
    // parameter count rather than silently accepting — and (S4,
    // editing-dx.md §6.2) names it by the DECLARATION's name, `helper`, even
    // though the call goes through the binding `f`.
    assert_fails_with(
        r#"
        fun helper(i: i32): i32 {
            i + 1
        }

        fun main() {
            let f = helper;
            let bad = f(1, 2);
        }
        "#,
        "`helper` expects 1 argument, but got 2 instead.",
    );
}

// --- S4: count messages name their subject (editing-dx.md §6-7) -----------
// `Expected 2 arguments, but got 1 instead.` named neither the callee nor
// what was missing; a struct-field count named neither the struct. Both now
// do, and the too-few direction also names the missing parameter/field by
// name and (for a call) its declared type — arguments and fields bind
// positionally/by-name, so which one is missing is unambiguous. Too many
// names the callee (P15/P16) but, for a call, not which argument is extra
// (B4: no principled guess); a struct literal's extra field IS identifiable
// (P18), since fields are named.

// P15 — a plain function call, too few arguments.
#[test]
fn call_argument_count_too_few_names_the_callee_and_the_missing_parameter() {
    assert_fails_spanning(
        r#"
        fun distance(x: i32, y: i32): i32 {
        	x + y
        }

        fun main() {
        	distance(3);
        }
        "#,
        "(3)",
        "`distance` expects 2 arguments, but got 1 instead: `y: i32` is missing.",
    );
}

// P15 — the same call, too many arguments: the callee is named; which
// argument is extra is not (B4 — no principled guess).
#[test]
fn call_argument_count_too_many_names_only_the_callee() {
    assert_fails_spanning(
        r#"
        fun distance(x: i32, y: i32): i32 {
        	x + y
        }

        fun main() {
        	distance(3, 4, 5);
        }
        "#,
        "(3, 4, 5)",
        "`distance` expects 2 arguments, but got 3 instead.",
    );
}

// The C3 "declared here" note (editing-dx.md §17.3, the residual §16
// deferred): an arity mismatch also notes the callee's OWN declaration, in
// the wording the codebase already uses for this note
// (``` `{name}` is declared here ```, const_eval.rs / init_order.rs) — so a
// call far from its definition doesn't leave the reader hunting for it.
#[test]
fn call_argument_count_notes_the_callees_declaration() {
    assert_fails_noting(
        r#"
        fun distance(x: i32, y: i32): i32 {
        	x + y
        }

        fun main() {
        	distance(3);
        }
        "#,
        "`distance` expects 2 arguments",
        "distance",
        "`distance` is declared here",
    );
}

// P16 — a method call behaves identically, naming the METHOD (not the
// receiver or the struct).
#[test]
fn method_argument_count_too_few_names_the_method_and_the_missing_parameter() {
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }
        impl Point {
        	fun shift(self, dx: i32, dy: i32): Point {
        		Point { x = self.x + dx, y = self.y + dy }
        	}
        }

        fun main() {
        	let origin: Point = Point { x = 0, y = 0 };
        	origin.shift(1);
        }
        "#,
        "(1)",
        "`shift` expects 2 arguments, but got 1 instead: `dy: i32` is missing.",
    );
}

#[test]
fn method_argument_count_too_many_names_only_the_method() {
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }
        impl Point {
        	fun shift(self, dx: i32, dy: i32): Point {
        		Point { x = self.x + dx, y = self.y + dy }
        	}
        }

        fun main() {
        	let origin: Point = Point { x = 0, y = 0 };
        	origin.shift(1, 2, 3);
        }
        "#,
        "(1, 2, 3)",
        "`shift` expects 2 arguments, but got 3 instead.",
    );
}

// The C3 note again, for a METHOD's arity: notes the method's own
// declaration inside `impl Point`, not the struct itself — `declared_here_
// note` resolves `member_id` the same way `callable_name` does.
#[test]
fn method_argument_count_notes_the_methods_declaration() {
    assert_fails_noting(
        r#"
        struct Point { x: i32, y: i32 }
        impl Point {
        	fun shift(self, dx: i32, dy: i32): Point {
        		Point { x = self.x + dx, y = self.y + dy }
        	}
        }

        fun main() {
        	let origin: Point = Point { x = 0, y = 0 };
        	origin.shift(1);
        }
        "#,
        "`shift` expects 2 arguments",
        "shift",
        "`shift` is declared here",
    );
}

// P17 — a wrapped argument list clamps its span to the first line: a count
// is a property of the whole list, not of how many lines the formatter
// split it across (§13.3). Checked by byte offset, not `assert_fails_spanning`,
// because the clamped span is not a source SUBSTRING (it ends mid-line at
// the newline, not at a token boundary the snippet-search would find).
#[test]
fn call_argument_count_span_clamps_to_the_first_line_of_a_wrapped_list() {
    let source = "
        fun distance(x: i32, y: i32): i32 {
        \tx + y
        }

        fun main() {
        \tdistance(
        \t\t3,
        \t);
        }
        ";
    let diagnostics = failure_diagnostics(source);
    let (message, range) = diagnostics
        .iter()
        .find(|(message, _)| message.contains("`distance` expects 2 arguments"))
        .expect("the arity diagnostic is published");
    assert_eq!(
        message,
        "`distance` expects 2 arguments, but got 1 instead: `y: i32` is missing.",
    );
    let call_open_paren = source.rfind("distance(").unwrap() + "distance".len();
    let first_line_end = source[call_open_paren..].find('\n').unwrap() + call_open_paren;
    assert_eq!(
        range.start, call_open_paren,
        "starts at the argument list's `(`"
    );
    assert!(
        range.end <= first_line_end,
        "clamped to the first line: {range:?} runs past {first_line_end} into the second"
    );
    assert!(
        range.end > call_open_paren,
        "not clamped into nothing: {range:?}"
    );
}

// P18 — a struct initializer, too few fields: names the struct and the
// missing field; the brace region stays the anchor (the gap has no
// narrower home).
#[test]
fn struct_initializer_field_count_too_few_names_the_struct_and_the_missing_field() {
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
        	let origin: Point = Point { x = 3 };
        }
        "#,
        "{ x = 3 }",
        "`Point` expects 2 fields, but got 1 instead: `y` is missing.",
    );
}

// P18 — too many fields: unlike an extra call argument, an extra struct
// field IS identifiable (fields are named), so this direction gets a steer
// too, and the anchor moves to the offending field's NAME.
#[test]
fn struct_initializer_field_count_too_many_names_the_struct_and_spans_the_extra_field() {
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
        	let origin: Point = Point { x = 3, y = 4, z = 5 };
        }
        "#,
        "z",
        "`Point` expects 2 fields, but got 3 instead: `z` is not a field of `Point`.",
    );
}

// The C3 note for a struct-field count mismatch: notes the struct's OWN
// declaration, the same wording and mechanism as the call-arity notes
// above, built by hand at the initializer's own push site (the subject is
// a `Struct`, which `declared_here_note` — scoped to callables — does not
// resolve).
#[test]
fn struct_initializer_field_count_notes_the_structs_declaration() {
    assert_fails_noting(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
        	let origin: Point = Point { x = 3 };
        }
        "#,
        "`Point` expects 2 fields",
        "Point",
        "`Point` is declared here",
    );
}

#[test]
fn a_fn_typed_binding_types_its_result() {
    // The call's TYPE is the declaration's return type, not `Unknown`: a `str`
    // result used as an `i32` has to fail.
    assert_fails_with(
        r#"
        fun name(): str {
            "x"
        }

        fun main() {
            let f = name;
            let n: i32 = f();
        }
        "#,
        "Expected i32, but got str instead.",
    );
}

// The four functions with no value form (`fn-coercion.md` §1 rules 1-4,
// `spec/types.md` §5.8). Each is DEFERRED there with its own reason, so a
// binding cannot hold one to call either — pinned so that widening the call
// operator can never quietly widen the value form with it. Each message names
// the disqualifying property, which is what the tour promises ("the compiler
// will tell you when you hit one").

#[test]
fn a_generic_fn_typed_binding_does_not_call() {
    // Rule 2. Left open, this MISCOMPILED: the binding emits the declaration's
    // name, monomorphization mints instance names from a disjoint pool, and the
    // call reached a name specialization never produced (`$a is not defined`).
    assert_fails_with(
        r#"
        fun identity<T>(x: T): T {
            x
        }

        fun main() {
            let f = identity;
            let n = f(1);
        }
        "#,
        "a generic function has no single value",
    );
}

#[test]
fn an_async_fn_typed_binding_does_not_call() {
    // Rule 4. Left open, this compiled and printed `Promise { 2 }`: a call
    // through a value is not awaited (the J2 gap), so the promise leaks.
    assert_fails_with(
        r#"
        async fun fetchy(i: i32): i32 {
            i + 1
        }

        async fun main() {
            let f = fetchy;
            let n = f(1);
        }
        "#,
        "an `async` function has no value form",
    );
}

#[test]
fn a_method_fn_typed_binding_does_not_call() {
    // Rule 3 — `x.method` as a value means receiver capture, deferred there.
    // `Bag::bump` types as `fn bump(Bag): i32`, so without the gate it would
    // have become callable as a side effect of this change.
    assert_fails_with(
        r#"
        struct Bag { n: i32 }

        impl Bag {
            fun bump(self): i32 {
                self.n + 1
            }
        }

        fun main() {
            let f = Bag::bump;
            let b = Bag { n = 1 };
            let n = f(b);
        }
        "#,
        "a method has no value form",
    );
}

#[test]
fn an_external_fn_typed_binding_does_not_call() {
    // Rule 1 — an extern's binding forms are call-shaped, so there is no sound
    // value to hold.
    assert_fails_with(
        r#"
        [extern("parseInt")]
        external fun parse_int(text: str): i32;

        fun main() {
            let f = parse_int;
            let n = f("12");
        }
        "#,
        "an `external` function has no value form",
    );
}

// --- K5: `std::time` + i53 on the wire (kolt-migration.md §2.5) --------------
//
// The runtime surface (arithmetic, describe, ISO, codec round-trips, sleep) is
// pinned by the corpus (`vilan/test/time.vl`, node-run; interpreter-excluded —
// host clock). These pin the compile-level rules.

#[test]
fn the_clock_is_not_const_evaluable() {
    // `now()` reads the host clock — an impure capability. A `const` forcing
    // it must fail at compile time, not fold a build-machine timestamp into
    // the program.
    let source = r#"
        import std::time::now;
        import std::io::print;

        fun main() {
            let moment = const now();
            print(moment.millis);
        }
        main();
        "#;
    match compile(source) {
        Ok(_) => panic!("expected `const now()` to be rejected, but it compiled"),
        Err(errors) => assert!(
            errors
                .iter()
                .any(|error| error.contains("unknown host call `Date.now`")),
            "no diagnostic rejects the host clock under const; got: {errors:#?}"
        ),
    }
}

#[test]
fn time_is_platform_neutral() {
    // `Date.now`/`Date`/`setTimeout` exist on every host, so the module lives
    // in the base layer: the same program compiles for node AND browser.
    let source = r#"
        import std::time::{ now, sleep_for, Instant, Duration };

        async fun main() {
            let anchor = Instant { millis = 0i53 };
            let age = now().since(anchor) + Duration::minutes(1);
            let _rendered = age.describe();
            let _shifted = now() - Duration::hours(1) + Duration::seconds(30);
            sleep_for(Duration::millis(1i53));
        }
        "#;
    assert_compiles(source);
    assert_compiles_browser(source);
}

#[test]
fn i53_fields_are_wire() {
    // The K5 blocker, closed: `i53` is a Wire scalar (its own serializer
    // channel), so timestamps and row ids ride derives directly — including
    // nested through `Instant` and `List`/`Option`.
    assert_compiles(
        r#"
        import std::time::Instant;
        import std::option::Option;

        [derive(Wire)]
        struct Task {
            id: i53,
            created_at: Instant,
            due: Option<i53>,
            checkpoints: List<i53>,
        }

        fun main() {
            let _task = Task {
                id = 9007199254740991i53,
                created_at = Instant { millis = 0i53 },
                due = Option::None,
                checkpoints = [1i53, 2i53],
            };
        }
        "#,
    );
}

#[test]
fn i53_signatures_are_rpc_legal() {
    // The `[rpc]` Wire-signature rule shares the scalar set: i53 parameters
    // and returns are legal.
    assert_compiles(
        r#"
        import std::reactive::{ Signal, SignalCell };

        [service(TickClient)]
        struct Ticker {
            [expose] latest: SignalCell<i53>,
        }

        impl Ticker {
            [rpc]
            fun record(self, at: i53): i53 {
                at
            }
        }

        fun main() {
            let _ticker = Ticker { latest = Signal::new(0i53) };
        }
        "#,
    );
}

#[test]
fn non_wire_fields_still_fail() {
    // The gate holds around the new scalar: a closure-typed field is still
    // rejected by the Wire boundary.
    assert_fails_spanning(
        r#"
        [derive(Wire)]
        struct Holder {
            callback: |i53| i53,
        }
        "#,
        "|i53| i53",
        "which is not Wire",
    );
}

// --- `std::time::Timer` — the cancelable timer -------------------------------
//
// `setTimeout`/`clearTimeout` as one value (backlog-2026-07-18.md's "per-task
// cancel handles" first field case). One pin per numbered semantic. Every
// timing here is ORDERING, never a wall-clock race: a timer armed before a
// longer sleep has fired by the time that sleep returns (node's timer list is
// expiry-ordered), and everything else is cancel-before-fire.

#[test]
fn timer_after_starts_the_host_timer_at_construction() {
    // §1 — the clock starts at `after`, not at the first `wait`. The
    // discriminator is a race the two readings decide differently: the timer
    // is armed for 60ms and left alone for 90ms, then its `wait` is run
    // against a fresh 30ms sleep. Started at construction it has already
    // fired, so its wait resolves on the microtask queue and wins; started
    // lazily at `wait` it would need 60ms and lose to the 30ms sleeper.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::task::nursery;
        import std::time::{ sleep, Timer };

        fun main() {
            let timer = Timer::after(60);
            sleep(90);

            let order: Shared<List<str>> = Shared::new([]);
            nursery(|n| {
                let _fired = async {
                    order.write().push(i"timer:{timer.wait()}");
                };
                let _slept = async {
                    sleep(30);
                    order.write().push("sleep");
                };
            });
            for mark in order.read() {
                print(mark);
            }
        }
        main();
        "#,
        "timer:true\nsleep\n",
    );
}

#[test]
fn timer_after_for_mirrors_sleep_for() {
    // §1 — the `Duration` spelling is the same timer (an i32-ms cap, like
    // `sleep_for`): armed at construction, fires, verdict `true`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::{ sleep, Duration, Timer };

        fun main() {
            let timer = Timer::after_for(Duration::millis(1i53));
            sleep(30);
            print(timer.wait());
        }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn timer_wait_gives_concurrent_waiters_one_verdict() {
    // §2 — two tasks parked on the same PENDING timer both observe the one
    // verdict when it fires.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;
        import std::task::nursery;
        import std::time::Timer;

        fun main() {
            let timer = Timer::after(20);
            let seen: Shared<List<str>> = Shared::new([]);
            nursery(|n| {
                let _one = async {
                    seen.write().push(i"one:{timer.wait()}");
                };
                let _two = async {
                    seen.write().push(i"two:{timer.wait()}");
                };
            });
            for mark in seen.read() {
                print(mark);
            }
        }
        main();
        "#,
        "one:true\ntwo:true\n",
    );
}

#[test]
fn timer_wait_after_settlement_returns_the_memoized_verdict() {
    // §2 — the verdict is MEMOIZED, not a second timer: waiting a settled
    // timer answers immediately, as often as you ask, on both verdicts.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::{ sleep, Timer };

        fun main() {
            let fired = Timer::after(1);
            sleep(30);
            print(i"{fired.wait()} {fired.wait()}");

            let called_off = Timer::after(60000);
            called_off.cancel();
            print(i"{called_off.wait()} {called_off.wait()}");
        }
        main();
        "#,
        "true true\nfalse false\n",
    );
}

#[test]
fn timer_cancel_before_settlement_resolves_waiters_false() {
    // §3 — a waiter parked before the cancel resolves `false` at once, and so
    // does everyone who asks afterwards.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::task::nursery;
        import std::time::{ sleep, Timer };

        fun main() {
            let timer = Timer::after(60000);
            nursery(|n| {
                let _waiter = async {
                    print(i"waiter:{timer.wait()}");
                };
                sleep(5);
                timer.cancel();
            });
            print(i"after:{timer.wait()}");
        }
        main();
        "#,
        "waiter:false\nafter:false\n",
    );
}

#[test]
fn timer_cancel_clears_the_host_timer() {
    // §3 — the other half of `cancel`, which stdout cannot show: settling the
    // verdict is not enough, the host timer must be CLEARED or a cancelled
    // timer would go on holding the process open (see
    // `a_pending_timer_keeps_the_process_alive`). Pinned on the emitted
    // helper, since process-exit timing is only observable as a wall-clock
    // race.
    let js = compile(
        r#"
        import std::io::print;
        import std::time::Timer;

        fun main() {
            let timer = Timer::after(60000);
            timer.cancel();
            print(timer.wait());
        }
        main();
        "#,
    )
    .expect("a timer program compiles");
    assert!(
        js.contains("\tcancel() {\n\t\tif (this.settled) return;\n\t\tclearTimeout(this.id);\n"),
        "`cancel` must clear the host timer before settling: {js}"
    );
}

#[test]
fn timer_cancel_after_firing_is_a_no_op() {
    // §3 — first settlement wins forever: a late cancel never rewrites a
    // `true` verdict into a `false` one.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::{ sleep, Timer };

        fun main() {
            let timer = Timer::after(1);
            sleep(30);
            timer.cancel();
            print(timer.wait());
        }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn timer_cancel_is_idempotent() {
    // §3 — cancelling twice is cancelling once; the second call finds the
    // timer settled and does nothing.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::Timer;

        fun main() {
            let timer = Timer::after(60000);
            timer.cancel();
            timer.cancel();
            timer.cancel();
            print(timer.wait());
        }
        main();
        "#,
        "false\n",
    );
}

#[test]
fn a_cancelling_nursery_tears_down_the_waiter_but_not_the_timer() {
    // §4 — the sharp distinction. `wait` carries the ambient cancel signal the
    // way `sleep` does, so a cancelling nursery unwinds the task that was
    // awaiting (neither UNREACHED line prints) — but that is structured
    // teardown of ONE waiter, not a verdict: the timer is neither settled nor
    // cleared, so afterwards `waited` still fires `true` and `called_off` is
    // still cancellable to `false` by the holder of the value.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::task::nursery;
        import std::time::{ sleep, Timer };

        fun main() {
            let waited = Timer::after(60);
            let called_off = Timer::after(60);
            nursery(|n| {
                let _a = async {
                    print(i"UNREACHED-a:{waited.wait()}");
                };
                let _b = async {
                    print(i"UNREACHED-b:{called_off.wait()}");
                };
                sleep(5);
                n.cancel();
            });
            print("nursery returned");
            called_off.cancel();
            print(i"called_off:{called_off.wait()}");
            print(i"waited:{waited.wait()}");
        }
        main();
        "#,
        "nursery returned\ncalled_off:false\nwaited:true\n",
    );
}

#[test]
fn a_timer_that_fires_with_no_waiters_memoizes_true() {
    // §5 — nothing has to be awaiting a timer for it to run out; the verdict
    // is waiting when someone finally asks.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::{ sleep, Timer };

        fun main() {
            let timer = Timer::after(1);
            sleep(30);
            print(timer.wait());
        }
        main();
        "#,
        "true\n",
    );
}

#[test]
fn a_pending_timer_keeps_the_process_alive() {
    // §6 — parity with `sleep`, and no unref knob. `main` returns with the
    // timer pending and the only other thing in flight a task awaiting it. A
    // pending promise does NOT hold node open by itself, so the second line
    // prints only because the host timer does.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::Timer;

        fun main() {
            let timer = Timer::after(30);
            let _watcher = async {
                print(i"fired:{timer.wait()}");
            };
            print("main done");
        }
        main();
        "#,
        "main done\nfired:true\n",
    );
}

#[test]
fn copying_a_timer_shares_the_underlying_host_timer() {
    // §7 — an ordinary value wrapping one external handle, like `Signal`:
    // assigning it and passing it to a function both alias the ONE timer, so
    // a cancel through any copy settles every copy.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::Timer;

        fun call_off(timer: Timer) {
            timer.cancel();
        }

        fun main() {
            let original = Timer::after(60000);
            let copy = original;
            copy.cancel();
            print(i"{original.wait()} {copy.wait()}");

            let passed = Timer::after(60000);
            call_off(passed);
            print(passed.wait());
        }
        main();
        "#,
        "false false\nfalse\n",
    );
}

#[test]
fn timers_are_platform_neutral() {
    // `setTimeout`/`clearTimeout` exist on every host, so `Timer` stays in
    // std's base layer alongside `sleep` — the same program compiles for node
    // AND browser.
    let source = r#"
        import std::time::{ Duration, Timer };

        fun main() {
            let timer = Timer::after_for(Duration::seconds(1i53));
            let _verdict = timer.wait();
            timer.cancel();
        }
        "#;
    assert_compiles(source);
    assert_compiles_browser(source);
}

// --- B22: return-expectation inference bound to the caller's generics --------
//
// A call's return-type-only generic inference (the `let n: Cell<i32> =
// Cell::fresh()` gap-filler) must bind only the CALLEE's own generics. When an
// abstract argument already bound the callee's `T` to the caller's `T`, the
// substituted return type's generics are the caller's — unifying THOSE against
// the expectation wrote a caller-keyed entry into the call's substitution map,
// and the bound check then demanded the caller generic's bounds of whatever it
// unified with (a raw unbounded struct binder), rejecting valid code.

#[test]
fn a_bounded_caller_constructs_an_unbounded_struct_via_a_generic_static_new() {
    // The motivating shape (std::reactive's `draft()`): `fun draft<T:
    // PartialEq>` building a struct whose field is made by an UNBOUNDED
    // generic container's static `new`. The field expectation mentions the
    // struct's raw binder; the call's return mentions the caller's `T` — the
    // poison unification paired the two and demanded `PartialEq` of the
    // struct binder.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        struct Cell<T> {
            value: T,
        }

        impl Cell<type T> {
            fun new(value: T): Cell<T> {
                Cell { value }
            }
        }

        struct Box<T> {
            inner: Cell<T>,
        }

        fun boxed<T: PartialEq>(initial: T): Box<T> {
            Box {
                inner = Cell::new(initial),
            }
        }

        fun main() {
            let held = boxed(3);
            print(held.inner.value);
        }
        main();
        "#,
        "3\n",
    );
}

#[test]
fn two_bounded_generics_construct_two_unbounded_fields() {
    // Multi-parameter form: each field's constructor call must stay keyed to
    // its own binding — before the fix BOTH `A` and `B` were rejected.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        struct Cell<T> {
            value: T,
        }

        impl Cell<type T> {
            fun new(value: T): Cell<T> {
                Cell { value }
            }
        }

        struct Duo<A, B> {
            left: Cell<A>,
            right: Cell<B>,
        }

        fun paired<A: PartialEq, B: PartialEq>(first: A, second: B): Duo<A, B> {
            Duo {
                left = Cell::new(first),
                right = Cell::new(second),
            }
        }

        fun main() {
            let held = paired(1, "two");
            print(held.left.value);
            print(held.right.value);
        }
        main();
        "#,
        "1\ntwo\n",
    );
}

#[test]
fn a_nested_generic_argument_still_binds_through_the_expectation() {
    // Nested form: the caller's `T` sits INSIDE the callee's binding
    // (`Cell::new([initial])` binds the callee's `T` to `List<T_caller>`).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        struct Cell<T> {
            value: T,
        }

        impl Cell<type T> {
            fun new(value: T): Cell<T> {
                Cell { value }
            }
        }

        struct Box<T> {
            inner: Cell<List<T>>,
        }

        fun boxed<T: PartialEq>(initial: T): Box<T> {
            Box {
                inner = Cell::new([initial]),
            }
        }

        fun main() {
            let held = boxed(7);
            print(held.inner.value[0]);
        }
        main();
        "#,
        "7\n",
    );
}

#[test]
fn return_type_only_inference_still_binds_a_static_generic() {
    // The feature the merge exists for keeps working: no argument mentions
    // `T`, so the expectation is the only thing that can bind it — the
    // callee's own return-type generic must still be inferred.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Cell<T> {
            value: List<T>,
        }

        impl Cell<type T> {
            fun fresh(): Cell<T> {
                Cell { value = [] }
            }
        }

        fun main() {
            let cell: Cell<i32> = Cell::fresh();
            print(cell.value.len());
        }
        main();
        "#,
        "0\n",
    );
}

// --- Draft<T>: local-first cells (std::reactive, kolt-migration §3) ----------
//
// `draft(initial, commit)` is a local-first cell: edits land in `local`
// FIRST (`push` spawns the commit, never awaits it), `adopt` folds in remote
// changes without fighting in-flight edits, and failure KEEPS the local value
// (unlike `optimistic`'s rollback — right for one-shot actions, hostile
// mid-typing). Conflicts are last-write-wins.

#[test]
fn draft_push_is_local_first_and_settles_synced() {
    // `push` returns with `local` set and the state Dirty while the commit
    // is still on the wire; the settle lands afterwards.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let committed: Shared<List<str>> = Shared::new([]);
            let name = draft("seed", |value: str| {
                sleep_for(Duration::millis(5));
                committed.write().push(value);
                None
            });
            print(name.state.get() == DraftState::Synced);
            name.push("edit");
            print(name.local.get());
            print(name.state.get() == DraftState::Dirty);
            sleep_for(Duration::millis(20));
            print(name.state.get() == DraftState::Synced);
            print(committed.read().len());
        }
        main();
        "#,
        "true\nedit\ntrue\ntrue\n1\n",
    );
}

#[test]
fn draft_adopt_echo_is_a_no_op() {
    // A pushed value reflected back by the remote (the mirror echo) changes
    // nothing — state stays Synced, `local` untouched.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::time::{ sleep_for, Duration };

        fun main() {
            let name = draft("seed", |value: str| {
                let _sent = value;
                None
            });
            name.push("edit");
            sleep_for(Duration::millis(10));
            name.adopt("edit");
            print(name.local.get());
            print(name.state.get() == DraftState::Synced);
        }
        main();
        "#,
        "edit\ntrue\n",
    );
}

#[test]
fn draft_adopt_takes_remote_when_local_is_clean() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };

        fun main() {
            let name = draft("seed", |value: str| {
                let _sent = value;
                None
            });
            name.adopt("remote");
            print(name.local.get());
            print(name.synced.read());
            print(name.state.get() == DraftState::Synced);
        }
        main();
        "#,
        "remote\nremote\ntrue\n",
    );
}

#[test]
fn draft_failure_keeps_the_local_value() {
    // Unlike `optimistic`, no rollback: the user's text survives the failed
    // commit, and the state carries the reason.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::time::{ sleep_for, Duration };

        fun main() {
            let sour = draft("base", |value: str| {
                let _sent = value;
                Some("boom")
            });
            sour.push("mine");
            sleep_for(Duration::millis(10));
            print(sour.state.get() == DraftState::Failed("boom"));
            print(sour.local.get());
            print(sour.synced.read());
        }
        main();
        "#,
        "true\nmine\nbase\n",
    );
}

#[test]
fn draft_dirty_local_survives_adoption() {
    // Last-write-wins: a dirty local ignores the remote value in `local`
    // (the user's text wins for now) while `synced` records it, so the
    // eventual push knowingly overwrites.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::time::{ sleep_for, Duration };

        fun main() {
            let sour = draft("base", |value: str| {
                let _sent = value;
                Some("boom")
            });
            sour.push("mine");
            sleep_for(Duration::millis(10));
            sour.adopt("theirs");
            print(sour.local.get());
            print(sour.synced.read());
        }
        main();
        "#,
        "mine\ntheirs\n",
    );
}

#[test]
fn draft_generation_guard_discards_superseded_pushes() {
    // Fast typing over a slow wire: the first push's commit lands LAST, but
    // only the newest push settles the state — the stale completion is
    // discarded.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::time::{ sleep_for, Duration };

        fun main() {
            let raced = draft("start", |value: str| {
                if value == "slow" {
                    sleep_for(Duration::millis(30));
                } else {
                    sleep_for(Duration::millis(5));
                }
                None
            });
            raced.push("slow");
            raced.push("fast");
            sleep_for(Duration::millis(60));
            print(raced.local.get());
            print(raced.synced.read());
            print(raced.state.get() == DraftState::Synced);
        }
        main();
        "#,
        "fast\nfast\ntrue\n",
    );
}

#[test]
fn draft_push_publishes_one_coherent_wave() {
    // A lifecycle transition writes TWO signals (`local` and `state`), so an
    // observer of both must never see half of one
    // (`proposal/optimistic-lifecycle.md` §5). Under a UI boundary turn they
    // coalesced already — `View.on` wraps every dispatch — but with NO ambient
    // turn (a node program, SSR, a test) `push` published the new text still
    // claiming `Synced` before publishing `Dirty`. `batch` joins the ambient
    // turn when there is one and creates one when there is not, so the middle
    // is unobservable either way.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState, combine };
        import std::option::Option::{ self, Some, None };
        import std::time::{ sleep_for, Duration };

        fun label(state: DraftState): str {
            match state {
                DraftState::Synced => "synced",
                DraftState::Dirty => "dirty",
                DraftState::Failed(let reason) => reason,
            }
        }

        fun main() {
            let cell = draft("A", |value: str| {
                let _sent = value;
                sleep_for(Duration::millis(5));
                let outcome: Option<str> = None;
                outcome
            });
            let both = combine((cell.local, cell.state));
            let _watch = both.sub(|pair| {
                let (text, state) = pair;
                print(i"{text}/{label(state)}");
            });
            cell.push("B");
            sleep_for(Duration::millis(20));
        }
        main();
        "#,
        "A/synced\nB/dirty\nB/synced\n",
    );
}

#[test]
fn draft_adoption_publishes_one_coherent_wave() {
    // The same rule on the other two-signal transition: `adopt`'s clean
    // branch writes `local` and `state`, and must publish them together.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState, combine };
        import std::option::Option::{ self, Some, None };

        fun label(state: DraftState): str {
            match state {
                DraftState::Synced => "synced",
                DraftState::Dirty => "dirty",
                DraftState::Failed(let reason) => reason,
            }
        }

        fun main() {
            let cell = draft("A", |value: str| {
                let _sent = value;
                let outcome: Option<str> = None;
                outcome
            });
            let both = combine((cell.local, cell.state));
            let _watch = both.sub(|pair| {
                let (text, state) = pair;
                print(i"{text}/{label(state)}");
            });
            cell.adopt("remote");
        }
        main();
        "#,
        "A/synced\nremote/synced\n",
    );
}

// --- Draft re-push on reconnect (A14, proposal/draft-reconnect.md) ----------
//
// `repush()` re-sends edits the remote never accepted — `local != synced`,
// which covers an edit whose commit never left AND one caught in flight by
// the drop. Wired to a transport's reconnect hook, a dropped connection
// stops losing the user's work. Delivery is at-least-once, by construction.

#[test]
fn draft_repush_resends_edits_the_remote_never_accepted() {
    // The outage shape: a commit that fail-fast rejects while down leaves
    // `local` ahead of `synced`, and the re-push sends it EXACTLY once —
    // then settles the draft, so a second reconnect sends nothing.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let down: Shared<bool> = Shared::new(true);
            let sent: Shared<List<str>> = Shared::new([]);
            let title = draft("base", |value: str| {
                sent.write().push(value);
                if down.read() { Some("not connected") } else { None }
            });

            title.push("mine");
            sleep_for(Duration::millis(10));
            print(sent.read().len());
            print(title.state.get() == DraftState::Failed("not connected"));
            print(title.synced.read());

            // The connection comes back.
            down.write() = false;
            title.repush();
            sleep_for(Duration::millis(10));
            print(sent.read().len());
            print(title.synced.read());
            print(title.state.get() == DraftState::Synced);

            // A LATER reconnect has nothing left to send.
            title.repush();
            sleep_for(Duration::millis(10));
            print(sent.read().len());
        }
        main();
        "#,
        "1\ntrue\nbase\n2\nmine\ntrue\n2\n",
    );
}

#[test]
fn draft_repush_on_a_clean_draft_sends_nothing() {
    // `local == synced` — the remote already has everything. A screen full
    // of untouched drafts costs zero frames on reconnect.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let sent: Shared<List<str>> = Shared::new([]);
            let name = draft("seed", |value: str| {
                sent.write().push(value);
                None
            });

            // Never edited.
            name.repush();
            sleep_for(Duration::millis(10));
            print(sent.read().len());

            // Edited, pushed, settled — clean again.
            name.push("edit");
            sleep_for(Duration::millis(10));
            print(sent.read().len());
            name.repush();
            sleep_for(Duration::millis(10));
            print(sent.read().len());
            print(name.state.get() == DraftState::Synced);
        }
        main();
        "#,
        "0\n1\n1\ntrue\n",
    );
}

#[test]
fn draft_repush_is_at_least_once() {
    // The documented hazard, pinned rather than hidden: a commit that
    // SUCCEEDED server-side but whose acknowledgement was lost with the
    // connection is indistinguishable from one that never arrived, so the
    // re-push sends it again and the server sees it twice. Draft's own
    // reconcile absorbs the duplicate (the state settles once, correctly);
    // an appending commit closure would not.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let server_saw: Shared<List<str>> = Shared::new([]);
            let ack_lost: Shared<bool> = Shared::new(true);
            let title = draft("base", |value: str| {
                // The server applies it either way.
                server_saw.write().push(value);
                if ack_lost.read() { Some("connection lost") } else { None }
            });

            title.push("mine");
            sleep_for(Duration::millis(10));
            ack_lost.write() = false;
            title.repush();
            sleep_for(Duration::millis(10));

            print(server_saw.read().len());
            print(server_saw.read()[0]);
            print(server_saw.read()[1]);
            print(title.state.get() == DraftState::Synced);
            print(title.local.get());
        }
        main();
        "#,
        "2\nmine\nmine\ntrue\nmine\n",
    );
}

#[test]
fn draft_repush_rides_the_reconnect_hook_shape() {
    // The composition the feature actually ships as: the hook is a plain
    // `|| void` in a list, drained the way `handle_drop` drains
    // `SocketDuplex.on_reconnect` — re-marked `async` at a `let` (J2's typed
    // channel) so a hook that awaits does. One reconnect, one re-push.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let down: Shared<bool> = Shared::new(true);
            let sent: Shared<List<str>> = Shared::new([]);
            let title = draft("base", |value: str| {
                sent.write().push(value);
                if down.read() { Some("not connected") } else { None }
            });

            let on_reconnect: Shared<List<|| void>> = Shared::new([]);
            on_reconnect.write().push(|| title.repush());

            title.push("mine");
            sleep_for(Duration::millis(10));
            print(sent.read().len());

            // What the reconnect loop does after a successful re-dial.
            down.write() = false;
            for entry in on_reconnect.read() {
                let hook: async || void = entry;
                hook();
            }
            sleep_for(Duration::millis(10));
            print(sent.read().len());
            print(title.synced.read());
        }
        main();
        "#,
        "1\n2\nmine\n",
    );
}

// --- Draft debounce (A14, proposal/draft-reconnect.md §5) -------------------
//
// `debounce(millis)` coalesces a burst of pushes into ONE commit, trailing
// edge, over a real `std::time::Timer` — cancelling settles the verdict and
// clears the host timeout. Local-first is untouched: the value and the
// Dirty state still land synchronously; only the commit waits.

#[test]
fn draft_debounce_coalesces_a_burst_into_one_commit() {
    // Three keystrokes inside the window produce one commit, carrying the
    // LAST value. Without the window they produce three.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let sent: Shared<List<str>> = Shared::new([]);
            let notes = draft("", |value: str| {
                sent.write().push(value);
                None
            }).debounce(30);

            notes.push("a");
            notes.push("ab");
            notes.push("abc");
            sleep_for(Duration::millis(150));

            print(sent.read().len());
            print(sent.read()[0]);
            print(notes.synced.read());
            print(notes.state.get() == DraftState::Synced);
        }
        main();
        "#,
        "1\nabc\nabc\ntrue\n",
    );
}

#[test]
fn draft_debounce_keeps_the_local_half_synchronous() {
    // The window delays the COMMIT, never the keystroke: `local` and the
    // Dirty state are set before `push` returns, exactly as undebounced.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let sent: Shared<List<str>> = Shared::new([]);
            let notes = draft("", |value: str| {
                sent.write().push(value);
                None
            }).debounce(30);

            notes.push("typed");
            // Same instant: the user's text is there, the wire is not.
            print(notes.local.get());
            print(notes.state.get() == DraftState::Dirty);
            print(sent.read().len());

            sleep_for(Duration::millis(150));
            print(sent.read().len());
        }
        main();
        "#,
        "typed\ntrue\n0\n1\n",
    );
}

#[test]
fn draft_commit_cancels_a_pending_debounce() {
    // The explicit save (a blur, a Save button): the pending window is
    // called off and the value goes now — exactly one commit, not the
    // manual one plus the window's. The sleep deliberately outlasts the
    // window, so a `commit` that failed to cancel shows up as a second
    // commit rather than passing unobserved.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let sent: Shared<List<str>> = Shared::new([]);
            let notes = draft("", |value: str| {
                sent.write().push(value);
                None
            }).debounce(30);

            notes.push("typed");
            notes.commit();
            sleep_for(Duration::millis(150));

            print(sent.read().len());
            print(sent.read()[0]);
            print(notes.state.get() == DraftState::Synced);
        }
        main();
        "#,
        "1\ntyped\ntrue\n",
    );
}

#[test]
fn draft_repush_cancels_a_pending_debounce() {
    // A reconnect arriving mid-window: recovery is not typing, so the
    // window is called off and the edit goes immediately. One commit — the
    // sleep outlasts the window, so a re-push that sent WITHOUT cancelling
    // shows up as the window's second commit.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };
        import std::shared::Shared;
        import std::time::{ sleep_for, Duration };

        fun main() {
            let sent: Shared<List<str>> = Shared::new([]);
            let notes = draft("", |value: str| {
                sent.write().push(value);
                None
            }).debounce(30);

            notes.push("typed");
            print(sent.read().len());
            notes.repush();
            sleep_for(Duration::millis(150));

            print(sent.read().len());
            print(sent.read()[0]);
            print(notes.state.get() == DraftState::Synced);
        }
        main();
        "#,
        "0\n1\ntyped\ntrue\n",
    );
}

#[test]
fn bind_draft_compiles_for_the_browser() {
    // The ui seam: an input two-way bound to a draft (user input pushes;
    // adoption writes `local` and bypasses the push path).
    assert_compiles_browser(
        r#"
        import std::ui::{ view, View, mount_root };
        import std::reactive::{ draft, Draft, DraftState };
        import std::option::Option::{ self, Some, None };

        fun main() {
            let name = draft("seed", |value: str| {
                let _sent = value;
                None
            });
            let _root = mount_root("app", || view("input").bind_draft(name));
        }
        main();
        "#,
    );
}

// --- B23: effect-closure parameter grounding (backlog.md §B.23) --------------

#[test]
fn an_effect_closures_unannotated_parameter_grounds_from_the_signal() {
    // B23, FIXED: the inherited-trait-default path now records the trait's
    // receiver bindings (so `effect`'s `|T| void` types concretely), and
    // `resolve_match` defers on a not-yet-filled closure parameter instead
    // of binding pattern captures against the enum's raw declaration.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, run_with_owner };
        import std::option::Option::{ self, Some, None };

        struct Task {
            name: str,
        }

        fun main() {
            let entry: SignalCell<Option<Task>> = Signal::new(Some(Task { name = "a" }));
            let owner = Owner::new();
            run_with_owner(owner, || {
                entry.effect(|current| {
                    match current {
                        Some(let task) => print(task.name),
                        None => {},
                    }
                });
            });
        }
        main();
        "#,
        "a\n",
    );
}

#[test]
fn an_annotated_effect_parameter_destructures_the_signals_payload() {
    // The pinned workaround (and the kolt draft editor's shipped shape):
    // annotating the parameter grounds everything downstream.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::reactive::{ Signal, SignalCell, Owner, run_with_owner };
        import std::option::Option::{ self, Some, None };

        struct Task {
            name: str,
        }

        fun main() {
            let entry: SignalCell<Option<Task>> = Signal::new(Some(Task { name = "a" }));
            let owner = Owner::new();
            run_with_owner(owner, || {
                entry.effect(|current: Option<Task>| {
                    match current {
                        Some(let task) => print(task.name),
                        None => {},
                    }
                });
            });
        }
        main();
        "#,
        "a\n",
    );
}

// --- Notes finale: cross-source notes + the recorded refinements -------------

#[test]
fn a_missing_trait_member_renders_the_signature_and_notes_the_trait() {
    // The conformance error names the member, renders the signature to
    // write (B4), and its note points INTO std at the trait's own
    // declaration (the first cross-source note).
    let diagnostics = failure_diagnostics_with_notes(
        r#"
        import std::compare::PartialEq;
        struct Point { x: i32 }
        impl Point with PartialEq {}
        fun main() {
            let _p = Point { x = 1 };
        }
        "#,
    );
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _, _)| message.contains("missing 'eq'"))
        .collect();
    assert!(!matching.is_empty(), "{diagnostics:#?}");
    assert!(
        matching
            .iter()
            .any(|(message, _, _)| message.contains("declare `fun eq(")),
        "the expected signature must render: {matching:#?}"
    );
    assert!(
        matching.iter().any(
            |(_, _, note)| note.as_ref().is_some_and(|(msg, _, cross_source)| {
                msg.contains("the trait declares it here") && *cross_source
            })
        ),
        "the note must point into the trait's file: {matching:#?}"
    );
}

#[test]
fn a_bound_failure_notes_the_bounds_declaration() {
    // "does not implement trait 'X', required by a generic bound" now notes
    // WHERE that bound is declared — in the callee's own file (here: this
    // one; std callees make it cross-source).
    assert_fails_noting(
        r#"
        trait Greet {
            fun greet(self): str;
        }
        struct Cat { name: str }
        fun welcome<T: Greet>(guest: T): str {
            guest.greet()
        }
        fun main() {
            let _w = welcome(Cat { name = "tom" });
        }
        "#,
        "does not implement trait 'Greet'",
        "T",
        "the bound is declared here",
    );
}

// --- Diagnostics audit, batch 7: cascades demoted (standard B5) --------------

#[test]
fn a_root_error_does_not_cascade_into_residual_noise() {
    // One unknown name used to produce the root error PLUS "type of
    // variable … could not be resolved" (and friends) for everything
    // downstream of it — five residuals for one cause in the worst
    // observed wall. The residuals are near-information-free, so they
    // surface only as the LONE signal.
    let diagnostics = failure_diagnostics(
        r#"
        fun main() {
            let text = zzz_missing(42);
            let doubled = text;
        }
        "#,
    );
    assert!(
        diagnostics
            .iter()
            .any(|(message, _)| message.contains("cannot find 'zzz_missing'")),
        "the root error must stand: {diagnostics:#?}"
    );
    assert!(
        diagnostics
            .iter()
            .all(|(message, _)| !message.contains("could not be resolved")),
        "residual cascade noise must be demoted behind the root: {diagnostics:#?}"
    );
}

#[test]
fn one_unresolved_name_does_not_cascade_across_many_use_sites() {
    // The multi-use-site form (backlog item 7): one unknown name feeds EVERY
    // residual-producing position — a plain variable, a field access, a call
    // argument, a struct field, and a match subject. Each of these is a
    // `could not be resolved` residual site (struct-initializer, field-
    // accessor, variable, call-subject, match); the std-missing wall printed
    // five of them for one cause before batch 7 demoted them (standard B5).
    // The root must stand alone: no residual echoes it at any of the five.
    let diagnostics = failure_diagnostics(
        r#"
        struct Box { v: i32 }
        fun take(x: i32): i32 { x }
        fun main() {
            let root = zzz_missing(1);
            let via_var = root;
            let via_field = root.field;
            let via_call = take(root);
            let via_struct = Box { v = root };
            let via_match = match root {
                _ => 1,
            };
        }
        "#,
    );
    // Exactly one root error, once — not once per downstream use.
    assert_eq!(
        diagnostics
            .iter()
            .filter(|(message, _)| message.contains("cannot find 'zzz_missing'"))
            .count(),
        1,
        "the root error must stand exactly once: {diagnostics:#?}"
    );
    // None of the five downstream positions emits a residual.
    assert!(
        diagnostics
            .iter()
            .all(|(message, _)| !message.contains("could not be resolved")),
        "one unresolved name must not fan into `could not be resolved` residuals: {diagnostics:#?}"
    );
    // And no echo storm: the root plus at most the one call-subject
    // consequence (`root` is called, so `zzz_missing(1)` also reports
    // `cannot call ... void`) — never a per-use-site wall.
    assert!(
        diagnostics.len() <= 2,
        "one unresolved name must not bury the user in echoes: {diagnostics:#?}"
    );
}

#[test]
fn an_unknown_struct_steers_to_its_import() {
    assert_fails_with(
        r#"
        fun main() {
            mut table = Map { };
        }
        "#,
        "unknown struct: Map; import it first (`import std::map::Map;`)",
    );
}

// --- Diagnostics audit, batch 5: generated-code diagnostics (standard A2) ----

#[test]
fn a_diagnostic_in_generated_code_anchors_at_the_attribute() {
    // The macro emits a function whose body mismatches its return type. The
    // error used to anchor in the generated text (invisible; the LSP showed
    // "(in generated code)" at 0..0); it now re-anchors at the ATTRIBUTE
    // that produced the code, provenance said in the message.
    let source = r#"
        macro fun Applied(item: Item): Source {
            source("fun oops(): i32 { \"text\" }")
        }

        [Applied]
        struct Point { x: i32 }

        fun main() {
            let p = Point { x = 1 };
        }
        "#;
    // The expected span is the ATTRIBUTE's name — the macro definition
    // contains the same text earlier, so locate it via the bracket form.
    let name_start = source.find("[Applied]").expect("attribute in source") + 1;
    let expected = name_start..name_start + "Applied".len();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics.iter().any(|(message, range)| {
            message.contains("in code generated by this attribute:")
                && message.contains("Expected i32, but got str instead.")
                && *range == expected
        }),
        "expected the generated-code error re-anchored at the attribute: {diagnostics:#?}"
    );
}

#[test]
fn e82_a_derive_refusal_anchors_at_the_attribute_not_the_generated_text() {
    // `[derive(PartialEq)]` on a struct whose field type provides no
    // `PartialEq` refuses inside the GENERATED `eq` — its field compare is an
    // `==` the post-fixpoint binary-operator pass checks. That pass pushed
    // without attributing, so the refusal kept the generated TEMPLATE's span
    // while claiming the entry file and drew its label over whatever the
    // entry held at those offsets (E82's live shape: a comment line). It
    // re-anchors at the attribute that generated the code, provenance said in
    // the message, exactly like every other generated-code diagnostic
    // (standard A2, `a_diagnostic_in_generated_code_anchors_at_the_attribute`).
    assert_fails_spanning(
        r#"
        import std::io::print;

        [derive(PartialEq)]
        struct Widget { item: Opaque }

        struct Opaque { x: i32 }

        fun main() {
            let w = Widget { item = Opaque { x = 1 } };
            print(w.item.x);
        }
        "#,
        "PartialEq",
        "in code generated by this attribute: type 'Opaque' does not implement the `PartialEq` operator",
    );
}

// --- Diagnostics audit, batch 3: method/call anchors (standard A1/A4) --------

#[test]
fn a_no_method_error_anchors_at_the_method_name() {
    // The NAME identifies the problem, not the argument list it happens to
    // be called with.
    assert_fails_spanning(
        r#"
        fun main() {
            let text = "x";
            text.launch(1, 2);
        }
        "#,
        "launch",
        "has no method 'launch'",
    );
}

#[test]
fn an_array_no_method_error_anchors_at_the_method_name() {
    assert_fails_spanning(
        r#"
        fun main() {
            mut a = [0; 4];
            a.push(1);
        }
        "#,
        "push",
        "has no method 'push'",
    );
}

#[test]
fn a_non_function_call_names_the_subjects_type() {
    // "cannot call a non-function value" said nothing about WHAT the value
    // was; it now renders the type and anchors at the subject.
    assert_fails_spanning(
        r#"
        fun main() {
            let x = (42)(1);
        }
        "#,
        "42",
        "cannot call this as a function: it is i32",
    );
}

// --- Diagnostics audit, batch 2: mismatch origins (standard B3) --------------

#[test]
fn a_reassignment_mismatch_notes_the_inferring_initializer() {
    // `mut n = 1` fixed n's type invisibly; the later conflicting write
    // names the origin as a note at the initializer (B3/C3).
    assert_fails_noting(
        r#"
        fun main() {
            mut n = 1;
            n = "two";
        }
        "#,
        "Expected i32, but got str instead.",
        "1",
        "the variable's type was inferred from this initializer (i32)",
    );
}

#[test]
fn an_annotated_variables_mismatch_stays_noteless() {
    // With an annotation the origin is visible — no note (the message
    // stands alone, exactly as before).
    let diagnostics = failure_diagnostics_with_notes(
        r#"
        fun main() {
            mut n: i32 = 1;
            n = "two";
        }
        "#,
    );
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _, _)| message.contains("Expected i32, but got str"))
        .collect();
    assert!(
        !matching.is_empty(),
        "expected the mismatch: {diagnostics:#?}"
    );
    assert!(
        matching.iter().all(|(_, _, note)| note.is_none()),
        "an annotated variable's mismatch must not carry an inference note: {matching:#?}"
    );
}

// --- Diagnostics audit, batch 1: name resolution steers (standard B4) --------
//
// "cannot find X" now steers to the import when X uniquely names a known
// module's export — the common miss after the derive-leak fix made
// `JsonValue` require its import. Ambiguous or unknown names stay silent
// (a wrong steer is worse than none).

#[test]
fn an_unknown_type_steers_to_its_std_import() {
    assert_fails_with(
        r#"
        fun main() {
            let v: JsonValue = 1;
        }
        "#,
        "cannot find type 'JsonValue'; import it first (`import std::json::JsonValue;`)",
    );
}

#[test]
fn an_unknown_value_steers_to_its_std_import() {
    assert_fails_with(
        r#"
        fun main() {
            let text = format(42);
        }
        "#,
        "import std::display::format;",
    );
}

#[test]
fn an_unknown_trait_steers_to_its_std_import() {
    assert_fails_with(
        r#"
        struct Point { x: i32 }
        impl Point with PartialOrd {
            fun partial_compare(self, b: Point): Option<Ordering> {
                None
            }
        }
        fun main() {}
        "#,
        "cannot find trait 'PartialOrd'; import it first (`import std::compare::PartialOrd;`)",
    );
}

// E103: the steer was not the value resolver's private property. A pattern's
// variant path and a `use` statement's root each grew their OWN scope lookup
// and raised the identical `cannot find 'X' in this scope` sentence without
// ever joining the steer — so the same missing `Some` came with the one-line
// fix in value position and bare in pattern position. That, not the `List`
// method the census happened to reach it through, is the class.

#[test]
fn a_variant_pattern_steers_to_its_std_import() {
    // The reported shape: a variant name appearing ONLY in the pattern —
    // nothing in the program takes the value-position path that used to be
    // the steer's only door. (The original exhibit used `Some`, which the
    // prelude has since made ambient — `Ordering::Less` keeps the shape with
    // a name still outside the prelude.)
    assert_fails_with(
        r#"
        fun main() {
            let picked = match 1 {
                Ordering::Less => 1,
                _ => 0,
            };
        }
        "#,
        "cannot find 'Ordering::Less' in this scope; import it first (`import std::compare::Ordering;`)",
    );
}

#[test]
fn the_value_position_some_still_steers() {
    // The green control on the sibling path: a bare VALUE-position name
    // steered before E103 and must go on steering after it — the fix joins a
    // second door to the steer, it does not move the first one. (`Some` left
    // this pin when the prelude made it ambient; `format` keeps the claim ON
    // THE VALUE DOOR — audit run 6 caught the first retarget landing on the
    // type door, which the sibling type pins already cover.)
    assert_fails_with(
        r#"
        fun main() {
            let v = format("x");
        }
        "#,
        "cannot find 'format' in this scope; import it first",
    );
}

#[test]
fn a_use_statement_root_steers_to_its_std_import() {
    // The class's other member: `use`'s root miss raised the same sentence
    // from its own lookup, equally unsteered.
    assert_fails_with(
        r#"
        fun main() {
            use Ordering::{ Less };
        }
        "#,
        "cannot find 'Ordering' in this scope; import it first (`import std::compare::Ordering;`)",
    );
}

#[test]
fn a_variant_pattern_whose_head_resolves_gets_no_steer() {
    // The steer names the head an import could supply. Here the head IS in
    // scope and a later segment is the miss, so no import fixes anything —
    // the message must stay plain rather than point at a name already there.
    let diagnostics = failure_diagnostics(
        r#"
        enum Signal { Quit, Go }
        fun main() {
            let s = Signal::Quit;
            let n = match s {
                Signal::Nope => 1,
                _ => 0,
            };
        }
        "#,
    );
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _)| message.contains("cannot find 'Signal::Nope'"))
        .collect();
    assert!(
        !matching.is_empty(),
        "expected the unresolved-variant error: {diagnostics:#?}"
    );
    assert!(
        matching
            .iter()
            .all(|(message, _)| !message.contains("import it first")),
        "a resolvable head must not be steered at: {matching:#?}"
    );
}

#[test]
fn an_unknown_name_gets_no_bogus_steer() {
    // No module exports `zzz_missing`; the message stays plain.
    let diagnostics = failure_diagnostics(
        r#"
        fun main() {
            let x = zzz_missing;
        }
        "#,
    );
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _)| message.contains("cannot find 'zzz_missing'"))
        .collect();
    assert!(
        !matching.is_empty(),
        "expected the plain error: {diagnostics:#?}"
    );
    assert!(
        matching
            .iter()
            .all(|(message, _)| !message.contains("import it first")),
        "an unknown name must not get a steer: {matching:#?}"
    );
}

// --- The derive-import leak: expansion imports are scoped (FIXED) ------------
//
// A derive expansion self-carries its imports; they used to register into
// the DERIVING module's scope, so `JsonValue` resolved after `[derive(Json)]`
// with no import — and user code could silently depend on an invisible name.
// Generated items now walk under a child scope (imports bind there only)
// with the expansion's DEFINITIONS hoisted to the module by node-level name.

#[test]
fn a_derives_imports_no_longer_leak() {
    assert_fails_with(
        r#"
        [derive(Json)]
        struct Point { x: i32 }
        fun main() {
            let v: JsonValue = Point { x = 1 }.to_json();
        }
        "#,
        "cannot find type 'JsonValue'",
    );
}

#[test]
fn a_derived_impl_stays_module_visible_and_explicit_imports_coexist() {
    // The hoist keeps generated definitions usable from module code, and an
    // explicit import of the same name a derive uses internally is fine.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::json::JsonValue;
        [derive(PartialEq, Json)]
        struct Point { x: i32 }
        fun typed(value: JsonValue): JsonValue { value }
        fun main() {
            let a = Point { x = 1 };
            let b = Point { x = 1 };
            print(a == b);                          // true — the derived impl
            print(Point { x = 2 }.to_json().len() > 0);   // true — Json derive
        }
        "#,
        "true\ntrue\n",
    );
}

// --- B13 residual: a later conflicting call names the inferring one (FIXED) --

#[test]
fn a_conflicting_later_call_names_the_first_call_inference() {
    // The first call fills an unannotated closure parameter's type; a later
    // conflicting call used to read as a bare mismatch with no hint of WHERE
    // i32 came from. It now names the origin and the fix.
    // (`|x| print(x)` would not reproduce: `print`'s `any` parameter makes
    // `x` adopt `any` through the argument-adoption channel before any call
    // — the identity body keeps the parameter open until the first call.)
    // The origin rides as a NOTE anchored at the FIRST call's argument
    // (diagnostics-standard.md B3/C3); the message keeps the annotate steer.
    assert_fails_noting(
        r#"
        fun main() {
            let pass = |x| x;
            let a = pass(1);
            let b = pass("two");
        }
        "#,
        "The parameter is unannotated; annotate it",
        "1",
        "inferred from this, the closure's first call",
    );
}

#[test]
fn consistent_later_calls_stay_clean() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let show = |x| print(x);
            show(1);
            show(2);
        }
        "#,
        "1\n2\n",
    );
}

// --- B16 remainder: an unannotated Map::new() checked vacuously (FIXED) ------
//
// `mut table = Map::new(); table.insert("k", 1); table.insert(2, "v")`
// COMPILED AND RAN, and a read came back under any annotation: Map is not a
// slot container, so K/V never grounded and every argument check reconciled
// against raw generics. The post-solve sweep now rejects any binding whose
// final type keeps a generic declared in ANOTHER file (`Map::new`'s `K` can
// never ground in user code) — general over containers, not Map-cased. A
// generic declared in the SAME file stays legal (a generic function's own
// body); the same-file leak shape is the recorded miss.

#[test]
fn an_unannotated_map_new_requires_an_annotation() {
    assert_fails_with(
        r#"
        import std::map::Map;
        fun main() {
            mut table = Map::new();
            table.insert("k", 1);
        }
        "#,
        "never fully determined",
    );
}

#[test]
fn an_unannotated_set_new_requires_an_annotation() {
    assert_fails_with(
        r#"
        import std::set::Set;
        fun main() {
            mut seen = Set::new();
            seen.insert(7);
        }
        "#,
        "never fully determined",
    );
}

#[test]
fn an_annotated_map_checks_its_inserts() {
    // With the annotation the parameters ground, so a mistyped insert is a
    // real error (the B16 substitution-applied argument check).
    assert_fails(
        r#"
        import std::map::Map;
        fun main() {
            mut table: Map<str, i32> = Map::new();
            table.insert(2, "v");
        }
        "#,
    );
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::map::Map;
        fun main() {
            mut table: Map<str, i32> = Map::new();
            table.insert("k", 1);
            print(table.get("k").unwrap_or(-1));
        }
        "#,
        "1\n",
    );
}

#[test]
fn a_generic_functions_own_bindings_stay_legal() {
    // The legitimacy rule: a residual generic declared in the SAME file (the
    // enclosing generic function's own parameter) is not a leak.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun pick<T>(a: T): T {
            let x = a;
            x
        }
        fun main() {
            print(pick(41) + 1);
        }
        "#,
        "42\n",
    );
}

// --- B28: conditions are not type-checked (FIXED) ----------------------------
//
// Found building expression lifting: NOTHING checked an `if`/`for` condition
// against `bool`, so `if 5 { .. }` compiled and branched on JS truthiness —
// and any non-empty aggregate (an Option is a tagged array) always took the
// branch. Conditions now check post-solve like the `&&`/`||` operands (B24):
// a grounded non-`bool` rejects; `Never`/`any` pass by their own rules;
// match guards already had their own equivalent check.

#[test]
fn an_integer_if_condition_is_rejected() {
    assert_fails_with(
        r#"
        fun main() {
            if 5 {
                let _x = 1;
            }
        }
        "#,
        "this `if` condition is `i32`, but a condition must be `bool`",
    );
}

#[test]
fn a_string_if_condition_is_rejected() {
    assert_fails_with(
        r#"
        fun main() {
            let name = "ada";
            if name {
                let _x = 1;
            }
        }
        "#,
        "this `if` condition is `str`, but a condition must be `bool`",
    );
}

#[test]
fn an_option_if_condition_is_rejected() {
    // The truthiness trap the check exists for: an Option is a tagged array
    // at runtime — always truthy, so this silently took the branch.
    assert_fails_with(
        r#"
        import std::option::Option::{ self, Some, None };
        fun main() {
            let maybe = Some(1);
            if maybe {
                let _x = 1;
            }
        }
        "#,
        "but a condition must be `bool`",
    );
}

#[test]
fn a_non_bool_while_condition_is_rejected() {
    assert_fails_with(
        r#"
        fun main() {
            mut n = 3;
            for n {
                n = n - 1;
            }
        }
        "#,
        "this `for` condition is `i32`, but a condition must be `bool`",
    );
}

#[test]
fn bool_conditions_of_every_shape_still_compile_and_run() {
    // The whole legitimate surface: a bool binding, a comparison, an `is`
    // test, `&&`-composition, a bool-returning call — in `if` and `for`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };
        fun ready(n: i32): bool { n > 1 }
        fun main() {
            let flag = true;
            if flag { print("flag"); }
            let maybe = Some(2);
            if maybe is Some(let n) && n > 1 { print("is"); }
            if ready(2) { print("call"); }
            mut n = 2;
            for n > 0 { n = n - 1; }
            print(n);
        }
        "#,
        "flag\nis\ncall\n0\n",
    );
}

#[test]
fn an_any_condition_stays_lenient() {
    // `any` absorbs everywhere (the std::db parameter rule); a condition of
    // type `any` keeps that leniency — documented, pinned.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        fun main() {
            let flags: List<any> = [true];
            if flags[0] {
                print("lenient");
            }
        }
        "#,
        "lenient\n",
    );
}

// --- B24: primitive comparisons skip operand-type checking (FIXED) ----------
//
// Found writing the spec (§5.7): comparison operators between PRIMITIVES
// bypassed the PartialEq/PartialOrd model, so ill-typed mixes compiled and
// emitted raw JS comparisons (with JS coercion semantics). The rule now
// checked on the native fast path: the right operand types as `B = Self`
// with no implicit conversions (§5.8), `bool` has no ordering, and `&&`/`||`
// take `bool`. The right side is inferred WITH the left's type as its
// expectation, so an unsuffixed literal adapts exactly as it does in a
// `let` — `1i53 < 3` is `i53 < i53` — while genuinely typed operands must
// match.

#[test]
fn a_bool_compared_to_an_integer_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = true < 3;
        }
        "#,
        "true < 3",
        "`bool` has no ordering",
    );
}

#[test]
fn an_integer_compared_to_a_string_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = 1 == "a";
        }
        "#,
        r#"1 == "a""#,
        "`==` compares two values of the same type",
    );
}

#[test]
fn mixed_width_typed_comparison_is_rejected() {
    // TYPED operands of different widths reject — no implicit conversions.
    assert_fails_spanning(
        r#"
        fun main() {
            let a: i53 = 1;
            let b: i32 = 3;
            let _x = a < b;
        }
        "#,
        "a < b",
        "`<` compares two values of the same type",
    );
}

#[test]
fn an_unsuffixed_literal_adapts_to_the_comparisons_peer() {
    // The literal rule (numeric-types.md §3): an unsuffixed integer takes
    // the expected type — the peer operand here — so this is `i53 < i53`.
    assert_compiles(
        r#"
        fun main() {
            let _x = 1i53 < 3;
        }
        "#,
    );
}

#[test]
fn equality_between_mismatched_natives_is_rejected_for_typed_operands() {
    assert_fails(
        r#"
        fun main() {
            let n: u32 = 5;
            let s = "five";
            let _x = n == s;
        }
        "#,
    );
}

#[test]
fn logical_operators_take_bool_operands() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = 1 && true;
        }
        "#,
        "1 && true",
        "`&&` takes `bool` operands; the left operand is `i32`",
    );
}

#[test]
fn ordering_dispatches_through_a_partial_ord_impl() {
    // B25, fixed: the ordering operators resolve `PartialOrd`'s comparison
    // methods — usually the trait DEFAULTS over the impl's `partial_compare`,
    // re-dispatched to the concrete receiver like any inherited method.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::{ now, Duration };

        fun main() {
            let started = now();
            let deadline = started + Duration::hours(2i53);
            if started < deadline {
                print("dispatches");
            }
        }
        "#,
        "dispatches\n",
    );
}

#[test]
fn all_four_orderings_dispatch_on_a_user_type() {
    // lt / le / gt / ge, each through the trait default over one
    // `partial_compare` — both truth values exercised.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::{ PartialEq, PartialOrd, Ordering };
        import std::option::Option::{ self, Some };

        struct Level { rank: i32 }

        impl Level with PartialEq {
            fun eq(self, b: Level): bool { self.rank == b.rank }
        }

        impl Level with PartialOrd {
            fun partial_compare(self, b: Level): Option<Ordering> {
                self.rank.partial_compare(b.rank)
            }
        }

        fun main() {
            let low = Level { rank = 1 };
            let high = Level { rank = 9 };
            if low < high { print("lt"); }
            if low <= low { print("le"); }
            if high > low { print("gt"); }
            if high >= high { print("ge"); }
            if high < low { print("wrong-lt"); }
            if low > high { print("wrong-gt"); }
        }
        "#,
        "lt\nle\ngt\nge\n",
    );
}

#[test]
fn a_declared_lt_override_wins_over_the_default() {
    // An impl may declare the operator method itself (the `binary_op_dispatch`
    // path) — reversed ordering proves the OVERRIDE ran, not the default.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::{ PartialEq, PartialOrd, Ordering };
        import std::option::Option::{ self, Some };

        struct Upside { value: i32 }

        impl Upside with PartialEq {
            fun eq(self, b: Upside): bool { self.value == b.value }
        }

        impl Upside with PartialOrd {
            fun partial_compare(self, b: Upside): Option<Ordering> {
                self.value.partial_compare(b.value)
            }

            fun lt(self, b: Upside): bool {
                self.value > b.value
            }
        }

        fun main() {
            let small = Upside { value = 1 };
            let big = Upside { value = 9 };
            if big < small { print("override"); }
            if small < big { print("default"); }
        }
        "#,
        "override\n",
    );
}

#[test]
fn a_partial_ord_bound_dispatches_orderings_generically() {
    // `T: PartialOrd` — the `OnConstraint` path, re-resolved per
    // monomorphization; exercised with std's `Duration` impl.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialOrd;
        import std::time::Duration;

        fun smallest<T: PartialOrd>(a: T, b: T): T {
            if a < b { a } else { b }
        }

        fun main() {
            let short = Duration::seconds(5i53);
            let long = Duration::minutes(2i53);
            print(smallest(long, short).describe());
            print(smallest(3, 11));
        }
        "#,
        "5s\n3\n",
    );
}

#[test]
fn ordering_a_struct_is_rejected_not_js_compared() {
    // No `PartialOrd` dispatch for user types yet — a silent raw-JS `<`
    // (object coercion) would be a miscompile, so it errors instead.
    assert_fails_spanning(
        r#"
        struct Point { x: i32 }

        fun main() {
            let a = Point { x = 1 };
            let b = Point { x = 2 };
            let _x = a < b;
        }
        "#,
        "a < b",
        "does not implement the `PartialOrd` operator; add `impl Point with PartialOrd` providing `partial_compare`",
    );
}

#[test]
fn same_type_native_comparisons_still_compile_and_run() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let a: u32 = 5;
            let b: u32 = 9;
            if a < b && "a" < "b" && "x" == "x" && 1.5 < 2.5 && true == false || 3 <= 3 {
                print("ok");
            }
        }
        "#,
        "ok\n",
    );
}

// --- B148: `+` skipped operand checking on the native path (FIXED) -----------
//
// B24 closed the hole for the COMPARISONS and left `+` open, so a native left
// operand still typed nothing on its right: `"here " + point` compiled, took
// its type from the left (`str`), and emitted `"here " + point` — which the
// host renders as the struct's runtime tuple, `here 1,2`. Two desugarings
// build the same expression, so the garbage turned up far from any `+` a
// reader wrote: an i-string is `("" + part + part + …)`, and a css block's
// mixed value is built to that shape (`a_mixed_css_value_refuses_a_struct_hole`
// in `styling.rs` holds that end).
//
// `+` on a native left operand now admits exactly `str + x` where `x` renders
// (the numeric primitives, `bool`, `str` — the set an i-string hole rests on)
// and `T + T` for a numeric primitive `T`. The other native operators still
// skip the check; theirs is a breaking numeric-strictness change with its own
// migration, not this fix.

#[test]
fn a_struct_concatenated_into_a_string_is_rejected() {
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
            let p = Point { x = 1, y = 2 };
            let _text = "here " + p;
        }
        "#,
        r#""here " + p"#,
        "`+` on `str` concatenates, and `Point` has no string form",
    );
}

#[test]
fn an_i_string_hole_holding_a_struct_is_rejected() {
    // The control the whole class hangs from: an i-string is not a second
    // mechanism, it lexes to this very concatenation — so it was the same bug
    // and it takes the same fix. The span is the left-associated prefix that
    // reaches the hole (`("" + "here " + p)`), which is where the `+` is.
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
            let p = Point { x = 1, y = 2 };
            let _text = i"here {p} there";
        }
        "#,
        r#"i"here {p}"#,
        "`+` on `str` concatenates, and `Point` has no string form",
    );
}

#[test]
fn an_enum_concatenated_into_a_string_is_rejected() {
    assert_fails_spanning(
        r#"
        enum Colour { Red, Green }

        fun main() {
            let c = Colour::Red;
            let _text = "colour " + c;
        }
        "#,
        r#""colour " + c"#,
        "`+` on `str` concatenates, and `Colour` has no string form",
    );
}

#[test]
fn a_list_concatenated_into_a_string_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            let items = [ 1, 2, 3 ];
            let _text = "items " + items;
        }
        "#,
        r#""items " + items"#,
        "`+` on `str` concatenates, and `List<i32>` has no string form",
    );
}

#[test]
fn an_option_concatenated_into_a_string_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::option::Some;

        fun main() {
            let held = Some(5);
            let _text = "held " + held;
        }
        "#,
        r#""held " + held"#,
        "`+` on `str` concatenates, and `Option<i32>` has no string form",
    );
}

#[test]
fn a_tuple_concatenated_into_a_string_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            let pair = (1, 2);
            let _text = "pair " + pair;
        }
        "#,
        r#""pair " + pair"#,
        "`+` on `str` concatenates, and `(i32, i32)` has no string form",
    );
}

#[test]
fn a_backed_enum_concatenated_into_a_string_is_rejected() {
    // A backing is a LOWERING, not a rendering: `Size::Small` lowers to "sm"
    // and would have concatenated as one, which reads as a display form the
    // program chose when nothing chose it. `.to_string()` is still the answer.
    assert_fails_spanning(
        r#"
        enum Size {
            Small = "sm",
            Large = "lg",
        }

        fun main() {
            let s = Size::Small;
            let _text = "size " + s;
        }
        "#,
        r#""size " + s"#,
        "`+` on `str` concatenates, and `Size` has no string form",
    );
}

#[test]
fn the_to_string_steer_the_refusal_names_compiles_and_renders() {
    // The refusal's whole worth is that the fix it names works, at both
    // spellings of the concatenation.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun main() {
            let p = Point { x = 1, y = 2 };
            print("here " + p.to_string());
            print(i"here {p.to_string()} there");
        }
        "#,
        "here (1, 2)\nhere (1, 2) there\n",
    );
}

#[test]
fn a_string_concatenation_still_admits_the_renderable_primitives() {
    // The set an i-string hole rests on, and so the set `str + x` must keep:
    // std's own `impl i32 with Display` is `i"{self}"`, which is `"" + self`.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        fun main() {
            let count = 3;
            let ratio = 1.5;
            let flag = true;
            let name = "vilan";
            print("n=" + count + " r=" + ratio + " f=" + flag + " s=" + name);
            print(i"n={count} r={ratio} f={flag} s={name}");
            print(count.to_string() + "!");
        }
        "#,
        "n=3 r=1.5 f=true s=vilan\nn=3 r=1.5 f=true s=vilan\n3!\n",
    );
}

#[test]
fn the_numeric_additions_still_compile_and_run() {
    // `T + T`, and an unsuffixed literal still adapts to its peer.
    assert_compiles_and_runs(
        r#"

        fun main() {
            let counted: u32 = 5;
            let stamp: i53 = 1;
            print(1 + 2);
            print(1.5 + 2.5);
            print(counted + 4);
            print(stamp + 1000);
            print(7n + 1n);
            print("a" + "b");
        }
        "#,
        "3\n4\n9\n1001\n8n\nab\n",
    );
}

#[test]
fn an_integer_added_to_a_string_is_rejected() {
    // The mirror shape. It used to emit "1a" while typing as `i32`, and the
    // numeric steer would misread the author: only a `str` LEFT operand
    // concatenates, because the expression takes its type from the left.
    assert_fails_spanning(
        r#"
        fun main() {
            let n = 1;
            let _text = n + "a";
        }
        "#,
        r#"n + "a""#,
        "only a `str` LEFT operand concatenates",
    );
}

#[test]
fn a_mixed_width_addition_is_rejected() {
    // What `<` has refused since B24, now refused by `+` too (§5.8).
    assert_fails_spanning(
        r#"
        fun main() {
            let ratio: f64 = 1.5;
            let count: i32 = 3;
            let _sum = ratio + count;
        }
        "#,
        "ratio + count",
        "`+` adds two values of the same type, but the operands are `f64` and `i32`",
    );
}

#[test]
fn adding_bools_is_rejected() {
    // `bool` is native for `==`/`<` and has no `Add`: the host would have
    // added the lowering, making `true + true` a `bool` holding 2.
    assert_fails_spanning(
        r#"
        fun main() {
            let _sum = true + true;
        }
        "#,
        "true + true",
        "`bool` is neither: it has no `Add`",
    );
}

#[test]
fn adding_backed_enum_variants_is_rejected() {
    assert_fails_spanning(
        r#"
        enum Level {
            Low = 1,
            High = 2,
        }

        fun main() {
            let _sum = Level::Low + Level::High;
        }
        "#,
        "Level::Low + Level::High",
        "`Level` is neither: it has no `Add`",
    );
}

#[test]
fn a_user_add_impl_still_dispatches_with_its_own_right_operand() {
    // The non-native path is untouched: the impl's `B` types the right
    // operand, and it need not be `Self`.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        struct Metres { value: i32 }

        impl Metres with Add<i32> {
            fun add(self, b: i32): Metres {
                Metres { value = self.value + b }
            }
        }

        fun main() {
            let far = Metres { value = 5 } + 4;
            print(far.value);
        }
        "#,
        "9\n",
    );
}

#[test]
fn a_compound_assignment_is_checked_like_the_addition_it_desugars_to() {
    // `text += p` is `text = text + p`, and the desugar SYNTHESIZES its own
    // binary from a second registration site — so the rule has to reach it
    // there too, or the one spelling most likely to be written in a loop
    // would have kept emitting the tuple.
    assert_fails_with(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
            let p = Point { x = 1, y = 2 };
            mut text = "here ";
            text += p;
        }
        "#,
        "`+` on `str` concatenates, and `Point` has no string form",
    );
}

#[test]
fn a_closure_or_function_reference_concatenated_into_a_string_is_rejected() {
    // Not only the nominal types: the rule is "has a string form", so the
    // structural ones are refused by the same predicate rather than by a list.
    assert_fails_with(
        r#"
        fun main() {
            let f = |n: i32| n + 1;
            let _text = "f=" + f;
        }
        "#,
        "`+` on `str` concatenates, and `|i32| i32` has no string form",
    );
}

#[test]
fn an_unbounded_generic_concatenated_into_a_string_is_rejected() {
    // B169 (was b148's recorded residual). The rule only rejected a GROUNDED
    // right operand, the same leniency B24 gave the comparisons, so a bare
    // `T` passed and every instantiation of `show` printed the runtime shape
    // — `show(Point { … })` emitted `"v=" + value` and printed `v=1,2`.
    //
    // The declaration is checked once for all instantiations (§5.7's note on
    // generic parameters), so the fix is the refusal HERE that pin asked for,
    // not a per-monomorphization check: an unbounded parameter promises
    // nothing, and nothing is not a string form.
    assert_fails_with(
        r#"
        fun show<T>(value: T): str {
            "v=" + value
        }

        fun main() {
            let _text = show(5);
        }
        "#,
        "has no string form",
    );
}

#[test]
fn an_unbounded_generic_added_to_a_number_is_rejected() {
    // The other half of the admitted set: `T + T` needs the operands to BE
    // the same type, and an unbounded parameter is not known to be `i32`.
    // `total + value` emitted the host's `+` and `add(Point { … })` returned
    // the string `"1,2"` typed as `i32`.
    assert_fails_spanning(
        r#"
        fun add<T>(value: T, total: i32): i32 {
            total + value
        }

        fun main() {
            let _n = add(5, 1);
        }
        "#,
        "total + value",
        "the operands are `i32` and `T`",
    );
}

#[test]
fn an_unbounded_generic_in_an_i_string_hole_is_rejected() {
    // The hole is this same concatenation, so it is refused at the same
    // place — and this is the spelling that actually turns up in a generic
    // `Display`-ish helper.
    assert_fails_with(
        r#"
        fun show<T>(value: T): str {
            i"v={value}"
        }

        fun main() {
            let _text = show(5);
        }
        "#,
        "has no string form",
    );
}

#[test]
fn a_generic_right_operand_bounded_to_its_peer_still_adds() {
    // The refusal must not reach a parameter that IS known to be the left
    // operand's type: a bound the operator dispatches through carries the
    // promise, and the `T + T` form keeps working.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        fun total<T: Add>(a: T, b: T): T {
            a + b
        }

        fun main() {
            print(total(1, 2));
            print(total("a", "b"));
        }
        "#,
        "3\nab\n",
    );
}

#[test]
fn the_to_string_steer_for_a_generic_operand_compiles_and_renders() {
    // The refusal names `Display` + `.to_string()`; that has to be a working
    // spelling, or the rule would leave a generic helper with no way to
    // render its own parameter.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun show<T: Display>(value: T): str {
            "v=" + value.to_string()
        }

        fun main() {
            print(show(Point { x = 1, y = 2 }));
            print(show(5));
        }
        "#,
        "v=(1, 2)\nv=5\n",
    );
}

// --- B176: an ADMITTED bounded generic operand emitted a raw `+` -------------
//
// B169 refused the UNBOUNDED parameter and left the bounded one admitted,
// which is right: `T: Display` promises exactly the string form `str + x`
// wants. But nothing ever KEPT the promise. `grounded` excludes every
// `Type::Generic`, so a bounded parameter reached neither the admitted set nor
// the refusal, fell through to the native emission, and `"v=" + value` came
// out as the host's `+` over the value's runtime shape — `show(Point { x = 1,
// y = 2 })` printed `v=1,2` with the `Display` impl never called. The typing
// was right and the codegen was wrong, which is the worse half: the program
// compiles, runs, and lies.
//
// The concatenation now asks the bound whether it provides `to_string` and
// records the site, so codegen routes the operand through the impl at each
// monomorphization — the channel `value.to_string()` already used. A bound
// that promises something else (`T: Add`) promises no string form and is
// refused with the rest.

#[test]
fn a_bounded_generic_concatenated_into_a_string_calls_its_display_impl() {
    // The find itself. `T: Display` is the admission's own promise, so the
    // emission has to call the impl rather than hand the operand to the host.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun show<T: Display>(value: T): str {
            "v=" + value
        }

        fun main() {
            print(show(Point { x = 1, y = 2 }));
        }
        "#,
        "v=(1, 2)\n",
    );
}

#[test]
fn a_bounded_generic_in_an_i_string_hole_calls_its_display_impl() {
    // The same expression by another spelling: `lexing::emit_interpolated`
    // desugars `i"v={value}"` to `("" + "v=" + value)`, so the hole rode the
    // identical hole and printed the identical `v=1,2`. One fix covers both,
    // and each keeps its own pin.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun show<T: Display>(value: T): str {
            i"v={value}"
        }

        fun main() {
            print(show(Point { x = 1, y = 2 }));
        }
        "#,
        "v=(1, 2)\n",
    );
}

#[test]
fn a_bounded_generic_operand_renders_in_a_non_tail_position() {
    // The operand mid-expression: `"a" + value + "b"` parses as `("a" +
    // value) + "b"`, so the render has to attach to the INNER binary's right
    // operand and leave the outer concatenation alone. A fix keyed on "the
    // last operand" would have rendered nothing here.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun wrap<T: Display>(value: T): str {
            "a" + value + "b"
        }

        fun two<T: Display, U: Display>(first: T, second: U): str {
            "[" + first + "|" + second + "]"
        }

        fun main() {
            print(wrap(Point { x = 1, y = 2 }));
            print(two(Point { x = 3, y = 4 }, 7));
        }
        "#,
        "a(1, 2)b\n[(3, 4)|7]\n",
    );
}

#[test]
fn a_bounded_generic_operand_renders_through_a_nested_call_chain() {
    // `show` calling `show`: the outer monomorphization binds `T = Point` and
    // the inner one is reached THROUGH it, so the dispatch has to resolve
    // under the active substitution rather than at the declaration.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun show<T: Display>(value: T): str {
            "v=" + value
        }

        fun twice<T: Display>(value: T): str {
            show(value) + " " + show(value)
        }

        fun main() {
            print(twice(Point { x = 1, y = 2 }));
            print(twice(5));
        }
        "#,
        "v=(1, 2) v=(1, 2)\nv=5 v=5\n",
    );
}

#[test]
fn a_concrete_operand_keeps_its_own_routing_beside_the_generic_one() {
    // The control, and the routing the fix reuses: a CONCRETE struct operand
    // is still refused outright (B148) and its `.to_string()` spelling still
    // renders, while the primitives still concatenate natively — the fix must
    // move neither. `show(5)` proves the native instantiation of the same
    // generic keeps the host's own rendering through the `impl i32 with
    // Display`, which is itself `i"{self}"`.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun show<T: Display>(value: T): str {
            "v=" + value
        }

        fun main() {
            let p = Point { x = 1, y = 2 };
            print("c=" + p.to_string());
            print("n=" + 3 + "/" + 1.5 + "/" + true);
            print(show(5));
            print(show("s"));
        }
        "#,
        "c=(1, 2)\nn=3/1.5/true\nv=5\nv=s\n",
    );
}

#[test]
fn a_concrete_struct_operand_is_still_refused_beside_the_bounded_generic() {
    // The other half of the control: admitting the bounded parameter must not
    // admit the concrete struct it instantiates to. The refusal B148 wrote
    // stands, and it is what the reader sees for the spelling that has no
    // bound to consult.
    assert_fails_with(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun main() {
            let p = Point { x = 1, y = 2 };
            let _text = "v=" + p;
        }
        "#,
        "`+` on `str` concatenates, and `Point` has no string form",
    );
}

#[test]
fn a_generic_bounded_to_something_other_than_display_is_rejected() {
    // The admission is the BOUND's promise, so a bound that promises
    // something else promises no string form. `T: Add` compiled and
    // `label(Point { … })` printed `v=1,2` for the same reason the unbounded
    // case did — the parameter was never asked what it could render.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        fun label<T: Add>(value: T): str {
            "v=" + value
        }

        fun main() {
            let _text = label(1);
        }
        "#,
        r#""v=" + value"#,
        "no bound on `T` provides `to_string`",
    );
}

#[test]
fn a_generic_bounded_to_something_other_than_display_is_rejected_in_a_hole() {
    // The hole is the same concatenation, so the same bound is required of
    // it — the pair the refusal's own wording promises.
    assert_fails_with(
        r#"
        import std::operators::Add;

        fun label<T: Add>(value: T): str {
            i"v={value}"
        }

        fun main() {
            let _text = label(1);
        }
        "#,
        "no bound on `T` provides `to_string`",
    );
}

#[test]
fn a_compound_append_of_a_bounded_generic_renders() {
    // `text += value` is `text = text + value`, and the desugar SYNTHESIZES
    // its own binary from a second registration site — the spelling B148 had
    // to reach separately, so the routing has to reach it separately too. It
    // is also the one most likely to be written in a loop, where a wrong
    // rendering repeats.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun lines<T: Display>(first: T, second: T): str {
            mut text = "";
            text += first;
            text += "/";
            text += second;
            text
        }

        fun main() {
            print(lines(Point { x = 1, y = 2 }, Point { x = 3, y = 4 }));
        }
        "#,
        "(1, 2)/(3, 4)\n",
    );
}

#[test]
fn a_display_bound_reached_through_a_supertrait_still_renders() {
    // The bound need not name `Display` itself: `to_string` reached through a
    // supertrait is the same promise, and the lookup walks the chain the way
    // every other bound-member lookup does.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        trait Labelled with Display {
            fun label(self): str;
        }

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        impl Point with Labelled {
            fun label(self): str {
                "point"
            }
        }

        fun show<T: Labelled>(value: T): str {
            value.label() + "=" + value
        }

        fun main() {
            print(show(Point { x = 1, y = 2 }));
        }
        "#,
        "point=(1, 2)\n",
    );
}

// --- B179: a BOUNDED generic right operand of a NATIVE operator -------------
//
// B169 closed the UNBOUNDED parameter on the right of `+`, B176 closed the
// bounded one where the left operand was `str`, and the square neither covered
// was a live miscompile: `fun bump<T: Add>(total: i32, value: T): i32
// { total + value }` compiled, and `bump(1, Point { x = 1, y = 2 })` printed
// `11,2` — a string, typed `i32`, from a declaration carrying a bound.
//
// RULED (2026-09-01): refuse. The `+` belongs to the LEFT operand, so the right
// one must be a member of what the left's `add` accepts — and a bound can prove
// membership only where that set is trait-characterizable. `str`'s is, which is
// exactly why B176's render bound works. A number's is not: `i32`'s `add`
// accepts `i32`, no trait names that set, and a bound promises a trait's
// METHODS, never that the parameter IS `i32`. So EVERY generic right operand of
// a numeric-left `+` refuses, whatever its bound promises — and the same
// argument closes the rest of the native family, which had the identical hole
// with the identical garbage. The generic LEFT operand is the other half of the
// frame and stayed open on purpose, because trait defaults wrote
// `self.once() + self.once()` over the trait's own parameter; B174 took that
// breaking step once its migration was priced at one site, and the two halves
// now agree — a bound must PROVIDE the operator's method to admit either.

#[test]
fn a_bounded_generic_added_to_a_number_is_rejected() {
    // The pin B179 was filed as. The bound is `Add` — the most plausible one
    // an author would reach for, and the one the pre-ruling message actually
    // STEERED them to ("Bound it (`<T: Add>`)"), straight into this program.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Point { x: i32, y: i32 }

        impl Point with Add {
            fun add(self, other: Point): Point {
                Point { x = self.x + other.x, y = self.y + other.y }
            }
        }

        fun bump<T: Add>(total: i32, value: T): i32 {
            total + value
        }

        fun main() {
            print(bump(1, Point { x = 1, y = 2 }));
        }
        "#,
        "total + value",
        "the operands are `i32` and `T`",
    );
}

#[test]
fn a_display_bounded_generic_added_to_a_number_is_rejected() {
    // "Whatever its bound promises" is the load-bearing half of the ruling, so
    // it needs a bound that is NOT `Add` and that genuinely works one square
    // over: `T: Display` is admitted on the right of a `str +` (B176 routes it
    // through the impl) and is still refused here, because a string form is
    // not membership of `i32`'s set either.
    assert_fails_spanning(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun bump<T: Display>(total: i32, value: T): i32 {
            total + value
        }

        fun main() {
            print(bump(1, Point { x = 1, y = 2 }));
        }
        "#,
        "total + value",
        "wider than what `i32`'s `add` accepts",
    );
}

#[test]
fn a_compound_add_of_a_bounded_generic_into_a_number_is_rejected() {
    // `total += value` desugars to `total = total + value` from a SECOND
    // registration site, so the routing has to reach it separately — and it is
    // the spelling most likely to sit in a loop. It printed the same `11,2`.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Point { x: i32, y: i32 }

        impl Point with Add {
            fun add(self, other: Point): Point {
                Point { x = self.x + other.x, y = self.y + other.y }
            }
        }

        fun bump<T: Add>(start: i32, value: T): i32 {
            mut total = start;
            total += value;
            total
        }

        fun main() {
            print(bump(1, Point { x = 1, y = 2 }));
        }
        "#,
        "total += value",
        "wider than what `i32`'s `add` accepts",
    );
}

#[test]
fn a_generic_arithmetic_operand_of_a_number_is_rejected() {
    // The arithmetic siblings. b148's SCOPE note deferred `f64 * i32` — two
    // GROUNDED numerics, which compute a correct answer — and that stays
    // deferred; a parameter computes no answer at all. `total - value` and
    // `total * value` both emitted `NaN`, typed `i32`.
    for operator in ["-", "*", "/", "%"] {
        assert_fails_with(
            &format!(
                r#"
                struct Point {{ x: i32, y: i32 }}

                fun bump<T>(total: i32, value: T): i32 {{
                    total {operator} value
                }}

                fun main() {{
                    print(bump(1, Point {{ x = 1, y = 2 }}));
                }}
                "#
            ),
            "`T` is wider than what `i32` admits",
        );
    }
}

#[test]
fn a_generic_bitwise_operand_of_a_number_is_rejected() {
    // The bitwise/shift class, where the garbage is quietest of all: the host
    // coerces the operand to `0`, so `total & value` was `0` and `total <<
    // value` was `total` — plausible integers with no sign anything was wrong.
    for operator in ["&", "|", "^", "<<", ">>"] {
        assert_fails_with(
            &format!(
                r#"
                struct Point {{ x: i32, y: i32 }}

                fun bump<T>(total: i32, value: T): i32 {{
                    total {operator} value
                }}

                fun main() {{
                    print(bump(1, Point {{ x = 1, y = 2 }}));
                }}
                "#
            ),
            "`T` is wider than what `i32` admits",
        );
    }
}

#[test]
fn a_generic_compared_against_a_number_is_rejected() {
    // The ordering class. B24 checked these operands and its `grounded`
    // leniency let a parameter straight through, so `total < value` emitted
    // the host's `<` over a struct's tuple and returned a plausible `false`.
    for operator in ["<", ">", "<=", ">="] {
        assert_fails_with(
            &format!(
                r#"
                struct Point {{ x: i32, y: i32 }}

                fun ahead<T>(total: i32, value: T): bool {{
                    total {operator} value
                }}

                fun main() {{
                    print(ahead(1, Point {{ x = 1, y = 2 }}));
                }}
                "#
            ),
            "`T` is wider than what `i32` admits",
        );
    }
}

#[test]
fn a_generic_equated_with_a_number_is_rejected() {
    // The equality class, the same shape and the same plausible `false`.
    for operator in ["==", "!="] {
        assert_fails_with(
            &format!(
                r#"
                struct Point {{ x: i32, y: i32 }}

                fun same<T>(total: i32, value: T): bool {{
                    total {operator} value
                }}

                fun main() {{
                    print(same(1, Point {{ x = 1, y = 2 }}));
                }}
                "#
            ),
            "`T` is wider than what `i32` admits",
        );
    }
}

#[test]
fn a_generic_equated_with_a_str_is_rejected() {
    // `str` earns its exception for `+` ALONE: concatenation is the one native
    // operator whose admitted set a trait can name. `==` on a `str` still
    // wants a `str`, and nothing names that set, so the parameter is refused
    // here exactly as against a number — `"a" == value` was `false`.
    assert_fails_spanning(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun same<T: Display>(label: str, value: T): bool {
            label == value
        }

        fun main() {
            print(same("a", Point { x = 1, y = 2 }));
        }
        "#,
        "label == value",
        "`T` is wider than what `str` admits",
    );
}

#[test]
fn a_bounded_generic_concatenated_after_a_str_still_renders() {
    // The exception the whole ruling turns on, guarded from the other side:
    // closing the numeric square must not close B176's. `str`'s admitted set
    // IS trait-characterizable, the render bound proves membership, and the
    // operand still routes through the impl at each monomorphization.
    assert_compiles_and_runs(
        r#"
        import std::display::Display;

        struct Point { x: i32, y: i32 }

        impl Point with Display {
            fun to_string(self): str {
                i"({self.x}, {self.y})"
            }
        }

        fun show<T: Display>(value: T): str {
            "v=" + value
        }

        fun main() {
            print(show(Point { x = 1, y = 2 }));
            print(show(5));
        }
        "#,
        "v=(1, 2)\nv=5\n",
    );
}

#[test]
fn the_conversion_steer_for_a_generic_numeric_operand_compiles_and_runs() {
    // The refusal names two spellings and the first one has to work, or the
    // rule leaves a numeric helper with no legal way to take a foreign value:
    // convert where the type is KNOWN, and declare the operand `i32`.
    assert_compiles_and_runs(
        r#"
        struct Point { x: i32, y: i32 }

        impl Point {
            fun magnitude(self): i32 {
                self.x + self.y
            }
        }

        fun bump(total: i32, value: i32): i32 {
            total + value
        }

        fun main() {
            print(bump(1, Point { x = 1, y = 2 }.magnitude()));
        }
        "#,
        "4\n",
    );
}

// --- B180: the DISPATCH path never read the impl's declared `B` -------------
//
// B179 ruled the operand roles for a NATIVE left operand; a NOMINAL one is
// where an impl gets to SAY what its operator accepts, and nothing read it.
// `impl Counter with Add { fun add(self, other: Counter): Counter }` resolved
// for `Counter { n = 1 } + Point { x = 1, y = 2 }`, handed the `Point` to a
// body typed for a `Counter`, and printed `2` — the struct's slot 0, read as
// `other.n`. Every operator the dispatch serves had it, all measured before the
// fix: `-` gave `7` and `*` gave `30` off the same slot, `==` answered `true`,
// `<` answered `true` through `PartialOrd`'s inherited default, `/ % << >> & ^
// |` all computed, and `c += Point { .. }` rode the desugar's second
// registration site into the same body. `impl Meters with Add<Feet>` accepted a
// `Meters` (6, reading `other.f` off `m`), and `impl Bag<type T> with Add<T>`
// at `Bag<i32>` accepted a `str`.
//
// The `B` to check is the IMPL's, and it is three things: a type the impl wrote
// (`Add<Feet>`), the impl's own parameter substituted through what the subject
// bound (`Add<T>` at `Vec2<i32>` wants an `i32`), or `Self` — spelled, or
// arrived at through `Add<B = Self>`'s default. A generic right operand refuses
// for B179's reason one level along: a bound promises a trait's METHODS, never
// that the parameter IS the declared `B`. The comparison is the RIGID one
// conformance uses, because the ordinary one treats a parameter as a hole and
// answers `true` to whatever is asked.

#[test]
fn a_foreign_struct_on_the_right_of_a_dispatched_add_is_rejected() {
    // The pin B180 was filed as: GROUNDED both sides, no generics anywhere, and
    // it printed `2`.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Counter { n: i32 }
        struct Point { x: i32, y: i32 }

        impl Counter with Add {
            fun add(self, other: Counter): Counter {
                Counter { n = self.n + other.n }
            }
        }

        fun main() {
            let counter = Counter { n = 1 };
            let point = Point { x = 1, y = 2 };
            print((counter + point).n);
        }
        "#,
        "counter + point",
        "`Counter`'s `add` accepts `Counter`, but the right operand is `Point`",
    );
}

#[test]
fn a_self_spelled_operand_is_the_subject_like_the_b_equals_self_default() {
    // Two spellings of the same `B`. `Add<B = Self>`'s default interns `B` as
    // the trait type itself rather than as a fresh parameter, and a `Self`
    // written in the impl lands on that same type — so the position has to be
    // read as "the subject" in both, or the check would have no `B` at all for
    // the overwhelmingly common impl.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Counter { n: i32 }
        struct Point { x: i32, y: i32 }

        impl Counter with Add {
            fun add(self, other: Self): Self {
                Counter { n = self.n + other.n }
            }
        }

        fun main() {
            let counter = Counter { n = 1 };
            let point = Point { x = 7, y = 9 };
            print((counter + point).n);
        }
        "#,
        "counter + point",
        "`Counter`'s `add` accepts `Counter`, but the right operand is `Point`",
    );
}

#[test]
fn the_self_spelled_operand_still_dispatches_for_the_subject() {
    // The other half: the `Self` spelling must still ACCEPT a `Counter`, or the
    // arm above would be refusing the position rather than checking it.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        struct Counter { n: i32 }

        impl Counter with Add {
            fun add(self, other: Self): Self {
                Counter { n = self.n + other.n }
            }
        }

        fun main() {
            print((Counter { n = 1 } + Counter { n = 4 }).n);
        }
        "#,
        "5\n",
    );
}

#[test]
fn every_dispatched_operator_checks_its_right_operand() {
    // One check at the dispatch site, not one per trait — so the whole family
    // is the pin. Each of these computed off `Point`'s slot 0 before the fix.
    for (operator, trait_name, method) in [
        ("-", "Sub", "sub"),
        ("*", "Mul", "mul"),
        ("/", "Div", "div"),
        ("%", "Rem", "rem"),
        ("<<", "Shl", "shl"),
        (">>", "Shr", "shr"),
        ("&", "BitAnd", "bit_and"),
        ("^", "BitXor", "bit_xor"),
        ("|", "BitOr", "bit_or"),
    ] {
        assert_fails_spanning(
            &format!(
                r#"
        import std::operators::{trait_name};

        struct Counter {{ n: i32 }}
        struct Point {{ x: i32, y: i32 }}

        impl Counter with {trait_name} {{
            fun {method}(self, other: Counter): Counter {{
                Counter {{ n = self.n + other.n }}
            }}
        }}

        fun main() {{
            let counter = Counter {{ n = 12 }};
            let point = Point {{ x = 3, y = 4 }};
            print((counter {operator} point).n);
        }}
        "#
            ),
            &format!("counter {operator} point"),
            &format!("`Counter`'s `{method}` accepts `Counter`, but the right operand is `Point`"),
        );
    }
}

#[test]
fn a_dispatched_equality_checks_its_other_operand() {
    // `PartialEq`'s parameter is the same `B`, and `Counter { n = 1 } == Point
    // { x = 1, y = 2 }` answered a plausible `true` off slot 0. `!=` shares the
    // dispatch (the transformer negates `eq`), so it shares the refusal.
    for operator in ["==", "!="] {
        assert_fails_spanning(
            &format!(
                r#"
        import std::compare::PartialEq;

        struct Counter {{ n: i32 }}
        struct Point {{ x: i32, y: i32 }}

        impl Counter with PartialEq {{
            fun eq(self, other: Counter): bool {{
                self.n == other.n
            }}
        }}

        fun main() {{
            let counter = Counter {{ n = 1 }};
            let point = Point {{ x = 1, y = 2 }};
            print(counter {operator} point);
        }}
        "#
            ),
            &format!("counter {operator} point"),
            "`Counter`'s `eq` accepts `Counter`, but the right operand is `Point`",
        );
    }
}

#[test]
fn a_dispatched_ordering_checks_the_operand_of_its_inherited_default() {
    // The SECOND dispatch branch: an impl declares `partial_compare`, and the
    // operator reaches its operand through `PartialOrd`'s inherited `lt`/`le`/
    // `gt`/`ge` default, whose parameter is the TRAIT's `B`. `Counter { n = 1 }
    // < Point { x = 5, y = 2 }` answered `true` off slot 0.
    for (operator, method) in [("<", "lt"), (">", "gt"), ("<=", "le"), (">=", "ge")] {
        assert_fails_spanning(
            &format!(
                r#"
        import std::compare::{{ PartialOrd, PartialEq, Ordering }};
        import std::option::Option::{{ Some }};

        struct Counter {{ n: i32 }}
        struct Point {{ x: i32, y: i32 }}

        impl Counter with PartialEq {{
            fun eq(self, other: Counter): bool {{
                self.n == other.n
            }}
        }}

        impl Counter with PartialOrd {{
            fun partial_compare(self, other: Counter): Option<Ordering> {{
                if self.n < other.n {{ Some(Ordering::Less) }} else {{ Some(Ordering::Greater) }}
            }}
        }}

        fun main() {{
            let counter = Counter {{ n = 1 }};
            let point = Point {{ x = 5, y = 2 }};
            print(counter {operator} point);
        }}
        "#
            ),
            &format!("counter {operator} point"),
            &format!("`Counter`'s `{method}` accepts `Counter`, but the right operand is `Point`"),
        );
    }
}

#[test]
fn a_compound_assignment_rides_the_dispatch_check() {
    // `c += p` desugars to `c = c + p` and registers its own binary from the
    // second site — the same route B179's `total += value` takes.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Counter { n: i32 }
        struct Point { x: i32, y: i32 }

        impl Counter with Add {
            fun add(self, other: Counter): Counter {
                Counter { n = self.n + other.n }
            }
        }

        fun main() {
            mut counter = Counter { n = 1 };
            counter += Point { x = 7, y = 9 };
            print(counter.n);
        }
        "#,
        "counter += Point { x = 7, y = 9 }",
        "`Counter`'s `add` accepts `Counter`, but the right operand is `Point`",
    );
}

#[test]
fn a_concrete_non_self_b_accepts_it_and_refuses_the_subject() {
    // `impl Meters with Add<Feet>` is the whole reason this is a reconciliation
    // against the DECLARED `B` and not an equality against the subject: `Meters
    // + Feet` is the impl's entire point and must run, while `Meters + Meters`
    // — the shape a `B = Self` reader would assume is the safe one — is the
    // miscompile, and printed `6` by reading `other.f` off the `m` slot.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        struct Meters { m: i32 }
        struct Feet { f: i32 }

        impl Meters with Add<Feet> {
            fun add(self, other: Feet): Meters {
                Meters { m = self.m + other.f }
            }
        }

        fun main() {
            print((Meters { m = 1 } + Feet { f = 2 }).m);
        }
        "#,
        "3\n",
    );
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Meters { m: i32 }
        struct Feet { f: i32 }

        impl Meters with Add<Feet> {
            fun add(self, other: Feet): Meters {
                Meters { m = self.m + other.f }
            }
        }

        fun main() {
            let near = Meters { m = 1 };
            let far = Meters { m = 5 };
            print((near + far).m);
        }
        "#,
        "near + far",
        "`Meters`'s `add` accepts `Feet`, but the right operand is `Meters`",
    );
}

#[test]
fn the_impls_own_parameter_as_b_binds_from_the_subject_and_accepts() {
    // `impl Vec2<type T> with Add<T>` declares its `B` as its OWN parameter,
    // which the subject binds: at `Vec2<i32>` the operand must be an `i32`, at
    // `Vec2<str>` a `str`. Both run, from ONE impl — the acceptance the check
    // has to preserve, and the reason a bare "must equal the subject" guard
    // would have been wrong.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        struct Vec2<T> { a: T }

        impl Vec2<type T> with Add<T> {
            fun add(self, other: T): Vec2<T> {
                Vec2 { a = other }
            }
        }

        fun main() {
            print((Vec2 { a = 1 } + 5).a);
            print((Vec2 { a = "x" } + "y").a);
        }
        "#,
        "5\ny\n",
    );
}

#[test]
fn the_impls_own_parameter_as_b_refuses_a_mis_binding() {
    // The same impl, mis-bound: `Vec2<i32>`'s `B` is `i32`, and a `str` there
    // was stored and printed as `oops` before the fix. The refusal names what
    // the subject bound, not the impl's abstract `T`.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Vec2<T> { a: T }

        impl Vec2<type T> with Add<T> {
            fun add(self, other: T): Vec2<T> {
                Vec2 { a = other }
            }
        }

        fun main() {
            let pair = Vec2 { a = 1 };
            print((pair + "oops").a);
        }
        "#,
        r#"pair + "oops""#,
        "`Vec2<i32>`'s `add` accepts `i32`, but the right operand is `str`",
    );
}

#[test]
fn a_nested_impl_parameter_as_b_substitutes_through_its_nominal() {
    // `Add<Vec2<T>>` — the binder inside a nominal argument, not at the top of
    // the position. Same impl, both verdicts: `Vec2<i32> + Vec2<i32>` runs, and
    // a `Point` there printed `3` off slot 0 before the fix.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        struct Vec2<T> { a: T }

        impl Vec2<type T> with Add<Vec2<T>> {
            fun add(self, other: Vec2<T>): Vec2<T> {
                other
            }
        }

        fun main() {
            print((Vec2 { a = 1 } + Vec2 { a = 9 }).a);
        }
        "#,
        "9\n",
    );
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Vec2<T> { a: T }
        struct Point { x: i32, y: i32 }

        impl Vec2<type T> with Add<Vec2<T>> {
            fun add(self, other: Vec2<T>): Vec2<T> {
                other
            }
        }

        fun main() {
            let pair = Vec2 { a = 1 };
            let point = Point { x = 3, y = 4 };
            print((pair + point).a);
        }
        "#,
        "pair + point",
        "`Vec2<i32>`'s `add` accepts `Vec2<i32>`, but the right operand is `Point`",
    );
}

#[test]
fn a_bounded_generic_right_operand_of_a_dispatched_operator_is_rejected() {
    // B179's shape with a NOMINAL left operand. `Point` implements `Add`, so
    // the bound is satisfied and the call type-checked — and `bump(Counter { n
    // = 1 }, Point { x = 7, y = 9 })` printed `8`: `1 + 7`, the `Point`'s slot
    // 0 read as `other.n`. The declaration is checked once for all of its
    // instantiations, and `T: Add` promises `Add`'s methods, never that `T` IS
    // `Counter`.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Counter { n: i32 }
        struct Point { x: i32, y: i32 }

        impl Counter with Add {
            fun add(self, other: Counter): Counter {
                Counter { n = self.n + other.n }
            }
        }

        impl Point with Add {
            fun add(self, other: Point): Point {
                Point { x = self.x + other.x, y = self.y + other.y }
            }
        }

        fun bump<T: Add>(counter: Counter, value: T): Counter {
            counter + value
        }

        fun main() {
            print(bump(Counter { n = 1 }, Point { x = 7, y = 9 }).n);
        }
        "#,
        "counter + value",
        "`Counter`'s `add` accepts `Counter`, but the right operand is `T`",
    );
}

#[test]
fn a_concrete_b_does_not_admit_a_parameter_either() {
    // The same rule where the impl declares a non-`Self` `B`: `Add<i32>` does
    // not admit `T`, whatever `T` is bounded to. This is the reading B179's
    // second steer does NOT mean — see the pin below for the one it does.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Bag { total: i32 }

        impl Bag with Add<i32> {
            fun add(self, other: i32): Bag {
                Bag { total = self.total + other }
            }
        }

        fun bump<T: Add>(bag: Bag, value: T): Bag {
            bag + value
        }

        fun main() {
            print(bump(Bag { total = 1 }, 4).total);
        }
        "#,
        "bag + value",
        "`Bag`'s `add` accepts `i32`, but the right operand is `T`",
    );
}

#[test]
fn two_different_parameters_in_the_two_positions_are_rejected() {
    // `Bag<A>`'s `B` is `A`; a `B` from the caller's own list is a DIFFERENT
    // parameter, and nothing relates them. Rigid comparison is what says so —
    // the ordinary one treats each as a hole and answers `true`, which is how
    // `mix(Bag { first = 1 }, "x")` printed `x` before the fix.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        struct Bag<T> { first: T }

        impl Bag<type T> with Add<T> {
            fun add(self, other: T): Bag<T> {
                Bag { first = other }
            }
        }

        fun mix<A, B>(bag: Bag<A>, value: B): Bag<A> {
            bag + value
        }

        fun main() {
            print(mix(Bag { first = 1 }, "x").first);
        }
        "#,
        "bag + value",
        "`Bag<A>`'s `add` accepts `A`, but the right operand is `B`",
    );
}

#[test]
fn b179s_second_steer_now_names_a_spelling_that_works() {
    // B179's refusal steers to "put a left operand there whose `Add` declares a
    // `B` that admits `T`", and until B180 closed that sentence steered into a
    // broken route: EVERY nominal left operand accepted the parameter, so the
    // steer named a program that miscompiled instead of one that worked. This
    // is the spelling it means — the left operand's own type carries the
    // parameter, so its `Add` declares `B = T` and the operand IS a member.
    // Same generic function, two instantiations, both running through the impl.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        struct Bag<T> { first: T }

        impl Bag<type T> with Add<T> {
            fun add(self, other: T): Bag<T> {
                Bag { first = other }
            }
        }

        fun bump<T: Add>(bag: Bag<T>, value: T): Bag<T> {
            bag + value
        }

        fun main() {
            print(bump(Bag { first = 1 }, 4).first);
            print(bump(Bag { first = "a" }, "b").first);
        }
        "#,
        "4\nb\n",
    );
}

#[test]
fn a_generic_receiver_and_operand_of_one_parameter_still_compare() {
    // The control the rigid comparison must not break, and std's own shape:
    // `impl List<type T: PartialEq> with PartialEq { fun eq(self, b: List<T>) }`
    // reached from a generic body, where BOTH sides are the caller's `T`. The
    // parameters are rigid but IDENTICAL, which is exactly the case rigidity
    // admits.
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;

        fun same<T: PartialEq>(left: List<T>, right: List<T>): bool {
            left == right
        }

        fun main() {
            print(same([1, 2], [1, 2]));
            print(same(["a"], ["b"]));
        }
        "#,
        "true\nfalse\n",
    );
}

// --- B170: `+` skipped its check when the LEFT operand was non-nominal -------
//
// B148 closed the hole for a NATIVE left operand and the dispatch loop's own
// guard kept it open from the other side: the loop `continue`d unless the left
// operand was a `Struct` or an `Enum`, so a tuple, an array, a closure, a
// function reference, `void` and an unbounded generic reached neither the
// admitted set above nor the no-`Add` refusal below. The old anything-goes
// emission survived for exactly those shapes — `(1, 2) + 1` printed `1,21`
// (b148's own miscompile, entered from the left), `nothing() + 1` was `NaN`,
// a closure concatenated its SOURCE TEXT, and `let v = if false { 1 }; v + 1`
// printed a plausible wrong `1`.
//
// `+`'s admitted set is now reached by every left-operand SHAPE. The guard
// still exists, but it only decides whether an IMPL could be found — not
// whether the check runs — so a non-nominal operand with a real
// `impl (i32, i32) with Add` dispatches, and one without is refused by name.

#[test]
fn a_tuple_left_operand_of_addition_is_rejected() {
    // The exact mirror of `a_tuple_concatenated_into_a_string_is_rejected`:
    // the same runtime garbage, entered from the left.
    assert_fails_spanning(
        r#"
        fun main() {
            let pair = (1, 2);
            let _sum = pair + 1;
        }
        "#,
        "pair + 1",
        "`(i32, i32)` is neither: it has no `Add`",
    );
}

#[test]
fn a_void_left_operand_of_addition_is_rejected() {
    // `nothing() + 1` was `NaN`. `void` gets its own reason: there is no
    // value to add, and no impl to write either.
    assert_fails_spanning(
        r#"
        fun nothing() {}

        fun main() {
            let _sum = nothing() + 1;
        }
        "#,
        "nothing() + 1",
        "this operand is `void`",
    );
}

#[test]
fn a_valueless_if_left_operand_of_addition_is_rejected() {
    // The shape that reads as working code: an `if` with no `else` produces
    // no value, so `v + 1` printed `1` — a plausible wrong answer rather
    // than a visible `NaN`.
    assert_fails_spanning(
        r#"
        fun main() {
            let v = if false { 1 };
            let _sum = v + 1;
        }
        "#,
        "v + 1",
        "this operand is `void`",
    );
}

#[test]
fn a_closure_left_operand_of_addition_is_rejected() {
    // The right side has refused this since B148
    // (`a_closure_or_function_reference_concatenated_into_a_string_is_rejected`);
    // from the left it concatenated the closure's SOURCE TEXT.
    assert_fails_spanning(
        r#"
        fun main() {
            let f = |n: i32| n + 1;
            let _text = f + "!";
        }
        "#,
        r#"f + "!""#,
        "`|i32| i32` is neither: it has no `Add`",
    );
}

#[test]
fn a_fixed_array_left_operand_of_addition_is_rejected() {
    // `List` is a struct and was already refused; a fixed array is not, and
    // lowers to the same JS array, so it rendered the same comma-joined shape.
    assert_fails_spanning(
        r#"
        fun main() {
            let a: [i32; 2] = [ 1, 2 ];
            let _sum = a + 1;
        }
        "#,
        "a + 1",
        "`[i32; 2]` is neither: it has no `Add`",
    );
}

#[test]
fn a_function_reference_left_operand_of_addition_is_rejected() {
    assert_fails_with(
        r#"
        fun helper(n: i32): i32 { n }

        fun main() {
            let _text = helper + 1;
        }
        "#,
        "is neither: it has no `Add`",
    );
}

// --- B174: the generic LEFT operand, the deferred breaking step, TAKEN ------
//
// The other half of the operand-role frame, and the last escape from the
// unbounded-parameter check inside a trait default. B169 and B179 closed the
// RIGHT operand and B181 closed the logical pair's right half; each time the
// LEFT was left, on the stated ground that refusing it is a bound requirement
// on every trait default written over the trait's OWN parameter — a breaking
// generics change with a migration, not a miscompile fix.
//
// The census priced that migration and the number was ONE: a single compiler
// fixture (`macros::an_inherited_default_on_a_generic_subject_dispatches`)
// writes the shape, and the only other estate sites are the `#[ignore]`d pins
// below, which the change turns green. Zero in std, the corpus, docs fences,
// examples, templates, kolt or the website. Ruled 2026-09-01: take it, and
// require a bound that PROVIDES the operator's method rather than merely any
// bound — otherwise `<T: Display>` on the left of `+` stays exactly as broken
// as `<T>` (P4 of the census), and the two sides would still disagree about
// what a bound has to prove.
//
// The garbage the refusals replace, from the census and audit run 7:
//   `bump(Point { … })`            emitted `value + 1`      -> the tuple, concatenated
//   `both(Point { … }, true)`      emitted `value && flag`  -> the struct, typed `bool`
//   `same<T>(M { n = 7 }, M { n = 7 })`                     -> `false`
//   `less<T>(M { n = 10 }, M { n = 9 })`                    -> `true` ("10" < "9")
// Each a plausible wrong answer rather than a visible `NaN`.

#[test]
fn an_unbounded_generic_left_operand_of_addition_is_rejected() {
    // The declaration is checked once for all instantiations and an unbounded
    // `T` promises nothing, so `bump(Point { … })` emitted `value + 1` and the
    // host concatenated the struct's tuple.
    assert_fails_spanning(
        r#"
        fun bump<T>(value: T): T {
            value + 1
        }

        fun main() {
            let _n = bump(5);
        }
        "#,
        "value + 1",
        "`+` on `T` needs `T: Add`",
    );
}

#[test]
fn a_bounded_generic_left_operand_of_addition_still_dispatches() {
    // The escape hatch the refusal steers to has to work, or the rule would
    // have no legal spelling.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        fun bump<T: Add>(value: T, one: T): T {
            value + one
        }

        fun main() {
            print(bump(5, 1));
            print(bump(1.5, 2.5));
        }
        "#,
        "6\n4\n",
    );
}

#[test]
fn an_unbounded_generic_left_operand_of_the_sibling_operators_is_rejected() {
    // Audit run 7 widened B174 past `+`: every operator that models a trait
    // escaped through the same fall-through, and the arithmetic ones are the
    // least dangerous of them. `-` and `*` produced `NaN`, but the comparisons
    // produced plausible BOOLEANS — `same(M { n = 7 }, M { n = 7 })` was
    // `false` (JS compares the lowered structs by reference) and
    // `less(M { n = 10 }, M { n = 9 })` was `true` (lexicographic `"10" < "9"`).
    // Each names the bound that admits it, which differs per operator.
    assert_fails_spanning(
        r#"
        fun drop_one<T>(value: T): T {
            value - 1
        }

        fun main() {
            let _n = drop_one(5);
        }
        "#,
        "value - 1",
        "`-` on `T` needs `T: Sub`",
    );
    assert_fails_spanning(
        r#"
        fun same<T>(a: T, b: T): bool {
            a == b
        }

        fun main() {
            let _same = same(1, 1);
        }
        "#,
        "a == b",
        "`==` on `T` needs `T: PartialEq`",
    );
    assert_fails_spanning(
        r#"
        fun less<T>(a: T, b: T): bool {
            a < b
        }

        fun main() {
            let _less = less(1, 2);
        }
        "#,
        "a < b",
        "`<` on `T` needs `T: PartialOrd`",
    );
}

#[test]
fn a_trait_defaults_own_parameter_as_a_left_operand_is_rejected() {
    // THE breaking shape, and the whole of the migration the census priced:
    // a default written over the trait's own unbounded parameter. It cannot be
    // fixed locally the way a free function's can — the bound goes on the
    // TRAIT, and every `impl` and every bound naming it moves with it — which
    // is why the refusal says where the parameter is declared.
    //
    // It worked by luck at `i32` and printed `abab` for `Holder { value = "ab" }`.
    assert_fails_spanning(
        r#"
        trait Doubler<T> {
            fun once(self): T;

            fun twice(self): T {
                self.once() + self.once()
            }
        }

        struct Holder<T> {
            value: T,
        }

        impl Holder<type T> with Doubler<T> {
            fun once(self): T {
                self.value
            }
        }

        fun main() {
            print(Holder { value = 21 }.twice());
        }
        "#,
        "self.once() + self.once()",
        "declared on `trait Doubler`",
    );
}

#[test]
fn a_left_operand_bound_that_does_not_provide_the_operator_is_rejected() {
    // The ruling's refinement, and the difference between closing the item and
    // closing the hole (census §6.2, probe P4): "require a bound" is not
    // "require the RIGHT bound". `T: Display` promises `to_string`, not `add`,
    // and before this it fell through to the SAME native emission an unbounded
    // parameter did — `Holder { value = "ab" }` still printed `abab`. The
    // right operand already checked adequacy (`T: Display` with `+` and with
    // `==` both fail there), so the two sides disagreed about what a bound has
    // to prove; now they do not.
    assert_fails_spanning(
        r#"
        import std::display::Display;

        fun bump<T: Display>(value: T): T {
            value + 1
        }

        fun main() {
            let _n = bump(5);
        }
        "#,
        "value + 1",
        "its bounds (`Display`) do not declare `add`",
    );
    assert_fails_with(
        r#"
        import std::display::Display;

        trait Doubler<T: Display> {
            fun once(self): T;

            fun twice(self): T {
                self.once() + self.once()
            }
        }
        "#,
        "`+` on `T` needs `T: Add`",
    );
}

#[test]
fn a_generic_left_operand_of_the_logical_operators_is_rejected() {
    // B181's left half, and the one family where "add a bound" is NOT the fix:
    // `&&` and `||` admit `bool` and nothing else, they model no operator trait
    // at all, and no trait names that set — so a parameter refuses outright,
    // whatever it is bounded to, exactly as B181 already refused it on the
    // right. `both(Point { x = 1, y = 2 }, true)` printed the struct: JS's `&&`
    // yields its RIGHT operand when the left is truthy.
    assert_fails_with(
        r#"
        struct Point { x: i32, y: i32 }

        fun both<T>(value: T, flag: bool): bool {
            value && flag
        }

        fun main() {
            print(both(Point { x = 1, y = 2 }, true));
        }
        "#,
        "takes `bool` operands",
    );
    assert_fails_with(
        r#"
        import std::display::Display;

        fun either<T: Display>(value: T, flag: bool): bool {
            value || flag
        }

        fun main() {
            print(either(7, true));
        }
        "#,
        "no bound on `T` can prove membership",
    );
}

#[test]
fn a_bounded_generic_left_operand_of_the_sibling_operators_still_dispatches() {
    // Every refusal above has to have a legal spelling that RUNS, or the rule
    // would only be a way of rejecting programs. One per bound the refusals
    // name — `Sub`, `PartialEq`, `PartialOrd` — dispatched through the bound
    // and re-resolved to each instantiation's own impl.
    assert_compiles_and_runs(
        r#"
        import std::operators::Sub;
        import std::compare::PartialEq;
        import std::compare::PartialOrd;

        fun drop_one<T: Sub>(value: T, one: T): T {
            value - one
        }

        fun same<T: PartialEq>(a: T, b: T): bool {
            a == b
        }

        fun less<T: PartialOrd>(a: T, b: T): bool {
            a < b
        }

        fun main() {
            print(drop_one(5, 1));
            print(same(7, 7));
            print(same("a", "b"));
            print(less(9, 10));
            print(less(10, 9));
        }
        "#,
        "4\ntrue\nfalse\ntrue\nfalse\n",
    );
}

#[test]
fn a_supertrait_bound_admits_the_left_operand() {
    // The adequacy check reads the bound's SUPERTRAITS, not just the bound —
    // std's own `math::minmax<T: Ord>` writes `a <= b`, and `Ord`'s `le` comes
    // from its `PartialOrd` supertrait. A check that looked only at the named
    // trait would refuse std.
    assert_compiles_and_runs(
        r#"
        import std::compare::Ord;

        fun smaller<T: Ord>(a: T, b: T): T {
            if a <= b { a } else { b }
        }

        fun main() {
            print(smaller(9, 4));
            print(smaller("b", "a"));
        }
        "#,
        "4\na\n",
    );
}

#[test]
fn a_bounded_trait_parameter_left_operand_still_dispatches() {
    // The estate edit's own pin, in the spelling that shipped: the ONE site the
    // census found, migrated. The bound is orthogonal to what the fixture
    // asserts (that an inherited default dispatches on a generic impl subject),
    // so the answer is unchanged — and now it is an answer the declaration
    // earns rather than one it gets by luck at `i32`.
    //
    // The impl's binder does NOT restate the bound, which is why the migration
    // is one edit and not two.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Doubler<T: Add> {
            fun once(self): T;

            fun twice(self): T {
                self.once() + self.once()
            }
        }

        struct Holder<T> {
            value: T,
        }

        impl Holder<type T> with Doubler<T> {
            fun once(self): T {
                self.value
            }
        }

        fun main() {
            print(Holder { value = 21 }.twice());
        }

        main();
        "#,
        "42\n",
    );
    // And the bound the trait now carries is load-bearing at the instantiation
    // that used to produce garbage: this is the `abab` program, refused where
    // the parameter is GROUNDED rather than where it is written — so the
    // unrestated binder loses nothing.
    assert_fails_with(
        r#"
        import std::io::print;
        import std::operators::Add;

        struct Point { x: i32, y: i32 }

        trait Doubler<T: Add> {
            fun once(self): T;

            fun twice(self): T {
                self.once() + self.once()
            }
        }

        struct Holder<T> {
            value: T,
        }

        impl Holder<type T> with Doubler<T> {
            fun once(self): T {
                self.value
            }
        }

        fun main() {
            print(Holder { value = Point { x = 1, y = 2 } }.twice());
        }
        "#,
        "'Point' does not implement trait 'Add'",
    );
}

#[test]
fn a_derived_body_over_a_reached_parameter_satisfies_the_left_operands_bound() {
    // The EXEMPTION IS GONE (B194 landing), and this is the pin that used to
    // assert it. B174 drew a carve-out here on B188's boundary and for B188's
    // reason: `[derive(PartialEq)]` on a generic struct emits
    // `fun eq(self, other: ..)` comparing a `T`-typed field, and back then no
    // generated impl bound a parameter at all, so the rule would have refused
    // the derive surface wholesale from a span (`[derive(..)]`) that is not
    // where a bound goes. B194 made the generators generic-aware, so the same
    // programs now pass the rule instead of skipping it: the derived impl is
    // `impl Holder<type T: PartialEq> with PartialEq`, the `eq` body's left
    // operand is a BOUNDED `T`, and it dispatches through the bound like any
    // other generic operand.
    //
    // Non-vacuous by the red-proof the b194-landing lane ran: plant the removal
    // of B194's binder (`derive_binders` in `macro_std/src/meta.vl`, the reached
    // branch emitting a bare `type T`) and this first program is refused with
    // "in code generated by this attribute: `==` on `T` needs `T: PartialEq`" —
    // which is exactly the diagnostic the exemption existed to suppress, and
    // exactly what restoring the exemption suppresses again.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        [derive(PartialEq)]
        struct Holder<T> {
            value: T,
        }

        fun main() {
            print(Holder { value = 1 } == Holder { value = 1 });
        }
        "#,
        "true\n",
    );
    // Bounding the struct itself changes nothing — the declaration's own bounds
    // belong to the declaration and are not what the impl binds — so the same
    // spelling still works and still runs.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        [derive(PartialEq)]
        struct Holder<T: PartialEq> {
            value: T,
        }

        fun main() {
            print(Holder { value = 1 } == Holder { value = 2 });
        }
        "#,
        "false\n",
    );
    // And the bound the impl now carries BITES, at the author's own call site
    // rather than inside the generated body: this is the instantiation the
    // exemption's stated objection was about — a diagnostic anchored where the
    // reader cannot act — and it lands on the comparison the author wrote,
    // naming the concrete type and the trait it does not implement.
    assert_fails_with(
        r#"
        import std::io::print;

        struct Opaque { tag: i32 }

        [derive(PartialEq)]
        struct Holder<T> {
            value: T,
        }

        fun main() {
            print(Holder { value = Opaque { tag = 1 } } == Holder { value = Opaque { tag = 1 } });
        }
        "#,
        "'Opaque' does not implement trait 'PartialEq', required by a generic bound of this call",
    );
    // The ENUM half of the same generator, which is a second code path and had
    // no pin anywhere before this lane: a variant's `eq` compares PAYLOAD
    // bindings (`s0 == o0`) rather than field accesses, and reachability reads
    // payload types, so `Slot<T>`'s `T` is reached and the binder carries
    // `PartialEq` exactly as the struct's does. Both branches of the match, and
    // the payload-less variant that compares nothing at all.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        [derive(PartialEq)]
        enum Slot<T> {
            Empty,
            Full(T),
        }

        fun main() {
            print(Slot::Full(1) == Slot::Full(1));
            print(Slot::Full(1) == Slot::Full(2));
            print(Slot<i32>::Empty == Slot<i32>::Empty);
        }
        "#,
        "true\nfalse\ntrue\n",
    );
}

#[test]
fn a_derived_body_over_a_phantom_parameter_is_refused_nothing() {
    // The other side of B194's rule, and the reason lifting the exemption costs
    // C7 nothing. A PHANTOM parameter takes a BARE binder
    // (`impl Handle<type T> with PartialEq`) — the C7 departure from Rust's
    // derive rule — so the left operand of every operator the generated body
    // writes is a grounded field type, never `T`. There is no operator on `T`
    // here to refuse, whatever `T` is instantiated with: `Session` holds a
    // closure and implements nothing at all.
    //
    // Had the bare binder been the wrong call, this is where the lift would
    // have shown it — an unbounded `T` reaching the rule through generated
    // code. It does not reach it, because the body never touches `T`.
    //
    // Non-vacuous by its own red-proof: plant Rust's rule in `derive_binders`
    // (bind the trait on the phantom branch too) and this program is refused
    // `'Session' does not implement trait 'PartialEq', required by a generic
    // bound of this call` — which is the C7 contradiction, in the small.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        struct Session { socket: |str| void }

        [derive(PartialEq)]
        struct Handle<T> {
            index: i32,
            generation: i32,
        }

        fun main() {
            let live: Handle<Session> = Handle { index = 1, generation = 0 };
            print(live == Handle<Session> { index = 1, generation = 0 });
            print(live == Handle<Session> { index = 2, generation = 0 });
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn a_tuple_left_operand_of_the_sibling_operators_is_rejected() {
    // The guard gated the whole loop, not just `+`, so every operator that
    // models a trait skipped its no-impl refusal for these shapes and emitted
    // the host's: `(1, 2) == (1, 2)` was `false` (JS compares references),
    // `(1, 2) < (1, 3)` was `true` (JS compares `"1,2" < "1,3"`), and
    // `(1, 2) - 1` was `NaN`.
    assert_fails_with(
        r#"
        fun main() {
            let _same = (1, 2) == (1, 2);
        }
        "#,
        "type '(i32, i32)' does not implement the `PartialEq` operator",
    );
    assert_fails_with(
        r#"
        fun main() {
            let _ordered = (1, 2) < (1, 3);
        }
        "#,
        "type '(i32, i32)' does not implement the `PartialOrd` operator",
    );
    assert_fails_with(
        r#"
        fun main() {
            let _difference = (1, 2) - 1;
        }
        "#,
        "type '(i32, i32)' does not implement the `Sub` operator",
    );
}

#[test]
fn a_void_or_function_operand_is_refused_without_impl_advice() {
    // A refusal is worth what the reader can do with it, and "add
    // `impl void with PartialEq`" is not something anyone can write. `void`
    // and a function value get the reason instead; a tuple and an array,
    // whose impls DO resolve, keep the standard advice above.
    assert_fails_with(
        r#"
        fun nothing() {}

        fun main() {
            let _same = nothing() == nothing();
        }
        "#,
        "`==` needs a value on the left, and this operand is `void`",
    );
    assert_fails_with(
        r#"
        fun main() {
            let f = |n: i32| n;
            let _same = f == f;
        }
        "#,
        "a function value has no `PartialEq`, and none can be written for one",
    );
}

#[test]
fn a_tuple_with_its_own_partial_eq_impl_dispatches() {
    // Proof the advice the tuple refusal gives is advice that works — and
    // that the fix routes the shape through the impl LOOKUP, not just
    // through a refusal.
    assert_compiles_and_runs(
        r#"
        import std::compare::PartialEq;

        impl (i32, i32) with PartialEq {
            fun eq(self, b: (i32, i32)): bool {
                self.0 == b.0 && self.1 == b.1
            }
        }

        fun main() {
            print((1, 2) == (1, 2));
            print((1, 2) == (1, 3));
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn a_tuple_with_its_own_add_impl_dispatches() {
    // The guard used to skip the DISPATCH too, so an `impl` on a non-nominal
    // subject resolved and then never ran: `(1, 2) + 1` emitted native `+`
    // and produced the string `"1,21"`. Routing the shape through the check
    // routes it through the impl lookup as well.
    assert_compiles_and_runs(
        r#"
        import std::operators::Add;

        impl (i32, i32) with Add<i32> {
            fun add(self, b: i32): (i32, i32) {
                (self.0 + b, self.1 + b)
            }
        }

        fun main() {
            let t = (1, 2) + 1;
            print(t.0);
            print(t.1);
        }
        "#,
        "2\n3\n",
    );
}

// --- §J.3: module-level initializers cannot await ----------------------------
//
// Initializers run at module load — no enclosing function to become async,
// no top-level await in the emission model. An async call there used to
// type-check as `T` while holding a live promise at runtime (`state + 1`
// was garbage); it is now refused cleanly. Creating async closures stays
// legal: nothing awaits at load.

#[test]
fn an_async_call_in_a_module_initializer_is_rejected() {
    assert_fails_spanning(
        r#"
        import std::io::print;
        import std::time::{ sleep_for, Duration };

        async fun ready(tag: str): i32 {
            sleep_for(Duration::millis(1));
            42
        }

        let state = ready("boot");

        fun main() {
            print(state + 1);
        }
        "#,
        r#"ready("boot")"#,
        "a module-level binding cannot await",
    );
}

#[test]
fn an_initializer_calling_an_inferred_async_function_is_rejected() {
    // `warm` never says `async`; it is inferred (it calls `sleep_for`), and
    // the initializer's call to it is refused all the same.
    assert_fails_spanning(
        r#"
        import std::time::{ sleep_for, Duration };

        fun warm(tag: str): i32 {
            sleep_for(Duration::millis(1));
            7
        }

        let state = warm("boot");

        fun main() {
            let _s = state;
        }
        "#,
        r#"warm("boot")"#,
        "calls `warm`, which is async",
    );
}

#[test]
fn creating_an_async_closure_in_an_initializer_stays_legal() {
    // The charge is on AWAITING at load, not on holding async machinery:
    // a closure created in an initializer awaits nothing until called.
    assert_compiles(
        r#"
        import std::time::{ sleep_for, Duration };

        let warm = || sleep_for(Duration::millis(1));

        fun main() {
            let _w = warm;
        }
        "#,
    );
}

// --- B86: the rule is AWAIT-shaped, not call-shaped ---------------------------
//
// The shipped check walked `initializer_calls_of`, so it only ever saw an
// async CALL. An `await` whose operand is not a call — a `Task`-valued
// binding, a spawn, a `Task` returned by a plain sync function — slipped
// through, compiled clean, and emitted a genuine top-level `await` into the
// bundle (`top-level-await.md` §1.3), which then miscompiled on the Node leg
// (§1.4) and failed to parse at all under HMR (§1.5). These pin every row of
// §5.2's boundary table, per case.

/// `ready` + a module-level spawn, the shared preamble for the rows below.
const AWAIT_SHAPED_PREAMBLE: &str = r#"
        import std::io::print;
        import std::task::Task;
        import std::time::{ sleep_for, Duration };

        fun ready(): i32 {
            sleep_for(Duration::millis(1));
            7
        }
"#;

#[test]
fn awaiting_a_task_valued_module_binding_is_rejected() {
    assert_fails_spanning(
        &format!(
            "{AWAIT_SHAPED_PREAMBLE}
        let pending: Task<i32> = async ready();
        let value: i32 = await pending;

        fun main() {{
            print(value);
        }}
        "
        ),
        "await pending",
        "a module-level binding cannot suspend",
    );
}

#[test]
fn awaiting_a_task_valued_module_binding_steers_to_main() {
    // The second message form: the operand is right there and already
    // spawned, so the steer is to move the `await`, not to restructure.
    assert_fails_noting(
        &format!(
            "{AWAIT_SHAPED_PREAMBLE}
        let pending: Task<i32> = async ready();
        let value: i32 = await pending;

        fun main() {{
            print(value);
        }}
        "
        ),
        "a module-level binding cannot suspend",
        "await pending",
        "hold `pending` here and `await` it in `main`",
    );
}

#[test]
fn awaiting_a_spawn_in_an_initializer_is_rejected() {
    // The sharpest hole: `async ready()` is a CREATION, so the call to
    // `ready` lives inside the spawned closure and never entered the
    // initializer's direct call set.
    assert_fails_spanning(
        &format!(
            "{AWAIT_SHAPED_PREAMBLE}
        let value: i32 = await async ready();

        fun main() {{
            print(value);
        }}
        "
        ),
        "await async ready()",
        "a module-level binding cannot suspend",
    );
}

#[test]
fn awaiting_an_async_block_in_an_initializer_is_rejected() {
    assert_fails_spanning(
        &format!(
            "{AWAIT_SHAPED_PREAMBLE}
        let value: i32 = await async {{ 7 }};

        fun main() {{
            print(value);
        }}
        "
        ),
        "await async { 7 }",
        "a module-level binding cannot suspend",
    );
}

#[test]
fn awaiting_a_task_from_a_sync_function_is_rejected() {
    // `spawn_it` is not async — it returns a `Task`. The call check sees a
    // sync callee and passes; the await check is what refuses it.
    assert_fails_spanning(
        &format!(
            "{AWAIT_SHAPED_PREAMBLE}
        fun spawn_it(): Task<i32> {{
            async ready()
        }}

        let value: i32 = await spawn_it();

        fun main() {{
            print(value);
        }}
        "
        ),
        "await spawn_it()",
        "a module-level binding cannot suspend",
    );
}

#[test]
fn an_await_nested_in_an_initializer_expression_is_rejected() {
    // Any `await` REACHABLE in the initializer's own expression tree — not
    // just one at its root.
    assert_fails_spanning(
        &format!(
            "{AWAIT_SHAPED_PREAMBLE}
        let pending: Task<i32> = async ready();
        let value: i32 = (await pending) + 1;

        fun main() {{
            print(value);
        }}
        "
        ),
        "await pending",
        "a module-level binding cannot suspend",
    );
}

#[test]
fn awaiting_a_non_task_in_an_initializer_is_rejected() {
    // `await` on a plain value is legal JS and legal vilan inside a function;
    // at module level it is still a suspension point, so it is still refused
    // — and the steer stays true (it never claims the operand is a spawn).
    assert_fails_spanning(
        r#"
        import std::io::print;

        let plain: i32 = 7;
        let value: i32 = await plain;

        fun main() {
            print(value);
        }
        "#,
        "await plain",
        "a module-level binding cannot suspend",
    );
}

#[test]
fn an_await_inside_a_closure_created_by_an_initializer_stays_legal() {
    // THE BOUNDARY, kept deliberately where the call-shaped check had it: a
    // closure's body is not the initializer. Creating the closure suspends
    // nothing at load, so the initializer does not await — only calling it
    // does, and that happens wherever the caller is.
    assert_compiles(&format!(
        "{AWAIT_SHAPED_PREAMBLE}
        let pending: Task<i32> = async ready();
        let later = || {{ await pending }};

        async fun main() {{
            print(await later());
        }}
        "
    ));
}

#[test]
fn an_await_inside_an_async_block_in_an_initializer_stays_legal() {
    // Same boundary through the `async { .. }` spelling: the block lowers to
    // a closure, which is its own unit.
    assert_compiles(&format!(
        "{AWAIT_SHAPED_PREAMBLE}
        let pending: Task<i32> = async ready();
        let wrapped: Task<i32> = async {{ await pending }};

        async fun main() {{
            print(await wrapped);
        }}
        "
    ));
}

#[test]
fn spawning_at_module_level_stays_legal() {
    // The idiom the diagnostic steers to, and the reason the null
    // recommendation holds: the work starts at load, only the observation
    // moves into `main`.
    assert_compiles(&format!(
        "{AWAIT_SHAPED_PREAMBLE}
        let pending: Task<i32> = async ready();

        async fun main() {{
            print(await pending);
        }}
        "
    ));
}

#[test]
fn an_explicit_await_on_an_async_call_keeps_the_call_message() {
    // `await ready()` is BOTH an await and an async call. The call form names
    // the callee, so it wins — and it must be the ONLY diagnostic, not a pair
    // for one line.
    // `warm` takes an argument so the call site's snippet (`warm(1)`) is
    // distinct from its declaration — the span must land on the call.
    let source = r#"
        import std::io::print;
        import std::time::{ sleep_for, Duration };

        fun warm(seed: i32): i32 {
            sleep_for(Duration::millis(1));
            seed
        }

        let value: i32 = await warm(1);

        fun main() {
            print(value);
        }
        "#;
    assert_fails_spanning(source, "warm(1)", "calls `warm`, which is async");
    let refusals = failure_diagnostics(source)
        .into_iter()
        .filter(|(message, _)| {
            message.contains("cannot await (module initialization is synchronous)")
                || message.contains("cannot suspend")
        })
        .count();
    assert_eq!(
        refusals, 1,
        "one refusal per binding, not a pair for one line"
    );
}

// --- J6: `main`'s promise gets a contract ------------------------------------
//
// An async `main` is emitted as a fire-and-forget IIFE, and its promise used to
// be DISCARDED. What a failing `main` then did was the HOST's policy, not
// vilan's: Node >= 15 rethrows an unhandled rejection and exits non-zero, but
// it buries the program's error under `UnhandledPromiseRejection` and an
// engine-internal stack, and a host configured otherwise exits 0. A sync `main`
// that panics has always terminated with the message and a non-zero code, and
// async `main` is what the language steers people to instead of top-level
// await (`top-level-await.md` §4.4/§8.3) — so the two must agree.

#[test]
fn a_rejecting_async_main_exits_nonzero_with_the_error_surfaced() {
    let (stdout, stderr, code) = compile_and_run_status(
        r#"
        import std::{ io::print, io::panic };
        import std::time::{ sleep_for, Duration };

        async fun main() {
            sleep_for(Duration::millis(1));
            print("before");
            panic("boom");
            print("after");
        }
        "#,
    );
    assert_eq!(code, 1, "a rejecting `main` must exit 1; stderr: {stderr}");
    assert!(
        stderr.contains("boom"),
        "the program's own error must reach stderr: {stderr}"
    );
    // The point is not merely a non-zero code — Node already gave one. It is
    // that the failure is OURS to report, so the host's unhandled-rejection
    // wrapper is gone and what remains is the message.
    assert!(
        !stderr.contains("UnhandledPromiseRejection")
            && !stderr.contains("ERR_UNHANDLED_REJECTION"),
        "the error must be surfaced by the shim, not left to the host's \
         unhandled-rejection path: {stderr}"
    );
    assert!(
        stdout.contains("before") && !stdout.contains("after"),
        "output before the failure must still flush, and nothing after it \
         may run: {stdout:?}"
    );
}

#[test]
fn a_resolving_async_main_exits_zero() {
    let (stdout, stderr, code) = compile_and_run_status(
        r#"
        import std::io::print;
        import std::time::{ sleep_for, Duration };

        async fun main() {
            sleep_for(Duration::millis(1));
            print("ok");
        }
        "#,
    );
    assert_eq!(code, 0, "a resolving `main` must exit 0; stderr: {stderr}");
    assert_eq!(stdout, "ok\n");
}

#[test]
fn a_panicking_sync_main_is_unchanged() {
    // The contract async `main` was brought level WITH; it must not move.
    let (stdout, stderr, code) = compile_and_run_status(
        r#"
        import std::{ io::print, io::panic };

        fun main() {
            print("before");
            panic("boom");
        }
        "#,
    );
    assert_eq!(code, 1, "a panicking sync `main` still exits 1: {stderr}");
    assert!(stderr.contains("boom"), "and still says why: {stderr}");
    assert!(stdout.contains("before"));
}

#[test]
fn an_async_main_that_keeps_working_is_not_cut_short() {
    // THE SERVER-LEG CARVE, in the form a test can hold: the shim attaches a
    // handler, it does not `await`. A `main` that suspends and resumes — the
    // shape a listening server generalizes — runs to completion, and a `main`
    // that never settles is likewise never hurried. Had the shim awaited the
    // IIFE (or exited on settle), this would truncate.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::time::{ sleep_for, Duration };

        async fun main() {
            print("start");
            sleep_for(Duration::millis(30));
            print("middle");
            sleep_for(Duration::millis(30));
            print("end");
        }
        "#,
        "start\nmiddle\nend\n",
    );
}

#[test]
fn the_browser_leg_gets_no_exit_handler() {
    // The browser has no exit code, and its own unhandled-rejection path
    // already reports to the console — so there is nothing to attach, and
    // `process` does not exist to reference.
    let emitted = compile_browser(
        r#"
        import std::io::print;
        import std::time::{ sleep_for, Duration };

        async fun main() {
            sleep_for(Duration::millis(1));
            print("ui");
        }
        "#,
    )
    .expect("expected a clean browser compile");
    assert!(
        !emitted.contains("process.exit"),
        "the browser bundle must not reference `process`:\n{emitted}"
    );
    assert!(
        emitted.contains("})();"),
        "the browser entry stays the bare fire-and-forget IIFE:\n{emitted}"
    );
}

// --- The i53/u53 rename (numeric-types.md §8) --------------------------------
//
// The f64-backed wide integers are named for the precision they deliver
// (±2^53), and unknown numeric suffixes are ERRORS rather than silently
// typing as unsuffixed (`5q` once compiled as an i32).

#[test]
fn an_unknown_numeric_suffix_errors() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = 5q;
        }
        "#,
        "5q",
        "unknown numeric suffix `q`",
    );
}

#[test]
fn a_fractional_literal_with_an_unknown_suffix_errors() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = 2.5q;
        }
        "#,
        "2.5q",
        "unknown numeric suffix `q`",
    );
}

#[test]
fn the_old_i64_suffix_errors_with_a_rename_hint() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _stamp = 1000i64;
        }
        "#,
        "1000i64",
        "`i64` was renamed to `i53`",
    );
}

#[test]
fn the_old_u64_suffix_errors_with_a_rename_hint() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _wide = 1000u64;
        }
        "#,
        "1000u64",
        "`u64` was renamed to `u53`",
    );
}

#[test]
fn i53_suffixed_literals_compile_and_run() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let wide = 9007199254740992i53;
            print(wide);
            print((3.9).as_i53());
            print((5i53).as_u53());
        }
        "#,
        "9007199254740992\n3\n5\n",
    );
}

// --- Bare-namespace paths in expression position (found by the walkthrough) --
//
// `std::math::min(1, 2)` inline used to PANIC the compiler: the failed
// resolution of the path head left its type id unmapped, and the static-
// accessor pass crashed on the first `get_type`. The namespace root is not
// a binding by design — qualified access goes through an imported module
// name — so the shape is a clean, guiding error now.

#[test]
fn a_bare_std_function_path_errors_cleanly() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = std::math::min(1, 2);
        }
        "#,
        "std",
        "`std` is a namespace, not a value",
    );
}

#[test]
fn a_bare_std_variant_path_errors_cleanly() {
    assert_fails_spanning(
        r#"
        fun main() {
            let _x = std::compare::Ordering::Less;
        }
        "#,
        "std",
        "`std` is a namespace, not a value",
    );
}

#[test]
fn an_imported_module_alias_qualifies_statics() {
    // The supported spelling: import the module, qualify through its name.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::math;

        fun main() {
            print(math::min(1, 2));
        }
        "#,
        "1\n",
    );
}

// --- Direct calls on postfix results (backlog §H.18, fixed) ------------------
//
// `self.hook.read()(a, b)` used to fail to parse ("expected a method name
// after `.`"): the member grammar greedily folded the second `(args)` into
// the member. A member now fuses at most ONE call; further `(args)` are
// direct-call postfixes on the chain (calling a closure-typed value).

#[test]
fn a_method_call_result_is_directly_callable() {
    // The service-hook shape that carried the bind-first workaround.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;

        struct Holder {
            hook: Shared<|i32, i32| i32>,
        }

        fun main() {
            let holder = Holder { hook = Shared::new(|a: i32, b: i32| a + b) };
            print(holder.hook.read()(20, 22));
        }
        "#,
        "42\n",
    );
}

#[test]
fn an_index_result_is_directly_callable() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let handlers: List<|i32| i32> = [|n: i32| n * 2, |n: i32| n + 1];
            print(handlers[0](21));
            print(handlers[1](41));
        }
        "#,
        "42\n42\n",
    );
}

#[test]
fn a_direct_call_chains_into_further_postfixes() {
    // The direct call's result re-enters the chain (here: indexed).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::shared::Shared;

        struct Factory {
            make: Shared<|i32| List<i32>>,
        }

        fun main() {
            let factory = Factory { make = Shared::new(|seed: i32| [seed, seed * 2]) };
            print(factory.make.read()(21)[1]);
        }
        "#,
        "42\n",
    );
}

#[test]
fn tuple_member_access_grounds() {
    // §I.19, fixed: `.0` resolves positionally against the tuple's elements
    // (spec §5.9) — the field path grew its Tuple arm. Destructuring remains
    // the multi-element form; `.0` is the point access.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let pair: (i32, i32) = (41, 1);
            print(pair.0 + pair.1);
        }
        "#,
        "42\n",
    );
}

#[test]
fn tuple_member_access_infers_without_an_annotation() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let pair = (40, 2);
            print(pair.0 + pair.1);
        }
        "#,
        "42\n",
    );
}

#[test]
fn tuple_elements_carry_their_own_types() {
    // `.1` on `(i32, str)` is a str — methods dispatch on the element type.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let entry = (7, "vilan");
            print(entry.1.len());
        }
        "#,
        "5\n",
    );
}

#[test]
fn nested_tuple_access_chains() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let nested = ((1, 2), 3);
            print(nested.0.1);
        }
        "#,
        "2\n",
    );
}

#[test]
fn a_tuple_typed_element_reads_as_a_value() {
    // Flat storage: `.0` on a nested tuple reslices its region, and the
    // result behaves as a full tuple value (destructure, re-access).
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let nested = ((1, 2), 3);
            let inner = nested.0;
            let (x, y) = inner;
            print(inner.1 + x + y);
        }
        "#,
        "5\n",
    );
}

#[test]
fn a_tuple_typed_element_assignment_writes_its_region() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut nested = ((1, 2), 3);
            nested.0 = (40, 2);
            print(nested.0.0 + nested.0.1 + nested.1);
        }
        "#,
        "45\n",
    );
}

#[test]
fn a_nested_tuple_write_hits_the_storage_not_a_copy() {
    // Chained positional accesses FOLD to one flat offset on the root, so a
    // write through a nested path mutates the tuple — never a resliced copy.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut deep = ((1, 2), 3);
            deep.0.1 = 41;
            print(deep.0.1 + deep.0.0);
        }
        "#,
        "42\n",
    );
}

#[test]
fn a_tuple_element_out_of_range_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            let pair = (41, 1);
            let _x = pair.2;
        }
        "#,
        "pair.2",
        "has no element 2: its arity is 2",
    );
}

#[test]
fn a_named_member_on_a_tuple_is_rejected() {
    assert_fails_spanning(
        r#"
        fun main() {
            let pair = (41, 1);
            let _x = pair.first;
        }
        "#,
        "pair.first",
        "a tuple's members are its positions",
    );
}

#[test]
fn a_tuple_element_assigns_through_a_mut_binding() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut pair: (i32, i32) = (41, 1);
            pair.0 = 40;
            pair.1 = 2;
            print(pair.0 + pair.1);
        }
        "#,
        "42\n",
    );
}

#[test]
fn a_tuple_element_assignment_needs_a_mut_binding() {
    assert_fails(
        r#"
        fun main() {
            let pair: (i32, i32) = (41, 1);
            pair.0 = 5;
        }
        "#,
    );
}

// --- Never-typed divergence (two gotchas closed) ------------------------------
//
// `panic(..)`, `ret ..`, and `jump break/continue` now type as `Never`,
// which YIELDS in unification: a diverging match leg or if branch no longer
// constrains (panic's old `Any` absorbed the whole match; `ret` legs typed
// void and mismatched). The transformer emits diverging leg results as
// statements (`return e`, not `x = return e`).

#[test]
fn a_ret_leg_no_longer_poisons_the_match_type() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::option::Option::{ self, Some, None };

        fun first_or_bail(items: List<i32>): i32 {
            mut copy = items;
            let head = match copy.pop() {
                Some(let value) => value,
                None => ret 0 - 1,
            };
            head * 2
        }

        fun main() {
            print(first_or_bail([21]));
            let empty: List<i32> = [];
            print(first_or_bail(empty));
        }
        "#,
        "42\n-1\n",
    );
}

#[test]
fn a_panic_leg_no_longer_absorbs_the_match_type() {
    // The binding is UNANNOTATED — the value leg's type wins.
    assert_compiles_and_runs(
        r#"
        import std::{ io::print, io::panic };
        import std::option::Option::{ self, Some, None };

        fun unwrap_or_panic(slot: Option<str>): str {
            let value = match slot {
                Some(let text) => text,
                None => panic("missing"),
            };
            value + "!"
        }

        fun main() {
            print(unwrap_or_panic(Some("hi")));
        }
        "#,
        "hi!\n",
    );
}

#[test]
fn a_panicking_if_branch_yields_to_the_other() {
    assert_compiles_and_runs(
        r#"
        import std::{ io::print, io::panic };

        fun main() {
            let flag = true;
            let picked = if flag { 42 } else { panic("no") };
            print(picked);
        }
        "#,
        "42\n",
    );
}

#[test]
fn a_jump_leg_diverges_inside_a_loop() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            mut total = 0;
            for step in [1, 0, 2, 0, 3] {
                let value = match step {
                    0 => jump continue,
                    let n => n,
                };
                total += value;
            }
            print(total);
        }
        "#,
        "6\n",
    );
}

#[test]
fn all_diverging_legs_still_satisfy_an_annotation() {
    // Never fits any expected type; nothing runs past the match.
    assert_compiles(
        r#"
        import std::io::panic;

        fun choose(flag: bool): i32 {
            let value: i32 = match flag {
                true => panic("a"),
                false => ret 0,
            };
            value
        }

        fun main() {
            let _n = choose(false);
        }
        "#,
    );
}

#[test]
fn a_direct_call_types_several_unannotated_parameters() {
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let add = |a, b| a + b;
            print(add(20, 22));
        }
        "#,
        "42\n",
    );
}

#[test]
fn a_direct_call_respects_annotated_parameters() {
    // Mixed: the annotation stays authoritative; only the Unknown fills.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let scale = |a: i32, b| a * b;
            print(scale(6, 7));
        }
        "#,
        "42\n",
    );
}

// --- B166: the struct-field ASSIGNMENT door is checked, by the literal's rule -
//
// `s.field = value` checked NOTHING. `b.value = "text"` into an `i32` field
// compiled and `b.value + 1` printed `text1`; a bare closure assigned into an
// `Option<|E| void>` field ran with its `is Some` test silently never matching.
// The literal door (`S { field = value }`) had always checked, so the two doors
// disagreed about the same value. Both now go through `check_field_value` —
// one rule, both doors — so every shape below is refused at the value's span
// (E7), the same anchor the literal door uses.

#[test]
fn a_str_assigned_into_an_i32_field_is_refused() {
    // The generalized form of the owner's find: accepted, and then `+ 1`
    // computed on it printed `text1`.
    assert_fails_spanning(
        r#"
        struct Box { value: i32 }

        fun main() {
            mut b = Box { value = 0 };
            b.value = "text";
            print(b.value + 1);
        }
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn a_bool_assigned_into_an_i32_field_is_refused() {
    // Scalar into scalar: no operand check stands in the way here, so this
    // shape reached the field with nothing between it and the store.
    assert_fails_with(
        r#"
        struct Box { value: i32 }

        fun main() {
            mut b = Box { value = 0 };
            b.value = true;
        }
        "#,
        "Expected i32, but got bool instead.",
    );
}

#[test]
fn a_bare_value_assigned_into_an_option_field_is_refused() {
    // The silent half: the value lands unwrapped, so the `is Some` test that
    // reads it back never matches and the program takes the wrong branch
    // without a word.
    assert_fails_with(
        r#"
        struct Box { value: Option<i32> }

        fun main() {
            mut b = Box { value = None };
            b.value = 5;
        }
        "#,
        "Expected Option<i32>, but got i32 instead.",
    );
}

#[test]
fn a_bare_closure_assigned_into_an_option_closure_field_is_refused() {
    // The owner's original exhibit (kolt's `DragHandler`): a builder storing
    // its handler with `self.field = handler` where the field is
    // `Option<|E| void>`. It ran, and the `is Some` never matched — the
    // refusal steers to the `Some(handler)` he meant to write.
    assert_fails_with(
        r#"
        struct Handler { on_move: Option<|i32| void> }

        impl Handler {
            fun new(): Handler {
                Handler { on_move = None }
            }

            fun on_move(own self, handler: |i32| void): Handler {
                self.on_move = handler;
                self
            }
        }

        fun main() {
            let _ = Handler::new().on_move(|n| print(n));
        }
        "#,
        "Expected Option<|i32| void>, but got |i32| void instead.",
    );
}

#[test]
fn a_nested_field_assignment_is_checked() {
    // `a.b.c = v` — the place chain is deeper, but it is still a field
    // target, so the same rule reaches it.
    assert_fails_spanning(
        r#"
        struct Inner { n: i32 }
        struct Outer { inner: Inner }

        fun main() {
            mut o = Outer { inner = Inner { n = 1 } };
            o.inner.n = "text";
        }
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn an_indexed_field_assignment_is_checked() {
    // `list[i].f = v` — the subject is a subscript rather than a local, and
    // the check keys on the TARGET resolving to a field, not on the shape of
    // what it is rooted in.
    assert_fails_spanning(
        r#"
        struct Cell { n: i32 }

        fun main() {
            mut cells = [Cell { n = 1 }];
            cells[0].n = "text";
        }
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

#[test]
fn a_compound_assignment_into_a_field_checks_the_value_that_lands() {
    // `b.text += 5` desugars to `b.text = b.text + 5`, and it is the SUM that
    // lands in the field — `str + i32` is `str`, so this is well typed and
    // must still run. Checking the written `5` instead of the sum (the
    // obvious wrong way to register the constraint) refuses it with
    // "Expected str, but got i32", which is what makes this pin bite.
    //
    // No compound shape can land the WRONG type today: every overloadable
    // operator's trait returns `Self`, so a compound that type-checks as a
    // binary at all yields the field's own type. The check is there; it
    // simply has nothing to catch until an operator can widen.
    assert_compiles_and_runs(
        r#"
        struct Box { text: str }

        fun main() {
            mut b = Box { text = "a" };
            b.text += 5;
            print(b.text);
        }
        "#,
        "a5\n",
    );
}

#[test]
fn a_well_typed_field_assignment_still_stores_what_the_pattern_reads_back() {
    // The green half of the Option shape: written `Some(..)`, the `is Some`
    // matches — which is what the silent version was supposed to do.
    assert_compiles_and_runs(
        r#"
        struct Box { value: Option<i32> }

        fun main() {
            mut b = Box { value = None };
            b.value = Some(5);
            if b.value is Some(let n) {
                print(n);
            }
        }
        "#,
        "5\n",
    );
}

#[test]
fn the_literal_door_control_refuses_the_same_value_it_always_did() {
    // The control: the literal door has always refused this, and routing it
    // through the shared rule must not have moved it — same message, same
    // anchor on the field's value.
    assert_fails_spanning(
        r#"
        struct Box { value: i32 }

        fun main() {
            let _ = Box { value = "text" };
        }
        "#,
        "\"text\"",
        "Expected i32, but got str instead.",
    );
}

// --- B167: an `is`-capture that is CALLED reads the same alias a read does ---
//
// `if stored is Some(let f) { f() }` compiled to `f()` against an `f` nothing
// ever declared — `ReferenceError` at run time from accepted vilan. An `is`
// capture has no declaration of its own: it is ALIASED to the subject's
// payload slot (`$a[1]`) and substituted at each use. The value-read arm
// consulted that alias table; the call arm's named-callee fast path, written
// for functions, did not, and swallowed every other kind of local. So the
// defect bit exactly the payloads whose uses are calls — closure-typed ones —
// which is why an `i32` payload had always worked and why `match`, whose legs
// DECLARE their captures, worked too. Every pin here RUNS the bundle: the
// program compiled before, so only execution can tell the difference.

#[test]
fn a_closure_payload_capture_is_callable_in_the_arm() {
    // The minimal repro. Direct use, no shadowing, no nesting — the name is
    // irrelevant, which is what falsified the first (alpha-rename) reading.
    assert_compiles_and_runs(
        r#"
        fun main() {
            let stored: Option<|| void> = Some(|| print("inner"));
            if stored is Some(let handler) {
                handler();
            }
        }
        "#,
        "inner\n",
    );
}

#[test]
fn a_closure_payload_capture_is_callable_from_a_closure_in_the_arm() {
    // The owner's shape: the call is CAPTURED by a closure created in the
    // arm and handed off. The alias has to survive into the closure body.
    assert_compiles_and_runs(
        r#"
        fun run(callback: || void) {
            callback();
        }

        fun main() {
            let stored: Option<|| void> = Some(|| print("inner"));
            if stored is Some(let handler) {
                run(|| {
                    handler();
                });
            }
        }
        "#,
        "inner\n",
    );
}

#[test]
fn a_closure_payload_capture_is_callable_two_closures_deep() {
    // Two nested closure boundaries between the capture and its call.
    assert_compiles_and_runs(
        r#"
        fun run(callback: || void) {
            callback();
        }

        fun main() {
            let stored: Option<|| void> = Some(|| print("inner"));
            if stored is Some(let handler) {
                run(|| {
                    run(|| {
                        handler();
                    });
                });
            }
        }
        "#,
        "inner\n",
    );
}

#[test]
fn a_closure_payload_capture_takes_arguments_and_returns() {
    // Not just a bare `f()`: arguments pass and the result is used, so the
    // alias has to hold in operand position too.
    assert_compiles_and_runs(
        r#"
        fun main() {
            let stored: Option<|i32| i32> = Some(|n| n * 2);
            if stored is Some(let double) {
                print(double(20) + 1);
            }
        }
        "#,
        "41\n",
    );
}

#[test]
fn a_closure_payload_capture_shadowing_an_outer_binding_calls_the_captured_one() {
    // The program as first reported, whose shadowing was a red herring: it
    // fails and is fixed for the same reason the unshadowed one is, and the
    // capture — not the outer binding — is what runs.
    assert_compiles_and_runs(
        r#"
        fun run(callback: || void) {
            callback();
        }

        fun main() {
            let handler = || print("outer");
            let stored: Option<|| void> = Some(|| print("inner"));
            if stored is Some(let handler) {
                run(|| {
                    handler();
                });
            }
        }
        "#,
        "inner\n",
    );
}

#[test]
fn an_i32_payload_capture_control_still_reads_through_the_alias() {
    // The control that always worked: a payload whose uses are READS took the
    // value arm, which consulted the alias table all along.
    assert_compiles_and_runs(
        r#"
        fun main() {
            let stored: Option<i32> = Some(41);
            if stored is Some(let n) {
                print(n + 1);
            }
        }
        "#,
        "42\n",
    );
}

#[test]
fn the_match_control_still_calls_its_closure_payload() {
    // The other control that always worked, and by a different mechanism:
    // a `match` leg DECLARES its captures as `const`s, so the callee had a
    // declaration to refer to. Untouched by the fix, and pinned so it stays
    // that way.
    assert_compiles_and_runs(
        r#"
        fun main() {
            let stored: Option<|| void> = Some(|| print("inner"));
            match stored {
                Some(let f) => f(),
                None => print("none"),
            }
        }
        "#,
        "inner\n",
    );
}

#[test]
fn a_struct_payload_capture_control_still_reads_its_fields() {
    // The shape kolt's servers run in production: a struct payload, read
    // through its fields rather than called. It must not move.
    assert_compiles_and_runs(
        r#"
        struct User { name: str, age: i32 }

        fun main() {
            let stored: Option<User> = Some(User { name = "ada", age = 36 });
            if stored is Some(let user) {
                print(user.name);
                print(user.age);
            }
        }
        "#,
        "ada\n36\n",
    );
}

#[test]
fn a_closure_payload_capture_in_an_else_if_chain_calls_the_right_arm() {
    // Two `is` tests in one chain: each arm's capture must alias ITS own
    // subject's slot, not the previous test's.
    assert_compiles_and_runs(
        r#"
        fun main() {
            let first: Option<|| void> = None;
            let second: Option<|| void> = Some(|| print("second"));
            if first is Some(let a) {
                a();
            } else if second is Some(let b) {
                b();
            } else {
                print("neither");
            }
        }
        "#,
        "second\n",
    );
}

// --- B175: `Type::Trait` comes OFF the operator check's skip list ------------
//
// B170 routed every left-operand SHAPE through the check and then had to carve
// one back out: `Type::Trait`. std's `List<T: Add + Default>::sum` wrote `mut
// total = T::default()`, the bound-path `T::default()` inferred as the BOUND,
// and judging a trait-typed left operand would have refused std's own
// `sum`/`product` over an inference wart in a different subsystem. B175 fixed
// the wart (`traits::b175_*`), so the carve-out goes, and with it the hole:
// a value typed as a bare trait now gets a verdict like every other shape.

#[test]
fn b175_a_bare_trait_left_operand_of_addition_is_rejected() {
    // The hole B170 left open, entered the only way a bare trait value can be
    // built now that all six DECLARATION positions refuse one (B4 §12.2): the
    // return of a trait's own associated function, whose `Self` legitimately
    // stays abstract on the `Trait::func` path (B162). Pre-fix this compiled —
    // the check `continue`d before ever looking for an impl — and emitted the
    // host's `+`.
    assert_fails_spanning(
        r#"
        import std::io::panic;

        trait Maker {
            fun make(): Self { panic("no default") }
        }

        fun main() {
            let m = Maker::make();
            let _sum = m + 1;
        }
        "#,
        "m + 1",
        "this operand is the bare trait `Maker`",
    );
}

#[test]
fn b175_a_bare_trait_left_operand_is_refused_without_impl_advice() {
    // The B170 rule about WHICH refusal, applied to the shape B170 skipped: a
    // tuple and an array can act on "add `impl (i32, i32) with PartialEq`"
    // because such an impl resolves; `impl Maker with PartialEq` is not a
    // declaration the language has, so a bare trait must get the reason
    // instead — a trait is a bound, not a type — and the steer that does work.
    let source = r#"
        import std::io::panic;

        trait Maker {
            fun make(): Self { panic("no default") }
        }

        fun main() {
            let _same = Maker::make() == Maker::make();
        }
        "#;
    assert_fails_with(source, "a trait is a bound, not a value type");
    assert_fails_with(source, "(`<T: Maker>`)");
    assert_fails_without(source, "add `impl Maker with PartialEq`");
}

#[test]
fn b175_the_sibling_operators_refuse_a_bare_trait_too() {
    // The carve-out gated the whole loop, not one operator, so every operator
    // modelling a trait skipped its refusal for this shape.
    for (operator, trait_name) in [("<", "PartialOrd"), ("-", "Sub"), ("*", "Mul")] {
        assert_fails_with(
            &format!(
                r#"
                import std::io::panic;

                trait Maker {{
                    fun make(): Self {{ panic("no default") }}
                }}

                fun main() {{
                    let _result = Maker::make() {operator} Maker::make();
                }}
                "#
            ),
            &format!("models `{trait_name}`, and this operand is the bare trait `Maker`"),
        );
    }
}

// --- B193: a trait default's `self <op> self` ---------------------------------
//
// The one trait-typed shape B175 left skipping, and the answer was never a
// refusal: `self + self` in a default over a supertrait `Add` is exactly the
// program declaring the supertrait is FOR. It miscompiled because nothing
// dispatched it — skipping the operator check kept the anything-goes native
// emission, and over two lowered structs the host's operators are garbage:
//
//   Money { cents = 21 }.twice()   →  `[21] + [21]` is the string "2121",
//                                     slot 0 of it is "2", so `.cents`
//                                     printed 2. A plausible wrong answer.
//   Money { cents = 21 }.zero()    →  `[21] - [21]` is NaN; `.cents` printed
//                                     `undefined`.
//   Money { cents = 3 }.square()   →  `[3] * [3]`, `undefined` likewise.
//   Tag { id = 1 }.same()          →  `self === self` — a reference compare
//                                     that ignored the impl, so a `PartialEq`
//                                     whose `eq` answers `false` still
//                                     printed `true`.
//
// One fix, at the dispatch: a default body's operand dispatches on the type
// the default is being SPECIALIZED for (`GenericDispatch::OnType(None, ..)`,
// read against `current_self_type` at emission — the same channel a `self`
// CALL in a default body has used since B55), and the analyzer stops skipping
// the shape.

#[test]
fn a_trait_defaults_self_operand_dispatches_to_the_specialized_type() {
    // The pin B193 was filed as. Pre-fix it printed `2`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Doubler with Add {
            fun twice(self): Self { self + self }
        }

        struct Money { cents: i32 }

        impl Money with Add {
            fun add(self, other: Money): Money { Money { cents = self.cents + other.cents } }
        }

        impl Money with Doubler {}

        fun main() {
            print(Money { cents = 21 }.twice().cents);
        }
        "#,
        "42\n",
    );
}

#[test]
fn b193_a_trait_defaults_self_subtraction_dispatches_too() {
    // Audit run 7 widened the item off `+`: `-` over the same pair is NaN, so
    // this one printed `undefined` rather than a plausible number.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Sub;

        trait Zeroer with Sub {
            fun zero(self): Self { self - self }
        }

        struct Money { cents: i32 }

        impl Money with Sub {
            fun sub(self, other: Money): Money { Money { cents = self.cents - other.cents } }
        }

        impl Money with Zeroer {}

        fun main() {
            print(Money { cents = 21 }.zero().cents);
        }
        "#,
        "0\n",
    );
}

#[test]
fn b193_a_trait_defaults_self_multiplication_dispatches_too() {
    // Pre-fix: `undefined`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Mul;

        trait Squarer with Mul {
            fun square(self): Self { self * self }
        }

        struct Money { cents: i32 }

        impl Money with Mul {
            fun mul(self, other: Money): Money { Money { cents = self.cents * other.cents } }
        }

        impl Money with Squarer {}

        fun main() {
            print(Money { cents = 3 }.square().cents);
        }
        "#,
        "9\n",
    );
}

#[test]
fn b193_a_trait_defaults_self_equality_dispatches_to_the_impl() {
    // `==` needs an impl that DISAGREES with the host to witness anything:
    // `self === self` is true for the same value whatever the impl says, so a
    // conventional `eq` would have hidden the defect. This one answers
    // `false`, and pre-fix the program printed `true` — the emission was
    // `self === self`, the impl never called.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::PartialEq;

        trait Selfsame with PartialEq {
            fun same(self): bool { self == self }
        }

        struct Tag { id: i32 }

        impl Tag with PartialEq {
            fun eq(self, other: Tag): bool { false }
        }

        impl Tag with Selfsame {}

        fun main() {
            print(Tag { id = 1 }.same());
        }
        "#,
        "false\n",
    );
}

#[test]
fn b193_a_trait_default_dispatches_at_each_specialization() {
    // The point of dispatching on the SPECIALIZED type rather than on
    // anything the default itself knows: one default body, two impls, two
    // answers. The native specialization keeps native JS, which is what the
    // emitter's own `compares_natively` guard is for — dispatching a native
    // back into std's `impl i32 with Add` would recurse forever.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        trait Doubler with Add {
            fun twice(self): Self { self + self }
        }

        struct Money { cents: i32 }
        struct Steps { count: i32 }

        impl Money with Add {
            fun add(self, other: Money): Money { Money { cents = self.cents + other.cents } }
        }

        impl Steps with Add {
            // Deliberately not a plain sum, so the dispatch is visible.
            fun add(self, other: Steps): Steps { Steps { count = self.count + other.count + 1 } }
        }

        impl Money with Doubler {}
        impl Steps with Doubler {}

        fun main() {
            print(Money { cents = 21 }.twice().cents);
            print(Steps { count = 21 }.twice().count);
        }
        "#,
        "42\n43\n",
    );
}

#[test]
fn b193_a_trait_default_operator_its_trait_never_promised_is_refused() {
    // The other half of no longer skipping: a default body may now be JUDGED,
    // and `self + self` in a trait with no `Add` supertrait is a real error —
    // every specialization would reach the host's `+` over a lowered value.
    // It gets its own sentence rather than B175's bare-trait one, whose steer
    // ("hold the value in a generic bounded by the trait") is nonsense inside
    // the trait's own body: the declaration that works is a supertrait.
    assert_fails_with(
        r#"
        trait Doubler {
            fun twice(self): Self { self + self }
        }

        struct Money { cents: i32 }

        impl Money with Doubler {}

        fun main() {
            print(Money { cents = 21 }.twice().cents);
        }
        "#,
        "Declare it as a supertrait (`trait Doubler with Add`)",
    );
}

// --- B197: an operator trait's method is required at impl time ---------------
//
// Audit run 7's F12, RULED. `std::operators`'s ten traits carry
// `panic("not implemented yet")` bodies — deliberately, so the declarations
// type-check (`ret-checking.md`) — and a body is a body, so the conformance
// check's "a default is inherited" rule let `impl P with Add { }` through.
//
// Pre-fix, that program:
//
//   vilan check   →  `no errors`.
//   vilan run     →  an uncaught `not implemented yet` from node, with no
//                    type, no method and no span — the one diagnostic in the
//                    surface that names nothing at all.
//
// while the refusal a type with NO impl gets reads "add `impl P with Add`
// providing `add`": advice this program had followed to the letter, minus the
// providing half.
//
// The ruling: the method is REQUIRED at the impl. The panicking bodies stay
// (they are what the compound-assignment derivation reads, and `+=` still
// derives from `+`), but there is no coherent program in which an operator
// impl omits its method, because the default's only behaviour is to throw.

#[test]
fn b197_an_operator_impl_with_no_method_is_refused() {
    // The exhibit the item was filed on. Pre-fix: `check` clean, `run`
    // throwing `not implemented yet`.
    assert_fails_with(
        r#"
        import std::operators::Add;

        struct P { n: i32 }

        impl P with Add { }

        fun main() {
            let sum = P { n = 1 } + P { n = 2 };
            print(sum.n);
        }
        "#,
        "`impl P with Add` provides no `add`",
    );
}

#[test]
fn b197_the_refusal_names_the_type_and_the_signature_to_write() {
    // The least the item asked for, which the ruling gets for free: the
    // runtime panic named nothing, and this names the type, the method, the
    // reason the default exists, and the exact signature. The signature is
    // rendered here rather than read off the trait, because the trait's own
    // `b: B` renders as `b: Add` — not a signature anyone can write.
    let source = r#"
        import std::operators::Mul;

        struct Money { cents: i32 }

        impl Money with Mul { }

        fun main() {}
        "#;
    assert_fails_with(source, "Declare `fun mul(self, b: Money): Money`");
    assert_fails_with(source, "it exists so `*=` can derive from `*`");
    assert_fails_without(source, "b: Mul");
}

#[test]
fn b197_a_declared_operand_type_is_named_in_the_signature() {
    // `impl Meters with Add<Feet>` declares its own `B`, so the signature the
    // refusal names is not the `Self`-defaulted one.
    assert_fails_with(
        r#"
        import std::operators::Add;

        struct Meters { m: i32 }
        struct Feet { f: i32 }

        impl Meters with Add<Feet> { }

        fun main() {}
        "#,
        "Declare `fun add(self, b: Feet): Meters`",
    );
}

#[test]
fn b197_the_requirement_reaches_through_a_supertrait() {
    // Reached through `trait Doubler with Add`, the requirement comes from a
    // trait the impl does not name — so the sentence says whose it is, and
    // names the other way to satisfy it.
    let source = r#"
        import std::operators::Add;

        trait Doubler with Add {
            fun twice(self): Self { self + self }
        }

        struct Money { cents: i32 }

        impl Money with Doubler {}

        fun main() {}
        "#;
    assert_fails_with(source, "(`Doubler` requires `Add`) provides no `add`");
    assert_fails_with(source, "in an `impl Money with Add` of its own");
}

#[test]
fn b197_a_separate_impl_of_the_operator_trait_satisfies_it() {
    // And it does satisfy it: the conformance check's existing
    // provided-elsewhere rule covers the operator family unchanged, which is
    // what B193's own programs rely on.
    assert_compiles(
        r#"
        import std::operators::Add;

        trait Doubler with Add {
            fun twice(self): Self { self + self }
        }

        struct Money { cents: i32 }

        impl Money with Add {
            fun add(self, other: Money): Money { Money { cents = self.cents + other.cents } }
        }

        impl Money with Doubler {}

        fun main() {}
        "#,
    );
}

#[test]
fn b197_every_operator_trait_requires_its_own_method() {
    // Per case, not per example: the item is one rule over ten traits, and a
    // rule pinned at one of them is a rule pinned nowhere.
    for (trait_name, method, symbol) in [
        ("Add", "add", "+"),
        ("Sub", "sub", "-"),
        ("Mul", "mul", "*"),
        ("Div", "div", "/"),
        ("Rem", "rem", "%"),
        ("Shl", "shl", "<<"),
        ("Shr", "shr", ">>"),
        ("BitAnd", "bit_and", "&"),
        ("BitXor", "bit_xor", "^"),
        ("BitOr", "bit_or", "|"),
    ] {
        let source = format!(
            r#"
            import std::operators::{trait_name};

            struct P {{ n: i32 }}

            impl P with {trait_name} {{ }}

            fun main() {{}}
            "#
        );
        assert_fails_with(
            &source,
            &format!("`impl P with {trait_name}` provides no `{method}`"),
        );
        assert_fails_with(
            &source,
            &format!("so `{symbol}=` can derive from `{symbol}`"),
        );
    }
}

#[test]
fn b197_an_operator_impl_that_writes_its_method_still_runs() {
    // The control the requirement must not break.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        struct P { n: i32 }

        impl P with Add {
            fun add(self, other: P): P { P { n = self.n + other.n } }
        }

        fun main() {
            print((P { n = 1 } + P { n = 2 }).n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn b197_the_compound_form_still_derives_from_the_operator() {
    // The reason the panicking defaults stay, pinned so a later lane cannot
    // delete them and call the suite green: `+=` derives from `+`.
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::operators::Add;

        struct P { n: i32 }

        impl P with Add {
            fun add(self, other: P): P { P { n = self.n + other.n } }
        }

        fun main() {
            mut total = P { n = 1 };
            total += P { n = 2 };
            print(total.n);
        }
        "#,
        "3\n",
    );
}

#[test]
fn b197_a_non_operator_traits_default_is_still_inherited() {
    // The rule is the operator family's, not "every default is now required":
    // an ordinary trait's default body is inherited exactly as before, and so
    // is a non-operator default of an operator trait's own supertrait chain
    // (`PartialOrd`'s `lt`/`le`/`gt`/`ge` over `partial_compare`).
    assert_compiles_and_runs(
        r#"
        import std::io::print;
        import std::compare::{ PartialEq, PartialOrd, Ordering };
        import std::option::{ Option, Some };

        trait Greeter {
            fun greet(self): str { "hello" }
        }

        struct Meters { m: i32 }

        impl Meters with Greeter {}

        impl Meters with PartialEq {
            fun eq(self, other: Meters): bool { self.m == other.m }
        }

        impl Meters with PartialOrd {
            fun partial_compare(self, other: Meters): Option<Ordering> {
                if self.m < other.m {
                    Some(Ordering::Less)
                } else {
                    if self.m > other.m { Some(Ordering::Greater) } else { Some(Ordering::Equal) }
                }
            }
        }

        fun main() {
            print(Meters { m = 1 }.greet());
            print(Meters { m = 1 } < Meters { m = 2 });
        }
        "#,
        "hello\ntrue\n",
    );
}

// --- B181: `&&`/`||` accepted a generic RIGHT operand and emitted the value --
//
// The same membership principle B179 settled for the native family, on the two
// operators that model no trait at all. `grounded` excludes every
// `Type::Generic`, so a parameter reached neither the `bool` check nor a
// refusal, and `compare_type` would have admitted it anyway — a parameter
// compares equal to whatever is asked of it. `fun both<T>(flag: bool, value: T):
// bool { flag && value }` compiled, and `both(true, Point { x = 1, y = 2 })`
// printed `[ 1, 2 ]`: JS's `&&` yields the RIGHT operand when the left is
// truthy, so the struct itself came back, typed `bool`.
//
// No bound rescues it. `&&` admits `bool` and nothing else, no trait names that
// set, and — unlike `+`, where a `str` left operand's admitted set IS
// trait-characterizable (B176's render bound) — there is not even an operator
// trait to consult. So every generic right operand refuses, whatever its bound.
//
// The LEFT half was B174's deferral shape and went with it: same check, side
// condition dropped, same sentence — the reason a bound cannot prove `bool`
// never depended on which operand was being judged.

#[test]
fn b181_an_unbounded_generic_right_operand_of_and_is_rejected() {
    // The pin B181 was filed as. Pre-fix this program compiled and PRINTED the
    // struct.
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }

        fun both<T>(flag: bool, value: T): bool {
            flag && value
        }

        fun main() {
            print(both(true, Point { x = 1, y = 2 }));
        }
        "#,
        "flag && value",
        "`&&` takes `bool` operands, and `T` is a type parameter",
    );
}

#[test]
fn b181_a_bounded_generic_right_operand_of_or_is_rejected_too() {
    // The bound is IRRELEVANT here, which is the ruling and therefore the pin:
    // the refusal must not steer the author to add one, because no bound can
    // make a parameter BE `bool`. `||` shares the arm, so it shares the rule.
    assert_fails_spanning(
        r#"
        import std::operators::Add;

        fun either<T: Add>(flag: bool, value: T): bool {
            flag || value
        }

        fun main() {
            print(either(false, 1));
        }
        "#,
        "flag || value",
        "no bound on `T` can prove membership",
    );
}

#[test]
fn b181_the_generic_right_operand_refusal_names_the_spelling_that_works() {
    // A refusal is worth what the reader can do with it: the value has to
    // become a `bool` before the operator sees it.
    assert_fails_with(
        r#"
        fun both<T>(flag: bool, value: T): bool {
            flag && value
        }

        fun main() {
            print(both(true, 1));
        }
        "#,
        "Test the value and combine the `bool`s, or declare this operand `bool`",
    );
}

#[test]
fn b181_a_bool_right_operand_still_short_circuits() {
    // The escape hatch the refusal steers to has to work, or the rule would
    // have no legal spelling — and the short circuit itself must survive: the
    // right operand is not evaluated when the left already decides.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun loud(): bool {
            print("evaluated");
            true
        }

        fun both<T>(flag: bool, value: T, ready: bool): bool {
            flag && ready
        }

        fun main() {
            print(both(true, 7, true));
            print(false && loud());
        }
        "#,
        "true\nfalse\n",
    );
}

#[test]
fn an_unbounded_generic_left_operand_of_and_is_rejected() {
    // B174 took the left half. `both(Point { x = 1, y = 2 }, true)` printed the
    // struct: the host's `&&` finds it truthy and yields the RIGHT operand,
    // which is then typed `bool`. B181 left this to the breaking step; the
    // wording it shipped already reads for either side, because the reason is
    // the same one — `bool`'s set is `bool`, and no bound can prove membership
    // of it.
    assert_fails_with(
        r#"
        struct Point { x: i32, y: i32 }

        fun both<T>(value: T, flag: bool): bool {
            value && flag
        }

        fun main() {
            print(both(Point { x = 1, y = 2 }, true));
        }
        "#,
        "takes `bool` operands",
    );
}

// --- B196: every native operator, not just `+`, when the LEFT operand is not
//     a number ---------------------------------------------------------------
//
// Audit run 7's F1, and a RELEASED miscompile: shipped in v0.40.0. b148 gated
// its native-operand check on `+` and argued the gate in a SCOPE note — "the
// other operators emit arithmetic on numbers, where `+` emits a rendering".
// That is a statement about a NUMERIC left operand. Three native types are not
// numbers, and for those the argument inverts: the host operator returns a
// number, a binary takes its static type from the LEFT operand, so the result
// is a number wearing a type it is not. Fifty-five wrong-running squares of the
// audit's 216-program operator matrix, one root.
//
// The pre-fix runs, recorded here because a green pin proves only that the
// program is refused NOW:
//
//   let c: bool = true - 3         →  -2. `if c` took the TRUE branch and
//                                     `c == true` printed false: a `bool`
//                                     that is neither value.
//   let c: bool = true & 3         →  1, printed as `1` by an i-string hole
//                                     typed `bool`.
//   let s: str = "12" - "3"        →  9, and `s.len()` was `undefined`.
//   let s: str = "12" << 2         →  48.
//   mut c: bool = true; c -= 3     →  -2, the compound form inheriting it
//                                     through the desugar.
//   mut s: str = "12"; s *= 3      →  36.
//   Level::High - Level::Low       →  4 typed `Level`; the `match` on it
//                                     panicked, "Level: 4 is not one of its
//                                     values".
//   Level::High ^ Level::Low       →  4 again, and SILENT: `== Level::Low`
//                                     and `== Level::High` both printed
//                                     false, a `Level` matching no variant.
//
// The carve-out is "the left operand is a number", never "the operator is not
// `+`": `f64 * i32` computes a correct answer of the declared type, and
// refusing it is the numeric-strictness change with an `as_f64()` migration
// that b148's SCOPE note deferred. It stays deferred, and is pinned below as a
// control so a later lane cannot take it by accident.
//
// The COMPARISONS need nothing: `bool` has no ordering (B24 refuses `<` on it),
// a string backing is not an order (§3.6 refuses that), and `str` and an
// integer backing both order correctly. Only the wrong-running squares close.

#[test]
fn b196_a_bool_left_operand_of_subtraction_is_rejected() {
    // The exhibit the item was filed on. Pre-fix: `-2`, truthy, `== true`
    // false.
    assert_fails_spanning(
        r#"
        fun main() {
            let c: bool = true - 3;
            print(c);
        }
        "#,
        "true - 3",
        "`-` on `bool` has no meaning",
    );
}

#[test]
fn b196_a_bool_left_operand_of_a_bitwise_operator_is_rejected() {
    // The quietest of the family: `true & 3` is `1`, which prints as `1` and
    // never looks like a `bool` going wrong until something compares it.
    assert_fails_spanning(
        r#"
        fun main() {
            let c: bool = true & 3;
            print(c);
        }
        "#,
        "true & 3",
        "`&` on `bool` has no meaning",
    );
}

#[test]
fn b196_a_str_left_operand_of_subtraction_is_rejected() {
    // Pre-fix: `9`, typed `str`, with `.len()` undefined on it.
    assert_fails_spanning(
        r#"
        fun main() {
            let s: str = "12" - "3";
            print(s);
        }
        "#,
        r#""12" - "3""#,
        "`-` on `str` has no meaning",
    );
}

#[test]
fn b196_a_str_left_operand_of_a_shift_is_rejected() {
    // Pre-fix: `48`.
    assert_fails_spanning(
        r#"
        fun main() {
            let s: str = "12" << 2;
            print(s);
        }
        "#,
        r#""12" << 2"#,
        "`<<` on `str` has no meaning",
    );
}

#[test]
fn b196_a_backed_enum_left_operand_of_subtraction_is_rejected() {
    // Pre-fix: a `Level` holding `4`, on which the `match` panicked — the one
    // arm of the family a runtime guard happened to catch.
    assert_fails_spanning(
        r#"
        enum Level { Low = 1, High = 5 }

        fun main() {
            let level: Level = Level::High - Level::Low;
            print(level == Level::Low);
        }
        "#,
        "Level::High - Level::Low",
        "`-` on `Level` has no meaning",
    );
}

#[test]
fn b196_a_backed_enum_left_operand_of_a_bitwise_operator_is_rejected() {
    // The same value with no guard in front of it: pre-fix both comparisons
    // printed false, a `Level` that is no variant at all.
    assert_fails_spanning(
        r#"
        enum Level { Low = 1, High = 5 }

        fun main() {
            let level: Level = Level::High ^ Level::Low;
            print(level == Level::Low);
        }
        "#,
        "Level::High ^ Level::Low",
        "`^` on `Level` has no meaning",
    );
}

#[test]
fn b196_the_compound_forms_inherit_the_refusal() {
    // `x -= y` desugars to `x = x - y` and reaches the same check, so the
    // whole compound family closes with the binary one. Pre-fix `c -= 3` left
    // `-2` in a `bool` and `s *= 3` left `36` in a `str`.
    assert_fails_spanning(
        r#"
        fun main() {
            mut c: bool = true;
            c -= 3;
            print(c);
        }
        "#,
        "c -= 3",
        "`-` on `bool` has no meaning",
    );
    assert_fails_spanning(
        r#"
        fun main() {
            mut s: str = "12";
            s *= 3;
            print(s);
        }
        "#,
        "s *= 3",
        "`*` on `str` has no meaning",
    );
}

#[test]
fn b196_every_arithmetic_and_bitwise_operator_closes_on_every_non_numeric_left() {
    // Per case, not per example: the rule is the left operand's SHAPE against
    // the whole nine-operator family, so all twenty-seven squares are held.
    for operator in ["-", "*", "/", "%", "&", "|", "^", "<<", ">>"] {
        assert_fails_with(
            &format!(
                r#"
                fun main() {{
                    let flag = true;
                    print(flag {operator} 3);
                }}
                "#
            ),
            &format!("`{operator}` on `bool` has no meaning"),
        );
        assert_fails_with(
            &format!(
                r#"
                fun main() {{
                    let text = "12";
                    print(text {operator} 3);
                }}
                "#
            ),
            &format!("`{operator}` on `str` has no meaning"),
        );
        assert_fails_with(
            &format!(
                r#"
                enum Level {{ Low = 1, High = 5 }}

                fun main() {{
                    print(Level::High {operator} Level::Low);
                }}
                "#
            ),
            &format!("`{operator}` on `Level` has no meaning"),
        );
    }
}

#[test]
fn b196_the_refusal_names_the_admitted_set_of_the_left_operand() {
    // The operand-role wording (row 345/353's family): a refusal that only
    // says "not this one" leaves the reader to guess which ones are, so each
    // left type names its own admitted set — and a STRING backing names a
    // narrower one, because §3.6 refuses its ordering too.
    assert_fails_with(
        r#"
        fun main() {
            print(true - 3);
        }
        "#,
        "`bool`'s admitted operators are `== != && || !`",
    );
    assert_fails_with(
        r#"
        fun main() {
            print("12" - 3);
        }
        "#,
        "`str`'s admitted operators are `+ == != < <= > >=`",
    );
    assert_fails_with(
        r#"
        enum Level { Low = 1, High = 5 }

        fun main() {
            print(Level::High - Level::Low);
        }
        "#,
        "`Level`'s admitted operators are `== != < <= > >=`",
    );
    assert_fails_with(
        r#"
        enum Size { Small = "sm", Large = "lg" }

        fun main() {
            print(Size::Large - Size::Small);
        }
        "#,
        "`Size`'s admitted operators are `== !=`",
    );
}

#[test]
fn b196_the_refusal_steers_to_the_spelling_that_works() {
    // A refusal is worth what the reader can do with it, and the three shapes
    // want three different things: a bitwise operator on a `bool` is nearly
    // always the logical one mistyped, a `str` wants parsing (or `.repeat`),
    // and a backing is not a number to compute with at all.
    assert_fails_with(
        r#"
        fun main() {
            print(true & false);
        }
        "#,
        "`&&` is `bool`'s conjunction",
    );
    assert_fails_with(
        r#"
        fun main() {
            print(true ^ false);
        }
        "#,
        "`!=` is `bool`'s exclusive or",
    );
    assert_fails_with(
        r#"
        fun main() {
            print("ab" * 3);
        }
        "#,
        "A `str` repeats with `.repeat(n)`",
    );
    assert_fails_with(
        r#"
        fun main() {
            print("12" - 3);
        }
        "#,
        "Parse the text first (`.parse_i32()`, `.parse_f64()`)",
    );
    assert_fails_with(
        r#"
        enum Level { Low = 1, High = 5 }

        fun main() {
            print(Level::High / Level::Low);
        }
        "#,
        "match on the variant, or hold the number you mean",
    );
}

#[test]
fn b196_the_steers_the_refusals_name_all_compile() {
    // Each escape hatch has to work, or the rule would have no legal spelling.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Level { Low = 1, High = 5 }

        fun main() {
            let flag = true;
            let as_number: i32 = if flag { 1 } else { 0 };
            print(as_number - 3);
            print(true && false);
            print(true != false);
            print("ab".repeat(3));
            print("12".parse_i32().unwrap_or(0) - 3);
            let rank: i32 = match Level::High { Level::Low => 1, Level::High => 5 };
            print(rank - 1);
        }
        "#,
        "-2\nfalse\ntrue\nababab\n9\n4\n",
    );
}

#[test]
fn b196_the_numeric_carve_out_is_untouched() {
    // b148's SCOPE note deferred `f64 * i32` — two GROUNDED numbers computing
    // a correct answer of the declared type — and B196 is not that change.
    // The whole nine-operator family stays admitted on a numeric left operand,
    // mixed widths included.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            let scale: f64 = 2.5;
            let count: i32 = 2;
            print(scale * count);
            print(scale - count);
            print(7 % 4);
            print(6 & 3);
            print(1 << 3);
        }
        "#,
        "5\n0.5\n3\n2\n8\n",
    );
}

#[test]
fn b196_the_admitted_operators_of_each_left_type_still_run() {
    // The controls. Every operator each refusal NAMES as admitted has to keep
    // working, or the rule would have eaten more than the bug.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        enum Level { Low = 1, High = 5 }
        enum Size { Small = "sm", Large = "lg" }

        fun main() {
            print("a" + "b");
            print("a" == "a");
            print("a" != "b");
            print("a" < "b");
            print(true == false);
            print(true && false);
            print(true || false);
            print(!true);
            print(Level::Low < Level::High);
            print(Level::Low == Level::Low);
            print(Size::Small == Size::Large);
        }
        "#,
        "ab\ntrue\ntrue\ntrue\nfalse\nfalse\ntrue\nfalse\ntrue\ntrue\nfalse\n",
    );
}

// --- B200: the unary operators' operands ------------------------------------
//
// B196's own find, closed here. The binary loop above reads
// `prepped_binary_ops`; a unary was typed somewhere else entirely
// (`Expr::Unary` in `infer_type_inner`, which simply returns the operand's
// type) and reached no operand rule at all. Same defect, same family, one
// operand to blame instead of two — and off the native path it is worse than
// the binary case, because `-` on an aggregate is `NaN` rather than a
// plausible wrong number.
//
// The pre-fix runs, recorded because a green pin proves only that the program
// is refused NOW:
//
//   let flipped: bool = -true      →  -1. `== true` printed false and
//                                      `== false` printed false: a `bool`
//                                      that is neither value, exactly as
//                                      `true - 3` was.
//   let value: str = -"12"         →  -12, typed `str`.
//   let level: Level = -Level::High →  -5 typed `Level`; `== Level::High` and
//                                      `== Level::Low` both printed false.
//   let p = -Point { x = 1, y = 2 } →  the host's `-[1, 2]`, `NaN`, and
//                                      `p.x` printed `undefined`.
//   fun negate<T: Sub>(v: T): T { -v } → compiled; `negate(5)` printed `-5`
//                                      and `negate(Point { … }).x` printed
//                                      `undefined`.
//   print(!5) / print(!"hi") /
//   print(!Point { x = 1, y = 2 }) →  false, false, false — the host's
//                                      truthiness test, never the question
//                                      the author asked.
//
// The admitted sets are stated, not read off an impl: vilan has no `Neg` and
// no `Not` trait, so nothing here ever dispatches, and `-` admits the numeric
// primitives while `!` admits `bool`. A type PARAMETER is refused for B179's
// reason — no trait names either set, so no bound can prove membership.

#[test]
fn b196_a_unary_minus_on_a_non_numeric_operand_is_rejected() {
    // The pin B200 was filed as (its `#[ignore]` reason named B200), kept
    // under its found-as name. Pre-fix: `-1`, equal to neither `true` nor
    // `false`.
    assert_fails_spanning(
        r#"
        fun main() {
            let flipped: bool = -true;
            print(flipped);
        }
        "#,
        "-true",
        "`-` on `bool` has no meaning",
    );
}

#[test]
fn b200_a_unary_minus_on_a_str_is_rejected() {
    // Pre-fix: `-12`, typed `str`.
    assert_fails_spanning(
        r#"
        fun main() {
            let value: str = -"12";
            print(value);
        }
        "#,
        r#"-"12""#,
        "`-` on `str` has no meaning",
    );
}

#[test]
fn b200_a_unary_minus_on_a_backed_enum_is_rejected() {
    // Pre-fix: `-5` typed `Level`, matching neither variant — the silent
    // shape, exactly as `Level::High ^ Level::Low` was.
    assert_fails_spanning(
        r#"
        enum Level { Low = 1, High = 5 }

        fun main() {
            let level: Level = -Level::High;
            print(level == Level::High);
        }
        "#,
        "-Level::High",
        "`-` on `Level` has no meaning",
    );
}

#[test]
fn b200_a_unary_minus_on_a_struct_is_rejected() {
    // The shape the binary family never had: no native coercion produces even
    // a plausible number. Pre-fix this compiled and `p.x` printed `undefined`.
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
            let p = -Point { x = 1, y = 2 };
            print(p.x);
        }
        "#,
        "-Point { x = 1, y = 2 }",
        "vilan has no `Neg` trait",
    );
}

#[test]
fn b200_a_unary_minus_on_a_bounded_generic_is_rejected() {
    // B179's rule at the unary site: the bound is IRRELEVANT, because no
    // trait names the numeric set. Pre-fix `negate(5)` printed `-5` and
    // `negate(Point { x = 1, y = 2 }).x` printed `undefined` — the same
    // declaration, correct for one instantiation and garbage for the other.
    assert_fails_spanning(
        r#"
        import std::operators::Sub;

        fun negate<T: Sub>(value: T): T {
            -value
        }

        fun main() {
            print(negate(5));
        }
        "#,
        "-value",
        "no bound on `T` can prove membership",
    );
}

#[test]
fn b200_a_unary_minus_on_an_unbounded_generic_is_rejected() {
    // The unbounded half gets the same sentence for the same reason: a bound
    // could not have rescued it either.
    assert_fails_spanning(
        r#"
        fun negate<T>(value: T): T {
            -value
        }

        fun main() {
            print(negate(5));
        }
        "#,
        "-value",
        "no bound on `T` can prove membership",
    );
}

#[test]
fn b200_a_unary_minus_on_void_is_rejected() {
    // B170's rule on the unary side: the refusal must be one the reader can
    // act on, and `void` has no number inside it to negate.
    assert_fails_with(
        r#"
        fun nothing() {}

        fun main() {
            print(-nothing());
        }
        "#,
        "this operand is `void`",
    );
}

#[test]
fn b200_a_bang_on_a_number_is_rejected() {
    // The twin. `!`'s RESULT was always typed `bool`, so nothing wore a type
    // it was not — the defect is that the host's `!` admits every value, so
    // `!5` compiled to `false` and the author's question was never asked.
    assert_fails_spanning(
        r#"
        fun main() {
            print(!5);
        }
        "#,
        "!5",
        "`!` negates a `bool`, and this operand is `i32`",
    );
}

#[test]
fn b200_a_bang_on_a_str_is_rejected() {
    // Pre-fix: `false`. The emptiness test the author plausibly meant is
    // `.is_empty()`, which the refusal names.
    assert_fails_spanning(
        r#"
        fun main() {
            print(!"hi");
        }
        "#,
        r#"!"hi""#,
        "`!` negates a `bool`, and this operand is `str`",
    );
}

#[test]
fn b200_a_bang_on_a_struct_is_rejected() {
    // Pre-fix: `false`, and it would have been `false` for every struct ever
    // written — an aggregate lowers to an array, and an array is always
    // truthy.
    assert_fails_spanning(
        r#"
        struct Point { x: i32, y: i32 }

        fun main() {
            print(!Point { x = 1, y = 2 });
        }
        "#,
        "!Point { x = 1, y = 2 }",
        "`!` negates a `bool`, and this operand is `Point`",
    );
}

#[test]
fn b200_a_bang_on_a_generic_is_rejected() {
    // B181's sentence at the unary site, and for B181's reason: `bool`'s set
    // is `bool` itself and `!` models no operator trait to consult.
    assert_fails_spanning(
        r#"
        fun negated<T>(value: T): bool {
            !value
        }

        fun main() {
            print(negated(true));
        }
        "#,
        "!value",
        "no bound on `T` can prove membership",
    );
}

#[test]
fn b200_a_unary_minus_on_a_trait_typed_operand_is_rejected() {
    // The trait-typed shape, which the BINARY site can rescue and this one
    // cannot: B193 dispatches a default body's `self + self` on the type being
    // specialized, because a supertrait can promise `Add`. Nothing can promise
    // `-` — there is no `Neg` trait to declare — so every specialization would
    // reach the host's `-` over a lowered value. Pre-fix,
    // `Money { cents = 21 }.flipped().cents` printed `undefined`.
    assert_fails_spanning(
        r#"
        trait Flipper {
            fun flipped(self): Self { -self }
        }

        struct Money { cents: i32 }

        impl Money with Flipper {}

        fun main() {
            print(Money { cents = 21 }.flipped().cents);
        }
        "#,
        "-self",
        "vilan has no `Neg` for one to require",
    );
}

#[test]
fn b200_a_bang_on_a_trait_typed_operand_is_rejected() {
    // Its twin: no `Not` trait either, so `!self` in a default body was the
    // host's truthiness test over a lowered value — `false` for every
    // specialization, whatever it held.
    assert_fails_spanning(
        r#"
        trait Negator {
            fun negated(self): bool { !self }
        }

        struct Money { cents: i32 }

        impl Money with Negator {}

        fun main() {
            print(Money { cents = 21 }.negated());
        }
        "#,
        "!self",
        "vilan has no `Not` for one to require",
    );
}

#[test]
fn b200_a_bare_trait_unary_operand_gets_the_bound_steer() {
    // B175's rule about WHICH refusal, on the unary side: the two ways of
    // arriving at a trait-typed operand need different steers, because only
    // the default body has a trait to add a method to. A bare trait outside
    // one gets B175's own sentence.
    let source = r#"
        import std::io::panic;

        trait Maker {
            fun make(): Self { panic("no default") }
        }

        fun main() {
            print(-Maker::make());
        }
        "#;
    assert_fails_with(source, "A trait is a bound, not a value type");
    assert_fails_with(source, "(`<T: Maker>`)");
    assert_fails_without(source, "Inside a default body");
}

#[test]
fn b200_the_admitted_unary_forms_still_compile_and_run() {
    // The control. Every form the two admitted sets cover, including the
    // negative literal (`-128i8` is `Unary('-')` OVER the literal, and the
    // range check runs before the minus applies) and a `!` over a comparison.
    assert_compiles_and_runs(
        r#"
        import std::io::print;

        fun main() {
            print(-5);
            print(-5.5);
            let x = 3;
            print(-x);
            print(-x - 1);
            print(-128i8);
            print(!true);
            print(!(1 == 2));
            print(!!true);
        }
        "#,
        "-5\n-5.5\n-3\n-4\n-128\nfalse\ntrue\ntrue\n",
    );
}
