//! The corpus byte gate (backlog E5): every `vilan/test/*.vl` with a `.js`
//! golden compiles — via the CURRENT `vilan` binary, exactly the command that
//! generated the goldens — to byte-identical output (`.css` assets included).
//!
//! This replaces the by-hand loop (rebuild the debug binary, regenerate,
//! `git diff`) that the golden-regen discipline existed to police: the binary
//! under test is always the one Cargo just built from this tree, so a stale
//! binary can no longer write or check goldens. A deliberate output change
//! still regenerates goldens by hand; this gate then verifies the commit.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::Command;

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test")
}

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

/// The first difference between golden and rebuilt output, reported at BYTE
/// granularity, for the report.
///
/// Byte-level deliberately (`windows-support.md` §3). The previous version
/// zipped `golden.lines()` with `rebuilt.lines()`, and `str::lines()` strips a
/// trailing `\r` — so a CRLF-vs-LF mismatch (the wholesale failure mode of a
/// Git-for-Windows `autocrlf` checkout, which §3's `.gitattributes` now
/// prevents) compared *equal* on every line and fell through to "lengths differ
/// (golden 412 lines, rebuilt 412)": a diagnostic that hides the cause and
/// states two identical numbers as though they differed. Excerpts are
/// `{:?}`-escaped so an invisible byte — `\r` above all — is visible in the
/// failure message.
fn first_difference(golden: &str, rebuilt: &str) -> String {
    let mismatch = golden
        .as_bytes()
        .iter()
        .zip(rebuilt.as_bytes())
        .position(|(golden_byte, rebuilt_byte)| golden_byte != rebuilt_byte);
    let Some(offset) = mismatch else {
        // Nothing differs in the overlap: one side is a strict prefix of the
        // other (or — unreachable from the gate, which only calls this after an
        // inequality — they are identical).
        let (golden_length, rebuilt_length) = (golden.len(), rebuilt.len());
        return match golden_length.cmp(&rebuilt_length) {
            Ordering::Equal => "no difference (identical bytes)".to_string(),
            Ordering::Less => format!(
                "golden is a strict prefix of rebuilt (golden {golden_length} bytes, \
                 rebuilt {rebuilt_length} bytes); rebuilt continues at byte \
                 {golden_length} (line {}) with {:?}",
                line_number(rebuilt, golden_length),
                line_at(rebuilt, golden_length)
            ),
            Ordering::Greater => format!(
                "rebuilt is a strict prefix of golden (golden {golden_length} bytes, \
                 rebuilt {rebuilt_length} bytes); golden continues at byte \
                 {rebuilt_length} (line {}) with {:?}",
                line_number(golden, rebuilt_length),
                line_at(golden, rebuilt_length)
            ),
        };
    };
    // Every byte before `offset` matches, so both sides agree on the line number.
    format!(
        "first difference at byte {offset} (line {}): golden {:?} vs rebuilt {:?}",
        line_number(golden, offset),
        line_at(golden, offset),
        line_at(rebuilt, offset)
    )
}

/// The 1-based line number containing `offset`. An offset pointing AT a `\n`
/// belongs to the line that newline terminates.
fn line_number(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|&&byte| byte == b'\n')
        .count()
        + 1
}

/// The line containing `offset`, without its terminating `\n` — but *with* any
/// `\r`, since naming that byte is the whole point. Sliced only at newline
/// boundaries (always char boundaries), so a mid-codepoint `offset` cannot panic.
fn line_at(text: &str, offset: usize) -> &str {
    let bytes = text.as_bytes();
    let offset = offset.min(bytes.len());
    let start = bytes[..offset]
        .iter()
        .rposition(|&byte| byte == b'\n')
        .map_or(0, |index| index + 1);
    let end = bytes[offset..]
        .iter()
        .position(|&byte| byte == b'\n')
        .map_or(bytes.len(), |index| offset + index);
    &text[start..end]
}

/// The EOL class the old `lines()` diagnostic could not name (§3): a CRLF golden
/// against an LF rebuild must report the `\r` itself, not "lengths differ" with
/// two equal numbers.
#[test]
fn first_difference_names_a_carriage_return() {
    let message = first_difference("let a = 1;\r\nlet b = 2;\r\n", "let a = 1;\nlet b = 2;\n");
    assert!(
        message.contains(r"\r"),
        "the carriage return is not named: {message}"
    );
    assert!(
        message.contains("byte 10") && message.contains("line 1"),
        "wrong position for the first differing byte: {message}"
    );
    assert!(
        !message.contains("lengths differ"),
        "still reporting the old (and, for this input, equal-numbered) line-count \
         diagnostic: {message}"
    );
}

/// A difference inside a line reports its byte offset and line number.
#[test]
fn first_difference_locates_a_mid_line_change() {
    let golden = "fun main() {\n\tprint(1);\n}\n";
    let rebuilt = "fun main() {\n\tprint(2);\n}\n";
    let message = first_difference(golden, rebuilt);
    // "fun main() {\n" is 13 bytes; "\tprint(" a further 7 — the digit is byte 20.
    assert_eq!(golden.as_bytes()[20], b'1');
    assert!(
        message.contains("byte 20") && message.contains("line 2"),
        "wrong position for the first differing byte: {message}"
    );
    assert!(
        message.contains(r#""\tprint(1);""#) && message.contains(r#""\tprint(2);""#),
        "both sides' lines are not shown: {message}"
    );
}

/// Truncated output (a rebuild that stopped early) is named as such, with both
/// byte lengths — the case the old diagnostic reported in *lines*.
#[test]
fn first_difference_reports_a_strict_prefix() {
    let message = first_difference("alpha\nbeta\n", "alpha\n");
    assert!(
        message.contains("rebuilt is a strict prefix of golden"),
        "the prefix relation is not named: {message}"
    );
    assert!(
        message.contains("golden 11 bytes") && message.contains("rebuilt 6 bytes"),
        "both byte lengths are not reported: {message}"
    );
    assert!(
        message.contains(r#""beta""#),
        "the first unmatched line is not shown: {message}"
    );

    let reversed = first_difference("alpha\n", "alpha\nbeta\n");
    assert!(
        reversed.contains("golden is a strict prefix of rebuilt")
            && reversed.contains("golden 6 bytes")
            && reversed.contains("rebuilt 11 bytes"),
        "the reversed prefix case is wrong: {reversed}"
    );
}

#[test]
fn every_corpus_golden_is_byte_identical() {
    let corpus = corpus_dir();
    // A full copy: corpus programs may import sibling modules, and building
    // in place would overwrite the goldens under comparison.
    let work = std::env::temp_dir().join(format!("vilan_corpus_gate_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("create corpus work dir");
    let mut programs: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&corpus).expect("corpus directory") {
        let path = entry.expect("corpus entry").path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(extension) = path.extension() else {
            continue;
        };
        if extension == "vl" {
            std::fs::copy(&path, work.join(name)).expect("copy corpus source");
            if path.with_extension("js").is_file() {
                programs.push(name.to_string());
            }
        }
    }
    programs.sort();
    assert!(
        programs.len() > 60,
        "suspiciously few corpus programs: {}",
        programs.len()
    );

    // Builds run against the repo's std (the goldens were generated with it),
    // in parallel chunks — each program is an independent compile.
    let failures: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = programs
            .chunks(programs.len().div_ceil(8).max(1))
            .map(|chunk| {
                let work = &work;
                let corpus = &corpus;
                scope.spawn(move || {
                    let mut failures = Vec::new();
                    for name in chunk {
                        let source = work.join(name);
                        let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
                            .arg("build")
                            .arg(&source)
                            .env("VILAN_STD", std_dir())
                            .output()
                            .expect("run vilan build");
                        if !output.status.success() {
                            failures.push(format!(
                                "{name}: build failed:\n{}",
                                String::from_utf8_lossy(&output.stderr)
                            ));
                            continue;
                        }
                        for asset in ["js", "css"] {
                            let golden_path = corpus.join(name).with_extension(asset);
                            if !golden_path.is_file() {
                                continue;
                            }
                            let golden =
                                std::fs::read_to_string(&golden_path).expect("read golden");
                            let rebuilt = std::fs::read_to_string(source.with_extension(asset))
                                .unwrap_or_default();
                            if golden != rebuilt {
                                failures.push(format!(
                                    "{name} (.{asset}): {}",
                                    first_difference(&golden, &rebuilt)
                                ));
                            }
                        }
                    }
                    failures
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("corpus worker"))
            .collect()
    });
    let _ = std::fs::remove_dir_all(&work);
    assert!(
        failures.is_empty(),
        "{} corpus golden(s) diverged:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The equivalence-gate rationale for HMR (A13, `hmr.md` §5): the `build` path
/// never sets `BuildOptions.hmr`, so no corpus golden may carry the watch-only
/// instrumentation (`__hmr_adopt*` / `__hmr_expose`). The runtime *guard*
/// (`__hmr_active`) and the guarded std hooks (`__hmr_register_teardown` &c.)
/// are deliberately NOT swept: they appear in plain builds of programs using
/// `mount_root`/`connect_socket`/`std::dev` and no-op without a shim — a
/// future corpus golden may legitimately carry them.
#[test]
fn no_corpus_golden_carries_hmr_instrumentation() {
    let corpus = corpus_dir();
    let watch_only = ["__hmr_adopt", "__hmr_expose"];
    let mut checked = 0usize;
    for entry in std::fs::read_dir(&corpus).expect("corpus directory") {
        let path = entry.expect("corpus entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("js") {
            continue;
        }
        let golden = std::fs::read_to_string(&path).expect("read golden");
        for symbol in watch_only {
            assert!(
                !golden.contains(symbol),
                "{path:?} carries `{symbol}` but was built off the `build` path"
            );
        }
        checked += 1;
    }
    assert!(checked > 60, "suspiciously few goldens swept: {checked}");
}

/// WO-1b: the emitted JS must not depend on the order of a file's import
/// STATEMENTS (which modules load and in what order). `vilan fmt` sorts imports
/// canonically (WO-1); before this change a pure import-statement reorder churned
/// the JS bytes, because module load order — and so entity-id assignment, and so
/// the id-sorted declaration emission — was LIFO of import order. The analyzer
/// now drains modules in a canonical order (`analyzer.rs`, `load_order_key`), so
/// permuting the import statements compiles to identical bytes.
///
/// SCOPE: this pins the module-walk mechanism only. A DISTINCT sensitivity —
/// the emission order of imported module-level CONSTANTS, which follows the
/// order names are listed inside a `{ .. }` brace set (`module_level_bindings`
/// iterates the entry scope's insertion-ordered `IndexMap`) — is deliberately
/// NOT exercised here: it is unaddressed by WO-1b and cannot be fixed by simply
/// sorting globals by id (that reorders semantically-significant, non-hoisted
/// `const` declarations and miscompiles a cross-module module-level dependency
/// into a TDZ error). So this test permutes only whole import statements, never
/// names within a brace set.
///
/// Non-vacuous by construction: `std::base64` and `std::display` both sit
/// OUTSIDE the always-loaded prelude closure (unlike `std::bytes`, which
/// `std::json` pulls in transitively, fixing its load order regardless of the
/// entry's imports). With two such modules present, their relative load order —
/// and the order of the helper functions they emit — did depend on import order
/// under the old LIFO drain (a measured 6-line churn). Reverting the drain to
/// LIFO fails this test.
#[test]
fn emitted_js_is_independent_of_import_order() {
    // A shared program body; only the leading import block's order differs
    // between the two variants. `encode_url`/`encode_utf8`/`format` keep every
    // module's functions reachable (and thus emitted).
    let body =
        "\n\nfun main() {\n\tprint(encode_url(encode_utf8(\"vilan\")));\n\tprint(format(42));\n}\n";
    let print_import = "import std::print;\n";
    let bytes_import = "import std::bytes::{ encode_utf8 };\n";
    let base64_import = "import std::base64::{ encode_url };\n";
    let display_import = "import std::display::{ format };\n";
    let order_a = format!("{print_import}{bytes_import}{base64_import}{display_import}{body}");
    // A genuine shuffle of the same four imports.
    let order_b = format!("{display_import}{base64_import}{print_import}{bytes_import}{body}");

    let work = std::env::temp_dir().join(format!("vilan_import_order_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    // Same basename in separate directories, so nothing but the import order
    // varies (the emitted JS embeds no source path — verified: identical dirs
    // produce identical bytes).
    let build = |variant: &str, source: &str| -> String {
        let dir = work.join(variant);
        std::fs::create_dir_all(&dir).expect("create work dir");
        let src = dir.join("prog.vl");
        std::fs::write(&src, source).expect("write source");
        let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .arg("build")
            .arg(&src)
            .env("VILAN_STD", std_dir())
            .output()
            .expect("run vilan build");
        assert!(
            output.status.success(),
            "build failed for variant {variant}:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
        std::fs::read_to_string(src.with_extension("js")).expect("read emitted js")
    };
    let js_a = build("a", &order_a);
    let js_b = build("b", &order_b);
    let _ = std::fs::remove_dir_all(&work);
    assert_eq!(
        js_a, js_b,
        "emitted JS differs under an import reorder — the module walk order is not canonical"
    );
}
