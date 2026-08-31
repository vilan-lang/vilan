//! Route-chunk planning pins (bundle-splitting.md S1): the planner's verdict
//! on the canonical routing app, and the recognizer's silence everywhere
//! else.

use std::path::{Path, PathBuf};

use vilan_core::{Platform, Workspace, analyze_source};

fn plan_for(source: &'static str, platform: Platform) -> vilan_core::chunks::ChunkPlan {
    let spec = vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    );
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &spec,
                Path::new("."),
                Path::new("chunk_probe.vl"),
                Some(platform),
                &Workspace::default(),
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
    let plan = plan_for(router_example(), Platform::Browser);
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
