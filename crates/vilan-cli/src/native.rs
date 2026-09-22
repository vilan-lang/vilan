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

/// The host surface the last emit reached (F18's work list), recorded on the
/// same terms and for the same reason as the boxed count above: the number
/// belongs to the COMPILE and the caller that wants it drives the binary.
static HOST_GAPS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

pub fn record_host_gaps(gaps: Vec<String>) {
    *HOST_GAPS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = gaps;
}

/// Whether the last emit reached `std::db`, and so whether the cargo project
/// written for it depends on `vilan-rt-sqlite` (F18 slice 2; Order 39's R1).
/// Recorded on the same terms as the two above, for the same reason: the fact
/// belongs to the COMPILE and the manifest is written after it.
static REACHES_SQLITE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn record_reaches_sqlite(reaches: bool) {
    REACHES_SQLITE.store(reaches, Ordering::Relaxed);
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
    vilan_embedded_std::materialize_rt()
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
        REACHES_SQLITE.load(Ordering::Relaxed),
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
        eprint!("{}", String::from_utf8_lossy(&output.stderr));
        eprintln!(
            "{} `cargo build` refused the emitted Rust. That is a BACKEND defect, not a defect \
             in the vilan program — please report it with the program and \
             `dist/native/*/src/main.rs`.",
            paint::error_prefix()
        );
        return Err(ExitCode::FAILURE);
    }
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| directory.join("target"));
    Ok(target.join("debug").join(name))
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
