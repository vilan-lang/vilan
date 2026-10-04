//! `vilan check --fix` (`proposal/index-type.md` §8.1): the fixes the language
//! server offers one at a time, applied to a package in bulk, to a fixed
//! point — the numeric mismatches' conversions, and A154's moved std paths
//! (`std::dom` → `std::web::dom`, `prelude = "std::web"` →
//! `"std::web::prelude"`), the one-command migration across v0.44.0's rename.
//!
//! The edits are not computed here. `vilan_ide::numeric_fix` and
//! `vilan_core::parsing::moved_std_module_edit` are the functions both
//! surfaces call, so the command line can never write a different edit from
//! the one the editor offers for the same diagnostic. This file is the DRIVER
//! §8.1 describes: analyze, apply every fix the diagnostics carry, and repeat
//! until a round applies nothing — then hand over to the ordinary `check`,
//! which reports whatever needs a person (a `-1` sentinel, a `>= 0` loop,
//! signed arithmetic that must convert once at its end) exactly as it would
//! have.
//!
//! The moved paths are found from the ANALYSIS's refusals, not by a second,
//! syntactic reading of the import lines. The refusal is raised exactly where
//! an import stops resolving, which is the real migration case: a v0.43.0
//! package opened with v0.44.0 is refused at every old import, and those
//! refusals are all the analysis must produce — it need not succeed. Reading
//! them is what keeps the command and the editor on one answer: only the
//! analyzer knows which `std::web::X` names the old web prelude (`X` is a name
//! it exports) and which is an ordinary miss, and only it knows which files
//! the program reaches. A round that applies a moved path applies nothing
//! else: a numeric diagnostic read under unresolved imports is not one to act
//! on. The manifest's `prelude` is rewritten before anything is resolved,
//! since an old value is refused before any analysis can run.
//!
//! One-shot, like the rest of the CLI: it runs on the compiler thread, outside
//! the panic fence, and a compiler panic fails the command loudly.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use vilan_core::parsing::{StdPathFix, moved_std_module_edit, moved_std_module_of_message};
use vilan_core::{Platform, Span, Workspace};
use vilan_ide::numeric_fix::{NumericFix, apply_numeric_fixes, numeric_fixes};

use super::{Unit, find_project_root, format_options, paint, resolve_workspace, std_dir};

/// Rounds past which the driver stops rather than loop: every round must
/// apply at least one edit to continue, and a real migration settles in a
/// handful. A fix that kept re-creating its own diagnostic would otherwise
/// never end.
const ROUND_LIMIT: usize = 32;

/// What a `--fix` pass did to a project.
#[derive(Default)]
pub(crate) struct Fixed {
    /// Numeric mismatches converted, and the files they were in.
    pub(crate) edits: usize,
    pub(crate) files: BTreeSet<PathBuf>,
    /// Moved std paths rewritten, per file (a manifest's `prelude` among
    /// them).
    pub(crate) moved: BTreeMap<PathBuf, usize>,
    /// Moved std paths left in a file under its package's declared
    /// `generated` root — the project regenerates that file, so the edit
    /// belongs in whatever generates it: per file, the root and the refused
    /// sites' offsets.
    pub(crate) generated: BTreeMap<PathBuf, (PathBuf, BTreeSet<usize>)>,
    /// Moved std paths no one edit rewrites correctly, per file, by offset.
    pub(crate) for_a_hand: BTreeMap<PathBuf, BTreeMap<usize, ForAHand>>,
}

/// A moved std path `--fix` leaves to a person, and why.
pub(crate) struct ForAHand {
    line: usize,
    column: usize,
    reason: &'static str,
}

/// A154's manifest half, run before the project is resolved (an old `prelude`
/// is refused while the manifest is read, so nothing could be analyzed past
/// it): the `prelude` of the manifest the command addresses — `target`'s, by
/// the discovery `check` itself uses — and of every `[project] packages`
/// member it lists, rewritten when it names a moved std module. Only the value
/// is replaced; the file's comments and layout stay.
pub(crate) fn fix_manifests(target: Option<&Path>, fixed: &mut Fixed) -> Result<(), String> {
    let root = match target {
        Some(path) if path.is_dir() => Some(path.to_path_buf()),
        Some(path) => path.parent().and_then(find_project_root),
        None => std::env::current_dir()
            .ok()
            .and_then(|directory| find_project_root(&directory)),
    };
    let Some(root) = root else {
        return Ok(());
    };
    let Some(text) = fix_manifest(&root, fixed)? else {
        return Ok(());
    };
    // A workspace's members carry their own manifests. A lenient read: a
    // manifest that does not parse is the resolution's to report.
    let members = vilan_core::manifest::Manifest::parse(&text)
        .ok()
        .and_then(|(manifest, _warnings)| manifest.project)
        .map(|project| project.packages)
        .unwrap_or_default();
    for member in members {
        fix_manifest(&root.join(member), fixed)?;
    }
    Ok(())
}

/// [`fix_manifests`] for one package directory: answers the manifest's text
/// as it now stands, or `None` when there is no manifest there.
fn fix_manifest(directory: &Path, fixed: &mut Fixed) -> Result<Option<String>, String> {
    let path = directory.join("vilan.toml");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let Some((rewritten, rewrites)) = vilan_core::manifest::rewrite_moved_prelude(&text) else {
        return Ok(Some(text));
    };
    std::fs::write(&path, &rewritten)
        .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    let path = path.canonicalize().unwrap_or(path);
    *fixed.moved.entry(path).or_default() += rewrites.len();
    Ok(Some(rewritten))
}

/// Applies the moved-std-path and numeric fixes to every file of `unit`'s
/// package the analysis of its entry under `platform` reports into, to a fixed
/// point. Never edits a file outside the package — std, a dependency — however
/// its diagnostics read, nor one under the package's declared `generated`
/// root (recorded instead, for the report). `Err` is a message for the caller
/// to report.
pub(crate) fn fix_unit(unit: &Unit, platform: Platform, fixed: &mut Fixed) -> Result<(), String> {
    let mut workspace: Workspace = resolve_workspace(unit)?;
    workspace.entry_mode = unit.entry_mode.clone();
    let std_spec = vilan_core::manifest::resolve_std(&std_dir(&unit.entry)?);
    let package_root = unit
        .package_dir
        .clone()
        .unwrap_or_else(|| unit.pkg_root.clone());
    let package_root = package_root.canonicalize().unwrap_or(package_root);
    for _ in 0..ROUND_LIMIT {
        let source = std::fs::read_to_string(&unit.entry)
            .map_err(|error| format!("cannot read {}: {error}", unit.entry.display()))?;
        // The analysis borrows its source for the program's lifetime; a
        // one-shot command leaks it, as `compile_to_js` does.
        let leaked: &'static str = Box::leak(source.into_boxed_str());
        let (program, diagnostics) = vilan_core::analyze_source(
            leaked,
            &std_spec,
            &unit.pkg_root,
            &unit.entry,
            Some(platform),
            &workspace,
        );
        let mut by_file: BTreeMap<PathBuf, Vec<NumericFix>> = BTreeMap::new();
        let mut moved: BTreeMap<PathBuf, BTreeMap<Span, StdPathFix>> = BTreeMap::new();
        let mut for_a_hand: BTreeMap<PathBuf, BTreeMap<usize, ForAHand>> = BTreeMap::new();
        let mut texts: BTreeMap<PathBuf, String> = BTreeMap::new();
        for (index, diagnostic) in diagnostics.iter().enumerate() {
            // A report re-raised out of a macro world indexes text the author
            // did not write; the fix belongs to the definition it echoes.
            if diagnostic.msg.starts_with("in this macro: ") {
                continue;
            }
            let path = program
                .as_ref()
                .and_then(|program| program.source_path(program.diagnostic_source(index)))
                .map(Path::to_path_buf)
                .unwrap_or_else(|| unit.entry.clone());
            let path = path.canonicalize().unwrap_or(path);
            if !path.starts_with(&package_root) {
                continue;
            }
            let is_moved_path = moved_std_module_of_message(&diagnostic.msg).is_some();
            if is_moved_path
                && let Some(generated) = vilan_core::manifest::generated_root_covering(&path)
            {
                fixed
                    .generated
                    .entry(path)
                    .or_insert_with(|| (generated, BTreeSet::new()))
                    .1
                    .insert(diagnostic.span.start);
                continue;
            }
            if !texts.contains_key(&path) {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                texts.insert(path.clone(), text);
            }
            let text = &texts[&path];
            if is_moved_path {
                match moved_std_module_edit(text, &diagnostic.msg, diagnostic.span) {
                    // One edit per site: a brace list's names are refused one
                    // by one at the same segment.
                    Some(Ok(fix)) => {
                        moved.entry(path).or_default().insert(fix.span, fix);
                    }
                    Some(Err(reason)) => {
                        let (line, column) = line_and_column(text, diagnostic.span.start);
                        for_a_hand.entry(path).or_default().insert(
                            diagnostic.span.start,
                            ForAHand {
                                line,
                                column,
                                reason,
                            },
                        );
                    }
                    None => {}
                }
                continue;
            }
            // The PREFERRED edit only: a counter's declaration where there is
            // one, else the conversion — the one the bulk action takes.
            if let Some(preferred) = numeric_fixes(text, diagnostic.span, &diagnostic.msg)
                .into_iter()
                .next()
            {
                by_file.entry(path).or_default().push(preferred);
            }
        }
        // The moved paths first, and alone: the round after them reads the
        // program with its imports resolved.
        let mut moved_this_round = 0;
        for (path, fixes) in moved {
            let text = &texts[&path];
            let (rewritten, applied) = apply_moved_paths(text, fixes.into_values());
            if applied == 0 {
                continue;
            }
            let rewritten = kept_canonical(&path, text, rewritten);
            std::fs::write(&path, rewritten)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            moved_this_round += applied;
            *fixed.moved.entry(path).or_default() += applied;
        }
        if moved_this_round > 0 {
            continue;
        }
        // A round that moved nothing reads every file as it now stands, so
        // its sites for a hand replace any an earlier round recorded.
        fixed.for_a_hand.extend(for_a_hand);
        let mut applied_this_round = 0;
        for (path, fixes) in by_file {
            let (rewritten, applied) = apply_numeric_fixes(&texts[&path], fixes);
            if applied == 0 {
                continue;
            }
            std::fs::write(&path, rewritten)
                .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
            applied_this_round += applied;
            fixed.files.insert(path);
        }
        fixed.edits += applied_this_round;
        if applied_this_round == 0 {
            return Ok(());
        }
    }
    Err(format!(
        "`--fix` stopped after {ROUND_LIMIT} rounds that each still found an edit to make in {} — \
         a fix is re-creating its own diagnostic; the files hold every round's edits",
        unit.entry.display()
    ))
}

/// `fixes` applied to `text` (each replaces its span), from the last to the
/// first so every span still indexes the text it was computed on; a fix whose
/// span overlaps one already applied is skipped (the next round recomputes
/// it). Answers the text and how many were applied.
fn apply_moved_paths(text: &str, fixes: impl Iterator<Item = StdPathFix>) -> (String, usize) {
    let mut fixes: Vec<StdPathFix> = fixes.collect();
    fixes.sort_by_key(|fix| std::cmp::Reverse((fix.span.start, fix.span.end)));
    let mut rewritten = text.to_string();
    let mut applied = 0;
    let mut floor = usize::MAX;
    for fix in fixes {
        if fix.span.end > floor {
            continue;
        }
        rewritten.replace_range(fix.span.into_range(), &fix.replacement);
        floor = fix.span.start;
        applied += 1;
    }
    (rewritten, applied)
}

/// `rewritten` (the moved paths written into `original`), reprinted the way
/// `vilan fmt` prints `path` when `original` already read that way: a new path
/// can change where an import sorts (`std::dom` → `std::web::dom` moves it
/// below `std::option`), and a file the author keeps canonical stays
/// canonical. A file that was not canonical is not reformatted — that is the
/// author's choice, not this command's — and a reprint the printer declines
/// leaves the rewrite as it is.
fn kept_canonical(path: &Path, original: &str, rewritten: String) -> String {
    let directory = path.parent().unwrap_or(Path::new("."));
    let options = format_options(vilan_core::manifest::fmt_opinions_covering(directory));
    if vilan_core::formatter::reprint_with(original, options).as_deref() != Ok(original) {
        return rewritten;
    }
    vilan_core::formatter::reprint_with(&rewritten, options).unwrap_or(rewritten)
}

/// The 1-based line and column (in characters) of byte `offset` in `text`.
fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    (
        before.matches('\n').count() + 1,
        before[line_start..].chars().count() + 1,
    )
}

/// `path` as the report prints it: relative to the working directory when it
/// lies under it, else whole. Built from path components, never by splitting
/// on a separator, so it reads the same on Windows.
fn shown(path: &Path) -> String {
    std::env::current_dir()
        .ok()
        .map(|directory| directory.canonicalize().unwrap_or(directory))
        .and_then(|directory| path.strip_prefix(directory).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

/// What `--fix` prints before the check's own report: the moved std paths it
/// rewrote — a total, then one line per file with its count — and the ones it
/// left (in a generated file, or for a hand, each with its reason), when there
/// were any; then the numeric line, always, in its own words.
pub(crate) fn report(fixed: &Fixed) {
    let fixed_word = paint::out(paint::Style::GREEN, "fixed");
    let moved: usize = fixed.moved.values().sum();
    if moved > 0 {
        let files = fixed.moved.len();
        println!(
            "{fixed_word} {moved} moved std path{} in {files} file{}",
            plural(moved, "", "s"),
            plural(files, "", "s"),
        );
        for (path, count) in &fixed.moved {
            println!("  {}: {count}", shown(path));
        }
    }
    let generated: usize = fixed.generated.values().map(|(_, sites)| sites.len()).sum();
    if generated > 0 {
        println!(
            "{} {generated} moved std path{} in generated files — the project regenerates them, \
             so change what generates them",
            paint::out(paint::Style::YELLOW, "left"),
            plural(generated, "", "s"),
        );
        for (path, (root, sites)) in &fixed.generated {
            println!(
                "  {}: {} (under the package's `generated` root, {})",
                shown(path),
                sites.len(),
                shown(root),
            );
        }
    }
    let for_a_hand: usize = fixed.for_a_hand.values().map(BTreeMap::len).sum();
    if for_a_hand > 0 {
        println!(
            "{} {for_a_hand} moved std path{} for a hand — no one edit rewrites {} correctly",
            paint::out(paint::Style::YELLOW, "left"),
            plural(for_a_hand, "", "s"),
            plural(for_a_hand, "it", "them"),
        );
        for (path, sites) in &fixed.for_a_hand {
            for site in sites.values() {
                println!(
                    "  {}:{}:{}: {}",
                    shown(path),
                    site.line,
                    site.column,
                    site.reason
                );
            }
        }
    }
    let edits = fixed.edits;
    let files = fixed.files.len();
    println!(
        "{fixed_word} {edits} numeric mismatch{} in {files} file{}",
        plural(edits, "", "es"),
        plural(files, "", "s"),
    );
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    /// `--fix` writes files and `--watch` re-runs on every write: the two are
    /// refused together at the command line, before anything is analyzed.
    #[test]
    fn fix_is_refused_beside_watch() {
        let parsed = super::super::Cli::try_parse_from(["vilan", "check", "--fix", "--watch"]);
        let error = parsed.err().expect("the pair is refused");
        assert_eq!(error.kind(), clap::error::ErrorKind::ArgumentConflict);
        assert!(super::super::Cli::try_parse_from(["vilan", "check", "--fix"]).is_ok());
    }
}
