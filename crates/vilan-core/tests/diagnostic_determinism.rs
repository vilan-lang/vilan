//! Diagnostics are the same, in the same order, every time (E38,
//! `proposal/diagnostics-standard.md` C1).
//!
//! Three things made that false. Two were **answer**-flipping: the "the bound is
//! declared here" note picked its declaration with a `.find()` over a `HashMap`,
//! so which FILE it pointed at changed run to run; and async inference's adapted
//! instances were walked in hash order under a first-wins dedupe, so which CALL
//! SITE a transitive `sync` violation named changed run to run. The third was
//! pure **order**: nothing normalized the push order of `Program::diagnostics`,
//! and `hmr::render_overlay` shows only the first `OVERLAY_DIAGNOSTIC_CAP` of
//! them — so which errors a user saw in the browser was a hash-seed artifact.
//!
//! Proving that needs repetition, and repetition needs the COLD path: the base
//! cache (`analysis-reuse.md` §6.10) serves the same analyzed world to every
//! compile after the first in a process, so a naive loop re-reads one answer and
//! proves nothing. Each attempt clears it first, exactly as B77's pins do. The
//! comparison is the FULL rendering — every diagnostic's file, span, message and
//! note, in order, plus the warnings — because a determinism claim about only
//! the part you looked at is not one.
//!
//! These are deliberately COLD and therefore slow (one full std analysis per
//! attempt). They live in their own binary so nextest runs them alongside
//! everything else rather than behind it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vilan_core::analyzer::SourceId;
use vilan_core::error::Error;
use vilan_core::{PackageSpec, Platform, Program, Workspace, analyze_source};

/// The number of cold attempts each pin compares. Enough that a coin-flip
/// answer is missed with probability 2^-29; the regressions this file guards
/// were measured at 13/30 and 15/30 when planted back.
const ATTEMPTS: usize = 30;

/// Mirrors `vilan_cli::hmr::OVERLAY_DIAGNOSTIC_CAP`, which this crate cannot
/// see. The overlay renders `diagnostics[..CAP]` and collapses the rest to
/// "… and N more", so the browser shows a stable set exactly when the leading
/// `CAP` entries are stable.
const OVERLAY_DIAGNOSTIC_CAP: usize = 20;

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// The file a `SourceId` names, by basename — enough to tell std from the entry
/// without pinning this machine's checkout path into a failure message.
fn file_of(program: &Program, source: SourceId) -> String {
    program
        .source_path(source)
        .map(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string())
        })
        .unwrap_or_else(|| format!("<source {}>", source.0))
}

/// One diagnostic, rendered with everything a consumer can see: the file its
/// span indexes into, the span, the message, and the note's own file, span and
/// text. Anything left out of this string is something the pins below would let
/// vary.
fn render_one(program: &Program, error: &Error, source: SourceId) -> String {
    let mut rendered = format!(
        "{}:{}..{}: {}",
        file_of(program, source),
        error.span.start,
        error.span.end,
        error.msg
    );
    if let Some(note) = &error.note {
        let note_file = match note.source {
            Some(note_source) => file_of(program, note_source),
            None => "<the diagnostic's own file>".to_string(),
        };
        rendered.push_str(&format!(
            "\n    note {note_file}:{}..{}: {}",
            note.span.start, note.span.end, note.msg
        ));
    }
    rendered
}

/// Every diagnostic and warning of one cold analysis, in the order a consumer
/// reads them.
fn render_all(source: &'static str, platform: Platform) -> Vec<String> {
    let (program, errors) = analyze_source(
        source,
        &std_spec(),
        Path::new("."),
        Path::new("test.vl"),
        Some(platform),
        &Workspace::default(),
    );
    let Some(program) = program else {
        return vec![format!("<no program>, {} diagnostics", errors.len())];
    };
    let mut lines: Vec<String> = program
        .diagnostics
        .iter()
        .enumerate()
        .map(|(index, error)| render_one(&program, error, program.diagnostic_source(index)))
        .collect();
    lines.extend(program.warnings.iter().enumerate().map(|(index, warning)| {
        format!(
            "warning {}",
            render_one(&program, warning, program.warning_source(index))
        )
    }));
    lines
}

/// Analyze `source` cold [`ATTEMPTS`] times and hand back the distinct
/// renderings with their counts. One entry means deterministic.
fn cold_renderings(source: &'static str, platform: Platform) -> BTreeMap<Vec<String>, usize> {
    let source = source.to_string();
    let source: &'static str = Box::leak(source.into_boxed_str());
    // The 256 MB worker every other compiler-behavior harness uses: a deep
    // program must surface as a diagnostic, not as an aborted test process.
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let mut seen: BTreeMap<Vec<String>, usize> = BTreeMap::new();
            for _ in 0..ATTEMPTS {
                // Without this, every attempt after the first is served the
                // first one's world and the loop measures the cache.
                vilan_core::analyzer::base_cache_clear();
                *seen.entry(render_all(source, platform)).or_default() += 1;
            }
            seen
        })
        .expect("spawn the analysis worker")
        .join()
        .expect("the analysis worker panicked")
}

/// Assert one rendering across every attempt, reporting each variant and how
/// often it won when there is more than one.
#[track_caller]
fn assert_cold_rendering_is_stable(source: &'static str, platform: Platform) -> Vec<String> {
    let renderings = cold_renderings(source, platform);
    if renderings.len() == 1 {
        return renderings.into_keys().next().expect("one rendering");
    }
    let report = renderings
        .iter()
        .map(|(rendering, count)| format!("--- {count}/{ATTEMPTS} ---\n{}", rendering.join("\n")))
        .collect::<Vec<_>>()
        .join("\n");
    panic!(
        "{} distinct diagnostic renderings over {ATTEMPTS} cold analyses:\n{report}",
        renderings.len()
    );
}

/// A program with several independent failures, a note that points into the
/// user's own file and one that points into std, and a warning — the whole
/// rendered list must be byte-identical on every cold analysis.
///
/// It is one program rather than three because the ORDER between unrelated
/// checks is exactly what was unnormalized: the view escapes come out of a
/// `HashMap` walk in `check_view_escape`, the bound violations out of
/// `method_call_substitution`, the warning out of `check_must_use`, and each
/// list landed in the diagnostics vector wherever its producer happened to run.
///
/// `pair(1, 2)` is the second half of the note's problem: two DIFFERENT
/// constraints report the same message at the same span, the dedup collapses
/// them on span and message alone, and the survivor decides which parameter the
/// note names.
///
/// Planted red three ways: restore the `.find()` in
/// `check_generic_bound_satisfaction` and the `Holder` note flips (20/10); drop
/// the constraint from that function's sort key and `pair`'s note flips between
/// `A` and `B` (16/14); remove the `normalize_diagnostic_order` call from
/// `post_analysis_passes` and the order flips.
#[test]
fn the_whole_diagnostic_rendering_is_identical_on_every_cold_analysis() {
    let rendering = assert_cold_rendering_is_stable(
        r#"
        import std::print;
        import std::math::min;

        trait Greet { fun greet(self): str; }

        struct Holder<T: Greet> { item: T }

        impl Holder<type T> {
            fun shout(self): str { self.item.greet() }
        }

        struct Plain { value: i32 }

        [must_use]
        fun tally(n: i32): i32 { n + 1 }

        fun pair<A: Greet, B: Greet>(a: A, b: B): str { a.greet() }

        fun escapes() {
            let first_value = 1;
            let second_value = 2;
            let third_value = 3;
            let first_view = &first_value;
            let second_view = &second_value;
            let third_view = &third_value;
            let first_list = [first_view];
            let second_list = [second_view];
            let third_list = [third_view];
            print(first_list.len() + second_list.len() + third_list.len());
        }

        fun main() {
            let holder: Holder<i32> = Holder { item = 1 };
            print(holder.shout());
            print(min(Plain { value = 1 }, Plain { value = 2 }).value);
            print(pair(1, 2));
            tally(1);
            escapes();
        }
        "#,
        Platform::default(),
    );
    // The program is supposed to be a mess — a stable rendering of nothing
    // would pass vacuously.
    assert!(
        rendering.len() >= 6,
        "the repro must produce a multi-diagnostic list: {rendering:#?}"
    );
    // The cross-file note is the half B77's class broke: a constraint declared
    // in std must be NAMED in std, every time.
    assert!(
        rendering
            .iter()
            .any(|line| line.contains("note math.vl:")
                && line.contains("the bound is declared here")),
        "the std-declared bound must note its own file: {rendering:#?}"
    );
    // And the same-file note must name the DECLARATION (`struct Holder<T:
    // Greet>`), not the `impl Holder<type T>` binder that merely inherits it.
    assert!(
        rendering
            .iter()
            .any(|line| line.contains("note test.vl:")
                && line.contains("the bound is declared here")),
        "the user-declared bound must note its own file: {rendering:#?}"
    );
    assert!(
        rendering.iter().any(|line| line.starts_with("warning ")),
        "the warning channel is normalized too, so it must be covered: {rendering:#?}"
    );
}

/// A transitive `sync` violation names ONE call site, and the same one every
/// time.
///
/// `forwards` is instantiated twice — once with only `f` async, once with `f`
/// and `h` — so two adapted instances reach the same `run_sync(f)` / `g` pair.
/// The pass reports it once and anchors it at the instance's ORIGIN (the call
/// that instantiated it), so whichever instance the walk reached first decided
/// which of `first` / `second` the user was sent to. Planted red by restoring
/// the `instance_async.keys()` walk and the first-discovery origin: 15/30.
#[test]
fn an_adapted_instance_violation_names_the_same_call_site_every_time() {
    let rendering = assert_cold_rendering_is_stable(
        r#"
        async external fun host_wait(ms: i32): void;

        fun run_sync(g: sync || i32): i32 { g() }

        fun forwards(f: || i32, h: || i32): i32 { run_sync(f) + h() }

        fun first(): i32 { forwards(|| { host_wait(1); 2 }, || 5) }

        fun second(): i32 { forwards(|| { host_wait(2); 3 }, || { host_wait(3); 7 }) }

        fun main() { first(); second(); }
        "#,
        Platform::default(),
    );
    assert!(
        rendering
            .iter()
            .any(|line| line.contains("requires a synchronous closure (`sync`)")),
        "the repro must still raise the violation it pins: {rendering:#?}"
    );
    assert!(
        rendering
            .iter()
            .any(|line| line.contains("forwarded into the `sync` parameter `g` here")),
        "the violation must keep its note: {rendering:#?}"
    );
}

/// The browser overlay shows only the first `OVERLAY_DIAGNOSTIC_CAP`
/// diagnostics and collapses the rest, so on a program with more than that its
/// SELECTION — not just its order — was a hash-seed artifact.
///
/// The cap's arithmetic already has unit pins in `vilan-cli`'s `hmr` module;
/// what those cannot see is whether the list handed to them is the same list
/// twice. This pins that: the leading `CAP` entries of an over-cap program are
/// identical on every cold analysis.
#[test]
fn the_overlay_cap_selects_the_same_diagnostics_every_time() {
    let rendering = assert_cold_rendering_is_stable(
        r#"
        import std::print;

        fun escapes(): i32 {
            let value = 1;
            let view = &value;
            let a = [view];  let b = [view];  let c = [view];  let d = [view];
            let e = [view];  let f = [view];  let g = [view];  let h = [view];
            let i = [view];  let j = [view];  let k = [view];  let l = [view];
            let m = [view];  let n = [view];  let o = [view];  let p = [view];
            let q = [view];  let r = [view];  let s = [view];  let t = [view];
            let u = [view];  let v = [view];  let w = [view];  let x = [view];
            a.len() + b.len() + c.len() + d.len() + e.len() + f.len()
                + g.len() + h.len() + i.len() + j.len() + k.len() + l.len()
                + m.len() + n.len() + o.len() + p.len() + q.len() + r.len()
                + s.len() + t.len() + u.len() + v.len() + w.len() + x.len()
        }

        fun main() { print(escapes()); }
        "#,
        Platform::default(),
    );
    assert!(
        rendering.len() > OVERLAY_DIAGNOSTIC_CAP,
        "the repro must overflow the overlay's cap, or the truncation is untested: \
         {} diagnostics",
        rendering.len()
    );
    // The overlay's own slice, spelled out: these are the lines the browser
    // renders, and they are the same lines every run.
    let shown = &rendering[..OVERLAY_DIAGNOSTIC_CAP];
    assert!(
        shown
            .iter()
            .all(|line| line.contains("a view cannot escape its scope")),
        "the capped slice should be the escape reports: {shown:#?}"
    );
}
