//! The shared harness for the `inference` test binary: the compile drivers, the
//! `assert_*` pins every subject module is written against, and the fixtures
//! that more than one subject needs.
//!
//! Split out of the single 69k-line `tests/inference.rs` by B145. The subject
//! modules beside this one are `mod`s of ONE binary (`main.rs`), deliberately:
//! each top-level integration-test file links the whole crate (`suite-speed.md`
//! E21), so N files would cost N-1 extra link steps.
//!
//! Every case here runs through the real pipeline on a large-stack worker, so a
//! recursion bug surfaces as an error rather than an aborted suite.

pub use std::path::{Path, PathBuf};

pub use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

pub fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Compile a source through the full pipeline (analyze → context → infer →
/// transform) on a 256 MB-stack worker, matching the CLI. Returns the emitted JS
/// on a clean compile, or the diagnostics. A panic becomes an error rather than
/// aborting the test process.
pub fn compile(source: &str) -> Result<String, Vec<String>> {
    compile_on(source, Platform::default())
}

/// `compile` for a browser build — the platform whose layer holds `std::ui` /
/// `std::dom` / `std::router`, none of which the default (node) platform can
/// import.
pub fn compile_browser(source: &str) -> Result<String, Vec<String>> {
    compile_on(source, Platform::Browser)
}

pub fn compile_on(source: &str, platform: Platform) -> Result<String, Vec<String>> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let leaked: &'static str = Box::leak(source.into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    Path::new("."),
                    Path::new("test.vl"),
                    Some(platform),
                    &Workspace::default(),
                );
                match program {
                    Some(program) if errors.is_empty() => {
                        transform(&program, &BuildOptions::default())
                            .map_err(|error| vec![error.msg])
                    }
                    _ => Err(errors.into_iter().map(|error| error.msg).collect()),
                }
            }))
            .unwrap_or_else(|_| Err(vec!["compiler panicked".to_string()]))
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| {
            Err(vec![
                "compiler thread aborted (likely a stack overflow)".to_string(),
            ])
        })
}

/// Compile a browser program with the HMR instrumentation flag set to `hmr`,
/// returning the emitted JS. `hmr = true` is the `run --watch` browser path; the
/// `false` arm must be byte-identical to a normal `compile_browser`.
pub fn compile_browser_with_hmr(source: &str, hmr: bool) -> Result<String, Vec<String>> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let leaked: &'static str = Box::leak(source.into_boxed_str());
                let (program, errors) = analyze_source(
                    leaked,
                    &std_spec(),
                    Path::new("."),
                    Path::new("test.vl"),
                    Some(Platform::Browser),
                    &Workspace::default(),
                );
                match program {
                    Some(program) if errors.is_empty() => {
                        let mut options = BuildOptions::default();
                        options.hmr = hmr;
                        transform(&program, &options).map_err(|error| vec![error.msg])
                    }
                    _ => Err(errors.into_iter().map(|error| error.msg).collect()),
                }
            }))
            .unwrap_or_else(|_| Err(vec!["compiler panicked".to_string()]))
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err(vec!["compiler thread aborted".to_string()]))
}

#[track_caller]
pub fn compile_hmr(source: &str) -> String {
    match compile_browser_with_hmr(source, true) {
        Ok(js) => js,
        Err(errors) => panic!("expected a clean HMR compile, got: {errors:#?}"),
    }
}

/// The djb2 fingerprint the HMR instrumentation stamped for `key`, read out of the
/// emitted `__hmr_adopt*("<key>", <fp>, ...)` (or expose) call.
pub fn hmr_fingerprint(js: &str, key: &str) -> Option<u32> {
    let needle = format!("\"{key}\", ");
    let start = js.find(&needle)? + needle.len();
    let rest = &js[start..];
    let end = rest.find(|c: char| !c.is_ascii_digit())?;
    rest[..end].parse().ok()
}

#[track_caller]
pub fn assert_compiles(source: &str) {
    if let Err(errors) = compile(source) {
        panic!("expected a clean compile, got: {errors:#?}");
    }
}

#[track_caller]
pub fn assert_fails(source: &str) {
    assert!(
        compile(source).is_err(),
        "expected a compile error, but it compiled cleanly"
    );
}

/// Asserts compilation fails with a diagnostic containing `message_part` — like
/// [`assert_fails`] but pinning *which* error, so a test can't pass on an
/// unrelated failure.
#[track_caller]
pub fn assert_fails_with(source: &str, message_part: &str) {
    match compile(source) {
        Ok(_) => panic!("expected a compile error, but it compiled cleanly"),
        Err(errors) => assert!(
            errors.iter().any(|error| error.contains(message_part)),
            "no diagnostic contains {message_part:?}; got: {errors:#?}"
        ),
    }
}

/// Asserts compilation fails with EXACTLY ONE diagnostic containing
/// `message_part` — for a rule whose reach is wide enough that the multiplicity
/// is part of the claim (B103: one inferred `List<Guard>` reaches R10 as the
/// literal, the binding, every read of it, and the aggregate holding it, and it
/// is one mistake).
#[track_caller]
pub fn assert_fails_once_with(source: &str, message_part: &str) {
    match compile(source) {
        Ok(_) => panic!("expected a compile error, but it compiled cleanly"),
        Err(errors) => {
            let matching = errors
                .iter()
                .filter(|error| error.contains(message_part))
                .count();
            assert_eq!(
                matching, 1,
                "expected exactly one diagnostic containing {message_part:?}; got: {errors:#?}"
            );
        }
    }
}

/// Asserts compilation fails and that NO diagnostic mentions `message_part` —
/// for a fix whose point is that a misleading message is gone, not merely that
/// a better one was added beside it.
#[track_caller]
pub fn assert_fails_without(source: &str, message_part: &str) {
    match compile(source) {
        Ok(_) => panic!("expected a compile error, but it compiled cleanly"),
        Err(errors) => assert!(
            errors.iter().all(|error| !error.contains(message_part)),
            "a diagnostic still contains {message_part:?}; got: {errors:#?}"
        ),
    }
}

#[track_caller]
pub fn assert_compiles_browser(source: &str) {
    if let Err(errors) = compile_browser(source) {
        panic!("expected a clean browser compile, got: {errors:#?}");
    }
}

/// Asserts a browser compile fails with a diagnostic containing `message_part`.
#[track_caller]
pub fn assert_fails_browser_with(source: &str, message_part: &str) {
    match compile_browser(source) {
        Ok(_) => panic!("expected a browser compile error, but it compiled cleanly"),
        Err(errors) => assert!(
            errors.iter().any(|error| error.contains(message_part)),
            "no browser diagnostic contains {message_part:?}; got: {errors:#?}"
        ),
    }
}

/// The analyzer's diagnostics as `(message, span range)` pairs — the E7 span
/// harness's raw material (`compile` keeps only the messages).
pub fn failure_diagnostics(source: &str) -> Vec<(String, std::ops::Range<usize>)> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (_program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            errors
                .into_iter()
                .map(|error| (error.msg, error.span.into_range()))
                .collect()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Asserts compilation fails with a diagnostic whose message contains
/// `message_part` and whose span covers exactly the first occurrence of
/// `spanning` in the source — spans pin like messages (backlog E7). The
/// distinct `spanning` snippet locates the *pertinent* expression, so a
/// diagnostic that regresses to an enclosing aggregate span fails here.
#[track_caller]
/// Like `failure_diagnostics`, but keeps each diagnostic's secondary note
/// (diagnostics-standard.md C3).
pub fn failure_diagnostics_with_notes(
    source: &str,
) -> Vec<(
    String,
    std::ops::Range<usize>,
    Option<(String, std::ops::Range<usize>, bool)>,
)> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (_program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            errors
                .into_iter()
                .map(|error| {
                    (
                        error.msg,
                        error.span.into_range(),
                        error
                            .note
                            .map(|note| (note.msg, note.span.into_range(), note.source.is_some())),
                    )
                })
                .collect()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Asserts a diagnostic whose message contains `message_part` carries a
/// secondary NOTE anchored at `note_spanning`'s occurrence in the source,
/// with a note message containing `note_part` (diagnostics-standard.md C2/C3).
pub fn assert_fails_noting(source: &str, message_part: &str, note_spanning: &str, note_part: &str) {
    let expected_start = source
        .find(note_spanning)
        .expect("the `note_spanning` snippet must occur in the source");
    let expected = expected_start..expected_start + note_spanning.len();
    let diagnostics = failure_diagnostics_with_notes(source);
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _, _)| message.contains(message_part))
        .collect();
    assert!(
        !matching.is_empty(),
        "no diagnostic contains {message_part:?}; got: {diagnostics:#?}"
    );
    assert!(
        matching.iter().any(|(_, _, note)| note
            .as_ref()
            .is_some_and(|(msg, range, _)| msg.contains(note_part) && *range == expected)),
        "no {message_part:?} diagnostic notes {note_part:?} at {expected:?} ({note_spanning:?}); got: {matching:#?}"
    );
}

/// `assert_fails_noting` for the R11 family's shape: the error is at the
/// instantiation site in this program and the note points into the std body
/// being instantiated. The note's span is therefore an offset into a file this
/// test never wrote, so it is checked by its `source` marker (present ⇒ a
/// different file) and its text, not a range.
///
/// It also asserts the program produces exactly ONE diagnostic, which is the
/// half that makes it a pin rather than a restatement: the value that genuinely
/// cannot be handled must be the only thing reported, with no leading
/// distraction about the receiver (B63).
#[track_caller]
pub fn assert_only_failure_noting_into_std(source: &str, message_part: &str, note_part: &str) {
    let diagnostics = failure_diagnostics_with_notes(source);
    assert_eq!(
        diagnostics.len(),
        1,
        "expected exactly one diagnostic; got: {diagnostics:#?}"
    );
    let (message, _, note) = &diagnostics[0];
    assert!(
        message.contains(message_part),
        "the diagnostic does not contain {message_part:?}: {message:?}"
    );
    assert!(
        note.as_ref().is_some_and(
            |(msg, _, from_other_source)| msg.contains(note_part) && *from_other_source
        ),
        "the diagnostic does not note {note_part:?} in another source; got: {note:#?}"
    );
}

/// `assert_fails_noting`, but the note spans the Nth occurrence (0-based) of
/// `note_spanning` — for notes that point at a declaration the diagnosed use
/// necessarily precedes (use-before-declaration's declared-later note).
#[track_caller]
pub fn assert_fails_noting_nth(
    source: &str,
    message_part: &str,
    note_spanning: &str,
    occurrence: usize,
    note_part: &str,
) {
    let mut start = 0;
    let mut at = None;
    for _ in 0..=occurrence {
        at = source[start..]
            .find(note_spanning)
            .map(|found| start + found);
        match at {
            Some(position) => start = position + 1,
            None => panic!("occurrence {occurrence} of {note_spanning:?} not found"),
        }
    }
    let expected_start = at.unwrap();
    let expected = expected_start..expected_start + note_spanning.len();
    let diagnostics = failure_diagnostics_with_notes(source);
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _, _)| message.contains(message_part))
        .collect();
    assert!(
        !matching.is_empty(),
        "no diagnostic contains {message_part:?}; got: {diagnostics:#?}"
    );
    assert!(
        matching.iter().any(|(_, _, note)| note
            .as_ref()
            .is_some_and(|(msg, range, _)| msg.contains(note_part) && *range == expected)),
        "no {message_part:?} diagnostic notes {note_part:?} at {expected:?} (occurrence {occurrence} of {note_spanning:?}); got: {matching:#?}"
    );
}

/// Like [`failure_diagnostics_with_notes`], but keeping each diagnostic's
/// E78 requirement trace: `(message, span, trace)` with one
/// `(label message, span range, cross-source?)` entry per hop, in the
/// analyzer's own order — entry → read.
pub fn failure_diagnostics_with_trace(
    source: &str,
) -> Vec<(
    String,
    std::ops::Range<usize>,
    Vec<(String, std::ops::Range<usize>, bool)>,
)> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (_program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            errors
                .into_iter()
                .map(|error| {
                    (
                        error.msg,
                        error.span.into_range(),
                        error
                            .trace
                            .into_iter()
                            .map(|hop| {
                                (
                                    hop.note.msg,
                                    hop.note.span.into_range(),
                                    hop.note.source.is_some(),
                                )
                            })
                            .collect(),
                    )
                })
                .collect()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// Asserts the ONE diagnostic containing `message_part` carries EXACTLY the
/// expected requirement trace (backlog E78): `expected` lists, in order
/// (entry → read), each label's span as (snippet, 0-based occurrence in the
/// source) plus a fragment of its message. Exactness is the pin: an extra
/// label — a covered call taking blame, a hop past the cap — fails here as
/// surely as a missing one.
#[track_caller]
pub fn assert_traces(source: &str, message_part: &str, expected: &[(&str, usize, &str)]) {
    let occurrence_span = |snippet: &str, occurrence: usize| -> std::ops::Range<usize> {
        let mut start = 0;
        let mut at = None;
        for _ in 0..=occurrence {
            at = source[start..].find(snippet).map(|found| start + found);
            match at {
                Some(position) => start = position + 1,
                None => panic!("occurrence {occurrence} of {snippet:?} not found"),
            }
        }
        let found = at.unwrap();
        found..found + snippet.len()
    };
    let diagnostics = failure_diagnostics_with_trace(source);
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _, _)| message.contains(message_part))
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "expected exactly one diagnostic containing {message_part:?}; got: {diagnostics:#?}"
    );
    let (_, _, trace) = matching[0];
    assert_eq!(
        trace.len(),
        expected.len(),
        "the trace must carry exactly the expected labels; got: {trace:#?}"
    );
    for (index, ((snippet, occurrence, label_part), (label, range, cross_source))) in
        expected.iter().zip(trace).enumerate()
    {
        let expected_range = occurrence_span(snippet, *occurrence);
        assert!(
            label.contains(label_part),
            "trace[{index}] message {label:?} lacks {label_part:?}"
        );
        assert_eq!(
            *range, expected_range,
            "trace[{index}] must span occurrence {occurrence} of {snippet:?}"
        );
        assert!(
            !cross_source,
            "trace[{index}] unexpectedly points into another file"
        );
    }
}

/// `assert_fails_spanning`, but targeting the Nth occurrence (0-based) of
/// `spanning` — for snippets that necessarily appear earlier in another
/// role (an attribute name also being the macro definition's, a use after
/// its declaration).
pub fn assert_fails_spanning_nth(
    source: &str,
    spanning: &str,
    occurrence: usize,
    message_part: &str,
) {
    let mut start = 0;
    let mut at = None;
    for _ in 0..=occurrence {
        at = source[start..].find(spanning).map(|found| start + found);
        match at {
            Some(position) => start = position + 1,
            None => panic!("occurrence {occurrence} of {spanning:?} not found"),
        }
    }
    let expected_start = at.unwrap();
    let expected = expected_start..expected_start + spanning.len();
    let diagnostics = failure_diagnostics(source);
    assert!(
        diagnostics
            .iter()
            .any(|(message, range)| message.contains(message_part) && *range == expected),
        "no {message_part:?} diagnostic spans occurrence {occurrence} of {spanning:?} at {expected:?}; got: {diagnostics:#?}"
    );
}

pub fn assert_fails_spanning(source: &str, spanning: &str, message_part: &str) {
    let expected_start = source
        .find(spanning)
        .expect("the `spanning` snippet must occur in the source");
    let expected = expected_start..expected_start + spanning.len();
    let diagnostics = failure_diagnostics(source);
    let matching: Vec<_> = diagnostics
        .iter()
        .filter(|(message, _)| message.contains(message_part))
        .collect();
    assert!(
        !matching.is_empty(),
        "no diagnostic contains {message_part:?}; got: {diagnostics:#?}"
    );
    assert!(
        matching.iter().any(|(_, range)| *range == expected),
        "no {message_part:?} diagnostic spans {expected:?} ({spanning:?}); spans: {:#?}",
        matching
            .iter()
            .map(|(message, range)| (message.as_str(), range.clone(), &source[range.clone()]))
            .collect::<Vec<_>>()
    );
}

/// The analyzer's non-fatal warning messages (e.g. unused `[must_use]` results).
pub fn warnings(source: &str) -> Vec<String> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, _errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            program
                .map(|program| {
                    program
                        .warnings
                        .into_iter()
                        .map(|warning| warning.msg)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_default()
}

/// [`warnings`] with each warning's span, through a CLEAN analysis against the
/// given std: panics unless analysis produced a program with zero diagnostics
/// (a warning that rides an error is not the non-fatal path), then returns
/// `(message, span)` per warning in the analyzer's own (C1-sorted) order.
pub fn warning_diagnostics_with_std(
    source: &str,
    std: PackageSpec,
) -> Vec<(String, std::ops::Range<usize>)> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std,
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            program
                .expect("analysis should produce a program")
                .warnings
                .into_iter()
                .map(|warning| (warning.msg, warning.span.into_range()))
                .collect::<Vec<_>>()
        })
        .expect("spawn worker")
        .join()
        .unwrap()
}

/// [`warning_diagnostics_with_std`] against the real std.
pub fn warning_diagnostics(source: &str) -> Vec<(String, std::ops::Range<usize>)> {
    warning_diagnostics_with_std(source, std_spec())
}

/// The warning twin of [`assert_fails_spanning`] (deprecation.md §1's C2 pin):
/// asserts a warning containing `message_part` spans the FIRST occurrence of
/// `spanning`, and — through [`warning_diagnostics`] — that the analysis was
/// clean and produced a program.
#[track_caller]
pub fn assert_warns_spanning(source: &str, spanning: &str, message_part: &str) {
    let expected_start = source
        .find(spanning)
        .expect("the `spanning` snippet must occur in the source");
    let expected = expected_start..expected_start + spanning.len();
    let warnings = warning_diagnostics(source);
    let matching: Vec<_> = warnings
        .iter()
        .filter(|(message, _)| message.contains(message_part))
        .collect();
    assert!(
        !matching.is_empty(),
        "no warning contains {message_part:?}; got: {warnings:#?}"
    );
    assert!(
        matching.iter().any(|(_, range)| *range == expected),
        "no {message_part:?} warning spans {expected:?} ({spanning:?}); spans: {:#?}",
        matching
            .iter()
            .map(|(message, range)| (message.as_str(), range.clone(), &source[range.clone()]))
            .collect::<Vec<_>>()
    );
}

/// The rendered per-function requirement line (`platform_color::requirements`
/// — the hover's data) for the named function, through the real pipeline on
/// the default platform. `None` = the function is colorless. Panics on
/// analysis errors or an unknown name, so a pin can't pass vacuously.
pub fn requirement_line_of(source: &str, function_name: &str) -> Option<String> {
    let source = source.to_string();
    let function_name = function_name.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let messages: Vec<String> = errors.into_iter().map(|error| error.msg).collect();
            assert!(
                messages.is_empty(),
                "expected a clean analysis, got: {messages:#?}"
            );
            let program = program.expect("analysis should produce a program");
            let function_id = program
                .functions
                .iter()
                .find(|(_, function)| function.name == function_name.as_str())
                .map(|(id, _)| *id)
                .or_else(|| {
                    // A layer function may be a bodiless extern (e.g.
                    // `std::fs::write_file`), seeded exactly like one with a body.
                    program
                        .external_functions
                        .iter()
                        .find(|(_, function)| function.name == function_name.as_str())
                        .map(|(id, _)| *id)
                })
                .or_else(|| {
                    // A module-level binding: its initializer is code, so it
                    // carries a requirement line like a function does.
                    program
                        .variables
                        .iter()
                        .find(|(_, variable)| variable.name == function_name.as_str())
                        .map(|(id, _)| *id)
                })
                .unwrap_or_else(|| panic!("no function or binding named `{function_name}`"));
            vilan_core::platform_color::requirements(&program)
                .get(&function_id)
                .cloned()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// Execute already-compiled JS with `node`, returning its stdout on a clean
/// exit or the stderr lines otherwise. Split out of `compile_and_run` (E32)
/// so a caller that needs to bound wall clock can time only the RUN: the
/// `compile` step above re-analyzes all of `std` in-process on every call
/// and can itself take seconds under nextest's full parallelism, which must
/// not count against a budget that is really about the emitted program's
/// own behavior.
pub fn run_js(js: &str) -> Result<String, Vec<String>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    // `.mjs`, exactly as the CLI's `run`/`test`/watch scripts: a process
    // runtime classifies before it parses, and a harness that ran its bundles
    // as CommonJS could not see an ESM-only defect at all
    // (`top-level-await.md` §8.1).
    let path = std::env::temp_dir().join(format!("vilan_test_{}_{unique}.mjs", std::process::id()));
    std::fs::write(&path, js).map_err(|error| vec![error.to_string()])?;
    let output = std::process::Command::new("node").arg(&path).output();
    let _ = std::fs::remove_file(&path);
    match output {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        }
        Ok(output) => Err(vec![String::from_utf8_lossy(&output.stderr).into_owned()]),
        Err(error) => Err(vec![format!("could not run node: {error}")]),
    }
}

/// Compile and run, returning `(stdout, stderr, exit code)` whatever the exit —
/// for pinning the entry shim's failure contract (J6), where the exit CODE and
/// what reached stderr are the claims, not the stdout.
pub fn compile_and_run_status(source: &str) -> (String, String, i32) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let javascript = compile(source).expect("expected a clean compile");
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("vilan_exit_{}_{unique}.mjs", std::process::id()));
    std::fs::write(&path, javascript).expect("write script");
    let output = std::process::Command::new("node")
        .arg(&path)
        .output()
        .expect("run node");
    let _ = std::fs::remove_file(&path);
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

/// Compile, then execute the emitted JS with `node`, returning its stdout. A
/// compile failure or a non-zero exit becomes `Err`. This catches *runtime*
/// miscompiles — a program that type-checks but emits the wrong code (e.g. a
/// generic dispatch that resolves to `undefined`) — which `assert_compiles`
/// alone cannot see.
pub fn compile_and_run(source: &str) -> Result<String, Vec<String>> {
    run_js(&compile(source)?)
}

/// `compile_and_run`, but timing only the RUN (E32): `compile` happens
/// first and is excluded from the returned `Duration`. For claims about the
/// emitted PROGRAM's own runtime behavior (a cancellation reacting inside
/// some window), the harness's compile step is noise — it reruns `std`
/// analysis in-process and can itself run to several seconds under load,
/// which used to be folded into (and starve) these tests' timing budget.
pub fn compile_and_run_timed(source: &str) -> (Result<String, Vec<String>>, std::time::Duration) {
    match compile(source) {
        Ok(js) => {
            let started = std::time::Instant::now();
            let result = run_js(&js);
            (result, started.elapsed())
        }
        Err(errors) => (Err(errors), std::time::Duration::ZERO),
    }
}

#[track_caller]
pub fn assert_compiles_and_runs(source: &str, expected_stdout: &str) {
    match compile_and_run(source) {
        Ok(stdout) => assert_eq!(stdout, expected_stdout, "stdout mismatch"),
        Err(errors) => panic!("expected a clean run, got: {errors:#?}"),
    }
}

/// `assert_compiles_and_runs`, bounding only the RUN's wall clock (E32):
/// compile happens first, untimed, so the budget measures the emitted
/// program's own behavior rather than the harness's (load-sensitive)
/// compile step.
#[track_caller]
pub fn assert_runs_within(source: &str, expected_stdout: &str, budget: std::time::Duration) {
    let (outcome, elapsed) = compile_and_run_timed(source);
    match outcome {
        Ok(stdout) => assert_eq!(stdout, expected_stdout, "stdout mismatch"),
        Err(errors) => panic!("expected a clean run, got: {errors:#?}"),
    }
    assert!(
        elapsed < budget,
        "the run alone (compile excluded) took {elapsed:?}, budget was {budget:?}"
    );
}

/// Like `compile_and_run`, but a ZERO-exit run yields `(stdout, stderr)` — for
/// pinning what a program reports while CONTINUING (the unobserved
/// task-failure report goes to stderr; the process does not crash).
pub fn compile_and_run_capturing_stderr(source: &str) -> Result<(String, String), Vec<String>> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);

    let js = compile(source)?;
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("vilan_task_{}_{unique}.mjs", std::process::id()));
    std::fs::write(&path, js).map_err(|error| vec![error.to_string()])?;
    let output = std::process::Command::new("node").arg(&path).output();
    let _ = std::fs::remove_file(&path);
    match output {
        Ok(output) if output.status.success() => Ok((
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )),
        Ok(output) => Err(vec![String::from_utf8_lossy(&output.stderr).into_owned()]),
        Err(error) => Err(vec![format!("could not run node: {error}")]),
    }
}

/// Compiles and runs `source`, asserting the run FAILS and its stderr mentions
/// `expected_in_stderr` — the shape of a runtime panic. (A compile failure also
/// arrives as `Err`, but its messages won't contain a panic string, so the
/// substring assert distinguishes the two.)
#[track_caller]
pub fn assert_run_panics(source: &str, expected_in_stderr: &str) {
    match compile_and_run(source) {
        Ok(stdout) => panic!(
            "expected a runtime panic mentioning {expected_in_stderr:?}, got a clean run: {stdout:?}"
        ),
        Err(errors) => {
            let combined = errors.join("\n");
            assert!(
                combined.contains(expected_in_stderr),
                "the failure does not mention {expected_in_stderr:?}:\n{combined}"
            );
        }
    }
}

/// Compiles `source` and asserts the emitted JS contains `needle` — the
/// serialized-literal check for const results.
#[track_caller]
pub fn assert_emits_containing(source: &str, needle: &str) {
    match compile(source) {
        Ok(js) => assert!(
            js.contains(needle),
            "emitted JS does not contain {needle:?}:\n{js}"
        ),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
}

/// The `(kind, line)` assets a program's const evaluation emitted.
pub fn collected_assets(source: &str) -> Vec<(String, String)> {
    let source = source.to_string();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(errors.is_empty(), "expected a clean analysis: {errors:#?}");
            program.map(|p| p.const_assets).unwrap_or_default()
        })
        .unwrap()
        .join()
        .unwrap()
}

/// How many times `needle` appears in a clean compile's emitted JS.
#[track_caller]
pub fn emitted_occurrences(source: &str, needle: &str) -> usize {
    match compile(source) {
        Ok(js) => js.matches(needle).count(),
        Err(errors) => panic!("expected a clean compile, got: {errors:#?}"),
    }
}

/// The R11 "not move-clean" diagnostics for `source`, each as
/// `(message, primary range, note)`.
pub fn r11_rejections(
    source: &str,
) -> Vec<(
    String,
    std::ops::Range<usize>,
    Option<(String, std::ops::Range<usize>, bool)>,
)> {
    failure_diagnostics_with_notes(source)
        .into_iter()
        .filter(|(message, _, _)| {
            message.contains("is not move-clean when instantiated with a resource")
        })
        .collect()
}
