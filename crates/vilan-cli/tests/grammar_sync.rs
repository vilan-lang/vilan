//! The three-place rule, generated and gated (AGENTS.md "A new keyword lands
//! in THREE places"; backlog D17, then E91): a keyword lands in the lexer, the
//! TextMate grammar (`editors/vscode/syntaxes/vilan.tmLanguage.json`) and the
//! book's highlight.js theme (`vilan/docs/theme/vilan.js`) — and the same drift
//! reaches the primitive-type, attribute-marker, numeric-suffix and operator
//! lists that sit beside the keywords in both grammars. `resource` shipped in
//! the lexer alone and was caught twice by eye; the D15 docs audit then found
//! `i64`/`u64` still coloured as types a release after they became a hard
//! error, and `platform` missing from both attribute lists. This file answers
//! in three layers:
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
//! - SCOPES (E163): the structural rules are held to their OUTCOME rather than
//!   to their own text — `vscode-textmate` over `vscode-oniguruma`, the
//!   tokeniser and regex engine VS Code itself loads, run over a program, and
//!   the scope each character ends up with asserted. The E161/E162 pins were
//!   regex-and-order pins until this layer existed, and the two defects E164
//!   closed were invisible to them. See the E163 section for the dependency,
//!   its cost, and why a scope pin may skip on a working copy but never in CI.
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
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use vilan_core::lexing::{KEYWORDS, TWO_CHARACTER_OPERATORS, tokenize};
use vilan_core::parsing::KNOWN_ATTRIBUTE_MARKERS;
use vilan_core::token::Token;
use vilan_core::type_::{NUMERIC_SUFFIXES, SCALAR_PRIMITIVE_NAMES};

const TEXTMATE_GRAMMAR: &str = "editors/vscode/syntaxes/vilan.tmLanguage.json";
const HIGHLIGHT_THEME: &str = "vilan/docs/theme/vilan.js";

/// Words a grammar may colour as keywords although the lexer hands them back
/// as identifiers. Each is CONTEXTUAL — a keyword in one position and a plain
/// name anywhere else — and the grammars match it by position (the TextMate
/// grammar and `vilan.js` both anchor `context` after what a clause follows —
/// a closure type's `)`, a parameter list's `)`, or a declaration's RETURN type
/// — and `sync` after the `(` that opens a closure type). Pinned to lex as
/// `Token::Ident`: the day one is promoted to a real keyword (a `KEYWORDS`
/// row), this list must shrink by it.
const CONTEXTUAL_WORDS: &[(&str, &str)] = &[
    (
        "context",
        "the clause on a closure type or a declaration: `(|| void) context owner`, `fun f(): i32 context settings`",
    ),
    (
        "sync",
        "the marker opening a closure type: `(sync || View)`",
    ),
    (
        "as",
        "the alias in an import path: `import a::b as c` (E142)",
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

/// Whether each of `texts` matches `regex`, evaluated in node — the grammars'
/// rules use lookbehind, which Rust's `regex` crate does not have, and the
/// point of the pin is to run the rule the way the editor and the book run it.
fn regex_matches(regex: &str, texts: &[&str]) -> Vec<bool> {
    const SCRIPT: &str = r#"
        const compiled = new RegExp(process.env.VILAN_REGEX);
        for (const text of process.env.VILAN_TEXTS.split("\u001f")) {
            console.log(compiled.test(text) ? "yes" : "no");
        }
    "#;
    let output = Command::new("node")
        .args(["-e", SCRIPT])
        .env("VILAN_REGEX", regex)
        .env("VILAN_TEXTS", texts.join("\u{1f}"))
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "evaluating {regex:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line == "yes")
        .collect()
}

/// Every place `regex` matches in `text`, as `(offset, matched text)` — the
/// global run of the same node `RegExp` [`regex_matches`] compiles. "Does it
/// match anywhere" is not the question a POSITIONAL guard raises: the book's
/// element-tag rule matches `<span>hello</span>` either way, and what E171 is
/// about is whether it matches the closing tag as well as the opening one.
fn regex_match_positions(regex: &str, text: &str) -> Vec<(usize, String)> {
    const SCRIPT: &str = r#"
        const compiled = new RegExp(process.env.VILAN_REGEX, "g");
        const text = process.env.VILAN_TEXTS;
        let found;
        while ((found = compiled.exec(text)) !== null) {
            console.log(found.index + "\t" + found[0]);
            // An empty match would spin here; the rule cannot produce one, and
            // a rule that starts to is a defect this loop should not hide.
            if (found.index === compiled.lastIndex) compiled.lastIndex += 1;
        }
    "#;
    let output = Command::new("node")
        .args(["-e", SCRIPT])
        .env("VILAN_REGEX", regex)
        .env("VILAN_TEXTS", text)
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "evaluating {regex:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| {
            let (at, matched) = line.split_once('\t').expect("offset and text");
            (at.parse().expect("a match offset"), matched.to_string())
        })
        .collect()
}

/// The book's element-tag rule: its ONE `name` rule. Addressed by className
/// rather than by a fragment of its own regex — E161's pin found it with
/// `regex.contains("</?")`, and E171 is exactly the change that stops the
/// closing form being spelled that way, so the finder would have gone missing
/// on the change it is meant to be watching.
fn book_element_tag_rule(grammar: &Grammar) -> &Rule {
    let rules = grammar.rules("name");
    assert_eq!(
        rules.len(),
        1,
        "{HIGHLIGHT_THEME}: one `name` (element-tag) rule expected, found {} — \
         did its shape change?",
        rules.len()
    );
    rules[0]
}

/// The rule of `grammar` under `key` whose regex spells exactly `word` — the
/// contextual rules are one word each, so this addresses one of them.
fn contextual_rule<'a>(grammar: &'a Grammar, key: &str, word: &str) -> &'a Rule {
    grammar
        .rules(key)
        .into_iter()
        .find(|rule| literal_words(&rule.regex) == [word])
        .unwrap_or_else(|| panic!("no `{word}`-only rule under `{key}`"))
}

/// `as` colours as a keyword in BOTH grammars, and ONLY where an alias can sit
/// (E145; E142 shipped the alias with `as` in neither grammar).
///
/// It is not a lexer keyword — `let as = 1;` parses, which is why it is in
/// [`CONTEXTUAL_WORDS`] — so each grammar guards it by position, and a guard
/// that merely EXISTS proves nothing: these run the two rules the way the
/// editor and the book do, over the shapes an alias takes and the shapes a
/// value named `as` takes.
#[test]
fn the_import_alias_as_is_coloured_by_position_in_both_grammars() {
    const ALIASES: &[&str] = &[
        "import a::b as c;",
        "import pkg::helper::greet as hello;",
        "use a::{ b as c };",
    ];
    const NOT_ALIASES: &[&str] = &["let as = 1;", "let x = as;", "as(1)", "value.as"];
    for (file, grammar, key) in [
        (TEXTMATE_GRAMMAR, textmate_grammar(&[]), "keywords"),
        (HIGHLIGHT_THEME, highlight_grammar(&[]), "keyword"),
    ] {
        let rule = contextual_rule(&grammar, key, "as");
        assert_eq!(
            regex_matches(&rule.regex, ALIASES),
            vec![true; ALIASES.len()],
            "{file}: {:?} misses an import alias among {ALIASES:?}",
            rule.regex,
        );
        assert_eq!(
            regex_matches(&rule.regex, NOT_ALIASES),
            vec![false; NOT_ALIASES.len()],
            "{file}: {:?} colours `as` where it is an ordinary name ({NOT_ALIASES:?})",
            rule.regex,
        );
    }
}

/// B343 (R9) — `context` colours as a keyword in BOTH grammars wherever a
/// CLAUSE can sit, and nowhere a value named `context` sits.
///
/// The clause's position is contexts.md §3's — after the return type — and R9
/// kept it there, so the grammars have to read it there: `fun f(): i32 context
/// settings` painted `context` as an ordinary identifier, because both rules
/// were anchored on the `)` of a closure type and nothing else. The guard is
/// two-sided now, which is also what keeps the reads THROUGH a context binding
/// (`context.run(..)`, `context.get()`) plain — the old one-sided rule painted
/// those wherever a `)` happened to precede them.
#[test]
fn the_context_clause_is_coloured_by_position_in_both_grammars() {
    const CLAUSES: &[&str] = &[
        "fun f(): i32 context settings",
        "fun f(): (|| void) context owner_scope",
        "fun f(): List<i32> context settings",
        "fun f(x: i32) context settings {",
        "fun f(): i32 context (a, b)",
        "let body: (|| View) context owner_scope = || view(\"div\");",
    ];
    const NOT_CLAUSES: &[&str] = &[
        "let context = 1;",
        "let x = context;",
        "context.run(1, || {})",
        "import std::context::Context;",
        "let value = read(x).context;",
    ];
    for (file, grammar, key) in [
        (TEXTMATE_GRAMMAR, textmate_grammar(&[]), "keywords"),
        (HIGHLIGHT_THEME, highlight_grammar(&[]), "keyword"),
    ] {
        let rule = contextual_rule(&grammar, key, "context");
        assert_eq!(
            regex_matches(&rule.regex, CLAUSES),
            vec![true; CLAUSES.len()],
            "{file}: {:?} misses a `context` clause among {CLAUSES:?}",
            rule.regex,
        );
        assert_eq!(
            regex_matches(&rule.regex, NOT_CLAUSES),
            vec![false; NOT_CLAUSES.len()],
            "{file}: {:?} colours `context` where it is an ordinary name ({NOT_CLAUSES:?})",
            rule.regex,
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
    // `lazy` modifies a parameter (`lazy message: str`) and a module binding
    // (`lazy let database: …`) — a storage modifier beside `const`/`mut`, not a
    // word that names a new item.
    ("lazy", KeywordRole::Modifier),
    // `dyn` modifies a TYPE (`dyn Source<i32>`) — it names no new item and
    // heads no statement, so it sits with the other type-position words rather
    // than in `storage.type`.
    ("dyn", KeywordRole::Other),
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

// --- E163: the grammar's own tokeniser ---------------------------------------
//
// Everything from here to the book's twin runs the TextMate grammar the way VS
// Code runs it — `vscode-textmate` over `vscode-oniguruma`, the same tokeniser
// and the same regex engine the editor loads — and asserts the SCOPE each
// character ends up with.
//
// It did not, until E163. The E161/E162 pins asserted each rule's own REGEX
// plus the two rule ORDERS that decide which rule gets to match, which together
// IMPLY an outcome without ever producing one, and the gap was not theoretical:
// both defects E164 closed (a closing tag straight after text painted as a
// generic argument list; the `>` of an attributed opening tag painted as an
// operator) were invisible to every regex pin in this file and obvious in the
// first line of tokeniser output.
//
// **The dependency, and what it costs.** `vscode-textmate` and
// `vscode-oniguruma` are `editors/vscode` devDependencies (+2 lockfile
// packages, 632 KB on disk, 0 advisories). Nothing ships them: the vsix bundles
// `dependencies` only and `.vscodeignore` drops `node_modules/` whole, so
// `editors/vscode/ThirdPartyNotices.txt` — which covers what `out/extension.js`
// BUNDLES — gains no entry, and the root notices gate reads `Cargo.lock` alone.
// `npm ci --prefix editors/vscode` is what puts them on disk (~0.8 s warm), and
// it is a step of ci.yml's `test` job and release.yml's `gate` job.
//
// **Why a scope pin may skip.** On a working copy the directory is optional,
// deliberately: a fresh clone builds and runs the suite with no node packages
// at all, and a grammar pin is not worth making that false. So [`painting`]
// returns `None` with a message naming the command when the packages are
// absent, and the pin returns green without asserting — EXCEPT under `CI`,
// where the step exists and its absence means a workflow stopped running it.
// There the skip is a failure, which is what keeps the skip honest.

/// The tokeniser helper, run under node with the source on stdin.
const TOKENIZER: &str = "crates/vilan-cli/tests/support/tokenize.js";
/// Where `npm ci --prefix editors/vscode` puts the two packages.
const EXTENSION_MODULES: &str = "editors/vscode/node_modules";
/// The command that makes the scope pins runnable, named in every message that
/// has to explain why they did not run.
const EXTENSION_INSTALL: &str = "npm ci --prefix editors/vscode";

/// One token the grammar produced: where it sits, its text, and the scope
/// stack it carries (outermost — always `source.vilan` — first).
#[derive(Debug)]
struct Scoped {
    line: usize,
    start: usize,
    end: usize,
    scopes: Vec<String>,
}

impl Scoped {
    /// The scope a theme actually colours this token by: the innermost one.
    fn innermost(&self) -> &str {
        self.scopes
            .last()
            .map(String::as_str)
            .expect("a scope stack")
    }
}

/// A tokenised program: the source it was read from, and every token in
/// tokenisation order. Queried by SOURCE TEXT rather than by offset, so a pin
/// reads as the claim it is making (`painting.scopes_over("</span>")`).
struct Painting {
    source: String,
    tokens: Vec<Scoped>,
}

impl Painting {
    /// The one occurrence of `needle`, as `(line, start, end)` columns. Panics
    /// unless it occurs EXACTLY once and on one line: a scope pin names the
    /// character it is asking about, and a needle that drifted into matching
    /// twice reds here rather than asserting about the wrong `>`.
    fn locate(&self, needle: &str) -> (usize, usize, usize) {
        assert!(
            !needle.contains('\n'),
            "a scope pin's needle is one line: {needle:?}"
        );
        let occurrences: Vec<usize> = self
            .source
            .match_indices(needle)
            .map(|(at, _)| at)
            .collect();
        assert_eq!(
            occurrences.len(),
            1,
            "{needle:?} occurs {} times in the exhibit — a scope pin asks about ONE place",
            occurrences.len()
        );
        let at = occurrences[0];
        let line = self.source[..at].matches('\n').count();
        let line_start = self.source[..at].rfind('\n').map_or(0, |index| index + 1);
        // The tokeniser counts UTF-16 code units within a line; every exhibit
        // here is ASCII, where that is the byte count.
        assert!(
            self.source.is_ascii(),
            "the column arithmetic below assumes an ASCII exhibit"
        );
        (at - line_start, at - line_start + needle.len(), line)
    }

    /// Every token overlapping the one occurrence of `needle`, in order.
    fn tokens_over(&self, needle: &str) -> Vec<&Scoped> {
        let (start, end, line) = self.locate(needle);
        let found: Vec<&Scoped> = self
            .tokens
            .iter()
            .filter(|token| token.line == line && token.start < end && token.end > start)
            .collect();
        assert!(!found.is_empty(), "no token covers {needle:?}");
        found
    }

    /// The scope each of those tokens is coloured by, in order — the pin's
    /// usual shape: `["punctuation.definition.tag.vilan", "entity.name.tag.vilan", …]`.
    fn scopes_over(&self, needle: &str) -> Vec<String> {
        self.tokens_over(needle)
            .into_iter()
            .map(|token| token.innermost().to_string())
            .collect()
    }

    /// The token that BEGINS at `needle`, for the pins about one character.
    fn token_at(&self, needle: &str) -> &Scoped {
        let (start, _, line) = self.locate(needle);
        self.tokens
            .iter()
            .find(|token| token.line == line && token.start == start)
            .unwrap_or_else(|| panic!("no token begins at {needle:?}"))
    }

    /// What that token is coloured by.
    fn scope_at(&self, needle: &str) -> &str {
        self.token_at(needle).innermost()
    }

    /// Its whole stack — the region pins (`meta.generic.vilan` nesting) read
    /// this, because "inside how many lists" is the claim they make.
    fn stack_at(&self, needle: &str) -> &[String] {
        &self.token_at(needle).scopes
    }

    /// How many regions named `region` that token sits inside — the nesting
    /// claims (`meta.generic.vilan` for a head's lists, `meta.tag.vilan` for
    /// an element head's extent) are what several pins below are about.
    fn region_depth(&self, needle: &str, region: &str) -> usize {
        self.stack_at(needle)
            .iter()
            .filter(|scope| scope.as_str() == region)
            .count()
    }

    /// How many generic argument lists that token sits inside.
    fn generic_depth(&self, needle: &str) -> usize {
        self.region_depth(needle, "meta.generic.vilan")
    }
}

/// `source` tokenised by the TextMate grammar, or `None` when the extension's
/// packages are not installed (see the section header: a skip on a working
/// copy, a failure under `CI`).
fn painting(source: &str) -> Option<Painting> {
    let modules = repo_root().join(EXTENSION_MODULES);
    if !modules.join("vscode-textmate").is_dir() || !modules.join("vscode-oniguruma").is_dir() {
        assert!(
            std::env::var_os("CI").is_none(),
            "`{EXTENSION_MODULES}` does not hold `vscode-textmate`/`vscode-oniguruma`, so \
             the scope pins cannot tokenise anything — and this is CI, where \
             `{EXTENSION_INSTALL}` is a step of ci.yml's `test` job and release.yml's \
             `gate` job. The step was dropped or failed: these pins are allowed to skip on \
             a working copy and never here"
        );
        eprintln!(
            "grammar_sync: the scope pins are SKIPPED — `{EXTENSION_MODULES}` is not \
             populated. `{EXTENSION_INSTALL}` makes them run (CI always does)."
        );
        return None;
    }
    let mut child = Command::new("node")
        .arg(repo_root().join(TOKENIZER))
        .env(
            "VILAN_GRAMMAR",
            repo_root().join(TEXTMATE_GRAMMAR).as_os_str(),
        )
        .env("VILAN_MODULES", modules.as_os_str())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run the tokeniser under node");
    child
        .stdin
        .take()
        .expect("the tokeniser's stdin")
        .write_all(source.as_bytes())
        .expect("write the exhibit to the tokeniser");
    let output = child.wait_with_output().expect("the tokeniser finishes");
    assert!(
        output.status.success(),
        "tokenising failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let tokens = String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| {
            // `<line>\t<start>\t<end>\t<JSON text>\t<scope> <scope> …`
            let fields: Vec<&str> = line.splitn(5, '\t').collect();
            assert_eq!(fields.len(), 5, "the tokeniser's line protocol: {line:?}");
            let number = |field: &str| field.parse().expect("a token offset");
            Scoped {
                line: number(fields[0]),
                start: number(fields[1]),
                end: number(fields[2]),
                scopes: fields[4].split(' ').map(str::to_string).collect(),
            }
        })
        .collect();
    Some(Painting {
        source: source.to_string(),
        tokens,
    })
}

// --- E161: the nested generic head, layer by layer ---------------------------
//
// `impl Source<Option<type _: Source<type U>>>` was painted by three different
// rules for one bracket kind, and by a fourth for the keyword. The vocabulary
// is asserted here on the OUTCOME — the scope each bracket, each binder and
// each tag actually carries when the editor's own tokeniser is run over a
// program (E163). The element controls (`<div>`, `a < b`) sit in the same
// exhibits, because "a head is a list and a tag is a tag" is one claim about
// two rules and the order between them, and a pin that reads the order out of
// the file cannot see a third rule reaching the text first.

/// A generic head, a markup tag and a spaced comparison in one program: three
/// `<` characters, three different vocabularies, and the head is never a tag
/// named `str` (which is what E161 was filed about).
const GENERIC_HEADS_AND_MARKUP: &str = "\
fun probe(cell: SignalCell<str>, list: List<i32>): View {
\tlet flag = a < b;
\t<my-tag>{cell}</my-tag>
}
";

/// The nested head E161 was filed over, plus the DECLARATION keyword `type` on
/// the line above it — the contrast the binder rule exists to draw.
const NESTED_GENERIC_HEAD: &str = "\
type Feed = i32;
impl Source<Option<type _: Source<type U>>> for Feed {
}
";

/// The one shape the generic list's begin cannot tell from a head: a GLUED
/// comparison, which the formatter never writes (it spaces every binary
/// operator) and which appears nowhere in std, the corpus, the examples or the
/// book. It costs the rest of its own statement, not the rest of the file.
const GLUED_COMPARISON: &str = "\
fun probe() {
\tif a<b { print(\"x\"); }
\tlet after = 1;
}
";

/// E162's exhibit: the one `;` a type argument legitimately contains.
const FIXED_ARRAY_ARGUMENT: &str = "\
fun probe(cell: SignalCell<[i32; 4]>) {
\tlet after = 1;
}
";

/// A101's exhibit: a `css` block whose declarations are CALLS. The property
/// keeps the CSS vocabulary; everything inside the parens is ordinary vilan.
const CSS_CALL_DECLARATIONS: &str = "\
fun card() {
\tcss {
\t\twidth(pct(100));
\t\t--brand-ink(gray(900));
\t\t.hover {
\t\t\tcolor(hex(\"#fafafa\"));
\t\t}
\t}
}
";

#[test]
fn a101_a_declaration_head_is_a_property_and_its_arguments_are_ordinary_vilan() {
    let Some(painting) = painting(CSS_CALL_DECLARATIONS) else {
        return;
    };
    // The property vocabulary reaches the CALL head, hyphenated names and
    // custom properties included — the grammar's own span-adjacent run.
    assert_eq!(
        painting.scope_at("width"),
        "support.type.property-name.vilan"
    );
    assert_eq!(
        painting.scope_at("--brand-ink"),
        "support.type.property-name.vilan"
    );
    // And it stops at the `(`: the value is vilan, painted by `$self`, which is
    // what the `{expression}` hole's own scopes used to mark.
    assert_eq!(
        painting.scope_at("(pct"),
        "punctuation.section.embedded.begin.vilan"
    );
    assert_ne!(
        painting.scope_at("pct"),
        "support.type.property-name.vilan",
        "an argument is an expression, not a property name"
    );
    assert_ne!(
        painting.scope_at("hex"),
        "support.type.property-name.vilan",
        "an argument inside a nested rule is an expression too"
    );
    // The dotted head is untouched: a combinator, not a property.
    assert_eq!(painting.scope_at("hover"), "entity.name.function.vilan");
    // A string argument is a string, with its own delimiters.
    assert_eq!(
        painting.scope_at("\"#fafafa\""),
        "punctuation.definition.string.begin.vilan"
    );
}

#[test]
fn e161_a_generic_head_is_a_list_a_tag_is_a_tag_and_a_comparison_is_an_operator() {
    let Some(painting) = painting(GENERIC_HEADS_AND_MARKUP) else {
        return;
    };
    // The head: its own three scopes, and the argument inside painted as the
    // type it is.
    let list = vec![
        "punctuation.definition.generic.begin.vilan".to_string(),
        "support.type.primitive.vilan".to_string(),
        "punctuation.definition.generic.end.vilan".to_string(),
    ];
    assert_eq!(painting.scopes_over("<str>"), list, "`SignalCell<str>`");
    assert_eq!(painting.scopes_over("<i32>"), list, "`List<i32>`");
    assert_eq!(painting.generic_depth("<str>"), 1);
    // The markup: `<my-tag>` and its close are tags, and neither is inside a
    // generic region.
    let tag = vec![
        "punctuation.definition.tag.vilan".to_string(),
        "entity.name.tag.vilan".to_string(),
        "punctuation.definition.tag.vilan".to_string(),
    ];
    assert_eq!(painting.scopes_over("<my-tag>"), tag, "an opening tag");
    assert_eq!(painting.scopes_over("</my-tag>"), tag, "a closing tag");
    assert_eq!(painting.generic_depth("<my-tag>"), 0);
    assert_eq!(painting.generic_depth("</my-tag>"), 0);
    // The comparison the formatter writes: an operator, neither a list nor a
    // tag named `b`.
    assert_eq!(painting.scope_at("< b"), "keyword.operator.vilan");
}

#[test]
fn e161_a_nested_head_closes_every_list_and_the_binder_is_a_binder() {
    let Some(painting) = painting(NESTED_GENERIC_HEAD) else {
        return;
    };
    // `>>>` closes the three lists the nesting opened — three brackets, three
    // depths, one vocabulary.
    assert_eq!(
        painting.scopes_over(">>>"),
        vec!["punctuation.definition.generic.end.vilan".to_string(); 3],
        "the three closing brackets"
    );
    assert_eq!(painting.generic_depth(">>>"), 3, "the innermost list");
    assert_eq!(painting.generic_depth("<Option<"), 1, "the outermost `<`");
    // The binder keyword inside a head, at both depths, and the anonymous
    // binder beside it.
    assert_eq!(
        painting.scope_at("type _"),
        "keyword.other.type-binder.vilan"
    );
    assert_eq!(
        painting.scope_at("type U"),
        "keyword.other.type-binder.vilan"
    );
    assert_eq!(
        painting.scope_at("_: Source"),
        "variable.language.wildcard.vilan"
    );
    // The contrast: `type` NAMING AN ITEM is the declaration keyword, which is
    // the scope the binder must not take.
    assert_eq!(painting.scope_at("type Feed"), "storage.type.vilan");
}

#[test]
fn e161_a_glued_comparison_gives_the_list_back_at_the_statement_boundary() {
    let Some(painting) = painting(GLUED_COMPARISON) else {
        return;
    };
    // The misfire, stated rather than papered over: a glued `<` opens a list.
    assert_eq!(
        painting.scope_at("<b {"),
        "punctuation.definition.generic.begin.vilan"
    );
    // And the bail-out, which is what keeps it to one statement: the block's
    // `{` is outside the region, and everything after it paints normally.
    assert_eq!(painting.generic_depth("{ print"), 0, "the bail-out");
    assert_eq!(painting.scope_at("print"), "entity.name.function.vilan");
    assert_eq!(painting.scope_at("1;"), "constant.numeric.vilan");
    assert_eq!(painting.generic_depth("1;"), 0, "the next statement");
}

#[test]
fn e162_a_fixed_array_argument_stays_inside_the_list() {
    let Some(painting) = painting(FIXED_ARRAY_ARGUMENT) else {
        return;
    };
    // THE POINT: the array's `;` is the list's own bail-out character, and the
    // array being its own region is what stops the bail-out from ever being
    // offered it. Every character of `[i32; 4]` is inside the list.
    assert_eq!(
        painting.scopes_over("[i32; 4]"),
        vec![
            "meta.generic.vilan".to_string(),
            "support.type.primitive.vilan".to_string(),
            "meta.generic.vilan".to_string(),
            "constant.numeric.vilan".to_string(),
            "meta.generic.vilan".to_string(),
        ],
        "the fixed-array argument"
    );
    assert_eq!(painting.generic_depth("; 4"), 1, "the length separator");
    // The list closes on its own bracket, and the tail is ordinary code again.
    assert_eq!(
        painting.scope_at(">) {"),
        "punctuation.definition.generic.end.vilan"
    );
    assert_eq!(painting.generic_depth(">) {"), 1);
    assert_eq!(painting.scope_at("1;"), "constant.numeric.vilan");
    assert_eq!(painting.generic_depth("1;"), 0);
}

// --- E164: a closing tag after text, and the `>` of an attributed head -------
//
// Two defects in one rule set, both found the day the tokeniser landed (E163)
// and neither visible to a pin that reads a regex:
//
//   1. `<span>hello</span>` painted `</span>` as `meta.generic.vilan`. The
//      generic list's begin is a `<` glued to an identifier character, `hello`
//      ends in one, and the element rule's own atom-position guard REFUSED the
//      `<` for the same reason — so the list got it. The commonest markup shape
//      in the language, mis-coloured. The generic begin declines a `</` now and
//      the closing tag is its own rule, guardless, because a `</` is never an
//      argument list whatever precedes it.
//   2. `<div class("row")>` painted its `>` as `keyword.operator.vilan`. The
//      head was a `match`, a `match` sees one line, and its optional `(/?>)`
//      could only reach a `>` with nothing between it and the tag name. The
//      head is a begin/end REGION now, so it ends where it ends — mid-line,
//      after a closure-valued item, or alone on its own line, which is E115's
//      line-start terminator subsumed rather than patched beside.
//
// Measured over the tree at the fix: of every token in all 257 tracked `.vl`
// files and all 456 `vilan` fences under `vilan/docs`, exactly 14 characters
// change scope, and every one is a `>` or `/>` of an element head moving from
// `keyword.operator.vilan` to `punctuation.definition.tag.vilan`.

/// Both defects and both controls in one program: a generic head and a spaced
/// comparison that must not move, an attributed head whose `>` must, and a
/// closing tag straight after text.
const E164_MARKUP: &str = "\
fun probe(list: List<i32>): View {
\tlet cmp = a < b;
\t<div class(\"row\")>hello</div>
}
";

/// A head written one item per line (E115's shape) whose value is a CLOSURE —
/// the `{` and the `;` inside it are the head region's own bail-out
/// characters, and `#element-head-value` is what stops them from ever being
/// offered to it.
const E164_MULTILINE_HEAD: &str = "\
fun probe(): View {
\t<a
\t\ton:click(|_| { bump(); })
\t\taria-label(\"x\")
\t>\"go\"</a>
}
";

/// The self-closing form, with the space the formatter writes.
const E164_SELF_CLOSING_HEAD: &str = "\
fun probe(): View {
\t<input type(\"checkbox\") disabled />
}
";

/// The head region's own runaway shape: a `<` glued to a name but not glued to
/// the expression before it. `a<b` is the generic list's misfire (pinned
/// above); `a <b` is this one's.
const E164_GLUED_TAG: &str = "\
fun probe() {
\tif a <b { print(\"x\"); }
\tlet after = 1;
}
";

#[test]
fn e164_a_closing_tag_after_text_is_a_tag_and_never_a_generic_list() {
    let Some(painting) = painting(E164_MARKUP) else {
        return;
    };
    let tag = vec![
        "punctuation.definition.tag.vilan".to_string(),
        "entity.name.tag.vilan".to_string(),
        "punctuation.definition.tag.vilan".to_string(),
    ];
    // THE DEFECT: `hello` ends in an identifier character, which is exactly
    // what the generic list's begin looks for.
    assert_eq!(painting.scopes_over("</div>"), tag, "the closing tag");
    assert_eq!(
        painting.region_depth("</div>", "meta.generic.vilan"),
        0,
        "the closing tag is inside a generic argument list again"
    );
    // The controls, in the same program: a generic head is still a list and a
    // spaced comparison is still an operator.
    assert_eq!(
        painting.scopes_over("<i32>"),
        vec![
            "punctuation.definition.generic.begin.vilan".to_string(),
            "support.type.primitive.vilan".to_string(),
            "punctuation.definition.generic.end.vilan".to_string(),
        ],
        "`List<i32>`"
    );
    assert_eq!(painting.scope_at("< b"), "keyword.operator.vilan");
}

#[test]
fn e164_the_bracket_closing_an_attributed_head_is_a_tag_delimiter() {
    let Some(painting) = painting(E164_MARKUP) else {
        return;
    };
    // THE DEFECT: a head item stood between the tag name and the `>`, so the
    // `match` never reached it and the operator list did.
    assert_eq!(
        painting.scope_at(">hello"),
        "punctuation.definition.tag.vilan",
        "the `>` of `<div class(\"row\")>`"
    );
    // The head opens and holds its item, which is what makes the `>` its end
    // rather than a `>` the region happened to run into.
    assert_eq!(
        painting.scope_at("<div"),
        "punctuation.definition.tag.vilan"
    );
    assert_eq!(painting.scope_at("div c"), "entity.name.tag.vilan");
    assert_eq!(painting.region_depth("class", "meta.tag.vilan"), 1);
    assert_eq!(painting.scope_at("row"), "string.quoted.double.vilan");
    // And the head ENDS there: the text child is outside it.
    assert_eq!(painting.region_depth("hello", "meta.tag.vilan"), 0);
}

#[test]
fn e164_a_head_spans_lines_and_a_closure_valued_item_does_not_end_it() {
    let Some(painting) = painting(E164_MULTILINE_HEAD) else {
        return;
    };
    // E115's shape: the `>` alone on its own line. It is the region's end now,
    // not a line-start rule sitting beside the region.
    assert_eq!(
        painting.scope_at(">\"go\""),
        "punctuation.definition.tag.vilan",
        "the `>` on its own line"
    );
    // THE POINT: `{` and `;` are the head's bail-out characters, and a closure
    // value contains both. The item's parens are consumed first, so the head
    // is still open on the line after them.
    assert_eq!(painting.region_depth("bump", "meta.tag.vilan"), 1);
    assert_eq!(painting.region_depth("; }", "meta.tag.vilan"), 1);
    assert_eq!(painting.region_depth("aria-label", "meta.tag.vilan"), 1);
    // The head's item names paint as attribute names, the event form included.
    assert_eq!(
        painting.scope_at("click"),
        "entity.other.attribute-name.vilan"
    );
    assert_eq!(
        painting.scope_at("aria-label"),
        "entity.other.attribute-name.vilan"
    );
    // The value is ordinary expression ground inside the head.
    assert_eq!(painting.scope_at("bump"), "entity.name.function.vilan");
    // And the close is a tag, painted from outside the head.
    assert_eq!(painting.region_depth("</a>", "meta.tag.vilan"), 0);
}

#[test]
fn e164_a_self_closing_head_ends_on_its_own_slash_bracket() {
    let Some(painting) = painting(E164_SELF_CLOSING_HEAD) else {
        return;
    };
    assert_eq!(
        painting.scopes_over("/>"),
        vec!["punctuation.definition.tag.vilan".to_string()],
        "` />`, the form the formatter normalises to"
    );
    assert_eq!(painting.region_depth("/>", "meta.tag.vilan"), 1);
    assert_eq!(painting.region_depth("}", "meta.tag.vilan"), 0);
}

#[test]
fn e164_a_head_that_is_really_a_comparison_gives_itself_back_at_the_statement() {
    let Some(painting) = painting(E164_GLUED_TAG) else {
        return;
    };
    // The misfire, stated rather than papered over: `<b` glued to a name and
    // not glued to what precedes it is a head by the rule. The formatter
    // writes `a < b`, and this shape appears nowhere in std, the corpus, the
    // examples or the book.
    assert_eq!(
        painting.scope_at("<b {"),
        "punctuation.definition.tag.vilan"
    );
    // And the bail-out is what keeps it to one statement — the whole reason
    // the head region carries the generic list's three boundary characters.
    assert_eq!(painting.region_depth("{ print", "meta.tag.vilan"), 0);
    assert_eq!(painting.scope_at("print"), "entity.name.function.vilan");
    assert_eq!(painting.scope_at("1;"), "constant.numeric.vilan");
    assert_eq!(painting.region_depth("1;", "meta.tag.vilan"), 0);
}

// --- E170: a head item's NAME is an attribute, not a call or a keyword ------
//
// `<div class("row")>` painted `class` `entity.name.function.vilan` and
// `<input type("checkbox")>` painted `type` `storage.type.vilan` — the call
// rule and the declaration-keyword rule reaching text inside a head, because
// the head region had only `$self` to offer its items and the attribute
// vocabulary lived in two rules that never asked to be in a head at all (the
// `on:` form and a hyphenated name, both guarded by their own spelling). So
// the head form's names were coloured by what they happen to look like
// elsewhere: `class` like a call, `type` and `for` like the keywords they
// spell. E164's head REGION is what makes the fix one rule — the vocabulary
// can be asked for inside a head and nowhere else.
//
// Measured over the tree at the fix: of every token in all 257 tracked `.vl`
// files and all 456 `vilan` fences under `vilan/docs`, 7 tokens change scope
// (22 characters in `vilan/test/element-syntax.vl` — `class`, `title`, `type`,
// `viewBox`, `d`; 16 in two doc fences — `class`, `placeholder`), and every
// one is a head item's own name. The builder spelling is untouched, which is
// the control that matters: `view("p").class("summary")` is a CALL and stays
// `entity.name.function.vilan` in all six `.vl` files that write it.

/// Every shape the rule has to tell apart, in one program: a plain attribute
/// name, two keyword-named ones, the `on:` form and a hyphenated name (the two
/// that already worked), a chained call inside a head item, a call outside any
/// head, and a postfix chain hanging off the element.
const E170_HEAD_ITEM_NAMES: &str = "\
fun probe(flag: bool): View {
\tlet widget = compute(\"x\");
\t<label class(\"row\") for(\"name\") hidden.show(flag)>
\t\t<input type(\"checkbox\") on:click(|_| { bump(); }) aria-label(\"y\") />
\t\t{widget}
\t</label>.child(<span>\"tail\"</span>)
}
";

#[test]
fn e170_a_head_items_name_is_an_attribute_name_whatever_it_spells() {
    let Some(painting) = painting(E170_HEAD_ITEM_NAMES) else {
        return;
    };
    let attribute = "entity.other.attribute-name.vilan";
    // THE DEFECT: a plain name went to the call rule, and a name that spells a
    // keyword went to the keyword list — `for` to `keyword.control.vilan` and
    // `type` to `storage.type.vilan`, the scope the declaration keyword takes.
    assert_eq!(painting.scope_at("class"), attribute, "a plain name");
    assert_eq!(painting.scope_at("for("), attribute, "a control keyword");
    assert_eq!(
        painting.scope_at("type("),
        attribute,
        "a declaration keyword"
    );
    // The two that already carried it, from rules of their own, unchanged.
    assert_eq!(painting.scope_at("click"), attribute, "the `on:` form");
    assert_eq!(
        painting.scope_at("aria-label"),
        attribute,
        "a hyphenated name"
    );
    // And every one of them is inside the head, which is the only place this
    // vocabulary is offered.
    for name in ["class", "for(", "type(", "click", "aria-label"] {
        assert_eq!(painting.region_depth(name, "meta.tag.vilan"), 1, "{name}");
    }
}

#[test]
fn e170_a_call_is_still_a_call_inside_a_head_item_and_outside_one() {
    let Some(painting) = painting(E170_HEAD_ITEM_NAMES) else {
        return;
    };
    let call = "entity.name.function.vilan";
    // Inside a head, but not a head item's own name: the rule's lookbehind
    // declines a name glued to a `.`, so a chained call keeps the call scope.
    assert_eq!(painting.scope_at("show"), call, "`hidden.show(flag)`");
    assert_eq!(painting.region_depth("show", "meta.tag.vilan"), 1);
    // Inside a head item's VALUE — `#element-head-value`'s region, which this
    // rule is not part of, so an argument paints as it would anywhere.
    assert_eq!(painting.scope_at("bump"), call, "a call in a closure value");
    assert_eq!(painting.region_depth("bump", "meta.tag.vilan"), 1);
    // Outside any head: an ordinary call, and the postfix chain the element
    // itself hangs — the head has ended at its `>` before the `.child(` runs.
    assert_eq!(
        painting.scope_at("compute"),
        call,
        "a call before the element"
    );
    assert_eq!(painting.region_depth("compute", "meta.tag.vilan"), 0);
    assert_eq!(
        painting.scope_at("child"),
        call,
        "the element's postfix chain"
    );
    assert_eq!(painting.region_depth("child", "meta.tag.vilan"), 0);
}

// --- E176: a head item with NO parens is an attribute name too --------------
//
// E170 gave the head its attribute vocabulary and reached only the names a `(`
// follows, so the two head items that carry no parens of their own painted as
// NOTHING: a bare boolean attribute (`disabled` in `<input type("checkbox")
// disabled />`) and the attribute half of an attribute-then-chain item
// (`hidden` in `hidden.show(flag)`). Both are attribute names by the grammar's
// own one-token disambiguation — a leading `.` is chain form, `on` plus `:` is
// the event form, an ident plus `(` is an attribute with a value, and a bare
// ident is a boolean attribute (element-syntax.md §113) — so the vocabulary is
// owed to them and E170's rule simply could not ask for it.
//
// TextMate only. The book's highlight.js theme has no head REGION — it is
// regex-level and paints tag names and the `on:` form from their own spelling —
// so it never carried E170's vocabulary either, and there is nothing here to
// mirror. The three-places rule is about a KEYWORD; this is a scope.
//
// Measured over the tree at the fix, tokenising every one of the 257 tracked
// `.vl` files and all 464 `vilan` fences under `vilan/docs` with both grammars
// and diffing per character: 10 characters move, all of them ONE token —
// `disabled` in `vilan/test/element-syntax.vl`, the estate's only parenless
// head item. kolt's 26 `.vl` move nothing: every head item it writes is the
// dotted chain form, which this rule's lookbehind declines.

/// Every shape the parenless rule has to tell apart: the two it must paint, the
/// tag names and the `on:` form it must not steal, a chain link's method, a
/// hyphenated name, a hole child and a call outside the markup.
const E176_PARENLESS_HEAD_ITEMS: &str = concat!(
    "fun probe(flag: bool): View {\n",
    "\tlet widget = compute(\"x\");\n",
    "\t<label class(\"row\") hidden.show(flag)>\n",
    "\t\t<input type(\"checkbox\") on:click(|_| { bump(); }) aria-label(\"y\") disabled />\n",
    "\t\t{widget}\n",
    "\t</label>\n",
    "}\n",
);

#[test]
fn e176_a_parenless_head_item_carries_the_attribute_vocabulary() {
    let Some(painting) = painting(E176_PARENLESS_HEAD_ITEMS) else {
        return;
    };
    let attribute = "entity.other.attribute-name.vilan";
    // THE DEFECT, both halves: neither name was painted at all.
    assert_eq!(
        painting.scope_at("disabled"),
        attribute,
        "a bare boolean attribute"
    );
    assert_eq!(
        painting.scope_at("hidden"),
        attribute,
        "the attribute half of `hidden.show(flag)`"
    );
    // Each inside the head, which is the only place the vocabulary is offered.
    for name in ["disabled", "hidden"] {
        assert_eq!(painting.region_depth(name, "meta.tag.vilan"), 1, "{name}");
    }
    // And the head items that already carried it still do — `on:click` in
    // particular, which this rule sits ahead of `$self` for and would have
    // taken the `on` of if its lookahead did not decline a `:`.
    assert_eq!(painting.scope_at("click"), attribute, "the `on:` form");
    assert_eq!(painting.scope_at("class"), attribute, "a valued attribute");
    assert_eq!(
        painting.scope_at("aria-label"),
        attribute,
        "a hyphenated name"
    );
}

#[test]
fn e176_the_parenless_rule_declines_a_tag_name_a_hole_and_a_chain_link() {
    let Some(painting) = painting(E176_PARENLESS_HEAD_ITEMS) else {
        return;
    };
    let attribute = "entity.other.attribute-name.vilan";
    // THE TAG-NAME CONTROL. A rule that paints bare lowercase words inside a
    // head is exactly the rule that could take the tag's own name, and the
    // head region's `begin` consuming it is what says it cannot — asserted
    // rather than assumed, on both an opening tag with items after it and one
    // with none.
    assert_eq!(painting.scope_at("label class"), "entity.name.tag.vilan");
    assert_eq!(painting.scope_at("input"), "entity.name.tag.vilan");
    // A `{hole}` child is OUTSIDE the head — the region ends at the `>` and
    // bails at a `{` — so nothing in it is an attribute and nothing in it is
    // in a head at all. (It is not one token of its own: the hole's name falls
    // to whatever `$self` makes of a plain identifier, which is the point.)
    let hole = painting.tokens_over("{widget}");
    assert!(
        hole.iter().all(|token| token.innermost() != attribute),
        "a hole child was painted as an attribute: {:?}",
        painting.scopes_over("{widget}"),
    );
    assert!(
        hole.iter()
            .all(|token| !token.scopes.iter().any(|scope| scope == "meta.tag.vilan")),
        "a hole child is not inside the head: {:?}",
        painting.scopes_over("{widget}"),
    );
    // A chain link's method keeps the call scope: the lookbehind declines a
    // name glued to a `.`, which is the same guard E170's rule carries.
    assert_eq!(painting.scope_at("show"), "entity.name.function.vilan");
    // And a call outside the markup is untouched.
    assert_eq!(painting.scope_at("compute"), "entity.name.function.vilan");
    assert_eq!(painting.region_depth("compute", "meta.tag.vilan"), 0);
}

/// The book's twin (the third place). highlight.js has no operator rule, so its
/// brackets were never mis-scoped — but its element-tag rule made the very same
/// `<type` mistake, and takes the very same guard.
#[test]
fn e161_the_books_tag_rule_ignores_a_generic_head_too() {
    let grammar = highlight_grammar(&[]);
    // Addressed by className (E171): this pin used to find the rule by
    // `regex.contains("</?")`, which is the very spelling E171 changed.
    let tag = &book_element_tag_rule(&grammar).regex;
    assert_eq!(
        regex_matches(
            tag,
            &[
                "Option<type _>",
                "SignalCell<str>",
                "Source<Option<type _: Source<type U>>>",
            ],
        ),
        vec![false, false, false],
        "the book still reads a generic head as a tag: {tag}"
    );
    assert_eq!(
        regex_matches(tag, &["<div>", "</div>", "<my-tag>"]),
        vec![true, true, true],
        "the book stopped matching an element: {tag}"
    );
}

// --- E171: the book's closing tag carries the argument list's guard ---------
//
// E164 fixed the closing tag in the TextMate grammar by giving it a rule of
// its own, deliberately without the opening form's atom-position guard. The
// book's element-tag rule is ONE regex for both forms, and the guard sat
// outside them both: `(?<=(?<![A-Za-z0-9_])</?)`, which reads "the `<` of
// either form, and not after an identifier character". That guard is only ever
// about an argument list — a tag `<` OPENS an atom, so a `<` glued to a name
// is `SignalCell<str>` and not markup — and a `</` is never an argument list
// whatever precedes it. So the book refused a closing tag glued to text for a
// reason that cannot apply to it. The one-regex fix moves the guard inside:
// `(?<=</|(?<![A-Za-z0-9_])<)`.
//
// LATENT, and measured rather than asserted. At the REGEX level the shape IS
// reached — over all 456 `vilan` fences under `vilan/docs` and all 257 tracked
// `.vl` files the fixed rule gains 10 matches in 6 files — but every one of
// them is inside a STRING or a COMMENT (`encode_utf8("<h1>hello</h1>")`,
// `markup + "\t</body>\n</html>\n"`, `// <p class="greeting">world</p>`), and
// `hljs.COMMENT`, the three string modes and the i-string modes all sit BEFORE
// the element rule in the language's `contains`, so the text is consumed
// before the tag rule is offered it and nothing rendered moves. In vilan
// itself a bare text child is a parse error, which is why no LIVE closing tag
// in the tree follows an identifier character: every one follows a `"`, a `}`
// or a `>`.

/// The shape the guard refused: a closing tag whose `<` is glued to text
/// ending in an identifier character. `<span>hello</span>` — `<` at 0, `span`
/// at 1, `</` at 11, `span` at 13.
const E171_GLUED_CLOSING_TAG: &str = "<span>hello</span>";

#[test]
fn e171_the_books_closing_tag_is_a_tag_after_an_identifier_character() {
    let grammar = highlight_grammar(&[]);
    let tag = &book_element_tag_rule(&grammar).regex;
    // THE DEFECT: only the opening tag matched — the closing one was refused
    // by the argument-list guard, for a reason a `</` cannot raise.
    assert_eq!(
        regex_match_positions(tag, E171_GLUED_CLOSING_TAG),
        vec![(1, "span".to_string()), (13, "span".to_string())],
        "{HIGHLIGHT_THEME}: both tags of {E171_GLUED_CLOSING_TAG:?} are tags: {tag}"
    );
    // The closing form that always worked, because a `}` is not an identifier
    // character — unchanged, which is what says the guard MOVED rather than
    // went.
    assert_eq!(
        regex_match_positions(tag, "<p>{name}</p>"),
        vec![(1, "p".to_string()), (11, "p".to_string())],
        "{HIGHLIGHT_THEME}: a closing tag after a hole: {tag}"
    );
    // And the guard still does its own job on the OPENING form: a `<` glued to
    // a name is an argument list (E161), and a spaced comparison is neither.
    for probe in [
        "SignalCell<str>",
        "Option<type _>",
        "Source<Option<type _: Source<type U>>>",
        "let flag = a < b;",
    ] {
        assert_eq!(
            regex_match_positions(tag, probe),
            Vec::new(),
            "{HIGHLIGHT_THEME}: {probe:?} is not markup: {tag}"
        );
    }
}
