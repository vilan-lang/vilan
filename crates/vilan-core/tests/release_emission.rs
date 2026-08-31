//! The release preset's emission path, pinned per defect.
//!
//! `preset = "release"` changes two things about the emitted JavaScript — what
//! the identifiers are called and how tightly the tokens are packed — and until
//! E36's differential (`release_differential.rs`) existed, neither was gated over
//! anything but one fixture. Both turned out to be broken. The renaming defect
//! (B69) is below; the tight-printing one the differential found on its first
//! run is at the end of the file.
//!
//! # B69 — the short-name renaming must be collision-free
//!
//! `preset = "release"` renames every binding to the shortest identifier free
//! in its JavaScript scope (`transformer.rs`, "Scope-aware name allocation").
//! That pass hands out names from `a, b, c, …` while leaving alone every name it
//! was not asked to re-allocate — and the names it left alone came out of the
//! generator's OWN `a, b, c, …` sequence. The two pools were the same alphabet,
//! so the pass reissued names that were already in use and the emitted program
//! declared one identifier twice. Seven corpus programs miscompiled this way on
//! the shipped v0.27.0 binary, with no `const` inference involved.
//!
//! One defect, four surface shapes — a redeclaration is a `SyntaxError`, a
//! shadowed function is silent infinite recursion, a read before the shadowing
//! declaration is a TDZ `ReferenceError` — so each is pinned on its own distilled
//! program rather than on the corpus-wide gate that found them
//! (`release_differential.rs`). Every pin runs the program under node, because
//! that is the only instrument that can tell "compiles" from "does the right
//! thing": the two-`function b` shape emits perfectly valid JavaScript.
//!
//! Each pin is stated as an EQUIVALENCE — debug and release must print the same
//! thing — so it keeps meaning if the allocator's letters ever shift.

use std::path::{Path, PathBuf};

use vilan_core::options::{BuildOptions, Preset};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source, transform};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// Compiles `source` under `options` on a large-stack worker (matching the CLI),
/// running the `const` inference sweep exactly when the options ask for it —
/// the same wiring `vilan build` uses, so a pin sees what a user's build sees.
fn compile(source: &str, options: BuildOptions) -> Result<String, String> {
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
            let mut program = match program {
                Some(program) if errors.is_empty() => program,
                _ => return Err(format!("compile failed: {errors:?}")),
            };
            program
                .const_results
                .extend(vilan_core::const_eval::infer(&program, &options));
            transform(&program, &options).map_err(|error| error.msg)
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err("worker thread aborted".to_string()))
}

/// Runs `javascript` under node, returning `(stdout, exit code)`. stderr is
/// folded into the failure text so a `SyntaxError` names itself.
fn run(javascript: &str, label: &str) -> (String, i32) {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "vilan_release_names_{}_{unique}_{label}.mjs",
        std::process::id()
    ));
    std::fs::write(&path, javascript).expect("write scratch program");
    let output = std::process::Command::new("node")
        .arg(&path)
        .output()
        .expect("run node");
    let _ = std::fs::remove_file(&path);
    let code = output.status.code().unwrap_or(-1);
    if code != 0 {
        return (
            format!(
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
            code,
        );
    }
    (String::from_utf8_lossy(&output.stdout).into_owned(), code)
}

/// The pin every case below is stated as: the release build of `source` runs,
/// and prints exactly what the debug build prints. Renaming is observationally
/// invisible or it is a miscompile — there is no third option.
///
/// Release is checked in BOTH of its shipped configurations, with the `const`
/// inference sweep on (the preset's default) and with `infer-const = false` (the
/// documented override, and exactly the release path v0.27.0 shipped). Neither
/// subsumes the other for a renaming bug: folding tree-shakes functions away and
/// so shifts every later allocation, which both HIDES collisions — the
/// two-`function b` shape folds to a literal under the sweep — and exposes new
/// ones. A pin written against one configuration only would be silently vacuous
/// in half the cases here.
#[track_caller]
fn assert_release_matches_debug(source: &str, expected_stdout: &str) {
    let mut without_sweep = BuildOptions::from_preset(Preset::Release);
    without_sweep.infer_const = false;

    for (label, options) in [
        ("debug", BuildOptions::from_preset(Preset::Debug)),
        ("release", BuildOptions::from_preset(Preset::Release)),
        ("release_no_sweep", without_sweep),
    ] {
        let emitted = compile(source, options)
            .unwrap_or_else(|error| panic!("the {label} build failed: {error}"));
        let (stdout, code) = run(&emitted, label);
        assert_eq!(
            (stdout.as_str(), code),
            (expected_stdout, 0),
            "the {label} build does not print what the source says.\nemitted:\n{emitted}"
        );
    }
}

/// Shape 1 — TWO MODULE-LEVEL `function b`, the `default.vl` failure. A generic
/// (`default<T>`) is monomorphized into an anonymous instance function, whose
/// name the generator minted directly rather than through an id; the pass never
/// saw it, and handed its letter to the `Default::default` body beside it. The
/// emitted JavaScript is VALID — the second declaration wins and calls itself —
/// so nothing but running it can catch this. Pre-fix: `RangeError: Maximum call
/// stack size exceeded`.
#[test]
fn two_module_level_functions_never_share_a_name() {
    assert_release_matches_debug(
        r#"
import std::{ io::print, number::u32 };

trait Default {
	fun default(): Self;
}

struct Id {
	n: u32
}

impl Id {
	fun new(n: u32) {
		Id { n }
	}
}

impl Id with Default {
	fun default() {
		Id::new(0)
	}
}

fun default<T: Default>(): T {
	T::default()
}

fun main() {
	let some_id = default<Id>();
	print(some_id);
}
"#,
        "[ 0 ]\n",
    );
}

/// Shape 2 — a monomorphized instance against a MODULE-LEVEL BINDING, the
/// `list-element-type.vl` failure. `sum` and `product` instantiate into two
/// unseen module-level functions and `numbers` is allocated a letter one of them
/// already holds. Pre-fix: `SyntaxError: Identifier 'b' has already been
/// declared`.
#[test]
fn a_generic_instance_never_collides_with_a_module_binding() {
    assert_release_matches_debug(
        r#"
import std::io::print;

fun main() {
	mut numbers = List::new();
	numbers.push(2);
	numbers.push(3);
	print(numbers.sum());
	print(numbers.product());
}
"#,
        "5\n6\n",
    );
}

/// Shape 3 — the collision INSIDE a function, the `capture-clones.vl` failure.
/// A `match` in a loop body emits a subject temp and a result temp; both were
/// invisible, and the pattern's own captures were then allocated the same two
/// letters in the same block. Pre-fix: `SyntaxError: Identifier 'f' has already
/// been declared`.
#[test]
fn a_match_temp_never_collides_inside_its_own_function() {
    assert_release_matches_debug(
        r#"
import std::io::print;

fun total_width(rows: List<(List<i32>, i32)>): i32 {
	mut total = 0;
	for row in rows {
		match row {
			(let cells, let weight) => {
				total = total + cells.len() * weight;
			}
		}
	}
	total
}

fun main() {
	mut rows = List::new();
	rows.push(([1, 2], 3));
	rows.push(([4], 1));
	print(total_width(rows));
}
"#,
        "7\n",
    );
}

/// Shape 4 — a module-level TEMP against a module-level binding, the
/// `iterator-protocol.vl` failure. Driving a custom iterator emits a `const` for
/// the iterable at module level; `numbers` was allocated its letter. Pre-fix:
/// `SyntaxError: Identifier 'f' has already been declared`.
#[test]
fn a_module_level_temp_never_redeclares_a_module_binding() {
    assert_release_matches_debug(
        r#"
import std::option::Option::{ self, Some, None };
import std::iterator::Iterator;
import std::io::print;

mut produced = 0;

struct Naturals { limit: i32 }

impl Naturals with Iterator<i32> {
	fun next(&mut self): Option<i32> {
		produced = produced + 1;
		if produced <= self.limit {
			Some(produced)
		} else {
			None
		}
	}
}

fun main() {
	let naturals = Naturals { limit = 3 };
	for n in naturals {
		print(n);
	}

	mut numbers = List::new();
	numbers.push(2);
	numbers.push(3);
	numbers.push(4);
	print(numbers.sum());
}
"#,
        "1\n2\n3\n9\n",
    );
}

/// Shape 5 — the one `const` INFERENCE exposed rather than caused, the
/// `json-roundtrip.vl` failure (const-eval.md §9.7). This program was clean under
/// release with the sweep OFF and broken with it on: folding `twice(3)`
/// tree-shakes the function away, which shifts every later allocation by one
/// letter and lands one of them on an invisible temp. The sweep does not create
/// the defect, it moves which programs trip it — which is why the helper above
/// checks both configurations, and why this case is worth its own name. Pre-fix:
/// `ReferenceError: Cannot access 'c' before initialization` — the TDZ shape, a
/// read of a name in the initializer of the declaration that shadows it.
#[test]
fn an_inference_fold_cannot_expose_a_collision() {
    assert_release_matches_debug(
        r#"
import std::io::print;
import std::option::Option::{ self, Some, None };

fun twice(n: i32): i32 {
	n * 2
}

fun main() {
	let base = twice(3);
	let held: Option<i32> = Some(base);
	match held {
		Some(let v) => print(v),
		None => print(0),
	}
}
"#,
        "6\n",
    );
}

// --- Tight printing ---------------------------------------------------------

/// The second defect the ungated release path was hiding, found by
/// `release_differential.rs` on its first run (`unary-minus.vl`). Release drops
/// the padding around operators, and `3 - -2` — a subtraction of a unary
/// negation — printed tight as `3--(2)`, which JavaScript lexes as a postfix
/// `--` and refuses to parse at all: `SyntaxError: Invalid left-hand side
/// expression in postfix operation`.
///
/// The rule the fix states is that dropping padding may not change the TOKEN
/// stream, so the cases here are the ones where the junction can fuse and the
/// ones where it must stay tight — a `between` that always emitted a space
/// would pass the first three assertions and quietly stop minifying.
#[test]
fn tight_printing_never_fuses_two_operators_into_one_token() {
    assert_release_matches_debug(
        r#"
import std::io::print;

fun main() {
	let n = 2;
	print(3 - -2);
	print(3 - -n);
	print(3 - -(-2));
	print(7 - 9);
	print(3 * -2);
}
"#,
        "5\n5\n1\n-2\n-6\n",
    );

    // And the packing is still tight where nothing would fuse: the fix must buy
    // its correctness with one space at one junction, not with padding
    // everywhere.
    let emitted = compile(
        "import std::io::print;\n\nfun main() {\n\tprint(7 - 9);\n\tprint(3 - -2);\n}\n",
        BuildOptions::from_preset(Preset::Release),
    )
    .expect("the release build");
    assert!(
        emitted.contains("7-9"),
        "an ordinary subtraction must still print tight:\n{emitted}"
    );
    assert!(
        emitted.contains("3- -(2)"),
        "the fusing junction takes exactly one space:\n{emitted}"
    );
}

#[test]
fn a_loop_condition_is_reevaluates_under_release() {
    // B136 (`proposal/markdown.md` §10.7): an `is` in a loop condition
    // compiled against a subject temp hoisted BEFORE the `while`, so body
    // reassignments never reached the condition — 3 where 1 is correct. The
    // fix moves the condition's prelude inside a `while (true)` head; this
    // pin holds it under both release configurations too.
    assert_release_matches_debug(
        r#"
import std::io::print;

import std::option::Option::{ None, Some, self };

fun main() {
	mut found: Option<i32> = None;
	mut cursor = 0;
	for (found is None) && cursor < 3 {
		found = Some(cursor);
		cursor += 1;
	}
	print(cursor);
}
"#,
        "1\n",
    );
}
