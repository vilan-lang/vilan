//! E124's dead top-level paint (`proposal/dead-code-paint.md`): which of a
//! package's top-level declarations NO entry reaches, and which of its modules
//! NO entry loads.
//!
//! Two questions, two granularities, one definition of "reached" — the pruner's
//! own walk, never the emitted output (§1.4, determination 2). Everything here
//! computes; nothing here decides what an editor draws.
//!
//! **The definition is narrow, and the narrowing is measured.** Top-level gray
//! covers exactly two item kinds — a top-level `fun` and a module-level `let`.
//! A `struct`, an `enum`, a `trait` and an `impl` block are never gray, used or
//! unused: they are type-level only, the transformer has no arm for them, and a
//! USED `Point { x = 1, y = 2 }` emits `[ 1, 2 ]` with no declaration anywhere
//! (§1.2, probes P1/P2). "Dead by the bundle's own definition" is not defined
//! for them and cannot be made so by any refinement of the pruner. Finding an
//! unreferenced type is a type-REFERENCE analysis with a different index; it is
//! not this one.
//!
//! **The exemptions are the reason this ships at all.** Measured on kolt, the
//! ruled definition grayed 1,859 of 1,943 top-level items — 95.7% — of which
//! sixteen were true finds. Four classes account for the rest, and three of
//! them are handled here or by the passes this module reads:
//!
//! - a declared `generated` root (1,815 items — [`is_generated`]);
//! - a callee reached only from a `const` module-binding initializer (27 —
//!   `CallGraph`'s paint-only const edges, followed by
//!   [`crate::platform_color::paint_reachable_nodes`]);
//! - a binding `context::thread_contexts` rewrites away (1 —
//!   `Program::context_bindings`);
//! - every type declaration (all of them — the narrowing above).
//!
//! A suppressor marker could not have been the answer: it would have taken
//! 1,859 of them on kolt, 1,815 in a file a build hook rewrites on the next tag
//! bump, which is precisely the file-switching cost the owner's reservation
//! refuses.

use std::path::{Path, PathBuf};

use crate::analyzer::{DERIVED_SOURCE, Program, SourceId};
use crate::fx::FxHashSet as HashSet;
use crate::id::Id;
use crate::manifest::Manifest;
use crate::platform_color::{PlatformChoice, PlatformReason};
use crate::span::Span;

/// A top-level item's identity ACROSS analyses: the file it is declared in and
/// the span of its name.
///
/// Entity ids cannot serve. They are minted per analysis, in file-walk order,
/// so the id of `theme.vl`'s `paint_ink1` under the `client` entry's program is
/// a different number from its id under the `server` entry's — the union of
/// three entries' reachability is only expressible on a key both programs agree
/// on. `Function::name_span` is that key: unique per declaration, and already
/// the anchor go-to-definition and rename hang on.
///
/// The path is canonical on both sides (`Program::canonical_sources`), so a
/// source reached through a symlink and one reached directly are one item.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemKey {
    pub path: PathBuf,
    pub name_span: Span,
}

/// Which of the two paintable kinds an item is — carried so a consumer can word
/// its own message, not because the paint treats them differently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    /// A top-level `fun`, including an inherent impl member.
    Function,
    /// A module-level `let`.
    Binding,
}

/// One top-level declaration the paint MAY gray, in one file of one program.
#[derive(Clone, Debug)]
pub struct TopLevelItem {
    pub id: Id,
    pub kind: ItemKind,
    pub name: String,
    pub name_span: Span,
}

/// Every top-level item of `source` that the paint may gray — the candidates,
/// before any reachability is consulted.
///
/// The exemptions applied here are the ones that hold whatever the walk says,
/// each pinned in `crates/vilan-core/tests/dead_items.rs`:
///
/// - **`main`** — the root itself, and a root cannot be unreached.
/// - **an `_`-led name** — the language's own "I know" marker, exactly as
///   E114's locals paint reads it (`let _page_defaults = const …` in kolt is
///   `_`-led *and* const, and the `_` alone should keep it quiet).
/// - **an ambient context binding** — `Program::context_bindings`, the
///   declarations `context::thread_contexts` rewrites out of the graph (§1.7).
/// - **a trait member and a trait-impl member** — the walk's dispatch
///   refinement is PER INSTANTIATION, so whether `Ci::area` is gray depends on
///   whether any entry constructs a `Ci`, and constructing one anywhere makes a
///   dozen grays vanish at once (§1.8). Correct by the definition, and the
///   class most likely to read as noise, so v1 leaves it unpainted. An
///   INHERENT impl member stays paintable — kolt's twelve-member unused overlay
///   API is exactly that shape, and it is the find the paint exists for.
/// - **a signature-only trait method** — no body, nothing to delete.
/// - **anything derive- or service-generated** — those entities carry
///   [`DERIVED_SOURCE`], which is outside `sources` and therefore outside every
///   real file's id range, so asking for a real `source` never returns one. The
///   guard below is belt-and-braces for a caller that passes the sentinel.
///
/// What is NOT exempted here, because the walk answers it correctly on its own:
/// an `[rpc]` method (reached through the `dispatcher(self)` the analyzer
/// synthesizes, so an installed service's methods are reached and an
/// uninstalled service's are genuinely dead), `[expose]` (a struct-FIELD
/// attribute with no bearing on items at all), and a `[platform]`-fenced item
/// (each entry walks under its own platform, so a browser-only item lives in
/// the union through the browser entry — §1.5).
pub fn paintable_items(program: &Program, source: SourceId) -> Vec<TopLevelItem> {
    if source == DERIVED_SOURCE {
        return Vec::new();
    }
    let ranges = program.id_ranges_of(source);
    let declared_here = |id: Id| ranges.iter().any(|range| range.contains(&id.0));
    let exempt = exempt_ids(program);
    let mut items: Vec<TopLevelItem> = Vec::new();
    for (id, function) in &program.functions {
        if !declared_here(*id) || !function.has_body {
            continue;
        }
        if function.name == "main" || function.name.starts_with('_') || exempt.contains(id) {
            continue;
        }
        items.push(TopLevelItem {
            id: *id,
            kind: ItemKind::Function,
            name: function.name.to_string(),
            name_span: function.name_span,
        });
    }
    for id in program.module_level_bindings() {
        if !declared_here(id) || exempt.contains(&id) {
            continue;
        }
        let Some(variable) = program.variables.get(&id) else {
            continue;
        };
        if variable.name.starts_with('_') {
            continue;
        }
        items.push(TopLevelItem {
            id,
            kind: ItemKind::Binding,
            name: variable.name.to_string(),
            name_span: variable.name_span,
        });
    }
    items.sort_unstable_by_key(|item| (item.name_span.start, item.name_span.end));
    items.dedup_by_key(|item| item.name_span);
    items
}

/// The ids [`paintable_items`] never offers: trait members, trait-impl members,
/// and the ambient context bindings. See that function for why each is here.
fn exempt_ids(program: &Program) -> HashSet<Id> {
    let mut exempt: HashSet<Id> = program.context_bindings.iter().copied().collect();
    for trait_ in program.traits.values() {
        exempt.extend(trait_.declarations.values().copied());
    }
    for implementation in &program.implementations {
        if implementation.trait_ids.is_empty() {
            continue;
        }
        exempt.extend(implementation.declared_members.iter().map(|(_, id)| *id));
    }
    exempt
}

/// Every top-level item ANY code reachable from this program's `main` reaches,
/// keyed so the answer survives the analysis that produced it.
///
/// `None` when the program has no `main`. That is not an edge case in the
/// editor — it is the common case: the language server analyzes the OPEN file
/// as the entry, so for nine of kolt's twelve hand-written files there is no
/// `main` in the program at all and the walk has no root to start from (§2.1,
/// probe P5). It is the whole reason the per-entry sets are computed out of
/// band, on a package clock, instead of read off the analysis in hand.
///
/// The union across a package's entries is the union of these sets, and taking
/// it is the caller's job: an item reached by exactly one entry of three is
/// reached, and a `[platform]`-fenced item is reached by the entry whose
/// platform it is fenced to.
pub fn reached_item_keys(program: &Program) -> Option<HashSet<ItemKey>> {
    let reached = crate::platform_color::paint_reachable_nodes(program)?;
    let index = SourceIndex::build(program);
    let module_level: HashSet<Id> = program.module_level_bindings().into_iter().collect();
    let mut keys: HashSet<ItemKey> = HashSet::default();
    for id in reached {
        let Some(path) = index.path_of(program, id) else {
            continue;
        };
        if let Some(function) = program.functions.get(&id) {
            keys.insert(ItemKey {
                path: path.to_path_buf(),
                name_span: function.name_span,
            });
        } else if module_level.contains(&id)
            && let Some(variable) = program.variables.get(&id)
        {
            keys.insert(ItemKey {
                path: path.to_path_buf(),
                name_span: variable.name_span,
            });
        }
    }
    Some(keys)
}

/// `Program::source_of` inverted and made cheap for a whole-program filter: the
/// id ranges sorted once, so "which file is this id from" is a binary search
/// rather than a linear scan of `source_ranges` re-run per row (the same
/// reasoning `Program::id_ranges_of` records for E114's paint walks).
struct SourceIndex {
    ranges: Vec<(u32, u32, usize)>,
}

impl SourceIndex {
    fn build(program: &Program) -> SourceIndex {
        let mut ranges: Vec<(u32, u32, usize)> = program
            .source_ranges
            .iter()
            .filter(|range| (range.source.0 as usize) < program.canonical_sources.len())
            .map(|range| (range.start, range.end, range.source.0 as usize))
            .collect();
        ranges.sort_unstable();
        SourceIndex { ranges }
    }

    fn path_of<'a>(&self, program: &'a Program, id: Id) -> Option<&'a Path> {
        let slot = self.ranges.partition_point(|(start, _, _)| *start <= id.0);
        let (start, end, source) = *self.ranges.get(slot.checked_sub(1)?)?;
        (id.0 >= start && id.0 < end)
            .then(|| program.canonical_sources.get(source).map(PathBuf::as_path))
            .flatten()
    }
}

/// Whether `file` lives under the package's declared `generated` root — in
/// which case it gets NO top-level gray, at either granularity.
///
/// Not a new key and not a new marker: `[package] generated = "src/lucide"`
/// already exists, kolt already sets it, `vilan fmt` already leaves everything
/// under it byte-identical, and it already means "this is not code you
/// maintain". Applying it here is what stops the paint fading an 18,198-line
/// machine-written file wall to wall, on every keystroke, forever, where the
/// correct user response is to do nothing: the file is regenerated from a
/// pinned upstream tag and its exhaustiveness is its purpose (§1.5,
/// determination 3).
///
/// Locals, unused imports and unreachable code stay painted under a generated
/// root — they are file-local and harmless. Only the top-level gray stops.
pub fn is_generated(manifest_dir: &Path, manifest: &Manifest, file: &Path) -> bool {
    let Some(generated) = manifest.generated_root() else {
        return false;
    };
    let root = crate::util::canonical_path(&manifest_dir.join(generated));
    crate::util::canonical_path(file).starts_with(&root)
}

/// The names of the package's entries when NO entry loads `file` — E124's
/// module-level slice, and the whole of it.
///
/// `Some(entries)` means the file is a package module that nothing builds:
/// every top-level item in it is dead, and so is the file. `None` means there
/// is nothing to say — an entry reaches it, it is not the package's to judge
/// (outside the source root, or under the declared `generated` root), or the
/// manifest is a `[library]`, which has no entries and therefore no union and
/// therefore no gray at all (§4, determination 9).
///
/// **`choices` is [`crate::platform_color::file_platform_choices`]' answer for
/// the same file**, passed in rather than recomputed. That walk — the per-entry
/// module-level reachability the language server already runs per keystroke to
/// color the file — IS this answer for a multi-entry package: a choice with
/// reason `ReachedBy` means an entry loads the file, and its absence means none
/// does. So the slice costs nothing where the package has `[entry.<name>]`
/// sections. The classic single-entry form (`[package] entry = …`) colors every
/// file under the root by the package target without walking anything, so there
/// the one walk is paid here.
pub fn unreached_module_entries(
    manifest_dir: &Path,
    manifest: &Manifest,
    file: &Path,
    choices: &[PlatformChoice],
) -> Option<Vec<String>> {
    let package = manifest.package.as_ref()?;
    let pkg_root = crate::util::canonical_path(&manifest_dir.join(package.root()));
    let file = crate::util::canonical_path(file);
    if !file.starts_with(&pkg_root) || is_generated(manifest_dir, manifest, &file) {
        return None;
    }
    if manifest.entries.is_empty() {
        // The classic single-entry package. `file_platform_choices` answers
        // `PackageTarget` here without walking, so the walk is this function's
        // to pay — once, over one entry.
        let entry = pkg_root.join(package.entry());
        if crate::analyzer::package_modules_reachable_from(&entry, &pkg_root).contains(&file) {
            return None;
        }
        return Some(vec![package.entry().display().to_string()]);
    }
    // A multi-entry package: the walk is already in `choices`. The skip inside
    // `file_platform_choices` — a leg whose platform another leg already
    // supplied is not walked — only ever applies AFTER a match, so "no
    // `ReachedBy` among the choices" is exactly "no entry loads this file".
    if choices
        .iter()
        .any(|choice| matches!(choice.reason, PlatformReason::ReachedBy(_)))
    {
        return None;
    }
    Some(manifest.entries.keys().cloned().collect())
}
