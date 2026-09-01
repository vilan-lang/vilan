//! The three-place rule, generated and gated (AGENTS.md "A new keyword lands
//! in THREE places"; backlog D17, then E91): a keyword lands in the lexer, the
//! TextMate grammar (`editors/vscode/syntaxes/vilan.tmLanguage.json`) and the
//! book's highlight.js theme (`vilan/docs/theme/vilan.js`) — and the same drift
//! reaches the primitive-type, attribute-marker, numeric-suffix and operator
//! lists that sit beside the keywords in both grammars. `resource` shipped in
//! the lexer alone and was caught twice by eye; the D15 docs audit then found
//! `i64`/`u64` still coloured as types a release after they became a hard
//! error, and `platform` missing from both attribute lists. This file answers
//! in two layers:
//!
//! - GENERATION (E91): the word-list halves of both grammars — the token
//!   tables — are emitted from the compiler's exported tables
//!   (`lexing::KEYWORDS`, `lexing::TWO_CHARACTER_OPERATORS`,
//!   `type_::SCALAR_PRIMITIVE_NAMES`, `type_::NUMERIC_SUFFIXES`,
//!   `parsing::KNOWN_ATTRIBUTE_MARKERS`) and byte-held in place on every suite
//!   run by `generated_fragments_are_current`, which REWRITES them under
//!   `VILAN_REGENERATE_GRAMMARS=1` (every red names the command). Only the
//!   STRUCTURAL rules — string shapes, element tags, the contextual
//!   `context`/`sync` anchors, capture layouts — stay hand-written.
//! - GATING: the compiler's lists are read programmatically — `lexing::KEYWORDS`,
//!   `type_::SCALAR_PRIMITIVE_NAMES`, `type_::NUMERIC_SUFFIXES`,
//!   `parsing::KNOWN_ATTRIBUTE_MARKERS` are each the table their own consumer
//!   reads — so nothing here is a second copy to drift. The grammars are read
//!   with node, the way `vscode_extension.rs` reads the extension manifest: the
//!   TextMate file as JSON, and `vilan.js` *evaluated* under a stub `hljs`, so
//!   the word lists checked are the ones the real highlighter registers (the
//!   keyword string is built by concatenation; a text scrape would read it
//!   wrong). The two layers cross-check: the byte gate proves the files carry
//!   the generator's output, the evaluation gate proves what each grammar
//!   REGISTERS is the compiler's list — a splice landing in the wrong rule
//!   greens one and reds the other.
//!
//! Each axis is checked in both directions: everything the compiler knows is in
//! both grammars, and nothing in either grammar is unknown to the compiler —
//! the `i64` direction — with the contextual words the grammars colour by
//! position (`context`, `sync`, …) allowed explicitly and pinned to still lex
//! as identifiers.
//!
//! Since `vilan.js` is already evaluated here, this file also pins its
//! fence-tag shim: the harness tag (`browser`/`fragment`/`norun`) must survive
//! into `data-vilan-tag` under both class forms mdBook renders (K14 — mdBook
//! 0.5 splits ```` ```vilan,fragment ```` into `language-vilan fragment`,
//! which the shim once read as bare `vilan`, putting run controls on
//! fragments).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use vilan_core::lexing::{KEYWORDS, TWO_CHARACTER_OPERATORS, tokenize};
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
    // The `types` repository: one word-list rule (the primitives) and the
    // PascalCase shape rule — the lists checked here are its words.
    let coloured = grammar.words("types");
    assert!(
        !coloured.is_empty(),
        "{TEXTMATE_GRAMMAR}: the `types` repository lists no primitive names — did its shape change?"
    );
    // Expected: the generator's own derivation (`BigInt` is PascalCase and
    // rides the user-type rule instead — checked by probe below).
    let expected: BTreeSet<String> = set(&textmate_primitive_words());
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

// --- The fence-tag shim ------------------------------------------------------

/// The class forms mdBook renders for a tagged fence, and the `data-vilan-tag`
/// the shim in `vilan.js`'s first IIFE must stash before it normalizes the
/// class away. mdBook 0.4 kept the fence's info string as one class token
/// (`language-vilan,fragment`); the 0.5 line `docs.yml` pins splits it on the
/// comma into separate classes (`language-vilan fragment`), which the shim
/// used to capture as bare `vilan` — the K14 find: the lost `fragment` put run
/// controls on three fences whose run could only fail. The tag vocabulary is
/// docs.rs's (`browser`/`fragment`/`norun`), enumerated so a neighbouring
/// class the harness never wrote (`hljs`, appended by mdBook's own first
/// highlight pass) is not read as a tag.
const FENCE_CLASS_FIXTURES: &[(&str, &str)] = &[
    ("language-vilan", "vilan"),
    // mdBook 0.5: the info string's comma becomes a space.
    ("language-vilan fragment", "vilan,fragment"),
    ("language-vilan browser", "vilan,browser"),
    ("language-vilan norun", "vilan,norun"),
    ("language-vilan browser norun", "vilan,browser,norun"),
    // mdBook's bundled highlighter already ran: `hljs` sits beside the fence
    // classes, before or after, and is no tag.
    ("language-vilan fragment hljs", "vilan,fragment"),
    ("hljs language-vilan fragment", "vilan,fragment"),
    // mdBook 0.4: the whole info string is one class token.
    ("language-vilan,fragment", "vilan,fragment"),
    ("language-vilan,browser", "vilan,browser"),
    ("language-vilan,norun", "vilan,norun"),
];

#[test]
fn the_fence_tag_shim_reads_both_mdbook_class_forms() {
    // The file is evaluated the way `highlight_grammar` evaluates it, but the
    // stub `document` hands the shim's selector one fake block per fixture
    // class; the script then reports what the shim left on each block. The
    // fixtures travel as ONE probe joined on `;` — a fixture class may itself
    // contain a comma, the helper's separator.
    const SCRIPT: &str = r#"
        const fs = require("fs");
        const vm = require("vm");
        const blocks = process.env.VILAN_PROBES.split(";").filter(Boolean).map((className) => ({
            className,
            dataset: {},
            original: className,
        }));
        globalThis.hljs = {
            registerLanguage() {},
            highlightElement() {},
        };
        globalThis.document = {
            querySelectorAll(selector) {
                return selector.includes("language-vilan") ? blocks : [];
            },
        };
        globalThis.window = {};
        const file = process.env.VILAN_FILE;
        vm.runInThisContext(fs.readFileSync(file, "utf8"), { filename: file });
        for (const block of blocks) {
            console.log(["tag", block.original, block.dataset.vilanTag || "", block.className].join("\t"));
        }
    "#;
    let fixture_list = FENCE_CLASS_FIXTURES
        .iter()
        .map(|(class, _)| *class)
        .collect::<Vec<_>>()
        .join(";");
    let lines = node(SCRIPT, &repo_root().join(HIGHLIGHT_THEME), &[&fixture_list]);
    let mut reported = Vec::new();
    for line in &lines {
        let fields: Vec<&str> = line.splitn(4, '\t').collect();
        match fields.as_slice() {
            ["tag", class, tag, normalized] => reported.push((*class, *tag, *normalized)),
            _ => panic!("unexpected line from node: {line:?}"),
        }
    }
    for (class, expected_tag) in FENCE_CLASS_FIXTURES {
        let (_, tag, normalized) = reported
            .iter()
            .find(|(reported_class, _, _)| reported_class == class)
            .unwrap_or_else(|| panic!("the shim reported nothing for class {class:?}"));
        assert_eq!(
            tag, expected_tag,
            "{HIGHLIGHT_THEME}: a block with class {class:?} must carry data-vilan-tag \
             {expected_tag:?} after the shim (the harness tag survives mdBook's class form)"
        );
        assert_eq!(
            *normalized, "language-vilan",
            "{HIGHLIGHT_THEME}: the shim must normalize class {class:?} to `language-vilan` \
             for the highlighter"
        );
    }
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

// --- Generation: the token-table halves (E91) --------------------------------
//
// The strategy (backlog E91, ratified 2026-08-25): the compiler stays the ONE
// grammar truth, delivered as semantic tokens wherever LSP runs; every static
// grammar is deliberately SKELETON-grade; and the static grammars' token
// tables are not hand-copies of the compiler's lists but generator OUTPUT —
// spliced into each file at fixed seams and byte-held there by
// `generated_fragments_are_current`, so a compiler-table change without
// regeneration is a red suite, not a reader's eye.
//
// The seams: in the TextMate grammar each generated `match`/`begin` value sits
// directly under a uniquely named rule (the anchor is its `"name":` line; the
// file's `information_for_contributors` header carries the command); in
// `vilan.js` each generated run of lines sits between a
// `// GENERATED(<name>)` and a `// END GENERATED(<name>)` marker, the begin
// marker carrying the command. Everything outside the seams is hand-written
// structure.
//
// --- The tree-sitter seam (E62) ----------------------------------------------
// When the Zed lane opens (backlog E62 — deferred until the syntax settles,
// at/after the beta switch, ideally once I2 const generics and B3 keyof land
// or park), the tree-sitter grammar hangs HERE: its `grammar.js` word tables
// and its query files (`highlights.scm` et al.) become one more consumer of
// these same generated fragments, under this same gate — the grammar is born
// gated. Notes from the strategy record: VS Code has NO tree-sitter support
// and none announced (TextMate + semantic tokens indefinitely); Zed is
// tree-sitter-ONLY for highlighting (weak semantic-token support), so the
// grammar carries real weight there; GitHub consumes tree-sitter for code
// navigation and TextMate (linguist) for highlighting — both grammars have a
// second customer. Until the E62 lane opens its paper, backlog E91's entry
// doubles as the strategy record.

/// Set (to anything) to make [`generated_fragments_are_current`] REWRITE the
/// stale fragments in place instead of failing — the regeneration entry point.
const REGENERATE_ENV: &str = "VILAN_REGENERATE_GRAMMARS";

/// The regeneration command, verbatim — named by every red the byte gate
/// raises, carried in each generated seam's marker, and pinned present in the
/// TextMate grammar's `information_for_contributors` header.
const REGENERATE_COMMAND: &str =
    "VILAN_REGENERATE_GRAMMARS=1 cargo test -p vilan-cli --test grammar_sync generated";

/// The grammar-side role of a lexer keyword — which word list (TextMate scope,
/// highlight.js group) it lands in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KeywordRole {
    /// Flow — `keyword.control.vilan`; the highlight.js `keyword` group.
    Control,
    /// `keyword.control.import.vilan`; `keyword`.
    Import,
    /// Introduces a named item — `storage.type.vilan`; `keyword`.
    Declaration,
    /// `storage.modifier.vilan`; `keyword`.
    Modifier,
    /// `with` and the `borrows` clause — `keyword.other.vilan`; `keyword`.
    Other,
    /// `constant.language.vilan`; the highlight.js `literal` group.
    Literal,
}

/// Each lexer keyword's role. MEMBERSHIP is `lexing::KEYWORDS`' alone —
/// [`generated_keyword_roles_cover_the_lexer`] holds this table to it in both
/// directions, so a keyword added to the lexer is red HERE until it is given a
/// role (and the grammars regenerated). The role split itself is presentation,
/// hand-assigned like the structural rules; rows are grouped by role in the
/// order each alternation spells them.
const KEYWORD_ROLES: &[(&str, KeywordRole)] = &[
    ("if", KeywordRole::Control),
    ("else", KeywordRole::Control),
    ("match", KeywordRole::Control),
    ("for", KeywordRole::Control),
    ("in", KeywordRole::Control),
    ("is", KeywordRole::Control),
    ("jump", KeywordRole::Control),
    ("ret", KeywordRole::Control),
    ("await", KeywordRole::Control),
    ("import", KeywordRole::Import),
    ("use", KeywordRole::Import),
    ("fun", KeywordRole::Declaration),
    ("struct", KeywordRole::Declaration),
    ("enum", KeywordRole::Declaration),
    ("impl", KeywordRole::Declaration),
    ("trait", KeywordRole::Declaration),
    ("type", KeywordRole::Declaration),
    ("mod", KeywordRole::Declaration),
    ("macro", KeywordRole::Declaration),
    ("let", KeywordRole::Modifier),
    ("mut", KeywordRole::Modifier),
    ("own", KeywordRole::Modifier),
    ("external", KeywordRole::Modifier),
    ("export", KeywordRole::Modifier),
    ("async", KeywordRole::Modifier),
    ("const", KeywordRole::Modifier),
    ("resource", KeywordRole::Modifier),
    ("with", KeywordRole::Other),
    ("borrows", KeywordRole::Other),
    // `css` heads an expression rather than declaring or modifying an item, so
    // it takes the general bucket beside `with`/`borrows` rather than
    // `storage.type` (which colours the word that names a new item).
    ("css", KeywordRole::Other),
    ("true", KeywordRole::Literal),
    ("false", KeywordRole::Literal),
    ("null", KeywordRole::Literal),
];

/// Words the highlight.js `literal` group carries beyond the lexer's literal
/// keywords. Each must be in [`CONTEXTUAL_WORDS`] (an identifier to the lexer)
/// — [`generated_keyword_roles_cover_the_lexer`] pins that.
const HIGHLIGHT_LITERAL_EXTRAS: &[(&str, &str)] = &[
    ("void", "the unit type — an identifier to the lexer"),
    ("self", "the receiver parameter"),
    ("Self", "the implementing type inside an `impl`"),
];

/// Sequences the TextMate operator rule colours as one operator although the
/// lexer never fuses them — presentation over token truth, deliberately.
const TEXTMATE_OPERATOR_EXTRAS: &[(&str, &str)] = &[
    (
        "&mut",
        "`&` + the `mut` keyword — the borrow pair reads as one operator",
    ),
    (
        "<=",
        "`<` is a control byte; the parser reassembles the comparison",
    ),
    (">=", "`>` likewise"),
    ("?.", "`?` + the control byte `.` — optional chaining"),
];

/// The single-character tail of the operator rule: the lexer's operator
/// charset (`-:!*/+=|&^?%`, `is_operator_byte`) minus `:` (alone it is
/// punctuation; only `::` colours) plus `<`/`>` (control bytes to the lexer,
/// comparisons to the eye).
const TEXTMATE_SINGLE_CHARACTER_OPERATORS: &str = "[-+*/=<>!&|^?%]";

/// The keywords of `role`, in [`KEYWORD_ROLES`] order.
fn role_words(role: KeywordRole) -> Vec<&'static str> {
    KEYWORD_ROLES
        .iter()
        .filter(|(_, keyword_role)| *keyword_role == role)
        .map(|(word, _)| *word)
        .collect()
}

/// `text` with every regex metacharacter escaped, so it matches literally
/// inside an alternation (Oniguruma and JS alike).
fn escape_regex_literal(text: &str) -> String {
    let mut escaped = String::new();
    for character in text.chars() {
        if "\\|(){}[]^$.*+?".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// `value` as a double-quoted JSON/JS string literal. The fragments are ASCII
/// regexes; anything else would raise encoding questions the splice cannot
/// answer, so it refuses.
fn double_quoted(value: &str) -> String {
    assert!(
        value.is_ascii() && !value.bytes().any(|byte| byte < 0x20),
        "a generated fragment must be printable ASCII: {value:?}"
    );
    let mut literal = String::from("\"");
    for character in value.chars() {
        if character == '"' || character == '\\' {
            literal.push('\\');
        }
        literal.push(character);
    }
    literal.push('"');
    literal
}

/// A `\b(word|word|…)\b` word-list regex.
fn word_list_regex(words: &[&str]) -> String {
    format!(r"\b({})\b", words.join("|"))
}

/// The TextMate primitive-type words: every lowercase scalar primitive that is
/// not already a keyword (`null` is), plus the built-in type words. `BigInt`
/// is PascalCase and rides the user-type shape rule instead.
fn textmate_primitive_words() -> Vec<&'static str> {
    let lexer = lexer_keywords();
    SCALAR_PRIMITIVE_NAMES
        .iter()
        .filter(|name| name.starts_with(|character: char| character.is_ascii_lowercase()))
        .filter(|name| !lexer.contains(**name))
        .chain(BUILT_IN_TYPE_WORDS)
        .copied()
        .collect()
}

/// The numeric-suffix alternation, longest suffix first so highlighting (which
/// takes the first alternative, no anchor to force backtracking) never colours
/// `1f32` as `1f` + digits.
fn number_suffix_group() -> String {
    let mut suffixes = NUMERIC_SUFFIXES.to_vec();
    suffixes.sort_by_key(|suffix| (std::cmp::Reverse(suffix.len()), *suffix));
    format!("(?:{})", suffixes.join("|"))
}

/// The attribute-marker alternation, in `KNOWN_ATTRIBUTE_MARKERS` order.
fn marker_alternation() -> String {
    KNOWN_ATTRIBUTE_MARKERS.join("|")
}

/// The TextMate operator regex: the lexer's fused pairs, then the
/// presentation-only extras, then the single-character class — multi-character
/// alternatives strictly before the class, or the class would win at their
/// first byte.
fn textmate_operator_regex() -> String {
    let alternatives: Vec<String> = TWO_CHARACTER_OPERATORS
        .iter()
        .copied()
        .chain(
            TEXTMATE_OPERATOR_EXTRAS
                .iter()
                .map(|(sequence, _)| *sequence),
        )
        .map(escape_regex_literal)
        .collect();
    format!(
        "{}|{}",
        alternatives.join("|"),
        TEXTMATE_SINGLE_CHARACTER_OPERATORS
    )
}

/// One generated TextMate value: the line directly under the unique line whose
/// trimmed text equals `anchor` must read `"<field>": <value-as-JSON-string>`.
struct TextmateFragment {
    anchor: &'static str,
    field: &'static str,
    value: String,
}

fn textmate_fragments() -> Vec<TextmateFragment> {
    let keyword_rule = |anchor, role| TextmateFragment {
        anchor,
        field: "match",
        value: word_list_regex(&role_words(role)),
    };
    let markers = marker_alternation();
    vec![
        keyword_rule(r#""name": "keyword.control.vilan","#, KeywordRole::Control),
        keyword_rule(
            r#""name": "keyword.control.import.vilan","#,
            KeywordRole::Import,
        ),
        keyword_rule(r#""name": "storage.type.vilan","#, KeywordRole::Declaration),
        keyword_rule(
            r#""name": "storage.modifier.vilan","#,
            KeywordRole::Modifier,
        ),
        keyword_rule(r#""name": "keyword.other.vilan","#, KeywordRole::Other),
        keyword_rule(
            r#""name": "constant.language.vilan","#,
            KeywordRole::Literal,
        ),
        TextmateFragment {
            anchor: r#""name": "support.type.primitive.vilan","#,
            field: "match",
            value: word_list_regex(&textmate_primitive_words()),
        },
        TextmateFragment {
            anchor: r#""name": "meta.attribute.vilan","#,
            field: "begin",
            value: format!(
                r"(\[)(?=\s*({markers})\b)|^\s*(\[)(?=\s*[A-Za-z_][A-Za-z0-9_]*\s*[\(\]])"
            ),
        },
        TextmateFragment {
            anchor: r#""name": "keyword.other.attribute.vilan","#,
            field: "match",
            value: format!(r"\b({markers})\b"),
        },
        TextmateFragment {
            anchor: r#""name": "keyword.operator.vilan","#,
            field: "match",
            value: textmate_operator_regex(),
        },
    ]
}

/// The generated regions of `vilan.js`: `(name, lines)`, each replacing
/// whatever currently sits from its `// GENERATED(name)` line through its
/// `// END GENERATED(name)` line, markers included.
fn highlight_regions() -> Vec<(&'static str, Vec<String>)> {
    let literal_group: Vec<&str> = role_words(KeywordRole::Literal)
        .into_iter()
        .chain(HIGHLIGHT_LITERAL_EXTRAS.iter().map(|(word, _)| *word))
        .collect();
    let keyword_group: Vec<String> = lexer_keywords()
        .into_iter()
        .filter(|keyword| !literal_group.contains(&keyword.as_str()))
        .collect();
    let suffix = number_suffix_group();
    let markers = marker_alternation();
    vec![
        (
            "keyword-groups",
            vec![
                format!(
                    "\t\t\t// GENERATED(keyword-groups): lexing.rs KEYWORDS split by grammar_sync.rs's KEYWORD_ROLES — regenerate: {REGENERATE_COMMAND}"
                ),
                format!(
                    "\t\t\tkeyword: {},",
                    double_quoted(&keyword_group.join(" "))
                ),
                format!(
                    "\t\t\tliteral: {},",
                    double_quoted(&literal_group.join(" "))
                ),
                "\t\t\t// END GENERATED(keyword-groups)".to_string(),
            ],
        ),
        (
            "number-suffixes",
            vec![
                format!(
                    "\t\t\t\t// GENERATED(number-suffixes): type_.rs NUMERIC_SUFFIXES — regenerate: {REGENERATE_COMMAND}"
                ),
                format!(
                    "\t\t\t\t{{ begin: {} }},",
                    double_quoted(&format!(r"\b0x[0-9a-fA-F]+{suffix}?"))
                ),
                format!(
                    "\t\t\t\t{{ begin: {} }},",
                    double_quoted(&format!(r"\b\d+(?:\.\d+)?{suffix}?"))
                ),
                "\t\t\t\t// END GENERATED(number-suffixes)".to_string(),
            ],
        ),
        (
            "attribute-markers",
            vec![
                format!(
                    "\t\t\t// GENERATED(attribute-markers): parsing.rs KNOWN_ATTRIBUTE_MARKERS — regenerate: {REGENERATE_COMMAND}"
                ),
                format!(
                    "\t\t\tbegin: {},",
                    double_quoted(&format!(r"^\s*\[(?:{markers})\b"))
                ),
                "\t\t\t// END GENERATED(attribute-markers)".to_string(),
            ],
        ),
    ]
}

/// `lines` with every TextMate fragment spliced in: for each fragment, the
/// unique anchor line is found and the `"field":` line under it is rewritten
/// to the generated value, preserving indentation and the trailing comma.
fn spliced_textmate(lines: &[String]) -> Vec<String> {
    let mut spliced = lines.to_vec();
    for fragment in textmate_fragments() {
        let key = format!("\"{}\":", fragment.field);
        let positions: Vec<usize> = spliced
            .iter()
            .enumerate()
            .filter(|(index, line)| {
                line.trim() == fragment.anchor
                    && spliced
                        .get(index + 1)
                        .is_some_and(|next| next.trim_start().starts_with(&key))
            })
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            positions.len(),
            1,
            "{TEXTMATE_GRAMMAR}: expected exactly one anchor {:?} followed by a {key} line \
             (found {}) — did the grammar's shape change?",
            fragment.anchor,
            positions.len()
        );
        let value_line = &mut spliced[positions[0] + 1];
        let indent: String = value_line
            .chars()
            .take_while(|character| character.is_whitespace())
            .collect();
        let comma = if value_line.trim_end().ends_with(',') {
            ","
        } else {
            ""
        };
        *value_line = format!("{indent}{key} {}{comma}", double_quoted(&fragment.value));
    }
    spliced
}

/// `lines` with every `vilan.js` generated region spliced in, located by its
/// begin/end markers.
fn spliced_highlight(lines: &[String]) -> Vec<String> {
    let mut spliced = lines.to_vec();
    for (name, region) in highlight_regions() {
        let begin_marker = format!("// GENERATED({name})");
        let end_marker = format!("// END GENERATED({name})");
        let find_unique = |lines: &[String], marker: &str, exclude: Option<&str>| {
            let positions: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter(|(_, line)| {
                    line.contains(marker) && exclude.is_none_or(|excluded| !line.contains(excluded))
                })
                .map(|(index, _)| index)
                .collect();
            assert_eq!(
                positions.len(),
                1,
                "{HIGHLIGHT_THEME}: expected exactly one {marker:?} line, found {}",
                positions.len()
            );
            positions[0]
        };
        let begin = find_unique(&spliced, &begin_marker, Some(&end_marker));
        let end = find_unique(&spliced, &end_marker, None);
        assert!(
            begin < end,
            "{HIGHLIGHT_THEME}: the {name} markers are out of order"
        );
        spliced.splice(begin..=end, region);
    }
    spliced
}

/// The first line where `current` and `desired` disagree, for the red.
fn first_difference(current: &[String], desired: &[String]) -> String {
    for (index, (current_line, desired_line)) in current.iter().zip(desired).enumerate() {
        if current_line != desired_line {
            return format!(
                "line {}:\n  in the file: {current_line}\n  generated:   {desired_line}",
                index + 1
            );
        }
    }
    format!(
        "the files differ in length ({} lines vs {} generated)",
        current.len(),
        desired.len()
    )
}

/// The role table and the extras stay honest against the compiler's tables:
/// every lexer keyword has exactly one role and every role row is a lexer
/// keyword — so a keyword added to `lexing::KEYWORDS` is red here until it is
/// assigned one — and no extra shadows a real token.
#[test]
fn generated_keyword_roles_cover_the_lexer() {
    let lexer = lexer_keywords();
    let mut seen = BTreeSet::new();
    for (word, _) in KEYWORD_ROLES {
        assert!(
            seen.insert(*word),
            "`{word}` is listed twice in KEYWORD_ROLES"
        );
        assert!(
            lexer.contains(*word),
            "`{word}` has a role in KEYWORD_ROLES but is not a lexer keyword (lexing.rs KEYWORDS)"
        );
    }
    for keyword in &lexer {
        assert!(
            seen.contains(keyword.as_str()),
            "the lexer keyword `{keyword}` has no role in KEYWORD_ROLES — assign one, then \
             regenerate the grammars: {REGENERATE_COMMAND}"
        );
    }
    // The extras: literal extras are contextual words (identifiers to the
    // lexer, already pinned as such), operator extras are sequences the lexer
    // really does not fuse.
    for (word, role) in HIGHLIGHT_LITERAL_EXTRAS {
        assert!(
            CONTEXTUAL_WORDS
                .iter()
                .any(|(contextual, _)| contextual == word),
            "`{word}` ({role}) is a HIGHLIGHT_LITERAL_EXTRAS row but not a CONTEXTUAL_WORDS one"
        );
    }
    for (sequence, role) in TEXTMATE_OPERATOR_EXTRAS {
        assert!(
            !TWO_CHARACTER_OPERATORS.contains(sequence),
            "`{sequence}` ({role}) is listed as a presentation-only extra but the lexer fuses \
             it (lexing.rs TWO_CHARACTER_OPERATORS) — drop the extra and regenerate"
        );
    }
}

/// The byte gate, and the regeneration entry point: every generated fragment
/// in both grammars is exactly the generator's output. Stale is red — the
/// message names the command — and under `VILAN_REGENERATE_GRAMMARS=1` stale
/// is rewritten in place instead (then re-verified, and checked idempotent).
#[test]
fn generated_fragments_are_current() {
    let regenerate = std::env::var_os(REGENERATE_ENV).is_some();
    let splices: [(&str, fn(&[String]) -> Vec<String>); 2] = [
        (TEXTMATE_GRAMMAR, spliced_textmate),
        (HIGHLIGHT_THEME, spliced_highlight),
    ];
    let mut stale = Vec::new();
    for (path, splice) in splices {
        let absolute = repo_root().join(path);
        let current: Vec<String> = std::fs::read_to_string(&absolute)
            .unwrap_or_else(|error| panic!("reading {path}: {error}"))
            .split('\n')
            .map(str::to_string)
            .collect();
        let desired = splice(&current);
        assert_eq!(
            splice(&desired),
            desired,
            "{path}: regeneration is not idempotent — a generated line matches a seam locator"
        );
        if current == desired {
            continue;
        }
        if regenerate {
            std::fs::write(&absolute, desired.join("\n"))
                .unwrap_or_else(|error| panic!("writing {path}: {error}"));
            eprintln!("regenerated {path}");
        } else {
            stale.push(format!("{path}: {}", first_difference(&current, &desired)));
        }
    }
    assert!(
        stale.is_empty(),
        "a grammar's generated fragments are not the compiler's tables — a table moved without \
         regeneration, or a fragment was hand-edited. Regenerate: `{REGENERATE_COMMAND}`\n{}",
        stale.join("\n")
    );
    // The pointer a human editor sees stays present: the TextMate grammar has
    // no comment syntax, so its `information_for_contributors` header is where
    // the command lives (the vilan.js seams carry it in their begin markers,
    // which the splice itself emits).
    let textmate = std::fs::read_to_string(repo_root().join(TEXTMATE_GRAMMAR)).unwrap();
    assert!(
        textmate.contains(REGENERATE_COMMAND),
        "{TEXTMATE_GRAMMAR}: the `information_for_contributors` header no longer names the \
         regeneration command ({REGENERATE_COMMAND})"
    );
}
