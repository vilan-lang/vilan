//! The keystroke path (E121, `proposal/editor-latency.md` §2.1): semantic
//! tokens, inlay hints and completion answered in **O(file)** from the last
//! LANDED analysis re-mapped onto the live buffer — and never by
//! type-checking.
//!
//! The paper's measurement is the reason this module exists. Every provider
//! today re-walks the whole analyzed program per request, so one unchanged
//! 468-token file costs 4.2 ms of CPU to tokenize inside a 4-function import
//! closure and 27.0 ms inside a 5,000-function one (§1.4a): the cost is
//! proportional to the *codebase*, not to the file, and the mandate's 10 ms
//! budget is spent at ≈490 reachable functions. Nothing about that is a
//! tuning problem. The fix is to stop recomputing.
//!
//! Three pieces, each with its own honesty argument:
//!
//! 1. **The two-sided anchor** ([`Anchor`]) — the generalization of B38's
//!    `compute_retained_tail`. Everything before the first differing byte and
//!    after the last differing byte is *byte-identical* between the analyzed
//!    text and the live buffer, so the landed answers for those regions map
//!    onto the live buffer by a constant shift and are position-exact. The
//!    middle window — the region the user is editing — has no image and is
//!    served from **syntax alone**.
//! 2. **The declaration-shape stamp** ([`ShapeStamp`]) — a hash of everything
//!    the token stream shows OUTSIDE a function body, which is exactly the
//!    text that binds a module-scope name. An unchanged stamp means no name's
//!    resolution can have moved, so the landed classification outside the
//!    window is still true; a changed one degrades the file's tokens to
//!    syntax-only until the next analysis lands.
//! 3. **The per-module symbol index** ([`SymbolIndex`]) — declared names,
//!    kinds and signature labels, grouped by the module that declares them,
//!    built once when an analysis lands and refreshed for the edited module
//!    from its own syntax, together with the `Program`-side half
//!    ([`vilan_ide::CompletionIndex`]: the auto-import candidate table and the
//!    origins' module listings, M25). Completion reads it instead of sweeping
//!    every module's name map and instead of `read_dir`.
//!
//! The three combine into three verdicts ([`Verdict`]), which are the honest
//! vocabulary for what an answer is worth mid-keystroke. `Document`'s
//! `keystroke_*` methods are the consumers.

use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vilan_core::Span;
use vilan_core::lexing::tokenize;
use vilan_core::token::Token;
use vilan_ide::{Completion, CompletionIndex, CompletionKind};

use crate::document::{MODIFIER_DECLARATION, MODIFIER_READONLY, TokenKind};
use crate::line_index::LineIndex;

// ---------------------------------------------------------------------------
// 1. The two-sided anchor
// ---------------------------------------------------------------------------

/// The byte-identical regions shared by the ANALYZED text and the LIVE buffer:
/// a common prefix, a common suffix, and the window between them that has no
/// image in the landed analysis.
///
/// B38 (`Document::compute_retained_tail`) already built half of this and
/// stated the honesty argument that governs the whole of it: *"Identity of
/// BYTES is the whole honesty argument: the suffix is literally the same text,
/// so positions are exact."* The generalization is that a keystroke has text
/// on BOTH sides of it, and the half B38 threw away — everything before the
/// edit — is the larger half in every file longer than the cursor's line.
///
/// Both edges are trimmed to a **line boundary**: the prefix back to the start
/// of the line holding the first difference, the suffix forward to the start
/// of the line after the last one. A line boundary is a token boundary (a
/// `\n` byte cannot occur inside a UTF-8 sequence, and the lexer's only
/// newline-spanning token is a multiline string, which the containment rules
/// below drop rather than mis-place), so the two anchors never cut a token in
/// half.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Anchor {
    /// Bytes of byte-identical, line-aligned common prefix. The same offset in
    /// both texts.
    pub prefix: usize,
    /// Bytes of byte-identical, line-aligned common suffix.
    pub suffix: usize,
    /// The analyzed text's length, so an analyzed offset can be classified.
    pub analyzed_len: usize,
    /// The live buffer's length, so a live offset can be classified.
    pub live_len: usize,
}

impl Anchor {
    /// The anchor between the text an analysis ran on and the text the user is
    /// looking at.
    ///
    /// Identical texts are the common case (every request between two
    /// keystrokes) and answer with the whole file as prefix and an empty
    /// window — a full re-serve of the landed answer at shift zero.
    pub fn compute(analyzed: &str, live: &str) -> Anchor {
        let analyzed_bytes = analyzed.as_bytes();
        let live_bytes = live.as_bytes();
        if analyzed_bytes == live_bytes {
            return Anchor {
                prefix: analyzed_bytes.len(),
                suffix: 0,
                analyzed_len: analyzed_bytes.len(),
                live_len: live_bytes.len(),
            };
        }
        let common_prefix = analyzed_bytes
            .iter()
            .zip(live_bytes)
            .take_while(|(old, new)| old == new)
            .count();
        // Back to the start of the line holding the first difference. The
        // result is 0 or one past a `\n`, so it is a char boundary in both
        // texts (their bytes agree up to `common_prefix`).
        let prefix = line_start(analyzed_bytes, common_prefix);

        let common_suffix = analyzed_bytes
            .iter()
            .rev()
            .zip(live_bytes.iter().rev())
            .take_while(|(old, new)| old == new)
            .count();
        // Clamp before trimming so the two anchors cannot overlap in EITHER
        // text — `"aa"` → `"a"` shares a one-byte prefix and a one-byte
        // suffix that are the same byte.
        let room = analyzed_bytes
            .len()
            .min(live_bytes.len())
            .saturating_sub(prefix);
        let common_suffix = common_suffix.min(room);
        // Forward to the start of the line after the last difference, in LIVE
        // coordinates; the same trim in analyzed coordinates follows from the
        // byte identity.
        let live_suffix_start = next_line_start(live_bytes, live_bytes.len() - common_suffix);
        let suffix = live_bytes.len() - live_suffix_start;

        Anchor {
            prefix,
            suffix,
            analyzed_len: analyzed_bytes.len(),
            live_len: live_bytes.len(),
        }
    }

    /// How far the tail anchor moved: the live buffer's length minus the
    /// analyzed text's. A landed token inside the tail is *this many bytes*
    /// later (or earlier) in the live buffer, exactly, by byte identity.
    pub fn shift(&self) -> i64 {
        self.live_len as i64 - self.analyzed_len as i64
    }

    /// Whether the anchor holds nothing: a paste (or a first analysis) left no
    /// byte-identical line on either side, so no landed answer has an image.
    /// This is [`Verdict::Unusable`]'s condition.
    pub fn is_empty(&self) -> bool {
        self.prefix == 0 && self.suffix == 0
    }

    /// The edit window in LIVE coordinates — the region served from syntax
    /// alone. Empty when the texts are identical.
    pub fn live_window(&self) -> Range<usize> {
        let end = self.live_len.saturating_sub(self.suffix);
        self.prefix.min(end)..end
    }

    /// Map an ANALYZED span into live coordinates, or `None` when it has no
    /// image.
    ///
    /// A span maps only when it lies **entirely** inside one anchor: a span
    /// that straddles a window edge is dropped rather than clamped, because
    /// half of it describes bytes that are gone. That is what keeps the
    /// mechanism a re-mapping instead of a guess.
    pub fn map_span(&self, span: Span) -> Option<Span> {
        if span.end <= self.prefix {
            return Some(span);
        }
        if span.start >= self.analyzed_len.saturating_sub(self.suffix) {
            let shift = self.shift();
            let start = span.start as i64 + shift;
            let end = span.end as i64 + shift;
            if start < 0 || end < start || end as usize > self.live_len {
                return None;
            }
            return Some(Span {
                start: start as usize,
                end: end as usize,
            });
        }
        None
    }

    /// Map an ANALYZED byte offset into live coordinates, or `None` when it
    /// falls in the window. The point form of [`Anchor::map_span`], for the
    /// offset-keyed answers (inlay hints).
    pub fn map_offset(&self, offset: usize) -> Option<usize> {
        self.map_span(Span {
            start: offset,
            end: offset,
        })
        .map(|span| span.start)
    }
}

/// The start of the line containing `at` — `0`, or one past the nearest
/// preceding `\n`.
fn line_start(bytes: &[u8], at: usize) -> usize {
    let mut at = at.min(bytes.len());
    while at > 0 && bytes[at - 1] != b'\n' {
        at -= 1;
    }
    at
}

/// The start of the line AFTER the one containing `at` — the length when `at`
/// is on the last line.
fn next_line_start(bytes: &[u8], at: usize) -> usize {
    let mut at = at.min(bytes.len());
    if at > 0 && bytes[at - 1] == b'\n' {
        return at;
    }
    while at < bytes.len() {
        let byte = bytes[at];
        at += 1;
        if byte == b'\n' {
            return at;
        }
    }
    bytes.len()
}

// ---------------------------------------------------------------------------
// 2. The declaration-shape stamp
// ---------------------------------------------------------------------------

/// A hash of a module's **declaration shape**: every token the lexer produces
/// outside a function body.
///
/// The rule is one sentence and it is the whole argument: *a module's
/// name bindings live outside its function bodies*. `import` and `use` lines,
/// `fun` signatures, `struct`/`enum`/`trait`/`impl` headers and their member
/// declarations, module-scope `let`s, `mod`s, `macro`s and `type` aliases are
/// all hashed token by token; a function body contributes one fixed marker and
/// nothing else. So an equal stamp means the paper's §2.1.2 cases 1–3 — a
/// top-level declaration added, removed or renamed; an `import` line changed;
/// a signature or annotation changed — did not happen, and the landed
/// analysis's classification of every name outside the edit window is still
/// what the analyzer would say. Case 4 (another module moved) is a different
/// question and `Document::depends_on` against the world revision answers it.
///
/// Two properties make it cheap enough for the keystroke path: it is computed
/// off the **lexer**, not the parser, so it costs one linear pass and survives
/// a mid-keystroke syntax error that would fail a parse; and every ambiguity
/// resolves toward *changed*, which is the safe direction (a false "stale"
/// costs a syntax-only paint that is never wrong, a false "exact" would keep a
/// lie on screen).
///
/// What it deliberately does NOT cover: a `let` added inside a function body
/// can shadow a later use of the same name in the same body. Both live in that
/// body; the edit is in the window and the shadowed use is in the tail anchor,
/// so it can be mis-coloured for one analysis. Q1 rules exactly this
/// acceptable for tokens ("a briefly mis-coloured identifier is cosmetic"),
/// and hints are withheld in the window where the risk is concentrated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ShapeStamp(u64);

/// The body marker: every function body hashes to this and nothing else, so
/// typing inside one leaves the stamp alone.
const BODY_MARKER: u64 = 0x5641_4c41_4e5f_424f;

/// The declaration-shape stamp of `text`.
pub fn shape_stamp(text: &str) -> ShapeStamp {
    let (tokens, errors) = tokenize(text);
    let mut hasher = vilan_core::fx::FxHasher::default();
    // A lex error is a shape change by itself: the token stream below it is
    // not trustworthy, and the safe direction is "changed".
    errors.len().hash(&mut hasher);
    let mut index = 0usize;
    while index < tokens.len() {
        let (token, _span) = &tokens[index];
        hash_token(token, &mut hasher);
        if matches!(token, Token::Fun) {
            index = hash_signature_and_skip_body(&tokens, index + 1, &mut hasher);
            continue;
        }
        index += 1;
    }
    ShapeStamp(hasher.finish())
}

/// Hash the tokens of a `fun` signature starting at `index`, then skip its
/// body (hashing [`BODY_MARKER`] in its place) and return the index just past
/// it. A body-less `fun` — a trait requirement, an `external` declaration —
/// ends at its `;` or at the next declaration and contributes no marker.
fn hash_signature_and_skip_body(
    tokens: &[(Token<'_>, Span)],
    mut index: usize,
    hasher: &mut vilan_core::fx::FxHasher,
) -> usize {
    let mut group_depth = 0i32;
    while index < tokens.len() {
        let (token, _) = &tokens[index];
        match token {
            Token::Ctrl('(' | '[') => group_depth += 1,
            Token::Ctrl(')' | ']') => group_depth -= 1,
            Token::Ctrl('{') if group_depth <= 0 => {
                BODY_MARKER.hash(hasher);
                return skip_braced(tokens, index);
            }
            Token::Ctrl(';') if group_depth <= 0 => {
                hash_token(token, hasher);
                return index + 1;
            }
            _ => {}
        }
        hash_token(token, hasher);
        index += 1;
    }
    index
}

/// The index just past the `{ … }` group opening at `open` (which must be a
/// `Ctrl('{')`), or the end of the stream when it never closes.
fn skip_braced(tokens: &[(Token<'_>, Span)], open: usize) -> usize {
    let mut depth = 0i32;
    let mut index = open;
    while index < tokens.len() {
        match &tokens[index].0 {
            Token::Ctrl('{') => depth += 1,
            Token::Ctrl('}') => {
                depth -= 1;
                if depth == 0 {
                    return index + 1;
                }
            }
            _ => {}
        }
        index += 1;
    }
    index
}

/// Hash one token by its shape: the discriminant, plus the text of the
/// variants that carry one. Spans are deliberately excluded — moving a
/// declaration down a line does not change what it binds.
fn hash_token(token: &Token<'_>, hasher: &mut vilan_core::fx::FxHasher) {
    std::mem::discriminant(token).hash(hasher);
    match token {
        Token::Ident(text) | Token::Op(text) | Token::String(text) => text.hash(hasher),
        Token::MultilineString(text) => text.hash(hasher),
        Token::Ctrl(character) => character.hash(hasher),
        Token::Bool(value) => value.hash(hasher),
        Token::Number(whole, fraction, suffix) => (whole, fraction, suffix).hash(hasher),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// 3. The three verdicts
// ---------------------------------------------------------------------------

/// What the landed analysis is worth for the buffer as it stands — the
/// vocabulary the paper's §2.1.2 table is written in, and the one decision
/// every `keystroke_*` provider branches on.
///
/// | verdict | condition | tokens | hints | completion |
/// |---|---|---|---|---|
/// | [`Verdict::Exact`] | an anchor exists, the stamp matches, no dependency moved | landed tokens re-mapped through the anchor, **syntax-only inside the window** | landed hints re-mapped, **withheld inside the window** | index + the landed scope, members from the landed type |
/// | [`Verdict::Stale`] | an anchor exists, but the stamp changed or a dependency moved | **syntax-only, whole file** | landed hints re-mapped, withheld inside the window — served unchanged rather than flickered off | index + the module's own names |
/// | [`Verdict::Unusable`] | no anchor at all: a paste replaced the file, or nothing has landed | **syntax-only, whole file** | **withheld entirely** | index only |
///
/// The asymmetry between tokens and hints in the stale row is Q1's ruling and
/// is not an oversight. A token has a syntax-only fallback that is *never
/// wrong*, so degrading to it costs nothing; a hint has none — there is
/// nothing to show — and VS Code's only available mark is removal, so
/// withholding a stale hint makes hints strobe across a typing burst. A hint
/// one analysis old is a smaller harm than a display that blinks, and
/// `inlayHint/refresh` already corrects it when the analysis lands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Positions are exact AND no name's resolution can have moved.
    Exact,
    /// Positions are exact, but a name binding changed: the landed
    /// classification may be a lie, so tokens fall back to syntax.
    Stale,
    /// Nothing of the landed answer survives: no byte-identical anchor.
    Unusable,
}

impl Verdict {
    /// The verdict for one buffer against one landed snapshot.
    ///
    /// `dependency_moved` is the caller's answer to the paper's case 4 —
    /// another module this file's analysis loaded has been edited — which no
    /// amount of local anchoring can repair.
    pub fn decide(anchor: &Anchor, stamp_matches: bool, dependency_moved: bool) -> Verdict {
        if anchor.is_empty() {
            return Verdict::Unusable;
        }
        if !stamp_matches || dependency_moved {
            return Verdict::Stale;
        }
        Verdict::Exact
    }
}

// ---------------------------------------------------------------------------
// 4. Syntax-only classification
// ---------------------------------------------------------------------------

/// The semantic tokens the LIVE buffer's own syntax determines, for the byte
/// range `window` — the answer served inside the edit window, and the whole
/// answer in the stale and unusable states.
///
/// **What it emits and what it does not** is the load-bearing half of Q5. The
/// LSP legend this server publishes (`document::TOKEN_TYPES`) carries only
/// *semantic* classes — namespace, struct, function, variable and their
/// siblings. It has no `comment`, `keyword`, `string`, `number` or `operator`,
/// because those are the client's TextMate grammar's job, and the grammar is
/// **never stale**: it re-highlights on the keystroke. So this function emits
/// a token only where syntax alone determines a *semantic* role, and stays
/// silent everywhere else.
///
/// That silence is exactly the owner's exhibit. `tokenize` produces no token
/// for a comment at all (`lexing.rs`'s own pin: `lex("// just a comment")` is
/// empty), so commenting a line out removes every semantic token that line
/// had. The client then paints it with the grammar's comment colour on the
/// very next keystroke instead of holding the line's function-and-variable
/// colours for the whole staleness window — which is what "the highlighting
/// lags" meant.
///
/// The rules, in priority order, and each decidable from the token stream:
///
/// - the identifier after `fun` / `struct` / `enum` / `trait` / `mod` /
///   `macro` is that kind's **declaration**;
/// - the identifier after `let` is a readonly variable declaration, after
///   `mut` a mutable one;
/// - `.name(` is a method, `.name` a property;
/// - `name(` is a function call, `name::` a namespace;
/// - every other identifier is a *plain* identifier and emits nothing —
///   whether it is a variable, a type or a constant is a resolution question,
///   and the anchors are where resolution answers come from.
pub fn syntax_tokens_in(text: &str, window: Range<usize>) -> Vec<(Span, TokenKind, u32)> {
    if window.is_empty() {
        return Vec::new();
    }
    let (tokens, _errors) = tokenize(text);
    let mut out: Vec<(Span, TokenKind, u32)> = Vec::new();
    for (index, (token, span)) in tokens.iter().enumerate() {
        let Token::Ident(_) = token else { continue };
        // Full containment, so a syntax token can never overlap an anchor
        // token: the two regions are disjoint by construction and the caller
        // needs no arbitration between them.
        if span.start < window.start || span.end > window.end {
            continue;
        }
        let previous = index.checked_sub(1).map(|before| &tokens[before].0);
        let next = tokens.get(index + 1).map(|(token, _)| token);
        let classified = match (previous, next) {
            (Some(Token::Fun), _) => Some((TokenKind::Function, MODIFIER_DECLARATION)),
            (Some(Token::Struct), _) => Some((TokenKind::Struct, MODIFIER_DECLARATION)),
            (Some(Token::Enum), _) => Some((TokenKind::Enum, MODIFIER_DECLARATION)),
            (Some(Token::Trait), _) => Some((TokenKind::Interface, MODIFIER_DECLARATION)),
            (Some(Token::Mod), _) => Some((TokenKind::Namespace, MODIFIER_DECLARATION)),
            (Some(Token::Macro), _) => Some((TokenKind::Macro, MODIFIER_DECLARATION)),
            (Some(Token::Let), _) => Some((
                TokenKind::Variable,
                MODIFIER_DECLARATION | MODIFIER_READONLY,
            )),
            (Some(Token::Mut), _) => Some((TokenKind::Variable, MODIFIER_DECLARATION)),
            (Some(Token::Ctrl('.')), Some(Token::Ctrl('('))) => Some((TokenKind::Method, 0)),
            (Some(Token::Ctrl('.')), _) => Some((TokenKind::Property, 0)),
            (_, Some(Token::Ctrl('('))) => Some((TokenKind::Function, 0)),
            (_, Some(Token::Op("::"))) => Some((TokenKind::Namespace, 0)),
            _ => None,
        };
        if let Some((kind, modifiers)) = classified {
            out.push((*span, kind, modifiers));
        }
    }
    out
}

/// Sort a token stream by position and drop overlaps, narrowest-first at each
/// start — the shape the LSP requires, and the same rule
/// `Document::semantic_tokens` applies to its own stream. Merging two sources
/// (re-mapped anchors and the window's syntax) is the one place that can
/// produce an out-of-order stream, so it is done in one named place.
pub fn sort_and_deoverlap(mut tokens: Vec<(Span, TokenKind, u32)>) -> Vec<(Span, TokenKind, u32)> {
    tokens.sort_by_key(|(span, _, _)| (span.start, span.end.saturating_sub(span.start)));
    let mut kept: Vec<(Span, TokenKind, u32)> = Vec::new();
    let mut last_end = 0usize;
    for (span, kind, modifiers) in tokens {
        if span.start >= last_end && span.start < span.end {
            last_end = span.end;
            kept.push((span, kind, modifiers));
        }
    }
    kept
}

// ---------------------------------------------------------------------------
// 5. The per-module symbol index
// ---------------------------------------------------------------------------

/// One name a module declares, with everything completion needs to offer it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolEntry {
    pub name: String,
    pub kind: CompletionKind,
    /// The rendered signature (a function's declaration line, a variable's
    /// type), where the source of this entry could produce one.
    pub signature: Option<String>,
    /// The parameter names a call-shaped insertion needs, `None` for a
    /// non-callable.
    pub call_parameters: Option<Vec<String>>,
    /// Which analysis filled this entry's resolution-derived fields. **Zero
    /// means purely syntactic** — read straight off a token stream, true of
    /// the buffer as it stands this instant. A consumer can therefore always
    /// tell a syntactic fact from a resolved one, which is the property the
    /// paper asks the index to carry.
    pub analysis_epoch: u64,
}

/// Every name one module declares, keyed by the module's own content.
#[derive(Clone, Debug, Default)]
pub struct ModuleSymbols {
    /// The module's canonical file path, when the analysis recorded one.
    pub path: Option<PathBuf>,
    /// The name a `path::` completion prefix spells this module with — the
    /// file stem, which is what a vilan module path segment is.
    pub module_name: String,
    /// The declaration-shape stamp of the text these entries were read from —
    /// `Some` only for a module whose text this process holds (the open
    /// buffer). An entry with a stamp is invalidated by exactly one thing: a
    /// change to that module's own declaration shape.
    pub stamp: Option<ShapeStamp>,
    pub entries: Vec<SymbolEntry>,
}

/// The keystroke path's completion source: declared names grouped by declaring
/// module.
///
/// It replaces three whole-program sweeps the paper measured
/// (`auto_import_completions`' per-module `name_to_id_map` walk, the scope
/// enumeration, and `modules_in_root`'s per-request `read_dir`) with a lookup
/// over a table whose size is the number of declarations, not the number of
/// entities. Nothing on the keystroke path touches the filesystem to read it.
#[derive(Clone, Debug, Default)]
pub struct SymbolIndex {
    pub by_module: Vec<ModuleSymbols>,
    /// The half of the index that lives over the analyzed `Program` rather
    /// than over a token stream (M25): the auto-import candidate table and the
    /// origins' module listings, derived once on the analysis thread by
    /// [`vilan_ide::CompletionIndex::build`].
    ///
    /// It is here, and not a second memo beside the snapshot, because it
    /// answers the same question `by_module` does — *which names exist, and
    /// under which module* — for the arm that reaches names this file has not
    /// imported. Held behind an `Arc` because a `SymbolIndex` is cloned with
    /// the document it belongs to and this table is the large part of it.
    pub completion: Arc<CompletionIndex>,
}

impl SymbolIndex {
    /// The module the open buffer is, by convention index 0 — the analysis's
    /// entry file.
    pub const ENTRY: usize = 0;

    /// The entries of the module spelled `name` in a `name::…` path.
    pub fn module(&self, name: &str) -> Option<&ModuleSymbols> {
        self.by_module
            .iter()
            .find(|module| module.module_name == name)
    }

    /// Replace the entry module's entries from the LIVE buffer's own syntax,
    /// but only when its declaration shape actually moved.
    ///
    /// This is the mandate's "invalidated only by that module's own edits",
    /// made exact: typing inside a function body leaves the stamp alone and
    /// costs one lex and one hash, and only a keystroke that adds, removes,
    /// renames or re-signs a declaration pays for a rebuild. It runs on the
    /// keystroke thread (`Document::set_text`) and is O(file).
    pub fn refresh_entry_from_syntax(&mut self, text: &str) -> bool {
        let stamp = shape_stamp(text);
        if let Some(entry) = self.by_module.get_mut(Self::ENTRY) {
            if entry.stamp == Some(stamp) {
                return false;
            }
            entry.stamp = Some(stamp);
            entry.entries = syntax_symbols(text);
            return true;
        }
        self.by_module.insert(
            Self::ENTRY,
            ModuleSymbols {
                path: None,
                module_name: String::new(),
                stamp: Some(stamp),
                entries: syntax_symbols(text),
            },
        );
        true
    }

    /// The declaration-shape stamp of the LIVE buffer, as the last refresh
    /// recorded it — maintained by [`SymbolIndex::refresh_entry_from_syntax`]
    /// on every edit, so a verdict reads it instead of hashing the buffer
    /// again. `None` only before the first refresh.
    pub fn entry_stamp(&self) -> Option<ShapeStamp> {
        self.by_module
            .get(Self::ENTRY)
            .and_then(|entry| entry.stamp)
    }
}

/// The names a module declares, read straight off its token stream — no parse,
/// no analysis, no filesystem.
///
/// The paper's property, restated: *a module's export list is determined by
/// its own syntax*. `fun`, `struct`, `enum`, `trait`, `mod`, `macro`, `type`
/// and a module-scope `let` all announce themselves with a keyword followed by
/// a name, at brace depth zero. That makes this function the honest answer for
/// the buffer as it stands **this instant** — a `fun` typed one keystroke ago
/// completes, where an index built from the last analysis cannot know it
/// exists.
pub fn syntax_symbols(text: &str) -> Vec<SymbolEntry> {
    let (tokens, _errors) = tokenize(text);
    let mut entries: Vec<SymbolEntry> = Vec::new();
    let mut depth = 0i32;
    let mut index = 0usize;
    while index < tokens.len() {
        match &tokens[index].0 {
            Token::Ctrl('{') => depth += 1,
            Token::Ctrl('}') => depth -= 1,
            keyword if depth == 0 => {
                let kind = match keyword {
                    Token::Fun => Some(CompletionKind::Function),
                    Token::Struct => Some(CompletionKind::Struct),
                    Token::Enum => Some(CompletionKind::Enum),
                    Token::Trait => Some(CompletionKind::Trait),
                    Token::Mod => Some(CompletionKind::Module),
                    Token::Macro => Some(CompletionKind::Macro),
                    Token::Let | Token::Mut => Some(CompletionKind::Variable),
                    _ => None,
                };
                if let Some(kind) = kind
                    && let Some((Token::Ident(name), _)) = tokens.get(index + 1)
                {
                    let call_parameters = (kind == CompletionKind::Function)
                        .then(|| parameter_names(&tokens, index + 2));
                    let signature = call_parameters
                        .as_ref()
                        .map(|parameters| format!("fun {name}({})", parameters.join(", ")));
                    entries.push(SymbolEntry {
                        name: (*name).to_string(),
                        kind,
                        signature,
                        call_parameters,
                        analysis_epoch: 0,
                    });
                }
            }
            _ => {}
        }
        index += 1;
    }
    entries
}

/// The parameter names of the `( … )` group at or after `index` — the first
/// identifier of each top-level comma group, which is a vilan parameter's
/// name. An empty list for a zero-parameter callable; also empty when the
/// group never opens (a mid-keystroke signature), which is the conservative
/// answer.
fn parameter_names(tokens: &[(Token<'_>, Span)], index: usize) -> Vec<String> {
    let Some((Token::Ctrl('('), _)) = tokens.get(index) else {
        return Vec::new();
    };
    let mut names: Vec<String> = Vec::new();
    let mut depth = 0i32;
    let mut expecting_name = true;
    for (token, _) in &tokens[index..] {
        match token {
            Token::Ctrl('(' | '[') => {
                depth += 1;
                if depth > 1 {
                    expecting_name = false;
                }
            }
            Token::Ctrl(')' | ']') => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            Token::Ctrl(',') if depth == 1 => expecting_name = true,
            Token::Ident(name) if depth == 1 && expecting_name => {
                names.push((*name).to_string());
                expecting_name = false;
            }
            _ => {}
        }
    }
    names
}

/// The module name a `path::` prefix spells a file with: its stem.
pub fn module_name_of(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// 5b. The landed snapshot
// ---------------------------------------------------------------------------

/// Everything the keystroke path is allowed to remember about the last landed
/// analysis, computed **once when that analysis is built** and never again.
///
/// This is the change that makes the budget reachable, and it is worth stating
/// plainly. The paper measured that `semantic_tokens()` re-walks
/// `program.functions`, `structs`, `enums`, `traits`, `variables`,
/// `parameters`, the whole `entity_map`, `member_name_spans` and
/// `type_references` **on every request** — which is why one unchanged
/// 468-token file costs 13.4 ms of CPU inside kolt's closure and 27.0 ms
/// inside a 5,000-function one. Capturing the answer at land time moves that
/// whole-program walk onto the analysis thread, where it happens once per
/// analysis instead of once per keystroke, and leaves the request with an
/// anchor, a shift and one lex of the edited window: **O(file)**, and flat in
/// the size of the codebase.
///
/// A snapshot with no tokens and a default stamp is the "nothing has landed"
/// state; every provider then answers from syntax alone.
#[derive(Clone, Debug, Default)]
pub struct LandedSnapshot {
    /// The declaration shape of the text this analysis ran on.
    pub stamp: ShapeStamp,
    /// The analysis's semantic tokens, in ANALYZED coordinates, ordered by
    /// start offset and non-overlapping (`Document::semantic_tokens`
    /// guarantees both, and B38's salvage tail appends strictly after them).
    pub tokens: Vec<(Span, TokenKind, u32)>,
    /// E122's viewport index over [`tokens`](Self::tokens): `token_lines[line]`
    /// is the position in `tokens` of the first token whose start line is at
    /// or after `line`. Non-decreasing, `tokens.len()` at the end, and one
    /// longer than the highest token line — so a line window is a clamp and
    /// two indexes, never a scan.
    ///
    /// It lives HERE, beside the tokens it indexes, rather than in a second
    /// memo of its own: the capture is already built once per analysis and
    /// invalidated in exactly one place (`Document::adopt_analysis`), and a
    /// parallel memo of the same stream would be a second thing to keep
    /// truthful. Empty on a snapshot nothing landed into, which
    /// [`tokens_in_lines`](Self::tokens_in_lines) reads as "no tokens".
    pub token_lines: Vec<u32>,
    /// The analysis's inlay hints, in ANALYZED coordinates.
    pub hints: Vec<(usize, String)>,
    /// The declared names of every module the analysis loaded.
    pub index: SymbolIndex,
    /// Whether an analysis produced this at all.
    pub landed: bool,
}

impl LandedSnapshot {
    /// Build [`token_lines`](Self::token_lines) over the tokens now held, in
    /// the `index` those tokens' offsets belong to (the ANALYZED one).
    ///
    /// Called once per analysis, from the two places the capture's tokens are
    /// set: `Document::capture_landed` on the analysis thread, and
    /// `Document::adopt_analysis` after B38's salvage tail is folded in. Start
    /// lines are non-decreasing because start offsets are strictly increasing,
    /// which is what lets a line window be a contiguous slice at all.
    pub fn index_token_lines(&mut self, index: &LineIndex) {
        let mut lines: Vec<u32> = Vec::with_capacity(self.tokens.len());
        for (span, ..) in &self.tokens {
            lines.push(index.range(span).start.line);
        }
        let highest = lines.last().copied().unwrap_or(0) as usize;
        let mut token_lines = vec![self.tokens.len() as u32; highest + 2];
        // Backwards, so a line with several tokens keeps the FIRST of them and
        // a line with none inherits the next line's start.
        for (position, line) in lines.iter().enumerate().rev() {
            token_lines[*line as usize] = position as u32;
        }
        for line in (0..token_lines.len() - 1).rev() {
            token_lines[line] = token_lines[line].min(token_lines[line + 1]);
        }
        self.token_lines = token_lines;
    }

    /// The positions in [`tokens`](Self::tokens) of the captured tokens whose
    /// start line lies in `first_line..=last_line` — the viewport slice
    /// `semanticTokens/range` is built from (E122, E125).
    ///
    /// Selecting the same tokens by filtering the whole stream is what this
    /// replaces, and it cost the FILE rather than the window: a whole-file walk
    /// of the program plus one line lookup per token in the file, so twenty
    /// visible lines cost what the whole file cost (12.2 ms on kolt's
    /// `views.vl`, `proposal/editor-latency.md` §1.6). Here it is a clamp and
    /// two index reads.
    ///
    /// Positions rather than a slice because a LIVE viewport can be carried by
    /// two disjoint stretches of the capture — the analyzed lines that reach it
    /// through the anchor's head, and the ones that reach it through its tail
    /// (E125, `Document::keystroke_tokens_in_lines`) — and merging two index
    /// ranges is arithmetic where merging two slices would be a copy.
    pub fn token_positions_in_lines(&self, first_line: u32, last_line: u32) -> Range<usize> {
        let Some(last) = self.token_lines.len().checked_sub(1) else {
            return 0..0;
        };
        let first = self.token_lines[(first_line as usize).min(last)] as usize;
        let past = self.token_lines[(last_line as usize).saturating_add(1).min(last)] as usize;
        first..past.max(first)
    }

    /// The tokens the live buffer should be painted with, given `anchor` and
    /// `verdict` — the §2.1.3 split, in one place.
    ///
    /// In **every** verdict the edit window is syntax-only, which is the
    /// property Q5 turns on: whatever the analysis thinks, the region the
    /// user's eyes are in is painted from the bytes on screen.
    pub fn tokens_for(
        &self,
        live: &str,
        anchor: &Anchor,
        verdict: Verdict,
    ) -> Vec<(Span, TokenKind, u32)> {
        match verdict {
            Verdict::Exact => {
                let mut painted: Vec<(Span, TokenKind, u32)> = self
                    .tokens
                    .iter()
                    .filter_map(|(span, kind, modifiers)| {
                        anchor.map_span(*span).map(|span| (span, *kind, *modifiers))
                    })
                    .collect();
                painted.extend(syntax_tokens_in(live, anchor.live_window()));
                sort_and_deoverlap(painted)
            }
            // A name binding moved, so no landed classification is trustworthy
            // — and syntax-only is never wrong, so the whole file takes it.
            Verdict::Stale | Verdict::Unusable => {
                sort_and_deoverlap(syntax_tokens_in(live, 0..live.len()))
            }
        }
    }

    /// The inlay hints the live buffer should show. Q1/Q4's ruling, in one
    /// place: re-mapped through the anchor, **withheld inside the window**
    /// (a hint on the line you are typing is the most likely to be wrong and
    /// the least useful, and withholding there is invisible because the hint
    /// was about to move anyway), served unchanged when stale rather than
    /// flickered off, and withheld entirely when there is no anchor to serve
    /// from.
    pub fn hints_for(&self, anchor: &Anchor, verdict: Verdict) -> Vec<(usize, String)> {
        if verdict == Verdict::Unusable {
            return Vec::new();
        }
        let mut hints: Vec<(usize, String)> = self
            .hints
            .iter()
            .filter_map(|(offset, label)| {
                anchor
                    .map_offset(*offset)
                    .map(|offset| (offset, label.clone()))
            })
            .collect();
        hints.sort();
        hints
    }
}

// ---------------------------------------------------------------------------
// 6. The cursor's syntactic context
// ---------------------------------------------------------------------------

/// What the cursor is asking for, decided from the LIVE buffer's **current
/// line** alone.
///
/// Reading one line is the whole point: `completion.rs:1095` re-tokenizes the
/// entire buffer on every keystroke to answer this, and the answer only ever
/// depends on the few characters behind the cursor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CursorContext {
    /// A bare word (possibly empty) in expression or statement position.
    Scope { prefix: String },
    /// `module::pre` — a path segment after `::`. `nested` says the path has
    /// MORE than one segment (`style::FlexDirection::pre`), in which case
    /// `module` is only its last one and the index — which is keyed by module
    /// name alone — cannot answer it: the question is a descent through the
    /// head, which only the landed engine can walk (E129).
    Path {
        module: String,
        prefix: String,
        nested: bool,
    },
    /// `receiver.pre` — a member after `.`. The receiver's type is a
    /// resolution question, which is why this arm is the one that consults the
    /// landed analysis.
    Member { prefix: String },
    /// Inside a comment, a string, or otherwise nowhere a name can go.
    None,
}

/// Classify `offset` in `text`. Byte offsets; `offset` is clamped into range.
pub fn cursor_context(text: &str, offset: usize) -> CursorContext {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());
    let start = line_start(bytes, offset);
    let line = &text[start..offset];
    // A `//` anywhere earlier on the line puts the cursor in a comment, and a
    // comment is nowhere a name can go. (An odd count of unescaped quotes says
    // the same about a string, and is the cheap test that catches it.)
    if line.contains("//") || line.bytes().filter(|byte| *byte == b'"').count() % 2 == 1 {
        return CursorContext::None;
    }
    let identifier_start = line
        .rfind(|character: char| !is_identifier_char(character))
        .map_or(0, |position| {
            position + line[position..].chars().next().map_or(1, char::len_utf8)
        });
    let prefix = line[identifier_start..].to_string();
    let before = &line[..identifier_start];
    if let Some(head) = before.strip_suffix("::") {
        let module_start = head
            .rfind(|character: char| !is_identifier_char(character))
            .map_or(0, |position| {
                position + head[position..].chars().next().map_or(1, char::len_utf8)
            });
        let module = head[module_start..].to_string();
        if !module.is_empty() {
            return CursorContext::Path {
                module,
                prefix,
                nested: head[..module_start].ends_with("::"),
            };
        }
    }
    if before.ends_with('.') && !before.ends_with("..") {
        return CursorContext::Member { prefix };
    }
    CursorContext::Scope { prefix }
}

fn is_identifier_char(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Turn index entries into completion candidates, filtered by `prefix`.
///
/// A `Completion` the keystroke path produces carries no `needs_import` and no
/// snippet: auto-import is a resolution question and belongs to the analysis's
/// own completion, which the client gets on the next landing.
pub fn candidates(entries: &[SymbolEntry], prefix: &str) -> Vec<Completion> {
    entries
        .iter()
        .filter(|entry| prefix.is_empty() || entry.name.starts_with(prefix))
        .map(|entry| Completion {
            label: entry.name.clone(),
            kind: entry.kind,
            detail: entry.signature.clone(),
            documentation: None,
            call_parameters: entry.call_parameters.clone(),
            snippet: None,
            needs_import: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- the two-sided anchor ---------------------------------------------

    #[test]
    fn identical_texts_anchor_the_whole_file() {
        let text = "fun main() {\n\tlet x = 1;\n}\n";
        let anchor = Anchor::compute(text, text);
        assert_eq!(anchor.prefix, text.len());
        assert_eq!(anchor.suffix, 0);
        assert_eq!(anchor.shift(), 0);
        assert!(anchor.live_window().is_empty());
        // Every landed span survives, unmoved.
        let span = Span { start: 4, end: 8 };
        assert_eq!(anchor.map_span(span), Some(span));
    }

    #[test]
    fn a_middle_edit_anchors_on_both_sides_and_the_window_is_one_line() {
        let analyzed = "fun a() {\n\tlet x = 1;\n}\nfun b() {\n\tlet y = 2;\n}\n";
        let live = "fun a() {\n\tlet xx = 1;\n}\nfun b() {\n\tlet y = 2;\n}\n";
        let anchor = Anchor::compute(analyzed, live);
        // The prefix stops at the start of the edited LINE, not at the edited
        // byte — B38's trim, applied to the head.
        assert_eq!(&analyzed[..anchor.prefix], "fun a() {\n");
        assert_eq!(
            &live[live.len() - anchor.suffix..],
            "}\nfun b() {\n\tlet y = 2;\n}\n"
        );
        assert_eq!(anchor.shift(), 1);
        assert_eq!(&live[anchor.live_window()], "\tlet xx = 1;\n");
        // A head token maps to itself; a tail token maps by the shift.
        assert_eq!(
            anchor.map_span(Span { start: 4, end: 5 }),
            Some(Span { start: 4, end: 5 })
        );
        let tail = Span {
            start: analyzed.len() - 6,
            end: analyzed.len() - 5,
        };
        assert_eq!(
            anchor.map_span(tail),
            Some(Span {
                start: tail.start + 1,
                end: tail.end + 1
            })
        );
    }

    #[test]
    fn a_span_inside_the_window_has_no_image() {
        let analyzed = "fun a() {\n\tlet x = 1;\n}\n";
        let live = "fun a() {\n\tlet xx = 1;\n}\n";
        let anchor = Anchor::compute(analyzed, live);
        // `x`'s own span is in the window: dropped, not clamped.
        assert_eq!(anchor.map_span(Span { start: 15, end: 16 }), None);
    }

    #[test]
    fn a_span_straddling_a_window_edge_is_dropped() {
        let analyzed = "fun a() {\n\tlet x = 1;\n}\n";
        let live = "fun a() {\n\tlet xx = 1;\n}\n";
        let anchor = Anchor::compute(analyzed, live);
        // A span from the head anchor across into the window.
        let straddler = Span {
            start: anchor.prefix - 2,
            end: anchor.prefix + 4,
        };
        assert_eq!(anchor.map_span(straddler), None);
    }

    #[test]
    fn a_wholesale_replacement_leaves_no_anchor() {
        let anchor = Anchor::compute("fun a() {}\n", "struct B { x: i32 }");
        assert!(anchor.is_empty());
        assert_eq!(
            Verdict::decide(&anchor, true, false),
            Verdict::Unusable,
            "a paste that shares no line is the unusable state, whatever the stamp says",
        );
    }

    #[test]
    fn the_two_anchors_never_overlap_on_a_repeating_text() {
        // `"aa"` → `"a"`: the common prefix and the common suffix are the same
        // byte, and an unclamped anchor would claim both.
        let anchor = Anchor::compute("aa", "a");
        assert!(anchor.prefix + anchor.suffix <= 1);
        let anchor = Anchor::compute("\n\n\n", "\n\n");
        assert!(anchor.prefix + anchor.suffix <= anchor.live_len);
        assert!(anchor.prefix + anchor.suffix <= anchor.analyzed_len);
    }

    #[test]
    fn a_multibyte_edit_lands_on_char_boundaries() {
        // The trims must never split a UTF-8 sequence — the whole file is
        // sliced by these offsets.
        let analyzed = "let a = \"héllo wörld\";\nlet b = 1;\n";
        let live = "let a = \"héllo wörld!\";\nlet b = 1;\n";
        let anchor = Anchor::compute(analyzed, live);
        assert!(analyzed.is_char_boundary(anchor.prefix));
        assert!(live.is_char_boundary(anchor.prefix));
        assert!(live.is_char_boundary(live.len() - anchor.suffix));
        assert!(analyzed.is_char_boundary(analyzed.len() - anchor.suffix));
        // And the window really is the edited line.
        assert_eq!(&live[anchor.live_window()], "let a = \"héllo wörld!\";\n");
    }

    #[test]
    fn an_append_anchors_the_whole_prefix() {
        let analyzed = "fun a() {}\n";
        let live = "fun a() {}\nfun b() {}\n";
        let anchor = Anchor::compute(analyzed, live);
        assert_eq!(anchor.prefix, analyzed.len());
        assert_eq!(&live[anchor.live_window()], "fun b() {}\n");
        assert_eq!(
            anchor.map_span(Span { start: 4, end: 5 }),
            Some(Span { start: 4, end: 5 })
        );
    }

    // --- the declaration-shape stamp ---------------------------------------

    #[test]
    fn a_body_edit_leaves_the_stamp_alone() {
        let before = "fun main() {\n\tlet x = 1;\n}\n";
        let after = "fun main() {\n\tlet x = 1;\n\tlet y = 2;\n}\n";
        assert_eq!(
            shape_stamp(before),
            shape_stamp(after),
            "nothing outside a body changed, so no module-scope name's resolution can have moved",
        );
    }

    #[test]
    fn a_renamed_declaration_moves_the_stamp() {
        assert_ne!(
            shape_stamp("fun main() {\n\tlet x = 1;\n}\n"),
            shape_stamp("fun other() {\n\tlet x = 1;\n}\n"),
        );
    }

    #[test]
    fn a_changed_signature_moves_the_stamp() {
        assert_ne!(
            shape_stamp("fun f(a: i32) {\n\ta;\n}\n"),
            shape_stamp("fun f(a: str) {\n\ta;\n}\n"),
            "a signature change re-classifies every call site in the anchors",
        );
    }

    #[test]
    fn a_changed_import_moves_the_stamp() {
        assert_ne!(
            shape_stamp("import std::io::print;\nfun f() {}\n"),
            shape_stamp("import std::io::println;\nfun f() {}\n"),
            "an import line moves the file's whole name resolution",
        );
    }

    #[test]
    fn a_new_declaration_moves_the_stamp() {
        assert_ne!(
            shape_stamp("fun f() {}\n"),
            shape_stamp("fun f() {}\nfun g() {}\n"),
        );
    }

    #[test]
    fn struct_fields_are_shape_not_body() {
        assert_ne!(
            shape_stamp("struct S { x: i32 }\n"),
            shape_stamp("struct S { y: i32 }\n"),
            "a field name binds `.x`, so it is declaration shape and not a body",
        );
    }

    #[test]
    fn moving_a_declaration_down_a_line_leaves_the_stamp_alone() {
        assert_eq!(
            shape_stamp("fun f() {}\nfun g() {}\n"),
            shape_stamp("fun f() {}\n\n\nfun g() {}\n"),
            "spans are excluded: where a declaration sits does not change what it binds",
        );
    }

    #[test]
    fn a_body_only_stamp_survives_two_nested_bodies() {
        let before = "fun outer() {\n\tfun inner() {\n\t\tlet a = 1;\n\t}\n}\nfun after() {}\n";
        let after = "fun outer() {\n\tfun inner() {\n\t\tlet a = 2;\n\t}\n}\nfun after() {}\n";
        assert_eq!(shape_stamp(before), shape_stamp(after));
        // …but the trailing declaration is still seen: removing it moves it.
        let trimmed = "fun outer() {\n\tfun inner() {\n\t\tlet a = 1;\n\t}\n}\n";
        assert_ne!(shape_stamp(before), shape_stamp(trimmed));
    }

    #[test]
    fn a_lex_error_is_a_shape_change() {
        // Mid-keystroke garbage must not be reported as an unchanged shape.
        assert_ne!(shape_stamp("fun f() {}\n"), shape_stamp("fun f() {}\n\"\n"));
    }

    // --- the verdicts -------------------------------------------------------

    #[test]
    fn the_three_verdicts_are_decided_by_anchor_then_stamp_then_dependency() {
        let anchor = Anchor::compute(
            "fun a() {\n\tlet x = 1;\n}\n",
            "fun a() {\n\tlet y = 1;\n}\n",
        );
        assert!(!anchor.is_empty());
        assert_eq!(Verdict::decide(&anchor, true, false), Verdict::Exact);
        assert_eq!(Verdict::decide(&anchor, false, false), Verdict::Stale);
        assert_eq!(
            Verdict::decide(&anchor, true, true),
            Verdict::Stale,
            "another module moving is the paper's case 4 — no local anchoring repairs it",
        );
        let none = Anchor::compute("fun a() {}\n", "totally different");
        assert_eq!(Verdict::decide(&none, true, false), Verdict::Unusable);
    }

    // --- syntax-only classification -----------------------------------------

    #[test]
    fn syntax_classifies_the_roles_syntax_can_decide() {
        let text =
            "fun greet(name) {\n\tlet who = name;\n\twho.trim();\n\twho.length;\n\tstd::io;\n}\n";
        let tokens = syntax_tokens_in(text, 0..text.len());
        let at = |needle: &str| -> Option<(TokenKind, u32)> {
            let start = text.find(needle)?;
            tokens
                .iter()
                .find(|(span, _, _)| span.start == start && span.end == start + needle.len())
                .map(|(_, kind, modifiers)| (*kind, *modifiers))
        };
        assert_eq!(
            at("greet"),
            Some((TokenKind::Function, MODIFIER_DECLARATION))
        );
        assert_eq!(
            at("who"),
            Some((
                TokenKind::Variable,
                MODIFIER_DECLARATION | MODIFIER_READONLY
            ))
        );
        assert_eq!(at("trim"), Some((TokenKind::Method, 0)));
        assert_eq!(at("length"), Some((TokenKind::Property, 0)));
        assert_eq!(at("std"), Some((TokenKind::Namespace, 0)));
    }

    #[test]
    fn a_plain_identifier_gets_no_syntax_token() {
        // `name` in `let who = name;` is a use of something syntax cannot
        // classify — a variable, a constant, a unit struct. Emitting a guess
        // would be the lie the anchors exist to avoid.
        let text = "fun greet(name) {\n\tlet who = name;\n}\n";
        let tokens = syntax_tokens_in(text, 0..text.len());
        let use_site = text.rfind("name").expect("the use site");
        assert!(
            !tokens.iter().any(|(span, _, _)| span.start == use_site),
            "syntax must stay silent where only resolution can answer",
        );
    }

    /// **The owner's Q5 exhibit.** Commenting a line out must read as a comment
    /// at once, not keep its semantic colours for the staleness window.
    #[test]
    fn commenting_a_line_out_removes_its_tokens_on_the_next_keystroke() {
        let analyzed = "fun main() {\n\tgreet(who);\n\tgreet(who);\n}\n";
        let live = "fun main() {\n\t// greet(who);\n\tgreet(who);\n}\n";
        let anchor = Anchor::compute(analyzed, live);
        let window = anchor.live_window();
        assert_eq!(
            &live[window.clone()],
            "\t// greet(who);\n",
            "the commented line is exactly the edit window",
        );
        let syntax = syntax_tokens_in(live, window);
        assert!(
            syntax.is_empty(),
            "the lexer produces no token for a comment, so the window paints nothing and the \
             client's grammar colours the line as a comment — {syntax:?}",
        );
        // And the second, still-live call keeps its function colouring: it is
        // in the tail anchor, so the landed answer maps onto it.
        let landed_call = analyzed.rfind("greet").expect("the second call");
        let mapped = anchor
            .map_span(Span {
                start: landed_call,
                end: landed_call + 5,
            })
            .expect("the second call is in the tail anchor");
        assert_eq!(&live[mapped.into_range()], "greet");
    }

    #[test]
    fn a_window_token_must_lie_wholly_inside_the_window() {
        let text = "fun a() {\n\tlet x = 1;\n}\n";
        // A window that cuts `let x` in half must not emit `x`.
        let cut = text.find("x").expect("the binding") + 1;
        let tokens = syntax_tokens_in(text, 0..cut);
        assert!(tokens.iter().all(|(span, _, _)| span.end <= cut));
    }

    #[test]
    fn sorting_drops_overlaps_narrowest_first() {
        let stream = vec![
            (Span { start: 5, end: 9 }, TokenKind::Function, 0),
            (Span { start: 5, end: 7 }, TokenKind::Variable, 0),
            (Span { start: 0, end: 3 }, TokenKind::Struct, 0),
            (Span { start: 8, end: 8 }, TokenKind::Method, 0),
        ];
        let kept = sort_and_deoverlap(stream);
        assert_eq!(
            kept.iter().map(|(span, ..)| *span).collect::<Vec<_>>(),
            vec![Span { start: 0, end: 3 }, Span { start: 5, end: 7 }],
        );
    }

    // --- the symbol index ---------------------------------------------------

    #[test]
    fn syntax_symbols_read_a_modules_declarations_off_its_tokens() {
        let text = "import std::io::print;\n\
                    fun greet(name: str, times: i32) {}\n\
                    struct Point { x: i32 }\n\
                    enum Colour { Red }\n\
                    trait Show {}\n\
                    let TAU = 6.28;\n";
        let entries = syntax_symbols(text);
        let named = |name: &str| entries.iter().find(|entry| entry.name == name);
        assert_eq!(
            named("greet").map(|e| e.kind),
            Some(CompletionKind::Function)
        );
        assert_eq!(
            named("greet").and_then(|e| e.call_parameters.clone()),
            Some(vec!["name".to_string(), "times".to_string()]),
        );
        assert_eq!(
            named("greet").and_then(|e| e.signature.clone()),
            Some("fun greet(name, times)".to_string()),
        );
        assert_eq!(named("Point").map(|e| e.kind), Some(CompletionKind::Struct));
        assert_eq!(named("Colour").map(|e| e.kind), Some(CompletionKind::Enum));
        assert_eq!(named("Show").map(|e| e.kind), Some(CompletionKind::Trait));
        assert_eq!(named("TAU").map(|e| e.kind), Some(CompletionKind::Variable));
        // A local inside a body is NOT an export.
        assert!(
            syntax_symbols("fun f() {\n\tlet inner = 1;\n}\n")
                .iter()
                .all(|entry| entry.name != "inner"),
        );
        // Everything read this way is marked syntactic.
        assert!(entries.iter().all(|entry| entry.analysis_epoch == 0));
    }

    #[test]
    fn the_entry_module_rebuilds_only_when_its_shape_moves() {
        let mut index = SymbolIndex::default();
        assert!(index.refresh_entry_from_syntax("fun f() {}\n"));
        assert!(
            !index.refresh_entry_from_syntax("fun f() {\n\tlet x = 1;\n}\n"),
            "a body edit must not invalidate the module's export list",
        );
        assert!(
            index.refresh_entry_from_syntax("fun f() {}\nfun g() {}\n"),
            "a new declaration must",
        );
        let entry = &index.by_module[SymbolIndex::ENTRY];
        assert_eq!(
            entry
                .entries
                .iter()
                .map(|e| e.name.as_str())
                .collect::<Vec<_>>(),
            vec!["f", "g"],
        );
    }

    #[test]
    fn a_module_is_looked_up_by_the_name_a_path_spells_it_with() {
        let index = SymbolIndex {
            by_module: vec![
                ModuleSymbols::default(),
                ModuleSymbols {
                    path: Some(PathBuf::from("/pkg/src/style.vl")),
                    module_name: module_name_of(Path::new("/pkg/src/style.vl")),
                    stamp: None,
                    entries: vec![SymbolEntry {
                        name: "colour".to_string(),
                        kind: CompletionKind::Function,
                        signature: None,
                        call_parameters: Some(Vec::new()),
                        analysis_epoch: 7,
                    }],
                },
            ],
            ..SymbolIndex::default()
        };
        let module = index.module("style").expect("style is indexed");
        assert_eq!(module.entries.len(), 1);
        assert_eq!(module.entries[0].analysis_epoch, 7);
        assert!(index.module("nothing").is_none());
    }

    // --- the cursor's context -----------------------------------------------

    #[test]
    fn the_cursor_context_is_read_from_the_line_alone() {
        let text = "fun f() {\n\tlet a = st\n}\n";
        let offset = text.find("st\n").expect("the prefix") + 2;
        assert_eq!(
            cursor_context(text, offset),
            CursorContext::Scope {
                prefix: "st".to_string()
            }
        );

        let text = "fun f() {\n\tstyle::col\n}\n";
        let offset = text.find("col\n").expect("the prefix") + 3;
        assert_eq!(
            cursor_context(text, offset),
            CursorContext::Path {
                module: "style".to_string(),
                prefix: "col".to_string(),
                nested: false,
            }
        );

        // E129: a nested path names its LAST segment, and says so — the index
        // is keyed by module name and would happily answer with an unrelated
        // module that happens to share it.
        let text = "fun f() {\n\tstyle::FlexDirection::Ro\n}\n";
        let offset = text.find("Ro\n").expect("the prefix") + 2;
        assert_eq!(
            cursor_context(text, offset),
            CursorContext::Path {
                module: "FlexDirection".to_string(),
                prefix: "Ro".to_string(),
                nested: true,
            }
        );

        let text = "fun f() {\n\twho.tr\n}\n";
        let offset = text.find("tr\n").expect("the prefix") + 2;
        assert_eq!(
            cursor_context(text, offset),
            CursorContext::Member {
                prefix: "tr".to_string()
            }
        );
    }

    #[test]
    fn a_cursor_inside_a_comment_or_string_asks_for_nothing() {
        let text = "fun f() {\n\t// let a = st\n}\n";
        let offset = text.find("st\n").expect("the prefix") + 2;
        assert_eq!(cursor_context(text, offset), CursorContext::None);

        let text = "fun f() {\n\tlet a = \"st\n}\n";
        let offset = text.find("st\n").expect("the prefix") + 2;
        assert_eq!(cursor_context(text, offset), CursorContext::None);
    }

    #[test]
    fn candidates_filter_by_the_typed_prefix() {
        let entries = syntax_symbols("fun greet() {}\nfun grow() {}\nfun other() {}\n");
        let offered = candidates(&entries, "gr");
        assert_eq!(
            offered.iter().map(|c| c.label.as_str()).collect::<Vec<_>>(),
            vec!["greet", "grow"],
        );
        assert_eq!(candidates(&entries, "").len(), 3);
    }
}

/// The keystroke path against a real analyzed `Document` — the pins that say
/// the mechanism above is actually wired to the answers the server gives.
///
/// Every one of these is red without this lane: today every provider answers
/// the ANALYZED snapshot and holds still until the next analysis lands
/// (`main.rs`'s `snapshot_consistency_tests`, §1.5's measured 409 ms window),
/// which is precisely what E121's Q5 ruling overturns for the edited region.
#[cfg(test)]
mod document_path {
    use super::*;
    use crate::document::Document;
    use crate::document::tests::std_root;

    const SOURCE: &str = "fun greet(name: str): str {\n\tlet who = name;\n\twho\n}\n\n\
                          fun main() {\n\tlet first = greet(\"a\");\n\tlet second = greet(\"b\");\n}\n";

    fn analyzed(text: &str) -> Document {
        let directory = std::env::temp_dir().join(format!(
            "vilan_keystroke_{}_{:p}",
            std::process::id(),
            text.as_ptr()
        ));
        let _ = std::fs::create_dir_all(&directory);
        let entry = directory.join("main.vl");
        let document = Document::analyze(text, &std_root(), &entry);
        let _ = std::fs::remove_dir_all(&directory);
        document
    }

    fn token_at<'a>(
        tokens: &'a [(Span, TokenKind, u32)],
        text: &str,
        needle: &str,
    ) -> Option<&'a (Span, TokenKind, u32)> {
        let start = text.find(needle)?;
        tokens
            .iter()
            .find(|(span, ..)| span.start == start && span.end == start + needle.len())
    }

    #[test]
    fn a_landed_analysis_captures_its_answers_once() {
        let document = analyzed(SOURCE);
        assert_eq!(
            document.keystroke_verdict(false),
            Verdict::Exact,
            "an unedited buffer is exact against its own analysis",
        );
        let tokens = document.keystroke_tokens(false);
        assert!(!tokens.is_empty(), "the fixture must produce tokens");
        assert_eq!(
            token_at(&tokens, SOURCE, "greet").map(|(_, kind, _)| *kind),
            Some(TokenKind::Function),
            "the landed classification is served whole when nothing has been typed",
        );
        assert!(
            !document.keystroke_hints(false).is_empty(),
            "the fixture must produce hints",
        );
    }

    /// **The owner's Q5 exhibit, end to end.** Comment a line out and ask for
    /// tokens on the very next keystroke: the line must carry none, so the
    /// client paints it as a comment instead of holding its function-and-
    /// variable colours for the whole staleness window.
    #[test]
    fn commenting_a_line_out_reads_as_a_comment_on_the_next_keystroke() {
        let mut document = analyzed(SOURCE);
        let landed = document.keystroke_tokens(false);
        let call_line = SOURCE
            .find("\tlet first = greet(\"a\");")
            .expect("the call line");
        assert!(
            landed
                .iter()
                .any(|(span, ..)| span.start >= call_line && span.start < call_line + 23),
            "the line must be coloured before it is commented out",
        );

        let edited = SOURCE.replace(
            "\tlet first = greet(\"a\");",
            "\t// let first = greet(\"a\");",
        );
        document.set_text(&edited);
        let commented = edited
            .find("\t// let first = greet(\"a\");")
            .expect("the commented line");
        let end = commented + "\t// let first = greet(\"a\");".len();
        let tokens = document.keystroke_tokens(false);
        let inside: Vec<_> = tokens
            .iter()
            .filter(|(span, ..)| span.start >= commented && span.start < end)
            .collect();
        assert!(
            inside.is_empty(),
            "a commented-out line must carry no semantic token on the very next keystroke — {inside:?}",
        );
        // …and the still-live sibling call keeps its colouring: it is in the
        // tail anchor, so the landed answer maps onto it.
        assert_eq!(
            token_at(&tokens, &edited, "second").map(|(_, kind, _)| *kind),
            Some(TokenKind::Variable),
            "the tail anchor must keep serving the landed classification",
        );
    }

    #[test]
    fn a_body_edit_stays_exact_and_repaints_only_its_own_line() {
        let mut document = analyzed(SOURCE);
        let edited = SOURCE.replace("\tlet who = name;", "\tlet whom = name;");
        document.set_text(&edited);
        assert_eq!(
            document.keystroke_verdict(false),
            Verdict::Exact,
            "renaming a LOCAL changes no module-scope binding, so the stamp holds",
        );
        let tokens = document.keystroke_tokens(false);
        // The renamed local is painted from syntax — same class, new position.
        assert_eq!(
            token_at(&tokens, &edited, "whom").map(|(_, kind, _)| *kind),
            Some(TokenKind::Variable),
        );
        // Everything after the edit moved by exactly one byte and kept its
        // landed classification.
        assert_eq!(
            token_at(&tokens, &edited, "main").map(|(_, kind, _)| *kind),
            Some(TokenKind::Function),
        );
    }

    #[test]
    fn renaming_a_declaration_degrades_the_file_to_syntax() {
        let mut document = analyzed(SOURCE);
        document.set_text(&SOURCE.replace("fun greet(", "fun greets("));
        assert_eq!(
            document.keystroke_verdict(false),
            Verdict::Stale,
            "a top-level rename re-classifies every use of that name",
        );
    }

    #[test]
    fn a_wholesale_paste_is_unusable_and_withholds_every_hint() {
        let mut document = analyzed(SOURCE);
        document.set_text("struct Replaced { field: i32 }");
        assert_eq!(document.keystroke_verdict(false), Verdict::Unusable);
        assert!(
            document.keystroke_hints(false).is_empty(),
            "with no anchor there is nothing to re-map a hint onto, and a hint is a claim \
             about a type — withholding beats lying (Q4)",
        );
        // Tokens still answer, from syntax alone.
        let tokens = document.keystroke_tokens(false);
        assert_eq!(
            token_at(&tokens, "struct Replaced { field: i32 }", "Replaced")
                .map(|(_, kind, _)| *kind),
            Some(TokenKind::Struct),
        );
    }

    /// Q1/Q4: a hint on the line being typed disappears until the analysis
    /// lands, and every hint outside the window keeps its landed label at an
    /// exact position.
    #[test]
    fn hints_are_withheld_inside_the_edit_window_and_exact_outside_it() {
        let mut document = analyzed(SOURCE);
        let landed = document.keystroke_hints(false);
        assert!(
            landed.len() >= 2,
            "the fixture must produce hints: {landed:?}"
        );
        let edited = SOURCE.replace(
            "\tlet first = greet(\"a\");",
            "\tlet first = greet(\"ab\");",
        );
        document.set_text(&edited);
        let window = document.keystroke_anchor().live_window();
        let hints = document.keystroke_hints(false);
        assert!(
            hints.iter().all(|(offset, _)| !window.contains(offset)),
            "no hint may be served inside the edit window — {hints:?} against {window:?}",
        );
        // And the hint on the LAST line rode the shift: it still sits on
        // `second`'s name, not one byte off it.
        let second = edited.find("second").expect("the second binding");
        assert!(
            hints
                .iter()
                .any(|(offset, _)| *offset == second + "second".len()),
            "a hint outside the window must be position-exact — {hints:?}",
        );
    }

    #[test]
    fn completion_offers_a_declaration_typed_one_keystroke_ago() {
        let mut document = analyzed(SOURCE);
        let edited = format!("{SOURCE}\nfun brand_new_helper() {{}}\n");
        document.set_text(&edited);
        let offset =
            edited.find("brand_new_helper").expect("the new name") + "brand_new_helper".len();
        assert!(
            document
                .keystroke_index()
                .by_module
                .first()
                .is_some_and(|entry| entry
                    .entries
                    .iter()
                    .any(|symbol| symbol.name == "brand_new_helper")),
            "the index must follow the LIVE buffer — a `fun` typed one keystroke ago \
             cannot be in the landed analysis",
        );
        let offered = document.keystroke_completion(offset, false);
        assert!(
            offered.iter().any(|item| item.label == "brand_new_helper"),
            "the index's whole point: it answers what the analysis has not seen yet",
        );
    }
}

/// E121 §6: the gate.
///
/// A pin family under the **thread clock** — M15's method, and for M15's exact
/// reason: this order recorded loadavg 8 to 80 on one 16-core machine, wall
/// readings moved by 5×, and CPU readings did not. Every assertion here is on
/// CPU time; wall and loadavg are recorded beside it and asserted on nowhere.
///
/// The subject is **generated** (Q6, and the owner's standing rule that kolt is
/// never integrated into this codebase): a synthetic module of N functions of
/// one shape over a shared wrapper, plus an app-shaped entry that calls four of
/// them — the §6.1 method, at N = 1,791 because that is kolt-with-lucide's
/// size. It is a generator in this file, not checked-in bulk.
///
/// **Why it is not vacuous.** Every run prints, beside the keystroke path's
/// number, the cost of the whole-program walk the path replaces — the same
/// `Document::semantic_tokens()` the paper measured at 13.4 ms of CPU on this
/// exhibit's size. That walk IS the planted regression §6.2 asks for: it is the
/// shape §2.1 removes, it is compiled in the same binary, and the gate asserts
/// that the path is under budget while recording that the walk it replaced is
/// not. Swapping which of the two is asserted reds the gate immediately.
///
/// Run the full gate deliberately:
///
/// ```text
/// cargo nextest run --release -p vilan-lsp --run-ignored ignored-only \
///     -E 'test(keystroke_path)' --no-capture > gate.log 2>&1
/// ```
#[cfg(test)]
pub(crate) mod gate {
    use crate::document::Document;
    use crate::document::tests::std_root;
    use std::time::{Duration, Instant};

    /// kolt-with-lucide's reachable-function count — §6.1's subject size.
    pub(crate) const GATE_FUNCTIONS: usize = 1791;
    /// The smoke subject: enough to exercise every seam, seconds to analyze.
    const SMOKE_FUNCTIONS: usize = 24;
    /// The mandate's budget, per request and for the burst.
    const BUDGET_MS: f64 = 10.0;
    /// M25's own bound on completion, which the mandate's 10 ms does not
    /// discriminate: at the end of the keystroke-path tranche completion WAS
    /// the whole remaining budget, and the tranche's claim is that it stopped
    /// costing the codebase. Measured on this exhibit, same machine, same
    /// instrument: **0.628 ms** with the per-request sweep of every `std`/`pkg`
    /// child module's `name_to_id_map` (loadavg 66), **0.126 ms** reading the
    /// table that sweep now fills once per analysis (loadavg 82). The bound
    /// sits between two MECHANISMS rather than between two tunings — no
    /// per-request sweep of a 1,791-function program gets under it.
    const COMPLETION_BUDGET_MS: f64 = 0.2;
    /// Requests per measured window — amortized past any clock granularity,
    /// the same discipline the paper's P8 used.
    const REPETITIONS: usize = 50;

    /// The calling thread's CPU time — the time it was actually ON a core, not
    /// the time that passed (backlog M15). `None` where the host exposes no
    /// such clock.
    #[cfg(unix)]
    pub(crate) fn thread_cpu_now() -> Option<Duration> {
        let mut timespec = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `clock_gettime` writes the `timespec` we hand it and reads
        // nothing else; the pointer is to a live local.
        let result = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut timespec) };
        (result == 0).then(|| {
            Duration::new(
                timespec.tv_sec.max(0) as u64,
                timespec.tv_nsec.clamp(0, 999_999_999) as u32,
            )
        })
    }

    /// The Windows half: kernel + user time for the current thread.
    #[cfg(windows)]
    pub(crate) fn thread_cpu_now() -> Option<Duration> {
        use windows_sys::Win32::Foundation::FILETIME;
        use windows_sys::Win32::System::Threading::{GetCurrentThread, GetThreadTimes};

        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: `GetThreadTimes` writes the four `FILETIME`s we hand it and
        // reads nothing else; `GetCurrentThread` returns a pseudo-handle that
        // needs no close. The pointers are to live locals.
        let ok = unsafe {
            GetThreadTimes(
                GetCurrentThread(),
                &mut creation,
                &mut exit,
                &mut kernel,
                &mut user,
            )
        };
        if ok == 0 {
            return None;
        }
        let hundred_nanoseconds =
            |time: FILETIME| ((time.dwHighDateTime as u64) << 32) | (time.dwLowDateTime as u64);
        let ticks = hundred_nanoseconds(kernel) + hundred_nanoseconds(user);
        Some(Duration::from_nanos(ticks.saturating_mul(100)))
    }

    #[cfg(not(any(unix, windows)))]
    pub(crate) fn thread_cpu_now() -> Option<Duration> {
        None
    }

    /// The whole PROCESS's CPU time — every thread's, summed (M26).
    ///
    /// M15's thread clock is the right instrument for a request, which is
    /// answered on the thread that asks. It is the wrong one for an ANALYSIS,
    /// which runs on its own spawned 128 MiB thread and accrues nothing to the
    /// caller's clock — the `diagnostics_budget` pin says so in as many words,
    /// and measures wall instead. This is the clock that can see it: the cost
    /// of a keystroke burst is the CPU every analysis thread it spawned burned,
    /// wherever they ran, and it is load-proof for the same reason the thread
    /// clock is.
    ///
    /// `None` where the host exposes no such clock — and on Windows, where the
    /// per-process counterpart (`GetProcessTimes`) is a different API than the
    /// thread one above and no measurement here has ever needed it.
    #[cfg(unix)]
    pub(crate) fn process_cpu_now() -> Option<Duration> {
        let mut timespec = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: `clock_gettime` writes the `timespec` we hand it and reads
        // nothing else; the pointer is to a live local.
        let result = unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut timespec) };
        (result == 0).then(|| {
            Duration::new(
                timespec.tv_sec.max(0) as u64,
                timespec.tv_nsec.clamp(0, 999_999_999) as u32,
            )
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn process_cpu_now() -> Option<Duration> {
        None
    }

    /// The 1-minute load average at report time — M13's provenance, printed
    /// beside every number and asserted on nowhere.
    pub(crate) fn loadavg_1m() -> String {
        std::fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|text| text.split_whitespace().next().map(str::to_string))
            .unwrap_or_else(|| "?".to_string())
    }

    pub(crate) fn profile() -> &'static str {
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    }

    /// Whether this build may assert a budget — **false under
    /// `debug_assertions`**, where it prints why instead (E141).
    ///
    /// Every per-request and per-keystroke budget in this module is a RELEASE
    /// figure. M25 derived [`COMPLETION_BUDGET_MS`] from 0.628 ms → 0.126 ms
    /// measured in release; the mandate's 10 ms and `diagnostics_budget`'s
    /// 500 ms are release figures for the same reason — they are about what an
    /// editor's user waits for, and nobody edits against a debug compiler. The
    /// gates carry `#[ignore]` for their cost, so PR CI never runs them, and
    /// two Order 27 lanes that ran them the only way an ignored test is
    /// normally run — `cargo nextest run --run-ignored`, in the default debug
    /// profile — measured completion at 0.705–0.813 ms against the 0.2 ms
    /// budget and read a 4× drift into the tranche. There was none: the same
    /// gate on a quiet box in release measures **0.038–0.041 ms** per
    /// completion and **0.042–0.043 ms** for the burst. The whole gap was the
    /// profile.
    ///
    /// A budget asserted in a profile it was not derived in is not a stricter
    /// gate, it is a different question with the same number on it, so the
    /// assertions do not run there. **The measurements still do**: every row
    /// above this call is printed in both profiles, tagged with
    /// [`profile`], because a debug number is a useful thing to look at and a
    /// useless thing to fail on. What is skipped, and why, and what the
    /// release figure is, is printed too — a silent skip would be the drift
    /// this exists to prevent, one level up.
    pub(crate) fn budgets_are_assertable(gate: &str, release_figure: &str) -> bool {
        if !cfg!(debug_assertions) {
            return true;
        }
        println!("{}", budget_skip_notice(gate, release_figure));
        false
    }

    /// The line [`budgets_are_assertable`] prints when it declines — named so
    /// it can be asserted rather than only read (E141's pin: the gate names
    /// its profile, and the figure it is not asserting, in its reason).
    pub(crate) fn budget_skip_notice(gate: &str, release_figure: &str) -> String {
        format!(
            "E141 {{\"section\":\"budget_skip\",\"gate\":\"{gate}\",\"profile\":\"{}\",\
             \"load\":\"{}\",\"reason\":\"{gate}'s budgets are RELEASE figures \
             ({release_figure}); this run is a {} build, so the rows above are informational \
             only and nothing is asserted. Re-run with --release to assert them: cargo nextest \
             run --release -p vilan-lsp --run-ignored all -E 'test(budget)'\"}}",
            profile(),
            loadavg_1m(),
            profile(),
        )
    }

    /// **The generated exhibit** (§6.1, Q6). A module of `functions` functions
    /// of one shape over a shared wrapper — lucide's shape, not lucide's
    /// content, and nothing copied from anyone's checkout.
    pub(crate) fn exhibit_module(functions: usize) -> String {
        let mut text = String::from(
            "// GENERATED by E121's keystroke-path gate: a synthetic module of one\n\
             // repeated shape, sized like kolt-with-lucide. Nothing here is copied\n\
             // from any application; the bodies are mechanical.\n\n\
             fun frame(seed: i32): i32 {\n\tseed * 2 + 1\n}\n\n",
        );
        for index in 0..functions {
            text.push_str(&format!(
                "/// synthetic entry {index}\n\
                 fun icon_{index:04}(): i32 {{\n\
                 \tlet base = frame({});\n\
                 \tbase + {}\n\
                 }}\n\n",
                index % 20,
                index % 24,
            ));
        }
        text
    }

    /// The app-shaped consumer: a small file that calls four of the module's
    /// functions, held **fixed** across every subject size. Holding the file
    /// fixed while the program grows around it is what isolates codebase size
    /// from file size — §1.4's method, and the reason the paper's 6.4× means
    /// anything.
    pub(crate) const EXHIBIT_ENTRY: &str = "import pkg::table::{ icon_0000, icon_0001, icon_0002, icon_0003 };\n\n\
         fun caption(prefix: str): str {\n\tlet rendered = prefix;\n\trendered\n}\n\n\
         fun panel(): i32 {\n\
         \tlet first = icon_0000();\n\
         \tlet second = icon_0001();\n\
         \tlet third = icon_0002();\n\
         \tlet fourth = icon_0003();\n\
         \tfirst + second + third + fourth\n}\n\n\
         fun main() {\n\tlet total = panel();\n\tlet label = caption(\"panel\");\n}\n";

    /// Write the exhibit to a fresh directory and land one analysis on the
    /// entry. Returns the directory (the caller removes it), the document, and
    /// how long the analysis took — recorded, never asserted.
    fn land(functions: usize) -> (std::path::PathBuf, Document, Duration) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("vilan_e121_gate_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the exhibit directory");
        std::fs::write(directory.join("table.vl"), exhibit_module(functions))
            .expect("write the generated module");
        let entry = directory.join("main.vl");
        std::fs::write(&entry, EXHIBIT_ENTRY).expect("write the exhibit entry");
        let started = Instant::now();
        let document = Document::analyze(EXHIBIT_ENTRY, &std_root(), &entry);
        (directory, document, started.elapsed())
    }

    /// One keystroke on the exhibit entry: a character typed inside a function
    /// BODY, which is the shape the anchor and the stamp are built for.
    pub(crate) fn one_keystroke() -> String {
        EXHIBIT_ENTRY.replace("\tlet total = panel();", "\tlet totals = panel();")
    }

    // ── E126: the VIEW-SHAPED exhibit ──────────────────────────────────────
    //
    // The arithmetic exhibit above meets the keystroke budget honestly, and
    // E121 built it for exactly that. It is the WRONG subject for the
    // diagnostics budget: its bodies are `let base = frame(seed); base + k`
    // over `i32`, so the analysis they drive records **104** method-call
    // substitutions and costs 50 ms of warm CPU — against an application's
    // 18,745 and 965 ms (per-module-analysis-reuse.md §1.6). A gate asserted
    // on it goes green on a program that never had the problem.
    //
    // So this second generator emits the shape that produces the cost: every
    // function returns a `View` built through a CHAINED builder, over a
    // generic-heavy std surface (`SignalCell` / `Source` / `combine` / `map`
    // / the `Slot`-bounded `.child`), with one imported module the size of an
    // icon set and a band of components written in element syntax. Nothing is
    // copied from any application — the bodies are mechanical, the icon
    // geometry is arithmetic on the index, and the only thing taken from a
    // real program is the four RECORDED COUNTS below.

    /// **The application shape this exhibit is built to track**, recorded once.
    ///
    /// Measured on the dev machine (16 cores, WSL2) on **2026-09-04**, release
    /// profile, 1-minute load average **31–33**, by a throwaway P3/P5-shaped
    /// probe — five in-process `Document::analyze` calls on one browser entry
    /// of a real vilan application, a distinct trailing comment each time,
    /// `CLOCK_PROCESS_CPUTIME_ID` around each — reading `Program`'s own tables
    /// afterwards. The application is READ-ONLY EVIDENCE and never enters this
    /// tree: these four integers are the whole of what crossed over, and the
    /// numbers agree with per-module-analysis-reuse.md §1.5/§1.6 (which
    /// recorded 97,070 entities, 18,540 substitutions and 965 ms of warm CPU
    /// against `next` at 635e3728, one order earlier).
    ///
    /// They are a TARGET, not a budget: [`the_view_exhibit_tracks_the_recorded_application_shape`]
    /// asserts the generated exhibit stays within 2× of them in both
    /// directions, which is the property that makes [`diagnostics_budget`]'s
    /// verdict mean something. A generator change that drifts outside the band
    /// reds that pin before it can quietly re-open §1.6's hole.
    pub(crate) const RECORDED_ENTITIES: usize = 99_522;
    /// See [`RECORDED_ENTITIES`] — `Program::method_call_substitution.len()`.
    pub(crate) const RECORDED_SUBSTITUTIONS: usize = 18_745;
    /// See [`RECORDED_ENTITIES`] — `Program::implementations.len()`.
    pub(crate) const RECORDED_IMPLEMENTATIONS: usize = 433;
    /// See [`RECORDED_ENTITIES`] — warm process CPU per keystroke, the minimum
    /// of five (median 1,057 ms). This is the number [`diagnostics_budget`] is
    /// asserted against a 500 ms mandate for.
    pub(crate) const RECORDED_WARM_CPU_MS: f64 = 903.0;
    /// The band the exhibit must track the recording within, each way.
    pub(crate) const TRACKING_FACTOR: f64 = 2.0;

    /// Children chained onto each generated icon — an icon set's own median.
    const ICON_CHILDREN: usize = 4;
    /// Components written in element syntax, each one a builder chain over the
    /// icon module and the reactive surface.
    const VIEW_COMPONENTS: usize = 40;

    /// What an analysis of the exhibit is SHAPED like — the counts the
    /// tracking pin compares, read off the landed program's own tables.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct Census {
        /// Entity ids minted across every source — the `source_ranges` spans
        /// summed, which is the census §1.6's "entities" column is.
        pub(crate) entities: usize,
        /// `method_call_substitution.len()` — §1.6's 178× miss, and the one
        /// count that says whether the bodies are chained builder calls or
        /// arithmetic.
        pub(crate) substitutions: usize,
        pub(crate) implementations: usize,
    }

    pub(crate) fn census(document: &Document) -> Census {
        document
            .program
            .as_ref()
            .map(|program| Census {
                entities: program
                    .source_ranges
                    .iter()
                    .map(|range| (range.end - range.start) as usize)
                    .sum(),
                substitutions: program.method_call_substitution.len(),
                implementations: program.implementations.len(),
            })
            .unwrap_or_default()
    }

    /// One child element of a generated icon: an svg primitive whose geometry
    /// is arithmetic on `(index, child)`, so the module is deterministic and
    /// nothing about it came from anyone's checkout.
    fn icon_child(index: usize, child: usize) -> String {
        let a = (index + child) % 20 + 2;
        let b = (index + 2 * child) % 18 + 2;
        let c = (index + 3 * child) % 16 + 1;
        let d = (index + 5 * child) % 14 + 1;
        let e = (index + 7 * child) % 12 + 1;
        match (index + child) % 6 {
            0 => format!("path d(\"M{a} {b}h{c}l{d} {e}\")"),
            1 => format!("path d(\"M{b} {a}v{c}\")"),
            2 => format!("circle cx(\"{a}\") cy(\"{b}\") r(\"{c}\")"),
            3 => format!("rect x(\"{a}\") y(\"{b}\") width(\"{c}\") height(\"{d}\") rx(\"{e}\")"),
            4 => format!("path d(\"m{a} {b} {c}.{d} {e}\")"),
            _ => format!("line x1(\"{a}\") y1(\"{b}\") x2(\"{c}\") y2(\"{d}\")"),
        }
    }

    /// The icon module — `icons` functions, each returning a `View` through a
    /// chain of `ICON_CHILDREN` `.child(..)` calls over one shared frame. This
    /// is the module that carries the exhibit's substitution count: `.child`
    /// is `fun child<C: Slot>(self, content: C)`, so every link in every chain
    /// is a generic method call the analysis has to substitute.
    pub(crate) fn view_icons_module(icons: usize) -> String {
        let mut text = String::from(
            "// GENERATED by E126's diagnostics gate: a synthetic icon module of one\n\
             // repeated VIEW shape, sized like an application's icon set. Nothing here\n\
             // is copied from any application; the geometry is arithmetic on the index.\n\n\
             import std::ui::{ View, view };\n\n\
             fun icon_frame(): View {\n\
             \t<svg\n\
             \t\twidth(\"24\")\n\
             \t\theight(\"24\")\n\
             \t\tviewBox(\"0 0 24 24\")\n\
             \t\tfill(\"none\")\n\
             \t\tstroke(\"currentColor\")\n\
             \t\tstroke-width(\"2\")\n\
             \t\tstroke-linecap(\"round\")\n\
             \t\tstroke-linejoin(\"round\")\n\
             \t/>\n\
             }\n\n",
        );
        for index in 0..icons {
            text.push_str(&format!(
                "/// generated icon {index}\nfun icon_{index:04}(): View {{\n\ticon_frame()\n"
            ));
            for child in 0..ICON_CHILDREN {
                text.push_str(&format!("\t\t.child(<{} />)\n", icon_child(index, child)));
            }
            text.push_str("}\n\n");
        }
        text
    }

    /// The generic-heavy state surface: a `SignalCell`-holding struct with a
    /// type parameter, `combine(..).map(..)` through the reactive std, and two
    /// functions bounded on `Source<T>`. Fixed — it is the same file at every
    /// exhibit size, which is what isolates codebase size from file size.
    pub(crate) const VIEW_STATE_MODULE: &str = "\
// GENERATED by E126's diagnostics gate.
import std::reactive::{ Source, combine };

/// A row of app state, held the way an application holds it.
struct Panel<T> {
\ttitle: SignalCell<str>,
\tcount: SignalCell<i32>,
\tpayload: SignalCell<T>,
}

impl Panel<type T> {
\tfun new(title: str, payload: T): Panel<T> {
\t\tPanel {
\t\t\ttitle = Signal::new(title),
\t\t\tcount = Signal::new(0),
\t\t\tpayload = Signal::new(payload),
\t\t}
\t}

\tfun label(self): SignalCell<str> {
\t\tcombine((self.title, self.count)).map(|pair| i\"{pair.0} ({pair.1})\")
\t}

\tfun bump(self) {
\t\tself.count.set(self.count.get() + 1);
\t}
}

fun render_label<S: Source<str>>(source: S): View {
\t<span .child(source.get()) />
}

fun tally<T: Source<i32>>(source: T, offset: i32): i32 {
\tsource.get() + offset
}
";

    /// The component band: `components` functions in element syntax, each one
    /// a styled builder chain that reaches the icon module, the reactive
    /// surface and an inherent `impl` on a std type — an application's own
    /// view file, mechanically.
    pub(crate) fn view_components_module(components: usize, icons: usize) -> String {
        let mut text = String::from(
            "// GENERATED by E126's diagnostics gate.\n\
             import std::reactive::{ Source, combine };\n\
             import pkg::icons;\n\
             import pkg::state::{ Panel, render_label, tally };\n\n\
             impl style::Style {\n\
             \tfun when(self, enabled: bool, modifier: style::Style) {\n\
             \t\tif enabled {\n\t\t\tself + modifier\n\t\t} else {\n\t\t\tself\n\t\t}\n\t}\n\n\
             \tfun flex_row(self) {\n\
             \t\tself.display(style::Display::Flex).flex_direction(style::FlexDirection::Row)\n\
             \t}\n}\n\n\
             let row_style = const style::style()\n\
             \t.padding(style::Length::rem(1))\n\
             \t.radius(style::Length::rem(1))\n\
             \t.flex_row();\n\n\
             fun chip(label: str, selected: bool): View {\n\
             \t<span .styled(row_style.when(selected, const style::style().opacity(0.5)))>\n\
             \t\t{label}\n\
             \t</span>\n}\n\n",
        );
        for index in 0..components {
            let selected = if index % 2 == 0 { "true" } else { "false" };
            let chipped = if index % 3 == 0 { "false" } else { "true" };
            text.push_str(&format!(
                "fun toolbar_{index:03}(panel: Panel<i32>): View {{\n\
                 \t<div .styled(row_style.when({selected}, const style::style().opacity(0.{})))>\n\
                 \t\t<button on:click(|_| {{ panel.bump(); }})>\n\
                 \t\t\t{{icons::icon_{:04}()}}\n\
                 \t\t</button>\n\
                 \t\t{{render_label(panel.label())}}\n\
                 \t\t{{chip(\"row {index}\", {chipped})}}\n\
                 \t\t<span .child(i\"{{tally(panel.count, {})}}\") />\n\
                 \t</div>\n}}\n\n",
                index % 9 + 1,
                index % icons.max(1),
                index % 7,
            ));
        }
        text
    }

    /// The app-shaped entry, held FIXED across every exhibit size for the
    /// reason [`EXHIBIT_ENTRY`] is: growing the program around an unchanged
    /// file is what separates codebase size from file size.
    pub(crate) const VIEW_ENTRY: &str = "\
// GENERATED by E126's diagnostics gate.
import pkg::components;
import pkg::icons;
import pkg::state::{ Panel, tally };

fun main() {
\tlet panel = Panel::new(\"panel\", 0);
\tlet slot_0 = components::toolbar_000(panel);
\tlet slot_1 = components::toolbar_001(panel);
\tlet slot_2 = components::toolbar_002(panel);
\tlet slot_3 = components::toolbar_003(panel);
\tlet badge = icons::icon_0000();
\tlet total = tally(panel.count, 1);
\tui::mount_root(\"app\", || <div .child(slot_0) .child(badge) />);
}
";

    /// One keystroke on the view entry: a character typed inside `main`'s
    /// BODY, so the shape stamp does not move and the verdict stays `Exact`.
    pub(crate) fn view_keystroke() -> String {
        VIEW_ENTRY.replace("\tlet total = tally(", "\tlet totals = tally(")
    }

    /// Write the view exhibit to a fresh package on disk and land one analysis
    /// on its entry.
    ///
    /// Unlike the arithmetic exhibit this one carries a `vilan.toml`: the
    /// shape under measurement is a BROWSER program written against the web
    /// prelude, and both facts live in the manifest. Without it there is no
    /// `View`, no `style`, no `ui` — and no diagnostics path worth gating.
    fn land_view(icons: usize) -> (std::path::PathBuf, Document, Duration) {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("vilan_e126_gate_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        let source = directory.join("src");
        std::fs::create_dir_all(&source).expect("create the exhibit directory");
        std::fs::write(
            directory.join("vilan.toml"),
            "# GENERATED by E126's diagnostics gate.\n\
             [package]\nname = \"exhibit\"\ndefault-entry = \"main\"\nprelude = \"std::web\"\n\n\
             [entry.main]\ntarget = \"browser\"\n",
        )
        .expect("write the exhibit manifest");
        std::fs::write(source.join("icons.vl"), view_icons_module(icons))
            .expect("write the icon module");
        std::fs::write(source.join("state.vl"), VIEW_STATE_MODULE).expect("write the state module");
        std::fs::write(
            source.join("components.vl"),
            view_components_module(VIEW_COMPONENTS, icons),
        )
        .expect("write the component module");
        let entry = source.join("main.vl");
        std::fs::write(&entry, VIEW_ENTRY).expect("write the exhibit entry");
        let started = Instant::now();
        let document = Document::analyze(VIEW_ENTRY, &std_root(), &entry);
        (directory, document, started.elapsed())
    }

    /// Which exhibit a gate body is running on.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) enum Subject {
        /// E121's original: `functions` functions of `i32` arithmetic. The
        /// keystroke path's subject, and honest for it.
        Arithmetic,
        /// E126's: `functions` `View`-returning functions through a chained
        /// builder, plus the component band and the reactive surface. The
        /// diagnostics path's subject.
        View,
    }

    impl Subject {
        /// The tag the machine-readable rows carry.
        fn tag(self) -> &'static str {
            match self {
                Subject::Arithmetic => "syn",
                Subject::View => "view",
            }
        }

        fn land(self, functions: usize) -> (std::path::PathBuf, Document, Duration) {
            match self {
                Subject::Arithmetic => land(functions),
                Subject::View => land_view(functions),
            }
        }

        fn keystroke(self) -> String {
            match self {
                Subject::Arithmetic => one_keystroke(),
                Subject::View => view_keystroke(),
            }
        }

        /// A bare scope position inside the edited body — where a completion
        /// request lands mid-word, and the arm that carries auto-import.
        fn cursor_needle(self) -> &'static str {
            match self {
                Subject::Arithmetic => "panel();",
                Subject::View => "tally(panel.count",
            }
        }
    }

    /// Run `work` `REPETITIONS` times and return the per-call thread CPU and
    /// wall time.
    fn per_call(mut work: impl FnMut()) -> (Option<f64>, f64) {
        let cpu_started = thread_cpu_now();
        let wall_started = Instant::now();
        for _ in 0..REPETITIONS {
            work();
        }
        let wall = wall_started.elapsed().as_secs_f64() * 1000.0 / REPETITIONS as f64;
        let cpu = cpu_started.zip(thread_cpu_now()).map(|(before, after)| {
            after.saturating_sub(before).as_secs_f64() * 1000.0 / REPETITIONS as f64
        });
        (cpu, wall)
    }

    /// One machine-readable row, the shape `perf_baseline`'s `PERF` lines have.
    fn row(subject: &str, request: &str, cpu: Option<f64>, wall: f64, count: usize) {
        println!(
            "E121 {{\"section\":\"keystroke_path\",\"subject\":\"{subject}\",\
             \"request\":\"{request}\",\"profile\":\"{}\",\"load\":\"{}\",\
             \"reps\":{REPETITIONS},\"cpu_ms\":{},\"wall_ms\":{wall:.3},\"count\":{count}}}",
            profile(),
            loadavg_1m(),
            cpu.map_or_else(|| "null".to_string(), |value| format!("{value:.3}")),
        );
    }

    /// The gate's body, over whatever subject size it is given.
    ///
    /// `assert_budget` is false for the smoke subject: a 24-function exhibit
    /// says nothing about a 1,791-function budget, and pretending otherwise
    /// would be the vacuous green M12 taught this tree to refuse.
    fn keystroke_path_budget_at(subject: Subject, functions: usize, assert_budget: bool) {
        let tag = subject.tag();
        let (directory, mut document, analysis) = subject.land(functions);
        let landed_tokens = document.keystroke_tokens(false).len();
        assert!(
            landed_tokens > 0,
            "the exhibit produced no tokens — the fixture is not analyzing, so nothing below \
             measures the keystroke path (diagnostics: {:?})",
            document
                .diagnostics
                .iter()
                .map(|e| &e.msg)
                .take(3)
                .collect::<Vec<_>>(),
        );
        println!(
            "E121 {{\"section\":\"keystroke_path\",\"subject\":\"{tag}{functions}\",\
             \"request\":\"land\",\"profile\":\"{}\",\"load\":\"{}\",\"reps\":1,\
             \"cpu_ms\":null,\"wall_ms\":{:.1},\"count\":{landed_tokens}}}",
            profile(),
            loadavg_1m(),
            analysis.as_secs_f64() * 1000.0,
        );

        // The regression the gate exists to catch, measured in the same
        // process on the same subject: the whole-program walk §2.1 removes.
        let (baseline_cpu, baseline_wall) = per_call(|| {
            std::hint::black_box(document.semantic_tokens());
        });
        row(tag, "landed_walk", baseline_cpu, baseline_wall, functions);

        // One keystroke, then the burst an editor fires.
        let edited = subject.keystroke();
        document.set_text(&edited);
        let offset = edited
            .find(subject.cursor_needle())
            .expect("a scope position");

        let (tokens_cpu, tokens_wall) = per_call(|| {
            std::hint::black_box(document.keystroke_tokens(false));
        });
        let count = document.keystroke_tokens(false).len();
        row(tag, "semanticTokens", tokens_cpu, tokens_wall, count);

        let (hints_cpu, hints_wall) = per_call(|| {
            std::hint::black_box(document.keystroke_hints(false));
        });
        row(
            tag,
            "inlayHint",
            hints_cpu,
            hints_wall,
            document.keystroke_hints(false).len(),
        );

        let (completion_cpu, completion_wall) = per_call(|| {
            std::hint::black_box(document.keystroke_completion(offset, false));
        });
        row(
            tag,
            "completion",
            completion_cpu,
            completion_wall,
            document.keystroke_completion(offset, false).len(),
        );

        // §1.2 measured that the server answers a burst SERIALLY, so the sum
        // is what the editor experiences — that is the number to gate on.
        let (burst_cpu, burst_wall) = per_call(|| {
            std::hint::black_box(document.keystroke_tokens(false));
            std::hint::black_box(document.keystroke_hints(false));
            std::hint::black_box(document.keystroke_completion(offset, false));
        });
        row(tag, "burst", burst_cpu, burst_wall, 3);
        let _ = std::fs::remove_dir_all(&directory);

        if !assert_budget {
            return;
        }
        // E141: the budgets below are release figures. Under `debug_assertions`
        // the rows above stand as informational and nothing is asserted.
        if !budgets_are_assertable(
            "the keystroke path",
            "completion 0.038-0.041 ms and the burst 0.042-0.043 ms in release on a quiet box, against 0.2 ms and 10 ms",
        ) {
            return;
        }
        let Some(burst_cpu) = burst_cpu else {
            panic!(
                "no thread CPU clock on this host, so the gate cannot assert anything \
                 load-proof (M15); wall was {burst_wall:.3} ms at loadavg {}",
                loadavg_1m()
            );
        };
        for (request, cpu, budget) in [
            ("semanticTokens", tokens_cpu, BUDGET_MS),
            ("inlayHint", hints_cpu, BUDGET_MS),
            ("completion", completion_cpu, COMPLETION_BUDGET_MS),
        ] {
            let cpu = cpu.expect("the clock answered for the burst, so it answered here");
            assert!(
                cpu < budget,
                "{request} cost {cpu:.3} ms of thread CPU per request on the {tag} exhibit at \
                 {functions} functions, over the {budget} ms budget (loadavg {}, the \
                 whole-program walk it replaces cost {} ms)",
                loadavg_1m(),
                baseline_cpu.map_or_else(|| "?".to_string(), |value| format!("{value:.3}")),
            );
        }
        assert!(
            burst_cpu < BUDGET_MS,
            "the five-provider burst cost {burst_cpu:.3} ms of thread CPU on the {tag} exhibit \
             at {functions} functions, over the {BUDGET_MS} ms budget — and the burst is what \
             the editor experiences, because §1.2 measured that the server answers one serially \
             (loadavg {})",
            loadavg_1m(),
        );
        // Non-vacuity, on this run and this machine: the walk the path
        // replaced is the planted regression, and it must be visibly worse.
        if let Some(baseline_cpu) = baseline_cpu {
            assert!(
                baseline_cpu > burst_cpu,
                "the whole-program walk ({baseline_cpu:.3} ms) did not cost more than the \
                 keystroke path ({burst_cpu:.3} ms) — the instrument cannot tell the two apart, \
                 so the green above proves nothing",
            );
        }
    }

    /// M25: the candidates the captured completion index serves are the ones
    /// DERIVING it per request would serve — the same candidates, in the same
    /// order, with the same import edits — at every shape of position.
    ///
    /// This is the property the tranche's whole speed-up rests on, and the only
    /// one it can break. Before M25 the engine derived two tables inside every
    /// request: `auto_import_completions` swept every `std`/`pkg` child
    /// module's `name_to_id_map`, classified each name and sorted the result,
    /// and an import path's origin arm called `modules_in_root` — a `read_dir`
    /// per source root. Both are functions of the analyzed program and the
    /// package tree it resolved, so both moved to the analysis that produced
    /// them (`Document::capture_landed`). What could go wrong is not the speed
    /// but the ANSWER: an order that ranks differently, a cap that truncates
    /// somewhere else, a listing that lost a module.
    ///
    /// So the reference is the old mechanism itself.
    /// `keystroke_completion_rebuilding_index` derives the index at request
    /// time, which is exactly what the engine used to do, and the two answers
    /// must be one answer. The subject is the generated exhibit — a real
    /// package on disk, analyzed, with a keystroke typed into a function body
    /// so the verdict is `Exact` and every arm is live.
    ///
    /// Ordering is asserted, not just membership: [`AUTO_IMPORT_COMPLETION_CAP`]
    /// caps the popup at 20, so *which* 20 survive is a ranking question, and a
    /// pin that compared sets would pass while the cap kept the wrong ones.
    #[test]
    fn the_captured_index_serves_what_deriving_it_per_request_would() {
        let (directory, mut document, _) = land(SMOKE_FUNCTIONS);
        // A word typed into a function BODY: the declaration shape does not
        // move, so the verdict is `Exact` and every arm below answers under
        // the capture rather than degrading to syntax. This buffer PARSES,
        // which the auto-import arm requires — its edits come from one parse
        // of the live text, and a buffer that does not parse has no safe
        // import edit to offer at all.
        let live = EXHIBIT_ENTRY.replace(
            "\tfirst + second + third + fourth\n",
            "\tlet ic = 0;\n\tfirst + second + third + fourth\n",
        );
        document.set_text(&live);
        assert_eq!(
            document.keystroke_verdict(false),
            crate::keystroke::Verdict::Exact,
            "the corpus below is about the captured index, not about degrading",
        );

        let at = |text: &str, needle: &str| {
            text.find(needle).unwrap_or_else(|| panic!("{needle}")) + needle.len()
        };
        let corpus = [
            // A bare scope position — the arm that carries auto-import.
            ("scope", at(&live, "\tlet ic = ")),
            ("scope-prefix", at(&live, "\tlet i")),
            // An import path, at its origin and past a module of it — the arm
            // that used to `read_dir` per request.
            ("import-origin", at(&live, "import pkg::")),
            ("import-module", at(&live, "import pkg::table")),
        ];

        let mut saw_import_edit = false;
        for (label, offset) in corpus {
            let served = document.keystroke_completion(offset, false);
            let derived = document.keystroke_completion_rebuilding_index(offset, false);
            saw_import_edit |= served.iter().any(|item| item.needs_import.is_some());
            assert_eq!(
                render_candidates(&served),
                render_candidates(&derived),
                "at the {label} position the captured index answered differently \
                 from deriving it per request",
            );
        }
        // Non-vacuous: the corpus reaches the arms it claims to. A scope
        // position that offered no auto-import candidate would compare two
        // empty tables and prove nothing about the order.
        assert!(
            saw_import_edit,
            "no position in the corpus offered an auto-import candidate, so the \
             ordered table was never read",
        );
        let origin = document.keystroke_completion(at(&live, "import pkg::"), false);
        assert!(
            origin.iter().any(|item| item.label == "table"),
            "the import-origin position must reach the exhibit's own module \
             through the captured listing — got {:?}",
            origin.iter().map(|item| &item.label).collect::<Vec<_>>(),
        );

        // The member arm, which needs a `.` the buffer cannot parse around —
        // its own phase, because an unparseable buffer suppresses the
        // auto-import edits the phase above is about.
        let member = live.replace("\tlet ic = 0;\n", "\tlet ic = 0;\n\tfirst.\n");
        document.set_text(&member);
        let offset = at(&member, "\tfirst.");
        let served = document.keystroke_completion(offset, false);
        assert!(
            !served.is_empty(),
            "the member position must offer the receiver's members, or the \
             equality below is vacuous",
        );
        assert_eq!(
            render_candidates(&served),
            render_candidates(&document.keystroke_completion_rebuilding_index(offset, false)),
            "at the member position the captured index answered differently \
             from deriving it per request",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// M29: the same identity property at MEMBER positions, over the shapes the
    /// per-type member table now serves — and with the import edits the
    /// captured table carries asserted equal to the live parse's.
    ///
    /// Two things moved onto the analysis in M29 and both can only break the
    /// ANSWER, never the speed. Member completion used to derive a type's
    /// surface per request by walking every `Implementation` the program holds
    /// (and then every trait and supertrait each provides); it now reads a
    /// table built once. Every auto-import candidate's edit used to be computed
    /// against a fresh parse of the LIVE buffer; it is now computed against the
    /// analyzed text when the index is built and re-mapped through the edit
    /// anchor. So the reference is the old mechanism itself, exactly as M25's
    /// pin above uses it: derive the index at request time and require one
    /// answer — and, for the edits, require the captured one to equal what
    /// parsing the buffer in the request would have produced, which is a
    /// question the anchor makes meaningful only because the two texts agree
    /// here.
    ///
    /// Its subject is its OWN small exhibit rather than M25's: the member arm
    /// needs a nominal receiver with fields, methods and an inherited trait
    /// default, and `EXHIBIT_ENTRY` is deliberately a file of `i32`s.
    #[test]
    fn the_captured_tables_serve_member_positions_and_import_edits_identically() {
        const ENTRY: &str = "import pkg::table::{ icon_0000 };\n\n\
             struct Panel { width: i32 }\n\
             impl Panel {\n\
             \tfun new(): Panel { Panel { width = icon_0000() } }\n\
             \tfun grow(self): Panel { Panel { width = self.width + 1 } }\n\
             }\n\n\
             fun main() {\n\tlet p = Panel::new();\n\tlet total = p.width;\n}\n";
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("vilan_m29_pin_{}_{unique}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create the exhibit directory");
        std::fs::write(directory.join("table.vl"), exhibit_module(4))
            .expect("write the generated module");
        let entry = directory.join("main.vl");
        std::fs::write(&entry, ENTRY).expect("write the entry");
        let mut document = Document::analyze(ENTRY, &std_root(), &entry);
        assert!(
            document.program.is_some(),
            "the fixture must analyze, or every comparison below is empty",
        );

        // A member position on each shape the live receiver walk types: a bare
        // binding, a static call, a method call on a call. Written into a
        // function BODY so the declaration shape does not move and the verdict
        // stays Exact.
        let mut answered = 0usize;
        for (label, receiver) in [
            ("binding receiver", "p"),
            ("static-call receiver", "Panel::new()"),
            ("method-call receiver", "p.grow()"),
        ] {
            // ONE dangling `.` at a time, which is what a buffer mid-keystroke
            // actually holds: three of them in one file would make each line's
            // receiver read as a member of the line above it, since a chain
            // written down the page is exactly that shape.
            let live = ENTRY.replace(
                "\tlet total = p.width;\n",
                &format!("\t{receiver}.\n\tlet total = p.width;\n"),
            );
            document.set_text(&live);
            assert_eq!(
                document.keystroke_verdict(false),
                crate::keystroke::Verdict::Exact,
                "the {label} corpus row is about the captured tables, not about degrading",
            );
            let offset = live
                .find(&format!("\t{receiver}.\n"))
                .expect("the inserted member position")
                + receiver.len()
                + 2;
            let served = document.keystroke_completion(offset, false);
            answered += usize::from(
                served.iter().any(|item| item.label == "grow")
                    && served.iter().any(|item| item.label == "width"),
            );
            assert_eq!(
                render_candidates(&served),
                render_candidates(&document.keystroke_completion_rebuilding_index(offset, false)),
                "at the {label} the captured member table answered differently from \
                 deriving it per request",
            );
        }
        assert_eq!(
            answered, 3,
            "every member position in the corpus must offer the receiver's own \
             `width` and `grow`, or the equalities above compare lists that say \
             nothing about the member table",
        );

        // The import edits: the buffer the request serves IS the analyzed text
        // here, so the captured edit and a live re-parse must agree byte for
        // byte, span included.
        document.set_text(ENTRY);
        let scope =
            ENTRY.find("\tlet total = ").expect("a scope position") + "\tlet total = ".len();
        let served = document.keystroke_completion(scope, false);
        let mut checked = 0usize;
        for candidate in &served {
            let Some(import) = candidate.needs_import.as_ref() else {
                continue;
            };
            let path: Vec<&str> = import.module_path.iter().map(String::as_str).collect();
            let live_edit = vilan_core::formatter::insert_import(ENTRY, &path, &candidate.label)
                .unwrap_or_else(|| panic!("no live edit for {}", candidate.label));
            assert_eq!(
                (import.edit_span, import.edit_replacement.as_str()),
                (live_edit.span, live_edit.replacement.as_str()),
                "the captured import edit for `{}` differs from parsing the buffer \
                 in the request",
                candidate.label,
            );
            checked += 1;
        }
        assert!(
            checked > 0,
            "no auto-import candidate carried an edit, so the equality above \
             compared nothing",
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// Every field of a candidate a client can see, as one comparable line —
    /// the label, its icon, its detail, its call shape, and the whole
    /// auto-import edit it carries.
    fn render_candidates(items: &[vilan_ide::Completion]) -> Vec<String> {
        items
            .iter()
            .map(|item| {
                let import = item.needs_import.as_ref().map(|import| {
                    (
                        import.module_path.join("::"),
                        import.edit_span.start,
                        import.edit_span.end,
                        import.edit_replacement.as_str(),
                        import.origin_tier,
                    )
                });
                format!(
                    "{}|{:?}|{:?}|{:?}|{import:?}",
                    item.label, item.kind, item.detail, item.call_parameters
                )
            })
            .collect()
    }

    /// §6.2's `keystroke_path_budget`, on the exhibit at kolt's size.
    #[test]
    #[ignore = "E121/E141: the keystroke-path gate — a generated 1,791-function exhibit, minutes of analysis; its budgets are RELEASE figures and are not asserted under debug_assertions (the rows still print). Run deliberately, in release (proposal/editor-latency.md §6)"]
    fn keystroke_path_budget() {
        keystroke_path_budget_at(Subject::Arithmetic, GATE_FUNCTIONS, true);
    }

    /// The seconds-long smoke pin the PR gate DOES pay: the harness builds its
    /// exhibit, lands an analysis, drives every provider through the keystroke
    /// path and produces rows. It asserts the mechanism, not the budget.
    #[test]
    fn keystroke_path_gate_smoke() {
        keystroke_path_budget_at(Subject::Arithmetic, SMOKE_FUNCTIONS, false);
    }

    // --- E141: a budget is a figure IN A PROFILE ----------------------------

    /// The profile guard itself, in whichever profile this binary was built
    /// in — the one pin in this module that costs nothing and runs on every
    /// PR, precisely because what it guards is a gate PR CI never runs.
    #[test]
    fn e141_a_budget_is_only_asserted_in_the_profile_it_was_derived_in() {
        assert_eq!(
            profile(),
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        );
        assert_eq!(
            budgets_are_assertable("the keystroke path", "0.2 ms"),
            !cfg!(debug_assertions),
            "a release build asserts its budgets; a debug build declines and says so",
        );
    }

    /// E141's own pin: the decline NAMES the profile it is declining in, and
    /// the release figure it is not asserting. A silent skip would be the
    /// drift this guard exists to prevent, one level up — two Order 27 lanes
    /// read a debug run's 0.705-0.813 ms completion as a 4x regression in the
    /// tranche, and the number was the profile.
    #[test]
    fn e141_the_skipped_gate_names_its_profile_and_the_release_figure() {
        let notice = budget_skip_notice(
            "the keystroke path",
            "completion 0.038-0.041 ms in release on a quiet box, against 0.2 ms",
        );
        assert!(notice.contains("\"profile\":\"debug\"") || !cfg!(debug_assertions));
        assert!(notice.contains("RELEASE figures"), "{notice}");
        assert!(notice.contains("0.038-0.041 ms"), "{notice}");
        assert!(notice.contains("against 0.2 ms"), "{notice}");
        assert!(notice.contains("--release"), "{notice}");
        assert!(
            notice.contains(&format!("this run is a {} build", profile())),
            "{notice}"
        );
    }

    /// §6.2's `diagnostics_budget`, **re-anchored on E126's view-shaped
    /// exhibit**. CPU-clocked, which makes it **debounce-exclusive by
    /// construction** (Q3): the debounce is a `tokio::time::sleep` and accrues
    /// no CPU, so a CPU assertion cannot include it.
    ///
    /// **It is RED, and that is the honest verdict.** One warm keystroke costs
    /// **1,053 ms** of process CPU on the view exhibit at [`GATE_FUNCTIONS`]
    /// icons — measured 2026-09-04, release, loadavg 121, with the exhibit's
    /// own census on the same run (104,430 entities, 26,000 substitutions) —
    /// against the mandate's 500 ms. That is the same order as the
    /// [`RECORDED_WARM_CPU_MS`] an application pays, which is the whole point
    /// of the subject swap. On the arithmetic exhibit this pin used to run on,
    /// the same measurement is ~50 ms and the gate went GREEN on a program
    /// that never had the problem (per-module-analysis-reuse.md §1.6: 104
    /// substitutions against an application's 18,745).
    ///
    /// The path to green is **M19 tranche 1** — the per-module analysis reuse
    /// this exhibit exists to gate. §1.5 measured that 83% of a warm
    /// keystroke's analyzer CPU re-analyzes ONE module whose content did not
    /// change; not doing that work is what gets under 500 ms, and no tuning of
    /// the work does. Until it lands the pin stays `#[ignore]`d with its
    /// measured number in the reason, which is the tree's rule for a gate that
    /// is right and red.
    ///
    /// It is also expensive: building and analyzing a 1,791-icon package is
    /// minutes of work in a debug suite. Both gates are run deliberately,
    /// together, by the command in this module's doc.
    #[test]
    #[ignore = "E121/E126: the diagnostics gate — RED and honest. One warm keystroke costs 1,053 ms of process CPU on the view-shaped exhibit against a 500 ms mandate (E126, 2026-09-04, release, loadavg 121); M19 tranche 1's per-module reuse is the path to green. Minutes of analysis; run deliberately (proposal/editor-latency.md §6)"]
    fn diagnostics_budget() {
        let (directory, _document, _) = land_view(GATE_FUNCTIONS);
        let entry = directory.join("src").join("main.vl");
        let edited = view_keystroke();
        let cpu_started = process_cpu_now();
        let wall_started = Instant::now();
        let analyzed = Document::analyze(&edited, &std_root(), &entry);
        let wall = wall_started.elapsed().as_secs_f64() * 1000.0;
        let cpu = cpu_started
            .zip(process_cpu_now())
            .map(|(before, after)| after.saturating_sub(before).as_secs_f64() * 1000.0);
        let count = analyzed.diagnostics.len();
        let shape = census(&analyzed);
        row("view", "publishDiagnostics", cpu, wall, count);
        // The subject's shape, on the same run that produced the number above:
        // a budget verdict is only worth reading beside the census that says
        // WHICH program it was taken on (§1.6's whole finding).
        println!(
            "E126 {{\"section\":\"diagnostics\",\"subject\":\"view{GATE_FUNCTIONS}\",\
             \"entities\":{},\"substitutions\":{},\"implementations\":{},\
             \"recorded_entities\":{RECORDED_ENTITIES},\
             \"recorded_substitutions\":{RECORDED_SUBSTITUTIONS}}}",
            shape.entities, shape.substitutions, shape.implementations,
        );
        assert!(
            analyzed.program.is_some(),
            "the view exhibit did not analyze, so the CPU below measures a failed \
             parse rather than the diagnostics path (diagnostics: {:?})",
            analyzed
                .diagnostics
                .iter()
                .map(|error| &error.msg)
                .take(3)
                .collect::<Vec<_>>(),
        );
        let _ = std::fs::remove_dir_all(&directory);
        // E141: 500 ms is a release figure, and the debug profile's number is
        // a different question. The row above is printed either way.
        if !budgets_are_assertable(
            "the diagnostics path",
            "500 ms of process CPU per keystroke, measured in release",
        ) {
            return;
        }
        let Some(cpu) = cpu else {
            panic!(
                "no process CPU clock on this host, so the gate cannot assert anything \
                 load-proof (M15); wall was {wall:.0} ms at loadavg {}",
                loadavg_1m(),
            );
        };
        assert!(
            cpu < 500.0,
            "one keystroke took {cpu:.0} ms of CPU to diagnostics on the view-shaped \
             {GATE_FUNCTIONS}-icon exhibit ({} entities, {} method-call substitutions), over the \
             500 ms budget (loadavg {}, {wall:.0} ms of wall). The subject is honest: a real \
             application of this shape pays {RECORDED_WARM_CPU_MS:.0} ms for the same keystroke, \
             and E121's arithmetic exhibit paid 50. The debounce is excluded by construction — it \
             is not in this span at all. M19 tranche 1 is the path: §1.5 measured 83% of this \
             going to ONE module whose content did not change",
            shape.entities,
            shape.substitutions,
            loadavg_1m(),
        );
    }

    /// **E126's own pin: the exhibit is the right SUBJECT.**
    ///
    /// [`diagnostics_budget`]'s verdict is only worth what its subject is
    /// worth, and §1.6 is the record of a gate that went green because its
    /// subject was 178× too easy. So the subject is asserted, against the four
    /// [`RECORDED_ENTITIES`] counts, in BOTH directions: an exhibit that
    /// drifts easier re-opens §1.6's hole, and one that drifts harder turns
    /// the budget into a number about the generator rather than about an
    /// application.
    ///
    /// `#[ignore]`d for its cost, not its verdict — it lands a full-size
    /// analysis, which is the same minutes the two budget gates pay.
    #[test]
    #[ignore = "E126: the exhibit-shape gate — a full-size view exhibit, minutes of analysis; run deliberately (proposal/editor-latency.md §6)"]
    fn the_view_exhibit_tracks_the_recorded_application_shape() {
        let (directory, document, analysis) = land_view(GATE_FUNCTIONS);
        let shape = census(&document);
        println!(
            "E126 {{\"section\":\"exhibit_shape\",\"subject\":\"view{GATE_FUNCTIONS}\",\
             \"profile\":\"{}\",\"load\":\"{}\",\"entities\":{},\"substitutions\":{},\
             \"implementations\":{},\"land_ms\":{:.0}}}",
            profile(),
            loadavg_1m(),
            shape.entities,
            shape.substitutions,
            shape.implementations,
            analysis.as_secs_f64() * 1000.0,
        );
        let diagnostics = document
            .diagnostics
            .iter()
            .map(|error| &error.msg)
            .take(3)
            .collect::<Vec<_>>();
        let _ = std::fs::remove_dir_all(&directory);
        assert!(
            diagnostics.is_empty(),
            "the view exhibit must analyze CLEAN — a program that does not type-check \
             stops walking bodies partway and its census means nothing: {diagnostics:?}",
        );
        for (label, measured, recorded) in [
            ("entities", shape.entities, RECORDED_ENTITIES),
            (
                "method-call substitutions",
                shape.substitutions,
                RECORDED_SUBSTITUTIONS,
            ),
            (
                "implementations",
                shape.implementations,
                RECORDED_IMPLEMENTATIONS,
            ),
        ] {
            let ratio = measured as f64 / recorded as f64;
            assert!(
                (1.0 / TRACKING_FACTOR..=TRACKING_FACTOR).contains(&ratio),
                "the exhibit's {label} came to {measured} against the recorded {recorded} \
                 ({ratio:.2}×), outside the {TRACKING_FACTOR}× band — the generated subject no \
                 longer tracks the application shape `diagnostics_budget` is asserted for \
                 (per-module-analysis-reuse.md §1.6)",
            );
        }
    }

    /// The generator is a GENERATOR: same input, same bytes, every time, and
    /// the shape it claims to emit is in the bytes.
    ///
    /// Cheap — no analysis — so the PR gate pays it. It is what catches a
    /// generator edit that quietly stops emitting chains (the §1.6 failure)
    /// without waiting for the minutes-long shape pin above.
    #[test]
    fn the_view_exhibit_generator_is_deterministic_and_view_shaped() {
        let icons = 64;
        assert_eq!(
            view_icons_module(icons),
            view_icons_module(icons),
            "the icon module is not deterministic, so no two runs of the gate share a subject",
        );
        assert_eq!(
            view_components_module(VIEW_COMPONENTS, icons),
            view_components_module(VIEW_COMPONENTS, icons),
            "the component module is not deterministic",
        );
        let module = view_icons_module(icons);
        assert_eq!(
            module.matches("(): View {").count(),
            icons + 1,
            "the icon module must declare one `View`-returning function per icon plus the \
             shared frame",
        );
        assert_eq!(
            module.matches(".child(<").count(),
            icons * ICON_CHILDREN,
            "every icon must chain {ICON_CHILDREN} `.child(..)` calls — the chain IS the \
             substitution count §1.6's finding is about, and an icon module of bare returns \
             is the exhibit that let the diagnostics gate go green",
        );
        let components = view_components_module(VIEW_COMPONENTS, icons);
        assert!(
            components.matches("icons::icon_").count() >= VIEW_COMPONENTS,
            "every component must reach the icon module, or the icons are dead weight the \
             entry never colours",
        );
        assert!(
            VIEW_STATE_MODULE.contains("combine((self.title, self.count)).map(")
                && VIEW_STATE_MODULE.contains("fun render_label<S: Source<str>>"),
            "the state module carries the generic std surface — `combine`, `map` and a \
             `Source`-bounded parameter — and without it the exhibit's generic instantiation \
             is the icon chain alone",
        );
    }

    /// The view exhibit's keystroke half: the mandate's per-request budgets, on
    /// the diagnostics subject.
    ///
    /// E121's gate asserts them on the arithmetic exhibit, and they hold there.
    /// They have to hold HERE too, or the keystroke path's claim is a claim
    /// about `i32` programs: the path serves a landed snapshot, and the
    /// snapshot of a 1,791-icon view program is the one an application has.
    #[test]
    #[ignore = "E121/E126/E141: the keystroke-path gate on the view exhibit — a generated 1,791-icon package, minutes of analysis; its budgets are RELEASE figures and are not asserted under debug_assertions (the rows still print). Run deliberately, in release (proposal/editor-latency.md §6)"]
    fn keystroke_path_budget_view() {
        keystroke_path_budget_at(Subject::View, GATE_FUNCTIONS, true);
    }

    /// The seconds-long smoke the PR gate DOES pay on the view subject: the
    /// generator writes a real package, it analyzes, every provider answers
    /// through the keystroke path. Mechanism, not budget — a 24-icon exhibit
    /// says nothing about a 1,791-icon one.
    #[test]
    fn keystroke_path_gate_smoke_view() {
        keystroke_path_budget_at(Subject::View, SMOKE_FUNCTIONS, false);
    }
}
