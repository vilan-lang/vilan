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

use std::io::{BufRead, Read, Write};
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
///
/// F18 slice 2 adds the two JSON rows: `derive-json.vl` (a `[derive(Json)]`
/// struct in both directions, a nested struct, a missing field and a
/// wrong-typed one) and `json-roundtrip.vl` (the `List`/`Option` blankets,
/// which reach a scalar's `[extern("JSON.stringify")]` member through a
/// generic dispatch). They were refused for four separate reasons before
/// `vilan_rt::json` existed, and every one of them is a row of the slice: the
/// host type, the six intrinsics, the `!` assertion the derived decoders are
/// written in, and a capturing `is`-test to the left of `&&`.
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
    "derive-json.vl",
    "json-roundtrip.vl",
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

/// The four lowering classes a `std::http` server program reaches and no corpus
/// program does (F20, found by lane native-b-39 while building the HTTP
/// runtime).
///
/// They are pinned as ONE probe rather than four because they are one shape —
/// an aggregate that holds a callback — seen from four sides, and because the
/// program that found them is not in the corpus, so the whole-set gate cannot
/// see any of them. In order: a struct field declared `async |T| U` (the type is
/// `Rc<dyn Fn(T) -> Boxed<U>>` and a SYNC literal landing in it is wrapped,
/// which is `Server::builder()`'s default handler); a field holding an `Option`
/// of a closure, which defeats the derived `PartialEq` exactly as a bare closure
/// field does; an ENUM payload holding a closure, which `ensure_enum` guarded
/// neither for equality nor for rendering; and a non-`Copy` field read off a
/// `&`-loaned receiver, which is a MOVE out of a shared reference where
/// `clone_sites` elided rule 1's copy.
#[test]
fn an_aggregate_holding_a_callback_compiles_the_same_way_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_callbacks.vl"), CALLBACK_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_callbacks.vl"),
        Verdict::Identical,
        "an aggregate holding a callback must compile and print identically on both backends"
    );
}

const CALLBACK_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    // A struct field declared `async |T| U`, plus an `Option` of a closure.
    "struct Server {\n",
    "\thandler: async |i32| str,\n",
    "\ton_close: Option<|| void>,\n",
    "\tname: str,\n",
    "}\n",
    "\n",
    "impl Server {\n",
    "\tfun builder(): Server {\n",
    // A SYNC literal in the async field — the wrap.
    "\t\tServer { handler = |n| { \"default\" }, on_close = None, name = \"s\" }\n",
    "\t}\n",
    "\n",
    "\tasync fun serve(self, n: i32): str {\n",
    "\t\tawait (self.handler)(n)\n",
    "\t}\n",
    "}\n",
    "\n",
    // An enum PAYLOAD holding a closure.
    "enum Body {\n",
    "\tFixed(str),\n",
    "\tStream(|| str),\n",
    "}\n",
    "\n",
    "fun render(body: Body): str {\n",
    "\tmatch body {\n",
    "\t\tBody::Fixed(let text) => text,\n",
    "\t\tBody::Stream(let make) => make(),\n",
    "\t}\n",
    "}\n",
    "\n",
    // A non-`Copy` field read off a loaned receiver.
    "struct Builder { body: str, items: List<i32> }\n",
    "\n",
    "impl Builder {\n",
    "\tfun build(self): Builder {\n",
    "\t\tBuilder { body = self.body, items = self.items }\n",
    "\t}\n",
    "}\n",
    "\n",
    "async fun main() {\n",
    "\tlet server = Server::builder();\n",
    "\tprint(await server.serve(1));\n",
    "\tlet custom = Server { handler = |n| { i\"n={n}\" }, on_close = None, name = \"c\" };\n",
    "\tprint(await custom.serve(7));\n",
    "\tprint(render(Body::Fixed(\"fixed\")));\n",
    "\tprint(render(Body::Stream(|| { \"streamed\" })));\n",
    "\tlet builder = Builder { body = \"b\", items = [1, 2] };\n",
    "\tlet copy = builder.build();\n",
    "\tprint(copy.body);\n",
    "\tprint(copy.items.len());\n",
    "}\n",
);

/// A `lazy` parameter defers, memoizes and FORWARDS the same way on both
/// backends (F20; `lazy.md` §1).
///
/// The corpus covers the feature well — thirteen programs reach a `lazy`
/// parameter, which is the whole `Option`/`Result` combinator surface since
/// A103 — but every one of them reaches it through a combinator whose argument
/// is a plain value, so none of them can tell a deferral from an eager
/// evaluation. This probe can: the argument counts its own evaluations and the
/// program prints the counter, so an emitter that forced at the call site prints
/// different numbers rather than the same ones.
#[test]
fn a_lazy_parameter_defers_and_memoizes_the_same_way_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_lazy.vl"), LAZY_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_lazy.vl"),
        Verdict::Identical,
        "a `lazy` parameter must defer, memoize and forward identically on both backends"
    );
}

const LAZY_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "mut evaluations = 0;\n",
    "\n",
    "fun costly(): str {\n",
    "\tevaluations = evaluations + 1;\n",
    "\ti\"built {evaluations}\"\n",
    "}\n",
    "\n",
    // The argument is never read, so the thunk never runs.
    "fun ignore(lazy message: str): i32 { 1 }\n",
    // Read TWICE: one evaluation, two reads.
    "fun twice(lazy message: str): str {\n",
    "\tlet first = message;\n",
    "\tlet second = message;\n",
    "\ti\"{first}/{second}\"\n",
    "}\n",
    // A FORWARD: the cell travels as-is, so the chain memoizes once.
    "fun forwards(lazy message: str): str { twice(message) }\n",
    "\n",
    "fun main() {\n",
    "\tprint(ignore(costly()));\n",
    "\tprint(evaluations);\n",
    "\tprint(twice(costly()));\n",
    "\tprint(evaluations);\n",
    "\tprint(forwards(costly()));\n",
    "\tprint(evaluations);\n",
    // A literal in the same position still works, and so does a value the
    // callee reads on only one of two paths.
    "\tprint(twice(\"plain\"));\n",
    "\tprint(evaluations);\n",
    // A103's own customers: `unwrap_or`'s fallback is `lazy`, so the `Some`
    // path never builds it and the `None` path does.
    "\tlet present: Option<i32> = Some(3);\n",
    "\tprint(present.unwrap_or(costly().len()));\n",
    "\tprint(evaluations);\n",
    "\tlet absent: Option<i32> = None;\n",
    "\tprint(absent.unwrap_or(costly().len()));\n",
    "\tprint(evaluations);\n",
    "}\n",
);

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

/// F23: a context-threaded hidden parameter is typed from the flavour the
/// CONTEXT PASS recorded, and one program carries both readings.
///
/// The parameter is deliberately record-less (no `parameters` entry, no span, no
/// type — editing-dx.md §19.3), so the Rust emitter, which must write a type
/// down, used to re-derive the flavour from the arguments every call site passes
/// — a worklist that had to connect clause-typed parameters to every closure
/// literal that can land there, and that fell back on the strict reading when
/// nothing settled. `context.rs` knows the answer where it MINTS the parameter:
/// a node holds the bare value when its provider is strict or a `run` closure,
/// and `Option<T>` otherwise.
///
/// The probe puts a safe reader (`peek`, which `get_safe`s) and a strict one
/// (`strict_report`, which `get`s and then calls `peek`) in one program, so the
/// two flavours are asserted against each other rather than one at a time: a
/// record that is uniformly wrong, or uniformly right for the wrong reason,
/// cannot pass both halves. Flipping the recorded bool swaps the two signatures,
/// which is what makes this non-vacuous.
#[test]
fn a_context_threaded_parameter_is_typed_from_the_recorded_flavour() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_context_flavour.vl"),
        CONTEXT_PROBE,
    )
    .expect("write the probe program");
    let emitted = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_context_flavour.vl",
        ])
        .output()
        .expect("build the probe");
    assert!(
        emitted.status.success(),
        "the context-flavour probe was refused:\n{}",
        String::from_utf8_lossy(&emitted.stderr)
    );
    let source = String::from_utf8_lossy(&emitted.stdout);
    let signature = |prefix: &str| -> String {
        source
            .lines()
            .find(|line| line.starts_with(&format!("fn {prefix}")))
            .unwrap_or_else(|| panic!("no `fn {prefix}..` in the emitted source:\n{source}"))
            .to_string()
    };
    // The safe region's parameter carries the `Option`; the strict region's
    // carries the value, because `run` hands it one.
    let safe = signature("peek_");
    assert!(
        safe.contains(": Option<i32>"),
        "a `get_safe`-reachable region's hidden parameter must be `Option<i32>`: {safe}"
    );
    let strict = signature("strict_report_");
    assert!(
        strict.contains(": i32") && !strict.contains(": Option<i32>"),
        "a strict region's hidden parameter must be the bare value: {strict}"
    );
    // And the program means the same thing on both backends, which is what the
    // types have to be right FOR.
    assert_eq!(
        compare(&staged, "native_probe_context_flavour.vl"),
        Verdict::Identical,
        "the context-threaded program must agree on both backends"
    );
}

/// Both context flavours in one program: `peek` is safe (it `get_safe`s, so its
/// hidden parameter is an `Option`), `strict_report` is strict (it `get`s, so
/// `run` hands it the bare value) and calls `peek`, which is the covered→safe
/// boundary that `Some`-wraps.
const CONTEXT_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::context::Context;\n",
    "import std::option::Option::{ Some, None };\n",
    "\n",
    "let current: Context<i32> = Context::new();\n",
    "\n",
    "fun peek(): str {\n",
    "\tmatch current.get_safe() {\n",
    "\t\tSome(let value) => i\"peeked {value}\",\n",
    "\t\tNone => \"nothing\",\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun strict_report() {\n",
    "\tlet value = current.get();\n",
    "\tprint(i\"strict {value}\");\n",
    "\tprint(peek());\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tprint(peek());\n",
    "\tcurrent.run(9, || {\n",
    "\t\tstrict_report();\n",
    "\t});\n",
    "\tprint(peek());\n",
    "}\n",
);

/// A destructuring `let` means the same thing on both backends (F18).
///
/// `std::http`'s response loop is what wanted it — `for header in
/// response.headers { let (name, value) = header; .. }` — and the emitter
/// refused the form by name. It is not an HTTP construct, so it is pinned as
/// what it is: a tuple pattern in a `let`, nested, with a wildcard, over a
/// binding and over a loop binder.
#[test]
fn a_destructuring_let_is_byte_identical_on_both_backends() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_destructure.vl"),
        DESTRUCTURE_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_destructure.vl"),
        Verdict::Identical,
        "a destructuring `let` must mean the same thing on both backends"
    );
}

const DESTRUCTURE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet pair = (1, \"one\");\n",
    "\tlet (number, word) = pair;\n",
    "\tprint(number);\n",
    "\tprint(word);\n",
    "\tlet nested = ((2, 3), \"two\");\n",
    "\tlet ((left, right), label) = nested;\n",
    "\tprint(left + right);\n",
    "\tprint(label);\n",
    "\tlet (_, kept) = pair;\n",
    "\tprint(kept);\n",
    "\tlet rows = [(1, \"a\"), (2, \"b\")];\n",
    "\tfor row in rows {\n",
    "\t\tlet (index, name) = row;\n",
    "\t\tprint(i\"{index}={name}\");\n",
    "\t}\n",
    "}\n",
);

/// F18 slice 1: the emitter reaches `vilan_rt::http`.
///
/// A `std::http` server program EMITS, and what comes out names the runtime's
/// own calls rather than a refusal — `create_server`, the bound `listen`, the
/// request body read, and the response's status/header/end. Twenty-three of
/// `std::http`'s twenty-five raw `node:http` bindings are answered by
/// `vilan_rt::http`; the two that are not are `NodeRequest::headers` and
/// `NodeSocket::remoteAddress`, which answer a `JsonValue` and are Order 40's,
/// and this program reaches neither.
///
/// It asserts the CALLS and not merely that the emit succeeded, because an
/// emitter that refused every binding under the census's `unimplemented!()`
/// would also "succeed". The slice's EXIT — the same program built, run, and
/// answering a GET over a real socket — is
/// [`a_native_std_http_server_answers_a_get_over_a_real_socket`]; this pin is
/// the cheap half, and it is what says WHICH bindings the exit went through.
#[test]
fn a_std_http_server_emits_calls_into_the_native_runtime() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_http.vl"), HTTP_PROBE)
        .expect("write the probe program");
    let output = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_http.vl",
        ])
        .output()
        .expect("build the probe");
    assert!(
        output.status.success(),
        "the http probe was refused:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source = String::from_utf8_lossy(&output.stdout);
    for needle in [
        "vilan_rt::http::create_server",
        "vilan_rt::http::read_request_bytes",
        ").listen(",
        ").address()",
        ").port()",
        ").set_status_code(",
        ").set_header(",
        ").end(",
        ").end_bytes(",
        "vilan_rt::http::Request",
        "vilan_rt::http::Response",
        "vilan_rt::http::Bytes",
    ] {
        assert!(
            source.contains(needle),
            "the emitted server must reach `{needle}`:\n{source}"
        );
    }
    assert!(
        !source.contains("unimplemented!()"),
        "a build emit must never carry a census placeholder:\n{source}"
    );
}

/// A `std::http` server built from the struct directly, which is the smallest
/// program that reaches the whole `node:http` surface `Server::start` binds.
///
/// `Server::builder()` is the shipped spelling and it is NOT used here: its
/// `build()` folds the rpc service list, which sorts (a backed enum plus the
/// `ListSortBy` intrinsic, both other lanes' items) and reaches `serve_build`'s
/// conditional-GET arm (`std::json`'s host type, Order 40's). The literal
/// reaches the same server.
const HTTP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::http::{ Server, Response };\n",
    "import std::option::Option::None;\n",
    "\n",
    "fun main() {\n",
    "\tlet server = Server {\n",
    "\t\tport = 0,\n",
    "\t\trequest_handler = |request| Response::builder()\n",
    "\t\t\t.set_header(\"Content-Type\", \"text/plain\")\n",
    "\t\t\t.body(\"hello\\n\")\n",
    "\t\t\t.build(),\n",
    "\t\ton_start = |started| print(i\"vilan-test-port={started.port()}\"),\n",
    "\t\ton_stop = |stopped| {},\n",
    "\t\tupgrade_handler = None,\n",
    "\t\tnode = None,\n",
    "\t};\n",
    "\tserver.start();\n",
    "}\n",
);

/// **F18 slice 1's EXIT**: a `std::http` server compiled with `--backend rust`
/// runs as a native binary and answers a `GET /` over a real socket, and the JS
/// twin answers the same thing.
///
/// The whole slice is measured here. Everything else about it — the
/// dependency-free HTTP/1.1 server in `vilan-rt`, the `IoSource` turn, the
/// twenty-three `node:http` bindings, the executor's free list — exists so that
/// this program serves a request, and a runtime whose own unit tests pass while
/// the compiler cannot reach it would be a runtime nobody can use.
///
/// **What is compared, and what cannot be.** The status line, the header the
/// PROGRAM set, and the body, byte for byte on both legs. Not the whole
/// response: node adds a `Date`, which changes every second, and node and this
/// server order `Connection` and `Content-Length` differently — two facts
/// written down rather than normalised away, because a reader deserves to know
/// the comparison is not the whole wire. stdout IS compared whole, and it is
/// the port announcement, which is the same line from both.
///
/// **The ordering is the harness's own, not a sleep.** The SERVER binds port 0
/// and announces the number it got; the fetch cannot start before that line has
/// arrived, because the line is where the number comes from. There is no
/// bind-release-rebind window (`support/port.rs`'s N40 finding) and no sleep
/// standing in for a happens-before.
#[test]
fn a_native_std_http_server_answers_a_get_over_a_real_socket() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_http.vl"), HTTP_PROBE)
        .expect("write the probe program");

    // The native leg: build, then run the BINARY rather than `vilan run`, so
    // the child this test kills is the server itself and not a parent that
    // would outlive it.
    let built = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_http.vl"])
        .output()
        .expect("build the server natively");
    assert!(
        built.status.success(),
        "the native leg did not build:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    let binary = String::from_utf8_lossy(&built.stdout)
        .lines()
        .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
        .expect("`vilan build` says where the binary is");
    let native = ServedRequest::take(Command::new(staged.join(&binary)));

    // The JS leg, the same program, the same request.
    let bundled = vilan(&staged)
        .args(["build", "native_probe_http.vl"])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut node = Command::new("node");
    node.current_dir(&staged).arg("native_probe_http.mjs");
    let javascript = ServedRequest::take(node);

    assert_eq!(
        native.status, javascript.status,
        "the two backends must answer the same status line"
    );
    assert_eq!(
        native.status, "HTTP/1.1 200 OK",
        "and it is a 200 — a pin that agreed on a 500 would agree about nothing"
    );
    assert_eq!(
        native.body, javascript.body,
        "the two backends must answer the same body"
    );
    assert_eq!(native.body, "hello\n", "and it is the handler's own body");
    // The header the PROGRAM set goes out on both. Node's `Date` and the order
    // it writes `Connection`/`Content-Length` in are its own; see this test's
    // header comment.
    for leg in [&native, &javascript] {
        assert!(
            leg.headers
                .iter()
                .any(|line| line == "Content-Type: text/plain"),
            "the program's header must reach the wire: {:?}",
            leg.headers
        );
        assert!(
            leg.headers.iter().any(|line| line == "Content-Length: 6"),
            "a buffered body declares its length: {:?}",
            leg.headers
        );
    }
    assert_eq!(
        native.announced_line.split('=').next(),
        javascript.announced_line.split('=').next(),
        "both legs announce through the same `on_start`"
    );
}

/// One request answered by a spawned server, and the pieces of the answer the
/// two backends can be held to.
struct ServedRequest {
    status: String,
    headers: Vec<String>,
    body: String,
    announced_line: String,
}

impl ServedRequest {
    /// Spawns `command`, waits for the port IT bound, fetches `GET /`, and
    /// reaps the child.
    ///
    /// The child is killed on the way out of this function on every path,
    /// including a panic inside it, because [`ServerUnderTest`] owns it and its
    /// `Drop` does the kill — a failed assertion must not leak a listener into
    /// the rest of the suite.
    fn take(mut command: Command) -> ServedRequest {
        let server = ServerUnderTest::spawn(&mut command);
        let port = server.port();
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
            .expect("connect to the port the server announced");
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
            .expect("send the request");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .expect("read the response");
        let (head, body) = response
            .split_once("\r\n\r\n")
            .unwrap_or_else(|| panic!("a response with a head and a body, got {response:?}"));
        let mut lines = head.split("\r\n");
        let status = lines.next().unwrap_or_default().to_string();
        ServedRequest {
            status,
            headers: lines.map(str::to_string).collect(),
            body: body.to_string(),
            announced_line: server.announcement.clone(),
        }
    }
}

/// A spawned server whose port is the one it actually bound, killed on drop.
///
/// `support/port.rs` is the same mechanism for the e2e suites; this binary has
/// no `mod support`, and the twenty lines are cheaper than giving it one for a
/// single test.
struct ServerUnderTest {
    child: std::process::Child,
    announcement: String,
    port: u16,
}

impl ServerUnderTest {
    fn spawn(command: &mut Command) -> ServerUnderTest {
        let mut child = command
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("spawn the server");
        let stdout = child.stdout.take().expect("the server's stdout");
        let (sender, lines) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in std::io::BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
            {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        let mut server = ServerUnderTest {
            child,
            announcement: String::new(),
            port: 0,
        };
        // A LIVENESS bound, not a claim about how fast a server boots: a green
        // spawn returns the moment the line lands.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!remaining.is_zero(), "the server never announced its port");
            match lines.recv_timeout(remaining) {
                Ok(line) => {
                    if let Some(number) = line
                        .split_whitespace()
                        .find_map(|field| field.strip_prefix("vilan-test-port="))
                    {
                        let port: u16 = number.parse().expect("the announced port is a number");
                        assert_ne!(
                            port, 0,
                            "the server reported the port it ASKED for, not one it bound"
                        );
                        server.announcement = line;
                        server.port = port;
                        return server;
                    }
                }
                Err(_) => panic!("the server's stdout ended before it announced a port"),
            }
        }
    }

    fn port(&self) -> u16 {
        self.port
    }
}

impl Drop for ServerUnderTest {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// F25: a program that FAILS answers the same exit code on both backends, and
/// the native binary does not print Rust's panic banner.
///
/// Node prints the error and exits 1; Rust prints
/// `thread 'main' panicked at src/main.rs:N:M:`, a `note: run with
/// RUST_BACKTRACE=1` line, and exits 101. stdout is what the differential
/// compares and it was already identical, so this is about what a SHELL sees —
/// and a shell reading 101 where the JS build gave it 1 is one program
/// answering two different things.
///
/// Both the synchronous and the `async fun main` paths, because they are two
/// different emitted shapes: one wraps the body, the other wraps the
/// `block_on`.
#[test]
fn a_failing_program_exits_one_on_both_backends_without_rusts_banner() {
    let staged = stage();
    for (file, source, expected_stdout) in [
        ("native_probe_panic.vl", PANIC_PROBE, "before the panic\n"),
        (
            "native_probe_panic_async.vl",
            ASYNC_PANIC_PROBE,
            "before the async panic\n",
        ),
    ] {
        std::fs::write(staged.join(file), source).expect("write the probe program");
        let native = vilan(&staged)
            .args(["run", "--backend", "rust", file])
            .output()
            .expect("run the failing probe natively");
        let javascript = vilan(&staged)
            .args(["run", file])
            .output()
            .expect("run the failing probe on the JS backend");
        assert_eq!(
            native.status.code(),
            Some(1),
            "{file}: the native binary must exit 1, not Rust's 101:\n{}",
            String::from_utf8_lossy(&native.stderr)
        );
        assert_eq!(
            javascript.status.code(),
            Some(1),
            "{file}: the JS leg is the oracle and it exits 1"
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            String::from_utf8_lossy(&javascript.stdout),
            "{file}: stdout up to the failure must still be identical"
        );
        assert_eq!(
            String::from_utf8_lossy(&native.stdout),
            expected_stdout,
            "{file}: the probe must get as far as its own output, or this pin is \
             asserting nothing about the failure"
        );
        let stderr = String::from_utf8_lossy(&native.stderr);
        assert!(
            !stderr.contains("panicked at"),
            "{file}: Rust's panic banner must not reach stderr: {stderr:?}"
        );
        assert!(
            !stderr.contains("RUST_BACKTRACE"),
            "{file}: Rust's backtrace note must not reach stderr: {stderr:?}"
        );
        // ONE line about the failure, and it is the program's own message —
        // node prints one too. Two was the shape before the executor stopped
        // reporting the root task as an unobserved failure.
        let said: Vec<&str> = stderr.lines().filter(|line| !line.is_empty()).collect();
        assert_eq!(
            said.len(),
            1,
            "{file}: one line about one failure: {said:?}"
        );
        assert!(
            said[0].contains("boom"),
            "{file}: and it is the program's own message: {said:?}"
        );
    }
}

const PANIC_PROBE: &str = concat!(
    "import std::io::{ print, panic };\n",
    "\n",
    "fun main() {\n",
    "\tprint(\"before the panic\");\n",
    "\tpanic(\"boom\");\n",
    "}\n",
);

const ASYNC_PANIC_PROBE: &str = concat!(
    "import std::io::{ print, panic };\n",
    "import std::time::sleep;\n",
    "\n",
    "async fun main() {\n",
    "\tprint(\"before the async panic\");\n",
    "\tsleep(1);\n",
    "\tpanic(\"boom\");\n",
    "}\n",
);

/// F25: printing a host handle or a value holding a function is refused where
/// it is WRITTEN.
///
/// Order 38 answered both with a runtime panic carrying the reason, which is
/// honest and one release too late — what node prints there is its own object
/// inspection (`Promise { <pending> }`, `[Function (anonymous)]`), so the
/// program cannot work and nothing is gained by letting it build.
#[test]
fn printing_a_host_handle_or_a_function_is_refused_at_compile_time() {
    let staged = stage();
    for (file, source, needle) in [
        (
            "native_probe_print_task.vl",
            PRINT_HANDLE_PROBE,
            "`print` of the host handle `Task`",
        ),
        (
            "native_probe_print_fn.vl",
            PRINT_FUNCTION_PROBE,
            "`print` of a value holding a function",
        ),
    ] {
        std::fs::write(staged.join(file), source).expect("write the probe program");
        let output = vilan(&staged)
            .args(["build", "--backend", "rust", "--stdout", file])
            .output()
            .expect("build the probe");
        let message = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "{file} must be refused rather than built"
        );
        assert!(
            message.contains(needle),
            "{file} must be refused by name (`{needle}`); it said:\n{message}"
        );
        // Non-vacuous: the JS backend BUILDS the same program, so the refusal
        // is the native backend's answer and not a defect in the probe.
        let javascript = vilan(&staged)
            .args(["build", file])
            .output()
            .expect("build the probe on the JS backend");
        assert!(
            javascript.status.success(),
            "{file} must be a program the JS backend accepts:\n{}",
            String::from_utf8_lossy(&javascript.stderr)
        );
    }
}

const PRINT_HANDLE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::sleep;\n",
    "\n",
    "async fun work(): i32 {\n",
    "\tsleep(1);\n",
    "\t7\n",
    "}\n",
    "\n",
    "async fun main() {\n",
    "\tlet task = async work();\n",
    "\tprint(task);\n",
    "}\n",
);

const PRINT_FUNCTION_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet f = |x: i32| x + 1;\n",
    "\tprint(f);\n",
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
