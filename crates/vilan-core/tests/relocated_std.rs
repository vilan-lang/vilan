//! B346: a `std` relocated away from its toolchain is REFUSED by name.
//!
//! `std` and `macro_std` are two packages of ONE toolchain, and only the first
//! of them has a discovery path a user can point at: `$VILAN_STD` (and the
//! editor's `vilan.stdPath`) names `std`, while `macro_std` is found by walking
//! up two levels from the resolved `std` and looking for a sibling. That second
//! rule reads the filesystem, not the configuration — so a `std` copied out of
//! its toolchain (a packaged std, a CI cache, a sibling checkout, `cp -a`)
//! resolves perfectly and arrives with no `macro_std` behind it.
//!
//! What the compiler said about that before this pin was two untruths. std's
//! own `macro fun`s failed to register with a sentence that named neither the
//! `std` it had nor the `macro_std` it wanted, and the user's own
//! `[derive(PartialEq)]` — which the missing macro could not expand — was
//! blamed on the COMPILER: "expanded before std's `compare.vl` declared its
//! `PartialEq` macro: a compiler load-ordering bug (B21's class); please report
//! how this module is reached". Nothing in the run named the actual mistake,
//! and the estate's own suites never saw it because every one of them points
//! `$VILAN_STD` at a `std` that still has its sibling.
//!
//! So: ONE root answers for both packages, and a root that carries only half a
//! toolchain is refused by a message naming BOTH paths.
//!
//! Its own test binary, like `derive_world_guard.rs` and `macro_world_census.rs`
//! beside it and for the same reason: the macro world cache is process-global
//! and keyed on the macro-definition set, so a test that analyzes against a
//! DIFFERENT `std` belongs outside any process asserting on that cache.

use std::path::{Path, PathBuf};

use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

/// The toolchain's own `vilan/` directory — `std/` and `macro_std/` are the two
/// packages in it.
fn toolchain_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan")
}

/// Copies a directory tree, whole and byte for byte.
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

/// Fails unless every file under `from` is present under `to` with identical
/// bytes. The premise of this whole file is that the relocated `std` is not a
/// DIFFERENT std — `diff -r` over the item's own repro was empty — so the pin
/// states the premise rather than assuming it.
fn assert_identical(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).expect("read the source tree") {
        let path = entry.expect("a source entry").path();
        let name = path.file_name().expect("a named entry");
        let mirror = to.join(name);
        if path.is_dir() {
            assert_identical(&path, &mirror);
        } else {
            let original = std::fs::read(&path).expect("read the original");
            let copied = std::fs::read(&mirror)
                .unwrap_or_else(|error| panic!("read {}: {error}", mirror.display()));
            assert_eq!(
                original,
                copied,
                "the relocated copy must be byte-identical: {}",
                mirror.display()
            );
        }
    }
}

/// A scratch root under the target directory rather than the system temp
/// (tracker N82: `/tmp` is one tmpfs shared by every worktree, and a suite run
/// under lane load filled it).
fn scratch(tag: &str) -> PathBuf {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "b346_{tag}_{}_{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// The smallest program that needs a std macro: a derive whose impl std's
/// `compare.vl` writes.
const DERIVING: &str = "[derive(PartialEq)]\nstruct Point { x: i32, y: i32 }\n\n\
     fun main() {\n\tlet a = Point { x = 1, y = 2 };\n\tlet b = Point { x = 1, y = 2 };\n\
     \tlet _same = a == b;\n}\n";

/// Analyzes [`DERIVING`] against `std` and returns every diagnostic, in the
/// order the compiler prints them.
fn errors_against(std: &PackageSpec) -> Vec<vilan_core::error::Error> {
    let leaked: &'static str = Box::leak(DERIVING.to_string().into_boxed_str());
    let (_program, errors) = analyze_source(
        leaked,
        std,
        Path::new("."),
        Path::new("main.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    errors
}

/// Analyzes [`DERIVING`] against `std` and returns every message.
fn messages_against(std: &PackageSpec) -> Vec<String> {
    errors_against(std)
        .into_iter()
        .map(|error| error.msg)
        .collect()
}

/// The sentence a split toolchain earns, identifying it in a message list.
const SPLIT_REFUSAL: &str = "the `macro_std` package was not found beside `std`";

#[test]
fn a_std_relocated_away_from_its_macro_std_is_refused_by_naming_both_paths() {
    let root = scratch("relocated");
    let std_dir = root.join("std-probe");
    stage_tree(&toolchain_root().join("std"), &std_dir);
    assert_identical(&toolchain_root().join("std"), &std_dir);
    // The condition under test: the copy's own root holds no `macro_std`.
    let macro_std = root.join("macro_std");
    assert!(!macro_std.exists(), "the relocated root holds no macro_std");

    let std = vilan_core::manifest::resolve_std(&std_dir);
    let messages = messages_against(&std);
    let _ = std::fs::remove_dir_all(&root);

    let names_both = |message: &String| {
        message.contains(&std_dir.display().to_string())
            && message.contains(&macro_std.display().to_string())
    };
    assert!(
        messages.iter().any(names_both),
        "a split toolchain is refused by a message naming the `std` it resolved \
         ({}) and the `macro_std` it looked for ({}): {messages:#?}",
        std_dir.display(),
        macro_std.display(),
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("load-ordering bug")),
        "and nothing blames the compiler: a derive that cannot expand because \
         this toolchain is missing half of itself is not a load-ordering bug \
         (B21's class): {messages:#?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("please report")),
        "nor asks the user to report it: {messages:#?}"
    );
}

#[test]
fn the_same_copy_with_its_macro_std_beside_it_analyzes_clean() {
    // The other half of the rule: ONE root wins, and a root carrying BOTH
    // packages is a toolchain wherever it sits. The item's own evidence —
    // `<repo>/vilan/std2` worked while `<repo>/target/std-probe` did not — is
    // this difference and nothing else.
    let root = scratch("whole");
    stage_tree(&toolchain_root().join("std"), &root.join("std"));
    stage_tree(&toolchain_root().join("macro_std"), &root.join("macro_std"));
    assert_identical(&toolchain_root().join("std"), &root.join("std"));

    let std = vilan_core::manifest::resolve_std(&root.join("std"));
    let messages = messages_against(&std);
    let _ = std::fs::remove_dir_all(&root);

    assert!(
        messages.is_empty(),
        "a relocated toolchain that kept both packages compiles exactly as the \
         checkout does: {messages:#?}"
    );
}

// --- E212: the refusal is ONE sentence, and it leads -------------------------
//
// B346's message was right and unreachable in practice. It was pushed from
// `register_file`'s guard, which runs once per macro-DEFINING file — and std
// declares macros in six of its own modules — so a split toolchain printed TEN
// copies of one sentence, each anchored on a `macro fun` inside a std file the
// reader did not write and cannot fix, and all of them AFTER a first diagnostic
// that blamed the reader's own code for the consequence ("type 'Point' does not
// implement the `PartialOrd` operator"). The owner reached it the way anyone
// would, by pointing `$VILAN_STD` at a std, and reported that the derive
// "silently fails to expand" and that std's compare.vl was blamed.
//
// Two commands reproduce it:
//
//     cp -a vilan/std /tmp/half/std
//     VILAN_STD=/tmp/half/std/src vilan check     # in any package using a derive
//
// So: a half toolchain is a fact about the CONFIGURATION, asked once, before
// the first registration, and attributed to the entry at offset 0 — which is
// what puts the actionable sentence first.

/// The refusal's position among `messages`, and how many of them are it.
fn split_refusal(messages: &[String]) -> (Option<usize>, usize) {
    (
        messages
            .iter()
            .position(|message| message.contains(SPLIT_REFUSAL)),
        messages
            .iter()
            .filter(|message| message.contains(SPLIT_REFUSAL))
            .count(),
    )
}

#[test]
fn the_split_toolchain_refusal_is_one_sentence_and_it_leads() {
    let root = scratch("once");
    let std_dir = root.join("std-probe");
    stage_tree(&toolchain_root().join("std"), &std_dir);
    assert!(
        !root.join("macro_std").exists(),
        "the relocated root holds no macro_std"
    );

    let std = vilan_core::manifest::resolve_std(&std_dir);
    let errors = errors_against(&std);
    let messages: Vec<String> = errors.iter().map(|error| error.msg.clone()).collect();
    let _ = std::fs::remove_dir_all(&root);

    let (first, count) = split_refusal(&messages);
    assert_eq!(
        count, 1,
        "ONE sentence — it used to be one per macro-defining file of std, which is \
         ten: {messages:#?}"
    );
    assert_eq!(
        first,
        Some(0),
        "and it leads, ahead of the type errors the unexpanded derives cause: {messages:#?}"
    );
    // Anchored on the ENTRY's own head, not inside a std file: the reader can
    // act on their configuration, and there is no file of std's to open.
    let anchor = errors[0].span.into_range();
    assert_eq!(
        (anchor.start, anchor.end),
        (0, 0),
        "the refusal points at the entry, not at a `macro fun` in std"
    );
}

#[test]
fn a_whole_toolchain_registers_every_macro_defining_file() {
    // The other side of the skip: refusing once and returning an EMPTY registry
    // must not be reachable when the toolchain is whole, or every derive in the
    // estate would stop expanding. The clean analysis above already proves the
    // derive expands; this states the premise the skip rests on.
    let root = scratch("whole-registry");
    stage_tree(&toolchain_root().join("std"), &root.join("std"));
    stage_tree(&toolchain_root().join("macro_std"), &root.join("macro_std"));

    let std = vilan_core::manifest::resolve_std(&root.join("std"));
    let messages = messages_against(&std);
    let _ = std::fs::remove_dir_all(&root);

    assert_eq!(split_refusal(&messages).1, 0, "{messages:#?}");
    assert!(messages.is_empty(), "{messages:#?}");
}
