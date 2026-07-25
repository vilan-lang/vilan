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
/// `vilan/proposal/org-migration.md`).
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
/// accept SVG, which is why `editors/vscode/icon.png` is rendered from the
/// brand master at all (`python3 scripts/icon_png.py`).
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
/// one of the two. Ours is opaque, on the palette's deep indigo — the same
/// ground as `galleryBanner`, and the pairing the branding README specifies for
/// the dark (pale-lavender) mark.
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
        "#110C31",
        "the listing's banner is the palette's deep indigo (assets/branding/README.md)"
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
        "https://vilan-lang.github.io/vilan/",
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
