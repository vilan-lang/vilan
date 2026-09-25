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
//!
//! # No assertion in this suite reads a clock (N116)
//!
//! Nothing here times anything, and that is deliberate — but a probe program
//! that SLEEPS is the same mistake wearing a different hat. The comparison is
//! over bytes, and if the ordering of those bytes depends on two deadlines
//! that a busy box can let expire together, the pin measures the box.
//!
//! So a probe whose claim is an ORDER states it with a gap no scheduling stall
//! on this machine closes: **the later deadline is at least ten times the
//! earlier one and at least 200 ms in absolute terms**, so the runtime has to
//! reach its timer phase at least that late before the two can tie. Where the
//! order is not the claim, a probe does not sleep at all. The alternative — a
//! virtual clock injected into `vilan-rt`'s deadline list — cannot reach here:
//! this suite compares two REAL processes, one of them `node`, and node's
//! clock is not ours to move.

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
///
/// F32 adds the two `BigInt` rows. `remainder.vl` and `numeric-types.vl` are
/// the corpus's only `n` literals, and both were refused by name after Order
/// 39 caught the narrowing miscompile behind them (`9007199254740993n` had been
/// emitting `…993i32`). They print `1n` and `3n` on both backends now, which is
/// the whole of what the `i128` ruling claims.
///
/// F33 adds `bytes-aliasing.vl`: the mutable `Bytes` the ruling chose, read
/// through every kind of alias a program can make.
///
/// A124 R3 adds `dyn-objects.vl`: trait objects on both backends — a
/// heterogeneous `List` behind a struct field, a supertrait member and a
/// parameterized trait through the table, a blanket and a generic over the
/// bound, printing a struct that holds one (`[ value, {} ]` on both sides),
/// and a cold node over a `dyn Source<i32>` notified through its upstream.
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
    "remainder.vl",
    "numeric-types.vl",
    // F21: a VIEW inside an enum payload (`Option<&mut T>`) — the shape behind
    // a view-returning `Arena::get`, refused by name since native-b-38. Every
    // projection in it is matched where it is built, which is the position the
    // payload view is carried through.
    "option-view.vl",
    "dyn-objects.vl",
    // I5 S1: `usize`, the index type — the platform word natively, `u53` on JS,
    // printing the same family surface on both.
    "usize.vl",
    // I5 S1 + B389: the literal law at `usize`, all 21 measured positions in one
    // program — the literals' native widths are the record B389 writes.
    "usize-literals.vl",
    // F33 (RULED (a)): `Bytes` is one shared, mutable buffer behind every
    // holder — two bindings, a parameter, a struct copy, a list and a closure
    // capture all write into the same bytes, as a `Uint8Array` does; `slice`
    // and `concat` are the two that make a new one.
    "bytes-aliasing.vl",
    // F18 slice 3: `ListCell`'s writes through `SignalCell::update(|&mut
    // list| ..)` and a `Delta` pushed with a parameter its payload leaves open
    // — refused at the Order 40 seal, byte-identical now.
    "delta-law.vl",
    // F34: `map` (a default's own generic bound from its closure argument) and
    // `flatten` (a `?` lift over an `Option`), both refused at the Order 40
    // seal.
    "reactive-on-change.vl",
    "reactive-flatten.vl",
];

/// Corpus programs that are OUTSIDE this differential by construction, named
/// with the reason (I5 ruling 2, `proposal/index-type.md` §5.2).
///
/// Not refusals and not breakages: programs whose output the language leaves
/// UNSPECIFIED, so the two backends legitimately print different things and
/// "identical" is not a claim either could make. An underflowing `usize`
/// subtraction goes negative on JS and panics in a native debug build; the
/// corpus keeps its JS golden, and
/// [`an_underflowing_usize_is_outside_the_differential_by_name`] pins the
/// native half.
const OUTSIDE_THE_DIFFERENTIAL: &[(&str, &str)] = &[(
    "usize-underflow.vl",
    "a `usize` subtracted past zero is unspecified: -1 on JS, a debug panic natively",
)];

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
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let outside = OUTSIDE_THE_DIFFERENTIAL
                .iter()
                .any(|(excluded, _)| *excluded == name);
            if !reaches_a_platform && !outside {
                programs.push(name);
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

/// I5 ruling 2: the one program [`OUTSIDE_THE_DIFFERENTIAL`] names does what
/// the book says on each backend — JS prints the negative value and exits 0, a
/// native debug build panics on the subtraction — and the enumeration leaves it
/// out, so the sweep cannot call it broken.
#[test]
fn an_underflowing_usize_is_outside_the_differential_by_name() {
    for (program, _) in OUTSIDE_THE_DIFFERENTIAL {
        assert!(
            corpus_dir().join(program).is_file(),
            "{program} is named outside the differential but is not a corpus program"
        );
        assert!(
            !platform_free_programs().contains(&program.to_string()),
            "{program} is named outside the differential but the sweep still enumerates it"
        );
        assert!(
            !DEFAULT_SUITE.contains(program),
            "{program} is outside the differential and cannot be in its default suite"
        );
    }
    let staged = stage();
    let javascript = vilan(&staged)
        .args(["run", "usize-underflow.vl"])
        .output()
        .expect("run the JS backend");
    assert!(
        javascript.status.success(),
        "the JS leg runs through the underflow: {}",
        String::from_utf8_lossy(&javascript.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "-1\ntrue\ntrue\n0\n"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "usize-underflow.vl"])
        .output()
        .expect("run the native backend");
    let stderr = String::from_utf8_lossy(&native.stderr);
    assert!(
        !native.status.success() && stderr.contains("attempt to subtract with overflow"),
        "the native debug build panics on the subtraction, it does not print a value:\n\
         stdout: {}\nstderr: {stderr}",
        String::from_utf8_lossy(&native.stdout)
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
        "vilan_rt::bytes::Bytes",
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

/// **F18 slice 2's EXIT**: a program with the SHAPE of kolt's server leg —
/// a SQLite store, a hashed password, an `/api/login` route that decodes a POST
/// body and answers a `[derive(Json)]` outcome, and a shell for every other
/// path — runs as a native binary and answers a login over a real socket.
///
/// **Why a shape and not the file.** Kolt is read-only for this tree and is
/// never copied into it; what is reproduced is the STRUCTURE the slice had to
/// carry, which is what the exit is measuring. Everything the slice built is on
/// the path: `std::json` (the derived encode, and `List<str>::from_json` over
/// the request body), `std::db` through the separate `vilan-rt-sqlite` crate,
/// `std::crypto`'s SHA-256, `Bytes` and `TextDecoder`, `std::http` over a real
/// socket, and a `for` over an `Iterator` impl (`Bytes::to_hex` walks a
/// `Range`).
///
/// **What is compared.** Three exchanges, each byte for byte on both legs: a
/// good login, a bad one, and the shell — status line, the header the program
/// set, `Content-Length` and the body. NOT compared, for Order 39's reasons
/// written at [`a_native_std_http_server_answers_a_get_over_a_real_socket`]:
/// node's `Date`, and the order the two write `Connection`/`Content-Length`.
///
/// **Non-vacuous by its content, not by its exit code.** The bodies are
/// asserted verbatim, and the two logins differ only in the password — so a
/// server that answered a constant, or one whose hash comparison always held,
/// fails on the second exchange.
#[test]
fn the_kolt_server_shape_answers_a_login_over_a_real_socket() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_kolt.vl"), KOLT_SHAPE_PROBE)
        .expect("write the probe program");

    let built = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_kolt.vl"])
        .output()
        .expect("build the server natively");
    assert!(
        built.status.success(),
        "the native leg did not build:\n{}{}",
        String::from_utf8_lossy(&built.stdout),
        String::from_utf8_lossy(&built.stderr)
    );
    // Order 39's R1, asserted rather than assumed: the SQLite crate is named by
    // the manifest of a program that reaches `std::db`.
    let manifest = std::fs::read_to_string(
        staged
            .join("dist")
            .join("native")
            .join("native_probe_kolt")
            .join("Cargo.toml"),
    )
    .expect("read the generated manifest");
    assert!(
        manifest.contains("vilan-rt-sqlite"),
        "a program reaching `std::db` depends on the SQLite crate:\n{manifest}"
    );
    let binary = String::from_utf8_lossy(&built.stdout)
        .lines()
        .find_map(|line| line.split(" -> ").nth(1).map(str::to_string))
        .expect("`vilan build` says where the binary is");
    let native = ServedLogin::take(Command::new(staged.join(&binary)));

    let bundled = vilan(&staged)
        .args(["build", "native_probe_kolt.vl"])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut node = Command::new("node");
    node.current_dir(&staged).arg("native_probe_kolt.mjs");
    let javascript = ServedLogin::take(node);

    assert_eq!(
        native.exchanges, javascript.exchanges,
        "the two backends must answer the same three exchanges"
    );
    let expected = [
        (
            "HTTP/1.1 200 OK",
            "Content-Type: application/json",
            "{\"ok\":true,\"message\":\"welcome ada\"}",
        ),
        (
            "HTTP/1.1 200 OK",
            "Content-Type: application/json",
            "{\"ok\":false,\"message\":\"wrong password\"}",
        ),
        (
            "HTTP/1.1 200 OK",
            "Content-Type: text/html",
            "<!doctype html><title>shape</title>",
        ),
    ];
    for (answered, (status, header, body)) in native.exchanges.iter().zip(expected) {
        assert_eq!(answered.status, status);
        assert_eq!(answered.body, body);
        assert!(
            answered.headers.iter().any(|line| line == header),
            "the program's header must reach the wire: {:?}",
            answered.headers
        );
        assert!(
            answered
                .headers
                .iter()
                .any(|line| line == &format!("Content-Length: {}", body.len())),
            "a buffered body declares its length: {:?}",
            answered.headers
        );
    }
}

const KOLT_SHAPE_PROBE: &str = concat!(
    "// THE SHAPE of kolt's server leg (`src/server.vl` + the `KoltAuth` half of\n",
    "// `src/store.vl`), written here from scratch: one `Server` over a SQLite\n",
    "// store, an `/api/login` route that reads a POST body, checks a password\n",
    "// against a hashed row and answers a `[derive(Json)]` outcome, and a shell\n",
    "// for every other path. Kolt's own files are never copied into this tree.\n",
    "import std::bytes::encode_utf8;\n",
    "import std::crypto::sha256;\n",
    "import std::db::Database;\n",
    "import std::http::{ Request, Response, Server };\n",
    "import std::io::print;\n",
    "import std::json::{ FromJson, Json };\n",
    "import std::option::Option::None;\n",
    "\n",
    "[derive(Json)]\n",
    "struct LoginOutcome {\n",
    "\tok: bool,\n",
    "\tmessage: str,\n",
    "}\n",
    "\n",
    "let store = open_store();\n",
    "\n",
    "fun open_store(): Database {\n",
    "\tlet db = Database::open(\":memory:\");\n",
    "\tdb.exec(\"CREATE TABLE account (id INTEGER PRIMARY KEY, username TEXT NOT NULL UNIQUE, hash TEXT NOT NULL)\");\n",
    "\tdb\n",
    "}\n",
    "\n",
    "async fun hash_password(username: str, password: str): str {\n",
    "\t// Kolt hashes with node:crypto's PBKDF2; the SHAPE is the same — a salted\n",
    "\t// digest of the password, stored beside the account.\n",
    "\tsha256(encode_utf8(username + \":\" + password)).to_hex()\n",
    "}\n",
    "\n",
    "async fun register(username: str, password: str): LoginOutcome {\n",
    "\tlet hashed = hash_password(username, password);\n",
    "\tstore.prepare(\"INSERT INTO account (username, hash) VALUES (?, ?)\").run([username, hashed]);\n",
    "\tLoginOutcome { ok = true, message = \"registered\" }\n",
    "}\n",
    "\n",
    "async fun login(username: str, password: str): LoginOutcome {\n",
    "\tlet hashed = hash_password(username, password);\n",
    "\tmatch store.prepare(\"SELECT hash FROM account WHERE username = ?\").first([username]) {\n",
    "\t\tSome(let row) => if row.text(\"hash\") == hashed {\n",
    "\t\t\tLoginOutcome { ok = true, message = \"welcome \" + username }\n",
    "\t\t} else {\n",
    "\t\t\tLoginOutcome { ok = false, message = \"wrong password\" }\n",
    "\t\t},\n",
    "\t\tNone => LoginOutcome { ok = false, message = \"no such account\" },\n",
    "\t}\n",
    "}\n",
    "\n",
    "async fun main() {\n",
    "\tregister(\"ada\", \"lovelace1\");\n",
    "\tlet server = Server {\n",
    "\t\tport = 0,\n",
    "\t\trequest_handler = |request| answer(request),\n",
    "\t\ton_start = |started| print(i\"vilan-test-port={started.port()}\"),\n",
    "\t\ton_stop = |stopped| {},\n",
    "\t\tupgrade_handler = None,\n",
    "\t\tnode = None,\n",
    "\t};\n",
    "\tserver.start();\n",
    "}\n",
    "\n",
    "async fun answer(request: Request): Response {\n",
    "\tif request.path() == \"/api/login\" && request.method() == \"POST\" {\n",
    "\t\tlet pair = List<str>::from_json(request.body()).unwrap_or([]);\n",
    "\t\tlet outcome = if pair.len() == 2 {\n",
    "\t\t\tlogin(pair.get(0).unwrap_or(\"\"), pair.get(1).unwrap_or(\"\"))\n",
    "\t\t} else {\n",
    "\t\t\tLoginOutcome { ok = false, message = \"malformed call\" }\n",
    "\t\t};\n",
    "\t\tret Response::builder()\n",
    "\t\t\t.set_header(\"Content-Type\", \"application/json\")\n",
    "\t\t\t.body(outcome.to_json())\n",
    "\t\t\t.build();\n",
    "\t}\n",
    "\tResponse::builder()\n",
    "\t\t.set_header(\"Content-Type\", \"text/html\")\n",
    "\t\t.body(\"<!doctype html><title>shape</title>\")\n",
    "\t\t.build()\n",
    "}\n",
);

/// **F18 slice 3**: `Server::builder()` — the shipped spelling, which every
/// server before this pin had to avoid for the `Server` struct literal — builds
/// natively and serves a build over a real socket, byte-for-byte with node,
/// INCLUDING `serve_build`'s conditional-GET arm.
///
/// The chain is the one kolt's server writes: `serve_build(require_build(..))`
/// (which reads the leg's manifest through `std::fs::stat` and `readFile` at
/// boot), `cache_build` with a validating policy, `on_request` for every path
/// the build does not claim, `on_start`. Three exchanges, each compared whole
/// between the legs (status, the headers the program and std set, the body):
/// the artifact with its `ETag` and `Cache-Control`; a REVALIDATION with that
/// `ETag` in `If-None-Match`, which is the `304` with no body and no length
/// (node sends none, and neither may this runtime — `Content-Length: 0` was
/// what it sent before this slice); and the fallback.
///
/// Red at the Order 40 seal: `Server::builder()` was refused by name (the
/// mutating `Bytes` bindings, `now_millis`, SHA-1), then — past those — rustc
/// refused fourteen emitted errors across five lowering classes (a `&mut` loan
/// handed on, a literal beside a `u32`, a consumed place read at a `let`, a
/// tuple, a field; an `async` hook in an `Option` field).
#[test]
fn the_builder_serves_its_build_and_revalidates_it_over_a_real_socket() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_builder.vl"), BUILDER_PROBE)
        .expect("write the probe program");
    let dist = staged.join("dist");
    std::fs::create_dir_all(&dist).expect("create the build directory");
    std::fs::write(dist.join("client.js"), "console.log(\"client\");\n")
        .expect("write the artifact");
    std::fs::write(
        dist.join("client.chunks.json"),
        "{\"leg\":\"client\",\"entry\":\"client.js\",\"styles\":null,\
         \"classic_script\":false,\"chunks\":[],\"assets\":[]}",
    )
    .expect("write the build manifest");

    let built = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_builder.vl"])
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
    let mut native_command = Command::new(staged.join(&binary));
    native_command.current_dir(&staged);
    let native = revalidated_exchanges(native_command);

    let bundled = vilan(&staged)
        .args(["build", "native_probe_builder.vl"])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut node = Command::new("node");
    node.current_dir(&staged).arg("native_probe_builder.mjs");
    let javascript = revalidated_exchanges(node);

    assert_eq!(
        native, javascript,
        "the two backends must answer the same three exchanges"
    );
    let [artifact, revalidated, fallback] = &native[..] else {
        panic!("three exchanges, got {native:?}");
    };
    assert_eq!(artifact.status, "HTTP/1.1 200 OK");
    assert_eq!(artifact.body, "console.log(\"client\");\n");
    assert!(
        artifact
            .headers
            .iter()
            .any(|line| line.starts_with("ETag: \""))
            && artifact
                .headers
                .iter()
                .any(|line| line == "Cache-Control: no-cache"),
        "the validating policy's headers: {:?}",
        artifact.headers
    );
    assert_eq!(revalidated.status, "HTTP/1.1 304 Not Modified");
    assert_eq!(revalidated.body, "");
    assert!(
        !revalidated
            .headers
            .iter()
            .any(|line| line.starts_with("Content-Length")),
        "a 304 declares no length: {:?}",
        revalidated.headers
    );
    assert_eq!(fallback.status, "HTTP/1.1 200 OK");
    assert_eq!(fallback.body, "fallback /other");
}

/// The artifact, its revalidation with the `ETag` the first answer carried,
/// and a path the build does not claim — over one spawned server.
fn revalidated_exchanges(mut command: Command) -> Vec<ServedRequest> {
    let server = ServerUnderTest::spawn(&mut command);
    let port = server.port();
    let artifact = ServedRequest::exchange(port, "GET", "/client.js", "");
    let etag = artifact
        .headers
        .iter()
        .find_map(|line| line.strip_prefix("ETag: "))
        .unwrap_or("\"none\"")
        .to_string();
    let revalidated = ServedRequest::exchange_with(
        port,
        "GET",
        "/client.js",
        &format!("If-None-Match: {etag}\r\n"),
        "",
    );
    let fallback = ServedRequest::exchange(port, "GET", "/other", "");
    vec![artifact, revalidated, fallback]
}

const BUILDER_PROBE: &str = concat!(
    "import std::build::require_build;\n",
    "import std::http::{ CachePolicy, Response, Server };\n",
    "import std::io::print;\n",
    "\n",
    "async fun main() {\n",
    "\tlet build = require_build(\"client\");\n",
    "\tServer::builder()\n",
    "\t\t.port(0)\n",
    "\t\t.serve_build(build)\n",
    "\t\t.cache_build(|url| CachePolicy::validated().cache_control(\"no-cache\"))\n",
    "\t\t.on_request(|request| Response::builder()\n",
    "\t\t\t.set_header(\"Content-Type\", \"text/plain\")\n",
    "\t\t\t.body(i\"fallback {request.path()}\")\n",
    "\t\t\t.build())\n",
    "\t\t.on_start(|server| print(i\"vilan-test-port={server.port()}\"))\n",
    "\t\t.build()\n",
    "\t\t.start();\n",
    "}\n",
);

/// Rule 1's copy at EVERY position that consumes a place read, for the types
/// the JS backend never copies (`str`, `Option`, enums, handles): a `let`, a
/// struct literal's field, a list and a tuple element, an assignment, a `push`,
/// a subscript read out of a `Vec` at a `let` and at a tail, and a FIELD handed
/// to a call while its struct is read again. Each was a use-after-move (or a
/// move out of a `Vec`) natively — eleven rustc errors at the Order 40 seal —
/// and all of them are on `Server::builder()`'s std path.
#[test]
fn a_consumed_place_read_is_copied_at_every_consuming_position() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_consumed.vl"), CONSUMED_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_consumed.vl"),
        Verdict::Identical,
        "a place read into a consuming position is a copy on both backends"
    );
}

const CONSUMED_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::option::Option::{ self, None, Some };\n",
    "\n",
    "struct Asset {\n",
    "\tname: str,\n",
    "\tkind: Option<str>,\n",
    "}\n",
    "\n",
    "fun describe(name: str, kind: Option<str>): str {\n",
    "\tlet shown: str = kind.unwrap_or(\"-\");\n",
    "\tname + \":\" + shown\n",
    "}\n",
    "\n",
    "fun last(path: str): str {\n",
    "\tlet parts = path.split(\"/\");\n",
    "\tparts[parts.len() - 1]\n",
    "}\n",
    "\n",
    "fun kind_of(asset: Asset): Option<str> {\n",
    "\tasset.kind\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet parts = \"a/b/c\".split(\"/\");\n",
    "\tlet first = parts[0];\n",
    "\tlet copy = first;\n",
    "\tprint(i\"{first} {copy} {last(\"x/y\")} {parts[1]}\");\n",
    "\tlet asset = Asset { name = \"logo\", kind = Some(\"svg\") };\n",
    "\tprint(describe(asset.name, asset.kind));\n",
    "\tlet again = asset;\n",
    "\tlet kind: str = kind_of(asset).unwrap_or(\"?\");\n",
    "\tprint(i\"{asset.name} {again.name} {kind}\");\n",
    "\tmut kept: List<(str, i32)> = [];\n",
    "\tfor entry in [(\"x\", 1), (\"y\", 2)] {\n",
    "\t\tlet (address, at) = entry;\n",
    "\t\tkept.push((address, at));\n",
    "\t\tif address == \"y\" {\n",
    "\t\t\tprint(i\"kept {address} at {at}\");\n",
    "\t\t}\n",
    "\t}\n",
    "\tprint(kept.len());\n",
    "\tlet maybe: Option<str> = Some(\"m\");\n",
    "\tlet other = maybe;\n",
    "\tlet built = Asset { name = first, kind = maybe };\n",
    "\tlet listed = [first, copy];\n",
    "\tmut target = \"t\";\n",
    "\ttarget = first;\n",
    "\tlet shown: str = maybe.unwrap_or(\"\");\n",
    "\tlet also: str = other.unwrap_or(\"\");\n",
    "\tprint(i\"{shown} {also} {built.name} {listed.len()} {target} {first}\");\n",
    "}\n",
);

/// A binding that IS a `&mut` loan — a `&mut` parameter, a `&mut self` — is
/// REBORROWED when handed to another `&mut` position (`&mut *loan`), not
/// borrowed again: `&mut loan` is a `&mut &mut T` that rustc takes only from a
/// `mut` binding. `[derive(Wire)]`'s `describe` hands its serializer on this
/// way, which is how `Server::builder()`'s rpc error encoding reached it.
#[test]
fn a_mutable_loan_is_reborrowed_when_it_is_handed_on() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_reborrow.vl"), REBORROW_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_reborrow.vl"),
        Verdict::Identical,
        "a forwarded `&mut` loan writes the caller's value on both backends"
    );
}

const REBORROW_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "struct Counter {\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "impl Counter {\n",
    "\tfun bump(&mut self) {\n",
    "\t\tself.n = self.n + 1;\n",
    "\t}\n",
    "\tfun twice(&mut self) {\n",
    "\t\tself.bump();\n",
    "\t\tself.bump();\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun step(counter: &mut Counter) {\n",
    "\tcounter.bump();\n",
    "}\n",
    "\n",
    "fun forward(counter: &mut Counter) {\n",
    "\tstep(counter);\n",
    "\tcounter.twice();\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut counter = Counter { n = 0 };\n",
    "\tforward(&mut counter);\n",
    "\tprint(counter.n);\n",
    "}\n",
);

/// An unsuffixed literal takes its PARTNER'S width across a binary operator
/// (`unit >= 65` over a `u32` is a `u32` comparison), and a host binding's
/// argument takes its own parameter's type rather than the enclosing
/// position's (`let unit: u32 = "A".code_at(0)` — the index is an `i32`).
/// `std::string`'s `to_lowercase_ascii`, on `Request::header`'s path, is the
/// first shape; seven rustc errors at the Order 40 seal.
#[test]
fn a_literal_operand_takes_its_partners_width() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_partner.vl"), PARTNER_WIDTH_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_partner.vl"),
        Verdict::Identical,
        "a literal beside a `u32` is a `u32` on both backends"
    );
}

const PARTNER_WIDTH_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet unit: u32 = \"A\".code_at(0);\n",
    "\tlet other: u32 = 70;\n",
    "\tlet wide: u53 = 9;\n",
    "\tprint(i\"{unit >= 65 && unit <= 90} {other > 65} {unit == 65} {unit - 65}\");\n",
    // A literal on the LEFT is B389's (solver-41): the analyzer refuses it
    // before either backend sees it, so only right-hand literals are here.
    "\tprint(i\"{wide * 2 > 17} {(wide + 1) == 10}\");\n",
    "}\n",
);

/// A closure TYPE written over a VIEW (`|&mut T| void`, `|&T| U`) takes its
/// argument by reference natively, as the literal landing in it binds it —
/// the analyzer records the `&`/`&mut` the type itself erases
/// (`Program::closure_type_parameter_views`). `SignalCell::update(|&mut list|
/// ..)` is the shape everything reactive writes through (kolt's store, the
/// keyed rpc mirrors, `ListCell`), and it was refused ("an unresolved type")
/// at the Order 40 seal; a user function taking one is here too.
#[test]
fn a_closure_type_over_a_view_takes_its_argument_by_reference() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_views.vl"), VIEW_CLOSURE_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_views.vl"),
        Verdict::Identical,
        "a view-parameter closure writes the caller's value on both backends"
    );
}

const VIEW_CLOSURE_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::map::Map;\n",
    "import std::reactive::{ Signal, SignalCell };\n",
    "\n",
    "struct Counter {\n",
    "\tn: i32,\n",
    "}\n",
    "\n",
    "fun apply(counter: &mut Counter, step: |&mut Counter| void) {\n",
    "\tstep(counter);\n",
    "\tstep(counter);\n",
    "}\n",
    "\n",
    "fun peek(counter: &Counter, read: |&Counter| i32): i32 {\n",
    "\tread(counter)\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tmut counter = Counter { n = 1 };\n",
    "\tapply(&mut counter, |&mut held| {\n",
    "\t\theld.n = held.n * 3;\n",
    "\t});\n",
    "\tprint(peek(&counter, |&held| held.n + 1));\n",
    "\tlet list: SignalCell<List<i32>> = SignalCell::new([]);\n",
    "\tlist.update(|&mut items| {\n",
    "\t\titems.push(4);\n",
    "\t\titems.push(5);\n",
    "\t});\n",
    "\tlet names: SignalCell<Map<str, i32>> = Signal::new(Map::new());\n",
    "\tnames.update(|&mut entries| {\n",
    "\t\tentries.insert(\"a\", 1);\n",
    "\t});\n",
    "\tprint(i\"{counter.n} {list.get().len()} {names.get().len()}\");\n",
    "}\n",
);

/// The one shape a closure-over-a-view cannot take natively, refused BY NAME
/// at compile time: a closure handed `update`'s `&mut` view that reads the
/// SAME cell again inside it. The JS backend answers the in-progress value (the
/// view and the cell are one object); natively the view is a live `RefMut` and
/// the read a second borrow, which safe Rust answers with a panic — so it is
/// named rather than run. `signal-update.vl`'s last section is this shape, and
/// the whole-set differential counts it refused. The control beside it reads a
/// DIFFERENT cell inside the closure and builds, identical on both backends.
#[test]
fn a_reentrant_read_of_an_updated_cell_is_refused_by_name() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_reentrant.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell };\n",
            "\n",
            "fun main() {\n",
            "\tlet todos: SignalCell<List<i32>> = Signal::new([1]);\n",
            "\ttodos.update(|&mut list| {\n",
            "\t\tlist.push(2);\n",
            "\t\tprint(todos.get().len());\n",
            "\t});\n",
            "}\n",
        ),
    )
    .expect("write the refused probe");
    std::fs::write(
        staged.join("native_probe_other_cell.vl"),
        concat!(
            "import std::io::print;\n",
            "import std::reactive::{ Signal, SignalCell };\n",
            "\n",
            "fun main() {\n",
            "\tlet todos: SignalCell<List<i32>> = Signal::new([1]);\n",
            "\tlet other: SignalCell<List<i32>> = Signal::new([7, 8]);\n",
            "\ttodos.update(|&mut list| {\n",
            "\t\tlist.push(other.get().len());\n",
            "\t});\n",
            "\tprint(todos.get());\n",
            "}\n",
        ),
    )
    .expect("write the control");
    match compare(&staged, "native_probe_reentrant.vl") {
        Verdict::Refused(reason) => assert!(
            reason.contains("reads the same place again"),
            "refused, and for this reason: {reason}"
        ),
        other => panic!("a reentrant read of an updated cell must be refused by name: {other:?}"),
    }
    assert_eq!(
        compare(&staged, "native_probe_other_cell.vl"),
        Verdict::Identical,
        "reading a different cell inside the closure is not the refused shape"
    );
}

/// A generic call whose binding the call itself leaves OPEN is closed by the
/// position it fills (B370's law on the generic-call path): `SignalCell::new([])`
/// under a `SignalCell<List<i32>>` annotation, `Signal::new(Map::new())` whose
/// argument's own binding is closed by the parameter the outer call binds, and a
/// variant whose payload leaves a parameter open (`Delta::Remove("k")`,
/// `Delta::Reset([1, 2])`) pushed into a `List<Delta<str, i32>>`. Refused at
/// the Order 40 seal ("an unresolved type").
#[test]
fn a_generic_call_left_open_is_closed_by_its_position() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_open.vl"), OPEN_BINDING_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_open.vl"),
        Verdict::Identical,
        "an open binding closed by its position builds the same value on both backends"
    );
}

const OPEN_BINDING_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::map::Map;\n",
    "import std::reactive::{ Signal, SignalCell };\n",
    "\n",
    "enum Delta<K, T> {\n",
    "\tReset(List<T>),\n",
    "\tRemove(K),\n",
    "}\n",
    "\n",
    "fun count_resets(ops: List<Delta<str, i32>>): i32 {\n",
    "\tmut resets = 0;\n",
    "\tfor op in ops {\n",
    "\t\tmatch op {\n",
    "\t\t\tDelta::Reset(let items) => resets += items.len(),\n",
    "\t\t\tDelta::Remove(let _key) => {},\n",
    "\t\t}\n",
    "\t}\n",
    "\tresets\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet cell: SignalCell<List<i32>> = SignalCell::new([]);\n",
    "\tlet named: SignalCell<Map<str, i32>> = Signal::new(Map::new());\n",
    "\tmut ops: List<Delta<str, i32>> = [];\n",
    "\tops.push(Delta::Reset([1, 2]));\n",
    "\tops.push(Delta::Remove(\"k\"));\n",
    "\tprint(i\"{cell.get().len()} {named.get().len()} {ops.len()} {count_resets(ops)}\");\n",
    "}\n",
);

/// Four lowering gaps kolt's server shape reached, each general: a `const`
/// expression's COMPUTED value in place of its subtree (`const
/// asset::read(..)`, as the JS emitter serializes it); a `str` literal NESTED in
/// a `match` pattern (`Some("api")` over an `Option<str>`, now a binding and a
/// guard); an unannotated function's return type from the analyzer's own
/// inference (`is_fingerprinted`'s bare `true` tail had emitted `-> ()`); and a
/// NESTED closure's destructured parameters kept out of the outer closure's
/// captures (`|(k, _)| k == key` inside `get_post_var`).
#[test]
fn the_kolt_shapes_lowering_gaps_build_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_lowering.vl"), KOLT_LOWERING_PROBE)
        .expect("write the probe program");
    std::fs::write(staged.join("native-head.txt"), "baked at build time")
        .expect("write the asset the probe reads at build time");
    assert_eq!(
        compare(&staged, "native_probe_lowering.vl"),
        Verdict::Identical,
        "the four shapes print the same on both backends"
    );
}

const KOLT_LOWERING_PROBE: &str = concat!(
    "import std::asset;\n",
    "import std::io::print;\n",
    "import std::option::Option::{ self, None, Some };\n",
    "\n",
    "fun is_short(text: str) {\n",
    "\tif text.len() > 3 {\n",
    "\t\tret false;\n",
    "\t}\n",
    "\ttrue\n",
    "}\n",
    "\n",
    "fun route(parts: List<str>): str {\n",
    "\tmatch parts.get(0) {\n",
    "\t\tSome(\"api\") => match parts.get(1) {\n",
    "\t\t\tSome(\"login\") => \"login\",\n",
    "\t\t\tSome(let other) => \"api:\" + other,\n",
    "\t\t\tNone => \"api\",\n",
    "\t\t},\n",
    "\t\tSome(let first) => \"page:\" + first,\n",
    "\t\tNone => \"root\",\n",
    "\t}\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet pairs = [(\"a\", 1), (\"b\", 2)];\n",
    "\tlet find = |key: str| pairs.find(|(k, _)| k == key).map(|(_, v)| v);\n",
    "\tprint(i\"{is_short(\"ab\")} {is_short(\"abcd\")}\");\n",
    "\tprint(route([\"api\", \"login\"]));\n",
    "\tprint(route([\"api\", \"x\"]));\n",
    "\tprint(route([\"home\"]));\n",
    "\tprint(route([]));\n",
    "\tprint(find(\"b\").unwrap_or(0));\n",
    "\tprint(const asset::read(\"native-head.txt\"));\n",
    "}\n",
);

/// **F34**: the two reactive COMBINATORS build natively. `map<U>` is a trait
/// DEFAULT with a generic parameter of its own, reached through a dispatch the
/// analyzer records no value for — `U` is bound from the closure argument
/// (`|n| n * 10` against `|T| U`), and the default is one instance per binding
/// of it as well as per receiver. `flatten` is written with `?` lifts
/// (`inner_subscription.read()?.dispose()`), which lower to a `match` that
/// rebuilds the bad half. Both refused at the Order 40 seal (`parameter 1 of
/// map`; `a ? lift`); `reactive.vl`, `reactive-on-change.vl`,
/// `reactive-flatten.vl` and `iterator-adapters.vl` flip with them, and two of
/// those are in [`DEFAULT_SUITE`].
#[test]
fn the_reactive_combinators_map_and_flatten_build_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_map.vl"), MAP_PROBE).expect("write the map probe");
    std::fs::write(staged.join("native_probe_flatten.vl"), FLATTEN_PROBE)
        .expect("write the flatten probe");
    for program in ["native_probe_map.vl", "native_probe_flatten.vl"] {
        assert_eq!(
            compare(&staged, program),
            Verdict::Identical,
            "{program}: a derived signal must print the same on both backends"
        );
    }
}

/// **F34 on A124 S2b's nodes**: a `.cell()` chain (`map` into a cached cell,
/// mapped again and cached again) and a `.distinct()` node that passes a change
/// on only when the value differs — its subscriber counts the changes that got
/// through — build natively and print the same as node.
#[test]
fn a_cell_chain_and_a_distinct_node_build_the_same_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_cell.vl"), CELL_CHAIN_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_cell.vl"),
        Verdict::Identical,
        "a `.cell()` chain and a `.distinct()` must print the same on both backends"
    );
}

const CELL_CHAIN_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Signal, SignalCell, Source };\n",
    "\n",
    "fun main() {\n",
    "\tlet count = SignalCell::new(1);\n",
    "\tlet scaled = count.map(|n| n * 10).cell();\n",
    "\tlet labelled = scaled.map(|n| i\"#{n}\").cell();\n",
    "\tlet parity = count.map(|n| n % 2).distinct();\n",
    "\tmut changes = 0;\n",
    "\tlet _watch = parity.sub(|value| {\n",
    "\t\tchanges += 1;\n",
    "\t});\n",
    "\tcount.set(3);\n",
    "\tcount.set(4);\n",
    "\tcount.set(6);\n",
    "\tlet now: i32 = scaled.get();\n",
    "\tlet label: str = labelled.get();\n",
    "\tlet odd: i32 = parity.get();\n",
    "\tprint(i\"{now} {label} {odd} {changes}\");\n",
    "}\n",
);

const MAP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Signal, SignalCell, Source };\n",
    "\n",
    "fun main() {\n",
    "\tlet count = SignalCell::new(1);\n",
    "\tlet scaled = count.map(|n| n * 10);\n",
    "\tlet labelled = scaled.map(|n| i\"#{n}\");\n",
    "\tlet halves = count.map(|n| n.as_f64() / 2.0);\n",
    "\tcount.set(4);\n",
    "\tlet now: i32 = scaled.get();\n",
    "\tlet label: str = labelled.get();\n",
    "\tlet half: f64 = halves.get();\n",
    "\tprint(i\"{now} {label} {half}\");\n",
    "\tcount.set(5);\n",
    "\tlet later: i32 = scaled.get();\n",
    "\tprint(later);\n",
    "}\n",
);

const FLATTEN_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::reactive::{ Signal, SignalCell, Source };\n",
    "\n",
    "fun main() {\n",
    "\tlet first = SignalCell::new(1);\n",
    "\tlet second = SignalCell::new(2);\n",
    "\tlet chosen = SignalCell::new(first);\n",
    "\tlet joined = chosen.flatten();\n",
    "\tfirst.set(10);\n",
    "\tchosen.set(second);\n",
    "\tsecond.set(20);\n",
    "\tprint(joined.get());\n",
    "}\n",
);

/// Builds `program` (already staged) natively and for node, and answers the
/// two commands that START each leg's server: the native binary and `node
/// <program>.mjs`, both run from the staging directory so a relative `dist/`
/// and a `const asset::read` beside the program resolve the same way.
fn both_servers(staged: &Path, program: &str) -> (Command, Command) {
    let built = vilan(staged)
        .args(["build", "--backend", "rust", program])
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
    let bundled = vilan(staged)
        .args(["build", program])
        .output()
        .expect("build the server for node");
    assert!(
        bundled.status.success(),
        "the JS leg did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
    let mut native = Command::new(staged.join(&binary));
    native.current_dir(staged);
    let mut node = Command::new("node");
    node.current_dir(staged).arg(program.replace(".vl", ".mjs"));
    (native, node)
}

/// Builds a client program for node (clients are always the JS leg: the
/// generated rpc client is a browser-shaped program).
fn build_client(staged: &Path, program: &str) {
    let bundled = vilan(staged)
        .args(["build", program])
        .output()
        .expect("build the client for node");
    assert!(
        bundled.status.success(),
        "the client did not build:\n{}",
        String::from_utf8_lossy(&bundled.stderr)
    );
}

/// Runs a node client to completion under a LIVENESS bound (the clients exit
/// by themselves; the bound only turns a hang into a failure), and answers its
/// stdout.
fn run_client(staged: &Path, program: &str, environment: &[(&str, String)]) -> String {
    let mut command = Command::new("node");
    command
        .current_dir(staged)
        .arg(program.replace(".vl", ".mjs"))
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    for (name, value) in environment {
        command.env(name, value);
    }
    let mut child = command.spawn().expect("spawn the client");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        if child.try_wait().expect("poll the client").is_some() {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            panic!("the client never finished");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let mut out = String::new();
    child
        .stdout
        .take()
        .expect("the client's stdout")
        .read_to_string(&mut out)
        .expect("read the client's stdout");
    out
}

/// **F18 slice 3 — the rpc server natively.** `service_layer.rs`'s keyed pin
/// (A39) with its two halves pulled apart: the SERVER — a `[service]` with an
/// `[expose]` and an `[expose(keyed)]` field, mounted with `Service::new` on
/// `Server::builder()` — is built by each backend in turn, and the CLIENT,
/// always node, connects over the upgrade handover (a real RFC 6455 accept key,
/// computed by `vilan_rt::crypto`), holds a PER-KEY subscription on the keyed
/// mirror, posts, edits, and prints what its mirrors hold and the contract hash
/// it computed.
///
/// The client's whole stdout is compared between the two servers, and the
/// server's own announced contract hash is compared with the client's — the
/// wire is byte-identical to node's exactly when a vilan client cannot tell
/// the two servers apart, and the hash is unmoved exactly when the native
/// server hashes its contract as node does (a moved hash is a client refused
/// as `Contract`, which the verbatim lines below would red on).
///
/// Red at the Order 40 seal: `Server::builder()` refused by name; past it, the
/// rpc path reached `SignalCell::update(|&mut store| ..)` (a closure TYPE's view
/// parameter, which the type erased), `Signal::new(Map::new())` (a binding the
/// position closes), `keyed_diff`'s `ops.push(Delta::Reset(..))` (an open
/// variant parameter), a `self` captured into a stored closure, and a context
/// argument the dispatched `SignalCell::sub` does not take.
#[test]
fn the_keyed_rpc_service_answers_a_node_client_the_same_from_a_native_server() {
    let staged = stage();
    std::fs::write(
        staged.join("native_keyed_server.vl"),
        include_str!("native/keyed_chat_server.vl"),
    )
    .expect("write the server");
    std::fs::write(
        staged.join("native_keyed_client.vl"),
        include_str!("native/keyed_chat_client.vl"),
    )
    .expect("write the client");
    build_client(&staged, "native_keyed_client.vl");
    let (mut native_command, mut node_command) = both_servers(&staged, "native_keyed_server.vl");
    let serve = |command: &mut Command| {
        let server = ServerUnderTest::spawn(command);
        let answered = run_client(
            &staged,
            "native_keyed_client.vl",
            &[("CHAT_PORT", server.port().to_string())],
        );
        let contract = server
            .announcement
            .split_whitespace()
            .find_map(|field| field.strip_prefix("contract="))
            .unwrap_or_default()
            .to_string();
        (answered, contract)
    };
    let (native, native_contract) = serve(&mut native_command);
    let (javascript, node_contract) = serve(&mut node_command);
    assert_eq!(
        native, javascript,
        "a node client must see the same wire from both servers"
    );
    assert_eq!(
        native_contract, node_contract,
        "the contract hash is unmoved"
    );
    for line in [
        "post:2",
        "edit:true",
        "m2:world again",
        "held:m2=world again ",
        "topic-held:general",
        "fault:false",
    ] {
        assert!(
            native.lines().any(|answered| answered == line),
            "the client's `{line}` over the native server:\n{native}"
        );
    }
    assert!(
        native
            .lines()
            .any(|line| line == format!("hash:{native_contract}")),
        "the client's contract hash is the server's own ({native_contract}):\n{native}"
    );
}

/// **F18 slice 3's EXIT** — a program with the SHAPE of kolt's server leg,
/// `Server::builder()` as kolt writes it, built natively: an auth door
/// (`/api/register`, `/api/login`) decoding kolt's `List<List<str>>` POST body
/// through `parse_path` as kolt writes it and checking a hashed password in
/// SQLite; a per-connection rpc store (`Service::factory`) behind a handshake
/// gate (`authorize`) that looks the session token up in the same database; the
/// build served from its own description with kolt's fingerprint-aware
/// `cache_build`; and the `Document` shell (`const asset::read` in its head) for
/// every other path. The program lives in `native/kolt_shape_server.vl` and is
/// written from scratch — kolt is never copied into this tree.
///
/// Over ONE server: six HTTP exchanges, each byte for byte against node's
/// (status line, the headers the program and std set, the body): a register, a
/// good login (whose answer carries the session token), a wrong password, a
/// malformed call, a deep link answered by the shell, and the client bundle
/// with its validator. Then two node clients over the WebSocket upgrade: one
/// with the token — `whoami` is the identity the handshake settled, a PER-KEY
/// subscription sees its key's writes and no other — and one with a bogus
/// token, refused at the handshake. Both clients' stdout compared whole.
///
/// Non-vacuous by content: the bodies and the client's lines are asserted
/// verbatim, and the two logins differ only in the password.
///
/// **F40: the REAL password path.** The shape hashes as kolt's `store.vl` does —
/// PBKDF2-HMAC-SHA-512 at 100,000 rounds through a node:crypto `pbkdf2Sync` the
/// program binds itself, read back through the `Buffer`'s `toString("hex")`, a
/// salt and a session token from `std::crypto::random_bytes` — so both legs run
/// `vilan-rt-crypto`'s arithmetic against node's. The tokens are random by
/// design and are compared MASKED ([`mask_session_tokens`]); the authorized
/// client carries the real one, so a token the store did not keep is a refused
/// client. PBKDF2's bytes themselves are held to node's in
/// [`the_crypto_surface_answers_nodes_bytes_on_both_backends`]; kolt's own
/// `server.vl`, built natively on a scratch copy, is measured beside the
/// order's census (`sweeps/order42/native-42/`), where one backend's hash is
/// verified by the other over one database file.
#[test]
fn the_kolt_server_shape_serves_a_login_and_a_keyed_subscription_from_a_native_build() {
    let staged = stage();
    for (name, contents) in [
        (
            "native_kolt_server.vl",
            include_str!("native/kolt_shape_server.vl"),
        ),
        (
            "native_kolt_client.vl",
            include_str!("native/kolt_shape_client.vl"),
        ),
        ("server-head.html", include_str!("native/server-head.html")),
    ] {
        std::fs::write(staged.join(name), contents).expect("stage the exit program");
    }
    let dist = staged.join("dist");
    std::fs::create_dir_all(&dist).expect("create the build directory");
    std::fs::write(dist.join("client.js"), "console.log(\"client\");\n")
        .expect("write the artifact");
    std::fs::write(
        dist.join("client.chunks.json"),
        "{\"leg\":\"client\",\"entry\":\"client.js\",\"styles\":null,\
         \"classic_script\":false,\"chunks\":[],\"assets\":[]}",
    )
    .expect("write the build manifest");
    build_client(&staged, "native_kolt_client.vl");
    let (mut native_command, mut node_command) = both_servers(&staged, "native_kolt_server.vl");

    let serve = |command: &mut Command| {
        let server = ServerUnderTest::spawn(command);
        let port = server.port();
        let credentials = "[[\"username\",\"ada\"],[\"password\",\"lovelace1\"]]";
        let exchanges = vec![
            ServedRequest::exchange(port, "POST", "/api/register", credentials),
            ServedRequest::exchange(port, "POST", "/api/login", credentials),
            ServedRequest::exchange(
                port,
                "POST",
                "/api/login",
                "[[\"username\",\"ada\"],[\"password\",\"wrong-one\"]]",
            ),
            ServedRequest::exchange(port, "POST", "/api/login", "[[\"username\",\"ada\"]]"),
            ServedRequest::exchange(port, "GET", "/some/deep/link", ""),
            ServedRequest::exchange(port, "GET", "/client.js", ""),
        ];
        let token = exchanges[1]
            .body
            .split("\"token\":\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_default()
            .to_string();
        let authorized = run_client(
            &staged,
            "native_kolt_client.vl",
            &[("KOLT_PORT", port.to_string()), ("KOLT_TOKEN", token)],
        );
        let refused = run_client(
            &staged,
            "native_kolt_client.vl",
            &[
                ("KOLT_PORT", port.to_string()),
                ("KOLT_TOKEN", "bogus".to_string()),
            ],
        );
        (exchanges, authorized, refused)
    };
    let mut native = serve(&mut native_command);
    let mut javascript = serve(&mut node_command);
    // F40: the salt and the session token are `random_bytes(32).to_hex()`
    // on both legs, as in kolt, so each body is compared with its tokens
    // MASKED — and the tokens themselves are held to what random ones must
    // be: 64 lowercase hex digits, fresh per session, never the other leg's.
    let native_tokens = mask_session_tokens(&mut native.0);
    let node_tokens = mask_session_tokens(&mut javascript.0);
    assert_eq!(
        native.0, javascript.0,
        "the six HTTP exchanges must be the same from both servers"
    );
    for tokens in [&native_tokens, &node_tokens] {
        assert_eq!(tokens.len(), 2, "register and login each open a session");
        assert_ne!(tokens[0], tokens[1], "a session token is fresh per session");
    }
    assert!(
        native_tokens
            .iter()
            .all(|token| !node_tokens.contains(token)),
        "two processes never draw the same token: {native_tokens:?} {node_tokens:?}"
    );
    assert_eq!(
        native.1, javascript.1,
        "the authorized client must see the same wire from both servers"
    );
    assert_eq!(
        native.2, javascript.2,
        "the refused client must be refused the same way by both servers"
    );

    let bodies: Vec<&str> = native
        .0
        .iter()
        .map(|exchange| exchange.body.as_str())
        .collect();
    assert_eq!(
        bodies[..4],
        [
            "{\"ok\":true,\"token\":\"<session token>\",\"message\":\"welcome ada\"}",
            "{\"ok\":true,\"token\":\"<session token>\",\"message\":\"welcome ada\"}",
            "{\"ok\":false,\"token\":\"\",\"message\":\"wrong password\"}",
            "malformed call",
        ]
    );
    assert_eq!(native.0[3].status, "HTTP/1.1 400 Bad Request");
    assert!(
        bodies[4].contains("<title>Kolt</title>")
            && bodies[4].contains("<meta name=\"shape\" content=\"kolt\">"),
        "the shell, with the head `const asset::read` baked in:\n{}",
        bodies[4]
    );
    assert_eq!(bodies[5], "console.log(\"client\");\n");
    assert!(
        native.0[5]
            .headers
            .iter()
            .any(|line| line == "Cache-Control: no-cache"),
        "an unfingerprinted artifact is validated, not immutable: {:?}",
        native.0[5].headers
    );
    for line in [
        "who:ada",
        "post:1",
        "m2:ada:world",
        "m2:ada:world again",
        "fault:false",
    ] {
        assert!(
            native.1.lines().any(|answered| answered == line),
            "the authorized client's `{line}` over the native server:\n{}",
            native.1
        );
    }
    assert!(
        !native.1.contains("m1"),
        "a per-key subscription sees ITS key and no other:\n{}",
        native.1
    );
    assert_eq!(native.2.trim(), "err:Unauthorized");
}

/// Replaces every session token in `exchanges`' bodies — a run of exactly 64
/// lowercase hex digits, which is `random_bytes(32).to_hex()` — with a
/// placeholder, and answers the tokens in order. Anything that LOOKS like a
/// token but is not 64 lowercase hex stays in the body and fails the compare.
fn mask_session_tokens(exchanges: &mut [ServedRequest]) -> Vec<String> {
    let mut tokens = Vec::new();
    for exchange in exchanges {
        let mut masked = String::new();
        let mut rest = exchange.body.as_str();
        while let Some(start) = rest.find("\"token\":\"") {
            let (before, after) = rest.split_at(start + "\"token\":\"".len());
            masked.push_str(before);
            let end = after.find('"').unwrap_or(after.len());
            let token = &after[..end];
            if token.len() == 64
                && token
                    .bytes()
                    .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
            {
                tokens.push(token.to_string());
                masked.push_str("<session token>");
            } else {
                masked.push_str(token);
            }
            rest = &after[end..];
        }
        masked.push_str(rest);
        exchange.body = masked;
    }
    tokens
}

/// The three exchanges the exit drives, over one spawned server.
struct ServedLogin {
    exchanges: Vec<ServedRequest>,
}

impl ServedLogin {
    fn take(mut command: Command) -> ServedLogin {
        let server = ServerUnderTest::spawn(&mut command);
        let port = server.port();
        let exchanges = [
            ("POST", "/api/login", "[\"ada\",\"lovelace1\"]"),
            ("POST", "/api/login", "[\"ada\",\"wrong\"]"),
            ("GET", "/", ""),
        ]
        .into_iter()
        .map(|(method, path, body)| ServedRequest::exchange(port, method, path, body))
        .collect();
        ServedLogin { exchanges }
    }
}

/// One request answered by a spawned server, and the pieces of the answer the
/// two backends can be held to.
#[derive(Debug, PartialEq, Eq)]
struct ServedRequest {
    status: String,
    headers: Vec<String>,
    body: String,
    announced_line: String,
}

impl ServedRequest {
    /// One request to an ALREADY-RUNNING server, so a test can drive several
    /// over one process. The body carries a `Content-Length`, which is the
    /// only framing `vilan_rt::http` accepts on the way in (a chunked request
    /// is refused with `411`, by design).
    ///
    /// `announced_line` is empty here: it belongs to the server, and a caller
    /// driving several exchanges has it from the spawn.
    fn exchange(port: u16, method: &str, path: &str, body: &str) -> ServedRequest {
        ServedRequest::exchange_with(port, method, path, "", body)
    }

    /// [`ServedRequest::exchange`] with extra request header lines (each ending
    /// `\r\n`) — how a revalidation sends its `If-None-Match`.
    fn exchange_with(
        port: u16,
        method: &str,
        path: &str,
        extra_headers: &str,
        body: &str,
    ) -> ServedRequest {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port))
            .expect("connect to the port the server announced");
        stream
            .write_all(
                format!(
                    "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n\
                     {extra_headers}Connection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
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
            // node adds a `Date` of its own and the two backends order
            // `Connection`/`Content-Length` differently; both are dropped here
            // so the two legs can be compared whole. See the exit's own
            // comment for why that is written down rather than normalised
            // away silently.
            headers: lines
                .filter(|line| !line.starts_with("Date:") && !line.starts_with("Connection:"))
                .map(str::to_string)
                .collect(),
            body: body.to_string(),
            announced_line: String::new(),
        }
    }

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
/// **F32 (RULED (b), Order 40)**: `BigInt` is an `i128` natively, and the limit
/// is enforced at BOTH ends.
///
/// The two corpus programs that hold the inside of the range are in
/// [`DEFAULT_SUITE`]; this pin is the two edges, which no corpus program can
/// carry because each one fails on purpose. A literal past `i128` is refused at
/// COMPILE time naming the value and the range (and the JS backend builds the
/// same program, which is what makes the refusal a backend limit rather than a
/// language one). An operation that leaves the range TRAPS at run time with the
/// same sentence, where the JS backend — arbitrary precision — simply answers
/// the bigger number; the pin reads both, so a native build that wrapped to a
/// negative would red.
#[test]
fn a_bigint_past_the_native_limit_is_refused_and_an_overflow_traps() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_bigint.vl"), BIGINT_LIMIT_PROBE)
        .expect("write the probe");
    let refused = vilan(&staged)
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_bigint.vl",
        ])
        .output()
        .expect("build the literal probe natively");
    assert!(!refused.status.success(), "the literal must be refused");
    let message = String::from_utf8_lossy(&refused.stderr);
    assert!(
        message.contains("is outside the native backend's range"),
        "the refusal names the rule: {message}"
    );
    assert!(
        message.contains("170141183460469231731687303715884105728"),
        "and the value it refused: {message}"
    );
    // The same program on the JS backend, where a `BigInt` really is arbitrary
    // precision — so this is a BACKEND limit and the message is honest.
    let javascript = vilan(&staged)
        .args(["run", "native_probe_bigint.vl"])
        .output()
        .expect("run the literal probe on the JS backend");
    assert!(javascript.status.success(), "the JS backend builds it");
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "170141183460469231731687303715884105728n\n"
    );

    std::fs::write(
        staged.join("native_probe_bigint_trap.vl"),
        BIGINT_TRAP_PROBE,
    )
    .expect("write the trap probe");
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_bigint_trap.vl"])
        .output()
        .expect("run the trap probe natively");
    assert!(!native.status.success(), "the overflow ends the program");
    assert!(
        String::from_utf8_lossy(&native.stderr).contains("left the native backend's range"),
        "the trap names the rule: {}",
        String::from_utf8_lossy(&native.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "170141183460469231731687303715884105727n\n",
        "the value INSIDE the range printed first, with node's `n`"
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_bigint_trap.vl"])
        .output()
        .expect("run the trap probe on the JS backend");
    assert!(
        javascript.status.success(),
        "arbitrary precision does not trap"
    );
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "170141183460469231731687303715884105727n\n\
         170141183460469231731687303715884105728n\n"
    );
}

/// N120: a program whose arithmetic rustc can see overflowing at compile time
/// is refused by the native build — `deny(arithmetic_overflow)` — and the CLI
/// says so as the PROGRAM's overflow, naming the expression rustc underlined,
/// rather than accusing the backend. A conforming program does not overflow
/// (spec §7.2a; I5's ruling 2), and the JS backend runs on past one: the second
/// half of this pin is that very program printing on the JS backend.
///
/// Red before N120: the refusal ended in "`cargo build` refused the emitted
/// Rust. That is a BACKEND defect, not a defect in the vilan program".
#[test]
fn a_constant_overflow_is_reported_as_the_programs_not_the_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_overflow.vl"), OVERFLOW_PROBE)
        .expect("write the probe");
    let native = vilan(&staged)
        .args(["build", "--backend", "rust", "native_probe_overflow.vl"])
        .output()
        .expect("build the overflow probe natively");
    assert!(
        !native.status.success(),
        "rustc refuses the constant overflow"
    );
    let message = String::from_utf8_lossy(&native.stderr);
    assert!(
        message.contains("the PROGRAM overflows: rustc evaluated `((a_")
            // the emitted path is `src/main.rs` on unix and `src\main.rs` on windows
            && message.contains("+ (1i32)))` (the emitted Rust, src")
            && message.contains("main.rs:")
            && message.contains("attempt to compute `i32::MAX + 1_i32`, which would overflow")
            && message.contains("the JavaScript backend would have run on past it"),
        "the refusal names the program's overflow and the expression: {message}"
    );
    assert!(
        !message.contains("BACKEND defect"),
        "an overflow the program wrote is not a backend defect: {message}"
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_overflow.vl"])
        .output()
        .expect("run the overflow probe on the JS backend");
    assert!(
        javascript.status.success(),
        "the JS backend runs on past it"
    );
    assert_eq!(String::from_utf8_lossy(&javascript.stdout), "2147483648\n");
}

const OVERFLOW_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet a: i32 = 2147483647;\n",
    "\tprint(a + 1);\n",
    "}\n",
);

const BIGINT_LIMIT_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tprint(170141183460469231731687303715884105728n);\n",
    "}\n",
);

const BIGINT_TRAP_PROBE: &str = concat!(
    "import std::io::print;\n",
    "\n",
    "fun main() {\n",
    "\tlet near = 170141183460469231731687303715884105727n;\n",
    "\tprint(near);\n",
    "\tprint(near + 1n);\n",
    "}\n",
);

/// **F18 slice 3's two seams**: node:crypto's SHA-1 behind `std::rpc_server`'s
/// `ws_accept_key`, and `std::time`'s host clock — the two host bindings, beside
/// F33's `Bytes`, that stood between `Server::builder()` and the native backend.
///
/// The accept key is RFC 6455 §1.3's own example (the client key
/// `dGhlIHNhbXBsZSBub25jZQ==` is answered `s3pPLMBiTxaQ9kYGzzhZRbK+xOo=`),
/// asserted verbatim as well as compared, so a native SHA-1 or base64 that
/// agreed with nothing would still red; the empty key is the second vector
/// because its digest is the one whose base64 ends in a single `=`. The clock
/// is compared only in what two processes a moment apart CAN agree on — it is
/// whole milliseconds since 1970 (the unit and the epoch, the two things a
/// wrong conversion gets wrong) and `now()` is not before it.
///
/// Red at the Order 40 seal (refused by name: `digest`, `now_millis`).
#[test]
fn the_websocket_accept_key_and_the_host_clock_agree_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_seams.vl"), SEAMS_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_seams.vl"),
        Verdict::Identical,
        "the accept key and the clock must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_seams.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\nKfh9QIsMVZcl6xEPYxPHzW8SZ8w=\ntrue true\ntrue\n",
        "RFC 6455's own accept key, and a clock in whole milliseconds since 1970"
    );
}

const SEAMS_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::rpc_server::ws_accept_key;\n",
    "import std::time::{ now, now_millis };\n",
    "\n",
    "fun main() {\n",
    "\tprint(ws_accept_key(\"dGhlIHNhbXBsZSBub25jZQ==\"));\n",
    "\tprint(ws_accept_key(\"\"));\n",
    "\tlet sampled = now_millis();\n",
    "\tprint(i\"{sampled > 1790000000000.0} {sampled == sampled.floor()}\");\n",
    "\tlet later = now();\n",
    "\tprint(i\"{later.millis >= sampled.as_i53()}\");\n",
    "}\n",
);

/// **F40 (RULED (a))**: `std::crypto`'s OS randomness, SHA-384/512, HMAC and
/// PBKDF2 natively, through the separate `vilan-rt-crypto` crate — and
/// node:crypto's `pbkdf2Sync` bound by the PROGRAM, with the `Buffer` it answers
/// declared as an `external struct` of the program's own naming and read back
/// through `toString(encoding)`, which is how kolt's `store.vl` hashes a
/// password.
///
/// Every deterministic line is compared byte for byte against node AND held
/// verbatim (each value is node's own answer), so a digest, an HMAC or a PBKDF2
/// that is wrong in any bit reds here; the random lines print only what two
/// processes can agree on — the lengths, that two draws differ, the UUID's
/// version nibble. The last line is `Shared::identity`'s stamp, which counts
/// from 1 in first-ask order on both backends (the native `i64` address it had
/// been did not even compile against std's `i32` declaration, which is the wall
/// kolt's server met past its host gaps).
///
/// And the reach is RECORDED: this program's manifest names `vilan-rt-crypto`,
/// and a program that reaches no crypto names neither optional crate.
///
/// Red before F40: refused by name (`random_bytes`, `sha384`, `sha512`,
/// `hmac_sha512`, `pbkdf2_sha512`, `pbkdf2_sync`, `to_string_encoded`).
#[test]
fn the_crypto_surface_answers_nodes_bytes_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_crypto.vl"), CRYPTO_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_crypto.vl"),
        Verdict::Identical,
        "the crypto surface must print the same bytes on both backends"
    );
    let native = vilan(&staged)
        .args(["run", "--backend", "rust", "native_probe_crypto.vl"])
        .output()
        .expect("run the native backend");
    assert_eq!(
        String::from_utf8_lossy(&native.stdout),
        concat!(
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\n",
            "cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7\n",
            "164b7a7bfcf819e2e395fbe73b56e0a387bd64222e831fd610270cd7ea2505549758bf75c05a994a6d034f65f8f0e6fdcaeab1a34d4a6b4b636e070a38bce737\n",
            "e1d9c16aa681708a45f5c7c4e215ceb66e011a2e9f0040713f18aefdb866d53cf76cab2868a39b9f7840edce4fef5a82be67335c77a6068e04112754f27ccf4e\n",
            "9c549ce63c45f8df93229c0fac3d6457dc31b241409e21ef1b4e45c97c11001333ddb86821b04cb42fdfa9e3cb9996f4cee97ff6a7e62be799a5b23a83fc5a7f\n",
            "6mwBTcctb4zNHtkqzh1B8NjeiVc=\n",
            "rk0Mla9rRtMtCt_5KPBt0CowP47zwlHf1uLYWpVHTEM\n",
            "32 64 false\n",
            "36 4\n",
            "1 2 1\n",
        ),
        "node's own answers, and what two processes can agree on about randomness"
    );
    let manifest_of = |program: &str| {
        std::fs::read_to_string(
            staged
                .join("dist")
                .join("native")
                .join(program)
                .join("Cargo.toml"),
        )
        .expect("read the generated manifest")
    };
    let manifest = manifest_of("native_probe_crypto");
    assert!(
        manifest.contains("vilan-rt-crypto") && !manifest.contains("vilan-rt-sqlite"),
        "a program reaching `std::crypto`'s randomness names the crypto crate, and only it:\n\
         {manifest}"
    );
    let plain = vilan(&staged)
        .args(["build", "--backend", "rust", "bool.vl"])
        .output()
        .expect("build a program that reaches no optional crate");
    assert!(plain.status.success());
    let manifest = manifest_of("bool");
    assert!(
        !manifest.contains("vilan-rt-crypto") && !manifest.contains("vilan-rt-sqlite"),
        "a program that reaches neither names neither:\n{manifest}"
    );
}

const CRYPTO_PROBE: &str = concat!(
    "import std::bytes::encode_utf8;\n",
    "import std::crypto::{ hmac_sha512, pbkdf2_sha512, random_bytes, random_uuid, sha384, sha512 };\n",
    "import std::io::print;\n",
    "import std::shared::Shared;\n",
    "\n",
    "external struct HashBuffer;\n",
    "\n",
    "impl HashBuffer {\n",
    "\t[extern(method, \"toString\")]\n",
    "\texternal fun to_string_encoded(self, encoding: str): str;\n",
    "}\n",
    "\n",
    "[extern(\"node:crypto\", \"pbkdf2Sync\")]\n",
    "external fun pbkdf2_sync(password: str, salt: str, iterations: i32, key_length: i32, digest: str): HashBuffer;\n",
    "\n",
    "async fun main() {\n",
    "\tprint(sha512(encode_utf8(\"abc\")).to_hex());\n",
    "\tprint(sha384(encode_utf8(\"abc\")).to_hex());\n",
    "\tprint(hmac_sha512(encode_utf8(\"Jefe\"), encode_utf8(\"what do ya want for nothing?\")).to_hex());\n",
    "\tprint(pbkdf2_sha512(encode_utf8(\"password\"), encode_utf8(\"salt\"), 2, 512).to_hex());\n",
    "\tlet derived = pbkdf2_sync(\"lovelace1\", \"0123456789abcdef\", 100000, 64, \"sha512\");\n",
    "\tprint(derived.to_string_encoded(\"hex\"));\n",
    "\tprint(pbkdf2_sync(\"password\", \"salt\", 2, 20, \"sha1\").to_string_encoded(\"base64\"));\n",
    "\tprint(pbkdf2_sync(\"password\", \"salt\", 2, 32, \"SHA256\").to_string_encoded(\"base64url\"));\n",
    "\tlet first = random_bytes(32);\n",
    "\tlet second = random_bytes(32);\n",
    "\tprint(i\"{first.len()} {first.to_hex().len()} {first.to_hex() == second.to_hex()}\");\n",
    "\tlet uuid = random_uuid();\n",
    "\tprint(i\"{uuid.len()} {uuid.substring(14, 15)}\");\n",
    "\tlet cell = Shared::new(1);\n",
    "\tlet other = Shared::new(2);\n",
    "\tlet same = cell;\n",
    "\tprint(i\"{cell.identity()} {other.identity()} {same.identity()}\");\n",
    "}\n",
);

/// **F18 slice 2**: a closure declared SYNCHRONOUS, answering nothing, whose
/// body awaits.
///
/// node drops the promise such a callback returns — the call site does not
/// wait, and the body finishes later — so the native backend SPAWNS the body
/// and the closure answers `()`. `std::http`'s `upgrade_handler` is the shape
/// this was built for (`|NodeRequest, NodeSocket, Bytes| void`, with A40's
/// `authorize` hook awaiting inside it).
///
/// The pin is the ORDER, which is the whole claim: `before`, `after`, then the
/// handler's line, because the call returns before the awaited body resumes.
/// A backend that simply ran the body to completion at the call would print
/// them in a different order and still "work".
///
/// Non-vacuous by its neighbour: `adapt.vl`, whose closures answer a VALUE,
/// stays refused by name in [`every_async_corpus_program_is_identical_or_named`]
/// — and the first spelling of the void test floated those three too and was
/// caught there by `expected i32, found ()`.
#[test]
fn a_void_closure_whose_body_awaits_floats_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_float.vl"), FLOAT_PROBE).expect("write the probe");
    assert_eq!(
        compare(&staged, "native_probe_float.vl"),
        Verdict::Identical
    );
    let javascript = vilan(&staged)
        .args(["run", "native_probe_float.vl"])
        .output()
        .expect("run the JS backend");
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        "before\nafter\nhandled 7\n",
        "the call returns BEFORE the awaited body resumes"
    );
}

const FLOAT_PROBE: &str = concat!(
    "import std::io::print;\n",
    "import std::time::sleep;\n",
    "\n",
    "struct Sink {\n",
    "\ton_event: |i32| void,\n",
    "}\n",
    "\n",
    "fun main() {\n",
    "\tlet sink = Sink {\n",
    "\t\ton_event = |value| {\n",
    "\t\t\tsleep(1);\n",
    "\t\t\tprint(i\"handled {value}\");\n",
    "\t\t},\n",
    "\t};\n",
    "\tprint(\"before\");\n",
    "\t(sink.on_event)(7);\n",
    "\tprint(\"after\");\n",
    "}\n",
);

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

/// A124 R3: the two object shapes the native backend does not lower are
/// refused BY NAME, and the JS backend builds both — so each refusal is the
/// native answer, not a broken probe. A `&mut self` slot would write through a
/// counted pointer every copy of the object shares (the JS backend copies the
/// pair instead); an async member's slot would answer a future no object slot
/// is built to carry.
#[test]
fn an_object_the_native_backend_cannot_lower_is_refused_by_name() {
    let staged = stage();
    for (file, source, needle) in [
        (
            "native_probe_dyn_mut.vl",
            DYN_MUT_SELF_PROBE,
            "`Counter::bump` through a `dyn` object (a `&mut self` slot",
        ),
        (
            "native_probe_dyn_async.vl",
            DYN_ASYNC_PROBE,
            "the async member `Fetch::get` through a `dyn` object",
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

const DYN_MUT_SELF_PROBE: &str = concat!(
    "trait Counter { fun get(self): i32; fun bump(&mut self): void; }\n",
    "struct C { n: i32 }\n",
    "impl C with Counter {\n",
    "\tfun get(self): i32 { self.n }\n",
    "\tfun bump(&mut self): void { self.n = self.n + 1; }\n",
    "}\n",
    "fun main() {\n",
    "\tmut a: dyn Counter = C { n = 1 };\n",
    "\ta.bump();\n",
    "\tprint(a.get());\n",
    "}\n",
);

const DYN_ASYNC_PROBE: &str = concat!(
    "trait Fetch { async fun get(self): str; }\n",
    "struct Local { u: str }\n",
    "impl Local with Fetch { fun get(self): str { \"local\" } }\n",
    "fun show(f: dyn Fetch) { print(f.get()); }\n",
    "fun main() { show(Local { u = \"b\" }); }\n",
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

/// A numeric literal in an ASSIGNMENT takes its width from the position, the
/// way one in a `let` already did (B370's law, on the paths the native emitter
/// had not carried it down).
///
/// `mut i: u53 = 5; i -= 1;` emitted `i - (1i32)` against a `u64` and rustc
/// refused the program — a BACKEND defect, and one the byte gate could not see
/// because no corpus program assigns a literal to a non-default width. Four
/// positions are in here, because each carries the expectation down a
/// different path: a plain binding, a subscript, a field, a counted cell's
/// `write()`, and a module-level binding's own initializer. The subscripts are
/// deliberately literal: an index is an index whatever the assignment expects,
/// and a first fix made `xs[1]` come out `xs[(1u64)]`.
const LITERAL_WIDTH_PROBE: &str = concat!(
    "import std::shared::Shared;\n",
    "\n",
    "struct Counter { n: u53 }\n",
    "\n",
    "mut level: u32 = 10;\n",
    "\n",
    "fun main() {\n",
    "\tmut a: u53 = 5;\n",
    "\ta -= 1;\n",
    "\ta += 2;\n",
    "\ta *= 3;\n",
    "\tmut b: i53 = 9;\n",
    "\tb = b - 1;\n",
    "\tmut c: u32 = 7;\n",
    "\tc /= 2;\n",
    "\tmut d: u8 = 200;\n",
    "\td -= 100;\n",
    "\tmut e: i8 = -5;\n",
    "\te += 3;\n",
    "\tmut f: f64 = 1.5;\n",
    "\tf *= 2;\n",
    "\tmut xs: List<u53> = [5u53, 6u53];\n",
    "\txs[0] -= 1;\n",
    "\txs[1] = xs[1] + 2;\n",
    "\tmut counter = Counter { n = 9 };\n",
    "\tcounter.n -= 4;\n",
    "\tlet cell: Shared<u53> = Shared::new(3u53);\n",
    "\tcell.write() = cell.read() + 1;\n",
    "\tlevel -= 3;\n",
    "\tprint(xs);\n",
    "\tprint(i\"{a} {b} {c} {d} {e} {f} {counter.n} {cell.read()} {level}\");\n",
    "}\n",
);

#[test]
fn a_literal_assigned_to_a_narrow_binding_takes_the_bindings_width() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_width.vl"), LITERAL_WIDTH_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_width.vl"),
        Verdict::Identical,
        "a literal assigned into a non-default width must take that width — a mismatch is a \
         rustc refusal of the emitted Rust, which is a backend defect"
    );
}

/// B389's five literal positions, natively: an unsuffixed literal takes its
/// context's width where the POSITION states none the emitter can read — the
/// left operand of a comparison and of an arithmetic operator, a `match` arm,
/// a generic call's argument, a list literal's elements, and a bare `let`
/// typed by a later use (through a comparison peer and a binding built from
/// it). The JS backend has one number and never asks; the Rust one wrote
/// `0i32 < n_u64` and `identity((5i32))` for a `u64` instance until the
/// solver recorded each literal's settled width.
const LITERAL_POSITIONS_PROBE: &str = concat!(
    "fun take(count: u53): u53 { count }\n",
    "fun half(value: f64): f64 { value / 2 }\n",
    "fun identity<T>(value: T): T { value }\n",
    "\n",
    "fun main() {\n",
    "\tlet n: u53 = 4;\n",
    "\tprint(take(1 + n));\n",
    "\tif 0 < n { print(\"positive\"); }\n",
    "\tlet m = 10 - n;\n",
    "\tprint(take(m));\n",
    "\tlet xs: List<u32> = [0, 1, 2];\n",
    "\tprint(xs[2]);\n",
    "\tmatch n {\n",
    "\t\t4 => print(\"four\"),\n",
    "\t\t_ => print(\"other\"),\n",
    "\t}\n",
    "\tlet k: i53 = identity(5);\n",
    "\tprint(k);\n",
    "\tlet bare = 7;\n",
    "\tprint(take(bare));\n",
    "\tmut i = 0;\n",
    "\tlet limit: u53 = 3;\n",
    "\tfor i < limit { i += 1; }\n",
    "\tprint(take(i));\n",
    "\tlet one = 1;\n",
    "\tlet halved = one / 2;\n",
    "\tprint(half(one));\n",
    "\tprint(halved);\n",
    "\tlet x: f64 = 3;\n",
    "\tprint(1 / x);\n",
    "\tprint(3 / 2.0);\n",
    "\tlet quarter = 1 / 4.0;\n",
    "\tprint(quarter);\n",
    "}\n",
);

#[test]
fn the_five_literal_positions_take_their_contexts_width_natively() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_literal_positions.vl"),
        LITERAL_POSITIONS_PROBE,
    )
    .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_literal_positions.vl"),
        Verdict::Identical,
        "a literal must take its context's width in every position B389 names — a mismatch is \
         a rustc refusal of the emitted Rust, which is a backend defect"
    );
}

/// B397: `combine` over a source whose value is itself a TUPLE. The JS
/// backend read it wrong (`x=1,2 y=c l=undefined`) until the comprehension was
/// emitted unrolled for that instance; the native one refuses a mapped tuple by
/// name today. The claim held here is the differential's own: whatever the
/// native backend does with this program, it is never a DIFFERENT answer — a
/// refusal now, the same bytes once it lowers comprehensions.
const COMBINE_TUPLE_ELEMENT_PROBE: &str = concat!(
    "import std::reactive::{ SignalCell, combine };\n",
    "\n",
    "fun main() {\n",
    "\tlet point = SignalCell::new((1, 2));\n",
    "\tlet label = SignalCell::new(\"c\");\n",
    "\tlet both = combine((point, label));\n",
    "\tlet ((x, y), l) = both.get();\n",
    "\tprint(i\"{x} {y} {l}\");\n",
    "\tpoint.set((3, 4));\n",
    "\tlet ((x2, y2), l2) = both.get();\n",
    "\tprint(i\"{x2} {y2} {l2}\");\n",
    "}\n",
);

#[test]
fn combine_over_a_tuple_valued_source_is_never_a_different_answer_natively() {
    let staged = stage();
    std::fs::write(
        staged.join("native_probe_combine_tuple.vl"),
        COMBINE_TUPLE_ELEMENT_PROBE,
    )
    .expect("write the probe program");
    let verdict = compare(&staged, "native_probe_combine_tuple.vl");
    assert!(
        !matches!(verdict, Verdict::Broken(_)),
        "the native backend must refuse this program by name or print what node prints: \
         {verdict:?}"
    );
}

/// F31's trap, in one program: the read whose binding was declared OUTSIDE the
/// loop keeps its copy, and the read whose binding the loop body itself
/// declares moves.
///
/// "No later read" alone gets the first one wrong — `weigh(names)` IS the last
/// read of `names` in the text, and moving there empties the binding the second
/// iteration reads. Nothing reads `names` after the loop, deliberately: a read
/// after it would make the loop read not-last for the trivial reason and the
/// pin would measure nothing. The rule asks "deeper in a repeating region than
/// the DECLARATION", which answers both halves with one question, and the two
/// are in one program so a fix that satisfies either alone fails here.
const LOOP_TRAP_PROBE: &str = concat!(
    "struct Row { id: i32, tags: List<str> }\n",
    "\n",
    "fun weigh(tags: List<str>): i32 { tags.len() }\n",
    "fun weigh_row(row: Row): i32 { row.tags.len() + row.id }\n",
    "\n",
    "fun main() {\n",
    "\tlet names = [\"alpha\", \"beta\"];\n",
    "\tmut total = 0;\n",
    "\tmut i = 0;\n",
    "\tfor i < 3 {\n",
    "\t\ttotal = total + weigh(names);\n",
    "\t\tlet row = Row { id = i, tags = [\"one\"] };\n",
    "\t\ttotal = total + weigh_row(row);\n",
    "\t\ti = i + 1;\n",
    "\t}\n",
    "\tprint(total);\n",
    "}\n",
);

#[test]
fn a_last_use_inside_a_loop_keeps_its_copy_and_one_declared_inside_it_moves() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_loop_trap.vl"), LOOP_TRAP_PROBE)
        .expect("write the probe program");
    assert_eq!(
        copy_census_of(&staged, "native_probe_loop_trap.vl"),
        (1, 1),
        "the read of a binding declared OUTSIDE the loop must copy, and the read of one the \
         loop body declares must move"
    );
    assert_eq!(
        compare(&staged, "native_probe_loop_trap.vl"),
        Verdict::Identical,
        "and the program still prints the same bytes on both backends"
    );
}

/// F31's census: the consumed place reads the native emitter COPIES, and the
/// ones it moves because the read is its binding's last use.
///
/// The committed table, beside `copy-elision-census.tsv` and for the same
/// reason its own head gives: the byte gate above already holds every one of
/// these programs identical on both backends, so a change in elision cannot
/// ship unnoticed — but it arrives as an unlabelled behaviour, and elision is
/// exactly where the gate and the meaning come apart. A program that loses a
/// copy is a win; a program that GAINS one is a regression in the liveness
/// walk, and byte-identical output says the same thing about both.
///
/// **What is counted** is not every `.clone()` in the emitted Rust — a refcount
/// bump on a handle is one of those and is not a copy. It is the copies
/// `copy_a_consumed_place_read` decides: the consumed positions rule 1's own
/// marking never reached (a closure call's arguments, a variant constructor's,
/// a destructure's), which are exactly the ones the native liveness pass is
/// answerable for.
///
/// **The programs** are [`DEFAULT_SUITE`] plus the paper's board probe, because
/// that is the set the byte gate runs on every build; the whole corpus is one
/// list away and costs an emit per program.
const NATIVE_COPY_CENSUS: &str = "crates/vilan-cli/tests/native-copy-census.tsv";

/// One program's census line, as the compiler reports it under
/// `VILAN_NATIVE_REPORT_COPIES=1`.
fn copy_census_of(staged: &Path, program: &str) -> (usize, usize) {
    let output = vilan(staged)
        .env("VILAN_NATIVE_REPORT_COPIES", "1")
        .args(["build", "--backend", "rust", "--stdout", program])
        .output()
        .expect("emit the program");
    assert!(
        output.status.success(),
        "{program} must emit for the census:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout
        .lines()
        .find(|line| line.starts_with("vilan-native: consumed-copies="))
        .unwrap_or_else(|| panic!("{program} reported no copy census"));
    let mut numbers = line
        .split(|character: char| !character.is_ascii_digit())
        .filter(|piece| !piece.is_empty())
        .map(|piece| piece.parse::<usize>().expect("a count"));
    let copied = numbers.next().expect("the copied count");
    let elided = numbers.next().expect("the elided count");
    (copied, elided)
}

#[test]
fn the_native_copy_census_matches_its_table() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_board.vl"), BOARD_PROBE)
        .expect("write the board probe");
    let mut rows = Vec::new();
    for program in DEFAULT_SUITE
        .iter()
        .copied()
        .chain(std::iter::once("native_probe_board.vl"))
    {
        let (copied, elided) = copy_census_of(&staged, program);
        rows.push(format!(
            "{}\t{copied}\t{elided}",
            program.trim_end_matches(".vl")
        ));
    }
    let measured = format!(
        "{}{}\n",
        concat!(
            "# Consumed place reads the NATIVE emitter copied, and the ones it\n",
            "# moved at a last use (tracker F31). Regenerate with\n",
            "# VILAN_REGENERATE_NATIVE_COPY_CENSUS=1 cargo test -p vilan-cli \
             --test native_differential\n",
        ),
        rows.join("\n")
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(NATIVE_COPY_CENSUS);
    if std::env::var_os("VILAN_REGENERATE_NATIVE_COPY_CENSUS").is_some() {
        std::fs::write(&path, &measured).expect("write the census");
        return;
    }
    let committed = std::fs::read_to_string(&path).expect("read the committed census");
    assert_eq!(
        committed, measured,
        "the native copy census moved; read the difference, then regenerate with \
         VILAN_REGENERATE_NATIVE_COPY_CENSUS=1"
    );
}

/// F29: a program whose host bindings sit ONLY inside a refused call — in its
/// arguments, and in the body of a closure among them.
///
/// `host_name` is reached nowhere but inside the argument of the refused
/// `set_priority` call; `range_i32` nowhere but inside the body of the closure
/// the refused `hmr_register_teardown` takes. Neither was in the census before
/// the walk continued past a refusal, which is how `createServer`'s handler hid
/// nine bindings from the census that sized F18.
///
/// The argument pair is the PROGRAM's own `node:os` bindings, which no backend
/// table will ever answer: the pair had been `random_bytes(random_uuid()..)`,
/// and F40 lowered both, which turned the pin's shape into a program with
/// nothing hidden in it. A pair the runtime cannot grow into keeps it measuring
/// the walk rather than the runtime's coverage.
const CENSUS_PROBE: &str = concat!(
    "import std::random::range_i32;\n",
    "import std::rpc::hmr_register_teardown;\n",
    "import std::io::print;\n",
    "\n",
    "[extern(\"node:os\", \"hostname\")]\n",
    "external fun host_name(): str;\n",
    "\n",
    "[extern(\"node:os\", \"setPriority\")]\n",
    "external fun set_priority(name: str): i32;\n",
    "\n",
    "fun main() {\n",
    "\tprint(set_priority(host_name()));\n",
    "\thmr_register_teardown(|| { print(range_i32(1, 4)); });\n",
    "}\n",
);

/// F29's pin: the host census reports what a REFUSAL stands in front of.
///
/// The census answers "what host surface does this program still need", and a
/// walk that stops at the first refusal answers a smaller question. The two
/// bindings asserted here are each reachable through exactly ONE refused
/// construct, so a walk that stops names neither — which is what makes this
/// pin measure the continuation rather than the program.
#[test]
fn the_host_census_walks_past_a_refusal_into_its_arguments_and_closure_bodies() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_census.vl"), CENSUS_PROBE)
        .expect("write the probe program");
    let output = vilan(&staged)
        .env("VILAN_NATIVE_HOST_CENSUS", "1")
        .args([
            "build",
            "--backend",
            "rust",
            "--stdout",
            "native_probe_census.vl",
        ])
        .output()
        .expect("census the probe");
    assert!(
        output.status.success(),
        "the census emit failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let census = String::from_utf8_lossy(&output.stdout);
    for hidden in [
        // Reached only inside the ARGUMENT of a refused host binding's call.
        "the host binding `host_name`",
        // Reached only inside the BODY of a closure handed to a refused one.
        "the intrinsic `RandomInt`",
    ] {
        assert!(
            census.contains(hidden),
            "the census must walk past a refusal and report `{hidden}`:\n{census}"
        );
    }
    // And the refusals themselves are still reported, which is what the walk is
    // a continuation OF.
    for refused in [
        "the host binding `set_priority`",
        "the host binding `hmr_register_teardown`",
    ] {
        assert!(
            census.contains(refused),
            "the census must still name the refusal itself `{refused}`:\n{census}"
        );
    }
}

/// F30: a binding that lives in a CELL, mutated IN PLACE through every spelling
/// the language has for it.
///
/// A module-level binding is a `thread_local!` and a mutably-captured one is a
/// `Captured` cell; a read of either answers a VALUE, which is right for a value
/// and silently wrong for a place — the mutation lands in a temporary that is
/// dropped at the end of the statement. Every line below printed the
/// UNMUTATED value natively while the JS backend printed the mutated one, and
/// none of them was visible to the whole-set differential because every mutated
/// module binding in the corpus holds a `Shared`, whose copy is the same cell.
///
/// The last two lines are the borrow's own hazard rather than the copy's: the
/// cell is borrowed for the whole of the mutating call, so a read of the same
/// binding among the ARGUMENTS has to happen before the borrow is taken.
const CELL_PLACE_PROBE: &str = concat!(
    "struct Counter { n: i32 }\n",
    "\n",
    "impl Counter {\n",
    "\tfun bump(&mut self) { self.n = self.n + 1; }\n",
    "}\n",
    "\n",
    "mut counts: List<i32> = [1, 2];\n",
    "mut counter: Counter = Counter { n = 0 };\n",
    "\n",
    "fun record(value: i32) { counts.push(value); }\n",
    "\n",
    "fun grow(xs: &mut List<i32>, by: i32) { xs.push(by); }\n",
    "\n",
    "fun main() {\n",
    "\trecord(7);\n",
    "\tprint(counts);\n",
    "\tcounter.bump();\n",
    "\tcounter.bump();\n",
    "\tprint(counter.n);\n",
    "\tcounter.n = 41;\n",
    "\tprint(counter.n);\n",
    "\tcounts[0] = 5;\n",
    "\tprint(counts);\n",
    "\tgrow(&mut counts, 9);\n",
    "\tprint(counts);\n",
    "\tmut seen: List<i32> = [];\n",
    "\tmut inner: Counter = Counter { n = 0 };\n",
    "\tlet bump = || { seen.push(seen.len()); inner.bump(); };\n",
    "\tbump();\n",
    "\tbump();\n",
    "\tprint(seen);\n",
    "\tprint(inner.n);\n",
    "\tcounts.push(counts.len());\n",
    "\tprint(counts);\n",
    "\tgrow(&mut counts, counts.len());\n",
    "\tprint(counts);\n",
    "}\n",
);

/// F30's pin: every cell-resident binding above is mutated IN PLACE, on both
/// backends, to the same bytes.
///
/// It is written as ONE program because it is one defect seen from seven
/// sides — a mutating intrinsic's receiver, a `&mut self` method, a field
/// write, an index write, a `&mut` argument, and the two argument orders the
/// borrow constrains — and because a program that mixes them is the one that
/// catches a fix applied at only one of them.
#[test]
fn a_binding_that_lives_in_a_cell_is_mutated_in_place_on_both_backends() {
    let staged = stage();
    std::fs::write(staged.join("native_probe_cell_place.vl"), CELL_PLACE_PROBE)
        .expect("write the probe program");
    assert_eq!(
        compare(&staged, "native_probe_cell_place.vl"),
        Verdict::Identical,
        "a module-level or mutably-captured binding mutated in place must be mutated in the CELL, \
         not in a copy of its value"
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
    // F22: `adapt.vl` joins them. It is the corpus's ADAPTED-INSTANCE program
    // — one `map` called with an async closure at one site and a synchronous
    // one at another, and a `run` the same way — so it is the pin that this
    // emitter monomorphises on asyncness as well as on types. Two instances of
    // each callee come out, one `async fn` and one plain, and the program that
    // proves it is the one whose two answers must be the same bytes.
    for required in ["await-postfix.vl", "nursery.vl", "adapt.vl"] {
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
/// is ordered (the 5 ms task finishes before the 500 ms one that was spawned
/// FIRST), and a join answers the spawned value.
///
/// The two deadlines are 5 ms and 500 ms because this probe's claim is an
/// ORDER, and the suite's rule for one is at the head (N116): a hundredfold
/// gap, so no scheduling stall on this machine can let the later deadline tie
/// with the earlier and invert the two runtimes' answers.
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
    // N116: and the order the two agree on is the order this probe CLAIMS.
    //
    // `compare` asks only whether the two backends print the same bytes, so a
    // change that inverted the deadline list on BOTH of them would satisfy it
    // perfectly — and, before the deadlines were widened, a busy box could
    // invert node's alone, which is what made this pin flaky. Stating the
    // expected stdout costs one more `vilan run` and turns both of those into
    // a red that names the rule.
    let javascript = vilan(&staged)
        .args(["run", "native_probe_spawn.vl"])
        .output()
        .expect("run the JS backend");
    assert!(javascript.status.success(), "{javascript:?}");
    assert_eq!(
        String::from_utf8_lossy(&javascript.stdout),
        // A spawn is EAGER, so both `enter` lines precede `spawned`; the
        // deadline list is ordered, so `early` leaves first although `late`
        // was spawned first; and each join answers its own task's value.
        "enter late\nenter early\nspawned\nleave early\nearly\nleave late\nlate\n",
        "the three rules this probe exists for, spelled out"
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
    // N116: 1 ms and 12 ms were the deadlines, and under a parallel lane both
    // could be overdue by the time either runtime reached its timer phase —
    // at which point node fires them in INSERTION order (`late` first) while
    // the native executor fires them in DEADLINE order, and the two stdouts
    // part company over nothing. 5 ms and 500 ms is the same claim with a
    // stall budget: the ordering inverts only if a runtime takes half a
    // second to look at its timers.
    "\tlet late = async step(\"late\", 500);\n",
    "\tlet early = async step(\"early\", 5);\n",
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
