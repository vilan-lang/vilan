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

/// The first 98 entries of a filled `0..` list (E277), as `dbg` lays them out
/// two spaces deep.
const FILLED_ZERO_TO_97: &str = concat!(
    "  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,\n",
    "  22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40,\n",
    "  41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 59,\n",
    "  60, 61, 62, 63, 64, 65, 66, 67, 68, 69, 70, 71, 72, 73, 74, 75, 76, 77, 78,\n",
    "  79, 80, 81, 82, 83, 84, 85, 86, 87, 88, 89, 90, 91, 92, 93, 94, 95, 96, 97,\n",
);

/// Q1's layout: one line when it fits 80 columns from where it starts, else
/// one entry per line, two spaces deeper, each with a trailing comma (a list
/// of scalars filling its lines instead, E277); a list past 100 entries stops
/// with `… N more`; a closure prints by its type.
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
            "[test.vl:4:2] long = Line {{\n  from = (1, 2),\n  to = (3, 4),\n  label = \"a label long enough to break\",\n}}\n[test.vl:9:13] many = [\n{}  98, 99, \u{2026} 3 more,\n]\n[test.vl:12:2] add = <closure |i32, i32| i32>\n",
            FILLED_ZERO_TO_97
        ),
    );
}

/// E277: a broken list (or set) whose entries are all scalars — numbers,
/// strings, bools, a field-less enum — fills each line up to the 80-column
/// limit, a trailing comma after every entry, where an aggregate element
/// (a struct, an `Option`) keeps a line of its own; nested in a broken struct
/// the filled lines sit two spaces deeper, and a list that fits stays on one
/// line.
#[test]
fn e277_a_list_of_scalars_fills_its_lines_and_aggregates_keep_one_per_line() {
    assert_dbg_runs(
        concat!(
            "import std::hash_set::HashSet;\n",
            "\n",
            "struct Point {\n",
            "\tx: i32,\n",
            "\ty: i32,\n",
            "}\n",
            "\n",
            "enum Color {\n",
            "\tRed,\n",
            "\tGreen,\n",
            "\tBlue,\n",
            "}\n",
            "\n",
            "struct Bag {\n",
            "\tlabel: str,\n",
            "\tvalues: List<i32>,\n",
            "}\n",
            "\n",
            "fun main() {\n",
            "\tmut numbers: List<i32> = [];\n",
            "\tmut next = 0;\n",
            "\tfor next < 40 {\n",
            "\t\tnumbers.push(next);\n",
            "\t\tnext = next + 1;\n",
            "\t}\n",
            "\tdbg(numbers);\n",
            "\tlet words = [\"alpha\", \"beta\", \"gamma\", \"delta\", \"epsilon\", \"zeta\", \"eta\", \"theta\", \"iota\", \"kappa\"];\n",
            "\tdbg(words);\n",
            "\tlet points = [Point { x = 1, y = 2 }, Point { x = 3, y = 4 }, Point { x = 5, y = 6 }];\n",
            "\tdbg(points);\n",
            "\tlet colors = [Color::Red, Color::Green, Color::Blue, Color::Red, Color::Green, Color::Blue];\n",
            "\tdbg(colors);\n",
            "\tlet maybes = [Some(1), None, Some(3), None, Some(5), None, Some(7), None, Some(9)];\n",
            "\tdbg(maybes);\n",
            "\tdbg(Bag { label = \"b\", values = numbers });\n",
            "\tmut seen: HashSet<i32> = HashSet::new();\n",
            "\tmut item = 100;\n",
            "\tfor item < 130 {\n",
            "\t\tseen.insert(item);\n",
            "\t\titem = item + 1;\n",
            "\t}\n",
            "\tdbg(seen);\n",
            "\tdbg([1.5, 2.5], [true, false]);\n",
            "}\n",
        ),
        "",
        concat!(
            "[test.vl:26:2] numbers = [\n",
            "  0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21,\n",
            "  22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39,\n",
            "]\n",
            "[test.vl:28:2] words = [\n",
            "  \"alpha\", \"beta\", \"gamma\", \"delta\", \"epsilon\", \"zeta\", \"eta\", \"theta\", \"iota\",\n",
            "  \"kappa\",\n",
            "]\n",
            "[test.vl:30:2] points = [\n",
            "  Point { x = 1, y = 2 },\n",
            "  Point { x = 3, y = 4 },\n",
            "  Point { x = 5, y = 6 },\n",
            "]\n",
            "[test.vl:32:2] colors = [\n",
            "  Color::Red, Color::Green, Color::Blue, Color::Red, Color::Green, Color::Blue,\n",
            "]\n",
            "[test.vl:34:2] maybes = [\n",
            "  Some(1),\n",
            "  None,\n",
            "  Some(3),\n",
            "  None,\n",
            "  Some(5),\n",
            "  None,\n",
            "  Some(7),\n",
            "  None,\n",
            "  Some(9),\n",
            "]\n",
            "[test.vl:35:2] Bag { label = \"b\", values = numbers } = Bag {\n",
            "  label = \"b\",\n",
            "  values = [\n",
            "    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20,\n",
            "    21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39,\n",
            "  ],\n",
            "}\n",
            "[test.vl:42:2] seen = HashSet {\n",
            "  100, 101, 102, 103, 104, 105, 106, 107, 108, 109, 110, 111, 112, 113, 114,\n",
            "  115, 116, 117, 118, 119, 120, 121, 122, 123, 124, 125, 126, 127, 128, 129,\n",
            "}\n",
            "[test.vl:43:2] [1.5, 2.5] = [1.5, 2.5]\n",
            "[test.vl:43:2] [true, false] = [true, false]\n",
        ),
    );
}

/// E275 (1): a WRITTEN `Debug` impl decides how `dbg` prints its type — at
/// the top, as a list element, as an option's payload, as a field of a type
/// that prints by its structure, and through a generic `T` — and a written
/// generic impl whose bound the type misses (`Boxed<Opaque>`) does not apply,
/// so the structure prints.
#[test]
fn e275_a_written_debug_impl_decides_how_dbg_prints_its_type() {
    assert_dbg_runs(
        concat!(
            "import std::debug::Debug;\n",
            "struct Celsius { degrees: f64 }\n",
            "impl Celsius with Debug {\n",
            "\tfun debug(self): str { self.degrees.debug() + \"C\" }\n",
            "}\n",
            "struct Boxed<T> { inner: T }\n",
            "impl Boxed<type T: Debug> with Debug {\n",
            "\tfun debug(self): str { \"boxed \" + self.inner.debug() }\n",
            "}\n",
            "struct Opaque { tag: i32 }\n",
            "enum Signal { Go, Stop(str) }\n",
            "impl Signal with Debug {\n",
            "\tfun debug(self): str {\n",
            "\t\tmatch self {\n",
            "\t\t\tSignal::Go => \"go!\",\n",
            "\t\t\tSignal::Stop(let why) => \"stop: \" + why,\n",
            "\t\t}\n",
            "\t}\n",
            "}\n",
            "struct Reading { place: str, temperature: Celsius }\n",
            "fun show<T>(value: T) {\n",
            "\tdbg(value);\n",
            "}\n",
            "fun main() {\n",
            "\tdbg(Celsius { degrees = 21.5 });\n",
            "\tdbg([Celsius { degrees = 1.0 }, Celsius { degrees = 2.0 }]);\n",
            "\tdbg(Boxed { inner = 7 }, Boxed { inner = Opaque { tag = 1 } });\n",
            "\tdbg(Signal::Go, Some(Signal::Stop(\"red\")));\n",
            "\tshow(Celsius { degrees = 3.0 });\n",
            "\tdbg(Reading { place = \"here\", temperature = Celsius { degrees = 4.5 } });\n",
            "}\n",
        ),
        "",
        concat!(
            "[test.vl:25:2] Celsius { degrees = 21.5 } = 21.5C\n",
            "[test.vl:26:2] [Celsius { degrees = 1.0 }, Celsius { degrees = 2.0 }] = [\n",
            "  1.0C,\n",
            "  2.0C,\n",
            "]\n",
            "[test.vl:27:2] Boxed { inner = 7 } = boxed 7\n",
            "[test.vl:27:2] Boxed { inner = Opaque { tag = 1 } } = Boxed {\n",
            "  inner = Opaque { tag = 1 },\n",
            "}\n",
            "[test.vl:28:2] Signal::Go = go!\n",
            "[test.vl:28:2] Some(Signal::Stop(\"red\")) = Some(stop: red)\n",
            "[test.vl:22:2] value = 3.0C\n",
            "[test.vl:30:2] Reading { place = \"here\", temperature = Celsius { degrees = 4.5 } } = Reading {\n",
            "  place = \"here\",\n",
            "  temperature = 4.5C,\n",
            "}\n",
        ),
    );
}

/// E275 (2): `[derive(Debug)]` spells an enum variant qualified
/// (`Shape::Rect(2, 3)`) and a string as the printer escapes it, so `.debug()`
/// is `dbg`'s spelling on one line; a field whose type has a written impl
/// renders through it in both; and `dbg` still lays a derived value out over
/// lines past 80 columns, where `.debug()` stays one line.
#[test]
fn e275_the_derive_spells_variants_qualified_and_agrees_with_dbg() {
    assert_dbg_runs(
        concat!(
            "import std::debug::Debug;\n",
            "struct Celsius { degrees: f64 }\n",
            "impl Celsius with Debug {\n",
            "\tfun debug(self): str { self.degrees.debug() + \"C\" }\n",
            "}\n",
            "[derive(Debug)]\n",
            "enum Shape { Circle(f64), Rect(i32, i32), Empty }\n",
            "[derive(Debug)]\n",
            "struct Reading { place: str, temperature: Celsius, shape: Shape }\n",
            "[derive(Debug)]\n",
            "struct Wide { first_label: str, second_label: str, numbers: List<i32> }\n",
            "fun main() {\n",
            "\tlet reading = Reading { place = \"a \\\"quoted\\\"\\tplace\\0\", temperature = Celsius { degrees = 4.5 }, shape = Shape::Rect(2, 3) };\n",
            "\tdbg(reading);\n",
            "\tprint(reading.debug());\n",
            "\tprint(Shape::Circle(1.5).debug());\n",
            "\tprint(Shape::Empty.debug());\n",
            "\tlet wide = Wide { first_label = \"the first label\", second_label = \"the second label\", numbers = [1, 2, 3] };\n",
            "\tdbg(wide);\n",
            "\tprint(wide.debug());\n",
            "}\n",
        ),
        concat!(
            "Reading { place = \"a \\\"quoted\\\"\\tplace\\0\", temperature = 4.5C, shape = Shape::Rect(2, 3) }\n",
            "Shape::Circle(1.5)\n",
            "Shape::Empty\n",
            "Wide { first_label = \"the first label\", second_label = \"the second label\", numbers = [1, 2, 3] }\n",
        ),
        concat!(
            "[test.vl:14:2] reading = Reading {\n",
            "  place = \"a \\\"quoted\\\"\\tplace\\0\",\n",
            "  temperature = 4.5C,\n",
            "  shape = Shape::Rect(2, 3),\n",
            "}\n",
            "[test.vl:19:2] wide = Wide {\n",
            "  first_label = \"the first label\",\n",
            "  second_label = \"the second label\",\n",
            "  numbers = [1, 2, 3],\n",
            "}\n",
        ),
    );
}

/// E275 (3): `.debug()` stays opt-in — a type without a derived or written
/// impl has no `debug` method (`dbg` is the path that needs none) — and a
/// program's own enum named `Option` prints qualified, as any enum of its own
/// does; only std's `Option` and `Result` print bare.
#[test]
fn e275_debug_stays_opt_in_and_only_stds_prelude_enums_print_bare() {
    assert_fails_with(
        "import std::debug::Debug;\nstruct Point { x: i32 }\nfun main() { print(Point { x = 1 }.debug()); }\n",
        "Point has no method 'debug'",
    );
    assert_dbg_runs(
        concat!(
            "mod mine {\n",
            "\texport enum Option { Some(i32), None }\n",
            "}\n",
            "fun main() {\n",
            "\tdbg(mine::Option::Some(3), Some(3));\n",
            "}\n",
        ),
        "",
        concat!(
            "[test.vl:5:2] mine::Option::Some(3) = Option::Some(3)\n",
            "[test.vl:5:2] Some(3) = Some(3)\n",
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

/// E276: `dbg` shows the real value, so negative zero is `-0.0` — a literal,
/// a computed one, an `f32`, and one inside an aggregate — while `print` keeps
/// N136's `0`. `.debug()` agrees with `dbg` (the printer's spelling, E275).
#[test]
fn e276_dbg_shows_negative_zero_and_print_keeps_zero() {
    assert_dbg_runs(
        concat!(
            "import std::debug::Debug;\n",
            "fun main() {\n",
            "\tlet zero = 0.0;\n",
            "\tdbg(-0.0, zero * -1.0, -0.0f, [0.0, -0.0], Some(-0.0));\n",
            "\tprint(zero * -1.0);\n",
            "\tprint((zero * -1.0).debug());\n",
            "\tprint((-0.0f).debug());\n",
            "\tdbg(0.0, -1.5);\n",
            "}\n",
        ),
        "0\n-0.0\n-0.0\n",
        concat!(
            "[test.vl:4:2] -0.0 = -0.0\n",
            "[test.vl:4:2] zero * -1.0 = -0.0\n",
            "[test.vl:4:2] -0.0f = -0.0\n",
            "[test.vl:4:2] [0.0, -0.0] = [0.0, -0.0]\n",
            "[test.vl:4:2] Some(-0.0) = Some(-0.0)\n",
            "[test.vl:8:2] 0.0 = 0.0\n",
            "[test.vl:8:2] -1.5 = -1.5\n",
        ),
    );
}

/// S1b: a `dyn` value prints the value it erased — `dyn Area(Square { side
/// = 2 })`, through the `show` slot its table carries — alone, in a list, in
/// an option and with trait arguments; a value whose type has a written
/// `Debug` prints through it there too. A table keeps its members' names
/// (`show` is a member here), and a program that never calls `dbg` carries no
/// slot at all.
#[test]
fn s1b_a_dyn_value_prints_what_it_holds() {
    let source = concat!(
        "trait Area { fun area(self): i32; }\n",
        "trait Named { fun show(self): str; }\n",
        "trait Label<T> { fun label(self): T; }\n",
        "struct Square { side: i32 }\n",
        "impl Square with Area { fun area(self): i32 { self.side * self.side } }\n",
        "impl Square with Named { fun show(self): str { \"square\" } }\n",
        "impl Square with Label<str> { fun label(self): str { \"sq\" } }\n",
        "fun main() {\n",
        "\tlet one: dyn Area = Square { side = 2 };\n",
        "\tlet shapes: List<dyn Area> = [Square { side = 3 }];\n",
        "\tlet named: dyn Named = Square { side = 4 };\n",
        "\tlet labelled: dyn Label<str> = Square { side = 5 };\n",
        "\tdbg(one, shapes, Some(one), named, labelled);\n",
        "\tprint(named.show());\n",
        "}\n",
    );
    assert_dbg_runs(
        source,
        "square\n",
        concat!(
            "[test.vl:13:2] one = dyn Area(Square { side = 2 })\n",
            "[test.vl:13:2] shapes = [dyn Area(Square { side = 3 })]\n",
            "[test.vl:13:2] Some(one) = Some(dyn Area(Square { side = 2 }))\n",
            "[test.vl:13:2] named = dyn Named(Square { side = 4 })\n",
            "[test.vl:13:2] labelled = dyn Label<str>(Square { side = 5 })\n",
        ),
    );
    let without_dbg = source.replace("\tdbg(one, shapes, Some(one), named, labelled);\n", "");
    let javascript = compile(&without_dbg).expect("a clean compile");
    assert!(!javascript.contains("$show"), "{javascript}");
    assert!(compile(source).expect("a clean compile").contains("$show:"));
}

// --- S4: `Debug` through the printer (E260) -----------------------------

/// E260: `[derive(Debug)]` on a struct with a `List` and an `Option` field
/// compiles and renders them; `T: Debug` takes a list, an option and a result;
/// a float keeps its `.0` (`3.0.debug()` is `"3.0"`) — the printer's
/// spellings, on one line.
#[test]
fn s4_debug_covers_every_container_the_printer_prints() {
    assert_compiles_and_runs(
        concat!(
            "import std::debug::Debug;\n",
            "[derive(Debug)]\n",
            "struct Bag { items: List<i32>, maybe: Option<f64> }\n",
            "fun show<T: Debug>(value: T): str { value.debug() }\n",
            "fun main() {\n",
            "\tprint(Bag { items = [1, 2], maybe = Some(3.0) }.debug());\n",
            "\tprint(show([1, 2]));\n",
            "\tprint(show(Some([Some(1)])));\n",
            "\tlet failed: Result<i32, str> = Err(\"no\");\n",
            "\tprint(show(failed));\n",
            "\tprint(3.0.debug());\n",
            "\tprint((0.0 * -1.0).debug());\n",
            "\tprint(1.5f.debug());\n",
            "}\n",
        ),
        concat!(
            "Bag { items = [1, 2], maybe = Some(3.0) }\n",
            "[1, 2]\n",
            "Some([Some(1)])\n",
            "Err(\"no\")\n",
            "3.0\n",
            "-0.0\n",
            "1.5\n",
        ),
    );
}

/// E260's tuple case: std's `impl type T: (2..: Debug) with Debug`, which
/// waited on B557 (a blanket over the tuple family was admitted for a
/// non-tuple at the bound check, so it would have made every type `Debug`).
#[test]
fn s4_debug_covers_a_tuple() {
    assert_compiles_and_runs(
        "import std::debug::Debug;\nfun show<T: Debug>(value: T): str { value.debug() }\nfun main() { print(show((1, \"two\", 2.5))); }\n",
        "(1, \"two\", 2.5)\n",
    );
}

// --- E259: readable instance names --------------------------------------

/// E259: a generic function's emitted instance is named after its source in
/// the readable build — `first`, and `describe` then `describe2` for a second
/// instance whose body differs — where every one used to be `$a`, which is
/// what a stack trace and a debugger showed.
#[test]
fn e259_a_generic_instance_is_named_after_its_function() {
    let source = concat!(
        "trait Named {\n",
        "\tfun name(self): str;\n",
        "}\n",
        "struct Ada {}\n",
        "struct Alan {}\n",
        "impl Ada with Named {\n",
        "\tfun name(self): str { \"ada\" }\n",
        "}\n",
        "impl Alan with Named {\n",
        "\tfun name(self): str { \"alan\" }\n",
        "}\n",
        "fun describe<T: Named>(value: T): str { value.name() }\n",
        "fun first<T>(items: List<T>): T { items[0] }\n",
        "fun main() {\n",
        "\tprint(describe(Ada {}));\n",
        "\tprint(describe(Alan {}));\n",
        "\tprint(first([1, 2]));\n",
        "\tprint(first([\"a\"]));\n",
        "}\n",
    );
    let javascript = compile(source).expect("a clean compile");
    for declared in [
        "function describe(",
        "function describe2(",
        "function first(",
    ] {
        assert!(
            javascript.contains(declared),
            "missing {declared}:\n{javascript}"
        );
    }
    assert!(
        !javascript.contains("function $"),
        "an instance kept a generated name:\n{javascript}"
    );
    assert_compiles_and_runs(source, "ada\nalan\n1\na\n");
}

// --- N136: `print` of a number -------------------------------------------

/// N136 (R-g door (a)): a number prints by the language's own conversion —
/// negative zero is `0`, a float and an integer alike, as on the native
/// backend — and only numbers are wrapped: a list still prints by node's
/// layout.
#[test]
fn n136_print_writes_negative_zero_as_zero() {
    let source = concat!(
        "fun main() {\n",
        "\tprint(0.0 * -1.0);\n",
        "\tlet zero = 0;\n",
        "\tprint(zero * -1);\n",
        "\tprint(-zero);\n",
        "\tprint(2.5);\n",
        "\tprint([1, 2]);\n",
        "}\n",
    );
    let javascript = compile(source).expect("a clean compile");
    assert!(
        javascript.contains("console.log(String(0.0 * -(1.0)));"),
        "{javascript}"
    );
    assert!(
        javascript.contains("console.log([ 1, 2 ]);"),
        "{javascript}"
    );
    assert_compiles_and_runs(source, "0\n0\n0\n2.5\n[ 1, 2 ]\n");
}

// --- S2: `dbg_stack()` ---------------------------------------------------

/// `L:C` of a byte offset in `source`, 1-based, columns in characters.
fn line_column(source: &str, offset: usize) -> String {
    let prefix = &source[..offset];
    let line = prefix.matches('\n').count() + 1;
    let column = prefix[prefix.rfind('\n').map_or(0, |at| at + 1)..]
        .chars()
        .count()
        + 1;
    format!("{line}:{column}")
}

/// E281/E282: what the move and view checks recorded at each `dbg_stack()`
/// call of `source`, as `L: name: verdict` lines — the call's line, then each
/// recorded binding — in source order.
#[track_caller]
fn dbg_stack_records(source: &str) -> Vec<String> {
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
            assert!(
                errors.is_empty(),
                "expected a clean analysis, got: {:#?}",
                errors.iter().map(|error| &error.msg).collect::<Vec<_>>()
            );
            let program = program.expect("a program");
            let name = |id| {
                program
                    .variables
                    .get(&id)
                    .map(|variable| variable.name)
                    .or_else(|| program.parameters.get(&id).map(|parameter| parameter.name))
                    .unwrap_or("?")
            };
            let mut lines: Vec<(usize, u32, String)> = Vec::new();
            for (call, moved) in &program.dbg_stack_moves {
                let at = program.span_map[call].start;
                let line = line_column(leaked, at);
                let line = line.split(':').next().unwrap_or("?").to_string();
                for (binding, state) in moved {
                    let verdict = match state {
                        vilan_core::analyzer::DbgStackMove::Moved(span) => {
                            format!("moved at {}", line_column(leaked, span.start))
                        }
                        vilan_core::analyzer::DbgStackMove::MovedOnSomePaths(_) => {
                            "moved on some paths".to_string()
                        }
                    };
                    lines.push((
                        at,
                        binding.0,
                        format!("{line}: {}: {verdict}", name(*binding)),
                    ));
                }
            }
            for (call, views) in &program.dbg_stack_invalidated {
                let at = program.span_map[call].start;
                let line = line_column(leaked, at);
                let line = line.split(':').next().unwrap_or("?").to_string();
                for entry in views {
                    let by = match entry.callee {
                        Some(callee) => program
                            .functions
                            .get(&callee)
                            .map(|function| function.name)
                            .or_else(|| {
                                program
                                    .external_functions
                                    .get(&callee)
                                    .map(|external| external.name)
                            })
                            .unwrap_or("?"),
                        None => "assignment",
                    };
                    let event_at = line_column(leaked, program.span_map[&entry.event].start);
                    lines.push((
                        at,
                        entry.view.0,
                        format!(
                            "{line}: {}: invalidated by {by} at {event_at}",
                            name(entry.view)
                        ),
                    ));
                }
            }
            lines.sort();
            lines.into_iter().map(|(_, _, line)| line).collect()
        })
        .expect("spawn worker")
        .join()
        .expect("worker panicked")
}

/// E281 + E282 on debug-48's repro (`s2_prerequisites.vl`): at a `dbg_stack()`
/// call the resource move scan records the moved binding with its move site,
/// and the view-invalidation scan the capture view past its last use that a
/// push invalidated since — the two facts the expansion must have to print
/// `<moved at 29:10>` and `<view, invalidated by push at 34:4>` without
/// reading either.
#[test]
fn e281_e282_the_move_and_view_scans_record_their_state_at_a_dbg_stack_call() {
    let source = concat!(
        "[resource]\n",
        "struct Guard {\n",
        "\tid: i32,\n",
        "}\n",
        "\n",
        "fun consume(own guard: Guard) {\n",
        "\tprint(guard.id);\n",
        "}\n",
        "\n",
        "fun main() {\n",
        "\tlet guard = Guard { id = 1 };\n",
        "\tconsume(guard);\n",
        "\tmut rows = [Some(1), Some(2)];\n",
        "\tmatch &rows[0] {\n",
        "\t\tSome(let first) => {\n",
        "\t\t\tprint(*first);\n",
        "\t\t\trows.push(None);\n",
        "\t\t\tprint(rows.len());\n",
        "\t\t\tdbg_stack();\n",
        "\t\t},\n",
        "\t\tNone => {},\n",
        "\t}\n",
        "}\n",
    );
    assert_eq!(
        dbg_stack_records(source),
        vec![
            "19: guard: moved at 12:10".to_string(),
            "19: first: invalidated by push at 17:4".to_string(),
        ]
    );
}

/// E281: R7's other legal state — moved on one path, payload-free on the other
/// (B67's `is` refinement) — is recorded as moved on SOME paths.
#[test]
fn e281_a_binding_moved_on_some_paths_is_recorded_so() {
    let source = concat!(
        "[resource]\n",
        "struct Guard { id: i32 }\n",
        "fun consume(own held: Option<Guard>) {\n",
        "\tif held is Some(let guard) {\n",
        "\t\tprint(guard.id);\n",
        "\t}\n",
        "}\n",
        "fun main() {\n",
        "\tlet held: Option<Guard> = Some(Guard { id = 1 });\n",
        "\tdbg_stack();\n",
        "\tif held is Some(_) {\n",
        "\t\tconsume(held);\n",
        "\t}\n",
        "\tdbg_stack();\n",
        "}\n",
    );
    assert_eq!(
        dbg_stack_records(source),
        vec!["14: held: moved on some paths".to_string()]
    );
}

/// E282 across branches and loops. An event on one path of an `if` reaches a
/// call after it but not a call on the other path; an assignment to the root
/// is an event too; and a capture view that is never used — retired from its
/// arm's start — is invalidated for a call EARLIER in a loop by a push later
/// in it, which comes first on the next iteration.
#[test]
fn e282_an_invalidation_follows_the_paths_and_the_loops_to_a_dbg_stack_call() {
    let branches = concat!(
        "fun main() {\n",
        "\tmut rows = [Some(1), Some(2)];\n",
        "\tmatch &rows[0] {\n",
        "\t\tSome(let first) => {\n",
        "\t\t\tprint(*first);\n",
        "\t\t\tif rows.len() > 5 {\n",
        "\t\t\t\trows.push(None);\n",
        "\t\t\t} else {\n",
        "\t\t\t\tdbg_stack();\n",
        "\t\t\t}\n",
        "\t\t\tdbg_stack();\n",
        "\t\t},\n",
        "\t\tNone => {},\n",
        "\t}\n",
        "\tmatch &rows[1] {\n",
        "\t\tSome(let second) => {\n",
        "\t\t\tprint(*second);\n",
        "\t\t\trows = [None];\n",
        "\t\t\tdbg_stack();\n",
        "\t\t},\n",
        "\t\tNone => {},\n",
        "\t}\n",
        "}\n",
    );
    assert_eq!(
        dbg_stack_records(branches),
        vec![
            "11: first: invalidated by push at 7:5".to_string(),
            "19: second: invalidated by assignment at 18:4".to_string(),
        ]
    );
    let looped = concat!(
        "fun main() {\n",
        "\tmut rows = [Some(1), Some(2)];\n",
        "\tmatch &rows[0] {\n",
        "\t\tSome(let first) => {\n",
        "\t\t\tmut count = 0;\n",
        "\t\t\tfor count < 2 {\n",
        "\t\t\t\tdbg_stack();\n",
        "\t\t\t\trows.push(None);\n",
        "\t\t\t\tcount = count + 1;\n",
        "\t\t\t}\n",
        "\t\t},\n",
        "\t\tNone => {},\n",
        "\t}\n",
        "}\n",
    );
    assert_eq!(
        dbg_stack_records(looped),
        vec!["7: first: invalidated by push at 8:5".to_string()]
    );
}

/// N149: N136's recording is static, so `print(value)` with `value: T` was not
/// wrapped where `T` is a number and negative zero printed `-0` on JS (`0`
/// natively). The type is read per instance now: the number instances share a
/// wrapped body, the string's stays bare — and a closure parameter whose type
/// is inferred from its use (`|n| print(n)` handed to a `List<i32>` walk) is
/// wrapped too.
#[test]
fn n149_print_of_a_generic_number_is_wrapped_per_instance() {
    let source = concat!(
        "fun show<T>(value: T) {\n",
        "\tprint(value);\n",
        "}\n",
        "struct Holder<T> { value: T }\n",
        "fun shout<T>(holder: Holder<T>) {\n",
        "\tprint(holder.value);\n",
        "}\n",
        "fun main() {\n",
        "\tlet zero = 0.0;\n",
        "\tshow(zero * -1.0);\n",
        "\tshow(-0.0f);\n",
        "\tshow(0 * -1);\n",
        "\tshow(\"text\");\n",
        "\tshout(Holder { value = zero * -1.0 });\n",
        "\t[0.0 * -1.0].for_each(|n| print(n));\n",
        "}\n",
    );
    let javascript = compile(source).expect("a clean compile");
    assert!(
        javascript.contains("function show(value) {\n\tconsole.log(String(value));"),
        "{javascript}"
    );
    assert!(
        javascript.contains("function show2(value) {\n\tconsole.log(value);"),
        "{javascript}"
    );
    assert_compiles_and_runs(source, "0\n0\n0\ntext\n0\n0\n");
}
