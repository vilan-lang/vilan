//! The source formatter behind `vilan fmt`: it reparses a file and reprints the
//! AST in canonical style (tab indentation, normalized spacing and blank lines),
//! reattaching the comments the lexer drops as trivia.
//!
//! Safety: reprinting from the AST could, given a bug, silently change a program.
//! So `format` re-lexes its own output and checks the token stream matches the
//! input's (ignoring spans, whitespace, and comments); on any mismatch it returns
//! the source unchanged rather than risk corrupting the file.

use std::cell::Cell;

use crate::node::{
    BinaryOp, Convention, ExternBinding, Func, GenericArguments, GenericParameters, ImportBranch,
    Node, NodeIfBranch, NodeList, Pattern, StructInitializerField,
};
use crate::span::{Span, Spanned};
use crate::token::Token;

thread_local! {
    /// How many whole-buffer parses ([`parse`]) this thread has paid so far —
    /// the formatter's unit of real work, and what made a bare scope
    /// completion cost ~20 member completions before E83 (`insert_import`
    /// re-parsed the buffer per auto-import candidate,
    /// `proposals/proposal/playground-completion.md` §9).
    static BUFFER_PARSES: Cell<u64> = const { Cell::new(0) };
}

/// The number of whole-buffer parses this thread's formatter has performed —
/// an instrumentation probe (E83), not a behavior surface. Monotonic: read a
/// snapshot before and after the work under test and assert on the
/// difference, the way the E23 leak tally is read. The pins that hold a
/// completion request to ONE buffer parse (however many auto-import
/// candidates it shapes) are what this exists for.
pub fn buffer_parse_count() -> u64 {
    BUFFER_PARSES.with(Cell::get)
}

/// Extracts `//` line comments from `source` as `(span, text)` in source order.
/// `text` keeps the leading `//` and is trimmed of trailing whitespace. String
/// literals are skipped so a `//` inside a string isn't taken for a comment.
pub fn extract_comments(source: &str) -> Vec<(Span, &str)> {
    let bytes = source.as_bytes();
    let mut comments = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b'"' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                let start = index;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                let text = source[start..index].trim_end();
                comments.push((Span::new((), start..start + text.len()), text));
            }
            _ => index += 1,
        }
    }
    comments
}

/// The lexer's token stream with spans stripped — the formatter's notion of "the
/// same code", used to check a reprint didn't change anything but trivia.
fn code_tokens(source: &str) -> Option<Vec<Token<'_>>> {
    let (tokens, lex_errors) = crate::lexing::tokenize(source);
    lex_errors
        .is_empty()
        .then(|| tokens.into_iter().map(|(token, _)| token).collect())
}

/// The formatter's token-level canonicalization, used to check a reprint changed
/// nothing but trivia and the three canonical orders. Four order-insensitivities
/// are folded in so the safety check accepts them: insignificant trailing commas
/// (dropped), the canonical ordering of a top-level import run (see the
/// canonical-import-order section below), the canonical ordering of a `style()`
/// builder chain's links (see the canonical-style-chain-order section), and the
/// canonical ordering of a `css` block's items (see the canonical-css-block-order
/// section). Everything else must match token for token, so the net still catches
/// every *other* reordering.
///
/// The css pass runs LAST so that a `style()` chain inside a hole is already
/// canonical when a block's items are permuted around it — the block scan then
/// moves whole, already-canonical items.
fn normalize(tokens: Vec<Token<'_>>) -> Vec<Token<'_>> {
    sort_css_blocks(sort_style_chains(sort_import_runs(&drop_trailing_commas(
        tokens,
    ))))
}

/// Drops every comma that sits immediately before a closing `}`, `)`, or `]`.
/// Vilan treats such a trailing comma as insignificant (tuples need two or more
/// elements, so there is no `(a,)` one-tuple to confuse it with), which lets the
/// safety check accept the formatter normalizing trailing commas in or out.
fn drop_trailing_commas(tokens: Vec<Token<'_>>) -> Vec<Token<'_>> {
    let mut result: Vec<Token<'_>> = Vec::with_capacity(tokens.len());
    for token in tokens {
        if matches!(
            token,
            Token::Ctrl('}') | Token::Ctrl(')') | Token::Ctrl(']')
        ) {
            while let Some(Token::Ctrl(',')) = result.last() {
                result.pop();
            }
        }
        result.push(token);
    }
    result
}

// --- Canonical import order --------------------------------------------------
//
// `vilan fmt` canonicalizes the order of a file's top-level `import`/`use`
// statements (the pruning of unused imports is the editor's job, not the
// formatter's). The rule, defined once here and applied by both the printer
// (which reorders AST items) and `normalize` (which reorders token statements)
// through the shared [`import_sort_key`], is:
//
//   * A *run* is a maximal span of consecutive top-level import/use statements.
//     Blank lines between them do not break a run — they coalesce, and the run
//     reprints as one block. A standalone (own-line) comment *does* break a run
//     (it pins a deliberate grouping), so imports never reorder across it; a
//     trailing same-line comment travels with its own import.
//   * Within a run, statements sort by: kind (`import` before `use`; an
//     `export import`/`export use` re-export sorts as a plain import/use — the
//     `export` prefix does not change grouping), then root namespace (`std`
//     first, dependency packages alphabetically, `pkg` last), then the full
//     `::`-separated path compared case-sensitively segment by segment.
//   * A brace-set import (`import std::x::{ b, a }`) sorts its inner branch list
//     the same way (`{ a, b }`), recursively.
//   * Only *top-level* runs are touched. Block-scoped imports (inside
//     `fn`/`impl`/`mod` bodies — backlog H2) are deliberate placements and are
//     left exactly as written, order and brace sets both.
//
// `normalize` applies the same canonicalization to *both* sides of the safety
// check, so the check passes whatever order the printer emits (its job is only
// to confirm no import was dropped or corrupted and no *other* code moved),
// while the printer's own tested logic is what fixes the visible order.

/// `import` vs `use` for the canonical order — imports sort before uses. The
/// `export` re-export prefix does not participate.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ImportKind {
    Import,
    Use,
}

/// The root-namespace rank: `std` first, then dependency packages ordered by
/// name, then `pkg` (the current package), then a bare brace-set import with no
/// leading namespace (`import { a, b }`, rare) last.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum RootRank {
    Std,
    Dependency(String),
    Pkg,
    Unrooted,
}

/// A `::`-separated import path reduced to an order-only comparable form: names
/// compare case-sensitively segment by segment, a shorter path sorts before a
/// longer one extending it (`a` before `a::b`, via `End` < `Path`), and a brace
/// set's branches are pre-sorted so the whole set compares canonically.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum BranchKey {
    End,
    Path(String, Box<BranchKey>),
    Set(Vec<BranchKey>),
}

/// The full sort key for one top-level import/use statement — the single
/// definition of the canonical order, shared by the printer and `normalize` so
/// the two cannot disagree. Ordered by kind, then root namespace, then path.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ImportSortKey {
    kind: ImportKind,
    root: RootRank,
    rest: BranchKey,
}

/// A parsed import path — `ImportBranch` without the source spans. Both the AST
/// (via [`branch_from_ast`]) and the token stream (via [`parse_token_branch`])
/// reduce to this shape, from which the shared key and the canonical token
/// re-emission are derived.
enum TokenBranch<'src> {
    Path(&'src str, Option<Box<TokenBranch<'src>>>),
    Set(Vec<TokenBranch<'src>>),
}

/// Drops the spans from an `ImportBranch`, giving the span-free [`TokenBranch`]
/// the shared key operates on.
fn branch_from_ast<'src>(branch: &ImportBranch<'src>) -> TokenBranch<'src> {
    match branch {
        ImportBranch::Path(name, _, child) => TokenBranch::Path(
            name,
            child.as_ref().map(|child| Box::new(branch_from_ast(child))),
        ),
        ImportBranch::Set(branches) => {
            TokenBranch::Set(branches.iter().map(branch_from_ast).collect())
        }
    }
}

/// The canonical view of a branch for ordering and re-emission: a one-member
/// brace set whose member is not `self` IS its member — `a::{ b }` and `a::b`
/// spell the same import (kolt.local 005) — so the shared key and the token
/// re-emission both see through the braces. Without this, the braced source
/// and its collapsed reprint would sort a run differently (`BranchKey::Set`
/// orders after `Path`) and the safety net would refuse the reprint. A lone
/// `self` keeps its set: it only means something inside braces.
fn unwrap_singleton_set<'branch, 'src>(
    branch: &'branch TokenBranch<'src>,
) -> &'branch TokenBranch<'src> {
    let mut current = branch;
    while let TokenBranch::Set(branches) = current {
        match branches.as_slice() {
            [only @ TokenBranch::Path(name, _)] if *name != "self" => current = only,
            _ => break,
        }
    }
    current
}

/// The order key for one import path — brace sets are sorted internally so equal
/// paths, whatever their source branch order, produce equal keys, and a
/// one-member set keys as its member ([`unwrap_singleton_set`]).
fn branch_key(branch: &TokenBranch<'_>) -> BranchKey {
    match unwrap_singleton_set(branch) {
        TokenBranch::Path(name, None) => {
            BranchKey::Path((*name).to_string(), Box::new(BranchKey::End))
        }
        TokenBranch::Path(name, Some(child)) => {
            BranchKey::Path((*name).to_string(), Box::new(branch_key(child)))
        }
        TokenBranch::Set(branches) => {
            let mut keys: Vec<BranchKey> = branches.iter().map(branch_key).collect();
            keys.sort();
            BranchKey::Set(keys)
        }
    }
}

/// The canonical sort key for an import/use of `kind` importing `branch`:
/// root-namespace rank first, then the path after the root. A one-member set
/// ranks as its member ([`unwrap_singleton_set`]), so `import { a };` and its
/// collapsed reprint `import a;` land in the same place.
fn import_sort_key(kind: ImportKind, branch: &TokenBranch<'_>) -> ImportSortKey {
    let (root, rest) = match unwrap_singleton_set(branch) {
        TokenBranch::Path(name, child) => {
            let root = match *name {
                "std" => RootRank::Std,
                "pkg" => RootRank::Pkg,
                other => RootRank::Dependency(other.to_string()),
            };
            let rest = match child {
                Some(child) => branch_key(child),
                None => BranchKey::End,
            };
            (root, rest)
        }
        TokenBranch::Set(_) => (RootRank::Unrooted, branch_key(branch)),
    };
    ImportSortKey { kind, root, rest }
}

/// If `node` is an import-like item — `import`/`use`, or an `export import` /
/// `export use` re-export — returns its kind and imported path (the `export`
/// prefix does not change the kind). `None` for any other item, which breaks a
/// run.
fn import_kind_and_branch<'node, 'src>(
    node: &'node Node<'src>,
) -> Option<(ImportKind, &'node ImportBranch<'src>)> {
    match node {
        Node::Import(branch) => Some((ImportKind::Import, branch)),
        Node::Use(branch) => Some((ImportKind::Use, branch)),
        Node::Export(inner) => import_kind_and_branch(&inner.0),
        _ => None,
    }
}

/// The canonical key of an import-like `node` (panics if it is not one — callers
/// gate on [`import_kind_and_branch`] first).
fn node_import_key(node: &Node<'_>) -> ImportSortKey {
    let (kind, branch) =
        import_kind_and_branch(node).expect("node_import_key on a non-import item");
    import_sort_key(kind, &branch_from_ast(branch))
}

/// Whether the tokens at `index` begin a top-level import/use statement —
/// `import …`, `use …`, or `export import …` / `export use …`.
fn starts_import(tokens: &[Token<'_>], index: usize) -> bool {
    match tokens.get(index) {
        Some(Token::Import | Token::Use) => true,
        Some(Token::Export) => {
            matches!(tokens.get(index + 1), Some(Token::Import | Token::Use))
        }
        _ => false,
    }
}

/// The path-segment name at `index`, mirroring the parser's `eat_name` (an
/// identifier, or the `true`/`false` literals treated as names).
fn token_name<'src>(tokens: &[Token<'src>], index: usize) -> Option<&'src str> {
    match tokens.get(index) {
        Some(&Token::Ident(name)) => Some(name),
        Some(&Token::Bool(true)) => Some("true"),
        Some(&Token::Bool(false)) => Some("false"),
        _ => None,
    }
}

/// Parses the `::`-separated import path beginning at `index` (mirroring the
/// parser's `parse_namespace_path`: a name-headed path is tried before a brace
/// set), returning the branch and the index just past it, or `None` if the
/// tokens do not match the import-path grammar.
fn parse_token_branch<'src>(
    tokens: &[Token<'src>],
    index: usize,
) -> Option<(TokenBranch<'src>, usize)> {
    if let Some(name) = token_name(tokens, index) {
        let mut next = index + 1;
        let continuation = if tokens.get(next) == Some(&Token::Op("::")) {
            let (child, after) = parse_token_branch(tokens, next + 1)?;
            next = after;
            Some(Box::new(child))
        } else {
            None
        };
        Some((TokenBranch::Path(name, continuation), next))
    } else if tokens.get(index) == Some(&Token::Ctrl('{')) {
        let mut branches = Vec::new();
        let mut next = index + 1;
        // An empty set `{}` closes immediately; otherwise each element is a
        // name-headed single path, comma-separated, allow-trailing.
        while tokens.get(next) != Some(&Token::Ctrl('}')) {
            let name = token_name(tokens, next)?;
            let mut after = next + 1;
            let continuation = if tokens.get(after) == Some(&Token::Op("::")) {
                let (child, past) = parse_token_branch(tokens, after + 1)?;
                after = past;
                Some(Box::new(child))
            } else {
                None
            };
            branches.push(TokenBranch::Path(name, continuation));
            next = after;
            match tokens.get(next) {
                Some(Token::Ctrl(',')) => next += 1,
                Some(Token::Ctrl('}')) => break,
                _ => return None,
            }
        }
        Some((TokenBranch::Set(branches), next + 1))
    } else {
        None
    }
}

/// Parses one import/use statement beginning at `index` into its kind, whether
/// it is an `export` re-export, its path, and the index past its `;` — or `None`
/// if the tokens do not match the import grammar (leaving the run unsorted, a
/// safe no-op). Callers gate on [`starts_import`] first.
fn parse_import_statement<'src>(
    tokens: &[Token<'src>],
    index: usize,
) -> Option<(ImportKind, bool, TokenBranch<'src>, usize)> {
    let mut next = index;
    let export = tokens.get(next) == Some(&Token::Export);
    if export {
        next += 1;
    }
    let kind = match tokens.get(next) {
        Some(Token::Import) => ImportKind::Import,
        Some(Token::Use) => ImportKind::Use,
        _ => return None,
    };
    next += 1;
    let (branch, after) = parse_token_branch(tokens, next)?;
    next = after;
    if tokens.get(next) != Some(&Token::Ctrl(';')) {
        return None;
    }
    Some((kind, export, branch, next + 1))
}

/// Appends the canonical token form of an import path, brace sets sorted and a
/// one-member set collapsed to its member ([`unwrap_singleton_set`], mirroring
/// `print_import_branch` — kolt.local 005): the safety net must reduce the
/// braced source and the collapsed reprint to the same canonical tokens.
fn emit_branch_tokens<'src>(branch: &TokenBranch<'src>, out: &mut Vec<Token<'src>>) {
    match unwrap_singleton_set(branch) {
        TokenBranch::Path(name, child) => {
            out.push(Token::Ident(name));
            if let Some(child) = child {
                out.push(Token::Op("::"));
                emit_branch_tokens(child, out);
            }
        }
        TokenBranch::Set(branches) => {
            out.push(Token::Ctrl('{'));
            let mut order: Vec<&TokenBranch<'src>> = branches.iter().collect();
            order.sort_by_cached_key(|branch| branch_key(branch));
            for (position, child) in order.iter().enumerate() {
                if position > 0 {
                    out.push(Token::Ctrl(','));
                }
                emit_branch_tokens(child, out);
            }
            out.push(Token::Ctrl('}'));
        }
    }
}

/// Reorders each contiguous run of top-level (brace-depth-zero) import/use
/// statements into the canonical order, re-emitting each in a canonical token
/// form (brace sets sorted) so that a source run and the printer's reordered
/// reprint reduce to the same token sequence. Statements inside a block
/// (`fn`/`impl`/`mod` bodies — brace depth ≥ 1) and every non-import token keep
/// their positions, so the safety net still catches every other reordering.
// `pub` (doc-hidden) only so the external corpus tripwire in
// `tests/parse_differential.rs` mirrors the net's import canonicalization through
// this ONE implementation rather than a divergent copy — the "cannot disagree"
// guarantee. Not part of the supported API.
#[doc(hidden)]
pub fn sort_import_runs<'src>(tokens: &[Token<'src>]) -> Vec<Token<'src>> {
    let mut result = Vec::with_capacity(tokens.len());
    let mut depth: i32 = 0;
    let mut index = 0;
    while index < tokens.len() {
        if depth == 0 && starts_import(tokens, index) {
            // Parse the maximal run of consecutive import statements. Each
            // statement consumes its own brace set, so depth stays 0 across it.
            let mut statements: Vec<(ImportSortKey, ImportKind, bool, TokenBranch<'src>)> =
                Vec::new();
            let mut cursor = index;
            let mut parsed_cleanly = true;
            while cursor < tokens.len() && starts_import(tokens, cursor) {
                match parse_import_statement(tokens, cursor) {
                    Some((kind, export, branch, next)) => {
                        let key = import_sort_key(kind, &branch);
                        statements.push((key, kind, export, branch));
                        cursor = next;
                    }
                    None => {
                        parsed_cleanly = false;
                        break;
                    }
                }
            }
            if parsed_cleanly && !statements.is_empty() {
                statements.sort_by(|left, right| left.0.cmp(&right.0));
                for (_, kind, export, branch) in &statements {
                    if *export {
                        result.push(Token::Export);
                    }
                    result.push(match kind {
                        ImportKind::Import => Token::Import,
                        ImportKind::Use => Token::Use,
                    });
                    emit_branch_tokens(branch, &mut result);
                    result.push(Token::Ctrl(';'));
                }
                index = cursor;
                continue;
            }
            // A parse failure (never expected for a cleanly-parsed source) falls
            // through to the raw passthrough below — a safe no-op.
        }
        match &tokens[index] {
            Token::Ctrl('{') | Token::Ctrl('(') | Token::Ctrl('[') => depth += 1,
            Token::Ctrl('}') | Token::Ctrl(')') | Token::Ctrl(']') => depth -= 1,
            _ => {}
        }
        result.push(tokens[index].clone());
        index += 1;
    }
    result
}

// --- Canonical style-chain order ---------------------------------------------
//
// `vilan fmt` canonicalizes the order of the `.name(…)` links in a `style()`
// builder chain (kolt.local 006). The order is Tailwind CSS's category sequence
// — layout, flexbox/grid, spacing, sizing, typography, backgrounds, borders,
// effects, filters, tables, transitions/animation, transforms, interactivity,
// svg, accessibility — with every CONDITION method after every property method,
// in the axis order the selector nests them (media → relation → attribute →
// pseudo): the same shape as Tailwind's plugin putting variant groups last.
//
// Two rules keep the reorder SEMANTICS-preserving, which is not optional: a
// chain merges last-wins per property slot (`vilan/std/src/style.vl`).
//
//   * A method the table does not know is a BARRIER — a user `impl Style`
//     extension (kolt's `paint_primary` writes colour AND background), or one
//     of the escape hatches whose slot is an argument rather than a name
//     (`raw`, `with_length`, `with_color`, `with_border`, `rule`). Links sort
//     only within the runs BETWEEN barriers, and a barrier holds its position
//     absolutely, so no known method can cross one. That is correct with zero
//     knowledge of user code, and it degrades gracefully: an all-custom chain
//     is left exactly as written.
//   * Methods whose slots are ENTANGLED share a FAMILY and never move relative
//     to each other — the same property (`line_height` / `line_height_length`),
//     or a CSS shorthand over it (`padding` over `padding_x` over
//     `padding_left`, `size` over `width` and `height`, `border` over
//     `border_color`). `padding` then `padding_x` means something and the
//     reverse means something else (`proposal/ui-styling.md` §0bis), so the
//     sort key is the FAMILY's rank, never the method's, and the sort is
//     stable. Only genuinely independent slots ever cross.
//
// What the reorder cannot change is the emitted stylesheet. `Style::rule` emits
// its atomic rule at the call, and that rule's text — including its
// content-hashed class name — is a function of the slot key and the declaration
// alone, never of the link's position in the chain. So the emitted CSS is
// byte-identical across any permutation, and the surviving slot map is
// identical across a permutation that respects the two rules above.
// `crates/vilan-cli/tests/style_chain_order.rs` proves both over a corpus, by
// building each chain in source and in sorted order and diffing the CSS.
//
// `Style + Style` operands are deliberately out of scope: that merge's order is
// semantic, and only the links inside one `style()` builder sort.

/// Tailwind CSS's category sequence, in order — the canonical group order for a
/// style chain. Categories with no `Style` method yet are kept so that a method
/// added later lands in the right place rather than at the end.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum StyleCategory {
    Layout,
    FlexboxGrid,
    Spacing,
    Sizing,
    Typography,
    Backgrounds,
    Borders,
    Effects,
    Filters,
    Tables,
    TransitionsAnimation,
    Transforms,
    Interactivity,
    Svg,
    Accessibility,
}

/// The four condition axes, in the order the selector nests them (and therefore
/// the order the condition combinators require at the call site — see
/// `render_rule` in `vilan/std/src/style.vl`). `Relation` is the axis
/// `within`/`children`/`divide` write (ui-styling.md §0bis.6) — it holds the
/// grammar seat the deleted `dark` held.
#[doc(hidden)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ConditionAxis {
    Media,
    Relation,
    Attribute,
    Pseudo,
}

/// One row of the canonical order table: a `Style` property method, the
/// Tailwind category it belongs to, the slot FAMILY it shares with every method
/// whose slots are entangled with its own, and the CSS properties it writes.
///
/// `properties` is not used by the sort — it is what lets
/// `crates/vilan-core/tests/style_table_sync.rs` CHECK the `family` column
/// against `style.vl`'s own shorthand table rather than trust it, instead of
/// taking the column's word for it.
#[doc(hidden)]
pub struct StyleMethod {
    pub name: &'static str,
    pub category: StyleCategory,
    pub family: &'static str,
    pub properties: &'static [&'static str],
}

/// The canonical order table. Row order IS the canonical order: rows are grouped
/// by [`StyleCategory`] in Tailwind's category sequence, and within a category
/// they follow Tailwind's own property sequence. A family's rows are contiguous,
/// and the family sorts at its FIRST row's position.
///
/// Every `fun name(self, …)` in `style.vl`'s `impl Style` appears here, in
/// [`STYLE_CONDITION_METHODS`], or in [`STYLE_BARRIER_METHODS`] — gated by
/// `crates/vilan-cli/tests/style_table_sync.rs`, so a new style method is a red
/// test rather than a silently unsorted link.
#[doc(hidden)]
#[rustfmt::skip]
pub const STYLE_PROPERTY_METHODS: &[StyleMethod] = &[
    // --- layout ---
    StyleMethod { name: "display",               category: StyleCategory::Layout,               family: "display",               properties: &["display"] },
    StyleMethod { name: "overflow",              category: StyleCategory::Layout,               family: "overflow",              properties: &["overflow"] },
    StyleMethod { name: "position",              category: StyleCategory::Layout,               family: "position",              properties: &["position"] },
    // `inset` is the four offsets' shorthand (`family_longhands`), so the five
    // sort as one unit at `inset`'s position.
    StyleMethod { name: "inset",                 category: StyleCategory::Layout,               family: "inset",                 properties: &["inset"] },
    StyleMethod { name: "top",                   category: StyleCategory::Layout,               family: "inset",                 properties: &["top"] },
    StyleMethod { name: "right",                 category: StyleCategory::Layout,               family: "inset",                 properties: &["right"] },
    StyleMethod { name: "bottom",                category: StyleCategory::Layout,               family: "inset",                 properties: &["bottom"] },
    StyleMethod { name: "left",                  category: StyleCategory::Layout,               family: "inset",                 properties: &["left"] },
    // --- flexbox & grid ---
    // `flex-direction` shares a prefix with the `flex` shorthand and is NOT
    // covered by it (`family_longhands` is deliberately not prefix-based), so
    // it is its own family and may cross `flex`.
    StyleMethod { name: "flex_direction",        category: StyleCategory::FlexboxGrid,          family: "flex-direction",        properties: &["flex-direction"] },
    StyleMethod { name: "flex",                  category: StyleCategory::FlexboxGrid,          family: "flex",                  properties: &["flex"] },
    StyleMethod { name: "flex_shrink",           category: StyleCategory::FlexboxGrid,          family: "flex",                  properties: &["flex-shrink"] },
    StyleMethod { name: "grid_template_columns", category: StyleCategory::FlexboxGrid,          family: "grid-template-columns", properties: &["grid-template-columns"] },
    StyleMethod { name: "gap",                   category: StyleCategory::FlexboxGrid,          family: "gap",                   properties: &["gap"] },
    StyleMethod { name: "justify_content",       category: StyleCategory::FlexboxGrid,          family: "justify-content",       properties: &["justify-content"] },
    StyleMethod { name: "align_items",           category: StyleCategory::FlexboxGrid,          family: "align-items",           properties: &["align-items"] },
    // --- spacing ---
    StyleMethod { name: "padding",               category: StyleCategory::Spacing,              family: "padding",               properties: &["padding"] },
    StyleMethod { name: "padding_x",             category: StyleCategory::Spacing,              family: "padding",               properties: &["padding-left", "padding-right"] },
    StyleMethod { name: "padding_y",             category: StyleCategory::Spacing,              family: "padding",               properties: &["padding-top", "padding-bottom"] },
    StyleMethod { name: "padding_top",           category: StyleCategory::Spacing,              family: "padding",               properties: &["padding-top"] },
    StyleMethod { name: "padding_right",         category: StyleCategory::Spacing,              family: "padding",               properties: &["padding-right"] },
    StyleMethod { name: "padding_bottom",        category: StyleCategory::Spacing,              family: "padding",               properties: &["padding-bottom"] },
    StyleMethod { name: "padding_left",          category: StyleCategory::Spacing,              family: "padding",               properties: &["padding-left"] },
    StyleMethod { name: "margin",                category: StyleCategory::Spacing,              family: "margin",                properties: &["margin"] },
    StyleMethod { name: "margin_x",              category: StyleCategory::Spacing,              family: "margin",                properties: &["margin-left", "margin-right"] },
    StyleMethod { name: "margin_y",              category: StyleCategory::Spacing,              family: "margin",                properties: &["margin-top", "margin-bottom"] },
    StyleMethod { name: "margin_top",            category: StyleCategory::Spacing,              family: "margin",                properties: &["margin-top"] },
    StyleMethod { name: "margin_right",          category: StyleCategory::Spacing,              family: "margin",                properties: &["margin-right"] },
    StyleMethod { name: "margin_bottom",         category: StyleCategory::Spacing,              family: "margin",                properties: &["margin-bottom"] },
    StyleMethod { name: "margin_left",           category: StyleCategory::Spacing,              family: "margin",                properties: &["margin-left"] },
    // --- sizing ---
    // `size` writes the same two slots `width` and `height` write, so the three
    // are one family — Tailwind's width-before-height order survives inside it
    // only where the source already had it.
    StyleMethod { name: "width",                 category: StyleCategory::Sizing,               family: "size",                  properties: &["width"] },
    StyleMethod { name: "height",                category: StyleCategory::Sizing,               family: "size",                  properties: &["height"] },
    StyleMethod { name: "size",                  category: StyleCategory::Sizing,               family: "size",                  properties: &["width", "height"] },
    StyleMethod { name: "min_width",             category: StyleCategory::Sizing,               family: "min-width",             properties: &["min-width"] },
    StyleMethod { name: "max_width",             category: StyleCategory::Sizing,               family: "max-width",             properties: &["max-width"] },
    StyleMethod { name: "min_height",            category: StyleCategory::Sizing,               family: "min-height",            properties: &["min-height"] },
    StyleMethod { name: "max_height",            category: StyleCategory::Sizing,               family: "max-height",            properties: &["max-height"] },
    // --- typography ---
    StyleMethod { name: "font_family",           category: StyleCategory::Typography,           family: "font-family",           properties: &["font-family"] },
    StyleMethod { name: "font_size",             category: StyleCategory::Typography,           family: "font-size",             properties: &["font-size"] },
    StyleMethod { name: "font_weight",           category: StyleCategory::Typography,           family: "font-weight",           properties: &["font-weight"] },
    StyleMethod { name: "letter_spacing",        category: StyleCategory::Typography,           family: "letter-spacing",        properties: &["letter-spacing"] },
    StyleMethod { name: "line_height",           category: StyleCategory::Typography,           family: "line-height",           properties: &["line-height"] },
    StyleMethod { name: "line_height_length",    category: StyleCategory::Typography,           family: "line-height",           properties: &["line-height"] },
    StyleMethod { name: "text_align",            category: StyleCategory::Typography,           family: "text-align",            properties: &["text-align"] },
    StyleMethod { name: "color",                 category: StyleCategory::Typography,           family: "color",                 properties: &["color"] },
    StyleMethod { name: "text_decoration",       category: StyleCategory::Typography,           family: "text-decoration",       properties: &["text-decoration"] },
    StyleMethod { name: "white_space",           category: StyleCategory::Typography,           family: "white-space",           properties: &["white-space"] },
    // --- backgrounds ---
    StyleMethod { name: "background",            category: StyleCategory::Backgrounds,          family: "background-color",      properties: &["background-color"] },
    StyleMethod { name: "background_gradient",   category: StyleCategory::Backgrounds,          family: "background-image",      properties: &["background-image"] },
    StyleMethod { name: "background_image",      category: StyleCategory::Backgrounds,          family: "background-image",      properties: &["background-image"] },
    StyleMethod { name: "background_size",       category: StyleCategory::Backgrounds,          family: "background-size",       properties: &["background-size"] },
    // --- borders ---
    // `border-radius` is NOT covered by the `border` shorthand, so it is its own
    // family; `border_color` IS covered by it, so it is not.
    StyleMethod { name: "radius",                category: StyleCategory::Borders,              family: "border-radius",         properties: &["border-radius"] },
    StyleMethod { name: "border",                category: StyleCategory::Borders,              family: "border",                properties: &["border"] },
    StyleMethod { name: "border_none",           category: StyleCategory::Borders,              family: "border",                properties: &["border"] },
    StyleMethod { name: "border_top",            category: StyleCategory::Borders,              family: "border",                properties: &["border-top"] },
    StyleMethod { name: "border_right",          category: StyleCategory::Borders,              family: "border",                properties: &["border-right"] },
    StyleMethod { name: "border_bottom",         category: StyleCategory::Borders,              family: "border",                properties: &["border-bottom"] },
    StyleMethod { name: "border_left",           category: StyleCategory::Borders,              family: "border",                properties: &["border-left"] },
    StyleMethod { name: "border_color",          category: StyleCategory::Borders,              family: "border",                properties: &["border-color"] },
    // --- effects ---
    StyleMethod { name: "box_shadow",            category: StyleCategory::Effects,              family: "box-shadow",            properties: &["box-shadow"] },
    StyleMethod { name: "opacity",               category: StyleCategory::Effects,              family: "opacity",               properties: &["opacity"] },
    // --- transitions & animation ---
    StyleMethod { name: "transition",            category: StyleCategory::TransitionsAnimation, family: "transition",            properties: &["transition"] },
    // --- transforms ---
    StyleMethod { name: "transform",             category: StyleCategory::Transforms,           family: "transform",             properties: &["transform"] },
    // --- interactivity ---
    StyleMethod { name: "cursor",                category: StyleCategory::Interactivity,        family: "cursor",                properties: &["cursor"] },
    StyleMethod { name: "user_select",           category: StyleCategory::Interactivity,        family: "user-select",           properties: &["user-select"] },
];

/// The condition combinators, each with the axis it writes. Every condition
/// sorts after every property method; among themselves they sort by axis, and
/// two conditions on the SAME axis keep their written order (which is what lets
/// `media`'s arbitrary min-width sit among `sm`/`md`/`lg`/`xl` without the
/// formatter having to read its argument).
#[doc(hidden)]
#[rustfmt::skip]
pub const STYLE_CONDITION_METHODS: &[(&str, ConditionAxis)] = &[
    ("sm",        ConditionAxis::Media),
    ("md",        ConditionAxis::Media),
    ("lg",        ConditionAxis::Media),
    ("xl",        ConditionAxis::Media),
    ("media",     ConditionAxis::Media),
    ("within",    ConditionAxis::Relation),
    ("children",  ConditionAxis::Relation),
    ("divide",    ConditionAxis::Relation),
    ("attribute", ConditionAxis::Attribute),
    ("hover",     ConditionAxis::Pseudo),
    ("focus",     ConditionAxis::Pseudo),
    ("active",    ConditionAxis::Pseudo),
    ("disabled",  ConditionAxis::Pseudo),
    ("first",     ConditionAxis::Pseudo),
    ("last",      ConditionAxis::Pseudo),
    ("pseudo",    ConditionAxis::Pseudo),
];

/// The breakpoint combinators' own min-widths, as `style.vl` spells them: `md`
/// is `self.media("768px", inner)`. Every row delegates to `media`, so this is
/// a NAME for a width rather than a fifth axis.
///
/// The formatter has no use for it; the language server does. `@media
/// (min-width: 768px)` in a `css` block is refused at the lexer with `.md { … }`
/// named as the fix (css-block.md §7.2 fix 2), and the quickfix that performs
/// that rewrite has to know which combinator a width belongs to. It lives here,
/// beside [`STYLE_CONDITION_METHODS`], so that
/// `crates/vilan-core/tests/style_table_sync.rs` can hold it to the method
/// bodies rather than let a second copy of std's breakpoints drift.
#[doc(hidden)]
pub const STYLE_BREAKPOINT_WIDTHS: &[(&str, &str)] = &[
    ("sm", "640px"),
    ("md", "768px"),
    ("lg", "1024px"),
    ("xl", "1280px"),
];

/// The `Style` methods that are barriers even though they ARE part of std: the
/// slot they write is an argument, not the name, so the formatter cannot know
/// it without evaluating the call. Listed rather than omitted so the
/// table-completeness gate can tell "deliberately a barrier" from "forgotten".
///
/// `add` (the `+` operator's method) and `class_list` are here for the same
/// reason a user extension is: `add` merges an arbitrary right-hand `Style`,
/// and `class_list` ends the chain.
#[doc(hidden)]
pub const STYLE_BARRIER_METHODS: &[&str] = &[
    "rule",
    "raw",
    "with_length",
    "with_color",
    "with_border",
    "child_relation",
    "add",
    "class_list",
];

/// A link's position in the canonical order. `Property` sorts before every
/// `Condition`, which is conditions-last; a property carries its FAMILY's rank
/// (the family's first row in [`STYLE_PROPERTY_METHODS`]) so that a stable sort
/// can never separate two entangled slots, and a condition carries its axis.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum StyleLinkRank {
    Property(usize),
    Condition(ConditionAxis),
}

/// The rank a slot FAMILY sorts at: the position of the family's FIRST row in
/// [`STYLE_PROPERTY_METHODS`]. Every method — and, in a `css` block, every CSS
/// property — that writes a slot entangled with the family's carries this one
/// rank, which is what makes a stable sort unable to separate two entangled
/// slots.
fn style_family_rank(family: &str) -> usize {
    STYLE_PROPERTY_METHODS
        .iter()
        .position(|row| row.family == family)
        .expect("a family is named by a row of the table it is read from")
}

/// The canonical rank of a chain link called `name`, or `None` when the name is
/// a BARRIER — an unknown method (a user `impl Style` extension) or one of
/// [`STYLE_BARRIER_METHODS`].
fn style_link_rank(name: &str) -> Option<StyleLinkRank> {
    if let Some(method) = STYLE_PROPERTY_METHODS.iter().find(|row| row.name == name) {
        return Some(StyleLinkRank::Property(style_family_rank(method.family)));
    }
    STYLE_CONDITION_METHODS
        .iter()
        .find(|(condition, _)| *condition == name)
        .map(|(_, axis)| StyleLinkRank::Condition(*axis))
}

/// The canonical permutation of a sequence of ranked links, or `None` when the
/// sequence is already canonical (so every caller can leave an unchanged
/// construct on its existing code path, byte for byte).
///
/// Each maximal run of KNOWN links between barriers (`None`) sorts on its own; a
/// barrier keeps its index. The sort is stable, so links sharing a rank — two
/// methods of one family, two conditions on one axis — keep their written order.
///
/// This is the ONE implementation of the order. Four callers reduce to it: the
/// printer permuting a chain's AST spine, the safety net permuting a chain's
/// token slice, and the same pair for a `css` block's items — so a block and the
/// chain it desugars to cannot disagree about what canonical means, and neither
/// can the printer and the net.
fn canonical_permutation(ranks: &[Option<StyleLinkRank>]) -> Option<Vec<usize>> {
    let mut order: Vec<usize> = (0..ranks.len()).collect();
    let mut run_start = 0;
    for index in 0..=ranks.len() {
        let is_barrier = index == ranks.len() || ranks[index].is_none();
        if is_barrier {
            order[run_start..index]
                .sort_by_key(|link| ranks[*link].expect("a run holds only ranked links"));
            run_start = index + 1;
        }
    }
    order
        .iter()
        .enumerate()
        .any(|(at, link)| at != *link)
        .then_some(order)
}

/// The canonical permutation of a `style()` chain's link `names`.
fn style_chain_permutation(names: &[&str]) -> Option<Vec<usize>> {
    let ranks: Vec<Option<StyleLinkRank>> =
        names.iter().map(|name| style_link_rank(name)).collect();
    canonical_permutation(&ranks)
}

/// Whether the tokens at `index` open a `style()` builder — the bare call
/// `style ( )`, not a method call `x.style()` and not a qualified `p::style()`.
/// The AST side matches the same shape (a `Call` on a bare `Accessor("style")`
/// with no arguments), which is what keeps the net and the printer in step.
fn starts_style_builder(tokens: &[Token<'_>], index: usize) -> bool {
    if !matches!(tokens.get(index), Some(Token::Ident("style"))) {
        return false;
    }
    if matches!(
        index.checked_sub(1).and_then(|before| tokens.get(before)),
        Some(Token::Ctrl('.')) | Some(Token::Op("::"))
    ) {
        return false;
    }
    matches!(tokens.get(index + 1), Some(Token::Ctrl('(')))
        && matches!(tokens.get(index + 2), Some(Token::Ctrl(')')))
}

/// One `.name(…)` link of a style chain, as it appears in the token stream: the
/// method's name, and the token range spanning the whole link (its `.`, its
/// name, and its balanced argument list).
type StyleChainLink<'src> = (&'src str, std::ops::Range<usize>);

/// The leading run of `.name(…)` links of the chain that begins at the
/// `style ( )` at `index`.
///
/// The run stops at the first postfix that is not a plain method call —
/// `.field`, `[i]`, `?`, `!`, a turbofish, a tuple index. Those glue to the link
/// before them, and past one the receiver is no longer the `Style` this table
/// describes, so nothing beyond is sorted and nothing crosses the boundary. The
/// printer stops the run at exactly the same place (`Printer::style_sorted_links`),
/// which is what keeps the net and the printer in step.
fn style_chain_links<'src>(
    tokens: &[Token<'src>],
    index: usize,
) -> Option<Vec<StyleChainLink<'src>>> {
    let mut links = Vec::new();
    let mut cursor = index + 3;
    while matches!(tokens.get(cursor), Some(Token::Ctrl('.'))) {
        // Anything but `. name (` ENDS the run rather than abandoning the
        // chain: a `.field`, a tuple index or a turbofish glues to the link
        // before it, and the links already collected still sort among
        // themselves.
        let Some(Token::Ident(name)) = tokens.get(cursor + 1) else {
            break;
        };
        if !matches!(tokens.get(cursor + 2), Some(Token::Ctrl('('))) {
            break;
        }
        let scan = balanced_end(tokens, cursor + 2)?;
        links.push((*name, cursor..scan + 1));
        cursor = scan + 1;
    }
    Some(links)
}

/// The index of the delimiter closing the one that opens at `open`, counting
/// every bracket kind together (the token stream is already known to balance —
/// it came from a file that parsed). `None` when the stream runs out first,
/// which is how a malformed slice declines rather than panicking.
fn balanced_end(tokens: &[Token<'_>], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut scan = open;
    loop {
        match tokens.get(scan)? {
            Token::Ctrl('(') | Token::Ctrl('[') | Token::Ctrl('{') => depth += 1,
            Token::Ctrl(')') | Token::Ctrl(']') | Token::Ctrl('}') => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(scan);
                }
            }
            _ => {}
        }
        scan += 1;
    }
}

/// Reorders the `.name(…)` links of every `style()` builder chain into the
/// canonical order, so that a source chain and the printer's reordered reprint
/// reduce to the same token sequence. Every other token keeps its position, so
/// the safety net still catches every other reordering.
///
/// Nested chains — a condition's inner `style()` — are reached because the scan
/// walks straight through a link's argument tokens after emitting the link.
// `pub` (doc-hidden) only so the external corpus tripwire in
// `tests/parse_differential.rs` mirrors the net's style-chain canonicalization
// through this ONE implementation rather than a divergent copy — the same
// "cannot disagree" guarantee [`sort_import_runs`] carries. Not part of the
// supported API.
#[doc(hidden)]
pub fn sort_style_chains<'src>(tokens: Vec<Token<'src>>) -> Vec<Token<'src>> {
    let mut result: Vec<Token<'src>> = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        // A chain the order leaves alone reports no permutation and falls
        // through to the passthrough below, token for token.
        let permuted = if starts_style_builder(&tokens, index) {
            style_chain_links(&tokens, index).and_then(|links| {
                let names: Vec<&str> = links.iter().map(|(name, _)| *name).collect();
                style_chain_permutation(&names).map(|order| (links, order))
            })
        } else {
            None
        };
        if let Some((links, order)) = permuted {
            result.extend_from_slice(&tokens[index..index + 3]);
            for link in order {
                // The link's own argument tokens are re-scanned by the
                // recursion, which is what sorts a condition's inner chain.
                let range = links[link].1.clone();
                result.extend(sort_style_chains(tokens[range].to_vec()));
            }
            index = links
                .last()
                .map(|(_, range)| range.end)
                .expect("a permutation implies at least two links");
            continue;
        }
        result.push(tokens[index].clone());
        index += 1;
    }
    result
}

// --- Canonical `css` block order ---------------------------------------------
//
// A headless `css { … }` block gets the SAME canonical order a `style()` chain
// gets (`proposal/css-block.md` §8; Q2 ruled 2026-08-28: the formatter sorts).
// It has to: the block lowers to that chain, so a block that formatted
// differently from the chain it desugars to would be a wart with two canonical
// spellings of one style.
//
// Neither of the chain sorter's gates fires on a block — the formatter reparses
// SOURCE and never sees the desugar, and there is no `style ( )` token run in a
// block — so the block needs its own pair of gates. What it deliberately does
// NOT get is a table of its own. The rank tables are keyed by METHOD name and a
// block writes CSS PROPERTY names, so the property rank is DERIVED from the very
// same rows: each already carries the slot family its properties belong to, held
// to `style.vl` by `crates/vilan-core/tests/style_table_sync.rs`, which also
// gates that no property is claimed by two families (or this derivation would
// not be well defined). One order function ([`canonical_permutation`]), one set
// of tables, four callers.
//
// Two differences from the chain, both in the block's favour and both earned:
//
//   * `raw` is a BARRIER in a chain because the slot it writes is an argument
//     the formatter cannot evaluate. In a block the property is a TOKEN, right
//     there in the source, so a declaration ranks by what it actually writes.
//     A property no row writes — `-webkit-mask-composite`, a custom property —
//     is a barrier, exactly as an unknown method is: nothing crosses it.
//   * The dot decides which table to read, as it decides everything else about
//     an item (§3). Undotted ranks by property, dotted by condition axis. The
//     order function never has to guess.
//
// Refused outright: reordering ANY block that contains a comment. A reordered
// body would carry its comments to the wrong item and the comment cursor only
// moves forward — the same refusal, for the same reason, that
// [`Printer::style_sorted_links`] makes for a chain. Such a block still prints
// canonically (one item per line, nested rules at +1); only the reorder is off.

/// The canonical rank of a CSS PROPERTY as written in a `css` block —
/// `flex-direction`, `padding-left`, `--brand-ink`. Read out of
/// [`STYLE_PROPERTY_METHODS`]'s `properties` column rather than a fourth
/// hand-maintained table; `None` for a property no row writes, which is a
/// BARRIER.
fn css_property_rank(property: &str) -> Option<StyleLinkRank> {
    STYLE_PROPERTY_METHODS
        .iter()
        .find(|row| row.properties.contains(&property))
        .map(|row| StyleLinkRank::Property(style_family_rank(row.family)))
}

/// The canonical rank of one `css` block item. `dotted` is the grammar's own
/// disambiguator (§3): a dotted item is a condition combinator and ranks by its
/// axis, an undotted one is a declaration and ranks by its property.
fn css_item_rank(dotted: bool, name: &str) -> Option<StyleLinkRank> {
    if dotted {
        return STYLE_CONDITION_METHODS
            .iter()
            .find(|(condition, _)| *condition == name)
            .map(|(_, axis)| StyleLinkRank::Condition(*axis));
    }
    css_property_rank(name)
}

/// One item of a `css` body as the TOKEN scan sees it, for the safety net's half
/// of the canonicalization.
struct CssTokenItem {
    rank: Option<StyleLinkRank>,
    /// The whole item: a declaration through its `;`, a nested rule through its
    /// closing `}`.
    range: std::ops::Range<usize>,
    /// A nested rule's body `{`, so the recursion reaches its items too.
    body_open: Option<usize>,
}

/// The items of the `css` body whose `{` sits at `open`, in written order, and
/// the index of the `}` that closes it.
///
/// `None` for a body that does not scan. The net then leaves the block's tokens
/// exactly as they came, which is the correct degradation: an unsorted stream on
/// both sides still compares equal.
fn css_body_items(tokens: &[Token<'_>], open: usize) -> Option<(Vec<CssTokenItem>, usize)> {
    let mut items = Vec::new();
    let mut cursor = open + 1;
    loop {
        match tokens.get(cursor)? {
            Token::Ctrl('}') => return Some((items, cursor)),
            // `.name { … }` / `.name(a, b) { … }` — a condition combinator.
            Token::Ctrl('.') => {
                let Some(Token::Ident(name)) = tokens.get(cursor + 1) else {
                    return None;
                };
                let mut scan = cursor + 2;
                if matches!(tokens.get(scan), Some(Token::Ctrl('('))) {
                    scan = balanced_end(tokens, scan)? + 1;
                }
                if !matches!(tokens.get(scan), Some(Token::Ctrl('{'))) {
                    return None;
                }
                let body_open = scan;
                let end = balanced_end(tokens, body_open)?;
                items.push(CssTokenItem {
                    rank: css_item_rank(true, name),
                    range: cursor..end + 1,
                    body_open: Some(body_open),
                });
                cursor = end + 1;
            }
            // `property: value;` — the property is span-adjacent name and `-`
            // tokens (`flex-direction` is three, `--color-ink` is five), which
            // the parser has already proved adjacent by accepting the file.
            _ => {
                let mut property = String::new();
                let mut scan = cursor;
                while let Some(token) = tokens.get(scan) {
                    match token {
                        Token::Op("-") => property.push('-'),
                        Token::Op(_)
                        | Token::Ctrl(_)
                        | Token::String(_)
                        | Token::MultilineString(_)
                        | Token::Number(..) => break,
                        // An identifier, or any keyword — CSS property names are
                        // not vilan's reserved words (`for`, `type`, `css`).
                        name => property.push_str(&name.to_string()),
                    }
                    scan += 1;
                }
                if property.is_empty() || !matches!(tokens.get(scan), Some(Token::Op(":"))) {
                    return None;
                }
                // The value runs to the `;` at brace depth zero; a `{expr}` hole
                // and a `calc(…)` both nest, and a `;` inside a string is a
                // `Token::String`, not a `Ctrl`.
                let mut depth = 0usize;
                loop {
                    match tokens.get(scan)? {
                        Token::Ctrl('(') | Token::Ctrl('[') | Token::Ctrl('{') => depth += 1,
                        Token::Ctrl(')') | Token::Ctrl(']') | Token::Ctrl('}') => {
                            depth = depth.checked_sub(1)?
                        }
                        Token::Ctrl(';') if depth == 0 => break,
                        _ => {}
                    }
                    scan += 1;
                }
                items.push(CssTokenItem {
                    rank: css_item_rank(false, &property),
                    range: cursor..scan + 1,
                    body_open: None,
                });
                cursor = scan + 1;
            }
        }
    }
}

/// The body at `open` with its items in canonical order — `{` and `}` included
/// — and the index just past the `}`.
fn sorted_css_body<'src>(tokens: &[Token<'src>], open: usize) -> Option<(Vec<Token<'src>>, usize)> {
    let (items, close) = css_body_items(tokens, open)?;
    let ranks: Vec<Option<StyleLinkRank>> = items.iter().map(|item| item.rank).collect();
    let order =
        canonical_permutation(&ranks).unwrap_or_else(|| (0..items.len()).collect::<Vec<usize>>());
    let mut body = vec![Token::Ctrl('{')];
    for at in order {
        let item = &items[at];
        match item.body_open {
            // A nested rule: its HEAD may hold a block of its own
            // (`.within(css_name(), …)`), and its body sorts by the same rule.
            Some(body_open) => {
                body.extend(sort_css_blocks(
                    tokens[item.range.start..body_open].to_vec(),
                ));
                let (inner, _) = sorted_css_body(tokens, body_open)?;
                body.extend(inner);
            }
            // A declaration: a hole is an ordinary expression and may hold a
            // block of its own.
            None => body.extend(sort_css_blocks(tokens[item.range.clone()].to_vec())),
        }
    }
    body.push(Token::Ctrl('}'));
    Some((body, close + 1))
}

/// Reorders the items of every `css { … }` block into the canonical order, so
/// that a source block and the printer's reordered reprint reduce to the same
/// token sequence. Every other token keeps its position, so the net still
/// catches every other reordering.
///
/// A `style()` chain inside a hole is already canonical by the time this runs —
/// [`normalize`] applies [`sort_style_chains`] to the whole stream first, and
/// that scan walks straight through a block's tokens.
// `pub` (doc-hidden) only so the external corpus tripwire in
// `tests/parse_differential.rs` mirrors the net's css-block canonicalization
// through this ONE implementation rather than a divergent copy — the same
// "cannot disagree" guarantee [`sort_style_chains`] carries. Not part of the
// supported API.
#[doc(hidden)]
pub fn sort_css_blocks<'src>(tokens: Vec<Token<'src>>) -> Vec<Token<'src>> {
    let mut result: Vec<Token<'src>> = Vec::with_capacity(tokens.len());
    let mut index = 0;
    while index < tokens.len() {
        // The block's own two-token gate, the one the parser uses: the keyword
        // and a `{`. Every other `css` is a parse error today, so nothing else
        // can reach here in a file that formatted.
        if matches!(tokens.get(index), Some(Token::Css))
            && matches!(tokens.get(index + 1), Some(Token::Ctrl('{')))
            && let Some((body, past)) = sorted_css_body(&tokens, index + 1)
        {
            result.push(Token::Css);
            result.extend(body);
            index = past;
            continue;
        }
        result.push(tokens[index].clone());
        index += 1;
    }
    result
}

// --- Organize imports (the editor action) ------------------------------------
//
// The LSP "Organize Imports" source action both *sorts* a file's top-level
// import runs — in exactly the order `vilan fmt` produces, through the same
// [`import_sort_key`], so the two can never disagree — and *prunes* the leaves
// an editor's analyzer reports as unused. Sorting is the formatter's job either
// way; the usage decision is the editor's, threaded in as the `keep` predicate.
// Pruning is deliberately NOT part of `vilan fmt` (fmt has no analyzer), so it
// lives only behind this entry point.

/// One organized top-level import run: the source span it currently occupies and
/// the canonical replacement text (empty when the whole run pruned away). The LSP
/// turns each into a `TextEdit`.
pub struct ImportRunEdit {
    pub span: Span,
    pub replacement: String,
}

/// A pruned import statement awaiting canonical rendering. A re-export is surface,
/// not usage, so it is never pruned and renders from its original node; an
/// `import`/`use` that survived (whole or in part) renders from a node rebuilt to
/// carry only the leaves `keep` retained.
enum PrunedStatement<'ast, 'src> {
    ReExport(&'ast Node<'src>),
    Rebuilt(Node<'src>),
}

impl<'src> PrunedStatement<'_, 'src> {
    fn node(&self) -> &Node<'src> {
        match self {
            PrunedStatement::ReExport(node) => node,
            PrunedStatement::Rebuilt(node) => node,
        }
    }
}

/// Prunes an import path to the leaves `keep` retains, returning the surviving
/// branch — or `None` if every leaf was dropped. `keep(name_span)` is asked of
/// each *terminal* segment (the actual imported name): a `Path` with a `::`
/// continuation survives iff its continuation does, and a brace `Set` keeps its
/// surviving branches (`{ a, b }` with `b` unused becomes `{ a }`), dying only
/// when all of them go.
fn prune_import_branch<'src>(
    branch: &ImportBranch<'src>,
    keep: &dyn Fn(Span) -> bool,
) -> Option<ImportBranch<'src>> {
    match branch {
        ImportBranch::Path(name, span, None) => {
            keep(*span).then_some(ImportBranch::Path(name, *span, None))
        }
        ImportBranch::Path(name, span, Some(child)) => prune_import_branch(child, keep)
            .map(|pruned| ImportBranch::Path(name, *span, Some(Box::new(pruned)))),
        ImportBranch::Set(branches) => {
            let kept: Vec<ImportBranch<'src>> = branches
                .iter()
                .filter_map(|branch| prune_import_branch(branch, keep))
                .collect();
            (!kept.is_empty()).then_some(ImportBranch::Set(kept))
        }
    }
}

/// The source spans of every TOP-LEVEL `import` / `use` / re-export statement.
///
/// The organize-imports usage model needs to tell a reference written by the
/// file's own CODE from one written by its import list. An import path's
/// segments resolve to the very definitions its leaves bind, so a model that
/// counts every reference alike lets an import justify its own existence and it
/// can then never be pruned — which is what kept an unused `Result::{ self }`
/// alive forever (kolt.local 004). Returns an empty list when the source does
/// not parse, which the caller reads as "decide nothing".
pub fn import_statement_spans(source: &str) -> Vec<Span> {
    let Some(items) = parse(source) else {
        return Vec::new();
    };
    items
        .iter()
        .filter(|item| import_kind_and_branch(&item.0).is_some())
        .map(|item| item.1)
        .collect()
}

/// Every top-level import LEAF's terminal-name span, in source order — the
/// spans an editor asks about when it fades the imports nobody uses (E114).
///
/// The walk is [`prune_import_branch`]'s, so the editor and the organizer are
/// asking about exactly one set of leaves: what the organizer would prune is
/// what the editor fades, and nothing else can drift between them. A RE-EXPORT
/// is excluded here for the same reason the organizer never prunes one —
/// `export import` binds a name for somebody else, so this file not using it is
/// the whole point rather than a mistake.
///
/// Empty when the source does not parse, which the caller reads as "decide
/// nothing" — the same contract [`import_statement_spans`] has.
pub fn import_leaf_name_spans(source: &str) -> Vec<Span> {
    let Some(items) = parse(source) else {
        return Vec::new();
    };
    let mut spans = Vec::new();
    for item in items.iter() {
        if matches!(item.0, Node::Export(_)) {
            continue;
        }
        let Some((_, branch)) = import_kind_and_branch(&item.0) else {
            continue;
        };
        collect_import_leaf_spans(branch, &mut spans);
    }
    spans
}

/// [`import_leaf_name_spans`]' recursion: a `Path` with a `::` continuation
/// defers to the continuation, a brace `Set` yields every member's leaf, and a
/// terminal `Path` IS the leaf.
fn collect_import_leaf_spans(branch: &ImportBranch<'_>, out: &mut Vec<Span>) {
    match branch {
        ImportBranch::Path(_, span, None) => out.push(*span),
        ImportBranch::Path(_, _, Some(child)) => collect_import_leaf_spans(child, out),
        ImportBranch::Set(branches) => {
            for branch in branches {
                collect_import_leaf_spans(branch, out);
            }
        }
    }
}

/// Organizes a file's *top-level* import runs: sorts each into canonical order
/// (the shared [`import_sort_key`], identical to `vilan fmt`) and, per `keep`,
/// prunes unused leaves. Returns one [`ImportRunEdit`] per run whose canonical
/// form differs from the source — an already-organized run yields nothing.
/// `keep(name_span)` decides whether the import leaf whose terminal name occupies
/// `name_span` survives; pass `|_| true` for sort-only. `None` when the source
/// doesn't parse cleanly (no edit would be safe). Block-scoped imports live
/// inside item bodies, not the top-level list, so they are never considered.
pub fn organize_import_runs(
    source: &str,
    keep: &dyn Fn(Span) -> bool,
) -> Option<Vec<ImportRunEdit>> {
    let items = parse(source)?;
    let mut printer = Printer {
        out: String::new(),
        indent: 0,
        comments: extract_comments(source),
        cursor: 0,
        source,
        bailed: false,
        split: Split::Off,
        probing: false,
    };
    Some(printer.organize_runs(&items, keep))
}

// --- Insert an import (the add-import quickfix and auto-import completion) --
//
// E54's second formatter entry point: not reorganizing the imports that are
// already there, but growing them by one leaf. `insert_import` extends an
// existing statement that already reaches the target module when one exists
// among the file's top-level imports, and otherwise inserts a brand new,
// canonically-positioned statement — both as ONE minimal [`ImportInsertEdit`],
// never a full reprint of the run (the LSP applies this from a live buffer,
// where a surgical edit is what keeps the cursor and the rest of the file
// untouched).

/// One insertion edit adding a leaf to `source`'s imports — a `TextEdit`
/// shape, like [`ImportRunEdit`] but for growing a file's imports rather than
/// reorganizing the ones already there.
pub struct ImportInsertEdit {
    pub span: Span,
    pub replacement: String,
}

/// A parsed import path's trailing shape, past the segments leading to it —
/// what [`decompose_import_branch`] splits a branch into.
enum ImportLeafShape<'ast, 'src> {
    /// A single trailing name with its own span (`import std::json::Json`).
    Single(&'src str, Span),
    /// A brace-set of trailing names (`import std::json::{ A, B }`).
    Set(&'ast [ImportBranch<'src>]),
}

/// Splits a parsed import path into the segments leading to its terminal
/// leaf(ves) and the leaf shape itself: `std::json::Json` decomposes to
/// (`["std", "json"]`, `Single("Json", ..)`); `std::json::{ A, B }` to
/// (`["std", "json"]`, `Set([A, B])`); a rare root-level brace set
/// (`import { a, b }`, no leading namespace) to (`[]`, `Set([a, b])`).
fn decompose_import_branch<'ast, 'src>(
    branch: &'ast ImportBranch<'src>,
) -> (Vec<&'src str>, ImportLeafShape<'ast, 'src>) {
    match branch {
        ImportBranch::Path(name, span, None) => (Vec::new(), ImportLeafShape::Single(name, *span)),
        ImportBranch::Path(name, _, Some(child)) => match child.as_ref() {
            ImportBranch::Set(branches) => (vec![*name], ImportLeafShape::Set(branches)),
            ImportBranch::Path(..) => {
                let (mut prefix, shape) = decompose_import_branch(child);
                prefix.insert(0, name);
                (prefix, shape)
            }
        },
        ImportBranch::Set(branches) => (Vec::new(), ImportLeafShape::Set(branches)),
    }
}

/// The result of [`try_extend_import`]'s probe of ONE existing `import`
/// statement.
enum ExtendOutcome {
    /// This statement doesn't import from the target module at all.
    NoMatch,
    /// It does, and the leaf is already among its names — nothing to do.
    AlreadyImported,
    /// It does, and doesn't have the leaf yet: the edit that adds it.
    Edit(Span, String),
}

/// Tries to extend `branch` — one plain `import` statement's path — with
/// `leaf`, when `branch` already reaches `module_path`: a brace set gains a
/// member inserted at its alphabetically-sorted position (`{ Decode }` ->
/// `{ Decode, Encode }`), and a bare single leaf becomes a two-member set
/// (`Json` -> `{ Encode, Json }`) — both computed from the AST's own leaf
/// spans, never a reprint of the whole statement. A brace set holding a
/// non-flat member (a further `::` continuation inside the set — not a shape
/// a plain leaf list ever takes in practice) is left alone: [`insert_import`]
/// falls back to a new import line rather than guess at reordering it.
fn try_extend_import<'src>(
    branch: &ImportBranch<'src>,
    module_path: &[&str],
    leaf: &str,
) -> ExtendOutcome {
    let (prefix, shape) = decompose_import_branch(branch);
    if prefix != module_path {
        return ExtendOutcome::NoMatch;
    }
    match shape {
        ImportLeafShape::Single(name, span) => {
            if name == leaf {
                return ExtendOutcome::AlreadyImported;
            }
            let (first, second) = if name < leaf {
                (name, leaf)
            } else {
                (leaf, name)
            };
            ExtendOutcome::Edit(span, format!("{{ {first}, {second} }}"))
        }
        ImportLeafShape::Set(branches) => {
            let mut members: Vec<(&str, Span)> = Vec::with_capacity(branches.len());
            for member in branches {
                match member {
                    ImportBranch::Path(name, span, None) => {
                        if *name == leaf {
                            return ExtendOutcome::AlreadyImported;
                        }
                        members.push((name, *span));
                    }
                    // A non-flat member: don't guess at reordering the set.
                    _ => return ExtendOutcome::NoMatch,
                }
            }
            match members.iter().find(|(name, _)| *name > leaf) {
                Some((_, span)) => ExtendOutcome::Edit(
                    Span {
                        start: span.start,
                        end: span.start,
                    },
                    format!("{leaf}, "),
                ),
                None => {
                    // The grammar guarantees a brace set has at least one member.
                    let end = members
                        .last()
                        .expect("a parsed brace set has at least one member")
                        .1
                        .end;
                    ExtendOutcome::Edit(Span { start: end, end }, format!(", {leaf}"))
                }
            }
        }
    }
}

/// Only a PLAIN `import` — never a `use` (a different binding form) and never
/// an `export import` (extending someone's re-export with a plain leaf would
/// silently make the new name part of the module's public surface too,
/// which is not what an add-import quickfix asked for).
fn plain_import_branch<'node, 'src>(node: &'node Node<'src>) -> Option<&'node ImportBranch<'src>> {
    match node {
        Node::Import(branch) => Some(branch),
        _ => None,
    }
}

/// Builds the sort key a fresh `import <module_path>::<leaf>;` statement
/// would have, for finding where it belongs among a run's existing entries —
/// without constructing a throwaway AST node to feed [`node_import_key`].
fn fresh_import_sort_key(module_path: &[&str], leaf: &str) -> ImportSortKey {
    let mut branch = TokenBranch::Path(leaf, None);
    for segment in module_path.iter().rev() {
        branch = TokenBranch::Path(segment, Some(Box::new(branch)));
    }
    import_sort_key(ImportKind::Import, &branch)
}

/// The contiguous run of top-level import-like items starting at the FIRST
/// one found — `(start, end)` index bounds into `items` (`end` exclusive).
/// `None` when `items` has no top-level import at all. Mirrors
/// [`Printer::organize_runs`]'s own run predicate
/// ([`import_kind_and_branch`]), so "the file's first import run" means the
/// same thing here as it does to Organize Imports.
fn first_import_run(items: &[Spanned<Node<'_>>]) -> Option<(usize, usize)> {
    let start = items
        .iter()
        .position(|item| import_kind_and_branch(&item.0).is_some())?;
    let mut end = start;
    while end < items.len() && import_kind_and_branch(&items[end].0).is_some() {
        end += 1;
    }
    Some((start, end))
}

/// The byte offset just past a statement's terminating `;`, scanning forward
/// from `from` — an import node's own span ends at its path, before the `;`
/// (see [`Printer::organize_run`]'s note on the same gap).
fn statement_end(source: &str, from: usize) -> usize {
    let bytes = source.as_bytes();
    let mut index = from;
    while index < bytes.len() && bytes[index] != b';' {
        index += 1;
    }
    if index < bytes.len() {
        index += 1;
    }
    index
}

/// One clean parse of a source buffer, held so repeated [`insert_import`]
/// probes share it (E83): auto-import completion computes an edit for every
/// surviving candidate against the SAME buffer, and the parse is the
/// expensive half of each probe — a bare scope position used to pay it once
/// per candidate (`proposals/proposal/playground-completion.md` §9). Parse
/// once per request, probe once per candidate.
pub struct ParsedSource<'src> {
    source: &'src str,
    items: NodeList<'src>,
}

impl<'src> ParsedSource<'src> {
    /// Parses `source` for [`Self::insert_import`] probes — `None` when it
    /// does not parse cleanly (the formatter's usual safety rule: no edit
    /// would be safe, so no probe could answer either).
    pub fn parse(source: &'src str) -> Option<Self> {
        Some(ParsedSource {
            source,
            items: parse(source)?,
        })
    }

    /// [`insert_import`] against the already-parsed buffer: byte-identical
    /// answers, one parse however many leaves are probed.
    pub fn insert_import(&self, module_path: &[&str], leaf: &str) -> Option<ImportInsertEdit> {
        insert_import_into(self.source, &self.items, module_path, leaf)
    }
}

/// Computes the edit that adds `import <module_path>::<leaf>;` to `source` —
/// the LSP add-import quickfix's insert half, and the edit an auto-import
/// completion candidate carries (E54). Extends an existing PLAIN `import`
/// that already reaches `module_path`, when one exists anywhere among the
/// file's top-level statements (see [`try_extend_import`]); otherwise inserts
/// a new statement, in its canonically sorted position, into the file's FIRST
/// top-level import run — or, when the file has no import at all, as a new
/// first line.
///
/// `None` when `source` doesn't parse cleanly (the formatter's usual safety
/// rule: no edit would be safe) or the leaf is already imported from
/// `module_path` (nothing to do — the caller asked to add something that's
/// already there).
///
/// Parses `source` per call. A caller probing MANY leaves against one buffer
/// — auto-import completion, one probe per candidate — goes through
/// [`ParsedSource`] instead and pays the parse once (E83).
pub fn insert_import(source: &str, module_path: &[&str], leaf: &str) -> Option<ImportInsertEdit> {
    ParsedSource::parse(source)?.insert_import(module_path, leaf)
}

/// The shared body behind [`insert_import`] and [`ParsedSource::insert_import`]:
/// the probe over an already-parsed item list.
fn insert_import_into(
    source: &str,
    items: &NodeList<'_>,
    module_path: &[&str],
    leaf: &str,
) -> Option<ImportInsertEdit> {
    for item in items {
        let Some(branch) = plain_import_branch(&item.0) else {
            continue;
        };
        match try_extend_import(branch, module_path, leaf) {
            ExtendOutcome::NoMatch => continue,
            ExtendOutcome::AlreadyImported => return None,
            ExtendOutcome::Edit(span, replacement) => {
                return Some(ImportInsertEdit { span, replacement });
            }
        }
    }
    let new_line = format!("import {}::{leaf};", module_path.join("::"));
    let Some((start, end)) = first_import_run(items) else {
        // No import anywhere in the file: a new first line.
        return Some(ImportInsertEdit {
            span: Span { start: 0, end: 0 },
            replacement: format!("{new_line}\n"),
        });
    };
    let new_key = fresh_import_sort_key(module_path, leaf);
    let insert_before = items[start..end]
        .iter()
        .find(|item| node_import_key(&item.0) > new_key);
    match insert_before {
        Some(item) => {
            let at = item.1.into_range().start;
            Some(ImportInsertEdit {
                span: Span { start: at, end: at },
                replacement: format!("{new_line}\n"),
            })
        }
        None => {
            let last = &items[end - 1];
            let at = statement_end(source, last.1.into_range().end);
            Some(ImportInsertEdit {
                span: Span { start: at, end: at },
                replacement: format!("\n{new_line}"),
            })
        }
    }
}

/// Parses `source` into its top-level item list, or `None` if it doesn't parse
/// perfectly cleanly — the formatter reprints only sources it fully understands.
///
/// The formatter parses in GROUP-PRESERVING mode
/// ([`crate::parsing::parse_preserving_groups`]): every `(…)` is recorded as a
/// [`Node::LiftGroup`] rather than dissolving into its inner expression. A
/// reprint has to reproduce the source's token stream, and a group the tree does
/// not record is one the printer cannot put back — the safety net would then see
/// two missing tokens and silently bail the whole file. Recording them makes
/// user-written parentheses PRESERVED: the formatter reprints the group it was
/// given and never adjudicates whether it was redundant.
fn parse(source: &str) -> Option<NodeList<'_>> {
    BUFFER_PARSES.with(|count| count.set(count.get() + 1));
    let (tree, errors) = crate::parsing::parse_preserving_groups(source);
    tree.filter(|_| errors.is_empty()).map(|(items, _)| items)
}

/// Formats `original`, returning the reprinted text. Returns the input unchanged
/// if it doesn't lex/parse, if the printer hits a construct it doesn't yet handle,
/// or if the reprint would change the code (see the safety note).
///
/// Canonical Vilan is LF and carries no BOM (`windows-support.md` §2), so the
/// whole reprint runs over the NORMALIZED text: a CRLF file formats to its LF
/// form exactly once and is idempotent after, the same way indentation is
/// canonicalized. Normalizing here rather than at each emission site is what
/// keeps the verbatim slices — macro arguments, an `i"…"` literal, a plain or
/// triple-quoted string's raw text — free of `\r`, and it keeps the token-stream
/// safety net comparing like with like (both sides lex from LF text). A bail
/// still returns the ORIGINAL bytes: a file the formatter does not fully
/// understand is not one to rewrite, not even its line endings.
pub fn format(original: &str) -> String {
    let normalized = crate::util::normalize_newlines(crate::util::strip_bom(original));
    let source: &str = &normalized;
    let Some(original_tokens) = code_tokens(source) else {
        return original.to_string();
    };
    let Some(items) = parse(source) else {
        return original.to_string();
    };
    let mut printer = Printer {
        out: String::new(),
        indent: 0,
        comments: extract_comments(source),
        cursor: 0,
        source,
        bailed: false,
        split: Split::Off,
        probing: false,
    };
    let prev_end = printer.print_items(&items, 0, true);
    // Comments after the last item (trailing end-of-file comments).
    printer.flush_comments_before(source.len(), prev_end);
    printer.out.push('\n');
    if printer.bailed {
        return original.to_string();
    }
    let matches = code_tokens(&printer.out)
        .is_some_and(|reprinted| normalize(reprinted) == normalize(original_tokens));
    if matches {
        printer.out
    } else {
        original.to_string()
    }
}

/// The column budget for ONE rendered line. A line whose inline rendering is
/// *wider* than this re-renders in split form when the construct on it has one
/// (a postfix chain of at least two `.name(…)` call links breaks one link per
/// line; a list literal breaks one element per line); at exactly the budget it
/// stays inline. The budget applies to every line the printer emits — a
/// statement's own line, and recursively each continuation line a split
/// produced. Deliberately not a knob: the formatter has one canonical output,
/// and a width knob would fork every file's shape.
const LINE_BUDGET: usize = 100;

/// The columns a tab occupies when measuring a line. Vilan indents with tabs,
/// so the measurement has to agree with what an editor shows.
const TAB_COLUMNS: usize = 4;

/// The display width of printed text, counting a tab as [`TAB_COLUMNS`] columns
/// and every other character as one. (A tab only ever reaches this from inside a
/// string literal or a verbatim slice — the printer's own indentation is
/// measured from the indent level, not from the text.)
fn display_width(text: &str) -> usize {
    text.chars()
        .map(|character| if character == '\t' { TAB_COLUMNS } else { 1 })
        .sum()
}

/// The permission to break the next expression across lines, armed after a
/// rendered line was measured over [`LINE_BUDGET`]. `print_expr` *takes* it on
/// entry, so every form drops it by default and only the arms that continue the
/// measured line re-arm it explicitly — a forgotten arm can lose a split, never
/// invent one.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
enum Split {
    /// Nothing armed: print inline.
    #[default]
    Off,
    /// A STATEMENT's own line overflowed. The statement's value position may
    /// break — a postfix chain into one link per line, a list literal into one
    /// element per line — reached through the prefix and binary operand
    /// positions that keep the value on the statement's own line, and through a
    /// call's LAST argument (`proposal/argument-tail-descent.md`), which is what
    /// lets `list.push(T { … })` break the literal that is its only breakable
    /// construct.
    Statement,
    /// A line *inside* a split rendering overflowed — a chain link's line, a
    /// list element's line.
    ///
    /// The two permissions allow exactly the same thing since backlog 43 closed
    /// the argument-descent gap between them; what survives is the distinction
    /// of where each was armed, which is what the rollback sites read to decide
    /// whether they are re-printing a statement or a line inside one.
    Tail,
}

struct Printer<'src> {
    out: String,
    indent: usize,
    comments: Vec<(Span, &'src str)>,
    cursor: usize,
    source: &'src str,
    bailed: bool,
    /// The pending [`Split`] permission for the next expression printed.
    split: Split,
    /// True while a seam probe is rendering a chain link to see whether it spans
    /// lines ([`Printer::link_spans_lines`]). Probes do not nest.
    probing: bool,
}

impl<'src> Printer<'src> {
    /// Whether the source between `from` and `to` contains a blank line (a run of
    /// only-whitespace with two or more newlines), used to preserve paragraph gaps.
    fn has_blank_between(&self, from: usize, to: usize) -> bool {
        from < to
            && self
                .source
                .get(from..to)
                .is_some_and(|gap| gap.bytes().filter(|byte| *byte == b'\n').count() >= 2)
    }

    /// Whether a standalone (own-line) comment sits between source offsets
    /// `after` and `before` — a comment preceded within the gap by a newline,
    /// i.e. not a trailing same-line comment of whatever ends at `after`. Such a
    /// comment pins an import run: the run breaks there, so imports never
    /// reorder across it.
    fn standalone_comment_between(&self, after: usize, before: usize) -> bool {
        self.comments.iter().any(|(span, _)| {
            let start = span.into_range().start;
            after <= start
                && start < before
                && self
                    .source
                    .get(after..start)
                    .is_some_and(|gap| gap.contains('\n'))
        })
    }

    /// Starts a fresh line at the current indentation (no leading newline at the
    /// very start of the output).
    fn line(&mut self) {
        if !self.out.is_empty() {
            self.out.push('\n');
            for _ in 0..self.indent {
                self.out.push('\t');
            }
        }
    }

    /// Emits a blank line (used to preserve a paragraph gap before the next item).
    fn blank_line(&mut self) {
        if !self.out.is_empty() {
            self.out.push('\n');
        }
    }

    /// Emits the standalone comments that appear before `pos`, each on its own
    /// line, preserving a blank line before a comment that the source had one
    /// before. Returns the source offset just past the last comment emitted (or
    /// `start_from` if none), so the caller can judge the gap before the item.
    fn flush_comments_before(&mut self, pos: usize, start_from: usize) -> usize {
        let mut at = start_from;
        while self.cursor < self.comments.len() {
            let (span, text) = self.comments[self.cursor];
            let range = span.into_range();
            if range.start >= pos {
                break;
            }
            if self.has_blank_between(at, range.start) {
                self.blank_line();
            }
            self.line();
            self.out.push_str(text);
            at = range.end;
            self.cursor += 1;
        }
        at
    }

    /// Emits a trailing (same-line) comment if the next pending comment starts on
    /// the same source line as `after` — i.e. it sat at the end of the item just
    /// printed (`foo(); // note`) rather than on its own line. Spacing collapses to
    /// a single space.
    fn flush_trailing_comment(&mut self, after: usize) {
        if let Some(text) = self.take_trailing_comment(after) {
            self.out.push(' ');
            self.out.push_str(text);
        }
    }

    /// Consumes and returns the next pending comment when it is a trailing
    /// same-line comment of whatever ends at `after` (`import x; // note`), so a
    /// caller reordering the items it belongs to can re-emit it in the new
    /// place. Returns `None` (consuming nothing) otherwise.
    fn take_trailing_comment(&mut self, after: usize) -> Option<&'src str> {
        if let Some((span, text)) = self.comments.get(self.cursor).copied() {
            let start = span.into_range().start;
            if start >= after
                && self
                    .source
                    .get(after..start)
                    .is_some_and(|gap| !gap.contains('\n'))
            {
                self.cursor += 1;
                return Some(text);
            }
        }
        None
    }

    /// Prints a list of items (top level or a block body), interleaving standalone
    /// comments and preserved blank lines. Returns the source offset past the last
    /// item, for any trailing comments. When `top_level`, a run of import/use
    /// statements prints in the canonical order (see the canonical-import-order
    /// section); block-scoped imports are left as written.
    fn print_items(
        &mut self,
        items: &[Spanned<Node<'src>>],
        start_from: usize,
        top_level: bool,
    ) -> usize {
        // A statement list is a fresh layout context: a split armed OUTSIDE it
        // never reaches a statement inside it. Every statement here earns its
        // own permission from its own measured line, which is the only thing
        // that permission was ever about.
        //
        // Without this, a declaration's own measurement leaks into its body. An
        // over-budget `fun` signature is the first line of the function's
        // rendering, so the statement rule arms a split on it; the signature has
        // no layout to spend it on (argument lists are never wrapped), so the
        // permission travelled into the body and broke the first statement
        // there — `let age = now().since(t).describe();` at 54 columns split
        // three ways because the `fun` above it was 108.
        self.split = Split::Off;
        let mut prev_end = start_from;
        let mut index = 0;
        while index < items.len() {
            if top_level && import_kind_and_branch(&items[index].0).is_some() {
                let run_end = self.import_run_end(items, index);
                prev_end = self.print_import_run(&items[index..run_end], prev_end);
                index = run_end;
                continue;
            }
            let item = &items[index];
            let range = item.1.into_range();
            let after_comments = self.flush_comments_before(range.start, prev_end);
            if self.has_blank_between(after_comments, range.start) {
                self.blank_line();
            }
            self.line();
            // Print the statement inline, then re-print it with the chain split
            // armed if that rendering overflowed the line budget. The terminator
            // is part of what is measured (and glues to the last link in the
            // split form); a trailing same-line comment is not — it is not the
            // statement's code, and letting comment text drive the layout would
            // make the shape depend on prose.
            let statement_start = self.out.len();
            let comment_cursor = self.cursor;
            let terminated = Self::needs_semicolon(&item.0);
            self.print_item(item);
            if terminated {
                self.out.push(';');
            }
            if self.begin_split_reprint(statement_start, comment_cursor) {
                self.print_item(item);
                self.split = Split::Off;
                if terminated {
                    self.out.push(';');
                }
            }
            self.flush_trailing_comment(range.end);
            prev_end = range.end;
            index += 1;
        }
        prev_end
    }

    /// The exclusive end of the import run starting at `start`: the longest span
    /// of consecutive import-like items that no standalone comment breaks (a
    /// blank line does not break it — the block coalesces).
    fn import_run_end(&self, items: &[Spanned<Node<'src>>], start: usize) -> usize {
        let mut end = start + 1;
        while end < items.len() {
            if import_kind_and_branch(&items[end].0).is_none() {
                break;
            }
            let previous_end = items[end - 1].1.into_range().end;
            let this_start = items[end].1.into_range().start;
            if self.standalone_comment_between(previous_end, this_start) {
                break;
            }
            end += 1;
        }
        end
    }

    /// Prints a run of top-level import-like items in canonical order (see
    /// [`import_sort_key`]): reordered by kind/root/path, brace sets sorted,
    /// blank lines coalesced into one block. Each item's trailing same-line
    /// comment travels with it. Returns the source offset past the run.
    fn print_import_run(&mut self, run: &[Spanned<Node<'src>>], prev_end: usize) -> usize {
        let first_start = run[0].1.into_range().start;
        let after_comments = self.flush_comments_before(first_start, prev_end);
        if self.has_blank_between(after_comments, first_start) {
            self.blank_line();
        }
        // Attach each item's trailing comment (in source order, as the cursor
        // reaches it) before reordering. A run has no standalone comments within
        // it (they break it), so every comment here is one item's trailing one.
        let mut entries: Vec<(ImportSortKey, usize, Option<&'src str>)> =
            Vec::with_capacity(run.len());
        for (position, item) in run.iter().enumerate() {
            let end = item.1.into_range().end;
            let key = node_import_key(&item.0);
            let trailing = self.take_trailing_comment(end);
            entries.push((key, position, trailing));
        }
        // A stable canonical sort — duplicate imports keep their source order.
        entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        for (_, position, trailing) in &entries {
            self.line();
            self.print_import_statement(&run[*position].0);
            if let Some(text) = trailing {
                self.out.push(' ');
                self.out.push_str(text);
            }
        }
        run.last().unwrap().1.into_range().end
    }

    /// Organizes the top-level import runs among `items` (sort + `keep`-prune),
    /// returning one edit per run that changes. Non-import items — and everything
    /// inside a block body — are untouched: only the top-level list is walked, and
    /// block-scoped imports never appear in it.
    fn organize_runs(
        &mut self,
        items: &[Spanned<Node<'src>>],
        keep: &dyn Fn(Span) -> bool,
    ) -> Vec<ImportRunEdit> {
        let mut edits = Vec::new();
        let mut index = 0;
        while index < items.len() {
            if import_kind_and_branch(&items[index].0).is_some() {
                let run_end = self.import_run_end(items, index);
                if let Some(edit) = self.organize_run(&items[index..run_end], keep) {
                    edits.push(edit);
                }
                index = run_end;
            } else {
                index += 1;
            }
        }
        edits
    }

    /// Organizes one import run (sort + prune) into a single replacement edit, or
    /// `None` when the run is already canonical. See [`organize_import_runs`].
    fn organize_run(
        &mut self,
        run: &[Spanned<Node<'src>>],
        keep: &dyn Fn(Span) -> bool,
    ) -> Option<ImportRunEdit> {
        let run_start = run[0].1.into_range().start;
        // Reach this run's own trailing comments; a standalone comment before the
        // run stays put (it is outside the replaced span, and it broke the run).
        self.skip_comments_before(run_start);

        // Source-order pass: prune each statement and claim its trailing comment,
        // which travels with it exactly as the printer's reorder does.
        let mut entries: Vec<(
            ImportSortKey,
            usize,
            PrunedStatement<'_, 'src>,
            Option<&'src str>,
        )> = Vec::with_capacity(run.len());
        for (position, item) in run.iter().enumerate() {
            let end = item.1.into_range().end;
            let statement = match &item.0 {
                // A re-export is surface, not usage — never pruned.
                Node::Export(_) => Some(PrunedStatement::ReExport(&item.0)),
                Node::Import(branch) => prune_import_branch(branch, keep)
                    .map(|pruned| PrunedStatement::Rebuilt(Node::Import(pruned))),
                Node::Use(branch) => prune_import_branch(branch, keep)
                    .map(|pruned| PrunedStatement::Rebuilt(Node::Use(pruned))),
                _ => None,
            };
            let trailing = self.take_trailing_comment(end);
            if let Some(statement) = statement {
                let key = node_import_key(statement.node());
                entries.push((key, position, statement, trailing));
            }
            // A statement pruned to nothing drops its trailing comment with it.
        }

        // The replaced span covers the whole run. An import node's span ends at
        // its path, so reach past the terminating `;` and then past the last
        // statement's trailing comment (the canonical text re-emits both).
        let last_terminator = self.statement_terminator_end(run.last().unwrap().1.into_range().end);
        let source_end = self
            .trailing_comment_end(last_terminator)
            .unwrap_or(last_terminator);

        // Every statement pruned away: delete the run, taking one line break so no
        // blank line is left behind.
        if entries.is_empty() {
            let mut deletion_end = source_end;
            // The whole line break, `\r\n` included — a CRLF buffer would
            // otherwise keep the stray `\r` as an empty line (windows-support.md
            // §2). The organizer edits the buffer as written, so unlike `format`
            // it meets both endings head-on instead of normalizing its input.
            let bytes = self.source.as_bytes();
            match bytes.get(deletion_end) {
                Some(b'\n') => deletion_end += 1,
                Some(b'\r') if bytes.get(deletion_end + 1) == Some(&b'\n') => deletion_end += 2,
                _ => {}
            }
            return Some(ImportRunEdit {
                span: Span::from(run_start..deletion_end),
                replacement: String::new(),
            });
        }

        // Canonical order — a stable sort, so equal keys keep their source order.
        entries.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));

        // Render through the printer's own import printing, so an organized run is
        // byte-for-byte what `vilan fmt` would produce for it (minus pruned
        // leaves). `self.out` doubles as the scratch buffer for one statement.
        let mut replacement = String::new();
        for (position, (_, _, statement, trailing)) in entries.iter().enumerate() {
            if position > 0 {
                replacement.push('\n');
            }
            self.out.clear();
            self.print_import_statement(statement.node());
            replacement.push_str(&self.out);
            if let Some(text) = trailing {
                replacement.push(' ');
                replacement.push_str(text);
            }
        }

        // An already-organized run offers no edit. The comparison ignores the
        // line ENDING: the canonical text is `\n`-joined while the buffer may be
        // CRLF, and Organize Imports is not a line-ending converter (that is
        // `fmt`'s job). Comparing raw would offer the action forever on a CRLF
        // buffer — and rewrite the run to LF on every format-on-save.
        let current = self
            .source
            .get(run_start..source_end)
            .map(crate::util::normalize_newlines);
        if current.as_deref() == Some(replacement.as_str()) {
            return None;
        }
        Some(ImportRunEdit {
            span: Span::from(run_start..source_end),
            replacement,
        })
    }

    /// Advances the comment cursor past every comment starting before `pos`,
    /// emitting nothing (unlike `flush_comments_before`) — the organizer uses it
    /// to reach a run's trailing comments while leaving earlier standalone
    /// comments in place.
    fn skip_comments_before(&mut self, pos: usize) {
        while let Some((span, _)) = self.comments.get(self.cursor) {
            if span.into_range().start >= pos {
                break;
            }
            self.cursor += 1;
        }
    }

    /// The offset just past the `;` that terminates an import statement whose
    /// path ends at `path_end` (the import node's span stops at its path; the
    /// `;`, possibly after whitespace, is a separate token). Falls back to
    /// `path_end` if no `;` is found — a cleanly parsed import always has one.
    fn statement_terminator_end(&self, path_end: usize) -> usize {
        let bytes = self.source.as_bytes();
        let mut index = path_end;
        while index < bytes.len() && bytes[index] != b';' {
            if !bytes[index].is_ascii_whitespace() {
                return path_end;
            }
            index += 1;
        }
        if bytes.get(index) == Some(&b';') {
            index + 1
        } else {
            path_end
        }
    }

    /// The end offset of the trailing same-line comment of an item ending at
    /// `after` (a comment starting at/after `after` with no intervening newline),
    /// or `None` — so the organizer's replaced span covers a comment it re-emits.
    fn trailing_comment_end(&self, after: usize) -> Option<usize> {
        self.comments.iter().find_map(|(span, _)| {
            let range = span.into_range();
            (range.start >= after
                && self
                    .source
                    .get(after..range.start)
                    .is_some_and(|gap| !gap.contains('\n')))
            .then_some(range.end)
        })
    }

    /// Prints one top-level import-like item with its brace sets sorted —
    /// `import`/`use`, or an `export import`/`export use` re-export.
    /// (Block-scoped imports print through `print_item` without sorting.)
    fn print_import_like(&mut self, node: &Node<'src>) {
        match node {
            Node::Use(branch) => {
                self.out.push_str("use ");
                self.print_import_branch(branch, true);
                self.out.push(';');
            }
            Node::Import(branch) => {
                self.out.push_str("import ");
                self.print_import_branch(branch, true);
                self.out.push(';');
            }
            Node::Export(inner) => {
                self.out.push_str("export ");
                self.print_import_like(&inner.0);
            }
            _ => {}
        }
    }

    /// Whether `node`, printed as a statement, takes a terminating `;`. Expression
    /// statements (`let`, assignments, calls, a `macro name(..)` invocation, …) do;
    /// control-flow forms (`if`/`for`/`match`/block), declarations (including a
    /// `macro fun`, a `macro { .. }` block, and a `[name] item` macro attribute),
    /// and `use`/`import` (which already emit their own `;`) do not.
    fn needs_semicolon(node: &Node<'src>) -> bool {
        !matches!(
            node,
            Node::If(_)
                | Node::For(_, _)
                | Node::ForIn(_, _, _)
                | Node::Match(_, _)
                | Node::Block(_)
                | Node::Func(_)
                | Node::Struct(_, _, _, _, _)
                | Node::Enum(_, _, _, _)
                | Node::Impl(_, _, _)
                | Node::Trait(_, _, _, _)
                | Node::Module(_, _)
                | Node::Derive(_, _)
                | Node::Service(_, _)
                | Node::Export(_)
                | Node::Use(_)
                | Node::Import(_)
                | Node::MacroFun(_)
                | Node::MacroBlock(_)
                | Node::MacroAttribute(_, _, _, _)
        )
    }

    /// Prints one top-level / block item. Sets `bailed` for anything not yet
    /// handled, so `format` falls back to the original source.
    fn print_item(&mut self, item: &Spanned<Node<'src>>) {
        match &item.0 {
            // `[resource ][external ]struct Name[<…>][;|{ fields }]` — canonical
            // modifier order is `resource external struct` (destruction.md §3).
            Node::Struct(name, generics, external, resource, body) => {
                if *resource {
                    self.out.push_str("resource ");
                }
                if *external {
                    self.out.push_str("external ");
                }
                self.out.push_str("struct ");
                self.out.push_str(name.0);
                self.print_generic_parameters(generics);
                match body {
                    None => self.out.push(';'),
                    Some(fields) if fields.0.is_empty() => self.out.push_str(" {}"),
                    Some(fields) => {
                        self.out.push_str(" {");
                        self.indent += 1;
                        let mut prev_end = fields.1.into_range().start + 1;
                        for ((field_name, field_type, exposed), span) in &fields.0 {
                            let range = span.into_range();
                            let after_comments = self.flush_comments_before(range.start, prev_end);
                            if self.has_blank_between(after_comments, range.start) {
                                self.blank_line();
                            }
                            self.line();
                            if *exposed {
                                self.out.push_str("[expose] ");
                            }
                            self.out.push_str(field_name.0);
                            if let Some(field_type) = field_type {
                                self.out.push_str(": ");
                                self.print_type(&field_type.0);
                            }
                            self.out.push(',');
                            self.flush_trailing_comment(range.end);
                            prev_end = range.end;
                        }
                        self.flush_comments_before(fields.1.into_range().end, prev_end);
                        self.indent -= 1;
                        self.line();
                        self.out.push('}');
                    }
                }
            }
            // `[resource ]enum Name[<…>] { Variant[(payload)][ = backing value], … }`.
            Node::Enum(name, generics, resource, variants) => {
                if *resource {
                    self.out.push_str("resource ");
                }
                self.out.push_str("enum ");
                self.out.push_str(name.0);
                self.print_generic_parameters(generics);
                if variants.0.is_empty() {
                    self.out.push_str(" {}");
                } else {
                    self.out.push_str(" {");
                    self.indent += 1;
                    let mut prev_end = variants.1.into_range().start + 1;
                    for ((variant_name, payload, backing), span) in &variants.0 {
                        let range = span.into_range();
                        let after_comments = self.flush_comments_before(range.start, prev_end);
                        if self.has_blank_between(after_comments, range.start) {
                            self.blank_line();
                        }
                        self.line();
                        self.out.push_str(variant_name);
                        if !payload.is_empty() {
                            self.out.push('(');
                            for (index, (payload_type, _)) in payload.iter().enumerate() {
                                if index > 0 {
                                    self.out.push_str(", ");
                                }
                                self.print_type(payload_type);
                            }
                            self.out.push(')');
                        }
                        // `Display` reprints the literal exactly as written —
                        // quotes and all for a string backing.
                        if let Some(backing) = backing {
                            self.out.push_str(" = ");
                            self.out.push_str(&backing.to_string());
                        }
                        self.out.push(',');
                        self.flush_trailing_comment(range.end);
                        prev_end = range.end;
                    }
                    self.flush_comments_before(variants.1.into_range().end, prev_end);
                    self.indent -= 1;
                    self.line();
                    self.out.push('}');
                }
            }
            // A block-scoped `use`/`import` (inside a `fn`/`impl`/`mod` body):
            // printed as written, brace set unsorted — a deliberate placement.
            // Top-level import runs print through `print_import_run` instead.
            Node::Use(branch) => {
                self.out.push_str("use ");
                self.print_import_branch(branch, false);
                self.out.push(';');
            }
            Node::Import(branch) => {
                self.out.push_str("import ");
                self.print_import_branch(branch, false);
                self.out.push(';');
            }
            Node::Func(func) => self.print_func(func),
            // `impl Subject[ with A + B] { items }`.
            Node::Impl(subject, traits, body) => {
                self.out.push_str("impl ");
                self.print_type(&subject.0);
                self.print_with_clause(traits);
                self.out.push(' ');
                self.print_braced_items(body);
            }
            // `trait Name[ with A + B] { items }`.
            Node::Trait(name, generics, supertraits, body) => {
                self.out.push_str("trait ");
                self.out.push_str(name.0);
                self.print_generic_parameters(generics);
                self.print_with_clause(supertraits);
                self.out.push(' ');
                self.print_braced_items(body);
            }
            // `[derive(A, B)]` sits on its own line above the item it annotates.
            Node::Derive(names, derived) => {
                self.out.push_str("[derive(");
                let names: Vec<&str> = names.iter().map(|(name, _)| *name).collect();
                self.out.push_str(&names.join(", "));
                self.out.push_str(")]");
                self.line();
                self.print_item(derived);
            }
            // `[service]` / `[service(Client)]` likewise sits above its struct.
            Node::Service(client_name, item) => {
                self.out.push_str("[service");
                if let Some(client_name) = client_name {
                    self.out.push('(');
                    self.out.push_str(client_name);
                    self.out.push(')');
                }
                self.out.push(']');
                self.line();
                self.print_item(item);
            }
            Node::Export(exported) => {
                self.out.push_str("export ");
                self.print_item(exported);
            }
            // `mod name { items }`.
            Node::Module(name, body) => {
                self.out.push_str("mod ");
                self.out.push_str(name);
                self.out.push(' ');
                self.print_braced_items(body);
            }
            // `macro fun name(..) { .. }` — a macro definition. The `macro`
            // keyword then the ordinary function form.
            Node::MacroFun(func) => {
                self.out.push_str("macro ");
                self.print_func(func);
            }
            // `[name(args)?] <item>` — a user macro attribute, on its own line
            // above the struct/enum/function it annotates (like `[derive(..)]`).
            // The optional arguments are SYNTAX — reprinted verbatim from source.
            Node::MacroAttribute(name, _name_span, argument_spans, annotated) => {
                self.out.push('[');
                self.out.push_str(name);
                if !argument_spans.is_empty() {
                    self.out.push('(');
                    self.print_argument_spans(argument_spans);
                    self.out.push(')');
                }
                self.out.push(']');
                self.line();
                self.print_item(annotated);
            }
            // Anything else is an expression appearing as a statement.
            _ => self.print_expr(item),
        }
    }

    /// Prints an import/use path: `a::b::{ c, d }`. When `sort`, a brace set's
    /// branches print in canonical order (`{ c, d }`) — used for top-level
    /// imports; block-scoped imports pass `false` to print them as written.
    /// Prints one import/use statement, re-printing it in split form when its
    /// own line overflows the budget. Both import paths go through here — `fmt`'s
    /// output and Organize Imports — because [`Self::organize_run`] depends on
    /// producing byte-for-byte what `fmt` would; if only one of them split, the
    /// editor action and the formatter would rewrite each other forever.
    ///
    /// The caller has already emitted this line's indentation, so the rollback
    /// keeps it and reprints only the statement.
    fn print_import_statement(&mut self, node: &Node<'src>) {
        let statement_start = self.out.len();
        self.print_import_like(node);
        if self.over_line_budget(statement_start) {
            self.out.truncate(statement_start);
            self.split = Split::Statement;
            self.print_import_like(node);
        }
    }

    fn print_import_branch(&mut self, branch: &ImportBranch<'src>, sort: bool) {
        // Take the pending split here, as `print_expr` does. A path FORWARDS it
        // to the child it leads to — `std::rpc::{…}` breaks at the set, not at
        // the `::` — and the set is what consumes it.
        let split = std::mem::take(&mut self.split);
        match branch {
            ImportBranch::Path(name, _, child) => {
                self.out.push_str(name);
                if let Some(child) = child {
                    self.out.push_str("::");
                    self.split = split;
                    self.print_import_branch(child, sort);
                }
            }
            ImportBranch::Set(branches) => {
                let mut order: Vec<&ImportBranch<'src>> = branches.iter().collect();
                if sort {
                    order.sort_by_cached_key(|branch| branch_key(&branch_from_ast(branch)));
                }
                let member_spans: Vec<Span> = order
                    .iter()
                    .filter_map(|child| Self::branch_span(child))
                    .collect();
                let inside_comment = self
                    .import_set_extent(&member_spans)
                    .is_some_and(|extent| self.comment_outside_elements(extent, &member_spans));
                // A one-member set has a canonical unbraced spelling — `a::{ b }`
                // IS `a::b` — so the braces collapse (kolt.local 005), recursively
                // (`a::{ b::{ c } }` reaches `a::b::c`). Two exceptions keep the
                // braces: `self`, which re-binds the namespace it sits in and only
                // means that inside braces (`Option::{ self }` publishes
                // `Option`), and a comment inside them, which is anchored to the
                // set's split form. `emit_branch_tokens` mirrors this collapse so
                // the safety net reduces both spellings to the same tokens.
                if let [only] = order.as_slice()
                    && !inside_comment
                    && !matches!(only, ImportBranch::Path("self", ..))
                {
                    self.split = split;
                    self.print_import_branch(only, sort);
                    return;
                }
                if !order.is_empty() && (split != Split::Off || inside_comment) {
                    let open = self
                        .import_set_extent(&member_spans)
                        .map_or(0, |extent| extent.into_range().start);
                    self.print_split_import_set(&order, sort, open);
                } else {
                    self.out.push_str("{ ");
                    for (index, child) in order.iter().enumerate() {
                        if index > 0 {
                            self.out.push_str(", ");
                        }
                        self.print_import_branch(child, sort);
                    }
                    self.out.push_str(" }");
                }
            }
        }
    }

    /// The source extent of a brace set, from its `{` to its last member's end.
    /// An `ImportBranch::Set` carries no span of its own, so it is recovered from
    /// the source: the `{` that opens it is the last one before the first member
    /// and after the statement boundary — the search stops at the previous `;` so
    /// a `{` inside an earlier comment cannot be mistaken for it.
    fn import_set_extent(&self, members: &[Span]) -> Option<Span> {
        let first = members.first()?.into_range().start;
        let last = members.last()?.into_range().end;
        let statement_start = self.source[..first].rfind(';').map_or(0, |at| at + 1);
        let open = self.source[statement_start..first].rfind('{')? + statement_start;
        Some(Span::from(open..last))
    }

    /// The source span of an import branch's head name, when it has one — the
    /// anchor a comment written before that member attaches to.
    fn branch_span(branch: &ImportBranch<'src>) -> Option<Span> {
        match branch {
            ImportBranch::Path(_, span, _) => Some(*span),
            ImportBranch::Set(_) => None,
        }
    }

    /// Prints a brace set in split form: one member per line, one indentation
    /// level in, with a trailing comma after every one, and `}` back at the
    /// opening line's indent where the `;` glues. The list literal's rule, on
    /// the one brace list that is not an expression — and the token-level import
    /// grammar allows the trailing comma, so a split run still sorts and
    /// organizes.
    ///
    /// Each member's line is measured in turn, so a nested set that is itself
    /// too wide breaks one level further in. No comment cursor rides along:
    /// comments inside an import run are attached by the run, never here.
    fn print_split_import_set(&mut self, order: &[&ImportBranch<'src>], sort: bool, open: usize) {
        self.out.push('{');
        self.indent += 1;
        let mut prev_end = open;
        for child in order {
            let member_start = self.out.len();
            if let Some(span) = Self::branch_span(child) {
                self.flush_element_comments(span.into_range().start, prev_end);
                prev_end = span.into_range().end;
            }
            self.line();
            let line_start = self.out.len();
            self.print_import_branch(child, sort);
            self.out.push(',');
            if self.over_line_budget(line_start) {
                self.out.truncate(member_start);
                self.line();
                self.split = Split::Tail;
                self.print_import_branch(child, sort);
                self.out.push(',');
            }
        }
        self.indent -= 1;
        self.line();
        self.out.push('}');
    }

    /// Prints a type expression: `i32`, `List<T>`, `Map<str, i32>`, `&mut T`.
    /// Bails (falling `format` back to the source) on any type form not yet handled.
    fn print_type(&mut self, node: &Node<'src>) {
        match node {
            Node::Accessor(name) => self.out.push_str(name),
            Node::AccessorWithGenerics(name, arguments) => {
                self.out.push_str(name);
                self.print_type_arguments(arguments);
            }
            // `style::Style`, `std::reactive::SignalCell<i32>` — a nominal type
            // reached through the modules that declare it (B172). The spine is
            // `StaticAccessor`s, the arguments (if any) sit on the last segment.
            Node::StaticAccessor(namespace, name, arguments) => {
                self.print_type(&namespace.0);
                self.out.push_str("::");
                self.out.push_str(name);
                if let Some(arguments) = arguments {
                    self.print_type_arguments(arguments);
                }
            }
            Node::Reference(mutable, inner) => {
                self.out.push('&');
                if *mutable {
                    self.out.push_str("mut ");
                }
                self.print_type(&inner.0);
            }
            // `async |A| B` / `sync |A| B` — closure-type contract markers.
            Node::AsyncType(inner) => {
                self.out.push_str("async ");
                self.print_type(&inner.0);
            }
            Node::SyncType(inner) => {
                self.out.push_str("sync ");
                self.print_type(&inner.0);
            }
            // `|A, B| Ret` (or `||` for no parameters) — a closure type.
            Node::ClosureType(parameters, return_type) => {
                if parameters.0.is_empty() {
                    self.out.push_str("||");
                } else {
                    self.out.push('|');
                    for (index, (name, parameter_type)) in parameters.0.iter().enumerate() {
                        if index > 0 {
                            self.out.push_str(", ");
                        }
                        if let Some(name) = name {
                            self.out.push_str(name);
                            self.out.push_str(": ");
                        }
                        self.print_type(&parameter_type.0);
                    }
                    self.out.push('|');
                }
                if let Some(return_type) = return_type {
                    self.out.push(' ');
                    self.print_type(&return_type.0);
                }
            }
            // `type T[: A + B]` — a generic binder inside an impl subject pattern.
            Node::TypeBinder(name, bounds) => {
                self.out.push_str("type ");
                self.out.push_str(name);
                self.print_bounds(bounds);
            }
            // `(A, B)` — a tuple type.
            Node::Tuple(elements) => {
                self.out.push('(');
                for (index, (element, _)) in elements.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_type(element);
                }
                self.out.push(')');
            }
            // `[T; n]` — a fixed-length array type (proposal/fixed-arrays.md): the
            // element type and a length (an integer literal). Nests as `[[T; m]; n]`.
            Node::ArrayType(element, length) => {
                self.out.push('[');
                self.print_type(&element.0);
                self.out.push_str("; ");
                self.print_expr(length);
                self.out.push(']');
            }
            // `(|| void) context turn_scope` / `T context (a, b)` — a type with the
            // ambient contexts its value demands (`proposal/ambient-owner.md`). One
            // name may be written bare or parenthesized and both reprint as
            // written: the parser records only the names, so the source decides.
            Node::TypeWithContexts(inner, contexts) => {
                self.print_type(&inner.0);
                self.out.push_str(" context ");
                let parenthesized = contexts.len() > 1
                    || contexts.first().is_some_and(|(_, span)| {
                        self.source
                            .get(inner.1.into_range().end..span.into_range().start)
                            .is_some_and(|between| between.contains('('))
                    });
                if parenthesized {
                    self.out.push('(');
                }
                for (index, (name, _)) in contexts.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.out.push_str(name);
                }
                if parenthesized {
                    self.out.push(')');
                }
            }
            // `(U in T: SignalCell<U>)` — a mapped tuple type: `template` applied to
            // each element of the tuple `source`, with `binder` naming the element
            // type. The parentheses are the form's own, not a group around it.
            Node::MappedType {
                binder,
                source,
                template,
                ..
            } => {
                self.out.push('(');
                self.out.push_str(binder);
                self.out.push_str(" in ");
                self.print_type(&source.0);
                self.out.push_str(": ");
                self.print_type(&template.0);
                self.out.push(')');
            }
            _ => self.bailed = true,
        }
    }

    /// Prints a `<A, B>` generic-argument list on a nominal type.
    fn print_type_arguments(&mut self, arguments: &GenericArguments<'src>) {
        self.out.push('<');
        for (index, (argument, _)) in arguments.0.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            self.print_type(argument);
        }
        self.out.push('>');
    }

    /// Prints a `: A + B` trait-bound list, or nothing when `bounds` is empty.
    fn print_bounds(&mut self, bounds: &[Spanned<Node<'src>>]) {
        if bounds.is_empty() {
            return;
        }
        self.out.push_str(": ");
        for (index, (bound, _)) in bounds.iter().enumerate() {
            if index > 0 {
                self.out.push_str(" + ");
            }
            self.print_type(bound);
        }
    }

    /// Prints a `with A + B` clause (the traits of an `impl`/`trait`), or nothing
    /// when there are none.
    fn print_with_clause(&mut self, traits: &[Spanned<Node<'src>>]) {
        if traits.is_empty() {
            return;
        }
        self.out.push_str(" with ");
        for (index, (trait_, _)) in traits.iter().enumerate() {
            if index > 0 {
                self.out.push_str(" + ");
            }
            self.print_type(trait_);
        }
    }

    /// Prints the `<T, U: Bound = Default>` parameter list of a generic item, or
    /// nothing when there are none.
    fn print_generic_parameters(&mut self, parameters: &Option<GenericParameters<'src>>) {
        let Some((parameters, _)) = parameters else {
            return;
        };
        self.out.push('<');
        for (index, parameter) in parameters.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            if parameter.is_type {
                self.out.push_str("type ");
            }
            self.out.push_str(parameter.name);
            self.print_bounds(&parameter.bounds);
            // `T: (2..)` / `(..10)` / `(..: Display)` — a tuple-arity bound, which
            // REPLACES the trait-bound list rather than joining it, so exactly one
            // of the two prints. Omitted endpoints stay omitted: `(2..)` is not
            // `(2..0)`. This was dropped entirely, which cost `reactive.vl` its
            // `combine<T: (2..)>` and, through the safety net, its whole file.
            if let Some(tuple_bound) = &parameter.tuple_bound {
                self.out.push_str(": (");
                if let Some(lo) = tuple_bound.lo {
                    self.out.push_str(&lo.to_string());
                }
                self.out.push_str("..");
                if let Some(hi) = tuple_bound.hi {
                    self.out.push_str(&hi.to_string());
                }
                if let Some(element) = &tuple_bound.element {
                    self.out.push_str(": ");
                    self.print_type(&element.0);
                }
                self.out.push(')');
            }
            if let Some(default) = &parameter.default {
                self.out.push_str(" = ");
                self.print_type(&default.0);
            }
        }
        self.out.push('>');
    }

    /// Prints a `{ items }` block of declarations (an `impl`/`trait`/`mod` body),
    /// each item on its own line, preserving interior comments and blank lines.
    fn print_braced_items(&mut self, body: &Spanned<NodeList<'src>>) {
        let range = body.1.into_range();
        if body.0.is_empty() && !self.has_comment_in(range.start, range.end) {
            self.out.push_str("{}");
            return;
        }
        self.out.push('{');
        self.indent += 1;
        let prev_end = self.print_items(&body.0, range.start + 1, false);
        self.flush_comments_before(range.end, prev_end);
        self.indent -= 1;
        self.line();
        self.out.push('}');
    }

    /// Prints a function declaration: its `[extern]`/`[must_use]`/`[rpc]`
    /// attributes (if any) each on their own line, then
    /// `[async ][external ]fun name[<…>](…)[: T][ borrows p]` followed by the
    /// body block, or a `;` for a signature with no body.
    fn print_func(&mut self, func: &Func<'src>) {
        if let Some(binding) = &func.extern_binding {
            self.print_extern_attribute(binding, func.extern_retains);
            self.line();
        }
        if func.must_use {
            self.out.push_str("[must_use]");
            self.line();
        }
        if func.rpc {
            self.out.push_str("[rpc]");
            self.line();
        }
        if func.trait_only {
            self.out.push_str("[trait_only]");
            self.line();
        }
        if func.doc_hidden {
            self.out.push_str("[doc(hidden)]");
            self.line();
        }
        if !func.platform_fence.is_empty() {
            let patterns = func
                .platform_fence
                .iter()
                .map(|(pattern, _)| format!("\"{pattern}\""))
                .collect::<Vec<_>>()
                .join(", ");
            self.out.push_str(&format!("[platform({patterns})]"));
            self.line();
        }
        if func.is_async {
            self.out.push_str("async ");
        }
        if func.external {
            self.out.push_str("external ");
        }
        self.out.push_str("fun ");
        self.out.push_str(func.name.0);
        self.print_generic_parameters(&func.generic_parameters);
        self.print_parameters(&func.parameters);
        if let Some(return_type) = &func.return_type {
            self.out.push_str(": ");
            self.print_type(&return_type.0);
        }
        if let Some(borrows) = func.borrows {
            self.out.push_str(" borrows ");
            self.out.push_str(borrows);
        }
        match &func.body {
            Some(body) => {
                self.out.push(' ');
                self.print_block(body);
            }
            None => self.out.push(';'),
        }
    }

    /// Prints a `[extern(..)]` host-binding attribute in its canonical form.
    fn print_extern_attribute(&mut self, binding: &ExternBinding<'src>, retains: bool) {
        self.out.push_str("[extern(");
        match binding {
            ExternBinding::Function {
                module: None,
                symbol,
            } => {
                self.out.push('"');
                self.out.push_str(symbol);
                self.out.push('"');
            }
            ExternBinding::Function {
                module: Some(module),
                symbol,
            } => {
                self.out.push('"');
                self.out.push_str(module);
                self.out.push_str("\", \"");
                self.out.push_str(symbol);
                self.out.push('"');
            }
            ExternBinding::Method { symbol: None } => self.out.push_str("method"),
            ExternBinding::Method {
                symbol: Some(symbol),
            } => {
                self.out.push_str("method, \"");
                self.out.push_str(symbol);
                self.out.push('"');
            }
            ExternBinding::New { module, symbol } => {
                self.out.push_str("new, ");
                if let Some(module) = module {
                    self.out.push('"');
                    self.out.push_str(module);
                    self.out.push_str("\", ");
                }
                self.out.push('"');
                self.out.push_str(symbol);
                self.out.push('"');
            }
            ExternBinding::Get { symbol } => {
                self.out.push_str("get, \"");
                self.out.push_str(symbol);
                self.out.push('"');
            }
            ExternBinding::Set { symbol } => {
                self.out.push_str("set, \"");
                self.out.push_str(symbol);
                self.out.push('"');
            }
        }
        // The retention flag reprints LAST, whatever position it was written in
        // — one canonical form for the round trip.
        if retains {
            self.out.push_str(", retains");
        }
        self.out.push_str(")]");
    }

    /// Prints a `(name: T, &mut self, …)` parameter list. The `&`/`&mut`/`own`
    /// convention prefix is reprinted only when it came from a prefix rather than
    /// the parameter's reference type (which already carries it).
    /// Prints a `fun`'s parameter list, breaking it one parameter per line when
    /// the signature's own line is over the budget (`proposal/signature-layout.md`).
    /// Reached only from [`Self::print_func`] — a closure's parameters go through
    /// [`Self::print_closure_parameters`] and are never broken, being an
    /// expression's own punctuation.
    ///
    /// What follows the `)` — the return type, a `borrows` clause, the body's
    /// `{` or a bodyless `;` — glues to the closing line, exactly as it does
    /// inline; none of it is a list entry.
    fn print_parameters(&mut self, parameters: &Spanned<Vec<crate::node::Parameter<'src>>>) {
        let (parameters, list_span) = (&parameters.0, parameters.1);
        let split = std::mem::take(&mut self.split);
        let parameter_spans: Vec<Span> =
            parameters.iter().map(|parameter| parameter.span).collect();
        if !parameters.is_empty()
            && (split != Split::Off || self.comment_outside_elements(list_span, &parameter_spans))
        {
            self.print_split_parameters(parameters, list_span.into_range().start);
            return;
        }
        self.out.push('(');
        self.print_parameters_inner(parameters);
        self.out.push(')');
    }

    /// The split form: `(` closes the signature's line, every parameter takes its
    /// own line one level in with a trailing comma — the last included, so adding
    /// a parameter is a one-line diff — and `)` returns to the declaration's
    /// indent.
    ///
    /// No line is re-measured here. A parameter is `name: Type` and a type has no
    /// layout of its own, so a parameter too wide for its line has nowhere to
    /// break and simply stays wide — unlike a list element or a struct field,
    /// either of which may be a chain.
    fn print_split_parameters(&mut self, parameters: &[crate::node::Parameter<'src>], open: usize) {
        self.out.push('(');
        self.indent += 1;
        let mut prev_end = open;
        for parameter in parameters {
            self.flush_element_comments(parameter.span.into_range().start, prev_end);
            prev_end = parameter.span.into_range().end;
            self.line();
            self.print_parameters_inner(std::slice::from_ref(parameter));
            self.out.push(',');
        }
        self.indent -= 1;
        self.line();
        self.out.push(')');
    }

    /// Prints the comma-separated parameters themselves, without the surrounding
    /// delimiters (shared by function `(…)` and closure `|…|` lists).
    fn print_parameters_inner(&mut self, parameters: &[crate::node::Parameter<'src>]) {
        for (index, parameter) in parameters.iter().enumerate() {
            let (binder, parameter_type) = (&parameter.pattern, &parameter.declared_type);
            if index > 0 {
                self.out.push_str(", ");
            }
            // `mut` (binder mutability) and the conventions are exclusive by
            // the grammar, so at most one prefix prints.
            if parameter.mutable {
                self.out.push_str("mut ");
            }
            let type_is_reference = matches!(
                parameter_type.as_deref().map(|spanned| &spanned.0),
                Some(Node::Reference(..))
            );
            match parameter.convention {
                Convention::Own => self.out.push_str("own "),
                Convention::Ref if !type_is_reference => self.out.push('&'),
                Convention::RefMut if !type_is_reference => self.out.push_str("&mut "),
                _ => {}
            }
            // `...items` — the spread marker binds to the binder, after any
            // prefix (variadic-generics.md §S). Refused alongside a convention,
            // so only `mut` can precede it.
            if parameter.spread {
                self.out.push_str("...");
            }
            self.print_binder(binder);
            if let Some(parameter_type) = parameter_type {
                self.out.push_str(": ");
                self.print_type(&parameter_type.0);
            }
        }
    }

    /// Prints a braced statement block `{ … }` — the body of a function, loop,
    /// `if`, or a block expression. An empty block (no statements, no tail, no
    /// interior comment) stays inline as `{}`.
    fn print_block(&mut self, block: &Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)>) {
        let range = block.1.into_range();
        let (statements, tail) = &block.0;
        let empty = statements.is_empty() && matches!(tail.0, Node::Void);
        if empty && !self.has_comment_in(range.start, range.end) {
            self.out.push_str("{}");
            return;
        }
        self.out.push('{');
        self.indent += 1;
        let mut prev_end = self.print_items(statements, range.start + 1, false);
        if !matches!(tail.0, Node::Void) {
            let tail_range = tail.1.into_range();
            let after_comments = self.flush_comments_before(tail_range.start, prev_end);
            if self.has_blank_between(after_comments, tail_range.start) {
                self.blank_line();
            }
            self.line();
            // A block's tail expression is a statement position too (it is the
            // block's value), so it takes the same width rule.
            let statement_start = self.out.len();
            let comment_cursor = self.cursor;
            self.print_expr(tail);
            if self.begin_split_reprint(statement_start, comment_cursor) {
                self.print_expr(tail);
                self.split = Split::Off;
            }
            self.flush_trailing_comment(tail_range.end);
            prev_end = tail_range.end;
        }
        self.flush_comments_before(range.end, prev_end);
        self.indent -= 1;
        self.line();
        self.out.push('}');
    }

    /// Whether the next non-trivia character at or after `from` in the source is a
    /// comma — used to preserve a match arm's optional separator comma (which the
    /// AST drops) so either corpus style round-trips.
    fn source_has_comma_at(&self, from: usize) -> bool {
        let bytes = self.source.as_bytes();
        let mut index = from;
        while index < bytes.len() {
            match bytes[index] {
                b' ' | b'\t' | b'\n' | b'\r' => index += 1,
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    while index < bytes.len() && bytes[index] != b'\n' {
                        index += 1;
                    }
                }
                b',' => return true,
                _ => return false,
            }
        }
        false
    }

    /// Whether any extracted comment falls within `[from, to)` — used to decide
    /// whether an otherwise-empty block must expand to carry its comments.
    fn has_comment_in(&self, from: usize, to: usize) -> bool {
        self.comments.iter().any(|(span, _)| {
            let range = span.into_range();
            range.start >= from && range.start < to
        })
    }

    /// The binding precedence of a binary operator (higher binds tighter), used to
    /// decide where operands need parentheses.
    fn binary_precedence(operator: BinaryOp) -> u8 {
        match operator {
            BinaryOp::Or => 0,
            BinaryOp::And => 1,
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Lt
            | BinaryOp::Gt
            | BinaryOp::LtEq
            | BinaryOp::GtEq => 3,
            // Vilan's source order (Rust-style): bitwise binds tighter than
            // comparison, looser than arithmetic. (JS's differs; the
            // transformer's own table handles emission.)
            BinaryOp::BitOr => 4,
            BinaryOp::BitXor => 5,
            BinaryOp::BitAnd => 6,
            BinaryOp::Shl | BinaryOp::Shr | BinaryOp::UShr => 7,
            BinaryOp::Add | BinaryOp::Sub => 8,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 9,
        }
    }

    /// The precedence of an expression as an operand — `100` for atoms/postfix
    /// (never need wrapping), `0` for statement-like forms (always wrapped as an
    /// operand). Mirrors the parser's expression layering.
    fn expression_precedence(node: &Node<'src>) -> u8 {
        match node {
            Node::Binary(operator, _, _) => Self::binary_precedence(*operator),
            Node::Is(_, _) => 2,
            Node::Unary(_, _)
            | Node::Reference(_, _)
            | Node::Dereference(_)
            | Node::Await(_)
            | Node::Async(_) => 10,
            Node::Assign(_, _, _)
            | Node::Let(_, _, _, _)
            | Node::Closure(_)
            | Node::If(_)
            | Node::For(_, _)
            | Node::ForIn(_, _, _)
            | Node::Match(_, _)
            | Node::Jump(_)
            | Node::FuncReturn(_)
            | Node::Const(_) => 0,
            _ => 100,
        }
    }

    /// Prints `expr` as an operand, wrapping it in parentheses when its precedence
    /// is below `minimum` (so the reprint reparses to the same tree). An
    /// interpolated string is reprinted verbatim and never wrapped — it already
    /// carries its own parentheses in the expanded token stream.
    fn print_operand(&mut self, expr: &Spanned<Node<'src>>, minimum: u8) {
        if self.interpolated_source(expr).is_some() {
            self.print_expr(expr);
        } else if Self::expression_precedence(&expr.0) < minimum {
            self.out.push('(');
            self.print_expr(expr);
            self.out.push(')');
        } else {
            self.print_expr(expr);
        }
    }

    /// Prints the subject of a `.member` or `[index]` postfix. A `Lift` (an
    /// `a?.b` chain) greedily absorbs any following `.member` / `[index]` / call
    /// into its continuation, so as a postfix subject it must be parenthesized —
    /// `(a?.b).c`, not `a?.b.c` (which reparses with `.c` pulled inside the
    /// lift). Every other subject follows the ordinary operand rule (min 100): an
    /// atom / call / member / index needs no parens; a binary / `is` / prefix
    /// form gets its source parens back through precedence.
    fn print_postfix_subject(&mut self, subject: &Spanned<Node<'src>>) {
        if matches!(subject.0, Node::Lift(_, _)) {
            self.out.push('(');
            self.print_expr(subject);
            self.out.push(')');
        } else {
            self.print_operand(subject, 100);
        }
    }

    // --- Width-aware layout: chain and list splitting ------------------------
    //
    // The formatter's only width-driven decision, applied RECURSIVELY. A line is
    // printed inline first; if that rendering overflows [`LINE_BUDGET`] the
    // output rolls back and the same code re-prints with a [`Split`] permission
    // armed, which breaks the construct on that line — a postfix chain into one
    // `.name(…)` link per line, a list literal into one element per line.
    // Measuring the real rendering — rather than predicting its width — is what
    // keeps the rule honest: the two forms are the same print, and the choice
    // between them is purely the measured width.
    //
    // The recursion is the same measurement one level in. A split chain gives
    // each link its own continuation line, so each link's line is measured the
    // same way; over budget, the link rolls back and reprints with
    // [`Split::Tail`], which descends through the call's LAST argument until it
    // reaches something breakable — the builder idiom's nested tree
    // (`.child(<tree>)`) or its list (`footer_column("title", [<elements>])`).
    // A split list measures each element's line the same way. Any depth.
    //
    // A binary's operands are both entry points, and the LEFT wins: it prints
    // first, and if it broke, the operator and the right operand take a fresh
    // continuation line. The right operand is then measured on whatever line it
    // landed on — the statement's own when the left stayed inline
    // (`const (art_blob + style()` ⏎ `.raw(…)`), the continuation line when the
    // left broke — and rolls back into a split of its own if that line is over.
    //
    // A rendering that spans lines is measured by its FIRST line, because that
    // is the line the decision is about. A statement carrying a block-bodied
    // closure, a `match` or a block renders as `…prefix… || {` and then more
    // lines; the prefix is a line like any other and breaks like one, while the
    // construct's own body lines are its business and are measured where they
    // are printed. (This used to refuse to measure such a rendering at all,
    // which exempted the whole statement from the budget: a `std::ui` tree
    // ending in `.when(cond, || { … })` stayed inline at any width — one
    // hand-split example collapsed to 707 columns.)
    //
    // What still does not break: an EARLIER argument that is the over-budget
    // cause (R5 — last-argument layout is the universal builder convention;
    // breaking an earlier one needs argument-list layout design) and a `?.`
    // lift chain (it has no postfix spine).

    /// Whether the line printed from output offset `start` — which must be the
    /// first byte after that line's indentation — overflows the line budget.
    ///
    /// A rendering that spans several lines is judged by its first line: the
    /// measured width and the line it describes stay the same thing, which is
    /// the property that keeps the rule honest, and a construct that opens a
    /// line and continues below no longer immunizes everything printed before
    /// it on that line.
    fn over_line_budget(&self, start: usize) -> bool {
        let rendered = &self.out[start..];
        let first_line = rendered.split('\n').next().unwrap_or(rendered);
        self.indent * TAB_COLUMNS + display_width(first_line) > LINE_BUDGET
    }

    /// Whether the line the printer is on right now — everything emitted since
    /// the last newline, that line's own indentation included — overflows the
    /// budget. [`Self::over_line_budget`] is given the offset just past a line's
    /// indentation and adds the *current* level back; this one reads the
    /// indentation from the text instead, so it also measures a line the printer
    /// opened at another level (a binary's continuation line). Used only where a
    /// split is already armed, which is what guarantees the line is a single
    /// line: an arming site never measures a rendering that spans lines.
    fn current_line_over_budget(&self) -> bool {
        let line_start = self.out.rfind('\n').map_or(0, |newline| newline + 1);
        display_width(&self.out[line_start..]) > LINE_BUDGET
    }

    /// Rolls the output and the comment cursor back to the start of the
    /// statement just printed inline and arms the statement-level split, so the
    /// caller can print the same statement again in split form. Returns `false`
    /// — changing nothing — when the statement fits the budget.
    fn begin_split_reprint(&mut self, statement_start: usize, comment_cursor: usize) -> bool {
        if !self.over_line_budget(statement_start) {
            return false;
        }
        self.out.truncate(statement_start);
        self.cursor = comment_cursor;
        self.split = Split::Statement;
        true
    }

    /// The postfix spine of `expr`: its innermost subject, then every postfix
    /// applied to it in application order (innermost first). A `Call` is *not*
    /// a spine step — `style()` is a chain's subject, not a link — and neither
    /// is a `?.` lift chain, which absorbs what follows it.
    fn postfix_spine<'ast>(
        expr: &'ast Spanned<Node<'src>>,
    ) -> (&'ast Spanned<Node<'src>>, Vec<&'ast Spanned<Node<'src>>>) {
        let mut spine = Vec::new();
        let mut subject = expr;
        while let Node::MemberAccessor(inner, _)
        | Node::Index(inner, _)
        | Node::TryAssert(inner)
        | Node::Lifted(inner) = &subject.0
        {
            spine.push(subject);
            subject = inner;
        }
        spine.reverse();
        (subject, spine)
    }

    /// Whether a spine step is a `.name(…)` call link — the unit the split form
    /// gives its own line. Every other postfix (`.field`, `[i]`, `?`, `!`) glues
    /// to the segment printed before it.
    fn is_call_link(node: &Node<'src>) -> bool {
        matches!(node, Node::MemberAccessor(_, member) if matches!(member.0, Node::Call(_, _, _)))
    }

    /// The method name of a `.name(…)` call link, when the link is a plain
    /// method call — no turbofish, and a bare name rather than a computed or
    /// qualified callee. `None` for every other spine step.
    fn call_link_name(step: &Spanned<Node<'src>>) -> Option<&'src str> {
        let Node::MemberAccessor(_, member) = &step.0 else {
            return None;
        };
        let Node::Call(callee, generic_arguments, _) = &member.0 else {
            return None;
        };
        if generic_arguments.is_some() {
            return None;
        }
        match callee.0 {
            Node::Accessor(name) => Some(name),
            _ => None,
        }
    }

    /// Whether `node` is the `style()` builder that opens a sortable chain — the
    /// bare, argument-less call, which is what the token-level
    /// [`starts_style_builder`] recognizes too.
    fn is_style_builder(node: &Node<'src>) -> bool {
        matches!(
            node,
            Node::Call(callee, None, arguments)
                if matches!(callee.0, Node::Accessor("style")) && arguments.0.is_empty()
        )
    }

    /// `expr`'s chain links in the canonical order (see the
    /// canonical-style-chain-order section), or `None` when the chain is not a
    /// sortable `style()` builder or is already canonical — so an unchanged
    /// chain stays on its existing code path, byte for byte.
    ///
    /// Only the LEADING run of `.name(…)` call links sorts. The run stops at the
    /// first postfix that is not a plain method call — `.field`, `[i]`, `?`,
    /// `!`, a turbofish — because those glue to the link before them and past
    /// one the receiver is no longer the `Style` this table describes. The rest
    /// of the spine is returned untouched, in place. The token-level
    /// [`style_chain_links`] stops the run at exactly the same place.
    ///
    /// Refused outright: a chain with a comment anywhere inside it. A reordered
    /// chain would carry its comments to the wrong link, and the comment cursor
    /// only moves forward.
    fn style_sorted_links<'ast>(
        &self,
        expr: &'ast Spanned<Node<'src>>,
    ) -> Option<Vec<&'ast Spanned<Node<'src>>>> {
        let (subject, spine) = Self::postfix_spine(expr);
        if !Self::is_style_builder(&subject.0) {
            return None;
        }
        let names: Vec<&str> = spine
            .iter()
            .map_while(|step| Self::call_link_name(step))
            .collect();
        let span = expr.1.into_range();
        if self.has_comment_in(span.start, span.end) {
            return None;
        }
        let order = style_chain_permutation(&names)?;
        let mut links: Vec<&'ast Spanned<Node<'src>>> =
            order.into_iter().map(|link| spine[link]).collect();
        links.extend(&spine[names.len()..]);
        Some(links)
    }

    /// Whether `expr` is a postfix chain the split form breaks: two or more
    /// `.name(…)` call links. One link is not a chain — breaking it would buy a
    /// line and no clarity.
    fn is_breakable_chain(expr: &Spanned<Node<'src>>) -> bool {
        let (_, spine) = Self::postfix_spine(expr);
        spine
            .iter()
            .filter(|node| Self::is_call_link(&node.0))
            .count()
            >= 2
    }

    /// Prints a postfix chain in split form: the subject stays on the line the
    /// chain started on, and every `.name(…)` link starts a fresh line one
    /// indentation level in, carrying whatever non-call postfixes follow it
    /// (`a.b(x).c` keeps `.c` on `.b(x)`'s line). The statement's terminator is
    /// the caller's, so it glues to the last link. Only ever called for a chain
    /// [`Self::is_breakable_chain`] accepted, so the spine holds at least the
    /// two call links.
    /// Prints a `style()` chain INLINE from an already-ordered link list — the
    /// rendering the recursive `MemberAccessor` arm of [`Self::print_expr`]
    /// produces, with the links taken from `links` instead of from the spine's
    /// own nesting. Only the LAST link carries the caller's split permission,
    /// exactly as only the outermost member does in the recursive form, so the
    /// argument-tail descent reaches the same place.
    fn print_inline_chain(
        &mut self,
        expr: &Spanned<Node<'src>>,
        links: &[&Spanned<Node<'src>>],
        split: Split,
    ) {
        let (subject, _) = Self::postfix_spine(expr);
        self.print_postfix_subject(subject);
        for (at, step) in links.iter().enumerate() {
            self.split = if at + 1 == links.len() {
                split
            } else {
                Split::Off
            };
            self.print_postfix_suffix(step);
        }
        self.split = Split::Off;
    }

    fn print_split_chain(&mut self, expr: &Spanned<Node<'src>>) {
        let (subject, mut spine) = Self::postfix_spine(expr);
        // A `style()` builder's links print in the canonical order, not the
        // written one (kolt.local 006).
        if let Some(sorted) = self.style_sorted_links(expr) {
            spine = sorted;
        }
        // The subject prints exactly as the innermost postfix would print it
        // inline — a `.member`/`[index]` subject through the lift-wrapping rule,
        // a `?`/`!` subject through the plain operand rule.
        match &spine[0].0 {
            Node::MemberAccessor(_, _) | Node::Index(_, _) => self.print_postfix_subject(subject),
            _ => self.print_operand(subject, 100),
        }
        for step in spine {
            if Self::is_call_link(&step.0) {
                self.print_split_link(step);
            } else {
                self.print_postfix_suffix(step);
            }
        }
    }

    /// Prints one `.name(…)` link of a split chain on its own continuation line,
    /// then measures THAT line and — if it overflows the budget — rolls the link
    /// back and reprints it with [`Split::Tail`] armed, so the call's last
    /// argument breaks one indentation level further in. The indent is raised
    /// for the whole link, which is what puts a nested split's own lines one
    /// level past this one.
    ///
    /// What is measured is the link's own rendering: whatever glues AFTER it —
    /// the statement's terminator, the closing paren of an enclosing call or
    /// group, an enclosing list's comma — belongs to a construct that has not
    /// printed yet and is the caller's, exactly as at statement level.
    fn print_split_link(&mut self, step: &Spanned<Node<'src>>) {
        let link_start = self.out.len();
        let comment_cursor = self.cursor;
        self.indent += 1;
        // A comment written before this link attaches ABOVE it, at link indent
        // (`proposal/split-comment-attachment.md` rule B) — the split form is
        // what finally gives it a line of its own.
        let member_start = match &step.0 {
            Node::MemberAccessor(inner, member) => {
                let after_subject = inner.1.into_range().end;
                self.flush_element_comments(member.1.into_range().start, after_subject);
                member.1.into_range().start
            }
            _ => step.1.into_range().start,
        };
        let _ = member_start;
        self.line();
        let line_start = self.out.len();
        self.print_postfix_suffix(step);
        if self.over_line_budget(line_start) {
            self.out.truncate(link_start);
            self.cursor = comment_cursor;
            self.indent -= 1;
            self.print_split_link_retry(step);
            return;
        }
        self.indent -= 1;
    }

    /// The over-budget reprint of one link: same attachment, then the link with
    /// [`Split::Tail`] armed so its last argument breaks one level further in.
    fn print_split_link_retry(&mut self, step: &Spanned<Node<'src>>) {
        self.indent += 1;
        if let Node::MemberAccessor(inner, member) = &step.0 {
            let after_subject = inner.1.into_range().end;
            self.flush_element_comments(member.1.into_range().start, after_subject);
        }
        self.line();
        self.split = Split::Tail;
        self.print_postfix_suffix(step);
        self.indent -= 1;
    }

    /// Prints a list literal in split form: `[` closes the line that opened it,
    /// every element takes its own line one indentation level in with a trailing
    /// comma — the last one included, so adding an element is a one-line diff and
    /// the shape matches what the `std::ui` idiom is already hand-written as —
    /// and `]` returns to the opening line's indent, where the caller's closing
    /// parens and terminator glue after it.
    ///
    /// Each element's line is measured in turn: over budget, the element rolls
    /// back and reprints with [`Split::Tail`] armed, so an element that is itself
    /// a chain breaks with its links one level past the element. The measured
    /// line includes the element's comma — unlike a link's terminator, the comma
    /// is printed by the list itself, onto the element's own line.
    /// Prints an element expression canonically (element-syntax S3). Inline —
    /// `<h2>"Todos"</h2>` — when the children are at most one non-element
    /// child and the rendering stays on its line within budget; otherwise the
    /// children take one line each at +1, `</tag>` back at the element's
    /// indent. A head too wide for the tag line breaks one item per line with
    /// `>` / `/>` at the element's indent (the signature-layout shape).
    /// Self-closing tags space before the slash: `<div />`, never `<div/>`.
    /// The source spans a split element lays out one per line — the tag, each
    /// head item, each child. The gaps between them are where a markup comment
    /// sits (`proposal/split-comment-attachment.md`, extended to elements).
    fn element_item_spans(body: &crate::node::ElementBody<'src>) -> Vec<Span> {
        let mut spans = vec![body.tag];
        for item in &body.head {
            spans.push(match item {
                crate::node::ElementHeadItem::Chain(link) => link.1,
                crate::node::ElementHeadItem::Event((_, name_span), handler) => {
                    (name_span.start..handler.1.end).into()
                }
                crate::node::ElementHeadItem::Attribute(name, value) => {
                    let end = value.as_ref().map(|value| value.1.end).unwrap_or(name.end);
                    (name.start..end).into()
                }
            });
        }
        for child in &body.children {
            spans.push(child.node().1);
        }
        spans
    }

    /// A `css { … }` block, printed canonically (proposal/css-block.md §8, §11
    /// S3 — this arm replaced S2's verbatim source-slice passthrough).
    ///
    /// One item per line, a nested rule's own items one level further in, and
    /// the items in the canonical order the chain sorter uses — derived from the
    /// same tables, through the same order function (see the canonical-css-block
    /// -order section). Blank lines between items are trivia and do not survive:
    /// a sorted body's paragraph gaps would land between items that no longer
    /// belong together, and one canonical shape is the formatter's whole design.
    ///
    /// Whether the file formats at all still rides on this arm existing. There
    /// are three `_ => self.bailed = true` fallbacks, the bail set is asserted
    /// EMPTY by `parse_differential::formatter_never_silently_bails`, and a bail
    /// returns the whole FILE unformatted while `--check` calls it clean.
    fn print_css(&mut self, body: &crate::node::CssBody<'src>) {
        self.out.push_str("css");
        self.print_css_body(body, true);
    }

    /// The items of `body` in the order they print: canonical, or — for a block
    /// holding a comment — exactly as written.
    ///
    /// Refused OUTRIGHT: reordering a body with a comment anywhere inside it,
    /// nested rules included. A permuted body would carry its comments to the
    /// wrong item, because the comment cursor only ever moves forward; this is
    /// the same refusal [`Self::style_sorted_links`] makes for a chain, for the
    /// same reason. Such a block still prints canonically — only the reorder is
    /// off, which is also why the token net needs no comment knowledge: it sorts
    /// both sides and they meet.
    fn css_ordered_items<'ast>(
        &self,
        body: &'ast crate::node::CssBody<'src>,
    ) -> Vec<&'ast crate::node::CssItem<'src>> {
        let items: Vec<&'ast crate::node::CssItem<'src>> = body.items.iter().collect();
        let braces = body.braces.into_range();
        if self.has_comment_in(braces.start, braces.end) {
            return items;
        }
        let ranks: Vec<Option<StyleLinkRank>> = items
            .iter()
            .map(|item| match item {
                crate::node::CssItem::Declaration(declaration) => {
                    css_item_rank(false, &self.source[declaration.property.into_range()])
                }
                crate::node::CssItem::Nested(nested) => css_item_rank(true, nested.name.0),
            })
            .collect();
        match canonical_permutation(&ranks) {
            Some(order) => order.into_iter().map(|at| items[at]).collect(),
            None => items,
        }
    }

    /// A block's or a nested rule's `{ … }`.
    ///
    /// The outer block collapses onto its line when it earns it — one
    /// declaration, no comment, and the rendering stays within the budget, which
    /// is the shape `let active = const css { padding: {space(6)}; };` wants. A
    /// NESTED rule never collapses (`inline_allowed` is false for one): a rule
    /// whose declarations share its line is not CSS, and one shape per construct
    /// beats a shape that depends on how much a rule happens to declare.
    fn print_css_body(&mut self, body: &crate::node::CssBody<'src>, inline_allowed: bool) {
        let items = self.css_ordered_items(body);
        let braces = body.braces.into_range();
        let holds_comment = self.has_comment_in(braces.start, braces.end);
        if items.is_empty() && !holds_comment {
            self.out.push_str(" {}");
            return;
        }
        // More than one item reads as CSS one per line, and a nested rule has a
        // body of its own that never belongs on someone else's line — the same
        // two forcings a split element makes for its children.
        let must_split = !inline_allowed
            || items.len() > 1
            || items
                .iter()
                .any(|item| matches!(item, crate::node::CssItem::Nested(_)))
            || holds_comment;
        if !must_split {
            let start = self.out.len();
            let comment_cursor = self.cursor;
            self.print_css_inline(&items);
            if !self.out[start..].contains('\n') && !self.current_line_over_budget() {
                return;
            }
            self.out.truncate(start);
            self.cursor = comment_cursor;
        }
        self.print_css_split(&items, braces);
    }

    fn print_css_inline(&mut self, items: &[&crate::node::CssItem<'src>]) {
        self.out.push_str(" {");
        for item in items {
            self.out.push(' ');
            self.print_css_item(item);
        }
        self.out.push_str(" }");
    }

    /// One item per line at +1, `}` back at the block's own indent. A comment
    /// attaches to the item it precedes, found through the item's own span —
    /// which is what the parser carried per item from the first commit.
    fn print_css_split(
        &mut self,
        items: &[&crate::node::CssItem<'src>],
        braces: std::ops::Range<usize>,
    ) {
        self.out.push_str(" {");
        self.indent += 1;
        let mut prev_end = braces.start;
        for item in items {
            let span = item.span().into_range();
            self.flush_element_comments(span.start, prev_end);
            self.line();
            self.print_css_item(item);
            prev_end = span.end;
        }
        // A comment after the last item still belongs inside the braces.
        self.flush_element_comments(braces.end, prev_end);
        self.indent -= 1;
        self.line();
        self.out.push('}');
    }

    fn print_css_item(&mut self, item: &crate::node::CssItem<'src>) {
        match item {
            crate::node::CssItem::Declaration(declaration) => {
                self.print_css_declaration(declaration)
            }
            crate::node::CssItem::Nested(nested) => self.print_css_nested(nested),
        }
    }

    /// `property: value;`. The property is a source slice (it spans several
    /// tokens carrying no joined text), and so is every stretch of the value
    /// between holes: a value is CSS, not vilan, so the formatter does not
    /// respace it — a `url("a  b")` would lose its own bytes. What IS
    /// canonicalized is each hole, which is an ordinary vilan expression:
    /// `{space( 4 )}` prints `{space(4)}`.
    fn print_css_declaration(&mut self, declaration: &crate::node::CssDeclaration<'src>) {
        self.out
            .push_str(&self.source[declaration.property.into_range()]);
        self.out.push_str(": ");
        for piece in &declaration.value {
            match piece {
                crate::node::CssValuePiece::Text(text) => {
                    self.out.push_str(&self.source[text.into_range()])
                }
                crate::node::CssValuePiece::Hole(expression, _) => {
                    self.out.push('{');
                    self.print_expr(expression);
                    self.out.push('}');
                }
            }
        }
        self.out.push(';');
    }

    /// `.name { … }` / `.name(a, b) { … }`. The head's arguments are ordinary
    /// vilan expressions, so they print as any call's do.
    fn print_css_nested(&mut self, nested: &crate::node::CssNested<'src>) {
        self.out.push('.');
        self.out.push_str(nested.name.0);
        if !nested.arguments.is_empty() {
            self.out.push('(');
            self.print_expression_list(&nested.arguments);
            self.out.push(')');
        }
        self.print_css_body(&nested.body, false);
    }

    /// A closure whose body is an ELEMENT that splits (E118): the element takes
    /// a line of its own, one level in from the statement, and its children and
    /// closing tag hang off THAT line. Returns whether this arm printed the
    /// body — `false` leaves the caller to print it inline as before.
    ///
    /// The exhibit is `overlays.attach(submenu, || <div .styled(s)>` with its
    /// children and `</div>` below. Left inline, the split element inherits the
    /// STATEMENT's indent for its children and its close, while its opening tag
    /// starts wherever `|| ` happened to end — three anchors that answer to
    /// nothing in common, and the wider the head the further apart they drift.
    /// Breaking after `|| ` collapses them to one: the open tag, the close tag
    /// and the children are all measured from a single column, which is exactly
    /// how a BLOCK body already reads (`print_block` puts its statements one
    /// level past the line the `{` opened and its `}` back on it).
    ///
    /// Width decides, as everywhere else. The element is printed inline first,
    /// and only a rendering that actually broke is rolled back and re-printed —
    /// so `|t| <li>{t}</li>` keeps its line, and the pins that hold an
    /// expression-bodied closure argument inline keep holding.
    fn print_closure_element_body(&mut self, body: &Spanned<Node<'src>>) -> bool {
        if !matches!(body.0, Node::Element(_)) {
            return false;
        }
        let inline_start = self.out.len();
        let comment_cursor = self.cursor;
        self.out.push(' ');
        self.print_expr(body);
        if !self.out[inline_start..].contains('\n') {
            return true;
        }
        self.out.truncate(inline_start);
        self.cursor = comment_cursor;
        self.indent += 1;
        self.line();
        self.print_expr(body);
        self.indent -= 1;
        true
    }

    fn print_element(&mut self, body: &crate::node::ElementBody<'src>) {
        // A comment between the element's items forces the split — collapsed,
        // there is no line to keep it on — and the split loops below attach it
        // to the item it precedes, like every other splittable construct.
        let must_split = body.children.len() > 1
            || body
                .children
                .first()
                .is_some_and(|child| matches!(child.node().0, Node::Element(_)))
            || self.comment_between_elements(&Self::element_item_spans(body));
        if !must_split {
            let element_start = self.out.len();
            let comment_cursor = self.cursor;
            self.print_element_inline(body);
            if !self.out[element_start..].contains('\n') && !self.current_line_over_budget() {
                return;
            }
            self.out.truncate(element_start);
            self.cursor = comment_cursor;
        }
        self.print_element_split(body);
    }

    fn print_element_inline(&mut self, body: &crate::node::ElementBody<'src>) {
        self.out.push('<');
        self.out.push_str(&self.source[body.tag.into_range()]);
        for item in &body.head {
            self.out.push(' ');
            self.print_element_head_item(item);
        }
        if body.self_closing {
            self.out.push_str(" />");
            return;
        }
        self.out.push('>');
        for (index, child) in body.children.iter().enumerate() {
            if index > 0 {
                self.out.push(' ');
            }
            self.print_element_child(child);
        }
        self.out.push_str("</");
        self.out.push_str(&self.source[body.tag.into_range()]);
        self.out.push('>');
    }

    fn print_element_split(&mut self, body: &crate::node::ElementBody<'src>) {
        // Head-item source spans, for comment attachment between items.
        let head_spans: Vec<Span> = {
            let all = Self::element_item_spans(body);
            all[..1 + body.head.len()].to_vec()
        };
        // A comment between head items forces the item-per-line head — inline,
        // it has no line of its own.
        let comment_in_head = self.comment_between_elements(&head_spans);
        // The head: on the tag line while it fits; one item per line otherwise,
        // measured by rendering it and looking, like every width decision.
        let head_start = self.out.len();
        let comment_cursor = self.cursor;
        self.out.push('<');
        self.out.push_str(&self.source[body.tag.into_range()]);
        for item in &body.head {
            self.out.push(' ');
            self.print_element_head_item(item);
        }
        let head_wide = self.out[head_start..].contains('\n') || self.current_line_over_budget();
        let split_head = (head_wide || comment_in_head) && !body.head.is_empty();
        if split_head {
            self.out.truncate(head_start);
            self.cursor = comment_cursor;
            self.out.push('<');
            self.out.push_str(&self.source[body.tag.into_range()]);
            self.indent += 1;
            let mut prev_end = body.tag.end;
            for (item, item_span) in body.head.iter().zip(head_spans[1..].iter()) {
                let item_start = self.out.len();
                let item_cursor = self.cursor;
                self.flush_element_comments(item_span.start, prev_end);
                self.line();
                let line_start = self.out.len();
                self.print_element_head_item(item);
                if self.over_line_budget(line_start) {
                    self.out.truncate(item_start);
                    self.cursor = item_cursor;
                    self.flush_element_comments(item_span.start, prev_end);
                    self.line();
                    self.split = Split::Tail;
                    self.print_element_head_item(item);
                }
                prev_end = item_span.end;
            }
            self.indent -= 1;
            self.line();
        }
        if body.self_closing {
            if split_head {
                self.out.push_str("/>");
            } else {
                self.out.push_str(" />");
            }
            return;
        }
        self.out.push('>');
        self.indent += 1;
        let mut prev_end = head_spans.last().map(|span| span.end).unwrap_or(0);
        for child in &body.children {
            let child_span = child.node().1;
            let child_start = self.out.len();
            let child_cursor = self.cursor;
            self.flush_element_comments(child_span.start, prev_end);
            self.line();
            let line_start = self.out.len();
            self.print_element_child(child);
            if self.over_line_budget(line_start) {
                self.out.truncate(child_start);
                self.cursor = child_cursor;
                self.flush_element_comments(child_span.start, prev_end);
                self.line();
                self.split = Split::Tail;
                self.print_element_child(child);
            }
            prev_end = child_span.end;
        }
        self.indent -= 1;
        self.line();
        self.out.push_str("</");
        self.out.push_str(&self.source[body.tag.into_range()]);
        self.out.push('>');
    }

    fn print_element_head_item(&mut self, item: &crate::node::ElementHeadItem<'src>) {
        match item {
            crate::node::ElementHeadItem::Chain(link) => {
                self.out.push('.');
                self.print_expr(link);
            }
            crate::node::ElementHeadItem::Event((name, _), handler) => {
                self.out.push_str("on:");
                self.out.push_str(name);
                self.out.push('(');
                self.print_expr(handler);
                self.out.push(')');
            }
            crate::node::ElementHeadItem::Attribute(name, value) => {
                self.out.push_str(&self.source[name.into_range()]);
                if let Some(value) = value {
                    self.out.push('(');
                    self.print_expr(value);
                    self.out.push(')');
                }
            }
        }
    }

    fn print_element_child(&mut self, child: &crate::node::ElementChild<'src>) {
        match child {
            crate::node::ElementChild::Hole(inner) => {
                self.out.push('{');
                self.print_expr(inner);
                self.out.push('}');
            }
            crate::node::ElementChild::Bare(inner) => self.print_expr(inner),
        }
    }

    fn print_split_list(&mut self, elements: &[Spanned<Node<'src>>], open: usize) {
        self.out.push('[');
        self.indent += 1;
        let mut prev_end = open;
        for element in elements {
            let element_start = self.out.len();
            let comment_cursor = self.cursor;
            self.flush_element_comments(element.1.into_range().start, prev_end);
            prev_end = element.1.into_range().end;
            self.line();
            let line_start = self.out.len();
            self.print_expr(element);
            self.out.push(',');
            if self.over_line_budget(line_start) {
                self.out.truncate(element_start);
                self.cursor = comment_cursor;
                self.flush_element_comments(element.1.into_range().start, prev_end);
                self.line();
                self.split = Split::Tail;
                self.print_expr(element);
                self.out.push(',');
            }
        }
        self.indent -= 1;
        self.line();
        self.out.push(']');
    }

    /// Prints a struct literal in split form: `{` closes the line that opened
    /// it, every field takes its own line one indentation level in with a
    /// trailing comma — the last one included, for the same reason a split list
    /// carries one — and `}` returns to the opening line's indent, where the
    /// caller's closing parens and terminator glue after it.
    ///
    /// A struct literal is a braced field list, so it breaks on exactly the rule
    /// [`Self::print_split_list`] applies to a bracketed element list: each
    /// field's line is measured in turn, and over budget the field rolls back
    /// and reprints with [`Split::Tail`] armed, so a field whose value is itself
    /// a chain or a list breaks with its own lines one level past the field. The
    /// measured line includes the field's comma — the literal prints it, onto
    /// the field's own line.
    fn print_split_struct(
        &mut self,
        fields: &[Spanned<StructInitializerField<'src>>],
        open: usize,
    ) {
        self.out.push_str(" {");
        self.indent += 1;
        let mut prev_end = open;
        for ((field_name, value), span) in fields {
            let field_start = self.out.len();
            let comment_cursor = self.cursor;
            self.flush_element_comments(span.into_range().start, prev_end);
            prev_end = span.into_range().end;
            self.line();
            let line_start = self.out.len();
            self.print_struct_field(field_name, value);
            self.out.push(',');
            if self.over_line_budget(line_start) {
                self.out.truncate(field_start);
                self.cursor = comment_cursor;
                self.flush_element_comments(span.into_range().start, prev_end);
                self.line();
                self.split = Split::Tail;
                self.print_struct_field(field_name, value);
                self.out.push(',');
            }
        }
        self.indent -= 1;
        self.line();
        self.out.push('}');
    }

    /// Prints one `name = value` field of a struct literal, in either form. Takes
    /// the pending split the way `print_expr` does, so a shorthand field — which
    /// has no value position to hand it to — drops it rather than leaking an
    /// armed split onto whatever prints next.
    fn print_struct_field(&mut self, field_name: &str, value: &Option<Spanned<Node<'src>>>) {
        let split = std::mem::take(&mut self.split);
        self.out.push_str(field_name);
        if let Some(value) = value {
            self.out.push_str(" = ");
            self.split = split;
            self.print_expr(value);
        }
    }

    /// Whether a standalone comment sits in one of the GAPS between the source
    /// spans in `elements` — the trigger for forcing a construct into its split
    /// form (`proposal/split-comment-attachment.md` rule A). A collapsed
    /// construct has no line to hold such a comment, so it would be flushed
    /// below the whole statement, orphaned from what it explains.
    ///
    /// The gaps, not the construct's whole span: a comment inside an element —
    /// a closure body a chain link carries, say — belongs to that body and
    /// already prints where it was written.
    fn comment_between_elements(&self, elements: &[Span]) -> bool {
        elements
            .windows(2)
            .any(|pair| self.has_comment_in(pair[0].into_range().end, pair[1].into_range().start))
    }

    /// Whether a standalone comment sits inside `construct` but outside every one
    /// of its `elements` — before the first, between two, or after the last.
    ///
    /// This is [`Self::comment_between_elements`] generalized to constructs whose
    /// elements do not begin at the construct's own start: `Name { // note` puts
    /// the comment before the first FIELD, which no between-elements gap covers.
    /// A chain needs no such boundary because its subject is its first element.
    fn comment_outside_elements(&self, construct: Span, elements: &[Span]) -> bool {
        let outer = construct.into_range();
        self.comments.iter().any(|(span, _)| {
            let at = span.into_range().start;
            at >= outer.start
                && at < outer.end
                && !elements
                    .iter()
                    .any(|element| element.into_range().contains(&at))
        })
    }

    /// Emits the standalone comments preceding `element_start` on their own
    /// lines at the current (element) indentation, then returns the offset the
    /// caller should treat as the previous end. The split printers call this
    /// before each element, which is what attaches a comment to the element it
    /// precedes instead of letting it fall out below the statement.
    fn flush_element_comments(&mut self, element_start: usize, prev_end: usize) -> usize {
        self.flush_comments_before(element_start, prev_end)
    }

    /// The source spans a split chain lays out one per line: its subject, then
    /// each `.name(…)` call link. The gaps between them are where a mid-chain
    /// comment sits.
    fn chain_element_spans(expr: &Spanned<Node<'src>>) -> Vec<Span> {
        let (subject, spine) = Self::postfix_spine(expr);
        let mut spans = vec![subject.1];
        for step in spine {
            if let Node::MemberAccessor(_, member) = &step.0
                && matches!(member.0, Node::Call(_, _, _))
            {
                spans.push(member.1);
            }
        }
        spans
    }

    /// Whether `expr`'s chain must break because a comment sits between two of
    /// its links (`proposal/split-comment-attachment.md` rule A). Collapsed,
    /// there is no line to keep that comment on and it falls out below the
    /// statement.
    fn chain_has_comment_between_links(&self, expr: &Spanned<Node<'src>>) -> bool {
        self.comment_between_elements(&Self::chain_element_spans(expr))
    }

    /// Whether `expr`'s chain must break regardless of width
    /// (`proposal/chain-seam-split.md`): a call link that is NOT the chain's last
    /// renders across lines, so its closing `})` lands on a line that then
    /// continues with more chain — the seam.
    ///
    /// The last link is excluded because it has no seam: when the chain ends at
    /// its spanning link, the `})` closes the statement and nothing follows it,
    /// which is the ordinary trailing-closure idiom and is already readable.
    fn chain_has_spanning_seam(&mut self, expr: &Spanned<Node<'src>>) -> bool {
        // A probe already in progress renders its subtree once, with no further
        // seam checks inside it. This bounds the cost and changes no answer: a
        // nested chain only seam-splits when a body already spans lines, which
        // the probe sees whether or not that inner split happens.
        if self.probing {
            return false;
        }
        let (_, spine) = Self::postfix_spine(expr);
        let last_call = spine.iter().rposition(|step| Self::is_call_link(&step.0));
        for (index, step) in spine.iter().enumerate() {
            if !Self::is_call_link(&step.0) || Some(index) == last_call {
                continue;
            }
            if self.link_spans_lines(step) {
                return true;
            }
        }
        false
    }

    /// Renders one chain link and reports whether it spans lines, then takes the
    /// rendering back out. Measured rather than predicted from the AST, for the
    /// reason the width rule measures: only the printer knows what the printer
    /// will do. Everything the render touched — output, comment cursor, bail
    /// flag, pending split — is restored, so a probe leaves no trace.
    fn link_spans_lines(&mut self, step: &Spanned<Node<'src>>) -> bool {
        let start = self.out.len();
        let cursor = self.cursor;
        let bailed = self.bailed;
        let split = self.split;
        self.probing = true;
        self.print_postfix_suffix(step);
        self.probing = false;
        let spans = self.out[start..].contains('\n');
        self.out.truncate(start);
        self.cursor = cursor;
        self.bailed = bailed;
        self.split = split;
        spans
    }

    /// Whether `expr` renders across lines. Measured by rendering it and looking,
    /// with the probe discipline [`Self::link_spans_lines`] uses: everything the
    /// render touched is restored, and probes do not nest.
    fn expr_spans_lines(&mut self, expr: &Spanned<Node<'src>>) -> bool {
        if self.probing {
            return false;
        }
        let start = self.out.len();
        let cursor = self.cursor;
        let bailed = self.bailed;
        let split = self.split;
        self.probing = true;
        self.print_expr(expr);
        self.probing = false;
        let spans = self.out[start..].contains('\n');
        self.out.truncate(start);
        self.cursor = cursor;
        self.bailed = bailed;
        self.split = split;
        spans
    }

    /// Whether a list literal must break because one of its elements renders
    /// across lines (`proposal/composite-spanning-split.md`).
    ///
    /// ANY element, where a chain needs a NON-FINAL link: a composite's closing
    /// delimiter always follows its last element — and usually an enclosing `)`
    /// and `;` after that — so there is no position in which a spanning element
    /// leaves a clean line. `{ id, notify = || { … } }` closes on `} });`.
    fn any_element_spans_lines(&mut self, elements: &[Spanned<Node<'src>>]) -> bool {
        elements
            .iter()
            .any(|element| self.expr_spans_lines(element))
    }

    /// The same, over a struct literal's field values.
    fn any_field_spans_lines(&mut self, fields: &[Spanned<StructInitializerField<'src>>]) -> bool {
        fields.iter().any(|((_, value), _)| {
            value
                .as_ref()
                .is_some_and(|value| self.expr_spans_lines(value))
        })
    }

    /// Prints just the postfix a spine step applies — its subject is already
    /// printed. Mirrors the four postfix arms of `print_expr`.
    fn print_postfix_suffix(&mut self, step: &Spanned<Node<'src>>) {
        match &step.0 {
            Node::MemberAccessor(_, member) => {
                self.out.push('.');
                self.print_expr(member);
            }
            Node::Index(_, index) => {
                self.out.push('[');
                self.print_expr(index);
                self.out.push(']');
            }
            Node::TryAssert(_) => self.out.push('!'),
            Node::Lifted(_) => self.out.push('?'),
            // `postfix_spine` yields only the four forms above.
            _ => self.bailed = true,
        }
    }

    /// Prints `expr` as an operand (see [`Self::print_operand`]), handing an
    /// armed split down to it only when the operand needs no parentheses of its
    /// own — splitting inside parentheses would strand the closing paren on a
    /// continuation line. This is the single way the split reaches past a
    /// statement's prefix (`let x = const …`, `ret …`, `await …`) and into the
    /// left operand of a binary.
    fn print_split_operand(&mut self, expr: &Spanned<Node<'src>>, minimum: u8, split: Split) {
        self.split = if Self::expression_precedence(&expr.0) >= minimum {
            split
        } else {
            Split::Off
        };
        self.print_operand(expr, minimum);
    }

    /// Prints the RIGHT operand of a binary expression, and — when the line it
    /// landed on then overflows the budget — rolls it back and reprints it with
    /// the split armed, so a breakable chain to the right of an operator breaks
    /// exactly as one to the left already does (`const (art_blob + style()` ⏎
    /// `.raw("left", "30%")` ⏎ …).
    ///
    /// This is the statement rule applied one operand in: print inline, measure
    /// the real line, reprint with the flag — never a prediction of what would
    /// fit, and never an AST test for what could break, so a reprint that finds
    /// nothing to split reproduces the inline bytes exactly. The left operand
    /// keeps winning when both could break, because it prints first and a left
    /// that broke puts this operand on a fresh continuation line, which is then
    /// measured on its own.
    ///
    /// What is measured is the line through this operand: whatever glues after
    /// it — the statement's terminator, the closing paren of an enclosing group
    /// — belongs to a construct that has not printed yet and is the caller's,
    /// exactly as for a chain link.
    fn print_split_right(&mut self, right: &Spanned<Node<'src>>, minimum: u8, split: Split) {
        if split == Split::Off {
            self.print_operand(right, minimum);
            return;
        }
        let right_start = self.out.len();
        let comment_cursor = self.cursor;
        self.print_operand(right, minimum);
        if self.current_line_over_budget() {
            self.out.truncate(right_start);
            self.cursor = comment_cursor;
            self.print_split_operand(right, minimum, split);
        }
    }

    /// Prints a comma-separated list of macro arguments, each reprinted VERBATIM
    /// from its source span. A macro's arguments are syntax (the parser keeps only
    /// their spans, not a tree), so — like an interpolated string — they are
    /// recovered from the source text rather than rebuilt. Whitespace inside an
    /// argument is preserved; the separator is normalized to `, `.
    fn print_argument_spans(&mut self, argument_spans: &[Span]) {
        for (index, span) in argument_spans.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            let range = span.into_range();
            self.out.push_str(&self.source[range]);
        }
    }

    /// Prints a comma-separated expression list (list/tuple elements) inline.
    fn print_expression_list(&mut self, elements: &[Spanned<Node<'src>>]) {
        for (index, element) in elements.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            self.print_expr(element);
        }
    }

    /// Prints a call's arguments inline, handing an armed [`Split::Tail`] to the
    /// LAST argument and only that one. A call's layout hangs off its final
    /// argument — that is the universal builder/DSL convention (`.child(<tree>)`,
    /// `footer_column("title", [<elements>])`), and it is what lets the descent
    /// walk `.child(footer_column(t, [..]))` down to the list. An earlier
    /// argument that is the over-budget cause keeps the pre-recursion behavior:
    /// the line stays long, because breaking there needs argument-list layout.
    /// A [`Split::Statement`] arming stops here too — a chain nested in an
    /// argument of a statement that is not itself a chain stays inline.
    fn print_call_arguments(&mut self, arguments: &[Spanned<Node<'src>>], split: Split) {
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            // Either permission descends through the LAST argument
            // (`proposal/argument-tail-descent.md`). Only the last: R5 stands,
            // so an earlier argument that is the over-budget cause still leaves
            // a long line.
            if split != Split::Off && index + 1 == arguments.len() {
                self.split = Split::Tail;
            }
            self.print_expr(argument);
        }
    }

    /// If `expr`'s source span is an `i"..."` interpolated string — which the
    /// lexer rewrites into a parenthesized `("" + parts..)` concatenation before
    /// parsing, with every produced token sharing the literal's span — return the
    /// literal's original source text. Reprinting that verbatim is exact;
    /// rebuilding it from the expanded AST would have to re-derive the lexer's
    /// brace/quote escaping.
    fn interpolated_source(&self, expr: &Spanned<Node<'src>>) -> Option<&'src str> {
        let range = expr.1.into_range();
        if self.source.get(range.start..range.start + 2) != Some("i\"") {
            return None;
        }
        // A concatenation's span ends at the last token IT consumed — which, when
        // the literal ends with a hole, is the hole's `}` and not the literal's
        // closing delimiter (the wrapper `)` belongs to the group the parser
        // dissolves). So the span's end may fall SHORT, and the real close has to
        // be recovered from the source.
        //
        // Only ever EXTEND, never shorten. This function is consulted for every
        // expression node, and a node whose span merely STARTS at an i-string is
        // not necessarily the literal: `i"…" + "t"`, `i"…".len()` and every other
        // left-headed compound share that start. Shortening one to the literal
        // drops the rest of the expression's tokens, the safety net sees a token
        // stream that no longer matches, and `format` returns the whole FILE's
        // original bytes — a silent whole-file bail that `--check` calls clean.
        // Taking the larger end keeps the property the single-quoted arms always
        // had: the slice is the whole expression's source, uncanonicalized but
        // token-complete.
        let end = if self.source.get(range.start..range.start + 4) == Some("i\"\"\"") {
            // The triple-quoted body is raw and runs to the first `"""` (backlog
            // H7 — the same scan the lexer makes).
            let body_start = range.start + 4;
            let close = body_start + self.source.get(body_start..)?.find("\"\"\"")? + 3;
            close.max(range.end)
        } else if self.source.as_bytes().get(range.end) == Some(&b'"') {
            // The single-quoted form is short by exactly its closing quote.
            range.end + 1
        } else {
            range.end
        };
        self.source.get(range.start..end)
    }

    /// Prints any expression. Sets `bailed` for forms not yet handled.
    fn print_expr(&mut self, expr: &Spanned<Node<'src>>) {
        // Take the pending split permission here, so that every form *drops* it
        // by default and only the arms below re-arm it explicitly (via
        // `print_split_operand` for the operand that continues the measured
        // line, `print_call_arguments` for the tail descent). Forgetting one of
        // those can only lose a split, never produce one where the width rule
        // says there should be none.
        let split = std::mem::take(&mut self.split);
        if let Some(interpolated) = self.interpolated_source(expr) {
            self.out.push_str(interpolated);
            return;
        }
        // Two doors into the split form: the width rule armed a split, or the
        // chain carries a `})` seam (`proposal/chain-seam-split.md`).
        if Self::is_breakable_chain(expr)
            && (split != Split::Off
                || self.chain_has_comment_between_links(expr)
                || self.chain_has_spanning_seam(expr))
        {
            self.print_split_chain(expr);
            return;
        }
        // A `style()` builder chain that the canonical order PERMUTES cannot go
        // through the recursive `MemberAccessor` arm below, which prints the
        // spine in its written order. It gets the same inline rendering, link by
        // link, off the sorted spine. A chain the order leaves alone falls
        // through untouched, so nothing else in the formatter moves.
        if let Some(links) = self.style_sorted_links(expr) {
            self.print_inline_chain(expr, &links, split);
            return;
        }
        match &expr.0 {
            Node::Number(whole, fraction, suffix) => {
                self.out.push_str(whole);
                if let Some(fraction) = fraction {
                    self.out.push('.');
                    self.out.push_str(fraction);
                }
                if let Some(suffix) = suffix {
                    self.out.push_str(suffix);
                }
            }
            Node::String(text) => {
                self.out.push('"');
                self.out.push_str(text);
                self.out.push('"');
            }
            // A triple-quoted string reprints VERBATIM: its inner whitespace is
            // semantic (the closing delimiter's indentation is the trim prefix),
            // so the formatter must never re-indent it.
            Node::MultilineString(text) => {
                self.out.push_str("\"\"\"");
                self.out.push_str(text);
                self.out.push_str("\"\"\"");
            }
            Node::Bool(value) => self.out.push_str(if *value { "true" } else { "false" }),
            Node::Null => self.out.push_str("null"),
            // `void` written as a value prints as `void`. The parser also
            // SYNTHESIZES a `Void` for a block that ends without a tail
            // expression, and that one is not text — it prints as nothing. The
            // two are told apart by span: a synthesized tail is zero-width at the
            // block's end, a written `void` covers its four characters. Printing
            // both as nothing dropped the argument from `Verdict::Bad(void)`,
            // which the safety net then caught as token drift — silently
            // returning `option.vl` unformatted, forever.
            Node::Void => {
                if !expr.1.into_range().is_empty() {
                    self.out.push_str("void");
                }
            }
            Node::Accessor(name) => self.out.push_str(name),
            Node::AccessorWithGenerics(name, arguments) => {
                self.out.push_str(name);
                self.out.push('<');
                for (index, (argument, _)) in arguments.0.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_type(argument);
                }
                self.out.push('>');
            }
            Node::MemberAccessor(subject, member) => {
                self.print_postfix_subject(subject);
                self.out.push('.');
                // The callee of `list.push(…)` IS the member, so a permission
                // dropped here never reaches the call's arguments — which made
                // a method call's tail unreachable from statement level however
                // far `print_call_arguments` was widened.
                self.split = split;
                self.print_expr(member);
            }
            // An expression path never carries generic arguments on its member
            // (they belong to the call that follows), so there are none to print
            // here; the type printer below has the arm that does.
            Node::StaticAccessor(subject, member, _) => {
                self.print_operand(subject, 100);
                self.out.push_str("::");
                self.out.push_str(member);
            }
            Node::Index(subject, index) => {
                self.print_postfix_subject(subject);
                self.out.push('[');
                self.print_expr(index);
                self.out.push(']');
            }
            Node::Call(callee, generic_arguments, arguments) => {
                // A call binds tighter than `.`/`[]`, so `a.b(c)` parses as
                // `a.(b(c))`. To call the *result* of a member/index access the
                // callee must be parenthesized — `(a.b)(c)` — or it reparses wrong.
                // A `?.` lift chain likewise absorbs a following call into its
                // continuation, so a `Lift` callee needs its own parens: `(a?.b)()`.
                if matches!(
                    callee.0,
                    Node::MemberAccessor(_, _) | Node::Index(_, _) | Node::Lift(_, _)
                ) {
                    self.out.push('(');
                    self.print_expr(callee);
                    self.out.push(')');
                } else {
                    self.print_operand(callee, 100);
                }
                if let Some((generic_arguments, _)) = generic_arguments {
                    self.out.push('<');
                    for (index, (argument, _)) in generic_arguments.iter().enumerate() {
                        if index > 0 {
                            self.out.push_str(", ");
                        }
                        self.print_type(argument);
                    }
                    self.out.push('>');
                }
                self.out.push('(');
                self.print_call_arguments(&arguments.0, split);
                self.out.push(')');
            }
            Node::Binary(operator, left, right) => {
                let precedence = Self::binary_precedence(*operator);
                let left_start = self.out.len();
                self.print_split_operand(left, precedence, split);
                // A split that broke the left operand across lines continues
                // here: the operator and the right operand take their own line
                // at the links' indentation (`…margin(space(0))` ⏎ `+ reveal`).
                // The split is the only thing that can have introduced a break —
                // a line whose inline rendering already spanned lines never
                // splits. The indent stays raised for the right operand, so a
                // break of its OWN lands one level past that continuation line.
                let continued = split != Split::Off && self.out[left_start..].contains('\n');
                if continued {
                    self.indent += 1;
                    self.line();
                } else {
                    self.out.push(' ');
                }
                self.out.push_str(binary_operator_symbol(*operator));
                self.out.push(' ');
                self.print_split_right(right, precedence + 1, split);
                if continued {
                    self.indent -= 1;
                }
            }
            // A prefix operator (`-x`, `!x`, `&x`, `*x`, `await x`) binds tighter
            // than every binary operator (the parser recurses on the unary chain
            // for the operand), so a binary operand must be parenthesized to
            // reparse the same way — `-(2 + 3)`, not `-2 + 3`. Operand minimum 10
            // (the prefix tier in `expression_precedence`) wraps every binary
            // (precedence 0–9) while leaving a nested prefix (`- -x`) and atoms
            // unwrapped.
            Node::Unary(operator, operand) => {
                self.out.push(*operator);
                self.print_split_operand(operand, 10, split);
            }
            Node::TryAssert(subject) => {
                self.print_operand(subject, 100);
                self.out.push('!');
            }
            Node::Lift(subject, continuation) => {
                // `a?.b.c`: the subject, `?`, then the continuation — whose
                // innermost `LiftBinder` prints nothing, so its leading
                // `.member` renders right after the `?`.
                self.print_operand(subject, 100);
                self.out.push('?');
                self.print_expr(continuation);
            }
            Node::LiftBinder => {}
            // A bare-`?` expression-lifting mark — the formatter parses raw
            // trees (the region rewrite runs only at the analyzer's entry),
            // so the mark prints back exactly as written.
            Node::Lifted(subject) => {
                self.print_operand(subject, 100);
                self.out.push('?');
            }
            // A recorded paren group — the parentheses were written, so they
            // reprint as written. (The formatter parses in group-preserving
            // mode, so this covers every `(…)`, not only the region-delimiting
            // ones the compiler's parse records.) An armed split is handed to
            // the interior: the group is the source's own, so the chain inside
            // it breaks and the closing paren glues after the last line —
            // unlike a paren the PRINTER adds, which `print_split_operand`
            // refuses to split inside.
            Node::LiftGroup(inner) => {
                self.out.push('(');
                self.split = split;
                self.print_expr(inner);
                self.out.push(')');
            }
            // Rewrite output — never present in the formatter's raw parse.
            Node::LiftRegion(..) | Node::LiftHole(_) => {
                unreachable!("lift regions exist only after the analyzer-entry rewrite")
            }
            Node::Reference(mutable, operand) => {
                self.out.push('&');
                if *mutable {
                    self.out.push_str("mut ");
                }
                self.print_split_operand(operand, 10, split);
            }
            Node::Dereference(operand) => {
                self.out.push('*');
                self.print_split_operand(operand, 10, split);
            }
            // `..e` — a tuple-value spread (variadic-generics.md §T). The operand
            // is printed WITHOUT the operand rule's parentheses: `..` takes the
            // whole following expression in the parser, so a wrap would be token
            // drift, not a faithful reprint. The split is handed down so a long
            // spread operand still breaks.
            Node::Spread(operand) => {
                self.out.push_str("..");
                self.split = split;
                self.print_expr(operand);
            }
            Node::Await(operand) => {
                self.out.push_str("await ");
                self.print_split_operand(operand, 10, split);
            }
            Node::Async(operand) => {
                self.out.push_str("async ");
                self.print_split_operand(operand, 0, split);
            }
            // Weak precedence: `const` captures everything to its right, so
            // the inner expression never needs wrapping; as an OPERAND the
            // whole `const ..` is parenthesized (precedence 0 above).
            Node::Const(inner) => {
                self.out.push_str("const ");
                self.print_split_operand(inner, 0, split);
            }
            Node::Let(name, declared_type, value, mutable) => {
                self.out.push_str(if *mutable { "mut " } else { "let " });
                self.out.push_str(name.0);
                if let Some(declared_type) = declared_type {
                    self.out.push_str(": ");
                    self.print_type(&declared_type.0);
                }
                if let Some(value) = value {
                    self.out.push_str(" = ");
                    self.print_split_operand(value, 0, split);
                }
            }
            // `let (a, b) = …` / `mut [x, y] = …` — a destructuring binding. As
            // `Let`, but the bound name is an irrefutable tuple/array binder
            // (a name, or a nesting of them) printed by `print_binder`.
            Node::LetDestructure(pattern, declared_type, value, mutable) => {
                self.out.push_str(if *mutable { "mut " } else { "let " });
                self.print_binder(&pattern.0);
                if let Some(declared_type) = declared_type {
                    self.out.push_str(": ");
                    self.print_type(&declared_type.0);
                }
                if let Some(value) = value {
                    self.out.push_str(" = ");
                    self.print_split_operand(value, 0, split);
                }
            }
            Node::Assign(target, operator, value) => {
                self.print_expr(target);
                self.out.push(' ');
                if let Some(operator) = operator {
                    self.out.push_str(binary_operator_symbol(*operator));
                }
                self.out.push_str("= ");
                self.print_split_operand(value, 0, split);
            }
            Node::If(branch) => self.print_if_branch(branch),
            Node::Match(subject, legs) => {
                self.out.push_str("match ");
                self.print_expr(subject);
                self.out.push_str(" {");
                self.indent += 1;
                let mut prev_end = legs.1.into_range().start + 1;
                for leg in &legs.0 {
                    let (patterns, _, body) = leg;
                    let start = patterns
                        .first()
                        .map(|(_, span)| span.into_range().start)
                        .unwrap_or(prev_end);
                    let after_comments = self.flush_comments_before(start, prev_end);
                    if self.has_blank_between(after_comments, start) {
                        self.blank_line();
                    }
                    self.line();
                    self.print_match_leg(leg);
                    // The arm separator comma is optional and not kept in the AST
                    // (the corpus mixes `=> { .. },` and `=> { .. }`), so preserve
                    // whatever the source had to round-trip either style faithfully.
                    let body_end = body.1.into_range().end;
                    if self.source_has_comma_at(body_end) {
                        self.out.push(',');
                    }
                    self.flush_trailing_comment(body_end);
                    prev_end = body_end;
                }
                self.flush_comments_before(legs.1.into_range().end, prev_end);
                self.indent -= 1;
                self.line();
                self.out.push('}');
            }
            Node::For(condition, body) => {
                self.out.push_str("for");
                if let Some(condition) = condition {
                    self.out.push(' ');
                    self.print_expr(condition);
                }
                self.out.push(' ');
                self.print_block(body);
            }
            Node::ForIn(variable, iterable, body) => {
                self.out.push_str("for ");
                self.out.push_str(variable);
                self.out.push_str(" in ");
                self.print_expr(iterable);
                self.out.push(' ');
                self.print_block(body);
            }
            Node::FuncReturn(value) => {
                self.out.push_str("ret");
                if let Some(value) = value {
                    self.out.push(' ');
                    self.print_split_operand(value, 0, split);
                }
            }
            Node::Jump(target) => {
                self.out.push_str("jump ");
                self.out.push_str(target);
            }
            Node::Block(block) => self.print_block(block),
            // `macro { .. }` — an anonymous immediately-expanded macro. Legal in
            // both item and expression position; the body is a statement block.
            Node::MacroBlock(body) => {
                self.out.push_str("macro ");
                self.print_block(body);
            }
            // `macro name(args)` — a macro invocation (item or expression
            // position). The arguments are SYNTAX — reprinted verbatim from their
            // source spans, never rebuilt from a parsed tree (only spans are kept).
            Node::MacroInvocation(name, _name_span, argument_spans) => {
                self.out.push_str("macro ");
                self.out.push_str(name);
                self.out.push('(');
                self.print_argument_spans(argument_spans);
                self.out.push(')');
            }
            Node::StructInitializer(namespace, name, generic_arguments, fields) => {
                // The path as written (B190) — the namespace spine, then the
                // name. The printer BAILS on a form it does not know, falling
                // the whole file back to its source silently, so a qualified
                // literal without this would have stopped being formatted.
                for segment in namespace {
                    self.out.push_str(segment);
                    self.out.push_str("::");
                }
                self.out.push_str(name.0);
                if let Some((generic_arguments, _)) = generic_arguments {
                    self.out.push('<');
                    for (index, (argument, _)) in generic_arguments.iter().enumerate() {
                        if index > 0 {
                            self.out.push_str(", ");
                        }
                        self.print_type(argument);
                    }
                    self.out.push('>');
                }
                // A struct literal whose line overflowed the budget breaks one
                // field per line (see [`Self::print_split_struct`]); one that
                // fits stays inline, WITHOUT a trailing comma, exactly as
                // before. An empty literal never breaks — `{⏎}` buys a line and
                // nothing else.
                let field_spans: Vec<Span> = fields.0.iter().map(|field| field.1).collect();
                if fields.0.is_empty() {
                    self.out.push_str(" {}");
                } else if split != Split::Off
                    || self.comment_outside_elements(fields.1, &field_spans)
                    || self.any_field_spans_lines(&fields.0)
                {
                    self.print_split_struct(&fields.0, fields.1.into_range().start);
                } else {
                    self.out.push_str(" { ");
                    for (index, ((field_name, value), _)) in fields.0.iter().enumerate() {
                        if index > 0 {
                            self.out.push_str(", ");
                        }
                        self.print_struct_field(field_name, value);
                    }
                    self.out.push_str(" }");
                }
            }
            // A list literal whose line overflowed the budget breaks one element
            // per line (see [`Self::print_split_list`]); one that fits stays
            // inline, WITHOUT a trailing comma, exactly as before. An empty list
            // never breaks — `[⏎]` buys a line and nothing else.
            Node::List(elements) => {
                let element_spans: Vec<Span> = elements.iter().map(|element| element.1).collect();
                if !elements.is_empty()
                    && (split != Split::Off
                        || self.comment_outside_elements(expr.1, &element_spans)
                        || self.any_element_spans_lines(elements))
                {
                    self.print_split_list(elements, expr.1.into_range().start);
                } else {
                    self.out.push('[');
                    self.print_expression_list(elements);
                    self.out.push(']');
                }
            }
            // `[value; n]` — a fixed-length array repeat literal (proposal/
            // fixed-arrays.md): the value copied into each of `n` slots. `; ` is
            // the canonical spelling of the length separator.
            Node::Repeat(value, length) => {
                self.out.push('[');
                self.print_expr(value);
                self.out.push_str("; ");
                self.print_expr(length);
                self.out.push(']');
            }
            Node::Tuple(elements) => {
                self.out.push('(');
                self.print_expression_list(elements);
                self.out.push(')');
            }
            Node::Closure(closure) => {
                self.print_closure_parameters(&closure.parameters.0);
                if let Some(return_type) = &closure.return_type {
                    self.out.push_str(": ");
                    self.print_type(&return_type.0);
                }
                if !self.print_closure_element_body(&closure.return_value) {
                    self.out.push(' ');
                    self.print_expr(&closure.return_value);
                }
            }
            Node::Is(subject, pattern) => {
                self.print_operand(subject, 3);
                self.out.push_str(" is ");
                self.print_match_pattern(pattern);
            }
            // `(source in sources => source.get())` — a tuple comprehension:
            // `body` evaluated for each element of the tuple `source`, with
            // `binder` naming the element. Like its type-level counterpart
            // `MappedType`, the parentheses are the form's own.
            Node::TupleComprehension {
                binder,
                source,
                body,
                ..
            } => {
                self.out.push('(');
                self.out.push_str(binder);
                self.out.push_str(" in ");
                self.print_expr(source);
                self.out.push_str(" => ");
                self.print_expr(body);
                self.out.push(')');
            }
            Node::Element(body) => self.print_element(body),
            Node::Css(body) => self.print_css(body),
            _ => self.bailed = true,
        }
    }

    /// Prints the closure parameter list `|a, b|` (or `||`) for a closure value.
    /// Closures share the function parameter syntax, but with `|` delimiters and a
    /// single-token `||` for the empty list.
    fn print_closure_parameters(&mut self, parameters: &[crate::node::Parameter<'src>]) {
        if parameters.is_empty() {
            self.out.push_str("||");
            return;
        }
        self.out.push('|');
        self.print_parameters_inner(parameters);
        self.out.push('|');
    }

    /// Prints an `if`/`else if`/`else` chain.
    fn print_if_branch(&mut self, branch: &NodeIfBranch<'src>) {
        match branch {
            NodeIfBranch::If(if_) => {
                self.out.push_str("if ");
                self.print_expr(&if_.condition);
                self.out.push(' ');
                self.print_block(&if_.then);
                if let Some((else_branch, _)) = &if_.else_ {
                    self.out.push_str(" else ");
                    match else_branch {
                        NodeIfBranch::If(_) => self.print_if_branch(else_branch),
                        NodeIfBranch::Else(block) => self.print_block(block),
                    }
                }
            }
            NodeIfBranch::Else(block) => self.print_block(block),
        }
    }

    /// Prints one `match` leg: `pattern[, pattern][ if guard] => body`.
    fn print_match_leg(&mut self, leg: &crate::node::MatchLeg<'src>) {
        let (patterns, guard, body) = leg;
        for (index, pattern) in patterns.iter().enumerate() {
            if index > 0 {
                self.out.push_str(", ");
            }
            self.print_match_pattern(pattern);
        }
        if let Some(guard) = guard {
            self.out.push_str(" if ");
            self.print_expr(guard);
        }
        self.out.push_str(" => ");
        self.print_expr(body);
    }

    /// Prints a binder in `let`/parameter position: a bare name (no `let `
    /// keyword), a tuple `(a, b)`, or a fixed-array `[a, b]` binder — each of
    /// which may nest binders. Distinct from a match pattern (`print_pattern`),
    /// where a binding reads `let x` / `mut x`.
    fn print_binder(&mut self, binder: &Pattern<'src>) {
        match binder {
            Pattern::Binding(name, _, _) => self.out.push_str(name),
            Pattern::Tuple(elements) => {
                self.out.push('(');
                for (index, (element, _)) in elements.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_binder(element);
                }
                self.out.push(')');
            }
            Pattern::Array(elements) => {
                self.out.push('[');
                for (index, (element, _)) in elements.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_binder(element);
                }
                self.out.push(']');
            }
            // A binder is only ever a name, a tuple, or an array of binders;
            // other pattern shapes can't reach here from the parser.
            other => self.print_pattern(other),
        }
    }

    /// Prints a match pattern, consulting the source to keep a binding tuple's
    /// spelling. `let (a, b)` (the keyword factored out, before the tuple) and
    /// `(let a, let b)` (a tuple of per-element binders) parse to the *same*
    /// `Tuple` of `Binding`s, and both appear in the corpus (`let (a, b)` in
    /// `destructuring.vl`; `Some((let x, let y))` in `option.vl`). Neither is
    /// canonically preferable, so the printer reproduces whichever the source
    /// used — decided by the tuple span's first byte: `l`/`m` (the `let`/`mut`
    /// keyword) is the factored form, `(` the per-element form. Every other
    /// pattern prints identically regardless of span.
    fn print_match_pattern(&mut self, pattern: &Spanned<Pattern<'src>>) {
        if let Pattern::Tuple(_) = &pattern.0 {
            let start = pattern.1.into_range().start;
            match self.source.as_bytes().get(start) {
                Some(b'l') => {
                    self.out.push_str("let ");
                    self.print_binder(&pattern.0);
                    return;
                }
                Some(b'm') => {
                    self.out.push_str("mut ");
                    self.print_binder(&pattern.0);
                    return;
                }
                _ => {}
            }
        }
        self.print_pattern(&pattern.0);
    }

    fn print_pattern(&mut self, pattern: &Pattern<'src>) {
        match pattern {
            Pattern::Wildcard => self.out.push('_'),
            Pattern::Binding(name, mutable, _) => {
                self.out.push_str(if *mutable { "mut " } else { "let " });
                self.out.push_str(name);
            }
            Pattern::Variant(path, payload) => {
                for (index, segment) in path.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str("::");
                    }
                    self.out.push_str(segment);
                }
                if let Some(payload) = payload {
                    self.out.push('(');
                    for (index, sub_pattern) in payload.iter().enumerate() {
                        if index > 0 {
                            self.out.push_str(", ");
                        }
                        self.print_match_pattern(sub_pattern);
                    }
                    self.out.push(')');
                }
            }
            Pattern::Tuple(elements) => {
                self.out.push('(');
                for (index, element) in elements.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_match_pattern(element);
                }
                self.out.push(')');
            }
            Pattern::Array(elements) => {
                self.out.push('[');
                for (index, element) in elements.iter().enumerate() {
                    if index > 0 {
                        self.out.push_str(", ");
                    }
                    self.print_match_pattern(element);
                }
                self.out.push(']');
            }
            Pattern::Literal(literal) => self.print_expr(literal),
        }
    }
}

/// The source spelling of a binary operator.
fn binary_operator_symbol(operator: BinaryOp) -> &'static str {
    match operator {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Rem => "%",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        // JS-only (the transformer's unsigned right shift); never in a parsed
        // source tree, but total for safety.
        BinaryOp::UShr => ">>",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitXor => "^",
        BinaryOp::BitOr => "|",
        BinaryOp::Eq => "==",
        BinaryOp::NotEq => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Gt => ">",
        BinaryOp::LtEq => "<=",
        BinaryOp::GtEq => ">=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_comments_skipping_strings() {
        let source = "let url = \"http://x\"; // a note\n// lead\nfun f() {}\n";
        let comments: Vec<&str> = extract_comments(source)
            .into_iter()
            .map(|(_, text)| text)
            .collect();
        assert_eq!(comments, vec!["// a note", "// lead"]);
    }

    #[test]
    fn comment_spans_are_forward_and_trimmed() {
        let source = "fun f() {} // trailing  \n";
        let (span, text) = extract_comments(source)[0];
        assert_eq!(text, "// trailing");
        let range = span.into_range();
        assert!(range.start <= range.end);
        assert_eq!(&source[range.start..range.end], "// trailing");
    }
}

#[cfg(test)]
mod reformats {
    use super::format;

    fn assert_formats(source: &str, expected: &str) {
        assert_eq!(format(source), expected);
        // The output must be a fixed point — formatting it again is a no-op.
        assert_eq!(format(expected), expected, "output is not idempotent");
    }

    // The extern retention flag (`lifetimes.md` §6.4) round-trips. It is
    // recognized in TRAILING position only — the one place a flag can sit
    // without displacing a form word — so the printer reprints it exactly where
    // it was written and the round trip is byte-exact for every binding shape
    // it composes with.
    #[test]
    fn the_extern_retention_flag_round_trips_last() {
        assert_formats(
            "[extern(\"queueMicrotask\", retains)]\nexternal fun queue(callback: || void);\n",
            "[extern(\"queueMicrotask\", retains)]\nexternal fun queue(callback: || void);\n",
        );
        assert_formats(
            "[extern(method, \"addEventListener\", retains)]\nexternal fun on(self, event: str, handler: || void): void;\n",
            "[extern(method, \"addEventListener\", retains)]\nexternal fun on(self, event: str, handler: || void): void;\n",
        );
        // An unmarked extern is untouched.
        assert_formats(
            "[extern(\"queueMicrotask\")]\nexternal fun queue(callback: || void);\n",
            "[extern(\"queueMicrotask\")]\nexternal fun queue(callback: || void);\n",
        );
        // The shape `std::dom`'s listen surface is declared in (`router.md`
        // §5.2): a MARKED registration and an UNMARKED removal, adjacent, on
        // the same host object. The audit rule draws its line between these two
        // lines — `addEventListener` stores the closure, `removeEventListener`
        // keeps nothing — so a printer that normalized the flag onto both (or
        // off both) would erase the distinction in the one place it is written.
        let listen_pair = "impl Window {\n\
             \t[extern(method, \"addEventListener\", retains)]\n\
             \texternal fun on_event(self, event: str, handler: |Event| void): void;\n\n\
             \t[extern(method, \"removeEventListener\")]\n\
             \texternal fun off_event(self, event: str, handler: |Event| void): void;\n\
             }\n";
        assert_formats(listen_pair, listen_pair);
    }

    // `async`/`sync` closure-type markers round-trip (they used to BAIL,
    // leaving marker-bearing files unformattable).
    #[test]
    fn closure_type_markers_round_trip() {
        let source =
            "fun take(f: async || i32, g: sync |i32| bool) {\n\tlet h: async || void = f;\n}\n";
        assert_formats(source, source);
    }

    #[test]
    fn struct_fields_onto_their_own_lines() {
        assert_formats(
            "struct Point{x:i32,y:i32}\n",
            "struct Point {\n\tx: i32,\n\ty: i32,\n}\n",
        );
    }

    #[test]
    fn a_struct_literal_operand_reformats() {
        // §H.1: a struct literal as an operator operand round-trips through
        // the formatter (parse → print → parse must hold).
        assert_formats(
            "fun f(p: Point): bool {\n\tPoint{x=1}==p\n}\n",
            "fun f(p: Point): bool {\n\tPoint { x = 1 } == p\n}\n",
        );
    }

    #[test]
    fn generic_and_reference_field_types() {
        assert_formats(
            "struct Boxed { item :  List<i32> , next : &mut Node }\n",
            "struct Boxed {\n\titem: List<i32>,\n\tnext: &mut Node,\n}\n",
        );
    }

    #[test]
    fn empty_struct_body_stays_inline() {
        assert_formats("struct Unit{\n}\n", "struct Unit {}\n");
    }

    #[test]
    fn enum_variants_onto_their_own_lines() {
        assert_formats("enum E{A,B}\n", "enum E {\n\tA,\n\tB,\n}\n");
    }

    #[test]
    fn generic_enum_with_payloads() {
        assert_formats(
            "enum Option<T>{Some(T),None}\n",
            "enum Option<T> {\n\tSome(T),\n\tNone,\n}\n",
        );
    }

    // C4 S1 (destruction.md §3): the `resource` declaration modifier prints back
    // in canonical position — `resource` before `struct`/`enum`, `resource
    // external struct` for the leaf host case — and re-formats to a fixed point.
    #[test]
    fn resource_struct_modifier_round_trips() {
        assert_formats(
            "resource struct S{x:i32}\n",
            "resource struct S {\n\tx: i32,\n}\n",
        );
    }

    #[test]
    fn resource_external_struct_keeps_canonical_order() {
        assert_formats(
            "resource external struct Database;\n",
            "resource external struct Database;\n",
        );
    }

    #[test]
    fn resource_enum_modifier_round_trips() {
        assert_formats(
            "resource enum E{A,B}\n",
            "resource enum E {\n\tA,\n\tB,\n}\n",
        );
    }

    #[test]
    fn function_signature_and_body() {
        assert_formats(
            "fun add(a:i32,b:i32):i32{a+b}\n",
            "fun add(a: i32, b: i32): i32 {\n\ta + b\n}\n",
        );
    }

    #[test]
    fn statements_take_semicolons_tail_does_not() {
        assert_formats(
            "fun f(){let x=1;print(x);x}\n",
            "fun f() {\n\tlet x = 1;\n\tprint(x);\n\tx\n}\n",
        );
    }

    #[test]
    fn precedence_parentheses_are_minimal() {
        assert_formats("fun f(){(a+b)*c}\n", "fun f() {\n\t(a + b) * c\n}\n");
        assert_formats("fun f(){a+b*c}\n", "fun f() {\n\ta + b * c\n}\n");
    }

    #[test]
    fn call_through_a_member_keeps_its_parentheses() {
        // `(self.fn)()` calls a field-closure; `self.fn()` is a method call.
        assert_formats(
            "fun f(self){(self.fn)()}\n",
            "fun f(self) {\n\t(self.fn)()\n}\n",
        );
    }

    #[test]
    fn trailing_comment_stays_on_its_line() {
        assert_formats(
            "fun f(){print(1);    // note\n}\n",
            "fun f() {\n\tprint(1); // note\n}\n",
        );
    }

    #[test]
    fn interpolated_string_is_reprinted_verbatim() {
        // The lexer expands `i"..."` to `("" + ..)` before parsing; the printer
        // recovers the original literal from the source rather than the AST.
        assert_formats(
            "fun f(self){print(i\"hi {self.name}!\")}\n",
            "fun f(self) {\n\tprint(i\"hi {self.name}!\")\n}\n",
        );
    }

    #[test]
    fn interpolated_string_with_escaped_braces() {
        assert_formats(
            "fun f(){let x=i\"a \\{b\\} c\";x}\n",
            "fun f() {\n\tlet x = i\"a \\{b\\} c\";\n\tx\n}\n",
        );
    }

    #[test]
    fn interpolated_triple_quoted_string_is_reprinted_verbatim() {
        // H7. The inner whitespace is semantic (the closing delimiter's
        // indentation is the trim prefix), so the literal reprints verbatim like
        // its plain twin — while the code around it still canonicalizes.
        assert_formats(
            "fun f(self){let x=i\"\"\"\n\t\thi {self.name}\n\t\t\"\"\";x}\n",
            "fun f(self) {\n\tlet x = i\"\"\"\n\t\thi {self.name}\n\t\t\"\"\";\n\tx\n}\n",
        );
    }

    #[test]
    fn an_interpolated_triple_quoted_string_ending_in_a_hole_keeps_its_closing_delimiter() {
        // THE span case: a concatenation's span ends at the last token it
        // consumed, so a literal ending with a hole reports a span stopping at
        // the hole's `}`. Recovering the slice from that span alone truncates the
        // literal — the reprint then fails to lex and the formatter bails.
        assert_formats(
            "fun f(){let x=i\"\"\"\n\t\tvalue: {n}\n\t\t\"\"\";x}\n",
            "fun f() {\n\tlet x = i\"\"\"\n\t\tvalue: {n}\n\t\t\"\"\";\n\tx\n}\n",
        );
    }

    #[test]
    fn an_interpolated_triple_quoted_string_leading_a_compound_is_not_a_bail() {
        // THE truncation case (the slice-6 review's block): `interpolated_source`
        // is consulted for every expression node, and a COMPOUND whose span
        // merely starts at the literal — a concatenation, a method call — is not
        // the literal. Recovering "the first `\"\"\"` after the start" as the
        // slice's end truncated the compound to the literal alone, the safety
        // net saw dropped tokens, and the WHOLE FILE silently kept its original
        // bytes with `--check` reporting clean. The recovered end may only ever
        // EXTEND the span (the ends-in-a-hole case), never shorten it.
        assert_formats(
            "fun f(){let x=i\"\"\"\n\t\thi\n\t\t\"\"\" + \"t\";x}\n",
            "fun f() {\n\tlet x = i\"\"\"\n\t\thi\n\t\t\"\"\" + \"t\";\n\tx\n}\n",
        );
        assert_formats(
            "fun f(){let x=i\"\"\"\n\t\thi\n\t\t\"\"\".len();x}\n",
            "fun f() {\n\tlet x = i\"\"\"\n\t\thi\n\t\t\"\"\".len();\n\tx\n}\n",
        );
    }

    #[test]
    fn an_interpolated_string_in_trailing_position_still_formats() {
        // Controls for the truncation case: trailing position never had the
        // problem (the compound's start is not the literal's), and the
        // single-quoted form's arm never shortened. Both must stay formatting.
        assert_formats(
            "fun f(){let x=\"x\" + i\"\"\"\n\t\thi\n\t\t\"\"\";x}\n",
            "fun f() {\n\tlet x = \"x\" + i\"\"\"\n\t\thi\n\t\t\"\"\";\n\tx\n}\n",
        );
        assert_formats(
            "fun f(){let x=i\"a{n}\" + \"t\";x}\n",
            "fun f() {\n\tlet x = i\"a{n}\" + \"t\";\n\tx\n}\n",
        );
    }

    #[test]
    fn impl_with_match_and_closure() {
        // `fn: |T| U` keeps the space after `:` — `:|` would lex as one operator.
        // The last arm has no source comma, so the faithful output keeps none.
        assert_formats(
            "impl Option<type T> { fun map<U>(self, fn: |T| U): Option<U> { match self { Some(let x)=>Some(fn(x)), None=>None } } }\n",
            "impl Option<type T> {\n\tfun map<U>(self, fn: |T| U): Option<U> {\n\t\tmatch self {\n\t\t\tSome(let x) => Some(fn(x)),\n\t\t\tNone => None\n\t\t}\n\t}\n}\n",
        );
    }

    /// A module-qualified TYPE path (B172) reprints as written, in every
    /// position the type printer is reached from. The printer BAILS on a form
    /// it does not know — falling the whole file back to its source, silently —
    /// so a new type form without an arm here is a file that stops being
    /// formatted rather than a failure anyone sees.
    #[test]
    fn a_module_qualified_type_path_round_trips() {
        let source = "import std::reactive;\nimport std::style;\n\n\
             struct Card {\n\
             \tstyle: style::Style,\n\
             \thits: reactive::SignalCell<i32>,\n\
             \tdeep: List<std::style::Style>,\n\
             }\n\n\
             impl style::Style {\n\
             \tfun tag(&self): str {\n\
             \t\t\"s\"\n\
             \t}\n\
             }\n\n\
             fun render(card: &Card, shape: (style::Style, i32)): style::Style {\n\
             \tlet held: style::Style = card.style;\n\
             \tshape.0\n\
             }\n";
        assert_formats(source, source);
    }

    /// A module-qualified struct LITERAL (B190) reprints as written. Same
    /// hazard as the type path above, and pinned for the same reason: the
    /// printer BAILS on a form it does not know, which turns a missing arm
    /// into a file that quietly stops being formatted rather than a failure
    /// anyone sees.
    #[test]
    fn a_module_qualified_struct_literal_round_trips() {
        let source = "mod shapes {\n\
             \tmod deep {\n\
             \t\tstruct Ring {\n\
             \t\t\tr: i32,\n\
             \t\t}\n\
             \t}\n\n\
             \tstruct Dot {\n\
             \t\tx: i32,\n\
             \t}\n\
             }\n\n\
             fun main() {\n\
             \tlet d = shapes::Dot { x = 1 };\n\
             \tlet r = shapes::deep::Ring { r = 2 };\n\
             \tprint(i\"{d.x}{r.r}\");\n\
             }\n";
        assert_formats(source, source);
    }
}

#[cfg(test)]
mod idempotency {
    use super::format;

    /// The real invariant: formatting is a fixed point. `format(x)` may tidy `x`,
    /// but formatting the result again must change nothing.
    fn assert_fixed_point(name: &str, source: &str) {
        // A BAILING file satisfies the fixed-point property trivially — `format`
        // hands back its input, so `once == twice` whatever the printer can or
        // cannot render. Two of the fixtures below were in exactly that state
        // (`option.vl`, `reactive.vl`), which made their pins prove nothing;
        // `reactive_vl` is here precisely to catch a dropped `[must_use]`
        // tripping the safety net into a silent no-op, and it was tripped.
        //
        // So assert non-bail first, the way `assert_construct` does: appending
        // blank lines is pure trivia, and a formatter that actually ran
        // canonicalizes it away, while a bail returns it verbatim.
        let once = format(source);
        assert_eq!(
            format(&format!("{source}\n\n")),
            once,
            "formatter silently BAILED on {name} — its fixed-point pin proves nothing"
        );
        let twice = format(&once);
        assert_eq!(once, twice, "formatting {name} is not a fixed point");
    }

    macro_rules! fixed_point_tests {
        ($($name:ident => $path:literal),* $(,)?) => {
            $(
                #[test]
                fn $name() {
                    assert_fixed_point($path, include_str!(concat!("../../../vilan/std/src/", $path)));
                }
            )*
        };
    }

    // A spread of std modules exercising functions, impls, traits, generics,
    // enums with payloads, matches, closures, and `[extern]` bindings —
    // `reactive.vl` also exercises `[must_use]` (which the formatter once
    // dropped, tripping its safety check into a silent no-op).
    fixed_point_tests! {
        null_vl => "null.vl",
        boolean_vl => "boolean.vl",
        option_vl => "option.vl",
        result_vl => "result.vl",
        list_vl => "list.vl",
        string_vl => "string.vl",
        set_vl => "set.vl",
        iterator_vl => "iterator.vl",
        arena_vl => "arena.vl",
        shared_vl => "shared.vl",
        display_vl => "display.vl",
        reactive_vl => "reactive.vl",
    }

    /// The bitwise/shift operators and hex literals print back exactly —
    /// well-formatted source containing them is a fixed point (`<<`/`>>` must
    /// re-lex as adjacent control tokens, `0xFF` keeps its spelling).
    #[test]
    fn bitwise_operators_and_hex_are_a_fixed_point() {
        let source = "fun main() {\n\tlet mask = 0xFFu32;\n\tlet mixed = 1 << 2 & 3 ^ 4 | 5;\n\tlet shifted = mask >> 4;\n\tlet big = 0xDEADn;\n}\n";
        assert_fixed_point("bitwise", source);
    }

    /// Attributes must be *retained*, not just idempotent — a formatter that
    /// deterministically deleted an attribute would still be a fixed point, so
    /// the retention is asserted directly. (Dropping one used to trip the
    /// safety check, silently leaving the whole file unformatted.)
    #[test]
    fn attributes_round_trip() {
        let source = "trait Source {\n\t[must_use]\n\t[platform(\"@process\", \"browser\")]\n\tfun sub(self): i32;\n\
                      \t[trait_only]\n\tfun tag(self): str;\n\
                      \t[doc(hidden)]\n\tfun internal(self): i32;\n}\n\
                      [service(Client)]\n\
                      struct Sess {\n\t[expose] status: SignalCell<str>,\n\thidden: i32,\n}\n\
                      impl Sess {\n\t[rpc]\n\tfun login(self, name: str): bool {\n\t\ttrue\n\t}\n}\n";
        let formatted = format(source);
        assert!(
            formatted.contains("[service(Client)]"),
            "service attribute lost:\n{formatted}"
        );
        assert!(
            formatted.contains("[must_use]")
                && formatted.contains("[platform(\"@process\", \"browser\")]"),
            "attributes lost:\n{formatted}"
        );
        assert!(
            formatted.contains("[expose] status"),
            "expose attribute lost:\n{formatted}"
        );
        assert!(
            formatted.contains("[rpc]"),
            "rpc attribute lost:\n{formatted}"
        );
        assert!(
            formatted.contains("[trait_only]"),
            "trait_only attribute lost:\n{formatted}"
        );
        assert!(
            formatted.contains("[doc(hidden)]"),
            "doc(hidden) attribute lost:\n{formatted}"
        );
        assert_fixed_point("attributes", source);
    }
}

#[cfg(test)]
mod bailing_constructs {
    //! Backlog E13 — the constructs that used to make `vilan fmt` silently
    //! no-op: each hit a `_ => bailed` printer fallback or tripped the
    //! re-lex-and-compare safety net, so the formatter returned the file
    //! unchanged (indistinguishable from an already-canonical file). Each now
    //! round-trips. Per construct, `assert_construct` proves the whole contract
    //! loudly: the output re-lexes to the SAME token stream as the input (the
    //! net's own criterion), the formatter did NOT silently bail (a
    //! token-preserving perturbation canonicalizes identically), the output is
    //! the canonical spelling, formatting is idempotent, and the canonical form
    //! round-trips unchanged.
    use super::{format, normalize};
    use crate::lexing::tokenize;
    use crate::token::Token;

    /// The formatter's notion of "the same code": the lexer's tokens with spans
    /// stripped and insignificant trailing commas normalized away.
    pub(super) fn code_tokens(text: &str) -> Vec<Token<'_>> {
        let (tokens, errors) = tokenize(text);
        assert!(
            errors.is_empty(),
            "did not lex cleanly: {text:?} ({errors:?})"
        );
        normalize(tokens.into_iter().map(|(token, _)| token).collect())
    }

    /// The whole formatter contract for one source, asserted loudly. Shared with
    /// the chain-splitting pins: a split rendering has to satisfy exactly the
    /// same contract as any other reprint.
    pub(super) fn assert_construct(source: &str, expected: &str) {
        let formatted = format(source);
        // (a) The output carries the SAME tokens as the source — the safety
        // net's criterion, asserted here rather than trusted silently.
        assert_eq!(
            code_tokens(&formatted),
            code_tokens(source),
            "output token-drifted from the input on {source:?}"
        );
        // Not a silent bail: a bail returns the input verbatim, so appending
        // blank lines (pure trivia) would survive instead of canonicalizing.
        assert_eq!(
            format(&format!("{source}\n\n")),
            formatted,
            "formatter silently bailed on {source:?}"
        );
        // (b) The canonical spelling.
        assert_eq!(
            formatted, expected,
            "unexpected canonical form for {source:?}"
        );
        // (c) Idempotent, and (d) the canonical form round-trips unchanged.
        assert_eq!(format(&formatted), formatted, "not idempotent: {source:?}");
        assert_eq!(
            format(expected),
            expected,
            "canonical form did not round-trip: {expected:?}"
        );
    }

    // --- `css` blocks: the canonical printer (css-block.md §8, §11 S3) -------
    // S2 shipped a verbatim source-slice passthrough so the bail set could stay
    // EMPTY — a bail returns the whole FILE unformatted while `--check` calls it
    // clean, so a grammar slice with no printer arm silently stops formatting
    // every file holding a block. S3 replaces it with the real printer: one item
    // per line, nested rules at +1, and the block's items in the SAME canonical
    // order the chain sorter gives the chain it desugars to.
    //
    // Every form below is pinned through `assert_construct`, which asserts the
    // whole contract at once: the output re-lexes to the same tokens as the
    // input (module the canonical orders `normalize` folds in), the formatter did
    // not silently bail, the output is the canonical spelling, formatting is
    // idempotent, and the canonical form round-trips unchanged. The bail set
    // itself stays asserted empty over the whole corpus by
    // `parse_differential::formatter_never_silently_bails` — that gate is the
    // real one.

    #[test]
    fn a_css_block_prints_one_declaration_per_line() {
        // A hole is an ordinary vilan expression and canonicalizes as one; the
        // value text around it is CSS and does not.
        assert_construct(
            "fun f() {\n\tcss {\n\t\tdisplay:flex;\n\t\tgap: {space( 4 )};\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\tgap: {space(4)};\n\t}\n}\n",
        );
    }

    #[test]
    fn a_one_declaration_block_collapses_onto_its_line() {
        // The `let active = const css { padding: {space(6)}; };` shape (§2):
        // one declaration, no comment, and it fits — so it stays on the line.
        assert_construct(
            "fun f(){let a=css{color:red;};a}\n",
            "fun f() {\n\tlet a = css { color: red; };\n\ta\n}\n",
        );
    }

    #[test]
    fn an_empty_block_prints_as_a_pair_of_braces() {
        assert_construct("fun f() {\n\tcss {\n\t}\n}\n", "fun f() {\n\tcss {}\n}\n");
    }

    #[test]
    fn a_nested_rule_always_takes_its_own_lines() {
        // A rule whose declarations share its line is not CSS, so a nested body
        // never collapses — not even the one-declaration one the OUTER block
        // would have collapsed. The head's arguments print as any call's do.
        assert_construct(
            "fun f() {\n\tcss { .within(\"data-theme\",\"dark\") { color: red; } }\n}\n",
            "fun f() {\n\tcss {\n\t\t.within(\"data-theme\", \"dark\") {\n\t\t\tcolor: red;\n\t\t}\n\t}\n}\n",
        );
    }

    #[test]
    fn a_block_sorts_into_the_canonical_order() {
        // Properties in Tailwind's category sequence, then conditions in the
        // axis order the selector nests them — the chain's order, reached
        // through the chain's own tables: `padding` (spacing) after `display`
        // (layout), `.md` (media) before `.hover` (pseudo), every condition
        // after every declaration.
        assert_construct(
            "fun f() {\n\tcss {\n\t\t.hover {\n\t\t\tcolor: blue;\n\t\t}\n\t\tpadding: {space(4)};\n\t\t.md {\n\t\t\tpadding: {space(6)};\n\t\t}\n\t\tdisplay: flex;\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\tpadding: {space(4)};\n\t\t.md {\n\t\t\tpadding: {space(6)};\n\t\t}\n\t\t.hover {\n\t\t\tcolor: blue;\n\t\t}\n\t}\n}\n",
        );
    }

    #[test]
    fn an_unknown_property_is_a_barrier_nothing_crosses() {
        // `raw`'s escape hatch reaches the block whole, so a property no row
        // writes — a vendor property, a custom property — cannot be ranked and
        // holds its index absolutely, exactly as an unknown METHOD does in a
        // chain. `padding` may not cross it to reach `display`.
        assert_construct(
            "fun f() {\n\tcss {\n\t\tpadding: {space(4)};\n\t\t--brand-ink: red;\n\t\tdisplay: flex;\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tpadding: {space(4)};\n\t\t--brand-ink: red;\n\t\tdisplay: flex;\n\t}\n}\n",
        );
    }

    #[test]
    fn entangled_properties_keep_their_written_order() {
        // The rule that makes the reorder SAFE. `padding` and `padding-left`
        // are one family, so they share a rank and a stable sort can never swap
        // them — `padding-left` then `padding` means something the reverse does
        // not. `display` still crosses both, because its slot is independent.
        //
        // The LONGHAND is written first on purpose. Rank a property by its own
        // row's index rather than by its FAMILY's — the plausible mistake, since
        // `padding-left` is first written by the `padding_x` row — and the two
        // swap, which is exactly the miscompile the family rule exists to
        // prevent. Written the other way round this pin would pass either way.
        assert_construct(
            "fun f() {\n\tcss {\n\t\tpadding-left: {space(6)};\n\t\tpadding: {space(4)};\n\t\tdisplay: flex;\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\tpadding-left: {space(6)};\n\t\tpadding: {space(4)};\n\t}\n}\n",
        );
    }

    #[test]
    fn a_nested_rules_own_items_sort_too() {
        assert_construct(
            "fun f() {\n\tcss {\n\t\t.hover {\n\t\t\tpadding: {space(4)};\n\t\t\tdisplay: flex;\n\t\t}\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\t.hover {\n\t\t\tdisplay: flex;\n\t\t\tpadding: {space(4)};\n\t\t}\n\t}\n}\n",
        );
    }

    #[test]
    fn a_block_holding_a_comment_is_never_reordered() {
        // Refused outright. A permuted body would carry its comments to the
        // wrong item — the comment cursor only moves forward — so a block with
        // a comment anywhere inside it prints canonically in WRITTEN order.
        // `padding` would otherwise sort after `display`.
        let source = "fun f() {\n\tcss {\n\t\t// a note\n\t\tpadding: {space(4)};\n\t\tdisplay: flex;\n\t}\n}\n";
        assert_construct(source, source);
        assert_eq!(format(source).matches("// a note").count(), 1);
        // Anti-vacuity, built the way
        // `a_declarations_chain_is_never_reordered_by_the_style_chain_sort` was:
        // the SAME body without the comment must reorder, or this pin would pass
        // on a body that was canonical all along.
        let commentless = source.replace("\t\t// a note\n", "");
        assert_eq!(
            format(&commentless),
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\tpadding: {space(4)};\n\t}\n}\n",
            "the fixture must be out of canonical order, or the refusal proves nothing"
        );
    }

    #[test]
    fn a_comment_in_a_nested_rule_pins_the_whole_block() {
        // The refusal is by the block's own braces, so a comment buried in a
        // nested rule pins the outer body too: reordering around it would move
        // the rule the comment is inside away from the comment above it.
        let source = "fun f() {\n\tcss {\n\t\t.hover {\n\t\t\t// a note\n\t\t\tcolor: red;\n\t\t}\n\t\tdisplay: flex;\n\t}\n}\n";
        assert_construct(source, source);
    }

    #[test]
    fn a_comment_attaches_to_the_item_it_precedes() {
        // Attachment is by ITEM SPAN, the anchor the parser carried from the
        // first commit — so a comment written above the second declaration
        // prints above the second declaration, not below the statement.
        assert_construct(
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\t// about the padding\n\t\tpadding: {space(4)};\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\t// about the padding\n\t\tpadding: {space(4)};\n\t}\n}\n",
        );
    }

    #[test]
    fn a_comment_after_the_last_item_stays_inside_the_braces() {
        assert_construct(
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\t// trailing\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\t// trailing\n\t}\n}\n",
        );
    }

    #[test]
    fn a_blank_line_between_declarations_does_not_survive() {
        // Paragraph gaps are trivia here. A sorted body's gaps would land
        // between items that no longer belong together, so the block has one
        // shape: item, item, item.
        assert_construct(
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\n\t\tpadding: {space(4)};\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tdisplay: flex;\n\t\tpadding: {space(4)};\n\t}\n}\n",
        );
    }

    #[test]
    fn a_mixed_value_keeps_its_own_spacing() {
        // A value is CSS, not vilan: the text between holes is a source slice,
        // because respacing it would rewrite the bytes inside a `url("a  b")`.
        // Only the holes canonicalize.
        assert_construct(
            "fun f() {\n\tcss {\n\t\tpadding: calc({ a } + 2px);\n\t\tbackground-image: url(\"tile.png\");\n\t}\n}\n",
            "fun f() {\n\tcss {\n\t\tpadding: calc({a} + 2px);\n\t\tbackground-image: url(\"tile.png\");\n\t}\n}\n",
        );
    }

    #[test]
    fn a_css_block_mixes_with_a_style_chain_in_one_file() {
        // Two spellings, one canonical order — which is the whole reason the
        // block sorts (§8). The chain sorter's gates still do not fire on a
        // block (the formatter reparses SOURCE and there is no `style ( )` token
        // run in one); the block has its own pair, reading the same tables.
        assert_construct(
            "fun f() {\n\tlet a = style().padding(x).display(y);\n\tlet b = css {\n\t\tpadding: x;\n\t\tdisplay: y;\n\t};\n\tb\n}\n",
            "fun f() {\n\tlet a = style().display(y).padding(x);\n\tlet b = css {\n\t\tdisplay: y;\n\t\tpadding: x;\n\t};\n\tb\n}\n",
        );
    }

    #[test]
    fn a_block_inside_an_element_head_prints_canonically() {
        // The two sugars compose: a block in a head item is still a block.
        assert_construct(
            "fun f() {\n\t<div .styled(const css{color:red;}) />\n}\n",
            "fun f() {\n\t<div .styled(const css { color: red; }) />\n}\n",
        );
    }

    // --- Prefix-operator precedence (unary-minus.vl) -------------------------

    // A prefix operator binds tighter than every binary operator, so a
    // parenthesized binary operand keeps its parens — dropping them reparses
    // `-2 + 3`. (This tripped the net; the operand minimum was too low.)
    #[test]
    fn unary_minus_over_a_parenthesized_binary_keeps_parens() {
        assert_construct("fun f() {\n\t-(2 + 3)\n}\n", "fun f() {\n\t-(2 + 3)\n}\n");
    }

    // `- -x` and `--x` lex identically (vilan has no `--` operator); a nested
    // prefix (precedence 10) never wraps, so double negation collapses.
    #[test]
    fn double_negation_collapses() {
        assert_construct("fun f() {\n\t- -x\n}\n", "fun f() {\n\t--x\n}\n");
    }

    // A binary subtraction of a negated operand needs no parens — the right
    // operand is a prefix form, which binds tighter than `-`.
    #[test]
    fn binary_subtract_of_a_negative() {
        assert_construct("fun f() {\n\t3 - -2\n}\n", "fun f() {\n\t3 - -2\n}\n");
    }

    #[test]
    fn plain_prefix_operands_round_trip() {
        assert_construct("fun f() {\n\t-x\n}\n", "fun f() {\n\t-x\n}\n");
        assert_construct("fun f() {\n\t!ok\n}\n", "fun f() {\n\t!ok\n}\n");
    }

    // The same precedence rule applies to every prefix operator, not just `-`.
    #[test]
    fn reference_of_a_parenthesized_binary_keeps_parens() {
        assert_construct("fun f() {\n\t&(a + b)\n}\n", "fun f() {\n\t&(a + b)\n}\n");
    }

    // --- Lift-chain postfix subjects (lift-chain.vl) -------------------------

    // A `?.` chain absorbs a following `.member` into its continuation, so a
    // member access on the *result* of a lift must parenthesize it —
    // `(a?.b).c`, not `a?.b.c` (which pulls `.c` inside the lift).
    #[test]
    fn member_access_on_a_lift_result_wraps_the_lift() {
        assert_construct("fun f() {\n\t(x?.y).z\n}\n", "fun f() {\n\t(x?.y).z\n}\n");
    }

    // Likewise a call on a lift result: `(a?.b)()`.
    #[test]
    fn call_on_a_lift_result_wraps_the_lift() {
        assert_construct("fun f() {\n\t(x?.y)()\n}\n", "fun f() {\n\t(x?.y)()\n}\n");
    }

    // Without parens the postfixes belong inside the lift, so none are added:
    // `.z` is absorbed, `!` (assert-or-return) is not, and both chain flat.
    #[test]
    fn absorbed_and_unabsorbed_lift_postfixes_need_no_parens() {
        assert_construct("fun f() {\n\tx?.y.z\n}\n", "fun f() {\n\tx?.y.z\n}\n");
        assert_construct("fun f() {\n\tx?.y!\n}\n", "fun f() {\n\tx?.y!\n}\n");
        assert_construct("fun f() {\n\tx?.y!.z\n}\n", "fun f() {\n\tx?.y!.z\n}\n");
    }

    // --- Tuple / array destructuring bindings (destructuring.vl, math.vl,
    //     reactive-owner.vl, fixed-arrays.vl) --------------------------------

    #[test]
    fn let_tuple_destructure() {
        assert_construct(
            "fun f() {\n\tlet (a,b)=pair;\n}\n",
            "fun f() {\n\tlet (a, b) = pair;\n}\n",
        );
    }

    #[test]
    fn nested_let_tuple_destructure() {
        assert_construct(
            "fun f() {\n\tlet (n, (m, label)) = x;\n}\n",
            "fun f() {\n\tlet (n, (m, label)) = x;\n}\n",
        );
    }

    #[test]
    fn let_and_mut_array_destructure() {
        assert_construct(
            "fun f() {\n\tlet [a, b] = arr;\n}\n",
            "fun f() {\n\tlet [a, b] = arr;\n}\n",
        );
        assert_construct(
            "fun f() {\n\tmut [r0, r1] = right;\n}\n",
            "fun f() {\n\tmut [r0, r1] = right;\n}\n",
        );
    }

    #[test]
    fn typed_tuple_destructure() {
        assert_construct(
            "fun f() {\n\tlet (a, b): (i32, str) = x;\n}\n",
            "fun f() {\n\tlet (a, b): (i32, str) = x;\n}\n",
        );
    }

    // A match tuple binding has two source spellings that parse identically:
    // `let (a, b)` (keyword factored out) and `(let a, let b)` (per-element).
    // Both are in the corpus, so the printer reproduces whichever was written —
    // this round-trip fails if the printer canonicalizes to one form.
    #[test]
    fn match_tuple_binding_keeps_its_source_spelling() {
        let canonical = "fun f() {\n\tmatch z {\n\t\tlet (a, b) => 0,\n\t\tSome(let (c, d)) => 1,\n\t\tSome((let e, let g)) => 2,\n\t}\n}\n";
        assert_construct(canonical, canonical);
    }

    // --- Fixed-array literals and types (fixed-arrays.vl) --------------------

    #[test]
    fn array_repeat_literal() {
        assert_construct(
            "fun f() {\n\tlet z = [0;4];\n}\n",
            "fun f() {\n\tlet z = [0; 4];\n}\n",
        );
    }

    // An aggregate repeat — a struct literal value copied into each slot.
    #[test]
    fn aggregate_array_repeat_literal() {
        assert_construct(
            "fun f() {\n\tmut cells = [Cell { n = 7 }; 3];\n}\n",
            "fun f() {\n\tmut cells = [Cell { n = 7 }; 3];\n}\n",
        );
    }

    #[test]
    fn fixed_array_type_in_a_signature() {
        assert_construct(
            "fun total(values:[i32;3]):i32 { 0 }\n",
            "fun total(values: [i32; 3]): i32 {\n\t0\n}\n",
        );
    }

    // Nested fixed-array type: `[[i32; 2]; 3]`.
    #[test]
    fn nested_fixed_array_type() {
        assert_construct(
            "fun grid(): [[i32; 2]; 3] {\n\tg\n}\n",
            "fun grid(): [[i32; 2]; 3] {\n\tg\n}\n",
        );
    }

    // --- Sized / hex / suffixed numerics (numeric-types.vl, math.vl) ---------

    // The number printer round-trips a width suffix, a float suffix, a hex
    // literal, and a `BigInt` suffix (all already handled — pinned as an edge).
    #[test]
    fn suffixed_hex_and_float_numerics() {
        let canonical = "fun f() {\n\tlet a = 0xFFu8;\n\tlet b = 2.25f32;\n\tlet c = 7n;\n\tlet d = 9007199254740992i53;\n}\n";
        assert_construct(canonical, canonical);
    }

    // --- Macro forms (macro-block.vl, macro-derive.vl, macro-invoke.vl) ------

    #[test]
    fn macro_fun_definition() {
        assert_construct(
            "macro fun make(): Source { source(\"\") }\n",
            "macro fun make(): Source {\n\tsource(\"\")\n}\n",
        );
    }

    // A `macro { .. }` block in item position: its body is a statement block and
    // it takes no `;` (like an item declaration). A body with several statements
    // (a "family" of items stamped at expansion) reprints on its own lines.
    #[test]
    fn macro_block_in_item_position() {
        assert_construct(
            "macro {\n\tmut generated = \"\";\n\tsource(generated)\n}\n",
            "macro {\n\tmut generated = \"\";\n\tsource(generated)\n}\n",
        );
    }

    // A `macro { .. }` block in expression position is the `let`'s value, so the
    // terminating `;` belongs to the `let`.
    #[test]
    fn macro_block_in_expression_position() {
        assert_construct(
            "fun f() {\n\tlet folded = macro {\n\t\tsource(i\"1\")\n\t};\n}\n",
            "fun f() {\n\tlet folded = macro {\n\t\tsource(i\"1\")\n\t};\n}\n",
        );
    }

    // A `macro name(args)` invocation in item position takes a `;`; its
    // arguments are syntax, reprinted verbatim from source.
    #[test]
    fn macro_invocation_in_item_position() {
        assert_construct(
            "macro constants(zero, one, two);\n",
            "macro constants(zero, one, two);\n",
        );
    }

    // In expression position the invocation splices in place; a closure argument
    // is reprinted verbatim (spans only are kept, so it is never rebuilt).
    #[test]
    fn macro_invocation_in_expression_position() {
        assert_construct(
            "fun f() {\n\tprint(macro unroll(4, |i: i32| accumulate(i)))\n}\n",
            "fun f() {\n\tprint(macro unroll(4, |i: i32| accumulate(i)))\n}\n",
        );
    }

    // A user macro attribute sits on its own line above the item it annotates —
    // with no arguments (`[derive_display]`) and with verbatim ones (`[grow(a, b)]`).
    #[test]
    fn macro_attribute_without_and_with_arguments() {
        assert_construct(
            "[derive_display]\nstruct Point {\n\tx: i32,\n}\n",
            "[derive_display]\nstruct Point {\n\tx: i32,\n}\n",
        );
        assert_construct(
            "[grow(a, b)]\nstruct Grid {\n\tn: i32,\n}\n",
            "[grow(a, b)]\nstruct Grid {\n\tn: i32,\n}\n",
        );
    }

    // A `[derive(A, B)]` built-in derive on a struct (already handled — pinned
    // as an E13 edge alongside the user-macro attribute above).
    #[test]
    fn derive_attributed_struct() {
        assert_construct(
            "[derive(Json, Debug)]\nstruct Packet {\n\tkind: u8,\n}\n",
            "[derive(Json, Debug)]\nstruct Packet {\n\tkind: u8,\n}\n",
        );
    }

    // --- Backlog 47: the std bail set ---------------------------------------
    //
    // Five std files bailed for four missing printer arms and one dropped node,
    // unnoticed because the zero-bail gate watched the CORPUS ALONE — these
    // constructs appear in the standard library and nowhere in `vilan/test`.
    // The gate now watches std, the examples and the templates too
    // (`parse_differential::formattable_files`), and each construct is pinned
    // here per shape rather than only in aggregate.

    /// `(|| void) context turn_scope` — a type carrying the ambient contexts its
    /// value demands. One context may be written bare or parenthesized, and both
    /// reprint as WRITTEN: the parser keeps only the names, so the source decides
    /// (and rewriting `context (a)` to `context a` would be token drift, which is
    /// a bail — the failure mode this whole item is about).
    #[test]
    fn type_with_contexts() {
        assert_construct(
            "fun ctx(body: (|| void) context turn_scope): i32 {\n\t0\n}\n",
            "fun ctx(body: (|| void) context turn_scope): i32 {\n\t0\n}\n",
        );
        assert_construct(
            "fun ctx(body: (|| void) context (a, b)): i32 {\n\t0\n}\n",
            "fun ctx(body: (|| void) context (a, b)): i32 {\n\t0\n}\n",
        );
        assert_construct(
            "fun ctx(body: (|| void) context (a)): i32 {\n\t0\n}\n",
            "fun ctx(body: (|| void) context (a)): i32 {\n\t0\n}\n",
        );
    }

    /// `(U in T: SignalCell<U>)` — a mapped tuple type, and its expression-level
    /// counterpart `(source in sources => source.get())`. The parentheses belong
    /// to each form (the parser consumes them), so the printer emits them.
    #[test]
    fn mapped_type_and_tuple_comprehension() {
        assert_construct(
            "fun combine<T: (2..)>(sources: (U in T: SignalCell<U>)): SignalCell<T> {\n\
             \tlet snapshot = || (source in sources => source.get());\n\
             \tSignal::new(snapshot())\n\
             }\n",
            "fun combine<T: (2..)>(sources: (U in T: SignalCell<U>)): SignalCell<T> {\n\
             \tlet snapshot = || (source in sources => source.get());\n\
             \tSignal::new(snapshot())\n\
             }\n",
        );
    }

    /// `T: (2..)` / `(..10)` / `(..: Display)` / `(2..4: Display)` — a tuple-arity
    /// bound, which REPLACES the trait-bound list. It was dropped outright, so
    /// `combine<T: (2..)>` reprinted as `combine<T>` and the drift bailed
    /// `reactive.vl` entirely. An omitted endpoint stays omitted.
    #[test]
    fn tuple_arity_bounds() {
        assert_construct(
            "fun bounds<A: (..10), B: (..: Display), C: (2..4: Display)>() {\n\tbody()\n}\n",
            "fun bounds<A: (..10), B: (..: Display), C: (2..4: Display)>() {\n\tbody()\n}\n",
        );
    }

    /// `...items: T` — a spread parameter (variadic-generics.md §S). The marker
    /// is three `.` control tokens with no node of its own, so a dropped one is
    /// silent token drift; `mut` may precede it, and the pack type is mandatory
    /// so it always reprints too.
    #[test]
    fn spread_parameters() {
        assert_construct(
            "fun log<T: (..: Display)>(...items: T): i32 {\n\t0\n}\n",
            "fun log<T: (..: Display)>(...items: T): i32 {\n\t0\n}\n",
        );
        assert_construct(
            "fun tail<T: (2..)>(sep: str, mut ...rest: T): i32 {\n\t0\n}\n",
            "fun tail<T: (2..)>(sep: str, mut ...rest: T): i32 {\n\t0\n}\n",
        );
        assert_construct(
            "fun gather<T: (2..)>(...sources: (U in T: SignalCell<U>)): SignalCell<T> {\n\
             \tSignal::new(sources)\n\
             }\n",
            "fun gather<T: (2..)>(...sources: (U in T: SignalCell<U>)): SignalCell<T> {\n\
             \tSignal::new(sources)\n\
             }\n",
        );
    }

    /// `void` written as a VALUE prints as `void`; the `Void` the parser
    /// synthesizes for a block with no tail expression is not text and prints as
    /// nothing. Printing both as nothing dropped the argument from
    /// `Verdict::Bad(void)` — token drift, and `option.vl` never formatted again.
    #[test]
    fn written_void_survives_and_a_synthesized_tail_stays_silent() {
        assert_construct(
            "fun voided(): Verdict<i32, void> {\n\tVerdict::Bad(void)\n}\n",
            "fun voided(): Verdict<i32, void> {\n\tVerdict::Bad(void)\n}\n",
        );
        assert_construct("fun empty_tail() {\n}\n", "fun empty_tail() {}\n");
    }
}

#[cfg(test)]
mod paren_groups {
    //! **User-written parentheses are preserved.** A `(…)` group the user wrote
    //! reprints as a group — the formatter does not adjudicate whether it was
    //! redundant, because a redundant group is usually deliberate clarity
    //! (`const (chain + reveal)`).
    //!
    //! Until the formatter's parse recorded groups, every one of these shapes
    //! bailed the WHOLE FILE: the parser dissolved `(expr)` to its inner
    //! expression, the printer put back only the parentheses precedence
    //! demanded, and the re-lex safety net — seeing an output two tokens short —
    //! returned the file's original bytes (which `fmt --check` then called
    //! clean). The formatter now parses in group-preserving mode
    //! (`parsing::parse_preserving_groups`), so the group is a node and reprints
    //! as written.
    //!
    //! Each pin runs the full formatter contract through `assert_construct`:
    //! same tokens out, no silent bail, canonical, idempotent, round-tripping.
    use super::bailing_constructs::assert_construct;
    use super::format;

    // --- The shapes that used to bail ---------------------------------------

    /// `let b = (1 + 2);` — a redundant group around a binary in a `let` value.
    #[test]
    fn a_redundant_group_in_a_let_value() {
        assert_construct(
            "fun f() {\n\tlet b = (1 + 2);\n}\n",
            "fun f() {\n\tlet b = (1 + 2);\n}\n",
        );
    }

    /// `let b = (x);` — a group around a BARE NAME. The atom case is the one
    /// precedence can never reconstruct (an atom is precedence 100, so no
    /// operand minimum ever wraps it), which is why it bailed.
    #[test]
    fn a_redundant_group_around_a_bare_name() {
        assert_construct(
            "fun f() {\n\tlet b = (x);\n}\n",
            "fun f() {\n\tlet b = (x);\n}\n",
        );
    }

    /// `(300).as_u8()` — the same atom case in method-subject position, the
    /// shape that kept `numeric-types.vl` in the corpus bail ledger through
    /// E13. The corpus sites were canonicalized away then; the shape is legal
    /// vilan and now formats.
    #[test]
    fn a_redundant_group_around_a_number_literal_subject() {
        assert_construct(
            "fun f() {\n\tlet b = (300).as_u8();\n}\n",
            "fun f() {\n\tlet b = (300).as_u8();\n}\n",
        );
    }

    /// `ret (1 + 2);` — the group survives an early return.
    #[test]
    fn a_redundant_group_in_a_return() {
        assert_construct(
            "fun f() {\n\tret (1 + 2);\n}\n",
            "fun f() {\n\tret (1 + 2);\n}\n",
        );
    }

    /// `f((1 + 2))` — a group as a call argument, where the call's own
    /// parentheses sit right beside the group's.
    #[test]
    fn a_redundant_group_as_a_call_argument() {
        assert_construct(
            "fun f() {\n\tg((1 + 2))\n}\n",
            "fun f() {\n\tg((1 + 2))\n}\n",
        );
    }

    /// `const (a.b(1) + c)` — the website's shape. `const` captures everything
    /// to its right, so the group is pure clarity; it is kept.
    #[test]
    fn a_redundant_group_after_const() {
        assert_construct(
            "fun f() {\n\tlet b = const (a.b(1) + c);\n}\n",
            "fun f() {\n\tlet b = const (a.b(1) + c);\n}\n",
        );
    }

    /// Nested redundant groups are preserved as written — one node per pair of
    /// parentheses, so `((x))` neither collapses to `(x)` nor to `x`.
    #[test]
    fn nested_redundant_groups_are_preserved_as_written() {
        assert_construct(
            "fun f() {\n\tlet b = ((x));\n}\n",
            "fun f() {\n\tlet b = ((x));\n}\n",
        );
        assert_construct(
            "fun f() {\n\tlet b = (((1 + 2)));\n}\n",
            "fun f() {\n\tlet b = (((1 + 2)));\n}\n",
        );
    }

    // --- Parentheses the printer would have added anyway ---------------------

    /// A group precedence REQUIRES is printed exactly once. The printer's own
    /// wrapping (`print_operand`'s minimum) sees a recorded group as an atom
    /// (precedence 100) and so never wraps it a second time — no `((a + b))`.
    #[test]
    fn a_precedence_required_group_is_not_doubled() {
        for (source, expected) in [
            // A binary operand of a tighter binary.
            ("(a + b) * c", "(a + b) * c"),
            // A binary operand of a prefix operator.
            ("-(2 + 3)", "-(2 + 3)"),
            ("&(a + b)", "&(a + b)"),
            // A member/index result being CALLED — `(self.fn)()` is a
            // field-closure call, `self.fn()` a method call.
            ("(self.fn)()", "(self.fn)()"),
            // A `?.` chain as a postfix subject: without the parens the chain
            // absorbs the `.z`.
            ("(x?.y).z", "(x?.y).z"),
        ] {
            assert_construct(
                &format!("fun f(self) {{\n\t{source}\n}}\n"),
                &format!("fun f(self) {{\n\t{expected}\n}}\n"),
            );
        }
    }

    /// The printer still ADDS the parentheses a reparse needs when the source
    /// did not write them — group preservation adds parentheses, it never
    /// removes the reconstruction. (`a + b` written without parens under a
    /// tighter operator can only arise from a source that had them, so the
    /// contrast pinned here is the unparenthesized form staying bare.)
    #[test]
    fn minimal_parentheses_are_still_minimal_without_a_group() {
        assert_construct("fun f() {\n\ta + b * c\n}\n", "fun f() {\n\ta + b * c\n}\n");
        assert_construct("fun f() {\n\tx.y.z\n}\n", "fun f() {\n\tx.y.z\n}\n");
    }

    // --- Idempotency over a paren-heavy fixture ------------------------------

    /// A file whose every statement carries parentheses of some kind — kept,
    /// required, nested, synthetic (`i"…"` expands to a parenthesized
    /// concatenation), and split across lines — is a fixed point, and its
    /// messy spelling canonicalizes to it.
    #[test]
    fn a_paren_heavy_file_is_a_fixed_point() {
        let canonical = "let plain = (1 + 2);\n\
                         let bare = (x);\n\
                         let nested = ((x));\n\
                         let required = (a + b) * c;\n\
                         let prefixed = -(2 + 3);\n\
                         let called = (registry.handler)(1);\n\
                         let text = (i\"hi {name}!\");\n\
                         \n\
                         fun f() {\n\
                         \tg((1 + 2));\n\
                         \tlet wide = const (style()\n\
                         \t\t.display(Display::Flex)\n\
                         \t\t.flex_direction(FlexDirection::Column)\n\
                         \t\t.gap(space(4))\n\
                         \t\t+ reveal);\n\
                         \tret (1 + 2);\n\
                         }\n";
        assert_eq!(
            format(canonical),
            canonical,
            "the paren-heavy fixture is not a fixed point"
        );
        let messy = "let plain=(1+2);\n\
                     let bare=(x);\n\
                     let nested=((x));\n\
                     let required=(a+b)*c;\n\
                     let prefixed= -(2+3);\n\
                     let called=(registry.handler)(1);\n\
                     let text=(i\"hi {name}!\");\n\
                     \n\
                     fun f(){\n\
                     g((1+2));\n\
                     let wide=const (style().display(Display::Flex)\
                     .flex_direction(FlexDirection::Column).gap(space(4))+reveal);\n\
                     ret (1+2);\n\
                     }\n";
        assert_eq!(
            format(messy),
            canonical,
            "the messy spelling did not canonicalize"
        );
    }
}

#[cfg(test)]
mod chain_splitting {
    //! The formatter's width-aware decision at STATEMENT level: a statement
    //! whose inline rendering is wider than [`LINE_BUDGET`] columns (a tab
    //! counting as [`TAB_COLUMNS`]) and whose expression carries a postfix chain
    //! of two or more `.name(…)` call links re-renders with the subject on the
    //! statement's line and every link on its own, one indentation level in.
    //! Everything else about the reprint is unchanged, so each pin runs the full
    //! formatter contract (same tokens out, no silent bail, canonical,
    //! idempotent). The same rule applied to the lines a split PRODUCES — a
    //! link's own line, a list element's — is pinned in
    //! [`super::nested_layout`].
    use super::bailing_constructs::assert_construct;
    use super::{LINE_BUDGET, display_width, format};

    /// The width the formatter measures for one rendered line.
    pub(super) fn columns(line: &str) -> usize {
        display_width(line)
    }

    /// Asserts a fixture line really is over the budget, so that a pin about a
    /// statement *not* splitting proves a rule rather than a short line.
    pub(super) fn assert_over_budget(line: &str) {
        assert!(
            columns(line) > LINE_BUDGET,
            "fixture is only {} columns, so it proves nothing about the budget: {line:?}",
            columns(line)
        );
    }

    // --- The shape (S1/S2) ---------------------------------------------------

    /// The motivating line, from the website's `page.vl`: a style-builder chain
    /// the formatter used to collapse onto one 101-column line.
    #[test]
    fn an_over_width_chain_splits_one_link_per_line() {
        let source = "let stack = const style().display(Display::Flex)\
                      .flex_direction(FlexDirection::Column).gap(space(4));\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let stack = const style()\n\
             \t.display(Display::Flex)\n\
             \t.flex_direction(FlexDirection::Column)\n\
             \t.gap(space(4));\n",
        );
    }

    /// The boundary, arithmetically: `let padded = s.aa("…").bb(2);` is 28
    /// columns of code around the padding string (13 for `let padded = `, 6 for
    /// `s.aa("`, 9 for `").bb(2);`), so 72 padding characters make exactly the
    /// 100-column budget and 73 make 101. At the budget the statement stays
    /// inline; one column over, it splits.
    #[test]
    fn exactly_the_budget_stays_inline_and_one_column_over_splits() {
        let at_budget = format!("let padded = s.aa(\"{}\").bb(2);\n", "P".repeat(72));
        let over_budget = format!("let padded = s.aa(\"{}\").bb(2);\n", "P".repeat(73));
        assert_eq!(columns(at_budget.trim_end()), LINE_BUDGET);
        assert_eq!(columns(over_budget.trim_end()), LINE_BUDGET + 1);
        assert_construct(&at_budget, &at_budget);
        assert_construct(
            &over_budget,
            &format!("let padded = s\n\t.aa(\"{}\")\n\t.bb(2);\n", "P".repeat(73)),
        );
    }

    /// The width is the statement's own line, so its leading indentation counts
    /// (a tab as four columns) and the links land one level past *that* — the
    /// same chain splits inside a block that would have fit at the top level.
    #[test]
    fn indentation_counts_toward_the_budget_and_the_links_indent_past_it() {
        // 97 columns at the top level, 101 inside one block (one tab = four).
        let statement = format!("let padded = s.aa(\"{}\").bb(2);", "P".repeat(69));
        assert_eq!(columns(&statement), LINE_BUDGET - 3);
        assert_construct(
            &format!("fun main() {{\n\t{statement}\n}}\n"),
            &format!(
                "fun main() {{\n\tlet padded = s\n\t\t.aa(\"{}\")\n\t\t.bb(2);\n}}\n",
                "P".repeat(69)
            ),
        );
    }

    /// A block's tail expression is a statement position too — this is the shape
    /// `std::process::rpc_server`'s `serve_rpc` (since retired, E71) reflowed
    /// into.
    #[test]
    fn a_block_tail_expression_splits() {
        let tail = "Server::builder().port(port).on_request(|request| handle(protocol, request))\
                    .on_start(on_ready).build().start()";
        assert_over_budget(&format!("\t{tail}"));
        assert_construct(
            &format!("fun serve(port: i32) {{\n\t{tail}\n}}\n"),
            "fun serve(port: i32) {\n\
             \tServer::builder()\n\
             \t\t.port(port)\n\
             \t\t.on_request(|request| handle(protocol, request))\n\
             \t\t.on_start(on_ready)\n\
             \t\t.build()\n\
             \t\t.start()\n\
             }\n",
        );
    }

    /// Every statement-value position reaches the chain through its prefix — an
    /// assignment's right-hand side, a `ret` value, an `await` operand — not
    /// just a `let` initializer. Each fixture is one column over the budget
    /// (`\ttotal = s.aa("…").bb(2);` is 27 columns around the padding, `\tret
    /// s.aa("…").bb(2)` is 22, `\tlet awaited = await s.aa("…").bb(2);` is 39).
    #[test]
    fn an_assignment_a_return_and_an_await_all_split() {
        let assignment = format!(
            "fun main() {{\n\ttotal = s.aa(\"{}\").bb(2);\n}}\n",
            "P".repeat(74)
        );
        let returned = format!(
            "fun give(): i32 {{\n\tret s.aa(\"{}\").bb(2)\n}}\n",
            "P".repeat(79)
        );
        let awaited = format!(
            "fun main() {{\n\tlet awaited = await s.aa(\"{}\").bb(2);\n}}\n",
            "P".repeat(62)
        );
        assert_construct(
            &assignment,
            &format!(
                "fun main() {{\n\ttotal = s\n\t\t.aa(\"{}\")\n\t\t.bb(2);\n}}\n",
                "P".repeat(74)
            ),
        );
        assert_construct(
            &returned,
            &format!(
                "fun give(): i32 {{\n\tret s\n\t\t.aa(\"{}\")\n\t\t.bb(2)\n}}\n",
                "P".repeat(79)
            ),
        );
        assert_construct(
            &awaited,
            &format!(
                "fun main() {{\n\tlet awaited = await s\n\t\t.aa(\"{}\")\n\t\t.bb(2);\n}}\n",
                "P".repeat(62)
            ),
        );
    }

    // --- What does *not* split ------------------------------------------------

    /// The collapse is preserved: a chain the author split by hand that fits the
    /// budget still comes back as one line. The choice is purely width-driven.
    #[test]
    fn an_under_width_hand_split_chain_collapses() {
        assert_construct(
            "fun main() {\n\tlet short = one()\n\t\t.two(2)\n\t\t.three(3);\n}\n",
            "fun main() {\n\tlet short = one().two(2).three(3);\n}\n",
        );
    }

    /// A chain that is a call's LAST argument breaks there
    /// (`proposal/argument-tail-descent.md`): the statement is not itself a
    /// chain, so its only breakable construct is the one inside the argument,
    /// and the permission now reaches it. The links indent one level past the
    /// statement and the enclosing `))` glues after the last of them.
    ///
    /// This pin asserted the opposite until backlog 43 — that was
    /// `Split::Statement`'s v1 scope, and the line simply stayed long.
    #[test]
    fn a_chain_nested_in_an_argument_splits_at_the_tail() {
        let source = "let nested = outer(subject.first(1).second(2).third(3).fourth(4)\
                      .fifth(5).sixth(666666666666666666));\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let nested = outer(subject\n\
             \t.first(1)\n\
             \t.second(2)\n\
             \t.third(3)\n\
             \t.fourth(4)\n\
             \t.fifth(5)\n\
             \t.sixth(666666666666666666));\n",
        );
    }

    /// One `.name(…)` link is not a chain — breaking it would buy a line and no
    /// clarity — and a long call that is not a chain at all never splits
    /// (argument wrapping is out of scope).
    #[test]
    fn a_single_link_and_a_plain_long_call_stay_inline() {
        let single_link = "let value = subject\
                           .only_one_link_here_but_a_very_long_one_indeed_truly_yes_it_is_here\
                           (1234567890123);\n";
        let plain_call = "let value = plain_function_call_with_no_chain_at_all(argument_one, \
                          argument_two, argument_three_xyz);\n";
        assert_over_budget(single_link.trim_end());
        assert_over_budget(plain_call.trim_end());
        assert_construct(single_link, single_link);
        assert_construct(plain_call, plain_call);
    }

    /// WIDTH does not split a statement that spans lines: the budget is read from
    /// the opening line (backlog 44), and here that line fits. The spanning link
    /// is the chain's LAST, so the seam rule (`proposal/chain-seam-split.md`) has
    /// nothing to say about it either — which is what makes this a pin about
    /// width rather than about the trailing closure.
    ///
    /// This fixture used to end `}).on("beta", …)`, which is a seam, and the
    /// pin asserted it stayed inline. That expectation moved with backlog 48;
    /// the seam form is pinned in [`super::chain_seam_layout`].
    #[test]
    fn a_spanning_last_link_is_not_split_by_width() {
        let source = "fun main() {\n\
                      \tlet built = registry.first(argument).on(\"beta\", |event| {\n\
                      \t\thandle(event);\n\
                      \t});\n\
                      }\n";
        assert_construct(source, source);
    }

    /// A `?.` lift chain absorbs everything that follows it into one node, so it
    /// has no postfix spine to break: it stays inline (a v1 gap, recorded here so
    /// the behavior is a decision rather than an accident).
    #[test]
    fn a_lift_chain_stays_inline() {
        let source = "let lifted = alpha?.beta(1).gamma(2)\
                      .delta(\"ZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZZ\");\n";
        assert_over_budget(source.trim_end());
        assert_construct(source, source);
    }

    // --- Glue and continuations (S3/S4) --------------------------------------

    /// A non-call postfix — a plain member, an index, a `!` — glues to the
    /// segment printed before it rather than taking a line of its own.
    #[test]
    fn non_call_postfixes_glue_to_the_preceding_segment() {
        let member = format!(
            "let glued = alpha.beta(1).gamma.delta(\"{}\");\n",
            "G".repeat(60)
        );
        let indexed = format!(
            "let glued = alpha.beta(1)[0].gamma(2).delta(\"{}\");\n",
            "G".repeat(53)
        );
        let asserted = format!(
            "let glued = alpha.beta(1)!.gamma(2)!.delta(\"{}\");\n",
            "G".repeat(54)
        );
        assert_over_budget(member.trim_end());
        assert_over_budget(indexed.trim_end());
        assert_over_budget(asserted.trim_end());
        assert_construct(
            &member,
            &format!(
                "let glued = alpha\n\t.beta(1).gamma\n\t.delta(\"{}\");\n",
                "G".repeat(60)
            ),
        );
        assert_construct(
            &indexed,
            &format!(
                "let glued = alpha\n\t.beta(1)[0]\n\t.gamma(2)\n\t.delta(\"{}\");\n",
                "G".repeat(53)
            ),
        );
        assert_construct(
            &asserted,
            &format!(
                "let glued = alpha\n\t.beta(1)!\n\t.gamma(2)!\n\t.delta(\"{}\");\n",
                "G".repeat(54)
            ),
        );
    }

    /// The website's `heading` shape: the chain is the left operand of a `+`, so
    /// the operator and its right operand continue on their own line at the
    /// links' indentation, with the terminator glued after.
    #[test]
    fn a_chain_operand_of_a_binary_puts_the_continuation_on_its_own_line() {
        let source = "let heading = const style().raw(\"font-family\", display_face)\
                      .font_size(Length::px(32.0)).raw(\"line-height\", \"48px\")\
                      .font_weight(600).margin(space(0)) + reveal;\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let heading = const style()\n\
             \t.raw(\"font-family\", display_face)\n\
             \t.font_size(Length::px(32.0))\n\
             \t.raw(\"line-height\", \"48px\")\n\
             \t.margin(space(0))\n\
             \t.font_weight(600)\n\
             \t+ reveal;\n",
        );
    }

    /// The same statement as the website actually writes it, with the redundant
    /// parentheses `const (…)` around the sum. The group is the source's own, so
    /// the split happens INSIDE it and the closing paren glues after the
    /// continuation — the parentheses are preserved, not adjudicated.
    #[test]
    fn a_parenthesized_chain_operand_keeps_its_parentheses() {
        assert_construct(
            "let heading = const (style().raw(\"font-family\", display_face)\
             .font_size(Length::px(32.0)).raw(\"line-height\", \"48px\")\
             .font_weight(600).margin(space(0)) + reveal);\n",
            "let heading = const (style()\n\
             \t.raw(\"font-family\", display_face)\n\
             \t.font_size(Length::px(32.0))\n\
             \t.raw(\"line-height\", \"48px\")\n\
             \t.margin(space(0))\n\
             \t.font_weight(600)\n\
             \t+ reveal);\n",
        );
    }

    // --- Comments and idempotency (S6/S7) ------------------------------------

    /// A comment written between two links attaches to the link it precedes
    /// (`proposal/split-comment-attachment.md`). It also FORCES the split form:
    /// collapsed, the chain has no line to keep the comment on, which is how it
    /// used to end up orphaned below the whole statement. Both fixtures are
    /// therefore fixed points — what the author wrote is what comes back.
    #[test]
    fn a_mid_chain_comment_attaches_to_its_link() {
        let short = "fun main() {\n\tlet short = one()\n\t\t// a note\n\t\t.two(2)\n\
                     \t\t.three(3);\n\tdone()\n}\n";
        assert_construct(short, short);
        let long = format!(
            "fun main() {{\n\tlet padded = s\n\t\t// a note\n\t\t.aa(\"{}\")\n\t\t.bb(2);\n\
             \tdone()\n}}\n",
            "P".repeat(69)
        );
        assert_construct(&long, &long);
    }

    /// Formatting is a fixed point over a file that mixes both forms: the split
    /// chains stay split, the inline ones stay inline, and nothing drifts on a
    /// second pass.
    #[test]
    fn a_file_mixing_split_and_inline_chains_is_a_fixed_point() {
        let source = "let stack = const style().display(Display::Flex)\
                      .flex_direction(FlexDirection::Column).gap(space(4));\n\
                      \n\
                      let small = style().gap(space(4));\n\
                      \n\
                      fun main() {\n\
                      \tlet nested = outer(subject.first(1).second(2).third(3).fourth(4)\
                      .fifth(5).sixth(666666666666666666));\n\
                      \tregistry.on(\"alpha\", handle_alpha).on(\"beta\", handle_beta)\
                      .on(\"gamma\", handle_gamma).build_this()\n\
                      }\n";
        let once = format(source);
        assert_eq!(format(&once), once, "formatting is not a fixed point");
        assert!(
            once.contains("let stack = const style()\n\t.display(Display::Flex)\n"),
            "the wide chain did not split:\n{once}"
        );
        assert!(
            once.contains("let small = style().gap(space(4));\n"),
            "the narrow chain did not stay inline:\n{once}"
        );
        assert!(
            once.contains("\tlet nested = outer(subject\n\t\t.first(1)\n"),
            "the chain nested in an argument did not split at the tail:\n{once}"
        );
        assert!(
            once.contains("\tregistry\n\t\t.on(\"alpha\", handle_alpha)\n"),
            "the wide tail chain did not split:\n{once}"
        );
    }
}

#[cfg(test)]
mod nested_layout {
    //! The budget is per-LINE and applies RECURSIVELY. A split chain gives each
    //! `.name(…)` link its own continuation line, so each link's line is
    //! measured the same way a statement's is; over budget, the link's call
    //! breaks its LAST argument one indentation level deeper — a nested chain
    //! into links, a list literal into elements — and the descent walks through
    //! nested calls until it reaches something breakable. A list element's line
    //! is measured the same way, so an element that is itself a chain splits
    //! too. Entry is width only, at every level: what fits stays inline.
    //!
    //! Each pin runs the whole formatter contract through `assert_construct`
    //! (same tokens out — nothing dropped, comments included — no silent bail,
    //! canonical form reached in ONE pass, idempotent).
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::{assert_over_budget, columns};
    use super::{LINE_BUDGET, format};

    // --- R1: a link's own line is measured, and its argument splits ----------

    /// The motivating shape, from the website's `art.vl` `diagram()`: a
    /// hand-nested `std::ui` view tree flattened onto one line. The statement's
    /// chain splits, then each `.child(…)` link whose OWN line overflows splits
    /// its argument tree one level deeper — its subject staying on the link's
    /// line after `.child(`, its links one level in, and the enclosing call's
    /// `)` gluing after the argument's last line. The subtrees that fit stay
    /// inline (R2), which is what keeps the shape readable rather than maximal.
    #[test]
    fn an_over_budget_link_splits_its_argument_tree() {
        assert_construct(
            "fun diagram(): View {\n\
             \tview(\"div\").styled(art_stage).child(grain()).child(view(\"div\")\
             .styled(dg_source).child(view(\"p\").styled(art_tab).text(\"notes.vl - one source\"))\
             .child(view(\"div\").styled(art_code).child(ln([kw(\"let\"), t(\" notes\")]))))\n\
             }\n",
            "fun diagram(): View {\n\
             \tview(\"div\")\n\
             \t\t.styled(art_stage)\n\
             \t\t.child(grain())\n\
             \t\t.child(view(\"div\")\n\
             \t\t\t.styled(dg_source)\n\
             \t\t\t.child(view(\"p\").styled(art_tab).text(\"notes.vl - one source\"))\n\
             \t\t\t.child(view(\"div\").styled(art_code)\
             .child(ln([kw(\"let\"), t(\" notes\")]))))\n\
             }\n",
        );
    }

    /// The boundary one level in, arithmetically. A link's line is its
    /// indentation (one tab = four columns) plus the link's own rendering:
    /// `.wrap(inner.aa("…").bb(2))` is 25 columns around the padding, so 71
    /// padding characters make exactly the budget and 72 make 101. What the
    /// enclosing statement glues AFTER the last link — here the `;` — is the
    /// statement's, not the link's, and is deliberately not measured.
    #[test]
    fn a_link_line_at_the_budget_stays_inline_and_one_column_over_splits() {
        let statement = |padding: &str| {
            format!("let built = subject.wrap(inner.aa(\"{padding}\").bb(2)).tail(3);\n")
        };
        let at_budget = "P".repeat(71);
        let over_budget = "P".repeat(72);
        assert_eq!(
            columns(&format!("\t.wrap(inner.aa(\"{at_budget}\").bb(2))")),
            LINE_BUDGET
        );
        assert_eq!(
            columns(&format!("\t.wrap(inner.aa(\"{over_budget}\").bb(2))")),
            LINE_BUDGET + 1
        );
        assert_construct(
            &statement(&at_budget),
            &format!(
                "let built = subject\n\t.wrap(inner.aa(\"{at_budget}\").bb(2))\n\t.tail(3);\n"
            ),
        );
        assert_construct(
            &statement(&over_budget),
            &format!(
                "let built = subject\n\
                 \t.wrap(inner\n\
                 \t\t.aa(\"{over_budget}\")\n\
                 \t\t.bb(2))\n\
                 \t.tail(3);\n"
            ),
        );
    }

    // --- R2: entry is width only, in both directions -------------------------

    /// A hand-nested tree that fits collapses whole — the nested links come back
    /// onto the statement's line exactly as the statement's own do. Nothing
    /// about a view builder makes it split; only its width does.
    #[test]
    fn a_hand_nested_tree_that_fits_collapses() {
        assert_construct(
            "fun small(): View {\n\
             \tview(\"div\")\n\
             \t\t.styled(art_stage)\n\
             \t\t.child(view(\"p\")\n\
             \t\t\t.styled(art_tab)\n\
             \t\t\t.text(\"short\"))\n\
             }\n",
            "fun small(): View {\n\
             \tview(\"div\").styled(art_stage).child(view(\"p\").styled(art_tab).text(\"short\"))\n\
             }\n",
        );
    }

    // --- R3: list literals -----------------------------------------------------

    /// A list literal on an over-budget line breaks one element per line, one
    /// indentation level past the line that opened the `[`, with a trailing
    /// comma after EVERY element (the last included, so adding one is a one-line
    /// diff) and `]` back at the opening line's indent, where the statement's
    /// terminator glues after it.
    #[test]
    fn an_over_budget_list_literal_splits_one_element_per_line() {
        let source = "let wide = [alpha_element_one, beta_element_two, gamma_element_three, \
                      delta_element_four, epsilon_five];\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let wide = [\n\
             \talpha_element_one,\n\
             \tbeta_element_two,\n\
             \tgamma_element_three,\n\
             \tdelta_element_four,\n\
             \tepsilon_five,\n\
             ];\n",
        );
    }

    /// The other direction, and the trailing comma's whole rule: a list that
    /// fits stays inline WITHOUT one — a hand-written trailing comma in a
    /// fitting list is dropped, so the comma marks a split list and nothing
    /// else.
    #[test]
    fn a_list_that_fits_stays_inline_without_a_trailing_comma() {
        assert_construct(
            "let fits = [alpha, beta, gamma];\n",
            "let fits = [alpha, beta, gamma];\n",
        );
        assert_construct(
            "let fits = [alpha, beta, gamma,];\n",
            "let fits = [alpha, beta, gamma];\n",
        );
    }

    /// An EMPTY list is never broken: `[⏎]` buys a line and no clarity. Here the
    /// over-budget cause is an earlier argument, so the armed tail reaches the
    /// empty list and declines.
    #[test]
    fn an_empty_list_at_an_over_budget_tail_stays_inline() {
        let padding = "P".repeat(81);
        assert_construct(
            &format!("let built = subject.wrap(\"{padding}\", []).tail(3);\n"),
            &format!("let built = subject\n\t.wrap(\"{padding}\", [])\n\t.tail(3);\n"),
        );
    }

    // --- R4: mixed nesting composes ------------------------------------------

    /// The website's footer shape — three constructs deep: the statement's chain
    /// splits, the `.child(…)` link's line overflows, the descent walks through
    /// `footer_column("title", …)` to its LAST argument, and that list breaks one
    /// element per line. The closing `]))` glues at the opening line's indent.
    #[test]
    fn a_links_tail_descends_through_a_call_into_a_list() {
        assert_construct(
            "fun footer(): View {\n\
             \tview(\"div\").styled(footer_grid).child(footer_column(\"Community\", \
             [view(\"a\").styled(footer_link).text(\"Issues\"), \
             view(\"a\").styled(footer_link).text(\"Discussions\")]))\n\
             }\n",
            "fun footer(): View {\n\
             \tview(\"div\")\n\
             \t\t.styled(footer_grid)\n\
             \t\t.child(footer_column(\"Community\", [\n\
             \t\t\tview(\"a\").styled(footer_link).text(\"Issues\"),\n\
             \t\t\tview(\"a\").styled(footer_link).text(\"Discussions\"),\n\
             \t\t]))\n\
             }\n",
        );
    }

    /// Four deep, and the composition closing back on itself: a list ELEMENT
    /// whose own line overflows is a chain, so it splits as one, its links one
    /// level past the element's indent. The element that fits stays inline
    /// beside it.
    #[test]
    fn an_over_budget_list_element_splits_as_a_chain() {
        assert_construct(
            "fun deep(): View {\n\
             \troot().styled(base).child(column(\"Community\", [alpha().styled(link)\
             .attr(\"href\", \"/a/rather/longer/path/that/pushes/over\").text(\"Issues\"), \
             beta().text(\"Short\")]))\n\
             }\n",
            "fun deep(): View {\n\
             \troot()\n\
             \t\t.styled(base)\n\
             \t\t.child(column(\"Community\", [\n\
             \t\t\talpha()\n\
             \t\t\t\t.styled(link)\n\
             \t\t\t\t.attr(\"href\", \"/a/rather/longer/path/that/pushes/over\")\n\
             \t\t\t\t.text(\"Issues\"),\n\
             \t\t\tbeta().text(\"Short\"),\n\
             \t\t]))\n\
             }\n",
        );
    }

    // --- R5: one argument recurses, siblings stay inline ----------------------

    /// The recorded limitation. Layout hangs off a call's LAST argument — the
    /// universal builder/DSL convention — so when an EARLIER argument is the
    /// over-budget cause the line simply stays long. Breaking there needs
    /// argument-list layout design, which nothing in the code motivates yet.
    #[test]
    fn an_earlier_argument_does_not_recurse_and_the_line_stays_long() {
        let padding = "P".repeat(72);
        let split_line = format!("\t.wrap(inner.aa(\"{padding}\").bb(2), tail)");
        assert_over_budget(&split_line);
        assert_construct(
            &format!("let built = subject.wrap(inner.aa(\"{padding}\").bb(2), tail).other(3);\n"),
            &format!("let built = subject\n{split_line}\n\t.other(3);\n"),
        );
    }

    // --- R6: closures keep today's printing ----------------------------------

    /// A closure argument is not a layout site: an expression-bodied `|x| …`
    /// prints as it always has, so a link whose last argument is one stays on
    /// its long line.
    #[test]
    fn an_expression_bodied_closure_argument_stays_inline() {
        let padding = "P".repeat(56);
        let link = format!("\t.wrap(|event| handle_the_event(event, \"{padding}\"))");
        assert_over_budget(&link);
        assert_construct(
            &format!(
                "let built = subject.wrap(|event| handle_the_event(event, \"{padding}\"))\
                 .tail(3);\n"
            ),
            &format!("let built = subject\n{link}\n\t.tail(3);\n"),
        );
    }

    /// A closure with a BLOCK body already has line structure, and its
    /// statements go through the same statement hook every block's do: the chain
    /// inside splits against its own indentation. (The enclosing chain does not
    /// split — its inline rendering spans lines — which is why the pin proves
    /// the inner rule rather than the outer one.)
    #[test]
    fn a_chain_inside_a_closure_block_body_splits_at_its_own_indentation() {
        let padding = "P".repeat(69);
        assert_construct(
            &format!(
                "fun main() {{\n\
                 \thandler.on(\"click\", || {{\n\
                 \t\tlet padded = s.aa(\"{padding}\").bb(2);\n\
                 \t}});\n\
                 }}\n"
            ),
            &format!(
                "fun main() {{\n\
                 \thandler.on(\"click\", || {{\n\
                 \t\tlet padded = s\n\
                 \t\t\t.aa(\"{padding}\")\n\
                 \t\t\t.bb(2);\n\
                 \t}});\n\
                 }}\n"
            ),
        );
    }

    // --- R7/R8: comments survive, and the shape is a one-pass fixed point ------

    /// The comment attaches where it was written, which is one level IN: it sits
    /// between the nested chain's links, so that chain is the one forced to
    /// split. The outer chain's own line fits and its only spanning link is its
    /// last, so it stays inline — the rule reaches exactly the construct the
    /// comment is inside of, and no further.
    #[test]
    fn a_comment_inside_a_nested_chain_attaches_there() {
        assert_construct(
            "fun noted(): View {\n\
             \troot().styled(base).child(view(\"div\").styled(inner_style)\n\
             \t\t// a note between the nested links\n\
             \t\t.child(leaf_one()).child(leaf_two_with_a_longer_name()).child(leaf_three_here()))\n\
             }\n",
            "fun noted(): View {\n\
             \troot().styled(base).child(view(\"div\")\n\
             \t\t.styled(inner_style)\n\
             \t\t// a note between the nested links\n\
             \t\t.child(leaf_one())\n\
             \t\t.child(leaf_two_with_a_longer_name())\n\
             \t\t.child(leaf_three_here()))\n\
             }\n",
        );
    }

    /// A hand-FLATTENED deep tree reaches its canonical nested form in ONE pass
    /// — the recursion happens inside the statement's single split reprint, not
    /// by re-running the formatter — and that form is a fixed point. (Every pin
    /// above asserts both through `assert_construct`; this one says it out loud
    /// over a fixture that exercises chain, call-tail and list nesting at once.)
    #[test]
    fn a_flattened_deep_tree_reaches_its_nested_form_in_one_pass() {
        let flattened = "fun deep(): View {\n\
                         \troot().styled(base).child(column(\"Community\", \
                         [alpha().styled(link).attr(\"href\", \
                         \"/a/rather/longer/path/that/pushes/over\").text(\"Issues\"), \
                         beta().text(\"Short\")])).child(view(\"p\").styled(art_caption)\
                         .text(\"one definition, both sides, and the compiler keeps them honest\"))\n\
                         }\n";
        let once = format(flattened);
        assert_eq!(format(&once), once, "formatting is not a fixed point");
        assert!(
            once.contains(
                "\t\t.child(column(\"Community\", [\n\t\t\talpha()\n\t\t\t\t.styled(link)"
            ),
            "one pass did not reach the nested form:\n{once}"
        );
        assert!(
            once.contains("\t\t\tbeta().text(\"Short\"),\n\t\t]))\n"),
            "the fitting element did not stay inline, or the list did not close:\n{once}"
        );
        assert!(
            once.contains("\t\t.child(view(\"p\")\n\t\t\t.styled(art_caption)\n"),
            "the sibling link did not split:\n{once}"
        );
    }
}

#[cfg(test)]
mod struct_literal_layout {
    //! A struct literal is a braced field list, so it breaks on exactly the rule
    //! a bracketed element list breaks on (see [`super::nested_layout`]): over
    //! the budget it renders one `field = value,` per line, one indentation
    //! level in, with a trailing comma after every field — the last included, so
    //! adding a field is a one-line diff — and `}` back at the opening line's
    //! indent. What fits stays inline WITHOUT a trailing comma, so the comma
    //! marks a split literal and nothing else.
    //!
    //! Before this, struct literals were the one composite the width rule did
    //! not reach: a hand-wrapped literal was COLLAPSED onto a single line of
    //! whatever width it came to, with no way to break it again (Kolt's
    //! `KoltStore { … }` came out at 357 columns), because the printer joined
    //! its fields with `", "` unconditionally.
    //!
    //! Each pin runs the whole formatter contract through `assert_construct`
    //! (same tokens out, no silent bail, canonical in ONE pass, idempotent).
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::{assert_over_budget, columns};
    use super::{LINE_BUDGET, format};

    // --- The shape -----------------------------------------------------------

    /// The motivating line, from Kolt's server: a struct literal the formatter
    /// used to collapse onto one 357-column line.
    #[test]
    fn an_over_budget_struct_literal_splits_one_field_per_line() {
        let source = "let store = KoltStore { workspaces = workspaces, tasks = tasks, \
                      register_hook = Shared::new(register), login_hook = Shared::new(login), \
                      create_hook = Shared::new(create) };\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let store = KoltStore {\n\
             \tworkspaces = workspaces,\n\
             \ttasks = tasks,\n\
             \tregister_hook = Shared::new(register),\n\
             \tlogin_hook = Shared::new(login),\n\
             \tcreate_hook = Shared::new(create),\n\
             };\n",
        );
    }

    /// The other direction, and the trailing comma's whole rule — the same one
    /// a list literal follows: a literal that fits stays inline WITHOUT one, and
    /// a hand-written trailing comma in a fitting literal is dropped.
    #[test]
    fn a_struct_literal_that_fits_stays_inline_without_a_trailing_comma() {
        assert_construct(
            "let fits = Point { x = 1, y = 2 };\n",
            "let fits = Point { x = 1, y = 2 };\n",
        );
        assert_construct(
            "let fits = Point { x = 1, y = 2, };\n",
            "let fits = Point { x = 1, y = 2 };\n",
        );
    }

    /// The boundary, arithmetically: `let padded = P { aa = "…" };` is 27
    /// columns of code around the padding string, so 73 padding characters make
    /// exactly the 100-column budget and 74 make 101. At the budget the literal
    /// stays inline; one column over, it splits.
    #[test]
    fn exactly_the_budget_stays_inline_and_one_column_over_splits() {
        let at_budget = format!("let padded = P {{ aa = \"{}\" }};\n", "P".repeat(73));
        let over_budget = format!("let padded = P {{ aa = \"{}\" }};\n", "P".repeat(74));
        assert_eq!(columns(at_budget.trim_end()), LINE_BUDGET);
        assert_eq!(columns(over_budget.trim_end()), LINE_BUDGET + 1);
        assert_construct(&at_budget, &at_budget);
        assert_construct(
            &over_budget,
            &format!("let padded = P {{\n\taa = \"{}\",\n}};\n", "P".repeat(74)),
        );
    }

    /// The width is the statement's own line, so its leading indentation counts
    /// (a tab as four columns): the same literal stays inline at the top level
    /// and splits inside a block that pushes it over.
    #[test]
    fn indentation_counts_toward_the_budget() {
        let padding = "P".repeat(70);
        let statement = format!("let padded = P {{ aa = \"{padding}\" }};");
        // 97 columns at the top level, 101 inside one block (one tab = four).
        assert_eq!(columns(&statement), LINE_BUDGET - 3);
        assert_construct(&format!("{statement}\n"), &format!("{statement}\n"));
        assert_construct(
            &format!("fun demo() {{\n\t{statement}\n}}\n"),
            &format!(
                "fun demo() {{\n\
                 \tlet padded = P {{\n\
                 \t\taa = \"{padding}\",\n\
                 \t}};\n\
                 }}\n"
            ),
        );
    }

    /// An EMPTY literal is never broken: `{⏎}` buys a line and no clarity. Here
    /// the over-budget cause is an earlier argument, so the armed tail reaches
    /// the empty literal and declines — the same pin the empty list carries.
    #[test]
    fn an_empty_struct_literal_at_an_over_budget_tail_stays_inline() {
        let padding = "P".repeat(81);
        let split_line = format!("\t.wrap(\"{padding}\", Empty {{}})");
        assert_over_budget(&split_line);
        assert_construct(
            &format!("let built = subject.wrap(\"{padding}\", Empty {{}}).tail(3);\n"),
            &format!("let built = subject\n{split_line}\n\t.tail(3);\n"),
        );
    }

    /// Shorthand fields (no `= value`) and generic arguments ride the split
    /// unchanged: the arguments stay on the opening line with the name, where
    /// they belong, and a shorthand field takes a line like any other.
    #[test]
    fn shorthand_fields_and_generic_arguments_ride_the_split() {
        let source = "let generic = Wrapper<Task> { alpha, beta_field_name, gamma = compute(a), \
                      delta = other(b), epsilon = third(c) };\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let generic = Wrapper<Task> {\n\
             \talpha,\n\
             \tbeta_field_name,\n\
             \tgamma = compute(a),\n\
             \tdelta = other(b),\n\
             \tepsilon = third(c),\n\
             };\n",
        );
    }

    // --- The recursion: a field's line is measured like any other ------------

    /// A field whose own line overflows splits in turn, so a struct literal
    /// nested in a field breaks one level past it — and the field that fits
    /// stays inline beside it.
    #[test]
    fn an_over_budget_field_splits_a_nested_struct_literal() {
        assert_construct(
            "fun demo() {\n\
             \tlet deep = Outer { name = \"a\", inner = Inner { alpha = one_value_here, \
             beta = two_value_here, gamma = three_value, delta = four_value, epsilon = five }, \
             tail = 1 };\n\
             }\n",
            "fun demo() {\n\
             \tlet deep = Outer {\n\
             \t\tname = \"a\",\n\
             \t\tinner = Inner {\n\
             \t\t\talpha = one_value_here,\n\
             \t\t\tbeta = two_value_here,\n\
             \t\t\tgamma = three_value,\n\
             \t\t\tdelta = four_value,\n\
             \t\t\tepsilon = five,\n\
             \t\t},\n\
             \t\ttail = 1,\n\
             \t};\n\
             }\n",
        );
    }

    /// The composition the other way: a field whose value is a postfix chain
    /// splits as one, its links one level past the field.
    #[test]
    fn an_over_budget_field_splits_as_a_chain() {
        assert_construct(
            "fun demo() {\n\
             \tlet chained = Holder { label = \"x\", built = subject.first_link(argument_one)\
             .second_link(argument_two).third_link(three_arg).fourth(f) };\n\
             }\n",
            "fun demo() {\n\
             \tlet chained = Holder {\n\
             \t\tlabel = \"x\",\n\
             \t\tbuilt = subject\n\
             \t\t\t.first_link(argument_one)\n\
             \t\t\t.second_link(argument_two)\n\
             \t\t\t.third_link(three_arg)\n\
             \t\t\t.fourth(f),\n\
             \t};\n\
             }\n",
        );
    }

    /// And into a list: the field's comma glues after the `]`, exactly as the
    /// statement's `;` glues after a split list's.
    #[test]
    fn an_over_budget_field_splits_a_list() {
        assert_construct(
            "fun demo() {\n\
             \tlet listed = Holder { label = \"x\", items = [alpha_element_one, \
             beta_element_two, gamma_element_three, delta_element_four, epsilon] };\n\
             }\n",
            "fun demo() {\n\
             \tlet listed = Holder {\n\
             \t\tlabel = \"x\",\n\
             \t\titems = [\n\
             \t\t\talpha_element_one,\n\
             \t\t\tbeta_element_two,\n\
             \t\t\tgamma_element_three,\n\
             \t\t\tdelta_element_four,\n\
             \t\t\tepsilon,\n\
             \t\t],\n\
             \t};\n\
             }\n",
        );
    }

    /// The descent reaches a literal the same way it reaches a list: a chain
    /// link whose line overflows breaks its LAST argument, and a struct literal
    /// sitting there is now something breakable. The `})` closes at the link's
    /// indent.
    #[test]
    fn a_links_tail_descends_into_a_struct_literal() {
        assert_construct(
            "fun tail(): View {\n\
             \tview(\"div\").styled(shell).child(Card { title = \"A rather long title here\", \
             body = \"and a body\", footer = \"plus a footer\" })\n\
             }\n",
            "fun tail(): View {\n\
             \tview(\"div\")\n\
             \t\t.styled(shell)\n\
             \t\t.child(Card {\n\
             \t\t\ttitle = \"A rather long title here\",\n\
             \t\t\tbody = \"and a body\",\n\
             \t\t\tfooter = \"plus a footer\",\n\
             \t\t})\n\
             }\n",
        );
    }

    // --- Stability -----------------------------------------------------------

    /// Backlog 41, now shipped for the literal too: a comment between fields
    /// attaches to the field it precedes and forces the split, so the literal
    /// keeps the shape its author gave it. This fixture fits the budget, which
    /// is the point — before, it collapsed and the comment fell out below.
    #[test]
    fn a_comment_between_fields_attaches_to_its_field() {
        let source = "fun demo() {\n\
                      \tlet store = KoltStore {\n\
                      \t\t// the authoritative lists\n\
                      \t\tworkspaces = workspaces,\n\
                      \t\ttasks = tasks,\n\
                      \t\tregister_hook = Shared::new(register),\n\
                      \t};\n\
                      }\n";
        assert_construct(source, source);
    }

    /// A file carrying both forms is a fixed point in one pass: the split
    /// literal stays split, the fitting one stays inline and uncommaed, and
    /// neither drifts on a second run.
    #[test]
    fn a_file_mixing_split_and_inline_struct_literals_is_a_fixed_point() {
        let canonical = "let fits = Point { x = 1, y = 2 };\n\
                         let store = KoltStore {\n\
                         \tworkspaces = workspaces,\n\
                         \ttasks = tasks,\n\
                         \tregister_hook = Shared::new(register),\n\
                         \tlogin_hook = Shared::new(login),\n\
                         \tcreate_hook = Shared::new(create),\n\
                         };\n";
        assert_construct(canonical, canonical);
        assert_eq!(format(canonical), canonical);
    }
}

#[cfg(test)]
mod spanning_renderings {
    //! A rendering that spans lines is measured by its FIRST line, because that
    //! is the line the split decision is about. The body lines of a block-bodied
    //! closure, a `match` or a block are printed — and measured — where they
    //! are; they say nothing about the line that opened them.
    //!
    //! This used to be the opposite: a rendering containing any newline was
    //! refused a measurement, which exempted the ENTIRE statement from the
    //! budget. One block-bodied closure at the tail of a `std::ui` tree kept the
    //! whole chain inline at any width — `examples/reactive-ui/todos.vl`, hand
    //! split by its author, reformatted into a single 707-column line, and the
    //! formatter had no way back out of it.
    //!
    //! Each pin runs the whole formatter contract through `assert_construct`.
    use super::LINE_BUDGET;
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::{assert_over_budget, columns};

    /// The motivating shape, reduced from `todos.vl`: a chain whose last link
    /// takes a block-bodied closure. The chain splits like any other, the link
    /// opens its line with `|| {`, and the closure's body indents one level past
    /// the link — the body was never the problem.
    #[test]
    fn a_chain_ending_in_a_block_closure_splits() {
        assert_construct(
            "fun demo(): View {\n\
             \tview(\"section\").class(\"todos\").child(view(\"h2\").text(\"Todos\"))\
             .when(items.map(|list| count_done(list) > 0), || {\n\
             \t\tview(\"p\").class(\"summary\")\n\
             \t})\n\
             }\n",
            "fun demo(): View {\n\
             \tview(\"section\")\n\
             \t\t.class(\"todos\")\n\
             \t\t.child(view(\"h2\").text(\"Todos\"))\n\
             \t\t.when(items.map(|list| count_done(list) > 0), || {\n\
             \t\t\tview(\"p\").class(\"summary\")\n\
             \t\t})\n\
             }\n",
        );
    }

    /// The boundary, with the spanning construct in place: the measured line is
    /// the opening one, `\tsubject.first("…").second(|| {`, which is 33 columns
    /// around the padding at one tab of indent. 67 padding characters make
    /// exactly the budget and stay inline; 68 make 101 and split.
    #[test]
    fn the_opening_line_is_what_is_measured_at_the_boundary() {
        let at_budget = format!(
            "fun demo() {{\n\tsubject.first(\"{}\").second(|| {{\n\t\tbody()\n\t}})\n}}\n",
            "P".repeat(67)
        );
        let over_budget = format!(
            "fun demo() {{\n\tsubject.first(\"{}\").second(|| {{\n\t\tbody()\n\t}})\n}}\n",
            "P".repeat(68)
        );
        // The opening line, indentation included — `columns` already counts the
        // leading tab as its four.
        assert_eq!(columns(at_budget.lines().nth(1).unwrap()), LINE_BUDGET);
        assert_eq!(
            columns(over_budget.lines().nth(1).unwrap()),
            LINE_BUDGET + 1
        );
        assert_construct(&at_budget, &at_budget);
        assert_construct(
            &over_budget,
            &format!(
                "fun demo() {{\n\
                 \tsubject\n\
                 \t\t.first(\"{}\")\n\
                 \t\t.second(|| {{\n\
                 \t\t\tbody()\n\
                 \t\t}})\n\
                 }}\n",
                "P".repeat(68)
            ),
        );
    }

    /// A declaration's own measurement stops at its body. Measuring first lines
    /// made `fun` signatures measurable for the first time — this one is 108
    /// columns — and the permission used to travel past the signature into the
    /// body and break the first statement it found there.
    ///
    /// The signature now SPENDS that permission on its own parameter list
    /// (`proposal/signature-layout.md`), which is what it was always measuring;
    /// what this pin is about is the line below it. `let age = …` is 54 columns
    /// and stays whole either way — under the wide signature, where the split
    /// stops at the `)`, and under a short one, where nothing was armed at all.
    #[test]
    fn an_over_budget_declaration_does_not_arm_its_body() {
        let wide_signature = "fun task_row(client: KoltClient<SocketTransport>, workspace_id: i32, \
                              task: Task, token: SignalCell<str>): View {\n\
                              \tlet age = now().since(task.created_at).describe();\n\
                              \tview(\"li\").styled(row)\n\
                              }\n";
        assert_over_budget(wide_signature.lines().next().unwrap());
        assert_construct(
            wide_signature,
            "fun task_row(\n\
             \tclient: KoltClient<SocketTransport>,\n\
             \tworkspace_id: i32,\n\
             \ttask: Task,\n\
             \ttoken: SignalCell<str>,\n\
             ): View {\n\
             \tlet age = now().since(task.created_at).describe();\n\
             \tview(\"li\").styled(row)\n\
             }\n",
        );

        let narrow_signature = "fun short(a: i32): View {\n\
                                \tlet age = now().since(task.created_at).describe();\n\
                                \tview(\"li\").styled(row)\n\
                                }\n";
        assert_construct(narrow_signature, narrow_signature);
    }

    /// The other half of "first line only": a statement whose opening line fits
    /// does NOT split, however wide the lines below it are. `match` opens with
    /// `let verdict = match outcome {` and is left alone; so is a closure whose
    /// body carries a 104-column line, because that line is the body's own and
    /// has nothing breakable on it anyway.
    #[test]
    fn a_statement_whose_opening_line_fits_is_left_alone() {
        let matched = "fun demo(): i32 {\n\
                       \tlet verdict = match outcome {\n\
                       \t\tSome(let value) => value,\n\
                       \t\tNone => 0,\n\
                       \t};\n\
                       \tverdict\n\
                       }\n";
        assert_construct(matched, matched);

        let long_body = "fun demo() {\n\
                         \tlet handler = || {\n\
                         \t\tlet a_rather_long_line_inside_the_body = compute(alpha, beta, gamma, \
                         delta, epsilon, zeta, eta);\n\
                         \t\ta_rather_long_line_inside_the_body\n\
                         \t};\n\
                         }\n";
        assert_construct(long_body, long_body);
    }
}

#[cfg(test)]
mod import_set_layout {
    //! An import's brace set is a list with braces, and over the budget it
    //! breaks like one: one name per line, one indentation level in, trailing
    //! comma on every one, `}` back at the opening line's indent where the `;`
    //! glues. A set that fits stays inline WITHOUT a trailing comma. The
    //! canonical sort happens first, so a split run is the sorted run.
    //!
    //! The trailing comma is safe on both sides of the toolchain: the language
    //! grammar accepts it, and so does the TOKEN-level import parser behind
    //! Organize Imports (`parse_token_branch`, "comma-separated,
    //! allow-trailing"), which is what lets a split run still sort.
    //!
    //! Both printers go through `print_import_statement`, because
    //! `organize_run` promises byte-for-byte agreement with `fmt` — if only one
    //! split, the editor action and the formatter would rewrite each other on
    //! every save.
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::{assert_over_budget, columns};
    use super::organize::organize;
    use super::{LINE_BUDGET, format};

    /// The motivating line, from `std/src/rpc.vl`: an import at 184 columns.
    #[test]
    fn an_over_budget_import_splits_one_name_per_line() {
        let source = "import std::rpc::{ Dispatcher, LocalTransport, RemoteSource, RpcError, \
                      RpcOutcome, Transport, arg, call, decode_failed };\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "import std::rpc::{\n\
             \tDispatcher,\n\
             \tLocalTransport,\n\
             \tRemoteSource,\n\
             \tRpcError,\n\
             \tRpcOutcome,\n\
             \tTransport,\n\
             \targ,\n\
             \tcall,\n\
             \tdecode_failed,\n\
             };\n",
        );
    }

    /// The other direction, and the trailing comma's rule — the same one the
    /// list and struct literals follow.
    #[test]
    fn an_import_that_fits_stays_inline_without_a_trailing_comma() {
        assert_construct(
            "import std::option::Option::{ self, Some, None };\n",
            "import std::option::Option::{ None, Some, self };\n",
        );
        assert_construct(
            "import std::x::{ alpha, beta, };\n",
            "import std::x::{ alpha, beta };\n",
        );
    }

    /// A one-member set has a canonical unbraced spelling (kolt.local 005):
    /// `a::{ b }` IS `a::b`, so the braces collapse — and recursively, so a
    /// chain of one-member sets reaches the plain path.
    #[test]
    fn a_one_member_set_collapses_to_the_unbraced_spelling() {
        assert_construct("import std::json::Json;\n", "import std::json::Json;\n");
        assert_construct("import std::json::{ Json };\n", "import std::json::Json;\n");
        // A trailing comma changes nothing: it is trivia, not a second member.
        assert_construct(
            "import std::json::{ Json, };\n",
            "import std::json::Json;\n",
        );
        // Recursion, both shapes: a one-member set around a multi-member set
        // collapses to the inner set, and a chain of one-member sets collapses
        // all the way down to the plain path.
        assert_construct(
            "import std::{ json::{ Decode, Encode } };\n",
            "import std::json::{ Decode, Encode };\n",
        );
        assert_construct(
            "import std::{ json::{ Json } };\n",
            "import std::json::Json;\n",
        );
        // `use` shares the canonical form.
        assert_construct(
            "use pkg::shapes::{ Circle };\n",
            "use pkg::shapes::Circle;\n",
        );
        // The run-order interaction `unwrap_singleton_set` exists for: the
        // braced spelling must RANK where its collapsed form does
        // (`BranchKey::Set` orders after `Path`), or the reprint orders the
        // run differently than the net's canonicalization of the source and
        // the whole file silently bails — the corpus's async-promise-all.vl
        // shape, caught by exactly that bail.
        assert_construct(
            "import std::{ io::print };\nimport std::task::Task;\n",
            "import std::io::print;\nimport std::task::Task;\n",
        );
    }

    /// The exception: `self` re-binds the namespace it sits in and only means
    /// that inside braces (`Option::{ self }` publishes `Option`) — there is no
    /// unbraced spelling to collapse to, so a lone `self` keeps its braces.
    #[test]
    fn a_lone_self_keeps_its_braces() {
        assert_construct(
            "import std::option::Option::{ self };\n",
            "import std::option::Option::{ self };\n",
        );
        assert_construct(
            "import std::option::Option::{ self, };\n",
            "import std::option::Option::{ self };\n",
        );
    }

    /// The organize path rides the same printer, so pruning a set down to one
    /// member renders the unbraced spelling too — Organize Imports can never
    /// regenerate the braced form `fmt` collapses.
    #[test]
    fn organize_prunes_a_set_down_to_the_unbraced_spelling() {
        assert_eq!(
            organize("import std::rpc::{ Dispatcher, call };\n", &["call"]),
            "import std::rpc::Dispatcher;\n"
        );
        assert_eq!(
            organize("import std::json::{ Json };\n", &[]),
            format("import std::json::{ Json };\n")
        );
    }

    /// The boundary: `import p::{ …, X };` is 18 columns around the padded name,
    /// so an 82-character name makes exactly the budget and 83 makes 101.
    #[test]
    fn exactly_the_budget_stays_inline_and_one_column_over_splits() {
        let at_budget = format!("import p::{{ {}, X }};\n", "A".repeat(82));
        let over_budget = format!("import p::{{ {}, X }};\n", "A".repeat(83));
        assert_eq!(columns(at_budget.trim_end()), LINE_BUDGET);
        assert_eq!(columns(over_budget.trim_end()), LINE_BUDGET + 1);
        assert_construct(&at_budget, &at_budget);
        assert_construct(
            &over_budget,
            &format!("import p::{{\n\t{},\n\tX,\n}};\n", "A".repeat(83)),
        );
    }

    /// The recursion, one brace level in: the outer set breaks, and a nested set
    /// whose own line still overflows breaks past it — while a nested set that
    /// fits stays inline on its member line.
    #[test]
    fn a_nested_set_breaks_only_when_its_own_line_overflows() {
        assert_construct(
            "import std::outer::{ alpha, beta::{ gamma_name_here, delta_name_here, \
             epsilon_name_here, zeta_name_here }, omega };\n",
            "import std::outer::{\n\
             \talpha,\n\
             \tbeta::{ delta_name_here, epsilon_name_here, gamma_name_here, zeta_name_here },\n\
             \tomega,\n\
             };\n",
        );
        assert_construct(
            "import std::outer::{ alpha, beta::{ gamma_name_here, delta_name_here, \
             epsilon_name_here, zeta_name_here, eta_name_here, theta_name_here }, omega };\n",
            "import std::outer::{\n\
             \talpha,\n\
             \tbeta::{\n\
             \t\tdelta_name_here,\n\
             \t\tepsilon_name_here,\n\
             \t\teta_name_here,\n\
             \t\tgamma_name_here,\n\
             \t\ttheta_name_here,\n\
             \t\tzeta_name_here,\n\
             \t},\n\
             \tomega,\n\
             };\n",
        );
    }

    /// The agreement `organize_run` depends on: Organize Imports renders a split
    /// run identically to `fmt`, so the action and the formatter agree instead of
    /// undoing each other. Pruning a leaf that brings the set back under the
    /// budget collapses it to the inline form, which is the same rule read
    /// backwards.
    #[test]
    fn organize_imports_renders_a_split_run_the_way_fmt_does() {
        let source = "import std::rpc::{ Dispatcher, LocalTransport, RemoteSource, RpcError, \
                      RpcOutcome, Transport, arg, call, decode_failed };\n";
        assert_eq!(organize(source, &[]), format(source));
        assert_eq!(
            organize(source, &[]),
            "import std::rpc::{\n\
             \tDispatcher,\n\
             \tLocalTransport,\n\
             \tRemoteSource,\n\
             \tRpcError,\n\
             \tRpcOutcome,\n\
             \tTransport,\n\
             \targ,\n\
             \tcall,\n\
             \tdecode_failed,\n\
             };\n"
        );
        // Pruning six leaves puts the set back under the budget: inline again.
        assert_eq!(
            organize(
                source,
                &[
                    "LocalTransport",
                    "RemoteSource",
                    "RpcOutcome",
                    "Transport",
                    "arg",
                    "decode_failed"
                ]
            ),
            "import std::rpc::{ Dispatcher, RpcError, call };\n"
        );
    }
}

#[cfg(test)]
mod composite_spanning_layout {
    //! `proposal/composite-spanning-split.md` — backlog 49, and backlog 47's
    //! recorded residue. A list or struct literal whose ELEMENT renders across
    //! lines splits regardless of width, because its closing delimiter always
    //! follows that element: `{ id, notify = || { … } }` inside a call closes on
    //! `} });`, three closings of three different things on one line.
    //!
    //! ANY element, where the chain rule needs a NON-FINAL link. The two differ
    //! because the constructs differ: a chain that ENDS at its spanning link
    //! leaves a clean line and is the trailing-closure idiom, and a composite has
    //! no equivalent position. Both halves of that asymmetry are pinned here.
    use super::bailing_constructs::assert_construct;

    /// The motivating shape, from `std/reactive.vl` — every line inside the
    /// budget, and its author had written it split before the formatter
    /// collapsed it.
    #[test]
    fn a_struct_literal_holding_a_spanning_field_breaks() {
        assert_construct(
            "fun demo() {\n\
             \tpush(Subscriber { id, notify = || {\n\
             \t\tobserver(get());\n\
             \t} });\n\
             }\n",
            "fun demo() {\n\
             \tpush(Subscriber {\n\
             \t\tid,\n\
             \t\tnotify = || {\n\
             \t\t\tobserver(get());\n\
             \t\t},\n\
             \t});\n\
             }\n",
        );
    }

    /// A list literal, same rule.
    #[test]
    fn a_list_holding_a_spanning_element_breaks() {
        assert_construct(
            "fun demo() {\n\
             \thandle([alpha, || {\n\
             \t\twork();\n\
             \t}]);\n\
             }\n",
            "fun demo() {\n\
             \thandle([\n\
             \t\talpha,\n\
             \t\t|| {\n\
             \t\t\twork();\n\
             \t\t},\n\
             \t]);\n\
             }\n",
        );
    }

    /// The asymmetry, stated as a pin rather than left to the prose: the
    /// spanning field above is the literal's LAST, and it still breaks, while a
    /// chain whose spanning link is its last is left alone. Both fixtures here
    /// are within the budget, so only these rules can be acting.
    #[test]
    fn a_chains_last_link_is_still_exempt_where_a_composites_last_element_is_not() {
        let trailing_closure = "fun demo() {\n\
                                \tchain.write().push(|| {\n\
                                \t\titem.dispose();\n\
                                \t});\n\
                                }\n";
        assert_construct(trailing_closure, trailing_closure);
    }

    /// Composites whose elements all fit are untouched — the collapse rules are
    /// unchanged for code that has nothing spanning in it.
    #[test]
    fn a_composite_whose_elements_all_fit_stays_inline() {
        let source = "fun demo() {\n\
                      \tlet plain = Point { x = 1, y = 2 };\n\
                      \tlet list = [alpha, beta];\n\
                      }\n";
        assert_construct(source, source);
    }
}

#[cfg(test)]
mod argument_tail_descent {
    //! `proposal/argument-tail-descent.md` — backlog 43. A statement's split
    //! descends through a call's LAST argument, the way a link's already did, so
    //! a statement whose only breakable construct sits in an argument can reach
    //! it. `list.push(T { … })` is the shape: one call link, so the statement is
    //! not a chain and has nothing to break at its own level.
    //!
    //! Only the last argument. R5 stands — layout hangs off a call's final
    //! argument, so an earlier one that is the over-budget cause still leaves a
    //! long line, and that is pinned here beside the new behavior.
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::assert_over_budget;

    /// The motivating shape, from Kolt's `load_tasks` and `examples/walkthrough`'s
    /// `load_notes` — the same function, wherever a row is read into a record.
    /// 152 columns, and stable at 152 before this.
    #[test]
    fn a_struct_literal_in_a_call_argument_breaks() {
        let source = "fun demo() {\n\
                      \tlist.push(Task { id = row.integer(\"id\"), workspace_id = \
                      row.integer(\"workspace_id\"), name = row.text(\"name\") });\n\
                      }\n";
        assert_over_budget(source.lines().nth(1).unwrap());
        assert_construct(
            source,
            "fun demo() {\n\
             \tlist.push(Task {\n\
             \t\tid = row.integer(\"id\"),\n\
             \t\tworkspace_id = row.integer(\"workspace_id\"),\n\
             \t\tname = row.text(\"name\"),\n\
             \t});\n\
             }\n",
        );
    }

    /// The descent has to survive the `.` of a method call: the callee of
    /// `list.push(…)` IS the member, so a permission dropped at the member
    /// access never reaches the arguments. Pinned separately from the shape
    /// above because widening only the argument printer left this a no-op.
    #[test]
    fn the_descent_reaches_through_a_method_call() {
        let source = "fun demo() {\n\
                      \tregistry.register([alpha_element_one, beta_element_two, \
                      gamma_element_three, delta_element_four, epsilon_five]);\n\
                      }\n";
        assert_over_budget(source.lines().nth(1).unwrap());
        assert_construct(
            source,
            "fun demo() {\n\
             \tregistry.register([\n\
             \t\talpha_element_one,\n\
             \t\tbeta_element_two,\n\
             \t\tgamma_element_three,\n\
             \t\tdelta_element_four,\n\
             \t\tepsilon_five,\n\
             \t]);\n\
             }\n",
        );
    }

    /// R5, unchanged: only the LAST argument is reachable, so an EARLIER
    /// argument that is the over-budget cause still leaves a long line. This is
    /// the boundary backlog 43 deliberately did not move.
    #[test]
    fn an_earlier_argument_is_still_not_reachable() {
        let padding = "P".repeat(72);
        let source = format!("let built = wrap(inner.aa(\"{padding}\").bb(2), tail);\n");
        assert_over_budget(source.trim_end());
        assert_construct(&source, &source);
    }
}

#[cfg(test)]
mod split_comment_attachment {
    //! `proposal/split-comment-attachment.md` — backlog 41. A comment written
    //! inside a splittable construct attaches to the element it precedes, at that
    //! element's indent, and FORCES the construct into its split form: collapsed,
    //! there is no line to keep the comment on, which is how such comments used
    //! to end up flushed below the whole statement, orphaned from what they
    //! explain.
    //!
    //! One mechanism, all five split forms — chains, list literals, struct
    //! literals, import brace sets and parameter lists — because the item asked
    //! for the fix to be written against the split construct generally rather
    //! than the chain specifically.
    //!
    //! The trigger is the GAPS between elements, never an element's own interior:
    //! a comment inside a closure body a link carries belongs to that body and
    //! already prints where it was written.
    use super::bailing_constructs::assert_construct;

    /// A comment before the FIRST element, which no between-elements gap covers —
    /// the case that needs the construct's own opening boundary. Fixture fits the
    /// budget, so only the comment can be splitting it.
    #[test]
    fn a_comment_before_the_first_element_attaches_to_it() {
        let source = "fun demo() {\n\
                      \tlet store = Store {\n\
                      \t\t// the authoritative lists\n\
                      \t\tworkspaces = workspaces,\n\
                      \t\ttasks = tasks,\n\
                      \t};\n\
                      }\n";
        assert_construct(source, source);
    }

    /// A list literal's element comment, same rule.
    #[test]
    fn a_list_element_comment_attaches_to_its_element() {
        let source = "fun demo() {\n\
                      \tlet items = [\n\
                      \t\t// the first one matters\n\
                      \t\talpha,\n\
                      \t\tbeta,\n\
                      \t];\n\
                      }\n";
        assert_construct(source, source);
    }

    /// An import brace set. The set carries no span of its own, so its extent is
    /// recovered from the source (`import_set_extent`); the canonical SORT still
    /// applies, and the comment stays attached to the member it precedes — here
    /// the one that sorts first, so the fixture is already canonical.
    #[test]
    fn an_import_set_comment_attaches_to_its_member() {
        let source = "import std::rpc::{\n\
                      \t// the transport we actually use\n\
                      \tRpcError,\n\
                      \tSocketTransport,\n\
                      };\n";
        assert_construct(source, source);
    }

    /// A `fun` parameter list — the width rule's declaration site takes comment
    /// attachment too, on a signature well inside the budget.
    #[test]
    fn a_parameter_comment_attaches_to_its_parameter() {
        let source = "fun demo(\n\
                      \t// the live client\n\
                      \tclient: Client,\n\
                      \ttoken: SignalCell<str>,\n\
                      ) {\n\
                      \twork()\n\
                      }\n";
        assert_construct(source, source);
    }

    /// The precision of the rule: a comment INSIDE an element — a closure body a
    /// link carries — is not between elements, belongs to that body, and forces
    /// nothing. All three constructs here stay inline.
    #[test]
    fn a_comment_inside_an_element_does_not_force_the_split() {
        let source = "fun main() {\n\
                      \tlet a = one().two(|| {\n\
                      \t\t// inside the closure body\n\
                      \t\twork();\n\
                      \t});\n\
                      \tlet b = Point { x = 1, y = 2 };\n\
                      \tlet c = [alpha, beta];\n\
                      }\n";
        assert_construct(source, source);
    }

    /// Comment-free code is untouched by any of this: the collapse rules still
    /// apply, so a hand-split construct that fits still comes back inline.
    #[test]
    fn comment_free_constructs_still_collapse() {
        assert_construct(
            "fun main() {\n\tlet short = one()\n\t\t.two(2)\n\t\t.three(3);\n}\n",
            "fun main() {\n\tlet short = one().two(2).three(3);\n}\n",
        );
        assert_construct(
            "fun main() {\n\tlet p = Point {\n\t\tx = 1,\n\t\ty = 2,\n\t};\n}\n",
            "fun main() {\n\tlet p = Point { x = 1, y = 2 };\n}\n",
        );
    }
}

#[cfg(test)]
mod chain_seam_layout {
    //! `proposal/chain-seam-split.md` — the width rule's second door. A chain
    //! splits regardless of width when a call link that is NOT its last renders
    //! across lines, because that link's closing `})` lands on a line which then
    //! continues with more chain: the end of one argument, the start of the next
    //! link, and the start of ITS argument, all on one line.
    //!
    //! The last link is excluded deliberately — a chain that ENDS at its
    //! spanning link has no seam, and that is the ordinary trailing-closure
    //! idiom. Counting it too would have touched 8 files and 170 lines across
    //! std and the examples instead of 5 and 121, every std case being a
    //! trailing closure that should stay put.
    //!
    //! Each pin runs the whole formatter contract through `assert_construct`.
    use super::LINE_BUDGET;
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::columns;

    /// The motivating shape, from `examples/fullstack`: every line inside the
    /// budget, and unreadable anyway.
    #[test]
    fn a_seam_splits_a_chain_that_fits() {
        let source = "fun main() {\n\
                      \tlet server = Server::builder().port(3000).on_request(|request| {\n\
                      \t\troute(request);\n\
                      \t}).on_start(|server| {\n\
                      \t\tprint(server.url());\n\
                      \t}).build();\n\
                      }\n";
        for line in source.lines() {
            assert!(
                columns(line) <= LINE_BUDGET,
                "fixture must be within the budget, so the pin is about the seam \
                 and not about width: {line:?}"
            );
        }
        assert_construct(
            source,
            "fun main() {\n\
             \tlet server = Server::builder()\n\
             \t\t.port(3000)\n\
             \t\t.on_request(|request| {\n\
             \t\t\troute(request);\n\
             \t\t})\n\
             \t\t.on_start(|server| {\n\
             \t\t\tprint(server.url());\n\
             \t\t})\n\
             \t\t.build();\n\
             }\n",
        );
    }

    /// The exclusion, and the whole reason the rule says "not the last": the
    /// trailing-closure idiom stays exactly as written. `std`'s `Owner::take` is
    /// this shape, and breaking it buys two lines and no clarity.
    #[test]
    fn a_spanning_last_link_is_left_alone() {
        let source = "fun take<T: Disposable>(self, item: T): T {\n\
                      \tself.cleanups.write().push(|| {\n\
                      \t\titem.dispose();\n\
                      \t});\n\
                      \titem\n\
                      }\n";
        assert_construct(source, source);
    }

    /// One call link is not a chain, so there is no shape to show and nothing to
    /// break — the seam rule inherits the split form's own entry condition.
    #[test]
    fn a_single_link_is_not_a_chain() {
        let source = "fun main() {\n\
                      \tregistry.on(\"alpha\", |event| {\n\
                      \t\thandle(event);\n\
                      \t});\n\
                      }\n";
        assert_construct(source, source);
    }

    /// A triple-quoted string spans lines because of its CONTENTS, not its
    /// structure. An earlier draft of this rule triggered on "the statement's
    /// rendering spans lines" and broke this at the `+`; restricting it to chain
    /// links is what excludes it.
    #[test]
    fn a_multiline_string_is_not_a_seam() {
        let source = "fun main() {\n\
                      \tlet line = \"\"\"\n\
                      \t\tkey: value\n\
                      \t\t\"\"\" + \"!\";\n\
                      \tprint(line);\n\
                      }\n";
        assert_construct(source, source);
    }

    /// The rule composes with width rather than replacing it. The statement's
    /// OPENING line is 41 columns, so width cannot arm the split and the seam is
    /// the only thing that can — and once split, the `.wrap(…)` link's own line
    /// is over the budget, so it recurses into its last argument exactly as a
    /// width-split link does. Both doors, one statement.
    #[test]
    fn a_seam_split_link_still_recurses_on_width() {
        let padding = "P".repeat(70);
        let source = format!(
            "fun main() {{\n\
             \tlet built = subject.on(\"x\", |event| {{\n\
             \t\thandle(event);\n\
             \t}}).wrap(\"{padding}\", inner.aa(1).bb(2));\n\
             }}\n"
        );
        // The measured line — the first — fits, so this pin cannot pass by width.
        assert_eq!(columns(source.lines().nth(1).unwrap()), 41);
        assert_construct(
            &source,
            &format!(
                "fun main() {{\n\
                 \tlet built = subject\n\
                 \t\t.on(\"x\", |event| {{\n\
                 \t\t\thandle(event);\n\
                 \t\t}})\n\
                 \t\t.wrap(\"{padding}\", inner\n\
                 \t\t\t.aa(1)\n\
                 \t\t\t.bb(2));\n\
                 }}\n"
            ),
        );
    }
}

#[cfg(test)]
mod signature_layout {
    //! `proposal/signature-layout.md` — the width rule's first DECLARATION site.
    //! A `fun` signature over the budget breaks its parameter list one parameter
    //! per line, one level in, trailing comma on every one, `)` back at the
    //! declaration's indent with the return type, `borrows` clause and the body's
    //! `{` (or a bodyless `;`) glued after it. The list rule, on the one
    //! bracketed list that is not an expression.
    //!
    //! Two things deliberately do NOT break: a closure's parameters, which are an
    //! expression's own punctuation, and a call's ARGUMENT list, which stays
    //! last-argument-driven (R5 / backlog 43). The proposal states that asymmetry
    //! and why it is intended rather than an oversight.
    //!
    //! Each pin runs the whole formatter contract through `assert_construct`.
    use super::LINE_BUDGET;
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::{assert_over_budget, columns};

    /// The motivating signature — `serve_connected` as it stood in
    /// `std/src/process/rpc_server.vl` (since retired, E71): 172 columns of
    /// closure-typed parameters, wide by construction.
    #[test]
    fn an_over_budget_signature_splits_one_parameter_per_line() {
        let source = "fun serve_connected(port: i32, protocol: RpcProtocol, \
                      on_connect: |i32, DuplexEnd| void, on_disconnect: |i32| void, \
                      fallback: |Request| Response, on_ready: |Server| void) {\n\
                      \tbody()\n\
                      }\n";
        assert_over_budget(source.lines().next().unwrap());
        assert_construct(
            source,
            "fun serve_connected(\n\
             \tport: i32,\n\
             \tprotocol: RpcProtocol,\n\
             \ton_connect: |i32, DuplexEnd| void,\n\
             \ton_disconnect: |i32| void,\n\
             \tfallback: |Request| Response,\n\
             \ton_ready: |Server| void,\n\
             ) {\n\
             \tbody()\n\
             }\n",
        );
    }

    /// A signature that fits stays inline WITHOUT a trailing comma, and a
    /// hand-written one is dropped — the comma marks a split list here too.
    #[test]
    fn a_signature_that_fits_stays_inline_without_a_trailing_comma() {
        assert_construct(
            "fun f(a: i32, b: i32) {\n\tbody()\n}\n",
            "fun f(a: i32, b: i32) {\n\tbody()\n}\n",
        );
        assert_construct(
            "fun f(a: i32, b: i32,) {\n\tbody()\n}\n",
            "fun f(a: i32, b: i32) {\n\tbody()\n}\n",
        );
    }

    /// The boundary: `fun f(…: i32) {` is 14 columns around the parameter name,
    /// so an 86-character name makes exactly the budget and 87 makes 101.
    #[test]
    fn exactly_the_budget_stays_inline_and_one_column_over_splits() {
        let at_budget = format!("fun f({}: i32) {{\n\tbody()\n}}\n", "a".repeat(86));
        let over_budget = format!("fun f({}: i32) {{\n\tbody()\n}}\n", "a".repeat(87));
        assert_eq!(columns(at_budget.lines().next().unwrap()), LINE_BUDGET);
        assert_eq!(
            columns(over_budget.lines().next().unwrap()),
            LINE_BUDGET + 1
        );
        assert_construct(&at_budget, &at_budget);
        assert_construct(
            &over_budget,
            &format!("fun f(\n\t{}: i32,\n) {{\n\tbody()\n}}\n", "a".repeat(87)),
        );
    }

    /// What follows the `)` rides the closing line, whatever it is: a return
    /// type, or a bodyless declaration's `;`. Neither is a list entry.
    #[test]
    fn the_return_type_and_a_bodyless_semicolon_ride_the_closing_line() {
        assert_construct(
            "fun with_ret(alpha_parameter: i32, beta_parameter: str, gamma_parameter: bool, \
             delta_parameter: i32, eps: i32): Result<List<i32>, RpcError> {\n\tbuild()\n}\n",
            "fun with_ret(\n\
             \talpha_parameter: i32,\n\
             \tbeta_parameter: str,\n\
             \tgamma_parameter: bool,\n\
             \tdelta_parameter: i32,\n\
             \teps: i32,\n\
             ): Result<List<i32>, RpcError> {\n\
             \tbuild()\n\
             }\n",
        );
        assert_construct(
            "external fun pbkdf2_sync(password: str, salt: str, iterations: i32, \
             key_length: i32, digest: str, extra: str): HashBuffer;\n",
            "external fun pbkdf2_sync(\n\
             \tpassword: str,\n\
             \tsalt: str,\n\
             \titerations: i32,\n\
             \tkey_length: i32,\n\
             \tdigest: str,\n\
             \textra: str,\n\
             ): HashBuffer;\n",
        );
    }

    /// An EMPTY parameter list never breaks — `(⏎)` buys a line and no clarity —
    /// so a signature pushed over by its NAME simply stays long. Same rule the
    /// empty list and the empty struct literal follow.
    #[test]
    fn an_empty_parameter_list_never_breaks() {
        let source = "fun a_function_whose_name_alone_runs_well_past_the_hundred_column_budget\
                      _without_any_parameters(): i32 {\n\tbody()\n}\n";
        assert_over_budget(source.lines().next().unwrap());
        assert_construct(source, source);
    }

    /// A CLOSURE's parameters are untouched: they are an expression's own
    /// punctuation, printed through `print_closure_parameters`, and only
    /// `print_parameters` — reached solely from a `fun` declaration — splits.
    #[test]
    fn closure_parameters_do_not_break() {
        let source = "fun demo() {\n\
                      \tlet handler = |alpha_parameter: i32, beta_parameter: str, \
                      gamma_parameter: bool, delta: i32| compute(alpha_parameter);\n\
                      }\n";
        assert_over_budget(source.lines().nth(1).unwrap());
        assert_construct(source, source);
    }
}

#[cfg(test)]
mod element_layout {
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::assert_over_budget;

    #[test]
    fn a_self_closing_element_spaces_before_the_slash() {
        assert_construct(
            "fun demo(): View {\n\t<div/>\n}\n",
            "fun demo(): View {\n\t<div />\n}\n",
        );
    }

    #[test]
    fn a_single_string_child_stays_inline() {
        assert_construct(
            "fun demo(): View {\n\t<h2>\"Todos\"</h2>\n}\n",
            "fun demo(): View {\n\t<h2>\"Todos\"</h2>\n}\n",
        );
    }

    #[test]
    fn a_single_hole_child_stays_inline_and_keeps_its_braces() {
        // `{\"x\"}` and `\"x\"` differ in tokens, so the braces are structural
        // (ElementChild::Hole), never inferred from the text.
        assert_construct(
            "fun demo(): View {\n\t<li class(\"item\")>{ label }</li>\n}\n",
            "fun demo(): View {\n\t<li class(\"item\")>{label}</li>\n}\n",
        );
    }

    #[test]
    fn an_element_child_splits_one_per_line() {
        assert_construct(
            "fun demo(): View {\n\t<div><span/></div>\n}\n",
            "fun demo(): View {\n\t<div>\n\t\t<span />\n\t</div>\n}\n",
        );
    }

    #[test]
    fn mixed_content_splits_one_child_per_line() {
        assert_construct(
            "fun demo(): View {\n\t<p .styled(lead)>\"Take \" <code>\"vilan upgrade\"</code> \".\"</p>\n}\n",
            "fun demo(): View {\n\t<p .styled(lead)>\n\t\t\"Take \"\n\t\t<code>\"vilan upgrade\"</code>\n\t\t\".\"\n\t</p>\n}\n",
        );
    }

    #[test]
    fn an_over_budget_head_splits_one_item_per_line() {
        let source = "fun demo(): View {\n\t<input placeholder(\"What needs doing?\") disabled aria-label(\"A long label here to push the head far past the hundred column budget\") />\n}\n";
        assert_over_budget(source.lines().nth(1).unwrap());
        assert_construct(
            source,
            "fun demo(): View {\n\t<input\n\t\tplaceholder(\"What needs doing?\")\n\t\tdisabled\n\t\taria-label(\"A long label here to push the head far past the hundred column budget\")\n\t/>\n}\n",
        );
    }

    #[test]
    fn an_open_empty_element_keeps_its_form() {
        // `<div></div>` and `<div />` differ in TOKENS — the re-lex net forbids
        // converting one to the other; each form round-trips as written.
        assert_construct(
            "fun demo(): View {\n\t<div></div>\n}\n",
            "fun demo(): View {\n\t<div></div>\n}\n",
        );
    }

    #[test]
    fn a_comment_inside_an_element_attaches_to_the_child_it_precedes() {
        // `proposal/split-comment-attachment.md`, extended to markup: the
        // comment stays on its own line above the child it explains.
        assert_construct(
            "fun demo(): View {\n\t<div>\n\t\t// a note\n\t\t<span/>\n\t</div>\n}\n",
            "fun demo(): View {\n\t<div>\n\t\t// a note\n\t\t<span />\n\t</div>\n}\n",
        );
    }

    #[test]
    fn a_comment_keeps_an_inlineable_element_split() {
        // Collapsed, `<h2>"Todos"</h2>` has no line for the comment — the
        // comment forces the split and rides above the child.
        assert_construct(
            "fun demo(): View {\n\t<h2>\n\t\t// heading note\n\t\t\"Todos\"\n\t</h2>\n}\n",
            "fun demo(): View {\n\t<h2>\n\t\t// heading note\n\t\t\"Todos\"\n\t</h2>\n}\n",
        );
    }

    #[test]
    fn a_comment_between_head_items_splits_the_head() {
        assert_construct(
            "fun demo(): View {\n\t<input placeholder(\"x\")\n\t\t// boolean\n\t\tdisabled />\n}\n",
            "fun demo(): View {\n\t<input\n\t\tplaceholder(\"x\")\n\t\t// boolean\n\t\tdisabled\n\t/>\n}\n",
        );
    }

    #[test]
    fn a_comment_after_the_last_child_relocates_below() {
        // List parity: attachment covers the gaps BETWEEN items; a trailing
        // comment has no following item and falls out below the statement.
        assert_construct(
            "fun demo(): View {\n\t<div>\n\t\t<span/>\n\t\t// trailing\n\t</div>\n}\n",
            "fun demo(): View {\n\t<div>\n\t\t<span />\n\t</div>\n\t// trailing\n}\n",
        );
    }

    #[test]
    fn an_element_subject_chain_glues_links_after_the_closing_tag() {
        // A multi-line element as a chain subject keeps its own layout; links
        // within budget continue on the closing tag's line.
        assert_construct(
            "fun demo(): View {\n\t<div><span/></div>.show(f).hide(g)\n}\n",
            "fun demo(): View {\n\t<div>\n\t\t<span />\n\t</div>.show(f).hide(g)\n}\n",
        );
    }

    #[test]
    fn a_chain_link_holding_an_inline_element_stays_inline() {
        assert_construct(
            "fun demo(): View {\n\t<ul .bind_each(items, |t| t.id, |t| <li>{t}</li>) />\n}\n",
            "fun demo(): View {\n\t<ul .bind_each(items, |t| t.id, |t| <li>{t}</li>) />\n}\n",
        );
    }

    // --- E118: a closure-argument element that SPLITS ------------------------

    /// The owner's exhibit, verbatim. Inline, the element's three anchors
    /// disagree: the open tag starts after `|| `, the children indent from the
    /// STATEMENT, and the close tag sits at the statement's column. Breaking
    /// after `|| ` puts all three on one column — the block body's rule.
    #[test]
    fn a_closure_argument_element_breaks_after_the_bar() {
        assert_construct(
            "fun demo() {\n\toverlays.attach(submenu, || <div .styled(example_padded_style)>\n\
             \t\t<button>\"Sub item\"</button>\n\t</div>);\n}\n",
            "fun demo() {\n\toverlays.attach(submenu, ||\n\
             \t\t<div .styled(example_padded_style)>\n\
             \t\t\t<button>\"Sub item\"</button>\n\t\t</div>);\n}\n",
        );
    }

    /// One child, and it is an element — the shape that forces a split without
    /// any help from the line budget.
    #[test]
    fn a_one_child_closure_argument_element_breaks_after_the_bar() {
        assert_construct(
            "fun demo() {\n\tattach(|| <div><span>\"x\"</span></div>);\n}\n",
            "fun demo() {\n\tattach(||\n\t\t<div>\n\t\t\t<span>\"x\"</span>\n\
             \t\t</div>);\n}\n",
        );
    }

    /// Nested elements under one closure argument: every level indents from the
    /// level above it, and each closing tag lands on its own opening tag's
    /// column — the property the inline form could not have, since only the
    /// outermost close had a column of its own.
    #[test]
    fn nested_children_indent_from_their_own_open_tag() {
        assert_construct(
            "fun demo() {\n\tattach(|| <div .styled(s)><section><span>\"x\"</span>\
             </section></div>);\n}\n",
            "fun demo() {\n\tattach(||\n\t\t<div .styled(s)>\n\t\t\t<section>\n\
             \t\t\t\t<span>\"x\"</span>\n\t\t\t</section>\n\t\t</div>);\n}\n",
        );
    }

    /// A closure argument nested INSIDE an element head — the `bind_each` shape
    /// the inline pin above uses, with a body that splits. The rule applies at
    /// that depth too, measured from the head-item line the closure sits on.
    #[test]
    fn a_closure_argument_element_inside_an_element_head_breaks_too() {
        assert_construct(
            "fun demo(): View {\n\t<ul .bind_each(items, |t| t.id, |t| \
             <li><b>{t}</b></li>) />\n}\n",
            "fun demo(): View {\n\t<ul\n\t\t.bind_each(items, |t| t.id, |t|\n\
             \t\t\t<li>\n\t\t\t\t<b>{t}</b>\n\t\t\t</li>)\n\t/>\n}\n",
        );
    }
}

#[cfg(test)]
mod binary_operand_layout {
    //! Both operands of a binary are layout sites, and the LEFT wins. The left
    //! prints first; if it broke, the operator and the right operand take a
    //! fresh continuation line. The right operand is then measured on whatever
    //! line it landed on — the statement's own when the left stayed inline
    //! (`const (art_blob + style()` ⏎ `.raw(…)`, the website's dominant shape),
    //! that continuation line when the left broke — and rolls back into a split
    //! of its own when that line is over budget, its links one level past the
    //! line it is on.
    //!
    //! The left-operand rule itself is v1's and is pinned in
    //! [`super::chain_splitting`]:
    //! `a_chain_operand_of_a_binary_puts_the_continuation_on_its_own_line` (the
    //! bare form) and `a_parenthesized_chain_operand_keeps_its_parentheses` (the
    //! recorded-group form). Neither changes here.
    //!
    //! Entry is width only, in both directions, at either arming. Each pin runs
    //! the whole formatter contract through `assert_construct` (same tokens out,
    //! no silent bail, canonical form in one pass, idempotent).
    use super::bailing_constructs::assert_construct;
    use super::chain_splitting::{assert_over_budget, columns};
    use super::{LINE_BUDGET, format};

    // --- B1: the right operand is the breakable chain ------------------------

    /// The motivating line, from the website's `art.vl` — 19 of that module's
    /// 21 over-long lines are this shape. Everything through the operator and
    /// the chain's subject stays on the statement's line, the links break one
    /// level in, and the group's `)` and the terminator glue after the last one.
    #[test]
    fn a_chain_as_the_right_operand_splits_after_the_operator() {
        let source = "let dg_blob_top = const (art_blob + style().raw(\"left\", \"30%\")\
                      .raw(\"top\", \"-14%\").raw(\"width\", \"42%\").raw(\"height\", \"55%\")\
                      .raw(\"background\", \"radial-gradient(closest-side, \
                      rgba(178, 48, 86, 0.5), transparent)\"));\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let dg_blob_top = const (art_blob + style()\n\
             \t.raw(\"left\", \"30%\")\n\
             \t.raw(\"top\", \"-14%\")\n\
             \t.raw(\"width\", \"42%\")\n\
             \t.raw(\"height\", \"55%\")\n\
             \t.raw(\"background\", \"radial-gradient(closest-side, \
             rgba(178, 48, 86, 0.5), transparent)\"));\n",
        );
    }

    /// A right operand that is not a *chain* is not a layout site: one call link
    /// buys a line and no clarity, so the long line stays long. Nothing about
    /// the arming is conditional on what it will find — the reprint simply
    /// reproduces the inline bytes.
    #[test]
    fn a_single_link_right_operand_leaves_the_line_long() {
        let statement = format!("let one = base + s.aa(\"{}\");\n", "P".repeat(90));
        assert_over_budget(statement.trim_end());
        assert_construct(&statement, &statement);
    }

    // --- B3: width only, both directions -------------------------------------

    /// The collapse direction: the same shape under the budget joins back onto
    /// one line, so a hand-split narrow chain is not preserved by accident.
    #[test]
    fn a_right_operand_chain_that_fits_collapses() {
        let collapsed = "let small = const (art_blob + style().raw(\"left\", \"30%\")\
                         .raw(\"top\", \"-14%\"));\n";
        assert!(columns(collapsed.trim_end()) <= LINE_BUDGET);
        assert_construct(
            "let small = const (art_blob + style()\n\
             \t.raw(\"left\", \"30%\")\n\
             \t.raw(\"top\", \"-14%\"));\n",
            collapsed,
        );
    }

    /// The boundary at this position, arithmetically. What is measured is the
    /// line *through the right operand*: `let v = base + s.aa("…").bb(2)` is 29
    /// columns around the padding, so 71 padding characters make exactly the
    /// budget and 72 make 101. The `;` the statement glues after the operand is
    /// the statement's, not the operand's, and is deliberately not measured
    /// (the same rule a chain link's line follows) — which is why the 101-column
    /// statement here arms a split and then declines it, leaving its line as
    /// written.
    #[test]
    fn a_right_operand_at_the_budget_stays_inline_and_one_column_over_splits() {
        let statement = |padding: &str| format!("let v = base + s.aa(\"{padding}\").bb(2);\n");
        let at_budget = "P".repeat(71);
        let over_budget = "P".repeat(72);
        assert_eq!(
            columns(&format!("let v = base + s.aa(\"{at_budget}\").bb(2)")),
            LINE_BUDGET
        );
        assert_eq!(
            columns(&format!("let v = base + s.aa(\"{over_budget}\").bb(2)")),
            LINE_BUDGET + 1
        );
        // Both statements are over the budget as statements, so the split is
        // armed in both: the difference is the operand's own measurement.
        assert_over_budget(statement(&at_budget).trim_end());
        assert_construct(&statement(&at_budget), &statement(&at_budget));
        assert_construct(
            &statement(&over_budget),
            &format!("let v = base + s\n\t.aa(\"{over_budget}\")\n\t.bb(2);\n"),
        );
    }

    // --- B2: the left operand still wins -------------------------------------

    /// With a breakable chain on both sides the LEFT splits, and the right
    /// prints on the continuation line exactly as v1 printed `+ reveal` — the
    /// right operand's own rule only asks whether *that* line is over budget,
    /// and here it is not.
    #[test]
    fn a_left_operand_chain_wins_and_a_fitting_right_stays_inline() {
        let source = "let both = const (style().raw(\"font-family\", display_face)\
                      .font_size(Length::px(32.0)).margin(space(0)) + style().color(ink)\
                      .gap(space(4)));\n";
        assert_over_budget(source.trim_end());
        assert!(columns("\t+ style().gap(space(4)).color(ink));") <= LINE_BUDGET);
        assert_construct(
            source,
            "let both = const (style()\n\
             \t.raw(\"font-family\", display_face)\n\
             \t.margin(space(0))\n\
             \t.font_size(Length::px(32.0))\n\
             \t+ style().gap(space(4)).color(ink));\n",
        );
    }

    /// …and when that continuation line is itself over budget, the right chain
    /// splits from there — its links one level past the continuation, which is
    /// the same measured-line rule a chain link's own line follows. This is
    /// `std::json`'s `struct_json_impls` shape, the one std statement the rule
    /// reached.
    #[test]
    fn an_over_budget_continuation_line_splits_the_right_chain_one_level_past_it() {
        assert_over_budget(
            "\t\t+ impl_of(target.name).implements(\"FromJson\").method(from_json)\
             .method(from_json_value).render()",
        );
        assert_construct(
            "fun impls(): str {\n\
             \timpl_of(target.name).implements(\"Json\").method(to_json).render() \
             + impl_of(target.name).implements(\"FromJson\").method(from_json)\
             .method(from_json_value).render()\n\
             }\n",
            "fun impls(): str {\n\
             \timpl_of(target.name)\n\
             \t\t.implements(\"Json\")\n\
             \t\t.method(to_json)\n\
             \t\t.render()\n\
             \t\t+ impl_of(target.name)\n\
             \t\t\t.implements(\"FromJson\")\n\
             \t\t\t.method(from_json)\n\
             \t\t\t.method(from_json_value)\n\
             \t\t\t.render()\n\
             }\n",
        );
    }

    // --- Both armings: a measured line inside a split reaches it too ---------

    /// The rule is the per-line one, so it fires wherever a line is measured —
    /// not only on a statement's own. Here the statement's chain splits, one
    /// link's line is still over the budget, and the descent through that
    /// call's last argument finds a sum whose right operand is the chain: it
    /// breaks one level past the link, with the call's `)` glued after.
    #[test]
    fn a_link_line_reaches_the_right_operand_of_its_argument() {
        let source = "let built = subject.first(1).wrap(base_value + style().raw(\"left\", \"30%\")\
                      .raw(\"top\", \"-14%\").raw(\"width\", \"42%\").raw(\"height\", \"55%\"))\
                      .tail(3);\n";
        assert_over_budget(
            "\t.wrap(base_value + style().raw(\"left\", \"30%\").raw(\"top\", \"-14%\")\
             .raw(\"width\", \"42%\").raw(\"height\", \"55%\"))",
        );
        assert_construct(
            source,
            "let built = subject\n\
             \t.first(1)\n\
             \t.wrap(base_value + style()\n\
             \t\t.raw(\"left\", \"30%\")\n\
             \t\t.raw(\"top\", \"-14%\")\n\
             \t\t.raw(\"width\", \"42%\")\n\
             \t\t.raw(\"height\", \"55%\"))\n\
             \t.tail(3);\n",
        );
    }

    /// The other measured line inside a split: a list element's. An element
    /// that is a sum with a breakable right operand splits the same way, its
    /// links one level past the element and the list's comma glued after the
    /// last of them.
    #[test]
    fn a_list_element_reaches_the_right_operand_of_its_sum() {
        let source = "let listed = subject.first(1).wrap(panel([short_one(), base_value \
                      + style().raw(\"left\", \"30%\").raw(\"top\", \"-14%\").raw(\"width\", \"42%\")\
                      .raw(\"height\", \"55%\")])).tail(3);\n";
        assert_over_budget(
            "\t\tbase_value + style().raw(\"left\", \"30%\").raw(\"top\", \"-14%\")\
             .raw(\"width\", \"42%\").raw(\"height\", \"55%\"),",
        );
        assert_construct(
            source,
            "let listed = subject\n\
             \t.first(1)\n\
             \t.wrap(panel([\n\
             \t\tshort_one(),\n\
             \t\tbase_value + style()\n\
             \t\t\t.raw(\"left\", \"30%\")\n\
             \t\t\t.raw(\"top\", \"-14%\")\n\
             \t\t\t.raw(\"width\", \"42%\")\n\
             \t\t\t.raw(\"height\", \"55%\"),\n\
             \t]))\n\
             \t.tail(3);\n",
        );
    }

    // --- B4: a chained binary ------------------------------------------------

    /// `a + b + c` parses left-nested, so the rule fires at the OUTERMOST binary
    /// — the one whose right operand is the breakable chain. The inner sum is
    /// the left operand there, it has nothing to break, and its own right
    /// operand's line (`let chained = base + mid`) fits: everything through the
    /// last operator stays on the statement's line and the chain breaks after it.
    #[test]
    fn a_chained_binary_splits_at_the_outermost_right_operand() {
        let source = "let chained = base + mid + style().raw(\"left\", \"30%\")\
                      .raw(\"top\", \"-14%\").raw(\"width\", \"42%\").raw(\"height\", \"55%\");\n";
        assert_over_budget(source.trim_end());
        assert_construct(
            source,
            "let chained = base + mid + style()\n\
             \t.raw(\"left\", \"30%\")\n\
             \t.raw(\"top\", \"-14%\")\n\
             \t.raw(\"width\", \"42%\")\n\
             \t.raw(\"height\", \"55%\");\n",
        );
    }

    // --- The shape as a whole is a fixed point -------------------------------

    /// A file mixing the right-operand shape with v1's left-operand one is a
    /// fixed point: each keeps its own layout and neither drifts into the
    /// other's on a second pass.
    #[test]
    fn a_file_mixing_a_right_split_and_a_left_split_is_a_fixed_point() {
        let source = "let heading = const style().raw(\"font-family\", display_face)\
                      .font_size(Length::px(32.0)).raw(\"line-height\", \"48px\")\
                      .font_weight(600).margin(space(0)) + reveal;\n\
                      \n\
                      let dg_blob_top = const (art_blob + style().raw(\"left\", \"30%\")\
                      .raw(\"top\", \"-14%\").raw(\"width\", \"42%\").raw(\"height\", \"55%\")\
                      .raw(\"background\", \"radial-gradient(closest-side, \
                      rgba(178, 48, 86, 0.5), transparent)\"));\n\
                      \n\
                      let small = const (art_blob + style().raw(\"left\", \"30%\")\
                      .raw(\"top\", \"-14%\"));\n";
        let once = format(source);
        assert_eq!(format(&once), once, "formatting is not a fixed point");
        assert!(
            once.contains("let heading = const style()\n\t.raw(\"font-family\", display_face)"),
            "the left-operand chain did not split:\n{once}"
        );
        assert!(
            once.contains("\t.font_weight(600)\n\t+ reveal;\n"),
            "the left-operand continuation moved:\n{once}"
        );
        assert!(
            once.contains("let dg_blob_top = const (art_blob + style()\n\t.raw(\"left\", \"30%\")"),
            "the right-operand chain did not split:\n{once}"
        );
        assert!(
            once.contains(
                "let small = const (art_blob + style().raw(\"left\", \"30%\")\
                 .raw(\"top\", \"-14%\"));\n"
            ),
            "the narrow sum did not stay inline:\n{once}"
        );
    }
}

#[cfg(test)]
mod import_sorting {
    //! `vilan fmt` reorders a file's top-level `import`/`use` statements into the
    //! canonical order (see the canonical-import-order section): kind (`import`
    //! before `use`), then root namespace (`std`, dependencies, `pkg`), then the
    //! full path, with brace sets sorted internally. Runs coalesce across blank
    //! lines and break at standalone comments; a trailing comment travels with
    //! its import; block-scoped imports are left as written.
    use super::{format, normalize};
    use crate::lexing::tokenize;
    use crate::token::Token;

    /// The reprint carries the canonical order, is idempotent, and did not
    /// silently bail (a bail returns the input verbatim, so appended blank
    /// lines — pure trivia — would survive instead of canonicalizing).
    fn assert_sorts(source: &str, expected: &str) {
        assert_eq!(format(source), expected, "unexpected canonical order");
        assert_eq!(format(expected), expected, "not idempotent");
        assert_eq!(
            format(&format!("{source}\n\n")),
            expected,
            "silently bailed on {source:?}"
        );
    }

    /// The lexer's tokens with spans stripped — a raw stream to feed `normalize`
    /// directly (unlike the reprint path, this does not sort anything itself).
    fn raw_tokens(text: &str) -> Vec<Token<'_>> {
        let (tokens, errors) = tokenize(text);
        assert!(errors.is_empty(), "did not lex cleanly: {text:?}");
        tokens.into_iter().map(|(token, _)| token).collect()
    }

    // A run mixing every root and both kinds sorts to canonical order: imports
    // before uses, then `std` < dependency (`acme`) < `pkg`, then path.
    #[test]
    fn shuffled_std_dependency_pkg_run_sorts_canonically() {
        assert_sorts(
            "import pkg::z::thing;\nimport std::io::print;\nimport acme::widget;\n\
             use std::option::Option;\nimport std::alpha;\nuse acme::helper;\n",
            "import std::alpha;\nimport std::io::print;\nimport acme::widget;\n\
             import pkg::z::thing;\nuse std::option::Option;\nuse acme::helper;\n",
        );
    }

    // A brace set's inner branch list sorts alphabetically (case-sensitive), the
    // path is otherwise unchanged.
    #[test]
    fn branch_set_inner_list_sorts() {
        assert_sorts(
            "import std::x::{ delta, beta, alpha };\n",
            "import std::x::{ alpha, beta, delta };\n",
        );
        // Case-sensitive: capitalized names sort before lowercase (ASCII).
        assert_sorts(
            "import std::option::Option::{ self, Some, None };\n",
            "import std::option::Option::{ None, Some, self };\n",
        );
    }

    // A `use` always sorts after every `import`, whatever the paths — the kind
    // is the primary key.
    #[test]
    fn use_sorts_after_import() {
        assert_sorts(
            "use std::a;\nimport std::z;\n",
            "import std::z;\nuse std::a;\n",
        );
    }

    // The `export` re-export prefix does not change grouping: an `export import`
    // sorts as a plain import (by root and path), keeping its prefix. A
    // `std`-rooted plain import still precedes `pkg` re-exports.
    #[test]
    fn export_reexports_sort_by_the_same_key() {
        assert_sorts(
            "export import pkg::string::str;\nexport import pkg::io::print;\n\
             import std::option::Option;\nexport import pkg::number::{ u32, BigInt, i8 };\n",
            "import std::option::Option;\nexport import pkg::io::print;\n\
             export import pkg::number::{ BigInt, i8, u32 };\nexport import pkg::string::str;\n",
        );
    }

    // A block-scoped import (inside a `fn` body — backlog H2) is a deliberate
    // placement: neither its run order nor its brace set is touched, even when
    // both are non-canonical. Byte-identical output, and the net did not trip.
    #[test]
    fn block_scoped_imports_are_untouched() {
        let source = "fun f() {\n\tuse zeta::last;\n\timport std::b::{ y, x };\n\
                      \timport std::a;\n\tb::go();\n}\n";
        assert_eq!(
            format(source),
            source,
            "block-scoped imports were reordered"
        );
    }

    // A standalone (own-line) comment pins the run: imports do not reorder
    // across it, so the two sides sort independently and the comment stays put.
    // A blank line, by contrast, coalesces (the run reprints as one block).
    #[test]
    fn standalone_comment_pins_the_run_blank_line_coalesces() {
        assert_sorts(
            "import std::b;\n\nimport std::c;\n// pin\nimport std::z;\nimport std::m;\n",
            "import std::b;\nimport std::c;\n// pin\nimport std::m;\nimport std::z;\n",
        );
    }

    // A trailing same-line comment travels with its import to the new position.
    #[test]
    fn trailing_comment_travels_with_its_import() {
        assert_sorts(
            "import std::c; // the c note\nimport std::a;\n",
            "import std::a;\nimport std::c; // the c note\n",
        );
    }

    // Formatting an already-canonical run changes nothing (no spurious churn) —
    // the property the whole corpus fallout depends on.
    #[test]
    fn already_canonical_run_is_a_fixed_point() {
        let canonical = "import std::a;\nimport std::b;\nuse dep::x;\n\nfun main() {}\n";
        assert_eq!(format(canonical), canonical);
    }

    // The safety net forgives import-run order (and brace-set order) — on BOTH
    // sides — so the printer's reorder passes. This is what makes the reprint
    // land instead of bailing.
    #[test]
    fn normalize_forgives_import_run_and_branch_order() {
        assert_eq!(
            normalize(raw_tokens("import std::b;\nimport std::a;\n")),
            normalize(raw_tokens("import std::a;\nimport std::b;\n")),
            "normalize must canonicalize top-level import-run order"
        );
        assert_eq!(
            normalize(raw_tokens("import std::x::{ b, a };\n")),
            normalize(raw_tokens("import std::x::{ a, b };\n")),
            "normalize must canonicalize brace-set order"
        );
    }

    // ...but nothing else. Swapping two top-level functions stays a detectable
    // difference — the net did not go order-insensitive beyond import runs, so
    // it still catches a genuine reprint reordering bug.
    #[test]
    fn net_still_catches_a_non_import_reordering() {
        assert_ne!(
            normalize(raw_tokens("fun a() {}\nfun b() {}\n")),
            normalize(raw_tokens("fun b() {}\nfun a() {}\n")),
            "the net went order-insensitive beyond import runs"
        );
    }

    // A block-scoped import run (brace depth ≥ 1) is likewise NOT forgiven by
    // the net: the two orders stay distinct, so the net still polices the
    // placements the printer deliberately never reorders.
    #[test]
    fn net_does_not_forgive_block_scoped_import_order() {
        assert_ne!(
            normalize(raw_tokens("fun f() {\nuse b::y;\nuse a::x;\n}\n")),
            normalize(raw_tokens("fun f() {\nuse a::x;\nuse b::y;\n}\n")),
            "normalize must not reorder block-scoped imports"
        );
    }
}

#[cfg(test)]
mod organize {
    //! `organize_import_runs` backs the LSP "Organize Imports" action: it sorts
    //! top-level import runs into the same canonical order `vilan fmt` produces
    //! and prunes the leaves an analyzer reports unused. Here the analyzer's role
    //! is faked by a name list — `keep` rejects any leaf whose terminal name is
    //! in `dead` — so these pin the sort/prune/edit mechanics in isolation; the
    //! LSP pins cover the real (usage-driven, macro-aware) predicate.
    use super::organize_import_runs;
    use crate::span::Span;

    /// Applies the organizer's edits to `source`, treating every leaf named in
    /// `dead` as unused. Edits apply back-to-front so earlier offsets stay valid.
    pub(super) fn organize(source: &str, dead: &[&str]) -> String {
        let keep = |span: Span| !dead.contains(&&source[span.into_range()]);
        let mut edits = organize_import_runs(source, &keep).expect("source parses cleanly");
        edits.sort_by_key(|edit| std::cmp::Reverse(edit.span.into_range().start));
        let mut result = source.to_string();
        for edit in edits {
            result.replace_range(edit.span.into_range(), &edit.replacement);
        }
        result
    }

    /// The organizer offers no edit at all (already organized / nothing to prune).
    fn assert_no_edit(source: &str, dead: &[&str]) {
        let keep = |span: Span| !dead.contains(&&source[span.into_range()]);
        let edits = organize_import_runs(source, &keep).expect("source parses cleanly");
        assert!(
            edits.is_empty(),
            "expected no edit, got {} edit(s)",
            edits.len()
        );
    }

    // Sort-only (nothing dead): a shuffled run reorders exactly as `vilan fmt`.
    #[test]
    fn sort_only_reorders_a_run() {
        assert_eq!(
            organize("import std::z;\nimport std::a;\n", &[]),
            "import std::a;\nimport std::z;\n",
        );
    }

    // A whole import with no live leaf is dropped; the survivor stays.
    #[test]
    fn a_dead_import_is_pruned() {
        assert_eq!(
            organize("import std::used;\nimport std::dead;\n", &["dead"]),
            "import std::used;\n",
        );
    }

    // A dead brace-set branch shrinks the set — down to the unbraced spelling
    // when one member remains (`{ alpha, beta }` → `alpha`, kolt.local 005); the
    // whole import survives because a live branch remains.
    #[test]
    fn a_dead_branch_shrinks_its_set() {
        assert_eq!(
            organize("import std::x::{ alpha, beta };\n", &["beta"]),
            "import std::x::alpha;\n",
        );
        // The middle of three goes, leaving the two live ones in canonical order.
        assert_eq!(
            organize("import std::x::{ a, b, c };\n", &["b"]),
            "import std::x::{ a, c };\n",
        );
    }

    // A whole import prunes only when EVERY branch is dead.
    #[test]
    fn an_import_dies_only_when_all_branches_do() {
        assert_eq!(
            organize("import std::x::{ a, b };\nfun main() {}\n", &["a", "b"]),
            "fun main() {}\n",
        );
    }

    // A re-export is surface, not usage — never pruned, even when its name is
    // reported unused. (It still participates in sorting.)
    #[test]
    fn a_reexport_is_never_pruned() {
        assert_no_edit("export import std::api;\n", &["api"]);
        // And a dead plain import next to a same-named re-export: the re-export
        // stays, the plain import goes.
        assert_eq!(
            organize(
                "export import std::api;\nimport std::dead;\n",
                &["api", "dead"],
            ),
            "export import std::api;\n",
        );
    }

    // A trailing same-line comment travels with its surviving import when the run
    // is reordered.
    #[test]
    fn a_trailing_comment_travels_when_sorting() {
        assert_eq!(
            organize("import std::z; // note z\nimport std::a;\n", &[]),
            "import std::a;\nimport std::z; // note z\n",
        );
    }

    // An already-canonical run with nothing dead offers no edit (the no-op the
    // action relies on to stay quiet).
    #[test]
    fn already_organized_offers_no_edit() {
        assert_no_edit(
            "import std::a;\nimport std::b;\nuse dep::x;\n\nfun main() {}\n",
            &[],
        );
    }

    // Block-scoped imports live in a block body, never the top-level list, so the
    // organizer leaves them entirely alone — order AND unused leaves both.
    #[test]
    fn block_scoped_imports_are_left_alone() {
        assert_no_edit(
            "fun f() {\n\tuse zeta::last;\n\tuse alpha::first;\n\tfirst();\n}\n",
            &["last"],
        );
    }

    // A whole run pruned away is deleted, taking its line break so no blank line
    // is left behind.
    #[test]
    fn a_fully_dead_run_is_deleted() {
        assert_eq!(
            organize("import std::dead;\nfun main() {}\n", &["dead"]),
            "fun main() {}\n",
        );
    }

    // Sort and prune compose: the run reorders and the dead leaf disappears in one
    // edit — and the surviving one-member set renders unbraced.
    #[test]
    fn sort_and_prune_compose() {
        assert_eq!(
            organize(
                "import std::z;\nimport std::a::{ keep, drop };\n",
                &["drop"],
            ),
            "import std::a::keep;\nimport std::z;\n",
        );
    }

    // --- CRLF buffers (windows-support.md §2) --------------------------------
    //
    // The organizer works on the buffer AS WRITTEN — its edits are spans into the
    // client's text, so unlike `format` it must never normalize its input. That
    // makes the two byte-level line-break checks its own responsibility.

    /// The CRLF twin of a buffer.
    fn crlf(source: &str) -> String {
        source.replace('\n', "\r\n")
    }

    // The permanent-dirty case: an already-organized CRLF run must offer NO edit.
    // Comparing the canonical (`\n`-joined) text against the raw slice never
    // matched, so the action was offered forever — and `codeActionsOnSave`
    // rewrote the run to LF on every single save.
    #[test]
    fn an_already_organized_crlf_run_offers_no_edit() {
        assert_no_edit(
            &crlf("import std::a;\nimport std::z;\n\nfun main() {}\n"),
            &[],
        );
    }

    // A CRLF run that really is out of order still reorders — the EOL-insensitive
    // comparison must not swallow a genuine edit.
    #[test]
    fn an_unsorted_crlf_run_still_reorders() {
        assert_eq!(
            organize(&crlf("import std::z;\nimport std::a;\n"), &[]),
            "import std::a;\nimport std::z;\r\n",
        );
    }

    // Deleting a fully-dead run takes the whole `\r\n`, not just the `\n` —
    // otherwise a stray `\r` is left behind as an empty line.
    #[test]
    fn a_fully_dead_crlf_run_is_deleted_with_its_whole_line_break() {
        let organized = organize(&crlf("import std::dead;\nfun main() {}\n"), &["dead"]);
        assert_eq!(organized, "fun main() {}\r\n");
        assert!(
            !organized.starts_with('\r'),
            "a stray CR is left behind: {organized:?}"
        );
    }
}

#[cfg(test)]
mod insert {
    //! `insert_import` backs the LSP's add-import quickfix and its auto-import
    //! completion edit (E54): given a module path and a leaf name, the edit
    //! that adds `import <path>::<leaf>;` to the file — extending an existing
    //! import that already reaches the module, or inserting a new, sorted
    //! line when none does.
    use super::{ParsedSource, buffer_parse_count, insert_import};

    /// Applies `insert_import`'s edit to `source`, or `None` when it offers
    /// none (the leaf is already imported).
    fn apply(source: &str, module_path: &[&str], leaf: &str) -> Option<String> {
        let edit = insert_import(source, module_path, leaf)?;
        let mut result = source.to_string();
        result.replace_range(edit.span.into_range(), &edit.replacement);
        Some(result)
    }

    // E83: auto-import completion probes MANY leaves against ONE buffer, so
    // `insert_import` gained a parsed-input twin. The twin must answer
    // byte-identically to the string entry across the edit shapes — extend a
    // set, extend the surface import, a fresh sorted statement, already
    // imported — while paying the whole-buffer parse once, not once per
    // probe (the string entry pays it per call, which is the cost §9 measured).
    #[test]
    fn a_parsed_source_answers_every_probe_identically_from_one_parse() {
        let source =
            "import std::json::{ Alpha, Zeta };\nimport std::io::print;\n\nfun main() {}\n";
        let probes: [(&[&str], &str); 4] = [
            (&["std", "json"], "Mid"),   // inserts into the brace set
            (&["std"], "read"),          // extends the surface import
            (&["std", "math"], "sqrt"),  // a fresh, sorted statement
            (&["std", "json"], "Alpha"), // already imported: no edit
        ];
        let parses_before = buffer_parse_count();
        let parsed = ParsedSource::parse(source).expect("the fixture parses cleanly");
        for (module_path, leaf) in probes {
            let from_parsed = parsed.insert_import(module_path, leaf);
            let from_string = insert_import(source, module_path, leaf);
            assert_eq!(
                from_parsed
                    .as_ref()
                    .map(|edit| (edit.span.into_range(), &edit.replacement)),
                from_string
                    .as_ref()
                    .map(|edit| (edit.span.into_range(), &edit.replacement)),
                "the parsed and string entries must answer identically for {module_path:?}::{leaf}"
            );
        }
        // One parse for `ParsedSource::parse`, one per string-entry call —
        // the parsed entry's probes never re-parse.
        assert_eq!(
            buffer_parse_count() - parses_before,
            1 + probes.len() as u64
        );
    }

    // The parsed entry keeps the string entry's safety rule: a buffer that
    // does not parse cleanly yields no handle at all, so no probe can offer
    // an edit into text the formatter does not understand.
    #[test]
    fn a_source_that_does_not_parse_cleanly_yields_no_parsed_source() {
        assert!(ParsedSource::parse("import std::json::{\n").is_none());
    }

    // --- Brace-set extension ---------------------------------------------------

    // A new leaf lands at its alphabetically-sorted position inside an
    // existing brace set — in the middle...
    #[test]
    fn a_new_leaf_inserts_into_the_middle_of_an_existing_set() {
        assert_eq!(
            apply(
                "import std::json::{ Alpha, Zeta };\n",
                &["std", "json"],
                "Mid"
            )
            .unwrap(),
            "import std::json::{ Alpha, Mid, Zeta };\n",
        );
    }

    // ...and at the end, when it sorts after every existing member.
    #[test]
    fn a_new_leaf_appends_to_the_end_of_an_existing_set() {
        assert_eq!(
            apply(
                "import std::json::{ Alpha, Mid };\n",
                &["std", "json"],
                "Zeta"
            )
            .unwrap(),
            "import std::json::{ Alpha, Mid, Zeta };\n",
        );
    }

    // A bare single leaf (no braces yet) becomes a two-member set, sorted —
    // from both directions.
    #[test]
    fn a_bare_leaf_becomes_a_sorted_two_member_set() {
        assert_eq!(
            apply("import std::json::Json;\n", &["std", "json"], "Apple").unwrap(),
            "import std::json::{ Apple, Json };\n",
        );
        assert_eq!(
            apply("import std::json::Json;\n", &["std", "json"], "Zebra").unwrap(),
            "import std::json::{ Json, Zebra };\n",
        );
    }

    // The one-segment (package-surface) form extends the same way — the
    // module path is just `["std"]`, not `["std", <module>]`. Spelled with a
    // stand-in surface: std's own package root publishes nothing since the
    // alias sweep (prelude.md §10.2), and this assertion is textual.
    #[test]
    fn a_surface_level_bare_leaf_becomes_a_set() {
        assert_eq!(
            apply("import shapes::alpha;\n", &["shapes"], "beta").unwrap(),
            "import shapes::{ alpha, beta };\n",
        );
    }

    // Extension finds a matching import ANYWHERE in the file, not only the
    // first one — and skips a RE-EXPORT of the same module entirely (adding a
    // plain leaf to someone's `export import` would silently publish it too),
    // reaching past it to the plain import that follows.
    #[test]
    fn extension_skips_a_reexport_and_reaches_the_plain_import_after_it() {
        assert_eq!(
            apply(
                "export import std::json::{ Decode };\nimport std::json::Json;\n",
                &["std", "json"],
                "Encode",
            )
            .unwrap(),
            "export import std::json::{ Decode };\nimport std::json::{ Encode, Json };\n",
        );
    }

    // Already imported (bare or in a set): no edit — there's nothing to add.
    #[test]
    fn an_already_imported_bare_leaf_yields_no_edit() {
        assert!(apply("import std::json::Json;\n", &["std", "json"], "Json").is_none());
    }

    #[test]
    fn an_already_imported_set_member_yields_no_edit() {
        assert!(
            apply(
                "import std::json::{ Alpha, Json };\n",
                &["std", "json"],
                "Json",
            )
            .is_none()
        );
    }

    // --- New sorted import line -------------------------------------------------

    // No existing import reaches the module: a new statement lands at its
    // sorted position inside the file's existing run — in the middle... (each
    // existing import is from a DIFFERENT module, `["std", "alpha"]` /
    // `["std", "zeta"]`, so neither is a candidate to extend — this is purely
    // the new-line path.)
    #[test]
    fn a_new_import_line_inserts_into_the_middle_of_the_run() {
        assert_eq!(
            apply(
                "import std::alpha::A;\nimport std::zeta::Z;\n",
                &["std", "middle"],
                "M",
            )
            .unwrap(),
            "import std::alpha::A;\nimport std::middle::M;\nimport std::zeta::Z;\n",
        );
    }

    // ...and appended after the run, when it sorts last.
    #[test]
    fn a_new_import_line_appends_after_the_run() {
        assert_eq!(
            apply(
                "import std::alpha::A;\nimport std::middle::M;\n",
                &["std", "zeta"],
                "Z",
            )
            .unwrap(),
            "import std::alpha::A;\nimport std::middle::M;\nimport std::zeta::Z;\n",
        );
    }

    // No import anywhere in the file: the new line becomes the file's first.
    #[test]
    fn a_new_import_becomes_the_files_first_line_when_there_is_no_run() {
        assert_eq!(
            apply("fun main() {}\n", &["std", "json"], "Json").unwrap(),
            "import std::json::Json;\nfun main() {}\n",
        );
    }

    // A `use` statement reaching the same names is not a `import` and is
    // never extended — a fresh `import` line is inserted instead, sorted
    // alongside it (kind sorts before `use` — `import_sort_key`).
    #[test]
    fn a_use_statement_is_never_extended() {
        assert_eq!(
            apply("use std::json::{ Decode };\n", &["std", "json"], "Json").unwrap(),
            "import std::json::Json;\nuse std::json::{ Decode };\n",
        );
    }
}

/// Canonical Vilan is LF with no BOM (windows-support.md §2 (b)): `vilan fmt`
/// converting a CRLF file is a correct reformat, not a bug — one canonical form,
/// exactly as with indentation. These pin that the conversion happens once, is
/// idempotent after, and does NOT trip the token-stream safety net (which would
/// silently leave the file untouched).
#[cfg(test)]
mod newlines {
    use super::format;

    /// The LF twin of `source`, i.e. what a CRLF file must format to.
    fn crlf(source: &str) -> String {
        source.replace('\n', "\r\n")
    }

    #[test]
    fn a_crlf_file_formats_to_its_lf_form_exactly_once() {
        let canonical = "fun main() {\n\tlet x = 1;\n}\n";
        let formatted = format(&crlf(canonical));
        assert_eq!(formatted, canonical);
        // Idempotent after: the second pass is a no-op, so a file converges
        // rather than oscillating between two "canonical" forms.
        assert_eq!(format(&formatted), canonical);
    }

    #[test]
    fn a_crlf_multi_line_string_does_not_bail_the_safety_net() {
        // The net compares the input's token stream against the reprint's. A
        // multi-line string's token carries its RAW body, so without normalizing
        // the input the two sides would disagree on `\r\n` vs `\n` and the
        // formatter would bail — leaving CRLF on disk forever. (The literal is
        // triple-quoted: a `"…"` no longer spans lines at all.)
        let canonical = "fun main() {\n\tlet text = \"\"\"\n\talpha\n\tbeta\n\t\"\"\";\n}\n";
        assert_eq!(format(&crlf(canonical)), canonical);
    }

    #[test]
    fn a_crlf_interpolated_string_reprints_without_carriage_returns() {
        // An i-string is recovered VERBATIM from source, so its slice is the
        // path a `\r` would ride into formatted output.
        let canonical = "fun main() {\n\tlet who = \"w\";\n\tlet t = i\"\"\"\n\thi {who}\n\tbye\n\t\"\"\";\n}\n";
        let formatted = format(&crlf(canonical));
        assert!(!formatted.contains('\r'), "{formatted:?}");
        assert_eq!(formatted, canonical);
    }

    #[test]
    fn a_line_break_in_a_single_quoted_string_bails_the_formatter() {
        // A now-illegal literal must never reach the printer: the reprint would
        // silently ADD the closing quote the author left off, rewriting the
        // program. `code_tokens` declines on any lexer error, so `format` takes
        // the ordinary bail path and returns the input's bytes untouched —
        // including its line endings, which is why the CRLF twin is checked too.
        for source in [
            "fun main() {\n\tlet text = \"alpha\nbeta\";\n}\n",
            "fun main() {\n\tlet text = i\"alpha\nbeta\";\n}\n",
        ] {
            assert_eq!(format(source), source, "{source:?}");
            let windows = crlf(source);
            assert_eq!(format(&windows), windows, "{source:?}");
        }
    }

    #[test]
    fn crlf_macro_arguments_reprint_without_carriage_returns() {
        // Macro arguments are syntax, reprinted verbatim from their spans — the
        // other verbatim path into the output.
        let canonical = "fun main() {\n\tmacro unroll(2, |i: i32| i\n\t\t+ 1);\n}\n";
        let formatted = format(&crlf(canonical));
        assert!(!formatted.contains('\r'), "{formatted:?}");
        // …and the CRLF file formats to exactly what its LF twin formats to,
        // which is also what proves the net did not silently bail.
        assert_eq!(formatted, format(canonical));
    }

    #[test]
    fn a_crlf_triple_quoted_string_reprints_without_carriage_returns() {
        // A triple-quoted body reprints verbatim (its whitespace is semantic),
        // so it is a third verbatim path; its VALUE already strips CR.
        let canonical = "fun main() {\n\tlet t = \"\"\"\n\ta\n\tb\n\t\"\"\";\n}\n";
        let formatted = format(&crlf(canonical));
        assert!(!formatted.contains('\r'), "{formatted:?}");
        assert_eq!(formatted, canonical);
    }

    #[test]
    fn a_crlf_interpolated_triple_quoted_string_reprints_without_carriage_returns() {
        // H7's literal joins the two verbatim paths above: it is recovered from
        // source like an i-string AND carries semantic inner whitespace like a
        // triple-quoted one.
        let canonical =
            "fun main() {\n\tlet w = \"w\";\n\tlet t = i\"\"\"\n\ta {w}\n\tb\n\t\"\"\";\n}\n";
        let formatted = format(&crlf(canonical));
        assert!(!formatted.contains('\r'), "{formatted:?}");
        assert_eq!(formatted, canonical);
    }

    #[test]
    fn a_leading_byte_order_mark_is_dropped_by_a_successful_format() {
        let canonical = "fun main() {}\n";
        assert_eq!(format(&format!("\u{feff}{canonical}")), canonical);
    }

    #[test]
    fn a_file_the_formatter_bails_on_keeps_its_original_bytes() {
        // A bail returns the input untouched — a file we do not fully
        // understand is not one to rewrite, not even its line endings.
        let broken = crlf("fun main( {\n");
        assert_eq!(format(&broken), broken);
    }
}

#[cfg(test)]
mod style_chain_order {
    //! kolt.local 006 — `vilan fmt` sorts the `.name(…)` links of a `style()`
    //! builder chain into Tailwind CSS's category order, with the condition
    //! combinators last in the axis order the selector nests them.
    //!
    //! The order is a ruling; the two rules that keep it SAFE are the design.
    //! A chain merges last-wins per property slot, so (a) a method the table
    //! does not know is a BARRIER that nothing crosses, and (b) methods whose
    //! slots are entangled share a FAMILY and never move relative to each
    //! other. Everything below either pins the ruling or pins one of those two
    //! rules. `crates/vilan-core/tests/style_table_sync.rs` holds the table to
    //! `vilan/std/src/style.vl`; `crates/vilan-cli/tests/style_chain_order.rs`
    //! proves the reorder leaves the emitted CSS byte-identical.
    use super::bailing_constructs::assert_construct;
    use super::{
        STYLE_BARRIER_METHODS, STYLE_CONDITION_METHODS, STYLE_PROPERTY_METHODS, css_item_rank,
        format, style_chain_permutation, style_link_rank,
    };

    // --- The ruling: Tailwind's category order -------------------------------

    /// One representative method per Tailwind category with a `Style` member,
    /// written in exactly reverse category order, sorts into the sequence
    /// itself: layout, flexbox/grid, spacing, sizing, typography, backgrounds,
    /// borders, effects, transitions/animation, transforms, interactivity.
    #[test]
    fn the_canonical_order_is_tailwinds_category_sequence() {
        assert_construct(
            "let s = const style().user_select(UserSelect::None).transform(\"scale(2)\")\
             .transition(\"all 1s\").opacity(0.5).border_none().background_size(\"cover\")\
             .white_space(WhiteSpace::Nowrap).max_height(space(1)).margin(space(1))\
             .gap(space(1)).display(Display::Flex);\n",
            "let s = const style()\n\
             \t.display(Display::Flex)\n\
             \t.gap(space(1))\n\
             \t.margin(space(1))\n\
             \t.max_height(space(1))\n\
             \t.white_space(WhiteSpace::Nowrap)\n\
             \t.background_size(\"cover\")\n\
             \t.border_none()\n\
             \t.opacity(0.5)\n\
             \t.transition(\"all 1s\")\n\
             \t.transform(\"scale(2)\")\n\
             \t.user_select(UserSelect::None);\n",
        );
    }

    /// Inside one category the order is Tailwind's own property sequence —
    /// `border-radius` before `border-width`, `background-color` before
    /// `background-image`.
    #[test]
    fn within_a_category_the_order_is_tailwinds_property_sequence() {
        assert_construct(
            "let s = const style().border_none().radius(space(1));\n",
            "let s = const style().radius(space(1)).border_none();\n",
        );
        assert_construct(
            "let s = const style().background_image(\"url(t.png)\").background(ink());\n",
            "let s = const style().background(ink()).background_image(\"url(t.png)\");\n",
        );
    }

    // --- Conditions last, in axis order --------------------------------------

    /// Every condition combinator sorts after every property method, however
    /// early it was written.
    #[test]
    fn conditions_sort_after_every_property_method() {
        assert_construct(
            "let s = const style().hover(style().color(ink())).padding(space(2));\n",
            "let s = const style().padding(space(2)).hover(style().color(ink()));\n",
        );
    }

    /// Among themselves the conditions follow the axis order the selector nests
    /// them in — media, the relation, attribute, pseudo — which is the order
    /// the combinators already require when they are NESTED (`style.vl`'s
    /// `render_rule`). Written here in exactly the reverse.
    #[test]
    fn conditions_sort_in_the_axis_order_the_selector_nests() {
        // The sorted chain is over the 100-column budget, so the canonical
        // form is also the split form — one link per line.
        assert_construct(
            "let s = const style().hover(a).attribute(\"data-open\", \"true\", b).within(\"data-theme\", \"dark\", c).md(d);\n",
            "let s = const style()\n\t.md(d)\n\t.within(\"data-theme\", \"dark\", c)\n\t.attribute(\"data-open\", \"true\", b)\n\t.hover(a);\n",
        );
    }

    /// The three relations share one axis, so they keep their written order —
    /// `children`/`divide` never cross `within` or each other.
    #[test]
    fn relations_keep_their_written_order() {
        for chain in [
            "let s = const style().children(a).divide(b);\n",
            "let s = const style().divide(a).within(\"data-theme\", \"dark\", b);\n",
        ] {
            assert_construct(chain, chain);
        }
    }

    /// Two conditions on the SAME axis keep their written order, which is what
    /// lets `media`'s arbitrary min-width sit among `sm`/`md`/`lg`/`xl` without
    /// the formatter having to read its argument.
    #[test]
    fn two_conditions_on_one_axis_keep_their_written_order() {
        for chain in [
            "let s = const style().lg(a).sm(b);\n",
            "let s = const style().media(\"50rem\", a).sm(b);\n",
            "let s = const style().focus(a).hover(b);\n",
        ] {
            assert_construct(chain, chain);
        }
    }

    // --- The barrier rule ----------------------------------------------------

    /// The load-bearing rule. A user `impl Style` extension can write ANY slots
    /// — kolt's `paint_primary` writes colour AND background — and the
    /// formatter cannot know which, so moving a known method across it could
    /// silently change the rendered style. An unknown method therefore holds
    /// its position absolutely and nothing crosses it.
    #[test]
    fn an_unknown_method_is_a_barrier_nothing_crosses() {
        assert_construct(
            "let s = const style().color(ink()).paint_primary().padding(space(2));\n",
            "let s = const style().color(ink()).paint_primary().padding(space(2));\n",
        );
    }

    /// The barrier cuts the chain into runs, and each run sorts on its own —
    /// so the sort still does its work on both sides of a custom method.
    #[test]
    fn each_run_between_barriers_sorts_on_its_own() {
        assert_construct(
            "let s = const style().color(a).display(b).ghost().opacity(0.5).gap(c);\n",
            "let s = const style().display(b).color(a).ghost().gap(c).opacity(0.5);\n",
        );
    }

    /// Degrades gracefully: a chain of nothing but custom methods is left
    /// exactly as written.
    #[test]
    fn an_all_custom_chain_is_left_alone() {
        let chain = "let s = const style().paint_ink2().script_label().ghost();\n";
        assert_construct(chain, chain);
    }

    /// The std escape hatches are barriers for the same reason a user extension
    /// is: the slot they write is an ARGUMENT, not the method name, so the
    /// formatter cannot know it without evaluating the call. `border_none()` IS
    /// `raw("border", "none")`, and both live instances of the hazard the family
    /// rules fixed had `raw` on one side.
    #[test]
    fn an_escape_hatch_whose_slot_is_an_argument_is_a_barrier() {
        for hatch in [
            "raw(\"left\", \"30%\")",
            "with_length(\"gap\", space(1))",
            "with_color(\"color\", ink())",
        ] {
            let chain = format!("let s = const style().color(a).{hatch}.padding(b);\n");
            assert_construct(&chain, &chain);
        }
    }

    // --- The family rule -----------------------------------------------------

    /// `padding` then `padding_x` means something and the reverse means
    /// something else (`proposal/ui-styling.md` §0bis), so the two never swap —
    /// in EITHER written order, which is what proves the pin is about
    /// preservation and not about one lucky direction.
    #[test]
    fn a_shorthand_and_its_longhand_keep_their_written_order() {
        for chain in [
            "let s = const style().padding(a).padding_x(b);\n",
            "let s = const style().padding_x(b).padding(a);\n",
            "let s = const style().margin_left(a).margin(b);\n",
            "let s = const style().top(a).inset(b);\n",
        ] {
            assert_construct(chain, chain);
        }
    }

    /// `size` writes the same two slots `width` and `height` write, so the three
    /// are one family however they are spelled.
    #[test]
    fn size_never_crosses_width_or_height() {
        for chain in [
            "let s = const style().size(a).width(b);\n",
            "let s = const style().height(a).size(b);\n",
            "let s = const style().height(a).width(b);\n",
        ] {
            assert_construct(chain, chain);
        }
    }

    /// `border-color` is one of the `border` shorthand's longhands, so
    /// `border_color` is in the `border` family — even though Tailwind's own
    /// sequence would put border-width ahead of border-color.
    #[test]
    fn border_color_never_crosses_the_border_shorthand() {
        let chain = "let s = const style().border_color(a).border(w, c);\n";
        assert_construct(chain, chain);
    }

    /// Two methods on the SAME property are trivially one family.
    #[test]
    fn two_methods_on_one_property_keep_their_written_order() {
        for chain in [
            "let s = const style().line_height_length(a).line_height(1.5);\n",
            "let s = const style().background_image(a).background_gradient(g);\n",
            "let s = const style().border_none().border(w, c);\n",
        ] {
            assert_construct(chain, chain);
        }
    }

    /// The family rule is a floor, not a ceiling: independent slots inside one
    /// family's category still sort. `border-radius` is deliberately NOT one of
    /// the `border` shorthand's longhands (`family_longhands` is not
    /// prefix-based), so `radius` is its own family and does cross `border`.
    #[test]
    fn independent_slots_still_sort_past_a_family() {
        assert_construct(
            "let s = const style().border(w, c).radius(r);\n",
            "let s = const style().radius(r).border(w, c);\n",
        );
        // `flex-direction` shares a prefix with the `flex` shorthand and is not
        // covered by it either.
        assert_construct(
            "let s = const style().flex(\"1\").flex_direction(FlexDirection::Row);\n",
            "let s = const style().flex_direction(FlexDirection::Row).flex(\"1\");\n",
        );
    }

    // --- Reach, and what the sort refuses ------------------------------------

    /// A condition's inner `style()` is a chain of its own and sorts too.
    #[test]
    fn a_nested_chain_inside_a_condition_sorts() {
        assert_construct(
            "let s = const style().hover(style().color(a).display(b));\n",
            "let s = const style().hover(style().display(b).color(a));\n",
        );
    }

    /// Only a `style()` BUILDER sorts. A chain on any other subject is somebody
    /// else's API, whose method names mean nothing to this table.
    #[test]
    fn a_chain_on_another_subject_is_left_alone() {
        for chain in [
            "let s = theme().color(a).display(b);\n",
            "let s = base.color(a).display(b);\n",
        ] {
            assert_construct(chain, chain);
        }
    }

    /// A postfix that is not a `.name(…)` call link ENDS the sortable run: the
    /// links before it still sort among themselves, and nothing crosses it —
    /// past one the receiver is no longer the `Style` this table describes.
    #[test]
    fn a_non_call_postfix_ends_the_run() {
        assert_construct(
            "let s = const style().color(a).display(b).rules;\n",
            "let s = const style().display(b).color(a).rules;\n",
        );
        assert_construct(
            "let s = style().color(a).display(b)!;\n",
            "let s = style().display(b).color(a)!;\n",
        );
        // Nothing crosses it: `display` is stranded past the `.rules` boundary
        // on a receiver this table knows nothing about, so it stays put.
        let stranded = "let s = const style().color(a).rules.display(b);\n";
        assert_construct(stranded, stranded);
    }

    /// A comment between two links would be carried to the wrong link by a
    /// reorder — the comment cursor only moves forward — so a chain with a
    /// comment anywhere inside it is left as written.
    #[test]
    fn a_chain_carrying_a_comment_is_left_alone() {
        assert_construct(
            "let s = const style()\n\t.color(a)\n\t// the box\n\t.display(b);\n",
            "let s = const style()\n\t.color(a)\n\t// the box\n\t.display(b);\n",
        );
    }

    /// `Style + Style` operands never reorder — that merge's order is semantic
    /// (`style.vl`'s `add` replays the right side over the left). Only the links
    /// INSIDE one builder sort.
    #[test]
    fn merge_operands_never_reorder() {
        assert_construct(
            "let s = const (style().color(a).display(b) + base);\n",
            "let s = const (style().display(b).color(a) + base);\n",
        );
    }

    // --- One order, two spellings (css-block.md §8, S3) ----------------------

    /// The block does not get a table. It gets the SAME rows, read by the CSS
    /// property they write instead of by the method name that writes it — so a
    /// declaration and the typed method it could have been spelled as rank
    /// identically, for every row in the table.
    ///
    /// This is what makes `css { padding: x; display: y; }` and
    /// `style().padding(x).display(y)` format into the same order. If the two
    /// ever disagree, one canonical style would have two canonical spellings.
    #[test]
    fn a_declaration_ranks_exactly_where_its_typed_method_does() {
        for method in STYLE_PROPERTY_METHODS {
            let by_method = style_link_rank(method.name);
            assert!(
                by_method.is_some(),
                "{} is in the property table but does not rank",
                method.name
            );
            for property in method.properties {
                assert_eq!(
                    css_item_rank(false, property),
                    by_method,
                    "the block ranks `{property}` somewhere other than where the chain ranks \
                     `{}`, so the two spellings would format differently",
                    method.name
                );
            }
        }
    }

    /// A dotted item reads the condition table, and reads it whole.
    #[test]
    fn a_nested_rule_ranks_exactly_where_its_combinator_does() {
        for (condition, _) in STYLE_CONDITION_METHODS {
            assert_eq!(
                css_item_rank(true, condition),
                style_link_rank(condition),
                "the block ranks `.{condition}` somewhere other than where the chain ranks it"
            );
        }
    }

    /// The dot is the disambiguator on this side too. A condition NAME in
    /// property position is not a property, and a property name in dotted
    /// position is not a condition — both are barriers, so neither can be
    /// mistaken for the other and reordered across something it depends on.
    #[test]
    fn the_dot_decides_which_table_an_item_reads() {
        assert_eq!(css_item_rank(false, "hover"), None);
        assert_eq!(css_item_rank(true, "padding"), None);
        // …and a property no row writes is a barrier, which is `raw`'s escape
        // hatch surviving into the block whole.
        assert_eq!(css_item_rank(false, "-webkit-mask-composite"), None);
        assert_eq!(css_item_rank(false, "--brand-ink"), None);
    }

    /// A `Style` method's name is not a CSS property, and the block must not
    /// accidentally rank one as if it were: `flex_direction` is spelled
    /// `flex-direction` in CSS, and only the hyphenated form ranks.
    #[test]
    fn a_method_name_is_not_a_css_property() {
        assert_eq!(css_item_rank(false, "flex_direction"), None);
        assert!(css_item_rank(false, "flex-direction").is_some());
    }

    // --- Idempotence ---------------------------------------------------------

    /// Formatting twice equals formatting once. `assert_construct` checks this
    /// per fixture; this pin checks it on the shape most likely to break it —
    /// a long chain that both SORTS and SPLITS, where the second pass re-reads
    /// an already-permuted spine.
    #[test]
    fn sorting_is_idempotent() {
        let source = "let s = const style().hover(style().color(a)).user_select(UserSelect::None)\
                      .paint_primary().opacity(0.5).padding_x(space(2)).padding(space(1))\
                      .md(style().display(Display::Flex)).radius(space(1)).gap(space(4));\n";
        let once = format(source);
        assert_eq!(format(&once), once, "not idempotent: {source:?}");
        assert_ne!(once, source, "fixture did not reorder, so it pins nothing");
    }

    // --- The permutation itself ----------------------------------------------

    /// An already-canonical chain reports NO permutation, so it stays on the
    /// formatter's existing code path byte for byte and this feature cannot
    /// perturb a chain it has nothing to say about.
    #[test]
    fn a_canonical_chain_reports_no_permutation() {
        assert_eq!(
            style_chain_permutation(&["display", "padding", "color", "hover"]),
            None
        );
        assert_eq!(style_chain_permutation(&["ghost", "paint_primary"]), None);
        assert_eq!(style_chain_permutation(&[]), None);
    }

    // --- The table's own shape -----------------------------------------------

    /// Row order IS the canonical order, so the table must be grouped by
    /// category in Tailwind's sequence — otherwise a family's rank could put it
    /// in the wrong category's band.
    #[test]
    fn the_table_is_grouped_by_category_in_tailwind_order() {
        let mut seen = Vec::new();
        for method in STYLE_PROPERTY_METHODS {
            if seen.last() != Some(&method.category) {
                assert!(
                    !seen.contains(&method.category),
                    "category {:?} is not contiguous — {} reopens it",
                    method.category,
                    method.name
                );
                assert!(
                    seen.last().is_none_or(|last| *last < method.category),
                    "category {:?} sorts before {:?} but is written after it",
                    method.category,
                    seen.last()
                );
                seen.push(method.category);
            }
        }
    }

    /// A family must be contiguous and live in ONE category: its rank is its
    /// first row's index, so a family straddling a category boundary would drag
    /// the later rows backwards out of their band.
    #[test]
    fn every_family_is_contiguous_and_lives_in_one_category() {
        let mut seen: Vec<&str> = Vec::new();
        for (at, method) in STYLE_PROPERTY_METHODS.iter().enumerate() {
            if at > 0 && STYLE_PROPERTY_METHODS[at - 1].family == method.family {
                assert_eq!(
                    STYLE_PROPERTY_METHODS[at - 1].category,
                    method.category,
                    "family {:?} straddles two categories",
                    method.family
                );
                continue;
            }
            assert!(
                !seen.contains(&method.family),
                "family {:?} is not contiguous — {} reopens it",
                method.family,
                method.name
            );
            seen.push(method.family);
        }
    }

    /// One row per method, and no name claimed by two of the three tables — a
    /// duplicate would make the rank silently depend on which row `find` reached
    /// first.
    #[test]
    fn every_method_name_is_claimed_exactly_once() {
        let mut names: Vec<&str> = STYLE_PROPERTY_METHODS.iter().map(|row| row.name).collect();
        names.extend(STYLE_CONDITION_METHODS.iter().map(|(name, _)| *name));
        names.extend(STYLE_BARRIER_METHODS);
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.len(),
            names.len(),
            "a style method name is claimed by more than one row"
        );
    }

    /// Every row names at least one CSS property — the column the family
    /// partition is checked against in
    /// `crates/vilan-core/tests/style_table_sync.rs`.
    #[test]
    fn every_property_row_names_its_slots() {
        for method in STYLE_PROPERTY_METHODS {
            assert!(
                !method.properties.is_empty(),
                "{} names no property",
                method.name
            );
        }
    }
}
