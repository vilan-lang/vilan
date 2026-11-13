//! The generated Lucide package, `vilan/packages/lucide` (gfx-path.md S4,
//! ruling R11: icon sets ship as generated packages outside std).
//!
//! Two claims, each its own pin:
//!
//! 1. **The source is the generator's.** `src/icons/lib.vl` is what
//!    `scripts/lucide-native.mjs render` makes of the package's data file,
//!    `icons.tsv`, byte for byte: neither moves alone. (The first step, the
//!    upstream SVG set to the data, needs the pinned tarball, which the
//!    repository does not carry; regenerating at the pinned version is a
//!    no-op, and the package's README says how. The GPU crate's
//!    `lucide_sweep` holds the data's names to the source's and draws every
//!    icon.)
//! 2. **It checks clean, on both backends.** `vilan check` of the package's
//!    `src/lib.vl`, which re-exports every icon, so all 1,999 functions are
//!    analysed. (`vilan check` of the package DIRECTORY checks a
//!    `[library]`'s platform contract only, not its modules' types.)

use std::path::PathBuf;
use std::process::Command;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn package() -> PathBuf {
    repository_root()
        .join("vilan")
        .join("packages")
        .join("lucide")
}

#[test]
fn the_generated_source_is_the_generators_rendering_of_its_data() {
    let rendered = Command::new("node")
        .arg(repository_root().join("scripts").join("lucide-native.mjs"))
        .arg("render")
        .arg(package().join("icons.tsv"))
        .output()
        .expect("run node");
    assert!(
        rendered.status.success(),
        "the generator failed:\n{}",
        String::from_utf8_lossy(&rendered.stderr)
    );
    let rendered = String::from_utf8(rendered.stdout).expect("UTF-8");
    let committed = std::fs::read_to_string(package().join("src").join("icons").join("lib.vl"))
        .expect("the generated source")
        .replace("\r\n", "\n");
    assert!(
        rendered == committed,
        "src/icons/lib.vl is not the generator's rendering of icons.tsv: regenerate the \
         package (vilan/packages/lucide/README.md), never edit either by hand"
    );
    assert!(
        committed.matches("export fun ").count() > 1000,
        "the generated source declares too few icons"
    );
}

#[test]
fn the_package_checks_clean_on_both_backends() {
    let entry = package().join("src").join("lib.vl");
    for backend in ["js", "rust"] {
        let checked = Command::new(env!("CARGO_BIN_EXE_vilan"))
            .args(["check", "--backend", backend])
            .arg(&entry)
            .output()
            .expect("run vilan");
        assert!(
            checked.status.success(),
            "`vilan check --backend {backend}` of the Lucide package failed:\n{}{}",
            String::from_utf8_lossy(&checked.stdout),
            String::from_utf8_lossy(&checked.stderr)
        );
    }
}
