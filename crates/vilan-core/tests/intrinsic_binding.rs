//! B265: an intrinsic binds to an `external fun` DECLARATION, not to a name.
//!
//! The compiler's intrinsic tables are keyed by NAME on a nominal subject —
//! `remove`, `insert`, `pop`, `push` on any `impl List` — which is how a built-in
//! lowering finds std's `List::remove` without std having to name the intrinsic.
//! Keyed by name alone it also matched a BODIED declaration: give `List::remove` a
//! vilan body and the body was dropped on the floor, every call lowering to
//! `splice` with nothing said. M47 found it A/B-ing an old std against the new
//! lowering, which is the only way anyone would — a user's `impl List` cannot
//! redeclare a name std already declares, so the reach is std authors, who are
//! exactly the people who write that body.
//!
//! The exhibit needs a std whose `List::remove` HAS a body, so these pins compile
//! against a scratch COPY of the toolchain with one declaration rewritten. That is
//! also why they live in their own target: every other core suite is written
//! against the one real std.

use std::path::{Path, PathBuf};

use vilan_core::analyzer::Intrinsic;
use vilan_core::id::Id;
use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

mod scratch;

fn toolchain() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan")
}

/// The program every pin below runs. `remove(0)` on `[1, 2, 3]` prints the element
/// it took and then the length that is left — which is `2` when the element really
/// came out and `3` when the substituted body only READ it. One run separates the
/// intrinsic from the body without reading a single emitted identifier.
const PROGRAM: &str = r#"
    import std::io::print;
    fun main() {
        mut xs = [1, 2, 3];
        print(xs.remove(0));
        print(xs.len());
    }
    "#;

/// A scratch toolchain (`std` + the `macro_std` beside it, which std's macros
/// require) with `std::list`'s `remove` declaration replaced by `replacement`.
/// Returns the std spec to compile against, and the directory to clean up.
fn scratch_toolchain(tag: &str, replacement: &str) -> (PackageSpec, PathBuf) {
    let root = scratch::root().join(format!("vilan-b265-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    copy_tree(&toolchain().join("std"), &root.join("std"));
    copy_tree(&toolchain().join("macro_std"), &root.join("macro_std"));
    let list = root.join("std/src/list.vl");
    let text = std::fs::read_to_string(&list).expect("the scratch std's list.vl");
    let declaration = "external fun remove(&mut self, index: i32): T;";
    assert!(
        text.contains(declaration),
        "std's `List::remove` is no longer the declaration these pins rewrite"
    );
    std::fs::write(&list, text.replace(declaration, replacement)).expect("rewrite list.vl");
    (vilan_core::manifest::resolve_std(&root.join("std")), root)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create the scratch directory");
    for entry in std::fs::read_dir(from).expect("read the toolchain") {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a file type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy a toolchain file");
        }
    }
}

/// Compile `PROGRAM` against `spec` on a large-stack worker, matching the CLI.
fn compile_against(spec: &PackageSpec) -> Result<String, Vec<String>> {
    let spec = spec.clone();
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                PROGRAM,
                &spec,
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).map_err(|error| vec![error.msg])
                }
                _ => Err(errors.into_iter().map(|error| error.msg).collect()),
            }
        })
        .expect("spawn the compile worker")
        .join()
        .expect("the compile worker finished")
}

fn run(js: &str, tag: &str) -> String {
    let path = scratch::root().join(format!("vilan-b265-{tag}-{}.mjs", std::process::id()));
    std::fs::write(&path, js).expect("write the emitted program");
    let output = std::process::Command::new("node")
        .arg(&path)
        .output()
        .expect("run node");
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "the emitted program failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn b265_a_bodied_list_remove_in_a_scratch_std_keeps_its_body() {
    // The exhibit. The substituted body deliberately does NOT remove — it reads the
    // element and leaves the list alone — so the intrinsic and the body disagree
    // about the length, and a run says which one shipped. Before this, `splice` was
    // emitted and the body never ran.
    let (spec, root) = scratch_toolchain(
        "bodied",
        "fun remove(&mut self, index: i32): T {\n\t\tself.get(index).unwrap()\n\t}",
    );
    let js = compile_against(&spec).expect("the scratch std compiles");
    assert!(
        !js.contains("splice"),
        "the intrinsic replaced a bodied `List::remove`:\n{js}"
    );
    assert_eq!(run(&js, "bodied"), "1\n3\n", "the vilan body did not run");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn b265_an_external_list_remove_is_still_the_intrinsic() {
    // The control, and the reason the gate is externality rather than a refusal:
    // std's own declaration is an `external fun`, so the lowering is exactly where
    // M47 left it. Rewritten to itself, so the scratch tree is the only difference
    // between this pin and the one above.
    let (spec, root) =
        scratch_toolchain("external", "external fun remove(&mut self, index: i32): T;");
    let js = compile_against(&spec).expect("the scratch std compiles");
    assert!(
        js.contains("splice"),
        "std's `external fun remove` stopped lowering to the intrinsic:\n{js}"
    );
    assert_eq!(run(&js, "external"), "1\n2\n");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn b265_a_bodied_remove_on_another_type_is_untouched() {
    // The other direction, unchanged and pinned so it stays that way: the tables
    // are keyed on the nominal subject as well as the name, so a `remove` on a
    // type that is not `List` was never a candidate.
    let js = compile_against(&vilan_core::manifest::resolve_std(&toolchain().join("std")))
        .expect("the real std compiles");
    assert!(
        js.contains("splice"),
        "the control program lost its intrinsic"
    );

    let source = r#"
        import std::io::print;
        struct Bag { items: List<i32> }
        impl Bag {
            fun remove(&mut self, index: i32): i32 {
                let taken = self.items.get(index).unwrap();
                print("bag");
                taken
            }
        }
        fun main() {
            mut bag = Bag { items = [1, 2, 3] };
            print(bag.remove(0));
            print(bag.items.len());
        }
        "#;
    let spec = vilan_core::manifest::resolve_std(&toolchain().join("std"));
    let js = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &spec,
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            match program {
                Some(program) if errors.is_empty() => {
                    transform(&program, &BuildOptions::default()).map_err(|error| vec![error.msg])
                }
                _ => Err(errors.into_iter().map(|error| error.msg).collect()),
            }
        })
        .expect("spawn the compile worker")
        .join()
        .expect("the compile worker finished")
        .expect("the user program compiles");
    assert_eq!(run(&js, "bag"), "bag\n1\n3\n");
}

/// N109: the same table is what binds `List::new` and `List::push`.
///
/// Until N109 these two were the compiler's only lowered externals OUTSIDE
/// `Program::intrinsics` — two `Program` fields keyed by function id, with a
/// dedicated `if Some(target) == self.list_new_fn_id` arm in the JS emitter's
/// named-callee path and, since B359, a second pair of arms for the dispatch
/// path beside it. B359 is what that costs: a DISPATCH resolving to one had no
/// arm at all, fell through to the emitted-function name and minted a mangled
/// name for a function nothing emits, so a trait default's `self.push(v)` over
/// a `List` compiled clean and threw `ReferenceError` at runtime.
///
/// So the invariant is the pin: a compiler-lowered external is a ROW. Both are
/// rows now, the rows are unique, and the two surviving `Program` fields —
/// which exist only because `vilan-rust` still recognizes the pair by id —
/// name exactly the ids those rows are filed under, so the field cannot drift
/// from the table it is read out of.
struct ListRows {
    new_rows: Vec<Id>,
    push_rows: Vec<Id>,
    new_field: Option<Id>,
    push_field: Option<Id>,
    javascript: String,
}

fn list_rows(source: &'static str) -> ListRows {
    let spec = vilan_core::manifest::resolve_std(&toolchain().join("std"));
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let (program, errors) = analyze_source(
                source,
                &spec,
                Path::new("."),
                Path::new("test.vl"),
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = program.expect("the probe program analyzes");
            assert!(
                errors.is_empty(),
                "the probe program must be clean: {:?}",
                errors
                    .into_iter()
                    .map(|error| error.msg)
                    .collect::<Vec<_>>()
            );
            let rows = |wanted: &dyn Fn(&Intrinsic) -> bool| -> Vec<Id> {
                let mut found: Vec<Id> = program
                    .intrinsics
                    .iter()
                    .filter(|(_, intrinsic)| wanted(intrinsic))
                    .map(|(id, _)| *id)
                    .collect();
                found.sort_by_key(|id| id.0);
                found
            };
            ListRows {
                new_rows: rows(&|intrinsic| matches!(intrinsic, Intrinsic::ListNew)),
                push_rows: rows(&|intrinsic| matches!(intrinsic, Intrinsic::ListPush)),
                new_field: program.list_new_fn_id,
                push_field: program.list_push_fn_id,
                javascript: transform(&program, &BuildOptions::default())
                    .expect("the probe program emits"),
            }
        })
        .expect("spawn the compile worker")
        .join()
        .expect("the compile worker finished")
}

#[test]
fn n109_list_new_and_push_are_intrinsic_rows_like_every_other_lowering() {
    // Every spelling of the pair in one program: the named constructor, the
    // literal (which is the same lowering arrived at through the elements
    // path), and a `push` on each.
    let rows = list_rows(
        r#"
        import std::io::print;
        fun main() {
            mut named: List<i32> = List::new();
            named.push(1);
            mut literal: List<i32> = [];
            literal.push(2);
            print(named.len() + literal.len());
        }
        "#,
    );

    assert_eq!(
        rows.new_rows.len(),
        1,
        "`List::new` must be exactly one `Intrinsic::ListNew` row, not a \
         `Program` field the emitters have to remember: {:?}",
        rows.new_rows
    );
    assert_eq!(
        rows.push_rows.len(),
        1,
        "`List::push` must be exactly one `Intrinsic::ListPush` row: {:?}",
        rows.push_rows
    );
    assert_eq!(
        rows.new_field,
        Some(rows.new_rows[0]),
        "`Program::list_new_fn_id` must name the id its row is filed under — \
         `vilan-rust` reads the field and the JS emitter reads the row, and \
         the two answering differently is the whole defect class"
    );
    assert_eq!(
        rows.push_field,
        Some(rows.push_rows[0]),
        "`Program::list_push_fn_id` must name the id its row is filed under"
    );

    // And the lowering is unchanged: the array literal and the host method,
    // with no emitted function standing in for either.
    assert!(
        rows.javascript.contains(".push(1)") && rows.javascript.contains(".push(2)"),
        "`push` must lower to the host method:\n{}",
        rows.javascript
    );
    assert_eq!(
        rows.javascript.matches("= [  ];").count(),
        2,
        "both constructors must lower to an empty array literal (the printer \
         spaces an empty one as `[  ]`):\n{}",
        rows.javascript
    );
    assert_eq!(run(&rows.javascript, "n109"), "2\n");
}
