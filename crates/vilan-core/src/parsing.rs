//! The handwritten parser (H6 S3 + S4, `proposal/frontend.md` §2 "Parser" + §3).
//!
//! A dependency-free, single-pass recursive-descent + precedence-climbing parser
//! over `&[(Token, Span)]` (the output of [`crate::lexing::tokenize`], which S1
//! proved byte-identical to the chumsky lexer). It produces the same
//! `Spanned<Node>` tree — spans included — that the chumsky grammar in `parser.rs`
//! produces, which stays in-tree as the oracle for the whole H6 arc (deleted at
//! S5). Nothing in the pipeline calls this yet; it is exercised by the corpus-
//! scale differential (`tests/parse_differential.rs`, which S3 repoints at this
//! module), the corpus-through-the-new-frontend byte gate
//! (`tests/corpus_new_frontend.rs`), the recovery pins (`tests/parser_recovery.rs`,
//! S4-repointed to run against BOTH frontends), the recovery-mode differential
//! (`tests/parse_recovery_differential.rs`), and this module's own pins.
//!
//! S4 adds RECOVERY and rich errors on top of the clean grammar: the
//! `nested_delimiters` sites reproduce their placeholders ([`Parser::recover_delimited`]),
//! the top-level parse keeps whatever prefix parsed, and [`ParseError`] carries the
//! found/expected/context/hint a real renderer ([`render`]) needs — messages that
//! may IMPROVE on chumsky's (§6a), never wired into the pipeline (that is S5).
//!
//! `editing-dx.md` S1 finishes the recovery design `frontend.md` §2 ratified and
//! the cutover left unbuilt: **statement/item boundaries synchronize on
//! `;`/`}`/item keywords** ([`Parser::recover_statement`]). Both statement loops
//! used to `break` on the first statement that declined, which is what the survey
//! measured as the *blackout* — while a file does not parse, everything after the
//! break stops being analyzed, so the diagnostics the rest of the file already had
//! disappear from the editor. They now report that statement, skip to the next
//! boundary and resume, which retired the block site's region-skipping recovery
//! (nine `nested_delimiters` sites remain: a body salvages statement by statement
//! instead of collapsing to an empty block).
//!
//! With S3 the grammar is COMPLETE — the whole file, not just its expressions.
//! S2 covered the *expression* and *type* grammar plus the block-bearing forms
//! (`closure`/`if`/`for`/`match`/block/`let`/assignment/`ret`/`jump`). S3 fills the
//! seam: top-level *items* (`fun`/`struct`/`enum`/`impl`/`trait`/`mod`/`import`/
//! `use`/`export`), the bracket-attribute grammar (`[derive(..)]`/`[service(..)]`/
//! `[extern(..)]`/`[platform(..)]`/`[must_use]`/`[rpc]`/`[trait_only]`/`[doc(..)]`/
//! `[expose]` and user macro attributes), the *macro forms* (`macro fun`/`macro
//! { }`/`macro name(..)`), and the deferred `tuple_comprehension` atom. The
//! statement/item interleaving reproduces the chumsky `statement` choice ORDER
//! exactly — the attribute/macro/export forms lead, then `expression ;` (and the
//! block-bearing statement forms), then the declaration items — because that
//! order decides which reading of an ambiguous `[`- or `async`-led head wins.
//!
//! Faithfulness over improvement: every shape here reproduces the chumsky grammar,
//! quirks included (the split-shift reassembly, the H.1 struct-literal-free
//! condition mode as a boolean rather than a parallel grammar, the collect-then-
//! group `?.` continuation, the arrow-less closure *type*, the paren-dissolving
//! atom that keeps the inner expression's own span, `apply_binding_mutability`
//! leaving array binders untouched, the FIXED attribute order on a function, the
//! optional `;` on a `macro`-statement vs the mandatory one on an expression, the
//! `resource`-misplaced steer). Ugly-but-reproduced behaviours are recorded for
//! the S4/S5 error-quality pass, not fixed. The differential is the referee.

use std::borrow::Cow;
use std::cell::Cell;

use crate::lexing;
use crate::node::{
    ANONYMOUS_TYPE_BINDER, BackingLiteral, BinaryOp, Closure, ComprehensionBinding, Convention,
    CssBody, CssDeclaration, CssItem, CssNested, ElementBody, ElementChild, ElementHeadItem,
    EnumVariant, ExportScope, Exposure, ExternBinding, Func, GenericArguments, GenericParameter,
    GenericParameters, If, IfSpelling, ImplSelector, ImportBranch, ImportModifier, ImportTail,
    ItemLabels, Labels, MatchLeg, Node, NodeIfBranch, NodeList, Parameter, Pattern, Reactivity,
    ServiceAttr, StructField, TupleBound,
};
use crate::span::{Span, Spanned};
use crate::token::Token;

/// Whether `text` is spelled like an identifier: a letter or `_`, then letters,
/// digits and `_` — what `[reactive(name = "..")]` may name, since the name
/// becomes a method.
fn is_identifier_text(text: &str) -> bool {
    let mut characters = text.chars();
    match characters.next() {
        Some(first) if first.is_alphabetic() || first == '_' => {
            characters.all(|character| character.is_alphanumeric() || character == '_')
        }
        _ => false,
    }
}

/// Every `(impl …)` selector element's span in an import/use tree — what the
/// `use` refusal reports at.
fn collect_branch_selectors(branch: &ImportBranch<'_>, into: &mut Vec<Span>) {
    match branch {
        ImportBranch::Path(_, _, ImportTail::Continue(child)) => {
            collect_branch_selectors(child, into)
        }
        ImportBranch::Path(..) => {}
        ImportBranch::Set(branches) => {
            for child in branches {
                collect_branch_selectors(child, into);
            }
        }
        ImportBranch::Selector(selector) => into.push(selector.span),
        ImportBranch::Reach(_, inner) => collect_branch_selectors(inner, into),
    }
}

/// Every binder inside an impl selector's subject that the selector may not
/// write (B318, RULED: "no binders are written in a selector"): a NAMED binder
/// (`type T`) anywhere, and a bare `_` that carries a BOUND. A bound-less `_`
/// is the placeholder and contributes nothing.
///
/// Walks the type grammar's own shapes; anything else (a name, a literal
/// length) holds no binder and ends the descent.
fn collect_selector_binders(node: &Spanned<Node<'_>>, into: &mut Vec<Span>) {
    match &node.0 {
        Node::TypeBinder((name, _), bounds, _) => {
            if *name != ANONYMOUS_TYPE_BINDER || !bounds.is_empty() {
                into.push(node.1);
            }
            for bound in bounds {
                collect_selector_binders(bound, into);
            }
        }
        Node::AccessorWithGenerics(_, arguments) => {
            for argument in &arguments.0 {
                collect_selector_binders(argument, into);
            }
        }
        Node::Tuple(items) => {
            for item in items {
                collect_selector_binders(item, into);
            }
        }
        Node::ArrayType(element, _) => collect_selector_binders(element, into),
        Node::Reference(_, inner) | Node::TypeWithContexts(inner, _) => {
            collect_selector_binders(inner, into)
        }
        _ => {}
    }
}

/// A parse error value: where it was detected, *what* went wrong (found/expected,
/// or a curated reason), the production context, and an optional targeted hint.
/// This is the shape [`render`] turns into a user-facing message following the
/// diagnostics standard (`proposal/diagnostics-standard.md` §1-4). Proposal §6a
/// governs it: parse errors may *improve* on chumsky's at cutover — they are not
/// byte-matched — so this carries what a good renderer needs, not chumsky's
/// internal `Rich` shape. Rendering is exported for S5 and tested directly here;
/// nothing in the pipeline reads it yet.
#[derive(Clone, Debug)]
pub struct ParseError {
    /// Where the error is anchored (diagnostics-standard.md A1 — the narrowest
    /// identifying span): the offending token, the recovered delimiter region, or
    /// a zero-width span at end of input.
    pub span: Span,
    /// What went wrong.
    pub reason: ParseErrorReason,
    /// The production context, outermost first — rendered as the `in <context>`
    /// tail (`in type`, `in function parameters`). Curated: only the ~productions
    /// where a label aids the message push one, so the noise chumsky emitted
    /// (`context clause` / `generic arguments` after every type) never appears.
    pub context: Vec<&'static str>,
    /// A targeted hint for a known-confusing shape (§6a's first-class messages —
    /// e.g. the `!=` soup), recognized structurally at the failure, not by string
    /// matching. Rendered as the ` — <hint>` tail.
    pub hint: Option<&'static str>,
}

/// Why a parse failed — the content a message is built from.
#[derive(Clone, Debug)]
pub enum ParseErrorReason {
    /// "found <found>, expected <one of expected>" — the structured recursive-
    /// descent farthest-failure. `expected` is a curated set (never the optional-
    /// continuation noise chumsky merged in at every type position).
    Expected { found: Found, expected: Vec<String> },
    /// A curated message stating a language rule (diagnostics-standard.md B6 — the
    /// prohibition explains itself and names the sanctioned spelling). The
    /// misplaced-`resource` steer is the one case today.
    Rule(&'static str),
    /// The visibility-marker rule ([`visibility_marker_rule`]) — the one curated
    /// rule whose text quotes a word from the SOURCE, so it carries the marker
    /// instead of a finished `&'static str`. Only the two spellings
    /// [`Parser::visibility_marker_before`] recognizes ever reach here, which is
    /// what keeps the payload `'static`.
    VisibilityMarker { marker: &'static str },
    /// A spelling another language uses, written where vilan has its own
    /// (B520): the parse went on as if vilan's had been written.
    ForeignSpelling(ForeignSpelling),
    /// A declaration head whose markers are out of the ruled order (B486,
    /// B485 Q6/Q8): `canonical` is the head respelled in it. The parse went on
    /// as if it had been written so.
    MarkerOrder { canonical: String },
    /// A WARNING, never an error (B536): a declaration head whose attributes
    /// are written out of [`attribute_rank`]'s order, and nothing else out of
    /// order. `canonical` is the head respelled in it; the parse read it so.
    AttributeOrder { canonical: String },
    /// A statement ran out without its terminating `;` (`editing-dx.md` §4.4, S2).
    /// The span is the GAP — the last character of the token before the one that
    /// could not continue the statement — so the diagnostic sits where the `;`
    /// goes, not on the next statement's head.
    MissingTerminator,
    /// A delimited region that was opened and never closed before the enclosing
    /// block's `}` or end of input (`editing-dx.md` §5.3 — the defining mid-edit
    /// shape). The span is the OPENER the user typed; the closer they have not
    /// typed yet is what the message asks for.
    Unclosed { delimiter: char },
    /// An unbalanced / garbled delimited region recovered at `production` — one of
    /// the ten `nested_delimiters` sites. `delimiter` is the opening bracket; the
    /// span is the recovered region.
    Unbalanced {
        production: &'static str,
        delimiter: char,
    },
}

/// What the parser found at a failure: a token (rendered via its `Display`), an
/// un-lexable character (an S1 [`lexing::LexError`] fed in), or end of input.
#[derive(Clone, Debug)]
pub enum Found {
    Token(String),
    Character(char),
    EndOfInput,
}

/// What a member position holds right after its `.` ([`Parser::member_after_dot`],
/// R-k).
enum DotMember {
    /// A name written against the dot (or reported and read on the same line).
    Read,
    /// No member: the mid-edit `Error` member, silent at parse.
    Missing,
    /// A name stranded on the next line: the chain declines, the expectation
    /// noted.
    Decline,
}

/// What [`Parser::recover_missing_terminator`] did with the statement it read —
/// three outcomes where there used to be two, because "reported it" and "kept
/// it" are not the same decision.
enum TerminatorRecovery<'src> {
    /// Not the missing-`;` shape at all: nothing was reported, the cursor has
    /// not moved, and the caller's next recovery step applies.
    Declined,
    /// Reported and KEPT. The statement is real code the parser read perfectly
    /// well, and dropping it would unbind names the rest of the file uses —
    /// `recover_missing_terminator`'s whole argument.
    Kept(Spanned<Node<'src>>),
    /// Reported and DROPPED, because the statement IS the mistake. A
    /// `pub`/`public` visibility marker reads as a bare identifier expression
    /// that binds nothing, so keeping it handed the analyzer a name nothing
    /// declares and the one curated refusal arrived beside `cannot find 'pub'
    /// in this scope` — a cascade off a word already refused (E109's F10). The
    /// cursor has still advanced past the marker, so the item below it parses
    /// normally and the caller's loop cannot spin.
    Dropped,
}

/// The expectation a statement terminator records ([`Parser::note_terminator`]),
/// and the marker [`Parser::emit_failure`] reads to route a failure to the gap
/// anchor and the `;` message. Spelled like every other `expect_ctrl` expectation
/// so a set that mixes it with others still renders.
const TERMINATOR_EXPECTED: &str = "';'";

/// The expectation an unfinished `::` path records (E135). Spelled as an
/// expectation rather than as a curated rule because that is exactly what it is
/// — the path wanted one more name — and the "found X expected …" frame is what
/// puts the token standing in the name's place into the message.
const A_NAME_AFTER_PATH_SEPARATOR: &str = "a name after `::`";

/// The expectation a `::` path that crosses a line break records (E142).
///
/// E135 fixed the case where NOTHING follows the `::`; it could not fix the
/// worse one, where the next LINE follows it. `style::` ⏎ `print(..)` is the
/// perfectly legal path `style::print`, so the parser read the next statement
/// as the tail of this one and swallowed it, and no diagnostic could exist —
/// there was no error to report. Only a rule that a path may not cross a line
/// break turns that into something the parser can see, which is why this is a
/// rule and not a recovery.
///
/// Spelled as an expectation, like its E135 sibling, so the "found X expected
/// …" frame puts the name standing on the next line into the message; the
/// steer names both cures, because the reason to write a path over two lines
/// is that it is long, and a long path is what an alias is for.
const A_NAME_AFTER_PATH_SEPARATOR_ON_THIS_LINE: &str = "a name after `::` on the same line: a `::` path does not cross a line break, because `a::` \
     at the end of a line joins whatever the next line starts with — join the line, or import \
     the path under a shorter name (`import a::b::c as d;`) and write `d`";

/// The expectation a member `.` records when its name does not follow it
/// directly (B414 S4, R-k RULED 2026-09-29: "only `.{NAME}` is legal").
///
/// E142's argument, one token over: `value.` at the end of a line joined
/// whatever the next line started with, so a half-typed `list.` above
/// `helper();` was the member call `list.helper()` and no diagnostic could
/// exist. The member tier makes that worse rather than rare — every word is a
/// member name after `.` now, so `list.` above `let x = 1;` would read
/// `list.let` — and the owner's ruling is the strict form: nothing at all
/// between the dot and the name, on one line or across two. A chain written
/// over several lines breaks BEFORE the dot, which the rule leaves exactly as
/// it was.
///
/// Spelled as an expectation, like its E142 sibling, so the "found X expected
/// …" frame puts the stranded name into the message.
const A_MEMBER_NAME_AGAINST_ITS_DOT: &str = "a member name written against its `.` — `value.name`, with no space or line \
     break between them, because a `.` at the end of a line would join whatever the next line \
     starts with. A chain continued on the next line breaks BEFORE the dot, and the next line \
     begins `.name()`";

/// The rule a `then` form read as a VALUE without its `else` breaks (B459
/// Q3, RULED 2026-09-29: "no bare `then` in expression position"). Curated
/// (diagnostics-standard.md B6): a value needs both branches, exactly as an
/// `if` used for its value needs its `else`, and the bare form has one reading
/// — the statement — whose spelling the rule names.
const THEN_NEEDS_ITS_ELSE: &str = "a `then` used as a VALUE needs its `else`, as an `if` used for its value does — \
     `ready then a else b`. Without one, `ready then go();` is a STATEMENT, which stands where a \
     statement does and ends with its `;`";

/// The rule the guard read as a VALUE breaks (B459). Curated: `c else S;` has
/// no value to give — the branch it names runs when `c` is false and the
/// other one does not exist — so it is a statement or nothing.
const THE_GUARD_IS_A_STATEMENT: &str = "`value else S;` is the GUARD, a statement: it stands where a statement does \
     and ends with its `;` — a value needs both branches, `ready then a else b`";

/// The rule a `let` as a `then`/`else` branch breaks (B459 Q8). Curated: a
/// branch is ONE statement with no block of its own, so the binding would be
/// scoped to a block nobody wrote and read by nothing.
const A_BRANCH_BINDS_NOTHING: &str = "a `then`/`else` branch is one statement with no block of its own, so a `let` \
     there binds a name nothing can read — bind it before the form, or write an `if` with a block";

/// The rule `fun f(): || void context c` breaks (B343, R9 RULED 2026-09-17).
/// Curated (diagnostics-standard.md B6): the prohibition explains itself, and
/// the two ways out are the two things the author can actually have meant.
///
/// The clause's POSITION stays where contexts.md §3 put it — after the return
/// type — and this is the whole cost of keeping it there. A type's own
/// `context` suffix parses greedily, so an un-parenthesized closure return type
/// swallows the clause onto its OWN return type, which cannot carry one; the
/// declaration then means neither of the two things it could have meant. Both
/// are one parenthesis away, and both are spelled here rather than left to be
/// guessed at.
pub const MISBOUND_RETURN_CLAUSE: &str = "a `context` clause after an UN-PARENTHESIZED closure return type binds to the \
     closure's own return type, which cannot carry one. Parenthesize the closure type to say \
     which clause you mean: `fun f(): (|| void) context c` declares `c` for the FUNCTION, and \
     `fun f(): (|| void context d) context c` declares `d` for the closure that is RETURNED \
     and `c` for the function itself";

/// The rule a program written before the `css` promotion breaks. Curated
/// (diagnostics-standard.md B6 — the prohibition explains itself and names the
/// sanctioned spelling): `css` became a hard keyword with the `css { … }` block
/// (proposal/css-block.md §5.4, Q3 ruled 2026-08-28), so every place that wanted
/// a NAME and found the word is a member the promotion renamed out of its way.
/// Both renames are spelled here, because a bare "found `css`, expected an
/// identifier" would leave the reader to guess what their `.css` became.
const CSS_IS_A_KEYWORD: &str = "`css` is a keyword: it begins a `css { … }` block. The style values that used \
     to spell it were renamed out of its way — `Length::css(…)` is now \
     `Length::raw(…)`, and the `.css` field of a `Length` or a `Color` is now \
     `.text`";

/// The rule a `css` block in condition position breaks. Curated
/// (diagnostics-standard.md B6): the block is brace-initial, so it occupies the
/// same shape a struct literal does and is suppressed where one is — a `{`
/// after a bare head in an `if`/`for`/`match` head is the BLOCK
/// (proposal/css-block.md §4.2) — and the sanctioned spelling is the one a
/// struct literal takes there.
const CSS_BLOCK_IS_BRACE_INITIAL: &str = "a `css { … }` block is brace-initial, so it is suppressed in a condition, a `for … in` \
     iterable and a `match` subject, exactly as a struct literal is — parenthesize it: \
     `(css { … })`";

/// What a `css` block's body admits — the dot rule, spelled for the reader
/// (proposal/css-block.md §3), chain links included (A69).
const CSS_ITEM_EXPECTED: &str = "a declaration (`property(value);`), a nested rule (`.name { … }`) or a chain link \
     (`.name();`)";

/// The rule `const mut` breaks (G24). Curated (diagnostics-standard.md B6):
/// the prohibition explains itself and names both sanctioned spellings.
///
/// `const let` and `const fun` are the two compile-time declarations; `const
/// mut` reads as "a mutable compile-time binding", which is a contradiction —
/// the value IS the build's, there is no runtime storage for a mutation to
/// land in, and anything that wanted one wanted a runtime `mut` seeded from a
/// `const` expression.
const CONST_HAS_NO_MUTATION: &str = "a compile-time value has no runtime mutation: `const let` binds a value the BUILD \
     computes, and there is nowhere for a later write to go. Write `const let` for the \
     compile-time binding, or `mut name = const ..;` for a runtime binding seeded from one";

/// The rule `async x = 1;` and `async let x = 1;` break (B494). Curated
/// (diagnostics-standard.md B6): `async` marks a function or an expression
/// run as a task, never a binding, and the two spellings the author can have
/// meant are named. `async x = 1;` was read as an ASSIGNMENT whose place is
/// `async x` — refused only when `x` was unbound ("cannot find 'x'"), and
/// emitted as invalid JS when it was bound.
const ASYNC_MARKS_NO_BINDING: &str = "`async` does not mark a binding: it marks a function, `async fun load()`, or an \
     expression run as a task, `let pending = async load();` — a binding is `let name = …`";

/// The rule a CSS pseudo-class written CSS-style breaks (tracker E153).
/// Curated (diagnostics-standard.md B6): the prohibition explains itself and
/// names the sanctioned spelling.
///
/// `:hover { … }` is the single most likely thing for a CSS writer to type
/// inside a block, and it reported the bare `found ':' expected a declaration
/// …` — true, and no help at all, because the reader has to guess that the
/// answer is a DOT. The dotted rule is deliberate: one name-blind form covers
/// pseudo-classes, breakpoints, `within` and `divide`, so the grammar never
/// consults a method list. The message says that, and names the fix.
const CSS_PSEUDO_CLASS_IS_DOTTED: &str = "a `css` block writes a pseudo-class as a DOTTED rule: `.hover { … }`, not `:hover { … }`. \
     One name-blind form covers pseudo-classes, breakpoints (`.md`), ancestor guards \
     (`.within(…)`) and `.divide` — so the grammar never consults a method list, and a \
     method added to `Style` cannot change what a block means";

/// The rule a CSS-SPELLED declaration breaks (A101). Curated
/// (diagnostics-standard.md B6): the prohibition explains itself and names the
/// sanctioned spelling.
///
/// `property: value;` was the block's declaration form through Order 36 and is
/// what every CSS author types, so the migration and the newcomer hit the same
/// token — the `:` where a `(` belongs. A declaration is a CALL now: the
/// property is the name, the value is its ordinary vilan expression arguments,
/// and the `{ }` hole is gone because there is no token span left for one to
/// interrupt.
/// Public for the same reason [`IMPORTANT_HAS_NO_PLACE`] is: the language
/// server's quick fix keys on it (E201) — one constant rather than a second
/// copy to drift from.
pub const A_CSS_DECLARATION_IS_A_CALL: &str = "a `css` declaration is a CALL: write `padding(space(4));`, not `padding: space(4);`. The \
     property is the name and its value is ordinary vilan expressions, so a typed value needs \
     no `{ }` hole — and several arguments join with one space, as CSS's own value lists do \
     (`margin(px(4), px(8))`)";

/// The rule `!important` breaks inside a `css` block. Curated
/// (diagnostics-standard.md B6): the prohibition explains itself and names the
/// sanctioned spelling — which is "nothing at all", because a later
/// declaration on the same property already wins (proposal/css-block.md §10).
///
/// Public because the language server's quickfix keys on it (css-block S5,
/// §7.2 fix 3) — one constant rather than a second copy to drift from.
pub const IMPORTANT_HAS_NO_PLACE: &str = "`!important` has no place in a `css` block: a `Style` merges by record update, so a later \
     declaration on the same property already wins — remove it";

/// The rule `[doc(hidden)]` breaks (B318 §7.5, RULED 2026-09-13). Curated
/// (diagnostics-standard.md B6): the prohibition explains itself and names the
/// sanctioned spelling.
///
/// The marker's one purpose — "callable, but omitted from editor completion" —
/// is, word for word, what a PRIVATE item now is, so the two overlap completely
/// and one of them has to go. It is also the one that never worked: it parsed,
/// landed on `Function::doc_hidden`, was round-tripped by the formatter, was
/// pinned callable and was recommended by `appendix/editor.md`, and NOTHING in
/// `vilan-ide` or `vilan-lsp` ever read it — a promise the tool did not keep,
/// for as long as it existed.
const DOC_HIDDEN_IS_SUPERSEDED: &str = "`[doc(hidden)]` is superseded by visibility: an item its module does not `export` is already \
     reachable and absent from completion, which is the whole of what this marker meant — delete \
     it, and write `export` on the names consumers are meant to find. For an item that IS part of \
     the surface and is dangerous to reach for, `[internal(\"reason\")]` is the other thing this \
     marker is reached for: it stays exported and callable, and the editor hides it from \
     completion, dims it and leads its hover with the reason";

/// B415's placement rule: `mod self;` hosts the FILE's own attributes (F27
/// R1's `[platform(..)]` today), so it leads the file — the one place a reader
/// looks for what the whole file is. Curated: the rule states itself and names
/// the move that satisfies it.
pub const MODULE_SELF_LEADS_THE_FILE: &str = "`mod self;` carries the attributes of the whole file, so it is the file's first statement: \
     move it above the first import. To fence one function instead, write `[platform(..)]` on \
     the function";

/// B415's reserved name: `self` is the file's OWN module, the one `mod self;`
/// declares, so no nested module may take it. Curated: it names the one
/// legal `self` module and the move.
pub const MODULE_SELF_IS_RESERVED: &str = "`self` is reserved for the file's own module — `mod self;`, with no body, as the file's \
     first statement — so a nested module cannot take the name: give it another";

/// B382's rule: a `[deprecated]` steer on an import is about the NAME a
/// re-export publishes, so it needs the `export`. Curated: it names the move.
pub const DEPRECATED_IMPORT_IS_A_RE_EXPORT: &str = "`[deprecated(..)]` on an `import` deprecates the name a RE-EXPORT publishes, and this \
     import is not exported, so it publishes nothing — write `export` before the attribute, or \
     delete it";

/// The rule `export <expression>;` breaks (B321). Curated
/// (diagnostics-standard.md B6 — the prohibition explains itself and names the
/// sanctioned spellings).
///
/// [`Parser::parse_export`] takes any STATEMENT, and an expression statement is
/// one, so `export (helper);` and `export * helper;` (which is `export` of the
/// deref `*helper`) both compiled clean and published nothing — a form with no
/// reading, accepted silently. Zero occurrences in the estate.
const EXPORT_TAKES_AN_ITEM: &str = "`export` takes an ITEM — a `fun`, `struct`, `enum`, `trait`, `impl`, `mod`, a module-level \
     `let`, or an `import`/`use` to re-export (`export import pkg::io::print;`) — plus `*;` for \
     the whole module and a `(in PATH)` scope before any of them (`export(in pkg) fun f()`): an \
     expression is none of those, and publishes nothing, checks nothing and emits nothing";

/// B492: a second `export` on one declaration. The marker is a statement
/// WRAPPER (§3.2), so `export export fun f()` parsed as an export of an
/// export and was accepted silently, meaning exactly what one marker means.
/// Refused where it stands, and read past, so the declaration still parses
/// and the reader gets the one sentence.
const EXPORT_IS_WRITTEN_ONCE: &str = "`export` is written once: the declaration is already marked, \
     and a second `export` adds nothing — delete it";

/// The rule a MALFORMED import path breaks (B320). Curated
/// (diagnostics-standard.md B6 — the prohibition explains itself and names the
/// sanctioned spelling).
///
/// `import` and `use` are keywords, so neither can begin an expression, and the
/// statement fork that reads them sits at the END of
/// [`Parser::parse_statement_inner`]: an import whose PATH the grammar could not
/// read fell through to the expression attempt, whose farthest failure is
/// recorded on the `import` keyword itself. Every mistyped import in the
/// language therefore reported `found 'import' expected an expression` at
/// column 1 of the statement, whatever the typo and however far into the path it
/// sat (six probe shapes, identical output — `{ (impl T) }`, `::*`, `{ !name }`).
///
/// [`Parser::import_path_failure`] records how far the path grammar actually
/// got, so the rule reports at the token it stopped on. The expression fallback
/// is untouched for everything that is not import-led.
const IMPORT_PATH_IS_NAMES_AND_SETS: &str = "an `import`/`use` path is `::`-separated NAMES, ending in a name or a `{ a, b }` set, with an \
     optional `as` alias on the leaf — `import pkg::a::{ b, c as d };` — and this token begins \
     none of those";

/// REWRITTEN for B318 (§7.4): every sentence of the old text became false on
/// the day the marker gained meaning. It said "a module's items are importable
/// as they stand … so the fix is to delete the word", and the fix is not to
/// delete the word — it is to WRITE the marker vilan does have. This is the
/// message a Rust or Swift writer meets in their first hour, which makes it the
/// most user-visible line in the whole feature.
///
/// The rule a program written with a Rust/Swift visibility marker breaks.
/// Curated (diagnostics-standard.md B6 — the prohibition explains itself and
/// names the sanctioned spelling): `pub` is an ordinary identifier here, so
/// `pub fun helper()` reads as the expression statement `pub` followed by an
/// item, and the located failure is a missing `;` three columns in — true, and
/// useless. Vilan has no visibility marker to reach for: a module's items are
/// importable as written (E101).
///
/// The marker is quoted back from the source rather than fixed at `pub`:
/// `public` is the same reflex one synonym over, and it was being refused in a
/// word its author never wrote (E109's F21). That is the only reason this is a
/// function where every other curated rule is a constant.
/// Whether `export` can take this statement (B321): an ITEM, an `import`/`use`,
/// or another `export`. The attribute wrappers are transparent — they annotate
/// the item under them and `export [derive(Wire)] struct S { .. }` is the same
/// declaration — so they are asked about their inner node rather than admitted
/// blindly.
///
/// [`Node::Error`] is admitted: it is the nesting bound's stand-in, already
/// refused once, and a second message about the same input is the double-report
/// diagnostics-standard B5 forbids.
fn export_takes(node: &Node<'_>) -> bool {
    match node {
        // G24's `const let` / `const fun` is the same kind of wrapper (N89): it
        // marks WHEN the declaration under it runs, not what kind of statement
        // it is, and `export const fun answer(): i32 { 42 }` publishes exactly
        // the function a bare `const fun` declares. Asked about its inner node
        // for the attribute wrappers' reason — `const (1 + 1)` is an expression
        // and is still refused, by the same test one level down.
        Node::Derive(_, inner)
        | Node::Service(_, inner)
        | Node::MacroAttribute(_, _, _, inner)
        | Node::Const(inner) => export_takes(&inner.0),
        Node::Func(_)
        | Node::MacroFun(_)
        | Node::MacroInvocation(..)
        | Node::MacroBlock(_)
        | Node::Struct(..)
        | Node::Enum(..)
        | Node::Trait(..)
        | Node::Impl(..)
        | Node::Module(..)
        | Node::Import(..)
        | Node::Use(_)
        | Node::Export(..)
        | Node::ExportAll
        | Node::ModulePlatform(_)
        | Node::Let(..)
        | Node::LetDestructure(..)
        | Node::Error => true,
        _ => false,
    }
}

fn visibility_marker_rule(marker: &str) -> String {
    format!(
        "`{marker}` is not a vilan keyword: the marker is `export`, so write \
         `export fun helper()` — an item a module does not export is the module's own, and \
         stays reachable to anyone who asks for it deliberately \
         (`import pkg::util::{{ #helper }};`). (`export` also RE-exports something this module \
         imported: `export import pkg::io::panic;`.)"
    )
}

/// A spelling another language uses for something vilan writes differently
/// (B520, R-h RULED 2026-10-03): `return` for `ret`, `fn`/`function`/`func`/
/// `def` for `fun`, and the `->` arrow for the `:` before a return type.
///
/// None of the four words is reserved — each stays an ordinary name wherever
/// it is one today (`let return = 1;`, a field `fn: i32`, `fun def()`). The
/// parser recognizes them only where no name can stand, so no valid program
/// reads differently:
///
/// - `return` at the head of an expression, followed by a token that BEGINS
///   an expression and cannot CONTINUE one after a name — another name, a
///   literal, `if`/`match`/`await`/`const`/`css`/`async`
///   ([`starts_foreign_return`]). Two names side by side are never an
///   expression, so `return x` has no other reading. `return;`, `return
///   (x)` and `return -x` DO have one — a read of a binding named `return`, a
///   call, a subtraction — and are left to the analyzer;
/// - `fn`/`function`/`func`/`def` at an item head (past any attribute run and
///   marker keywords), followed by a name and the `(` or `<` that opens a
///   signature ([`Parser::take_foreign_item_word`]);
/// - `->`, the two tokens written against each other, where a return type's
///   `:` may stand: after a `fun`'s parameter list, a closure literal's, and
///   a closure type's. A `-` followed directly by `>` is never an expression.
///
/// Each reports ONE diagnostic at the foreign token — "vilan spells this
/// `ret`" — and the parse goes on as if the right spelling had been written:
/// the word's token is rewritten in place (to `ret` or `fun`), and the arrow
/// is read as the `:`. So the declaration or the return is in the tree, and
/// nothing after it cascades.
///
/// **The editor's half.** [`ForeignSpelling::code`] is the diagnostic's stable
/// code, [`ForeignSpelling::of_message`] recognizes the diagnostic from its
/// rendered text (the one field a diagnostic carries through the pipeline),
/// and [`foreign_spelling_fix`] is the quick fix's edit: the span to replace
/// and the text to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ForeignSpelling {
    /// `return value;` — vilan writes `ret value;`.
    Return,
    /// `fn name(..)` (Rust) — vilan writes `fun`.
    Fn,
    /// `function name(..)` (JavaScript, Lua, PHP) — vilan writes `fun`.
    Function,
    /// `func name(..)` (Go, Swift) — vilan writes `fun`.
    Func,
    /// `def name(..)` (Python, Ruby, Scala) — vilan writes `fun`.
    Def,
    /// `fun name() -> T` (Rust, Swift, Python's annotations) — vilan writes
    /// `fun name(): T`, and `|x| -> T body` on a closure literal is `|x|: T
    /// body`.
    Arrow,
    /// `|i32| -> str` in a closure TYPE — vilan writes the result directly
    /// after the `|..|`, `|i32| str`, so the fix deletes the arrow rather
    /// than writing a `:`.
    TypeArrow,
}

/// The `fun` steer's sentence for one written word — one literal per word, so
/// each message is a fixed string the diagnostics ledger can key on.
macro_rules! fun_spelling_steer {
    ($word:literal) => {
        concat!(
            "`",
            $word,
            "` is not a vilan keyword: vilan spells this `fun` — `fun name(parameter: Type): \
             Result { … }`"
        )
    };
}

impl ForeignSpelling {
    /// Every foreign spelling, in a fixed order.
    pub const ALL: [ForeignSpelling; 7] = [
        ForeignSpelling::Return,
        ForeignSpelling::Fn,
        ForeignSpelling::Function,
        ForeignSpelling::Func,
        ForeignSpelling::Def,
        ForeignSpelling::Arrow,
        ForeignSpelling::TypeArrow,
    ];

    /// The foreign spelling as the author wrote it.
    pub fn written(self) -> &'static str {
        match self {
            ForeignSpelling::Return => "return",
            ForeignSpelling::Fn => "fn",
            ForeignSpelling::Function => "function",
            ForeignSpelling::Func => "func",
            ForeignSpelling::Def => "def",
            ForeignSpelling::Arrow | ForeignSpelling::TypeArrow => "->",
        }
    }

    /// The spelling vilan uses in its place — the quick fix's replacement
    /// text for the foreign token (empty for a closure type's arrow, which
    /// vilan does not write at all).
    pub fn vilan(self) -> &'static str {
        match self {
            ForeignSpelling::Return => "ret",
            ForeignSpelling::Fn
            | ForeignSpelling::Function
            | ForeignSpelling::Func
            | ForeignSpelling::Def => "fun",
            ForeignSpelling::Arrow => ":",
            ForeignSpelling::TypeArrow => "",
        }
    }

    /// The diagnostic's STABLE code: `foreign-spelling/<what was written>`,
    /// with the arrow named `arrow` (`type-arrow` in a closure type). The editor publishes it as the LSP
    /// diagnostic's `code` and keys its quick fix on it; it never changes
    /// when the message is reworded.
    pub fn code(self) -> &'static str {
        match self {
            ForeignSpelling::Return => "foreign-spelling/return",
            ForeignSpelling::Fn => "foreign-spelling/fn",
            ForeignSpelling::Function => "foreign-spelling/function",
            ForeignSpelling::Func => "foreign-spelling/func",
            ForeignSpelling::Def => "foreign-spelling/def",
            ForeignSpelling::Arrow => "foreign-spelling/arrow",
            ForeignSpelling::TypeArrow => "foreign-spelling/type-arrow",
        }
    }

    /// The diagnostic's text.
    pub fn message(self) -> &'static str {
        match self {
            ForeignSpelling::Return => {
                "`return` is not a vilan keyword: vilan spells this `ret` — `ret value;` returns \
                 a value, and a bare `ret;` leaves a function that returns nothing"
            }
            ForeignSpelling::Fn => fun_spelling_steer!("fn"),
            ForeignSpelling::Function => fun_spelling_steer!("function"),
            ForeignSpelling::Func => fun_spelling_steer!("func"),
            ForeignSpelling::Def => fun_spelling_steer!("def"),
            ForeignSpelling::Arrow => {
                "`->` is not how vilan writes a return type: vilan spells this `:` — `fun \
                 name(parameter: Type): Result`, and `|parameter: Type|: Result` on a closure"
            }
            ForeignSpelling::TypeArrow => {
                "`->` is not how vilan writes a closure type: its result follows the `|..|` \
                 directly — `|Type| Result`, and `|| Result` with no parameters"
            }
        }
    }

    /// The quick fix's title.
    pub fn fix_title(self) -> &'static str {
        match self {
            ForeignSpelling::Return => "Write `ret`",
            ForeignSpelling::Fn
            | ForeignSpelling::Function
            | ForeignSpelling::Func
            | ForeignSpelling::Def => "Write `fun`",
            ForeignSpelling::Arrow => "Write `:` for the return type",
            ForeignSpelling::TypeArrow => "Remove the `->`",
        }
    }

    /// The foreign spelling a diagnostic's rendered message reports, if it
    /// reports one. The parser renders these with no context and no hint, so
    /// the message is exactly [`ForeignSpelling::message`].
    pub fn of_message(message: &str) -> Option<ForeignSpelling> {
        Self::ALL
            .into_iter()
            .find(|spelling| message == spelling.message())
    }

    /// The `fun` steer's word, by its text.
    fn item_word(word: &str) -> Option<ForeignSpelling> {
        match word {
            "fn" => Some(ForeignSpelling::Fn),
            "function" => Some(ForeignSpelling::Function),
            "func" => Some(ForeignSpelling::Func),
            "def" => Some(ForeignSpelling::Def),
            _ => None,
        }
    }
}

/// The quick fix for a foreign-spelling diagnostic (B520): the span to
/// replace and the text to write there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellingFix {
    /// The diagnostic's stable code ([`ForeignSpelling::code`]).
    pub code: &'static str,
    /// The quick fix's title ([`ForeignSpelling::fix_title`]).
    pub title: &'static str,
    /// The source range the edit replaces.
    pub span: Span,
    /// What the edit writes there.
    pub replacement: &'static str,
}

/// The quick fix for the diagnostic `message` anchored at `span` in `source`,
/// or `None` when it is not a foreign-spelling diagnostic (B520).
///
/// A word's fix replaces the word: its span IS the diagnostic's. The arrow's
/// also takes the blanks written between the parameter list and the `->`, so
/// `fun f() -> i32` becomes `fun f(): i32` — the `:` against the `)`, the way
/// the formatter prints it — rather than `fun f() : i32`. A closure type's
/// arrow is deleted with the blanks AFTER it, so `|i32| -> str` becomes
/// `|i32| str`.
pub fn foreign_spelling_fix(source: &str, message: &str, span: Span) -> Option<SpellingFix> {
    const BLANKS: [char; 4] = [' ', '\t', '\r', '\n'];
    let spelling = ForeignSpelling::of_message(message)?;
    let (mut start, mut end) = (span.start, span.end);
    match spelling {
        ForeignSpelling::Arrow => {
            start = source.get(..span.start)?.trim_end_matches(BLANKS).len();
        }
        ForeignSpelling::TypeArrow => {
            let after = source.get(span.end..)?;
            end += after.len() - after.trim_start_matches(BLANKS).len();
        }
        _ => {}
    }
    Some(SpellingFix {
        code: spelling.code(),
        title: spelling.fix_title(),
        span: Span::from(start..end),
        replacement: spelling.vilan(),
    })
}

/// A154 (ruled 2026-10-03): the std modules that moved under a namespace, each
/// old path (under `std::`) with the path that replaced it. The ONE table: the
/// analyzer's refusal of an old import or qualified path, the manifest's refusal
/// of an old `prelude` value and the editor's quick fix all read it, so no two of
/// them can disagree about where a module went. There are no forwarding modules
/// (the ruling's (2)): an old path resolves to nothing and is refused with this.
///
/// `web` is the web PRELUDE's old path — `std::web` itself is a namespace now,
/// so its row is consulted only where a path stops AT `web` or reaches a name
/// the namespace does not hold (see the analyzer's import walk).
pub const MOVED_STD_MODULES: &[(&str, &str)] = &[
    ("dom", "web::dom"),
    ("ui", "web::ui"),
    ("style", "web::style"),
    ("dev", "web::dev"),
    ("router", "web::router"),
    ("storage", "web::storage"),
    ("document", "web::document"),
    ("asset", "web::asset"),
    ("web", "web::prelude"),
    ("hash_map_cell", "reactive::hash_map_cell"),
    ("hash_set_cell", "reactive::hash_set_cell"),
    ("transient", "reactive::transient"),
    ("store", "reactive::store"),
    ("store_core", "reactive::store_core"),
    ("delta", "reactive::delta"),
    ("null", "js::null"),
    ("promise", "js::promise"),
    ("native_map", "js::native_map"),
    ("rpc_server", "rpc::server"),
];

/// The path [`MOVED_STD_MODULES`] gives the old std module `old` (the segment
/// after `std::`), or `None` when `old` did not move.
pub fn moved_std_module(old: &str) -> Option<&'static str> {
    MOVED_STD_MODULES
        .iter()
        .find(|(module, _)| *module == old)
        .map(|(_, new)| *new)
}

/// The stable code of the moved-module refusal ([`moved_std_module_message`]).
/// The editor publishes it as the LSP diagnostic's `code`.
pub const MOVED_STD_MODULE_CODE: &str = "std-path/moved";

/// The refusal for a path through the moved std module `old` (A154). The text
/// is its whole contract: [`moved_std_module_fix`] reads the two paths back out
/// of it, so its head is fixed.
pub fn moved_std_module_message(old: &str, new: &str) -> String {
    format!(
        "`std::{old}` moved to `std::{new}`: std's modules are grouped under namespaces since \
         v0.44.0, and the old path is gone — write `std::{new}`"
    )
}

/// The `(old, new)` pair a moved-module refusal names, when `message` is one.
pub fn moved_std_module_of_message(message: &str) -> Option<(&'static str, &'static str)> {
    let rest = message.strip_prefix("`std::")?;
    let (old, rest) = rest.split_once('`')?;
    let rest = rest.strip_prefix(" moved to `std::")?;
    let (new, _) = rest.split_once('`')?;
    MOVED_STD_MODULES
        .iter()
        .find(|(module, moved)| *module == old && *moved == new)
        .map(|(module, moved)| (*module, *moved))
}

/// The stable code of a std-path diagnostic `message`, if it is one.
pub fn std_path_diagnostic_code(message: &str) -> Option<&'static str> {
    moved_std_module_of_message(message).map(|_| MOVED_STD_MODULE_CODE)
}

/// The quick fix for a moved-module refusal: the span to replace and the text
/// to write there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StdPathFix {
    /// The diagnostic's stable code ([`MOVED_STD_MODULE_CODE`]).
    pub code: &'static str,
    /// The quick fix's title: "Write `std::web::dom`".
    pub title: String,
    /// The source range the edit replaces: the OLD module segment, or, for a
    /// brace list under the old web-prelude path that also names `std::web`'s
    /// own children, the stretch from its first prelude name to its last.
    pub span: Span,
    /// What the edit writes there: the new path below `std::` (`web::dom`), or
    /// that stretch with `prelude::` written before each prelude name.
    pub replacement: String,
}

/// The quick fix for the diagnostic `message` anchored at `span` in `source`
/// (A154), or `None` when it is not a moved-module refusal, `span` no longer
/// covers the old segment (the buffer moved on), or the import is a shape no
/// one edit rewrites correctly ([`moved_std_module_edit`] says which and why).
pub fn moved_std_module_fix(source: &str, message: &str, span: Span) -> Option<StdPathFix> {
    moved_std_module_edit(source, message, span)?.ok()
}

/// The edit that answers a moved-module refusal (A154) — the ONE computation the
/// editor's quick fix and `vilan check --fix` both apply, so the two cannot
/// write different paths for the same refusal. `None` when `message` is not a
/// moved-module refusal or `span` no longer covers the old segment;
/// `Some(Err(reason))` when the import is a shape the fix leaves to a person,
/// with the reason in words.
///
/// The refusal anchors at the old module's own segment — `dom` in
/// `import std::dom::create_element;`, `web` in `std::web::Signal` — so the edit
/// replaces exactly that segment with the new path below `std::`, and whatever
/// the path continues with (`::create_element`, a brace list, an alias) stays.
///
/// One shape needs more (E268): a brace list under the old web-prelude path
/// that also names one of `std::web`'s own children — `std::web::{ Signal,
/// dom::create_element }`. `std::web` is that namespace now, so `dom::..`
/// already resolves there; rewriting `web` to `web::prelude` would carry it
/// along and break it. The edit instead writes `prelude::` before each name
/// that is not a child: `std::web::{ prelude::Signal, dom::create_element }`.
/// The children are read off the one table (every module that moved to
/// `web::<child>`), so a name that is not one is a prelude name — or a typo,
/// which stays an ordinary miss under `prelude::` exactly as it does under
/// `std::web::prelude::{ .. }`.
pub fn moved_std_module_edit(
    source: &str,
    message: &str,
    span: Span,
) -> Option<Result<StdPathFix, &'static str>> {
    let (old, new) = moved_std_module_of_message(message)?;
    if source.get(span.into_range())? != old {
        return None;
    }
    let (tree, _errors) = parse(source);
    // E269: where the old module is itself the LEAF of the import —
    // `import std::web;`, which bound the old web prelude as `web` — the new
    // path's last segment would bind a different name (`prelude`, or `server`
    // for `rpc_server`) and every `web::..` use after it would stop resolving.
    // The edit keeps the binding's name with an alias. An aliased leaf
    // (`import std::web as w;`) already names its binding, and a path that
    // continues past the segment binds what it continues to.
    let leaf = tree
        .as_ref()
        .and_then(|(nodes, _)| import_tail_at(nodes, span))
        .is_some_and(|tail| matches!(tail, ImportTail::Leaf));
    let renamed = new.rsplit("::").next() != Some(old);
    let whole = || {
        let written = match leaf && renamed {
            true => format!("{new} as {old}"),
            false => new.to_string(),
        };
        StdPathFix {
            code: MOVED_STD_MODULE_CODE,
            title: format!("Write `std::{written}`"),
            span,
            replacement: written,
        }
    };
    if old != "web" {
        return Some(Ok(whole()));
    }
    let Some(elements) = tree
        .as_ref()
        .and_then(|(nodes, _)| web_brace_list_at(nodes, span))
    else {
        // `std::web::Signal`, `std::web::{ .. }` read as no import (a parse
        // the tree does not hold): the segment alone is the edit.
        return Some(Ok(whole()));
    };
    let mut prelude_names = Vec::new();
    let mut names_a_child = false;
    // E269: a `self` element bound the old prelude module as `web` (or as its
    // alias): it becomes `prelude as web` (`prelude`, under its alias).
    let mut selves: Vec<(Span, bool)> = Vec::new();
    for element in elements {
        match element {
            ImportBranch::Path("self", self_span, tail) => {
                selves.push((*self_span, matches!(tail, ImportTail::Leaf)));
            }
            ImportBranch::Path(name, ..) if is_web_namespace_child(name) => names_a_child = true,
            ImportBranch::Path(name, name_span, _) => prelude_names.push((*name, *name_span)),
            ImportBranch::Reach(..) | ImportBranch::Selector(..) | ImportBranch::Set(..) => {
                return Some(Err(MOVED_WEB_MARKED_REASON));
            }
        }
    }
    if selves.is_empty() && (!names_a_child || prelude_names.is_empty()) {
        return Some(Ok(whole()));
    }
    // Each prelude name gets `prelude::` before it and each `self` becomes the
    // prelude itself; a child of `std::web` stays where it resolves. One edit
    // over the stretch from the first rewritten element to the last.
    let mut edits: Vec<(Span, String)> = prelude_names
        .iter()
        .map(|(_, name_span)| {
            (
                Span::from(name_span.start..name_span.start),
                "prelude::".to_string(),
            )
        })
        .collect();
    edits.extend(selves.iter().map(|(self_span, leaf)| {
        let written = match leaf {
            true => "prelude as web",
            false => "prelude",
        };
        (*self_span, written.to_string())
    }));
    edits.sort_by_key(|(edit_span, _)| edit_span.start);
    let first = edits.first()?.0.start;
    let last = edits.last()?.0.end;
    let mut replacement = String::new();
    let mut cursor = first;
    for (edit_span, written) in &edits {
        replacement.push_str(source.get(cursor..edit_span.start)?);
        replacement.push_str(written);
        cursor = edit_span.end;
    }
    let title = match (selves.is_empty(), prelude_names.is_empty()) {
        (false, true) => {
            "Write `prelude as web` for `self` (the web prelude is `std::web::prelude`)".to_string()
        }
        (false, false) => "Write `prelude as web` for `self` and `prelude::` before the prelude's \
             names (the web prelude is `std::web::prelude`)"
            .to_string(),
        (true, _) => {
            let names = prelude_names
                .iter()
                .map(|(name, _)| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!("Write `prelude::` before {names} (the web prelude is `std::web::prelude`)")
        }
    };
    Some(Ok(StdPathFix {
        code: MOVED_STD_MODULE_CODE,
        title,
        span: Span::from(first..last),
        replacement,
    }))
}

/// Why [`moved_std_module_edit`] leaves a marked brace list under the old
/// web-prelude path to a person: a reach marker (`#name`), an impl selector
/// (`(impl T)`) or a nested `{ .. }` group binds no plain name the edit can
/// prefix with `prelude::`.
pub const MOVED_WEB_MARKED_REASON: &str = "the brace list holds a reach marker, an impl selector      or a nested group, which the edit cannot prefix with `prelude::`; move those names under      `std::web::prelude` by hand";

/// Whether `name` is one of `std::web`'s own children — every module the one
/// table moved to `web::<name>`, the web prelude among them.
fn is_web_namespace_child(name: &str) -> bool {
    MOVED_STD_MODULES
        .iter()
        .filter_map(|(_, new)| new.strip_prefix("web::"))
        .any(|child| child == name)
}

/// What follows the import segment spanning exactly `anchor` — the end of the
/// statement, an alias, or more path — wherever the import sits. `None` when
/// no import holds such a segment.
fn import_tail_at<'tree, 'src>(
    nodes: &'tree NodeList<'src>,
    anchor: Span,
) -> Option<&'tree ImportTail<'src>> {
    fn in_branch<'tree, 'src>(
        branch: &'tree ImportBranch<'src>,
        anchor: Span,
    ) -> Option<&'tree ImportTail<'src>> {
        match branch {
            ImportBranch::Path(_, span, tail) if *span == anchor => Some(tail),
            ImportBranch::Path(_, _, ImportTail::Continue(next)) => in_branch(next, anchor),
            ImportBranch::Path(..) | ImportBranch::Selector(..) => None,
            ImportBranch::Set(elements) => elements
                .iter()
                .find_map(|element| in_branch(element, anchor)),
            ImportBranch::Reach(_, inner) => in_branch(inner, anchor),
        }
    }
    fn in_node<'tree, 'src>(
        node: &'tree Spanned<Node<'src>>,
        anchor: Span,
    ) -> Option<&'tree ImportTail<'src>> {
        if !(node.1.start <= anchor.start && anchor.end <= node.1.end) {
            return None;
        }
        if let Node::Import(branch, _) | Node::Use(branch) = &node.0 {
            return in_branch(branch, anchor);
        }
        let mut found = None;
        node.0.for_each_child(&mut |child| {
            if found.is_none() {
                found = in_node(child, anchor);
            }
        });
        found
    }
    nodes.iter().find_map(|node| in_node(node, anchor))
}

/// The elements of the brace list that follows the import segment spanning
/// exactly `anchor` (`std::web::{ .. }`), wherever the import sits — at the
/// top of a file or block-scoped. `None` when that segment is not followed by
/// a brace list (or no import holds it).
fn web_brace_list_at<'tree, 'src>(
    nodes: &'tree NodeList<'src>,
    anchor: Span,
) -> Option<&'tree [ImportBranch<'src>]> {
    fn in_branch<'tree, 'src>(
        branch: &'tree ImportBranch<'src>,
        anchor: Span,
    ) -> Option<&'tree [ImportBranch<'src>]> {
        match branch {
            ImportBranch::Path(_, span, ImportTail::Continue(next)) if *span == anchor => {
                match next.as_ref() {
                    ImportBranch::Set(elements) => Some(elements),
                    _ => None,
                }
            }
            ImportBranch::Path(_, _, ImportTail::Continue(next)) => in_branch(next, anchor),
            ImportBranch::Path(..) | ImportBranch::Selector(..) => None,
            ImportBranch::Set(elements) => elements
                .iter()
                .find_map(|element| in_branch(element, anchor)),
            ImportBranch::Reach(_, inner) => in_branch(inner, anchor),
        }
    }
    fn in_node<'tree, 'src>(
        node: &'tree Spanned<Node<'src>>,
        anchor: Span,
    ) -> Option<&'tree [ImportBranch<'src>]> {
        if !(node.1.start <= anchor.start && anchor.end <= node.1.end) {
            return None;
        }
        match &node.0 {
            Node::Import(branch, _) | Node::Use(branch) => return in_branch(branch, anchor),
            _ => {}
        }
        let mut found = None;
        node.0.for_each_child(&mut |child| {
            if found.is_none() {
                found = in_node(child, anchor);
            }
        });
        found
    }
    nodes.iter().find_map(|node| in_node(node, anchor))
}

/// A157: the warning on a WRITTEN `autofocus` attribute in an element head
/// (`<input autofocus />`, which lowers to `.attr("autofocus", "")`). Raised by
/// the analyzer's `check_written_autofocus`, at the attribute's NAME; an
/// explicit `.attr("autofocus", ..)` is not steered (it is how a `<dialog>` or
/// a popover gets the native attribute). Fixed text, no slots: the editor
/// recognizes the diagnostic by it ([`written_autofocus_fix`]).
pub const WRITTEN_AUTOFOCUS_MESSAGE: &str = "a written `autofocus` attribute is the browser's \
     native one, which acts only while the page is first parsed: on an element inserted later \
     the document refuses it once something has focus, and Chromium logs \"Autofocus processing \
     was blocked because a document already has a focused element\". Write `.autofocus()` — it \
     focuses the element once it is in the document, an enclosing focus scope starts on it, and a \
     server render still writes the native attribute (for a `<dialog>` or a popover that wants \
     the native one, write `.attr(\"autofocus\", \"\")`)";

/// The element-head ATTRIBUTE names the analyzer steers to a `View` method
/// instead (A157's `autofocus`, at [`WRITTEN_AUTOFOCUS_MESSAGE`]), each with
/// the method it steers to. The HTML attribute table is name-blind and keeps
/// them; completion reads this to offer the method and not the attribute that
/// would warn on the next analysis (E264).
pub const STEERED_ELEMENT_ATTRIBUTES: &[(&str, &str)] = &[("autofocus", "autofocus")];

/// [`WRITTEN_AUTOFOCUS_MESSAGE`]'s STABLE code. The editor publishes it as the
/// LSP diagnostic's `code`; it never changes when the message is reworded.
pub const WRITTEN_AUTOFOCUS_CODE: &str = "element-attribute/autofocus";

/// The stable code of an element-syntax diagnostic `message`, if it is one.
pub fn element_diagnostic_code(message: &str) -> Option<&'static str> {
    (message == WRITTEN_AUTOFOCUS_MESSAGE).then_some(WRITTEN_AUTOFOCUS_CODE)
}

/// The quick fix for an element-syntax diagnostic: the span to replace and the
/// text to write there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ElementFix {
    /// The diagnostic's stable code ([`WRITTEN_AUTOFOCUS_CODE`]).
    pub code: &'static str,
    /// The quick fix's title.
    pub title: &'static str,
    /// The source range the edit replaces.
    pub span: Span,
    /// What the edit writes there.
    pub replacement: &'static str,
}

/// The quick fix for the diagnostic `message` anchored at `span` in `source`
/// (A157), or `None` when it is not [`WRITTEN_AUTOFOCUS_MESSAGE`], `span` no
/// longer covers the word `autofocus` (the buffer moved on), or the attribute
/// carries a value other than `("")` — `autofocus(flag)` is a value the method
/// form has no slot for, so the warning stands without an edit.
///
/// The edit replaces the attribute — the bare name, or the name through its
/// `("")` — with `.autofocus()` IN PLACE: a dotted link may stand anywhere in
/// a head, and the formatter's element-head order treats it as a barrier, so
/// the result is already what `vilan fmt` prints.
pub fn written_autofocus_fix(source: &str, message: &str, span: Span) -> Option<ElementFix> {
    if message != WRITTEN_AUTOFOCUS_MESSAGE || source.get(span.into_range())? != "autofocus" {
        return None;
    }
    let after = source.get(span.end..)?;
    let rest = after.trim_start();
    let end = if let Some(inside) = rest.strip_prefix('(') {
        let close = inside.find(')')?;
        if inside[..close].trim() != "\"\"" {
            return None;
        }
        span.end + (after.len() - rest.len()) + 1 + close + 1
    } else {
        span.end
    };
    Some(ElementFix {
        code: WRITTEN_AUTOFOCUS_CODE,
        title: "Write `.autofocus()`",
        span: Span::from(span.start..end),
        replacement: ".autofocus()",
    })
}

/// Whether the identifier `return` at `index` begins a FOREIGN return (B520):
/// the token after it begins an expression and cannot continue one after a
/// name. Anything else after the word — `;`, `}`, `(`, `-`, `.`, `=`, an
/// operator, `then` — reads the NAME `return` as it always did.
fn starts_foreign_return(tokens: &[Spanned<Token<'_>>], index: usize) -> bool {
    if tokens.get(index).map(|(token, _)| token) != Some(&Token::Ident("return")) {
        return false;
    }
    matches!(
        tokens.get(index + 1).map(|(token, _)| token),
        Some(
            Token::Number(..)
                | Token::String(_)
                | Token::MultilineString(_)
                | Token::Bool(_)
                | Token::Null
                | Token::If
                | Token::Match
                | Token::Await
                | Token::Const
                | Token::Css
                | Token::Async
        )
    ) || matches!(
        tokens.get(index + 1).map(|(token, _)| token),
        // `then` is the one name that continues an operand (B459's infix
        // conditional): `return then go();` tests a binding named `return`.
        Some(Token::Ident(name)) if *name != "then"
    )
}

/// Whether `fn`/`function`/`func`/`def` at `index` heads a foreign item
/// (B520): followed by a name and the `(` or `<` that opens a signature —
/// [`Parser::take_foreign_item_word`]'s shape at a statement head, for the
/// recovery's sync points.
fn starts_foreign_item(tokens: &[Spanned<Token<'_>>], index: usize) -> bool {
    let token = |offset: usize| tokens.get(index + offset).map(|(token, _)| token);
    matches!(token(0), Some(Token::Ident(word)) if ForeignSpelling::item_word(word).is_some())
        && matches!(token(1), Some(Token::Ident(name)) if *name != "then")
        && matches!(token(2), Some(Token::Ctrl('(' | '<')))
}

/// Whether the two tokens at `index` are the `->` arrow (B520): a `-` written
/// directly against a `>`, with nothing between them.
fn is_foreign_arrow(tokens: &[Spanned<Token<'_>>], index: usize) -> bool {
    match (tokens.get(index), tokens.get(index + 1)) {
        (Some((Token::Op("-"), minus)), Some((Token::Ctrl('>'), greater))) => {
            minus.end == greater.start
        }
        _ => false,
    }
}

/// The rule an impl selector written OUTSIDE a brace set breaks (B318 S3,
/// `proposal/visibility.md` §2.5). Curated (diagnostics-standard.md B6): the
/// prohibition explains itself and names the sanctioned spelling.
///
/// The selector is a brace-set ELEMENT and nothing else — it binds no name, so
/// it has no leaf position to occupy, and `import a::(impl T);` would read as a
/// statement whose whole payload is a filter. Without this the `(` falls
/// through both halves of the path grammar and the statement reports at column
/// one, which is the failure mode §2.5 says a build of this slice must not
/// reproduce.
const IMPL_SELECTOR_IS_A_BRACE_ELEMENT: &str = "an `impl` selector is a brace-set ELEMENT, because it selects \
     implementations rather than binding a name: write `import a::{ (impl T) };`, and put any names \
     it travels with in the same set — `import a::{ Thing, (impl Thing) };`";

/// The shape an `(impl …)` selector must have (B318 S3). Curated
/// (diagnostics-standard.md B6): stated at the token the selector stopped on,
/// so a typo inside one reports where it is rather than at the `import`
/// keyword (`proposal/visibility.md` §2.5's first pinned grammar fact).
const IMPL_SELECTOR_SHAPE: &str = "an `impl` selector is `(impl TYPE)`, optionally followed by \
     `::name` or `::{ a, b }`: `import a::{ (impl List<i32>)::{ first, last } };`. `_` stands for any \
     type in an argument position (`(impl List<_>)`), and `(impl _)` selects every implementation the \
     module declares";

/// The rule a binder written inside a selector breaks (B318, RULED
/// 2026-09-12: "no binders are written in a selector"). Curated
/// (diagnostics-standard.md B6): the prohibition explains itself and names the
/// sanctioned spelling, which is the placeholder.
///
/// A selector is a FILTER over blocks that are already generic; it introduces
/// nothing of its own, so a `type T` in one would name a parameter no body can
/// read, and a bound on `_` would ask for the block whose bound is that one —
/// a selection with no exhibit, which is why `_` covers the whole design.
const IMPL_SELECTOR_TAKES_NO_BINDER: &str = "an `impl` selector writes no binders: it filters blocks that \
     declare their own, so there is nothing for a `type T` to name. Write `_` for a position that may \
     be any type — `(impl List<_>)` selects every `List` block, whatever its element and whatever \
     bound it carries";

/// The rule `as` on an impl selector breaks (B318, RULED: "a method selector
/// refuses `as` — methods are called by name on a receiver"). Curated
/// (diagnostics-standard.md B6).
const IMPL_SELECTOR_REFUSES_AS: &str = "an `impl` selector takes no `as`: a method is called by NAME on a \
     receiver, so renaming one would produce a name nothing can call. `import a::Length::rem` binds a \
     self-less function as a free name and takes an alias; `import a::{ (impl Length)::rem }` puts the \
     member in `Length`'s namespace for this file, and that name is the member's own";

/// The rule an `(impl …)` selector inside a `use` breaks (B318 S3). Curated
/// (diagnostics-standard.md B6): a selector says which of a MODULE's `impl`
/// blocks this file admits, and a `use` reaches no module — it destructures a
/// namespace already in scope. The two productions share a brace set, so the
/// selector parses there and would otherwise do nothing at all, silently.
const USE_TAKES_NO_IMPL_SELECTOR: &str = "an `impl` selector belongs to `import`: it says which of a \
     MODULE's implementations this file admits, and a `use` reaches no module — it destructures a \
     namespace this file already has. Write `import <module>::{ (impl T) };`";

/// The rule `use … only;` breaks (B318 §2.4). Curated
/// (diagnostics-standard.md B6): `only` subtracts the implementations an
/// `import` brings, and a `use` never brought any — it destructures a
/// namespace that is already in scope — so the word would subtract nothing and
/// read as if it did.
const USE_TAKES_NO_ONLY: &str = "`only` belongs to `import`: it drops the implementations an import \
     brings along its path, and a `use` brings none — it destructures a namespace this file already \
     reaches. Delete the word";

/// The rule a block-like form followed by an operator or a `.` chain breaks
/// (B248, widened by B259). Curated (diagnostics-standard.md B6): the
/// prohibition explains itself — a block-like form is COMPLETE — and names the
/// sanctioned spelling, which is the parentheses B231 already admits the form
/// inside. One sentence for every position, because the mistake and the fix are
/// the same in all of them: B259 found the statement head was only where the
/// refusal FIRED, not where the rule applies.
const BLOCK_LIKE_STATEMENT_IS_COMPLETE: &str = "a `match`, `if`, `for` or `{` form is COMPLETE at its \
     closing brace, so the operator or `.` after it does not continue it — in statement position it \
     begins a new statement instead: parenthesize the block-like form — `(match x { .. }) + 1`, \
     `(match x { .. }).to_str()` — to use its value as an operand. (The rule is what lets a line \
     beginning `-x` or `*p` mean subtraction or a dereference by where it sits.)";

/// The rule a block-like form followed by `::` breaks (E157). Its own rule
/// rather than [`BLOCK_LIKE_STATEMENT_IS_COMPLETE`], because that message's
/// STEER is a fix for every other continuation and is not one here: `::` is the
/// only one with no valid spelling at all. `(match x { .. }) + 1` and
/// `(match x { .. }).to_str()` both parse — B231 admits the form inside
/// parentheses — and `(match x { .. })::foo` does not, because `::` reaches
/// into a NAMESPACE and what stands to its left is a name rather than a value.
/// Offering the parentheses there sent the author to a second parse error.
/// Curated (diagnostics-standard.md B6): the prohibition explains itself, and
/// the two ways out are the two things the author can actually have meant.
const A_PATH_CANNOT_START_AT_A_BLOCK: &str = "`::` reaches into a NAMESPACE — a module, a type or an enum — so what \
     stands to its left has to be a NAME, and a `match`, `if`, `for` or `{` form is a value: \
     parentheses do not help here, the way they do for an operator or a `.` chain after one \
     (`(match x { .. }) + 1`). Write the path on its own if the block-like form ended the \
     statement before it, or bind the form first — `let value = match x { .. };` — and reach \
     for the member through the value.";

/// The rule `let mut x = …` breaks. Curated (diagnostics-standard.md B6): `let`
/// and `mut` are the two BINDING FORMS, not a keyword and a modifier on it, so
/// the pair is a Rust spelling with no reading here — and the failure it
/// produces ("found 'let' expected a statement") names the one token that was
/// right (E101).
const OWN_IS_A_PARAMETER_CONVENTION: &str = "`own` is a PARAMETER convention — `fun take(own list: List<T>)` — and has no \
     reading here: a `let` already owns its value, and a type is written without it";

const DYN_IS_A_TYPE_MARKER: &str = "`dyn` marks a trait object in TYPE position — `let shape: dyn Shape = circle;` — and \
     has no reading in an expression: a value becomes a trait object where it meets that type";

const LAZY_IS_WRITTEN_AT_THE_DECLARATION: &str = "`lazy` is written where the deferral is DECLARED — `lazy let name: T = …;` at \
     module level, or `lazy name: T` on a parameter — and a call passes the argument plainly";

const LET_MUT_IS_ONE_WORD: &str = "a mutable binding is spelled `mut x = …`: `let` and `mut` are the two binding forms, \
     not a keyword and a modifier — `let` binds immutably, `mut` binds mutably, and \
     writing both is neither";

/// The rule `Some(let mut x)` and `Some(mut let x)` break — [`LET_MUT_IS_ONE_WORD`]'s
/// twin inside a PATTERN (A80). Curated (diagnostics-standard.md B6): a pattern
/// binder follows the declaration syntax exactly, so `let` and `mut` are the two
/// forms there too and the steer is the pattern spelling the author wanted. Its own
/// constant rather than a second use of the declaration's, because that one's steer
/// (`mut x = …`, with an initializer) is not a thing you can write in a pattern.
///
/// D7: and it names what `Some(mut list)` then MEANS. The author who reaches for
/// a mutable binder is usually growing a collection inside a `SignalCell::update`
/// — the shape A80 was filed from — and the binder is a binding, so it takes rule
/// 1's copy: the spelling the steer offers still leaves the subject alone without
/// a write-back. Saying "`mut x`" and stopping sends them one step down a path
/// that ends where they started.
const PATTERN_BINDER_IS_ONE_WORD: &str = "a pattern binds mutably with `mut x`: `let` and `mut` are the two binding forms \
     inside a pattern exactly as they are in a declaration, not a keyword and a \
     modifier — `Some(let list)` binds immutably, `Some(mut list)` binds mutably, and \
     writing both is neither. A binder is a BINDING, so `Some(mut list)` binds a \
     copy: to change the subject, assign back through it (`held = Some(list)`) or \
     use `take`/`replace`";

/// The did-you-mean note for a failure INSIDE an interpolation hole. A `{` in an
/// `i"…"` opens a hole, so a literal brace has to be escaped — and code that
/// GENERATES braces (a CSS rule, a JS body, a JSON object) hits this constantly,
/// with a message about an expression it never wrote (E101).
const BRACE_IN_AN_ISTRING: &str = "a `{` inside an `i\"…\"` string opens an interpolation hole, so this is being read as \
     an expression — write `\\{` (and `\\}`) for a literal brace";

/// A keyword that declares an ITEM — `fun`/`struct`/…, plus the `external`
/// modifier that leads one. An item is never part of an expression, so
/// [`Parser::scan_to_sync_point`] may stop at one even inside a delimited region it
/// is skipping (a `{` above it excepted: a block or closure body holds ordinary
/// statements, and a nested `fun` is one of them).
/// Whether a RETURN type can carry a `context` clause of its own (B309): a
/// closure type, under the `(..)` grouping the clause's grammar needs and under
/// an `async` / `sync` marker.
///
/// This is the one disambiguation the two clause readings need. `fun f(): i32
/// context settings` declares what the BODY may read (B242) — `i32` cannot
/// carry a clause, so nothing is lost by binding it to the function. `fun f():
/// (|| View) context owner_scope` returns an INJECTED closure, and binding that
/// clause to the function would quietly mean something else entirely.
fn return_type_carries_its_own_clause(node: &Node<'_>) -> bool {
    let grouped = match node {
        Node::Tuple(elements) if elements.len() == 1 => &elements[0].0,
        other => other,
    };
    matches!(
        grouped,
        Node::ClosureType(..) | Node::AsyncType(..) | Node::SyncType(..)
    )
}

/// B343 (R9, RULED 2026-09-17) — the ONE shape the "clause after the return
/// type" position cannot spell, reported instead of mis-bound.
///
/// `fun f(): || void context c` has three readings and the grammar takes the
/// one nobody means: `parse_type` is greedy, so `context c` lands on the
/// CLOSURE'S OWN return type `void`, which cannot carry a clause at all. The two
/// readings a writer could have meant are both a parenthesis away — `(|| void)
/// context c` gives the clause to the FUNCTION, `(|| void context c)` gives it
/// to the closure that is returned — and contexts.md §3's position (after the
/// return type) is kept for every other shape, so this is the whole cost of
/// keeping it.
///
/// Takes the clause off as it reports it, so the analyzer does not refuse the
/// same mistake a second time with its own ("a `context` clause is only
/// supported on a closure type") — one mistake, one diagnostic, and the rest of
/// the signature reads exactly as it would have without the clause.
///
/// Answers `false` for every legal shape, including the nested one: `fun f(): ||
/// (|| void) context c` returns a closure that returns an INJECTED closure, and
/// the clause is that inner closure type's — [`return_type_carries_its_own_
/// clause`] is the same predicate the peel below uses, so the two readings are
/// decided in one place.
fn take_misbound_return_clause(node: &mut Node<'_>) -> bool {
    let closure = match node {
        Node::AsyncType(inner) | Node::SyncType(inner) => &mut inner.0,
        other => other,
    };
    let Node::ClosureType(_, Some(returns)) = closure else {
        return false;
    };
    let Node::TypeWithContexts(inner, _) = &returns.0 else {
        return false;
    };
    if return_type_carries_its_own_clause(&inner.0) {
        return false;
    }
    let Node::TypeWithContexts(inner, _) = std::mem::replace(&mut returns.0, Node::Error) else {
        unreachable!("just matched");
    };
    **returns = *inner;
    true
}

/// E233 — a declaration's return type `&T context c` (or `&mut T …`) as the
/// peel in [`Parser::parse_function`] expects it: the clause lifted off the
/// view's target onto the view, so `TypeWithContexts(&T, c)` and not
/// `&TypeWithContexts(T, c)`.
///
/// The `&` production parses a whole TYPE after it, clause suffix included,
/// so the clause a writer put after `&i32` lands on `i32`, where it means
/// nothing — the analyzer refused it ("a `context` clause is only supported on
/// a closure type") and the formatter, finding no function clause to print,
/// reprinted the written order. A target that carries its own clause (a
/// closure type, B309) keeps it: `&(|| View) context owner` is a view of an
/// injected closure. Every other shape comes back unchanged.
fn hoist_clause_out_of_a_view(annotation: Spanned<Node<'_>>) -> Spanned<Node<'_>> {
    let (node, span) = annotation;
    let Node::Reference(mutable, target) = node else {
        return (node, span);
    };
    let (target, target_span) = hoist_clause_out_of_a_view(*target);
    match target {
        Node::TypeWithContexts(inner, names) if !return_type_carries_its_own_clause(&inner.0) => {
            let view_span = Span::from(span.start..inner.1.end);
            (
                Node::TypeWithContexts(
                    Box::new((Node::Reference(mutable, inner), view_span)),
                    names,
                ),
                span,
            )
        }
        other => (
            Node::Reference(mutable, Box::new((other, target_span))),
            span,
        ),
    }
}

/// Whether `node` is a `then`/`else` form (B459) — the one `Node::If` that
/// is not block-like: it ends at an expression, not at a `}`, so as a
/// statement it takes the `;` every expression statement takes.
fn is_then_form(node: &Node<'_>) -> bool {
    matches!(
        node,
        Node::If(NodeIfBranch::If(if_)) if matches!(if_.spelling, IfSpelling::Then { .. })
    )
}

/// B459: a `then`/`else` form terminated at STATEMENT position, re-read as
/// the statement it is (Q6) — each branch a statement whose value is
/// discarded, so the branches need not unify: `c then f() else g();` is
/// `if c { f(); } else { g(); }`. A branch that is itself a form is at
/// statement position too, so the reading recurses (`a then b then f();`,
/// and the `else`-chain `a then f() else b then g() else h();`). Anything
/// else comes back unchanged.
fn read_as_statement(node: Spanned<Node<'_>>) -> Spanned<Node<'_>> {
    let (Node::If(NodeIfBranch::If(mut if_)), span) = node else {
        return node;
    };
    let IfSpelling::Then {
        then_word,
        else_word,
        statement: false,
    } = if_.spelling
    else {
        return (Node::If(NodeIfBranch::If(if_)), span);
    };
    if_.spelling = IfSpelling::Then {
        then_word,
        else_word,
        statement: true,
    };
    if_.then = branch_as_statement(if_.then);
    if_.else_ = if_.else_.map(|(branch, branch_span)| match branch {
        NodeIfBranch::Else(block) => (NodeIfBranch::Else(branch_as_statement(block)), branch_span),
        chained => (chained, branch_span),
    });
    (Node::If(NodeIfBranch::If(if_)), span)
}

/// One branch body of [`read_as_statement`]: `{ a }` becomes `{ a; }`, the
/// tail moved into the statements (itself re-read) and the tail left `Void`
/// at the branch's last character — where an `if` block's own `Void` sits, on
/// its `}`. An empty branch (the guard's `then`) is already a statement.
fn branch_as_statement<'src>(
    block: Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)>,
) -> Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)> {
    let ((mut statements, tail), span) = block;
    if matches!(tail.0, Node::Void) {
        return ((statements, tail), span);
    }
    let end = tail.1.end;
    statements.push(read_as_statement(*tail));
    let void_span = Span::from(end.saturating_sub(1)..end);
    ((statements, Box::new((Node::Void, void_span))), span)
}

/// The `then`/`else` forms still read as VALUES once the parse is done
/// (B459): each needs both branches (Q3), and the guard, which has one,
/// is a statement only. Run over the finished tree, because a form's reading
/// is its enclosing statement's to decide — `a then b then f();` is a
/// statement whose inner form, without its own `else`, is legal only once the
/// outer one has been read as a statement.
fn refuse_then_forms_read_as_values(root: &Spanned<NodeList<'_>>, errors: &mut Vec<ParseError>) {
    fn visit(node: &Spanned<Node<'_>>, errors: &mut Vec<ParseError>) {
        if let Node::If(NodeIfBranch::If(if_)) = &node.0
            && let IfSpelling::Then {
                then_word,
                else_word,
                statement: false,
            } = if_.spelling
        {
            let refusal = match (then_word, else_word) {
                (None, Some(word)) => {
                    Some((word, ParseErrorReason::Rule(THE_GUARD_IS_A_STATEMENT)))
                }
                (Some(word), None) => Some((word, ParseErrorReason::Rule(THEN_NEEDS_ITS_ELSE))),
                _ => None,
            };
            if let Some((span, reason)) = refusal {
                errors.push(ParseError {
                    span,
                    reason,
                    context: Vec::new(),
                    hint: None,
                });
            }
        }
        node.0.for_each_child(&mut |child| visit(child, errors));
    }
    for statement in &root.0 {
        visit(statement, errors);
    }
}

/// Whether `token` is one of the RESERVED words (`lexing::KEYWORDS`) — the/// Whether `token` is one of the RESERVED words (`lexing::KEYWORDS`) — the
/// words the lexer hands back as their own token rather than as a name. The
/// member tier (B414 S4) admits them wherever a member name stands; a
/// contextual keyword needs no admitting, since it lexes as an identifier.
fn is_reserved_word(token: &Token<'_>) -> bool {
    lexing::KEYWORDS.iter().any(|(_, keyword)| keyword == token)
}

fn starts_item(token: &Token<'_>) -> bool {
    matches!(
        token,
        Token::Fun
            | Token::Struct
            | Token::Enum
            | Token::Impl
            | Token::Trait
            | Token::Mod
            | Token::Import
            | Token::Use
            | Token::Export
            | Token::Macro
            | Token::External
    )
}

/// A keyword that can only begin a fresh statement or item — the token-class half
/// of `frontend.md:137-140`'s sync points ("statement/item boundaries synchronize
/// on `;`/`}`/item keywords"), used by [`Parser::scan_to_sync_point`].
///
/// Item heads and statement heads (`let`/`mut`/`ret`/`jump`/`if`/`for`/`match`/
/// `const`/`async`) are both here, because both are places a recovering parse can
/// pick up cleanly. Identifiers and literals are deliberately NOT: they begin an
/// expression statement, but they also appear all through a broken one, so
/// stopping at them would resume mid-garbage and report again (the cascade
/// `editing-dx.md` §9 records vilan as not having). B520's foreign heads are the
/// exception, by shape rather than by word: `fn name(` and `return value` are
/// two names (or a name and a literal) side by side, which no broken statement
/// is made of either, and a `pub fn f()` reaches the visibility rule through
/// here.
fn starts_statement_or_item(tokens: &[Spanned<Token<'_>>], index: usize) -> bool {
    let Some((token, _)) = tokens.get(index) else {
        return false;
    };
    starts_item(token)
        || starts_contextual_statement(tokens, index)
        || starts_foreign_item(tokens, index)
        || starts_foreign_return(tokens, index)
        || matches!(
            token,
            Token::Let
                | Token::Mut
                | Token::Ret
                | Token::If
                | Token::For
                | Token::Match
                | Token::Const
                | Token::Async
        )
}

/// Whether the CONTEXTUAL keyword at `index` begins a statement — the LL(2)
/// test of `proposal/contextual-keywords.md` §2 at a statement head (B414):
/// `jump` followed by its target (a name) is the jump; `lazy` followed by
/// `let`/`mut` is the lazy binding, and followed by a name it is the
/// missing-binder-word recovery `parse_let` reports (two juxtaposed names are
/// never an expression, so nothing else could read it). Anything else after
/// either word — `jump.height`, `lazy = 3;`, `lazy.force()` — is an expression
/// over a NAME. One predicate for the dispatch in
/// [`Parser::parse_secondary_inner`] and the recovery sync points, so the two
/// can never disagree about what a statement head is.
fn starts_contextual_statement(tokens: &[Spanned<Token<'_>>], index: usize) -> bool {
    let next = tokens.get(index + 1).map(|(token, _)| token);
    match tokens.get(index).map(|(token, _)| token) {
        Some(Token::Ident("jump")) => matches!(next, Some(Token::Ident(_))),
        Some(Token::Ident("lazy")) => {
            matches!(next, Some(Token::Let | Token::Mut | Token::Ident(_)))
        }
        _ => false,
    }
}

/// The placement rule for a contextual keyword written, before a name, where
/// its keyword reading is not admitted (B414) — or `None` for a word that has
/// no such misreading (`with`/`borrows` are positional: the typo cases read the
/// same before and after the demotion).
fn misplaced_contextual_keyword(word: &str) -> Option<ParseErrorReason> {
    match word {
        "own" => Some(ParseErrorReason::Rule(OWN_IS_A_PARAMETER_CONVENTION)),
        "dyn" => Some(ParseErrorReason::Rule(DYN_IS_A_TYPE_MARKER)),
        "lazy" => Some(ParseErrorReason::Rule(LAZY_IS_WRITTEN_AT_THE_DECLARATION)),
        _ => None,
    }
}

/// The closing bracket that matches an opening one — for the `Unbalanced` message.
fn matching_close(open: char) -> char {
    match open {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        '<' => '>',
        other => other,
    }
}

/// Render one parse error as user-facing text, following the diagnostics standard
/// (`proposal/diagnostics-standard.md`): "found X expected Y in <context> — <hint>"
/// for the structured case, the curated rule verbatim, and an unclosed-delimiter
/// message for a genuinely garbled region. Wired into the pipeline's fold sites at
/// the S5 cutover (`analyze_source`, the module loader, macro expansion, the CLI
/// report), replacing chumsky's `render_parse_error`.
///
/// No noise-filtering is needed (chumsky's renderer dropped the ever-present
/// `context clause` / `generic arguments` expectations): the farthest-failure
/// records only real expectations, so an over-eager optional continuation is never
/// in the set to begin with.
pub fn render(error: &ParseError) -> String {
    use std::fmt::Write;

    let mut message = match &error.reason {
        ParseErrorReason::Rule(rule) => rule.to_string(),
        ParseErrorReason::VisibilityMarker { marker } => visibility_marker_rule(marker),
        ParseErrorReason::ForeignSpelling(spelling) => spelling.message().to_string(),
        ParseErrorReason::MarkerOrder { canonical } => marker_order_rule(canonical),
        ParseErrorReason::AttributeOrder { canonical } => attribute_order_rule(canonical),
        ParseErrorReason::MissingTerminator => "expected `;` to end this statement".to_string(),
        ParseErrorReason::Unclosed { delimiter } => format!(
            "unclosed `{delimiter}`: expected a matching `{}`",
            matching_close(*delimiter)
        ),
        ParseErrorReason::Unbalanced {
            production,
            delimiter,
        } => {
            format!(
                "unclosed `{delimiter}` in {production}: expected a matching `{}`",
                matching_close(*delimiter)
            )
        }
        ParseErrorReason::Expected { found, expected } => {
            let found = match found {
                Found::Token(text) => format!("'{text}'"),
                Found::Character(character) => format!("'{}'", character.escape_debug()),
                Found::EndOfInput => "end of input".to_string(),
            };
            let mut message = format!("found {found} expected ");
            match expected.as_slice() {
                [] => message.push_str("something else"),
                [only] => message.push_str(only),
                [first, second] => write!(message, "{first} or {second}").unwrap(),
                many => {
                    for one in &many[..many.len() - 1] {
                        write!(message, "{one}, ").unwrap();
                    }
                    write!(message, "or {}", many[many.len() - 1]).unwrap();
                }
            }
            message
        }
    };
    for label in &error.context {
        write!(message, " in {label}").unwrap();
    }
    if let Some(hint) = error.hint {
        write!(message, "; {hint}").unwrap();
    }
    message
}

/// Parse `source` into its statement list (with spans) and any diagnostics. For a
/// CLEAN source the tree is the same `Spanned<NodeList>` the chumsky parser produces
/// (byte-identical, spans included — the S3 differential) and the error list is
/// empty. For a BROKEN source (S4) the parser RECOVERS: the `nested_delimiters`
/// sites produce their placeholders, a declined statement is reported and skipped
/// past at the next `;`/`}`/item boundary (`editing-dx.md` S1) instead of stopping
/// the parse, the `.`/`?.` member cases and the `resource`
/// steer synchronize — so a tree ALWAYS comes back (like chumsky's
/// `into_output_errors`), covering the WHOLE file rather than the prefix before its
/// first syntax error, alongside a non-empty error list. The clean-or-decline
/// contract the differential relies on is therefore the ERROR LIST (empty ⇒ clean),
/// not a missing tree.
///
/// (This is a deliberate improvement chumsky's top-level parse does not make — it is
/// all-or-nothing, discarding the whole tree on any leftover; see
/// `tests/parse_recovery_differential.rs`'s divergence ledger. Rendering the errors
/// is [`render`], exported for the S5 cutover, not wired into the pipeline here.)
///
/// The returned tree borrows `source` (identifiers, string bodies, and numeric
/// slices are `&'src str` copied out of the tokens), exactly like the chumsky
/// parser; the intermediate token vector does not outlive this call.
pub fn parse(source: &str) -> (Option<Spanned<NodeList<'_>>>, Vec<ParseError>) {
    let (tree, errors, _) = parse_with(source, false);
    (tree, errors)
}

/// [`parse`], and the parse's WARNINGS beside its errors (B536): what the
/// parser accepted and reads as written in canonical form, and still reports
/// — today, a declaration's attributes written out of [`attribute_rank`]'s
/// order ([`ParseErrorReason::AttributeOrder`]). A warning never makes a
/// source unclean: the tree is the one the canonical spelling parses to.
///
/// The pipelines that REPORT diagnostics read this (the entry analysis, the
/// CLI, the module loader, the clean-parse cache); every other reader of a
/// tree keeps calling [`parse`].
pub fn parse_with_warnings(source: &str) -> ParsedWithWarnings<'_> {
    parse_with(source, false)
}

/// What [`parse_with_warnings`] returns: the tree, the errors, the warnings.
pub type ParsedWithWarnings<'src> = (
    Option<Spanned<NodeList<'src>>>,
    Vec<ParseError>,
    Vec<ParseError>,
);

/// [`parse`], but every parenthesized expression is RECORDED as a
/// [`Node::LiftGroup`] node instead of dissolving into its inner expression.
///
/// This is the **formatter's** parse mode and nothing else's: `vilan fmt`
/// reprints from the tree, so a group the tree does not record is a group the
/// reprint cannot reproduce — the re-lex safety net then sees an output missing
/// two tokens and the whole file silently bails (`let b = (1 + 2);` did exactly
/// that). User-written parentheses are preserved rather than adjudicated, so
/// recording every group is precisely what the formatter needs.
///
/// The main pipeline (analyzer, macro expansion, emission, the CLI, the language
/// server's analysis) keeps calling [`parse`] and keeps seeing the dissolved
/// tree, so nothing downstream changes. That separation is what the corpus
/// byte-gate and `tests/parse_differential.rs` guard.
pub fn parse_preserving_groups(source: &str) -> (Option<Spanned<NodeList<'_>>>, Vec<ParseError>) {
    let (tree, errors, _) = parse_with(source, true);
    (tree, errors)
}

fn parse_with(source: &str, preserve_paren_groups: bool) -> ParsedWithWarnings<'_> {
    let (mut tokens, lex_errors) = lexing::tokenize(source);
    let token_count = tokens.len();

    let mut parser = Parser::new(&mut tokens, source, preserve_paren_groups);
    let root = parser.parse_program();
    let mut then_form_errors = Vec::new();
    refuse_then_forms_read_as_values(&root, &mut then_form_errors);
    debug_assert_eq!(
        parser.position, token_count,
        "the statement/item synchronizer consumes the whole token stream: an \
         unparseable statement is reported and skipped past, never left over",
    );

    // The lexer never discards its stream (S1): un-lexable characters are skipped
    // and reported, and the parser recovers over the surviving tokens. So a tree
    // always comes back — clean, or recovered from delimiter/character errors —
    // exactly as chumsky's `into_output_errors` returns `Some(tree)` alongside its
    // diagnostics. The clean-or-decline contract the differential relies on is now
    // expressed by the ERROR LIST (empty ⇒ clean), not by a missing tree.
    let mut errors: Vec<ParseError> = lex_errors
        .iter()
        .map(|error| ParseError {
            span: (error.position..error.position + error.character.len_utf8()).into(),
            // A lexer error that knows WHICH rule the character broke states it;
            // the rest render as the generic "found X expected a token".
            reason: match error.rule {
                Some(rule) => ParseErrorReason::Rule(rule),
                None => ParseErrorReason::Expected {
                    found: Found::Character(error.character),
                    expected: vec!["a token".to_string()],
                },
            },
            context: Vec::new(),
            hint: None,
        })
        .collect();
    errors.append(&mut parser.errors);
    errors.append(&mut then_form_errors);
    // The depth bound's refusal (B142), which was held off `parser.errors` so
    // that `attempt` could not roll it back — see `Parser::nesting_refusal`. It
    // sorts into place with the rest below.
    errors.extend(parser.nesting_refusal.take());
    // B520's rewrites, held off `parser.errors` for the same reason.
    errors.append(&mut parser.rewrite_refusals);
    // A stable, span-ordered diagnostic list (diagnostics-standard.md C1): lexer
    // errors and recovered-region errors interleave by where they occur.
    errors.sort_by_key(|error| (error.span.start, error.span.end));
    let mut warnings = std::mem::take(&mut parser.warnings);
    warnings.sort_by_key(|warning| (warning.span.start, warning.span.end));

    (Some(root), errors, warnings)
}

/// The spans of `source`'s CONTEXTUAL keywords (B414) where the parser read
/// them AS keywords — `with` in `impl A with B`, and not the method
/// `list.with(..)`; `own` in `fun f(own x: T)`, and not the parameter
/// `own: Owner`. The editor's raw-parse read (contextual-keywords.md §6, Q5):
/// the lexer cannot answer, because it hands every contextual word back as an
/// identifier, and the analyzed program cannot either, because a keyword
/// binds no entity. Sorted, without duplicates; a recovered parse answers for
/// what it recovered.
pub fn contextual_keyword_readings(source: &str) -> Vec<Span> {
    let (mut tokens, _) = lexing::tokenize(source);
    let mut parser = Parser::new(&mut tokens, source, false);
    parser.parse_program();
    let mut indices = std::mem::take(&mut parser.contextual_readings);
    indices.sort_unstable();
    indices.dedup();
    indices
        .into_iter()
        .filter_map(|index| tokens.get(index).map(|(_, span)| *span))
        .collect()
}

/// The spans of `source`'s RESERVED words the parser read as MEMBER names
/// (B414 S4) — `type` in `event.type` or in a field `type: str`, `match` in an
/// `impl`'s `fun match(self)`. The editor's raw-parse read, the member tier's
/// twin of [`contextual_keyword_readings`]: a reserved word lexes as its own
/// token, so a keyword hover would otherwise answer for every one of them.
/// Sorted, without duplicates.
pub fn keyword_member_readings(source: &str) -> Vec<Span> {
    let (mut tokens, _) = lexing::tokenize(source);
    let mut parser = Parser::new(&mut tokens, source, false);
    parser.parse_program();
    let mut indices = std::mem::take(&mut parser.member_readings);
    indices.sort_unstable();
    indices.dedup();
    indices
        .into_iter()
        .filter_map(|index| tokens.get(index).map(|(_, span)| *span))
        .collect()
}

struct Parser<'a, 'src> {
    /// The token stream, in SOURCE order but for one rewrite: an attribute run
    /// written ahead of `export` is rotated behind the marker as the statement
    /// is reached ([`Parser::lead_export_past_its_attributes`], B445), which is
    /// why the parser holds it mutably. Every token keeps its own span.
    tokens: &'a mut [Spanned<Token<'src>>],
    position: usize,
    /// The source the tokens index into — read only to place the **gap anchor** of
    /// a missing statement terminator on the last CHARACTER of the preceding token
    /// ([`Parser::gap_span`]), which needs a char boundary, not a byte offset.
    source: &'src str,
    /// The end-of-input offset (`source.len()`), the span the chumsky parser reports
    /// at EOI — `.map((end..end).into(), …)` in every call site.
    eoi: usize,
    /// Recovered-region and steer errors, in the order produced. A failed
    /// [`Parser::attempt`] rolls these back with the cursor (a backtracked
    /// alternative emits nothing), so a recovery error survives exactly when the
    /// enclosing parse path that produced it survives — chumsky's "errors on the
    /// successful branch are kept" behavior.
    errors: Vec<ParseError>,
    /// Per token position, whether an assignment operator is still REACHABLE as
    /// the operator of an assignment whose place starts there: one occurs later
    /// at the same bracket depth, before the enclosing bracket group closes and
    /// before a `;` ends the statement.
    ///
    /// [`Parser::parse_assignment`] speculatively parses a whole precedence
    /// chain to discover whether an assignment operator follows it, and throws
    /// that chain away when none does. Because it is tried before the operator
    /// tower — which then parses the SAME text again — every expression that
    /// re-enters `parse_expression` inside a bracket pays for its own subtree
    /// TWICE, once per level: `C(n) = 2·C(n-1)`, i.e. 2^n for nested
    /// parentheses or array literals. `(1 + (1 + …))` at 20 levels took 9.0s in
    /// the parser alone (B140), which is why nested arithmetic looked like an
    /// analyzer bug — the analyzer's own phases stayed flat.
    ///
    /// A place is a BALANCED token run, so its operator is always at the same
    /// bracket depth the place started at; if no such operator is reachable the
    /// attempt cannot succeed and is skipped, and the tower's parse is the only
    /// one. Computed once per parse in one backward pass, read in O(1).
    ///
    /// `,` is deliberately NOT a barrier even though it separates arguments:
    /// generic arguments (`::<A, B>`) put a comma at bracket depth zero inside
    /// a place, so barring it there would skip a real assignment. Missing a
    /// barrier only costs a declined attempt; a wrong one loses a parse.
    assignment_reachable: Vec<bool>,
    /// The farthest point any attempt reached before it could not proceed — the
    /// location and (curated) expectations for the top-level decline diagnostic.
    /// The standard recursive-descent heuristic: a speculative alternative that
    /// fails at a shallower position is overwritten once the parser advances past
    /// it, so what remains is where parsing genuinely got stuck. Purely
    /// diagnostic — it never affects the parsed tree, so the clean parse stays
    /// byte-identical to chumsky's.
    farthest_failure: Option<Failure>,
    /// The live production-context stack (`in type`, `in function parameters`),
    /// snapshotted when a new farthest failure is recorded.
    context_stack: Vec<&'static str>,
    /// Record EVERY parenthesized expression as a [`Node::LiftGroup`] rather than
    /// dissolving the ones that carry no lift mark — the formatter's parse mode
    /// (see [`parse_preserving_groups`]). Read at exactly one site,
    /// [`Parser::parse_paren_atom`]; false everywhere the compiler proper parses.
    preserve_paren_groups: bool,
    /// Inside a `trait` body or an `impl` ITEM LIST — so the function being
    /// parsed is a MEMBER, reached by dispatch rather than named directly.
    /// [`Parser::parse_function`] reads it to refuse a spread parameter there
    /// (variadic-generics.md §S.7), and clears it for the body it then parses:
    /// a `fun` declared inside a member's body is a free function.
    in_member_body: bool,
    /// Whether the statement about to be read is the FILE's first (B415): the
    /// one position `mod self;` — the host of the file's platform — may stand
    /// in. Set by
    /// [`Parser::parse_program`] before its first statement and TAKEN by the
    /// first [`Parser::parse_statement_inner`] that runs — so a statement nested
    /// inside that first one (a function body's, a `mod`'s) already sees it
    /// false.
    file_head: bool,
    /// How many levels of SOURCE NESTING are open, against
    /// [`Parser::NESTING_DEPTH_LIMIT`] (B142) — the parser's own bound, the
    /// companion to the analyzer's `WALK_DEPTH_LIMIT` and `RETURN_DEPTH_LIMIT`.
    /// Counted by [`Parser::parse_nested_as`], the one funnel every nesting
    /// grammar descends through: expressions, types, binders and patterns,
    /// items, import paths and elements all draw on this single counter, so it
    /// bounds the parser's TOTAL recursion depth rather than any one family's.
    nesting_depth: usize,
    /// The nesting bound's diagnostic, held aside rather than pushed into
    /// `errors` — set once and never overwritten, so a 5000-deep nest reports
    /// ONCE and not 4500 times. [`parse_with`] folds it into the error list at
    /// the end.
    ///
    /// Held aside because it must survive [`Parser::attempt`], which truncates
    /// `errors` when a branch declines. Refusing hands the enclosing rule a
    /// stand-in where it wanted an operand, so that rule's `)` is missing
    /// and it declines — and the refusal would be rolled back with it, leaving
    /// a file that was refused for depth reporting only a puzzled "expected
    /// `)`". This is exactly `farthest_failure`'s argument, and it is kept the
    /// same way: how deep the input actually went is a fact about the INPUT, not
    /// a claim by whichever branch happened to be exploring when it got there.
    nesting_refusal: Option<ParseError>,
    /// How far the `import`/`use` PATH grammar got before it declined (B320) —
    /// the token index a malformed import reports at.
    ///
    /// Held outside `farthest_failure` because the path grammar is built from
    /// speculative `eat_*` probes rather than committed demands, so it records
    /// nothing there: `import a::{ (impl T) };` explores to the `(` and notes an
    /// expectation nowhere, leaving the statement's farthest failure on the
    /// `import` keyword. Recorded only where the path genuinely FAILS — a
    /// [`Parser::parse_namespace_path_inner`] with no alternative left, or a
    /// brace-set element that is not a path — never at an alternative a sibling
    /// production then reads, so a path that parses records nothing at all.
    /// Cleared at the head of every `import`/`use`, so one statement's record
    /// can never be read by the next.
    import_path_failure: Option<usize>,
    /// The token indices at which a CONTEXTUAL keyword (B414) was read as its
    /// keyword — pushed by [`Parser::eat_word`] and
    /// [`Parser::eat_binder_prefix`], the two funnels every contextual reading
    /// goes through, and popped past the rewind point when an
    /// [`Parser::attempt`] declines. Read only by
    /// [`contextual_keyword_readings`], the editor's raw-parse question "is
    /// this `with` the keyword or a name?" (hover, contextual-keywords.md Q5).
    contextual_readings: Vec<usize>,
    /// The token indices at which a RESERVED word was read as a MEMBER name
    /// (B414 S4) — `x.type`, `fun match(self)` in an `impl`, a field `if: i32`.
    /// Pushed by [`Parser::eat_member_name`] and rolled back with
    /// `contextual_readings` when an [`Parser::attempt`] declines; read only by
    /// [`keyword_member_readings`], so the editor hovers such a word as the
    /// member it is and not as the keyword it spells.
    member_readings: Vec<usize>,
    /// The token index at which the statement being parsed begins, while its
    /// expression is read (B459). A `then`/`else` form that starts THERE may
    /// be the statement reading — the only place the guard `c else S;` can
    /// stand — and one that starts anywhere else is an operand. Set and
    /// restored around each statement's expression, so a statement nested in
    /// a block inside it has its own.
    statement_head: Option<usize>,
    /// The diagnostics of the parser's in-place TOKEN REWRITES (B520's
    /// foreign spellings), held aside from `errors` for `nesting_refusal`'s
    /// reason: [`Parser::attempt`] truncates `errors` when a branch declines,
    /// but the rewrite it reports is not undone — the token stays `ret` or
    /// `fun` for every alternative read after it — so a refusal rolled back
    /// with the branch would leave the foreign word silently accepted. One
    /// per span ([`Parser::record_rewrite`]); [`parse_with`] folds them into
    /// the error list at the end.
    rewrite_refusals: Vec<ParseError>,
    /// Where a REORDERED marker run began in the source, by the stream
    /// position each suffix of the run now starts at
    /// ([`Parser::canonicalize_marker_run`]): `(position, offset)`. A node
    /// beginning at `position` covers that suffix and the declaration after
    /// it, and so begins at `offset` — the earliest of those units as written
    /// — rather than at whichever unit the reorder placed first.
    /// [`Parser::span_from`] reads it; empty unless a run was reordered.
    written_starts: Vec<(usize, usize)>,
    /// The parse's WARNINGS (B536): a declaration head whose attributes are
    /// written out of [`attribute_rank`]'s order. The head parses and analyzes
    /// as if written in it ([`Parser::canonicalize_marker_run`]), so nothing
    /// is refused; [`parse_with_warnings`] hands them back beside the errors.
    /// One per span, held aside from `errors` for `rewrite_refusals`' reason.
    warnings: Vec<ParseError>,
}

/// A recorded farthest failure (see [`Parser::farthest_failure`]).
struct Failure {
    /// The token index reached (converted to a byte span at emission).
    position: usize,
    /// The curated expectations recorded at [`Failure::position`].
    expected: Vec<String>,
    /// The production context at [`Failure::position`], outermost first.
    context: Vec<&'static str>,
}

// --- A postfix suffix in the member/call chain, collected then grouped ----------

/// One postfix operator in the member chain (`proposal/try-and-lift.md` §3): a
/// faithful copy of `chain_expr_parser`'s local `Postfix` enum. Collected in source
/// order, then grouped so the segment from one `?.` to the next `?.`/`!`/chain end
/// forms that lifted link's continuation.
enum Postfix<'src> {
    Member(Spanned<Node<'src>>),
    Index(Spanned<Node<'src>>),
    TryAssert,
    LiftMember(Spanned<Node<'src>>),
    /// A bare `?` (one NOT followed by `.`): an expression-lifting mark.
    LiftBare,
    /// `subject(args)` where the subject is itself a postfix result.
    DirectCall(Spanned<NodeList<'src>>),
}

/// Stamps a binder pattern's bindings mutable (or not) — `mut` at the binder
/// applies to every binding under it, and the pattern parser cannot see the
/// keyword, so it walks them immutable and this restamps. Used by the match/`is`
/// pattern grammar (the `let`/`mut` binding arm); `let`/parameter binders carry
/// mutability in a separate field and are restamped by the analyzer's
/// `set_pattern_bindings_mutable` instead.
///
/// Tuple AND array binders recurse, matching that analyzer twin (B53 finding 5).
/// The array arm used to fall through untouched, so `match arr { mut [a, b] => …
/// }` bound `a`/`b` IMMUTABLE while the identical `mut [a, b] = arr` bound them
/// mutable — one spelling of one keyword meaning two things.
fn apply_binding_mutability(pattern: Pattern<'_>, mutable: bool) -> Pattern<'_> {
    match pattern {
        Pattern::Binding(name, _, name_span) => Pattern::Binding(name, mutable, name_span),
        Pattern::Tuple(patterns) => Pattern::Tuple(
            patterns
                .into_iter()
                .map(|(pattern, span)| (apply_binding_mutability(pattern, mutable), span))
                .collect(),
        ),
        Pattern::Array(patterns) => Pattern::Array(
            patterns
                .into_iter()
                .map(|(pattern, span)| (apply_binding_mutability(pattern, mutable), span))
                .collect(),
        ),
        other => other,
    }
}

/// One argument inside a `[extern(..)]` attribute — a bare word (`method`/`get`/
/// `set`/`new`) or a quoted string (a module path or host symbol). A faithful copy
/// of `parser.rs::ExternArg`.
enum ExternArg<'src> {
    Word(&'src str),
    Text(&'src str),
}

/// Interprets a `[extern(..)]` attribute's arguments into a host binding. A
/// faithful copy of `parser.rs::extern_binding_from_args` — a malformed attribute
/// (author error) lowers to an empty global symbol, exactly as the oracle does.
fn extern_binding_from_args<'src>(args: &[ExternArg<'src>]) -> ExternBinding<'src> {
    use ExternArg::{Text, Word};
    match args {
        [Text(symbol)] => ExternBinding::Function {
            module: None,
            symbol,
        },
        [Text(module), Text(symbol)] => ExternBinding::Function {
            module: Some(module),
            symbol,
        },
        [Word("method")] => ExternBinding::Method { symbol: None },
        [Word("method"), Text(symbol)] => ExternBinding::Method {
            symbol: Some(symbol),
        },
        [Word("get"), Text(symbol)] => ExternBinding::Get { symbol },
        [Word("set"), Text(symbol)] => ExternBinding::Set { symbol },
        [Word("new"), Text(symbol)] => ExternBinding::New {
            module: None,
            symbol,
        },
        [Word("new"), Text(module), Text(symbol)] => ExternBinding::New {
            module: Some(module),
            symbol,
        },
        _ => ExternBinding::Function {
            module: None,
            symbol: "",
        },
    }
}

/// The built-in attribute-marker names — `[derive(..)]`, `[service]`, … — each
/// with its own parser, fused into `function`/`struct` or earlier in the
/// statement choice, and therefore excluded from a *user* macro attribute's name
/// ([`is_known_attribute_marker`]). The one source of truth for the list: both
/// highlighting grammars (`editors/vscode/syntaxes/vilan.tmLanguage.json`,
/// `vilan/docs/theme/vilan.js`) carry a copy, held to this one by
/// `crates/vilan-cli/tests/grammar_sync.rs` (AGENTS.md's three-place rule).
pub const KNOWN_ATTRIBUTE_MARKERS: &[&str] = &[
    "derive",
    "service",
    "client_service",
    "extern",
    "must_use",
    // debugging.md S0 (Q11): the function reports its CALLER's location when
    // it panics, through a hidden `std::debug::Location` parameter.
    "track_caller",
    "rpc",
    "trait_only",
    "doc",
    "expose",
    "platform",
    "deprecated",
    "internal",
    "resource",
    "hint",
    // A142 S7: a struct field's store knobs, `[reactive(coarse)]` and
    // `[reactive(name = "..")]` — a field-position attribute, like `expose`.
    "reactive",
];

/// Whether `name` is one of [`KNOWN_ATTRIBUTE_MARKERS`]. Mirrors the chumsky
/// `macro_attribute_name` guard.
fn is_known_attribute_marker(name: &str) -> bool {
    KNOWN_ATTRIBUTE_MARKERS.contains(&name)
}

/// A declaration attribute's place in the ONE canonical order
/// (keywords-vs-attributes.md §6.2, Q7 RULED 2026-10-01): attributes are
/// written in any order, and this is the order `vilan fmt` prints them in —
/// today's production order, so no attribute moved:
///
/// - generation (0): `[derive]`, `[service]`, `[client_service]`, a user
///   macro attribute (any name the table does not know);
/// - labels: `[deprecated]` (1), `[internal]` (2), `[hint]` (3);
/// - binding: `[extern]` (4);
/// - checks: `[must_use]` (5), `[track_caller]` (6), `[rpc]` (7),
///   `[trait_only]` (8), and the retired `[doc(hidden)]` (9), refused where a
///   function's prefix reads it;
/// - fence: `[platform]` (10);
/// - class: `[resource]` (11).
///
/// Ties keep the order they were written in (a `[service]` and a
/// `[client_service]`, two `[hint]`s). The parser sorts a run into this
/// order before a production reads it ([`Parser::canonicalize_marker_run`]),
/// and the formatter's safety net sorts both streams by it.
pub fn attribute_rank(name: &str) -> u8 {
    match name {
        "deprecated" => 1,
        "internal" => 2,
        "hint" => 3,
        "extern" => 4,
        "must_use" => 5,
        "track_caller" => 6,
        "rpc" => 7,
        "trait_only" => 8,
        "doc" => 9,
        "platform" => 10,
        "resource" => 11,
        _ => 0,
    }
}

/// The marker KEYWORDS of a declaration head (B485 Q8, B486), in the order
/// they are written: `export`, then `const` or `lazy`, then `async`, then
/// `external` or `macro` — after every attribute, before the declaration
/// word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkerKeyword {
    Export,
    Const,
    Lazy,
    Async,
    External,
    Macro,
}

impl MarkerKeyword {
    fn word(self) -> &'static str {
        match self {
            MarkerKeyword::Export => "export",
            MarkerKeyword::Const => "const",
            MarkerKeyword::Lazy => "lazy",
            MarkerKeyword::Async => "async",
            MarkerKeyword::External => "external",
            MarkerKeyword::Macro => "macro",
        }
    }

    /// The ruled WRITTEN order (Q8).
    fn written_rank(self) -> u8 {
        match self {
            MarkerKeyword::Export => 0,
            MarkerKeyword::Const | MarkerKeyword::Lazy => 1,
            MarkerKeyword::Async => 2,
            MarkerKeyword::External | MarkerKeyword::Macro => 3,
        }
    }
}

/// One unit of a declaration's marker run: an attribute group or a keyword
/// (with `export`'s `(in PATH)` scope), and the token range it occupies.
#[derive(Clone, Debug)]
struct MarkerUnit<'src> {
    tokens: std::ops::Range<usize>,
    kind: MarkerUnitKind<'src>,
}

#[derive(Clone, Copy, Debug)]
enum MarkerUnitKind<'src> {
    Attribute { name: &'src str, arguments: bool },
    Keyword(MarkerKeyword),
}

impl MarkerUnitKind<'_> {
    /// Where the unit stands in the order the PRODUCTIONS read — which is not
    /// the written one for three keywords: `export` wraps the whole statement,
    /// and `const` and `macro` each wrap the declaration their production then
    /// reads with its own attribute prefix (`parse_const_declaration`,
    /// `parse_macro_fun`), so all three lead the attributes on the stream.
    fn reading_key(self) -> (u8, u8) {
        match self {
            MarkerUnitKind::Keyword(MarkerKeyword::Export) => (0, 0),
            MarkerUnitKind::Keyword(MarkerKeyword::Const) => (1, 0),
            MarkerUnitKind::Keyword(MarkerKeyword::Macro) => (2, 0),
            MarkerUnitKind::Attribute { name, .. } => (3, attribute_rank(name)),
            MarkerUnitKind::Keyword(MarkerKeyword::Lazy) => (4, 0),
            MarkerUnitKind::Keyword(MarkerKeyword::Async) => (5, 0),
            MarkerUnitKind::Keyword(MarkerKeyword::External) => (6, 0),
        }
    }

    /// Where the unit stands in the WRITTEN canonical order (§6.2): every
    /// attribute by its rank, then the keywords by theirs.
    fn written_key(self) -> (u8, u8) {
        match self {
            MarkerUnitKind::Attribute { name, .. } => (0, attribute_rank(name)),
            MarkerUnitKind::Keyword(keyword) => (1, keyword.written_rank()),
        }
    }

    /// The unit as the steer spells it: `[platform(..)]`, `[must_use]`, `async`.
    fn spelled(self) -> String {
        match self {
            MarkerUnitKind::Attribute { name, arguments } => {
                format!("[{name}{}]", if arguments { "(..)" } else { "" })
            }
            MarkerUnitKind::Keyword(keyword) => keyword.word().to_string(),
        }
    }
}

/// The declaration word a marker run may end at, as the steer spells it, or
/// `None` for any other token — where the run is not a declaration's.
fn declaration_word(token: Option<&Token<'_>>) -> Option<&'static str> {
    Some(match token? {
        Token::Fun => "fun",
        Token::Struct => "struct",
        Token::Enum => "enum",
        Token::Trait => "trait",
        Token::Impl => "impl",
        Token::Let => "let",
        Token::Mut => "mut",
        Token::Mod => "mod",
        Token::Import => "import",
        Token::Use => "use",
        _ => return None,
    })
}

/// Whether `keywords` (in written order, `export` left out) is a set the
/// declaration `word` takes — so a swapped pair can be steered to an order
/// that then PARSES. A set no order makes legal (`async const fun`, `lazy
/// fun`) is left as written, for the production's own refusal.
///
/// `async macro fun` is one (B524, decided by Q8's table): `async` before
/// `external`|`macro`, so `macro async fun`, the one order the macro
/// production read before, is the steered spelling.
fn marker_keywords_are_legal(keywords: &[MarkerKeyword], word: &str) -> bool {
    use MarkerKeyword::*;
    match word {
        "fun" => matches!(
            keywords,
            [] | [Async] | [External] | [Async, External] | [Const] | [Macro] | [Async, Macro]
        ),
        "struct" => matches!(keywords, [] | [External]),
        "let" | "mut" => matches!(keywords, [] | [Const] | [Lazy]),
        _ => keywords.is_empty(),
    }
}

/// The steer for a declaration head written out of order (B486; B485 Q6 and
/// Q8, RULED; B485 S3 for `export` and `macro`): the markers stack
/// attributes first, then the keywords in one order, then the declaration
/// word. `canonical` is the head respelled in that order, attributes
/// abbreviated. An ERROR ([`MarkerOrderDiagnostic::Keywords`]).
fn marker_order_rule(canonical: &str) -> String {
    format!(
        "a declaration's markers are written in one order — its attributes, then the keywords \
         `export`, `const` or `lazy`, `async`, `external` or `macro`, then the declaration \
         word: write `{canonical}`"
    )
}

/// [`marker_order_rule`]'s fixed head, which recognizes it (the rule spells
/// it out in full, for the diagnostics ledger's literal search; the fix pin
/// holds the two together).
const MARKER_ORDER_HEAD: &str = "a declaration's markers are written in one order — ";

/// The warning for a declaration whose attributes alone are out of
/// [`attribute_rank`]'s order (B536, the owner's ruling revising Q7). The
/// head parses as written in the order, so it is not refused; `vilan fmt`
/// writes it so. A WARNING ([`MarkerOrderDiagnostic::Attributes`]).
fn attribute_order_rule(canonical: &str) -> String {
    format!(
        "a declaration's attributes are written in one order — `[derive]` and the other \
         generators, `[deprecated]`, `[internal]`, `[hint]`, `[extern]`, `[must_use]`, `[rpc]`, \
         `[trait_only]`, `[platform]`, `[resource]`: write `{canonical}`"
    )
}

/// [`attribute_order_rule`]'s fixed head, which recognizes it (spelled out
/// there in full, as [`MARKER_ORDER_HEAD`] is).
const ATTRIBUTE_ORDER_HEAD: &str = "a declaration's attributes are written in one order — ";

/// The two diagnostics a declaration head written out of THE order carries
/// (B536), as the editor keys them. The head is read as if written in the
/// order either way; what differs is whether the order it broke is one this
/// release refuses.
///
/// **The editor's half**, the same shape as [`ForeignSpelling`]'s:
/// [`MarkerOrderDiagnostic::code`] is the stable code to publish as the
/// diagnostic's `code`, [`MarkerOrderDiagnostic::of_message`] recognizes the
/// diagnostic from its rendered text, and [`marker_order_fix`] is the quick
/// fix's edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkerOrderDiagnostic {
    /// Refused since v0.45.0 (a WARNING for one release, B536): the attributes
    /// are out of [`attribute_rank`]'s order, and
    /// nothing else is ([`ParseErrorReason::AttributeOrder`]).
    Attributes,
    /// An ERROR: a keyword stands ahead of an attribute — `export` and
    /// `macro` included (B485 S3) — or two keywords are inverted
    /// ([`ParseErrorReason::MarkerOrder`]).
    Keywords,
}

impl MarkerOrderDiagnostic {
    /// The diagnostic's stable code.
    pub fn code(self) -> &'static str {
        match self {
            MarkerOrderDiagnostic::Attributes => "marker-order/attributes",
            MarkerOrderDiagnostic::Keywords => "marker-order/keywords",
        }
    }

    /// Whether the diagnostic is a warning (the program is accepted). Neither
    /// is since v0.45.0: the attribute order warned for one release (B536).
    pub fn is_warning(self) -> bool {
        false
    }

    /// The marker-order diagnostic a rendered message reports, if it reports
    /// one. The parser renders both with no context and no hint, so each
    /// message begins with its fixed head.
    pub fn of_message(message: &str) -> Option<MarkerOrderDiagnostic> {
        if message.starts_with(ATTRIBUTE_ORDER_HEAD) {
            Some(MarkerOrderDiagnostic::Attributes)
        } else if message.starts_with(MARKER_ORDER_HEAD) {
            Some(MarkerOrderDiagnostic::Keywords)
        } else {
            None
        }
    }
}

/// The quick fix for a marker-order diagnostic (B536): replace `span` — the
/// diagnostic's own, the head's marker run — with `replacement`, the run's
/// units in THE order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkerOrderFix {
    /// The diagnostic's stable code ([`MarkerOrderDiagnostic::code`]).
    pub code: &'static str,
    /// The quick fix's title: "Write `[deprecated(..)] [must_use] fun`".
    pub title: String,
    /// The source range the edit replaces.
    pub span: Span,
    /// What the edit writes there.
    pub replacement: String,
}

/// The quick fix for the diagnostic `message` anchored at `span` in `source`,
/// or `None` when it is not a marker-order diagnostic (B536), or `span` is
/// not a marker run in `source` (the buffer moved on since the diagnostic).
///
/// The edit permutes the run's UNITS — each attribute group and keyword
/// (`export` with its `(in PATH)`) exactly as written, arguments and all —
/// into THE order, and keeps what stood BETWEEN them where it stood: the
/// blanks, a line break, a comment. So `[must_use]` ⏎ `[deprecated("d")]` ⏎
/// `fun` becomes `[deprecated("d")]` ⏎ `[must_use]` ⏎ `fun`, and `export
/// [must_use] fun` becomes `[must_use] export fun`. The result is the head
/// the parser read; `vilan fmt` lays it out.
pub fn marker_order_fix(source: &str, message: &str, span: Span) -> Option<MarkerOrderFix> {
    let diagnostic = MarkerOrderDiagnostic::of_message(message)?;
    let (tokens, _) = lexing::tokenize(source);
    let start = tokens
        .iter()
        .position(|(_, token_span)| token_span.start == span.start)?;
    let run = marker_run_at(&tokens, start)?;
    let unit_range = |unit: &MarkerUnit<'_>| {
        tokens[unit.tokens.start].1.start..tokens[unit.tokens.end - 1].1.end
    };
    let mut written = run.units.clone();
    written.sort_by_key(|unit| tokens[unit.tokens.start].1.start);
    let run_end = written.iter().map(|unit| unit_range(unit).end).max()?;
    if run_end != span.end {
        return None;
    }
    let mut canonical = written.clone();
    canonical.sort_by_key(|unit| unit.kind.written_key());
    let mut replacement = String::new();
    for (index, (slot, unit)) in written.iter().zip(&canonical).enumerate() {
        if index > 0 {
            let gap = unit_range(&written[index - 1]).end..unit_range(slot).start;
            replacement.push_str(source.get(gap)?);
        }
        replacement.push_str(source.get(unit_range(unit))?);
    }
    let mut spelled: Vec<String> = canonical.iter().map(|unit| unit.kind.spelled()).collect();
    spelled.push(run.word.to_string());
    Some(MarkerOrderFix {
        code: diagnostic.code(),
        title: format!("Write `{}`", spelled.join(" ")),
        span,
        replacement,
    })
}

/// A declaration's marker run: its units in STREAM order, and the declaration
/// word it ends at.
struct MarkerRun<'src> {
    units: Vec<MarkerUnit<'src>>,
    /// The declaration word, as the steer spells it.
    word: &'static str,
    /// The word's token index.
    word_at: usize,
}

/// The token index just past the balanced group opening at `open`, or `None`
/// when it never closes.
fn past_balanced_group_in(tokens: &[Spanned<Token<'_>>], open: usize) -> Option<usize> {
    past_balanced_group_with(|at| tokens.get(at).map(|(token, _)| token), open)
}

/// [`past_balanced_group_in`] over any token reader.
fn past_balanced_group_with<'t, 'src: 't>(
    token_at: impl Fn(usize) -> Option<&'t Token<'src>>,
    open: usize,
) -> Option<usize> {
    let mut depth = 0usize;
    let mut at = open;
    loop {
        match token_at(at)? {
            Token::Ctrl('[' | '(' | '{') => depth += 1,
            Token::Ctrl(']' | ')' | '}') => {
                depth -= 1;
                if depth == 0 {
                    return Some(at + 1);
                }
            }
            _ => {}
        }
        at += 1;
    }
}

/// The run of attribute groups and marker keywords starting at `start` in
/// `tokens`, when it ends at a declaration word ([`declaration_word`]);
/// `None` for any other run — `[a][b];`, `async { .. }`, an unclosed group.
/// The ONE scanner the parser's canonicalizer, the quick fix and the
/// formatter's safety net share ([`marker_run_with`]).
fn marker_run_at<'src>(tokens: &[Spanned<Token<'src>>], start: usize) -> Option<MarkerRun<'src>> {
    marker_run_with(|at| tokens.get(at).map(|(token, _)| token), start)
}

/// The token ranges of the marker run at `start` in a bare token stream, in
/// THE written order (attributes by [`attribute_rank`], then the keywords by
/// Q8's), ties as written, and the index of the declaration word it ends at;
/// `None` when no run of two or more units ending at a declaration word
/// starts there. The formatter's safety net puts both of its streams' heads
/// into this order (B536), so it accepts the printer writing a head the
/// parser read in THE order whatever order it was written in.
pub(crate) fn marker_run_in_written_order(
    tokens: &[Token<'_>],
    start: usize,
) -> Option<(Vec<std::ops::Range<usize>>, usize)> {
    let run = marker_run_with(|at| tokens.get(at), start)?;
    if run.units.len() < 2 {
        return None;
    }
    let mut units = run.units;
    units.sort_by_key(|unit| unit.kind.written_key());
    Some((
        units.into_iter().map(|unit| unit.tokens).collect(),
        run.word_at,
    ))
}

/// [`marker_run_at`] over any token reader.
fn marker_run_with<'t, 'src: 't>(
    token_at: impl Fn(usize) -> Option<&'t Token<'src>> + Copy,
    start: usize,
) -> Option<MarkerRun<'src>> {
    let mut units: Vec<MarkerUnit<'src>> = Vec::new();
    let mut at = start;
    loop {
        let keyword = match token_at(at) {
            Some(Token::Ctrl('[')) => {
                let Some(Token::Ident(name)) = token_at(at + 1) else {
                    break;
                };
                let name = *name;
                let end = past_balanced_group_with(token_at, at)?;
                let arguments = token_at(at + 2) == Some(&Token::Ctrl('('));
                units.push(MarkerUnit {
                    tokens: at..end,
                    kind: MarkerUnitKind::Attribute { name, arguments },
                });
                at = end;
                continue;
            }
            Some(Token::Export) => MarkerKeyword::Export,
            Some(Token::Const) => MarkerKeyword::Const,
            Some(Token::Ident("lazy")) => MarkerKeyword::Lazy,
            Some(Token::Async) => MarkerKeyword::Async,
            Some(Token::External) => MarkerKeyword::External,
            Some(Token::Macro) => MarkerKeyword::Macro,
            _ => break,
        };
        let mut end = at + 1;
        if keyword == MarkerKeyword::Export
            && token_at(end) == Some(&Token::Ctrl('('))
            && token_at(end + 1) == Some(&Token::In)
        {
            end = past_balanced_group_with(token_at, end)?;
        }
        units.push(MarkerUnit {
            tokens: at..end,
            kind: MarkerUnitKind::Keyword(keyword),
        });
        at = end;
    }
    let word = declaration_word(token_at(at))?;
    Some(MarkerRun {
        units,
        word,
        word_at: at,
    })
}

thread_local! {
    /// How many atoms ([`Parser::parse_atom`]) this thread's parser has entered
    /// — the parser's unit of real work, and what speculative re-parsing
    /// multiplies. See [`atom_parse_count`].
    static ATOM_PARSES: Cell<u64> = const { Cell::new(0) };
}

/// The number of expression atoms this thread's parser has entered — an
/// instrumentation probe (B140), not a behavior surface, in the shape of
/// [`crate::formatter::buffer_parse_count`]: monotonic, read as a snapshot
/// difference around the work under test.
///
/// It exists because the cost this pins is INVISIBLE to wall-clock at the sizes
/// a test may use and astronomical just past them. `parse_assignment` used to
/// parse a whole precedence chain speculatively and discard it, so every
/// bracket level parsed its own subtree twice and the atom count doubled per
/// level; `(1 + (1 + …))` at 20 levels cost 9.0s in the parser alone. A wall
/// ceiling would catch that, but it would NOT catch a regression to merely
/// quadratic — which this counter does, because the count is compared against a
/// line in the nesting depth rather than against a clock. Counting is
/// unconditional (one thread-local increment per atom, noise against a parse)
/// so the pin needs no environment plumbing: a `OnceLock`-cached switch would
/// have to be set before the process's first parse.
pub fn atom_parse_count() -> u64 {
    ATOM_PARSES.with(Cell::get)
}

/// Whether `symbol` is one of the six assignment operators `parse_assignment`
/// accepts after a place.
fn is_assignment_operator(symbol: &str) -> bool {
    matches!(symbol, "=" | "+=" | "-=" | "*=" | "/=" | "%=")
}

/// The [`Parser::assignment_reachable`] table: for every token position, whether
/// an assignment operator follows it at the SAME bracket depth, within the same
/// bracket group and the same `;`-terminated statement.
///
/// One backward pass with a stack of per-depth flags. Read right-to-left a
/// CLOSING bracket opens a deeper region (push a fresh flag) and an OPENING one
/// ends it (pop); a `;` clears the current depth's flag because no place may
/// span a statement terminator; an assignment operator sets it. The answer for a
/// position is the flag standing after that position's own token is folded in,
/// which makes the table conservative at the operator itself (a place is never
/// empty, so that entry only costs a declined attempt).
fn assignment_reachable(tokens: &[Spanned<Token<'_>>]) -> Vec<bool> {
    // One past the end so a cursor sitting at EOF reads `false` without a bounds
    // check at every call site.
    let mut table = vec![false; tokens.len() + 1];
    let mut depths: Vec<bool> = vec![false];
    for (index, (token, _span)) in tokens.iter().enumerate().rev() {
        match token {
            Token::Ctrl(')' | ']' | '}') => depths.push(false),
            Token::Ctrl('(' | '[' | '{') => {
                depths.pop();
                // Unbalanced source (the parser still runs on a recovered tree):
                // never leave the stack empty.
                if depths.is_empty() {
                    depths.push(false);
                }
            }
            Token::Ctrl(';') => {
                if let Some(flag) = depths.last_mut() {
                    *flag = false;
                }
            }
            Token::Op(symbol) if is_assignment_operator(symbol) => {
                if let Some(flag) = depths.last_mut() {
                    *flag = true;
                }
            }
            _ => {}
        }
        table[index] = depths.last().copied().unwrap_or(false);
    }
    table
}

impl<'a, 'src> Parser<'a, 'src> {
    fn new(
        tokens: &'a mut [Spanned<Token<'src>>],
        source: &'src str,
        preserve_paren_groups: bool,
    ) -> Self {
        let assignment_reachable = assignment_reachable(tokens);
        Parser {
            tokens,
            position: 0,
            source,
            eoi: source.len(),
            errors: Vec::new(),
            assignment_reachable,
            farthest_failure: None,
            context_stack: Vec::new(),
            preserve_paren_groups,
            in_member_body: false,
            file_head: false,
            nesting_depth: 0,
            nesting_refusal: None,
            import_path_failure: None,
            contextual_readings: Vec::new(),
            member_readings: Vec::new(),
            statement_head: None,
            rewrite_refusals: Vec::new(),
            written_starts: Vec::new(),
            warnings: Vec::new(),
        }
    }

    // --- Cursor primitives ---------------------------------------------------

    fn peek(&self) -> Option<&Token<'src>> {
        self.tokens.get(self.position).map(|(token, _)| token)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token<'src>> {
        self.tokens
            .get(self.position + offset)
            .map(|(token, _)| token)
    }

    fn at_end(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn bump(&mut self) {
        self.position += 1;
    }

    fn peek_is_ctrl(&self, character: char) -> bool {
        matches!(self.peek(), Some(Token::Ctrl(found)) if *found == character)
    }

    fn peek_at_is_ctrl(&self, offset: usize, character: char) -> bool {
        matches!(self.peek_at(offset), Some(Token::Ctrl(found)) if *found == character)
    }

    fn peek_is_op(&self, symbol: &str) -> bool {
        matches!(self.peek(), Some(Token::Op(found)) if *found == symbol)
    }

    fn peek_is(&self, token: &Token<'src>) -> bool {
        self.peek() == Some(token)
    }

    /// The `&str` of the current `Op` token, if any (for the arithmetic/comparison
    /// operator tables).
    fn peek_op(&self) -> Option<&'src str> {
        if let Some(Token::Op(symbol)) = self.peek() {
            Some(symbol)
        } else {
            None
        }
    }

    fn eat_ctrl(&mut self, character: char) -> bool {
        if self.peek_is_ctrl(character) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat_op(&mut self, symbol: &str) -> bool {
        if self.peek_is_op(symbol) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn eat(&mut self, token: &Token<'src>) -> bool {
        if self.peek_is(token) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// Expect a control token. A CLOSING delimiter (`)` `]` `}` `>`) is a committed
    /// demand — the matching opener was already consumed — so on failure it records
    /// what was wanted (`note_expected`), letting a farthest-failure diagnostic name
    /// the real, located error ("found `y` expected `,` or `}`" at `y`) rather than a
    /// false "unclosed" claim on a region that actually closed (`recover_delimited`).
    /// An OPENING delimiter, by contrast, is a speculative head-check for an
    /// alternative and stays silent — noting it would fill the expected set with every
    /// alternative's opener (chumsky's expected-dump noise, which the curated
    /// farthest-failure exists to avoid). The note is on the failure path only, so a
    /// clean (error-free) parse is untouched and stays byte-identical.
    fn expect_ctrl(&mut self, character: char) -> Option<()> {
        if self.eat_ctrl(character) {
            Some(())
        } else {
            if matches!(character, ')' | ']' | '}' | '>') {
                self.note_expected(&format!("'{character}'"));
            }
            None
        }
    }

    /// Expect an operator token. Every `expect_op` site is a COMMITTED demand (a
    /// speculative operator check uses `eat_op` / `peek_is_op` and backtracks), so on
    /// failure it records what was wanted — e.g. a closure's closing `|` or a match
    /// arm's `=>`. Failure-path only, so a clean parse is untouched.
    fn expect_op(&mut self, symbol: &str) -> Option<()> {
        if self.eat_op(symbol) {
            Some(())
        } else {
            self.note_expected(&format!("'{symbol}'"));
            None
        }
    }

    /// Expect a specific token. Kept SILENT on failure: `expect` guards the leading
    /// keyword of item/statement alternatives (`fun`, `struct`, `is`, …), which are
    /// speculative head-checks — noting them would list every item keyword at any
    /// statement-position failure. Delimiter/closer demands (the false-"unclosed"
    /// class) go through `expect_ctrl`/`expect_op`, which do note.
    fn expect(&mut self, token: &Token<'src>) -> Option<()> {
        self.eat(token).then_some(())
    }

    fn eat_ident(&mut self) -> Option<&'src str> {
        if let Some(Token::Ident(name)) = self.peek() {
            let name = *name;
            self.bump();
            Some(name)
        } else {
            None
        }
    }

    /// A name in path / variant position: an identifier, or the boolean-literal
    /// keywords (`true`/`false`) so the bootstrap `bool` enum and its variants can
    /// be written. Mirrors the chumsky `name` production.
    fn eat_name(&mut self) -> Option<&'src str> {
        match self.peek() {
            Some(Token::Ident(name)) => {
                let name = *name;
                self.bump();
                Some(name)
            }
            Some(Token::Bool(true)) => {
                self.bump();
                Some("true")
            }
            Some(Token::Bool(false)) => {
                self.bump();
                Some("false")
            }
            _ => None,
        }
    }

    /// A MEMBER name (B414 S4, contextual-keywords.md §5): an identifier, or
    /// any RESERVED word — `event.type`, `fun match(self)` in an `impl`, a
    /// field `if: i32`. Each member position is entered after a token that
    /// commits to it (`.`, `::`, `fun` inside a member body, a struct body's
    /// `{`/`,`), so no production can start there with the keyword and the
    /// admission costs no lookahead. The word is the source text of the
    /// token (a reserved word's token carries none), and a reserved reading is
    /// recorded for the editor ([`Parser::member_readings`]).
    fn eat_member_name(&mut self) -> Option<&'src str> {
        let (token, span) = self.tokens.get(self.position)?;
        match token {
            Token::Ident(name) => {
                let name = *name;
                self.bump();
                Some(name)
            }
            token if is_reserved_word(token) => {
                let word = self.source.get(span.into_range())?;
                self.member_readings.push(self.position);
                self.bump();
                Some(word)
            }
            _ => None,
        }
    }

    /// Whether the token at the cursor can be a member name — an identifier,
    /// a reserved word, or a tuple index (`.0`).
    fn peek_is_member_name(&self) -> bool {
        match self.peek() {
            Some(Token::Ident(_) | Token::Number(..)) => true,
            Some(token) => is_reserved_word(token),
            None => false,
        }
    }

    /// R-k (B414 S4, RULED 2026-09-29): the name after a member `.` is
    /// written against it. Called with the dot just consumed, before its name
    /// is read; answers what the member position holds.
    ///
    /// - Nothing between them: the member, read as before.
    /// - No name at all (`list.` before a `}` or a `;`): the mid-edit shape
    ///   completion lives on — an `Error` member, silent here, the receiver
    ///   still analyzed.
    /// - A space on the same line (`list. len()`): reported, and the member is
    ///   read anyway — the reading is not in doubt, only the spelling.
    /// - A line break before a word that BEGINS a statement or an item
    ///   (`list.` ⏎ `let x = 1;`): the half-typed dot above the next line. The
    ///   word is not taken — under the member tier it could be — and the dot
    ///   gets the `Error` member it always got, so the statement's missing-`;`
    ///   recovery keeps it and the receiver stays analyzed for completion.
    /// - A line break before any other word (`list.` ⏎ `helper();`): declined
    ///   with the expectation noted, exactly as E142 declines a `::` that ends
    ///   its line, so the next line is never swallowed as the member and the
    ///   statement's recovery reports the one mistake, at the stranded name.
    fn member_after_dot(&mut self) -> DotMember {
        if !self.peek_is_member_name() {
            return DotMember::Missing;
        }
        let Some(dot) = self
            .position
            .checked_sub(1)
            .and_then(|at| self.tokens.get(at))
        else {
            return DotMember::Read;
        };
        let dot_span = dot.1;
        let name_span = self.here_span();
        let gap = self
            .source
            .get(dot_span.into_range().end..name_span.into_range().start)
            .unwrap_or("");
        if gap.is_empty() {
            return DotMember::Read;
        }
        if gap.contains('\n') {
            if starts_statement_or_item(self.tokens, self.position) {
                return DotMember::Missing;
            }
            self.note_expected(A_MEMBER_NAME_AGAINST_ITS_DOT);
            return DotMember::Decline;
        }
        let found = self
            .source
            .get(name_span.into_range())
            .unwrap_or_default()
            .to_string();
        self.errors.push(ParseError {
            span: Span::from(dot_span.into_range().start..name_span.into_range().end),
            reason: ParseErrorReason::Expected {
                found: Found::Token(found),
                expected: vec![A_MEMBER_NAME_AGAINST_ITS_DOT.to_string()],
            },
            context: self.context_stack.clone(),
            hint: None,
        });
        DotMember::Read
    }

    // --- Contextual keywords (B414, proposal/contextual-keywords.md) --------

    /// Whether the cursor is at the identifier `word` — how every contextual
    /// keyword (lexing.rs `CONTEXTUAL_KEYWORDS`) is recognized: the lexer hands
    /// it back as a name, and the parser reads its keyword meaning by position.
    fn peek_is_word(&self, word: &str) -> bool {
        matches!(self.peek(), Some(Token::Ident(found)) if *found == word)
    }

    /// Consumes the contextual keyword `word` at a POSITIONAL decision point —
    /// one where no name can stand, so the word there is always the keyword:
    /// `with` after an `impl` subject or a `trait` head, `borrows` after a
    /// declaration's return type.
    fn eat_word(&mut self, word: &str) -> bool {
        if self.peek_is_word(word) {
            self.contextual_readings.push(self.position);
            self.bump();
            true
        } else {
            false
        }
    }

    /// The LL(2) decision (contextual-keywords.md §2) for a contextual keyword
    /// whose reading is a PREFIX of a binder — `own` and `lazy` at a parameter
    /// head: the word at the cursor is the keyword when what follows it can
    /// begin a binder (a name, `mut`, a destructure's `(`/`[`, a spread's
    /// `...`), and a NAME otherwise (`own: Owner`, `|lazy| lazy.force()`).
    /// Sound because vilan never puts two names side by side.
    ///
    /// A spread is all THREE dots (B446): one `.` after the word is a member
    /// access on a parameter NAMED `own` or `lazy` — `own.x` cannot be a
    /// binder, and reading the word as the convention there left the binder
    /// to fail on the `.` and the parameter list to report at the token
    /// before it.
    fn eat_binder_prefix(&mut self, word: &str) -> bool {
        let prefixes_a_binder = self.peek_is_word(word)
            && match self.peek_at(1) {
                Some(Token::Ident(_) | Token::Mut | Token::Ctrl('(' | '[')) => true,
                Some(Token::Ctrl('.')) => {
                    self.peek_at_is_ctrl(2, '.') && self.peek_at_is_ctrl(3, '.')
                }
                _ => false,
            };
        if prefixes_a_binder {
            self.contextual_readings.push(self.position);
            self.bump();
        }
        prefixes_a_binder
    }

    // --- Span assembly -------------------------------------------------------

    /// The span of a parse that began at token index `start` and ends at the
    /// current position — reproducing chumsky's `MappedInput::span` (input.rs):
    /// `first_token.start .. last_consumed_token.end` for a non-empty parse, and
    /// the EOI span (`eoi..eoi`) when there is no token at `start`. The empty-parse
    /// case (position == start with a token present) reproduces chumsky's
    /// `first_token.start .. previous_token.end`, which never surfaces in a produced
    /// node here but is kept faithful.
    fn span_from(&self, start: usize) -> Span {
        if start >= self.tokens.len() {
            return (self.eoi..self.eoi).into();
        }
        let mut start_offset = self.tokens[start].1.start;
        if let Some((_, written)) = self
            .written_starts
            .iter()
            .find(|(position, _)| *position == start)
        {
            start_offset = start_offset.min(*written);
        }
        let end_offset = if self.position > start {
            self.tokens[self.position - 1].1.end
        } else if start > 0 {
            self.tokens[start - 1].1.end
        } else {
            self.eoi
        };
        (start_offset..end_offset).into()
    }

    /// The span of the current token (a single-token span), or a zero-width span at
    /// end of input.
    fn here_span(&self) -> Span {
        if let Some((_, span)) = self.tokens.get(self.position) {
            *span
        } else {
            (self.eoi..self.eoi).into()
        }
    }

    /// Run `body`; if it declines (`None`), restore the cursor AND roll back any
    /// errors it emitted. The recursive-descent equivalent of chumsky's
    /// backtracking on a failed alternative: a discarded branch leaves nothing
    /// behind — neither cursor movement nor diagnostics — so a recovery error
    /// emitted inside a branch is kept only if that branch ultimately succeeds
    /// (the farthest-failure record, deliberately, is NOT rolled back — the
    /// deepest exploration is remembered across backtracks, which is what makes
    /// the decline diagnostic point at the real problem).
    fn attempt<T>(&mut self, body: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let start = self.position;
        let error_count = self.errors.len();
        let result = body(self);
        if result.is_none() {
            self.position = start;
            self.errors.truncate(error_count);
            while self
                .contextual_readings
                .last()
                .is_some_and(|index| *index >= start)
            {
                self.contextual_readings.pop();
            }
            while self
                .member_readings
                .last()
                .is_some_and(|index| *index >= start)
            {
                self.member_readings.pop();
            }
            // `nesting_refusal` is deliberately NOT restored: like
            // `farthest_failure`, it records how deep the input went, which no
            // backtrack un-does. See the field.
        }
        result
    }

    // --- Error tracking (farthest failure) -----------------------------------

    /// Record that `expected` was wanted at the current position, keeping only the
    /// farthest such point. Called at every committed token demand ([`Parser::expect`]
    /// / [`Parser::expect_ctrl`] / [`Parser::expect_op`]), the comma-list separator,
    /// and the semantic leaves (atom/type/pattern), never at the speculative `eat_*`
    /// probes. Purely diagnostic — it never touches the parsed tree, so the clean
    /// (error-free) path is unchanged and a clean parse stays byte-identical.
    fn note_expected(&mut self, expected: &str) {
        let position = self.position;
        let advance = match &self.farthest_failure {
            Some(failure) => position > failure.position,
            None => true,
        };
        if advance {
            self.farthest_failure = Some(Failure {
                position,
                expected: vec![expected.to_string()],
                context: self.context_stack.clone(),
            });
        } else if let Some(failure) = &mut self.farthest_failure
            && position == failure.position
            && !failure.expected.iter().any(|e| e == expected)
        {
            failure.expected.push(expected.to_string());
        }
    }

    /// Push a production context for the span of `body`, popping it afterward
    /// (always — even when `body` declines), so the stack a farthest failure
    /// snapshots reflects where it occurred. Only the ~productions where a label
    /// aids the message wrap their body, keeping the `in <context>` tail curated.
    fn in_context<T>(
        &mut self,
        label: &'static str,
        body: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Option<T> {
        self.context_stack.push(label);
        let result = body(self);
        self.context_stack.pop();
        result
    }

    /// Record that a statement's terminating `;` was wanted here — the committed
    /// side of `frontend.md:24-31`'s noting rule, which `;` was never put on
    /// (`editing-dx.md` §4.2/S2). Called at the three STATEMENT-terminator demands
    /// (`expression ;`, `import … ;`, `use … ;`) and nowhere else: the `[T; N]`
    /// array type and the `[value; length]` repeat fork spell `;` as a speculative
    /// fork between two readings, not as a committed demand, so noting them would
    /// put "expected `;` to end this statement" on a type-position failure.
    fn note_terminator(&mut self) {
        self.note_expected(TERMINATOR_EXPECTED);
    }

    /// Record that the `import`/`use` path grammar stopped at token `at` (B320),
    /// keeping the FARTHEST such point — one statement's path is explored
    /// outside-in, so the deepest stop is the one the reader typed wrong.
    fn note_import_failure(&mut self, at: usize) {
        if self
            .import_path_failure
            .is_none_or(|recorded| at > recorded)
        {
            self.import_path_failure = Some(at);
        }
    }

    /// B248/B259: refuse an operator — or a `.` chain — that continues an
    /// expression the block-like form just parsed has already ENDED, and steer to
    /// the parentheses that spell what was meant. Called at the four block-bearing
    /// HEADS of [`Parser::parse_secondary_inner`], so it covers every position a
    /// block-like form can head an expression from: a statement, a `let`
    /// initializer, an argument, a `ret` tail.
    ///
    /// B231 admitted a block-like form as an OPERAND (`flag && match p() { .. }`),
    /// where nothing is ambiguous because an operator has already committed the
    /// position to an expression. As the HEAD it is a different question, and
    /// vilan answers it the way Rust does: a statement that begins with `match`,
    /// `if`, `for` or `{` is COMPLETE at its closing brace, so what follows begins
    /// a new statement. That rule is what makes a leading `-` or `*` on the next
    /// line mean subtraction or a dereference by where it sits rather than by what
    /// the parser felt like — and the language already relies on it:
    /// `if c { 1 } else { 2 }` followed by `* 3;` parses today as two statements,
    /// and admitting the tower after a block-like head would silently re-read it
    /// as one.
    ///
    /// B248 put the refusal at the STATEMENT fork, which is where the shape was
    /// found; B259 found the other three positions, where the same source got the
    /// bare parse error the refusal exists to replace (`let y = match x { .. } + 1;`
    /// reported "expected `;`" and nothing about why, an argument position
    /// `found '+' expected ',' or ')'`). The rule is about the FORM, not about the
    /// statement, so it is now stated once where the form is parsed.
    ///
    /// Only an operator that cannot BEGIN an expression is refused. `-`, `!`, `&`
    /// and `*` can, and the two readings of those are exactly the ambiguity the
    /// rule exists to settle: they begin a new statement, as they always have. `<`
    /// and `>` are control characters here rather than operators, and a statement
    /// CAN begin with `<` (element syntax), so they are outside the set too — as
    /// are the `(` and `[` postfixes, which lead a parenthesized expression and a
    /// list literal.
    ///
    /// `::` is INSIDE the set and takes a rule of its own (E157,
    /// [`A_PATH_CANNOT_START_AT_A_BLOCK`]). The shared message's value is its
    /// STEER, and the steer has to be a fix: `(match x { .. }) + 1` and
    /// `(match x { .. }).to_str()` both parse, and `(match x { .. })::foo` does
    /// not — a path reaches into a namespace, so its left side is a name and no
    /// expression can stand there. Sending the author to parentheses bought
    /// them a second parse error and nothing else.
    ///
    /// `=>` is outside it for a different reason, and it is the one B248 could not
    /// see from the statement fork: it is a SEPARATOR of the enclosing production,
    /// not a continuation of this expression. A match GUARD is an expression that
    /// ends where the arrow begins — `Some(let n) if match n { 0 => false, _ => true }
    /// => …` is a real shape in the tree — so an arrow after a block-like form is
    /// the arm's, and taking it would eat the arm.
    ///
    /// The RECOVERY consumes what it refused, which is what makes this REPLACE the
    /// bare failure rather than sit above it: the author gets one diagnostic naming
    /// the shape instead of two, the second of them about a statement they did not
    /// know they had written (diagnostics-standard B5). A `.` chain is consumed
    /// WHOLE — skipping only the `.` would leave `to_str()` as a bare name and
    /// cascade to "cannot find 'to_str'", a second diagnostic about generated
    /// nonsense.
    fn refuse_block_like_continuation(&mut self, no_struct: bool) {
        let mut refused = false;
        loop {
            let operator = match self.peek() {
                // `::` is an operator token, and the ONE continuation whose
                // refusal is not this rule's (E157): every other one is steered
                // to parentheses, and that steer is a FIX for every other one.
                // A path is not an expression, so there is nothing to
                // parenthesize and the steer sent the author to a second parse
                // error. Its own statement, at its own site.
                Some(Token::Op("::")) => {
                    if !refused {
                        self.errors.push(ParseError {
                            span: self.here_span(),
                            reason: ParseErrorReason::Rule(A_PATH_CANNOT_START_AT_A_BLOCK),
                            context: self.context_stack.clone(),
                            hint: None,
                        });
                        refused = true;
                    }
                    true
                }
                Some(Token::Op(symbol)) if !matches!(*symbol, "!" | "-" | "&" | "*" | "=>") => true,
                Some(Token::Ctrl('.')) => false,
                _ => return,
            };
            if !refused {
                self.errors.push(ParseError {
                    span: self.here_span(),
                    reason: ParseErrorReason::Rule(BLOCK_LIKE_STATEMENT_IS_COMPLETE),
                    context: self.context_stack.clone(),
                    hint: None,
                });
                refused = true;
            }
            if operator {
                // The operator, then the rest of the tower it opened — attempted,
                // so a right operand that does not parse leaves the position where
                // the operator left it rather than half-consumed.
                self.bump();
                self.attempt(|parser| parser.parse_operators(no_struct));
                return;
            }
            // Every link of the chain. Each consumes at least its own leading
            // token, so the loop strictly advances; a following operator is then
            // taken by the arm above, under the refusal already pushed.
            while matches!(self.parse_one_postfix(), Some(Some(_))) {}
        }
    }

    /// Push an `Expected` error for a failure at `position`: the found token, its
    /// curated `expected` set, its production `context`, and the structural
    /// `!=`-soup hint when it applies. Shared by the top-level leftover diagnostic
    /// and delimiter recovery (which surfaces the real inner error, not a claim the
    /// region was unclosed).
    fn emit_failure(&mut self, position: usize, expected: Vec<String>, context: Vec<&'static str>) {
        // A failure ON the `css` keyword outranks every other reading of it,
        // the missing-terminator anchor included: `css` became a hard keyword
        // with the block (proposal/css-block.md §5.4, Q3), so a parse that
        // stopped there is almost always a pre-promotion spelling — a field, a
        // path segment or a binding that used to be named `css` — and the gap
        // anchor would point at the `::` before it and ask for a `;`. A block
        // whose body is malformed never reaches here: its body COMMITS, and
        // reports at the item that broke.
        if matches!(self.tokens.get(position), Some((Token::Css, _))) {
            // A `{` after the word means the author wrote a BLOCK, in a
            // position that suppresses one: a `css` block is brace-initial, so
            // condition mode excludes it exactly as it excludes a struct
            // literal (§4.2), and the fix is a pair of parentheses, not a
            // rename.
            let block = matches!(self.tokens.get(position + 1), Some((Token::Ctrl('{'), _)));
            self.errors.push(ParseError {
                span: self.token_span(position),
                reason: ParseErrorReason::Rule(if block {
                    CSS_BLOCK_IS_BRACE_INITIAL
                } else {
                    CSS_IS_A_KEYWORD
                }),
                context,
                hint: None,
            });
            return;
        }
        // B320: an `import`/`use` whose PATH the grammar could not read. The
        // located failure is on the keyword — `import` begins no expression, so
        // the expression fork notes there and nothing deeper notes at all — and
        // the keyword is the one token that was right. The rule replaces the
        // message and reports where the path actually stopped.
        if matches!(
            self.tokens.get(position),
            Some((Token::Import | Token::Use, _))
        ) && let Some(stopped) = self.import_path_failure
            && stopped > position
        {
            self.errors.push(ParseError {
                span: self.token_span(stopped),
                reason: ParseErrorReason::Rule(IMPORT_PATH_IS_NAMES_AND_SETS),
                context,
                hint: None,
            });
            return;
        }
        // `pub fun …` outranks the missing-terminator reading of it the way the
        // `css` keyword does: the `;` the gap anchor asks for is a true statement
        // about a program nobody wrote, and the word before it is the whole
        // mistake. Recognized structurally — the identifier `pub`, immediately
        // before a token that starts a fresh statement or item — never by
        // matching the message.
        if let Some((at, marker)) = self.visibility_marker_before(position) {
            self.errors.push(ParseError {
                span: self.token_span(at),
                reason: ParseErrorReason::VisibilityMarker { marker },
                context,
                hint: None,
            });
            return;
        }
        // `let mut x = …`: two binding forms written as one. The located failure
        // is on the `let` — or, since the binder records what it wanted
        // (B446), on the `mut` it found where a name goes — and neither is
        // the mistake the reader needs named, so the rule replaces the message
        // rather than trailing it.
        let let_mut = if matches!(self.tokens.get(position), Some((Token::Let, _)))
            && matches!(self.tokens.get(position + 1), Some((Token::Mut, _)))
        {
            Some(position)
        } else if matches!(self.tokens.get(position), Some((Token::Mut, _)))
            && let Some(previous) = position.checked_sub(1)
            && matches!(self.tokens.get(previous), Some((Token::Let, _)))
        {
            Some(previous)
        } else {
            None
        };
        if let Some(position) = let_mut {
            let span = (self.token_span(position).start..self.token_span(position + 1).end).into();
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Rule(LET_MUT_IS_ONE_WORD),
                context,
                hint: None,
            });
            return;
        }
        // B414: a contextual keyword followed by a name, where its keyword
        // reading is not admitted — `let own x = 1;`, `let s = dyn Shape;` —
        // reads the word as a NAME and stops at the name after it. The word is
        // the whole mistake, so its placement rule replaces the message
        // (contextual-keywords.md §8; `parse_misplaced_resource` is the
        // precedent).
        if let Some(previous) = position.checked_sub(1)
            && matches!(self.tokens.get(position), Some((Token::Ident(_), _)))
            && let Some((Token::Ident(word), _)) = self.tokens.get(previous)
            && let Some(reason) = misplaced_contextual_keyword(word)
        {
            self.errors.push(ParseError {
                span: self.token_span(previous),
                reason,
                context,
                hint: None,
            });
            return;
        }
        // A missing statement terminator is not a "found X expected Y" — the token
        // at `position` is a perfectly good next statement, and the mistake is in
        // the whitespace before it (`editing-dx.md` §4.4). It reports at the gap,
        // and says `;`.
        if expected.iter().any(|one| one == TERMINATOR_EXPECTED) {
            let span = self.gap_span(position);
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::MissingTerminator,
                context,
                hint: None,
            });
            return;
        }
        let span = self.token_span(position);
        let found = self.found_at(position);
        let hint = self
            .soup_hint(position)
            .or_else(|| self.istring_brace_hint(position));
        self.errors.push(ParseError {
            span,
            reason: ParseErrorReason::Expected { found, expected },
            context,
            hint,
        });
    }

    /// The `pub`-style visibility marker standing immediately before `position`,
    /// when `position` starts a fresh statement or item — the shape
    /// `pub fun helper()` takes once `pub` lexes as the ordinary identifier it
    /// is. `public` is included: it is the same reflex, one synonym over.
    ///
    /// Answers the marker's token index AND its spelling, because the rule
    /// quotes the word that was written (E109) — and the spelling is `'static`
    /// because only these two literals ever match.
    fn visibility_marker_before(&self, position: usize) -> Option<(usize, &'static str)> {
        let previous = position.checked_sub(1)?;
        let marker: &'static str = match self.tokens.get(previous) {
            Some((Token::Ident("pub"), _)) => "pub",
            Some((Token::Ident("public"), _)) => "public",
            _ => return None,
        };
        let starts_fresh = starts_statement_or_item(self.tokens, position);
        starts_fresh.then_some((previous, marker))
    }

    /// Whether the failure at `position` lies inside an interpolation HOLE, in
    /// which case a `{` the author meant literally is the likeliest cause.
    ///
    /// Recognized structurally, from the token stream the lexer's desugaring
    /// leaves behind: a hole becomes a parenthesized group whose parens carry the
    /// `{…}` span, so the nearest enclosing opener whose source text begins with
    /// `{` IS a hole. The i-string's own wrapper parens carry the whole literal's
    /// span (which begins `i"`), and a real `(` begins with itself, so neither is
    /// mistaken for one.
    fn istring_brace_hint(&self, position: usize) -> Option<&'static str> {
        let mut depth = 0usize;
        for index in (0..position.min(self.tokens.len())).rev() {
            match self.tokens.get(index) {
                Some((Token::Ctrl(')'), _)) => depth += 1,
                Some((Token::Ctrl('('), span)) => {
                    if depth > 0 {
                        depth -= 1;
                    } else {
                        return self.source[span.start..]
                            .starts_with('{')
                            .then_some(BRACE_IN_AN_ISTRING);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// The anchor for a missing statement terminator: the LAST CHARACTER of the
    /// token before `position` — the gap where the `;` belongs (`editing-dx.md`
    /// §4.4, drawn under the `}` of `… y = 4 }`). One character rather than the
    /// zero-width point the same section also describes, because the editor is the
    /// instrument that matters and a zero-width LSP range draws as nothing there
    /// (§3.2); `line_index.rs` converts both ends verbatim, so nothing downstream
    /// would widen it. Char-aligned, so a token ending in a multi-byte character
    /// still yields a slice-able span.
    fn gap_span(&self, position: usize) -> Span {
        match position
            .checked_sub(1)
            .and_then(|previous| self.tokens.get(previous))
        {
            Some((_, span)) => {
                let start = self.source[span.start..span.end]
                    .chars()
                    .next_back()
                    .map_or(span.start, |last| span.end - last.len_utf8());
                (start..span.end).into()
            }
            None => self.token_span(position),
        }
    }

    /// Take the recorded farthest failure iff it lies strictly inside the token
    /// range `(start, end)` — deeper than an opening delimiter at `start` and before
    /// the region's close at `end`. Clears it, so the top-level leftover diagnostic
    /// does not also report it. A failure outside the region is stale (recorded by an
    /// earlier sibling) and left in place.
    fn take_failure_within(&mut self, start: usize, end: usize) -> Option<Failure> {
        match &self.farthest_failure {
            Some(failure) if failure.position > start && failure.position < end => {
                self.farthest_failure.take()
            }
            _ => None,
        }
    }

    /// The `!=` soup, recognized structurally (not by string matching, as the
    /// generated `parse_error_hint` must): `a!==b` lexes as `!=` then `=`, so the
    /// operand after `!=` is missing and the failure lands on that stray `=` whose
    /// immediately-preceding token is `!=`. A first-class message (§6a).
    fn soup_hint(&self, position: usize) -> Option<&'static str> {
        let found_equals = matches!(self.tokens.get(position), Some((Token::Op("="), _)));
        let after_not_equals = position
            .checked_sub(1)
            .and_then(|previous| self.tokens.get(previous))
            .is_some_and(|(token, _)| matches!(token, Token::Op("!=")));
        (found_equals && after_not_equals).then_some(
            "if this was postfix `!` before a comparison, the space is required: \
             `a! == b` (`!=` always lexes as not-equals)",
        )
    }

    /// The byte span of the token at `position`, or the zero-width EOI span.
    fn token_span(&self, position: usize) -> Span {
        match self.tokens.get(position) {
            Some((_, span)) => *span,
            None => (self.eoi..self.eoi).into(),
        }
    }

    /// What the parser found at `position`, for a `found <X>` message.
    fn found_at(&self, position: usize) -> Found {
        match self.tokens.get(position) {
            Some((token, _)) => Found::Token(token.to_string()),
            None => Found::EndOfInput,
        }
    }

    // --- Statement/item synchronization (frontend.md §2, editing-dx.md S1) ---

    /// Recover from a statement that declined at token `start`: report ONE
    /// diagnostic for it and move the cursor to the next statement/item boundary,
    /// so the statements after it are parsed instead of dropped.
    ///
    /// This is `frontend.md:137-140`'s ratified "statement/item boundaries
    /// synchronize on `;`/`}`/item keywords" clause, which the H6 cutover never
    /// built — both statement loops simply `break`, which is what
    /// `editing-dx.md` §2 measures as the *blackout*: everything after the first
    /// broken statement stops being parsed, so every diagnostic it would have
    /// produced disappears from the editor while the user types.
    ///
    /// Which diagnostic, in the order the survey's grades ask for:
    /// 1. a delimiter opened inside the statement and never closed, with no
    ///    committed demand failing *inside* it — the `print(1` mid-edit shape —
    ///    reports `unclosed \`(\`` at the OPENER (§5.3), not `found '}'` at a brace
    ///    the user never touched;
    /// 2. otherwise the farthest failure, which is the located "found X expected Y"
    ///    (§5.1's already-good separator/closer messages, and S2's missing-`;`);
    /// 3. otherwise — nothing recorded anything, so the statement's very first
    ///    token is unparseable — the position's own fallback.
    ///
    /// The cursor always advances at least one token, so the caller's loop cannot
    /// spin.
    ///
    /// Returns a statement RECOVERED from inside the abandoned region, when one
    /// exists — `editing-dx.md` §8 clause 3's one residual (§15.8/§17.5): a
    /// statement written where a call's arguments were expected (`print(` then
    /// `let swallowed: i32 = 2;`) is read as an argument, and lost with the whole
    /// abandoned call when its `attempt` backtracks. See
    /// [`Parser::recover_swallowed_statement`].
    fn recover_statement(&mut self, start: usize, in_block: bool) -> Option<Spanned<Node<'src>>> {
        let (resume, unclosed) = self.scan_to_sync_point(start, in_block);
        // A committed demand that failed strictly INSIDE the unclosed region
        // located the real error (`distance(1, 2;` fails at the `;`, wanting `,`
        // or `)`) — that message is better than naming the opener, and §5.1 grades
        // it the best in the survey. Only a region nothing complained inside of is
        // reported as unclosed.
        let inside = unclosed.and_then(|opener| self.take_failure_within(opener, resume));
        let swallowed = match (unclosed, inside) {
            (_, Some(failure)) => {
                self.emit_failure(failure.position, failure.expected, failure.context);
                unclosed.and_then(|opener| self.recover_swallowed_statement(opener, resume))
            }
            (Some(opener), None) => {
                let delimiter = match self.tokens.get(opener) {
                    Some((Token::Ctrl(character), _)) => *character,
                    _ => '(',
                };
                self.errors.push(ParseError {
                    span: self.token_span(opener),
                    reason: ParseErrorReason::Unclosed { delimiter },
                    context: self.context_stack.clone(),
                    hint: None,
                });
                self.farthest_failure = None;
                None
            }
            (None, None) => {
                self.emit_statement_failure(start, in_block);
                None
            }
        };
        // Unconditional, regardless of what `recover_swallowed_statement`'s own
        // attempt left `self.position` at (its own `attempt` already restores it
        // to `opener + 1` on decline): `resume` is the scan's own authoritative
        // answer, never second-guessed by the recovery it enables.
        self.position = resume;
        swallowed
    }

    /// The one case of `recover_statement`'s abandoned region that is not
    /// actually garbled: a call left unclosed by a statement typed where an
    /// argument was expected. `opener` and `resume` box that statement's tokens
    /// exactly — `scan_to_sync_point` stops at the first token that cannot be
    /// part of a parenthesized region, so `(opener + 1)..resume` is precisely
    /// "one statement, plus the `;` that ended it" whenever the abandoned
    /// region genuinely holds one. Retried from a fresh position as an ordinary
    /// statement; kept only when that retry both succeeds AND lands EXACTLY on
    /// `resume` — nothing more, nothing less — so a region shaped any other way
    /// (more than one statement, a genuinely garbled head, a nested unclosed
    /// delimiter of its own) declines rather than guesses, and the caller's
    /// existing jump-to-`resume` behavior is exactly as before it.
    ///
    /// Scoped to `(` on purpose: `[value; length]`'s own `;` fork and a
    /// block's `{ statement* }` already parse their contents as themselves, so
    /// only a call's argument list can swallow a foreign statement whole.
    fn recover_swallowed_statement(
        &mut self,
        opener: usize,
        resume: usize,
    ) -> Option<Spanned<Node<'src>>> {
        if !matches!(self.tokens.get(opener), Some((Token::Ctrl('('), _))) {
            return None;
        }
        self.position = opener + 1;
        let recovered = self
            .attempt(Self::parse_statement)
            .filter(|_| self.position == resume);
        // Either outcome may have left farthest-failure tracking pointed at a
        // committed demand from INSIDE the region this call already reported
        // on (the retry's own clean demands on success; `attempt`'s rollback
        // does not touch this field on decline) — cleared so it cannot be
        // mistaken for a later statement's failure.
        self.farthest_failure = None;
        recovered
    }

    /// Recover a statement whose only fault is the missing `;`: the body parsed to
    /// completion and the token after it can only begin a fresh statement or item
    /// (or end the block, or the file), so the terminator is the one thing absent.
    /// Report it at the gap and KEEP the statement.
    ///
    /// Skipping it instead would be honest about the syntax and wrong about the
    /// program: dropping `import std::io::print` because its `;` is missing unbinds
    /// `print` at every call site below, and dropping `let origin = …` unbinds
    /// `origin` — a screenful of "cannot find" on lines that are correct, from a
    /// statement the parser read perfectly well. This is `editing-dx.md` §8
    /// clause 3 in the direction the survey did not measure: a parse error that
    /// must not remove a diagnostic must not manufacture one either.
    ///
    /// The boundary condition is what keeps it from cascading. Only a KEYWORD
    /// head (or `}`, or end of input) counts, never an identifier or a literal:
    /// `print 1);` would resume at `1`, which is not a statement anyone wrote, so
    /// it takes the skipping path and reports once (§5.2's accepted outcome).
    ///
    /// The ONE statement that is dropped rather than kept is the one that IS the
    /// mistake: a `pub`/`public` visibility marker, which reads as a bare
    /// identifier expression binding nothing (E109 — see
    /// [`TerminatorRecovery::Dropped`]).
    fn recover_missing_terminator(&mut self) -> TerminatorRecovery<'src> {
        let Some(statement) = self.attempt(|parser| {
            // The three forms that take a terminator — the same three
            // `note_terminator` records one for.
            let head = parser.position;
            let body = parser
                .attempt(Self::parse_statement_expression)
                .map(|expression| parser.read_at_statement_position(expression, head))
                .or_else(|| parser.attempt(Self::parse_import))
                .or_else(|| parser.attempt(Self::parse_use))?;
            let at_a_fresh_statement = parser.at_end()
                || parser.peek_is_ctrl('}')
                || starts_statement_or_item(parser.tokens, parser.position);
            at_a_fresh_statement.then_some(body)
        }) else {
            return TerminatorRecovery::Declined;
        };
        // Asked BEFORE the report, of the same predicate `emit_failure` routes
        // on, so "the visibility rule was emitted" and "the statement is the
        // marker" can never disagree.
        let marker = self.visibility_marker_before(self.position).is_some();
        self.emit_failure(
            self.position,
            vec![TERMINATOR_EXPECTED.to_string()],
            Vec::new(),
        );
        match marker {
            true => TerminatorRecovery::Dropped,
            false => TerminatorRecovery::Kept(statement),
        }
    }

    /// The located diagnostic for a statement that declined at `start`: the
    /// farthest failure when it lies at or past the statement's head (the deepest
    /// the parser got), else the head token itself with the position's own
    /// expectation — an item at file scope, a statement inside a block. Mirrors
    /// [`Parser::emit_leftover_error`], which is the same choice made once at the
    /// end of a parse that stopped instead of synchronizing.
    fn emit_statement_failure(&mut self, start: usize, in_block: bool) {
        match self.farthest_failure.take() {
            Some(failure) if failure.position >= start => {
                self.emit_failure(failure.position, failure.expected, failure.context)
            }
            _ => {
                let expected = if in_block {
                    vec!["a statement".to_string(), "'}'".to_string()]
                } else {
                    vec!["an item".to_string(), "end of input".to_string()]
                };
                self.emit_failure(start, expected, Vec::new())
            }
        }
    }

    /// Scan forward from the declined statement at `start` for the next
    /// statement/item boundary, WITHOUT moving the cursor. Returns the token index
    /// to resume at — always `> start` — and the index of the outermost delimiter
    /// still open when the scan stopped, if any.
    ///
    /// The boundaries are `frontend.md`'s: a `;` at statement depth (resume after
    /// it), a `}` closing the enclosing block (resume ON it, so the block closes
    /// normally), and an item keyword. A statement-head keyword joins them —
    /// `let`/`mut`/`ret`/`if`/`for`/`match` cannot continue the broken statement,
    /// and stopping there is what keeps the *next* statement's diagnostics alive
    /// after a missing `;` (§8 clause 3) instead of swallowing it to the following
    /// terminator.
    ///
    /// Delimiters nest, and a `}` that does not match the innermost opener ends the
    /// scan: the enclosing block's closer outranks a region the user has not
    /// finished typing, which is exactly how `print(` stops eating the rest of the
    /// file (§2.2 mechanism 3). At FILE scope there is no enclosing block, so a
    /// stray closer is consumed rather than stopped at — otherwise `}}}}` would
    /// report once per brace.
    ///
    /// Two of the boundaries reach INSIDE an unfinished region, because the token
    /// they stop on cannot be part of one:
    /// - a `;` whose innermost opener is `(` — a call's arguments and a
    ///   parenthesized expression admit no semicolon (the constructs that do,
    ///   `[value; length]` and a block's statements, sit under `[` or `{`), so it
    ///   terminates a statement written BELOW the region;
    /// - an ITEM keyword with no `{` open above it — `fun`/`struct`/`impl`/… are
    ///   never part of a parenthesized expression, so an unclosed `(` in an item's
    ///   own header stops eating the items below it. A `{` on the stack means a
    ///   block or closure body, where a nested item is ordinary code, so the scan
    ///   keeps going there. Statement heads get no such reach: `if`/`match`/`async`
    ///   are perfectly good arguments.
    fn scan_to_sync_point(&self, start: usize, in_block: bool) -> (usize, Option<usize>) {
        let mut open: Vec<(usize, char)> = Vec::new();
        let mut index = start;
        while let Some((token, _)) = self.tokens.get(index) {
            let outermost_open = open.first().map(|(at, _)| *at);
            if let Token::Ctrl(character) = token {
                let character = *character;
                if matches!(character, '(' | '[' | '{') {
                    open.push((index, matching_close(character)));
                    index += 1;
                    continue;
                }
                if matches!(character, ')' | ']' | '}') {
                    if open.last().is_some_and(|(_, closer)| *closer == character) {
                        open.pop();
                        index += 1;
                        continue;
                    }
                    if character == '}' && in_block {
                        return (index, outermost_open);
                    }
                    index += 1;
                    continue;
                }
                if character == ';' && open.last().is_none_or(|(_, closer)| *closer == ')') {
                    return (index + 1, outermost_open);
                }
                index += 1;
                continue;
            }
            if index > start {
                let inside_a_block = open.iter().any(|(_, closer)| *closer == '}');
                let stops = if open.is_empty() {
                    starts_statement_or_item(self.tokens, index)
                } else {
                    !inside_a_block && starts_item(token)
                };
                if stops {
                    return (index, outermost_open);
                }
            }
            index += 1;
        }
        (index, open.first().map(|(at, _)| *at))
    }

    // --- Delimiter recovery (chumsky `nested_delimiters`) --------------------

    /// Reproduce chumsky's `nested_delimiters(open, close, others, fallback)`
    /// recovery (recovery.rs): from an opening `open`, skip a balanced region —
    /// nesting the `others` pairs as well as `open`/`close` — up to the matching
    /// `close`, then report the region and return its span; the caller maps the
    /// span to the site's placeholder. Precondition: the site's clean parse just
    /// failed and rewound to `open`. If the cursor is NOT at `open`, or the region
    /// cannot be balanced (an unbalanced inner delimiter, or EOI first — the exact
    /// case chumsky hard-fails on), the cursor is left untouched and `None` is
    /// returned (the caller declines too).
    fn recover_delimited(
        &mut self,
        production: &'static str,
        open: char,
        close: char,
        others: &[(char, char)],
    ) -> Option<Span> {
        if !self.peek_is_ctrl(open) {
            return None;
        }
        let end = self.scan_balanced(self.position, open, close, others)?;
        let start = self.position;
        self.position = end;
        let span = self.span_from(start);
        // `scan_balanced` only returns here when the region CLOSED. If a farthest
        // failure was recorded strictly inside it — a committed demand
        // (`expect_ctrl`/`expect_op`, e.g. a missing separator or closer) or a
        // semantic leaf that hit the real error — surface THAT, with its context and
        // the `!=`-soup hint: the located "found X expected Y", never a false
        // "unclosed" on a region that did close. Only a GENUINELY GARBLED region —
        // whose content declines at its very first token, committing to nothing
        // (`struct S { 1 2 3 }`, `fun f<1 2 3>`) — reaches the `Unbalanced` fallback,
        // which names the production and its opener. (That fallback's "unclosed"
        // wording is imprecise for a closed region; it is kept as the last-resort
        // production-namer, pinned at `render_names_the_unclosed_delimiter`.)
        match self.take_failure_within(start, end) {
            Some(failure) => self.emit_failure(failure.position, failure.expected, failure.context),
            // A region that committed to nothing but holds a misused `css` (the
            // word not heading a block) garbled BECAUSE of it: `struct S { css:
            // str }` declines at its first token, so nothing inside was noted
            // and the fallback would name the struct body instead of the word.
            None if self.css_misuse_within(start, end).is_some() => {
                let span = self
                    .css_misuse_within(start, end)
                    .expect("just matched as present");
                self.errors.push(ParseError {
                    span,
                    reason: ParseErrorReason::Rule(CSS_IS_A_KEYWORD),
                    context: self.context_stack.clone(),
                    hint: None,
                });
            }
            None => self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Unbalanced {
                    production,
                    delimiter: open,
                },
                context: self.context_stack.clone(),
                hint: None,
            }),
        }
        Some(span)
    }

    /// The span of the first MISUSED `css` in the token range `start..end`: the
    /// keyword written anywhere but at the head of a `css { … }` block, which is
    /// the only shape it has. Definitively an error wherever it appears, so a
    /// recovery that has nothing better to say says this
    /// (proposal/css-block.md §5.4, Q3).
    fn css_misuse_within(&self, start: usize, end: usize) -> Option<Span> {
        self.tokens
            .get(start..end)?
            .iter()
            .enumerate()
            .find(|(offset, (token, _))| {
                *token == Token::Css
                    && !matches!(
                        self.tokens.get(start + offset + 1),
                        Some((Token::Ctrl('{'), _))
                    )
            })
            .map(|(_, (_, span))| *span)
    }

    /// Scan a balanced `open..close` region starting at token index `start` (which
    /// must hold `open`), nesting `open`/`close` and every `others` pair, and
    /// return the index one past the matching `close` — or `None` if a closing
    /// delimiter appears unbalanced or the input ends first. A faithful, non-
    /// backtracking reading of the chumsky `nested_delimiters` grammar (a repeated
    /// choice of a nested balanced block or a non-delimiter token): the two agree
    /// on accept/reject and on the consumed region. Pure over the token slice.
    fn scan_balanced(
        &self,
        start: usize,
        open: char,
        close: char,
        others: &[(char, char)],
    ) -> Option<usize> {
        let is_opener =
            |character: char| character == open || others.iter().any(|&(o, _)| o == character);
        let closer_for = |character: char| -> Option<char> {
            if character == open {
                Some(close)
            } else {
                others
                    .iter()
                    .find(|&&(o, _)| o == character)
                    .map(|&(_, c)| c)
            }
        };
        let is_closer =
            |character: char| character == close || others.iter().any(|&(_, c)| c == character);

        // A stack of expected closers; `start` holds `open`, so seed with `close`.
        let mut expected_closers = vec![close];
        let mut index = start + 1;
        while let Some(top) = expected_closers.last().copied() {
            let (token, _) = self.tokens.get(index)?;
            if let Token::Ctrl(character) = token {
                let character = *character;
                if character == top {
                    expected_closers.pop();
                    index += 1;
                } else if is_opener(character) {
                    expected_closers.push(closer_for(character).unwrap());
                    index += 1;
                } else if is_closer(character) {
                    // A closer that does not match the innermost open: the chumsky
                    // `repeated` stops here and the enclosing delimiter fails.
                    return None;
                } else {
                    // A non-delimiter control token (`.`, `,`, `;`): consumed.
                    index += 1;
                }
            } else {
                // Any non-control token: consumed.
                index += 1;
            }
        }
        Some(index)
    }

    /// `item (',' item)* ','?` up to (but not consuming) the closer `is_close`
    /// reports — the `allow_trailing` comma-list shared by argument lists, generic
    /// arguments, list literals, tuple types, and closure parameters. An empty list
    /// (the closer immediately) is allowed; the caller enforces any minimum.
    fn comma_list<T>(
        &mut self,
        mut item: impl FnMut(&mut Self) -> Option<T>,
        is_close: impl Fn(&Self) -> bool,
    ) -> Option<Vec<T>> {
        let mut items = Vec::new();
        if is_close(self) {
            return Some(items);
        }
        loop {
            items.push(item(self)?);
            if self.eat_ctrl(',') {
                if is_close(self) {
                    break;
                }
                continue;
            }
            // No separator: the list ends here. Record (for diagnostics) that a `,`
            // — another item — was ALSO admissible, so if the caller's closer is
            // then missing the report reads "expected `,` or <closer>" at this
            // token, not the bare closer alone. On a clean parse the closer follows
            // and this note is never surfaced.
            self.note_expected("','");
            break;
        }
        Some(items)
    }

    // --- Program / statements ------------------------------------------------

    /// The whole source: a sequence of statements. A token that begins no statement
    /// is REPORTED and SYNCHRONIZED past ([`Parser::recover_statement`]) rather than
    /// stopping the parse, so an item after a broken one still reaches the analyzer
    /// (`editing-dx.md` §2.2 mechanism 3 — the file-tail blackout).
    fn parse_program(&mut self) -> Spanned<NodeList<'src>> {
        let mut statements = Vec::new();
        self.file_head = true;
        loop {
            if self.at_end() {
                break;
            }
            match self.parse_statement() {
                Some(statement) => statements.push(statement),
                None => match self.recover_missing_terminator() {
                    TerminatorRecovery::Kept(statement) => statements.push(statement),
                    TerminatorRecovery::Dropped => {}
                    TerminatorRecovery::Declined => {
                        if let Some(swallowed) = self.recover_statement(self.position, false) {
                            statements.push(swallowed);
                        }
                    }
                },
            }
        }
        (statements, self.span_from(0))
    }

    /// One statement, reproducing the chumsky `statement` choice in ORDER — the
    /// ordering is load-bearing (it decides which reading of an ambiguous `[`- or
    /// `async`-led head wins). Each alternative backtracks cleanly on a mismatch,
    /// so the first that matches wins, exactly as chumsky's ordered `choice`.
    /// Returns `None` (restoring) when no statement starts here, so the caller's
    /// loop can stop and a trailing block value can be taken instead.
    ///
    /// The chumsky order (parser.rs `statement.define`):
    ///
    /// 1. `[derive(..)] struct|enum`
    /// 2. `[service(..)] struct`
    /// 3. `[<user>(..)] struct|enum|fun`
    /// 4. `macro fun`
    /// 5. `macro { } ;?`
    /// 6. `macro name(..) ;?`
    /// 7. `export <stmt>`
    /// 8. `expression ;`
    /// 9. through 11: `if` / `for` / `match` without `;` (not-block-end)
    /// 12. `fun`
    /// 13. `struct`
    /// 14. `enum`
    /// 15. misplaced-`resource` steer
    /// 16. `impl`
    /// 17. `trait`
    /// 18. `mod`
    /// 19. `import ;`
    /// 20. `use ;`
    /// 21. `{ } block` without `;` (not-block-end)
    ///
    /// Items 8-11 and 21 are fused into one expression attempt (an expression
    /// carries the block-bearing forms and the bare block already), exactly as
    /// S2 did.
    fn parse_statement(&mut self) -> Option<Spanned<Node<'src>>> {
        // Item nesting is its own recursive grammar and reaches no expression
        // rule (B142): `fun a() { fun a() { .. } }`, `mod`, `impl` and `trait`
        // all close their cycle back through here. A chained `export` did
        // until B492: a repeated marker is refused and read past, so it no
        // longer nests.
        //
        // The stand-in CONSUMES a token, which the other funnels' does not, and
        // that is what keeps the refusal linear here: `parse_program` and
        // `parse_block_clean` loop on a `Some`, so a success that consumed
        // nothing would spin forever, while declining sends all 500 open frames
        // back through their remaining alternatives — measured, `fun a() { .. }`
        // at 505 levels went from 9 ms to not finishing in 25 s. One token per
        // refusal makes each turn of those loops O(1) and strictly advancing.
        self.parse_nested_as(
            Self::ITEM_NESTING_REFUSAL,
            |parser, span| {
                parser.bump();
                Some((Node::Error, span))
            },
            Self::parse_statement_inner,
        )
    }

    /// [`Parser::parse_statement`]'s body, past the depth bound.
    fn parse_statement_inner(&mut self) -> Option<Spanned<Node<'src>>> {
        // B415: only the file's first statement may be its `mod self;` (the
        // host of F27 R1's platform). Taken here, once, so every statement
        // nested inside this one reads false.
        let file_head = std::mem::take(&mut self.file_head);
        let _ = self.lead_export_past_its_attributes();
        self.take_foreign_item_word();
        self.canonicalize_marker_run();
        if let Some(item) = self.attempt(Self::parse_module_self) {
            if !file_head {
                self.errors.push(ParseError {
                    span: item.1,
                    reason: ParseErrorReason::Rule(MODULE_SELF_LEADS_THE_FILE),
                    context: self.context_stack.clone(),
                    hint: None,
                });
            }
            return Some(item);
        }
        // G24's `const let` / `const fun` / `const mut`, ahead of everything:
        // `const` begins no other statement, and the expression fork below
        // would otherwise read `const let` as its prefix over a `let`
        // expression and `const fun` as a missing expression.
        if let Some(item) = self.attempt(Self::parse_const_declaration) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_derived_item) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_service_item) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_macro_attributed_item) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_macro_fun) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_macro_block_statement) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_macro_invocation_statement) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_export) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_labelled_let) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_misplaced_async_binding) {
            return Some(item);
        }
        // Items 8-11 & 21: `expression ;`, or a block-bearing form
        // (`if`/`for`/`match`/`{ }`) used as a statement — which needs no `;` but
        // must not be the last thing in its block (chumsky's `not_block_end`).
        if let Some(statement) = self.attempt(|parser| {
            let head = parser.position;
            let expression = parser.parse_statement_expression()?;
            if parser.eat_ctrl(';') {
                return Some(parser.read_at_statement_position(expression, head));
            }
            // A block-bearing form needs no `;` (chumsky's `not_block_end`). Its
            // own continuation rule is stated where the form is parsed
            // (`refuse_block_like_continuation`), so by here nothing operator-like
            // is left for this fork to read.
            if is_block_like(&expression.0) && !parser.peek_is_ctrl('}') {
                return Some(expression);
            }
            // The expression is complete and its `;` is not there. Record the
            // demand (S2). A block-bearing form only reaches here at the end of its
            // block, where it is the block's VALUE and wants no `;`.
            if !is_block_like(&expression.0) {
                parser.note_terminator();
            }
            None
        }) {
            return Some(statement);
        }
        if let Some(item) = self.attempt(Self::parse_function) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_struct) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_enum) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_misplaced_resource) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_impl) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_trait) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_module) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_import_statement) {
            return Some(item);
        }
        if let Some(item) = self.attempt(Self::parse_use_statement) {
            return Some(item);
        }
        None
    }

    /// B445: a declaration's attribute run may be written AHEAD of its
    /// `export` — `[platform("browser")] export impl P with Show { … }` — as
    /// well as after it, and either way the run is the ITEM's prefix: the
    /// marker wraps the declaration whole (§3.2), attributes and all.
    ///
    /// Read by rotating the tokens, once, where the statement begins: the
    /// run `[..] [..]` and the marker `export (in PATH)?` after it trade
    /// places, so every production downstream reads the one spelling it
    /// always read, `export [..] item`, and the attribute prefixes keep the
    /// single order each already states rather than gaining a second. Each
    /// token keeps its own span; [`Parser::parse_export`] widens the export's
    /// span back over the run, so the statement still begins where it was
    /// written. The assignment table rotates with the tokens it describes.
    ///
    /// Only an ATTRIBUTE run leads: each group is `[` followed by a name, and
    /// the run must end at the marker. A list literal there (`[1] export …`)
    /// is the expression statement it always was, and a run before `export
    /// *;` is left for the statement reader to refuse — `*;` takes no
    /// attributes. Nothing else in the grammar puts a `[name …]` group in
    /// front of `export`, so a rotation is never undone: a later alternative
    /// at this position reads the same rotated stream, and finds no run to
    /// rotate again.
    fn lead_export_past_its_attributes(&mut self) -> bool {
        let start = self.position;
        let mut at = start;
        while self.tokens.get(at).map(|(token, _)| token) == Some(&Token::Ctrl('['))
            && matches!(self.tokens.get(at + 1), Some((Token::Ident(_), _)))
        {
            let Some(after) = self.past_balanced_group(at) else {
                return false;
            };
            at = after;
        }
        let marker = at;
        if marker == start
            || self.tokens.get(marker).map(|(token, _)| token) != Some(&Token::Export)
        {
            return false;
        }
        // The marker's scope travels with it — and so does a REPEATED marker,
        // with or without a scope of its own (B492): `[..] export export fun`
        // reaches `parse_export` as `export export [..] fun` and takes the
        // repeat's refusal there, rather than nesting one export in another.
        let mut past_marker = marker + 1;
        loop {
            let at = self.tokens.get(past_marker).map(|(token, _)| token);
            if at == Some(&Token::Export) {
                past_marker += 1;
            } else if at == Some(&Token::Ctrl('('))
                && self.tokens.get(past_marker + 1).map(|(token, _)| token) == Some(&Token::In)
            {
                let Some(after) = self.past_balanced_group(past_marker) else {
                    return false;
                };
                past_marker = after;
            } else {
                break;
            }
        }
        if matches!(self.tokens.get(past_marker), Some((Token::Op("*"), _))) {
            return false;
        }
        self.tokens[start..past_marker].rotate_left(marker - start);
        self.assignment_reachable[start..past_marker].rotate_left(marker - start);
        true
    }

    /// Records the refusal for a token rewrite ([`Parser::rewrite_refusals`]),
    /// once per span: a rewrite reached again by a later alternative is the
    /// same rewrite.
    fn record_rewrite(&mut self, span: Span, reason: ParseErrorReason) {
        if self
            .rewrite_refusals
            .iter()
            .any(|refusal| refusal.span == span)
        {
            return;
        }
        self.rewrite_refusals.push(ParseError {
            span,
            reason,
            context: Vec::new(),
            hint: None,
        });
    }

    /// B485 and B536 (RULED): the declaration head at the cursor — a run of
    /// attribute groups and marker keywords ending at a declaration word — is
    /// put into the order the productions read, once, before any of them
    /// reads it. THE order it is WRITTEN in is the attributes by
    /// [`attribute_rank`], then the keywords — `export`, `const`|`lazy`,
    /// `async`, `external`|`macro` (Q8) — then the declaration word (Q6).
    ///
    /// - **Attributes out of rank** (and nothing else out of order) are read
    ///   in rank and WARNED, once, spanning the run
    ///   ([`ParseErrorReason::AttributeOrder`], B536 revising Q7's free
    ///   order): `[internal(..)] [deprecated(..)] fun` reads as
    ///   `[deprecated(..)] [internal(..)] fun`, which `vilan fmt` writes.
    /// - **A keyword ahead of an attribute, or two keywords inverted**, is
    ///   REFUSED once, spanning the run, with the head respelled in the order
    ///   ([`ParseErrorReason::MarkerOrder`]), and read as if written so:
    ///   `async [platform(..)] fun`, `external async fun`, `lazy export let`.
    ///   That includes `export` and `macro` ahead of the attributes (B485 S3,
    ///   v0.44.0): `export [must_use] fun`, the order before B485, and
    ///   `macro [deprecated(..)] fun`.
    ///
    /// The WRITTEN order is read off the tokens' spans, not their places on
    /// the stream: [`Parser::lead_export_past_its_attributes`] has already
    /// rotated a canonical `[..] export` into the `export [..]` the
    /// productions read, and each token kept its own span.
    ///
    /// The run is rewritten by permuting whole units on the token stream, as
    /// that rotation does; every token keeps its own span, and
    /// [`Parser::written_starts`] keeps each node beginning where its first
    /// unit was written. Only a run that ENDS at a declaration word is touched
    /// — `[a][b];` is a list indexed by a list, `async { .. }` a block — and
    /// a run in THE order reads exactly as it did before B486. A keyword set
    /// no order makes legal (`async const fun`, `lazy const let`) is left as
    /// written, for its production's own refusal.
    fn canonicalize_marker_run(&mut self) {
        let start = self.position;
        // A run already put in reading order — at its head, or at any suffix
        // a production reads again from (`export`'s statement, `const`'s
        // declaration) — is not a written one: `const [deprecated(..)] let`
        // there is this pass's own output, not the author's.
        if self
            .written_starts
            .iter()
            .any(|(position, _)| *position == start)
        {
            return;
        }
        let Some(MarkerRun {
            units,
            word,
            word_at: at,
        }) = marker_run_at(self.tokens, start)
        else {
            return;
        };
        if units.len() < 2 {
            return;
        }
        // A REPEATED keyword is its own refusal (`export export`, B492) or its
        // production's, read where it was written.
        let mut keywords_seen: Vec<MarkerKeyword> = Vec::new();
        for unit in &units {
            if let MarkerUnitKind::Keyword(keyword) = unit.kind {
                if keywords_seen.contains(&keyword) {
                    return;
                }
                keywords_seen.push(keyword);
            }
        }
        let mut written = units.clone();
        written.sort_by_key(|unit| self.token_span(unit.tokens.start).start);
        // Out of THE order: a keyword ahead of an attribute (Q6; `export` and
        // `macro` too since B485 S3), or two keywords inverted (Q8) — refused;
        // or, short of either, two attributes out of rank (B536) — warned.
        let mut out_of_order = false;
        let mut out_of_rank = false;
        let mut keyword_written = false;
        let mut highest_keyword = None;
        let mut highest_attribute = None;
        for unit in &written {
            match unit.kind {
                MarkerUnitKind::Attribute { name, .. } => {
                    out_of_order |= keyword_written;
                    let rank = attribute_rank(name);
                    out_of_rank |= highest_attribute.is_some_and(|highest| rank < highest);
                    highest_attribute = highest_attribute.max(Some(rank));
                }
                MarkerUnitKind::Keyword(keyword) => {
                    let rank = keyword.written_rank();
                    out_of_order |= highest_keyword.is_some_and(|highest| rank < highest);
                    highest_keyword = highest_keyword.max(Some(rank));
                    keyword_written = true;
                }
            }
        }
        let mut canonical: Vec<MarkerUnitKind<'src>> =
            written.iter().map(|unit| unit.kind).collect();
        canonical.sort_by_key(|kind| kind.written_key());
        let keywords: Vec<MarkerKeyword> = canonical
            .iter()
            .filter_map(|kind| match kind {
                MarkerUnitKind::Keyword(MarkerKeyword::Export) => None,
                MarkerUnitKind::Keyword(keyword) => Some(*keyword),
                MarkerUnitKind::Attribute { .. } => None,
            })
            .collect();
        // A keyword set no order makes legal is read as written, in whatever
        // order: reordering it would only trade one refusal for another, and
        // could hand a production a spelling it reads (`lazy const let`
        // becoming the `const` prefix over `lazy let`).
        if !marker_keywords_are_legal(&keywords, word) {
            return;
        }
        let mut reading = units.clone();
        reading.sort_by_key(|unit| unit.kind.reading_key());
        let reordered = reading
            .iter()
            .zip(&units)
            .any(|(read, written)| read.tokens != written.tokens);
        // `const [deprecated(..)] fun` is already in the order the production
        // reads, and still out of the written one: refused, not reordered.
        if !reordered && !out_of_order && !out_of_rank {
            return;
        }
        // The run as WRITTEN: from its first unit to the end of its last —
        // which, after `lead_export_past_its_attributes`' rotation, is not
        // the token ahead of the word.
        let run_span = Span::from(
            units
                .iter()
                .map(|unit| self.token_span(unit.tokens.start).start)
                .min()
                .unwrap_or_default()
                ..units
                    .iter()
                    .map(|unit| self.token_span(unit.tokens.end - 1).end)
                    .max()
                    .unwrap_or_default(),
        );
        // Each suffix of the reading order begins where the earliest of its
        // units was written — and is marked as read, so a production reading
        // on from it (`export`'s statement, `const`'s declaration) does not
        // take this pass's output for a written run and refuse it again.
        let mut position = start;
        for (index, unit) in reading.iter().enumerate() {
            let earliest = reading[index..]
                .iter()
                .map(|later| self.token_span(later.tokens.start).start)
                .min()
                .unwrap_or_default();
            self.written_starts.push((position, earliest));
            position += unit.tokens.len();
        }
        if reordered {
            let tokens: Vec<Spanned<Token<'src>>> = reading
                .iter()
                .flat_map(|unit| self.tokens[unit.tokens.clone()].iter().cloned())
                .collect();
            let reachable: Vec<bool> = reading
                .iter()
                .flat_map(|unit| {
                    self.assignment_reachable[unit.tokens.clone()]
                        .iter()
                        .copied()
                })
                .collect();
            self.tokens[start..at].clone_from_slice(&tokens);
            self.assignment_reachable[start..at].copy_from_slice(&reachable);
        }
        let mut spelled: Vec<String> = canonical.iter().map(|kind| kind.spelled()).collect();
        spelled.push(word.to_string());
        let canonical = spelled.join(" ");
        // B536 (v0.45.0, R-c): attributes out of rank are refused as the
        // keyword orders are — read as written in THE order, so the analysis
        // goes on and `vilan fmt` still writes the migration — where they
        // warned for a release.
        if out_of_order {
            self.record_rewrite(run_span, ParseErrorReason::MarkerOrder { canonical });
        } else if out_of_rank {
            self.record_rewrite(run_span, ParseErrorReason::AttributeOrder { canonical });
        }
    }

    /// B520: `fn`/`function`/`func`/`def` at the ITEM HEAD at the cursor —
    /// past any attribute run and marker keywords (`export`, `async`,
    /// `external`, `const`, `macro`) — followed by a name and the `(` or `<`
    /// that opens a signature, is refused once and rewritten to `fun` in
    /// place, so every production reads the declaration it is.
    ///
    /// A name followed by a name is never an expression, and none of the four
    /// words is a contextual keyword, so nothing that parses today reaches
    /// the rewrite. `then` is the one exception the name test makes (`fn then
    /// (go());` is B459's conditional over a binding named `fn`). In a member
    /// body a method may be named by a reserved word, so one is admitted
    /// there as the name — except `else`, `is` and `in`, which continue an
    /// operand.
    fn take_foreign_item_word(&mut self) {
        let mut at = self.position;
        loop {
            match self.tokens.get(at).map(|(token, _)| token) {
                Some(Token::Ctrl('['))
                    if matches!(self.tokens.get(at + 1), Some((Token::Ident(_), _))) =>
                {
                    let Some(after) = self.past_balanced_group(at) else {
                        return;
                    };
                    at = after;
                }
                Some(Token::Export) => {
                    at += 1;
                    if self.tokens.get(at).map(|(token, _)| token) == Some(&Token::Ctrl('('))
                        && self.tokens.get(at + 1).map(|(token, _)| token) == Some(&Token::In)
                    {
                        let Some(after) = self.past_balanced_group(at) else {
                            return;
                        };
                        at = after;
                    }
                }
                Some(Token::Async | Token::External | Token::Const | Token::Macro) => at += 1,
                _ => break,
            }
        }
        let Some((Token::Ident(word), span)) = self.tokens.get(at) else {
            return;
        };
        let Some(spelling) = ForeignSpelling::item_word(word) else {
            return;
        };
        let span = *span;
        let named = match self.tokens.get(at + 1).map(|(token, _)| token) {
            Some(Token::Ident(name)) => *name != "then",
            Some(Token::Else | Token::Is | Token::In) => false,
            Some(token) => self.in_member_body && is_reserved_word(token),
            None => false,
        };
        let opens_signature = matches!(
            self.tokens.get(at + 2).map(|(token, _)| token),
            Some(Token::Ctrl('(' | '<'))
        );
        if !named || !opens_signature {
            return;
        }
        self.tokens[at].0 = Token::Fun;
        self.record_rewrite(span, ParseErrorReason::ForeignSpelling(spelling));
    }

    /// B520: the identifier `return` at the cursor, where it begins a foreign
    /// return ([`starts_foreign_return`]), refused once and rewritten to `ret`
    /// in place, then read as the return it is. Its own method so the arm in
    /// [`Parser::parse_secondary_inner`] adds nothing to that frame.
    #[inline(never)]
    fn parse_foreign_return(&mut self) -> Option<Spanned<Node<'src>>> {
        let span = self.here_span();
        self.tokens[self.position].0 = Token::Ret;
        self.record_rewrite(
            span,
            ParseErrorReason::ForeignSpelling(ForeignSpelling::Return),
        );
        self.parse_return()
    }

    /// B520: the `->` arrow at the cursor, where a return type's `:` may
    /// stand (or, `spelling` [`ForeignSpelling::TypeArrow`], where a closure
    /// type's result may) — refused once and read past, so the caller reads
    /// the return type the arrow introduced. `false`, consuming nothing, when the cursor
    /// is not at an arrow.
    fn eat_foreign_arrow(&mut self, spelling: ForeignSpelling) -> bool {
        if !is_foreign_arrow(self.tokens, self.position) {
            return false;
        }
        let span = Span::from(
            self.token_span(self.position).start..self.token_span(self.position + 1).end,
        );
        self.record_rewrite(span, ParseErrorReason::ForeignSpelling(spelling));
        self.bump();
        self.bump();
        true
    }

    /// The index just past the bracket group opening at `open` (a `[`, `(` or
    /// `{`), counting every bracket kind, or `None` if the stream ends first.
    fn past_balanced_group(&self, open: usize) -> Option<usize> {
        past_balanced_group_in(self.tokens, open)
    }

    // --- Expressions ---------------------------------------------------------

    /// How deep the parser will descend into nested source before it refuses
    /// (B142), the companion to the analyzer's `WALK_DEPTH_LIMIT` and
    /// `RETURN_DEPTH_LIMIT` and chosen the same way: a large multiple of the
    /// deepest realistic nesting. `VILAN_DEPTH_STATS`'s `parse` family, swept
    /// over all 211 compilable corpus entries (std, every `vilan/test` fixture,
    /// the examples, the benchmarks and `macro_std`), peaks at **23** levels in
    /// the deepest single file with a median of 14 — so 500 is 21.7x the worst
    /// realistic case.
    ///
    /// That multiple reads lower than B138's 25x, and the reason is the counter,
    /// not the margin: B138 measured a counter that only ever counted expression
    /// levels, and the same corpus peaks at 15 there — 33x. This counter
    /// aggregates six grammars (below), so its levels are finer-grained than
    /// source nesting. In SOURCE terms the headroom is the larger figure; 21.7x
    /// is the conservative way to state it, so it is the one stated.
    ///
    /// It is deliberately the SAME number as `WALK_DEPTH_LIMIT`, though the two
    /// are complementary rather than redundant. The parser runs first and its
    /// counter climbs faster, so for SYNTACTIC nesting the parser always refuses
    /// first; the walk's bound covers what is deep to the walk but flat to the
    /// parser (a method chain — see `a_5000_deep_expression_is_refused_cleanly`).
    /// Sharing the number means a writer who flattens far enough for one has
    /// flattened far enough for the other, rather than fixing a program twice.
    ///
    /// **What the bound buys.** A finite worst case where there was none, which
    /// is what the stack margins are sized from: measured through the CLI on the
    /// worst plant (5000 nested parentheses), a bounded parse peaks at depth 501
    /// and **16.24 MiB** unoptimized, 3.93 MiB optimized, against no ceiling at
    /// all before. (Re-measured for N101 — the record said 35.2 MiB and ~10 MiB,
    /// 2.2× what `VILAN_DEPTH_STATS` reads at this sha; `deep_nesting.rs`'s
    /// parse pins carry the method and what is and is not settled about the
    /// difference. `vilan check` and not `vilan build`: on a plant the bound
    /// refuses, `build` exits before the instrument reports.) And because the
    /// parser will not descend past this, it cannot
    /// BUILD a tree deeper than it either — so every later walk over the AST is
    /// bounded by construction rather than by a bound of its own.
    ///
    /// **One counter for every grammar, not one per grammar.** What the stack
    /// margin needs bounded is the parser's TOTAL recursion depth, not any single
    /// family's — six per-family counters of 500 would admit 3000 nested frames
    /// between them, which is no bound at all for the purpose. So types,
    /// patterns, items, import paths, elements and expressions all draw on
    /// [`Parser::nesting_depth`], and 500 is a hard ceiling on nested parser
    /// frames of any kind. The cost is that one level of SOURCE nesting can spend
    /// more than one level of the counter (a nested block spends one for the
    /// expression and one for the statement inside it), which is why the sweep
    /// above is re-measured against this counter rather than inherited from the
    /// instrument's earlier `parse_atom` placement.
    const NESTING_DEPTH_LIMIT: usize = 500;

    /// The nesting bound's diagnostics, one per grammar. All six share the
    /// "nests more than 500 levels deep" spine — it is one bound, and the phase-1
    /// walk's twin (`analyzer::Analyzer::walk_expr_node`) words it the same way —
    /// and each steers toward the flattening that actually applies to the
    /// construct that was too deep. `ParseErrorReason::Rule` takes a
    /// `&'static str` and so cannot interpolate, which is why the limit is
    /// spelled out; `the_refusals_spell_out_the_limit` pins the text to the
    /// constant so the two cannot drift apart.
    const NESTING_REFUSAL: &'static str = "this expression nests more than 500 levels deep, \
         which parsing refuses; lift inner expressions into `let` bindings to flatten it";

    /// [`Parser::NESTING_REFUSAL`] for the type grammar. Vilan has no type alias,
    /// so the flattening on offer is a named `struct`, not a shorthand.
    const TYPE_NESTING_REFUSAL: &'static str = "this type nests more than 500 levels deep, \
         which parsing refuses; name the inner shape as a `struct` and refer to it to flatten it";

    /// [`Parser::NESTING_REFUSAL`] for the binder/pattern grammar.
    const PATTERN_NESTING_REFUSAL: &'static str = "this pattern nests more than 500 levels deep, \
         which parsing refuses; match the outer shape and destructure the rest inside the arm";

    /// [`Parser::NESTING_REFUSAL`] for the item/statement grammar — nested `fun`,
    /// `mod`, `impl` and `trait` (a chained `export` no longer nests, B492).
    const ITEM_NESTING_REFUSAL: &'static str = "this declaration nests more than 500 levels \
         deep, which parsing refuses; lift the inner declarations out to the top level";

    /// [`Parser::NESTING_REFUSAL`] for the `import`/`use` path grammar.
    const IMPORT_NESTING_REFUSAL: &'static str = "this import path nests more than 500 levels \
         deep, which parsing refuses; split it into separate declarations";

    /// [`Parser::NESTING_REFUSAL`] for the element grammar.
    const ELEMENT_NESTING_REFUSAL: &'static str = "this element nests more than 500 levels deep, \
         which parsing refuses; lift inner elements into components of their own";

    /// [`Parser::NESTING_REFUSAL`] for the `css` block grammar. The flattening
    /// on offer is the one the model already has: styles combine with `+`.
    const CSS_NESTING_REFUSAL: &'static str = "this `css` block nests more than 500 levels deep, \
         which parsing refuses; name the inner blocks and combine them with `+`";

    /// One level deeper into a nested grammar, against
    /// [`Parser::NESTING_DEPTH_LIMIT`] — the parser's own depth bound (B142).
    /// `stand_in` builds what the grammar hands back for a subtree it refuses to
    /// descend into — a value, or `None` to decline; `refusal` is the sentence
    /// that explains why.
    ///
    /// **Why the guard is not in `parse_atom`.** `parse_atom` is where the depth
    /// instrument was first hung, and it is the obvious site because the
    /// bracketed forms — `(..)`, `[..]`, a call's arguments, an index — all
    /// re-enter it once per level. But it is not the only door, and that turned
    /// out to be true far past the expression grammar. Within expressions, the
    /// BLOCK-BEARING forms (`{ .. }`, `if`, `for`, `match`, a closure body) reach
    /// a nested expression through [`Parser::parse_secondary`] without ever
    /// touching an atom, and so do the unary prefixes and the `const` prefix,
    /// which recurse into themselves. Measured, `{ { .. 1; } }` nesting parses in
    /// LINEAR time and reaches the stack cliff exactly like a parenthesis chain
    /// does.
    ///
    /// Beyond expressions there are five more recursive grammars in this file,
    /// each a closed cycle that reaches no expression rule at all, so no bound
    /// placed anywhere in the expression grammar could see them: types
    /// ([`Parser::parse_type`], the sole caller of `parse_type_atom`),
    /// binders/patterns, items ([`Parser::parse_statement`] — nested `fun`,
    /// `mod`, `impl` and `trait`; `export` chaining too until B492), import paths, and nested
    /// elements. These are not theoretical: `fun a() { fun a() { .. } }` at 5000
    /// levels overflowed a 64 MiB worker with no diagnostic at all, which is
    /// precisely the outcome B142 exists to prevent. Each of those funnels calls
    /// through here too, so the counter is the whole of the parser's recursion
    /// rather than the whole of one grammar's.
    ///
    /// **Why a stand-in and never `None`.** Declining hands the enclosing rule a
    /// failed alternative to backtrack over and re-try, and 500 open frames each
    /// re-descending through their remaining alternatives is a time bomb, not
    /// just a lost diagnostic: declining from [`Parser::parse_statement`] took
    /// `fun a() { .. }` at 505 levels from 9 ms to not finishing in 25 seconds. A
    /// stand-in is a parse that SUCCEEDED with a hole in it, which is the
    /// existing recovery contract (the `recover_delimited` arms in `parse_atom`
    /// produce exactly this), and it costs the enclosing frame nothing to accept.
    ///
    /// **Which stand-ins consume.** [`Parser::parse_statement`] and
    /// [`Parser::parse_element`] advance the parser by one token as they refuse;
    /// the other four do not. The difference is whether the caller LOOPS on a
    /// `Some`: `parse_program` and `parse_block_clean` push the statement and
    /// `continue`, and the element child loop does the same, so a success that
    /// consumed nothing would spin there forever. One token per refusal makes
    /// each turn of those loops strictly advancing, and the loop drains what is
    /// left of the nest linearly. [`Parser::comma_list`] — the loop the other
    /// four funnels sit under — needs no such help: with no `,` following the
    /// item, it breaks.
    fn parse_nested_as<T>(
        &mut self,
        refusal: &'static str,
        stand_in: impl FnOnce(&mut Self, Span) -> Option<T>,
        body: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Option<T> {
        let _depth = crate::depth_stats::DepthFrame::enter(crate::depth_stats::PARSE);
        self.nesting_depth += 1;
        let parsed = if self.nesting_depth > Self::NESTING_DEPTH_LIMIT {
            let span = self.refuse_nesting(refusal);
            stand_in(self, span)
        } else {
            body(self)
        };
        self.nesting_depth -= 1;
        parsed
    }

    /// [`Parser::parse_nested_as`] for the grammars whose stand-in is a
    /// [`Node::Error`] — expressions and types.
    fn parse_nested(
        &mut self,
        refusal: &'static str,
        body: impl FnOnce(&mut Self) -> Option<Spanned<Node<'src>>>,
    ) -> Option<Spanned<Node<'src>>> {
        self.parse_nested_as(refusal, |_, span| Some((Node::Error, span)), body)
    }

    /// The refusal itself: ONE steering diagnostic per parse (a 5000-deep nest
    /// would otherwise report 4500 times — `walk_depth_refused`'s twin, spelled
    /// as an `Option` being filled rather than a flag beside a push). The span is
    /// the token the parser stopped at, which is inside the nest and as close to
    /// the offending depth as the parser can point. Returns it so each grammar
    /// can build its own stand-in around it.
    fn refuse_nesting(&mut self, refusal: &'static str) -> Span {
        let span = self.here_span();
        self.nesting_refusal.get_or_insert_with(|| ParseError {
            span,
            reason: ParseErrorReason::Rule(refusal),
            context: Vec::new(),
            hint: None,
        });
        span
    }

    /// A full expression: the weak-precedence `const` prefix, else the secondary
    /// grammar (with struct literals admitted as operands). `condition_expression`
    /// is the sibling for condition positions and has NO `const` (§H.1).
    fn parse_expression(&mut self) -> Option<Spanned<Node<'src>>> {
        if self.peek_is(&Token::Const) {
            let start = self.position;
            self.bump();
            // `const const .. 1` recurses HERE, not through `parse_secondary`,
            // so it carries its own nesting level (B142).
            let inner = self.parse_nested(Self::NESTING_REFUSAL, Self::parse_expression)?;
            return Some((Node::Const(Box::new(inner)), self.span_from(start)));
        }
        self.parse_secondary(false)
    }

    /// An expression statement's expression: [`Parser::parse_expression`] with
    /// the statement's head recorded, so a `then`/`else` form starting there
    /// may be the statement reading and the guard may stand (B459).
    fn parse_statement_expression(&mut self) -> Option<Spanned<Node<'src>>> {
        let outer = self.statement_head.replace(self.position);
        let expression = self.parse_expression();
        self.statement_head = outer;
        expression
    }

    /// A statement's expression, terminated: a `then`/`else` form that IS the
    /// statement is re-read as one ([`read_as_statement`]). "Is the statement"
    /// means it begins at the statement's first token — a parenthesized form
    /// keeps its inner span, so `(c then f());` is an operand in parentheses
    /// and stays a value, in the compiler's parse and the formatter's alike.
    fn read_at_statement_position(
        &self,
        expression: Spanned<Node<'src>>,
        head: usize,
    ) -> Spanned<Node<'src>> {
        let starts_the_statement = self
            .tokens
            .get(head)
            .is_some_and(|(_, span)| span.start == expression.1.start);
        if starts_the_statement {
            read_as_statement(expression)
        } else {
            expression
        }
    }

    /// The condition-position expression (`if`/`for` conditions, a `for … in`
    /// iterable, a `match` subject): the secondary grammar with struct literals
    /// excluded as operands, so the `{` after `if Foo` is the block, not a literal
    /// (§H.1). No `const` prefix.
    fn parse_condition(&mut self) -> Option<Spanned<Node<'src>>> {
        self.parse_secondary(true)
    }

    /// `secondary_expression` / `condition_expression`: the block-bearing and
    /// statement-shaped forms, then assignment, then the operator tower. The only
    /// difference between the two grammars is `no_struct`, which reaches ONLY the
    /// operator tower's chain (and its atom head); the block-bearing sub-parsers
    /// each recurse with their own mode (full expressions for values/bodies,
    /// conditions for nested heads), so it is not threaded into them.
    fn parse_secondary(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_nested(Self::NESTING_REFUSAL, |parser| {
            parser.parse_secondary_inner(no_struct)
        })
    }

    /// [`Parser::parse_secondary`]'s body, past the depth bound.
    fn parse_secondary_inner(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let block_like = match self.peek() {
            // A closure literal (`|params| body`, `|| body`) — always tried before
            // the tower, so a leading `||` is never a logical-or (which needs a left
            // operand). Nothing else in the grammar leads with `|`/`||` here.
            Some(Token::Op("|") | Token::Op("||")) => return self.parse_closure(),
            // B414: `jump` and `lazy` are contextual — the keyword only where
            // the LL(2) test says so (`starts_contextual_statement`), a name
            // (`jump.height`, `lazy = 3;`) everywhere else.
            Some(Token::Ident("jump"))
                if starts_contextual_statement(self.tokens, self.position) =>
            {
                return self.parse_jump();
            }
            Some(Token::Let | Token::Mut) => return self.parse_let(),
            Some(Token::Ident("lazy"))
                if starts_contextual_statement(self.tokens, self.position) =>
            {
                return self.parse_let();
            }
            Some(Token::Ret) => return self.parse_return(),
            // B520: `return value` — vilan's `ret`, written another
            // language's way where no name can stand.
            Some(Token::Ident("return")) if starts_foreign_return(self.tokens, self.position) => {
                return self.parse_foreign_return();
            }
            // The four block-bearing heads. They share one rule past their closing
            // brace — B248/B259's: the form is COMPLETE there, so an operator or a
            // `.` after it is refused rather than read as a continuation.
            Some(Token::Ctrl('{')) => self.parse_block_as_expression()?,
            Some(Token::If) => self.parse_if()?,
            Some(Token::For) => self.parse_for()?,
            Some(Token::Match) => self.parse_match()?,
            _ => {
                // Assignment (an lvalue then `=`/`+=`/…) is tried before the tower;
                // it backtracks when no assignment operator follows the place.
                if let Some(assignment) = self.parse_assignment() {
                    return Some(assignment);
                }
                return self.parse_operators(no_struct);
            }
        };
        self.refuse_block_like_continuation(no_struct);
        Some(block_like)
    }

    /// The operator tower above the postfix/precedence chain: the `is` pattern test,
    /// then `&&`, then `||` (each looser than the last). Built over the chain in the
    /// selected struct-literal mode.
    fn parse_operators(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_conditional(no_struct)
    }

    /// B459: the infix conditional — `c then a else b`, and at a statement's
    /// head the statement forms `c then S;`, `c else S;` (the guard) and `c
    /// then S else S;`. Sugar over `if`: the form parses to a `Node::If`
    /// spelled [`IfSpelling::Then`], and the statement reading is decided by
    /// the statement that ends it ([`read_as_statement`]).
    ///
    /// The tier sits above assignment and below `||` (Q1): `a || b then x
    /// else y` tests `a || b`, and `v = c then x else y` assigns the form.
    /// Each branch is a whole expression, so a chain is right-associative
    /// (Q2): `a then x else b then y else z` is an `else`-if chain, and an
    /// `else` binds to the nearest `then` without one. `then` is CONTEXTUAL
    /// (Q4): only here, after a complete operand, where no name can stand —
    /// vilan never puts two names side by side — so `let then = 1;` and
    /// `promise.then(f)` are unaffected. A bare `else` after an operand is the
    /// guard, and is read only at a statement's head: anywhere else the
    /// `else` belongs to an enclosing form's `then` (`c then f() else g();`).
    ///
    /// The part after the condition is ATTEMPTED: a `then` whose branches do
    /// not parse is taken back, so a missing `;` before a line that starts
    /// with a name `then` is still reported as the missing `;`.
    fn parse_conditional(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let at_statement_head = self.statement_head == Some(start);
        let condition = self.parse_logical_or(no_struct)?;
        let guard = at_statement_head && self.peek_is(&Token::Else);
        if !self.peek_is_word("then") && !guard {
            return Some(condition);
        }
        let after_condition = self.position;
        let form = self.attempt(|parser| {
            let then = if parser.peek_is_word("then") {
                let word = parser.here_span();
                parser.contextual_readings.push(parser.position);
                parser.bump();
                Some((word, parser.parse_then_branch(no_struct)?))
            } else {
                None
            };
            let otherwise = if parser.peek_is(&Token::Else) {
                let word = parser.here_span();
                parser.bump();
                Some((word, parser.parse_then_branch(no_struct)?))
            } else {
                None
            };
            Some((then, otherwise))
        });
        let Some((then, otherwise)) = form else {
            self.position = after_condition;
            return Some(condition);
        };
        let then_word = then.as_ref().map(|(word, _)| *word);
        let else_word = otherwise.as_ref().map(|(word, _)| *word);
        let then_block = match then {
            Some((_, branch)) => {
                let span = branch.1;
                ((Vec::new(), Box::new(branch)), span)
            }
            // The guard's `then` is the empty block `if c {} else { S; }`
            // has, placed at its `else`.
            None => {
                let at = else_word.map_or(condition.1.end, |word| word.start);
                let span = Span::from(at..at);
                ((Vec::new(), Box::new((Node::Void, span))), span)
            }
        };
        let else_ = otherwise.map(|(_, branch)| {
            let span = branch.1;
            (
                NodeIfBranch::Else(((Vec::new(), Box::new(branch)), span)),
                span,
            )
        });
        Some((
            Node::If(NodeIfBranch::If(Box::new(If {
                condition: Box::new(condition),
                then: then_block,
                else_,
                spelling: IfSpelling::Then {
                    then_word,
                    else_word,
                    statement: false,
                },
            }))),
            self.span_from(start),
        ))
    }

    /// One branch of a `then`/`else` form: a whole expression (a statement, at
    /// statement position — every statement vilan has is an expression
    /// production), in the enclosing condition mode. A `let` parses and is
    /// refused (Q8, [`A_BRANCH_BINDS_NOTHING`]).
    fn parse_then_branch(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let branch = if no_struct {
            self.parse_secondary(true)?
        } else {
            self.parse_expression()?
        };
        if matches!(branch.0, Node::Let(..)) {
            self.errors.push(ParseError {
                span: branch.1,
                reason: ParseErrorReason::Rule(A_BRANCH_BINDS_NOTHING),
                context: self.context_stack.clone(),
                hint: None,
            });
        }
        Some(branch)
    }

    fn parse_logical_or(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut left = self.parse_logical_and(no_struct)?;
        loop {
            let save = self.position;
            if !self.eat_op("||") {
                break;
            }
            match self.parse_logical_and(no_struct) {
                Some(right) => {
                    left = (
                        Node::Binary(BinaryOp::Or, Box::new(left), Box::new(right)),
                        self.span_from(start),
                    );
                }
                None => {
                    self.position = save;
                    break;
                }
            }
        }
        Some(left)
    }

    fn parse_logical_and(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut left = self.parse_is_expression(no_struct)?;
        loop {
            let save = self.position;
            if !self.eat_op("&&") {
                break;
            }
            match self.parse_is_expression(no_struct) {
                Some(right) => {
                    left = (
                        Node::Binary(BinaryOp::And, Box::new(left), Box::new(right)),
                        self.span_from(start),
                    );
                }
                None => {
                    self.position = save;
                    break;
                }
            }
        }
        Some(left)
    }

    /// `subject is pattern` — a single, optional pattern test (binds tighter than
    /// `&&`). Backtracks the `is` if no pattern follows.
    fn parse_is_expression(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let subject = self.parse_chain(no_struct)?;
        let save = self.position;
        if self.eat(&Token::Is) {
            match self.parse_pattern() {
                Some(pattern) => {
                    return Some((
                        Node::Is(Box::new(subject), Box::new(pattern)),
                        self.span_from(start),
                    ));
                }
                None => self.position = save,
            }
        }
        Some(subject)
    }

    /// The precedence chain (`chain_expr_parser`): the postfix/call/static-access
    /// expression, then the arithmetic/bitwise/comparison tower up to (and
    /// including) comparison — everything below the `is`/`&&`/`||` tier.
    fn parse_chain(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_compare(no_struct)
    }

    /// A left-associative binary level: `operand (op operand)*`, folding with a span
    /// from the chain's start. `operator` returns the `BinaryOp` for the current
    /// token and consumes it, or `None` to stop. On a right-operand failure the
    /// operator is backtracked (chumsky's `op.then(operand)` is atomic).
    fn parse_binary_level(
        &mut self,
        no_struct: bool,
        mut operator: impl FnMut(&mut Self) -> Option<BinaryOp>,
        mut operand: impl FnMut(&mut Self, bool) -> Option<Spanned<Node<'src>>>,
    ) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut left = operand(self, no_struct)?;
        loop {
            let save = self.position;
            let Some(op) = operator(self) else {
                break;
            };
            match operand(self, no_struct) {
                Some(right) => {
                    left = (
                        Node::Binary(op, Box::new(left), Box::new(right)),
                        self.span_from(start),
                    );
                }
                None => {
                    self.position = save;
                    break;
                }
            }
        }
        Some(left)
    }

    fn parse_compare(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_binary_level(
            no_struct,
            |parser| {
                // `==` / `!=` are `Op`s; `<` / `>` are `Ctrl`, and `<=` / `>=` lex as
                // `<` / `>` then `=`.
                if parser.eat_op("==") {
                    Some(BinaryOp::Eq)
                } else if parser.eat_op("!=") {
                    Some(BinaryOp::NotEq)
                } else if parser.eat_ctrl('<') {
                    Some(if parser.eat_op("=") {
                        BinaryOp::LtEq
                    } else {
                        BinaryOp::Lt
                    })
                } else if parser.eat_ctrl('>') {
                    Some(if parser.eat_op("=") {
                        BinaryOp::GtEq
                    } else {
                        BinaryOp::Gt
                    })
                } else {
                    None
                }
            },
            Self::parse_bit_or,
        )
    }

    fn parse_bit_or(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_binary_level(
            no_struct,
            |parser| parser.eat_op("|").then_some(BinaryOp::BitOr),
            Self::parse_bit_xor,
        )
    }

    fn parse_bit_xor(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_binary_level(
            no_struct,
            |parser| parser.eat_op("^").then_some(BinaryOp::BitXor),
            Self::parse_bit_and,
        )
    }

    fn parse_bit_and(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_binary_level(
            no_struct,
            |parser| parser.eat_op("&").then_some(BinaryOp::BitAnd),
            Self::parse_shift,
        )
    }

    fn parse_shift(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_binary_level(no_struct, Self::eat_shift_operator, Self::parse_sum)
    }

    /// `<<` / `>>` — reassembled from two SPAN-ADJACENT `Ctrl` tokens (there is no
    /// dedicated shift token; `<`/`>` are control characters). A lone `<`/`>`, or a
    /// non-adjacent pair (`a < < b`), is not a shift — the second token is left for
    /// the comparison level.
    fn eat_shift_operator(&mut self) -> Option<BinaryOp> {
        for (character, op) in [('<', BinaryOp::Shl), ('>', BinaryOp::Shr)] {
            if self.peek_is_ctrl(character) && self.peek_at_is_ctrl(1, character) {
                let first = self.tokens[self.position].1;
                let second = self.tokens[self.position + 1].1;
                if first.end == second.start {
                    self.position += 2;
                    return Some(op);
                }
            }
        }
        None
    }

    fn parse_sum(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_binary_level(
            no_struct,
            |parser| match parser.peek_op() {
                Some("+") => {
                    parser.bump();
                    Some(BinaryOp::Add)
                }
                Some("-") => {
                    parser.bump();
                    Some(BinaryOp::Sub)
                }
                _ => None,
            },
            Self::parse_product,
        )
    }

    fn parse_product(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        self.parse_binary_level(
            no_struct,
            |parser| match parser.peek_op() {
                Some("*") => {
                    parser.bump();
                    Some(BinaryOp::Mul)
                }
                Some("/") => {
                    parser.bump();
                    Some(BinaryOp::Div)
                }
                Some("%") => {
                    parser.bump();
                    Some(BinaryOp::Rem)
                }
                _ => None,
            },
            Self::parse_unary,
        )
    }

    /// Unary prefixes, binding tighter than the binary ops and recursing on
    /// themselves: `!`, prefix `-`, `await`, `async` (a block or any unary), `&` /
    /// `&mut` (take a view), `*` (deref). Falls through to the postfix chain.
    fn parse_unary(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        // Each prefix recurses into `parse_unary` DIRECTLY, without passing
        // through `parse_secondary`, so every one of them takes its own level
        // of the nesting counter (B142). Without that, `!!!..!1` — measured at
        // ~14.1 KiB per `!` unoptimized — would still be unbounded.
        let start = self.position;
        if self.eat_op("!") {
            let inner = self.parse_nested(Self::NESTING_REFUSAL, |parser| {
                parser.parse_unary(no_struct)
            })?;
            return Some((Node::Unary('!', Box::new(inner)), self.span_from(start)));
        }
        if self.eat_op("-") {
            let inner = self.parse_nested(Self::NESTING_REFUSAL, |parser| {
                parser.parse_unary(no_struct)
            })?;
            return Some((Node::Unary('-', Box::new(inner)), self.span_from(start)));
        }
        if self.eat(&Token::Await) {
            let inner = self.parse_nested(Self::NESTING_REFUSAL, |parser| {
                parser.parse_unary(no_struct)
            })?;
            return Some((Node::Await(Box::new(inner)), self.span_from(start)));
        }
        if self.eat(&Token::Async) {
            // `async { .. }` takes the block; `async expr` any unary.
            let inner = if self.peek_is_ctrl('{') {
                self.parse_block_as_expression()?
            } else {
                self.parse_nested(Self::NESTING_REFUSAL, |parser| {
                    parser.parse_unary(no_struct)
                })?
            };
            return Some((Node::Async(Box::new(inner)), self.span_from(start)));
        }
        if self.eat_op("&") {
            let mutable = self.eat(&Token::Mut);
            let inner = self.parse_nested(Self::NESTING_REFUSAL, |parser| {
                parser.parse_unary(no_struct)
            })?;
            return Some((
                Node::Reference(mutable, Box::new(inner)),
                self.span_from(start),
            ));
        }
        if self.eat_op("*") {
            let inner = self.parse_nested(Self::NESTING_REFUSAL, |parser| {
                parser.parse_unary(no_struct)
            })?;
            return Some((Node::Dereference(Box::new(inner)), self.span_from(start)));
        }
        // B231: a BLOCK-LIKE expression as an operand. `match`/`if` are values
        // everywhere else — an initializer, an argument, a `ret` tail — and the
        // operator tower was the one position that refused them (`flag && match
        // probe() { … }` read as `found 'match' expected an expression`), so the
        // steer was "bind it first" for no reason the grammar could state. Both
        // are led by a KEYWORD, so admitting them here is unambiguous: the
        // construct's own braces are its legs/body, and the token after them is
        // left for whatever wanted it (an enclosing `if`'s block, the next
        // operator). B224's `Sequence` slot is what makes the lowering uniform —
        // an operand that lowers to statements no longer loses its
        // short-circuit.
        //
        // A BARE block is admitted in expression mode only. In condition mode
        // (`no_struct`) a `{` after an operator is the enclosing construct's
        // body — the same ambiguity `no_struct` already resolves for struct
        // literals and `css` blocks — so there it stays refused and parentheses
        // are the spelling.
        match self.peek() {
            Some(Token::Match) => return self.parse_match(),
            Some(Token::If) => return self.parse_if(),
            Some(Token::Ctrl('{')) if !no_struct => return self.parse_block_as_expression(),
            _ => {}
        }
        self.parse_member_accessor(no_struct)
    }

    /// The postfix chain over a call/static-access base: `.member`, `[index]`, `!`,
    /// a direct call `(args)`, `?.member`, and a bare `?`. Collected in order, then
    /// grouped so a `?.` link absorbs the following plain postfixes into its
    /// continuation (up to the next `?.`/`!`/chain end).
    fn parse_member_accessor(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let base = self.parse_call(no_struct)?;
        let mut postfixes: Vec<(Postfix<'src>, Span)> = Vec::new();
        loop {
            let start = self.position;
            match self.parse_one_postfix()? {
                Some(postfix) => postfixes.push((postfix, self.span_from(start))),
                None => break,
            }
        }
        Some(group_postfixes(base, postfixes))
    }

    /// One postfix suffix, or `Some(None)` when none starts here (stop the loop).
    /// The outer `Option` is the clean-or-decline signal (a required inner element
    /// failed); the inner `Option` distinguishes "no postfix here".
    fn parse_one_postfix(&mut self) -> Option<Option<Postfix<'src>>> {
        // `.member` — with the mid-edit recovery to an `Error` member (silent at
        // parse; the receiver still analyzes, for LSP completion).
        if self.peek_is_ctrl('.') {
            let dot_span = self.here_span();
            self.bump();
            let member = match self.member_after_dot() {
                DotMember::Read => self.parse_member_call(),
                DotMember::Missing => None,
                DotMember::Decline => return None,
            };
            return Some(Some(Postfix::Member(
                member.unwrap_or((Node::Error, dot_span)),
            )));
        }
        // `[index]`.
        if self.peek_is_ctrl('[') {
            self.bump();
            let index = self.parse_expression()?;
            self.expect_ctrl(']')?;
            return Some(Some(Postfix::Index(index)));
        }
        // `expr!` — assert-or-return. `!=` is one token, so this never eats a
        // comparison's `!`.
        if self.eat_op("!") {
            return Some(Some(Postfix::TryAssert));
        }
        // `(args)` on a postfix result — calling a closure-typed value.
        if self.peek_is_ctrl('(') {
            let arguments = self.parse_argument_list()?;
            return Some(Some(Postfix::DirectCall(arguments)));
        }
        // `?.member` — a lifted link (tried before the bare `?`).
        if self.peek_is_op("?") && self.peek_at_is_ctrl(1, '.') {
            let start = self.position;
            self.bump(); // `?`
            self.bump(); // `.`
            let dot_span = self.span_from(start);
            let member = match self.member_after_dot() {
                DotMember::Read => self.parse_member_call(),
                DotMember::Missing => None,
                DotMember::Decline => return None,
            };
            return Some(Some(Postfix::LiftMember(
                member.unwrap_or((Node::Error, dot_span)),
            )));
        }
        // A bare `?` — an expression-lifting mark.
        if self.eat_op("?") {
            return Some(Some(Postfix::LiftBare));
        }
        Some(None)
    }

    /// A member after `.`/`?.`: a tuple index (`.0`), or a name with at most ONE
    /// fused call (`.method(args)`, optionally `.method<T>(args)`). Further
    /// `(args)` suffixes are left to the `DirectCall` postfix (the `.read()(a)`
    /// case). Returns `None` when no member follows (the recovery site).
    fn parse_member_call(&mut self) -> Option<Spanned<Node<'src>>> {
        if let Some(Token::Number(whole, fraction, suffix)) = self.peek() {
            let node = Node::Number(whole, *fraction, *suffix);
            let span = self.here_span();
            self.bump();
            return Some((node, span));
        }
        let start = self.position;
        let name = self.eat_member_name()?;
        let accessor = (Node::Accessor(name), self.span_from(start));
        // An optional single fused call: `<generics>? ( args )`. If generics parse
        // but no `(` follows, they are backtracked and the bare accessor is kept.
        let save = self.position;
        let error_count = self.errors.len();
        let generic_arguments = self.parse_generic_arguments();
        if self.peek_is_ctrl('(') {
            let arguments = self.parse_argument_list()?;
            Some((
                Node::Call(Box::new(accessor), generic_arguments, arguments),
                self.span_from(start),
            ))
        } else {
            // A recovered but un-called `<...>` is backtracked whole — including any
            // recovery error it emitted, since this reading is being discarded.
            self.position = save;
            self.errors.truncate(error_count);
            Some(accessor)
        }
    }

    /// `f(args)` / `f<T>(args)` folded over the static-access base. Generic
    /// arguments stick only when a `(` follows (so `a < b` stays a comparison); on
    /// no `(` the whole generic attempt is backtracked.
    fn parse_call(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut callee = self.parse_static_accessor(no_struct)?;
        loop {
            let save = self.position;
            let error_count = self.errors.len();
            let generic_arguments = self.parse_generic_arguments();
            if self.peek_is_ctrl('(') {
                let arguments = self.parse_argument_list()?;
                callee = (
                    Node::Call(Box::new(callee), generic_arguments, arguments),
                    self.span_from(start),
                );
            } else {
                // A recovered but un-called `<...>` is backtracked whole — including
                // any recovery error it emitted, since this reading is discarded.
                self.position = save;
                self.errors.truncate(error_count);
                break;
            }
        }
        Some(callee)
    }

    /// `( expr, … )` argument list (allow-trailing), carrying its own `(`..`)` span.
    /// An argument may be a spread (`f(..pair)`) — a spread parameter collects the
    /// arguments into a tuple construction, so the spread lands inside one
    /// (variadic-generics.md §T.6); anywhere else the analyzer refuses it.
    fn parse_argument_list(&mut self) -> Option<Spanned<NodeList<'src>>> {
        let start = self.position;
        self.expect_ctrl('(')?;
        let arguments = self.comma_list(Self::parse_element_or_spread, |parser| {
            parser.peek_is_ctrl(')')
        })?;
        self.expect_ctrl(')')?;
        Some((arguments, self.span_from(start)))
    }

    /// E142: the segment after a `::` must begin on the SAME line as the `::`.
    /// Call with the separator just consumed; returns `Some(())` when the path
    /// may continue, and `None` — with
    /// [`A_NAME_AFTER_PATH_SEPARATOR_ON_THIS_LINE`] noted, so statement
    /// recovery surfaces it once, located — when a newline sits between the
    /// separator and the token after it.
    ///
    /// The rule lands on the two productions that COMMIT to a separator: the
    /// expression path here and the `import`/`use` path. E145 extended it to
    /// the two that PROBE both tokens first — a type path
    /// ([`Parser::parse_path_type`]) and a struct-literal head
    /// ([`Parser::parse_struct_initializer`]), through
    /// [`Parser::peeked_separator_crosses_a_line`]. Neither can swallow a
    /// following STATEMENT, which is the harm the rule was written for, so
    /// the extension buys consistency rather than a new save: one rule about
    /// where a path's next name may sit, not a rule with two exceptions a
    /// reader has to learn. The census that made it free is E142's — zero
    /// lines end in `::` across the tree, kolt and the book — and it holds at
    /// these two positions as well.
    ///
    /// The parser is otherwise entirely line-insensitive (`tokens_adjacent` is
    /// byte adjacency, not line identity), so this reads the source text
    /// directly: the gap between two tokens is trivia, and a newline in it is
    /// the whole question.
    fn separator_crosses_a_line(&self) -> bool {
        let Some(separator) = self
            .position
            .checked_sub(1)
            .and_then(|at| self.tokens.get(at))
        else {
            return false;
        };
        let from = separator.1.into_range().end;
        let to = match self.tokens.get(self.position) {
            Some((_, span)) => span.into_range().start,
            None => self.eoi,
        };
        self.source
            .get(from..to)
            .is_some_and(|gap| gap.contains('\n'))
    }

    /// [`Self::separator_crosses_a_line`] for a production that has NOT
    /// consumed the separator yet (E145): a type path or a struct-literal
    /// head, which probe the `::` and the name after it before committing to
    /// either. The separator is therefore at the cursor rather than behind it.
    ///
    /// Notes the expectation and answers `true` when that `::` ends its line,
    /// so the loop stops and leaves the separator exactly where it was — which
    /// is what those two productions promise their callers, and what lets the
    /// enclosing statement's recovery report the note once, located.
    fn peeked_separator_crosses_a_line(&mut self) -> bool {
        let Some(separator) = self.tokens.get(self.position) else {
            return false;
        };
        let from = separator.1.into_range().end;
        let to = match self.tokens.get(self.position + 1) {
            Some((_, span)) => span.into_range().start,
            None => self.eoi,
        };
        let crosses = self
            .source
            .get(from..to)
            .is_some_and(|gap| gap.contains('\n'));
        if crosses {
            self.note_expected(A_NAME_AFTER_PATH_SEPARATOR_ON_THIS_LINE);
        }
        crosses
    }

    /// [`Self::separator_crosses_a_line`] as the guard a path continuation
    /// takes: notes the expectation and declines when the next segment is on
    /// another line.
    fn refuse_a_path_crossing_a_line(&mut self) -> Option<()> {
        if self.separator_crosses_a_line() {
            self.note_expected(A_NAME_AFTER_PATH_SEPARATOR_ON_THIS_LINE);
            return None;
        }
        Some(())
    }

    /// `head (:: member)*` — a `::` path. The head is a generic static head
    /// (`List<str>::…`) when a `::` follows generics, else the chain head atom.
    fn parse_static_accessor(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut current = match self.parse_generic_static_head() {
            Some(head) => head,
            None => self.parse_chain_head(no_struct)?,
        };
        loop {
            let save = self.position;
            if !self.eat_op("::") {
                break;
            }
            // E142: the next segment must start on this line. Declining here
            // takes the same path the "nothing after the `::`" arm below takes
            // — cursor rolled back, expectation noted, statement recovery
            // reporting it once — because the two are one mistake seen from
            // two sides: a path whose next name is not where a path's next name
            // can be.
            if self.separator_crosses_a_line() {
                self.note_expected(A_NAME_AFTER_PATH_SEPARATOR_ON_THIS_LINE);
                self.position = save;
                return None;
            }
            // B414 S4: a segment after `::` is a member position — any word
            // (`Event::type(..)`, a static named for a reserved word).
            match self.eat_member_name() {
                Some(member) => {
                    // No generic arguments: in expression position a `<...>`
                    // after the member belongs to the CALL that follows, which
                    // `parse_call`'s fold takes. See `Node::StaticAccessor`.
                    current = (
                        Node::StaticAccessor(Box::new(current), member, None),
                        self.span_from(start),
                    );
                }
                None => {
                    // `style::` with nothing after it — the shape a path is in
                    // while it is being TYPED. The roll-back alone told the
                    // reader nothing: `style` became the whole value, the `::`
                    // was left for whatever came next, and the only diagnostic
                    // was the enclosing statement's missing `;` anchored on the
                    // operator — the name that is actually absent never named
                    // (E135).
                    //
                    // NOTING the expectation is what fixes that, and it is all
                    // that is needed: `farthest_failure` is deliberately not
                    // rolled back by `attempt`, so the note survives every
                    // backtrack above this one and the statement recovery
                    // surfaces it as the located `found '<' expected a name
                    // after `::``. The arm still DECLINES — it does not hand
                    // back a `Node::Error` stand-in — because a stand-in leaves
                    // the cursor on the token after the `::`, where a `<` opening
                    // the next line reads as a comparison and drags the element
                    // into an operator soup whose failure lands on the `let`.
                    // Declining hands the statement to `recover_statement`,
                    // which reports this note once and resynchronizes.
                    //
                    // The cursor rolls back exactly as before, so a caller that
                    // does not backtrack sees the same position it always did.
                    self.note_expected(A_NAME_AFTER_PATH_SEPARATOR);
                    self.position = save;
                    return None;
                }
            }
        }
        Some(current)
    }

    /// `Name<Args>` as a `::`-path head — only when a `::` actually follows (matched
    /// with a lookahead, not consumed), so a generic *call* `default<Id>()` is left
    /// for the call level.
    fn parse_generic_static_head(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            let name = parser.eat_ident()?;
            let generic_arguments = parser.parse_generic_arguments()?;
            if !parser.peek_is_op("::") {
                return None;
            }
            parser.refuse_generic_self(name, start);
            Some((
                Node::AccessorWithGenerics(name, generic_arguments),
                parser.span_from(start),
            ))
        })
    }

    /// B361 (R4) — `Self<..>`, refused with the steer.
    ///
    /// `Self` names the impl's SUBJECT, which is already the whole applied type:
    /// inside `impl Cell<type T>`, `Self` IS `Cell<T>`, so writing arguments on
    /// it names a second application of a type that has one. It parsed in a
    /// type position, in a `::`-path head and at a struct literal, and nothing
    /// in std, the corpus, the examples, the docs, the templates, the website,
    /// the playground or kolt ever wrote it — and where it was written it did
    /// not work: `fun same(self): Self<i32>` with a matching body was refused
    /// `Expected i32, but got i32 instead`, and `Self<i32> { .. }` with
    /// `cannot initialize a non-struct: Self`. A spelling with no use and no
    /// coherent meaning is one refusal, at the spelling, naming the fix.
    fn refuse_generic_self(&mut self, name: &'src str, start: usize) {
        if name != "Self" {
            return;
        }
        self.errors.push(ParseError {
            span: self.span_from(start),
            reason: ParseErrorReason::Rule(
                "`Self` already names the impl's subject WITH its arguments — inside \
                 `impl Cell<type T>` it is `Cell<T>` — so it takes none of its own: write \
                 the type's name (`Cell<i32>`)",
            ),
            context: self.context_stack.clone(),
            hint: None,
        });
    }

    /// The chain head: in expression mode a `css { … }` block, then a struct
    /// initializer, then the plain atom; in condition mode (`no_struct`) only the
    /// plain atom, so a `{` after a bare name is a block, not a literal (§H.1).
    ///
    /// The `css` block sits HERE rather than in [`Parser::parse_atom`] for the
    /// one reason the atom cannot serve it: an atom does not know `no_struct`,
    /// and a `css` block is BRACE-INITIAL. Unlike an element — which begins with
    /// `<`, a byte no expression could start, so `element-syntax.md` could
    /// truthfully leave the condition mode untouched — `css { … }` occupies the
    /// same shape a struct literal does, and is suppressed in condition position
    /// for the same reason (proposal/css-block.md §4.2). Parenthesize to use one
    /// there.
    fn parse_chain_head(&mut self, no_struct: bool) -> Option<Spanned<Node<'src>>> {
        if !no_struct {
            // `css` + `{` — two tokens, and the second is what separates a
            // block from a bare keyword. Without the `{` the word falls
            // through to the atom, whose failure is the promotion's own
            // refusal (a pre-promotion `.css` / `Length::css`), which is
            // strictly the better message.
            if self.peek_is(&Token::Css) && self.peek_at_is_ctrl(1, '{') {
                return self.parse_css_atom();
            }
            if let Some(initializer) = self.parse_struct_initializer() {
                return Some(initializer);
            }
        }
        self.parse_atom()
    }

    /// `type-path { field, … }` — a struct initializer (expression mode only).
    /// Backtracks when no `{` follows the head, so a bare name — or a qualified
    /// path that is not a literal, `shapes::make()` — falls through to the atom.
    ///
    /// The head is the SAME production B172 gave every type position
    /// (`IDENT { "::" IDENT } [ generic-args ]`, [`Parser::parse_path_type`]).
    /// B190: the literal was the one spelling left keyed on a bare identifier,
    /// in a language where `shapes::Dot` is a type and `shapes::make()` is a
    /// call — so `shapes::Dot { x = 1 }` was a parse error ("expected `;` to
    /// end this statement"), and two pins in B172's own lane had to construct
    /// through a `make()` helper to say what they meant.
    ///
    /// The condition-position rule is untouched and did not need restating: a
    /// condition parses through `no_struct`, which does not call this at all,
    /// so a qualified path before a `{` there stays an operand exactly as the
    /// bare form does (§3.8).
    fn parse_struct_initializer(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            // A `::` continues the path only when a NAME follows it — probing
            // BOTH tokens before committing, exactly as `parse_path_type`
            // does, so a trailing `::` is left where the caller can see it.
            let mut namespace: Vec<&'src str> = Vec::new();
            let mut name_start = parser.position;
            let mut name = parser.eat_ident()?;
            while parser.peek_is_op("::")
                && matches!(parser.peek_at(1), Some(Token::Ident(_)))
                // E145: and that name is on this line.
                && !parser.peeked_separator_crosses_a_line()
            {
                namespace.push(name);
                parser.bump(); // `::`
                name_start = parser.position;
                name = parser.eat_ident().expect("peeked as an identifier");
            }
            let name_span = parser.span_from(name_start);
            let generic_arguments = parser.parse_generic_arguments();
            if !parser.peek_is_ctrl('{') {
                return None;
            }
            // B361's third position: `Self<i32> { .. }`. Refused only once the
            // `{` has been seen, so a declining attempt never pushes it — the
            // attempt's own truncation covers the rest.
            if generic_arguments.is_some() && namespace.is_empty() {
                parser.refuse_generic_self(name, name_start);
            }
            // The `{ field, ... }` list, clean or recovered to empty fields on a
            // garbled body (chumsky's `nested_delimiters` on the struct-initializer
            // fields, site 3 of 10). A `{ ... }` that cannot even be balanced makes
            // the whole initializer decline, so a bare name falls through to the
            // atom — matching the oracle.
            let fields = match parser.attempt(|parser| {
                let fields_start = parser.position;
                parser.expect_ctrl('{')?;
                let fields = parser.comma_list(Self::parse_struct_initializer_field, |parser| {
                    parser.peek_is_ctrl('}')
                })?;
                parser.expect_ctrl('}')?;
                Some((fields, parser.span_from(fields_start)))
            }) {
                Some(clean) => clean,
                None => {
                    let span = parser.recover_delimited(
                        "struct initializer",
                        '{',
                        '}',
                        &[('(', ')'), ('[', ']')],
                    )?;
                    (Vec::new(), span)
                }
            };
            Some((
                Node::StructInitializer(
                    namespace,
                    (name, name_span),
                    generic_arguments.map(Box::new),
                    Box::new(fields),
                ),
                parser.span_from(start),
            ))
        })
    }

    /// `name` or `name = value` — one struct-initializer field.
    fn parse_struct_initializer_field(
        &mut self,
    ) -> Option<Spanned<(&'src str, Option<Spanned<Node<'src>>>)>> {
        let start = self.position;
        // B414 S4: a field given WITH `=` is a member position, so any word
        // names it (`Event { type = kind }`); the shorthand `{ type }` reads a
        // binding of that name, which a reserved word can never be.
        let name = if !matches!(self.peek(), Some(Token::Ident(_))) && self.peek_at_is_op(1, "=") {
            self.eat_member_name()?
        } else {
            self.eat_ident()?
        };
        let value = if self.eat_op("=") {
            Some(self.parse_expression()?)
        } else {
            None
        };
        Some(((name, value), self.span_from(start)))
    }

    /// An atom (containing no ambiguity), in the chumsky `atom` choice order: a
    /// literal, a `tuple_comprehension` (`(x in xs => e)`), a `macro name(..)`
    /// invocation, a `macro { }` block, a bare name (`Accessor`), a `[..]` (repeat
    /// or list), or a `(..)` (tuple or parenthesised group). `local_type` is dead
    /// in expression position (the bare-name alternative always wins), matching the
    /// chumsky choice, so it is omitted.
    fn parse_atom(&mut self) -> Option<Spanned<Node<'src>>> {
        ATOM_PARSES.with(|count| count.set(count.get().saturating_add(1)));
        if let Some(literal) = self.parse_literal() {
            return Some(literal);
        }
        if let Some(comprehension) = self.parse_tuple_comprehension() {
            return Some(comprehension);
        }
        if let Some(invocation) = self.parse_macro_invocation() {
            return Some(invocation);
        }
        if let Some(macro_block) = self.parse_macro_block() {
            return Some(macro_block);
        }
        // B520: a foreign `return value` is no operand — the same as the `ret`
        // it stands for — so `1 + return 5` declines at the word, and the
        // statement's recovery resumes there and reads the return.
        if let Some(Token::Ident(name)) = self.peek()
            && !starts_foreign_return(self.tokens, self.position)
        {
            let node = Node::Accessor(name);
            let span = self.here_span();
            self.bump();
            return Some((node, span));
        }
        // An element expression `<div …>` (proposal/element-syntax.md §3): `<`
        // begins no other expression, so atom-position `<` followed by a name
        // is markup. The attempt keeps a garbled element's notes (the farthest
        // failure survives backtracking) while the cursor rolls back for the
        // balanced `<…>` head recovery below.
        if (self.peek_is_ctrl('<') && self.peek_at_is_name(1)) || self.peek_is_fragment_open() {
            let fragment = self.peek_is_fragment_open();
            if let Some(element) = self.attempt(Self::parse_element) {
                return Some(element);
            }
            // A FRAGMENT's head is the two-token `<>` (A46), so the balanced
            // `<…>` recovery below would consume exactly that and hand the
            // body back to statement parsing — which then fails FARTHER along
            // and buries the element's own note (`</>`, or a nested tag's
            // close) under `expected an expression`. A garbled fragment
            // declines instead, so its farthest failure is what surfaces.
            if !fragment
                && let Some(span) = self.recover_delimited(
                    "element",
                    '<',
                    '>',
                    &[('(', ')'), ('[', ']'), ('{', '}')],
                )
            {
                return Some((Node::Error, span));
            }
        }
        if self.peek_is_ctrl('[')
            && let Some(list) = self.parse_bracket_atom()
        {
            return Some(list);
        }
        if self.peek_is_ctrl('(')
            && let Some(paren) = self.parse_paren_atom()
        {
            return Some(paren);
        }
        // Recovery: the two chained `recover_with` on the chumsky `atom` choice — a
        // balanced-but-garbled `(...)` (site 4) / `[...]` (site 5) recovers to a
        // `Node::Error`. Paren is tried first, as chumsky orders them; the two are
        // disjoint on the opening bracket, so the order is not observable.
        if let Some(span) =
            self.recover_delimited("expression", '(', ')', &[('[', ']'), ('{', '}')])
        {
            return Some((Node::Error, span));
        }
        if let Some(span) =
            self.recover_delimited("expression", '[', ']', &[('(', ')'), ('{', '}')])
        {
            return Some((Node::Error, span));
        }
        self.note_expected("an expression");
        None
    }

    /// A single-token literal, including `void` (the unit value — a bare `void`
    /// identifier in expression position).
    fn parse_literal(&mut self) -> Option<Spanned<Node<'src>>> {
        let node = match self.peek()? {
            Token::Null => Node::Null,
            Token::Bool(value) => Node::Bool(*value),
            Token::Number(whole, fraction, suffix) => Node::Number(whole, *fraction, *suffix),
            Token::String(text) => Node::String(text),
            Token::MultilineString(text) => Node::MultilineString(text),
            Token::Ident("void") => Node::Void,
            _ => return None,
        };
        let span = self.here_span();
        self.bump();
        Some((node, span))
    }

    /// `[value; length]` (repeat) or `[a, b, …]` (list). The `;` after the first
    /// element is the fork.
    fn parse_bracket_atom(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('[')?;
            if parser.eat_ctrl(']') {
                return Some((Node::List(Vec::new()), parser.span_from(start)));
            }
            let first = parser.parse_expression()?;
            if parser.eat_ctrl(';') {
                let length = parser.parse_expression()?;
                parser.expect_ctrl(']')?;
                return Some((
                    Node::Repeat(Box::new(first), Box::new(length)),
                    parser.span_from(start),
                ));
            }
            let mut items = vec![first];
            while parser.eat_ctrl(',') {
                if parser.peek_is_ctrl(']') {
                    break;
                }
                items.push(parser.parse_expression()?);
            }
            // The list has its own item loop (the `[v; n]` repeat form rules out
            // `comma_list`), so record here — as `comma_list` does — that a `,`
            // (another element) was also admissible, giving "expected `,` or `]`" if
            // the closer is missing rather than the bare `]`.
            parser.note_expected("','");
            parser.expect_ctrl(']')?;
            Some((Node::List(items), parser.span_from(start)))
        })
    }

    /// `(a, b, …)` (a tuple, ≥2 elements) or `(expr)` (a group that dissolves to its
    /// inner expression — keeping the inner's own span — unless it contains a
    /// bare-`?` mark, when it becomes a region-delimiting `LiftGroup`).
    ///
    /// Under [`Parser::preserve_paren_groups`] — the formatter's parse mode — every
    /// group is recorded as a `LiftGroup` instead, so a reprint can put the
    /// parentheses back exactly where they were written. This is the one site the
    /// flag is read.
    fn parse_paren_atom(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('(')?;
            // A leading `..` settles the tuple/group fork on the spot: `..e` is not
            // an expression, so a group cannot hold one, and the ≥2-element minimum
            // that exists only to keep `(e)` a group does not apply
            // (variadic-generics.md §T.3). `(..pair)` is the concatenation of one —
            // the shape `f(..pair)` desugars to.
            if let Some(spread) = parser.parse_spread_element() {
                let mut items = vec![spread];
                while parser.eat_ctrl(',') {
                    items.push(parser.parse_element_or_spread()?);
                }
                parser.expect_ctrl(')')?;
                return Some((Node::Tuple(items), parser.span_from(start)));
            }
            let first = parser.parse_expression()?;
            if parser.peek_is_ctrl(',') {
                // A tuple is `expr (',' expr)*` (≥2 elements) with NO trailing comma
                // — unlike a list literal, the chumsky `tuple` atom has no
                // `allow_trailing`, so `(a, b,)` declines (a group can't hold a comma
                // either). Every `,` here must be followed by an expression.
                let mut items = vec![first];
                while parser.eat_ctrl(',') {
                    items.push(parser.parse_element_or_spread()?);
                }
                parser.expect_ctrl(')')?;
                Some((Node::Tuple(items), parser.span_from(start)))
            } else {
                parser.expect_ctrl(')')?;
                if parser.preserve_paren_groups || first.0.contains_lift_mark() {
                    Some((Node::LiftGroup(Box::new(first)), parser.span_from(start)))
                } else {
                    // A group dissolves to its inner expression, keeping the inner's
                    // own span (the parens contribute nothing).
                    Some(first)
                }
            }
        })
    }

    /// One entry of a tuple construction or an argument list: a spread (`..e`) or
    /// an ordinary expression.
    fn parse_element_or_spread(&mut self) -> Option<Spanned<Node<'src>>> {
        match self.parse_spread_element() {
            Some(spread) => Some(spread),
            None => self.parse_expression(),
        }
    }

    /// `..e` — a tuple-value spread, recognized ONLY where an element begins
    /// (variadic-generics.md §T.1). That position, not adjacency, is what
    /// separates it from the member-access dots: `(1..3, x)` starts its first
    /// element with `1`, so it never reaches here and parses exactly as it did
    /// before this feature existed. Declines (consuming nothing) when the two
    /// dots are absent.
    fn parse_spread_element(&mut self) -> Option<Spanned<Node<'src>>> {
        if !(self.peek_is_ctrl('.') && self.peek_at_is_ctrl(1, '.')) {
            return None;
        }
        self.attempt(|parser| {
            let start = parser.position;
            parser.bump();
            parser.bump();
            // `...e` in a value position is the parameter marker written where a
            // value spread belongs. Consume the third dot and name the difference,
            // rather than reading `..` and then failing to parse `.e`.
            if parser.eat_ctrl('.') {
                parser.errors.push(ParseError {
                    span: parser.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "`...` marks a spread PARAMETER, on a declaration; a tuple-value \
                         spread is `..` — write `(..pair, x)`",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
            let operand = parser.parse_expression()?;
            Some((Node::Spread(Box::new(operand)), parser.span_from(start)))
        })
    }

    // --- `css` blocks (proposal/css-block.md §4.3) ---------------------------

    /// A `css { … }` block in atom position. The body commits and recovers per
    /// item, so this declines only on a block that never closes — the mid-edit
    /// shape — and then falls back on the element grammar's delimiter recovery
    /// so a half-typed block does not flatten the enclosing statement to an
    /// error atom.
    fn parse_css_atom(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        if let Some(block) = self.attempt(Self::parse_css_block) {
            return Some(block);
        }
        self.bump();
        if self
            .recover_delimited("css block", '{', '}', &[('(', ')'), ('[', ']')])
            .is_some()
        {
            return Some((Node::Error, self.span_from(start)));
        }
        self.position = start;
        None
    }

    /// `css { item* }` — the whole block. The keyword is the atom's own token;
    /// the body is [`Parser::parse_css_body`], shared with every nested rule.
    fn parse_css_block(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Css)?;
        let body = self.parse_css_body()?;
        Some((Node::Css(body), self.span_from(start)))
    }

    /// `{ item* }` — a block's or a nested rule's body, in written order.
    /// Nothing is reordered, deduped or merged here: what you write is the
    /// chain you get (§3).
    ///
    /// The `{` COMMITS, so a broken item is reported and skipped rather than
    /// declining — E49's lesson, taken from the element head: declining would
    /// hand the whole block to `parse_css_atom`'s delimiter recovery, whose
    /// last-resort message ("unclosed `{` in css block") is a claim about a
    /// region that plainly closed, and would throw away the missing-`;` the
    /// author actually needs to read. Recovering here also keeps the items
    /// around the mistake in the tree, which is what lets the language server
    /// still answer inside a block under construction. Only running out of
    /// input declines.
    fn parse_css_body(&mut self) -> Option<CssBody<'src>> {
        let start = self.position;
        self.expect_ctrl('{')?;
        let mut items = Vec::new();
        loop {
            if self.eat_ctrl('}') {
                break;
            }
            if self.at_end() {
                self.note_expected("'}'");
                return None;
            }
            let item_start = self.position;
            let error_count = self.errors.len();
            match self.parse_css_item() {
                Some(item) => items.push(item),
                None => {
                    // Every committed item reports for itself (see
                    // `parse_css_declaration`); this covers the one that
                    // declined before committing to anything.
                    if self.errors.len() == error_count {
                        let context = self.context_stack.clone();
                        self.emit_failure(item_start, vec![CSS_ITEM_EXPECTED.to_string()], context);
                    }
                    self.skip_broken_css_item();
                }
            }
        }
        Some(CssBody {
            items,
            braces: self.span_from(start),
        })
    }

    /// Report a `css` item's own failure AT the token that stopped it, straight
    /// into the error list.
    ///
    /// The farthest-failure channel cannot serve here, and the reason is worth
    /// recording: a `css` block in statement position is parsed TWICE — once by
    /// `parse_assignment`'s speculative place probe, once for real — and the
    /// speculative pass leaves `farthest_failure` sitting at the enclosing
    /// statement's own terminator note, which is farther along than anything
    /// inside the block. The real pass's note then never advances past it, and
    /// a missing `:` reads as a missing `;` from the line below. An item is a
    /// COMMITTED production once its property name or its dot is read, so it
    /// reports for itself; `parse_css_body` recovers around it, which is what
    /// keeps the error on the surviving branch.
    fn report_css_failure(&mut self, expected: &str) {
        let context = self.context_stack.clone();
        self.emit_failure(self.position, vec![expected.to_string()], context);
    }

    /// Skip past a broken item: to just after the next `;` at brace depth 0, or
    /// to the `}` that closes the body (left unconsumed, for the loop to take).
    fn skip_broken_css_item(&mut self) {
        let mut depth = 0usize;
        while !self.at_end() {
            if self.peek_is_ctrl('{') {
                depth += 1;
            } else if self.peek_is_ctrl('}') {
                if depth == 0 {
                    return;
                }
                depth -= 1;
            } else if depth == 0 && self.peek_is_ctrl(';') {
                self.bump();
                return;
            }
            self.bump();
        }
    }

    /// One item. The dot decides, and decides alone (§3): dotted is a condition
    /// combinator, undotted is a declaration — so the grammar never consults
    /// `Style`'s method list, and a method added to `Style` can never change
    /// what existing `css` means.
    fn parse_css_item(&mut self) -> Option<CssItem<'src>> {
        if self.peek_is_ctrl('.') {
            return self.parse_css_dotted();
        }
        // E153: an item that STARTS with `:` is a CSS pseudo-class selector,
        // the one thing a CSS writer is most likely to reach for here. Steered
        // rather than reported as a missing declaration — the answer is a dot,
        // and nothing in "expected a declaration" says so.
        if self.peek_is_op(":") {
            let context = self.context_stack.clone();
            self.errors.push(ParseError {
                span: self.here_span(),
                reason: ParseErrorReason::Rule(CSS_PSEUDO_CLASS_IS_DOTTED),
                context,
                hint: None,
            });
            return None;
        }
        self.parse_css_declaration().map(CssItem::Declaration)
    }

    /// A DOTTED item: `.name { … }` (a condition combinator) or `.name;` (a
    /// chain link, A69). The head is one parse either way and what FOLLOWS it
    /// decides — a `{` makes a rule, anything else a link — so the grammar
    /// still never consults `Style`'s method list, and the two forms need no
    /// lookahead past the head.
    ///
    /// Only the OUTERMOST block arrives through the atom; every nested rule
    /// re-enters here directly, so the nesting needs its own depth level
    /// (B142). A link nests nothing and pays the same bound harmlessly, which
    /// is cheaper than splitting the head parse in two.
    fn parse_css_dotted(&mut self) -> Option<CssItem<'src>> {
        self.parse_nested_as(
            Self::CSS_NESTING_REFUSAL,
            |parser, _span| {
                // Consume, for the reason every other stand-in does: the item
                // loop pushes and continues, so a refusal that consumed nothing
                // would spin.
                parser.bump();
                None
            },
            Self::parse_css_dotted_inner,
        )
    }

    /// [`Parser::parse_css_dotted`]'s body, past the depth bound.
    fn parse_css_dotted_inner(&mut self) -> Option<CssItem<'src>> {
        let start = self.position;
        self.expect_ctrl('.')?;
        // R-k: a dotted item's name is written against its dot, as a member's
        // is everywhere else.
        if self.peek_is_member_name() && !self.previous_token_is_adjacent() {
            self.report_css_failure(A_MEMBER_NAME_AGAINST_ITS_DOT);
            return None;
        }
        let name_span = self.here_span();
        let Some(name) = self.eat_member_name() else {
            self.report_css_failure(
                "a condition combinator (`.hover { … }`) or a chain link (`.ghost();`)",
            );
            return None;
        };
        // The head's arguments are ORDINARY vilan expressions, so
        // `.within("data-theme", Some("dark")) { … }` and `.pseudo("first-child") { … }`
        // work with no special casing (§4.3) — and so do a link's.
        let parenthesized = self.peek_is_ctrl('(');
        let arguments = if parenthesized {
            self.parse_argument_list()?.0
        } else {
            Vec::new()
        };
        let head = self.span_from(start);
        // A69: `{` is the condition rule, and anything else is a chain link
        // ended by its required `;` — the same terminator a declaration takes,
        // reported the same gap-anchored way.
        if !self.peek_is_ctrl('{') {
            if !self.peek_is_ctrl(';') {
                self.report_css_failure(TERMINATOR_EXPECTED);
                return None;
            }
            self.bump();
            return Some(CssItem::Link(crate::node::CssLink {
                name: (name, name_span),
                arguments,
                parenthesized,
                span: self.span_from(start),
            }));
        }
        let body = self.parse_css_body()?;
        Some(CssItem::Nested(CssNested {
            name: (name, name_span),
            arguments,
            body,
            head,
            span: self.span_from(start),
        }))
    }

    /// `property(value);` — one declaration, which is a CALL (A101). The `;`
    /// is REQUIRED, including after the last: the formatter may never invent a
    /// token (the token equality net), and a required terminator keeps an item
    /// decidable in one pass (§4.3).
    ///
    /// The arguments are an ORDINARY argument list — the same production a
    /// nested rule's head and every other call in the language take — so a
    /// typed value needs no hole, value completion is expression completion,
    /// and a value whose type is not a raw value is the ordinary type error AT
    /// the argument rather than a string that silently reaches the sheet.
    fn parse_css_declaration(&mut self) -> Option<CssDeclaration<'src>> {
        let start = self.position;
        let Some(property) = self.parse_css_property() else {
            self.report_css_failure(CSS_ITEM_EXPECTED);
            return None;
        };
        // The one token every migrating program and every CSS author writes
        // here. It is a committed declaration by now — the property name read
        // — so the rule reports for itself and names the call form.
        if self.peek_is_op(":") {
            let context = self.context_stack.clone();
            self.errors.push(ParseError {
                span: self.here_span(),
                reason: ParseErrorReason::Rule(A_CSS_DECLARATION_IS_A_CALL),
                context,
                hint: None,
            });
            return None;
        }
        if !self.peek_is_ctrl('(') {
            self.report_css_failure("'('");
            return None;
        }
        // `!important` is refused permanently and with its fix: merge is a
        // record update, so a `Style` that needed it would be a `Style` that
        // had lost the property the whole model is for (§10). Read off the
        // TOKENS before the arguments are parsed, because `red !important` is
        // not an expression and the argument list would report a `,` it never
        // wanted — the reader would then never see the sentence that answers.
        if let Some(span) = self.important_within_arguments() {
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Rule(IMPORTANT_HAS_NO_PLACE),
                context: self.context_stack.clone(),
                hint: None,
            });
            return None;
        }
        let (arguments, parens) = self.parse_argument_list()?;
        if arguments.is_empty() {
            self.note_expected("a value");
            return None;
        }
        // The `;` reports as a MISSING TERMINATOR, gap-anchored: the mistake is
        // in the whitespace before the next item, not on it, and the message is
        // the one the language server already carries an "Insert `;`" quickfix
        // for (`editing-dx.md` §4.4).
        if !self.peek_is_ctrl(';') {
            self.report_css_failure(TERMINATOR_EXPECTED);
            return None;
        }
        self.bump();
        Some(CssDeclaration {
            property,
            arguments,
            parens,
            span: self.span_from(start),
        })
    }

    /// The span of a `!important` written inside the argument list opening at
    /// the cursor, at any depth — `color(red !important)`, the CSS author's own
    /// transliteration. `None` when the list holds none, or does not close.
    fn important_within_arguments(&self) -> Option<Span> {
        let mut depth = 0usize;
        let mut at = self.position;
        while let Some((token, span)) = self.tokens.get(at) {
            match token {
                Token::Ctrl('(') | Token::Ctrl('[') | Token::Ctrl('{') => depth += 1,
                Token::Ctrl(')') | Token::Ctrl(']') | Token::Ctrl('}') => {
                    depth -= 1;
                    if depth == 0 {
                        return None;
                    }
                }
                Token::Op("!")
                    if matches!(
                        self.tokens.get(at + 1),
                        Some((Token::Ident("important"), _))
                    ) =>
                {
                    return Some((span.start..self.tokens[at + 1].1.end).into());
                }
                _ => {}
            }
            at += 1;
        }
        None
    }

    /// A property name — `{ "-" } NAME { "-" NAME }`, span-adjacent, so
    /// `flex-direction` is three tokens and `--color-ink` is five, and
    /// `data - id` is a name and an operator rather than a name. Returns the
    /// SPAN; the text is sliced at desugar, exactly as an element's tag is.
    ///
    /// R12: the leading-dash run is what admits a CUSTOM property as a call
    /// head — `--brand-ink(gray(900));` — and it needs nothing new, exactly as
    /// an element attribute's `data-`/`aria-` names need nothing new.
    fn parse_css_property(&mut self) -> Option<Span> {
        let start = self.position;
        let start_offset = self.here_span().start;
        // The custom-property prefix. `--` fuses into no operator token (it is
        // not in `TWO_CHARACTER_OPERATORS`), so a leading run is one `-` per
        // token, each adjacent to the next.
        let mut cursor = start_offset;
        while matches!(self.peek(), Some(Token::Op("-")))
            && self.here_span().start == cursor
            && self.tokens_adjacent(0, 1)
        {
            cursor = self.here_span().end;
            self.bump();
        }
        // The name proper reuses the element grammar's hyphen joiner — the same
        // span-adjacency rule, minted for `aria-label`.
        let Some((name, _)) = self.parse_element_name() else {
            self.position = start;
            return None;
        };
        if name.start != cursor {
            // A leading `-` that did not touch the name: not a property.
            self.position = start;
            return None;
        }
        Some((start_offset..name.end).into())
    }

    // --- Elements (proposal/element-syntax.md) -------------------------------

    /// True when the token at `offset` can begin an element or attribute NAME:
    /// an identifier, any keyword (`type`, `for` — ordinary HTML attribute
    /// names), or a bool literal. Everything carrying punctuation or a value
    /// shape is excluded.
    fn peek_at_is_name(&self, offset: usize) -> bool {
        !matches!(
            self.peek_at(offset),
            None | Some(
                Token::Ctrl(_)
                    | Token::Op(_)
                    | Token::String(_)
                    | Token::MultilineString(_)
                    | Token::Number(..)
            )
        )
    }

    /// True when the tokens at `first`/`second` (offsets from the cursor) touch
    /// — the span-adjacency test `<<`/`>>` reassembly uses, generalized.
    fn tokens_adjacent(&self, first: usize, second: usize) -> bool {
        match (
            self.tokens.get(self.position + first),
            self.tokens.get(self.position + second),
        ) {
            (Some(a), Some(b)) => a.1.end == b.1.start,
            _ => false,
        }
    }

    /// Whether the token at the cursor touches the one before it — no trivia
    /// between them. `false` at the start of the stream.
    fn previous_token_is_adjacent(&self) -> bool {
        match (
            self.position
                .checked_sub(1)
                .and_then(|at| self.tokens.get(at)),
            self.tokens.get(self.position),
        ) {
            (Some(previous), Some(current)) => previous.1.end == current.1.start,
            _ => false,
        }
    }

    fn peek_at_is_op(&self, offset: usize, symbol: &str) -> bool {
        matches!(self.peek_at(offset), Some(Token::Op(found)) if *found == symbol)
    }

    /// `<>` — a fragment's nameless head (A46). SPAN-ADJACENT, like `/>` and
    /// `</`: `<` and `>` are separate control tokens (neither is in the
    /// operator charset, so the lexer never fuses them), and requiring them to
    /// touch keeps the pair out of every expression `<` already begins.
    fn peek_is_fragment_open(&self) -> bool {
        self.peek_is_ctrl('<') && self.peek_at_is_ctrl(1, '>') && self.tokens_adjacent(0, 1)
    }

    /// A (possibly hyphenated) element NAME — `div`, `type`, `aria-label`,
    /// `my-widget`: a name token, then any number of SPAN-ADJACENT `-`-name
    /// joints (`data - id` is two names and an operator, not a name). Returns
    /// the name's span and its token range; the TEXT is sliced at desugar time
    /// — the parser has no source access, and keyword tokens carry none.
    fn parse_element_name(&mut self) -> Option<(Span, std::ops::Range<usize>)> {
        if !self.peek_at_is_name(0) {
            return None;
        }
        let start_index = self.position;
        let start_span = self.here_span();
        self.bump();
        let mut end = start_span.end;
        while matches!(self.peek(), Some(Token::Op("-")))
            && self.tokens_adjacent(0, 1)
            && self.peek_at_is_name(1)
            && self.tokens[self.position].1.start == end
        {
            end = self.tokens[self.position + 1].1.end;
            self.bump();
            self.bump();
        }
        Some(((start_span.start..end).into(), start_index..self.position))
    }

    /// A name's text, rebuilt from its tokens (for diagnostics only — the tree
    /// stores spans and the desugar slices the source).
    fn element_name_text(&self, range: &std::ops::Range<usize>) -> String {
        self.tokens[range.clone()]
            .iter()
            .map(|(token, _)| token.to_string())
            .collect()
    }

    /// An element expression: `<tag head-items… />` or
    /// `<tag head-items…> children… </tag>`. Committed once `<` + a name is
    /// seen; the caller wraps the whole parse in `attempt`, so a decline here
    /// leaves only its farthest-failure note behind.
    fn parse_element(&mut self) -> Option<Spanned<Node<'src>>> {
        // Only the OUTERMOST element arrives through `parse_atom`; every nested
        // child re-enters here directly, so element nesting needs its own level
        // (B142). Its stand-in consumes for the same reason `parse_statement`'s
        // does — the element child loop pushes and continues.
        self.parse_nested_as(
            Self::ELEMENT_NESTING_REFUSAL,
            |parser, span| {
                parser.bump();
                Some((Node::Error, span))
            },
            Self::parse_element_inner,
        )
    }

    /// [`Parser::parse_element`]'s body, past the depth bound.
    fn parse_element_inner(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        // E115: the angle brackets are recorded as they are consumed — this is
        // the only place their positions are known, and the editor's
        // semantic-token pass needs them to paint a head that spans lines.
        let mut punctuation = vec![self.here_span()];
        // A46: `<>…</>` — the nameless head, a FRAGMENT. It takes no head
        // items and has no self-closing form, so the whole head is the
        // adjacent `<>` pair and everything after it is children up to `</>`.
        // The lowering diverges too (`elements.rs`): a `List<View>` literal,
        // not a `view("tag")` chain.
        if self.peek_is_fragment_open() {
            self.bump();
            punctuation.push(self.here_span());
            self.bump();
            let (children, _close_tag, close_punctuation) = self.parse_element_children(None)?;
            punctuation.extend(close_punctuation);
            let body = ElementBody {
                tag: None,
                head: Vec::new(),
                children,
                self_closing: false,
                close_tag: None,
                punctuation,
            };
            return Some((Node::Element(Box::new(body)), self.span_from(start)));
        }
        self.expect_ctrl('<')?;
        let (tag, tag_tokens) = self.parse_element_name()?;
        let mut head = Vec::new();
        let children = loop {
            // `/>` — self-closing (span-adjacent, like `<<`).
            if self.peek_is_op("/") && self.peek_at_is_ctrl(1, '>') && self.tokens_adjacent(0, 1) {
                let slash = self.here_span();
                self.bump();
                let angle = self.here_span();
                self.bump();
                punctuation.push((slash.start..angle.end).into());
                break (Vec::new(), true, None);
            }
            if self.peek_is_ctrl('>') {
                punctuation.push(self.here_span());
                self.bump();
                let (children, close_tag, close_punctuation) =
                    self.parse_element_children(Some(&tag_tokens))?;
                punctuation.extend(close_punctuation);
                break (children, false, close_tag);
            }
            if self.at_end() {
                self.note_expected("`>` or `/>`");
                return None;
            }
            if let Some(item) = self.parse_element_head_item()? {
                head.push(item);
            }
        };
        let (children, self_closing, close_tag) = children;
        let body = ElementBody {
            tag: Some(tag),
            head,
            children,
            self_closing,
            close_tag,
            punctuation,
        };
        Some((Node::Element(Box::new(body)), self.span_from(start)))
    }

    /// One head item: a `.method(…)` chain link, an `on:event(handler)`, an
    /// attribute `name(value)`, or a bare boolean attribute `name`
    /// (proposal/element-syntax.md §2 — the dot is the disambiguator, so the
    /// grammar never consults any method list).
    ///
    /// `Some(None)` is the recovered case: a head item that could not be
    /// completed, already reported, which the head loop drops while keeping the
    /// element (see the chain arm).
    fn parse_element_head_item(&mut self) -> Option<Option<ElementHeadItem<'src>>> {
        // Chain form — the link node exactly as a written chain builds it.
        if self.peek_is_ctrl('.') {
            self.bump();
            // R-k: a head's items are separated by whitespace, so a name that
            // does not touch the dot is the NEXT item (`<input . disabled>`)
            // and the link is the unfinished one — reported the way a dot with
            // no name is, with the rule's own expectation.
            if self.peek_is_member_name() && !self.previous_token_is_adjacent() {
                let context = self.context_stack.clone();
                self.emit_failure(
                    self.position,
                    vec![A_MEMBER_NAME_AGAINST_ITS_DOT.to_string()],
                    context,
                );
                return Some(None);
            }
            let Some(link) = self.attempt(Self::parse_member_call) else {
                // A dot with no name after it: the shape a head is in while a
                // chain link is being TYPED (`<div .`). The dot has already
                // committed the item to the chain form, so this is a committed
                // production failing — recover it the way E49 recovers every
                // other one, by reporting and carrying on, rather than by
                // declining and letting `parse_atom`'s element recovery flatten
                // the whole tag (and, nested, the whole statement) to an error
                // atom. Keeping the element is what lets the language server
                // still answer inside a tag under construction (E67).
                self.note_expected("a method name");
                let context = self.context_stack.clone();
                self.emit_failure(self.position, vec!["a method name".to_string()], context);
                return Some(None);
            };
            return Some(Some(ElementHeadItem::Chain(link)));
        }
        // Event form — `on`, an adjacent `:`, an adjacent event name.
        if matches!(self.peek(), Some(Token::Ident("on")))
            && self.peek_at_is_op(1, ":")
            && self.tokens_adjacent(0, 1)
            && matches!(self.peek_at(2), Some(Token::Ident(_)))
            && self.tokens_adjacent(1, 2)
        {
            self.bump();
            self.bump();
            let event_span = self.here_span();
            let Some(Token::Ident(event)) = self.peek() else {
                unreachable!("guarded above");
            };
            let event = *event;
            self.bump();
            self.expect_ctrl('(')?;
            let handler = self.parse_expression()?;
            self.expect_ctrl(')')?;
            return Some(Some(ElementHeadItem::Event(
                (event, event_span),
                Box::new(handler),
            )));
        }
        // Attribute form.
        let Some((name, _)) = self.parse_element_name() else {
            self.note_expected("an attribute, a `.method(…)` link, or `>`");
            return None;
        };
        if !self.peek_is_ctrl('(') {
            return Some(Some(ElementHeadItem::Attribute(name, None)));
        }
        let open = self.position;
        self.bump();
        let value = self.parse_expression()?;
        if self.peek_is_ctrl(',') {
            // A second value in an attribute — `<div raw("inert", scrim)>`. The
            // message this raises is already the right one; what E136 fixed is
            // that it used to be a HARD decline, and a hard decline threw the
            // message away in the shape the mistake is usually written in.
            //
            // Declining fails the element, `parse_atom` falls back to
            // `recover_delimited("element", '<', '>')`, and that re-emits this
            // failure — but only if the enclosing STATEMENT then parses. With a
            // paired `</div>` it does not: the two angle brackets read as
            // comparisons, `attempt`'s `errors.truncate` drops the curated
            // message with the branch that produced it, and what surfaces is
            // `expected ';'` on the tag. The self-closing `/>` spelling escaped
            // only because nothing else could read it.
            //
            // So the comma is recovered exactly as the `.`-chain arm above
            // recovers a nameless link (E49): report HERE, where the mistake is,
            // skip the attribute's own parentheses, and answer `Some(None)` —
            // the head item is dropped, the element survives, and the statement
            // around it parses, so nothing truncates.
            self.note_expected("`)` (an attribute takes one value; a chain link starts with `.`)");
            let context = self.context_stack.clone();
            self.emit_failure(
                self.position,
                vec![
                    "`)` (an attribute takes one value; a chain link starts with `.`)".to_string(),
                ],
                context,
            );
            // Unbalanced parentheses have no matching `)` to skip to; the region
            // is genuinely garbled and the element recovery is the better
            // reader, so that case still declines.
            let end = self.scan_balanced(open, '(', ')', &[('[', ']'), ('{', '}')])?;
            self.position = end;
            return Some(None);
        }
        self.expect_ctrl(')')?;
        Some(Some(ElementHeadItem::Attribute(name, Some(value))))
    }

    /// Children up to the matching `</tag>`: nested elements, quoted strings
    /// (an i-string arrives as its lexed paren group), and `{expression}`
    /// holes. Bare text is a parse error that teaches the quoted form.
    ///
    /// `open_tokens` is the opening tag's name tokens, or `None` for a
    /// FRAGMENT (A46), which closes on `</>` and has no name to match.
    ///
    /// Returns the children, the close tag's NAME span (`None` for `</>`), and
    /// the close tag's own two angle-bracket spans — its `</` and its `>`
    /// (E115).
    fn parse_element_children(
        &mut self,
        open_tokens: Option<&std::ops::Range<usize>>,
    ) -> Option<(Vec<ElementChild<'src>>, Option<Span>, [Span; 2])> {
        let mut children: Vec<ElementChild<'src>> = Vec::new();
        loop {
            // `</tag>` — the close (span-adjacent `</`), name-matched against
            // the opener token-by-token.
            if self.peek_is_ctrl('<') && self.peek_at_is_op(1, "/") && self.tokens_adjacent(0, 1) {
                let angle = self.here_span();
                self.bump();
                let slash = self.here_span();
                self.bump();
                // A fragment closes on `</>`: nothing stands where the name
                // would, and a name there is the mismatch this reports.
                let Some(open_tokens) = open_tokens else {
                    let closing_angle = self.here_span();
                    if !self.peek_is_ctrl('>') {
                        self.note_expected("`</>`");
                        return None;
                    }
                    self.bump();
                    return Some((
                        children,
                        None,
                        [(angle.start..slash.end).into(), closing_angle],
                    ));
                };
                let close = self.parse_element_name();
                let matches_open = close.as_ref().is_some_and(|(_, close_tokens)| {
                    close_tokens.len() == open_tokens.len()
                        && self.tokens[close_tokens.clone()]
                            .iter()
                            .zip(&self.tokens[open_tokens.clone()])
                            .all(|(a, b)| a.0 == b.0)
                });
                if !matches_open {
                    let open_name = self.element_name_text(open_tokens);
                    self.note_expected(&format!("`</{open_name}>`"));
                    return None;
                }
                let closing_angle = self.here_span();
                self.expect_ctrl('>')?;
                let (close_span, _) = close.expect("matched above");
                return Some((
                    children,
                    Some(close_span),
                    [(angle.start..slash.end).into(), closing_angle],
                ));
            }
            if self.at_end() {
                self.note_expected(&match open_tokens {
                    Some(open_tokens) => format!("`</{}>`", self.element_name_text(open_tokens)),
                    None => "`</>`".to_string(),
                });
                return None;
            }
            // A nested element, or a nested fragment (A46).
            if (self.peek_is_ctrl('<') && self.peek_at_is_name(1)) || self.peek_is_fragment_open() {
                children.push(ElementChild::Bare(self.parse_element()?));
                continue;
            }
            // A quoted string child.
            if let Some(Token::String(text)) = self.peek() {
                let node = Node::String(text);
                let span = self.here_span();
                self.bump();
                children.push(ElementChild::Bare((node, span)));
                continue;
            }
            if let Some(Token::MultilineString(text)) = self.peek() {
                let node = Node::MultilineString(text);
                let span = self.here_span();
                self.bump();
                children.push(ElementChild::Bare((node, span)));
                continue;
            }
            // An i-string child — the lexer already turned it into a paren
            // group, so it arrives as `(`; a literal parenthesized expression
            // parses identically (the recorded wrinkle).
            if self.peek_is_ctrl('(') {
                children.push(ElementChild::Bare(self.parse_paren_atom()?));
                continue;
            }
            // A `{expression}` hole.
            if self.eat_ctrl('{') {
                let child = self.parse_expression()?;
                self.expect_ctrl('}')?;
                children.push(ElementChild::Hole(child));
                continue;
            }
            self.note_expected("a child: an element, a quoted string, or a `{expression}` hole");
            return None;
        }
    }

    // --- Generic arguments ---------------------------------------------------

    /// `<Type, …>` — generic arguments (allow-trailing), or `None` (backtracking)
    /// when no well-formed `<…>` is present, which is how `a < b` stays a
    /// comparison. A balanced-but-garbled `<…>` recovers to an empty argument vec
    /// (chumsky's `nested_delimiters` on `generic_arguments`, site 2 of 10) —
    /// which is safe here precisely because recovery requires a matching `>`, so a
    /// lone `<` (a comparison) still declines.
    fn parse_generic_arguments(&mut self) -> Option<GenericArguments<'src>> {
        if let Some(clean) = self.attempt(|parser| {
            if !parser.peek_is_ctrl('<') {
                return None;
            }
            let start = parser.position;
            parser.expect_ctrl('<')?;
            let arguments =
                parser.comma_list(Self::parse_type, |parser| parser.peek_is_ctrl('>'))?;
            parser.expect_ctrl('>')?;
            Some((arguments, parser.span_from(start)))
        }) {
            return Some(clean);
        }
        self.recover_delimited(
            "generic arguments",
            '<',
            '>',
            &[('(', ')'), ('[', ']'), ('{', '}')],
        )
        .map(|span| (Vec::new(), span))
    }

    // --- Block-bearing forms -------------------------------------------------

    /// A brace-delimited block: `statement* trailing_expression?`. The trailing
    /// expression is the block's value; with none, the value is `void` at the
    /// closing brace.
    ///
    /// A broken body no longer recovers by SKIPPING the whole `{…}` region to an
    /// empty block (chumsky's `nested_delimiters` on `block`, site 6 of 10):
    /// [`Parser::parse_block_clean`] recovers statement by statement instead, so
    /// the siblings of a broken statement survive — `editing-dx.md` §2.2's
    /// mechanism 2, the one that made a body's every diagnostic disappear while
    /// its last statement was half-typed. The region-skipping arm is gone with it:
    /// past the `{`, the clean parse can no longer decline, so it was unreachable.
    fn parse_block(&mut self) -> Option<Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)>> {
        self.attempt(Self::parse_block_clean)
    }

    /// The `{ statement* trailing_expression? }` parse. Once the `{` is consumed
    /// this always produces a block: a statement that declines is either the
    /// block's trailing expression, or it is broken — reported and synchronized
    /// past ([`Parser::recover_statement`]), leaving its siblings in the tree.
    ///
    /// That is `editing-dx.md` §2.2's mechanism 2: before S1 a body with one
    /// half-typed statement in it was thrown away wholesale and replaced by an
    /// EMPTY block, so every diagnostic every other statement in that body would
    /// have produced vanished with it — measured keystroke by keystroke in P30.
    /// A body that runs out of input is likewise kept, with `unclosed \`{\``
    /// reported at the opening brace (§5.3) instead of `found end of input` at the
    /// far end of the file.
    fn parse_block_clean(&mut self) -> Option<Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)>> {
        let start = self.position;
        self.expect_ctrl('{')?;
        let mut statements = Vec::new();
        let mut tail = None;
        loop {
            if self.peek_is_ctrl('}') || self.at_end() {
                break;
            }
            if let Some(statement) = self.parse_statement() {
                statements.push(statement);
                continue;
            }
            // No statement starts here. The block's trailing expression is the
            // other legitimate reading (it is the one thing in a block that needs
            // no `;`), and it must close the block to be one.
            if let Some(expression) = self.attempt(|parser| {
                let expression = parser.parse_expression()?;
                parser.peek_is_ctrl('}').then_some(expression)
            }) {
                tail = Some(expression);
                break;
            }
            match self.recover_missing_terminator() {
                TerminatorRecovery::Kept(statement) => {
                    statements.push(statement);
                    continue;
                }
                TerminatorRecovery::Dropped => continue,
                TerminatorRecovery::Declined => {}
            }
            if let Some(swallowed) = self.recover_statement(self.position, true) {
                statements.push(swallowed);
            }
        }
        if !self.eat_ctrl('}') {
            // The loop only leaves on `}` or end of input, so this is a `{` the
            // user has not closed yet. Report it where they typed it.
            self.errors.push(ParseError {
                span: self.token_span(start),
                reason: ParseErrorReason::Unclosed { delimiter: '{' },
                context: self.context_stack.clone(),
                hint: None,
            });
        }
        let span = self.span_from(start);
        // S3 (editing-dx.md §3.5/§3.9): the closing brace itself — width one,
        // exactly `}` — not the zero-width point one byte PAST it. `}` is a
        // single ASCII byte, so `span.end - 1..span.end` IS the brace; the old
        // `span.end..span.end` sat just after it, which an editor draws as
        // nothing at all (a caret, not an underline) — the doc comment above
        // already claimed "at the closing brace"; this is what makes it true.
        let void_span = (span.end.saturating_sub(1)..span.end).into();
        let tail = tail
            .map(Box::new)
            .unwrap_or_else(|| Box::new((Node::Void, void_span)));
        Some(((statements, tail), span))
    }

    /// A block used as an expression (`Node::Block`) — the secondary-expression `{`
    /// alternative and the `async { .. }` body.
    fn parse_block_as_expression(&mut self) -> Option<Spanned<Node<'src>>> {
        let (body, span) = self.parse_block()?;
        Some((Node::Block((body, span)), span))
    }

    /// `if condition { .. } (else ({ .. } | if …))?`.
    fn parse_if(&mut self) -> Option<Spanned<Node<'src>>> {
        let (branch, span) = self.parse_if_branch()?;
        Some((Node::If(branch), span))
    }

    /// The recursive core of `if`, yielding a `NodeIfBranch::If`. `else if` recurses
    /// into another branch; `else { .. }` is a `NodeIfBranch::Else`.
    fn parse_if_branch(&mut self) -> Option<Spanned<NodeIfBranch<'src>>> {
        let start = self.position;
        self.expect(&Token::If)?;
        let condition = self.parse_condition()?;
        let then = self.parse_block()?;
        let else_ = if self.eat(&Token::Else) {
            if self.peek_is(&Token::If) {
                Some(self.parse_if_branch()?)
            } else {
                let block = self.parse_block()?;
                let block_span = block.1;
                Some((NodeIfBranch::Else(block), block_span))
            }
        } else {
            None
        };
        Some((
            NodeIfBranch::If(Box::new(If {
                condition: Box::new(condition),
                then,
                else_,
                spelling: IfSpelling::Keyword,
            })),
            self.span_from(start),
        ))
    }

    /// Every loop form: `for item in iterable { .. }`, `for { .. }` (infinite), or
    /// `for condition { .. }` (while). The `item in` form is distinguished by a
    /// bare loop variable followed by `in`.
    fn parse_for(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::For)?;
        // `for <binder> in …`. The binder production is `let`'s (B368/R5), so a
        // tuple binder destructures the element in the header — `for (index,
        // item) in list.iter().enumerate()` — exactly as `let (index, item) =
        // pair;` does one line lower. Read as an ATTEMPT rather than by peeking
        // for `IDENT` + `in`, because the binder is now arbitrarily wide: what
        // decides the form is that the binder is followed by `in`, and a
        // `for a == b { .. }` while-loop backtracks out of it with its errors
        // truncated.
        if let Some(binder) = self.attempt(|parser| {
            let binder = parser.parse_binder()?;
            parser.expect(&Token::In)?;
            Some(binder)
        }) {
            let iterable = self.parse_condition()?;
            let body = self.parse_block()?;
            return Some((
                Node::ForIn(Box::new(binder), Box::new(iterable), body),
                self.span_from(start),
            ));
        }
        // A header that SAYS `in` but whose binder is not one the binding
        // grammar takes — `for Some(x) in xs`, `for 3 in xs`, `for (only) in
        // xs` — is refused BY NAME, naming the sanctioned spelling (R5). Before
        // this the attempt above fell through to the while-loop branch, the
        // condition parse died on the `in`, and the author read `found 'for'
        // expected a statement or '}'` anchored on the `for` keyword, with no
        // mention of the binder at all (B368's second half). The rest of the
        // header is consumed so the file keeps parsing.
        if let Some(offset) = self.for_header_binder_width() {
            // Consumed FIRST, so the span the refusal carries is the binder the
            // author wrote and not the `for` keyword (E190's rule: the
            // diagnostic anchors where the fix goes).
            let binder_start = self.position;
            for _ in 0..offset {
                self.bump();
            }
            self.errors.push(ParseError {
                span: self.span_from(binder_start),
                reason: ParseErrorReason::Rule(
                    "a `for … in` header binds the element with `let`'s binder — a name, or a \
                     tuple or array of names (`for (index, item) in …`); bind the element and \
                     destructure in the body for anything else",
                ),
                context: self.context_stack.clone(),
                hint: None,
            });
            self.expect(&Token::In)?;
            let iterable = self.parse_condition()?;
            let body = self.parse_block()?;
            let binder = Box::new((Pattern::Wildcard, self.span_from(start)));
            return Some((
                Node::ForIn(binder, Box::new(iterable), body),
                self.span_from(start),
            ));
        }
        // `for { .. }` (infinite) — the block is tried before a condition so its
        // brace is not read as a condition.
        if self.peek_is_ctrl('{') {
            let body = self.parse_block()?;
            return Some((Node::For(None, body), self.span_from(start)));
        }
        // `for condition { .. }` (while).
        let condition = self.parse_condition()?;
        let body = self.parse_block()?;
        Some((
            Node::For(Some(Box::new(condition)), body),
            self.span_from(start),
        ))
    }

    /// Is this `for` header an `in` form, and if so how many tokens is its
    /// binder? Read by scanning from just past the keyword to the `in` that
    /// would separate binder from iterable — stopping at the header's own block
    /// brace, and never counting an `in` inside a nested delimited region, so a
    /// `for probe(pick(x)) { .. }` while-loop is not mistaken for one.
    /// Consulted only once [`Parser::parse_binder`] has already declined, i.e.
    /// only to choose between "an unreadable binder" and "a condition".
    fn for_header_binder_width(&self) -> Option<usize> {
        let mut depth = 0usize;
        let mut offset = 0usize;
        while let Some(token) = self.peek_at(offset) {
            match token {
                Token::Ctrl('(' | '[') => depth += 1,
                Token::Ctrl(')' | ']') => depth = depth.saturating_sub(1),
                // The header's own `{` ends it: past there is the body, and an
                // `in` inside the body is some inner loop's.
                Token::Ctrl('{') if depth == 0 => return None,
                // A binder of nothing is not a binder — `for in xs` is a
                // condition parse's problem, not this refusal's.
                Token::In if depth == 0 => return (offset > 0).then_some(offset),
                _ => {}
            }
            offset += 1;
        }
        None
    }

    /// `match subject { leg, … }`.
    fn parse_match(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Match)?;
        let subject = self.parse_condition()?;
        let legs_start = self.position;
        self.expect_ctrl('{')?;
        let mut legs = Vec::new();
        loop {
            if self.peek_is_ctrl('}') || self.at_end() {
                break;
            }
            legs.push(self.parse_match_leg()?);
            // A comma after a leg is optional.
            self.eat_ctrl(',');
        }
        self.expect_ctrl('}')?;
        let legs = (legs, self.span_from(legs_start));
        Some((Node::Match(Box::new(subject), legs), self.span_from(start)))
    }

    /// One leg: `pattern (, pattern)* (if guard)? => body`.
    fn parse_match_leg(&mut self) -> Option<MatchLeg<'src>> {
        let mut patterns = vec![self.parse_pattern()?];
        loop {
            let save = self.position;
            if !self.eat_ctrl(',') {
                break;
            }
            match self.parse_pattern() {
                Some(pattern) => patterns.push(pattern),
                None => {
                    // A trailing `,` before the guard/`=>` (no pattern list is
                    // allow-trailing) is backtracked.
                    self.position = save;
                    break;
                }
            }
        }
        let guard = if self.eat(&Token::If) {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        self.expect_op("=>")?;
        let body = self.parse_expression()?;
        Some((patterns, guard, body))
    }

    /// `jump target` — a loop-control keyword.
    fn parse_jump(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        if !self.eat_word("jump") {
            return None;
        }
        let target = self.eat_ident()?;
        Some((Node::Jump(target), self.span_from(start)))
    }

    /// `ret expr?` — return a value, or a bare `ret` for an early void return.
    fn parse_return(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Ret)?;
        let value = self.parse_expression();
        Some((Node::FuncReturn(value.map(Box::new)), self.span_from(start)))
    }

    /// `let`/`mut` binding: `(let|mut) binder (: type)? (= value)?`, lowering a bare
    /// name to `Let` and a destructuring binder to `LetDestructure`.
    fn parse_let(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        // `lazy let name: T = init;` (proposal/lazy.md §2) — the initializer
        // runs at the binding's first USE instead of at module load, then
        // memoizes. `lazy` is the outermost word, as it is on a parameter, and
        // for the same reason: it is about WHEN, before anything about what.
        let lazy = self.eat_word("lazy");
        let mutable = if self.eat(&Token::Let) {
            false
        } else if self.eat(&Token::Mut) {
            true
        } else if lazy {
            // `lazy name = …` — the binder word is missing. Taken as `let` and
            // reported, rather than declined: declining would hand the word
            // back to an expression attempt that cannot read it either, and
            // `attempt` truncates the errors a declining branch pushed, so the
            // rule would never reach the author (they would get "found 'lazy'
            // expected an item" about a keyword they spelled correctly). This
            // is `misplaced_mut`'s move on the parameter side, for the same
            // reason (diagnostics-standard B5).
            self.errors.push(ParseError {
                span: self.span_from(start),
                reason: ParseErrorReason::Rule(
                    "a lazy binding is `lazy let name: T = <initializer>;` — `lazy` \
                     defers a BINDING's initializer to its first use, and a parameter's \
                     argument to the callee's first read",
                ),
                context: Vec::new(),
                hint: None,
            });
            false
        } else {
            return None;
        };
        let (pattern, pattern_span) = self.parse_binder()?;
        let type_ = if self.eat_op(":") {
            Some(Box::new(
                self.in_context("type annotation", Self::parse_type)?,
            ))
        } else {
            None
        };
        let value = if self.eat_op("=") {
            Some(Box::new(self.parse_expression()?))
        } else {
            None
        };
        if lazy {
            // `lazy mut` — the binding is initialized ONCE, at first use, and
            // memoized; a `mut` module global is a different contract (a slot
            // anything may rewrite) and the two say opposite things about when
            // the value is settled.
            if mutable {
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "a lazy binding is initialized once, at its first use, and memoized, \
                         so it is `lazy let`; `mut` names a slot anything may rewrite",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
            // The initializer IS the feature — there is nothing to defer
            // without one, and a declaration-only `let` is a shape the binding
            // grammar allows for other reasons.
            if value.is_none() {
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "a lazy binding declares the initializer it defers: write \
                         `lazy let name: T = <initializer>;`",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
            // One cell, one name: a destructure would need one memo per piece
            // and a first-use rule per piece, which is not what `lazy` means.
            if !matches!(pattern, Pattern::Binding(..)) {
                self.errors.push(ParseError {
                    span: pattern_span,
                    reason: ParseErrorReason::Rule(
                        "a lazy binding binds ONE name to one memo cell; destructure inside \
                         the initializer, or bind the whole value lazily and read its pieces",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
        }
        let node = match pattern {
            Pattern::Binding(name, _, _) => {
                Node::Let((name, pattern_span), type_, value, mutable, lazy, None)
            }
            pattern => Node::LetDestructure((pattern, pattern_span), type_, value, mutable),
        };
        Some((node, self.span_from(start)))
    }

    /// G24 — the `const` DECLARATION forms, read at STATEMENT position before
    /// the expression grammar sees `const` as its weak-precedence prefix:
    /// `const let NAME[: T] = EXPR;`, `const fun NAME(..) { .. }`, and `const
    /// mut`, which is refused.
    ///
    /// `None` for anything else after `const` — a plain `const <expr>` is the
    /// prefix `parse_expression` has always read, and the statement funnel
    /// falls through to it unchanged. Declaration position is the whole of the
    /// restriction: `const let` is a statement, not a sub-expression, which is
    /// what makes "module level AND locally" (R3) the complete answer to where
    /// it may be written.
    ///
    /// Both forms keep the shape `Node::Const(<declaration>)`. The analyzer's
    /// `const` arm FORWARDS its inner node, so the binding and the item walk
    /// exactly as a plain `let` and a plain `fun` do, and what the marker adds
    /// — the initializer evaluated at build time, the body capability-checked
    /// at its declaration — is recorded beside the entity rather than spelled
    /// as a second AST.
    ///
    /// B487: both take the label prefix a plain declaration does —
    /// `[deprecated("use g")] const fun f()`, `[internal("why")] const let x
    /// = 1;` — so the deprecation policy reaches a `const` item. Written ahead
    /// of the keyword (B485 §6.2), the run reaches here BEHIND it:
    /// [`Parser::canonicalize_marker_run`] leads `const` past the attributes
    /// on the stream, as `export` is led, and the declaration under it reads
    /// its own prefix — `parse_function`'s for a `fun`, the item labels for a
    /// `let`. `const mut` takes none: it is refused and read as the `mut` it
    /// spells.
    fn parse_const_declaration(&mut self) -> Option<Spanned<Node<'src>>> {
        if !self.peek_is(&Token::Const) {
            return None;
        }
        let start = self.position;
        let mut head = start + 1;
        while self.tokens.get(head).map(|(token, _)| token) == Some(&Token::Ctrl('['))
            && matches!(self.tokens.get(head + 1), Some((Token::Ident(_), _)))
        {
            head = self.past_balanced_group(head)?;
        }
        let labelled = head > start + 1;
        match self.tokens.get(head).map(|(token, _)| token) {
            Some(Token::Let) => {
                self.bump();
                let labels = self.parse_item_labels();
                if self.position != head {
                    // An attribute a binding does not take (`[must_use]`):
                    // declined, for the statement funnel's refusal.
                    return None;
                }
                let declaration = match (self.parse_let()?, labels) {
                    (declaration, None) => declaration,
                    ((Node::Let(name, type_, value, mutable, lazy, None), span), labels) => {
                        (Node::Let(name, type_, value, mutable, lazy, labels), span)
                    }
                    // Only a plain binding takes a label, as a plain `let`'s
                    // does (`parse_labelled_let`).
                    _ => return None,
                };
                self.eat_declaration_terminator()?;
                Some((Node::Const(Box::new(declaration)), self.span_from(start)))
            }
            // `const mut` is refused and then RECOVERED as the runtime `mut`
            // it spells, so the rest of the file parses and the author gets
            // one diagnostic rather than a cascade. The error survives the
            // statement funnel's `attempt` because this arm returns `Some`.
            Some(Token::Mut) if !labelled => {
                let context = self.context_stack.clone();
                self.errors.push(ParseError {
                    span: self.here_span(),
                    reason: ParseErrorReason::Rule(CONST_HAS_NO_MUTATION),
                    context,
                    hint: None,
                });
                self.bump();
                let declaration = self.parse_let()?;
                self.eat_declaration_terminator()?;
                Some(declaration)
            }
            Some(Token::Fun) => {
                self.bump();
                let declaration = self.parse_function()?;
                Some((Node::Const(Box::new(declaration)), self.span_from(start)))
            }
            _ => None,
        }
    }

    /// B494: `async` written where a binding begins — `async x = 1;`,
    /// `async x: i32 = 1;`, `async let x = 1;`, `async mut x = 1;` — is
    /// refused once ([`ASYNC_MARKS_NO_BINDING`]) and read as the plain binding
    /// it spells, so the name is bound and nothing after it cascades. `async x
    /// = 1;` rewrites the `async` to the `let` it stands in for; before `let`
    /// or `mut` the word is read past.
    ///
    /// Nothing that works today reads differently: `async NAME =` was the
    /// assignment `(async NAME) = …`, which the JS backend emits as an invalid
    /// left-hand side, and `async let` never parsed. `async { … }`, `async
    /// load()` and `async fun` are untouched.
    fn parse_misplaced_async_binding(&mut self) -> Option<Spanned<Node<'src>>> {
        if !self.peek_is(&Token::Async) {
            return None;
        }
        let names_a_binding = match self.peek_at(1) {
            Some(Token::Let | Token::Mut) => true,
            Some(Token::Ident(_)) => {
                matches!(self.peek_at(2), Some(Token::Op("=") | Token::Op(":")))
            }
            _ => false,
        };
        if !names_a_binding {
            return None;
        }
        let span = self.here_span();
        if matches!(self.peek_at(1), Some(Token::Ident(_))) {
            self.tokens[self.position].0 = Token::Let;
        } else {
            self.bump();
        }
        self.record_rewrite(span, ParseErrorReason::Rule(ASYNC_MARKS_NO_BINDING));
        let declaration = self.parse_let()?;
        self.eat_declaration_terminator()?;
        Some(declaration)
    }

    /// The `;` a `const let` owes, with the statement funnel's own recovery
    /// note on a miss — the funnel's terminator handling belongs to the
    /// expression fork this form no longer travels through.
    fn eat_declaration_terminator(&mut self) -> Option<()> {
        if self.eat_ctrl(';') {
            return Some(());
        }
        self.note_terminator();
        None
    }

    /// An assignment: `(*)? place op value`, where `place` is the struct-free
    /// precedence chain and `op` is `=`/`+=`/`-=`/`*=`/`/=`/`%=`. Backtracks when no
    /// assignment operator follows the place, so an ordinary expression is left to
    /// the operator tower.
    fn parse_assignment(&mut self) -> Option<Spanned<Node<'src>>> {
        // No assignment operator is reachable from here, so the speculative
        // place parse below could only ever be thrown away — and throwing it
        // away is what makes nested expressions exponential (the
        // `assignment_reachable` field doc has the measurement).
        if !self.assignment_reachable[self.position.min(self.tokens.len())] {
            return None;
        }
        self.attempt(|parser| {
            let start = parser.position;
            let deref = parser.eat_op("*");
            let place = parser.parse_chain(true)?;
            let target = if deref {
                (Node::Dereference(Box::new(place)), parser.span_from(start))
            } else {
                place
            };
            let op = if parser.eat_op("=") {
                None
            } else if parser.eat_op("+=") {
                Some(BinaryOp::Add)
            } else if parser.eat_op("-=") {
                Some(BinaryOp::Sub)
            } else if parser.eat_op("*=") {
                Some(BinaryOp::Mul)
            } else if parser.eat_op("/=") {
                Some(BinaryOp::Div)
            } else if parser.eat_op("%=") {
                Some(BinaryOp::Rem)
            } else {
                return None;
            };
            let value = parser.parse_expression()?;
            Some((
                Node::Assign(Box::new(target), op, Box::new(value)),
                parser.span_from(start),
            ))
        })
    }

    /// A closure literal: `|param, …| : return_type? body` or `|| : return_type?
    /// body`. Parameters take the SAME grammar as a function's — `(mut | own |
    /// & mut?)? binder (: type)?` — so a callback can receive a writable view
    /// (`|&mut list| { list.push(..) }`, backlog A18). A closure literal is
    /// parsed speculatively, and `attempt` rewinds both the position and any
    /// rule errors, so admitting the prefixes cannot turn a non-closure into a
    /// diagnostic: only an expression *starting* with `|` reaches here, and no
    /// binary operator can start one.
    fn parse_closure(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            let parameters = if parser.eat_op("||") {
                Vec::new()
            } else if parser.eat_op("|") {
                let parameters = parser.comma_list(Self::parse_function_parameter, |parser| {
                    parser.peek_is_op("|")
                })?;
                parser.expect_op("|")?;
                parameters
            } else {
                return None;
            };
            parser.reject_spread_position(
                &parameters,
                "a closure cannot take a spread parameter: a closure TYPE \
                 (`sync |A, B| C`) has no variadic form, so such a closure could not \
                 be annotated, stored, or passed anywhere. Take a tuple parameter \
                 (`|items: T|`) and call it with one",
            );
            parser.reject_lazy_position(
                &parameters,
                "a closure cannot take a `lazy` parameter: a closure TYPE \
                 (`sync |A| B`) has no lazy form, so the thunking would be invisible \
                 to every position the closure is annotated, stored or passed in. \
                 Take a closure parameter (`|make: sync || T|`) and call it where the \
                 value is wanted",
            );
            let parameters = (parameters, parser.span_from(start));
            // B520: `|x| -> T body` reads as `|x|: T body`.
            let return_type =
                if parser.eat_op(":") || parser.eat_foreign_arrow(ForeignSpelling::Arrow) {
                    Some(Box::new(parser.parse_type()?))
                } else {
                    None
                };
            let return_value = parser.parse_expression()?;
            Some((
                Node::Closure(Closure {
                    parameters,
                    return_type,
                    return_value: Box::new(return_value),
                }),
                parser.span_from(start),
            ))
        })
    }

    // --- Binders and patterns ------------------------------------------------

    /// A binder in `let`/parameter position: a plain name, a tuple of binders
    /// (`(a, b)`, ≥2), or a fixed-array binder (`[a, b, c]`, ≥1). Nests recursively.
    /// Bindings are parsed immutable; `let`/`mut` stamps mutability separately.
    fn parse_binder(&mut self) -> Option<Spanned<Pattern<'src>>> {
        // Binders nest without touching an expression rule (B142): `let [[[a]]]`
        // recurses here through `comma_list`. `Pattern::Wildcard` is the stand-in
        // — it matches anything and binds nothing, which is what a subtree the
        // parser declined to read is worth.
        self.parse_nested_as(
            Self::PATTERN_NESTING_REFUSAL,
            |_, span| Some((Pattern::Wildcard, span)),
            Self::parse_binder_inner,
        )
    }

    /// [`Parser::parse_binder`]'s body, past the depth bound.
    fn parse_binder_inner(&mut self) -> Option<Spanned<Pattern<'src>>> {
        let start = self.position;
        if self.peek_is_ctrl('(') {
            return self.attempt(|parser| {
                parser.expect_ctrl('(')?;
                let patterns =
                    parser.comma_list(Self::parse_binder, |parser| parser.peek_is_ctrl(')'))?;
                parser.expect_ctrl(')')?;
                if patterns.len() < 2 {
                    return None;
                }
                Some((Pattern::Tuple(patterns), parser.span_from(start)))
            });
        }
        if self.peek_is_ctrl('[') {
            return self.attempt(|parser| {
                parser.expect_ctrl('[')?;
                let patterns =
                    parser.comma_list(Self::parse_binder, |parser| parser.peek_is_ctrl(']'))?;
                parser.expect_ctrl(']')?;
                if patterns.is_empty() {
                    return None;
                }
                Some((Pattern::Array(patterns), parser.span_from(start)))
            });
        }
        // B446: a binder that is not one RECORDS what it wanted, here, where
        // the reader went wrong. Without it the farthest failure on record was
        // whatever the production before had noted — after a generic-typed
        // parameter, the `,` its argument list could have taken at its `>` —
        // and `fun f(a: List<i32>, 5)` reported "found '>' expected ','" one
        // parameter early. A `let` and a `for` binder read the same production.
        let Some(name) = self.eat_ident() else {
            self.note_expected("a name");
            return None;
        };
        // A binder's own span IS the bare name, so the name span and the pattern
        // span coincide here. They part company one level up, where the match/`is`
        // grammar's `let`/`mut` arm widens the pattern span over the keyword.
        let name_span = self.span_from(start);
        // N113: `void` is the unit VALUE's spelling, and the atom production
        // reads every `void` as `Node::Void` unconditionally (it is contextual,
        // §2.2, but not contextual here). So a binder may take the name and
        // nothing can ever read it back: `let void = 3; let x: i32 = void;` is
        // refused `Expected i32, but got void`, about a binding the author is
        // looking straight at. One binder production serves `let`, `for`, a
        // function parameter and a match capture, so this is the one site.
        // Only a BINDER — a struct field or a method named `void` is reached
        // through a receiver and reads back perfectly well.
        if name == "void" {
            self.errors.push(ParseError {
                span: name_span,
                reason: ParseErrorReason::Rule(
                    "`void` is the unit value's own spelling, so a binding cannot take \
                     it: every later `void` still reads as the unit, not as this \
                     binding — name it something else",
                ),
                context: self.context_stack.clone(),
                hint: None,
            });
        }
        Some((Pattern::Binding(name, false, name_span), name_span))
    }

    /// A match/`is` pattern: `_`, `let x` / `mut x` (a binder), a literal (`"quit"`,
    /// `42`), a tuple (`(a, b)`, ≥2), or a variant (`Some(let x)`, `Enum::Variant`).
    fn parse_pattern(&mut self) -> Option<Spanned<Pattern<'src>>> {
        // `S(S(S(..)))` in a match arm — the other half of the pattern grammar's
        // cycle, and counted for the same reason as `parse_binder` (B142).
        self.parse_nested_as(
            Self::PATTERN_NESTING_REFUSAL,
            |_, span| Some((Pattern::Wildcard, span)),
            Self::parse_pattern_inner,
        )
    }

    /// [`Parser::parse_pattern`]'s body, past the depth bound.
    fn parse_pattern_inner(&mut self) -> Option<Spanned<Pattern<'src>>> {
        let start = self.position;
        // `let x` / `mut x` — a binder, stamped mutable per the keyword.
        if self.peek_is(&Token::Let) || self.peek_is(&Token::Mut) {
            let mut mutable = self.eat(&Token::Mut);
            if !mutable {
                self.bump(); // `let`
            }
            // `Some(let mut x)` / `Some(mut let x)`: the two binding forms
            // written as one, exactly as `let mut x = …` writes them at a
            // declaration (A80). Refused by name, and CONSUMED — the binder is
            // taken as MUTABLE, which is what either spelling was reaching for,
            // so the arm still parses and the author reads one diagnostic
            // naming `Some(mut x)` instead of a "found '(' expected '=>'" about
            // the payload's own paren, thrown when the pattern backtracked out
            // from under it (diagnostics-standard B5).
            let paired = if mutable { Token::Let } else { Token::Mut };
            if self.peek_is(&paired) {
                self.bump();
                mutable = true;
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(PATTERN_BINDER_IS_ONE_WORD),
                    context: self.context_stack.clone(),
                    hint: None,
                });
            }
            let (binder, _) = self.parse_binder()?;
            let pattern = apply_binding_mutability(binder, mutable);
            return Some((pattern, self.span_from(start)));
        }
        // A tuple pattern `(a, b, …)` (≥2, keeping a single parenthesised pattern
        // unambiguous — there is no grouping arm).
        if self.peek_is_ctrl('(') {
            return self.attempt(|parser| {
                parser.expect_ctrl('(')?;
                let patterns =
                    parser.comma_list(Self::parse_pattern, |parser| parser.peek_is_ctrl(')'))?;
                parser.expect_ctrl(')')?;
                if patterns.len() < 2 {
                    return None;
                }
                Some((Pattern::Tuple(patterns), parser.span_from(start)))
            });
        }
        // A literal pattern — matched by equality (`bool`/`null` stay variant/keyword
        // patterns).
        let literal_node = match self.peek() {
            Some(Token::String(text)) => Some(Node::String(text)),
            Some(Token::MultilineString(text)) => Some(Node::MultilineString(text)),
            Some(Token::Number(whole, fraction, suffix)) => {
                Some(Node::Number(whole, *fraction, *suffix))
            }
            _ => None,
        };
        if let Some(node) = literal_node {
            let span = self.here_span();
            self.bump();
            return Some((
                Pattern::Literal(Box::new((node, span))),
                self.span_from(start),
            ));
        }
        // A variant path `Name (:: member)* (payload)?` — a bare `_` with no path
        // and no payload is the wildcard.
        if let Some(head) = self.eat_name() {
            let mut path = vec![head];
            loop {
                let save = self.position;
                if !self.eat_op("::") {
                    break;
                }
                match self.eat_ident() {
                    Some(member) => path.push(member),
                    None => {
                        self.position = save;
                        break;
                    }
                }
            }
            let payload = if self.peek_is_ctrl('(') {
                self.attempt(|parser| {
                    parser.expect_ctrl('(')?;
                    let patterns = parser
                        .comma_list(Self::parse_pattern, |parser| parser.peek_is_ctrl(')'))?;
                    parser.expect_ctrl(')')?;
                    Some(patterns)
                })
            } else {
                None
            };
            let pattern = if path.len() == 1 && path[0] == "_" && payload.is_none() {
                Pattern::Wildcard
            } else {
                Pattern::Variant(path, payload)
            };
            return Some((pattern, self.span_from(start)));
        }
        self.note_expected("a pattern");
        None
    }

    // --- Types ---------------------------------------------------------------

    /// A type, with the optional `context` clause suffix (`Type context name` /
    /// `context (a, b)`).
    fn parse_type(&mut self) -> Option<Spanned<Node<'src>>> {
        // The type grammar is a closed cycle that reaches no expression rule at
        // all (B142) — `& & & ..`, `[[..; 1]; 1]`, `L<L<..>>`, `((..))`, closure
        // types and bounds all come back through here, and `parse_type_atom` has
        // this as its only caller, so this is the type grammar's single door. It
        // is reachable from a bounded expression too, through a call's generic
        // arguments, which is why one level of expression nesting cannot stand in
        // for it.
        self.parse_nested(Self::TYPE_NESTING_REFUSAL, Self::parse_type_inner)
    }

    /// [`Parser::parse_type`]'s body, past the depth bound.
    fn parse_type_inner(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let inner = self.parse_type_atom()?;
        if let Some(contexts) = self.parse_context_clause() {
            Some((
                Node::TypeWithContexts(Box::new(inner), contexts),
                self.span_from(start),
            ))
        } else {
            Some(inner)
        }
    }

    /// A type without the context suffix: `[T; n]`, `&T`/`&mut T`, a `type` binder,
    /// a closure type (with the optional `async`/`sync` marker), a nominal path
    /// (`Dot`, `List<T>`, `style::Style`, `std::reactive::SignalCell<i32>`), a
    /// mapped tuple type (`(U in T: F<U>)`), or a tuple type. Tried in the chumsky
    /// order.
    fn parse_type_atom(&mut self) -> Option<Spanned<Node<'src>>> {
        if self.peek_is_ctrl('[')
            && let Some(array) = self.parse_array_type()
        {
            return Some(array);
        }
        if self.peek_is_op("&") {
            return self.parse_reference_type();
        }
        if self.peek_is(&Token::Type) || self.peek_is(&Token::Ident(ANONYMOUS_TYPE_BINDER)) {
            return self.parse_type_binder();
        }
        // B414: `dyn` is contextual — the trait-object marker at a type head,
        // except `dyn::`, which is a path into a module named `dyn`.
        if self.peek_is_word("dyn") && !self.peek_at_is_op(1, "::") {
            return self.parse_dyn_type();
        }
        if let Some(closure) = self.parse_closure_type() {
            return Some(closure);
        }
        if let Some(path) = self.parse_path_type() {
            return Some(path);
        }
        if self.peek_is_ctrl('(') {
            if let Some(mapped) = self.parse_mapped_type() {
                return Some(mapped);
            }
            if let Some(tuple) = self.parse_tuple_type() {
                return Some(tuple);
            }
        }
        self.note_expected("a type");
        None
    }

    /// `dyn Source<i32>` — a trait object type (A124 R3).
    ///
    /// The keyword takes a PATH TYPE and nothing else. `dyn |i32| str`,
    /// `dyn [T; 4]`, `dyn &T` and `dyn (A, B)` name no trait, so the grammar
    /// refuses them here rather than letting the analyzer meet a `dyn` over a
    /// closure and say something about object safety; the message names what
    /// may follow, which is the one thing the reader needs.
    ///
    /// Nesting is by the ordinary type cycle: `List<dyn Source<i32>>` reaches
    /// this production through the application's argument walk, so a `dyn`
    /// stands wherever a type stands.
    fn parse_dyn_type(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        if !self.eat_word("dyn") {
            return None;
        }
        let Some(inner) = self.parse_path_type() else {
            self.note_expected("a trait name after `dyn`");
            return None;
        };
        Some((Node::DynType(Box::new(inner)), self.span_from(start)))
    }

    /// `[T; length]` — a fixed-length array type; `length` is an integer literal.
    fn parse_array_type(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('[')?;
            let element = parser.parse_type()?;
            parser.expect_ctrl(';')?;
            let length = parser.parse_array_length()?;
            parser.expect_ctrl(']')?;
            Some((
                Node::ArrayType(Box::new(element), Box::new(length)),
                parser.span_from(start),
            ))
        })
    }

    /// An array-type length: an integer (numeric) literal.
    fn parse_array_length(&mut self) -> Option<Spanned<Node<'src>>> {
        if let Some(Token::Number(whole, fraction, suffix)) = self.peek() {
            let node = Node::Number(whole, *fraction, *suffix);
            let span = self.here_span();
            self.bump();
            Some((node, span))
        } else {
            None
        }
    }

    /// `&T` / `&mut T` — a view type.
    fn parse_reference_type(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect_op("&")?;
        let mutable = self.eat(&Token::Mut);
        let inner = self.parse_type()?;
        Some((
            Node::Reference(mutable, Box::new(inner)),
            self.span_from(start),
        ))
    }

    /// `type X (: A + B)?` / `_ (: A + B)?` — a generic binder in type position
    /// (impl subject patterns).
    ///
    /// Two spellings, one node. The keyword NAMES the parameter, and the
    /// keyword is what says "this introduces a name" in a position that
    /// otherwise reads a type. `_` (B294) introduces one the author declined to
    /// name — the pattern wildcard's spelling (`Some(_)`, `let _`) in the
    /// impl-subject position, and the only way to write a BOUND on an anonymous
    /// parameter: `_: Source<type U>` was a parse error at the `:`, because
    /// nothing but the keyword reached this production and a bare `_` was an
    /// ordinary name that no scope declared.
    ///
    /// The reading is unconditional — `_` in ANY type position is this node,
    /// not a name — so the parser stays context-free and a `_` written where no
    /// binder may be introduced (`let x: List<_>`, an inference placeholder) is
    /// refused where every other unbound name is, by resolution, with a message
    /// that says what `_` is for.
    fn parse_type_binder(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        if !self.eat(&Token::Type) && !self.peek_is(&Token::Ident(ANONYMOUS_TYPE_BINDER)) {
            return None;
        }
        let name_start = self.position;
        let name = self.eat_ident()?;
        // The NAME's own span, the way `GenericParameter` carries one: the
        // node's span reaches from the `type` keyword to the end of the bounds,
        // and what the binder's ENTITY must be spanned by is the thing an
        // editor selects for it (E161).
        let name_span = self.span_from(name_start);
        // A122: a tuple-family bound (`type T: (2..)`) is tried before the
        // trait-bound list, exactly as a generic parameter's is.
        let (bounds, tuple_bound) = if self.eat_op(":") {
            match self.parse_tuple_bound() {
                Some(bound) => (Vec::new(), Some(Box::new(bound))),
                None => (self.parse_type_bounds()?, None),
            }
        } else {
            (Vec::new(), None)
        };
        Some((
            Node::TypeBinder((name, name_span), bounds, tuple_bound),
            self.span_from(start),
        ))
    }

    /// `A + B + …` — a `+`-separated bound list (≥1).
    fn parse_type_bounds(&mut self) -> Option<Vec<Spanned<Node<'src>>>> {
        let mut bounds = vec![self.parse_type()?];
        while self.eat_op("+") {
            bounds.push(self.parse_type()?);
        }
        Some(bounds)
    }

    /// A closure type: an optional `async`/`sync` (contextual) marker, then
    /// `|param, …| return?` or `|| return?`. A closure parameter is `(name :)? type`.
    /// The whole thing backtracks (falling through to a plain local) if no `|`/`||`
    /// follows the marker — so a bare `sync` is a type name.
    fn parse_closure_type(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            enum Marker {
                Async,
                Sync,
            }
            let marker = if parser.eat(&Token::Async) {
                Some(Marker::Async)
            } else if matches!(parser.peek(), Some(Token::Ident("sync"))) {
                parser.bump();
                Some(Marker::Sync)
            } else {
                None
            };
            let inner = parser.parse_closure_type_inner()?;
            Some(match marker {
                Some(Marker::Async) => (Node::AsyncType(Box::new(inner)), parser.span_from(start)),
                Some(Marker::Sync) => (Node::SyncType(Box::new(inner)), parser.span_from(start)),
                None => inner,
            })
        })
    }

    /// The `|params| return?` core of a closure type (no marker). Note there is NO
    /// arrow: the return type follows the parameter delimiters directly.
    fn parse_closure_type_inner(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let parameters = if self.eat_op("||") {
            Vec::new()
        } else if self.eat_op("|") {
            let parameters = self.comma_list(Self::parse_closure_type_parameter, |parser| {
                parser.peek_is_op("|")
            })?;
            self.expect_op("|")?;
            parameters
        } else {
            return None;
        };
        let parameters = (parameters, self.span_from(start));
        // B520: `|i32| -> str` reads as `|i32| str`, refused at the arrow.
        self.eat_foreign_arrow(ForeignSpelling::TypeArrow);
        let return_type = self.attempt(|parser| parser.parse_type()).map(Box::new);
        Some((
            Node::ClosureType(parameters, return_type),
            self.span_from(start),
        ))
    }

    /// One closure-type parameter: `(name :)? type`.
    fn parse_closure_type_parameter(
        &mut self,
    ) -> Option<(Option<&'src str>, Box<Spanned<Node<'src>>>)> {
        let name = if matches!(self.peek(), Some(Token::Ident(_)))
            && matches!(self.peek_at(1), Some(Token::Op(":")))
        {
            let name = self.eat_ident();
            self.bump(); // `:`
            name
        } else {
            None
        };
        let type_ = self.parse_type()?;
        Some((name, Box::new(type_)))
    }

    /// `IDENT { "::" IDENT } generic-args?` — the nominal type form: a name,
    /// optionally reached through the modules that declare it, optionally applied
    /// to generic arguments. `Dot`, `List<T>`, `style::Style` and
    /// `std::reactive::SignalCell<i32>` are all this production (B172).
    ///
    /// The namespace spine folds into `StaticAccessor` nodes — the very shape an
    /// expression path builds — so a qualified type resolves through the same
    /// module-member lookup `style::style()` does, and nothing downstream needs a
    /// second notion of what a path is.
    ///
    /// Generic arguments belong to the LAST segment, which is the only one that
    /// names a type; the earlier ones name modules, which take none.
    fn parse_path_type(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut name = self.eat_ident()?;
        let mut namespace: Option<Spanned<Node<'src>>> = None;
        // A `::` continues the path only when a NAME follows it. Probing BOTH
        // tokens before committing is what keeps a trailing `::` — one that
        // belongs to whatever the caller parses next — exactly where it was,
        // without the backtracking an `eat_op` here would need.
        while self.peek_is_op("::")
            && matches!(self.peek_at(1), Some(Token::Ident(_)))
            // E145: and that name is on this line.
            && !self.peeked_separator_crosses_a_line()
        {
            let span = self.span_from(start);
            namespace = Some(match namespace {
                Some(inner) => (Node::StaticAccessor(Box::new(inner), name, None), span),
                None => (Node::Accessor(name), span),
            });
            self.bump(); // `::`
            name = self.eat_ident().expect("peeked as an identifier");
        }
        let generic_arguments = self.attempt(Self::parse_generic_arguments);
        if generic_arguments.is_some() && namespace.is_none() {
            self.refuse_generic_self(name, start);
        }
        let node = match namespace {
            Some(namespace) => Node::StaticAccessor(Box::new(namespace), name, generic_arguments),
            None => match generic_arguments {
                Some(generic_arguments) => Node::AccessorWithGenerics(name, generic_arguments),
                None => Node::Accessor(name),
            },
        };
        Some((node, self.span_from(start)))
    }

    /// `(U in T: F<U>)` — a mapped tuple type (tried before the plain tuple type,
    /// distinguished by the `in`).
    fn parse_mapped_type(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('(')?;
            let binder_start = parser.position;
            let binder = parser.eat_ident()?;
            let binder_span = parser.span_from(binder_start);
            parser.expect(&Token::In)?;
            let source = parser.parse_type()?;
            parser.expect_op(":")?;
            let template = parser.parse_type()?;
            parser.expect_ctrl(')')?;
            Some((
                Node::MappedType {
                    binder,
                    binder_span,
                    source: Box::new(source),
                    template: Box::new(template),
                },
                parser.span_from(start),
            ))
        })
    }

    /// `(A, B, …)` — a tuple type (allow-trailing, no minimum: `()` is the empty
    /// tuple and `(A)` a one-tuple, unlike a parenthesised expression).
    fn parse_tuple_type(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('(')?;
            let elements =
                parser.comma_list(Self::parse_type, |parser| parser.peek_is_ctrl(')'))?;
            parser.expect_ctrl(')')?;
            // N113: `()` reads as the EMPTY tuple, and nothing can produce one
            // — a field, parameter or return written at it is uninhabited, so
            // every program that touched it was refused somewhere else, with a
            // message about the type it did not get. The unit is `void`, and
            // that is the whole of what the author meant. Refused here rather
            // than in the analyzer because the spelling is the mistake: the
            // type is read, so the rest of the declaration still parses.
            if elements.is_empty() {
                parser.errors.push(ParseError {
                    span: parser.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "the unit type is spelled `void`: `()` is the empty tuple, \
                         which no expression can produce",
                    ),
                    context: parser.context_stack.clone(),
                    hint: None,
                });
            }
            Some((Node::Tuple(elements), parser.span_from(start)))
        })
    }

    /// `context name` / `context (a, b)` — the optional context clause on a type
    /// (`context` is contextual: an `Ident`, so `std::context` paths stay legal).
    fn parse_context_clause(&mut self) -> Option<Vec<(&'src str, Span)>> {
        if !matches!(self.peek(), Some(Token::Ident("context"))) {
            return None;
        }
        self.attempt(|parser| {
            parser.bump(); // `context`
            if parser.peek_is_ctrl('(') {
                parser.expect_ctrl('(')?;
                let names = parser
                    .comma_list(Self::parse_context_name, |parser| parser.peek_is_ctrl(')'))?;
                parser.expect_ctrl(')')?;
                if names.is_empty() {
                    return None;
                }
                Some(names)
            } else {
                let start = parser.position;
                let name = parser.eat_ident()?;
                Some(vec![(name, parser.span_from(start))])
            }
        })
    }

    fn parse_context_name(&mut self) -> Option<(&'src str, Span)> {
        let start = self.position;
        let name = self.eat_ident()?;
        Some((name, self.span_from(start)))
    }

    // --- Item bodies ---------------------------------------------------------

    /// `{ statement* }` — an item body (an `impl` or `mod` block): a bare statement
    /// list with NO trailing expression (unlike [`Parser::parse_block`]), carrying
    /// the `{ .. }` span. A garbled body recovers to an empty body, and the item
    /// after synchronizes (chumsky's `nested_delimiters`, sites 8 `impl` / 10
    /// `module` of 10 — identical fallback `(Vec::new(), span)`, `production`
    /// naming which for the message).
    fn parse_item_body(&mut self, production: &'static str) -> Option<Spanned<NodeList<'src>>> {
        if let Some(clean) = self.attempt(Self::parse_item_body_clean) {
            return Some(clean);
        }
        self.recover_delimited(production, '{', '}', &[('(', ')'), ('[', ']')])
            .map(|span| (Vec::new(), span))
    }

    /// The clean `{ statement* }` parse, wrapped by [`Parser::parse_item_body`]'s
    /// recovery.
    fn parse_item_body_clean(&mut self) -> Option<Spanned<NodeList<'src>>> {
        let start = self.position;
        self.expect_ctrl('{')?;
        let mut statements = Vec::new();
        loop {
            if self.peek_is_ctrl('}') || self.at_end() {
                break;
            }
            match self.parse_statement() {
                Some(statement) => statements.push(statement),
                None => break,
            }
        }
        self.expect_ctrl('}')?;
        Some((statements, self.span_from(start)))
    }

    /// `{ function* }` — a trait body: a list of function declarations ONLY (not
    /// arbitrary statements), carrying the `{ .. }` span. A garbled body recovers
    /// to an empty body (chumsky's `nested_delimiters` on the trait body, site 9
    /// of 10).
    fn parse_trait_body(&mut self) -> Option<Spanned<NodeList<'src>>> {
        if let Some(clean) = self.attempt(Self::parse_trait_body_clean) {
            return Some(clean);
        }
        self.recover_delimited("trait body", '{', '}', &[('(', ')'), ('[', ']')])
            .map(|span| (Vec::new(), span))
    }

    /// The clean `{ function* }` parse, wrapped by [`Parser::parse_trait_body`]'s
    /// recovery.
    fn parse_trait_body_clean(&mut self) -> Option<Spanned<NodeList<'src>>> {
        let start = self.position;
        self.expect_ctrl('{')?;
        let mut functions = Vec::new();
        loop {
            if self.peek_is_ctrl('}') || self.at_end() {
                break;
            }
            self.take_foreign_item_word();
            self.canonicalize_marker_run();
            match self.attempt(Self::parse_function) {
                Some(function) => functions.push(function),
                None => break,
            }
        }
        self.expect_ctrl('}')?;
        Some((functions, self.span_from(start)))
    }

    // --- Generic parameters --------------------------------------------------

    /// `<param, ...>` — generic PARAMETERS in declaration position (allow-trailing),
    /// or `None` (backtracking) when no well-formed `<...>` is present. Distinct
    /// from [`Parser::parse_generic_arguments`] (types). A balanced-but-garbled
    /// `<...>` recovers to an empty parameter vec (chumsky's `nested_delimiters` on
    /// `generic_parameters`, site 1 of 10).
    fn parse_generic_parameters(&mut self) -> Option<GenericParameters<'src>> {
        if let Some(clean) = self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('<')?;
            let parameters = parser.comma_list(Self::parse_generic_parameter, |parser| {
                parser.peek_is_ctrl('>')
            })?;
            parser.expect_ctrl('>')?;
            Some((parameters, parser.span_from(start)))
        }) {
            return Some(clean);
        }
        self.recover_delimited(
            "generic parameters",
            '<',
            '>',
            &[('(', ')'), ('[', ']'), ('{', '}')],
        )
        .map(|span| (Vec::new(), span))
    }

    /// One generic parameter: `type? name (: (tuple_bound | A + B))? (= default)?`.
    /// The `type` marker makes it a binder (impl subject patterns); the bound is a
    /// tuple bound (`(2..)`) tried before the `+`-separated trait-bound list.
    fn parse_generic_parameter(&mut self) -> Option<GenericParameter<'src>> {
        let is_type = self.eat(&Token::Type);
        let name_start = self.position;
        let name = self.eat_ident()?;
        let name_span = self.span_from(name_start);
        let (bounds, tuple_bound) = if self.eat_op(":") {
            match self.parse_tuple_bound() {
                Some(bound) => (Vec::new(), Some(bound)),
                None => (self.parse_type_bounds()?, None),
            }
        } else {
            (Vec::new(), None)
        };
        let default = if self.eat_op("=") {
            Some(Box::new(self.parse_type()?))
        } else {
            None
        };
        Some(GenericParameter {
            name,
            name_span,
            is_type,
            bounds,
            tuple_bound,
            default,
        })
    }

    /// `(lo?..hi? (: element)?)` — a tuple-arity bound (`T: (2..)`, `(..: Display)`).
    /// The `..` is two `.` control tokens (NO adjacency check, matching the chumsky
    /// `dot_dot`, unlike the shift operator). Backtracks when the `(` does not open
    /// an `int? .. …` shape (so a tuple-type bound `(A, B)` falls through to the
    /// trait-bound list). Endpoints that do not parse as `u32` become `None`.
    fn parse_tuple_bound(&mut self) -> Option<TupleBound<'src>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('(')?;
            let lo = parser.eat_integer();
            parser.expect_ctrl('.')?;
            parser.expect_ctrl('.')?;
            let hi = parser.eat_integer();
            let element = if parser.eat_op(":") {
                Some(Box::new(parser.parse_type()?))
            } else {
                None
            };
            parser.expect_ctrl(')')?;
            Some(TupleBound {
                lo: lo.and_then(|whole| whole.parse::<u32>().ok()),
                hi: hi.and_then(|whole| whole.parse::<u32>().ok()),
                element,
                span: parser.span_from(start),
            })
        })
    }

    // --- Functions -----------------------------------------------------------

    /// A function declaration: the ORDERED attribute prefix (`[deprecated(..)]`,
    /// `[extern(..)]`, `[must_use]`, `[rpc]`, `[trait_only]`,
    /// `[platform(..)]` — each optional but IN THIS ORDER, a faithful quirk),
    /// then `async? external?
    /// fun name generics? (params) (: return)? (borrows param)? (block | ;)`.
    fn parse_function(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let deprecated = self.parse_deprecated_attribute();
        let internal = self.parse_internal_attribute();
        let (extern_binding, extern_retains) = match self.parse_extern_attribute() {
            Some((binding, retains)) => (Some(binding), retains),
            None => (None, false),
        };
        let must_use = self.eat_marker_attribute("must_use");
        let track_caller = self.eat_marker_attribute("track_caller");
        let rpc = self.eat_marker_attribute("rpc");
        let trait_only = self.eat_marker_attribute("trait_only");
        self.refuse_doc_hidden_attribute();
        let platform_fence = self.parse_platform_attribute().unwrap_or_default();
        let is_async = self.eat(&Token::Async);
        let external = self.eat(&Token::External);
        self.expect(&Token::Fun)?;
        let name_start = self.position;
        // B414 S4: a METHOD — a `fun` in an `impl` or `trait` item list — is
        // reached through a receiver or a `::` path, never through the atom
        // production, so any word names it; a free function binds a name and
        // keeps the identifier rule.
        let name = if self.in_member_body {
            self.eat_member_name()?
        } else {
            self.eat_ident()?
        };
        let name = (name, self.span_from(name_start));
        let generic_parameters = self.parse_generic_parameters();
        let parameters = self.parse_function_parameters()?;
        if external {
            // An external has no Vilan body, so there is no copy to mutate —
            // a `mut` parameter on one can only be a misunderstanding.
            for parameter in &parameters.0 {
                if parameter.mutable {
                    self.errors.push(ParseError {
                        span: parameter.span,
                        reason: ParseErrorReason::Rule(
                            "an `external fun` has no body; `mut` marks a \
                             parameter the body mutates, so it has no meaning here",
                        ),
                        context: Vec::new(),
                        hint: None,
                    });
                }
            }
            self.reject_spread_position(
                &parameters.0,
                "an `external fun` binds a host function, whose calling convention is \
                 the host's; declare a tuple parameter (`items: T`) instead",
            );
            self.reject_lazy_position(
                &parameters.0,
                "an `external fun` binds a host function, whose calling convention is \
                 the host's: nothing on that side forces a thunk. Take the value \
                 eagerly, or a closure the host calls",
            );
        }
        if self.in_member_body {
            self.reject_spread_position(
                &parameters.0,
                "a spread parameter is only available on a free `fun`: it is part of \
                 the signature, and a method is reached by dispatch (inherent, through \
                 a trait, through a generic bound). Declare a tuple parameter \
                 (`items: T`) and call it `m((a, b))`",
            );
        }
        self.reject_misplaced_spread(&parameters.0);
        // B520: `fun f() -> T` reads as `fun f(): T`, refused at the arrow.
        let mut return_type = if self.eat_op(":") || self.eat_foreign_arrow(ForeignSpelling::Arrow)
        {
            Some(Box::new(self.in_context("return type", Self::parse_type)?))
        } else {
            None
        };
        // The DECLARATION's `context` clause (B242): the contexts the body may
        // read, stated on the signature. Two ways it arrives, because the type
        // grammar carries the same suffix (§3.9) and takes it greedily:
        //
        //   `fun f(): i32 context settings`  — `parse_type` swallowed it as a
        //     clause on the return type `i32`, where it means nothing and was
        //     refused. PEELED here: the clause binds to the FUNCTION and
        //     `i32` is the return type, which is the reading anyone writing it
        //     meant.
        //
        // B309 splits that rule in two, because a CLOSURE return type can now
        // carry a clause of its own: `fun f(): (|| View) context owner_scope`
        // returns an INJECTED closure, and the clause is the type's. So the
        // peel happens only where the return type cannot carry one — which is
        // every shape B242 was written for, and none of B309's.
        //   `fun f(x: i32) context settings` — no return type to swallow it, so
        //     it is still on the token stream; parsed below, after `borrows`.
        //
        // Both record the NAME LIST's span, not the `context` word's: that is
        // what the editor's "declare the inferred contexts" fix rewrites, and
        // it is the same span whichever way the clause arrived. The formatter
        // normalizes the two arrivals into ONE printed position — last, after
        // `borrows` (E146 rule 3) — through a token canonicalization its
        // safety net shares, so either way in is the same way out.
        // B343 (R9): the one shape the position cannot spell, refused at the
        // head rather than mis-bound. See [`take_misbound_return_clause`].
        if let Some(annotation) = return_type.as_deref_mut() {
            // The WRITTEN return type, taken before the clause comes off: it
            // spans `|| void context c` whole, which is what the editor's
            // parenthesizing fix rewrites and what makes the refusal point at
            // the thing it is asking to be changed rather than at one name.
            let written = annotation.1;
            if take_misbound_return_clause(&mut annotation.0) {
                self.errors.push(ParseError {
                    span: written,
                    reason: ParseErrorReason::Rule(MISBOUND_RETURN_CLAUSE),
                    context: Vec::new(),
                    hint: None,
                });
            }
        }
        let mut contexts: Option<(Vec<Spanned<&'src str>>, Span)> = None;
        if let Some(annotation) = return_type.take() {
            // E233: `&T context c` reads the clause on the VIEW's target (the
            // `&` production takes a whole type), which is the same mis-binding
            // the peel below undoes for a bare `T` — hoisted first so both
            // spellings reach it as one shape.
            let annotation = hoist_clause_out_of_a_view(*annotation);
            match annotation.0 {
                Node::TypeWithContexts(inner, names)
                    if !return_type_carries_its_own_clause(&inner.0) =>
                {
                    let clause_start = names
                        .first()
                        .map(|(_, span)| span.start)
                        .unwrap_or(annotation.1.start);
                    contexts = Some((names, Span::from(clause_start..annotation.1.end)));
                    return_type = Some(Box::new(*inner));
                }
                other => return_type = Some(Box::new((other, annotation.1))),
            }
        }
        // `borrows <param>` — the returned view is a projection of that parameter.
        // Contextual (B414): positional, after the return type, where no name
        // can stand.
        let borrows = if self.eat_word("borrows") {
            Some(self.eat_ident()?)
        } else {
            None
        };
        if contexts.is_none() {
            let clause_start = self.position;
            if let Some(names) = self.parse_context_clause() {
                let whole = self.span_from(clause_start);
                let start = names
                    .first()
                    .map(|(_, span)| span.start)
                    .unwrap_or(whole.start);
                contexts = Some((names, Span::from(start..whole.end)));
            }
        }
        // E148: where a `context` clause would be inserted — after the return
        // type and after a `borrows` clause, before the body. Taken here,
        // because this is the one moment the position exists: the analyzed
        // program records where a signature's PIECES are and never where the
        // signature ends, so the editor's fix had nowhere to write on a
        // function carrying no clause.
        let signature_end = self
            .position
            .checked_sub(1)
            .and_then(|at| self.tokens.get(at))
            .map(|(_, span)| {
                let end = span.into_range().end;
                Span::from(end..end)
            });
        // A block body, or `;` for a signature-only declaration (a required trait
        // method or an `external` intrinsic). The block is tried first (chumsky
        // `block.map(Some).or(';'.map(|_| None))`), but the two lead on disjoint
        // tokens (`{` vs `;`) so a bare `;` short-circuits equivalently.
        // A function declared INSIDE a member's body is a free function, not a
        // member — memberhood belongs to the impl/trait item list, not to
        // everything lexically within it. Clear the flag for the body (the
        // member's own parameters were checked above, before this point).
        let body = if self.eat_ctrl(';') {
            None
        } else {
            let outer_member_body = std::mem::take(&mut self.in_member_body);
            let block = self.parse_block();
            self.in_member_body = outer_member_body;
            match block {
                Some(block) => Some(block),
                None => {
                    // A COMMITTED demand, noted so the failure is located
                    // (B172): `fun` + a name + `(params)` is a function and
                    // nothing else in the grammar, so once the head has parsed
                    // there is no alternative reading left to be quiet for. An
                    // opening `{` is otherwise a silent head-check
                    // (`expect_ctrl`), which left the farthest failure with the
                    // `expression ;` alternative tried BEFORE this one — and
                    // that one declines on the `fun` keyword, so a function
                    // whose body was missing reported `found 'fun' expected an
                    // expression` at the item's own first column.
                    //
                    // The `;` alternative is NOT spelled `"';'"`: that is
                    // `TERMINATOR_EXPECTED` verbatim, and `emit_failure` reads
                    // it as a statement that lost its terminator — which would
                    // re-anchor this failure into the preceding gap and ask for
                    // a `;` to end a statement that is not there.
                    self.note_expected("'{'");
                    self.note_expected("';' for a bodyless declaration");
                    return None;
                }
            }
        };
        Some((
            Node::Func(Box::new(Func {
                name,
                is_async,
                external,
                deprecated,
                internal,
                extern_binding,
                extern_retains,
                must_use,
                track_caller,
                rpc,
                trait_only,
                platform_fence,
                generic_parameters,
                parameters,
                return_type,
                borrows,
                contexts,
                signature_end,
                body,
            })),
            self.span_from(start),
        ))
    }

    /// `(param, ...)` — a function parameter list (allow-trailing), carrying the
    /// `( .. )` span.
    fn parse_function_parameters(&mut self) -> Option<Spanned<Vec<Parameter<'src>>>> {
        let start = self.position;
        self.expect_ctrl('(')?;
        let parameters = self.comma_list(Self::parse_function_parameter, |parser| {
            parser.peek_is_ctrl(')')
        })?;
        self.expect_ctrl(')')?;
        Some((parameters, self.span_from(start)))
    }

    /// One function parameter:
    /// `"lazy"? (mut | own | & mut?)? "..."? binder (: type)?`. The convention
    /// is the explicit prefix, else inferred from a `&T` / `&mut T` type, else
    /// `Bare`. A leading `mut` is binder mutability, not a convention
    /// (proposal/mut-parameters.md): the body may rebind and field-write its
    /// by-value copy, invisibly to the caller. A `...` marks a SPREAD parameter
    /// (proposal/variadic-generics.md §S) — a call convention over an ordinary
    /// tuple parameter. A leading `lazy` marks a LAZY parameter
    /// (proposal/lazy.md §1): the argument is thunked at the call site and
    /// forced on the parameter's first read. `lazy` is first because it is what
    /// the CALL SITE does with the argument, before any question of how the
    /// callee receives it — and it composes with none of the answers to that
    /// question (the three rules below).
    fn parse_function_parameter(&mut self) -> Option<Parameter<'src>> {
        let start = self.position;
        // B414: `lazy` and `own` are contextual — each is the prefix only
        // when a binder follows it (`eat_binder_prefix`), so a parameter may
        // be NAMED either (`own: Owner`, `|lazy| …`).
        let lazy = self.eat_binder_prefix("lazy");
        let mutable = self.eat(&Token::Mut);
        let prefix = if self.eat_binder_prefix("own") {
            Some(Convention::Own)
        } else if self.eat_op("&") {
            Some(if self.eat(&Token::Mut) {
                Convention::RefMut
            } else {
                Convention::Ref
            })
        } else {
            None
        };
        // `own mut x` / `&mut mut x` — a stray `mut` after the convention,
        // consumed here so the binder still parses and the rule below fires.
        let misplaced_mut = prefix.is_some() && self.eat(&Token::Mut);
        // `own lazy x` / `mut lazy x` — `lazy` written AFTER the prefix it
        // belongs in front of. Consumed for `misplaced_mut`'s reason: `lazy`
        // composes with neither, so the binder still parses and one of the
        // composition rules below names the real problem, instead of the
        // backtrack throwing "expected an expression" at the next declaration
        // (diagnostics-standard B5).
        let lazy = lazy | self.eat_binder_prefix("lazy");
        let spread = self.eat_spread();
        let Some((pattern, pattern_span)) = self.parse_binder() else {
            // B446: the binder noted the name it wanted. With nothing of this
            // parameter read yet, the list's `)` would have done as well — the
            // list asks for an item only once its closer is not next — so the
            // report names both: `fun broken( {` is "found '{' expected a name
            // or ')'", the located demand `recover_statement` ranks above the
            // bare unclosed `(`.
            if self.position == start {
                self.note_expected("')'");
            }
            return None;
        };
        let parameter_type = if self.eat_op(":") {
            Some(Box::new(
                self.in_context("parameter type", Self::parse_type)?,
            ))
        } else {
            None
        };
        let convention =
            prefix.unwrap_or_else(
                || match parameter_type.as_deref().map(|spanned| &spanned.0) {
                    Some(Node::Reference(true, _)) => Convention::RefMut,
                    Some(Node::Reference(false, _)) => Convention::Ref,
                    _ => Convention::Bare,
                },
            );
        // `mut` marks the callee's by-value copy writable, so it composes with
        // nothing that changes how the argument is received: not `own` (already
        // rebindable), not a view (mutability there belongs to the target, via
        // `&mut`). The inferred-convention arm catches `mut x: &T` too.
        if (mutable && convention != Convention::Bare) || misplaced_mut {
            let span = self.span_from(start);
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Rule(
                    "`mut` makes the parameter's by-value copy writable; it cannot \
                     combine with `own` or a view (`&`, `&mut`), which choose how \
                     the argument is received",
                ),
                context: Vec::new(),
                hint: None,
            });
        }
        self.reject_mut_destructure(mutable, &pattern, pattern_span);
        if lazy {
            // A lazy argument is a THUNK the call site builds — there is no
            // caller-side place for `own` to transfer or for a view to alias,
            // and the value does not exist until the callee forces it
            // (lazy.md §1). The inferred-convention arm catches `lazy x: &T`
            // too.
            if convention != Convention::Bare {
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "a `lazy` parameter receives a thunk the call site builds, and \
                         the value does not exist until the callee forces it, so there \
                         is nothing for `own` or a view (`&`, `&mut`) to transfer or \
                         alias",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
            // `mut` makes the callee's by-value copy writable; a lazy parameter
            // reads through its memo cell on every use, so there is no copy to
            // rebind. `mut x = <the parameter>` in the body is the spelling.
            if mutable {
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "a `lazy` parameter is forced on its first read and memoized, so \
                         there is no by-value copy for `mut` to make writable; bind the \
                         forced value in the body (`mut value = name;`)",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
            // The pack is built by the CALL SITE out of the collected
            // arguments — one thunk cannot stand for a variable number of
            // argument expressions.
            if spread {
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "a spread parameter collects a variable number of argument \
                         expressions into one pack, and `lazy` defers ONE expression; \
                         declare a tuple parameter, or make the pack's elements lazy \
                         where they are read",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
        }
        if spread {
            // A spread parameter's argument is a value the CALL SITE builds out
            // of the collected arguments — there is no caller-side tuple to
            // transfer or to alias, so a rule-3 convention has nothing to name
            // (variadic-generics.md §S.5). The inferred-convention arm catches
            // `...items: &T` too.
            if convention != Convention::Bare {
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "a spread parameter receives a tuple the call site builds from \
                         the collected arguments, so there is nothing for `own` or a \
                         view (`&`, `&mut`) to transfer or alias",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
            // The pack is what the collection PRODUCES, so it cannot be
            // inferred from the collection whose shape it defines (§S.3).
            if parameter_type.is_none() {
                self.errors.push(ParseError {
                    span: self.span_from(start),
                    reason: ParseErrorReason::Rule(
                        "a spread parameter must declare its pack type \
                         (`...items: T` with `T: (..)`, a mapped tuple, or a tuple type)",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
            // Destructuring the pack in the signature would hide the arity rule
            // the call site has to satisfy; destructure in the body (§S.3).
            if !matches!(pattern, Pattern::Binding(..)) {
                self.errors.push(ParseError {
                    span: pattern_span,
                    reason: ParseErrorReason::Rule(
                        "a spread parameter binds the whole pack to a plain name; \
                         destructure it in the body",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
        }
        Some(Parameter {
            pattern,
            declared_type: parameter_type,
            convention,
            mutable,
            spread,
            lazy,
            span: pattern_span,
        })
    }

    /// `...` — the spread-parameter marker, three `.` control tokens (the lexer
    /// has no `...`, exactly as the tuple bound's `..` is two; there is no
    /// adjacency check, matching `parse_tuple_bound`). Consumes nothing unless
    /// all three are present.
    fn eat_spread(&mut self) -> bool {
        if self.peek_is_ctrl('.') && self.peek_at_is_ctrl(1, '.') && self.peek_at_is_ctrl(2, '.') {
            self.bump();
            self.bump();
            self.bump();
            return true;
        }
        false
    }

    /// `mut` on a parameter applies to a plain name binder; a destructure gets
    /// its mutable pieces in the body (`mut (a, b) = pair;`).
    fn reject_mut_destructure(&mut self, mutable: bool, pattern: &Pattern<'src>, span: Span) {
        if mutable && !matches!(pattern, Pattern::Binding(..)) {
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Rule(
                    "`mut` on a parameter applies to a plain name; destructure in \
                     the body (`mut (a, b) = pair;`) for mutable pieces",
                ),
                context: Vec::new(),
                hint: None,
            });
        }
    }

    /// A spread parameter collects the arguments from its position onward, so
    /// it can only be the LAST parameter — and therefore there is at most one
    /// per signature (variadic-generics.md §S.3). Both readings of a misplaced
    /// one are the same error, reported at the offending parameter.
    fn reject_misplaced_spread(&mut self, parameters: &[Parameter<'src>]) {
        let last = parameters.len().saturating_sub(1);
        for (index, parameter) in parameters.iter().enumerate() {
            if parameter.spread && index != last {
                self.errors.push(ParseError {
                    span: parameter.span,
                    reason: ParseErrorReason::Rule(
                        "a spread parameter collects every argument from its position \
                         onward, so it must be the last parameter (and there can be \
                         only one)",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
            }
        }
    }

    /// A spread parameter is refused wherever the desugar's left-hand side is
    /// not a free `fun` declaration (variadic-generics.md §S.6/§S.7): a closure
    /// literal (a closure TYPE has no variadic form, so such a closure could
    /// never be named, stored, or passed), a trait declaration or any `impl`
    /// member (`...` IS part of the signature, unlike `mut`, and a method is
    /// reached by dispatch on three routes), and an `external fun` (the host's
    /// calling convention has no pack form).
    fn reject_spread_position(&mut self, parameters: &[Parameter<'src>], reason: &'static str) {
        for parameter in parameters.iter().filter(|parameter| parameter.spread) {
            self.errors.push(ParseError {
                span: parameter.span,
                reason: ParseErrorReason::Rule(reason),
                context: Vec::new(),
                hint: None,
            });
        }
    }

    /// A `lazy` parameter is refused wherever the thunk has no caller that could
    /// build it or no callee that could force it (lazy.md §1): a closure literal
    /// (a closure TYPE has no lazy form, so the modifier would be invisible
    /// wherever the closure travels) and an `external fun` (the host's calling
    /// convention forces nothing). The THREE homes that keep it are the three
    /// the paper names — a free `fun`, an `impl` method and a `trait`
    /// signature — so, unlike `...`, this is NOT refused in a member body.
    fn reject_lazy_position(&mut self, parameters: &[Parameter<'src>], reason: &'static str) {
        for parameter in parameters.iter().filter(|parameter| parameter.lazy) {
            self.errors.push(ParseError {
                span: parameter.span,
                reason: ParseErrorReason::Rule(reason),
                context: Vec::new(),
                hint: None,
            });
        }
    }

    // --- Structs / enums -----------------------------------------------------

    /// `labels [resource]? external? struct (name | null) generics? ({ fields } |
    /// ;)`. The `[resource]` attribute (B413; the keyword it replaced sat in the
    /// same place) closes the label prefix, ahead of `external` (canonical order
    /// `[resource] external struct`); the name may be the `null` keyword (the
    /// built-in `external struct null`); a bodyless `;` form is valid only for an
    /// `external` struct (checked past the parser).
    fn parse_struct(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let labels = self.parse_item_labels();
        let resource = self.parse_resource_kind();
        let external = self.eat(&Token::External);
        self.expect(&Token::Struct)?;
        let name_start = self.position;
        let name = if let Some(name) = self.eat_ident() {
            name
        } else if self.eat(&Token::Null) {
            "null"
        } else {
            return None;
        };
        let name = (name, self.span_from(name_start));
        let generic_parameters = self.parse_generic_parameters();
        let body = if self.peek_is_ctrl('{') {
            // The `{ field, ... }` body, clean or recovered to empty fields on a
            // garbled body (chumsky's `nested_delimiters` on the struct body, site
            // 7 of 10).
            let fields = match self.attempt(|parser| {
                let fields_start = parser.position;
                parser.expect_ctrl('{')?;
                let fields = parser
                    .comma_list(Self::parse_struct_field, |parser| parser.peek_is_ctrl('}'))?;
                parser.expect_ctrl('}')?;
                Some((fields, parser.span_from(fields_start)))
            }) {
                Some(clean) => clean,
                None => {
                    let span =
                        self.recover_delimited("struct body", '{', '}', &[('(', ')'), ('[', ']')])?;
                    (Vec::new(), span)
                }
            };
            Some(fields)
        } else if self.eat_ctrl(';') {
            None
        } else {
            return None;
        };
        Some((
            Node::Struct(
                name,
                generic_parameters.map(Box::new),
                external,
                resource,
                body.map(Box::new),
                labels,
            ),
            self.span_from(start),
        ))
    }

    /// `[internal(..)]? [expose]? name (: type)?` — one struct field, carrying
    /// the whole-field span (the inner name keeps its own span).
    fn parse_struct_field(&mut self) -> Option<Spanned<StructField<'src>>> {
        let start = self.position;
        // E213's label leads, as it does on a function: it is about the field
        // rather than about what crosses the wire.
        let internal = self.parse_internal_attribute();
        // B413: a FIELD is no type declaration; refused, and parsed past.
        self.refuse_misplaced_resource_attribute();
        // A142 S7: the store's knobs, on either side of `[expose]` — both are
        // about what the field becomes elsewhere, and neither outranks the other.
        let reactivity = self.parse_reactive_attribute();
        let exposed = self.eat_expose_attribute();
        let reactivity = match reactivity {
            Some(written) => written,
            None => match self.parse_reactive_attribute() {
                Some(written) => Reactivity {
                    after_expose: true,
                    ..written
                },
                None => Reactivity::default(),
            },
        };
        let name_start = self.position;
        // B414 S4: a declared field is a member position — any word.
        let name = self.eat_member_name()?;
        let name = (name, self.span_from(name_start));
        let type_ = if self.eat_op(":") {
            Some(self.parse_type()?)
        } else {
            None
        };
        Some((
            (name, type_, exposed, internal, reactivity),
            self.span_from(start),
        ))
    }

    /// `[reactive(coarse)]`, `[reactive(name = "nick")]`, or both, comma-separated
    /// (tracker A142 S7, `proposal/store.md` Q4 and Q9): a struct field's store
    /// knobs, read by `[derive(Storable)]` and by nothing else. `None` when no
    /// such attribute leads.
    ///
    /// COMMITTED once `[reactive` is read: an argument this does not know, or a
    /// `name` that is not an identifier, is refused where it stands and the
    /// attribute is parsed past, rather than rolled back into "expected a field
    /// name" at the bracket.
    fn parse_reactive_attribute(&mut self) -> Option<Reactivity<'src>> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("reactive")) {
                return None;
            }
            parser.bump();
            Some(())
        })?;
        let mut reactivity = Reactivity::default();
        if self.expect_ctrl('(').is_none() {
            self.refuse_reactive_argument(self.span_from(self.position));
            self.skip_past_attribute();
            return Some(reactivity);
        }
        loop {
            let argument_start = self.position;
            if self.peek() == Some(&Token::Ident("coarse")) {
                self.bump();
                reactivity.coarse = true;
            } else if self.peek() == Some(&Token::Ident("name")) {
                self.bump();
                let literal_start = self.position;
                let written = if self.eat_op("=") {
                    match self.peek() {
                        Some(Token::String(text)) => {
                            let text = *text;
                            self.bump();
                            Some(text)
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                match written {
                    Some(text) if is_identifier_text(text) => reactivity.name = Some(text),
                    Some(_) => {
                        self.errors.push(ParseError {
                            span: self.span_from(literal_start),
                            reason: ParseErrorReason::Rule(
                                "`[reactive(name = \"..\")]` names the field's PROJECTION — a \
                                 method on the store handle — so it must be an identifier: \
                                 letters, digits and `_`, not starting with a digit",
                            ),
                            context: Vec::new(),
                            hint: None,
                        });
                    }
                    None => {
                        self.refuse_reactive_argument(self.span_from(argument_start));
                        self.skip_past_attribute();
                        return Some(reactivity);
                    }
                }
            } else {
                self.bump();
                self.refuse_reactive_argument(self.span_from(argument_start));
                self.skip_past_attribute();
                return Some(reactivity);
            }
            if !self.eat_ctrl(',') {
                break;
            }
        }
        if self.expect_ctrl(')').is_none() || self.expect_ctrl(']').is_none() {
            self.refuse_reactive_argument(self.span_from(self.position));
            self.skip_past_attribute();
        }
        Some(reactivity)
    }

    /// The one refusal `[reactive(..)]`'s arguments share.
    fn refuse_reactive_argument(&mut self, span: Span) {
        self.errors.push(ParseError {
            span,
            reason: ParseErrorReason::Rule(
                "`[reactive(..)]` on a field takes `coarse` (one slot, compared whole, even when \
                 the field's type derives `Storable`) and `name = \"..\"` (the name its store \
                 projection is generated under), comma-separated: `[reactive(coarse)]`, \
                 `[reactive(name = \"nick\")]`",
            ),
            context: Vec::new(),
            hint: None,
        });
    }

    /// Skip to just past the `]` that closes the attribute being refused, so the
    /// field after it still parses.
    fn skip_past_attribute(&mut self) {
        while let Some(token) = self.peek() {
            if token == &Token::Ctrl(']') {
                self.bump();
                return;
            }
            if token == &Token::Ctrl('}') {
                return;
            }
            self.bump();
        }
    }

    /// `labels [resource]? enum name generics? { variants }`. There is no
    /// `external enum`, so `[resource]` (B413) is the only kind attribute.
    fn parse_enum(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let labels = self.parse_item_labels();
        let resource = self.parse_resource_kind();
        self.expect(&Token::Enum)?;
        let name_start = self.position;
        let name = self.eat_ident()?;
        let name = (name, self.span_from(name_start));
        let generic_parameters = self.parse_generic_parameters();
        let variants_start = self.position;
        self.expect_ctrl('{')?;
        let variants =
            self.comma_list(Self::parse_enum_variant, |parser| parser.peek_is_ctrl('}'))?;
        self.expect_ctrl('}')?;
        let variants = (variants, self.span_from(variants_start));
        Some((
            Node::Enum(
                name,
                generic_parameters.map(Box::new),
                resource,
                Box::new(variants),
                labels,
            ),
            self.span_from(start),
        ))
    }

    /// One enum variant: `[internal(..)]? name (payload types)? (= backing
    /// value)?`, carrying the whole-variant span.
    fn parse_enum_variant(&mut self) -> Option<Spanned<EnumVariant<'src>>> {
        let start = self.position;
        // E221: a variant's label leads it, as a field's does (E213).
        let internal = self.parse_internal_attribute();
        // B413: nor is a VARIANT; refused, and parsed past.
        self.refuse_misplaced_resource_attribute();
        let name = self.eat_name()?;
        let data = self.attempt(|parser| {
            parser.expect_ctrl('(')?;
            let types = parser.comma_list(Self::parse_type, |parser| parser.peek_is_ctrl(')'))?;
            parser.expect_ctrl(')')?;
            Some(types)
        });
        let backing = self.parse_backing_literal();
        Some((
            (name, data.unwrap_or_default(), backing, internal),
            self.span_from(start),
        ))
    }

    /// `= ( (-)? NUMBER | STRING )` — an explicit enum backing value, or `None`
    /// (backtracking) when no `=` follows.
    ///
    /// The string arm is `proposal/backed-enums.md` §3.1: a GENERALIZATION of
    /// the integer discriminant, not a second kind of enum. Both arms are
    /// carried UNREDUCED — the number token also admits a fraction and a
    /// suffix, and reducing here is what turned an overflowing magnitude into
    /// `0` (B79); the string's text is the raw source slice, unescaped at
    /// emission like any other literal. The analyzer reads the value and
    /// rejects every spelling the production does not mean.
    fn parse_backing_literal(&mut self) -> Option<BackingLiteral<'src>> {
        if !self.peek_is_op("=") {
            // A speculative head-check that still records the demand, so a
            // missing list separator after a variant keeps steering to `=` as
            // well as `,`/`}` (the S5 shape) — `expect_op` did this implicitly
            // before the production committed after its `=`.
            self.note_expected("'='");
            return None;
        }
        // COMMITTED once the `=` is seen: nothing else in a variant may follow
        // one, so a backtrack here would blame the `=` for what came after it
        // ("found '=' expected ','" pointed at the wrong token). §3.4's type set
        // is what the failure states — `str` and the integers are the whole of
        // it, `= true` and `= 1.5f` included by exclusion.
        self.bump();
        let start = self.position;
        if let Some(Token::String(text)) = self.peek() {
            let text = *text;
            self.bump();
            return Some(BackingLiteral::Str {
                text,
                span: self.span_from(start),
            });
        }
        let negative = self.eat_op("-");
        match self.eat_number() {
            Some((whole, fraction, suffix)) => Some(BackingLiteral::Int {
                negative,
                whole,
                fraction,
                suffix,
                span: self.span_from(start),
            }),
            None => {
                // §3.4's type set, stated as the farthest-failure expectation:
                // `str` and the integers are the whole of it, so `= true` and
                // `= 1.5f64` are refused by the production rather than by a
                // later rule.
                self.note_expected("an integer");
                self.note_expected("a string");
                None
            }
        }
    }

    // --- impl / trait / mod --------------------------------------------------

    /// `impl <subject> (with A + B)? { statements }`. The subject is a type (its
    /// `type X` binders declare the impl's generics); the optional `with` clause is
    /// the `+`-separated list of implemented traits.
    fn parse_impl(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let labels = self.parse_item_labels();
        self.expect(&Token::Impl)?;
        let subject = self.parse_type()?;
        // B414: `with` is contextual, and positional — after the subject no
        // name can stand (a type path never consumes a following identifier).
        let traits = if self.eat_word("with") {
            self.parse_type_bounds()?
        } else {
            Vec::new()
        };
        let body =
            self.within_member_body(|parser| parser.parse_item_body("implementation body"))?;
        Some((
            Node::Impl(Box::new(subject), traits, body, labels),
            self.span_from(start),
        ))
    }

    /// Parses `body` with [`Parser::in_member_body`] set, restoring the previous
    /// value afterwards (impls nest inside modules, and a closure body inside a
    /// member is still inside the member).
    fn within_member_body<T>(&mut self, body: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let outer = std::mem::replace(&mut self.in_member_body, true);
        let result = body(self);
        self.in_member_body = outer;
        result
    }

    /// `trait name generics? (with A + B)? { functions }`. The body is a list of
    /// function declarations only.
    fn parse_trait(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut labels = self.parse_item_labels();
        // B470: `[resource] trait` — it closes the label prefix, as it does on
        // a struct or an enum.
        if self.eat_marker_attribute("resource") {
            labels.get_or_insert_with(Default::default).resource = true;
        }
        self.expect(&Token::Trait)?;
        let name_start = self.position;
        let name = self.eat_ident()?;
        let name = (name, self.span_from(name_start));
        let generic_parameters = self.parse_generic_parameters();
        let supertraits = if self.eat_word("with") {
            self.parse_type_bounds()?
        } else {
            Vec::new()
        };
        let body = self.within_member_body(Self::parse_trait_body)?;
        Some((
            Node::Trait(
                name,
                generic_parameters.map(Box::new),
                supertraits,
                Box::new(body),
                labels,
            ),
            self.span_from(start),
        ))
    }

    /// `mod name { statements }` — a nested module. `self` is the file's own
    /// module (B415's `mod self;`), so a nested module by that name is refused
    /// where it is written — and still parsed, so the reader gets the one
    /// sentence and not a cascade.
    fn parse_module(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Mod)?;
        let name_span = self.here_span();
        let name = self.eat_ident()?;
        if name == "self" {
            self.errors.push(ParseError {
                span: name_span,
                reason: ParseErrorReason::Rule(MODULE_SELF_IS_RESERVED),
                context: self.context_stack.clone(),
                hint: None,
            });
        }
        let body = self.parse_item_body("module body")?;
        Some((Node::Module(name, body), self.span_from(start)))
    }

    // --- import / use / export -----------------------------------------------

    /// `import <namespace_path> only?` (the node's span covers only
    /// `import <path> only`; the statement-level `;` is consumed separately).
    fn parse_import(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Import)?;
        // B320: one statement's record, never the previous statement's.
        self.import_path_failure = None;
        let path = self.parse_namespace_path()?;
        let modifier = self.parse_import_modifier();
        Some((Node::Import(path, modifier), self.span_from(start)))
    }

    /// The trailing `only` of an `import` statement (B318 §2.4), or
    /// [`ImportModifier::None`].
    ///
    /// `only` is CONTEXTUAL, exactly as `as` is
    /// ([`Parser::parse_namespace_single_path`]'s E142 note), and for the same
    /// reason: it is recognized only HERE, where the whole path has ended and a
    /// `;` is the only other thing that may follow. A module or an item called
    /// `only` is unaffected — it is reached as a path SEGMENT, which this rule
    /// never sees — and the word occurs as an identifier nowhere in vilan, kolt
    /// or the website (`visibility.md` §2.4: zero hits outside prose).
    ///
    /// It qualifies the STATEMENT, not a leaf, which is why it lives here and
    /// not in the path grammar: `only` subtracts the implementations the walk
    /// to the leaf brought, and that walk is the statement's.
    fn parse_import_modifier(&mut self) -> ImportModifier {
        if self.peek() != Some(&Token::Ident("only")) {
            return ImportModifier::None;
        }
        let span = self.here_span();
        self.bump();
        ImportModifier::Only(span)
    }

    /// `import <namespace_path> only? ;` — an import used as a statement.
    fn parse_import_statement(&mut self) -> Option<Spanned<Node<'src>>> {
        // B382: a steer on an import that is not re-exported publishes nothing
        // — refused where it is written, and the import still parses, so the
        // reader gets the one sentence and not a cascade.
        if let Some(attribute) = self.attempt(|parser| {
            let start = parser.position;
            parser.parse_deprecated_attribute()?;
            (parser.peek() == Some(&Token::Import)).then(|| parser.span_from(start))
        }) {
            self.errors.push(ParseError {
                span: attribute,
                reason: ParseErrorReason::Rule(DEPRECATED_IMPORT_IS_A_RE_EXPORT),
                context: self.context_stack.clone(),
                hint: None,
            });
        }
        let import = self.parse_import()?;
        if !self.eat_ctrl(';') {
            self.note_terminator();
            return None;
        }
        Some(import)
    }

    /// `use <namespace_path>` (the statement-level `;` is consumed separately).
    fn parse_use(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Use)?;
        // B320: one statement's record, never the previous statement's.
        self.import_path_failure = None;
        let path = self.parse_namespace_path()?;
        Some((Node::Use(path), self.span_from(start)))
    }

    /// `use <namespace_path> ;` — a use used as a statement. A trailing `only`
    /// is refused where it is written ([`USE_TAKES_NO_ONLY`]) and then eaten,
    /// so the statement still parses and the reader gets one message rather
    /// than a cascade at the `;`.
    fn parse_use_statement(&mut self) -> Option<Spanned<Node<'src>>> {
        let use_ = self.parse_use()?;
        if let Node::Use(branch) = &use_.0 {
            let mut selectors = Vec::new();
            collect_branch_selectors(branch, &mut selectors);
            for span in selectors {
                self.errors.push(ParseError {
                    span,
                    reason: ParseErrorReason::Rule(USE_TAKES_NO_IMPL_SELECTOR),
                    context: Vec::new(),
                    hint: None,
                });
            }
        }
        if self.peek() == Some(&Token::Ident("only")) {
            self.errors.push(ParseError {
                span: self.here_span(),
                reason: ParseErrorReason::Rule(USE_TAKES_NO_ONLY),
                context: Vec::new(),
                hint: None,
            });
            self.bump();
        }
        if !self.eat_ctrl(';') {
            self.note_terminator();
            return None;
        }
        Some(use_)
    }

    /// `export <statement>` — re-export an import or expose a declaration. The
    /// inner statement consumes its own terminator (so `export import a::b;`'s
    /// `Export` span includes the `;`, while the inner `Import` span does not).
    fn parse_export(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Export)?;
        // B318 §2.1: `export *;` is the module-wide marker. The lookahead takes
        // the `;` as well as the `*`, because a following NAME is a real
        // expression — `export * helper;` is `export` of the deref `*helper`
        // (probe P1b) — and reading that as a mistyped `export *;` would take
        // a shape the language already has. Nothing else in the language has
        // this one: `*` starts a prefix deref and finds no operand at the `;`.
        if self.peek_is_op("*") && matches!(self.peek_at(1), Some(Token::Ctrl(';'))) {
            self.bump();
            self.bump();
            return Some((Node::ExportAll, self.span_from(start)));
        }
        // B492: one marker per declaration — a repeated one is refused and
        // read past, so the tree is the one export it means. Either marker
        // may carry the scope (`export export(in pkg)`, `export(in pkg)
        // export`); the first one written is the declaration's.
        //
        // A repeat is a marker, a scope, or an attribute run that ends at
        // another `export` (`export [m] export fun`): the run is the item's,
        // so it is rotated behind the repeat exactly as at a statement's head,
        // and the repeat refused. So no `export` nests in another, and a
        // marker chain is not a nesting door at all.
        let mut scope = None;
        loop {
            self.refuse_repeated_export_markers();
            if scope.is_none() {
                scope = self.parse_export_scope();
                if scope.is_some() {
                    continue;
                }
            }
            if self.lead_export_past_its_attributes() {
                continue;
            }
            break;
        }
        // B382: `export [deprecated("use …")] import …;` — the steer is the
        // RE-EXPORT's, so the export carries it. Read only ahead of `import`:
        // before a declaration the same attribute is the declaration's own
        // prefix, which its production reads.
        let labels = self.attempt(|parser| {
            let steer = parser.parse_deprecated_attribute()?;
            (parser.peek() == Some(&Token::Import)).then(|| {
                Box::new(Labels {
                    deprecated: Some(steer),
                    ..Labels::default()
                })
            })
        });
        let inner = self.parse_statement()?;
        // B321: `parse_statement` reads an EXPRESSION statement too, so
        // `export (helper);` and `export * helper;` parsed and meant nothing.
        // Reported and KEPT — the inner statement is whatever the author wrote
        // and dropping it would unbind a name the rest of the file uses, which
        // is `recover_missing_terminator`'s argument applied to a wrapper.
        if !export_takes(&inner.0) {
            self.errors.push(ParseError {
                span: inner.1,
                reason: ParseErrorReason::Rule(EXPORT_TAKES_AN_ITEM),
                context: self.context_stack.clone(),
                hint: None,
            });
        }
        // B445: an attribute run written ahead of the marker was rotated
        // behind it ([`Parser::lead_export_past_its_attributes`]), so the
        // inner item begins EARLIER in the source than the marker does; the
        // statement begins where the author began it.
        let span = self.span_from(start);
        let span = Span::from(span.start.min(inner.1.start)..span.end);
        Some((Node::Export(scope, Box::new(inner), labels), span))
    }

    /// B492's refusal: a run of `export` markers at the cursor is reported
    /// ONCE, spanning the run, and read past.
    fn refuse_repeated_export_markers(&mut self) {
        let start = self.position;
        while self.peek_is(&Token::Export) {
            self.bump();
        }
        if self.position > start {
            self.errors.push(ParseError {
                span: self.span_from(start),
                reason: ParseErrorReason::Rule(EXPORT_IS_WRITTEN_ONCE),
                context: self.context_stack.clone(),
                hint: None,
            });
        }
    }

    /// `(in PATH)` after `export` — B318 §2.2's narrowing, `None` when the
    /// marker carries none.
    ///
    /// `in` is already [`Token::In`] (`for … in`), so the inner grammar needs no
    /// contextual-keyword dance; `mod` is [`Token::Mod`] and is admitted as a
    /// path segment by name, which is what makes `export(in mod)` spellable
    /// without reserving a second word. A `(` that is NOT followed by `in`
    /// declines here and falls to the statement reader, where [`export_takes`]
    /// refuses it and names this form — which is how `export(pkg)`, the spelling
    /// that reads as a CALL, gets a steer rather than a silent acceptance.
    fn parse_export_scope(&mut self) -> Option<Box<ExportScope<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('(')?;
            // §2.2: `export(pkg)` — the spelling P2c shows reads as a CALL — is
            // the scope form with `in` left out. Taken and REPORTED rather than
            // declined, so the author gets the steer instead of the missing-`;`
            // three tokens later that the fall-through produced. The `;`
            // lookahead is what keeps it off `export (helper);`, which is an
            // expression STATEMENT and B321's case: a scope is followed by the
            // item it narrows, never by a terminator.
            let missing_in = !parser.eat(&Token::In);
            if missing_in && parser.terminates_an_export_group() {
                return None;
            }
            let mut path = Vec::new();
            loop {
                let at = parser.position;
                let segment = if parser.eat(&Token::Mod) {
                    "mod"
                } else {
                    parser.eat_name()?
                };
                path.push((segment, parser.span_from(at)));
                if !parser.eat_op("::") {
                    break;
                }
            }
            parser.expect_ctrl(')')?;
            let span = parser.span_from(start);
            if missing_in {
                parser.errors.push(ParseError {
                    span,
                    reason: ParseErrorReason::Rule(EXPORT_TAKES_AN_ITEM),
                    context: parser.context_stack.clone(),
                    hint: None,
                });
            }
            Some(Box::new(ExportScope { path, span }))
        })
    }

    /// Whether the parenthesised group opening at the current position is
    /// closed by a `;` — an expression STATEMENT after `export`, not a
    /// visibility scope. Scans forward at paren depth, stopping at anything a
    /// balanced group cannot contain.
    fn terminates_an_export_group(&self) -> bool {
        let mut depth = 1usize;
        let mut at = self.position;
        while let Some((token, _)) = self.tokens.get(at) {
            match token {
                Token::Ctrl('(') => depth += 1,
                Token::Ctrl(')') => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(self.tokens.get(at + 1), Some((Token::Ctrl(';'), _)));
                    }
                }
                Token::Ctrl('{') | Token::Ctrl('}') | Token::Ctrl(';') => return false,
                _ => {}
            }
            at += 1;
        }
        false
    }

    /// A `::`-separated namespace path ending in a name or a `{ a, b }` set (H2) —
    /// the recursive `import`/`use` path grammar. A single path (a name with an
    /// optional `:: continuation`) is tried before a brace set, matching the chumsky
    /// `path.clone().or(set)`.
    fn parse_namespace_path(&mut self) -> Option<ImportBranch<'src>> {
        // `use a::a::a::..;` and `use a::{a::{..}};` recurse here once per `::`
        // or brace, reaching no expression rule (B142). An empty set is the
        // stand-in: a branch that imports nothing, which is what a path the
        // parser declined to read leads to.
        self.parse_nested_as(
            Self::IMPORT_NESTING_REFUSAL,
            |_, _| Some(ImportBranch::Set(Vec::new())),
            Self::parse_namespace_path_inner,
        )
    }

    /// [`Parser::parse_namespace_path`]'s body, past the depth bound.
    fn parse_namespace_path_inner(&mut self) -> Option<ImportBranch<'src>> {
        if let Some(path) = self.attempt(Self::parse_namespace_single_path) {
            return Some(path);
        }
        // A selector OUTSIDE a set: refused where it is written rather than at
        // column one, and then parsed anyway so the rest of the statement is
        // still read (`visibility.md` §2.5). Marked or not — `#(impl T)` is the
        // same element and earns the same rule.
        if self.at_impl_selector() || self.at_reach_marked_impl_selector() {
            let span = self.here_span();
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Rule(IMPL_SELECTOR_IS_A_BRACE_ELEMENT),
                context: Vec::new(),
                hint: None,
            });
            return self.parse_namespace_set_element();
        }
        let branch = self.parse_namespace_set();
        if branch.is_none() {
            // B320: neither alternative reads what stands here, so the path
            // genuinely stops at this token. A DECLINE of this production always
            // fails the whole statement (its caller propagates with `?`), which
            // is what keeps the record free of positions a sibling then reads.
            self.note_import_failure(self.position);
        }
        branch
    }

    /// Whether the cursor sits on `( impl` — the selector's two-token gate
    /// (B318 S3). A `(` that is not followed by `impl` is not a selector and is
    /// left to whatever else may read it.
    fn at_impl_selector(&self) -> bool {
        self.peek_is_ctrl('(') && self.peek_at(1) == Some(&Token::Impl)
    }

    /// Whether the cursor sits on `# ( impl` — [`Parser::at_impl_selector`]'s
    /// reach-marked form (B318 S4), which
    /// [`Parser::parse_namespace_single_path`] reads and which the
    /// outside-a-set rule has to recognize for itself.
    fn at_reach_marked_impl_selector(&self) -> bool {
        self.peek_is(&Token::Hash)
            && self.peek_at(1) == Some(&Token::Ctrl('('))
            && self.peek_at(2) == Some(&Token::Impl)
    }

    /// `"(" "impl" type ")" ( "::" ( NAME | "{" NAME,* "}" ) )?` — an impl
    /// SELECTOR (B318 S3, `proposal/visibility.md` §2.5).
    ///
    /// COMMITTED at the gate: once `( impl` has been seen there is no other
    /// reading of the tokens, so a failure past it reports at the token the
    /// selector stopped on with [`IMPL_SELECTOR_SHAPE`] and the branch is still
    /// produced (empty of members). Returning `None` would fail the whole
    /// statement and the recovery would report `found 'import' expected an
    /// expression` at column one — P5/P5b/P5c/P10b's shared failure, the one
    /// §2.5 says this slice must remove before it adds the grammar.
    ///
    /// The subject is the ordinary type grammar, which already admits `_` at
    /// any position as B294's anonymous binder — the selector's placeholder,
    /// with no lexer or grammar rule of its own. What the selector refuses is a
    /// NAMED binder and a bound ([`IMPL_SELECTOR_TAKES_NO_BINDER`]).
    fn parse_impl_selector(&mut self) -> Option<ImportBranch<'src>> {
        let start = self.position;
        self.expect_ctrl('(')?;
        self.expect(&Token::Impl)?;
        let subject = match self.parse_type() {
            Some(subject) => subject,
            None => return Some(self.selector_refusal(start, None, Vec::new())),
        };
        self.refuse_selector_binders(&subject);
        let subject_text = self.text_of(subject.1);
        // B455 (RULED 2026-10-01): `with TRAIT` names the block by the trait it
        // implements, the declaration's own spelling.
        let trait_ = match self.eat_word("with") {
            true => match self.parse_type() {
                Some(trait_) => {
                    self.refuse_selector_binders(&trait_);
                    Some(trait_)
                }
                None => {
                    return Some(self.selector_refusal(
                        start,
                        Some((subject, subject_text)),
                        Vec::new(),
                    ));
                }
            },
            false => None,
        };
        let trait_text = trait_
            .as_ref()
            .map(|trait_| Cow::Borrowed(self.text_of(trait_.1)));
        if !self.eat_ctrl(')') {
            return Some(self.selector_refusal(start, Some((subject, subject_text)), Vec::new()));
        }
        let mut members = Vec::new();
        if self.eat_op("::") {
            if self.eat_ctrl('{') {
                let names =
                    self.comma_list(Self::eat_selector_member, |parser| parser.peek_is_ctrl('}'));
                match names {
                    Some(names) => members = names,
                    None => {
                        return Some(self.selector_refusal(
                            start,
                            Some((subject, subject_text)),
                            Vec::new(),
                        ));
                    }
                }
                if !self.eat_ctrl('}') {
                    return Some(self.selector_refusal(
                        start,
                        Some((subject, subject_text)),
                        members,
                    ));
                }
            } else {
                match self.eat_selector_member() {
                    Some(member) => members.push(member),
                    None => {
                        return Some(self.selector_refusal(
                            start,
                            Some((subject, subject_text)),
                            Vec::new(),
                        ));
                    }
                }
            }
        }
        self.refuse_selector_alias();
        Some(ImportBranch::Selector(Box::new(ImplSelector {
            subject: Some(Box::new(subject)),
            subject_text: Cow::Borrowed(subject_text),
            trait_: trait_.map(Box::new),
            trait_text,
            members,
            span: self.span_from(start),
        })))
    }

    /// One member name inside a selector's `::name` / `::{ a, b }` tail, with
    /// its span. `as` on one is refused where it is written
    /// ([`IMPL_SELECTOR_REFUSES_AS`]) and eaten, so the set still reads.
    ///
    /// A selector names METHODS, so the name is a member name (B414 S4): a
    /// method declared `fun type(self)` is selected as `(impl T)::type`.
    fn eat_selector_member(&mut self) -> Option<(&'src str, Span)> {
        let start = self.position;
        let name = self.eat_member_name()?;
        let span = self.span_from(start);
        self.refuse_selector_alias();
        Some((name, span))
    }

    /// Refuses a contextual `as <name>` at the cursor and consumes it (B318:
    /// a selector takes no alias, on the block or on a member).
    fn refuse_selector_alias(&mut self) {
        if self.peek() != Some(&Token::Ident("as"))
            || !matches!(self.peek_at(1), Some(Token::Ident(_)))
        {
            return;
        }
        let start = self.position;
        self.bump();
        self.bump();
        self.errors.push(ParseError {
            span: self.span_from(start),
            reason: ParseErrorReason::Rule(IMPL_SELECTOR_REFUSES_AS),
            context: Vec::new(),
            hint: None,
        });
    }

    /// Refuses every NAMED binder and every BOUND binder in a selector's
    /// subject (B318: no binders are written in a selector). A bare `_` is the
    /// placeholder and is the one binder node that survives.
    fn refuse_selector_binders(&mut self, subject: &Spanned<Node<'src>>) {
        let mut refused = Vec::new();
        collect_selector_binders(subject, &mut refused);
        for span in refused {
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Rule(IMPL_SELECTOR_TAKES_NO_BINDER),
                context: Vec::new(),
                hint: None,
            });
        }
    }

    /// The shape refusal, reported at the token the selector stopped on, plus
    /// the branch the caller returns in its place.
    fn selector_refusal(
        &mut self,
        start: usize,
        subject: Option<(Spanned<Node<'src>>, &'src str)>,
        members: Vec<(&'src str, Span)>,
    ) -> ImportBranch<'src> {
        self.errors.push(ParseError {
            span: self.here_span(),
            reason: ParseErrorReason::Rule(IMPL_SELECTOR_SHAPE),
            context: Vec::new(),
            hint: None,
        });
        let (subject, subject_text) = match subject {
            Some((subject, text)) => (Some(Box::new(subject)), text),
            None => (None, ""),
        };
        ImportBranch::Selector(Box::new(ImplSelector {
            subject,
            subject_text: Cow::Borrowed(subject_text),
            trait_: None,
            trait_text: None,
            members,
            span: self.span_from(start),
        }))
    }

    /// The source text a span covers — the selector's subject, reprinted
    /// verbatim by the formatter and keyed on by the sorter.
    fn text_of(&self, span: Span) -> &'src str {
        self.source.get(span.into_range()).unwrap_or("")
    }

    /// `name ( :: branch | as name )?` — one path in a namespace path (the
    /// chumsky `path`). The name's `ImportBranch::Path` span is the name token
    /// only; a `::` continuation is a full [`Parser::parse_namespace_path`] (a
    /// further path or a set).
    ///
    /// `as` is CONTEXTUAL, not a keyword: it is recognized only here, where a
    /// path segment has ended and the next two tokens are `as <name>`, so a
    /// module or an item actually called `as` is unaffected and no existing
    /// program's identifier is taken (E142). The alias renames the LEAF, so it
    /// is an alternative to the `::` continuation rather than something that
    /// can follow one — `a::b as c::d` does not parse, and the tail type says
    /// so. It reaches a brace element by the same production: `{ a as b, c }`.
    fn parse_namespace_single_path(&mut self) -> Option<ImportBranch<'src>> {
        // B318 §1/§2.3: `#` before a segment is the REACH marker — "I know this
        // is not exported and I want it anyway". It wraps whatever follows
        // rather than becoming part of it, so it composes with a leaf
        // (`{ #hidden }`), with a segment mid-path (`a::#m::helper`) and with
        // S3's selector, and it adds no path segment — which is what keeps
        // go-to-definition, find-references and RENAME pointing at the name.
        if self.peek_is(&Token::Hash) {
            let marker = self.here_span();
            self.bump();
            // B318 S4: `#(impl T)` — the marker on a SELECTOR, which is the one
            // and only reach to an `impl` block its module does not `export`
            // (RULED 2026-09-13: an invisible impl contributes no methods and
            // no ambient impls; `#` is the way in). The selector is not a path,
            // so the recursion below cannot read it: this is the seam, at
            // `at_impl_selector`, and the wrapper composes exactly as it does
            // over a name.
            let inner = if self.at_impl_selector() {
                self.parse_impl_selector()?
            } else {
                self.parse_namespace_single_path()?
            };
            return Some(ImportBranch::Reach(marker, Box::new(inner)));
        }
        let start = self.position;
        let name = self.eat_name()?;
        let name_span = self.span_from(start);
        if self.eat_op("::") {
            self.refuse_a_path_crossing_a_line()?;
            let continuation = Box::new(self.parse_namespace_path()?);
            return Some(ImportBranch::Path(
                name,
                name_span,
                ImportTail::Continue(continuation),
            ));
        }
        if self.peek() == Some(&Token::Ident("as"))
            && matches!(self.peek_at(1), Some(Token::Ident(_)))
        {
            self.bump();
            let alias_start = self.position;
            let alias = self.eat_name().expect("peeked as an identifier");
            return Some(ImportBranch::Path(
                name,
                name_span,
                ImportTail::Alias(alias, self.span_from(alias_start)),
            ));
        }
        Some(ImportBranch::Path(name, name_span, ImportTail::Leaf))
    }

    /// `{ path | selector, ... }` — a brace-delimited set (allow-trailing).
    /// Each element is a single path (chumsky's `path`, which must start with a
    /// name) or, since B318 S3, an `(impl TYPE)` SELECTOR — tried first,
    /// because its `(` begins nothing else here. A nested bare set is still not
    /// a legal element.
    fn parse_namespace_set(&mut self) -> Option<ImportBranch<'src>> {
        self.attempt(|parser| {
            parser.expect_ctrl('{')?;
            let paths = parser.comma_list(
                |parser| {
                    // B320: an element that is neither a path nor a selector stops the set HERE,
                    // and here is deeper than the `{` the enclosing production
                    // would otherwise record — `{ (impl T) }` reports on the
                    // `(`, which is the token that was typed.
                    let at = parser.position;
                    let element = parser.parse_namespace_set_element();
                    if element.is_none() {
                        parser.note_import_failure(at);
                    }
                    element
                },
                |parser| parser.peek_is_ctrl('}'),
            )?;
            parser.expect_ctrl('}')?;
            Some(ImportBranch::Set(paths))
        })
    }

    /// One element of a brace set: an `(impl …)` selector, or a single path.
    fn parse_namespace_set_element(&mut self) -> Option<ImportBranch<'src>> {
        if self.at_impl_selector() {
            return self.parse_impl_selector();
        }
        // `#(impl T)` falls through: the path production owns the marker and
        // routes back to the selector past it (B318 S4).
        self.parse_namespace_single_path()
    }

    // --- Derive / service / macro-attribute items ----------------------------

    /// `[derive(A, B)] (struct | enum)` — a derive attribute wrapping a struct/enum.
    fn parse_derived_item(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let derives = self.parse_derive_attribute()?;
        let item = match self.attempt(Self::parse_struct) {
            Some(item) => item,
            None => self.attempt(Self::parse_enum)?,
        };
        Some((Node::Derive(derives, Box::new(item)), self.span_from(start)))
    }

    /// `[derive(A, B)]` — the derive trait names (with spans), allow-trailing; or
    /// `None` when no derive attribute leads.
    fn parse_derive_attribute(&mut self) -> Option<Vec<(&'src str, Span)>> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("derive")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl('(')?;
            let names =
                parser.comma_list(Self::parse_spanned_ident, |parser| parser.peek_is_ctrl(')'))?;
            parser.expect_ctrl(')')?;
            parser.expect_ctrl(']')?;
            Some(names)
        })
    }

    /// An identifier with its own span, for derive names.
    fn parse_spanned_ident(&mut self) -> Option<(&'src str, Span)> {
        let start = self.position;
        let name = self.eat_ident()?;
        Some((name, self.span_from(start)))
    }

    /// `[service(Client)?]` / `[client_service]` (either order, or both) `struct …`
    /// — a service struct. `[service]`'s first argument names the generated client
    /// type (default `<Struct>Client`); its `client = H` argument names the
    /// `[client_service]` struct this server may call back into (§9.3, R1).
    fn parse_service_item(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let mut attribute = ServiceAttr::default();
        // Either attribute may lead, and a peer-to-peer struct writes both.
        loop {
            if let Some((client_name, handler_name, http)) = self.parse_service_attribute() {
                if attribute.server_side {
                    return None;
                }
                attribute.server_side = true;
                attribute.client_name = client_name;
                attribute.handler_name = handler_name;
                attribute.http = http;
            } else if self.parse_client_service_attribute().is_some() {
                if attribute.client_side {
                    return None;
                }
                attribute.client_side = true;
            } else {
                break;
            }
        }
        if !attribute.server_side && !attribute.client_side {
            return None;
        }
        let item = self.parse_struct()?;
        Some((
            Node::Service(attribute, Box::new(item)),
            self.span_from(start),
        ))
    }

    /// `[service(Name?, http?, client = Handler?)?]` — a service attribute. The
    /// outer `Option` is whether this is a service attribute at all (`None` ⇒
    /// not one); the triple is the optional client name, the optional `client =
    /// H` handler name, and whether the `http` marker was written (A120 S5).
    #[allow(clippy::type_complexity)]
    fn parse_service_attribute(&mut self) -> Option<(Option<&'src str>, Option<&'src str>, bool)> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("service")) {
                return None;
            }
            parser.bump();
            let arguments = parser.attempt(|parser| {
                parser.expect_ctrl('(')?;
                let mut client_name = None;
                let mut handler_name = None;
                let mut http = false;
                while !parser.peek_is_ctrl(')') {
                    // `http` — the marker (A120 S5). A bare word in any
                    // position, and never a client NAME: a client type spelled
                    // `http` would be a lowercase type, which nothing in the
                    // language writes, and reading the word as the marker is
                    // what lets `[service(http)]` keep the default client name.
                    if parser.peek() == Some(&Token::Ident("http")) {
                        if http {
                            return None;
                        }
                        parser.bump();
                        http = true;
                        if !parser.eat_ctrl(',') {
                            break;
                        }
                        continue;
                    }
                    // `client = Handler` — the one named argument; anything else
                    // is the positional client name, which leads or not at all.
                    let named = parser.attempt(|parser| {
                        if parser.peek() != Some(&Token::Ident("client")) {
                            return None;
                        }
                        parser.bump();
                        if !parser.eat_op("=") {
                            return None;
                        }
                        parser.eat_ident()
                    });
                    match named {
                        Some(name) => handler_name = Some(name),
                        None => {
                            if client_name.is_some() || handler_name.is_some() || http {
                                return None;
                            }
                            client_name = Some(parser.eat_ident()?);
                        }
                    }
                    if !parser.eat_ctrl(',') {
                        break;
                    }
                }
                parser.expect_ctrl(')')?;
                Some((client_name, handler_name, http))
            });
            parser.expect_ctrl(']')?;
            Some(arguments.unwrap_or((None, None, false)))
        })
    }

    /// `[client_service]` — the handler-side attribute (§9.3, R1). No arguments:
    /// the proxy the server calls through is always `<Struct>Proxy`.
    fn parse_client_service_attribute(&mut self) -> Option<()> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("client_service")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl(']')?;
            Some(())
        })
    }

    /// `[<user-name>(args)?] (struct | enum | fun)` — a user macro attribute. The
    /// name must NOT be a known built-in marker (they keep their own parsers); the
    /// `(args)?` are OPTIONAL and captured as argument SPANS (source text is what the
    /// macro receives).
    fn parse_macro_attributed_item(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect_ctrl('[')?;
        let name_start = self.position;
        let name = match self.peek() {
            Some(Token::Ident(name)) if !is_known_attribute_marker(name) => *name,
            _ => return None,
        };
        self.bump();
        let name_span = self.span_from(name_start);
        let arguments = self
            .attempt(|parser| {
                parser.expect_ctrl('(')?;
                let arguments = parser
                    .comma_list(Self::parse_argument_span, |parser| parser.peek_is_ctrl(')'))?;
                parser.expect_ctrl(')')?;
                Some(arguments)
            })
            .unwrap_or_default();
        self.expect_ctrl(']')?;
        let item = match self.attempt(Self::parse_struct) {
            Some(item) => item,
            None => match self.attempt(Self::parse_enum) {
                Some(item) => item,
                None => self.attempt(Self::parse_function)?,
            },
        };
        Some((
            Node::MacroAttribute(name, name_span, arguments, Box::new(item)),
            self.span_from(start),
        ))
    }

    /// The SPAN of one macro-argument expression (its source text is what the macro
    /// receives — arguments are syntax). Parses a full expression, keeps its span.
    fn parse_argument_span(&mut self) -> Option<Span> {
        self.parse_expression().map(|(_, span)| span)
    }

    // --- Bracket attribute helpers -------------------------------------------

    /// `[ marker ]` — a bare marker attribute (`[must_use]`, `[rpc]`, `[trait_only]`,
    /// `[expose]`). Consumes it and returns `true` when the exact `[ marker ]` is
    /// next; leaves the cursor untouched and returns `false` otherwise.
    /// `[expose]`, `[expose(keyed)]` or `[expose(keyed = K)]` — a struct
    /// field's exposure, or [`Exposure::None`] when no expose attribute leads.
    ///
    /// The argument form is parsed rather than matched as a marker so that an
    /// unrecognized one is REFUSED by name instead of silently reading as a
    /// plain `[expose]` (the shapes differ on the wire, so a typo would ship a
    /// whole-value channel where the author asked for a keyed one). The
    /// attribute is still consumed on that path, so the field itself parses and
    /// the file keeps going.
    ///
    /// `= K` (tracker A51) names the KEY TYPE, and it is the one attribute
    /// argument in the language that is a type rather than a word: a keyed
    /// mirror is a `KeyedSource<K, T>`, the `[service]` expansion reads both
    /// types off the annotation because vilan has no associated types, and a
    /// `List<T>` names only the element. It is parsed with the ordinary type
    /// grammar and kept as the SOURCE TEXT it spans — the macro engine takes it
    /// as a string and the formatter reprints it, and nothing here resolves it.
    /// Its SPAN rides with the text (tracker A56): the analyzer's refusal of an
    /// argument that disagrees with a `HashMap<K, V>` element's own key is about
    /// the ARGUMENT, and has to point at it.
    fn eat_expose_attribute(&mut self) -> Exposure<'src> {
        let form = self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("expose")) {
                return None;
            }
            parser.bump();
            let form = if parser.eat_ctrl('(') {
                let name_start = parser.position;
                let name = parser.eat_ident()?;
                let name_span = parser.span_from(name_start);
                // `= K`: the key type, as written. A malformed one is refused
                // where it stands rather than backtracking the whole attribute,
                // which would report it as "expected a field name".
                let key = if parser.eat_op("=") {
                    let key_start = parser.position;
                    match parser.parse_type() {
                        Some((_node, span)) => {
                            Some(Ok((&parser.source[span.start..span.end], span)))
                        }
                        None => {
                            // Skip to the closing paren so the attribute still
                            // CONSUMES, and the refusal below is what the
                            // author reads. Without this the `attempt` fails at
                            // `expect_ctrl(')')`, rolls the whole attribute
                            // back — errors included — and the field is
                            // reported as a missing name.
                            while parser.peek().is_some() && !parser.peek_is_ctrl(')') {
                                parser.bump();
                            }
                            Some(Err(parser.span_from(key_start)))
                        }
                    }
                } else {
                    None
                };
                parser.expect_ctrl(')')?;
                Some((name, name_span, key))
            } else {
                None
            };
            parser.expect_ctrl(']')?;
            Some(form)
        });
        match form {
            None => Exposure::None,
            Some(None) => Exposure::Whole,
            Some(Some(("keyed", _span, key))) => match key {
                None => Exposure::Keyed(None),
                Some(Ok(written)) => Exposure::Keyed(Some(written)),
                Some(Err(span)) => {
                    self.errors.push(ParseError {
                        span,
                        reason: ParseErrorReason::Rule(
                            "`[expose(keyed = …)]`'s argument is a TYPE — the key type the \
                             mirror is keyed by, as in `[expose(keyed = str)]`. It is written \
                             here because a keyed mirror is a `KeyedSource<K, T>` and a \
                             `List<T>` names only the element; a `HashMap<K, V>` element names both, \
                             and takes the bare `[expose(keyed)]`",
                        ),
                        context: Vec::new(),
                        hint: None,
                    });
                    Exposure::Keyed(None)
                }
            },
            Some(Some((_other, span, _key))) => {
                self.errors.push(ParseError {
                    span,
                    reason: ParseErrorReason::Rule(
                        "the only argument `[expose]` takes is `keyed` — write `[expose]` for a \
                         whole-value channel, `[expose(keyed)]` for a keyed `HashMap<K, V>`, and \
                         `[expose(keyed = K)]` for any other keyed collection",
                    ),
                    context: Vec::new(),
                    hint: None,
                });
                Exposure::Whole
            }
        }
    }

    fn eat_marker_attribute(&mut self, marker: &str) -> bool {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident(marker)) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl(']')?;
            Some(())
        })
        .is_some()
    }

    /// `[extern(args)]` — the host binding for the `external` function that follows,
    /// or `None` when no extern attribute leads. Args are bare words or quoted
    /// strings, interpreted by [`extern_binding_from_args`] (a malformed attribute
    /// lowers to an empty global symbol, exactly as the oracle does).
    fn parse_extern_attribute(&mut self) -> Option<(ExternBinding<'src>, bool)> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("extern")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl('(')?;
            let args =
                parser.comma_list(Self::parse_extern_arg, |parser| parser.peek_is_ctrl(')'))?;
            parser.expect_ctrl(')')?;
            parser.expect_ctrl(']')?;
            // `retains` is a FLAG, not a form: stripped before the positional
            // match so it composes with every binding shape
            // (`[extern(method, "addEventListener", retains)]`) instead of
            // needing an arm per combination. Recognized in TRAILING position
            // only — the one place a flag can sit without displacing a form
            // word, and what lets the formatter reprint it exactly where it was
            // written. Anywhere else it is an unknown argument, and the
            // attribute lowers to the empty global symbol exactly as every
            // other malformed extern attribute does.
            let mut binding = args;
            let retains = matches!(binding.last(), Some(ExternArg::Word("retains")));
            if retains {
                binding.pop();
            }
            Some((extern_binding_from_args(&binding), retains))
        })
    }

    /// One `[extern(..)]` argument: a bare word (`method`/`get`/`set`/`new`) or a
    /// quoted string (a module path or host symbol). Word is tried before Text,
    /// matching the chumsky choice.
    fn parse_extern_arg(&mut self) -> Option<ExternArg<'src>> {
        match self.peek() {
            Some(Token::Ident(word)) => {
                let word = *word;
                self.bump();
                Some(ExternArg::Word(word))
            }
            Some(Token::String(text)) => {
                let text = *text;
                self.bump();
                Some(ExternArg::Text(text))
            }
            _ => None,
        }
    }

    /// `[doc(hidden)]` — RETIRED (B318 §7.5). Recognized and REFUSED, rather
    /// than simply deleted from the grammar: the attribute is in the wild (the
    /// book recommended it), and "found `[` expected `fun`" would tell its
    /// author nothing. Consumed, so the item below it parses normally and the
    /// file raises one diagnostic rather than cascading.
    fn refuse_doc_hidden_attribute(&mut self) {
        let start = self.position;
        let refused = self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("doc")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl('(')?;
            if parser.peek() != Some(&Token::Ident("hidden")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl(')')?;
            parser.expect_ctrl(']')?;
            Some(())
        });
        if refused.is_some() {
            self.errors.push(ParseError {
                span: (self.token_span(start).start..self.token_span(self.position - 1).end).into(),
                reason: ParseErrorReason::Rule(DOC_HIDDEN_IS_SUPERSEDED),
                context: self.context_stack.clone(),
                hint: None,
            });
        }
    }

    /// `[platform("a", "b")]` — a platform fence (≥1 string patterns, allow-trailing),
    /// or `None` when no platform attribute leads.
    fn parse_platform_attribute(&mut self) -> Option<Vec<Spanned<&'src str>>> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("platform")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl('(')?;
            let patterns = parser.comma_list(Self::parse_platform_pattern, |parser| {
                parser.peek_is_ctrl(')')
            })?;
            if patterns.is_empty() {
                return None;
            }
            parser.expect_ctrl(')')?;
            parser.expect_ctrl(']')?;
            Some(patterns)
        })
    }

    /// `[deprecated("use …")]` — the deprecation steer for the function that
    /// follows (proposal/deprecation.md §2): exactly one quoted string, the
    /// replacement clause the use-site warning carries verbatim after
    /// `` `{name}` is deprecated; ``. `None` when no deprecated attribute
    /// leads.
    fn parse_deprecated_attribute(&mut self) -> Option<&'src str> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("deprecated")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl('(')?;
            let Some(Token::String(steer)) = parser.peek() else {
                return None;
            };
            let steer = *steer;
            parser.bump();
            parser.expect_ctrl(')')?;
            parser.expect_ctrl(']')?;
            Some(steer)
        })
    }

    /// `[internal("reason")]` — E213's label for an item that is reachable on
    /// purpose and dangerous on purpose. Exactly one quoted string, REQUIRED:
    /// the reason is what hover leads with and what completion shows, and a
    /// label with no reason is how these rot. `None` when no internal
    /// attribute leads.
    ///
    /// Shaped on [`Self::parse_deprecated_attribute`], and placed beside it in
    /// the ordered prefix for the same reason: both are labels ABOUT the
    /// declaration rather than parts of its signature.
    fn parse_internal_attribute(&mut self) -> Option<&'src str> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("internal")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl('(')?;
            let Some(Token::String(reason)) = parser.peek() else {
                return None;
            };
            let reason = *reason;
            parser.bump();
            parser.expect_ctrl(')')?;
            parser.expect_ctrl(']')?;
            Some(reason)
        })
    }

    /// The labels an item declaration carries about itself (E221) — the
    /// ordered prefix `[internal("reason")]?`, read ahead of a struct, an
    /// enum, a trait or a module `let`. `None` when no label leads, which is
    /// the one-null-pointer case nearly every declaration takes.
    fn parse_item_labels(&mut self) -> ItemLabels<'src> {
        // B382: `[deprecated("use …")]` leads, as it leads a function's prefix;
        // F27 R1's `[platform("…")]` follows `[internal(..)]`, the order a
        // function's prefix gives the three.
        let deprecated = self.parse_deprecated_attribute();
        let internal = self.parse_internal_attribute();
        // E227: `[hint(..)]` is a label ABOUT the declaration, so it sits with
        // the two above and ahead of `[platform]`, which is about analysis.
        let mut hint = Vec::new();
        while let Some(written) = self.parse_hint_attribute() {
            hint.push(crate::node::HintArgument(std::sync::Arc::new(written)));
        }
        let platform = self.parse_platform_attribute().unwrap_or_default();
        if deprecated.is_none() && internal.is_none() && hint.is_empty() && platform.is_empty() {
            return None;
        }
        Some(Box::new(Labels {
            deprecated,
            internal,
            platform,
            resource: false,
            hint,
        }))
    }

    /// `[hint(Trait<..>)]` (E227) — the trait application an inlay hint shows
    /// the declaration as, parsed by the ordinary `parse_type`: it names the
    /// declaration's generic parameters before they are declared, which is
    /// resolution's business, not the parser's. `None` when no hint leads.
    fn parse_hint_attribute(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            parser.expect_ctrl('[')?;
            if parser.peek() != Some(&Token::Ident("hint")) {
                return None;
            }
            parser.bump();
            parser.expect_ctrl('(')?;
            let written = parser.parse_type()?;
            parser.expect_ctrl(')')?;
            parser.expect_ctrl(']')?;
            Some(written)
        })
    }

    /// `[platform("…", …)]? mod self;` — the host of the FILE's own
    /// attributes (B415): today the file's platform (F27 R1), which R1 first
    /// shipped as the bare `[platform(..)];`. `self` is the file's own module,
    /// and a `mod` with no body is only ever this one. A host with no attribute
    /// declares nothing (empty patterns). Where it may stand is
    /// [`Parser::parse_statement_inner`]'s rule, not this production's; a
    /// `mod self` WITH a body is [`Parser::parse_module`]'s refusal.
    fn parse_module_self(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let patterns = self.parse_platform_attribute().unwrap_or_default();
        self.expect(&Token::Mod)?;
        if self.peek() != Some(&Token::Ident("self")) {
            return None;
        }
        self.bump();
        let span = self.span_from(start);
        self.expect_ctrl(';')?;
        Some((Node::ModulePlatform(patterns), span))
    }

    /// A LABELLED `let` statement (E221): `[internal("reason")] let name = …;`.
    ///
    /// Read at statement position ahead of the expression fork, because `[`
    /// begins a list literal there: without this, `[internal("x")]` parses as
    /// a one-element list and the `let` after it as a missing `;`. A label is
    /// required — an unlabelled `let` is the expression fork's, unchanged — and
    /// only a plain binding takes one (a destructuring `let` names several
    /// things and none of them is an item). Whether the binding is a MODULE
    /// binding is not the parser's to know (a module and a function body share
    /// this production); `labels::check` refuses a labelled local.
    fn parse_labelled_let(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        let labels = self.parse_item_labels()?;
        let (node, _) = self.parse_let()?;
        // The statement's span ends where an unlabelled `let`'s does, before
        // its `;` — it only STARTS earlier, at the label.
        let span = self.span_from(start);
        self.expect_ctrl(';')?;
        let Node::Let(name, type_, value, mutable, lazy, None) = node else {
            return None;
        };
        Some((
            Node::Let(name, type_, value, mutable, lazy, Some(labels)),
            span,
        ))
    }

    /// One platform pattern: a quoted string with its span.
    fn parse_platform_pattern(&mut self) -> Option<Spanned<&'src str>> {
        let start = self.position;
        if let Some(Token::String(text)) = self.peek() {
            let text = *text;
            self.bump();
            Some((text, self.span_from(start)))
        } else {
            None
        }
    }

    // --- Macro forms ---------------------------------------------------------

    /// `macro fun name(..) { .. }` — a macro definition. The `function` production
    /// is reused and its `Node::Func` re-wrapped as `Node::MacroFun`.
    fn parse_macro_fun(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        self.expect(&Token::Macro)?;
        let (node, _) = self.parse_function()?;
        let node = match node {
            Node::Func(function) => Node::MacroFun(function),
            other => other,
        };
        Some((node, self.span_from(start)))
    }

    /// `macro { .. }` — an anonymous, immediately-expanded macro block. An atom
    /// (expression position) and a statement (via
    /// [`Parser::parse_macro_block_statement`]).
    fn parse_macro_block(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect(&Token::Macro)?;
            let body = parser.parse_block()?;
            Some((Node::MacroBlock(body), parser.span_from(start)))
        })
    }

    /// `macro { .. } ;?` — a macro block used as a statement (the `;` is OPTIONAL,
    /// unlike the mandatory `;` on an expression statement); the node's span
    /// excludes the `;`.
    fn parse_macro_block_statement(&mut self) -> Option<Spanned<Node<'src>>> {
        let macro_block = self.parse_macro_block()?;
        self.eat_ctrl(';');
        Some(macro_block)
    }

    /// `macro name(args)` — a macro invocation. An atom (expression position) and a
    /// statement (via [`Parser::parse_macro_invocation_statement`]). Arguments are
    /// captured as SPANS (their source text is what the macro receives).
    fn parse_macro_invocation(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect(&Token::Macro)?;
            let name_start = parser.position;
            let name = parser.eat_ident()?;
            let name_span = parser.span_from(name_start);
            parser.expect_ctrl('(')?;
            let arguments =
                parser.comma_list(Self::parse_argument_span, |parser| parser.peek_is_ctrl(')'))?;
            parser.expect_ctrl(')')?;
            Some((
                Node::MacroInvocation(name, name_span, arguments),
                parser.span_from(start),
            ))
        })
    }

    /// `macro name(args) ;?` — a macro invocation used as a statement (`;` OPTIONAL);
    /// the node's span excludes the `;`.
    fn parse_macro_invocation_statement(&mut self) -> Option<Spanned<Node<'src>>> {
        let invocation = self.parse_macro_invocation()?;
        self.eat_ctrl(';');
        Some(invocation)
    }

    /// `(binder in source => body)` — a tuple comprehension. The `in` distinguishes
    /// it from a tuple/group atom (and the `=>` from the mapped *type* `(U in T:
    /// F)`); `source` is a secondary expression, `body` a full expression.
    /// Further `, binder in source` bindings ZIP with the first (B183): `(a in
    /// aa, b in bb => a + b)`. Backtracks when the `(binder in` shape is absent.
    fn parse_tuple_comprehension(&mut self) -> Option<Spanned<Node<'src>>> {
        self.attempt(|parser| {
            let start = parser.position;
            parser.expect_ctrl('(')?;
            let mut bindings = Vec::new();
            loop {
                let binder_start = parser.position;
                let binder = parser.eat_ident()?;
                let binder_span = parser.span_from(binder_start);
                parser.expect(&Token::In)?;
                let source = parser.parse_secondary(false)?;
                bindings.push(ComprehensionBinding {
                    binder,
                    binder_span,
                    source,
                });
                if !parser.eat_ctrl(',') {
                    break;
                }
            }
            parser.expect_op("=>")?;
            let body = parser.parse_expression()?;
            parser.expect_ctrl(')')?;
            Some((
                Node::TupleComprehension {
                    bindings,
                    body: Box::new(body),
                },
                parser.span_from(start),
            ))
        })
    }

    /// `[resource]` NOT followed by `external` / `struct` / `enum` — the
    /// misplaced-attribute steer (item 15 in the statement choice, after
    /// `struct`/`enum`, so a valid `[resource] struct` / `[resource] external
    /// struct` / `[resource] enum` is never shadowed). Emits a parse error and
    /// recovers to a `Node::Error` spanning the attribute, leaving the item after
    /// it unconsumed (so `fun`/`impl`/`let`/`trait` still parse on the next
    /// statement). B413 moved the kind from a keyword to this attribute; the
    /// steer moved with it.
    fn parse_misplaced_resource(&mut self) -> Option<Spanned<Node<'src>>> {
        let start = self.position;
        if !self.eat_marker_attribute("resource") {
            return None;
        }
        if matches!(
            self.peek(),
            Some(Token::External | Token::Struct | Token::Enum | Token::Trait)
        ) {
            return None;
        }
        let span = self.span_from(start);
        self.push_misplaced_resource(span);
        Some((Node::Error, span))
    }

    /// The `[resource]` kind of a struct or an enum (B413), or `false`. The
    /// RETIRED keyword spelling — `resource` as a word right before `struct`,
    /// `external` or `enum`, which only a declaration can be — is taken too, as
    /// the kind it meant, with ONE refusal steering to the attribute: the
    /// declaration still parses, so the reader gets the sentence and not a
    /// cascade.
    fn parse_resource_kind(&mut self) -> bool {
        if self.eat_marker_attribute("resource") {
            return true;
        }
        if self.peek() == Some(&Token::Ident("resource"))
            && matches!(
                self.peek_at(1),
                Some(Token::External | Token::Struct | Token::Enum)
            )
        {
            let span = self.here_span();
            self.bump();
            self.errors.push(ParseError {
                span,
                reason: ParseErrorReason::Rule(
                    "`resource` is an attribute, not a keyword: write `[resource]` before the \
                     declaration (`[resource] struct`, `[resource] external struct`, \
                     `[resource] enum`)",
                ),
                context: Vec::new(),
                hint: None,
            });
            return true;
        }
        false
    }

    /// B413: `[resource]` where no type is declared — a field, a variant — is
    /// the misplaced-attribute steer, and the position then parses as if it
    /// were not there.
    fn refuse_misplaced_resource_attribute(&mut self) {
        let start = self.position;
        if self.eat_marker_attribute("resource") {
            let span = self.span_from(start);
            self.push_misplaced_resource(span);
        }
    }

    /// The one statement of the misplaced-`[resource]` rule, for every position
    /// that refuses it.
    fn push_misplaced_resource(&mut self, span: Span) {
        self.errors.push(ParseError {
            span,
            reason: ParseErrorReason::Rule(
                "`[resource]` marks a type as a resource: it may label only a `struct`, \
                 an `enum` or a `trait` declaration",
            ),
            context: Vec::new(),
            hint: None,
        });
    }

    /// The whole part of a `Number` token, consumed — the chumsky `integer`
    /// selector, used by tuple bounds and array lengths.
    fn eat_integer(&mut self) -> Option<&'src str> {
        Some(self.eat_number()?.0)
    }

    /// A whole `Number` token, consumed: `(whole, fraction, suffix)`. The
    /// discriminant production needs every part, because which parts are
    /// PRESENT is what says whether the literal is the integer the grammar
    /// means.
    fn eat_number(&mut self) -> Option<(&'src str, Option<&'src str>, Option<&'src str>)> {
        if let Some(Token::Number(whole, fraction, suffix)) = self.peek() {
            let parts = (*whole, *fraction, *suffix);
            self.bump();
            Some(parts)
        } else {
            None
        }
    }
}

/// Whether a node is a block-bearing form that may be a statement without a
/// trailing `;` (the chumsky `if_`/`for_`/`match_`/`block` statement alternatives).
fn is_block_like(node: &Node<'_>) -> bool {
    matches!(
        node,
        Node::If(_) | Node::For(..) | Node::ForIn(..) | Node::Match(..) | Node::Block(_)
    ) && !is_then_form(node)
}

/// Apply one plain postfix to a subject, spanning from the chain's start. A
/// faithful copy of `chain_expr_parser::apply_postfix`.
fn apply_postfix<'src>(
    subject: Spanned<Node<'src>>,
    postfix: Postfix<'src>,
    end: usize,
) -> Spanned<Node<'src>> {
    let span: Span = (subject.1.start..end).into();
    match postfix {
        Postfix::Member(member) => (
            Node::MemberAccessor(Box::new(subject), Box::new(member)),
            span,
        ),
        Postfix::Index(index) => (Node::Index(Box::new(subject), Box::new(index)), span),
        Postfix::TryAssert => (Node::TryAssert(Box::new(subject)), span),
        Postfix::LiftBare => (Node::Lifted(Box::new(subject)), span),
        // Grouping is handled in the loop below; a `LiftMember` never reaches this
        // arm from `group_postfixes`, but the faithful construction is kept.
        Postfix::LiftMember(member) => (
            Node::Lift(
                Box::new(subject),
                Box::new((
                    Node::MemberAccessor(Box::new((Node::LiftBinder, member.1)), Box::new(member)),
                    span,
                )),
            ),
            span,
        ),
        Postfix::DirectCall(arguments) => (Node::Call(Box::new(subject), None, arguments), span),
    }
}

/// Group a collected postfix list onto its base, building each `?.` link's
/// continuation from the following plain postfixes (up to the next `?.`/`!`/chain
/// end). A faithful copy of `chain_expr_parser`'s member-accessor fold.
fn group_postfixes<'src>(
    base: Spanned<Node<'src>>,
    postfixes: Vec<(Postfix<'src>, Span)>,
) -> Spanned<Node<'src>> {
    let mut current = base;
    let mut items = postfixes.into_iter().peekable();
    while let Some((postfix, postfix_span)) = items.next() {
        match postfix {
            Postfix::LiftMember(member) => {
                let member_span = member.1;
                let mut continuation: Spanned<Node> = (
                    Node::MemberAccessor(
                        Box::new((Node::LiftBinder, member_span)),
                        Box::new(member),
                    ),
                    member_span,
                );
                let mut lift_end = postfix_span.end;
                while matches!(
                    items.peek(),
                    Some((
                        Postfix::Member(_) | Postfix::Index(_) | Postfix::DirectCall(_),
                        _
                    ))
                ) {
                    let (step, step_span) = items.next().unwrap();
                    lift_end = step_span.end;
                    continuation = apply_postfix(continuation, step, lift_end);
                }
                let span: Span = (current.1.start..lift_end).into();
                current = (Node::Lift(Box::new(current), Box::new(continuation)), span);
            }
            step => {
                current = apply_postfix(current, step, postfix_span.end);
            }
        }
    }
    current
}

#[cfg(test)]
mod tests {
    //! Durable pins for the S2 grammar. Unlike the differential (which retires at
    //! S5 with the chumsky oracle), these touch no chumsky and survive the cutover:
    //! they are the parser's own regression corpus for the shapes S2 introduced —
    //! precedence, associativity, the `?.` grouping, the split-shift reassembly, the
    //! §H.1 condition mode, and the type forms.

    use super::*;

    /// Every nesting refusal must spell out the limit it enforces, and spell out
    /// the SAME one (B142). `ParseErrorReason::Rule` takes a `&'static str`, so
    /// unlike the analyzer's twin — which interpolates `WALK_DEPTH_LIMIT` into a
    /// `format!` — these six sentences carry the number as literal text. Nothing
    /// but this test stops `NESTING_DEPTH_LIMIT` from being retuned while the
    /// diagnostics keep quoting the old figure, which would be a compiler
    /// telling a writer to flatten to a depth it no longer enforces.
    #[test]
    fn the_refusals_spell_out_the_limit() {
        let limit = Parser::NESTING_DEPTH_LIMIT.to_string();
        for refusal in [
            Parser::NESTING_REFUSAL,
            Parser::TYPE_NESTING_REFUSAL,
            Parser::PATTERN_NESTING_REFUSAL,
            Parser::ITEM_NESTING_REFUSAL,
            Parser::IMPORT_NESTING_REFUSAL,
            Parser::ELEMENT_NESTING_REFUSAL,
        ] {
            assert!(
                refusal.contains(&format!("nests more than {limit} levels deep")),
                "the refusal must quote NESTING_DEPTH_LIMIT ({limit}) on the \
                 shared spine, got: {refusal}"
            );
            assert!(
                refusal.contains("which parsing refuses"),
                "the refusal must say who refused, got: {refusal}"
            );
        }
    }

    /// Parse `source` as a bare expression, asserting a clean full-consumption
    /// parse. Spans are at their natural source offsets (no wrapper).
    fn expr(source: &str) -> Spanned<Node<'_>> {
        expr_in_mode(source, false)
    }

    /// [`expr`], parsed in the formatter's group-preserving mode.
    fn expr_preserving_groups(source: &str) -> Spanned<Node<'_>> {
        expr_in_mode(source, true)
    }

    fn expr_in_mode(source: &str, preserve_paren_groups: bool) -> Spanned<Node<'_>> {
        let (mut tokens, errors) = lexing::tokenize(source);
        assert!(errors.is_empty(), "lex errors on {source:?}: {errors:?}");
        let token_count = tokens.len();
        let mut parser = Parser::new(&mut tokens, source, preserve_paren_groups);
        let node = parser.parse_expression().expect("expression did not parse");
        assert_eq!(
            parser.position, token_count,
            "unconsumed tokens parsing {source:?}: {node:?}"
        );
        node
    }

    /// Parse `source` as a condition-position expression (§H.1: struct-literal-free).
    fn condition(source: &str) -> Spanned<Node<'_>> {
        let (mut tokens, errors) = lexing::tokenize(source);
        assert!(errors.is_empty(), "lex errors on {source:?}: {errors:?}");
        let token_count = tokens.len();
        let mut parser = Parser::new(&mut tokens, source, false);
        let node = parser.parse_condition().expect("condition did not parse");
        assert_eq!(
            parser.position, token_count,
            "unconsumed parsing {source:?}"
        );
        node
    }

    /// Parse `source` as a type, asserting a clean full-consumption parse.
    fn type_(source: &str) -> Spanned<Node<'_>> {
        let (mut tokens, errors) = lexing::tokenize(source);
        assert!(errors.is_empty(), "lex errors on {source:?}: {errors:?}");
        let token_count = tokens.len();
        let mut parser = Parser::new(&mut tokens, source, false);
        let node = parser.parse_type().expect("type did not parse");
        assert_eq!(
            parser.position, token_count,
            "unconsumed parsing {source:?}"
        );
        node
    }

    /// Whether the whole-program entry declines `source` (a non-empty error list).
    fn declines(source: &str) -> bool {
        let (tree, errors) = parse(source);
        tree.is_none() || !errors.is_empty()
    }

    /// Parse a whole `source` file, asserting a clean parse (a tree and no errors),
    /// and return its statement list. The S3 whole-file entry — the one S2's seam
    /// could not reach.
    fn program(source: &str) -> Spanned<NodeList<'_>> {
        let (tree, errors) = parse(source);
        assert!(errors.is_empty(), "parse errors on {source:?}: {errors:?}");
        tree.expect("program did not parse")
    }

    /// The single top-level item's node, for the common one-item pins.
    fn only_item(source: &str) -> Node<'_> {
        let (mut statements, _span) = program(source);
        assert_eq!(statements.len(), 1, "expected one item in {source:?}");
        statements.remove(0).0
    }

    // --- Precedence and associativity (full span-inclusive Debug) ------------

    #[test]
    fn product_binds_tighter_than_sum() {
        assert_eq!(
            format!("{:?}", expr("a + b * c")),
            "(Binary(Add, (Accessor(\"a\"), 0..1), \
             (Binary(Mul, (Accessor(\"b\"), 4..5), (Accessor(\"c\"), 8..9)), 4..9)), 0..9)"
        );
    }

    #[test]
    fn subtraction_is_left_associative() {
        assert_eq!(
            format!("{:?}", expr("a - b - c")),
            "(Binary(Sub, (Binary(Sub, (Accessor(\"a\"), 0..1), (Accessor(\"b\"), 4..5)), 0..5), \
             (Accessor(\"c\"), 8..9)), 0..9)"
        );
    }

    #[test]
    fn bitand_binds_tighter_than_bitor_which_binds_tighter_than_compare() {
        // `a & b == c | d` — Rust's order: `&` over `|`, both over `==`.
        match &expr("a & b == c | d").0 {
            Node::Binary(BinaryOp::Eq, left, right) => {
                assert!(matches!(left.0, Node::Binary(BinaryOp::BitAnd, _, _)));
                assert!(matches!(right.0, Node::Binary(BinaryOp::BitOr, _, _)));
            }
            other => panic!("expected Eq at root, got {other:?}"),
        }
    }

    #[test]
    fn logical_and_binds_tighter_than_logical_or() {
        match &expr("a && b || c").0 {
            Node::Binary(BinaryOp::Or, left, _) => {
                assert!(matches!(left.0, Node::Binary(BinaryOp::And, _, _)));
            }
            other => panic!("expected Or at root, got {other:?}"),
        }
    }

    #[test]
    fn is_binds_tighter_than_logical_and() {
        // `x is None && ready` — the `is` groups with `x`, not with `ready`.
        match &expr("x is None && ready").0 {
            Node::Binary(BinaryOp::And, left, right) => {
                assert!(matches!(left.0, Node::Is(_, _)));
                assert!(matches!(right.0, Node::Accessor("ready")));
            }
            other => panic!("expected And at root, got {other:?}"),
        }
    }

    // --- The split-shift reassembly -----------------------------------------

    #[test]
    fn adjacent_angle_pair_is_a_shift() {
        assert_eq!(
            format!("{:?}", expr("a << b")),
            "(Binary(Shl, (Accessor(\"a\"), 0..1), (Accessor(\"b\"), 5..6)), 0..6)"
        );
        match &expr("a >> b").0 {
            Node::Binary(BinaryOp::Shr, _, _) => {}
            other => panic!("expected Shr, got {other:?}"),
        }
    }

    #[test]
    fn non_adjacent_angle_pair_is_not_a_shift() {
        // `a < < b` (a space between the angles) is neither a shift nor a valid
        // comparison — it does not fully parse.
        assert!(declines("let __probe = a < < b;"));
        // A lone `<` stays a comparison.
        match &expr("a < b").0 {
            Node::Binary(BinaryOp::Lt, _, _) => {}
            other => panic!("expected Lt, got {other:?}"),
        }
    }

    #[test]
    fn shift_binds_tighter_than_bitor_but_looser_than_sum() {
        // `a + b << c | d` — `+` over `<<` over `|`.
        match &expr("a + b << c | d").0 {
            Node::Binary(BinaryOp::BitOr, left, _) => match &left.0 {
                Node::Binary(BinaryOp::Shl, shift_left, _) => {
                    assert!(matches!(shift_left.0, Node::Binary(BinaryOp::Add, _, _)));
                }
                other => panic!("expected Shl under BitOr, got {other:?}"),
            },
            other => panic!("expected BitOr at root, got {other:?}"),
        }
    }

    // --- The postfix / `?.` grouping ----------------------------------------

    #[test]
    fn lift_link_absorbs_following_plain_postfixes() {
        // `a?.b.c` — the `.c` joins the `?.b` link's continuation.
        match &expr("a?.b.c").0 {
            Node::Lift(subject, continuation) => {
                assert!(matches!(subject.0, Node::Accessor("a")));
                // continuation = (LiftBinder.b).c
                match &continuation.0 {
                    Node::MemberAccessor(inner, member) => {
                        assert!(matches!(member.0, Node::Accessor("c")));
                        assert!(matches!(inner.0, Node::MemberAccessor(_, _)));
                    }
                    other => panic!("expected nested MemberAccessor continuation, got {other:?}"),
                }
            }
            other => panic!("expected Lift, got {other:?}"),
        }
    }

    #[test]
    fn try_assert_stops_the_lift_continuation() {
        // `a?.b!` — the `!` applies to the LIFTED result, not inside the link.
        match &expr("a?.b!").0 {
            Node::TryAssert(inner) => assert!(matches!(inner.0, Node::Lift(_, _))),
            other => panic!("expected TryAssert wrapping a Lift, got {other:?}"),
        }
    }

    #[test]
    fn chained_lifts_nest() {
        // `a?.b?.c` — Lift(Lift(a, .b), .c).
        match &expr("a?.b?.c").0 {
            Node::Lift(subject, _) => assert!(matches!(subject.0, Node::Lift(_, _))),
            other => panic!("expected nested Lift, got {other:?}"),
        }
    }

    #[test]
    fn bare_question_is_a_lift_mark_and_a_group_records_it() {
        assert!(matches!(expr("a?").0, Node::Lifted(_)));
        // Parens containing a mark become a LiftGroup; otherwise they dissolve.
        assert!(matches!(expr("(a?)").0, Node::LiftGroup(_)));
        assert!(matches!(expr("(a)").0, Node::Accessor("a")));
    }

    // --- The formatter's group-preserving mode -------------------------------

    /// The COMPILER's parse is unchanged by the formatter-only flag: a group
    /// carrying no lift mark still dissolves to its inner expression, keeping
    /// the inner's own span. This is the seam the "main pipeline untouched"
    /// constraint lives at — every pipeline entry (`lib.rs`, the CLI, the module
    /// loader, macro expansion, the language server's analysis) calls `parse`,
    /// and only `formatter::parse` calls `parse_preserving_groups`.
    #[test]
    fn the_default_mode_still_dissolves_a_mark_free_group() {
        assert!(matches!(expr("(a)").0, Node::Accessor("a")));
        assert!(matches!(
            expr("(1 + 2)").0,
            Node::Binary(BinaryOp::Add, _, _)
        ));
        // Dissolving keeps the INNER expression's own span — the parens
        // contribute nothing, so `1 + 2` spans 1..6 of `(1 + 2)`, not 0..7.
        assert_eq!(expr("(1 + 2)").1.into_range(), 1..6);
        // A nested group dissolves at every level.
        assert!(matches!(expr("((a))").0, Node::Accessor("a")));
    }

    /// Group-preserving mode records every group — including the ones the
    /// default mode dissolves — and the recorded node spans the parentheses.
    #[test]
    fn group_preserving_mode_records_every_paren_group() {
        let group = expr_preserving_groups("(1 + 2)");
        assert!(matches!(group.0, Node::LiftGroup(_)));
        assert_eq!(group.1.into_range(), 0..7);
        assert!(matches!(
            expr_preserving_groups("(a)").0,
            Node::LiftGroup(_)
        ));
        // Nested groups nest, one node per pair of parentheses.
        match &expr_preserving_groups("((a))").0 {
            Node::LiftGroup(inner) => assert!(matches!(inner.0, Node::LiftGroup(_))),
            other => panic!("expected a nested LiftGroup, got {other:?}"),
        }
        // A mark-carrying group is recorded exactly as it already was.
        assert!(matches!(
            expr_preserving_groups("(a?)").0,
            Node::LiftGroup(_)
        ));
    }

    /// Only the GROUP form is affected: a tuple stays a tuple, and a
    /// parenthesized expression list elsewhere (call arguments) is untouched —
    /// the flag is read at one site, the group branch of the paren atom.
    #[test]
    fn group_preserving_mode_leaves_tuples_and_arguments_alone() {
        assert!(matches!(expr_preserving_groups("(a, b)").0, Node::Tuple(_)));
        match &expr_preserving_groups("f(a)").0 {
            Node::Call(_, _, arguments) => {
                assert!(matches!(arguments.0[0].0, Node::Accessor("a")));
            }
            other => panic!("expected a Call, got {other:?}"),
        }
    }

    /// `..e` is a spread only where an ELEMENT BEGINS — a tuple construction's
    /// entry and a call argument (variadic-generics.md §T.1). Position, not
    /// adjacency, is the disambiguator, and the leading `..` also settles the
    /// tuple/group fork, which is what makes the lone `(..a)` a construction.
    #[test]
    fn a_leading_dot_dot_marks_a_spread_element() {
        let spread_at = |node: &Node, index: usize| match node {
            Node::Tuple(items) => matches!(items[index].0, Node::Spread(_)),
            other => panic!("expected a Tuple, got {other:?}"),
        };
        assert!(spread_at(&expr("(..a, b)").0, 0), "leading");
        assert!(spread_at(&expr("(b, ..a)").0, 1), "trailing");
        assert!(spread_at(&expr("(b, ..a, c)").0, 1), "interleaved");
        let twice = expr("(..a, ..b)");
        assert!(spread_at(&twice.0, 0) && spread_at(&twice.0, 1), "two");
        // A lone spread is a Tuple, not the group `(e)` would have dissolved to.
        assert!(spread_at(&expr("(..a)").0, 0), "lone");
        match &expr("f(..a)").0 {
            Node::Call(_, _, arguments) => {
                assert!(matches!(arguments.0[0].0, Node::Spread(_)));
            }
            other => panic!("expected a Call, got {other:?}"),
        }
    }

    /// The ruling's whole value is what it leaves ALONE. `..` after an
    /// expression is still the member-access dots it has always been: `(1..3,
    /// x)` parses — silently, as it did before the spread existed — into a
    /// member chain over `1` whose first link is an error node, and reaches no
    /// spread branch because its element does not begin with `..`. Vilan has no
    /// range operator; if one ever lands, this is the shape it must not break.
    #[test]
    fn dots_after_an_expression_are_still_member_access() {
        match &expr("(1..3, x)").0 {
            Node::Tuple(items) => {
                assert_eq!(items.len(), 2);
                assert!(
                    !matches!(items[0].0, Node::Spread(_)),
                    "`1..3` is not a spread — the element does not begin with `..`"
                );
                assert!(matches!(items[0].0, Node::MemberAccessor(_, _)));
            }
            other => panic!("expected a Tuple, got {other:?}"),
        }
    }

    #[test]
    fn direct_call_folds_onto_a_method_result() {
        // `self.hook.read()(a)` — the trailing `(a)` is a DirectCall on the member
        // result (backlog §H.18).
        match &expr("self.hook.read()(a)").0 {
            Node::Call(callee, None, _) => assert!(matches!(callee.0, Node::MemberAccessor(_, _))),
            other => panic!("expected outer Call over a MemberAccessor, got {other:?}"),
        }
    }

    #[test]
    fn member_call_fuses_one_call_then_leaves_the_rest() {
        // `a.method<T>(x)` — the member is a single fused generic call.
        match &expr("a.method<T>(x)").0 {
            Node::MemberAccessor(_, member) => match &member.0 {
                Node::Call(_, Some(_), _) => {}
                other => panic!("expected a generic Call member, got {other:?}"),
            },
            other => panic!("expected MemberAccessor, got {other:?}"),
        }
    }

    #[test]
    fn tuple_index_reads_a_number_member() {
        match &expr("a.0").0 {
            Node::MemberAccessor(_, member) => {
                assert!(matches!(member.0, Node::Number("0", None, None)))
            }
            other => panic!("expected MemberAccessor with a Number member, got {other:?}"),
        }
    }

    // --- Static paths and generics ------------------------------------------

    #[test]
    fn generic_static_head_needs_a_trailing_colon_colon() {
        // `List<str>::new()` — the head keeps its generics because `::` follows.
        match &expr("List<str>::new()").0 {
            Node::Call(callee, None, _) => match &callee.0 {
                // No tail generics: an expression path never carries them.
                Node::StaticAccessor(head, "new", None) => {
                    assert!(matches!(head.0, Node::AccessorWithGenerics("List", _)))
                }
                other => panic!("expected StaticAccessor over generics, got {other:?}"),
            },
            other => panic!("expected Call, got {other:?}"),
        }
    }

    #[test]
    fn generic_call_without_colon_colon_attaches_to_the_call() {
        // `default<Id>()` — the bare name wins the atom, the generics go to the call.
        match &expr("default<Id>()").0 {
            Node::Call(callee, Some(_), _) => {
                assert!(matches!(callee.0, Node::Accessor("default")))
            }
            other => panic!("expected a generic Call over a bare name, got {other:?}"),
        }
    }

    #[test]
    fn bare_generic_lookalike_is_a_comparison() {
        // `foo<T>` with no `::` and no `(` does NOT form generics — the `<` is a
        // comparison. As a whole expression the `>` is left dangling, so only the
        // expression prefix is checked here; the generics-attach-to-a-call contrast
        // is `default<Id>()` above.
        let (mut tokens, errors) = lexing::tokenize("foo<T>");
        assert!(errors.is_empty());
        let mut parser = Parser::new(&mut tokens, "foo<T>", false);
        let node = parser.parse_expression().expect("prefix parses");
        assert!(matches!(node.0, Node::Binary(BinaryOp::Lt, _, _)));
    }

    // --- Unary --------------------------------------------------------------

    #[test]
    fn unary_stacks_and_binds_tighter_than_member() {
        assert!(matches!(expr("!!x").0, Node::Unary('!', _)));
        // `-a.b` is `-(a.b)` — unary binds looser than the member chain.
        match &expr("-a.b").0 {
            Node::Unary('-', inner) => assert!(matches!(inner.0, Node::MemberAccessor(_, _))),
            other => panic!("expected Unary('-') over a member, got {other:?}"),
        }
    }

    #[test]
    fn reference_and_dereference() {
        assert!(matches!(expr("&mut x").0, Node::Reference(true, _)));
        assert!(matches!(expr("&x").0, Node::Reference(false, _)));
        assert!(matches!(expr("*x").0, Node::Dereference(_)));
    }

    #[test]
    fn async_takes_a_block_or_a_unary() {
        match &expr("async { f() }").0 {
            Node::Async(inner) => assert!(matches!(inner.0, Node::Block(_))),
            other => panic!("expected Async(Block), got {other:?}"),
        }
        match &expr("async f()").0 {
            Node::Async(inner) => assert!(matches!(inner.0, Node::Call(_, _, _))),
            other => panic!("expected Async(Call), got {other:?}"),
        }
    }

    // --- §H.1 condition mode -------------------------------------------------

    #[test]
    fn condition_mode_excludes_struct_literals() {
        // In condition position a bare name is an accessor; the `{` after it is a
        // block, not a struct literal — so `Foo` alone parses to `Accessor`.
        assert!(matches!(condition("Foo").0, Node::Accessor("Foo")));
        // In expression position the same head with a brace IS a struct literal.
        assert!(matches!(
            expr("Foo { x = 1 }").0,
            Node::StructInitializer(_, ("Foo", _), _, _)
        ));
        // A parenthesised struct literal is admitted even in a condition.
        assert!(matches!(
            condition("(Foo { x = 1 })").0,
            Node::StructInitializer(..)
        ));
    }

    // --- Assignment ----------------------------------------------------------

    #[test]
    fn assignment_targets_and_operators() {
        assert!(matches!(expr("x = 5").0, Node::Assign(_, None, _)));
        assert!(matches!(
            expr("x += 1").0,
            Node::Assign(_, Some(BinaryOp::Add), _)
        ));
        match &expr("*p = 5").0 {
            Node::Assign(target, None, _) => assert!(matches!(target.0, Node::Dereference(_))),
            other => panic!("expected Assign over a Dereference target, got {other:?}"),
        }
    }

    // --- Closures ------------------------------------------------------------

    #[test]
    fn closures_parse_params_return_type_and_body() {
        assert!(matches!(expr("|| 0").0, Node::Closure(_)));
        match &expr("|x: i32|: i32 x + 1").0 {
            Node::Closure(closure) => {
                assert_eq!(closure.parameters.0.len(), 1);
                assert!(closure.return_type.is_some());
                assert!(matches!(
                    closure.return_value.0,
                    Node::Binary(BinaryOp::Add, _, _)
                ));
            }
            other => panic!("expected Closure, got {other:?}"),
        }
    }

    // --- match / if / blocks -------------------------------------------------

    #[test]
    fn match_legs_patterns_guards_and_or_patterns() {
        match &expr("match x { let a if a > 0 => a, Some(let b), None => b }").0 {
            Node::Match(_, legs) => {
                assert_eq!(legs.0.len(), 2);
                assert!(legs.0[0].1.is_some(), "first leg has a guard");
                assert_eq!(legs.0[1].0.len(), 2, "second leg is an or-pattern");
            }
            other => panic!("expected Match, got {other:?}"),
        }
    }

    #[test]
    fn block_takes_statements_then_a_trailing_value() {
        match &expr("{ let x = 1; x }").0 {
            Node::Block(body) => {
                assert_eq!(body.0.0.len(), 1, "one statement");
                assert!(matches!(body.0.1.0, Node::Accessor("x")), "trailing value");
            }
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[test]
    fn empty_block_value_is_void_at_the_closing_brace() {
        match &expr("{ }").0 {
            Node::Block(body) => assert!(matches!(body.0.1.0, Node::Void)),
            other => panic!("expected Block, got {other:?}"),
        }
    }

    #[test]
    fn if_else_if_chains_nest_in_the_else_branch() {
        match &expr("if a { 1 } else if b { 2 } else { 3 }").0 {
            Node::If(NodeIfBranch::If(if_)) => match &if_.else_ {
                Some((NodeIfBranch::If(_), _)) => {}
                other => panic!("expected an else-if branch, got {other:?}"),
            },
            other => panic!("expected If, got {other:?}"),
        }
    }

    // --- Types ---------------------------------------------------------------

    #[test]
    fn reference_array_and_local_types() {
        assert_eq!(
            format!("{:?}", type_("&mut T")),
            "(Reference(true, (Accessor(\"T\"), 5..6)), 0..6)"
        );
        assert!(matches!(type_("[i32; 4]").0, Node::ArrayType(_, _)));
        assert!(matches!(
            type_("List<T>").0,
            Node::AccessorWithGenerics("List", _)
        ));
    }

    #[test]
    fn closure_types_have_no_arrow_and_take_markers() {
        // `|A| B` — the return type follows the params directly (no arrow).
        match &type_("|A| B").0 {
            Node::ClosureType(parameters, Some(_)) => assert_eq!(parameters.0.len(), 1),
            other => panic!("expected ClosureType with a return, got {other:?}"),
        }
        assert!(matches!(type_("async || T").0, Node::AsyncType(_)));
        assert!(matches!(type_("sync || T").0, Node::SyncType(_)));
        // A bare `sync` is still a type name (the marker only bites before `|`).
        assert!(matches!(type_("sync").0, Node::Accessor("sync")));
    }

    #[test]
    fn mapped_tuple_and_context_types() {
        assert!(matches!(type_("(U in T: F<U>)").0, Node::MappedType { .. }));
        assert!(matches!(type_("(A, B)").0, Node::Tuple(_)));
        assert!(
            matches!(type_("(A)").0, Node::Tuple(_)),
            "a one-tuple, not a group"
        );
        match &type_("Foo context bar").0 {
            Node::TypeWithContexts(_, contexts) => assert_eq!(contexts.len(), 1),
            other => panic!("expected TypeWithContexts, got {other:?}"),
        }
    }

    // B242: the DECLARATION's `context` clause, both ways it can arrive — with
    // a return type written the type grammar takes it first and the
    // declaration peels it back off, so the two spellings parse the same.
    #[test]
    fn function_context_clause_binds_to_the_declaration() {
        for source in [
            "fun render(x: i32): i32 context settings { x }",
            "fun render(x: i32) context settings { x }",
            "fun render(x: i32): i32 borrows x context settings { x }",
        ] {
            match only_item(source) {
                Node::Func(function) => {
                    let (names, _) = function.contexts.as_ref().expect("a declared clause");
                    assert_eq!(names.len(), 1, "{source}");
                    assert_eq!(names[0].0, "settings", "{source}");
                    // The return type is the bare one: the clause is the
                    // FUNCTION's, never a suffix on `i32`.
                    assert!(
                        function
                            .return_type
                            .as_ref()
                            .is_none_or(|node| matches!(node.0, Node::Accessor("i32"))),
                        "{source}"
                    );
                }
                other => panic!("expected Func, got {other:?}"),
            }
        }
        match only_item("fun render(): i32 context (a, b) { 1 }") {
            Node::Func(function) => {
                let (names, _) = function.contexts.as_ref().expect("a declared clause");
                assert_eq!(
                    names.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
                    vec!["a", "b"]
                );
            }
            other => panic!("expected Func, got {other:?}"),
        }
    }

    // E233: a VIEW return type (`&T`, `&mut T`) takes the clause the same way —
    // the `&` production parses a whole type, so the clause first lands on the
    // view's target and is hoisted onto the declaration from there. The return
    // type keeps its `&`, and spans only the view.
    #[test]
    fn e233_a_view_return_types_context_clause_binds_to_the_declaration() {
        for (source, mutable) in [
            (
                "fun pick(xs: &L): &i32 context settings borrows xs { xs }",
                false,
            ),
            (
                "fun pick(xs: &L): &i32 borrows xs context settings { xs }",
                false,
            ),
            (
                "fun pick(xs: &mut L): &mut i32 context settings borrows xs { xs }",
                true,
            ),
        ] {
            match only_item(source) {
                Node::Func(function) => {
                    let (names, _) = function.contexts.as_ref().expect("a declared clause");
                    assert_eq!(names[0].0, "settings", "{source}");
                    assert_eq!(function.borrows, Some("xs"), "{source}");
                    let returns = function.return_type.as_ref().expect("a return type");
                    match &returns.0 {
                        Node::Reference(written, target) => {
                            assert_eq!(*written, mutable, "{source}");
                            assert!(matches!(target.0, Node::Accessor("i32")), "{source}");
                        }
                        other => panic!("expected a view return type, got {other:?} ({source})"),
                    }
                    let view = if mutable { "&mut i32" } else { "&i32" };
                    assert_eq!(&source[returns.1.into_range()], view, "{source}");
                }
                other => panic!("expected Func, got {other:?}"),
            }
        }
        // A view of a CLOSURE type keeps the clause on the closure (B309): the
        // hoist is the non-closure peel's, not a second rule.
        match only_item("fun pick(): &(|| i32) context settings { x }") {
            Node::Func(function) => assert!(function.contexts.is_none()),
            other => panic!("expected Func, got {other:?}"),
        }
    }

    // --- Decline behaviour (never more permissive than the grammar) ----------

    #[test]
    fn trailing_comma_tuple_and_single_tuple_decline() {
        assert!(declines("let __probe = (a, b,);"));
        assert!(declines("let __probe = (a,);"));
    }

    // --- S3 items: functions -------------------------------------------------

    #[test]
    fn function_signature_only_has_no_body() {
        // A `;` body (a required trait method / external declaration) is `None`.
        match only_item("fun default(): Self;") {
            Node::Func(function) => {
                assert_eq!(function.name.0, "default");
                assert!(function.body.is_none(), "signature-only body is None");
                assert!(function.return_type.is_some());
            }
            other => panic!("expected Func, got {other:?}"),
        }
    }

    #[test]
    fn function_generics_conventions_and_borrows() {
        // Generics, an `own`/`&`/`&mut`/bare mix of conventions, a return type, and
        // a `borrows` clause — every function-signature axis in one pin.
        match only_item(
            "fun slot<T>(&mut self, own value: T, plain: i32): &mut T borrows self { self }",
        ) {
            Node::Func(function) => {
                let generics = function.generic_parameters.as_ref().expect("generics");
                assert_eq!(generics.0.len(), 1);
                let conventions: Vec<Convention> = function
                    .parameters
                    .0
                    .iter()
                    .map(|parameter| parameter.convention)
                    .collect();
                assert_eq!(
                    conventions,
                    vec![Convention::RefMut, Convention::Own, Convention::Bare]
                );
                assert_eq!(function.borrows, Some("self"));
                assert!(function.body.is_some());
            }
            other => panic!("expected Func, got {other:?}"),
        }
    }

    #[test]
    fn parameter_convention_is_inferred_from_a_reference_type() {
        // No prefix, but a `&mut T` type gives `RefMut`; a `&T` type gives `Ref`.
        match only_item("fun f(a: &mut i32, b: &i32, c: i32) { }") {
            Node::Func(function) => {
                let conventions: Vec<Convention> = function
                    .parameters
                    .0
                    .iter()
                    .map(|parameter| parameter.convention)
                    .collect();
                assert_eq!(
                    conventions,
                    vec![Convention::RefMut, Convention::Ref, Convention::Bare]
                );
            }
            other => panic!("expected Func, got {other:?}"),
        }
    }

    #[test]
    fn a_spread_parameter_is_marked_and_only_the_last_one_is() {
        // `...` is three `.` control tokens (the lexer has none of its own), so
        // the flag is the ONLY record that it was written — a dropped one is
        // silent. `mut` may precede it; the pack type is mandatory.
        match only_item("fun log<T: (2..)>(sep: str, mut ...items: T): i32 { 0 }") {
            Node::Func(function) => {
                let spreads: Vec<bool> = function
                    .parameters
                    .0
                    .iter()
                    .map(|parameter| parameter.spread)
                    .collect();
                assert_eq!(spreads, vec![false, true]);
                let last = &function.parameters.0[1];
                assert!(last.mutable, "`mut ...items` keeps binder mutability");
                assert_eq!(last.convention, Convention::Bare);
                assert!(last.declared_type.is_some(), "the pack type is mandatory");
            }
            other => panic!("expected Func, got {other:?}"),
        }
    }

    #[test]
    fn async_fun_is_a_function_not_an_expression() {
        // `async fun` is an item (the expression attempt fails on `fun`), unlike
        // `async { .. }` / `async expr`, which are expressions.
        match only_item("async fun tick() { }") {
            Node::Func(function) => {
                assert!(function.is_async);
                assert!(!function.external);
            }
            other => panic!("expected an async Func, got {other:?}"),
        }
        // `external fun` is a bodyless intrinsic.
        match only_item("external fun print(line: str);") {
            Node::Func(function) => {
                assert!(function.external);
                assert!(function.body.is_none());
            }
            other => panic!("expected an external Func, got {other:?}"),
        }
    }

    // --- S3 items: structs / enums -------------------------------------------

    #[test]
    fn struct_fields_generics_and_modifiers() {
        match only_item("struct Point<T> { x: T, y: T }") {
            Node::Struct(name, generics, external, resource, body, _) => {
                assert_eq!(name.0, "Point");
                assert!(generics.is_some());
                assert!(!external && !resource);
                assert_eq!(body.expect("fields").0.len(), 2);
            }
            other => panic!("expected Struct, got {other:?}"),
        }
        // `[resource] external struct null;` — every modifier, the `null` name, the
        // bodyless `;` form.
        match only_item("[resource] external struct null;") {
            Node::Struct(name, _, external, resource, body, _) => {
                assert_eq!(name.0, "null");
                assert!(external && resource);
                assert!(body.is_none());
            }
            other => panic!("expected Struct, got {other:?}"),
        }
    }

    #[test]
    fn exposed_struct_field_is_recorded() {
        match only_item("struct Room { [expose] count: Signal, name: str }") {
            Node::Struct(_, _, _, _, Some(fields), _) => {
                let exposed: Vec<Exposure> = fields.0.iter().map(|field| field.0.2).collect();
                assert_eq!(exposed, vec![Exposure::Whole, Exposure::None]);
            }
            other => panic!("expected a struct with fields, got {other:?}"),
        }
    }

    /// A142 S7: `[reactive(..)]`'s two arguments, alone and together, on either
    /// side of `[expose]` — recorded on the field's fifth slot, which the
    /// `Storable` derive reads.
    #[test]
    fn a_reactive_attribute_is_recorded_on_its_field() {
        let source = "struct S { [reactive(coarse)] a: i32, [reactive(name = \"verb\")] get: str, \
                      [reactive(coarse, name = \"all\")] [expose] b: C, \
                      [expose] [reactive(name = \"c2\")] c: C, d: i32 }";
        match only_item(source) {
            Node::Struct(_, _, _, _, Some(fields), _) => {
                let knobs: Vec<Reactivity> = fields.0.iter().map(|field| field.0.4).collect();
                assert_eq!(
                    knobs,
                    vec![
                        Reactivity {
                            coarse: true,
                            name: None,
                            after_expose: false,
                        },
                        Reactivity {
                            coarse: false,
                            name: Some("verb"),
                            after_expose: false,
                        },
                        Reactivity {
                            coarse: true,
                            name: Some("all"),
                            after_expose: false,
                        },
                        Reactivity {
                            coarse: false,
                            name: Some("c2"),
                            after_expose: true,
                        },
                        Reactivity::default(),
                    ]
                );
                let exposed: Vec<Exposure> = fields.0.iter().map(|field| field.0.2).collect();
                assert_eq!(
                    exposed,
                    vec![
                        Exposure::None,
                        Exposure::None,
                        Exposure::Whole,
                        Exposure::Whole,
                        Exposure::None
                    ]
                );
            }
            other => panic!("expected a struct with fields, got {other:?}"),
        }
    }

    /// A142 S7: a malformed `[reactive(..)]` is refused where it stands, and the
    /// fields around it still parse — the refusal is the attribute's, not
    /// "expected a field name".
    #[test]
    fn a_malformed_reactive_attribute_is_refused_and_parsed_past() {
        for (source, expected) in [
            (
                "struct S { [reactive(fine)] a: i32, b: i32 }",
                "takes `coarse`",
            ),
            (
                "struct S { [reactive(name = \"1x\")] a: i32, b: i32 }",
                "must be an identifier",
            ),
            (
                "struct S { [reactive(name)] a: i32, b: i32 }",
                "takes `coarse`",
            ),
            ("struct S { [reactive] a: i32, b: i32 }", "takes `coarse`"),
        ] {
            let (tree, errors) = parse(source);
            assert!(
                errors
                    .iter()
                    .any(|error| format!("{:?}", error.reason).contains(expected)),
                "{source}: expected a refusal containing {expected:?}, got {errors:?}"
            );
            let statements = tree.map(|(statements, _)| statements).unwrap_or_default();
            let field_count = statements.iter().find_map(|node| match &node.0 {
                Node::Struct(_, _, _, _, Some(fields), _) => Some(fields.0.len()),
                _ => None,
            });
            assert_eq!(field_count, Some(2), "{source}: both fields still parse");
        }
    }

    #[test]
    fn enum_variants_payloads_and_discriminants() {
        match only_item("enum Sign { Less = -1, Zero = 0, More(i32, str) }") {
            Node::Enum(name, _, resource, variants, _) => {
                assert_eq!(name.0, "Sign");
                assert!(!resource);
                let (_, less_data, less_backing, _) = &variants.0[0].0;
                assert!(less_data.is_empty());
                let less_backing = less_backing.as_ref().expect("Less has a backing value");
                match less_backing {
                    BackingLiteral::Int {
                        negative, whole, ..
                    } => {
                        assert!(negative);
                        assert_eq!(*whole, "1");
                    }
                    other => panic!("expected an integer backing, got {other:?}"),
                }
                assert_eq!(less_backing.to_string(), "-1");
                let (_, more_data, more_backing, _) = &variants.0[2].0;
                assert_eq!(more_data.len(), 2, "More carries two payload types");
                assert_eq!(*more_backing, None);
            }
            other => panic!("expected Enum, got {other:?}"),
        }
        // `[resource] enum` — an enum takes the attribute too (B413).
        match only_item("[resource] enum Handle { Open, Closed }") {
            Node::Enum(_, _, resource, _, _) => assert!(resource),
            other => panic!("expected a resource Enum, got {other:?}"),
        }
    }

    #[test]
    fn enum_variants_carry_string_backing_values() {
        // backed-enums.md §3.1: the discriminant production GENERALIZES to
        // `= ( (-)? INTEGER | STRING )`. The literal is carried raw, quotes
        // reprinted by `Display` so the formatter round-trips it and a
        // diagnostic can tell `1` from `"1"`.
        match only_item(r#"enum Align { Start = "flex-start", End = "end" }"#) {
            Node::Enum(name, _, _, variants, _) => {
                assert_eq!(name.0, "Align");
                let (_, _, start_backing, _) = &variants.0[0].0;
                match start_backing.as_ref().expect("Start has a backing value") {
                    BackingLiteral::Str { text, .. } => assert_eq!(*text, "flex-start"),
                    other => panic!("expected a string backing, got {other:?}"),
                }
                assert_eq!(
                    start_backing.as_ref().unwrap().to_string(),
                    "\"flex-start\""
                );
            }
            other => panic!("expected Enum, got {other:?}"),
        }
    }

    #[test]
    fn generic_parameter_bounds_and_tuple_bounds() {
        // A trait-bound list, a defaulted binder, and a tuple-arity bound.
        match only_item("fun f<A: Show + Eq, type B = Self, C: (2..: Display)>() { }") {
            Node::Func(function) => {
                let generics = &function.generic_parameters.as_ref().unwrap().0;
                assert_eq!(generics[0].bounds.len(), 2, "A: Show + Eq");
                assert!(
                    generics[1].is_type && generics[1].default.is_some(),
                    "type B = Self"
                );
                let tuple_bound = generics[2].tuple_bound.as_ref().expect("C tuple bound");
                assert_eq!(tuple_bound.lo, Some(2));
                assert_eq!(tuple_bound.hi, None);
                assert!(tuple_bound.element.is_some(), "(..: Display) element bound");
            }
            other => panic!("expected Func, got {other:?}"),
        }
    }

    // --- S3 items: impl / trait / mod ----------------------------------------

    #[test]
    fn impl_with_clause_and_body() {
        match only_item("impl Point<type T> with Show + Eq { fun show(&self): str { \"p\" } }") {
            Node::Impl(_subject, traits, body, _) => {
                assert_eq!(traits.len(), 2, "with Show + Eq");
                assert_eq!(body.0.len(), 1, "one method");
                assert!(matches!(body.0[0].0, Node::Func(_)));
            }
            other => panic!("expected Impl, got {other:?}"),
        }
    }

    #[test]
    fn an_anonymous_type_binder_parses_as_the_keyword_form_does() {
        // B294: `_` in type position is the binder production, with or without
        // a bound, and it produces the very node `type _` does.
        let binders = |source: &'static str| match only_item(source) {
            Node::Impl(subject, _traits, _body, _) => match subject.0 {
                Node::AccessorWithGenerics(_, arguments) => arguments
                    .0
                    .into_iter()
                    .map(|argument| match argument.0 {
                        Node::TypeBinder(name, bounds, _) => (name.0, bounds.len()),
                        other => panic!("expected a TypeBinder argument, got {other:?}"),
                    })
                    .collect::<Vec<_>>(),
                other => panic!("expected an applied subject, got {other:?}"),
            },
            other => panic!("expected Impl, got {other:?}"),
        };
        assert_eq!(
            binders("impl Pair<_, type T> { }"),
            vec![("_", 0), ("T", 0)]
        );
        // The bound `_` could not carry before: `_: Bound` stopped at the `:`.
        assert_eq!(
            binders("impl Pair<_: Show, _> { }"),
            vec![("_", 1), ("_", 0)]
        );
        // Two occurrences are two nodes, which is what makes them two
        // parameters downstream.
        assert_eq!(
            binders("impl Pair<type _, _> { }"),
            vec![("_", 0), ("_", 0)]
        );
    }

    #[test]
    fn a_bare_underscore_annotation_is_a_binder_node_not_a_name() {
        // B294: the reading is unconditional, so an inference-placeholder `_`
        // is refused by RESOLUTION (with a message that says what `_` is for)
        // rather than by the parser.
        match only_item("fun f(value: _) { }") {
            Node::Func(function) => {
                match &function.parameters.0[0].declared_type.as_ref().unwrap().0 {
                    Node::TypeBinder((name, name_span), bounds, _) => {
                        assert_eq!(*name, "_");
                        assert!(bounds.is_empty());
                        // The NAME's own span (E161), not the whole binder's.
                        assert_eq!(name_span.into_range().len(), 1);
                    }
                    other => panic!("expected a TypeBinder annotation, got {other:?}"),
                }
            }
            other => panic!("expected Func, got {other:?}"),
        }
    }

    #[test]
    fn trait_body_holds_declarations_and_default_members() {
        // A signature-only declaration and a defaulted method, plus a supertrait.
        match only_item(
            "trait Ord<T> with Eq { fun cmp(&self, other: &T): i32; fun max(&self): i32 { 0 } }",
        ) {
            Node::Trait(name, generics, supertraits, body, _) => {
                assert_eq!(name.0, "Ord");
                assert!(generics.is_some());
                assert_eq!(supertraits.len(), 1);
                assert_eq!(body.0.len(), 2);
                let bodies: Vec<bool> = body
                    .0
                    .iter()
                    .map(|member| match &member.0 {
                        Node::Func(function) => function.body.is_some(),
                        other => panic!("trait member is not a Func: {other:?}"),
                    })
                    .collect();
                assert_eq!(bodies, vec![false, true], "decl then default");
            }
            other => panic!("expected Trait, got {other:?}"),
        }
    }

    #[test]
    fn module_nests_items() {
        match only_item(
            "mod geometry { struct Point { x: i32 } fun origin(): Point { Point { x = 0 } } }",
        ) {
            Node::Module(name, body) => {
                assert_eq!(name, "geometry");
                assert_eq!(body.0.len(), 2);
            }
            other => panic!("expected Module, got {other:?}"),
        }
    }

    // --- S3 items: import / use / export -------------------------------------

    #[test]
    fn import_recursive_path_and_brace_set() {
        // `std::collections::{ Map, Set }` — a `::` path ending in a set.
        match only_item("import std::collections::{ Map, Set };") {
            Node::Import(ImportBranch::Path("std", _, ImportTail::Continue(next)), ..) => {
                match &*next {
                    ImportBranch::Path("collections", _, ImportTail::Continue(set)) => match &**set
                    {
                        ImportBranch::Set(members) => assert_eq!(members.len(), 2),
                        other => panic!("expected a Set continuation, got {other:?}"),
                    },
                    other => panic!("expected a nested path, got {other:?}"),
                }
            }
            other => panic!("expected Import(Path), got {other:?}"),
        }
    }

    #[test]
    fn use_bare_path_and_export_reexport() {
        assert!(matches!(
            only_item("use option::Some;"),
            Node::Use(ImportBranch::Path("option", _, ImportTail::Continue(_)))
        ));
        // `export import a::b;` — the inner import consumes its own `;`; the Export
        // wraps it (and its span, tested via the differential, includes the `;`).
        match only_item("export import shared::config;") {
            Node::Export(_, inner, _) => assert!(matches!(inner.0, Node::Import(..))),
            other => panic!("expected Export, got {other:?}"),
        }
    }

    #[test]
    fn top_level_let_and_mut_bindings() {
        assert!(matches!(only_item("let answer = 42;"), Node::Let(..)));
        assert!(matches!(only_item("mut total = 0;"), Node::Let(..)));
    }

    // --- S3 attribute / macro forms ------------------------------------------

    #[test]
    fn derive_and_service_attributes_wrap_their_item() {
        match only_item("[derive(Show, Eq)] struct P { x: i32 }") {
            Node::Derive(derives, item) => {
                let names: Vec<&str> = derives.iter().map(|(name, _)| *name).collect();
                assert_eq!(names, vec!["Show", "Eq"]);
                assert!(matches!(item.0, Node::Struct(..)));
            }
            other => panic!("expected Derive, got {other:?}"),
        }
        // `[service(Client)] struct` names its generated client type.
        match only_item("[service(RoomClient)] struct Room { }") {
            Node::Service(attribute, item) => {
                assert_eq!(attribute.client_name, Some("RoomClient"));
                assert_eq!(attribute.handler_name, None);
                assert!(attribute.server_side && !attribute.client_side);
                assert!(matches!(item.0, Node::Struct(..)));
            }
            other => panic!("expected Service, got {other:?}"),
        }
        // Bare `[service]` defaults the client name to `None`.
        match only_item("[service] struct Room { }") {
            Node::Service(attribute, _) => {
                assert_eq!(attribute.client_name, None);
                assert!(attribute.server_side && !attribute.client_side);
            }
            other => panic!("expected Service, got {other:?}"),
        }
    }

    #[test]
    fn the_http_marker_rides_the_service_node_in_any_position_but_first_of_a_name() {
        // A120 S5: `http` is a MARKER, not a client name — `[service(http)]`
        // keeps the default client name.
        for (source, client_name, handler_name) in [
            ("[service(http)] struct Door { }", None, None),
            (
                "[service(DoorClient, http)] struct Door { }",
                Some("DoorClient"),
                None,
            ),
            (
                "[service(DoorClient, http, client = Peer)] struct Door { }",
                Some("DoorClient"),
                Some("Peer"),
            ),
            (
                "[service(client = Peer, http)] struct Door { }",
                None,
                Some("Peer"),
            ),
        ] {
            match only_item(source) {
                Node::Service(attribute, _) => {
                    assert!(attribute.http, "{source}");
                    assert_eq!(attribute.client_name, client_name, "{source}");
                    assert_eq!(attribute.handler_name, handler_name, "{source}");
                }
                other => panic!("expected Service for {source}, got {other:?}"),
            }
        }
        match only_item("[service(DoorClient)] struct Door { }") {
            Node::Service(attribute, _) => assert!(!attribute.http),
            other => panic!("expected Service, got {other:?}"),
        }
    }

    #[test]
    fn client_service_attributes_ride_the_service_node() {
        // §9.3, R1: `client = H` is a NAMED argument beside the positional
        // client name, `[client_service]` is its own attribute, and a struct
        // carrying both is peer-to-peer on ONE node.
        match only_item("[service(RoomClient, client = RoomHandlers)] struct Room { }") {
            Node::Service(attribute, _) => {
                assert_eq!(attribute.client_name, Some("RoomClient"));
                assert_eq!(attribute.handler_name, Some("RoomHandlers"));
                assert!(attribute.server_side && !attribute.client_side);
            }
            other => panic!("expected Service, got {other:?}"),
        }
        // `client = H` alone, with the client name defaulted.
        match only_item("[service(client = RoomHandlers)] struct Room { }") {
            Node::Service(attribute, _) => {
                assert_eq!(attribute.client_name, None);
                assert_eq!(attribute.handler_name, Some("RoomHandlers"));
            }
            other => panic!("expected Service, got {other:?}"),
        }
        match only_item("[client_service] struct RoomHandlers { }") {
            Node::Service(attribute, item) => {
                assert!(!attribute.server_side && attribute.client_side);
                assert_eq!(attribute.client_name, None);
                assert!(matches!(item.0, Node::Struct(..)));
            }
            other => panic!("expected Service, got {other:?}"),
        }
        // Peer-to-peer: both attributes, either order, one node.
        for source in [
            "[service(PeerClient, client = Peer)] [client_service] struct Peer { }",
            "[client_service] [service(PeerClient, client = Peer)] struct Peer { }",
        ] {
            match only_item(source) {
                Node::Service(attribute, _) => {
                    assert!(attribute.server_side && attribute.client_side);
                    assert_eq!(attribute.client_name, Some("PeerClient"));
                    assert_eq!(attribute.handler_name, Some("Peer"));
                }
                other => panic!("expected Service, got {other:?}"),
            }
        }
    }

    #[test]
    fn function_attributes_are_recognized_in_fixed_order() {
        // The full ordered attribute prefix (`deprecated`, `extern`, `must_use`,
        // `rpc`, `trait_only`, `platform`) on one external function.
        // `[doc(hidden)]` used to sit between `trait_only` and `platform`; B318
        // retired it, and its slot in the order is refused rather than read.
        match only_item(
            "[deprecated(\"use serve_all()\")] [extern(\"node:http\", \"createServer\")] [must_use] [rpc] [trait_only] [platform(\"@process\")] external fun serve();",
        ) {
            Node::Func(function) => {
                assert!(matches!(
                    function.extern_binding,
                    Some(ExternBinding::Function {
                        module: Some("node:http"),
                        symbol: "createServer"
                    })
                ));
                assert!(function.must_use && function.rpc && function.trait_only);
                assert_eq!(function.deprecated, Some("use serve_all()"));
                assert_eq!(function.platform_fence.len(), 1);
                assert!(function.external);
            }
            other => panic!("expected a fully-attributed Func, got {other:?}"),
        }
    }

    /// The exported item under an `Export` with no scope and no re-export
    /// label, or a panic naming what came back instead.
    fn exported(node: Node<'_>) -> Node<'_> {
        match node {
            Node::Export(None, inner, None) => inner.0,
            other => panic!("expected a plain `export` of an item, got {other:?}"),
        }
    }

    #[test]
    fn attributes_written_before_export_are_the_exported_items_own() {
        // B445: the attribute run may stand on either side of `export`, and
        // either way it is the ITEM's prefix — the tree is the one `export
        // [..] item` builds. The issue's own program first.
        match exported(only_item(
            "[platform(\"browser\")] export impl P with Show { fun show(self): str { \"p\" } }",
        )) {
            Node::Impl(_, traits, _, Some(labels)) => {
                assert_eq!(traits.len(), 1);
                let patterns: Vec<&str> = labels
                    .platform
                    .iter()
                    .map(|(pattern, _)| *pattern)
                    .collect();
                assert_eq!(patterns, ["browser"]);
            }
            other => panic!("expected a labelled Impl, got {other:?}"),
        }
        // A RUN, in the function prefix's order, before a scoped export.
        match only_item("[deprecated(\"use g()\")] [must_use] export(in pkg) fun f(): i32 { 1 }") {
            Node::Export(Some(scope), inner, None) => {
                assert_eq!(scope.path.len(), 1);
                match inner.0 {
                    Node::Func(function) => {
                        assert_eq!(function.deprecated, Some("use g()"));
                        assert!(function.must_use);
                    }
                    other => panic!("expected a Func, got {other:?}"),
                }
            }
            other => panic!("expected a scoped Export, got {other:?}"),
        }
        // A wrapper attribute: the derive still wraps the struct.
        match exported(only_item("[derive(Debug)] export struct S { x: i32 }")) {
            Node::Derive(names, inner) => {
                assert_eq!(names[0].0, "Debug");
                assert!(matches!(inner.0, Node::Struct(..)));
            }
            other => panic!("expected a Derive, got {other:?}"),
        }
        // A module binding's label.
        match exported(only_item("[internal(\"why\")] export let x = 1;")) {
            Node::Let(_, _, _, _, _, Some(labels)) => assert_eq!(labels.internal, Some("why")),
            other => panic!("expected a labelled Let, got {other:?}"),
        }
        // `[resource]` closes the type's prefix from either side.
        match exported(only_item("[resource] export struct Handle { id: i32 }")) {
            Node::Struct(_, _, external, resource, ..) => assert!(resource && !external),
            other => panic!("expected a resource Struct, got {other:?}"),
        }
        // Both sides at once: refused since B485 S3 (an attribute after the
        // marker), and still read as one prefix, in its order.
        let source = "[deprecated(\"use g()\")] export [platform(\"node\")] fun f(): i32 { 1 }";
        let (tree, errors) = parse(source);
        assert_eq!(
            errors.iter().map(render).collect::<Vec<_>>(),
            vec![marker_order_rule(
                "[deprecated(..)] [platform(..)] export fun"
            )]
        );
        let (mut statements, _) = tree.expect("a tree");
        match exported(statements.remove(0).0) {
            Node::Func(function) => {
                assert_eq!(function.deprecated, Some("use g()"));
                assert_eq!(function.platform_fence.len(), 1);
            }
            other => panic!("expected a Func, got {other:?}"),
        }
        // B382's re-export label, written ahead of the marker: still the
        // EXPORT's (it deprecates the name the re-export publishes).
        match only_item("[deprecated(\"use a::c\")] export import a::b;") {
            Node::Export(None, inner, Some(labels)) => {
                assert!(matches!(inner.0, Node::Import(..)));
                assert_eq!(labels.deprecated, Some("use a::c"));
            }
            other => panic!("expected a labelled re-export, got {other:?}"),
        }
    }

    #[test]
    fn an_export_written_after_its_attributes_spans_them() {
        // The statement begins at the attribute: a diagnostic about the whole
        // export, a folding range and the formatter's comment placement all
        // read the Export's span, and it must cover what the author wrote.
        let source = "[platform(\"node\")] export fun f(): i32 { 1 }";
        let (statements, _) = program(source);
        assert_eq!(statements[0].1.start, 0);
        assert_eq!(statements[0].1.end, source.len());
        // Two statements in a row, the second attributed ahead of its marker.
        let source = "export fun a() {}\n[must_use] export fun b(): i32 { 1 }\nfun c() {}";
        let (statements, _) = program(source);
        assert_eq!(statements.len(), 3);
        assert_eq!(
            &source[statements[1].1.start..statements[1].1.end],
            "[must_use] export fun b(): i32 { 1 }"
        );
    }

    /// The rendered first error of `source` and the text its span covers.
    fn first_error(source: &str) -> (String, &str) {
        let (_, errors) = parse(source);
        let error = errors
            .first()
            .unwrap_or_else(|| panic!("{source:?} parsed clean"));
        (render(error), &source[error.span.start..error.span.end])
    }

    #[test]
    fn b446_a_parameter_that_is_no_binder_is_reported_where_it_is_written() {
        // A parameter NAMED `own` (or `lazy`) after a generic-typed one is a
        // name: `:` begins no binder, so the word is not the convention.
        program("fun f(a: Shared<List<Foo>>, own: i32, lazy: i32) {}");
        program("fun f(xs: List<i32>, own: i32) {}");
        // One `.` is not a spread's three: `own.x` is the name `own` and a
        // stray member access, reported at the `.` — not the convention, a
        // failed binder, and "found '>' expected ','" one parameter early.
        for source in [
            "fun f(a: List<i32>, own.x: i32) {}",
            "fun f(a: List<i32>, lazy.x: i32) {}",
            "fun f(a: i32, own.x: i32) {}",
        ] {
            let (message, at) = first_error(source);
            assert_eq!(at, ".", "{source}: {message}");
            // The NAME reading: the parameter ended at `own`, and the list
            // wanted its next separator — not "expected a name", which is
            // the convention reading's binder failing on the same `.`.
            assert!(
                message.starts_with("found '.' expected ',' or ')'"),
                "{source}: {message}"
            );
        }
        // A binder that is not one is reported AT it, whatever led it in:
        // nothing, a convention, `mut`, a view, a spread, a destructure.
        for (source, offending) in [
            ("fun f(a: List<i32>, 5) {}", "5"),
            ("fun f(a: List<List<i32>>, own (5)) {}", "5"),
            ("fun f(a: List<i32>, own [1]) {}", "1"),
            ("fun f(a: List<i32>, mut 5) {}", "5"),
            ("fun f(a: List<i32>, & 5) {}", "5"),
            ("fun f(a: List<i32>, ...5) {}", "5"),
            ("fun f(a: List<i32>, lazy (1)) {}", "1"),
            ("fun g() { let 5 = 1; }", "5"),
            ("fun g() { let (5, a) = (1, 2); }", "5"),
        ] {
            let (message, at) = first_error(source);
            assert_eq!(at, offending, "{source}: {message}");
            assert!(message.contains("expected a name"), "{source}: {message}");
        }
        // At a parameter's head, with nothing of it read, the list's `)`
        // would have done as well, and the report says so.
        let (message, at) = first_error("fun broken( {");
        assert_eq!(
            (message.as_str(), at),
            ("found '{' expected a name or ')'", "{")
        );
        let (message, _) = first_error("fun f(a: List<i32>, 5) {}");
        assert_eq!(message, "found '5' expected a name or ')'");
        // After a convention the parameter is under way: a name alone.
        let (message, _) = first_error("fun f(a: List<i32>, mut 5) {}");
        assert_eq!(message, "found '5' expected a name");
        // The spread is still the convention's business, and still refused.
        let (message, _) = first_error("fun f(a: List<i32>, own ...items: (i32, i32)) {}");
        assert!(
            message.starts_with("a spread parameter receives a tuple"),
            "{message}"
        );
    }

    #[test]
    fn b492_a_repeated_export_is_refused_and_read_past() {
        // (source, the text each refusal spans): one refusal per RUN of
        // repeated markers, wherever the repeat stands.
        for (source, refused) in [
            ("export export fun f() {}", vec!["export"]),
            ("export export export let x = 1;", vec!["export export"]),
            (
                "[platform(\"node\")] export export fun f() {}",
                vec!["export"],
            ),
            ("export(in pkg) export struct S {}", vec!["export"]),
            ("export export(in pkg) struct S {}", vec!["export"]),
            (
                "[must_use] export(in pkg) export fun f(): i32 { 1 }",
                vec!["export"],
            ),
            // Separated by an attribute run: the run is the item's.
            (
                "export [must_use] export fun f(): i32 { 1 }",
                vec!["export"],
            ),
            (
                "[deprecated(\"x\")] export [must_use] export fun f(): i32 { 1 }",
                vec!["export"],
            ),
            (
                "export [must_use] export [platform(\"node\")] export fun f() {}",
                vec!["export", "export"],
            ),
        ] {
            let (tree, errors) = parse(source);
            let rendered: Vec<String> = errors.iter().map(render).collect();
            let spans: Vec<&str> = errors
                .iter()
                .map(|error| &source[error.span.start..error.span.end])
                .collect();
            assert_eq!(spans, refused, "{source}: {rendered:?}");
            for message in &rendered {
                assert_eq!(message, EXPORT_IS_WRITTEN_ONCE, "{source}");
            }
            // One export, of the declaration: not an export of an export.
            let (statements, _) = tree.expect("a tree");
            match &statements[0].0 {
                Node::Export(_, inner, _) => {
                    assert!(!matches!(inner.0, Node::Export(..)), "{source}: {inner:?}")
                }
                other => panic!("{source}: expected an Export, got {other:?}"),
            }
        }
        // The attributes on either side of a repeat are the item's prefix.
        let (tree, _) = parse("[deprecated(\"x\")] export [must_use] export fun f(): i32 { 1 }");
        match &tree.expect("a tree").0[0].0 {
            Node::Export(_, inner, _) => match &inner.0 {
                Node::Func(function) => {
                    assert_eq!(function.deprecated, Some("x"));
                    assert!(function.must_use);
                }
                other => panic!("expected a Func, got {other:?}"),
            },
            other => panic!("expected an Export, got {other:?}"),
        }
        // The first marker is not refused, and `export *;` is untouched.
        program("export fun f() {}");
        program("export *;");
    }

    #[test]
    fn a_list_before_export_is_not_an_attribute_run() {
        // Only an ATTRIBUTE shape (`[` then a name) leads a marker; a list
        // literal there is the expression statement it always was, refused
        // for its missing `;`.
        assert!(declines("[1] export fun f() {}"));
        // An attribute run before `export *;` is not an item's prefix.
        assert!(declines("[platform(\"node\")] export *;"));
    }

    #[test]
    fn function_attributes_in_any_order_read_as_the_canonical_prefix() {
        // B485 Q7 (RULED): the run is sorted before the production reads it,
        // so another order is the function the canonical order spells,
        // attribute for attribute — refused since v0.45.0 (B536), and still
        // read so, which is what lets `vilan fmt` write the migration.
        for (written, canonical) in [
            (
                "[rpc] [must_use] fun f() { }",
                "[must_use] [rpc] fun f() { }",
            ),
            (
                "[extern(\"fs\", \"read\")] [deprecated(\"use read_all()\")] external fun read();",
                "[deprecated(\"use read_all()\")] [extern(\"fs\", \"read\")] external fun read();",
            ),
        ] {
            assert_eq!(
                attributes_of(written),
                attributes_of(canonical),
                "{written}"
            );
        }
    }

    /// The single top-level item of `source`, which parses clean or carries
    /// exactly one refusal of its attribute ORDER (B536): such a head is read
    /// as if written in the order, so the item is the canonical one's.
    fn only_item_read_in_order(source: &str) -> Node<'_> {
        let (tree, errors) = parse(source);
        assert!(
            errors.len() <= 1
                && errors
                    .iter()
                    .all(|error| matches!(error.reason, ParseErrorReason::AttributeOrder { .. })),
            "parse errors on {source:?}: {errors:?}"
        );
        let (mut statements, _) = tree.expect("program did not parse");
        assert_eq!(statements.len(), 1, "expected one item in {source:?}");
        statements.remove(0).0
    }

    /// The attribute fields of the one `fun` in `source`, which must parse
    /// clean but for its attribute order (refused since v0.45.0, B536, and read
    /// in it) — what a reordered prefix has to agree on with the canonical one.
    fn attributes_of(source: &str) -> String {
        match only_item_read_in_order(source) {
            Node::Func(function) => format!(
                "{:?} {:?} {:?} {} {} {} {:?}",
                function.deprecated,
                function.internal,
                function.extern_binding,
                function.must_use,
                function.rpc,
                function.trait_only,
                function
                    .platform_fence
                    .iter()
                    .map(|(pattern, _)| *pattern)
                    .collect::<Vec<_>>(),
            ),
            other => panic!("expected a Func, got {other:?}"),
        }
    }

    #[test]
    fn a_deprecated_attribute_carries_its_steer() {
        // The ordinary shape: one quoted steer, flattened into the `Func` field
        // (proposal/deprecation.md §2).
        match only_item("[deprecated(\"use two()\")] fun one() { }") {
            Node::Func(function) => assert_eq!(function.deprecated, Some("use two()")),
            other => panic!("expected a deprecated Func, got {other:?}"),
        }
        // Without the attribute the field is empty.
        match only_item("fun plain() { }") {
            Node::Func(function) => assert_eq!(function.deprecated, None),
            other => panic!("expected a Func, got {other:?}"),
        }
        // The steer is REQUIRED — a bare `[deprecated]` marker is not the
        // attribute (the warning's head needs its replacement clause), and
        // `deprecated` is a known marker, so no user-macro reading claims it
        // either: the program declines.
        assert!(declines("[deprecated] fun one() { }"));
    }

    #[test]
    fn an_internal_attribute_carries_its_reason_on_a_function_and_on_a_field() {
        // E213. The shape is `[deprecated(..)]`'s, deliberately: both are
        // labels ABOUT the declaration rather than parts of its signature.
        match only_item("[internal(\"row bookkeeping\")] fun cut_row() { }") {
            Node::Func(function) => assert_eq!(function.internal, Some("row bookkeeping")),
            other => panic!("expected a labelled Func, got {other:?}"),
        }
        match only_item("fun plain() { }") {
            Node::Func(function) => assert_eq!(function.internal, None),
            other => panic!("expected a Func, got {other:?}"),
        }
        // A FIELD is the case declaration visibility cannot serve at all.
        match only_item("struct Region { [internal(\"the end marker\")] anchor: str, label: str }")
        {
            Node::Struct(_, _, _, _, Some(fields), _) => {
                assert_eq!(fields.0[0].0.3, Some("the end marker"));
                assert_eq!(fields.0[1].0.3, None);
            }
            other => panic!("expected a Struct, got {other:?}"),
        }
        // The reason is REQUIRED — a bare `[internal]` is not the attribute,
        // and `internal` is a known marker, so no user-macro reading claims it
        // either: the program declines.
        assert!(declines("[internal] fun one() { }"));
        // It follows `[deprecated(..)]` in the canonical prefix and precedes
        // `[extern(..)]`; written in the other order it reads the same (B485
        // Q7), where it used to decline.
        assert!(matches!(
            only_item("[deprecated(\"use two()\")] [internal(\"seam\")] fun one() { }"),
            Node::Func(_)
        ));
        assert_eq!(
            attributes_of("[extern(\"fs\", \"read\")] [internal(\"seam\")] external fun read();"),
            attributes_of("[internal(\"seam\")] [extern(\"fs\", \"read\")] external fun read();"),
        );
    }

    #[test]
    fn a_deprecated_steer_rides_a_type_and_a_re_export() {
        // B382: the function attribute, admitted on the nominals and a trait —
        // leading the ordered prefix, as it leads a function's.
        fn steer(source: &str) -> Option<&str> {
            match only_item_read_in_order(source) {
                Node::Struct(.., labels) | Node::Enum(.., labels) | Node::Trait(.., labels) => {
                    labels.and_then(|labels| labels.deprecated)
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(steer("[deprecated(\"use B\")] struct A {}"), Some("use B"));
        assert_eq!(steer("[deprecated(\"use B\")] enum A { X }"), Some("use B"));
        assert_eq!(
            steer("[deprecated(\"use B\")] trait A { fun f(self); }"),
            Some("use B")
        );
        assert_eq!(
            steer("[deprecated(\"use B\")] [internal(\"x\")] struct A {}"),
            Some("use B")
        );
        // …and on an `export import`, the re-export the ruling names.
        match only_item("[deprecated(\"use D\")] export import pkg::a::D as K;") {
            Node::Export(_, inner, Some(labels)) => {
                assert_eq!(labels.deprecated, Some("use D"));
                assert!(matches!(&inner.0, Node::Import(..)));
            }
            other => panic!("{other:?}"),
        }
        // Without the `export` the steer publishes nothing: refused, and the
        // import itself still parses.
        let (tree, errors) = parse("[deprecated(\"use D\")] import pkg::a::D;\n");
        assert!(
            errors
                .iter()
                .any(|error| render(error) == DEPRECATED_IMPORT_IS_A_RE_EXPORT),
            "{errors:?}"
        );
        assert!(matches!(tree.expect("a tree").0[0].0, Node::Import(..)));
        // Either order is the one prefix (B485 Q7): `[internal]` written
        // before `[deprecated]` used to decline.
        assert_eq!(
            steer("[internal(\"x\")] [deprecated(\"use B\")] struct A {}"),
            Some("use B")
        );
    }

    #[test]
    fn an_impl_binder_takes_a_tuple_family_bound() {
        // A122 (tuple-module.md §4.1): `impl type T: (2..) with Tuple` — the
        // binder's bound is a tuple bound, tried before the trait-bound list
        // as a generic parameter's is.
        match only_item("impl type T: (2..) with Tuple { }") {
            Node::Impl(subject, traits, _, _) => {
                assert_eq!(traits.len(), 1);
                match &subject.0 {
                    Node::TypeBinder((name, _), bounds, Some(tuple_bound)) => {
                        assert_eq!(*name, "T");
                        assert!(bounds.is_empty());
                        assert_eq!((tuple_bound.lo, tuple_bound.hi), (Some(2), None));
                        assert!(tuple_bound.element.is_none());
                    }
                    other => panic!("expected a tuple-bounded binder, got {other:?}"),
                }
            }
            other => panic!("expected an impl, got {other:?}"),
        }
        // The anonymous binder and an element bound take it too.
        match only_item("impl _: (2..4: Display) with Tuple { }") {
            Node::Impl(subject, ..) => assert!(matches!(
                &subject.0,
                Node::TypeBinder(_, _, Some(bound)) if bound.hi == Some(4) && bound.element.is_some()
            )),
            other => panic!("{other:?}"),
        }
        // A trait bound is still a trait bound.
        match only_item("impl type T: Display with Show { }") {
            Node::Impl(subject, ..) => {
                assert!(
                    matches!(&subject.0, Node::TypeBinder(_, bounds, None) if bounds.len() == 1)
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_file_leading_mod_self_hosts_the_files_platform() {
        // B415: `[platform("…")] mod self;` as the file's first statement is
        // the host for the file's own attributes (F27 R1's platform).
        let items = program("[platform(\"browser\")] mod self;\n\nimport std::web::ui::Region;\n");
        match &items.0[0].0 {
            Node::ModulePlatform(patterns) => {
                assert_eq!(
                    patterns.iter().map(|(text, _)| *text).collect::<Vec<_>>(),
                    vec!["browser"]
                );
            }
            other => panic!("expected the module's platform, got {other:?}"),
        }
        match only_item("[platform(\"@process\", \"browser\")] mod self;") {
            Node::ModulePlatform(patterns) => assert_eq!(patterns.len(), 2),
            other => panic!("{other:?}"),
        }
        // A host with no attribute on it declares nothing, and parses.
        match only_item("mod self;") {
            Node::ModulePlatform(patterns) => assert!(patterns.is_empty()),
            other => panic!("{other:?}"),
        }
        // The attribute on a first function is that function's fence, as it
        // always was.
        match only_item("[platform(\"browser\")] fun f() {}") {
            Node::Func(function) => assert_eq!(function.platform_fence.len(), 1),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_bare_file_platform_statement_is_gone() {
        // B415 removes F27 R1's `[platform("…")];`: `mod self;` is its host,
        // and the attribute with only a `;` after it is no statement at all.
        assert!(
            !matches!(
                program("[platform(\"browser\")];\n")
                    .0
                    .first()
                    .map(|item| &item.0),
                Some(Node::ModulePlatform(_))
            ),
            "the bare form must not parse as the file's platform"
        );
    }

    #[test]
    fn mod_self_anywhere_but_the_files_head_is_refused() {
        for source in [
            "import std::web::ui::Region;\n[platform(\"browser\")] mod self;\n",
            "import std::web::ui::Region;\nmod self;\n",
            "fun f() {\n\t[platform(\"browser\")] mod self;\n}\n",
            "mod inner {\n\t[platform(\"browser\")] mod self;\n}\n",
        ] {
            let (_, errors) = parse(source);
            assert!(
                errors
                    .iter()
                    .any(|error| render(error) == MODULE_SELF_LEADS_THE_FILE),
                "{source:?}: {errors:?}"
            );
        }
        // The head itself is clean, a leading comment notwithstanding.
        assert!(!declines(
            "// the client's slot\n[platform(\"browser\")] mod self;\nfun f() {}\n"
        ));
    }

    #[test]
    fn self_is_a_reserved_module_name() {
        // B415: `self` names the file's own module, so no module may be
        // declared by that name — at the head or anywhere else.
        for source in [
            "mod self {\n\tfun f() {}\n}\n",
            "fun g() {}\nmod self {}\n",
            "mod outer {\n\tmod self {}\n}\n",
        ] {
            let (_, errors) = parse(source);
            assert!(
                errors
                    .iter()
                    .any(|error| render(error) == MODULE_SELF_IS_RESERVED),
                "{source:?}: {errors:?}"
            );
        }
        // Any other name is a module, as it always was.
        assert!(!declines("mod selfish {\n\tfun f() {}\n}\n"));
    }

    #[test]
    fn an_impl_and_a_nominal_carry_a_platform_label() {
        match only_item("[platform(\"browser\")] impl Region { fun f(self) {} }") {
            Node::Impl(_, _, _, Some(labels)) => assert_eq!(labels.platform.len(), 1),
            other => panic!("{other:?}"),
        }
        match only_item("[internal(\"x\")] [platform(\"browser\")] struct Slot {}") {
            Node::Struct(.., Some(labels)) => {
                assert_eq!(labels.internal, Some("x"));
                assert_eq!(labels.platform[0].0, "browser");
            }
            other => panic!("{other:?}"),
        }
        match only_item("impl Region { fun f(self) {} }") {
            Node::Impl(_, _, _, None) => {}
            other => panic!("an unlabelled impl carries none: {other:?}"),
        }
    }

    #[test]
    fn a_hint_label_carries_its_type_and_sits_between_internal_and_platform() {
        // E227: the argument is parsed as a TYPE; every occurrence is kept
        // (a second is the analyzer's refusal, not a parse error).
        fn hints_of(source: &str) -> Vec<String> {
            match only_item(source) {
                Node::Struct(.., labels) | Node::Enum(.., labels) => labels
                    .map(|labels| {
                        labels
                            .hint
                            .iter()
                            .map(|hint| source[hint.0.1.into_range()].to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
                other => panic!("expected a labelled declaration, got {other:?}"),
            }
        }
        assert_eq!(
            hints_of("[hint(Source<U>)] struct Map<S, T, U> { up: S }"),
            vec!["Source<U>"]
        );
        assert_eq!(
            hints_of(
                "[internal(\"n\")] [hint(Iterator<(usize, T)>)] [platform(\"browser\")] [resource] struct E<I, T> { up: I }"
            ),
            vec!["Iterator<(usize, T)>"]
        );
        assert_eq!(
            hints_of("[hint(A<T>)] [hint(B<T>)] enum Two<T> { One(T) }"),
            vec!["A<T>", "B<T>"]
        );
        assert!(hints_of("struct Plain {}").is_empty());
        // Still an ordinary name everywhere else.
        match only_item("let hint = 3;") {
            Node::Let(..) => {}
            other => panic!("`hint` is a plain binder: {other:?}"),
        }
    }

    #[test]
    fn an_internal_label_rides_every_e221_position() {
        // E221: the nominals, a variant, a trait and a module binding.
        fn label_of(source: &str) -> Option<&str> {
            match only_item(source) {
                Node::Struct(.., labels)
                | Node::Enum(.., labels)
                | Node::Trait(.., labels)
                | Node::Let(.., labels) => labels.and_then(|labels| labels.internal),
                other => panic!("expected a labelled declaration, got {other:?}"),
            }
        }
        assert_eq!(label_of("[internal(\"s\")] struct Region {}"), Some("s"));
        assert_eq!(
            label_of("[internal(\"r\")] [resource] struct Handle { id: i32 }"),
            Some("r")
        );
        assert_eq!(label_of("[internal(\"e\")] enum Side { Left }"), Some("e"));
        assert_eq!(
            label_of("[internal(\"t\")] trait Seam { fun seam(self); }"),
            Some("t")
        );
        assert_eq!(label_of("[internal(\"b\")] let cache = 3;"), Some("b"));
        assert_eq!(label_of("[internal(\"l\")] lazy let db = 3;"), Some("l"));
        assert_eq!(label_of("[internal(\"m\")] mut counter = 0;"), Some("m"));
        // Unlabelled, each is the one null pointer it was.
        assert_eq!(label_of("struct Region {}"), None);
        assert_eq!(label_of("let cache = 3;"), None);
        // A variant, on the variant's own record.
        match only_item("enum Side { Left, [internal(\"v\")] Auto }") {
            Node::Enum(_, _, _, variants, None) => {
                assert_eq!(variants.0[0].0.3, None);
                assert_eq!(variants.0[1].0.3, Some("v"));
            }
            other => panic!("expected an Enum, got {other:?}"),
        }
        // Behind `export`, and after a `[derive(..)]`, as a function's is.
        match only_item("[internal(\"x\")] export struct Marker {}") {
            Node::Export(_, inner, _) => {
                assert!(
                    matches!(&inner.0, Node::Struct(.., Some(labels)) if labels.internal == Some("x"))
                )
            }
            other => panic!("expected an Export, got {other:?}"),
        }
        match only_item("[derive(Clone)] [internal(\"d\")] struct Point { x: i32 }") {
            Node::Derive(_, inner) => {
                assert!(
                    matches!(&inner.0, Node::Struct(.., Some(labels)) if labels.internal == Some("d"))
                )
            }
            other => panic!("expected a Derive, got {other:?}"),
        }
        // A bare `[internal]` is not the label on any of them.
        assert!(declines("[internal] struct Region {}"));
        assert!(declines("[internal] let cache = 3;"));
        // And a `let` destructuring several names takes none.
        assert!(declines("[internal(\"p\")] let (a, b) = (1, 2);"));
    }

    #[test]
    fn bracket_attribute_vs_list_literal_disambiguation() {
        // `[must_use] fun` is a function (the list-literal expression reading fails
        // for want of a `;`, then `function` claims it).
        assert!(matches!(only_item("[must_use] fun f() { }"), Node::Func(_)));
        // A genuine list-literal statement (`[a, b];`) is still an expression.
        assert!(matches!(only_item("[a, b];"), Node::List(_)));
        // A user macro attribute (name NOT a known marker) wraps its item.
        match only_item("[route(\"/\", get)] fun index() { }") {
            Node::MacroAttribute(name, _, arguments, item) => {
                assert_eq!(name, "route");
                assert_eq!(arguments.len(), 2, "argument SPANS");
                assert!(matches!(item.0, Node::Func(_)));
            }
            other => panic!("expected MacroAttribute, got {other:?}"),
        }
        // A bare user attribute with no args wraps too.
        assert!(matches!(
            only_item("[handler] struct H { }"),
            Node::MacroAttribute("handler", _, _, _)
        ));
    }

    #[test]
    fn macro_definition_block_and_invocation_forms() {
        // `macro fun` is a definition.
        assert!(matches!(
            only_item("macro fun expand(): Source { source(\"\") }"),
            Node::MacroFun(_)
        ));
        // `macro { .. }` is a block; the statement `;` is optional (both parse).
        assert!(matches!(
            only_item("macro { ret void }"),
            Node::MacroBlock(_)
        ));
        assert!(matches!(
            only_item("macro { ret void };"),
            Node::MacroBlock(_)
        ));
        // `macro name(args)` is an invocation, `;` optional; arguments are SPANS.
        match only_item("macro define(a, b + 1)") {
            Node::MacroInvocation(name, _, arguments) => {
                assert_eq!(name, "define");
                assert_eq!(arguments.len(), 2);
            }
            other => panic!("expected MacroInvocation, got {other:?}"),
        }
        assert!(matches!(
            only_item("macro define(a);"),
            Node::MacroInvocation(..)
        ));
    }

    #[test]
    fn macro_invocation_in_expression_position_is_an_atom() {
        // Anywhere but statement head, `macro name(args)` is an expression atom.
        match &expr("x + macro pick(a)").0 {
            Node::Binary(BinaryOp::Add, _, right) => {
                assert!(matches!(right.0, Node::MacroInvocation(..)));
            }
            other => panic!("expected a macro invocation operand, got {other:?}"),
        }
    }

    #[test]
    fn tuple_comprehension_atom_parses() {
        // `(x in xs => e)` — the deferred S2 atom, now live.
        match &expr("(x in items => x + 1)").0 {
            Node::TupleComprehension { bindings, .. } => assert_eq!(bindings[0].binder, "x"),
            other => panic!("expected TupleComprehension, got {other:?}"),
        }
        // The `in` is what forks it from a group / tuple — `(a + b)` still dissolves.
        assert!(matches!(
            expr("(a + b)").0,
            Node::Binary(BinaryOp::Add, _, _)
        ));
    }

    // --- S3 statement interleaving + the resource steer ----------------------

    #[test]
    fn misplaced_resource_declines_but_a_valid_resource_declaration_parses() {
        // `[resource]` before a non-declaration is the steer (an error) —
        // declines (B413: the attribute, as the keyword was before it).
        assert!(declines("[resource] fun f() { }"));
        assert!(declines("[resource] impl Foo { }"));
        // But `[resource] struct` / `[resource] external struct` /
        // `[resource] enum` are valid and parse cleanly (the steer never
        // shadows them).
        assert!(matches!(
            only_item("[resource] struct File { }"),
            Node::Struct(_, _, false, true, _, _)
        ));
        assert!(matches!(
            only_item("[resource] enum State { A, B }"),
            Node::Enum(_, _, true, _, _)
        ));
        // After the labels, in the prefix's order, and before `external`.
        assert!(matches!(
            only_item("[internal(\"r\")] [resource] external struct Db;"),
            Node::Struct(_, _, true, true, _, Some(_))
        ));
    }

    #[test]
    fn the_resource_attribute_is_refused_on_everything_but_a_struct_an_enum_or_a_trait() {
        // B413: one rule, at every other position — an item, a local, a field
        // and a variant. B470 added the trait (its objects may hold a resource).
        let (_, errors) = parse("[resource] trait Foo {}\n");
        assert!(
            errors.is_empty(),
            "{:?}",
            errors.iter().map(render).collect::<Vec<_>>()
        );
        for source in [
            "[resource] fun f() {}\n",
            "[resource] impl Foo {}\n",
            "fun main() {\n\t[resource] let x = 1;\n}\n",
            "struct S {\n\t[resource] handle: i32,\n}\n",
            "enum E {\n\t[resource] Open,\n}\n",
        ] {
            let (_, errors) = parse(source);
            let rendered: Vec<String> = errors.iter().map(render).collect();
            assert!(
                rendered.contains(
                    &"`[resource]` marks a type as a resource: it may label only a `struct`, \
                      an `enum` or a `trait` declaration"
                        .to_string()
                ),
                "{source:?}: {rendered:?}"
            );
        }
    }

    #[test]
    fn the_retired_resource_keyword_is_refused_with_the_attribute_steer() {
        // B413: `resource struct` / `resource external struct` / `resource
        // enum` — the spelling before the keyword dissolved — is ONE refusal
        // naming the attribute, and the declaration still parses under it.
        for source in [
            "resource struct File { fd: i32 }\n",
            "resource external struct Database;\n",
            "resource enum State { A, B }\n",
            "export resource struct File {}\n",
        ] {
            let (_, errors) = parse(source);
            assert_eq!(
                errors.iter().map(render).collect::<Vec<_>>(),
                vec![
                    "`resource` is an attribute, not a keyword: write `[resource]` before the \
                     declaration (`[resource] struct`, `[resource] external struct`, \
                     `[resource] enum`)"
                        .to_string()
                ],
                "{source:?}"
            );
        }
    }

    #[test]
    fn resource_is_an_ordinary_name_now() {
        // B413: the word is no longer reserved — a binding, a field, a
        // function and a member read may all be called `resource`.
        assert!(!declines("fun main() {\n\tlet resource = 1;\n}\n"));
        assert!(!declines("struct Lease {\n\tresource: i32,\n}\n"));
        assert!(!declines("fun resource(): i32 {\n\t1\n}\n"));
        assert!(!declines("fun f(x: Lease) {\n\tlet _ = x.resource;\n}\n"));
    }

    #[test]
    fn a_block_bearing_form_is_a_statement_only_when_not_block_last() {
        // Inside a module body, `fun a` then `fun b` — two statements, no trailing
        // expression (an item body has none).
        match only_item("mod m { fun a() { } fun b() { } }") {
            Node::Module(_, body) => assert_eq!(body.0.len(), 2),
            other => panic!("expected Module, got {other:?}"),
        }
    }

    #[test]
    fn a_whole_file_is_a_sequence_of_items() {
        let (statements, _span) = program(
            "import std::io;\n\
             struct Point { x: i32, y: i32 }\n\
             [derive(Show)] enum Dir { N, S }\n\
             fun main() { print(\"hi\") }\n",
        );
        assert_eq!(statements.len(), 4);
        assert!(matches!(statements[0].0, Node::Import(..)));
        assert!(matches!(statements[1].0, Node::Struct(..)));
        assert!(matches!(statements[2].0, Node::Derive(..)));
        assert!(matches!(statements[3].0, Node::Func(_)));
    }

    // --- S4 recovery + error rendering (durable — no chumsky, survives S5) ----
    //
    // `parser_recovery.rs` pins the recovered SHAPES against BOTH frontends and
    // `parse_recovery_differential.rs` pins byte-equality with the oracle; these
    // pins are the handwritten frontend's OWN durable corpus for the recovered
    // trees, the parse contract, and — the part no other target covers — the
    // rendered messages.

    /// Parse `source` and render every error, for the message pins.
    fn rendered_errors(source: &str) -> Vec<String> {
        let (_tree, errors) = parse(source);
        errors.iter().map(render).collect()
    }

    #[test]
    fn a_clean_source_reports_no_errors_and_a_broken_one_does() {
        let (tree, errors) = parse("fun main() { }\n");
        assert!(tree.is_some() && errors.is_empty(), "clean: {errors:?}");
        let (tree, errors) = parse("struct S { 1 2 3 }\n");
        assert!(tree.is_some(), "recovery always returns a tree");
        assert!(!errors.is_empty(), "a recovered source reports");
    }

    #[test]
    fn the_ten_delimiter_sites_recover_to_their_exact_placeholders() {
        // The exact recovered shape at each of the ten sites (durable counterpart
        // of the cross-frontend `parser_recovery.rs` substring pins).
        let cases: &[(&str, &str)] = &[
            (
                "fun f<1 2 3>() {}\n",
                "generic_parameters: Some(([], 5..12)",
            ),
            (
                "fun f(x: List<1 2 3>) {}\n",
                "AccessorWithGenerics(\"List\", ([], 13..20)",
            ),
            (
                "fun main() { let p = Point { 1 2 3 }; }\n",
                "StructInitializer([], (\"Point\", 21..26), None, ([], 27..36)",
            ),
            ("fun main() { let x = (1 +); }\n", "Some((Error, 21..26))"),
            ("fun main() { let x = [1 +]; }\n", "Some((Error, 21..26))"),
            (
                // The synthesized tail `Void` carries the CLOSING BRACE's span
                // (editing-dx.md §16's S3 anchor rule), not a zero-width point
                // past it — composed with S1's recovery, which declines the
                // broken statement and leaves the body empty.
                "fun main() { let x = 1 + ; }\n",
                "body: Some((([], (Void, 27..28)",
            ),
            (
                "struct S { 1 2 3 }\n",
                "Struct((\"S\", 7..8), None, false, false, Some(([], 9..18))",
            ),
            (
                "impl Foo { 1 2 3 }\nfun after() {}\n",
                "Impl((Accessor(\"Foo\"), 5..8), [], ([], 9..18), None)",
            ),
            (
                "trait Foo { 1 2 3 }\nfun after() {}\n",
                "Trait((\"Foo\", 6..9), None, [], ([], 10..19), None)",
            ),
            (
                "mod foo { 1 2 3 }\nfun after() {}\n",
                "Module(\"foo\", ([], 8..17))",
            ),
        ];
        for (source, shape) in cases {
            let (tree, errors) = parse(source);
            let tree = format!("{:?}", tree.expect("a tree comes back"));
            assert!(tree.contains(shape), "{source:?} → {tree}");
            assert!(!errors.is_empty(), "{source:?} must report");
        }
    }

    #[test]
    fn render_names_the_unclosed_delimiter_and_its_production() {
        assert_eq!(
            rendered_errors("fun f<1 2 3>() {}\n"),
            vec!["unclosed `<` in generic parameters: expected a matching `>`".to_string()]
        );
        assert_eq!(
            rendered_errors("struct S { 1 2 3 }\n"),
            vec!["unclosed `{` in struct body: expected a matching `}`".to_string()]
        );
    }

    #[test]
    fn render_states_the_resource_language_rule() {
        // diagnostics-standard.md B6 — the prohibition explains itself.
        assert_eq!(
            rendered_errors("[resource] fun foo() {}\n"),
            vec![
                "`[resource]` marks a type as a resource: it may label only a `struct`, \
                 an `enum` or a `trait` declaration"
                    .to_string()
            ]
        );
    }

    #[test]
    fn render_gives_the_not_equals_soup_a_first_class_message() {
        // §6a — the `parse_error_hint` stopgap becomes a structural first-class
        // message: recognized by the stray `=` after a `!=` token, not by string
        // matching the source.
        assert_eq!(
            rendered_errors("let x = a!==b;\n"),
            vec![
                "found '=' expected an expression; if this was postfix `!` before a \
                 comparison, the space is required: `a! == b` (`!=` always lexes as \
                 not-equals)"
                    .to_string()
            ]
        );
    }

    #[test]
    fn render_carries_the_production_context() {
        // diagnostics-standard.md §4 — `in parameter type` / `in return type`,
        // curated (never the `context clause` / `generic arguments` noise chumsky
        // merged in at every type position).
        assert_eq!(
            rendered_errors("fun f(x: ) {}\n"),
            vec!["found ')' expected a type in parameter type".to_string()]
        );
        assert_eq!(
            rendered_errors("fun f(): {}\n"),
            vec!["found '{' expected a type in return type".to_string()]
        );
    }

    #[test]
    fn render_surfaces_the_real_error_from_inside_a_recovered_block() {
        // FIX (frontend.md S5 review): a block whose body has a syntax error
        // recovers over its (balanced, CLOSED) braces, but the diagnostic must name
        // the real inner error — the farthest failure recorded inside the region —
        // never falsely claim the block was unclosed. The `!=`-soup hint fires from
        // inside a block, and an unclosed generic in a `let` type annotation steers
        // to `,`/`>` at the offending token.
        assert_eq!(
            rendered_errors("fun f() { let bad = a!==b; }\n"),
            vec![
                "found '=' expected an expression; if this was postfix `!` before a \
                 comparison, the space is required: `a! == b` (`!=` always lexes as \
                 not-equals)"
                    .to_string()
            ]
        );
        assert_eq!(
            rendered_errors("fun f() { let p: Map<str, List<i32> = m; }\n"),
            vec!["found '=' expected ',' or '>' in type annotation".to_string()]
        );
    }

    /// A46: `<>` is a SPAN-ADJACENT pair, like `/>` and `</`. Spaced apart it
    /// is not a fragment head, and the `<` falls through to everything `<`
    /// already begins — so nothing about a comparison changes.
    #[test]
    fn a_spaced_angle_pair_is_not_a_fragment() {
        assert_eq!(
            rendered_errors("fun main() { let p = < >; }\n"),
            vec!["found '<' expected an expression".to_string()]
        );
    }

    /// A46: a fragment parses as a NAMELESS element body — the shape the
    /// formatter and the editor's markup pass read, before the desugar retires
    /// the element node and emits the list literal. All four angle-bracket
    /// spans are recorded (E115), because `<>` and `</>` are the only
    /// punctuation a fragment has.
    #[test]
    fn a_fragment_parses_as_a_nameless_element_body() {
        let (node, _span) = expr("<><i>\"a\"</i>{row}</>");
        let Node::Element(body) = node else {
            panic!("a fragment must parse as an element body, got {node:?}");
        };
        assert!(body.tag.is_none(), "a fragment head carries no name");
        assert!(body.close_tag.is_none(), "`</>` carries no name either");
        assert!(body.head.is_empty(), "a fragment takes no head items");
        assert!(!body.self_closing, "a fragment has no self-closing form");
        assert_eq!(body.children.len(), 2, "both children are kept");
        assert_eq!(body.punctuation.len(), 4, "`<`, `>`, `</`, `>`");
    }

    /// A46: a NAMED element is unchanged — its head still carries a tag span,
    /// which is what keeps the nameless case a distinguishable second form and
    /// not a default.
    #[test]
    fn a_named_element_still_carries_its_tag_span() {
        let (node, _span) = expr("<i>\"a\"</i>");
        let Node::Element(body) = node else {
            panic!("an element must parse as an element body, got {node:?}");
        };
        assert!(body.tag.is_some(), "a named head carries its name");
        assert!(body.close_tag.is_some(), "and so does its close");
    }

    #[test]
    fn render_steers_a_missing_parameter_comma() {
        // FIX (frontend.md S5 review): a missing comma between parameters is a
        // committed list-close failure — it steers to `,`/`)` at the offending
        // token, instead of backtracking to the statement-position expression
        // attempt (which would blame the leading `fun` at column 1).
        assert_eq!(
            rendered_errors("fun f(x: i32 y: i32) {}\n"),
            vec!["found 'y' expected ',' or ')'".to_string()]
        );
    }

    #[test]
    fn render_expects_a_method_name_in_an_unfinished_chain_link() {
        // E67 (editing-dx.md §18): an element head's `.` with no member after
        // it is a COMMITTED chain link failing — the recovery keeps the
        // element (parser_recovery.rs pins the shape); this pins the curated
        // expectation's rendered text (ledger row 204's flagged gap).
        assert_eq!(
            rendered_errors("fun main() { let p = <div><span .></span></div>; }\n"),
            vec!["found '>' expected a method name".to_string()]
        );
    }

    #[test]
    fn render_steers_a_missing_list_separator() {
        // FIX (frontend.md S5 review): a missing separator between comma-list items
        // — at every ~committed closer, not just parameters — steers to `,` or the
        // closer at the offending token, never a false "unclosed" on a region that
        // closed. The committed close demand (`expect_ctrl` for `) ] } >`, `expect_op`
        // for a closure's `|`) records the closer; the list records the `,`. Covers
        // the review's four shapes (struct field, enum variant, call arg, closure
        // param) plus the list literal.
        assert_eq!(
            rendered_errors("struct S { x: i32 y: i32 }\n"),
            vec!["found 'y' expected ',' or '}'".to_string()]
        );
        assert_eq!(
            rendered_errors("enum E { A(i32) B(str) }\n"),
            vec!["found 'B' expected '=', ',', or '}'".to_string()]
        );
        assert_eq!(
            rendered_errors("fun f() { outer(a b) }\n"),
            vec!["found 'b' expected ',' or ')'".to_string()]
        );
        assert_eq!(
            rendered_errors("fun f() { let xs = [a b]; }\n"),
            vec!["found 'b' expected ',' or ']'".to_string()]
        );
        assert_eq!(
            rendered_errors("fun f() { let g = |a b| a; }\n"),
            vec!["found 'b' expected ',' or '|'".to_string()]
        );
    }

    #[test]
    fn render_reports_an_illegal_character() {
        // The S1 `LexError` feeds in as a `found <char>` error (mid-file, so the
        // rest still parses — one error, the skipped BEL).
        let errors = rendered_errors("fun main() { \u{0007} }\n");
        assert_eq!(errors, vec!["found '\\u{7}' expected a token".to_string()]);
    }

    #[test]
    fn the_reach_marker_lexes_parses_and_reprints() {
        // B318 §2.3 — the bill for `#`, paid. It marks a LEAF, a segment
        // mid-path (§10 d: a private `mod` is reachable), and a brace-set
        // element; `use` shares the grammar; and every one round-trips through
        // `vilan fmt` unchanged, because the marker is a fact about the author's
        // intent rather than a formatting decision — stripping it would silently
        // re-arm the plain-reach warning.
        for source in [
            "import pkg::a::{ #hidden };\n",
            "import pkg::a::#hidden;\n",
            "import pkg::a::#m::helper;\n",
            "import pkg::a::{ shown, #hidden, other };\n",
            "use pkg::a::{ #hidden };\n",
        ] {
            assert!(
                rendered_errors(source).is_empty(),
                "{source:?}: {:?}",
                rendered_errors(source)
            );
        }
        // The marker adds no path SEGMENT — it wraps the branch — which is what
        // keeps go-to-definition, find-references and rename pointing at the
        // name.
        let marked = only_item("import pkg::a::#hidden;");
        let Node::Import(ImportBranch::Path("pkg", _, ImportTail::Continue(after_pkg)), ..) =
            &marked
        else {
            panic!("the path reads as written: {marked:?}");
        };
        let ImportBranch::Path("a", _, ImportTail::Continue(after_a)) = after_pkg.as_ref() else {
            panic!("the path reads as written: {marked:?}");
        };
        assert!(
            matches!(after_a.as_ref(), ImportBranch::Reach(_, _)),
            "the marker wraps the branch it marks: {marked:?}"
        );
        // fmt keeps it, and collapses a marked singleton set exactly as it
        // collapses a plain one.
        assert_eq!(
            crate::formatter::format("import pkg::a::{ #hidden };\n"),
            "import pkg::a::#hidden;\n"
        );
        assert_eq!(
            crate::formatter::format("import pkg::a::{ shown, #hidden, other };\n"),
            "import pkg::a::{ #hidden, other, shown };\n"
        );
        assert_eq!(
            crate::formatter::format("import pkg::a::#m::helper;\n"),
            "import pkg::a::#m::helper;\n"
        );
    }

    #[test]
    fn a101_a_css_declaration_is_a_call() {
        // The grammar: `declaration = property "(" [ expression { "," expression }
        // [ "," ] ] ")" ";"`, the property span-adjacent as before. A typed value
        // is an ordinary argument, N arguments are ordinary arguments, and a
        // custom property is a call head (R12).
        for source in [
            "fun main() { let s = css { outline(\"none\"); }; }\n",
            "fun main() { let s = css { width(pct(100)); }; }\n",
            "fun main() { let s = css { margin(px(4), px(8)); }; }\n",
            "fun main() { let s = css { margin(px(4), px(8),); }; }\n",
            "fun main() { let s = css { --brand-ink(gray(900)); }; }\n",
            "fun main() { let s = css { flex-direction(\"column\"); }; }\n",
            "fun main() { let s = css { color(Color::current().alpha(0.5)); }; }\n",
        ] {
            assert!(
                rendered_errors(source).is_empty(),
                "{source:?}: {:?}",
                rendered_errors(source)
            );
        }
        // The `:` spelling every migrating program writes: the rule names the
        // call form, once, at the `:` itself.
        //
        // Written as two pieces on purpose: this is the one fixture that must
        // KEEP the spelling A101 retired, and a whole `css { … }` block in one
        // literal is exactly what the codemod migrates.
        let (_tree, errors) = parse(concat!(
            "fun main() { let s = css { padding",
            ": 1rem; }; }\n"
        ));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(render(&errors[0]), A_CSS_DECLARATION_IS_A_CALL);
        assert_eq!(
            errors[0].span.into_range().len(),
            1,
            "the `:` itself is the token that is wrong"
        );
        // A declaration still needs its `;`, and it still needs a value.
        assert_eq!(
            rendered_errors("fun main() { let s = css { outline(\"none\") }; }\n").len(),
            1
        );
        assert!(!rendered_errors("fun main() { let s = css { outline(); }; }\n").is_empty());
        // `!important` keeps its rule — read off the tokens, because
        // `red !important` is not an expression and the argument list would
        // otherwise report a `,` the author never wanted.
        let (_tree, errors) = parse("fun main() { let s = css { color(red !important); }; }\n");
        assert_eq!(render(&errors[0]), IMPORTANT_HAS_NO_PLACE);
        // And the `#` colour rule is RETIRED with the value grammar it guarded:
        // no CSS token ever reaches a declaration now, so `#333` is refused as
        // the ordinary expression it is not.
        assert!(
            rendered_errors("fun main() { let s = css { color(#333); }; }\n")
                .iter()
                .all(|error| !error.contains("is not a colour here")),
        );
        // `#` outside an import still refuses as a parse error, as it did.
        assert_eq!(
            rendered_errors("fun main() { let x = # 3; }\n"),
            vec!["found '#' expected an expression".to_string()]
        );
    }

    #[test]
    fn export_all_and_the_scope_narrowing_parse_and_reprint() {
        // B318 §2.1/§2.2. `export *;` is a `*` + `;` LOOKAHEAD, not "`*` after
        // `export`": `export * helper;` is a real expression (the deref
        // `*helper`, probe P1b) and reading it as a mistyped `export *;` would
        // take a shape the language already has — it stays B321's refusal.
        assert!(rendered_errors("export *;\n").is_empty());
        assert!(matches!(only_item("export *;"), Node::ExportAll));
        assert_eq!(
            rendered_errors("export * helper;\n"),
            vec![EXPORT_TAKES_AN_ITEM.to_string()]
        );
        // `(in PATH)` is GENERAL (§10 c): `mod` and `pkg` are reserved heads and
        // any other path names the module subtree it roots. `mod` is a KEYWORD
        // token and is admitted as a segment by name, which is what makes
        // `export(in mod)` spellable without reserving a second word.
        for source in [
            "export(in mod) fun helper(): i32 { 1 }\n",
            "export(in pkg) fun helper(): i32 { 1 }\n",
            "export(in pkg::a) struct S { x: i32 }\n",
            "export(in mod) import pkg::io::print;\n",
        ] {
            assert!(
                rendered_errors(source).is_empty(),
                "{source:?}: {:?}",
                rendered_errors(source)
            );
        }
        let Node::Export(Some(scope), _, _) = only_item("export(in pkg::a) struct S { x: i32 }")
        else {
            panic!("the narrowing rides the export node");
        };
        assert_eq!(
            scope.path.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            vec!["pkg", "a"]
        );
        // `export(pkg)` — the spelling that reads as a CALL (probe P2c) — is the
        // scope with `in` left out, and is REPORTED rather than declined, so the
        // author gets the steer instead of a missing-`;` three tokens later. The
        // `;` lookahead keeps that off `export (helper);`, which is B321's.
        assert_eq!(
            rendered_errors("export(pkg) fun helper(): i32 { 1 }\n"),
            vec![EXPORT_TAKES_AN_ITEM.to_string()]
        );
        // fmt reprints both new shapes as written, `::`-joined, no space before
        // the `(`.
        assert_eq!(crate::formatter::format("export *;\n"), "export *;\n");
        assert_eq!(
            crate::formatter::format("export(in pkg::a) import pkg::io::print;\n"),
            "export(in pkg::a) import pkg::io::print;\n"
        );
    }

    #[test]
    fn export_refuses_an_expression_and_keeps_every_item() {
        // B321: `parse_export` took any STATEMENT, so an `export` of a
        // parenthesised expression — and of `*` followed by a name, which is
        // `export` of a deref — compiled clean and published nothing. The two
        // nonsense forms, then every form `export` really takes.
        for nonsense in ["export (helper);\n", "export * helper;\n"] {
            assert_eq!(
                rendered_errors(nonsense),
                vec![EXPORT_TAKES_AN_ITEM.to_string()],
                "for {nonsense:?}"
            );
        }
        for item in [
            "export fun helper(): i32 { 1 }\n",
            "export struct S { x: i32 }\n",
            "export enum E { A }\n",
            "export trait T { fun f(); }\n",
            "export impl S { fun make(): i32 { 1 } }\n",
            "export mod m { fun f() {} }\n",
            "export let answer = 42;\n",
            "export import pkg::io::print;\n",
            "export use pkg::a::b;\n",
            // The attribute wrappers are transparent: the rule asks about the
            // declaration under them, not about the wrapper.
            "[derive(Wire)] export struct S { x: i32 }\n",
            "export external fun serve();\n",
        ] {
            assert!(
                rendered_errors(item).is_empty(),
                "`export` takes {item:?}: {:?}",
                rendered_errors(item)
            );
        }
        // The refusal is about the SHAPE, not about the parentheses: a call and
        // an operator tower are refused with the same rule.
        assert_eq!(
            rendered_errors("export helper();\n"),
            vec![EXPORT_TAKES_AN_ITEM.to_string()]
        );
        assert_eq!(
            rendered_errors("export 1 + 1;\n"),
            vec![EXPORT_TAKES_AN_ITEM.to_string()]
        );
    }

    #[test]
    fn a_malformed_import_reports_at_the_token_it_stopped_on() {
        // B320: the six probe shapes of visibility.md §2.7, which before this
        // all reported `found 'import' expected an expression` at COLUMN 1 of
        // the statement — the keyword, which is the one token that was right.
        // Each now reports the import grammar's own rule, anchored on the token
        // the path actually stopped at. The shapes are B318's new import forms
        // (a selector, `::*`, a reach marker), which is why the row is owed
        // before they are built rather than after.
        // B318 S3 landed in the same order (visibility-b-35), so the SELECTOR
        // shapes parse clean now — they are pinned as grammar in
        // `inference/modules.rs`; the two forms still outside the grammar
        // (`::*` is Order 36's, `!` is no marker) keep this rule.
        for source in [
            "import a::{ (impl Thing) };\n",
            "import a::{ (impl List<i32>)::{ first, last } };\n",
            "import a::{ (impl List<_>) };\n",
            "import a::{ (impl _) };\n",
        ] {
            let (_tree, errors) = parse(source);
            assert!(
                errors.is_empty(),
                "a selector is grammar now, for {source:?}: {errors:?}"
            );
        }
        for (source, stopped) in [
            ("import pkg::a::m::*;\n", "*"),
            ("import a::{ !hidden };\n", "!"),
        ] {
            let (_tree, errors) = parse(source);
            assert_eq!(errors.len(), 1, "one diagnostic for {source:?}: {errors:?}");
            assert_eq!(
                render(&errors[0]),
                IMPORT_PATH_IS_NAMES_AND_SETS,
                "the import grammar's rule, for {source:?}"
            );
            assert_eq!(
                &source[errors[0].span.into_range()],
                stopped,
                "anchored on the token the path stopped at, for {source:?}"
            );
        }
        // `use` shares the path grammar; a selector inside a `use` is S3's own
        // refusal (it parses, then is declined where it is written), and a form
        // the path grammar cannot read at all still names this rule.
        assert_eq!(
            rendered_errors("use a::{ (impl T) };\n"),
            vec![USE_TAKES_NO_IMPL_SELECTOR.to_string()]
        );
        assert_eq!(
            rendered_errors("use a::{ !hidden };\n"),
            vec![IMPORT_PATH_IS_NAMES_AND_SETS.to_string()]
        );
    }

    #[test]
    fn a_well_formed_import_keeps_every_other_reading() {
        // The rule REPLACES nothing it should not: a path the grammar reads is
        // clean, a missing `;` still reports at the gap (the record is only
        // written where the path genuinely fails, so a parsed path leaves it
        // empty), an unclosed brace is still unclosed, and a statement that is
        // not import-led keeps the expression fallback.
        assert!(rendered_errors("import pkg::a::{ b, c as d };\n").is_empty());
        // `only` is B318 S3's trailing modifier now, so it reads clean; a
        // stray word that is not a modifier still stops at the `;` gap.
        assert!(rendered_errors("import pkg::a only;\n").is_empty());
        assert_eq!(
            rendered_errors("import pkg::a merely;\n"),
            vec!["expected `;` to end this statement".to_string()]
        );
        assert_eq!(
            rendered_errors("import a::{ b\n"),
            vec!["unclosed `{`: expected a matching `}`".to_string()]
        );
        assert_eq!(
            rendered_errors("nonsense *;\n"),
            vec!["found ';' expected an expression".to_string()]
        );
    }

    #[test]
    fn a_syntax_error_salvages_the_parsed_prefix() {
        // The LSP payoff (recorded in the recovery differential's ledger): a broken
        // statement does not blank the complete items before it.
        let (tree, errors) = parse("fun ok() {}\nBROKEN nonsense\n");
        let (statements, _span) = tree.expect("a tree is always returned");
        assert_eq!(statements.len(), 1, "the complete `fun ok` survives");
        assert!(matches!(statements[0].0, Node::Func(_)));
        assert!(!errors.is_empty(), "the broken tail is still reported");
    }

    // --- B414: contextual keywords --------------------------------------------

    /// The words the parser READ as keywords in `source`, in order.
    fn readings(source: &str) -> Vec<&str> {
        contextual_keyword_readings(source)
            .into_iter()
            .map(|span| &source[span.into_range()])
            .collect()
    }

    #[test]
    fn b414_each_contextual_keyword_is_read_as_the_keyword_only_at_its_decision_point() {
        let source = concat!(
            "impl Point with Show { fun with(self, with: i32): i32 { with } }\n",
            "trait Ordered with Equal {}\n",
            "fun first(xs: &List<T>): &T borrows xs { xs.borrows }\n",
            "fun take(own list: T, own: T, lazy fallback: T, lazy: T, shape: dyn Show) {}\n",
            "lazy let config = 1;\n",
            "fun main() {\n",
            "\tlet own = 1; let dyn = own; let jump = dyn; let lazy = jump;\n",
            "\tlet close = |own| own.dispose();\n",
            "\tlet path: dyn::Registry = dyn::Registry::new();\n",
            "\tfor x in xs { jump break; }\n",
            "\tlazy = 3; jump.height; with.len();\n",
            "}\n",
        );
        let (_, errors) = parse(source);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            readings(source),
            vec![
                "with", "with", "borrows", "own", "lazy", "dyn", "lazy", "jump"
            ],
        );
    }

    #[test]
    fn b414_a_declined_attempt_takes_its_readings_back() {
        // `dyn` read inside a speculative type attempt that backtracks must not
        // survive as a reading: `a < dyn` is a comparison over a name.
        let source = "fun main() { let b = a < dyn; }\n";
        let (_, errors) = parse(source);
        assert!(errors.is_empty(), "{errors:?}");
        assert!(readings(source).is_empty(), "{:?}", readings(source));
    }

    #[test]
    fn b414_jump_and_lazy_are_statement_heads_only_before_a_name() {
        let tokens = |source: &'static str| lexing::tokenize(source).0;
        for (source, heads) in [
            ("jump break", true),
            ("jump;", false),
            ("jump.height", false),
            ("lazy let x = 1", true),
            ("lazy mut x = 1", true),
            ("lazy x = 1", true),
            ("lazy = 3", false),
            ("lazy.force()", false),
            ("lazy is Some(x)", false),
        ] {
            assert_eq!(
                starts_contextual_statement(&tokens(source), 0),
                heads,
                "{source:?}"
            );
            assert_eq!(
                starts_statement_or_item(&tokens(source), 0),
                heads,
                "{source:?} as a recovery sync point"
            );
        }
    }

    #[test]
    fn b414_a_misplaced_prefix_word_names_its_placement() {
        assert_eq!(
            rendered_errors("fun main() {\n\tlet own x = 1;\n}\n"),
            vec![OWN_IS_A_PARAMETER_CONVENTION.to_string()]
        );
        assert_eq!(
            rendered_errors("fun main() {\n\tlet s = dyn Shape;\n}\n"),
            vec![DYN_IS_A_TYPE_MARKER.to_string()]
        );
    }

    // --- B414 S4: the member tier, and R-k --------------------------------------

    /// The RESERVED words the parser read as MEMBER names in `source`, in order.
    fn member_readings(source: &str) -> Vec<&str> {
        keyword_member_readings(source)
            .into_iter()
            .map(|span| &source[span.into_range()])
            .collect()
    }

    #[test]
    fn b414_s4_a_reserved_word_is_read_as_a_member_at_each_member_position() {
        let source = concat!(
            "struct Event { type: str, if: i32 }\n",
            "trait Shape { fun match(self): str; }\n",
            "impl Event { fun for(self): i32 { self.if } fun in(): Event { Event { type = \"x\", if = 1 } } }\n",
            "import a::{ (impl Event)::ret };\n",
            "fun main() {\n",
            "\tlet e = Event::in();\n",
            "\tlet n = e.for() + e.if;\n",
            "\tlet kind = found?.match();\n",
            "\tif e.if == 1 { ret; } else { ret; }\n",
            "}\n",
        );
        let (_, errors) = parse(source);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(
            member_readings(source),
            vec![
                // declared: fields, a trait method, two impl methods
                "type", "if", "match", "for", "if", "in", "type", "if",
                // an impl selector's member
                "ret", // `::`, `.`, `?.`
                "in", "for", "if", "match", "if",
            ],
        );
    }

    #[test]
    fn b414_s4_a_reserved_word_is_no_member_where_a_name_is_bound() {
        // A free function, a binding, a parameter and the literal shorthand
        // (`{ type }` reads a binding) keep the identifier rule.
        for source in [
            "fun type(): i32 { 1 }\n",
            "fun main() { let type = 1; }\n",
            "fun f(type: i32) {}\n",
            "fun main() { let e = Event { type }; }\n",
        ] {
            let (_, errors) = parse(source);
            assert!(!errors.is_empty(), "{source:?} must not parse");
        }
    }

    #[test]
    fn b414_s4_r_k_a_member_name_is_written_against_its_dot() {
        let steer = format!("found 'n' expected {A_MEMBER_NAME_AGAINST_ITS_DOT}");
        // Same line: reported, and the member is still read — one error.
        assert_eq!(
            rendered_errors("fun main() { let a = s. n; }\n"),
            vec![steer.clone()]
        );
        assert_eq!(
            rendered_errors("fun main() { let a = s?. n; }\n"),
            vec![steer.clone()]
        );
        // Across a line: the next line's first word is never the member.
        assert_eq!(
            rendered_errors("fun main() {\n\tlet a = s.\n\t\tn;\n}\n"),
            vec![steer]
        );
        let (tree, errors) = parse("fun main() {\n\tlet a = s.\n\tlet b = 2;\n}\n");
        assert_eq!(errors.len(), 1, "{errors:?}");
        let (statements, _) = tree.expect("a tree");
        let Node::Func(main) = &statements[0].0 else {
            panic!("expected main");
        };
        let body = main.body.as_ref().expect("a body");
        assert!(
            body.0
                .0
                .iter()
                .any(|statement| matches!(&statement.0, Node::Let(..))),
            "the next line's `let b` survives as its own statement"
        );
        // Nothing between the dot and its name, and nothing after a trailing
        // dot at all — both unchanged, as is the chain broken BEFORE its dot.
        for source in [
            "fun main() { let a = s.n; }\n",
            "fun main() {\n\tlet a = xs\n\t\t.len();\n}\n",
        ] {
            let (_, errors) = parse(source);
            assert!(errors.is_empty(), "{source:?}: {errors:?}");
        }
        let (_, errors) = parse("fun main() { s. }\n");
        assert!(
            errors.is_empty(),
            "the mid-edit `s.` still recovers silently: {errors:?}"
        );
    }

    // --- B459: the `then`/`else` forms -----------------------------------------

    /// The `If` a `then` form parses to, and its spelling.
    fn then_form<'a, 'src>(node: &'a Node<'src>) -> (&'a If<'src>, bool, bool, bool) {
        let Node::If(NodeIfBranch::If(if_)) = node else {
            panic!("expected a `then` form, got {node:?}");
        };
        let IfSpelling::Then {
            then_word,
            else_word,
            statement,
        } = if_.spelling
        else {
            panic!("expected the `then` spelling, got {:?}", if_.spelling);
        };
        (if_, then_word.is_some(), else_word.is_some(), statement)
    }

    /// The statements of `fun main() { … }`'s body.
    fn main_statements(source: &str) -> Vec<Spanned<Node<'_>>> {
        let (mut statements, _) = program(source);
        let Node::Func(main) = statements.remove(0).0 else {
            panic!("expected `fun main`");
        };
        main.body.expect("a body").0.0
    }

    #[test]
    fn b459_the_expression_form_sits_above_assignment_and_below_or() {
        // `a || b then x else y` tests `a || b` (Q1).
        let node = expr("a || b then x else y");
        let (if_, then, otherwise, statement) = then_form(&node.0);
        assert!(then && otherwise && !statement);
        assert!(matches!(if_.condition.0, Node::Binary(BinaryOp::Or, _, _)));
        // `v = c then x else y` assigns the whole form.
        let node = expr("v = c then x else y");
        let Node::Assign(_, None, value) = &node.0 else {
            panic!("expected an assignment, got {node:?}");
        };
        then_form(&value.0);
        // The expression form's branches are the block TAILS: `if c { x } else { y }`.
        let node = expr("c then x else y");
        let (if_, ..) = then_form(&node.0);
        assert!(if_.then.0.0.is_empty());
        assert!(matches!(if_.then.0.1.0, Node::Accessor("x")));
        let Some((NodeIfBranch::Else(block), _)) = &if_.else_ else {
            panic!("expected an else block");
        };
        assert!(matches!(block.0.1.0, Node::Accessor("y")));
    }

    #[test]
    fn b459_a_chain_is_right_associative_and_else_binds_the_nearest_then() {
        // Q2: `a then x else b then y else z` is an `else`-if chain.
        let node = expr("a then x else b then y else z");
        let (if_, ..) = then_form(&node.0);
        let Some((NodeIfBranch::Else(block), _)) = &if_.else_ else {
            panic!("expected an else block");
        };
        let (inner, then, otherwise, _) = then_form(&block.0.1.0);
        assert!(then && otherwise);
        assert!(matches!(inner.condition.0, Node::Accessor("b")));
        // The dangling `else`: `a then b then x else y` nests the `else` in.
        let node = expr("a then b then x else y");
        let (outer, _, outer_else, _) = then_form(&node.0);
        assert!(!outer_else, "the `else` is the nearest `then`'s");
        let (_, _, inner_else, _) = then_form(&outer.then.0.1.0);
        assert!(inner_else);
    }

    #[test]
    fn b459_at_statement_position_the_statement_reading_applies() {
        let statements = main_statements(concat!(
            "fun main() {\n",
            "\tc then f() else g();\n",
            "\tc then f();\n",
            "\tc else ret;\n",
            "\ta then b then f() else g();\n",
            "\tlet v = c then 1 else 2;\n",
            "\t(c then 1 else 2);\n",
            "}\n",
        ));
        // `c then f() else g();` is `if c { f(); } else { g(); }` (Q6).
        let (if_, then, otherwise, statement) = then_form(&statements[0].0);
        assert!(then && otherwise && statement);
        assert_eq!(if_.then.0.0.len(), 1);
        assert!(matches!(if_.then.0.1.0, Node::Void));
        // `c then f();` and the guard `c else ret;`.
        let (_, then, otherwise, statement) = then_form(&statements[1].0);
        assert!(then && !otherwise && statement);
        let (guard, then, otherwise, statement) = then_form(&statements[2].0);
        assert!(!then && otherwise && statement);
        assert!(
            guard.then.0.0.is_empty(),
            "the guard's `then` is the empty block"
        );
        // A form that is a statement's branch is read as a statement too.
        let (outer, ..) = then_form(&statements[3].0);
        let (_, _, _, inner_statement) = then_form(&outer.then.0.0[0].0);
        assert!(inner_statement);
        // An initializer and a parenthesized form are VALUES.
        let Node::Let(_, _, Some(value), ..) = &statements[4].0 else {
            panic!("expected a let, got {:?}", statements[4].0);
        };
        let (_, _, _, statement) = then_form(&value.0);
        assert!(!statement);
        let (_, _, _, statement) = then_form(&statements[5].0);
        assert!(!statement, "`(c then 1 else 2);` is a value in parentheses");
    }

    #[test]
    fn b459_a_value_needs_both_branches_and_the_guard_is_a_statement() {
        assert_eq!(
            rendered_errors("fun main() { let x = c then 1; }\n"),
            vec![THEN_NEEDS_ITS_ELSE.to_string()]
        );
        assert_eq!(
            rendered_errors("fun main() { (c then f()); }\n"),
            vec![THEN_NEEDS_ITS_ELSE.to_string()]
        );
        assert_eq!(
            rendered_errors("fun main() { c then let x = 1; }\n"),
            vec![A_BRANCH_BINDS_NOTHING.to_string()]
        );
        // The guard reads only at a statement's head.
        assert!(declines("fun main() { let x = c else 1; }\n"));
        // A form with both branches is a value anywhere, a block's tail included.
        for source in [
            "fun f(): i32 { c then 1 else 2 }\n",
            "fun main() { let pick = |x: bool| x then 1 else 2; }\n",
            "fun main() { print(1 + (a then 10 else 20)); }\n",
        ] {
            let (_, errors) = parse(source);
            assert!(errors.is_empty(), "{source:?}: {errors:?}");
        }
    }

    #[test]
    fn b459_then_is_a_keyword_only_after_a_complete_operand() {
        let source = concat!(
            "fun then(then: i32): i32 { then }\n",
            "fun main() {\n",
            "\tlet then = 1;\n",
            "\tlet next = then + then;\n",
            "\tpromise.then(done);\n",
            "\tready then go() else then(2);\n",
            "}\n",
        );
        let (_, errors) = parse(source);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(readings(source), vec!["then"]);
        let at = source.find("then go").expect("the keyword");
        assert_eq!(contextual_keyword_readings(source)[0].start, at);
    }

    /// The source with each `(foreign, vilan)` spelling replaced, padded with
    /// spaces to the foreign text's length so every other token keeps its
    /// offset — the tree of the rewrite is then comparable span for span.
    fn respelled(source: &str, replacements: &[(&str, &str)]) -> String {
        let mut respelled = source.to_string();
        for (foreign, vilan) in replacements {
            assert!(
                vilan.len() <= foreign.len(),
                "{vilan} must fit in {foreign}"
            );
            let padded = format!("{vilan:width$}", width = foreign.len());
            respelled = respelled.replace(foreign, &padded);
        }
        respelled
    }

    #[test]
    fn b520_a_foreign_spelling_is_refused_once_and_read_as_vilans() {
        use ForeignSpelling::*;
        // (source, the spellings it refuses in order, the replacements that
        // make it vilan). The refusal spans the foreign token, and the tree is
        // the tree of the source with vilan's spelling written instead.
        let cases: &[(&str, &[ForeignSpelling], &[(&str, &str)])] = &[
            ("fn  add(a: i32): i32 { a }", &[Fn], &[("fn ", "fun")]),
            (
                "function add(a: i32): i32 { a }",
                &[Function],
                &[("function", "fun")],
            ),
            ("func add(a: i32): i32 { a }", &[Func], &[("func", "fun")]),
            ("def add(a: i32): i32 { a }", &[Def], &[("def", "fun")]),
            // Past the markers and attributes that lead an item.
            ("export fn  f() {}", &[Fn], &[("fn ", "fun")]),
            ("async fn  f(): i32 { 1 }", &[Fn], &[("fn ", "fun")]),
            ("[must_use] fn  f(): i32 { 1 }", &[Fn], &[("fn ", "fun")]),
            ("const fn  f(): i32 { 1 }", &[Fn], &[("fn ", "fun")]),
            (
                "[platform(\"node\")] export def f() {}",
                &[Def],
                &[("def", "fun")],
            ),
            ("fn  id<T>(x: T): T { x }", &[Fn], &[("fn ", "fun")]),
            // Members: an impl's item list, a trait body, a method named by a
            // reserved word.
            (
                "impl S { fn  get(self): i32 { 1 } }",
                &[Fn],
                &[("fn ", "fun")],
            ),
            ("trait T { fn  t(self): i32; }", &[Fn], &[("fn ", "fun")]),
            (
                "impl S { def type(self): i32 { 1 } }",
                &[Def],
                &[("def", "fun")],
            ),
            // `return` wherever an expression begins.
            (
                "fun f(): i32 { return 1; }",
                &[Return],
                &[("return", "ret")],
            ),
            ("fun f(): i32 { return 1 }", &[Return], &[("return", "ret")]),
            (
                "fun f(x: i32): i32 { if x > 0 { return x; } x }",
                &[Return],
                &[("return", "ret")],
            ),
            (
                "fun f(x: i32): i32 { match x { 1 => return 2, _ => 3 } }",
                &[Return],
                &[("return", "ret")],
            ),
            (
                "fun f(): i32 { let g = |x: i32| { return x; }; g(1) }",
                &[Return],
                &[("return", "ret")],
            ),
            (
                "fun f(c: bool): i32 { return if c { 1 } else { 2 }; }",
                &[Return],
                &[("return", "ret")],
            ),
            (
                "fun f(): str { return \"s\"; }",
                &[Return],
                &[("return", "ret")],
            ),
            (
                "fun f(self): i32 { return self.x; }",
                &[Return],
                &[("return", "ret")],
            ),
            (
                "fun f(): bool { return true; }",
                &[Return],
                &[("return", "ret")],
            ),
            // The arrow, where a return type's `:` stands — and in a closure
            // type, where the result follows the `|..|` directly.
            ("fun f() -> i32 { 1 }", &[Arrow], &[("->", ":")]),
            ("trait T { fun t(self) -> i32; }", &[Arrow], &[("->", ":")]),
            (
                "fun f(): i32 { let g = |x: i32| -> i32 { x }; g(1) }",
                &[Arrow],
                &[("->", ":")],
            ),
            ("fun f(g: |i32| -> i32) {}", &[TypeArrow], &[("->", "")]),
            ("fun f(g: || -> void) {}", &[TypeArrow], &[("->", "")]),
            // Several in one declaration: each its own refusal, nothing more.
            (
                "fn  f() -> i32 { return 1; }",
                &[Fn, Arrow, Return],
                &[("fn ", "fun"), ("->", ":"), ("return", "ret")],
            ),
        ];
        for (source, spellings, replacements) in cases {
            let (tree, errors) = parse(source);
            let refused: Vec<(Option<ForeignSpelling>, &str)> = errors
                .iter()
                .map(|error| {
                    (
                        ForeignSpelling::of_message(&render(error)),
                        &source[error.span.start..error.span.end],
                    )
                })
                .collect();
            let expected: Vec<(Option<ForeignSpelling>, &str)> = spellings
                .iter()
                .map(|spelling| (Some(*spelling), spelling.written()))
                .collect();
            assert_eq!(refused, expected, "{source}");
            let vilan = respelled(source, replacements);
            let clean = program(&vilan);
            assert_eq!(
                format!("{:?}", tree.expect("a tree")),
                format!("{clean:?}"),
                "{source} reads as {vilan}"
            );
        }
    }

    #[test]
    fn b520_the_foreign_words_stay_names_wherever_they_are_names() {
        // None of the four words is reserved, and nothing that parses today
        // reads differently: each of these is clean and means what it meant.
        for source in [
            "fun main() { let return = 1; let fn = 2; let function = 3; let def = 4; }",
            "fun main() { let func = 5; return; fn; }",
            "fun main() { return(1); fn(2); def(3); }",
            "fun main() { return - 1; return.x; return = 2; return += 1; }",
            "fun main() { return then go(); fn then (go()); return else go(); }",
            "fun main() { let total = return + fn * def; }",
            "fun main() { print(return); print([return, fn]); }",
            "struct S { return: i32, fn: i32, function: i32, def: i32, func: i32 }",
            "fun def(fn: i32): i32 { fn }",
            "fun return(): i32 { 1 }",
            "fun f(return: i32, own function: i32) {}",
            "import a::{ fn, return };",
            "import a::b as fn;",
            "fun main() { x.return(1); x.fn; let s = S { return = 1, fn = 2 }; }",
            "fun main() { if fn is Some(x) { } }",
            "fun main() { match return { _ => 1 } }",
            "fun main() { for x in fn { } }",
            "fun main() { let f = |return: i32| return; }",
        ] {
            let (_, errors) = parse(source);
            assert!(errors.is_empty(), "{source}: {errors:?}");
        }
        // A `-` and a `>` apart are not the arrow: the parse errors stay
        // today's, and none of them is the steer.
        let (_, errors) = parse("fun f() - > i32 { 1 }");
        assert!(!errors.is_empty());
        for error in &errors {
            assert_eq!(ForeignSpelling::of_message(&render(error)), None);
        }
    }

    #[test]
    fn b520_the_recovery_resumes_at_a_foreign_head() {
        // `1 + return 5`: `return value` is no operand, as `ret` is none — the
        // operand's refusal, then the return read as one.
        let source = "fun main() {\n    let y = 1 +\n    return 5;\n}\n";
        let (_, errors) = parse(source);
        let rendered: Vec<String> = errors.iter().map(render).collect();
        assert_eq!(
            rendered,
            vec![
                "found 'return' expected an expression".to_string(),
                ForeignSpelling::Return.message().to_string(),
            ],
            "{source}"
        );
        // `pub fn f()`: the visibility rule at `pub` — `fn name(` is a fresh
        // item — then the `fun` steer; nothing else.
        let source = "pub fn f(): i32 { 1 }\n";
        let (tree, errors) = parse(source);
        let rendered: Vec<String> = errors.iter().map(render).collect();
        assert_eq!(rendered.len(), 2, "{rendered:?}");
        assert!(rendered[0].starts_with("`pub` is not a vilan keyword"));
        assert_eq!(rendered[1], ForeignSpelling::Fn.message());
        let (statements, _) = tree.expect("a tree");
        assert!(
            statements
                .iter()
                .any(|(node, _)| matches!(node, Node::Func(_))),
            "the declaration is in the tree: {statements:?}"
        );
    }

    #[test]
    fn b520_each_foreign_spelling_carries_its_code_and_its_fix() {
        // The codes are the editor's keys: stable, distinct, and spelled here
        // so a rename reds.
        let codes: Vec<&str> = ForeignSpelling::ALL
            .iter()
            .map(|spelling| spelling.code())
            .collect();
        assert_eq!(
            codes,
            vec![
                "foreign-spelling/return",
                "foreign-spelling/fn",
                "foreign-spelling/function",
                "foreign-spelling/func",
                "foreign-spelling/def",
                "foreign-spelling/arrow",
                "foreign-spelling/type-arrow",
            ]
        );
        // (source, the source after the fix): applying the diagnostic's own
        // fix turns each into vilan, which parses clean.
        for (source, fixed) in [
            ("fun f(): i32 { return 1; }", "fun f(): i32 { ret 1; }"),
            ("fn add(a: i32): i32 { a }", "fun add(a: i32): i32 { a }"),
            ("function f() {}", "fun f() {}"),
            ("func f() {}", "fun f() {}"),
            ("def f() {}", "fun f() {}"),
            ("fun f() -> i32 { 1 }", "fun f(): i32 { 1 }"),
            ("fun f()->i32 { 1 }", "fun f():i32 { 1 }"),
            ("fun f()\n    -> i32 { 1 }", "fun f(): i32 { 1 }"),
            (
                "fun f(): i32 { let g = |x: i32| -> i32 { x }; g(1) }",
                "fun f(): i32 { let g = |x: i32|: i32 { x }; g(1) }",
            ),
            ("fun f(g: |i32| -> i32) {}", "fun f(g: |i32| i32) {}"),
        ] {
            let (_, errors) = parse(source);
            assert_eq!(errors.len(), 1, "{source}: {errors:?}");
            let message = render(&errors[0]);
            let fix = foreign_spelling_fix(source, &message, errors[0].span)
                .unwrap_or_else(|| panic!("{source}: no fix for {message}"));
            let spelling = ForeignSpelling::of_message(&message).expect("a spelling");
            assert_eq!(fix.code, spelling.code());
            assert_eq!(fix.replacement, spelling.vilan());
            let mut edited = source.to_string();
            edited.replace_range(fix.span.start..fix.span.end, fix.replacement);
            assert_eq!(edited, fixed, "{source}");
            program(&edited);
        }
        // Any other diagnostic has no such fix.
        let (_, errors) = parse("fun f( {");
        let message = render(&errors[0]);
        assert_eq!(
            foreign_spelling_fix("fun f( {", &message, errors[0].span),
            None
        );
    }

    /// B486's matrix (papers-45's `order_matrix.py`), re-read in B536's THE
    /// order: each declaration kind's full stack — its attributes by
    /// [`attribute_rank`], then `export` and the other keywords by Q8's — and
    /// every ADJACENT swap of it, 29 in all.
    const B486_STACKS: &[(&[&str], &str)] = &[
        (
            &[
                "[deprecated(\"use g\")]",
                "[internal(\"why\")]",
                "[extern(\"f\")]",
                "[must_use]",
                "[platform(\"node\")]",
                "export",
                "async",
                "external",
            ],
            "fun f(): i32;",
        ),
        (
            &[
                "[deprecated(\"use g\")]",
                "[internal(\"why\")]",
                "[must_use]",
                "[platform(\"node\")]",
                "export",
                "async",
            ],
            "fun f(): i32 { 1 }",
        ),
        (
            &[
                "[deprecated(\"use T\")]",
                "[internal(\"why\")]",
                "[platform(\"node\")]",
                "[resource]",
                "export",
                "external",
            ],
            "struct H;",
        ),
        (
            &[
                "[derive(PartialEq)]",
                "[deprecated(\"use T\")]",
                "[internal(\"why\")]",
                "[platform(\"node\")]",
                "[resource]",
                "export",
            ],
            "struct S { a: i32 }",
        ),
        (
            &[
                "[deprecated(\"use U\")]",
                "[internal(\"why\")]",
                "[platform(\"node\")]",
                "[resource]",
                "export",
            ],
            "trait T { fun t(self): i32; }",
        ),
        (
            &[
                "[deprecated(\"use y\")]",
                "[internal(\"why\")]",
                "export",
                "lazy",
            ],
            "let x = 1;",
        ),
    ];

    /// The declaration head a stack spells, attributes abbreviated as the
    /// marker-order diagnostics write them.
    fn b486_spelled(stack: &[&str], declaration: &str) -> String {
        let mut spelled: Vec<String> = stack
            .iter()
            .map(|marker| {
                if !marker.starts_with('[') {
                    return marker.to_string();
                }
                let name = marker[1..].split(['(', ']']).next().unwrap();
                if marker.contains('(') {
                    format!("[{name}(..)]")
                } else {
                    format!("[{name}]")
                }
            })
            .collect();
        spelled.push(declaration.split(' ').next().unwrap().to_string());
        spelled.join(" ")
    }

    /// B536's classification of the 29 swaps (RULED 2026-10-03): the stack is
    /// CANONICAL; a swap of two attributes is REFUSED as out of rank (it
    /// warned for one release, v0.44.0 — the v0.45.0 flip, R-c) and read in
    /// it; a swap that puts a keyword ahead of an attribute — `export`
    /// included, B485 S3 — or inverts two keywords is REFUSED, and read as the
    /// stack. Every one of them `vilan fmt` writes as the stack, idempotently.
    #[test]
    fn b536_every_adjacent_marker_swap_is_canonical_warned_or_refused() {
        let is_attribute = |marker: &str| marker.starts_with('[');
        let (mut warned, mut refused) = (0, 0);
        for (stack, declaration) in B486_STACKS {
            let canonical = format!("{} {declaration}\n", stack.join(" "));
            let spelled = b486_spelled(stack, declaration);
            let (_, errors, warnings) = parse_with_warnings(&canonical);
            assert!(
                errors.is_empty() && warnings.is_empty(),
                "{canonical}: {errors:?} {warnings:?}"
            );
            let canonical_print = crate::formatter::reprint(&canonical)
                .unwrap_or_else(|decline| panic!("{canonical}: {decline:?}"));
            assert_eq!(
                crate::formatter::reprint(&canonical_print).as_deref(),
                Ok(canonical_print.as_str()),
                "{canonical}: not idempotent"
            );
            // The printer writes THE order itself — the net sorts both of its
            // streams, so only this says the print is not merely the source's
            // tokens in some order the net accepts.
            let (_, errors, warnings) = parse_with_warnings(&canonical_print);
            assert!(
                errors.is_empty() && warnings.is_empty(),
                "{canonical_print}: {errors:?} {warnings:?}"
            );
            for at in 0..stack.len() - 1 {
                let mut swapped = stack.to_vec();
                swapped.swap(at, at + 1);
                let source = format!("{} {declaration}\n", swapped.join(" "));
                let (tree, errors, warnings) = parse_with_warnings(&source);
                let errors: Vec<String> = errors.iter().map(render).collect();
                let warnings: Vec<(Span, String)> = warnings
                    .iter()
                    .map(|warning| (warning.span, render(warning)))
                    .collect();
                let run = Span::from(0..source.find(declaration).unwrap() - 1);
                if is_attribute(swapped[at]) && is_attribute(swapped[at + 1]) {
                    warned += 1;
                    assert!(warnings.is_empty(), "{source}: {warnings:?}");
                    assert_eq!(errors, vec![attribute_order_rule(&spelled)], "{source}");
                    let (_, spanned, _) = parse_with_warnings(&source);
                    assert_eq!(spanned[0].span, run, "{source}");
                } else {
                    refused += 1;
                    assert!(warnings.is_empty(), "{source}: {warnings:?}");
                    assert_eq!(errors, vec![marker_order_rule(&spelled)], "{source}");
                    let (statements, _) = tree.expect("a tree");
                    assert_eq!(statements.len(), 1, "{source}: {statements:?}");
                    assert!(
                        matches!(statements[0].0, Node::Export(..)),
                        "{source}: {statements:?}"
                    );
                }
                let print = crate::formatter::reprint(&source)
                    .unwrap_or_else(|decline| panic!("{source}: {decline:?}"));
                assert_eq!(print, canonical_print, "{source}");
            }
        }
        assert_eq!((warned, refused), (18, 11));
    }

    /// B536: attributes out of [`attribute_rank`]'s order, and nothing else
    /// out of order, are REFUSED since v0.45.0 (a WARNING for one release) —
    /// spanning the run and naming the head in THE order; the tree is still
    /// the canonical spelling's, which is what `vilan fmt` writes.
    #[test]
    fn b536_attributes_out_of_rank_are_refused_and_read_in_it() {
        for (source, canonical, spelled) in [
            (
                "[internal(\"r\")] [deprecated(\"d\")] fun f() {}",
                "[deprecated(\"d\")] [internal(\"r\")] fun f() {}",
                "[deprecated(..)] [internal(..)] fun",
            ),
            (
                "[platform(\"node\")]\n[must_use]\nexport async fun f(): i32 { 1 }",
                "[must_use]\n[platform(\"node\")]\nexport async fun f(): i32 { 1 }",
                "[must_use] [platform(..)] export async fun",
            ),
            (
                "[resource] [derive(PartialEq)] struct S { a: i32 }",
                "[derive(PartialEq)] [resource] struct S { a: i32 }",
                "[derive(..)] [resource] struct",
            ),
            (
                "[internal(\"r\")] [deprecated(\"d\")] lazy let x = 1;",
                "[deprecated(\"d\")] [internal(\"r\")] lazy let x = 1;",
                "[deprecated(..)] [internal(..)] lazy let",
            ),
            (
                "[must_use] [deprecated(\"d\")] const fun f(): i32 { 1 }",
                "[deprecated(\"d\")] [must_use] const fun f(): i32 { 1 }",
                "[deprecated(..)] [must_use] const fun",
            ),
            (
                "trait T {\n\t[must_use] [deprecated(\"d\")] fun t(self): i32;\n}",
                "trait T {\n\t[deprecated(\"d\")] [must_use] fun t(self): i32;\n}",
                "[deprecated(..)] [must_use] fun",
            ),
            (
                "[platform(\"node\")] [deprecated(\"d\")] async macro fun m() {}",
                "[deprecated(\"d\")] [platform(\"node\")] async macro fun m() {}",
                "[deprecated(..)] [platform(..)] async macro fun",
            ),
        ] {
            let (tree, errors, warnings) = parse_with_warnings(source);
            assert!(warnings.is_empty(), "{source}: {warnings:?}");
            let rendered: Vec<String> = errors.iter().map(render).collect();
            assert_eq!(rendered, vec![attribute_order_rule(spelled)], "{source}");
            let run_start = source.find('[').unwrap();
            let word = spelled.rsplit(' ').next().unwrap();
            let run_end = source[..source.find(&format!(" {word} ")).unwrap()]
                .trim_end()
                .len();
            assert_eq!(errors[0].span, Span::from(run_start..run_end), "{source}");
            assert_eq!(
                MarkerOrderDiagnostic::of_message(&rendered[0]),
                Some(MarkerOrderDiagnostic::Attributes)
            );
            // The tree is the canonical spelling's: `vilan fmt` prints the
            // two alike.
            assert!(tree.is_some());
            let (_, canonical_errors, canonical_warnings) = parse_with_warnings(canonical);
            assert!(canonical_errors.is_empty() && canonical_warnings.is_empty());
            assert_eq!(
                crate::formatter::reprint(source),
                crate::formatter::reprint(canonical),
                "{source}"
            );
            // `parse` — what every reader but the reporting pipelines calls —
            // hands back the same refusal.
            let (_, errors) = parse(source);
            assert_eq!(errors.len(), 1, "{source}: {errors:?}");
        }
        // Nothing to warn about: THE order, a tie kept as written (two
        // generators, two `[hint]`s), one attribute, a run that is not a
        // declaration head.
        for source in [
            "[deprecated(\"d\")] [internal(\"r\")] [must_use] [platform(\"node\")] export async fun f(): i32 { 1 }",
            "[service(Api)] [derive(PartialEq)] struct S { a: i32 }",
            "[hint(Show)] [hint(Eq)] [resource] external struct H;",
            "[must_use] fun f(): i32 { 1 }",
            "fun main() { let a = [1]; let b = [0]; [a][b]; }",
        ] {
            let (_, errors, warnings) = parse_with_warnings(source);
            assert!(warnings.is_empty(), "{source}: {warnings:?}");
            assert!(
                errors
                    .iter()
                    .all(|error| !matches!(error.reason, ParseErrorReason::MarkerOrder { .. })),
                "{source}: {errors:?}"
            );
        }
    }

    /// B536's editor half: the stable codes, the message recognizer, and the
    /// quick fix — the run's units permuted into THE order with what stood
    /// between them kept in place — which re-parses with neither the error
    /// nor the warning.
    #[test]
    fn b536_the_marker_order_fix_writes_the_head_in_the_order() {
        assert_eq!(
            MarkerOrderDiagnostic::Attributes.code(),
            "marker-order/attributes"
        );
        assert_eq!(
            MarkerOrderDiagnostic::Keywords.code(),
            "marker-order/keywords"
        );
        // Neither is a warning since v0.45.0 (B536's flip).
        assert!(!MarkerOrderDiagnostic::Attributes.is_warning());
        assert!(!MarkerOrderDiagnostic::Keywords.is_warning());
        assert_eq!(
            MarkerOrderDiagnostic::of_message(&attribute_order_rule("[must_use] fun")),
            Some(MarkerOrderDiagnostic::Attributes)
        );
        assert_eq!(
            MarkerOrderDiagnostic::of_message(&marker_order_rule("[must_use] export fun")),
            Some(MarkerOrderDiagnostic::Keywords)
        );
        assert_eq!(MarkerOrderDiagnostic::of_message("cannot find 'x'"), None);
        for (source, fixed, code, title) in [
            (
                "[internal(\"r\")] [deprecated(\"d\")] fun f() {}",
                "[deprecated(\"d\")] [internal(\"r\")] fun f() {}",
                "marker-order/attributes",
                "Write `[deprecated(..)] [internal(..)] fun`",
            ),
            (
                "// lead\n[must_use]\n// between\n[deprecated(\"d\")]\nfun f(): i32 { 1 }",
                "// lead\n[deprecated(\"d\")]\n// between\n[must_use]\nfun f(): i32 { 1 }",
                "marker-order/attributes",
                "Write `[deprecated(..)] [must_use] fun`",
            ),
            (
                "export [must_use] fun f(): i32 { 1 }",
                "[must_use] export fun f(): i32 { 1 }",
                "marker-order/keywords",
                "Write `[must_use] export fun`",
            ),
            (
                "export(in pkg)\n[deprecated(\"d\")]\nfun f() {}",
                "[deprecated(\"d\")]\nexport(in pkg)\nfun f() {}",
                "marker-order/keywords",
                "Write `[deprecated(..)] export fun`",
            ),
            (
                "[internal(\"r\")] export [deprecated(\"d\")] struct S {}",
                "[deprecated(\"d\")] [internal(\"r\")] export struct S {}",
                "marker-order/keywords",
                "Write `[deprecated(..)] [internal(..)] export struct`",
            ),
            (
                "async [platform(\"node\")] fun f() {}",
                "[platform(\"node\")] async fun f() {}",
                "marker-order/keywords",
                "Write `[platform(..)] async fun`",
            ),
            (
                "external async fun f(): i32;",
                "async external fun f(): i32;",
                "marker-order/keywords",
                "Write `async external fun`",
            ),
            (
                "macro async fun m() {}",
                "async macro fun m() {}",
                "marker-order/keywords",
                "Write `async macro fun`",
            ),
            (
                "fun main() {}\nexport [must_use] [deprecated(\"d\")] fun f(): i32 { 1 }\n",
                "fun main() {}\n[deprecated(\"d\")] [must_use] export fun f(): i32 { 1 }\n",
                "marker-order/keywords",
                "Write `[deprecated(..)] [must_use] export fun`",
            ),
        ] {
            let (_, errors, warnings) = parse_with_warnings(source);
            let diagnostics: Vec<&ParseError> = errors.iter().chain(&warnings).collect();
            assert_eq!(diagnostics.len(), 1, "{source}: {diagnostics:?}");
            let message = render(diagnostics[0]);
            let fix = marker_order_fix(source, &message, diagnostics[0].span)
                .unwrap_or_else(|| panic!("{source}: no fix for {message}"));
            assert_eq!(fix.code, code, "{source}");
            assert_eq!(fix.title, title, "{source}");
            let mut edited = source.to_string();
            edited.replace_range(fix.span.start..fix.span.end, &fix.replacement);
            assert_eq!(edited, fixed, "{source}");
            let (_, errors, warnings) = parse_with_warnings(&edited);
            assert!(
                errors.is_empty() && warnings.is_empty(),
                "{edited}: {errors:?} {warnings:?}"
            );
        }
        // Any other diagnostic, and a span that is no longer a run, has none.
        let (_, errors) = parse("fun f( {");
        assert_eq!(
            marker_order_fix("fun f( {", &render(&errors[0]), errors[0].span),
            None
        );
        let source = "export [must_use] fun f(): i32 { 1 }";
        let (_, errors) = parse(source);
        let message = render(&errors[0]);
        assert_eq!(
            marker_order_fix("fun g() {}", &message, errors[0].span),
            None
        );
    }

    /// B485 S3 (RULED for v0.44.0): each stack in the order before B485 —
    /// `export` ahead of every attribute — is refused, steered to THE order,
    /// read as it, and written in it by `vilan fmt`.
    #[test]
    fn b485_s3_export_ahead_of_the_attributes_is_refused_and_formatted() {
        for (stack, declaration) in B486_STACKS {
            let canonical = format!("{} {declaration}\n", stack.join(" "));
            let mut old = stack.to_vec();
            old.retain(|marker| *marker != "export");
            old.insert(0, "export");
            let source = format!("{} {declaration}\n", old.join(" "));
            let (_, errors) = parse(&source);
            assert_eq!(
                errors.iter().map(render).collect::<Vec<_>>(),
                vec![marker_order_rule(&b486_spelled(stack, declaration))],
                "{source}"
            );
            assert_eq!(
                crate::formatter::reprint(&source),
                crate::formatter::reprint(&canonical),
                "{source}"
            );
        }
    }

    #[test]
    fn b486_keywords_take_one_order_and_come_after_the_attributes() {
        // (written, the head the steer spells): Q8's order — `export`,
        // `const`|`lazy`, `async`, `external`|`macro` — after the attributes.
        for (source, canonical) in [
            ("async export fun f(): i32 { 1 }", "export async fun"),
            ("lazy export let x = 1;", "export lazy let"),
            ("const export fun f(): i32 { 1 }", "export const fun"),
            ("macro export fun m() { }", "export macro fun"),
            (
                "[extern(\"f\")] external async fun f(): i32;",
                "[extern(..)] async external fun",
            ),
            ("external export struct H;", "export external struct"),
            (
                "export async [must_use] fun f(): i32 { 1 }",
                "[must_use] export async fun",
            ),
            (
                "const [deprecated(\"x\")] fun f(): i32 { 1 }",
                "[deprecated(..)] const fun",
            ),
            (
                "export const [internal(\"x\")] let x = 1;",
                "[internal(..)] export const let",
            ),
            (
                "lazy [internal(\"x\")] let x = 1;",
                "[internal(..)] lazy let",
            ),
            // B485 S3: `export` and `macro` ahead of the attributes, the
            // order before B485 — and a run split across the marker.
            (
                "export [must_use] async fun f(): i32 { 1 }",
                "[must_use] export async fun",
            ),
            (
                "export(in pkg) [deprecated(\"x\")] fun f() {}",
                "[deprecated(..)] export fun",
            ),
            (
                "[deprecated(\"x\")] export [must_use] fun f(): i32 { 1 }",
                "[deprecated(..)] [must_use] export fun",
            ),
            (
                "export [deprecated(\"use b\")] import a::b as c;",
                "[deprecated(..)] export import",
            ),
            (
                "macro [deprecated(\"x\")] fun m() { }",
                "[deprecated(..)] macro fun",
            ),
            // A keyword ahead of attributes that are out of rank as well: the
            // one refusal, spelling the whole head in the order.
            (
                "export [must_use] [deprecated(\"x\")] fun f(): i32 { 1 }",
                "[deprecated(..)] [must_use] export fun",
            ),
            // B524: `async` before `macro` (Q8's table), so the order the
            // macro production read before B524 is the steered one.
            ("macro async fun m() { }", "async macro fun"),
            (
                "macro [deprecated(\"x\")] async fun m() { }",
                "[deprecated(..)] async macro fun",
            ),
        ] {
            let (tree, errors) = parse(source);
            let rendered: Vec<String> = errors.iter().map(render).collect();
            assert_eq!(rendered, vec![marker_order_rule(canonical)], "{source}");
            let (statements, _) = tree.expect("a tree");
            assert_eq!(statements.len(), 1, "{source}: {statements:?}");
        }
        // A set no order makes legal is not steered to one: the production's
        // own refusal stands.
        for source in ["async const fun f(): i32 { 1 }", "async lazy let x = 1;"] {
            let (_, errors) = parse(source);
            assert!(!errors.is_empty(), "{source}");
            for error in &errors {
                assert!(
                    !matches!(error.reason, ParseErrorReason::MarkerOrder { .. }),
                    "{source}: {errors:?}"
                );
            }
        }
        // The ruled order reads clean, and a run the order does not reach — a
        // list indexed by a list, an `async` block — is the expression it
        // always was. (`export` ahead of the attributes, the order this test
        // read clean before B485 S3, is in the list above.)
        for source in [
            "[must_use] export async fun f(): i32 { 1 }",
            "[deprecated(\"x\")] export const fun f(): i32 { 1 }",
            "async macro fun m() { }",
            "[deprecated(\"x\")] async macro fun m() { }",
            "[deprecated(\"x\")] export(in pkg) fun f() {}",
            "[resource] export external struct H;",
            "fun main() { [a][b]; }",
            "fun main() { let x = async { 1 }; }",
            "fun main() { const [1, 2]; }",
        ] {
            program(source);
        }
    }

    #[test]
    fn b486_a_reordered_head_still_begins_where_it_was_written() {
        // The reorder permutes tokens, and every node still begins at the
        // first unit as written — the statement, the export, the item.
        // (Refused since v0.45.0, B536 — and still read, so still spanned.)
        let source = "[platform(\"node\")] [deprecated(\"x\")] export fun f() {}";
        let (tree, errors) = parse(source);
        assert_eq!(errors.len(), 1);
        let (statements, _) = tree.expect("a tree");
        assert_eq!(statements[0].1, Span::from(0..source.len()));
        match &statements[0].0 {
            Node::Export(_, inner, _) => assert_eq!(inner.1.start, 0, "{inner:?}"),
            other => panic!("{other:?}"),
        }
        let source = "async [platform(\"node\")] fun f() {}";
        let (tree, errors) = parse(source);
        assert_eq!(errors.len(), 1);
        let (statements, _) = tree.expect("a tree");
        assert_eq!(statements[0].1, Span::from(0..source.len()));
    }

    #[test]
    fn b487_a_const_declaration_carries_the_label_prefix() {
        match only_item("[deprecated(\"use g\")] [must_use] const fun f(): i32 { 1 }") {
            Node::Const(inner) => match &inner.0 {
                Node::Func(function) => {
                    assert_eq!(function.deprecated, Some("use g"));
                    assert!(function.must_use);
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        match only_item("[internal(\"why\")] const let x = 1;") {
            Node::Const(inner) => match &inner.0 {
                Node::Let(name, _, _, _, _, Some(labels)) => {
                    assert_eq!(name.0, "x");
                    assert_eq!(labels.internal, Some("why"));
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        }
        // Under `export` (the other side of it is B485 S3's refusal).
        for source in ["[deprecated(\"use y\")] export const let y = 1;"] {
            match only_item(source) {
                Node::Export(_, inner, None) => match &inner.0 {
                    Node::Const(declaration) => {
                        assert!(
                            matches!(&declaration.0, Node::Let(.., Some(labels)) if labels.deprecated == Some("use y")),
                            "{source}: {declaration:?}"
                        );
                    }
                    other => panic!("{source}: {other:?}"),
                },
                other => panic!("{source}: {other:?}"),
            }
        }
        // A binding takes no `[must_use]`, and a destructure no label: not a
        // declaration this production reads.
        assert!(declines("[must_use] const let x = 1;"));
        assert!(declines("[internal(\"x\")] const let (a, b) = (1, 2);"));
        // `const mut` stays the refusal it was, unlabelled.
        let (_, errors) = parse("const mut x = 1;");
        assert_eq!(render(&errors[0]), CONST_HAS_NO_MUTATION);
    }

    #[test]
    fn b494_async_where_a_binding_begins_is_refused_and_read_as_the_binding() {
        // (source, the binding's name): one refusal at `async`, and the
        // binding bound — so nothing reading it cascades into "cannot find".
        for (source, name) in [
            ("fun main() { async x = 1; print(x); }", "x"),
            (
                "fun main() { async total: i32 = 1; print(total); }",
                "total",
            ),
            ("fun main() { async let x = 1; print(x); }", "x"),
            ("fun main() { async mut x = 1; x = 2; }", "x"),
            ("async config = 1;", "config"),
        ] {
            let (tree, errors) = parse(source);
            let refused: Vec<(String, &str)> = errors
                .iter()
                .map(|error| (render(error), &source[error.span.start..error.span.end]))
                .collect();
            assert_eq!(
                refused,
                vec![(ASYNC_MARKS_NO_BINDING.to_string(), "async")],
                "{source}"
            );
            let printed = format!("{:?}", tree.expect("a tree"));
            assert!(
                printed.contains(&format!("Let((\"{name}\"")),
                "{source}: the binding is in the tree: {printed}"
            );
        }
        // `async` before a function, a block or an expression is untouched.
        for source in [
            "async fun f(): i32 { 1 }",
            "fun main() { let pending = async { 1 }; }",
            "fun main() { async go(); }",
            "fun main() { let x = async load(); }",
        ] {
            program(source);
        }
    }

    /// A154 / E268: the edit for a moved-module refusal, on every import shape
    /// it meets. `anchor` is the n-th occurrence of the old segment the
    /// refusal anchors at (the analyzer's span, pinned in `module_resolution`).
    fn moved_edit(
        source: &str,
        old: &str,
        occurrence: usize,
    ) -> Option<Result<String, &'static str>> {
        let start = source
            .match_indices(old)
            .nth(occurrence)
            .map(|(start, _)| start)
            .expect("the anchor");
        let new = moved_std_module(old).expect("a moved module");
        let message = moved_std_module_message(old, new);
        let edit = moved_std_module_edit(source, &message, Span::from(start..start + old.len()))?;
        Some(edit.map(|fix| {
            let mut text = source.to_string();
            text.replace_range(fix.span.into_range(), &fix.replacement);
            text
        }))
    }

    #[test]
    fn a154_the_moved_path_edit_replaces_the_old_segment() {
        for (source, old, after) in [
            (
                "import std::dom::{ create_element, Element };",
                "dom",
                "import std::web::dom::{ create_element, Element };",
            ),
            (
                "import std::{ rpc_server::Server, ui::View };",
                "rpc_server",
                "import std::{ rpc::server::Server, ui::View };",
            ),
            (
                "import std::web::Signal as S;",
                "web",
                "import std::web::prelude::Signal as S;",
            ),
            // A list of prelude names only: the segment is still the edit.
            (
                "import std::web::{ Signal, Memo };",
                "web",
                "import std::web::prelude::{ Signal, Memo };",
            ),
        ] {
            assert_eq!(
                moved_edit(source, old, 0),
                Some(Ok(after.to_string())),
                "{source}"
            );
        }
    }

    /// E268: a brace list under the old web-prelude path that also names one of
    /// `std::web`'s children keeps the child and writes `prelude::` before each
    /// prelude name — at the top of a file, nested in an outer list, aliased,
    /// and block-scoped.
    #[test]
    fn e268_a_mixed_web_list_prefixes_the_prelude_names_and_keeps_the_children() {
        for (source, after) in [
            (
                "import std::web::{ Signal, dom::create_element };",
                "import std::web::{ prelude::Signal, dom::create_element };",
            ),
            (
                "import std::web::{ dom::create_element, Signal, ui, Memo as M };",
                "import std::web::{ dom::create_element, prelude::Signal, ui, prelude::Memo as M };",
            ),
            (
                "import std::{ web::{ Signal, prelude::view }, option::Option };",
                "import std::{ web::{ prelude::Signal, prelude::view }, option::Option };",
            ),
            (
                "export import std::web::{\n\tSignal,\n\tstyle,\n};",
                "export import std::web::{\n\tprelude::Signal,\n\tstyle,\n};",
            ),
            (
                "fun main() {\n\timport std::web::{ Signal, dom::x };\n}",
                "fun main() {\n\timport std::web::{ prelude::Signal, dom::x };\n}",
            ),
        ] {
            assert_eq!(
                moved_edit(source, "web", 0),
                Some(Ok(after.to_string())),
                "{source}"
            );
        }
    }

    /// E269: the old prelude imported AS A MODULE — `import std::web;`, read
    /// as `web::Signal`, or `self` in a brace list under the old path — keeps
    /// the binding's name: `prelude as web`. An aliased leaf keeps its alias,
    /// and another moved module whose new path ends in another name
    /// (`rpc_server` → `rpc::server`) is aliased back the same way.
    #[test]
    fn e269_the_old_web_prelude_as_a_module_keeps_its_name() {
        for (source, old, after) in [
            (
                "import std::web;",
                "web",
                "import std::web::prelude as web;",
            ),
            (
                "import std::web as w;",
                "web",
                "import std::web::prelude as w;",
            ),
            (
                "import std::web::{ self };",
                "web",
                "import std::web::{ prelude as web };",
            ),
            (
                "import std::web::{ self, Signal };",
                "web",
                "import std::web::{ prelude as web, prelude::Signal };",
            ),
            (
                "import std::web::{ Signal, self as w, dom::x };",
                "web",
                "import std::web::{ prelude::Signal, prelude as w, dom::x };",
            ),
            (
                "import std::rpc_server;",
                "rpc_server",
                "import std::rpc::server as rpc_server;",
            ),
            // A module whose new path ends in its own name needs no alias.
            ("import std::dom;", "dom", "import std::web::dom;"),
            ("import std::dom::x;", "dom", "import std::web::dom::x;"),
        ] {
            assert_eq!(
                moved_edit(source, old, 0),
                Some(Ok(after.to_string())),
                "{source}"
            );
        }
    }

    /// The shapes no one edit rewrites correctly are left to a person, with
    /// the reason; a span that no longer covers the old segment is no edit.
    #[test]
    fn e268_a_web_list_naming_a_marker_is_left_with_its_reason() {
        assert_eq!(
            moved_edit("import std::web::{ #Signal, dom::x };", "web", 0),
            Some(Err(MOVED_WEB_MARKED_REASON))
        );
        let source = "import std::dom::x;";
        let message = moved_std_module_message("dom", "web::dom");
        assert_eq!(
            moved_std_module_edit(source, &message, Span::from(0..3)),
            None
        );
        assert_eq!(
            moved_std_module_edit(source, "cannot find 'x'", Span::from(11..14)),
            None
        );
    }
}
