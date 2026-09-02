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

use vilan_core::analyzer::{Expr, Implementation, SourceId};
use vilan_core::formatter::{STYLE_CONDITION_METHODS, STYLE_PROPERTY_METHODS};
use vilan_core::id::Id;
use vilan_core::lexing::tokenize;
use vilan_core::token::Token;
use vilan_core::type_::{Type, TypeId};
use vilan_core::{PackageSpec, Platform as BuildPlatform, Span};

use crate::analysis::{
    Analysis, binding_type_id, call_parameter_names, nominal_type_id, signature_label,
};

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
    /// The signature (functions/methods) or type (variables) shown in the
    /// completion popup's detail line — the same house rendering hover uses.
    /// `None` for keywords, macros, modules, types, and fields (WO-3: a field's
    /// type is not cheaply renderable from the analyzed `Program`).
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
        Token::Struct => "struct",
        Token::Trait => "trait",
        Token::Type => "type",
        Token::Use => "use",
        Token::With => "with",
        Token::Ident(_)
        | Token::Ctrl(_)
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
    best: &mut Option<(usize, usize)>,
) {
    use vilan_core::node::Node;
    let span = node.1.into_range();
    if span.start <= offset && offset <= span.end {
        let tag_end = match &node.0 {
            Node::Element(body) => Some(body.tag.end),
            Node::Error => error_tag_name_end(source, span.start, span.end),
            _ => None,
        };
        if let Some(tag_end) = tag_end
            && tag_end <= offset
            && best.is_none_or(|(width, _)| span.end - span.start <= width)
        {
            *best = Some((span.end - span.start, tag_end));
        }
    }
    node.0
        .for_each_child(&mut |child| innermost_open_tag_end(child, offset, source, best));
}

/// The end of the tag name in an error node the element recovery produced —
/// `<` immediately followed by a name, the whole run closed by `>`. `None`
/// for any other error node, so a failed expression is never mistaken for
/// markup.
fn error_tag_name_end(source: &str, start: usize, end: usize) -> Option<usize> {
    let slice = source.get(start..end)?;
    if !slice.starts_with('<') || !slice.ends_with('>') {
        return None;
    }
    let name: usize = slice[1..]
        .bytes()
        .take_while(|byte| is_identifier_byte(*byte) || *byte == b'-')
        .count();
    (name > 0).then_some(start + 1 + name)
}

/// The candidates for one of §7.1's four positions in a `css` body.
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
fn css_block_completions(position: CssPosition) -> Vec<Completion> {
    match position {
        CssPosition::Property => {
            let mut seen: HashSet<&str> = HashSet::new();
            STYLE_PROPERTY_METHODS
                .iter()
                .flat_map(|method| method.properties.iter())
                .filter(|property| seen.insert(property))
                .map(|property| Completion::bare(property.to_string(), CompletionKind::Field))
                .collect()
        }
        CssPosition::Condition => STYLE_CONDITION_METHODS
            .iter()
            .map(|(condition, _)| Completion::bare(condition.to_string(), CompletionKind::Method))
            .collect(),
        // Both v1 blanks (Q4), and blank rather than absent: falling through to
        // the enclosing scope is what an element head refuses for the same
        // reason — nothing in scope is a CSS value or a custom property.
        CssPosition::CustomProperty | CssPosition::Value => Vec::new(),
    }
}

/// The root of a raw parse, shared by the two sub-language worlds
/// [`Analysis::cursor_context`] classifies (an element head, a `css` body).
type RawRoot<'src> = vilan_core::Spanned<vilan_core::node::NodeList<'src>>;

/// Whether `offset` (LIVE space — see [`Analysis::completion`]) sits in an
/// element's OPENING TAG, where the desugar takes an attribute, an
/// `on:event(…)`, or a `.method(…)` chain link (element-syntax.md §2–4).
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
fn in_element_head(
    root: Option<&RawRoot<'_>>,
    text: &str,
    tokens: &[(Token<'_>, Span)],
    offset: usize,
) -> bool {
    let mut best: Option<(usize, usize)> = None;
    if let Some(root) = root {
        for item in &root.0 {
            innermost_open_tag_end(item, offset, text, &mut best);
        }
    }
    let Some((_, tag_end)) = best else {
        return false;
    };
    let mut depth = 0usize;
    for (token, span) in tokens {
        let range = span.into_range();
        if range.start < tag_end {
            continue;
        }
        if range.start >= offset {
            break;
        }
        match token {
            Token::Ctrl('(' | '[' | '{') => depth += 1,
            Token::Ctrl(')' | ']' | '}') => depth = depth.saturating_sub(1),
            // The head is already closed: the cursor is among the children.
            Token::Ctrl('>') if depth == 0 => return false,
            _ => {}
        }
    }
    depth == 0
}

/// Which of §7.1's four positions `offset` (LIVE space) sits in, or `None` when
/// the cursor is not in a `css` body at all.
///
/// Two questions, in the element head's own order. [`innermost_css_body_start`]
/// answers *which body* from the raw parse; the token walk below answers *where
/// in it*, at the body's OWN brace depth — a hole (`{…}`) and a condition head's
/// arguments are ordinary expression ground, and both are bracket-deep, so the
/// depth clause declines them exactly as the head's does.
///
/// Within one item the two markers are the grammar's own: the leading `.` is the
/// whole declaration/combinator disambiguator (§3), and the `:` separates the
/// property from its value. A `;` starts the next item — and so does a nested
/// rule's closing `}`, which is why that one arm is guarded on `dotted`: a
/// HOLE's `}` closes no item, and reading it as one would put the rest of the
/// value in property position.
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
    let mut after_colon = false;
    for (token, span) in tokens {
        let range = span.into_range();
        if range.start < body_start {
            continue;
        }
        if range.start >= offset {
            break;
        }
        match token {
            Token::Ctrl('(' | '[' | '{') => depth += 1,
            // The body's own `}`: the cursor is past the block entirely.
            Token::Ctrl('}') if depth == 0 => return None,
            // A nested rule's `}` ends its item; a hole's does not.
            Token::Ctrl('}') if depth == 1 && dotted => {
                depth = 0;
                dotted = false;
                after_colon = false;
            }
            Token::Ctrl(')' | ']' | '}') => depth = depth.saturating_sub(1),
            Token::Ctrl(';') if depth == 0 => {
                dotted = false;
                after_colon = false;
            }
            // The declaration's separator is an OPERATOR token, not a control
            // one (`parse_css_declaration` reads it with `peek_is_op`).
            Token::Op(":") if depth == 0 => after_colon = true,
            Token::Ctrl('.') if depth == 0 && !after_colon => dotted = true,
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    if after_colon {
        return Some(CssPosition::Value);
    }
    if dotted {
        return Some(CssPosition::Condition);
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
    /// An item's head after the `.` that commits it to a condition combinator.
    Condition,
    /// After the `:` — the declaration's value. Empty in v1 (Q4): offering
    /// `flex` after `display:` needs a property->enum map that does not exist,
    /// and the enclosing scope is not an answer either (a value is source text,
    /// so a binding's NAME is what would land on the sheet).
    Value,
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
enum CursorContext {
    /// Not a code position at all — inside a string literal's body, or inside a
    /// `//` comment. Nothing is offered.
    NoCode,
    /// A macro name: `[Na…` at an item position, or inside `[derive(…)`.
    MacroName,
    /// Inside an element's opening tag (E67). `chain` is the `.` that commits
    /// the head item to the chain form rather than to an attribute.
    ElementHead { chain: bool },
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
fn member_context(tokens: &[(Token, Span)], start: usize) -> Option<CursorContext> {
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
        match context {
            // Text, not code (kolt.local 001): a name here is a caption or a
            // note, and every candidate would be wrong.
            CursorContext::NoCode => return Vec::new(),
            // Macro names are always bare, so they bypass the call-suppression
            // below.
            CursorContext::MacroName => return self.macro_name_completions(),
            CursorContext::ElementHead { chain } => return self.element_head_completions(chain),
            CursorContext::CssBlock(position) => return css_block_completions(position),
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
            } => self.lifted_member_completions(receiver_end),
            CursorContext::Member {
                receiver_end,
                lifted: false,
            } => self.member_completions(receiver_end),
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
        candidates
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
    fn cursor_context(
        &self,
        text: &str,
        tokens: &[(Token<'_>, Span)],
        offset: usize,
        start: usize,
    ) -> CursorContext {
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
        let raw = vilan_core::parsing::parse(text).0;
        // An element's opening tag is its own world (E67): between `<div` and
        // `>` the desugar takes an attribute, an `on:event(…)` or a `.method(…)`
        // chain link — and nothing that is merely in scope. The check runs from
        // `start` (the head item being typed), and the `.` just before it is the
        // same disambiguator the grammar uses.
        if in_element_head(raw.as_ref(), text, tokens, start) {
            return CursorContext::ElementHead {
                chain: start >= 1 && bytes[start - 1] == b'.',
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
        CursorContext::Expression
    }

    /// The candidates for an element's head (E67). `chain` says the cursor
    /// follows a `.`, so the head item under construction is a chain link.
    ///
    /// Both halves come from the compiler's own knowledge, so neither can
    /// drift: the chain form's vocabulary is the `View` type's method set,
    /// read from the std declaration the program compiles against, and the
    /// event form is a *grammar* form, not a name list. The undotted
    /// ATTRIBUTE vocabulary is deliberately absent — element-syntax.md §2 and
    /// §9 item 3 make the desugar name-blind (`name(x)` lowers to
    /// `.attr("name", x)` whatever `name` is), so there is no list to offer
    /// and inventing one here would be a second source of truth with nothing
    /// to gate it. What the head position stops offering is the enclosing
    /// scope: not one binding, type, keyword or construct snippet may appear
    /// between `<div` and `>`.
    fn element_head_completions(&self, chain: bool) -> Vec<Completion> {
        let Some(view_id) = self.element_view_nominal_id() else {
            return Vec::new();
        };
        let mut items = Vec::new();
        self.push_methods(view_id, true, &mut items);
        if !chain {
            // Undotted: the chain form is offered in its own spelling, dot
            // included, because an undotted `text(…)` is an ATTRIBUTE named
            // "text" — a different construct, and the one §4's warning exists
            // to catch.
            for item in &mut items {
                item.label = format!(".{}", item.label);
            }
            items.push(Completion::snippet(
                "on:",
                "an event handler",
                "on:${1:click}(|${2:event}| { $0 })",
                "on:",
            ));
        }
        items
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
    fn member_completions(&self, receiver_end: usize) -> Vec<Completion> {
        let Some(type_id) = self.receiver_nominal_id(receiver_end) else {
            return Vec::new();
        };
        self.nominal_member_completions(type_id)
    }

    /// The fields + methods of one nominal type — the member-completion list.
    fn nominal_member_completions(&self, type_id: Id) -> Vec<Completion> {
        let program = self.program;
        let mut items = Vec::new();
        if let Some(structure) = program.structs.get(&type_id) {
            for field in &structure.fields {
                items.push(Completion::bare(
                    field.name.to_string(),
                    CompletionKind::Field,
                ));
            }
        }
        self.push_methods(type_id, true, &mut items);
        items
    }

    /// Members of the ELEMENT under a lifted chain (`a?.` on an
    /// `Option<Profile>` offers Profile's members): the receiver ends at
    /// `receiver_end` (LIVE space, as [`CursorContext::Member`] found it) and
    /// its container's first type argument is the element.
    fn lifted_member_completions(&self, receiver_end: usize) -> Vec<Completion> {
        let program = self.program;
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
        receiver_end
            .checked_sub(1)
            .map(|offset| self.to_analyzed_offset(offset))
            .and_then(|offset| self.entity_at(offset))
            .and_then(|receiver| {
                self.expression_element_nominal_id(receiver).or_else(|| {
                    self.hover_label(receiver)
                        .and_then(|label| first_generic_argument(&label).map(str::to_string))
                        .and_then(|element| self.nominal_id_by_name(base_type_name(&element)))
                })
            })
            .map(|type_id| self.nominal_member_completions(type_id))
            .unwrap_or_default()
    }

    /// The nominal struct/enum id of the receiver value ending at
    /// `receiver_end` — one past its last byte, LIVE space (see
    /// [`CursorContext::Member`]).
    fn receiver_nominal_id(&self, receiver_end: usize) -> Option<Id> {
        // A bare name (`p.`): resolve through scope, or — when the cursor's own
        // statement failed to parse and dropped its local scope — the nearest
        // same-file binding of that name, then read its declared type. Robust while
        // the buffer is mid-edit, which is exactly when completion fires. The
        // NAME comes off the live text (`receiver_end` is where it ends), but
        // resolving that name against a scope is a `program` lookup, so it
        // converts to ANALYZED space first (E52).
        if let Some(name) = identifier_ending_at(self.live.text(), receiver_end) {
            let analyzed_offset = self.to_analyzed_offset(receiver_end);
            let binding = self
                .binding_in_scope(name, analyzed_offset)
                .or_else(|| self.same_file_variable(name, analyzed_offset));
            if let Some(nominal) = binding.and_then(|id| self.binding_nominal_id(id)) {
                return Some(nominal);
            }
        }
        // A complex receiver (`foo().`, `a.b.`): the parsed entity's own value
        // type — another `program` lookup, so `entity_at` also takes the
        // ANALYZED offset (E52). The rendered label is the FALLBACK, not the
        // answer: it is hover's phrasing, and hover answers a constructor call
        // with the thing being constructed (`Some(1)` -> `enum Option`), which
        // is right here, but a plain call with the CALLEE's signature
        // (`make()` -> `fn make(): Point`), which never names a type at all.
        receiver_end
            .checked_sub(1)
            .map(|offset| self.to_analyzed_offset(offset))
            .and_then(|offset| self.entity_at(offset))
            .and_then(|receiver| {
                self.expression_nominal_id(receiver).or_else(|| {
                    self.hover_label(receiver)
                        .and_then(|label| self.nominal_id_by_name(base_type_name(&label)))
                })
            })
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
    /// members on the nominal head, and that head is written in the declaration.
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
                return Some(return_type_id);
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
            Type::Closure(_, return_type_id) => Some(*return_type_id),
            _ => None,
        }
    }

    /// The nominal struct/enum id a `let`/parameter binding's declared type names.
    fn binding_nominal_id(&self, binding: Id) -> Option<Id> {
        let program = self.program;
        nominal_type_id(program, binding_type_id(program, binding)?)
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
        let Some((module_roots, surface)) = roots.origin_roots(origin, program.platform) else {
            // Not an origin — a same-file `mod`, or a namespace already in the
            // program under some other name. The last segment is the namespace
            // being descended into, which is all the by-name lookup reads.
            return self.namespace_completions_by_name(rest.last().unwrap_or(origin));
        };
        match rest.split_first() {
            None => origin_member_completions(&module_roots, surface.as_deref()),
            Some((module, past_module)) => {
                module_member_completions(&module_roots, module, past_module)
            }
        }
    }

    /// Items reachable through `left::` in CODE — an enum's variants and
    /// statics, a struct's statics, or a module's members — where `left` is the
    /// identifier ending just before the `::` at `colon_offset`.
    ///
    /// `left` is resolved THROUGH SCOPE (E53). Matching it against every loaded
    /// module's declarations by name, which is what this did, offered whatever
    /// any module in the process happened to declare — and nine std modules are
    /// ALWAYS loaded for the derive prelude, so `Json::` completed `parse`,
    /// `stringify`, and friends in a file that never imported `std::json`. An
    /// import path is the opposite case, where reaching what is not in scope is
    /// the entire point; it is served by [`Self::import_completions`], which
    /// keeps the by-name lookup ([`Self::namespace_completions_by_name`]).
    fn code_path_completions(&self, text: &str, colon_offset: usize) -> Vec<Completion> {
        let Some(left) = identifier_ending_at(text, colon_offset) else {
            return Vec::new();
        };
        let analyzed_offset = self.to_analyzed_offset(colon_offset);
        let Some(namespace) = self.namespace_in_scope(left, analyzed_offset) else {
            return Vec::new();
        };
        self.namespace_completions(namespace)
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
                items.push(Completion::bare(
                    variant.name.to_string(),
                    CompletionKind::EnumVariant,
                ));
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
    fn auto_import_completions(&self, in_scope: &HashSet<&str>) -> Vec<Completion> {
        let program = self.program;
        let mut candidates: Vec<(u8, String, CompletionKind, Vec<String>)> = Vec::new();
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
                let child_source = program.source_of(child_id);
                for (&name, &entity_id) in &child_scope.name_to_id_map {
                    if in_scope.contains(name) {
                        continue;
                    }
                    if program.source_of(entity_id) != child_source {
                        continue;
                    }
                    let kind = self.kind_of(entity_id);
                    if matches!(
                        kind,
                        CompletionKind::Module | CompletionKind::Keyword | CompletionKind::Snippet
                    ) {
                        continue;
                    }
                    candidates.push((
                        tier,
                        name.to_string(),
                        kind,
                        vec![root.to_string(), child_module.name.to_string()],
                    ));
                }
            }
        }
        candidates.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        candidates.truncate(AUTO_IMPORT_COMPLETION_CAP);
        if candidates.is_empty() {
            return Vec::new();
        }
        // ONE parse of the live buffer, shared by every candidate's edit
        // (E83): the string-input `insert_import` re-parses per call, which
        // made a bare scope position cost ~20 member completions — the parse
        // was the whole bill (playground-completion.md §9). When the buffer
        // does not parse cleanly there is no candidate to offer at all: no
        // import edit would be safe, which is exactly the `None` each
        // per-candidate call used to answer.
        let source = self.live.text();
        let Some(parsed) = vilan_core::formatter::ParsedSource::parse(source) else {
            return Vec::new();
        };
        candidates
            .into_iter()
            .filter_map(|(tier, name, kind, module_path)| {
                let path_refs: Vec<&str> = module_path.iter().map(String::as_str).collect();
                let edit = parsed.insert_import(&path_refs, &name)?;
                Some(Completion {
                    label: name,
                    kind,
                    detail: None,
                    documentation: None,
                    call_parameters: None,
                    snippet: None,
                    needs_import: Some(AutoImport {
                        module_path,
                        edit_span: edit.span,
                        edit_replacement: edit.replacement,
                        origin_tier: tier,
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

    /// The scope of the entity at — or nearest before — `analyzed_offset`
    /// (ANALYZED space), so the current function's locals are in scope even
    /// when the cursor sits in fresh text.
    fn scope_at(&self, analyzed_offset: usize) -> Option<Id> {
        let program = self.program;
        let entity = self.entity_at(analyzed_offset).or_else(|| {
            self.entity_spans
                .iter()
                .filter(|(_, end, _)| *end <= analyzed_offset)
                .max_by_key(|(_, end, _)| *end)
                .map(|(_, _, id)| *id)
        })?;
        program.entity_scope_map.get(&entity).copied()
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
    /// Two sources, in precedence order: the members the type's impl blocks
    /// DECLARE, then the default-bodied trait methods those impls INHERIT
    /// (kolt.local 033). Reading only the first left every default invisible on
    /// every implementing type — `list.iter().` offered `next` and nothing
    /// else, because `impl ListIterator<type T> with Iterator<T>` declares
    /// exactly that one member and `trait Iterator<T>`'s other fourteen are
    /// defaults. One name is offered ONCE: a declaration wins its name outright,
    /// which is what makes an impl that overrides a default offer the override
    /// rather than both.
    fn push_methods(&self, type_id: Id, want_self: bool, items: &mut Vec<Completion>) {
        let program = self.program;
        let mut offered: HashSet<&str> = HashSet::new();
        for implementation in &program.implementations {
            if self.impl_subject_id(implementation) != Some(type_id) {
                continue;
            }
            for (name, member_id) in &implementation.declarations {
                if self.is_self_method(*member_id) == want_self && offered.insert(name) {
                    items.push(self.entity_completion(
                        name.to_string(),
                        *member_id,
                        CompletionKind::Method,
                    ));
                }
            }
        }
        if want_self {
            self.push_inherited_defaults(type_id, &mut offered, items);
        }
    }

    /// Appends the trait defaults `type_id` inherits: for every impl of the
    /// type, every default-bodied INSTANCE method its traits (and their
    /// supertraits) declare and the impls themselves do not — `offered`
    /// carrying the names already spoken for.
    ///
    /// The admission rule is the analyzer's own
    /// (`Analyzer::inherited_default_candidates`), so the popup and the call
    /// site agree on what the concrete type provides: a member with no default
    /// body is never inherited (conformance forces the impl to declare it, and
    /// the pass above already found it there), a `[trait_only]` default stays
    /// off the concrete surface (`proposal/transport-rpc.md` §3.2), and a
    /// static has no inherited path onto a value at all.
    fn push_inherited_defaults(
        &self,
        type_id: Id,
        offered: &mut HashSet<&'src str>,
        items: &mut Vec<Completion>,
    ) {
        let program = self.program;
        for implementation in &program.implementations {
            if self.impl_subject_id(implementation) != Some(type_id) {
                continue;
            }
            for trait_id in &implementation.trait_ids {
                for home_id in self.trait_with_supertraits(*trait_id) {
                    let Some(home) = program.traits.get(&home_id) else {
                        continue;
                    };
                    for (name, member_id) in &home.declarations {
                        if self.member_has_default_body(*member_id)
                            && !self.declaration_is_trait_only(*member_id)
                            && self.is_self_method(*member_id)
                            && offered.insert(name)
                        {
                            items.push(self.entity_completion(
                                name.to_string(),
                                *member_id,
                                CompletionKind::Method,
                            ));
                        }
                    }
                }
            }
        }
    }

    /// `trait_id` plus its transitive supertraits — a trait's full interface
    /// includes everything its supertraits declare (`trait Ord with Eq +
    /// PartialOrd` reaches `PartialOrd`'s `lt`/`le`/`gt`/`ge`).
    fn trait_with_supertraits(&self, trait_id: Id) -> Vec<Id> {
        let program = self.program;
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
                if let Some(Type::Trait(super_id, _)) = program.type_id_to_type_map.get(supertrait)
                {
                    pending.push(*super_id);
                }
            }
        }
        result
    }

    /// Whether a trait member has a source-provided body — a DEFAULT method,
    /// which an impl of the trait may inherit rather than supply itself.
    fn member_has_default_body(&self, member_id: Id) -> bool {
        let program = self.program;
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
    fn declaration_is_trait_only(&self, member_id: Id) -> bool {
        let program = self.program;
        match program.entity_map.get(&member_id) {
            Some(Expr::Function(function_id)) => program
                .functions
                .get(function_id)
                .is_some_and(|function| function.trait_only),
            _ => false,
        }
    }

    /// Whether a method's first parameter is `self` — i.e. it is called on a value
    /// (`v.method()`) rather than on the type (`Type::method()`).
    fn is_self_method(&self, member_id: Id) -> bool {
        let program = self.program;
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

    /// The nominal struct/enum id an impl's subject names, ignoring type arguments.
    fn impl_subject_id(&self, implementation: &Implementation) -> Option<Id> {
        let program = self.program;
        nominal_type_id(program, implementation.subject)
    }

    /// The struct or enum named `name` (type arguments already stripped).
    fn nominal_id_by_name(&self, name: &str) -> Option<Id> {
        let program = self.program;
        program
            .structs
            .iter()
            .find(|(_, structure)| structure.name == name)
            .map(|(id, _)| *id)
            .or_else(|| {
                program
                    .enums
                    .iter()
                    .find(|(_, enumeration)| enumeration.name == name)
                    .map(|(id, _)| *id)
            })
    }

    /// The completion category for a name bound in a scope.
    fn kind_of(&self, id: Id) -> CompletionKind {
        let program = self.program;
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
                }
            }
            CompletionKind::Variable => {
                completion.detail = self.hover_label(id);
            }
            _ => {}
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

/// What `origin::` offers: every module under the origin's source roots, in the
/// loader's own root order (an earlier root shadows a later one, so a matching
/// platform layer wins over the base), followed by the names its `lib.vl`
/// surface publishes.
fn origin_member_completions(module_roots: &[&Path], surface: Option<&Path>) -> Vec<Completion> {
    let mut items = Vec::new();
    let mut seen: HashSet<String> = HashSet::default();
    for root in module_roots {
        for (name, _path) in vilan_core::analyzer::modules_in_root(root) {
            // `lib.vl` is the package's SURFACE, integrated into the package
            // name itself — its members are offered right here, one loop down,
            // and `import std::lib` is not how anyone reaches them.
            if name == "lib" || !seen.insert(name.clone()) {
                continue;
            }
            items.push(Completion::bare(name, CompletionKind::Module));
        }
    }
    for importable in surface
        .map(vilan_core::analyzer::module_importables)
        .unwrap_or_default()
    {
        if seen.insert(importable.name.to_string()) {
            items.push(importable_completion(&importable));
        }
    }
    items
}

/// What `origin::module::` offers: the module's own importable names, read on
/// demand from its source file. `past_module` are the segments beyond it — an
/// enum name descends into that enum's variants, which is the only descent
/// `resolve_import` makes past a module; anything deeper offers nothing.
fn module_member_completions(
    module_roots: &[&Path],
    module: &str,
    past_module: &[&str],
) -> Vec<Completion> {
    let Some(path) = vilan_core::analyzer::module_source_file(module_roots, module) else {
        return Vec::new();
    };
    let importables = vilan_core::analyzer::module_importables(&path);
    let Some((name, past_enum)) = past_module.split_first() else {
        return importables.iter().map(importable_completion).collect();
    };
    if !past_enum.is_empty() {
        return Vec::new();
    }
    importables
        .iter()
        .find(|importable| {
            importable.name == *name
                && importable.kind == vilan_core::analyzer::ImportableKind::Enum
        })
        .map(|enumeration| {
            enumeration
                .variants
                .iter()
                .map(|variant| Completion::bare(variant.to_string(), CompletionKind::EnumVariant))
                .collect()
        })
        .unwrap_or_default()
}

/// One importable name as a completion candidate. Bare by construction — an
/// import binds a name, it never calls it — so the shaping post-pass in
/// [`Analysis::completion`] has nothing left to strip.
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
