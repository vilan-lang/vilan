//! End-to-end pins for how a source file's ON-DISK encoding is read
//! (windows-support.md §2, spec §2): a leading U+FEFF byte-order mark is not
//! source text, and a `\r\n` is one line terminator. Both are properties of the
//! bytes on disk, so they can only be pinned through the real binary reading a
//! real file — the in-process pins in `vilan-core` work from `&str`.
//!
//! A Windows editor writes both by default, so these are exactly the files a
//! Windows contributor produces.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// A fresh temp directory for one test's project tree.
fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_encoding_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("vilan.toml"),
        "[package]\nname = \"encoding\"\nroot = \".\"\n",
    )
    .unwrap();
    dir
}

/// Writes `bytes` to `dir/name` verbatim — the point of these tests is the exact
/// bytes, so nothing may normalize them on the way in.
fn write_bytes(dir: &Path, name: &str, bytes: &[u8]) {
    std::fs::write(dir.join(name), bytes).unwrap();
}

/// Runs the `vilan` binary in `dir`. `NO_COLOR` is set so the assertions below
/// read plain text: a piped child is already non-TTY today, but that is the
/// colour gate's business, not this file's.
fn vilan(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .env("NO_COLOR", "1")
        .args(args)
        .output()
        .expect("run vilan")
}

/// Everything the CLI wrote, both streams.
fn combined(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Removes a test's tree on success (a failing test keeps it for inspection —
/// the assertion panics before this runs), so a suite run leaks nothing.
fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

const BOM: &[u8] = b"\xef\xbb\xbf";

/// The CRLF twin of an LF source.
fn crlf(source: &str) -> Vec<u8> {
    source.replace('\n', "\r\n").into_bytes()
}

#[test]
fn a_bom_prefixed_file_compiles_and_spans_its_first_line_correctly() {
    // Before this rule the BOM lexed as an illegal character and the file did
    // not compile at all. Now it is ignored, and — the point — columns are
    // counted from the byte AFTER it, which is what VS Code (which strips the
    // BOM before sending the buffer) has always assumed.
    let dir = temp_project("bom_span");
    let source = "import std::nonexistent_module;\n\nfun main() {\n}\n";
    write_bytes(&dir, "main.vl", &[BOM, source.as_bytes()].concat());

    let output = vilan(&dir, &["build", "main.vl", "--stdout"]);
    let text = combined(&output);
    assert!(
        !text.contains("feff") && !text.contains("expected a token"),
        "the BOM must not reach the lexer: {text}"
    );
    // `import std::` is 12 bytes, so the offending name starts at column 13 —
    // the BOM's three bytes are not counted.
    assert!(
        text.contains("main.vl:1:13"),
        "line-1 columns must ignore the BOM: {text}"
    );
    cleanup(&dir);
}

#[test]
fn a_bom_prefixed_crlf_file_compiles_like_its_plain_twin() {
    // The two Windows-editor defaults together: BOM *and* CRLF. The emitted
    // bundle must be byte-identical to the plain LF file's.
    let dir = temp_project("bom_crlf");
    let source = "import std::print;\n\nfun main() {\n\tlet text = \"\"\"\n\talpha\n\tbeta\n\t\"\"\";\n\tprint(text);\n}\n\nmain();\n";
    write_bytes(&dir, "plain.vl", source.as_bytes());
    write_bytes(&dir, "windows.vl", &[BOM, &crlf(source)].concat());

    let plain = vilan(&dir, &["build", "plain.vl", "--stdout"]);
    let windows = vilan(&dir, &["build", "windows.vl", "--stdout"]);
    assert!(plain.status.success(), "{}", combined(&plain));
    assert!(windows.status.success(), "{}", combined(&windows));
    assert_eq!(
        plain.stdout, windows.stdout,
        "a BOM'd CRLF file must emit the same bundle as its plain twin"
    );
    assert!(
        !windows.stdout.contains(&b'\r'),
        "emitted JavaScript carries no carriage return"
    );
    cleanup(&dir);
}

#[test]
fn a_crlf_file_runs_with_lf_string_values() {
    // The miscompile, observed at RUNTIME rather than in the bundle text: the
    // program prints two lines, not one line ending in a stray carriage return.
    let dir = temp_project("crlf_run");
    let source = "import std::print;\n\nfun main() {\n\tprint(\"\"\"\n\talpha\n\tbeta\n\t\"\"\");\n}\n\nmain();\n";
    write_bytes(&dir, "main.vl", &crlf(source));

    let output = vilan(&dir, &["run", "main.vl"]);
    assert!(output.status.success(), "{}", combined(&output));
    assert_eq!(
        String::from_utf8_lossy(&output.stdout),
        "alpha\nbeta\n",
        "a CRLF source's string value carries LF"
    );
    cleanup(&dir);
}

#[test]
fn fmt_converts_a_crlf_file_to_lf_exactly_once() {
    // Canonical Vilan is LF (windows-support.md §2 (b)): converting is a correct
    // reformat. Two files that differ ONLY in line endings must end up identical
    // on disk, and the second run must report nothing left to do.
    let dir = temp_project("fmt_crlf");
    let source = "import std::print;\n\nfun main() {\n\tlet text = \"\"\"\n\talpha\n\tbeta\n\t\"\"\";\n\tprint(text);\n}\n";
    write_bytes(&dir, "canonical.vl", source.as_bytes());
    write_bytes(&dir, "windows.vl", &crlf(source));

    // `--check` sees the CRLF file as needing a reformat…
    let check = vilan(&dir, &["fmt", "--check", "windows.vl"]);
    assert!(
        !check.status.success(),
        "a CRLF file is not canonically formatted: {}",
        combined(&check)
    );

    let first = vilan(&dir, &["fmt", "windows.vl"]);
    assert!(first.status.success(), "{}", combined(&first));
    let formatted = std::fs::read(dir.join("windows.vl")).unwrap();
    assert!(
        !formatted.contains(&b'\r'),
        "formatted output carries no carriage return"
    );
    assert_eq!(
        formatted,
        std::fs::read(dir.join("canonical.vl")).unwrap(),
        "the CRLF file converges on its LF twin"
    );

    // …and once. A second pass changes nothing.
    let second = vilan(&dir, &["fmt", "--check", "windows.vl"]);
    assert!(
        second.status.success(),
        "the conversion is idempotent: {}",
        combined(&second)
    );
    assert_eq!(formatted, std::fs::read(dir.join("windows.vl")).unwrap());
    cleanup(&dir);
}

#[test]
fn a_crlf_module_import_compiles_like_its_lf_twin() {
    // Not just the entry file: a `pkg::` module is read through the analyzer's
    // own loader, a different read site from the CLI's entry read.
    let dir = temp_project("crlf_module");
    let entry =
        "import pkg::helper::shout;\nimport std::print;\n\nfun main() {\n\tprint(shout());\n}\n";
    let helper = "fun shout(): str {\n\t\"\"\"\n\tfirst\n\tsecond\n\t\"\"\"\n}\n";
    write_bytes(&dir, "main.vl", entry.as_bytes());

    write_bytes(&dir, "helper.vl", helper.as_bytes());
    let plain = vilan(&dir, &["build", "main.vl", "--stdout"]);
    assert!(plain.status.success(), "{}", combined(&plain));

    write_bytes(&dir, "helper.vl", &[BOM, &crlf(helper)].concat());
    let windows = vilan(&dir, &["build", "main.vl", "--stdout"]);
    assert!(windows.status.success(), "{}", combined(&windows));

    assert_eq!(
        plain.stdout, windows.stdout,
        "an imported module's encoding must not change the bundle"
    );
    cleanup(&dir);
}

#[test]
fn a_bom_prefixed_manifest_builds_like_its_clean_twin() {
    // `vilan.toml` is read like any other file a Windows editor may save with
    // a BOM. Stripping happens at `Manifest::parse`, the choke point every
    // reader goes through (windows-support.md §2). A GUARD: `toml` 0.8 already
    // tolerates a leading BOM (measured), so this pins the end-to-end
    // guarantee rather than a failure observed today.
    let clean = temp_project("manifest_clean");
    let marked = temp_project("manifest_bom");
    let manifest = b"[package]\nname = \"encoding\"\nroot = \".\"\ntarget = \"node\"\n";
    write_bytes(&clean, "vilan.toml", manifest);
    write_bytes(&marked, "vilan.toml", &[BOM, manifest.as_slice()].concat());

    let source = "import std::print;\n\nfun main() {\n\tprint(\"hi\");\n}\n";
    write_bytes(&clean, "main.vl", source.as_bytes());
    write_bytes(&marked, "main.vl", source.as_bytes());

    let plain = vilan(&clean, &["build", "main.vl", "--stdout"]);
    let windows = vilan(&marked, &["build", "main.vl", "--stdout"]);
    assert!(plain.status.success(), "{}", combined(&plain));
    assert!(
        windows.status.success(),
        "a BOM'd vilan.toml must build: {}",
        combined(&windows)
    );
    assert_eq!(
        plain.stdout, windows.stdout,
        "a BOM'd manifest must produce the same bundle as its clean twin"
    );
    cleanup(&clean);
    cleanup(&marked);
}
