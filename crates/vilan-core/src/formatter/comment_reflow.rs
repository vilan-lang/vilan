//! E205: re-filling a PARAGRAPH of comment lines to the line width, behind
//! `[fmt] wrap_comments`.
//!
//! The formatter has always laid out code to a width and left comments exactly
//! as typed, so a `//` line runs past the budget as far as its author took it
//! and a paragraph edited in the middle keeps its ragged lines forever. This
//! module closes that, and it is the whole of the risk: the printer's other
//! work is checked by a token stream, and a comment is trivia the lexer drops
//! — nothing downstream can tell the formatter it just rewrote a sentence.
//!
//! So the design is a wall of refusals with a small filler behind it. Ten
//! kinds of comment line are NEVER touched, each because re-wrapping it
//! destroys information no byte comparison would notice:
//!
//! 1. **Fences** — a ```` ``` ```` region is verbatim by definition.
//! 2. **Lists** — an item is a line; joining two items makes one item, and a
//!    continuation's hanging indent is an interior space run (rule 6) anyway.
//! 3. **Tables** — a `|` row's columns are its structure.
//! 4. **Headings** — a `#` line is a break in the document, not a sentence in
//!    a paragraph.
//! 5. **Block quotes** — a `>` line is quoted text; its breaks are the
//!    quotation's.
//! 6. **An interior run of two or more spaces** — aligned columns, an ASCII
//!    drawing, a hand-laid table. This one rule catches most of the others'
//!    continuations too.
//! 7. **Toolchain directives** — `// witness:`, `// GENERATED(..)`: a
//!    directive is READ by a tool, so its line structure is a contract.
//!    [`TOOLCHAIN_DIRECTIVES`] lists the ones the toolchain reads today, and
//!    the general lowercase-key rule covers the next one.
//! 8. **Commented-out code** — a `// list.push(x);` is code, and code's line
//!    breaks are where the author put them.
//! 9. **License headers** — a legal notice is quoted text with a fixed shape.
//! 10. **Section banners and horizontal rules** — `// ---------` over
//!     `// Placement`, or `// --- conditions ---`. This one was found by
//!     running the census over std and kolt rather than reasoned out
//!     ([`has_a_rule_run`]), which is the argument for running a census
//!     before trusting a list of rules.
//!
//! What survives is prose, and it is re-filled greedily, at the paragraph's own
//! marker (`//`, `///`, `//!`) and the printer's own indentation. Two further
//! rules hold the line: a URL or a `` `code span` `` longer than the budget is
//! never BROKEN (it takes a line of its own and overflows, exactly as a long
//! unbreakable word does in any filler), and no character is ever substituted —
//! the filler moves whitespace between atoms and does nothing else. What comes
//! out is checked by [`words_match`], which is the honest half: if the words are
//! not the same words in the same order, that is a bug in this module and
//! `vilan fmt` declines the file rather than write it.

/// The comment markers a paragraph may be written with, longest first so the
/// scan cannot read `///` as a `//` followed by a `/`.
pub(super) const MARKERS: &[&str] = &["///", "//!", "//"];

/// The directive prefixes the toolchain READS out of a `.vl` comment — the
/// grep is `crates/*/tests` and `crates/*/src` for a comment-shaped string
/// literal. A line starting with one of these is never reflowed, because its
/// line structure is a contract with a tool and not a typographic choice.
///
/// `witness:` / `witness-absent:` are `corpus.rs`'s claim directives;
/// `GENERATED(` / `END GENERATED(` are the seams `grammar_sync.rs` and
/// `mime_table_sync.rs` splice generated fragments between (`build.vl`'s mime
/// table is the in-tree one). The general rule in [`is_directive`] covers a
/// directive nobody has written yet: a lowercase key and a colon is the shape
/// every one of them has, and a missed reflow is the safe direction.
const TOOLCHAIN_DIRECTIVES: &[&str] = &[
    "witness:",
    "witness-absent:",
    "GENERATED(",
    "END GENERATED(",
];

/// Words that open a Vilan DECLARATION and nothing in English — the
/// unambiguous half of the commented-out-code test. `if`, `for`, `while`,
/// `match`, `use` and `type` are deliberately absent: they are ordinary
/// English words, and excluding every prose paragraph that starts a line with
/// "if" would cost more than the few commented-out `if`s it catches. The
/// suffix and infix tests in [`is_code`] reach those anyway, since real code
/// carries `;`, `{`, `}` or `=>`.
const CODE_OPENERS: &[&str] = &[
    "let ",
    "fun ",
    "const ",
    "import ",
    "export ",
    "impl ",
    "trait ",
    "struct ",
    "enum ",
    "external ",
    "resource ",
    "macro ",
    "mod ",
    "#[",
];

/// The narrowest body budget a reflow will work in. Below it the fill would be
/// one or two words a line, which is not a paragraph — so a comment nested
/// that deep is left as written. Unreachable in practice (fifteen levels of
/// indentation still leaves ~37 columns); it is a floor, not a policy.
const NARROWEST: usize = 20;

/// What [`reflow`] decided about one paragraph.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Reflow {
    /// The paragraph is left exactly as written, byte for byte — either
    /// because a never-reflow rule fired, or because it is already filled.
    AsWritten,
    /// These lines, in order, replace the paragraph (each already carrying its
    /// marker; the caller adds the indentation).
    Filled(Vec<String>),
    /// The fill changed the paragraph's WORDS. That is a defect in this module
    /// and never a thing to write: the caller declines the file.
    WordsChanged,
}

/// Re-fills one paragraph — a run of comment lines that were consecutive in
/// the source, share a marker, and are separated by nothing but a newline.
///
/// `budget` is the whole line's column budget at the printer's current
/// indentation, so the body's budget is what is left after the marker and its
/// one space.
///
/// The lines arrive as the lexer's trivia gave them: marker included, trailing
/// whitespace already trimmed, no leading indentation.
pub(super) fn reflow(lines: &[&str], budget: usize) -> Reflow {
    reflow_with(lines, budget, fill)
}

/// [`reflow`] with the filler handed in, so a pin can PLANT a broken one and
/// watch [`words_match`] catch it. The net holds by construction against the
/// real [`fill`], which is exactly why it cannot be pinned without a plant.
fn reflow_with(
    lines: &[&str],
    budget: usize,
    filler: fn(&[String], usize) -> Vec<String>,
) -> Reflow {
    let Some(marker) = shared_marker(lines) else {
        return Reflow::AsWritten;
    };
    let Some(bodies) = bodies(lines, marker) else {
        return Reflow::AsWritten;
    };
    if bodies.iter().any(|body| never_reflow(body)) {
        return Reflow::AsWritten;
    }
    let Some(body_budget) = budget
        .checked_sub(marker.len() + 1)
        .filter(|budget| *budget >= NARROWEST)
    else {
        return Reflow::AsWritten;
    };
    let Some(atoms) = atoms(&bodies) else {
        return Reflow::AsWritten;
    };
    if atoms.is_empty() {
        return Reflow::AsWritten;
    }
    let filled = filler(&atoms, body_budget);
    let rendered: Vec<String> = filled
        .iter()
        .map(|body| format!("{marker} {body}"))
        .collect();
    if rendered.len() == lines.len() && rendered.iter().zip(lines).all(|(new, old)| new == old) {
        return Reflow::AsWritten;
    }
    if !words_match(&bodies, &filled) {
        return Reflow::WordsChanged;
    }
    Reflow::Filled(rendered)
}

/// The marker every line in the paragraph is written with, or `None` if they
/// are not all written with the same one. A paragraph that mixes `//` and
/// `///` is two paragraphs as far as a reader is concerned, and joining them
/// would move a line into or out of a doc comment.
fn shared_marker(lines: &[&str]) -> Option<&'static str> {
    let marker_of = |line: &str| {
        MARKERS
            .iter()
            .find(|marker| line.starts_with(*marker))
            .copied()
    };
    let first = marker_of(lines.first()?)?;
    lines
        .iter()
        .all(|line| marker_of(line) == Some(first))
        .then_some(first)
}

/// Each line's body: the text after the marker and its ONE separating space.
///
/// `None` — leave the paragraph alone — when any line does not have exactly
/// that shape. A `//no space` line cannot be re-filled without INSERTING a
/// space after the marker, and a reflow never substitutes a character; an
/// empty `//` is a paragraph break the caller never puts in a paragraph, so
/// seeing one here means the caller's grouping and this function disagree, and
/// the safe answer is to write what was written.
fn bodies<'line>(lines: &[&'line str], marker: &str) -> Option<Vec<&'line str>> {
    lines
        .iter()
        .map(|line| line.get(marker.len()..)?.strip_prefix(' '))
        .collect()
}

/// Whether this line is one of the nine kinds a reflow never touches. See the
/// module header for why each is on the list.
fn never_reflow(body: &str) -> bool {
    let trimmed = body.trim_start();
    body.contains("  ")
        || body.contains('\t')
        || body.contains("```")
        || trimmed.starts_with("~~~")
        || body.contains('|')
        || trimmed.starts_with('#')
        || trimmed.starts_with('>')
        || has_a_rule_run(body)
        || starts_a_list_item(trimmed)
        || is_directive(trimmed)
        || is_license(body)
        || is_code(trimmed)
}

/// Whether `body` carries a RULE RUN — three or more of the same rule
/// character in a row.
///
/// This is the section banner and the horizontal rule, and it was found by
/// running the census rather than reasoned out: kolt's `lib/overlay.vl` writes
///
/// ```text
/// // ---------
/// // Placement
/// // ---------
/// ```
///
/// and std writes `// --- conditions ---` and
/// `// --- The sized numeric family (…) ---`. None of those is a list item
/// (`---` is not `- `), none has an interior space run, and joining them
/// turns a banner into one line of dashes with a word inside it. A markdown
/// thematic break (`---`, `***`, `___`) is the same shape and the same answer.
///
/// `.` is deliberately NOT a rule character: an ellipsis is prose.
fn has_a_rule_run(body: &str) -> bool {
    const RULE: &[char] = &['-', '=', '*', '~', '_', '#', '+'];
    let mut run = 0;
    let mut previous = None;
    for character in body.chars() {
        run = match previous == Some(character) && RULE.contains(&character) {
            true => run + 1,
            false => 1,
        };
        if run >= 3 && RULE.contains(&character) {
            return true;
        }
        previous = Some(character);
    }
    false
}

/// Whether `trimmed` opens a list item: a `-`/`*`/`+` bullet, or an ordered
/// marker (`1.`, `2)`). The bullet character alone counts, since an item whose
/// text starts on the next line is still an item.
fn starts_a_list_item(trimmed: &str) -> bool {
    let mut characters = trimmed.chars();
    match characters.next() {
        Some('-' | '*' | '+') => matches!(characters.next(), None | Some(' ')),
        Some(digit) if digit.is_ascii_digit() => {
            let rest = trimmed.trim_start_matches(|c: char| c.is_ascii_digit());
            let mut rest = rest.chars();
            matches!(rest.next(), Some('.' | ')')) && matches!(rest.next(), None | Some(' '))
        }
        _ => false,
    }
}

/// Whether `trimmed` is a toolchain directive: one of the prefixes a tool
/// reads today, or the SHAPE they all share — a single lowercase key and a
/// colon, at the very start of the body.
///
/// The general rule is deliberately lowercase-only. Prose lead-ins are
/// capitalized (`Note:`, `Safety:`) and reflowing one of those is harmless,
/// while every directive anyone has written is lowercase; a rule that took
/// both would exclude a good share of real paragraphs for nothing.
fn is_directive(trimmed: &str) -> bool {
    if TOOLCHAIN_DIRECTIVES
        .iter()
        .any(|directive| trimmed.starts_with(directive))
    {
        return true;
    }
    let Some(key) = trimmed.split(':').next().filter(|key| !key.is_empty()) else {
        return false;
    };
    trimmed.len() > key.len()
        && key
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '-')
}

/// Whether `body` reads as part of a license header. A legal notice is quoted
/// text: its line breaks are the licence's, not the file's.
fn is_license(body: &str) -> bool {
    let lowered = body.to_ascii_lowercase();
    ["copyright", "spdx-license-identifier", "licensed under"]
        .iter()
        .any(|phrase| lowered.contains(phrase))
}

/// Whether `trimmed` reads as commented-OUT CODE rather than prose.
///
/// A heuristic, and knowingly a conservative one: a false positive costs a
/// paragraph its reflow, a false negative rewraps somebody's disabled code.
/// `` `code spans` `` are removed first, because a prose sentence CITING a
/// path (``the `std::reactive` scheduler``) is prose and the `::` in it is a
/// quotation.
fn is_code(trimmed: &str) -> bool {
    let prose = without_code_spans(trimmed);
    let prose = prose.trim();
    if prose.is_empty() {
        return false;
    }
    if prose.ends_with(';')
        || prose.ends_with('{')
        || prose.ends_with('}')
        || prose.ends_with(',')
        || prose.ends_with('(')
        || prose.ends_with('=')
    {
        return true;
    }
    if prose.contains("::") || prose.contains("=>") || prose.contains("->") {
        return true;
    }
    CODE_OPENERS.iter().any(|opener| prose.starts_with(opener))
}

/// `text` with every backtick-delimited span replaced by a space, so a test
/// meant for prose does not read a quotation as code. An unterminated span
/// swallows the rest of the line, which is the conservative reading.
fn without_code_spans(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for character in text.chars() {
        if character == '`' {
            inside = !inside;
            out.push(' ');
        } else if !inside {
            out.push(character);
        }
    }
    out
}

/// The paragraph's ATOMS: its words in order, with each backtick-delimited
/// span held together as one unbreakable unit.
///
/// `None` when the paragraph's backticks do not pair up — there is then no
/// telling where a span ends, and a filler that guessed could break one.
fn atoms(bodies: &[&str]) -> Option<Vec<String>> {
    let mut atoms: Vec<String> = Vec::new();
    let mut open: Option<String> = None;
    for word in bodies.iter().flat_map(|body| body.split_whitespace()) {
        let ticks = word.chars().filter(|character| *character == '`').count();
        match &mut open {
            Some(span) => {
                span.push(' ');
                span.push_str(word);
                if ticks % 2 == 1 {
                    atoms.push(open.take().unwrap_or_default());
                }
            }
            None if ticks % 2 == 1 => open = Some(word.to_string()),
            None => atoms.push(word.to_string()),
        }
    }
    open.is_none().then_some(atoms)
}

/// Greedy fill: as many atoms per line as `budget` columns hold, one atom a
/// line minimum so an over-long URL or code span is never broken.
///
/// One extra rule, and it is what keeps the result re-readable as the same
/// paragraph: an atom that would make the next line LOOK like a list item, a
/// heading, a quote or a fence stays on the current line instead of opening
/// one. Otherwise a prose dash could land at a line start, the next run would
/// read the paragraph as a list, and the two runs would disagree about what
/// the paragraph is.
fn fill(atoms: &[String], budget: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for atom in atoms {
        if current.is_empty() {
            current.push_str(atom);
            continue;
        }
        if display_width(&current) + 1 + display_width(atom) <= budget {
            current.push(' ');
            current.push_str(atom);
            continue;
        }
        if !would_open_a_block(atom) {
            lines.push(std::mem::take(&mut current));
            current.push_str(atom);
            continue;
        }
        // The atom does not fit AND would open a block if it started the next
        // line. Take the word before it down with it, so the new line starts
        // with prose and the budget is still respected — kolt's `server.vl`
        // found this: a trailing `-` held on the current line pushed it to 102
        // columns.
        match take_last_word(&mut current) {
            Some(previous) => {
                lines.push(std::mem::take(&mut current));
                current.push_str(&previous);
                current.push(' ');
                current.push_str(atom);
            }
            // One word on the line: there is nothing to take down, so the
            // overflow is the lesser evil against a `-` at a line start.
            None => {
                current.push(' ');
                current.push_str(atom);
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Removes and returns the last word of `line`, leaving no trailing space —
/// `None` when the line holds one word or none, which is when there is
/// nothing to take down to the next line.
fn take_last_word(line: &mut String) -> Option<String> {
    let at = line.rfind(' ')?;
    let word = line[at + 1..].to_string();
    line.truncate(at);
    Some(word)
}

/// Whether starting a line with `atom` would make that line read as a list
/// item, a heading, a block quote, a table row or a fence — i.e. as something
/// [`never_reflow`] would refuse on the next run.
fn would_open_a_block(atom: &str) -> bool {
    starts_a_list_item(atom)
        || atom.starts_with('#')
        || atom.starts_with('>')
        || atom.starts_with('|')
        || atom.starts_with("```")
        || atom.starts_with("~~~")
}

/// The column width of comment text. Comment bodies hold no tabs — a body
/// with one is refused by [`never_reflow`] — so a character is a column, and
/// the count is in CHARACTERS rather than bytes so an em dash costs one.
fn display_width(text: &str) -> usize {
    text.chars().count()
}

/// The safety net: the paragraph's words, in order, unchanged.
///
/// Whitespace-split on both sides, which is exactly the property a fill is
/// allowed to change and nothing else. It holds by construction — [`atoms`]
/// keeps order and [`fill`] only redistributes — so a failure here is a defect
/// in this module, and the caller's answer to it is to decline the file.
fn words_match(before: &[&str], after: &[String]) -> bool {
    let written = before.iter().flat_map(|body| body.split_whitespace());
    let filled = after.iter().flat_map(|body| body.split_whitespace());
    written.eq(filled)
}

#[cfg(test)]
mod tests {
    use super::{Reflow, reflow};

    /// The budget a top-level comment is filled to: [`super::super::LINE_BUDGET`]
    /// at zero indentation.
    const WIDE: usize = 100;

    fn filled(lines: &[&str], budget: usize) -> Vec<String> {
        match reflow(lines, budget) {
            Reflow::Filled(lines) => lines,
            other => panic!("expected a fill, got {other:?}"),
        }
    }

    #[test]
    fn a_long_prose_line_is_filled_to_the_budget() {
        let long = "// the formatter has laid code out to a width since it existed and left every comment exactly as typed, which is the asymmetry this closes";
        let out = filled(&[long], WIDE);
        assert_eq!(
            out,
            vec![
                "// the formatter has laid code out to a width since it existed and left every comment exactly as".to_string(),
                "// typed, which is the asymmetry this closes".to_string(),
            ]
        );
        assert!(out.iter().all(|line| line.chars().count() <= WIDE));
    }

    #[test]
    fn a_ragged_paragraph_is_re_filled_rather_than_left() {
        let out = filled(
            &[
                "// one two",
                "// three four five six seven eight nine ten eleven twelve",
                "// thirteen",
            ],
            48,
        );
        assert_eq!(
            out,
            vec![
                "// one two three four five six seven eight nine".to_string(),
                "// ten eleven twelve thirteen".to_string(),
            ]
        );
    }

    #[test]
    fn an_already_filled_paragraph_is_byte_identical() {
        let lines = ["// one two three", "// four"];
        // Narrow enough that the written break is the one the fill would pick.
        assert_eq!(reflow(&lines, 20), Reflow::AsWritten);
    }

    #[test]
    fn the_fill_is_idempotent() {
        let long = "// alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau";
        let once = filled(&[long], 40);
        let borrowed: Vec<&str> = once.iter().map(String::as_str).collect();
        assert_eq!(reflow(&borrowed, 40), Reflow::AsWritten, "{once:?}");
    }

    #[test]
    fn a_doc_comment_keeps_its_marker() {
        let out = filled(
            &["/// what this does, at some length, in words that will not fit on one line"],
            40,
        );
        assert!(out.iter().all(|line| line.starts_with("/// ")));
    }

    #[test]
    fn a_paragraph_that_mixes_markers_is_left_alone() {
        assert_eq!(
            reflow(&["// plain and rather long indeed", "/// doc"], 20),
            Reflow::AsWritten
        );
    }

    #[test]
    fn a_line_without_a_space_after_the_marker_is_left_alone() {
        // Filling it would have to INSERT a space, and a reflow substitutes no
        // character.
        assert_eq!(
            reflow(&["//tight and long enough to want wrapping badly"], 20),
            Reflow::AsWritten
        );
    }

    #[test]
    fn a_long_url_takes_its_own_line_and_is_never_broken() {
        let url = "https://example.com/a/very/long/path/that/exceeds/the/budget/on/its/own";
        let out = filled(&[&format!("// see {url} for the rest")], 30);
        assert!(
            out.iter().any(|line| line == &format!("// {url}")),
            "{out:?}"
        );
        assert!(out.join(" ").contains(url));
    }

    #[test]
    fn a_code_span_is_one_atom_even_across_a_written_break() {
        let out = filled(&["// the `map filter", "// pair` is one name here now"], 60);
        assert_eq!(
            out,
            vec!["// the `map filter pair` is one name here now".to_string()]
        );
    }

    #[test]
    fn unbalanced_backticks_leave_the_paragraph_alone() {
        assert_eq!(
            reflow(
                &["// an `unterminated span that runs long and wants wrapping"],
                20
            ),
            Reflow::AsWritten
        );
    }

    #[test]
    fn a_banner_and_a_horizontal_rule_are_left_alone() {
        // Found by the census, not by reasoning: kolt's `lib/overlay.vl` and
        // std's section headers are written this way, and joining them turns a
        // banner into a line of dashes with a word inside it.
        assert_eq!(
            reflow(&["// ---------", "// Placement", "// ---------"], WIDE),
            Reflow::AsWritten
        );
        assert_eq!(
            reflow(
                &["// --- The sized numeric family (proposal/numeric-types.md) ---"],
                40
            ),
            Reflow::AsWritten
        );
        assert_eq!(
            reflow(&["// ***", "// and more words here"], 30),
            Reflow::AsWritten
        );
        // An ellipsis is prose, not a rule: `.` is not a rule character.
        assert!(matches!(
            reflow(
                &[
                    "// the first, the second, the third and so on... and then a good deal more text"
                ],
                40
            ),
            Reflow::Filled(_)
        ));
    }

    #[test]
    fn a_dash_never_lands_at_a_line_start() {
        // Otherwise the next run reads the paragraph as a list and the two
        // runs disagree about what it is.
        let out = filled(
            &["// alpha beta gamma delta - epsilon zeta eta theta iota kappa"],
            24,
        );
        assert!(out.iter().all(|line| !line.starts_with("// - ")), "{out:?}");
        let borrowed: Vec<&str> = out.iter().map(String::as_str).collect();
        assert_eq!(reflow(&borrowed, 24), Reflow::AsWritten, "{out:?}");
    }
}

#[cfg(test)]
mod safety_net {
    //! The words-in-order net, planted.
    //!
    //! Against the real [`super::fill`] the net holds by construction — atoms
    //! keep their order and the filler only moves whitespace between them — so
    //! there is no source that reaches it. A pin that waited for one would pin
    //! nothing, and the net is the whole reason the feature is safe. So the
    //! FILLER is planted: a broken one that drops a word, one that reorders,
    //! one that invents.

    use super::{Reflow, fill, reflow_with};

    /// A filler that loses the paragraph's last word.
    fn drops_a_word(atoms: &[String], budget: usize) -> Vec<String> {
        fill(&atoms[..atoms.len() - 1], budget)
    }

    /// A filler that keeps every word and swaps two of them.
    fn reorders(atoms: &[String], budget: usize) -> Vec<String> {
        let mut atoms = atoms.to_vec();
        atoms.swap(0, 1);
        fill(&atoms, budget)
    }

    /// A filler that adds a word nobody wrote.
    fn invents(atoms: &[String], budget: usize) -> Vec<String> {
        let mut atoms = atoms.to_vec();
        atoms.push("invented".to_string());
        fill(&atoms, budget)
    }

    const PARAGRAPH: &[&str] = &[
        "// the words of this paragraph are its contract and the filler moves",
        "// nothing but the whitespace between them",
    ];

    #[test]
    fn the_real_filler_passes_the_net() {
        // Non-vacuity from the other side: the same paragraph through the real
        // filler is a clean fill, so the three failures below are the plants'
        // and not the fixture's.
        assert!(matches!(
            reflow_with(PARAGRAPH, 60, fill),
            Reflow::Filled(_)
        ));
    }

    #[test]
    fn a_filler_that_drops_a_word_is_caught() {
        assert_eq!(
            reflow_with(PARAGRAPH, 60, drops_a_word),
            Reflow::WordsChanged
        );
    }

    #[test]
    fn a_filler_that_reorders_is_caught() {
        assert_eq!(reflow_with(PARAGRAPH, 60, reorders), Reflow::WordsChanged);
    }

    #[test]
    fn a_filler_that_invents_a_word_is_caught() {
        assert_eq!(reflow_with(PARAGRAPH, 60, invents), Reflow::WordsChanged);
    }
}
