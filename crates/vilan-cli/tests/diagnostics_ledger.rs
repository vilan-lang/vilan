//! The diagnostics ledger, and the errors appendix, held by machine (N36/N37).
//!
//! Two records describe the compiler's user-facing message surface, and until
//! this file existed neither was checked by anything:
//!
//! - `projects/vilan/proposal/diagnostics-ledger.md` in the **proposals**
//!   repository — every message form, its site, its audit verdict, its pins.
//!   It went three orders stale (audit run 6, F8): roughly four dozen shipped
//!   message forms were unrowed, and two rows keyed on literals that no longer
//!   existed (F9). Every previous sweep was a lane reading the tree by hand.
//! - `vilan/docs/appendix/errors.md` — the reader-facing error index. It had no
//!   C2-equivalent either (F12): nothing checked that a curated message HAS an
//!   entry, or that an entry still names a message the compiler prints.
//!
//! The proposals repository is not present in a CI checkout, so the ledger's
//! machine-readable half lives HERE, beside its gate:
//! `crates/vilan-cli/tests/diagnostics-ledger.tsv` — one line per ledger row,
//! `row → flagship? → key`. The prose stays in the ledger; the two are one
//! record, and a row lands in both in the change that ships the message.
//!
//! # What this file verifies
//!
//! 1. **The index is well formed.** Rows ascend, none repeats, every key has a
//!    literal fragment to search for (the two recorded exceptions below).
//! 2. **Every row still lives** ([`every_indexed_row_still_lives_in_the_tree`]).
//!    A key is split on its `{...}` slots and `...` elisions and EVERY literal
//!    fragment of [`MIN_FRAGMENT`] characters or more is searched,
//!    fixed-string, over the compiler sources and `vilan/std`. A reworded or
//!    deleted message reds the row that keys on it. This is the L13 re-key, run
//!    by the suite instead of by a lane. It searched only the LONGEST fragment
//!    until Order 38 (N100), which left the middle and the tail of 288 of the
//!    532 rows held by nothing — see [`missing_fragments`] for the widening and
//!    for why the needle is normalized.
//! 3. **Every diagnostic is rowed**
//!    ([`every_diagnostic_the_compiler_builds_is_indexed`]). Three anchor
//!    families are enumerated in full — every `Error { .. msg: <literal> }` in
//!    the compiler crates, every `errors.push(<literal>)` in `manifest.rs` (the
//!    manifest refusal family), and (N96) every `Failure::new(FailureKind::_,
//!    <literal>)` and `Failure::unsupported(<literal>)`, which is how the const
//!    channel and the macro engine refuse — and a message no row's key matches
//!    as a prefix reds, named by file, line and text. The host runtime's own
//!    sentences, reproduced verbatim for equivalence, are fenced in
//!    [`MESSAGES_THAT_ARE_THE_HOSTS`] rather than rowed.
//! 4. **Every appendix entry still names a live message**
//!    ([`every_errors_appendix_entry_still_names_a_live_message`]) — the same
//!    fixed-string search over each `**"..."**` head.
//! 5. **Every flagship row is in the appendix**
//!    ([`every_flagship_row_is_quoted_by_the_errors_appendix`]) — a row the
//!    index marks `flagship` must be quoted by some appendix entry. Dropping an
//!    entry, or marking a new row flagship without writing one, reds.
//! 6. **The curated rule statements are enumerated** (N65,
//!    [`every_rule_statement_constant_is_curated`] and
//!    [`every_curated_rule_statement_still_opens_as_recorded`]). A
//!    `ParseErrorReason::Rule` stated through a NAMED constant is a message the
//!    index cannot key on — row 229 is keyless for exactly this reason, and its
//!    count notices a site added, never a message reworded. The eighteen are
//!    named with their heads in [`CURATED_RULE_STATEMENTS`], held to the tree in
//!    both directions.
//! 7. **The exemptions expire** (N42/N50, and N27's rule for `#[ignore]`
//!    reasons applied to a list). Four checks ask the INVERSE question, each
//!    with the predicate of the check it widens: a row
//!    `ROWS_THE_ENUMERATION_CANNOT_REACH` names that the walk now reaches, a
//!    key `KEYS_WITHOUT_A_FRAGMENT` names that is now searchable, a needle
//!    `APPENDIX_HEADS_NOT_HELD` names whose entries the tree now holds
//!    literally, and a head `HEADS_THAT_ARE_NOT_MESSAGES` names that now quotes
//!    a message, are each an exemption subtracting a check for nothing — and
//!    each stays green forever without this, because a list that only ever
//!    subtracts work cannot red by being wrong.
//! 8. **No rowed literal swallows a line continuation** (N94,
//!    [`no_rowed_diagnostic_literal_swallows_a_line_continuation`]). A message
//!    whose `\`-continuations were lost keeps the source indentation as a run
//!    of spaces mid-sentence; three in a row is the threshold, over the
//!    enumeration's messages and the index's keys.
//!
//! # What this file does NOT verify
//!
//! Stated so nobody reads more into a green than is there.
//!
//! - **No verdict, pin, span or anchor is checked.** A row may say QUALIFIES
//!   and be wrong; that is an audit's job, not a gate's.
//! - **The enumeration is not the whole message surface.** A `msg:` that names
//!   a variable — a helper-built message (`removed_std_alias`,
//!   `not_callable_message`, the lexer's rule constants), a forwarded one — is
//!   not enumerated, nor are std's runtime refusals, nor CLI output text. Those
//!   rows exist and check (2) holds them, so a REWORD of one still reds; but a
//!   brand-new message in one of those families escapes check (3). The largest
//!   such family is the `let msg = if … { format!(…) }` ladder handed over by
//!   field shorthand: its rows are listed, and the choice not to widen the walk
//!   to reach them argued, on [`ROWS_THE_ENUMERATION_CANNOT_REACH`].
//! - **`Note { .. msg: .. }` sites are deliberately not rowed.** A C3 note is
//!   recorded inside its primary's row, which is the convention every ledger
//!   batch has used. They are enumerated only to be skipped.
//! - **`Failure::internal` is outside the walk, deliberately.** Its own doc
//!   calls it "a compiler or interpreter bug, not a user error", it prints
//!   under the internal-error envelope with the please-report steer, and the 46
//!   of them state semantic impossibilities no program is supposed to reach.
//!   Rowing them would fill the ledger with sentences whose audience is this
//!   repository.
//! - **A `Failure` built from a `Result<_, String>` is outside it too.** The
//!   const channel's project-file and staging helpers in `const_eval.rs` return
//!   `Err(String)` and are re-wrapped at `Err(message) => Err(Failure::new(..,
//!   message))` — a variable, not a literal, so the walk reads nothing there,
//!   exactly as it reads nothing at a helper-built `msg:`. The largest of those
//!   is `asset::staged`'s "reads the registry AFTER evaluation has finished",
//!   which is unrowed at this sha and is filed.
//! - **The `unsupported` family is rowed on its SUBJECT, not on its sentence.**
//!   `Failure::unsupported` composes `"{what} is not available at expansion
//!   time"`, and the two halves live apart, so the envelope has a row of its
//!   own and each subject has one keyed on the literal written at the site —
//!   which is what check (2) can search for. What a green therefore does NOT
//!   say is that the two halves still compose into the sentence the ledger's
//!   prose records.
//! - **Nothing here proves a message is REACHABLE.** A row whose site is dead
//!   code still passes check (2) as long as the literal is in the tree.
//!
//! Regenerating is deliberately not automated: a new message is supposed to
//! cost its author a ledger row and a verdict, and a `--fix` flag is exactly
//! how the record went three orders stale in the first place.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const INDEX: &str = "crates/vilan-cli/tests/diagnostics-ledger.tsv";
const APPENDIX: &str = "vilan/docs/appendix/errors.md";

/// The shortest literal run a fragment search will trust. Below this a
/// fragment matches by accident (` of `, `; `), which is a false green.
const MIN_FRAGMENT: usize = 8;

/// Rows whose key is entirely slots — no literal run reaches
/// [`MIN_FRAGMENT`] — with the reason. Each is a COMPOSED head: the sentence
/// the user reads is assembled from literals that live apart, so there is no
/// one string to search for. Check (3) still covers them, because the
/// enumeration finds the composing literal at its own site.
///
/// **Keyed by the row's KEY, not by its number** (N76's rule, applied to this
/// list by N96 — the sibling list has carried it since Order 33 and this one
/// had not been asked for a row since). A row number is the LEDGER's to assign
/// at integration, so a lane shipping a composed head writes `NEW` and has no
/// number to record; two lanes writing `NEW` in one order would exempt each
/// other's row. The entry is the key IN FULL and matches exactly, where
/// [`ROWS_THE_ENUMERATION_CANNOT_REACH`] takes a prefix: a fragmentless key is
/// short by construction, and one of them (`` `{name}` ``) is a PREFIX of
/// other rows' keys, so a prefix rule could not name it at all.
/// [`every_hand_rowed_row_is_in_the_index`] holds each entry to exactly one row.
const KEYS_WITHOUT_A_FRAGMENT: &[(&str, &str)] = &[
    (
        "{headline}{subject}: {}{}",
        "const evaluation's failure envelope; the two headlines (`const \
         evaluation failed`, `const evaluation did not finish within the \
         compile-time budget`) are separate literals in `const_eval.rs`, and \
         G24's `const let` steer is a third",
    ),
    // N96's three, and all three are one shape: a host capability the
    // expansion environment does not have, named by the SPELLING the program
    // wrote. There is nothing else in the sentence — `Failure::unsupported`
    // supplies the rest — so the key is a pair of backticks around a slot,
    // and an exact match against the site's own literal is the whole of what
    // can be held. Which is enough: each names exactly the sites that write
    // it, and a fourth spelling of the same shape would land unrowed rather
    // than be swallowed by one of these.
    (
        "`{helper}`",
        "the impure-helper refusal — a `std` helper the const subset excludes",
    ),
    (
        "`{name}`",
        "the host-global and host-call refusals (`__scan`, `fetch`, \
         `setTimeout`, `__timer`, …), which name the binding and nothing else",
    ),
    (
        "`{base}.{method}`",
        "the host-METHOD refusal, which names receiver and method the same way",
    ),
];

/// Rows the enumeration cannot reach, ROWED BY HAND, with why the walk does
/// not get to each (N41).
///
/// **Keyed by the row's KEY, not by its number** (N76). A row number is the
/// LEDGER's to assign at integration, so a lane that ships a message the walk
/// cannot read had nowhere to record it: its row is `NEW` until the merge, and
/// a second lane's `NEW` row in the same order would make an entry spelled
/// `"NEW"` name the wrong one. The key is the row's own text and is written in
/// the same commit as the message, so an entry here lands with the row it
/// exempts and survives the renumbering untouched — the integrator's
/// `apply_row_mapping.py` has nothing to rewrite in this block.
///
/// An entry's first field is a PREFIX of the key as `diagnostics-ledger.tsv`
/// spells it, long enough to name exactly one row;
/// [`every_hand_rowed_row_is_in_the_index`] resolves it and reds on a prefix
/// that matches no row or several. Long enough is not 60 characters, which is
/// what the batch-7 generation cut heads at and what this list was first
/// imagined carrying: three of the `+` ladder's arms share their first 64
/// characters and two more share 60, so the prefixes below run to whatever it
/// takes to separate them. A prefix that goes ambiguous later reds rather than
/// silently exempting the wrong row, which is the direction this list has to
/// fail in.
///
/// `anchored_messages` reads the literal written AT its anchor. The `+`
/// operator's refusal ladder writes none: it builds its six arms into a
/// `let msg = if … { format!(…) } else if …`, then hands the binding over by
/// field shorthand (`Error { .., msg }`), which is neither the `msg:` anchor
/// nor a literal. So check (3) has never seen any of the six — the family the
/// errors appendix documents most heavily.
///
/// Extending the walk one assignment upstream was the alternative and is NOT
/// what shipped. It would reach these six, and with them every arm of the
/// eighteen other `let msg` ladders in `analyzer.rs` and its neighbours —
/// scores of messages, each owing a ledger row whose PROSE lives in the
/// proposals repository. Landing the index half here without the prose half
/// there splits the one record this file's header says must land together, so
/// the walk stays where it is and the six are rowed by hand instead. The hole
/// is now named rather than silent, and [`every_hand_rowed_row_is_in_the_index`]
/// keeps the naming honest.
const ROWS_THE_ENUMERATION_CANNOT_REACH: &[(&str, &str)] = &[
    (
        "`+` on `str` concatenates, and `{rhs_label}` has no string form: a \
         parameter promises only what its bounds promise, and this",
        "the `+` ladder's unbounded-parameter concatenation arm (`analyzer.rs`)",
    ),
    (
        "`+` adds two values of the same type, but the operands are \
         `{lhs_label}` and `{rhs_label}`: `{rhs_label}`",
        "its B179 arm — a parameter right of a number's `+`",
    ),
    (
        "`+` on `str` concatenates, and `{rhs_label}` has no string form: a \
         parameter promises only what its bounds promise, and no",
        "its B176 arm — bounded, but to the wrong promise",
    ),
    (
        "`+` on `str` concatenates, and `{rhs_label}` has no string form: \
         concatenating",
        "its plain no-string-form arm",
    ),
    (
        "`+` on `{lhs_label}` adds, and `str` is not a number: only a",
        "its `str`-on-the-right arm",
    ),
    (
        "`+` adds two values of the same type, but the operands are \
         `{lhs_label}` and `{rhs_label}`: there",
        "its same-type arm, the ladder's fallthrough",
    ),
    // B200's unary ladders, built the same way and handed over by the same
    // field shorthand — one `let msg = if … else if …` per operator. Their
    // arms differ in HEAD, not just in a slot (`-` on a native non-numeric
    // states an admitted set; on an aggregate it states that no `Neg` trait
    // exists to give the symbol a meaning; on a parameter it states why no
    // bound can prove membership), so folding them into one templated
    // `msg:` literal the enumeration could read would cost the sentences
    // their accuracy. Rowed by hand instead, as the `+` ladder above is.
    (
        "`!` negates a `bool`, and `{label}` is a type parameter: `bool`'s",
        "`!`'s parameter arm (`analyzer.rs`)",
    ),
    (
        "`!` negates a `bool`, and this operand is `void`: the expression",
        "`!`'s `void` arm",
    ),
    (
        "`!` negates a `bool`, and this operand is `{label}`: the host's",
        "`!`'s truthiness arm — every other non-`bool` operand",
    ),
    (
        "`-` negates a number, and `{label}` is a type parameter: the",
        "`-`'s parameter arm",
    ),
    (
        "`-` on `bool` has no meaning: `bool`'s admitted unary operator",
        "`-`'s `bool` arm",
    ),
    (
        "`-` on `str` has no meaning: `str`'s admitted operators are `+",
        "`-`'s `str` arm",
    ),
    (
        "`-` on `{label}` has no meaning: `{label}`'s admitted operators",
        "`-`'s backed-enum arm",
    ),
    (
        "`-` negates a number, and this operand is `void`: the expression",
        "`-`'s `void` arm",
    ),
    (
        "`-` negates a number, and `{label}` is not one: vilan has no",
        "`-`'s no-`Neg`-trait arm — every other non-numeric operand",
    ),
    // B197's operator-conformance refusal shares the missing-member push with
    // the ordinary one, choosing between them in a `let msg = if …` the walk
    // cannot read for the same reason.
    (
        "`impl {subject_name} with {trait}`{inherited} provides no \
         `{member_name}`:",
        "the operator arm of the trait-conformance refusal (`analyzer.rs`)",
    ),
    (
        "`!` negates a `bool`, and `{label}` is a trait: no trait names",
        "`!`'s trait-typed arm",
    ),
    (
        "`-` negates a number, and `{label}` is a trait: no trait names",
        "`-`'s trait-typed arm",
    ),
];

/// Messages the widened walk reads that are the HOST RUNTIME's own words
/// rather than this compiler's, with the JavaScript error each reproduces.
///
/// The interpreter's contract is behavioural equivalence with the emitted JS
/// (AGENTS.md's first invariant), so where a `const` expression hits a limit
/// JavaScript itself refuses, it throws JavaScript's sentence verbatim and the
/// two backends fail alike. Rowing one would put V8's wording in a ledger
/// whose whole subject is the wording this project chose, and a REWORD here
/// would be a bug rather than a re-key — which is the opposite of what a row
/// is for. Fenced by their exact text, so a change to any of them reds
/// [`every_hosts_message_is_still_thrown`] instead of passing silently.
///
/// This is not the same exemption as `ROWS_THE_ENUMERATION_CANNOT_REACH`:
/// those are the tree's own messages the walk cannot READ. These the walk
/// reads perfectly well and the ledger does not want.
const MESSAGES_THAT_ARE_THE_HOSTS: &[(&str, &str)] = &[
    (
        "Invalid count value",
        "V8's RangeError from `String.prototype.repeat`",
    ),
    (
        "Division by zero",
        "V8's RangeError from BigInt `/` and `%`",
    ),
    (
        "Do not know how to serialize a BigInt",
        "V8's TypeError from `JSON.stringify`",
    ),
];

/// Ledger rows with no key at all, and so absent from the index, with the
/// reason. Both are recorded in the ledger itself.
const ROWS_WITHOUT_A_KEY: &[(&str, &str)] = &[
    (
        "154",
        "a forwarding push — the message arrives already built",
    ),
    (
        "229",
        "the `ParseErrorReason::Rule` statements, held by the parse-rule tests",
    ),
];

/// How many sites in `parsing.rs` CONSTRUCT a `ParseErrorReason::Rule`, which is
/// the population row 229 stands for.
///
/// The note above used to carry the number in prose, and prose does not red: it
/// said "nine" — the count at Order 22, when row 206 founded the class — through
/// every arc that added one, and the ledger row itself had already been rewritten
/// to "twenty-two by Order 29" without the note noticing (tracker N60). So the
/// number lives here, where [`the_keyless_row_still_counts_the_rule_sites`] holds
/// it to the tree, and the note names no count at all.
///
/// Construction sites only. `parsing.rs` mentions the variant three more times —
/// the `match &error.reason` arm that RENDERS one, and two doc comments — and a
/// mention is not a statement. Three of them carry a caller-supplied
/// `&'static str` rather than a literal (the lexer's rule constants come through
/// one of them), which is why the enumeration in check (3) cannot reach the row
/// and why it is keyless in the first place.
const RULE_STATEMENT_SITES: usize = 44;

/// The one literal run of the resource-derive refusal that is neither a slot
/// nor assembled: what its ledger row is keyed on, and what
/// [`the_helper_built_resource_derive_refusal_is_rowed`] holds in both
/// directions (N65).
const RESOURCE_DERIVE_REFUSAL_FRAGMENT: &str =
    "`: a resource is an owned handle, not plain data — ";

/// The CURATED rule statements: the messages a `ParseErrorReason::Rule` states
/// through a NAMED constant rather than a literal written at the site — the
/// `css` block's five, the two statement-shape steers, the seven nesting
/// refusals, and the lexer's five, which reach `parsing.rs` through
/// `LexError::rule` from another file entirely (N65).
///
/// This is row 229's twin, and the half a COUNT cannot be. The row is keyless
/// because the enumeration in check (3) reads the literal written AT its
/// anchor, and every one of these is at least one indirection away;
/// [`RULE_STATEMENT_SITES`] stands in for the family by counting its SITES,
/// which notices a rule statement added or removed and cannot notice a
/// reworded one. E153 rewrote the `:hover` rule — `CSS_PSEUDO_CLASS_IS_DOTTED`,
/// three lines below — and had no row to edit, which is the shape of the item:
/// these were unrowed messages, never red ones.
///
/// So each is named here with the head it opens with, [`every_curated_rule_
/// statement_still_opens_as_recorded`] holds the head to the tree, and
/// [`every_rule_statement_constant_is_curated`] holds the LIST to the tree in
/// the other direction. A reworded head reds this row, which is the row to
/// edit; a new curated constant reds until it has one.
///
/// The head, not the whole sentence, for the index's own reason: a ledger key
/// is a message HEAD (the file's header says so, and the batch-7 generation's
/// keys are truncated at sixty characters). A head long enough to be
/// unmistakable is the bar, and a tail edit that leaves the head standing
/// still lands on a row that names the constant.
const CURATED_RULE_STATEMENTS: &[(&str, &str, &str)] = &[
    (
        "crates/vilan-core/src/parsing.rs",
        "CSS_IS_A_KEYWORD",
        "`css` is a keyword: it begins a `css { … }` block.",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "CSS_BLOCK_IS_BRACE_INITIAL",
        "a `css { … }` block is brace-initial,",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "A_PATH_CANNOT_START_AT_A_BLOCK",
        "`::` reaches into a NAMESPACE — a module, a type or an enum — so what",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "CSS_PSEUDO_CLASS_IS_DOTTED",
        "a `css` block writes a pseudo-class as a DOTTED rule:",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "CONST_HAS_NO_MUTATION",
        "a compile-time value has no runtime mutation:",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "IMPORTANT_HAS_NO_PLACE",
        "`!important` has no place in a `css` block:",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "A_CSS_DECLARATION_IS_A_CALL",
        "a `css` declaration is a CALL: write `padding(space(4));`,",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "IMPL_SELECTOR_IS_A_BRACE_ELEMENT",
        "an `impl` selector is a brace-set ELEMENT, because it selects",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "IMPL_SELECTOR_SHAPE",
        "an `impl` selector is `(impl TYPE)`, optionally followed by",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "IMPL_SELECTOR_TAKES_NO_BINDER",
        "an `impl` selector writes no binders: it filters blocks that",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "IMPL_SELECTOR_REFUSES_AS",
        "an `impl` selector takes no `as`: a method is called by NAME on a",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "USE_TAKES_NO_IMPL_SELECTOR",
        "an `impl` selector belongs to `import`: it says which of a",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "USE_TAKES_NO_ONLY",
        "`only` belongs to `import`: it drops the implementations an import",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "BLOCK_LIKE_STATEMENT_IS_COMPLETE",
        "a `match`, `if`, `for` or `{` form is COMPLETE at its closing brace,",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "EXPORT_TAKES_AN_ITEM",
        "`export` takes an ITEM — a `fun`, `struct`, `enum`, `trait`, `impl`,",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "IMPORT_PATH_IS_NAMES_AND_SETS",
        "an `import`/`use` path is `::`-separated NAMES, ending in a name",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "LET_MUT_IS_ONE_WORD",
        "a mutable binding is spelled `mut x = …`:",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "PATTERN_BINDER_IS_ONE_WORD",
        "a pattern binds mutably with `mut x`:",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "NESTING_REFUSAL",
        "this expression nests more than 500 levels deep, which parsing refuses;",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "TYPE_NESTING_REFUSAL",
        "this type nests more than 500 levels deep, which parsing refuses;",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "PATTERN_NESTING_REFUSAL",
        "this pattern nests more than 500 levels deep, which parsing refuses;",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "ITEM_NESTING_REFUSAL",
        "this declaration nests more than 500 levels deep, which parsing refuses;",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "IMPORT_NESTING_REFUSAL",
        "this import path nests more than 500 levels deep, which parsing refuses;",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "ELEMENT_NESTING_REFUSAL",
        "this element nests more than 500 levels deep, which parsing refuses;",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "CSS_NESTING_REFUSAL",
        "this `css` block nests more than 500 levels deep, which parsing refuses;",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "DOC_HIDDEN_IS_SUPERSEDED",
        "`[doc(hidden)]` is superseded by visibility:",
    ),
    (
        "crates/vilan-core/src/parsing.rs",
        "MISBOUND_RETURN_CLAUSE",
        "a `context` clause after an UN-PARENTHESIZED closure return type binds to the",
    ),
    (
        "crates/vilan-core/src/lexing.rs",
        "AT_IS_NOT_A_TOKEN",
        "`@` is not a vilan token;",
    ),
    (
        "crates/vilan-core/src/lexing.rs",
        "HOLE_IS_NOT_AN_EXPRESSION",
        "an interpolation hole holds one expression, and `{` and `}` delimit it:",
    ),
    (
        "crates/vilan-core/src/lexing.rs",
        "UNESCAPED_BRACE",
        "a literal `}` inside an interpolated string is written `\\}`:",
    ),
    (
        "crates/vilan-core/src/lexing.rs",
        "LINE_BREAK_IN_STRING",
        "a string cannot span lines unless it is triple-quoted:",
    ),
];

/// Where a constant's name has to appear for its text to be a rule statement:
/// the `Rule` construction itself, the lexer's `rule` field (written both as
/// `Some(..)` and as a `match` over the character), and the three helpers that
/// take one as a parameter and state it themselves.
const RULE_STATEMENT_CONTEXTS: &[&str] = &[
    "ParseErrorReason::Rule(",
    "rule: Some(",
    "rule: match",
    "parse_nested(",
    "parse_nested_as(",
    "refuse_nesting(",
];

/// How far back from a mention the context may sit. A `parse_nested_as(` call
/// puts its refusal on the next line under rustfmt, and the lexer's `match`
/// arm sits two lines under `rule:`; 120 characters covers both and is short
/// enough that an unrelated `Rule(` up the file cannot reach.
const RULE_CONTEXT_WINDOW: usize = 120;

/// Appendix heads that cannot be held against the tree literally, with the
/// reason each cannot. Every one is a COMPOSED head — the entry quotes a
/// sentence the compiler assembles from two or more literals — so the fixed
/// string the entry shows never appears anywhere as one run. They are listed
/// here rather than reworded because the composed form is what the reader
/// actually sees.
const APPENDIX_HEADS_NOT_HELD: &[(&str, &str)] = &[
    (
        "expects N arguments, but got M instead",
        "`{head}: `{}` is missing.` — the head and the tail are two literals \
         (`analyzer.rs`), and `N`/`M` stand in for counts the head renders",
    ),
    (
        "expects N fields, but got M instead",
        "the struct-literal flavor of the row above, composed the same way",
    ),
    (
        "which is async: a module-level binding cannot await",
        "`async_infer.rs` builds `calls `{name}`, which is async` as the \
         `{culprit}` slot of the refusal that follows it",
    ),
    (
        "const evaluation failed in",
        "`{headline}{subject}: {}` — the headline and the ` in `{f}`` subject \
         are separate literals (`const_eval.rs`); see KEYS_WITHOUT_A_FRAGMENT",
    ),
    (
        "const evaluation did not finish within the compile-time budget in",
        "the budget headline of the same envelope, composed the same way",
    ),
];

/// Appendix entries whose head names a CONDITION rather than quoting a
/// message, with the reason. Each documents something a reader arrives with
/// that has no one sentence to quote — a runtime error variant, a parse
/// outcome — so the entry heads it the way the reader meets it.
const HEADS_THAT_ARE_NOT_MESSAGES: &[(&str, &str)] = &[
    (
        "`RpcError::Contract` at connect time",
        "an error VARIANT the caller matches on, not a compiler diagnostic",
    ),
    (
        "`RpcError::Transport(\"not connected\")`",
        "two transport variants, documented together",
    ),
    (
        "A struct literal in a condition parses as the block",
        "a parse outcome with no message of its own — the refusal it \
         produces varies with what follows",
    ),
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves")
}

fn read(relative: &str) -> String {
    let path = repository_root().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn walk(directory: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut sorted: Vec<PathBuf> = entries.filter_map(|e| e.ok()).map(|e| e.path()).collect();
    sorted.sort();
    for path in sorted {
        if path.is_dir() {
            walk(&path, extension, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}

/// Every source a diagnostic's text can live in: the compiler crates and the
/// standard library (whose runtime refusals the ledger rows from 230 on).
fn message_sources() -> Vec<PathBuf> {
    let root = repository_root();
    let mut paths = Vec::new();
    for crate_directory in [
        "vilan-core",
        "vilan-cli",
        "vilan-lsp",
        "vilan-wasm",
        "vilan-rust",
    ] {
        walk(
            &root.join("crates").join(crate_directory).join("src"),
            "rs",
            &mut paths,
        );
    }
    walk(&root.join("vilan/std/src"), "vl", &mut paths);
    walk(&root.join("vilan/macro_std"), "vl", &mut paths);
    paths
}

/// A source, with the escapes that stand between a written literal and the
/// string it denotes removed — so a fixed-string search finds a message that
/// the source spells across several lines.
///
/// The line CONTINUATION is the one that matters: Rust's `\` + newline +
/// indentation swallows both, so a long format string is one run in the
/// program and four lines in the file. Batch 8's and L13's scans triaged
/// exactly this artifact by hand, over and over.
fn normalized(text: &str) -> String {
    let bytes: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == '\\' && index + 1 < bytes.len() {
            match bytes[index + 1] {
                '\n' | '\r' => {
                    index += 2;
                    while index < bytes.len()
                        && (bytes[index] == ' '
                            || bytes[index] == '\t'
                            || bytes[index] == '\n'
                            || bytes[index] == '\r')
                    {
                        index += 1;
                    }
                    continue;
                }
                '"' => {
                    out.push('"');
                    index += 2;
                    continue;
                }
                '\\' => {
                    out.push('\\');
                    index += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    // `{{` / `}}` are how a format string writes one brace.
    out.replace("{{", "{").replace("}}", "}")
}

fn source_blob() -> String {
    let mut blob = String::new();
    for path in message_sources() {
        blob.push_str(&normalized(
            &std::fs::read_to_string(&path).unwrap_or_default(),
        ));
        blob.push('\n');
    }
    blob
}

// --- The index -------------------------------------------------------------

struct Row {
    /// The id AS WRITTEN: a ledger row number, or the placeholder `NEW`. A
    /// change that ships a message rows it here in the same commit, but the
    /// NUMBER is the ledger's to assign at integration — two lanes in one order
    /// that each minted "the next one" would both be wrong, and the collision
    /// only shows up at the merge. So a lane writes `NEW` and integration
    /// numbers it; everything below holds a `NEW` row exactly as it holds a
    /// numbered one, except the ordering it has no place in yet.
    number: String,
    /// The id as an ordinal, for the ascending check — `None` for `NEW`.
    ordinal: Option<u32>,
    flagship: bool,
    key: String,
}

fn index() -> Vec<Row> {
    let text = read(INDEX);
    let mut rows = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') || line.trim().is_empty() || line.starts_with("row\t") {
            continue;
        }
        let mut fields = line.splitn(3, '\t');
        let number = fields.next().expect("a row number");
        let flagship = fields.next().expect("a flagship column");
        let key = fields.next().expect("a key");
        rows.push(Row {
            number: number.to_string(),
            ordinal: match number {
                NEW_ROW => None,
                other => Some(
                    other
                        .parse()
                        .unwrap_or_else(|_| panic!("row number: {other:?}")),
                ),
            },
            flagship: match flagship {
                "flagship" => true,
                "-" => false,
                other => {
                    panic!("row {number}: the flagship column is `flagship` or `-`, not {other:?}")
                }
            },
            key: key.to_string(),
        });
    }
    rows
}

/// The id a row carries until the ledger numbers it.
const NEW_ROW: &str = "NEW";

/// One piece of a key: a literal run, or a slot the message fills in.
enum Piece {
    Literal(String),
    Slot,
}

/// Splits a key on its `{...}` slots and `...` elisions. A key truncated
/// mid-slot (the batch-7 generation cut heads at 60 characters) ends in an
/// unterminated `{`, which reads as a slot — the truncation is an elision.
fn pieces(key: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut chars = key.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '…' => {
                pieces.push(Piece::Literal(std::mem::take(&mut current)));
                pieces.push(Piece::Slot);
            }
            '{' => {
                pieces.push(Piece::Literal(std::mem::take(&mut current)));
                pieces.push(Piece::Slot);
                for inner in chars.by_ref() {
                    if inner == '}' {
                        break;
                    }
                }
            }
            _ => current.push(character),
        }
    }
    pieces.push(Piece::Literal(current));
    pieces
}

fn fragments(key: &str) -> Vec<String> {
    pieces(key)
        .into_iter()
        .filter_map(|piece| match piece {
            Piece::Literal(text) => {
                let trimmed = text.trim().to_string();
                (trimmed.chars().count() >= MIN_FRAGMENT).then_some(trimmed)
            }
            Piece::Slot => None,
        })
        .collect()
}

fn longest_fragment(key: &str) -> Option<String> {
    fragments(key).into_iter().max_by_key(|f| f.chars().count())
}

/// The fragments of `key` that [`source_blob`] does not carry — check (2),
/// widened by N100 from the LONGEST fragment to EVERY one of them.
///
/// Until this order the search took `longest_fragment` alone, so a reword
/// inside any shorter run left its row green: 288 of the index's 532 rows carry
/// more than one searchable fragment, which is 54 % of the ledger whose middle
/// and tail were held by nothing. Widening costs nothing — the blob is built
/// once and a `contains` is a scan — and it is the only half of check (2) that
/// was ever weaker than its own doc comment claimed ("its literal runs appear
/// in order").
///
/// The needle is [`normalized`] the same way the haystack was, and that is not
/// cosmetic. A key is written in the FORMAT-STRING spelling of the message it
/// records — `{name}` for a slot, `{{` for one literal brace — because check
/// (3) matches it against the literal as WRITTEN at its site. `source_blob`
/// collapses `{{` to `{`, so a needle carrying the doubled brace matches
/// nothing. Exactly two rows spell one (494 and 499, the marked-import steer
/// `import {module}::{{ #{leaf} }};`), and they were the only two rows the
/// widening would have reddened: not a wrong key — a needle nobody had
/// prepared. Normalizing AFTER [`pieces`] has split the key is what keeps the
/// doubled brace from being read as a slot on the way in.
fn missing_fragments(key: &str, blob: &str) -> Vec<String> {
    fragments(key)
        .into_iter()
        .filter(|fragment| !blob.contains(&normalized(fragment)))
        .collect()
}

/// Whether `key` describes `message`: its literal runs appear in order from the
/// message's start, with each slot free to swallow anything. A key is a
/// PREFIX of the head it records (the ledger truncates; it never starts in the
/// middle), so the first run is anchored unless a slot leads.
fn key_describes(key: &str, message: &str) -> bool {
    let mut position = 0usize;
    let mut may_skip = false;
    for piece in pieces(key) {
        match piece {
            Piece::Slot => may_skip = true,
            Piece::Literal(run) if run.is_empty() => {}
            Piece::Literal(run) => {
                if may_skip {
                    match message[position..].find(&run) {
                        Some(offset) => position += offset + run.len(),
                        None => return false,
                    }
                } else if message[position..].starts_with(&run) {
                    position += run.len();
                } else {
                    return false;
                }
                may_skip = false;
            }
        }
    }
    true
}

// --- The enumeration -------------------------------------------------------

struct Site {
    file: String,
    line: usize,
    message: String,
    is_note: bool,
}

/// Reads the Rust string literal that starts at `start` (which must be its
/// opening quote), resolving the escapes that change what the program prints.
fn string_literal(text: &[char], start: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut index = start + 1;
    while index < text.len() {
        match text[index] {
            '\\' => {
                let next = *text.get(index + 1)?;
                match next {
                    '\n' | '\r' => {
                        index += 2;
                        while index < text.len() && matches!(text[index], ' ' | '\t' | '\n' | '\r')
                        {
                            index += 1;
                        }
                    }
                    'n' => {
                        out.push('\n');
                        index += 2;
                    }
                    't' => {
                        out.push('\t');
                        index += 2;
                    }
                    other => {
                        out.push(other);
                        index += 2;
                    }
                }
            }
            '"' => return Some((out, index + 1)),
            character => {
                out.push(character);
                index += 1;
            }
        }
    }
    None
}

/// Every literal message at `anchor` in one file. The literal must follow the
/// anchor with nothing between but whitespace and a builder call (`format!(`,
/// `String::from(`) — an anchor whose message comes from a HELPER is skipped,
/// deliberately: this enumeration only claims the sites where the sentence is
/// written at the site. (`errors.push(reserved_name_refusal(..))` is the shape
/// that would otherwise record an argument as a message.)
fn anchored_messages(path: &Path, anchor: &str) -> Vec<Site> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let chars: Vec<char> = text.chars().collect();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();
    let anchor_chars: Vec<char> = anchor.chars().collect();
    let mut sites = Vec::new();
    let mut line = 1usize;
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '\n' {
            line += 1;
        }
        if chars[index..].starts_with(anchor_chars.as_slice()) {
            let mut cursor = index + anchor_chars.len();
            let mut ok = true;
            loop {
                match chars.get(cursor) {
                    Some(' ' | '\t' | '\n' | '\r') => cursor += 1,
                    Some('"') => break,
                    Some(_) => {
                        let rest: String = chars[cursor..(cursor + 14).min(chars.len())]
                            .iter()
                            .collect();
                        if let Some(builder) = ["format!(", "String::from("]
                            .into_iter()
                            .find(|builder| rest.starts_with(builder))
                        {
                            cursor += builder.chars().count();
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && let Some((message, _)) = string_literal(&chars, cursor) {
                let before: String = chars[..index].iter().collect();
                let is_note = before.rfind("Note {") > before.rfind("Error {");
                sites.push(Site {
                    file: name.clone(),
                    line,
                    message,
                    is_note,
                });
            }
        }
        index += 1;
    }
    sites
}

/// The const channel's refusals: `Failure::new(FailureKind::_, <literal>)` and
/// `Failure::unsupported(<literal>)`.
///
/// Its own walk rather than one more [`anchored_messages`] anchor, because
/// `Failure::new`'s message is its SECOND argument, behind a `FailureKind::…`
/// the reader has to step over.
///
/// `Failure::unsupported`'s literal is a COMPOSED head, in exactly the sense
/// [`KEYS_WITHOUT_A_FRAGMENT`] means it: the constructor is
/// `format!("{} is not available at expansion time", what)`, so the sentence
/// the user reads is the site's literal followed by an envelope that lives in
/// one place. The site's own literal is what is enumerated and what a row is
/// keyed on — the envelope gets a row of its own — because that is the half
/// check (2) can search for. Keying on the whole sentence would key on a run
/// that appears nowhere in the tree, and every row in the family would go
/// stale the day it landed.
///
/// `Failure::internal` is deliberately absent: its own doc calls it "a
/// compiler or interpreter bug, not a user error", it prints under the
/// internal-error envelope with the please-report steer, and rowing 46 of
/// those would fill the ledger with sentences no program is supposed to
/// reach.
fn failure_messages(path: &Path) -> Vec<Site> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let chars: Vec<char> = text.chars().collect();
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("?")
        .to_string();
    let mut sites = Vec::new();
    let mut line = 1usize;
    let mut index = 0usize;
    while index < chars.len() {
        if chars[index] == '\n' {
            line += 1;
        }
        for anchor in ["Failure::unsupported(", "Failure::new("] {
            let anchor_chars: Vec<char> = anchor.chars().collect();
            if !chars[index..].starts_with(anchor_chars.as_slice()) {
                continue;
            }
            let mut cursor = index + anchor_chars.len();
            let mut ok = true;
            loop {
                match chars.get(cursor) {
                    Some(' ' | '\t' | '\n' | '\r') => cursor += 1,
                    Some('"') => break,
                    Some(_) => {
                        let rest: String = chars[cursor..(cursor + 24).min(chars.len())]
                            .iter()
                            .collect();
                        if let Some(builder) = ["format!(", "String::from("]
                            .into_iter()
                            .find(|builder| rest.starts_with(builder))
                        {
                            cursor += builder.chars().count();
                        } else if rest.starts_with("FailureKind::") {
                            // The kind, and the comma after it.
                            match chars[cursor..].iter().position(|c| *c == ',') {
                                Some(offset) => cursor += offset + 1,
                                None => {
                                    ok = false;
                                    break;
                                }
                            }
                        } else {
                            ok = false;
                            break;
                        }
                    }
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            if ok && let Some((message, _)) = string_literal(&chars, cursor) {
                sites.push(Site {
                    file: name.clone(),
                    line,
                    message,
                    is_note: false,
                });
            }
        }
        index += 1;
    }
    sites
}

/// The message surface this file claims to enumerate — see the header for what
/// it deliberately leaves out.
fn enumerated_sites() -> Vec<Site> {
    let root = repository_root();
    let mut sites = Vec::new();
    for crate_directory in [
        "vilan-core",
        "vilan-cli",
        "vilan-lsp",
        "vilan-wasm",
        "vilan-rust",
    ] {
        let mut paths = Vec::new();
        walk(
            &root.join("crates").join(crate_directory).join("src"),
            "rs",
            &mut paths,
        );
        for path in paths {
            sites.extend(anchored_messages(&path, "msg:"));
            sites.extend(failure_messages(&path));
        }
    }
    sites.extend(anchored_messages(
        &root.join("crates/vilan-core/src/manifest.rs"),
        "errors.push(",
    ));
    sites
}

// --- The appendix ----------------------------------------------------------

/// Every quoted message form in `errors.md`, with the line its entry starts on.
/// An entry's head is one or more `**"..."**` runs and may wrap across lines.
fn appendix_heads() -> Vec<(usize, String)> {
    let text = read(APPENDIX);
    let lines: Vec<&str> = text.lines().collect();
    let mut heads = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        if lines[index].starts_with("**\"") {
            let mut block = lines[index].to_string();
            let mut last = index;
            while !block.contains("\"**") && last + 1 < lines.len() {
                last += 1;
                block.push(' ');
                block.push_str(lines[last].trim());
            }
            let mut rest = block.as_str();
            while let Some(open) = rest.find("**\"") {
                let after = &rest[open + 3..];
                match after.find("\"**") {
                    Some(close) => {
                        heads.push((index + 1, unescape_markdown(&after[..close])));
                        rest = &after[close + 3..];
                    }
                    None => break,
                }
            }
            index = last + 1;
        } else {
            index += 1;
        }
    }
    heads
}

fn unescape_markdown(head: &str) -> String {
    head.replace("\\`", "`")
        .replace("\\\"", "\"")
        .replace("\\*", "*")
}

/// An appendix head's literal runs. Holes are `...`, `{...}`, and — in the
/// second pass — code spans, because a head may quote an ILLUSTRATIVE filling
/// (`` `f` ``, `` `Name` ``, `` `&&` ``) where the message has a slot.
fn appendix_fragments(head: &str, code_spans_are_holes: bool) -> Vec<String> {
    let mut prepared = head.to_string();
    if code_spans_are_holes {
        let mut out = String::new();
        let mut inside = false;
        for character in prepared.chars() {
            if character == '`' {
                inside = !inside;
                out.push('…');
            } else if !inside {
                out.push(character);
            }
        }
        prepared = out;
    }
    fragments(&prepared)
        .into_iter()
        .map(|f| {
            f.trim_matches(|c| c == '`' || c == '\'' || c == ' ')
                .to_string()
        })
        .filter(|f| f.chars().count() >= MIN_FRAGMENT)
        .collect()
}

/// Whether `line` opens an appendix ENTRY: a line that is nothing but bold
/// (plus an optional italic aside). A bold word leading a sentence of prose is
/// not one.
fn is_entry_head(line: &str) -> bool {
    line.starts_with("**")
        && line
            .trim_end()
            .trim_end_matches([')', '*', '('])
            .ends_with("**")
        || line.starts_with("**") && line.trim_end().ends_with("**")
}

/// Whether the tree holds `head` literally: its runs as written, or — second
/// pass — with its code spans read as holes, because a head may quote an
/// illustrative filling where the message has a slot. One predicate, so the
/// check that reports a stale entry and the check that reports a stale
/// EXEMPTION cannot drift apart.
fn appendix_head_is_held(head: &str, blob: &str) -> bool {
    appendix_fragments(head, false)
        .iter()
        .all(|fragment| blob.contains(fragment))
        || appendix_fragments(head, true)
            .iter()
            .all(|fragment| blob.contains(fragment))
}

// --- The gates -------------------------------------------------------------

#[test]
fn the_index_is_well_formed() {
    let rows = index();
    assert!(rows.len() > 300, "the index lost rows: {}", rows.len());
    let mut previous = 0u32;
    for row in &rows {
        // A `NEW` row has no ordinal yet, so it sits outside the order rather
        // than breaking it. Everything else below still applies to it.
        if let Some(ordinal) = row.ordinal {
            assert!(
                ordinal > previous,
                "row {} is out of order (after {previous})",
                row.number
            );
            previous = ordinal;
        }
        assert!(
            !row.key.trim().is_empty(),
            "row {} has an empty key",
            row.number
        );
        let exempt = KEYS_WITHOUT_A_FRAGMENT
            .iter()
            .any(|(key, _)| *key == row.key);
        assert!(
            exempt || longest_fragment(&row.key).is_some(),
            "row {}'s key has no literal run of {MIN_FRAGMENT} characters to search for: {:?}.\n\
             Either the key is wrong, or the head is COMPOSED — record it in \
             KEYS_WITHOUT_A_FRAGMENT with the reason.",
            row.number,
            row.key
        );
    }
    for (number, _) in ROWS_WITHOUT_A_KEY {
        assert!(
            !rows.iter().any(|row| row.number == *number),
            "row {number} is recorded as keyless but appears in the index"
        );
    }
}

#[test]
fn every_indexed_row_still_lives_in_the_tree() {
    let blob = source_blob();
    let mut stale = Vec::new();
    for row in index() {
        let missing = missing_fragments(&row.key, &blob);
        if missing.is_empty() {
            continue;
        }
        let named: Vec<String> = missing
            .iter()
            .map(|fragment| format!("{fragment:?}"))
            .collect();
        stale.push(format!(
            "  row {}: {:?}\n      not in the tree: {}",
            row.number,
            row.key,
            named.join(", ")
        ));
    }
    assert!(
        stale.is_empty(),
        "{} ledger row(s) key on text the tree no longer carries. A reworded \
         message owes its row a re-key, in the change that rewords it \
         (diagnostics-standard.md §5's standing rule):\n{}",
        stale.len(),
        stale.join("\n")
    );
}

/// N100: check (2) reds on a reword inside a fragment that is NOT the longest.
///
/// The fixture is built so the old search passes and the new one fails, which
/// is the whole of what the widening bought: the long run is in the blob, the
/// short one is not.
#[test]
fn n100_a_reword_of_a_fragment_that_is_not_the_longest_reds_its_row() {
    let key = "{subject} alpha bravo charlie {member} delta echo foxtrot golf hotel";
    let blob = "a tree that carries delta echo foxtrot golf hotel and nothing else";

    let longest = longest_fragment(key).expect("the fixture key has a fragment");
    assert_eq!(
        longest, "delta echo foxtrot golf hotel",
        "the fixture's longest fragment is the one the blob keeps"
    );
    assert!(
        blob.contains(&longest),
        "the fixture must keep the LONGEST fragment in the blob — otherwise the \
         pin would red under the single-fragment search too and would prove \
         nothing about the widening"
    );
    assert_eq!(
        missing_fragments(key, blob),
        vec!["alpha bravo charlie".to_string()],
        "the widened search names the short run the blob does not carry"
    );
}

/// N100's other half: a key spelling a LITERAL brace the way a format string
/// does (`{{`) is searched as [`source_blob`] spells it (`{`).
///
/// Rows 494 and 499 are the two that do, both quoting B318's marked import.
/// Without the normalization their tail fragments are needles no tree can ever
/// hold, and the widening above would have reddened them on the day it landed.
#[test]
fn n100_a_key_written_with_a_doubled_brace_is_searched_as_the_blob_spells_it() {
    let key = "reach it — `import {module}::{{ #(impl {subject}) }};` — or write `export`";
    let blob = normalized(key);

    assert!(
        fragments(key)
            .iter()
            .any(|fragment| fragment.contains("}}") && !blob.contains(fragment.as_str())),
        "the fixture must carry a doubled-brace fragment the normalized blob \
         cannot match verbatim, or the pin proves nothing"
    );
    assert!(
        missing_fragments(key, &blob).is_empty(),
        "every fragment of a doubled-brace key is found once the needle is \
         normalized the way the blob was: {:?}",
        missing_fragments(key, &blob)
    );
}

#[test]
fn every_diagnostic_the_compiler_builds_is_indexed() {
    // N41: a key with no literal run of its own is ALL SLOTS, and
    // `key_describes` lets a leading slot skip — so row 291's
    // `{headline}{subject}: {}` describes any message containing `": "`, and
    // silently held five enumerated sites it has nothing to do with. Check (2)
    // already refuses to search for such a key; check (3) now refuses to let one
    // describe a message. What a fragment-less row may still hold is its OWN
    // site: the envelope literal it is keyed on, verbatim. Anything looser is
    // the catch-all again.
    let rows: Vec<(Row, bool)> = index()
        .into_iter()
        .map(|row| {
            let composed = KEYS_WITHOUT_A_FRAGMENT
                .iter()
                .any(|(key, _)| *key == row.key);
            (row, composed)
        })
        .collect();
    let mut unrowed = Vec::new();
    for site in enumerated_sites() {
        if site.is_note {
            continue;
        }
        // N96: the host runtime's own sentences, reproduced for equivalence.
        if MESSAGES_THAT_ARE_THE_HOSTS
            .iter()
            .any(|(message, _)| *message == site.message)
        {
            continue;
        }
        if rows.iter().any(|(row, composed)| {
            if *composed {
                row.key == site.message
            } else {
                key_describes(&row.key, &site.message)
            }
        }) {
            continue;
        }
        unrowed.push(format!(
            "  {}:{}\n      {:?}",
            site.file, site.line, site.message
        ));
    }
    assert!(
        unrowed.is_empty(),
        "{} diagnostic message(s) no ledger row records. Add a row to \
         `diagnostics-ledger.md` (site, head, verdict) and its key to \
         `{INDEX}`, classified `flagship` — the errors appendix documents it, \
         by the criteria in the index header — or `-`:\n{}",
        unrowed.len(),
        unrowed.join("\n")
    );
}

/// N94: no rowed diagnostic literal carries a SWALLOWED line continuation.
///
/// Rust's `\` + newline + indentation is stripped before the program ever
/// sees it, which is how every long message in this tree is written: four
/// lines in the file, one run in the binary. A literal that LOSES its
/// backslashes — joined by a tool, reflowed by hand — keeps the indentation
/// instead, and ships with a run of eighteen spaces in the middle of a
/// sentence. Two did: `const_eval.rs`'s `asset::staged` refusal, which a user
/// reads, and an `analyzer.rs` test assertion, which a maintainer reads. Both
/// predate Order 36 and neither was noticed by anything, because a run of
/// spaces breaks no test and renders as a gap only when the message is shown.
///
/// THREE is the threshold and it is not arbitrary: no message in this tree
/// separates words by more than one space, the tightest swallow (a
/// four-space indent minus nothing) is four, and two would fire on the
/// double space after a full stop that a message could legitimately carry.
///
/// The reach is the ROWED surface — every message the enumeration in check
/// (3) reads at its anchor, plus every key in the index, which is the half
/// that would otherwise carry the run forward into the ledger's prose. It is
/// deliberately not "every string literal in the compiler": the aligned
/// tables in `formatter.rs` and `bindgen.rs`'s report columns are runs of
/// spaces on purpose, and a gate that has to except them stops being a gate.
/// What that leaves uncovered is named in the header.
#[test]
fn no_rowed_diagnostic_literal_swallows_a_line_continuation() {
    /// A run this long is indentation. Nothing writes it on purpose.
    const RUN: &str = "   ";

    let mut swallowed = Vec::new();
    for site in enumerated_sites() {
        if site.message.contains(RUN) {
            swallowed.push(format!(
                "  {}:{}\n      {:?}",
                site.file, site.line, site.message
            ));
        }
    }
    for row in index() {
        if row.key.contains(RUN) {
            swallowed.push(format!("  {INDEX} row {}\n      {:?}", row.number, row.key));
        }
    }
    assert!(
        swallowed.is_empty(),
        "{} diagnostic literal(s) carry a run of three or more spaces, which is \
         a line continuation whose `\\` was lost — the indentation is now part \
         of the message. Restore the backslashes, or write the literal as \
         concatenated lines:\n{}",
        swallowed.len(),
        swallowed.join("\n")
    );
}

#[test]
fn every_errors_appendix_entry_still_names_a_live_message() {
    let blob = source_blob();
    let mut stale = Vec::new();
    for (line, head) in appendix_heads() {
        if appendix_head_is_held(&head, &blob) {
            continue;
        }
        let missing: Vec<String> = appendix_fragments(&head, true)
            .into_iter()
            .filter(|fragment| !blob.contains(fragment))
            .collect();
        if APPENDIX_HEADS_NOT_HELD
            .iter()
            .any(|(needle, _)| head.contains(needle))
        {
            continue;
        }
        stale.push(format!(
            "  errors.md:{line}\n      {head:?}\n      not in the tree: {missing:?}"
        ));
    }
    assert!(
        stale.is_empty(),
        "{} errors-appendix entry(ies) quote text the compiler no longer \
         prints. Reword the entry to the message the compiler prints, or — if \
         the entry quotes a sentence the compiler COMPOSES from several \
         literals — record it in APPENDIX_HEADS_NOT_HELD with the reason:\n{}",
        stale.len(),
        stale.join("\n")
    );
}

#[test]
fn every_flagship_row_is_quoted_by_the_errors_appendix() {
    let heads: Vec<String> = appendix_heads().into_iter().map(|(_, head)| head).collect();
    let mut undocumented = Vec::new();
    for row in index() {
        if !row.flagship {
            continue;
        }
        // The two texts truncate differently — the ledger cuts its keys, the
        // appendix elides with `…` — so neither contains the other. What is
        // checkable is that they SHARE their longest literal run, in whichever
        // direction the two cuts left it.
        let row_run = longest_fragment(&row.key);
        let quoted = heads.iter().any(|head| {
            [false, true].iter().any(|&relaxed| {
                let head_fragments = appendix_fragments(head, relaxed);
                let head_run = head_fragments.iter().max_by_key(|f| f.chars().count());
                head_run.is_some_and(|fragment| row.key.contains(fragment))
                    || row_run
                        .as_ref()
                        .is_some_and(|fragment| head_fragments.iter().any(|h| h.contains(fragment)))
            })
        });
        if !quoted {
            undocumented.push(format!("  row {}: {:?}", row.number, row.key));
        }
    }
    assert!(
        undocumented.is_empty(),
        "{} row(s) the index marks `flagship` are documented by no errors-appendix \
         entry. Write the entry, or drop the mark — the appendix is a curated \
         subset and the criteria are in the index header:\n{}",
        undocumented.len(),
        undocumented.join("\n")
    );
}

#[test]
fn every_appendix_entry_carries_a_quoted_head() {
    let text = read(APPENDIX);
    let mut headless = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if !is_entry_head(line) || line.starts_with("**\"") {
            continue;
        }
        if HEADS_THAT_ARE_NOT_MESSAGES
            .iter()
            .any(|(head, _)| line.contains(head))
        {
            continue;
        }
        headless.push(format!("  errors.md:{}: {line}", number + 1));
    }
    assert!(
        headless.is_empty(),
        "every errors-appendix entry opens with the message it documents, \
         quoted: `**\"...\"**` — that is what makes the entry checkable. An \
         entry that names a CONDITION instead belongs in \
         HEADS_THAT_ARE_NOT_MESSAGES, with the reason:\n{}",
        headless.join("\n")
    );
}

/// The row an exemption's key prefix names, or why it names none (N76). Exactly
/// one match is the contract: zero means the row is gone (or the key moved),
/// several mean the prefix stopped separating the rows it has to separate, and
/// both are a reason to red rather than to guess.
fn hand_rowed_row<'rows>(rows: &'rows [Row], prefix: &str) -> Result<&'rows Row, String> {
    let matched: Vec<&Row> = rows
        .iter()
        .filter(|row| row.key.starts_with(prefix))
        .collect();
    match matched.as_slice() {
        [row] => Ok(row),
        [] => Err(format!("  no row's key starts with {prefix:?}")),
        several => Err(format!(
            "  {prefix:?} names {} rows ({}) — lengthen it",
            several.len(),
            several
                .iter()
                .map(|row| row.number.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[test]
fn every_hand_rowed_row_is_in_the_index() {
    // The exemption above is only worth its ink while the rows it names are
    // real: a hand-rowed message that loses its row loses ALL coverage, since
    // the enumeration never reached it to begin with. N76: the entry names its
    // row by KEY, so this check is also what holds the prefixes to their one
    // job — naming exactly one row, whatever the ledger has numbered it.
    let rows = index();
    let unresolved: Vec<String> = ROWS_THE_ENUMERATION_CANNOT_REACH
        .iter()
        .filter_map(|(prefix, _)| hand_rowed_row(&rows, prefix).err())
        .collect();
    assert!(
        unresolved.is_empty(),
        "key(s) recorded in ROWS_THE_ENUMERATION_CANNOT_REACH do not name exactly \
         one row of `{INDEX}`. A message the enumeration cannot see is held by \
         its row and nothing else, so dropping the row drops the message — and an \
         AMBIGUOUS key exempts whichever row the list happens to find \
         first:\n{}",
        unresolved.join("\n")
    );

    // The same question of the other key-addressed list (N96). A composed row
    // buys its exemption from check (2) with an exact match in check (3), and
    // an entry naming no row buys nothing while looking like it does.
    let missing: Vec<String> = KEYS_WITHOUT_A_FRAGMENT
        .iter()
        .filter_map(
            |(key, reason)| match rows.iter().filter(|row| row.key == *key).count() {
                1 => None,
                0 => Some(format!("  no row is keyed {key:?} ({reason})")),
                several => Some(format!("  {several} rows are keyed {key:?}")),
            },
        )
        .collect();
    assert!(
        missing.is_empty(),
        "key(s) recorded in KEYS_WITHOUT_A_FRAGMENT do not name exactly one row \
         of `{INDEX}`:\n{}",
        missing.join("\n")
    );
}

#[test]
fn n76_a_hand_rowed_exemption_resolves_by_key_not_by_number() {
    // N76's whole point, asked of the resolver rather than of the live index:
    // a lane that ships a message the walk cannot read writes its row as `NEW`
    // (the ledger numbers it at integration), so an exemption that named rows
    // by NUMBER had nothing to write — and two lanes each writing `"NEW"` in
    // one order would have exempted each other's row. A key is written in the
    // same commit as the message and does not move at the renumbering.
    let rows = vec![
        Row {
            number: NEW_ROW.to_string(),
            ordinal: None,
            flagship: false,
            key: "`~` inverts a number, and `{label}` is a type parameter: nothing".to_string(),
        },
        Row {
            number: "1".to_string(),
            ordinal: Some(1),
            flagship: false,
            key: "`~` inverts a number, and `{label}` is a trait: nothing".to_string(),
        },
    ];
    let Ok(resolved) = hand_rowed_row(&rows, "`~` inverts a number, and `{label}` is a type")
    else {
        panic!("the prefix names the unnumbered row");
    };
    assert_eq!(
        resolved.number, NEW_ROW,
        "a row with no number yet is still nameable — that is what keying on the \
         key buys"
    );

    // And the two ways a key stops naming exactly one row both red, which is
    // what keeps the list from exempting a row nobody meant.
    let Err(absent) = hand_rowed_row(&rows, "`^` xors") else {
        panic!("no row starts with `^`");
    };
    assert!(absent.contains("no row's key starts with"), "{absent}");
    let Err(ambiguous) = hand_rowed_row(&rows, "`~` inverts a number,") else {
        panic!("both rows start with that prefix");
    };
    assert!(
        ambiguous.contains("names 2 rows") && ambiguous.contains("lengthen it"),
        "{ambiguous}"
    );
}

#[test]
fn every_hand_rowed_row_is_still_out_of_the_enumerations_reach() {
    // The inverse (tracker N42), and the half the check above cannot make. An
    // exemption is a claim about the WALK — "check (3) never sees this row's
    // message" — and the walk is a thing that changes: widen an anchor, move a
    // ladder's `format!` to the `msg:` site, and the row becomes reachable. It
    // stays exempt forever regardless, because being listed here only ever
    // subtracts work, so the rot is silent in exactly the direction nobody
    // looks.
    //
    // Reached is asked the way check (3) asks it: some enumerated ERROR site
    // this row's key describes. That is the same predicate, so a row this test
    // calls reachable is a row check (3) would hold on its own.
    let rows = index();
    let sites = enumerated_sites();
    let reached: Vec<String> = ROWS_THE_ENUMERATION_CANNOT_REACH
        .iter()
        .filter_map(|(prefix, _)| {
            // An entry whose key names no row, or several, is the OTHER check's
            // to report; this one asks its question of the rows it can resolve.
            let row = hand_rowed_row(&rows, prefix).ok()?;
            sites
                .iter()
                .filter(|site| !site.is_note)
                .find(|site| key_describes(&row.key, &site.message))
                .map(|site| {
                    format!(
                        "  row {}: now enumerated at {}:{}",
                        row.number, site.file, site.line
                    )
                })
        })
        .collect();
    assert!(
        reached.is_empty(),
        "row(s) recorded in ROWS_THE_ENUMERATION_CANNOT_REACH are reachable now — \
         the walk found their message at its own site, so check (3) holds them and \
         the hand-rowing is dead weight claiming otherwise. Delete the entry (and, \
         if the list empties, the argument on it):\n{}",
        reached.join("\n")
    );
}

#[test]
fn every_hosts_message_is_still_thrown() {
    // N42's shape for the third list (N96). `MESSAGES_THAT_ARE_THE_HOSTS`
    // subtracts a row's worth of work on a claim about the TREE — "this text
    // is V8's and the interpreter copies it" — and a claim about the tree is
    // a thing that goes stale. If the site is gone, or its text moved a
    // character, the fence is excusing a message nobody throws while the one
    // that IS thrown goes unrowed.
    let thrown: BTreeSet<String> = enumerated_sites()
        .into_iter()
        .map(|site| site.message)
        .collect();
    let stale: Vec<String> = MESSAGES_THAT_ARE_THE_HOSTS
        .iter()
        .filter(|(message, _)| !thrown.contains(*message))
        .map(|(message, reason)| format!("  {message:?}\n      fenced as {reason}"))
        .collect();
    assert!(
        stale.is_empty(),
        "{} fenced message(s) are no longer thrown at any enumerated site. The \
         fence excuses text nothing prints. Delete the entry — and if the \
         message merely moved, row the one that replaced it:\n{}",
        stale.len(),
        stale.join("\n")
    );
}

#[test]
fn every_fragmentless_key_still_has_no_fragment() {
    // The same inverse for the other list (tracker N42). `KEYS_WITHOUT_A_FRAGMENT`
    // exempts a row from check (2) — from being searched for in the tree at all —
    // and buys that with a much weaker check (3): a composed row may hold only
    // its own site, matched verbatim. A key that has since been RE-KEYED onto a
    // real literal run should get check (2) back, and nothing was asking.
    let stale: Vec<String> = index()
        .into_iter()
        .filter(|row| {
            KEYS_WITHOUT_A_FRAGMENT
                .iter()
                .any(|(key, _)| *key == row.key)
        })
        .filter_map(|row| {
            longest_fragment(&row.key)
                .map(|fragment| format!("  row {}: searchable as {fragment:?}", row.number))
        })
        .collect();
    assert!(
        stale.is_empty(),
        "row(s) recorded in KEYS_WITHOUT_A_FRAGMENT now carry a literal run of \
         {MIN_FRAGMENT} characters, so they are searchable and the exemption costs \
         them check (2) for nothing. Delete the entry:\n{}",
        stale.join("\n")
    );
}

#[test]
fn the_enumeration_reaches_the_message_surface_it_claims() {
    // A guard on the gate itself: if the anchors ever stop matching — a
    // refactor renames the `msg` field, `manifest.rs` moves — every check
    // above would go quietly green over nothing.
    let sites = enumerated_sites();
    let errors = sites.iter().filter(|site| !site.is_note).count();
    let notes = sites.len() - errors;
    assert!(
        errors > 200,
        "the enumeration found only {errors} diagnostic message sites; the \
         anchors have stopped matching"
    );
    assert!(notes > 10, "the enumeration found only {notes} note sites");
    assert!(
        sites.iter().any(|site| site.file == "manifest.rs"),
        "the manifest refusal family is no longer enumerated"
    );
    // N96's half of the same guard: the `Failure` anchors have their own
    // cursor, and a rename of either constructor would take 50-odd const-channel
    // and macro-engine refusals out of check (3) without a single row reding.
    let failures = sites
        .iter()
        .filter(|site| site.file == "interpreter.rs")
        .count();
    assert!(
        failures > 40,
        "the enumeration found only {failures} `Failure` sites; the \
         `Failure::new`/`Failure::unsupported` anchors have stopped matching"
    );
    let files: BTreeSet<&str> = sites.iter().map(|site| site.file.as_str()).collect();
    assert!(files.len() > 5, "the enumeration reaches only {files:?}");
}

#[test]
fn every_unheld_appendix_head_still_cannot_be_held() {
    // The inverse for `APPENDIX_HEADS_NOT_HELD` (tracker N50, N42's shape).
    // The entry's reason is a claim about the TREE — "the sentence this entry
    // quotes is composed from literals that live apart, so it appears nowhere
    // as one run" — and the tree is a thing that changes: join the two literals
    // into one `format!` and the head becomes holdable, while the exemption
    // goes on excusing it from the check forever.
    //
    // Held is asked the way `every_errors_appendix_entry_still_names_a_live_message`
    // asks it, through the same predicate, so a head this test calls holdable is
    // a head that check would hold on its own.
    let blob = source_blob();
    let heads = appendix_heads();
    let stale: Vec<String> = APPENDIX_HEADS_NOT_HELD
        .iter()
        .filter_map(|(needle, _)| {
            let covered: Vec<&(usize, String)> = heads
                .iter()
                .filter(|(_, head)| head.contains(needle))
                .collect();
            if covered.is_empty() {
                return Some(format!(
                    "  {needle:?}: no errors-appendix entry quotes it any more, so \
                     the entry exempts nothing"
                ));
            }
            covered
                .iter()
                .all(|(_, head)| appendix_head_is_held(head, &blob))
                .then(|| {
                    format!(
                        "  {needle:?}: every entry it covers is held literally by the \
                         tree now (errors.md:{})",
                        covered
                            .iter()
                            .map(|(line, _)| line.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
        })
        .collect();
    assert!(
        stale.is_empty(),
        "entry(ies) in APPENDIX_HEADS_NOT_HELD excuse a head from the literal \
         check that the literal check would pass. Delete the entry — an entry the \
         tree holds should be held, so the next rewording of its message is \
         red:\n{}",
        stale.join("\n")
    );
}

#[test]
fn every_non_message_head_still_names_a_headless_entry() {
    // The inverse for `HEADS_THAT_ARE_NOT_MESSAGES` (tracker N50). The entry
    // exempts one appendix line from the rule that an entry opens with the
    // message it documents, quoted. Reword that line into a quoted head — which
    // is what happens when the condition it names finally gets a message of its
    // own — and the exemption covers a line the gate no longer looks at; delete
    // the entry from the appendix and it covers nothing at all.
    //
    // Asked with `is_entry_head`, the gate's own predicate, so a line this test
    // calls covered is a line the gate would reach.
    let text = read(APPENDIX);
    let stale: Vec<String> = HEADS_THAT_ARE_NOT_MESSAGES
        .iter()
        .filter_map(|(head, _)| {
            let named: Vec<usize> = text
                .lines()
                .enumerate()
                .filter(|(_, line)| line.contains(head))
                .map(|(number, _)| number + 1)
                .collect();
            if named.is_empty() {
                return Some(format!(
                    "  {head:?}: no line of `{APPENDIX}` carries it any more"
                ));
            }
            let still_flagged = text.lines().any(|line| {
                line.contains(head) && is_entry_head(line) && !line.starts_with("**\"")
            });
            (!still_flagged).then(|| {
                format!(
                    "  {head:?}: errors.md:{named:?} opens with a quoted message now, \
                     so the gate skips it on its own"
                )
            })
        })
        .collect();
    assert!(
        stale.is_empty(),
        "entry(ies) in HEADS_THAT_ARE_NOT_MESSAGES record an appendix entry as \
         naming a CONDITION that does not. Delete the entry — an entry that quotes \
         its message is checkable, and this list is what says it is not:\n{}",
        stale.join("\n")
    );
}

#[test]
fn the_keyless_row_still_counts_the_rule_sites() {
    // Tracker N60. Row 229 is keyless because its messages are `&'static str`
    // constants the enumeration cannot reach, so the only thing standing for
    // them is a COUNT — and a count written in prose went eleven sites stale
    // across seven orders without a single gate noticing. This is that count,
    // asked of the tree.
    let parsing = repository_root().join("crates/vilan-core/src/parsing.rs");
    let source = std::fs::read_to_string(&parsing)
        .unwrap_or_else(|error| panic!("{}: {error}", parsing.display()));
    let mentions = source.matches("ParseErrorReason::Rule(").count();
    // The one mention that is not a statement: the arm that renders a `Rule`
    // back into its message. The two doc comments spell the variant without the
    // paren and never reach this count.
    let rendering_arm = source
        .matches("ParseErrorReason::Rule(rule) => rule.to_string()")
        .count();
    assert_eq!(
        rendering_arm, 1,
        "the arm that renders a `Rule` moved or was reworded — the subtraction \
         below no longer subtracts it"
    );
    assert_eq!(
        mentions - rendering_arm,
        RULE_STATEMENT_SITES,
        "`parsing.rs` builds {} `ParseErrorReason::Rule` statement(s), and \
         `RULE_STATEMENT_SITES` says {RULE_STATEMENT_SITES}. A rule statement \
         added or removed moves ledger row 229's population: update the constant \
         here AND the row in `proposal/diagnostics-ledger.md`, which carries the \
         same number in prose.",
        mentions - rendering_arm
    );
}

/// Every `&'static str` constant in `path` that a rule statement states, as
/// `(name, text)` — the declarations, filtered to the ones whose name reaches
/// one of [`RULE_STATEMENT_CONTEXTS`] (N65).
fn rule_statement_constants(path: &Path) -> Vec<(String, String)> {
    let source = std::fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("{}: {error}", path.display());
    });
    let characters: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    for (offset, _) in source.match_indices("const ") {
        let after = &source[offset + "const ".len()..];
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let declaration = &after[name.len()..];
        let Some(rest) = declaration
            .strip_prefix(": &'static str = ")
            .or_else(|| declaration.strip_prefix(": &str = "))
        else {
            continue;
        };
        if !rest.starts_with('"') {
            continue;
        }
        let quote = source.len() - rest.len();
        let quote_index = source[..quote].chars().count();
        let Some((text, _)) = string_literal(&characters, quote_index) else {
            continue;
        };
        if states_a_rule(&source, &name) {
            found.push((name, text));
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Whether `name` is MENTIONED where a rule statement is made — the test the
/// list stands on, asked of the tree rather than of a memory of it.
fn states_a_rule(source: &str, name: &str) -> bool {
    source.match_indices(name).any(|(at, _)| {
        // Back up to a character boundary: the window is a byte count, and
        // these files are full of `—` and `…`.
        let mut from = at.saturating_sub(RULE_CONTEXT_WINDOW);
        while from < at && !source.is_char_boundary(from) {
            from += 1;
        }
        let window = &source[from..at];
        RULE_STATEMENT_CONTEXTS
            .iter()
            .any(|context| window.contains(context))
    })
}

#[test]
fn every_rule_statement_constant_is_curated() {
    // N65, and N42's rule for a list: the enumeration is only worth its ink
    // while it is the WHOLE population. A rule statement stated through a new
    // constant is a message with no row anywhere — not in the index, which
    // cannot key on it, and not here — and nothing would have said so.
    let mut in_tree: Vec<String> = Vec::new();
    for file in [
        "crates/vilan-core/src/parsing.rs",
        "crates/vilan-core/src/lexing.rs",
    ] {
        for (name, _) in rule_statement_constants(&repository_root().join(file)) {
            in_tree.push(format!("{file}\t{name}"));
        }
    }
    let listed: BTreeSet<String> = CURATED_RULE_STATEMENTS
        .iter()
        .map(|(file, name, _)| format!("{file}\t{name}"))
        .collect();
    let in_tree: BTreeSet<String> = in_tree.into_iter().collect();
    let unlisted: Vec<&String> = in_tree.difference(&listed).collect();
    assert!(
        unlisted.is_empty(),
        "rule statement constant(s) no ledger row records: {unlisted:?}. A curated \
         rule statement is a message, and row 229 can only count it — add it to \
         CURATED_RULE_STATEMENTS with the head it opens with, and to \
         `proposal/diagnostics-ledger.md`'s row 229 population."
    );
    let gone: Vec<&String> = listed.difference(&in_tree).collect();
    assert!(
        gone.is_empty(),
        "CURATED_RULE_STATEMENTS names constant(s) the tree no longer states as a \
         rule: {gone:?}. A row for a message nobody prints is the rot the ledger \
         exists to catch — delete the entry, or restore the statement."
    );
}

#[test]
fn every_curated_rule_statement_still_opens_as_recorded() {
    // The half a count cannot make (N65). A reworded rule statement now reds
    // the row that records it, which is what "a row EDIT" needs: something to
    // edit.
    let mut reworded = Vec::new();
    for (file, name, head) in CURATED_RULE_STATEMENTS {
        let constants = rule_statement_constants(&repository_root().join(file));
        let Some((_, text)) = constants.iter().find(|(found, _)| found == name) else {
            continue; // the check above names it; this one says nothing twice.
        };
        if !text.starts_with(head) {
            let shown: String = text.chars().take(head.chars().count() + 20).collect();
            reworded.push(format!("  {file}: {name}\n      now opens {shown:?}"));
        }
    }
    assert!(
        reworded.is_empty(),
        "{} curated rule statement(s) no longer open with the head recorded in \
         CURATED_RULE_STATEMENTS. A reworded message is a row EDIT: update the \
         head here, and the row in `proposal/diagnostics-ledger.md`:\n{}",
        reworded.len(),
        reworded.join("\n")
    );
}

#[test]
fn the_helper_built_resource_derive_refusal_is_rowed() {
    // N65's other half. `analyzer::resource_derive_refusal` builds its sentence
    // in a helper and hands back a `String`, so the enumeration in check (3) —
    // which reads the literal written AT a `msg:` anchor — never saw it, and it
    // carried no row at all: an unrowed message, not a red one. It has one now,
    // and this is the pin in both directions.
    //
    // Forward: the tree still prints it, at the helper the row stands for.
    let analyzer = normalized(&read("crates/vilan-core/src/analyzer.rs"));
    assert!(
        analyzer.contains(RESOURCE_DERIVE_REFUSAL_FRAGMENT),
        "`resource_derive_refusal` no longer builds {RESOURCE_DERIVE_REFUSAL_FRAGMENT:?} — \
         reword its row in `{INDEX}` with it"
    );
    // Back: a row in the index is keyed on it. Check (2) then holds the key
    // against the tree on every run, which is the coverage the message never
    // had.
    let rowed = index()
        .into_iter()
        .any(|row| row.key.contains(RESOURCE_DERIVE_REFUSAL_FRAGMENT));
    assert!(
        rowed,
        "no row in `{INDEX}` is keyed on the resource-derive refusal. It is built \
         in a helper, so nothing else in this file can reach it: without its row it \
         has no coverage at all."
    );
}
