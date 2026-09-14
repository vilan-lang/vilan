//! B339: a `[derive(..)]` inside a macro WORLD is skipped, not refused.
//!
//! A macro world is a nested analysis (macro-engine.md §3): one macro-defining
//! file, blanked to its definitions, compiled against `macro_std`. Its own
//! analysis must register NO macros — std's prelude modules declare `macro
//! fun`s, and registering them would recursively compile their worlds without
//! bound — so the expander's scope inside a world is empty by design.
//!
//! N79 then deleted the Rust generators that used to stand behind std's derive
//! macros, and the arm that lost its fallback answers an unresolved
//! `[derive(X)]` with one of two sentences: "expanded before std's `<module>`
//! declared its `X` macro: a compiler load-ordering bug", or "the std this
//! package resolves to does not carry one". Inside a world BOTH are false: the
//! scope is empty on purpose, and the std is fine.
//!
//! The reach is empty in the shipped tree — the entry a world sees is blanked,
//! `macro_std` declares no derives, and none of the eleven std modules a world
//! force-loads derives anything — so this pin PLANTS the shape instead of
//! waiting for it: a staged std whose `boolean.vl` (one of the eleven) carries
//! a `[derive(Debug)]`, and an entry whose own derive compiles a world over it.
//!
//! Its own test binary, like `macro_world_census.rs` beside it and for the same
//! reason: the world cache is process-global and keyed on the macro-definition
//! set, so a test that analyzes against a DIFFERENT std belongs outside any
//! process that asserts on that cache.

use std::path::{Path, PathBuf};

use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

/// The toolchain's own `vilan/` directory — `std/` and `macro_std/` sit here,
/// and `resolve_macro_std` finds the second from the first by walking up two
/// levels, so a staged std has to keep them siblings.
fn toolchain_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan")
}

/// Copies a directory tree, whole.
fn stage_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the staged directory");
    for entry in std::fs::read_dir(from).expect("read the source tree") {
        let path = entry.expect("a source entry").path();
        let name = path.file_name().expect("a named entry");
        if path.is_dir() {
            stage_tree(&path, &to.join(name));
        } else {
            std::fs::copy(&path, to.join(name)).expect("copy a staged file");
        }
    }
}

/// A std whose `boolean.vl` — a module every macro world force-loads — derives
/// something. Staged under the target directory rather than the system temp
/// (tracker N82: `/tmp` is one tmpfs shared by every worktree, and a suite run
/// under lane load filled it).
fn staged_std_with_a_planted_derive() -> (PathBuf, PackageSpec) {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "b339_std_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let toolchain = toolchain_root();
    stage_tree(&toolchain.join("std"), &root.join("std"));
    stage_tree(&toolchain.join("macro_std"), &root.join("macro_std"));

    let planted = root.join("std/src/boolean.vl");
    let mut source = std::fs::read_to_string(&planted).expect("read the staged boolean.vl");
    source.push_str("\n[derive(Debug)]\nstruct WorldProbe {\n\tflag: bool,\n}\n");
    std::fs::write(&planted, source).expect("plant the derive");
    let spec = vilan_core::manifest::resolve_std(&root.join("std"));
    (root, spec)
}

/// The smallest entry that compiles a macro world at all: a derive of its own
/// sends the expander into std's `debug` world (`macro_world_census.rs`'s
/// `DERIVING`).
const DERIVING: &str = "[derive(Debug)]\nstruct Point { x: i32, y: i32 }\n\n\
     fun main() {\n\tlet p = Point { x = 1, y = 2 };\n\tlet _shown = p.debug();\n}\n";

#[test]
fn a_derive_in_a_force_loaded_std_module_is_not_refused_inside_a_macro_world() {
    let (root, std) = staged_std_with_a_planted_derive();
    let leaked: &'static str = Box::leak(DERIVING.to_string().into_boxed_str());
    let (program, errors) = analyze_source(
        leaked,
        &std,
        Path::new("."),
        Path::new("main.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("load-ordering bug")),
        "a derive a world's empty scope cannot resolve is not a load-ordering \
         bug and must not be reported as one: {messages:#?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("does not carry one")),
        "nor a std that does not carry the derive — this one does: {messages:#?}"
    );
    assert!(
        messages.is_empty(),
        "and the analysis is clean: {messages:#?}"
    );
    assert!(program.is_some(), "the program is built");
}
