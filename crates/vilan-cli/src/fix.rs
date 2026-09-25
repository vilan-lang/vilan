//! `vilan check --fix` (`proposal/index-type.md` §8.1): the numeric-mismatch
//! fixes the language server offers one at a time, applied to a package in
//! bulk, to a fixed point.
//!
//! The edits are not computed here. `vilan_ide::numeric_fix` is the one
//! function both surfaces call, so the command line can never write a
//! different conversion from the one the editor offers for the same
//! diagnostic. This file is the DRIVER §8.1 describes: analyze, apply every
//! fix the diagnostics carry, and repeat until a round applies nothing — then
//! hand over to the ordinary `check`, which reports whatever needs a person
//! (a `-1` sentinel, a `>= 0` loop, signed arithmetic that must convert once
//! at its end) exactly as it would have.
//!
//! One-shot, like the rest of the CLI: it runs on the compiler thread, outside
//! the panic fence, and a compiler panic fails the command loudly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use vilan_core::{Platform, Workspace};
use vilan_ide::numeric_fix::{NumericFix, apply_numeric_fixes, numeric_fixes};

use super::{Unit, paint, resolve_workspace, std_dir};

/// Rounds past which the driver stops rather than loop: every round must
/// apply at least one edit to continue, and a real migration settles in a
/// handful. A fix that kept re-creating its own diagnostic would otherwise
/// never end.
const ROUND_LIMIT: usize = 32;

/// What a `--fix` pass did to one unit.
#[derive(Default)]
pub(crate) struct Fixed {
    pub(crate) edits: usize,
    pub(crate) files: std::collections::BTreeSet<PathBuf>,
}

/// Applies the numeric fixes to every file of `unit`'s package the analysis
/// of its entry under `platform` reports into, to a fixed point. Never edits
/// a file outside the package — std, a dependency — however its diagnostics
/// read. `Err` is a message for the caller to report.
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
            if !texts.contains_key(&path) {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                texts.insert(path.clone(), text);
            }
            let text = &texts[&path];
            // The PREFERRED edit only: a counter's declaration where there is
            // one, else the conversion — the one the bulk action takes.
            if let Some(preferred) = numeric_fixes(text, diagnostic.span, &diagnostic.msg)
                .into_iter()
                .next()
            {
                by_file.entry(path).or_default().push(preferred);
            }
        }
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

/// The one line `--fix` prints before the check's own report.
pub(crate) fn report(fixed: &Fixed) {
    let edits = fixed.edits;
    let files = fixed.files.len();
    println!(
        "{} {edits} numeric mismatch{} in {files} file{}",
        paint::out(paint::Style::GREEN, "fixed"),
        if edits == 1 { "" } else { "es" },
        if files == 1 { "" } else { "s" },
    );
}
