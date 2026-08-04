//! The std twin-surface parity gate (backlog E34).
//!
//! `std` is a LAYERED library (`vilan/std/vilan.toml`): a module may exist once
//! in `src/` and serve every platform, or twice — once in `src/browser`, once in
//! `src/process` — as a TWIN, two implementations of one import path. A twin's
//! two halves must declare the SAME surface, because the same component source
//! is compiled through both: the browser build for the live DOM, the process
//! build for the SSR render and for the layer-requirement analysis that a
//! process build runs over browser modules.
//!
//! Nothing structurally held the halves against each other, and it bit twice
//! during the bundle-splitting arc: `std::router` re-exports `ui::chunk_pending`
//! and is analyzed in process builds too, so a browser-only `chunk_pending` left
//! `std::router` uncompilable there (`proposal/bundle-splitting.md`, closing
//! note). The pins that caught it were incidental. This is the standing gate.
//!
//! **The mechanism is the compiler's own answer, not a scan of the text.** Each
//! twin is ANALYZED on its platform, and the surface is read off the resulting
//! `Program`: the module scope's bindings filtered to those DECLARED in that
//! twin's own file (so the two halves' differing `import`s never read as
//! divergence), plus the members of every type declared there — `View.swap_split`
//! is a real divergence that only the member half sees.
//!
//! **What it does not compare:** signatures. `on_event` is
//! `|Event| void` in the browser and generic `|E| void` on the process side,
//! deliberately — a server layer cannot name the browser-only `std::dom::Event`,
//! and the handler is discarded anyway (process/ui.vl documents it). Names are
//! the surface that breaks a build at analysis; shapes are held by
//! `ssr_differential.rs`.
//!
//! Divergences that ARE intentional live in [`ALLOWED_DIVERGENCES`], each with
//! the reason it is intentional. That list is held honest in both directions: an
//! entry naming something the twins actually share, or something neither
//! declares, fails too — a stale allowlist would silently re-open the gap.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use vilan_core::analyzer::{Program, SourceId};
use vilan_core::type_::Type;
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

/// Which half of a twin declares a name the other does not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    BrowserOnly,
    ProcessOnly,
}

impl Side {
    /// The side that would have to grow the name to close the divergence — what
    /// a failure message must say, since "these differ" is not actionable.
    fn missing_from(self) -> &'static str {
        match self {
            Side::BrowserOnly => "the PROCESS twin (vilan/std/src/process)",
            Side::ProcessOnly => "the BROWSER twin (vilan/std/src/browser)",
        }
    }
}

/// The twins that must exist, and are gated below. Adding a module to BOTH
/// `src/browser` and `src/process` creates a new twin and fails
/// [`the_twin_inventory_is_known`] until it is listed here — which is the point:
/// a second twinned module inherits this gate rather than quietly going
/// unguarded.
///
/// `ui` is the only twin today. Every other layer module is single-platform
/// (browser: `dev`, `dom`, `router`, `storage`; process: `db`, `fs`, `http`,
/// `process`, `rpc_server`) and has no counterpart to be held against — a
/// missing name there is already a hard cross-platform import error at the use
/// site, not a silent surface drift. `time` and every other shared module lives
/// in the base layer, compiled once for both platforms, so it cannot diverge at
/// all.
const TWINNED_MODULES: &[&str] = &["ui"];

/// Names one twin declares and the other deliberately does not: `(module, name,
/// side, why)`. A member is spelled `Type.member`.
const ALLOWED_DIVERGENCES: &[(&str, &str, Side, &str)] = &[
    // --- browser-only ------------------------------------------------------
    (
        "ui",
        "mount",
        Side::BrowserOnly,
        "BY DESIGN: mounting a component into a live document is a CLIENT entry \
         point. A server render has no document — the natural factoring is a \
         shared `fun app(): View` and a per-leg `main` (process/ui.vl's OMITTED \
         note, proposal/ssr.md §2/§6a).",
    ),
    (
        "ui",
        "mount_root",
        Side::BrowserOnly,
        "BY DESIGN, with `mount`: the reactive root a mount establishes is a \
         browser-lifetime thing. A request render is create-serialize-discard — \
         no owner survives it.",
    ),
    (
        "ui",
        "is_svg_tag",
        Side::BrowserOnly,
        "A private helper, not a surface: the browser routes SVG tags through \
         `createElementNS`, while the process twin seeds the `xmlns` attribute on \
         an `svg` root inside `view` and lets descendants inherit it. The SVG \
         STORY is mirrored (both files carry KEEP-IN-STEP notes); the mechanism \
         differs, so the helper does not exist on both sides.",
    ),
    (
        "ui",
        "chunk_arm",
        Side::BrowserOnly,
        "A route-chunk host intrinsic. Splitting is opt-in per BROWSER entry \
         (proposal/bundle-splitting.md §2/§4), so a process build has no chunk \
         map to select an arm from.",
    ),
    (
        "ui",
        "chunk_ready",
        Side::BrowserOnly,
        "With `chunk_arm`: a browser-only chunk-presence test. A server render \
         runs against code that is already loaded.",
    ),
    (
        "ui",
        "chunk_load",
        Side::BrowserOnly,
        "With `chunk_arm`: fetches a chunk. Nothing is ever in flight on the \
         server.",
    ),
    (
        "ui",
        "View.swap_split",
        Side::BrowserOnly,
        "Emitter-selected, never written: a split build retargets a splittable \
         route match's `swap` to this, and `chunks.rs` builds that gate only when \
         the method exists (`view_method(program, \"swap_split\")`). No source \
         names it and a process build never splits, so its absence degrades the \
         gate away rather than breaking a build. CONTRAST `chunk_pending`, the \
         one chunk-machinery name user code DOES bind (through \
         `std::router::pending`) — it is mirrored on both sides, and its absence \
         is exactly what E34 was filed for.",
    ),
    // --- process-only ------------------------------------------------------
    (
        "ui",
        "render",
        Side::ProcessOnly,
        "BY DESIGN: serializes the built tree to markup — the process layer's \
         entire reason for existing. A browser view IS the document already; \
         there is nothing to serialize.",
    ),
    (
        "ui",
        "Attribute",
        Side::ProcessOnly,
        "Part of the string-tree REPRESENTATION: the process `View` is a tag plus \
         ordered attributes and children, where the browser `View` wraps a live \
         `std::dom::Element` and stores nothing itself.",
    ),
    (
        "ui",
        "Child",
        Side::ProcessOnly,
        "With `Attribute`: the ordered child list (an element or a text node), \
         which the DOM holds for the browser twin.",
    ),
    (
        "ui",
        "set_attribute",
        Side::ProcessOnly,
        "The set-or-replace-by-name helper that makes the string tree match the \
         DOM's own `setAttribute` semantics (repeat updates in place, new \
         appends). The browser twin calls the DOM for this.",
    ),
    (
        "ui",
        "is_void_element",
        Side::ProcessOnly,
        "A serialization detail behind `render`: void elements take no closing \
         tag. Meaningless against a live DOM.",
    ),
    (
        "ui",
        "escape_text",
        Side::ProcessOnly,
        "A serialization detail behind `render`. The DOM escapes on its own — \
         setting `textContent` never needs it.",
    ),
    (
        "ui",
        "escape_attribute",
        Side::ProcessOnly,
        "With `escape_text`: attribute-position escaping for the serializer.",
    ),
];

fn std_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(&std_root())
}

/// The module stems declared by a layer directory (`src/browser` -> `dom`,
/// `router`, `ui`, ...).
fn layer_modules(layer: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let Ok(entries) = std::fs::read_dir(std_root().join("src").join(layer)) else {
        panic!("std has no `{layer}` layer directory");
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "vl") {
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            if stem != "lib" {
                names.insert(stem);
            }
        }
    }
    names
}

/// Analyze a program that imports `module` on `platform` and read the surface
/// the compiler resolved off the `Program`.
fn surface(module_name: &str, platform: Platform) -> BTreeSet<String> {
    let source: &'static str =
        Box::leak(format!("import std::{module_name};\nfun main() {{}}\n").into_boxed_str());
    let owned_name = module_name.to_string();
    let collected = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &std_spec(),
                Path::new("."),
                Path::new("twin_parity.vl"),
                Some(platform),
                &Workspace::default(),
            );
            assert!(
                errors.is_empty(),
                "`import std::{owned_name}` must analyze cleanly on {platform:?}: {errors:?}"
            );
            let program = program.expect("a program");
            declared_surface(&program, &owned_name)
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked");
    assert!(
        !collected.is_empty(),
        "empty surface for `{module_name}` on {platform:?} — the reader is broken, \
         not the twins"
    );
    collected
}

/// The names `module_name` contributes, as the analyzer resolved them: module
/// scope bindings DECLARED in that module's own file, plus the members of every
/// type declared there (`View.text`, `Slot.place`, ...).
///
/// The source filter is what keeps the comparison honest — a module scope also
/// holds its `import`s, and the twins import different things (`std::dom` on one
/// side, nothing like it on the other), which is not surface divergence.
fn declared_surface(program: &Program<'_>, module_name: &str) -> BTreeSet<String> {
    let file_name = format!("{module_name}.vl");
    let source_id = program
        .sources
        .iter()
        .position(|path| {
            path.file_name()
                .is_some_and(|name| name == file_name.as_str())
        })
        .map(|index| SourceId(index as u32))
        .unwrap_or_else(|| {
            panic!(
                "`{file_name}` was not loaded; sources: {:?}",
                program.sources
            )
        });

    let module = program
        .modules
        .values()
        .find(|module| module.name == module_name)
        .unwrap_or_else(|| panic!("no module named `{module_name}` in the analyzed program"));
    let scope = program
        .scopes
        .get(&module.body.1)
        .expect("the module's own scope");

    let mut names = BTreeSet::new();
    for (name, id) in &scope.name_to_id_map {
        if program.source_of(*id) == Some(source_id) {
            names.insert((*name).to_string());
        }
    }

    for implementation in &program.implementations {
        let Some(subject) = program.type_id_to_type_map.get(&implementation.subject) else {
            continue;
        };
        let subject_id = match subject {
            Type::Struct(id, _) | Type::Enum(id, _) | Type::Trait(id, _) => *id,
            _ => continue,
        };
        if program.source_of(subject_id) != Some(source_id) {
            continue;
        }
        let subject_name = program
            .structs
            .get(&subject_id)
            .map(|struct_| struct_.name.to_string())
            .or_else(|| program.enums.get(&subject_id).map(|e| e.name.to_string()))
            .or_else(|| program.traits.get(&subject_id).map(|t| t.name.to_string()))
            .unwrap_or_else(|| format!("{subject_id:?}"));
        for member in implementation.declarations.keys() {
            names.insert(format!("{subject_name}.{member}"));
        }
    }
    names
}

fn allowed_for(module: &str) -> BTreeMap<&'static str, (Side, &'static str)> {
    ALLOWED_DIVERGENCES
        .iter()
        .filter(|(allowed_module, ..)| *allowed_module == module)
        .map(|(_, name, side, reason)| (*name, (*side, *reason)))
        .collect()
}

/// A module in both layer directories is a twin and must be gated. This fails
/// on a NEW twin, so the gate widens with std rather than being pinned to `ui`.
#[test]
fn the_twin_inventory_is_known() {
    let twins: BTreeSet<String> = layer_modules("browser")
        .intersection(&layer_modules("process"))
        .cloned()
        .collect();
    let gated: BTreeSet<String> = TWINNED_MODULES
        .iter()
        .map(|name| name.to_string())
        .collect();
    assert_eq!(
        twins, gated,
        "the set of modules implemented in BOTH std layers changed. A new twin \
         must be added to `TWINNED_MODULES` (and given its own allowlist \
         entries); a removed one must be dropped from it."
    );
}

/// The gate: every twin's two halves declare the same names, except the
/// divergences recorded as deliberate.
#[test]
fn twinned_modules_declare_the_same_surface() {
    for module in TWINNED_MODULES {
        let browser = surface(module, Platform::Browser);
        let process = surface(module, Platform::default());
        let allowed = allowed_for(module);

        let mut unjustified: Vec<String> = Vec::new();
        for name in browser.difference(&process) {
            match allowed.get(name.as_str()) {
                Some((Side::BrowserOnly, _)) => {}
                _ => unjustified.push(format!(
                    "`std::{module}::{name}` is declared by the browser twin but missing from {}",
                    Side::BrowserOnly.missing_from()
                )),
            }
        }
        for name in process.difference(&browser) {
            match allowed.get(name.as_str()) {
                Some((Side::ProcessOnly, _)) => {}
                _ => unjustified.push(format!(
                    "`std::{module}::{name}` is declared by the process twin but missing from {}",
                    Side::ProcessOnly.missing_from()
                )),
            }
        }
        assert!(
            unjustified.is_empty(),
            "the `std::{module}` twins' surfaces diverge:\n  {}\n\nMirror the name \
             on the missing side, or — if the divergence is deliberate — add it to \
             `ALLOWED_DIVERGENCES` with the reason it is deliberate.",
            unjustified.join("\n  ")
        );
    }
}

/// The allowlist is held honest: every entry must name a real, still-present
/// divergence on the side it claims. A stale entry is how this gate would go
/// quietly vacuous — a name mirrored later, or removed, leaving a standing
/// exemption for whatever takes its place.
#[test]
fn every_allowed_divergence_is_real() {
    for module in TWINNED_MODULES {
        let browser = surface(module, Platform::Browser);
        let process = surface(module, Platform::default());
        for (allowed_module, name, side, _) in ALLOWED_DIVERGENCES {
            if allowed_module != module {
                continue;
            }
            let (declaring, other, declaring_label, other_label) = match side {
                Side::BrowserOnly => (&browser, &process, "browser", "process"),
                Side::ProcessOnly => (&process, &browser, "process", "browser"),
            };
            assert!(
                declaring.contains(*name),
                "`ALLOWED_DIVERGENCES` claims `std::{module}::{name}` is \
                 {declaring_label}-only, but the {declaring_label} twin does not \
                 declare it — drop the stale entry."
            );
            assert!(
                !other.contains(*name),
                "`ALLOWED_DIVERGENCES` exempts `std::{module}::{name}` as \
                 {declaring_label}-only, but the {other_label} twin declares it \
                 too — the twins agree here, so drop the exemption and let the \
                 gate hold the name."
            );
        }
    }
}

/// Every allowlist entry carries a REASON, not just a name: the list is the
/// record of why each divergence is intentional, and an empty reason makes it a
/// mute exemption.
#[test]
fn every_allowed_divergence_states_why() {
    for (module, name, _, reason) in ALLOWED_DIVERGENCES {
        assert!(
            reason.len() > 40,
            "`std::{module}::{name}`'s allowlist entry needs a real reason, not {reason:?}"
        );
    }
}
