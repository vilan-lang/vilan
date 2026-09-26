//! `--backend rust`: the native backend's build and run arms (tracker F1,
//! slice S1a; `proposal/native-apps.md` §5-S1).
//!
//! The compile is the ordinary one — same analysis, same post-passes, same
//! diagnostics — and only the emitter differs (`compile_to_js`'s one seam). What
//! is left is what this module does with the Rust source that comes out: write a
//! cargo project under `dist/native/<entry>/`, build it, and (for `run`) execute
//! the binary.
//!
//! **Debug by default, and the help says so** (Order 37's R8, `native-apps.md`
//! §8 Q8). rustc is the inner loop from here on: the paper's probe took 1.15 s
//! of user time to build ~700 lines in release, and a real program is orders
//! larger. A developer asking `vilan run --backend rust` is asking to see their
//! program run, not to see it optimised.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use vilan_core::{Backend, Platform};

use crate::{CompileGoal, ExitCode, RoundOutcome, Unit, compile_unit, paint};

/// R3's measurement, recorded by the emitter and read by the differential: how
/// many bindings the last emit boxed into a counted cell because a closure
/// captures them and something writes them.
///
/// A process-global because the number belongs to the COMPILE and is wanted by a
/// test harness that drives the binary, which has no other channel to read it
/// through; `VILAN_NATIVE_REPORT_BOXED=1` is what makes it print.
static BOXED_BINDINGS: AtomicUsize = AtomicUsize::new(0);

pub fn record_boxed_bindings(count: usize) {
    BOXED_BINDINGS.store(count, Ordering::Relaxed);
}

/// F31's measurement, on the same terms: the consumed place reads the last
/// emit COPIED, and the ones it moved because the read was a last use.
/// `VILAN_NATIVE_REPORT_COPIES=1` prints them, and it prints on the `--stdout`
/// path too — the census wants the numbers, not a linked binary.
static CONSUMED_COPIES: AtomicUsize = AtomicUsize::new(0);
static CONSUMED_COPIES_ELIDED: AtomicUsize = AtomicUsize::new(0);

pub fn record_copy_census(copied: usize, elided: usize) {
    CONSUMED_COPIES.store(copied, Ordering::Relaxed);
    CONSUMED_COPIES_ELIDED.store(elided, Ordering::Relaxed);
}

/// The one line both report paths print.
fn report_copy_census() {
    if std::env::var_os("VILAN_NATIVE_REPORT_COPIES").is_some() {
        println!(
            "vilan-native: consumed-copies={} elided={}",
            CONSUMED_COPIES.load(Ordering::Relaxed),
            CONSUMED_COPIES_ELIDED.load(Ordering::Relaxed)
        );
    }
}

/// The host surface the last emit reached (F18's work list), recorded on the
/// same terms and for the same reason as the boxed count above: the number
/// belongs to the COMPILE and the caller that wants it drives the binary.
static HOST_GAPS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn record_host_gaps(gaps: Vec<String>) {
    *HOST_GAPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = gaps;
}

/// The optional runtime crates the last emit reached, and so which of them the
/// cargo project written for it depends on (F18 slice 2's `vilan-rt-sqlite`,
/// F40's `vilan-rt-crypto`). Recorded on the same terms as the three above, for
/// the same reason: the fact belongs to the COMPILE and the manifest is written
/// after it.
static OPTIONAL_CRATES: std::sync::Mutex<vilan_rust::OptionalCrates> =
    std::sync::Mutex::new(vilan_rust::OptionalCrates {
        sqlite: false,
        crypto: false,
    });

pub fn record_optional_crates(reached: vilan_rust::OptionalCrates) {
    *OPTIONAL_CRATES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = reached;
}

/// Where the runtime crate lives (tracker F19).
///
/// THREE roots, in a fixed order, and the order is B346's one-root rule applied
/// to a second toolchain asset: **`$VILAN_RT` wins and never falls through.** A
/// variable naming a directory that holds no `Cargo.toml` is a mistake to
/// report, not a reason to silently link a different runtime than the one the
/// developer pointed at — mixing two runtimes in one build is the outcome worse
/// than refusing, exactly as mixing two std worlds is.
///
/// With nothing named: the `crates/vilan-rt` of the CHECKOUT the entry sits in,
/// so a tree editing the runtime builds against its edits rather than against
/// whatever was embedded when the compiler was last linked; else the embedded
/// copy materialized under `~/.vilan/rt-cache/<hash>/`, which is what makes
/// `--backend rust` work from an INSTALLED toolchain (before F19 it worked from
/// a source checkout and nowhere else).
///
/// The walk is `std_dir`'s, deliberately, and it asks for `vilan/std/vilan.toml`
/// beside the runtime rather than for the runtime alone: an ancestor that
/// carries both is the SAME root `std_dir` would have resolved, so the runtime
/// and the standard library can never come from two different trees. A
/// compile-time `CARGO_MANIFEST_DIR` sibling — S1a's spelling — cannot express
/// that, and worse, it is baked into a copied binary, so an "installed" one went
/// on reading whatever checkout it was built in.
fn runtime_crate(unit: &Unit) -> Result<PathBuf, String> {
    if let Some(from_env) = std::env::var_os("VILAN_RT") {
        let path = PathBuf::from(from_env);
        if path.join("Cargo.toml").is_file() {
            return Ok(path);
        }
        return Err(format!(
            "`VILAN_RT` is set to {}, which holds no `Cargo.toml`. It must name the \
             `vilan-rt` crate's own directory (`crates/vilan-rt` in a vilan checkout). \
             Unset it to use this toolchain's own embedded runtime.",
            path.display()
        ));
    }
    let entry_dir = unit
        .entry
        .canonicalize()
        .ok()
        .and_then(|file| file.parent().map(Path::to_path_buf));
    let working_dir = std::env::current_dir().ok();
    for start in [entry_dir, working_dir].into_iter().flatten() {
        let mut directory = Some(start.as_path());
        while let Some(current) = directory {
            let candidate = current.join("crates").join("vilan-rt");
            if candidate.join("Cargo.toml").is_file()
                && current
                    .join("vilan")
                    .join("std")
                    .join("vilan.toml")
                    .is_file()
            {
                return Ok(vilan_core::util::canonical_path(&candidate));
            }
            directory = current.parent();
        }
    }
    vilan_embedded::materialize_rt()
}

/// The project directory for `unit` — `dist/native/<entry-stem>/`, beside the
/// package the way every other build artifact is, so `rm -rf dist` means the
/// same thing here as everywhere else.
fn project_dir(unit: &Unit) -> PathBuf {
    let root = unit
        .package_dir
        .clone()
        .unwrap_or_else(|| crate::pkg_root_of(&unit.entry));
    let stem = unit
        .entry
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "program".to_string());
    root.join("dist").join("native").join(stem)
}

/// A cargo package name from the entry's stem. Cargo accepts `[A-Za-z0-9_-]`,
/// and a vilan file stem can carry a `.`; the crate name is cosmetic here (the
/// binary is found by path), so a conservative mapping is enough.
fn package_name(unit: &Unit) -> String {
    let stem = unit
        .entry
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| "program".to_string());
    let mapped: String = stem
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if mapped.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("p{mapped}")
    } else {
        mapped
    }
}

/// Compiles `unit` with the native emitter and writes the cargo project.
/// Answers the project directory and the package name.
fn emit_source(unit: &Unit, platform: Platform, emit_debug: bool) -> Result<String, ExitCode> {
    let compiled = compile_unit(
        unit,
        platform,
        Backend::Rust,
        CompileGoal::Emit,
        emit_debug,
        false,
        None,
        None,
    )?;
    Ok(compiled.javascript)
}

/// Writes the cargo project for `unit`, answering its directory and package
/// name.
fn write_project(
    unit: &Unit,
    platform: Platform,
    emit_debug: bool,
) -> Result<(PathBuf, String), ExitCode> {
    let source = emit_source(unit, platform, emit_debug)?;
    let runtime = match runtime_crate(unit) {
        Ok(runtime) => runtime,
        Err(reason) => {
            eprintln!(
                "{} the native backend needs the `vilan-rt` crate: {reason}",
                paint::error_prefix()
            );
            return Err(ExitCode::FAILURE);
        }
    };
    let directory = project_dir(unit);
    let name = package_name(unit);
    let source_dir = directory.join("src");
    if let Err(error) = std::fs::create_dir_all(&source_dir) {
        eprintln!(
            "{} cannot create {}: {error}",
            paint::error_prefix(),
            source_dir.display()
        );
        return Err(ExitCode::FAILURE);
    }
    let manifest = vilan_rust::cargo_manifest(
        &name,
        &runtime.to_string_lossy(),
        *OPTIONAL_CRATES
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    if let Err(error) = std::fs::write(directory.join("Cargo.toml"), manifest) {
        eprintln!(
            "{} cannot write the cargo manifest: {error}",
            paint::error_prefix()
        );
        return Err(ExitCode::FAILURE);
    }
    if let Err(error) = std::fs::write(source_dir.join("main.rs"), &source) {
        eprintln!(
            "{} cannot write the emitted Rust: {error}",
            paint::error_prefix()
        );
        return Err(ExitCode::FAILURE);
    }
    Ok((directory, name))
}

/// Runs `cargo build` over the written project, answering the binary's path.
///
/// `CARGO_TARGET_DIR` is honoured when the caller set it — which is how the
/// differential shares ONE `vilan-rt` build across a hundred programs instead of
/// paying for it a hundred times.
fn cargo_build(directory: &Path, name: &str) -> Result<PathBuf, ExitCode> {
    let mut command = Command::new("cargo");
    command.arg("build").current_dir(directory);
    // Cargo would otherwise try to read the surrounding workspace through the
    // directory chain; the generated manifest declares its own `[workspace]`,
    // and this keeps a stray `RUSTFLAGS`-style inheritance out of the picture.
    command.env_remove("CARGO");
    let output = match command.output() {
        Ok(output) => output,
        Err(error) => {
            eprintln!(
                "{} the native backend needs `cargo` on PATH: {error}",
                paint::error_prefix()
            );
            return Err(ExitCode::FAILURE);
        }
    };
    if !output.status.success() {
        let rustc_output = String::from_utf8_lossy(&output.stderr);
        eprint!("{rustc_output}");
        let refusal = RustcRefusal::read(&rustc_output);
        for overflow in &refusal.overflows {
            eprintln!(
                "{} the PROGRAM overflows: rustc evaluated {} at compile time and refused it \
                 ({}). A conforming program does not overflow (spec §7.2a): the JavaScript \
                 backend would have run on past it with an out-of-range value, and a native \
                 build refuses an overflow rustc can see at compile time — so this is the \
                 program's own arithmetic, not a backend defect.",
                paint::error_prefix(),
                overflow.named(),
                overflow.reason,
            );
        }
        if refusal.other_errors > 0 || refusal.overflows.is_empty() {
            eprintln!(
                "{} `cargo build` refused the emitted Rust. That is a BACKEND defect, not a \
                 defect in the vilan program — please report it with the program and \
                 `dist/native/*/src/main.rs`.",
                paint::error_prefix()
            );
        }
        return Err(ExitCode::FAILURE);
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| directory.join("target"));
    Ok(target.join("debug").join(name))
}

/// What rustc's refusal of the emitted Rust was made of, read off its
/// human-readable output (N120).
///
/// One class of refusal is NOT a backend defect: rustc's deny-by-default
/// `arithmetic_overflow` lint evaluates constant integer arithmetic and refuses
/// the build when it overflows — `let a: i32 = 2147483647; print(a + 1);`.
/// A conforming program does not overflow (spec §7.2a, and I5's ruling 2 for
/// `usize`: backend-defined, never memory-unsafe), the JavaScript backend runs
/// on past one, and this program did overflow — so blaming the backend for it
/// is a false accusation.
/// Everything else rustc refuses still is a backend defect: the emitter wrote
/// Rust that does not compile.
///
/// Read from the TEXT because the CLI carries no JSON reader, and the lint's
/// head (`this arithmetic operation will overflow`) and rustc's snippet layout
/// have been stable across every toolchain this backend has pinned. A refusal
/// whose text does not match is simply not recognised, and falls back to the
/// backend-defect sentence — the conservative reading.
#[derive(Debug, Default, PartialEq)]
struct RustcRefusal {
    overflows: Vec<ConstantOverflow>,
    /// rustc errors that are not `arithmetic_overflow` — cargo's own trailer
    /// (`could not compile`) and rustc's (`aborting due to`) are not counted.
    other_errors: usize,
}

/// One `arithmetic_overflow` refusal: where in the emitted Rust, the
/// expression rustc underlined there, and rustc's own reason.
#[derive(Debug, PartialEq)]
struct ConstantOverflow {
    /// `src/main.rs:LINE:COLUMN`, as rustc's `-->` line gives it.
    location: String,
    /// The underlined expression, when the span is on one line.
    expression: Option<String>,
    /// rustc's label: `attempt to compute `i32::MAX + 1_i32`, which would
    /// overflow`.
    reason: String,
}

impl ConstantOverflow {
    /// The expression and where it is, for the sentence.
    fn named(&self) -> String {
        match &self.expression {
            Some(expression) => format!("`{expression}` (the emitted Rust, {})", self.location),
            None => format!("the expression at {} in the emitted Rust", self.location),
        }
    }
}

/// The lint's message head, which names the refusal whatever else changes.
const OVERFLOW_HEAD: &str = "error: this arithmetic operation will overflow";

impl RustcRefusal {
    fn read(rustc_output: &str) -> RustcRefusal {
        let lines: Vec<String> = rustc_output.lines().map(strip_ansi).collect();
        let mut refusal = RustcRefusal::default();
        for (index, line) in lines.iter().enumerate() {
            if line == OVERFLOW_HEAD {
                refusal
                    .overflows
                    .push(ConstantOverflow::read(&lines[index + 1..]));
            } else if (line.starts_with("error:") || line.starts_with("error["))
                && !line.starts_with("error: could not compile")
                && !line.starts_with("error: aborting due to")
            {
                refusal.other_errors += 1;
            }
        }
        refusal
    }
}

impl ConstantOverflow {
    /// Reads the snippet after the lint's head: the `-->` location, then the
    /// first line whose gutter holds carets — the underline — and the source
    /// line above it, which the carets index by column once both gutters are
    /// cut at their `| `.
    fn read(following: &[String]) -> ConstantOverflow {
        let block: Vec<&String> = following
            .iter()
            .take_while(|line| !line.starts_with("error") && !line.starts_with("warning"))
            .collect();
        let location = block
            .iter()
            .find_map(|line| line.trim_start().strip_prefix("--> "))
            .unwrap_or("src/main.rs")
            .to_string();
        let underline = block.iter().position(|line| {
            after_gutter(line).is_some_and(|text| text.trim_start().starts_with('^'))
        });
        let (expression, reason) = match underline {
            Some(position) => {
                // Columns, counted in characters on both lines: the gutters
                // are cut, so the underline's leading spaces are the
                // expression's column in the source line above it.
                let marks = after_gutter(block[position]).unwrap_or_default();
                let start = marks.chars().take_while(|mark| *mark == ' ').count();
                let width = marks
                    .chars()
                    .skip(start)
                    .take_while(|mark| *mark == '^')
                    .count();
                let reason: String = marks.chars().skip(start + width).collect();
                let expression = position
                    .checked_sub(1)
                    .and_then(|above| after_gutter(block[above]))
                    .filter(|source| source.chars().count() >= start + width)
                    .map(|source| source.chars().skip(start).take(width).collect());
                (expression, reason.trim().to_string())
            }
            None => (None, String::new()),
        };
        ConstantOverflow {
            location,
            expression,
            reason: if reason.is_empty() {
                "the arithmetic would overflow".to_string()
            } else {
                reason
            },
        }
    }
}

/// The text after a snippet line's `| ` gutter, if the line has one.
fn after_gutter(line: &str) -> Option<&str> {
    let (gutter, text) = line.split_once('|')?;
    if !gutter
        .trim()
        .chars()
        .all(|character| character.is_ascii_digit())
    {
        return None;
    }
    Some(text.strip_prefix(' ').unwrap_or(text))
}

/// `line` without ANSI escape sequences — `CARGO_TERM_COLOR=always` colours
/// the output even into a pipe.
fn strip_ansi(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character == '\u{1b}' {
            for terminator in characters.by_ref() {
                if terminator.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(character);
        }
    }
    plain
}

/// `vilan build --backend rust` — write the project, build it, say where the
/// binary is.
pub fn build(unit: &Unit, platform: Platform, emit_debug: bool, stdout: bool) -> RoundOutcome {
    if stdout {
        // `--stdout` prints the emitted source and writes NOTHING, exactly as
        // it does for JavaScript: it prints a program, not a build. It has to
        // come BEFORE the project is written, or the flag that writes nothing
        // would leave a `dist/native/` behind.
        return match emit_source(unit, platform, emit_debug) {
            Ok(source) => {
                // The host census (F18's work list) prints the list INSTEAD of
                // the source: the source a census emit produces carries
                // `unimplemented!()` where a host body belongs and is not a
                // build. See `vilan_rust::Emitted::host_gaps`.
                if std::env::var_os("VILAN_NATIVE_HOST_CENSUS").is_some() {
                    let gaps = HOST_GAPS
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    println!("vilan-native: {} host gaps", gaps.len());
                    for gap in gaps.iter() {
                        println!("  {gap}");
                    }
                } else {
                    print!("{source}");
                }
                report_copy_census();
                RoundOutcome::Succeeded
            }
            Err(_) => RoundOutcome::Failed,
        };
    }
    let (directory, name) = match write_project(unit, platform, emit_debug) {
        Ok(written) => written,
        Err(_) => return RoundOutcome::Failed,
    };
    let binary = match cargo_build(&directory, &name) {
        Ok(binary) => binary,
        Err(_) => return RoundOutcome::Failed,
    };
    println!(
        "{} {} -> {}",
        paint::out(paint::Style::GREEN, "Compiled"),
        unit.entry.display(),
        paint::out(paint::Style::BOLD, &binary.display().to_string())
    );
    if std::env::var_os("VILAN_NATIVE_REPORT_BOXED").is_some() {
        println!(
            "vilan-native: boxed-bindings={}",
            BOXED_BINDINGS.load(Ordering::Relaxed)
        );
    }
    report_copy_census();
    RoundOutcome::Succeeded
}

/// `vilan run --backend rust` — build in debug, then execute.
pub fn run(unit: &Unit, platform: Platform, args: &[String]) -> ExitCode {
    let (directory, name) = match write_project(unit, platform, false) {
        Ok(written) => written,
        Err(code) => return code,
    };
    let binary = match cargo_build(&directory, &name) {
        Ok(binary) => binary,
        Err(code) => return code,
    };
    crate::exit_code_of(Command::new(&binary).args(args).status())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// rustc's refusal of `let a: i32 = 2147483647; print(a + 1);` as `cargo
    /// build` printed it into a pipe (1.90 and 1.98.1 print it alike).
    /// A RAW string, so the diagnostics ledger's continuation gate reads it
    /// as the drawing it is rather than as a lost `\`.
    const OVERFLOW: &str = r#"   Compiling over v0.0.0 (/tmp/over/dist/native/over)
error: this arithmetic operation will overflow
  --> src/main.rs:10:22
   |
10 |     vilan_rt::print(&((a_8852 + (1i32))));
   |                      ^^^^^^^^^^^^^^^^^^^ attempt to compute `i32::MAX + 1_i32`, which would overflow
   |
   = note: `#[deny(arithmetic_overflow)]` on by default

error: could not compile `over` (bin "over") due to 1 previous error
"#;

    /// A refusal that IS the emitter's: a type mismatch.
    const MISMATCH: &str = r#"error[E0308]: mismatched types
 --> src/main.rs:4:18
  |
4 |     let a: u64 = 1i32;
  |            ---   ^^^^ expected `u64`, found `i32`
  |            |
  |            expected due to this

"#;

    #[test]
    fn an_arithmetic_overflow_refusal_is_read_as_the_programs_own() {
        let refusal = RustcRefusal::read(OVERFLOW);
        assert_eq!(
            refusal,
            RustcRefusal {
                overflows: vec![ConstantOverflow {
                    location: "src/main.rs:10:22".to_string(),
                    expression: Some("((a_8852 + (1i32)))".to_string()),
                    reason: "attempt to compute `i32::MAX + 1_i32`, which would overflow"
                        .to_string(),
                }],
                other_errors: 0,
            }
        );
    }

    #[test]
    fn any_other_rustc_error_is_still_counted_as_the_backends() {
        assert_eq!(
            RustcRefusal::read(MISMATCH),
            RustcRefusal {
                overflows: Vec::new(),
                other_errors: 1,
            }
        );
        // Mixed: the overflow is read, AND the mismatch keeps the
        // backend-defect sentence owed.
        let mixed = RustcRefusal::read(&format!("{MISMATCH}{OVERFLOW}"));
        assert_eq!(mixed.overflows.len(), 1);
        assert_eq!(mixed.other_errors, 1);
    }

    #[test]
    fn a_coloured_refusal_reads_the_same() {
        let coloured = OVERFLOW
            .replace(
                "error: this arithmetic",
                "\u{1b}[1m\u{1b}[91merror\u{1b}[0m\u{1b}[1m: this arithmetic",
            )
            .replace("   |      ", "\u{1b}[1m\u{1b}[94m   |\u{1b}[0m      ");
        assert_eq!(RustcRefusal::read(&coloured), RustcRefusal::read(OVERFLOW));
    }

    #[test]
    fn a_multi_line_span_names_its_location_without_an_expression() {
        let multi_line = r#"error: this arithmetic operation will overflow
  --> src/main.rs:10:5
   |
10 | /     vilan_rt::print(&((a_8852
11 | |         + (1i32))));
   | |___________________^ attempt to compute `i32::MAX + 1_i32`, which would overflow
"#;
        let refusal = RustcRefusal::read(multi_line);
        assert_eq!(refusal.overflows.len(), 1);
        assert_eq!(refusal.overflows[0].location, "src/main.rs:10:5");
        assert_eq!(refusal.overflows[0].expression, None);
        assert_eq!(
            refusal.overflows[0].named(),
            "the expression at src/main.rs:10:5 in the emitted Rust"
        );
    }
}
