//! The PERMUTATION DIFFERENTIAL (`analyzer-pass-map.md` §6, Q4; M110).
//!
//! The edit-replay differential proves that an incremental analysis answers
//! what a clean one does. It cannot prove that the clean analysis is itself
//! independent of LOAD ORDER: both of its legs share the drain's canonical
//! order (`load_order_key`: a package's modules by name), the two-phase
//! resolve and every walk over `modules` or `implementations`, so an answer
//! chosen by a module's position — B554's blame, M128's first-match sites, a
//! top-level-only walk — is invisible to it by construction. This binary is
//! the missing gate: **the same package, analyzed under a permutation of its
//! file names, answers what the canonical package answers**, on an
//! observation rendered without ids.
//!
//! Two permutations, and their composition: every module renamed so the
//! name order — and with it the load order, every entity id and every walk
//! — is REVERSED (a consistent alpha-renaming of the whole package, so the
//! program means the same); every module nested a directory deeper
//! (`pkg::deep::name`, the walks over nested modules B560/B561/E267 found
//! wanting). The observation is `render_observation`'s — diagnostics, labels,
//! resolutions, type references — normalized back to the canonical spelling
//! (file names, byte offsets through the renaming, the messages' identifiers)
//! and sorted within each section, because the PUBLISHED order of
//! diagnostics is by id, which is a ruled positional answer (the C1 rule) and
//! the one thing a reversed world is allowed to change.
//!
//! The corpus is the edit-replay differential's: every fixture package in
//! every state its script puts it in (the §6 classes, M121's served impls,
//! B553's entry impl, B573's twins, the re-walk pins' leaf, the post-pass
//! packages), and the corpus leg's programs re-hosted as modules. B554 is the
//! known red, pinned ignored until the element-slot rule blames the
//! declaration.

mod replay_harness;
mod scratch;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use replay_harness::packages::*;
use vilan_core::Platform;

/// A permutation of a package's module layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permutation {
    /// Every module renamed so the modules' name order reverses: the drain
    /// loads them back to front, every id is minted in the other order, and
    /// every first match over a load-ordered table meets the other candidate
    /// first.
    ReversedNames,
    /// Every module moved a directory deeper (`deep/name.vl`, addressed as
    /// `pkg::deep::name`): the load order stands, the module tree gains a
    /// level.
    NestedDeeper,
    /// Both.
    NestedReversed,
}

const PERMUTATIONS: [Permutation; 3] = [
    Permutation::ReversedNames,
    Permutation::NestedDeeper,
    Permutation::NestedReversed,
];

/// The directory the nesting permutation puts every module under.
const DEEP: &str = "deep";

/// A package's files under a permutation, with what maps them back.
struct Permuted {
    files: Vec<(String, String)>,
    /// `(permuted identifier, canonical identifier)` — the module renames.
    renames: HashMap<String, String>,
    nested: bool,
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// `text` with every maximal identifier run that `map` names replaced — the
/// consistent alpha-renaming the permutation is, applied forward (canonical →
/// permuted) and back.
fn rename_identifiers(text: &str, map: &HashMap<String, String>) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while at < bytes.len() {
        if is_identifier_byte(bytes[at]) {
            let start = at;
            while at < bytes.len() && is_identifier_byte(bytes[at]) {
                at += 1;
            }
            let word = &text[start..at];
            match map.get(word) {
                Some(renamed) => out.push_str(renamed),
                None => out.push_str(word),
            }
        } else {
            // A non-identifier byte; copy the whole UTF-8 character it starts.
            let character = text[at..].chars().next().unwrap();
            out.push(character);
            at += character.len_utf8();
        }
    }
    out
}

/// The module a package file is, or `None` for the entry and for data files.
fn module_name(file: &str) -> Option<&str> {
    if file == "main.vl" {
        return None;
    }
    file.strip_suffix(".vl")
}

/// `files` under `permutation`. `opaque` names the files whose TEXT the
/// renaming must not touch (the corpus leg's hosted program, which never
/// names its package's modules and whose identifiers are its own); their
/// names still move. `None` when the permutation cannot be applied: an opaque
/// file that spells `pkg::` would not survive the nesting.
fn permute(
    files: &[(String, String)],
    opaque: &[&str],
    permutation: Permutation,
) -> Option<Permuted> {
    let reversed = matches!(
        permutation,
        Permutation::ReversedNames | Permutation::NestedReversed
    );
    let nested = matches!(
        permutation,
        Permutation::NestedDeeper | Permutation::NestedReversed
    );
    let mut forward: HashMap<String, String> = HashMap::new();
    if reversed {
        let mut names: Vec<&str> = files
            .iter()
            .filter_map(|(file, _)| module_name(file))
            .collect();
        names.sort_unstable();
        let count = names.len();
        for (index, name) in names.iter().enumerate() {
            // The i-th name in ascending order takes the i-th prefix in
            // DESCENDING order, so the permuted names sort back to front:
            // `pz_a`, `py_b`, … — two letters when a package has more than
            // twenty-six modules.
            let rank = count - 1 - index;
            let prefix = if count <= 26 {
                format!("p{}_", (b'a' + rank as u8) as char)
            } else {
                format!(
                    "p{}{}_",
                    (b'a' + (rank / 26) as u8) as char,
                    (b'a' + (rank % 26) as u8) as char
                )
            };
            forward.insert(name.to_string(), format!("{prefix}{name}"));
        }
        for (file, text) in files {
            for permuted in forward.values() {
                assert!(
                    !text.contains(permuted.as_str()),
                    "{file} already spells {permuted}; the renaming would not be invertible"
                );
            }
        }
    }
    let renamed_files: Vec<(String, String)> = files
        .iter()
        .map(|(file, text)| {
            let name = match module_name(file) {
                Some(name) => format!(
                    "{}.vl",
                    forward
                        .get(name)
                        .cloned()
                        .unwrap_or_else(|| name.to_string())
                ),
                None => file.clone(),
            };
            let name = if nested && module_name(file).is_some() {
                format!("{DEEP}/{name}")
            } else {
                name
            };
            let mut text = text.clone();
            if !opaque.contains(&file.as_str()) && file.ends_with(".vl") {
                if reversed {
                    text = rename_identifiers(&text, &forward);
                }
                if nested {
                    text = text.replace("pkg::", &format!("pkg::{DEEP}::"));
                }
            }
            (name, text)
        })
        .collect();
    if nested
        && files
            .iter()
            .any(|(file, text)| opaque.contains(&file.as_str()) && text.contains("pkg::"))
    {
        return None;
    }
    Some(Permuted {
        files: renamed_files,
        renames: forward
            .into_iter()
            .map(|(canonical, permuted)| (permuted, canonical))
            .collect(),
        nested,
    })
}

impl Permuted {
    /// The canonical spelling of `text` rendered under this permutation: the
    /// renaming undone, the nesting prefix dropped.
    fn canonical(&self, text: &str) -> String {
        let mut text = if self.renames.is_empty() {
            text.to_string()
        } else {
            rename_identifiers(text, &self.renames)
        };
        if self.nested {
            text = text.replace(&format!("pkg::{DEEP}::"), "pkg::");
        }
        text
    }

    /// The canonical file a permuted package-relative path is.
    fn canonical_file(&self, file: &str) -> String {
        let file = if self.nested {
            file.strip_prefix(&format!("{DEEP}/")).unwrap_or(file)
        } else {
            file
        };
        match file.strip_suffix(".vl") {
            Some(stem) => format!(
                "{}.vl",
                self.renames.get(stem).map(String::as_str).unwrap_or(stem)
            ),
            None => file.to_string(),
        }
    }

    /// The permuted `file`'s text under `start..end`.
    fn text_under(&self, file: &str, start: usize, end: usize) -> Option<&str> {
        let (_, text) = self.files.iter().find(|(name, _)| name == file)?;
        text.get(start..end)
    }

    /// The canonical byte offset of `offset` into the permuted `file`: the
    /// length of the prefix once its renamed identifiers are spelled
    /// canonically again.
    fn canonical_offset(&self, file: &str, offset: usize) -> usize {
        let Some((_, text)) = self.files.iter().find(|(name, _)| name == file) else {
            return offset;
        };
        let offset = offset.min(text.len());
        let mut cut = offset;
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        let prefix = &text[..cut];
        self.canonical(prefix).len() + (offset - cut)
    }
}

/// A diagnostic row with its trace hops sorted. A requirement trace (E78)
/// orders its hops by depth and, at one depth, by id — the C1 rule, a ruled
/// positional answer the reversed world is allowed to change — so the
/// comparison reads a trace as the SET of its hops, each with its location
/// and label, and leaves the depth order to the clean analysis.
fn sort_trace_hops(row: &str) -> String {
    if !row.contains(" | trace ") {
        return row.to_string();
    }
    let mut segments = row.split(" | ");
    let mut kept: Vec<&str> = vec![segments.next().unwrap_or("")];
    let mut hops: Vec<&str> = Vec::new();
    for segment in segments {
        if segment.starts_with("trace ") {
            hops.push(segment);
        } else {
            kept.push(segment);
        }
    }
    hops.sort_unstable();
    kept.extend(hops);
    kept.join(" | ")
}

/// `rendering` (what [`observe`] rendered for a package written at `root`)
/// in canonical spelling: paths relative to the package, every offset and
/// identifier mapped back through `permuted` when there is one, and the rows
/// of each section sorted.
fn normalize(rendering: &str, root: &Path, permuted: Option<&Permuted>) -> String {
    let root = format!("{}/", root.display());
    let mut out = String::new();
    let mut section: Vec<String> = Vec::new();
    let flush = |section: &mut Vec<String>, out: &mut String| {
        section.sort_unstable();
        section.dedup();
        for row in section.drain(..) {
            out.push_str(&row);
            out.push('\n');
        }
    };
    for line in rendering.lines() {
        if line.starts_with("# ") {
            flush(&mut section, &mut out);
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // Every package location in a row is `<root>/<file> <start>..<end>`;
        // the file is mapped back and the range through the file's text. Both
        // tokens are space-free (the scratch root has no spaces; a message
        // after them may).
        let tokens: Vec<&str> = line.split(' ').collect();
        let mut rebuilt: Vec<String> = Vec::with_capacity(tokens.len());
        let mut index = 0;
        let mut names_the_nesting = false;
        while index < tokens.len() {
            let token = tokens[index];
            if let Some(file) = token.strip_prefix(root.as_str()) {
                let range = tokens.get(index + 1).copied().unwrap_or("");
                let (file, range) = match permuted {
                    Some(permuted) => {
                        let parsed = range.split_once("..").and_then(|(start, end)| {
                            Some((start.parse::<usize>().ok()?, end.parse::<usize>().ok()?))
                        });
                        // The nesting permutation adds a path segment the
                        // canonical package has no row for: the reference
                        // `deep` in every `pkg::deep::…` resolves to the
                        // namespace module. Those rows are the permutation's
                        // own, not the package's.
                        if let Some((start, end)) = parsed
                            && permuted.nested
                            && index == 1
                            && permuted.text_under(file, start, end) == Some(DEEP)
                        {
                            names_the_nesting = true;
                        }
                        let mapped = parsed
                            .map(|(start, end)| {
                                format!(
                                    "{}..{}",
                                    permuted.canonical_offset(file, start),
                                    permuted.canonical_offset(file, end)
                                )
                            })
                            .unwrap_or_else(|| range.to_string());
                        (permuted.canonical_file(file), mapped)
                    }
                    None => (file.to_string(), range.to_string()),
                };
                rebuilt.push(format!("<pkg>/{file}"));
                if index + 1 < tokens.len() {
                    rebuilt.push(range);
                }
                index += 2;
            } else {
                rebuilt.push(token.to_string());
                index += 1;
            }
        }
        if names_the_nesting && line.starts_with("type-reference ") {
            continue;
        }
        // A message may spell the package's directory in prose too (a const
        // read's resolved path): every occurrence maps to the same spelling.
        let row = sort_trace_hops(&rebuilt.join(" ").replace(root.as_str(), "<pkg>/"));
        section.push(match permuted {
            Some(permuted) => permuted.canonical(&row),
            None => row,
        });
    }
    flush(&mut section, &mut out);
    out
}

/// The clean analysis of a package written from `files`, rendered.
fn observe_clean(name: &str, platform: Platform, files: &[(String, String)]) -> (String, PathBuf) {
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(file, text)| (file.as_str(), text.as_str()))
        .collect();
    let package = Package::write(name, platform, &borrowed);
    let observation = observe(&package, Vec::new(), Leg::Clean);
    let root = package.directory.clone();
    package.remove();
    (observation.rendering, root)
}

/// The first row where the canonical rendering and a permuted one part.
fn first_difference(canonical: &str, permuted: &str) -> String {
    let left: Vec<&str> = canonical.lines().collect();
    let right: Vec<&str> = permuted.lines().collect();
    let at = left
        .iter()
        .zip(&right)
        .position(|(a, b)| a != b)
        .unwrap_or(left.len().min(right.len()));
    format!(
        "row {at}:\n    canonical: {:?}\n    permuted:  {:?}\n    ({} rows against {})",
        left.get(at),
        right.get(at),
        left.len(),
        right.len()
    )
}

/// One state of one package under one permutation: the divergence, if any.
fn compare(
    label: &str,
    platform: Platform,
    files: &[(String, String)],
    opaque: &[&str],
    permutation: Permutation,
) -> Option<String> {
    let name = label
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .take(40)
        .collect::<String>();
    let (canonical, root) = observe_clean(&format!("perm_{name}"), platform, files);
    let canonical = normalize(&canonical, &root, None);
    let permuted = permute(files, opaque, permutation)?;
    let (rendering, root) = observe_clean(
        &format!("perm_{name}_{permutation:?}"),
        platform,
        &permuted.files,
    );
    let rendering = normalize(&rendering, &root, Some(&permuted));
    (canonical != rendering).then(|| {
        format!(
            "{label} under {permutation:?}: the permuted analysis differs from the canonical \
             one at {}",
            first_difference(&canonical, &rendering)
        )
    })
}

/// Every state of `fixture`, under every permutation; `skip` names the states
/// (by a substring of the edit's label) the gate leaves to a pin of their own.
fn compare_fixture(fixture: &Fixture, skip: &[&str]) -> Vec<String> {
    let mut divergences = Vec::new();
    for (label, files) in fixture.states() {
        if skip.iter().any(|needle| label.contains(needle)) {
            continue;
        }
        for permutation in PERMUTATIONS {
            divergences.extend(compare(&label, fixture.platform, &files, &[], permutation));
        }
    }
    divergences
}

/// B554's state of the classes package: the module loaded first pushes a
/// different type into another module's binding, and the blame follows the
/// load order. Its own pin below.
const B554_CLASS_EDIT: &str = "from the module loaded first";

/// **The gate, the classes leg.** The §6 classes package in every state its
/// script puts it in, and M121's served impls, under every permutation.
#[test]
fn the_classes_package_answers_the_same_under_every_permutation() {
    let mut divergences = compare_fixture(&CLASSES_FIXTURE, &[B554_CLASS_EDIT]);
    divergences.extend(compare_fixture(&SERVED_IMPL_FIXTURE, &[]));
    assert!(
        divergences.is_empty(),
        "{} state(s) of the classes package depend on the modules' load order:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}

/// **The gate, the other fixtures.** B553's entry impl, B573's browser twins,
/// the re-walk pins' leaf package, the four post-pass packages and M128's
/// four first-match sites (two candidates in different load positions), in
/// every state, under every permutation.
#[test]
fn every_other_fixture_answers_the_same_under_every_permutation() {
    let mut divergences = Vec::new();
    for fixture in [&ENTRY_IMPL_FIXTURE, &PLATFORM_FIXTURE, &LEAF_FIXTURE] {
        divergences.extend(compare_fixture(fixture, &[]));
    }
    for fixture in POST_PASS_FIXTURES.iter().chain(M128_FIXTURES) {
        divergences.extend(compare_fixture(fixture, &[]));
    }
    assert!(
        divergences.is_empty(),
        "{} state(s) depend on the modules' load order:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}

/// **The gate, the corpus leg.** Every corpus program re-hosted as the module
/// of a package with an importer and a bystander (the edit-replay
/// differential's hosting), under every permutation. The program's own text is
/// opaque to the renaming: it never names the package's modules.
#[test]
fn the_corpus_as_modules_answers_the_same_under_every_permutation() {
    let corpus = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/test");
    let mut paths: Vec<PathBuf> = std::fs::read_dir(&corpus)
        .expect("corpus directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            (path.extension()? == "vl").then_some(path)
        })
        .collect();
    paths.sort();
    assert!(paths.len() > 60, "suspiciously few corpus programs");
    let divergences: Vec<String> = std::thread::scope(|scope| {
        let workers: Vec<_> = paths
            .chunks(paths.len().div_ceil(16).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    let mut divergences = Vec::new();
                    for path in chunk {
                        let mut source = std::fs::read_to_string(path).expect("read corpus file");
                        source.push_str(PROBE_REFUSAL);
                        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
                        let files: Vec<(String, String)> = vec![
                            ("main.vl".to_string(), CORPUS_MAIN.to_string()),
                            ("module.vl".to_string(), source),
                            ("user.vl".to_string(), CORPUS_USER.to_string()),
                            ("side.vl".to_string(), CORPUS_SIDE.to_string()),
                        ];
                        for permutation in PERMUTATIONS {
                            divergences.extend(compare(
                                &format!("corpus {name}"),
                                Platform::default(),
                                &files,
                                &["module.vl"],
                                permutation,
                            ));
                        }
                    }
                    divergences
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| worker.join().expect("worker panicked"))
            .collect()
    });
    assert!(
        divergences.is_empty(),
        "{} corpus state(s) depend on the modules' load order:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}

/// **B554, the known red.** Two modules push `1` and `"two"` into one module
/// binding; the blame lands on whichever push's module loads later. The
/// invariant (pass map §5.2): a diagnostic's location is a function of the
/// program, not of the file names — the conflict is reported at the binding's
/// declaration, or names every push.
#[test]
#[ignore = "B554: an element-type conflict on a module binding is blamed on whichever push's module loads later in name order"]
fn b554_an_element_conflicts_blame_is_a_fact_about_the_program() {
    let mut divergences = compare_fixture(&B554_FIXTURE, &[]);
    let (label, files) = CLASSES_FIXTURE
        .states()
        .into_iter()
        .find(|(label, _)| label.contains(B554_CLASS_EDIT))
        .expect("the classes script has B554's edit");
    for permutation in PERMUTATIONS {
        divergences.extend(compare(&label, NODE, &files, &[], permutation));
    }
    assert!(
        divergences.is_empty(),
        "{} state(s) blame by file name:\n{}",
        divergences.len(),
        divergences.join("\n")
    );
}

/// Non-vacuity: the normalization must not hide a changed answer. A permuted
/// package whose text also moves a written return type has to part from the
/// canonical one — and the permutation alone must not.
#[test]
fn the_differential_sees_a_changed_answer_through_the_normalization() {
    let (label, files) = &CLASSES_FIXTURE.states()[0];
    for permutation in PERMUTATIONS {
        if let Some(divergence) = compare(label, NODE, files, &[], permutation) {
            panic!("the base classes package must agree with itself: {divergence}");
        }
    }
    let (canonical, root) = observe_clean("perm_plant", NODE, files);
    let canonical = normalize(&canonical, &root, None);
    let mut permuted = permute(files, &[], Permutation::NestedReversed).expect("permutable");
    let model = permuted
        .files
        .iter_mut()
        .find(|(file, _)| file.ends_with("model.vl"))
        .expect("the model module");
    assert!(model.1.contains("export fun size(): i32 {"));
    model.1 = model
        .1
        .replace("export fun size(): i32 {", "export fun size(): i53 {");
    let (rendering, root) = observe_clean("perm_plant_moved", NODE, &permuted.files);
    let rendering = normalize(&rendering, &root, Some(&permuted));
    assert_ne!(
        canonical, rendering,
        "a return type moved in the permuted package must show through the normalization"
    );
    // And the renaming round-trips: the permuted texts, spelled canonically
    // again, are the canonical texts.
    let permuted = permute(files, &[], Permutation::NestedReversed).expect("permutable");
    for (file, text) in &permuted.files {
        let canonical_file = permuted.canonical_file(file);
        let original = files
            .iter()
            .find(|(name, _)| *name == canonical_file)
            .unwrap_or_else(|| panic!("{file} maps back to {canonical_file}, which is no file"));
        assert_eq!(
            permuted.canonical(text),
            original.1,
            "{file}'s text does not round-trip"
        );
    }
}
