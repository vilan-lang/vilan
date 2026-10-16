//! std-prefix P3's round-trip differential, the EMITTED half: through the
//! binary, every program of a corpus sample is built to JavaScript and to the
//! native backend's Rust (`--stdout`) four ways, and the bytes — stdout and
//! stderr both — must agree:
//!
//! 1. **canonical** — `VILAN_STD_PREFIX=off`, the drain's one order;
//! 2. **built** — over the std-only prefix built in the process, the world
//!    cache off;
//! 3. **written** — the same, with the world cache pointed at a scratch
//!    directory: the prefix file is written (the counters say so);
//! 4. **loaded** — a fresh process finds the file and serves it.
//!
//! The in-process differential (`vilan-core`'s `prefix_roundtrip_differential`)
//! compares the observation and the JS over the harness's fixtures; this one
//! is the cross-PROCESS leg the file exists for, and the native backend's
//! names carry ids, which is what makes the Rust bytes the sharpest reading
//! of the loaded world's id layout (M144).

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// A sample of the native differential's default suite: single-file
/// programs the native backend accepts, each a different std closure (so
/// each writes and loads a file of its own), and one package with two
/// sibling modules — M144's repro shape.
const PROGRAMS: &[&str] = &[
    "bool.vl",
    "recursion.vl",
    "loops.vl",
    "for-in.vl",
    "list-splice.vl",
    "match-ergonomics.vl",
    "generic-inference.vl",
    "time.vl",
];

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test")
}

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// One emission: `vilan build --stdout [--backend rust] <file>` with the
/// prefix and the world cache as asked; stdout, stderr, and the counters'
/// `prefix-file=` reading.
struct Emission {
    stdout: Vec<u8>,
    stderr: String,
    prefix_world: bool,
    prefix_file: String,
}

/// `None` when the native backend REFUSES the program ("does not emit": F1's
/// slice S1b scope) — a refusal names the construct, not the prefix, and the
/// JS leg still compares.
fn emit(
    file: &Path,
    backend: Option<&str>,
    prefix: &str,
    cache: Option<&Path>,
) -> Option<Emission> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .arg("build")
        .arg("--stdout")
        .arg(file)
        .env("VILAN_STD", std_dir())
        .env(
            "VILAN_RT",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../vilan-rt"),
        )
        .env("VILAN_STD_PREFIX", prefix)
        .env("VILAN_COUNTERS", "1")
        .env(
            "VILAN_WORLD_CACHE",
            cache.map_or("off".to_string(), |cache| cache.display().to_string()),
        );
    if let Some(backend) = backend {
        command.args(["--backend", backend]);
    }
    let output = command.output().expect("run vilan");
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() && backend.is_some() && stderr.contains("does not emit") {
        return None;
    }
    assert!(
        output.status.success(),
        "{} ({backend:?}, prefix {prefix}) failed:\n{stderr}",
        file.display()
    );
    let counters = stderr
        .lines()
        .find(|line| line.contains("prefix-file="))
        .unwrap_or_else(|| panic!("no counters line for {}:\n{stderr}", file.display()));
    let read = |label: &str| -> String {
        counters
            .split_whitespace()
            .find_map(|word| word.strip_prefix(label))
            .unwrap_or_else(|| panic!("no {label} in: {counters}"))
            .to_string()
    };
    // The instrument lines (`[vilan counters]`, `[vilan phase]`, `[vilan
    // walk-frame]`: a walk's frame is measured only where std walks) name the
    // process's own numbers; everything else on stderr (diagnostics,
    // warnings) must agree across the legs.
    let stderr_without_counters: String = stderr
        .lines()
        .filter(|line| !line.starts_with("[vilan "))
        .map(|line| format!("{line}\n"))
        .collect();
    Some(Emission {
        stdout: output.stdout,
        stderr: stderr_without_counters,
        prefix_world: read("prefix-world=") == "1",
        prefix_file: read("prefix-file="),
    })
}

fn compare_program(file: &Path, cache: &Path, divergences: &mut Vec<String>) -> bool {
    let label = file.file_name().unwrap().to_string_lossy().into_owned();
    let mut loaded_once = false;
    for backend in [None, Some("rust")] {
        let leg = |name: &str| format!("{label} ({}, {name})", backend.unwrap_or("js"));
        let Some(canonical) = emit(file, backend, "off", None) else {
            // The native backend refuses this program: the JS leg compares it.
            continue;
        };
        let built = emit(file, backend, "on", None).expect("the canonical leg emitted");
        if !built.prefix_world {
            // Refused by the prefix (a package impl a std question reaches,
            // a macro): the other differentials compare it.
            return false;
        }
        let written = emit(file, backend, "on", Some(cache)).expect("the canonical leg emitted");
        let loaded = emit(file, backend, "on", Some(cache)).expect("the canonical leg emitted");
        if !matches!(written.prefix_file.as_str(), "written" | "loaded") {
            divergences.push(format!(
                "{}: the first run with the world cache neither wrote nor loaded: {}",
                leg("written"),
                written.prefix_file
            ));
        }
        if loaded.prefix_file != "loaded" {
            divergences.push(format!(
                "{}: the second run did not load the file: {}",
                leg("loaded"),
                loaded.prefix_file
            ));
        } else {
            loaded_once = true;
        }
        for (name, other) in [
            ("built", &built),
            ("written", &written),
            ("loaded", &loaded),
        ] {
            if other.stdout != canonical.stdout {
                divergences.push(format!(
                    "{}: the emitted bytes differ from the canonical build's ({} against {} bytes)",
                    leg(name),
                    other.stdout.len(),
                    canonical.stdout.len()
                ));
            }
            if other.stderr != canonical.stderr {
                divergences.push(format!(
                    "{}: the diagnostics differ from the canonical build's:\n--- canonical\n{}--- {name}\n{}",
                    leg(name),
                    canonical.stderr,
                    other.stderr
                ));
            }
        }
    }
    loaded_once
}

#[test]
fn every_sampled_program_emits_the_same_bytes_over_a_loaded_prefix() {
    let cache = support::scratch_dir("prefix-roundtrip-native");
    let mut divergences = Vec::new();
    let mut loaded = 0;
    for program in PROGRAMS {
        let file = corpus_dir().join(program);
        assert!(file.is_file(), "{} is in the corpus", file.display());
        if compare_program(&file, &cache, &mut divergences) {
            loaded += 1;
        }
    }
    assert!(
        divergences.is_empty(),
        "{} divergence(s) over the persisted std prefix:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
    assert!(
        loaded >= PROGRAMS.len() / 2,
        "only {loaded} of {} programs loaded a prefix file; the comparison is otherwise vacuous",
        PROGRAMS.len()
    );
    let _ = std::fs::remove_dir_all(&cache);
}

/// M144's shape through the file: a package whose entry imports two sibling
/// modules beside `std::path`, emitted natively over a loaded prefix, names
/// its std items exactly as the canonical build does (`basename_<id>`) — the
/// package's own names are the prefix shape's (the find in the lane's
/// report) and are compared between the built and the loaded prefix only.
#[test]
fn m144_a_package_with_siblings_names_std_the_same_over_a_loaded_prefix() {
    let cache = support::scratch_dir("prefix-roundtrip-m144");
    let package = support::scratch_dir("prefix-roundtrip-m144-pkg");
    std::fs::create_dir_all(package.join("src")).expect("src");
    std::fs::write(
        package.join("vilan.toml"),
        "[package]\nname = \"siblings\"\n[entry.main]\n",
    )
    .expect("manifest");
    std::fs::write(
        package.join("src/main.vl"),
        "import std::path::basename;\nimport pkg::b::two;\nimport pkg::c::three;\n\nfun main() {\n\tprint(basename(\"/x/y.txt\"));\n\tprint(two() + three());\n}\n",
    )
    .expect("entry");
    std::fs::write(
        package.join("src/b.vl"),
        "export fun two(): i32 {\n\t2\n}\n",
    )
    .expect("b");
    std::fs::write(
        package.join("src/c.vl"),
        "export fun three(): i32 {\n\t3\n}\n",
    )
    .expect("c");
    let entry = package.join("src/main.vl");
    let canonical = emit(&entry, Some("rust"), "off", None).expect("the repro emits natively");
    let built = emit(&entry, Some("rust"), "on", None).expect("the repro emits natively");
    assert!(built.prefix_world, "the package takes the prefix");
    let written = emit(&entry, Some("rust"), "on", Some(&cache)).expect("the repro emits natively");
    assert!(
        matches!(written.prefix_file.as_str(), "written" | "loaded"),
        "{}",
        written.prefix_file
    );
    let loaded = emit(&entry, Some("rust"), "on", Some(&cache)).expect("the repro emits natively");
    assert_eq!(loaded.prefix_file, "loaded");
    assert_eq!(
        loaded.stdout, built.stdout,
        "the loaded prefix must emit the built prefix's bytes"
    );
    let std_name = |bytes: &[u8]| -> String {
        let text = String::from_utf8_lossy(bytes);
        text.lines()
            .find_map(|line| {
                line.split_whitespace()
                    .find(|word| word.starts_with("basename_"))
            })
            .map(|word| word.trim_end_matches('(').to_string())
            .expect("basename's native name")
    };
    assert_eq!(
        std_name(&loaded.stdout),
        std_name(&canonical.stdout),
        "M144: std's items are named alike in the canonical and the loaded-prefix build"
    );
    let _ = std::fs::remove_dir_all(&cache);
    let _ = std::fs::remove_dir_all(&package);
}
