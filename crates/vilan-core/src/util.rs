pub fn plural(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 { singular } else { plural }.to_string()
}

/// Joins items into an English phrase — `a`, `a and b`, `a, b and c` — with
/// `conjunction` between the last two. For a diagnostic that has to name
/// several candidates in prose rather than as a list.
pub fn join_with(items: &[String], conjunction: &str) -> String {
    match items {
        [] => String::new(),
        [only] => only.clone(),
        [first, last] => format!("{first} {conjunction} {last}"),
        [rest @ .., last] => format!("{} {conjunction} {last}", rest.join(", ")),
    }
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
///
/// **The open-document overlay wins over disk.** A registered buffer is the
/// file's current truth: reading past it gave the analyzer one text and every
/// span consumer another, so a diagnostic in an edited-but-unsaved module
/// landed at the wrong line. Consulting it here puts every reader on one text,
/// which is what the first paragraph above already promised. A buffer also
/// satisfies a read for a path that is not on disk at all, which is what lets a
/// module exist only in the editor — or, with no filesystem behind it, at all.
///
/// Buffered text is returned EXACTLY as the client sent it, with no BOM strip.
/// That asymmetry is deliberate and predates this: the client's own line index
/// is authoritative for its buffers, and VS Code already strips the BOM over
/// the wire, so stripping again here would shift every span by three bytes.
pub fn read_source(path: impl AsRef<Path>) -> std::io::Result<String> {
    read_source_traced(path).map(|(contents, _)| contents)
}

/// Where [`read_source_traced`] found the content: the open-document overlay
/// (an editor buffer, served verbatim) or the disk (BOM-stripped). The module
/// loader is the caller that cares (M9, `leak-soak.md` §7.9.4): an
/// overlay-served module during an opted-in analysis is parsed into
/// analysis-owned allocations, and reading the provenance off the one
/// overlay-then-disk seam keeps that decision from paying a second overlay
/// probe (`canonical_path` canonicalizes on the filesystem).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SourceProvenance {
    Overlay,
    Disk,
}

thread_local! {
    /// How many times THIS THREAD has materialized a source text through
    /// [`read_source_traced`] — overlay and disk alike, each one a fresh
    /// `String` of the whole file. What made a bare scope completion clone a
    /// std module's text once per doc-carrying candidate before E83
    /// (`Analysis::doc_comment_of`, `proposals/proposal/playground-completion.md`
    /// §9).
    static SOURCE_READS: Cell<u64> = const { Cell::new(0) };
}

/// The number of source texts this thread has materialized through
/// [`read_source_traced`] — an instrumentation probe (E83), not a behavior
/// surface. Monotonic: read a snapshot before and after the work under test
/// and assert on the difference. The pin that holds a completion request to
/// one read per module (not one per candidate) is what this exists for.
pub fn source_read_count() -> u64 {
    SOURCE_READS.with(Cell::get)
}

/// [`read_source`], reporting where the content came from.
pub fn read_source_traced(path: impl AsRef<Path>) -> std::io::Result<(String, SourceProvenance)> {
    SOURCE_READS.with(|count| count.set(count.get() + 1));
    let path = path.as_ref();
    if let Some(buffered) = crate::analyzer::document_overlay_get(path) {
        return Ok((buffered, SourceProvenance::Overlay));
    }
    let contents = std::fs::read_to_string(path)?;
    let contents = match contents.strip_prefix(BYTE_ORDER_MARK) {
        Some(stripped) => stripped.to_string(),
        None => contents,
    };
    Ok((contents, SourceProvenance::Disk))
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
    canonicalized(path.as_ref()).unwrap_or_else(|| normalize_components(path.as_ref()))
}

/// [`canonical_path`]'s on-disk arm alone: `None` where the path is not there.
fn canonicalized(path: &Path) -> Option<PathBuf> {
    let canonical = std::fs::canonicalize(path).ok()?;
    // A path Windows cannot spell in UTF-8 (an unpaired surrogate) keeps its
    // verbatim form: it is still a consistent key, just a longer one.
    Some(match canonical.to_str() {
        Some(text) => match strip_verbatim_prefix(text) {
            Cow::Borrowed(stripped) if stripped.len() == text.len() => canonical,
            stripped => PathBuf::from(stripped.into_owned()),
        },
        None => canonical,
    })
}

/// [`canonical_path`] for a path that **is not on disk yet** — a build product
/// before its generator has written it: the deepest ancestor that IS on disk is
/// canonicalized, and the components below it are re-attached exactly as they
/// were spelled.
///
/// The comparison key `canonical_path` yields is only like-with-like when both
/// sides resolved. When one did not, the two are a resolved spelling and a
/// spelled one, and every way a filesystem can give a path two spellings makes
/// them differ: a symlink anywhere in the ancestry (unix and Windows alike), and
/// on a case-insensitive filesystem the case of every component. Containment
/// then answers NO for a path that is plainly inside its root — B198's fail-open,
/// found on Windows against `gen` / `GEN` and reachable on unix through a link.
///
/// So this is the resolution a containment test uses when the subject may not
/// exist: **canonical-or-fail, never folded-against-lexical**. What cannot be
/// resolved is the tail, which is by definition the part no filesystem has an
/// opinion about yet — so the two sides of the comparison are again like with
/// like, and the answer for a tree where nothing at all is on disk degrades to
/// G17's spelled ladder (both sides lexical) rather than to a mixed comparison.
///
/// Not a replacement for [`canonical_path`], whose promise to every other caller
/// — "the path as the disk spells it, or the path as you spelled it" — is
/// unchanged. This one costs one `canonicalize` per missing ancestor, and for a
/// path that IS on disk it costs exactly what `canonical_path` costs: the first
/// attempt succeeds.
pub fn canonical_path_of_unwritten(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    let mut unwritten: Vec<&std::ffi::OsStr> = Vec::new();
    let mut ancestor = path;
    loop {
        if let Some(mut resolved) = canonicalized(ancestor) {
            for name in unwritten.iter().rev() {
                resolved.push(name);
            }
            return resolved;
        }
        // Nothing on this path exists: there is no anchor, so the whole thing
        // normalizes lexically, exactly as `canonical_path` would answer.
        let (Some(parent), Some(name)) = (ancestor.parent(), ancestor.file_name()) else {
            return normalize_components(path);
        };
        unwritten.push(name);
        ancestor = parent;
    }
}

/// The path as it was **spelled**, made absolute and lexically normalized —
/// symlinks deliberately left unresolved. [`canonical_path`]'s complement: same
/// folding of `.` and `..`, same comparability, but it answers *how did the
/// caller reach this file* rather than *what is this file really*.
///
/// A symlink gives one file two honest ancestries, and both are project layout
/// (`const.md` §9.2's symlink doctrine, G19): the tree it was reached through
/// and the tree it lives in. A rule that has to find the manifest ABOVE a file
/// needs the spelled ancestry — a package declaring `generated = "src/icons"`
/// over a link out of its own tree is never found by climbing the resolved path
/// — while every containment COMPARISON stays canonical, so the two paths that
/// name one file always answer alike. That pairing is what
/// `manifest::generated_root_covering` is built from.
///
/// A relative path is made absolute against the working directory, since an
/// ancestry is exactly what a relative path does not carry. When even that is
/// unreadable the path is normalized where it is, which is the same degradation
/// [`canonical_path`] takes.
pub fn spelled_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        return normalize_components(path);
    }
    match std::env::current_dir() {
        Ok(working_directory) => normalize_components(&working_directory.join(path)),
        Err(_) => normalize_components(path),
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
                // The guard already counts this family's depth; the B138
                // instrument only reads the high-water mark.
                crate::depth_stats::note(crate::depth_stats::TYPE_WALK, current + 1);
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
        canonical_path, canonical_path_of_unwritten, join_with, normalize_components,
        normalize_newlines, strip_bom, strip_verbatim_prefix, trim_multiline_string,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn a_candidate_list_reads_as_english() {
        let one = ["'A'".to_string()];
        let two = ["'A'".to_string(), "'B'".to_string()];
        let three = ["'A'".to_string(), "'B'".to_string(), "'C'".to_string()];
        assert_eq!(join_with(&one, "and"), "'A'");
        assert_eq!(join_with(&two, "and"), "'A' and 'B'");
        assert_eq!(join_with(&three, "or"), "'A', 'B' or 'C'");
        assert_eq!(join_with(&[], "and"), "");
    }

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

    /// [`strip_verbatim_prefix`]'s doc justifies stripping UNCONDITIONALLY on
    /// the grounds that "the result is a comparison key, never a path we
    /// reopen". Audit run 7 checked, and three callers reopen it:
    /// `TreeWalk::rooted_at` stats the canonical root (`is_dir()`,
    /// `main.rs` ~2039), `find_project_root` probes `vilan.toml` beside every
    /// ancestor of one (`main.rs` ~4069), and `generated_root_in` reads that
    /// file's bytes (`manifest.rs` ~807). So the caveat `dunce` exists to
    /// respect — that an ordinary spelling cannot always address what a verbatim
    /// one can — is live here rather than excluded by construction.
    ///
    /// It holds, and this pin is why it is allowed to: `std` re-applies the
    /// verbatim form itself on the way back in (`maybe_verbatim`), so a path
    /// past `MAX_PATH` survives the round trip through the stripped spelling.
    /// That is a property of the standard library, not of this module, which is
    /// exactly the kind of thing that changes underneath a comment. The pin
    /// turns the caveat into a gate, on the longest path the callers can hand
    /// back — `cfg(windows)` because `MAX_PATH` and the prefix are only there.
    #[cfg(windows)]
    #[test]
    fn a_stripped_canonical_path_is_still_openable() {
        let base = std::env::temp_dir().join(format!("vilan-longpath-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        // Comfortably past `MAX_PATH` (260): the verbatim prefix is the only
        // thing that lets Win32 address a path this long, so removing it is
        // precisely the risk under test.
        let mut deep = base.clone();
        for _ in 0..12 {
            deep.push("directory_with_a_name_long_enough_to_pass_max_path");
        }
        assert!(
            deep.as_os_str().len() > 260,
            "the probe must actually be a long path: {}",
            deep.display()
        );
        std::fs::create_dir_all(&deep).expect("create a path past MAX_PATH");
        std::fs::write(deep.join("vilan.toml"), "[package]\nname = \"app\"\n").unwrap();

        let key = canonical_path(&deep);
        assert!(
            !key.to_string_lossy().starts_with(r"\\?\"),
            "the prefix is gone — that is the behavior whose caveat this pins: {}",
            key.display()
        );
        // The three reopens, in the shapes their callers use.
        assert!(
            key.is_dir(),
            "a canonical root is stat'd by the walk that starts there: {}",
            key.display()
        );
        assert!(
            key.join("vilan.toml").is_file(),
            "and probed for a manifest at every level of the climb above it"
        );
        assert!(
            std::fs::read_to_string(key.join("vilan.toml")).is_ok(),
            "and that manifest is then READ, which is the reopen that decides \
             whether a generated root is found at all"
        );
        let _ = std::fs::remove_dir_all(&base);
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
    fn an_unwritten_path_resolves_through_the_deepest_ancestor_that_exists() {
        // B198. `canonical_path` answers a path not on disk with the caller's
        // own spelling, which is the right key and the wrong SIDE of a
        // containment test: the other side resolved. Here the directory is real
        // and reached through a link, so `canonical_path` and this one give
        // measurably different answers for the same missing file.
        let base = std::env::temp_dir().join(format!(
            "vilan-canonical-unwritten-{}-{}",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("real")).expect("create the probe directory");
        let root = canonical_path(&base);
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("real"), root.join("link"))
            .expect("link the probe directory");

        let unwritten = root.join("real/not_written_yet.vl");
        assert!(!unwritten.exists(), "the probe file must not be on disk");
        assert_eq!(
            canonical_path_of_unwritten(&unwritten),
            root.join("real/not_written_yet.vl"),
            "the tail is re-attached to its resolved ancestor"
        );
        #[cfg(unix)]
        assert_eq!(
            canonical_path_of_unwritten(root.join("link/not_written_yet.vl")),
            root.join("real/not_written_yet.vl"),
            "and the ancestor is RESOLVED, which is the whole difference from \
             `canonical_path` — it answers the link's own spelling here"
        );

        // Nothing BELOW the probe directory exists, so the whole tail is
        // re-attached to the deepest ancestor that does — resolved, and with the
        // tail's `.` folded. Windows spells its temp directory with an 8.3 short
        // name (`RUNNER~1`) that resolves to the long one, which is exactly the
        // difference this function exists for: `base` is the caller's spelling,
        // `root` is what is really there.
        let nowhere = base.join("absent/pkg/./src/main.vl");
        assert_eq!(
            canonical_path_of_unwritten(&nowhere),
            root.join("absent/pkg/src/main.vl"),
            "the unwritten tail rides the RESOLVED ancestor"
        );
        // From an ancestor that is already canonical there is nothing left to
        // resolve, so the two functions agree — on every platform.
        let nowhere_canonical = root.join("absent/pkg/./src/main.vl");
        assert_eq!(
            canonical_path_of_unwritten(&nowhere_canonical),
            canonical_path(&nowhere_canonical),
            "with a canonical anchor there is nothing to resolve, so the two agree"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_leading_byte_order_mark_is_dropped() {
        assert_eq!(
            strip_bom("\u{feff}import std::io::print;"),
            "import std::io::print;"
        );
    }

    #[test]
    fn a_source_without_a_byte_order_mark_is_untouched() {
        assert_eq!(
            strip_bom("import std::io::print;"),
            "import std::io::print;"
        );
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
