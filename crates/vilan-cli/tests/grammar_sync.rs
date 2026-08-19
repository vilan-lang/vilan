//! The three-place rule, gated (AGENTS.md "A new keyword lands in THREE
//! places"; backlog D17): a keyword lands in the lexer, the TextMate grammar
//! (`editors/vscode/syntaxes/vilan.tmLanguage.json`) and the book's
//! highlight.js theme (`vilan/docs/theme/vilan.js`) — and the same drift
//! reaches the primitive-type and attribute-marker lists that sit beside the
//! keywords in both grammars. `resource` shipped in the lexer alone and was
//! caught twice by eye; the D15 docs audit then found `i64`/`u64` still
//! coloured as types a release after they became a hard error, and `platform`
//! missing from both attribute lists. This file is the lexer-vs-list diff that
//! AGENTS.md asks for, run on every suite.
//!
//! The compiler's lists are read programmatically — `lexing::KEYWORDS`,
//! `type_::SCALAR_PRIMITIVE_NAMES`, `type_::NUMERIC_SUFFIXES`,
//! `parsing::KNOWN_ATTRIBUTE_MARKERS` are each the table their own consumer
//! reads — so nothing here is a second copy to drift. The grammars are read
//! with node, the way `vscode_extension.rs` reads the extension manifest: the
//! TextMate file as JSON, and `vilan.js` *evaluated* under a stub `hljs`, so
//! the word lists checked are the ones the real highlighter registers (the
//! keyword string is built by concatenation; a text scrape would read it wrong).
//!
//! Each axis is checked in both directions: everything the compiler knows is in
//! both grammars, and nothing in either grammar is unknown to the compiler —
//! the `i64` direction — with the contextual words the grammars colour by
//! position (`context`, `sync`, …) allowed explicitly and pinned to still lex
//! as identifiers.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use vilan_core::lexing::{KEYWORDS, tokenize};
use vilan_core::parsing::KNOWN_ATTRIBUTE_MARKERS;
use vilan_core::token::Token;
use vilan_core::type_::{NUMERIC_SUFFIXES, SCALAR_PRIMITIVE_NAMES};

const TEXTMATE_GRAMMAR: &str = "editors/vscode/syntaxes/vilan.tmLanguage.json";
const HIGHLIGHT_THEME: &str = "vilan/docs/theme/vilan.js";

/// Words a grammar may colour as keywords although the lexer hands them back
/// as identifiers. Each is CONTEXTUAL — a keyword in one position and a plain
/// name anywhere else — and the grammars match it by position (the TextMate
/// grammar and `vilan.js` both anchor `context` after a closure type's `)` and
/// `sync` after the `(` that opens one). Pinned to lex as `Token::Ident`: the
/// day one is promoted to a real keyword (a `KEYWORDS` row), this list must
/// shrink by it.
const CONTEXTUAL_WORDS: &[(&str, &str)] = &[
    (
        "context",
        "the clause on a closure type: `(|| void) context owner`",
    ),
    (
        "sync",
        "the marker opening a closure type: `(sync || View)`",
    ),
    ("self", "the receiver parameter"),
    ("Self", "the implementing type inside an `impl`"),
    (
        "void",
        "the unit type — `Token::Ident(\"void\")` in type position",
    ),
];

/// Type names the TextMate grammar colours as primitives that are not
/// scalar-view primitives: `bool` is the numeric enum `type_.rs` keeps BESIDE
/// `SCALAR_PRIMITIVE_NAMES` (never in it, AGENTS.md); `void` and `any` are the
/// analyzer's two built-in type names (`walk_type_node`).
const BUILT_IN_TYPE_WORDS: &[&str] = &["bool", "void", "any"];

/// Numeric suffixes the grammar must NOT accept, probed beside the accepted
/// list. `i64`/`u64` are the live example (renamed to `i53`/`u53`, a hard
/// error since); the rest are the spellings a neighbouring language would
/// suggest.
const REJECTED_SUFFIX_PROBES: &[&str] = &["i64", "u64", "i128", "u128", "f16", "f128", "q"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs `script` under node with `VILAN_FILE` naming `file` and `VILAN_PROBES`
/// carrying `probes` (comma-separated), returning stdout's lines.
fn node(script: &str, file: &Path, probes: &[&str]) -> Vec<String> {
    let output = Command::new("node")
        .args(["-e", script])
        .env("VILAN_FILE", file)
        .env("VILAN_PROBES", probes.join(","))
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "reading {}: {}",
        file.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// One regex of a grammar: which list it belongs to (a TextMate repository
/// key, or a highlight.js `className`), the field it was read from, and the
/// pattern itself.
#[derive(Debug)]
struct Rule {
    key: String,
    field: String,
    regex: String,
}

/// A grammar's rules plus, for each probe word, the keys of the rules whose
/// regex matched it (`(key, word)`).
struct Grammar {
    rules: Vec<Rule>,
    matches: Vec<(String, String)>,
    /// highlight.js only: the `keywords` object, `(group, words)`.
    keyword_groups: Vec<(String, Vec<String>)>,
}

impl Grammar {
    fn rules(&self, key: &str) -> Vec<&Rule> {
        self.rules.iter().filter(|rule| rule.key == key).collect()
    }

    /// Every literal word in every regex under `key` (see [`literal_words`]).
    fn words(&self, key: &str) -> BTreeSet<String> {
        self.rules(key)
            .into_iter()
            .flat_map(|rule| literal_words(&rule.regex))
            .collect()
    }

    fn matched(&self, key: &str, word: &str) -> bool {
        self.matches
            .iter()
            .any(|(rule_key, matched)| rule_key == key && matched == word)
    }

    fn keyword_group(&self, group: &str) -> BTreeSet<String> {
        self.keyword_groups
            .iter()
            .filter(|(name, _)| name == group)
            .flat_map(|(_, words)| words.iter().cloned())
            .collect()
    }
}

/// Parses node's line protocol: `rule\t<key>\t<field>\t<regex>`,
/// `match\t<key>\t<word>`, `keywords\t<group>\t<words>`. Split on the first
/// tabs only, so a regex may contain a tab (a newline it may not — the script
/// refuses one).
fn parse_grammar(lines: Vec<String>) -> Grammar {
    let mut grammar = Grammar {
        rules: Vec::new(),
        matches: Vec::new(),
        keyword_groups: Vec::new(),
    };
    for line in lines {
        let mut fields = line.splitn(4, '\t');
        let kind = fields.next().unwrap_or("");
        let first = fields.next().unwrap_or("").to_string();
        let second = fields.next().unwrap_or("").to_string();
        match kind {
            "rule" => grammar.rules.push(Rule {
                key: first,
                field: second,
                regex: fields.next().unwrap_or("").to_string(),
            }),
            "match" => grammar.matches.push((first, second)),
            "keywords" => grammar.keyword_groups.push((
                first,
                second
                    .split_whitespace()
                    // highlight.js allows `word|relevance`.
                    .map(|word| word.split('|').next().unwrap_or("").to_string())
                    .collect(),
            )),
            other => panic!("unexpected line from node: {other:?} in {line:?}"),
        }
    }
    assert!(!grammar.rules.is_empty(), "node reported no rules");
    grammar
}

/// The TextMate grammar: every `match`/`begin`/`end` under each repository
/// entry, nested patterns included, keyed by the repository name; plus, for
/// each probe, the keys whose regexes match it (a regex node cannot compile —
/// an Oniguruma-only feature — is skipped for probing, never for listing).
fn textmate_grammar(probes: &[&str]) -> Grammar {
    const SCRIPT: &str = r#"
        const grammar = require(process.env.VILAN_FILE);
        const probes = process.env.VILAN_PROBES.split(",").filter(Boolean);
        function walk(key, patterns) {
            for (const pattern of patterns || []) {
                for (const field of ["match", "begin", "end"]) {
                    const regex = pattern[field];
                    if (typeof regex !== "string") continue;
                    if (regex.includes("\n")) throw new Error("a regex with a newline: " + key);
                    console.log(["rule", key, field, regex].join("\t"));
                    for (const probe of probes) {
                        let compiled;
                        try { compiled = new RegExp(regex); } catch (_) { continue; }
                        if (compiled.test(probe)) console.log(["match", key, probe].join("\t"));
                    }
                }
                walk(key, pattern.patterns);
            }
        }
        for (const key of Object.keys(grammar.repository)) {
            walk(key, grammar.repository[key].patterns);
        }
    "#;
    parse_grammar(node(SCRIPT, &repo_root().join(TEXTMATE_GRAMMAR), probes))
}

/// The highlight.js language, as `vilan.js` registers it: the file is run
/// under a stub `hljs` (and the `document`/`window` its two IIFEs touch), and
/// the definition `registerLanguage` receives is printed — its `keywords`
/// groups, and every `contains` rule's `begin` (a `variants` rule contributes
/// each variant's) keyed by the rule's `className`. Probes are matched against
/// the `number` rule's regexes, whole-string.
fn highlight_grammar(probes: &[&str]) -> Grammar {
    const SCRIPT: &str = r#"
        const fs = require("fs");
        const vm = require("vm");
        const probes = process.env.VILAN_PROBES.split(",").filter(Boolean);
        const languages = {};
        globalThis.hljs = {
            registerLanguage(name, define) { languages[name] = define(globalThis.hljs); },
            COMMENT(begin, end) { return { className: "comment", begin, end }; },
            highlightElement() {},
        };
        globalThis.document = { querySelectorAll() { return []; } };
        globalThis.window = {};
        const file = process.env.VILAN_FILE;
        vm.runInThisContext(fs.readFileSync(file, "utf8"), { filename: file });
        const language = languages.vilan;
        if (!language) throw new Error("vilan.js registered no `vilan` language");
        for (const [group, words] of Object.entries(language.keywords)) {
            console.log(["keywords", group, Array.isArray(words) ? words.join(" ") : words].join("\t"));
        }
        const source = (regex) => (typeof regex === "string" ? regex : regex.source);
        const begins = (rule) => (rule.variants ? rule.variants.map((variant) => variant.begin) : [rule.begin])
            .filter(Boolean)
            .map(source);
        for (const rule of language.contains) {
            const key = rule.className || "";
            for (const begin of begins(rule)) {
                if (begin.includes("\n")) throw new Error("a regex with a newline: " + key);
                console.log(["rule", key, "begin", begin].join("\t"));
                if (key !== "number") continue;
                for (const probe of probes) {
                    if (new RegExp("^(?:" + begin + ")$").test(probe)) {
                        console.log(["match", key, probe].join("\t"));
                    }
                }
            }
        }
    "#;
    parse_grammar(node(SCRIPT, &repo_root().join(HIGHLIGHT_THEME), probes))
}

/// The literal words a regex spells out: every maximal identifier-shaped run
/// (`[A-Za-z_][A-Za-z0-9_]*`) that is neither an escape (`\b`, `\s`) nor inside
/// a bracket class (`[A-Z]`). A regex written as a word list — `\b(if|else)\b`,
/// `(?:derive|service)\b`, `(?<=\))\s+(context)\b` — yields exactly its words;
/// a shape rule — `\b[A-Z][A-Za-z0-9_]*\b` — yields none.
fn literal_words(regex: &str) -> Vec<String> {
    let bytes = regex.as_bytes();
    let mut words = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            // An escape and the character it escapes.
            b'\\' => index += 2,
            // A bracket class, escapes inside it included.
            b'[' => {
                index += 1;
                while index < bytes.len() && bytes[index] != b']' {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
                index += 1;
            }
            byte if byte.is_ascii_alphanumeric() || byte == b'_' => {
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                if !bytes[start].is_ascii_digit() {
                    words.push(regex[start..index].to_string());
                }
            }
            _ => index += 1,
        }
    }
    words
}

fn lexer_keywords() -> BTreeSet<String> {
    KEYWORDS
        .iter()
        .map(|(keyword, _)| keyword.to_string())
        .collect()
}

fn set(words: &[&str]) -> BTreeSet<String> {
    words.iter().map(|word| word.to_string()).collect()
}

fn sorted(words: &BTreeSet<String>) -> Vec<&str> {
    words.iter().map(String::as_str).collect()
}

/// The words the TextMate grammar colours as keywords: every word listed under
/// its `keywords` repository entry (`keyword.control`, `storage.type`, the
/// contextual `context`/`sync` rules, the literals, `self`).
fn textmate_keyword_words(grammar: &Grammar) -> BTreeSet<String> {
    let words = grammar.words("keywords");
    assert!(
        words.len() >= KEYWORDS.len(),
        "{TEXTMATE_GRAMMAR}: the `keywords` repository lists only {words:?} — did its shape change?"
    );
    words
}

/// The words `vilan.js` colours as keywords: the `keyword` and `literal`
/// groups of its `keywords` object, plus the word of every `contains` rule
/// whose `className` is `keyword` (the position-anchored `context`/`sync`).
fn highlight_keyword_words(grammar: &Grammar) -> BTreeSet<String> {
    let mut words = grammar.keyword_group("keyword");
    words.extend(grammar.keyword_group("literal"));
    words.extend(grammar.words("keyword"));
    assert!(
        words.len() >= KEYWORDS.len(),
        "{HIGHLIGHT_THEME}: the registered language's keyword groups list only {words:?} — did its shape change?"
    );
    words
}

// --- Keywords ----------------------------------------------------------------

#[test]
fn every_lexer_keyword_is_in_both_grammars() {
    let textmate = textmate_keyword_words(&textmate_grammar(&[]));
    let highlight = highlight_keyword_words(&highlight_grammar(&[]));
    let mut missing = Vec::new();
    for keyword in lexer_keywords() {
        if !textmate.contains(&keyword) {
            missing.push(format!(
                "`{keyword}` is not in {TEXTMATE_GRAMMAR}'s `keywords` repository"
            ));
        }
        if !highlight.contains(&keyword) {
            missing.push(format!(
                "`{keyword}` is not in {HIGHLIGHT_THEME}'s keyword/literal groups"
            ));
        }
    }
    assert!(
        missing.is_empty(),
        "a keyword in the lexer (lexing.rs `KEYWORDS`) is missing from a grammar — the three-place rule:\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_grammar_keyword_is_a_lexer_keyword_or_contextual() {
    let lexer = lexer_keywords();
    let contextual: BTreeSet<String> = CONTEXTUAL_WORDS
        .iter()
        .map(|(word, _)| word.to_string())
        .collect();
    let textmate = textmate_keyword_words(&textmate_grammar(&[]));
    let highlight = highlight_keyword_words(&highlight_grammar(&[]));
    for (file, words) in [(TEXTMATE_GRAMMAR, &textmate), (HIGHLIGHT_THEME, &highlight)] {
        let unknown: Vec<&str> = words
            .iter()
            .filter(|word| !lexer.contains(*word) && !contextual.contains(*word))
            .map(String::as_str)
            .collect();
        assert!(
            unknown.is_empty(),
            "{file} colours {unknown:?} as keywords, but the lexer (lexing.rs `KEYWORDS`) knows no such \
             keyword and they are not in this test's CONTEXTUAL_WORDS"
        );
    }
    // The allowance stays honest: each contextual word is still an identifier
    // to the lexer (promote one and this moves it into `KEYWORDS`'s check), and
    // each is still used by at least one grammar.
    for (word, role) in CONTEXTUAL_WORDS {
        let (tokens, errors) = tokenize(word);
        assert!(errors.is_empty(), "{word}: {errors:?}");
        assert_eq!(
            tokens.iter().map(|(token, _)| token).collect::<Vec<_>>(),
            vec![&Token::Ident(word)],
            "`{word}` ({role}) is listed as contextual but the lexer now classifies it — move it to the keyword check"
        );
        assert!(
            textmate.contains(*word) || highlight.contains(*word),
            "`{word}` ({role}) is allowed as contextual but no grammar colours it any more — drop it from CONTEXTUAL_WORDS"
        );
    }
}

// --- Primitive types ---------------------------------------------------------

#[test]
fn the_textmate_primitive_types_are_the_scalar_primitives() {
    let grammar = textmate_grammar(SCALAR_PRIMITIVE_NAMES);
    let lexer = lexer_keywords();
    // The `types` repository: one word-list rule (the primitives) and the
    // PascalCase shape rule — the lists checked here are its words.
    let coloured = grammar.words("types");
    assert!(
        !coloured.is_empty(),
        "{TEXTMATE_GRAMMAR}: the `types` repository lists no primitive names — did its shape change?"
    );
    // Expected: every lowercase scalar primitive that is not already a keyword
    // (`null` is), plus the built-in type words. `BigInt` is PascalCase and
    // rides the user-type rule instead — checked by probe below.
    let mut expected: BTreeSet<String> = SCALAR_PRIMITIVE_NAMES
        .iter()
        .filter(|name| name.starts_with(|character: char| character.is_ascii_lowercase()))
        .filter(|name| !lexer.contains(**name))
        .map(|name| name.to_string())
        .collect();
    expected.extend(set(BUILT_IN_TYPE_WORDS));
    assert_eq!(
        sorted(&coloured),
        sorted(&expected),
        "{TEXTMATE_GRAMMAR}'s primitive-type list must be type_.rs's SCALAR_PRIMITIVE_NAMES (plus \
         {BUILT_IN_TYPE_WORDS:?}): left is the grammar, right is the compiler — `i64`/`u64` was the live drift"
    );
    // And every scalar primitive is coloured by SOME rule — the `types`
    // repository or, for `null`, the keyword literals.
    for name in SCALAR_PRIMITIVE_NAMES {
        assert!(
            grammar.matched("types", name) || grammar.matched("keywords", name),
            "{TEXTMATE_GRAMMAR}: no `types` or `keywords` rule matches the primitive `{name}`"
        );
    }
}

#[test]
fn the_highlight_number_suffixes_are_the_analyzers() {
    let accepted = set(NUMERIC_SUFFIXES);
    let rejected = set(REJECTED_SUFFIX_PROBES);
    assert!(
        accepted.is_disjoint(&rejected),
        "REJECTED_SUFFIX_PROBES overlaps NUMERIC_SUFFIXES: update the probes"
    );
    // Each suffix on a decimal literal. (Not on a hex one: `0x1f` is hex
    // digits to the regex before it is a suffix, so the probe would not be
    // asking about the list.)
    let probes: Vec<String> = accepted
        .iter()
        .chain(rejected.iter())
        .map(|suffix| format!("1{suffix}"))
        .collect();
    let probe_refs: Vec<&str> = probes.iter().map(String::as_str).collect();
    let grammar = highlight_grammar(&probe_refs);
    assert!(
        !grammar.rules("number").is_empty(),
        "{HIGHLIGHT_THEME}: no `number` rule — did its shape change?"
    );
    let mut wrong = Vec::new();
    for probe in &probes {
        let suffix = &probe[1..];
        let is_number = grammar.matched("number", probe);
        if is_number != accepted.contains(suffix) {
            wrong.push(format!(
                "`{probe}` is {} by the theme's number rule but `{suffix}` is {} to the analyzer",
                if is_number { "accepted" } else { "rejected" },
                if accepted.contains(suffix) {
                    "a valid suffix"
                } else {
                    "unknown"
                }
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "{HIGHLIGHT_THEME}'s number-suffix list must be type_.rs's NUMERIC_SUFFIXES:\n{}",
        wrong.join("\n")
    );
}

// --- Attribute markers -------------------------------------------------------

#[test]
fn the_attribute_markers_are_in_both_grammars() {
    let markers = set(KNOWN_ATTRIBUTE_MARKERS);
    let textmate = textmate_grammar(&[]);
    let highlight = highlight_grammar(&[]);
    // Every regex in the TextMate `attributes` repository that names any marker
    // is a marker list, and must name all of them and nothing else — the
    // opening lookahead and the inner `keyword.other.attribute` rule both.
    // (`(method|get|set)` names none, so it is not held to the list.)
    let mut lists = 0;
    for rule in textmate.rules("attributes") {
        let words: BTreeSet<String> = literal_words(&rule.regex).into_iter().collect();
        if words.is_disjoint(&markers) {
            continue;
        }
        lists += 1;
        assert_eq!(
            sorted(&words),
            sorted(&markers),
            "{TEXTMATE_GRAMMAR}: the `attributes` repository's `{}` {} lists markers that are not \
             parsing.rs's KNOWN_ATTRIBUTE_MARKERS (left is the grammar, right is the parser)",
            rule.field,
            rule.regex
        );
    }
    assert!(
        lists >= 2,
        "{TEXTMATE_GRAMMAR}: expected the `attributes` repository to list the markers in its opening \
         lookahead and its inner rule; found {lists} list(s) — did its shape change?"
    );
    // `vilan.js`: the `meta` rule's opening regex is its marker list.
    let words = highlight.words("meta");
    assert!(
        !words.is_empty(),
        "{HIGHLIGHT_THEME}: no `meta` (attribute) rule — did its shape change?"
    );
    assert_eq!(
        sorted(&words),
        sorted(&markers),
        "{HIGHLIGHT_THEME}'s attribute-marker list must be parsing.rs's KNOWN_ATTRIBUTE_MARKERS \
         (left is the theme, right is the parser)"
    );
}

// --- The extraction itself ---------------------------------------------------

#[test]
fn literal_words_reads_word_lists_and_ignores_shapes() {
    assert_eq!(
        literal_words(r"\b(if|else|match)\b"),
        ["if", "else", "match"]
    );
    assert_eq!(
        literal_words(r"^\s*\[(?:derive|must_use)\b"),
        ["derive", "must_use"]
    );
    assert_eq!(literal_words(r"(?<=\)\s{0,8})context\b"), ["context"]);
    assert_eq!(literal_words(r"(?<=\()(sync)\b"), ["sync"]);
    assert_eq!(
        literal_words(r"\b[A-Z][A-Za-z0-9_]*\b"),
        Vec::<String>::new()
    );
    // A digit-led run is not a word (`0x`, the `8|16|32|53` widths), and a
    // bracket class hides its letters even when it holds an escape.
    assert_eq!(
        literal_words(r"\b0x[0-9a-fA-F]+(?:[iu](?:8|16|32|53)|f32|f64|[fn])?"),
        ["f32", "f64"]
    );
    assert_eq!(literal_words(r"[\(\]]\s*(\[)"), Vec::<String>::new());
}
