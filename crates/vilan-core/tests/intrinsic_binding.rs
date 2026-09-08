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

use vilan_core::{BuildOptions, PackageSpec, Platform, Workspace, analyze_source, transform};

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
    let root = std::env::temp_dir().join(format!("vilan-b265-{tag}-{}", std::process::id()));
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
    let path = std::env::temp_dir().join(format!("vilan-b265-{tag}-{}.mjs", std::process::id()));
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
