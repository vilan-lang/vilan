//! The keyword table, EXPORTED (E225, K24): the lexer's [`KEYWORDS`] as the one
//! list every consumer outside the lexer reads, instead of a copy of its own.
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
//! The JSON's shape is the contract a consumer reads, so it is fixed here and
//! pinned: one object, `"keywords"`, the spellings sorted, one per line.
//! Contextual keywords (B414) are the lexer's to classify; when the table
//! grows that distinction, it is added here as a second field, never by
//! dropping a word from `"keywords"` that an old reader still highlights.

use crate::lexing::KEYWORDS;

/// Every spelling the lexer classifies as a keyword, sorted.
pub fn keywords() -> Vec<&'static str> {
    let mut words: Vec<&'static str> = KEYWORDS.iter().map(|(word, _)| *word).collect();
    words.sort_unstable();
    words
}

/// Whether `name` is lexed as a keyword rather than an identifier.
pub fn is_keyword(name: &str) -> bool {
    KEYWORDS.iter().any(|(word, _)| *word == name)
}

/// The table as the JSON document `vilan --print-keywords` prints:
///
/// ```text
/// {
///   "keywords": [
///     "async",
///     …
///   ]
/// }
/// ```
///
/// Written by hand rather than through a serializer: every spelling is a
/// lowercase ASCII identifier (pinned below), so no character needs escaping,
/// and the layout is part of what a committed copy diffs against.
pub fn to_json() -> String {
    let lines: Vec<String> = keywords()
        .iter()
        .map(|word| format!("    \"{word}\""))
        .collect();
    format!("{{\n  \"keywords\": [\n{}\n  ]\n}}\n", lines.join(",\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exported_list_is_the_lexers_table_sorted() {
        let words = keywords();
        assert_eq!(words.len(), KEYWORDS.len(), "every row, once");
        assert!(
            words.windows(2).all(|pair| pair[0] < pair[1]),
            "sorted and unique"
        );
        for (word, _) in KEYWORDS {
            assert!(words.contains(word), "{word} is exported");
            assert!(is_keyword(word));
        }
        // The words the drift was about, by name (E225, K24).
        for word in ["css", "dyn", "lazy"] {
            assert!(is_keyword(word), "{word} is a keyword");
        }
        assert!(!is_keyword("resource"), "B413: `resource` is an attribute");
    }

    #[test]
    fn every_spelling_is_a_plain_identifier_so_the_json_needs_no_escaping() {
        for word in keywords() {
            assert!(
                word.bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_'),
                "{word:?} would need escaping in to_json"
            );
        }
    }

    #[test]
    fn the_json_carries_every_keyword_in_order() {
        let json = to_json();
        assert!(
            json.starts_with("{\n  \"keywords\": [\n    \"async\",\n"),
            "{json}"
        );
        assert!(json.ends_with("\"\n  ]\n}\n"), "{json}");
        let listed: Vec<&str> = json
            .lines()
            .filter_map(|line| line.trim().trim_end_matches(',').strip_prefix('"'))
            .filter_map(|line| line.strip_suffix('"'))
            .filter(|word| *word != "keywords")
            .collect();
        assert_eq!(listed, keywords());
    }
}
