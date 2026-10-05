//! `vilan check --fix` (I5, `proposal/index-type.md` §8.1): the numeric
//! mismatches' fixes applied to a package to a fixed point, then the ordinary
//! check. The edits themselves are `vilan_ide::numeric_fix`'s, the function
//! the language server's quick fixes call, and are pinned there; these pins
//! hold the DRIVER — the rounds, what it may edit, what it leaves, and the
//! exit code.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

mod support;

/// A fresh single-package project holding `source` as its entry.
fn temp_package(tag: &str, source: &str) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = support::scratch_root().join(format!(
        "vilan_check_fix_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("vilan.toml"), "[package]\nname = \"app\"\n").unwrap();
    std::fs::write(dir.join("src/main.vl"), source).unwrap();
    dir
}

fn vilan(dir: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_vilan"))
        .current_dir(dir)
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run vilan")
}

fn entry(dir: &Path) -> String {
    std::fs::read_to_string(dir.join("src/main.vl")).unwrap()
}

const TAKE: &str = concat!(
    "fun take(xs: List<str>, at: usize): str {\n",
    "\txs[at]\n",
    "}\n",
    "\n",
);

#[test]
fn every_steered_mismatch_is_converted_and_the_check_then_passes() {
    let dir = temp_package(
        "convert",
        &format!(
            "{TAKE}{}",
            concat!(
                "fun main() {\n",
                "\tlet xs = [\"a\", \"b\", \"c\"];\n",
                "\tlet width: i32 = 2;\n",
                "\tprint(take(xs, width + 0));\n",
                "\tlet at: usize = 1;\n",
                "\tlet wide: i32 = at;\n",
                "\tprint(i\"{width < at} {wide}\");\n",
                "}\n",
            )
        ),
    );
    assert!(
        !vilan(&dir, &["check", "."]).status.success(),
        "the program is refused before the fix, so the pin is not vacuous"
    );
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "{stdout}{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("fixed 3 numeric mismatches in 1 file"),
        "{stdout}"
    );
    assert!(stdout.contains("no errors"), "{stdout}");
    // The argument: the whole value, parenthesized. The annotation: the name.
    // The comparison: the NON-index operand converts to `usize`.
    assert!(
        text.contains("print(take(xs, (width + 0).as_usize()));"),
        "{text}"
    );
    assert!(text.contains("let wide: i32 = at.as_i32();"), "{text}");
    assert!(text.contains("{width.as_usize() < at}"), "{text}");
}

#[test]
fn a_literal_counter_is_declared_usize_and_its_other_uses_convert_in_later_rounds() {
    let dir = temp_package(
        "declare",
        &format!(
            "{TAKE}{}",
            concat!(
                "fun step(value: i32): i32 {\n",
                "\tvalue + 1\n",
                "}\n",
                "\n",
                "fun main() {\n",
                "\tlet xs = [\"a\", \"b\", \"c\"];\n",
                "\tmut at = 0;\n",
                "\tat = step(at);\n",
                "\tprint(take(xs, at));\n",
                "}\n",
            )
        ),
    );
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(output.status.success(), "{stdout}");
    // Round one declares the counter; the declaration then moves the
    // mismatch to `step`'s `i32` world, and rounds two and three convert at
    // its boundary — never a `.as_usize()` on `at` itself.
    assert!(text.contains("\tmut at: usize = 0;\n"), "{text}");
    assert!(
        text.contains("\tat = step(at.as_i32()).as_usize();\n"),
        "{text}"
    );
    assert!(!text.contains("at.as_usize()"), "{text}");
}

#[test]
fn what_needs_a_person_is_left_and_reported_with_a_failing_exit() {
    // A `str` is not a number: no conversion names it, so the fix pass
    // leaves the line alone and the check that follows reports it.
    let dir = temp_package(
        "residue",
        &format!(
            "{TAKE}{}",
            concat!(
                "fun main() {\n",
                "\tlet xs = [\"a\", \"b\"];\n",
                "\tlet width: i32 = 1;\n",
                "\tprint(take(xs, width));\n",
                "\tlet label: str = width;\n",
                "\tprint(label);\n",
                "}\n",
            )
        ),
    );
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!output.status.success(), "{stdout}{stderr}");
    assert!(
        stdout.contains("fixed 1 numeric mismatch in 1 file"),
        "{stdout}"
    );
    assert!(
        text.contains("print(take(xs, width.as_usize()));"),
        "{text}"
    );
    assert!(text.contains("let label: str = width;"), "{text}");
    assert!(
        stderr.contains("Expected str, but got i32 instead."),
        "{stderr}"
    );
}

#[test]
fn a_clean_package_is_left_byte_for_byte() {
    let source = "fun main() {\n\tlet xs = [1, 2];\n\tprint(i\"{xs[0]}\");\n}\n";
    let dir = temp_package("clean", source);
    let output = vilan(&dir, &["check", "--fix", "."]);
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let text = entry(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(output.status.success(), "{stdout}");
    assert!(
        stdout.contains("fixed 0 numeric mismatches in 0 files"),
        "{stdout}"
    );
    assert_eq!(text, source);
}

// --- A154: the moved std paths (the one-command migration to v0.44.0) -------
//
// Paths are compared as `Path::join(..).display()` builds them, never as a
// string with a separator in it: the report prints a file relative to the
// working directory through `Path::strip_prefix`, which spells `src\main.vl`
// on Windows, and these pins read the same there.

/// A fresh package directory holding `files` (relative path → text), the
/// manifest among them.
fn temp_tree(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    let dir = support::scratch_root().join(format!(
        "vilan_check_fix_a154_{tag}_{}_{unique}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    for (relative, text) in files {
        let path = dir.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    dir
}

fn read(dir: &Path, relative: &str) -> String {
    std::fs::read_to_string(dir.join(relative)).unwrap()
}

/// `relative` (written with `/`) as the report prints it on this platform.
fn shown(relative: &str) -> String {
    relative
        .split('/')
        .fold(PathBuf::new(), |path, part| path.join(part))
        .display()
        .to_string()
}

fn run(dir: &Path, args: &[&str]) -> (bool, String, String) {
    let output = vilan(dir, args);
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

const OLD_MANIFEST: &str = "# the app\n[package]\nname = \"app\"\ntarget = \"browser\"\n# the web set\nprelude = \"std::web\"  # ambient ui/style\n";

const OLD_MAIN: &str = concat!(
    "import pkg::state::count;\n",
    "import pkg::views::card;\n",
    "import std::dom::create_element;\n",
    "\n",
    "fun main() {\n",
    "\tlet _ = create_element(\"div\");\n",
    "\tlet _ = card();\n",
    "\tlet _ = count();\n",
    "}\n",
);

const OLD_VIEWS: &str =
    "import std::ui::{ View, view };\n\nexport fun card(): View {\n\tview(\"div\")\n}\n";

const OLD_STATE: &str = concat!(
    "import std::{ hash_map_cell::HashMapCell, rpc_server };\n",
    "import std::web::{ Signal, dom::create_element };\n",
    "\n",
    "export fun count(): i32 {\n",
    "\tlet signal = Signal::new(1);\n",
    "\tlet _ = create_element(\"p\");\n",
    "\tsignal.get()\n",
    "}\n",
);

#[test]
fn a154_old_paths_in_three_files_and_the_manifest_are_migrated_by_one_run() {
    let dir = temp_tree(
        "package",
        &[
            ("vilan.toml", OLD_MANIFEST),
            ("src/main.vl", OLD_MAIN),
            ("src/views.vl", OLD_VIEWS),
            ("src/state.vl", OLD_STATE),
        ],
    );
    let (ok, _, stderr) = run(&dir, &["check", "."]);
    assert!(
        !ok && stderr.contains("names the old path of a std module"),
        "the package is refused before the fix (at its manifest), so the pin is not vacuous: \
         {stderr}"
    );
    let (ok, stdout, stderr) = run(&dir, &["check", "--fix", "."]);
    let texts = (
        read(&dir, "vilan.toml"),
        read(&dir, "src/main.vl"),
        read(&dir, "src/views.vl"),
        read(&dir, "src/state.vl"),
    );
    let (ok_after, _, after) = run(&dir, &["check", "."]);
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "{stdout}{stderr}");
    assert!(ok_after, "the migrated package checks clean: {after}");
    let expected_report = format!(
        "fixed 6 moved std paths in 4 files\n  {}: 1\n  {}: 3\n  {}: 1\n  {}: 1\nfixed 0 numeric \
         mismatches in 0 files\n",
        shown("src/main.vl"),
        shown("src/state.vl"),
        shown("src/views.vl"),
        shown("vilan.toml"),
    );
    assert!(stdout.starts_with(&expected_report), "{stdout}");
    assert!(stdout.contains("no errors"), "{stdout}");
    // The manifest: the value and nothing else.
    assert_eq!(
        texts.0,
        OLD_MANIFEST.replace("\"std::web\"", "\"std::web::prelude\"")
    );
    // Each file was canonical, so it stays canonical: the moved import sorts
    // below `pkg::` now, where `vilan fmt` puts it.
    assert_eq!(
        texts.1,
        OLD_MAIN.replace(
            "import std::dom::create_element;\n",
            "import std::web::dom::create_element;\n"
        )
    );
    assert_eq!(texts.2, OLD_VIEWS.replace("std::ui::", "std::web::ui::"));
    // A brace list over moved modules, and E268's mixed list under the old
    // web-prelude path: the child stays, the prelude name moves under
    // `prelude::`.
    assert_eq!(
        texts.3,
        OLD_STATE
            .replace(
                "std::{ hash_map_cell::HashMapCell, rpc_server }",
                "std::{ reactive::hash_map_cell::HashMapCell, rpc::server }"
            )
            .replace(
                "std::web::{ Signal, dom::create_element }",
                "std::web::{ prelude::Signal, dom::create_element }"
            )
    );
}

#[test]
fn a154_a_second_run_changes_nothing_and_says_so() {
    let dir = temp_tree(
        "idempotent",
        &[
            ("vilan.toml", OLD_MANIFEST),
            ("src/main.vl", OLD_MAIN),
            ("src/views.vl", OLD_VIEWS),
            ("src/state.vl", OLD_STATE),
        ],
    );
    let (ok, first, _) = run(&dir, &["check", "--fix", "."]);
    assert!(ok, "{first}");
    let files = ["vilan.toml", "src/main.vl", "src/views.vl", "src/state.vl"];
    let before: Vec<String> = files.iter().map(|file| read(&dir, file)).collect();
    let (ok, second, stderr) = run(&dir, &["check", "--fix", "."]);
    let after: Vec<String> = files.iter().map(|file| read(&dir, file)).collect();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "{second}{stderr}");
    assert_eq!(before, after, "the second run writes nothing");
    assert!(
        second.starts_with("fixed 0 numeric mismatches in 0 files\n"),
        "nothing moved is said in the numeric line's own words, and no moved line: {second}"
    );
    assert!(!second.contains("moved std path"), "{second}");
}

/// `vilan check <file> --fix`: the file's own package — its manifest first,
/// then the files its analysis reports into.
#[test]
fn a154_a_file_run_migrates_the_file_and_its_manifest() {
    let dir = temp_tree(
        "file",
        &[
            ("vilan.toml", OLD_MANIFEST),
            ("src/views.vl", OLD_VIEWS),
            (
                "src/main.vl",
                "import pkg::views::card;\n\nfun main() {\n\tlet _ = card();\n\tlet _ = Signal::new(1);\n}\n",
            ),
        ],
    );
    let main = dir.join("src").join("main.vl");
    let (ok, stdout, stderr) = run(&dir, &["check", main.to_str().unwrap(), "--fix"]);
    let manifest = read(&dir, "vilan.toml");
    let views = read(&dir, "src/views.vl");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "{stdout}{stderr}");
    assert!(
        stdout.starts_with(&format!(
            "fixed 2 moved std paths in 2 files\n  {}: 1\n  {}: 1\n",
            shown("src/views.vl"),
            shown("vilan.toml"),
        )),
        "{stdout}"
    );
    assert!(
        manifest.contains("prelude = \"std::web::prelude\"  # ambient"),
        "{manifest}"
    );
    assert_eq!(views, OLD_VIEWS.replace("std::ui::", "std::web::ui::"));
}

/// The shapes no one edit rewrites correctly are LEFT, byte for byte, and
/// reported with the line, the column and the reason; the check that follows
/// still refuses them.
#[test]
fn a154_a_web_list_naming_self_is_left_for_a_hand_with_its_reason() {
    let main = "import std::web::{ self, Signal };\n\nfun main() {\n\tlet _ = Signal::new(1);\n}\n";
    let dir = temp_tree(
        "hand",
        &[
            (
                "vilan.toml",
                "[package]\nname = \"app\"\ntarget = \"browser\"\nprelude = \"std::web::prelude\"\n",
            ),
            ("src/main.vl", main),
        ],
    );
    let (ok, stdout, stderr) = run(&dir, &["check", "--fix", "."]);
    let text = read(&dir, "src/main.vl");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(!ok, "{stdout}{stderr}");
    assert_eq!(text, main);
    assert!(
        stdout.starts_with(&format!(
            "left 1 moved std path for a hand — no one edit rewrites it correctly\n  {}:1:13: \
             the brace list names `self`",
            shown("src/main.vl")
        )),
        "{stdout}"
    );
    assert!(
        stdout.contains("fixed 0 numeric mismatches in 0 files"),
        "{stdout}"
    );
    assert!(
        stderr.contains("`std::web` moved to `std::web::prelude`"),
        "{stderr}"
    );
}

/// A file under the package's declared `generated` root is regenerated by the
/// project, so it is not rewritten: it is named, with its root.
#[test]
fn a154_a_generated_file_is_named_and_not_rewritten() {
    let generated =
        "import std::ui::{ View, view };\n\nexport fun icon(): View {\n\tview(\"svg\")\n}\n";
    let dir = temp_tree(
        "generated",
        &[
            (
                "vilan.toml",
                "[package]\nname = \"app\"\ntarget = \"browser\"\nprelude = \"std::web::prelude\"\ngenerated = \"src/icons\"\n",
            ),
            ("src/icons/lib.vl", generated),
            (
                "src/main.vl",
                "import pkg::icons::icon;\nimport std::dom::create_element;\n\nfun main() {\n\tlet _ = icon();\n\tlet _ = create_element(\"div\");\n}\n",
            ),
        ],
    );
    let (ok, stdout, stderr) = run(&dir, &["check", "--fix", "."]);
    let text = read(&dir, "src/icons/lib.vl");
    let main = read(&dir, "src/main.vl");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        !ok,
        "the generated file's old path is still refused: {stdout}{stderr}"
    );
    assert_eq!(text, generated);
    assert!(
        main.contains("import std::web::dom::create_element;"),
        "{main}"
    );
    assert!(
        stdout.starts_with(&format!(
            "fixed 1 moved std path in 1 file\n  {}: 1\nleft 1 moved std path in generated files \
             — the project regenerates them, so change what generates them\n  {}: 1 (under the \
             package's `generated` root, {})\n",
            shown("src/main.vl"),
            shown("src/icons/lib.vl"),
            shown("src/icons"),
        )),
        "{stdout}"
    );
}

#[test]
fn a154_a_moved_path_and_a_numeric_mismatch_are_fixed_in_one_run() {
    let dir = temp_tree(
        "both",
        &[
            ("vilan.toml", "[package]\nname = \"app\"\n"),
            (
                "src/main.vl",
                &format!(
                    "import std::delta::delta_log_limit;\n\n{TAKE}fun main() {{\n\tlet xs = [\"a\", \"b\"];\n\tlet width: i32 = 1;\n\tprint(take(xs, width));\n\tprint(i\"{{delta_log_limit}}\");\n}}\n"
                ),
            ),
        ],
    );
    let (ok, stdout, stderr) = run(&dir, &["check", "--fix", "."]);
    let text = read(&dir, "src/main.vl");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        stdout.starts_with(&format!(
            "fixed 1 moved std path in 1 file\n  {}: 1\nfixed 1 numeric mismatch in 1 file\n",
            shown("src/main.vl")
        )),
        "{stdout}{stderr}"
    );
    assert!(
        text.contains("import std::reactive::delta::delta_log_limit;"),
        "{text}"
    );
    assert!(
        text.contains("print(take(xs, width.as_usize()));"),
        "{text}"
    );
    assert!(ok, "{stdout}{stderr}");
}

/// A workspace's members carry their own manifests: each one's `prelude` is
/// rewritten by the run addressed at the workspace.
#[test]
fn a154_a_workspace_members_manifest_is_rewritten() {
    let member = "[package]\nname = \"web\"\ntarget = \"browser\"\nprelude = \"std::web\"\n";
    let dir = temp_tree(
        "workspace",
        &[
            ("vilan.toml", "[project]\npackages = [\"web\"]\n"),
            ("web/vilan.toml", member),
            (
                "web/src/main.vl",
                "fun main() {\n\tlet _ = Signal::new(1);\n}\n",
            ),
        ],
    );
    let (ok, stdout, stderr) = run(&dir, &["check", "--fix", "."]);
    let text = read(&dir, "web/vilan.toml");
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "{stdout}{stderr}");
    assert_eq!(
        text,
        member.replace("\"std::web\"", "\"std::web::prelude\"")
    );
    assert!(
        stdout.starts_with(&format!(
            "fixed 1 moved std path in 1 file\n  {}: 1\n",
            shown("web/vilan.toml")
        )),
        "{stdout}"
    );
}

/// Nothing to fix writes nothing — the manifest included — and the report
/// is today's one numeric line.
#[test]
fn a154_nothing_to_fix_writes_nothing() {
    let manifest =
        "[package]\nname = \"app\"\ntarget = \"browser\"\nprelude = \"std::web::prelude\"\n";
    let main = "import std::web::dom::create_element;\n\nfun main() {\n\tlet _ = create_element(\"div\");\n\tlet _ = Signal::new(1);\n}\n";
    let dir = temp_tree(
        "nothing",
        &[("vilan.toml", manifest), ("src/main.vl", main)],
    );
    let modified = |relative: &str| {
        std::fs::metadata(dir.join(relative))
            .unwrap()
            .modified()
            .unwrap()
    };
    let before = (modified("vilan.toml"), modified("src/main.vl"));
    let (ok, stdout, stderr) = run(&dir, &["check", "--fix", "."]);
    let after = (modified("vilan.toml"), modified("src/main.vl"));
    let texts = (read(&dir, "vilan.toml"), read(&dir, "src/main.vl"));
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "{stdout}{stderr}");
    assert_eq!(texts, (manifest.to_string(), main.to_string()));
    assert_eq!(
        before, after,
        "no file is written, not even with the same bytes"
    );
    assert!(
        stdout.starts_with("fixed 0 numeric mismatches in 0 files\n"),
        "{stdout}"
    );
}
