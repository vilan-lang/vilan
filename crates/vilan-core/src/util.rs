pub fn plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 { singular } else { plural }.to_string()
}

use std::borrow::Cow;
use std::cell::Cell;
use std::path::{Component, Path, PathBuf};

/// The byte-order mark, U+FEFF, as UTF-8 — what a Windows editor writes at the
/// head of a "UTF-8 with BOM" file.
const BYTE_ORDER_MARK: &str = "\u{feff}";

/// Drops a LEADING byte-order mark: it is an encoding marker, not source text
/// (`windows-support.md` §2), so the lexer must never see it. Only at offset 0 —
/// an interior U+FEFF is content and stays.
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix(BYTE_ORDER_MARK).unwrap_or(text)
}

/// Reads a source file and drops its leading BOM, so every reader — the module
/// loader, the CLI, the language server's disk reads — indexes the same text.
/// Line endings are NOT touched here: spans must keep addressing the file as it
/// sits on disk (an editor's line index is built from the same bytes), and the
/// `\r\n`-is-one-terminator rule applies where a *value* is built, not to the
/// span space.
pub fn read_source(path: impl AsRef<Path>) -> std::io::Result<String> {
    let contents = std::fs::read_to_string(path)?;
    match contents.strip_prefix(BYTE_ORDER_MARK) {
        Some(stripped) => Ok(stripped.to_string()),
        None => Ok(contents),
    }
}

/// Rewrites `\r\n` to `\n`: a CRLF is ONE line terminator (`windows-support.md`
/// §2), so text built from source carries `\n` per source line break whatever
/// the file's on-disk encoding. A LONE `\r` is not a line terminator we bless —
/// it is left exactly as it is. Borrows unchanged when there is no `\r` at all,
/// so an all-LF file (every file in the tree) pays one scan and no allocation.
pub fn normalize_newlines(text: &str) -> Cow<'_, str> {
    if !text.contains('\r') {
        return Cow::Borrowed(text);
    }
    Cow::Owned(text.replace("\r\n", "\n"))
}

/// Windows' verbatim (`\\?\`) path prefix — what `fs::canonicalize` returns on
/// Windows, and what nothing else in a build ever produces.
const VERBATIM_PREFIX: &str = r"\\?\";

/// Rewrites a Windows verbatim path (`\\?\C:\src`, `\\?\UNC\host\share\src`) to
/// its ordinary spelling (`C:\src`, `\\host\share\src`), so a canonicalized path
/// compares with a join-built one (`windows-support.md` §5). Anything that is not
/// a verbatim path — every path on unix — is returned unchanged.
///
/// A verbatim device path (`\\?\Volume{…}`) has no ordinary spelling and stays
/// verbatim. The strip is UNCONDITIONAL for the two forms it does handle: the
/// result is a comparison key, never a path we reopen, so the "is it still
/// openable" caveats that make `dunce` keep some verbatim paths as they are
/// would only reintroduce the mixed-form mismatch this exists to kill.
fn strip_verbatim_prefix(path: &str) -> Cow<'_, str> {
    let Some(rest) = path.strip_prefix(VERBATIM_PREFIX) else {
        return Cow::Borrowed(path);
    };
    if let Some(share) = rest.strip_prefix(r"UNC\") {
        return Cow::Owned(format!(r"\\{share}"));
    }
    // A drive-letter verbatim path: `\\?\C:` or `\\?\C:\…`.
    let mut characters = rest.chars();
    let drive = characters.next();
    let colon = characters.next();
    let separator = characters.next();
    if drive.is_some_and(|drive| drive.is_ascii_alphabetic())
        && colon == Some(':')
        && matches!(separator, None | Some('\\'))
    {
        return Cow::Borrowed(rest);
    }
    Cow::Borrowed(path)
}

/// Folds away the components that make two spellings of one path compare
/// unequal: a `.` anywhere, and a `..` that has a real component to cancel.
/// Purely lexical — this is the arm for a path that is NOT on disk, where
/// there is nothing to resolve. A leading `..` (nothing to pop) is kept, and an
/// empty result becomes `.` so it stays a path.
fn normalize_components(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            // `Path::components` already folds an interior `.`; a LEADING one
            // survives, and dropping it is what makes `./a` and `a` agree.
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(
                    normalized.components().next_back(),
                    Some(Component::Normal(_))
                ) {
                    normalized.pop();
                } else {
                    normalized.push("..");
                }
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    if normalized.as_os_str().is_empty() {
        normalized.push(".");
    }
    normalized
}

/// THE canonical form of a path, for comparing two paths and for keying a map
/// by one (`windows-support.md` §5). One helper, so every such comparison is
/// like with like:
///
/// - On disk: `fs::canonicalize`, with Windows' `\\?\` verbatim prefix stripped
///   — otherwise a canonicalized library root never `starts_with`-matches a
///   join-built source path, and mixed keys duplicate a map entry.
/// - Not on disk (an unsaved buffer, a dependency directory that does not
///   exist): the components are normalized instead, so `a/./b` and `a/b` are
///   one key rather than two raw strings that happen to differ.
///
/// The result is for comparison, not for reopening: it is not guaranteed to be
/// the longest-path-safe spelling, and it is never shown to the user (the
/// original path is what diagnostics print).
pub fn canonical_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let Ok(canonical) = std::fs::canonicalize(path) else {
        return normalize_components(path);
    };
    // A path Windows cannot spell in UTF-8 (an unpaired surrogate) keeps its
    // verbatim form: it is still a consistent key, just a longer one.
    match canonical.to_str() {
        Some(text) => match strip_verbatim_prefix(text) {
            Cow::Borrowed(stripped) if stripped.len() == text.len() => canonical,
            stripped => PathBuf::from(stripped.into_owned()),
        },
        None => canonical,
    }
}

/// Verifies that every component of `relative` names an on-disk entry under
/// `root` **byte-for-byte**, and reports the first that does not as
/// `(requested, on-disk)`.
///
/// A case-insensitive filesystem (NTFS, and APFS by default) answers
/// `Path::exists` for `foo.vl` when the file on disk is `Foo.vl`, so `import
/// foo` resolves there and the same program fails on Linux — a program that
/// builds on one machine and not another, which is exactly what the
/// platform-independence invariant forbids (`windows-support.md` §5, ratified
/// call (c)). Enforcing exact case is the general fix; this is the check.
///
/// It lives here rather than beside the module loader because nothing about it
/// is specific to an `import`: the CLI runs the same check on the ENTRY file's
/// spelling (`main.rs::entry_case_mismatch` — `windows-support.md` §12's
/// residual), where the "requested" name comes from the command line or a
/// manifest instead of an import statement.
///
/// `None` when the names agree, or when a directory cannot be read — an
/// unreadable directory is a different failure, not this check's to report.
///
/// The `read_dir` is deliberately **not cached, and must not be**: a memoized
/// directory listing goes stale the moment a file is renamed, and a long-lived
/// process (`run --watch`, the language server) would then invent a case
/// mismatch for a file that is now spelled correctly — inventing a diagnostic
/// is far worse than the cost. Measured on `examples/walkthrough`: 179 calls
/// per build, each one `read_dir` of a ~50-entry directory, median build 1986 ms
/// with the check against 1980 ms without — inside the run-to-run noise.
pub fn case_exact_mismatch(root: &Path, relative: &Path) -> Option<(String, String)> {
    let mut directory = root.to_path_buf();
    for component in relative.components() {
        let requested = component.as_os_str();
        let entries = std::fs::read_dir(&directory).ok()?;
        let mut on_disk: Option<std::ffi::OsString> = None;
        let mut exact = false;
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name == requested {
                exact = true;
                break;
            }
            if name
                .to_string_lossy()
                .eq_ignore_ascii_case(&requested.to_string_lossy())
            {
                on_disk = Some(name);
            }
        }
        if !exact {
            let on_disk = on_disk?;
            return Some((
                requested.to_string_lossy().into_owned(),
                on_disk.to_string_lossy().into_owned(),
            ));
        }
        directory.push(requested);
    }
    None
}

thread_local! {
    static RECURSION_DEPTH: Cell<usize> = const { Cell::new(0) };
}

/// A safety net for the recursive type operations (`reconcile_type`,
/// `substitute_type`, the transformer's `resolve_type_id`). A self-mapping or
/// otherwise pathological generic graph that slips past the explicit guards must
/// degrade to a graceful bail rather than overflow the stack — a compiler should
/// never crash on user input. The limit is far above any real type's nesting.
pub struct RecursionGuard;

impl RecursionGuard {
    /// Enters one level of recursion; `None` once the depth limit is reached, so
    /// the caller can return a graceful fallback instead of recursing.
    pub fn enter() -> Option<RecursionGuard> {
        RECURSION_DEPTH.with(|depth| {
            let current = depth.get();
            if current >= 2048 {
                None
            } else {
                depth.set(current + 1);
                Some(RecursionGuard)
            }
        })
    }
}

impl Drop for RecursionGuard {
    fn drop(&mut self) {
        RECURSION_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// The validated shape of a triple-quoted literal's raw inner text: the byte
/// range of its content lines within `raw`, and the indentation prefix every one
/// of them starts with. See [`multiline_layout`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultilineLayout<'src> {
    /// The content lines, `first newline + 1 .. last newline` — empty when the
    /// literal has no content lines at all.
    pub content: std::ops::Range<usize>,
    /// The whitespace preceding the closing `"""`, stripped from every content
    /// line.
    pub prefix: &'src str,
}

/// Validates a triple-quoted string literal's raw inner text — everything between
/// the `"""` delimiters — and returns its layout (backlog H4; Swift's multiline
/// rule):
///
/// - The opening `"""` is followed by a newline (after optional whitespace):
///   content starts on the next line.
/// - The closing `"""` sits alone on its line; the whitespace before it is the
///   INDENTATION PREFIX, stripped from every content line.
/// - A content line must start with that exact prefix (the same characters, so
///   a tab never satisfies a space prefix) — unless it is whitespace-only, in
///   which case it may be shorter and becomes empty.
/// - The newlines adjoining the delimiters belong to the syntax, not the
///   string.
///
/// This is the ONE implementation of the multiline shape rule: the plain form
/// trims through [`trim_multiline_string`], and the interpolated form
/// (`i"""…"""`, backlog H7) fragments through the same layout in the lexer, so
/// the two can never drift. Holes are ordinary characters here — the rule is
/// defined on the literal's RAW text, before any fragmentation.
///
/// An error carries the offending byte range RELATIVE TO `raw`, so the caller
/// can span the diagnostic at the exact offender rather than the whole literal.
pub fn multiline_layout(
    raw: &str,
) -> Result<MultilineLayout<'_>, (String, std::ops::Range<usize>)> {
    let Some(first_newline) = raw.find('\n') else {
        return Err((
            "a triple-quoted string spans lines: the opening \"\"\" must be followed by a newline"
                .to_string(),
            0..raw.len(),
        ));
    };
    let opener_rest = raw[..first_newline].trim_end_matches('\r');
    if !opener_rest.trim().is_empty() {
        let start = opener_rest.len() - opener_rest.trim_start().len();
        return Err((
            format!(
                "nothing may follow the opening \"\"\" on its line (found `{}`)",
                opener_rest.trim()
            ),
            start..opener_rest.trim_end().len(),
        ));
    }
    let last_newline = raw.rfind('\n').expect("found above");
    let prefix = &raw[last_newline + 1..];
    if !prefix.chars().all(|c| c == ' ' || c == '\t') {
        return Err((
            "the closing \"\"\" must sit alone on its line, preceded only by indentation"
                .to_string(),
            last_newline + 1..raw.len(),
        ));
    }
    if first_newline == last_newline {
        // `"""` directly followed by the closing line: zero content lines.
        return Ok(MultilineLayout {
            content: first_newline..first_newline,
            prefix,
        });
    }
    let content = first_newline + 1..last_newline;
    let body = &raw[content.clone()];
    let mut line_start = content.start;
    for (index, line) in body.split('\n').enumerate() {
        let raw_line_length = line.len();
        let line = line.strip_suffix('\r').unwrap_or(line);
        if !line.starts_with(prefix) && !line.chars().all(|c| c == ' ' || c == '\t') {
            // A whitespace-only line may fall short of the prefix; anything else
            // must carry it.
            return Err((
                format!(
                    "line {} of the triple-quoted string is not indented to its closing \"\"\" \
                     (every line must start with the whitespace that precedes the closing delimiter)",
                    index + 1
                ),
                line_start..line_start + line.len(),
            ));
        }
        line_start += raw_line_length + 1;
    }
    Ok(MultilineLayout { content, prefix })
}

/// Trims a triple-quoted string literal's raw inner text to its value: the
/// content lines of [`multiline_layout`] with the indentation prefix stripped,
/// joined by `\n`. The body is RAW — no escape processing at all (the appeal is
/// pasting code verbatim), so `\n` is a backslash and an `n` — and a `\r` before
/// any line-ending `\n` is dropped (CRLF tolerance).
pub fn trim_multiline_string(raw: &str) -> Result<String, (String, std::ops::Range<usize>)> {
    let layout = multiline_layout(raw)?;
    if layout.content.is_empty() {
        return Ok(String::new());
    }
    let content_lines = raw[layout.content]
        .split('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            // Validated above: a line either carries the prefix or is
            // whitespace-only (and then contributes nothing).
            line.strip_prefix(layout.prefix).unwrap_or("")
        })
        .collect::<Vec<_>>();
    Ok(content_lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_path, normalize_components, normalize_newlines, strip_bom, strip_verbatim_prefix,
        trim_multiline_string,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn a_verbatim_drive_path_loses_its_prefix() {
        // `fs::canonicalize` on Windows returns this form; a join-built path
        // never does, so the two only meet once the prefix is gone.
        assert_eq!(
            strip_verbatim_prefix(r"\\?\C:\src\lib.vl"),
            r"C:\src\lib.vl"
        );
        assert_eq!(strip_verbatim_prefix(r"\\?\c:\"), r"c:\");
        assert_eq!(strip_verbatim_prefix(r"\\?\C:"), r"C:");
    }

    #[test]
    fn a_verbatim_unc_path_becomes_its_ordinary_share_spelling() {
        assert_eq!(
            strip_verbatim_prefix(r"\\?\UNC\host\share\src\lib.vl"),
            r"\\host\share\src\lib.vl"
        );
    }

    #[test]
    fn a_non_verbatim_or_unspellable_path_is_untouched() {
        // Ordinary paths (every path on unix) pass through byte-for-byte…
        assert_eq!(strip_verbatim_prefix("/srv/app/lib.vl"), "/srv/app/lib.vl");
        assert_eq!(strip_verbatim_prefix(r"C:\src\lib.vl"), r"C:\src\lib.vl");
        // …and a device path has no ordinary spelling, so it keeps the prefix.
        assert_eq!(
            strip_verbatim_prefix(r"\\?\Volume{deadbeef}\src"),
            r"\\?\Volume{deadbeef}\src"
        );
        // `\\?\pipe\name` is not a drive letter either.
        assert_eq!(strip_verbatim_prefix(r"\\?\pipe\vilan"), r"\\?\pipe\vilan");
    }

    #[test]
    fn components_normalize_away_dot_and_parent() {
        assert_eq!(
            normalize_components(Path::new("a/./b")),
            PathBuf::from("a/b")
        );
        assert_eq!(
            normalize_components(Path::new("./a/b")),
            PathBuf::from("a/b")
        );
        assert_eq!(
            normalize_components(Path::new("a/c/../b")),
            PathBuf::from("a/b")
        );
        // Nothing to cancel: a leading `..` is kept rather than silently eaten.
        assert_eq!(
            normalize_components(Path::new("../a")),
            PathBuf::from("../a")
        );
        // An all-`.` path stays a path.
        assert_eq!(normalize_components(Path::new("./")), PathBuf::from("."));
    }

    #[test]
    fn a_path_not_on_disk_still_has_one_canonical_form() {
        // The fallback arm — the one that used to compare raw strings, so an
        // unsaved buffer registered as `a/./b` was invisible to a lookup of
        // `a/b`. Neither spelling exists on disk here.
        let base = std::env::temp_dir().join(format!(
            "vilan-canonical-missing-{}-{}",
            std::process::id(),
            line!()
        ));
        let spelled = base.join("pkg/./src/../src/main.vl");
        let plain = base.join("pkg/src/main.vl");
        assert!(!plain.exists(), "the probe path must not be on disk");
        assert_eq!(canonical_path(&spelled), canonical_path(&plain));
    }

    #[test]
    fn a_path_on_disk_canonicalizes_through_its_spellings() {
        let directory = std::env::temp_dir().join(format!(
            "vilan-canonical-real-{}-{}",
            std::process::id(),
            line!()
        ));
        let nested = directory.join("src");
        std::fs::create_dir_all(&nested).expect("create the probe directory");
        let file = nested.join("main.vl");
        std::fs::write(&file, "fun main() {}\n").expect("write the probe file");
        let round_about = directory.join("src/../src/./main.vl");
        assert_eq!(canonical_path(&round_about), canonical_path(&file));
        // And the canonical form of a real path is absolute and prefix-free.
        let canonical = canonical_path(&file);
        assert!(canonical.is_absolute(), "{}", canonical.display());
        assert!(
            !canonical.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            canonical.display()
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_leading_byte_order_mark_is_dropped() {
        assert_eq!(
            strip_bom("\u{feff}import std::print;"),
            "import std::print;"
        );
    }

    #[test]
    fn a_source_without_a_byte_order_mark_is_untouched() {
        assert_eq!(strip_bom("import std::print;"), "import std::print;");
        // Byte-for-byte the same slice, not a re-derived equal one.
        let text = "fun main() {}";
        assert!(std::ptr::eq(strip_bom(text), text));
    }

    #[test]
    fn an_interior_byte_order_mark_is_content() {
        // Only offset 0 is an encoding marker; a U+FEFF inside a string
        // literal is a character the program means to carry.
        assert_eq!(strip_bom("let a = \"\u{feff}\";"), "let a = \"\u{feff}\";");
    }

    #[test]
    fn crlf_becomes_one_line_terminator() {
        assert_eq!(normalize_newlines("a\r\nb\r\n"), "a\nb\n");
    }

    #[test]
    fn a_lone_carriage_return_is_left_alone() {
        // Classic-Mac line endings are not blessed (windows-support.md §2):
        // a `\r` with no following `\n` stays exactly what it is.
        assert_eq!(normalize_newlines("a\rb"), "a\rb");
        // …including one directly before a CRLF, where only the pair folds.
        assert_eq!(normalize_newlines("a\r\r\nb"), "a\r\nb");
    }

    #[test]
    fn an_lf_only_text_is_borrowed_unchanged() {
        let text = "a\nb\n";
        match normalize_newlines(text) {
            std::borrow::Cow::Borrowed(borrowed) => assert!(std::ptr::eq(borrowed, text)),
            std::borrow::Cow::Owned(_) => panic!("an all-LF text must not allocate"),
        }
    }

    #[test]
    fn trims_each_line_by_the_closing_indentation() {
        // The motivating example (H4): opener/closer indented 4 spaces.
        let raw = "\n        line 1\n    line 2\n\n      line 3\n        \n    ";
        assert_eq!(
            trim_multiline_string(raw).unwrap(),
            "    line 1\nline 2\n\n  line 3\n    "
        );
    }

    #[test]
    fn a_tab_prefix_strips_tabs() {
        let raw = "\n\t\thello\n\tworld\n\t";
        assert_eq!(trim_multiline_string(raw).unwrap(), "\thello\nworld");
    }

    #[test]
    fn a_column_zero_closer_strips_nothing() {
        let raw = "\n  a\nb\n";
        assert_eq!(trim_multiline_string(raw).unwrap(), "  a\nb");
    }

    #[test]
    fn a_short_whitespace_only_line_becomes_empty() {
        let raw = "\n    a\n  \n    b\n    ";
        assert_eq!(trim_multiline_string(raw).unwrap(), "a\n\nb");
    }

    #[test]
    fn zero_content_lines_is_the_empty_string() {
        assert_eq!(trim_multiline_string("\n    ").unwrap(), "");
        assert_eq!(trim_multiline_string("\n").unwrap(), "");
    }

    #[test]
    fn crlf_line_endings_are_tolerated() {
        let raw = "\r\n    a\r\n    b\r\n    ";
        assert_eq!(trim_multiline_string(raw).unwrap(), "a\nb");
    }

    #[test]
    fn trailing_whitespace_after_the_prefix_is_kept() {
        let raw = "\n    a   \n    ";
        assert_eq!(trim_multiline_string(raw).unwrap(), "a   ");
    }

    #[test]
    fn no_newline_at_all_is_an_error() {
        let (error, _) = trim_multiline_string("one line").unwrap_err();
        assert!(error.contains("followed by a newline"), "{error}");
    }

    #[test]
    fn content_after_the_opener_is_an_error() {
        let (error, range) = trim_multiline_string("oops\n    a\n    ").unwrap_err();
        assert!(error.contains("nothing may follow the opening"), "{error}");
        assert!(error.contains("oops"), "{error}");
        assert_eq!(range, 0..4, "the range covers the offending text");
    }

    #[test]
    fn content_before_the_closer_is_an_error() {
        let (error, _) = trim_multiline_string("\n    a\n    b: ").unwrap_err();
        assert!(error.contains("alone on its line"), "{error}");
    }

    #[test]
    fn insufficient_indentation_is_an_error_naming_the_line() {
        let (error, range) = trim_multiline_string("\n    a\n  b\n    ").unwrap_err();
        assert!(error.contains("line 2"), "{error}");
        assert!(error.contains("not indented"), "{error}");
        // raw = "\n    a\n  b\n    ": line 2 ("  b") starts at byte 7.
        assert_eq!(range, 7..10, "the range covers the offending line");
    }

    #[test]
    fn a_tab_never_satisfies_a_space_prefix() {
        let (error, _) = trim_multiline_string("\n\ta\n    ").unwrap_err();
        assert!(error.contains("line 1"), "{error}");
    }
}
