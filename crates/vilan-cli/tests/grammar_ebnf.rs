//! The normative grammar (`vilan/docs/spec/grammar.md`), gated (N34).
//!
//! `grammar_sync.rs` next door gates the two syntax-HIGHLIGHTING grammars
//! against the compiler's word tables, and the book's anchors golden pins
//! headings. Nothing read spec §3's EBNF: the normative grammar was kept true
//! by review alone, which is how `extern-args` went three forms stale — and one
//! form WRONG — across two releases with nothing red (the smalls lane's N32
//! finding, Order 22).
//!
//! # What this file verifies
//!
//! 1. **Keyword coverage, both directions.** Every word in `lexing::KEYWORDS`
//!    is spelled as a quoted terminal somewhere in the EBNF, and every
//!    identifier-shaped quoted terminal in the EBNF is a keyword, a known
//!    attribute marker, an extern form word, a contextual word, or on the
//!    recorded allow-list below. A keyword the grammar never mentions, and a
//!    grammar word the lexer has never heard of, are both red.
//! 2. **Attribute markers.** Every `parsing::KNOWN_ATTRIBUTE_MARKERS` name
//!    appears as a quoted terminal — the D15 rot site (`platform` was missing
//!    from both highlighting grammars for a release), asserted here on its own
//!    so the red names the marker.
//! 3. **`extern-args`, the proven rot site.** The bare form words the
//!    `extern-args` production offers are exactly the ones
//!    `extern_binding_from_args` matches on, read from the parser's own source
//!    rather than from a second table, and `retains` is present as the trailing
//!    flag.
//! 4. **Internal closure.** Every nonterminal a production references is
//!    defined by some production in the document, or is a declared token class,
//!    or is one of the recorded prose-defined names; and every production is
//!    referenced from some other production, or is the start symbol. A
//!    production that names a rule nobody wrote, and a rule nothing reaches,
//!    are both drift.
//! 5. **The exemptions expire** (N42, N50). Every table at the top of this file
//!    widens one of the checks above, and a widening is only worth its ink
//!    while what it covers is still uncovered: a `TOKEN_CLASSES_NOT_IN_LEXICAL`
//!    class §2 has since declared, a `NON_KEYWORD_TERMINALS` word the compiler
//!    now has a table for, a `PROSE_DEFINED` name §3 now writes a production
//!    for, a `PROSE_RIGHT_HAND_SIDES` side that reads as grammar after all, a
//!    `UNREACHED_PRODUCTIONS` root something reaches — each reds, by name.
//!
//! # What this file does NOT verify
//!
//! Emphatically: **this is not a parser-equivalence check.** It says nothing
//! about whether the parser accepts the language these productions describe.
//! In particular it does not check
//!
//! - that a production's SHAPE matches the parser's — the operand order, the
//!   optionality, the repetition, the precedence. §3.7's table is prose and
//!   stays prose; `operator-expr` is written as shape only and says so.
//! - that the grammar is unambiguous, or that its recovery behaviour is real;
//! - that a quoted terminal appears in the RIGHT production — `mut` being a
//!   keyword the grammar mentions somewhere is all check 1 knows;
//! - anything about `spec/lexical.md`'s token classes beyond the names this
//!   document declares it uses.
//!
//! A green here means the grammar's WORD SURFACE and its own internal wiring
//! agree with the compiler and with themselves. Everything about its structure
//! is still kept true by review.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use vilan_core::lexing::KEYWORDS;
use vilan_core::parsing::KNOWN_ATTRIBUTE_MARKERS;

const GRAMMAR: &str = "vilan/docs/spec/grammar.md";
const PARSER: &str = "crates/vilan-core/src/parsing.rs";
const LEXICAL: &str = "vilan/docs/spec/lexical.md";

/// Identifier-shaped terminals the EBNF quotes that are not keywords, not
/// attribute markers and not extern form words, with what each is. Every one
/// is a CONTEXTUAL word or a fixed argument the parser matches by text.
const NON_KEYWORD_TERMINALS: &[(&str, &str)] = &[
    ("context", "the contextual clause on a closure type (§2.2)"),
    (
        "sync",
        "the contextual marker opening a closure type (§7.4)",
    ),
    ("on", "the element head's event form: `on:click(..)`"),
    ("hidden", "the sole argument of `[doc(hidden)]`"),
    (
        "as",
        "the contextual alias on an import/use path leaf (§3.2, E142)",
    ),
    ("_", "the wildcard pattern"),
];

/// Nonterminals the document defines in PROSE rather than with a production,
/// with where. Each is a token run the grammar deliberately does not shape.
const PROSE_DEFINED: &[(&str, &str)] = &[(
    "expr-span",
    "a raw balanced token run handed to a macro as source text — §3.3's \
     macro-item fence defines it in the paragraph beneath",
)];

/// Words that appear in a production's right-hand side as ENGLISH, because the
/// production itself is prose. Listed so check 4 does not read them as
/// nonterminals.
const PROSE_RIGHT_HAND_SIDES: &[(&str, &str)] = &[(
    "INTEGER",
    "`NUMBER without a fractional part and without a SUFFIX` — a restriction \
     on a token class, not a shape",
)];

/// Productions no other production reaches, with why. The start symbol is the
/// only one there should ever be.
const UNREACHED_PRODUCTIONS: &[(&str, &str)] = &[("module", "the start symbol (§3)")];

/// Token classes §3 uses that `spec/lexical.md` does NOT declare with a
/// production of its own, with why each is here rather than there. Everything
/// else §3 names in capitals must be a production in §2 — that cross-document
/// check is what [`token_classes`] does.
const TOKEN_CLASSES_NOT_IN_LEXICAL: &[(&str, &str)] = &[
    (
        "NAME",
        "`an identifier or any keyword` — §3.6's element-name rule defines it          in its own fence comment, because it is a §3 concept",
    ),
    (
        "TOKEN",
        "`any token but \";\", \"{\", \"}\"` — the css-block value scanner's          meta-class, defined in §3.6's own fence comment",
    ),
    (
        "INTEGER",
        "`NUMBER without a fractional part and without a SUFFIX` — declared in          §3.3's fixed-array fence, over §2's NUMBER",
    ),
];

/// The token classes `spec/lexical.md` DECLARES — §2's own productions, and
/// nothing else. Read from the document rather than copied, so a class that
/// leaves §2 stops being available to §3.
fn lexical_token_classes() -> BTreeSet<String> {
    let lexical = read(LEXICAL);
    let mut classes: BTreeSet<String> = BTreeSet::new();
    for line in lexical.lines() {
        if let Some((left, _)) = line.split_once('=') {
            let name = left.trim();
            if !name.is_empty()
                && name.chars().all(|c| c.is_ascii_uppercase() || c == '_')
                && name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            {
                classes.insert(name.to_string());
            }
        }
    }
    assert!(
        classes.len() >= 4,
        "spec/lexical.md reads as {classes:?} token classes — the §2 scan has          stopped matching"
    );
    classes
}

/// Every capitalized token class §3 may use: the productions `spec/lexical.md`
/// writes, plus the recorded few above.
fn token_classes() -> BTreeSet<String> {
    let mut classes = lexical_token_classes();
    classes.extend(
        TOKEN_CLASSES_NOT_IN_LEXICAL
            .iter()
            .map(|(name, _)| name.to_string()),
    );
    classes
}

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

/// The EBNF: every ```` ```text ```` fence in the grammar document, joined.
/// The fences are the normative grammar; everything around them is commentary.
fn ebnf() -> String {
    let text = read(GRAMMAR);
    let mut fences = String::new();
    let mut inside = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            inside = line.trim() == "```text";
            continue;
        }
        if inside {
            fences.push_str(line);
            fences.push('\n');
        }
    }
    assert!(
        fences.lines().count() > 100,
        "the EBNF fences read as {} lines — the fence scan has stopped matching",
        fences.lines().count()
    );
    fences
}

/// `(* ... *)` comments removed. They carry prose, and prose is not grammar.
fn without_comments(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("(*") {
        out.push_str(&rest[..open]);
        match rest[open..].find("*)") {
            Some(close) => rest = &rest[open + close + 2..],
            None => {
                rest = "";
                break;
            }
        }
        out.push(' ');
    }
    out.push_str(rest);
    out
}

/// Every `"..."` terminal in the EBNF.
fn terminals(text: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut chars = text.chars().peekable();
    let mut current: Option<String> = None;
    while let Some(character) = chars.next() {
        match (&mut current, character) {
            (None, '"') => current = Some(String::new()),
            (Some(_), '"') => {
                found.insert(current.take().expect("an open terminal"));
            }
            (Some(open), other) => open.push(other),
            (None, _) => {}
        }
        let _ = chars.peek();
    }
    found
}

fn is_identifier_shaped(word: &str) -> bool {
    !word.is_empty()
        && word.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !word.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Each production, by name, with its right-hand side. A production opens on a
/// line whose first token is a name followed by `=`, and runs to the `;` that
/// closes it.
fn productions(ebnf: &str) -> BTreeMap<String, String> {
    let mut found = BTreeMap::new();
    let mut name: Option<String> = None;
    let mut body = String::new();
    for line in ebnf.lines() {
        let head = line
            .split_once('=')
            .map(|(left, _)| left.trim())
            .filter(|left| {
                !left.is_empty()
                    && left.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
                    && left.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
            });
        if let Some(head) = head
            && !line.starts_with(' ')
        {
            if let Some(previous) = name.take() {
                found.insert(previous, std::mem::take(&mut body));
            }
            name = Some(head.to_string());
            body.push_str(line.split_once('=').expect("an `=`").1);
        } else if name.is_some() {
            body.push('\n');
            body.push_str(line);
        }
        if line.trim_end().ends_with(';')
            && let Some(previous) = name.take()
        {
            found.insert(previous, std::mem::take(&mut body));
        }
    }
    if let Some(previous) = name {
        found.insert(previous, body);
    }
    found
}

/// The nonterminal names a right-hand side mentions: every bare word left once
/// the comments and the quoted terminals are gone.
fn references(right_hand_side: &str) -> BTreeSet<String> {
    let stripped = without_comments(right_hand_side);
    let mut without_terminals = String::new();
    let mut inside = false;
    for character in stripped.chars() {
        if character == '"' {
            inside = !inside;
            without_terminals.push(' ');
        } else if !inside {
            without_terminals.push(character);
        }
    }
    without_terminals
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .filter(|word| {
            !word.is_empty() && word.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        })
        .map(str::to_string)
        .collect()
}

// --- The gates -------------------------------------------------------------

#[test]
fn every_keyword_is_spelled_in_the_normative_grammar() {
    let ebnf = ebnf();
    let terminals = terminals(&ebnf);
    let missing: Vec<&str> = KEYWORDS
        .iter()
        .map(|(word, _)| *word)
        .filter(|word| !terminals.contains(*word))
        .collect();
    assert!(
        missing.is_empty(),
        "the lexer knows {:?}, and spec §3's EBNF never spells {}. A keyword \
         lands in the grammar as well as in the lexer — that is the whole \
         reason `extern-args` could go three forms stale.",
        missing,
        if missing.len() == 1 { "it" } else { "them" }
    );
}

#[test]
fn every_word_the_grammar_quotes_is_one_the_compiler_knows() {
    let ebnf = ebnf();
    let form_words = extern_form_words();
    let known: BTreeSet<&str> = KEYWORDS
        .iter()
        .map(|(word, _)| *word)
        .chain(KNOWN_ATTRIBUTE_MARKERS.iter().copied())
        .chain(form_words.iter().map(String::as_str))
        .chain(NON_KEYWORD_TERMINALS.iter().map(|(word, _)| *word))
        .collect();
    let unknown: Vec<String> = terminals(&ebnf)
        .into_iter()
        .filter(|word| is_identifier_shaped(word) && !known.contains(word.as_str()))
        .collect();
    assert!(
        unknown.is_empty(),
        "spec §3's EBNF quotes {unknown:?} as terminal(s) the compiler has no \
         table for. Either the word is gone from the language (delete it — \
         `i64` coloured as a type for a release after it became a hard error), \
         or it is a contextual word and belongs in NON_KEYWORD_TERMINALS with \
         what it is."
    );
}

#[test]
fn every_attribute_marker_is_spelled_in_the_normative_grammar() {
    let terminals = terminals(&ebnf());
    let missing: Vec<&str> = KNOWN_ATTRIBUTE_MARKERS
        .iter()
        .copied()
        .filter(|marker| !terminals.contains(*marker))
        .collect();
    assert!(
        missing.is_empty(),
        "the parser accepts the attribute marker(s) {missing:?} and spec §3's \
         EBNF never spells them."
    );
}

/// The bare form words `[extern(..)]` accepts, read from the parser's own
/// source — `extern_binding_from_args`'s `Word("..")` patterns — so this gate
/// holds the grammar to the code rather than to a copy of it.
fn extern_form_words() -> BTreeSet<String> {
    let source = read(PARSER);
    let start = source
        .find("fn extern_binding_from_args")
        .expect("`extern_binding_from_args` is where the extern arms live");
    let end = source[start..]
        .find("\n}\n")
        .map(|offset| start + offset)
        .unwrap_or(source.len());
    let body = &source[start..end];
    let mut words = BTreeSet::new();
    let mut rest = body;
    while let Some(offset) = rest.find("Word(\"") {
        rest = &rest[offset + 6..];
        if let Some(close) = rest.find('"') {
            words.insert(rest[..close].to_string());
        }
    }
    // The retention flag is recognized separately, in trailing position.
    if source.contains("ExternArg::Word(\"retains\")") {
        words.insert("retains".to_string());
    }
    assert!(
        words.len() >= 4,
        "the extern form-word scan found {words:?} — `extern_binding_from_args` \
         has been restructured and this gate is reading nothing"
    );
    words
}

#[test]
fn the_extern_args_production_offers_exactly_the_parsers_form_words() {
    let productions = productions(&ebnf());
    let extern_args = productions
        .get("extern-args")
        .expect("spec §3.3 declares `extern-args`");
    let attribute = productions
        .get("extern-attr")
        .expect("spec §3.3 declares `extern-attr`");
    let mut spelled: BTreeSet<String> = terminals(extern_args)
        .into_iter()
        .filter(|word| is_identifier_shaped(word))
        .collect();
    // `retains` is a flag on the ATTRIBUTE, not an arm of `extern-args`.
    if attribute.contains("\"retains\"") {
        spelled.insert("retains".to_string());
    }
    let parsed = extern_form_words();
    assert_eq!(
        spelled, parsed,
        "the `extern-args` production offers {spelled:?} and \
         `extern_binding_from_args` matches {parsed:?}. This production is the \
         one N34 names: it went three forms stale, and one form wrong, across \
         two releases."
    );
    assert!(
        attribute.contains("\"retains\""),
        "`retains` is a trailing FLAG on `extern-attr`, not one of \
         `extern-args`' arms (§6.8) — the production must show it in trailing \
         position: {attribute}"
    );
}

#[test]
fn every_production_the_grammar_names_is_one_the_grammar_writes() {
    let ebnf = ebnf();
    let productions = productions(&ebnf);
    assert!(
        productions.len() > 60,
        "the grammar reads as {} productions — the production scan has stopped \
         matching",
        productions.len()
    );
    let classes = token_classes();
    let mut undefined: BTreeSet<String> = BTreeSet::new();
    for (name, body) in &productions {
        if PROSE_RIGHT_HAND_SIDES
            .iter()
            .any(|(prose, _)| prose == name)
        {
            continue;
        }
        for reference in references(body) {
            if productions.contains_key(&reference)
                || classes.contains(&reference)
                || PROSE_DEFINED.iter().any(|(word, _)| *word == reference)
            {
                continue;
            }
            undefined.insert(format!("{reference} (referenced by `{name}`)"));
        }
    }
    assert!(
        undefined.is_empty(),
        "spec §3's EBNF names rule(s) it never writes: {undefined:?}. Write the \
         production, or — if the name is defined in the prose beneath its \
         fence — record it in PROSE_DEFINED with where."
    );
}

#[test]
fn every_production_the_grammar_writes_is_one_the_grammar_reaches() {
    let ebnf = ebnf();
    let productions = productions(&ebnf);
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for (name, body) in &productions {
        for reference in references(body) {
            if reference != *name {
                referenced.insert(reference);
            }
        }
    }
    let unreached: Vec<&String> = productions
        .keys()
        .filter(|name| {
            !referenced.contains(*name)
                && !UNREACHED_PRODUCTIONS
                    .iter()
                    .any(|(root, _)| *root == name.as_str())
        })
        .collect();
    assert!(
        unreached.is_empty(),
        "spec §3 writes production(s) nothing reaches: {unreached:?}. Either a \
         rule that references them was dropped (the grammar is behind the \
         parser), or they are dead and should go."
    );
}

#[test]
fn every_recorded_token_class_still_needs_recording() {
    // The inverse of the exemption (tracker N42). `TOKEN_CLASSES_NOT_IN_LEXICAL`
    // widens what §3 may name, and a widening only earns its ink while the thing
    // it covers is still uncovered: the moment `spec/lexical.md` grows a
    // production for one of these, the entry stops exempting anything and
    // becomes a claim about the document that is no longer true.
    //
    // This is not hypothetical. N39 removed `ISTRING` from this list BY HAND,
    // having noticed it in a read-through, and nothing would have noticed it
    // otherwise: the gate stays green either way, because a name that is BOTH
    // recorded here and declared in §2 is simply inserted twice into a set.
    let declared = lexical_token_classes();
    let stale: Vec<&str> = TOKEN_CLASSES_NOT_IN_LEXICAL
        .iter()
        .map(|(name, _)| *name)
        .filter(|name| declared.contains(*name))
        .collect();
    assert!(
        stale.is_empty(),
        "token class(es) {stale:?} are recorded in TOKEN_CLASSES_NOT_IN_LEXICAL as \
         classes `{LEXICAL}` does not declare — and it declares them. The exemption \
         covers nothing now: delete the entry, and §2's production carries the class \
         on its own."
    );
}

// --- The exemptions' inverses (tracker N50) ---------------------------------
//
// Four of the five tables at the top of this file WIDEN a gate, and the fifth
// (`TOKEN_CLASSES_NOT_IN_LEXICAL`) already has the check that keeps a widening
// honest: `every_recorded_token_class_still_needs_recording`, N42's shape. The
// other four rot in the same silent direction and for the same reason — being
// listed only ever SUBTRACTS work, so an entry whose reason has stopped holding
// goes on subtracting it forever, and the gate stays green either way. N39
// removed a rotted entry from the fifth list BY HAND, having noticed it in a
// read-through, which is how long the silence lasts otherwise.
//
// Each check below asks its table's own question — "is this still exempting
// anything?" — with the predicate the gate it widens uses, so an entry these
// tests call dead is an entry the gate would hold on its own.

#[test]
fn every_recorded_non_keyword_terminal_still_needs_recording() {
    // `NON_KEYWORD_TERMINALS` widens check 1: a quoted terminal listed here is
    // allowed to be a word no compiler table knows. Two ways that stops being
    // true — the word becomes a keyword, a marker or an extern form word (the
    // table now covers it, and the entry claims otherwise), or the grammar
    // stops quoting it at all (there is nothing left to exempt).
    let quoted = terminals(&ebnf());
    let known: BTreeSet<String> = KEYWORDS
        .iter()
        .map(|(word, _)| (*word).to_string())
        .chain(
            KNOWN_ATTRIBUTE_MARKERS
                .iter()
                .map(|word| (*word).to_string()),
        )
        .chain(extern_form_words())
        .collect();
    let stale: Vec<String> = NON_KEYWORD_TERMINALS
        .iter()
        .filter_map(|(word, _)| {
            if !quoted.contains(*word) {
                Some(format!(
                    "  {word:?}: spec §3's EBNF no longer quotes it, so the entry \
                     exempts nothing"
                ))
            } else if known.contains(*word) {
                Some(format!(
                    "  {word:?}: the compiler has a table for it now, so check 1 \
                     accepts it on its own"
                ))
            } else {
                None
            }
        })
        .collect();
    assert!(
        stale.is_empty(),
        "entry(ies) in NON_KEYWORD_TERMINALS record a word as contextual that is \
         no longer one. Delete the entry — the word's own table carries it:\n{}",
        stale.join("\n")
    );
}

#[test]
fn every_prose_defined_name_still_needs_recording() {
    // `PROSE_DEFINED` widens check 4's first half: a name listed here may be
    // referenced without any production writing it, because the prose beneath
    // its fence defines it. The claim dies when §3 grows a production for the
    // name — the fence is no longer the definition — and the entry stops
    // exempting anything when no production references the name at all.
    let productions = productions(&ebnf());
    let referenced: BTreeSet<String> = productions
        .iter()
        .flat_map(|(name, body)| {
            references(body)
                .into_iter()
                .filter(move |reference| reference != name)
        })
        .collect();
    let stale: Vec<String> = PROSE_DEFINED
        .iter()
        .filter_map(|(name, _)| {
            if productions.contains_key(*name) {
                Some(format!(
                    "  `{name}`: spec §3 writes a production for it now, so check 4 \
                     resolves it on its own"
                ))
            } else if !referenced.contains(*name) {
                Some(format!(
                    "  `{name}`: no production names it any more, so there is \
                     nothing left to resolve"
                ))
            } else {
                None
            }
        })
        .collect();
    assert!(
        stale.is_empty(),
        "entry(ies) in PROSE_DEFINED record a name as defined in prose that no \
         longer needs it. Delete the entry:\n{}",
        stale.join("\n")
    );
}

#[test]
fn every_prose_right_hand_side_still_needs_recording() {
    // `PROSE_RIGHT_HAND_SIDES` widens check 4 the hard way: it drops a whole
    // production's right-hand side out of the scan, because that side is
    // English rather than shape. So the question is the one check 4 would ask —
    // read as grammar, does this side name anything undefined? A production
    // rewritten into real shape names nothing undefined, and then the exemption
    // is blinding the check over a side it would pass.
    let productions = productions(&ebnf());
    let classes = token_classes();
    let stale: Vec<String> = PROSE_RIGHT_HAND_SIDES
        .iter()
        .filter_map(|(name, _)| {
            let Some(body) = productions.get(*name) else {
                return Some(format!(
                    "  `{name}`: spec §3 no longer writes this production, so the \
                     entry exempts nothing"
                ));
            };
            let undefined: Vec<String> = references(body)
                .into_iter()
                .filter(|reference| {
                    !productions.contains_key(reference)
                        && !classes.contains(reference)
                        && !PROSE_DEFINED
                            .iter()
                            .any(|(word, _)| *word == reference.as_str())
                })
                .collect();
            undefined.is_empty().then(|| {
                format!(
                    "  `{name}`: read as grammar its right-hand side names nothing \
                     undefined, so check 4 holds it as written"
                )
            })
        })
        .collect();
    assert!(
        stale.is_empty(),
        "entry(ies) in PROSE_RIGHT_HAND_SIDES take a production out of check 4 \
         that check 4 would pass. Delete the entry — the production is shape now, \
         and hiding it from the check hides its future drift too:\n{}",
        stale.join("\n")
    );
}

#[test]
fn every_unreached_production_is_still_written_and_still_unreached() {
    // `UNREACHED_PRODUCTIONS` widens check 4's second half for the start symbol
    // and nothing else. A production some other production reaches now needs no
    // exemption, and one the grammar no longer writes cannot be reached or
    // unreached — either way the entry is a claim about a document that has
    // moved on.
    let productions = productions(&ebnf());
    let referenced: BTreeSet<String> = productions
        .iter()
        .flat_map(|(name, body)| {
            references(body)
                .into_iter()
                .filter(move |reference| reference != name)
        })
        .collect();
    let stale: Vec<String> = UNREACHED_PRODUCTIONS
        .iter()
        .filter_map(|(name, _)| {
            if !productions.contains_key(*name) {
                Some(format!(
                    "  `{name}`: spec §3 no longer writes this production"
                ))
            } else if referenced.contains(*name) {
                Some(format!(
                    "  `{name}`: some production reaches it now, so check 4 holds \
                     it on its own"
                ))
            } else {
                None
            }
        })
        .collect();
    assert!(
        stale.is_empty(),
        "entry(ies) in UNREACHED_PRODUCTIONS record a production as unreachable \
         that is not. Delete the entry — the start symbol is the only one there \
         should ever be:\n{}",
        stale.join("\n")
    );
}
