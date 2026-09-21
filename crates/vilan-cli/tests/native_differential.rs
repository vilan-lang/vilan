//! The native backend's exit test (tracker F1, slice S1a; `native-apps.md`
//! §5-S1): every program the Rust emitter ACCEPTS prints, as a native binary,
//! byte-identical stdout to the same program compiled by the JS backend.
//!
//! # Why this is the right gate
//!
//! It needs no new oracle. `ssr_differential` proves a second `std::ui`
//! implementation by compiling one program two ways and comparing bytes; this
//! is the same shape applied to a BACKEND rather than to a platform, and the
//! corpus it runs over is the one the whole project already trusts to say what
//! a program means (`vilan/test/`, whose rule is that a program terminates and
//! its output is its claim).
//!
//! # The three verdicts, and why a REFUSAL is one of them
//!
//! S1a's emitter is a first cut. Over the corpus, a program lands in exactly
//! one of three places:
//!
//! * **refused** — the emitter names a construct it does not reach (a generic
//!   function, a module-level binding, `async`, a host binding) and answers an
//!   error. Recorded, not failed: naming the gap is what turns it into S1b's
//!   work list, and [`the_refusals_are_named_and_counted`] prints the census.
//! * **identical** — compiled both ways, same bytes. The claim.
//! * **anything else** — a program the emitter accepted and then got wrong, or
//!   emitted Rust that rustc will not build. **That is a failure**, and it is
//!   the only thing this file lets through as one. A backend that quietly
//!   prints a different number is worse than a backend that refuses.
//!
//! # Cost
//!
//! rustc is ~1 s per program even in debug, so the default suite runs a handful
//! and `VILAN_NATIVE_DIFFERENTIAL=1` runs the whole corpus (the seal does). The
//! `vilan-rt` build is shared through one `CARGO_TARGET_DIR` under
//! `CARGO_TARGET_TMPDIR`, so the runtime compiles ONCE for the whole sweep
//! rather than once per program.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The programs the default suite runs — small, fast, and between them they
/// cover every value shape S1a claims: scalars, `str`, `bool`, a struct with an
/// `impl`, an enum with a payload, a `List`, a `match`, a loop, recursion.
///
/// Between them they cover: `bool` and `match`, recursion and mutual
/// recursion, loops, rule 1's copies and the elisions the last-use pass makes,
/// a struct with an `impl` and a field write, iteration over a `List`, `Option`
/// through `List::get`/`remove`, `Display`'s interpolation, an enum payload
/// capture, and `&`/`&mut` views through a `borrows` return.
///
/// `board.vl`, the paper's own probe, is deliberately NOT here: it does not
/// compile natively yet, and [`the_probes_board_program_names_its_own_gap`]
/// pins WHY rather than pretending otherwise.
///
/// S1b adds six rows, one per thing the slice built: monomorphisation of a
/// generic function over two instantiations (`generic-inference.vl`), a
/// generic parameter's own defaulted bound (`default-generic-param.vl`), a
/// trait default specialized per type plus an inferred return type
/// (`default.vl`), `mut` parameters (`mut-parameters.vl`), a module-level
/// binding read and WRITTEN plus B105's hoist (`compound-index.vl`), and a
/// string literal's escapes (`interpolated-multiline-string.vl` — the class
/// S1a got wrong for every escape there is).
const DEFAULT_SUITE: &[&str] = &[
    "bool.vl",
    "recursion.vl",
    "loops.vl",
    "copy-elision.vl",
    "field-assignment.vl",
    "for-in.vl",
    "list-splice.vl",
    "display.vl",
    "match-ergonomics.vl",
    "borrows.vl",
    "generic-inference.vl",
    "default-generic-param.vl",
    "default.vl",
    "mut-parameters.vl",
    "compound-index.vl",
    "interpolated-multiline-string.vl",
];

/// The corpus's ASYNC programs (tracker J6, lane native-b-38).
///
/// They are enumerated by name rather than reached through
/// [`platform_free_programs`] because `std::task` and `std::time` are still
/// rows of [`PLATFORM_MODULES`] — the executor gives the backend an event loop,
/// not a `std::time` twin (`now_millis`, `Date`, `sleep_for`'s `Duration` path
/// still bind the host). So this list is what J6's exit is measured against,
/// and every program on it lands in one of the same three verdicts the corpus
/// sweep uses: identical, refused by NAME, or broken.
const ASYNC_SUITE: &[&str] = &[
    "async-await.vl",
    "async-promise-all.vl",
    "await-postfix.vl",
    "adapt.vl",
    "nursery.vl",
    "reactive-turns.vl",
    "time.vl",
];

/// Modules whose presence in an `import` means the program reaches a platform
/// surface S1a has none of. Written as a support list so Order 38 widens the
/// corpus by deleting rows rather than by rewriting the walk.
const PLATFORM_MODULES: &[&str] = &[
    "std::dom",
    "std::fetch",
    "std::fs",
    "std::http",
    "std::db",
    "std::rpc",
    "std::ui",
    "std::web",
    "std::storage",
    "std::router",
    "std::dev",
    "std::canvas",
    "std::process",
    "std::asset",
    "std::build",
    "std::task",
    "std::time",
    "std::crypto",
    "std::random",
];

fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test")
}

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

fn runtime_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../vilan-rt")
}

/// One shared cargo target directory for the whole sweep, so `vilan-rt` is
/// built once. Under `CARGO_TARGET_TMPDIR`, which cargo gives every integration
/// test and cleans with the rest of `target/`.
fn shared_target() -> PathBuf {
    let shared = Path::new(env!("CARGO_TARGET_TMPDIR")).join("native-differential");
    std::fs::create_dir_all(&shared).expect("create the shared cargo target directory");
    shared
}

/// Every platform-free corpus program, in a stable order.
///
/// **The enumeration is a support function on purpose** (the brief's own
/// instruction): Order 38's widening of the native corpus is an edit to
/// [`PLATFORM_MODULES`] and to what the emitter accepts, never a second walk
/// written beside this one that can disagree with it about what the corpus is.
pub fn platform_free_programs() -> Vec<String> {
    let mut programs = Vec::new();
    let entries = std::fs::read_dir(corpus_dir()).expect("read the corpus directory");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|extension| extension == "vl") {
            let source = std::fs::read_to_string(&path).expect("read a corpus program");
            let reaches_a_platform = PLATFORM_MODULES
                .iter()
                .any(|module| source.contains(&format!("import {module}")));
            if !reaches_a_platform {
                programs.push(path.file_name().unwrap().to_string_lossy().into_owned());
            }
        }
    }
    programs.sort();
    programs
}

/// A staged copy of the corpus, so a build writes its artifacts beside a copy
/// rather than into the tree. Per PROCESS: nextest runs each test of this binary
/// in its own process, concurrently, and a staging directory the four shared —
/// each one beginning by removing it — was a race the Order 37 seal lost
/// (`cannot create ./dist/native/bool/src`: a sibling test had just deleted the
/// working directory out from under the build). The shared cargo target
/// directory stays shared on purpose; cargo locks it itself.
fn stage() -> PathBuf {
    let staged = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("native-differential-src-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staged);
    copy_tree(&corpus_dir(), &staged);
    staged
}

/// Copies the corpus tree, DIRECTORIES INCLUDED.
///
/// The files-only copy was a harness defect that read as a backend one:
/// `module-dirs.vl` imports `pkg::nested::…`, whose modules live in
/// `vilan/test/nested/`, and a staging directory without it failed the native
/// leg with `cannot find 'nested' in the imported path` — which `compare`
/// classifies as BROKEN, because the message carries no refusal. The JS leg was
/// never reached, so the program looked like an emitter failure while the JS
/// backend would have failed identically. One recursive copy, and the
/// `dist/native/` a build writes still lands in the copy rather than the tree.
fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the staging directory");
    for entry in std::fs::read_dir(from)
        .expect("read the corpus directory")
        .flatten()
    {
        let path = entry.path();
        let name = path.file_name().expect("a corpus entry has a name");
        // `dist` is a build artifact of the tree, not a corpus input.
        if name == "dist" || name == "target" {
            continue;
        }
        if path.is_dir() {
            copy_tree(&path, &to.join(name));
        } else if path.is_file() {
            std::fs::copy(&path, to.join(name)).expect("stage a corpus program");
        }
    }
}

fn vilan(staged: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vilan"));
    command
        .current_dir(staged)
        .env("VILAN_STD", std_dir())
        .env("VILAN_RT", runtime_dir())
        .env("CARGO_TARGET_DIR", shared_target());
    command
}

/// What one program did.
#[derive(Debug, PartialEq)]
enum Verdict {
    /// The emitter named a construct it does not reach. The reason, verbatim.
    Refused(String),
    Identical,
    /// Accepted and wrong: the two backends printed different things, or the
    /// emitted Rust did not build. The detail is the report.
    Broken(String),
}

fn compare(staged: &Path, program: &str) -> Verdict {
    let native = vilan(staged)
        .args(["run", "--backend", "rust", program])
        .output()
        .expect("run the native backend");
    if !native.status.success() {
        let message = String::from_utf8_lossy(&native.stderr).into_owned();
        if message.contains("does not emit") {
            let reason = message
                .lines()
                .find(|line| line.contains("does not emit"))
                .unwrap_or("")
                .trim()
                .to_string();
            return Verdict::Refused(reason);
        }
        return Verdict::Broken(format!("the native leg failed:\n{message}"));
    }
    let javascript = vilan(staged)
        .args(["run", program])
        .output()
        .expect("run the JS backend");
    if !javascript.status.success() {
        return Verdict::Broken(format!(
            "the JS leg failed, so there is nothing to compare against:\n{}",
            String::from_utf8_lossy(&javascript.stderr)
        ));
    }
    if native.stdout == javascript.stdout {
        return Verdict::Identical;
    }
    Verdict::Broken(format!(
        "stdout differs.\n  js:   {:?}\n  rust: {:?}",
        String::from_utf8_lossy(&javascript.stdout),
        String::from_utf8_lossy(&native.stdout),
    ))
}

#[test]
fn the_default_suite_is_byte_identical_on_both_backends() {
    let staged = stage();
    let mut broken = Vec::new();
    for program in DEFAULT_SUITE {
        match compare(&staged, program) {
            Verdict::Identical => {}
            Verdict::Refused(reason) => broken.push(format!(
                "{program}: the default suite must be programs the backend ACCEPTS, and this one \
                 is refused — {reason}"
            )),
            Verdict::Broken(detail) => broken.push(format!("{program}: {detail}")),
        }
    }
    assert!(
        broken.is_empty(),
        "the native backend disagrees with the JS backend:\n{}",
        broken.join("\n")
    );
}

/// The whole platform-free corpus, under `VILAN_NATIVE_DIFFERENTIAL=1`.
///
/// It prints the census — refused / identical — because that census IS the
/// slice's number, and a reader who runs this wants to see it move.
#[test]
fn every_platform_free_program_is_identical_or_named() {
    if std::env::var_os("VILAN_NATIVE_DIFFERENTIAL").is_none() {
        eprintln!(
            "skipped: set VILAN_NATIVE_DIFFERENTIAL=1 to sweep the whole corpus \
             (rustc is ~1 s per program)"
        );
        return;
    }
    let staged = stage();
    let programs = platform_free_programs();
    let mut identical_programs: Vec<String> = Vec::new();
    let mut refused: Vec<(String, String)> = Vec::new();
    let mut broken = Vec::new();
    for program in &programs {
        match compare(&staged, program) {
            Verdict::Identical => identical_programs.push(program.clone()),
            Verdict::Refused(reason) => refused.push((program.clone(), reason)),
            Verdict::Broken(detail) => broken.push(format!("{program}: {detail}")),
        }
    }
    eprintln!(
        "native differential: {} enumerated, {} identical, {} refused by name, {} broken",
        programs.len(),
        identical_programs.len(),
        refused.len(),
        broken.len()
    );
    for (program, reason) in &refused {
        eprintln!("  refused  {program}: {reason}");
    }
    for program in &identical_programs {
        eprintln!("  identical  {program}");
    }
    assert!(
        broken.is_empty(),
        "programs the native backend ACCEPTED and then got wrong:\n{}",
        broken.join("\n")
    );
}

/// `native-apps.md`'s own probe — **the differential this pin was written to
/// become** (F20).
///
/// It was a named-gap pin for two slices. S1a's wall was `SignalCell<i32>`, a
/// generic type; S1b removed every wall it named and recorded the host type
/// `Hash`. F20 built `Hash` (`vilan_rt::Hash`, the `CanonicalHash`/`HashEq`
/// intrinsics and `JSON.stringify`'s own renderer), and behind it stood
/// `std::reactive`'s three glue bindings (`__guarded`, `__with_finally`,
/// `queueMicrotask`) and five move/copy defects the program was the first to
/// reach: `Shared::write()` as a place, a `Shared` binding read twice, a `move`
/// closure taking a capture the frame still needs, a `match` leg destructuring a
/// place the body reads afterwards, and a boxed binding pushed through a COPY of
/// its value (which printed `0 0` against the JS backend's `2 2`).
///
/// So the assertion is now the claim rather than the gap: the paper's probe
/// prints the same bytes as a native binary and as a JS one.
#[test]
fn the_probes_board_program_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_board.vl"), BOARD_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_board.vl"),
        Verdict::Identical,
        "`native-apps.md`'s board probe is the native path's exit; it must print the same bytes \
         on both backends"
    );
}

/// The canonical key, as the JS backend's `__hash` computes it (F20): a
/// primitive keys as ITSELF and an aggregate keys as its `JSON.stringify` text,
/// so the four `Hash` arms are the four JS primitive kinds and nothing else.
///
/// Every stock `impl Hashable` in `std::hash` is exercised through a `Map` key,
/// because keying is the only thing a `Hash` is for and a wrong canonicalisation
/// shows up as a lookup that misses or a duplicate that collides — not as a
/// printed value. The `1` / `"1"` pair is the case a naive
/// canonicalise-to-a-string would get wrong: two DIFFERENT JS primitives, so two
/// different keys.
#[test]
fn a_canonical_hash_keys_the_same_values_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_hash.vl"), HASH_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_hash.vl"),
        Verdict::Identical,
        "a canonical hash must key the same values on both backends"
    );
}

const HASH_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::map::Map;\n",
    "import std::set::Set;\n",
    "\n",
    "[derive(Hashable, PartialEq)]\n",
    "struct Point { x: i32, y: i32 }\n",
    "\n",
    "fun main() {\n",
    // A string key and an integer key that render the same: two JS primitives,
    // two keys.
    "\tmut mixed: Map<str, i32> = Map::new();\n",
    "\tmixed.insert(\"1\", 10);\n",
    "\tmixed.insert(\"one\", 11);\n",
    "\tprint(mixed.len());\n",
    "\tprint(mixed.get(\"1\"));\n",
    "\tmut numbers: Map<i32, str> = Map::new();\n",
    "\tnumbers.insert(1, \"one\");\n",
    "\tnumbers.insert(2, \"two\");\n",
    "\tnumbers.insert(1, \"uno\");\n",
    "\tprint(numbers.len());\n",
    "\tprint(numbers.get(1));\n",
    // A re-insert keeps the ORIGINAL position, as a JS `Map` does.
    "\tfor key in numbers.keys() { print(key); }\n",
    // `bool` and `f64` keys — the other two primitive arms.
    "\tmut flags: Map<bool, i32> = Map::new();\n",
    "\tflags.insert(true, 1);\n",
    "\tflags.insert(false, 0);\n",
    "\tprint(flags.get(true));\n",
    "\tprint(flags.contains_key(false));\n",
    "\tmut reals: Map<f64, str> = Map::new();\n",
    "\treals.insert(1.5, \"half\");\n",
    "\treals.insert(0.0, \"zero\");\n",
    "\tprint(reals.get(1.5));\n",
    "\tprint(reals.len());\n",
    // An AGGREGATE key: `[derive(Hashable)]` canonicalises through
    // `JSON.stringify`, so two equal points are one key and a different one is
    // its own.
    "\tmut points: Map<Point, str> = Map::new();\n",
    "\tpoints.insert(Point { x = 1, y = 2 }, \"a\");\n",
    "\tpoints.insert(Point { x = 1, y = 2 }, \"b\");\n",
    "\tpoints.insert(Point { x = 2, y = 1 }, \"c\");\n",
    "\tprint(points.len());\n",
    "\tprint(points.get(Point { x = 1, y = 2 }));\n",
    // A `List` key — `impl List<T: Hashable> with Hashable`.
    "\tmut lists: Map<List<i32>, str> = Map::new();\n",
    "\tlists.insert([1, 2], \"twelve\");\n",
    "\tprint(lists.get([1, 2]));\n",
    "\tprint(lists.get([2, 1]));\n",
    // A `Set`, which keys the same way.
    "\tmut words: Set<str> = Set::new();\n",
    "\twords.insert(\"a\");\n",
    "\twords.insert(\"a\");\n",
    "\twords.insert(\"b\");\n",
    "\tprint(words.len());\n",
    "\tprint(words.contains(\"b\"));\n",
    "\tprint(words.contains(\"z\"));\n",
    "}\n",
);

/// S1b's monomorphisation, held to the shape rather than to one program: a
/// generic function, a generic struct and a generic enum each emit ONE Rust
/// item per instantiation, and two instantiations of one declaration are two
/// distinct items.
///
/// Written as an inline probe rather than over the corpus because the corpus
/// has no program that instantiates one declaration at two types AND prints
/// both — which is precisely the case a single-instance emitter would pass.
#[test]
fn one_declaration_at_two_types_emits_two_rust_items() {
    let staged = stage();
    let probe = staged.join("native_probe_mono.vl");
    std::fs::write(&probe, MONO_PROBE).expect("write the probe program");
    let output = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_mono.vl",
        ])
        .output()
        .expect("build the probe");
    let source = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the monomorphisation probe was refused:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Two instances of the function, two of the struct, two of the enum. The
    // names carry the declaration's id and a per-declaration sequence number,
    // so counting the DECLARED items is counting the instances.
    let count = |needle: &str| source.matches(needle).count();
    assert_eq!(
        count("fn identity_"),
        2,
        "two instances of `identity`:\n{source}"
    );
    assert_eq!(
        count("struct Pair_"),
        2,
        "two instances of `Pair`:\n{source}"
    );
    assert_eq!(count("enum Tree_"), 2, "two instances of `Tree`:\n{source}");
    // And the instantiations are really distinct: one `Pair` holds an i32 left,
    // the other a string one.
    assert!(source.contains("left: i32"), "{source}");
    assert!(source.contains("left: vilan_rt::Str"), "{source}");
    // A NON-generic declaration keeps S1a's plain `{name}_{id}` — the property
    // that leaves every program the previous slice emitted byte-identical.
    assert!(
        source.contains("fn plain_"),
        "a non-generic function keeps its unsuffixed name:\n{source}"
    );

    let run = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_mono.vl"])
        .output()
        .expect("run the probe natively");
    let javascript = vilan(&staged)
        .args(["run", "native_probe_mono.vl"])
        .output()
        .expect("run the probe on the JS backend");
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&javascript.stdout),
        "the two backends disagree about the monomorphised program"
    );
}

/// The monomorphisation probe: one generic function, one generic struct and
/// one generic enum, each at TWO instantiations, plus a non-generic function
/// whose emitted name must not move.
const MONO_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun identity<T>(value: T): T { value }\n",
    "\n",
    "fun plain(n: i32): i32 { n + 1 }\n",
    "\n",
    "struct Pair<A, B> { left: A, right: B }\n",
    "\n",
    "enum Tree<T> { Leaf(T), Empty }\n",
    "\n",
    "fun main() {\n",
    "\tprint(identity(3));\n",
    "\tprint(identity(\"hi\"));\n",
    "\tprint(plain(1));\n",
    "\tlet a = Pair { left = 1, right = \"two\" };\n",
    "\tlet b = Pair { left = \"a\", right = 2 };\n",
    "\tprint(a.left);\n",
    "\tprint(b.right);\n",
    "\tlet leaf: Tree<i32> = Tree::Leaf(7);\n",
    "\tlet word: Tree<str> = Tree::Leaf(\"x\");\n",
    "\tmatch leaf {\n",
    "\t\tTree::Leaf(let v) => { print(v); },\n",
    "\t\tTree::Empty => { print(0); },\n",
    "\t}\n",
    "\tmatch word {\n",
    "\t\tTree::Leaf(let v) => { print(v); },\n",
    "\t\tTree::Empty => { print(\"\"); },\n",
    "\t}\n",
    "}\n",
);

/// A string literal's ESCAPES mean the same thing on both backends.
///
/// S1a wrote a literal's source text straight into a Rust literal and escaped
/// its backslashes, so `print("a\nb")` printed `a\nb` natively against the JS
/// backend's two lines — and because no program in the accepted corpus carried
/// an escape, the differential never saw it. This is that class, as a probe,
/// covering each of the six escapes vilan recognises, an unknown escape (which
/// keeps both characters), and a literal backslash.
#[test]
fn a_string_literals_escapes_mean_the_same_thing_on_both_backends() {
    let staged = stage();
    let probe = staged.join("native_probe_escapes.vl");
    std::fs::write(&probe, ESCAPE_PROBE).expect("write the probe program");
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_escapes.vl"])
        .output()
        .expect("run the escape probe natively");
    assert!(
        native.status.success(),
        "{}",
        String::from_utf8_lossy(&native.stderr)
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_escapes.vl"])
        .output()
        .expect("run the escape probe on the JS backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        String::from_utf8_lossy(&javascript.stdout),
        "the two backends disagree about a string literal's escapes"
    );
    // Non-vacuous by construction: the JS side really does interpret them.
    assert!(
        String::from_utf8_lossy(&javascript.stdout).contains("a\nb"),
        "the probe's `\\n` must be a real newline on the JS side: {:?}",
        String::from_utf8_lossy(&javascript.stdout)
    );
}

const ESCAPE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tprint(\"a\\nb\");\n",
    "\tprint(\"tab\\there\");\n",
    "\tprint(\"cr\\rhere\");\n",
    "\tprint(\"quote\\\"q\");\n",
    "\tprint(\"back\\\\slash\");\n",
    "\tprint(\"unknown\\u0041escape\");\n",
    "}\n",
);

/// C15's measurement, as a pin rather than as a number in a report: the
/// emitter's boxed-binding count is REACHABLE, and it is zero over a program
/// with no mutably-captured binding and non-zero over one that has.
///
/// R3 ruled that v1 boxes every mutably-captured binding and that the count is
/// what pays for the by-value capture optimisation later. A measurement nothing
/// holds to a shape is a measurement that silently stops being taken.
#[test]
fn the_boxed_binding_count_is_reachable_and_counts_the_right_bindings() {
    let staged = stage();
    let none = staged.join("native_probe_boxed_none.vl");
    std::fs::write(
        &none,
        "import std::io::print;\n\nfun main() {\n\tlet n = 1;\n\tlet f = || { n + 1 };\n\tprint(f());\n}\n",
    )
    .expect("write the probe");
    let some = staged.join("native_probe_boxed_some.vl");
    std::fs::write(
        &some,
        "import std::io::print;\n\nfun main() {\n\tmut n = 1;\n\tlet bump = || { n = n + 1; };\n\tbump();\n\tbump();\n\tprint(n);\n}\n",
    )
    .expect("write the probe");

    let count = |program: &str| {
        let output = vilan(&staged)
            .env("VILAN_NATIVE_REPORT_BOXED", "1")
            .args(["build", "--backend", "rust", program])
            .output()
            .expect("build the probe");
        assert!(
            output.status.success(),
            "{program}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines()
            .find_map(|line| line.strip_prefix("vilan-native: boxed-bindings="))
            .unwrap_or_else(|| panic!("{program} printed no boxed-bindings line:\n{text}"))
            .trim()
            .parse::<usize>()
            .expect("a count")
    };
    assert_eq!(
        count("native_probe_boxed_none.vl"),
        0,
        "a binding a closure only READS is not boxed"
    );
    assert_eq!(
        count("native_probe_boxed_some.vl"),
        1,
        "a binding a closure WRITES is boxed (spec §6.9: a closure captures the binding)"
    );
}

/// `native-apps.md`'s probe, verbatim from
/// `proposals/projects/vilan/proposal/native-apps-probe/board.vl` — copied
/// rather than referenced because the proposals repository is not a build
/// input.
const BOARD_PROBE: &str = concat!(
    "import std::reactive::{ Owner, SignalCell };\n",
    "\n",
    "struct Todo { id: i32, title: str, done: bool }\n",
    "\n",
    "struct Board { name: str, todos: List<Todo> }\n",
    "\n",
    "impl Board {\n",
    "\tfun new(name: str): Board { Board { name, todos = [] } }\n",
    "\tfun add(&mut self, todo: Todo) { self.todos.push(todo); }\n",
    "\tfun open(self): i32 {\n",
    "\t\tmut n = 0;\n",
    "\t\tfor todo in self.todos { if !todo.done { n = n + 1; } }\n",
    "\t\tn\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut board = Board::new(\"inbox\");\n",
    "\tboard.add(Todo { id = 1, title = \"write the paper\", done = false });\n",
    "\tboard.add(Todo { id = 2, title = \"read the census\", done = true });\n",
    "\tmut snapshot = board.todos;\n",
    "\tsnapshot.push(Todo { id = 3, title = \"ghost\", done = false });\n",
    "\tprint(board.todos.len());\n",
    "\tprint(snapshot.len());\n",
    "\tlet count: SignalCell<i32> = SignalCell::new(board.open());\n",
    "\tmut seen: List<i32> = [];\n",
    "\tlet owner = Owner::new();\n",
    "\tlet _sub = owner.take(count.sub(|value| { seen.push(value); }));\n",
    "\tboard.add(Todo { id = 4, title = \"ship it\", done = false });\n",
    "\tcount.set(board.open());\n",
    "\tprint(seen.len());\n",
    "\towner.dispose();\n",
    "\tcount.set(99);\n",
    "\tprint(seen.len());\n",
    "\tprint(count.get());\n",
    "}\n",
);

/// J6's exit: every async corpus program the backend ACCEPTS prints
/// byte-identically, and the ones it refuses say which construct stopped them.
///
/// The census is printed for the same reason the corpus sweep prints its own —
/// the list of refusals IS the work list, and a reader who runs this wants to
/// see it shrink.
#[test]
fn every_async_corpus_program_is_identical_or_named() {
    let staged = stage();
    let mut identical_programs: Vec<String> = Vec::new();
    let mut refused: Vec<(String, String)> = Vec::new();
    let mut broken = Vec::new();
    for program in ASYNC_SUITE {
        match compare(&staged, program) {
            Verdict::Identical => identical_programs.push((*program).to_string()),
            Verdict::Refused(reason) => refused.push(((*program).to_string(), reason)),
            Verdict::Broken(detail) => broken.push(format!("{program}: {detail}")),
        }
    }
    eprintln!(
        "async differential: {} enumerated, {} identical, {} refused by name, {} broken",
        ASYNC_SUITE.len(),
        identical_programs.len(),
        refused.len(),
        broken.len()
    );
    for (program, reason) in &refused {
        eprintln!("  refused  {program}: {reason}");
    }
    for program in &identical_programs {
        eprintln!("  identical  {program}");
    }
    assert!(
        broken.is_empty(),
        "async programs the native backend ACCEPTED and then got wrong:\n{}",
        broken.join("\n")
    );
    // The two that compile today, and they are the right two.
    // `await-postfix.vl` was written as a BYTE-level gate on where the
    // parentheses of an await go, and every helper in it awaits, so every call
    // to one is awaited on the caller's behalf. `nursery.vl` is structured
    // concurrency whole: a helper's spawn, a grandchild spawned by a running
    // child, and a join that must wait for a child list which GREW while it was
    // draining. If either stops being identical the executor or the emitter's
    // async arms have moved.
    for required in ["await-postfix.vl", "nursery.vl"] {
        assert!(
            identical_programs.iter().any(|program| program == required),
            "{required} must be byte-identical on both backends; the census was:\n{}",
            refused
                .iter()
                .map(|(program, reason)| format!("  {program}: {reason}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

/// The executor's ORDERING, through an emitted program rather than through a
/// `vilan-rt` unit test (J6).
///
/// `reactive-turns.vl` is the corpus's ordering pin and it is still refused for
/// its generics, so the ordering the backend must reproduce is pinned here
/// instead, on the three rules a reader can check by eye: a spawn is EAGER (its
/// `enter` prints before the line after the spawn expression), the deadline list
/// is ordered (the 1 ms task finishes before the 12 ms one that was spawned
/// FIRST), and a join answers the spawned value.
#[test]
fn the_spawn_and_sleep_ordering_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_spawn.vl"), SPAWN_ORDER_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_spawn.vl"),
        Verdict::Identical,
        "the executor's spawn and sleep ordering must match the JS turn model"
    );
}

/// `std::time::Timer`'s memoized verdict, through an emitted program: a fired
/// timer answers `true` twice (from the memo, not from a second timer) and a
/// cancelled one answers `false`.
#[test]
fn a_timers_verdict_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_timer.vl"), TIMER_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_timer.vl"),
        Verdict::Identical,
        "`Timer`'s verdict must be the same on both backends"
    );
}

/// `Task::settle_all` and `Task::race` — the two joins on `Task<T>`, through an
/// emitted program (J6).
///
/// `async-promise-all.vl` is the corpus's own pin on `settle_all` and it is
/// still refused, for its `[extern("node:timers/promises", "setTimeout")]`
/// rather than for anything about the join, so the same shape is pinned here
/// over `std::time::sleep`. `settle_all` preserves ORDER whatever the delays
/// are, and `race` answers the first task to settle while its loser keeps
/// running.
#[test]
fn the_two_task_joins_are_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_joins.vl"), TASK_JOIN_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_joins.vl"),
        Verdict::Identical,
        "`Task::settle_all` and `Task::race` must answer the same on both backends"
    );
}

const TASK_JOIN_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::task::Task;\n",
    "import std::time::sleep;\n",
    "\n",
    "fun delayed(label: str, ms: i32): str {\n",
    "\tsleep(ms);\n",
    "\tlabel\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut tasks: List<Task<str>> = List::new();\n",
    "\ttasks.push(async delayed(\"a\", 20));\n",
    "\ttasks.push(async delayed(\"b\", 10));\n",
    "\ttasks.push(async delayed(\"c\", 30));\n",
    "\tlet results: List<str> = Task::settle_all(tasks);\n",
    "\tfor result in results {\n",
    "\t\tprint(result);\n",
    "\t}\n",
    "\tmut racers: List<Task<str>> = List::new();\n",
    "\tracers.push(async delayed(\"slow\", 40));\n",
    "\tracers.push(async delayed(\"quick\", 5));\n",
    "\tprint(Task::race(racers));\n",
    "}\n",
);

const SPAWN_ORDER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::sleep;\n",
    "\n",
    "async fun step(label: str, ms: i32): str {\n",
    "\tprint(i\"enter {label}\");\n",
    "\tsleep(ms);\n",
    "\tprint(i\"leave {label}\");\n",
    "\tlabel\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet late = async step(\"late\", 12);\n",
    "\tlet early = async step(\"early\", 1);\n",
    "\tprint(\"spawned\");\n",
    "\tprint(await early);\n",
    "\tprint(await late);\n",
    "}\n",
);

const TIMER_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::Timer;\n",
    "\n",
    "fun main() {\n",
    "\tlet fired = Timer::after(1);\n",
    "\tprint(fired.wait());\n",
    "\tprint(fired.wait());\n",
    "\tlet called_off = Timer::after(500);\n",
    "\tcalled_off.cancel();\n",
    "\tprint(called_off.wait());\n",
    "\tprint(\"done\");\n",
    "}\n",
);

#[test]
fn the_enumeration_finds_a_corpus_and_excludes_the_platform_programs() {
    let programs = platform_free_programs();
    assert!(
        programs.len() > 80,
        "the platform-free enumeration found only {} programs, which means the corpus moved or \
         the filter over-reaches",
        programs.len()
    );
    assert!(
        !programs
            .iter()
            .any(|program| program == "db.vl" || program == "file.vl"),
        "a program reaching the filesystem or a database is not platform-free"
    );
    assert!(programs.iter().any(|program| program == "bool.vl"));
}
