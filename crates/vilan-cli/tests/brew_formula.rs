//! The Homebrew tap (proposal/distribution.md §4): `homebrew/Formula/vilan.rb`
//! and the script that writes it.
//!
//! The formula lives in another repository — `vilan-lang/homebrew-vilan` —
//! and is generated: `scripts/brew-formula.sh <version> sha256sums.txt`
//! renders it, the release workflow's `publish-brew` job runs that against the
//! tag's own checksums and pushes the result, and the copy staged under
//! `homebrew/` here is the same command's output for the current release. That
//! copy is what seeded the tap and what a reviewer reads, so the first pin is
//! that it is exactly what the job would write — a hand-edit there, or a
//! change to the script that nobody re-ran, fails here rather than at the next
//! `brew install`.
//!
//! Nothing else in this repository exercises any of it. A wrong checksum, a
//! target that lost its block, or a `url` pointing at `releases/latest` would
//! first be noticed by a user on a platform nobody tested, which is why the
//! rest of the file pins the formula's contents directly.
//!
//! unix-only: Homebrew runs on macOS and Linux, the generator is a POSIX shell
//! script, and the Windows leg of CI has no shell to run it with
//! (`tests/upgrade.rs` is gated the same way, for the same reason).
#![cfg(unix)]

use std::path::PathBuf;
use std::process::{Command, Output};

/// The release the committed formula describes: the version whose checksums
/// `SHA256SUMS` are, and therefore the version the generator has to be handed
/// to reproduce the file byte-for-byte.
const SEED_VERSION: &str = "0.14.0";

/// `sha256sums.txt` exactly as the v0.14.0 GitHub Release publishes it —
/// fetched once, checked in, never fetched again: a test that reached the
/// network would fail offline and, worse, would pass by comparing a freshly
/// downloaded file against a formula generated from the same download.
///
/// It is the whole file, not the four lines the formula needs. The extra
/// entries (the two install scripts, the `.vsix`, the Windows `.zip`) are what
/// make the generator's selection a real selection.
const SHA256SUMS: &str = "\
07fd5f824c650b81ae58bf33b11cb3e3e4d79c22c27a58dfa373bcf7c847eb76  install.ps1
d6a4ba031b90fc8aafb1e1b52f4e9754e28ad95e031b1f4562c1ca4042f76371  install.sh
f4ccc413449dddd334549962719ba59f10e7091bc8b91f1d98bd2b8ef2a05343  vilan-aarch64-apple-darwin.tar.gz
3e85a1f663efbcb610a88028aa0844e5ff3fbaf8010dfcf6d808c882d1639b68  vilan-aarch64-unknown-linux-musl.tar.gz
71e503adc5b0562296697d3153ea28b41d0aab16a1756f8bf5ac9d7f844d0bc8  vilan-vscode.vsix
491e6cc1cd945c4ec36d453d537a14c78d8d5a1a36d7175f57377f12bc275d23  vilan-x86_64-apple-darwin.tar.gz
55fb6311e967815da2bc6e676002ef29f2a25ae709ed01ad283994dc63979ec8  vilan-x86_64-pc-windows-msvc.zip
644d7df705d8b4ea92b9c912c6602d8ea94f553c34b21f5ee5571bc701f5f6d0  vilan-x86_64-unknown-linux-musl.tar.gz
";

/// The four targets Homebrew installs, in the order the formula names them.
/// The release matrix builds a fifth — see
/// `the_generator_covers_every_unix_target_the_release_matrix_builds`.
const TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-musl",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The committed formula — the tap's content, staged here.
fn formula() -> String {
    let path = repo_root().join("homebrew/Formula/vilan.rb");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

/// Runs the generator the way the release workflow does — the script by its
/// own path, so the test also depends on its executable bit being intact.
fn generate(label: &str, version: &str, sums: &str) -> Output {
    let file = std::env::temp_dir().join(format!(
        "vilan-brew-{label}-{}.sha256sums.txt",
        std::process::id()
    ));
    std::fs::write(&file, sums).expect("write the checksum fixture");
    let output = Command::new(repo_root().join("scripts/brew-formula.sh"))
        .args([version, &file.to_string_lossy()])
        .output()
        .expect("run scripts/brew-formula.sh");
    let _ = std::fs::remove_file(&file);
    output
}

fn describe(output: &Output) -> String {
    format!(
        "status {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Every `url` with the `sha256` that follows it, in order. Pairing is what
/// makes them meaningful: a checksum belongs to one archive and to no other.
fn urls_with_checksums(formula: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut pending: Option<String> = None;
    for line in formula.lines() {
        let line = line.trim();
        if let Some(url) = line.strip_prefix("url \"") {
            assert!(
                pending.is_none(),
                "two urls in a row — {url} has no checksum before it"
            );
            pending = Some(url.trim_end_matches('"').to_string());
        } else if let Some(checksum) = line.strip_prefix("sha256 \"") {
            let url = pending.take().expect("a sha256 with no url above it");
            pairs.push((url, checksum.trim_end_matches('"').to_string()));
        }
    }
    assert!(pending.is_none(), "a url with no sha256 under it");
    pairs
}

/// What the release published for one asset, out of the fixture.
fn published_checksum(asset: &str) -> &'static str {
    SHA256SUMS
        .lines()
        .find_map(|line| {
            let (checksum, name) = line.split_once("  ")?;
            (name == asset).then_some(checksum)
        })
        .unwrap_or_else(|| panic!("the release published no {asset}"))
}

/// One of the formula's top-level settings, as written (quotes included).
fn setting(name: &str) -> String {
    let formula = formula();
    let prefix = format!("  {name} ");
    formula
        .lines()
        .find_map(|line| line.strip_prefix(prefix.as_str()))
        .unwrap_or_else(|| panic!("the formula sets no `{name}`"))
        .trim()
        .to_string()
}

/// The same, for a setting whose value is a single string.
fn string_setting(name: &str) -> String {
    setting(name).trim_matches('"').to_string()
}

fn semantic(version: &str) -> (u64, u64, u64) {
    let parts: Vec<u64> = version
        .split('.')
        .map(|part| {
            part.parse()
                .unwrap_or_else(|_| panic!("{version} is not a three-number version"))
        })
        .collect();
    assert_eq!(parts.len(), 3, "{version} is not a three-number version");
    (parts[0], parts[1], parts[2])
}

// ---------------------------------------------------------------------------
// The formula and the script cannot disagree
// ---------------------------------------------------------------------------

/// The no-drift pin. `homebrew/Formula/vilan.rb` is not a document that
/// happens to resemble the generator's output — it *is* the output, for this
/// release's version and this release's checksums. Anything that edits one
/// without the other lands here.
#[test]
fn the_committed_formula_is_the_generators_output_for_the_released_checksums() {
    let rendered = generate("no-drift", SEED_VERSION, SHA256SUMS);
    assert!(
        rendered.status.success(),
        "the generator failed: {}",
        describe(&rendered)
    );
    let rendered = String::from_utf8(rendered.stdout).expect("the formula is text");
    assert_eq!(
        rendered,
        formula(),
        "homebrew/Formula/vilan.rb is not what scripts/brew-formula.sh {SEED_VERSION} \
         writes for the v{SEED_VERSION} checksums — regenerate it (the release \
         workflow will), or fix the script"
    );
}

/// The pin the byte comparison above *cannot* give: it compares the generator
/// with its own output, so a generator that paired every url with the wrong
/// archive's checksum would satisfy it happily. Here each url is checked
/// against the release's published checksum for the asset it actually names.
#[test]
fn every_url_carries_the_checksum_the_release_published_for_that_archive() {
    let formula = formula();
    let pairs = urls_with_checksums(&formula);
    assert_eq!(pairs.len(), TARGETS.len(), "one url per platform");
    for (url, checksum) in pairs {
        let asset = url.rsplit('/').next().expect("a file name");
        assert_eq!(
            checksum,
            published_checksum(asset),
            "{asset} is paired with a checksum that is not its own — Homebrew \
             would refuse every install on that platform"
        );
    }
}

/// A formula pins a version. `releases/latest/download/…` is a URL whose bytes
/// change the moment the next release goes out, under a `sha256` that does
/// not — every install on the tap would start failing the checksum, and the
/// fix would be a formula nobody edited.
#[test]
fn every_url_pins_the_release_tag_rather_than_latest() {
    let formula = formula();
    let expected =
        format!("https://github.com/vilan-lang/vilan/releases/download/v{SEED_VERSION}/");
    for (url, _) in urls_with_checksums(&formula) {
        assert!(
            url.starts_with(&expected),
            "{url} is not under this release's tag ({expected})"
        );
    }
    assert!(
        !formula.contains("releases/latest"),
        "the formula names a moving download URL"
    );
}

/// Four archives, one per platform Homebrew runs on — and the Windows archive
/// the same release carries is not one of them. A `.zip` here would install
/// nothing on any machine that could reach it.
#[test]
fn the_formula_names_the_four_unix_archives_and_no_windows_asset() {
    let formula = formula();
    let named: Vec<String> = urls_with_checksums(&formula)
        .into_iter()
        .map(|(url, _)| url.rsplit('/').next().expect("a file name").to_string())
        .collect();
    let expected: Vec<String> = TARGETS
        .iter()
        .map(|target| format!("vilan-{target}.tar.gz"))
        .collect();
    assert_eq!(named, expected, "the formula's archives");

    for absent in ["windows", ".zip", "pc-windows-msvc", "install.ps1"] {
        assert!(
            !formula.contains(absent),
            "the formula mentions {absent:?} — Homebrew does not run there, and \
             install.ps1 is that platform's channel"
        );
    }
}

/// The targets are written down twice: the release matrix builds them, the
/// generator turns them into `on_*` blocks. A sixth target added to the matrix
/// with no decision here would ship a release Homebrew silently cannot install
/// on — so the two lists are compared, with Windows subtracted by name.
#[test]
fn the_generator_covers_every_unix_target_the_release_matrix_builds() {
    let workflow = std::fs::read_to_string(repo_root().join(".github/workflows/release.yml"))
        .expect("read the release workflow");
    let mut built: Vec<&str> = workflow
        .lines()
        .filter_map(|line| line.trim().strip_prefix("- target: "))
        .filter(|target| !target.contains("windows"))
        .collect();
    built.sort();
    assert_eq!(built.len(), TARGETS.len(), "unix targets in the matrix");

    let script = std::fs::read_to_string(repo_root().join("scripts/brew-formula.sh"))
        .expect("read the generator");
    let table = script
        .split_once("TARGETS=\"")
        .expect("the TARGETS table")
        .1
        .split_once('"')
        .expect("its closing quote")
        .0;
    let mut rendered: Vec<&str> = table
        .lines()
        .map(|row| {
            let (target, rest) = row
                .split_once(':')
                .expect("<rust target>:<os block>:<cpu block>");
            let (operating_system, cpu) = rest.split_once(':').expect("<os block>:<cpu block>");
            assert!(
                ["macos", "linux"].contains(&operating_system),
                "{row}: Homebrew has no on_{operating_system}"
            );
            assert!(["arm", "intel"].contains(&cpu), "{row}: no on_{cpu}");
            target
        })
        .collect();
    rendered.sort();
    assert_eq!(
        rendered, built,
        "scripts/brew-formula.sh and the release matrix disagree about which \
         unix targets exist"
    );

    let mut sorted = TARGETS;
    sorted.sort();
    assert_eq!(rendered, sorted, "and this file's own list is a third copy");
}

// ---------------------------------------------------------------------------
// What the formula claims
// ---------------------------------------------------------------------------

/// `homepage`, `license` and `version` are the listing, the legal notice, and
/// the thing `brew upgrade` compares against. Each is checked against where it
/// comes from rather than against a literal repeated here: the book URL the
/// other channels publish, and the crate's own dual license.
#[test]
fn the_formula_carries_the_projects_identity() {
    let homepage = string_setting("homepage");
    assert_eq!(homepage, "https://vilan-lang.org/docs/");
    let npm = std::fs::read_to_string(repo_root().join("npm/meta/package.json"))
        .expect("read the npm meta manifest");
    assert!(
        npm.contains(&format!("\"homepage\": \"{homepage}\"")),
        "the tap and the npm package point users at different homepages"
    );

    let manifest = std::fs::read_to_string(repo_root().join("crates/vilan-cli/Cargo.toml"))
        .expect("read the CLI manifest");
    let declared = manifest
        .lines()
        .find_map(|line| line.strip_prefix("license = "))
        .expect("the crate declares a license")
        .trim_matches('"');
    let mut expected: Vec<&str> = declared.split(" OR ").collect();
    expected.sort();
    let license = setting("license");
    let mut named: Vec<&str> = license
        .strip_prefix("any_of: [")
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or_else(|| panic!("a dual license is spelled `any_of: [...]`, not {license}"))
        .split(',')
        .map(|name| name.trim().trim_matches('"'))
        .collect();
    named.sort();
    assert_eq!(
        named, expected,
        "the formula's license and the crate's disagree ({license} vs {declared})"
    );

    assert_eq!(
        string_setting("version"),
        SEED_VERSION,
        "the archives carry no version in their names, so `version` is the only \
         thing telling Homebrew what it installed"
    );
}

/// Homebrew's own audit rules for `desc`, which no machine here can run
/// (`brew` is macOS/Linuxbrew-only) and which nothing else would catch until
/// the tap was audited by hand: no leading article, no leading formula name,
/// no closing full stop, under 80 characters, and it has to say what this is.
#[test]
fn the_description_reads_the_way_homebrew_requires() {
    let desc = string_setting("desc");
    assert!(desc.len() < 80, "{} characters: {desc}", desc.len());
    assert!(
        desc.chars().next().is_some_and(char::is_uppercase),
        "a desc starts with a capital: {desc}"
    );
    assert!(!desc.ends_with('.'), "a desc has no full stop: {desc}");
    let first = desc.split_whitespace().next().expect("a first word");
    assert!(
        !["a", "an", "the"].contains(&first.to_lowercase().as_str()),
        "a desc does not start with an article: {desc}"
    );
    assert!(
        !desc.to_lowercase().starts_with("vilan"),
        "a desc does not start with the formula's own name: {desc}"
    );
    for word in ["vilan", "language server"] {
        assert!(
            desc.contains(word),
            "the one line the tap shows never says {word:?}: {desc}"
        );
    }
}

/// Both binaries, or the extension's language server is missing from every
/// Homebrew install — and the smoke test has to be one that would notice.
#[test]
fn the_formula_installs_both_binaries_and_smoke_tests_the_version() {
    let formula = formula();
    assert!(
        formula.contains(r#"bin.install "vilan", "vilan-lsp""#),
        "the formula does not install both binaries"
    );
    assert!(
        formula.contains(r#"prefix.install "LICENSE-MIT", "LICENSE-APACHE""#),
        "the archives carry both licenses and a redistribution ships them"
    );
    assert!(
        formula.contains(r##"assert_match version.to_s, shell_output("#{bin}/vilan --version")"##),
        "`brew test vilan` must run the compiler and see this version — which is \
         what `vilan --version` prints"
    );
}

/// The version relationship the release process actually allows.
///
/// Not the equality `tests/vscode_extension.rs` uses for the extension: that
/// manifest is bumped by `scripts/bump-version.sh` before the tag, while a
/// formula can only be written *after* the release exists, from checksums of
/// archives that do not exist until CI builds them. So between a bump and its
/// publish the seed is one release behind on purpose, and an equality pin
/// would fail the suite at exactly the step the release process runs it
/// (`bump → changelog → suite → tag`) — it would make the next release
/// impossible to cut.
///
/// What is always wrong is a formula naming a version the workspace has never
/// reached: a hand-typed tag, or a seed for a release that was never cut. That
/// is what this pins.
#[test]
fn the_seed_formula_never_names_a_version_the_workspace_has_not_reached() {
    let workspace = env!("CARGO_PKG_VERSION");
    assert!(
        semantic(SEED_VERSION) <= semantic(workspace),
        "the tap's formula is at {SEED_VERSION}, ahead of the workspace's \
         {workspace} — no such release exists to download"
    );
}

// ---------------------------------------------------------------------------
// The generator refuses to write a formula it cannot stand behind
// ---------------------------------------------------------------------------

/// A checksum file missing one target renders a formula that installs on three
/// platforms and fails on the fourth — with a Ruby error, at the user's
/// machine. One case per target, because "the loop covers them" is exactly the
/// claim a single example does not support.
#[test]
fn the_generator_fails_loudly_when_a_target_has_no_checksum() {
    for target in TARGETS {
        let asset = format!("vilan-{target}.tar.gz");
        let sums: String = SHA256SUMS
            .lines()
            .filter(|line| !line.ends_with(&asset))
            .map(|line| format!("{line}\n"))
            .collect();
        assert!(
            sums.lines().count() == SHA256SUMS.lines().count() - 1,
            "the fixture lost more than {asset}"
        );

        let output = generate(&format!("missing-{target}"), SEED_VERSION, &sums);
        assert!(
            !output.status.success(),
            "{target} vanished and the generator still wrote a formula: {}",
            describe(&output)
        );
        assert!(
            output.stdout.is_empty(),
            "a half-written formula reached stdout: {}",
            describe(&output)
        );
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(
            message.contains(&asset) && message.contains("no entry"),
            "the failure does not name the missing archive: {message}"
        );
    }
}

/// The other way a checksum file goes wrong: an entry that is there but is not
/// a sha256 (a truncated line, a `SHA256 (file) = …` BSD-format file, a
/// reformatted paste). Homebrew would only report it as a mismatch, at install
/// time, on every platform at once.
#[test]
fn the_generator_fails_loudly_on_a_checksum_that_is_not_a_sha256() {
    let asset = "vilan-x86_64-apple-darwin.tar.gz";
    let sums: String = SHA256SUMS
        .lines()
        .map(|line| {
            if line.ends_with(asset) {
                format!("491e6cc1  {asset}\n")
            } else {
                format!("{line}\n")
            }
        })
        .collect();

    let output = generate("truncated", SEED_VERSION, &sums);
    assert!(
        !output.status.success(),
        "a truncated checksum rendered fine: {}",
        describe(&output)
    );
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains(asset) && message.contains("is not a sha256"),
        "the failure does not say what is wrong: {message}"
    );
}

/// And the path itself: the workflow hands the generator a file it downloaded,
/// so a rename or a failed download must stop the job rather than produce a
/// formula with four missing checksums.
#[test]
fn the_generator_refuses_a_checksum_file_that_is_not_there() {
    let output = Command::new(repo_root().join("scripts/brew-formula.sh"))
        .args([SEED_VERSION, "no-such-sha256sums.txt"])
        .output()
        .expect("run scripts/brew-formula.sh");
    assert!(!output.status.success(), "{}", describe(&output));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no such checksum file"),
        "{}",
        describe(&output)
    );
}
