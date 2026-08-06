//! `vilan bindgen` — the CLI wrapper (backlog E31, `proposal/bindgen.md` §1).
//!
//! A thin front-end, in the shape `fmt` set: argument handling and exit codes
//! only. The `.d.ts → .vl` machinery lives in [`vilan_core::bindgen`] so it
//! stays reachable from somewhere other than the CLI binary — an LSP quick-fix
//! that generates a binding for an unresolved import, say.
//!
//! Explicitly NOT a build step (§1): nothing in `build`/`check`/`run` reaches
//! this, no manifest key turns it on, and it reads nothing from `vilan.toml`.
//! The `.vl` it writes is a file the developer reviews, edits, and commits.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vilan_core::bindgen::{Options, generate};

use crate::paint;
use crate::report_error;

/// Generates a vilan bindings module from `file`, writing it to `output` (or
/// `<file-stem>.vl` beside the input, matching `vilan build`'s default).
pub fn bindgen(
    file: PathBuf,
    output: Option<PathBuf>,
    platform: String,
    only: Vec<String>,
    stdout: bool,
    stats: bool,
) -> ExitCode {
    let source = match fs::read_to_string(&file) {
        Ok(source) => source,
        Err(error) => {
            return report_error(&format!("cannot read {}: {error}", file.display()));
        }
    };
    let options = Options {
        platform,
        source_name: file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| file.display().to_string()),
        only,
    };
    if let Err(message) = options.validate() {
        return report_error(&message);
    }

    let generated = generate(&source, &options);

    // A misspelled `--only` would otherwise write a file quietly missing the
    // type the caller asked for, which is worse than writing nothing.
    if !generated.unknown_only.is_empty() {
        return report_error(&format!(
            "{} declares none of: {}",
            file.display(),
            generated.unknown_only.join(", ")
        ));
    }

    if stats {
        eprint!("{}", generated.coverage.report());
    }

    if stdout {
        print!("{}", generated.source);
        return ExitCode::SUCCESS;
    }

    let destination = output.unwrap_or_else(|| default_output_path(&file));
    if let Err(error) = fs::write(&destination, &generated.source) {
        return report_error(&format!("cannot write {}: {error}", destination.display()));
    }
    let todos = generated.coverage.total_todos();
    println!(
        "{} {}{}",
        paint::out(paint::Style::GREEN, "Generated"),
        paint::out(paint::Style::BOLD, &destination.display().to_string()),
        if todos == 0 {
            String::new()
        } else {
            format!(" ({todos} TODO(bindgen) comment(s) to review)")
        }
    );
    ExitCode::SUCCESS
}

/// `leaflet.d.ts` → `leaflet.vl`, beside the input (§1). `.d.ts` is two
/// extensions deep, so the plain file stem would leave a stray `.d`.
fn default_output_path(file: &Path) -> PathBuf {
    let stem = file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = stem
        .strip_suffix(".d.ts")
        .or_else(|| stem.strip_suffix(".ts"))
        .unwrap_or(&stem);
    file.with_file_name(format!("{stem}.vl"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_output_path_strips_both_extensions_of_a_d_ts() {
        assert_eq!(
            default_output_path(Path::new("vendor/leaflet.d.ts")),
            PathBuf::from("vendor/leaflet.vl")
        );
        assert_eq!(
            default_output_path(Path::new("leaflet.ts")),
            PathBuf::from("leaflet.vl")
        );
        assert_eq!(
            default_output_path(Path::new("leaflet")),
            PathBuf::from("leaflet.vl")
        );
    }
}
