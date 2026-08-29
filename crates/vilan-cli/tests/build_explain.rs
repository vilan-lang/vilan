//! `vilan build --explain` — every `dist/` output names its contributors
//! (backlog G11).
//!
//! The complaint the verb answers is that the const channel's contributions are
//! scattered across files *by construction* — that scatter IS import-driven
//! composition — so "where did this `dist/` file come from?" was a grep, and for
//! an accumulated kind a grep over every `emit` site. The compiler held the
//! answer the whole time: every channel call knows its `const` site, the flush
//! knows which kind each line landed in, the copy knows what it carried, and the
//! hook runner knows what ran and what was `Fresh`.
//!
//! **One project, the whole surface, one build.** Tier-agnostic coverage is the
//! point of the feature, so it is the point of the fixture: an accumulated kind
//! with two contributing sites, an `asset::bundle`, an `asset::bundle_as` whose
//! target is nothing like its path, an `asset::digest` whose input reaches no
//! `dist/` file except the bundle it is compiled into, a `[[build.hook]]` with
//! declared `inputs`/`outputs`, and two legs so the report has to keep one
//! leg's sites off the other leg's files. A pin per property over six fixtures
//! would test six builds that never happen; this tests the one that does.
//!
//! The negative is half the feature: a plain `vilan build` prints not one line
//! of it. `--explain` is a verb that READS and SAYS, and a build that started
//! narrating itself unasked would be a different (and unwanted) change.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

/// The resource `asset::bundle` carries under its own path.
const ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"><circle r=\"4\"/></svg>\n";

/// The resource `asset::bundle_as` carries to a url its path does not spell.
const LOGO: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"><rect width=\"8\"/></svg>\n";

/// The file `asset::digest` fingerprints. Nothing copies it, so the only
/// `dist/` file it can move is the bundle its digest is compiled into — which
/// is exactly the line a report that only followed emits and copies would miss.
const VERSION: &str = "2026.8.29\n";

fn temp_project(tag: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!(
        "vilan_explain_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write(dir: &Path, relative: &str, contents: &str) {
    let path = dir.join(relative);
    std::fs::create_dir_all(path.parent().expect("a parent")).expect("create the directory");
    std::fs::write(path, contents).expect("write the file");
}

/// `NO_COLOR=1` so every line can be asserted as literal text.
fn vilan(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan")
}

/// A hook command that writes one line to `file`, in the platform's shell.
/// Kept at the project root so no separator has to survive `cmd`'s redirect.
fn write_line(file: &str, text: &str) -> String {
    if cfg!(windows) {
        format!("echo {text}> {file}")
    } else {
        format!("printf '{text}\\n' > {file}")
    }
}

/// The whole surface in one project: two legs, an accumulated kind with two
/// contributing sites, both bundle spellings, a digest, and a declared hook.
///
/// The two `emit("css", …)` calls sit in two different functions reached from
/// two different `const` sites, which is what makes the kind's block have two
/// contributors — the shape a grep over `emit` was the only way to find.
fn stage(tag: &str) -> PathBuf {
    let dir = temp_project(tag);
    write(
        &dir,
        "vilan.toml",
        &format!(
            "[package]\nname = \"explained\"\n\n\
             [entry.client]\ntarget = \"browser\"\n\n\
             [entry.server]\n\n\
             [[build.hook]]\n\
             name    = \"stamp\"\n\
             run     = \"{}\"\n\
             inputs  = [\"seed.txt\"]\n\
             outputs = [\"stamped.txt\"]\n",
            write_line("stamped.txt", "stamped")
        ),
    );
    write(
        &dir,
        "src/client.vl",
        "import std::asset::{ bundle, bundle_as, digest, emit };\n\
         import std::io::print;\n\
         \n\
         fun base(): i32 {\n\
         \temit(\"css\", \".base{color:red}\");\n\
         \t1\n\
         }\n\
         \n\
         fun accent(): i32 {\n\
         \temit(\"css\", \".accent{color:blue}\");\n\
         \t2\n\
         }\n\
         \n\
         let _base = const base();\n\
         let _accent = const accent();\n\
         let icon = const bundle(\"static/icon.svg\");\n\
         let logo = const bundle_as(\"static/logo.svg\", \"/brand/logo.svg\");\n\
         let stamp = const digest(\"data/version.txt\");\n\
         \n\
         fun main() {\n\
         \tprint(icon);\n\
         \tprint(logo);\n\
         \tprint(stamp);\n\
         }\n\
         main();\n",
    );
    write(
        &dir,
        "src/server.vl",
        "import std::io::print;\n\nfun main() {\n\tprint(\"server\");\n}\nmain();\n",
    );
    write(&dir, "src/static/icon.svg", ICON);
    write(&dir, "src/static/logo.svg", LOGO);
    write(&dir, "src/data/version.txt", VERSION);
    write(&dir, "seed.txt", "seed\n");
    dir
}

fn build(dir: &Path, extra: &[&str]) -> Output {
    let mut args = vec!["build", dir.to_str().expect("utf-8 temp path")];
    args.extend_from_slice(extra);
    vilan(&args)
}

/// The report, with host separators normalized to `/` so one assertion reads
/// the same on both platforms. Stdout only: the report is the verb's OUTPUT.
fn explain(dir: &Path) -> String {
    let output = build(dir, &["--explain"]);
    assert!(
        output.status.success(),
        "build --explain failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).replace('\\', "/")
}

/// One block of the report: the `output`/`input` line naming a path that ends
/// with `suffix`, and its indented detail lines. Panics with the whole report
/// when there is no such block, because that failure is always "the shape
/// moved" and the shape is what one wants to read.
fn block(report: &str, kind: &str, suffix: &str) -> Vec<String> {
    let mut lines = report.lines();
    while let Some(line) = lines.next() {
        let Some(path) = line.strip_prefix(kind) else {
            continue;
        };
        if !path.trim().ends_with(suffix) {
            continue;
        }
        return lines
            .take_while(|line| line.starts_with("  "))
            .map(|line| line.trim().to_string())
            .collect();
    }
    panic!("no `{kind}` block for `{suffix}` in:\n{report}");
}

/// Whether any line of a block starts with `key` and ends with `tail` — the
/// shape every assertion below wants, since the value is an absolute temp path
/// whose head is noise.
fn has(block: &[String], key: &str, tail: &str) -> bool {
    block
        .iter()
        .any(|line| line.starts_with(key) && line.ends_with(tail))
}

#[test]
fn explain_names_every_output_and_what_contributed_to_it() {
    let dir = stage("surface");
    let report = explain(&dir);

    // An accumulated kind: both contributing sites, by file and line. The two
    // `const` sites are lines 14 and 15 of `src/client.vl` — the enclosing
    // `const` expressions, which is the granularity the record keeps.
    let css = block(&report, "output ", "dist/client.css");
    assert!(
        has(&css, "role", "emitted kind `css`"),
        "the sheet's role names the kind: {css:?}"
    );
    assert!(
        has(&css, "emitted", "src/client.vl:14"),
        "the first contributing site: {css:?}"
    );
    assert!(
        has(&css, "emitted", "src/client.vl:15"),
        "the second contributing site: {css:?}"
    );

    // A bundled copy: its source, and the `const` site that named it, with the
    // spelling used. `bundle` puts the file at its own path.
    let icon = block(&report, "output ", "dist/static/icon.svg");
    assert!(has(&icon, "role", "bundled copy"), "{icon:?}");
    assert!(has(&icon, "source", "src/static/icon.svg"), "{icon:?}");
    assert!(
        has(&icon, "named", "src/client.vl:16 (asset::bundle)"),
        "the naming site and the spelling: {icon:?}"
    );

    // `bundle_as` puts it somewhere its path does not spell — the case where a
    // reader most needs to be told which call decided.
    let logo = block(&report, "output ", "dist/brand/logo.svg");
    assert!(has(&logo, "role", "bundled copy"), "{logo:?}");
    assert!(has(&logo, "source", "src/static/logo.svg"), "{logo:?}");
    assert!(
        has(&logo, "named", "src/client.vl:17 (asset::bundle_as)"),
        "the naming site and the spelling: {logo:?}"
    );

    // The build's own files, named as such — for both legs, so the report
    // covers a `dist/` and not one entry of it.
    assert!(
        has(
            &block(&report, "output ", "dist/client.js"),
            "role",
            "compiled bundle"
        ),
        "the browser leg's bundle:\n{report}"
    );
    assert!(
        has(
            &block(&report, "output ", "dist/server.mjs"),
            "role",
            "compiled bundle"
        ),
        "the node leg's bundle:\n{report}"
    );
    assert!(
        has(
            &block(&report, "output ", "dist/client.chunks.json"),
            "role",
            "build manifest"
        ),
        "the leg's build manifest:\n{report}"
    );

    // A hook's declared output, with the hook's name and this build's verdict.
    let stamped = block(&report, "output ", "stamped.txt");
    assert!(has(&stamped, "role", "hook output"), "{stamped:?}");
    assert!(
        has(&stamped, "hook", "stamp (ran)"),
        "the first build runs the hook: {stamped:?}"
    );

    // Tracked inputs, and what each one invalidates. A `bundle` source moves
    // its copy AND the bundle it is compiled into (a const input is a source to
    // the compile).
    let source = block(&report, "input ", "src/static/icon.svg");
    assert!(
        has(&source, "read", "src/client.vl:16 (asset::bundle)"),
        "{source:?}"
    );
    assert!(
        has(&source, "invalidates", "dist/static/icon.svg"),
        "{source:?}"
    );
    assert!(has(&source, "invalidates", "dist/client.js"), "{source:?}");

    // A digest-tracked input reaches no `dist/` file by copy or flush, so the
    // bundle is the whole of its answer — and it is not "nothing".
    let version = block(&report, "input ", "src/data/version.txt");
    assert!(
        has(&version, "read", "src/client.vl:18 (asset::digest)"),
        "{version:?}"
    );
    assert!(
        has(&version, "invalidates", "dist/client.js"),
        "{version:?}"
    );

    // A hook's declared input invalidates that hook's declared outputs — one
    // declaration, read here the same way the freshness stamp and the watcher
    // read it.
    let seed = block(&report, "input ", "seed.txt");
    assert!(has(&seed, "declared", "`[[build.hook]]` stamp"), "{seed:?}");
    assert!(has(&seed, "invalidates", "stamped.txt"), "{seed:?}");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_lone_package_explains_the_outputs_beside_its_entry() {
    // A `dist/` is a multi-entry package's shape; a lone package writes beside
    // its source, through a different function with its own print. The report
    // is a statement about what a build wrote, not about a directory named
    // `dist`, so this path has to say it too — and nothing else here reaches
    // it.
    let dir = temp_project("lone");
    write(
        &dir,
        "app.vl",
        "import std::asset::emit;\n\
         import std::io::print;\n\
         \n\
         fun routes(): i32 {\n\
         \temit(\"routes\", \"GET /health\");\n\
         \t1\n\
         }\n\
         \n\
         let _routes = const routes();\n\
         \n\
         fun main() {\n\
         \tprint(\"lone\");\n\
         }\n\
         main();\n",
    );
    let entry = dir.join("app.vl");
    let output = vilan(&[
        "build",
        entry.to_str().expect("utf-8 temp path"),
        "--explain",
    ]);
    assert!(
        output.status.success(),
        "build --explain failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report = String::from_utf8_lossy(&output.stdout).replace('\\', "/");
    let routes = block(&report, "output ", "app.routes");
    assert!(has(&routes, "role", "emitted kind `routes`"), "{routes:?}");
    assert!(has(&routes, "emitted", "app.vl:9"), "{routes:?}");
    assert!(
        has(
            &block(&report, "output ", "app.mjs"),
            "role",
            "compiled bundle"
        ),
        "the lone package's bundle:\n{report}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_hook_that_was_skipped_is_explained_as_fresh() {
    let dir = stage("fresh");
    // First build: nothing is stamped, so the hook runs.
    assert!(
        has(
            &block(&explain(&dir), "output ", "stamped.txt"),
            "hook",
            "stamp (ran)"
        ),
        "a hook with no stamp runs"
    );
    // Second build over an unmoved tree: the hook is fresh, and the report says
    // so where it names the file the hook wrote. That difference is the whole
    // reason a hook's output carries a verdict at all — the file is there
    // either way, and "current" and "left by the last run" are not the same
    // fact about it.
    assert!(
        has(
            &block(&explain(&dir), "output ", "stamped.txt"),
            "hook",
            "stamp (Fresh)"
        ),
        "an unmoved tree skips the hook"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_plain_build_explains_nothing() {
    let dir = stage("silent");
    let output = build(&dir, &[]);
    assert!(
        output.status.success(),
        "build failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for line in printed.lines() {
        assert!(
            !line.starts_with("output ") && !line.starts_with("input "),
            "a plain build printed a report line: `{line}`\n{printed}"
        );
    }
    // And none of the report's detail keys either — the block heads are the
    // grep, but a stray `role`/`invalidates` line would mean half a report.
    assert!(
        !printed.contains("  role  ") && !printed.contains("  invalidates  "),
        "a plain build printed report details:\n{printed}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn explain_refuses_stdout_rather_than_corrupting_it() {
    let dir = stage("stdout");
    let output = build(&dir, &["--explain", "--stdout"]);
    assert!(
        !output.status.success(),
        "the pair must be refused:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let message = String::from_utf8_lossy(&output.stderr);
    assert!(
        message.contains("prints a bundle, not a build"),
        "the refusal names why: {message}"
    );
    // Nothing on stdout: the refusal must not half-write the stream it exists
    // to protect.
    assert!(
        String::from_utf8_lossy(&output.stdout).trim().is_empty(),
        "the refused command wrote to stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let _ = std::fs::remove_dir_all(&dir);
}
