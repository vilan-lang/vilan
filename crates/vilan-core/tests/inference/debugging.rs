//! debugging.md (E257): `[track_caller]` and panic locations (S0), `dbg(..)`
//! and its printer (S1, S1b), `dbg_stack()` (S2) and `Debug` through the
//! printer (S4). The run-time output of each is ALSO pinned on the native
//! backend, byte for byte, in `vilan-cli`'s `native_differential`; these pins
//! own the analyzer's verdicts and the JS emission.

use super::support::*;

// --- S0: `[track_caller]` -------------------------------------------------

/// A trait member is reached by dispatch, where no call site knows it is
/// calling a tracking function, so the attribute is refused there — on the
/// trait's declaration and on an implementation's member alike.
#[test]
fn s0_track_caller_is_refused_on_a_trait_method() {
    assert_fails_with(
        "trait Probe {\n\t[track_caller]\n\tfun probe(self): i32;\n}\nfun main() {}\n",
        "`[track_caller]` is not supported on a trait method",
    );
    assert_fails_with(
        "trait Probe {\n\tfun probe(self): i32;\n}\nstruct P {}\nimpl P with Probe {\n\t[track_caller]\n\tfun probe(self): i32 { 1 }\n}\nfun main() { print(P {}.probe()); }\n",
        "`[track_caller]` is not supported on a trait method",
    );
}

/// A tracking function taken as a VALUE is refused: a call through the value
/// cannot know to pass a location, and passing none would print nothing where
/// the report promises a site.
#[test]
fn s0_a_tracking_function_taken_as_a_value_is_refused() {
    assert_fails_with(
        "[track_caller]\nfun check(value: i32): i32 { value }\nfun main() {\n\tlet f = check;\n\tprint(f(1));\n}\n",
        "`check` is `[track_caller]`: it takes its caller's location as a hidden argument",
    );
}

/// An inherent method and a free function both take the attribute, and the
/// hidden parameter is invisible to the caller: the arity is the written one.
#[test]
fn s0_track_caller_keeps_the_written_arity() {
    assert_compiles_and_runs(
        "import std::debug::caller;\nstruct Probe {}\nimpl Probe {\n\t[track_caller]\n\tfun at(self, label: str): str { label + \" \" + caller().text() }\n}\n[track_caller]\nfun site(): str { caller().text() }\nfun main() {\n\tprint(site());\n\tprint(Probe {}.at(\"method\"));\n}\n",
        "test.vl:10:8\nmethod test.vl:11:17\n",
    );
}

/// The panic the JS backend throws is an `Error` whose header is the report
/// line, so an uncaught one prints `panicked at <site>: <message>` and exits
/// non-zero; `assert` names the line that called it.
#[test]
fn s0_an_uncaught_panic_prints_its_site_on_js() {
    let (_, stderr, code) = compile_and_run_status(
        "import std::io::assert;\nfun check(n: i32) {\n\tassert(n > 3, \"n too small\");\n}\nfun main() { check(1); }\n",
    );
    assert_eq!(code, 1);
    assert!(
        stderr
            .lines()
            .any(|line| line == "panicked at test.vl:3:2: n too small"),
        "stderr: {stderr}"
    );
}
