//! The book's two copies of website-repo sources, pinned to committed
//! fixtures (K10/K15, design-language.md §2.6).
//!
//! The book must build standalone — a plain `mdbook build` fetches nothing —
//! so two files under `vilan/docs/theme/` deliberately COPY what the website
//! repo owns rather than importing it: `css/variables.css` restates the role
//! palette `src/theme.vl`'s `themed_values` table declares (both themes), and
//! `vilan.js`'s share codec restates the two functions of
//! `playground/codec.js` its ▶ links need. Both copies drifted only by luck
//! before; this file makes the drift a red suite instead of a reader's eye.
//!
//! The website repo is not present here, so each copy is held to a fixture
//! committed BESIDE it, generated at the website side and re-copied whenever
//! the source moves (each fixture's header carries the exact command):
//!
//! - `vilan/docs/theme/css/tokens-fixture.css` — the chrome leg's class-scoped
//!   rendering of `themed_values` (`html.navy` / `html.light`);
//! - `vilan/docs/theme/codec-fixture.js` — a byte-identical copy of
//!   `playground/codec.js`.
//!
//! A stale fixture is caught at regeneration time (the copy differs); a book
//! copy that disagrees with its fixture is caught here, on every suite run.

use std::path::PathBuf;

const VARIABLES_CSS: &str = "vilan/docs/theme/css/variables.css";
const TOKENS_FIXTURE: &str = "vilan/docs/theme/css/tokens-fixture.css";
const HIGHLIGHT_THEME: &str = "vilan/docs/theme/vilan.js";
const CODEC_FIXTURE: &str = "vilan/docs/theme/codec-fixture.js";

/// Fixture tokens the book deliberately does not mirror, with the reason —
/// every other fixture token must appear, byte-equal, in every theme block of
/// `variables.css` that declares role tokens.
const UNMIRRORED_TOKENS: &[(&str, &str)] = &[
    (
        "tint-comment",
        "the book states the same fact as `--code-comment-alpha` + `color-mix` \
         (the alpha is the shared truth, not the composed color — §2.5)",
    ),
    (
        "shadow",
        "the art's shadow; the art never renders in the book",
    ),
    (
        "art-error",
        "the art's diagnostic red; the art never renders in the book",
    ),
];

/// The codec functions the book's harness copies. `vilan.js` must define each
/// of these byte-equal to the fixture; any OTHER fixture function it grows a
/// copy of is held equal too (the loop checks every fixture function the book
/// defines).
const REQUIRED_CODEC_FUNCTIONS: &[&str] = &["encodeBase64Url", "deflate"];

/// Every function the fixture must define — the canonical module's full
/// surface, pinned so a gutted fixture cannot green the gate vacuously.
const FIXTURE_CODEC_FUNCTIONS: &[&str] =
    &["encodeBase64Url", "decodeBase64Url", "deflate", "inflate"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(path: &str) -> String {
    std::fs::read_to_string(repo_root().join(path))
        .unwrap_or_else(|error| panic!("reading {path}: {error}"))
}

// --- a small CSS reader ------------------------------------------------------
//
// Enough CSS to read these two files honestly: comments stripped, blocks with
// one level of `@media` nesting, declarations split on `;` and the first `:`.
// Anything outside that shape is an assertion failure, not a silent skip.

#[derive(Debug)]
struct Block {
    media: Option<String>,
    selector: String,
    declarations: Vec<(String, String)>,
}

fn strip_comments(css: &str) -> String {
    let mut out = String::new();
    let mut rest = css;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find("*/")
            .unwrap_or_else(|| panic!("an unterminated comment"));
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

fn matching_brace(source: &str, open: usize) -> usize {
    let mut depth = 0;
    for (index, character) in source[open..].char_indices() {
        match character {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return open + index;
                }
            }
            _ => {}
        }
    }
    panic!("an unbalanced brace at byte {open}");
}

fn parse_declarations(body: &str) -> Vec<(String, String)> {
    body.split(';')
        .filter_map(|declaration| {
            let declaration = declaration.trim();
            if declaration.is_empty() {
                return None;
            }
            let (name, value) = declaration
                .split_once(':')
                .unwrap_or_else(|| panic!("a declaration without a colon: {declaration:?}"));
            Some((name.trim().to_string(), value.trim().to_string()))
        })
        .collect()
}

fn parse_blocks_into(source: &str, media: Option<&str>, blocks: &mut Vec<Block>) {
    let mut position = 0;
    while let Some(offset) = source[position..].find('{') {
        let open = position + offset;
        let prelude = source[position..open].trim();
        let close = matching_brace(source, open);
        let body = &source[open + 1..close];
        if let Some(query) = prelude.strip_prefix('@') {
            assert!(
                query.starts_with("media"),
                "only @media at-rules are expected here, found @{query}"
            );
            assert!(media.is_none(), "a nested @media: {prelude}");
            parse_blocks_into(body, Some(prelude), blocks);
        } else {
            blocks.push(Block {
                media: media.map(str::to_string),
                selector: prelude.to_string(),
                declarations: parse_declarations(body),
            });
        }
        position = close + 1;
    }
    assert!(
        !source[position..].contains('}'),
        "a stray closing brace after the last block"
    );
}

fn parse_blocks(css: &str) -> Vec<Block> {
    let stripped = strip_comments(css);
    let mut blocks = Vec::new();
    parse_blocks_into(&stripped, None, &mut blocks);
    assert!(!blocks.is_empty(), "the stylesheet parsed to no blocks");
    blocks
}

// --- the tokens gate ---------------------------------------------------------

/// Which theme a `variables.css` block declares, read the way the book reads
/// it: the picker's classes, the retired stock aliases, and the no-script
/// fallback (`html:not(.js)` is light by default, dark under the
/// `prefers-color-scheme: dark` media copy).
fn theme_of(block: &Block) -> Option<&'static str> {
    if let Some(media) = &block.media {
        assert!(
            media.contains("prefers-color-scheme: dark"),
            "{VARIABLES_CSS}: an unexpected media query declares role tokens: {media}"
        );
        return Some("dark");
    }
    let selector = block.selector.as_str();
    if selector.contains(".navy") || selector.contains(".coal") || selector.contains(".ayu") {
        return Some("dark");
    }
    if selector.contains(".light")
        || selector.contains(".rust")
        || selector.contains("html:not(.js)")
    {
        return Some("light");
    }
    None
}

/// True for a custom property that belongs to the token vocabulary the
/// fixture generates — the names `variables.css` may only spell if the
/// fixture spells them (the book's own extras, `--code-*`, `--shadow-float`,
/// `--color-scheme`, `--mono-font` and mdBook's layout variables, all fall
/// outside these shapes).
fn is_role_shaped(name: &str) -> bool {
    for prefix in ["--down-", "--up-", "--stroke-", "--tint-"] {
        if name.starts_with(prefix) {
            return true;
        }
    }
    matches!(
        name,
        "--primary" | "--primary-on" | "--accent" | "--shadow" | "--art-error"
    )
}

#[test]
fn the_book_role_tokens_match_the_generated_fixture() {
    // The fixture: exactly the two class-scoped blocks the generator writes.
    let fixture = parse_blocks(&read(TOKENS_FIXTURE));
    assert_eq!(
        fixture
            .iter()
            .map(|block| block.selector.as_str())
            .collect::<Vec<_>>(),
        ["html.navy", "html.light"],
        "{TOKENS_FIXTURE}: expected exactly the html.navy and html.light blocks — regenerate it \
         (the command is in its header)"
    );
    let expected = |theme: &str, token: &str| -> &str {
        let selector = if theme == "dark" {
            "html.navy"
        } else {
            "html.light"
        };
        fixture
            .iter()
            .find(|block| block.selector == selector)
            .unwrap()
            .declarations
            .iter()
            .find(|(name, _)| name == &format!("--{token}"))
            .map(|(_, value)| value.as_str())
            .unwrap()
    };
    let fixture_tokens: Vec<String> = fixture[0]
        .declarations
        .iter()
        .map(|(name, _)| name.trim_start_matches("--").to_string())
        .collect();
    assert_eq!(
        fixture_tokens,
        fixture[1]
            .declarations
            .iter()
            .map(|(name, _)| name.trim_start_matches("--").to_string())
            .collect::<Vec<_>>(),
        "{TOKENS_FIXTURE}: the two theme blocks list different tokens — regenerate it"
    );
    for (token, _) in UNMIRRORED_TOKENS {
        assert!(
            fixture_tokens.iter().any(|name| name == token),
            "UNMIRRORED_TOKENS allows `{token}` but the fixture no longer generates it — drop it \
             from the allowlist"
        );
    }

    // The book: every block that declares role-shaped tokens is a theme block,
    // and every theme block carries the complete mirrored set, byte-equal.
    let unmirrored: Vec<&str> = UNMIRRORED_TOKENS.iter().map(|(token, _)| *token).collect();
    let blocks = parse_blocks(&read(VARIABLES_CSS));
    let mut top_level_dark = 0;
    let mut top_level_light = 0;
    let mut no_script_dark = 0;
    let mut failures = Vec::new();
    for block in &blocks {
        let role_names: Vec<&str> = block
            .declarations
            .iter()
            .map(|(name, _)| name.as_str())
            .filter(|name| is_role_shaped(name))
            .collect();
        if role_names.is_empty() {
            continue;
        }
        let theme = theme_of(block).unwrap_or_else(|| {
            panic!(
                "{VARIABLES_CSS}: the block `{}` declares role tokens ({role_names:?}) but reads \
                 as neither theme",
                block.selector
            )
        });
        match (theme, &block.media) {
            ("dark", None) => top_level_dark += 1,
            ("light", None) => top_level_light += 1,
            ("dark", Some(_)) => no_script_dark += 1,
            _ => {}
        }
        // Nothing role-shaped beyond the fixture's vocabulary…
        for name in &role_names {
            let token = name.trim_start_matches("--");
            if !fixture_tokens
                .iter()
                .any(|fixture_name| fixture_name == token)
            {
                failures.push(format!(
                    "`{name}` in the `{}` block is role-shaped but the fixture generates no such \
                     token — the book may not invent roles",
                    block.selector
                ));
            }
        }
        // …and everything mirrored present, byte-equal, per block (the
        // no-script copy dropping one token would silently fall through to
        // the other theme's value — completeness is per block on purpose).
        for token in &fixture_tokens {
            if unmirrored.contains(&token.as_str()) {
                continue;
            }
            let name = format!("--{token}");
            match block
                .declarations
                .iter()
                .find(|(declared, _)| declared == &name)
            {
                None => failures.push(format!(
                    "`{name}` is missing from the {theme} block `{}`{}",
                    block.selector,
                    block
                        .media
                        .as_deref()
                        .map(|media| format!(" (inside `{media}`)"))
                        .unwrap_or_default()
                )),
                Some((_, value)) => {
                    let generated = expected(theme, token);
                    if value != generated {
                        failures.push(format!(
                            "`{name}` in the {theme} block `{}` is `{value}` but the generated \
                             fixture says `{generated}`",
                            block.selector
                        ));
                    }
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{VARIABLES_CSS} disagrees with {TOKENS_FIXTURE} — theme.vl moved without this file, or \
         this file moved alone; regenerate the fixture (its header has the command) and update \
         the book in the same change-set:\n{}",
        failures.join("\n")
    );
    // The file's shape: the picker's two theme blocks and the no-script dark
    // copy all present (the copy is how a JS-less dark-OS reader gets dark).
    assert_eq!(
        (top_level_dark, top_level_light, no_script_dark),
        (1, 1, 1),
        "{VARIABLES_CSS}: expected one top-level dark block, one top-level light block and one \
         no-script dark copy under @media — did its shape change?"
    );
}

// --- the codec gate ----------------------------------------------------------

/// The text of `function name(…) {…}` (an `async function` included) declared
/// at a line start in `source`, from the keyword through the matching brace.
/// Exactly one declaration is required; the indentation `indent` prefixes
/// every line of it (the book's copy sits one tab deep in its IIFE) and is
/// stripped from the returned text.
fn function_text(source: &str, name: &str, indent: &str, file: &str) -> Option<String> {
    let mut found = Vec::new();
    for keyword in ["function", "async function"] {
        let needle = format!("{keyword} {name}(");
        let mut offset = 0;
        while let Some(position) = source[offset..].find(&needle) {
            let start = offset + position;
            offset = start + needle.len();
            // Declared at a line start (behind the given indentation), not a
            // mention in a comment or a nested helper.
            let line_start = source[..start]
                .rfind('\n')
                .map(|index| index + 1)
                .unwrap_or(0);
            if &source[line_start..start] != indent {
                continue;
            }
            // `function name(` also matches inside `async function name(` —
            // skip the plain form when the async keyword precedes it.
            if keyword == "function" && source[..start].ends_with("async ") {
                continue;
            }
            let open = start
                + source[start..]
                    .find('{')
                    .expect("a function without a body");
            let close = matching_brace(source, open);
            found.push(source[start..close + 1].to_string());
        }
    }
    match found.len() {
        0 => None,
        1 => {
            let text = found.remove(0);
            let mut lines: Vec<&str> = Vec::new();
            for (index, line) in text.lines().enumerate() {
                if index == 0 || indent.is_empty() {
                    lines.push(line);
                } else {
                    lines.push(line.strip_prefix(indent).unwrap_or_else(|| {
                        panic!("{file}: a line of `{name}` is not indented by {indent:?}: {line:?}")
                    }));
                }
            }
            Some(lines.join("\n"))
        }
        _ => panic!("{file}: `{name}` is declared more than once"),
    }
}

#[test]
fn the_book_share_codec_matches_the_playground_codec_fixture() {
    let fixture = read(CODEC_FIXTURE);
    let theme = read(HIGHLIGHT_THEME);
    for name in FIXTURE_CODEC_FUNCTIONS {
        assert!(
            function_text(&fixture, name, "", CODEC_FIXTURE).is_some(),
            "{CODEC_FIXTURE}: the canonical codec's `{name}` is missing — re-copy the fixture \
             from the website repo's playground/codec.js (the command is in its header)"
        );
    }
    for name in REQUIRED_CODEC_FUNCTIONS {
        assert!(
            function_text(&theme, name, "\t", HIGHLIGHT_THEME).is_some(),
            "{HIGHLIGHT_THEME}: the share codec's `{name}` is missing — the ▶ links need it"
        );
    }
    // Every fixture function the book carries a copy of is held byte-equal
    // (modulo the IIFE's one tab of indentation), so a codec edit can never
    // land on one side alone.
    for name in FIXTURE_CODEC_FUNCTIONS {
        let canonical = function_text(&fixture, name, "", CODEC_FIXTURE).unwrap();
        let Some(copied) = function_text(&theme, name, "\t", HIGHLIGHT_THEME) else {
            continue;
        };
        assert_eq!(
            copied, canonical,
            "{HIGHLIGHT_THEME}: the copy of `{name}` differs from {CODEC_FIXTURE} — edit the \
             codec at its one home (the website repo's playground/codec.js), re-copy the fixture, \
             and bring the book's copy along in the same change-set"
        );
    }
}
