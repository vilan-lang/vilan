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

// --- The generator's SVG reader: comments ---------------------------------------

/// A scratch `lucide-static` package holding ONE icon, `probe`, whose SVG is
/// `svg`, at the version the generator pins (read out of the generator, so a
/// version bump does not strand this pin). Answers the generator's run over it.
fn generate_one(test: &str, svg: &str) -> std::process::Output {
    let script = repository_root().join("scripts").join("lucide-native.mjs");
    let source = std::fs::read_to_string(&script).expect("the generator");
    let pinned = |key: &str| {
        let needle = format!("{key}: \"");
        let after = source
            .split("const UPSTREAM = {")
            .nth(1)
            .and_then(|rest| rest.split(&needle).nth(1))
            .unwrap_or_else(|| panic!("the generator's UPSTREAM.{key}"));
        after.split('"').next().expect("a quoted value").to_string()
    };
    let scratch = std::env::temp_dir().join(format!(
        "vilan-lucide-{test}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    let upstream = scratch.join("package");
    std::fs::create_dir_all(upstream.join("icons")).expect("scratch");
    std::fs::write(
        upstream.join("package.json"),
        format!(
            "{{\"name\": \"{}\", \"version\": \"{}\"}}",
            pinned("name"),
            pinned("version")
        ),
    )
    .expect("package.json");
    std::fs::write(upstream.join("icon-nodes.json"), "{\"probe\": []}").expect("icon-nodes.json");
    std::fs::write(upstream.join("icons").join("probe.svg"), svg).expect("probe.svg");
    let output = Command::new("node")
        .arg(&script)
        .arg("generate")
        .arg(&upstream)
        .arg(scratch.join("out"))
        .output()
        .expect("run node");
    let _ = std::fs::remove_dir_all(&scratch);
    output
}

/// Lucide's root, as every upstream icon writes it.
const ROOT: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"24\" height=\"24\" \
     viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" \
     stroke-linecap=\"round\" stroke-linejoin=\"round\">";

/// A comment anywhere outside a tag is dropped, as upstream's licence line is.
#[test]
fn the_generator_drops_a_comment_and_reads_the_icon() {
    let svg = format!(
        "<!-- @license lucide-static - ISC -->\n{ROOT}\n  <!-- a dot -->\n  \
         <path d=\"M12 5v14\" />\n</svg>\n"
    );
    let output = generate_one("comment", &svg);
    assert!(
        output.status.success(),
        "a commented icon generates:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Removing comments in ONE pass can leave a new opener behind: the inner
/// comment of `<!<!-- a -->--` goes and a bare `<!--` remains (code scanning
/// alert 19, js/incomplete-multi-character-sanitization). The reader removes
/// to a fixed point and refuses an opener that survives, by name, where it
/// used to fall through to a later "content other than self-closing
/// elements".
#[test]
fn the_generator_refuses_a_comment_opener_that_survives_removal() {
    for (case, inside) in [
        (
            "an opener rebuilt by the removal",
            "<!<!-- a -->-- left open",
        ),
        ("a comment never closed", "<!-- never closed"),
    ] {
        let svg = format!("{ROOT}\n  {inside}\n  <path d=\"M12 5v14\" />\n</svg>\n");
        let output = generate_one("opener", &svg);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success() && stderr.contains("probe: an unterminated comment"),
            "{case}: refused by name, nothing written:\n{stderr}"
        );
    }
    // The nested form that DOES close is one comment after two passes, and the
    // icon reads: the fixed point is what makes it so.
    let svg = format!("{ROOT}\n  <!<!-- a -->-- b -->\n  <path d=\"M12 5v14\" />\n</svg>\n");
    let output = generate_one("nested", &svg);
    assert!(
        output.status.success(),
        "a comment rebuilt and then closed is removed whole:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
