//! `Style::when`'s equivalence gate (backlog A36, style-variants.md §8 Q3).
//!
//! The combinator's whole claim is that a CHAIN of conditional merges and the
//! `+`-and-`if` spelling it folds up produce the same style — not "the same
//! properties", the same rendered `class` attribute, at every cell of the state
//! space. Class names are content hashes of the slot key and the declaration,
//! so comparing class lists is comparing resolved declarations: if one link
//! merged in the wrong order, or cleared a family it should not have, a hash
//! moves and the strings differ.
//!
//! The corpus program `vilan/test/style-when.vl` carries the same exhibit
//! through the byte gate and the interpreter-equivalence gate; what it cannot
//! do is check the program's OUTPUT against an expectation, which is this.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The 015 exhibit's shape: a base style and two deltas on independent state
/// axes, spelled both ways, over all four cells of the two flags.
const EXHIBIT: &str = r#"import std::io::print;
import std::style::{ Color, Style, space, style };

let base: Style = const style()
	.padding(space(2))
	.color(Color::gray(900))
	.background(Color::white());
let chosen: Style = const style().color(Color::blue(900)).background(Color::blue(100));
let muted: Style = const style().color(Color::gray(400));

fun chained(is_chosen: bool, is_muted: bool): str {
	base.when(is_chosen, chosen).when(is_muted, muted).class_list()
}

fun built(is_chosen: bool, is_muted: bool): str {
	mut out = base;
	if is_chosen {
		out = out + chosen;
	}
	if is_muted {
		out = out + muted;
	}
	out.class_list()
}

fun main() {
	mut cell = 0;
	for cell < 4 {
		let is_chosen = cell % 2 == 1;
		let is_muted = cell / 2 == 1;
		let chain = chained(is_chosen, is_muted);
		let sum = built(is_chosen, is_muted);
		print(i"cell {cell} same={chain == sum} classes={chain}");
		cell += 1;
	}
}

main();
"#;

fn std_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std")
}

fn build_and_run(program: &str) -> String {
    let dir = std::env::temp_dir().join(format!("vilan_style_when_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create the program directory");
    let source = dir.join("app.vl");
    std::fs::write(&source, program).expect("write the program");

    let build = Command::new(env!("CARGO_BIN_EXE_vilan"))
        .arg("build")
        .arg(&source)
        .env("VILAN_STD", std_dir())
        .output()
        .expect("run vilan build");
    assert!(
        build.status.success(),
        "vilan build failed:\n{}\n{}",
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );

    let run = Command::new("node")
        .arg("app.mjs")
        .current_dir(&dir)
        .output()
        .expect("run node");
    let stdout = String::from_utf8_lossy(&run.stdout).into_owned();
    assert!(
        run.status.success(),
        "the program failed:\n{stdout}\n{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
    stdout
}

/// Two things at once, and the second is why the first is not tautological.
///
/// 1. **Every cell agrees.** All four `same=` flags are true, so the chain is
///    the `+` spelling for every combination of the two flags — the equivalence
///    the accepted note (P15) states.
/// 2. **Chain order is precedence.** Both deltas set `color`; when both fire,
///    the class list carries the LATER one's colour class, which is also the
///    one cell 2 (muted alone) resolves to and NOT the one cell 1 (chosen
///    alone) does. Without that, a `when` that merged in the wrong direction
///    would still pass (1), because `built` would be wrong in the same way.
#[test]
fn a_when_chain_renders_the_same_classes_as_the_merge_it_folds_up() {
    let stdout = build_and_run(EXHIBIT);
    let cells: Vec<&str> = stdout.lines().collect();
    assert_eq!(cells.len(), 4, "expected four state cells; got:\n{stdout}");
    for cell in &cells {
        assert!(
            cell.contains("same=true"),
            "a `when` chain must render exactly what `+` and `if` render; \
             got:\n{stdout}"
        );
    }

    let classes = |line: &str| {
        line.split_once("classes=")
            .expect("a classes= field")
            .1
            .split(' ')
            .map(str::to_string)
            .collect::<Vec<String>>()
    };
    let (neither, chosen, muted, both) = (
        classes(cells[0]),
        classes(cells[1]),
        classes(cells[2]),
        classes(cells[3]),
    );
    // Slot order is stable across the four builds — padding, colour,
    // background — so index 1 is the colour slot in every cell.
    assert_eq!(
        neither.len(),
        3,
        "the exhibit must resolve three slots; got:\n{stdout}"
    );
    assert_ne!(
        chosen[1], muted[1],
        "the two deltas must resolve the colour slot differently, or the \
         precedence half of this test proves nothing; got:\n{stdout}"
    );
    assert_eq!(
        both[1], muted[1],
        "when two `when`s both fire, the LATER delta must win the property \
         they share — chain order is precedence, exactly as with `+`; \
         got:\n{stdout}"
    );
}
