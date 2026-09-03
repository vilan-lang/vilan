//! E36 — THE gate for the release preset: over the whole corpus, a
//! `preset = "release"` build must be observationally identical to the debug
//! build of the same program.
//!
//! Until this existed the release emission path was ungated. The corpus gate
//! (`vilan-cli/tests/corpus.rs`) builds bare `.vl` files with no manifest, so it
//! only ever exercises the debug default — deliberately, because that is what
//! makes "no corpus golden moved" a real signal — and `infer_preset.rs` pins the
//! release path on a single fixture. Everything else release does to a program
//! was unobserved, which is how B69 shipped: minification's short-name renaming
//! collided on seven corpus programs, on a v0.27.0 binary, for as long as anyone
//! had been able to run `vilan build` on a release manifest.
//!
//! **Not a golden.** Release output legitimately differs from debug in almost
//! every byte — different names, no indentation, no padding, folded constants —
//! so there is nothing to compare bytes against and a release golden would pin
//! the minifier's current letters rather than its correctness. What the two
//! builds owe each other is BEHAVIOUR, and the only instrument that reads it is
//! node. That matters more here than anywhere else in the tree: a renaming bug
//! emits perfectly valid JavaScript (`default.vl`'s two `function b` compile
//! fine and recurse forever), so "it still builds" proves nothing at all.
//!
//! Method (`infer_differential.rs`'s shape, reused): each corpus program is
//! analyzed ONCE and transformed twice off the same `Program` — debug, then
//! release with the inference sweep's folds installed, exactly as `vilan build`
//! wires it — and both emissions are run under node with their `(stdout, exit
//! code)` compared. Unlike the inference differential there is no
//! run-only-on-difference shortcut to take, since release always differs; the
//! whole corpus runs twice, which is what the leg costs.
//!
//! # One test per corpus program (tracker N49)
//!
//! That cost used to be paid by ONE test. It looped the corpus behind an 8-way
//! `thread::scope`, and at 615.5 s under load it was the union's last finisher
//! — a straggler nextest could do nothing about, because the whole leg was a
//! single unit of scheduling that could not use more than its own eight threads
//! however idle the box was, and whose static chunking made the slowest chunk
//! the whole test's clock. Its failure message had to name the program itself,
//! for the same reason.
//!
//! So the corpus is DECLARED here, one line each, and [`corpus_programs!`]
//! writes a test per program. nextest then schedules 124 independent processes
//! across every core it has, a regression names its program in the test id
//! rather than in a message, and one bad program can be re-run on its own.
//! The corpus-wide claim survives as the SUM of the parts, and
//! [`every_corpus_program_has_a_test_of_its_own`] is what makes the sum whole:
//! a `.vl` added to `vilan/test/` and not declared here is red, by name.
//!
//! **And the split named its program on the first run.** 600 of those 615
//! seconds were `watch.vl` alone, which never exits under node: both builds ran
//! to `NODE_TIMEOUT`, both were killed at 300 s, and the gate compared two
//! identical "node did not exit" strings and passed. The corpus was never the
//! cost and the whole-corpus loop is what hid it — a per-program time is the
//! thing nobody could read while every program shared one clock. It is in
//! [`NOT_RUN`] now, with the reason, and the family's longest member is 9.6 s.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use vilan_core::options::{BuildOptions, Preset};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source, transform};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// How long a corpus program gets under node before the run is declared hung.
///
/// A LIVENESS bound, not a performance assertion — this gate's claim is that
/// the two presets print the same thing, never that either prints it quickly —
/// so the number only has to be too large for a healthy program and finite for
/// a hung one. A green run never pays it: the loop breaks the moment the child
/// exits.
///
/// It was 30 s, and 30 s is not too large for a healthy corpus program on a
/// contended box: this binary was one of the >120 s members of two sibling
/// lanes' unions under ten-lane load (tracker N46), which is the same disease
/// E39/E40 treated across the watch family — a fixed clock around real work,
/// measuring the runner rather than the program. 300 s is the value that family
/// settled on and the reasoning transfers unchanged.
const NODE_TIMEOUT: Duration = Duration::from_secs(300);

/// Corpus programs this gate does not run, with the reason — `infer_differential.
/// rs`'s list plus one: their output is not a function of their source alone, so
/// "both builds printed the same thing" is not a claim either build can make.
/// They are still COMPILED both ways below, which is itself worth having; only
/// the node leg is skipped.
const NOT_RUN: &[(&str, &str)] = &[
    ("time.vl", "host clock: two runs print different timestamps"),
    ("crypto.vl", "host WebCrypto: a fresh random draw per run"),
    ("db.vl", "host database: touches the filesystem"),
    ("process-env.vl", "reads the host environment and argv"),
    // Sequential runs agree; the divergence appeared only under the 8-way
    // parallelism this gate used to run its own loop with, which is the proof
    // that it is scheduling and not codegen. E32's rule applies — a wall-clock
    // margin is not an assertion.
    (
        "nursery.vl",
        "host timers 10ms apart decide the print order, which load can reorder",
    ),
    // The whole of tracker N49, in one row. `watch.vl` blocks on `flat.next()`
    // for a filesystem change that never comes — "a watch never ends on its
    // own", as the program's own comment says — so BOTH builds ran to
    // `NODE_TIMEOUT` and were killed, twice 300 s, and the gate then compared
    // two identical `Err("node did not exit within 300s")` strings and passed.
    // That was 600 s of the union's 607 s critical path spent on a verdict that
    // could not come out any other way. Shortening the wait instead would be a
    // fixed clock around real work — E32's disease, and the one this file's own
    // `NODE_TIMEOUT` comment was written to cure — since the observation would
    // become "what had it printed when we gave up", which load decides.
    (
        "watch.vl",
        "never terminates: it waits on a filesystem change that never arrives, \
         so both builds are killed at the deadline and neither is observed",
    ),
];

/// Runs `node <path>` under a deadline, returning `(stdout, exit code)`. Both
/// pipes are drained by reader threads so a chatty program cannot deadlock
/// against a full pipe buffer (the shape `tests/interpreter.rs` established;
/// `timeout(1)` is not available on Windows and this gate must run there).
fn run_node(path: &Path) -> Result<(String, i32), String> {
    let mut child = Command::new("node")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not run node: {error}"))?;
    let mut stdout_pipe = child.stdout.take().expect("piped stdout");
    let mut stderr_pipe = child.stderr.take().expect("piped stderr");
    let reading_stdout = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut bytes);
        bytes
    });
    let reading_stderr = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stderr_pipe.read_to_end(&mut bytes);
        bytes
    });
    let deadline = Instant::now() + NODE_TIMEOUT;
    let status = loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) => break Some(status),
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    let stdout = String::from_utf8_lossy(&reading_stdout.join().unwrap_or_default()).into_owned();
    let stderr = String::from_utf8_lossy(&reading_stderr.join().unwrap_or_default()).into_owned();
    match status {
        // A failing build's stderr is the whole diagnosis (`SyntaxError:
        // Identifier 'b' has already been declared`), so it rides along with the
        // exit code rather than being dropped.
        Some(status) if !status.success() => Ok((
            format!("{stdout}--- stderr ---\n{stderr}"),
            status.code().unwrap_or(-1),
        )),
        Some(status) => Ok((stdout, status.code().unwrap_or(-1))),
        None => Err(format!(
            "node did not exit within {}s (killed)",
            NODE_TIMEOUT.as_secs()
        )),
    }
}

/// What one corpus program's two builds came to.
struct Compared {
    debug: String,
    release: String,
}

/// Analyzes once and transforms twice off the same `Program`, so the only
/// difference between the two emissions is the preset. The release half runs the
/// inference sweep the way `vilan build` does — after analysis, gated on the
/// resolved options — so what this gate reads is the release path a user gets,
/// not an approximation of it.
/// A corpus program's package root is the corpus DIRECTORY, not the process
/// working directory. Every runner over the corpus has to say so: a corpus
/// program may name a project file — `const asset::bundle` carries a resource
/// beside it into the build (kolt.local 029), and `const asset::read` would
/// read one — and the const channel resolves those against the package root.
/// Compiled under `.`, such a program fails to find a file that is right there
/// beside it.
fn build_both_ways(source: String, root: PathBuf) -> Result<Compared, String> {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &root,
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let mut program = match program {
                Some(program) if errors.is_empty() => program,
                _ => return Err(format!("compile failed: {errors:?}")),
            };
            let debug_options = BuildOptions::from_preset(Preset::Debug);
            let release_options = BuildOptions::from_preset(Preset::Release);

            // Debug first: it must see `const_results` as analysis left them.
            let debug = transform(&program, &debug_options).map_err(|error| error.msg)?;

            program
                .const_results
                .extend(vilan_core::const_eval::infer(&program, &release_options));
            let release = transform(&program, &release_options).map_err(|error| error.msg)?;

            Ok(Compared { debug, release })
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err("worker thread aborted".to_string()))
}

/// Writes `javascript` to a uniquely named scratch file and runs it.
fn run(javascript: &str, label: &str) -> Result<(String, i32), String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_release_diff_{}_{unique}_{label}.mjs",
        std::process::id()
    ));
    std::fs::write(&path, javascript).map_err(|error| error.to_string())?;
    let outcome = run_node(&path);
    let _ = std::fs::remove_file(&path);
    outcome
}

/// The corpus directory: the package root every corpus program is compiled
/// under, and the only place this gate looks for programs.
fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test")
}

/// One corpus program, both ways. The body every generated test runs.
fn the_release_preset_is_neutral_on(program: &str) {
    let corpus = corpus_dir();
    let path = corpus.join(program);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let compared = build_both_ways(source, corpus).unwrap_or_else(|error| panic!("{error}"));

    if let Some((_, why)) = NOT_RUN.iter().find(|(excluded, _)| *excluded == program) {
        // Compiled both ways — which is worth having on its own — and not run.
        // Said out loud, because a skip nobody can see is a skip nobody rereads.
        eprintln!(
            "[release differential] {program}: compiled both ways, not run under node — {why}"
        );
        return;
    }

    let release = run(&compared.release, "release");
    let debug = run(&compared.debug, "debug");
    match (release, debug) {
        (Ok(release), Ok(debug)) => assert!(
            release == debug,
            "THE RELEASE PRESET CHANGED BEHAVIOUR\n  \
             release: exit {}, stdout {:?}\n  \
             debug:   exit {}, stdout {:?}\n  \
             release emitted:\n{}",
            release.1,
            release.0,
            debug.1,
            debug.0,
            compared.release
        ),
        // A run that could not happen is only a failure if the two sides
        // disagree about it.
        (release, debug) => assert!(
            format!("{release:?}") == format!("{debug:?}"),
            "one build ran and the other did not\n  release: {release:?}\n  debug: {debug:?}"
        ),
    }
}

/// Writes one test per corpus program, and records the declaration the coverage
/// gate below reads.
///
/// The module is named for the program, so the test id nextest prints is
/// `release_differential list_sort::survives_the_release_preset` — the program's
/// own name, in the place a runner shows it. Two columns rather than one
/// because the mapping is not derivable in either direction: `async-await.vl`
/// cannot be an identifier and `resource_take.vl` really does spell an
/// underscore, so the file name is written out and
/// [`every_corpus_program_has_a_test_of_its_own`] holds the pair to the
/// convention.
macro_rules! corpus_programs {
    ($($module:ident => $file:literal,)*) => {
        /// Every corpus program with a test, as `(module name, file name)`.
        const DECLARED: &[(&str, &str)] = &[$((stringify!($module), $file),)*];

        $(
            mod $module {
                #[test]
                fn survives_the_release_preset() {
                    super::the_release_preset_is_neutral_on($file);
                }
            }
        )*
    };
}

corpus_programs! {
    adapt => "adapt.vl",
    arena => "arena.vl",
    asset_bundle => "asset_bundle.vl",
    async_await => "async-await.vl",
    async_promise_all => "async-promise-all.vl",
    await_postfix => "await-postfix.vl",
    backed_enum_keys => "backed-enum-keys.vl",
    blanket_impl => "blanket-impl.vl",
    bool => "bool.vl",
    borrows_inferred => "borrows-inferred.vl",
    borrows => "borrows.vl",
    capture_clones => "capture-clones.vl",
    closure_param_inference => "closure-param-inference.vl",
    compound_index => "compound-index.vl",
    r#const => "const.vl",
    copy_elision => "copy-elision.vl",
    copy_in_loop => "copy-in-loop.vl",
    crypto => "crypto.vl",
    css_block => "css-block.vl",
    db => "db.vl",
    default_generic_param => "default-generic-param.vl",
    default => "default.vl",
    derive_debug => "derive-debug.vl",
    derive_default => "derive-default.vl",
    derive_enum => "derive-enum.vl",
    derive_eq => "derive-eq.vl",
    derive_json => "derive-json.vl",
    destructuring => "destructuring.vl",
    display => "display.vl",
    element_clones => "element-clones.vl",
    element_syntax => "element-syntax.vl",
    enum_discriminant => "enum-discriminant.vl",
    equality => "equality.vl",
    estate => "estate.vl",
    expression_lift => "expression-lift.vl",
    field_assignment => "field-assignment.vl",
    file => "file.vl",
    fixed_arrays => "fixed-arrays.vl",
    for_in => "for-in.vl",
    format => "format.vl",
    for_mut_container => "for-mut-container.vl",
    for_views => "for-views.vl",
    gap_b => "gap-b.vl",
    generic_adapter_dispatch => "generic-adapter-dispatch.vl",
    generic_dispatch => "generic-dispatch.vl",
    generic_equality => "generic-equality.vl",
    generic_inference => "generic-inference.vl",
    generic_method_return => "generic-method-return.vl",
    generic_methods => "generic-methods.vl",
    interpolated_multiline_string => "interpolated-multiline-string.vl",
    iterator_adapters => "iterator-adapters.vl",
    iterator_protocol => "iterator-protocol.vl",
    iterator => "iterator.vl",
    json_roundtrip => "json-roundtrip.vl",
    lift_chain => "lift-chain.vl",
    list_build_infer => "list-build-infer.vl",
    list_element_type => "list-element-type.vl",
    list_get_pop => "list-get-pop.vl",
    list_join => "list-join.vl",
    list_literal_iteration => "list-literal-iteration.vl",
    list_methods => "list-methods.vl",
    list_search => "list-search.vl",
    list_sort => "list-sort.vl",
    list_splice => "list-splice.vl",
    loops => "loops.vl",
    macro_block => "macro-block.vl",
    macro_derive => "macro-derive.vl",
    macro_invoke => "macro-invoke.vl",
    main_ret => "main-ret.vl",
    map => "map.vl",
    match_ergonomics => "match-ergonomics.vl",
    match_patterns => "match-patterns.vl",
    math => "math.vl",
    multiline_string => "multiline-string.vl",
    mut_parameters => "mut-parameters.vl",
    number_math => "number-math.vl",
    numeric_types => "numeric-types.vl",
    nursery => "nursery.vl",
    operator_overload => "operator-overload.vl",
    option_view => "option-view.vl",
    parse_f64 => "parse-f64.vl",
    parse_i32 => "parse-i32.vl",
    preflight => "preflight.vl",
    prelude => "prelude.vl",
    process_env => "process-env.vl",
    range => "range.vl",
    reactive_flatten => "reactive-flatten.vl",
    reactive_keyed => "reactive-keyed.vl",
    reactive_owner => "reactive-owner.vl",
    reactive_turns => "reactive-turns.vl",
    reactive => "reactive.vl",
    recursion => "recursion.vl",
    remainder => "remainder.vl",
    resource_exit => "resource_exit.vl",
    resource_take => "resource_take.vl",
    resource => "resource.vl",
    result_combinators => "result-combinators.vl",
    scoped_import => "scoped-import.vl",
    self_return => "self-return.vl",
    set => "set.vl",
    shared => "shared.vl",
    side_effect_let => "side-effect-let.vl",
    signal_update => "signal-update.vl",
    spread_parameters => "spread-parameters.vl",
    ssr_render => "ssr-render.vl",
    string_interpolation => "string-interpolation.vl",
    string_methods => "string-methods.vl",
    struct_literal_call => "struct-literal-call.vl",
    style => "style.vl",
    subscript => "subscript.vl",
    theme => "theme.vl",
    time => "time.vl",
    trait_default => "trait-default.vl",
    transparent_references => "transparent-references.vl",
    try_assert => "try-assert.vl",
    tuple_access => "tuple-access.vl",
    tuple_spread => "tuple-spread.vl",
    unary_minus => "unary-minus.vl",
    value_semantics => "value-semantics.vl",
    view_basic => "view-basic.vl",
    view_conventions => "view-conventions.vl",
    view_field => "view-field.vl",
    view_params => "view-params.vl",
    view_primitive => "view-primitive.vl",
    watch => "watch.vl",
}

/// The identifier a program's file name must be declared under: the stem, with
/// everything an identifier cannot carry replaced by `_`.
fn module_name_for(file: &str) -> String {
    file.strip_suffix(".vl")
        .unwrap_or(file)
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '_'
            }
        })
        .collect()
}

#[test]
fn every_corpus_program_has_a_test_of_its_own() {
    // The corpus-wide claim, as the SUM of the parts (tracker N49). The old
    // single test read the directory at run time, so a new corpus program was
    // covered the moment it landed; a declared list is not read by the
    // filesystem and would rot in silence in exactly that direction — which is
    // the direction that matters, since an uncovered program is a release bug
    // nothing would catch. So the declaration is held to the directory, both
    // ways, by name.
    let present: BTreeSet<String> = std::fs::read_dir(corpus_dir())
        .expect("corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()? != "vl" {
                return None;
            }
            Some(path.file_name()?.to_string_lossy().into_owned())
        })
        .collect();
    let declared: BTreeSet<&str> = DECLARED.iter().map(|(_, file)| *file).collect();

    assert_eq!(
        DECLARED.len(),
        declared.len(),
        "a corpus program is declared twice — two tests over one program leave \
         another program with none"
    );
    let undeclared: Vec<&String> = present
        .iter()
        .filter(|file| !declared.contains(file.as_str()))
        .collect();
    assert!(
        undeclared.is_empty(),
        "corpus program(s) {undeclared:?} have no test here, so the release preset \
         is unobserved on them. Add a line to `corpus_programs!` — the module is \
         the file's stem with `-` written `_`."
    );
    let gone: Vec<&&str> = declared
        .iter()
        .filter(|file| !present.contains(**file))
        .collect();
    assert!(
        gone.is_empty(),
        "`corpus_programs!` declares {gone:?}, which `vilan/test/` no longer holds \
         — the test would fail on a missing file. Delete the line."
    );
    for (module, file) in DECLARED {
        assert_eq!(
            module.trim_start_matches("r#"),
            module_name_for(file),
            "the test for `{file}` is declared under `{module}`, which names a \
             different program than it runs"
        );
    }

    // Non-vacuity, the two bounds the single test used to assert at the end of
    // its own loop: the corpus is a corpus, and nearly all of it reaches node.
    assert!(present.len() > 60, "suspiciously few corpus programs");
    assert!(
        DECLARED.len() - NOT_RUN.len() >= 60,
        "only {} corpus program(s) reach node — the gate is close to vacuous. \
         Every corpus program that compiles should run both ways.",
        DECLARED.len() - NOT_RUN.len()
    );
}

#[test]
fn every_program_not_run_is_still_a_corpus_program() {
    // `NOT_RUN`'s inverse (N42's shape, N50's family). An exemption only ever
    // subtracts work, so a name that has left the corpus goes on subtracting it
    // from nothing, and the next program to take that name inherits a skip
    // nobody chose.
    let declared: BTreeSet<&str> = DECLARED.iter().map(|(_, file)| *file).collect();
    let gone: Vec<&str> = NOT_RUN
        .iter()
        .map(|(file, _)| *file)
        .filter(|file| !declared.contains(file))
        .collect();
    assert!(
        gone.is_empty(),
        "NOT_RUN names {gone:?}, which is not a corpus program — the exclusion \
         excludes nothing. Delete the entry."
    );
}
