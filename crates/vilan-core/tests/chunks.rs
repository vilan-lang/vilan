//! Route-chunk planning pins (bundle-splitting.md S1): the planner's verdict
//! on the canonical routing app, and the recognizer's silence everywhere
//! else.

use std::path::{Path, PathBuf};

use vilan_core::manifest::{PreludeSpec, WEB_PRELUDE};
use vilan_core::{Platform, Workspace, analyze_source};

/// Plans `source` under the BASE prelude — the ambient scope a package with no
/// `prelude` key resolves under, and the right one for the self-contained
/// fixtures below, which name every import they use.
fn plan_for(source: &'static str, platform: Platform) -> vilan_core::chunks::ChunkPlan {
    plan_under(source, platform, PreludeSpec::default())
}

/// Plans `source` under a declared ambient scope (`prelude.md` §6.2). A fixture
/// read off disk has to be analyzed under the prelude ITS manifest declares, or
/// the probe means something the build never does.
fn plan_under(
    source: &'static str,
    platform: Platform,
    prelude: PreludeSpec,
) -> vilan_core::chunks::ChunkPlan {
    let spec = vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    );
    let workspace = Workspace {
        entry_prelude: prelude,
        ..Workspace::default()
    };
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &spec,
                Path::new("."),
                Path::new("chunk_probe.vl"),
                Some(platform),
                &workspace,
            );
            assert!(errors.is_empty(), "{errors:?}");
            vilan_core::chunks::plan(&program.expect("program"))
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

fn router_example() -> &'static str {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/examples/router/app.vl");
    Box::leak(
        std::fs::read_to_string(path)
            .expect("read router example")
            .into_boxed_str(),
    )
}

#[test]
fn the_router_example_splits_into_its_three_routes() {
    // `vilan/examples/router/vilan.toml` declares `prelude = "std::web"`, so
    // `view`, `View`, `SignalCell` and the `ui` module are ambient there and the
    // file imports none of them. Analyzing it under the base prelude instead
    // would fail to resolve them — the probe has to read the example the way its
    // own manifest does.
    let plan = plan_under(
        router_example(),
        Platform::Browser,
        PreludeSpec::Module(WEB_PRELUDE.to_string()),
    );
    eprintln!("{}", vilan_core::chunks::render(&plan, "app.vl"));
    assert_eq!(plan.sites, 1, "one splittable match (the nested one is v2)");
    let arms: Vec<&str> = plan.chunks.iter().map(|chunk| chunk.arm.as_str()).collect();
    assert_eq!(
        arms,
        ["Route::Home", "Route::Items(..)", "Route::NotFound"],
        "one chunk per top-level route arm"
    );
    let of = |arm: &str| {
        plan.chunks
            .iter()
            .find(|chunk| chunk.arm == arm)
            .map(|chunk| chunk.functions.clone())
            .unwrap_or_default()
    };
    assert_eq!(of("Route::Home"), ["home_page"]);
    assert_eq!(
        of("Route::Items(..)"),
        ["item_detail", "items_layout", "items_list"],
        "the nested routes ride their parent's chunk in v1"
    );
    assert_eq!(of("Route::NotFound"), ["not_found_page"]);
    for chunk in &plan.chunks {
        assert!(chunk.bytes > 0, "byte estimate must be non-trivial");
        assert_eq!(
            chunk.functions.len(),
            chunk.ids.len(),
            "emission partitions on the same set the report names"
        );
    }
    // The tag is the arm's variant index, which is what a route value carries
    // and so what the emitted chunk map is keyed by (S2).
    let tags: Vec<usize> = plan.chunks.iter().map(|chunk| chunk.tag).collect();
    assert_eq!(tags, [0, 1, 2], "one tag per arm, in declaration order");
    let gate = plan
        .gate
        .as_ref()
        .expect("the browser layer declares the gate");
    assert_eq!(gate.calls.len(), 1, "the one recognized `swap` call");
    assert_ne!(gate.swap, gate.swap_split, "the gate is a different method");
}

/// A `_` arm has no variant tag, so nothing could address its chunk at a
/// navigation. Its exclusive code stays EAGER rather than becoming a chunk
/// that can never be fetched — and it is still emitted, which is the part a
/// naive "skip the arm" would lose.
#[test]
fn a_wildcard_arm_keeps_its_code_eager() {
    let plan = plan_for(
        r#"
import std::ui::{View, view, mount_root};
import std::reactive::Signal;
import std::router::{current_path, segments};

[derive(PartialEq)]
enum Route {
    Home,
    Other,
}

fun parse(path: str): Route {
    if segments(path).len() == 0 { Route::Home } else { Route::Other }
}

fun home_page(): View {
    view("h1").child("home")
}

fun fallback_page(): View {
    view("h1").child("elsewhere")
}

fun main() {
    let route = current_path().map(parse);
    mount_root("app", || {
        view("div").swap(route, |current| match current {
            Route::Home => home_page(),
            _ => fallback_page(),
        })
    });
}
"#,
        Platform::Browser,
    );
    assert_eq!(plan.sites, 1);
    let arms: Vec<&str> = plan.chunks.iter().map(|chunk| chunk.arm.as_str()).collect();
    assert_eq!(arms, ["Route::Home"], "only the tagged arm chunks");
    assert_eq!(
        plan.chunks[0].functions,
        ["home_page"],
        "and it keeps its own page"
    );
}

/// Two route matches would alias each other's chunks — a chunk is addressed by
/// the route value's variant tag alone — so v1 declines the split and the
/// report says so, rather than emitting a map the runtime can misread.
#[test]
fn a_second_route_match_declines_the_split() {
    let plan = plan_for(
        r#"
import std::ui::{View, view, mount_root};
import std::reactive::Signal;
import std::router::{current_path, segments};

[derive(PartialEq)]
enum Route {
    Home,
    Other,
}

[derive(PartialEq)]
enum Tab {
    Left,
    Right,
}

fun parse(path: str): Route {
    if segments(path).len() == 0 { Route::Home } else { Route::Other }
}

fun home_page(): View { view("h1").child("home") }
fun other_page(): View { view("h1").child("other") }
fun left_pane(): View { view("p").child("left") }
fun right_pane(): View { view("p").child("right") }

fun main() {
    let route = current_path().map(parse);
    let tab = Signal::new(Tab::Left);
    mount_root("app", || {
        view("div")
            .child(view("section").swap(tab, |current| match current {
                Tab::Left => left_pane(),
                Tab::Right => right_pane(),
            }))
            .swap(route, |current| match current {
                Route::Home => home_page(),
                Route::Other => other_page(),
            })
    });
}
"#,
        Platform::Browser,
    );
    assert_eq!(plan.sites, 2, "both matches are recognized");
    assert!(plan.chunks.is_empty(), "but v1 splits neither");
    let report = vilan_core::chunks::render(&plan, "app.vl");
    assert!(
        report.contains("v1 splits one per entry") && report.contains("nothing would split"),
        "the report explains the decline: {report}"
    );
}

/// The artifact name reduces an arm pattern to identifier characters, so
/// `dist/<leg>.<arm>.js` is a real file name (`bundle-splitting.md` §3).
#[test]
fn a_chunk_file_name_is_its_arm_reduced_to_a_file_name() {
    use vilan_core::chunks::chunk_file_name;
    assert_eq!(
        chunk_file_name("client", "Route::Home"),
        "client.Route_Home.js"
    );
    assert_eq!(
        chunk_file_name("client", "Route::Items(..)"),
        "client.Route_Items.js"
    );
    // No arm reduces to nothing today (an untagged one stays eager); the
    // fallback is what keeps that from ever naming `client..js`.
    assert_eq!(chunk_file_name("client", "_"), "client.chunk.js");
}

/// The recognizer's silence: a browser app with swap but NO route match on
/// the closure parameter plans nothing.
#[test]
fn a_swap_without_a_route_match_is_not_splittable() {
    let plan = plan_for(
        r#"
import std::ui::{View, view, mount_root};
import std::reactive::Signal;

fun label(on: bool): View {
    view("span").child(if on { "on" } else { "off" })
}

fun main() {
    let flag = Signal::new(true);
    mount_root("app", || {
        view("div").swap(flag, |on| label(on))
    });
}
"#,
        Platform::Browser,
    );
    assert_eq!(plan.sites, 0, "no match on the parameter, no split");
    assert!(plan.chunks.is_empty());
}

/// Node programs never split (no browser layer, no swap).
#[test]
fn a_node_program_plans_nothing() {
    let plan = plan_for(
        "import std::io::print;\nfun main() { print(1); }\n",
        Platform::default(),
    );
    assert_eq!(plan.sites, 0);
    assert!(plan.chunks.is_empty());
}

// === Emission across a chunk boundary (M20, bundle-boundaries.md §4.1 / D5) ==
//
// The route partition cannot put two mutually-referencing functions in
// different chunks — anything two arms reach is eager, so a chunk's non-std
// dependencies are all eager (§1.6, fact 2) — and that is the ONLY reason the
// chunk preamble's by-value snapshot is safe. A declared boundary voids it, and
// so does any shared-chunk extraction. The emitter therefore takes its
// partition as an argument (`transform_split_with_plan`), and the pins below
// hand it the partition v1's planner cannot make: `docs_double` moved out of
// the docs chunk into one of its own, leaving a reference in each direction.

/// A router program whose docs arm reaches three functions: the page, which
/// builds the DOM, and two arithmetic helpers under it. Splitting the helpers
/// apart is what makes a cross-chunk reference, and keeping them free of every
/// eager name is what lets the pin below CALL one under node with an empty
/// registry.
const CROSSING_SOURCE: &str = r#"
import std::ui::{View, view, mount_root};
import std::router::{current_path, segments};

[derive(PartialEq)]
enum Route {
    Home,
    Docs,
}

fun parse(path: str): Route {
    if segments(path).len() == 0 { Route::Home } else { Route::Docs }
}

fun home_page(): View {
    view("h1").text("home")
}

fun docs_page(): View {
    view("h1").text(i"page {docs_double(3)}")
}

fun docs_double(page: i32): i32 {
    docs_plus(page) * 2
}

fun docs_plus(page: i32): i32 {
    page + 1
}

fun main() {
    let route = current_path().map(parse);
    mount_root("app", || {
        view("div").swap(route, |current| match current {
            Route::Home => home_page(),
            Route::Docs => docs_page(),
        })
    });
}
"#;

/// The arm the repartition invents for `docs_double` — a name no route pattern
/// could produce, so nothing about the pin can be mistaken for v1's planner.
const BOUNDARY_ARM: &str = "Boundary::docs_double";

/// Plans [`CROSSING_SOURCE`], moves `docs_double` into a chunk of its own, and
/// emits. The result is the shape M18's nested boundaries reach on their first
/// step: the docs chunk calls `docs_double`, and `docs_double`'s chunk calls
/// `docs_plus` back in the docs chunk — neither can be evaluated first.
fn emit_the_crossing_split() -> vilan_core::SplitProgram {
    let spec = vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    );
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                CROSSING_SOURCE,
                &spec,
                Path::new("."),
                Path::new("chunk_probe.vl"),
                Some(Platform::Browser),
                &Workspace::default(),
            );
            assert!(errors.is_empty(), "{errors:?}");
            let program = program.expect("program");
            let mut plan = vilan_core::chunks::plan(&program);

            let moved = program
                .functions
                .iter()
                .find_map(|(id, function)| (function.name == "docs_double").then_some(*id))
                .expect("`docs_double` is a function of the program");
            let owner = plan
                .chunks
                .iter()
                .position(|chunk| chunk.ids.contains(&moved))
                .expect("the docs arm chunks `docs_double`");
            plan.chunks[owner].ids.retain(|id| *id != moved);
            plan.chunks[owner]
                .functions
                .retain(|name| name != "docs_double");
            let tag = plan.chunks.iter().map(|chunk| chunk.tag).max().unwrap_or(0) + 1;
            plan.chunks.push(vilan_core::chunks::Chunk {
                arm: BOUNDARY_ARM.to_string(),
                tag,
                functions: vec!["docs_double".to_string()],
                ids: vec![moved],
                bytes: 0,
            });

            vilan_core::transform_split_with_plan(
                &program,
                &vilan_core::BuildOptions::default(),
                "app",
                &plan,
            )
            .expect("emit the repartitioned split")
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

fn chunk_of<'a>(split: &'a vilan_core::SplitProgram, arm: &str) -> &'a vilan_core::EmittedChunk {
    split
        .chunks
        .iter()
        .find(|chunk| chunk.arm == arm)
        .unwrap_or_else(|| panic!("no chunk for `{arm}`"))
}

/// The emission rule (D5): a name a SIBLING chunk owns is read at the use, and
/// an EAGER name keeps its one-time snapshot. Both halves are asserted, because
/// the fix is worthless if it also stops the eager names — those are sound by
/// construction (the eager registrations run in the entry's own module
/// evaluation) and paying a property read for them would be a regression on
/// every call a chunk makes.
#[test]
fn a_sibling_chunks_function_is_read_at_the_use_and_an_eager_one_is_snapshotted() {
    let split = emit_the_crossing_split();
    let docs = chunk_of(&split, "Route::Docs");
    let boundary = chunk_of(&split, BOUNDARY_ARM);

    // Each direction of the crossing is a property read AT THE CALL.
    assert!(
        docs.source.contains("__vilan_chunks.fn.docs_double("),
        "the docs chunk must call its sibling through the registry:\n{}",
        docs.source
    );
    assert!(
        boundary.source.contains("__vilan_chunks.fn.docs_plus("),
        "and the sibling must call back the same way:\n{}",
        boundary.source
    );
    // …and NOT the snapshot probe P3 showed binds `undefined` forever.
    assert!(
        !docs.source.contains("const docs_double ="),
        "a sibling's name must not be snapshotted at evaluation:\n{}",
        docs.source
    );
    assert!(
        !boundary.source.contains("const docs_plus ="),
        "a sibling's name must not be snapshotted at evaluation:\n{}",
        boundary.source
    );

    // The control: an eager name is still read ONCE, into a `const`, and never
    // at the call.
    assert!(
        docs.source.contains("const view = __vilan_chunks.fn.view;"),
        "an eager name keeps its by-value snapshot:\n{}",
        docs.source
    );
    assert!(
        !docs.source.contains("__vilan_chunks.fn.view("),
        "and is not re-read at every use:\n{}",
        docs.source
    );

    // Registration is unchanged: a chunk still publishes its own names, which
    // is what makes the reads above resolve.
    assert!(
        boundary
            .source
            .contains("__vilan_chunks.fn.docs_double = docs_double;"),
        "a chunk registers what it declares:\n{}",
        boundary.source
    );

    // The cost, counted: one property read per crossing reference, per chunk.
    assert_eq!(
        (docs.cross_chunk_references, boundary.cross_chunk_references),
        (1, 1),
        "one crossing in each direction"
    );
}

/// P3's shape, run: the two chunks EVALUATED IN THE WRONG ORDER. The dependent
/// lands first, its provider registers afterwards, and the call still resolves —
/// which is precisely what the by-value snapshot could not do (it bound
/// `undefined` at evaluation and kept it, throwing `TypeError` after the
/// provider had registered).
#[test]
fn a_chunk_evaluated_before_its_dependency_still_calls_it() {
    let split = emit_the_crossing_split();
    let staged = std::env::temp_dir().join(format!("vilan_chunk_crossing_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);
    std::fs::create_dir_all(&staged).expect("create the staging directory");
    for chunk in &split.chunks {
        std::fs::write(staged.join(&chunk.file), &chunk.source).expect("write a chunk");
    }
    // The registry an eager bundle would have installed, empty of every name:
    // nothing here needs the entry, only the two chunks and the order they
    // arrive in.
    std::fs::write(
        staged.join("harness.js"),
        format!(
            r#"globalThis.__vilan_chunks = {{ fn: {{}}, url: {{}}, pending: {{}}, loaded: {{}} }};
require("./{boundary}");
require("./{docs}");
try {{
	console.log(globalThis.__vilan_chunks.fn.docs_double(3));
}} catch (error) {{
	console.log("THREW: " + error.constructor.name + ": " + error.message);
}}
"#,
            boundary = chunk_of(&split, BOUNDARY_ARM).file,
            docs = chunk_of(&split, "Route::Docs").file,
        ),
    )
    .expect("write the harness");

    let output = std::process::Command::new("node")
        .arg("harness.js")
        .current_dir(&staged)
        .output()
        .expect("run the node harness");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let _ = std::fs::remove_dir_all(&staged);
    assert!(
        output.status.success(),
        "the crossing harness must run:\n{stdout}\n{stderr}"
    );
    // `docs_double(3)` is `docs_plus(3) * 2`, and `docs_plus` lives in the chunk
    // that was evaluated SECOND.
    assert_eq!(
        stdout, "8\n",
        "a late-registered dependency must resolve at the call\n{stderr}"
    );
}
