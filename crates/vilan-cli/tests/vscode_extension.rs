//! The marketplace listing (proposal/distribution.md §3): what
//! `editors/vscode/package.json` promises the VS Code Marketplace and Open VSX.
//!
//! None of it is exercised by anything else in this repo. The extension is
//! built and packaged by CI, and the first moment a wrong publisher, a missing
//! icon, or a version that drifted from the toolchain's would be noticed is a
//! `vsce publish` against a real gallery — where a published version can be
//! unpublished but never replaced. So the manifest gets pinned here instead,
//! beside `npm_stub.rs`, which does the same job for the npm channel.
//!
//! The manifest is read with node (a suite-wide requirement already — every
//! emitted-JS test runs it) rather than by string-matching JSON.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The registered publisher identity — the org's, not a person's (F9,
/// `proposal/org-migration.md`).
const PUBLISHER: &str = "vilan-lang";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn extension_dir() -> PathBuf {
    repo_root().join("editors/vscode")
}

/// One field out of a JSON file, as node prints it.
fn json_field(file: &Path, expression: &str) -> String {
    let output = Command::new("node")
        .args(["-e", &format!("console.log({expression})")])
        .env("VILAN_JSON", file)
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "reading {}: {}",
        file.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string()
}

fn manifest_field(expression: &str) -> String {
    json_field(
        &extension_dir().join("package.json"),
        &format!("require(process.env.VILAN_JSON).{expression}"),
    )
}

#[test]
fn the_publisher_is_the_registered_marketplace_identity() {
    assert_eq!(
        manifest_field("publisher"),
        PUBLISHER,
        "the marketplace publisher is registered and permanent — a different \
         value here publishes into someone else's namespace, or nowhere"
    );
}

/// One version for the CLI, the LSP, the embedded std, and the extension
/// (releases.md §4) — `scripts/bump-version.sh` sets all of them, and this
/// crate's own version is that version. `npm version` keeps the lockfile in
/// step; a hand-edited manifest does not, and the lock is what `npm ci` builds
/// the published `.vsix` from.
#[test]
fn the_extension_version_is_the_toolchain_version() {
    let version = env!("CARGO_PKG_VERSION");
    assert_eq!(manifest_field("version"), version, "package.json");

    let lock = extension_dir().join("package-lock.json");
    assert_eq!(
        json_field(&lock, "require(process.env.VILAN_JSON).version"),
        version,
        "package-lock.json"
    );
    assert_eq!(
        json_field(
            &lock,
            "require(process.env.VILAN_JSON).packages[''].version"
        ),
        version,
        "package-lock.json's root package entry"
    );
}

/// The icon the manifest names, as bytes. PNG only: the marketplace does not
/// accept SVG, which is why `editors/vscode/icon.png` is a vendored copy of
/// the brand pipeline's `baked/icon_256.png` (the private `vilan-lang/branding`
/// repository bakes it from the flat mark).
fn icon() -> Vec<u8> {
    let name = manifest_field("icon");
    assert!(!name.is_empty() && name != "undefined", "no `icon` field");
    let path = extension_dir().join(&name);
    std::fs::read(&path).unwrap_or_else(|error| {
        panic!(
            "the manifest names icon {name:?}, which is not readable at {}: {error}",
            path.display()
        )
    })
}

/// The first 8 bytes are the signature, then the IHDR chunk: length (4), type
/// (4), width (4), height (4), bit depth (1), colour type (1).
fn image_header() -> (u32, u32, u8) {
    let bytes = icon();
    assert_eq!(
        &bytes[..8],
        b"\x89PNG\r\n\x1a\n",
        "the icon is not a PNG — the marketplace rejects every other format"
    );
    assert_eq!(&bytes[12..16], b"IHDR", "malformed PNG: IHDR is not first");
    let read = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    (read(16), read(20), bytes[25])
}

#[test]
fn the_icon_is_square_and_at_least_the_marketplace_minimum() {
    let (width, height, _) = image_header();
    assert_eq!(width, height, "the icon is {width}x{height}, not square");
    assert!(
        width >= 128,
        "the icon is {width}px — the marketplace requires 128 and recommends 256"
    );
}

/// Colour type 2 is truecolour *without* an alpha channel. It matters: gallery
/// pages render the icon on a white card and the editor's Extensions view
/// renders it on the user's theme, so a transparent icon can only read against
/// one of the two. Ours is opaque, on the palette's primary-dark ground — the
/// same ground as `galleryBanner`, and the pairing the branding README
/// specifies for the light (blush) mark.
#[test]
fn the_icon_is_opaque_so_it_reads_on_a_light_gallery_and_a_dark_one() {
    let (_, _, colour_type) = image_header();
    assert_eq!(
        colour_type, 2,
        "the icon carries an alpha channel — it will vanish into one of the two \
         backgrounds the marketplace renders it on"
    );
}

#[test]
fn the_gallery_banner_is_the_brands_dark_ground() {
    assert_eq!(
        manifest_field("galleryBanner.color"),
        "#120004",
        "the listing's banner is the palette's primary dark (assets/branding/README.md)"
    );
    assert_eq!(
        manifest_field("galleryBanner.theme"),
        "dark",
        "a dark ground needs the light text the `dark` theme selects"
    );
}

/// The description is the one line of prose the gallery shows beside the icon,
/// and it went stale: it still advertised the v0.7-era feature set long after
/// completion, inlay hints, semantic tokens, formatting, and Organize Imports
/// shipped. Both halves are pinned — the old phrasing is gone, *and* the
/// features it was missing are named — because a description that merely
/// dropped the stale sentence would pass a one-sided check while saying
/// nothing.
#[test]
fn the_description_matches_what_the_extension_does_today() {
    let description = manifest_field("description");
    assert!(
        !description.contains("syntax highlighting, diagnostics, hover, go-to-definition"),
        "the description is still the stale feature list: {description}"
    );
    for feature in [
        "completion",
        "inlay hints",
        "semantic",
        "formatting",
        "Organize Imports",
    ] {
        assert!(
            description.contains(feature),
            "the description does not mention {feature:?}: {description}"
        );
    }
}

#[test]
fn the_listing_links_point_at_the_project() {
    assert_eq!(
        manifest_field("homepage"),
        "https://vilan-lang.org/docs/",
        "the listing's homepage is the book"
    );
    assert_eq!(
        manifest_field("bugs.url"),
        format!("https://github.com/{PUBLISHER}/vilan/issues"),
    );
    assert_eq!(
        manifest_field("repository.url"),
        format!("https://github.com/{PUBLISHER}/vilan"),
    );
}

#[test]
fn the_listing_carries_search_keywords() {
    let keywords = manifest_field("keywords.join(' ')");
    assert!(
        keywords.split(' ').any(|keyword| keyword == "vilan"),
        "the extension's keywords must include the language's name, which is \
         what someone searching the marketplace types: {keywords}"
    );
}

/// `editors/vscode/README.md` *is* the marketplace listing page, and vsce
/// rewrites every relative link in it against the **repository root** — this
/// extension lives in a subdirectory, so a relative link that resolves fine on
/// GitHub becomes a 404 on the listing. Absolute links only.
#[test]
fn the_listing_page_has_no_relative_links() {
    let readme = std::fs::read_to_string(extension_dir().join("README.md")).expect("README.md");
    let mut broken = Vec::new();
    for (number, line) in readme.lines().enumerate() {
        let mut rest = line;
        while let Some(open) = rest.find("](") {
            rest = &rest[open + 2..];
            let Some(close) = rest.find(')') else { break };
            let target = &rest[..close];
            if !target.starts_with("http") && !target.starts_with('#') {
                broken.push(format!("{}: {target}", number + 1));
            }
            rest = &rest[close..];
        }
    }
    assert!(
        broken.is_empty(),
        "relative links in the marketplace listing (vsce resolves them against \
         the repository root, not editors/vscode/):\n{}",
        broken.join("\n")
    );
}

// --- E194: the client's word pattern ----------------------------------------

/// The word at the END of `text`, as the CLIENT would compute it: the
/// language configuration's own `wordPattern`, run by node's RegExp (the same
/// engine VS Code uses), asked for the match that touches the cursor.
///
/// This is the whole of the bug E194 closed, and it can only be seen from the
/// client's side: the server's candidate list is right — `stroke-width` is in
/// it, and `crates/vilan-ide`'s pins assert so — but VS Code filters that list
/// against the word under the cursor, and with no `wordPattern` declared its
/// DEFAULT excludes `-`. At `<svg stroke-w|` the word was `w`, which
/// `stroke-width` does not match, so the one candidate the author was typing
/// towards was the one that disappeared. The `Completion` type carries no
/// `filter_text` and the server sends no edit range, so nothing server-side
/// could override it.
fn word_at_end(text: &str) -> Option<String> {
    let script = "const fs = require('fs');\n\
         const config = JSON.parse(fs.readFileSync(process.env.VILAN_JSON, 'utf8'));\n\
         if (!config.wordPattern) { console.log('NO-WORD-PATTERN'); process.exit(0); }\n\
         const text = process.env.VILAN_TEXT;\n\
         const pattern = new RegExp(config.wordPattern, 'g');\n\
         let answer = '';\n\
         let match;\n\
         while ((match = pattern.exec(text)) !== null) {\n\
         \x20   if (match.index + match[0].length === text.length) answer = match[0];\n\
         \x20   if (match[0].length === 0) break;\n\
         }\n\
         console.log(answer);";
    let output = Command::new("node")
        .args(["-e", script])
        .env(
            "VILAN_JSON",
            extension_dir().join("language-configuration.json"),
        )
        .env("VILAN_TEXT", text)
        .output()
        .expect("run node");
    assert!(
        output.status.success(),
        "the word pattern must be a valid RegExp: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let answer = String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_string();
    assert_ne!(
        answer, "NO-WORD-PATTERN",
        "the language configuration must declare a `wordPattern` — without one \
         the client's default excludes `-`, and every hyphenated attribute and \
         css property is filtered out of its own completion list (E194)"
    );
    (!answer.is_empty()).then_some(answer)
}

/// The item's own position: `<svg stroke-w|`. The word must be `stroke-w`, so
/// the client keeps `stroke-width` in the list and replaces the whole prefix
/// when it is accepted.
#[test]
fn the_word_pattern_reads_a_hyphenated_attribute_prefix_whole() {
    assert_eq!(
        word_at_end("\t\tview(\"svg\") <svg stroke-w").as_deref(),
        Some("stroke-w"),
        "an attribute prefix in element-head position is ONE word"
    );
    // A css property is the same shape one syntax over (E153's vocabulary),
    // and a CUSTOM property keeps its leading dashes (A101 R12).
    assert_eq!(
        word_at_end("\tcss {\n\t\tfont-fam").as_deref(),
        Some("font-fam")
    );
    assert_eq!(word_at_end("\t\t--card-ga").as_deref(), Some("--card-ga"));
}

/// The pattern WIDENS the default and narrows nothing: every word the default
/// reads whole is still one word. A word pattern is not only completion — it
/// is double-click, `Ctrl+D` and word-wise motion — so a regression here is
/// felt on every keystroke in the editor, which is why the pin is a table
/// rather than the one case the item names.
#[test]
fn the_word_pattern_keeps_every_word_the_default_read() {
    for (text, expected) in [
        // Ordinary identifiers and paths: `.` and `::` still break a word.
        ("foo.bar", Some("bar")),
        ("Route::Home", Some("Home")),
        ("view(\"svg\").stroke_width", Some("stroke_width")),
        // Numbers, suffixed numbers, and decimals.
        ("let a = 1.5", Some("1.5")),
        ("padding(px(4", Some("4")),
        ("let width = 4px", Some("4px")),
        ("let n: i53", Some("i53")),
        // NON-ASCII, the case a hand-written ASCII identifier class breaks:
        // a comment or a string in any language must still have words.
        ("// le café", Some("café")),
        // A SPACED binary minus — what the formatter writes — is untouched.
        ("let d = x - 1", Some("1")),
        // And a position that is not in a word has none.
        ("let a = ", None),
    ] {
        assert_eq!(
            word_at_end(text).as_deref(),
            expected,
            "word at the end of {text:?}"
        );
    }
    // The one deliberate widening beyond attribute position: an UNSPACED
    // binary minus reads as one word. Canonical vilan spaces it (`vilan fmt`
    // is authoritative), so this is reachable only in text the formatter has
    // not seen — and it is the price of the hyphen, named rather than hidden.
    assert_eq!(word_at_end("let d = a-b").as_deref(), Some("a-b"));
}

// --- E202 / E203: the pairs the configuration declares -----------------------

/// The language configuration, as node parses it, printed as JSON so a pin can
/// assert over the whole list rather than one field.
fn language_configuration() -> String {
    json_field(
        &extension_dir().join("language-configuration.json"),
        "JSON.stringify(JSON.parse(require('fs').readFileSync(process.env.VILAN_JSON, 'utf8')))",
    )
}

/// E202 (R9): `<`/`>` is a SURROUNDING pair — select a type name, type `<`, and
/// the selection is wrapped — and deliberately NOT an auto-closing one.
///
/// The two are different questions. Surrounding a SELECTION is unambiguous:
/// there is no reading of "wrap this in `<>`" that means a comparison. Typing
/// `<` on its own is ambiguous, and a static auto-closing pair cannot tell
/// `List<` from `a < b` (its only filter is `notIn: [string, comment]`), so
/// that half is the SERVER's — `onTypeFormatting`, which knows which names are
/// types. A `<` entry here would grow a `>` in every comparison anybody typed.
///
/// **What the server half costs, written down because the pin cannot hold it.**
/// VS Code types OVER a closing character only when it auto-inserted that
/// character itself (`editor.autoClosingOvertype: "auto"`), and an edit the
/// server returned is not that — so typing the `>` of `List<i32>` by hand
/// yields `List<i32>>`, and an `onTypeFormatting` answer cannot fix it because
/// a text edit cannot move the caret past a character it leaves in place
/// (`List<List<i32>>` needs the caret between the two, not before them). The
/// only configuration that buys overtype is an `autoClosingPairs` entry for
/// `<`, which is the hazard this pin exists to keep out. Tracker E202's owed
/// "typing `>` over the placed one does not double it" is therefore NOT
/// pinnable by configuration and is reported as an open question, with a
/// client-side `type` override in `extension.ts` — which can move the caret —
/// as the candidate follow-up.
#[test]
fn e202_the_angle_pair_surrounds_but_does_not_auto_close() {
    let config = language_configuration();
    assert!(
        config.contains(r#"["<","<>"#) || config.contains(r#"["<",">"]"#),
        "`surroundingPairs` must carry [\"<\", \">\"]: {config}"
    );
    assert_eq!(
        config.matches(r#""open":"<""#).count(),
        0,
        "`<` must NOT be an autoClosingPair — `a < b` would grow a `>` (E202): {config}"
    );
}

/// E203 (R9): the backtick is a GLOBAL pair with `notIn: ["string"]`.
///
/// Global — not scoped to comments — because a backtick means nothing to the
/// lexer, so there is no position where pairing one breaks code. `notIn:
/// ["string"]` is the one exclusion that matters: a string body is text the
/// author is writing literally, and a second backtick appearing inside one is
/// a character they did not type. Doc prose is where backticks live (every std
/// `///` uses them), and it is a comment, which the exclusion does not cover.
#[test]
fn e203_the_backtick_is_a_global_pair_outside_strings() {
    let config = language_configuration();
    assert!(
        config.contains(r#"{"open":"`","close":"`","notIn":["string"]}"#),
        "the backtick must auto-close everywhere but inside a string body: {config}"
    );
    assert!(
        config.contains(r#"["`","`"]"#),
        "and surround a selection, which is the other half of the ask: {config}"
    );
}

/// E203: a pair is placed only BEFORE one of a conservative set of characters,
/// stated rather than inherited.
///
/// VS Code's `autoCloseBefore` decides whether an auto-closing pair fires at
/// all: it fires only when the character AFTER the cursor is one of these (or
/// the line ends there). Without the field a language inherits exactly this
/// set, so declaring it changes nothing today — and that is the point. The
/// behaviour a backtick needs (typing one before a WORD character must not
/// pair, because `` `word `` is somebody quoting a word that is already
/// written) now reads off this file instead of off a default that could move
/// under it.
#[test]
fn e203_a_pair_fires_only_before_the_conservative_set() {
    let config = language_configuration();
    assert!(
        config.contains(r#""autoCloseBefore":";:.,=}])> \n\t""#),
        "the language declares its own `autoCloseBefore`: {config}"
    );
    // The characters a backtick must NOT pair before are the ones absent from
    // that set, and a word character is the case the item names.
    for character in ['a', 'Z', '0', '_', '`'] {
        assert!(
            !";:.,=}])> \n\t".contains(character),
            "`{character}` must stay out of the set, or a backtick pairs before a word"
        );
    }
}

/// The premise a GLOBAL backtick pair rests on: the lexer has no backtick
/// token at all, so a backtick is never syntax and pairing one can never
/// change what a program means (E203).
///
/// A grep, deliberately — the claim is about the absence of a token, and an
/// absence has no behavior to observe. It COUNTS rather than `contains`-es, so
/// the day someone adds a template-literal token this pin reds and the pair's
/// scope is re-decided with the real case in hand.
#[test]
fn e203_the_lexer_still_has_no_backtick_token() {
    let lexing = std::fs::read_to_string(repo_root().join("crates/vilan-core/src/lexing.rs"))
        .expect("read lexing.rs");
    // Doc comments quote code with backticks by the hundred, so the search is
    // for the CHARACTER LITERAL a lexer arm would have to match on, not for the
    // character.
    let literal = format!("'{}'", '`');
    assert_eq!(
        lexing.matches(&literal).count(),
        0,
        "the lexer now matches a backtick: E203's global pair assumed it could not, \
         and the pair's `notIn` list must be re-decided (tracker E203)"
    );
    let token = std::fs::read_to_string(repo_root().join("crates/vilan-core/src/token.rs"))
        .expect("read token.rs");
    assert_eq!(
        token.to_lowercase().matches("backtick").count(),
        0,
        "a `Backtick` token appeared: see above"
    );
}
