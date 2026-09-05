//! The Vilan compiler as a library: lexing, parsing, semantic analysis, and JS
//! code generation. Both the `vilan` CLI and the `vilan-lsp` language server are
//! thin front-ends over this crate.

pub mod analyzer;
pub mod async_infer;
pub mod bindgen;
pub mod call_graph;
pub mod cancel;
pub mod chunks;
pub mod closest_name;
pub mod const_eval;
pub mod context;
pub mod css;
pub mod dead_items;
pub(crate) mod depth_stats;
pub mod dispatch_refine;
pub mod drop_plan_stats;
pub mod elements;
pub mod error;
pub mod formatter;
pub mod fx;
pub mod git_dep;
pub mod id;
pub mod impl_select;
pub mod init_order;
pub mod interpreter;
pub mod leak_tally;
pub mod lexing;
pub mod lift;
pub(crate) mod macros;
pub mod manifest;
pub mod node;
pub mod options;
pub mod owned_modules;
pub mod parsing;
pub mod platform_color;
pub mod span;
pub mod target;
pub mod token;
pub mod transformer;
pub mod type_;
pub mod util;

// The common pipeline + core types, re-exported for convenience.
pub use analyzer::{EntryMode, Layer, PackageSpec, PreludeRepair, Program, Workspace, analyze};
pub use error::Error;
pub use macros::MacroLimits;
#[doc(hidden)]
pub use macros::macro_world_cache_clear;
pub use manifest::Manifest;
pub use options::{BuildOptions, Preset};
pub use owned_modules::OwnedModules;
pub use span::{Span, Spanned};
pub use target::{Backend, Platform, PlatformPattern};
pub use transformer::{
    EmittedChunk, JsProgram, SplitProgram, transform, transform_split, transform_split_with_plan,
    transform_to_ast,
};

use std::collections::{HashMap, HashSet};
use std::path::Path;

use node::{Func, ImportBranch, ImportTail, Node, NodeList};
use target::PlatformPattern as Pattern;

/// Infers a build platform for editor analysis (which has no `--platform`) from a
/// file's top-level imports. Evidence, per `import std::<module>` reference:
///
/// - a module served ONLY by a browser layer (`std::dom`) is browser evidence —
///   the file cannot mean anything else;
/// - a module served by a browser layer AND another root — a platform TWIN,
///   like `std::ui` — is evidence only through the NAMES imported from it: a
///   name declared by just the browser twin (`mount`) says browser, one
///   declared by just the other side (`render`) says process, and a name both
///   declare says nothing. B36: the old rule read *any* `std::ui` import as
///   browser evidence, so a two-entry package's shared file importing the
///   process twin's `render` analyzed as browser in the editor and its import
///   red-flagged, while `vilan build` was clean on every entry.
///
/// Any browser evidence wins (the old bias, kept for a file whose imports
/// contradict each other); otherwise Node, whose layer set serves the process
/// twins. Layer directories are read from `std`'s manifest, not a hardcoded
/// list.
fn infer_platform(root: &NodeList, std: &PackageSpec) -> Platform {
    let Some(browser_root) = std
        .layers
        .iter()
        .find(|layer| layer.patterns.iter().any(|p| matches!(p, Pattern::Browser)))
        .map(|layer| layer.root.as_path())
    else {
        return Platform::default();
    };
    // Every root that could serve a module to a NON-browser build: the other
    // layers, then the base.
    let other_roots: Vec<&Path> = std
        .layers
        .iter()
        .filter(|layer| !layer.patterns.iter().any(|p| matches!(p, Pattern::Browser)))
        .map(|layer| layer.root.as_path())
        .chain(std::iter::once(std.base_root.as_path()))
        .collect();
    // The module file `name` resolves to under `root` (`name.vl` or `name/lib.vl`).
    fn module_file(root: &Path, name: &str) -> Option<std::path::PathBuf> {
        let file = root.join(format!("{name}.vl"));
        if file.exists() {
            return Some(file);
        }
        let lib = root.join(name).join("lib.vl");
        lib.exists().then_some(lib)
    }
    // Whether the module at `path` declares `name` at its top level (through
    // the item wrappers). A file that fails to read or parse declares nothing —
    // inference is a heuristic and must never error.
    fn declares(path: &Path, name: &str) -> bool {
        fn node_declares(node: &Node, name: &str) -> bool {
            match node {
                Node::Export(inner) | Node::Derive(_, inner) | Node::Service(_, inner) => {
                    node_declares(&inner.0, name)
                }
                Node::Func(function) => function.name.0 == name,
                Node::Struct(declared, ..)
                | Node::Enum(declared, ..)
                | Node::Trait(declared, ..)
                | Node::Let(declared, ..) => declared.0 == name,
                Node::Module(declared, _) => *declared == name,
                _ => false,
            }
        }
        let Ok(source) = util::read_source(path) else {
            return false;
        };
        let Some((tree, _)) = parse_clean_cached(&source) else {
            return false;
        };
        tree.0.iter().any(|node| node_declares(&node.0, name))
    }
    // The names an import branch takes from its module: the immediate segment
    // of each leaf path (`render`, or `Option` of `Option::{ self, Some }`) —
    // the identifier the module must declare at its top level.
    fn leaf_names<'a>(branch: &'a ImportBranch, into: &mut Vec<&'a str>) {
        match branch {
            ImportBranch::Path(name, _, _) => into.push(name),
            ImportBranch::Set(branches) => {
                for branch in branches {
                    leaf_names(branch, into);
                }
            }
        }
    }
    // Browser evidence for one `std::<module>` reference (see the doc comment).
    fn child_is_browser_evidence(
        branch: &ImportBranch,
        browser_root: &Path,
        other_roots: &[&Path],
    ) -> bool {
        match branch {
            ImportBranch::Path(module, _, sub) => {
                let Some(browser_file) = module_file(browser_root, module) else {
                    return false;
                };
                let twin_files: Vec<std::path::PathBuf> = other_roots
                    .iter()
                    .filter_map(|root| module_file(root, module))
                    .collect();
                if twin_files.is_empty() {
                    // Browser-exclusive — the module itself is the evidence.
                    return true;
                }
                // A twin: only a name the browser side alone declares says
                // browser. A bare `import std::ui;` names nothing — neutral,
                // and so is an aliased one (`import std::ui as u;`), which
                // takes the module and no name out of it.
                let ImportTail::Continue(sub) = sub else {
                    return false;
                };
                let mut names = Vec::new();
                leaf_names(sub, &mut names);
                names.iter().any(|name| {
                    declares(&browser_file, name)
                        && !twin_files.iter().any(|file| declares(file, name))
                })
            }
            ImportBranch::Set(branches) => branches
                .iter()
                .any(|branch| child_is_browser_evidence(branch, browser_root, other_roots)),
        }
    }
    let imports_browser_layer = |branch: &ImportBranch| {
        matches!(branch, ImportBranch::Path("std", _, ImportTail::Continue(child))
            if child_is_browser_evidence(child, browser_root, &other_roots))
    };
    // Imports are block-scoped statements (backlog H2), so scan at every depth —
    // a browser import inside a function body flags the file too.
    fn any_node(nodes: &NodeList, matches: &mut dyn FnMut(&Node) -> bool) -> bool {
        fn walk(node: &Spanned<Node>, matches: &mut dyn FnMut(&Node) -> bool) -> bool {
            if matches(&node.0) {
                return true;
            }
            let mut found = false;
            node.0
                .for_each_child(&mut |child| found = found || walk(child, matches));
            found
        }
        nodes.iter().any(|node| walk(node, matches))
    }
    let references_browser = any_node(root, &mut |node| match node {
        Node::Import(branch) | Node::Use(branch) => imports_browser_layer(branch),
        _ => false,
    });
    if references_browser {
        Platform::Browser
    } else {
        Platform::default()
    }
}

/// [`parse_clean_cached`]'s store: clean parses by content hash. At module
/// scope (rather than local to the function, as it began) only so
/// [`parse_clean_cache_clear`] can reach it.
static PARSE_CLEAN_CACHE: std::sync::OnceLock<
    std::sync::Mutex<HashMap<u64, (&'static Spanned<node::NodeList<'static>>, &'static str)>>,
> = std::sync::OnceLock::new();
/// Content hashes known NOT to parse clean — so a broken file (an entry
/// mid-edit under `--watch`, say) is leaked and re-parsed once per distinct
/// content, not once per round.
static PARSE_CLEAN_BROKEN: std::sync::OnceLock<std::sync::Mutex<HashSet<u64>>> =
    std::sync::OnceLock::new();

/// Drops every entry in [`parse_clean_cached`]'s two process-global maps — the
/// clean-parse store and its known-broken set — so the next compile re-lexes
/// and re-parses every module it loads (backlog M6: without this, the perf
/// harness's in-process "cold" was world-cold but parse-warm, and the true
/// first-compile shape was unmeasurable in-process).
///
/// **The memory stays leaked.** The cache's values are `Box::leak`ed `'static`
/// sources and ASTs, and clearing the maps drops the *pointers*, never the
/// allocations — analyzed programs that borrowed them may still be alive, and
/// the leak tally's records stand. So this is a measurement/test surface, not
/// a memory reclaim; it sits beside [`analyzer::base_cache_clear`] and
/// [`macro_world_cache_clear`], the harness's other two cold switches.
#[doc(hidden)]
pub fn parse_clean_cache_clear() {
    if let Some(cache) = PARSE_CLEAN_CACHE.get() {
        cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
    if let Some(broken) = PARSE_CLEAN_BROKEN.get() {
        broken
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }
}

/// A process-global, content-addressed cache of clean parses, shared by every
/// compile in the process — the CLI's long-lived `--watch` loop, the language
/// server, the test harness. The key is a hash of the source; the value is the
/// leaked `'static` AST (already lift-rewritten, so callers must not lift again)
/// and its leaked source text. Returns `None` when the source is not perfectly
/// clean, so the caller falls back to its rich-diagnostic pipeline — an erroring
/// file is not the hot path.
///
/// This is the same mechanism [`analyzer::load_package_module`] uses to reuse
/// `std` and package modules, lifted so the **entry** file — the one file the
/// CLI parses directly — shares it too. Across watch rounds an unchanged leg's
/// entry (and every unchanged module) is served from the cache instead of being
/// re-lexed and re-parsed (backlog E12). Keying on content (never mtime) keeps
/// it correct: an edited file hashes differently and is parsed afresh; only
/// byte-identical content is reused. A cache hit returns the identical `'static`
/// pointer it stored, which is how a test proves reuse without timing.
pub fn parse_clean_cached(
    source: &str,
) -> Option<(&'static Spanned<node::NodeList<'static>>, &'static str)> {
    let cache = PARSE_CLEAN_CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
    let broken = PARSE_CLEAN_BROKEN.get_or_init(|| std::sync::Mutex::new(HashSet::new()));

    let key = content_hash(source);
    // Recover from poisoning rather than propagate it (backlog E97): this map is
    // process-global and the long-lived surfaces CATCH panics (the four fences),
    // so a propagated poison would outlive the request that caused it and answer
    // "the compiler panicked" for the rest of the session. Every value here is a
    // fully-built `'static` pointer inserted in one step, so a recovered guard
    // cannot hand back a half-written entry — the worst a panic can do is leave
    // an entry absent, which re-parses.
    if let Some(cached) = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key)
    {
        return Some(*cached);
    }
    // Same posture: a poisoned known-broken set must not wedge the session, and
    // a `u64` key set has no torn state to observe.
    if broken
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(&key)
    {
        return None;
    }

    // Cache miss: leak the source so the parsed tree (which borrows it) can live
    // for the whole process, then parse. A non-clean source yields `None` — the
    // caller re-parses it for real diagnostics (leaking the source first mirrors
    // `load_package_module`, whose rich path also reuses the leaked text).
    let leaked: &'static str = Box::leak(source.to_string().into_boxed_str());
    leak_tally::record(leak_tally::LeakSite::ParseCleanCacheText, leaked.len());
    // The handwritten frontend always returns a (possibly recovered) tree; a
    // source is "clean" — and cacheable — exactly when it produced no diagnostics.
    let (tree, errors) = parsing::parse(leaked);
    let Some(mut root) = tree.filter(|_| errors.is_empty()) else {
        // Recovering (E97): the insert is one step over a `Copy` key, so a
        // recovered guard sees a well-formed set either way.
        broken
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(key);
        return None;
    };
    css::rewrite_items(&mut root.0, leaked);
    elements::rewrite_items(&mut root.0, leaked);
    lift::rewrite_items(&mut root.0);
    let leaked_root: &'static Spanned<node::NodeList<'static>> = Box::leak(Box::new(root));
    leak_tally::record(
        leak_tally::LeakSite::ParseCleanCacheAst,
        std::mem::size_of_val(leaked_root),
    );
    // Recovering (E97): the tree and its text are both leaked and complete
    // BEFORE the lock is taken, so the entry is whole or absent, never torn.
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, (leaked_root, leaked));
    Some((leaked_root, leaked))
}

/// The content hash the compiler keys its caches and source fingerprints on —
/// one definition, so the parse cache and the watch loop's per-leg source
/// verification can never disagree about what "same content" means.
pub fn content_hash(text: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// [`content_hash`] over bytes — what a build input that is not text is keyed
/// on (`asset::bundle`'s files: a font, a favicon, a `.wasm`). Deliberately
/// NOT comparable with `content_hash` of the same file decoded: the two are
/// change detectors for different channels and are never mixed in one map.
pub fn content_hash_bytes(bytes: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// The handle to an entry tree `analyze_source_reclaimable` leaked: the
/// `Program` it returned borrows this tree for `'static`, and the owner that
/// drops that program may give the tree back (`Leaked::reclaim`, `unsafe`,
/// its contract there). Dropping the handle keeps the leak.
pub type LeakedEntryAst = leak_tally::Leaked<span::Spanned<node::NodeList<'static>>>;

/// What one entry analysis produced, with the tree it leaked attached — the
/// return of [`analyze_source_reclaimable`]. `program` and `diagnostics` are
/// exactly [`analyze_source`]'s pair; `ast` is the handle to the parsed entry
/// tree the program borrows (`None` when parsing produced no tree, in which
/// case there is no program either).
pub struct AnalyzedEntry {
    pub program: Option<Program<'static>>,
    pub diagnostics: Vec<Error>,
    pub ast: Option<LeakedEntryAst>,
    /// The claims this analysis holds on overlay-served module allocations
    /// (M9, `leak-soak.md` §7.9.4; M23's claim protocol) — drained from the
    /// collection scope, reclaimable with `ast` once the program is dropped.
    /// One per module the analysis parsed, plus one per module a base-cache
    /// hit served it out of a stored world. Empty unless the analysis opted
    /// in ([`analyze_source_owning_overlay_modules`]).
    pub owned_modules: OwnedModules,
}

/// Lex, parse, and fully analyze a source string. The source must already be
/// leaked to `'static` so the returned `Program` (which borrows it) can outlive
/// this call — the front-end that owns the document lifecycle does the leak.
///
/// Returns the analyzed program — present whenever parsing produced a tree, even
/// a partial one recovered from syntax errors — together with every diagnostic
/// (lexer, parser, and analyzer) for the entry file. Analysis is wrapped so a
/// panic on malformed input degrades to "no program" rather than taking the
/// process down, which matters when an editor analyzes on every keystroke.
/// `platform` is the build platform to analyze against — pass `Some` when the
/// front-end knows it (e.g. the language server resolved it from the project's
/// `vilan.toml`), or `None` to infer it from the file's imports.
///
/// The entry tree this parses is leaked for the program to borrow and stays
/// leaked — the shape every caller but the language server wants (the macro
/// world's nested compile keeps its tree in the cached world; the tests and
/// the wasm front end never drop a program mid-process). A front-end that DOES
/// drop programs calls [`analyze_source_reclaimable`] and owns the tree.
pub fn analyze_source(
    source: &'static str,
    std: &PackageSpec,
    pkg_root: &Path,
    entry_path: &Path,
    platform: Option<Platform>,
    workspace: &Workspace,
) -> (Option<Program<'static>>, Vec<Error>) {
    let analyzed =
        analyze_source_reclaimable(source, std, pkg_root, entry_path, platform, workspace);
    // `analyzed.ast` drops here without a reclaim: the tree stays leaked.
    (analyzed.program, analyzed.diagnostics)
}

/// [`analyze_source`], handing back the handle to the entry tree it leaked so
/// the caller can reclaim it once the program is dropped (`leak-soak.md` §7 —
/// the language server's per-keystroke session leak). Same pipeline, same
/// fences, same program and diagnostics; only the ownership of the tree
/// differs. The source text is still the caller's leak, as before, so a
/// caller that wants the text back keeps its own `Leaked<str>` handle.
pub fn analyze_source_reclaimable(
    source: &'static str,
    std: &PackageSpec,
    pkg_root: &Path,
    entry_path: &Path,
    platform: Option<Platform>,
    workspace: &Workspace,
) -> AnalyzedEntry {
    // One outer fence covers the stages the analysis fence below does not —
    // lexing/parsing and the lift rewrite. A panic there used to unwind into
    // the caller: in the editor that meant through `Document::analyze`'s
    // thread join and out of a request handler, aborting the whole language
    // server (B40). It degrades to "no program" plus an honest diagnostic
    // instead; the panic hook (or default hook) has already written the
    // payload and location to stderr. A tree leaked before such a panic is
    // lost with the unwound frame (once per panic, not a session rate) —
    // the inner fence around `analyze` below is the one that matters for
    // reclaim, and it returns the handle.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        analyze_source_unfenced(source, std, pkg_root, entry_path, platform, workspace)
    }))
    .unwrap_or_else(|_| AnalyzedEntry {
        program: None,
        diagnostics: vec![Error { trace: Vec::new(),
            note: None,
            span: crate::span::Span::new((), 0..0),
            msg: "internal error: the compiler panicked analyzing this file (this is a bug; the details are on stderr)".to_string(),
        }],
        ast: None,
        owned_modules: OwnedModules::none(),
    })
}

/// [`analyze_source_reclaimable`] with the M9 opt-in active (`leak-soak.md`
/// §7.9.4): for the duration of this analysis, a module load served from the
/// open-document overlay bypasses the process-global parse caches
/// ([`parse_clean_cached`] and the loader's error cache) and parses into
/// allocations the returned entry OWNS (`AnalyzedEntry::owned_modules`),
/// reclaimable beside the entry's tree once the program is dropped.
///
/// The language server is the caller, and the reason is §7.5: an open
/// buffer's content is a keystroke's, which can never recur, so caching it
/// process-globally is a session leak — one text + tree of the edited file
/// per landed keystroke while a dependent is open. Activation is THIS
/// explicit opt-in, not an ambient property of the overlay: the wasm front
/// end serves everything from the overlay and must keep the global caches,
/// as must every transient reader. A macro-world compile inside the analysis
/// keeps the global caches too — its world outlives every analysis
/// (§7.9.4b).
///
/// What makes the returned handles' reclaim sound is a reference count, not
/// exclusivity (M23): a base world stored for this analysis DOES borrow these
/// allocations, and holds its own claim on each, so `reclaim` frees only what
/// nothing else still claims. §7.9.4a's outright refusal to store such a
/// world is what this replaces — it cost every entry importing an open
/// sibling the whole pre-entry world on every keystroke.
pub fn analyze_source_owning_overlay_modules(
    source: &'static str,
    std: &PackageSpec,
    pkg_root: &Path,
    entry_path: &Path,
    platform: Option<Platform>,
    workspace: &Workspace,
) -> AnalyzedEntry {
    let scope = owned_modules::CollectionScope::activate();
    let mut analyzed =
        analyze_source_reclaimable(source, std, pkg_root, entry_path, platform, workspace);
    // The panic paths land here too (`analyze_source_reclaimable` catches
    // its unwinds): whatever the scope collected before the panic has no
    // borrower left — the unwind dropped every analyzer local, and no global
    // holds one (the store gate, the macro carve-out) — so the handles ride
    // the degraded entry and are reclaimed with it, like the entry tree.
    analyzed.owned_modules = scope.drain();
    analyzed
}

fn analyze_source_unfenced(
    source: &'static str,
    std: &PackageSpec,
    pkg_root: &Path,
    entry_path: &Path,
    platform: Option<Platform>,
    workspace: &Workspace,
) -> AnalyzedEntry {
    // The handwritten frontend lexes and parses in a single fast-and-rich pass,
    // always returning a tree — clean, or recovered from syntax errors — together
    // with every diagnostic (lexer and parser, span-ordered). Analysis below runs
    // on the salvaged tree, so a mid-edit source still yields a partial program
    // rather than nothing (frontend.md §3 S4/S5 — the LSP-facing improvement).
    // The depth instrument (B138) anchors per top-level analysis; a macro world
    // is a nested analysis on the same thread whose depths belong to the outer
    // run, so it must not re-anchor. It anchors HERE rather than inside
    // `analyze` so the parser's own recursion — which runs first and is not
    // depth-bounded (B139) — is inside the measured window: the pipeline's
    // stack need is the deepest point ANY phase reaches, and leaving the first
    // phase out of the instrument is what let a nesting depth the analyzer
    // handles comfortably overflow the stack before analysis began.
    if !macros::in_macro_world() {
        depth_stats::begin();
    }
    let (tree, parse_errors) = parsing::parse(source);
    let mut diagnostics: Vec<Error> = parse_errors
        .iter()
        .map(|error| Error {
            trace: Vec::new(),
            note: None,
            span: error.span,
            msg: parsing::render(error),
        })
        .collect();
    let Some(mut root) = tree else {
        return AnalyzedEntry {
            program: None,
            diagnostics,
            ast: None,
            owned_modules: OwnedModules::none(),
        };
    };

    // A macro WORLD's entry gets the ambient meta prelude (macro-engine.md
    // §3/§10): the reflection vocabulary binds at file scope. Names the file
    // defines itself are excluded, so an explicit definition shadows the
    // prelude.
    if macros::in_macro_world() {
        // `macro { .. }` blocks survive world blanking verbatim and parse at
        // the world's top level; wrap each into the synthetic zero-argument
        // `fun __macro_block_<n>(): Source` the expansion engine dispatches
        // (macro-engine.md Phase 4). Numbering is source order — the same
        // order registration assigned.
        let mut block_ordinal = 0usize;
        for node in root.0.iter_mut() {
            if matches!(node.0, Node::MacroBlock(_)) {
                let placeholder = std::mem::replace(&mut node.0, Node::Error);
                let Node::MacroBlock(body) = placeholder else {
                    unreachable!("just matched MacroBlock");
                };
                let name: &'static str =
                    Box::leak(macros::block_entry_name(block_ordinal).into_boxed_str());
                leak_tally::record(leak_tally::LeakSite::MacroBlockEntryName, name.len());
                block_ordinal += 1;
                let start = node.1.into_range().start;
                let head: Span = (start..start).into();
                node.0 = Node::Func(Func {
                    name: (name, head),
                    is_async: false,
                    external: false,
                    deprecated: None,
                    extern_binding: None,
                    extern_retains: false,
                    must_use: false,
                    platform_fence: Vec::new(),
                    rpc: false,
                    trait_only: false,
                    doc_hidden: false,
                    generic_parameters: None,
                    parameters: (Vec::new(), head),
                    return_type: Some(Box::new((Node::Accessor("Source"), head))),
                    borrows: None,
                    contexts: None,
                    body: Some(body),
                });
            }
        }
        let mut defined = std::collections::HashSet::new();
        for (node, _span) in root.0.iter() {
            let function = match node {
                Node::Func(function) => Some(function),
                Node::Export(inner) => match &inner.0 {
                    Node::Func(function) => Some(function),
                    _ => None,
                },
                _ => None,
            };
            if let Some(function) = function {
                defined.insert(function.name.0);
            }
        }
        if let Some(prelude) = macros::world_prelude_nodes(&defined) {
            root.0.splice(0..0, prelude);
        }
    }

    // `css` blocks desugar to their style chains, elements to their view
    // chains, then bare-`?` marks become lift regions, before the tree freezes
    // (css-block.md §5, element-syntax.md §4, expression-lifting.md) — the
    // formatter parses separately and keeps raw trees, so source text prints
    // back verbatim. The css pass runs FIRST so a block inside an element's
    // head or a child hole is already a chain when the element pass reaches it.
    css::rewrite_items(&mut root.0, source);
    elements::rewrite_items(&mut root.0, source);
    lift::rewrite_items(&mut root.0);
    // The tally is a tree-proportional estimate — one `Spanned<Node>` of
    // storage per node — so growth in the tree is visible to the counters;
    // the root box alone would record a constant ~40 B whatever the file
    // holds. Vec spare capacity is not counted: a deterministic lower bound,
    // not a heap audit (the leak_tally module doc has the full contract).
    fn tree_estimate(nodes: &NodeList) -> usize {
        fn count(node: &span::Spanned<Node>, total: &mut usize) {
            *total += 1;
            node.0.for_each_child(&mut |child| count(child, total));
        }
        let mut total = 0;
        for node in nodes {
            count(node, &mut total);
        }
        total * std::mem::size_of::<span::Spanned<Node>>()
    }
    // A macro WORLD's entry is analysed through this same function, and its
    // tree is kept by the cached world — bounded by `WORLDS`, not a per-
    // analysis leak — so it records at its own site, and `EntryAst` means
    // exactly the top-level entry's tree: the one a front-end may reclaim.
    let tree_site = if macros::in_macro_world() {
        leak_tally::LeakSite::MacroWorldAst
    } else {
        leak_tally::LeakSite::EntryAst
    };
    let estimate = tree_estimate(&root.0);
    // Leaked with the handle kept: the `Program` below borrows `root` for
    // `'static`; whoever drops that program may reclaim the tree through the
    // handle (`analyze_source` drops the handle and keeps the leak).
    let (ast, root) = leak_tally::Leaked::leak(Box::new(root), tree_site, estimate);
    // Use the front-end's resolved platform (e.g. from `vilan.toml`), else infer
    // one from the file's own imports: a file importing the browser DOM layer is a
    // browser file, otherwise Node. This keeps the platform gate from
    // false-flagging valid `std::dom` usage while still catching a genuine
    // cross-platform import (e.g. `std::http` in a file that also reaches for
    // `std::dom`).
    let platform = platform.unwrap_or_else(|| infer_platform(&root.0, std));
    // M26's PARSE boundary (`editor-latency.md` §4.2): the first of the
    // checkpoints, and the cheapest place to stop — the tree is parsed and the
    // analysis proper has not begun. The handle rides out with the (empty)
    // entry so the caller reclaims the tree exactly as it does on the panic
    // path below; skipping the analysis is the whole saving.
    if cancel::cancelled() {
        return AnalyzedEntry {
            program: None,
            diagnostics,
            ast: Some(ast),
            owned_modules: OwnedModules::none(),
        };
    }
    let analyzed = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        // M26: `None` is a CANCELLED analysis — a newer revision of this
        // document arrived while it ran, so it stopped at a checkpoint and
        // there is no program. It reaches the caller the same way the panic
        // path below does: no program, and the tree handle still in hand to
        // reclaim.
        let mut program = analyzer::analyze_cancellable(
            root, source, std, pkg_root, entry_path, platform, workspace,
        )?;
        // The post-pass half of the `VILAN_PHASE_TIMING` split prints inside
        // `post_analysis_passes` itself (backlog M5), so BOTH pipelines —
        // this one (LSP, wasm, the test harnesses) and the CLI's — show it.
        post_analysis_passes(&mut program, platform, &options::BuildOptions::default());
        Some(program)
    }));
    match analyzed {
        Ok(Some(program)) => {
            diagnostics.extend(program.diagnostics.iter().cloned());
            AnalyzedEntry {
                program: Some(program),
                diagnostics,
                ast: Some(ast),
                owned_modules: OwnedModules::none(),
            }
        }
        // Cancelled inside the analyzer, past the parse checkpoint above.
        Ok(None) => AnalyzedEntry {
            program: None,
            diagnostics,
            ast: Some(ast),
            owned_modules: OwnedModules::none(),
        },
        // The analysis unwound inside its fence: every analyzer local went
        // with it and nothing global borrowed the tree (leak-soak.md §7.2),
        // so the handle is still the caller's to reclaim.
        Err(_) => AnalyzedEntry {
            program: None,
            diagnostics,
            ast: Some(ast),
            owned_modules: OwnedModules::none(),
        },
    }
}

/// The whole-program passes that run after `analyze()` returns, in order, and
/// the ONE call graph they share.
///
/// This sequence used to be written out twice — here for `analyze_source`
/// (tests, LSP, wasm) and again in the CLI's `main.rs` — which is the standing
/// trap that a pass added to one pipeline is silently skipped by the other.
/// One definition, both callers.
///
/// **The call graph is built once, here, and that placement is the invariant**
/// (E35, `const-eval.md` §8.4). `context::thread_contexts` is the last thing
/// that rewrites the tables `CallGraph::build` reads — its `apply` deletes call
/// edges (a threaded `get()` becomes a local read) and mints new ones (the
/// hidden context argument) — so it owns the one graph that cannot be shared,
/// builds its own, and hands it back only when it applied no rewrite. Every
/// pass from here on writes only diagnostics and its own result tables:
///
/// - `async_infer::infer` → `async_functions`, `async_values`, `awaited_calls`,
///   `adapted_instances`;
/// - `check_async_drops` / `check_context_drops` → diagnostics only;
/// - `platform_color::check` → diagnostics only;
/// - `const_eval::evaluate` → takes `&Program` and returns its results;
/// - `init_order::check_cycles` and everything downstream (chunk planning,
///   emission, the LSP's `platform_color::requirements`) → read it back off the
///   program.
///
/// None of those fields is a graph input, so every consumer sees the identical
/// graph it would have built for itself. This is a SHARE, not a narrowing:
/// `CallGraph::build` takes no scope, filter or edge-kind options — there is
/// one whole-program graph with one edge vocabulary, and per-pass views are
/// taken downstream of it (`CallGraph::successors`, `reachable_bindings`),
/// unchanged.
pub fn post_analysis_passes(
    program: &mut Program,
    platform: Platform,
    options: &options::BuildOptions,
) {
    // The post-pass half of the `VILAN_PHASE_TIMING` split, per pass (backlog
    // M5): the aggregate `post-passes` wall could not say WHICH pass moved,
    // and attributing M4 meant hand-patching `Instant` marks into this
    // function three separate times (`perf-baseline.md` §4.3, `const-eval.md`
    // §10.1/§10.6). The marks are unconditional, on the analyzer's argument —
    // a handful of clock reads is noise next to a pass — and the line prints
    // here rather than in `analyze_source` so both pipelines (the LSP/test
    // path AND the CLI's) show it.
    let phase_post_start = PhaseClock::now();
    // N43: `dispatch_refine::refined_edges` is reached from two of the buckets
    // below (`contexts+graph` and `const-pass`), and on kolt it was most of
    // both. Zero the accumulator here — the top of the only region that calls
    // it — so the `dispatch-refine` bucket is this analysis's total.
    dispatch_refine::reset_refine_time();
    // M26's POST-PASS boundary, the outermost of the three the phase line names
    // (`contexts+graph`, `const-pass`, `dispatch-refine`; the last is a slice
    // through the first two, so cancelling either cancels it). The passes are
    // 0.31–0.46 s of a superseded analysis on kolt, and every one of them
    // writes only diagnostics and result tables onto a `Program` this analysis
    // owns and the caller is about to drop — so returning here leaves nothing
    // half-written that anyone else can read. `call_graph_memo` is a memo: a
    // consumer that asks an uninstalled program for the graph builds it, so
    // skipping the install below is a missing OPTIMISATION, not a missing fact.
    if cancel::cancelled() {
        return;
    }
    let phase_contexts_start = PhaseClock::now();
    let call_graph =
        context::thread_contexts(program).unwrap_or_else(|| call_graph::CallGraph::build(program));
    let phase_contexts = phase_contexts_start.elapsed();
    // M26's `contexts+graph` boundary, the first the phase line names and the
    // largest post-pass (171–242 ms on kolt): a cancel that arrived while the
    // graph was being built stops here rather than paying the six passes that
    // read it. The graph is dropped with the frame — it is installed on the
    // program only at the bottom of this function, which a cancelled analysis
    // never reaches.
    if cancel::cancelled() {
        return;
    }
    let phase_async_start = PhaseClock::now();
    async_infer::infer(program, &call_graph);
    let phase_async = phase_async_start.elapsed();
    // E3's implicit half (B119, view-invalidation.md §7): a view may not live
    // across a call that CAN SUSPEND, which is the call graph's answer, not the
    // `await` token's. `check_invalidation` recorded the candidate sites inside
    // `analyze()` (it owns view liveness); this decides them against the
    // suspension set `async_infer` just settled.
    let phase_views_start = PhaseClock::now();
    analyzer::check_view_suspensions(program, &call_graph);
    let phase_views = phase_views_start.elapsed();
    // `drop` must be synchronous (destruction.md §5): reject an async drop
    // body now that `async_functions` is settled — an awaiting body is async
    // only by inference, so this cannot run inside `analyze`.
    let phase_async_drops_start = PhaseClock::now();
    analyzer::check_async_drops(program);
    let phase_async_drops = phase_async_drops_start.elapsed();
    // And teardown must be context-free (destruction.md §8): a `drop` body
    // whose call sites (scope exits) can thread no context is rejected. Runs
    // after `thread_contexts` fills `context_dependent_functions`.
    let phase_context_drops_start = PhaseClock::now();
    analyzer::check_context_drops(program);
    let phase_context_drops = phase_context_drops_start.elapsed();
    let phase_platform_start = PhaseClock::now();
    platform_color::check(program, platform, &call_graph);
    let phase_platform = phase_platform_start.elapsed();
    // M26's `const-pass` boundary, the second the phase line names: the pass
    // costs 65–158 ms on kolt and most of it is `check_const_only`'s dispatch
    // refinement (N43). Skipping it leaves `const_results` and its siblings
    // empty, which for a cancelled analysis is the same nothing the whole
    // program is about to become.
    if cancel::cancelled() {
        return;
    }
    // The const pass (proposal/const-eval.md): evaluate `const`-marked
    // expressions in dependency order; results serialize in place at
    // transform time, failures are ordinary diagnostics. Runs here so
    // `check`, the LSP, and every build path agree.
    let phase_const_start = PhaseClock::now();
    let evaluated = const_eval::evaluate(program, options, &call_graph);
    let phase_const = phase_const_start.elapsed();
    program.const_results = evaluated.results;
    program.const_assets = evaluated.assets;
    program.const_input_files = evaluated.input_files;
    program.const_bundled_files = evaluated.bundled;
    program.const_facts = evaluated.facts;
    for (error, source) in evaluated.errors {
        program.push_diagnostic(error, source);
    }
    // The last pass to want it by reference is done, so the graph moves onto
    // the program: the cycle check below, chunk planning, emission and the
    // LSP's requirement hover all read it through `Program::call_graph`.
    program.install_call_graph(call_graph);
    // A dependency cycle among module-level initializers has no valid
    // declaration order (b33-emission-order.md §3), so it is an error
    // rather than a load-time `ReferenceError`. Runs last: the relation is
    // only meaningful for a program that analyzed cleanly.
    let phase_init_start = PhaseClock::now();
    init_order::check_cycles(program);
    let phase_init = phase_init_start.elapsed();
    // THE seam for diagnostic order (E38, diagnostics-standard.md C1). Nothing
    // after this point adds to either list, and both pipelines run this
    // function, so normalizing here is what every consumer reads — including
    // the HMR overlay, which shows only the first `OVERLAY_DIAGNOSTIC_CAP` of
    // them and so needs the order to be an answer, not an artifact.
    program.normalize_diagnostic_order();

    // One line, whitespace-separated `name value` pairs like the in-analyze
    // line, stderr for the same reason. The named buckets do not sum to the
    // `post-passes` total — the residual is the seam glue (the graph install,
    // diagnostic-order normalization) — and `const-lower`/`const-interp` are a
    // SUB-split of `const-pass`: the shared world's lowering + per-site
    // assembly against the interpreter's evaluation, the two thirds/one third
    // `const-eval.md` §10.2 had to hand-measure. Printed for macro worlds too,
    // exactly as the aggregate line this extends was.
    //
    // N43 — TWO buckets are named after what they TIME, not after the pass a
    // reader assumed. `contexts+graph` is `context::thread_contexts` (which
    // builds its own call graph when it rewrites, and falls back to
    // `CallGraph::build` when it does not); calling it `call-graph` sent the
    // editor-perf lane looking at graph construction for a cost that was
    // dispatch refinement. `const-pass` is the whole const pass, of which
    // `const-lower + const-interp` is the actual const EVALUATION: on kolt the
    // bucket read 1,189 ms while the evaluation inside it was 11 ms, the rest
    // being `check_const_only`'s dispatch refinement. `dispatch-refine` is
    // that shared constant, summed across both call sites, so the next lane
    // reads the split off the line instead of a profiler. It is deliberately
    // NOT disjoint from the two buckets it explains — it is a slice through
    // them, and the comment above already says the buckets do not sum.
    if phase_timing_enabled() {
        let milliseconds = |duration: std::time::Duration| duration.as_secs_f64() * 1000.0;
        let (const_lower, const_interp) = const_eval::phase_split();
        eprintln!(
            "[vilan phase] post-passes {:.1}ms contexts+graph {:.1}ms async-infer {:.1}ms \
             view-suspensions {:.1}ms async-drops {:.1}ms context-drops {:.1}ms \
             platform-color {:.1}ms const-pass {:.1}ms const-lower {:.1}ms \
             const-interp {:.1}ms const-fuel-max {} init-order {:.1}ms \
             dispatch-refine {:.1}ms",
            milliseconds(phase_post_start.elapsed()),
            milliseconds(phase_contexts),
            milliseconds(phase_async),
            milliseconds(phase_views),
            milliseconds(phase_async_drops),
            milliseconds(phase_context_drops),
            milliseconds(phase_platform),
            milliseconds(phase_const),
            milliseconds(const_lower),
            milliseconds(const_interp),
            const_eval::max_fuel_used(),
            milliseconds(phase_init),
            milliseconds(dispatch_refine::refine_time()),
        );
    }
    // The depth line (B138), after the last pass that recurses: macro worlds
    // are nested analyses whose depths already accumulated into the outer
    // run's peaks, so only the top-level run prints (the same reason the
    // in-analyze phase line guards).
    if !macros::in_macro_world() {
        depth_stats::report();
    }
}

/// Anchor the `VILAN_DEPTH_STATS` instrument for an analysis a front end is
/// about to drive itself.
///
/// [`analyze_source`] anchors around its own parse and needs no help. The CLI
/// parses the entry file itself and hands the tree to `analyze`, so without
/// this its parse — the pipeline's FIRST recursion, and the one that is not
/// depth-bounded (B139) — would fall outside the measured window and the depth
/// line would report the parser as three levels deep whatever the file nests.
/// Anchoring is idempotent within an analysis and released by the report at the
/// end of `post_analysis_passes`, so calling this before a parse that `analyze`
/// will later be handed is correct, and calling it redundantly is harmless.
pub fn begin_depth_stats() {
    if !macros::in_macro_world() {
        depth_stats::begin();
    }
}

/// Whether `VILAN_LEAK_REPORT` asks for the per-analysis leak line (any value
/// but empty or `0`). Read once and cached: an env var does not change under a
/// live process, and the LSP asks on every keystroke.
pub(crate) fn leak_report_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("VILAN_LEAK_REPORT").is_ok_and(|value| !value.is_empty() && value != "0")
    })
}

/// Whether `VILAN_PHASE_TIMING` asks for the per-analysis phase line (any
/// value but empty or `0`) — the std-tax arc's instrument
/// (proposal/analysis-reuse.md §6): one stderr line per analysis splitting
/// the wall between module loading+walking, `build()`, and the whole-program
/// checks, so a reuse slice's effect is measured where it lands. Read once
/// and cached, like the leak report: the LSP asks on every keystroke.
/// A wall-clock mark that is a NO-OP on wasm32 — `std::time::Instant::now()`
/// aborts on `wasm32-unknown-unknown` (no clock without WASI), and the phase
/// marks run unconditionally on every analysis, so the v0.23.0 playground
/// crashed on its first compile. The smoke gate caught it pre-publish; this
/// keeps the instrument for hosts and makes wasm report zeros.
#[derive(Clone, Copy)]
pub(crate) struct PhaseClock {
    #[cfg(not(target_arch = "wasm32"))]
    started: std::time::Instant,
}

impl PhaseClock {
    pub(crate) fn now() -> Self {
        PhaseClock {
            #[cfg(not(target_arch = "wasm32"))]
            started: std::time::Instant::now(),
        }
    }

    pub(crate) fn elapsed(&self) -> std::time::Duration {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.started.elapsed()
        }
        #[cfg(target_arch = "wasm32")]
        {
            std::time::Duration::ZERO
        }
    }
}

/// Whether the `VILAN_PHASE_TIMING` instrument is on — public so a FRONT END
/// can add its own phases to the same line under the same switch. The language
/// server does (`document::analyze_on_this_thread`): the costs it owns —
/// project resolution, the editor tables, a shared module's further legs — all
/// sit outside `analyze`, so the core line above cannot see them, and a second
/// switch would mean two half-pictures.
pub fn phase_timing_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("VILAN_PHASE_TIMING").is_ok_and(|value| !value.is_empty() && value != "0")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// backlog E97 — the tree's ONE posture on mutex poisoning: every lock of a
    /// shared cache recovers with `PoisonError::into_inner` rather than
    /// propagating. The long-lived surfaces CATCH panics (the four fences), so a
    /// propagated poison outlives the request that caused it and answers
    /// "internal error" for the rest of the session.
    ///
    /// Poisoned exactly the way a caught panic poisons: an unwind out of code
    /// holding the guard, caught, with the process still running afterwards.
    #[test]
    fn a_poisoned_parse_cache_recovers_instead_of_wedging_the_session() {
        let source = "fun poisoned_cache_probe() {}\n";
        let first = parse_clean_cached(source).expect("a clean source caches");
        let cache = PARSE_CLEAN_CACHE
            .get()
            .expect("the clean-parse cache is live");
        let broken = PARSE_CLEAN_BROKEN
            .get()
            .expect("the known-broken set is live");

        let poison = |mutex: &dyn Fn()| {
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(mutex));
            assert!(outcome.is_err(), "the probe panic must have unwound");
        };
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        poison(&|| {
            let _guard = cache.lock().expect("not poisoned yet");
            panic!("a compiler panic inside the fence");
        });
        poison(&|| {
            let _guard = broken.lock().expect("not poisoned yet");
            panic!("a compiler panic inside the fence");
        });
        std::panic::set_hook(previous_hook);
        assert!(
            cache.is_poisoned(),
            "the probe poisoned the clean-parse cache"
        );
        assert!(
            broken.is_poisoned(),
            "the probe poisoned the known-broken set"
        );

        // A hit still hits: the identical `'static` pointer comes back, which is
        // how reuse is proven without timing.
        let again = parse_clean_cached(source).expect("the poisoned cache still serves");
        assert!(
            std::ptr::eq(first.0, again.0) && std::ptr::eq(first.1, again.1),
            "a recovered guard serves the cached tree, not a fresh parse"
        );
        // A miss still inserts through the recovered guard...
        let other = "fun poisoned_cache_probe_two() {}\n";
        let inserted = parse_clean_cached(other).expect("a fresh clean source caches");
        assert!(
            std::ptr::eq(inserted.0, parse_clean_cached(other).expect("cached").0),
            "the entry written under a recovered guard is readable back"
        );
        // ...and the known-broken half records through its own recovered guard.
        assert!(
            parse_clean_cached("fun (").is_none(),
            "a broken source is still refused, not panicked on"
        );
        assert!(
            parse_clean_cached("fun (").is_none(),
            "and it is served from the recovered known-broken set the second time"
        );

        // Leave the globals as they were found: the posture makes a poisoned
        // mutex harmless, but a later test reading `is_poisoned` should not
        // inherit this one's probe.
        cache.clear_poison();
        broken.clear_poison();
    }

    /// The posture is only worth having if it is UNIFORM — the defect E97 filed
    /// was "defended in one function and not in its neighbour thirty lines
    /// later", which is a drift bug, not a missing-defense bug. So the gate is
    /// on the drift: no lock in any crate's sources may propagate a poison.
    ///
    /// Deliberately a source scan rather than a per-site pin. A poison pin can
    /// only reach a cache a test can name, and most of these are private to the
    /// function that owns them; the thing worth holding is that the NEXT lock
    /// added anywhere in the workspace joins the posture, and that is a property
    /// of the text.
    ///
    /// Scoped to each crate's `src/` — the separate `tests/` binaries are out,
    /// where a lock is a single-threaded arrangement with no session to wedge.
    /// An in-`src` test that must ACQUIRE a lock (this file's neighbour above
    /// poisons one on purpose) spells `.expect("…")`, so a deliberate assertion
    /// reads as one and the gate stays a signal rather than noise.
    #[test]
    fn every_lock_in_the_workspace_recovers_from_poisoning() {
        // Assembled rather than written literally, so the gate does not match
        // its own needles when it scans this file.
        let needles: Vec<String> = ["lock", "read", "write"]
            .iter()
            .map(|method| format!(".{method}().{}()", "unwrap"))
            .collect();
        let crates = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("crates/");
        let mut offenders = Vec::new();
        let mut scanned = 0;
        let mut directories: Vec<std::path::PathBuf> = std::fs::read_dir(crates)
            .expect("a readable crates/ tree")
            .map(|entry| entry.expect("a readable entry").path().join("src"))
            .filter(|source_root| source_root.is_dir())
            .collect();
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(&directory).expect("a readable source directory") {
                let path = entry.expect("a readable entry").path();
                if path.is_dir() {
                    directories.push(path);
                    continue;
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                scanned += 1;
                let text = std::fs::read_to_string(&path).expect("a readable source file");
                // Whitespace-stripped, so rustfmt's multi-line chain reads the
                // same as the one-liner. `.unwrap_or_else(…)` does not match.
                let flat: String = text.chars().filter(|c| !c.is_whitespace()).collect();
                for needle in &needles {
                    if flat.contains(needle) {
                        offenders.push(format!(
                            "{} propagates a poison at `{needle}`",
                            path.display()
                        ));
                    }
                }
            }
        }
        assert!(scanned > 0, "the scan found no Rust sources — wrong root?");
        assert!(
            offenders.is_empty(),
            "backlog E97: every lock recovers with `.unwrap_or_else(std::sync::PoisonError::into_inner)`, \
             so a caught panic cannot wedge a long-lived session. Offending sites:\n{}",
            offenders.join("\n")
        );
    }
}
