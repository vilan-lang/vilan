//! One canonical form for document URLs (`windows-support.md` §7).
//!
//! The server keys per-file state on `Url`s that reach it from two directions:
//! the client sends the URI it minted for a buffer, and the server mints its own
//! from the compiler's `PathBuf`s. On Windows those two are *different strings
//! for one file* — VS Code percent-encodes the drive colon (`file:///c%3A/…`)
//! while `Url::from_file_path` writes it plainly and upper-cases the letter
//! (`file:///C:/…`) — so a raw-`Url` map holds two entries for one file and
//! neither ever sees the other's updates. The class cannot occur on Linux, where
//! both directions serialize identically.
//!
//! [`normalize`] is the single seam that folds those spellings together. It is a
//! **comparison key**, never an address: what goes back on the wire is decided by
//! the caller (see `publish.rs`), because the client's own spelling is
//! authoritative for its buffers.

use tower_lsp::lsp_types::Url;

/// The canonical key form of a document URL: the same on-disk file always
/// produces the same `Url`, whichever way it was spelled.
///
/// Symlinks resolve too (`canonical_path` runs `fs::canonicalize`), which is
/// wanted here — two spellings of one on-disk file *should* share a key, so a
/// diagnostic published through one spelling is cleared through the other.
///
/// `windows` selects the host's drive-letter rule. It is a parameter rather than
/// a `cfg!` so both platforms' behavior is testable from either one (the
/// `util::home_dir_from` precedent); production passes `cfg!(windows)`.
pub fn normalize(url: &Url, windows: bool) -> Url {
    let folded = if windows {
        fold_drive_letter(url).unwrap_or_else(|| url.clone())
    } else {
        url.clone()
    };
    // Not a file URL we can map to a path — `untitled:` buffers, a `file://host/`
    // share the platform cannot express, a Windows drive letter on a unix build.
    // Pass it through: there is nothing to canonicalize, and inventing a key the
    // client never used is worse than keying on the client's own string.
    let Ok(path) = folded.to_file_path() else {
        return folded;
    };
    // Same policy when the canonical form cannot be spelled back as a URL:
    // `from_file_path` requires an absolute path, and `canonical_path` leaves a
    // relative one relative when it is not on disk to resolve.
    Url::from_file_path(vilan_core::util::canonical_path(&path)).unwrap_or(folded)
}

/// Fold a leading Windows drive-letter segment to the `/C:` form:
/// `file:///c%3A/x`, `file:///c:/x` and `file:///C:/x` are one file.
///
/// This happens on the URL rather than on the path because it has to work for a
/// file that is not on disk: `to_file_path` decodes `%3A` but keeps the letter's
/// case, and `fs::canonicalize` — which is what folds the case on Windows — only
/// speaks for paths that exist. `None` when the URL has no drive-letter segment,
/// which leaves it untouched.
fn fold_drive_letter(url: &Url) -> Option<Url> {
    let rest = url.path().strip_prefix('/')?;
    let (first, tail) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let letter = match first.as_bytes() {
        [letter, b':'] if letter.is_ascii_alphabetic() => *letter,
        [letter, b'%', b'3', b'a' | b'A'] if letter.is_ascii_alphabetic() => *letter,
        _ => return None,
    };
    let mut folded = url.clone();
    folded.set_path(&format!("/{}:{tail}", letter.to_ascii_uppercase() as char));
    Some(folded)
}

#[cfg(test)]
mod tests {
    use super::{fold_drive_letter, normalize};
    use tower_lsp::lsp_types::Url;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    // THE Windows key collision: what VS Code sends and what the server mints for
    // one file are different strings, and under the unix rule they stay different
    // (which is why the bug cannot be observed on Linux without this flag).
    #[test]
    fn the_two_windows_spellings_of_one_path_normalize_together() {
        let from_client = url("file:///c%3A/project/src/main.vl");
        let from_server = url("file:///C:/project/src/main.vl");
        assert_ne!(from_client, from_server, "the fixture must start apart");
        assert_eq!(
            normalize(&from_client, true),
            normalize(&from_server, true),
            "one file, one key"
        );
        assert_eq!(
            normalize(&from_server, true).as_str(),
            "file:///C:/project/src/main.vl",
            "the folded form is what `Url::from_file_path` writes on Windows"
        );
        assert_ne!(
            normalize(&from_client, false),
            normalize(&from_server, false),
            "the unix rule leaves them apart — the non-vacuity of the fold"
        );
    }

    // The lower-case plain form (`file:///c:/…`, which some clients send) folds
    // too, and a drive letter is recognized whether or not more path follows.
    #[test]
    fn a_drive_letter_folds_in_every_spelling() {
        for spelling in [
            "file:///c:/x/y.vl",
            "file:///C:/x/y.vl",
            "file:///c%3A/x/y.vl",
            "file:///c%3a/x/y.vl",
        ] {
            assert_eq!(
                fold_drive_letter(&url(spelling)).unwrap().as_str(),
                "file:///C:/x/y.vl",
                "{spelling}"
            );
        }
        assert_eq!(
            fold_drive_letter(&url("file:///d%3A")).unwrap().as_str(),
            "file:///D:",
        );
        // The rest of the path is left exactly as the client encoded it — the
        // fold is about the drive letter and nothing else.
        assert_eq!(
            fold_drive_letter(&url("file:///c%3A/a%20b/y.vl"))
                .unwrap()
                .as_str(),
            "file:///C:/a%20b/y.vl",
        );
    }

    // A unix path is never mistaken for a drive letter, on either rule.
    #[test]
    fn a_unix_path_is_left_alone() {
        for spelling in [
            "file:///workspace/dev/x.vl",
            "file:///c/x.vl",
            "file:///cc:/x.vl",
            "file:///1:/x.vl",
        ] {
            assert!(fold_drive_letter(&url(spelling)).is_none(), "{spelling}");
        }
    }

    // Percent-encoding that is NOT a drive letter still folds, on every platform:
    // the round trip decodes it and `from_file_path` re-encodes canonically. This
    // is the same class as the `%3A` bug and is observable on Linux.
    #[test]
    fn percent_encoding_folds_through_the_round_trip() {
        let encoded = url("file:///srv/dev/%73olo.vl");
        assert_eq!(
            normalize(&encoded, false).as_str(),
            "file:///srv/dev/solo.vl"
        );
    }

    // The failure arms pass through untouched: there is no path to canonicalize,
    // so the client's own string stays the key.
    #[test]
    fn a_url_without_a_path_passes_through() {
        for spelling in [
            "untitled:Untitled-1",
            "vilan:builtin/std",
            "https://example.invalid/x.vl",
        ] {
            let original = url(spelling);
            assert_eq!(normalize(&original, false), original, "{spelling}");
            assert_eq!(normalize(&original, true), original, "{spelling}");
        }
    }

    // Normalization is idempotent — a key that is fed back through the seam (an
    // already-normalized target re-entering the planner) must not drift.
    #[test]
    fn normalization_is_idempotent() {
        for spelling in [
            "file:///srv/dev/%73olo.vl",
            "file:///c%3A/x/y.vl",
            "untitled:Untitled-1",
        ] {
            let once = normalize(&url(spelling), true);
            assert_eq!(normalize(&once, true), once, "{spelling}");
        }
    }
}
