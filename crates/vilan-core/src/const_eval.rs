//! The `const` pass (proposal/const-eval.md): evaluates `const`-marked
//! expressions post-analysis with the macro interpreter, in dependency order,
//! producing plain-data results the transformer serializes in place — plus
//! spanned diagnostics for everything that cannot evaluate. Free variables of
//! a const expression must be compile-time-known: an item (function, struct,
//! enum), or an immutable binding whose initializer is a literal or another
//! `const` expression.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::analyzer::{Expr, Program, SourceId};
use crate::call_graph::{Call, CallGraph, CallTarget, Node};
use crate::error::{Error, Note};
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::interpreter::{self, ConstValue, FailureKind, Limits};
use crate::options::BuildOptions;
use crate::span::Span;
use crate::transformer;
use crate::type_::{Type, TypeId};

/// The budgets the EXPLICIT form evaluates under (const-eval.md §9.3). A miss
/// here is a diagnostic (§4's "did not finish within the compile-time budget"),
/// so the user can see it and act — which is what lets them be generous.
///
/// FUEL, measured 2026-08-26 (K13 step 2, the const input channel): the
/// canonical heavy workload — `const` `asset::read` + `std::markdown::parse`
/// of the book's largest real page, `docs/spec/memory.md` at 40,758 bytes —
/// consumes **2,001,457 fuel** (~49/char, reads charged per byte), which the
/// old 1,000,000 could not finish (docs-port.md §2.1 located the wall; this
/// sizing replaces its 2.5–10M estimate with the measured number). 16M is 8×
/// that: room for a page several times the largest plus parser growth, while
/// the diagnostic keeps the runaway case bounded — the interpreter burns
/// ~16.4M fuel/s in a release binary (~3.3M/s debug), so a `while true`
/// exhausts in ~1 s where the editor's debounced const pass must survive it
/// (§4). Sized against both ends, not just one.
///
/// Alternatives considered and declined for now: a `[const]` manifest knob
/// (§9.3 keeps budgets compiler constants; §8's knob question is the owner's,
/// still open, and a lane must not pre-empt it), per-invocation budgets (same
/// question in call-site clothes), and fuel-exempting intrinsics (would
/// unbound the one lever that keeps a runaway const finite). Memoization
/// stays the recorded refinement it was (§4).
const EXPLICIT_LIMITS: Limits = Limits {
    fuel: 16_000_000,
    call_depth: 512,
};

/// The budgets an INFERRED attempt evaluates under (const-eval.md §9.3):
/// tighter in every dimension, because a miss here is silent. Sized against
/// the measurement in §9.1 — every fold the tree produces completes within 200
/// fuel — so these carry ~50× headroom while sitting far under the explicit
/// budget in every dimension (they deliberately did NOT move with the K13
/// step-2 raise above: inference's economics are §9.1's, not the book's).
const INFERRED_LIMITS: Limits = Limits {
    fuel: 10_000,
    call_depth: 64,
};

/// The most bytes an inferred fold's literal may occupy (const-eval.md §9.3).
/// §5's rule — "a 10 KB table literal replacing a 20-character call is a
/// regression nobody asked for" — with explicit `const` as the opt-in for big
/// results. The largest fold in the tree measures 21 bytes, the median 2.
const INFERRED_SIZE_CAP: usize = 256;

/// Which const form a [`State`] is evaluating. They share one machine and
/// differ in exactly three places — whether a failure is a diagnostic, which
/// budgets apply, and whether a pending inference candidate counts as
/// compile-time-known — so a second implementation of eligibility (the way the
/// two forms would silently drift apart) is not needed.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Explicit,
    Inferred,
}

thread_local! {
    /// The `VILAN_PHASE_TIMING` sub-split of this pass (backlog M5): how much
    /// of `evaluate`'s wall went to LOWERING — the shared const world's walks
    /// plus per-site prelude and site assembly ([`transformer::ConstWorld`]'s
    /// `prepare`/`site`) — against the interpreter EVALUATING the lowered
    /// sites. The one-third/two-thirds proportion `const-eval.md` §10.2 had
    /// to hand-patch marks in to learn, kept as a run instead. Accumulated
    /// unconditionally on the analyzer's argument (a clock read per site is
    /// noise next to the site), reset by [`evaluate`], read back by
    /// `post_analysis_passes` for its phase line. Thread-local because an
    /// analysis is single-threaded, the same way the transformer's
    /// `CONST_LOWERING_COUNT` is.
    static PHASE_LOWER: std::cell::Cell<std::time::Duration> =
        const { std::cell::Cell::new(std::time::Duration::ZERO) };
    static PHASE_INTERP: std::cell::Cell<std::time::Duration> =
        const { std::cell::Cell::new(std::time::Duration::ZERO) };
    /// The most fuel any single explicit `const` site consumed this analysis —
    /// the budget instrument beside the timing split: `EXPLICIT_LIMITS.fuel`
    /// is sized against measured workloads, and this is how a workload gets
    /// measured (`VILAN_PHASE_TIMING` prints it as `const-fuel-max`). Reset by
    /// [`evaluate`] with the timing cells.
    static FUEL_MAX: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// (lowering, interpreting) — what [`evaluate`] spent since its last call, for
/// the `VILAN_PHASE_TIMING` line. The two do NOT sum to the pass: the
/// remainder is classification (free locals, `check_const_only`) and failure
/// attribution.
pub(crate) fn phase_split() -> (std::time::Duration, std::time::Duration) {
    (
        PHASE_LOWER.with(std::cell::Cell::get),
        PHASE_INTERP.with(std::cell::Cell::get),
    )
}

/// The most fuel any single explicit `const` site consumed in the last
/// [`evaluate`] — the `VILAN_PHASE_TIMING` line's `const-fuel-max` value.
pub(crate) fn max_fuel_used() -> u64 {
    FUEL_MAX.with(std::cell::Cell::get)
}

fn phase_add(
    bucket: &'static std::thread::LocalKey<std::cell::Cell<std::time::Duration>>,
    started: crate::PhaseClock,
) {
    bucket.with(|cell| cell.set(cell.get() + started.elapsed()));
}

/// `asset::read`'s host (docs-port.md §3.3, the input direction of
/// const-eval.md §3's channel): resolves each read against the package root —
/// never the process working directory — reads through the one overlay-aware
/// seam every other source read uses (which is what makes the wasm
/// playground's registered documents readable and a plain wasm context fail
/// cleanly), and records every touched path, hit and miss alike, as a build
/// input. Recording the misses matters: a file that was not there is still a
/// dependency, and its APPEARANCE must invalidate whatever reused the result
/// exactly as a change to it would.
struct ProjectReader {
    root: PathBuf,
    inputs: RefCell<Vec<(PathBuf, Option<u64>)>>,
    /// Every file `asset::bundle` registered, as (resolved source, the name it
    /// takes in the output directory). Insertion-ordered and deduplicated by
    /// name, so bundling one file from two call sites registers it once.
    bundled: RefCell<Vec<(PathBuf, String)>>,
}

/// The package-relative name a bundled file takes in the output directory —
/// the requested path with `./` segments dropped, `/`-joined whatever the host
/// separator is, or the reason it is not a name at all.
///
/// The path IS the name (kolt.local 029). Nothing is renamed behind the
/// author's back, two different files can never claim one name, and a
/// subdirectory survives — which is what the vilan-website's
/// `playground/editor.js` needs and a basename rule would have flattened onto
/// the site root. Where a resource lands in `dist/` is therefore a layout
/// decision the author makes by putting the file somewhere, not a policy the
/// compiler applies.
///
/// POSIX-shaped, exactly as `std::path` is (kolt.local 017): the emitted name
/// is derived output — a url, a manifest row, a golden — and a separator-aware
/// rule would make every one of them host-dependent. `\` is refused rather
/// than translated, so a path that means two things on two hosts means nothing
/// on either.
fn bundled_name(path: &str) -> Result<String, String> {
    if path.contains('\\') {
        return Err(format!(
            "`asset::bundle` paths are `/`-separated on every host; `{path}` contains a backslash"
        ));
    }
    let requested = Path::new(path);
    if requested.is_absolute() {
        return Err(format!(
            "`asset::bundle` paths are relative to the package root; `{path}` is absolute"
        ));
    }
    let mut parts: Vec<&str> = Vec::new();
    for component in requested.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => match part.to_str() {
                Some(part) => parts.push(part),
                None => {
                    return Err(format!(
                        "`asset::bundle` paths must be valid text; `{path}` is not"
                    ));
                }
            },
            _ => {
                return Err(format!(
                    "`asset::bundle` paths resolve inside the package root; `{path}` escapes it"
                ));
            }
        }
    }
    if parts.is_empty() {
        return Err(format!(
            "`asset::bundle` needs a file inside the package root; `{path}` names none"
        ));
    }
    Ok(parts.join("/"))
}

impl interpreter::AssetReader for ProjectReader {
    fn read(&self, path: &str) -> Result<String, String> {
        let requested = Path::new(path);
        // Relative, inside the root, by construction: the channel reads THE
        // PROJECT, deterministically per build-input closure — a path that is
        // absolute or climbs out of the root reaches state the build cannot
        // track (const-eval.md §9.5's determinism stance). Refused lexically,
        // before any filesystem look, so the refusal itself is deterministic.
        if requested.is_absolute() {
            return Err(format!(
                "`asset::read` paths are relative to the package root; `{path}` is absolute"
            ));
        }
        if requested
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(format!(
                "`asset::read` paths resolve inside the package root; `{path}` escapes it"
            ));
        }
        let resolved = self.root.join(requested);
        match crate::util::read_source(&resolved) {
            Ok(text) => {
                self.inputs
                    .borrow_mut()
                    .push((resolved, Some(crate::content_hash(&text))));
                Ok(text)
            }
            Err(error) => {
                self.inputs.borrow_mut().push((resolved.clone(), None));
                Err(format!(
                    "cannot read `{path}` (resolved against the package root to `{}`): {error}",
                    resolved.display()
                ))
            }
        }
    }

    /// Registers `path` as a file the build carries, and answers with the url
    /// its bundled copy is served at — `/` + the name, so the value is exactly
    /// what an `<img src>` and `serve_build`'s route table both want.
    ///
    /// The file's BYTES are read here and thrown away. They are read because a
    /// bundled file is a build input like any other — a change to it must
    /// invalidate the compile that named it, and this tree decides that by
    /// content and never by mtime — and thrown away because nothing downstream
    /// wants them: the CLI copies the file itself, so holding a font in the
    /// `Program` would buy nothing. A miss is an ERROR and not a recorded
    /// absence: `read`'s miss can be a legitimate answer to ask about, but a
    /// build cannot carry a file that is not there.
    fn bundle(&self, path: &str) -> Result<String, String> {
        let name = bundled_name(path)?;
        let resolved = self.root.join(&name);
        // Recorded before the read succeeds, exactly as `read` records its
        // misses: a file that was not there is still a dependency, and its
        // APPEARANCE must invalidate the compile that failed on it.
        match std::fs::read(&resolved) {
            Ok(bytes) => {
                self.inputs
                    .borrow_mut()
                    .push((resolved, Some(crate::content_hash_bytes(&bytes))));
            }
            Err(error) => {
                self.inputs.borrow_mut().push((resolved.clone(), None));
                return Err(format!(
                    "cannot bundle `{path}` (resolved against the package root to `{}`): {error}",
                    resolved.display()
                ));
            }
        }
        let mut bundled = self.bundled.borrow_mut();
        if !bundled.iter().any(|(_, existing)| *existing == name) {
            bundled.push((self.root.join(&name), name.clone()));
        }
        Ok(format!("/{name}"))
    }
}

/// Takes the analysis tail's shared call graph rather than building its own
/// (E35). This pass writes nothing to the program at all — it takes `&Program`
/// and RETURNS its results for the caller to store — so the graph it is handed
/// is bit-for-bit the one it used to build.
pub struct Evaluated {
    pub results: HashMap<Id, ConstValue>,
    pub assets: Vec<(String, String)>,
    /// Each failure with the file its span indexes into (backlog E16): the pass
    /// walks the whole program, so a `const` in a module reports in that module.
    pub errors: Vec<(Error, SourceId)>,
    /// Every file `asset::read` read or `asset::bundle` registered, resolved,
    /// with the content hash it saw (`None` for a miss) — the build inputs the
    /// caller must treat as it treats the `.vl` sources (watch them; key reuse
    /// on them).
    pub input_files: Vec<(PathBuf, Option<u64>)>,
    /// Every file `asset::bundle` registered, as (resolved source, the name it
    /// takes in the output directory) — the build OUTPUTS the caller copies.
    pub bundled: Vec<(PathBuf, String)>,
}

pub fn evaluate(program: &Program, options: &BuildOptions, graph: &CallGraph) -> Evaluated {
    // Reset the phase buckets FIRST, before any early return, so the timing
    // line never reports a previous analysis's accumulation.
    PHASE_LOWER.with(|cell| cell.set(std::time::Duration::ZERO));
    PHASE_INTERP.with(|cell| cell.set(std::time::Duration::ZERO));
    FUEL_MAX.with(|cell| cell.set(0));
    // A program that already failed analysis skips evaluation entirely: the
    // transformer's entity lookups (used to lower the const world) assume
    // a clean program, exactly as `transform` itself does.
    if !program.diagnostics.is_empty() {
        return Evaluated {
            results: HashMap::default(),
            assets: Vec::new(),
            errors: Vec::new(),
            input_files: Vec::new(),
            bundled: Vec::new(),
        };
    }
    let reader = ProjectReader {
        root: program.pkg_root.clone(),
        inputs: RefCell::new(Vec::new()),
        bundled: RefCell::new(Vec::new()),
    };
    let mut world = transformer::ConstWorld::new(program, options);
    let mut state = State::new(program, Mode::Explicit, HashSet::default(), Some(&reader));
    state.check_const_only(graph);
    for &expr_id in &program.const_exprs {
        state.evaluate_one(&mut world, expr_id);
    }
    // Destructure first: `state` holds the borrow of `reader`, and the borrow
    // must end before the recorded inputs move out of it.
    let State {
        results,
        assets,
        errors,
        ..
    } = state;
    let mut inputs = reader.inputs.into_inner();
    inputs.sort();
    inputs.dedup();
    Evaluated {
        results,
        assets,
        errors,
        input_files: inputs,
        // Insertion order, NOT sorted: a build log that names the files in the
        // order the program asked for them reads as the program does, and the
        // registry is already deduplicated by name.
        bundled: reader.bundled.into_inner(),
    }
}

/// The INFERENCE sweep (const-eval.md §9): fold every `let`/`mut` initializer
/// the const evaluator can settle, and leave every one it cannot exactly as it
/// was — with **zero** diagnostics, whatever went wrong.
///
/// Returns only the NEW folds; the explicit pass's results are already on the
/// program and are seeded in here so an inferred fold may read a binding the
/// explicit form settled.
///
/// **This runs on the `vilan` CLI's build path and nowhere else.** It must
/// never be called from `analyze_source`, which is what the language server,
/// the wasm playground, and the test harnesses enter through (§4's tooling
/// split, §9.6) — silent-fallback optimization produces nothing an editor
/// could surface. `const_eval_reach.rs` pins that at the source level, the way
/// the playground's split guard does.
pub fn infer(program: &Program, options: &BuildOptions) -> HashMap<Id, ConstValue> {
    // The preset gate (§9.4): debug keeps the computation so it stays in stack
    // traces. A program that failed analysis is skipped for the same reason
    // `evaluate` skips it — the transformer's entity lookups assume a clean
    // program.
    if !options.infer_const || !program.diagnostics.is_empty() {
        return HashMap::default();
    }
    let candidates = inference_candidates(program);
    if candidates.is_empty() {
        return HashMap::default();
    }
    let mut world = transformer::ConstWorld::new(program, options);
    // No reader: the inferred form's effect channels are closed (§9.2), and
    // `asset::read` is const-only besides — a candidate reaching it was
    // already refused statically.
    let mut state = State::new(
        program,
        Mode::Inferred,
        candidates.iter().copied().collect(),
        None,
    );
    // An inferred fold may read what the explicit pass already computed.
    state.results = program.const_results.clone();
    for &expr_id in &candidates {
        state.evaluate_one(&mut world, expr_id);
    }
    debug_assert!(
        state.errors.is_empty(),
        "the inference sweep produced a diagnostic; silent fallback is the whole \
         reason it is safe to run over every binding (const-eval.md §9.2)"
    );
    for id in program.const_results.keys() {
        state.results.remove(id);
    }
    state.results
}

/// The bindings the sweep attempts, in a SOURCE-DERIVED order (§9.5): sorted by
/// file and then by position, never in `HashMap` order, so the same source
/// folds identically across builds by reading rather than by trusting a hash
/// seed.
///
/// The universe is every `let`/`mut` initializer in every source — entry,
/// modules, and std alike (§9.1: std holds almost every fold in this tree, so a
/// rule that excepted it would cost the feature most of its value). Two
/// exclusions are pure savings: an already-`const` initializer belongs to the
/// explicit pass, and a literal or bare-alias initializer folds to itself.
/// A binding DECLARED inside a `const` expression is a third: the enclosing
/// expression already folded, so the transformer never walks as far as this
/// initializer.
///
/// The fourth is the one that is about SOUNDNESS rather than savings — a
/// binding inside a type-parameter-dependent function body, where a fold has no
/// monomorphization context and would silently produce the wrong value. See
/// [`GenericRegions`].
///
/// Everything else is attempted, and the free-variable rule is the filter.
fn inference_candidates(program: &Program) -> Vec<Id> {
    let const_set: HashSet<Id> = program.const_exprs.iter().copied().collect();
    let generic_regions = GenericRegions::build(program);
    let mut candidates: Vec<Id> = program
        .variables
        .values()
        .filter_map(|variable| variable.initial)
        .filter(|initial| {
            !const_set.contains(initial)
                && !matches!(
                    program.entity_map.get(initial),
                    Some(
                        Expr::String(_)
                            | Expr::MultilineString(_)
                            | Expr::Number(..)
                            | Expr::Bool(_)
                            | Expr::Null
                            | Expr::Local(_)
                    )
                )
                && !within_a_const_expression(program, *initial)
                && !generic_regions.covers(program, *initial)
        })
        .collect();
    candidates.sort_by_key(|id| {
        (
            program.source_of(*id).map(|source| source.0),
            program.span_map.get(id).map(|span| span.start),
            id.0,
        )
    });
    candidates.dedup();
    candidates
}

/// Deduplicates and deterministically orders the collected `(kind, line)`
/// pairs into per-kind file contents (newline-terminated). Every kind's
/// bytes are a function of the SET of contributions — write order never
/// leaks (build-hooks.md §5.1) — under a kind-specific rule (const-eval.md
/// §3):
///
/// - **Every kind but `css` sorts lexically by line.** That is the one
///   content-derived order that assumes nothing about what the lines mean,
///   and it is exactly what the proposed keyed surface gives an un-keyed
///   `emit` (`emit_keyed(kind, line, line)` sorts by `(line, line)` —
///   build-hooks.md §5.3), so these bytes hold if that surface lands.
///
/// - **`css` alone adds the cascade override**: `@media (min-width: …)`
///   lines sort by ascending min-width, after everything else, not by digit
///   bytes. The lexical half is SOUND for the CSS the styling system emits —
///   `.class` rules ('.' = 0x2E) sort before `:root` variables and `@media`
///   blocks ('@' = 0x40), so media rules take the later cascade position
///   they need, and pseudo-class rules don't compete with base rules on
///   cascade order at all (their classes are distinct and their specificity
///   is higher). The width override exists because on a wide viewport every
///   narrower `min-width` rule also matches, specificity ties, and cascade
///   order decides — the widest matching breakpoint must come last for a
///   mobile-first `.sm(x).lg(y)` chain to render `y`, and the lexical digit
///   sort put `1024px` before `640px` so the narrow rule won (B35). Applied
///   to any other kind that comparator silently reordered — a line that
///   happened to parse as a media rule sorted last whatever its first byte,
///   because `None` precedes `Some` in the key (G5) — which is why it is
///   fenced to the one kind whose semantics justify it.
pub fn assemble_assets(assets: &[(String, String)]) -> BTreeMap<String, String> {
    let mut by_kind: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (kind, line) in assets {
        by_kind.entry(kind).or_default().insert(line);
    }
    by_kind
        .into_iter()
        .map(|(kind, lines)| {
            let mut lines = lines.into_iter().collect::<Vec<_>>();
            if kind == "css" {
                // Media lines as a group sort after everything else ('@' is
                // the highest first byte the styling system emits) — the key
                // only has to order them among themselves and keep the rest
                // lexical.
                lines.sort_by_key(|line| (media_min_width(line).map(f64::to_bits), *line));
            }
            let mut content = lines.join("\n");
            content.push('\n');
            (kind.to_string(), content)
        })
        .collect()
}

/// The numeric minimum width of an `@media (min-width: …)` line, in px —
/// `em`/`rem` normalized at the CSS-initial 16px — or `None` for a non-media
/// line, or a width in units the styling system doesn't emit (those keep
/// their lexical position). `f64::to_bits` in the sort key above is
/// order-preserving because widths are non-negative.
fn media_min_width(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("@media (min-width: ")?;
    let close = rest.find(')')?;
    let number_end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(close);
    let number: f64 = rest[..number_end].parse().ok()?;
    match &rest[number_end..close] {
        "px" => Some(number),
        "em" | "rem" => Some(number * 16.0),
        _ => None,
    }
}

/// The frame trace, outermost of the shown frames first. A depth failure
/// unwinds hundreds of identical frames, so only the innermost few are shown;
/// `…` marks the ones dropped.
fn render_call_chain(trace: &[String]) -> String {
    const SHOWN: usize = 4;
    let chain = trace
        .iter()
        .take(SHOWN)
        .rev()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(" → ");
    if trace.len() > SHOWN {
        format!("… → {chain}")
    } else {
        chain
    }
}

/// The source regions where a fold would be meaningless: the bodies of every
/// function whose meaning depends on a TYPE PARAMETER (const-eval.md §9.1).
///
/// This is §5's recorded scope limit — const generics are out, and "a `const`
/// inside a generic function body is legal only if its initializer is
/// independent of the type parameters" — made operational. The explicit form
/// pushes that judgement onto the author, who wrote the keyword. Inference has
/// to make it itself, and the failure mode if it does not is the worst kind:
/// the const world walks the initializer with NO substitution
/// context, so `let total = T::default();` inside `List<T>::sum` does not fail
/// — it quietly evaluates to `undefined`, and the folded program prints
/// `undefined` where it used to print `0`. Found by the corpus differential on
/// `list-element-type.vl`, which is exactly the gate's job.
///
/// A function counts as type-parameter-dependent when its own generic
/// parameters, any parameter's type, or its return type mentions a `Generic` —
/// the receiver is what catches `List<T>`'s methods, whose own
/// `generic_parameter_constraint_ids` are empty because `T` belongs to the
/// type, not the method. Conservative on purpose: an unresolved or unknown type
/// counts too.
///
/// Stored per source as a merged, sorted, DISJOINT interval list, so nesting
/// (a closure inside a generic function) needs no special case and a
/// containment test is one binary search. See [`SpanRegions`].
struct GenericRegions {
    regions: SpanRegions,
}

impl GenericRegions {
    fn build(program: &Program) -> Self {
        let mut mentions = TypeParameterScan::new(program);
        let dependent: Vec<Id> = program
            .functions
            .iter()
            .filter(|(_, function)| {
                !function.generic_parameter_constraint_ids.is_empty()
                    || function
                        .return_type_id
                        .is_some_and(|type_id| mentions.reaches_a_type_parameter(type_id))
                    || function.parameters.iter().any(|parameter_id| {
                        program
                            .parameters
                            .get(parameter_id)
                            .is_some_and(|parameter| {
                                mentions.reaches_a_type_parameter(parameter.type_id)
                            })
                    })
            })
            .map(|(function_id, _)| *function_id)
            .collect();
        Self {
            regions: SpanRegions::of(program, &dependent),
        }
    }

    fn covers(&self, program: &Program, id: Id) -> bool {
        self.regions.contains(program, id)
    }
}

/// A set of source regions, indexed for containment: per source, the spans
/// sorted and merged into DISJOINT intervals, so a test is one binary search
/// instead of a scan of every region.
///
/// Merging is sound for the two things indexed here — function bodies and
/// `const` subtrees — because both are syntax subtrees, so two of them in one
/// file are nested or disjoint, never partially overlapping. A nested one is
/// absorbed by its enclosing one, which is the answer containment wants anyway.
struct SpanRegions {
    by_source: HashMap<u32, Vec<(usize, usize)>>,
}

impl SpanRegions {
    /// The index over `roots`' own spans. An entity with no source or no span
    /// contributes nothing, and is contained by nothing.
    fn of(program: &Program, roots: &[Id]) -> Self {
        let mut by_source: HashMap<u32, Vec<(usize, usize)>> = HashMap::default();
        for root in roots {
            let (Some(source), Some(span)) = (program.source_of(*root), program.span_map.get(root))
            else {
                continue;
            };
            by_source
                .entry(source.0)
                .or_default()
                .push((span.start, span.end));
        }
        for regions in by_source.values_mut() {
            regions.sort_unstable();
            let mut merged: Vec<(usize, usize)> = Vec::with_capacity(regions.len());
            for &(start, end) in regions.iter() {
                match merged.last_mut() {
                    Some(last) if start <= last.1 => last.1 = last.1.max(end),
                    _ => merged.push((start, end)),
                }
            }
            *regions = merged;
        }
        Self { by_source }
    }

    /// Whether `id`'s span lies inside one of the regions, in the same file.
    fn contains(&self, program: &Program, id: Id) -> bool {
        let (Some(source), Some(span)) = (program.source_of(id), program.span_map.get(&id)) else {
            return false;
        };
        let Some(regions) = self.by_source.get(&source.0) else {
            return false;
        };
        let index = regions.partition_point(|(start, _)| *start <= span.start);
        index > 0 && regions[index - 1].1 >= span.end
    }
}

/// Walks a `TypeId` looking for a type parameter, memoizing per type so a
/// deeply shared type is not re-walked once per function signature.
struct TypeParameterScan<'p, 'src> {
    program: &'p Program<'src>,
    seen: HashMap<TypeId, bool>,
}

impl<'p, 'src> TypeParameterScan<'p, 'src> {
    fn new(program: &'p Program<'src>) -> Self {
        Self {
            program,
            seen: HashMap::default(),
        }
    }

    fn reaches_a_type_parameter(&mut self, type_id: TypeId) -> bool {
        let mut visiting = HashSet::default();
        self.walk(type_id, &mut visiting)
    }

    fn walk(&mut self, type_id: TypeId, visiting: &mut HashSet<TypeId>) -> bool {
        if let Some(answer) = self.seen.get(&type_id) {
            return *answer;
        }
        // A recursive type (`enum Tree { Node(List<Tree>) }`) would otherwise
        // walk forever. Mid-cycle it contributes nothing.
        if !visiting.insert(type_id) {
            return false;
        }
        let answer = match self.program.type_id_to_type_map.get(&type_id) {
            // The type parameter itself, and the two "we do not know" cases —
            // conservative, since a fold under either is unverifiable.
            Some(Type::Generic(_)) | Some(Type::Unknown) | Some(Type::Unresolved) | None => true,
            Some(Type::Closure(arguments, result)) => {
                let result = *result;
                arguments
                    .clone()
                    .into_iter()
                    .chain(std::iter::once(result))
                    .any(|inner| self.walk(inner, visiting))
            }
            Some(Type::Enum(_, arguments))
            | Some(Type::Struct(_, arguments))
            | Some(Type::Trait(_, arguments))
            | Some(Type::Tuple(arguments)) => arguments
                .clone()
                .into_iter()
                .any(|inner| self.walk(inner, visiting)),
            Some(Type::Array(element, _)) => {
                let element = *element;
                self.walk(element, visiting)
            }
            Some(Type::Mapped(binder, source, template)) => {
                let (binder, source, template) = (*binder, *source, *template);
                self.walk(binder, visiting)
                    || self.walk(source, visiting)
                    || self.walk(template, visiting)
            }
            Some(_) => false,
        };
        visiting.remove(&type_id);
        self.seen.insert(type_id, answer);
        answer
    }
}

/// Whether `id` sits inside some `const` expression's span, in the same file.
fn within_a_const_expression(program: &Program, id: Id) -> bool {
    let Some(source) = program.source_of(id) else {
        return false;
    };
    let Some(span) = program.span_map.get(&id) else {
        return false;
    };
    program.const_exprs.iter().any(|root| {
        program.source_of(*root) == Some(source)
            && program
                .span_map
                .get(root)
                .is_some_and(|root_span| span.start >= root_span.start && span.end <= root_span.end)
    })
}

/// A span-sorted index of every `Expr::Local` reference, bucketed by source.
///
/// This exists because `free_locals` used to answer "which locals does this
/// subtree reference?" by scanning the WHOLE `entity_map` — 0.09–0.40 ms per
/// expression, which the explicit form could afford at a handful of `const`s
/// per program and inference could not at several hundred candidates
/// (const-eval.md §9.1: the unindexed sweep cost more than the entire
/// analysis, up to 173 % of it). §8.3 named the same waste while measuring the
/// LSP and left it; this takes it, and both forms get it.
///
/// References are keyed by their source and sorted by start, so a query is a
/// binary search to the root's span followed by a walk of just that range.
struct LocalIndex {
    /// Per `SourceId.0`: `(start, end, reference id, bound id)`, sorted.
    by_source: HashMap<u32, Vec<(usize, usize, Id, Id)>>,
}

impl LocalIndex {
    fn build(program: &Program) -> Self {
        let mut by_source: HashMap<u32, Vec<(usize, usize, Id, Id)>> = HashMap::default();
        for (id, expr) in &program.entity_map {
            if let Expr::Local(binding) = expr
                && let Some(source) = program.source_of(*id)
                && let Some(span) = program.span_map.get(id)
            {
                by_source
                    .entry(source.0)
                    .or_default()
                    .push((span.start, span.end, *id, *binding));
            }
        }
        for references in by_source.values_mut() {
            // Sorted by position, which is also the order diagnostics want.
            references.sort_unstable_by_key(|(start, end, id, _)| (*start, *end, id.0));
        }
        Self { by_source }
    }

    /// Every `Expr::Local` reference whose span lies within `root`'s (same
    /// file), paired with the binding it names.
    fn references_within(
        &self,
        program: &Program,
        root: Id,
    ) -> impl Iterator<Item = (Id, Id)> + '_ {
        // An unknown source or span yields an empty slice, so the bounds below
        // are never consulted.
        let (candidates, end) = self
            .lookup(program, root)
            .unwrap_or((&[] as &[(usize, usize, Id, Id)], 0));
        candidates
            .iter()
            .take_while(move |(start, ..)| *start < end)
            .filter(move |(_, reference_end, ..)| *reference_end <= end)
            .map(|(_, _, id, binding)| (*id, *binding))
    }

    /// The references at or after `root`'s span start, in `root`'s file, plus
    /// the span end the caller stops at.
    fn lookup(&self, program: &Program, root: Id) -> Option<(&[(usize, usize, Id, Id)], usize)> {
        let source = program.source_of(root)?;
        let root_span = **program.span_map.get(&root)?;
        let references = self.by_source.get(&source.0)?;
        let first = references.partition_point(|(start, ..)| *start < root_span.start);
        Some((&references[first..], root_span.end))
    }
}

struct State<'p, 'src> {
    program: &'p Program<'src>,
    /// Which form this is evaluating — see [`Mode`].
    mode: Mode,
    const_set: HashSet<Id>,
    /// The initializers the inference sweep is attempting. Empty in
    /// [`Mode::Explicit`]; in [`Mode::Inferred`] a candidate counts as
    /// compile-time-known, which is what makes `let a = 1 + 2; let b = a * 2;`
    /// fold both (const-eval.md §9.5).
    inferable: HashSet<Id>,
    locals: LocalIndex,
    /// The `const` subtrees, as a per-source interval index — see
    /// [`SpanRegions`] and [`State::in_const_subtree`].
    const_regions: SpanRegions,
    results: HashMap<Id, ConstValue>,
    assets: Vec<(String, String)>,
    failed: HashSet<Id>,
    in_progress: HashSet<Id>,
    errors: Vec<(Error, SourceId)>,
    /// `asset::read`'s host — present in [`Mode::Explicit`], `None` in
    /// [`Mode::Inferred`] (the inferred form's channels are closed, §9.2).
    reader: Option<&'p ProjectReader>,
}

/// How a const expression's free variable is (or isn't) compile-time-known.
enum Known<'src> {
    /// An item or a literal-initialized immutable binding — usable as-is.
    Ok,
    /// An immutable binding whose initializer is a `const` expression:
    /// evaluate that first.
    Const(Id),
    /// A runtime value — an error at the reference.
    Runtime(&'src str),
}

impl<'p, 'src> State<'p, 'src> {
    fn new(
        program: &'p Program<'src>,
        mode: Mode,
        inferable: HashSet<Id>,
        reader: Option<&'p ProjectReader>,
    ) -> Self {
        Self {
            program,
            mode,
            const_set: program.const_exprs.iter().copied().collect(),
            inferable,
            locals: LocalIndex::build(program),
            const_regions: SpanRegions::of(program, &program.const_exprs),
            results: HashMap::default(),
            assets: Vec::new(),
            failed: HashSet::default(),
            in_progress: HashSet::default(),
            errors: Vec::new(),
            reader,
        }
    }

    /// Records a diagnostic — or, in [`Mode::Inferred`], does not.
    ///
    /// This is the single place silent fallback is implemented (const-eval.md
    /// §9.2). Every failure path below calls it and then returns `false`; the
    /// `false` is what leaves the binding runtime, and in the inferred mode
    /// that is the ONLY thing that happens. Keeping it to one method is what
    /// makes "zero diagnostics, whatever went wrong" checkable by reading
    /// rather than by auditing every arm.
    fn report(&mut self, anchor: Id, error: Error) {
        if self.mode == Mode::Inferred {
            return;
        }
        self.errors.push((error, self.source_of(anchor)));
    }

    fn evaluate_one<'w>(&mut self, world: &mut transformer::ConstWorld<'w>, expr_id: Id) -> bool {
        if self.results.contains_key(&expr_id) {
            return true;
        }
        if self.failed.contains(&expr_id) {
            return false;
        }
        if !self.in_progress.insert(expr_id) {
            let error = Error {
                trace: Vec::new(),
                note: None,
                span: self.span_of(expr_id),
                msg: "`const` expressions form a dependency cycle".to_string(),
            };
            self.report(expr_id, error);
            self.failed.insert(expr_id);
            return false;
        }
        let ok = self.evaluate_inner(world, expr_id);
        self.in_progress.remove(&expr_id);
        if !ok {
            self.failed.insert(expr_id);
        }
        ok
    }

    fn evaluate_inner<'w>(&mut self, world: &mut transformer::ConstWorld<'w>, expr_id: Id) -> bool {
        // The free-variable rule, with precise spans at each reference.
        let mut ok = true;
        let free = self.free_locals(expr_id);
        let external: HashSet<Id> = free.iter().map(|(_, binding)| *binding).collect();
        for (reference_id, binding) in free {
            match self.classify(binding) {
                Known::Ok => {}
                Known::Const(dependency) => {
                    if !self.evaluate_one(world, dependency) {
                        ok = false;
                    }
                }
                Known::Runtime(name) => {
                    let error = Error {
                        trace: Vec::new(),
                        note: None,
                        span: self.span_of(reference_id),
                        msg: format!(
                            "`{name}` is a runtime value; a `const` expression reads only \
                             compile-time-known bindings"
                        ),
                    };
                    self.report(reference_id, error);
                    ok = false;
                }
            }
        }
        if !ok {
            return false;
        }

        // Assemble this site against the shared world (const-eval.md §10.6).
        // Bindings reached through CALLED functions surface as `unresolved` —
        // const-initialized ones get evaluated and the assembly retried;
        // anything else is a diagnostic.
        let mut attempts = 0;
        loop {
            let lower_started = crate::PhaseClock::now();
            let (reach, prelude, unresolved) = world.prepare(expr_id, &external, &self.results);
            phase_add(&PHASE_LOWER, lower_started);
            let mut retry = false;
            for binding in &unresolved {
                match self.classify(*binding) {
                    Known::Ok => {}
                    Known::Const(dependency) => {
                        if self.evaluate_one(world, dependency) {
                            retry = true;
                        } else {
                            ok = false;
                        }
                    }
                    Known::Runtime(name) => {
                        let error = Error {
                            trace: Vec::new(),
                            note: None,
                            span: self.span_of(expr_id),
                            msg: format!(
                                "this `const` expression reaches `{name}`, whose value is not \
                                 compile-time-known"
                            ),
                        };
                        self.report(expr_id, error);
                        ok = false;
                    }
                }
            }
            if !ok {
                return false;
            }
            if retry && attempts < 4 {
                attempts += 1;
                continue;
            }
            let lower_started = crate::PhaseClock::now();
            let site = world.site(expr_id, &reach, prelude);
            phase_add(&PHASE_LOWER, lower_started);
            return match self.mode {
                Mode::Explicit => {
                    let interp_started = crate::PhaseClock::now();
                    let reader = self
                        .reader
                        .map(|reader| reader as &dyn interpreter::AssetReader);
                    let evaluated = interpreter::eval_const(&site, EXPLICIT_LIMITS, reader);
                    phase_add(&PHASE_INTERP, interp_started);
                    match evaluated {
                        Ok(outcome) => {
                            FUEL_MAX.with(|cell| cell.set(cell.get().max(outcome.fuel_used)));
                            self.results.insert(expr_id, outcome.value);
                            self.assets.extend(outcome.assets);
                            true
                        }
                        Err(failure) => {
                            let frames = world.resolve_trace(&failure.trace);
                            let error = self.failure_error(expr_id, failure, &frames);
                            self.report(expr_id, error);
                            false
                        }
                    }
                }
                // The inferred form's tighter budgets, its closed effect
                // channels (both inside `eval_inferred`), and the size cap —
                // and any of the three missing is a silent fallback, which is
                // simply `false` with nothing reported (const-eval.md §9.2/3).
                Mode::Inferred => {
                    let interp_started = crate::PhaseClock::now();
                    let evaluated = interpreter::eval_inferred(&site, INFERRED_LIMITS);
                    phase_add(&PHASE_INTERP, interp_started);
                    match evaluated {
                        Ok(value) if value.literal_size() <= INFERRED_SIZE_CAP => {
                            self.results.insert(expr_id, value);
                            true
                        }
                        _ => false,
                    }
                }
            };
        }
    }

    /// The const-only capability check (const-eval.md §2): no RUNTIME call
    /// path may reach the compile-time channel — `asset::emit`, `asset::read`
    /// or `asset::bundle`. R = the functions/closures that reach one through call
    /// sites OUTSIDE `const` subtrees; roots (`main`, top-level initializers)
    /// never join R — a root's call into R is the offending boundary,
    /// reported at that call site.
    ///
    /// A call THROUGH a value resolves to `CallTarget::Indirect(Value)`, which
    /// carries no caller edge, so the fixpoint cannot follow it. §2's rule is
    /// therefore a refusal at the point the value is MADE: an R-member
    /// referenced as a function value, or an escaping R closure, outside every
    /// `const` subtree. Without it the escape is silent and the emitted JS
    /// carries a live `__emit_asset`/`__read_asset` call with no runtime
    /// binding.
    fn check_const_only(&mut self, graph: &CallGraph) {
        // The const-only set: the channel's three directions, in a fixed order
        // so an R-member reaching more than one is NAMED for the same one every
        // run.
        let const_only: Vec<Id> = [
            self.program.asset_emit_fn_id,
            self.program.asset_read_fn_id,
            self.program.asset_bundle_fn_id,
        ]
        .into_iter()
        .flatten()
        .collect();
        if const_only.is_empty() {
            return;
        }
        let main_id = self
            .program
            .scopes
            .get(&self.program.global_scope_id)
            .and_then(|scope| scope.name_to_id_map.get("main").copied());

        // Seed: nodes calling a const-only builtin directly through a
        // non-const site. `reaches` remembers WHICH builtin put each member in
        // R — it only names the diagnostic, so first-in wins is fine.
        let mut in_r: HashSet<Id> = HashSet::default();
        let mut reaches: HashMap<Id, Id> = HashMap::default();
        let mut worklist: Vec<Id> = Vec::new();
        let mut boundary_errors: Vec<(Id, Id)> = Vec::new(); // (call site, callee)
        let mut owned_calls: HashSet<Id> = HashSet::default();
        for node in graph.nodes() {
            for call in graph.calls_of(node.id()) {
                owned_calls.insert(call.call_id);
                let target = match call.target {
                    CallTarget::External(target) if const_only.contains(&target) => target,
                    _ => continue,
                };
                if self.in_const_subtree(call.call_id) {
                    continue;
                }
                if Some(node.id()) == main_id {
                    boundary_errors.push((call.call_id, target));
                } else if in_r.insert(node.id()) {
                    reaches.insert(node.id(), target);
                    worklist.push(node.id());
                }
            }
        }
        // Propagate to callers through non-const sites; roots never join.
        while let Some(member) = worklist.pop() {
            for caller in graph.callers_of(member) {
                let caller_id = caller.id();
                if in_r.contains(&caller_id) {
                    continue;
                }
                let sites: Vec<&Call> = graph
                    .calls_of(caller_id)
                    .iter()
                    .filter(|call| match call.target {
                        CallTarget::Function(target) | CallTarget::Closure(target) => {
                            target == member
                        }
                        _ => false,
                    })
                    .collect();
                for site in sites {
                    if self.in_const_subtree(site.call_id) {
                        continue;
                    }
                    if Some(caller_id) == main_id {
                        boundary_errors.push((site.call_id, member));
                    } else if in_r.insert(caller_id) {
                        if let Some(&builtin) = reaches.get(&member) {
                            reaches.insert(caller_id, builtin);
                        }
                        worklist.push(caller_id);
                    }
                }
            }
        }
        // Top-level initializers own no graph node: a direct-call site outside
        // every node whose subject resolves to a const-only builtin or an
        // R-function is the same boundary.
        for (call_id, function_call) in &self.program.function_calls {
            if owned_calls.contains(call_id) || self.in_const_subtree(*call_id) {
                continue;
            }
            let Some(Expr::Local(target)) = self.program.entity_map.get(&function_call.subject_id)
            else {
                continue;
            };
            if const_only.contains(target) || in_r.contains(target) {
                boundary_errors.push((*call_id, *target));
            }
        }
        // The key is the WHOLE tuple, because `dedup` compares the whole tuple
        // (backlog B93, E38's family): keyed on `span.start` alone, two distinct
        // tuples sharing a start can interleave with a repeat of the first, and
        // `dedup` — which only collapses ADJACENT equals — then lets the repeat
        // through. `span.start` stays the leading component so the order is the
        // one the diagnostics were always emitted in; the ids only break its
        // ties, and break them the same way every run.
        boundary_errors.sort_by_key(|(site, callee)| (self.span_of(*site).start, site.0, callee.0));
        boundary_errors.dedup();
        for (site, callee) in boundary_errors {
            let name = self.const_only_name(callee, &reaches);
            self.errors.push((
                Error {
                    trace: Vec::new(),
                    note: None,
                    span: self.span_of(site),
                    msg: format!(
                        "{name} is compile-time-only; evaluate this call inside a `const` \
                         expression"
                    ),
                },
                self.source_of(site),
            ));
        }

        self.check_value_escapes(graph, &in_r, &reaches);
    }

    /// The value-escape half of §2's rule. Two shapes make a runtime function
    /// value out of R, and both bypass the call-graph fixpoint:
    ///
    /// - an R-member NAMED as a value (fn-to-closure coercion) — the call graph
    ///   already separates these from call subjects in `function_references`;
    /// - an R closure that is never immediately applied — it joined R through
    ///   its own body, but nothing calls it by identity, so no boundary error
    ///   can fire for it.
    ///
    /// Both are refused at the site the value is made, which is also the
    /// narrowest span that identifies the problem (diagnostics-standard A1).
    /// A reference inside a `const` subtree is untouched: there the interpreter
    /// makes the call, which is the whole styling shape.
    fn check_value_escapes(
        &mut self,
        graph: &CallGraph,
        in_r: &HashSet<Id>,
        reaches: &HashMap<Id, Id>,
    ) {
        let mut escapes: Vec<(Id, Option<Id>)> = Vec::new(); // (site, named function)

        // `function_references` is keyed by every function node, every closure
        // node, and every module-level binding's initializer — the same key set
        // the graph itself walks. A `const`-marked initializer is skipped at
        // build time, so module-level const chains never appear here at all.
        let reference_owners = graph
            .nodes()
            .iter()
            .map(|node| node.id())
            .chain(self.program.module_level_bindings());
        for owner in reference_owners {
            for &(reference_id, function_id) in graph.function_references_of(owner) {
                if !in_r.contains(&function_id) || self.in_const_subtree(reference_id) {
                    continue;
                }
                escapes.push((reference_id, Some(function_id)));
            }
        }

        // An immediately-applied closure literal is `CallTarget::Closure`; any
        // R closure that is NOT one of those exists only as a value.
        let mut applied: HashSet<Id> = HashSet::default();
        for node in graph.nodes() {
            for call in graph.calls_of(node.id()) {
                if let CallTarget::Closure(target) = call.target {
                    applied.insert(target);
                }
            }
        }
        for binding in self.program.module_level_bindings() {
            for call in graph.initializer_calls_of(binding) {
                if let CallTarget::Closure(target) = call.target {
                    applied.insert(target);
                }
            }
        }
        for node in graph.nodes() {
            let Node::Closure(closure_id) = *node else {
                continue;
            };
            if !in_r.contains(&closure_id)
                || applied.contains(&closure_id)
                || self.in_const_subtree(closure_id)
            {
                continue;
            }
            escapes.push((closure_id, None));
        }

        // The whole tuple keys the sort, for the reason above (B93): this list
        // is the one that can actually repeat a tuple — a reference reachable
        // from two owners is pushed twice — so an interleaving neighbour at the
        // same `span.start` is exactly what would defeat the `dedup`.
        escapes.sort_by_key(|(site, function_id)| {
            (
                self.span_of(*site).start,
                site.0,
                function_id.map(|function_id| function_id.0),
            )
        });
        escapes.dedup();
        for (site, function_id) in escapes {
            let subject = match function_id {
                Some(function_id) => self.const_only_name(function_id, reaches),
                None => format!(
                    "this closure (it reaches `{}`)",
                    self.builtin_name(site, reaches)
                ),
            };
            self.errors.push((
                Error {
                    trace: Vec::new(),
                    note: None,
                    span: self.span_of(site),
                    msg: format!(
                        "{subject} is compile-time-only; call it directly inside a `const` \
                         expression — a compile-time-only function has no runtime value form"
                    ),
                },
                self.source_of(site),
            ));
        }
    }

    /// How a const-only callee names itself in a diagnostic: the builtin
    /// itself (`asset::emit` / `asset::read` / `asset::bundle`), or the
    /// R-member that reaches one — named for the builtin `reaches` recorded
    /// for it.
    fn const_only_name(&self, callee: Id, reaches: &HashMap<Id, Id>) -> String {
        if Some(callee) == self.program.asset_emit_fn_id {
            return "`asset::emit`".to_string();
        }
        if Some(callee) == self.program.asset_read_fn_id {
            return "`asset::read`".to_string();
        }
        if Some(callee) == self.program.asset_bundle_fn_id {
            return "`asset::bundle`".to_string();
        }
        let via = self.builtin_name(callee, reaches);
        self.program
            .functions
            .get(&callee)
            .map(|function| format!("`{}` (it reaches `{via}`)", function.name))
            .unwrap_or_else(|| format!("this closure (it reaches `{via}`)"))
    }

    /// The channel builtin an R-member was recorded as reaching — what the
    /// "(it reaches `…`)" parenthetical names. `asset::emit` is the fallback
    /// for a member with no record (unreachable in practice: every R-member is
    /// seeded or propagated with one).
    fn builtin_name(&self, member: Id, reaches: &HashMap<Id, Id>) -> &'static str {
        match reaches.get(&member) {
            Some(&builtin) if Some(builtin) == self.program.asset_read_fn_id => "asset::read",
            Some(&builtin) if Some(builtin) == self.program.asset_bundle_fn_id => "asset::bundle",
            _ => "asset::emit",
        }
    }

    /// An interpreter failure as a diagnostic. The primary span stays the
    /// `const` expression — the interpreted tree carries no positions, so
    /// there is no inner span to move to (const-eval.md §8.2) — but the frame
    /// trace names the function the failure happened in, and a secondary note
    /// anchors at that function's declaration so the editor can reach it.
    /// A std frame is legal in a note and would not be legal as the primary
    /// span (diagnostics-standard A2, C3).
    ///
    /// `frames` is the trace resolved to the functions that emitted it
    /// (`ConstWorld::resolve_trace`) — attribution by identity, not by the
    /// emitted name, which is a generated artifact and never was the source's
    /// (const-eval.md §10.6). A frame that resolves to nothing is a synthetic
    /// or monomorphized one, which B1 says must never reach a user.
    fn failure_error(
        &self,
        expr_id: Id,
        failure: interpreter::Failure,
        frames: &[Option<Id>],
    ) -> Error {
        // The kind stops at the const boundary: a budget miss is not a program
        // error, and §4 promised it says so.
        let headline = match failure.kind {
            FailureKind::Fuel | FailureKind::Depth => {
                "const evaluation did not finish within the compile-time budget"
            }
            _ => "const evaluation failed",
        };
        let source_name = |function_id: Id| self.program.functions[&function_id].name;
        let (subject, note) = match frames.first().copied().flatten() {
            None => (String::new(), None),
            Some(function_id) => {
                let name = source_name(function_id);
                let source = self.source_of(function_id);
                let note = Note {
                    // The name, not the whole declaration (A1) — and the
                    // file only when it differs from the primary span's.
                    span: self.program.functions[&function_id].name_span,
                    msg: if failure.trace.len() > 1 {
                        let chain: Vec<String> = frames
                            .iter()
                            .zip(&failure.trace)
                            .map(|(resolved, emitted)| match resolved {
                                Some(id) => source_name(*id).to_string(),
                                None => emitted.clone(),
                            })
                            .collect();
                        format!("the compile-time call chain: {}", render_call_chain(&chain))
                    } else {
                        format!("`{name}` is declared here")
                    },
                    source: (source != self.source_of(expr_id)).then_some(source),
                };
                (format!(" in `{name}`"), Some(note))
            }
        };
        Error {
            trace: Vec::new(),
            note,
            span: self.span_of(expr_id),
            msg: format!("{headline}{subject}: {}", failure.message),
        }
    }

    /// The file an anchor entity's span indexes into — the file its diagnostic
    /// renders in (backlog E16); a synthetic entity falls back to the entry, and
    /// generated code to the file that wrote the attribute.
    fn source_of(&self, id: Id) -> SourceId {
        self.program.diagnostic_source_of(id)
    }

    /// Whether an entity sits inside any `const` expression's span (same
    /// source file) — the site test the capability check cuts edges by.
    ///
    /// Answered from the interval index rather than by scanning every `const`
    /// expression: `check_const_only` asks this once per call site in the
    /// program, so the scan was the pass's one super-linear term — O(call sites
    /// × const sites), 64 ms of the website's server entry at 210 sites and
    /// growing as the square (const-eval.md §10).
    fn in_const_subtree(&self, id: Id) -> bool {
        self.const_regions.contains(self.program, id)
    }

    /// The free local references of the const subtree: every `Expr::Local`
    /// whose span lies inside the expression's span (same source file), minus
    /// bindings DECLARED inside it (block `let`s, closure parameters — their
    /// references are internal, not free).
    ///
    /// Answered from [`LocalIndex`], which is why the inference sweep is
    /// affordable at all (const-eval.md §9.1).
    fn free_locals(&self, root: Id) -> Vec<(Id, Id)> {
        let root_span = self.span_of(root);
        let Some(root_source) = self.program.source_of(root) else {
            return Vec::new();
        };
        let declared_within =
            |id: Id| -> bool {
                self.program.source_of(id) == Some(root_source)
                    && self.program.span_map.get(&id).is_some_and(|span| {
                        span.start >= root_span.start && span.end <= root_span.end
                    })
            };
        // The index yields references in span order already — the order
        // diagnostics want.
        self.locals
            .references_within(self.program, root)
            .filter(|(_, binding)| !declared_within(*binding))
            .collect()
    }

    fn classify(&self, binding: Id) -> Known<'src> {
        if let Some(parameter) = self.program.parameters.get(&binding) {
            return Known::Runtime(parameter.name);
        }
        if let Some(variable) = self.program.variables.get(&binding) {
            if variable.mutable {
                return Known::Runtime(variable.name);
            }
            let Some(initial) = variable.initial else {
                return Known::Runtime(variable.name);
            };
            if self.const_set.contains(&initial) {
                return Known::Const(initial);
            }
            // A binding the sweep is itself attempting counts as
            // compile-time-known, so chains fold (const-eval.md §9.5). ONLY in
            // the inferred mode: an explicit `const` reading a plain runtime
            // binding must keep erroring with §1's message, or the same program
            // would fail in debug and compile in release.
            if self.mode == Mode::Inferred && self.inferable.contains(&initial) {
                return Known::Const(initial);
            }
            let literal = matches!(
                self.program.entity_map.get(&initial),
                Some(
                    Expr::String(_)
                        | Expr::MultilineString(_)
                        | Expr::Number(..)
                        | Expr::Bool(_)
                        | Expr::Null
                )
            );
            if literal {
                return Known::Ok;
            }
            return Known::Runtime(variable.name);
        }
        // Items — functions, structs, enum constructors — are code, not
        // runtime state; the const world lowers them.
        Known::Ok
    }

    fn span_of(&self, id: Id) -> Span {
        self.program
            .span_map
            .get(&id)
            .map(|span| **span)
            .unwrap_or((0..0).into())
    }
}
