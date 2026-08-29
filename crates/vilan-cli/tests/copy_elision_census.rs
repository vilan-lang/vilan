//! The copy-elision census: how many rule-1 deep copies the corpus's emitted
//! JavaScript still carries, per program, committed as a table.
//!
//! **Why a count and not just the byte gate.** `corpus.rs` already holds every
//! `.mjs` golden byte-identical, so a change in elision cannot ship
//! unnoticed — but it arrives as an unlabelled byte diff, and elision is
//! exactly the property where the diff and the meaning come apart: a golden
//! that loses a `__clone` is a win, a golden that GAINS one is a regression in
//! the last-use dataflow (`analyzer/liveness.rs`, `lifetimes.md` §6/S2), and
//! the byte gate says the same thing about both. This table names the number.
//! `lifetimes.md` §11 asks for it by name as S2's test plan: "elision count
//! golden over the corpus".
//!
//! **What is counted.** Calls to the emitted `__clone` helper — one per copy
//! rule 1 required and rule 2 did not elide. The helper's own three recursive
//! calls are subtracted where it is emitted at all, so the number is call
//! SITES in the program, and a program that elides its last copy drops the
//! helper entirely and lands on 0.
//!
//! **Where the number comes from.** The committed goldens, not a fresh build —
//! `corpus.rs` is what proves those goldens ARE a fresh build, and reading them
//! costs milliseconds where rebuilding the corpus costs a minute. The two gates
//! are therefore honest together and neither duplicates the other's work.
//!
//! Regenerate deliberately, after reading the byte diff:
//!
//! ```text
//! VILAN_REGENERATE_COPY_ELISION_CENSUS=1 cargo test -p vilan-cli --test copy_elision_census
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The committed table. It sits beside its gate rather than in `vilan/test/`,
/// the way `mime-table.tsv` does: the corpus directory is staged wholesale by
/// `corpus.rs`, and a dataset that describes the corpus is not part of it.
const CENSUS: &str = "crates/vilan-cli/tests/copy-elision-census.tsv";

/// The head of the emitted deep-copy helper. Its body makes three parenthesised
/// recursive calls, which are the helper's own text and not copy sites.
const HELPER_HEAD: &str = "function __clone(value) {";
const HELPER_INTERNAL_CALLS: usize = 3;

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves")
}

/// Copy call sites in one emitted program.
fn copy_sites(emitted: &str) -> usize {
    let calls = emitted.matches("__clone(").count();
    if emitted.contains(HELPER_HEAD) {
        calls - HELPER_INTERNAL_CALLS
    } else {
        calls
    }
}

/// The census as the goldens have it, keyed by program name.
fn measured() -> BTreeMap<String, usize> {
    let corpus = repository_root().join("vilan/test");
    let mut census = BTreeMap::new();
    for entry in std::fs::read_dir(&corpus).expect("corpus directory") {
        let path = entry.expect("corpus entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("mjs") {
            continue;
        }
        let name = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("a golden has a name")
            .to_string();
        let emitted = std::fs::read_to_string(&path).expect("read a golden");
        census.insert(name, copy_sites(&emitted));
    }
    census
}

fn render(census: &BTreeMap<String, usize>) -> String {
    let total: usize = census.values().sum();
    let mut text = String::from(
        "# Emitted `__clone` call sites per corpus golden — the copy-elision\n\
         # census (lifetimes.md §11, slice S2). Regenerate with\n\
         # VILAN_REGENERATE_COPY_ELISION_CENSUS=1 cargo test -p vilan-cli \
         --test copy_elision_census\n",
    );
    text.push_str(&format!("# total\t{total}\n"));
    for (program, sites) in census {
        text.push_str(&format!("{program}\t{sites}\n"));
    }
    text
}

fn parse(text: &str) -> BTreeMap<String, usize> {
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let (program, sites) = line
                .split_once('\t')
                .unwrap_or_else(|| panic!("a census row is `program<TAB>count`: {line:?}"));
            (
                program.to_string(),
                sites.trim().parse().expect("a census count is a number"),
            )
        })
        .collect()
}

#[test]
fn the_corpus_copy_census_is_current() {
    let census = measured();
    let path = repository_root().join(CENSUS);
    let rendered = render(&census);
    if std::env::var_os("VILAN_REGENERATE_COPY_ELISION_CENSUS").is_some() {
        std::fs::write(&path, &rendered).expect("write the census");
        return;
    }
    let committed = std::fs::read_to_string(&path).expect("read the committed census");
    if committed == rendered {
        return;
    }
    // Report the movement, not the file diff: which way, and by how much.
    let expected = parse(&committed);
    let mut moved: Vec<String> = Vec::new();
    for (program, sites) in &census {
        match expected.get(program) {
            Some(before) if before == sites => {}
            Some(before) => moved.push(format!(
                "{program}: {before} -> {sites} ({})",
                if sites > before {
                    "REGRESSED — a copy the dataflow used to elide came back"
                } else {
                    "improved"
                }
            )),
            None => moved.push(format!("{program}: new golden, {sites} copies")),
        }
    }
    for program in expected.keys() {
        if !census.contains_key(program) {
            moved.push(format!("{program}: golden removed"));
        }
    }
    let before: usize = expected.values().sum();
    let after: usize = census.values().sum();
    panic!(
        "the copy-elision census moved (total {before} -> {after}). Read the corpus byte diff \
         first — a copy that came BACK is a last-use dataflow regression, not a golden to \
         regenerate. Then, deliberately:\n  \
         VILAN_REGENERATE_COPY_ELISION_CENSUS=1 cargo test -p vilan-cli --test \
         copy_elision_census\n{}",
        moved.join("\n")
    );
}

/// The counter itself, on text rather than on the corpus: the helper's own
/// recursion must not be mistaken for the copies a program performs.
#[test]
fn the_counter_does_not_count_the_helper_as_a_copy_site() {
    let helper = "function __clone(value) {\n\
                  \tif (Array.isArray(value)) return value.map(__clone);\n\
                  \tif (value instanceof Set) return new Set([ ...value ].map(__clone));\n\
                  \tif (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => \
                  [ __clone(k), __clone(v) ]));\n\
                  \treturn value;\n\
                  }\n";
    assert_eq!(
        copy_sites(helper),
        0,
        "the helper alone is not a copy site (its three internal calls are its own text)"
    );
    assert_eq!(
        copy_sites(&format!("{helper}const a = [ __clone(b) ];\n")),
        1,
        "one real call site beside the helper counts as one"
    );
    assert_eq!(
        copy_sites("const a = [ __clone(b), __clone(c) ];\n"),
        2,
        "a program with no helper counts every call"
    );
}
