//! The completion engine (`proposal/playground-completion.md` §3): the value
//! types, the two tables (every keyword, the construct snippets), the
//! gatherers — member, lifted-member, `::` path, import-path, element-head,
//! macro-name, scope and auto-import — and the insertion rule that shapes a
//! call. Moved here VERBATIM from the language server's `Document` so that the
//! server and the playground answer identically; its behaviour is recorded in
//! `editing-dx.md` §18 (E66/E67) and pinned through the server's own tests.
//!
//! Everything is answered over an [`Analysis`]. The trigger scan reads the
//! LIVE text (the character being typed is live by nature); every lookup
//! that touches `program` data converts to the ANALYZED offset first
//! ([`Analysis::to_analyzed_offset`], E52).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use vilan_core::analyzer::{Expr, Implementation, Program, SourceId};
use vilan_core::formatter::{STYLE_CONDITION_METHODS, STYLE_PROPERTY_METHODS};
use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::lexing::tokenize;
use vilan_core::token::Token;
use vilan_core::type_::{Type, TypeId};
use vilan_core::{PackageSpec, Platform as BuildPlatform, Span};

use crate::analysis::{
    Analysis, binding_type_id, call_parameter_names, nominal_type_id, signature_label,
};

/// N115: the engine's own pins, driven without a protocol layer.
#[cfg(test)]
mod tests;

/// A scope-position construct snippet's insertion text (E14). The server
/// renders `body` for a snippet-capable client and falls back to `fallback` (the
/// bare keyword) otherwise — a `${1:…}` body would surface as literal text on a
/// client that cannot expand tab-stops.
pub struct SnippetInsertion {
    /// The `${n:…}`-tabstopped snippet body (LSP `InsertTextFormat::SNIPPET`).
    pub body: String,
    /// The plain keyword inserted when the client lacks snippet support.
    pub fallback: String,
}

/// A completion candidate offered at the cursor (mapped to an LSP `CompletionItem`
/// by the server).
pub struct Completion {
    pub label: String,
    pub kind: CompletionKind,
    /// The signature (functions/methods) or type (variables/fields) shown in
    /// the completion popup's detail line — the same house rendering hover
    /// uses. `None` for keywords, macros, modules and types.
    ///
    /// A FIELD carries its declared type since E204, read out of the struct's
    /// pre-rendered declaration label ([`Analysis::field_type_label`]). WO-3's
    /// "a field's type is not cheaply renderable from the analyzed `Program`"
    /// was true until E160 gave that label a reader; it is not any more, and
    /// the two field positions answer alike.
    pub detail: Option<String>,
    /// The first paragraph of the declaration's `///` doc, where present.
    pub documentation: Option<String>,
    /// The parameter names (`self` excluded) when this candidate is a function
    /// or method that should insert call-shaped — `Some(names)`, possibly empty
    /// for a zero-parameter callable. `None` requires a bare-name insertion: a
    /// non-callable, a callee already followed by `(`, or a use/import path.
    /// The server (`to_completion_item`) turns this into the actual insert text
    /// per the `vilan.completion.functionCall` setting.
    pub call_parameters: Option<Vec<String>>,
    /// The template insertion when this candidate is a construct snippet
    /// (`CompletionKind::Snippet`, from [`CONSTRUCT_SNIPPETS`]); `None` for every
    /// other candidate (E14).
    pub snippet: Option<SnippetInsertion>,
    /// A plain insertion that is neither call-shaped nor a construct snippet
    /// (E160): a struct-initializer field inserts `name = `, or the bare
    /// `name` where the shorthand applies. `None` inserts the label, which is
    /// what every other candidate does.
    ///
    /// Its own field rather than a reuse of [`Self::snippet`], because
    /// `snippet` also carries a construct snippet's RANKING — the front-ends
    /// sort one below every entity (`~`-prefixed `sort_text`), and a field is
    /// the only thing offered at its position, so burying it would be
    /// nonsense.
    pub insert: Option<InsertText>,
    /// What the CLIENT should match the typed prefix against (E211). `None`
    /// means the label, which is the LSP default.
    ///
    /// E194 fixed this from the client's side for VS Code by declaring a
    /// `wordPattern` that reads `stroke-width` and `--card-gap` as one word.
    /// Every other LSP client has its own word rules and no such file, so a
    /// hyphenated candidate was filtered out of its own list there — the server
    /// said `stroke-width`, the client saw the word `w`, and the candidate the
    /// author was typing towards was the one that disappeared.
    pub filter_text: Option<String>,
    /// The span this candidate REPLACES when it is accepted (E211), in LIVE
    /// coordinates. `None` inserts at the cursor, which is the LSP default.
    ///
    /// This is the half that actually fixes a hyphenated candidate: a client
    /// given an explicit edit range filters against the text in THAT range
    /// rather than against its own notion of a word, so `stroke-w` is the
    /// prefix wherever the list is being shown. It is also what makes accepting
    /// one replace the prefix instead of doubling it.
    pub replace_span: Option<Span>,
    /// The `[internal("reason")]` label on this candidate's declaration
    /// (E213), or `None` for the overwhelming majority that carry none.
    ///
    /// It is a fact about the DECLARATION, carried here so one rule can be
    /// applied in one place: [`Analysis::completion`] drops an internal
    /// candidate unless the typed prefix is an exact prefix of three
    /// characters or more, and the server sorts what survives last and shows
    /// this reason as the candidate's `detail`.
    pub internal: Option<String>,
    /// The import this candidate needs before it resolves (E54c) — `None` for
    /// a candidate already reachable without one (every candidate except the
    /// ones [`Analysis::auto_import_completions`] adds). The server
    /// (`to_completion_item`) turns `Some` into a labeled `detail` (the
    /// module, e.g. `std::json`) and the `additionalTextEdits` that insert
    /// the import when the candidate is accepted.
    pub needs_import: Option<AutoImport>,
}

/// The ready-made import edit an auto-import completion candidate carries
/// (E54c): the module path (for the popup's `detail` label) and the
/// [`vilan_core::formatter::insert_import`] edit that adds it, already
/// computed against the live buffer.
pub struct AutoImport {
    pub module_path: Vec<String>,
    pub edit_span: Span,
    pub edit_replacement: String,
    /// This candidate's auto-import ranking tier (E59, [`import_origin_tier`]),
    /// carried through so the server (`main::to_completion_item`) can bucket
    /// the client-visible `sort_text` by it without re-deriving it from
    /// `module_path` — one computation, read in two places.
    pub origin_tier: u8,
}

impl Completion {
    /// A plain candidate — a bare-name insertion, no signature and no
    /// call-shaping (keywords, macros, fields, enum variants, type names).
    fn bare(label: String, kind: CompletionKind) -> Self {
        Completion {
            label,
            kind,
            detail: None,
            documentation: None,
            call_parameters: None,
            snippet: None,
            insert: None,
            filter_text: None,
            replace_span: None,
            internal: None,
            needs_import: None,
        }
    }

    /// A construct-snippet candidate (E14): a distinguishing `label`, a short
    /// `detail`, the `${n:…}` `body`, and the bare `keyword` fallback for a
    /// client without snippet support. Offered alongside the bare keyword at
    /// scope positions only.
    fn snippet(label: &str, detail: &str, body: &str, keyword: &str) -> Self {
        Completion {
            label: label.to_string(),
            kind: CompletionKind::Snippet,
            detail: Some(detail.to_string()),
            documentation: None,
            call_parameters: None,
            snippet: Some(SnippetInsertion {
                body: body.to_string(),
                fallback: keyword.to_string(),
            }),
            insert: None,
            filter_text: None,
            replace_span: None,
            internal: None,
            needs_import: None,
        }
    }
}

/// The category of a completion, for its editor icon.
///
/// A plain fieldless tag, so it carries the derives a tag should: E121's
/// keystroke-path symbol index stores one per declared name and compares them
/// (`crates/vilan-lsp/src/keystroke.rs`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompletionKind {
    Macro,
    Function,
    Method,
    Field,
    Struct,
    Enum,
    EnumVariant,
    Trait,
    Variable,
    Module,
    Keyword,
    /// A fill-in-the-blanks construct template (E14) — a distinct icon from the
    /// bare keyword it accompanies.
    Snippet,
}

/// How far [`Analysis::expression_type_id`] follows a value through nesting
/// shapes (a block's trailing expression, a closure-typed callee) before giving
/// up. Real receivers nest a step or two; the bound is what keeps a malformed
/// mid-edit tree from spinning.
const EXPRESSION_TYPE_DEPTH_LIMIT: usize = 8;

/// The vilan book's published base URL — keyword hovers deep-link into it.
/// (`crates/vilan-cli/tests/vscode_extension.rs` and `brew_formula.rs` pin the
/// same URL as the marketplace listing's and the tap's homepage.)
pub const BOOK_BASE: &str = "https://vilan-lang.org/docs/";

/// Every keyword the lexer classifies (`token.rs`), each with a one-line
/// meaning and a deep link into the book: `(keyword, sentence, page#anchor)`.
/// Semantics-bearing keywords point at the specification; the rest point where
/// the book teaches them best. The set is kept in lockstep with the lexer by
/// [`keyword_lexeme`], whose every keyword arm has an entry here. Every
/// `page#anchor` is held to the book's own headings by `book_sync.rs` — the
/// anchor is mdBook's slug of a heading in `vilan/docs/<page>.md`, so a
/// heading edit there has to land here too.
pub const KEYWORD_DOCS: &[(&str, &str, &str)] = &[
    (
        "fun",
        "Declares a function.",
        "tour/functions-and-closures.html#functions",
    ),
    (
        "struct",
        "Declares a struct, a product type with named fields.",
        "tour/data-and-traits.html#structs",
    ),
    (
        "enum",
        "Declares an enum, a sum type whose value is one of several variants.",
        "tour/data-and-traits.html#enums",
    ),
    (
        "trait",
        "Declares a trait, a set of methods a type can implement.",
        "tour/data-and-traits.html#traits",
    ),
    (
        "impl",
        "Implements methods for a type (and, with a trait, that trait).",
        "tour/data-and-traits.html#impl-methods-and-statics",
    ),
    (
        "with",
        "Names the trait(s) an `impl` provides (or a trait's supertraits).",
        "spec/types.html#54-impls",
    ),
    (
        "type",
        "Declares a type alias.",
        "spec/types.html#53-declarations",
    ),
    (
        "external",
        "Declares a host (FFI) type or function: its surface comes from the host, not Vilan.",
        "spec/types.html#53-declarations",
    ),
    (
        "macro",
        "Declares a macro, code that runs at compile time to produce code.",
        "spec/macros.html#101-declaring-and-invoking",
    ),
    (
        "const",
        "Evaluates an expression at compile time (`const expr`).",
        "spec/const.html#91-the-const-expression",
    ),
    (
        "css",
        "Begins a `css { … }` block: CSS declarations that build a `Style`.",
        "guide/styling.html#the-css-block",
    ),
    (
        "import",
        "Loads a module and binds the named items into this module's scope.",
        "spec/names.html#43-imports",
    ),
    (
        "use",
        "Binds names from an already-visible type's namespace (variants, statics) without loading a module.",
        "spec/names.html#43-imports",
    ),
    (
        "export",
        "Re-exports a statement's names so importers see them as if declared here.",
        "spec/names.html#43-imports",
    ),
    ("mod", "Declares a submodule.", "spec/names.html#41-modules"),
    (
        "let",
        "Binds an immutable local or module-level value.",
        "tour/values-and-types.html#bindings",
    ),
    (
        "lazy",
        "Defers a parameter's argument to the callee's first read of it, evaluated at most once.",
        "tour/functions-and-closures.html#lazy-parameters",
    ),
    (
        "mut",
        "Binds a mutable value, one that can be reassigned.",
        "tour/values-and-types.html#bindings",
    ),
    (
        "own",
        "Passes a parameter by value as an owned copy; for a `resource` this moves ownership into the callee.",
        "spec/memory.html#63-rule-3--references-are-second-class-views",
    ),
    (
        "borrows",
        "Names which parameter a function returns a view into: the one sanctioned way a view escapes a function (often inferred).",
        "spec/memory.html#65-projections-borrows",
    ),
    (
        "resource",
        "An owned value with exactly one owner, moved rather than copied, and torn down at scope end.",
        "spec/memory.html#68-resources-and-destruction",
    ),
    (
        "dyn",
        "`dyn Trait` is a trait OBJECT: a value whose concrete type is erased, carrying its trait's members in a table. Written explicitly, and only over a trait every one of whose REQUIRED members takes a receiver, names no `Self`, and is non-generic.",
        "tour/data-and-traits.html#trait-objects--dyn-trait",
    ),
    (
        "if",
        "Chooses between branches; `if` is an expression that produces a value.",
        "tour/control-flow.html#if--else",
    ),
    (
        "else",
        "The alternative branch of an `if`.",
        "tour/control-flow.html#if--else",
    ),
    (
        "match",
        "Matches a value against patterns, taking it apart by shape.",
        "tour/control-flow.html#match",
    ),
    (
        "is",
        "Tests whether a value matches a pattern, yielding a bool.",
        "tour/control-flow.html#match",
    ),
    (
        "for",
        "Iterates over the elements of a collection (`for x in xs`).",
        "tour/control-flow.html#loops",
    ),
    (
        "in",
        "Separates the binder from the iterated collection in a `for` loop.",
        "tour/control-flow.html#loops",
    ),
    (
        "jump",
        "Transfers control within a loop: `jump break` or `jump continue`.",
        "tour/control-flow.html#loops",
    ),
    (
        "ret",
        "Returns early from a function.",
        "tour/control-flow.html#early-return-ret",
    ),
    (
        "async",
        "Spawns work without waiting for it (`async expr` / `async { … }`), yielding a `Task<T>`; ordinary calls are awaited for you.",
        "tour/async.html#opting-out-of-waiting-async-and-await",
    ),
    (
        "await",
        "Collects a `Task<T>` spawned with `async`; ordinary calls need no `await`.",
        "tour/async.html#opting-out-of-waiting-async-and-await",
    ),
    (
        "true",
        "The boolean literal `true`.",
        "tour/values-and-types.html#primitives",
    ),
    (
        "false",
        "The boolean literal `false`.",
        "tour/values-and-types.html#primitives",
    ),
    (
        "null",
        "The null literal, the sole value of the `null` type.",
        "tour/values-and-types.html#wheres-null",
    ),
];

/// The scope-position construct snippets (E14) — the shape-heavy declarations
/// offered as fill-in-the-blanks templates *alongside* their bare keyword.
/// Each row is `(keyword, label, detail, body)`: `label` is the popup's
/// distinguishing display, `detail` its one-line description, `body` the
/// `${n:…}`-tabstopped snippet, and `keyword` both the lexer keyword this rides
/// and the plain-text fallback for a client without snippet support. The bodies
/// follow house style — tab indent, trailing comma, `i32` — verified against the
/// corpus. Growth is one row; each keyword stays a subset of the lexer's, pinned
/// by `construct_snippet_keywords_are_lexer_keywords`.
pub const CONSTRUCT_SNIPPETS: &[(&str, &str, &str, &str)] = &[
    (
        "for",
        "for … in { }",
        "iterate over a collection",
        "for ${1:item} in ${2:items} {\n\t$0\n}",
    ),
    (
        "fun",
        "fun … ( ) { }",
        "declare a function",
        "fun ${1:name}(${2}) {\n\t$0\n}",
    ),
    (
        "struct",
        "struct … { }",
        "declare a struct",
        "struct ${1:Name} {\n\t${2:field}: ${3:i32},\n}",
    ),
    (
        "match",
        "match … { }",
        "match on a value",
        "match ${1:subject} {\n\t${2:pattern} => $0,\n}",
    ),
];

/// The keyword lexeme a token spells, or `None` for non-keyword tokens
/// (identifiers, literals, operators, punctuation). Exhaustive over `Token`
/// deliberately: a new keyword variant must be classified here, which forces
/// the matching [`KEYWORD_DOCS`] entry it needs.
pub fn keyword_lexeme(token: &Token) -> Option<&'static str> {
    Some(match token {
        Token::Async => "async",
        Token::Await => "await",
        Token::Const => "const",
        Token::Css => "css",
        Token::Else => "else",
        Token::Enum => "enum",
        Token::Export => "export",
        Token::External => "external",
        Token::Bool(true) => "true",
        Token::Bool(false) => "false",
        Token::For => "for",
        Token::Fun => "fun",
        Token::If => "if",
        Token::Impl => "impl",
        Token::Import => "import",
        Token::In => "in",
        Token::Is => "is",
        Token::Jump => "jump",
        Token::Lazy => "lazy",
        Token::Let => "let",
        Token::Macro => "macro",
        Token::Match => "match",
        Token::Mod => "mod",
        Token::Mut => "mut",
        Token::Null => "null",
        Token::Own => "own",
        Token::Borrows => "borrows",
        Token::Ret => "ret",
        Token::Resource => "resource",
        Token::Dyn => "dyn",
        Token::Struct => "struct",
        Token::Trait => "trait",
        Token::Type => "type",
        Token::Use => "use",
        Token::With => "with",
        Token::Ident(_)
        | Token::Ctrl(_)
        // B318 §2.3: `#` is the import reach marker — punctuation, not a
        // keyword, so it takes no `KEYWORD_DOCS` entry.
        | Token::Hash
        | Token::Number(_, _, _)
        | Token::Op(_)
        | Token::String(_)
        | Token::MultilineString(_) => return None,
    })
}

/// Whether `offset` sits inside a `use`/`import` item — where a name is being
/// bound into scope, not called, so even a function completes to a bare name
/// (`use std::math::sqrt`, not `sqrt(…)`), and where the candidates themselves
/// come from the package tree rather than from scope (E57).
pub fn in_import_path(text: &str, offset: usize) -> bool {
    import_path_prefix(text, offset).is_some()
}

/// The import path typed so far on the line ending at `offset` — everything
/// after the `import`/`use` keyword — or `None` when the line is not an import
/// item.
///
/// Imports are single-line, newline-terminated items, so this reads the current
/// line's leading keyword (a leading `export` prefix — `export import …` — is
/// skipped). Multi-line braced groups past their first line are not recognized;
/// the corpus has none.
fn import_path_prefix(text: &str, offset: usize) -> Option<&str> {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map(|at| at + 1).unwrap_or(0);
    let mut line = text[line_start..offset].trim_start();
    if let Some(after_export) = strip_keyword(line, "export") {
        line = after_export.trim_start();
    }
    let after_keyword = strip_keyword(line, "import").or_else(|| strip_keyword(line, "use"))?;
    Some(after_keyword.trim_start())
}

/// `text` with a leading `keyword` removed — only when it stands there as a
/// WHOLE word, so `imported = 5` is an assignment and `used` is a name.
fn strip_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let rest = text.strip_prefix(keyword)?;
    match rest.as_bytes().first() {
        Some(byte) if is_identifier_byte(*byte) => None,
        _ => Some(rest),
    }
}

/// The import path's COMPLETED segments to the left of the cursor — the partial
/// name being typed is never one of them, so `import std::js|` yields `["std"]`
/// and `import s|` yields `[]`.
///
/// A brace set completes at the same level as the path before it: every leaf in
/// `import std::json::{ Json, |` is one more member of `std::json`, so the
/// innermost open brace splits the path from the partial name exactly as the
/// final `::` does otherwise — which is what makes brace-position completion
/// fall out of the routing rather than need its own machinery.
///
/// `None` when the line is not an import path, or when a segment is not an
/// identifier (a nested brace set, a half-typed operator): completion answers
/// nothing rather than guessing at a shape it does not understand.
pub fn import_path_segments(text: &str, offset: usize) -> Option<Vec<&str>> {
    let prefix = import_path_prefix(text, offset)?;
    let (path, in_braces) = match prefix.rfind('{') {
        Some(brace) => (&prefix[..brace], true),
        None => (prefix, false),
    };
    let mut segments: Vec<&str> = path.split("::").map(str::trim).collect();
    if in_braces {
        // The text before a brace ends at its `::`, leaving a trailing empty
        // piece; a brace directly after the keyword (`import { … }`) leaves the
        // whole path empty.
        segments.retain(|segment| !segment.is_empty());
    } else {
        // The last piece is the partial name under the cursor — empty right
        // after a `::`, and never a completed segment.
        segments.pop();
    }
    segments
        .iter()
        .all(|segment| is_identifier(segment))
        .then_some(segments)
}

/// Where inside an `(impl …)` SELECTOR the cursor sits (B318 S3,
/// `visibility.md` §7.1's third surface).
#[derive(Debug, PartialEq, Eq)]
pub enum SelectorPosition<'src> {
    /// After `impl ` — the module's impl SUBJECTS are the answer.
    Subject { module: Vec<&'src str> },
    /// After `)::`, braced or not — the selected block's members are.
    Member {
        module: Vec<&'src str>,
        subject: &'src str,
    },
}

/// The selector position of the cursor on the import line ending at `offset`,
/// or `None` when it is not inside one.
///
/// Read from the line's TEXT rather than from tokens, like every other part of
/// import-path completion (`import_path_segments` next door): a line being
/// typed does not parse, and what is being reached for does not exist in the
/// program yet — which is the property E57 rests the whole family on.
///
/// The subject may hold parentheses of its own (a tuple type), so the closing
/// `)` is found by depth rather than by the first one.
pub fn impl_selector_position(text: &str, offset: usize) -> Option<SelectorPosition<'_>> {
    let prefix = import_path_prefix(text, offset)?;
    let open = prefix.rfind("(impl")?;
    // `(impl` has to stand as a word: `(implementation` is a call, not a
    // selector.
    let after = &prefix[open + 5..];
    match after.as_bytes().first() {
        None => {}
        Some(byte) if !is_identifier_byte(*byte) => {}
        Some(_) => return None,
    }
    let module = selector_module_segments(&prefix[..open])?;
    let mut depth = 0usize;
    let mut close = None;
    for (at, byte) in after.bytes().enumerate() {
        match byte {
            b'(' => depth += 1,
            b')' if depth == 0 => {
                close = Some(at);
                break;
            }
            b')' => depth -= 1,
            _ => {}
        }
    }
    let Some(close) = close else {
        // Still inside `(impl …` — the subject is what is being written.
        return Some(SelectorPosition::Subject { module });
    };
    let subject = after[..close].trim();
    // Past the `)`, only a `::` tail is a completion position at all.
    let tail = after[close + 1..].trim_start();
    let tail = tail.strip_prefix("::")?;
    // `::{ a, ` is the same position as `::` — a further member of the set.
    let tail = tail.trim_start().strip_prefix('{').unwrap_or(tail);
    if tail.contains('}') {
        return None;
    }
    Some(SelectorPosition::Member { module, subject })
}

/// The module path standing before a selector — the segments of `path` up to
/// the brace set the selector is an element of. `None` when a segment is not an
/// identifier, the rule [`import_path_segments`] answers by.
fn selector_module_segments(path: &str) -> Option<Vec<&str>> {
    let path = match path.rfind('{') {
        Some(brace) => &path[..brace],
        None => path,
    };
    let mut segments: Vec<&str> = path.split("::").map(str::trim).collect();
    segments.retain(|segment| !segment.is_empty());
    segments
        .iter()
        .all(|segment| is_identifier(segment))
        .then_some(segments)
}

/// Whether `name` is a vilan identifier — a non-empty run of identifier bytes
/// that does not start with a digit.
fn is_identifier(name: &str) -> bool {
    !name.is_empty() && !name.as_bytes()[0].is_ascii_digit() && name.bytes().all(is_identifier_byte)
}

/// The package roots an `import`/`use` path in this file resolves against, kept
/// from the analysis that produced the `Program` (E57).
///
/// Import-path completion cannot read its candidates out of the `Program`: the
/// point of an import is to reach a module that has NOT been loaded, and the
/// head of the path names an *origin* — `std`, `pkg`, a dependency package —
/// which is not an entity at all. So it reads the package tree, and it must read
/// the same tree the loader would: these are the very values
/// `analyze_on_this_thread` handed to `analyze_source`, kept instead of dropped.
///
/// The platform is not among them — it selects which of a library's layers a
/// module resolves from, and the analysis records the one it settled on as
/// `Program::platform`.
pub struct ImportRoots {
    /// The `std` library's layered spec (`resolve_std`, or the playground's
    /// hand-built embedded spec).
    pub std: PackageSpec,
    /// Where `import pkg::..` siblings live — this file's package source root.
    pub pkg_root: PathBuf,
    /// The entry package's direct dependencies, each under the name an import
    /// addresses it by.
    pub dependencies: Vec<(String, PackageSpec)>,
}

impl ImportRoots {
    /// The source roots `origin::` resolves its modules from, in the loader's
    /// own order, together with the package SURFACE (`lib.vl`) that origin
    /// publishes. `None` when `origin` names no origin.
    ///
    /// A library has a surface — `import std::io::print` names a leaf of std's
    /// `lib.vl`, which declares nothing and re-exports everything. The entry
    /// package does not: a `[package]` has a `main.vl`, and its modules are
    /// addressed by path. This mirrors the loader exactly, which searches the
    /// layered `search_roots` for `std` and a dependency, and the single
    /// `pkg_root` for the entry's own `pkg::`.
    pub fn origin_roots(
        &self,
        origin: &str,
        platform: BuildPlatform,
    ) -> Option<(Vec<&Path>, Option<PathBuf>)> {
        fn library(spec: &PackageSpec, platform: BuildPlatform) -> (Vec<&Path>, Option<PathBuf>) {
            (
                spec.search_roots(platform),
                spec.surface.then(|| spec.base_root.join("lib.vl")),
            )
        }
        match origin {
            "std" => Some(library(&self.std, platform)),
            "pkg" => Some((vec![self.pkg_root.as_path()], None)),
            _ => self
                .dependencies
                .iter()
                .find(|(name, _)| name == origin)
                .map(|(_, spec)| library(spec, platform)),
        }
    }
}

/// The end of the TAG NAME of the innermost element whose opening tag could
/// contain `offset` — the start of its head (E67).
///
/// Two shapes count, because element syntax is desugared before analysis
/// (`elements.rs`) and so is only ever seen through a RAW parse, mid-edit:
///
/// - a parsed [`Node::Element`], which is what a complete tag gives; and
/// - a [`Node::Error`] spanning `<…>`, which is what `parse_atom`'s element
///   recovery leaves behind whenever a head item does not parse — `<div .>`
///   (no method name after the dot) and `<div >` (no `</div>` yet) both land
///   here, and they are exactly the buffers completion fires in.
///
/// Only the tag NAME bounds the answer; where the head ends, and whether the
/// cursor is still at the head's own bracket depth, is
/// [`Analysis::in_element_head`]'s token walk.
fn innermost_open_tag_end(
    node: &vilan_core::Spanned<vilan_core::node::Node<'_>>,
    offset: usize,
    source: &str,
    best: &mut Option<(usize, std::ops::Range<usize>)>,
) {
    use vilan_core::node::Node;
    let span = node.1.into_range();
    if span.start <= offset && offset <= span.end {
        let tag = match &node.0 {
            // A fragment (A46) has no head at all, so nothing completes
            // inside `<>` — `None`, not a zero-width head.
            Node::Element(body) => body.tag.map(|tag| tag.start..tag.end),
            Node::Error => error_tag_name_range(source, span.start, span.end),
            _ => None,
        };
        if let Some(tag) = tag
            && tag.end <= offset
            && best
                .as_ref()
                .is_none_or(|(width, _)| span.end - span.start <= *width)
        {
            *best = Some((span.end - span.start, tag));
        }
    }
    node.0
        .for_each_child(&mut |child| innermost_open_tag_end(child, offset, source, best));
}

/// The tag name's byte range in an error node the element recovery produced —
/// `<` immediately followed by a name, the whole run closed by `>`. `None`
/// for any other error node, so a failed expression is never mistaken for
/// markup.
fn error_tag_name_range(source: &str, start: usize, end: usize) -> Option<std::ops::Range<usize>> {
    let slice = source.get(start..end)?;
    if !slice.starts_with('<') || !slice.ends_with('>') {
        return None;
    }
    let name: usize = slice[1..]
        .bytes()
        .take_while(|byte| is_identifier_byte(*byte) || *byte == b'-')
        .count();
    (name > 0).then_some(start + 1..start + 1 + name)
}

/// The candidates for one of §7.1's positions in a `css` body.
///
/// The part that matters is where the vocabulary comes from. E67 refused to
/// invent an HTML attribute list, on the ground that it "would be a second
/// source of truth with nothing to gate it" — and that refusal is what this has
/// to clear. It clears it on a real disanalogy: the CSS property vocabulary is
/// NOT invented here. Every name below is a slot some `Style` method already
/// writes, read from [`STYLE_PROPERTY_METHODS`]'s own `properties` column, which
/// `crates/vilan-core/tests/style_table_sync.rs` holds to the method bodies
/// through six gates — so the list cannot drift from std without a red test.
/// The combinators come from [`STYLE_CONDITION_METHODS`] the same way.
///
/// Table order is canonical order (S3's sorter reads the same rows), so the
/// list arrives in the sequence `vilan fmt` would put the declarations in.
///
/// A101 took one row off the table and half-dissolved another. There is no
/// VALUE position any more — a declaration's value is an ordinary argument, so
/// [`css_position`] declines inside the parens and the cursor falls through to
/// expression completion, which is where `pct(`, `Color::` and every name in
/// scope already live. And the property row inserts the `(` with the name,
/// because a declaration is a call.
impl Analysis<'_, '_> {
    fn css_block_completions(&self, position: CssPosition, offset: usize) -> Vec<Completion> {
        match position {
            CssPosition::Property => {
                // E153: std's slots FIRST, in canonical order — those are the
                // properties this system has a typed method for, and the
                // sequence `vilan fmt` would put them in — then the rest of the
                // CSS property index (`css_properties::CSS_PROPERTIES`), which
                // is what the block exists to reach: `raw` writes ANY property,
                // and the fifty-odd std slots were silent about `mask`,
                // `contain` and `scroll-snap-type`. Ordering is the whole
                // difference between the two halves; both are offered as
                // fields.
                let mut seen: HashSet<&str> = HashSet::new();
                STYLE_PROPERTY_METHODS
                    .iter()
                    .flat_map(|method| method.properties.iter().copied())
                    .chain(vilan_core::css_properties::CSS_PROPERTIES.iter().copied())
                    .filter(|property| seen.insert(property))
                    .map(|property| {
                        let mut completion =
                            Completion::bare(property.to_string(), CompletionKind::Field);
                        // A101: the `(` comes with the name, and the cursor
                        // lands where the value goes. Plain text rather than a
                        // snippet — there is no tab-stop to place, and a client
                        // without snippet support would surface a `$0`.
                        completion.insert = Some(InsertText {
                            text: format!("{property}("),
                            is_snippet: false,
                        });
                        completion
                    })
                    .collect()
            }
            CssPosition::DottedHead => {
                self.css_dotted_head_completions(self.to_analyzed_offset(offset))
            }
            // The same rows, as the free CONSTRUCTORS a set is summed from. They
            // are functions here and methods above, which is exactly the
            // difference between `hover()` (the condition) and `.hover { … }`
            // (the sugar that puts a style under it) — and `element` appears
            // only here, because it is a value with no combinator twin.
            CssPosition::ConditionValue => STYLE_CONDITION_METHODS
                .iter()
                .map(|(condition, _)| {
                    Completion::bare(condition.to_string(), CompletionKind::Function)
                })
                .collect(),
            // The one v1 blank left (Q4), and blank rather than absent: falling
            // through to the enclosing scope is what an element head refuses for
            // the same reason — nothing in scope is a custom property.
            CssPosition::CustomProperty => Vec::new(),
        }
    }

    /// E183: every `impl Style` method the file can reach, at the dotted head.
    ///
    /// The combinators come FIRST, because a dotted head is most often a
    /// condition rule and the fourteen rows are the vocabulary the block's own
    /// grammar is shaped around. Then every OTHER method on `Style` — std's
    /// non-condition members (`raw`, `on`, the typed property methods) and the
    /// program's own extensions, which is what an app actually writes there:
    /// kolt's `script_label` in `theme.vl`, its `flex_row` a few lines up
    /// `styles.vl`. The table the members come from is the analyzed IMPL TABLE,
    /// the one source that already knows both halves — so this cannot drift
    /// from what a call at the same position would resolve to, and the
    /// playground reaches it unchanged.
    ///
    /// The B318 visibility bit is read off the impl BLOCK, which is what
    /// `export impl` marks, under the uncurated-module exemption: a module that
    /// has curated nothing offers everything, exactly as `Analyzer::
    /// is_exported_in` decides it everywhere else.
    ///
    /// Insertion: a nested-rule entry opens a body (`.hover() { }` — the head's
    /// own parens, then the block), a plain method ends its item (`.raw();`),
    /// because those are the two shapes the grammar admits after a dotted head.
    fn css_dotted_head_completions(&self, analyzed_offset: usize) -> Vec<Completion> {
        let program = self.program;
        let mut items: Vec<Completion> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for (condition, _) in STYLE_CONDITION_METHODS
            .iter()
            .filter(|(condition, _)| *condition != "element")
        {
            seen.insert((*condition).to_string());
            let mut completion = Completion::bare((*condition).to_string(), CompletionKind::Method);
            completion.insert = Some(InsertText {
                text: format!("{condition}() {{ }}"),
                is_snippet: false,
            });
            items.push(completion);
        }
        // `Style` through the SCOPE at the cursor (E195): a `css` block reaches
        // it by import, so the file's own chain is the right answer — and a
        // program carrying a second `Style` (a dependency's, kolt's own) must
        // not have its methods offered here.
        let Some(style_id) = self.nominal_id_by_name("Style", analyzed_offset) else {
            return items;
        };
        for implementation in &program.implementations {
            if nominal_type_id(program, implementation.subject) != Some(style_id) {
                continue;
            }
            if program
                .curated_modules
                .contains(&implementation.module_scope)
                && !program.exported_entities.contains(&implementation.impl_id)
            {
                continue;
            }
            for (name, member_id) in &implementation.declarations {
                if !seen.insert((*name).to_string()) {
                    continue;
                }
                let mut completion =
                    self.entity_completion((*name).to_string(), *member_id, CompletionKind::Method);
                // A dotted head is a call in progress, so the shaping post-pass
                // must not add a second pair of parens on top of this one.
                completion.call_parameters = None;
                completion.insert = Some(InsertText {
                    text: format!("{name}();"),
                    is_snippet: false,
                });
                items.push(completion);
            }
        }
        items
    }
}

/// The ATTRIBUTE names offered undotted in `<tag |>` (E69): the tag's own,
/// then the globals every element takes, then — for an SVG-shaped tag — the
/// presentation attributes the SVG index gives the whole namespace.
///
/// The tag's own names come first because they are the ones the tag is FOR;
/// the client sorts by label within its own filter, but the playground ranks
/// by the order it is handed (`vilan-wasm`'s `boost`), and a `<input |>` whose
/// first offer is `accesskey` would be a worse answer than one whose first
/// offer is `accept`.
///
/// An unknown tag — a custom element, a mid-edit `<di|` — is not a refusal: it
/// has no own names and takes the globals, which is what every element takes.
fn attribute_completions(tag: &str) -> Vec<Completion> {
    use crate::html_attributes::{
        ELEMENT_ATTRIBUTES, GLOBAL_ATTRIBUTES, SVG_ELEMENTS, SVG_GLOBAL_ATTRIBUTES,
    };
    // The table is sorted by `(tag, attribute)`, so one binary search finds the
    // tag's first row and the run ends at the first row naming another tag.
    let first = ELEMENT_ATTRIBUTES.partition_point(|(element, _)| *element < tag);
    let own = ELEMENT_ATTRIBUTES[first..]
        .iter()
        .take_while(|(element, _)| *element == tag)
        .map(|(_, attribute)| *attribute);
    let wide: &[&str] = if SVG_ELEMENTS.binary_search(&tag).is_ok() {
        SVG_GLOBAL_ATTRIBUTES
    } else {
        &[]
    };
    let wide = wide.iter().copied();
    own.chain(GLOBAL_ATTRIBUTES.iter().copied())
        .chain(wide)
        .map(|attribute| {
            let mut completion = Completion::bare(attribute.to_string(), CompletionKind::Field);
            // An attribute takes exactly one value (`parse_element_head_item`
            // refuses a second), so the call shape is the one-parameter one —
            // and going through `call_parameters` rather than a snippet is what
            // makes accepting one honour the user's own
            // `vilan.completion.functionCall` setting and their client's
            // snippet support, exactly as a method link does.
            completion.call_parameters = Some(vec!["value".to_string()]);
            completion
        })
        .collect()
}

/// The `on:event(…)` candidates offered undotted in `<tag |>` (E69): one per
/// `GlobalEventHandlers` name, plus the bare `on:` template that was E67's
/// whole answer here.
///
/// Snippets, and deliberately: the event form is a *grammar* form whose body is
/// a closure, so what the author wants inserted is the closure too
/// (`on:click(|event| { … })`), and `CompletionKind::Snippet`'s `~`-prefixed
/// ranking is what keeps seventy-odd of them from burying the attribute names
/// at the same position.
///
/// The bare `on:` stays because the table is `GlobalEventHandlers` and a
/// CUSTOM event (`on:my-thing`) is still a legal head item — the desugar is
/// name-blind about events exactly as it is about attributes.
fn event_completions() -> Vec<Completion> {
    let mut items = vec![Completion::snippet(
        "on:",
        "an event handler",
        "on:${1:click}(|${2:event}| { $0 })",
        "on:",
    )];
    items.extend(crate::html_attributes::EVENTS.iter().map(|event| {
        Completion::snippet(
            &format!("on:{event}"),
            "an event handler",
            &format!("on:{event}(|${{1:event}}| {{ $0 }})"),
            &format!("on:{event}"),
        )
    }));
    items
}

/// The root of a raw parse, shared by the two sub-language worlds
/// [`Analysis::cursor_context`] classifies (an element head, a `css` body).
type RawRoot<'src> = vilan_core::Spanned<vilan_core::node::NodeList<'src>>;

/// The TAG NAME of the element whose OPENING TAG `offset` (LIVE space — see
/// [`Analysis::completion`]) sits in, where the desugar takes an attribute, an
/// `on:event(…)`, or a `.method(…)` chain link (element-syntax.md §2–4);
/// `None` when the cursor is not in a head at all.
///
/// The name travels with the answer because E69's attribute vocabulary is
/// per-element: `<input |>` and `<svg |>` offer different lists, and the tag
/// is the only thing that says which.
///
/// "In the head" is *after the tag name, before the head's `>`, and at the
/// head's own bracket depth*. The depth clause is what keeps this honest:
/// a head item's ARGUMENT is ordinary expression ground — the cursor in
/// `<form on:submit(|event| { print(client.add(x).| ) })>` is inside a
/// closure, three brackets deep, and belongs to E66's answer, not to this
/// one. It is also what makes the recovered shape safe to use, since a
/// flattened `<…>` error node spans the arguments too.
///
/// The token walk reads the LIVE buffer, like the rest of completion's
/// dispatch: the character being typed is live by nature. `tokens` is that
/// buffer's lexis, tokenized once by [`Analysis::cursor_context`] and shared
/// with the member test; `root` is that buffer's raw parse, shared with the
/// `css` body test.
fn element_head_tag<'text>(
    root: Option<&RawRoot<'_>>,
    text: &'text str,
    tokens: &[(Token<'_>, Span)],
    offset: usize,
) -> Option<&'text str> {
    let mut best: Option<(usize, std::ops::Range<usize>)> = None;
    if let Some(root) = root {
        for item in &root.0 {
            innermost_open_tag_end(item, offset, text, &mut best);
        }
    }
    let (_, tag) = best?;
    let mut depth = 0usize;
    for (token, span) in tokens {
        let range = span.into_range();
        if range.start < tag.end {
            continue;
        }
        if range.start >= offset {
            break;
        }
        match token {
            Token::Ctrl('(' | '[' | '{') => depth += 1,
            Token::Ctrl(')' | ']' | '}') => depth = depth.saturating_sub(1),
            // The head is already closed: the cursor is among the children.
            Token::Ctrl('>') if depth == 0 => return None,
            _ => {}
        }
    }
    (depth == 0).then(|| text.get(tag).unwrap_or_default())
}

/// Which of §7.1's positions `offset` (LIVE space) sits in, or `None` when the
/// cursor is not in a `css` body at all — which since A101 includes a
/// declaration's ARGUMENTS, because those are ordinary expression ground and
/// the cursor there belongs to ordinary expression completion.
///
/// Two questions, in the element head's own order. [`innermost_css_body_start`]
/// answers *which body* from the raw parse; the token walk below answers *where
/// in it*, at the body's OWN brace depth — a hole (`{…}`) and a condition head's
/// arguments are ordinary expression ground, and both are bracket-deep, so the
/// depth clause declines them exactly as the head's does.
///
/// Within one item the marker is the grammar's own: the leading `.` is the whole
/// declaration/combinator disambiguator (§3). A `;` starts the next item — and
/// so does a nested rule's closing `}`, which is why that one arm is guarded on
/// `dotted`.
fn css_position(
    root: Option<&RawRoot<'_>>,
    text: &str,
    tokens: &[(Token<'_>, Span)],
    offset: usize,
) -> Option<CssPosition> {
    let mut best: Option<(usize, usize)> = None;
    if let Some(root) = root {
        for item in &root.0 {
            innermost_css_body_start(item, offset, &mut best);
        }
    }
    // The parse stays the authority wherever it has an answer; a block still
    // being typed is the one shape it cannot have one for (E105).
    let body_start = match best {
        Some((_, body_start)) => body_start,
        None => unclosed_css_body_start(tokens, offset)?,
    };
    let mut depth = 0usize;
    let mut dotted = false;
    // The one place inside a head's arguments whose vocabulary is known (A95
    // S3): `combinator` is the dotted item's name, `on_head` says the cursor is
    // inside THAT name's argument list, and the two trailing tokens say whether
    // the cursor sits where a condition value goes.
    let mut combinator: Option<&str> = None;
    let mut on_head = false;
    let mut previous: Option<&Token<'_>> = None;
    let mut last: Option<&Token<'_>> = None;
    for (token, span) in tokens {
        let range = span.into_range();
        if range.start < body_start {
            continue;
        }
        if range.start >= offset {
            break;
        }
        if on_head && depth == 1 {
            previous = last;
            last = Some(token);
        }
        if dotted && depth == 0 {
            if let Token::Ident(name) = token {
                combinator = Some(name);
            }
            if matches!(token, Token::Ctrl('(')) && combinator == Some("on") {
                on_head = true;
                previous = None;
                last = None;
            }
        }
        match token {
            Token::Ctrl('(' | '[' | '{') => depth += 1,
            // The body's own `}`: the cursor is past the block entirely.
            Token::Ctrl('}') if depth == 0 => return None,
            // A nested rule's `}` ends its item; a hole's does not.
            Token::Ctrl('}') if depth == 1 && dotted => {
                depth = 0;
                dotted = false;
            }
            Token::Ctrl(')' | ']' | '}') => depth = depth.saturating_sub(1),
            Token::Ctrl(';') if depth == 0 => {
                dotted = false;
                combinator = None;
                on_head = false;
            }
            Token::Ctrl('.') if depth == 0 => dotted = true,
            _ => {}
        }
    }
    if depth == 1 && on_head && at_condition_value(previous, last) {
        return Some(CssPosition::ConditionValue);
    }
    if depth != 0 {
        return None;
    }
    if dotted {
        return Some(CssPosition::DottedHead);
    }
    // A property name is a span-adjacent `name`-`-`-`name` run, so the name
    // being typed reaches back over hyphens as well as identifier bytes — which
    // is what tells `--brand-|` (the custom-property row) from `flex-|` (an
    // ordinary hyphenated property, whose prefix the editor filters on).
    let bytes = text.as_bytes();
    let mut name_start = offset.min(bytes.len());
    while name_start > body_start
        && (is_identifier_byte(bytes[name_start - 1]) || bytes[name_start - 1] == b'-')
    {
        name_start -= 1;
    }
    Some(if text[name_start..].starts_with("--") {
        CssPosition::CustomProperty
    } else {
        CssPosition::Property
    })
}

/// Whether the cursor sits where a condition VALUE goes inside an `on` head:
/// directly after the head's `(`, after a `+`, or partway through the name that
/// follows either. `previous` and `last` are the two tokens before the cursor at
/// the head's own depth, youngest last.
fn at_condition_value(previous: Option<&Token<'_>>, last: Option<&Token<'_>>) -> bool {
    let opens = |token: Option<&Token<'_>>| {
        matches!(token, None | Some(Token::Ctrl('(')) | Some(Token::Op("+")))
    };
    match last {
        Some(Token::Ident(_)) => opens(previous),
        other => opens(other),
    }
}

/// The body start (one past the `{`) of the innermost `css` block body
/// containing `offset` — the css twin of [`innermost_open_tag_end`], and E67's
/// pattern verbatim: read from a RAW parse, because the css desugar retires
/// `Node::Css` before analysis exactly as the element desugar retires
/// `Node::Element`.
///
/// A nested rule's body is a body too, so the walk descends `CssBody`'s items
/// as well as the ordinary expression children `for_each_child` reaches — a
/// cursor inside `.md { … }` belongs to the RULE's body, not the outer one.
/// The narrowest body containing the offset wins, which is what "innermost"
/// means when they nest.
///
/// The body parser COMMITS (`parsing.rs::parse_css_body`), so a half-typed item
/// leaves the block's own `Node::Css` in the tree with the items around the
/// mistake intact — which is why this needs no `Node::Error` arm of the kind
/// the element head's recovery does. A block that never closes at all declines
/// its atom and leaves no node; [`unclosed_css_body_start`] answers that shape
/// from the live lexis instead.
fn innermost_css_body_start(
    node: &vilan_core::Spanned<vilan_core::node::Node<'_>>,
    offset: usize,
    best: &mut Option<(usize, usize)>,
) {
    use vilan_core::node::Node;
    if let Node::Css(body) = &node.0 {
        css_body_start(body, offset, best);
    }
    node.0
        .for_each_child(&mut |child| innermost_css_body_start(child, offset, best));
}

fn css_body_start(
    body: &vilan_core::node::CssBody<'_>,
    offset: usize,
    best: &mut Option<(usize, usize)>,
) {
    use vilan_core::node::CssItem;
    let range = body.braces.into_range();
    // Strictly inside the braces: the cursor ON the `{` is not in the body yet.
    if range.start < offset
        && offset < range.end
        && best.is_none_or(|(width, _)| range.end - range.start <= width)
    {
        *best = Some((range.end - range.start, range.start + 1));
    }
    for item in &body.items {
        if let CssItem::Nested(nested) = item {
            css_body_start(&nested.body, offset, best);
        }
    }
}

/// The body start (one past the `{`) of the innermost `css` body still OPEN at
/// `offset` — the mid-edit shape [`innermost_css_body_start`] cannot answer,
/// because a block whose `}` has not been typed leaves no node to read (E105).
///
/// Read from the LIVE buffer's lexis for the same reason the position walk
/// above it is: text the author has not finished writing is in no parse tree,
/// and `css {` with the block still open is exactly that. The parse is not
/// second-guessed — this is consulted only where it has nothing to say — and
/// the parser is left alone deliberately: `parse_css_atom` declines an unclosed
/// block because the region has no end, so its span is not a fact about the
/// program, and the statement recovery's located `unclosed \`{\`` at the opener
/// is the diagnostic the author needs. Minting a node to end-of-input would
/// trade that message, and hand the desugar a block nobody has finished, to
/// answer a question the editor can answer for itself.
///
/// One pass with a stack of open brackets, each remembering whether it is a css
/// BODY. Two markers decide, and they are the grammar's own (§3): a block's own
/// `{` is the one directly after the `css` keyword, and a nested rule's is one
/// opened while the enclosing body's item has taken the `.` that commits it to
/// a condition. Every other `{` — a hole, a condition head's argument, an
/// ordinary block — is not a body, so a cursor inside one is not in css
/// position and the walk says so by leaving a non-body on top of the stack.
///
/// Comments and string bodies cannot plant a phantom `css {` here: comments are
/// trivia and never reach the token stream, and a string is one token.
fn unclosed_css_body_start(tokens: &[(Token<'_>, Span)], offset: usize) -> Option<usize> {
    /// One open bracket: where its css body starts (`None` when it is not a
    /// body), and the enclosing item's `dotted` state to restore when it closes.
    struct OpenBracket {
        body_start: Option<usize>,
        enclosing_dotted: bool,
    }
    let mut open: Vec<OpenBracket> = Vec::new();
    // The innermost body's current item: whether it has taken the `.`, and
    // whether it is past the `:` that separates a property from its value (a
    // `.` in a value commits nothing — the same guard the position walk uses).
    let mut dotted = false;
    let mut after_colon = false;
    let mut previous_was_css = false;
    for (token, span) in tokens {
        let range = span.into_range();
        if range.start >= offset {
            break;
        }
        let in_css_body = open
            .last()
            .is_some_and(|bracket| bracket.body_start.is_some());
        match token {
            Token::Ctrl('{') => {
                let body_start = if previous_was_css || (in_css_body && dotted) {
                    Some(range.end)
                } else {
                    None
                };
                open.push(OpenBracket {
                    body_start,
                    enclosing_dotted: dotted,
                });
                dotted = false;
                after_colon = false;
            }
            Token::Ctrl('(' | '[') => {
                open.push(OpenBracket {
                    body_start: None,
                    enclosing_dotted: dotted,
                });
                dotted = false;
                after_colon = false;
            }
            Token::Ctrl(')' | ']' | '}') => {
                if let Some(bracket) = open.pop() {
                    dotted = bracket.enclosing_dotted;
                    after_colon = false;
                }
            }
            Token::Ctrl(';') if in_css_body => {
                dotted = false;
                after_colon = false;
            }
            // The declaration's separator is an OPERATOR token, not a control
            // one (`parse_css_declaration` reads it with `peek_is_op`).
            Token::Op(":") if in_css_body => after_colon = true,
            Token::Ctrl('.') if in_css_body && !after_colon => dotted = true,
            _ => {}
        }
        previous_was_css = matches!(token, Token::Css);
    }
    open.last().and_then(|bracket| bracket.body_start)
}

/// Where in a `css` block's body the cursor is (css-block.md §7.1) — the four
/// positions, and the four answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CssPosition {
    /// An item's head, undotted: a CSS property name is being written.
    Property,
    /// The same head on a CUSTOM property (`--|`). Its vocabulary is the custom
    /// properties declared in this build, which is nothing in v1 (Q4) — and it
    /// is deliberately not the standard list, which no `--` name can match.
    CustomProperty,
    /// An item's head after the `.` — a condition combinator or any other
    /// `impl Style` method (E183; renamed from `Condition`, which named only
    /// half of what the position admits).
    DottedHead,
    /// Inside an `.on(<set>)` head, at a place a condition VALUE goes: directly
    /// after the `(`, or after a `+` (A95 S3). The head's arguments are
    /// ordinary expression ground everywhere else, and this is the one place in
    /// one head where the vocabulary is known — the same rows the combinator
    /// list reads, as the free constructors the set is built from.
    ConditionValue,
}

/// What the cursor is IN — the classification [`Analysis::completion`]
/// dispatches on, and the general answer to kolt.local 001.
///
/// The engine used to read the two bytes immediately before the cursor and
/// dispatch on them. That made it blind along two axes at once, and every face
/// the owner reported was one of them: TRIVIA (a `.` that started the next line
/// was not a member position, and a space before the `.` turned one into a bare
/// scope position offering all eighty names in scope), and the difference
/// between CODE and TEXT (a string body was not distinguished from an
/// expression, so a caption offered every function in scope). Asking the
/// question once, in one place, is what stops the next face being a fourth
/// patch — the three earlier faces (E66's call receiver, E67's element head)
/// each arrived as their own branch of the same byte-pair dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CursorContext<'text> {
    /// Not a code position at all — inside a string literal's body, or inside a
    /// `//` comment. Nothing is offered.
    NoCode,
    /// A macro name: `[Na…` at an item position, or inside `[derive(…)`.
    MacroName,
    /// Inside an element's opening tag (E67). `chain` is the `.` that commits
    /// the head item to the chain form rather than to an attribute; `tag` is
    /// the element's own name, which is what picks E69's attribute list.
    ElementHead { chain: bool, tag: &'text str },
    /// Inside a `css` block's own body (css-block.md §7.1) — a second
    /// sub-language world, and the same shape as the first: the block is
    /// desugared before analysis, so nothing in scope belongs here.
    CssBlock(CssPosition),
    /// Inside an import path (E57) — names come from the package tree.
    ImportPath,
    /// A member position: after a `.`, whatever trivia surrounds it and whatever
    /// follows the cursor. `receiver_end` is one past the receiver's last byte
    /// (which is where the pre-001 dispatch simply assumed the `.` was), and
    /// `lifted` marks the `?.` form (proposal/try-and-lift.md §5).
    Member { receiver_end: usize, lifted: bool },
    /// A `::` path position; `path_start` is the first `:`.
    Path { path_start: usize },
    /// A FIELD position inside a struct initializer (E160): `KoltStore { us▎`.
    /// `struct_id` is the struct being constructed and `open` is the token
    /// index of its `{`, from which the already-written field names are read
    /// ([`Analysis::struct_initializer_completions`]) — an index rather than
    /// the names themselves so this classifier stays `Copy`, like every arm
    /// beside it.
    StructInitializer { struct_id: Id, open: usize },
    /// An ordinary expression position: the names in scope.
    Expression,
}

/// The member context at `start` — the cursor with the partial identifier being
/// typed scanned off — or `None` when the cursor is not after a `.`.
///
/// Read in TOKEN space, which is the whole of what makes it trivia-blind: the
/// lexer already knows that whitespace and `//` comments are not significant, so
/// asking it for "the last thing that ended before here" needs no second notion
/// of trivia to drift from the first. The receiver is likewise the token before
/// the `.` rather than the byte before it, which is what lets a chain be written
/// down the page (`p\n\t\t.|`).
fn member_context<'text>(tokens: &[(Token, Span)], start: usize) -> Option<CursorContext<'text>> {
    let dot = tokens
        .iter()
        .rposition(|(_, span)| span.into_range().end <= start)?;
    if !matches!(tokens[dot].0, Token::Ctrl('.')) {
        return None;
    }
    let before = dot.checked_sub(1)?;
    // `?.` lexes as two tokens (`lexing.rs`: "`.` splits `?.`/`..` apart"), so
    // the lifted form is the `?` sitting between the receiver and the dot.
    let lifted = matches!(tokens[before].0, Token::Op("?"));
    let receiver = if lifted {
        before.checked_sub(1)?
    } else {
        before
    };
    Some(CursorContext::Member {
        receiver_end: tokens[receiver].1.into_range().end,
        lifted,
    })
}

/// The struct-initializer FIELD position at `start` (E160): the token index of
/// the initializer's `{` and the token index of its head NAME, or `None` when
/// the cursor is not at one.
///
/// Read in TOKEN space for [`member_context`]'s reason — the lexer already
/// decides what is trivia, so a `{` inside a string or a `//` comment is not a
/// bracket here at all. Three clauses, each one clause of the grammar
/// `parsing.rs::parse_struct_initializer` accepts:
///
/// 1. The cursor is inside an UNCLOSED `{ … }`, found by walking back with a
///    depth counter — so a nested initializer, call or list resolves to the
///    INNERMOST brace, which is the one being typed in.
/// 2. That brace's head is a name: `Name {`, `a::b::Name {` or `Name<Args> {`
///    (the argument list is walked back over `<`/`>`, which the lexer always
///    emits as single control tokens — there is no fused `>>`). The CALLER
///    resolves that name against the program, and that is what keeps an
///    ordinary block out: `fun f() {` and `match x {` have no struct name
///    before the brace, and the two shapes that do — `struct Point {`,
///    `impl Point {` — are declined by [`head_is_not_an_initializer`].
/// 3. The cursor is at a FIELD position and not a VALUE one: the
///    comma-separated run it sits in carries no `=` yet. `Point { x = p|` is a
///    value position, and falls through to the ordinary gatherers so the
///    expression being written there completes normally.
fn struct_initializer_head(
    tokens: &[(Token<'_>, Span)],
    start: usize,
) -> Option<(usize, std::ops::RangeInclusive<usize>)> {
    let mut index = tokens
        .iter()
        .rposition(|(_, span)| span.into_range().end <= start)?;
    let mut depth = 0usize;
    // Whether the walk is still inside the run the cursor sits in. Only there
    // does an `=` mean "the cursor is past the field name"; a `=` in an
    // earlier field's value says nothing about this one.
    let mut in_cursor_run = true;
    let open = loop {
        match tokens[index].0 {
            Token::Ctrl(')' | ']' | '}') => depth += 1,
            Token::Ctrl('(' | '[') => depth = depth.checked_sub(1)?,
            Token::Ctrl('{') => {
                if depth == 0 {
                    break index;
                }
                depth -= 1;
            }
            // A field list has no statements in it; a `;` at this depth means
            // the enclosing brace is a block.
            Token::Ctrl(';') if depth == 0 => return None,
            Token::Ctrl(',') if depth == 0 => in_cursor_run = false,
            // Exactly `=`: `==`, `+=` and `=>` are their own tokens.
            Token::Op("=") if depth == 0 && in_cursor_run => return None,
            _ => {}
        }
        index = index.checked_sub(1)?;
    };
    let mut head = open.checked_sub(1)?;
    if matches!(tokens[head].0, Token::Ctrl('>')) {
        let mut angle = 0usize;
        loop {
            match tokens[head].0 {
                Token::Ctrl('>') => angle += 1,
                Token::Ctrl('<') => {
                    angle = angle.checked_sub(1)?;
                    if angle == 0 {
                        break;
                    }
                }
                _ => {}
            }
            head = head.checked_sub(1)?;
        }
        head = head.checked_sub(1)?;
    }
    if !matches!(tokens[head].0, Token::Ident(_)) {
        return None;
    }
    // Back over the rest of a qualified head (`a::b::Name {`) to its FIRST
    // segment, which is the token the guard below must look behind: `::`
    // precedes a qualified initializer and a qualified return type alike, so
    // one token of lookahead cannot tell `shapes::Dot { x = 1 }` from `fun
    // make(): shapes::Dot {`.
    let mut path_start = head;
    while path_start >= 2
        && matches!(tokens[path_start - 1].0, Token::Op("::"))
        && matches!(tokens[path_start - 2].0, Token::Ident(_))
    {
        path_start -= 2;
    }
    if path_start
        .checked_sub(1)
        .is_some_and(|previous| head_is_not_an_initializer(&tokens[previous].0))
    {
        return None;
    }
    Some((open, path_start..=head))
}

/// The field names already written in the initializer opened at token `open`
/// (E160) — every comma-separated run's leading identifier, from the `{` to its
/// matching `}` or to the end of the buffer when it is still unclosed.
///
/// The run the cursor is IN is excluded: its name is the prefix being typed
/// (`KoltStore { us▎`), and while retyping a written one (`KoltStore { use▎r }`)
/// that field is still the candidate the author wants.
fn struct_initializer_written<'src>(
    tokens: &[(Token<'src>, Span)],
    open: usize,
    start: usize,
) -> HashSet<&'src str> {
    let mut written = HashSet::new();
    let mut depth = 0usize;
    let mut at_run_start = true;
    for (token, span) in &tokens[open + 1..] {
        match token {
            Token::Ctrl('(' | '[' | '{') => depth += 1,
            Token::Ctrl(')' | ']') => depth = depth.saturating_sub(1),
            Token::Ctrl('}') => {
                if depth == 0 {
                    break;
                }
                depth -= 1;
            }
            Token::Ctrl(',') if depth == 0 => {
                at_run_start = true;
                continue;
            }
            Token::Ident(name) if depth == 0 && at_run_start => {
                let range = span.into_range();
                if !(range.start <= start && start <= range.end) {
                    written.insert(*name);
                }
            }
            _ => {}
        }
        at_run_start = false;
    }
    written
}

/// Whether the token BEFORE a struct-named head puts that head in a
/// declaration or a type position, so the `{` after it opens a body and not a
/// field list (E160).
///
/// Three families, and each one really collides — the head names a struct in
/// all of them:
///
/// - `struct Point {`, `impl Point {`: the declaration and its impl block.
/// - `:` — every TYPE position, and the common one is a function whose return
///   type is the struct it builds (`fun store_for(…): KoltStore {`, kolt
///   `store.vl:263`, whose body's first line is the initializer this context
///   exists for).
/// - the control-flow words, because a condition parses in the parser's
///   `no_struct` mode (`parsing.rs::parse_chain_head`), where a `{` after a
///   bare name is a block BY RULE and not by what the name happens to denote.
fn head_is_not_an_initializer(token: &Token<'_>) -> bool {
    matches!(
        token,
        Token::Op(":")
            | Token::Struct
            | Token::Enum
            | Token::Trait
            | Token::Impl
            | Token::With
            | Token::If
            | Token::Else
            | Token::Match
            | Token::For
            | Token::In
            | Token::Is
    )
}

/// Whether `offset` (LIVE space) is TEXT rather than code — inside a string
/// literal's body, or inside a `//` comment. Both are the same answer to
/// completion (nothing at all), and neither was visible to the byte-pair
/// dispatch that preceded the context model.
///
/// A string is read off the lexer's own tokens, so there is no second notion of
/// where one ends. The NARROWEST containing token decides, which is what keeps
/// an interpolation HOLE a code position: `i"…"`'s wrapper tokens all carry the
/// whole literal's span while the hole's tokens carry their own
/// (`lexing.rs::emit_interpolated`), so the narrowest span containing the cursor
/// is the hole's where the cursor is in one and the literal's where it is not.
/// A cursor exactly ON a delimiter is outside — containment is strict, so
/// `|"a"` and `"a"|` are both code.
fn in_no_code_position(text: &str, tokens: &[(Token, Span)], offset: usize) -> bool {
    let mut narrowest: Option<usize> = None;
    let mut narrowest_is_string = false;
    for (token, span) in tokens {
        let range = span.into_range();
        if range.start >= offset || offset >= range.end {
            continue;
        }
        let width = range.end - range.start;
        let is_string = matches!(token, Token::String(_) | Token::MultilineString(_));
        match narrowest {
            Some(best) if best < width => {}
            Some(best) if best == width => narrowest_is_string |= is_string,
            _ => {
                narrowest = Some(width);
                narrowest_is_string = is_string;
            }
        }
    }
    narrowest_is_string || in_line_comment(text, tokens, offset)
}

/// Whether `offset` sits after a `//` on its own line. A comment leaves no
/// token to read — the lexer discards it as trivia — so this is the one place
/// the classifier scans bytes for itself, and the token spans are still what
/// rule out a `//` that is inside a string (`"http://…"` opens no comment).
fn in_line_comment(text: &str, tokens: &[(Token, Span)], offset: usize) -> bool {
    let cursor = offset.min(text.len());
    let line_start = text[..cursor].rfind('\n').map(|at| at + 1).unwrap_or(0);
    text[line_start..cursor].match_indices("//").any(|(at, _)| {
        let position = line_start + at;
        !tokens.iter().any(|(token, span)| {
            matches!(token, Token::String(_) | Token::MultilineString(_))
                && span.into_range().start < position
                && position < span.into_range().end
        })
    })
}

impl<'a, 'src> Analysis<'a, 'src> {
    /// Completion candidates at `offset` — a LIVE-space offset (the caller
    /// converts an LSP `Position` through `line_index`, never `analyzed_offset`:
    /// completion's dispatch reads the buffer the user is mid-keystroke in).
    /// Dispatched by the syntax just before the cursor: members after `.`, path
    /// items after `::`, else names in scope plus keywords. The editor filters
    /// the list by whatever prefix is being typed.
    ///
    /// The trigger scan below (`start`, the `.`/`?.`/`::` check, the
    /// open-paren/import sniffs) legitimately stays in LIVE space throughout —
    /// it is reading the character the user just typed. But every candidate
    /// gatherer that walks `program` data (`scope_completions`, and
    /// `member_completions`/`lifted_member_completions` by way of
    /// `receiver_nominal_id`) must resolve its scope/entity in ANALYZED space,
    /// via `to_analyzed_offset` — or a scope or receiver resolved from the live
    /// offset against a stale program answers the wrong question the moment the
    /// two snapshots diverge (E52).
    pub fn completion(&self, offset: usize) -> Vec<Completion> {
        let text = self.live.text();
        let bytes = text.as_bytes();
        // Scan back over the partial identifier the user is typing to reach the
        // syntactic context (`.`, `::`, or open scope) that drives the candidates.
        let mut start = offset.min(bytes.len());
        while start > 0 && is_identifier_byte(bytes[start - 1]) {
            start -= 1;
        }
        let (tokens, _errors) = tokenize(text);
        let context = self.cursor_context(text, &tokens, offset, start);
        // E211: the prefix this request's candidates REPLACE, which is what
        // every front-end filters against once the server states it. The
        // hyphenated positions get the wider word — a css property, an element
        // attribute and a custom property are one name each, and E194's whole
        // defect was a client reading `stroke-w` as `w`.
        let replaced = match context {
            CursorContext::CssBlock(_) | CursorContext::ElementHead { .. } => {
                Span::from(hyphenated_word_start(bytes, offset)..offset)
            }
            _ => Span::from(start..offset),
        };
        match context {
            // Text, not code (kolt.local 001): a name here is a caption or a
            // note, and every candidate would be wrong.
            CursorContext::NoCode => return Vec::new(),
            // Macro names are always bare, so they bypass the call-suppression
            // below.
            CursorContext::MacroName => {
                return stamp_replacements(self.macro_name_completions(), replaced);
            }
            CursorContext::ElementHead { chain, tag } => {
                return stamp_replacements(self.element_head_completions(chain, tag), replaced);
            }
            CursorContext::CssBlock(position) => {
                return stamp_replacements(self.css_block_completions(position, offset), replaced);
            }
            // A field position offers the struct's fields and NOTHING else
            // (E160) — the element head's rule, for the element head's reason:
            // a name in scope is not a field name, and the one thing the author
            // is typing is the latter.
            CursorContext::StructInitializer { struct_id, open } => {
                return stamp_replacements(
                    self.struct_initializer_completions(&tokens, struct_id, open, offset, start),
                    replaced,
                );
            }
            _ => {}
        }
        // An import path takes names from the package tree, and it also shapes
        // the post-pass below (no snippets, no call-shaped insertion).
        let in_import = context == CursorContext::ImportPath;
        let mut candidates = match context {
            CursorContext::ImportPath => self.import_completions(text, offset),
            // `a?.` completes on the LIFTED element (`Option<Profile>` offers
            // Profile's members — proposal/try-and-lift.md §5).
            CursorContext::Member {
                receiver_end,
                lifted: true,
            } => self.lifted_member_completions(&tokens, receiver_end),
            CursorContext::Member {
                receiver_end,
                lifted: false,
            } => self.member_completions(&tokens, receiver_end),
            CursorContext::Path { path_start } => self.code_path_completions(text, path_start),
            _ => {
                // An ordinary expression position: the cursor's own scope,
                // resolved in ANALYZED space (E52) — `path_completions` needs
                // no such conversion, since it answers by NAME across the whole
                // program rather than by scope containment.
                let mut scope_candidates = self.scope_completions(self.to_analyzed_offset(offset));
                // E54c: importable-but-unimported names, LABELED and
                // edit-carrying (E53's rule stands — nothing here is silent).
                // Only at this bare scope position; a `.`/`::` receiver is
                // already resolved to something in scope, so there is nothing to
                // import there.
                let in_scope: HashSet<&str> = scope_candidates
                    .iter()
                    .map(|candidate| candidate.label.as_str())
                    .collect();
                scope_candidates.extend(self.auto_import_completions(&in_scope));
                scope_candidates
            }
        };
        // A call-shaped insertion is wrong when the callee is already
        // parenthesized — the char right after the cursor is `(`, so the user
        // pre-typed the parens or is retyping a call — or when a name is being
        // imported, not called (`use std::math::sqrt`). Fall back to a bare name
        // for every candidate; the signature and docs still show (WO-3 escape
        // hatches).
        let next_is_open_paren = bytes.get(offset).copied() == Some(b'(');
        // An import path takes names, so the construct snippets (`for …`,
        // `fun …`) have no business there — drop them entirely (E14). Import
        // completion no longer produces any (it never reaches
        // `scope_completions`), so this now only guards the invariant.
        if in_import {
            candidates.retain(|candidate| !matches!(candidate.kind, CompletionKind::Snippet));
        }
        if next_is_open_paren || in_import {
            for candidate in &mut candidates {
                candidate.call_parameters = None;
            }
        }
        // E213: an `[internal("…")]` name is reachable and not a name to reach
        // for, so it is not in the list somebody is scanning — it is in the
        // list somebody has already half-typed. Three characters, matching
        // exactly, is the line: shorter than that and the popup is a browse,
        // and a name the author is spelling out is a name they mean.
        //
        // Applied HERE, once, rather than at each gatherer: every candidate
        // carries the label, so one rule covers the scope position, the `.`
        // position and the `::` one. The struct-INITIALIZER position returns
        // before this deliberately — a literal must name every field, so
        // hiding one there would hide the way to construct the value.
        let typed = text.get(start..offset).unwrap_or_default();
        candidates.retain(|candidate| match &candidate.internal {
            Some(_) => typed.chars().count() >= 3 && candidate.label.starts_with(typed),
            None => true,
        });
        stamp_replacements(candidates, replaced)
    }

    /// What the cursor is IN — the one classification every completion path
    /// consults (kolt.local 001).
    ///
    /// `offset` is the cursor and `start` is the cursor with the partial
    /// identifier being typed scanned off; both are LIVE space (see
    /// [`Self::completion`]), and `tokens` is the live text's lexis. The order
    /// below is a precedence, not a sequence of guesses: text outranks every
    /// syntactic trigger, the two worlds that suppress scope entirely
    /// (an element head, an import path) outrank the ordinary triggers, and the
    /// bare expression position is what is left.
    ///
    /// Trivia is the axis this used to be blind to. The member test reads TOKEN
    /// space, so the lexer's own notion of what is significant decides where the
    /// receiver and the `.` are — `p.`, `p .`, `p\n\t.` and `p // note\n.` are
    /// one position, and what FOLLOWS the cursor never enters the question at
    /// all (`a.|.b` is the `a.` position).
    fn cursor_context<'text>(
        &self,
        text: &'text str,
        tokens: &[(Token<'_>, Span)],
        offset: usize,
        start: usize,
    ) -> CursorContext<'text> {
        let bytes = text.as_bytes();
        // Text, not code. First, because every trigger below reads characters
        // that mean nothing inside one: a `.` in a caption is not a member
        // access, and a name in a comment is prose.
        if in_no_code_position(text, tokens, offset) {
            return CursorContext::NoCode;
        }
        // `[Na|` at an item position (the line holds only the attribute so far)
        // and `[derive(Na|` complete registered macro names — the last piece of
        // the macro-LSP tail.
        if start >= 1 && bytes[start - 1] == b'[' {
            let line_start = text[..start - 1].rfind('\n').map(|at| at + 1).unwrap_or(0);
            if text[line_start..start - 1].trim().is_empty() {
                return CursorContext::MacroName;
            }
        }
        if start >= 1 && bytes[start - 1] == b'(' && text[..start - 1].ends_with("[derive") {
            return CursorContext::MacroName;
        }
        // ONE raw parse serves both sub-language worlds below. Element syntax
        // and the `css` block are each desugared before analysis, so neither
        // survives into `program` and each is only ever seen through a raw
        // parse — and both are in the same tree, so it is parsed once.
        //
        // …and only where either world can possibly contain the cursor (M29).
        // The parse is a whole-buffer one and it was paid on EVERY completion,
        // in a file with no element and no `css` in it at all — 0.14 ms of the
        // 0.55 ms an ordinary scope completion cost. The test below is exact,
        // not a guess: both consumers look for an item that ENCLOSES the
        // cursor, an element item begins with a `<` and a `css` block with the
        // `css` keyword, so with neither token anywhere before the cursor there
        // is no such item for the parse to find. `css_position`'s own
        // token-only fallback for a block still being typed (E105) is reached
        // through the same `css` token, so it is not lost either.
        let sub_language = tokens.iter().any(|(token, span)| {
            span.into_range().start < offset && matches!(token, Token::Ctrl('<') | Token::Css)
        });
        let raw = sub_language
            .then(|| vilan_core::parsing::parse(text).0)
            .flatten();
        // An element's opening tag is its own world (E67): between `<div` and
        // `>` the desugar takes an attribute, an `on:event(…)` or a `.method(…)`
        // chain link — and nothing that is merely in scope. The check runs from
        // `start` (the head item being typed), and the `.` just before it is the
        // same disambiguator the grammar uses.
        if let Some(tag) = element_head_tag(raw.as_ref(), text, tokens, start) {
            return CursorContext::ElementHead {
                chain: start >= 1 && bytes[start - 1] == b'.',
                tag,
            };
        }
        // A `css` block's body is the second such world (css-block.md §7.1),
        // and it is checked after the head for the reason the two can nest: a
        // block written in an element's head (`<div .styled(css { … })>`) sits
        // inside a head ARGUMENT, which the head's own depth clause already
        // declines.
        if let Some(position) = css_position(raw.as_ref(), text, tokens, start) {
            return CursorContext::CssBlock(position);
        }
        // An import path is its own world too (E57): a name there is being
        // reached FOR THE FIRST TIME, so none of the in-scope machinery applies
        // — candidates come from the package tree, and the head of the path
        // names an origin (`std`, `pkg`, a dependency), which is not an entity
        // at all.
        if in_import_path(text, offset) {
            return CursorContext::ImportPath;
        }
        if let Some(member) = member_context(tokens, start) {
            return member;
        }
        if start >= 2 && bytes[start - 1] == b':' && bytes[start - 2] == b':' {
            return CursorContext::Path {
                path_start: start - 2,
            };
        }
        // A struct initializer's FIELD position (E160), last of the syntactic
        // triggers because it is the widest: it is a claim about the enclosing
        // brace rather than about the character just before the cursor, and a
        // `.` or a `::` written inside one is still the member or path position
        // it looks like (`Point { x = origin.|` completes `origin`'s members).
        // What it does outrank is the bare scope position, which is the whole
        // defect — `KoltStore { us|` used to list every binding in scope.
        if let Some(context) = self.struct_initializer_context(tokens, offset, start) {
            return context;
        }
        CursorContext::Expression
    }

    /// The candidates for an element's head (E67, E69). `chain` says the cursor
    /// follows a `.`, so the head item under construction is a chain link;
    /// `tag` is the element's own name, which picks the attribute list.
    ///
    /// Every half comes from a gated source, so none can drift: the chain
    /// form's vocabulary is the `View` type's method set, read from the std
    /// declaration the program compiles against; the ATTRIBUTE and EVENT
    /// vocabularies are [`crate::html_attributes`], generated from the WHATWG
    /// and SVG attribute indices and held to their vendored extract by
    /// `html_attributes_sync`.
    ///
    /// E67 left the attribute names out and said why: a hand list "would be a
    /// second source of truth with nothing to gate it". E69's ruling is the
    /// answer to exactly that — GENERATED and GATED — and nothing else about
    /// the head changed: the desugar stays NAME-BLIND (element-syntax.md §2,
    /// §9 item 3), so a `data-tip(…)` or an `aria-` name this table has never
    /// heard of is written and lowered exactly as before. Completion is an
    /// offer, never a vocabulary.
    ///
    /// What the head position still stops offering is the enclosing scope: not
    /// one binding, type, keyword or construct snippet may appear between
    /// `<div` and `>`.
    fn element_head_completions(&self, chain: bool, tag: &str) -> Vec<Completion> {
        let mut items = Vec::new();
        if let Some(view_id) = self.element_view_nominal_id() {
            self.push_methods(view_id, true, &mut items);
            if !chain {
                // Undotted: the chain form is offered in its own spelling, dot
                // included, because an undotted `text(…)` is an ATTRIBUTE named
                // "text" — a different construct, and the one §4's warning
                // exists to catch.
                for item in &mut items {
                    item.label = format!(".{}", item.label);
                }
            }
        }
        if !chain {
            items.extend(attribute_completions(tag));
            items.extend(event_completions());
        }
        items
    }

    /// The struct-initializer field position at `start`, with the head resolved
    /// against the program (E160, E193). `None` when the token walk finds no
    /// initializer, or when its head names no struct — which is how a block
    /// whose head happens to be an identifier (`match value {`, `for x in xs
    /// {`) declines without the classifier needing a parse.
    ///
    /// The head resolves through the SCOPE CHAIN and, for the qualified form,
    /// through B190's `type-path` (E193). E160 looked the last segment up by
    /// name over the whole program and took the first struct that matched,
    /// which is right only while one spelling means one struct: a file with its
    /// own `struct Dot` beside a sibling's was offered the SIBLING's fields at
    /// `Dot { ▎`, and `shapes::Dot { ▎` ignored the namespace it was told and
    /// answered whichever `Dot` the program recorded first.
    ///
    /// The generic ARGUMENTS play no part, here or below: the fields are the
    /// DECLARATION's, and `Holder<i32> { … }` and `Holder<str> { … }` name the
    /// same ones (`struct_initializer_head` walks back over them for exactly
    /// that reason).
    ///
    /// The program-wide scan survives as a LAST resort, and only as one: a
    /// buffer mid-edit may not have bound the name yet — a freshly typed
    /// `struct` above the cursor, a file whose scopes did not survive the
    /// analysis — and answering with the declaration that exists beats
    /// answering with nothing, which is what this position did before E160.
    fn struct_initializer_context<'text>(
        &self,
        tokens: &[(Token<'_>, Span)],
        offset: usize,
        start: usize,
    ) -> Option<CursorContext<'text>> {
        let (open, path) = struct_initializer_head(tokens, start)?;
        // `a :: b :: Name` — the idents at the even offsets of the run.
        let mut segments: Vec<&str> = Vec::new();
        for index in path.clone().step_by(2) {
            let Token::Ident(segment) = tokens[index].0 else {
                return None;
            };
            segments.push(segment);
        }
        let (&name, namespace) = segments.split_last()?;
        let analyzed_offset = self.to_analyzed_offset(offset);
        let struct_id = self
            .path_struct_id(namespace, name, analyzed_offset)
            .or_else(|| {
                // Nothing in scope answers: the program-wide first match, which
                // is all E160 ever asked and is still better than silence.
                self.program
                    .structs
                    .iter()
                    .find(|(_, structure)| structure.name == name)
                    .map(|(id, _)| *id)
            })?;
        Some(CursorContext::StructInitializer { struct_id, open })
    }

    /// The struct a written head names, resolved the way the analyzer resolves
    /// it (E193): the leading segment out of the SCOPE at the cursor, each
    /// further segment out of the namespace before it, and the last one held to
    /// being a struct.
    ///
    /// `None` when any step fails, which is a decline and not a guess — the
    /// caller's fallback is what decides whether a guess is better than silence.
    fn path_struct_id(&self, namespace: &[&str], name: &str, analyzed_offset: usize) -> Option<Id> {
        self.path_entity_id(namespace, name, analyzed_offset)
            .filter(|id| self.program.structs.contains_key(id))
    }

    /// The entity a written path names, resolved the way the analyzer resolves
    /// it (E193): the leading segment out of the SCOPE at the cursor, each
    /// further segment out of the namespace before it. No filter on what it
    /// turns out to be — [`Self::path_struct_id`] and
    /// [`Self::nominal_id_by_name`] each apply their own, which is the only
    /// thing that differed between the two spellings of this walk (E195).
    fn path_entity_id(&self, namespace: &[&str], name: &str, analyzed_offset: usize) -> Option<Id> {
        let Some((first, rest)) = namespace.split_first() else {
            // Unqualified: the binding this file's scope chain gives the name.
            return self.binding_in_scope(name, analyzed_offset);
        };
        let mut current = self.namespace_in_scope(first, analyzed_offset)?;
        for segment in rest {
            current = self.namespace_entry_id(current, segment)?;
        }
        self.namespace_entry_id(current, name)
    }

    /// The candidates at a struct-initializer field position (E160): the
    /// struct's fields minus the ones already written, each carrying its
    /// declared type as the popup's detail and its own `///` first paragraph as
    /// the documentation.
    ///
    /// Accepting one writes `name = ` — the author's next keystrokes — except
    /// where a binding of the same name is in scope, which is the SHORTHAND
    /// (`Point { x }` is `Point { x = x }`) and is what kolt's `store.vl:265`
    /// writes for four of its five fields. That is the one place the two lists
    /// this context used to confuse actually overlap, so it is decided here
    /// rather than left to the author to delete the ` = ` again.
    fn struct_initializer_completions(
        &self,
        tokens: &[(Token<'_>, Span)],
        struct_id: Id,
        open: usize,
        offset: usize,
        start: usize,
    ) -> Vec<Completion> {
        let program = self.program;
        let Some(structure) = program.structs.get(&struct_id) else {
            return Vec::new();
        };
        let written = struct_initializer_written(tokens, open, start);
        // The shorthand test is a scope question, so it is asked in ANALYZED
        // space like every other one (E52).
        let analyzed_offset = self.to_analyzed_offset(offset);
        let source = program.source_of(struct_id);
        structure
            .fields
            .iter()
            .enumerate()
            .filter(|(_, field)| !written.contains(field.name))
            .map(|(index, field)| {
                let mut completion =
                    Completion::bare(field.name.to_string(), CompletionKind::Field);
                completion.detail = self.field_type_label(struct_id, index, field.name);
                completion.documentation = source.and_then(|source| {
                    self.doc_first_paragraph_at(source, field.name_span.into_range().start)
                });
                if self.binding_in_scope(field.name, analyzed_offset).is_none() {
                    completion.insert = Some(InsertText {
                        text: format!("{} = ", field.name),
                        is_snippet: false,
                    });
                }
                completion
            })
            .collect()
    }

    /// The rendered type of the struct field at `index`, read out of the
    /// struct's pre-rendered declaration label (E160) — the block hover fences,
    /// whose field lines are `\t{name}: {type},` in declaration order
    /// (`analyzer.rs::struct_declaration_label`).
    ///
    /// Read from the label rather than rendered here because the analyzer's
    /// type printer is not on `Program` — a `TypeId` cannot be rendered outside
    /// the analyzer, which is the sentence [`Completion::detail`] recorded as
    /// "a field's type is not cheaply renderable from the analyzed `Program`".
    /// The label already IS that printer's answer for exactly these fields, so
    /// this is one source of truth and no per-request cost: the table is built
    /// with the analysis. The field's own name is checked off the line rather
    /// than assumed, so a label whose shape ever changes yields `None` — no
    /// detail — instead of a wrong type.
    pub fn field_type_label(&self, struct_id: Id, index: usize, name: &str) -> Option<String> {
        let label = self.program.declaration_labels.get(&struct_id)?;
        // Line 0 is `struct Name… {`; the fields follow in declaration order.
        let line = label.lines().nth(index + 1)?.trim();
        let rendered = line.strip_prefix(name)?.strip_prefix(": ")?;
        Some(rendered.trim_end_matches(',').to_string())
    }

    /// The `View` the element desugar builds on: the nominal `view("tag")`
    /// returns (element-syntax.md §4 — a head lowers to a `view(…)` chain),
    /// read from the declaration rather than matched by name, so the browser
    /// and process twins each answer for their own platform.
    fn element_view_nominal_id(&self) -> Option<Id> {
        let program = self.program;
        let mut fallback = None;
        for (id, function) in &program.functions {
            if function.name != "view" {
                continue;
            }
            let Some(nominal) = function
                .return_type_id
                .and_then(|type_id| nominal_type_id(program, type_id))
            else {
                continue;
            };
            // std's `view`, not a same-named entry-file function.
            if program.source_of(*id) != Some(SourceId(0)) {
                return Some(nominal);
            }
            fallback = fallback.or(Some(nominal));
        }
        fallback
    }

    /// Every registered macro name, for attribute-position completion. The
    /// union over all scopes deliberately over-offers (visibility is
    /// file-scoped; the expansion engine still gates actual use) — the
    /// recorded refinement is filtering to this file's macro scope.
    fn macro_name_completions(&self) -> Vec<Completion> {
        let program = self.program;
        let mut names: Vec<&str> = program
            .scopes
            .values()
            .flat_map(|scope| scope.macro_name_to_id.keys().copied())
            .collect();
        names.sort_unstable();
        names.dedup();
        names
            .into_iter()
            .map(|name| Completion::bare(name.to_string(), CompletionKind::Macro))
            .collect()
    }

    /// Fields and methods of the receiver value ending at `receiver_end` —
    /// one past its last byte, LIVE space, as [`CursorContext::Member`] found
    /// it. Not "one before the `.`": the two coincide only where no trivia
    /// separates the receiver from the dot (kolt.local 001).
    fn member_completions(
        &self,
        tokens: &[(Token<'_>, Span)],
        receiver_end: usize,
    ) -> Vec<Completion> {
        let Some(type_id) = self.receiver_nominal_id(tokens, receiver_end) else {
            return Vec::new();
        };
        self.nominal_member_completions(type_id)
    }

    /// The fields + methods of one nominal type — the member-completion list.
    ///
    /// A field carries the same two lines a struct-initializer field does
    /// (E204): its declared type as the popup's `detail`, read out of the
    /// pre-rendered declaration label by [`Self::field_type_label`], and its own
    /// `///` first paragraph as the documentation. The two field positions a
    /// program has now answer alike — there was no reason for the one after a
    /// `.` to say less than the one inside a `{ … }`, and `Completion::detail`'s
    /// old "a field's type is not cheaply renderable" was true before E160 gave
    /// the label a reader.
    fn nominal_member_completions(&self, type_id: Id) -> Vec<Completion> {
        let program = self.program;
        let mut items = Vec::new();
        if let Some(structure) = program.structs.get(&type_id) {
            let source = program.source_of(type_id);
            for (index, field) in structure.fields.iter().enumerate() {
                let mut completion =
                    Completion::bare(field.name.to_string(), CompletionKind::Field);
                completion.detail = self.field_type_label(type_id, index, field.name);
                completion.documentation = source.and_then(|source| {
                    self.doc_first_paragraph_at(source, field.name_span.into_range().start)
                });
                // E213: `Region.anchor` is the exhibit — public on purpose,
                // dangerous on purpose, and the one case declaration
                // visibility cannot serve at all.
                completion.internal = field.internal.map(str::to_string);
                items.push(completion);
            }
        }
        self.push_methods(type_id, true, &mut items);
        items
    }

    /// Members of the ELEMENT under a lifted chain (`a?.` on an
    /// `Option<Profile>` offers Profile's members): the receiver ends at
    /// `receiver_end` (LIVE space, as [`CursorContext::Member`] found it) and
    /// its container's first type argument is the element.
    fn lifted_member_completions(
        &self,
        tokens: &[(Token<'_>, Span)],
        receiver_end: usize,
    ) -> Vec<Completion> {
        let program = self.program;
        // The receiver read from the LIVE tokens (E131), which is where its
        // identity lives while the buffer is ahead of the analysis.
        if let Some(element) = self
            .live_receiver_index(tokens, receiver_end)
            .and_then(|index| self.live_receiver_type_id(tokens, index, 0))
            .and_then(|type_id| match program.type_id_to_type_map.get(&type_id) {
                Some(Type::Struct(_, arguments)) | Some(Type::Enum(_, arguments)) => {
                    arguments.first().copied()
                }
                _ => None,
            })
            .and_then(|element| nominal_type_id(program, element))
        {
            return self.nominal_member_completions(element);
        }
        // A bare name (`p?.`): the binding's declared container type. The NAME
        // comes off the live text, but resolving it is a `program` lookup, so
        // it converts to ANALYZED space first (E52).
        if let Some(name) = identifier_ending_at(self.live.text(), receiver_end) {
            let analyzed_offset = self.to_analyzed_offset(receiver_end);
            let binding = self
                .binding_in_scope(name, analyzed_offset)
                .or_else(|| self.same_file_variable(name, analyzed_offset));
            let element = binding
                .and_then(|id| {
                    program
                        .variables
                        .get(&id)
                        .map(|variable| variable.type_id)
                        .or_else(|| {
                            program
                                .parameters
                                .get(&id)
                                .map(|parameter| parameter.type_id)
                        })
                })
                .and_then(|type_id| match program.type_id_to_type_map.get(&type_id) {
                    Some(Type::Enum(_, arguments)) | Some(Type::Struct(_, arguments)) => {
                        arguments.first().copied()
                    }
                    _ => None,
                })
                .and_then(
                    |element_id| match program.type_id_to_type_map.get(&element_id) {
                        Some(Type::Struct(id, _)) | Some(Type::Enum(id, _)) => Some(*id),
                        _ => None,
                    },
                );
            if let Some(element) = element {
                return self.nominal_member_completions(element);
            }
        }
        // A complex receiver (`find(x)?.`): the first type argument of its own
        // value type names the element — another `program` lookup, so
        // `entity_at` also takes the ANALYZED offset (E52). A CALL receiver is
        // resolved structurally (E66); the rendered label is the fallback for
        // whatever that cannot type.
        //
        // Gated on the analyzed text still describing the receiver's own bytes
        // (E131): inside the edit window the converted offset lands on a
        // clamped position and this answers about an expression that is no
        // longer written there. A wrong list is worse than no list, and the
        // next settled request is right.
        if !self.analyzed_agrees_at(receiver_end) {
            return Vec::new();
        }
        // Asked AT `receiver_end`, not one byte inside it: `entity_at` is
        // end-inclusive (E139), so the entity that ENDS where the receiver
        // ends is exactly the receiver, and the old `receiver_end - 1` probe —
        // strict containment one byte in — now answers whatever narrower
        // entity happens to close there (`Some(1).` resolved to the literal
        // `1`). The question this arm is asking has always been "what ends
        // here"; only the strict test made "one byte inside" the way to spell
        // it.
        Some(self.to_analyzed_offset(receiver_end))
            .and_then(|offset| self.entity_at(offset))
            .and_then(|receiver| {
                self.expression_element_nominal_id(receiver).or_else(|| {
                    self.hover_label(receiver)
                        .and_then(|label| first_generic_argument(&label).map(str::to_string))
                        .and_then(|element| {
                            self.nominal_id_by_name(
                                base_type_name(&element),
                                self.to_analyzed_offset(receiver_end),
                            )
                        })
                })
            })
            .map(|type_id| self.nominal_member_completions(type_id))
            .unwrap_or_default()
    }

    /// The nominal struct/enum id of the receiver value ending at
    /// `receiver_end` — one past its last byte, LIVE space (see
    /// [`CursorContext::Member`]).
    fn receiver_nominal_id(&self, tokens: &[(Token<'_>, Span)], receiver_end: usize) -> Option<Id> {
        // The receiver typed from the LIVE tokens (E131) — a bare name through
        // scope, a call through its callee's declaration, a method call
        // through the receiver's own nominal, a field through its declared
        // type. Every one of those is a question about bytes the user is
        // looking at answered against declarations the landing still owns, and
        // none of them asks the analyzed tree what sits at an offset.
        if let Some(nominal) = self
            .live_receiver_index(tokens, receiver_end)
            .and_then(|index| self.live_receiver_type_id(tokens, index, 0))
            .and_then(|type_id| nominal_type_id(self.program, type_id))
        {
            return Some(nominal);
        }
        // What the live walk cannot type — a literal, an `if`, a `match`, an
        // index — falls back to the parsed entity's own value type, another
        // `program` lookup, so `entity_at` takes the ANALYZED offset (E52).
        // The rendered label is the FALLBACK, not the answer: it is hover's
        // phrasing, and hover answers a constructor call with the thing being
        // constructed (`Some(1)` -> `enum Option`), which is right here, but a
        // plain call with the CALLEE's signature (`make()` -> `fn make():
        // Point`), which never names a type at all.
        //
        // And it is GATED on the analyzed text still describing the receiver's
        // own bytes: `to_analyzed_offset` clamps, the cursor's line is by
        // construction inside the edit window, and inside it this answers about
        // whatever expression used to be written there. Silence is the honest
        // answer, and the next settled request is right (Q4).
        if !self.analyzed_agrees_at(receiver_end) {
            return None;
        }
        // At `receiver_end`, for the reason `member_completions_for` states:
        // `entity_at` is end-inclusive (E139), so the receiver is the entity
        // that ends where the receiver ends.
        Some(self.to_analyzed_offset(receiver_end))
            .and_then(|offset| self.entity_at(offset))
            .and_then(|receiver| {
                self.expression_nominal_id(receiver).or_else(|| {
                    self.hover_label(receiver).and_then(|label| {
                        self.nominal_id_by_name(
                            base_type_name(&label),
                            self.to_analyzed_offset(receiver_end),
                        )
                    })
                })
            })
    }

    /// The index of the token the receiver ENDS on — `receiver_end` is one past
    /// its last byte, as [`member_context`] read it off the same token stream.
    fn live_receiver_index(
        &self,
        tokens: &[(Token<'_>, Span)],
        receiver_end: usize,
    ) -> Option<usize> {
        tokens
            .iter()
            .rposition(|(_, span)| span.into_range().end == receiver_end)
    }

    /// The type of the receiver expression whose LAST TOKEN is `tokens[index]`,
    /// read from the live buffer's own tokens (E131).
    ///
    /// This is the half of member resolution that must not be asked in analyzed
    /// coordinates. The receiver's IDENTITY — which expression it is — is a
    /// fact about the bytes the user is looking at, and the cursor's own line
    /// is, by construction, a line the analysis has not seen: the edit window
    /// is line-aligned and the line being typed on is the line that differs.
    /// Resolving it through `to_analyzed_offset` and `entity_at` asked the
    /// LANDED tree what sits at a clamped offset on that line, which is
    /// whatever used to be written there — `<div .styled(const style::style().`
    /// answered `View`'s method set, because the clamp landed back on the
    /// element.
    ///
    /// So the SHAPE is walked in token space (the same space
    /// [`member_context`] finds the receiver in, so trivia and comments are
    /// already the lexer's problem and a `(` inside a string literal cannot be
    /// mistaken for a bracket), and only NAMES are resolved against the landed
    /// `Program` — which is what a landed analysis is still authoritative
    /// about, since a declaration is not on the line being typed.
    ///
    /// Four shapes, the ones a receiver is actually written as:
    /// `p` (a binding), `f(…)` / `a::b::f(…)` (a call), `x.m(…)` (a method
    /// call) and `x.f` (a field). Anything else — a literal, an `if`, a
    /// `match`, an index — answers `None` and falls back to the caller's
    /// analyzed-coordinate arm, which the gate then decides on.
    fn live_receiver_type_id(
        &self,
        tokens: &[(Token<'_>, Span)],
        index: usize,
        depth: usize,
    ) -> Option<TypeId> {
        if depth > EXPRESSION_TYPE_DEPTH_LIMIT {
            return None;
        }
        let program = self.program;
        // A call: `…(…)`. The matching `(` is found by depth in token space,
        // and the token before it is the callee's name.
        if matches!(tokens[index].0, Token::Ctrl(')')) {
            let open = matching_open_bracket(tokens, index)?;
            let callee = open.checked_sub(1)?;
            let Token::Ident(name) = tokens[callee].0 else {
                return None;
            };
            let target = match callee.checked_sub(1).map(|before| &tokens[before].0) {
                // `x.m(…)` — the member `m` of whatever `x` resolves to.
                Some(Token::Ctrl('.')) => {
                    let receiver = self.live_receiver_type_id(tokens, callee - 2, depth + 1)?;
                    self.member_id(nominal_type_id(program, receiver)?, name)?
                }
                // `a::b::f(…)` — a module member or a type's static.
                Some(Token::Op("::")) => {
                    let namespace = self.live_namespace_id(tokens, callee - 2, depth)?;
                    self.namespace_entry_id(namespace, name)?
                }
                // `f(…)` — a name in scope.
                _ => self.binding_in_scope(
                    name,
                    self.to_analyzed_offset(tokens[callee].1.into_range().start),
                )?,
            };
            return self.declared_result_type_id(target);
        }
        let Token::Ident(name) = tokens[index].0 else {
            return None;
        };
        match index.checked_sub(1).map(|before| &tokens[before].0) {
            // `x.f` — a field read.
            Some(Token::Ctrl('.')) => {
                let receiver = self.live_receiver_type_id(tokens, index - 2, depth + 1)?;
                let structure = program.structs.get(&nominal_type_id(program, receiver)?)?;
                structure
                    .fields
                    .iter()
                    .find(|field| field.name == name)
                    .map(|field| field.type_id)
            }
            // `a::B` names a namespace, not a value.
            Some(Token::Op("::")) => None,
            // A bare name: the binding it denotes, in the scope the cursor is
            // in. The name is live, the scope is a `program` lookup, and that
            // split is E52's own rule — this is the arm that was already right,
            // and the item's own isolation (a bare-name receiver answers
            // correctly even while the request is stale).
            _ => {
                let range = tokens[index].1.into_range();
                let analyzed_offset = self.to_analyzed_offset(range.end);
                let binding = self
                    .binding_in_scope(name, analyzed_offset)
                    .or_else(|| self.same_file_variable(name, analyzed_offset))?;
                binding_type_id(program, binding)
            }
        }
    }

    /// The namespace a `::` path whose LAST segment is `tokens[index]` denotes
    /// — the token-space twin of [`Self::code_path_completions`]' walk, for the
    /// head of a live `a::b::f(…)` receiver.
    fn live_namespace_id(
        &self,
        tokens: &[(Token<'_>, Span)],
        index: usize,
        depth: usize,
    ) -> Option<Id> {
        if depth > EXPRESSION_TYPE_DEPTH_LIMIT {
            return None;
        }
        let Token::Ident(name) = tokens[index].0 else {
            return None;
        };
        // A further `::` to the left means this segment is a descent step, not
        // the head; the head is the one resolved through scope (E53).
        if index >= 2 && matches!(tokens[index - 1].0, Token::Op("::")) {
            let outer = self.live_namespace_id(tokens, index - 2, depth + 1)?;
            return self.namespace_member(outer, name);
        }
        self.namespace_in_scope(
            name,
            self.to_analyzed_offset(tokens[index].1.into_range().start),
        )
    }

    /// The id `namespace::name` denotes for a VALUE position — a module member
    /// or a type's STATIC (`Panel::new()`) — where [`Self::namespace_member`]
    /// answers only for the namespaces a path descends through.
    fn namespace_entry_id(&self, namespace: Id, name: &str) -> Option<Id> {
        let program = self.program;
        if let Some(module) = program.modules.get(&namespace)
            && let Some(scope) = program.scopes.get(&module.body.1)
        {
            return scope.name_to_id_map.get(name).copied();
        }
        self.index
            .members
            .methods(namespace, false)
            .iter()
            .find(|(member, _)| member == name)
            .map(|(_, id)| *id)
    }

    /// The instance member `name` the nominal type `type_id` provides — read
    /// from the captured table, so typing a live method-call receiver costs a
    /// lookup rather than a walk over every impl in the program (M29).
    fn member_id(&self, type_id: Id, name: &str) -> Option<Id> {
        self.index
            .members
            .methods(type_id, true)
            .iter()
            .find(|(member, _)| member == name)
            .map(|(_, id)| *id)
    }

    /// The result type of calling `target`, from its DECLARATION alone: the
    /// declared return, E107's inferred one, or an external's.
    ///
    /// A declared return that is a bare type PARAMETER answers `None` rather
    /// than `T` itself: E130 substitutes such a return through what the
    /// analyzer recorded for a call it can NAME, and a receiver typed since the
    /// landing has no analyzed call id at all. `None` here falls back to the
    /// analyzed arm, which the gate then decides on — silence, not a wrong
    /// list.
    fn declared_result_type_id(&self, target: Id) -> Option<TypeId> {
        let program = self.program;
        // An impl's `declarations` entry is a MEMBER id, and a name in scope is
        // a binding: both reach their declaration through the same chain walk
        // hover and go-to-definition use.
        let target = self.function_target(target).unwrap_or(target);
        let type_id = if let Some(function) = program.functions.get(&target) {
            function
                .return_type_id
                .or_else(|| program.inferred_return_types.get(&target).copied())?
        } else {
            program.external_functions.get(&target)?.return_type_id
        };
        (!matches!(
            program.type_id_to_type_map.get(&type_id),
            Some(Type::Generic(_))
        ))
        .then_some(type_id)
    }

    /// The nominal struct/enum id of the VALUE an expression produces — the
    /// question member completion asks of a receiver, and a different one from
    /// [`Self::hover_label`], which describes the expression *as written*.
    ///
    /// The analyzer records a type on an expression's own id only where one is
    /// *produced*; a call, and a block whose value is one, are typed on demand
    /// and store nothing (the same silence B85 hit on `for … in` iterables and
    /// B70 on tuple elements). So `expr_types`/`expr_type_ids` answer a field, an
    /// index, a literal and a struct initializer directly, and the shapes below
    /// are resolved by structure instead (E66).
    fn expression_nominal_id(&self, id: Id) -> Option<Id> {
        let program = self.program;
        nominal_type_id(program, self.expression_type_id(id, 0)?)
    }

    /// [`Self::expression_nominal_id`]'s LIFTED twin: the nominal of the
    /// container's ELEMENT — `find(x)?.` on an `Option<Profile>` offers
    /// Profile's members (proposal/try-and-lift.md §5).
    fn expression_element_nominal_id(&self, id: Id) -> Option<Id> {
        let program = self.program;
        let type_id = self.expression_type_id(id, 0)?;
        let element = match program.type_id_to_type_map.get(&type_id)? {
            Type::Struct(_, arguments) | Type::Enum(_, arguments) => *arguments.first()?,
            _ => return None,
        };
        nominal_type_id(program, element)
    }

    /// The resolved type of the value `id` produces. `depth` bounds the walk
    /// through the nesting shapes (a block's trailing expression is itself an
    /// expression), so a malformed mid-edit tree cannot spin here.
    fn expression_type_id(&self, id: Id, depth: usize) -> Option<TypeId> {
        let program = self.program;
        if depth > EXPRESSION_TYPE_DEPTH_LIMIT {
            return None;
        }
        if let Some(type_id) = program.expr_type_ids.get(&id) {
            return Some(*type_id);
        }
        match program.entity_map.get(&id)? {
            Expr::Local(binding) | Expr::Variable(binding) | Expr::Parameter(binding) => {
                binding_type_id(program, *binding)
            }
            Expr::Call(call_id) => self.call_result_type_id(*call_id, depth),
            Expr::Block((_, tail)) => self.expression_type_id(*tail, depth + 1),
            _ => None,
        }
    }

    /// A call's result type, read off the callee's declaration: the return type
    /// of the function it names, or the result of the closure type it holds.
    ///
    /// The type ARGUMENTS of a generic return (`Result<Note, RpcError>`) do not
    /// have to be solved for this to be useful — member completion resolves
    /// members on the nominal head, and that head is written in the
    /// declaration. Except where it is NOT: a return declared as a bare type
    /// PARAMETER (`fun get(self): T`) has no head at all, and the declaration
    /// alone can never name one. That case goes through
    /// [`Self::substituted_return_type_id`] (E130).
    fn call_result_type_id(&self, call_id: Id, depth: usize) -> Option<TypeId> {
        let program = self.program;
        let subject_id = program.function_calls.get(&call_id)?.subject_id;
        // The callee is reached as a bare reference to its declaration.
        let callee_id = match program.entity_map.get(&subject_id) {
            Some(Expr::Local(binding))
            | Some(Expr::Variable(binding))
            | Some(Expr::Parameter(binding)) => *binding,
            _ => subject_id,
        };
        if let Some(function) = program.functions.get(&callee_id) {
            if let Some(return_type_id) = function.return_type_id {
                return Some(self.substituted_return_type_id(call_id, return_type_id));
            }
            // An UNANNOTATED return is not an unknown one (E107): vilan infers it,
            // and the analyzer memoizes the answer per function. Reading only the
            // declaration made member completion go silent for every call in a
            // builder chain written the way the language invites — `fun on_drag(own
            // self, …) { …; self }` — which is one `.` away from the whole chain
            // offering nothing. The record is keyed by function alone and written
            // only for an exact answer under an empty substitution, so it is the
            // function's own return type, never a caller's specialization.
            if let Some(return_type_id) = program.inferred_return_types.get(&callee_id) {
                return Some(*return_type_id);
            }
        }
        if let Some(external) = program.external_functions.get(&callee_id) {
            return Some(external.return_type_id);
        }
        // A closure-typed callee (`let render = || …; render().`).
        let subject_type_id = self.expression_type_id(subject_id, depth + 1)?;
        match program.type_id_to_type_map.get(&subject_type_id)? {
            Type::Closure(_, return_type_id, _) => Some(*return_type_id),
            _ => None,
        }
    }

    /// A declared return type resolved AT THIS CALL SITE: `return_type_id`
    /// unchanged for every return that names something, and the type the
    /// analyzer bound the parameter to for a return that is a bare type
    /// parameter (E130).
    ///
    /// `fun get(self): T` on a `SignalCell<List<str>>` returns `List<str>`,
    /// and only the call site knows that. Reading the declaration verbatim
    /// gave completion `T`'s own TypeId — a `Type::Generic`, which
    /// [`nominal_type_id`] matches nothing for and `hover_label` renders as
    /// the literal `T` — so the whole popup went silent on a shape std's
    /// reactive cell puts one `.` away from every read of a signal. E107 fixed
    /// the sibling where the callee declares NO return; this is the one where
    /// it declares a return that names no type.
    ///
    /// The bindings are the analyzer's own, recorded per call and already
    /// carried out on `Program`: `method_call_substitution` is the single
    /// channel every instantiation shape writes into — an impl's subject
    /// generics, a method's own generics, and a free call's bindings whether
    /// the source spelled them (`echo<Point>(…)`) or the solver inferred them
    /// — which its own declaration site says in as many words, and which the
    /// free-generic pin holds. The positional records beside it
    /// (`own_generic_call_bindings`, `FunctionCall::generic_argument_ids`)
    /// exist for the emission's re-dispatch and add no reach here; nothing is
    /// read from them. A miss leaves the declared id alone, which is exactly
    /// what this answered before.
    ///
    /// The map is populated MID-EDIT, which is the property this rests on: the
    /// pins' buffers carry `expected a field or method name after '.'` and the
    /// call before the dot is still recorded with its substitution, because a
    /// parse error in one expression does not un-analyze the receiver that
    /// parsed.
    fn substituted_return_type_id(&self, call_id: Id, return_type_id: TypeId) -> TypeId {
        let program = self.program;
        // A `Type::Generic` wraps the CONSTRAINT id the declaration's type
        // parameter was minted as, and that inner id is what every binding
        // record below is keyed by — the return type's own id is the
        // occurrence, not the parameter.
        let Some(Type::Generic(parameter)) = program.type_id_to_type_map.get(&return_type_id)
        else {
            return return_type_id;
        };
        // The substitution map keyed by the constraint id the declaration
        // wrote — the one channel that covers an impl's subject generics, which
        // is where `SignalCell<T>::get`'s `T` comes from.
        if let Some(bound) = program
            .method_call_substitution
            .get(&call_id)
            .and_then(|substitution| substitution.get(parameter))
        {
            return *bound;
        }
        return_type_id
    }

    /// The nearest same-file `let`/`mut` binding named `name` declared before
    /// `analyzed_offset` (ANALYZED space — `variable.name_span` is a program
    /// span) — a fallback for when the cursor's statement failed to parse and
    /// so dropped its enclosing scope from the analysis.
    fn same_file_variable(&self, name: &str, analyzed_offset: usize) -> Option<Id> {
        let program = self.program;
        let mut best: Option<(usize, Id)> = None;
        for (id, variable) in &program.variables {
            let start = variable.name_span.into_range().start;
            if variable.name == name
                && start < analyzed_offset
                && program.source_of(*id) == Some(SourceId(0))
                && best.is_none_or(|(best_start, _)| start > best_start)
            {
                best = Some((start, *id));
            }
        }
        best.map(|(_, id)| id)
    }

    /// Candidates for an `import`/`use` path (E57), routed by how many segments
    /// precede the cursor:
    ///
    /// - **none** (`import |`, `import s|`) — the ORIGINS: `std`, `pkg`, and
    ///   each dependency package's import name. Not the names in scope, not the
    ///   keywords, not the construct snippets — none of them may follow
    ///   `import`, and offering them is what the head position did before.
    /// - **one** (`import std::|`) — that origin's modules, enumerated from its
    ///   source roots, plus the package's own `lib.vl` surface where it has one
    ///   (`import std::io::print`).
    /// - **two or more** (`import std::json::|`) — the named module's importable
    ///   names, LOADED ON DEMAND. The point of an import is to reach a module
    ///   the program has not loaded, so the analyzed `Program` cannot answer;
    ///   the load is the loader's own, through its content-keyed parse cache.
    ///   A further segment descends into an enum's variants
    ///   (`import std::option::Option::Some`), the only namespace past a module
    ///   that `resolve_import` descends into.
    ///
    /// A head naming none of the origins falls back to the whole-`Program`
    /// lookup by name — that is what serves a same-file `mod` block
    /// (`import geometry::area`), and global reach is correct HERE, which is the
    /// half of E53's split that stays.
    ///
    /// Anything that does not resolve answers EMPTY. A completion request is
    /// answered on the editor's critical path: it never errors, and a module
    /// that is not there is simply not offered.
    fn import_completions(&self, text: &str, offset: usize) -> Vec<Completion> {
        let program = self.program;
        // B318 S3: inside an `(impl …)` selector the answer is not a NAME the
        // module offers — it is a block the module writes, or a member of one.
        // Asked first, because a selector's own text is not a path and
        // `import_path_segments` would decline it.
        if let Some(position) = impl_selector_position(text, offset) {
            return self.impl_selector_completions(position);
        }
        let Some(segments) = import_path_segments(text, offset) else {
            return Vec::new();
        };
        let Some(roots) = self.import_roots else {
            // The degraded internal-error document resolved no package tree, so
            // there is nothing to enumerate.
            return Vec::new();
        };
        let Some((origin, rest)) = segments.split_first() else {
            return origin_completions(roots);
        };
        let Some((module_roots, _surface)) = roots.origin_roots(origin, program.platform) else {
            // Not an origin — a same-file `mod`, or a namespace already in the
            // program under some other name. The last segment is the namespace
            // being descended into, which is all the by-name lookup reads.
            return self.namespace_completions_by_name(rest.last().unwrap_or(origin));
        };
        match rest.split_first() {
            // `origin::` — the origin's whole module list. Served from the
            // index's captured listing (M25): this arm used to call
            // `modules_in_root`, a `read_dir` per source root, per request,
            // and §2.1 is explicit that the keystroke path may not read the
            // filesystem. An origin the analysis did not record enumerates
            // nothing, which is the same empty answer an unresolvable origin
            // has always given.
            None => self
                .index
                .origin(origin)
                .map(|listing| listing.completions())
                .unwrap_or_default(),
            Some((module, past_module)) => {
                module_member_completions(&module_roots, module, past_module)
            }
        }
    }

    /// B318 S3's completion surface (`visibility.md` §7.1): after `impl ` the
    /// module's impl SUBJECTS, after `)::` the selected block's members.
    ///
    /// Both come out of the parse cache through
    /// [`vilan_core::analyzer::module_impl_blocks`], with no analyzer, which is
    /// the property the whole import-path family keeps: the module being
    /// selected from is one this program may never have loaded.
    fn impl_selector_completions(&self, position: SelectorPosition<'_>) -> Vec<Completion> {
        let (module_path, subject) = match &position {
            SelectorPosition::Subject { module } => (module, None),
            SelectorPosition::Member { module, subject } => (module, Some(*subject)),
        };
        let Some(roots) = self.import_roots else {
            return Vec::new();
        };
        let Some((origin, rest)) = module_path.split_first() else {
            return Vec::new();
        };
        let Some((module_roots, _surface)) = roots.origin_roots(origin, self.program.platform)
        else {
            return Vec::new();
        };
        if rest.is_empty() {
            return Vec::new();
        }
        let module_roots: Vec<&Path> = module_roots.to_vec();
        let Some(file) = vilan_core::analyzer::module_source_file(&module_roots, &rest.join("::"))
        else {
            return Vec::new();
        };
        let blocks = vilan_core::analyzer::module_impl_blocks(&file);
        // E178: a selector names a SUBJECT, and a subject the module keeps
        // private is not a block an importer may admit. `module_impl_blocks`
        // answers with the block's written head and nothing about visibility —
        // an `impl` block carries no marker of its own at this sha (B318 S4 is
        // what gives one meaning), so the bit that exists to be read is the
        // SUBJECT's, and a head that names no importable of this module (a
        // primitive, or a type declared elsewhere and extended here) is left
        // offered, because the module is not the place that decides it.
        let importables = vilan_core::analyzer::module_importables(&file);
        let offered = offered_importables(&importables);
        let declares_privately = |head: &str| {
            importables.iter().any(|row| row.name == head)
                && !offered.iter().any(|row| row.name == head)
        };
        match subject {
            // After `impl `: one row per SUBJECT the module writes a block for,
            // deduplicated — two blocks for one head are one thing to select.
            None => {
                let mut seen: HashSet<String> = HashSet::new();
                blocks
                    .into_iter()
                    .filter(|(head, _)| !declares_privately(head))
                    .filter(|(head, _)| seen.insert(head.clone()))
                    .map(|(head, _)| Completion::bare(head, CompletionKind::Struct))
                    .collect()
            }
            // After `)::`: the members of every block whose head the selector
            // names. The subject as typed may carry arguments
            // (`(impl List<i32>)::`), and the block is found by HEAD — which is
            // also what the selector filters by before its type test runs.
            Some(subject) => {
                let head = selector_subject_head(subject);
                if declares_privately(head) {
                    return Vec::new();
                }
                let mut seen: HashSet<String> = HashSet::new();
                blocks
                    .into_iter()
                    .filter(|(block_head, _)| block_head == head)
                    .flat_map(|(_, members)| members)
                    .filter(|member| seen.insert(member.clone()))
                    .map(|member| Completion::bare(member, CompletionKind::Function))
                    .collect()
            }
        }
    }

    /// Items reachable through a `::` path in CODE — an enum's variants and
    /// statics, a struct's statics, or a module's members — where the path is
    /// the whole `a::b::c` chain ending just before the `::` at `colon_offset`.
    ///
    /// The HEAD is resolved THROUGH SCOPE (E53). Matching it against every
    /// loaded module's declarations by name, which is what this did, offered
    /// whatever any module in the process happened to declare — and nine std
    /// modules are ALWAYS loaded for the derive prelude, so `Json::` completed
    /// `parse`, `stringify`, and friends in a file that never imported
    /// `std::json`. An import path is the opposite case, where reaching what is
    /// not in scope is the entire point; it is served by
    /// [`Self::import_completions`], which keeps the by-name lookup
    /// ([`Self::namespace_completions_by_name`]).
    ///
    /// Everything PAST the head is a descent, not a second scope lookup (E129).
    /// This used to read one identifier, so `style::FlexDirection::` looked up
    /// `FlexDirection` in scope — where it is not, and never was: it is a
    /// MEMBER of `style` — and answered nothing, while the import spelling of
    /// the same path (`import std::style::FlexDirection::`) descended fine.
    /// Each further segment is one step through
    /// [`Self::namespace_member`], the same single-step descent
    /// [`Self::namespace_completions`] offers the last one's items from; a
    /// segment that names nothing a path descends into (an enum variant, a
    /// function, a typo) stops the walk and answers empty, which is what an
    /// unresolvable path has always answered.
    fn code_path_completions(&self, text: &str, colon_offset: usize) -> Vec<Completion> {
        let Some(segments) = code_path_segments(text, colon_offset) else {
            return Vec::new();
        };
        let Some((head, rest)) = segments.split_first() else {
            return Vec::new();
        };
        let analyzed_offset = self.to_analyzed_offset(colon_offset);
        let Some(mut namespace) = self.namespace_in_scope(head, analyzed_offset) else {
            return Vec::new();
        };
        for segment in rest {
            let Some(next) = self.namespace_member(namespace, segment) else {
                return Vec::new();
            };
            namespace = next;
        }
        self.namespace_completions(namespace)
    }

    /// One step of a `::` descent: the namespace `namespace::name` denotes.
    ///
    /// A module's own scope is the only thing a code path descends THROUGH — a
    /// module holds enums, structs and nested modules, and each of those is a
    /// namespace in turn. An enum is where a path STOPS: its variants and
    /// statics are values, not namespaces, and `import_completions` stops in
    /// the same place for the same reason (`resolve_import` descends no
    /// further either). `None` for anything else, which answers empty rather
    /// than guessing.
    fn namespace_member(&self, namespace: Id, name: &str) -> Option<Id> {
        let program = self.program;
        let module = program.modules.get(&namespace)?;
        let scope = program.scopes.get(&module.body.1)?;
        let member = *scope.name_to_id_map.get(name)?;
        self.is_namespace(member).then_some(member)
    }

    /// The struct / enum / module `name` denotes at `analyzed_offset` — the
    /// cursor's scope out to global: locals, parameters, this file's top-level
    /// items, and everything a `use`/`import` bound.
    ///
    /// No same-file fallback of the kind [`Self::same_file_variable`] gives
    /// member completion, and none is needed: a type is declared at a file's TOP
    /// LEVEL, so it lives in the global scope that every scope chains to, and it
    /// stays reachable however badly the cursor's own statement is mid-edit. The
    /// fallback exists for member completion because a `let` binding lives in
    /// the very scope a broken statement drops.
    fn namespace_in_scope(&self, name: &str, analyzed_offset: usize) -> Option<Id> {
        self.binding_in_scope(name, analyzed_offset)
            .filter(|id| self.is_namespace(*id))
    }

    /// Whether `id` names something a `::` path descends into.
    fn is_namespace(&self, id: Id) -> bool {
        let program = self.program;
        program.enums.contains_key(&id)
            || program.structs.contains_key(&id)
            || program.modules.contains_key(&id)
    }

    /// The items `namespace::` offers: an enum's variants plus its statics, a
    /// struct's statics, a module's members. Empty for anything else.
    fn namespace_completions(&self, namespace: Id) -> Vec<Completion> {
        let program = self.program;
        let mut items = Vec::new();
        if let Some(enumeration) = program.enums.get(&namespace) {
            for variant in &enumeration.variants {
                let mut completion =
                    Completion::bare(variant.name.to_string(), CompletionKind::EnumVariant);
                // E221: `Side::` offers a labelled variant only to its prefix.
                completion.internal = variant.internal.map(str::to_string);
                items.push(completion);
            }
            self.push_methods(namespace, false, &mut items);
        } else if program.structs.contains_key(&namespace) {
            self.push_methods(namespace, false, &mut items);
        } else if let Some(module) = program.modules.get(&namespace)
            && let Some(scope) = program.scopes.get(&module.body.1)
        {
            for (name, id) in &scope.name_to_id_map {
                let kind = self.kind_of(*id);
                items.push(self.entity_completion(name.to_string(), *id, kind));
            }
        }
        items
    }

    /// The items `name::` offers, looked up across the WHOLE program by name —
    /// every loaded enum, struct, and module, in scope or not.
    ///
    /// Correct in an import path and wrong everywhere else (E53). An import is
    /// how a name gets into scope, so requiring it to be in scope already would
    /// answer nothing; and this is what serves a same-file `mod` block
    /// (`import geometry::area`), whose namespace is not an origin's.
    fn namespace_completions_by_name(&self, name: &str) -> Vec<Completion> {
        let program = self.program;
        let mut items = Vec::new();
        for (id, enumeration) in &program.enums {
            if enumeration.name == name {
                items.extend(self.namespace_completions(*id));
            }
        }
        for (id, structure) in &program.structs {
            if structure.name == name {
                items.extend(self.namespace_completions(*id));
            }
        }
        for (id, module) in &program.modules {
            if module.name == name {
                items.extend(self.namespace_completions(*id));
            }
        }
        items
    }

    /// Importable-but-unimported candidates at a bare scope position (E54c):
    /// every function/struct/enum/trait/module-level value a directly-loaded
    /// `std` or `pkg` child module declares as its OWN top-level item (not a
    /// re-export or an import it merely forwards — a plain `import`/`use`
    /// inside a module lands in that module's scope too, and counting it
    /// would offer the same name a second time), whose name isn't already in
    /// `in_scope`.
    ///
    /// This is the SAME whole-program-by-name territory E53 walled off from
    /// silent, unscoped completion ([`Self::namespace_completions_by_name`],
    /// just above) — reused here on purpose and EXPLICITLY: every candidate
    /// is LABELED with its declaring module (`to_completion_item` shows it as
    /// `detail`) and carries the text edit that adds the import, so accepting
    /// one is never a surprise (E53's rule stands: nothing silent came back).
    ///
    /// Dependency packages are not scanned here — reaching them the way
    /// [`Self::import_candidates`] does is a disk-bound full-origin scan, fine
    /// for an on-demand quickfix but too slow to pay on every keystroke.
    ///
    /// **Position-aware filtering, declined (E59):** the caller (`completion`)
    /// reaches this function from one branch only — no preceding `.` or `::`
    /// — used identically for a bare value expression and a bare type
    /// annotation; neither it nor this function is told which. Telling them
    /// apart is not a read of data some earlier pass already computed (the
    /// analyzed `Program`'s own `type_references` only covers RESOLVED code,
    /// not the very position being typed); it is new syntactic analysis —
    /// scanning back past whatever sits before the cursor to find the
    /// enclosing form (`let x: |`, `fun f(): |`, `List<|>`, `x as |`, a
    /// struct field type, …), each a different shape, unlike
    /// [`in_import_path`]'s single-line anchor. Declined here; recorded as
    /// E59's residual.
    ///
    /// Ranked by [`import_origin_tier`] (E59), THEN alphabetically within a
    /// tier, and capped at [`AUTO_IMPORT_COMPLETION_CAP`] — applied in that
    /// order, before the truncation, which is the point: a plain alphabetical
    /// sort let the always-loaded `std` prelude's capitalized trait/type names
    /// (`Add`, `BitAnd`, …) fill the whole cap ahead of a small real file's
    /// own unimported names, which sort no higher than any other lowercase
    /// identifier. Tiering the user's own `pkg` ahead of `std` means the
    /// cap's last-to-survive candidates are always `std`'s, never `pkg`'s —
    /// a std-heavy file's loaded surface still cannot flood the popup, and
    /// now `std` only spends slots `pkg` didn't need.
    ///
    /// **The candidates themselves are not derived here** (M25). Which names
    /// exist, what kind each is, which module declares it and how they rank
    /// are all functions of the analyzed `Program` alone, so they are computed
    /// once per analysis into [`AutoImportOrder`] and this function reads the
    /// table. What is left is the part that genuinely depends on the cursor
    /// and the live buffer: skipping the names already in scope, stopping at
    /// the cap, and rendering each survivor's import edit.
    fn auto_import_completions(&self, in_scope: &HashSet<&str>) -> Vec<Completion> {
        let order = &self.index.auto_import;
        let chosen = order.take(in_scope, AUTO_IMPORT_COMPLETION_CAP);
        if chosen.is_empty() {
            return Vec::new();
        }
        // The edits themselves came off the analysis (M29): the parse they are
        // computed against is the analyzed text's, done once when the index was
        // built, not once per request. E83 had already reduced this arm to ONE
        // parse per request from one per candidate; this takes the last one out
        // of the request entirely.
        //
        // What the REQUEST still owes is the coordinates. An edit is a span in
        // the analyzed text, and the buffer may have moved since; the anchor
        // maps it, and a span with no image — the user is typing ON the import
        // run the edit would touch — drops the candidate rather than offering
        // an edit at a stale offset. That is the same rule E121's token
        // re-mapping follows, and it is why the analyzed parse is safe here:
        // where the answer could be wrong, there is no answer.
        chosen
            .into_iter()
            .filter_map(|candidate| {
                let module = &order.modules[candidate.module as usize];
                let edit = candidate.edit.as_ref()?;
                let span = self.map_analyzed_span(edit.span)?;
                Some(Completion {
                    label: candidate.name.clone(),
                    kind: candidate.kind,
                    detail: None,
                    documentation: None,
                    call_parameters: None,
                    snippet: None,
                    insert: None,
                    // An auto-import candidate is a name the file does not
                    // have yet; E213's label rides on the declaration and is
                    // read where the candidate is one the program knows.
                    internal: None,
                    filter_text: None,
                    replace_span: None,
                    needs_import: Some(AutoImport {
                        module_path: module.path.clone(),
                        edit_span: span,
                        edit_replacement: edit.replacement.clone(),
                        origin_tier: candidate.tier,
                    }),
                })
            })
            .collect()
    }

    /// Names visible at `analyzed_offset` (ANALYZED space — the cursor's scope,
    /// then each enclosing scope up to global) plus the language keywords.
    fn scope_completions(&self, analyzed_offset: usize) -> Vec<Completion> {
        let program = self.program;
        let mut items = Vec::new();
        let mut seen = HashSet::new();
        let mut scope_id = self.scope_at(analyzed_offset);
        while let Some(id) = scope_id {
            let Some(scope) = program.scopes.get(&id) else {
                break;
            };
            for (name, entity_id) in &scope.name_to_id_map {
                if seen.insert(*name) {
                    let kind = self.kind_of(*entity_id);
                    items.push(self.entity_completion(name.to_string(), *entity_id, kind));
                }
            }
            scope_id = scope.parent_id;
        }
        // The offered keywords are exactly the lexer's, drawn from the one
        // documented table [`KEYWORD_DOCS`] (kept in lockstep with the lexer by
        // [`keyword_lexeme`]) — no separate hand-list to drift (WO-3).
        for (keyword, _sentence, _link) in KEYWORD_DOCS {
            items.push(Completion::bare(
                keyword.to_string(),
                CompletionKind::Keyword,
            ));
        }
        // The shape-heavy constructs also complete as fill-in snippets, next to
        // the bare keyword (E14). Only scope positions reach here — member and
        // path completion never call this, and the import-path post-pass in
        // `completion` drops them — so the snippets stay out of `.`/`::`/import
        // contexts.
        for (keyword, label, detail, body) in CONSTRUCT_SNIPPETS {
            items.push(Completion::snippet(label, detail, body, keyword));
        }
        items
    }

    /// How far up a scope's parent chain [`Self::scope_extents`] folds one
    /// entity's span. Deep enough for any body a human writes, and a bound
    /// rather than a `while` so a malformed parent cycle cannot hang a
    /// keystroke.
    const SCOPE_NESTING_BOUND: usize = 256;

    /// The scope at `analyzed_offset` (ANALYZED space): the innermost scope the
    /// offset sits INSIDE, so the enclosing function's locals are offered
    /// wherever the cursor is in its body.
    ///
    /// E165: it used to be "the scope of the entity at — or nearest before —
    /// the offset", and on a BLANK LINE that is the wrong question. `entity_at`
    /// answers with the innermost entity whose span CONTAINS the offset, which
    /// on an empty line inside a body is the enclosing FUNCTION; and
    /// `entity_scope_map` holds the scope an entity is DECLARED IN, so a
    /// function answered the module scope. `fun main() { let start = 1; ▮ }`
    /// therefore offered only globals, while `st▮` one character away offered
    /// `start` — the accessor being typed is its own entity, and it lives in
    /// the body scope. A blank line is the moment a user actually asks for
    /// completion, so the fallback was firing exactly where it was least
    /// wanted.
    ///
    /// The extent test answers what the fallback was reaching for and is the
    /// question the language actually asks (see [`Self::scope_extents`]). The
    /// nearest-entity path stays as the fallback for an offset no scope
    /// contains — fresh text past the last top-level item, where there is no
    /// enclosing body at all.
    fn scope_at(&self, analyzed_offset: usize) -> Option<Id> {
        let program = self.program;
        if let Some((_, _, scope_id)) = self
            .scope_extents()
            .iter()
            .find(|(start, end, _)| *start <= analyzed_offset && analyzed_offset <= *end)
        {
            return Some(*scope_id);
        }
        let entity = self.entity_at(analyzed_offset).or_else(|| {
            self.entity_spans
                .iter()
                .filter(|(_, end, _)| *end <= analyzed_offset)
                .max_by_key(|(_, end, _)| *end)
                .map(|(_, _, id)| *id)
        })?;
        program.entity_scope_map.get(&entity).copied()
    }

    /// The byte extent of every scope the ENTRY FILE writes, narrowest first —
    /// so the first row containing an offset is the innermost scope around it
    /// (E165).
    ///
    /// A scope has no span of its own: `Scope` is a name map with a parent, and
    /// the braces that open it belong to the `Block`/`Func` node. Its extent is
    /// read off its MEMBERS instead — the union of the spans of the entry
    /// entities `entity_scope_map` files under it, folded up the parent chain
    /// because a scope's text contains its children's.
    ///
    /// The union reaches the closing brace because the parser's filler for a
    /// block with no trailing expression is an `Expr::Void` SPANNING that brace
    /// (S3, editing-dx.md §3.9). `entity_spans` excludes it — it is not
    /// something the user wrote and has no hover — and this walk deliberately
    /// keeps it: it is the one entity that says where the body ENDS, and
    /// without it a blank line after the last statement would fall outside the
    /// body it is written in.
    ///
    /// Entry entities only, by the entry's own id ranges (M27's rule): a scope
    /// shared with a loaded module would otherwise union spans from two
    /// different coordinate spaces.
    fn scope_extents(&self) -> &[(usize, usize, Id)] {
        self.scope_extents.get_or_init(|| {
            let program = self.program;
            let mut extents: HashMap<Id, (usize, usize)> = HashMap::default();
            let widen = |extents: &mut HashMap<Id, (usize, usize)>,
                         scope_id: Id,
                         start: usize,
                         end: usize| {
                let slot = extents.entry(scope_id).or_insert((start, end));
                slot.0 = slot.0.min(start);
                slot.1 = slot.1.max(end);
            };
            for id in program
                .id_ranges_of(SourceId(0))
                .into_iter()
                .flatten()
                .map(Id)
            {
                let (Some(scope_id), Some(span)) = (
                    program.entity_scope_map.get(&id).copied(),
                    program.span_map.get(&id),
                ) else {
                    continue;
                };
                let range = span.into_range();
                if range.start >= range.end {
                    continue;
                }
                widen(&mut extents, scope_id, range.start, range.end);
                // Up the parent chain: a block whose only statement is another
                // block declares nothing of its own, and its extent is its
                // child's. Bounded so a malformed parent cycle costs nothing.
                let mut parent = program
                    .scopes
                    .get(&scope_id)
                    .and_then(|scope| scope.parent_id);
                for _ in 0..Self::SCOPE_NESTING_BOUND {
                    let Some(id) = parent else { break };
                    widen(&mut extents, id, range.start, range.end);
                    parent = program.scopes.get(&id).and_then(|scope| scope.parent_id);
                }
            }
            let mut rows: Vec<(usize, usize, Id)> = extents
                .into_iter()
                .map(|(scope_id, (start, end))| (start, end, scope_id))
                .collect();
            // Narrowest first, and every tie broken, so the answer is a
            // function of the program and not of a hash map's iteration order.
            rows.sort_by_key(|(start, end, scope_id)| (end - start, *start, scope_id.0));
            rows
        })
    }

    /// The binding `name` resolves to in the scope at `analyzed_offset`
    /// (ANALYZED space, searching the enclosing scopes up to global) — a
    /// local, parameter, or top-level item.
    fn binding_in_scope(&self, name: &str, analyzed_offset: usize) -> Option<Id> {
        let program = self.program;
        let mut scope_id = self.scope_at(analyzed_offset);
        while let Some(id) = scope_id {
            let scope = program.scopes.get(&id)?;
            if let Some(binding) = scope.name_to_id_map.get(name) {
                return Some(*binding);
            }
            scope_id = scope.parent_id;
        }
        None
    }

    /// Appends `type_id`'s methods, restricted to either instance methods
    /// (`want_self`, for `value.`) or static/associated ones (for `Type::`). A
    /// `value.default()` (a static method with no `self`) would not type-check, so
    /// member completion must not offer it.
    ///
    /// The two sources and their precedence — the members the type's impl
    /// blocks DECLARE, then the default-bodied trait methods those impls
    /// INHERIT (kolt.local 033) — are resolved once per analysis into
    /// [`MemberTable`], whose doc carries them; this reads the answer and
    /// renders it. Deriving them here was a walk over every impl in the program
    /// on every request (M29).
    fn push_methods(&self, type_id: Id, want_self: bool, items: &mut Vec<Completion>) {
        for (name, member_id) in self.index.members.methods(type_id, want_self) {
            items.push(self.entity_completion(name.clone(), *member_id, CompletionKind::Method));
        }
    }

    /// The struct or enum a written type name refers to AT `analyzed_offset`
    /// (type arguments already stripped, a `::`-qualified spelling admitted).
    ///
    /// E195: this used to be the program-wide FIRST match by name, which is the
    /// shape E193 fixed for the struct-initializer head — and with two modules
    /// in one program declaring a `Dot` each, "first" is an answer about entity
    /// id order and not about this file. The resolution is
    /// [`Self::path_entity_id`], the analyzer's own: the scope chain at the
    /// cursor, then each further segment out of the namespace before it.
    ///
    /// The program-wide scan STAYS as the fallback, for the same reason
    /// [`CursorContext::StructInitializer`] keeps one: these callers hand this a
    /// name read off a RENDERED type label, and a label may name a type no
    /// scope of this file binds (a type reached only as another type's
    /// argument). A first match is then still better than silence — it is what
    /// this answered for every name before — and it can only be reached once
    /// scope resolution has declined.
    fn nominal_id_by_name(&self, name: &str, analyzed_offset: usize) -> Option<Id> {
        let program = self.program;
        let is_nominal =
            |id: &Id| program.structs.contains_key(id) || program.enums.contains_key(id);
        let segments: Vec<&str> = name.split("::").map(str::trim).collect();
        if let Some((last, namespace)) = segments.split_last()
            && let Some(id) = self
                .path_entity_id(namespace, last, analyzed_offset)
                .filter(is_nominal)
        {
            return Some(id);
        }
        let leaf = segments.last().copied().unwrap_or(name);
        program
            .structs
            .iter()
            .find(|(_, structure)| structure.name == leaf)
            .map(|(id, _)| *id)
            .or_else(|| {
                program
                    .enums
                    .iter()
                    .find(|(_, enumeration)| enumeration.name == leaf)
                    .map(|(id, _)| *id)
            })
    }

    /// The completion category for a name bound in a scope.
    fn kind_of(&self, id: Id) -> CompletionKind {
        kind_of(self.program, id)
    }

    /// Builds a completion for a named entity, enriched for the popup and for
    /// call-shaped insertion (WO-3): a function/method carries its full
    /// signature (`detail`), its `///` first paragraph (`documentation`), and
    /// its parameter names (`call_parameters`, `self` dropped) so the server can
    /// insert `name(…)`; a variable carries its rendered type as `detail`.
    /// Everything else is a bare name. `id` is the entity id bound in scope (or
    /// an impl member id for a method), resolved to a definition through
    /// [`Self::function_target`].
    fn entity_completion(&self, label: String, id: Id, kind: CompletionKind) -> Completion {
        let program = self.program;
        let mut completion = Completion::bare(label, kind);
        match completion.kind {
            CompletionKind::Function | CompletionKind::Method => {
                if let Some(target) = self.function_target(id) {
                    completion.detail = signature_label(program, target);
                    completion.documentation = self.doc_first_paragraph(target);
                    completion.call_parameters = call_parameter_names(program, target);
                    // E213: read off the DECLARATION the call would reach, so
                    // a method and a free function answer alike and an
                    // external (std's runtime seams are often externals) is
                    // not a third answer.
                    completion.internal =
                        vilan_core::labels::internal_of(program, target).map(str::to_string);
                }
            }
            CompletionKind::Variable => {
                completion.detail = self.hover_label(id);
                // E221: a module binding carries the label too.
                completion.internal =
                    vilan_core::labels::internal_of(program, id).map(str::to_string);
            }
            // E221: a struct, an enum or a trait — the nominal positions S1
            // left for this slice. One reader for every kind, so the rule the
            // candidate is filtered by cannot answer differently per kind.
            _ => {
                completion.internal =
                    vilan_core::labels::internal_of(program, id).map(str::to_string);
            }
        }
        completion
    }
}

pub(crate) fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// The cap [`Analysis::auto_import_completions`] truncates to (E54c) — a
/// std-heavy file can have a large loaded surface, and this is what keeps a
/// bare scope completion from drowning the popup in auto-import candidates
/// beneath the names actually in scope.
///
/// Kept at 20 by E59: the filing that found the flood was explicit that the
/// cap's SIZE was never the bug — a popup already offers 20 auto-import
/// candidates beneath the in-scope ones, plenty for a human to scan, and
/// [`import_origin_tier`] fixes which 20 those are. No evidence turned up
/// that a real file needs more of its own names surfaced at once than this;
/// raising it would only hand `std` back more of the slots `pkg` still
/// doesn't need.
pub const AUTO_IMPORT_COMPLETION_CAP: usize = 20;

/// An auto-import candidate's ranking tier by where it comes from (E59): the
/// user's own package (`pkg`) outranks the standard library (`std`) — a real
/// file's own unimported names are a far likelier completion target than the
/// always-loaded prelude's surface, which used to fill the whole cap first
/// purely because its capitalized trait/type names (`Add`, `BitAnd`, …) sort
/// ahead of an ordinary lowercase identifier in bare alphabetical order. Used
/// both pre-truncation ([`Analysis::auto_import_completions`]'s sort) and in
/// the client-visible `sort_text` (`main::to_completion_item`, via
/// [`AutoImport::origin_tier`]) — the one mapping, read in both places.
///
/// Tier 1 is reserved for a dependency package's names, ranked between the
/// two: closer to the user's intent than `std`'s always-loaded surface (the
/// user chose to add the dependency), but not the user's own authored code.
/// It is unreachable today — `auto_import_completions` only ever calls this
/// with `"std"` or `"pkg"`, since E54 scoped this keystroke-path candidate
/// gathering to those two roots (a dependency scan is the disk-bound
/// full-origin one `Analysis::import_candidates` pays for the on-demand
/// quickfix, not this path) — recorded here for whenever that changes rather
/// than left for a future tier scheme to rediscover.
fn import_origin_tier(root: &str) -> u8 {
    match root {
        "pkg" => 0,
        "std" => 2,
        _ => 1,
    }
}

/// A selector subject's written HEAD — `List` for `List<i32>`, `Boxed` for
/// `item::Boxed<_>` — which is what [`vilan_core::analyzer::module_impl_blocks`]
/// keys its rows on.
fn selector_subject_head(subject: &str) -> &str {
    let head = subject.split('<').next().unwrap_or(subject).trim();
    head.rsplit("::").next().unwrap_or(head).trim()
}

/// The origins an import path may start with: the two the loader always knows
/// (`std`, `pkg`) plus every dependency package, under the name this file
/// addresses it by (E57).
fn origin_completions(roots: &ImportRoots) -> Vec<Completion> {
    let mut items = vec![
        Completion::bare("std".to_string(), CompletionKind::Module),
        Completion::bare("pkg".to_string(), CompletionKind::Module),
    ];
    items.extend(
        roots
            .dependencies
            .iter()
            .map(|(name, _)| Completion::bare(name.clone(), CompletionKind::Module)),
    );
    items
}

/// What `origin::module::` offers: the module's own importable names, read on
/// demand from its source file, plus (A65) the SUBMODULES its directory holds.
///
/// The segments typed are a PATH, and where the module ends is a question about
/// the disk — `pkg::lib::ui::widget::` is the module `lib/ui/widget.vl` with
/// nothing past it, while `pkg::lib::util::Tab::` is `lib/util.vl` with an enum
/// past it. So the longest prefix that resolves is the module, exactly as the
/// loader decides it, and what is left is the descent: a TYPE name descends
/// into that type's namespace — an enum's variants and, B317, the self-less
/// functions this module's own `impl` blocks declare for it, which is the whole
/// of what `resolve_import` descends into past a module; anything deeper offers
/// nothing.
///
/// A path that resolves to no module at all may still be a pure NAMESPACE — a
/// directory with no `lib.vl` — and then the children are the whole answer,
/// which is also the only thing an import of it could name.
fn module_member_completions(
    module_roots: &[&Path],
    module: &str,
    past_module: &[&str],
) -> Vec<Completion> {
    let mut segments: Vec<&str> = Vec::with_capacity(1 + past_module.len());
    segments.push(module);
    segments.extend(past_module.iter().copied());
    for cut in (1..=segments.len()).rev() {
        let path = segments[..cut].join("::");
        let file = vilan_core::analyzer::module_source_file(module_roots, &path);
        let children = vilan_core::analyzer::submodules_in_roots(module_roots, &path);
        // A prefix names something when it has a body, or children, or both. A
        // pure namespace has only children, which is exactly what an import
        // through it can reach.
        if file.is_none() && children.is_empty() {
            continue;
        }
        let importables = file
            .as_deref()
            .map(vilan_core::analyzer::module_importables)
            .unwrap_or_default();
        // E178: the visibility bit, here as at the origin listing — a module's
        // private machinery is not offered to an import path.
        let offered = offered_importables(&importables);
        let Some((name, past_enum)) = segments[cut..].split_first() else {
            let mut items: Vec<Completion> = offered
                .iter()
                .map(|importable| importable_completion(importable))
                .collect();
            // The directory's children, after the module's own names: an item
            // and a submodule can share a spelling, and the module's own item
            // is what an import of that name binds (the walk asks the item
            // scope first).
            let seen: HashSet<&str> = importables
                .iter()
                .map(|importable| importable.name)
                .collect();
            for child in children {
                if !seen.contains(child.as_str()) {
                    items.push(Completion::bare(child, CompletionKind::Module));
                }
            }
            return items;
        };
        if !past_enum.is_empty() {
            return Vec::new();
        }
        return offered
            .iter()
            .find(|importable| {
                importable.name == *name
                    && (matches!(
                        importable.kind,
                        vilan_core::analyzer::ImportableKind::Enum
                            | vilan_core::analyzer::ImportableKind::Struct
                    ) || !importable.statics.is_empty())
            })
            .map(|type_| {
                // B317: a type's namespace, both halves of it — the variants an
                // enum declares, and the self-less functions this module's own
                // `impl` blocks declare for it, which is exactly the set an
                // import through this module can bind. A row that is neither a
                // struct nor an enum but CARRIES statics is an extension impl
                // beside an `export import` of the type it extends: the name is
                // re-exported here and the block is written here, so both are
                // reachable through this module and this is where they are
                // offered.
                type_
                    .variants
                    .iter()
                    .map(|variant| {
                        Completion::bare(variant.to_string(), CompletionKind::EnumVariant)
                    })
                    .chain(type_.statics.iter().map(|static_| {
                        Completion::bare(static_.to_string(), CompletionKind::Function)
                    }))
                    .collect()
            })
            .unwrap_or_default();
    }
    Vec::new()
}

/// One importable name as a completion candidate. Bare by construction — an
/// import binds a name, it never calls it — so the shaping post-pass in
/// [`Analysis::completion`] has nothing left to strip.
/// B318 §1's completion filter (E178): the rows of a module whose importables
/// are `importables` that completion may OFFER.
///
/// The bit gates three tooling consumers, and completion is the one S1 left —
/// the add-import quickfix reads it (`Document::import_candidates`) and the
/// steers read it, while an import-path popup went on listing every name a
/// module declares, private ones included. It is the same one-line rule in both
/// places, under the same **uncurated-module exemption**: a module carrying no
/// `export` marker anywhere offers everything it declares, exactly as it did
/// before the bit existed, because on the day the marker gains meaning no
/// module in the estate has written one and hiding every name in std from
/// completion is not a migration, it is an outage. The plain-reach WARNING is
/// what tells an author to curate; this is what stops the tooling punishing
/// them for not having done it yet.
///
/// A NARROWED export (`export(in pkg)`) counts as offered, which is
/// `Visibility::is_exported`'s own documented rule: none of the three consumers
/// has an importing file to test a narrowing against — a completion list is
/// offered before the import exists — and letting the reach warning correct an
/// out-of-scope use beats hiding a name the module's author deliberately
/// published.
fn offered_importables<'a>(
    importables: &'a [vilan_core::analyzer::Importable<'a>],
) -> Vec<&'a vilan_core::analyzer::Importable<'a>> {
    let curated = vilan_core::analyzer::module_is_curated(importables);
    importables
        .iter()
        .filter(|row| !curated || row.exported.is_exported())
        .collect()
}

/// The start of the word ending at `offset` under the HYPHENATED rule (E211):
/// identifier bytes plus `-`, which is what makes `stroke-width`,
/// `font-family` and `--card-gap` one name each.
///
/// Used only where a hyphen can be part of a NAME — inside a css block and
/// inside an element head. In code a `-` is the subtraction operator and a
/// vilan identifier cannot contain one, so the ordinary identifier scan stands
/// there and `a-b` is not offered as one word.
fn hyphenated_word_start(bytes: &[u8], offset: usize) -> usize {
    let mut start = offset.min(bytes.len());
    while start > 0 && (is_identifier_byte(bytes[start - 1]) || bytes[start - 1] == b'-') {
        start -= 1;
    }
    start
}

/// Stamps every candidate of one request with the span it replaces and the
/// text the client should filter it by (E211).
///
/// One place, applied to every context's list, because the answer is a property
/// of the REQUEST and not of the candidate: the prefix being replaced is where
/// the cursor is, and a candidate that carried its own would be a second
/// opinion about that. `filter_text` is the label — stating it explicitly is
/// what an LSP client without a `wordPattern` of its own needs, and it is what
/// the label already meant for every client that has one.
fn stamp_replacements(candidates: Vec<Completion>, replaced: Span) -> Vec<Completion> {
    candidates
        .into_iter()
        .map(|mut candidate| {
            candidate.filter_text = Some(candidate.label.clone());
            candidate.replace_span = Some(replaced);
            candidate
        })
        .collect()
}

fn importable_completion(importable: &vilan_core::analyzer::Importable) -> Completion {
    use vilan_core::analyzer::ImportableKind;
    let kind = match importable.kind {
        ImportableKind::Function => CompletionKind::Function,
        ImportableKind::Macro => CompletionKind::Macro,
        ImportableKind::Struct => CompletionKind::Struct,
        ImportableKind::Enum => CompletionKind::Enum,
        ImportableKind::Trait => CompletionKind::Trait,
        ImportableKind::Value => CompletionKind::Variable,
        ImportableKind::Module => CompletionKind::Module,
        // A re-export names whatever it points at, and the module file that
        // publishes it does not say what that is — following the path to find
        // out is a further load per candidate, deliberately not paid here.
        ImportableKind::Reexport => CompletionKind::Variable,
    };
    Completion::bare(importable.name.to_string(), kind)
}

/// The nominal name in a rendered type label: `struct Point` -> `Point`,
/// `enum Option<i32>` -> `Option` (drops the `struct`/`enum`/`trait` prefix the
/// type renderer adds, plus any type arguments and surrounding whitespace).
/// The first generic argument of a rendered type label — `Option<Profile>` →
/// `Profile`, `Result<User, str>` → `User` (nesting respected).
fn first_generic_argument(label: &str) -> Option<&str> {
    let open = label.find('<')?;
    let inner = &label[open + 1..];
    let mut depth = 0usize;
    for (index, character) in inner.char_indices() {
        match character {
            '<' => depth += 1,
            '>' if depth == 0 => return Some(inner[..index].trim()),
            '>' => depth -= 1,
            ',' if depth == 0 => return Some(inner[..index].trim()),
            _ => {}
        }
    }
    None
}

fn base_type_name(label: &str) -> &str {
    let label = label.trim();
    let label = ["struct ", "enum ", "trait "]
        .iter()
        .find_map(|prefix| label.strip_prefix(prefix))
        .unwrap_or(label);
    label.split('<').next().unwrap_or(label).trim()
}

/// The identifier ending at byte `end` in `text`, if any.
/// The index of the bracket token that OPENS the one at `close`, by depth over
/// the token stream — `None` when the buffer is mid-edit and there is no match.
///
/// Token space, not bytes: a `(` inside a string literal is part of one token
/// and cannot be counted, which is the whole reason a live receiver's shape is
/// walked here rather than over the text (E131).
fn matching_open_bracket(tokens: &[(Token<'_>, Span)], close: usize) -> Option<usize> {
    let mut depth = 0usize;
    for index in (0..=close).rev() {
        match tokens[index].0 {
            Token::Ctrl(')') | Token::Ctrl(']') | Token::Ctrl('}') => depth += 1,
            Token::Ctrl('(') | Token::Ctrl('[') | Token::Ctrl('{') => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The `::`-separated identifier path ending at `end` — `["style",
/// "FlexDirection"]` for `let d = style::FlexDirection::|`, in source order.
///
/// [`identifier_ending_at`] walks back over identifier bytes only, so it stops
/// at a `::` and sees one segment; this repeats it across each `::` it lands
/// on, which is what lets a code path descend as far as the user has written
/// it (E129). `None` when there is no identifier at all just before `end`.
fn code_path_segments(text: &str, end: usize) -> Option<Vec<&str>> {
    let bytes = text.as_bytes();
    let mut segments = Vec::new();
    let mut end = end.min(bytes.len());
    loop {
        let segment = identifier_ending_at(text, end)?;
        segments.push(segment);
        let start = end - segment.len();
        if start >= 2 && bytes[start - 1] == b':' && bytes[start - 2] == b':' {
            end = start - 2;
        } else {
            break;
        }
    }
    segments.reverse();
    Some(segments)
}

fn identifier_ending_at(text: &str, end: usize) -> Option<&str> {
    let bytes = text.as_bytes();
    let mut start = end.min(bytes.len());
    while start > 0 && is_identifier_byte(bytes[start - 1]) {
        start -= 1;
    }
    (start < end).then(|| &text[start..end])
}

/// How completion inserts a function or method call — the language server's
/// `vilan.completion.functionCall` setting, consumed by [`call_insertion`]:
/// `Full` fills named parameter tab-stops, `ParensOnly` inserts the
/// parentheses, `None` inserts the bare name. The playground fixes `Full`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompletionFunctionCall {
    /// Insert the name only.
    None,
    /// Insert `name()` (empty parentheses).
    ParensOnly,
    /// Insert `name(…)` with a placeholder argument list.
    Full,
}

/// What accepting a call-shaped completion inserts: the text, and whether it
/// is an LSP-syntax snippet (`${1:name}` tab-stops, `$0` the final cursor)
/// rather than plain text.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct InsertText {
    pub text: String,
    pub is_snippet: bool,
}

/// The insert text for a call-shaped completion, or `None` when the mode is
/// `None` — leaving the bare label. `Full` fills each parameter as a named
/// tab-stop (`name(${1:a}, ${2:b})$0`); `ParensOnly` positions the cursor
/// between the parens (`name($0)`); both write `name()$0` for a zero-parameter
/// callable. Without snippet support every shape degrades to the plain
/// `name()` (cursor after) — a snippet's tab-stops would otherwise surface as
/// literal text. One rule for both front-ends (the language server maps
/// `is_snippet` to `InsertTextFormat`; the playground to a CodeMirror snippet).
pub fn call_insertion(
    label: &str,
    parameters: &[String],
    mode: CompletionFunctionCall,
    snippet_support: bool,
) -> Option<InsertText> {
    if matches!(mode, CompletionFunctionCall::None) {
        return None;
    }
    if !snippet_support {
        return Some(InsertText {
            text: format!("{label}()"),
            is_snippet: false,
        });
    }
    let text = if parameters.is_empty() {
        format!("{label}()$0")
    } else {
        match mode {
            CompletionFunctionCall::Full => {
                let placeholders: Vec<String> = parameters
                    .iter()
                    .enumerate()
                    .map(|(index, name)| format!("${{{}:{name}}}", index + 1))
                    .collect();
                format!("{label}({})$0", placeholders.join(", "))
            }
            // `ParensOnly` (with parameters): cursor inside the parens.
            _ => format!("{label}($0)"),
        }
    };
    Some(InsertText {
        text,
        is_snippet: true,
    })
}

// ---------------------------------------------------------------------------
// The completion index (E121 §2.1.4, M25)
// ---------------------------------------------------------------------------

/// Everything completion needs that is a function of the ANALYSIS ALONE —
/// built once when an analysis lands, read by every request until the next one.
///
/// M25's find, and the reason this type exists. Two of `completion`'s
/// gatherers cost the CODEBASE on every keystroke, inside a request that the
/// mandate says must cost the FILE:
///
/// - [`AutoImportOrder`] — `auto_import_completions` swept every `std`/`pkg`
///   child module's `name_to_id_map`, classified each name and sorted the
///   result, per request. On kolt's `views.vl` that sweep found 2,239
///   candidates and cost 1.05 ms of the 2.19 ms a scope completion took; on
///   E121's generated 1,791-function exhibit it was most of 0.63 ms.
/// - [`OriginListing`] — an import path's origin arm called
///   [`vilan_core::analyzer::modules_in_root`], a `read_dir` per source root,
///   per request. §2.1 is explicit that the keystroke path may not read the
///   filesystem at all.
///
/// Neither depends on the cursor, on the live buffer, or on the file's own
/// scope, so neither belongs in a request. Both are derived here from the
/// analyzed `Program` and the package tree it resolved, on the thread that
/// produced them — the language server's analysis thread
/// (`Document::capture_landed`) and the playground's `Retained::new`. A
/// request reads the table and pays for the answer, not for the derivation.
///
/// The freshness rule is the one §2.1.4 asks for, stated exactly: an entry is
/// as old as the analysis it was built from, and it is replaced wholesale when
/// the next analysis lands. Every event that could move it — an edit to any
/// module, a manifest change, a file appearing or disappearing under a source
/// root — is already an event that re-analyzes, so there is no second
/// invalidation protocol to keep truthful and no way for the two to disagree.
#[derive(Clone, Debug, Default)]
pub struct CompletionIndex {
    auto_import: AutoImportOrder,
    origins: Vec<OriginListing>,
    members: MemberTable,
    docs: DocParagraphs,
}

impl CompletionIndex {
    /// Derive the index from a landed analysis. `import_roots` is the package
    /// tree that analysis resolved under — `None` for the degraded
    /// internal-error document, which can enumerate no origin; `analyzed` is
    /// the ENTRY TEXT that analysis ran on, which every candidate's import edit
    /// is computed against (M29).
    pub fn build(
        program: &Program,
        import_roots: Option<&ImportRoots>,
        analyzed: &str,
    ) -> CompletionIndex {
        CompletionIndex {
            auto_import: AutoImportOrder::build(program, analyzed),
            origins: import_roots
                .map(|roots| OriginListing::build(roots, program.platform))
                .unwrap_or_default(),
            members: MemberTable::build(program),
            docs: DocParagraphs::build(program),
        }
    }

    /// A non-entry declaration's rendered first doc paragraph (M39).
    ///
    /// `None` means the table does not COVER this declaration — the entry's
    /// own, a derived one, something that is not a function — and the caller
    /// must render it itself. `Some(None)` means it is covered and has no
    /// doc, which is the answer, and is why this is not a plain
    /// `Option<&str>`: an undocumented std function that fell through to the
    /// renderer would read its module's whole text to discover there was
    /// nothing above it, which is the very cost the table exists to remove.
    pub(crate) fn doc_paragraph(&self, declaration_id: Id) -> Option<Option<&str>> {
        self.docs
            .by_declaration
            .get(&declaration_id)
            .map(|paragraph| paragraph.as_deref())
    }

    /// The modules and surface `origin::` offers, as the package tree stood
    /// when the analysis ran. `None` for a name that is not an origin.
    fn origin(&self, origin: &str) -> Option<&OriginListing> {
        self.origins.iter().find(|listing| listing.origin == origin)
    }
}

/// Every NON-ENTRY function's `///` first paragraph, rendered ONCE per
/// analysis (M39).
///
/// Rendering one costs the DECLARING MODULE'S TEXT. `Analysis::source_text`
/// caches a module's text per QUERY, so a completion offering names imported
/// from a large module read that module on every keystroke — the last cost
/// left in a completion request once M25 and M29 had taken the table
/// derivations out (0.415 ms per request, 0.324 ms with the render stubbed).
///
/// Nothing about it depends on the cursor or the live buffer, which is the
/// same argument [`MemberTable`] and [`AutoImportOrder`] rest on, and the
/// freshness rule is theirs unchanged: an entry is as old as the analysis it
/// was built from and is replaced wholesale when the next one lands, so an
/// edit to any module — the declaring one included — is already an event that
/// rebuilds this.
///
/// **The entry's own declarations are excluded.** Their text is the analyzed
/// text the query already holds, so rendering them here would buy nothing and
/// would put the one text a keystroke changes into a captured table. One read
/// per declaring module, not one per declaration: the sources are visited in
/// order and each is read once.
#[derive(Clone, Debug, Default)]
struct DocParagraphs {
    /// Every non-entry function and external the program declares, whether or
    /// not it HAS a doc — an absent entry means "not covered", never "not
    /// documented", so an undocumented declaration is answered from the table
    /// rather than by reading its module to find nothing.
    by_declaration: HashMap<Id, Option<String>>,
}

impl DocParagraphs {
    fn build(program: &Program) -> DocParagraphs {
        // (declaration, name-span start), grouped by declaring source, so a
        // module's text is read once however many declarations it carries.
        //
        // M65: `source_of` is a LINEAR scan of `source_ranges`, and this loop
        // asks it once per declaration in the whole program — on kolt's client
        // that is the bulk of the 14,580 calls the index build makes, against
        // about sixty ranges. `source_lookup` is the same question answered by a
        // binary search that verifies the ranges are ascending and disjoint and
        // falls back to the very scan otherwise, so it is answer-identical by
        // construction (M27, pinned since M58).
        let source_of = program.source_lookup();
        let mut by_source: HashMap<SourceId, Vec<(Id, usize)>> = HashMap::default();
        let declarations = program
            .functions
            .iter()
            .map(|(id, function)| (*id, function.name_span))
            .chain(
                program
                    .external_functions
                    .iter()
                    .map(|(id, external)| (*id, external.name_span)),
            );
        for (id, name_span) in declarations {
            let Some(source) = source_of.of(id) else {
                continue;
            };
            // The entry (and the derived sentinel, which has no file) render
            // lazily where the text is already in hand.
            if source == SourceId(0) || program.source_path(source).is_none() {
                continue;
            }
            by_source
                .entry(source)
                .or_default()
                .push((id, name_span.into_range().start));
        }
        let mut by_declaration: HashMap<Id, Option<String>> = HashMap::default();
        for (source, declarations) in by_source {
            let Some(path) = program.source_path(source) else {
                continue;
            };
            let Ok(text) = vilan_core::util::read_source(path) else {
                continue;
            };
            for (id, start) in declarations {
                let paragraph = crate::analysis::doc_comment_above(&text, start)
                    .and_then(|docs| crate::analysis::first_paragraph(&docs));
                by_declaration.insert(id, paragraph);
            }
        }
        DocParagraphs { by_declaration }
    }
}

/// Every nominal type's member surface, derived ONCE per analysis (M29).
///
/// Deriving it per request is what member completion used to do, and it is a
/// walk over the whole program: `push_methods` scanned every `Implementation`
/// the analysis holds to find the ones whose subject is this type, and
/// `push_inherited_defaults` scanned them again and then every trait and
/// supertrait each one provides. On kolt that made a member request the single
/// most expensive thing the keystroke path answered (2.63 ms), and the cost
/// followed the CODEBASE rather than the file — the same shape M25 took off
/// the auto-import arm, in the one gatherer M25 did not touch.
///
/// Nothing in that derivation depends on the cursor or on the live buffer: it
/// is a function of the analyzed program alone, so it is built where the
/// program is (the server's analysis thread, the playground's `Retained::new`)
/// and a request reads a lookup. The capture can never be staler than the
/// analysis the request is already being answered from, which is M25's own
/// argument for the tables beside it — and E131 is what makes that safe at a
/// member position specifically: the receiver's IDENTITY comes from the live
/// text, and only the TYPE's surface is read from the landing.
///
/// One pass fills every type, and the pass is a regrouping rather than extra
/// work: the impls are visited once in total instead of once per request per
/// type.
#[derive(Clone, Debug, Default)]
struct MemberTable {
    by_type: HashMap<Id, TypeMembers>,
}

/// One nominal type's methods, split by how they are called.
#[derive(Clone, Debug, Default)]
struct TypeMembers {
    /// `value.method()` — the impls' own `self` methods, then the
    /// default-bodied instance methods their traits inherit.
    instance: Vec<(String, Id)>,
    /// `Type::method()` — statics and associated functions.
    statics: Vec<(String, Id)>,
}

impl MemberTable {
    /// Group every impl in the program by the nominal its subject names, then
    /// resolve each group's member surface with the precedence the per-request
    /// walk used: a DECLARATION wins its name outright, so an impl that
    /// overrides a trait default offers the override and not both.
    fn build(program: &Program) -> MemberTable {
        let mut by_type: HashMap<Id, TypeMembers> = HashMap::default();
        let mut grouped: HashMap<Id, Vec<&Implementation>> = HashMap::default();
        for implementation in &program.implementations {
            if let Some(type_id) = nominal_type_id(program, implementation.subject) {
                grouped.entry(type_id).or_default().push(implementation);
            }
        }
        for (type_id, implementations) in grouped {
            let mut members = TypeMembers::default();
            let mut instance_names: HashSet<&str> = HashSet::new();
            let mut static_names: HashSet<&str> = HashSet::new();
            for implementation in &implementations {
                for (name, member_id) in &implementation.declarations {
                    if is_self_method(program, *member_id) {
                        if instance_names.insert(name) {
                            members.instance.push((name.to_string(), *member_id));
                        }
                    } else if static_names.insert(name) {
                        members.statics.push((name.to_string(), *member_id));
                    }
                }
            }
            // The default-bodied INSTANCE methods the impls' traits (and their
            // supertraits) declare and the impls themselves do not
            // (kolt.local 033: reading only the declarations left every trait
            // default invisible on every implementing type — `list.iter().`
            // offered `next` and nothing else).
            //
            // The admission rule is the analyzer's own
            // (`Analyzer::inherited_default_candidates`), so the popup and the
            // call site agree on what the concrete type provides: a member with
            // no default body is never inherited (conformance forces the impl to
            // declare it, and the pass above already found it there), a
            // `[trait_only]` default stays off the concrete surface
            // (`proposal/transport-rpc.md` §3.2), and a static has no inherited
            // path onto a value at all.
            for implementation in &implementations {
                for trait_id in &implementation.trait_ids {
                    for home_id in trait_with_supertraits(program, *trait_id) {
                        let Some(home) = program.traits.get(&home_id) else {
                            continue;
                        };
                        for (name, member_id) in &home.declarations {
                            if member_has_default_body(program, *member_id)
                                && !declaration_is_trait_only(program, *member_id)
                                && is_self_method(program, *member_id)
                                && instance_names.insert(name)
                            {
                                members.instance.push((name.to_string(), *member_id));
                            }
                        }
                    }
                }
            }
            by_type.insert(type_id, members);
        }
        MemberTable { by_type }
    }

    /// `type_id`'s instance methods (`want_self`) or its statics, in the order
    /// the impls declare them. Empty for a type with no impl at all.
    fn methods(&self, type_id: Id, want_self: bool) -> &[(String, Id)] {
        self.by_type
            .get(&type_id)
            .map(|members| {
                if want_self {
                    &members.instance
                } else {
                    &members.statics
                }
            })
            .map_or(&[][..], Vec::as_slice)
    }
}

/// Whether a method's first parameter is `self` — i.e. it is called on a value
/// (`v.method()`) rather than on the type (`Type::method()`).
fn is_self_method(program: &Program, member_id: Id) -> bool {
    let first_parameter = match program.entity_map.get(&member_id) {
        Some(Expr::Function(function_id)) => program
            .functions
            .get(function_id)
            .and_then(|function| function.parameters.first()),
        Some(Expr::ExternalFunction(external_id)) => program
            .external_functions
            .get(external_id)
            .and_then(|external| external.parameters.first()),
        _ => None,
    };
    first_parameter
        .and_then(|parameter_id| program.parameters.get(parameter_id))
        .is_some_and(|parameter| parameter.name == "self")
}

/// `trait_id` plus its transitive supertraits — a trait's full interface
/// includes everything its supertraits declare (`trait Ord with Eq +
/// PartialOrd` reaches `PartialOrd`'s `lt`/`le`/`gt`/`ge`).
fn trait_with_supertraits(program: &Program, trait_id: Id) -> Vec<Id> {
    let mut result = Vec::new();
    let mut pending = vec![trait_id];
    while let Some(id) = pending.pop() {
        if result.contains(&id) {
            continue;
        }
        result.push(id);
        let Some(trait_) = program.traits.get(&id) else {
            continue;
        };
        for supertrait in &trait_.supertraits {
            if let Some(Type::Trait(super_id, _)) = program.type_id_to_type_map.get(supertrait) {
                pending.push(*super_id);
            }
        }
    }
    result
}

/// Whether a trait member has a source-provided body — a DEFAULT method,
/// which an implementing type inherits.
fn member_has_default_body(program: &Program, member_id: Id) -> bool {
    match program.entity_map.get(&member_id) {
        Some(Expr::Function(function_id)) => program
            .functions
            .get(function_id)
            .is_some_and(|function| function.has_body),
        _ => false,
    }
}

/// Whether a trait member is marked `[trait_only]` — reachable through a
/// bound, never on the concrete type's own member surface.
fn declaration_is_trait_only(program: &Program, member_id: Id) -> bool {
    match program.entity_map.get(&member_id) {
        Some(Expr::Function(function_id)) => program
            .functions
            .get(function_id)
            .is_some_and(|function| function.trait_only),
        _ => false,
    }
}

/// One origin's module listing — `std`, `pkg`, or a dependency package under
/// the name this file addresses it by.
///
/// The names are [`vilan_core::analyzer::modules_in_root`]'s, over the origin's
/// source roots in the loader's own order with an earlier root shadowing a
/// later one, and with the package SURFACE (`lib.vl`) dropped: it is integrated
/// into the package name and `import std::lib` is not how anyone reaches its
/// members. That is the same reduction `origin_member_completions` used to
/// perform per request, moved to the analysis that already read the tree.
#[derive(Clone, Debug)]
struct OriginListing {
    origin: String,
    /// `(module name, source file)`, first root wins, `lib` excluded.
    modules: Vec<(String, PathBuf)>,
    /// A65: the pure NAMESPACES directly under the origin's roots — module
    /// directories with no `lib.vl`, which have no source file of their own but
    /// are the head of every path into what they hold. Captured beside
    /// `modules` for the same reason those are: it is a `read_dir`, and §2.1
    /// keeps the keystroke path off the filesystem.
    namespaces: Vec<String>,
    /// The `lib.vl` this origin publishes, where it has one.
    surface: Option<PathBuf>,
}

impl OriginListing {
    /// What `origin::` offers: every module under the origin's source roots, in
    /// the loader's own root order (an earlier root shadows a later one, so a
    /// matching platform layer wins over the base), followed by the names its
    /// `lib.vl` surface publishes.
    ///
    /// The surface's importables are read here rather than captured because
    /// they come from [`vilan_core::analyzer::module_importables`], which
    /// answers through the shared parse cache — warm after the analysis that
    /// built this listing already loaded the file.
    fn completions(&self) -> Vec<Completion> {
        let mut items: Vec<Completion> = self
            .modules
            .iter()
            .map(|(name, _path)| Completion::bare(name.clone(), CompletionKind::Module))
            .collect();
        let mut seen: HashSet<String> = self.modules.iter().map(|(name, _)| name.clone()).collect();
        for namespace in &self.namespaces {
            if seen.insert(namespace.clone()) {
                items.push(Completion::bare(namespace.clone(), CompletionKind::Module));
            }
        }
        // E178: the surface's own names, filtered on the visibility bit under
        // the uncurated-module exemption (see [`offered_importables`]). The
        // MODULES above are not filtered and are not a leak: a module is a
        // file, `export` marks items inside one, and a private `mod` is a
        // different question (B318's open (d)).
        let importables = self
            .surface
            .as_deref()
            .map(vilan_core::analyzer::module_importables)
            .unwrap_or_default();
        for importable in offered_importables(&importables) {
            if seen.insert(importable.name.to_string()) {
                items.push(importable_completion(importable));
            }
        }
        items
    }

    fn build(roots: &ImportRoots, platform: BuildPlatform) -> Vec<OriginListing> {
        let mut origins: Vec<String> = vec!["std".to_string(), "pkg".to_string()];
        origins.extend(roots.dependencies.iter().map(|(name, _)| name.clone()));
        origins
            .into_iter()
            .filter_map(|origin| {
                let (module_roots, surface) = roots.origin_roots(&origin, platform)?;
                let mut modules: Vec<(String, PathBuf)> = Vec::new();
                for root in &module_roots {
                    for (name, path) in vilan_core::analyzer::modules_in_root(root) {
                        // The package SURFACE is the FLAT `lib.vl` at the root,
                        // and only that: A65 makes a DIRECTORY called `lib` an
                        // ordinary module of the package (it is the exhibit's
                        // own name), so the drop is by path, not by spelling.
                        let is_surface = name == "lib" && path.parent() == Some(root);
                        if is_surface || modules.iter().any(|(known, _)| *known == name) {
                            continue;
                        }
                        modules.push((name, path));
                    }
                }
                // A65: and the bodiless module directories, which `modules_in_root`
                // cannot list because it answers with a FILE per name.
                let namespaces: Vec<String> =
                    vilan_core::analyzer::submodules_in_roots(&module_roots, "")
                        .into_iter()
                        .filter(|name| !modules.iter().any(|(known, _)| known == name))
                        .collect();
                Some(OriginListing {
                    origin,
                    modules,
                    namespaces,
                    surface,
                })
            })
            .collect()
    }
}

/// The auto-import candidate table (E54c/E59): every name under `std` or `pkg`
/// that this program could add an import for, in the order a completion answer
/// takes them.
///
/// The derivation is `auto_import_completions`' own, unchanged — the same two
/// roots, the same "declared HERE, not re-exported here" rule
/// ([`Program::source_of`]), the same excluded kinds, the same
/// [`import_origin_tier`] ranking and the same alphabetical tiebreak. Only its
/// TIME moved: it runs once per analysis instead of once per keystroke.
///
/// **Why filtering later is the same answer.** The per-request form dropped
/// the names already in scope *before* sorting and truncating; this one sorts
/// everything once and skips them while taking. `sort_by` is stable, and a
/// stable sort of a subsequence is the same subsequence of the stable sort — so
/// the first `cap` survivors are the same candidates in the same order, which
/// is what [`AUTO_IMPORT_COMPLETION_CAP`]'s "which 20" argument depends on.
#[derive(Clone, Debug, Default)]
pub struct AutoImportOrder {
    /// The modules that contribute, in discovery order; a candidate names one
    /// by index. Grouping the path here rather than on every candidate is what
    /// makes the table proportional to the names, not to the names times their
    /// module path.
    modules: Vec<AutoImportModule>,
    /// Every candidate, sorted by `(tier, name)`.
    candidates: Vec<AutoImportCandidate>,
}

#[derive(Clone, Debug)]
struct AutoImportModule {
    /// The segments an `import` needs before a name — `["std", "json"]`.
    path: Vec<String>,
}

#[derive(Clone, Debug)]
struct AutoImportCandidate {
    tier: u8,
    name: String,
    kind: CompletionKind,
    module: u32,
    /// The import edit this candidate carries, computed at BUILD time against
    /// the analyzed text and in that text's coordinates (M29) — `None` for a
    /// name already imported, or in a buffer that did not parse cleanly, which
    /// is the same `None` the per-request probe answered.
    edit: Option<CapturedImportEdit>,
}

/// One candidate's ready-made `import` edit, in ANALYZED coordinates.
#[derive(Clone, Debug)]
struct CapturedImportEdit {
    span: Span,
    replacement: String,
}

impl AutoImportOrder {
    fn build(program: &Program, analyzed: &str) -> AutoImportOrder {
        // M65, as in [`DocParagraphs::build`]: the declares-it test below is
        // asked once per NAME in every module of `std` and `pkg`, and
        // `source_of` is a linear scan.
        let source_of = program.source_lookup();
        let mut modules: Vec<AutoImportModule> = Vec::new();
        let mut candidates: Vec<AutoImportCandidate> = Vec::new();
        for root in ["std", "pkg"] {
            let Some(&root_module_id) = program.module_id_by_name.get(root) else {
                continue;
            };
            let Some(root_module) = program.modules.get(&root_module_id) else {
                continue;
            };
            let Some(root_scope) = program.scopes.get(&root_module.body.1) else {
                continue;
            };
            let tier = import_origin_tier(root);
            for &child_id in root_scope.name_to_id_map.values() {
                let Some(child_module) = program.modules.get(&child_id) else {
                    continue;
                };
                let Some(child_scope) = program.scopes.get(&child_module.body.1) else {
                    continue;
                };
                let child_source = source_of.of(child_id);
                // E184 — the FOURTH consumer of B318 §1's visibility bit, and
                // the one E178 had to leave behind. The bit's other route is
                // `module_importables`, a syntactic read of a module's FILE,
                // and this table is built once per ANALYSIS, on the analysis
                // thread but OUTSIDE the scope that owns overlay loads
                // (`document.rs` builds the index after the analysis returns).
                // Asking it here parsed every std and pkg module into the
                // process-global, content-keyed parse cache — for a module the
                // user has OPEN, a fresh entry per keystroke, which is exactly
                // §7.5's session leak M9 closed, and four
                // `overlay_module_reclaim` pins caught it immediately
                // (`ParseCleanCacheText` grew 36 and 272 bytes).
                //
                // So the answer does not come from the file: visibility-36 put
                // it on `Program` for this consumer, computed by the walk that
                // already had it. The predicate below IS
                // `Analyzer::is_exported_in` — held equal to
                // `module_importables`' own answer by
                // `module_resolution.rs::e178_the_program_visibility_fields_
                // agree_with_module_importables` — and it reads two sets that
                // are in hand, so the keystroke path parses nothing at all.
                let curated = program.curated_modules.contains(&child_module.body.1);
                let module = modules.len() as u32;
                modules.push(AutoImportModule {
                    path: vec![root.to_string(), child_module.name.to_string()],
                });
                for (&name, &entity_id) in &child_scope.name_to_id_map {
                    // Only a name this module DECLARES is an add-import target;
                    // a re-export names an item that lives somewhere else.
                    if source_of.of(entity_id) != child_source {
                        continue;
                    }
                    // E184, under the UNCURATED-MODULE EXEMPTION that carries
                    // the whole estate (visibility.md §14): a module with no
                    // `export` marker anywhere offers everything it declares,
                    // exactly as it did before the bit existed, so no package
                    // that has not curated loses a candidate.
                    if curated && !program.exported_entities.contains(&entity_id) {
                        continue;
                    }
                    let kind = kind_of(program, entity_id);
                    if matches!(
                        kind,
                        CompletionKind::Module | CompletionKind::Keyword | CompletionKind::Snippet
                    ) {
                        continue;
                    }
                    candidates.push(AutoImportCandidate {
                        tier,
                        name: name.to_string(),
                        kind,
                        module,
                        edit: None,
                    });
                }
            }
        }
        candidates.sort_by(|left, right| {
            left.tier
                .cmp(&right.tier)
                .then_with(|| left.name.cmp(&right.name))
        });
        // Each candidate's IMPORT EDIT, computed here against the analyzed text
        // rather than in the request (M29). A request used to parse the whole
        // live buffer for this — 0.14 ms of a 0.55 ms scope completion on
        // E121's exhibit, and the largest single item left in it after M25 —
        // and then probe the parse once per surviving candidate. The probes are
        // cheap; the parse was the bill, and it is a function of the text the
        // analysis ran on, so it belongs to the analysis.
        //
        // ONE parse fills every candidate. A buffer that does not parse cleanly
        // has no safe import edit at all, which is exactly the `None` the
        // per-request probe answered, and the whole table then carries `None`.
        if let Some(parsed) = vilan_core::formatter::ParsedSource::parse(analyzed) {
            for candidate in &mut candidates {
                let module = &modules[candidate.module as usize];
                let path: Vec<&str> = module.path.iter().map(String::as_str).collect();
                candidate.edit =
                    parsed
                        .insert_import(&path, &candidate.name)
                        .map(|edit| CapturedImportEdit {
                            span: edit.span,
                            replacement: edit.replacement,
                        });
            }
        }
        AutoImportOrder {
            modules,
            candidates,
        }
    }

    /// The first `cap` candidates whose name is not already offered.
    ///
    /// This is the whole per-request cost of the arm: a walk that stops at the
    /// cap, so it reads about `cap` entries of a table that may hold thousands.
    fn take(&self, in_scope: &HashSet<&str>, cap: usize) -> Vec<&AutoImportCandidate> {
        self.candidates
            .iter()
            .filter(|candidate| !in_scope.contains(candidate.name.as_str()))
            .take(cap)
            .collect()
    }
}

/// A name's completion category, from the analyzed program alone — the one
/// classification [`AutoImportOrder::build`] and [`Analysis::kind_of`] share,
/// so a candidate's icon cannot depend on which of the two produced it.
fn kind_of(program: &Program, id: Id) -> CompletionKind {
    if program.functions.contains_key(&id) || program.external_functions.contains_key(&id) {
        CompletionKind::Function
    } else if program.structs.contains_key(&id) {
        CompletionKind::Struct
    } else if program.enums.contains_key(&id) {
        CompletionKind::Enum
    } else if program.traits.contains_key(&id) {
        CompletionKind::Trait
    } else if program.modules.contains_key(&id) {
        CompletionKind::Module
    } else {
        CompletionKind::Variable
    }
}
