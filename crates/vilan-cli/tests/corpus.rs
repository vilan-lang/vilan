//! The corpus byte gate (backlog E5): every `vilan/test/*.vl` with a `.mjs`
//! golden compiles — via the CURRENT `vilan` binary, exactly the command that
//! generated the goldens — to byte-identical output (`.css` assets included).
//!
//! This replaces the by-hand loop (rebuild the debug binary, regenerate,
//! `git diff`) that the golden-regen discipline existed to police: the binary
//! under test is always the one Cargo just built from this tree, so a stale
//! binary can no longer write or check goldens. A deliberate output change
//! still regenerates goldens by hand; this gate then verifies the commit.
//!
//! # What belongs in the corpus (tracker N51)
//!
//! **A corpus program TERMINATES, and its output is the claim.** This gate reads
//! the bytes a program compiles to, and the `// witness:` rule below makes the
//! load-bearing ones nameable — but the differentials over the same directory
//! (`vilan-core/tests/{release,infer}_differential.rs`) read what it PRINTS,
//! and that is the claim the corpus rests on: two builds of one program are the
//! same program if and only if they print the same thing. A program that never
//! exits has no stdout for anything to compare, so it is not a weaker corpus
//! entry, it is not one at all — and its cost is paid whether or not anyone
//! notices, since a runner has nothing to wait for but its own deadline.
//! `watch.vl` was exactly that for as long as it existed: it blocked on
//! `flat.next()` for a change nothing ever made, and when the release
//! differential first ran it, both builds were killed at 300 s and the gate
//! compared two identical "node did not exit" strings and passed — 600 s of a
//! 607 s critical path spent on a verdict that could not come out any other way.
//! It now makes its own bounded change and observes it (`created probe.txt`,
//! `modified probe.txt`, exit 0, ~1 s at loadavg 126); the endless form is
//! `vilan/examples/watch`, where a program that runs until you stop it is the
//! thing being shown.
//!
//! The rule does not ask a program to be FAST, and shortening a wait to make one
//! terminate is the other disease (E32: the observation becomes "what had it
//! printed when we gave up", which load decides). It asks the program to reach
//! its own end on its own — to make the event it waits for, or to stop waiting
//! for one. Programs whose output is not a function of their source alone — a
//! clock, a random draw, the host environment — are still corpus programs and
//! are compiled by every gate; they simply do not reach the node leg
//! (`corpus_harness::NOT_RUN` names them, with the reason each).

use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The extension a corpus golden carries. Corpus programs are bare files with
/// no manifest, so `vilan build` compiles them for the default platform (Node)
/// and writes `.mjs` — the process legs take the extension that declares ESM to
/// the runtime rather than leaving it to be sniffed (`top-level-await.md` §8.1).
const GOLDEN_EXTENSION: &str = "mjs";

/// Copies `from` into `to`, whole. The corpus's own resource trees are small
/// and shallow; a symlink is followed like any other entry, because
/// `std::fs::copy` follows one and the corpus has none.
fn stage_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create a staged directory");
    for entry in std::fs::read_dir(from).expect("read a corpus subdirectory") {
        let path = entry.expect("a corpus subdirectory entry").path();
        let name = path.file_name().expect("an entry has a name");
        if path.is_dir() {
            stage_tree(&path, &to.join(name));
        } else {
            std::fs::copy(&path, to.join(name)).expect("stage a corpus resource");
        }
    }
}

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
    // A full copy: corpus programs may import sibling modules — and may bundle
    // sibling RESOURCES — and building in place would overwrite the goldens
    // under comparison.
    let work = std::env::temp_dir().join(format!("vilan_corpus_gate_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&work);
    std::fs::create_dir_all(&work).expect("create corpus work dir");
    let mut programs: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&corpus).expect("corpus directory") {
        let path = entry.expect("corpus entry").path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        // A corpus program may ENUMERATE a directory (`asset::read_dir_all`,
        // kolt.local 035), so a subdirectory travels whole. Without this it
        // would not travel at all — it has no extension, so the filter below
        // dropped it — and the listing would come back empty in the work dir,
        // diverging the golden on a difference that is the staging's rather
        // than the compiler's.
        if path.is_dir() {
            stage_tree(&path, &work.join(name));
            continue;
        }
        let Some(extension) = path.extension() else {
            continue;
        };
        // Goldens stay put; everything else is staged. Non-`.vl` files are
        // staged because a corpus program may DEPEND on one:
        // `const asset::bundle` names a resource relative to the package root,
        // which for a bare corpus file is the directory it is copied into
        // (kolt.local 029). A resource that did not travel would fail the
        // build rather than diverge a golden.
        if extension == GOLDEN_EXTENSION || extension == "css" {
            continue;
        }
        std::fs::copy(&path, work.join(name)).expect("stage a corpus file");
        if extension == "vl" && path.with_extension(GOLDEN_EXTENSION).is_file() {
            programs.push(name.to_string());
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
                        for asset in [GOLDEN_EXTENSION, "css"] {
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

/// The directive a corpus program uses to name the bytes that carry a claim.
const WITNESS_DIRECTIVE: &str = "// witness:";

/// The negative form: bytes a claim says are ABSENT.
const WITNESS_ABSENT_DIRECTIVE: &str = "// witness-absent:";

/// The shortest a normalized POSITIVE witness may be. A one- or two-character
/// witness (`(`, `;`) is in every golden and therefore pins nothing; the floor
/// stops a directive from being written down as a no-op.
///
/// It applies to the positive form only, because the strength argument inverts
/// for the negative one: a SHORT absent-witness is the harder claim (more
/// goldens contain `finally` than contain `} finally { $a(r); }`), so the
/// short-is-vacuous reasoning does not carry over, and `resource_exit.vl`'s
/// seven-character `finally` is the strongest form of its own sentence.
const SHORTEST_WITNESS: usize = 8;

/// Collapses every run of ASCII whitespace to one space and trims, so a witness
/// may be written on one comment line and still match bytes that the emitter
/// spread over several indented lines.
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every `// witness:` / `// witness-absent:` line in `source`, normalized, as
/// `(line number, present, witness)`.
fn witnesses(source: &str) -> Vec<(usize, bool, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let trimmed = line.trim_start();
            // The two prefixes diverge before either colon (`witness:` against
            // `witness-`), so neither is a prefix of the other and this order is
            // a readability choice rather than a correctness one.
            let (present, rest) = if let Some(rest) = trimmed.strip_prefix(WITNESS_ABSENT_DIRECTIVE)
            {
                (false, rest)
            } else {
                (true, trimmed.strip_prefix(WITNESS_DIRECTIVE)?)
            };
            Some((index + 1, present, normalize_whitespace(rest)))
        })
        .collect()
}

/// Audit run 7's F7/F8 rule, mechanized: a corpus program's prose claim is
/// checked against the bytes it claims.
///
/// The finding that minted this: `resource.vl` said "Two locals drop in reverse
/// declaration order at the scope end" and, since c8609287 moved disposal from
/// the scope end to the LAST USE, neither local was read — so both dropped at
/// their declaration, in declaration order, and the golden beside the sentence
/// proved the opposite of it. The inference twins were adjusted in that commit;
/// the corpus twins were not. Nothing was red, because a golden gate checks that
/// the bytes are the bytes the compiler emits, never that they are the bytes the
/// comment above them describes.
///
/// A `// witness:` line closes that gap by making the load-bearing bytes
/// nameable. It is deliberately NOT a second copy of the golden: a witness names
/// the FRAGMENT a claim rests on — the `finally` that has to close after the
/// last statement, the drop that has to precede the write — so a regeneration
/// that moves that fragment fails here with the claim's own words beside it,
/// where the byte gate would only have said "regenerate". The negative form
/// (`// witness-absent:`) carries the claims that are about an emission NOT
/// happening, which `resource_exit.vl`'s "no `finally` in the emitted bytes at
/// all" is one of and which no positive substring can express.
///
/// Matching is whitespace-normalized on both sides, so a witness fits on one
/// comment line and still spans the emitter's indented multi-line shapes. It is
/// substring containment rather than a regex: a witness should be readable as
/// the JS it names.
///
/// The rejected weaker design, recorded because it looks adequate: "every corpus
/// program's leading comment mentions a token the emitted JS contains". It is
/// vacuous against the very finding it would answer — `resource.vl`'s stale
/// header mentioned "drop", the golden contains `drop`, and the check stays
/// green through the whole regression. A claim is only checkable when the
/// program says WHICH bytes are the claim.
///
/// The floors below keep the mechanism from rotting to nothing while it is still
/// being adopted file by file; they rise as programs are annotated.
#[test]
fn every_declared_witness_is_in_its_golden() {
    const FEWEST_ANNOTATED_PROGRAMS: usize = 2;
    const FEWEST_WITNESSES: usize = 12;

    let corpus = corpus_dir();
    let mut annotated = 0usize;
    let mut total = 0usize;
    let mut failures: Vec<String> = Vec::new();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("corpus directory")
        .map(|entry| entry.expect("corpus entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.extension().and_then(|extension| extension.to_str()) != Some("vl") {
            continue;
        }
        let golden_path = path.with_extension(GOLDEN_EXTENSION);
        let Ok(golden) = std::fs::read_to_string(&golden_path) else {
            continue;
        };
        let source = std::fs::read_to_string(&path).expect("read a corpus program");
        let declared = witnesses(&source);
        if declared.is_empty() {
            continue;
        }
        annotated += 1;
        total += declared.len();
        let normalized_golden = normalize_whitespace(&golden);
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string();
        for (line, present, witness) in declared {
            if present && witness.len() < SHORTEST_WITNESS {
                failures.push(format!(
                    "{name}:{line}: witness {witness:?} is shorter than {SHORTEST_WITNESS} \
                     characters, so it pins nothing"
                ));
                continue;
            }
            if witness.is_empty() {
                failures.push(format!("{name}:{line}: the witness is empty"));
                continue;
            }
            if normalized_golden.contains(&witness) != present {
                let complaint = if present {
                    "is not in"
                } else {
                    "is in (and the claim above says it is absent from)"
                };
                failures.push(format!(
                    "{name}:{line}: the declared witness {witness:?} {complaint} \
                     {} — the claim above it is unwitnessed. Fix the program until \
                     the claim holds, or correct the claim; do NOT relax the \
                     witness to whatever the golden happens to say.",
                    golden_path.display()
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} corpus witness(es) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert!(
        annotated >= FEWEST_ANNOTATED_PROGRAMS && total >= FEWEST_WITNESSES,
        "the witness mechanism has rotted: {annotated} annotated program(s) \
         (floor {FEWEST_ANNOTATED_PROGRAMS}) carrying {total} witness(es) \
         (floor {FEWEST_WITNESSES})"
    );
}

/// The witness gate's own parsing, pinned: the two directives are told apart,
/// whitespace normalization spans lines, and a bare `//` comment is not a
/// witness.
#[test]
fn witness_directives_parse() {
    let source = "// a claim\n\
                  // witness: } finally {\t$a(a);\n\
                  \t// witness-absent: finally\n\
                  // witnessing something is not a directive\n";
    let parsed = witnesses(source);
    assert_eq!(
        parsed,
        vec![
            (2, true, "} finally { $a(a);".to_string()),
            (3, false, "finally".to_string()),
        ],
        "witness parsing changed"
    );
    assert_eq!(
        normalize_whitespace("\ttry {\n\t\t$a(r);\n"),
        "try { $a(r);",
        "whitespace normalization must let a one-line witness span emitted lines"
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
        if path.extension().and_then(|extension| extension.to_str()) != Some(GOLDEN_EXTENSION) {
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

/// The emitted JS must not depend on how a file SPELLS its imports — neither
/// the order of the import STATEMENTS (which modules load, and in what order)
/// nor the order of the names inside a `{ .. }` brace set. `vilan fmt` sorts
/// both canonically (WO-1), so a reformat must never change a program's bytes.
///
/// Two mechanisms had to fall for that to hold, and this pins both:
///
/// - **The module walk** (WO-1b). Load order was LIFO of import order, and it
///   decided entity-id assignment and so function emission. `analyzer.rs`'s
///   `load_order_key` now drains modules canonically.
/// - **Module-level binding emission** (B33). Emission used to hand the
///   transformer `Program::module_level_bindings()`, whose first half is the
///   entry scope's insertion-ordered `IndexMap` — import statement order ×
///   brace order. So the declaration order of imported CONSTANTS was a
///   spelling detail, and since a module-level `let` emits a non-hoisted
///   `const`, a *dependency* between two of them could land the wrong way
///   round and TDZ-crash at load. Emission now runs
///   `init_order::initialization_order`: a topological sort of the load-time
///   relation, ties broken by the canonical key.
///
/// The reasoning trail, because it is the reason B33 was not a one-liner:
/// sorting the globals by id ALONE does not work. Canonical load order walks
/// module names in order, so a module whose binding is *depended on* can load
/// second and get the higher id — `alpha.vl`'s `let A = Z * 2;` against
/// `zeta.vl`'s `let Z = 21;` — and an id sort then emits `A` first, which is
/// exactly the TDZ miscompile. The topological sort is what makes the
/// canonical tie-break safe; the `inference` suite's
/// `a_dependency_in_a_later_loading_module_is_declared_first` pins that shape.
///
/// Non-vacuous by construction, in both halves:
///
/// - `std::base64` and `std::display` both sit OUTSIDE the always-loaded
///   prelude closure (unlike `std::bytes`, which `std::json` pulls in
///   transitively, fixing its load order regardless of the entry's imports).
///   With two such modules present, their relative load order — and the order
///   of the helper functions they emit — depended on import-statement order
///   under the old LIFO drain (a measured 6-line churn). Reverting the drain to
///   LIFO fails this test.
/// - The `std::math` brace set carries six module-level constants, permuted
///   between the variants, and the program declares one of its own that reads
///   `TAU`. Reverting emission to `module_level_bindings()` order fails this
///   test (verified: the six `const` declarations churn to match each variant's
///   brace order).
#[test]
fn emitted_js_is_independent_of_import_order() {
    // A shared program body; only the leading import block differs between the
    // two variants — statement order AND the order inside `std::math`'s brace
    // set. `encode_url`/`encode_utf8`/`format` keep every module's functions
    // reachable (and thus emitted); the six constants are all read, so all six
    // are emitted; and `QUARTER_TURN` gives the entry a module-level binding
    // that DEPENDS on an imported one (`TAU` must be declared before it in
    // either spelling).
    let body = "\nlet QUARTER_TURN: f64 = TAU / 4f;\n\n\
                fun main() {\n\
                \tprint(encode_url(encode_utf8(\"vilan\")));\n\
                \tprint(format(42));\n\
                \tprint(PI + E + EPSILON + QUARTER_TURN);\n\
                \tprint(INFINITY.is_infinite() && NAN.is_nan());\n}\n";
    let print_import = "import std::io::print;\n";
    let bytes_import = "import std::bytes::{ encode_utf8 };\n";
    let base64_import = "import std::base64::{ encode_url };\n";
    let display_import = "import std::display::{ format };\n";
    let math_import_sorted = "import std::math::{ E, EPSILON, INFINITY, NAN, PI, TAU };\n";
    // The same six names, shuffled inside the brace set.
    let math_import_shuffled = "import std::math::{ TAU, NAN, PI, INFINITY, E, EPSILON };\n";
    let order_a = format!(
        "{math_import_sorted}{print_import}{bytes_import}{base64_import}{display_import}{body}"
    );
    // A genuine shuffle of the same five imports, and of the brace set.
    let order_b = format!(
        "{display_import}{base64_import}{print_import}{bytes_import}{math_import_shuffled}{body}"
    );

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
        std::fs::read_to_string(src.with_extension(GOLDEN_EXTENSION)).expect("read emitted js")
    };
    let js_a = build("a", &order_a);
    let js_b = build("b", &order_b);
    let _ = std::fs::remove_dir_all(&work);
    assert_eq!(
        js_a, js_b,
        "emitted JS differs under an import reorder — either the module walk is no \
         longer canonical (function order churns) or module-level bindings are no \
         longer emitted in initialization order (`const` order churns)"
    );
}

/// E44 — a corpus build is byte-deterministic under CONCURRENCY.
///
/// Two concurrent corpus runs sharing a box once emitted different JavaScript
/// for `reactive`, `reactive-flatten` and `signal-update` (`$E` against `$G`,
/// `new2` against `fresh_id`) while sequential runs stayed byte-stable, and the
/// filed suspicion was that scheduling reached inference order and so the type
/// ids that monomorphization keys on (B95).
///
/// It cannot. A `vilan build` is single-threaded — the CLI's only threads are
/// the HMR server's, and the compile path reads a clock for timing reports and
/// nothing else — so no ambient scheduling decision is an input to it. The
/// compile is a pure function of (binary, source, `$VILAN_STD`, environment,
/// working directory); its temp and cache paths are process-namespaced or
/// written by atomic rename. What DOES differ per process is the `HashMap` seed,
/// which E38 established does not reach the output, and this pin re-establishes
/// it for free: every worker below is a separate process with its own seed.
///
/// Measured before this pin existed: 216 concurrent builds (6 processes x 12
/// rounds x the 3 named programs) on a loaded box, byte-identical, both with the
/// id-keyed instance identity and with B95's structural one. The observed
/// difference was therefore an INPUT difference, and the only input that moves
/// while a box is being worked on is the binary itself: the harness resolves
/// `CARGO_BIN_EXE_vilan`, a fixed path, which any concurrent `cargo build` in
/// the same lane rewrites underneath a run in progress. That is the trap
/// `CLAUDE.md` already states — "a run started mid-editing tests a tree that no
/// longer exists" — and the three programs are simply the corpus's most
/// sensitive: a one-instance difference anywhere in `std::reactive` shifts every
/// generated name after it, which is exactly the reported shape.
///
/// The pin holds the property rather than the diagnosis: whatever the mechanism,
/// concurrent builds of one source must agree.
#[test]
fn concurrent_builds_of_one_program_agree_byte_for_byte() {
    // The corpus's most name-sensitive programs — the three E44 named.
    const PROGRAMS: [&str; 3] = ["reactive", "reactive-flatten", "signal-update"];
    const WORKERS: usize = 4;

    let corpus = corpus_dir();
    let root =
        std::env::temp_dir().join(format!("vilan_corpus_concurrency_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    // A private copy per worker — corpus programs import sibling modules, and
    // two workers writing one directory would race on the output, not on the
    // compiler.
    for worker in 0..WORKERS {
        let directory = root.join(worker.to_string());
        std::fs::create_dir_all(&directory).expect("create the worker directory");
        for entry in std::fs::read_dir(&corpus).expect("corpus directory") {
            let path = entry.expect("corpus entry").path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("vl") {
                let name = path.file_name().expect("a corpus file has a name");
                std::fs::copy(&path, directory.join(name)).expect("copy corpus source");
            }
        }
    }

    let emissions: Vec<Vec<(String, String)>> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..WORKERS)
            .map(|worker| {
                let root = &root;
                scope.spawn(move || {
                    let directory = root.join(worker.to_string());
                    PROGRAMS
                        .iter()
                        .map(|name| {
                            let source = directory.join(format!("{name}.vl"));
                            let output = Command::new(env!("CARGO_BIN_EXE_vilan"))
                                .arg("build")
                                .arg(&source)
                                .env("VILAN_STD", std_dir())
                                .output()
                                .expect("run vilan build");
                            assert!(
                                output.status.success(),
                                "{name} failed to build in worker {worker}:\n{}",
                                String::from_utf8_lossy(&output.stderr)
                            );
                            let emitted = source.with_extension(GOLDEN_EXTENSION);
                            (
                                (*name).to_string(),
                                std::fs::read_to_string(&emitted).expect("read the emission"),
                            )
                        })
                        .collect()
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| worker.join().expect("concurrency worker"))
            .collect()
    });
    let _ = std::fs::remove_dir_all(&root);

    let reference = emissions.first().expect("at least one worker");
    for (worker, emission) in emissions.iter().enumerate().skip(1) {
        for ((name, mine), (_, theirs)) in emission.iter().zip(reference) {
            assert!(
                mine == theirs,
                "worker {worker} emitted different bytes for {name} than worker 0: {}",
                first_difference(theirs, mine)
            );
        }
    }
}
