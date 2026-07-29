//! Publication hygiene: no tracked file may contain an absolute
//! home-directory path, nor the project's pre-migration owner strings.
//! Build artifacts and pasted terminal output are how development machine
//! paths leak into public repos; anything path-shaped that must live in the
//! tree should be relative (and everything that needs an absolute path
//! derives it at runtime or via CARGO_MANIFEST_DIR).
//!
//! The home-path check is deliberately generic — any absolute path under the
//! Linux, macOS, or Windows user-profile roots — so it is safe to publish and
//! independent of any particular machine or username. (The needles are
//! assembled at runtime so this file doesn't trip itself; the owner-string
//! check below does the same.)

use std::path::PathBuf;
use std::process::Command;

/// Every tracked text file, as `(repo-relative name, contents)`. Binaries and
/// deleted-but-staged entries are skipped; every leakable string in this repo
/// lives in text.
fn tracked_text_files() -> Vec<(String, String)> {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let listing = Command::new("git")
        .args(["ls-files", "-z"])
        .current_dir(&repo_root)
        .output()
        .expect("git ls-files");
    assert!(listing.status.success(), "git ls-files failed");
    let names = String::from_utf8_lossy(&listing.stdout);

    let mut files = Vec::new();
    for name in names.split('\0').filter(|name| !name.is_empty()) {
        let Ok(bytes) = std::fs::read(repo_root.join(name)) else {
            continue; // deleted-but-staged etc.
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue; // binary
        };
        files.push((name.to_string(), text));
    }
    files
}

#[test]
fn no_tracked_file_contains_an_absolute_home_path() {
    let needles = [
        format!("/{}/", "home"),
        format!("/{}/", "Users"),
        format!("C:\\{}\\", "Users"),
    ];
    let mut offenders = Vec::new();
    for (name, text) in tracked_text_files() {
        for (index, line) in text.lines().enumerate() {
            if needles.iter().any(|needle| line.contains(needle.as_str())) {
                offenders.push(format!("{name}:{}: {}", index + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "absolute home-directory paths in tracked files (use relative paths):\n{}",
        offenders.join("\n")
    );
}

/// Files that legitimately carry a consumer-mailbox address.
const PERSONAL_MAILBOX_ALLOWLIST: &[(&str, &str)] = &[(
    "THIRD-PARTY-NOTICES.txt",
    "generated from upstream license headers — those addresses are the \
     dependency authors' and are not ours to rewrite",
)];

/// No tracked file may publish a personal mailbox as a project contact.
///
/// Separate from the owner-string gate below on purpose: that one protects a
/// mechanical invariant (never reuse the old repository name, or the
/// `releases/download/…` redirects keeping installed binaries' `vilan upgrade`
/// alive die with it). This one is about the project speaking with an
/// organizational voice — a contact address in a public repo is scraped, it
/// cannot be rotated without a commit, and it does not survive a second
/// maintainer. Role addresses on `vilan-lang.org` do all three better.
///
/// Deliberately matched by consumer-mail DOMAIN rather than by the one address
/// that prompted this, so the next one is caught too — the failure mode here is
/// a new file, not a regression in an old one. A role address at the project's
/// own domain passes; that is the point.
///
/// (Needles assembled at runtime so this file doesn't trip itself.)
#[test]
fn no_tracked_file_publishes_a_personal_mailbox() {
    let needles = ["gmail", "outlook", "hotmail", "yahoo", "icloud", "proton"]
        .map(|provider| format!("@{provider}."));
    let mut offenders = Vec::new();
    for (name, text) in tracked_text_files() {
        if PERSONAL_MAILBOX_ALLOWLIST
            .iter()
            .any(|(allowed, _reason)| *allowed == name.as_str())
        {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            let lowered = line.to_lowercase();
            if needles.iter().any(|needle| lowered.contains(needle.as_str())) {
                offenders.push(format!("{name}:{}: {}", index + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "personal mailboxes in tracked files (use a role address on \
         vilan-lang.org, e.g. conduct@vilan-lang.org):\n{}",
        offenders.join("\n")
    );
}

/// Documents *about* the migration, which necessarily name the old owner.
/// Everything else must have been swept (F9 S4).
const OWNER_STRING_ALLOWLIST: &[(&str, &str)] = &[
    (
        "vilan/proposal/org-migration.md",
        "the migration plan itself — the old owner is its subject",
    ),
    (
        "vilan/proposal/backlog-2026-07-18.md",
        "the F9 backlog entry states the problem in terms of the old owner",
    ),
    (
        "vilan/proposal/releases.md",
        "release history quotes the install one-liner as it was published",
    ),
];

/// The repository moved from the maintainer's personal account to the
/// `vilan-lang` org, and the book with it — it now publishes at
/// `vilan-lang.org/docs` (F9, `vilan/proposal/org-migration.md`).
/// The invariant behind this gate: **the old GitHub repository name is never
/// reused.** A transfer leaves permanent redirects for git operations *and*
/// `releases/download/…` URLs, which is the only thing keeping every
/// already-installed binary's `vilan upgrade` alive — their baked-in base URL
/// points at the old name forever, and re-creating a repository under that
/// name would kill those redirects instantly. Pages does not redirect at all,
/// so the old book URL is served by a separate tombstone user-site instead.
/// New code must therefore carry the *new* strings: this test fails the build
/// if an old one comes back.
///
/// (Needles are assembled at runtime so this file doesn't trip itself, and
/// matched case-insensitively so no spelling of the old owner slips through.)
#[test]
fn no_tracked_file_contains_a_pre_migration_owner_string() {
    let needles = [
        format!("{}/{}", "reedsyllas", "vilan"),
        format!("{}.{}", "reedsyllas", "github.io"),
    ];
    let mut offenders = Vec::new();
    for (name, text) in tracked_text_files() {
        if OWNER_STRING_ALLOWLIST
            .iter()
            .any(|(allowed, _reason)| *allowed == name.as_str())
        {
            continue;
        }
        for (index, line) in text.lines().enumerate() {
            let lowered = line.to_lowercase();
            if needles
                .iter()
                .any(|needle| lowered.contains(needle.as_str()))
            {
                offenders.push(format!("{name}:{}: {}", index + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "pre-migration owner strings in tracked files (the project lives at \
         vilan-lang/vilan and the book at vilan-lang.org/docs):\n{}",
        offenders.join("\n")
    );
}
