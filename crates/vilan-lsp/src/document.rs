//! Per-document analysis state and the navigation queries the language-server
//! handlers run against it: position→entity lookup, hover, go-to-definition,
//! find-references, and rename.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tower_lsp::lsp_types::{Position, Range};
use vilan_core::analyzer::{DERIVED_SOURCE, Expr, Parameter, SourceId};
use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::leak_tally::{LeakSite, Leaked};
use vilan_core::lexing::tokenize;
use vilan_core::node::Convention;
use vilan_core::{
    Error, LeakedEntryAst, Manifest, OwnedModules, Platform as BuildPlatform, Program, Span,
    Workspace as BuildWorkspace, analyze_source_owning_overlay_modules,
};

use crate::line_index::LineIndex;
use crate::references::{Definition, DefinitionKind, ReferenceIndex};
use vilan_ide::{
    Analysis, BOOK_BASE, Completion, ImportRoots, KEYWORD_DOCS, keyword_lexeme,
    source_call_subject, span_of,
};

/// A file's project context, resolved from the nearest `vilan.toml`: the build
/// platform to analyze it against, and the package source root (where `import
/// pkg::..` siblings resolve). Either is `None` when there's no project (or the
/// file's role can't be determined) — analysis then infers the platform from the
/// file's imports and roots `pkg::` at the file's own directory.
struct ProjectContext {
    platform: Option<BuildPlatform>,
    pkg_root: Option<PathBuf>,
    /// The file's resolved dependency workspace (P2), so cross-package imports
    /// (`import <dep>::..`) type-check in the editor.
    workspace: BuildWorkspace,
    /// Why the project didn't resolve, when it didn't (F5 S5). Everything below
    /// still degrades exactly as it did — the difference is that the reason is
    /// now published instead of swallowed.
    manifest_problem: Option<ManifestProblem>,
}

impl ProjectContext {
    fn none() -> ProjectContext {
        ProjectContext {
            platform: None,
            pkg_root: None,
            workspace: BuildWorkspace::default(),
            manifest_problem: None,
        }
    }
}

/// A `vilan.toml` failure, as the editor reports it: the manifest it belongs
/// to, the message (the CLI's own — one wording for both surfaces), and whether
/// it is a warning.
///
/// The severity split is the point of carrying this at all: a manifest that
/// does not parse, or a dependency that does not resolve, is an **error**; a
/// git dependency that simply has not been fetched yet is a **warning**,
/// because nothing is wrong — the editor never fetches (proposal
/// `distribution.md` §5) and one `vilan build` fixes it.
struct ManifestProblem {
    path: PathBuf,
    message: String,
    warning: bool,
}

/// Resolves a file's [`ProjectContext`] from the nearest ancestor `vilan.toml`.
/// A `[package]` roots `pkg::` at its source `root`, analyzes its files against
/// its platform (the package `target`, or per-entry targets under the
/// `[entry.<name>]` form), and resolves its dependency workspace (so
/// cross-package imports type-check). A `[library]` roots `pkg::` at the layer
/// the file lives in and resolves its dependencies the same way, with no
/// platform — see the branch itself for the two limits that carries. Anything
/// unreadable / unrecognized yields [`ProjectContext::none`].
fn resolve_project_context(entry_path: &Path) -> ProjectContext {
    let mut directory = entry_path.parent();
    let (manifest_path, root) = loop {
        let Some(current) = directory else {
            return ProjectContext::none();
        };
        let candidate = current.join("vilan.toml");
        if candidate.is_file() {
            break (candidate, current);
        }
        directory = current.parent();
    };
    let Ok(contents) = std::fs::read_to_string(&manifest_path) else {
        return ProjectContext::none();
    };
    let manifest = match Manifest::parse(&contents) {
        Ok((manifest, _warnings)) => manifest,
        // A manifest that doesn't parse resolves nothing, and until F5 S5 said
        // nothing either — the file the user is editing just quietly lost its
        // package. The wording is the CLI's, so both surfaces agree.
        Err(error) => {
            return ProjectContext {
                manifest_problem: Some(ManifestProblem {
                    path: manifest_path.clone(),
                    message: format!("invalid {}: {error}", manifest_path.display()),
                    warning: false,
                }),
                ..ProjectContext::none()
            };
        }
    };

    // A package: root `pkg::` at its declared source root and resolve its
    // dependency workspace (best-effort — a resolution error degrades to no
    // deps). The platform: the classic single-entry form analyzes every file
    // under the root against the package target; a multi-entry package
    // (proposal/platform-coloring.md §4.2) analyzes an ENTRY file under its
    // declared target, and any other file with a platform inferred from its
    // own imports — a module may be reached from several entries, and having
    // no `main` it faces no admission walk, so the choice only affects
    // scratch-style inference (hover colors are platform-independent).
    if let Some(package) = &manifest.package {
        let pkg_root = root.join(package.root());
        let platform = if manifest.entries.is_empty() {
            let build_platform = package.resolved_target().unwrap_or_default();
            is_within(&pkg_root, entry_path).then_some(build_platform)
        } else {
            manifest.entries.iter().find_map(|(name, entry)| {
                same_file(&pkg_root.join(entry.path(name)), entry_path)
                    .then(|| entry.resolved_target().unwrap_or_default())
            })
        };
        let (workspace, manifest_problem) = resolve_dependencies(root, &manifest_path);
        return ProjectContext {
            platform,
            pkg_root: Some(pkg_root),
            workspace,
            manifest_problem,
        };
    }

    // A `[library]`: its own modules and its own dependencies, which is what a
    // file inside it imports. Two limits are deliberate, and both are the
    // platform-model era's recorded deferral rather than an oversight:
    //
    //   - **No platform.** A library declares no `target`; it is compiled once
    //     per platform its layers serve, and the contract checker (`vilan check
    //     <library>`, `check_library_contract`) is what verifies that. The
    //     editor analyzes one buffer, so it keeps the no-project behavior —
    //     infer from the file's own imports — instead of inventing a target and
    //     reporting coloring violations the library never committed to.
    //   - **One root, the file's own layer.** `pkg::` resolution for the
    //     ENTRY's package searches a single directory (only a *dependency* gets
    //     the layered `search_roots`), so a file is rooted at the layer it
    //     lives in — its base layer, or the platform layer containing it. A
    //     module therefore reaches its own layer's siblings and not the other
    //     layers'. Reaching both needs the entry package to carry a layered
    //     spec, which is the deferral above; rooting at the file's own layer is
    //     the subset that never resolves less than the no-project fallback did.
    //
    // `std` costs nothing extra: `analyze` recognizes any of std's own layer
    // roots as "compiling std", after which the full layered search applies.
    if manifest.library.is_some() {
        let spec = vilan_core::manifest::resolve_library(root);
        // The deepest layer containing the file — layer roots may nest, and the
        // innermost one is the file's own.
        let pkg_root = spec
            .layers
            .iter()
            .filter(|layer| is_within(&layer.root, entry_path))
            .max_by_key(|layer| layer.root.as_os_str().len())
            .map(|layer| layer.root.clone())
            .unwrap_or(spec.base_root);
        let (workspace, manifest_problem) = resolve_dependencies(root, &manifest_path);
        return ProjectContext {
            platform: None,
            pkg_root: Some(pkg_root),
            workspace,
            manifest_problem,
        };
    }

    // A `[project]` workspace root has no buildable package of its own.
    ProjectContext::none()
}

/// The dependency workspace for the package rooted at `root`, with the reason
/// it did not resolve, if it did not.
///
/// The editor's git-dependency policy: **the cache, never the network**
/// (proposal/distribution.md §5). Analysis runs on every keystroke and must
/// never block on a repository — nor fetch behind the user's back — so a
/// dependency that no build has fetched yet is simply not in the workspace, and
/// its imports stay unresolved until `vilan build` (or `vilan check`) fetches
/// it.
///
/// The failure is not swallowed (F5 S5): the workspace still degrades to none —
/// imports into a dependency stay unresolved either way — but the REASON is
/// published on the manifest, so the wall of "cannot find module" errors has
/// something to point at. A git dependency the editor hasn't been allowed to
/// fetch is a warning: that manifest is correct, and `vilan build` is the whole
/// fix.
fn resolve_dependencies(
    root: &Path,
    manifest_path: &Path,
) -> (BuildWorkspace, Option<ManifestProblem>) {
    let git = vilan_core::git_dep::GitDeps::cache_only(vilan_embedded_std::default_git_dep_root());
    match vilan_core::manifest::resolve_workspace(root, &git) {
        Ok(workspace) => (workspace, None),
        Err(error) => (
            BuildWorkspace::default(),
            Some(ManifestProblem {
                // The manifest that WROTE the broken declaration, which for an
                // inherited dependency is the project root, not this file's own
                // `vilan.toml` (distribution.md §7's S5 residual). Squiggling
                // the member is technically true and practically useless: the
                // edit happens elsewhere.
                path: error
                    .declared_in()
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| manifest_path.to_path_buf()),
                warning: error.is_unfetched(),
                message: error.message().to_string(),
            }),
        ),
    }
}

/// Whether two paths name the same file. Both sides go through the one
/// canonicalization helper (`windows-support.md` §5), so the comparison is like
/// with like whether or not the file is on disk yet — the raw-string fallback
/// this replaces made a not-yet-saved buffer invisible whenever the two
/// spellings differed by a `.` or a `..`.
fn same_file(a: &Path, b: &Path) -> bool {
    vilan_core::util::canonical_path(a) == vilan_core::util::canonical_path(b)
}

/// Whether `file` lives within `directory`, through the same helper.
fn is_within(directory: &Path, file: &Path) -> bool {
    vilan_core::util::canonical_path(file).starts_with(vilan_core::util::canonical_path(directory))
}

/// A package source root for a file with no manifest: its own directory.
fn pkg_root_fallback(entry_path: &Path) -> PathBuf {
    entry_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .to_path_buf()
}

/// A content hash of a document's text, used to skip re-analysis when an edit
/// leaves the buffer byte-for-byte unchanged (undo/redo, a cursor-only change).
pub fn hash_text(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// Why a rename cannot be performed.
///
/// kolt.local 002's rule: a rename that cannot produce a COMPLETE edit set
/// refuses with a reason, and never emits a partial or stale one — a
/// half-applied rename does not fail loudly, it leaves a program that no longer
/// builds and an editor that reported success.
///
/// Note what is deliberately absent: a variant for "the client rejected the
/// edits". That was the old failure mode — "Rename failed to apply edits" is the
/// client's message, not the server's — and it came from emitting the same span
/// twice. The reference index cannot do that any more, so the condition is
/// prevented rather than reported.
#[derive(Debug, PartialEq, Eq)]
pub enum RenameRefusal {
    /// The cursor is not on an identifier.
    NotAnIdentifier,
    /// The new name is not a valid vilan identifier.
    InvalidName(String),
    /// The definition belongs to the standard library or a dependency, so a
    /// rename would edit code this project does not own.
    NotOwned { what: String, origin: &'static str },
    /// The definition is referenced from `[derive(..)]`-generated code, which
    /// has no file on disk to edit — so the edit set cannot be complete.
    Generated { what: String },
    /// The index knows it is missing references to this definition: use sites
    /// whose recorded span could not be proven to cover an identifier.
    Incomplete { what: String, missing: usize },
    /// An open file that imports the definition's file has un-analyzed edits,
    /// so its analyzed spans cannot be trusted against its live buffer —
    /// applying them could corrupt it, and skipping it would emit the partial
    /// rename this enum exists to forbid. The state lasts one debounce.
    StillAnalyzing { what: String },
}

impl RenameRefusal {
    /// The message the editor shows the user.
    pub fn message(&self) -> String {
        match self {
            RenameRefusal::NotAnIdentifier => "there is no symbol to rename here".to_string(),
            RenameRefusal::InvalidName(name) => {
                format!("`{name}` is not a valid vilan identifier")
            }
            RenameRefusal::NotOwned { what, origin } => {
                format!(
                    "cannot rename {what}: it is declared in {origin}, which this project does not own"
                )
            }
            RenameRefusal::Generated { what } => format!(
                "cannot rename {what}: it is used in code generated by `[derive(..)]`, which has no file to edit"
            ),
            RenameRefusal::Incomplete { what, missing } => format!(
                "cannot rename {what}: {missing} of its references could not be located, so the edit would be incomplete"
            ),
            RenameRefusal::StillAnalyzing { what } => format!(
                "cannot rename {what} yet: an open file that references it is still being analyzed; retry in a moment"
            ),
        }
    }
}

/// Whether `name` is a valid vilan identifier — a rename that writes anything
/// else produces a program that does not parse.
pub fn is_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_alphabetic() || first == '_') {
        return false;
    }
    if !characters.all(|character| character.is_alphanumeric() || character == '_') {
        return false;
    }
    !vilan_core::lexing::KEYWORDS
        .iter()
        .any(|(keyword, _)| *keyword == name)
}

/// A kind of declaration, for the document outline.
pub enum SymbolKind {
    Function,
    Struct,
    Field,
    Enum,
    Trait,
}

/// One node in the document outline.
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    /// The whole declaration span.
    pub full: Span,
    /// The name span (must lie within `full`).
    pub selection: Span,
    pub children: Vec<Symbol>,
}

/// A parameter's signature fragment for hover, with its declared calling
/// convention: `own x: T`, `x: &T`, `x: &mut T`, or the plain `x: T`. The `&` /
/// `&mut` live on the convention (rule 3), not in `type_label`, so they are
/// prepended here; `self` renders in its convention-specific self form.
///
/// A spread parameter renders its `...` (variadic-generics.md §S): unlike
/// `mut`, it IS part of the signature — it is exactly what a reader of the
/// hover needs in order to know whether to write the arguments out flat or as
/// one tuple. It never combines with a convention.
fn parameter_signature(parameter: &Parameter, type_label: &str) -> String {
    if parameter.name == "self" {
        return match parameter.convention {
            Convention::Bare => "self".to_string(),
            Convention::Own => "own self".to_string(),
            Convention::Ref => "&self".to_string(),
            Convention::RefMut => "&mut self".to_string(),
        };
    }
    if parameter.spread {
        return format!("...{}: {type_label}", parameter.name);
    }
    match parameter.convention {
        Convention::Bare => format!("{}: {type_label}", parameter.name),
        Convention::Own => format!("own {}: {type_label}", parameter.name),
        Convention::Ref => format!("{}: &{type_label}", parameter.name),
        Convention::RefMut => format!("{}: &mut {type_label}", parameter.name),
    }
}

/// Clamp a rendered hover preview to its display budget, cutting at a char
/// boundary. The budget is in BYTES, and byte 160 of a value carrying
/// multi-byte text — an em-dash in a style constant's CSS, an arrow in a
/// label — is not necessarily a boundary; `String::truncate` PANICS off one,
/// and a hover must never take the server down (it did: page.vl's `stack`).
fn clamp_preview(mut rendered: String) -> String {
    const BUDGET: usize = 160;
    if rendered.len() > BUDGET {
        let mut cut = BUDGET;
        while !rendered.is_char_boundary(cut) {
            cut -= 1;
        }
        rendered.truncate(cut);
        rendered.push('…');
    }
    rendered
}

/// One open file, held as TWO snapshots (`lsp-snapshot-consistency.md`):
///
/// - the **live** snapshot — [`text`](Document::text) and
///   [`line_index`](Document::line_index) — advanced synchronously by
///   [`set_text`](Document::set_text) on every edit, so live-text operations
///   (completion's context scan, whole-document formatting) see the character
///   that was just typed;
/// - the **analyzed** snapshot — the `program`, every product derived from it,
///   and `analyzed_index` (the line index OF the text the analysis consumed) —
///   advanced only when an analysis lands ([`adopt_analysis`](Document::adopt_analysis)).
///
/// Every program span and offset lives in the analyzed snapshot's coordinate
/// space, so it must be converted through `analyzed_index` — see
/// [`analyzed_range`](Document::analyzed_range). Converting a stale program's
/// byte offsets through the *live* index is what made highlighting and inlay
/// hints slide around while typing: same bytes, different text.
/// One applied live edit: `old_len` bytes at `start` became `new_len` bytes,
/// offsets in the text the edit applied to. The document's log of these —
/// accumulated since the analyzed snapshot — is what maps an analyzed-space
/// offset into live space (backlog B39c: the inlay viewport filter's
/// exactness under incremental sync).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EditDelta {
    pub start: usize,
    pub old_len: usize,
    pub new_len: usize,
}

pub struct Document {
    /// The LIVE line index: the text as of the last edit. Shared as an `Arc`
    /// with `analyzed_index` while the two snapshots agree, which is the
    /// steady state — an edit replaces this one alone.
    pub line_index: Arc<LineIndex>,
    /// The edits applied to the live text SINCE the analyzed snapshot, in
    /// application order — `Some(vec![])` is identity (the two texts agree),
    /// `None` is UNMAPPABLE (a whole-text replacement, or any text change
    /// that bypassed [`Document::apply_change`]) and consumers fall back to
    /// their approximate behavior. An analysis only ever lands on the live
    /// text (see `land`), so adoption resets this to identity.
    pub live_edits: Option<Vec<EditDelta>>,
    /// The ANALYZED line index: the text the current `program` was built from.
    /// A `LineIndex` owns its text, so this IS the analyzed-text record — there
    /// is no second `String` to keep in step.
    analyzed_index: Arc<LineIndex>,
    /// The analyzed program, paired with the entry text and tree it borrows
    /// for `'static` — one value, so a superseded analysis's program and its
    /// allocations are replaced (and reclaimed) together. See
    /// [`AnalyzedProgram`].
    pub program: AnalyzedProgram,
    pub diagnostics: Vec<Error>,
    /// The source file each diagnostic belongs to, parallel to `diagnostics`
    /// (`SourceId(0)` = this document; imported modules publish to their own
    /// files — backlog E1).
    pub diagnostic_sources: Vec<SourceId>,
    /// Non-fatal diagnostics (`[must_use]` drops) — published at Warning severity.
    pub warnings: Vec<Error>,
    /// The file each warning belongs to, parallel to `warnings` — the same
    /// attribution channel the errors use (backlog E16), so a `[must_use]`
    /// warning in an imported module squiggles in that module.
    pub warning_sources: Vec<SourceId>,
    /// The LIVE buffer text, as of the last edit — kept so a dependent
    /// re-analysis (another open file changed) can re-run this document without
    /// the editor resending its content.
    pub text: String,
    /// A hash of the source text this document was analyzed from, so an edit that
    /// leaves the buffer unchanged can skip re-analysis.
    pub text_hash: u64,
    /// `(start, end, id)` for every entry-file entity with a real span, used to
    /// find the innermost entity under a cursor.
    entity_spans: Vec<(usize, usize, Id)>,
    /// Every identifier occurrence in the analyzed program, keyed by the
    /// definition it names — the one table find-references and rename both read
    /// (see `crate::references`). Computed with the analysis so a query is a
    /// lookup rather than a scan of the whole entity map.
    reference_index: ReferenceIndex,
    /// Salvage tail retention (B38): the PREVIOUS analysis's semantic tokens
    /// for the byte-identical, line-aligned common suffix of the old and new
    /// analyzed texts, already shifted into the new text's coordinates.
    /// Served only when the fresh stream is entirely silent within the
    /// suffix — the salvage-truncation signature — so a complete parse
    /// (including one that legitimately RE-classifies identical tail text;
    /// semantics flow downward) always wins. Recomputed at every adoption.
    retained_tail: Vec<(Span, TokenKind, u32)>,
    /// Where the retained suffix begins in the CURRENT analyzed text;
    /// `usize::MAX` when nothing is retained.
    retained_tail_start: usize,
    /// Per-function platform requirements (`platform_color::requirements`),
    /// rendered lines like ``requires the `process` layer of `std` (via `…`)``
    /// — appended to the hover of any function that carries one.
    platform_requirements: HashMap<Id, String>,
    /// The `vilan.toml` failure behind this analysis, if any — published as one
    /// diagnostic on the manifest itself (see [`ManifestProblem`]).
    manifest_problem: Option<ManifestProblem>,
    /// What an `import`/`use` path in this file can reach (E57) — the analysis's
    /// own `std` spec, package root, and dependency packages, kept so completion
    /// can enumerate modules the `Program` never loaded. `None` on the degraded
    /// internal-error document, which resolved nothing.
    import_roots: Option<ImportRoots>,
}

/// The analyzed `Program` together with the allocations it borrows for
/// `'static`: the copy of the entry text `analyze_on_this_thread` leaks, the
/// entry tree the analysis leaks, and — M9 — the overlay-served module
/// copies the analysis parsed for itself. The program's lifetime parameter
/// says `'static`; this pairing is what makes that true for exactly as long
/// as the program lives, and gives the bytes back afterwards — the M7 fix
/// (`leak-soak.md` §7): before it, every keystroke's analysis leaked both
/// entry allocations for the rest of the session (3.12 MiB of RSS per
/// keystroke on a 735-line file, 6.1 GiB after two thousand); before M9
/// (§7.5/§7.9.4), a keystroke in a file another open document imports leaked
/// that file's text + tree once per distinct content through the
/// process-global parse caches.
///
/// **The invariant** (promised at [`AnalyzedProgram::new`], relied on in
/// `Drop`): the program borrows only `text`, `ast`, its `owned_modules`, and
/// allocations that are immortal (std and module texts served from
/// `parse_clean_cached`, interned names, cached macro worlds); and nothing
/// outside this value borrows `text`, `ast`, or any owned module copy — no
/// process-global cache, no thread-local, nothing the server retains.
/// `leak-soak.md` §7.2 is the audit that establishes the second half for the
/// entry pair, global by global; for the owned modules it is the mechanism's
/// own construction (§7.9.4): the base-world store gate refuses to store a
/// world that loaded one, and a macro-world compile never loads through the
/// scope. The first half is what `analyze_source_owning_overlay_modules`
/// returns. Every `Document` query returns owned values, so nothing borrowed
/// from the program outlives the borrow of `self` that produced it.
///
/// `Drop` does the ordering in one visible place — program first, then the
/// reclaims — rather than leaning on field declaration order.
pub struct AnalyzedProgram {
    program: Option<Program<'static>>,
    /// The leaked entry text the program borrows (`None` on a document that
    /// analyzed nothing — the degraded internal-error document).
    text: Option<Leaked<str>>,
    /// The leaked entry tree the program borrows (`None` when parsing produced
    /// no tree, or nothing was analyzed).
    ast: Option<LeakedEntryAst>,
    /// The overlay-served module allocations the analysis owns (M9): one
    /// text + tree (+ rendered errors when broken) per overlay-resident
    /// module its loader read. Empty when it loaded none.
    owned_modules: OwnedModules,
}

impl AnalyzedProgram {
    /// Pairs a program with the allocations it borrows.
    ///
    /// # Safety
    ///
    /// `program` must borrow nothing with a non-`'static` life other than
    /// `*text`, `*ast`, and the `owned_modules` allocations — it is the
    /// program `analyze_source_owning_overlay_modules` built over exactly
    /// that text and returned with exactly these handles — and nothing else
    /// may hold a reference derived from any of them: when this value drops,
    /// all are freed.
    unsafe fn new(
        program: Option<Program<'static>>,
        text: Option<Leaked<str>>,
        ast: Option<LeakedEntryAst>,
        owned_modules: OwnedModules,
    ) -> AnalyzedProgram {
        AnalyzedProgram {
            program,
            text,
            ast,
            owned_modules,
        }
    }

    /// No program, nothing leaked — the internal-error document's analysis.
    pub fn none() -> AnalyzedProgram {
        AnalyzedProgram {
            program: None,
            text: None,
            ast: None,
            owned_modules: OwnedModules::none(),
        }
    }

    pub fn as_ref(&self) -> Option<&Program<'static>> {
        self.program.as_ref()
    }

    pub fn is_some(&self) -> bool {
        self.program.is_some()
    }
}

impl Drop for AnalyzedProgram {
    fn drop(&mut self) {
        // The program borrows the two allocations: it goes FIRST, and only
        // then are they given back. Nothing else borrows them (the `new`
        // contract, established by leak-soak.md §7.2's audit), so after this
        // line no reference into either allocation exists anywhere.
        self.program = None;
        if let Some(text) = self.text.take() {
            // SAFETY: the program — the only borrower — was just dropped.
            unsafe { text.reclaim() };
        }
        if let Some(ast) = self.ast.take() {
            // SAFETY: as above; the tree's only borrower is gone.
            unsafe { ast.reclaim() };
        }
        // SAFETY: as above — the program was the owned modules' only
        // borrower (the `new` contract; leak-soak.md §7.9.4's store gate and
        // macro carve-out are what keep every global out of them).
        unsafe { std::mem::take(&mut self.owned_modules).reclaim() };
    }
}

/// A semantic-token classification (E2): precision highlighting from the
/// ANALYZED program, over TextMate's regex approximations. The discriminant
/// order IS the LSP legend order (`TOKEN_TYPES`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TokenKind {
    Namespace,
    Struct,
    Enum,
    Interface,
    TypeParameter,
    Parameter,
    Variable,
    Function,
    Method,
    Property,
    EnumMember,
    Macro,
    /// A markup tag name (element-syntax S5) — rendered as the LSP `type`
    /// token, the class-ish color themes give components and tags.
    Tag,
}

/// Token-modifier bits, index-aligned with `TOKEN_MODIFIERS`.
pub const MODIFIER_DECLARATION: u32 = 1 << 0;
pub const MODIFIER_READONLY: u32 = 1 << 1;

/// The modifier legend.
pub const TOKEN_MODIFIERS: [&str; 2] = ["declaration", "readonly"];

/// The LSP legend, index-aligned with `TokenKind`.
pub const TOKEN_TYPES: [&str; 13] = [
    "namespace",
    "struct",
    "enum",
    "interface",
    "typeParameter",
    "parameter",
    "variable",
    "function",
    "method",
    "property",
    "enumMember",
    "macro",
    "type",
];

/// One diagnostic as the language server publishes it: the file it belongs to
/// (`None` = the analyzed document itself), its span *in that file's text*, the
/// message, and the severity. LSP-type-free so the grouping is unit-testable.
pub struct PublishedDiagnostic {
    pub path: Option<PathBuf>,
    pub span: Span,
    pub message: String,
    pub warning: bool,
    /// The diagnostic's secondary note (diagnostics-standard.md C3): span,
    /// message, and the note's own file when it lives elsewhere (`None` =
    /// the diagnostic's file) — published as LSP related information.
    pub note: Option<(Span, String, Option<PathBuf>)>,
    /// The diagnostic's requirement trace (backlog E78), in the same
    /// per-location shape as `note`: one entry per uncovered upstream call,
    /// ordered entry → read — published as LSP related information ahead of
    /// the note, preserving this order, and each CALL hop additionally as
    /// its own diagnostic at the call (E81).
    pub trace: Vec<PublishedHop>,
}

/// One requirement-trace entry as the publisher wants it (backlog E78):
/// located like the C3 note, plus whether it marks an uncovered CALL — a
/// call hop additionally publishes as its own diagnostic at that location
/// (E81), while the elision tail only ever rides as related information
/// (its span is the last kept hop's, already underlined by that hop's own
/// diagnostic).
pub struct PublishedHop {
    pub span: Span,
    pub message: String,
    pub path: Option<PathBuf>,
    pub call: bool,
}

/// The markup spans of a raw parse (element-syntax S5): tag names (open and
/// close), attribute and event names, and the desugar-scaffolding spans whose
/// analyzed tokens the markup replaces.
#[derive(Default)]
struct MarkupSpans {
    scaffolding: Vec<Span>,
    tags: Vec<Span>,
    attributes: Vec<Span>,
}

fn collect_markup_spans(
    node: &vilan_core::Spanned<vilan_core::node::Node<'_>>,
    out: &mut MarkupSpans,
) {
    use vilan_core::node::{ElementHeadItem, Node};
    if let Node::Element(body) = &node.0 {
        out.scaffolding.push((node.1.start..body.tag.end).into());
        out.tags.push(body.tag);
        if let Some(close) = body.close_tag {
            out.tags.push(close);
        }
        for item in &body.head {
            match item {
                ElementHeadItem::Attribute(name, _) => out.attributes.push(*name),
                ElementHeadItem::Event((_, name_span), _) => out.attributes.push(*name_span),
                ElementHeadItem::Chain(_) => {}
            }
        }
    }
    node.0
        .for_each_child(&mut |child| collect_markup_spans(child, out));
}

/// The innermost element whose open or close TAG NAME contains `offset`,
/// as (open, close) spans — `None` when the cursor is elsewhere or the
/// element is self-closing. Children visit after their parent, so the last
/// hit is the innermost.
fn find_linked_tags(
    node: &vilan_core::Spanned<vilan_core::node::Node<'_>>,
    offset: usize,
    out: &mut Option<(Span, Span)>,
) {
    use vilan_core::node::Node;
    if let Node::Element(body) = &node.0 {
        if let Some(close) = body.close_tag {
            let open = body.tag;
            let touches = |span: Span| span.start <= offset && offset <= span.end;
            if touches(open) || touches(close) {
                *out = Some((open, close));
            }
        }
    }
    node.0
        .for_each_child(&mut |child| find_linked_tags(child, offset, out));
}

/// Whether an expression is a value-position use of the definition `def_id` — the
/// forms the entity map records for a resolved name (a call subject, a bare value,
/// an enum variant). Used by the Organize Imports prune to keep a value-used
/// import. An enum-variant expression carries its enum's Id.
fn expr_references_definition(expr: &Expr, def_id: Id) -> bool {
    match expr {
        Expr::Local(id)
        | Expr::Function(id)
        | Expr::ExternalFunction(id)
        | Expr::Struct(id)
        | Expr::Enum(id)
        | Expr::Trait(id)
        | Expr::Module(id)
        | Expr::EnumVariant(id, _) => *id == def_id,
        _ => false,
    }
}

impl Document {
    pub fn analyze(text: &str, std_dir: &Path, entry_path: &Path) -> Self {
        // The pipeline recurses deeply (chumsky), and macro-world compiles NEST
        // a full analysis inside the analysis — run the whole thing on a
        // dedicated big-stack thread, like the CLI's compiler thread (128 MiB,
        // whose measured rationale lives at its `COMPILER_STACK_SIZE`, and
        // which covers this nesting explicitly; B138/B139/B142).
        // Callers stay synchronous (the LSP already wraps this in
        // spawn_blocking).
        //
        // The thread body is panic-fenced (B40): the core pipeline carries its
        // own fence, but the stages around it (workspace/manifest discovery,
        // index building) do not, and an unwinding analysis used to re-raise
        // through the join — out of whichever handler called it, aborting the
        // whole server. It degrades to an internal-error document instead.
        let text = text.to_string();
        let outer_text = text.clone();
        let std_dir = std_dir.to_path_buf();
        let entry_path = entry_path.to_path_buf();
        std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(move || {
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Self::analyze_on_this_thread(&text, &std_dir, &entry_path)
                }))
                .unwrap_or_else(|_| Self::internal_error(&text))
            })
            .expect("spawn analysis thread")
            .join()
            // Unreachable while the thread body catches unwinds (an abort
            // never returns here); kept graceful all the same.
            .unwrap_or_else(|_| Self::internal_error(&outer_text))
    }

    /// The degraded document a panicked analysis lands on: no program, the
    /// live text faithfully recorded (so position mapping and re-analysis on
    /// the next edit behave), and one honest diagnostic.
    fn internal_error(text: &str) -> Self {
        let line_index = Arc::new(LineIndex::new(text));
        Document {
            // A fresh analysis IS the analyzed text: the map is identity.
            live_edits: Some(Vec::new()),
            analyzed_index: Arc::clone(&line_index),
            line_index,
            program: AnalyzedProgram::none(),
            diagnostics: vec![Error { trace: Vec::new(),
                note: None,
                span: vilan_core::span::Span::new((), 0..0),
                msg: "internal error: the compiler panicked analyzing this file (this is a bug; the details are on stderr)"
                    .to_string(),
            }],
            diagnostic_sources: vec![SourceId(0)],
            warnings: Vec::new(),
            warning_sources: Vec::new(),
            text: text.to_string(),
            text_hash: hash_text(text),
            entity_spans: Vec::new(),
            reference_index: ReferenceIndex::default(),
            retained_tail: Vec::new(),
            retained_tail_start: usize::MAX,
            platform_requirements: HashMap::default(),
            manifest_problem: None,
            import_roots: None,
        }
    }

    fn analyze_on_this_thread(text: &str, std_dir: &Path, entry_path: &Path) -> Self {
        // A fresh analysis has one snapshot: its text IS both the live and the
        // analyzed one, so both indices share a single `Arc`. They part company
        // only when an edit lands (`set_text`).
        let line_index = Arc::new(LineIndex::new(text));
        let text_hash = hash_text(text);
        // The program borrows its source for `'static`, so leak a copy — with
        // the handle kept: the `AnalyzedProgram` built below owns it and gives
        // it back when the analysis is superseded or the document closes.
        let (leaked_text, leaked) = Leaked::leak(
            text.to_string().into_boxed_str(),
            LeakSite::LspEntryText,
            text.len(),
        );
        // Prefer the project's declared platform and source root (the file's role in
        // its `vilan.toml`); fall back to inferring the platform from imports and
        // rooting `pkg::` at the file's own directory.
        let context = resolve_project_context(entry_path);
        let manifest_problem = context.manifest_problem;
        let pkg_root = context
            .pkg_root
            .unwrap_or_else(|| pkg_root_fallback(entry_path));
        // `std` is resolved as a library (its layered roots) from the std directory
        // — the manifest when present, else a bare base layer (L2).
        let std = vilan_core::manifest::resolve_std(std_dir);
        // The same roots the analysis just resolved, kept for import-path
        // completion — which reaches modules the analysis never loaded, so the
        // `Program` cannot tell it where they live (E57).
        let import_roots = ImportRoots {
            std: std.clone(),
            pkg_root: pkg_root.clone(),
            dependencies: context
                .workspace
                .entry_dependencies
                .iter()
                .filter_map(|(name, index)| {
                    let spec = context.workspace.packages.get(*index)?;
                    Some((name.clone(), spec.clone()))
                })
                .collect(),
        };
        // The M9 opt-in (leak-soak.md §7.9.4): modules served from the
        // open-document overlay parse into allocations THIS analysis owns —
        // returned as `owned_modules` — instead of entries in the
        // process-global caches, which a keystroke's content would leak for
        // the session (§7.5). The `AnalyzedProgram` built below owns and
        // reclaims them beside the entry text and tree.
        let vilan_core::AnalyzedEntry {
            program,
            diagnostics,
            ast,
            owned_modules,
        } = analyze_source_owning_overlay_modules(
            leaked,
            &std,
            &pkg_root,
            entry_path,
            context.platform,
            &context.workspace,
        );

        // The entity table the navigation queries index, computed by the one
        // function both front-ends use (`vilan_ide::entity_spans`).
        let entity_spans = program
            .as_ref()
            .map(vilan_ide::entity_spans)
            .unwrap_or_default();

        // The identifier-occurrence table the reference queries read.
        let reference_index = program
            .as_ref()
            .map(ReferenceIndex::build)
            .unwrap_or_default();

        // `diagnostics` = the entry's own lex/parse errors, then the program's
        // (see `analyze_source`) — so the source list is an entry-attributed
        // prefix followed by the program's per-diagnostic attribution.
        let program_diagnostics = program
            .as_ref()
            .map(|program| program.diagnostics.len())
            .unwrap_or(0);
        let mut diagnostic_sources =
            vec![SourceId(0); diagnostics.len().saturating_sub(program_diagnostics)];
        if let Some(program) = &program {
            diagnostic_sources.extend(program.diagnostic_sources.iter().copied());
        }
        let warnings = program
            .as_ref()
            .map(|program| program.warnings.clone())
            .unwrap_or_default();
        let warning_sources = program
            .as_ref()
            .map(|program| program.warning_sources.clone())
            .unwrap_or_default();
        let platform_requirements = program
            .as_ref()
            .map(vilan_core::platform_color::requirements)
            .unwrap_or_default();
        // SAFETY: `program` was built by `analyze_source_owning_overlay_modules`
        // over `leaked` (the text `leaked_text` owns) and returned with `ast`
        // — the handle to the very tree it borrows — and `owned_modules`, the
        // allocations of exactly the overlay-served modules it loaded;
        // everything read from it above was copied into owned values
        // (`entity_spans`, the diagnostics, the requirements). Nothing else
        // borrows any of these allocations — leak-soak.md §7.2 audits every
        // process-global and thread-local for the entry pair, and §7.9.4's
        // store gate and macro carve-out keep every global out of the owned
        // modules.
        let program =
            unsafe { AnalyzedProgram::new(program, Some(leaked_text), ast, owned_modules) };
        Document {
            // A fresh analysis IS the analyzed text: the map is identity.
            live_edits: Some(Vec::new()),
            analyzed_index: Arc::clone(&line_index),
            line_index,
            program,
            diagnostics,
            diagnostic_sources,
            warnings,
            warning_sources,
            text: text.to_string(),
            text_hash,
            entity_spans,
            reference_index,
            retained_tail: Vec::new(),
            retained_tail_start: usize::MAX,
            platform_requirements,
            manifest_problem,
            import_roots: Some(import_roots),
        }
    }

    /// The document's diagnostics grouped for publishing: errors attributed to
    /// the file they occurred in (`None` = this document), plus this document's
    /// warnings. Diagnostics from generated (derive) code carry template spans
    /// that map to no file — they attach to the entry at offset 0, labeled.
    pub fn published_diagnostics(&self) -> Vec<PublishedDiagnostic> {
        let mut published = Vec::new();
        // The C3 note as the publisher wants it: its span, its message, and the
        // file it lives in when it has one of its own (`None` = the
        // diagnostic's own file, whichever that is — backlog E17).
        let locate = |note: &vilan_core::error::Note| {
            let note_path = note
                .source
                .and_then(|source| self.program.as_ref()?.source_path(source))
                .map(Path::to_path_buf);
            (note.span, note.msg.clone(), note_path)
        };
        let note_of = |error: &Error| error.note.as_ref().map(locate);
        // The E78 requirement trace, each hop located exactly like the note.
        let trace_of = |error: &Error| {
            error
                .trace
                .iter()
                .map(|hop| {
                    let (span, message, path) = locate(&hop.note);
                    PublishedHop {
                        span,
                        message,
                        path,
                        call: hop.call,
                    }
                })
                .collect::<Vec<_>>()
        };
        for (index, error) in self.diagnostics.iter().enumerate() {
            let source = self
                .diagnostic_sources
                .get(index)
                .copied()
                .unwrap_or(SourceId(0));
            if source == SourceId(0) {
                published.push(PublishedDiagnostic {
                    path: None,
                    span: error.span,
                    message: error.msg.clone(),
                    warning: false,
                    note: note_of(error),
                    trace: trace_of(error),
                });
            } else if source == DERIVED_SOURCE {
                published.push(PublishedDiagnostic {
                    path: None,
                    span: Span::from(0..0),
                    message: format!("(in generated code) {}", error.msg),
                    warning: false,
                    note: None,
                    trace: Vec::new(),
                });
            } else {
                let path = self
                    .program
                    .as_ref()
                    .and_then(|program| program.source_path(source))
                    .map(Path::to_path_buf);
                match path {
                    // The note rides along with the diagnostic wherever it is
                    // published (backlog E17): a declaration note is exactly
                    // what LSP related information is for, and dropping it in
                    // this branch cost every module-attributed diagnostic its
                    // second location.
                    Some(path) => published.push(PublishedDiagnostic {
                        path: Some(path),
                        span: error.span,
                        message: error.msg.clone(),
                        warning: false,
                        note: note_of(error),
                        trace: trace_of(error),
                    }),
                    // An unknown source (shouldn't happen): keep the error
                    // visible on the entry rather than dropping it.
                    None => published.push(PublishedDiagnostic {
                        path: None,
                        span: Span::from(0..0),
                        message: error.msg.clone(),
                        warning: false,
                        note: None,
                        trace: Vec::new(),
                    }),
                }
            }
        }
        for (index, warning) in self.warnings.iter().enumerate() {
            // A warning is attributed like an error: a module's warning
            // squiggles in the module, not at that offset in this document.
            let source = self
                .warning_sources
                .get(index)
                .copied()
                .unwrap_or(SourceId(0));
            let path = (source != SourceId(0)).then(|| {
                self.program
                    .as_ref()
                    .and_then(|program| program.source_path(source))
                    .map(Path::to_path_buf)
            });
            match path {
                // This document's own.
                None => published.push(PublishedDiagnostic {
                    path: None,
                    span: warning.span,
                    message: warning.msg.clone(),
                    warning: true,
                    note: note_of(warning),
                    trace: trace_of(warning),
                }),
                Some(Some(path)) => published.push(PublishedDiagnostic {
                    path: Some(path),
                    span: warning.span,
                    message: warning.msg.clone(),
                    warning: true,
                    note: note_of(warning),
                    trace: trace_of(warning),
                }),
                // A source with no file (generated code): keep it visible on
                // the entry rather than at that offset in the wrong text.
                Some(None) => published.push(PublishedDiagnostic {
                    path: None,
                    span: Span::from(0..0),
                    message: warning.msg.clone(),
                    warning: true,
                    note: None,
                    trace: Vec::new(),
                }),
            }
        }
        // The manifest channel (F5 S5): ONE diagnostic, on `vilan.toml` itself
        // — which is where the mistake is, and where the planner already knows
        // how to publish (it addresses every non-entry file the same way, open
        // or not). The span is the start of the file: the failure belongs to
        // the manifest as a whole, and locating a TOML key would mean parsing
        // TOML for spans, which this slice deliberately does not do. Two open
        // files in one broken package publish the identical diagnostic, and the
        // planner's union dedups it to one.
        if let Some(problem) = &self.manifest_problem {
            published.push(PublishedDiagnostic {
                path: Some(problem.path.clone()),
                span: Span::from(0..0),
                message: problem.message.clone(),
                warning: problem.warning,
                note: None,
                trace: Vec::new(),
            });
        }
        published
    }

    /// Advances the LIVE snapshot — the text and its line index — without
    /// re-analyzing. Applied on every edit so live-text queries (notably
    /// completion's context scan) see the just-typed character immediately,
    /// while the heavier re-analysis stays debounced. The analyzed snapshot
    /// (`program`, `analyzed_index`, `text_hash`) is deliberately untouched:
    /// program answers stay exactly right for the text they were computed
    /// from, and the pending re-analysis still fires.
    pub fn set_text(&mut self, text: &str) {
        self.line_index = Arc::new(LineIndex::new(text));
        self.text = text.to_string();
        // A whole-text set has no edit shape to record: the map from the
        // analyzed snapshot is broken until the next analysis lands.
        self.live_edits = None;
    }

    /// Apply one LSP content change to the LIVE snapshot: a ranged event
    /// splices at UTF-16 positions against the current live text (the
    /// incremental-sync contract — events in one notification apply in
    /// order, each against the text as already edited) and RECORDS its
    /// shape in `live_edits`; an event without a range is the full-sync
    /// form and resets the log to unmappable.
    pub fn apply_change(&mut self, range: Option<tower_lsp::lsp_types::Range>, replacement: &str) {
        let Some(range) = range else {
            self.set_text(replacement);
            return;
        };
        let start = self.line_index.offset(range.start);
        let end = self.line_index.offset(range.end).max(start);
        let mut text = self.text.clone();
        text.replace_range(start..end, replacement);
        self.line_index = Arc::new(LineIndex::new(&text));
        self.text = text;
        if let Some(edits) = self.live_edits.as_mut() {
            edits.push(EditDelta {
                start,
                old_len: end - start,
                new_len: replacement.len(),
            });
        }
    }

    /// Map an ANALYZED-space byte offset into the live text, through the
    /// recorded edits — `None` when the log is unmappable. An offset inside
    /// a replaced region clamps into the replacement (the anchor's text is
    /// gone; its nearest surviving position is the honest answer).
    pub fn live_offset(&self, offset: usize) -> Option<usize> {
        let edits = self.live_edits.as_ref()?;
        let mut offset = offset;
        for edit in edits {
            if offset >= edit.start + edit.old_len {
                offset = offset - edit.old_len + edit.new_len;
            } else if offset > edit.start {
                offset = edit.start + (offset - edit.start).min(edit.new_len);
            }
        }
        Some(offset)
    }

    /// The shared queries' view of this document (`vilan_ide::Analysis`): the
    /// analyzed program read against both snapshots. A struct of references,
    /// built per query.
    fn analysis<'a, 'src>(&'a self, program: &'a Program<'src>) -> Analysis<'a, 'src> {
        Analysis {
            program,
            analyzed: self.analyzed_index.shared(),
            live: self.line_index.shared(),
            entity_spans: &self.entity_spans,
            platform_requirements: &self.platform_requirements,
            import_roots: self.import_roots.as_ref(),
            source_texts: Default::default(),
        }
    }

    /// The innermost entry-file entity whose span contains `offset`.
    fn entity_at(&self, offset: usize) -> Option<Id> {
        vilan_ide::analysis::entity_at(&self.entity_spans, offset)
    }

    /// The line index of the text the current analysis consumed: the coordinate
    /// space every program span and offset lives in.
    pub fn analyzed_index(&self) -> &LineIndex {
        &self.analyzed_index
    }

    /// The text the current analysis consumed.
    pub fn analyzed_text(&self) -> &str {
        self.analyzed_index.text()
    }

    /// The LSP range for a program span — the outbound program-space
    /// conversion (semantic tokens, inlay hints, definition/reference/symbol
    /// locations). Correct for the analyzed text, and therefore visually
    /// correct in the editor everywhere except the lines being actively edited;
    /// converting the same bytes through the live index is correct for
    /// *neither* text.
    pub fn analyzed_range(&self, span: &Span) -> Range {
        self.analyzed_index.range(span)
    }

    /// The LSP position for a program byte offset (an inlay hint's anchor).
    pub fn analyzed_position(&self, offset: usize) -> Position {
        self.analyzed_index.position(offset)
    }

    /// The program byte offset for an LSP position — the inbound program-space
    /// conversion, feeding `entity_at` and the queries built on it (hover,
    /// definition, references, rename).
    pub fn analyzed_offset(&self, position: Position) -> usize {
        self.analyzed_index.offset(position)
    }

    /// Whether the live buffer has advanced past the analyzed text — i.e. an
    /// analysis is pending and program answers describe an older text.
    ///
    /// Deliberately a text comparison rather than a flag set by `set_text`: an
    /// edit that returns the buffer to the analyzed text takes the debounce's
    /// unchanged-text short-circuit, so no analysis ever lands to clear a flag
    /// and it would stick "stale" forever. Text equality heals itself.
    pub fn is_stale(&self) -> bool {
        self.text != self.analyzed_text()
    }

    /// Whether this document's LAST analysis loaded `path` — the dependency
    /// edge reanalysis gating asks about (backlog B39): an edit to a file
    /// this document never loaded cannot change its diagnostics, so its
    /// dependents-sweep can skip it. The set is `Program.canonical_sources`
    /// (the entry, every imported package module, and std), compared through
    /// `same_file` so path spelling cannot fake a miss. A document with no
    /// program answers TRUE: with no recorded set, re-analysis is the
    /// conservative direction — the old always-sweep behavior, kept exactly
    /// where its reason still holds.
    pub fn depends_on(&self, path: &Path) -> bool {
        let Some(program) = self.program.as_ref() else {
            return true;
        };
        program
            .canonical_sources
            .iter()
            .any(|source| same_file(source, path))
    }

    /// Land a completed analysis of this document (`analysis` is a fresh
    /// [`Document::analyze`] result for the same file).
    ///
    /// Every field is classified. The **analysis side** — `program` and
    /// everything derived from it, `analyzed_index`, `text_hash`, the
    /// diagnostics and their attribution, the const results, entity spans,
    /// platform requirements, the manifest problem — is adopted wholesale. The
    /// **live side** — `text` and `line_index` — is kept whenever the buffer
    /// advanced while the analysis ran, so a merge can never undo typing (this
    /// replaced a whole-`Document` overwrite, which lost every character typed
    /// during the 80–190 ms analysis). When the two texts are equal the live
    /// side is adopted too, which puts both indices back on one shared `Arc`.
    pub fn adopt_analysis(&mut self, analysis: Document) {
        // B38: decide what the OUTGOING analysis may keep answering for
        // before its side is replaced — the byte-identical tail carries over,
        // the rest dies with it.
        self.compute_retained_tail(&analysis.text);
        let Document {
            line_index: analyzed_live_index,
            retained_tail: _,
            retained_tail_start: _,
            analyzed_index,
            program,
            diagnostics,
            diagnostic_sources,
            warnings,
            warning_sources,
            text: analyzed_text,
            live_edits: _,
            text_hash,
            entity_spans,
            reference_index,
            platform_requirements,
            manifest_problem,
            import_roots,
        } = analysis;
        // The analysis side, in full. `program` is the pair of the new
        // program and the allocations it borrows; assigning it drops the
        // OUTGOING pair — its program first, then its entry text and tree are
        // reclaimed (`AnalyzedProgram`'s `Drop`). This is the line the
        // session leak M7 measured (leak-soak.md §4.1) stops at.
        self.analyzed_index = analyzed_index;
        self.program = program;
        self.diagnostics = diagnostics;
        self.diagnostic_sources = diagnostic_sources;
        self.warnings = warnings;
        self.warning_sources = warning_sources;
        self.text_hash = text_hash;
        self.entity_spans = entity_spans;
        self.reference_index = reference_index;
        self.platform_requirements = platform_requirements;
        self.manifest_problem = manifest_problem;
        self.import_roots = import_roots;
        // The live side, only when the buffer has not moved on.
        if self.text == analyzed_text {
            self.text = analyzed_text;
            self.line_index = analyzed_live_index;
            // Live and analyzed agree again: the edit map is identity.
            self.live_edits = Some(Vec::new());
        } else {
            // The adopted analysis matches an older text; the recorded
            // edits no longer start from this snapshot.
            self.live_edits = None;
        }
    }

    /// Computes B38's retained tail: the longest byte-identical common
    /// suffix of the outgoing analyzed text and `incoming` (the next analyzed
    /// text), trimmed forward to a line boundary, and the outgoing tokens
    /// that live entirely inside it, shifted into the incoming text's
    /// coordinates. Identity of BYTES is the whole honesty argument: the
    /// suffix is literally the same text, so positions are exact; whether the
    /// tokens are still semantically current is governed at serve time (a
    /// fresh stream that reaches the suffix suppresses them). Chains across
    /// successive truncated analyses, because `semantic_tokens` below folds
    /// the current retention into what the next adoption captures.
    fn compute_retained_tail(&mut self, incoming: &str) {
        let outgoing_tokens = if self.program.is_some() {
            self.semantic_tokens()
        } else {
            Vec::new()
        };
        self.retained_tail = Vec::new();
        self.retained_tail_start = usize::MAX;
        if outgoing_tokens.is_empty() {
            return;
        }
        let outgoing = self.analyzed_text();
        let common = outgoing
            .bytes()
            .rev()
            .zip(incoming.bytes().rev())
            .take_while(|(old, new)| old == new)
            .count();
        if common == 0 {
            return;
        }
        // Trim to a line boundary so the shift is a pure offset with no
        // mid-line seam: the suffix begins at the first character after a
        // newline (or at 0 when the texts are wholly identical).
        let mut new_start = incoming.len() - common;
        if new_start > 0 && incoming.as_bytes()[new_start - 1] != b'\n' {
            match incoming[new_start..].find('\n') {
                Some(newline) => new_start += newline + 1,
                None => return,
            }
        }
        if new_start >= incoming.len() {
            return;
        }
        let old_start = outgoing.len() - (incoming.len() - new_start);
        let shift = new_start as i64 - old_start as i64;
        self.retained_tail = outgoing_tokens
            .into_iter()
            .filter(|(span, ..)| span.start >= old_start)
            .map(|(span, kind, modifiers)| {
                let start = (span.start as i64 + shift) as usize;
                let end = (span.end as i64 + shift) as usize;
                (Span { start, end }, kind, modifiers)
            })
            .collect();
        self.retained_tail_start = new_start;
    }

    /// Whether `offset` falls inside a lexed token of the analyzed text —
    /// false in trivia: comments, whitespace, blank lines. Containment
    /// lookups (`entity_at`) are only meaningful for offsets that touch
    /// actual code; a comment inside a function body is *contained* by the
    /// function's span but is not the function.
    fn offset_touches_a_token(&self, offset: usize) -> bool {
        let (tokens, _errors) = tokenize(self.analyzed_text());
        tokens.iter().any(|(_, span)| {
            let range = span.into_range();
            range.start <= offset && offset < range.end
        })
    }

    /// The hover for the entity under `offset` (E9): a fenced full
    /// declaration when the entity names one (function signature — with
    /// inferred `async` prepended — or a struct/enum block), the
    /// declaration's leading `//` comment as prose, and the platform
    /// requirement line where one is inferred. Anything else keeps its
    /// rendered type.
    /// The matching-tag pair at `offset` (element-syntax S5): the open and
    /// close tag-name spans of the innermost element whose open or close name
    /// the cursor touches — the linked-editing nicety, so renaming one tag
    /// renames the other. Raw-parsed per request, like `keyword_hover`'s lex:
    /// cheap, and independent of analysis succeeding.
    pub fn linked_tag_ranges(&self, offset: usize) -> Option<(Span, Span)> {
        let (tree, _errors) = vilan_core::parsing::parse(self.analyzed_text());
        let root = tree?;
        let mut found: Option<(Span, Span)> = None;
        for item in &root.0 {
            find_linked_tags(item, offset, &mut found);
        }
        found
    }

    pub fn hover(&self, offset: usize) -> Option<String> {
        // A keyword under the cursor: its one-line meaning + a book link. This
        // is purely lexical, so it works even when analysis produced no program
        // (a keyword hovers on a document that doesn't yet compile).
        if let Some(keyword) = self.keyword_hover(offset) {
            return Some(keyword);
        }
        let program = self.program.as_ref()?;
        // A type name in type position: the full declaration when known.
        if let Some((definition, label)) = self.type_reference_at(program, offset) {
            if let Some(definition) = definition {
                if let Some(declaration) = program.declaration_labels.get(&definition) {
                    return Some(self.compose_hover(program, definition, declaration, None));
                }
            }
            return Some(label);
        }
        // Everything below answers by span CONTAINMENT, and an entity's span
        // contains its trivia — a comment or blank line inside a function body
        // would hover as the enclosing function. Only code hovers.
        if !self.offset_touches_a_token(offset) {
            return None;
        }
        let id = self.entity_at(offset)?;
        // A function (or requirement-carrying binding): the full signature.
        if let Some(target) = self.analysis(program).function_target(id) {
            let requirement = self.platform_requirements.get(&target).cloned();
            if let Some(declaration) = program.declaration_labels.get(&target) {
                return Some(self.compose_hover(program, target, declaration, requirement));
            }
        }
        // A struct/enum name in value position (a constructor, a variant).
        if let Some(definition) = self.type_declaration_target(program, id) {
            if let Some(declaration) = program.declaration_labels.get(&definition) {
                return Some(self.compose_hover(program, definition, declaration, None));
            }
        }
        // A variable (`let`/`mut`, local or module-level, or a destructured
        // binder) or a parameter: its typed declaration; a member read: the
        // fenced `name: T` (E72); else the bare type — fenced too, so every
        // hover reads as code.
        let type_label = self
            .binding_hover(program, id)
            .or_else(|| self.member_hover(program, id))
            .or_else(|| {
                self.analysis(program).hover_label(id).map(|label| {
                    // A constant shows its VALUE beside its type (E9).
                    let label = match self.const_value_label(program, id) {
                        Some(value) => format!("{label} = {value}"),
                        None => label,
                    };
                    format!("```vilan\n{label}\n```")
                })
            });
        let requirement = self
            .analysis(program)
            .function_target(id)
            .and_then(|function| self.platform_requirements.get(&function))
            .cloned();
        match (type_label, requirement) {
            // A blank markdown line, so the requirement renders as its own
            // paragraph under the type.
            (Some(type_label), Some(requirement)) => Some(format!("{type_label}\n\n{requirement}")),
            (Some(type_label), None) => Some(type_label),
            (None, requirement) => requirement,
        }
    }

    /// Assembles a declaration hover: the fenced declaration (with inferred
    /// `async` prepended to a function signature), its leading `//` doc
    /// block, and the platform requirement, each as its own paragraph.
    fn compose_hover(
        &self,
        program: &Program,
        declaration_id: Id,
        declaration: &str,
        requirement: Option<String>,
    ) -> String {
        let declaration = if program.async_functions.contains(&declaration_id)
            && !declaration.starts_with("async ")
        {
            format!("async {declaration}")
        } else {
            declaration.to_string()
        };
        let mut out = format!("```vilan\n{declaration}\n```");
        if let Some(docs) = self.analysis(program).doc_comment_of(declaration_id) {
            out.push_str("\n\n");
            out.push_str(&docs);
        }
        if let Some(requirement) = requirement {
            out.push_str("\n\n");
            out.push_str(&requirement);
        }
        out
    }

    /// The hover for a keyword under `offset`: a one-line meaning and a deep
    /// link into the book. Lexes the buffer (cheap, hover is a glance) and
    /// classifies the token whose span contains the cursor — only a keyword
    /// token yields a hover, so a string literal like `"fun"` never does.
    fn keyword_hover(&self, offset: usize) -> Option<String> {
        // The ANALYZED text: `offset` arrived through `analyzed_offset`, so
        // every lookup hover makes must index the same snapshot. (Lexing is
        // independent of analysis SUCCEEDING, so a keyword still hovers on a
        // document that doesn't compile — that was the point of doing this
        // before the `program` check.)
        let (tokens, _errors) = tokenize(self.analyzed_text());
        let (token, _span) = tokens.iter().find(|(_, span)| {
            let range = span.into_range();
            range.start <= offset && offset < range.end
        })?;
        let lexeme = keyword_lexeme(token)?;
        let (_, sentence, path) = KEYWORD_DOCS
            .iter()
            .find(|(keyword, _, _)| *keyword == lexeme)?;
        Some(format!(
            "**`{lexeme}`**: {sentence}\n\n[The vilan book →]({BOOK_BASE}{path})"
        ))
    }

    /// The hover for a `let`/`mut` variable or a parameter under the cursor,
    /// rendered as a fenced declaration in the house style: `let name: T` /
    /// `mut name: T` for a variable (its `///` doc appended), and the
    /// convention-carrying `own x: T` / `x: &mut T` / `x: T` for a parameter
    /// (a function-typed parameter shows its `|A| R` closure shape). A use site
    /// resolves through to its binding, so both the declaration and every use
    /// hover the same. The type is the resolved label the analyzer pre-rendered
    /// (`expr_types`) — the element type for a destructured binder. Returns
    /// `None` for anything that is not a binding, leaving the bare-type path.
    fn binding_hover(&self, program: &Program, id: Id) -> Option<String> {
        let binding = match program.entity_map.get(&id) {
            Some(Expr::Local(inner) | Expr::Variable(inner) | Expr::Parameter(inner)) => *inner,
            _ => id,
        };
        if let Some(variable) = program.variables.get(&binding) {
            let type_label = program.expr_types.get(&binding)?;
            let keyword = if variable.mutable { "mut" } else { "let" };
            let mut signature = format!("{keyword} {}: {type_label}", variable.name);
            // A `const`-initialized binding shows its evaluated VALUE too (E9).
            if let Some(value) = self.const_value_label(program, binding) {
                signature.push_str(&format!(" = {value}"));
            }
            let mut out = format!("```vilan\n{signature}\n```");
            if let Some(docs) = self.analysis(program).doc_comment_of(binding) {
                out.push_str("\n\n");
                out.push_str(&docs);
            }
            return Some(out);
        }
        if let Some(parameter) = program.parameters.get(&binding) {
            let type_label = program.expr_types.get(&binding)?;
            return Some(format!(
                "```vilan\n{}\n```",
                parameter_signature(parameter, type_label)
            ));
        }
        None
    }

    /// The hover for a MEMBER read — `foo.bar` with the cursor on the member
    /// expression — in the house style (E72): the fenced `bar: T`, the
    /// member's name from the expression itself, the type the analyzer
    /// rendered for it. A member *call* resolves through
    /// [`Self::function_target`] to its declaration before this runs; the
    /// call shape is skipped here rather than dressed in a field's clothes.
    /// `None` for anything that is not a member read, leaving the bare-type
    /// path.
    fn member_hover(&self, program: &Program, id: Id) -> Option<String> {
        let member_span = program.member_name_spans.get(&id)?;
        if program.function_calls.contains_key(&id) {
            return None;
        }
        let name = self.analyzed_text().get(member_span.into_range())?;
        let type_label = self.analysis(program).hover_label(id)?;
        Some(format!("```vilan\n{name}: {type_label}\n```"))
    }

    /// The struct/enum definition an entity names in VALUE position — a
    /// constructor, a bare type reference, or an enum variant.
    fn type_declaration_target(&self, program: &Program, id: Id) -> Option<Id> {
        if program.structs.contains_key(&id) || program.enums.contains_key(&id) {
            return Some(id);
        }
        match program.entity_map.get(&id)? {
            Expr::Struct(struct_id) => Some(*struct_id),
            Expr::StructInitializer(initializer_id, _) => program
                .struct_initializer_to_def
                .get(initializer_id)
                .copied(),
            Expr::Enum(enum_id) | Expr::EnumVariant(enum_id, _) => Some(*enum_id),
            _ => None,
        }
    }

    /// A constant's evaluated value for hover (`= 42`), when `id` is (or
    /// names) a binding whose initializer is a `const` expression the
    /// evaluation resolved. Rendered compactly and clamped — hover is a
    /// glance, not a dump.
    fn const_value_label(&self, program: &Program, id: Id) -> Option<String> {
        use vilan_core::analyzer::Expr;
        let binding = match program.entity_map.get(&id)? {
            Expr::Local(binding) | Expr::Variable(binding) => *binding,
            _ => id,
        };
        let initial = program.variables.get(&binding)?.initial?;
        let value = program.const_results.get(&initial)?;
        fn render(value: &vilan_core::interpreter::ConstValue, out: &mut String) {
            use vilan_core::interpreter::ConstValue;
            match value {
                ConstValue::Undefined => out.push_str("undefined"),
                ConstValue::Null => out.push_str("null"),
                ConstValue::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
                ConstValue::Number(value) => out.push_str(&value.to_string()),
                ConstValue::BigInt(value) => {
                    out.push_str(&value.to_string());
                    out.push('n');
                }
                ConstValue::Str(value) => {
                    out.push('"');
                    out.push_str(value);
                    out.push('"');
                }
                ConstValue::Array(items) => {
                    out.push('[');
                    for (index, item) in items.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        render(item, out);
                        if out.len() > 120 {
                            out.push('…');
                            break;
                        }
                    }
                    out.push(']');
                }
                ConstValue::Set(items) => {
                    out.push_str("Set[");
                    for (index, item) in items.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        render(item, out);
                        if out.len() > 120 {
                            out.push('…');
                            break;
                        }
                    }
                    out.push(']');
                }
                ConstValue::Map(entries) => {
                    out.push_str("Map[");
                    for (index, (key, entry)) in entries.iter().enumerate() {
                        if index > 0 {
                            out.push_str(", ");
                        }
                        render(key, out);
                        out.push_str(": ");
                        render(entry, out);
                        if out.len() > 120 {
                            out.push('…');
                            break;
                        }
                    }
                    out.push(']');
                }
            }
        }
        let mut rendered = String::new();
        render(value, &mut rendered);
        Some(clamp_preview(rendered))
    }

    /// The definition location `(file, span)` for the entity under `offset`.
    pub fn definition(&self, offset: usize) -> Option<(SourceId, Span)> {
        let program = self.program.as_ref()?;
        // A type name in type position resolves straight to its definition (type
        // references aren't entities). Being inside one but with no navigable
        // target (a generic) yields nothing rather than falling through.
        if let Some((definition, _)) = self.type_reference_at(program, offset) {
            let definition = definition?;
            return Some((
                program.source_of(definition)?,
                self.analysis(program).definition_name_span(definition)?,
            ));
        }
        let id = self.entity_at(offset)?;
        self.definition_of(program, id)
    }

    /// The innermost type reference under `offset` in the open file, as
    /// `(definition id, label)`.
    /// Inlay type hints: `: T` after each UNANNOTATED binding whose type
    /// resolved — inference made a decision the source doesn't show, so the
    /// editor shows it in place. Sorted by position.
    pub fn inlay_hints(&self) -> Vec<(usize, String)> {
        let Some(program) = self.program.as_ref() else {
            return Vec::new();
        };
        let mut hints: Vec<(usize, String)> = Vec::new();
        for (id, variable) in &program.variables {
            if variable.annotated || program.source_of(*id) != Some(SourceId(0)) {
                continue;
            }
            let Some(label) = program.expr_types.get(id) else {
                continue;
            };
            if label.is_empty() || label == "?" || label.contains("Unknown") {
                continue;
            }
            let range = variable.name_span.into_range();
            if range.is_empty() {
                continue;
            }
            hints.push((range.end, format!(": {label}")));
        }
        hints.sort();
        hints
    }

    /// The entry document's semantic tokens (E2), name-sized and
    /// non-overlapping, sorted by position. Classification comes from the
    /// ANALYZED program: declaration name spans, identifier-sized reference
    /// entities, method-call name spans, and type-position references (whose
    /// definitions also cover macro names — they share trait names by design,
    /// and only semantics can tell them apart).
    pub fn semantic_tokens(&self) -> Vec<(Span, TokenKind, u32)> {
        let Some(program) = self.program.as_ref() else {
            return Vec::new();
        };
        let entry = |id: Id| program.source_of(id) == Some(SourceId(0));
        let mut tokens: Vec<(Span, TokenKind, u32)> = Vec::new();
        let classify_target = |target: Id| -> TokenKind {
            use vilan_core::analyzer::Expr;
            match program.entity_map.get(&target) {
                Some(Expr::Function(_)) | Some(Expr::ExternalFunction(_)) => TokenKind::Function,
                Some(Expr::Struct(_)) => TokenKind::Struct,
                Some(Expr::Enum(_)) => TokenKind::Enum,
                Some(Expr::EnumVariant(_, _)) => TokenKind::EnumMember,
                Some(Expr::Trait(_)) => TokenKind::Interface,
                Some(Expr::Module(_)) => TokenKind::Namespace,
                Some(Expr::Generic(_)) => TokenKind::TypeParameter,
                Some(Expr::Macro) => TokenKind::Macro,
                _ => {
                    if program.parameters.contains_key(&target) {
                        TokenKind::Parameter
                    } else {
                        TokenKind::Variable
                    }
                }
            }
        };
        // Declaration names.
        for (id, function) in &program.functions {
            if entry(*id) {
                tokens.push((
                    function.name_span,
                    TokenKind::Function,
                    MODIFIER_DECLARATION,
                ));
            }
        }
        for (id, struct_) in &program.structs {
            if entry(*id) {
                tokens.push((struct_.name_span, TokenKind::Struct, MODIFIER_DECLARATION));
            }
        }
        for (id, enum_) in &program.enums {
            if entry(*id) {
                tokens.push((enum_.name_span, TokenKind::Enum, MODIFIER_DECLARATION));
            }
        }
        for (id, trait_) in &program.traits {
            if entry(*id) {
                tokens.push((trait_.name_span, TokenKind::Interface, MODIFIER_DECLARATION));
            }
        }
        for (id, variable) in &program.variables {
            if entry(*id) {
                let readonly = if variable.mutable {
                    0
                } else {
                    MODIFIER_READONLY
                };
                tokens.push((
                    variable.name_span,
                    TokenKind::Variable,
                    MODIFIER_DECLARATION | readonly,
                ));
            }
        }
        for (id, _parameter) in &program.parameters {
            // A parameter entity's `span_map` entry IS its name.
            if entry(*id)
                && let Some(span) = span_of(program, *id)
            {
                tokens.push((span, TokenKind::Parameter, MODIFIER_DECLARATION));
            }
        }
        // Identifier-sized reference entities.
        {
            use vilan_core::analyzer::Expr;
            for (id, expr) in &program.entity_map {
                if !entry(*id) {
                    continue;
                }
                let Some(span) = span_of(program, *id) else {
                    continue;
                };
                let range = span.into_range();
                if range.start >= range.end {
                    continue;
                }
                match expr {
                    Expr::Local(target) => {
                        let readonly = match program.variables.get(target) {
                            Some(variable) if !variable.mutable => MODIFIER_READONLY,
                            _ => 0,
                        };
                        tokens.push((span, classify_target(*target), readonly));
                    }
                    Expr::Generic(_) => tokens.push((span, TokenKind::TypeParameter, 0)),
                    Expr::Module(_) => tokens.push((span, TokenKind::Namespace, 0)),
                    _ => {}
                }
            }
        }
        // Method-call names — a member with a call is a method, a plain
        // member read is a property (field).
        for (call_id, span) in &program.member_name_spans {
            if !entry(*call_id) {
                continue;
            }
            let kind = if program.function_calls.contains_key(call_id) {
                TokenKind::Method
            } else {
                TokenKind::Property
            };
            tokens.push((*span, kind, 0));
        }
        // Type-position references (macro names arrive here too).
        for (source, span, definition, _) in &program.type_references {
            if *source != SourceId(0) {
                continue;
            }
            // A reference with no resolved definition (an unresolved or
            // synthetic segment) stays untokenized — TextMate's base layer
            // keeps whatever it had.
            let Some(kind) = definition.map(classify_target) else {
                continue;
            };
            tokens.push((*span, kind, 0));
        }
        // Markup (element-syntax S5): tags and attribute names come from a
        // RAW parse — the desugar retires `Node::Element` before analysis, so
        // the analyzed program has no entity for a tag and the closing tag
        // has no node at all. The `view` scaffolding accessor keeps its wide
        // `<tag` span for the missing-import note, so its Function token is
        // suppressed here and the tag painted from the markup itself. (A
        // per-request parse is the `keyword_hover` pattern: cheap, and
        // independent of analysis succeeding.)
        {
            let mut markup = MarkupSpans::default();
            let (tree, _errors) = vilan_core::parsing::parse(self.analyzed_text());
            if let Some(root) = &tree {
                for item in &root.0 {
                    collect_markup_spans(item, &mut markup);
                }
            }
            if !markup.scaffolding.is_empty() {
                let scaffolding: std::collections::HashSet<(usize, usize)> = markup
                    .scaffolding
                    .iter()
                    .map(|span| (span.start, span.end))
                    .collect();
                tokens.retain(|(span, _, _)| !scaffolding.contains(&(span.start, span.end)));
            }
            for span in markup.tags {
                tokens.push((span, TokenKind::Tag, 0));
            }
            for span in markup.attributes {
                tokens.push((span, TokenKind::Property, 0));
            }
        }

        // Sort and drop overlaps: narrowest-first at each start, then keep
        // strictly non-overlapping tokens (the LSP requires it).
        tokens.sort_by_key(|(span, _, _)| {
            let range = span.into_range();
            (range.start, range.end - range.start)
        });
        let mut kept: Vec<(Span, TokenKind, u32)> = Vec::new();
        let mut last_end = 0usize;
        for (span, kind, modifiers) in tokens {
            let range = span.into_range();
            if range.start >= last_end && range.start < range.end {
                last_end = range.end;
                kept.push((span, kind, modifiers));
            }
        }
        // B38, the salvage signature: when the fresh stream is entirely
        // silent within the retained suffix — the shape of a parse break
        // truncating the file to a prefix — the previous analysis's tokens
        // for the byte-identical tail fill in, already shifted. A stream
        // that reaches the suffix suppresses this wholesale, which is what
        // keeps re-classification of identical text (semantics flow
        // downward) fresh rather than stale.
        if !self.retained_tail.is_empty()
            && kept
                .iter()
                .all(|(span, ..)| span.end <= self.retained_tail_start)
        {
            kept.extend(
                self.retained_tail
                    .iter()
                    .filter(|(span, ..)| span.start >= self.retained_tail_start)
                    .cloned(),
            );
        }
        kept
    }

    fn type_reference_at(&self, program: &Program, offset: usize) -> Option<(Option<Id>, String)> {
        program
            .type_references
            .iter()
            .filter(|(source, span, _, _)| {
                *source == SourceId(0) && {
                    let range = span.into_range();
                    range.start <= offset && offset < range.end
                }
            })
            .min_by_key(|(_, span, _, _)| {
                let range = span.into_range();
                range.end - range.start
            })
            .map(|(_, _, definition, label)| (*definition, label.clone()))
    }

    fn definition_of(&self, program: &Program, id: Id) -> Option<(SourceId, Span)> {
        // The call → subject chain is walked iteratively with a seen-list —
        // the same guard as `hover_label` and `function_target` (E73): the
        // chain is data a lowering can rewire, and a cycle must answer `None`
        // rather than recurse off the stack.
        let mut seen: Vec<Id> = Vec::new();
        let mut current = id;
        loop {
            if seen.contains(&current) {
                return None;
            }
            seen.push(current);
            // A hidden context parameter has no source declaration to land
            // on — it is compiler-minted (E75). The explicit honest `None`.
            if program.context_hidden_parameters.contains_key(&current) {
                return None;
            }
            // A call whose ENTITY record the context pass overwrote (a plain
            // `get` becomes a parameter read, a none-rooted safe read a
            // `None` literal, `Context::new()` an opaque `Null`) still
            // carries its call record, whose subject names the source
            // callee — resolve through it, as `function_target` does (E75).
            if program.function_calls.contains_key(&current)
                && !matches!(program.entity_map.get(&current), Some(Expr::Call(_)))
            {
                current = source_call_subject(program, current)?;
                continue;
            }
            return match program.entity_map.get(&current)? {
                Expr::Local(binding) | Expr::Variable(binding) | Expr::Parameter(binding) => {
                    // Resolve to the name span of the thing the binding actually is —
                    // a function, a `let`/`mut` variable, or (parameters/generics,
                    // whose `span_map` entry is already the name) the span itself.
                    if let Some(function) = program.functions.get(binding) {
                        return Some((program.source_of(*binding)?, function.name_span));
                    }
                    if let Some(function) = program.external_functions.get(binding) {
                        return Some((program.source_of(*binding)?, function.name_span));
                    }
                    if let Some(variable) = program.variables.get(binding) {
                        return Some((program.source_of(*binding)?, variable.name_span));
                    }
                    Some((program.source_of(*binding)?, span_of(program, *binding)?))
                }
                Expr::Field(_, struct_id, index) => {
                    let field = program.structs.get(struct_id)?.fields.get(*index)?;
                    Some((program.source_of(*struct_id)?, field.name_span))
                }
                Expr::EnumVariant(enum_id, _) => {
                    Some((program.source_of(*enum_id)?, span_of(program, *enum_id)?))
                }
                Expr::Call(call_id) => {
                    // The SOURCE subject: the erased original where the
                    // context pass rewired the call record (E75).
                    current = source_call_subject(program, *call_id)?;
                    continue;
                }
                Expr::Function(function_id) => Some((
                    program.source_of(*function_id)?,
                    program.functions.get(function_id)?.name_span,
                )),
                Expr::ExternalFunction(function_id) => Some((
                    program.source_of(*function_id)?,
                    program.external_functions.get(function_id)?.name_span,
                )),
                Expr::Struct(struct_id) => Some((
                    program.source_of(*struct_id)?,
                    program.structs.get(struct_id)?.name_span,
                )),
                Expr::StructInitializer(initializer_id, _) => {
                    let struct_id = program.struct_initializer_to_def.get(initializer_id)?;
                    Some((
                        program.source_of(*struct_id)?,
                        program.structs.get(struct_id)?.name_span,
                    ))
                }
                Expr::Enum(enum_id) => Some((
                    program.source_of(*enum_id)?,
                    program.enums.get(enum_id)?.name_span,
                )),
                Expr::Trait(trait_id) => Some((
                    program.source_of(*trait_id)?,
                    program.traits.get(trait_id)?.name_span,
                )),
                _ => None,
            };
        }
    }

    /// All references to the symbol under `offset` (including its declaration),
    /// as `(file, span)` with each span covering exactly the identifier.
    ///
    /// Answered from the reference index: the cursor is resolved by looking
    /// `offset` up in the *same* identifier-occurrence table the answer is drawn
    /// from, so resolution and enumeration cannot disagree — a symbol kind the
    /// index can find is necessarily one it can also enumerate. An empty result
    /// means the cursor is not on an identifier, and nothing else.
    pub fn references(&self, offset: usize) -> Vec<(SourceId, Span)> {
        let Some((definition, _)) = self.reference_target(offset) else {
            return Vec::new();
        };
        self.reference_index
            .occurrences_of(definition)
            .map(|occurrence| (occurrence.source, occurrence.span))
            .collect()
    }

    /// Every occurrence of the definition under `offset`, unioned over this
    /// document's program AND each `neighbors` (the other open documents')
    /// program, as `(canonical file path, span)` — kolt.local 034 (003 branch
    /// (c)).
    ///
    /// A single program reaches only its own import closure, so a symbol
    /// queried in the file that DEFINES it cannot see its importers from any
    /// one program — the declaration came back and nothing else. Each
    /// importer's program has already indexed those occurrences; this query
    /// re-resolves the definition there through its cross-program
    /// [`DefinitionKey`] and merges the answers. The union is in path space
    /// because source ids are per-program; it is deduplicated by `(path,
    /// span)` because the declaration itself is a row in every program that
    /// loaded its file. An occurrence in generated code has no file and is
    /// dropped, exactly as the location conversion has always dropped it.
    pub fn references_across<'a>(
        &self,
        offset: usize,
        neighbors: impl IntoIterator<Item = &'a Document>,
    ) -> Vec<(PathBuf, Span)> {
        let Some((definition, _)) = self.reference_target(offset) else {
            return Vec::new();
        };
        let own = self
            .reference_index
            .occurrences_of(definition)
            .map(|occurrence| (occurrence.source, occurrence.span))
            .collect();
        let mut merged = self.spans_by_path(own);
        if let Some(key) = self.definition_key(definition) {
            for neighbor in neighbors {
                // `depends_on` scopes the union (B39a's edge, reused): a
                // program that never loaded the defining file cannot hold a
                // reference to the definition.
                if !neighbor.depends_on(key.path()) {
                    continue;
                }
                let Some(local) = neighbor.definition_of_key(&key) else {
                    continue;
                };
                let found = neighbor
                    .reference_index
                    .occurrences_of(local)
                    .map(|occurrence| (occurrence.source, occurrence.span))
                    .collect();
                merged.extend(neighbor.spans_by_path(found));
            }
        }
        merged.sort();
        merged.dedup();
        merged
    }

    /// The cross-program identity of `definition` — [`ReferenceIndex::key_of`]
    /// over this document's program. `None` when the definition has no
    /// declaration address another program could agree on.
    pub fn definition_key(
        &self,
        definition: Definition,
    ) -> Option<crate::references::DefinitionKey> {
        let program = self.program.as_ref()?;
        self.reference_index.key_of(program, definition)
    }

    /// The definition `key` names in THIS document's program —
    /// [`ReferenceIndex::definition_of_key`]'s document form.
    pub fn definition_of_key(&self, key: &crate::references::DefinitionKey) -> Option<Definition> {
        let program = self.program.as_ref()?;
        self.reference_index.definition_of_key(program, key)
    }

    /// `(source, span)` rows from this document's program in `(canonical file
    /// path, span)` form — the program-independent coordinates the
    /// cross-document union merges in. A row whose source has no path
    /// (generated code) is dropped.
    fn spans_by_path(&self, spans: Vec<(SourceId, Span)>) -> Vec<(PathBuf, Span)> {
        let Some(program) = self.program.as_ref() else {
            return Vec::new();
        };
        spans
            .into_iter()
            .filter_map(|(source, span)| {
                program
                    .canonical_sources
                    .get(source.0 as usize)
                    .map(|path| (path.clone(), span))
            })
            .collect()
    }

    /// The canonical path of the file this document's analysis read as its
    /// entry (`None` when nothing was analyzed) — how the location conversion
    /// recognizes a path-space span as belonging to an open document.
    pub fn entry_path(&self) -> Option<&Path> {
        self.program
            .as_ref()?
            .canonical_sources
            .first()
            .map(PathBuf::as_path)
    }

    /// The definition the identifier under `offset` names, with its kind — the
    /// shared front half of find-references and rename.
    pub fn reference_target(&self, offset: usize) -> Option<(Definition, DefinitionKind)> {
        let program = self.program.as_ref()?;
        let occurrence = self.reference_index.at(SourceId(0), offset)?;
        let kind = crate::references::kind_of(program, occurrence.definition)?;
        Some((occurrence.definition, kind))
    }

    /// The spans a rename of the symbol under `offset` must rewrite, or the
    /// reason it cannot.
    ///
    /// This is a thin layer over the reference index — deliberately, since
    /// rename and find-references disagreeing about what a symbol's references
    /// ARE is precisely the class kolt.local 002 and 003 were two faces of. The
    /// layer adds only what rename needs beyond finding: that the new name is
    /// spellable, that every reference sits in a file this project may edit, and
    /// that none are known to be missing.
    pub fn rename_edits(
        &self,
        offset: usize,
        new_name: &str,
    ) -> std::result::Result<Vec<(SourceId, Span)>, RenameRefusal> {
        let (definition, what) = self.rename_target(offset, new_name)?;
        self.rename_spans(definition, &what)
    }

    /// [`Document::rename_edits`] unioned over each `neighbors` (the other
    /// open documents') program, in `(canonical file path, span)` form — the
    /// rename face of [`Document::references_across`], per the standing rule
    /// that rename reads the same index reach find-references does. Every
    /// contributing program's spans pass the same per-program validation the
    /// single-document rename runs, so a refusal reason cannot be laundered
    /// away by the union.
    pub fn rename_edits_across<'a>(
        &self,
        offset: usize,
        new_name: &str,
        neighbors: impl IntoIterator<Item = &'a Document>,
    ) -> std::result::Result<Vec<(PathBuf, Span)>, RenameRefusal> {
        let (definition, what) = self.rename_target(offset, new_name)?;
        let mut merged = self.spans_by_path(self.rename_spans(definition, &what)?);
        if let Some(key) = self.definition_key(definition) {
            for neighbor in neighbors {
                if !neighbor.depends_on(key.path()) {
                    continue;
                }
                // A stale importer's analyzed program may also MISS a
                // just-typed reference, so this refuses before asking whether
                // the key even resolves there — conservative, and over in one
                // debounce.
                if neighbor.is_stale() {
                    return Err(RenameRefusal::StillAnalyzing { what });
                }
                let Some(local) = neighbor.definition_of_key(&key) else {
                    continue;
                };
                merged.extend(neighbor.spans_by_path(neighbor.rename_spans(local, &what)?));
            }
        }
        merged.sort();
        merged.dedup();
        Ok(merged)
    }

    /// The shared front half of the rename entry points: the definition under
    /// `offset` and the phrase refusals name it by, with `new_name` validated.
    fn rename_target(
        &self,
        offset: usize,
        new_name: &str,
    ) -> std::result::Result<(Definition, String), RenameRefusal> {
        let Some(program) = self.program.as_ref() else {
            return Err(RenameRefusal::NotAnIdentifier);
        };
        let Some((definition, kind)) = self.reference_target(offset) else {
            return Err(RenameRefusal::NotAnIdentifier);
        };
        if !is_identifier(new_name) {
            return Err(RenameRefusal::InvalidName(new_name.to_string()));
        }
        let name = crate::references::name_of(program, definition).unwrap_or("this symbol");
        Ok((definition, format!("the {} `{name}`", kind.noun())))
    }

    /// The per-program back half: every span a rename of `definition` must
    /// rewrite IN THIS PROGRAM, or the reason the edit set cannot be complete.
    /// `definition` is this program's own (for a neighbor, the result of
    /// re-resolving the origin's [`crate::references::DefinitionKey`] here).
    fn rename_spans(
        &self,
        definition: Definition,
        what: &str,
    ) -> std::result::Result<Vec<(SourceId, Span)>, RenameRefusal> {
        let Some(program) = self.program.as_ref() else {
            return Err(RenameRefusal::NotAnIdentifier);
        };
        let missing = self.unindexed_references(definition);
        if missing > 0 {
            return Err(RenameRefusal::Incomplete {
                what: what.to_string(),
                missing,
            });
        }

        let spans: Vec<(SourceId, Span)> = self
            .reference_index()
            .occurrences_of(definition)
            .map(|occurrence| (occurrence.source, occurrence.span))
            .collect();
        if spans.is_empty() {
            return Err(RenameRefusal::NotAnIdentifier);
        }
        for (source, _) in &spans {
            // Generated code has no path, so an edit there cannot be expressed —
            // and dropping it silently is the partial rename the rule forbids.
            if *source == DERIVED_SOURCE {
                return Err(RenameRefusal::Generated {
                    what: what.to_string(),
                });
            }
            // A rename reached through an import must not rewrite the library it
            // reached into. The old code would happily hand the client edits for
            // files under `$VILAN_STD`.
            if program.std_sources.contains(source) {
                return Err(RenameRefusal::NotOwned {
                    what: what.to_string(),
                    origin: "the standard library",
                });
            }
            if program.dependency_sources.contains(source) {
                return Err(RenameRefusal::NotOwned {
                    what: what.to_string(),
                    origin: "a dependency",
                });
            }
        }
        Ok(spans)
    }

    /// The reference index this document's queries read.
    pub(crate) fn reference_index(&self) -> &ReferenceIndex {
        &self.reference_index
    }

    /// How many references to `definition` the index knows it is missing: use
    /// sites whose recorded span could not be proven to cover an identifier.
    /// Non-zero means an edit set over this definition would be incomplete.
    pub fn unindexed_references(&self, definition: Definition) -> usize {
        self.reference_index.dropped_for(definition)
    }

    /// The "Organize Imports" edits (WO-2): the top-level import runs sorted into
    /// canonical order — the same order `vilan fmt` produces, through the shared
    /// `formatter::organize_import_runs` — and unused imports pruned. Returns one
    /// `(span, replacement)` per run whose canonical form differs from the source;
    /// empty when already organized (the action then offers nothing).
    ///
    /// Pruning is conservative. It happens only when the analyzed program matches
    /// the current buffer exactly and carries no diagnostics — a mid-edit
    /// unresolved name might be about to use an import, so a broken or stale
    /// document sorts but never prunes. Re-exports are never pruned (handled in
    /// the formatter — they are surface, not usage), and an import a macro
    /// expansion references is kept (see `unused_import_leaf_spans`).
    pub fn organize_import_edits(&self) -> Vec<(Span, String)> {
        // The LIVE text: the returned spans come from the formatter's own parse
        // of this string, so they are live-space and the handler converts them
        // through the live index. (The handler also refuses outright while the
        // snapshots diverge — S3 — so in practice the two texts are equal here.)
        let source = self.text.as_str();
        // Prune only against a fresh, diagnostic-free analysis of THIS buffer: a
        // stale or broken document (a mid-edit unresolved name might be about to
        // use an import) sorts but never prunes.
        let prunable_program = self
            .program
            .as_ref()
            .filter(|_| self.diagnostics.is_empty() && !self.is_stale());
        let edits = match prunable_program {
            Some(program) => {
                // Computed once for the whole pass: which byte ranges belong to
                // the file's import list, so a reference written there is not
                // mistaken for the file using the import.
                let import_spans = vilan_core::formatter::import_statement_spans(source);
                let keep =
                    |leaf_span: Span| self.import_leaf_is_used(program, leaf_span, &import_spans);
                vilan_core::formatter::organize_import_runs(source, &keep)
            }
            None => vilan_core::formatter::organize_import_runs(source, &|_| true),
        };
        edits
            .map(|edits| {
                edits
                    .into_iter()
                    .map(|edit| (edit.span, edit.replacement))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the top-level import whose terminal name occupies `leaf_span` is
    /// used, so the organizer keeps it. Maps the leaf to the definition it binds
    /// (`resolve_import` records the leaf as a reference at its own span — see
    /// `flatten_namespace_branch`/`record_reference`), then asks whether that
    /// definition is referenced anywhere in this file beyond the import itself.
    /// An unmappable leaf (an import that didn't resolve — but then the file
    /// carries a diagnostic and pruning is off) is conservatively kept.
    ///
    /// Conservatism, per the surfaces a use can land in: a reference on ANY of
    /// them keeps the import.
    ///  - (A) Type/trait positions — `type_references`, from this file or from
    ///    code generated out of it, so a derive-only import survives (`[derive(
    ///    Json)]` alone references `Json`). Generated references used to arrive
    ///    mislabeled as this file's own and were counted by accident; they now
    ///    carry `DERIVED_SOURCE` and are counted on purpose, which is what (B)
    ///    below has always done.
    ///  - (B) Value positions (call subject, bare value) — the entity map, whose
    ///    per-use source lets us filter to this file (or code generated from it).
    ///    `reference_count` is deliberately NOT used: an import binds its leaf
    ///    directly to the shared definition Id, so that tally aggregates uses
    ///    across every file and reads ~0 for type-only imports.
    ///  - (C) Struct constructors (`Point { .. }`) — the initializer map.
    fn import_leaf_is_used(
        &self,
        program: &Program,
        leaf_span: Span,
        import_spans: &[Span],
    ) -> bool {
        let entry = SourceId(0);
        let Some(definition_id) = program
            .type_references
            .iter()
            .find_map(|(source, span, definition, _)| {
                (*source == entry && *span == leaf_span).then_some(*definition)
            })
            .flatten()
        else {
            // The leaf binds nothing this analysis recorded — keep it, since
            // pruning on no evidence is how a green build gets broken.
            return true;
        };
        let definition = Definition::Entity(definition_id);

        // A reference written by the file's IMPORT LIST is not the file using
        // anything: an import path's segments resolve to the same definitions
        // its leaves bind, so counting them let an import justify itself.
        let written_in_an_import = |span: Span| {
            import_spans
                .iter()
                .any(|statement| statement.start <= span.start && span.end <= statement.end)
        };
        let used_here = |occurrence: &crate::references::Occurrence| match occurrence.source {
            // Derive-generated code indexes a template, so its offsets are not
            // this file's and there is no import list to exclude — any reference
            // among them is a real use.
            DERIVED_SOURCE => true,
            source => source == entry && !written_in_an_import(occurrence.span),
        };

        // (1) The file's own code names the definition — as a type, as a value,
        // as a constructor, as a method callee, as an enum variant. One question
        // now, because the reference index answers all of those from one table.
        if self
            .reference_index
            .occurrences_of(definition)
            .any(used_here)
        {
            return true;
        }

        // (2) A whole-module import brings more than its own name: every `impl`
        // in that module's file arrives with it. So a method call whose
        // implementation lives there IS a use of the import, even though the
        // module name is never written — and pruning it breaks the build, which
        // is the over-pruning half of kolt.local 004. The accounting is the
        // analyzer's own provenance: did this file resolve anything DECLARED in
        // the file this import reaches into?
        if matches!(
            crate::references::kind_of(program, definition),
            Some(crate::references::DefinitionKind::Module)
        ) {
            if let Some(home) = program.source_of(definition_id) {
                // A module whose file is this one brings nothing new, and would
                // otherwise match every local declaration and never prune.
                if home != entry {
                    return self
                        .reference_index
                        .occurrences_in(entry)
                        .any(|occurrence| {
                            !written_in_an_import(occurrence.span)
                                && crate::references::declaration_source(
                                    program,
                                    occurrence.definition,
                                ) == Some(home)
                        });
                }
            }
        }
        false
    }

    /// Whether a use site belongs to the entry file or to code generated from it
    /// (a derive expansion) — the two sources whose references keep an import.
    fn use_in_entry_or_generated(&self, program: &Program, use_id: Id) -> bool {
        matches!(
            program.source_of(use_id),
            Some(SourceId(0)) | Some(DERIVED_SOURCE)
        )
    }

    // --- Quickfixes: add-import, closest-name field rename (E54, E58) ------

    /// Every place `name` may be imported from — origins in loader order
    /// (`std`, `pkg`, each dependency by its import name), the package's own
    /// surface before its modules within an origin — searched via the E57
    /// machinery (import-path completion's own candidate source, repointed at
    /// a NAME instead of a path prefix). Each hit is the segments an `import`
    /// needs before `name` (`["std", "json"]` for `Json`; `["std"]` for a
    /// name std's own surface re-exports directly, like `print`). More than
    /// one hit is a genuine ambiguity: the caller decides (E54b offers one
    /// action per candidate; E54d's "fix all" skips the name rather than
    /// guess).
    ///
    /// One full-origin scan per call — bounded by std's own module count
    /// (E57: ~0.64 ms cold / 0.035 ms warm per module through the shared
    /// parse cache), which is fine for an on-demand quickfix but too slow to
    /// pay per candidate on every keystroke; [`Self::auto_import_completions`]
    /// uses the cheaper already-loaded `Program` maps instead.
    fn import_candidates(&self, program: &Program, name: &str) -> Vec<Vec<String>> {
        let Some(roots) = self.import_roots.as_ref() else {
            return Vec::new();
        };
        let mut origins: Vec<String> = vec!["std".to_string(), "pkg".to_string()];
        origins.extend(
            roots
                .dependencies
                .iter()
                .map(|(dep_name, _)| dep_name.clone()),
        );
        let mut candidates = Vec::new();
        for origin in &origins {
            let Some((module_roots, surface)) = roots.origin_roots(origin, program.platform) else {
                continue;
            };
            if let Some(surface_path) = &surface {
                if vilan_core::analyzer::module_importables(surface_path)
                    .iter()
                    .any(|importable| importable.name == name)
                {
                    candidates.push(vec![origin.clone()]);
                }
            }
            let mut seen_modules: HashSet<String> = HashSet::new();
            for root in &module_roots {
                for (module_name, module_path) in vilan_core::analyzer::modules_in_root(root) {
                    if module_name == "lib" || !seen_modules.insert(module_name.clone()) {
                        continue;
                    }
                    if vilan_core::analyzer::module_importables(&module_path)
                        .iter()
                        .any(|importable| importable.name == name)
                    {
                        candidates.push(vec![origin.clone(), module_name]);
                    }
                }
            }
        }
        candidates
    }

    /// The quickfix menu for the diagnostics overlapping `range` (LIVE
    /// space — safe because the caller gates staleness first, S3: while
    /// non-stale, live spans and this document's own `diagnostics` spans
    /// address the same text): one action per unambiguous add-import
    /// candidate (E54b — several when a name is AMBIGUOUS across modules,
    /// never guessed), and the field-rename fix on a closest-name suggestion
    /// (E58c). Reads THIS document's own diagnostics directly rather than the
    /// client-echoed `context.diagnostics` — only ours carries the span and
    /// note data a fix needs, and the staleness refusal is what makes that a
    /// safe substitution.
    pub fn quickfixes(&self, program: &Program, range: Span) -> Vec<QuickFix> {
        let mut fixes = Vec::new();
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if self
                .diagnostic_sources
                .get(index)
                .copied()
                .unwrap_or(SourceId(0))
                != SourceId(0)
            {
                continue; // an edit can only ever reach this document
            }
            if !spans_overlap(diagnostic.span, range) {
                continue;
            }
            if let Some(name) = unresolved_name(&diagnostic.msg) {
                for module_path in self.import_candidates(program, name) {
                    let path_refs: Vec<&str> = module_path.iter().map(String::as_str).collect();
                    let Some(edit) =
                        vilan_core::formatter::insert_import(&self.text, &path_refs, name)
                    else {
                        continue;
                    };
                    fixes.push(QuickFix {
                        title: format!("Import `{name}` from {}", module_path.join("::")),
                        span: edit.span,
                        replacement: edit.replacement,
                    });
                }
            } else if let Some(suggestion) = diagnostic
                .note
                .as_ref()
                .and_then(|note| closest_name_suggestion(&note.msg))
            {
                fixes.push(QuickFix {
                    title: format!("Change to `{suggestion}`"),
                    span: diagnostic.span,
                    replacement: suggestion.to_string(),
                });
            } else if diagnostic.msg.starts_with(MISSING_TERMINATOR_MESSAGE) {
                // S2 (editing-dx.md §17.4, E54's home): the diagnostic's own
                // span IS the gap — the parser's `gap_span` already computed
                // "the last character before the `;` belongs", one character
                // wide (§4.4/§15.5) — so the fix is a zero-width insertion
                // right after it. No program lookup needed: the parser's
                // anchor is the whole answer.
                let insertion = diagnostic.span.end;
                fixes.push(QuickFix {
                    title: "Insert `;`".to_string(),
                    span: Span::from(insertion..insertion),
                    replacement: ";".to_string(),
                });
            } else if diagnostic.msg.ends_with(DISCARDED_VALUE_MESSAGE)
                && let Some(semicolon_span) =
                    trailing_semicolon_to_remove(&self.text, program, diagnostic.span)
            {
                // Regime 1' (S3, editing-dx.md §17.4): the diagnostic anchors
                // at the callable's closing BRACE, not the `;` — the fix
                // locates the `;` from the program's own last-statement
                // bookkeeping (the same question `missing_return_value_
                // message` asks analyzer-side) rather than guessing from the
                // brace backwards through possible comments.
                fixes.push(QuickFix {
                    title: "Remove `;`".to_string(),
                    span: semicolon_span,
                    replacement: String::new(),
                });
            }
        }
        fixes
    }

    /// Every unambiguous missing-import fix in the file, folded into ONE edit
    /// (E54d, the "add all missing imports" source action): each unresolved
    /// name with EXACTLY one candidate module gets imported; an AMBIGUOUS
    /// name (more than one candidate) is skipped outright — this action never
    /// guesses between modules. Fixes apply SEQUENTIALLY against a running
    /// copy of the text, so two names newly imported from the same module
    /// merge into one brace set exactly as two separate manual add-imports
    /// would (`insert_import` sees the first one already landed on the
    /// second call). `None` when there's nothing unambiguous to add.
    pub fn add_all_missing_imports_edit(&self, program: &Program) -> Option<(Span, String)> {
        let mut names: Vec<&str> = Vec::new();
        for (index, diagnostic) in self.diagnostics.iter().enumerate() {
            if self
                .diagnostic_sources
                .get(index)
                .copied()
                .unwrap_or(SourceId(0))
                != SourceId(0)
            {
                continue;
            }
            if let Some(name) = unresolved_name(&diagnostic.msg) {
                if !names.contains(&name) {
                    names.push(name);
                }
            }
        }
        let mut working = self.text.clone();
        let mut changed = false;
        for name in names {
            let candidates = self.import_candidates(program, name);
            let [module_path] = candidates.as_slice() else {
                continue; // zero or ambiguous candidates: never guess
            };
            let path_refs: Vec<&str> = module_path.iter().map(String::as_str).collect();
            if let Some(edit) = vilan_core::formatter::insert_import(&working, &path_refs, name) {
                working = splice(&working, edit.span, &edit.replacement);
                changed = true;
            }
        }
        changed.then(|| (Span::from(0..self.text.len()), working))
    }

    /// The outline of the entry file: functions, structs (with their fields),
    /// enums, and traits, each with its declaration and name spans.
    pub fn document_symbols(&self) -> Vec<Symbol> {
        let Some(program) = self.program.as_ref() else {
            return Vec::new();
        };
        let in_entry = |id: Id| program.source_of(id) == Some(SourceId(0));
        let mut symbols = Vec::new();

        for (id, function) in &program.functions {
            if !in_entry(*id) {
                continue;
            }
            symbols.push(Symbol {
                name: function.name.to_string(),
                kind: SymbolKind::Function,
                full: span_of(program, *id).unwrap_or(function.name_span),
                selection: function.name_span,
                children: Vec::new(),
            });
        }
        for (id, structure) in &program.structs {
            if !in_entry(*id) {
                continue;
            }
            let Some(full) = span_of(program, *id) else {
                continue;
            };
            let children = structure
                .fields
                .iter()
                .map(|field| Symbol {
                    name: field.name.to_string(),
                    kind: SymbolKind::Field,
                    full: field.name_span,
                    selection: field.name_span,
                    children: Vec::new(),
                })
                .collect();
            symbols.push(Symbol {
                name: structure.name.to_string(),
                kind: SymbolKind::Struct,
                full,
                selection: full,
                children,
            });
        }
        for (id, enumeration) in &program.enums {
            if !in_entry(*id) {
                continue;
            }
            let Some(full) = span_of(program, *id) else {
                continue;
            };
            symbols.push(Symbol {
                name: enumeration.name.to_string(),
                kind: SymbolKind::Enum,
                full,
                selection: full,
                children: Vec::new(),
            });
        }
        for (id, trait_definition) in &program.traits {
            if !in_entry(*id) {
                continue;
            }
            let Some(full) = span_of(program, *id) else {
                continue;
            };
            symbols.push(Symbol {
                name: trait_definition.name.to_string(),
                kind: SymbolKind::Trait,
                full,
                selection: full,
                children: Vec::new(),
            });
        }
        symbols
    }

    /// Completion candidates at `offset` — a LIVE-space offset (the caller
    /// converts an LSP `Position` through `line_index`, never `analyzed_offset`:
    /// completion's dispatch reads the buffer the user is mid-keystroke in).
    /// The engine is `vilan_ide`'s, shared with the playground (K9); it
    /// converts to the ANALYZED offset itself wherever it touches `program`
    /// data (E52).
    pub fn completion(&self, offset: usize) -> Vec<Completion> {
        let Some(program) = self.program.as_ref() else {
            return Vec::new();
        };
        self.analysis(program).completion(offset)
    }
}

/// One quickfix's ready-made edit (E54b, E54d, E58c): a menu title and the
/// `(span, replacement)` this document's own text needs — LIVE space, same
/// convention as [`Document::organize_import_edits`].
pub struct QuickFix {
    pub title: String,
    pub span: Span,
    pub replacement: String,
}

/// The name in an unknown-name diagnostic's message: `cannot find 'X' in this
/// scope...` (a bare value) or `cannot find type 'X'...` — the two "cannot
/// find" shapes B4's import steer already targets
/// (`analyzer.rs::import_steer`/`import_steer_inner`). `None` for every other
/// diagnostic shape (a module-path segment, a trait, a struct field, a
/// context …) — E54's add-import quickfix is deliberately scoped to these
/// two; the others are the filing's own later customers (E58d's rule for the
/// closest-name primitive applies here too).
fn unresolved_name(message: &str) -> Option<&str> {
    for prefix in ["cannot find '", "cannot find type '"] {
        if let Some(rest) = message.strip_prefix(prefix) {
            if let Some(end) = rest.find('\'') {
                return Some(&rest[..end]);
            }
        }
    }
    None
}

/// The suggested name in a "did you mean" note E58 attaches to the
/// invalid-initializer-field diagnostic (`analyzer.rs`) — the note text IS
/// the fix's source of truth, so the LSP quickfix never recomputes its own
/// closest-name guess and risks disagreeing with the diagnostic it's fixing.
/// Anchored to the note starting with EXACTLY `did you mean` — the unrelated
/// field/method-callable ambiguity note (`analyzer.rs`, "...: did you mean
/// the plain access `x.member`?") uses the same words mid-sentence, never at
/// the very start, so it can never match here.
fn closest_name_suggestion(note_message: &str) -> Option<&str> {
    note_message
        .strip_prefix("did you mean `")?
        .strip_suffix("`?")
}

/// S2's parse-error message (`parsing.rs::render`, `ParseErrorReason::
/// MissingTerminator`) — matched by PREFIX since a curated parse error can
/// carry a trailing `" in <context>"` label (`render`'s own context loop),
/// which none of the three `note_terminator` call sites currently reach but
/// nothing guarantees against structurally.
const MISSING_TERMINATOR_MESSAGE: &str = "expected `;` to end this statement";

/// Regime 1's message suffix (`analyzer.rs::missing_return_value_message`) —
/// matched by SUFFIX (own sentence, own period) so it can't fire on regime
/// 1's sibling wording ("this body ends without producing a value.") which
/// names a DIFFERENT, non-fixable gap (no statement to blame at all).
const DISCARDED_VALUE_MESSAGE: &str = "the `;` discards this body's last value.";

/// The `;` a regime-1' diagnostic ("the `;` discards this body's last
/// value") names, located from the program's own bookkeeping rather than
/// guessed from the brace backwards. `diagnostic_span` is the callable's
/// closing-brace anchor (S3, editing-dx.md §16/§3.9) — unique per callable
/// in one file — so it pairs with exactly one function or (braced) closure,
/// whose last STATEMENT id (excluding the trailing `;`, consumed separately
/// per §15.6) is already what the analyzer's own
/// `missing_return_value_message` asks about. From that statement's span
/// end, `;` is found by scanning forward past ASCII whitespace only — a
/// comment in the gap declines the fix rather than guessing past it (B4:
/// no fix is better than a wrong one).
fn trailing_semicolon_to_remove(
    text: &str,
    program: &Program,
    diagnostic_span: Span,
) -> Option<Span> {
    let last_statement_id = program
        .functions
        .values()
        .find(|function| {
            program
                .span_map
                .get(&function.body.1)
                .is_some_and(|span| **span == diagnostic_span)
        })
        .and_then(|function| function.body.0.last().copied())
        .or_else(|| {
            program.closures.values().find_map(|closure| {
                let Expr::Block((statement_ids, _)) = program.entity_map.get(&closure.return_)?
                else {
                    return None;
                };
                let block_span = **program.span_map.get(&closure.return_)?;
                let brace_span = Span {
                    start: block_span.end.saturating_sub(1),
                    end: block_span.end,
                };
                if brace_span != diagnostic_span {
                    return None;
                }
                statement_ids.last().copied()
            })
        })?;
    let statement_span = **program.span_map.get(&last_statement_id)?;
    let bytes = text.as_bytes();
    let mut cursor = statement_span.end;
    loop {
        match bytes.get(cursor) {
            Some(byte) if byte.is_ascii_whitespace() => cursor += 1,
            Some(b';') => return Some(Span::from(cursor..cursor + 1)),
            _ => return None,
        }
    }
}

/// Whether two spans share at least one byte position — touching counts, so
/// a zero-width cursor range sitting right at a diagnostic's edge still
/// overlaps it.
fn spans_overlap(a: Span, b: Span) -> bool {
    a.start <= b.end && b.start <= a.end
}

/// Replaces the byte range `span` in `source` with `replacement`. The
/// primitive [`Document::add_all_missing_imports_edit`] folds a SEQUENCE of
/// `insert_import` edits through, each computed against the previous
/// splice's result — so two new imports from the same not-yet-imported
/// module land in one merged brace set, exactly as two separate manual
/// add-imports would.
fn splice(source: &str, span: Span, replacement: &str) -> String {
    let range = span.into_range();
    let mut result =
        String::with_capacity(source.len() - (range.end - range.start) + replacement.len());
    result.push_str(&source[..range.start]);
    result.push_str(replacement);
    result.push_str(&source[range.end..]);
    result
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::path::PathBuf;
    use vilan_ide::completion::{import_path_segments, in_import_path};
    use vilan_ide::{AUTO_IMPORT_COMPLETION_CAP, CONSTRUCT_SNIPPETS, CompletionKind};

    pub(crate) fn std_root() -> PathBuf {
        // The std PACKAGE directory (holding `vilan.toml`), like the server's
        // `discover_std_dir` — pointing at the bare source root instead would
        // drop the manifest's platform layers (no `std::fs`/`std::http`/…).
        std::env::var_os("VILAN_STD")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"))
    }

    /// Runs `work` on a 256 MiB-stack thread and joins it — the stack
    /// `Document::analyze` gives the pipeline, for tests that drive
    /// `analyze_on_this_thread` directly (to read the thread-local leak tally
    /// on the thread that analyzed).
    pub(crate) fn on_big_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(work)
            .expect("spawn measurement thread")
            .join()
            .expect("measurement thread panicked")
    }

    // `same_file` / `is_within` decide a document's package and platform, so a
    // false negative silently drops a file's project context. Both go through
    // `vilan_core::util::canonical_path` (windows-support.md §5) — including
    // the arm where the path is NOT on disk, which used to compare raw strings.
    #[test]
    fn same_file_agrees_across_spellings_of_a_path_not_on_disk() {
        let root = std::env::temp_dir().join(format!("vilan-lsp-samefile-{}", std::process::id()));
        let spelled = root.join("src/./pkg/../pkg/main.vl");
        let plain = root.join("src/pkg/main.vl");
        assert!(!plain.exists(), "the pin needs a path not on disk");
        assert!(same_file(&spelled, &plain));
        assert!(!same_file(&plain, &root.join("src/pkg/other.vl")));
    }

    #[test]
    fn is_within_agrees_across_spellings_of_a_path_not_on_disk() {
        let root = std::env::temp_dir().join(format!("vilan-lsp-within-{}", std::process::id()));
        let directory = root.join("pkg/./src");
        let file = root.join("pkg/src/deep/main.vl");
        assert!(!file.exists(), "the pin needs a path not on disk");
        assert!(is_within(&directory, &file));
        assert!(!is_within(&root.join("pkg/other"), &file));
    }

    #[test]
    fn same_file_and_is_within_agree_on_a_path_that_is_on_disk() {
        let root = std::env::temp_dir().join(format!("vilan-lsp-ondisk-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("src")).unwrap();
        let file = root.join("src/main.vl");
        std::fs::write(&file, "fun main() {}\n").unwrap();
        assert!(same_file(&root.join("src/../src/./main.vl"), &file));
        assert!(is_within(&root.join("./src"), &file));
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A throwaway on-disk package: `files` written under a fresh temp dir,
    /// the first file analyzed as the open document. Returns the temp dir (for
    /// later edits + cleanup) and the analyzed document.
    pub(crate) fn analyze_workspace(files: &[(&str, &str)]) -> (PathBuf, Document) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("vilan_lsp_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for (relative, contents) in files {
            let path = dir.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(path, contents).unwrap();
        }
        let entry = dir.join(files[0].0);
        let text = std::fs::read_to_string(&entry).unwrap();
        let document = Document::analyze(&text, &std_root(), &entry);
        (dir, document)
    }

    // An error INSIDE an imported module publishes to that module's path, with
    // a span that is correct in THAT file's text — the vanishing-diagnostics
    // bug (it used to map through the entry's line index and disappear).
    #[test]
    fn imported_file_error_groups_to_its_path_with_its_own_span() {
        let module = "fun answer(): i32 {\n\t\"not a number\"\n}\n";
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
            ),
            ("broken.vl", module),
        ]);
        let published = document.published_diagnostics();
        let item = published
            .iter()
            .find(|item| item.message.contains("Expected i32"))
            .expect("the module's type error should be published");
        let path = item.path.as_ref().expect("attributed to a file");
        assert!(path.ends_with("broken.vl"), "{path:?}");
        // The span must be an offset into broken.vl's own text — at the string
        // literal the error is about.
        let expected = module.find("\"not a number\"").unwrap();
        assert_eq!(
            item.span.into_range().start,
            expected,
            "span should locate the literal in the MODULE's text"
        );
        assert!(!item.warning);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Entry-file errors stay on the entry (path = None), even alongside module
    // errors in the same analysis.
    #[test]
    fn entry_errors_group_to_the_entry() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::greet;\nfun main() {\n\tgreet();\n\tmissing_in_entry();\n}\n",
            ),
            ("helper.vl", "fun greet() {\n\tmissing_in_helper();\n}\n"),
        ]);
        let published = document.published_diagnostics();
        let entry_error = published
            .iter()
            .find(|item| item.message.contains("missing_in_entry"))
            .expect("the entry's error should be published");
        assert!(entry_error.path.is_none(), "entry errors carry no path");
        let helper_error = published
            .iter()
            .find(|item| item.message.contains("missing_in_helper"))
            .expect("the helper's error should be published");
        assert!(
            helper_error
                .path
                .as_ref()
                .is_some_and(|path| path.ends_with("helper.vl")),
            "{:?}",
            helper_error.path
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- E54/E58: the quickfix home, add-import, auto-import completion ------
    // (E59 adds the pkg-above-std ordering the cap truncates by — see
    // `import_origin_tier` and `auto_import_completions`'s doc.)

    // A single unambiguous candidate: one quickfix, titled with its module,
    // whose edit — applied and re-analyzed — actually resolves the name.
    #[test]
    fn quickfix_offers_and_applies_an_add_import_for_an_unambiguous_name() {
        let (dir, document) = analyze_workspace(&[
            ("main.vl", "fun main() {\n\thelp_topic();\n}\n"),
            ("topic.vl", "fun help_topic() {}\n"),
        ]);
        let program = document
            .program
            .as_ref()
            .expect("analyzed cleanly enough to have a program");
        let text = document.line_index.text();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let fixes = document.quickfixes(program, whole_file);
        assert_eq!(
            fixes.len(),
            1,
            "{:?}",
            fixes.iter().map(|f| &f.title).collect::<Vec<_>>()
        );
        assert!(fixes[0].title.contains("help_topic"), "{}", fixes[0].title);
        assert!(fixes[0].title.contains("pkg::topic"), "{}", fixes[0].title);
        assert_eq!(fixes[0].replacement, "import pkg::topic::help_topic;\n");
        // Applied: splice the edit in and re-analyze — the name now resolves.
        let mut applied = text.to_string();
        applied.replace_range(fixes[0].span.into_range(), &fixes[0].replacement);
        let entry = dir.join("main.vl");
        std::fs::write(&entry, &applied).unwrap();
        let reanalyzed = Document::analyze(&applied, &std_root(), &entry);
        assert!(
            reanalyzed.diagnostics.is_empty(),
            "applying the fix should leave the file clean: {:#?}",
            reanalyzed.diagnostics
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The ratified first target (E54): element syntax with no `view` in
    // scope. `<div/>` desugars to an unresolved `view` accessor, which
    // already carries the "element syntax lowers to std::ui::view" note
    // (element-syntax S4) — the quickfix comes from the SAME general
    // unresolved-name path as any other name, reaching `view` in real std
    // via `import_candidates`' disk scan, not from the note's text.
    #[test]
    fn quickfix_offers_the_add_import_fix_for_an_unresolved_element_view() {
        let (dir, document) =
            analyze_workspace(&[("main.vl", "fun main() {\n\tlet _x = <div/>;\n}\n")]);
        let program = document.program.as_ref().unwrap();
        let text = document.line_index.text();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let fixes = document.quickfixes(program, whole_file);
        let view_fixes: Vec<_> = fixes
            .iter()
            .filter(|fix| fix.title.contains("`view`"))
            .collect();
        assert_eq!(
            view_fixes.len(),
            1,
            "expected exactly one unambiguous `view` fix: {:?}",
            fixes.iter().map(|f| &f.title).collect::<Vec<_>>()
        );
        assert!(
            view_fixes[0].title.contains("std::ui"),
            "{}",
            view_fixes[0].title
        );
        assert_eq!(view_fixes[0].replacement, "import std::ui::view;\n");
        // Applied and re-analyzed: the element head resolves.
        let mut applied = text.to_string();
        applied.replace_range(view_fixes[0].span.into_range(), &view_fixes[0].replacement);
        let entry = dir.join("main.vl");
        std::fs::write(&entry, &applied).unwrap();
        let reanalyzed = Document::analyze(&applied, &std_root(), &entry);
        assert!(
            reanalyzed
                .diagnostics
                .iter()
                .all(|error| !error.msg.contains("cannot find 'view'")),
            "applying the fix should resolve the element head: {:#?}",
            reanalyzed.diagnostics
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // An AMBIGUOUS name — two sibling modules each declare it — offers one
    // quickfix PER CANDIDATE, never a guess.
    #[test]
    fn quickfix_offers_one_action_per_candidate_for_an_ambiguous_name() {
        let (dir, document) = analyze_workspace(&[
            ("main.vl", "fun main() {\n\tshared();\n}\n"),
            ("alpha.vl", "fun shared() {}\n"),
            ("beta.vl", "fun shared() {}\n"),
        ]);
        let program = document.program.as_ref().unwrap();
        let text = document.line_index.text();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let mut fixes = document.quickfixes(program, whole_file);
        fixes.sort_by(|a, b| a.title.cmp(&b.title));
        assert_eq!(
            fixes.len(),
            2,
            "{:?}",
            fixes.iter().map(|f| &f.title).collect::<Vec<_>>()
        );
        assert!(fixes[0].title.contains("pkg::alpha"), "{}", fixes[0].title);
        assert!(fixes[1].title.contains("pkg::beta"), "{}", fixes[1].title);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // "Add all missing imports" fixes every UNAMBIGUOUS name and skips the
    // ambiguous one outright — never guessing between `alpha` and `beta`.
    #[test]
    fn add_all_missing_imports_skips_an_ambiguous_name() {
        let (dir, document) = analyze_workspace(&[
            ("main.vl", "fun main() {\n\thelp_topic();\n\tshared();\n}\n"),
            ("topic.vl", "fun help_topic() {}\n"),
            ("alpha.vl", "fun shared() {}\n"),
            ("beta.vl", "fun shared() {}\n"),
        ]);
        let program = document.program.as_ref().unwrap();
        let (_span, new_text) = document
            .add_all_missing_imports_edit(program)
            .expect("the unambiguous fix alone is still something to add");
        assert!(
            new_text.contains("import pkg::topic::help_topic;"),
            "{new_text}"
        );
        // `shared()` (the call) is untouched original text — what must be
        // ABSENT is an IMPORT of it, from either candidate module.
        assert!(
            !new_text.contains("import pkg::alpha::shared")
                && !new_text.contains("import pkg::beta::shared"),
            "an ambiguous name must never be guessed: {new_text}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E58c: the field-rename quickfix rewrites exactly the diagnostic's span
    // (the field name) with the closest-name suggestion the analyzer noted.
    #[test]
    fn quickfix_rewrites_a_misspelled_initializer_field_to_the_closest_name() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "struct Config {\n\tentries: i32,\n}\n\nfun main() {\n\tlet _ = Config { entires = 5 };\n}\n",
        )]);
        let program = document.program.as_ref().unwrap();
        let text = document.line_index.text();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let fixes = document.quickfixes(program, whole_file);
        assert_eq!(
            fixes.len(),
            1,
            "{:?}",
            fixes.iter().map(|f| &f.title).collect::<Vec<_>>()
        );
        assert_eq!(fixes[0].title, "Change to `entries`");
        assert_eq!(fixes[0].replacement, "entries");
        let expected_start = text.find("entires").unwrap();
        assert_eq!(
            fixes[0].span,
            Span {
                start: expected_start,
                end: expected_start + "entires".len(),
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E61/S2 (editing-dx.md §17.4): the missing-terminator diagnostic's own
    // gap span becomes an insertion point, exercised directly against
    // `quickfixes` (main.rs's end-to-end tests cover the handler wiring).
    #[test]
    fn quickfix_offers_an_insert_semicolon_fix_at_the_gap() {
        let (dir, document) =
            analyze_workspace(&[("main.vl", "fun main() {\n\tlet x: i32 = 1\n\tx;\n}\n")]);
        let program = document.program.as_ref().unwrap();
        let text = document.line_index.text();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let fixes = document.quickfixes(program, whole_file);
        assert_eq!(
            fixes.len(),
            1,
            "{:?}",
            fixes.iter().map(|f| &f.title).collect::<Vec<_>>()
        );
        assert_eq!(fixes[0].title, "Insert `;`");
        assert_eq!(fixes[0].replacement, ";");
        let insertion = text.find(" 1\n").map(|p| p + 2).unwrap(); // right after `1`
        assert_eq!(
            fixes[0].span,
            Span {
                start: insertion,
                end: insertion,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E61/S3-residual (editing-dx.md §17.4), the CLOSURE shape: regime 1'
    // fires the same way for a closure whose expected return type is known
    // (S3-ii, `check_return_position` reached through the closure's
    // annotation route) — the `;`-locating scan reaches it through
    // `program.closures`, not `program.functions`, proving that branch is
    // not dead code.
    #[test]
    fn quickfix_removes_a_discarding_semicolon_inside_a_closure() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "fun main() {\n\tlet scale: |i32| i32 = |value| { value * 2; };\n}\n",
        )]);
        let program = document.program.as_ref().unwrap();
        let text = document.line_index.text();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let fixes = document.quickfixes(program, whole_file);
        let remove_fixes: Vec<_> = fixes
            .iter()
            .filter(|fix| fix.title == "Remove `;`")
            .collect();
        assert_eq!(
            remove_fixes.len(),
            1,
            "{:?}",
            fixes.iter().map(|f| &f.title).collect::<Vec<_>>()
        );
        assert_eq!(remove_fixes[0].replacement, "");
        // The `;` right before the closure's own closing brace — not the
        // outer `let`'s statement-terminating `;` two characters later.
        let semicolon = text.find("2; }").map(|p| p + 1).unwrap();
        assert_eq!(
            remove_fixes[0].span,
            Span {
                start: semicolon,
                end: semicolon + 1,
            }
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E54c: an unimported name in an ALREADY-LOADED module (loaded because a
    // sibling name from it is imported) is offered at a bare scope position,
    // labeled with its module and carrying the brace-set-extension edit.
    //
    // Natural, unprefixed names on purpose (E59): real std is analyzed
    // alongside this tiny fixture (the LSP always loads it), and its OWN
    // loaded prelude modules contribute plenty of unimported candidates of
    // their own — capitalized type/trait names (`Add`, `BitAnd`, …) that, in
    // bare alphabetical order, sort ahead of an ordinary lowercase
    // identifier like `farewell`. Before E59's `pkg`-above-`std` tiering,
    // this test needed its names `AAA_`-prefixed to survive
    // `AUTO_IMPORT_COMPLETION_CAP` at all; the tiering makes that
    // unnecessary — a `pkg` candidate now outranks every `std` one
    // regardless of its label, so the plain name proves the natural case.
    #[test]
    fn auto_import_completion_offers_a_labeled_edit_carrying_sibling_from_an_already_loaded_module()
    {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::greet;\n\nfun main() {\n\tgreet();\n\t\n}\n",
            ),
            ("helper.vl", "fun greet() {}\n\nfun farewell() {}\n"),
        ]);
        let marker = "greet();\n\t";
        let text = document.line_index.text();
        let offset = text.find(marker).unwrap() + marker.len();
        let candidates = document.completion(offset);
        let farewell = candidates
            .iter()
            .find(|candidate| candidate.label == "farewell")
            .expect("an unimported sibling in an already-loaded module is offered");
        let auto_import = farewell
            .needs_import
            .as_ref()
            .expect("labeled with the import it needs");
        assert_eq!(
            auto_import.module_path,
            vec!["pkg".to_string(), "helper".to_string()]
        );
        // `helper` is already imported (bare `greet`): the edit EXTENDS it
        // into a two-member set rather than inserting a new line.
        assert_eq!(auto_import.edit_replacement, "{ farewell, greet }");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A name already offered from SCOPE is never duplicated as an auto-import
    // candidate — the in-scope one is the only match on the menu. (Same
    // natural-name reasoning as above — E59.)
    #[test]
    fn a_name_already_in_scope_is_not_offered_as_an_auto_import_candidate() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::greet;\n\nfun farewell() {}\n\n\
                 fun main() {\n\tgreet();\n\t\n}\n",
            ),
            ("helper.vl", "fun greet() {}\n\nfun farewell() {}\n"),
        ]);
        let marker = "greet();\n\t";
        let text = document.line_index.text();
        let offset = text.find(marker).unwrap() + marker.len();
        let candidates = document.completion(offset);
        let farewells: Vec<_> = candidates
            .iter()
            .filter(|candidate| candidate.label == "farewell")
            .collect();
        assert_eq!(
            farewells.len(),
            1,
            "the in-scope declaration only, no auto-import duplicate"
        );
        assert!(farewells[0].needs_import.is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E54c's cap: a module with many unimported siblings doesn't flood the
    // popup — the count never exceeds `AUTO_IMPORT_COMPLETION_CAP`, though at
    // least one still comes through. Natural, unprefixed names (E59, see
    // above) so this fixture's own 30 unimported candidates are what compete
    // for the cap's 20 slots, not std's much larger loaded surface — proven
    // by `origin_tier`, not by out-sorting std alphabetically.
    #[test]
    fn auto_import_completions_are_capped() {
        let many_functions: String = (0..30)
            .map(|index| format!("fun sibling{index:02}() {{}}\n"))
            .collect();
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::sibling00;\n\nfun main() {\n\tsibling00();\n\t\n}\n",
            ),
            ("helper.vl", &many_functions),
        ]);
        let marker = "sibling00();\n\t";
        let text = document.line_index.text();
        let offset = text.find(marker).unwrap() + marker.len();
        let candidates = document.completion(offset);
        let auto_import_labels: Vec<&str> = candidates
            .iter()
            .filter(|candidate| candidate.needs_import.is_some())
            .map(|candidate| candidate.label.as_str())
            .collect();
        assert!(
            auto_import_labels.len() <= AUTO_IMPORT_COMPLETION_CAP,
            "expected the cap to hold, got {}: {auto_import_labels:?}",
            auto_import_labels.len()
        );
        assert!(
            auto_import_labels
                .iter()
                .all(|label| label.starts_with("sibling")),
            "expected the fixture's own 29 unimported siblings to fill the \
             cap, not std's: {auto_import_labels:?}"
        );
        assert_eq!(
            auto_import_labels.len(),
            AUTO_IMPORT_COMPLETION_CAP,
            "29 candidates over a cap of {AUTO_IMPORT_COMPLETION_CAP} should saturate it: {auto_import_labels:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E83: ONE whole-buffer parse per completion request, however many
    // auto-import candidates the request shapes. `insert_import`'s
    // string-input form re-parses the buffer per call, and calling it once
    // per surviving candidate (up to `AUTO_IMPORT_COMPLETION_CAP`) is what
    // made a bare scope position cost ~20 member completions
    // (playground-completion.md §9) — in the language server and the
    // playground alike, since both drive this engine. The shared
    // `formatter::ParsedSource` pays the parse once; this pin holds the
    // count, not the time.
    #[test]
    fn a_scope_completion_with_many_auto_import_candidates_parses_the_buffer_once() {
        let many_functions: String = (0..30)
            .map(|index| format!("fun sibling{index:02}() {{}}\n"))
            .collect();
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::sibling00;\n\nfun main() {\n\tsibling00();\n\t\n}\n",
            ),
            ("helper.vl", &many_functions),
        ]);
        let marker = "sibling00();\n\t";
        let text = document.line_index.text();
        let offset = text.find(marker).unwrap() + marker.len();
        let parses_before = vilan_core::formatter::buffer_parse_count();
        let candidates = document.completion(offset);
        let parses = vilan_core::formatter::buffer_parse_count() - parses_before;
        let auto_imports = candidates
            .iter()
            .filter(|candidate| candidate.needs_import.is_some())
            .count();
        assert_eq!(
            auto_imports, AUTO_IMPORT_COMPLETION_CAP,
            "the scenario must shape a full cap of candidates for the parse count to mean anything"
        );
        assert_eq!(
            parses, 1,
            "a completion request parses the buffer once, not once per auto-import candidate"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E83's other per-candidate cost: a non-entry declaration's `///` doc is
    // sliced out of its module's text, and a bare scope position resolves
    // docs for every Function candidate in scope — three names imported from
    // `std::math` used to be three reads (three clones) of math.vl in ONE
    // request. The per-query cache on `Analysis` (`source_texts`) makes it
    // one read per module per request. The fixture imports from exactly one
    // std module and declares its other functions locally (entry-file docs
    // slice the analyzed text, no read), so the expected count is exactly 1;
    // if this goes red after a std reshuffle, first check what the entry
    // scope now resolves docs from.
    #[test]
    fn one_completion_request_reads_a_docs_module_text_once() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::math::{ max, min, minmax };\n\n\
             fun main() {\n\tlet low = min(1, 2);\n\t\n}\n",
        )]);
        let marker = "min(1, 2);\n\t";
        let text = document.line_index.text();
        let offset = text.find(marker).unwrap() + marker.len();
        let reads_before = vilan_core::util::source_read_count();
        let candidates = document.completion(offset);
        let reads = vilan_core::util::source_read_count() - reads_before;
        for name in ["max", "min", "minmax"] {
            assert!(
                candidates.iter().any(|candidate| candidate.label == name),
                "{name} must be offered for the read count to mean anything"
            );
        }
        assert_eq!(
            reads, 1,
            "three same-module candidates resolve docs from one read of math.vl, not three"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E59: the pin proving the filing's own case — a small real file's own
    // pkg name (alphabetically LAST among its siblings, deliberately named
    // to lose against std's capitalized prelude in bare string order) still
    // appears, ahead of every std candidate, because `pkg` outranks `std` by
    // tier rather than by label. Plant-proof: reverting `import_origin_tier`
    // to return the same tier for every root turns this red (`zzz_local` is
    // squeezed out by std's >20 capitalized prelude names before the sort
    // ever reaches it) — restored after confirming it.
    #[test]
    fn a_pkg_name_that_loses_alphabetically_still_outranks_stds_surface() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::anchor;\n\nfun main() {\n\tanchor();\n\t\n}\n",
            ),
            ("helper.vl", "fun anchor() {}\n\nfun zzz_local() {}\n"),
        ]);
        let marker = "anchor();\n\t";
        let text = document.line_index.text();
        let offset = text.find(marker).unwrap() + marker.len();
        let candidates = document.completion(offset);
        let local = candidates
            .iter()
            .find(|candidate| candidate.label == "zzz_local")
            .expect(
                "a pkg name must survive the cap even when it sorts after \
                 std's entire loaded surface alphabetically",
            );
        assert_eq!(
            local
                .needs_import
                .as_ref()
                .expect("labeled with the import it needs")
                .origin_tier,
            0,
            "pkg is tier 0"
        );
        let std_present = candidates.iter().any(|candidate| {
            candidate.needs_import.as_ref().is_some_and(|auto_import| {
                auto_import.module_path.first().map(String::as_str) == Some("std")
            })
        });
        assert!(
            std_present,
            "the pkg tier (2 names) doesn't come close to filling the cap, \
             so std candidates still appear behind them"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A published set's messages — `PublishedDiagnostic` is not `Debug`, and
    /// the message is what an assertion failure needs to read.
    fn messages(published: &[PublishedDiagnostic]) -> Vec<&str> {
        published.iter().map(|item| item.message.as_str()).collect()
    }

    // An initialization cycle (b33-emission-order.md §3) reaches the editor
    // like any other analyzer diagnostic: `check_cycles` runs inside
    // `analyze_source`, so it is in `program.diagnostics` by the time the
    // document is built. It carries the C3 note, and its `diagnostic_sources`
    // entry publishes a cross-module cycle in the module holding the read that
    // closes it, spanned in THAT file's text.
    #[test]
    fn an_initialization_cycle_publishes_to_the_file_that_closes_it() {
        let alpha = "import pkg::zeta::{ Z };\nlet A: i32 = Z + 1;\n";
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::alpha::{ A };\nimport pkg::zeta::{ Z };\n\
                 fun main() { print(A); print(Z); }\n",
            ),
            ("alpha.vl", alpha),
            (
                "zeta.vl",
                "import pkg::alpha::{ A };\nlet Z: i32 = A + 2;\n",
            ),
        ]);
        let published = document.published_diagnostics();
        assert_eq!(
            published.len(),
            1,
            "one diagnostic per cycle: {:?}",
            messages(&published)
        );
        let item = &published[0];
        assert!(
            item.message
                .contains("`A` and `Z` form an initialization cycle")
                && item.message.contains("via `A` → `Z` → `A`"),
            "the cycle and its chain are published: {}",
            item.message
        );
        let path = item.path.as_ref().expect("attributed to the module file");
        assert!(path.ends_with("alpha.vl"), "{path:?}");
        let read = alpha.find("Z + 1").expect("the read is in alpha.vl");
        assert_eq!(
            item.span.into_range(),
            read..read + 1,
            "spanned at the read, in alpha.vl's own text"
        );
        // The C3 note survives the module-attributed branch too (backlog E17):
        // it used to be dropped there, so every module-attributed diagnostic
        // reached the editor without its second location. `Z` is declared in
        // `zeta.vl`, so the note carries that file — the note's own source, not
        // the diagnostic's.
        let (note_span, note_message, note_path) = item
            .note
            .as_ref()
            .expect("the C3 declaration note survives a module-attributed publish");
        assert!(
            note_message.contains("`Z` is declared here"),
            "{note_message}"
        );
        let note_path = note_path.as_ref().expect("the note names its own file");
        assert!(note_path.ends_with("zeta.vl"), "{note_path:?}");
        let declaration = std::fs::read_to_string(note_path).expect("read zeta.vl");
        assert_eq!(
            note_span.into_range().start,
            declaration
                .find("let Z: i32 = A + 2")
                .expect("Z's declaration"),
            "the note is spanned in zeta.vl's own text"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── B112: a post-`build()` check publishes on the file its span indexes ──
    //
    // The editor half of the bug. R10 runs after `build()`, where the analyzer's
    // "current file" is the entry, so a written `List<Guard>` in an imported
    // module published against THIS document at the module's offsets — a
    // squiggle over unrelated text in the file the user has open, and nothing at
    // all in the file that has the mistake.
    #[test]
    fn a_container_resource_in_a_module_publishes_on_the_module() {
        let module = "import std::print;\nimport std::drop::Drop;\n\
                      resource struct Guard { label: str }\n\
                      impl Guard with Drop { fun drop(&mut self) { print(self.label); } }\n\
                      fun keep() {\n\tmut arr: List<Guard> = [];\n}\n";
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::store::keep;\nfun main() { keep(); }\n",
            ),
            ("store.vl", module),
        ]);
        let published = document.published_diagnostics();
        let item = published
            .iter()
            .find(|item| item.message.contains("cannot hold the resource `Guard`"))
            .unwrap_or_else(|| panic!("R10 should publish: {:?}", messages(&published)));
        let path = item.path.as_ref().expect("attributed to a file");
        assert!(path.ends_with("store.vl"), "{path:?}");
        // And the span is an offset into store.vl's own text — the half that
        // makes the path worth having.
        let annotation = module
            .find("List<Guard>")
            .expect("the annotation is in store.vl");
        assert_eq!(
            item.span.into_range(),
            annotation..annotation + "List<Guard>".len(),
            "spanned at the annotation, in store.vl's own text"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── E82: a derive refusal on a module's struct publishes on the module ──
    //
    // The generated-code twin of the B112 shape. `[derive(PartialEq)]` on a
    // struct whose field type provides no `PartialEq` refuses inside the
    // GENERATED `eq` (its field compare is an `==` the post-fixpoint
    // binary-operator pass checks). That pass pushed without attributing, so
    // the refusal kept the generated template's span while claiming the entry
    // — a squiggle over unrelated entry text (a comment line, in the live
    // repro), and nothing at all in the file that has the mistake. It now
    // re-anchors at the attribute that generated the code (standard A2) and
    // publishes in the deriving module.
    #[test]
    fn a_derive_refusal_in_a_module_publishes_on_the_attribute_in_the_module() {
        let module =
            "[derive(PartialEq)]\nstruct Widget { item: Opaque }\n\nstruct Opaque { x: i32 }\n";
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::page::{ Widget, Opaque };\n\
                 fun main() {\n\tlet w = Widget { item = Opaque { x = 1 } };\n\tprint(w.item.x);\n}\n",
            ),
            ("page.vl", module),
        ]);
        let published = document.published_diagnostics();
        let item = published
            .iter()
            .find(|item| {
                item.message
                    .contains("does not implement the `PartialEq` operator")
            })
            .unwrap_or_else(|| panic!("the refusal should publish: {:?}", messages(&published)));
        assert!(
            item.message
                .contains("in code generated by this attribute:"),
            "provenance said in the message: {}",
            item.message
        );
        let path = item.path.as_ref().expect("attributed to a file");
        assert!(path.ends_with("page.vl"), "{path:?}");
        // The span is the attribute NAME's, in page.vl's own text — the
        // location acting on the refusal means editing.
        let attribute = module
            .find("PartialEq")
            .expect("the derive name is in page.vl");
        assert_eq!(
            item.span.into_range(),
            attribute..attribute + "PartialEq".len(),
            "spanned at the derive name, in page.vl's own text"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The same-file half: a cycle in the open document publishes on the entry
    // (no path), so the squiggle lands under the read the user is looking at.
    #[test]
    fn an_entry_file_initialization_cycle_publishes_on_the_entry() {
        let entry = "import std::print;\nlet A: i32 = B + 1;\nlet B: i32 = A + 2;\n\
                     fun main() { print(A); print(B); }\n";
        let (dir, document) = analyze_workspace(&[("main.vl", entry)]);
        let published = document.published_diagnostics();
        assert_eq!(
            published.len(),
            1,
            "one diagnostic per cycle: {:?}",
            messages(&published)
        );
        assert!(
            published[0].path.is_none(),
            "entry diagnostics carry no path"
        );
        let read = entry.find("B + 1").expect("the read is in the entry");
        assert_eq!(published[0].span.into_range(), read..read + 1);
        // The entry-attributed branch keeps the C3 note, so the editor can show
        // the other member's declaration as related information.
        let (note_span, note_message, note_path) = published[0]
            .note
            .as_ref()
            .expect("the C3 declaration note survives publishing");
        assert!(
            note_message.contains("`B` is declared here"),
            "{note_message}"
        );
        assert!(note_path.is_none(), "same file: {note_path:?}");
        let declaration = entry.find("let B: i32 = A + 2").expect("B's declaration");
        assert_eq!(note_span.into_range().start, declaration);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The staleness half: fixing the imported file on disk and re-analyzing the
    // SAME entry clears the module's diagnostics — what `reanalyze_dependents`
    // relies on (a dependent's re-analysis reads the dependency fresh).
    #[test]
    fn reanalysis_after_fixing_the_import_clears_its_diagnostics() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
            ),
            ("broken.vl", "fun answer(): i32 {\n\t\"not a number\"\n}\n"),
        ]);
        assert!(
            document
                .published_diagnostics()
                .iter()
                .any(|item| item.message.contains("Expected i32")),
            "the broken dependency should report first"
        );
        // Fix the module on disk; re-analyze the unchanged entry.
        std::fs::write(dir.join("broken.vl"), "fun answer(): i32 {\n\t42\n}\n").unwrap();
        let entry = dir.join("main.vl");
        let text = std::fs::read_to_string(&entry).unwrap();
        let reanalyzed = Document::analyze(&text, &std_root(), &entry);
        assert!(
            reanalyzed.published_diagnostics().is_empty(),
            "fixed dependency should publish clean: {:?}",
            reanalyzed
                .published_diagnostics()
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // `[must_use]` drops surface as warnings on the entry.
    #[test]
    fn must_use_drops_publish_as_warnings() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "[must_use]\nfun important(): i32 { 7 }\nfun main() {\n\timportant();\n}\n",
        )]);
        let published = document.published_diagnostics();
        let warning = published
            .iter()
            .find(|item| item.warning)
            .expect("the dropped result should warn");
        assert!(warning.path.is_none());
        assert!(
            warning.message.contains("must_use")
                || warning.message.contains("result")
                || warning.message.contains("unused"),
            "{}",
            warning.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A `[deprecated]` use surfaces as a WARNING diagnostic (deprecation.md
    // §1's ledger note) — the third producer on the same non-fatal channel
    // `[must_use]` rides, through the same publish path; nothing new at this
    // layer, and this pin holds it to that.
    #[test]
    fn deprecated_uses_publish_as_warnings() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "[deprecated(\"use two()\")]\nfun one(): i32 { 7 }\nfun two(): i32 { 7 }\nfun main() {\n\tlet _ = one();\n}\n",
        )]);
        let published = document.published_diagnostics();
        let warning = published
            .iter()
            .find(|item| item.warning)
            .expect("the deprecated use should warn");
        assert!(warning.path.is_none());
        assert!(
            warning.message.contains("`one` is deprecated; use two()"),
            "{}",
            warning.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── git dependencies in the editor (proposal/distribution.md §5) ──

    // The editor's half of the git-dependency policy: **it never fetches**.
    // The repository below is real, local, correct and one `clone` away — and
    // the language server still must not take it, because analysis runs on
    // every keystroke and must not reach the network (nor write into the
    // user's cache) behind their back. The dependency simply stays unresolved
    // until a `vilan build` fetches it, which is the same degradation this
    // call has always had for an unresolvable manifest.
    #[test]
    fn the_editor_never_fetches_a_git_dependency() {
        let root = std::env::temp_dir().join(format!("vilan_lsp_gitdep_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let repository = root.join("shapes");
        std::fs::create_dir_all(repository.join("src")).unwrap();
        std::fs::write(
            repository.join("vilan.toml"),
            "[library]\nname = \"shapes\"\n",
        )
        .unwrap();
        std::fs::write(
            repository.join("src/lib.vl"),
            "fun greeting(): str { \"hi\" }\n",
        )
        .unwrap();
        let git = |arguments: &[&str]| {
            let output = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=vilan test",
                    "-c",
                    "user.email=test@vilan.invalid",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(arguments)
                .current_dir(&repository)
                .output()
                .expect("git must be installed for this pin");
            assert!(output.status.success(), "git {arguments:?} failed");
        };
        git(&["init", "--quiet", "."]);
        git(&["add", "-A"]);
        git(&["commit", "--quiet", "-m", "fixture"]);
        git(&["tag", "v1.0.0"]);
        let url = format!("file://{}", repository.display());

        let (dir, document) = analyze_workspace(&[
            (
                "src/main.vl",
                "import shapes::greeting;\n\nfun main() {\n\tgreeting();\n}\n",
            ),
            (
                "vilan.toml",
                &format!(
                    "[package]\nname = \"app\"\n\n[package.dependencies]\n\
                     shapes = {{ git = \"{url}\", tag = \"v1.0.0\" }}\n"
                ),
            ),
        ]);
        let published = document.published_diagnostics();
        assert!(
            published
                .iter()
                .any(|item| item.message.contains("cannot find module 'shapes'") && !item.warning),
            "an unfetched git dependency stays unresolved: {:?}",
            published
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
        // ...and nothing was written into the real cache: the entry for this
        // (unique, temp-directory) URL must not exist.
        let source = vilan_core::git_dep::GitSource {
            url,
            reference: vilan_core::git_dep::GitRef::Tag("v1.0.0".to_string()),
        };
        let entry =
            vilan_core::git_dep::entry_path(&vilan_embedded_std::default_git_dep_root(), &source);
        assert!(
            !entry.exists(),
            "the editor must not populate the git cache: {}",
            entry.display()
        );
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ── the `vilan.toml` diagnostic channel (F5 S5; distribution.md §7's S4
    // residual: manifest failures were swallowed) ──
    //
    // The whole channel is ONE diagnostic per analysis, addressed to the
    // manifest, with the severity the failure deserves.

    /// The manifest diagnostic in `document`'s published set, if any: the one
    /// item attributed to a `vilan.toml`.
    fn manifest_diagnostic(document: &Document) -> Option<PublishedDiagnostic> {
        document.published_diagnostics().into_iter().find(|item| {
            item.path
                .as_ref()
                .is_some_and(|path| path.ends_with("vilan.toml"))
        })
    }

    #[test]
    fn a_manifest_that_does_not_parse_publishes_on_the_manifest() {
        let (dir, document) = analyze_workspace(&[
            ("src/main.vl", "fun main() {}\n"),
            // An unterminated table header: TOML, not vilan, so nothing in the
            // pipeline would ever have said a word about it.
            ("vilan.toml", "[package\nname = \"app\"\n"),
        ]);
        let item = manifest_diagnostic(&document).expect("the parse failure is published");
        assert!(!item.warning, "a manifest that does not parse is an error");
        assert!(item.message.contains("invalid"), "{}", item.message);
        assert!(item.message.contains("vilan.toml"), "{}", item.message);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_reserved_package_name_publishes_on_the_manifest() {
        // L12 (std-shape.md §4): a dependency key claiming a reserved import
        // root — before the refusal, this dependency silently shadowed the
        // whole standard library — reaches the editor through the same
        // channel every manifest problem rides.
        let (dir, document) = analyze_workspace(&[
            ("src/main.vl", "fun main() {}\n"),
            (
                "vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 std = { path = \"../std\" }\n",
            ),
        ]);
        let item = manifest_diagnostic(&document).expect("the reserved name is published");
        assert!(!item.warning, "a reserved package name is an error");
        assert!(
            item.message.contains("`std` is a reserved package name"),
            "{}",
            item.message
        );
        assert!(
            item.message.contains("rename the dependency"),
            "{}",
            item.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unresolvable_dependency_publishes_the_reason_on_the_manifest() {
        // The wall-of-unresolved-imports case: the manifest parses, but its
        // dependency does not resolve, so the workspace is empty and every
        // `import shapes::…` fails. Before this channel, the *reason* was
        // dropped on the floor.
        let (dir, document) = analyze_workspace(&[
            ("src/main.vl", "import shapes::area;\n\nfun main() {}\n"),
            (
                "vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { path = \"../nowhere\" }\n",
            ),
        ]);
        let item = manifest_diagnostic(&document).expect("the resolution failure is published");
        assert!(!item.warning, "an unresolvable dependency is an error");
        assert!(
            item.message.contains("dependency `shapes`"),
            "{}",
            item.message
        );
        // ...and the import diagnostic it explains is still there.
        assert!(
            document
                .published_diagnostics()
                .iter()
                .any(|other| other.message.contains("shapes") && other.path.is_none()),
            "the unresolved import stays"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cold_git_dependency_steers_to_vilan_build_as_a_warning() {
        // The steer S4 handed to S5. The URL is unreachable by construction and
        // the editor's policy never fetches, so this entry can never be in the
        // real cache — the diagnostic is deterministic. It is a WARNING: the
        // manifest is correct and one build fixes it.
        let (dir, document) = analyze_workspace(&[
            ("src/main.vl", "import shapes::area;\n\nfun main() {}\n"),
            (
                "vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { git = \"https://example.invalid/org/shapes\", tag = \"v9.9.9\" }\n",
            ),
        ]);
        let item = manifest_diagnostic(&document).expect("the cold cache is published");
        assert!(item.warning, "a cache miss is a steer, not a fault");
        assert!(
            item.message.contains("git dependency `shapes`"),
            "{}",
            item.message
        );
        assert!(
            item.message.contains("not in the local cache"),
            "{}",
            item.message
        );
        assert!(item.message.contains("vilan build"), "{}", item.message);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_healthy_project_publishes_no_manifest_diagnostic() {
        // The silence half: a project that resolves says nothing about its
        // manifest — the channel must not become ambient noise.
        let (dir, document) = analyze_workspace(&[
            ("app/src/main.vl", "import shapes::area;\n\nfun main() {}\n"),
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { path = \"../shapes\" }\n",
            ),
            ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
            ("shapes/src/lib.vl", "fun area(): i32 { 1 }\n"),
        ]);
        assert!(
            manifest_diagnostic(&document).is_none(),
            "a healthy project publishes nothing about its manifest: {:?}",
            document
                .published_diagnostics()
                .iter()
                .map(|item| (item.path.clone(), item.message.clone()))
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_inherited_dependency_resolves_in_the_editor_too() {
        // The other half of Task 1 in the editor: a member that opts in to the
        // workspace root's declaration type-checks its import — the CLI and the
        // LSP resolve through the same `resolve_workspace`.
        let (dir, document) = analyze_workspace(&[
            (
                "app/src/main.vl",
                "import shapes::area;\n\nfun main() {\n\tlet size = area();\n}\n",
            ),
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { project = true }\n",
            ),
            (
                "vilan.toml",
                "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                 shapes = { path = \"shapes\" }\n",
            ),
            ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
            ("shapes/src/lib.vl", "fun area(): i32 { 1 }\n"),
        ]);
        assert!(
            document.published_diagnostics().is_empty(),
            "{:?}",
            document
                .published_diagnostics()
                .iter()
                .map(|item| item.message.clone())
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_project_declaration_publishes_on_the_projects_manifest() {
        // distribution.md §7's S5 residual: the member's `vilan.toml` is
        // CORRECT — it opted in and nothing more — so squiggling it points the
        // user at a file with nothing to fix. The declaration lives in the
        // project root, and that is where the diagnostic goes.
        let (dir, document) = analyze_workspace(&[
            ("app/src/main.vl", "import shapes::area;\n\nfun main() {}\n"),
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { project = true }\n",
            ),
            (
                "vilan.toml",
                "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                 shapes = { path = \"nowhere\" }\n",
            ),
        ]);
        let item = manifest_diagnostic(&document).expect("the resolution failure is published");
        let path = item.path.clone().expect("addressed to a manifest");
        assert_eq!(
            path,
            vilan_core::util::canonical_path(&dir).join("vilan.toml"),
            "the project's manifest, not the member's"
        );
        assert!(!item.warning, "an unresolvable dependency is an error");
        // The wording carries the same fact, for the CLI and for a reader who
        // sees the message without its address.
        assert!(
            item.message.contains("is inherited from"),
            "{}",
            item.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_broken_member_declaration_still_publishes_on_the_member() {
        // The control: a member that declares its own dependency owns the
        // mistake, so the address is unchanged by the inheritance rule above.
        let (dir, document) = analyze_workspace(&[
            ("app/src/main.vl", "import shapes::area;\n\nfun main() {}\n"),
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { path = \"../nowhere\" }\n",
            ),
            ("vilan.toml", "[project]\npackages = [\"app\"]\n"),
        ]);
        let item = manifest_diagnostic(&document).expect("the resolution failure is published");
        let path = item.path.clone().expect("addressed to a manifest");
        assert_eq!(
            path,
            dir.join("app").join("vilan.toml"),
            "the member's own manifest"
        );
        assert!(
            !item.message.contains("is inherited from"),
            "{}",
            item.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_invalid_project_declaration_publishes_on_the_projects_manifest() {
        // The other broken-declaration shape: a spelling `validate` rejects
        // (both `tag` and `rev`). It fails before any path is resolved — a
        // different code path (`enclosing_project`) that must reach the same
        // address.
        let (dir, document) = analyze_workspace(&[
            ("app/src/main.vl", "import shapes::area;\n\nfun main() {}\n"),
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n[package.dependencies]\n\
                 shapes = { project = true }\n",
            ),
            (
                "vilan.toml",
                "[project]\npackages = [\"app\"]\n[project.dependencies]\n\
                 shapes = { git = \"https://example.invalid/org/shapes\", \
                 tag = \"v1\", rev = \"0123456\" }\n",
            ),
        ]);
        let item = manifest_diagnostic(&document).expect("the invalid manifest is published");
        let path = item.path.clone().expect("addressed to a manifest");
        assert_eq!(
            path,
            vilan_core::util::canonical_path(&dir).join("vilan.toml"),
            "the project's manifest, not the member's"
        );
        assert!(!item.warning, "an invalid manifest is an error");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── a `[library]`-rooted document resolves its own package (the S5 report's
    // noted pre-existing gap: a file whose nearest manifest is a `[library]`
    // got NO project at all, so its own `pkg::` modules read as unresolved) ──

    #[test]
    fn a_library_module_resolves_its_pkg_siblings() {
        // The library's own layer is the `pkg::` root, so a sibling module is a
        // module — not "cannot find". The document is a NESTED module
        // (`src/deep/lib.vl`, i.e. `pkg::deep`), which is exactly the shape the
        // no-project fallback got wrong: it rooted `pkg::` at the file's own
        // directory, so `pkg::util` looked for `src/deep/util.vl`.
        let (dir, document) = analyze_workspace(&[
            (
                "src/deep/lib.vl",
                "import pkg::util::triple;\n\nfun twice(n: i32): i32 { triple(n) }\n",
            ),
            ("src/util.vl", "fun triple(n: i32): i32 { n * 3 }\n"),
            ("vilan.toml", "[library]\nname = \"shapes\"\n"),
        ]);
        assert!(
            document.published_diagnostics().is_empty(),
            "{:?}",
            document
                .published_diagnostics()
                .iter()
                .map(|item| item.message.clone())
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_module_resolves_its_own_dependency() {
        // `[library.dependencies]` are the edges of anything compiled inside
        // the library — `resolve_workspace` used to return an empty workspace
        // for any manifest without a `[package]`.
        let (dir, document) = analyze_workspace(&[
            (
                "lib2/src/lib.vl",
                "import shapes::area;\n\nfun doubled(): i32 { area() * 2 }\n",
            ),
            (
                "lib2/vilan.toml",
                "[library]\nname = \"lib2\"\n[library.dependencies]\n\
                 shapes = { path = \"../shapes\" }\n",
            ),
            ("shapes/vilan.toml", "[library]\nname = \"shapes\"\n"),
            ("shapes/src/lib.vl", "fun area(): i32 { 7 }\n"),
        ]);
        assert!(
            document.published_diagnostics().is_empty(),
            "{:?}",
            document
                .published_diagnostics()
                .iter()
                .map(|item| item.message.clone())
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_library_modules_manifest_failures_reach_the_editor_too() {
        // The `vilan.toml` channel now covers libraries: a library whose own
        // dependency does not resolve says so, on its own manifest.
        let (dir, document) = analyze_workspace(&[
            (
                "src/lib.vl",
                "import shapes::area;\n\nfun a(): i32 { area() }\n",
            ),
            (
                "vilan.toml",
                "[library]\nname = \"lib2\"\n[library.dependencies]\n\
                 shapes = { path = \"../nowhere\" }\n",
            ),
        ]);
        let item = manifest_diagnostic(&document).expect("the resolution failure is published");
        assert!(!item.warning, "an unresolvable dependency is an error");
        assert!(
            item.message.contains("dependency `shapes`"),
            "{}",
            item.message
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_layered_library_module_is_rooted_at_its_own_layer() {
        // The boundary, pinned as current truth: `pkg::` for the entry's OWN
        // package searches one root, so a file under a platform layer is rooted
        // THERE — same-layer siblings resolve, base-layer modules do not.
        // Reaching both needs the entry package to carry a layered spec (the
        // platform-model era's deferral); rooting at the file's own layer is
        // the subset that never resolves less than the fallback it replaces.
        let (dir, document) = analyze_workspace(&[
            (
                "src/browser/widget.vl",
                "import pkg::paint::tint;\nimport pkg::shared::base_value;\n\n\
                 fun render(): i32 { tint() + base_value() }\n",
            ),
            ("src/browser/paint.vl", "fun tint(): i32 { 2 }\n"),
            ("src/shared.vl", "fun base_value(): i32 { 1 }\n"),
            (
                "vilan.toml",
                "[library]\nname = \"widgets\"\n[library.layer.browser]\n\
                 platform = [\"@browser\"]\n",
            ),
        ]);
        let messages: Vec<String> = document
            .published_diagnostics()
            .iter()
            .map(|item| item.message.clone())
            .collect();
        assert!(
            !messages.iter().any(|message| message.contains("tint")),
            "the same-layer sibling must resolve: {messages:?}"
        );
        assert!(
            messages
                .iter()
                .any(|message| message.contains("base_value")),
            "the base-layer module is out of reach today: {messages:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── platform coloring in the editor (proposal/platform-coloring.md, phase 2) ──

    // A browser-target package whose entry REACHES `std::fs` publishes the
    // coloring violation live: chain-rendered with module-labeled library
    // frames, anchored at the offending call in the entry.
    #[test]
    fn coloring_violation_publishes_live_on_a_browser_target() {
        let entry = "import std::fs;\n\nfun main() {\n\tlet present = fs::stat(\"marker\");\n}\n";
        let (dir, document) = analyze_workspace(&[
            ("src/main.vl", entry),
            (
                "vilan.toml",
                "[package]\nname = \"app\"\ntarget = \"browser\"\n",
            ),
        ]);
        let published = document.published_diagnostics();
        let violation = published
            .iter()
            .find(|item| {
                item.message
                    .contains("requires the `process` layer of `std`")
            })
            .unwrap_or_else(|| {
                panic!(
                    "no coloring violation published: {:?}",
                    published
                        .iter()
                        .map(|item| &item.message)
                        .collect::<Vec<_>>()
                )
            });
        assert!(violation.path.is_none(), "anchored in the entry itself");
        assert!(!violation.warning);
        assert!(
            violation.message.contains("cannot run on `browser`"),
            "{}",
            violation.message
        );
        assert!(
            violation.message.contains("main → stat (std::fs)"),
            "{}",
            violation.message
        );
        // The anchor is the deepest user-code call site: the `fs::stat` call.
        let call = entry.find("stat(").unwrap();
        let range = violation.span.into_range();
        assert!(
            range.start <= call && call < range.end,
            "span {range:?} should cover the call at {call}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The same reach under the package's declared `node` target is admissible —
    // the manifest's `target` is what drives the editor's platform.
    #[test]
    fn the_manifest_target_admits_the_same_reach_on_node() {
        let entry = "import std::fs;\n\nfun main() {\n\tlet present = fs::stat(\"marker\");\n}\n";
        let (dir, document) = analyze_workspace(&[
            ("src/main.vl", entry),
            (
                "vilan.toml",
                "[package]\nname = \"app\"\ntarget = \"node\"\n",
            ),
        ]);
        let published = document.published_diagnostics();
        assert!(
            published.is_empty(),
            "{:?}",
            published
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A manifest-less scratch file gets its platform INFERRED from its imports:
    // `std::dom` marks it a browser file, so reaching `std::fs` colors.
    #[test]
    fn an_inferred_browser_file_colors_without_a_manifest() {
        let document = Document::analyze(
            "import std::dom;\nimport std::fs;\n\nfun main() {\n\tlet present = fs::stat(\"marker\");\n}\n",
            &std_root(),
            Path::new("scratch.vl"),
        );
        let published = document.published_diagnostics();
        assert!(
            published.iter().any(|item| {
                item.message
                    .contains("requires the `process` layer of `std`")
                    && item.message.contains("cannot run on `browser`")
            }),
            "{:?}",
            published
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
    }

    // A multi-entry package (proposal/platform-coloring.md §4.2): an entry
    // file analyzes under ITS entry's target — the browser entry colors on
    // reaching the store, the node entry running the same code doesn't — and
    // a shared module (no entry, no `main`) analyzes clean, its hover still
    // knowing the color.
    #[test]
    fn multi_entry_files_analyze_under_their_entry_targets() {
        let manifest =
            "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n";
        let store = "import std::fs;\n\nfun load(): bool {\n\tfs::stat(\"state\").is_some()\n}\n";
        let reach = "import std::print;\nimport pkg::store::load;\n\nfun main() {\n\tif load() { print(\"?\") }\n}\n";
        let (dir, client) = analyze_workspace(&[
            ("src/client.vl", reach),
            ("vilan.toml", manifest),
            ("src/store.vl", store),
            ("src/server.vl", reach),
        ]);
        assert!(
            client.published_diagnostics().iter().any(|item| {
                item.message
                    .contains("requires the `process` layer of `std`")
                    && item.message.contains("cannot run on `browser`")
            }),
            "the client entry should color: {:?}",
            client
                .published_diagnostics()
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
        // The node entry, same code: admissible.
        let entry = dir.join("src/server.vl");
        let server = Document::analyze(
            &std::fs::read_to_string(&entry).unwrap(),
            &std_root(),
            &entry,
        );
        assert!(
            server.published_diagnostics().is_empty(),
            "{:?}",
            server
                .published_diagnostics()
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
        // The shared module: no `main`, no admission walk — clean, but hover
        // on `load` still shows its requirement.
        let entry = dir.join("src/store.vl");
        let text = std::fs::read_to_string(&entry).unwrap();
        let module = Document::analyze(&text, &std_root(), &entry);
        assert!(module.published_diagnostics().is_empty());
        let hover = module
            .hover(text.find("load").unwrap())
            .expect("hover on `load` should produce a label");
        assert!(
            hover.contains("requires the `process` layer of `std`"),
            "{hover}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // B36: a shared (non-entry) file in a two-entry package importing a name
    // only the PROCESS twin of `std::ui` declares (`render`). The old
    // inference read any `std::ui` import as browser evidence, analyzed the
    // file as browser, and red-flagged the import — while `vilan build` was
    // clean on every entry. Name-level evidence infers Node here.
    #[test]
    fn a_shared_file_importing_the_process_twins_name_is_not_red_flagged() {
        let manifest =
            "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n";
        let shared = "import std::ui::{ view, View, render };\n\nfun page_markup(): str {\n\trender(view(\"main\").text(\"hi\"))\n}\n";
        let entry = "import std::print;\n\nfun main() {\n\tprint(\"server\");\n}\n";
        let (dir, _client) = analyze_workspace(&[
            ("src/client.vl", entry),
            ("vilan.toml", manifest),
            ("src/page.vl", shared),
            ("src/server.vl", entry),
        ]);
        let path = dir.join("src/page.vl");
        let text = std::fs::read_to_string(&path).unwrap();
        let document = Document::analyze(&text, &std_root(), &path);
        assert!(
            document.published_diagnostics().is_empty(),
            "{:?}",
            document
                .published_diagnostics()
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The mirror B36 guards: a shared file importing a name only the BROWSER
    // twin declares (`mount`) must keep inferring browser — a module-level
    // "twins are never evidence" rule would have broken this direction.
    #[test]
    fn a_shared_file_importing_the_browser_twins_name_still_infers_browser() {
        let manifest =
            "[package]\nname = \"app\"\n\n[entry.client]\ntarget = \"browser\"\n\n[entry.server]\n";
        let shared = "import std::ui::{ view, View, mount };\n\nfun attach() {\n\tmount(\"app\", view(\"main\").text(\"hi\"));\n}\n";
        let entry = "import std::print;\n\nfun main() {\n\tprint(\"server\");\n}\n";
        let (dir, _client) = analyze_workspace(&[
            ("src/client.vl", entry),
            ("vilan.toml", manifest),
            ("src/widget.vl", shared),
            ("src/server.vl", entry),
        ]);
        let path = dir.join("src/widget.vl");
        let text = std::fs::read_to_string(&path).unwrap();
        let document = Document::analyze(&text, &std_root(), &path);
        assert!(
            document.published_diagnostics().is_empty(),
            "{:?}",
            document
                .published_diagnostics()
                .iter()
                .map(|item| &item.message)
                .collect::<Vec<_>>()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The hover text at the cursor marked `|` in `src` (a bare manifest-less
    /// file, like `completions_at_cursor` — keep the sources closure-free, the
    /// marker would collide with closure pipes).
    fn hover_at_cursor(src: &str) -> Option<String> {
        let offset = src
            .find('|')
            .expect("test source needs a `|` cursor marker");
        let text = src.replace('|', "");
        let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
        document.hover(offset)
    }

    // Hovering a function name appends its inferred platform requirement — the
    // coloring fixpoint surfaced in the editor, with the same via-chain
    // vocabulary the diagnostics use.
    #[test]
    fn hover_appends_a_functions_platform_requirement() {
        let hover = hover_at_cursor(
            "import std::fs;\n\nfun save() {\n\tfs::write_file(\"state\", \"data\");\n}\n\nfun main() {\n\tsa|ve();\n}\n",
        )
        .expect("hovering `save` should produce a label");
        assert!(
            hover.contains("requires the `process` layer of `std` (via `write_file (std::fs)`)"),
            "{hover}"
        );
    }

    // A spread parameter's `...` rides the hover (variadic-generics.md §S):
    // unlike `mut`, it IS part of the signature, and it is precisely what tells
    // the reader whether to write the arguments out flat or as one tuple.
    #[test]
    fn hover_shows_a_spread_parameters_marker() {
        let hover = hover_at_cursor(
            "fun width<T: (..)>(sep: str, ...items: T): i32 {\n\t1\n}\n\nfun main() {\n\twi|dth(\"-\", 1, 2);\n}\n",
        )
        .expect("hovering `width` should produce a label");
        assert!(hover.contains("...items: T"), "{hover}");
        assert!(hover.contains("sep: str"), "{hover}");
    }

    // The declaration name carries the requirement too, not just call sites.
    #[test]
    fn hover_on_the_definition_name_carries_the_requirement() {
        let hover = hover_at_cursor(
            "import std::fs;\n\nfun sa|ve() {\n\tfs::write_file(\"state\", \"data\");\n}\n\nfun main() {\n\tsave();\n}\n",
        );
        assert!(
            hover
                .as_deref()
                .is_some_and(|hover| { hover.contains("requires the `process` layer of `std`") }),
            "hover on the declaration name should carry the requirement: {hover:?}"
        );
    }

    // A method call resolves through its wired subject to the method function,
    // whose requirement rides the hover alongside the call's type.
    #[test]
    fn hover_on_a_method_call_attributes_the_methods_requirement() {
        let hover = hover_at_cursor(
            "import std::fs;\n\nstruct Store { path: str }\n\nimpl Store {\n\tfun persist(self): bool {\n\t\tfs::write_file(self.path, \"state\");\n\t\ttrue\n\t}\n}\n\nfun main() {\n\tlet store = Store { path = \"s.txt\" };\n\tstore.per|sist();\n}\n",
        )
        .expect("hovering `persist` should produce a label");
        assert!(
            hover.contains("requires the `process` layer of `std` (via `write_file (std::fs)`)"),
            "{hover}"
        );
    }

    // A module-level binding's requirement rides hover like a function's —
    // its initializer is code, and the line says what running it needs.
    #[test]
    fn hover_on_a_global_reference_shows_the_initializers_requirement() {
        let hover = hover_at_cursor(
            "import std::fs::read_file_to_str;\n\nlet cache = read_file_to_str(\"cache.txt\");\n\nfun main() {\n\tlet content = ca|che;\n}\n",
        );
        assert!(
            hover.as_deref().is_some_and(|hover| hover.contains(
                "requires the `process` layer of `std` (via `read_file_to_str (std::fs)`)"
            )),
            "{hover:?}"
        );
    }

    // E2: semantic tokens classify from the ANALYZED program. The cases
    // TextMate cannot get right: a generic parameter at use, a macro name
    // (which deliberately shares its trait's name), method vs field on the
    // same `.name` shape, and module qualifiers.
    #[test]
    fn semantic_tokens_classify_the_ambiguous_cases() {
        let text = "import std::math;\n\nstruct Point {\n\tx: i32,\n}\n\nfun pick<T>(value: T): T {\n\tvalue\n}\n\nfun main() {\n\tlet p = Point { x = 1 };\n\tlet n = p.x;\n\tlet low = math::min(1, 2);\n\tlet chosen = pick(n);\n\tlet size = chosen.abs();\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let tokens = document.semantic_tokens();
        let kind_of = |snippet: &str, occurrence: usize| -> Option<TokenKind> {
            let mut start = 0;
            let mut position = None;
            for _ in 0..=occurrence {
                position = text[start..].find(snippet).map(|at| start + at);
                start = position? + 1;
            }
            let at = position?;
            tokens
                .iter()
                .find(|(span, _, _)| {
                    let range = span.into_range();
                    range.start == at && range.end == at + snippet.len()
                })
                .map(|(_, kind, _)| *kind)
        };
        // The generic parameter at its USE site (T in `value: T`).
        assert_eq!(
            kind_of("T", 1),
            Some(TokenKind::TypeParameter),
            "{tokens:?}"
        );
        // A struct name in type/constructor position.
        assert_eq!(kind_of("Point", 1), Some(TokenKind::Struct), "{tokens:?}");
        // A field read is a property, not a method.
        assert_eq!(kind_of("x", 2), Some(TokenKind::Property), "{tokens:?}");
        // A module import name is a namespace.
        assert_eq!(kind_of("math", 0), Some(TokenKind::Namespace), "{tokens:?}");
        // Parameters and variables split.
        assert_eq!(
            kind_of("value", 0),
            Some(TokenKind::Parameter),
            "{tokens:?}"
        );
        assert_eq!(
            kind_of("chosen", 0),
            Some(TokenKind::Variable),
            "{tokens:?}"
        );
        // A member CALL is a method (the same `.name` shape as the property
        // read above — only semantics can split them).
        assert_eq!(kind_of("abs", 0), Some(TokenKind::Method), "{tokens:?}");
    }

    #[test]
    fn semantic_tokens_paint_markup() {
        // Element-syntax S5: tags (open AND close) paint as Tag, attribute and
        // event names as Property, and the desugar's `<tag` scaffolding token
        // is suppressed. The fixture builds UI outside a boundary, so analysis
        // reports the owner fence — tokens are computed regardless, which is
        // itself the salvage property the markup pass relies on.
        let text = "import std::ui::{ view, View };\n\nfun page(): View {\n\t<div aria-label(\"x\")>\"hi\" <span/></div>\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let tokens = document.semantic_tokens();
        let kind_of = |snippet: &str, occurrence: usize| -> Option<TokenKind> {
            let mut start = 0;
            let mut position = None;
            for _ in 0..=occurrence {
                position = text[start..].find(snippet).map(|at| start + at);
                start = position? + 1;
            }
            let at = position?;
            tokens
                .iter()
                .find(|(span, _, _)| {
                    let range = span.into_range();
                    range.start == at && range.end == at + snippet.len()
                })
                .map(|(_, kind, _)| *kind)
        };
        // `div` appears in the import line? No — occurrence 0 is the open tag.
        assert_eq!(kind_of("div", 0), Some(TokenKind::Tag), "{tokens:?}");
        assert_eq!(kind_of("div", 1), Some(TokenKind::Tag), "{tokens:?}");
        assert_eq!(kind_of("span", 0), Some(TokenKind::Tag), "{tokens:?}");
        assert_eq!(
            kind_of("aria-label", 0),
            Some(TokenKind::Property),
            "{tokens:?}"
        );
        // The `<div` scaffolding Function token is suppressed.
        assert_eq!(kind_of("<div", 0), None, "{tokens:?}");
        // The invariant the sweep guarantees, re-checked over markup.
        let mut last_end = 0usize;
        for (span, _, _) in &tokens {
            let range = span.into_range();
            assert!(
                range.start >= last_end && range.end > range.start,
                "{tokens:?}"
            );
            last_end = range.end;
        }
    }

    #[test]
    fn linked_tag_ranges_pair_open_and_close() {
        let text =
            "import std::ui::{ view, View };\n\nfun page(): View {\n\t<div>\"hi\"</div>\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let open_at = text.find("<div").expect("fixture") + 1;
        let (open, close) = document.linked_tag_ranges(open_at).expect("a pair");
        assert_eq!(&text[open.into_range()], "div");
        assert_eq!(&text[close.into_range()], "div");
        assert!(close.start > open.end);
        // From inside the CLOSING tag, the same pair.
        let close_at = close.start + 1;
        assert_eq!(document.linked_tag_ranges(close_at), Some((open, close)));
        // A self-closing element has no pair; elsewhere in the file, none.
        let solo = "import std::ui::{ view, View };\n\nfun page(): View {\n\t<div />\n}\n";
        let document = Document::analyze(solo, &std_root(), Path::new("test.vl"));
        let at = solo.find("<div").expect("fixture") + 1;
        assert_eq!(document.linked_tag_ranges(at), None);
        assert_eq!(document.linked_tag_ranges(0), None);
    }

    #[test]
    fn semantic_tokens_are_sorted_and_non_overlapping() {
        let text = "import std::option::Option::{ self, Some, None };\n\nfun main() {\n\tlet maybe = Some(2);\n\tlet doubled = maybe? * 2;\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let tokens = document.semantic_tokens();
        assert!(!tokens.is_empty());
        let mut last_end = 0;
        for (span, _, _) in &tokens {
            let range = span.into_range();
            assert!(range.start >= last_end, "overlap at {range:?}: {tokens:?}");
            assert!(range.end > range.start);
            last_end = range.end;
        }
    }

    // E6: a dependent's analysis reads an OPEN document's buffer, not the
    // file on disk — the overlay seam in `load_package_module`. The disk
    // copy of the helper only defines `one`; the overlay renames it to
    // `two`, and the entry calling `two()` analyzes clean exactly when the
    // overlay is consulted.
    #[test]
    fn a_dependents_analysis_reads_the_open_buffer_not_the_disk() {
        let dir = std::env::temp_dir().join(format!("vilan-e6-overlay-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let helper_path = dir.join("helper.vl");
        std::fs::write(&helper_path, "export fun one(): i32 {\n\t1\n}\n").expect("write helper");
        let entry_path = dir.join("main.vl");
        let entry_text =
            "import pkg::helper::two;\n\nfun main() {\n\tlet _x = two();\n}\n".to_string();
        std::fs::write(&entry_path, &entry_text).expect("write entry");

        // Disk truth: `two` does not exist — the entry has errors.
        let stale = Document::analyze(&entry_text, &std_root(), &entry_path);
        assert!(
            !stale.diagnostics.is_empty(),
            "expected the disk-backed analysis to fail on `two`"
        );

        // The helper is "open" with an edited, unsaved buffer defining `two`.
        vilan_core::analyzer::set_document_overlay(
            &helper_path,
            Some("export fun two(): i32 {\n\t2\n}\n".to_string()),
        );
        let live = Document::analyze(&entry_text, &std_root(), &entry_path);
        vilan_core::analyzer::set_document_overlay(&helper_path, None);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            live.diagnostics.is_empty(),
            "expected the overlay-backed analysis to be clean, got {:?}",
            live.diagnostics
        );
    }

    // Expression lifting (expression-lifting.md): hovering the RECEIVER of a
    // bare `?` shows the receiver's own container type — the region's binder
    // entity carries an empty span exactly so it cannot tie with the
    // receiver in the narrowest-span selection and leak the element type.
    #[test]
    fn hover_on_a_lift_receiver_shows_the_container_type() {
        let hover = hover_at_cursor(
            "import std::option::Option::{ self, Some, None };\n\nfun main() {\n\tlet count = Some(2);\n\tlet doubled = cou|nt? * 2;\n}\n",
        )
        .expect("hovering the receiver should produce a label");
        assert!(hover.contains("Option<i32>"), "{hover}");
    }

    // The binding a region initializes hovers as the lifted type.
    #[test]
    fn hover_on_a_region_initialized_binding_shows_the_lifted_type() {
        let hover = hover_at_cursor(
            "import std::option::Option::{ self, Some, None };\n\nfun main() {\n\tlet count = Some(2);\n\tlet dou|bled = count? * 2;\n}\n",
        )
        .expect("hovering the binding should produce a label");
        assert!(hover.contains("Option<i32>"), "{hover}");
    }

    // The applicative form analyzes and hovers without incident too — the
    // whole-document smoke for the region machinery under the LSP path.
    #[test]
    fn hover_across_an_applicative_region_document() {
        let hover = hover_at_cursor(
            "import std::option::Option::{ self, Some, None };\n\nfun main() {\n\tlet price = Some(40);\n\tlet tax = Some(2);\n\tlet tot|al = price? + tax?;\n}\n",
        )
        .expect("hovering the binding should produce a label");
        assert!(hover.contains("Option<i32>"), "{hover}");
    }

    // E9: hovering a function shows its FULL signature, fenced as code —
    // parameter names and types, the return type.
    #[test]
    fn hover_shows_the_full_function_signature() {
        let hover = hover_at_cursor(
            "import std::print;\n\nfun descr|ibe(count: i32, label: str): str {\n\tlabel\n}\n\nfun main() {\n\tprint(describe(1, \"x\"));\n}\n",
        )
        .expect("hovering the declaration should produce a label");
        assert!(
            hover.contains("```vilan\nfun describe(count: i32, label: str): str\n```"),
            "{hover}"
        );
    }

    // E9: INFERRED async (no `async` keyword written) prepends to the
    // signature — inference runs after the labels are built, so the server
    // adds it.
    #[test]
    fn hover_prepends_inferred_async() {
        let hover = hover_at_cursor(
            "import std::time::{ sleep_for, Duration };\n\nfun wa|rm() {\n\tsleep_for(Duration::millis(1));\n}\n\nfun main() {\n\twarm();\n}\n",
        )
        .expect("hover on the declaration");
        assert!(hover.contains("```vilan\nasync fun warm()\n```"), "{hover}");
    }

    // E9 (rule4-completion S1): the inferred `borrows` root-set surfaces in the
    // signature like the source clause. A `&mut self` method returning a
    // projection of `self` renders `borrows self` though no clause was written.
    #[test]
    fn hover_shows_an_inferred_single_borrows_position() {
        let hover = hover_at_cursor(
            "import std::print;\n\nstruct Wrapper { value: i32 }\n\nimpl Wrapper {\n\tfun sl|ot(&mut self): &mut i32 {\n\t\t&mut self.value\n\t}\n}\n\nfun main() {\n\tmut w = Wrapper { value = 1 };\n\tw.slot() = 2;\n\tprint(w.value);\n}\n",
        )
        .expect("hovering `slot` should produce a label");
        assert!(hover.contains("borrows self"), "{hover}");
    }

    // A wrapped view projecting a different parameter per branch unions both
    // positions; the clause names them in order — `borrows a, b`.
    #[test]
    fn hover_shows_an_inferred_multi_borrows_position() {
        let hover = hover_at_cursor(
            "import std::option::Option::{ self, Some, None };\n\nstruct Box { x: i32 }\n\nfun pi|ck(a: &mut Box, b: &mut Box, first: bool): Option<&mut i32> {\n\tif first { Some(&mut a.x) } else { Some(&mut b.x) }\n}\n\nfun main() {\n\tmut p = Box { x = 1 };\n\tmut q = Box { x = 2 };\n\tmatch pick(&mut p, &mut q, true) {\n\t\tSome(let v) => { v = 9; }\n\t\tNone => {}\n\t}\n}\n",
        )
        .expect("hovering `pick` should produce a label");
        assert!(hover.contains("borrows a, b"), "{hover}");
    }

    // The rendered position is the one the chain projects, not always the
    // receiver: `pick` returns `grow(b)`, so it borrows `b` — `borrows b`.
    #[test]
    fn hover_shows_a_chained_non_receiver_borrows_position() {
        let hover = hover_at_cursor(
            "fun grow(x: &mut i32): &mut i32 borrows x {\n\tx\n}\n\nfun pi|ck(a: &mut i32, b: &mut i32): &mut i32 {\n\tgrow(b)\n}\n\nfun main() {\n\tmut p = 1;\n\tmut q = 2;\n\tpick(&mut p, &mut q) = 9;\n}\n",
        )
        .expect("hovering `pick` should produce a label");
        assert!(hover.contains("borrows b"), "{hover}");
        assert!(!hover.contains("borrows a"), "{hover}");
    }

    // The inferred `bumps` effect renders after `borrows` (rule-4 S2, C6): a
    // geometry-advancing mutator names its bumping parameter.
    #[test]
    fn hover_shows_an_inferred_bumps_clause() {
        let hover = hover_at_cursor(
            "fun to|uch(xs: &mut List<i32>) {\n\txs.push(1);\n}\n\nfun main() {\n\tmut xs = [ 1 ];\n\ttouch(&mut xs);\n}\n",
        )
        .expect("hovering `touch` should produce a label");
        assert!(hover.contains("bumps xs"), "{hover}");
    }

    // A content-stable `&mut` mutator (field writes only) carries NO bumps
    // clause — the absence is the verdict.
    #[test]
    fn hover_omits_bumps_for_a_content_stable_mutator() {
        let hover = hover_at_cursor(
            "struct Point { x: i32, y: i32 }\n\nfun re|tag(p: &mut Point) {\n\tp.x = 1;\n}\n\nfun main() {\n\tmut p = Point { x = 0, y = 0 };\n\tretag(&mut p);\n}\n",
        )
        .expect("hovering `retag` should produce a label");
        assert!(!hover.contains("bumps"), "{hover}");
    }

    // E9: the declaration's leading `///` block surfaces as prose, and
    // attribute lines between it and the name don't break the chain.
    #[test]
    fn hover_surfaces_the_leading_doc_comment() {
        let hover = hover_at_cursor(
            "import std::print;\n\n/// Renders the badge label.\n/// Two lines of docs.\n[must_use]\nfun bad|ge(count: i32): str {\n\t\"b\"\n}\n\nfun main() {\n\tlet _b = badge(1);\n\tprint(\"x\");\n}\n",
        )
        .expect("hover on the declaration");
        assert!(
            hover.contains("Renders the badge label.\nTwo lines of docs."),
            "{hover}"
        );
        assert!(hover.contains("fun badge(count: i32): str"), "{hover}");
    }

    // WO-4 variables: a local `let` hovers as its typed binding — `let name: T`,
    // fenced like a declaration, the type resolved by inference.
    #[test]
    fn hover_on_a_local_let_shows_its_typed_binding() {
        let hover = hover_at_cursor("fun main() {\n\tlet cou|nt = 5;\n\tlet _ = count;\n}\n")
            .expect("hover on the let binding");
        assert!(hover.contains("```vilan\nlet count: i32\n```"), "{hover}");
    }

    // A `mut` binding hovers with the `mut` keyword — it can be reassigned.
    #[test]
    fn hover_on_a_mut_binding_shows_mut() {
        let hover = hover_at_cursor("fun main() {\n\tmut tot|al = 0;\n\ttotal = 1;\n}\n")
            .expect("hover on the mut binding");
        assert!(hover.contains("```vilan\nmut total: i32\n```"), "{hover}");
    }

    // A module-level binding hovers as a `let` too, not just locals.
    #[test]
    fn hover_on_a_module_binding_shows_its_typed_binding() {
        let hover =
            hover_at_cursor("let cap|acity = 100;\n\nfun main() {\n\tlet _ = capacity;\n}\n")
                .expect("hover on the module binding");
        assert!(
            hover.contains("```vilan\nlet capacity: i32\n```"),
            "{hover}"
        );
    }

    // A destructured binder hovers as `let name: T` with its ELEMENT type.
    #[test]
    fn hover_on_a_destructured_binder_shows_its_element_type() {
        let hover = hover_at_cursor(
            "fun main() {\n\tlet (a|a, bb) = (1, 2);\n\tlet _ = aa;\n\tlet _ = bb;\n}\n",
        )
        .expect("hover on the destructured binder");
        assert!(hover.contains("```vilan\nlet aa: i32\n```"), "{hover}");
    }

    // A use site hovers identically to the declaration it resolves to.
    #[test]
    fn hover_on_a_binding_use_site_matches_the_declaration() {
        let hover = hover_at_cursor("fun main() {\n\tlet count = 5;\n\tlet _ = cou|nt;\n}\n")
            .expect("hover on the use site");
        assert!(hover.contains("```vilan\nlet count: i32\n```"), "{hover}");
    }

    // A binding's leading `///` doc rides its hover, like a declaration's.
    #[test]
    fn hover_on_a_binding_surfaces_its_doc_comment() {
        let hover = hover_at_cursor(
            "fun main() {\n\t/// how many things\n\tlet cou|nt = 5;\n\tlet _ = count;\n}\n",
        )
        .expect("hover on the documented binding");
        assert!(hover.contains("```vilan\nlet count: i32\n```"), "{hover}");
        assert!(hover.contains("how many things"), "{hover}");
    }

    // WO-4 parameters: a plain parameter hovers as `name: T`.
    #[test]
    fn hover_on_a_plain_parameter_shows_name_and_type() {
        let hover = hover_at_cursor(
            "fun f(coun|t: i32): i32 {\n\tcount\n}\n\nfun main() {\n\tlet _ = f(1);\n}\n",
        )
        .expect("hover on the plain parameter");
        assert!(hover.contains("```vilan\ncount: i32\n```"), "{hover}");
    }

    // An `own` parameter carries its convention: `own name: T`.
    #[test]
    fn hover_on_an_own_parameter_shows_the_own_convention() {
        let hover = hover_at_cursor(
            "struct Box { n: i32 }\n\nfun consume(own |b: Box): i32 {\n\tb.n\n}\n\nfun main() {\n\tlet _ = consume(Box { n = 1 });\n}\n",
        )
        .expect("hover on the own parameter");
        assert!(hover.contains("```vilan\nown b: Box\n```"), "{hover}");
    }

    // A `&` (readonly view) parameter — the `&` lives on the convention, not the
    // type, so hover reconstructs `name: &T`.
    #[test]
    fn hover_on_a_ref_parameter_shows_the_ref_convention() {
        let hover = hover_at_cursor("fun peek(|x: &i32): i32 {\n\tx\n}\n\nfun main() {\n\tlet a = 1;\n\tlet _ = peek(&a);\n}\n")
            .expect("hover on the ref parameter");
        assert!(hover.contains("```vilan\nx: &i32\n```"), "{hover}");
    }

    // A `&mut` (writable view) parameter, hovered at a USE site — the convention
    // is not in the pre-rendered type, so hover adds `&mut` back.
    #[test]
    fn hover_on_a_mut_ref_parameter_use_shows_the_mut_ref_convention() {
        let hover = hover_at_cursor(
            "fun f(xs: &mut i32) {\n\tx|s = 1;\n}\n\nfun main() {\n\tmut a = 0;\n\tf(&mut a);\n}\n",
        )
        .expect("hover on the &mut parameter use");
        assert!(hover.contains("```vilan\nxs: &mut i32\n```"), "{hover}");
    }

    // A function-typed parameter shows its closure shape (`|A| R`). The source
    // carries closure pipes, so the `|` cursor marker can't be used — the offset
    // is computed straight onto the parameter name.
    #[test]
    fn hover_on_a_closure_parameter_shows_its_shape() {
        let text = "fun apply(g: |i32| i32): i32 {\n\tg(1)\n}\n\nfun main() {\n\tlet _ = apply(fun(x: i32): i32 { x });\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let offset = text.find("(g:").unwrap() + 1;
        let hover = document
            .hover(offset)
            .expect("hover on the closure parameter");
        assert!(hover.contains("```vilan\ng: |i32| i32\n```"), "{hover}");
    }

    // WO-4 keywords: a keyword hovers as one crisp sentence + a book deep link.
    // Covers the flagship memory-model word `resource` (spec link), a second
    // memory-model word `own` (spec link), and a control-flow word `for` (tour
    // link) — sentence AND URL asserted per case.
    #[test]
    fn hover_on_a_keyword_shows_its_meaning_and_book_link() {
        let hover = hover_at_cursor("resou|rce struct File { fd: i32 }\n\nfun main() {}\n")
            .expect("hover on `resource`");
        assert!(
            hover.contains("An owned value with exactly one owner, moved rather than copied"),
            "{hover}"
        );
        assert!(
            hover.contains(
                "https://vilan-lang.org/docs/spec/memory.html#68-resources-and-destruction"
            ),
            "{hover}"
        );

        let hover = hover_at_cursor(
            "struct Box { n: i32 }\n\nfun consume(o|wn b: Box): i32 {\n\tb.n\n}\n\nfun main() {\n\tlet _ = consume(Box { n = 1 });\n}\n",
        )
        .expect("hover on `own`");
        assert!(hover.contains("moves ownership into the callee"), "{hover}");
        assert!(
            hover.contains("https://vilan-lang.org/docs/spec/memory.html#63-rule-3"),
            "{hover}"
        );

        let hover =
            hover_at_cursor("fun main() {\n\tfo|r x in [ 1, 2 ] {\n\t\tlet _ = x;\n\t}\n}\n")
                .expect("hover on `for`");
        assert!(hover.contains("Iterates over the elements"), "{hover}");
        assert!(
            hover.contains("https://vilan-lang.org/docs/tour/control-flow.html#loops"),
            "{hover}"
        );
    }

    // A keyword hovers even on a document that does not compile — the lookup is
    // purely lexical, ahead of any analysis.
    #[test]
    fn hover_on_a_keyword_works_without_a_program() {
        let text = "fun main() {\n\tresource\n}\n"; // `resource` misused — analysis fails.
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let offset = text.find("resource").unwrap() + 1;
        let hover = document
            .hover(offset)
            .expect("keyword hover without a program");
        assert!(
            hover.contains("An owned value with exactly one owner"),
            "{hover}"
        );
    }

    // The keyword table stays in lockstep with the lexer: every documented
    // keyword lexes to exactly one keyword token that classifies back to
    // itself. If a new keyword lands in the lexer, `keyword_lexeme` (exhaustive
    // over `Token`) forces a new arm, and this pin forces its `KEYWORD_DOCS`
    // entry — so no keyword ships without a hover.
    #[test]
    fn every_documented_keyword_round_trips_through_the_lexer() {
        for (keyword, _sentence, _link) in KEYWORD_DOCS {
            let (tokens, errors) = tokenize(keyword);
            assert!(errors.is_empty(), "{keyword} lexed with errors: {errors:?}");
            assert_eq!(tokens.len(), 1, "{keyword} should lex to one token");
            assert_eq!(
                keyword_lexeme(&tokens[0].0),
                Some(*keyword),
                "{keyword} must classify back to itself"
            );
        }
    }

    // A no-hover case that must STAY silent: whitespace between items and a
    // comment name no entity and are no keyword.
    #[test]
    fn hover_stays_silent_on_whitespace_and_comments() {
        let hover = hover_at_cursor("fun a() {}\n|\nfun main() {}\n");
        assert!(hover.is_none(), "whitespace should not hover: {hover:?}");
        let text = "fun a() {}\n// just a note\nfun main() {}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let offset = text.find("just").unwrap();
        assert!(
            document.hover(offset).is_none(),
            "a comment should not hover: {:?}",
            document.hover(offset)
        );
    }

    // The macro-LSP tail's last piece: `[` at an item position offers the
    // registered macro names — derives included — and `[derive(` offers
    // them for the derive list.
    #[test]
    fn attribute_position_completes_macro_names() {
        let completions = completions_at_cursor(
            "import std::print;\n\n[Hash|]\nstruct Point { x: i32 }\n\nfun main() {\n\tprint(1);\n}\n",
        );
        assert!(
            completions.iter().any(|label| label == "Hashable"),
            "expected the derive prelude: {completions:?}"
        );
        assert!(
            completions.iter().any(|label| label == "Json"),
            "{completions:?}"
        );
        let derive_completions = completions_at_cursor(
            "import std::print;\n\n[derive(Pa|)]\nstruct Point { x: i32 }\n\nfun main() {\n\tprint(1);\n}\n",
        );
        assert!(
            derive_completions.iter().any(|label| label == "PartialEq"),
            "{derive_completions:?}"
        );
    }

    // Inlay hints: an UNANNOTATED binding shows its inferred type in
    // place; an annotated one shows nothing (the source already says it).
    #[test]
    fn inlay_hints_show_inferred_types_only() {
        let text = "import std::option::Option::{ self, Some, None };\n\nfun main() {\n\tlet count = Some(2);\n\tlet doubled = count? * 2;\n\tlet named: i32 = 4;\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let hints = document.inlay_hints();
        let hint_after = |name: &str| {
            let at = text.find(name).unwrap() + name.len();
            hints
                .iter()
                .find(|(offset, _)| *offset == at)
                .map(|(_, label)| label.clone())
        };
        assert_eq!(
            hint_after("doubled"),
            Some(": Option<i32>".to_string()),
            "{hints:?}"
        );
        assert!(hint_after("count").is_some(), "{hints:?}");
        assert_eq!(hint_after("named"), None, "{hints:?}");
    }

    // Token modifiers: declarations carry `declaration`; an immutable
    // binding and its uses carry `readonly`, a `mut` one does not.
    #[test]
    fn semantic_token_modifiers_split_readonly_and_declarations() {
        let text = "import std::print;\n\nfun main() {\n\tlet fixed = 1;\n\tmut counter = 2;\n\tprint(fixed + counter);\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let tokens = document.semantic_tokens();
        let modifiers_at = |at: usize, len: usize| {
            tokens
                .iter()
                .find(|(span, _, _)| {
                    let range = span.into_range();
                    range.start == at && range.end == at + len
                })
                .map(|(_, _, modifiers)| *modifiers)
        };
        let fixed_declaration = text.find("fixed").unwrap();
        let counter_declaration = text.find("counter").unwrap();
        let fixed_use = text.rfind("fixed").unwrap();
        let counter_use = text.rfind("counter").unwrap();
        assert_eq!(
            modifiers_at(fixed_declaration, 5),
            Some(MODIFIER_DECLARATION | MODIFIER_READONLY),
            "{tokens:?}"
        );
        assert_eq!(
            modifiers_at(counter_declaration, 7),
            Some(MODIFIER_DECLARATION),
            "{tokens:?}"
        );
        assert_eq!(
            modifiers_at(fixed_use, 5),
            Some(MODIFIER_READONLY),
            "{tokens:?}"
        );
        assert_eq!(modifiers_at(counter_use, 7), Some(0), "{tokens:?}");
    }

    // E9: hover on a `const` binding shows its evaluated VALUE beside the
    // type — the LSP evaluation is fuel-capped and skips broken documents.
    #[test]
    fn hover_shows_a_constants_value() {
        let hover = hover_at_cursor(
            "import std::print;\n\nlet SIZE = const 8 * 8;\n\nfun main() {\n\tprint(SI|ZE);\n}\n",
        )
        .expect("hover on the constant");
        assert!(hover.contains("= 64"), "{hover}");
    }

    // --- trivia does not hover (found probing the 2026-07-28 report) --------
    //
    // Hover's containment fallback (`entity_at`) answers for any offset inside
    // an entity's span — and a function's span contains its whole body, trivia
    // included, so a comment or a blank line inside it hovered as
    // `fun main()`. Trivia is not code: those offsets answer nothing. Offsets
    // ON tokens keep containment (the closing brace still names its function).

    #[test]
    fn a_comment_inside_a_body_does_not_hover_as_the_function() {
        assert_eq!(
            hover_at_cursor("fun main() {\n\t// a lo|ne note\n\tlet _x = 1;\n}\n"),
            None,
        );
    }

    #[test]
    fn a_blank_line_inside_a_body_does_not_hover() {
        assert_eq!(hover_at_cursor("fun main() {\n\tlet _x = 1;\n|\n}\n"), None,);
    }

    #[test]
    fn a_top_level_comment_does_not_hover() {
        assert_eq!(
            hover_at_cursor(
                "// a to|p-level note\nlet size = 1;\nfun main() {\n\tlet _s = size;\n}\n"
            ),
            None,
        );
    }

    // The boundary of the gate: a real token inside the body still resolves by
    // containment — trivia was the wart, not containment itself.
    #[test]
    fn a_body_brace_still_hovers_the_enclosing_function() {
        let hover = hover_at_cursor("fun main() {\n\tlet _x = 1;\n|}\n").expect("the brace hovers");
        assert!(hover.contains("fun main()"), "{hover}");
    }

    // The crash shape (page.vl's `stack`, 2026-07-28): a const whose rendered
    // value is long multi-byte text, so byte 160 of the preview lands inside
    // an em-dash. The clamp used to be a bare `String::truncate(160)`, which
    // PANICS off a char boundary — and a hover panic took the whole server
    // down (five crashes and the client stops restarting it). The pin is the
    // absence of that panic, plus the visible contract: clamped with an
    // ellipsis.
    #[test]
    fn hover_clamps_a_long_multibyte_constant_without_panicking() {
        let source = format!(
            "import std::print;\n\nlet BANNER = const \"ab{}\";\n\nfun main() {{\n\tprint(BAN|NER);\n}}\n",
            "—".repeat(70),
        );
        let hover = hover_at_cursor(&source).expect("hover on the constant");
        assert!(hover.contains('…'), "the preview is clamped: {hover}");
    }

    // --- E73: the hover chain guard (editing-dx.md §19) ---------------------
    //
    // The context-threading pass (vilan-core `context.rs`, `apply`) lowers an
    // unprovided `get_safe()` read to a plain read of a pass-minted hidden
    // parameter: `entity_map[call] = Local(hidden)`, where `hidden`'s own
    // entry is the self-describing `Parameter(hidden)` — a SELF-LOOP — with
    // no `expr_types` entry and no `parameters` record. `hover_label`
    // resolved id → binding recursively with no cycle guard, so hovering
    // `get_safe` overflowed the server's stack: SIGABRT, five restarts, and
    // the client stops restarting it (the owner's live crash, 2026-08-19).

    /// The owner's crash shape, distilled: a module-level `Context<T>`
    /// binding with no provider, `get_safe()` on it in a function nothing
    /// calls.
    const LOWERED_GET_SAFE_SOURCE: &str = "import std::context::Context;\n\nlet app_context = Context<i32>::new();\n\nfun probe() {\n\tapp_context.get_safe();\n}\n";

    #[test]
    fn hover_on_a_lowered_context_read_returns() {
        let document =
            Document::analyze(LOWERED_GET_SAFE_SOURCE, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = LOWERED_GET_SAFE_SOURCE.find("get_safe").unwrap() + 2;
        let id = document
            .entity_at(offset)
            .expect("an entity under `get_safe`");
        // The enabling shape, asserted so this pin says so if the lowering
        // ever stops minting it: the read resolves to a binding whose own
        // entity record is a self-loop with no type.
        let Some(Expr::Local(binding)) = program.entity_map.get(&id) else {
            panic!(
                "the lowered read should wire as a Local: {:?}",
                program.entity_map.get(&id)
            );
        };
        assert!(
            matches!(program.entity_map.get(binding), Some(Expr::Parameter(parameter)) if parameter == binding),
            "the hidden parameter should self-loop: {:?}",
            program.entity_map.get(binding)
        );
        assert!(
            program.expr_types.get(binding).is_none(),
            "and carry no type"
        );
        // The pin is the RETURN: this call used to recurse to a stack
        // overflow and abort the whole server.
        let _ = document.hover(offset);
    }

    /// A guard on the general shape, not just the self-loop the lowering
    /// mints today: a two-node `entity_map` cycle (constructed directly — no
    /// analyzed program is known to wire one) answers the honest `None`.
    #[test]
    fn hover_label_answers_none_on_an_entity_cycle() {
        let mut document = Document::analyze("fun main() {}\n", &std_root(), Path::new("test.vl"));
        let program = document
            .program
            .program
            .as_mut()
            .expect("the program analyzes");
        let first = Id(program.next_entity_id);
        let second = Id(program.next_entity_id + 1);
        program.entity_map.insert(first, Expr::Local(second));
        program.entity_map.insert(second, Expr::Local(first));
        let program = document.program.as_ref().unwrap();
        assert_eq!(document.analysis(program).hover_label(first), None);
        assert_eq!(document.analysis(program).hover_label(second), None);
    }

    /// The same through the call → subject arm: a call whose subject chains
    /// back to the call itself (again constructed — the shape a rewiring
    /// lowering could produce) answers `None` from every resolver that walks
    /// it.
    #[test]
    fn resolvers_answer_none_on_a_call_cycle() {
        let mut document = Document::analyze("fun main() {}\n", &std_root(), Path::new("test.vl"));
        let program = document
            .program
            .program
            .as_mut()
            .expect("the program analyzes");
        let call = Id(program.next_entity_id);
        program.entity_map.insert(call, Expr::Call(call));
        program.function_calls.insert(
            call,
            vilan_core::analyzer::FunctionCall {
                id: call,
                subject_id: call,
                generic_argument_ids: Vec::new(),
                argument_ids: Vec::new(),
                arguments_span: vilan_core::span::Span::new((), 0..0),
            },
        );
        let program = document.program.as_ref().unwrap();
        assert_eq!(document.analysis(program).hover_label(call), None);
        assert_eq!(document.analysis(program).function_target(call), None);
        assert_eq!(document.definition_of(program, call), None);
    }

    /// A chain that ends on a record-less id (no type, no entity entry)
    /// answers `None` — not a panic, not a made-up label.
    #[test]
    fn hover_label_answers_none_when_the_chain_yields_no_type() {
        let mut document = Document::analyze("fun main() {}\n", &std_root(), Path::new("test.vl"));
        let program = document
            .program
            .program
            .as_mut()
            .expect("the program analyzes");
        let first = Id(program.next_entity_id);
        let second = Id(program.next_entity_id + 1);
        program.entity_map.insert(first, Expr::Local(second));
        let program = document.program.as_ref().unwrap();
        assert_eq!(document.analysis(program).hover_label(first), None);
    }

    /// The guard must not cut the legitimate chain: a use site still resolves
    /// through its binding to the binding's type.
    #[test]
    fn hover_label_still_resolves_a_use_through_its_binding() {
        let text = "fun main() {\n\tlet width = 3;\n\tlet doubled = width * 2;\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = text.rfind("width").unwrap() + 1;
        let id = document.entity_at(offset).expect("the use entity");
        assert!(
            program.expr_types.get(&id).is_none(),
            "the use must carry no type of its own, or this pins nothing"
        );
        assert_eq!(
            document.analysis(program).hover_label(id).as_deref(),
            Some("i32")
        );
    }

    // --- E72: member hovers wear the house style (editing-dx.md §19) --------

    // A FIELD hovers as the fenced `name: T` — not the bare pre-house-style
    // type string the fallback used to hand back.
    #[test]
    fn hover_on_a_field_shows_the_fenced_name_and_type() {
        let hover = hover_at_cursor(
            "struct Point { x: i32 }\n\nfun main() {\n\tlet p = Point { x = 1 };\n\tlet n = p.|x;\n}\n",
        )
        .expect("hovering the field should produce a label");
        assert_eq!(hover, "```vilan\nx: i32\n```");
    }

    // A std METHOD name answers the method's declaration, fenced — through
    // `function_target`'s wired subject, like a user method.
    #[test]
    fn hover_on_a_std_method_name_shows_its_signature() {
        let hover =
            hover_at_cursor("fun main() {\n\tlet name = \"vilan\";\n\tlet n = name.l|en();\n}\n")
                .expect("hovering the method should produce a label");
        assert!(hover.starts_with("```vilan\n"), "{hover}");
        assert!(hover.contains("fun len(self): i32"), "{hover}");
    }

    // The E73 crash shape, now answering: the context pass lowers the
    // unprovided `get_safe()` to a hidden-parameter read and rewrites the
    // call's entity record, but the call record's wired subject survives —
    // `function_target` resolves through it to the declaration the source
    // names, instead of the lowered view (`enum Option`, or nothing).
    #[test]
    fn hover_on_an_unprovided_get_safe_shows_its_declaration() {
        let document =
            Document::analyze(LOWERED_GET_SAFE_SOURCE, &std_root(), Path::new("test.vl"));
        let offset = LOWERED_GET_SAFE_SOURCE.find("get_safe").unwrap() + 2;
        let hover = document
            .hover(offset)
            .expect("hovering `get_safe` should answer");
        assert!(hover.starts_with("```vilan\n"), "{hover}");
        assert!(hover.contains("fun get_safe(self): Option<T>"), "{hover}");
    }

    // "Anything else" keeps the bare rendered type but gains the fence: an
    // index expression's hover is its element type, as code.
    #[test]
    fn a_bare_expression_type_hover_wears_the_fence() {
        let hover = hover_at_cursor("fun main() {\n\tlet xs = [ 1 ];\n\tlet n = xs|[0];\n}\n")
            .expect("the index expression hovers");
        assert_eq!(hover, "```vilan\ni32\n```");
    }

    // --- E75: the context lowering records what it erases (editing-dx.md
    // §19.3). The two lowerings that rewire `function_calls[call].subject_id`
    // itself — a covered `get_safe` (the `Some`-wrap) and `Context::run` (the
    // body closure becomes the subject) — record the erased original, and the
    // resolvers answer the SOURCE view through it. --------------------------

    /// A COVERED safe read: the `get_safe` sits in a `run` body, so its
    /// holder carries the bare value and the pass rewires the call into
    /// `Some(hidden)`.
    const COVERED_CONTEXT_SOURCE: &str = "import std::context::Context;\n\nlet app_context = Context<i32>::new();\n\nfun main() {\n\tapp_context.run(7, || {\n\t\tapp_context.get_safe();\n\t});\n}\n";

    #[test]
    fn hover_on_a_covered_get_safe_shows_its_declaration() {
        let document = Document::analyze(COVERED_CONTEXT_SOURCE, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = COVERED_CONTEXT_SOURCE.find("get_safe").unwrap() + 2;
        let id = document
            .entity_at(offset)
            .expect("an entity under `get_safe`");
        // The enabling shape, asserted so this pin announces itself if the
        // lowering changes: the pass rewired the call record's subject away
        // from the wired callee and recorded the erased original.
        let call = program.function_calls.get(&id).expect("the call record");
        let erased = program
            .context_erased_subjects
            .get(&id)
            .expect("the pass records the subject it erases");
        assert_ne!(
            call.subject_id, *erased,
            "the `Some`-wrap should have rewired the subject"
        );
        let hover = document
            .hover(offset)
            .expect("hovering the covered `get_safe` should answer");
        assert!(hover.starts_with("```vilan\n"), "{hover}");
        assert!(
            hover.contains("fun get_safe(self): Option<T>"),
            "the SOURCE callee's signature, not the lowered `Some`: {hover}"
        );
    }

    #[test]
    fn definition_on_a_covered_get_safe_lands_on_the_callee() {
        let document = Document::analyze(COVERED_CONTEXT_SOURCE, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = COVERED_CONTEXT_SOURCE.find("get_safe").unwrap() + 2;
        let (source, span) = document
            .definition(offset)
            .expect("go-to-definition on the covered `get_safe` should answer");
        let get_safe_fn = program
            .context_get_safe_fn_id
            .expect("std's context module loaded");
        assert_eq!(
            source,
            program.source_of(get_safe_fn).expect("get_safe has a file"),
            "the definition lives in std's context.vl"
        );
        let name_span = program
            .external_functions
            .get(&get_safe_fn)
            .expect("`get_safe` is an external fn")
            .name_span;
        assert_eq!(span.into_range(), name_span.into_range());
    }

    #[test]
    fn hover_on_context_run_shows_its_declaration() {
        let document = Document::analyze(COVERED_CONTEXT_SOURCE, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = COVERED_CONTEXT_SOURCE.find(".run(").unwrap() + 2;
        let id = document.entity_at(offset).expect("an entity under `run`");
        // The enabling shape: the body closure became the subject, the
        // erased original is recorded.
        let call = program.function_calls.get(&id).expect("the call record");
        let erased = program
            .context_erased_subjects
            .get(&id)
            .expect("the pass records the subject it erases");
        assert_ne!(
            call.subject_id, *erased,
            "the `run` lowering should have rewired the subject"
        );
        let hover = document
            .hover(offset)
            .expect("hovering `run` should answer");
        assert!(hover.starts_with("```vilan\n"), "{hover}");
        assert!(
            hover.contains("fun run(self, value: T, body: || U): U"),
            "the SOURCE callee's signature, not the closure's type: {hover}"
        );
        assert!(
            hover.contains("yields its body's value"),
            "the declaration's doc rides along: {hover}"
        );
    }

    #[test]
    fn definition_on_context_run_lands_on_the_callee() {
        let document = Document::analyze(COVERED_CONTEXT_SOURCE, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = COVERED_CONTEXT_SOURCE.find(".run(").unwrap() + 2;
        let (source, span) = document
            .definition(offset)
            .expect("go-to-definition on `run` should answer");
        let run_fn = program
            .context_run_fn_id
            .expect("std's context module loaded");
        assert_eq!(
            source,
            program.source_of(run_fn).expect("run has a file"),
            "the definition lives in std's context.vl"
        );
        let name_span = program
            .external_functions
            .get(&run_fn)
            .expect("`run` is an external fn")
            .name_span;
        assert_eq!(span.into_range(), name_span.into_range());
    }

    // The ENTITY-record overwrite (an unprovided `get_safe` lowers to a read
    // of the hidden parameter): the call record survives with its wired
    // subject, and `definition_of` now resolves through it — go-to-definition
    // lands on the callee where it used to answer nothing.
    #[test]
    fn definition_on_an_unprovided_get_safe_lands_on_the_callee() {
        let document =
            Document::analyze(LOWERED_GET_SAFE_SOURCE, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = LOWERED_GET_SAFE_SOURCE.find("get_safe").unwrap() + 2;
        let id = document
            .entity_at(offset)
            .expect("an entity under `get_safe`");
        // The enabling shape: the entity record is the lowered parameter
        // read, not a call — the surviving call record is the only way back.
        assert!(
            matches!(program.entity_map.get(&id), Some(Expr::Local(_))),
            "the lowering should have overwritten the entity record: {:?}",
            program.entity_map.get(&id)
        );
        let (source, span) = document
            .definition(offset)
            .expect("go-to-definition on the lowered `get_safe` should answer");
        let get_safe_fn = program
            .context_get_safe_fn_id
            .expect("std's context module loaded");
        assert_eq!(
            source,
            program.source_of(get_safe_fn).expect("get_safe has a file")
        );
        let name_span = program
            .external_functions
            .get(&get_safe_fn)
            .expect("`get_safe` is an external fn")
            .name_span;
        assert_eq!(span.into_range(), name_span.into_range());
    }

    // The minted hidden parameter carries the pass's marker — parameter id →
    // the context binding it threads — and the chain walkers answer the
    // explicit honest `None` on it (no label, no definition), by design
    // rather than by the self-loop meeting the cycle guard.
    #[test]
    fn the_hidden_parameter_is_marked_and_answers_nothing() {
        let document =
            Document::analyze(LOWERED_GET_SAFE_SOURCE, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        let offset = LOWERED_GET_SAFE_SOURCE.find("get_safe").unwrap() + 2;
        let id = document
            .entity_at(offset)
            .expect("an entity under `get_safe`");
        let Some(Expr::Local(hidden)) = program.entity_map.get(&id) else {
            panic!(
                "the lowered read should wire as a Local: {:?}",
                program.entity_map.get(&id)
            );
        };
        let app_context = program
            .variables
            .iter()
            .find(|(_, variable)| variable.name == "app_context")
            .map(|(id, _)| *id)
            .expect("the context binding");
        assert_eq!(
            program.context_hidden_parameters.get(hidden),
            Some(&app_context),
            "the marker names the context the parameter threads"
        );
        assert_eq!(document.analysis(program).hover_label(*hidden), None);
        assert_eq!(document.definition_of(program, *hidden), None);
    }

    // --- clamp_preview: the hover budget cuts at char boundaries ------------

    // Byte 160 inside a 3-byte character: back up to the boundary below.
    // Boundaries here are 2 + 3k, so the cut lands at 158 (2 ASCII + 52 whole
    // em-dashes) — never mid-character, never a panic.
    #[test]
    fn the_preview_clamp_backs_off_a_three_byte_straddle() {
        let text = format!("ab{}", "—".repeat(60));
        assert!(!text.is_char_boundary(160), "the fixture must straddle");
        let clamped = super::clamp_preview(text.clone());
        let kept = clamped.strip_suffix('…').expect("clamped");
        assert_eq!(kept.len(), 158, "2 ASCII + 52 whole em-dashes");
        assert_eq!(kept, &text[..158]);
    }

    // The same for a 4-byte (astral) character: boundaries 2 + 4k, cut at 158
    // (2 ASCII + 39 whole emoji).
    #[test]
    fn the_preview_clamp_backs_off_a_four_byte_straddle() {
        let text = format!("ab{}", "😀".repeat(50));
        assert!(!text.is_char_boundary(160), "the fixture must straddle");
        let clamped = super::clamp_preview(text.clone());
        let kept = clamped.strip_suffix('…').expect("clamped");
        assert_eq!(kept.len(), 158, "2 ASCII + 39 whole emoji");
        assert_eq!(kept, &text[..158]);
    }

    // When byte 160 IS a boundary, the clamp cuts exactly there…
    #[test]
    fn the_preview_clamp_cuts_exactly_on_a_boundary() {
        let text = "a".repeat(200);
        let clamped = super::clamp_preview(text.clone());
        let kept = clamped.strip_suffix('…').expect("clamped");
        assert_eq!(kept.len(), 160);
        assert_eq!(kept, &text[..160]);
    }

    // …and a preview inside the budget passes through untouched — multi-byte
    // text included, no ellipsis.
    #[test]
    fn a_preview_inside_the_budget_is_untouched() {
        let text = "short — with a dash".to_string();
        assert_eq!(super::clamp_preview(text.clone()), text);
    }

    // E9: a parameter's `context` clause renders in the hovered signature.
    #[test]
    fn hover_renders_a_parameters_context_clause() {
        let hover = hover_at_cursor(
            "import std::reactive::{ owner_scope, Owner };\n\nfun with_o|wner(body: (|| void) context owner_scope) {\n\tlet _b = body;\n}\n\nfun main() {}\n",
        )
        .expect("hover on the declaration");
        assert!(hover.contains("context owner_scope"), "{hover}");
    }

    // std is documented with `///` (user decision): hovering a std function
    // from user code surfaces its doc line, read cross-file from the std
    // source.
    #[test]
    fn hover_surfaces_std_docs_cross_file() {
        let hover = hover_at_cursor(
            "import std::time::{ now, Instant };\n\nfun main() {\n\tlet started = no|w();\n}\n",
        )
        .expect("hover on the std function");
        assert!(hover.contains("The current moment, typed."), "{hover}");
    }

    // `///` is the doc syntax — a plain `//` block is an implementation note
    // and must NOT surface (user decision, 2026-07-16).
    #[test]
    fn hover_ignores_plain_comment_blocks() {
        let hover = hover_at_cursor(
            "import std::print;\n\n// An internal note, not docs.\nfun bad|ge(count: i32): str {\n\t\"b\"\n}\n\nfun main() {\n\tlet _b = badge(1);\n\tprint(\"x\");\n}\n",
        )
        .expect("hover on the declaration");
        assert!(
            !hover.contains("An internal note"),
            "plain `//` must not surface: {hover}"
        );
        assert!(hover.contains("fun badge(count: i32): str"), "{hover}");
    }

    // E9: struct hovers show the declaration block with fields; enum hovers
    // show variants with payloads.
    #[test]
    fn hover_shows_struct_fields_and_enum_variants() {
        let hover = hover_at_cursor(
            "import std::print;\n\nstruct Point { x: i32, name: str }\n\nfun main() {\n\tlet p = Po|int { x = 1, name = \"a\" };\n\tprint(p.name);\n}\n",
        )
        .expect("hover on the constructor");
        assert!(
            hover.contains("```vilan\nstruct Point {\n\tx: i32,\n\tname: str,\n}\n```"),
            "{hover}"
        );
        let hover = hover_at_cursor(
            "import std::print;\n\nenum Shape {\n\tDot,\n\tBox2(i32, i32),\n}\n\nfun main() {\n\tlet s = Sha|pe::Dot;\n\tmatch s {\n\t\tShape::Dot => print(\"dot\"),\n\t\tShape::Box2(let _w, let _h) => print(\"box\"),\n\t}\n}\n",
        )
        .expect("hover on the enum reference");
        assert!(
            hover.contains("Dot,") && hover.contains("Box2(i32, i32),"),
            "{hover}"
        );
    }

    // E9: a std function's docs come from its source file on disk (the
    // non-entry read path) alongside the signature.
    #[test]
    fn hover_reads_imported_declarations_from_their_files() {
        let hover =
            hover_at_cursor("import std::fs::stat;\n\nfun main() {\n\tlet _s = st|at(\"x\");\n}\n")
                .expect("hover on the std call");
        assert!(
            hover.contains("stat(") && hover.contains("```vilan"),
            "{hover}"
        );
    }

    // Colorless functions hover exactly as before — no requirement line.
    #[test]
    fn hover_stays_clean_on_a_colorless_function() {
        let hover = hover_at_cursor(
            "import std::print;\n\nfun greet() {\n\tprint(\"hi\");\n}\n\nfun main() {\n\tgre|et();\n}\n",
        );
        assert!(
            hover
                .as_deref()
                .is_none_or(|hover| !hover.contains("requires")),
            "{hover:?}"
        );
    }

    /// The completion labels offered at the cursor marked `|` in `src`.
    fn completions_at_cursor(src: &str) -> Vec<String> {
        completion_items_at_cursor(src)
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    /// [`completions_at_cursor`] with an explicit cursor marker — the element
    /// pins carry closure literals, whose `|` the default marker would claim.
    fn completions_at_marker(src: &str, marker: char) -> Vec<String> {
        let offset = src
            .find(marker)
            .unwrap_or_else(|| panic!("test source needs a `{marker}` cursor marker"));
        let text = src.replace(marker, "");
        let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
        document
            .completion(offset)
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    /// The full completion candidates offered at the `|` cursor in `src` —
    /// carrying `detail`, `documentation`, and `call_parameters` (WO-3).
    fn completion_items_at_cursor(src: &str) -> Vec<Completion> {
        let offset = src
            .find('|')
            .expect("test source needs a `|` cursor marker");
        let text = src.replace('|', "");
        let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
        document.completion(offset)
    }

    /// The one candidate labelled `label` at the `|` cursor in `src` (the pins
    /// probe a specific function/method/keyword by name).
    fn completion_named(src: &str, label: &str) -> Completion {
        completion_items_at_cursor(src)
            .into_iter()
            .find(|completion| completion.label == label)
            .unwrap_or_else(|| panic!("no `{label}` completion offered"))
    }

    /// The completion labels at the `|` cursor in the FIRST of `files`, with the
    /// whole set written to a real package directory on disk — what
    /// [`completion_items_at_cursor`] cannot give, since `import pkg::…` needs
    /// siblings to find and a sibling module needs a file.
    fn workspace_completions_at_cursor(files: &[(&str, &str)]) -> Vec<String> {
        let (entry_name, entry_source) = files[0];
        let offset = entry_source
            .find('|')
            .expect("test source needs a `|` cursor marker");
        let entry_text = entry_source.replace('|', "");
        let mut written: Vec<(&str, &str)> = vec![(entry_name, &entry_text)];
        written.extend_from_slice(&files[1..]);
        let (directory, document) = analyze_workspace(&written);
        let labels = document
            .completion(offset)
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        let _ = std::fs::remove_dir_all(&directory);
        labels
    }

    #[test]
    fn lifted_member_completion_offers_the_element() {
        let labels = completions_at_cursor(
            "import std::option::Option::{ self, Some, None };\n\
             struct Profile { name: str, age: i32 }\n\
             impl Profile { fun greeting(self): str { self.name } }\n\
             fun find(): Option<Profile> { None }\n\
             fun main() {\n\tlet p: Option<Profile> = find();\n\tp?.|\n}\n",
        );
        assert!(labels.contains(&"name".to_string()), "fields: {labels:?}");
        assert!(
            labels.contains(&"greeting".to_string()),
            "methods: {labels:?}"
        );
        assert!(
            !labels.contains(&"unwrap_or".to_string()),
            "the ELEMENT's members, not Option's: {labels:?}"
        );
    }

    #[test]
    fn member_completion_lists_fields_and_methods() {
        let labels = completions_at_cursor(
            "struct Point { x: i32, y: i32 }\n\
             impl Point { fun sum(self): i32 { self.x + self.y } }\n\
             fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tp.|x\n}\n",
        );
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"y".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"sum".to_string()), "methods: {labels:?}");
    }

    #[test]
    fn member_completion_on_incomplete_receiver() {
        // The realistic moment: `p.` typed with nothing after it yet.
        let labels = completions_at_cursor(
            "struct Point { x: i32, y: i32 }\n\
             fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tp.|\n}\n",
        );
        assert!(
            labels.contains(&"x".to_string()),
            "incomplete member: {labels:?}"
        );
    }

    // --- E66: a call receiver (editing-dx.md §18) ---------------------------
    //
    // The shapes below all end in a value the analyzer types on DEMAND and
    // records nothing for, so none of them could resolve through `expr_types`
    // the way a field or an index does. The name-receiver case they contrast
    // with is pinned by `member_completion_lists_fields_and_methods` and
    // `member_completion_on_incomplete_receiver` above.

    /// The prelude the call-receiver pins share: a nominal with a field and a
    /// method, a free function returning it, a method returning it, and an
    /// `Option` producer for the lifted shape.
    const CALL_RECEIVER_PRELUDE: &str = "import std::option::Option::{ self, Some, None };\n\
         struct Point { x: i32, y: i32 }\n\
         impl Point { fun twin(self): Point { self } }\n\
         fun make(): Point { Point { x = 1, y = 2 } }\n\
         fun find(): Option<Point> { None }\n\
         fun echo(p: Point): Point { p }\n";

    fn call_receiver_completions(body: &str) -> Vec<String> {
        completions_at_cursor(&format!("{CALL_RECEIVER_PRELUDE}fun main() {{\n{body}}}\n"))
    }

    // E66: the owner's shape — a `.` typed straight after a call's closing
    // paren. The receiver is the call's RESULT, never the callee.
    #[test]
    fn member_completion_on_a_call_receiver() {
        let labels = call_receiver_completions("\tmake().|\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"twin".to_string()), "methods: {labels:?}");
        assert!(
            !labels.contains(&"make".to_string()),
            "the RESULT's members, not the callee: {labels:?}"
        );
    }

    // E66: a chained call — the receiver is itself a call on a call.
    #[test]
    fn member_completion_on_a_chained_call_receiver() {
        let labels = call_receiver_completions("\tmake().twin().|\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"twin".to_string()), "methods: {labels:?}");
    }

    // E66: a method call on a bound name — the receiver resolves through the
    // METHOD's return type, not the binding's.
    #[test]
    fn member_completion_on_a_method_call_receiver() {
        let labels = call_receiver_completions("\tlet p = make();\n\tp.twin().|\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
    }

    // E66: a call in ARGUMENT position, the trailing `)` and `;` still to come
    // — the shape the playground was actually being typed in.
    #[test]
    fn member_completion_on_a_call_in_argument_position() {
        let labels = call_receiver_completions("\tlet _q = echo(make().|);\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"twin".to_string()), "methods: {labels:?}");
    }

    // E66: a `?.`-lifted call offers the ELEMENT's members, exactly as the
    // lifted NAME receiver does (`lifted_member_completion_offers_the_element`).
    #[test]
    fn lifted_member_completion_on_a_call_receiver() {
        let labels = call_receiver_completions("\tfind()?.|\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"twin".to_string()), "methods: {labels:?}");
        assert!(
            !labels.contains(&"unwrap_or".to_string()),
            "the ELEMENT's members, not Option's: {labels:?}"
        );
    }

    // E66: a block's value is a call — the walk through the trailing
    // expression, which `expr_types` records nothing for either.
    #[test]
    fn member_completion_on_a_block_receiver() {
        let labels = call_receiver_completions("\t{ make() }.|\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
    }

    // E66: a constructor call still answers with the constructed type — the
    // shape `hover_label` already covered, kept green by the fallback.
    #[test]
    fn member_completion_on_a_constructor_call_receiver() {
        let labels = call_receiver_completions("\tSome(1).|\n");
        assert!(
            labels.contains(&"unwrap_or".to_string()),
            "Option's members: {labels:?}"
        );
    }

    // E66, the field case verbatim (vilan-playground/todo/src/client.vl:42):
    // a generated client method's `Result<…>` result, reached inside a closure
    // inside an element's `on:submit(…)` attribute.
    #[test]
    fn member_completion_on_a_call_inside_an_element_attribute_closure() {
        let labels = completions_at_marker(
            "import std::print;\n\
             import std::reactive::Signal;\n\
             import std::result::Result;\n\
             import std::ui::view;\n\
             struct Note { id: i32, text: str }\n\
             struct NotesClient { }\n\
             impl NotesClient {\n\
             \tfun add(self, name: str): Result<Note, str> { Result::Ok(Note { id = 1, text = name }) }\n\
             }\n\
             fun app(client: NotesClient, note_name: Signal<str>) {\n\
             \t<form on:submit(|event| { print(client.add(note_name.get()).~); })></form>\n\
             }\n",
            '~',
        );
        assert!(
            labels.contains(&"is_ok".to_string()) && labels.contains(&"unwrap".to_string()),
            "the client method's `Result` members: {labels:?}"
        );
    }

    // --- kolt.local 001: the cursor-context classifier, its open faces ------
    //
    // One recurring class: completion decides what to offer from the cursor's
    // surroundings, and that decision is blind to the TRIVIA around the dot.
    // E66 and E67 above fixed two earlier faces of the same classifier; the
    // item asks for one cursor-context model rather than more symptom patches,
    // so each face is pinned by name and the general fix un-ignores them
    // together.
    //
    // The prelude is E66's on purpose: the same `Point` (field `x`, method
    // `twin`) the fixed faces resolve against, so a red here is the CLASSIFIER
    // and not the type resolution E66 already closed.
    //
    // Note the class cuts BOTH ways, which is the item's real point — the same
    // blindness offers nothing where it should offer members, and offers the
    // whole scope where it should offer nothing.

    // (a) The `.` starts the next line — the ordinary way a long method chain
    // is written. Observed: no candidates at all.
    #[test]
    fn member_completion_fires_when_the_dot_starts_the_next_line() {
        let labels = call_receiver_completions("\tlet p = make();\n\tp\n\t\t.|\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"twin".to_string()), "methods: {labels:?}");
    }

    // (a, second shape) The same trivia blindness on ONE line: a space between
    // the receiver and the dot. Observed: the classifier does not read this as
    // a member position at all and offers the entire scope — 80 candidates,
    // every type and keyword in it — where it should offer three members. Found
    // while pinning the face above; it is the same fault, so it rides the same
    // fix.
    #[test]
    fn member_completion_fires_when_a_space_precedes_the_dot() {
        let labels = call_receiver_completions("\tlet p = make();\n\tp . |\n");
        assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
        assert!(labels.contains(&"twin".to_string()), "methods: {labels:?}");
        assert!(
            !labels.contains(&"str".to_string()),
            "a member position offers members, not the scope: {labels:?}"
        );
    }

    // (b) Between two dots (`a.|.b`). The item lists this as a third open face;
    // it does NOT reproduce — every shape tried (a call receiver, a trailing
    // dot, a field or a method after the cursor, a partial name before it)
    // resolves the members correctly. Pinned GREEN rather than dropped, so the
    // face is covered by name and a regression is caught: what follows the
    // cursor must not change what the cursor IS.
    #[test]
    fn member_completion_fires_between_two_dots() {
        for body in [
            "\tlet p = make();\n\tp.|.twin();\n",
            "\tlet p = make();\n\tp.|.x;\n",
            "\tmake().|.twin();\n",
        ] {
            let labels = call_receiver_completions(body);
            assert!(labels.contains(&"x".to_string()), "fields: {labels:?}");
            assert!(labels.contains(&"twin".to_string()), "methods: {labels:?}");
        }
    }

    // (c) The same classifier failing the other way: inside a string body there
    // is no code position, so there is nothing to offer. Observed: the whole
    // scope, functions included.
    #[test]
    fn no_completion_inside_a_string_body() {
        let labels = call_receiver_completions("\tlet caption = \"make |\";\n");
        for function in ["make", "echo", "find", "twin"] {
            assert!(
                !labels.contains(&function.to_string()),
                "`{function}` is code, and the cursor is inside a string: {labels:?}"
            );
        }
    }

    // 001's context matrix. The item asks for one cursor-context model and a
    // matrix that pins each class, so that the next regression is caught by the
    // class it belongs to rather than by whichever face happens to be reported.
    // Three rows, one per class of the taxonomy, each asserting what the class
    // MEANS rather than which branch answered it.

    // A member position is a member position whatever TRIVIA surrounds the dot
    // and whatever FOLLOWS the cursor. Every row here is the same position.
    #[test]
    fn the_context_matrix_reads_every_member_position() {
        for body in [
            "\tlet p = make();\n\tp.|\n",
            "\tlet p = make();\n\tp .|\n",
            "\tlet p = make();\n\tp. |\n",
            "\tlet p = make();\n\tp . |\n",
            "\tlet p = make();\n\tp\n\t\t.|\n",
            "\tlet p = make();\n\tp\t\t.  |\n",
            "\tlet p = make();\n\tp // the point\n\t\t.|\n",
            "\tlet p = make();\n\tp.|.twin();\n",
            "\tlet p = make();\n\tp.|x;\n",
            "\tmake()\n\t\t.|\n",
            "\tmake() .|\n",
        ] {
            let labels = call_receiver_completions(body);
            assert!(
                labels.contains(&"x".to_string()) && labels.contains(&"twin".to_string()),
                "a member position offers the receiver's members ({body:?}): {labels:?}"
            );
            assert!(
                !labels.contains(&"str".to_string()) && !labels.contains(&"echo".to_string()),
                "…and nothing from scope ({body:?}): {labels:?}"
            );
        }
    }

    // Text is not code. A string's body and a comment are the same answer, and
    // the answer is nothing at all — no scope, no keywords, no snippets.
    #[test]
    fn the_context_matrix_offers_nothing_where_there_is_no_code() {
        for body in [
            "\tlet caption = \"make |\";\n",
            "\tlet caption = \"|\";\n",
            "\tlet caption = \"a.| b\";\n",
            "\tlet caption = i\"make | now\";\n",
            "\t// make |\n",
            "\tlet p = make(); // make |\n",
            "\tlet p = make();\n\tp.twin(); // p.|\n",
        ] {
            let labels = call_receiver_completions(body);
            assert!(
                labels.is_empty(),
                "there is no code position here ({body:?}): {labels:?}"
            );
        }
    }

    // The boundaries the two rows above must not swallow. A cursor ON a
    // delimiter is code; an interpolation HOLE is code inside a literal; a `//`
    // inside a string opens no comment; and an ordinary expression position
    // still offers everything in scope.
    #[test]
    fn the_context_matrix_keeps_an_expression_position() {
        for body in [
            "\tma|\n",
            "\tlet caption = \"hi\";\n\tma|\n",
            "\tlet caption = i\"now {ma|}\";\n",
            "\tlet caption = \"http://example.com\"; ma|\n",
            "\tlet caption = \"hi\"|;\n",
            "\tlet p = make(); // a note\n\tma|\n",
        ] {
            let labels = call_receiver_completions(body);
            assert!(
                labels.contains(&"make".to_string()),
                "an expression position offers the scope ({body:?}): {labels:?}"
            );
        }
    }

    // --- kolt.local 033: the inherited trait defaults ----------------------
    //
    // Member RESOLUTION, not 001's cursor CONTEXT: the cursor is read
    // correctly and the answer is short. `push_methods` gathered only
    // `implementation.declarations` — what an impl block itself declares — so
    // every method a trait provides with a DEFAULT BODY was invisible on every
    // implementing type.
    //
    // The rule the gatherer now mirrors is the analyzer's own
    // (`inherited_default_candidates`): a trait's default-bodied instance
    // methods are inherited onto the concrete surface, `[trait_only]` ones are
    // not, supertraits count, and the impl's own declaration wins the name.

    /// A trait with one required member and one default-bodied member, an impl
    /// that takes the default, and an impl that OVERRIDES it — the four pins
    /// below all read this one program.
    const TRAIT_DEFAULT_PRELUDE: &str = "trait Greeter {\n\
         \tfun name(self): str;\n\
         \tfun greet(self): str { self.name() }\n\
         }\n\
         struct Polite { }\n\
         impl Polite with Greeter { fun name(self): str { \"polite\" } }\n\
         struct Loud { }\n\
         impl Loud with Greeter {\n\
         \tfun name(self): str { \"loud\" }\n\
         \tfun greet(self): str { \"LOUD\" }\n\
         }\n";

    // 033, the owner's shape: `list.iter().` offered `next` and nothing else —
    // one method of fifteen, because `impl ListIterator<type T> with
    // Iterator<T>` declares exactly `next` and the other fourteen are
    // `trait Iterator<T>`'s defaults.
    #[test]
    fn member_completion_offers_a_traits_inherited_defaults() {
        let labels = completions_at_cursor(
            "fun main() {\n\tlet xs: List<i32> = List::new();\n\txs.iter().|\n}\n",
        );
        for adapter in [
            "map",
            "filter",
            "take",
            "skip",
            "enumerate",
            "zip",
            "chain",
            "to_list",
            "fold",
            "for_each",
            "count",
            "any",
            "all",
            "rev",
        ] {
            assert!(
                labels.contains(&adapter.to_string()),
                "`{adapter}` is an inherited `Iterator` default: {labels:?}"
            );
        }
    }

    // 033: the required member is NOT a default, so it reaches the list the way
    // it always did — through the impl that must declare it. The contrast is the
    // point: the fix adds the trait's defaults without disturbing this.
    #[test]
    fn member_completion_still_offers_a_requirement_from_its_impl() {
        let labels = completions_at_cursor(
            "fun main() {\n\tlet xs: List<i32> = List::new();\n\txs.iter().|\n}\n",
        );
        assert!(
            labels.contains(&"next".to_string()),
            "`next` has no default body and comes from the impl: {labels:?}"
        );
        let labels = completions_at_cursor(&format!(
            "{TRAIT_DEFAULT_PRELUDE}fun main() {{\n\tlet p = Polite {{ }};\n\tp.|\n}}\n"
        ));
        assert_eq!(
            labels.iter().filter(|label| *label == "name").count(),
            1,
            "the requirement, once, from the impl that declares it: {labels:?}"
        );
    }

    // 033: `Ord` is the std case that is not an iterator — a type implementing
    // it offers `min`/`max`/`clamp`, and through the SUPERTRAITS (`Ord with Eq
    // + PartialOrd`) the comparisons a reader expects to find with them.
    #[test]
    fn member_completion_offers_the_ord_defaults_and_its_supertraits() {
        let labels = completions_at_cursor(
            "import std::compare::{ Eq, Ord, PartialEq, PartialOrd };\n\
             struct Version { major: i32 }\n\
             impl Version with PartialEq {\n\
             \tfun eq(self, b: Version): bool { self.major == b.major }\n\
             }\n\
             impl Version with Eq { }\n\
             impl Version with PartialOrd {\n\
             \tfun compare(self, b: Version): i32 { self.major - b.major }\n\
             }\n\
             impl Version with Ord { }\n\
             fun main() {\n\tlet v = Version { major = 1 };\n\tv.|\n}\n",
        );
        for default in ["min", "max", "clamp"] {
            assert!(
                labels.contains(&default.to_string()),
                "`{default}` is an `Ord` default: {labels:?}"
            );
        }
        for inherited in ["ne", "lt", "le", "gt", "ge"] {
            assert!(
                labels.contains(&inherited.to_string()),
                "`{inherited}` reaches `Ord` through a supertrait: {labels:?}"
            );
        }
    }

    // 033: an impl that OVERRIDES a default offers that member exactly ONCE —
    // the naive union offers it twice, from the impl and from the trait.
    #[test]
    fn member_completion_offers_an_overridden_default_exactly_once() {
        let overriding = completions_at_cursor(&format!(
            "{TRAIT_DEFAULT_PRELUDE}fun main() {{\n\tlet l = Loud {{ }};\n\tl.|\n}}\n"
        ));
        assert_eq!(
            overriding.iter().filter(|label| *label == "greet").count(),
            1,
            "the override, once: {overriding:?}"
        );
        let inheriting = completions_at_cursor(&format!(
            "{TRAIT_DEFAULT_PRELUDE}fun main() {{\n\tlet p = Polite {{ }};\n\tp.|\n}}\n"
        ));
        assert_eq!(
            inheriting.iter().filter(|label| *label == "greet").count(),
            1,
            "the inherited default, once: {inheriting:?}"
        );
    }

    // 033's companion finding, pinned so it stays a finding. Hover and
    // go-to-definition do NOT share completion's blind spot, and the reason is
    // structural rather than lucky: they read the target the ANALYZER already
    // resolved at the call site (`wire_method_call` records the TRAIT's own
    // declaration id behind the call, whichever tier resolved it), so an
    // inherited default hovers as the trait's signature and jumps to the
    // trait's default body. Completion is the one surface that cannot borrow
    // that answer, because it must speak for a member not yet typed — which is
    // why it was the only surface with the hole.
    #[test]
    fn hover_and_definition_reach_an_inherited_trait_default() {
        let source = format!(
            "{TRAIT_DEFAULT_PRELUDE}fun main() {{\n\tlet p = Polite {{ }};\n\tp.greet();\n}}\n"
        );
        let call = source.rfind("greet").expect("the call site") + 2;
        let document = Document::analyze(&source, &std_root(), Path::new("test.vl"));
        let hover = document
            .hover(call)
            .expect("hovering an inherited default should answer");
        assert!(
            hover.contains("fun greet(self): str"),
            "the trait's own signature: {hover}"
        );
        let (_, span) = document
            .definition(call)
            .expect("go-to-definition on an inherited default should answer");
        assert_eq!(
            source.get(span.into_range()),
            Some("greet"),
            "the name it lands on"
        );
        assert_eq!(
            span.into_range().start,
            source.find("fun greet").expect("the trait's default body") + "fun ".len(),
            "the DEFAULT BODY in the trait, since `impl Polite with Greeter` declares none"
        );
    }

    // --- E67: an element's opening tag (editing-dx.md §18) ------------------

    /// The prelude the element-head pins share.
    const ELEMENT_HEAD_PRELUDE: &str =
        "import std::ui::view;\nimport std::reactive::Signal;\nimport std::print;\n";

    fn element_head_completions(body: &str) -> Vec<String> {
        completions_at_marker(
            &format!("{ELEMENT_HEAD_PRELUDE}fun main() {{\n{body}}}\n"),
            '~',
        )
    }

    // E67: `<div .|>` is the chain's method-completion site — the head lowers
    // to a `view("div")` chain (element-syntax.md §4), so the candidates are
    // the `View` type's own methods.
    #[test]
    fn element_head_dot_offers_the_view_methods() {
        let labels = element_head_completions("\t<div .~></div>\n");
        for method in ["bind_each", "on", "text", "child", "styled"] {
            assert!(
                labels.contains(&method.to_string()),
                "`{method}` is a View method: {labels:?}"
            );
        }
        assert!(
            !labels.iter().any(|label| label.starts_with('.')),
            "the dot is already typed: {labels:?}"
        );
        // A head item is a CHAIN LINK, so the candidates are the View's
        // methods and not its members: a `View` field is not something the
        // desugar can splice into the chain, and offering one here is what an
        // ordinary member completion on the desugared `view("div")` would do.
        for field in ["tag", "attributes"] {
            assert!(
                !labels.contains(&field.to_string()),
                "`{field}` is a View FIELD, not a chain link: {labels:?}"
            );
        }
    }

    // E67: `<div |>` — the undotted head position. The chain form is offered
    // in its own spelling (dot included: undotted `text(…)` is an ATTRIBUTE),
    // the event form as the grammar's `on:`, and nothing that is merely in
    // scope: no bindings, no type names, no keywords, no construct snippets.
    #[test]
    fn element_head_offers_the_head_forms_and_nothing_in_scope() {
        let labels = element_head_completions("\tlet caption = \"hi\";\n\t<div ~></div>\n");
        assert!(
            labels.contains(&".bind_each".to_string()) && labels.contains(&".on".to_string()),
            "the chain form, dot included: {labels:?}"
        );
        assert!(
            labels.contains(&"on:".to_string()),
            "the event form: {labels:?}"
        );
        for wrong in ["caption", "str", "view", "fun", "for … in { }"] {
            assert!(
                !labels.contains(&wrong.to_string()),
                "`{wrong}` may not appear in a head: {labels:?}"
            );
        }
    }

    // E67: a NESTED tag under construction. The unfinished chain link used to
    // flatten its own tag to an error atom and, nested, took the whole
    // statement with it; the head-item recovery keeps both elements alive.
    #[test]
    fn element_head_dot_completes_in_a_nested_element() {
        let labels = element_head_completions("\t<div><span .~></span></div>\n");
        assert!(
            labels.contains(&"bind_value".to_string())
                && !labels.contains(&"attributes".to_string()),
            "the inner tag's View methods: {labels:?}"
        );
        let self_closing = element_head_completions("\t<div><span .~ /></div>\n");
        assert!(
            self_closing.contains(&"bind_value".to_string()),
            "a self-closing inner tag: {self_closing:?}"
        );
    }

    // E67: mid-word (`<div .bi|>`) offers the same list — the editor filters
    // it by the prefix, as everywhere else in completion.
    #[test]
    fn element_head_dot_mid_word_offers_the_view_methods() {
        let labels = element_head_completions("\t<div .bi~></div>\n");
        assert!(
            labels.contains(&"bind_each".to_string())
                && labels.contains(&"bind_value".to_string())
                && !labels.contains(&"attributes".to_string()),
            "{labels:?}"
        );
    }

    // E67, the boundary: a head item's ARGUMENT is ordinary expression ground.
    // The cursor sits inside a closure inside `on:click(…)` — brackets deep,
    // so the head vocabulary does not apply and the receiver's members do.
    #[test]
    fn an_element_head_argument_is_not_head_position() {
        let labels = element_head_completions(
            "\t<div on:click(|| { let s = Signal::new(\"\"); s.~; })></div>\n",
        );
        assert!(
            labels.contains(&"get".to_string()) && labels.contains(&"set".to_string()),
            "the Signal's members: {labels:?}"
        );
        assert!(
            !labels.contains(&"bind_each".to_string()),
            "not the View's: {labels:?}"
        );
    }

    // E67, the negative: a `.` outside any markup is untouched.
    #[test]
    fn a_dot_outside_an_element_still_completes_normally() {
        let labels = element_head_completions("\tlet s = Signal::new(\"\");\n\ts.~\n");
        assert!(
            labels.contains(&"get".to_string()),
            "the Signal's members: {labels:?}"
        );
        assert!(
            !labels.contains(&"bind_each".to_string()),
            "not the View's: {labels:?}"
        );
    }

    // E67, the other negative: an element's CHILD position is expression
    // ground too — `{expr}` holes complete from scope, not from the head.
    #[test]
    fn an_element_child_is_not_head_position() {
        let labels = element_head_completions("\tlet caption = \"hi\";\n\t<div>{capt~}</div>\n");
        assert!(
            labels.contains(&"caption".to_string()),
            "the binding in scope: {labels:?}"
        );
    }

    // WO-3: a function completion in a call position carries its full
    // signature (the same string hover fences), the first paragraph of its
    // `///` doc, and its parameter names (for the call-shaped insertion) — a
    // multi-parameter case, with the second doc paragraph correctly dropped.
    #[test]
    fn function_completion_carries_signature_parameters_and_doc() {
        let add = completion_named(
            "/// Adds two numbers.\n\
             ///\n\
             /// A second paragraph, not shown.\n\
             fun add(a: i32, b: i32): i32 { a + b }\n\
             fun main() {\n\tad|\n}\n",
            "add",
        );
        assert_eq!(
            add.call_parameters,
            Some(vec!["a".to_string(), "b".to_string()]),
            "parameter names for the placeholder insertion"
        );
        let detail = add
            .detail
            .expect("a function completion carries a signature");
        assert!(
            detail.contains("a: i32, b: i32") && detail.contains("): i32"),
            "signature must show the parameter list and return type: {detail:?}"
        );
        assert_eq!(
            add.documentation.as_deref(),
            Some("Adds two numbers."),
            "only the first `///` paragraph"
        );
    }

    // WO-3: a method drops the `self` receiver from the call placeholders (it
    // is supplied by the `value.` receiver, not typed as an argument), while
    // the signature detail still renders `self` in full.
    #[test]
    fn method_completion_skips_self_in_call_parameters() {
        let scale = completion_named(
            "struct Point { x: i32, y: i32 }\n\
             impl Point {\n\tfun scale(self, factor: i32): i32 { self.x * factor }\n}\n\
             fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tp.sc|\n}\n",
            "scale",
        );
        assert_eq!(
            scale.call_parameters,
            Some(vec!["factor".to_string()]),
            "`self` must not be a call placeholder"
        );
        let detail = scale
            .detail
            .expect("a method completion carries a signature");
        assert!(
            detail.contains("self") && detail.contains("factor: i32") && detail.contains("): i32"),
            "the method signature keeps `self`: {detail:?}"
        );
    }

    // WO-3: a zero-parameter callable carries an EMPTY parameter list (distinct
    // from a non-callable's `None`) — the server inserts `name()`.
    #[test]
    fn zero_parameter_function_has_empty_call_parameters() {
        let tick = completion_named("fun tick() { }\nfun main() {\n\tti|\n}\n", "tick");
        assert_eq!(tick.call_parameters, Some(Vec::new()));
    }

    // WO-3 escape hatch: when the callee is already followed by `(` — the user
    // pre-typed the parens, or is retyping a call — the completion inserts a
    // bare name (no duplicated parens), yet still shows the signature.
    #[test]
    fn callee_before_open_paren_suppresses_call_shape() {
        let add = completion_named(
            "fun add(a: i32, b: i32): i32 { a + b }\nfun main() {\n\tadd|(1, 2)\n}\n",
            "add",
        );
        assert_eq!(
            add.call_parameters, None,
            "no parens when `(` already follows"
        );
        assert!(add.detail.is_some(), "the signature still shows");
    }

    // WO-3 escape hatch: inside a `use`/`import` path a callable is being bound
    // into scope, not called, so it inserts a bare name — while the SAME
    // function in expression position keeps its call shape.
    #[test]
    fn import_path_suppresses_call_shape_but_expression_keeps_it() {
        let imported = completion_named(
            "mod geometry {\n\tfun area(w: i32, h: i32): i32 { w * h }\n}\n\
             import geometry::ar|\n",
            "area",
        );
        assert_eq!(
            imported.call_parameters, None,
            "a name in an import path inserts bare"
        );
        let called = completion_named(
            "mod geometry {\n\tfun area(w: i32, h: i32): i32 { w * h }\n}\n\
             fun main() {\n\tlet a = geometry::ar|\n}\n",
            "area",
        );
        assert_eq!(
            called.call_parameters,
            Some(vec!["w".to_string(), "h".to_string()]),
            "the same function in expression position keeps its call shape"
        );
    }

    // WO-3: a type name never grows parens — a struct is not call-shaped
    // regardless of position (its kind, not the cursor, decides).
    #[test]
    fn type_name_completion_never_call_shapes() {
        let point = completion_named(
            "struct Point { x: i32, y: i32 }\nfun main() {\n\tlet p = Poi|\n}\n",
            "Point",
        );
        assert_eq!(point.call_parameters, None, "a struct name inserts bare");
    }

    // WO-3 (the WO-4 finding): the offered keywords are EXACTLY the lexer's set,
    // drawn from the one documented table — no stale hand-list. Guards the two
    // concrete bugs it replaced: `return` (spelled `ret`) is gone, and
    // `const`/`borrows`/`resource`/`macro` are now present.
    #[test]
    fn keyword_completions_are_exactly_the_lexer_keywords() {
        let items = completion_items_at_cursor("fun main() {\n\t|\n}\n");
        let mut offered: Vec<String> = items
            .iter()
            .filter(|completion| matches!(completion.kind, CompletionKind::Keyword))
            .map(|completion| completion.label.clone())
            .collect();
        offered.sort();
        let mut expected: Vec<String> = KEYWORD_DOCS
            .iter()
            .map(|(keyword, _, _)| keyword.to_string())
            .collect();
        expected.sort();
        assert_eq!(
            offered, expected,
            "keyword completions == the documented set"
        );
        // Each offered keyword really lexes to that keyword (offered ⊆ lexer);
        // combined with the documented set == every `keyword_lexeme` arm (pinned
        // by `every_documented_keyword_round_trips_through_the_lexer`), the
        // offered set is exactly the lexer's.
        for keyword in &offered {
            let (tokens, errors) = tokenize(keyword);
            assert!(errors.is_empty(), "{keyword} lexed with errors: {errors:?}");
            assert_eq!(tokens.len(), 1, "{keyword} should lex to one token");
            assert_eq!(keyword_lexeme(&tokens[0].0), Some(keyword.as_str()));
        }
        assert!(
            !offered.iter().any(|keyword| keyword == "return"),
            "`return` is not a vilan keyword — it is `ret`"
        );
        for added in ["const", "borrows", "resource", "macro"] {
            assert!(
                offered.iter().any(|keyword| keyword == added),
                "the `{added}` keyword must be offered (it was missing from the old hand-list)"
            );
        }
    }

    // WO-3: `in_import_path` reads the current line's leading keyword, skipping
    // an `export` prefix, and does not confuse an identifier that merely starts
    // with `import`/`use`.
    #[test]
    fn in_import_path_recognizes_import_and_use_lines() {
        assert!(in_import_path("import std::math::sqrt", 22));
        assert!(in_import_path("use pkg::option::Option", 23));
        assert!(in_import_path("export import pkg::x::y", 23));
        assert!(in_import_path("\tuse a::b", 9));
        assert!(!in_import_path("fun main() { sqrt", 17));
        assert!(
            !in_import_path("imported = 5", 12),
            "a word starting with `import`"
        );
        assert!(!in_import_path("used = 5", 8), "a word starting with `use`");
    }

    // E57: the path split that routes every level of import completion. The
    // partial name under the cursor is never a completed segment — that is what
    // makes `import s|` a HEAD position and `import std::|` a one-segment one —
    // and a brace set splits at its brace exactly as the path splits at `::`.
    #[test]
    fn import_path_segments_are_the_completed_ones() {
        fn at_end(line: &str) -> Option<Vec<&str>> {
            import_path_segments(line, line.len())
        }
        assert_eq!(at_end("import "), Some(vec![]), "the head, nothing typed");
        assert_eq!(at_end("import s"), Some(vec![]), "the head, mid-word");
        assert_eq!(at_end("import std::"), Some(vec!["std"]));
        assert_eq!(at_end("import std::js"), Some(vec!["std"]), "mid-word");
        assert_eq!(at_end("import std::json::"), Some(vec!["std", "json"]));
        assert_eq!(
            at_end("export import pkg::shapes::Point::"),
            Some(vec!["pkg", "shapes", "Point"]),
            "an `export` prefix is skipped, and the path runs as deep as it is typed"
        );
        assert_eq!(
            at_end("import std::json::{ Json, J"),
            Some(vec!["std", "json"]),
            "a brace set is one more member of the namespace before it"
        );
        assert_eq!(
            at_end("import std::{ "),
            Some(vec!["std"]),
            "a brace set directly under an origin"
        );
        assert_eq!(at_end("fun main() { sqrt"), None, "not an import line");
        assert_eq!(
            at_end("import std::{ json::{ pa"),
            None,
            "a nested brace set is a shape this does not read — it guesses at nothing"
        );
    }

    // E14: at a scope position (an open function body) each shape-heavy
    // construct completes as a SNIPPET-kind template carrying its exact
    // tab-stopped body. The bodies are pinned verbatim — house style (tab
    // indent, trailing comma, `i32`) is part of the contract.
    #[test]
    fn construct_snippets_are_offered_at_a_scope_position() {
        let source = "fun main() {\n\t|\n}\n";
        for (label, body) in [
            ("for … in { }", "for ${1:item} in ${2:items} {\n\t$0\n}"),
            ("fun … ( ) { }", "fun ${1:name}(${2}) {\n\t$0\n}"),
            (
                "struct … { }",
                "struct ${1:Name} {\n\t${2:field}: ${3:i32},\n}",
            ),
            (
                "match … { }",
                "match ${1:subject} {\n\t${2:pattern} => $0,\n}",
            ),
        ] {
            let completion = completion_named(source, label);
            assert!(
                matches!(completion.kind, CompletionKind::Snippet),
                "`{label}` should be a snippet"
            );
            let snippet = completion.snippet.expect("a snippet carries its body");
            assert_eq!(snippet.body, body, "`{label}` body");
            // The fallback is the bare keyword (the label's first word).
            assert_eq!(snippet.fallback, label.split(' ').next().unwrap());
        }
    }

    // E14: the snippet is offered ALONGSIDE the bare keyword, not instead of it —
    // typing `for` still surfaces the plain keyword AND the distinctly-labelled
    // template, each with its own kind.
    #[test]
    fn scope_completion_offers_the_bare_keyword_alongside_the_snippet() {
        let items = completion_items_at_cursor("fun main() {\n\t|\n}\n");
        for keyword in ["for", "fun", "struct", "match"] {
            assert!(
                items
                    .iter()
                    .any(|c| c.label == keyword && matches!(c.kind, CompletionKind::Keyword)),
                "the bare `{keyword}` keyword is still offered"
            );
        }
        assert!(
            items
                .iter()
                .any(|c| c.label == "for … in { }" && matches!(c.kind, CompletionKind::Snippet)),
            "and the `for` snippet, distinctly labelled"
        );
    }

    // E14: construct snippets are a scope-position feature — a member list
    // (after `.`) offers none. The list is non-empty (the receiver's fields), so
    // this is a real member completion, not a vacuously empty one.
    #[test]
    fn construct_snippets_are_absent_in_member_completion() {
        let items = completion_items_at_cursor(
            "struct Point { x: i32, y: i32 }\n\
             fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tp.|\n}\n",
        );
        assert!(
            items.iter().any(|c| c.label == "x"),
            "the member list has the receiver's fields: {:?}",
            items.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
        assert!(
            !items
                .iter()
                .any(|c| matches!(c.kind, CompletionKind::Snippet)),
            "member completion offers no construct snippets: {:?}",
            items.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    // E14: an import path (`import st|`) offers no construct snippets. It once
    // reached `scope_completions` and had them dropped by a post-pass; E57
    // routes it to import completion instead, which never produces one. The
    // non-vacuity witness moves with it: the list is the ORIGINS now, not the
    // keywords, because a keyword may not follow `import` either — which is the
    // same argument E14 made about the snippets, carried to its conclusion.
    #[test]
    fn construct_snippets_are_absent_in_import_path() {
        let items = completion_items_at_cursor("import st|\nfun main() {}\n");
        assert!(
            items.iter().any(|c| c.label == "std"),
            "the import-path completion still ran (origins present): {:?}",
            items.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
        assert!(
            !items
                .iter()
                .any(|c| matches!(c.kind, CompletionKind::Snippet)),
            "import path offers no construct snippets: {:?}",
            items.iter().map(|c| &c.label).collect::<Vec<_>>()
        );
    }

    // E14: the snippet table stays a subset of the lexer's keywords, in lockstep
    // with `KEYWORD_DOCS` (WO-4's round-trip guard pattern) — every snippet rides
    // a real keyword that classifies back to itself and carries a doc entry. The
    // four named constructs are pinned in their exact order.
    #[test]
    fn construct_snippet_keywords_are_lexer_keywords() {
        let keywords: Vec<&str> = CONSTRUCT_SNIPPETS.iter().map(|(k, _, _, _)| *k).collect();
        assert_eq!(
            keywords,
            ["for", "fun", "struct", "match"],
            "the four named constructs, in order"
        );
        for (keyword, _label, _detail, _body) in CONSTRUCT_SNIPPETS {
            let (tokens, errors) = tokenize(keyword);
            assert!(errors.is_empty(), "{keyword} lexed with errors: {errors:?}");
            assert_eq!(tokens.len(), 1, "{keyword} should lex to one token");
            assert_eq!(
                keyword_lexeme(&tokens[0].0),
                Some(*keyword),
                "{keyword} must classify back to itself (subset of the lexer)"
            );
            assert!(
                KEYWORD_DOCS
                    .iter()
                    .any(|(documented, _, _)| documented == keyword),
                "{keyword} must have a KEYWORD_DOCS entry (lockstep)"
            );
        }
    }

    /// The shipped example projects must analyze cleanly through the *LSP* path
    /// (`Document::analyze` — project-context + `pkg::` + `std` resolution), not
    /// just the CLI. Guards against a regression where the editor surfaces errors
    /// the CLI doesn't, and pins that the RPC example's cross-file object-stub form
    /// stays diagnostic-free. Reads the real files, so an example edit that breaks
    /// analysis fails here.
    fn assert_example_analyzes_clean(relative: &str) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        let document = Document::analyze(&text, &std_root(), &path);
        let messages: Vec<String> = document
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.msg.clone())
            .collect();
        assert!(
            messages.is_empty(),
            "{relative}: expected no LSP diagnostics, got {messages:#?}"
        );
    }

    #[test]
    fn rpc_example_analyzes_without_diagnostics() {
        // The entry: the generated `[service(Client)]` paradigm over `std::rpc`
        // (the runtime module itself now lives in std).
        assert_example_analyzes_clean("../../vilan/examples/rpc/src/main.vl");
    }

    #[test]
    fn todo_example_analyzes_without_diagnostics() {
        // The realtime app: ONE package with two entries (`[entry.client]` /
        // `[entry.server]`), sharing `[derive(Wire)] Todo` and a generated
        // `[service(TodoClient)]`. Both entries and the non-entry package
        // modules (which have no `main`) must analyze via project context, not
        // be rejected the way a bare `vilan check <file>` would — and each entry
        // must resolve against its OWN target, so the browser entry sees the
        // generated stub while the server-colored bodies beside it stay put.
        assert_example_analyzes_clean("../../vilan/examples/todo/src/server.vl");
        assert_example_analyzes_clean("../../vilan/examples/todo/src/client.vl");
        assert_example_analyzes_clean("../../vilan/examples/todo/src/todos.vl");
        assert_example_analyzes_clean("../../vilan/examples/todo/src/todo.vl");
    }

    #[test]
    fn workspace_library_module_analyzes_without_diagnostics() {
        // The other half of the shape, kept pinned now that the full-stack
        // examples are single-package: a `[library]` member's module inside a
        // `[project]` workspace, with no `main` of its own, reached through the
        // workspace root's project context.
        assert_example_analyzes_clean("../../vilan/examples/fullstack/common/src/lib.vl");
        assert_example_analyzes_clean("../../vilan/examples/fullstack/server/src/main.vl");
    }

    #[test]
    fn span_to_range_conversions_never_panic_on_multibyte_source() {
        // The RPC example's leading comment contains em-dashes (3-byte chars).
        // Converting an entity/symbol span whose byte boundary lands inside one
        // (documentSymbol, go-to-definition, diagnostics) used to panic the server
        // on a non-char-boundary string slice (`line_index.rs`). Drive the whole
        // span→range path the editor exercises on open, on the real file.
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/examples/rpc/src/main.vl");
        let text = std::fs::read_to_string(&path).unwrap();
        let document = Document::analyze(&text, &std_root(), &path);
        for symbol in document.document_symbols() {
            let _ = document.analyzed_range(&symbol.full);
            let _ = document.analyzed_range(&symbol.selection);
        }
        for (start, end, _) in &document.entity_spans {
            let _ = document.analyzed_position(*start);
            let _ = document.analyzed_position(*end);
        }
    }

    #[test]
    fn derive_synthesized_entities_are_excluded_from_the_user_file() {
        // `[derive(Json)] struct User` synthesizes `to_json`/`from_json` impls whose
        // spans are offsets into a *generated template*, not this file. They used to
        // be bundled into the entry's `SourceId(0)` range, so `source_of` reported
        // them as user-file entities and the editor placed them at those bogus
        // offsets — landing inside the leading comment (and, on the em-dash, crashing
        // position conversion). The fix attributes them to `DERIVED_SOURCE`, so they
        // are excluded from `entity_spans`/`document_symbols`. Pin that: no
        // user-file span may begin in the leading comment block, which ends at the
        // file's first `import`.
        //
        // The boundary is found as the first line-initial `import `, NOT a
        // particular one: it used to look for `import std::print`, and when the
        // formatter's canonical import sort reordered that block the proxy landed
        // past two legitimate imports and the pin fired on them.
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/examples/rpc/src/main.vl");
        let text = std::fs::read_to_string(&path).unwrap();
        let first_code = text.find("\nimport ").expect("first import") + 1;
        let document = Document::analyze(&text, &std_root(), &path);

        for (start, _end, id) in &document.entity_spans {
            assert!(
                *start >= first_code,
                "entity {id:?} span starts at {start}, inside the leading comment \
                 (a derive-synthesized entity leaking into the user file)"
            );
        }
        for symbol in document.document_symbols() {
            let start = symbol.selection.into_range().start;
            assert!(
                start >= first_code,
                "symbol {:?} selection starts at {start}, inside the leading comment",
                symbol.name
            );
        }
    }

    // The same leak as `derive_synthesized_entities_are_excluded_from_the_user_file`,
    // one channel over. That fix converted the ENTITY channel (`source_ranges`,
    // hence `entity_spans`/`document_symbols`); `type_references` — the span-keyed
    // channel behind semantic tokens, hover and go-to-definition — kept recording
    // the DERIVING file, because the generated walk never changed
    // `current_source_id`. Every type name in a derive's template therefore
    // arrived as a user-file token at a generated-text offset, painting the
    // leading comment and (worse) swallowing real tokens: `semantic_tokens` drops
    // overlaps, so a bogus wide span starting earlier evicts the genuine ones
    // behind it.
    #[test]
    fn semantic_tokens_exclude_derive_generated_spans() {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/examples/rpc/src/main.vl");
        let text = std::fs::read_to_string(&path).unwrap();
        let first_code = text.find("\nimport ").expect("first import") + 1;
        let document = Document::analyze(&text, &std_root(), &path);

        for (span, kind, _) in document.semantic_tokens() {
            let range = span.into_range();
            assert!(
                range.start >= first_code,
                "a {kind:?} token spans {range:?}, inside the leading comment \
                 (a derive-generated span leaking into the user file)"
            );
        }
        // The hover / go-to-definition half of the same channel.
        let program = document.program.as_ref().expect("the example analyzes");
        for offset in 0..first_code {
            assert!(
                document.type_reference_at(program, offset).is_none(),
                "a type reference covers offset {offset}, inside the leading comment"
            );
        }
    }

    // Every record the editor reads ABOUT this file must index this file's text.
    // `context` clauses (`|| void context owner_scope`) are resolved in
    // `build()`, past the import fixpoint, where the ambient source is the
    // entry's — but a clause's name spans belong to whatever file WROTE it, and
    // `std::reactive` is full of them. Importing it therefore handed the editor
    // references at reactive.vl's offsets, labeled as this file's own. A span
    // past the end of the buffer is that leak's visible half: the language server
    // clamps it, so in a short file it lands as an invisible zero-width token and
    // in a long one it lands on unrelated text (a real one drew over a comment 200
    // lines from anything reactive).
    #[test]
    fn entry_records_index_the_entry_text() {
        let text = "import std::reactive::Signal;\n\nstruct Row {\n\tcell: Signal<i32>,\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let program = document.program.as_ref().expect("the program analyzes");
        for (source, span, _, label) in &program.type_references {
            let range = span.into_range();
            assert!(
                *source != SourceId(0) || range.end <= text.len(),
                "a type reference ({label}) spans {range:?}, past the end of this {}-byte file",
                text.len()
            );
        }
        for (span, kind, _) in document.semantic_tokens() {
            let range = span.into_range();
            assert!(
                range.end <= text.len(),
                "a {kind:?} token spans {range:?}, past the end of this {}-byte file",
                text.len()
            );
        }
    }

    // A written generic type application highlights as its HEAD name plus its
    // arguments, each a name of its own. The reference used to be recorded at the
    // whole `Name<Args>` span, and since `semantic_tokens` drops overlaps, that
    // one token ate every argument's: `Signal<List<str>>` lit up as a single
    // struct and both arguments went dark. Nesting and a closure argument are
    // both here — a closure's parameter and return types are the case the whole
    // span reached furthest over.
    #[test]
    fn a_generic_type_application_tokenizes_its_head_and_arguments() {
        let text = "import std::reactive::Signal;\nimport std::shared::Shared;\n\nstruct Row {\n\tcells: Signal<List<str>>,\n\thook: Shared<|i32| bool>,\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let tokens = document.semantic_tokens();
        // The token covering exactly the `occurrence`-th `name` in the text.
        let token_at = |name: &str, occurrence: usize| -> Option<TokenKind> {
            let mut start = 0;
            let mut position = None;
            for _ in 0..=occurrence {
                position = text[start..].find(name).map(|at| start + at);
                start = position? + 1;
            }
            let at = position?;
            tokens
                .iter()
                .find(|(span, _, _)| {
                    let range = span.into_range();
                    range.start == at && range.end == at + name.len()
                })
                .map(|(_, kind, _)| *kind)
        };
        // The head of each application, name-sized rather than swallowing its
        // arguments.
        assert!(token_at("Signal", 1).is_some(), "Signal head: {tokens:?}");
        assert!(token_at("Shared", 1).is_some(), "Shared head: {tokens:?}");
        // The arguments, no longer eaten: nested nominal, and a closure's
        // parameter and return types.
        assert!(token_at("List", 0).is_some(), "List argument: {tokens:?}");
        // Occurrence 1: the first `str` in this text is the one inside `struct`.
        assert!(token_at("str", 1).is_some(), "str argument: {tokens:?}");
        assert!(token_at("i32", 0).is_some(), "i32 parameter: {tokens:?}");
        assert!(token_at("bool", 0).is_some(), "bool return: {tokens:?}");
    }

    // The invariant the wire format depends on and the leak broke: the classifier
    // only ever produces NAME-sized spans, so every token's text is an identifier.
    // A generated-template offset laid over this file lands mid-word or across
    // punctuation, which no real name does. Held over a program whose derives
    // synthesize a lot (`Wire` alone writes a to/from-wire impl pair).
    #[test]
    fn every_semantic_token_covers_an_identifier() {
        let text = "// A leading comment, wide enough to catch a generated offset.\nimport std::json::Json;\n\n[derive(Json, PartialEq)]\nstruct Point {\n\tx: i32,\n\ty: i32,\n}\n\nfun main() {\n\tlet p = Point { x = 1, y = 2 };\n}\n";
        let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
        let tokens = document.semantic_tokens();
        assert!(!tokens.is_empty(), "the program should produce tokens");
        for (span, kind, _) in &tokens {
            let range = span.into_range();
            let lexeme = text.get(range.clone()).unwrap_or_else(|| {
                panic!(
                    "a {kind:?} token spans {range:?}, which is not a char boundary of this file"
                )
            });
            assert!(
                !lexeme.is_empty()
                    && lexeme
                        .chars()
                        .all(|character| character.is_alphanumeric() || character == '_'),
                "a {kind:?} token spans {range:?} = {lexeme:?}, which is not an identifier"
            );
        }
    }

    #[test]
    fn scope_completion_includes_top_level_and_keywords() {
        let labels = completions_at_cursor(
            "fun helper(): i32 { 42 }\nfun main() {\n\tlet value = hel|\n}\n",
        );
        assert!(
            labels.contains(&"helper".to_string()),
            "top-level: {labels:?}"
        );
        assert!(labels.contains(&"fun".to_string()), "keyword: {labels:?}");
    }

    #[test]
    fn path_completion_lists_enum_variants() {
        let labels = completions_at_cursor(
            "enum Color { Red, Green, Blue }\nfun main() {\n\tlet c = Color::|\n}\n",
        );
        assert!(labels.contains(&"Red".to_string()), "variants: {labels:?}");
        assert!(
            labels.contains(&"Green".to_string()),
            "variants: {labels:?}"
        );
        assert!(labels.contains(&"Blue".to_string()), "variants: {labels:?}");
    }

    const COUNTER: &str = "struct Counter { n: i32 }\n\
         impl Counter {\n\
         \tfun new(): Counter { Counter { n = 0 } }\n\
         \tfun bump(self): i32 { self.n + 1 }\n\
         }\n";

    #[test]
    fn member_completion_excludes_static_methods() {
        // `b.new()` would not type-check (`new` has no `self`), so it must not be
        // offered on `b.` — only `bump` (a `self` method) and the field `n`.
        let labels = completions_at_cursor(&format!(
            "{COUNTER}fun main() {{\n\tlet b = Counter {{ n = 0 }};\n\tb.|\n}}\n"
        ));
        assert!(
            labels.contains(&"bump".to_string()),
            "instance method: {labels:?}"
        );
        assert!(labels.contains(&"n".to_string()), "field: {labels:?}");
        assert!(
            !labels.contains(&"new".to_string()),
            "static excluded: {labels:?}"
        );
    }

    #[test]
    fn path_completion_lists_static_methods_not_instance() {
        let labels = completions_at_cursor(&format!(
            "{COUNTER}fun main() {{\n\tlet c = Counter::|\n}}\n"
        ));
        assert!(
            labels.contains(&"new".to_string()),
            "static method: {labels:?}"
        );
        assert!(
            !labels.contains(&"bump".to_string()),
            "instance excluded: {labels:?}"
        );
    }

    // --- E53: a code-position `Name::` answers from SCOPE, not from every
    // module the process happens to have loaded ---

    // The headline case. `compare.vl` is one of the nine std modules the loader
    // ALWAYS pulls in for the derive prelude, so its `enum Ordering` sits in
    // every program ever analyzed — and matching the left of `::` against
    // `program.enums` by name offered its variants in a file that had never
    // heard of `std::compare`.
    #[test]
    fn code_path_completion_excludes_the_always_loaded_prelude() {
        let labels = completions_at_cursor("fun main() {\n\tlet o = Ordering::|\n}\n");
        assert!(
            !labels.contains(&"Less".to_string()),
            "`std::compare` was never imported: {labels:?}"
        );
        let json = completions_at_cursor("fun main() {\n\tlet k = JsonKind::|\n}\n");
        assert!(
            !json.contains(&"Number".to_string()),
            "`std::json` was never imported: {json:?}"
        );
    }

    // The same exclusion for a type in the user's own package: a sibling module
    // is loaded (the entry imports something else from it) and declares `Color`,
    // but this file never brought `Color` into scope.
    #[test]
    fn code_path_completion_excludes_an_unimported_same_named_type() {
        let labels = workspace_completions_at_cursor(&[
            (
                "main.vl",
                "import pkg::palette::shade;\nfun main() {\n\tlet c = Color::|\n}\n",
            ),
            (
                "palette.vl",
                "enum Color { Red, Green, Blue }\nfun shade(): i32 { 1 }\n",
            ),
        ]);
        assert!(
            !labels.contains(&"Red".to_string()),
            "`Color` is declared in a loaded module but never imported here: {labels:?}"
        );
    }

    // The flip side, and the reason the exclusion is a scope question rather
    // than a "std is off limits" rule: import the very same type and it
    // completes.
    #[test]
    fn code_path_completion_includes_an_imported_type() {
        let labels = completions_at_cursor(
            "import std::compare::Ordering;\nfun main() {\n\tlet o = Ordering::|\n}\n",
        );
        assert!(
            labels.contains(&"Less".to_string()),
            "an imported enum completes: {labels:?}"
        );
        let workspace = workspace_completions_at_cursor(&[
            (
                "main.vl",
                "import pkg::palette::Color;\nfun main() {\n\tlet c = Color::|\n}\n",
            ),
            ("palette.vl", "enum Color { Red, Green, Blue }\n"),
        ]);
        assert!(
            workspace.contains(&"Red".to_string()),
            "an imported package enum completes: {workspace:?}"
        );
    }

    // A type declared in the file being edited is in scope by declaration, and
    // stays so even when the cursor's own statement has not parsed — the case
    // `same_file_namespace` exists for, and the one a naive scope-only rule
    // would have broken.
    #[test]
    fn code_path_completion_survives_an_unparsed_statement() {
        let labels = completions_at_cursor(
            "enum Color { Red, Green, Blue }\nfun main() {\n\tlet c = ((( Color::|\n}\n",
        );
        assert!(
            labels.contains(&"Red".to_string()),
            "a locally-declared enum completes mid-edit: {labels:?}"
        );
    }

    // --- E57: import-path completion ---

    // The head of an import path names an ORIGIN, which is not an entity: no
    // lookup against the program can ever answer it, which is why `import std::`
    // completed nothing at all. It offers the origins, and only those — a
    // keyword, a construct snippet, and a name in scope are all ungrammatical
    // after `import`.
    #[test]
    fn import_head_offers_the_origins_and_nothing_else() {
        let items =
            completion_items_at_cursor("fun helper() {}\nimport |\nfun main() { helper(); }\n");
        let labels: Vec<&str> = items.iter().map(|item| item.label.as_str()).collect();
        assert!(labels.contains(&"std"), "the std origin: {labels:?}");
        assert!(labels.contains(&"pkg"), "the pkg origin: {labels:?}");
        assert!(
            !labels.contains(&"fun"),
            "a keyword may not follow `import`: {labels:?}"
        );
        assert!(
            !labels.contains(&"helper"),
            "a name in scope may not follow `import`: {labels:?}"
        );
        assert!(
            !items
                .iter()
                .any(|item| matches!(item.kind, CompletionKind::Snippet)),
            "no construct snippets: {labels:?}"
        );
    }

    // `import std::` lists the std tree — the embedded/checked-out std the
    // analysis itself resolved, enumerated from its layered roots. Asserted by
    // membership, never as a frozen list: std grows.
    #[test]
    fn import_lists_the_std_modules() {
        let labels = completions_at_cursor("import std::|\nfun main() {}\n");
        for module in ["json", "math", "option", "list"] {
            assert!(
                labels.contains(&module.to_string()),
                "`std::{module}` is a module: {labels:?}"
            );
        }
        // A layer directory is not a path segment: `src/process/fs.vl` is
        // `std::fs`, and `process` is a module in its own right, not a namespace.
        assert!(
            labels.contains(&"fs".to_string()),
            "a layered module lists under its own name: {labels:?}"
        );
        // `lib.vl` is the package SURFACE, not a module of it — and its
        // re-exports are offered right here, under the origin.
        assert!(
            !labels.contains(&"lib".to_string()),
            "`import std::lib` is not a thing: {labels:?}"
        );
        assert!(
            labels.contains(&"print".to_string()),
            "std's `lib.vl` surface is reachable as `std::print`: {labels:?}"
        );
    }

    // `import pkg::` lists the package's OWN source files, by the same names the
    // module loader resolves them under — including the directory form.
    #[test]
    fn import_lists_the_packages_own_modules() {
        let labels = workspace_completions_at_cursor(&[
            ("main.vl", "import pkg::|\nfun main() {}\n"),
            ("palette.vl", "enum Color { Red }\n"),
            ("shapes/lib.vl", "struct Point { x: i32 }\n"),
        ]);
        assert!(
            labels.contains(&"palette".to_string()),
            "a flat sibling module: {labels:?}"
        );
        assert!(
            labels.contains(&"shapes".to_string()),
            "a directory module resolves under its directory's name: {labels:?}"
        );
    }

    // The load-on-demand case, in the shape it actually happens: the buffer is
    // ahead of the analysis (150 ms of debounce), so the program the document
    // holds knows nothing of `std::math` — and the candidates still arrive,
    // because they come from the module file, not from the program.
    #[test]
    fn import_members_load_a_module_the_program_never_did() {
        let mut document = analyze_text("fun main() {}\n");
        assert!(
            !document
                .program
                .as_ref()
                .expect("analyzed")
                .modules
                .values()
                .any(|module| module.name == "random"),
            "`std::random` is outside the always-loaded prelude's closure"
        );
        let typed = "import std::random::\nfun main() {}\n";
        document.set_text(typed);
        let labels: Vec<String> = document
            .completion(typed.find('\n').expect("end of the import line"))
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        assert!(
            labels.contains(&"range".to_string()) && labels.contains(&"Random".to_string()),
            "`std::random`'s members, loaded on demand: {labels:?}"
        );
    }

    // A brace set completes at the level of the path before it — one more member
    // of the same namespace — which falls out of splitting the path at the brace
    // exactly as it splits at the final `::`.
    #[test]
    fn import_completes_inside_a_brace_set() {
        let labels = completions_at_cursor("import std::compare::{ Ordering, |\nfun main() {}\n");
        assert!(
            labels.contains(&"PartialEq".to_string()),
            "a further member of the same module: {labels:?}"
        );
    }

    // Past a module, an enum is the one namespace an import descends into —
    // `resolve_import` descends through modules and enums and nothing else.
    #[test]
    fn import_descends_into_an_enums_variants() {
        let labels = completions_at_cursor("import std::compare::Ordering::|\nfun main() {}\n");
        assert!(
            labels.contains(&"Less".to_string()),
            "an enum's variants are importable: {labels:?}"
        );
    }

    // A module that does not resolve answers EMPTY. The request is on the
    // editor's critical path: it degrades, it never errors, and it never panics.
    #[test]
    fn an_import_of_a_module_that_is_not_there_is_empty() {
        assert!(
            completions_at_cursor("import std::no_such_module::|\nfun main() {}\n").is_empty(),
            "a module that fails to load offers nothing"
        );
        assert!(
            completions_at_cursor("import no_such_origin::|\nfun main() {}\n").is_empty(),
            "a head that names no origin and no loaded namespace offers nothing"
        );
    }

    // The routing is one-way: import completion answers import lines, and a
    // plain code position is untouched — no origins leak into it.
    #[test]
    fn origins_do_not_leak_into_a_code_position() {
        let items = completion_items_at_cursor("fun main() {\n\tlet x = |\n}\n");
        assert!(
            items
                .iter()
                .any(|item| matches!(item.kind, CompletionKind::Keyword)),
            "a code position still offers keywords"
        );
        assert!(
            items
                .iter()
                .any(|item| matches!(item.kind, CompletionKind::Snippet)),
            "a code position still offers construct snippets"
        );
    }

    // --- E8: editor support for macros ---

    // Hover on a macro attribute shows the macro's signature; definition
    // jumps to the `macro fun` (same file here).
    #[test]
    fn macro_attribute_hover_and_definition() {
        let source = "macro fun derive_tag(item: Item): Source {\n\timport macro_std::source;\n\timport macro_std::meta::{ Item, Source };\n\tsource(\"\")\n}\n\n[derive_tag]\nstruct Point {\n\tx: i32,\n}\n\nfun main() {}\n\nmain();\n";
        let (_dir, document) = analyze_workspace(&[("main.vl", source)]);
        // The attribute site is the SECOND occurrence of the name.
        let definition_at = source.find("derive_tag").unwrap();
        let use_at = source[definition_at + 1..].find("derive_tag").unwrap() + definition_at + 1;
        let hover = document
            .hover(use_at + 2)
            .expect("hover on the attribute name");
        assert!(
            hover.contains("macro fun derive_tag(item: Item): Source"),
            "hover should show the signature, got: {hover}"
        );
        let (source_id, span) = document
            .definition(use_at + 2)
            .expect("definition on the attribute name");
        assert_eq!(source_id, vilan_core::analyzer::SourceId(0));
        assert_eq!(
            span.into_range().start,
            definition_at,
            "definition should land on the macro fun's name"
        );
    }

    // A prelude derive navigates CROSS-FILE into std (compare.vl).
    #[test]
    fn prelude_derive_definition_reaches_std() {
        let source =
            "[derive(PartialEq)]\nstruct Point {\n\tx: i32,\n}\n\nfun main() {}\n\nmain();\n";
        let (_dir, document) = analyze_workspace(&[("main.vl", source)]);
        let use_at = source.find("PartialEq").unwrap();
        let hover = document
            .hover(use_at + 2)
            .expect("hover on the derive name");
        assert!(
            hover.contains("macro fun PartialEq(item: Item): Source"),
            "hover should show the prelude macro's signature, got: {hover}"
        );
        let (source_id, _span) = document
            .definition(use_at + 2)
            .expect("definition on the derive name");
        assert_ne!(
            source_id,
            vilan_core::analyzer::SourceId(0),
            "the definition lives in std's compare.vl, not the entry"
        );
    }

    // --- WO-2: Organize Imports (sort + conservative prune) ----------------
    //
    // A helper package for the mechanics pins: two free functions and a struct,
    // so imports resolve without depending on std's exact surface. The
    // derive-survival pin needs a real derive, so it uses `std::json`.
    const ORGANIZE_HELPER: &str = "fun alpha() {}\nfun beta() {}\nstruct Widget {}\n";

    /// Applies the Organize Imports edits to the document's text (back-to-front,
    /// so earlier offsets stay valid), or `None` when the action offers no edit.
    fn organized(document: &Document) -> Option<String> {
        let mut edits = document.organize_import_edits();
        if edits.is_empty() {
            return None;
        }
        edits.sort_by_key(|(span, _)| std::cmp::Reverse(span.into_range().start));
        let mut text = document.text.clone();
        for (span, replacement) in edits {
            text.replace_range(span.into_range(), &replacement);
        }
        Some(text)
    }

    // kolt.local 004, the OVER-pruning half: a module import whose only
    // contribution is an `impl` the file calls a method from. The module's name
    // is never written, so a syntactic notion of "used" saw nothing and removed
    // the import — and the file stopped building.
    #[test]
    fn organize_keeps_a_module_import_whose_impl_method_is_used() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper;\n\nfun main(): i32 {\n\tlet n = 2;\n\tn.doubled()\n}\n",
            ),
            (
                "helper.vl",
                "impl i32 {\n\tfun doubled(self): i32 {\n\t\tself * 2\n\t}\n}\n",
            ),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "the pin needs a green program"
        );
        assert_eq!(
            organized(&document),
            None,
            "the import is what brings `doubled` — organize must leave it alone",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // kolt.local 004, the UNDER-pruning half: an import path's own segments
    // resolve to the definitions its leaves bind, so counting them as usage let
    // an import justify its own existence. `Result::{ self }` could never be
    // pruned, however unused, because the `Result` segment in the path counted
    // as a use of `Result`.
    #[test]
    fn organize_prunes_an_import_nothing_but_its_own_path_references() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::result::Result::{ self, Err, Ok };\n\nfun main(): i32 {\n\t1\n}\n",
        )]);
        assert!(
            document.diagnostics.is_empty(),
            "the pin needs a green program"
        );
        let organized_text = organized(&document).expect("the whole import is unused");
        assert!(
            !organized_text.contains("import"),
            "nothing in the file uses Result, Ok or Err, but the import survived:\n{organized_text}",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One build-preservation case: a workspace whose entry file has imports
    /// organize must act on. `entry` is the first file.
    struct OrganizeCase {
        label: &'static str,
        files: &'static [(&'static str, &'static str)],
    }

    // THE pin kolt.local 004 asked for: an organize pass must never break a
    // green build. Each case starts green, organize is required to actually
    // change the text (a no-op case would pass vacuously and prove nothing), and
    // the organized text must analyze green again. This is the assertion that
    // makes the usage model's two directions safe at once — over-pruning shows
    // up here as a diagnostic, under-pruning as an unchanged text.
    #[test]
    fn organizing_never_breaks_a_green_build() {
        let cases = [
            OrganizeCase {
                label: "a module import reached only through an impl method",
                files: &[
                    (
                        "main.vl",
                        "import pkg::zebra;\nimport pkg::helper;\n\nfun main(): i32 {\n\tlet n = 2;\n\tn.doubled() + zebra::three()\n}\n",
                    ),
                    (
                        "helper.vl",
                        "impl i32 {\n\tfun doubled(self): i32 {\n\t\tself * 2\n\t}\n}\n",
                    ),
                    ("zebra.vl", "fun three(): i32 {\n\t3\n}\n"),
                ],
            },
            OrganizeCase {
                label: "a genuinely unused module import beside a used one",
                files: &[
                    (
                        "main.vl",
                        "import pkg::unused;\nimport pkg::zebra;\n\nfun main(): i32 {\n\tzebra::three()\n}\n",
                    ),
                    ("zebra.vl", "fun three(): i32 {\n\t3\n}\n"),
                    ("unused.vl", "fun nothing(): i32 {\n\t0\n}\n"),
                ],
            },
            OrganizeCase {
                label: "a brace set with a used and an unused leaf",
                files: &[
                    (
                        "main.vl",
                        "import pkg::shapes::{ Square, Circle };\n\nfun main(): i32 {\n\tlet c = Circle { radius = 2 };\n\tc.radius\n}\n",
                    ),
                    (
                        "shapes.vl",
                        "struct Circle {\n\tradius: i32,\n}\n\nstruct Square {\n\tside: i32,\n}\n",
                    ),
                ],
            },
            OrganizeCase {
                label: "an unused enum-variant import beside a used one",
                files: &[(
                    "main.vl",
                    "import std::result::Result::{ self, Err, Ok };\n\nfun main(): Result<i32, str> {\n\tOk(1)\n}\n",
                )],
            },
            OrganizeCase {
                label: "a shuffled run where every leaf is used",
                files: &[(
                    "main.vl",
                    "import std::print;\nimport std::result::Result::{ self, Ok };\n\nfun main(): Result<i32, str> {\n\tprint(\"hi\");\n\tOk(1)\n}\n",
                )],
            },
        ];

        for case in cases {
            let (dir, document) = analyze_workspace(case.files);
            assert!(
                document.diagnostics.is_empty(),
                "{}: the pin needs a GREEN starting program, got {:?}",
                case.label,
                document
                    .diagnostics
                    .iter()
                    .map(|e| &e.msg)
                    .collect::<Vec<_>>(),
            );
            let Some(organized_text) = organized(&document) else {
                panic!(
                    "{}: organize made no edit, so this case proves nothing",
                    case.label,
                );
            };
            let _ = std::fs::remove_dir_all(&dir);

            // Re-analyze the ORGANIZED text in the same workspace.
            let mut files: Vec<(&str, &str)> = case.files.to_vec();
            files[0] = (case.files[0].0, organized_text.as_str());
            let (dir, reanalyzed) = analyze_workspace(&files);
            assert!(
                reanalyzed.diagnostics.is_empty(),
                "{}: organizing BROKE the build:\n--- organized ---\n{}\n--- errors ---\n{:?}",
                case.label,
                organized_text,
                reanalyzed
                    .diagnostics
                    .iter()
                    .map(|e| &e.msg)
                    .collect::<Vec<_>>(),
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    // A shuffled top-level run sorts into canonical order; both imports are used,
    // so nothing is pruned.
    #[test]
    fn organize_sorts_a_shuffled_run() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::beta;\nimport pkg::helper::alpha;\nfun main() {\n\talpha();\n\tbeta();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let result = organized(&document).expect("a shuffled run offers a sort edit");
        assert_eq!(
            result,
            "import pkg::helper::alpha;\nimport pkg::helper::beta;\nfun main() {\n\talpha();\n\tbeta();\n}\n",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // An import referenced nowhere is pruned; the used one stays.
    #[test]
    fn organize_prunes_an_unused_import() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::alpha;\nimport pkg::helper::beta;\nfun main() {\n\talpha();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let result = organized(&document).expect("an unused import offers a prune edit");
        assert_eq!(
            result,
            "import pkg::helper::alpha;\nfun main() {\n\talpha();\n}\n",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A brace set with one dead branch shrinks to the live branch; the whole
    // import survives because a live branch remains.
    #[test]
    fn organize_shrinks_a_brace_set_to_its_used_branch() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::{ alpha, beta };\nfun main() {\n\talpha();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let result = organized(&document).expect("a dead branch offers a shrink edit");
        // The one survivor renders unbraced — `{ alpha }` IS `alpha`, the
        // formatter's canonical spelling (kolt.local 005).
        assert_eq!(
            result,
            "import pkg::helper::alpha;\nfun main() {\n\talpha();\n}\n",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // An import referenced ONLY by derive-generated code survives: the code
    // `[derive(Json)]` synthesizes (the `impl Json for Point`) references `Json`,
    // and the analyzer attributes that reference to this file. The empty-diags
    // assert guards against a vacuous pass (a diagnostic would disable pruning).
    #[test]
    fn organize_keeps_an_import_used_only_by_a_derive() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::json::Json;\n[derive(Json)]\nstruct Point {\n\tx: i32,\n\ty: i32,\n}\nfun make(): Point {\n\tPoint { x = 1, y = 2 }\n}\n",
        )]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(
            organized(&document),
            None,
            "an import used only by a derive was pruned",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A re-export is public surface, not local usage — never pruned, even when
    // its name is used nowhere in this file.
    #[test]
    fn organize_never_prunes_a_reexport() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "export import pkg::helper::alpha;\nfun main() {}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(organized(&document), None, "a re-export was pruned");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A file with diagnostics still sorts (a mid-edit error disables pruning, not
    // sorting): the run reorders but the unused import is NOT pruned. Both halves
    // asserted.
    #[test]
    fn organize_with_diagnostics_sorts_but_does_not_prune() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::beta;\nimport pkg::helper::alpha;\nfun main() {\n\talpha();\n\tundefined_name();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            !document.diagnostics.is_empty(),
            "the entry must carry the unresolved-name error",
        );
        let result = organized(&document).expect("sorting is still offered under diagnostics");
        assert!(
            result.contains("import pkg::helper::beta;"),
            "beta was pruned despite diagnostics:\n{result}",
        );
        let alpha_at = result.find("helper::alpha").unwrap();
        let beta_at = result.find("helper::beta").unwrap();
        assert!(alpha_at < beta_at, "the run was not sorted:\n{result}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Block-scoped imports (inside a fn body) are deliberate placements the
    // organizer never touches: they live in a block body, not the top-level item
    // list it walks. The file still parses (the shuffled block `use`s are valid
    // syntax — they don't resolve, which is backlog H2, but the organizer skips
    // them structurally either way), so a no-op here proves the organizer never
    // reached into the block to reorder them.
    #[test]
    fn organize_leaves_block_scoped_imports_alone() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "fun main() {\n\tuse std::collections::Map;\n\tuse std::collections::Set;\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert_eq!(
            organized(&document),
            None,
            "the organizer reached into a block and reordered its imports",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // An already-organized file (sorted, nothing dead) offers no edit — the
    // no-op the action relies on to stay quiet under `codeActionsOnSave`.
    #[test]
    fn organize_is_a_no_op_when_already_organized() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::alpha;\nimport pkg::helper::beta;\nfun main() {\n\talpha();\n\tbeta();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(
            organized(&document),
            None,
            "an already-organized file offered an edit",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- E51: a module import used only through `::` static access ---------
    //
    // A static accessor's SUBJECT (`math` in `math::min(..)`) resolves through
    // the TYPE-position walk (`walk_type_node`'s `Node::Accessor` arm feeding
    // `prepped_type_locals`), whose `definition_id` match used to omit
    // `Type::Module` — so the use site recorded `definition: None` in
    // `type_references` and, since the accessor's own resolution binds only the
    // MEMBER (`min`) into `entity_map`, `import_leaf_is_used` found the module
    // referenced nowhere and pruned it.

    // The reported shape: a module import referenced only via `::`, never as a
    // bare name, stays.
    #[test]
    fn organize_keeps_a_module_import_used_only_via_static_access() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::math;\nfun main() {\n\tmath::min(1, 2);\n}\n",
        )]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(
            organized(&document),
            None,
            "a module import used only via `::` was pruned",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The brace-set variant: a set with one module leaf used only via `::` and
    // one dead sibling shrinks to the live branch, keeping the module leaf.
    #[test]
    fn organize_shrinks_a_brace_set_keeping_a_module_leaf_used_via_static_access() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::{ io, math };\nfun main() {\n\tmath::min(1, 2);\n}\n",
        )]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let result = organized(&document).expect("a dead branch offers a shrink edit");
        // The surviving module leaf renders unbraced (kolt.local 005).
        assert_eq!(
            result,
            "import std::math;\nfun main() {\n\tmath::min(1, 2);\n}\n",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The element-syntax companion (backlog's "companion pin owed"): markup
    // desugars to a bare `view` accessor in VALUE position, which the entity-map
    // check ((B) in `import_leaf_is_used`) already detects — this should already
    // pass; the pin guards it from regressing alongside the module fix above.
    #[test]
    fn organize_keeps_a_view_import_used_only_by_markup() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::ui::view;\nfun page() {\n\t<div>\"hi\"</div>\n}\n",
        )]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(
            organized(&document),
            None,
            "an import used only through markup's desugared `view` accessor was pruned",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The negative: a module import referenced nowhere — not as a bare name, not
    // through `::` — is still pruned. The fix must not turn every module import
    // into a permanent keeper.
    #[test]
    fn organize_prunes_a_genuinely_unused_module_import() {
        let (dir, document) =
            analyze_workspace(&[("main.vl", "import std::math;\nfun main() {}\n")]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let result = organized(&document).expect("a wholly unused module import offers a prune");
        assert_eq!(result, "fun main() {}\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The analyzer-level statement, pinned directly (the `inference` suite's harness
    // exposes only compile success/failure, not `type_references` — this is the
    // LSP-layer pin the task calls for instead): a static accessor's module
    // SUBJECT records the SAME definition id in `type_references` as the
    // import's own leaf reference, rather than `None`. Plant the bug (drop the
    // `Type::Module` arm from the `definition_id` match in analyzer.rs) and
    // `use_definition` goes from `Some(..)` to `None`.
    #[test]
    fn a_module_static_accessors_subject_shares_the_imports_definition_id() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::math;\nfun main() {\n\tmath::min(1, 2);\n}\n",
        )]);
        let program = document.program.as_ref().expect("the program analyzes");
        let text = document.text.clone();
        let leaf_offset = text.find("math").expect("the import leaf");
        let use_offset = text.rfind("math::min").expect("the use site");
        let find_definition = |offset: usize| {
            program
                .type_references
                .iter()
                .find(|(source, span, _, _)| {
                    *source == SourceId(0) && span.into_range().start == offset
                })
                .and_then(|(_, _, definition, _)| *definition)
        };
        let leaf_definition =
            find_definition(leaf_offset).expect("the import's own leaf records a definition");
        let use_definition = find_definition(use_offset).expect(
            "the use site's module SUBJECT must record a definition id (E51's root cause: the \
             definition_id match omitted Type::Module, so this was None and Organize Imports \
             saw the module referenced nowhere)",
        );
        assert_eq!(
            leaf_definition, use_definition,
            "the use site resolved to a different entity than the import's own leaf",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A survey side effect of the fix, pinned deliberately (not a regression
    // guard for E51 itself): go-to-definition on the module SUBJECT of a `::`
    // use site now jumps to the module, same as it already did for the module
    // name written inside the `import` statement itself (`resolve_import`
    // records a reference on every segment, including the root). Before the
    // fix, `definition()`'s `let definition = definition?;` short-circuited on
    // the `None` this use site recorded and answered nothing at all — not a
    // wrong jump, no jump. The landing spot is the module's registered location
    // ("its file, at the top", analyzer.rs) rather than a name span, because a
    // module has no name token of its own to land on.
    #[test]
    fn goto_definition_on_a_modules_static_access_subject_jumps_to_the_module() {
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "import std::math;\nfun main() {\n\tmath::min(1, 2);\n}\n",
        )]);
        let use_offset = document.text.rfind("math::min").expect("the use site") + 1;
        let (source, _span) = document
            .definition(use_offset)
            .expect("go-to-definition on the module subject of a `::` use site");
        assert_ne!(
            source,
            SourceId(0),
            "the module's definition should live in std's math.vl, not the entry",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- WO-5: LSP features survive recoverable errors ---------------------
    //
    // Since the handwritten frontend cut over (H6 S5), `parsing::parse` salvages
    // the parsed prefix of a broken file and `analyze_source` runs the analyzer
    // on that partial tree, so `Document::program` is `Some` for everything that
    // parsed — a syntax error no longer blanks the file. These pins prove every
    // position-based feature keeps serving the salvaged program, per input class.
    //
    // A mid-file error inside a `{}` body: the delimiter recovery closes the
    // broken region and parsing continues, so the items above AND below the
    // error all survive. `let x = ;` is the syntax error; nothing else is wrong.
    const RECOVERABLE_INBODY: &str = "struct Widget { size: i32 }\n\nfun above(w: Widget): i32 {\n\tw.size\n}\n\nfun broken() {\n\tlet x = ;\n}\n\nfun below(): i32 {\n\thelper()\n}\n\nfun helper(): i32 {\n\t7\n}\n";
    // A stray token at file scope: the top-level statement loop reports it and
    // synchronizes to the next item keyword (`editing-dx.md` S1), so the prefix
    // AND the tail are salvaged. This is the other salvage regime.
    const RECOVERABLE_TOPLEVEL: &str =
        "fun above(): i32 {\n\t42\n}\n\n$ garbage here $\n\nfun below(): i32 {\n\t7\n}\n";
    // A clean parse with an analyzer error in the middle (`no_such_name` is
    // unresolved): the whole program is present, so every feature works on both
    // sides of the erroring expression.
    const ANALYZER_ONLY: &str = "struct Point { x: i32 }\n\nfun before(): i32 {\n\tlet p = Point { x = 1 };\n\tp.x\n}\n\nfun uses_undefined(): i32 {\n\tno_such_name()\n}\n\nfun after(): i32 {\n\tlet q = Point { x = 2 };\n\tq.x\n}\n";

    fn analyze_text(text: &str) -> Document {
        Document::analyze(text, &std_root(), Path::new("test.vl"))
    }

    /// The byte offset of `needle` in `text`, plus `delta` (to land inside the
    /// matched identifier). Panics if the needle is absent, so a source edit that
    /// invalidates a pin fails loudly rather than silently probing offset 0.
    fn offset_at(text: &str, needle: &str, delta: usize) -> usize {
        text.find(needle)
            .unwrap_or_else(|| panic!("{needle:?} not found in the pin source"))
            + delta
    }

    // A syntax error mid-file leaves `program` present (salvage) and the
    // diagnostics non-empty — the precondition every pin below relies on. Guards
    // against a source that accidentally became clean (a vacuous pass).
    #[test]
    fn a_recoverable_source_still_yields_a_program() {
        let document = analyze_text(RECOVERABLE_INBODY);
        assert!(
            document.program.is_some(),
            "the salvaged tree must still analyze to a program",
        );
        assert!(
            !document.diagnostics.is_empty(),
            "the syntax error must be reported",
        );
    }

    // Hover on a function name ABOVE a mid-file syntax error shows its full
    // signature (the salvaged program still carries the declaration).
    #[test]
    fn hover_above_a_syntax_error_shows_the_signature() {
        let document = analyze_text(RECOVERABLE_INBODY);
        let hover = document
            .hover(offset_at(RECOVERABLE_INBODY, "fun above", 4))
            .expect("hovering `above` above the error");
        assert!(hover.contains("fun above(w: Widget): i32"), "{hover}",);
    }

    // Hover on a TYPE reference above the error resolves to its declaration —
    // the type_references surface survives salvage.
    #[test]
    fn hover_on_a_type_above_a_syntax_error_shows_its_declaration() {
        let document = analyze_text(RECOVERABLE_INBODY);
        let hover = document
            .hover(offset_at(RECOVERABLE_INBODY, "w: Widget", 3))
            .expect("hovering the `Widget` annotation above the error");
        assert!(hover.contains("struct Widget"), "{hover}");
    }

    // Go-to-definition from a use-site above the error jumps to the binding it
    // resolves to (here the parameter `w`).
    #[test]
    fn goto_definition_above_a_syntax_error_resolves() {
        let document = analyze_text(RECOVERABLE_INBODY);
        let (source, span) = document
            .definition(offset_at(RECOVERABLE_INBODY, "w.size", 0))
            .expect("go-to-def on `w` above the error");
        assert_eq!(source, SourceId(0));
        assert_eq!(
            &RECOVERABLE_INBODY[span.into_range()],
            "w",
            "the jump target is the parameter's name",
        );
    }

    // Scope completion inside a function above the error offers the scope
    // entities: the local parameter (`w`) and a top-level sibling (`helper`),
    // proving both the local and the global scope survive salvage.
    #[test]
    fn completion_above_a_syntax_error_offers_scope_entities() {
        let document = analyze_text(RECOVERABLE_INBODY);
        let labels: Vec<String> = document
            .completion(offset_at(RECOVERABLE_INBODY, "w.size", 0))
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        assert!(labels.contains(&"w".to_string()), "local param: {labels:?}");
        assert!(
            labels.contains(&"helper".to_string()),
            "top-level sibling: {labels:?}",
        );
    }

    // Member completion on a receiver above the error lists the receiver's
    // fields — the receiver's type still resolves in the salvaged program.
    #[test]
    fn member_completion_above_a_syntax_error_lists_fields() {
        let document = analyze_text(RECOVERABLE_INBODY);
        let labels: Vec<String> = document
            .completion(offset_at(RECOVERABLE_INBODY, "w.size", 2))
            .into_iter()
            .map(|completion| completion.label)
            .collect();
        assert!(labels.contains(&"size".to_string()), "{labels:?}");
    }

    // Semantic tokens cover the salvaged region (non-empty) and the pass never
    // panics on a partial program.
    #[test]
    fn semantic_tokens_cover_a_salvaged_region() {
        let document = analyze_text(RECOVERABLE_INBODY);
        assert!(
            !document.semantic_tokens().is_empty(),
            "the salvaged declarations must still tokenize",
        );
    }

    // An i-string being typed is the shape that reaches the server most often:
    // the opening `i"""` exists before its closing delimiter does. The lexer
    // hands the rest of the file to the unterminated literal, so the ITEMS ABOVE
    // are what survives — analysis must still terminate, report the error, and
    // tokenize (H7).
    #[test]
    fn an_unterminated_interpolated_triple_quoted_string_still_analyzes() {
        let document = analyze_text(
            "fun above(): i32 {\n\t42\n}\n\nfun typing() {\n\tlet text = i\"\"\"\n\thalf written\n",
        );
        assert!(
            document.program.is_some(),
            "the salvaged tree must still analyze to a program",
        );
        assert!(
            !document.diagnostics.is_empty(),
            "the unterminated literal must be reported",
        );
        assert!(
            !document.semantic_tokens().is_empty(),
            "the salvaged declarations must still tokenize",
        );
        let names: Vec<String> = document
            .document_symbols()
            .into_iter()
            .map(|symbol| symbol.name)
            .collect();
        assert!(names.contains(&"above".to_string()), "{names:?}");
    }

    // Document symbols list every salvaged item — the outline survives a mid-file
    // error (an in-body error recovers, so the tail items are here too).
    #[test]
    fn document_symbols_list_the_salvaged_items() {
        let document = analyze_text(RECOVERABLE_INBODY);
        let names: Vec<String> = document
            .document_symbols()
            .into_iter()
            .map(|symbol| symbol.name)
            .collect();
        for expected in ["Widget", "above", "broken", "below", "helper"] {
            assert!(
                names.contains(&expected.to_string()),
                "missing `{expected}` from the outline: {names:?}",
            );
        }
    }

    // The reality of the in-body regime: an error inside a `{}` body is
    // delimiter-recovered, so the items AFTER it survive too — hover and
    // go-to-def work on a function declared below the broken one.
    #[test]
    fn an_in_body_error_keeps_the_items_after_it() {
        let document = analyze_text(RECOVERABLE_INBODY);
        let hover = document
            .hover(offset_at(RECOVERABLE_INBODY, "fun below", 4))
            .expect("hovering `below`, declared after the error");
        assert!(hover.contains("fun below(): i32"), "{hover}");
        let (source, span) = document
            .definition(offset_at(RECOVERABLE_INBODY, "helper()", 0))
            .expect("go-to-def on `helper`, called and declared after the error");
        assert_eq!(source, SourceId(0));
        assert_eq!(&RECOVERABLE_INBODY[span.into_range()], "helper");
    }

    // The top-level regime, since the statement/item synchronizer shipped
    // (`editing-dx.md` S1): a stray token at file scope is reported and skipped to
    // the next item boundary, so the items on BOTH sides of it survive. This pin
    // used to assert the opposite — that `below` was not in the program at all —
    // which is precisely the file-tail blackout §2.2 mechanism 3 measured.
    #[test]
    fn a_top_level_error_keeps_the_items_on_both_sides() {
        let document = analyze_text(RECOVERABLE_TOPLEVEL);
        assert!(document.program.is_some());
        assert!(!document.diagnostics.is_empty());
        let names: Vec<String> = document
            .document_symbols()
            .into_iter()
            .map(|symbol| symbol.name)
            .collect();
        assert!(
            names.contains(&"above".to_string()),
            "prefix kept: {names:?}"
        );
        assert!(
            names.contains(&"below".to_string()),
            "the tail after a top-level stray token is recovered too: {names:?}",
        );
        assert!(
            document
                .hover(offset_at(RECOVERABLE_TOPLEVEL, "fun above", 4))
                .is_some_and(|hover| hover.contains("fun above(): i32")),
            "the prefix item still hovers",
        );
        assert!(
            document
                .hover(offset_at(RECOVERABLE_TOPLEVEL, "fun below", 4))
                .is_some_and(|hover| hover.contains("fun below(): i32")),
            "and so does the item below the error",
        );
    }

    // A clean parse with an analyzer error in the middle: hover and go-to-def
    // work on BOTH sides of the erroring expression (the whole program is
    // present — a diagnostic never blanks a feature).
    #[test]
    fn hover_and_goto_work_on_both_sides_of_an_analyzer_error() {
        let document = analyze_text(ANALYZER_ONLY);
        assert!(
            document
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.msg.contains("cannot find 'no_such_name'")),
            "the analyzer error must be present: {:?}",
            document.diagnostics,
        );
        // Before the error.
        assert!(
            document
                .hover(offset_at(ANALYZER_ONLY, "Point { x = 1 }", 0))
                .is_some_and(|hover| hover.contains("struct Point")),
            "hover before the error",
        );
        let (source, span) = document
            .definition(offset_at(ANALYZER_ONLY, "Point { x = 1 }", 0))
            .expect("go-to-def before the error");
        assert_eq!(
            (source, &ANALYZER_ONLY[span.into_range()]),
            (SourceId(0), "Point")
        );
        // After the error.
        assert!(
            document
                .hover(offset_at(ANALYZER_ONLY, "Point { x = 2 }", 0))
                .is_some_and(|hover| hover.contains("struct Point")),
            "hover after the error",
        );
        let (source, span) = document
            .definition(offset_at(ANALYZER_ONLY, "Point { x = 2 }", 0))
            .expect("go-to-def after the error");
        assert_eq!(
            (source, &ANALYZER_ONLY[span.into_range()]),
            (SourceId(0), "Point")
        );
        // The full outline and tokens are present (nothing degraded).
        assert_eq!(document.document_symbols().len(), 4);
        assert!(!document.semantic_tokens().is_empty());
    }

    // The graceful-empty case: a hopeless file panics nowhere and every feature
    // returns cleanly. The test completing IS the no-panic proof; the empty
    // assertions pin that a program with no salvageable items answers with
    // nothing rather than garbage.
    #[test]
    fn a_hopeless_file_answers_every_feature_without_panicking() {
        for source in [
            "@@@@ !!!! $$$$ %%%%\n",
            "",
            "))))]]]]}}}}\n",
            "12 34 fun fun",
        ] {
            let document = analyze_text(source);
            // Position queries at several offsets, all in bounds.
            for offset in [0, source.len() / 2, source.len()] {
                let _ = document.hover(offset);
                let _ = document.definition(offset);
                let _ = document.completion(offset);
                let _ = document.references(offset);
            }
            assert!(
                document.document_symbols().is_empty(),
                "no items to outline in {source:?}",
            );
            assert!(
                document.semantic_tokens().is_empty(),
                "no tokens in {source:?}",
            );
            assert!(document.inlay_hints().is_empty(), "no hints in {source:?}");
            assert!(
                document.organize_import_edits().is_empty(),
                "no imports to organize in {source:?}",
            );
        }
    }

    // Formatting a source that does not parse cleanly degrades to no edit: the
    // formatter's net requires a clean parse, so `format` returns the input
    // verbatim, and the LSP `formatting` handler turns `formatted == source`
    // into `Ok(None)` — no edit, no error popup.
    #[test]
    fn a_broken_source_formats_to_no_edit() {
        for source in [RECOVERABLE_INBODY, RECOVERABLE_TOPLEVEL] {
            assert_eq!(
                vilan_core::formatter::format(source),
                source,
                "a non-clean source must format to itself (the handler then emits no edit)",
            );
        }
    }

    // --- Snapshot consistency (lsp-snapshot-consistency.md) ----------------
    //
    // The two-snapshot law: an edit advances the LIVE snapshot alone, so every
    // answer computed from the program stays in the ANALYZED snapshot's
    // coordinates until an analysis lands. Each pin below asserts BOTH halves —
    // the analyzed conversion is unmoved, and the live conversion (what the
    // handlers used to call) really does move. Without the second half a pin
    // could pass on a fixture that never skewed at all.

    /// A three-line program with one token per line and an un-annotated `let`
    /// (so it carries an inlay hint too).
    const SKEW_SOURCE: &str = "fun main() {\n\tlet value = 1;\n\tlet other = value;\n}\n";

    /// Every semantic token's range in a given index, in token order.
    fn token_ranges(document: &Document, index: &LineIndex) -> Vec<Range> {
        document
            .semantic_tokens()
            .iter()
            .map(|(span, _, _)| index.range(span))
            .collect()
    }

    // Skew pin 1: one character inserted on an early line. Every token below it
    // keeps its ANALYZED coordinates — line/column are stable under edits on
    // other lines, so the highlighting the client is showing stays exactly
    // where the words still are. Through the live index the same bytes land a
    // column early, which is the highlighting-breaks-while-typing bug.
    #[test]
    fn semantic_token_positions_hold_still_when_an_early_line_grows() {
        let mut document = analyze_text(SKEW_SOURCE);
        let before = token_ranges(&document, document.analyzed_index());
        assert!(!before.is_empty(), "the fixture must produce tokens");
        // One character on line 0 (`fun  main`), so every later byte shifts by 1.
        document.set_text(&SKEW_SOURCE.replace("fun main", "fun  main"));
        assert!(document.is_stale(), "the buffer has advanced");
        assert_eq!(
            token_ranges(&document, document.analyzed_index()),
            before,
            "program spans convert through the ANALYZED index",
        );
        assert_ne!(
            token_ranges(&document, &document.line_index),
            before,
            "the fixture must actually skew through the live index",
        );
    }

    // Skew pin 2: the same for inlay hints, whose anchors are program offsets
    // too. A slid hint is worse than a wrong one — the viewport filter in the
    // handler drops it once it slides out of the requested range.
    #[test]
    fn inlay_hint_positions_hold_still_when_an_early_line_grows() {
        let mut document = analyze_text(SKEW_SOURCE);
        let hints = document.inlay_hints();
        assert!(!hints.is_empty(), "the fixture must produce a hint");
        let before: Vec<Position> = hints
            .iter()
            .map(|(offset, _)| document.analyzed_position(*offset))
            .collect();
        document.set_text(&SKEW_SOURCE.replace("fun main", "fun  main"));
        let after: Vec<Position> = document
            .inlay_hints()
            .iter()
            .map(|(offset, _)| document.analyzed_position(*offset))
            .collect();
        assert_eq!(after, before, "hint anchors hold their analyzed positions");
        let through_live: Vec<Position> = document
            .inlay_hints()
            .iter()
            .map(|(offset, _)| document.line_index.position(*offset))
            .collect();
        assert_ne!(through_live, before, "the fixture must actually skew");
    }

    // Skew pin 3: a NEWLINE inserted above — the edit that moves every token to
    // a different LINE through the live index, not merely a different column.
    #[test]
    fn token_positions_hold_still_when_a_newline_is_inserted() {
        let mut document = analyze_text(SKEW_SOURCE);
        let before = token_ranges(&document, document.analyzed_index());
        document.set_text(&format!("// a new first line\n{SKEW_SOURCE}"));
        assert_eq!(
            token_ranges(&document, document.analyzed_index()),
            before,
            "an inserted line does not move the analyzed coordinates",
        );
        let through_live = token_ranges(&document, &document.line_index);
        assert_ne!(through_live, before, "the fixture must actually skew");
        assert!(
            through_live
                .iter()
                .any(|range| range.start.line != range.end.line),
            "through the live index the same bytes can even straddle a line \
             boundary — a shape the wire format has no encoding for: {through_live:?}",
        );
    }

    // Skew pin 4: a SHRINKING edit that leaves a token's old offset past the
    // new end of the buffer. The live index clamps such an offset to the text
    // length, so every token below the cut used to pile up on the last
    // character; the analyzed index still holds the real text they index.
    #[test]
    fn a_shrinking_edit_does_not_clamp_tokens_to_the_new_end() {
        let mut document = analyze_text(SKEW_SOURCE);
        let before = token_ranges(&document, document.analyzed_index());
        assert!(before.len() >= 2, "need tokens on more than one line");
        // Cut the file down to two lines — shorter than the last token's offset.
        document.set_text("fun main() {\n}\n");
        assert_eq!(
            token_ranges(&document, document.analyzed_index()),
            before,
            "no clamping, no panic: the analyzed text is still there",
        );
        let through_live = token_ranges(&document, &document.line_index);
        let last = through_live.last().copied().expect("a token");
        let piled_up = through_live
            .iter()
            .filter(|range| range.start == last.start)
            .count();
        assert!(
            piled_up > 1,
            "the fixture must make the live index pile tokens on one spot: {through_live:?}",
        );
    }

    // Inbound pin (9): a position → program offset lookup goes through the
    // ANALYZED index, so a hover over a word the analysis saw still resolves
    // after an unrelated edit above it. Through the live index the same
    // position names a different byte and the hover misses (or, worse, hits the
    // neighbouring entity).
    #[test]
    fn a_program_lookup_after_an_early_edit_resolves_through_the_analyzed_index() {
        let mut document = analyze_text(SKEW_SOURCE);
        // The `value` USE on line 2 (`let other = value;`).
        let use_offset = offset_at(SKEW_SOURCE, "= value;", 2);
        let position = document.analyzed_position(use_offset);
        let expected = document.hover(use_offset).expect("`value` hovers");
        document.set_text(&SKEW_SOURCE.replace("fun main", "fun  main"));
        assert_eq!(
            document.hover(document.analyzed_offset(position)),
            Some(expected),
            "the analyzed index maps the position back to the same entity",
        );
        assert_ne!(
            document.analyzed_offset(position),
            document.line_index.offset(position),
            "the fixture must actually skew the inbound conversion",
        );
    }

    // Merge pin (7): an analysis landing on a buffer that moved on adopts every
    // analysis-side field and keeps BOTH live-side ones. Clobbering the whole
    // document — what `documents.insert` used to do — threw away every
    // character typed during the 80–190 ms analysis.
    #[test]
    fn landing_an_analysis_keeps_a_live_edit_and_adopts_the_program() {
        let first = "fun main() {\n\tlet value = 1;\n}\n";
        let second = "fun main() {\n\tlet value = 1;\n\tlet other = 2;\n}\n";
        let mut document = analyze_text(first);
        // The analysis of `second` starts…
        let analysis = analyze_text(second);
        // …and while it runs the user types a third revision.
        let live = "fun main() {\n\tlet value = 1;\n\tlet other = 2;\n\tlet third = 3;\n}\n";
        document.set_text(live);
        let analyzed_tokens = analysis.semantic_tokens();
        document.adopt_analysis(analysis);

        // Live side: kept, both halves.
        assert_eq!(document.text, live, "typing is never undone by a merge");
        assert_eq!(
            document.line_index.text(),
            live,
            "the live index tracks the live text",
        );
        // Analysis side: adopted, and the analyzed index is the one the spans
        // index (it holds `second`, not the live text).
        assert_eq!(document.analyzed_text(), second);
        assert_eq!(document.text_hash, hash_text(second));
        assert_eq!(document.semantic_tokens(), analyzed_tokens);
        // …and program spans convert through the ADOPTED index — the analyzed
        // conversion answers in `second`'s coordinates, not the live text's.
        let (first_span, _, _) = document.semantic_tokens()[0];
        assert_eq!(
            document.analyzed_range(&first_span),
            LineIndex::new(second).range(&first_span),
            "the adopted analyzed index is the one conversions use",
        );
        assert_eq!(
            document.inlay_hints().len(),
            2,
            "the newly analyzed `other` hint is adopted",
        );
        assert!(document.is_stale(), "the buffer is ahead of the analysis");
        // …and the live text is what completion's context scan reads.
        assert_eq!(document.line_index.text(), live);
    }

    // Merge pin (8): when the buffer did NOT move, everything is adopted and
    // the document is not stale — the steady state after a typing pause.
    #[test]
    fn landing_an_analysis_on_unchanged_text_adopts_everything() {
        let first = "fun main() {\n\tlet value = 1;\n}\n";
        let second = "fun main() {\n\tlet value = 1;\n\tlet other = 2;\n}\n";
        let mut document = analyze_text(first);
        document.set_text(second);
        document.adopt_analysis(analyze_text(second));
        assert!(!document.is_stale());
        assert_eq!(document.text, second);
        assert_eq!(document.analyzed_text(), second);
        assert_eq!(document.line_index.text(), document.analyzed_text());
        assert_eq!(document.inlay_hints().len(), 2);
    }

    // `is_stale` is TEXT equality, never a flag set by `set_text`: an edit that
    // returns the buffer to the analyzed text takes the debounce's
    // unchanged-text short-circuit, so no analysis would ever land to clear a
    // flag — a mutating request would refuse for the rest of the session.
    #[test]
    fn a_buffer_edited_back_to_the_analyzed_text_is_not_stale() {
        let source = "fun main() {\n\tlet value = 1;\n}\n";
        let mut document = analyze_text(source);
        assert!(!document.is_stale());
        document.set_text("fun main() {\n\tlet value = 12;\n}\n");
        assert!(document.is_stale(), "mid-edit");
        document.set_text(source);
        assert!(
            !document.is_stale(),
            "reverting heals without needing an analysis to land",
        );
    }

    // --- B39a: the dependents sweep is gated on the dependency edge ---
    //
    // `depends_on` is the whole decision `reanalyze_dependents` filters by,
    // so these pin the recorded behavior directly: an importer IS a
    // dependent, a stranger is NOT (one analysis per pause, not two), path
    // spelling cannot fake a miss, and a document with no program stays on
    // the conservative always-sweep arm.

    #[test]
    fn a_document_depends_on_the_files_its_analysis_loaded() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::greet;\nfun main() { greet(); }\n",
            ),
            ("helper.vl", "fun greet() {}\n"),
        ]);
        assert!(
            document.depends_on(&dir.join("helper.vl")),
            "the imported module is a dependency edge"
        );
        assert!(
            document.depends_on(&dir.join("./helper.vl")),
            "path spelling must not fake a miss"
        );
        assert!(
            document.depends_on(&dir.join("main.vl")),
            "a document depends on its own file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_document_does_not_depend_on_an_unrelated_open_file() {
        let (dir, document) = analyze_workspace(&[
            ("main.vl", "fun main() {}\n"),
            ("other.vl", "fun elsewhere() {}\n"),
        ]);
        assert!(
            !document.depends_on(&dir.join("other.vl")),
            "no import edge means no reanalysis - one analysis per pause, not two"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_document_without_a_program_is_conservatively_a_dependent() {
        let (dir, mut document) = analyze_workspace(&[("main.vl", "fun main() {}\n")]);
        document.program = AnalyzedProgram::none();
        assert!(
            document.depends_on(&dir.join("anything.vl")),
            "with no recorded source set, re-analysis is the conservative direction"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // --- B39c: incremental edits apply in order and stay mappable ---
    //
    // `apply_change` is the sync contract (ordered ranged splices at UTF-16
    // positions, full-replacement resets), and `live_offset` is the map the
    // inlay filter's exactness rides on. Each case pins a distinct shape.

    fn plain_document(text: &str) -> Document {
        let dir = std::env::temp_dir().join(format!(
            "vilan_lsp_b39c_{}_{:p}",
            std::process::id(),
            text.as_ptr()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let entry = dir.join("main.vl");
        std::fs::write(&entry, text).unwrap();
        Document::analyze(text, &std_root(), &entry)
    }

    fn range_at(line: u32, start: u32, end: u32) -> tower_lsp::lsp_types::Range {
        tower_lsp::lsp_types::Range::new(
            tower_lsp::lsp_types::Position::new(line, start),
            tower_lsp::lsp_types::Position::new(line, end),
        )
    }

    #[test]
    fn ordered_ranged_edits_rebuild_the_text_a_full_sync_would_send() {
        let mut document = plain_document("fun main() {\n\tlet a = 1;\n}\n");
        // Two events in one notification: insert, then edit AFTER the insert
        // against the already-edited text - the incremental-sync contract.
        document.apply_change(Some(range_at(1, 5, 6)), "value");
        document.apply_change(Some(range_at(1, 13, 14)), "2");
        assert_eq!(document.text, "fun main() {\n\tlet value = 2;\n}\n");
        assert_eq!(
            document.live_edits.as_ref().map(|edits| edits.len()),
            Some(2),
            "both splices recorded"
        );
    }

    #[test]
    fn a_ranged_edit_at_an_astral_column_splices_at_the_character() {
        // The emoji is one character, two UTF-16 units: a column AFTER it
        // must land after all four of its bytes.
        let mut document = plain_document("// \u{1F980} x\n");
        document.apply_change(Some(range_at(0, 6, 7)), "y");
        assert_eq!(document.text, "// \u{1F980} y\n");
    }

    #[test]
    fn a_multi_line_deletion_splices_across_lines() {
        let mut document = plain_document("fun main() {\n\tlet a = 1;\n\tlet b = 2;\n}\n");
        let range = tower_lsp::lsp_types::Range::new(
            tower_lsp::lsp_types::Position::new(1, 0),
            tower_lsp::lsp_types::Position::new(2, 0),
        );
        document.apply_change(Some(range), "");
        assert_eq!(document.text, "fun main() {\n\tlet b = 2;\n}\n");
    }

    #[test]
    fn a_full_replacement_event_resets_the_map() {
        let mut document = plain_document("fun main() {}\n");
        document.apply_change(Some(range_at(0, 0, 0)), "x");
        assert!(document.live_edits.is_some(), "ranged edits keep the map");
        document.apply_change(None, "fun other() {}\n");
        assert_eq!(document.text, "fun other() {}\n");
        assert!(
            document.live_edits.is_none(),
            "a whole-text replacement has no shape to record"
        );
    }

    #[test]
    fn live_offset_maps_through_the_recorded_edits() {
        let mut document = plain_document("fun main() {\n\tlet a = 1;\n}\n");
        // Insert a whole line above: everything below shifts by its length.
        document.apply_change(Some(range_at(0, 0, 0)), "// note\n");
        let shifted = document.live_offset(13).expect("mappable");
        assert_eq!(shifted, 13 + "// note\n".len());
        assert_eq!(
            document.live_offset(0),
            Some(8),
            "the old start sits after the insert"
        );
        // An anchor INSIDE a replaced region clamps into the replacement.
        let mut replaced = plain_document("fun main() {\n\tlet abc = 1;\n}\n");
        replaced.apply_change(Some(range_at(1, 5, 8)), "x");
        let inside = replaced.live_offset(20).expect("mappable");
        assert!(
            inside >= 18 && inside <= 19,
            "clamped into the replacement, got {inside}"
        );
        // A broken map answers None; adoption restores identity.
        let mut broken = plain_document("fun main() {}\n");
        broken.set_text("fun main() {}\n");
        assert_eq!(broken.live_offset(3), None, "set_text breaks the map");
        let fresh = plain_document("fun main() {}\n");
        broken.adopt_analysis(fresh);
        assert_eq!(broken.live_offset(3), Some(3), "adoption restores identity");
    }

    // --- B38: salvage tail retention ---------------------------------------

    /// The headline: a parse break that truncates analysis to a prefix (an
    /// unterminated triple-quoted string) no longer blanks the tail — the
    /// byte-identical suffix keeps the previous analysis's tokens, shifted.
    /// The first assertion validates the premise (the break really does
    /// truncate), so this pin cannot pass vacuously.
    #[test]
    fn a_salvage_break_keeps_the_byte_identical_tail_highlighted() {
        let whole = "fun alpha() {\n\tlet a = 1;\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        // An unterminated interpolated triple-quoted string: there is no
        // resynchronisation point inside one, so the LEXER stops there and every
        // token below is gone — the blank-tail shape B38 exists for. (A stray
        // top-level token used to do this too; since the statement/item
        // synchronizer shipped, `editing-dx.md` S1, the parse skips it and the
        // tail is analyzed normally, which is the blackout's death and leaves the
        // lexer-level break as the honest premise here.)
        let broken = "fun alpha() {\n\tlet a = i\"\"\";\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        let zeta = offset_at(broken, "zeta", 0);

        // Premise: the broken text's own analysis has no token at `zeta`.
        let fresh = analyze_text(broken);
        assert!(
            !fresh
                .semantic_tokens()
                .iter()
                .any(|(span, ..)| span.start <= zeta && zeta < span.end),
            "the break no longer truncates the parse — this pin's premise \
             (and B38 itself) needs a new break shape"
        );

        let mut document = analyze_text(whole);
        document.adopt_analysis(fresh);
        let retained: Vec<_> = document
            .semantic_tokens()
            .into_iter()
            .filter(|(span, ..)| span.start <= zeta && zeta < span.end)
            .collect();
        assert!(
            !retained.is_empty(),
            "the byte-identical tail below the break must keep its tokens"
        );
    }

    /// The honesty half: a tail line the user EDITED is not byte-identical,
    /// so it gets nothing — retained tokens never cover changed text.
    #[test]
    fn an_edited_line_below_the_break_stays_unhighlighted() {
        let whole = "fun alpha() {\n\tlet a = 1;\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        let broken_and_edited =
            "fun alpha() {\n\tlet a = i\"\"\";\n}\nfun omega() {\n\tlet quux = 8;\n}\n";
        let quux = offset_at(broken_and_edited, "quux", 0);
        let mut document = analyze_text(whole);
        document.adopt_analysis(analyze_text(broken_and_edited));
        assert!(
            !document
                .semantic_tokens()
                .iter()
                .any(|(span, ..)| span.start <= quux && quux < span.end),
            "an edited tail line must not inherit tokens from text it no \
             longer matches"
        );
    }

    /// A complete analysis suppresses retention wholesale: once the text
    /// parses again, the stream is exactly the fresh one.
    #[test]
    fn a_complete_analysis_suppresses_the_retained_tail() {
        let whole = "fun alpha() {\n\tlet a = 1;\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        let broken = "fun alpha() {\n\tlet a = i\"\"\";\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        let mut document = analyze_text(whole);
        document.adopt_analysis(analyze_text(broken));
        // The user closes the string; the next analysis is whole again.
        document.adopt_analysis(analyze_text(whole));
        let expected = analyze_text(whole).semantic_tokens();
        assert_eq!(
            document.semantic_tokens().len(),
            expected.len(),
            "a complete analysis must serve exactly its own tokens"
        );
    }
}

/// M7 (`leak-soak.md` §7): the entry text and tree an analysis leaks are given
/// back when the `Document` drops or replaces the analysis. Platform-
/// independent — the counters need no `/proc` — so unlike `leak_measurement`
/// below this is not Linux-gated. Each pin runs its analyses on ONE big-stack
/// thread and reads that thread's counters, because the tally is thread-local
/// (`leak_tally`'s module doc); the last pin is the exception on purpose.
#[cfg(test)]
mod entry_reclaim {
    use super::*;
    use crate::document::tests::{on_big_stack, std_root};
    use vilan_core::leak_tally::{self, LeakSite};

    const FIRST: &str = "import std::print;\n\nfun main() {\n\tprint(\"one\");\n}\n";
    const SECOND: &str =
        "import std::print;\n\nfun main() {\n\tlet greeting = \"two\";\n\tprint(greeting);\n}\n";

    fn analyze_here(text: &str) -> Document {
        Document::analyze_on_this_thread(text, &std_root(), Path::new("reclaim.vl"))
    }

    /// Closing a document (dropping it) gives back exactly the text and tree
    /// its analysis recorded — the gross record stands, the outstanding
    /// balance at both sites is zero.
    #[test]
    fn dropping_a_document_reclaims_its_entry_text_and_tree() {
        on_big_stack(|| {
            leak_tally::reset();
            let document = analyze_here(FIRST);
            assert!(document.program.is_some(), "the fixture analyzes");
            assert_eq!(leak_tally::bytes(LeakSite::LspEntryText), FIRST.len());
            let tree = leak_tally::bytes(LeakSite::EntryAst);
            assert!(tree > 0, "no entry tree was recorded — the pin is vacuous");
            assert_eq!(leak_tally::released(LeakSite::LspEntryText), 0);
            assert_eq!(leak_tally::released(LeakSite::EntryAst), 0);
            drop(document);
            assert_eq!(
                leak_tally::released(LeakSite::LspEntryText),
                FIRST.len(),
                "the entry text was not given back to the byte"
            );
            assert_eq!(
                leak_tally::released(LeakSite::EntryAst),
                tree,
                "the entry tree was not given back at the bytes it recorded"
            );
            assert_eq!(leak_tally::outstanding(LeakSite::LspEntryText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::EntryAst), 0);
            // The gross record is unchanged by the reclaim: `bytes` still says
            // what the analysis leaked, which is what the plateau pins read.
            assert_eq!(leak_tally::bytes(LeakSite::LspEntryText), FIRST.len());
        });
    }

    /// The server's path: a document analyzes, the buffer changes, a second
    /// analysis lands through `adopt_analysis`. Exactly the FIRST analysis's
    /// allocations are given back, exactly the SECOND's stay out, and the
    /// adopted program still answers — its own allocations were not the ones
    /// reclaimed. Then the document drops and both sites net to zero.
    #[test]
    fn a_document_that_analyzes_twice_reclaims_the_first_analysis_and_keeps_the_second() {
        on_big_stack(|| {
            leak_tally::reset();
            let mut document = analyze_here(FIRST);
            let first_tree = leak_tally::bytes(LeakSite::EntryAst);
            let second = analyze_here(SECOND);
            let second_tree = leak_tally::bytes(LeakSite::EntryAst) - first_tree;
            assert!(
                first_tree > 0 && second_tree > 0,
                "both analyses record a tree"
            );
            assert_eq!(
                leak_tally::released(LeakSite::LspEntryText),
                0,
                "nothing is reclaimed before the adoption"
            );
            document.adopt_analysis(second);
            assert_eq!(
                leak_tally::released(LeakSite::LspEntryText),
                FIRST.len(),
                "adoption must reclaim the superseded analysis's text, and only that"
            );
            assert_eq!(
                leak_tally::released(LeakSite::EntryAst),
                first_tree,
                "adoption must reclaim the superseded analysis's tree, and only that"
            );
            assert_eq!(
                leak_tally::outstanding(LeakSite::LspEntryText),
                SECOND.len() as isize,
                "the adopted analysis's text is the one still out"
            );
            assert_eq!(
                leak_tally::outstanding(LeakSite::EntryAst),
                second_tree as isize,
                "the adopted analysis's tree is the one still out"
            );
            // The adopted program answers from its own, live allocations.
            assert_eq!(document.analyzed_text(), SECOND);
            let offset = SECOND
                .find("greeting")
                .expect("the binding is in the fixture")
                + 1;
            assert!(
                document.hover(offset).is_some(),
                "the adopted analysis no longer answers hover"
            );
            assert!(
                !document.semantic_tokens().is_empty(),
                "the adopted analysis no longer produces semantic tokens"
            );
            drop(document);
            assert_eq!(leak_tally::outstanding(LeakSite::LspEntryText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::EntryAst), 0);
        });
    }

    /// The shipped server's allocation lifetime: `Document::analyze` records on
    /// a thread that then dies, and the `Document` is dropped on the caller's
    /// thread. The reclaim happens where the drop happens — this thread sees a
    /// release it never recorded, a negative outstanding balance that a cross-
    /// thread sum (the soak's) nets to zero.
    #[test]
    fn an_analysis_from_its_own_thread_is_reclaimed_on_the_thread_that_drops_it() {
        leak_tally::reset();
        let document = Document::analyze(FIRST, &std_root(), Path::new("reclaim.vl"));
        assert!(document.program.is_some(), "the fixture analyzes");
        assert_eq!(
            leak_tally::bytes(LeakSite::LspEntryText),
            0,
            "the record belongs to the analysis thread, not this one"
        );
        drop(document);
        assert_eq!(leak_tally::released(LeakSite::LspEntryText), FIRST.len());
        assert!(
            leak_tally::released(LeakSite::EntryAst) > 0,
            "the tree was not given back on the dropping thread"
        );
        assert_eq!(
            leak_tally::outstanding(LeakSite::LspEntryText),
            -(FIRST.len() as isize)
        );
    }

    /// The degraded document (a panicked analysis) owns nothing: dropping it
    /// releases nothing and cannot double-free.
    #[test]
    fn the_internal_error_document_owns_nothing_to_reclaim() {
        leak_tally::reset();
        let document = Document::internal_error(FIRST);
        assert!(!document.program.is_some());
        drop(document);
        assert_eq!(leak_tally::released_total(), 0);
    }
}

/// M9 (`leak-soak.md` §7.9.4): the §7.5 dependent-edit shape — an edited file
/// that another OPEN document imports. `did_change` updates the overlay and
/// `reanalyze_dependents` re-analyzes the importer, whose loader reads the
/// edited buffer from the overlay; every landed keystroke is a DISTINCT
/// content. This is §7.9.1's throwaway probe promoted into the harness as the
/// measurement. Platform-independent like `entry_reclaim` above — the
/// instrument is the thread-local leak tally and a wall clock, no `/proc`.
#[cfg(test)]
mod overlay_module_reclaim {
    use super::*;
    use crate::document::tests::{on_big_stack, std_root};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{Duration, Instant};
    use vilan_core::leak_tally::{self, LeakSite};

    /// The open importer: `main.vl`, importing the module under edit.
    const ENTRY: &str =
        "import pkg::helper::value;\n\nfun main() {\n\tlet doubled = value() * 2;\n}\n";

    /// One DISTINCT, fixed-width clean helper content per `i` — a landed
    /// keystroke in a 36-byte file. Fixed width so per-content byte claims
    /// are exact multiples.
    fn clean_helper(i: usize) -> String {
        format!("export fun value(): i32 {{\n\t{:06}\n}}\n", 100000 + i)
    }

    /// One DISTINCT broken helper content per `i` — the same file mid-edit,
    /// missing its closing brace, so it does not parse clean.
    fn broken_helper(i: usize) -> String {
        format!("export fun value(): i32 {{\n\t{:06}\n", 100000 + i)
    }

    /// A realistically sized module under edit (~8.5 KB, 121 functions): the
    /// timing fixture, so the re-parse cost the mechanism trades for the leak
    /// is measured on a file big enough to see.
    fn large_helper(i: usize) -> String {
        let mut text = clean_helper(i);
        for ordinal in 0..120 {
            text.push_str(&format!(
                "export fun value_{ordinal:03}(): i32 {{\n\tlet a = {ordinal} + 1;\n\tlet b = a * 2;\n\ta + b\n}}\n"
            ));
        }
        text
    }

    /// A scratch package on disk: `main.vl` beside `helper.vl` (the loader
    /// resolves `pkg::helper` next to the entry). Unique per call — the
    /// overlay is process-global, so two tests must never share a path.
    fn scratch_package() -> (PathBuf, PathBuf, PathBuf) {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("vilan_m9_overlay_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let entry_path = dir.join("main.vl");
        let helper_path = dir.join("helper.vl");
        std::fs::write(&entry_path, ENTRY).expect("write main.vl");
        std::fs::write(&helper_path, clean_helper(0)).expect("write helper.vl");
        (dir, entry_path, helper_path)
    }

    /// One measured phase: `count` dependent re-analyses of the entry, the
    /// helper's overlay set to `helper_at(start + round)` before each, every
    /// analysis landed through `adopt_analysis` — the server's flow. Returns
    /// the wall time of the whole phase; the caller reads the tally.
    fn run_phase(
        document: &mut Document,
        entry_path: &Path,
        helper_path: &Path,
        helper_at: &dyn Fn(usize) -> String,
        start: usize,
        count: usize,
    ) -> Duration {
        let std_dir = std_root();
        let phase_start = Instant::now();
        for round in start..start + count {
            vilan_core::analyzer::set_document_overlay(helper_path, Some(helper_at(round)));
            let analysis = Document::analyze_on_this_thread(ENTRY, &std_dir, entry_path);
            document.adopt_analysis(analysis);
        }
        phase_start.elapsed()
    }

    fn per_analysis(wall: Duration, count: usize) -> String {
        format!(
            "{:.2} ms/analysis",
            wall.as_secs_f64() * 1000.0 / count as f64
        )
    }

    /// The M9 claim, asserted after every measured phase: the overlay's
    /// churning contents reached NEITHER process-global cache — zero growth
    /// at `parse_clean_cached`'s two sites (which §7.5 measured leaking one
    /// text + tree per distinct content, and §7.9.1 measured leaking a
    /// broken content's text a second time) and zero at the loader's
    /// error-cache sites.
    fn assert_no_global_cache_growth(label: &str) {
        for site in [
            LeakSite::ParseCleanCacheText,
            LeakSite::ParseCleanCacheAst,
            LeakSite::ModuleErrorText,
            LeakSite::ModuleErrorAst,
        ] {
            assert_eq!(
                leak_tally::bytes(site),
                0,
                "[{label}] the overlay-served module grew the process-global \
                 cache site {site:?} — §7.5's session leak is back",
            );
        }
    }

    /// The measurement (§7.9.1's table, plus wall clocks): distinct clean
    /// contents, one repeated content, distinct broken contents — small and
    /// large helper — with the per-site tally read after each phase and the
    /// outstanding balances read after the document drops.
    #[test]
    fn dependent_edit_measurement() {
        let (dir, entry_path, helper_path) = scratch_package();
        on_big_stack(move || {
            let std_dir = std_root();
            // Warmup: fills std's parses; the entry imports `pkg::`, so no
            // base world is stored for it (the cache's own gate).
            vilan_core::analyzer::set_document_overlay(&helper_path, Some(clean_helper(0)));
            let mut document = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
            for _ in 0..2 {
                let analysis = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
                document.adopt_analysis(analysis);
            }
            assert!(
                document.diagnostics.is_empty(),
                "the fixture must compile clean, got {:?}",
                document.diagnostics
            );

            let report = |label: &str, analyses: usize, wall: Duration| {
                println!(
                    "[m9 {label}] {} over {analyses} analyses: \
                     ParseCleanCacheText {} B, ParseCleanCacheAst {} B, \
                     ModuleErrorText {} B, ModuleErrorAst {} B; \
                     owned gross {} / {} / {} B, owned outstanding {} / {} / {} B; \
                     entry outstanding {} / {} B",
                    per_analysis(wall, analyses),
                    leak_tally::bytes(LeakSite::ParseCleanCacheText),
                    leak_tally::bytes(LeakSite::ParseCleanCacheAst),
                    leak_tally::bytes(LeakSite::ModuleErrorText),
                    leak_tally::bytes(LeakSite::ModuleErrorAst),
                    leak_tally::bytes(LeakSite::OwnedModuleText),
                    leak_tally::bytes(LeakSite::OwnedModuleAst),
                    leak_tally::bytes(LeakSite::OwnedModuleErrors),
                    leak_tally::outstanding(LeakSite::OwnedModuleText),
                    leak_tally::outstanding(LeakSite::OwnedModuleAst),
                    leak_tally::outstanding(LeakSite::OwnedModuleErrors),
                    leak_tally::outstanding(LeakSite::LspEntryText),
                    leak_tally::outstanding(LeakSite::EntryAst),
                );
            };

            // Phase 1: 30 DISTINCT clean contents — 30 landed keystrokes.
            leak_tally::reset();
            let wall = run_phase(
                &mut document,
                &entry_path,
                &helper_path,
                &clean_helper,
                1,
                30,
            );
            report("distinct-clean", 30, wall);
            assert_no_global_cache_growth("distinct-clean");
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                30 * clean_helper(1).len(),
                "each analysis owns exactly one copy of the edited module"
            );

            // Phase 2: 30 re-analyses of ONE content — a repeated buffer.
            leak_tally::reset();
            let wall = run_phase(
                &mut document,
                &entry_path,
                &helper_path,
                &|_| clean_helper(1),
                0,
                30,
            );
            report("repeated-clean", 30, wall);
            assert_no_global_cache_growth("repeated-clean");
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                30 * clean_helper(1).len(),
                "a repeated content is parsed and owned per analysis — the \
                 mechanism's stated, bounded cost"
            );

            // Phase 3: 20 DISTINCT broken contents — the file mid-edit.
            leak_tally::reset();
            let wall = run_phase(
                &mut document,
                &entry_path,
                &helper_path,
                &broken_helper,
                1,
                20,
            );
            assert!(
                !document.diagnostics.is_empty(),
                "a broken helper must surface diagnostics in the importer"
            );
            report("distinct-broken", 20, wall);
            assert_no_global_cache_growth("distinct-broken");
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                20 * broken_helper(1).len(),
                "a broken content's text is owned ONCE — §7.9.1's \
                 pre-cleanliness double-leak stays retired"
            );
            assert!(
                leak_tally::bytes(LeakSite::OwnedModuleErrors) > 0,
                "a broken content's rendered errors are analysis-owned"
            );

            // Phase 4/5: the large helper, distinct then repeated — the
            // timing pair the perf number comes from.
            leak_tally::reset();
            let wall = run_phase(
                &mut document,
                &entry_path,
                &helper_path,
                &large_helper,
                1,
                30,
            );
            assert!(
                document.diagnostics.is_empty(),
                "the large fixture must compile clean, got {:?}",
                document.diagnostics
            );
            report("distinct-clean-large", 30, wall);
            assert_no_global_cache_growth("distinct-clean-large");

            leak_tally::reset();
            let wall = run_phase(
                &mut document,
                &entry_path,
                &helper_path,
                &|_| large_helper(1),
                0,
                30,
            );
            report("repeated-clean-large", 30, wall);
            assert_no_global_cache_growth("repeated-clean-large");

            // The raw parse cost of the module under edit, isolated from the
            // analysis around it: what one overlay-resident import adds to
            // one dependent re-analysis once it is parsed per analysis.
            for (label, text) in [("small", clean_helper(1)), ("large", large_helper(1))] {
                let iterations = 200;
                let parse_start = Instant::now();
                for _ in 0..iterations {
                    let (tree, _errors) = vilan_core::parsing::parse(&text);
                    std::hint::black_box(&tree);
                }
                let wall = parse_start.elapsed();
                println!(
                    "[m9 parse-cost {label}] {} B: {:.3} ms/parse",
                    text.len(),
                    wall.as_secs_f64() * 1000.0 / iterations as f64,
                );
            }

            leak_tally::reset();
            drop(document);
            println!(
                "[m9 after-drop] outstanding: LspEntryText {} B, EntryAst {} B, \
                 OwnedModuleText {} B, OwnedModuleAst {} B, OwnedModuleErrors {} B",
                leak_tally::outstanding(LeakSite::LspEntryText),
                leak_tally::outstanding(LeakSite::EntryAst),
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                leak_tally::outstanding(LeakSite::OwnedModuleAst),
                leak_tally::outstanding(LeakSite::OwnedModuleErrors),
            );

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The core M9 pin, per case: a DISTINCT overlay content per analysis (a
    /// landed keystroke) grows neither process-global cache; each analysis
    /// owns exactly one copy of the edited module; supersession
    /// (`adopt_analysis`) reclaims the previous analysis's copy; and closing
    /// the document nets every owned site to zero.
    #[test]
    fn a_dependent_edits_module_copies_are_analysis_owned_and_reclaimed() {
        let (dir, entry_path, helper_path) = scratch_package();
        on_big_stack(move || {
            let std_dir = std_root();
            // Warm the process-global caches on the DISK content, so the
            // measured window below sees only the overlay-served loads.
            let _ = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);

            let helper_bytes = clean_helper(1).len();
            leak_tally::reset();
            vilan_core::analyzer::set_document_overlay(&helper_path, Some(clean_helper(1)));
            let mut document = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
            assert!(
                document.diagnostics.is_empty(),
                "the overlaid fixture must compile clean, got {:?}",
                document.diagnostics
            );
            for keystroke in 2..=10 {
                vilan_core::analyzer::set_document_overlay(
                    &helper_path,
                    Some(clean_helper(keystroke)),
                );
                let analysis = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
                document.adopt_analysis(analysis);
            }
            assert_no_global_cache_growth("distinct-content pin");
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                10 * helper_bytes,
                "each of the 10 analyses owns exactly one copy of the module text"
            );
            assert!(
                leak_tally::bytes(LeakSite::OwnedModuleAst) > 0,
                "no owned tree was recorded — the pin is vacuous"
            );
            // Supersession reclaimed every previous copy: only the CURRENT
            // analysis's is still out.
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                helper_bytes as isize,
                "adoption must reclaim the superseded analysis's module copy"
            );
            drop(document);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleErrors), 0);

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The error-path pin: a buffer that is mid-edit and does not parse clean
    /// used to leak TWICE per distinct content — `parse_clean_cached`'s
    /// pre-cleanliness text copy (§7.9.1) plus the error cache's own text,
    /// tree, and rendered errors. All of it is analysis-owned now, the
    /// importer still surfaces the module's parse error, and closing the
    /// document reclaims the error slice with the rest.
    #[test]
    fn a_broken_buffers_copies_and_errors_are_owned_and_reclaimed() {
        let (dir, entry_path, helper_path) = scratch_package();
        on_big_stack(move || {
            let std_dir = std_root();
            let _ = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);

            let broken_bytes = broken_helper(1).len();
            leak_tally::reset();
            vilan_core::analyzer::set_document_overlay(&helper_path, Some(broken_helper(1)));
            let mut document = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
            for keystroke in 2..=8 {
                vilan_core::analyzer::set_document_overlay(
                    &helper_path,
                    Some(broken_helper(keystroke)),
                );
                let analysis = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
                document.adopt_analysis(analysis);
            }
            // Behavior parity with the global rich path: the importer names
            // the module's parse error.
            assert!(
                document
                    .diagnostics
                    .iter()
                    .any(|error| error.msg.contains("parse error in")),
                "the broken module's parse error must reach the importer, got {:?}",
                document.diagnostics
            );
            assert_no_global_cache_growth("broken-content pin");
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                8 * broken_bytes,
                "a broken content's text is owned ONCE per analysis — the \
                 §7.9.1 double-leak stays retired"
            );
            assert!(
                leak_tally::bytes(LeakSite::OwnedModuleErrors) > 0,
                "no owned error slice was recorded — the pin is vacuous"
            );
            drop(document);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleErrors), 0);

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The repeated-content pin: the mechanism's honest cost is one parse and
    /// one owned copy per analysis even when the buffer has not changed —
    /// bounded by the open set, reclaimed on supersession, never cached
    /// globally.
    #[test]
    fn a_repeated_content_is_owned_per_analysis_and_reclaimed() {
        let (dir, entry_path, helper_path) = scratch_package();
        on_big_stack(move || {
            let std_dir = std_root();
            let _ = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);

            let helper_bytes = clean_helper(1).len();
            leak_tally::reset();
            vilan_core::analyzer::set_document_overlay(&helper_path, Some(clean_helper(1)));
            let mut document = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
            for _ in 0..4 {
                let analysis = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
                document.adopt_analysis(analysis);
            }
            assert_no_global_cache_growth("repeated-content pin");
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                5 * helper_bytes,
                "a repeated content is parsed and owned per analysis"
            );
            drop(document);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The multi-dependent pin: two open documents importing the edited
    /// module each own their OWN copy — the bound is the open set — and each
    /// copy dies with its document: dropping one leaves the other's analysis
    /// answering from its own, live copy.
    #[test]
    fn each_open_dependent_owns_and_reclaims_its_own_copy() {
        let (dir, entry_path, helper_path) = scratch_package();
        let second_entry_path = dir.join("other.vl");
        const SECOND_ENTRY: &str = "import pkg::helper::value;

fun main() {
	let tripled = value() * 3;
}
";
        std::fs::write(&second_entry_path, SECOND_ENTRY).expect("write other.vl");
        on_big_stack(move || {
            let std_dir = std_root();
            let _ = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);

            let helper_bytes = clean_helper(1).len();
            leak_tally::reset();
            vilan_core::analyzer::set_document_overlay(&helper_path, Some(clean_helper(1)));
            let first = Document::analyze_on_this_thread(ENTRY, &std_dir, &entry_path);
            let second =
                Document::analyze_on_this_thread(SECOND_ENTRY, &std_dir, &second_entry_path);
            assert!(
                first.diagnostics.is_empty() && second.diagnostics.is_empty(),
                "both dependents must compile clean, got {:?} / {:?}",
                first.diagnostics,
                second.diagnostics
            );
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                2 * helper_bytes,
                "each open dependent owns its own copy of the module"
            );
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                2 * helper_bytes as isize,
            );
            drop(first);
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                helper_bytes as isize,
                "dropping one dependent must reclaim exactly its own copy"
            );
            assert!(
                !second.semantic_tokens().is_empty(),
                "the surviving dependent no longer answers from its program"
            );
            drop(second);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The no-dependent pin: an analysis that loads no overlay-served module
    /// — the ordinary open document, whatever unrelated overlays exist —
    /// owns nothing, so the mechanism costs it nothing and the base-world
    /// cache keeps working for it (the store gate reads the same emptiness).
    #[test]
    fn an_analysis_that_loads_no_overlay_module_owns_nothing() {
        let unrelated =
            std::env::temp_dir().join(format!("vilan_m9_unrelated_{}.vl", std::process::id()));
        on_big_stack(move || {
            vilan_core::analyzer::set_document_overlay(&unrelated, Some("x".to_string()));
            leak_tally::reset();
            let document = Document::analyze_on_this_thread(
                "import std::print;

fun main() {
	print(1);
}
",
                &std_root(),
                Path::new("m9_no_dependent.vl"),
            );
            assert!(document.program.is_some(), "the fixture analyzes");
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText)
                    + leak_tally::bytes(LeakSite::OwnedModuleAst)
                    + leak_tally::bytes(LeakSite::OwnedModuleErrors),
                0,
                "an analysis with no overlay-served import must own no module copies"
            );
            drop(document);
            assert_eq!(
                leak_tally::released(LeakSite::OwnedModuleText)
                    + leak_tally::released(LeakSite::OwnedModuleAst)
                    + leak_tally::released(LeakSite::OwnedModuleErrors),
                0,
                "nothing owned, nothing to release"
            );
            vilan_core::analyzer::set_document_overlay(&unrelated, None);
        });
    }
}

// Linux-only, and specifically Linux rather than unix: the harness reads
// resident-set size from `/proc/self/statm`, which Windows does not have (the
// CI run failed with `NotFound`) and macOS does not have either. The E3 Phase-1
// leak claims these pin are Linux-measured — the leak sites themselves are
// platform-independent counters, so a Linux measurement speaks for all hosts;
// what is Linux-only is the *instrument*, not the claim.
#[cfg(all(test, target_os = "linux"))]
mod leak_measurement {
    use super::*;
    use crate::document::tests::{on_big_stack, std_root};
    use vilan_core::leak_tally::{self, LeakSite};

    /// Resident set size in KiB, from /proc/self/statm (Linux pages × 4).
    fn rss_kib() -> usize {
        let statm = std::fs::read_to_string("/proc/self/statm").expect("statm");
        let pages: usize = statm
            .split_whitespace()
            .nth(1)
            .expect("resident field")
            .parse()
            .expect("resident pages");
        pages * 4
    }

    /// The allocator's own split of the heap, from glibc's `mallinfo2`:
    /// `(uordblks, fordblks)` — bytes IN USE (allocated, never freed) and
    /// bytes free but retained. The instrument leak-soak.md §7.7 attributed
    /// the const evaluator's cycles with: in-use bytes growing flat to the
    /// kilobyte across windows are a genuine leak, where RSS confounds it
    /// with allocator retention. `None` off glibc — like `/proc` above, the
    /// gate is about the instrument, not the claim.
    #[cfg(target_env = "gnu")]
    fn heap_split_bytes() -> Option<(isize, isize)> {
        /// glibc's `struct mallinfo2` (malloc.h): ten `size_t` counters.
        #[repr(C)]
        struct MallInfo2 {
            arena: usize,
            ordblks: usize,
            smblks: usize,
            hblks: usize,
            hblkhd: usize,
            usmblks: usize,
            fsmblks: usize,
            uordblks: usize,
            fordblks: usize,
            keepcost: usize,
        }
        unsafe extern "C" {
            fn mallinfo2() -> MallInfo2;
        }
        // SAFETY: mallinfo2 reads allocator statistics and touches nothing
        // else; glibc ≥ 2.33 exports it with exactly this shape.
        let info = unsafe { mallinfo2() };
        Some((info.uordblks as isize, info.fordblks as isize))
    }

    #[cfg(not(target_env = "gnu"))]
    fn heap_split_bytes() -> Option<(isize, isize)> {
        None
    }

    /// The macro-expansion leak sites. analysis-reuse.md §2's fix routes the
    /// stamped `parse_generated` calls through the content cache, so after an
    /// unchanged program's expansions are cached these must PLATEAU — leak zero
    /// further bytes per analysis. (Before the fix, a gensym-stamped expansion
    /// re-leaked its parse every analysis, the true per-keystroke leak.)
    const MACRO_SITES: &[LeakSite] = &[
        LeakSite::MacroParseText,
        LeakSite::MacroParseAst,
        LeakSite::MacroExpansion,
        LeakSite::MacroWorldText,
        LeakSite::MacroWorldProgram,
        LeakSite::MacroWorldAst,
        LeakSite::MacroPreludeText,
        LeakSite::MacroBlockEntryName,
    ];

    fn macro_bytes() -> usize {
        MACRO_SITES.iter().copied().map(leak_tally::bytes).sum()
    }

    /// The subset of the per-site tally a [`LeakReport`] is built from, read on
    /// whichever thread ran the analyses.
    ///
    /// Its own type, rather than fields read inline where the report is built,
    /// because the tally is **thread-local** and one of the two drivers below
    /// gives every analysis its own thread: those counters have to be read
    /// *inside* that thread, before it exits, and summed here. Read after the
    /// join they are zero — which is not "nothing leaked", it is no measurement
    /// at all.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    struct Counts {
        /// GROSS bytes recorded at the entry-text site: one copy of the
        /// analysed source per analysis, whether or not it was later given
        /// back — the file-proportional figure the plateau claims read.
        entry_text: usize,
        /// GROSS bytes recorded at the entry-tree site, likewise.
        entry_ast: usize,
        /// The NET balance at the entry-text site once every `Document` the
        /// window produced has dropped: recorded minus reclaimed. Zero is the
        /// M7 claim (`leak-soak.md` §7); signed because a reclaim may happen
        /// on another thread than the record.
        entry_text_outstanding: isize,
        /// The net balance at the entry-tree site, likewise.
        entry_ast_outstanding: isize,
        display: usize,
        /// The two sites analysis-reuse.md §2 fixes: `parse_generated`'s leaked
        /// source and AST, reached from the stamped expansion paths.
        stamped_parse: usize,
        macro_bytes: usize,
        total: usize,
    }

    impl Counts {
        /// This thread's counters, as they stand.
        fn read() -> Counts {
            Counts {
                entry_text: leak_tally::bytes(LeakSite::LspEntryText),
                entry_ast: leak_tally::bytes(LeakSite::EntryAst),
                entry_text_outstanding: leak_tally::outstanding(LeakSite::LspEntryText),
                entry_ast_outstanding: leak_tally::outstanding(LeakSite::EntryAst),
                display: leak_tally::bytes(LeakSite::DisplayName),
                stamped_parse: leak_tally::bytes(LeakSite::MacroParseText)
                    + leak_tally::bytes(LeakSite::MacroParseAst),
                macro_bytes: macro_bytes(),
                total: leak_tally::total(),
            }
        }

        fn add(&mut self, other: &Counts) {
            self.entry_text += other.entry_text;
            self.entry_ast += other.entry_ast;
            self.entry_text_outstanding += other.entry_text_outstanding;
            self.entry_ast_outstanding += other.entry_ast_outstanding;
            self.display += other.display;
            self.stamped_parse += other.stamped_parse;
            self.macro_bytes += other.macro_bytes;
            self.total += other.total;
        }

        /// The named, by-design per-analysis leaks: the entry source the
        /// `Program` borrows for `'static`, its AST, and a dependency package's
        /// display name. Everything else at every other site is expected to be
        /// zero over a warm window.
        fn named(&self) -> usize {
            self.entry_text + self.entry_ast + self.display
        }
    }

    /// The per-analysis leak counted over the `measured` window, plus the RSS
    /// growth (a noisy report, never asserted on).
    struct LeakReport {
        rss_grown: usize,
        /// `uordblks` growth over the window — bytes allocated during it and
        /// never freed (`None` off glibc). The M8 signal (leak-soak.md
        /// §7.7/§7.8): flat once the const evaluator's cycles are broken.
        in_use_grown: Option<isize>,
        /// `fordblks` growth over the window — freed bytes the allocator kept.
        /// Noise around zero; reported so the RSS number can be read.
        free_retained_grown: Option<isize>,
        entry_text: usize,
        entry_ast: usize,
        entry_text_outstanding: isize,
        entry_ast_outstanding: isize,
        display: usize,
        /// The two sites analysis-reuse.md §2 fixes: `parse_generated`'s leaked
        /// source and AST, reached from the stamped expansion paths.
        stamped_parse: usize,
        macro_bytes: usize,
        total: usize,
        measured: usize,
    }

    impl LeakReport {
        fn from_counts(
            counts: Counts,
            rss_grown: usize,
            heap_grown: Option<(isize, isize)>,
            measured: usize,
        ) -> LeakReport {
            LeakReport {
                rss_grown,
                in_use_grown: heap_grown.map(|(in_use, _)| in_use),
                free_retained_grown: heap_grown.map(|(_, free_retained)| free_retained),
                entry_text: counts.entry_text,
                entry_ast: counts.entry_ast,
                entry_text_outstanding: counts.entry_text_outstanding,
                entry_ast_outstanding: counts.entry_ast_outstanding,
                display: counts.display,
                stamped_parse: counts.stamped_parse,
                macro_bytes: counts.macro_bytes,
                total: counts.total,
                measured,
            }
        }

        fn counts(&self) -> Counts {
            Counts {
                entry_text: self.entry_text,
                entry_ast: self.entry_ast,
                entry_text_outstanding: self.entry_text_outstanding,
                entry_ast_outstanding: self.entry_ast_outstanding,
                display: self.display,
                stamped_parse: self.stamped_parse,
                macro_bytes: self.macro_bytes,
                total: self.total,
            }
        }

        fn print(&self, label: &str) {
            println!(
                "[{label}] RSS +{} KiB ≈ {:.1} KiB/analysis over {} analyses (report only)",
                self.rss_grown,
                self.rss_grown as f64 / self.measured as f64,
                self.measured,
            );
            if let (Some(in_use), Some(free_retained)) =
                (self.in_use_grown, self.free_retained_grown)
            {
                println!(
                    "[{label}] heap in use {in_use:+} B ≈ {:+.1} KiB/analysis; \
                     free-retained {free_retained:+} B over the window (mallinfo2)",
                    in_use as f64 / 1024.0 / self.measured as f64,
                );
            }
            println!(
                "[{label}] counted leak over {} analyses: entry-text {} B, entry-AST {} B, \
                 display {} B, macro {} B, total {} B ≈ {:.0} B/analysis; outstanding after \
                 the documents dropped: entry-text {} B, entry-AST {} B",
                self.measured,
                self.entry_text,
                self.entry_ast,
                self.display,
                self.macro_bytes,
                self.total,
                self.total as f64 / self.measured as f64,
                self.entry_text_outstanding,
                self.entry_ast_outstanding,
            );
        }
    }

    /// How the harness runs one analysis — the two allocation lifetimes the
    /// language server actually has.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Driver {
        /// Inline on the measuring thread, through `analyze_on_this_thread`.
        /// Every fixture that predates the soak uses this: one long-lived
        /// thread, one tally, read once at the end of a window.
        Inline,
        /// One fresh big-stack thread per analysis — the exact shape
        /// [`Document::analyze`] gives the real server, which spawns, runs and
        /// joins a thread per call so the deeply recursive pipeline gets its own
        /// stack (the LSP then wraps *that* in `spawn_blocking`).
        ///
        /// Nothing the compiler caches is thread-local — `BASE_CACHE`, the macro
        /// `WORLDS`/`FAILURES`/`EXPANSIONS`/`PARSES` and `parse_clean_cached` are
        /// process-global `OnceLock<Mutex<…>>`, and the `thread_local!`s in
        /// `analyzer`, `macros`, `call_graph`, `util` and `transformer` are
        /// per-analysis scratch or test counters — so both drivers see the same
        /// warm caches. What differs is that every allocation the analysis makes
        /// belongs to a thread that then *dies*, which returns its arenas to the
        /// allocator on a different schedule. That is the lifetime worth
        /// measuring separately, and it is the one the shipped server has.
        PerAnalysisThread,
    }

    impl Driver {
        fn label(self) -> &'static str {
            match self {
                Driver::Inline => "inline",
                Driver::PerAnalysisThread => "per-thread",
            }
        }
    }

    /// One measured window: `count` analyses of `text_at(i)` over
    /// `start..start + count`, and the bytes they leaked.
    ///
    /// The two drivers accumulate differently and have to: the inline driver
    /// zeroes this thread's counters and reads them once at the end, while the
    /// per-analysis-thread driver reads each thread's own counters before that
    /// thread exits and sums them here. Same total, two mechanisms, because a
    /// thread-local does not outlive its thread.
    fn run_window(
        driver: Driver,
        text_at: &impl Fn(usize) -> String,
        entry: &Path,
        std_dir: &Path,
        start: usize,
        count: usize,
    ) -> Counts {
        match driver {
            Driver::Inline => {
                leak_tally::reset();
                for i in start..start + count {
                    let _ = Document::analyze_on_this_thread(&text_at(i), std_dir, entry);
                }
                Counts::read()
            }
            Driver::PerAnalysisThread => {
                let mut window = Counts::default();
                for i in start..start + count {
                    let text = text_at(i);
                    let std_dir = std_dir.to_path_buf();
                    let entry = entry.to_path_buf();
                    window.add(&on_big_stack(move || {
                        let _ = Document::analyze_on_this_thread(&text, &std_dir, &entry);
                        Counts::read()
                    }));
                }
                window
            }
        }
    }

    /// Runs `warmup` unmeasured analyses, then `windows` disjoint measured
    /// windows of `window` analyses each, reporting every window on its own.
    ///
    /// More than one window is the whole point of the soak, and it is what turns
    /// "the leak is small" into "the leak **plateaus**": two equal-length windows
    /// over an equal-length document must leak the same bytes at every site.
    /// Anything that accumulates — a cache keyed on something that changes per
    /// keystroke, a registry nobody prunes, a per-round retention — makes the
    /// second window larger than the first, and exact integer counters say so
    /// with no threshold, no tolerance and no curve fit. RSS cannot do this job
    /// and is only printed: it is dominated by allocator retention from
    /// rebuilding and dropping the reachable `Program` every call (`leak_tally`'s
    /// own module doc).
    fn measure_windows(
        text_at: impl Fn(usize) -> String,
        entry: &Path,
        driver: Driver,
        warmup: usize,
        window: usize,
        windows: usize,
    ) -> Vec<LeakReport> {
        let std_dir = std_root();
        // Warmup fills every content-addressed cache (the reachable std, the
        // module parses, the macro worlds and their stamped expansions) so the
        // measured windows see only the genuinely per-analysis leaks. It runs
        // inline whichever driver is measuring: the caches it fills are
        // process-global, so which thread fills them is not a distinction.
        for i in 0..warmup {
            let _ = Document::analyze_on_this_thread(&text_at(i), &std_dir, entry);
        }
        let mut reports = Vec::with_capacity(windows);
        for index in 0..windows {
            let before_rss = rss_kib();
            let before_heap = heap_split_bytes();
            let start = warmup + index * window;
            let counts = run_window(driver, &text_at, entry, &std_dir, start, window);
            let heap_grown = before_heap.and_then(|(in_use, free_retained)| {
                heap_split_bytes()
                    .map(|(in_use_now, free_now)| (in_use_now - in_use, free_now - free_retained))
            });
            reports.push(LeakReport::from_counts(
                counts,
                rss_kib().saturating_sub(before_rss),
                heap_grown,
                window,
            ));
        }
        reports
    }

    /// [`measure_windows`] against a synthetic entry in a temp directory, on
    /// the inline driver. Callers must invoke this on a big-stack thread — the
    /// pipeline nests a full analysis inside macro-world compiles.
    fn measure_windows_in_temp(
        text_at: impl Fn(usize) -> String,
        warmup: usize,
        window: usize,
        windows: usize,
    ) -> Vec<LeakReport> {
        let dir = std::env::temp_dir().join(format!("vilan_leak_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let entry = dir.join("main.vl");
        let reports = measure_windows(text_at, &entry, Driver::Inline, warmup, window, windows);
        let _ = std::fs::remove_dir_all(&dir);
        reports
    }

    /// Runs `warmup` then `measured` analyses of `text_at(i)` **on the current
    /// thread** (via `analyze_on_this_thread`, so the leaks land in this
    /// thread's `leak_tally`), zeroing the counters after warmup.
    fn measure(text_at: impl Fn(usize) -> String, warmup: usize, measured: usize) -> LeakReport {
        measure_windows_in_temp(text_at, warmup, measured, 1).remove(0)
    }

    // A changing, std-using document with no macros. Each `i` differs (a
    // keystroke), so every analysis re-parses and re-analyzes.
    fn no_macro_text(i: usize) -> String {
        format!(
            "import std::print;\nimport std::option::Option::{{ self, Some, None }};\n\n\
             fun describe(value: Option<i32>): str {{\n\
             \tmatch value {{\n\t\tSome(let n) => int_to_string(n),\n\t\tNone => \"empty {i}\",\n\t}}\n}}\n\n\
             fun int_to_string(n: i32): str {{\n\t\"n\"\n}}\n\n\
             fun main() {{\n\tlet value = Some({i});\n\tprint(describe(value));\n\tprint(describe(None));\n}}\n"
        )
    }

    // The Phase-1 pin (analysis-reuse.md §2): a changing, std-using document
    // that uses NO macros. After warmup the ONLY per-analysis leaks must be the
    // named, file-size-proportional ones — entry source text and entry AST (no
    // dependency packages, so no display names) — and nothing on the macro path
    // or any other site. RSS is far noisier (allocator retention from rebuilding
    // the reachable `Program`); it is printed, never asserted.
    //
    // And since M7 (leak-soak.md §7) the two named sites are RECLAIMED: every
    // `Document` the window produced was dropped, so the outstanding balance at
    // both is exactly zero — the gross record is still one source copy and one
    // tree per analysis (the window genuinely re-analysed), and none of it is
    // still out.
    #[test]
    fn per_analysis_leak_is_bounded_by_named_sites_and_the_entry_is_reclaimed() {
        let warmup = 20;
        let measured = 200;
        let report = on_big_stack(move || measure(no_macro_text, warmup, measured));
        report.print("no-macro");

        // M7: the entry text and tree of every dropped document were given
        // back — to the byte, because the reclaim releases exactly what the
        // leak recorded.
        assert_eq!(
            report.entry_text_outstanding, 0,
            "{} B of entry text is still out after every document in the window dropped — \
             the superseded analysis's text is not being reclaimed (leak-soak.md §7)",
            report.entry_text_outstanding,
        );
        assert_eq!(
            report.entry_ast_outstanding, 0,
            "{} B of entry tree is still out after every document in the window dropped — \
             the superseded analysis's AST is not being reclaimed (leak-soak.md §7)",
            report.entry_ast_outstanding,
        );

        // The counted per-analysis leak is EXACTLY the named sites — every other
        // leak site (macro path, the content-keyed module parses, the loader's
        // error path) contributed zero over the measured window.
        assert_eq!(
            report.total,
            report.entry_text + report.entry_ast + report.display,
            "an unnamed leak site grew per analysis: total {} B, named {} B",
            report.total,
            report.entry_text + report.entry_ast + report.display,
        );
        assert_eq!(
            report.macro_bytes, 0,
            "a non-macro document leaked {} macro bytes over {} analyses",
            report.macro_bytes, report.measured,
        );
        // The entry source is the dominant named record and is file-
        // proportional: it is exactly the bytes of every analyzed text (each
        // keystroke leaks its own source copy, and — above — gives it back).
        let expected_entry_text: usize = (warmup..warmup + measured)
            .map(|i| no_macro_text(i).len())
            .sum();
        assert_eq!(
            report.entry_text, expected_entry_text,
            "entry-text leak {} B is not the sum of analyzed source lengths {} B",
            report.entry_text, expected_entry_text,
        );
    }

    // A document that defines and invokes an expression-position macro emitting
    // a `fresh()` gensym: its output is `__s<site>_m<N>`-stamped, the path that
    // used to `parse_generated` uncached (analysis-reuse.md §2). `tail` changes
    // per analysis (a keystroke that does not touch the macro), but always four
    // digits — so the length-preserving world blanking maps every analysis to a
    // byte-identical blanked source, and the macro world (which the macro
    // definition living in this file would otherwise recompile on every edit)
    // stays cached. The changing invocation is thus isolated: the only thing
    // that could re-leak on the macro path is the stamped expansion's parse.
    fn gensym_text(tail: usize) -> String {
        format!(
            "import std::print;\n\n\
             macro fun unroll(arguments: Arguments): Source {{\n\
             \timport macro_std::source;\n\
             \timport macro_std::fresh;\n\
             \timport macro_std::meta::{{ Arguments, Source }};\n\
             \timport macro_std::option::Option::{{ self, Some, None }};\n\
             \tlet count = match arguments.as_i32(0) {{\n\t\tSome(let n) => n,\n\t\tNone => 0,\n\t}};\n\
             \tlet binder = fresh();\n\
             \tmut sum = \"0\";\n\
             \tmut index = 0;\n\
             \tfor index < count {{\n\t\tsum = sum + i\" + {{binder}}({{index}})\";\n\t\tindex = index + 1;\n\t}}\n\
             \tsource(i\"\\{{ let {{binder}} = (|x: i32| x + 1); {{sum}} \\}}\")\n\
             }}\n\n\
             fun main() {{\n\tlet unrolled = macro unroll(4);\n\tprint(unrolled);\n\tlet tail = {tail:04};\n\tprint(tail);\n}}\n\n\
             main();\n"
        )
    }

    // The gensym plateau — the leak analysis-reuse.md §2 actually fixes. Before
    // the fix the stamped expression parse re-leaked every analysis; after, the
    // content cache (keyed on the site-stamped text) makes it plateau to zero.
    #[test]
    fn gensym_expansion_leak_plateaus() {
        // `tail` stays four digits so the blanked world source is byte-stable.
        let warmup = 8;
        let measured = 40;
        let report = on_big_stack(move || {
            let report = measure(|i| gensym_text(1000 + i), warmup, measured);
            for site in MACRO_SITES {
                println!("[gensym] {:?} = {} B", site, leak_tally::bytes(*site));
            }
            report
        });
        report.print("gensym");

        // The §2-fixed sites: the stamped expression parse is content-cached, so
        // the warm, unchanged invocation re-leaks nothing.
        assert_eq!(
            report.stamped_parse, 0,
            "the gensym expansion's stamped parse re-leaked {} B over {} analyses — \
             `parse_generated` is not being content-cached (analysis-reuse.md §2)",
            report.stamped_parse, report.measured,
        );
        // With the world cached (fixed-width tail) the WHOLE macro path plateaus.
        assert_eq!(
            report.macro_bytes, 0,
            "the macro path re-leaked {} B over {} analyses (see the per-site breakdown above)",
            report.macro_bytes, report.measured,
        );
        // The entry still leaks per analysis (the changing tail), so the fixture
        // genuinely re-analyzes each round rather than short-circuiting.
        assert!(
            report.entry_text > 0,
            "the changing gensym fixture leaked no entry text — it may not be re-analyzing",
        );
    }

    // The E23 pin: the SAME macro-defining program under a keystroke that
    // CHANGES THE FILE'S LENGTH every analysis (a growing trailing comment)
    // without touching the macro definition. Blanking preserves length, so
    // before the fix every analysis produced a distinct blanked source, missed
    // the world cache, and re-leaked a full world (`MacroWorldText` +
    // `MacroWorldProgram`) — the leak the gensym fixture's fixed-width tail
    // deliberately dodges. The world key hashes only the definition segments,
    // so the world compiled during warmup must serve every subsequent
    // analysis: the whole macro path plateaus.
    #[test]
    fn world_leak_plateaus_under_length_changing_edits() {
        let warmup = 8;
        let measured = 40;
        let report = on_big_stack(move || {
            let report = measure(
                |i| format!("{}// {}\n", gensym_text(1000), "x".repeat(i)),
                warmup,
                measured,
            );
            for site in MACRO_SITES {
                println!(
                    "[length-changing] {:?} = {} B",
                    site,
                    leak_tally::bytes(*site)
                );
            }
            report
        });
        report.print("length-changing");

        assert_eq!(
            report.macro_bytes, 0,
            "a length-changing edit outside the macro definition re-leaked {} B on the \
             macro path over {} analyses — the world cache is keyed on the file's \
             layout, not the definitions' content (backlog E23; see the per-site \
             breakdown above)",
            report.macro_bytes, report.measured,
        );
        assert!(
            report.entry_text > 0,
            "the length-changing fixture leaked no entry text — it may not be re-analyzing",
        );
    }

    // A macro whose WORLD does not compile (its body calls an undefined name).
    // Failures never reached the world cache — only `Ok` worlds were inserted —
    // so a buffer holding a broken macro definition re-leaked the world's text
    // per analysis, even UNEDITED. The failure cache (keyed on the definition
    // segments AND their offsets, so the cached diagnostics' spans stay true)
    // must make the macro path plateau while the diagnostics keep being
    // reported. The tail edit stays BELOW the definition: the offsets hold, so
    // the cached failure stays valid. (An edit that MOVES a still-broken
    // definition recompiles once per layout — recorded, accepted.)
    #[test]
    fn broken_world_failure_plateaus_without_releaking() {
        fn broken_macro_text(i: usize) -> String {
            format!(
                "import std::print;\n\n\
                 macro fun broken(arguments: Arguments): Source {{\n\
                 \timport macro_std::source;\n\
                 \timport macro_std::meta::{{ Arguments, Source }};\n\
                 \tsource(undefined_name())\n\
                 }}\n\n\
                 fun main() {{\n\tlet x = macro broken(1);\n\tprint(x);\n}}\n\n\
                 main();\n// {}\n",
                "x".repeat(i)
            )
        }
        let warmup = 8;
        let measured = 40;
        let report = on_big_stack(move || {
            let report = measure(broken_macro_text, warmup, measured);
            for site in MACRO_SITES {
                println!("[broken-world] {:?} = {} B", site, leak_tally::bytes(*site));
            }
            report
        });
        report.print("broken-world");

        assert_eq!(
            report.macro_bytes, 0,
            "a broken macro definition re-leaked {} B on the macro path over {} \
             analyses — world-compile failures are not being cached (backlog E23; \
             see the per-site breakdown above)",
            report.macro_bytes, report.measured,
        );
        assert!(
            report.entry_text > 0,
            "the broken-world fixture leaked no entry text — it may not be re-analyzing",
        );
    }

    // A const-heavy document: every cycle shape leak-soak.md §7.7 names —
    // hoisted world functions (a root-scope cycle per `const` site), a closure
    // declared inside a called function's body (a call-scope cycle, the shape
    // the root-only experiment could not reach), and loop iterations between
    // them — with list results fat enough that a stranded root scope holds
    // real bytes. Written for `moving_edit`, so every analysis is a distinct
    // content of identical length.
    const CONST_HEAVY_BASE: &str = "import std::print;\n\n\
         fun labels(count: i32): List<str> {\n\
         \tlet describe = |index: i32| { \"a labelled entry in the fixture\" };\n\
         \tmut result: List<str> = List::new();\n\
         \tmut index = 0;\n\
         \tfor index < count {\n\t\tresult.push(describe(index));\n\t\tindex = index + 1;\n\t}\n\
         \tresult\n\
         }\n\n\
         fun total(count: i32): i32 {\n\
         \tmut sum = 0;\n\
         \tmut index = 0;\n\
         \tfor index < count {\n\t\tsum = sum + index;\n\t\tindex = index + 1;\n\t}\n\
         \tsum\n\
         }\n\n\
         let NAMES: List<str> = const labels(12);\n\
         let MORE: List<str> = const labels(18);\n\
         let SUM: i32 = const total(15);\n\
         let AGAIN: i32 = const total(24);\n\
         let LAST: List<str> = const labels(9);\n\n\
         fun main() {\n\
         \tprint(NAMES.len());\n\tprint(MORE.len());\n\tprint(SUM);\n\tprint(AGAIN);\n\tprint(LAST.len());\n\
         }\n";

    // The M8 pin (leak-soak.md §7.8), in §7.7's mallinfo2 harness shape: a
    // const-heavy document's IN-USE bytes — allocated and never freed, the
    // counter that cannot confuse a leak with allocator retention — must be
    // flat window over window. Before the per-run scope registry, every
    // `const` site of every analysis stranded its root scope behind a
    // closure–scope `Rc` cycle — measured with the teardown planted out:
    // +8.4 KiB of in-use heap per analysis on this fixture, flat across both
    // windows, exactly a leak's signature (+1,523.9 KiB per analysis on
    // `vilan-website/src/page.vl`) — so in-use bytes grew linearly in
    // keystrokes. The exact half of the pin
    // is the scope counter: interpreter scopes created minus dropped on the
    // measuring thread is zero once its runs are done, on any platform and
    // under any test runner. The byte half is asserted only where the
    // instrument exists (glibc), with a cap far under the broken rate and far
    // over warm-window noise.
    #[test]
    fn const_evaluations_in_use_bytes_plateau() {
        let warmup = 8;
        let window = 75;
        let (reports, scopes_alive) = on_big_stack(move || {
            let reports =
                measure_windows_in_temp(|i| moving_edit(CONST_HEAVY_BASE, i), warmup, window, 2);
            (reports, vilan_core::interpreter::live_scope_count())
        });
        for (index, report) in reports.iter().enumerate() {
            report.print(&format!("const-heavy w{}", index + 1));
        }
        assert!(
            reports[0].entry_text > 0,
            "the const-heavy fixture leaked no entry text — it may not be re-analyzing",
        );
        assert_eq!(
            scopes_alive, 0,
            "{scopes_alive} interpreter scope(s) outlived their runs on the measuring thread — \
             the const evaluator's cycles are stranding scopes again (leak-soak.md §7.8)",
        );
        // `uordblks` is process-global, so the byte half asserts only under
        // nextest's process-per-test isolation — under `cargo test`'s
        // in-process threads a neighbouring test's live allocations would land
        // in the window, which is exactly why RSS has always been report-only
        // here. The scope counter above is thread-local and gates everywhere.
        let isolated = std::env::var_os("NEXTEST").is_some();
        match reports[1].in_use_grown {
            Some(in_use) if isolated => {
                let per_analysis = in_use / window as isize;
                assert!(
                    per_analysis < 2048,
                    "the warm window grew {per_analysis} B of in-use heap per analysis over \
                     {window} analyses ({in_use} B) — allocated-and-never-freed bytes should be \
                     flat with the const evaluator's scopes torn down (leak-soak.md §7.8)",
                );
            }
            Some(in_use) => println!(
                "[const-heavy] shared-process run — in-use growth {in_use} B reported, not \
                 asserted (the scope counter above still gates)"
            ),
            None => println!(
                "[const-heavy] mallinfo2 unavailable — the in-use cap was not asserted \
                 (the scope counter above still gates)"
            ),
        }
    }

    // --- The soak: real corpora, thousands of analyses (proposal/leak-soak.md) --

    /// The sibling-repository corpora, addressed by environment variable and
    /// **skipped, never failed**, when absent — the same two variables the
    /// `perf_baseline` module beside this one reads, so one export serves both
    /// harnesses and both speak about the same two files. They live in checkouts
    /// a fresh clone of this repository does not have.
    const SOAK_CORPORA: &[(&str, &str, &str)] = &[
        ("kolt_views", "VILAN_PERF_KOLT", "src/views.vl"),
        ("website_page", "VILAN_PERF_WEBSITE", "src/page.vl"),
    ];

    /// Unmeasured analyses before the first measured window. Larger than the
    /// synthetic fixtures' 8–20 because a real package drags in real modules:
    /// every one of them has to be parsed, resolved and (for the derives) macro-
    /// expanded into its content-addressed cache before "warm" is true.
    const SOAK_WARMUP: usize = 10;

    /// How many analyses one measured window runs. Two windows per driver, so
    /// the default soak is 2,000 analyses of each corpus on the inline driver —
    /// "thousands", where the shipped fixtures do tens and hundreds.
    ///
    /// Overridable with `VILAN_LEAK_SOAK_WINDOW` because the honest answer to
    /// "how long should a soak run" is "longer than you think, but you are the
    /// one waiting": a 735-line file costs ~320 ms an analysis in release
    /// (`perf-baseline.md` §2.3), so 2,000 of them is ~11 minutes and the same
    /// run in debug is most of an hour.
    fn soak_window() -> usize {
        std::env::var("VILAN_LEAK_SOAK_WINDOW")
            .ok()
            .and_then(|value| value.parse().ok())
            .filter(|window| *window > 0)
            .unwrap_or(1000)
    }

    /// A real file under a **moving single-character edit**: a fixed-width
    /// trailing comment carrying one `x` that walks one column per iteration and
    /// wraps.
    ///
    /// Three properties, each load-bearing:
    ///
    /// - **Every iteration is a distinct content**, so nothing is served from
    ///   `parse_clean_cached` and every analysis is a genuine re-analysis rather
    ///   than a cache hit wearing one.
    /// - **Every iteration is the same LENGTH**, which is what makes the plateau
    ///   assertion exact instead of statistical: the entry-text leak over a
    ///   window of N analyses is exactly N × the file's bytes, and two windows of
    ///   N must then match to the byte.
    /// - **A trailing comment is valid in every file**, so one mutation works on
    ///   any corpus without knowing anything about it — the same argument the
    ///   `perf_baseline` module makes for its own trailing-comment keystroke.
    ///
    /// It deliberately edits nothing the analyzer resolves. The question a soak
    /// asks is what a keystroke costs the *process* over thousands of rounds,
    /// not what one particular edit costs the type solver; a `perf_baseline`
    /// row already answers the second.
    fn moving_edit(base: &str, i: usize) -> String {
        const TRACK: usize = 64;
        let column = i % TRACK;
        format!(
            "{base}\n// {}x{}\n",
            " ".repeat(column),
            " ".repeat(TRACK - 1 - column),
        )
    }

    /// One window as a machine-readable row, the shape `perf_baseline`'s `PERF`
    /// lines have, so a soak run greps into a file that diffs against the next
    /// one.
    fn soak_row(
        corpus: &str,
        lines: usize,
        source_bytes: usize,
        driver: Driver,
        window_index: usize,
        report: &LeakReport,
    ) {
        let json_or_null = |value: Option<isize>| {
            value.map_or_else(|| "null".to_string(), |value| value.to_string())
        };
        println!(
            "LEAK {{\"corpus\":\"{corpus}\",\"lines\":{lines},\"source_bytes\":{source_bytes},\
             \"driver\":\"{}\",\"window\":{window_index},\"analyses\":{},\"entry_text_b\":{},\
             \"entry_ast_b\":{},\"display_b\":{},\"macro_b\":{},\"total_b\":{},\
             \"bytes_per_analysis\":{},\"entry_text_outstanding_b\":{},\
             \"entry_ast_outstanding_b\":{},\"rss_grown_kib\":{},\"in_use_grown_b\":{},\
             \"free_retained_grown_b\":{}}}",
            driver.label(),
            report.measured,
            report.entry_text,
            report.entry_ast,
            report.display,
            report.macro_bytes,
            report.total,
            report.total / report.measured,
            report.entry_text_outstanding,
            report.entry_ast_outstanding,
            report.rss_grown,
            json_or_null(report.in_use_grown),
            json_or_null(report.free_retained_grown),
        );
    }

    /// The soak's tier 1 (`leak-soak.md` §2): the shipped plateau assertion, run
    /// against real application files instead of synthetic ones, for thousands
    /// of keystrokes instead of tens, through both of the server's allocation
    /// lifetimes.
    ///
    /// `#[ignore]`d because it is minutes to hours of measurement; the cheap
    /// fixtures above stay in the gate and keep asserting the same invariant on
    /// the shapes that can be asserted in seconds.
    ///
    /// Each corpus is measured in two disjoint equal windows and the windows are
    /// compared **to the byte**. That comparison is the finding, not a
    /// threshold: a per-analysis leak that is by design (the entry source the
    /// `Program` borrows for `'static`, its AST) contributes the same bytes to
    /// both windows, while anything that *accumulates* contributes more to the
    /// second. RSS is printed beside it and asserted on nowhere, for the reason
    /// `leak_tally`'s module doc gives.
    ///
    /// M12: it REFUSES when it measured nothing. Both corpora live in sibling
    /// checkouts behind environment variables nothing in this tree sets, and the
    /// old body `continue`d past each with a skip line — so the default outcome
    /// of running the soak was a PASS in milliseconds with zero assertions
    /// against its own "thousands of analyses" charter, and "I ran the soak" and
    /// "the soak ran" were the same green. Nothing was left unchecked by that
    /// (the synthetic fixtures above hold the same invariant on every run), so
    /// the fix is not to weaken the gate but to stop the soak claiming a verdict
    /// it does not have: invoking it — and it is `#[ignore]`d, so invoking it is
    /// always deliberate — with no corpus present is a mistake in the invocation
    /// and says so, naming the variables to set.
    #[test]
    #[ignore = "the leak soak: thousands of analyses per corpus, run deliberately (proposal/leak-soak.md §5)"]
    fn leak_soak_corpus_plateaus() {
        soak_corpora(SOAK_CORPORA);
    }

    /// The soak's body, over whatever corpus table it is given, so the
    /// no-corpus refusal below is reachable from a test that costs microseconds
    /// instead of hours.
    fn soak_corpora(corpora: &[(&str, &str, &str)]) {
        let mut measured_corpora = 0usize;
        for &(corpus, variable, relative) in corpora {
            let Some(root) = std::env::var_os(variable).map(PathBuf::from) else {
                println!("LEAK-SKIP {corpus}: {variable} is not set");
                continue;
            };
            let entry = root.join(relative);
            let Ok(base) = std::fs::read_to_string(&entry) else {
                println!("LEAK-SKIP {corpus}: {} is not readable", entry.display());
                continue;
            };
            let lines = base.lines().count();
            let source_bytes = moving_edit(&base, 0).len();
            measured_corpora += 1;

            for driver in [Driver::Inline, Driver::PerAnalysisThread] {
                let window = match driver {
                    Driver::Inline => soak_window(),
                    // A quarter of the window on the per-thread driver, stated
                    // rather than tuned by feel: what it has to support is the
                    // same plateau claim, and a plateau is proven by two windows
                    // being EQUAL, not by their length. Its extra cost over the
                    // inline driver is one 256 MiB-stack thread spawn and join
                    // per analysis — small beside a real file's analysis, but
                    // paid thousands of times.
                    Driver::PerAnalysisThread => (soak_window() / 4).max(25),
                };
                let text = base.clone();
                let subject = entry.clone();
                let reports = on_big_stack(move || {
                    measure_windows(
                        move |i| moving_edit(&text, i),
                        &subject,
                        driver,
                        SOAK_WARMUP,
                        window,
                        2,
                    )
                });
                for (index, report) in reports.iter().enumerate() {
                    report.print(&format!("{corpus} {} w{}", driver.label(), index + 1));
                    soak_row(corpus, lines, source_bytes, driver, index + 1, report);
                }

                for (index, report) in reports.iter().enumerate() {
                    assert_eq!(
                        report.total,
                        report.counts().named(),
                        "{corpus} ({}) window {}: an unnamed leak site grew over {} analyses \
                         — total {} B, named sites {} B (macro path {} B)",
                        driver.label(),
                        index + 1,
                        report.measured,
                        report.total,
                        report.counts().named(),
                        report.macro_bytes,
                    );
                }
                assert_eq!(
                    reports[0].entry_text,
                    window * source_bytes,
                    "{corpus} ({}): the entry-text leak is not one copy of the {source_bytes}-byte \
                     source per analysis over {window} analyses",
                    driver.label(),
                );
                // M7 (leak-soak.md §7): every one of those copies, and every
                // tree, was given back when its document dropped — on both
                // drivers, the per-thread one included (each analysis thread
                // drops its own document before it reads its counters).
                for (index, report) in reports.iter().enumerate() {
                    assert_eq!(
                        (report.entry_text_outstanding, report.entry_ast_outstanding),
                        (0, 0),
                        "{corpus} ({}) window {}: {} B of entry text and {} B of entry tree are \
                         still out after {} analyses whose documents all dropped — the session \
                         leak M7 fixed is back",
                        driver.label(),
                        index + 1,
                        report.entry_text_outstanding,
                        report.entry_ast_outstanding,
                        report.measured,
                    );
                }
                assert_eq!(
                    reports[1].counts(),
                    reports[0].counts(),
                    "{corpus} ({}): the second window of {window} analyses did not leak what the \
                     first did — something is accumulating across keystrokes, which is the leak \
                     this soak exists to find",
                    driver.label(),
                );
            }
        }

        if let Some(refusal) = soak_refusal(corpora, measured_corpora) {
            panic!("{refusal}");
        }
    }

    /// The soak's verdict on its own inputs (M12): `Some(message)` when it
    /// measured NOTHING, `None` when at least one corpus ran. A partial run is
    /// deliberately allowed — one corpus present is still thousands of analyses
    /// of a real file, and the plateau claim is per corpus — so this is the only
    /// case where the soak has no finding at all to report.
    ///
    /// The message names every variable, and where each one points, because the
    /// failure is in the INVOCATION and the reader is the person who just typed
    /// it: what they need is the export they forgot, not a diagnosis.
    fn soak_refusal(corpora: &[(&str, &str, &str)], measured_corpora: usize) -> Option<String> {
        if measured_corpora > 0 {
            return None;
        }
        let addresses = corpora
            .iter()
            .map(|(corpus, variable, relative)| {
                format!("{variable}=<checkout> for `{corpus}` ({relative})")
            })
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "the leak soak measured NO corpus, so it asserted nothing — it did not pass, it did \
             not run. Every corpus lives in a sibling checkout addressed by an environment \
             variable, and none was set or readable: set {addresses}. (The synthetic fixtures in \
             this module hold the same plateau invariant on every suite run; this test exists to \
             hold it over thousands of analyses of a real file, which needs the real file.)"
        ))
    }

    /// M12's pin on the verdict: a soak that measured nothing refuses, and the
    /// refusal names every variable the invoker was missing — the whole point,
    /// since a message that only said "no corpus" would leave them where the
    /// silent skip lines already left them. One corpus measured is enough, so
    /// this never turns a partial soak red.
    #[test]
    fn a_soak_that_measured_no_corpus_refuses_and_names_its_variables() {
        let refusal =
            soak_refusal(SOAK_CORPORA, 0).expect("measuring no corpus at all must refuse");
        for &(_, variable, _) in SOAK_CORPORA {
            assert!(
                refusal.contains(variable),
                "the refusal must name {variable}, the export the invoker is missing: {refusal}"
            );
        }
        assert!(
            soak_refusal(SOAK_CORPORA, 1).is_none(),
            "one corpus measured is a real soak — a partial run must not be refused"
        );
        assert!(
            soak_refusal(SOAK_CORPORA, SOAK_CORPORA.len()).is_none(),
            "every corpus measured must not be refused"
        );
    }

    /// …and the pin that the soak BODY takes that verdict, which the pure one
    /// above cannot say. Driven through a corpus addressed by a variable nothing
    /// sets, so the body skips it exactly as it skipped the real two and reaches
    /// the refusal without measuring anything: microseconds, where the soak
    /// proper is hours. Without this, `soak_refusal` could be perfect and never
    /// called — which is the shape of the defect M12 filed.
    #[test]
    fn the_soak_body_refuses_when_every_corpus_is_absent() {
        const ABSENT: &[(&str, &str, &str)] = &[(
            "absent_for_the_pin",
            "VILAN_PERF_ABSENT_CORPUS_FOR_THE_M12_PIN",
            "src/main.vl",
        )];
        assert!(
            std::env::var_os(ABSENT[0].1).is_none(),
            "{} must be unset for this pin to mean anything",
            ABSENT[0].1
        );
        let outcome = std::panic::catch_unwind(|| soak_corpora(ABSENT));
        let payload = outcome
            .expect_err("a soak over an absent corpus measured nothing and must refuse, not pass");
        let message = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&str>().copied())
            .unwrap_or("<non-string panic payload>");
        assert!(
            message.contains(ABSENT[0].1) && message.contains("measured NO corpus"),
            "the body's refusal must be the one `soak_refusal` composed: {message}"
        );
    }

    /// The gate's pin on the soak harness: a handful of analyses through BOTH
    /// drivers, asserting they agree to the byte and that two equal windows
    /// plateau. Seconds, not minutes.
    ///
    /// Not a small copy of the heavy soak — it is the pin that the heavy soak's
    /// *instrument* works, and specifically the one the per-analysis-thread
    /// driver cannot do without. Read that driver's tally after the join instead
    /// of inside the thread and every count comes back zero, which reads exactly
    /// like a perfect plateau; the equality against the inline driver is what
    /// makes shipping that impossible.
    #[test]
    fn leak_soak_harness_smoke() {
        let base = no_macro_text(0);
        let source_bytes = moving_edit(&base, 0).len();
        let mut per_driver = Vec::new();
        for driver in [Driver::Inline, Driver::PerAnalysisThread] {
            let text = base.clone();
            let reports = on_big_stack(move || {
                let dir = std::env::temp_dir().join(format!(
                    "vilan_leak_soak_smoke_{}_{}",
                    std::process::id(),
                    driver.label()
                ));
                let _ = std::fs::remove_dir_all(&dir);
                std::fs::create_dir_all(&dir).unwrap();
                let entry = dir.join("main.vl");
                let reports =
                    measure_windows(move |i| moving_edit(&text, i), &entry, driver, 2, 2, 2);
                let _ = std::fs::remove_dir_all(&dir);
                reports
            });
            for (index, report) in reports.iter().enumerate() {
                report.print(&format!("soak-smoke {} w{}", driver.label(), index + 1));
            }
            assert_eq!(
                reports[0].entry_text,
                2 * source_bytes,
                "the {} driver did not tally one copy of the {source_bytes}-byte source per \
                 analysis — a driver that reports nothing reports a clean plateau",
                driver.label(),
            );
            assert_eq!(
                (
                    reports[0].entry_text_outstanding,
                    reports[0].entry_ast_outstanding
                ),
                (0, 0),
                "the {} driver left entry text or tree outstanding after its documents dropped \
                 — the reclaim (leak-soak.md §7) is not reaching this driver's lifetime",
                driver.label(),
            );
            assert_eq!(
                reports[1].counts(),
                reports[0].counts(),
                "the {} driver's two equal windows did not leak equally",
                driver.label(),
            );
            per_driver.push(reports[0].counts());
        }
        assert_eq!(
            per_driver[1], per_driver[0],
            "the per-analysis-thread driver must tally exactly what the inline driver tallies — \
             the same analyses ran, only the thread they ran on differs",
        );
    }
}

/// The edit-latency half of the performance baseline (`proposal/perf-baseline.md`).
///
/// `leak_measurement` above answers "does a keystroke leak"; this answers "what
/// does a keystroke *cost*", over the same loop and through the same entry
/// point (`Document::analyze_on_this_thread`, which is why the section lives
/// here rather than in the CLI's harness — it is private to this crate, and a
/// benchmark is not a reason to widen it). A tail latency, not a mean: an
/// editor is judged by the keystroke that stalls, so the row is p50/p95/p99.
///
/// Not `target_os = "linux"`-gated, unlike its neighbor — that gate is about
/// `/proc/self/statm`, and a clock is everywhere.
///
/// Run it (with the CLI half, one command, `perf-baseline.md` §3):
///
/// ```text
/// cargo nextest run --release --workspace --run-ignored ignored-only \
///     -E 'test(perf_baseline)' --no-capture > perf.log 2>&1
/// ```
#[cfg(test)]
mod perf_baseline {
    use super::*;
    use crate::document::tests::std_root;
    use std::time::{Duration, Instant};

    /// Which build measured the row. A debug-profile number is a fact about
    /// `-O0`, not about the compiler a user installs, and a baseline that does
    /// not say which it is invites exactly that confusion.
    fn profile() -> &'static str {
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    }

    /// Nearest-rank percentile over sorted samples.
    fn percentile(sorted: &[Duration], fraction: f64) -> f64 {
        let rank = (fraction * sorted.len() as f64).ceil().max(1.0) as usize;
        sorted[rank.min(sorted.len()) - 1].as_secs_f64() * 1000.0
    }

    /// The same `PERF {…}` line shape the CLI harness emits, so a run's rows
    /// from both binaries concatenate into one diffable summary.
    fn report(corpus: &str, note: &str, samples: &mut Vec<Duration>) {
        samples.sort_unstable();
        let milliseconds = |duration: Duration| duration.as_secs_f64() * 1000.0;
        println!(
            "PERF {{\"section\":\"lsp_edit\",\"corpus\":\"{}\",\"mode\":\"warm\",\
             \"metric\":\"analyze\",\"profile\":\"{}\",\"runs\":{},\"min_ms\":{:.2},\
             \"median_ms\":{:.2},\"p95_ms\":{:.2},\"p99_ms\":{:.2},\"max_ms\":{:.2},\
             \"note\":\"{}\"}}",
            corpus,
            profile(),
            samples.len(),
            milliseconds(samples[0]),
            percentile(samples, 0.50),
            percentile(samples, 0.95),
            percentile(samples, 0.99),
            milliseconds(*samples.last().expect("at least one sample")),
            note,
        );
    }

    /// Runs `warmup` unmeasured then `measured` timed analyses of `text_at(i)`,
    /// each a distinct document (a keystroke), and returns the per-analysis
    /// wall times.
    ///
    /// The mode is **warm** and could not honestly be anything else: this is
    /// the editor's steady state, where the resolved base world is already in
    /// the process and only the entry is new. The cold shape — a first analysis
    /// after the server starts — is what the CLI harness's `cold` rows measure.
    /// The warmup is what makes the distinction real rather than assumed
    /// (`suite-speed.md` §2.1/E26): without it the first samples carry the whole
    /// std resolve and the percentile is a mixture of two populations.
    fn measure(
        text_at: impl Fn(usize) -> String,
        entry: &Path,
        warmup: usize,
        measured: usize,
    ) -> Vec<Duration> {
        let std_dir = std_root();
        for i in 0..warmup {
            let _ = Document::analyze_on_this_thread(&text_at(i), &std_dir, entry);
        }
        let mut samples = Vec::with_capacity(measured);
        for i in warmup..warmup + measured {
            let text = text_at(i);
            let started = Instant::now();
            let _ = Document::analyze_on_this_thread(&text, &std_dir, entry);
            samples.push(started.elapsed());
        }
        samples
    }

    fn on_big_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
        std::thread::Builder::new()
            .stack_size(256 * 1024 * 1024)
            .spawn(work)
            .expect("spawn the latency measurement thread")
            .join()
            .expect("the latency measurement thread panicked")
    }

    /// The synthetic subject: a std-using document with no macros, one
    /// character different per iteration. Deliberately the same fixture
    /// `leak_measurement` uses, so the leak plateau and the latency curve are
    /// statements about one document.
    fn synthetic_text(i: usize) -> String {
        format!(
            "import std::print;\nimport std::option::Option::{{ self, Some, None }};\n\n\
             fun describe(value: Option<i32>): str {{\n\
             \tmatch value {{\n\t\tSome(let n) => int_to_string(n),\n\t\tNone => \"empty {i}\",\n\t}}\n}}\n\n\
             fun int_to_string(n: i32): str {{\n\t\"n\"\n}}\n\n\
             fun main() {{\n\tlet value = Some({i});\n\tprint(describe(value));\n\tprint(describe(None));\n}}\n"
        )
    }

    /// A real package file, edited the way the broken-world fixture above edits
    /// its own: a trailing comment that changes every iteration. It is a whole
    /// re-analysis either way — the entry is never served from a content cache
    /// once its bytes move — and a trailing comment is the one edit that is
    /// valid in every file, so the same mutation works on any corpus.
    fn corpus_text(base: &str, i: usize) -> String {
        format!("{base}\n// keystroke {i}\n")
    }

    /// The sibling-repository corpora, addressed by environment variable and
    /// skipped when absent (`perf-baseline.md` §1): `VILAN_PERF_KOLT`,
    /// `VILAN_PERF_WEBSITE`.
    const CORPORA: &[(&str, &str, &str)] = &[
        ("kolt_views", "VILAN_PERF_KOLT", "src/views.vl"),
        ("website_page", "VILAN_PERF_WEBSITE", "src/page.vl"),
    ];

    #[test]
    #[ignore = "the performance baseline: minutes of measurement, run deliberately (proposal/perf-baseline.md §3)"]
    fn perf_baseline_lsp_edit_latency() {
        let samples = on_big_stack(|| {
            let directory =
                std::env::temp_dir().join(format!("vilan_perf_latency_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("create the fixture directory");
            let entry = directory.join("main.vl");
            let samples = measure(synthetic_text, &entry, 50, 2000);
            let _ = std::fs::remove_dir_all(&directory);
            samples
        });
        let mut samples = samples;
        report("synthetic", "15 lines, no macros", &mut samples);

        for (name, variable, relative) in CORPORA {
            let Some(root) = std::env::var_os(variable).map(PathBuf::from) else {
                println!("PERF-SKIP {name}: {variable} is not set");
                continue;
            };
            let entry = root.join(relative);
            let Ok(base) = std::fs::read_to_string(&entry) else {
                println!("PERF-SKIP {name}: {} is not readable", entry.display());
                continue;
            };
            let lines = base.lines().count();
            // Fewer iterations than the synthetic subject, and the reason is
            // recorded rather than tuned by feel: each analysis leaks its entry
            // text and AST (the known, named leak `leak_measurement` bounds), so
            // a 25 KB file at 2000 keystrokes is hundreds of megabytes of
            // deliberate garbage — and at a real file's per-keystroke cost that
            // many iterations is most of an hour. 100 is enough for p50/p95 and
            // is reported with its `runs` so a reader can judge the p99 for
            // themselves.
            let mut samples =
                on_big_stack(move || measure(move |i| corpus_text(&base, i), &entry, 10, 100));
            report(name, &format!("{lines} lines"), &mut samples);
        }
    }

    /// The gate's pin on this half of the harness: it runs and produces an
    /// ordered, non-empty sample set. A handful of analyses, seconds.
    #[test]
    fn perf_baseline_lsp_harness_smoke() {
        let mut samples = on_big_stack(|| {
            let directory = std::env::temp_dir()
                .join(format!("vilan_perf_latency_smoke_{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&directory);
            std::fs::create_dir_all(&directory).expect("create the fixture directory");
            let entry = directory.join("main.vl");
            let samples = measure(synthetic_text, &entry, 1, 3);
            let _ = std::fs::remove_dir_all(&directory);
            samples
        });
        assert_eq!(samples.len(), 3, "the harness measured the wrong count");
        report("synthetic", "smoke", &mut samples);
        assert!(
            samples.windows(2).all(|pair| pair[0] <= pair[1]),
            "reporting did not leave the samples sorted, so the percentiles are not percentiles",
        );
        assert!(
            samples[0] > Duration::ZERO,
            "an analysis measured zero time — the clock is not measuring the work",
        );
    }

    /// The pin on the statistic itself, over known samples. Three measured
    /// analyses cannot tell a p95 from a p5 — every rank of a three-sample set
    /// is within one of every other — so the tail numbers this section exists
    /// to report need a fixture that can distinguish them.
    #[test]
    fn perf_baseline_lsp_percentiles_are_nearest_rank() {
        let sorted: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();
        assert_eq!(percentile(&sorted, 0.50), 50.0);
        assert_eq!(percentile(&sorted, 0.95), 95.0);
        assert_eq!(percentile(&sorted, 0.99), 99.0);
        // A single sample answers every question with itself.
        assert_eq!(percentile(&sorted[..1], 0.99), 1.0);
    }
}
