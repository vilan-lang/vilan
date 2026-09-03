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
//!    A key is split on its `{...}` slots and `...` elisions and its longest
//!    literal fragment is searched, fixed-string, over the compiler sources and
//!    `vilan/std`. A reworded or deleted message reds the row that keys on it.
//!    This is the L13 re-key, run by the suite instead of by a lane.
//! 3. **Every diagnostic is rowed**
//!    ([`every_diagnostic_the_compiler_builds_is_indexed`]). Two anchors are
//!    enumerated in full — every `Error { .. msg: <literal> }` in the compiler
//!    crates, and every `errors.push(<literal>)` in `manifest.rs` (the manifest
//!    refusal family) — and a message no row's key matches as a prefix reds,
//!    named by file, line and text.
//! 4. **Every appendix entry still names a live message**
//!    ([`every_errors_appendix_entry_still_names_a_live_message`]) — the same
//!    fixed-string search over each `**"..."**` head.
//! 5. **Every flagship row is in the appendix**
//!    ([`every_flagship_row_is_quoted_by_the_errors_appendix`]) — a row the
//!    index marks `flagship` must be quoted by some appendix entry. Dropping an
//!    entry, or marking a new row flagship without writing one, reds.
//! 6. **The two exemptions expire** (N42, and N27's rule for `#[ignore]`
//!    reasons applied to a list). Both
//!    [`every_hand_rowed_row_is_still_out_of_the_enumerations_reach`] and
//!    [`every_fragmentless_key_still_has_no_fragment`] ask the INVERSE
//!    question: a row `ROWS_THE_ENUMERATION_CANNOT_REACH` names that the walk
//!    now reaches, and a key `KEYS_WITHOUT_A_FRAGMENT` names that is now
//!    searchable, are each an exemption subtracting a check for nothing — and
//!    each stays green forever without this, because a list that only ever
//!    subtracts work cannot red by being wrong.
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
const KEYS_WITHOUT_A_FRAGMENT: &[(&str, &str)] = &[(
    "291",
    "`{headline}{subject}: {}` — const evaluation's failure envelope; the two \
     headlines (`const evaluation failed`, `const evaluation did not finish \
     within the compile-time budget`) are separate literals in `const_eval.rs`",
)];

/// Rows the enumeration cannot reach, ROWED BY HAND, with why the walk does
/// not get to each (N41).
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
        "352",
        "the `+` ladder's unbounded-parameter concatenation arm (`analyzer.rs`)",
    ),
    ("353", "its B179 arm — a parameter right of a number's `+`"),
    ("354", "its B176 arm — bounded, but to the wrong promise"),
    ("355", "its plain no-string-form arm"),
    ("356", "its `str`-on-the-right arm"),
    ("357", "its same-type arm, the ladder's fallthrough"),
    // B200's unary ladders, built the same way and handed over by the same
    // field shorthand — one `let msg = if … else if …` per operator. Their
    // arms differ in HEAD, not just in a slot (`-` on a native non-numeric
    // states an admitted set; on an aggregate it states that no `Neg` trait
    // exists to give the symbol a meaning; on a parameter it states why no
    // bound can prove membership), so folding them into one templated
    // `msg:` literal the enumeration could read would cost the sentences
    // their accuracy. Rowed by hand instead, as the `+` ladder above is.
    ("368", "`!`'s parameter arm (`analyzer.rs`)"),
    ("369", "`!`'s `void` arm"),
    (
        "370",
        "`!`'s truthiness arm — every other non-`bool` operand",
    ),
    ("371", "`-`'s parameter arm"),
    ("372", "`-`'s `bool` arm"),
    ("373", "`-`'s `str` arm"),
    ("374", "`-`'s backed-enum arm"),
    ("375", "`-`'s `void` arm"),
    (
        "376",
        "`-`'s no-`Neg`-trait arm — every other non-numeric operand",
    ),
    // B197's operator-conformance refusal shares the missing-member push with
    // the ordinary one, choosing between them in a `let msg = if …` the walk
    // cannot read for the same reason.
    (
        "378",
        "the operator arm of the trait-conformance refusal (`analyzer.rs`)",
    ),
    ("379", "`!`'s trait-typed arm"),
    ("380", "`-`'s trait-typed arm"),
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
        "the nine `ParseErrorReason::Rule` statements, held by the parse-rule tests",
    ),
];

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
    for crate_directory in ["vilan-core", "vilan-cli", "vilan-lsp", "vilan-wasm"] {
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
    /// The ledger row number — or `None` for a row still written `NEW`.
    ///
    /// A lane that ships a message owes it a row in the same change, but it
    /// cannot know its NUMBER: every other lane of the same order is adding
    /// rows to the same tail of this file, so a minted number is a collision
    /// waiting for the merge and a renumbering afterwards. `NEW` is what a
    /// lane writes instead; integration assigns the numbers when the branches
    /// land, in the order they land. Every other check treats a `NEW` row
    /// exactly as a numbered one — it must have a key, that key must still be
    /// in the tree, and it holds enumerated sites the same way — so a message
    /// gains its coverage the moment it ships rather than at the merge.
    number: Option<u32>,
    flagship: bool,
    key: String,
}

impl Row {
    /// How the row names itself in a failure message, and the string the
    /// exemption lists are keyed on: the number, or `NEW`. A `NEW` row matches
    /// no exemption, which is the right default — an exemption is an argument
    /// about a specific row, and there is no specific row yet.
    fn label(&self) -> String {
        match self.number {
            Some(number) => number.to_string(),
            None => NEW.to_string(),
        }
    }
}

/// The row id a lane writes for a message whose number integration assigns.
const NEW: &str = "NEW";

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
            number: match number {
                NEW => None,
                _ => Some(
                    number
                        .parse()
                        .unwrap_or_else(|_| panic!("row number: {number:?}")),
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

/// The message surface this file claims to enumerate — see the header for what
/// it deliberately leaves out.
fn enumerated_sites() -> Vec<Site> {
    let root = repository_root();
    let mut sites = Vec::new();
    for crate_directory in ["vilan-core", "vilan-cli", "vilan-lsp", "vilan-wasm"] {
        let mut paths = Vec::new();
        walk(
            &root.join("crates").join(crate_directory).join("src"),
            "rs",
            &mut paths,
        );
        for path in paths {
            sites.extend(anchored_messages(&path, "msg:"));
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

// --- The gates -------------------------------------------------------------

#[test]
fn the_index_is_well_formed() {
    let rows = index();
    assert!(rows.len() > 300, "the index lost rows: {}", rows.len());
    let mut previous = 0u32;
    let mut unnumbered = false;
    for row in &rows {
        match row.number {
            Some(number) => {
                // A numbered row after a `NEW` one would be renumbered into the
                // middle of the file at integration, which is how the ledger's
                // order and this file's order come apart.
                assert!(
                    !unnumbered,
                    "row {number} is numbered but follows a `{NEW}` row; the rows \
                     awaiting a number belong at the END of the index"
                );
                assert!(
                    number > previous,
                    "row {number} is out of order (after {previous})"
                );
                previous = number;
            }
            None => unnumbered = true,
        }
        assert!(
            !row.key.trim().is_empty(),
            "row {} has an empty key",
            row.label()
        );
        let exempt = KEYS_WITHOUT_A_FRAGMENT
            .iter()
            .any(|(number, _)| *number == row.label());
        assert!(
            exempt || longest_fragment(&row.key).is_some(),
            "row {}'s key has no literal run of {MIN_FRAGMENT} characters to search for: {:?}.\n\
             Either the key is wrong, or the head is COMPOSED — record it in \
             KEYS_WITHOUT_A_FRAGMENT with the reason.",
            row.label(),
            row.key
        );
    }
    for (number, _) in ROWS_WITHOUT_A_KEY {
        assert!(
            !rows.iter().any(|row| row.label() == *number),
            "row {number} is recorded as keyless but appears in the index"
        );
    }
}

#[test]
fn every_indexed_row_still_lives_in_the_tree() {
    let blob = source_blob();
    let mut stale = Vec::new();
    for row in index() {
        let Some(fragment) = longest_fragment(&row.key) else {
            continue;
        };
        if !blob.contains(&fragment) {
            stale.push(format!(
                "  row {}: {:?}\n      not in the tree: {fragment:?}",
                row.label(),
                row.key
            ));
        }
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
                .any(|(number, _)| *number == row.label());
            (row, composed)
        })
        .collect();
    let mut unrowed = Vec::new();
    for site in enumerated_sites() {
        if site.is_note {
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

#[test]
fn every_errors_appendix_entry_still_names_a_live_message() {
    let blob = source_blob();
    let mut stale = Vec::new();
    for (line, head) in appendix_heads() {
        // Two passes: the head as written, then with its code spans read as
        // holes, because an entry may quote an illustrative filling.
        let strict: Vec<String> = appendix_fragments(&head, false);
        if strict.iter().all(|fragment| blob.contains(fragment)) {
            continue;
        }
        let relaxed: Vec<String> = appendix_fragments(&head, true);
        let missing: Vec<String> = relaxed
            .into_iter()
            .filter(|fragment| !blob.contains(fragment))
            .collect();
        if missing.is_empty() {
            continue;
        }
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
            undocumented.push(format!("  row {}: {:?}", row.label(), row.key));
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
        // An ENTRY head is a line that is nothing but bold (plus an optional
        // italic aside). A bold word leading a sentence of prose is not one.
        let is_entry_head = line.starts_with("**")
            && line
                .trim_end()
                .trim_end_matches([')', '*', '('])
                .ends_with("**")
            || line.starts_with("**") && line.trim_end().ends_with("**");
        if !is_entry_head || line.starts_with("**\"") {
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

#[test]
fn every_hand_rowed_row_is_in_the_index() {
    // The exemption above is only worth its ink while the rows it names are
    // real: a hand-rowed message that loses its row loses ALL coverage, since
    // the enumeration never reached it to begin with.
    let rows = index();
    let missing: Vec<&str> = ROWS_THE_ENUMERATION_CANNOT_REACH
        .iter()
        .map(|(number, _)| *number)
        .filter(|number| !rows.iter().any(|row| row.label() == *number))
        .collect();
    assert!(
        missing.is_empty(),
        "row(s) {missing:?} are recorded in ROWS_THE_ENUMERATION_CANNOT_REACH but \
         are not in `{INDEX}`. A message the enumeration cannot see is held by \
         its row and nothing else, so dropping the row drops the message."
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
        .filter_map(|(number, _)| {
            let row = rows.iter().find(|row| row.label() == *number)?;
            sites
                .iter()
                .filter(|site| !site.is_note)
                .find(|site| key_describes(&row.key, &site.message))
                .map(|site| {
                    format!(
                        "  row {number}: now enumerated at {}:{}",
                        site.file, site.line
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
                .any(|(number, _)| *number == row.label())
        })
        .filter_map(|row| {
            let label = row.label();
            longest_fragment(&row.key)
                .map(|fragment| format!("  row {label}: searchable as {fragment:?}"))
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
    let files: BTreeSet<&str> = sites.iter().map(|site| site.file.as_str()).collect();
    assert!(files.len() > 5, "the enumeration reaches only {files:?}");
}
