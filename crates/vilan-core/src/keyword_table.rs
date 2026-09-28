//! The keyword table, EXPORTED (E225, K24): the lexer's [`KEYWORDS`] and its
//! [`CONTEXTUAL_KEYWORDS`] (B414) as the one list every consumer outside the
//! lexer reads, instead of a copy of its own.
//!
//! Two copies had drifted by the time anyone looked (Order 42): bindgen's
//! reserved-name list lacked `css`, `dyn` and `lazy` — so a TypeScript member
//! named one of them generated bindings that do not parse — and still named
//! `resource`, which B413 dissolved into the `[resource]` attribute; and the
//! website playground's editor carried the same four mistakes in its own list.
//! Both now derive from here: bindgen through [`is_keyword`], the site through
//! `vilan --print-keywords`, which prints [`to_json`] — the site's editor
//! imports that file, and the site's test diffs it against the toolchain's.
//!
//! The TextMate grammar and the book's highlight theme were already generated
//! from [`KEYWORDS`] and gated (`grammar_sync.rs`, E91), and the keyword hover
//! docs are held to it in `vilan-lsp`; this module is the same discipline for
//! the consumers that cannot link the crate.
//!
//! **Two kinds of keyword (B414).** A RESERVED word ([`KEYWORDS`]) is lexed as
//! a keyword token and can never be a name — what bindgen must escape. A
//! CONTEXTUAL keyword ([`CONTEXTUAL_KEYWORDS`]: `with`, `own`, `dyn`, `lazy`,
//! `as`, `context`, …) lexes as an identifier and reads as a keyword only at
//! its position — a legal name everywhere else, so bindgen leaves it alone and
//! a highlighter paints it only where it is one.
//!
//! The JSON's shape is the contract a consumer reads, so it is fixed here and
//! pinned: one object; `"keywords"` lists EVERY word with a keyword reading,
//! reserved and contextual, sorted, one per line — so a reader that highlights
//! that list keeps every word it highlighted before B414 demoted six of them;
//! `"contextual"` lists the subset that is contextual, for a reader that paints
//! those by position. The reserved words are the difference.

use crate::lexing::{CONTEXTUAL_KEYWORDS, KEYWORDS};

/// Every word with a keyword reading — reserved and contextual — sorted.
pub fn keywords() -> Vec<&'static str> {
    let mut words: Vec<&'static str> = reserved();
    words.extend(contextual());
    words.sort_unstable();
    words
}

/// The RESERVED words — lexed as keyword tokens, never a name — sorted.
pub fn reserved() -> Vec<&'static str> {
    let mut words: Vec<&'static str> = KEYWORDS.iter().map(|(word, _)| *word).collect();
    words.sort_unstable();
    words
}

/// The CONTEXTUAL keywords (B414) — identifiers to the lexer, keywords by
/// position — sorted.
pub fn contextual() -> Vec<&'static str> {
    let mut words: Vec<&'static str> = CONTEXTUAL_KEYWORDS.iter().map(|(word, _)| *word).collect();
    words.sort_unstable();
    words
}

/// Whether `name` is lexed as a keyword rather than an identifier — a
/// RESERVED word, which no name may be. A contextual keyword answers `false`:
/// it is a legal name.
pub fn is_keyword(name: &str) -> bool {
    KEYWORDS.iter().any(|(word, _)| *word == name)
}

/// The table as the JSON document `vilan --print-keywords` prints:
///
/// ```text
/// {
///   "keywords": [
///     "Self",
///     "as",
///     "async",
///     …
///   ],
///   "contextual": [
///     "Self",
///     "as",
///     …
///   ]
/// }
/// ```
///
/// Written by hand rather than through a serializer: every spelling is an
/// ASCII identifier (pinned below), so no character needs escaping, and the
/// layout is part of what a committed copy diffs against.
pub fn to_json() -> String {
    let list = |words: Vec<&str>| -> String {
        words
            .iter()
            .map(|word| format!("    \"{word}\""))
            .collect::<Vec<_>>()
            .join(",\n")
    };
    format!(
        "{{\n  \"keywords\": [\n{}\n  ],\n  \"contextual\": [\n{}\n  ]\n}}\n",
        list(keywords()),
        list(contextual())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exported_list_is_the_lexers_two_tables_sorted() {
        let words = keywords();
        assert_eq!(
            words.len(),
            KEYWORDS.len() + CONTEXTUAL_KEYWORDS.len(),
            "every row of both tables, once"
        );
        assert!(
            words.windows(2).all(|pair| pair[0] < pair[1]),
            "sorted and unique"
        );
        for (word, _) in KEYWORDS {
            assert!(words.contains(word), "{word} is exported");
            assert!(is_keyword(word));
        }
        for (word, _) in CONTEXTUAL_KEYWORDS {
            assert!(words.contains(word), "{word} is exported");
            assert!(contextual().contains(word));
            assert!(!is_keyword(word), "{word} is a legal name (B414)");
        }
        // The words the drift was about, by name (E225, K24): `css` is
        // reserved; B414 made `dyn` and `lazy` contextual.
        assert!(is_keyword("css"));
        for word in ["dyn", "lazy", "with", "own", "borrows", "jump"] {
            assert!(!is_keyword(word) && contextual().contains(&word), "{word}");
        }
        assert!(!is_keyword("resource"), "B413: `resource` is an attribute");
        assert!(!words.contains(&"resource"));
    }

    #[test]
    fn every_spelling_is_a_plain_identifier_so_the_json_needs_no_escaping() {
        for word in keywords() {
            assert!(
                word.bytes()
                    .all(|byte| byte.is_ascii_alphabetic() || byte == b'_'),
                "{word:?} would need escaping in to_json"
            );
        }
    }

    #[test]
    fn the_json_carries_every_keyword_then_the_contextual_ones_in_order() {
        let json = to_json();
        assert!(json.starts_with("{\n  \"keywords\": [\n    \""), "{json}");
        assert!(json.ends_with("\"\n  ]\n}\n"), "{json}");
        let (keywords_part, contextual_part) = json
            .split_once("\"contextual\"")
            .expect("a contextual field");
        let quoted = |part: &str| -> Vec<String> {
            part.lines()
                .filter_map(|line| line.trim().trim_end_matches(',').strip_prefix('"'))
                .filter_map(|line| line.strip_suffix('"'))
                .filter(|word| *word != "keywords")
                .map(str::to_string)
                .collect()
        };
        assert_eq!(quoted(keywords_part), keywords());
        assert_eq!(quoted(contextual_part), contextual());
    }
}
