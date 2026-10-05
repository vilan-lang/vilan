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

// --- S1: `dbg(..)` ------------------------------------------------------

/// Runs `source` on node (the harness's default platform), where `dbg` writes
/// to STDERR (Q3), and holds both streams.
#[track_caller]
fn assert_dbg_runs(source: &str, expected_stdout: &str, expected_stderr: &str) {
    match compile_and_run_capturing_stderr(source) {
        Ok((stdout, stderr)) => {
            assert_eq!(stderr, expected_stderr, "stderr mismatch");
            assert_eq!(stdout, expected_stdout, "stdout mismatch");
        }
        Err(errors) => panic!("expected a clean run, got: {errors:#?}"),
    }
}

/// Every shape S1 prints, in vilan's own literal syntax (Q1): a struct by its
/// fields, an enum variant qualified (the prelude's four bare), a tuple, a
/// list, a string quoted and escaped, a float keeping its `.0`, an integer of
/// any width, a field-less struct by its name — to stderr on node (Q3).
#[test]
fn s1_dbg_prints_each_shape_in_vilan_literal_syntax() {
    assert_dbg_runs(
        concat!(
            "struct Point { x: i32, y: i32 }\n",
            "struct Unit {}\n",
            "enum Shape { Circle(f64), Rect(i32, i32), Empty }\n",
            "fun main() {\n",
            "\tdbg(Point { x = 1, y = -2 });\n",
            "\tdbg(Shape::Circle(1.5), Shape::Rect(2, 3), Shape::Empty);\n",
            "\tlet none: Option<u8> = None;\n",
            "\tlet failed: Result<i32, str> = Err(\"no\");\n",
            "\tdbg(Some(5), none, failed);\n",
            "\tdbg((1, (2, \"x\")), [[1, 2], []], Unit {});\n",
            "\tdbg(\"a \\\"quoted\\\" \\\\ line\\n\", 3.0, 0.25, true, 7usize);\n",
            "}\n",
        ),
        "",
        concat!(
            "[test.vl:5:2] Point { x = 1, y = -2 } = Point { x = 1, y = -2 }\n",
            "[test.vl:6:2] Shape::Circle(1.5) = Shape::Circle(1.5)\n",
            "[test.vl:6:2] Shape::Rect(2, 3) = Shape::Rect(2, 3)\n",
            "[test.vl:6:2] Shape::Empty = Shape::Empty\n",
            "[test.vl:9:2] Some(5) = Some(5)\n",
            "[test.vl:9:2] none = None\n",
            "[test.vl:9:2] failed = Err(\"no\")\n",
            "[test.vl:10:2] (1, (2, \"x\")) = (1, (2, \"x\"))\n",
            "[test.vl:10:2] [[1, 2], []] = [[1, 2], []]\n",
            "[test.vl:10:2] Unit {} = Unit\n",
            "[test.vl:11:2] \"a \\\"quoted\\\" \\\\ line\\n\" = \"a \\\"quoted\\\" \\\\ line\\n\"\n",
            "[test.vl:11:2] 3.0 = 3.0\n",
            "[test.vl:11:2] 0.25 = 0.25\n",
            "[test.vl:11:2] true = true\n",
            "[test.vl:11:2] 7usize = 7\n",
        ),
    );
}

/// Q3: in the browser there is no stderr, so `dbg` writes with `console.log`.
#[test]
fn s1_dbg_writes_to_the_console_log_in_the_browser() {
    let javascript = compile_on("fun main() { dbg(1 + 1); }\n", Platform::Browser)
        .expect("a clean browser compile");
    assert!(
        javascript.contains("__dbg(console.log, \"test.vl:1:14\""),
        "{javascript}"
    );
    let node = compile("fun main() { dbg(1 + 1); }\n").expect("a clean compile");
    assert!(
        node.contains("__dbg(console.error, \"test.vl:1:14\""),
        "{node}"
    );
}

/// Q2: `dbg` answers its argument (so it wraps an expression in place), a
/// TUPLE of them for several — a tuple argument's slots splice, as every
/// tuple's do — and `()` for none, printing the bare location.
#[test]
fn s1_dbg_answers_its_argument_a_tuple_or_nothing() {
    assert_dbg_runs(
        concat!(
            "fun main() {\n",
            "\tlet total = dbg(2 * 3) + 1;\n",
            "\tprint(total);\n",
            "\tlet pair = dbg((1, 2), \"x\");\n",
            "\tprint(pair.0.1);\n",
            "\tprint(pair.1);\n",
            "\tdbg();\n",
            "}\n",
        ),
        "7\n2\nx\n",
        concat!(
            "[test.vl:2:14] 2 * 3 = 6\n",
            "[test.vl:4:13] (1, 2) = (1, 2)\n",
            "[test.vl:4:13] \"x\" = \"x\"\n",
            "[test.vl:7:2]\n",
        ),
    );
}

/// Q2: a `dbg` STATEMENT reads its arguments in place — `dbg(guard);` leaves
/// the resource where it was, and its destructor runs once — while in
/// expression position the value moves through, and a plain aggregate is a
/// copy (`mut copy = dbg(xs); copy.push(9)` leaves `xs`).
#[test]
fn s1_a_dbg_statement_reads_in_place_and_an_expression_moves_through() {
    assert_dbg_runs(
        concat!(
            "import std::drop::Drop;\n",
            "[resource]\n",
            "struct Guard { id: i32 }\n",
            "impl Guard with Drop {\n",
            "\tfun drop(&mut self) { print(\"dropped\"); }\n",
            "}\n",
            "fun main() {\n",
            "\tlet guard = Guard { id = 7 };\n",
            "\tdbg(guard);\n",
            "\tdbg(guard.id);\n",
            "\tlet moved = dbg(guard);\n",
            "\tprint(moved.id);\n",
            "\tmut xs = [1, 2];\n",
            "\tmut copy = dbg(xs);\n",
            "\tcopy.push(9);\n",
            "\tprint(xs.len());\n",
            "}\n",
        ),
        "7\ndropped\n2\n",
        concat!(
            "[test.vl:9:2] guard = Guard { id = 7 }\n",
            "[test.vl:10:2] guard.id = 7\n",
            "[test.vl:11:14] guard = Guard { id = 7 }\n",
            "[test.vl:14:13] xs = [1, 2]\n",
        ),
    );
}

/// Q6: a generic `T` is printed as the instance's concrete type — the body is
/// emitted per instantiation already, so each instance calls its own printer.
#[test]
fn s1_dbg_of_a_generic_value_prints_each_instantiations_type() {
    assert_dbg_runs(
        concat!(
            "struct Point { x: i32, y: i32 }\n",
            "enum Tree<T> { Leaf(T), Node(List<Tree<T>>) }\n",
            "fun show<T>(value: T): T {\n",
            "\tdbg(value)\n",
            "}\n",
            "fun main() {\n",
            "\tshow(3);\n",
            "\tshow(\"s\");\n",
            "\tshow(Point { x = 1, y = 2 });\n",
            "\tshow(Tree::Node([Tree::Leaf(1.5), Tree::Leaf(2.0)]));\n",
            "}\n",
        ),
        "",
        concat!(
            "[test.vl:4:2] value = 3\n",
            "[test.vl:4:2] value = \"s\"\n",
            "[test.vl:4:2] value = Point { x = 1, y = 2 }\n",
            "[test.vl:4:2] value = Tree::Node([Tree::Leaf(1.5), Tree::Leaf(2.0)])\n",
        ),
    );
}

/// Q1's layout: one line when it fits 80 columns from where it starts, else
/// one entry per line, two spaces deeper, each with a trailing comma; a list
/// past 100 entries stops with `… N more`; a closure prints by its type.
#[test]
fn s1_dbg_breaks_past_80_columns_and_cuts_a_long_list() {
    assert_dbg_runs(
        concat!(
            "struct Line { from: (i32, i32), to: (i32, i32), label: str }\n",
            "fun main() {\n",
            "\tlet long = Line { from = (1, 2), to = (3, 4), label = \"a label long enough to break\" };\n",
            "\tdbg(long);\n",
            "\tmut many: List<usize> = [];\n",
            "\tfor many.len() < 103 {\n",
            "\t\tmany.push(many.len());\n",
            "\t}\n",
            "\tlet head = dbg(many).len();\n",
            "\tprint(head);\n",
            "\tlet add = |a: i32, b: i32| a + b;\n",
            "\tdbg(add);\n",
            "}\n",
        ),
        "103\n",
        &format!(
            "[test.vl:4:2] long = Line {{\n  from = (1, 2),\n  to = (3, 4),\n  label = \"a label long enough to break\",\n}}\n[test.vl:9:13] many = [\n{}  \u{2026} 3 more,\n]\n[test.vl:12:2] add = <closure |i32, i32| -> i32>\n",
            (0..100).map(|n| format!("  {n},\n")).collect::<String>()
        ),
    );
}

/// A program's OWN `dbg` wins over the prelude's, as any prelude name does.
#[test]
fn s1_a_programs_own_dbg_shadows_the_intrinsic() {
    assert_compiles_and_runs(
        "fun dbg(value: i32): i32 { value * 10 }\nfun main() { print(dbg(4)); }\n",
        "40\n",
    );
}
