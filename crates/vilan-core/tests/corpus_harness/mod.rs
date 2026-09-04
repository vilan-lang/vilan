//! The corpus, declared ONCE, plus the node harness every corpus differential
//! runs it through (tracker N52).
//!
//! Two gates read `vilan/test/` program by program — `release_differential.rs`
//! (debug against the release preset) and `infer_differential.rs` (the
//! inference sweep off against on) — and N49 gave the first of them the shape
//! that makes a regression nameable: the corpus is DECLARED, one line per
//! program, and a macro writes a test per line, so nextest schedules one
//! process each, a failure names its program in the test id, and one bad
//! program can be re-run on its own.
//!
//! Declaring it TWICE would be the obvious way to give the second gate the same
//! shape, and it is the one thing this module exists to prevent. The two lists
//! had already drifted while there was only one of them: the older gate's
//! exclusions were four entries and the newer one's six, so `nursery.vl` — whose
//! print order host timers decide — was excluded from the release gate and run
//! by the inference gate, and nothing anywhere said the two were meant to agree.
//! A list a human has to copy is a list that is one commit from being wrong in a
//! direction nobody reads.
//!
//! So there is ONE list here, and [`corpus_manifest!`] hands it to whatever
//! macro a gate passes in — the gate writes the test body, this module writes
//! the roster. The pin that they cannot come apart is
//! [`assert_the_declaration_is_the_corpus`], which BOTH gates call on their own
//! generated `DECLARED`: each is held to `vilan/test/` in both directions, so
//! each equals the directory and therefore equals the other. A `.vl` added to
//! the corpus and not declared here is red in both binaries, by name.
//!
//! The node leg is shared for the same reason. It was two copies of one
//! function with a divergence nobody chose: one deadline was 300 s (E39/E40's
//! number, raised across the watch family by N46) and the other was still 30 s,
//! which is not too large for a healthy corpus program on a box carrying ten
//! lanes — a fixed clock around real work, measuring the runner rather than the
//! program. One copy, one number.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Every corpus program, once, handed to the caller's macro.
///
/// The caller passes the name of a `macro_rules!` that accepts
/// `$module:ident => $file:literal,` rows and writes whatever a gate needs from
/// them — a test per program, and the `DECLARED` roster
/// [`assert_the_declaration_is_the_corpus`] reads.
///
/// Two columns rather than one because the mapping is not derivable in either
/// direction: `async-await.vl` cannot be an identifier and `resource_take.vl`
/// really does spell an underscore, so the file name is written out and the
/// coverage gate holds the pair to the convention.
macro_rules! corpus_manifest {
    ($tests:ident) => {
        $tests! {
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
    };
}

/// Corpus programs no differential RUNS, with the reason.
///
/// One list, because the reason is a property of the PROGRAM and not of the
/// gate reading it: these are the programs whose output is not a function of
/// their source alone — a clock, a random draw, a host database, the
/// environment, an order host timers pick — so "both builds printed the same
/// thing" is not a claim either build can make about them. They are still
/// COMPILED both ways by every gate, which is worth having on its own; only the
/// node leg is skipped.
///
pub const NOT_RUN: &[(&str, &str)] = &[
    ("time.vl", "host clock: two runs print different timestamps"),
    ("crypto.vl", "host WebCrypto: a fresh random draw per run"),
    ("db.vl", "host database: touches the filesystem"),
    ("process-env.vl", "reads the host environment and argv"),
    // Sequential runs agree; the divergence appeared only under the 8-way
    // parallelism the release gate used to run its own loop with, which is the
    // proof that it is scheduling and not codegen. E32's rule applies — a
    // wall-clock margin is not an assertion.
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
    // fixed clock around real work — E32's disease, and the one `NODE_TIMEOUT`
    // below was written to cure — since the observation would become "what had
    // it printed when we gave up", which load decides. Tracker N51 asks the
    // owner what the corpus should hold instead.
    (
        "watch.vl",
        "never terminates: it waits on a filesystem change that never arrives, \
         so both builds are killed at the deadline and neither is observed",
    ),
];

/// The reason `program` is not run under node, if it is one of [`NOT_RUN`]'s.
pub fn not_run_reason(program: &str) -> Option<&'static str> {
    NOT_RUN
        .iter()
        .find(|(excluded, _)| *excluded == program)
        .map(|(_, why)| *why)
}

/// How long a corpus program gets under node before the run is declared hung.
///
/// A LIVENESS bound, not a performance assertion — no gate here claims how
/// fast a program prints, only that two builds of it print the same thing — so
/// the number only has to be too large for a healthy program and finite for a
/// hung one. A green run never pays it: the loop breaks the moment the child
/// exits.
///
/// 30 s is not too large for a healthy corpus program on a contended box: these
/// binaries were among the >120 s members of two sibling lanes' unions under
/// ten-lane load (tracker N46), which is the same disease E39/E40 treated across
/// the watch family — a fixed clock around real work, measuring the runner
/// rather than the program. 300 s is the value that family settled on and the
/// reasoning transfers unchanged.
pub const NODE_TIMEOUT: Duration = Duration::from_secs(300);

/// The corpus directory: the package root every corpus program is compiled
/// under, and the only place these gates look for programs.
pub fn corpus_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test")
}

/// Runs `node <path>` under [`NODE_TIMEOUT`], returning `(stdout, exit code)`.
/// Both pipes are drained by reader threads so a chatty program cannot deadlock
/// against a full pipe buffer (the shape `tests/interpreter.rs` established;
/// `timeout(1)` is not available on Windows and these gates must run there).
pub fn run_node(path: &Path) -> Result<(String, i32), String> {
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

/// Writes `javascript` to a uniquely named scratch file and runs it. `gate`
/// names the differential and `label` the side of it, so a leftover file after
/// a kill says which run wrote it.
pub fn run(javascript: &str, gate: &str, label: &str) -> Result<(String, i32), String> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_{gate}_diff_{}_{unique}_{label}.mjs",
        std::process::id()
    ));
    std::fs::write(&path, javascript).map_err(|error| error.to_string())?;
    let outcome = run_node(&path);
    let _ = std::fs::remove_file(&path);
    outcome
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

/// The corpus-wide claim, as the SUM of the parts (tracker N49), and the pin
/// that the two gates' rosters are ONE roster (tracker N52).
///
/// A whole-corpus loop read the directory at run time, so a new corpus program
/// was covered the moment it landed; a declared list is not read by the
/// filesystem and would rot in silence in exactly that direction — which is the
/// direction that matters, since an uncovered program is a bug nothing would
/// catch. So the declaration is held to the directory, BOTH ways, by name. Every
/// gate calls this on its own generated roster, which is what makes the rosters
/// equal: each equals `vilan/test/`, so each equals the other.
pub fn assert_the_declaration_is_the_corpus(declared_rows: &[(&str, &str)]) {
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
    let declared: BTreeSet<&str> = declared_rows.iter().map(|(_, file)| *file).collect();

    assert_eq!(
        declared_rows.len(),
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
        "corpus program(s) {undeclared:?} have no test here, so they are \
         unobserved. Add a line to `corpus_manifest!` in \
         `tests/corpus_harness/mod.rs` — the module is the file's stem with `-` \
         written `_`, and both differentials pick it up."
    );
    let gone: Vec<&&str> = declared
        .iter()
        .filter(|file| !present.contains(**file))
        .collect();
    assert!(
        gone.is_empty(),
        "`corpus_manifest!` declares {gone:?}, which `vilan/test/` no longer holds \
         — the test would fail on a missing file. Delete the line."
    );
    for (module, file) in declared_rows {
        assert_eq!(
            module.trim_start_matches("r#"),
            module_name_for(file),
            "the test for `{file}` is declared under `{module}`, which names a \
             different program than it runs"
        );
    }

    // Non-vacuity, the two bounds the whole-corpus loops used to assert at the
    // end of their own bodies: the corpus is a corpus, and nearly all of it
    // reaches node.
    assert!(present.len() > 60, "suspiciously few corpus programs");
    assert!(
        declared_rows.len() - NOT_RUN.len() >= 60,
        "only {} corpus program(s) reach node — the gate is close to vacuous. \
         Every corpus program that compiles should run.",
        declared_rows.len() - NOT_RUN.len()
    );
}

/// [`NOT_RUN`]'s inverse (N42's shape, N50's family). An exemption only ever
/// subtracts work, so a name that has left the corpus goes on subtracting it
/// from nothing, and the next program to take that name inherits a skip nobody
/// chose.
pub fn assert_every_program_not_run_is_a_corpus_program(declared_rows: &[(&str, &str)]) {
    let declared: BTreeSet<&str> = declared_rows.iter().map(|(_, file)| *file).collect();
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
