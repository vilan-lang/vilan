//! Per-document analysis state and the navigation queries the language-server
//! handlers run against it: position→entity lookup, hover, go-to-definition,
//! find-references, and rename.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tower_lsp::lsp_types::{Position, Range};
use vilan_core::analyzer::{DERIVED_SOURCE, Expr, ExprIfBranch, Parameter, SourceId};
use vilan_core::cancel::CancelToken;
use vilan_core::formatter::{STYLE_BREAKPOINT_WIDTHS, STYLE_CONDITION_METHODS};
use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::leak_tally::{LeakSite, Leaked};
use vilan_core::lexing::{AT_IS_NOT_A_TOKEN, HASH_IS_NOT_A_TOKEN, tokenize};
use vilan_core::node::{Convention, CssDeclaration, CssItem, CssValuePiece, Node};
use vilan_core::parsing::IMPORTANT_HAS_NO_PLACE;
use vilan_core::{
    Error, LeakedEntryAst, Manifest, OwnedModules, Platform as BuildPlatform, Program, Span,
    Workspace as BuildWorkspace, analyze_source_owning_overlay_modules,
};

use crate::keystroke::{
    Anchor, CursorContext, LandedSnapshot, ModuleSymbols, SymbolEntry, SymbolIndex, Verdict,
    candidates, cursor_context, module_name_of, shape_stamp, sort_and_deoverlap, syntax_tokens_in,
};
use crate::line_index::LineIndex;
use crate::references::{Definition, DefinitionKind, ReferenceIndex};
use vilan_ide::{
    Analysis, BOOK_BASE, Completion, CompletionKind, ImportRoots, KEYWORD_DOCS, keyword_lexeme,
    source_call_subject, span_of,
};

/// A file's project context, resolved from the nearest `vilan.toml`: the build
/// platform to analyze it against, and the package source root (where `import
/// pkg::..` siblings resolve). Either is `None` when there's no project (or the
/// file's role can't be determined) — analysis then infers the platform from the
/// file's imports and roots `pkg::` at the file's own directory.
struct ProjectContext {
    platform: Option<BuildPlatform>,
    /// The FURTHER colors the build compiles this file under — a module shared
    /// between the legs of a multi-entry package (E113). Each is analyzed too,
    /// for its diagnostics only, so the editor reports what the build would
    /// rather than one leg's half of it.
    shared_platforms: Vec<BuildPlatform>,
    pkg_root: Option<PathBuf>,
    /// The file's resolved dependency workspace (P2), so cross-package imports
    /// (`import <dep>::..`) type-check in the editor. Its `platform_reason` is
    /// already stamped for `platform`, the leg the editor's own analysis runs
    /// under (E119); a shared leg re-stamps it from `platform_reasons` below.
    workspace: BuildWorkspace,
    /// E119: per color, WHY this file is analyzed under it — the same answer
    /// `vilan check <file>` prints, from the same function. Empty when there is
    /// no project to answer from.
    platform_reasons: Vec<(BuildPlatform, String)>,
    /// Why the project didn't resolve, when it didn't (F5 S5). Everything below
    /// still degrades exactly as it did — the difference is that the reason is
    /// now published instead of swallowed.
    manifest_problem: Option<ManifestProblem>,
    /// The directory of the `vilan.toml` this context resolved from — the
    /// package's identity for E124's clock, which is keyed per PACKAGE and not
    /// per file or per source root. `None` when no manifest was found.
    manifest_dir: Option<PathBuf>,
    /// E124's module-level slice: the package's entry names when NO entry loads
    /// this file. `None` — the overwhelmingly common answer — when an entry
    /// does load it, when the file is not the package's to judge, or when the
    /// manifest is a `[library]` (no entries, no union, no gray).
    unloaded_by_entries: Option<Vec<String>>,
    /// Whether this file lives under the package's declared `generated` root.
    /// It gets no top-level gray at either granularity: the file is machine-
    /// written, `vilan fmt` already leaves it alone, and fading kolt's
    /// 18,198-line lucide module wall to wall would be the paint's single
    /// largest lie (`dead-code-paint.md` §1.5).
    generated: bool,
}

impl ProjectContext {
    fn none() -> ProjectContext {
        ProjectContext {
            platform: None,
            shared_platforms: Vec::new(),
            pkg_root: None,
            workspace: BuildWorkspace::default(),
            platform_reasons: Vec::new(),
            manifest_problem: None,
            manifest_dir: None,
            unloaded_by_entries: None,
            generated: false,
        }
    }

    /// This context's workspace as a leg OTHER than the primary sees it: the
    /// same dependency graph, re-stamped with that leg's own E119 reason. The
    /// primary leg reads `workspace` directly and clones nothing.
    fn workspace_for(&self, platform: BuildPlatform) -> BuildWorkspace {
        let mut workspace = self.workspace.clone();
        workspace.platform_reason = self
            .platform_reasons
            .iter()
            .find(|(colored, _)| *colored == platform)
            .map(|(_, reason)| reason.clone());
        workspace
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
    // deps). The platform is `platform_color::file_platforms`' answer, the same
    // one `vilan check <file>` takes: the classic single-entry form analyzes
    // every file under the root against the package target, and a multi-entry
    // package (proposal/platform-coloring.md §4.2) analyzes a file under the
    // platform of the entry that REACHES it.
    //
    // §4.2 originally left a non-entry file to inference "because a module has
    // no `main` and thus no admission walk". That reasoning missed what a
    // platform also decides: which `std` layer serves a twin module. Under the
    // process overlay a browser module's `View` is `{ tag, attributes, children,
    // text }` and `self.element` is a field that does not exist — so every
    // browser-only module of a fullstack app cried wolf in the editor while
    // `vilan build` was clean (E113, the owner's kolt report).
    if let Some(package) = &manifest.package {
        let pkg_root = root.join(package.root());
        // Each color with the REASON it was chosen (E119) — the same function
        // `vilan check <file>` calls, so the two surfaces cannot come to two
        // conclusions about why a file is colored either.
        let choices =
            vilan_core::platform_color::file_platform_choices(&pkg_root, &manifest, entry_path);
        let platform_reasons: Vec<(BuildPlatform, String)> = choices
            .iter()
            .map(|choice| (choice.platform, choice.reason.clause()))
            .collect();
        // E124's module-level slice, taken off the SAME per-entry walk: a
        // choice with reason `ReachedBy` means an entry loads this file, so for
        // a multi-entry package the answer is already in hand and the slice is
        // free. See `dead_items::unreached_module_entries`.
        let unloaded_by_entries =
            vilan_core::dead_items::unreached_module_entries(root, &manifest, entry_path, &choices);
        let generated = vilan_core::dead_items::is_generated(root, &manifest, entry_path);
        let mut platforms = choices.into_iter().map(|choice| choice.platform);
        let platform = platforms.next();
        let shared_platforms: Vec<BuildPlatform> = platforms.collect();
        let (mut workspace, manifest_problem) = resolve_dependencies(root, &manifest_path);
        workspace.platform_reason = platform.and_then(|platform| {
            platform_reasons
                .iter()
                .find(|(colored, _)| *colored == platform)
                .map(|(_, reason)| reason.clone())
        });
        return ProjectContext {
            platform,
            shared_platforms,
            pkg_root: Some(pkg_root),
            workspace,
            platform_reasons,
            manifest_problem,
            manifest_dir: Some(root.to_path_buf()),
            unloaded_by_entries,
            generated,
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
            shared_platforms: Vec::new(),
            pkg_root: Some(pkg_root),
            workspace,
            // A `[library]` declares no target and the editor invents none (see
            // above), so there is no colour to explain.
            platform_reasons: Vec::new(),
            manifest_problem,
            // A `[library]` has no entries — validation refuses them outright —
            // so it has no union and gets NO top-level gray, workspace member
            // or not (`dead-code-paint.md` §4, determination 9). Every top-level
            // item is surface a consumer may import, and that property is what
            // saves a consumer from forking an under-exported package. Locals,
            // unused imports and unreachable code stay painted; they need no
            // entry. The `manifest_dir` stays `None` for the same reason: there
            // is no package clock to key.
            manifest_dir: None,
            unloaded_by_entries: None,
            generated: vilan_core::dead_items::is_generated(root, &manifest, entry_path),
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
///
/// [`canonical_path_of_unwritten`](vilan_core::util::canonical_path_of_unwritten)
/// rather than `canonical_path`, for the reason spelled on [`is_within`]: the
/// editor's subject is an OPEN BUFFER, which need not be on disk, and
/// `canonical_path` answers a resolved spelling for one side and a lexical one
/// for the other the moment only one of them exists. Identical for a path that
/// IS on disk, so the on-disk case is byte-for-byte the comparison it was.
fn same_file(a: &Path, b: &Path) -> bool {
    vilan_core::util::canonical_path_of_unwritten(a)
        == vilan_core::util::canonical_path_of_unwritten(b)
}

/// The smallest span covering both — E114's unreachable third builds one range
/// per dead block out of its statements' spans.
fn union(a: Span, b: Span) -> Span {
    Span::new((), a.start.min(b.start)..a.end.max(b.end))
}

/// Every STATEMENT LIST the ENTRY file wrote, paired with its trailing
/// expression: the regions a diverging statement can leave a dead tail in
/// (E114's unreachable third).
///
/// A block-shaped region is not one syntactic thing in the resolved program —
/// `Expr::Block` is one shape, but an `if` arm, a loop body and a function body
/// each store their own `(Vec<Id>, Id)` inline rather than wrapping a block —
/// so all four are collected here, once, instead of at the two call sites that
/// would otherwise each have to remember the list. `match` legs need no arm of
/// their own: a leg's body is an expression id, and a braced one IS an
/// `Expr::Block`. Closures likewise — a closure's `return_` is one expression.
///
/// **The ENTRY's regions is not a filter applied afterwards, it is the walk's
/// scope**, and that is a cost decision as much as a correctness one: only the
/// open file is painted, so `Program::entities_of` FETCHES the file's rows by id
/// range instead of scanning a whole-program map — and the divergence walk that
/// follows no longer visits every `std` body on every publish. The function
/// table is filtered rather than fetched, because it is small next to the
/// expression map and a declaration's id is the key.
///
/// The regions are unordered and may repeat a list reachable two ways; the
/// caller sorts and dedups the spans it derives, which is cheaper than deduping
/// the regions and is the only ordering anything downstream needs.
fn block_regions<'a>(
    program: &'a Program<'a>,
    entry_ids: &[std::ops::Range<u32>],
) -> Vec<(&'a [Id], Id)> {
    fn if_arms<'a>(branch: &'a ExprIfBranch, regions: &mut Vec<(&'a [Id], Id)>) {
        match branch {
            ExprIfBranch::If(_, (statements, tail), next) => {
                regions.push((statements, *tail));
                if let Some(next) = next {
                    if_arms(next, regions);
                }
            }
            ExprIfBranch::Else((statements, tail)) => regions.push((statements, *tail)),
        }
    }

    let mut regions: Vec<(&[Id], Id)> = Vec::new();
    for (_, expression) in program.entities_of(SourceId(0)) {
        match expression {
            Expr::Block((statements, tail))
            | Expr::For(_, (statements, tail))
            | Expr::ForEach(_, _, (statements, tail)) => regions.push((statements, *tail)),
            Expr::If(branch) => if_arms(branch, &mut regions),
            _ => {}
        }
    }
    for (id, function) in &program.functions {
        if entry_ids.iter().any(|range| range.contains(&id.0)) {
            regions.push((&function.body.0, function.body.1));
        }
    }
    regions
}

/// Whether `file` lives within `directory`, through the same helper.
///
/// **Both sides resolve the same way, and the helper is
/// [`canonical_path_of_unwritten`](vilan_core::util::canonical_path_of_unwritten)**
/// (B207, B198's shape in the LSP). `canonical_path` never fails: where the
/// resolution fails it degrades to the LEXICAL spelling, which is the right
/// answer for a comparison key and the wrong one for one side of a containment
/// test whose other side resolved. The editor's `file` is an open BUFFER and
/// need not be on disk — an untitled document, a file created in the editor and
/// not yet saved — while `directory` is a layer root that always is, so a
/// project root reached through a symlink (or, on a case-insensitive
/// filesystem, spelled in another case) made the two sides a resolved path and
/// a spelled one and this answered NO for a buffer plainly inside its own
/// package. The document then lost its project context: no package root, no
/// platform, `pkg::` imports unresolved.
///
/// `canonical_path_of_unwritten` resolves the deepest ancestor that IS on disk
/// and re-attaches the tail as spelled, so both sides are resolved down to the
/// part no filesystem has an opinion about, and a tree where nothing exists
/// degrades to G17's spelled ladder on BOTH sides rather than to a mixed
/// comparison. For a path that is on disk it costs exactly what `canonical_path`
/// costs and answers exactly what it answered.
fn is_within(directory: &Path, file: &Path) -> bool {
    vilan_core::util::canonical_path_of_unwritten(file)
        .starts_with(vilan_core::util::canonical_path_of_unwritten(directory))
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
    /// One of the definition's references is a struct-init field SHORTHAND
    /// (`A { x }`), whose single identifier names two things at once: the
    /// field `A::x` and the local `x` it reads (E134). There is no span to
    /// rewrite that serves both — renaming from either side would silently
    /// break the other, which is the "half-applied rename" this enum exists to
    /// forbid, and today renaming the field does exactly that. A correct
    /// rename has to EXPAND the site to `A { x = value }` first, which is a
    /// text rewrite rather than a span rewrite: every other edit this module
    /// emits replaces an identifier with `new_name`, so an expansion cannot be
    /// expressed in the edit set at all. The refusal names the expansion and
    /// the user does it once, after which both names have spans of their own
    /// and the rename goes through.
    SharedSpan {
        what: String,
        with: String,
        name: String,
    },
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
            RenameRefusal::SharedSpan { what, with, name } => format!(
                "cannot rename {what}: it is written as a field shorthand, where the one `{name}` names both it and {with}. \
                 Expand that site to `{name} = {name}` first, so each name has a span of its own"
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
    /// **M27**: what this analysis's EDITOR TABLES cost — the `lsp-index`
    /// phase (`entity_spans` + the [`ReferenceIndex`]) plus the `lsp-landed`
    /// one ([`capture_landed`](Document::capture_landed)'s single walk), which
    /// together are every table the server builds over a finished analysis.
    ///
    /// Carried on the document because the cost is paid on the analysis thread
    /// and read by the SERVER, one join later: `analyze_and_publish` records it
    /// on the session trace beside the analysis counts. Measured on a real
    /// browser application (E126, 2026-09-04, release): `lsp-index` alone runs
    /// 110–292 ms of wall per keystroke at loadavg 31–33 against `lsp-analyze`
    /// at 1,146–1,436 ms, and under load the two halves stay in proportion
    /// (494–1,058 ms of index against 232–350 ms of landed walk at loadavg
    /// 150–166). A fifth per-keystroke cost, outside every tranche, and half
    /// of it sat outside the instrument too until this phase line grew the
    /// `lsp-landed` label. `ZERO` on a document that never analyzed.
    pub index_time: std::time::Duration,
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
    /// What the OTHER legs of a multi-entry package say about this file (E113):
    /// a module shared between a browser entry and a node one is compiled once
    /// per leg and must type-check under each, so the editor reports the union
    /// the build would. Already published — the programs that produced them
    /// were dropped (and reclaimed) at the end of the analysis, so there is
    /// nothing left to resolve a `SourceId` against. Empty for every file with
    /// one color, which is nearly all of them.
    shared_diagnostics: Vec<PublishedDiagnostic>,
    /// What an `import`/`use` path in this file can reach (E57) — the analysis's
    /// own `std` spec, package root, and dependency packages, kept so completion
    /// can enumerate modules the `Program` never loaded. `None` on the degraded
    /// internal-error document, which resolved nothing.
    import_roots: Option<ImportRoots>,
    /// The server's world revision this analysis READ (E117), stamped by the
    /// caller through [`Document::stamp_analysis`] — every buffer change bumps
    /// it, so a larger number is a strictly later view of every open file.
    ///
    /// `text_hash` answers "is this analysis of my own current text", which is
    /// enough for the file being typed in and vacuous for every other one: a
    /// DEPENDENT's buffer does not move when the module it imports does, so two
    /// of its analyses — one that read the module mid-edit, one that read it
    /// restored — are text-identical and both would land, in either order. This
    /// is what separates them. Zero on a document nobody stamped (every test
    /// fixture, and the degraded internal-error document), which keeps the
    /// comparison a no-op there.
    analysis_revision: u64,
    /// The `pkg::` source root this analysis resolved under, canonicalized —
    /// `None` when the file belongs to no project at all. E116's identity: two
    /// open documents sharing a root are colored by ONE import graph, so an
    /// edit that changes which entry reaches a file has to invalidate every one
    /// of them, not only the ones that import the edited file.
    package_root: Option<PathBuf>,
    /// E124: the directory of the `vilan.toml` this analysis resolved from —
    /// the package the dead-item paint's clock is keyed by. `None` for a file
    /// with no project, and for a `[library]`, which gets no top-level gray.
    manifest_dir: Option<PathBuf>,
    /// E124's module-level slice: the package's entry names when NO entry
    /// loads this file. A module nothing builds is dead whole, and the answer
    /// is a by-product of the per-entry walk `resolve_project_context` already
    /// runs (`dead_items::unreached_module_entries`).
    unloaded_by_entries: Option<Vec<String>>,
    /// Whether this file is under the declared `generated` root — no top-level
    /// gray at either granularity.
    generated: bool,
    /// E124's union, as of the last time the package clock landed one — the
    /// LIVE side, owned by the server and handed to this document at publish
    /// time, never by an analysis.
    ///
    /// `None` is the withdrawal, and it is the whole staleness rule: a top-level
    /// gray may be arbitrarily stale in the direction of FEWER grays and must
    /// never be served stale in the direction of more, because a gray is a
    /// claim the user acts on by deleting and the fact that falsifies it lives
    /// in another file (`dead-code-paint.md` §3.2, determination 8). Downgraded
    /// on edit, upgraded on land.
    package_reach: Option<Arc<crate::dead_items::PackageReach>>,
    /// E121's keystroke path: what this analysis's answers were, captured once
    /// when it was built, so a request between two landings costs an anchor and
    /// a lex instead of a walk of the whole analyzed program. Part of the
    /// ANALYSIS side — `adopt_analysis` takes it wholesale with the program it
    /// describes. See [`crate::keystroke`].
    landed: LandedSnapshot,
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
/// outside this value borrows `text` or `ast` — no process-global cache, no
/// thread-local, nothing the server retains. `leak-soak.md` §7.2 is the audit
/// that establishes that second half for the entry pair, global by global.
/// The first half is what `analyze_source_owning_overlay_modules` returns.
/// Every `Document` query returns owned values, so nothing borrowed from the
/// program outlives the borrow of `self` that produced it.
///
/// The owned modules are the one place the invariant is a COUNT rather than
/// an exclusivity (M23): a stored base world may borrow the same copies, and
/// says so by holding its own claim. `owned_modules` is this document's
/// claims, `Drop` gives back exactly those, and an allocation another holder
/// still claims survives — which is precisely why the reclaim below is sound
/// without knowing anything about the base cache.
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
    /// `*text`, `*ast`, and the allocations `owned_modules` holds claims on —
    /// it is the program `analyze_source_owning_overlay_modules` built over
    /// exactly that text and returned with exactly these handles. Nothing
    /// else may hold a reference derived from `*text` or `*ast`: when this
    /// value drops, both are freed. An owned module allocation is freed only
    /// if this document's claim was the LAST (M23), so another holder's
    /// reference into one is fine — and is what the claim protocol exists
    /// for.
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
        // SAFETY: as above — the program was the only thing borrowing
        // through THIS document's claims (the `new` contract). Giving them
        // back frees an allocation only if no stored base world still claims
        // it (M23); one that does keeps it, correctly, alive.
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
#[derive(Clone)]
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

impl PublishedDiagnostic {
    /// Whether two published diagnostics are the SAME squiggle: same file, same
    /// span, same words, same severity. The union of two legs' verdicts on a
    /// shared module (E113) is deduplicated by it — most of what a browser
    /// compile and a node compile say about one module is the same sentence
    /// about the same characters, and the reader wants one of it.
    fn same_place_and_words(&self, other: &PublishedDiagnostic) -> bool {
        self.path == other.path
            && self.span == other.span
            && self.warning == other.warning
            && self.message == other.message
    }
}

/// One requirement-trace entry as the publisher wants it (backlog E78):
/// located like the C3 note, plus whether it marks an uncovered CALL — a
/// call hop additionally publishes as its own diagnostic at that location
/// (E81), while the elision tail only ever rides as related information
/// (its span is the last kept hop's, already underlined by that hop's own
/// diagnostic).
#[derive(Clone)]
pub struct PublishedHop {
    pub span: Span,
    pub message: String,
    pub path: Option<PathBuf>,
    pub call: bool,
}

/// A line number moved by `shift`, clamped into `u32` — the line arithmetic
/// [`Document::keystroke_tokens_in_lines`] does against
/// [`Document::tail_line_shift`]. Saturating on both ends: a window shifted
/// off the top of the file names line 0, which is a wider ANALYZED range than
/// needed and therefore still a superset of what can map into the viewport.
fn shift_line(line: u32, shift: i64) -> u32 {
    (line as i64 + shift).clamp(0, u32::MAX as i64) as u32
}

/// The union of two position ranges in one stream, as ranges — merged when
/// they meet, both when they do not, and empty ones dropped.
///
/// The two are the head anchor's window and the tail anchor's window into the
/// same capture; for an unedited buffer they are the same range, and for an
/// edit above the viewport they are two. Merging keeps a token from being
/// mapped (and offered) twice.
fn merge_positions(
    left: std::ops::Range<usize>,
    right: std::ops::Range<usize>,
) -> impl Iterator<Item = usize> {
    let mut ranges: Vec<std::ops::Range<usize>> = [left, right]
        .into_iter()
        .filter(|range| range.start < range.end)
        .collect();
    ranges.sort_by_key(|range| range.start);
    if let [first, second] = ranges.as_mut_slice()
        && first.end >= second.start
    {
        let merged = first.start..first.end.max(second.end);
        ranges = vec![merged];
    }
    ranges.into_iter().flatten()
}

/// B38, the salvage signature: when `stream` is entirely silent within the
/// retained suffix — the shape of a parse break truncating the file to a
/// prefix — the previous analysis's tokens for the byte-identical tail fill
/// in, already shifted into this analysis's coordinates. A stream that reaches
/// the suffix suppresses this wholesale, which is what keeps re-classification
/// of identical text (semantics flow downward) fresh rather than stale.
///
/// Two callers, and they are the same rule applied to the same tokens at the
/// two moments they exist: [`Document::semantic_tokens`], the walk, and
/// [`Document::adopt_analysis`], which folds the tail into the analysis's
/// CAPTURE the moment it computes one. Appending rather than merging is sound
/// because the guard has just established that every kept token ends at or
/// before `tail_start` and every retained one starts at or after it, so the
/// result is still sorted and still non-overlapping.
fn fold_retained_tail(
    stream: &mut Vec<(Span, TokenKind, u32)>,
    tail: &[(Span, TokenKind, u32)],
    tail_start: usize,
) {
    if tail.is_empty() || !stream.iter().all(|(span, ..)| span.end <= tail_start) {
        return;
    }
    stream.extend(
        tail.iter()
            .filter(|(span, ..)| span.start >= tail_start)
            .cloned(),
    );
}

/// The markup spans of a raw parse (element-syntax S5): tag names (open and
/// close), the angle brackets around them, attribute and event names, and the
/// desugar-scaffolding spans whose analyzed tokens the markup replaces.
#[derive(Default)]
struct MarkupSpans {
    scaffolding: Vec<Span>,
    tags: Vec<Span>,
    /// The elements' angle brackets — `<`, `>`, `</`, `/>` (E115).
    punctuation: Vec<Span>,
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
        out.punctuation.extend(body.punctuation.iter().copied());
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

/// The `css`-block spans of a raw parse (css-block S5): property names,
/// condition-head names, and the one desugar-scaffolding span that is NOT
/// zero-width.
///
/// S2 cut every one of these deliberately, for this slice: the AST carries the
/// property-name span, and each generated accessor takes a zero-width anchor so
/// that no analyzed token ever lands on CSS-side syntax. The single exception is
/// the outer `style()`, which keeps the `css` keyword's own span so a missing
/// `import std::style::style` underlines the word that asked for a `Style` — and
/// that one accessor is what `scaffolding` suppresses here, exactly as the
/// element desugar's `<tag` accessor is suppressed.
#[derive(Default)]
struct CssSpans {
    scaffolding: Vec<Span>,
    properties: Vec<Span>,
    conditions: Vec<Span>,
}

/// The `css` keyword's own length. A `Node::Css` span starts exactly at the
/// keyword (`css.rs`'s `KEYWORD`), so this is the outer `style()` accessor's
/// span — the one token to suppress.
const CSS_KEYWORD_LENGTH: usize = "css".len();

fn collect_css_spans(node: &vilan_core::Spanned<vilan_core::node::Node<'_>>, out: &mut CssSpans) {
    if let Node::Css(body) = &node.0 {
        out.scaffolding
            .push((node.1.start..node.1.start + CSS_KEYWORD_LENGTH).into());
        collect_css_body_spans(body, out);
    }
    // Holes and condition-head arguments are ordinary expression positions, so
    // a block written inside one is reached the same way any other child is.
    node.0
        .for_each_child(&mut |child| collect_css_spans(child, out));
}

fn collect_css_body_spans(body: &vilan_core::node::CssBody<'_>, out: &mut CssSpans) {
    for item in &body.items {
        match item {
            CssItem::Declaration(declaration) => out.properties.push(declaration.property),
            CssItem::Nested(nested) => {
                out.conditions.push(nested.name.1);
                collect_css_body_spans(&nested.body, out);
            }
        }
    }
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
    if let Node::Element(body) = &node.0
        && let Some(close) = body.close_tag
    {
        let open = body.tag;
        let touches = |span: Span| span.start <= offset && offset <= span.end;
        if touches(open) || touches(close) {
            *out = Some((open, close));
        }
    }
    node.0
        .for_each_child(&mut |child| find_linked_tags(child, offset, out));
}

impl Document {
    /// [`analyze_cancellable`](Document::analyze_cancellable) under a token
    /// nobody holds the other end of: no checkpoint can fire, so the analysis
    /// always produces a document.
    ///
    /// Test-only since M26. The shipped server analyzes through the scheduler,
    /// which always has a token to hand, so there is no production caller left
    /// — and saying so with `cfg(test)` is what keeps `-D warnings` able to
    /// notice if this door is ever the one a new path takes by accident.
    #[cfg(test)]
    pub fn analyze(text: &str, std_dir: &Path, entry_path: &Path) -> Self {
        Self::analyze_cancellable(text, std_dir, entry_path, &CancelToken::new())
            .expect("an analysis under an uncancelled token always produces a document")
    }

    /// [`analyze`](Document::analyze), cancellable (M26,
    /// `proposal/editor-latency.md` §4.2).
    ///
    /// `cancel` is installed on the analysis thread, where the analyzer's phase
    /// boundaries and its long per-function loops read it
    /// ([`vilan_core::cancel`]). Answering `None` means the token was set while
    /// the analysis ran: what it had computed is a TRUNCATED view of the
    /// program, so it is destroyed here, on the analysis thread, and never
    /// reaches the caller — there is no truncated `Document` for anyone to
    /// land, publish or answer a request from.
    ///
    /// Cancellation is an optimisation over E117's revision stamps, not a
    /// replacement for them: a superseded analysis that finishes before its
    /// token is read still returns `Some`, and `land` still drops it. Nothing
    /// here is load-bearing for correctness — remove every checkpoint and the
    /// editor shows the same thing, more slowly.
    pub fn analyze_cancellable(
        text: &str,
        std_dir: &Path,
        entry_path: &Path,
        cancel: &CancelToken,
    ) -> Option<Self> {
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
        let cancel = cancel.clone();
        std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(move || {
                // Installed for the life of the analysis and torn down before
                // the thread ends, so the token is exactly the analysis's.
                let _scope = cancel.install();
                let document = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    Self::analyze_on_this_thread(&text, &std_dir, &entry_path)
                }))
                .unwrap_or_else(|_| Self::internal_error(&text));
                // Read AFTER the analysis, on the thread that ran it: a
                // cancelled analysis's document is dropped here — which is what
                // gives its entry text, tree and owned modules back
                // (`AnalyzedProgram`'s `Drop`, `leak-soak.md` §7) — rather than
                // travelling back to a caller who would only drop it anyway.
                (!cancel.is_cancelled()).then_some(document)
            })
            .expect("spawn analysis thread")
            .join()
            // Unreachable while the thread body catches unwinds (an abort
            // never returns here); kept graceful all the same.
            .unwrap_or_else(|_| Some(Self::internal_error(&outer_text)))
    }

    /// A document holding `text` and NOTHING an analysis produces: no program,
    /// no diagnostics, the live text faithfully recorded so position mapping
    /// and the next re-analysis behave.
    ///
    /// Two callers, and they are the two ways a document can exist without an
    /// analysis behind it: the degraded document a panicked analysis lands on
    /// ([`internal_error`](Document::internal_error), which adds its one honest
    /// diagnostic), and the entry `did_open` puts in the map before it schedules
    /// the first analysis (E123). Every query handler already reads `program`
    /// through an `Option` and answers emptily when there is none — the same
    /// state the debounce window has always had between an edit and its
    /// analysis.
    ///
    /// `text_hash` is the hash of `text`, which says "the analyzed text is
    /// this" while nothing has been analyzed. That is deliberate and safe
    /// because a document only ever reaches the map WITH an analysis of that
    /// exact text scheduled: the skip it can produce (`pause_action`'s
    /// `Unchanged`, when an edit is undone inside one debounce window) skips a
    /// re-analysis that the open's own analysis is already doing.
    pub fn unanalyzed(text: &str) -> Self {
        let line_index = Arc::new(LineIndex::new(text));
        Document {
            // A fresh analysis IS the analyzed text: the map is identity.
            live_edits: Some(Vec::new()),
            analyzed_index: Arc::clone(&line_index),
            line_index,
            program: AnalyzedProgram::none(),
            index_time: std::time::Duration::ZERO,
            diagnostics: Vec::new(),
            diagnostic_sources: Vec::new(),
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
            shared_diagnostics: Vec::new(),
            import_roots: None,
            analysis_revision: 0,
            package_root: None,
            manifest_dir: None,
            unloaded_by_entries: None,
            generated: false,
            package_reach: None,
            // Nothing landed, so the keystroke path answers from syntax alone.
            landed: LandedSnapshot::default(),
        }
    }

    /// The degraded document a panicked analysis lands on: [`unanalyzed`], plus
    /// the one honest diagnostic saying so.
    ///
    /// [`unanalyzed`]: Document::unanalyzed
    fn internal_error(text: &str) -> Self {
        let mut document = Self::unanalyzed(text);
        document.diagnostics = vec![Error {
            trace: Vec::new(),
            note: None,
            span: vilan_core::span::Span::new((), 0..0),
            msg: "internal error: the compiler panicked analyzing this file (this is a bug; the details are on stderr)"
                .to_string(),
        }];
        document.diagnostic_sources = vec![SourceId(0)];
        document
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
        //
        // Timed for the same reason the core pipeline's phases are (E106): this
        // is not a lookup. `platform_color::file_platforms` walks the loader's
        // `pkg::` graph from EVERY entry of the manifest until one reaches this
        // file (E113), and the walk resolves and parses each module it reaches
        // — per analysis, so per keystroke — while `resolve_dependencies`
        // re-reads the manifest closure beside it. The core line cannot see any
        // of it: it starts inside `analyze`.
        let phase_context_start = std::time::Instant::now();
        let mut context = resolve_project_context(entry_path);
        let phase_context = phase_context_start.elapsed();
        let manifest_problem = context.manifest_problem.take();
        let manifest_dir = context.manifest_dir.take();
        let unloaded_by_entries = context.unloaded_by_entries.take();
        let generated = context.generated;
        // E116: the DECLARED root, canonicalized, kept as the file's package
        // identity. Deliberately not the fallback below — a file with no
        // project has no package to be colored by, and rooting it at its own
        // directory would make unrelated neighbours look like siblings.
        let package_root = context
            .pkg_root
            .as_deref()
            .map(vilan_core::util::canonical_path);
        let pkg_root = context
            .pkg_root
            .clone()
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
        let phase_analyze_start = std::time::Instant::now();
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
        let phase_analyze = phase_analyze_start.elapsed();
        // M26: the analysis was superseded while it ran. Everything below is
        // work for a result that cannot land — the editor tables the queries
        // index, and the shared-platform legs, which are a FULL analysis each
        // (E113) — so stop, and give back what this analysis leaked on the way
        // in. Wrapping the (possibly `None`) program in an `AnalyzedProgram`
        // and dropping it is what performs the reclaim: the wrap owns the entry
        // text, the entry tree and the overlay-served modules, and its `Drop`
        // is the only thing that hands them back (`leak-soak.md` §7). The
        // degraded document returned here is never seen by a caller —
        // `analyze_cancellable` reads the token and answers `None` — so it
        // carries nothing but the text.
        if vilan_core::cancel::cancelled() {
            // SAFETY: the same contract the wrap below is built under — this is
            // the program `analyze_source_owning_overlay_modules` built over
            // `leaked` with exactly these handles, and nothing has been derived
            // from any of them on this path.
            drop(unsafe { AnalyzedProgram::new(program, Some(leaked_text), ast, owned_modules) });
            return Self::unanalyzed(text);
        }
        let phase_index_start = std::time::Instant::now();

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
        let phase_index = phase_index_start.elapsed();
        // The other legs' verdicts on a shared module (E113), computed AFTER
        // the primary so a panic in one of them cannot cost the analysis the
        // user is looking at. Each is a full analysis under that leg's platform
        // whose program is published and then dropped — the diagnostics are all
        // the editor keeps, and hover/goto/completion stay the primary leg's.
        let phase_legs_start = std::time::Instant::now();
        let shared_diagnostics = context
            .shared_platforms
            .iter()
            .flat_map(|platform| {
                Self::diagnostics_under(
                    text,
                    &std,
                    &pkg_root,
                    entry_path,
                    *platform,
                    // Each leg gets its own E119 reason: a shared module's miss
                    // under the browser leg is explained by the browser leg.
                    &context.workspace_for(*platform),
                )
            })
            .collect();
        let phase_legs = phase_legs_start.elapsed();
        let mut document = Document {
            // A fresh analysis IS the analyzed text: the map is identity.
            live_edits: Some(Vec::new()),
            analyzed_index: Arc::clone(&line_index),
            line_index,
            program,
            // Filled below, once the landed walk it also counts has run.
            index_time: std::time::Duration::ZERO,
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
            shared_diagnostics,
            import_roots: Some(import_roots),
            analysis_revision: 0,
            package_root,
            manifest_dir,
            unloaded_by_entries,
            generated,
            // The union is the server's to hand over; a fresh analysis carries
            // none, which is the withdrawn state and the safe one.
            package_reach: None,
            landed: LandedSnapshot::default(),
        };
        // E121: the keystroke path's whole-program walk, paid HERE — once per
        // analysis, on the analysis thread — instead of once per request on
        // the keystroke thread. See [`LandedSnapshot`].
        let phase_landed_start = std::time::Instant::now();
        document.landed = document.capture_landed(entry_path);
        let phase_landed = phase_landed_start.elapsed();
        // M27: the editor tables, as ONE number the server can carry — the
        // reference/entity index and the landed walk are the same family of
        // cost (a table built over a finished analysis, thrown away by the
        // next keystroke) and no budget separates them.
        document.index_time = phase_index + phase_landed;
        // The server's half of the `VILAN_PHASE_TIMING` split (E106): one line
        // per LSP analysis, naming the costs the core pipeline's own line
        // cannot see — project resolution (the E113 reachability walk and the
        // dependency closure), the analysis proper, the editor tables built
        // over it, the keystroke path's landed walk, and the extra full
        // analysis each FURTHER leg of a shared module costs. Stderr, like the
        // core line, and behind the same switch, so one variable turns the
        // whole picture on. `legs` is the count, not a duration: a file two
        // legs reach pays TWO analyses per keystroke, and that is the fact to
        // read first.
        //
        // **M27 moved this print.** It used to run before `capture_landed`,
        // which put E121's whole-program walk — the fifth per-keystroke cost —
        // outside the only line that could see it. `lsp-landed` is that walk,
        // and it is on the line now for the same reason `lsp-index` is: a cost
        // nobody prints is a cost nobody budgets (N43's rule).
        if vilan_core::phase_timing_enabled() {
            let milliseconds = |duration: std::time::Duration| duration.as_secs_f64() * 1000.0;
            eprintln!(
                "[vilan phase] lsp-context {:.1}ms lsp-analyze {:.1}ms lsp-index {:.1}ms \
                 lsp-landed {:.1}ms lsp-legs {:.1}ms legs {}",
                milliseconds(phase_context),
                milliseconds(phase_analyze),
                milliseconds(phase_index),
                milliseconds(phase_landed),
                milliseconds(phase_legs),
                context.shared_platforms.len(),
            );
        }
        document
    }

    /// Capture what this freshly analyzed document's answers are, for the
    /// keystroke path to re-serve until the next analysis lands.
    fn capture_landed(&self, entry_path: &Path) -> LandedSnapshot {
        if !self.program.is_some() {
            return LandedSnapshot::default();
        }
        let mut landed = LandedSnapshot {
            stamp: shape_stamp(self.analyzed_text()),
            tokens: self.semantic_tokens(),
            token_lines: Vec::new(),
            hints: self.inlay_hints(),
            index: self.landed_symbol_index(entry_path),
            landed: true,
        };
        // E122: the viewport index over the tokens just captured, paid on the
        // same thread and in the same breath as the walk that produced them.
        landed.index_token_lines(&self.analyzed_index);
        landed
    }

    /// The per-module symbol index this analysis supports (§2.1.4): every
    /// declared name, grouped by the module that declares it.
    ///
    /// Grouping is by `Program::source_of`, whose `SourceRange` windows are
    /// disjoint by construction (an entity id only grows), and the module's
    /// identity is `canonical_sources[source]` — the same table
    /// [`Document::depends_on`] reads. Module index 0 is the entry, which is
    /// the convention [`SymbolIndex::ENTRY`] names; its entries are the ones
    /// the keystroke path re-reads from live syntax.
    ///
    /// Derive-generated entities (`DERIVED_SOURCE`) are skipped: their spans
    /// are offsets into a template, not into any file a user can complete in.
    fn landed_symbol_index(&self, entry_path: &Path) -> SymbolIndex {
        let Some(program) = self.program.as_ref() else {
            return SymbolIndex::default();
        };
        let mut by_module: Vec<ModuleSymbols> = vec![ModuleSymbols {
            path: Some(entry_path.to_path_buf()),
            module_name: module_name_of(entry_path),
            // Filled by `refresh_entry_from_syntax` below, so the entry's
            // export list is always the LIVE buffer's.
            stamp: None,
            entries: Vec::new(),
        }];
        let slot_of = |source: SourceId, by_module: &mut Vec<ModuleSymbols>| -> Option<usize> {
            if source == SourceId(0) {
                return Some(SymbolIndex::ENTRY);
            }
            if source == DERIVED_SOURCE {
                return None;
            }
            let path = program.canonical_sources.get(source.0 as usize)?;
            if let Some(existing) = by_module
                .iter()
                .position(|module| module.path.as_deref() == Some(path.as_path()))
            {
                return Some(existing);
            }
            by_module.push(ModuleSymbols {
                path: Some(path.clone()),
                module_name: module_name_of(path),
                stamp: None,
                entries: Vec::new(),
            });
            Some(by_module.len() - 1)
        };
        let epoch = self.analysis_revision.max(1);
        let push =
            |id: Id, name: String, kind: CompletionKind, by_module: &mut Vec<ModuleSymbols>| {
                let Some(source) = program.source_of(id) else {
                    return;
                };
                let Some(slot) = slot_of(source, by_module) else {
                    return;
                };
                let call_parameters = (kind == CompletionKind::Function).then(|| {
                    program
                        .functions
                        .get(&id)
                        .map(|function| {
                            function
                                .parameters
                                .iter()
                                .filter_map(|parameter| program.parameters.get(parameter))
                                .map(|parameter| parameter.name.to_string())
                                .collect()
                        })
                        .unwrap_or_default()
                });
                by_module[slot].entries.push(SymbolEntry {
                    name,
                    kind,
                    signature: vilan_ide::signature_label(program, id),
                    call_parameters,
                    analysis_epoch: epoch,
                });
            };
        for (id, function) in &program.functions {
            push(
                *id,
                function.name.to_string(),
                CompletionKind::Function,
                &mut by_module,
            );
        }
        for (id, struct_) in &program.structs {
            push(
                *id,
                struct_.name.to_string(),
                CompletionKind::Struct,
                &mut by_module,
            );
        }
        for (id, enum_) in &program.enums {
            push(
                *id,
                enum_.name.to_string(),
                CompletionKind::Enum,
                &mut by_module,
            );
        }
        for (id, trait_) in &program.traits {
            push(
                *id,
                trait_.name.to_string(),
                CompletionKind::Trait,
                &mut by_module,
            );
        }
        let mut index = SymbolIndex {
            by_module,
            // M25: the arms that reach names this file has NOT imported —
            // auto-import candidates and an origin's module listing — are
            // functions of the analyzed program and the package tree it
            // resolved, so they are derived here, on the analysis thread, and
            // never in a request.
            completion: Arc::new(vilan_ide::CompletionIndex::build(
                program,
                self.import_roots.as_ref(),
                self.analyzed_text(),
            )),
        };
        index.refresh_entry_from_syntax(self.analyzed_text());
        index
    }

    /// One further leg's verdict on this file: analyze it under `platform` and
    /// publish what that compile says, keeping nothing (E113).
    ///
    /// The program is built and dropped inside this function, so its entry
    /// text, tree and overlay-served module copies are reclaimed here —
    /// `AnalyzedProgram`'s `Drop` is the same reclaim a superseded analysis
    /// takes. Publishing before the drop is what makes that safe: a
    /// `PublishedDiagnostic` carries resolved `PathBuf`s, so nothing that
    /// survives needs a `SourceId` to mean anything.
    fn diagnostics_under(
        text: &str,
        std: &vilan_core::PackageSpec,
        pkg_root: &Path,
        entry_path: &Path,
        platform: BuildPlatform,
        workspace: &BuildWorkspace,
    ) -> Vec<PublishedDiagnostic> {
        let (leaked_text, leaked) = Leaked::leak(
            text.to_string().into_boxed_str(),
            LeakSite::LspEntryText,
            text.len(),
        );
        let vilan_core::AnalyzedEntry {
            program,
            diagnostics,
            ast,
            owned_modules,
        } = analyze_source_owning_overlay_modules(
            leaked,
            std,
            pkg_root,
            entry_path,
            Some(platform),
            workspace,
        );
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
        // SAFETY: exactly `analyze_on_this_thread`'s pairing — the program was
        // built by `analyze_source_owning_overlay_modules` over the text
        // `leaked_text` owns, `ast` is the handle to the tree it borrows, and
        // `owned_modules` the overlay-served copies it parsed for itself.
        let program =
            unsafe { AnalyzedProgram::new(program, Some(leaked_text), ast, owned_modules) };
        let published = publish(
            program.as_ref(),
            &diagnostics,
            &diagnostic_sources,
            &warnings,
            &warning_sources,
        );
        // Everything kept is owned; the pair (and its reclaim) ends here.
        drop(program);
        published
    }

    /// The document's diagnostics grouped for publishing: errors attributed to
    /// the file they occurred in (`None` = this document), plus this document's
    /// warnings. Diagnostics from generated (derive) code carry template spans
    /// that map to no file — they attach to the entry at offset 0, labeled.
    ///
    /// A file shared between the legs of a multi-entry package also carries the
    /// other legs' diagnostics (E113), deduplicated against this leg's: the two
    /// compiles agree about most of a shared module, and one mistake reported
    /// twice is one squiggle.
    pub fn published_diagnostics(&self) -> Vec<PublishedDiagnostic> {
        let mut published = publish(
            self.program.as_ref(),
            &self.diagnostics,
            &self.diagnostic_sources,
            &self.warnings,
            &self.warning_sources,
        );
        for shared in &self.shared_diagnostics {
            if !published
                .iter()
                .any(|item| item.same_place_and_words(shared))
            {
                published.push(shared.clone());
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
}

/// One analysis's diagnostics and warnings, grouped for publishing — the body
/// [`Document::published_diagnostics`] used to inline, taken as parameters so a
/// further leg's throwaway program can be published the same way (E113).
fn publish(
    program: Option<&Program>,
    diagnostics: &[Error],
    diagnostic_sources: &[SourceId],
    warnings: &[Error],
    warning_sources: &[SourceId],
) -> Vec<PublishedDiagnostic> {
    let mut published = Vec::new();
    // The C3 note as the publisher wants it: its span, its message, and the
    // file it lives in when it has one of its own (`None` = the
    // diagnostic's own file, whichever that is — backlog E17).
    let locate = |note: &vilan_core::error::Note| {
        let note_path = note
            .source
            .and_then(|source| program?.source_path(source))
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
    for (index, error) in diagnostics.iter().enumerate() {
        let source = diagnostic_sources
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
            let path = program
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
    for (index, warning) in warnings.iter().enumerate() {
        // A warning is attributed like an error: a module's warning
        // squiggles in the module, not at that offset in this document.
        let source = warning_sources.get(index).copied().unwrap_or(SourceId(0));
        let path = (source != SourceId(0)).then(|| {
            program
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
    published
}

impl Document {
    /// Advances the LIVE snapshot — the text and its line index — without
    /// re-analyzing. Applied on every edit so live-text queries (notably
    /// completion's context scan) see the just-typed character immediately,
    /// while the heavier re-analysis stays debounced. The analyzed snapshot
    /// (`program`, `analyzed_index`, `text_hash`, and with them the analysis's
    /// captured answers) is deliberately untouched: program answers stay
    /// exactly right for the text they were computed from, and the pending
    /// re-analysis still fires.
    pub fn set_text(&mut self, text: &str) {
        self.line_index = Arc::new(LineIndex::new(text));
        self.text = text.to_string();
        // A whole-text set has no edit shape to record: the map from the
        // analyzed snapshot is broken until the next analysis lands.
        self.live_edits = None;
        self.refresh_keystroke_index();
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
        self.refresh_keystroke_index();
    }

    /// E121 §2.1.4: bring the edited module's entry in the symbol index back to
    /// the live buffer, but only when the buffer's declaration shape moved.
    ///
    /// The mandate's "invalidated only by that module's own edits", made exact.
    /// The common keystroke types inside a function body, leaves the stamp
    /// alone and costs one lex and one hash; only a keystroke that adds,
    /// removes, renames or re-signs a declaration pays for the rebuild. Runs on
    /// the keystroke thread and is O(file).
    fn refresh_keystroke_index(&mut self) {
        self.landed.index.refresh_entry_from_syntax(&self.text);
    }

    /// Map an ANALYZED-space byte offset into the live text, through the
    /// recorded edits — `None` when the log is unmappable. An offset inside
    /// a replaced region clamps into the replacement (the anchor's text is
    /// gone; its nearest surviving position is the honest answer).
    ///
    /// E121 retired its shipped caller: the inlay-hint handler used this to
    /// approximate a live position for an analyzed-space hint, and the
    /// keystroke path answers in live space outright, so there is nothing left
    /// to approximate. The mechanism (`live_edits`, `EditDelta`) is still the
    /// incremental-sync log B39c records and B39c's pins still hold it, so it
    /// is compiled with the tests rather than carried dead in the shipped
    /// binary — the rule `Document::references` states below.
    #[cfg(test)]
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
        self.analysis_over(program, &self.landed.index.completion)
    }

    /// The same query surface against a NAMED completion index, which is the
    /// only thing about an [`Analysis`] a caller ever needs to vary: the pins
    /// that prove the captured table answers what deriving it per request
    /// would (M25) hand in a freshly built one and compare.
    fn analysis_over<'a, 'src>(
        &'a self,
        program: &'a Program<'src>,
        index: &'a vilan_ide::CompletionIndex,
    ) -> Analysis<'a, 'src> {
        Analysis {
            program,
            analyzed: self.analyzed_index.shared(),
            live: self.line_index.shared(),
            entity_spans: &self.entity_spans,
            platform_requirements: &self.platform_requirements,
            import_roots: self.import_roots.as_ref(),
            index,
            source_texts: Default::default(),
            anchor: Default::default(),
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

    /// The LSP position for a program byte offset. Its shipped caller was the
    /// inlay-hint handler, which E121 moved onto live-space offsets and the
    /// live index; compiled with the tests for the reason
    /// `Document::references` states below.
    #[cfg(test)]
    pub fn analyzed_position(&self, offset: usize) -> Position {
        self.analyzed_index.position(offset)
    }

    /// The program byte offset for an LSP position — the inbound program-space
    /// conversion, feeding `entity_at` and the queries built on it (hover,
    /// definition, references, rename).
    pub fn analyzed_offset(&self, position: Position) -> usize {
        self.analyzed_index.offset(position)
    }

    /// Record the world revision this analysis read (E117). Called on a fresh
    /// [`Document::analyze`] result before it is landed; the value travels with
    /// the analysis through [`Document::adopt_analysis`].
    pub fn stamp_analysis(&mut self, revision: u64) {
        self.analysis_revision = revision;
    }

    /// The world revision the current analysis read — the ordering key that
    /// says which of two results is the later view (see the field).
    pub fn analysis_revision(&self) -> u64 {
        self.analysis_revision
    }

    /// The canonical `pkg::` source root this analysis resolved under — the
    /// package whose import graph colors this file (E116). `None` for a file
    /// that belongs to no project: it shares a package with nobody, so it is
    /// never swept as a peer and never sweeps one.
    pub fn package_root(&self) -> Option<&Path> {
        self.package_root.as_deref()
    }

    /// A fingerprint of the package modules this analysis reached: every
    /// `canonical_sources` entry under the package root, order-independent.
    ///
    /// E116: a file's platform color is decided by which ENTRY reaches it
    /// (`platform_color::file_platforms` walks the `pkg::` graph), so an
    /// `import pkg::a` written anywhere in the package can re-color a file that
    /// imports nothing and is imported by nobody the editor knows about. That
    /// edit is invisible to the dependency-edge gate — the unreached file does
    /// not depend on the entry, the entry depends on IT — which is why the
    /// color stuck until a restart. A change in this fingerprint is the signal
    /// that the graph moved, and the package's other open documents are swept.
    /// Std and dependency sources are excluded: they cannot change which of
    /// this package's entries reaches this package's files.
    pub fn package_graph_fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let (Some(program), Some(root)) = (self.program.as_ref(), self.package_root.as_ref())
        else {
            return 0;
        };
        let mut reached: Vec<&Path> = program
            .canonical_sources
            .iter()
            .filter(|source| source.starts_with(root))
            .map(PathBuf::as_path)
            .collect();
        reached.sort_unstable();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        reached.hash(&mut hasher);
        hasher.finish()
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
            index_time,
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
            shared_diagnostics,
            import_roots,
            analysis_revision,
            package_root,
            manifest_dir,
            unloaded_by_entries,
            generated,
            // The union is LIVE state, not an analysis product: the server owns
            // it and re-hands it at every publish, so an adoption must not
            // carry the analysis's (always absent) copy over the document's.
            package_reach: _,
            landed,
        } = analysis;
        // The analysis side, in full. `program` is the pair of the new
        // program and the allocations it borrows; assigning it drops the
        // OUTGOING pair — its program first, then its entry text and tree are
        // reclaimed (`AnalyzedProgram`'s `Drop`). This is the line the
        // session leak M7 measured (leak-soak.md §4.1) stops at.
        self.analyzed_index = analyzed_index;
        self.program = program;
        self.index_time = index_time;
        self.diagnostics = diagnostics;
        self.diagnostic_sources = diagnostic_sources;
        self.warnings = warnings;
        self.warning_sources = warning_sources;
        self.text_hash = text_hash;
        self.entity_spans = entity_spans;
        self.reference_index = reference_index;
        self.platform_requirements = platform_requirements;
        self.manifest_problem = manifest_problem;
        self.shared_diagnostics = shared_diagnostics;
        self.import_roots = import_roots;
        self.analysis_revision = analysis_revision;
        self.package_root = package_root;
        self.manifest_dir = manifest_dir;
        self.unloaded_by_entries = unloaded_by_entries;
        self.generated = generated;
        // E121: the captured answers belong to the program that produced them,
        // so they are adopted with it. Nothing is recomputed here — the walk
        // was paid on the analysis thread.
        self.landed = landed;
        // E122, and the ONE place the capture's inputs move: the analysis was
        // built without the salvage tail this adoption just computed for it, so
        // the tail is folded in HERE and the line index rebuilt over the
        // result. After this line `self.landed.tokens` is exactly what
        // `self.semantic_tokens()` would answer, which is what lets `full` and
        // `range` serve one capture instead of two pictures of it.
        fold_retained_tail(
            &mut self.landed.tokens,
            &self.retained_tail,
            self.retained_tail_start,
        );
        self.landed.index_token_lines(&self.analyzed_index);
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
        // The entry module's export list follows the LIVE buffer, not the
        // analyzed one — a `fun` typed during the analysis must complete.
        self.landed.index.refresh_entry_from_syntax(&self.text);
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
    ///
    /// A caret at a token's END counts as touching it, the same convention
    /// [`crate::references::ReferenceIndex::at`] answers rename and
    /// find-references by (E133). The two gates decide the SAME question — is
    /// the cursor on this word — for two features the user reads as one, so
    /// hover going blank at `name|` while rename works there is the two of them
    /// disagreeing rather than a separate rule. Trivia is unaffected: the end
    /// of a token is code either way, and an offset inside whitespace still
    /// touches nothing.
    fn offset_touches_a_token(&self, offset: usize) -> bool {
        let (tokens, _errors) = tokenize(self.analyzed_text());
        tokens.iter().any(|(_, span)| {
            let range = span.into_range();
            range.start <= offset && offset <= range.end
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
    ///
    /// `offset` is a LIVE offset and the spans come back in LIVE coordinates,
    /// because this parses `self.text` (E132). It used to parse
    /// `analyzed_text()`, and that was the one place a RAW PARSE — an S2
    /// citizen, owing nothing to the analysis — was handed the S1 snapshot.
    /// Nothing here is program data: there is no reason for the tag positions
    /// to lag the buffer, and one decisive reason for them not to. This
    /// handler PRODUCES EDITS by proxy — the client mirrors every keystroke
    /// from one returned range into the other — so answering in the analyzed
    /// snapshot's coordinates during the debounce pointed the mirror at
    /// whatever live text had moved into the tag's old offsets and typed into
    /// it (E132: the owner's "unrelated text deleted"; E125's twin on
    /// `semanticTokens/range`). The S3 staleness refusal every other
    /// edit-producing handler takes is the wrong cure here and only here: it
    /// would kill tag rename during exactly the typing it exists for, while
    /// the live parse makes the feature CORRECT during typing instead.
    pub fn linked_tag_ranges(&self, offset: usize) -> Option<(Span, Span)> {
        let (tree, _errors) = vilan_core::parsing::parse(&self.text);
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
            if let Some(definition) = definition
                && let Some(declaration) = program.declaration_labels.get(&definition)
            {
                return Some(self.compose_hover(program, definition, declaration, None));
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
        if let Some(definition) = self.type_declaration_target(program, id)
            && let Some(declaration) = program.declaration_labels.get(&definition)
        {
            return Some(self.compose_hover(program, definition, declaration, None));
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
    ///
    /// This is the WALK, and it is paid **once per landed analysis**: E121's
    /// [`capture_landed`](Document::capture_landed) calls it on the analysis
    /// thread and every request afterwards is served from the capture it
    /// stores (`LandedSnapshot::tokens`, and for a viewport request the line
    /// index beside it — E122). The other caller is
    /// [`compute_retained_tail`](Document::compute_retained_tail), which reads
    /// the OUTGOING analysis one last time as it is replaced.
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
        //
        // A `css` block (css-block S5) rides the very same parse for the very
        // same reason: `Node::Css` is retired before analysis too, so property
        // names and condition heads exist only in the raw tree.
        {
            let mut markup = MarkupSpans::default();
            let mut css = CssSpans::default();
            let (tree, _errors) = vilan_core::parsing::parse(self.analyzed_text());
            if let Some(root) = &tree {
                for item in &root.0 {
                    collect_markup_spans(item, &mut markup);
                    collect_css_spans(item, &mut css);
                }
            }
            let scaffolding: std::collections::HashSet<(usize, usize)> = markup
                .scaffolding
                .iter()
                .chain(css.scaffolding.iter())
                .map(|span| (span.start, span.end))
                .collect();
            if !scaffolding.is_empty() {
                tokens.retain(|(span, _, _)| !scaffolding.contains(&(span.start, span.end)));
            }
            for span in markup.tags {
                tokens.push((span, TokenKind::Tag, 0));
            }
            // E115: the angle brackets paint as the tag they belong to. Until
            // now the analyzed stream said nothing about them and the TextMate
            // grammar was the only thing coloring them — which is fine for a
            // one-line head and wrong the moment attributes spread out, because
            // a grammar rule is matched one line at a time and a `>` that lands
            // on a line of its own has no `<tag` in front of it to be part of.
            // It fell through to the operator list and read as a comparison.
            // The parse knows exactly where these are whatever shape the head
            // was written in, so the two sources agree by construction.
            for span in markup.punctuation {
                tokens.push((span, TokenKind::Tag, 0));
            }
            for span in markup.attributes {
                tokens.push((span, TokenKind::Property, 0));
            }
            // A property name is CSS, not a method reference; a condition head
            // IS the `Style` method it lowers to, and paints as one — the
            // precise form of TextMate's two approximations
            // (`support.type.property-name`, `entity.name.function`).
            for span in css.properties {
                tokens.push((span, TokenKind::Property, 0));
            }
            for span in css.conditions {
                tokens.push((span, TokenKind::Method, 0));
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
        fold_retained_tail(&mut kept, &self.retained_tail, self.retained_tail_start);
        kept
    }

    // -- E121's keystroke path (proposal/editor-latency.md §2.1) -------------
    //
    // Four entry points, each O(file) and none of which runs `Analyzer`,
    // `resolve_project_context`, or the filesystem. They answer the LIVE
    // buffer; the providers above answer the ANALYZED one and remain the
    // source the snapshot is captured from.

    /// The two-sided anchor between the analyzed text and the live buffer.
    pub fn keystroke_anchor(&self) -> Anchor {
        Anchor::compute(self.analyzed_text(), &self.text)
    }

    /// What the landed analysis is worth for the buffer as it stands (§2.1.2).
    ///
    /// `dependency_moved` is the caller's answer to the paper's case 4 —
    /// another module this analysis loaded has been edited since. The server
    /// passes `false`, and the reason is that case 4 already closes by LANDING
    /// rather than by degrading: an edit to a module this file imports leaves
    /// this file's own buffer untouched, so its anchor is the identity and the
    /// landed answer is served exactly as it is today, until
    /// `reanalyze_dependents` re-lands it. Nothing here is newly stale; the
    /// parameter is the seam a cancellation-aware scheduler would supply.
    ///
    /// The live buffer's stamp is READ, not recomputed: `set_text` and
    /// `apply_change` maintain it on the symbol index as part of the same lex
    /// the index needs anyway, so a verdict costs a comparison and the anchor's
    /// two byte scans. It falls back to hashing when no index entry exists,
    /// which is only the never-analyzed document.
    pub fn keystroke_verdict(&self, dependency_moved: bool) -> Verdict {
        if !self.landed.landed {
            return Verdict::Unusable;
        }
        let anchor = self.keystroke_anchor();
        let live_stamp = self
            .landed
            .index
            .entry_stamp()
            .unwrap_or_else(|| shape_stamp(&self.text));
        Verdict::decide(&anchor, live_stamp == self.landed.stamp, dependency_moved)
    }

    /// Semantic tokens for the LIVE buffer, in LIVE coordinates.
    ///
    /// The landed stream re-mapped through the anchor, plus the edit window
    /// painted from syntax alone — and, when a name binding moved or no anchor
    /// survives, syntax alone for the whole file. The cost is one anchor
    /// (two byte scans), a filter over the landed stream, and one lex: O(file),
    /// and flat in the size of the analyzed program, which is the whole
    /// mandate.
    pub fn keystroke_tokens(&self, dependency_moved: bool) -> Vec<(Span, TokenKind, u32)> {
        let anchor = self.keystroke_anchor();
        let verdict = self.keystroke_verdict(dependency_moved);
        self.landed.tokens_for(&self.text, &anchor, verdict)
    }

    /// Semantic tokens for the LIVE buffer over one VIEWPORT, in LIVE
    /// coordinates — `semanticTokens/range`'s answer (E125).
    ///
    /// Byte for byte the window of [`keystroke_tokens`](Self::keystroke_tokens)
    /// whose tokens START on a line in `first_line..=last_line`, and that
    /// equality is the whole point of the method. Before E125 this request
    /// sliced the capture in the ANALYZED snapshot's coordinates while `full`
    /// re-served the same capture through the anchor, so the two answered two
    /// pictures: a viewport request after an unlanded edit ABOVE the window
    /// painted the window's tokens at the lines they occupied before the edit,
    /// and they stayed there until the next analysis landed — the exact drift
    /// the keystroke path exists to remove, on the request an editor sends
    /// most.
    ///
    /// The cost still follows the WINDOW, which is what E122 bought and what
    /// this must not spend. The capture is indexed by ANALYZED line, and the
    /// anchor moves an analyzed line by a constant — zero through the head,
    /// [`tail_line_shift`](Self::tail_line_shift) through the tail — so the
    /// analyzed lines that can carry a token into a live window are two
    /// contiguous stretches of that index, and the viewport is read as at most
    /// two slices rather than as a scan. The edit window's syntax is lexed
    /// only when the window's own lines meet the requested ones.
    pub fn keystroke_tokens_in_lines(
        &self,
        first_line: u32,
        last_line: u32,
        dependency_moved: bool,
    ) -> Vec<(Span, TokenKind, u32)> {
        if first_line > last_line {
            return Vec::new();
        }
        let anchor = self.keystroke_anchor();
        let verdict = self.keystroke_verdict(dependency_moved);
        let requested = |span: &Span| {
            let line = self.line_index.range(span).start.line;
            line >= first_line && line <= last_line
        };
        let mut painted: Vec<(Span, TokenKind, u32)> = Vec::new();
        if verdict == Verdict::Exact {
            let shift = self.tail_line_shift(&anchor);
            let head = self.landed.token_positions_in_lines(first_line, last_line);
            let tail = self.landed.token_positions_in_lines(
                shift_line(first_line, -shift),
                shift_line(last_line, -shift),
            );
            for position in merge_positions(head, tail) {
                let (span, kind, modifiers) = self.landed.tokens[position];
                if let Some(span) = anchor.map_span(span)
                    && requested(&span)
                {
                    painted.push((span, kind, modifiers));
                }
            }
            // Q5: the edit window is syntax-only in every verdict. A window
            // that does not reach the requested lines can contribute nothing
            // to them — every token it holds starts inside it — so the lex is
            // not paid at all for a viewport away from the cursor.
            let window = anchor.live_window();
            if !window.is_empty() {
                let window_first = self.line_index.position(window.start).line;
                let window_last = self.line_index.position(window.end - 1).line;
                if window_first <= last_line && window_last >= first_line {
                    painted.extend(
                        syntax_tokens_in(&self.text, window)
                            .into_iter()
                            .filter(|(span, _, _)| requested(span)),
                    );
                }
            }
        } else {
            // Stale and Unusable degrade exactly as `full` does — the whole
            // file from syntax alone — and the viewport is that answer's
            // window. Syntax is never wrong, so no line of it is withheld.
            painted.extend(
                syntax_tokens_in(&self.text, 0..self.text.len())
                    .into_iter()
                    .filter(|(span, _, _)| requested(span)),
            );
        }
        sort_and_deoverlap(painted)
    }

    /// How many LINES the anchor's tail moved: the live line of the first byte
    /// of the common suffix, minus its analyzed line.
    ///
    /// Byte identity gives the offset shift; this is the same fact counted in
    /// lines, which is what a viewport is addressed in. Zero when the two
    /// texts are identical (the suffix is empty and both offsets are the end
    /// of their text), so an unedited buffer reads the capture's own line
    /// index unchanged.
    fn tail_line_shift(&self, anchor: &Anchor) -> i64 {
        let analyzed_suffix_start = anchor.analyzed_len.saturating_sub(anchor.suffix);
        let live_suffix_start = anchor.live_len.saturating_sub(anchor.suffix);
        self.line_index.position(live_suffix_start).line as i64
            - self.analyzed_index.position(analyzed_suffix_start).line as i64
    }

    /// Inlay hints for the LIVE buffer, in LIVE coordinates — Q1/Q4's ruling:
    /// re-mapped through the anchor, withheld inside the edit window, served
    /// unchanged rather than flickered off when stale, withheld entirely when
    /// no anchor survives.
    pub fn keystroke_hints(&self, dependency_moved: bool) -> Vec<(usize, String)> {
        let anchor = self.keystroke_anchor();
        let verdict = self.keystroke_verdict(dependency_moved);
        self.landed.hints_for(&anchor, verdict)
    }

    /// Completion candidates at a LIVE `offset`, answered from the symbol
    /// index and the cursor's syntactic context (§2.1.4).
    ///
    /// Three shapes, and what each may read:
    ///
    /// - **scope** — the edited module's own declarations, read from the live
    ///   buffer's syntax, so a `fun` typed one keystroke ago completes. Only
    ///   its own: a name another module declares is not in scope here without
    ///   an import, and reaching it is auto-import's job. Measured on a kolt
    ///   copy, adding every loaded module's names to this arm turned a
    ///   125-candidate scope completion into a 3,144-candidate one.
    /// - **`module::`** — that module's entry in the index, by the name a path
    ///   spells it with. This is the arm the index straightforwardly replaces:
    ///   no `read_dir`, no per-module `name_to_id_map` sweep.
    /// - **`receiver.`** — a member list is a *type* question and the index
    ///   cannot answer it. When the verdict is [`Verdict::Exact`] the LANDED
    ///   analysis answers (a read of a finished `Program`, not a type-check);
    ///   otherwise the receiver's binding may have moved, and the module's own
    ///   names are offered instead of a member list that would be a lie of the
    ///   kind Q4 rules against for hints.
    ///
    /// The landed engine then fills in everything only resolution can supply —
    /// locals, members, keywords, snippets, auto-imports — and a label the
    /// index already offered is dropped rather than repeated. The index goes
    /// FIRST because it is the only source that knows what the live buffer
    /// declares: a `fun` typed one keystroke ago is in it and cannot be in the
    /// landed analysis.
    pub fn keystroke_completion(&self, offset: usize, dependency_moved: bool) -> Vec<Completion> {
        self.keystroke_completion_over(offset, dependency_moved, &self.landed.index.completion)
    }

    /// The whole answer, with the analysis-side completion index named rather
    /// than taken from the capture.
    ///
    /// M25's identity pin hands in an index derived AT REQUEST TIME — which is
    /// what the engine used to do on every keystroke — and asserts the two
    /// answers are the same candidates. That is the one property capturing the
    /// table can break, so it is the one the pin holds.
    fn keystroke_completion_over(
        &self,
        offset: usize,
        dependency_moved: bool,
        completion_index: &vilan_ide::CompletionIndex,
    ) -> Vec<Completion> {
        let verdict = self.keystroke_verdict(dependency_moved);
        let context = cursor_context(&self.text, offset);
        if context == CursorContext::None {
            return Vec::new();
        }
        let index = &self.landed.index;
        let entry_names = |prefix: &str| {
            index
                .by_module
                .get(SymbolIndex::ENTRY)
                .map(|entry| candidates(&entry.entries, prefix))
                .unwrap_or_default()
        };
        let mut offered = match &context {
            // Handled above; the arm keeps the match total without a
            // catch-all that would swallow a future context.
            CursorContext::None => Vec::new(),
            // The EDITED module's own declarations, and deliberately nothing
            // else. A name another module declares is not in scope here without
            // an import, and offering it as though it were is both wrong and
            // expensive: measured on a kolt copy, adding every loaded module's
            // names turned a 125-candidate scope completion into a
            // 3,144-candidate one. Reaching those names is auto-import's job,
            // which the landed engine below already does with the ranking tiers
            // and the `additionalTextEdits` that insert the import.
            CursorContext::Scope { prefix } => entry_names(prefix),
            // A one-segment path is the arm the index straightforwardly
            // replaces. A NESTED one is not: the index is keyed by module
            // name, `module` here is only the path's last segment, and
            // answering `style::FlexDirection::` with whatever module happens
            // to be called `FlexDirection` would be a lie. The descent is a
            // resolution question — the landed engine's
            // `code_path_completions` walks it (E129), and this defers.
            CursorContext::Path {
                module,
                prefix,
                nested,
            } => {
                if *nested {
                    Vec::new()
                } else {
                    index
                        .module(module)
                        .map(|module| candidates(&module.entries, prefix))
                        .unwrap_or_default()
                }
            }
            // A member position in the exact state: the landed analysis owns
            // the member list, and it arrives below. Otherwise the receiver's
            // binding may have moved, and the module's own names are the
            // honest offer.
            CursorContext::Member { prefix } => match verdict {
                Verdict::Exact => Vec::new(),
                Verdict::Stale | Verdict::Unusable => entry_names(prefix),
            },
        };
        // The landed engine's resolution-derived candidates, deduplicated by
        // label. It reads the last completed analysis and never runs
        // `Analyzer`, and since M25 it derives nothing whole-program per
        // request either: `auto_import_completions` reads the captured
        // candidate table and an import path's origin arm reads the captured
        // module listing, so neither the per-module `name_to_id_map` sweep nor
        // `modules_in_root`'s `read_dir` is on this path any more.
        for candidate in self.completion_over(offset, completion_index) {
            if !offered
                .iter()
                .any(|existing| existing.label == candidate.label)
            {
                offered.push(candidate);
            }
        }
        offered
    }

    /// The same answer with the completion index derived from THIS request
    /// instead of read from the capture — the pre-M25 engine, for the pin that
    /// holds the two to one answer.
    #[cfg(test)]
    pub(crate) fn keystroke_completion_rebuilding_index(
        &self,
        offset: usize,
        dependency_moved: bool,
    ) -> Vec<Completion> {
        let Some(program) = self.program.as_ref() else {
            return Vec::new();
        };
        let index = vilan_ide::CompletionIndex::build(
            program,
            self.import_roots.as_ref(),
            self.analyzed_text(),
        );
        self.keystroke_completion_over(offset, dependency_moved, &index)
    }

    /// The keystroke path's own view of the symbol index, for the pins.
    #[cfg(test)]
    pub(crate) fn keystroke_index(&self) -> &SymbolIndex {
        &self.landed.index
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
    ///
    /// The single-document face, driven by this crate's pins: the server always
    /// answers through [`Self::references_across`] (kolt.local 034's
    /// cross-document union), so this is compiled with the tests rather than
    /// carried dead in the shipped binary.
    #[cfg(test)]
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
    ///
    /// The single-document face, driven by this crate's pins, exactly as
    /// [`Self::references`] is: the server renames through
    /// [`Self::rename_edits_across`].
    #[cfg(test)]
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

        // E134: a struct-init field shorthand `A { x }` is ONE identifier
        // naming two definitions — the field key and the local it reads — so
        // there is no rewrite of that span that serves both. Refuse from
        // either side rather than emit the edit that silently breaks the other
        // name (which is what renaming the field used to do). The refusal
        // names the expansion that gives each name a span of its own.
        if let Some(other) = self
            .reference_index()
            .occurrences_of(definition)
            .find_map(|occurrence| occurrence.shared_with(definition))
        {
            let name = crate::references::name_of(program, definition).unwrap_or("this symbol");
            let with = match crate::references::kind_of(program, other) {
                Some(kind) => format!("the {} `{name}`", kind.noun()),
                None => format!("`{name}`"),
            };
            return Err(RenameRefusal::SharedSpan {
                what: what.to_string(),
                with,
                name: name.to_string(),
            });
        }

        // The index guarantees one row per `(source, span)`, so this cannot
        // hold a duplicate — but a duplicate span is what the CLIENT rejects
        // ("Rename failed to apply edits"), so the guarantee is re-stated
        // where the edit set is actually produced rather than relied on from
        // two layers away.
        let mut spans: Vec<(SourceId, Span)> = self
            .reference_index()
            .occurrences_of(definition)
            .map(|occurrence| (occurrence.source, occurrence.span))
            .collect();
        spans.sort_by_key(|(source, span)| (source.0, span.start, span.end));
        spans.dedup();
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

    /// The top-level import leaves nothing in this file uses (E114) — the spans
    /// the editor FADES, in the analyzed text's coordinates.
    ///
    /// Paint, not a warning: an unused import is a tidiness observation, so it
    /// publishes at hint severity with `DiagnosticTag::Unnecessary` and never
    /// enters a count or gates anything (see `publish::diagnostic_groups`).
    ///
    /// It is the ORGANIZER's answer, not a second one: the same leaf walk
    /// (`formatter::import_leaf_name_spans`) and the same usage test
    /// ([`Document::import_leaf_is_used`], which counts type positions, value
    /// positions and struct constructors, and counts references from code
    /// generated out of this file so a derive-only import survives). Whatever
    /// Organize Imports would prune is what fades, which is the only honest
    /// relationship between a mark and the fix offered for it.
    ///
    /// Conservative in exactly the organizer's two ways, because a mark that
    /// lies is worse than no mark: nothing fades while the buffer is ahead of
    /// the analysis, and nothing fades in a file that carries a diagnostic — a
    /// half-typed name might be about to use the very import in question.
    /// Re-exports are not leaves here at all.
    pub fn unused_import_spans(&self) -> Vec<Span> {
        let Some(program) = self
            .program
            .as_ref()
            .filter(|_| self.diagnostics.is_empty() && !self.is_stale())
        else {
            return Vec::new();
        };
        // The ANALYZED text, so the spans this returns are in the coordinates
        // the publisher converts through — and equal to the live text anyway,
        // since a stale document decides nothing above.
        let source = self.analyzed_text();
        let import_spans = vilan_core::formatter::import_statement_spans(source);
        vilan_core::formatter::import_leaf_name_spans(source)
            .into_iter()
            .filter(|leaf| !self.import_leaf_is_used(program, *leaf, &import_spans))
            .collect()
    }

    /// The local bindings nothing in this file reads (E114's declarations
    /// third) — the spans the editor FADES, in the analyzed text's coordinates.
    ///
    /// **Local, and only local, because Vilan has no other private scope.**
    /// There is no visibility marker in the language: `pub fun helper()` is a
    /// parse error whose curated rule says so ("a module's items are importable
    /// as they stand"), and `module_importables` will bind ANY top-level name an
    /// `import` asks for. So a top-level `fun`/`struct`/`enum`/`let` is module
    /// surface — a file the editor never analyzed may import it — and fading one
    /// on the strength of a single-entry analysis would be a guess. A function
    /// body is the one scope the language genuinely closes: nothing outside it
    /// can name a `let` declared inside, so "unreferenced here" IS "dead", with
    /// no world the editor cannot see. (See the lane's report for the
    /// whole-package design that WOULD reach the top level, and its cost.)
    ///
    /// Paint, not a warning, exactly like the imports third: hint severity,
    /// `DiagnosticTag::Unnecessary`, out of every count (`publish::
    /// diagnostic_groups`).
    ///
    /// Conservative in the imports third's two ways plus two of its own:
    ///  - nothing fades while the buffer is ahead of the analysis, and nothing
    ///    fades in a file carrying a diagnostic (a half-typed line might be
    ///    about to read the binding);
    ///  - an `_`-led name is the language's own "I know" marker (`let _ = …` is
    ///    what the `[must_use]` rule tells you to write), so it never fades;
    ///  - a binding whose reference index DROPPED a use site (a span that could
    ///    not be narrowed onto its identifier) is kept: an incomplete tally is
    ///    no evidence of zero.
    ///
    /// Parameters are deliberately not here. A parameter is signature, not a
    /// local: a trait impl must take what the declaration takes, so an unused
    /// one is frequently obligatory rather than dead.
    pub fn unused_local_spans(&self) -> Vec<Span> {
        let Some(program) = self
            .program
            .as_ref()
            .filter(|_| self.diagnostics.is_empty() && !self.is_stale())
        else {
            return Vec::new();
        };
        // The entry's id ranges first, because every later test is cheaper than
        // `source_of` (a linear scan of `source_ranges`, which asked per
        // variable is that scan re-run once per row).
        let entry_ids = program.id_ranges_of(SourceId(0));
        let module_level: HashSet<Id> = program.module_level_bindings().into_iter().collect();
        program
            .variables
            .iter()
            .filter(|(id, _)| entry_ids.iter().any(|range| range.contains(&id.0)))
            .filter(|(id, _)| !module_level.contains(*id))
            .filter(|(_, variable)| !variable.name.starts_with('_'))
            .filter(|(id, _)| {
                let definition = Definition::Entity(**id);
                self.reference_index.dropped_for(definition) == 0
                    && self
                        .reference_index
                        .occurrences_of(definition)
                        .all(|occurrence| occurrence.is_declaration_of(definition))
            })
            .map(|(_, variable)| variable.name_span)
            .collect()
    }

    /// E124's module-level slice: this file's top-level items, faded whole,
    /// because NO entry of the package loads the module — with the message
    /// naming the entries that were asked.
    ///
    /// This is the coarse half of the dead-item paint and the one that costs
    /// nothing: `platform_color::file_platform_choices` already runs a
    /// per-entry module-level walk per keystroke to decide this file's colour,
    /// and "no entry reached it" is that walk's own answer read a second way
    /// (`dead-code-paint.md` §2.5). It also has none of the fine paint's
    /// false-gray classes — the question is about the FILE, so nothing about
    /// dispatch refinement, const initializers or context rewrites can make it
    /// wrong — and it needs no cache and no clock.
    ///
    /// What it fades is deliberately the same two item kinds the fine paint
    /// covers: a top-level `fun` and a module-level `let`. An unloaded module's
    /// `struct` is as dead as its functions, but the owner's narrowing is the
    /// narrowing — types are a different analysis — and a user who sees one
    /// rule at two granularities can hold it in their head.
    ///
    /// Gated exactly as E114's three producers are: nothing fades while the
    /// buffer is ahead of the analysis, and nothing fades in a file carrying a
    /// diagnostic.
    pub fn unloaded_module_paint(&self) -> Option<(String, Vec<Span>)> {
        let entries = self.unloaded_by_entries.as_ref()?;
        let program = self
            .program
            .as_ref()
            .filter(|_| self.diagnostics.is_empty() && !self.is_stale())?;
        let spans: Vec<Span> = vilan_core::dead_items::paintable_items(program, SourceId(0))
            .into_iter()
            .map(|item| item.name_span)
            .collect();
        if spans.is_empty() {
            return None;
        }
        let named = entries
            .iter()
            .map(|entry| format!("`{entry}`"))
            .collect::<Vec<_>>()
            .join(", ");
        Some((
            format!("no entry loads this module (the package builds {named})"),
            spans,
        ))
    }

    /// E124's fine paint: the top-level `fun`s and module-level `let`s of this
    /// file that NO entry of the package reaches — the spans the editor fades.
    ///
    /// The union is not computed here and cannot be. The language server
    /// analyzes the OPEN file as the entry, so for most files there is no
    /// `main` in the program at all and the reachability walk has no root to
    /// start from (`dead-code-paint.md` §2.1, probe P5) — every term of the
    /// union, including the one for the entry this file belongs to, comes from
    /// a separately computed per-entry set on the package clock
    /// (`crate::dead_items`). What happens here is the cheap half: this file's
    /// candidates, minus the union, matched on `(canonical path, name span)`
    /// because entity ids are minted per analysis and are not comparable across
    /// the entries' three programs.
    ///
    /// Empty whenever there is no union in hand, which is the withdrawal: an
    /// edit anywhere in the package drops it, and it returns when the clock
    /// lands. Empty too under a declared `generated` root, and — through
    /// `manifest_dir` being `None` — for a `[library]` and for a file with no
    /// project.
    ///
    /// Gated as E114's producers are, and it needs the gate more: a salvaged
    /// parse can lose a whole block or the file's entire tail, and **a smaller
    /// program reads to a reachability walk as a deader one** (§3.3).
    pub fn dead_item_spans(&self) -> Vec<Span> {
        if self.generated {
            return Vec::new();
        }
        let (Some(reach), Some(program)) = (
            self.package_reach.as_ref(),
            self.program
                .as_ref()
                .filter(|_| self.diagnostics.is_empty() && !self.is_stale()),
        ) else {
            return Vec::new();
        };
        // The module-level slice already fades this whole file; saying it twice
        // over the same spans would publish two hints per declaration.
        if self.unloaded_by_entries.is_some() {
            return Vec::new();
        }
        let Some(path) = program.canonical_sources.first() else {
            return Vec::new();
        };
        vilan_core::dead_items::paintable_items(program, SourceId(0))
            .into_iter()
            .filter(|item| {
                !reach.reached.contains(&vilan_core::dead_items::ItemKey {
                    path: path.clone(),
                    name_span: item.name_span,
                })
            })
            .map(|item| item.name_span)
            .collect()
    }

    /// The directory of the `vilan.toml` this document's package is declared in
    /// — the key of E124's per-package clock. `None` for a `[library]` and for
    /// a file with no project, both of which get no top-level gray.
    pub fn manifest_dir(&self) -> Option<&Path> {
        self.manifest_dir.as_deref()
    }

    /// Hand this document the package union to paint from, or `None` to
    /// withdraw. Called by the server at publish time and nowhere else — the
    /// union belongs to the package, not to any one buffer.
    pub fn set_package_reach(&mut self, reach: Option<Arc<crate::dead_items::PackageReach>>) {
        self.package_reach = reach;
    }

    /// The statements this file can never reach (E114's unreachable third) — the
    /// spans the editor FADES, in the analyzed text's coordinates. One span per
    /// block, covering the whole dead tail rather than one mark per statement:
    /// what died is the REST of the block, and N faded lines say that N times.
    ///
    /// It is the CHECKER's divergence analysis, not a second one
    /// ([`vilan_core::analyzer::Divergence`], which the analyzer's own
    /// `block_diverges` calls too) — and since B204 it is the checker's answer
    /// exactly, not a widening of it: `ret`, `jump` (the loop tails), a
    /// `panic(…)` call (which lowers to a `throw`), an endless `for { … }`
    /// nothing breaks out of — `for` with no condition being the language's
    /// only endless-loop form, since `for cond { … }` is the `while` and
    /// `for … in` finishes with its iterable — an `if` whose every arm diverges
    /// *and* has an `else`, and a `match` whose every arm diverges. What fades
    /// here is exactly what the checker treats as dead.
    ///
    /// Conservative in the imports third's two ways — nothing fades while the
    /// buffer is ahead of the analysis, nothing fades in a file carrying a
    /// diagnostic — plus the two this walk needs: a dead region is reported only
    /// where every statement in it carries a real span IN THIS FILE (a
    /// desugaring's synthesized statement borrows a span it did not write, and a
    /// synthesized void tail has none at all), and a region whose span does not
    /// grow is dropped rather than published as an empty range.
    pub fn unreachable_spans(&self) -> Vec<Span> {
        let Some(program) = self
            .program
            .as_ref()
            .filter(|_| self.diagnostics.is_empty() && !self.is_stale())
        else {
            return Vec::new();
        };
        let entry_ids = program.id_ranges_of(SourceId(0));
        let divergence = vilan_core::analyzer::Divergence::of_program(program);
        let mut spans: Vec<Span> = Vec::new();
        for (statements, tail) in block_regions(program, &entry_ids) {
            let Some(diverging) = statements
                .iter()
                .position(|statement| divergence.expr(*statement))
            else {
                continue;
            };
            // Everything after the diverging statement, as ONE range — and only
            // if every piece of it is code this file actually wrote. The tail is
            // asked separately, and asked TWICE, because a block's trailing
            // expression is usually synthesized: `fun f() { ret; }` ends in a
            // `Void` whose recorded span is the closing BRACE (the S3 callable
            // anchor). It passes every span test and is not code at all, so it
            // is excluded by its expression rather than by its span — and being
            // excluded it must also not veto the real dead statements before it.
            let dead = &statements[diverging + 1..];
            let mut extent = self.written_extent(program, &entry_ids, dead);
            if !dead.is_empty() && extent.is_none() {
                continue;
            }
            let tail_is_written = !matches!(program.entity_map.get(&tail), Some(Expr::Void) | None);
            if let Some(tail_span) = tail_is_written
                .then(|| self.written_extent(program, &entry_ids, std::slice::from_ref(&tail)))
                .flatten()
            {
                extent = Some(match extent {
                    Some(existing) => union(existing, tail_span),
                    None => tail_span,
                });
            }
            if let Some(span) = extent {
                spans.push(span);
            }
        }
        spans.sort_by_key(|span| (span.start, span.end));
        spans.dedup();
        spans
    }

    /// The span covering `entities`, or `None` unless every one of them is code
    /// WRITTEN IN THIS FILE: a real, non-empty span, in the entry source, inside
    /// the analyzed text. A desugaring's synthesized statement fails this, which
    /// is the whole reason it is asked — fading a range the user cannot see (or
    /// one that belongs to a different file) is the mark that lies. An empty
    /// `entities` answers `None`: there is nothing to fade.
    fn written_extent(
        &self,
        program: &Program,
        entry_ids: &[std::ops::Range<u32>],
        entities: &[Id],
    ) -> Option<Span> {
        let limit = self.analyzed_text().len();
        let mut extent: Option<Span> = None;
        for id in entities {
            if !entry_ids.iter().any(|range| range.contains(&id.0)) {
                return None;
            }
            let span = program.span_map.get(id)?;
            if span.start >= span.end || span.end > limit {
                return None;
            }
            extent = Some(match extent {
                Some(existing) => union(existing, **span),
                None => **span,
            });
        }
        extent
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
        // (0) The PRELUDE already binds this definition ambiently
        // (`prelude.md` §11.1): the import is redundant, so removing it cannot
        // change what the file means, and leaving it would have the estate
        // carry hundreds of dead statements that every copy-pasted new file
        // reproduces. Matched on the DEFINITION, not the name — `import
        // my_lib::print;` beside an ambient `std::io::print` is not redundant and
        // survives. This is the action's existing contract ("prune the leaves
        // the analyzer reports as unused") reaching one more kind of unused.
        if program.prelude_bindings.contains(&definition_id) {
            return false;
        }
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
        ) && let Some(home) = program.source_of(definition_id)
        {
            // A module whose file is this one brings nothing new, and would
            // otherwise match every local declaration and never prune.
            if home != entry {
                return self
                    .reference_index
                    .occurrences_in(entry)
                    .any(|occurrence| {
                        !written_in_an_import(occurrence.span)
                            && crate::references::declaration_source(program, occurrence.definition)
                                == Some(home)
                    });
            }
        }
        false
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
            if let Some(surface_path) = &surface
                && vilan_core::analyzer::module_importables(surface_path)
                    .iter()
                    .any(|importable| importable.name == name)
            {
                candidates.push(vec![origin.clone()]);
            }
            let mut seen_modules: HashSet<String> = HashSet::new();
            for root in &module_roots {
                for (module_name, module_path) in vilan_core::analyzer::modules_in_root(root) {
                    if module_name == "lib" || !seen_modules.insert(module_name.clone()) {
                        continue;
                    }
                    // Only a module that DECLARES the name is an add-import
                    // target. This is the analyzer's own rule for the B4 steer
                    // (`collect_declared_names`: "pointing at a module that
                    // merely forwards it would name the wrong file"), and the
                    // quickfix path had drifted from it — harmlessly until std
                    // gained the prelude modules, whose whole content is
                    // re-exports, at which point `view` started offering both
                    // `std::ui` and `std::web` and the menu went ambiguous.
                    // Nobody should ever be told to `import std::web::view`.
                    if vilan_core::analyzer::module_importables(&module_path)
                        .iter()
                        .any(|importable| {
                            importable.name == name
                                && importable.kind != vilan_core::analyzer::ImportableKind::Reexport
                        })
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
            } else if diagnostic.msg.starts_with(HASH_IS_NOT_A_TOKEN)
                && let Some(span) = hex_colour_span(&self.text, diagnostic.span.start)
            {
                // css-block S5, §7.2 fix 1. The lexer is context-free, so the
                // diagnostic is ONE character wide (`#`) — the colour it
                // belongs to is read off the text here, and the fix is the
                // hole spelling the diagnostic already names, so the two
                // cannot disagree (E58c's rule, applied to a curated rule
                // instead of a note).
                let hole = format!("{{Color::hex(\"{}\")}}", &self.text[span.into_range()]);
                fixes.push(QuickFix {
                    title: format!("Wrap as `{hole}`"),
                    span,
                    replacement: hole,
                });
            } else if diagnostic.msg.starts_with(AT_IS_NOT_A_TOKEN)
                && let Some(fix) = media_rule_fix(&self.text, diagnostic.span.start)
            {
                // §7.2 fix 2, the `#`'s twin: the one at-rule with a
                // combinator spelling is a min-width media query.
                fixes.push(fix);
            } else if diagnostic.msg.starts_with(IMPORTANT_HAS_NO_PLACE) {
                // §7.2 fix 3. The parser excises `!important` from the value
                // and reports at exactly its span, so the fix is that span
                // plus the whitespace holding it to the value — removing the
                // marker alone would leave `flex ;`.
                let start = self.text[..diagnostic.span.start]
                    .trim_end_matches([' ', '\t'])
                    .len();
                fixes.push(QuickFix {
                    title: "Remove `!important`".to_string(),
                    span: Span::from(start..diagnostic.span.end),
                    replacement: String::new(),
                });
            }
        }
        fixes
    }

    /// The `css`-spelling conversion offered over `range` (LIVE space, and the
    /// caller gates staleness first exactly as it does for [`Self::quickfixes`])
    /// — css-block.md §7.2's one refactor, and the estate's migration path.
    ///
    /// Read from a RAW parse, like every other css query: neither spelling's
    /// distinguishing node survives desugaring, and the block is the one that
    /// does not survive at all. The block direction is tried first, because a
    /// cursor in a block is never in a chain but a cursor in a chain LINK's
    /// argument may be in a block.
    pub fn css_spelling_conversion(&self, range: Span) -> Option<CssConversion> {
        let source = self.text.as_str();
        let (tree, _errors) = vilan_core::parsing::parse(source);
        let root = tree?;
        let commented = |span: Span| {
            vilan_core::formatter::extract_comments(source)
                .iter()
                .any(|(comment, _)| spans_overlap(span, *comment))
        };
        let mut block = None;
        for item in &root.0 {
            innermost_css_node(item, range.start, &mut block);
        }
        if let Some(node) = block {
            let Node::Css(body) = &node.0 else {
                unreachable!("innermost_css_node only records a `Node::Css`");
            };
            // A comment's attachment is not recoverable across the reshape —
            // the S3 printer refuses to reorder a commented block for exactly
            // this reason, and inventing a placement here would be worse.
            if commented(node.1) {
                return None;
            }
            return Some(CssConversion {
                to_chain: true,
                span: node.1,
                replacement: render_style_chain(body, source, &line_indent(source, node.1.start))?,
            });
        }
        let mut chain = None;
        for item in &root.0 {
            outermost_style_chain(item, range.start, &mut chain);
        }
        let node = chain?;
        if commented(node.1) {
            return None;
        }
        Some(CssConversion {
            to_chain: false,
            span: node.1,
            replacement: render_css_block(node, source, &line_indent(source, node.1.start))?,
        })
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
            if let Some(name) = unresolved_name(&diagnostic.msg)
                && !names.contains(&name)
            {
                names.push(name);
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
    ///
    /// The server reaches it through [`keystroke_completion`](Self::keystroke_completion),
    /// which offers the index's own candidates first and these after; this is
    /// the engine on its own, which is what the completion pins drive.
    #[cfg(test)]
    pub(crate) fn completion(&self, offset: usize) -> Vec<Completion> {
        self.completion_over(offset, &self.landed.index.completion)
    }

    /// [`completion`](Self::completion) against a NAMED completion index —
    /// the seam M25's identity pin measures the capture against, by handing in
    /// one derived on the spot.
    fn completion_over(
        &self,
        offset: usize,
        index: &vilan_ide::CompletionIndex,
    ) -> Vec<Completion> {
        let Some(program) = self.program.as_ref() else {
            return Vec::new();
        };
        self.analysis_over(program, index).completion(offset)
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
        if let Some(rest) = message.strip_prefix(prefix)
            && let Some(end) = rest.find('\'')
        {
            return Some(&rest[..end]);
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

/// One direction of the `css`-block ⇄ `style()`-chain conversion (css-block.md
/// §7.2's refactor). The two spellings are interconvertible because the lowering
/// is TOTAL and one-to-one (§5.2): a declaration is a `.raw` link, a nested rule
/// is a combinator link carrying the inner chain as its final argument, and
/// there is no third row.
///
/// Both directions DECLINE rather than guess, which is the whole of what makes a
/// refactor safe to offer from a menu. The three refusals, each about meaning
/// rather than shape:
///
/// - a **comment** anywhere inside the construct;
/// - a value whose text carries a **backslash** — a chain's string literal has
///   its escapes processed at emission and a block's token run does not, so the
///   two spellings would stop meaning the same thing (a `"` is fine: escaping it
///   into the literal round-trips exactly);
/// - a chain link that is neither `.raw(<name>, …)` nor a condition combinator.
///   `.padding(space(4))` lowers to `with_length("padding", …)`, which is NOT
///   the node `padding: {space(4)};` lowers to — so the inverse is partial by
///   construction, and says so by not being offered.
pub struct CssConversion {
    /// `true` for block → chain, `false` for chain → block. The server picks
    /// the action's title from it, as a literal, so `book_sync.rs` can hold the
    /// book's editor page to both.
    pub to_chain: bool,
    pub span: Span,
    pub replacement: String,
}

/// The innermost `Node::Css` containing `offset`. Nested rules are not nodes, so
/// this narrows only across blocks written inside one another's holes.
fn innermost_css_node<'a, 'src>(
    node: &'a vilan_core::Spanned<vilan_core::node::Node<'src>>,
    offset: usize,
    out: &mut Option<&'a vilan_core::Spanned<vilan_core::node::Node<'src>>>,
) {
    if matches!(node.0, Node::Css(_)) && node.1.start <= offset && offset <= node.1.end {
        *out = Some(node);
    }
    node.0
        .for_each_child(&mut |child| innermost_css_node(child, offset, out));
}

/// The OUTERMOST `style()`-seeded chain containing `offset` — the whole chain,
/// never a link's own inner one, which is why the walk stops at its first hit.
fn outermost_style_chain<'a, 'src>(
    node: &'a vilan_core::Spanned<vilan_core::node::Node<'src>>,
    offset: usize,
    out: &mut Option<&'a vilan_core::Spanned<vilan_core::node::Node<'src>>>,
) {
    if out.is_some() {
        return;
    }
    if node.1.start <= offset
        && offset <= node.1.end
        && style_chain_links(node).is_some_and(|links| !links.is_empty())
    {
        *out = Some(node);
        return;
    }
    node.0
        .for_each_child(&mut |child| outermost_style_chain(child, offset, out));
}

/// The links of a `style()`-seeded chain, in written order, or `None` when
/// `node` is some other expression. The seed is a bare `style()` call — a
/// receiver of any other shape is not a chain this refactor can read.
fn style_chain_links<'a, 'src>(
    node: &'a vilan_core::Spanned<vilan_core::node::Node<'src>>,
) -> Option<Vec<&'a vilan_core::Spanned<vilan_core::node::Node<'src>>>> {
    match &node.0 {
        Node::MemberAccessor(subject, member) => {
            let mut links = style_chain_links(subject)?;
            links.push(member);
            Some(links)
        }
        Node::Call(callee, None, arguments)
            if matches!(callee.0, Node::Accessor("style")) && arguments.0.is_empty() =>
        {
            Some(Vec::new())
        }
        _ => None,
    }
}

/// The leading whitespace of the line `offset` sits on — the indentation the
/// converted construct is written at.
fn line_indent(source: &str, offset: usize) -> String {
    let line_start = source[..offset].rfind('\n').map(|at| at + 1).unwrap_or(0);
    source[line_start..offset]
        .chars()
        .take_while(|character| *character == ' ' || *character == '\t')
        .collect()
}

/// A `css` body as the `style()` chain it lowers to. One link per line at
/// `indent` + 1, except that a body of a single declaration renders inline —
/// the shape S3 gives the outer body itself, and the shape the corpus's own
/// chain twin is written in.
fn render_style_chain(
    body: &vilan_core::node::CssBody<'_>,
    source: &str,
    indent: &str,
) -> Option<String> {
    let inline = body.items.len() <= 1
        && !body
            .items
            .iter()
            .any(|item| matches!(item, CssItem::Nested(_)));
    let mut out = String::from("style()");
    for item in &body.items {
        let link = render_chain_link(item, source)?;
        if !inline {
            out.push('\n');
            out.push_str(indent);
            out.push('\t');
        }
        out.push_str(&link);
    }
    Some(out)
}

/// A body rendered on ONE line — a nested rule's inner chain, which is a chain
/// link's ARGUMENT and never breaks.
fn render_inline_chain(body: &vilan_core::node::CssBody<'_>, source: &str) -> Option<String> {
    let mut out = String::from("style()");
    for item in &body.items {
        out.push_str(&render_chain_link(item, source)?);
    }
    Some(out)
}

fn render_chain_link(item: &CssItem<'_>, source: &str) -> Option<String> {
    match item {
        CssItem::Declaration(declaration) => {
            let property = &source[declaration.property.into_range()];
            let value = render_chain_value(declaration, source)?;
            Some(format!(".raw(\"{property}\", {value})"))
        }
        CssItem::Nested(nested) => {
            let mut arguments: Vec<String> = nested
                .arguments
                .iter()
                .map(|argument| source[argument.1.into_range()].to_string())
                .collect();
            // Inner-last, as the desugar appends it (§5.3).
            arguments.push(render_inline_chain(&nested.body, source)?);
            Some(format!(".{}({})", nested.name.0, arguments.join(", ")))
        }
    }
}

/// A declaration's value as the `raw` argument it lowers to — the three rows of
/// §5.2's table, in the same order the desugar reads them.
fn render_chain_value(declaration: &CssDeclaration<'_>, source: &str) -> Option<String> {
    match declaration.value.as_slice() {
        // Exactly one hole passes its expression through untouched, so the
        // argument IS that expression.
        [CssValuePiece::Hole(_, braces)] => Some(
            source[braces.start + 1..braces.end.saturating_sub(1)]
                .trim()
                .to_string(),
        ),
        [CssValuePiece::Text(text)] => {
            Some(format!("\"{}\"", escape_value(&source[text.into_range()])?))
        }
        // Mixed: the i-string the desugar's concatenation already is — the two
        // build the same tree (§5.2), and the value's own text is an i-string
        // body verbatim, holes included.
        pieces => {
            let mut literal = String::new();
            for piece in pieces {
                match piece {
                    CssValuePiece::Hole(_, braces) => {
                        literal.push_str(&source[braces.into_range()])
                    }
                    CssValuePiece::Text(text) => {
                        literal.push_str(&escape_value(&source[text.into_range()])?)
                    }
                }
            }
            Some(format!("i\"{literal}\""))
        }
    }
}

/// `text` as a vilan string-literal BODY, or `None` when the two spellings would
/// stop meaning the same thing — see [`CssConversion`]'s backslash refusal.
fn escape_value(text: &str) -> Option<String> {
    (!text.contains('\\')).then(|| text.replace('"', "\\\""))
}

/// A `style()` chain as the `css` block it is the lowering of.
fn render_css_block(
    chain: &vilan_core::Spanned<vilan_core::node::Node<'_>>,
    source: &str,
    indent: &str,
) -> Option<String> {
    let items = render_css_items(&style_chain_links(chain)?, source, indent)?;
    Some(format!("css {{\n{items}{indent}}}"))
}

fn render_css_items(
    links: &[&vilan_core::Spanned<vilan_core::node::Node<'_>>],
    source: &str,
    indent: &str,
) -> Option<String> {
    let inner = format!("{indent}\t");
    let mut out = String::new();
    for link in links {
        let Node::Call(callee, None, arguments) = &link.0 else {
            return None;
        };
        let Node::Accessor(name) = callee.0 else {
            return None;
        };
        if name == "raw" {
            let [property, value] = arguments.0.as_slice() else {
                return None;
            };
            let Node::String(property) = property.0 else {
                return None;
            };
            if !is_css_property(property) {
                return None;
            }
            let value = render_block_value(value, source);
            out.push_str(&format!("{inner}{property}: {value};\n"));
        } else if STYLE_CONDITION_METHODS
            .iter()
            .any(|(condition, _)| *condition == name)
        {
            let (nested, head) = arguments.0.split_last()?;
            let body = render_css_items(&style_chain_links(nested)?, source, &inner)?;
            let head = if head.is_empty() {
                String::new()
            } else {
                let written: Vec<String> = head
                    .iter()
                    .map(|argument| source[argument.1.into_range()].to_string())
                    .collect();
                format!("({})", written.join(", "))
            };
            out.push_str(&format!("{inner}.{name}{head} {{\n{body}{inner}}}\n"));
        } else {
            // Not a row of the lowering table: no block spelling exists, and
            // one is not this refactor's to invent.
            return None;
        }
    }
    Some(out)
}

/// A `raw` argument as a declaration's value. A plain token run is written as
/// itself; everything else goes back through a HOLE, which is exact — a value
/// that is exactly one hole passes its expression through untouched.
fn render_block_value(
    value: &vilan_core::Spanned<vilan_core::node::Node<'_>>,
    source: &str,
) -> String {
    if let Node::String(literal) = value.0
        && !literal.is_empty()
        && literal.trim() == literal
        && !literal.contains(['\\', '"', ';', '{', '}'])
    {
        return literal.to_string();
    }
    format!("{{{}}}", &source[value.1.into_range()])
}

/// Whether `name` is spellable as a `css` property: the span-adjacent
/// `name`-`-`-`name` run the grammar reads (`parse_css_property`), custom
/// properties and vendor prefixes included. A `raw` call naming anything else
/// has no block spelling.
fn is_css_property(name: &str) -> bool {
    let body = name.trim_start_matches('-');
    if name.len() - body.len() > 2 || body.is_empty() {
        return false;
    }
    body.split('-').all(|segment| {
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            && !segment.as_bytes()[0].is_ascii_digit()
    })
}

/// The whole `#rrggbb` run the `#` diagnostic at `hash` points at — when it IS
/// one. CSS has exactly four hex-colour lengths (3, 4, 6 and 8 digits), and the
/// run has to END there: `#zzz` and `#333xyz` are not colours, and a fix that
/// rewrote them would be inventing a value the author never wrote. Those keep
/// the rule's explanation and get no edit at all (css-block.md §7.2 fix 1).
fn hex_colour_span(text: &str, hash: usize) -> Option<Span> {
    let rest = text.get(hash..)?.strip_prefix('#')?;
    let digits = rest.bytes().take_while(u8::is_ascii_hexdigit).count();
    let ends = rest
        .as_bytes()
        .get(digits)
        .is_none_or(|byte| !byte.is_ascii_alphanumeric() && *byte != b'_');
    (matches!(digits, 3 | 4 | 6 | 8) && ends).then(|| Span::from(hash..hash + 1 + digits))
}

/// css-block.md §7.2 fix 2: `@media (min-width: 768px) {` → `.md {`.
///
/// The diagnostic is the lexer's and one character wide, so the at-rule's head
/// is read off the text: `@media`, a parenthesized query, and the `{` that opens
/// the rule. The combinator comes from the query's own min-width —
/// [`STYLE_BREAKPOINT_WIDTHS`] where one names it, and the general `.media("…")`
/// the four delegate to where none does, which is exact for any width.
///
/// `None` for every other at-rule. `@supports`, `@font-face` and `@keyframes`
/// have no combinator spelling (§10), so there is nothing to offer and the
/// diagnostic's own sentence is the whole answer.
fn media_rule_fix(text: &str, at: usize) -> Option<QuickFix> {
    let rest = text.get(at..)?.strip_prefix("@media")?;
    let query_start = rest.len() - rest.trim_start().len();
    let query = rest[query_start..].strip_prefix('(')?;
    let query_end = query.find(')')?;
    let (property, width) = query[..query_end].split_once(':')?;
    if property.trim() != "min-width" {
        return None;
    }
    let width = width.trim();
    let after_query = query_start + 1 + query_end + 1;
    let brace = rest[after_query..].len() - rest[after_query..].trim_start().len();
    if !rest[after_query + brace..].starts_with('{') {
        return None;
    }
    let head = match STYLE_BREAKPOINT_WIDTHS
        .iter()
        .find(|(_, breakpoint)| *breakpoint == width)
    {
        Some((name, _)) => format!(".{name}"),
        None => format!(".media(\"{width}\")"),
    };
    // The spelling is built once and quoted into the title, rather than doubling
    // braces inside the `format!` template: `book_sync.rs` reads that template
    // literally to hold the book's editor page in sync, and `{{` is not a hole
    // to it.
    let spelling = format!("{head} {{ … }}");
    Some(QuickFix {
        title: format!("Use `{spelling}`"),
        span: Span::from(at..at + "@media".len() + after_query + brace + 1),
        replacement: format!("{head} {{"),
    })
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
    /// Serializes the pins that read or write the compiler's PROCESS-GLOBAL
    /// base-cache state — its worlds, its M23 overlay claims, its M24 byte
    /// budget (`vilan-core/tests/base_cache.rs` keeps its own `CACHE_LOCK`
    /// for exactly this reason).
    ///
    /// `cargo nextest` gives every test its own process, so under the
    /// project's gate this lock is never contended. Plain `cargo test` runs a
    /// binary's tests as threads in ONE process, and CLAUDE.md records that
    /// as a correct, slower equivalent — which it stops being the moment two
    /// tests clear each other's cache or lower each other's budget. Acquire
    /// it in the test body, before `on_big_stack`: the guard is not `Send`,
    /// and it does not need to be, because that call blocks until its thread
    /// joins.
    pub(crate) static BASE_CACHE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Takes [`BASE_CACHE_LOCK`], recovering from a poisoned one: a pin that
    /// panicked has already reported, and the next pin's own setup (a clear,
    /// a budget reset) is what puts the cache back in a known state.
    pub(crate) fn base_cache_guard() -> std::sync::MutexGuard<'static, ()> {
        BASE_CACHE_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

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

    /// B207 — the shape B198 fixed in the build, unaudited in the editor: the
    /// subject of a containment test here is an OPEN BUFFER, and an open buffer
    /// need not be on disk.
    ///
    /// A project root reached through a symlink is supported layout (`const.md`
    /// §9.2), so `link/pkg/src/untitled.vl` — a file the user created in the
    /// editor and has not saved — is a file inside `pkg/src`. With
    /// `canonical_path` on both sides the buffer degraded to its LEXICAL
    /// spelling (nothing on disk to resolve) while the layer root resolved
    /// through the link, and the containment answered NO: the document lost its
    /// package root, its platform and every `pkg::` import.
    ///
    /// Unix-only for the link; [`is_within_holds_for_an_unsaved_buffer_under_a_real_root`]
    /// is the control that runs everywhere.
    #[cfg(unix)]
    #[test]
    fn is_within_holds_for_an_unsaved_buffer_under_a_symlinked_root() {
        let root = std::env::temp_dir().join(format!("vilan-lsp-b207-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("real/pkg/src")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();

        // The layer root as the manifest scan found it: on disk, real spelling.
        let directory = root.join("real/pkg/src");
        // The open buffer, reached through the link and NOT on disk.
        let unsaved = root.join("link/pkg/src/untitled.vl");
        assert!(!unsaved.exists(), "the pin needs a buffer not on disk");
        assert!(
            is_within(&directory, &unsaved),
            "an unsaved buffer under a symlinked project root is inside its own package"
        );
        // The link's own spelling of the root answers the same, in both
        // directions: one file, two honest ancestries.
        assert!(is_within(&root.join("link/pkg/src"), &unsaved));
        assert!(is_within(
            &root.join("link/pkg/src"),
            &root.join("real/pkg/src/untitled.vl")
        ));
        // And the same buffer is the same file whichever name reached it.
        assert!(same_file(&unsaved, &root.join("real/pkg/src/untitled.vl")));
        // Still a real test: a sibling package does not contain it.
        std::fs::create_dir_all(root.join("real/other/src")).unwrap();
        assert!(!is_within(&root.join("real/other/src"), &unsaved));
        assert!(!is_within(&root.join("link/other/src"), &unsaved));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// The control for [`is_within_holds_for_an_unsaved_buffer_under_a_symlinked_root`]:
    /// no link anywhere, so both sides resolve the same way whatever helper is
    /// used, and the unsaved buffer is inside its package on every platform.
    /// It is what says the symlink pin above is about the LINK and not about
    /// the file being missing.
    #[test]
    fn is_within_holds_for_an_unsaved_buffer_under_a_real_root() {
        let root =
            std::env::temp_dir().join(format!("vilan-lsp-b207-control-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("pkg/src")).unwrap();
        let unsaved = root.join("pkg/src/untitled.vl");
        assert!(!unsaved.exists(), "the pin needs a buffer not on disk");
        assert!(is_within(&root.join("pkg/src"), &unsaved));
        assert!(!is_within(&root.join("pkg/other"), &unsaved));
        let _ = std::fs::remove_dir_all(&root);
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
                "import std::io::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
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

    // E110 (audit run 6, F22): a name the WEB set would have made ambient
    // carries the manifest steer analyzer-side (`prelude = "std::web"`), and
    // the add-import quickfix is offered beside it. `web_prelude_steer`'s
    // comment used to claim the opposite — that the arm's different suffix
    // steered the LSP's `unresolved_name` parser off — which was never true:
    // that parser keys on the `cannot find '` PREFIX, and every name still in
    // the steer's set has a real declaring module (the module-carried entries
    // left it with F2). Two repairs, both of which compile; the pin is what
    // keeps the comment from drifting back.
    #[test]
    fn quickfix_offers_the_add_import_beside_the_web_set_steer() {
        let (dir, document) =
            analyze_workspace(&[("main.vl", "fun main() {\n\tlet _s = Signal;\n}\n")]);
        let steered = document
            .diagnostics
            .iter()
            .find(|error| error.msg.contains("cannot find 'Signal'"))
            .expect("the unresolved name is reported");
        assert!(
            steered.msg.contains("prelude of the web set"),
            "the analyzer's own steer is the premise of this pin: {}",
            steered.msg
        );
        let program = document.program.as_ref().unwrap();
        let text = document.line_index.text();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let fixes = document.quickfixes(program, whole_file);
        let signal_fixes: Vec<_> = fixes
            .iter()
            .filter(|fix| fix.title.contains("`Signal`"))
            .collect();
        assert_eq!(
            signal_fixes.len(),
            1,
            "expected exactly one unambiguous `Signal` fix: {:?}",
            fixes.iter().map(|fix| &fix.title).collect::<Vec<_>>()
        );
        assert_eq!(
            signal_fixes[0].replacement, "import std::reactive::Signal;\n",
            "{}",
            signal_fixes[0].title
        );
        // Applied and re-analyzed: the import is a repair, not a detour — which
        // is the whole reason the quickfix is wanted here rather than silenced.
        let mut applied = text.to_string();
        applied.replace_range(
            signal_fixes[0].span.into_range(),
            &signal_fixes[0].replacement,
        );
        let entry = dir.join("main.vl");
        std::fs::write(&entry, &applied).unwrap();
        let reanalyzed = Document::analyze(&applied, &std_root(), &entry);
        assert!(
            reanalyzed
                .diagnostics
                .iter()
                .all(|error| !error.msg.contains("cannot find 'Signal'")),
            "applying the fix should resolve the name: {:#?}",
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

    // --- css-block S5: the quickfixes of §7.2 --------------------------------

    /// The fixes `quickfixes` offers over a whole `css`-block fixture, as
    /// `(title, replaced text, replacement)` — the shape every §7.2 pin reads.
    fn css_block_fixes(body: &str) -> Vec<(String, String, String)> {
        let source = format!(
            "import std::style::{{ Color, Style, style }};\n\nfun card(): Style {{\n{body}}}\n"
        );
        let (directory, document) = analyze_workspace(&[("main.vl", &source)]);
        let program = document
            .program
            .as_ref()
            .expect("a css fixture still analyzes");
        let text = document.line_index.text().to_string();
        let whole_file = Span {
            start: 0,
            end: text.len(),
        };
        let fixes = document
            .quickfixes(program, whole_file)
            .into_iter()
            .map(|fix| {
                (
                    fix.title,
                    text[fix.span.into_range()].to_string(),
                    fix.replacement,
                )
            })
            .collect();
        let _ = std::fs::remove_dir_all(&directory);
        fixes
    }

    // §7.2 fix 1. `#` cannot lex — lexing is context-free and finishes before
    // the parser exists (§4.1) — so the diagnostic is ONE CHARACTER wide and
    // the fix reads the colour off the text itself. It rewrites the whole run,
    // into the hole spelling the diagnostic already names.
    #[test]
    fn quickfix_wraps_a_hex_colour_as_a_colour_hole() {
        let fixes = css_block_fixes("\tcss {\n\t\tcolor: #336699;\n\t}\n");
        assert!(
            fixes.contains(&(
                "Wrap as `{Color::hex(\"#336699\")}`".to_string(),
                "#336699".to_string(),
                "{Color::hex(\"#336699\")}".to_string(),
            )),
            "{fixes:?}"
        );
        // The three-digit form too, and the four hex lengths are the whole
        // gate: a `#` that is not a colour keeps the explanation and gets no
        // edit, because there is nothing to wrap.
        let short = css_block_fixes("\tcss {\n\t\tcolor: #333;\n\t}\n");
        assert!(
            short
                .iter()
                .any(|(title, _, _)| title == "Wrap as `{Color::hex(\"#333\")}`"),
            "{short:?}"
        );
        for stray in ["#zzz", "#33669", "#333ing"] {
            let not_a_colour = css_block_fixes(&format!("\tcss {{\n\t\tcolor: {stray};\n\t}}\n"));
            assert!(
                !not_a_colour
                    .iter()
                    .any(|(title, _, _)| title.starts_with("Wrap as")),
                "`{stray}` is not a colour, so it gets the rule and no edit: {not_a_colour:?}"
            );
        }
    }

    // §7.2 fix 2. `@` is the `#`'s twin, refused for the same context-free
    // reason, and the fix is the breakpoint combinator the block spells media
    // queries with. Which combinator comes from the query's OWN min-width,
    // matched against `STYLE_BREAKPOINT_WIDTHS` — held to `style.vl`'s method
    // bodies by `style_table_sync.rs`, so the map cannot drift from std.
    #[test]
    fn quickfix_rewrites_an_at_media_rule_as_its_breakpoint() {
        let fixes = css_block_fixes(
            "\tcss {\n\t\t@media (min-width: 768px) {\n\t\t\tdisplay: flex;\n\t\t}\n\t}\n",
        );
        assert!(
            fixes.contains(&(
                "Use `.md { … }`".to_string(),
                "@media (min-width: 768px) {".to_string(),
                ".md {".to_string(),
            )),
            "{fixes:?}"
        );
        // A width no breakpoint names still has an exact spelling — the
        // general combinator the four delegate to — rather than no fix.
        let arbitrary = css_block_fixes(
            "\tcss {\n\t\t@media (min-width: 900px) {\n\t\t\tdisplay: flex;\n\t\t}\n\t}\n",
        );
        assert!(
            arbitrary.contains(&(
                "Use `.media(\"900px\") { … }`".to_string(),
                "@media (min-width: 900px) {".to_string(),
                ".media(\"900px\") {".to_string(),
            )),
            "{arbitrary:?}"
        );
        // An at-rule that is not a min-width media query has no combinator
        // spelling at all (§10), so it gets the rule and no edit.
        let keyframes =
            css_block_fixes("\tcss {\n\t\t@keyframes spin {\n\t\t\tdisplay: flex;\n\t\t}\n\t}\n");
        assert!(
            !keyframes
                .iter()
                .any(|(title, _, _)| title.starts_with("Use `.")),
            "{keyframes:?}"
        );
    }

    // §7.2 fix 3. The parser excises `!important` from the value and reports
    // at exactly its span; the fix removes it — and takes the space in front
    // of it, so `flex !important;` becomes `flex;` rather than `flex ;`.
    #[test]
    fn quickfix_removes_an_important_marker() {
        let fixes = css_block_fixes("\tcss {\n\t\tdisplay: flex !important;\n\t}\n");
        assert!(
            fixes.contains(&(
                "Remove `!important`".to_string(),
                " !important".to_string(),
                String::new(),
            )),
            "{fixes:?}"
        );
    }

    // §7.2 fix 5, "the existing quickfix, unchanged" — asserted rather than
    // assumed. A declaration's missing `;` reports as the ordinary
    // missing-terminator diagnostic (`parse_css_declaration` raises
    // `TERMINATOR_EXPECTED`, which is gap-anchored like any other), so E61's
    // insertion fires inside a block with no css-side code at all.
    #[test]
    fn quickfix_inserts_a_missing_semicolon_in_a_css_block() {
        let fixes = css_block_fixes("\tcss {\n\t\tdisplay: flex\n\t}\n");
        assert!(
            fixes.contains(&("Insert `;`".to_string(), String::new(), ";".to_string())),
            "{fixes:?}"
        );
    }

    // --- css-block S5: the convert-between-spellings refactor (§7.2) --------

    /// The conversion offered at the `~` cursor in a `css`-block fixture, as
    /// `(to_chain, replaced text, replacement)`.
    fn css_conversion(body: &str) -> Option<(bool, String, String)> {
        let source = format!(
            "import std::style::{{ Color, Length, Style, space, style }};\n\nfun card(): Style {{\n{body}}}\n"
        );
        let offset = source.find('~').expect("fixture needs a `~` cursor");
        let text = source.replace('~', "");
        let (directory, document) = analyze_workspace(&[("main.vl", &text)]);
        let conversion = document
            .css_spelling_conversion(Span {
                start: offset,
                end: offset,
            })
            .map(|conversion| {
                (
                    conversion.to_chain,
                    text[conversion.span.into_range()].to_string(),
                    conversion.replacement,
                )
            });
        let _ = std::fs::remove_dir_all(&directory);
        conversion
    }

    // The refactor's forward direction. The lowering is total and one-to-one
    // (§5.2), so the chain it prints is the chain the block already lowers to:
    // a declaration is a `.raw` link, a nested rule is a combinator link with
    // the inner chain as its final argument, and there is no third row.
    #[test]
    fn refactor_converts_a_css_block_to_a_style_chain() {
        let conversion = css_conversion(
            "\tcss {\n\t\tdis~play: flex;\n\t\tgap: {space(4)};\n\t\ttransition-duration: {150}ms;\n\t\t.md {\n\t\t\tpadding: {space(6)};\n\t\t}\n\t}\n",
        )
        .expect("a block converts");
        assert!(conversion.0, "block -> chain");
        assert!(conversion.1.starts_with("css {"), "{conversion:?}");
        assert_eq!(
            conversion.2,
            "style()\n\t\t.raw(\"display\", \"flex\")\n\t\t.raw(\"gap\", space(4))\n\t\t.raw(\"transition-duration\", i\"{150}ms\")\n\t\t.md(style().raw(\"padding\", space(6)))",
            "{conversion:?}"
        );
    }

    // A one-declaration block converts on ONE line, the shape S3 gives the
    // outer body itself: `const css { padding: {space(6)}; }` is written
    // inline, and its chain is too.
    #[test]
    fn refactor_converts_a_single_declaration_block_inline() {
        let conversion =
            css_conversion("\tconst css { pad~ding: {space(6)}; }\n").expect("a block converts");
        assert_eq!(conversion.2, "style().raw(\"padding\", space(6))");
    }

    // The inverse. Only the two rows of the lowering table have a block
    // spelling, and a value that is not a plain token run goes back through a
    // HOLE — which is exact, since a value that is exactly one hole passes its
    // expression through untouched.
    #[test]
    fn refactor_converts_a_style_chain_to_a_css_block() {
        let conversion = css_conversion(
            "\tsty~le()\n\t\t.raw(\"display\", \"flex\")\n\t\t.raw(\"gap\", space(4))\n\t\t.md(style().raw(\"padding\", space(6)))\n",
        )
        .expect("a chain converts");
        assert!(!conversion.0, "chain -> block");
        assert!(conversion.1.starts_with("style()"), "{conversion:?}");
        assert_eq!(
            conversion.2,
            "css {\n\t\tdisplay: flex;\n\t\tgap: {space(4)};\n\t\t.md {\n\t\t\tpadding: {space(6)};\n\t\t}\n\t}",
            "{conversion:?}"
        );
    }

    // The inverse is PARTIAL, and says so by not being offered. A typed
    // property method is `with_length("padding", …)`, which is not the node
    // `padding: {space(4)};` lowers to — so a chain carrying one has no block
    // spelling this refactor is entitled to invent.
    #[test]
    fn refactor_declines_a_chain_with_a_typed_property_link() {
        assert_eq!(
            css_conversion("\tsty~le()\n\t\t.padding(space(4))\n\t\t.raw(\"display\", \"flex\")\n"),
            None
        );
        // `class_list` ends the chain in something that is not a `Style` at
        // all — likewise not convertible.
        assert_eq!(
            css_conversion("\tsty~le()\n\t\t.raw(\"display\", \"flex\")\n\t\t.class_list()\n"),
            None
        );
    }

    // The two directions are INVERSES, which is the claim a migration path has
    // to make: converting a block and converting the result straight back gives
    // the block again, byte for byte, nesting included.
    #[test]
    fn the_two_conversions_round_trip() {
        let block = "css {\n\t\tdisplay: flex;\n\t\tgap: {space(4)};\n\t\t.md {\n\t\t\tpadding: {space(6)};\n\t\t}\n\t}";
        let to_chain = css_conversion(&format!("\t{}\n", block.replacen("display", "dis~play", 1)))
            .expect("a block converts");
        assert!(to_chain.0, "block -> chain");
        let back = css_conversion(&format!(
            "\t{}\n",
            to_chain.2.replacen("style", "sty~le", 1)
        ))
        .expect("the chain converts back");
        assert!(!back.0, "chain -> block");
        assert_eq!(back.2, block, "the round trip is the identity");
    }

    // Two refusals, both about meaning rather than shape. A comment's
    // attachment is not recoverable across the reshape (the S3 printer refuses
    // to reorder a commented block for the same reason), and a value carrying a
    // BACKSLASH means different things in the two spellings — a chain's string
    // literal has its escapes processed at emission and a block's token run
    // does not.
    #[test]
    fn refactor_declines_where_the_two_spellings_would_differ() {
        assert_eq!(
            css_conversion("\tcss {\n\t\t// keep me\n\t\tdis~play: flex;\n\t}\n"),
            None
        );
        assert_eq!(
            css_conversion("\tcss {\n\t\tcon~tent: \"\\201C\";\n\t}\n"),
            None
        );
        // A quoted value is fine, though: escaping a `\"` into the chain's
        // string literal round-trips exactly.
        let quoted = css_conversion("\tcss {\n\t\tbackground-im~age: url(\"tile.png\");\n\t}\n")
            .expect("a quoted value converts");
        assert_eq!(
            quoted.2,
            "style().raw(\"background-image\", \"url(\\\"tile.png\\\")\")"
        );
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

    // E83, then M29: NO whole-buffer parse in a completion request at all,
    // however many auto-import candidates the request shapes.
    //
    // `insert_import`'s string-input form re-parses the buffer per call, and
    // calling it once per surviving candidate (up to
    // `AUTO_IMPORT_COMPLETION_CAP`) is what made a bare scope position cost
    // ~20 member completions (playground-completion.md §9); E83's shared
    // `formatter::ParsedSource` brought that to ONE parse per request. M29
    // moved that one onto the analysis: the edits are computed against the
    // ANALYZED text when the completion index is built and re-mapped through
    // the edit anchor when a request serves them, so the request parses
    // nothing. `BUFFER_PARSES` is thread-local and the index is built on the
    // analysis thread, so what this counts is exactly the request's own
    // parses. The pin holds the count, not the time.
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
            parses, 0,
            "a completion request parses the buffer not at all: the import edits come \
             off the analysis (M29), and before it they cost one parse per request \
             (E83) and one per candidate before that"
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
    // slice the analyzed text, no read) — but the PRELUDE now puts std's base
    // seven in scope too, and `print` is a Function candidate whose docs come
    // from `std/src/io.vl`. So the expected count is two modules, one read
    // each, which is the property under test; if this goes red after a std
    // reshuffle, first check what the entry scope now resolves docs from.
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
            reads, 2,
            "each module's text is read ONCE per request: math.vl for the imported \
             candidates, io.vl for the prelude's `print`"
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
                "import std::io::print;\nimport pkg::alpha::{ A };\nimport pkg::zeta::{ Z };\n\
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
        let module = "import std::io::print;\nimport std::drop::Drop;\n\
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
                "import std::io::print;\nimport pkg::page::{ Widget, Opaque };\n\
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
        let entry = "import std::io::print;\nlet A: i32 = B + 1;\nlet B: i32 = A + 2;\n\
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
                "import std::io::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
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
        let reach = "import std::io::print;\nimport pkg::store::load;\n\nfun main() {\n\tif load() { print(\"?\") }\n}\n";
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
        let entry = "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n";
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
        let entry = "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n";
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

    // ── E113: a module's color is the entry that REACHES it ──────────────────
    //
    // §4.2 left a non-entry file to inference "because a module has no `main`
    // and thus no admission walk". The platform decides more than admission: it
    // picks `std`'s layer overlay, so it decides what the file's types ARE.
    // Under the process overlay a browser module's `View` is
    // `{ tag, attributes, children, text }` and `self.element` is a field that
    // does not exist — the owner's kolt report, where every browser-only module
    // showed "struct 'View' has no field 'element'" in the editor while
    // `vilan build` was clean. Reachability answers it, exactly as the build
    // does, and `platform_color::file_platforms` is the one place both the
    // editor and `vilan check <file>` ask.

    /// The kolt shape: a browser `client`, a node `server`, `default-entry` on
    /// the node side, plus the caller's modules. The FIRST file is the one
    /// `analyze_workspace` opens.
    fn fullstack_package(default_entry: &str) -> String {
        format!(
            "[package]\nname = \"app\"\ndefault-entry = \"{default_entry}\"\n\n\
             [entry.client]\ntarget = \"browser\"\n\n[entry.server]\n"
        )
    }

    /// A module using the BROWSER `View`'s `element` field: clean under
    /// `browser`, "no field 'element'" under any process target.
    const BROWSER_ONLY_MODULE: &str = "import std::ui::{ View, view };\n\n\
         fun attach(): View {\n\tlet root = view(\"div\");\n\t\
         root.element.set_attribute(\"id\", \"app\");\n\troot\n}\n";

    /// The mirror: the PROCESS `View`'s `tag`. Clean under node, red under
    /// `browser`.
    const PROCESS_ONLY_MODULE: &str = "import std::ui::{ View, view };\n\n\
         fun markup(): str {\n\tlet root = view(\"div\");\n\troot.tag\n}\n";

    #[test]
    fn a_browser_only_module_analyzes_under_the_entry_that_reaches_it() {
        // E113 in the editor: `widget.vl` is reached only from the browser
        // entry, and `default-entry` names the node one.
        let (dir, widget) = analyze_workspace(&[
            ("src/widget.vl", BROWSER_ONLY_MODULE),
            ("vilan.toml", &fullstack_package("server")),
            (
                "src/client.vl",
                "import pkg::widget::attach;\n\nfun main() {\n\tattach();\n}\n",
            ),
            (
                "src/server.vl",
                "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\n",
            ),
        ]);
        assert!(
            widget.published_diagnostics().is_empty(),
            "a module only the browser entry reaches analyzes as browser: {:?}",
            messages(&widget.published_diagnostics())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_node_only_module_analyzes_under_the_entry_that_reaches_it() {
        // The mirror-image control: reached only from the NODE entry while
        // `default-entry` names the browser one. The answer is reachability,
        // not a browser preference — a "prefer browser" fix would redden this.
        let (dir, store) = analyze_workspace(&[
            ("src/store.vl", PROCESS_ONLY_MODULE),
            ("vilan.toml", &fullstack_package("client")),
            (
                "src/client.vl",
                "import std::io::print;\n\nfun main() {\n\tprint(\"client\");\n}\n",
            ),
            (
                "src/server.vl",
                "import std::io::print;\nimport pkg::store::markup;\n\n\
                 fun main() {\n\tprint(markup());\n}\n",
            ),
        ]);
        assert!(
            store.published_diagnostics().is_empty(),
            "a module only the node entry reaches analyzes as node: {:?}",
            messages(&store.published_diagnostics())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_shared_module_publishes_every_reaching_legs_diagnostics() {
        // A module BOTH entries reach is compiled once per leg and must
        // type-check under each, so the editor reports the union the build
        // would. The mistake here is one only the BROWSER leg can see, in a
        // package whose `default-entry` is the node one — so a single-color
        // answer would have missed it whichever color it picked first.
        let reach = "import pkg::shared::labelled;\n\nfun main() {\n\tlabelled(\"app\");\n}\n";
        let (dir, shared) = analyze_workspace(&[
            (
                "src/shared.vl",
                "import std::ui::{ View, view };\n\n\
                 fun labelled(text: str): str {\n\tlet root = view(text);\n\troot.tag\n}\n",
            ),
            ("vilan.toml", &fullstack_package("server")),
            ("src/client.vl", reach),
            ("src/server.vl", reach),
        ]);
        let published = shared.published_diagnostics();
        assert!(
            published
                .iter()
                .any(|item| item.message.contains("has no field 'tag'")),
            "the browser leg's verdict reaches the editor too: {:?}",
            messages(&shared.published_diagnostics())
        );
        // One squiggle per mistake: the legs agree about everything else in the
        // file, and a duplicate would be published twice at the same span.
        assert_eq!(
            published
                .iter()
                .filter(|item| item.message.contains("has no field 'tag'"))
                .count(),
            1,
            "deduplicated across legs: {:?}",
            messages(&shared.published_diagnostics())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unreached_module_analyzes_under_the_default_entry() {
        // No entry loads it — a module in progress, or one whose importer was
        // just deleted. The designated `default-entry` answers, and moving the
        // designation moves the color.
        let entry = "import std::io::print;\n\nfun main() {\n\tprint(\"hi\");\n}\n";
        let (dir, orphan) = analyze_workspace(&[
            ("src/orphan.vl", BROWSER_ONLY_MODULE),
            ("vilan.toml", &fullstack_package("client")),
            ("src/client.vl", entry),
            ("src/server.vl", entry),
        ]);
        assert!(
            orphan.published_diagnostics().is_empty(),
            "`default-entry = \"client\"` colors it browser: {:?}",
            messages(&orphan.published_diagnostics())
        );
        std::fs::write(dir.join("vilan.toml"), fullstack_package("server")).unwrap();
        let path = dir.join("src/orphan.vl");
        let text = std::fs::read_to_string(&path).unwrap();
        let orphan = Document::analyze(&text, &std_root(), &path);
        assert!(
            orphan
                .published_diagnostics()
                .iter()
                .any(|item| item.message.contains("has no field 'element'")),
            "and moving the designation moves the color: {:?}",
            messages(&orphan.published_diagnostics())
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

    // --- E128: `Self` in a TRAIT declaration renders as `Self` ---------------
    //
    // `declaration_labels` called `function_signature_label` with no impl, and a
    // `= Self`-defaulted trait generic resolves to the very same type as `Self`
    // (`trait Add<B = Self>` — both are `Type::Trait(Add, [])`), so hover on
    // `Add::add` printed the TRAIT's name: `fun add(self, b: Add): Add`, a
    // signature nobody can write. B206 fixed exactly this for the conformance
    // steer, by rendering FOR the impl; hover has no impl in hand and does not
    // want one — it is showing the DECLARATION.
    //
    // Ruled: render the literal `Self`. It is what the trait's author wrote, it
    // is the only spelling a reader can write back, and a `= Self` parameter
    // shows its default, which is the whole of what the shorthand says.

    #[test]
    fn e128_hover_on_a_trait_member_renders_self_and_not_the_traits_name() {
        // The item's own exhibit: `Add::add`, reached from a sub-trait's default
        // body. Both positions — the `= Self`-defaulted `b` and the `Self`
        // return — printed `Add` before.
        let hover = hover_at_cursor(
            "import std::operators::Add;\n\ntrait Doubler with Add {\n\tfun twice(self): Self {\n\t\tself.a|dd(self)\n\t}\n}\n\nfun main() {}\n\nmain();\n",
        )
        .expect("hover on `add` should produce a label");
        assert!(
            hover.contains("```vilan\nfun add(self, b: Self): Self\n```"),
            "{hover}"
        );
    }

    #[test]
    fn e128_hover_through_a_generic_bound_renders_self_too() {
        // The other route to a trait's own declaration: a call dispatched
        // through a bound. Same label, because it is the same declaration.
        let hover = hover_at_cursor(
            "import std::io::print;\nimport std::operators::Sub;\n\nfun gap<T: Sub>(a: T, b: T): T {\n\ta.s|ub(b)\n}\n\nfun main() {\n\tprint(gap(3, 2));\n}\n\nmain();\n",
        )
        .expect("hover on `sub` should produce a label");
        assert!(
            hover.contains("```vilan\nfun sub(self, b: Self): Self\n```"),
            "{hover}"
        );
    }

    #[test]
    fn e128_hover_on_a_user_traits_defaulted_parameter_renders_self() {
        // A user trait, so the rule is not std's — hovered on the declaration
        // itself, where a reader is most likely to ask.
        let hover = hover_at_cursor(
            "trait Adder<B = Self> {\n\tfun pl|us(self, b: B): Self;\n}\n\nfun main() {}\n\nmain();\n",
        )
        .expect("hover on `plus` should produce a label");
        assert!(
            hover.contains("```vilan\nfun plus(self, b: Self): Self\n```"),
            "{hover}"
        );
    }

    #[test]
    fn e128_hover_renders_self_in_a_partial_eq_shaped_declaration() {
        // `PartialEq`'s shape — a `= Self` parameter under a `bool` return — so
        // the rule is pinned on a position whose SIBLING is not ambiguous: only
        // the parameter moves, and `bool` is still `bool`.
        let hover = hover_at_cursor(
            "trait Same<B = Self> {\n\tfun al|ike(self, other: B): bool;\n}\n\nfun main() {}\n\nmain();\n",
        )
        .expect("hover on `alike` should produce a label");
        assert!(
            hover.contains("```vilan\nfun alike(self, other: Self): bool\n```"),
            "{hover}"
        );
    }

    #[test]
    fn e128_a_trait_members_ordinary_parameter_still_renders_as_written() {
        // The control that keeps the rule narrow: only a position resolving to
        // the DECLARING trait's own abstract type is rewritten. A concrete
        // parameter and a mention of another trait's name are untouched.
        let hover = hover_at_cursor(
            "trait Labelled<B = Self> {\n\tfun la|bel(self, times: i32, other: B): str;\n}\n\nfun main() {}\n\nmain();\n",
        )
        .expect("hover on `label` should produce a label");
        assert!(
            hover.contains("```vilan\nfun label(self, times: i32, other: Self): str\n```"),
            "{hover}"
        );
    }

    #[test]
    fn e128_hover_on_an_impls_method_still_renders_the_impls_own_types() {
        // The other control: an IMPL is not a declaration, and its signature was
        // never ambiguous — it says the concrete type on both sides.
        let hover = hover_at_cursor(
            "import std::io::print;\nimport std::operators::PartialEq;\n\nstruct Tag { n: i32 }\nimpl Tag with PartialEq {\n\tfun e|q(self, other: Tag): bool { self.n == other.n }\n}\n\nfun main() {\n\tprint(Tag { n = 1 }.eq(Tag { n = 1 }));\n}\n\nmain();\n",
        )
        .expect("hover on the impl's `eq` should produce a label");
        assert!(
            hover.contains("```vilan\nfun eq(self, other: Tag): bool\n```"),
            "{hover}"
        );
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
        // E115: the angle brackets themselves paint too, as the tag they
        // belong to — the open `<`, the head's `>`, a `/>`, and the close
        // tag's `</` and `>`.
        assert_eq!(kind_of("<", 0), Some(TokenKind::Tag), "{tokens:?}");
        assert_eq!(kind_of(">", 0), Some(TokenKind::Tag), "{tokens:?}");
        assert_eq!(kind_of("/>", 0), Some(TokenKind::Tag), "{tokens:?}");
        assert_eq!(kind_of("</", 0), Some(TokenKind::Tag), "{tokens:?}");
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

    // E115: the owner's report — a head whose attributes span lines, with the
    // closing `>` on a line of its own, loses that bracket's highlight. The
    // rule this pins is that the SHAPE of the head cannot change the tokens:
    // the same head written one-line and multi-line paints the same things, in
    // the same order, with the same kinds. That is a property only a
    // parse-driven source can have — a TextMate rule is matched one line at a
    // time, so the `>` is out of its reach the moment it leaves the tag's line.
    #[test]
    fn a_multi_line_element_head_paints_what_a_one_line_head_paints() {
        let prelude = "import std::ui::{ view, View };\n\nfun page(): View {\n";
        let one_line =
            format!("{prelude}\t<div aria-label(\"x\") on:click(handle)>\"hi\"</div>\n}}\n");
        let multi_line = format!(
            "{prelude}\t<div\n\t\taria-label(\"x\")\n\t\ton:click(handle)\n\t>\"hi\"</div>\n}}\n"
        );
        // Each token as (the source text it covers, its kind) — the shape a
        // reader sees painted, independent of where the bytes landed.
        let painted = |text: &str| -> Vec<(String, TokenKind)> {
            let document = Document::analyze(text, &std_root(), Path::new("test.vl"));
            document
                .semantic_tokens()
                .into_iter()
                .map(|(span, kind, _)| (text[span.into_range()].to_string(), kind))
                .collect()
        };
        let flat = painted(&one_line);
        assert_eq!(
            flat,
            painted(&multi_line),
            "the head's shape must not change what is painted",
        );
        // …and the terminator is in there, painted as its tag rather than left
        // to fall through to the operator list as a comparison.
        assert!(
            flat.contains(&(">".to_string(), TokenKind::Tag)),
            "the head's closing `>` is painted: {flat:?}",
        );
    }

    #[test]
    fn semantic_tokens_paint_a_css_block() {
        // css-block S5: a property name paints as Property and a condition
        // head as Method, both from the RAW parse — the desugar retires
        // `Node::Css` before analysis and every accessor it generates is
        // zero-width (S2, cut for this slice), so the analyzed program carries
        // no token for either. The one accessor that keeps a REAL span — the
        // outer `style()`, at the `css` keyword, so the missing-import note can
        // underline the word that asked for a `Style` — is suppressed here,
        // exactly as `<div`'s Function token is.
        let text = "import std::style::{ Color, Style, space, style };\n\nfun card(): Style {\n\tcss {\n\t\tdisplay: flex;\n\t\tflex-direction: column;\n\t\tgap: {space(4)};\n\t\t--brand-ink: {Color::gray(900)};\n\t\t.md {\n\t\t\tcolor: {Color::gray(50)};\n\t\t}\n\t}\n}\n";
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
        assert_eq!(
            kind_of("display", 0),
            Some(TokenKind::Property),
            "{tokens:?}"
        );
        // A hyphenated property is several tokens the parser joins by span
        // adjacency — the whole run paints as one Property.
        assert_eq!(
            kind_of("flex-direction", 0),
            Some(TokenKind::Property),
            "{tokens:?}"
        );
        assert_eq!(kind_of("gap", 0), Some(TokenKind::Property), "{tokens:?}");
        // A custom property is the same span-adjacency run, leading `--`
        // included.
        assert_eq!(
            kind_of("--brand-ink", 0),
            Some(TokenKind::Property),
            "{tokens:?}"
        );
        // A NESTED declaration too — the walk descends the whole body.
        assert_eq!(kind_of("color", 0), Some(TokenKind::Property), "{tokens:?}");
        // A condition head is the name without its dot, painted as the method
        // it lowers to (TextMate's `entity.name.function` approximation, made
        // precise).
        assert_eq!(kind_of("md", 0), Some(TokenKind::Method), "{tokens:?}");
        // The `style()` scaffolding token at the keyword is suppressed, so
        // TextMate's `keyword.other.vilan` keeps the word.
        assert_eq!(kind_of("css", 0), None, "{tokens:?}");
        // A hole is ordinary vilan and keeps its ordinary tokens.
        assert_eq!(kind_of("space", 1), Some(TokenKind::Function), "{tokens:?}");
        // The invariant the sweep guarantees, re-checked over a block.
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
            "import std::io::print;\n\nfun descr|ibe(count: i32, label: str): str {\n\tlabel\n}\n\nfun main() {\n\tprint(describe(1, \"x\"));\n}\n",
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
            "import std::io::print;\n\nstruct Wrapper { value: i32 }\n\nimpl Wrapper {\n\tfun sl|ot(&mut self): &mut i32 {\n\t\t&mut self.value\n\t}\n}\n\nfun main() {\n\tmut w = Wrapper { value = 1 };\n\tw.slot() = 2;\n\tprint(w.value);\n}\n",
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
            "import std::io::print;\n\n/// Renders the badge label.\n/// Two lines of docs.\n[must_use]\nfun bad|ge(count: i32): str {\n\t\"b\"\n}\n\nfun main() {\n\tlet _b = badge(1);\n\tprint(\"x\");\n}\n",
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

    // E133: a caret at the very END of a name hovers, the same convention
    // rename and find-references answer by. Hover going blank at `count|`
    // while rename works there is the two gates disagreeing about one question
    // the user reads as one feature — is the cursor on this word — so
    // `offset_touches_a_token` counts a token's end as touching it. Trivia is
    // untouched: the pin's last case is a caret inside whitespace, which still
    // hovers nothing.
    #[test]
    fn hover_at_the_end_of_a_name_answers_the_same_as_inside_it() {
        let inside = hover_at_cursor("fun main() {\n\tlet cou|nt = 5;\n\tlet _ = count;\n}\n")
            .expect("hover inside the name");
        assert_eq!(
            hover_at_cursor("fun main() {\n\tlet count| = 5;\n\tlet _ = count;\n}\n"),
            Some(inside),
            "the caret at `count|` is on `count`",
        );
        // A module binding and a function name, the other two shapes a rename
        // is started from at `name|`.
        assert_eq!(
            hover_at_cursor("let capacity| = 100;\n\nfun main() {\n\tlet _ = capacity;\n}\n"),
            hover_at_cursor("let cap|acity = 100;\n\nfun main() {\n\tlet _ = capacity;\n}\n"),
        );
        assert_eq!(
            hover_at_cursor("fun helper|(value: i32): i32 {\n\tvalue + 1\n}\n"),
            hover_at_cursor("fun hel|per(value: i32): i32 {\n\tvalue + 1\n}\n"),
        );
        assert_eq!(
            hover_at_cursor("fun main() {\n\tlet count = 5;\n\t | \n\tlet _ = count;\n}\n"),
            None,
            "a caret in whitespace still touches no token",
        );
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
            "import std::io::print;\n\n[Hash|]\nstruct Point { x: i32 }\n\nfun main() {\n\tprint(1);\n}\n",
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
            "import std::io::print;\n\n[derive(Pa|)]\nstruct Point { x: i32 }\n\nfun main() {\n\tprint(1);\n}\n",
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
        let text = "import std::io::print;\n\nfun main() {\n\tlet fixed = 1;\n\tmut counter = 2;\n\tprint(fixed + counter);\n}\n";
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
            "import std::io::print;\n\nlet SIZE = const 8 * 8;\n\nfun main() {\n\tprint(SI|ZE);\n}\n",
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
            "import std::io::print;\n\nlet BANNER = const \"ab{}\";\n\nfun main() {{\n\tprint(BAN|NER);\n}}\n",
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
            !program.expr_types.contains_key(binding),
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
            !program.expr_types.contains_key(&id),
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
            "import std::io::print;\n\n// An internal note, not docs.\nfun bad|ge(count: i32): str {\n\t\"b\"\n}\n\nfun main() {\n\tlet _b = badge(1);\n\tprint(\"x\");\n}\n",
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
            "import std::io::print;\n\nstruct Point { x: i32, name: str }\n\nfun main() {\n\tlet p = Po|int { x = 1, name = \"a\" };\n\tprint(p.name);\n}\n",
        )
        .expect("hover on the constructor");
        assert!(
            hover.contains("```vilan\nstruct Point {\n\tx: i32,\n\tname: str,\n}\n```"),
            "{hover}"
        );
        let hover = hover_at_cursor(
            "import std::io::print;\n\nenum Shape {\n\tDot,\n\tBox2(i32, i32),\n}\n\nfun main() {\n\tlet s = Sha|pe::Dot;\n\tmatch s {\n\t\tShape::Dot => print(\"dot\"),\n\t\tShape::Box2(let _w, let _h) => print(\"box\"),\n\t}\n}\n",
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
            "import std::io::print;\n\nfun greet() {\n\tprint(\"hi\");\n}\n\nfun main() {\n\tgre|et();\n}\n",
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
            "import std::io::print;\n\
             import std::reactive::{ Signal, SignalCell };\n\
             import std::result::Result;\n\
             import std::ui::view;\n\
             struct Note { id: i32, text: str }\n\
             struct NotesClient { }\n\
             impl NotesClient {\n\
             \tfun add(self, name: str): Result<Note, str> { Result::Ok(Note { id = 1, text = name }) }\n\
             }\n\
             fun app(client: NotesClient, note_name: SignalCell<str>) {\n\
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
    const ELEMENT_HEAD_PRELUDE: &str = "import std::ui::view;\nimport std::reactive::{ Signal, SignalCell };\nimport std::io::print;\n";

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

    // --- css-block S5: the four positions of a `css` body (§7.1) ------------

    /// The prelude the `css`-block pins share.
    const CSS_BLOCK_PRELUDE: &str =
        "import std::style::{ Color, Length, Style, space, style };\nimport std::io::print;\n";

    fn css_block_completions(body: &str) -> Vec<String> {
        completions_at_marker(
            &format!("{CSS_BLOCK_PRELUDE}fun main() {{\n{body}}}\n"),
            '~',
        )
    }

    // §7.1 row 1: property position offers CSS PROPERTY NAMES, and they are not
    // invented — every one is a slot some `Style` method writes, read from
    // `STYLE_PROPERTY_METHODS`'s `properties` column, which
    // `style_table_sync.rs` already holds to the method bodies. What the
    // position stops offering is the enclosing scope, exactly as an element
    // head does: a value here is source text, not an expression.
    #[test]
    fn css_property_position_offers_the_property_names() {
        let labels = css_block_completions("\tlet card = css {\n\t\tdisp~\n\t};\n");
        for property in [
            "display",
            "flex-direction",
            "background-color",
            "padding-left",
            "border-radius",
        ] {
            assert!(
                labels.contains(&property.to_string()),
                "`{property}` is a slot a `Style` method writes: {labels:?}"
            );
        }
        // The METHOD spelling is not a property name — the block writes CSS.
        assert!(
            !labels.contains(&"flex_direction".to_string()),
            "the method spelling has no place in a block: {labels:?}"
        );
        // Several methods write `padding-left` (`padding_x` and `padding_left`
        // both do), and a property is offered once.
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), labels.len(), "{labels:?}");
        // Nothing merely in scope, and no construct snippets.
        for wrong in ["card", "str", "space", "fun", "for … in { }", "print"] {
            assert!(
                !labels.contains(&wrong.to_string()),
                "`{wrong}` may not appear in a css body: {labels:?}"
            );
        }
    }

    // §7.1 row 3: the dotted head offers the condition combinators, from
    // `STYLE_CONDITION_METHODS` — the dot is the grammar's whole
    // disambiguator, so a dotted item is a combinator and never a property.
    #[test]
    fn css_dotted_head_offers_the_condition_combinators() {
        let labels = css_block_completions("\tlet card = css {\n\t\t.~\n\t};\n");
        for condition in ["md", "hover", "within", "children", "attribute", "pseudo"] {
            assert!(
                labels.contains(&condition.to_string()),
                "`{condition}` is a condition combinator: {labels:?}"
            );
        }
        assert!(
            !labels.iter().any(|label| label.starts_with('.')),
            "the dot is already typed: {labels:?}"
        );
        assert!(
            !labels.contains(&"display".to_string()),
            "a dotted item is never a property: {labels:?}"
        );
        // Mid-word offers the same list; the editor filters by the prefix.
        let mid_word = css_block_completions("\tlet card = css {\n\t\t.ho~\n\t};\n");
        assert!(
            mid_word.contains(&"hover".to_string()),
            "mid-word: {mid_word:?}"
        );
    }

    // §7.1 row 4: a hole is an ordinary expression, and completes as one —
    // "unchanged", which is what makes typed values reachable at all.
    #[test]
    fn css_hole_is_ordinary_expression_ground() {
        let labels = css_block_completions(
            "\tlet ink = Color::gray(900);\n\tlet card = css {\n\t\tcolor: {i~};\n\t};\n",
        );
        assert!(
            labels.contains(&"ink".to_string()) && labels.contains(&"space".to_string()),
            "the names in scope: {labels:?}"
        );
        assert!(
            !labels.contains(&"display".to_string()),
            "not the property vocabulary: {labels:?}"
        );
    }

    // Value position offers NOTHING in v1 (§7.1's closing paragraph, Q4):
    // `flex` after `display:` needs a property->enum map that does not exist,
    // and inventing one is the second source of truth E67 refused. Offering
    // the scope instead would be worse than offering nothing — a binding name
    // in value position is emitted as literal text.
    #[test]
    fn css_value_position_offers_nothing() {
        let labels = css_block_completions("\tlet card = css {\n\t\tdisplay: ~\n\t};\n");
        assert!(
            labels.is_empty(),
            "value position is empty in v1: {labels:?}"
        );
        // Mid-value, after a hole: the hole's own `}` must not be read as a
        // nested rule's, or the rest of the value reads as property position.
        let after_hole =
            css_block_completions("\tlet card = css {\n\t\tpadding: {space(4)} ~;\n\t};\n");
        assert!(
            after_hole.is_empty(),
            "still the value after a hole closes: {after_hole:?}"
        );
    }

    // §7.1 row 2: a custom property completes from the declarations of this
    // build — which is nothing in v1 (Q4). It must not fall back to the
    // standard property list, which no `--`-prefixed name can ever match.
    #[test]
    fn css_custom_property_position_offers_nothing() {
        let labels = css_block_completions("\tlet card = css {\n\t\t--~\n\t};\n");
        assert!(labels.is_empty(), "no custom properties in v1: {labels:?}");
        // A hyphenated STANDARD property is not the custom-property row, and
        // still offers the vocabulary the prefix filters.
        let hyphenated = css_block_completions("\tlet card = css {\n\t\tflex-~\n\t};\n");
        assert!(
            hyphenated.contains(&"flex-direction".to_string()),
            "a hyphenated standard property: {hyphenated:?}"
        );
    }

    // A nested rule's body is a body: the innermost one containing the cursor
    // decides, so `.md { pad| }` is property position inside the RULE.
    #[test]
    fn css_completion_fires_inside_a_nested_rule() {
        let labels = css_block_completions(
            "\tlet card = css {\n\t\tdisplay: flex;\n\t\t.md {\n\t\t\tpad~\n\t\t}\n\t};\n",
        );
        assert!(
            labels.contains(&"padding".to_string()) && labels.contains(&"padding-left".to_string()),
            "the property vocabulary inside a rule: {labels:?}"
        );
        // And after a completed nested rule the OUTER body is property
        // position again — the rule's `}` closes its item.
        let after = css_block_completions(
            "\tlet card = css {\n\t\t.md {\n\t\t\tpadding: 1px;\n\t\t}\n\t\tdisp~\n\t};\n",
        );
        assert!(
            after.contains(&"display".to_string()),
            "back in the outer body: {after:?}"
        );
    }

    // The boundary, element syntax's own: a condition head's ARGUMENT is
    // ordinary expression ground — brackets deep, so the css vocabulary does
    // not apply and the names in scope do.
    #[test]
    fn a_css_condition_argument_is_not_a_css_position() {
        let labels = css_block_completions(
            "\tlet theme = \"dark\";\n\tlet card = css {\n\t\t.within(\"data-theme\", the~) {\n\t\t\tdisplay: flex;\n\t\t}\n\t};\n",
        );
        assert!(
            labels.contains(&"theme".to_string()),
            "the binding in scope: {labels:?}"
        );
        assert!(
            !labels.contains(&"hover".to_string()),
            "not the combinator vocabulary: {labels:?}"
        );
    }

    // E105: the shape none of the 23 S5 pins covered — the block whose `}` has
    // not been typed yet. There is no `Node::Css` to read (the atom declines a
    // region with no end), so the parse-only question returned `None` and
    // completion fell through to ordinary expression ground, offering `fun`,
    // `for … in { }` and every name in scope inside what the author is plainly
    // writing as CSS. The live lexis answers it instead.
    #[test]
    fn an_unterminated_css_block_is_still_css_position() {
        // Nothing closes: no `}` for the block and none for `main` either.
        let labels = completions_at_marker(
            &format!("{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet card = css {{\n\t\tdisp~\n"),
            '~',
        );
        assert!(
            labels.contains(&"display".to_string())
                && labels.contains(&"flex-direction".to_string()),
            "the property vocabulary in an unterminated block: {labels:?}"
        );
        for wrong in ["fun", "for … in { }", "struct", "card", "space", "print"] {
            assert!(
                !labels.contains(&wrong.to_string()),
                "`{wrong}` is ordinary scope, not CSS: {labels:?}"
            );
        }
    }

    #[test]
    fn an_unterminated_css_block_keeps_its_four_positions() {
        // The dotted head, the value, and the custom property answer the same
        // as they do in a closed block — the fallback hands the position walk a
        // body start and changes nothing else about the four answers.
        let dotted = completions_at_marker(
            &format!("{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet card = css {{\n\t\t.~\n"),
            '~',
        );
        assert!(
            dotted.contains(&"hover".to_string()) && !dotted.contains(&"display".to_string()),
            "the combinators: {dotted:?}"
        );
        let value = completions_at_marker(
            &format!("{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet card = css {{\n\t\tdisplay: ~\n"),
            '~',
        );
        assert!(value.is_empty(), "value position is empty: {value:?}");
        let custom = completions_at_marker(
            &format!("{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet card = css {{\n\t\t--~\n"),
            '~',
        );
        assert!(custom.is_empty(), "no custom properties: {custom:?}");
    }

    #[test]
    fn an_unterminated_nested_rule_is_still_css_position() {
        // A nested rule's body is a body here too: the `.` that commits the
        // item to a condition is what makes the `{` after it a body rather than
        // a hole, which is the grammar's own marker.
        let labels = completions_at_marker(
            &format!(
                "{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet card = css {{\n\t\t.md {{\n\t\t\tpad~\n"
            ),
            '~',
        );
        assert!(
            labels.contains(&"padding".to_string()) && labels.contains(&"padding-left".to_string()),
            "the property vocabulary inside an unterminated rule: {labels:?}"
        );
    }

    #[test]
    fn an_unterminated_hole_is_not_css_position() {
        // The negative that keeps the fallback honest: a `{…}` hole is an
        // ordinary expression, and an unclosed one does not become CSS just
        // because a `css` block encloses it.
        let labels = completions_at_marker(
            &format!(
                "{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet ink = Color::gray(900);\n\tlet card = css {{\n\t\tcolor: {{i~\n"
            ),
            '~',
        );
        assert!(
            !labels.contains(&"display".to_string())
                && !labels.contains(&"flex-direction".to_string()),
            "a hole is not the property vocabulary: {labels:?}"
        );
        // Ordinary expression ground, which is what the hole always was. (The
        // names the enclosing `let` would bind are not among them, because with
        // the statement unterminated there is no analyzed binding to offer —
        // that is the mid-edit analysis, not this classification.)
        assert!(
            labels.contains(&"Color".to_string()) && labels.contains(&"fun".to_string()),
            "the expression vocabulary: {labels:?}"
        );
    }

    #[test]
    fn a_closed_css_block_does_not_leak_past_its_brace() {
        // The other negative: the fallback must not reach a block the author
        // finished. After the closing `}` the cursor is ordinary ground again,
        // even with the enclosing function still unterminated.
        let labels = completions_at_marker(
            &format!(
                "{CSS_BLOCK_PRELUDE}fun main() {{\n\tlet card = css {{\n\t\tdisplay: flex;\n\t}};\n\tlet other = ca~\n"
            ),
            '~',
        );
        assert!(
            labels.contains(&"card".to_string()),
            "the binding in scope: {labels:?}"
        );
        assert!(
            !labels.contains(&"display".to_string()),
            "not the property vocabulary: {labels:?}"
        );
    }

    // The negative: a `.` outside any block is untouched.
    #[test]
    fn a_dot_outside_a_css_block_still_completes_normally() {
        let labels = css_block_completions("\tlet card = style();\n\tcard.~\n");
        assert!(
            labels.contains(&"raw".to_string()) && labels.contains(&"flex_direction".to_string()),
            "the Style's members, in their METHOD spelling: {labels:?}"
        );
        assert!(
            !labels.contains(&"flex-direction".to_string()),
            "not the property vocabulary: {labels:?}"
        );
    }

    // A block in an element's head is still a block: the css pass runs BEFORE
    // the element pass and descends into markup, and the head's own vocabulary
    // does not reach inside a `.styled(…)` argument.
    #[test]
    fn css_completion_fires_inside_an_element_head_argument() {
        let source = format!(
            "{CSS_BLOCK_PRELUDE}import std::ui::view;\n\nfun main() {{\n\t<div .styled(css {{ disp~ }})></div>;\n}}\n"
        );
        let labels = completions_at_marker(&source, '~');
        assert!(
            labels.contains(&"display".to_string()),
            "the property vocabulary inside the head's argument: {labels:?}"
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
        // particular one: it used to look for `import std::io::print`, and when the
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
        let text = "import std::reactive::{ Signal, SignalCell };\n\nstruct Row {\n\tcell: SignalCell<i32>,\n}\n";
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
    // one token ate every argument's: `SignalCell<List<str>>` lit up as a single
    // struct and both arguments went dark. Nesting and a closure argument are
    // both here — a closure's parameter and return types are the case the whole
    // span reached furthest over.
    #[test]
    fn a_generic_type_application_tokenizes_its_head_and_arguments() {
        let text = "import std::reactive::SignalCell;\nimport std::shared::Shared;\n\nstruct Row {\n\tcells: SignalCell<List<str>>,\n\thook: Shared<|i32| bool>,\n}\n";
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
        assert!(
            token_at("SignalCell", 1).is_some(),
            "SignalCell head: {tokens:?}"
        );
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

    // --- E129: a NESTED `::` path descends, like an import path already does ---
    //
    // `code_path_completions` used to read only the identifier ending at the
    // `::`, so `style::FlexDirection::` saw `FlexDirection` — a MEMBER of
    // `style`, never a binding — and answered nothing. The import arm has
    // always descended (`import std::style::FlexDirection::` → four variants);
    // these hold the code arm to the same reach, with E53's in-scope rooting
    // still deciding the HEAD.

    // The owner's own case, spelled the way kolt spells it: a `prelude`
    // manifest puts `std::web`'s names in scope, so `style` is a module
    // reachable with no import — and the path descends into the enum from
    // there.
    #[test]
    fn nested_code_path_completion_descends_a_std_module_into_an_enum() {
        let labels = workspace_completions_at_cursor(&[
            (
                "main.vl",
                "fun main() {\n\tlet d = style::FlexDirection::|\n}\n",
            ),
            (
                "vilan.toml",
                "[package]\nname = \"probe\"\nprelude = \"std::web\"\n\n[entry.main]\ntarget = \"browser\"\n",
            ),
        ]);
        assert!(
            labels.contains(&"Row".to_string()) && labels.contains(&"ColumnReverse".to_string()),
            "`style::FlexDirection::` offers the enum's variants: {labels:?}"
        );
    }

    // The same descent through an explicit import, which is the spelling a
    // file without a prelude uses.
    #[test]
    fn nested_code_path_completion_descends_an_imported_std_module() {
        let labels = completions_at_cursor(
            "import std::style;\n\nfun main() {\n\tlet d = style::FlexDirection::|\n}\n",
        );
        assert!(
            labels.contains(&"Row".to_string()) && labels.contains(&"ColumnReverse".to_string()),
            "`style::FlexDirection::` offers the enum's variants: {labels:?}"
        );
    }

    #[test]
    fn nested_code_path_completion_descends_a_same_file_module_into_an_enum() {
        let labels = completions_at_cursor(
            "mod geo {\n\tenum Shape { Circle, Square }\n}\n\nfun main() {\n\tlet s = geo::Shape::|\n}\n",
        );
        assert!(
            labels.contains(&"Circle".to_string()) && labels.contains(&"Square".to_string()),
            "`geo::Shape::` offers the enum's variants: {labels:?}"
        );
    }

    // The one-segment control: the head still answers as it did, so the
    // descent above is an addition and not a replacement.
    #[test]
    fn one_segment_code_path_completion_still_answers_the_module() {
        let labels = completions_at_cursor(
            "mod geo {\n\tenum Shape { Circle, Square }\n}\n\nfun main() {\n\tlet s = geo::|\n}\n",
        );
        assert!(
            labels.contains(&"Shape".to_string()),
            "`geo::` offers the module's members: {labels:?}"
        );
        assert!(
            !labels.contains(&"Circle".to_string()),
            "and not the enum's variants, which are one level deeper: {labels:?}"
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
        // `lib.vl` is the package SURFACE, not a module of it.
        assert!(
            !labels.contains(&"lib".to_string()),
            "`import std::lib` is not a thing: {labels:?}"
        );
        // std's `lib.vl` surface publishes NOTHING since the alias sweep
        // (prelude.md §10.2) — its sixteen re-exports were short-name aliases
        // and each name is spelled at its real home now. The two prelude
        // modules are ordinary modules and list as such.
        assert!(
            !labels.contains(&"print".to_string()),
            "`std::print` was removed; `print` lives at `std::io::print`: {labels:?}"
        );
        for module in ["prelude", "web", "io"] {
            assert!(
                labels.contains(&module.to_string()),
                "`std::{module}` is a module: {labels:?}"
            );
        }
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
                    "import std::io::print;\nimport std::result::Result::{ self, Ok };\n\nfun main(): Result<i32, str> {\n\tprint(\"hi\");\n\tOk(1)\n}\n",
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

    // ── E114: the same answer, as PAINT ──────────────────────────────────────
    //
    // "Graying out dead code generally looks better to me" — the owner. The
    // spans the editor fades are the organizer's own verdict rather than a
    // second opinion, so what is faded is exactly what Organize Imports would
    // remove. These pin that identity, and the conservatism that comes with it.

    /// The name of the leaf each unused span covers, so a pin reads as the
    /// import the user would see faded.
    fn faded(document: &Document) -> Vec<String> {
        let text = document.analyzed_text().to_string();
        document
            .unused_import_spans()
            .into_iter()
            .map(|span| text[span.into_range()].to_string())
            .collect()
    }

    #[test]
    fn an_unused_import_leaf_is_faded_and_a_used_one_is_not() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::{ alpha, beta };\nfun main() {\n\talpha();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert_eq!(faded(&document), vec!["beta".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_re_export_is_never_faded() {
        // `export import` binds a name for somebody ELSE. This file not using
        // it is the point of writing it, so fading it would be a lie — and the
        // organizer never prunes one either, which is the same rule.
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "export import pkg::helper::beta;\nfun main() {}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(faded(&document).is_empty(), "{:?}", faded(&document));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_type_only_import_is_not_faded() {
        // The honesty case the organizer already had to get right: a name used
        // only in a TYPE position has no value reference at all, and a fade
        // driven by value uses alone would gray a live import.
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::Widget;\nfun main() {\n\tlet w: Widget = Widget {};\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(faded(&document).is_empty(), "{:?}", faded(&document));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_is_faded_while_the_file_carries_a_diagnostic() {
        // A half-typed name might be about to use the very import in question,
        // so a broken file fades nothing. A mark that lies is worse than no
        // mark, and this is the same gate the organizer's pruning takes.
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::beta;\nfun main() {\n\tmissing_name();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            !document.diagnostics.is_empty(),
            "the fixture must actually be broken",
        );
        assert!(faded(&document).is_empty(), "{:?}", faded(&document));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── E114: unused LOCAL declarations, as paint ────────────────────────────
    //
    // The declarations third, at the only scope Vilan actually closes. There is
    // no visibility marker in the language (`pub fun f()` is a parse error whose
    // curated rule says "a module's items are importable as they stand"), so a
    // top-level item is module surface and can never be faded from a single
    // entry's analysis — which is itself pinned below, because it is the whole
    // shape of the feature and a later refactor must not quietly widen it.

    /// The name each faded local covers, so a pin reads as the binding the user
    /// would see grayed.
    fn faded_locals(document: &Document) -> Vec<String> {
        let text = document.analyzed_text().to_string();
        let mut names: Vec<String> = document
            .unused_local_spans()
            .into_iter()
            .map(|span| text[span.into_range()].to_string())
            .collect();
        names.sort();
        names
    }

    /// The source each faded dead region covers.
    fn faded_dead(document: &Document) -> Vec<String> {
        let text = document.analyzed_text().to_string();
        document
            .unreachable_spans()
            .into_iter()
            .map(|span| text[span.into_range()].to_string())
            .collect()
    }

    /// A green fixture's two answers, with the fixture's own greenness asserted
    /// first — every pin below depends on a clean analysis, since both producers
    /// are switched off by a diagnostic and would otherwise pass vacuously.
    fn green(files: &[(&str, &str)]) -> (PathBuf, Document) {
        let (dir, document) = analyze_workspace(files);
        assert!(
            document.diagnostics.is_empty(),
            "the pin needs a GREEN fixture, got {:?}",
            document
                .diagnostics
                .iter()
                .map(|error| &error.msg)
                .collect::<Vec<_>>(),
        );
        (dir, document)
    }

    #[test]
    fn a_local_nothing_reads_is_faded_and_a_read_one_is_not() {
        let (dir, document) = green(&[(
            "main.vl",
            "fun main() {\n\tlet kept = 1;\n\tlet dead = 2;\n\tprint(kept);\n}\n",
        )]);
        assert_eq!(faded_locals(&document), vec!["dead".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_underscore_led_local_is_never_faded() {
        // `_`-led is the language's own "I know" marker — the `[must_use]` rule
        // tells you to write `let _ = …` — so fading it would gray the very
        // gesture that says "I meant this".
        let (dir, document) = green(&[(
            "main.vl",
            "fun main() {\n\tlet _unused = 1;\n\tlet _ = 2;\n}\n",
        )]);
        assert!(
            faded_locals(&document).is_empty(),
            "{:?}",
            faded_locals(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_top_level_item_is_never_faded_because_the_language_has_no_private_one() {
        // THE RULING, pinned. `unreachable_helper` and `spare` are referenced by
        // nothing in this program — the emission pruner would emit neither — and
        // they still must not fade: any file the editor never analyzed may write
        // `import pkg::main::unreachable_helper;` and get it, with nothing in
        // the language marking it private. A fade here would be a guess about a
        // world this analysis cannot see.
        let (dir, document) = green(&[(
            "main.vl",
            "let spare = 7;\nfun unreachable_helper(): i32 {\n\t1\n}\nfun main() {}\n",
        )]);
        assert!(
            faded_locals(&document).is_empty(),
            "a top-level item is module surface: {:?}",
            faded_locals(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_parameter_is_never_faded() {
        // A parameter is signature, not a local: an impl must take what the
        // declaration takes, so an unread one is routinely obligatory.
        let (dir, document) = green(&[(
            "main.vl",
            "fun ignores(value: i32) {}\nfun main() {\n\tignores(1);\n}\n",
        )]);
        assert!(
            faded_locals(&document).is_empty(),
            "{:?}",
            faded_locals(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_local_read_only_under_a_browser_color_is_not_faded() {
        // E113's coloring, on this third. The file is analyzed under the color
        // its `vilan.toml` declares — the same one the build takes — so a read
        // that only exists in a browser build is still a read. Under the process
        // analysis this file would not even type-check, and the diagnostic gate
        // below would switch the fade off rather than gray a live binding.
        let (dir, document) = green(&[
            (
                "src/main.vl",
                "fun main() {\n\tlet width = 4;\n\tprint(width);\n}\n",
            ),
            (
                "vilan.toml",
                "[package]\nname = \"app\"\ntarget = \"browser\"\n",
            ),
        ]);
        assert!(
            faded_locals(&document).is_empty(),
            "{:?}",
            faded_locals(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_local_is_faded_while_the_file_carries_a_diagnostic() {
        // The imports third's conservatism, inherited: a half-typed line might
        // be about to read the binding.
        let (dir, document) = analyze_workspace(&[(
            "main.vl",
            "fun main() {\n\tlet dead = 1;\n\tmissing_name();\n}\n",
        )]);
        assert!(
            !document.diagnostics.is_empty(),
            "the fixture must actually be broken",
        );
        assert!(
            faded_locals(&document).is_empty(),
            "{:?}",
            faded_locals(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_local_is_faded_while_the_buffer_is_ahead_of_the_analysis() {
        // The other half of the imports third's conservatism: a stale analysis
        // describes a file the user has already changed, and its spans no longer
        // index the text the editor would fade.
        let (dir, mut document) =
            green(&[("main.vl", "fun main() {\n\tlet dead = 1;\n\tprint(2);\n}\n")]);
        assert_eq!(faded_locals(&document), vec!["dead".to_string()]);
        document.set_text("fun main() {\n\tlet dead = 1;\n\tprint(3);\n}\n");
        assert!(document.is_stale(), "the fixture must actually be stale");
        assert!(
            faded_locals(&document).is_empty(),
            "{:?}",
            faded_locals(&document),
        );
        assert!(
            faded_dead(&document).is_empty(),
            "{:?}",
            faded_dead(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── E114: unreachable code, as paint ─────────────────────────────────────
    //
    // The divergence analysis is the CHECKER's (`analyzer::Divergence`) — since
    // B204, with no widening at all: a `panic(…)` call and an endless
    // `for { … }` are leaves for both askers, so what fades here is what the
    // checker already treats as dead. Each pin below is one way control leaves.

    #[test]
    fn a_statement_after_ret_is_faded() {
        let (dir, document) = green(&[("main.vl", "fun main() {\n\tret;\n\tprint(1);\n}\n")]);
        assert_eq!(faded_dead(&document), vec!["print(1)".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_whole_dead_tail_is_one_faded_region_not_one_per_statement() {
        // What died is the REST of the block. Three faded lines say the same
        // thing three times; the editor gets one range.
        let (dir, document) = green(&[(
            "main.vl",
            "fun main() {\n\tret;\n\tprint(1);\n\tprint(2);\n\tprint(3);\n}\n",
        )]);
        assert_eq!(
            faded_dead(&document),
            vec!["print(1);\n\tprint(2);\n\tprint(3)".to_string()],
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_statement_after_panic_is_faded() {
        // The paint-only widening. `panic` lowers to a `throw`, so the statement
        // after one genuinely never runs — but the CHECKER does not count it as
        // divergence, and must not: `expr_diverges` gates the R4/R7 exemption
        // and return-position checking, and widening it there would change what
        // the language accepts.
        let (dir, document) = green(&[(
            "main.vl",
            "import std::io::panic;\nfun main() {\n\tpanic(\"stop\");\n\tprint(1);\n}\n",
        )]);
        assert_eq!(faded_dead(&document), vec!["print(1)".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_statement_after_a_loop_tail_jump_is_faded() {
        // `jump` is the loop tail — the language's `break`/`continue` — and one
        // of the checker's own two divergence leaves.
        let (dir, document) = green(&[(
            "main.vl",
            "fun main() {\n\tfor {\n\t\tjump break;\n\t\tprint(1);\n\t}\n}\n",
        )]);
        assert_eq!(faded_dead(&document), vec!["print(1)".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_statement_after_an_if_whose_every_arm_diverges_is_faded() {
        let (dir, document) = green(&[(
            "main.vl",
            "fun main() {\n\tif 1 > 0 {\n\t\tret;\n\t} else {\n\t\tret;\n\t}\n\tprint(1);\n}\n",
        )]);
        assert_eq!(faded_dead(&document), vec!["print(1)".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_if_with_no_else_leaves_the_rest_of_the_block_alive() {
        // The exemption that keeps the whole class honest: without an `else` the
        // implicit fall-through continues, so the statement after it is reached
        // on the condition's false path.
        let (dir, document) = green(&[(
            "main.vl",
            "fun main() {\n\tif 1 > 0 {\n\t\tret;\n\t}\n\tprint(1);\n}\n",
        )]);
        assert!(
            faded_dead(&document).is_empty(),
            "{:?}",
            faded_dead(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_statement_after_an_endless_for_is_faded_and_a_broken_one_is_not() {
        // `for { … }` with no condition is the language's only endless-loop
        // form (`for cond { … }` is the `while`), and control leaves it only by
        // leaving the function. A `jump break` binds to the NEAREST enclosing
        // loop, so the second fixture's loop does fall through and its tail
        // stays alive.
        let (endless_dir, endless) = green(&[(
            "main.vl",
            "fun main() {\n\tfor {\n\t\tprint(1);\n\t}\n\tprint(2);\n}\n",
        )]);
        assert_eq!(faded_dead(&endless), vec!["print(2)".to_string()]);
        let _ = std::fs::remove_dir_all(&endless_dir);

        let (broken_dir, broken) = green(&[(
            "main.vl",
            "fun main() {\n\tfor {\n\t\tjump break;\n\t}\n\tprint(2);\n}\n",
        )]);
        assert!(
            faded_dead(&broken).is_empty(),
            "a loop something breaks out of falls through: {:?}",
            faded_dead(&broken),
        );
        let _ = std::fs::remove_dir_all(&broken_dir);
    }

    #[test]
    fn nothing_is_faded_as_unreachable_while_the_file_carries_a_diagnostic() {
        // The same conservatism the imports third takes: a broken file's
        // statement list is a guess, and a mark that lies is worse than no mark.
        let (dir, document) =
            analyze_workspace(&[("main.vl", "fun main() {\n\tret;\n\tmissing_name();\n}\n")]);
        assert!(
            !document.diagnostics.is_empty(),
            "the fixture must actually be broken",
        );
        assert!(
            faded_dead(&document).is_empty(),
            "{:?}",
            faded_dead(&document),
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_body_that_merely_ends_in_ret_fades_nothing() {
        // The synthesized-tail case, which is the common one: `fun f() { ret; }`
        // has a trailing void expression the user never wrote. Fading it would
        // gray a closing brace.
        let (dir, document) = green(&[("main.vl", "fun main() {\n\tprint(1);\n\tret;\n}\n")]);
        assert!(
            faded_dead(&document).is_empty(),
            "{:?}",
            faded_dead(&document),
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

    // Organize Imports STRIPS what the prelude covers (prelude.md §11.1). The
    // import is USED here — `print` is called — but it is redundant, because
    // the base prelude binds the same definition ambiently. That is the whole
    // point of the determination: leaving it would have every file carry the
    // statement the feature exists to delete.
    #[test]
    fn organize_strips_an_import_the_prelude_already_covers() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import std::io::print;\nimport pkg::helper::alpha;\nfun main() {\n\tprint(\"x\");\n\talpha();\n}\n",
            ),
            ("helper.vl", ORGANIZE_HELPER),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let result = organized(&document).expect("a prelude-covered import offers a strip edit");
        assert_eq!(
            result,
            "import pkg::helper::alpha;\nfun main() {\n\tprint(\"x\");\n\talpha();\n}\n",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The same-DEFINITION half of §11.1: a local `print` that merely shares the
    // prelude name is a different definition, so its import survives.
    #[test]
    fn organize_keeps_an_import_that_only_shares_a_prelude_name() {
        let (dir, document) = analyze_workspace(&[
            (
                "main.vl",
                "import pkg::helper::print;\nfun main() {\n\tprint(\"x\");\n}\n",
            ),
            ("helper.vl", "export fun print(message: str): void {}\n"),
        ]);
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        assert_eq!(
            organized(&document),
            None,
            "an import naming a DIFFERENT definition is not prelude-covered"
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
            (18..=19).contains(&inside),
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

    /// E122's fold, and the thing that lets ONE capture serve every token
    /// request: the salvage tail is folded into the analysis's CAPTURE at
    /// adoption, not read back out of the walk at serve time. So the captured
    /// stream — what `semanticTokens/full` and `semanticTokens/range` both
    /// answer from — is byte-for-byte what the walk answers, salvage included,
    /// and the line index built beside it reaches into the salvaged tail.
    ///
    /// Before this, `capture_landed` ran on the analysis thread (where the
    /// tail does not exist yet) and nothing folded it in afterwards, so the
    /// capture and the walk disagreed exactly on the salvaged region.
    #[test]
    fn the_capture_carries_the_salvaged_tail_and_indexes_its_lines() {
        let whole = "fun alpha() {\n\tlet a = 1;\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        let broken = "fun alpha() {\n\tlet a = i\"\"\";\n}\nfun omega() {\n\tlet zeta = 9;\n}\n";
        let zeta = offset_at(broken, "zeta", 0);
        let mut document = analyze_text(whole);
        document.adopt_analysis(analyze_text(broken));
        // Premise: this adoption really did retain a tail, or the equality
        // below would hold for the uninteresting reason.
        assert!(
            !document.retained_tail.is_empty(),
            "the break no longer retains a tail — this pin's premise is gone"
        );

        assert_eq!(
            document.landed.tokens,
            document.semantic_tokens(),
            "the capture every request is served from must be exactly the walk, \
             salvage and all"
        );

        let line = document.analyzed_index().position(zeta).line;
        let window = document.landed.token_positions_in_lines(line, line);
        let sliced = &document.landed.tokens[window];
        let filtered: Vec<_> = document
            .semantic_tokens()
            .into_iter()
            .filter(|(span, ..)| document.analyzed_index().range(span).start.line == line)
            .collect();
        assert!(
            !sliced.is_empty(),
            "the line index must reach into the salvaged tail, not stop at the \
             last token the fresh analysis produced"
        );
        assert_eq!(
            sliced, filtered,
            "and the slice of a salvaged line must be the filter of it"
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

    const FIRST: &str = "import std::io::print;\n\nfun main() {\n\tprint(\"one\");\n}\n";
    const SECOND: &str = "import std::io::print;\n\nfun main() {\n\tlet greeting = \"two\";\n\tprint(greeting);\n}\n";

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

    /// M26: a CANCELLED analysis gives back everything it leaked, on the thread
    /// that ran it.
    ///
    /// The token is set before the analysis starts, so the first checkpoint —
    /// the parse boundary in `analyze_source_unfenced` — is the one that fires:
    /// the entry text and the entry tree are both leaked by then, and nothing
    /// downstream has been built. That is the earliest a cancel can land and
    /// therefore the shape most likely to leave a handle behind, which is why
    /// it is the one pinned exactly. The reclaim seam itself is shared by every
    /// checkpoint: they all fall through to the same wrap-and-drop in
    /// `analyze_on_this_thread`.
    #[test]
    fn a_cancelled_analysis_gives_back_its_entry_text_and_tree() {
        on_big_stack(|| {
            leak_tally::reset();
            let token = vilan_core::cancel::CancelToken::new();
            token.cancel();
            let _scope = token.install();
            let document =
                Document::analyze_on_this_thread(FIRST, &std_root(), Path::new("reclaim.vl"));
            assert!(
                !document.program.is_some(),
                "a cancelled analysis carries no program — it stopped before there was one",
            );
            assert_eq!(
                leak_tally::bytes(LeakSite::LspEntryText),
                FIRST.len(),
                "the entry text is leaked on the way IN, before any checkpoint can fire",
            );
            let tree = leak_tally::bytes(LeakSite::EntryAst);
            assert!(tree > 0, "and so is the parsed tree");
            assert_eq!(
                leak_tally::released(LeakSite::LspEntryText),
                FIRST.len(),
                "…and given back before the cancelled analysis returned",
            );
            assert_eq!(leak_tally::released(LeakSite::EntryAst), tree);
            assert_eq!(leak_tally::outstanding(LeakSite::LspEntryText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::EntryAst), 0);
            drop(document);
            assert_eq!(
                leak_tally::outstanding(LeakSite::LspEntryText),
                0,
                "the degraded document owns nothing, so dropping it cannot double-free",
            );
        });
    }

    /// M26, the other half: a cancel that lands DURING the analysis rather than
    /// before it.
    ///
    /// Where it lands is the machine's business — the watcher fires after a
    /// millisecond, which on a fast box is inside the checks and on a slow one
    /// may be after the analysis finished altogether — and the pin is written
    /// so that it does not matter. What is asserted is the tally after the
    /// document drops, which must be zero in every case: a cancelled analysis
    /// reclaims on its own thread, an uncancelled one reclaims when its
    /// document is dropped, and there is no third outcome that leaves bytes
    /// outstanding.
    #[test]
    fn a_cancel_landing_mid_analysis_leaves_nothing_outstanding() {
        on_big_stack(|| {
            leak_tally::reset();
            let token = vilan_core::cancel::CancelToken::new();
            let watcher = {
                let token = token.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    token.cancel();
                })
            };
            let _scope = token.install();
            let document =
                Document::analyze_on_this_thread(SECOND, &std_root(), Path::new("reclaim.vl"));
            watcher.join().expect("the watcher thread");
            let stopped_early = !document.program.is_some();
            drop(document);
            assert_eq!(
                leak_tally::outstanding(LeakSite::LspEntryText),
                0,
                "the entry text is given back whether the analysis was cancelled                  (stopped_early={stopped_early}) or outran the cancel",
            );
            assert_eq!(
                leak_tally::outstanding(LeakSite::EntryAst),
                0,
                "and so is the tree (stopped_early={stopped_early})",
            );
        });
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
    use crate::document::tests::{base_cache_guard, on_big_stack, std_root};
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
        let _cache = base_cache_guard();
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
                clean_helper(1).len(),
                "M23: an unchanged buffer is parsed and owned ONCE — the base \
                 world stored for the first analysis claims that copy and \
                 serves the other 29, which is the cost M9 paid per analysis"
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
            assert_eq!(
                leak_tally::bytes(LeakSite::OwnedModuleText),
                large_helper(1).len(),
                "M23, at the realistic size: one parse for 30 analyses"
            );

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
            let claims_before_drop = vilan_core::analyzer::base_cache_overlay_claims();
            drop(document);
            println!(
                "[m9 after-drop] outstanding: LspEntryText {} B, EntryAst {} B, \
                 OwnedModuleText {} B, OwnedModuleAst {} B, OwnedModuleErrors {} B; \
                 base-cache overlay claims {claims_before_drop:?}",
                leak_tally::outstanding(LeakSite::LspEntryText),
                leak_tally::outstanding(LeakSite::EntryAst),
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                leak_tally::outstanding(LeakSite::OwnedModuleAst),
                leak_tally::outstanding(LeakSite::OwnedModuleErrors),
            );
            vilan_core::analyzer::base_cache_clear();
            println!(
                "[m23 after-clear] outstanding: OwnedModuleText {} B, \
                 OwnedModuleAst {} B, base-cache overlay claims {:?}",
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                leak_tally::outstanding(LeakSite::OwnedModuleAst),
                vilan_core::analyzer::base_cache_overlay_claims(),
            );

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The core M9 pin, per case: a DISTINCT overlay content per analysis (a
    /// landed keystroke) grows neither process-global cache; each analysis
    /// owns exactly one copy of the edited module; supersession
    /// (`adopt_analysis`) reclaims the previous analysis's copy; and closing
    /// the document, then the base cache that claims the last copy (M23),
    /// nets every owned site to zero.
    #[test]
    fn a_dependent_edits_module_copies_are_analysis_owned_and_reclaimed() {
        let _cache = base_cache_guard();
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
            // analysis's is still out — and the base world stored for it
            // holds a second claim on that same copy (M23), not a copy of
            // its own.
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                helper_bytes as isize,
                "adoption must reclaim the superseded analysis's module copy"
            );
            assert_eq!(
                vilan_core::analyzer::base_cache_overlay_claims(),
                (1, helper_bytes),
                "the stored base world claims the live copy — one claim, no \
                 second copy"
            );
            drop(document);
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                helper_bytes as isize,
                "M23: closing the document releases the ANALYSIS's claim; the \
                 stored world's keeps the allocation alive"
            );
            vilan_core::analyzer::base_cache_clear();
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
        let _cache = base_cache_guard();
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
            // Behavior parity with the global rich path: the importer surfaces
            // the module's parse error, and it carries a real span into the
            // module's own text rather than the empty one the loader used to
            // push (E100).
            let parse_error = document
                .diagnostics
                .iter()
                .find(|error| error.msg.contains("expected a matching `}`"))
                .unwrap_or_else(|| {
                    panic!(
                        "the broken module's parse error must reach the importer, got {:?}",
                        document.diagnostics
                    )
                });
            assert_ne!(
                parse_error.span.into_range(),
                0..0,
                "a module parse error carries its own span: {parse_error:?}"
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
            // M23: the base world stored for the last analysis claims that
            // analysis's copies — text, tree AND the rendered error slice —
            // so the balance nets to zero only once the cache lets go.
            vilan_core::analyzer::base_cache_clear();
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleErrors), 0);

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The repeated-content pin, and M23's win at its sharpest: an unchanged
    /// buffer is parsed and owned ONCE, not once per analysis. The base world
    /// stored for the first analysis claims that copy, so every later
    /// analysis is served the world and its claim — which is exactly what
    /// M9's store gate forbade, at the price of rebuilding the whole
    /// pre-entry world on every keystroke.
    #[test]
    fn a_repeated_content_is_loaded_once_and_served_from_the_stored_world() {
        let _cache = base_cache_guard();
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
                helper_bytes,
                "M23: five analyses over one unchanged buffer parse it once — \
                 four of them are served the stored world's claim"
            );
            assert_eq!(
                vilan_core::analyzer::base_cache_overlay_claims(),
                (1, helper_bytes),
                "one stored world, one claim, on the one copy"
            );
            drop(document);
            vilan_core::analyzer::base_cache_clear();
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The multi-dependent pin, and M23's reference count at work: two open
    /// documents importing the edited module hold a claim EACH on one copy —
    /// they share the base world the first of them stored — and the copy
    /// outlives every individual release. Dropping one document leaves the
    /// other answering from memory its own claim keeps alive; the allocation
    /// dies only when the LAST claim goes, which here is the cache's.
    #[test]
    fn open_dependents_share_one_claimed_copy_that_outlives_each_of_them() {
        let _cache = base_cache_guard();
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
                helper_bytes,
                "M23: the second dependent is served the first's stored world \
                 — one copy, two claims"
            );
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                helper_bytes as isize,
            );
            drop(first);
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                helper_bytes as isize,
                "dropping one dependent releases ITS claim only — the other \
                 dependent is still reading this allocation"
            );
            assert!(
                !second.semantic_tokens().is_empty(),
                "the surviving dependent no longer answers from its program"
            );
            drop(second);
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                helper_bytes as isize,
                "the stored world's claim is the last one standing"
            );
            vilan_core::analyzer::base_cache_clear();
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleText), 0);
            assert_eq!(leak_tally::outstanding(LeakSite::OwnedModuleAst), 0);

            vilan_core::analyzer::set_document_overlay(&helper_path, None);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    /// The no-dependent pin: an analysis that loads no overlay-served module
    /// — the ordinary open document, whatever unrelated overlays exist —
    /// owns nothing, so the mechanism costs it nothing and the world it
    /// stores claims nothing.
    #[test]
    fn an_analysis_that_loads_no_overlay_module_owns_nothing() {
        let _cache = base_cache_guard();
        let unrelated =
            std::env::temp_dir().join(format!("vilan_m9_unrelated_{}.vl", std::process::id()));
        on_big_stack(move || {
            vilan_core::analyzer::set_document_overlay(&unrelated, Some("x".to_string()));
            leak_tally::reset();
            let document = Document::analyze_on_this_thread(
                "import std::io::print;

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

/// M23's gate: the three-file scripted session the perf-25 lane measured on
/// kolt — `views.vl` → `theme.vl` → `client.vl`, each opened, edited a few
/// times, all three left OPEN — asserted on a package of the same shape.
///
/// `client.vl` imports `pkg::views`, and `views.vl` is an open buffer, so
/// every one of `client.vl`'s loads of it is OVERLAY-served. Under M9's store
/// gate that meant `client.vl` never stored a base world and never hit one:
/// measured on kolt, `base` 1,357–2,713 ms on EVERY `client.vl` keystroke,
/// against `base 0.0 ms` for its two siblings, which import no open file. The
/// claim protocol (M23) is what closes it, and this pin is the shape the
/// number came from.
#[cfg(test)]
mod m23_scripted_session {
    use super::*;
    use crate::document::tests::{base_cache_guard, on_big_stack, std_root};
    use std::sync::atomic::{AtomicU32, Ordering};

    /// The open sibling `client.vl` imports — big enough that rebuilding the
    /// world around it is real work, and it imports nothing itself, so its
    /// OWN analyses store a world that claims nothing.
    fn views(keystroke: usize) -> String {
        let mut text = format!(
            "export fun app_shell(): i32 {{\n\t{}\n}}\n",
            1000 + keystroke
        );
        for ordinal in 0..40 {
            text.push_str(&format!(
                "export fun view_{ordinal:02}(input: i32): i32 {{\n\tlet scaled = input * {};\n\tscaled + {ordinal}\n}}\n",
                ordinal + 1
            ));
        }
        text
    }

    /// The const-heavy sibling that reaches no package module — kolt's
    /// `theme.vl`, the control that already read `base 0.0 ms`.
    fn theme(keystroke: usize) -> String {
        format!("export fun accent(): i32 {{\n\t{}\n}}\n", 2000 + keystroke)
    }

    /// The subject: it imports the OPEN sibling, which is the whole point.
    fn client(keystroke: usize) -> String {
        format!(
            "import pkg::views::app_shell;\n\nfun main() {{\n\tlet shell = app_shell() + {};\n}}\n",
            3000 + keystroke
        )
    }

    /// A scratch package of the three files, on disk and unique per call (the
    /// overlay map is process-global).
    fn scratch_session() -> (PathBuf, PathBuf, PathBuf, PathBuf) {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("vilan_m23_session_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let views_path = dir.join("views.vl");
        let theme_path = dir.join("theme.vl");
        let client_path = dir.join("client.vl");
        std::fs::write(&views_path, views(0)).expect("write views.vl");
        std::fs::write(&theme_path, theme(0)).expect("write theme.vl");
        std::fs::write(&client_path, client(0)).expect("write client.vl");
        (dir, views_path, theme_path, client_path)
    }

    /// The session, and the assertion the item names: `client.vl` hits the
    /// base cache from its SECOND analysis, with its two open siblings still
    /// overlaid — plus the observation identity that makes a hit legitimate
    /// (the served world answers what a cleared cache answers).
    #[test]
    fn the_scripted_sessions_third_file_hits_from_its_second_analysis() {
        let _cache = base_cache_guard();
        let (dir, views_path, theme_path, client_path) = scratch_session();
        on_big_stack(move || {
            let std_dir = std_root();
            vilan_core::analyzer::base_cache_clear();

            // File 1: views.vl opened and edited. It imports no sibling, so
            // it hits from its own second analysis — M21's behavior, and the
            // vacuity guard for everything below.
            vilan_core::analyzer::set_document_overlay(&views_path, Some(views(0)));
            let mut views_document =
                Document::analyze_on_this_thread(&views(0), &std_dir, &views_path);
            for keystroke in 1..=4 {
                let text = views(keystroke);
                vilan_core::analyzer::set_document_overlay(&views_path, Some(text.clone()));
                let (hits_before, _) = vilan_core::analyzer::base_cache_stats();
                views_document.adopt_analysis(Document::analyze_on_this_thread(
                    &text,
                    &std_dir,
                    &views_path,
                ));
                let (hits_after, _) = vilan_core::analyzer::base_cache_stats();
                assert!(
                    hits_after > hits_before,
                    "views.vl keystroke {keystroke} must hit the base cache \
                     (M21) — without it this pin cannot tell M23 apart from a \
                     cache that never works"
                );
            }
            assert!(
                views_document.diagnostics.is_empty(),
                "views.vl must compile clean, got {:?}",
                views_document.diagnostics
            );

            // File 2: theme.vl, opened and edited while views.vl stays open.
            vilan_core::analyzer::set_document_overlay(&theme_path, Some(theme(0)));
            let mut theme_document =
                Document::analyze_on_this_thread(&theme(0), &std_dir, &theme_path);
            for keystroke in 1..=4 {
                let text = theme(keystroke);
                vilan_core::analyzer::set_document_overlay(&theme_path, Some(text.clone()));
                theme_document.adopt_analysis(Document::analyze_on_this_thread(
                    &text,
                    &std_dir,
                    &theme_path,
                ));
            }
            assert!(theme_document.diagnostics.is_empty());
            assert_eq!(
                vilan_core::analyzer::base_cache_overlay_claims(),
                (0, 0),
                "neither sibling imports an open file, so neither world \
                 claims anything — the claims below are client.vl's"
            );

            // File 3: client.vl, which imports the OPEN views.vl. Its first
            // analysis is a miss that stores; every one after it hits.
            let client_text = client(0);
            vilan_core::analyzer::set_document_overlay(&client_path, Some(client_text.clone()));
            let (hits_open, misses_open) = vilan_core::analyzer::base_cache_stats();
            let mut client_document =
                Document::analyze_on_this_thread(&client_text, &std_dir, &client_path);
            let (hits_first, misses_first) = vilan_core::analyzer::base_cache_stats();
            assert!(
                client_document.diagnostics.is_empty(),
                "client.vl must compile clean over its open sibling, got {:?}",
                client_document.diagnostics
            );
            assert_eq!(
                (hits_first, misses_first > misses_open),
                (hits_open, true),
                "the first analysis of a new key must MISS and store"
            );
            let views_bytes = views(4).len();
            assert_eq!(
                vilan_core::analyzer::base_cache_overlay_claims(),
                (1, views_bytes),
                "the stored world must claim the overlay-served views.vl copy \
                 it borrows — that claim is what M9's store gate refused to \
                 make, at the price of `base` 1.4-2.7 s per keystroke"
            );

            for keystroke in 1..=8 {
                let text = client(keystroke);
                vilan_core::analyzer::set_document_overlay(&client_path, Some(text.clone()));
                let (hits_before, misses_before) = vilan_core::analyzer::base_cache_stats();
                client_document.adopt_analysis(Document::analyze_on_this_thread(
                    &text,
                    &std_dir,
                    &client_path,
                ));
                let (hits_after, misses_after) = vilan_core::analyzer::base_cache_stats();
                assert!(
                    hits_after > hits_before,
                    "M23: client.vl keystroke {keystroke} must HIT the base \
                     cache — it imports an open sibling, which is exactly the \
                     case §7.9.4a's store gate refused"
                );
                assert_eq!(
                    misses_after, misses_before,
                    "client.vl keystroke {keystroke} must not also miss"
                );
                assert!(
                    client_document.diagnostics.is_empty(),
                    "client.vl keystroke {keystroke} must stay clean over the \
                     served world, got {:?}",
                    client_document.diagnostics
                );
            }

            // Observation identity: what the served world answers must be
            // what a cold build answers. A world whose borrows had been freed
            // answered `cannot find 'greeting' in the imported path` when the
            // M9 gate was planted away, so this is the assertion that tells a
            // legitimate hit from a corrupt one.
            let final_text = client(8);
            let hot_tokens = format!("{:?}", client_document.semantic_tokens());
            let hot_diagnostics = format!("{:?}", client_document.diagnostics);
            vilan_core::analyzer::base_cache_clear();
            let cold = Document::analyze_on_this_thread(&final_text, &std_dir, &client_path);
            assert_eq!(
                (
                    format!("{:?}", cold.semantic_tokens()),
                    format!("{:?}", cold.diagnostics)
                ),
                (hot_tokens, hot_diagnostics),
                "a base-cache hit over an open sibling must be \
                 observation-identical to a build with the cache cleared"
            );

            drop(cold);
            drop(client_document);
            drop(theme_document);
            drop(views_document);
            vilan_core::analyzer::base_cache_clear();
            for path in [&views_path, &theme_path, &client_path] {
                vilan_core::analyzer::set_document_overlay(path, None);
            }
            let _ = std::fs::remove_dir_all(&dir);
        });
    }
}

/// M24, where it meets M23: a world evicted by the BYTE BUDGET lets go of the
/// overlay claims it held, exactly as a displaced or stale one does — and the
/// live analysis that was served from that world goes on reading, because its
/// own claim is what keeps the allocation alive.
#[cfg(test)]
mod m24_budget_eviction {
    use super::*;
    use crate::document::tests::{base_cache_guard, on_big_stack, std_root};
    use std::sync::atomic::{AtomicU32, Ordering};
    use vilan_core::leak_tally::{self, LeakSite};

    const SIBLING: &str = "export fun app_shell(): i32 {\n\t7\n}\n";
    const IMPORTER: &str =
        "import pkg::views::app_shell;\n\nfun main() {\n\tlet shell = app_shell();\n}\n";

    #[test]
    fn an_evicted_world_releases_its_overlay_claims_and_the_live_analysis_survives() {
        let _cache = base_cache_guard();
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("vilan_m24_claims_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let sibling_path = dir.join("views.vl");
        let importer_path = dir.join("client.vl");
        std::fs::write(&sibling_path, SIBLING).expect("write views.vl");
        std::fs::write(&importer_path, IMPORTER).expect("write client.vl");

        on_big_stack(move || {
            let std_dir = std_root();
            vilan_core::analyzer::set_base_cache_budget(
                vilan_core::analyzer::BASE_CACHE_DEFAULT_BUDGET,
            );
            vilan_core::analyzer::base_cache_clear();
            // The sibling is OPEN, so the importer's world claims its copy.
            vilan_core::analyzer::set_document_overlay(&sibling_path, Some(SIBLING.to_string()));
            leak_tally::reset();
            let document = Document::analyze_on_this_thread(IMPORTER, &std_dir, &importer_path);
            assert!(
                document.diagnostics.is_empty(),
                "the fixture must compile clean, got {:?}",
                document.diagnostics
            );
            assert_eq!(
                vilan_core::analyzer::base_cache_overlay_claims(),
                (1, SIBLING.len()),
                "the stored world must claim the open sibling's copy (M23) — \
                 without that this pin has nothing to evict"
            );

            // Evict it by budget rather than by staleness or displacement.
            vilan_core::analyzer::set_base_cache_budget(1);
            assert_eq!(
                vilan_core::analyzer::base_cache_retained(),
                0,
                "the budget must evict the world"
            );
            assert_eq!(
                vilan_core::analyzer::base_cache_overlay_claims(),
                (0, 0),
                "an evicted world must give its overlay claims back — every \
                 eviction path releases through the same routine"
            );
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                SIBLING.len() as isize,
                "the LIVE analysis's own claim keeps the allocation alive: \
                 evicting a world must not free what a program is reading"
            );
            assert!(
                !document.semantic_tokens().is_empty(),
                "the analysis served from the evicted world must go on \
                 answering from memory its own claim holds"
            );

            drop(document);
            assert_eq!(
                leak_tally::outstanding(LeakSite::OwnedModuleText),
                0,
                "the last claim released is what frees the copy"
            );

            vilan_core::analyzer::set_base_cache_budget(
                vilan_core::analyzer::BASE_CACHE_DEFAULT_BUDGET,
            );
            vilan_core::analyzer::set_document_overlay(&sibling_path, None);
            let _ = std::fs::remove_dir_all(&dir);
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
    use crate::document::tests::{base_cache_guard, on_big_stack, std_root};
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
            "import std::io::print;\nimport std::option::Option::{{ self, Some, None }};\n\n\
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
        let _cache = base_cache_guard();
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
            "import std::io::print;\n\n\
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
        let _cache = base_cache_guard();
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
        let _cache = base_cache_guard();
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
        let _cache = base_cache_guard();
        fn broken_macro_text(i: usize) -> String {
            format!(
                "import std::io::print;\n\n\
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
    const CONST_HEAVY_BASE: &str = "import std::io::print;\n\n\
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
        let _cache = base_cache_guard();
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
        let _cache = base_cache_guard();
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

    /// The 1-minute load average at report time, or `"?"` — the CLI
    /// harness's provenance twin (backlog M13).
    fn loadavg_1m() -> String {
        std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|text| text.split_whitespace().next().map(str::to_string))
            .unwrap_or_else(|| "?".to_string())
    }

    /// Nearest-rank percentile over sorted samples.
    fn percentile(sorted: &[Duration], fraction: f64) -> f64 {
        let rank = (fraction * sorted.len() as f64).ceil().max(1.0) as usize;
        sorted[rank.min(sorted.len()) - 1].as_secs_f64() * 1000.0
    }

    /// The same `PERF {…}` line shape the CLI harness emits, so a run's rows
    /// from both binaries concatenate into one diffable summary.
    fn report(corpus: &str, note: &str, samples: &mut [Duration]) {
        samples.sort_unstable();
        let milliseconds = |duration: Duration| duration.as_secs_f64() * 1000.0;
        println!(
            "PERF {{\"section\":\"lsp_edit\",\"corpus\":\"{}\",\"mode\":\"warm\",\
             \"metric\":\"analyze\",\"profile\":\"{}\",\"load\":\"{}\",\"runs\":{},\
             \"min_ms\":{:.2},\"median_ms\":{:.2},\"p95_ms\":{:.2},\"p99_ms\":{:.2},\
             \"max_ms\":{:.2},\"note\":\"{}\"}}",
            corpus,
            profile(),
            loadavg_1m(),
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
            "import std::io::print;\nimport std::option::Option::{{ self, Some, None }};\n\n\
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
        let mut measured_subjects: usize = 1;

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
            measured_subjects += 1;
        }

        // The run-provenance row, the CLI harness's twin (backlog M13).
        println!(
            "PERF {{\"section\":\"run\",\"corpus\":\"vilan-lsp\",\"mode\":\"-\",\
             \"metric\":\"provenance\",\"profile\":\"{}\",\"load\":\"{}\",\
             \"runs\":{measured_subjects},\"min_ms\":0.00,\"median_ms\":0.00,\
             \"p95_ms\":0.00,\"p99_ms\":0.00,\"max_ms\":0.00,\
             \"note\":\"{measured_subjects} subjects\"}}",
            profile(),
            loadavg_1m(),
        );
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

/// E111: a capture's name span inside an `is`/`match` pattern — the one span
/// semantic tokens and inlay hints BOTH read (`Variable::name_span`), so a
/// wrong one paints the highlighting and slides the hint together.
///
/// The owner's live site was kolt `interact.vl:129`,
/// `if handler.on_double_click is Some(let (delay, on_double_click))`: every
/// token from the tuple onward sat four characters late. The analyzer used to
/// rebuild the name span as `pattern.start + "let ".len()` under a flag threaded
/// down from the caller. That is right for `Some(let x)`, whose pattern span
/// really does cover the keyword — and wrong by exactly four for a binding
/// reached through a BINDER tuple or array, whose elements carry bare identifier
/// spans. Both shapes are the same `Pattern::Tuple` in the tree, which is why the
/// fix had to move the name span into the AST rather than sharpen the flag.
///
/// These assert ABSOLUTE offsets against the fixture text, so an off-by-any drift
/// fails loudly rather than shifting an expectation along with the bug.
#[cfg(test)]
mod pattern_capture_name_spans {
    use super::tests::std_root;
    use super::*;
    use std::path::Path;

    fn analyze(text: &str) -> Document {
        Document::analyze(text, &std_root(), Path::new("test.vl"))
    }

    /// Every DECLARATION token (a capture's own name), as the text it covers.
    fn declaration_tokens(document: &Document, text: &str) -> Vec<(usize, usize, String)> {
        document
            .semantic_tokens()
            .iter()
            .filter(|(_, kind, modifiers)| {
                matches!(kind, TokenKind::Variable) && modifiers & MODIFIER_DECLARATION != 0
            })
            .map(|(span, _, _)| {
                let range = span.into_range();
                (
                    range.start,
                    range.end,
                    text.get(range.start..range.end)
                        .unwrap_or("<out of bounds>")
                        .to_string(),
                )
            })
            .collect()
    }

    /// The span the fixture text says a capture occupies — the pin's own oracle,
    /// independent of anything the compiler computed.
    fn expected(text: &str, name: &str, after: &str) -> (usize, usize, String) {
        let anchor = text.find(after).expect("the fixture anchor");
        let start = text[anchor..].find(name).expect("the capture name") + anchor;
        (start, start + name.len(), name.to_string())
    }

    /// The bug's exact shape: an `is` pattern whose payload is a TUPLE binder.
    /// Both captures used to land four characters late (`"y, ot"`, `"r)) {"`).
    #[test]
    fn a_tuple_payload_paints_its_captures_on_their_own_names() {
        let text = "fun main() {\n\tlet a: Option<(i32, i32)> = Some((1, 2));\n\tif a is Some(let (delay, other)) {\n\t\tprint(delay);\n\t}\n}\n";
        let document = analyze(text);
        assert_eq!(
            declaration_tokens(&document, text),
            vec![
                expected(text, "a", "let a: Option"),
                expected(text, "delay", "Some(let ("),
                expected(text, "other", "Some(let ("),
            ],
        );
    }

    /// The `mut` spelling of the same binder — `"mut "` is four characters too,
    /// so the old arithmetic drifted here identically rather than differently.
    #[test]
    fn a_mut_tuple_payload_paints_its_captures_on_their_own_names() {
        let text = "fun main() {\n\tlet a: Option<(i32, i32)> = Some((1, 2));\n\tif a is Some(mut (delay, other)) {\n\t\tprint(delay);\n\t}\n}\n";
        let document = analyze(text);
        assert_eq!(
            declaration_tokens(&document, text),
            vec![
                expected(text, "a", "let a: Option"),
                expected(text, "delay", "Some(mut ("),
                expected(text, "other", "Some(mut ("),
            ],
        );
    }

    /// The ARRAY binder, the shape neither the report nor B166/B167 covered: the
    /// analyzer recursed into `Pattern::Array` with the same flag, so it drifted
    /// too. Pinned per case, not by family.
    #[test]
    fn an_array_payload_paints_its_captures_on_their_own_names() {
        let text = "fun main() {\n\tlet a: Option<[i32; 2]> = Some([1, 2]);\n\tif a is Some(let [delay, other]) {\n\t\tprint(delay);\n\t}\n}\n";
        let document = analyze(text);
        assert_eq!(
            declaration_tokens(&document, text),
            vec![
                expected(text, "a", "let a: Option"),
                expected(text, "delay", "Some(let ["),
                expected(text, "other", "Some(let ["),
            ],
        );
    }

    /// A `match` leg reaches `walk_pattern` by a different caller than `is` does,
    /// and drifted the same way — so the leg gets its own pin.
    #[test]
    fn a_match_legs_tuple_payload_paints_its_captures_on_their_own_names() {
        let text = "fun main() {\n\tlet a: Option<(i32, i32)> = Some((1, 2));\n\tmatch a {\n\t\tSome(let (delay, other)) => print(delay),\n\t\tNone => {}\n\t}\n}\n";
        let document = analyze(text);
        assert_eq!(
            declaration_tokens(&document, text),
            vec![
                expected(text, "a", "let a: Option"),
                expected(text, "delay", "Some(let ("),
                expected(text, "other", "Some(let ("),
            ],
        );
    }

    /// The three shapes that were already RIGHT, so the fix is pinned as a
    /// narrowing and not as a swap of one drift for another: a bare payload
    /// (whose pattern span really does cover `let `), a pattern tuple whose
    /// elements each spell their own `let`, and a destructuring `let`.
    #[test]
    fn the_shapes_that_already_painted_correctly_are_unmoved() {
        let bare = "fun main() {\n\tlet a: Option<i32> = Some(3);\n\tif a is Some(let value) {\n\t\tprint(value);\n\t}\n}\n";
        assert_eq!(
            declaration_tokens(&analyze(bare), bare),
            vec![
                expected(bare, "a", "let a: Option"),
                expected(bare, "value", "Some(let "),
            ],
        );

        let nested = "fun main() {\n\tlet a: Option<Option<i32>> = Some(Some(3));\n\tif a is Some(Some(let value)) {\n\t\tprint(value);\n\t}\n}\n";
        assert_eq!(
            declaration_tokens(&analyze(nested), nested),
            vec![
                expected(nested, "a", "let a: Option"),
                expected(nested, "value", "Some(Some(let "),
            ],
        );

        // Each element spells its own `let`, so each element's span DOES cover a
        // keyword — the case the deleted flag existed to serve.
        let legs = "fun main() {\n\tlet a = (1, 2);\n\tmatch a {\n\t\t(let first, let second) => print(first + second),\n\t}\n}\n";
        assert_eq!(
            declaration_tokens(&analyze(legs), legs),
            vec![
                expected(legs, "a", "let a = "),
                expected(legs, "first", "\t\t(let "),
                expected(legs, "second", ", let "),
            ],
        );

        let destructure =
            "fun main() {\n\tlet (first, second) = (1, 2);\n\tprint(first + second);\n}\n";
        assert_eq!(
            declaration_tokens(&analyze(destructure), destructure),
            vec![
                expected(destructure, "first", "let ("),
                expected(destructure, "second", "let ("),
            ],
        );
    }

    /// The hints ride the SAME `name_span`, and a hint is the worse half of the
    /// bug: a slid anchor lands mid-identifier, and the handler's viewport filter
    /// can drop it entirely. Anchored at each capture name's END.
    #[test]
    fn inlay_hints_anchor_at_the_end_of_a_tuple_payloads_capture_names() {
        let text = "fun main() {\n\tlet a: Option<(i32, i32)> = Some((1, 2));\n\tif a is Some(let (delay, other)) {\n\t\tprint(delay);\n\t}\n}\n";
        let document = analyze(text);
        let anchor = |name: &str| {
            let (_, end, _) = expected(text, name, "Some(let (");
            (end, ": i32".to_string())
        };
        let hints = document.inlay_hints();
        assert!(
            hints.contains(&anchor("delay")) && hints.contains(&anchor("other")),
            "hints must sit just past each capture name, not four characters on: \
             {hints:?}",
        );
    }
}

/// E107: member completion inside a BUILDER chain — the owner's kolt
/// `interact.vl` shape, reported as "a `.` on its own line between two existing
/// `.calls` offers nothing".
///
/// The line break was a red herring. The classifier reads member position in
/// token space and has since kolt.local 001, so every trivia shape resolves; the
/// receiver's TYPE was what went missing. `call_result_type_id` resolved a call
/// receiver by reading the callee's DECLARED return type, and kolt's builders —
/// like the language invites — declare none:
///
/// ```text
/// fun on_drag(own self, handler: || DragHandler) { self.on_drag = Some(handler); self }
/// ```
///
/// So `Handler::new().` was already silent, and so was every link after it, on
/// one line or ten. The analyzer had inferred the answer all along and memoized
/// it; it simply never left the analyzer, which is why the fix is one exported
/// field (`Program::inferred_return_types`) rather than a second inference in
/// the IDE.
///
/// Both spellings are pinned side by side in every shape, so a regression that
/// re-narrows this to declared returns fails on the unannotated row alone.
#[cfg(test)]
mod builder_chain_member_completion {
    use super::tests::std_root;
    use super::*;
    use std::path::Path;

    /// kolt's spelling: `own self` builders that return `self` with no return
    /// annotation, plus a field, so a silent answer is distinguishable from a
    /// merely incomplete one.
    const UNANNOTATED: &str = "struct Handler { count: i32 }\n\
         impl Handler {\n\
         \tfun new() { Handler { count = 0 } }\n\
         \tfun on_drag(own self, handler: || void) { self.count = 1; self }\n\
         \tfun on_double_click(own self, handler: || void) { self.count = 2; self }\n\
         }\n";

    /// The same builders with the return type written out — the control that
    /// keeps the pin honest about WHICH half broke.
    const ANNOTATED: &str = "struct Handler { count: i32 }\n\
         impl Handler {\n\
         \tfun new(): Handler { Handler { count = 0 } }\n\
         \tfun on_drag(own self, handler: || void): Handler { self.count = 1; self }\n\
         \tfun on_double_click(own self, handler: || void): Handler { self.count = 2; self }\n\
         }\n";

    /// The labels offered at the `~` cursor in `prelude` + `body`.
    fn labels(prelude: &str, body: &str) -> Vec<String> {
        let source = format!("{prelude}fun main() {{\n{body}}}\n");
        let offset = source.find('~').expect("the pin source needs a `~` cursor");
        let text = source.replace('~', "");
        let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
        document
            .completion(offset)
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    /// Asserts the chain's members are offered, for BOTH spellings of the
    /// builder's return type.
    fn assert_offers_the_builders_members(shape: &str, body: &str) {
        for (spelling, prelude) in [("unannotated", UNANNOTATED), ("annotated", ANNOTATED)] {
            let found = labels(prelude, body);
            for member in ["count", "on_drag", "on_double_click"] {
                assert!(
                    found.contains(&member.to_string()),
                    "{shape} ({spelling} builder) must offer `{member}`: {found:?}",
                );
            }
        }
    }

    /// The reported shape: a `.` alone on a continuation line, with the chain
    /// continuing on the line BELOW it.
    #[test]
    fn a_dot_on_its_own_line_mid_chain_offers_the_receivers_members() {
        assert_offers_the_builders_members(
            "a mid-chain dot on its own line",
            "\tlet h = Handler::new()\n\t\t.on_drag(|| {})\n\t\t.~\n\t\t.on_double_click(|| {});\n",
        );
    }

    /// The same, parenthesized — the way the owner's site wraps the chain.
    #[test]
    fn a_parenthesized_chains_own_line_dot_offers_the_receivers_members() {
        assert_offers_the_builders_members(
            "a parenthesized mid-chain dot",
            "\tlet h = (Handler::new()\n\t\t.on_drag(|| {})\n\t\t.~\n\t\t.on_double_click(|| {}));\n",
        );
    }

    /// The trailing dot, and the dot immediately before the closing paren — the
    /// other two shapes the item names.
    #[test]
    fn a_trailing_dot_and_a_dot_before_the_close_paren_offer_the_receivers_members() {
        assert_offers_the_builders_members(
            "a trailing dot",
            "\tlet h = Handler::new()\n\t\t.on_drag(|| {})\n\t\t.~;\n",
        );
        assert_offers_the_builders_members(
            "a dot before the closing paren",
            "\tlet h = (Handler::new()\n\t\t.on_drag(|| {})\n\t\t.~\n\t);\n",
        );
    }

    /// The same-line control: the report is about a line break, so the pin has
    /// to show the SAME answer with no line break at all — which is how the
    /// investigation learned the break was not the variable.
    #[test]
    fn the_same_line_control_offers_the_same_members() {
        assert_offers_the_builders_members(
            "a same-line chain",
            "\tlet h = Handler::new().on_drag(|| {}).~;\n",
        );
        assert_offers_the_builders_members(
            "a bare constructor call receiver",
            "\tlet h = Handler::new().~;\n",
        );
    }

    /// The owner's literal site: the chain is an ARGUMENT inside an element's
    /// opening tag, one closure deep. The element head is its own cursor-context
    /// world, so a chain nested in a head argument gets its own row.
    #[test]
    fn a_builder_chain_inside_an_element_head_argument_offers_the_receivers_members() {
        for (spelling, prelude) in [("unannotated", UNANNOTATED), ("annotated", ANNOTATED)] {
            let source = format!(
                "import std::ui::view;\n{prelude}fun main() {{\n\
                 \t<div\n\
                 \t\t.on(Handler::new()\n\
                 \t\t\t.on_drag(|| {{}})\n\
                 \t\t\t.~\n\
                 \t\t\t.on_double_click(|| {{}}))\n\
                 \t></div>\n\
                 }}\n"
            );
            let offset = source.find('~').expect("cursor");
            let text = source.replace('~', "");
            let document = Document::analyze(&text, &std_root(), Path::new("test.vl"));
            let found: Vec<String> = document
                .completion(offset)
                .into_iter()
                .map(|completion| completion.label)
                .collect();
            for member in ["count", "on_drag", "on_double_click"] {
                assert!(
                    found.contains(&member.to_string()),
                    "an element-head argument's chain ({spelling}) must offer \
                     `{member}`: {found:?}",
                );
            }
        }
    }

    /// A nested chain inside a closure ARGUMENT of the outer chain — kolt's
    /// `DragHandler::new().on_drag_move(…)` inside `on_drag(|| { … })`. The inner
    /// receiver is a different unannotated builder reached through a closure body.
    #[test]
    fn a_chain_nested_in_a_closure_argument_offers_the_inner_receivers_members() {
        let prelude = "struct Inner { m: i32 }\n\
             impl Inner {\n\
             \tfun new() { Inner { m = 0 } }\n\
             \tfun on_move(own self, handler: || void) { self.m = 1; self }\n\
             \tfun on_end(own self, handler: || void) { self.m = 2; self }\n\
             }\n\
             struct Handler { count: i32 }\n\
             impl Handler {\n\
             \tfun new() { Handler { count = 0 } }\n\
             \tfun on_drag(own self, handler: || Inner) { self.count = 1; self }\n\
             }\n";
        let found = labels(
            prelude,
            "\tlet h = Handler::new()\n\t\t.on_drag(|| {\n\t\t\tInner::new()\n\t\t\t\t.on_move(|| {})\n\t\t\t\t.~\n\t\t\t\t.on_end(|| {})\n\t\t});\n",
        );
        for member in ["m", "on_move", "on_end"] {
            assert!(
                found.contains(&member.to_string()),
                "the INNER builder's `{member}`: {found:?}",
            );
        }
        assert!(
            !found.contains(&"on_drag".to_string()),
            "…and not the outer chain's: {found:?}",
        );
    }

    /// The inferred return type must be the FUNCTION's, never a caller's
    /// specialization: the exported record is keyed by function alone, so a
    /// generic builder is the case that would expose a leak between call sites.
    #[test]
    fn an_unannotated_generic_returns_its_own_type_not_a_callers() {
        let prelude = "struct Wrapper<T> { value: T }\n\
             struct Point { x: i32 }\n\
             impl Point { fun twin(self): Point { self } }\n\
             fun wrap<T>(value: T) { Wrapper { value = value } }\n";
        let found = labels(prelude, "\tlet w = wrap(Point { x = 1 });\n\twrap(1).~;\n");
        assert!(
            found.contains(&"value".to_string()),
            "the wrapper's own field: {found:?}",
        );
        assert!(
            !found.contains(&"twin".to_string()) && !found.contains(&"x".to_string()),
            "nothing from the OTHER call site's specialization: {found:?}",
        );
    }

    // --- E130: the declared return IS a type parameter (E107's other half) ---
    //
    // E107 covered the callee with NO declared return. This is the callee whose
    // declared return is a bare `T`: the declaration is there, so the
    // `inferred_return_types` fallback is never reached, and `T`'s own TypeId
    // is a `Type::Generic` that names no nominal at all. The receiver's own
    // type argument is what says which type `T` is at THIS call, and the
    // analyzer recorded it.

    /// The item's user-code reduction, with no std type involved: a generic
    /// `Box<T>` whose `get` returns the bare parameter and whose `wrap` returns
    /// a nominal head over it.
    const GENERIC_BOX: &str = "import std::option::Option::{ self, Some, None };\n\
         struct Box<T> {\n\tv: T,\n}\n\
         impl Box<type T> {\n\
         \tfun get(self): T { self.v }\n\
         \tfun wrap(self): Option<T> { Some(self.v) }\n\
         }\n\
         struct Point { x: i32, y: i32 }\n\
         impl Point { fun twin(self): Point { self } }\n";

    #[test]
    fn a_call_returning_a_bare_type_parameter_offers_the_bound_types_members() {
        let found = labels(
            GENERIC_BOX,
            "\tlet b: Box<Point> = Box { v = Point { x = 1, y = 2 } };\n\tb.get().~;\n",
        );
        assert!(
            found.contains(&"x".to_string()) && found.contains(&"twin".to_string()),
            "`get(): T` on a `Box<Point>` answers Point's members: {found:?}",
        );
        assert!(
            !found.contains(&"get".to_string()) && !found.contains(&"v".to_string()),
            "the RESULT's members, not the box's: {found:?}",
        );
    }

    /// A FREE generic function whose return is its own parameter — the other
    /// channel the bindings arrive through, with no impl subject in sight.
    #[test]
    fn a_free_generic_call_returning_its_own_parameter_substitutes_too() {
        let prelude = "struct Point { x: i32, y: i32 }\n\
             impl Point { fun twin(self): Point { self } }\n\
             fun echo<T>(value: T): T { value }\n";
        for body in [
            "\techo(Point { x = 1, y = 2 }).~;\n",
            "\techo<Point>(Point { x = 1, y = 2 }).~;\n",
        ] {
            let found = labels(prelude, body);
            assert!(
                found.contains(&"x".to_string()) && found.contains(&"twin".to_string()),
                "`echo<T>(value: T): T` at a Point call answers Point's members \
                 ({body:?}): {found:?}",
            );
        }
    }

    /// The control the item names: the SAME impl's `wrap(): Option<T>` has a
    /// nominal head written in the declaration and has always answered, so a
    /// red here says the fix broke the path it was built beside.
    #[test]
    fn a_call_returning_a_nominal_over_a_parameter_still_answers() {
        let found = labels(
            GENERIC_BOX,
            "\tlet b: Box<Point> = Box { v = Point { x = 1, y = 2 } };\n\tb.wrap().~;\n",
        );
        assert!(
            found.contains(&"unwrap_or".to_string()),
            "`wrap(): Option<T>` answers Option's members: {found:?}",
        );
    }

    /// The reported shape, on std's own reactive cell: `SignalCell<T>::get`
    /// declares `T`, and the owner's buffer is `c.get().` on a
    /// `SignalCell<List<str>>`.
    #[test]
    fn a_signal_cells_get_offers_the_held_types_members() {
        let found = labels(
            "import std::reactive::SignalCell;\n",
            "\tlet c: SignalCell<List<str>> = SignalCell::new(List::new());\n\tc.get().~;\n",
        );
        assert!(
            found.contains(&"len".to_string()) && found.contains(&"push".to_string()),
            "`SignalCell<List<str>>::get()` answers List's members: {found:?}",
        );
    }
}

/// E131: a member request answered while the buffer is AHEAD of the analysis
/// resolves its receiver from the LIVE text, not from analyzed coordinates.
///
/// The owner's report: base text `<div .styled(base_style)>`, one un-landed
/// change to `<div .styled(const style::style().)>`, a request inside the
/// 150 ms debounce — and the popup offered `element, text, class, styled,
/// style_var, attr, on, child, bind_text, …`, which is `View`'s method set.
/// Settled, the same position offers Style's 89.
///
/// The cause was a coordinate one, and E125 fixed its twin for
/// `semanticTokens/range`. `receiver_nominal_id`'s complex arm did
/// `to_analyzed_offset(receiver_end - 1)` and then `entity_at`, and
/// `to_analyzed_offset` is a line/character round-trip that CLAMPS: it repairs
/// other lines (E52), but the cursor's own line is ALWAYS an edited line, so
/// the live column clamped back onto the analyzed line's last character and
/// `entity_at` answered with the enclosing `.styled(..)`/element — typed
/// `View`. The keystroke layer could not catch it either: `keystroke_verdict`
/// is `Exact` because `shape_stamp` skips `fun` bodies, and `Exact` is the
/// anchor's claim about TOKEN positions, while the anchor is line-aligned and
/// the cursor's line is inside its window by construction.
///
/// Both halves are here. The receiver's IDENTITY now comes from the live token
/// stream — a bare name, a call, a method call, a field — with only NAMES
/// resolved against the landed program; and what that walk cannot type falls
/// back to the analyzed arm only where the analyzed text still describes the
/// receiver's own bytes. Inside the edit window it declines, because a wrong
/// list is worse than no list and the next settled request is right (Q4).
#[cfg(test)]
mod stale_receiver_member_completion {
    use super::tests::analyze_workspace;
    use super::*;

    /// kolt's own manifest shape: the web prelude puts `style`, `View` and the
    /// element vocabulary in scope with no import, which is what makes the
    /// reported buffer the buffer it is.
    const MANIFEST: &str = "[package]\nname = \"probe\"\nprelude = \"std::web\"\n\n\
         [entry.main]\ntarget = \"browser\"\n";

    /// The LANDED text: the attribute holds a plain binding.
    const BASE: &str = "fun app(): View {\n\t<div .styled(base_style)>\n\t\t\"hi\"\n\t</div>\n}\n\n\
         let base_style = const style::style();\nlet styles = [const style::style()];\n";

    fn labels(document: &Document, offset: usize) -> Vec<String> {
        document
            .completion(offset)
            .into_iter()
            .map(|completion| completion.label)
            .collect()
    }

    /// The labels at the `~` cursor in `live`, with `BASE` analyzed and `live`
    /// applied as an UN-LANDED edit — a request inside the debounce.
    fn stale(live: &str) -> Vec<String> {
        let text = live.replace('~', "");
        let offset = live.find('~').expect("the pin source needs a `~` cursor");
        let (directory, mut document) =
            analyze_workspace(&[("main.vl", BASE), ("vilan.toml", MANIFEST)]);
        document.set_text(&text);
        assert_eq!(
            document.keystroke_verdict(false),
            crate::keystroke::Verdict::Exact,
            "the reported request is one the keystroke layer calls Exact — that is \
             the half of the report the gate exists for",
        );
        let found = labels(&document, offset);
        let _ = std::fs::remove_dir_all(&directory);
        found
    }

    /// The same position with the analysis LANDED on the same text — what the
    /// request answers a moment later, and the answer the stale one must not
    /// contradict.
    fn settled(live: &str) -> Vec<String> {
        let text = live.replace('~', "");
        let offset = live.find('~').expect("the pin source needs a `~` cursor");
        let (directory, document) =
            analyze_workspace(&[("main.vl", &text), ("vilan.toml", MANIFEST)]);
        let found = labels(&document, offset);
        let _ = std::fs::remove_dir_all(&directory);
        found
    }

    /// Style members that are not View members, and vice versa — the two lists
    /// the report is about, named so a failure says WHICH one came back.
    fn assert_style_and_not_view(found: &[String], what: &str) {
        for member in ["flex_direction", "padding", "gap"] {
            assert!(
                found.contains(&member.to_string()),
                "{what} must offer Style's `{member}`: {found:?}",
            );
        }
        for member in ["child", "bind_text", "style_var"] {
            assert!(
                !found.contains(&member.to_string()),
                "{what} must not offer View's `{member}`: {found:?}",
            );
        }
    }

    const CALL_RECEIVER: &str = "fun app(): View {\n\t<div .styled(const style::style().~)>\n\t\t\"hi\"\n\t</div>\n}\n\n\
         let base_style = const style::style();\nlet styles = [const style::style()];\n";

    /// The report, verbatim.
    #[test]
    fn a_stale_call_receiver_answers_the_live_expressions_type() {
        assert_style_and_not_view(&stale(CALL_RECEIVER), "a stale call receiver");
    }

    /// …and it is the SAME answer the settled request gives, which is the
    /// property the whole item is about.
    #[test]
    fn the_settled_call_receiver_answers_what_the_stale_one_does() {
        let settled = settled(CALL_RECEIVER);
        assert_style_and_not_view(&settled, "the settled call receiver");
        assert_eq!(
            settled,
            stale(CALL_RECEIVER),
            "the stale request and the settled one answer one list",
        );
    }

    /// The item's own isolation: a BARE-NAME receiver was always right, because
    /// that arm reads the name off the live text. It stays right.
    #[test]
    fn a_stale_bare_name_receiver_still_answers() {
        assert_style_and_not_view(
            &stale(
                "fun app(): View {\n\t<div .styled(base_style.~)>\n\t\t\"hi\"\n\t</div>\n}\n\n\
                 let base_style = const style::style();\nlet styles = [const style::style()];\n",
            ),
            "a stale bare-name receiver",
        );
    }

    /// A METHOD call typed since the landing — the `x.m(…)` shape of the live
    /// walk, where the sub-receiver is itself resolved live.
    #[test]
    fn a_stale_method_call_receiver_answers_the_methods_return() {
        assert_style_and_not_view(
            &stale(
                "fun app(): View {\n\t<div .styled(base_style.padding(space(4)).~)>\n\t\t\"hi\"\n\t</div>\n}\n\n\
                 let base_style = const style::style();\nlet styles = [const style::style()];\n",
            ),
            "a stale method-call receiver",
        );
    }

    const INDEX_RECEIVER: &str = "fun app(): View {\n\t<div .styled(styles[0].~)>\n\t\t\"hi\"\n\t</div>\n}\n\n\
         let base_style = const style::style();\nlet styles = [const style::style()];\n";

    /// The GATE. An index is a shape the live walk does not type, and the
    /// analyzed arm behind it would answer about whatever used to be written on
    /// this line — `View`'s members, the report's own wrong list. Inside the
    /// edit window it declines instead.
    #[test]
    fn a_stale_receiver_the_live_walk_cannot_type_declines_rather_than_guessing() {
        let found = stale(INDEX_RECEIVER);
        assert!(
            found.is_empty(),
            "an unresolvable stale receiver offers nothing rather than the old \
             expression's members: {found:?}",
        );
    }

    /// …and the gate is not a permanent refusal: the same position, settled,
    /// answers through the analyzed arm exactly as it always did.
    #[test]
    fn the_gate_lifts_the_moment_the_analysis_lands() {
        assert_style_and_not_view(&settled(INDEX_RECEIVER), "the settled index receiver");
    }
}

/// E124's paint, at both granularities (`proposal/dead-code-paint.md`).
///
/// These are the pins that need a MANIFEST — the module-level slice, the
/// library rule and the `generated` root are all manifest facts, and the
/// package union is a fact about several files at once, so none of them can be
/// pinned on a single source string the way E114's three producers are. The
/// definition's own pins (the exemptions, the type-level narrowing, the
/// trait-impl class) live in `vilan-core/tests/dead_items.rs`.
#[cfg(test)]
mod dead_item_paint_tests {
    use super::tests::std_root;
    use super::*;
    use std::sync::Arc;
    use vilan_core::cancel::CancelToken;

    /// The kolt shape, shrunk: two entries, a module both of them load, and a
    /// module neither does.
    const MANIFEST: &str = "[package]\nname = \"app\"\ndefault-entry = \"server\"\n\n[entry.client]\n\n[entry.server]\n";
    const CLIENT: &str =
        "import pkg::shared::used_by_client;\n\nfun main() {\n\tused_by_client();\n}\n";
    const SERVER: &str =
        "import pkg::shared::used_by_server;\n\nfun main() {\n\tused_by_server();\n}\n";
    const SHARED: &str = "import std::io::print;\n\n\
         let read_by_client: i32 = 1;\n\n\
         let read_by_nobody: i32 = 2;\n\n\
         fun used_by_client() {\n\tprint(i\"{read_by_client}\");\n}\n\n\
         fun used_by_server() {\n\tprint(\"s\");\n}\n\n\
         fun used_by_nobody() {\n\tprint(\"n\");\n}\n";
    const ORPHAN: &str = "import std::io::print;\n\n\
         let orphan_binding: i32 = 7;\n\n\
         fun orphan_fun() {\n\tprint(\"o\");\n}\n";

    fn workspace(name: &str, files: &[(&str, &str)]) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("vilan-e124-{name}-{}-{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        for (relative, contents) in files {
            let path = directory.join(relative);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("a scratch directory");
            }
            std::fs::write(path, contents).expect("a fixture file");
        }
        directory
    }

    /// The package's own two-entry fixture, on disk.
    fn package(name: &str) -> PathBuf {
        workspace(
            name,
            &[
                ("vilan.toml", MANIFEST),
                ("src/client.vl", CLIENT),
                ("src/server.vl", SERVER),
                ("src/shared.vl", SHARED),
                ("src/orphan.vl", ORPHAN),
            ],
        )
    }

    fn open(directory: &Path, relative: &str) -> Document {
        let path = directory.join(relative);
        let text = std::fs::read_to_string(&path).expect("the fixture file");
        let document = Document::analyze(&text, &std_root(), &path);
        assert!(
            document.diagnostics.is_empty(),
            "{relative} analyzes cleanly: {:?}",
            document
                .diagnostics
                .iter()
                .map(|e| &e.msg)
                .collect::<Vec<_>>(),
        );
        document
    }

    /// The package's union, computed the way the server's clock computes it —
    /// from disk, since these fixtures have no buffers.
    fn union(directory: &Path) -> Option<Arc<crate::dead_items::PackageReach>> {
        let entries = crate::dead_items::entry_paths(directory)?;
        crate::dead_items::compute(&entries, &std_root(), 0, &CancelToken::new(), |path| {
            std::fs::read_to_string(path).ok()
        })
        .map(Arc::new)
    }

    /// The source text each faded span covers.
    fn named(document: &Document, spans: &[Span]) -> Vec<String> {
        let text = document.analyzed_text();
        let mut names: Vec<String> = spans
            .iter()
            .map(|span| text[span.start..span.end].to_string())
            .collect();
        names.sort();
        names
    }

    /// **A module no entry loads is faded whole**, and the message names the
    /// entries that were asked — the free slice (§2.5), which rides the
    /// per-entry module walk the analysis already paid for.
    #[test]
    fn a_module_no_entry_loads_is_faded_whole() {
        let directory = package("orphan");
        let document = open(&directory, "src/orphan.vl");
        let (message, spans) = document
            .unloaded_module_paint()
            .expect("no entry loads `orphan.vl`");
        assert_eq!(
            named(&document, &spans),
            vec!["orphan_binding".to_string(), "orphan_fun".to_string()],
            "both top-level items of the unloaded module fade",
        );
        assert!(
            message.contains("`client`") && message.contains("`server`"),
            "the message names the entries that were asked: {message}",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The other direction: a module an entry DOES load is not faded whole,
    /// whatever is unused inside it. Without this the pin above would pass on a
    /// paint that fades everything.
    #[test]
    fn a_module_an_entry_loads_is_never_faded_whole() {
        let directory = package("loaded");
        for relative in ["src/shared.vl", "src/client.vl", "src/server.vl"] {
            let document = open(&directory, relative);
            assert!(
                document.unloaded_module_paint().is_none(),
                "{relative} is loaded by an entry",
            );
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// **The union is a union** (pin 12): an item reached by exactly one of the
    /// two entries is not gray, and only the item neither reaches is.
    ///
    /// This is also the NO-MAIN pin (13): `shared.vl` has no `main`, so its own
    /// analysis has no root to walk from — every term of the union comes from
    /// the package clock, which is the finding that reframed the ruling.
    #[test]
    fn an_item_no_entry_reaches_is_faded_and_one_a_single_entry_reaches_is_not() {
        let directory = package("union");
        let mut document = open(&directory, "src/shared.vl");
        assert!(
            vilan_core::platform_color::paint_reachable_nodes(
                document.program.as_ref().expect("a program")
            )
            .is_none(),
            "`shared.vl` analyzed as its own entry has no `main` — the premise",
        );
        document.set_package_reach(union(&directory));
        assert_eq!(
            named(&document, &document.dead_item_spans()),
            vec!["read_by_nobody".to_string(), "used_by_nobody".to_string()],
            "`used_by_client` and `used_by_server` are each reached by exactly \
             ONE entry and live in the union; only what neither reaches grays",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// **Withdrawal** (pin 15, determination 8): with no union in hand the
    /// paint is silent. This is the state every edit puts the package into, and
    /// it is why a gray can never be served stale toward MORE grays — there is
    /// nothing to serve.
    #[test]
    fn the_paint_is_silent_until_the_package_union_lands() {
        let directory = package("withdrawn");
        let mut document = open(&directory, "src/shared.vl");
        assert!(
            document.dead_item_spans().is_empty(),
            "no union, no gray — the withdrawn state",
        );
        document.set_package_reach(union(&directory));
        assert!(
            !document.dead_item_spans().is_empty(),
            "the union lands and the gray returns",
        );
        document.set_package_reach(None);
        assert!(
            document.dead_item_spans().is_empty(),
            "withdrawing takes it off again, with no analysis in between",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// **A broken parse anywhere in the package suppresses the package's
    /// top-level grays** (pin 17), not merely its own file's. A salvaged parse
    /// can lose a whole block or the file's entire tail, and a smaller program
    /// reads to a reachability walk as a deader one.
    #[test]
    fn a_broken_module_anywhere_refuses_the_whole_union() {
        let directory = package("broken");
        std::fs::write(
            directory.join("src/shared.vl"),
            format!("{SHARED}\nfun half_typed(: {{\n"),
        )
        .expect("break the module");
        assert!(
            union(&directory).is_none(),
            "one broken module in the package refuses the union outright",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// **A `[library]`'s top-level items are never gray** (pins 18/19,
    /// determination 9). A library has no entries — validation refuses them
    /// outright — so there is no union and nothing to say. Every top-level item
    /// is surface a consumer may import, and that is the property that saves a
    /// consumer from forking an under-exported package.
    #[test]
    fn a_library_module_is_never_gray_at_either_granularity() {
        let directory = workspace(
            "library",
            &[
                ("vilan.toml", "[library]\nname = \"lib\"\n"),
                (
                    "src/lib.vl",
                    "import std::io::print;\n\nfun exported() {\n\tprint(\"e\");\n}\n",
                ),
                ("src/aside.vl", ORPHAN),
            ],
        );
        for relative in ["src/lib.vl", "src/aside.vl"] {
            let mut document = open(&directory, relative);
            assert!(
                document.unloaded_module_paint().is_none(),
                "{relative}: a library module is never faded whole",
            );
            document.set_package_reach(union(&directory));
            assert!(
                document.dead_item_spans().is_empty(),
                "{relative}: a library item is never gray",
            );
        }
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// **Nothing under a declared `generated` root is gray** (pin 9,
    /// determination 3). The key already exists, kolt already sets it for
    /// lucide, and it already means "this is not code you maintain": on kolt
    /// the paint as ruled would have faded an 18,198-line machine-written
    /// module wall to wall, 1,815 of its 1,820 items, forever.
    #[test]
    fn a_module_under_a_declared_generated_root_is_never_gray() {
        let directory = workspace(
            "generated",
            &[
                (
                    "vilan.toml",
                    "[package]\nname = \"app\"\ngenerated = \"src/icons\"\n\n[entry.server]\n",
                ),
                (
                    "src/server.vl",
                    "import std::io::print;\n\nfun main() {\n\tprint(\"s\");\n}\n",
                ),
                ("src/icons/lib.vl", ORPHAN),
            ],
        );
        let mut document = open(&directory, "src/icons/lib.vl");
        assert!(
            document.unloaded_module_paint().is_none(),
            "a generated module is not faded whole even though no entry loads it",
        );
        document.set_package_reach(union(&directory));
        assert!(
            document.dead_item_spans().is_empty(),
            "and none of its items is grayed either",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The classic single-entry form (`[package] entry = …`) answers the same
    /// way as the `[entry.<name>]` form — pin 14's shape: the two manifests
    /// produce the same grays for an item only one of them reaches.
    #[test]
    fn a_single_entry_package_answers_at_both_granularities() {
        let directory = workspace(
            "single",
            &[
                ("vilan.toml", "[package]\nname = \"app\"\n"),
                (
                    "src/main.vl",
                    "import pkg::shared::used_by_client;\n\nfun main() {\n\tused_by_client();\n}\n",
                ),
                ("src/shared.vl", SHARED),
                ("src/orphan.vl", ORPHAN),
            ],
        );
        let orphan = open(&directory, "src/orphan.vl");
        let (_, spans) = orphan
            .unloaded_module_paint()
            .expect("the single entry does not load `orphan.vl`");
        assert_eq!(
            named(&orphan, &spans),
            vec!["orphan_binding".to_string(), "orphan_fun".to_string()],
        );
        let mut shared = open(&directory, "src/shared.vl");
        shared.set_package_reach(union(&directory));
        assert_eq!(
            named(&shared, &shared.dead_item_spans()),
            vec![
                "read_by_nobody".to_string(),
                "used_by_nobody".to_string(),
                "used_by_server".to_string(),
            ],
            "with one entry, what the other entry used to reach grays too",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }
}
