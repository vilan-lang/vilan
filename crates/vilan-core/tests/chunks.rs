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
    }
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
        "import std::print;\nfun main() { print(1); }\n",
        Platform::default(),
    );
    assert_eq!(plan.sites, 0);
    assert!(plan.chunks.is_empty());
}
