//! The `Shared` census (C14 / M71): every construction site of a `Shared<T>`
//! cell in the standard library, counted per file, with the class each file's
//! cells belong to written down beside it.
//!
//! `proposal/signal-cell-representation.md` §2 classifies every declaration
//! site into four classes — **R** root-scoped (a module binding, program
//! lifetime), **O** owner-scoped (dies with a disposal boundary), **E**
//! escaping (minted under one holder, read by another), and **F** frame-scoped
//! (never outlives the call or builder that made it, and nothing subscribes to
//! it). The F class is the one that wants **no cell at all**: a closure
//! captures the *binding* (spec §6.9), so a `mut` local that sibling closures
//! in the same frame write and read is already shared storage, and the cell
//! around it is an allocation and an indirection bought for nothing.
//!
//! M71 retired the eight F cells that were reachable without a surface change
//! (see `FRAME_SCOPED_RETIRED`). This gate is what keeps the class at zero:
//! the per-file counts below are a committed census, so a new `Shared::new`
//! anywhere in std fails here until someone has classified it. A cell that is
//! genuinely R, O or E updates its file's row; a cell that is F does not get
//! written in the first place.
//!
//! The two F residues that a surface change still blocks are recorded in
//! `FRAME_SCOPED_BLOCKED` with the reason each. Neither is an oversight: both
//! are the language telling the code what shape it may have.

use std::path::{Path, PathBuf};

/// `Shared::new(` construction sites per std file, and the class those cells
/// belong to. The count is of OCCURRENCES, not lines — `binary.vl:26`
/// constructs two cells on one line.
const CENSUS: &[(&str, usize, &str)] = &[
    (
        "binary.vl",
        4,
        "F (blocked: the Wire visitor's by-value receivers)",
    ),
    ("browser/router.vl", 1, "R: the module-level `wired` latch"),
    ("browser/ui.vl", 21, "O: per-boundary row/owner bookkeeping"),
    (
        "json.vl",
        6,
        "F (blocked: the Wire visitor's by-value receivers)",
    ),
    (
        "memo.vl",
        1,
        "E: the memo cache outlives every maker's scope",
    ),
    (
        "process/fs.vl",
        1,
        "F (blocked: `Reader::next` awaits, so no `&mut self`)",
    ),
    (
        "process/rpc_server.vl",
        6,
        "R + O: the registry, the server's stats",
    ),
    ("process/ui.vl", 3, "O: the SSR request's view tree"),
    ("reactive.vl", 26, "R + O + E: turns, owners, cells, drafts"),
    (
        "rpc.vl",
        52,
        "R + O + E: sessions, wiring, the mirrors' leases",
    ),
    ("time.vl", 3, "O: the debouncer's pending/running/timer"),
    ("ws.vl", 4, "O: the frame decoder's state"),
];

/// The frame-scoped cells M71 retired, as the shape that replaced each. A
/// `mut` local captured by the closures of its own frame IS the shared
/// storage, so each of these is the cell deleted and nothing put in its place.
const FRAME_SCOPED_RETIRED: &[(&str, &str)] = &[
    ("process/rpc_server.vl", "mut settled = false;"),
    ("process/rpc_server.vl", "mut expired = false;"),
    ("process/rpc_server.vl", "mut closed = false;"),
    ("process/rpc_server.vl", "mut greeted = false;"),
    ("rpc.vl", "mut connection = \"\";"),
    ("rpc.vl", "mut refused = \"\";"),
    ("rpc.vl", "mut fault: Option<str> = None;"),
];

/// The F residue, and what blocks each. Recorded rather than asserted away:
/// the count above already pins the number, and this is why it is not zero.
const FRAME_SCOPED_BLOCKED: &[(&str, usize, &str)] = &[
    (
        "json.vl",
        6,
        "`JsonWriter`/`JsonReader` are the visitor's state, and \
         `Serialize`/`Deserialize` declare every method on a by-value `self`, \
         so the state cannot live in plain fields behind `&mut self`. Retiring \
         these six needs the trait receivers to become `&mut self` and \
         `Wire::describe`/`rebuild` to take `&mut S`/`&mut D` — a breaking \
         change to a public surface, and its own item.",
    ),
    (
        "binary.vl",
        4,
        "`BinaryWriter`/`BinaryReader`, for the same reason as `json.vl`.",
    ),
    (
        "process/fs.vl",
        1,
        "`Reader.cursor` is FORCED rather than chosen, and the type's own \
         doc-comment says so: `Reader::next` awaits the read, and a `&mut` \
         view may not be held across a suspension (spec §6.3), so a plain \
         field behind `&mut self` is not a shape this method may have.",
    ),
];

fn std_source_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std/src")
}

fn read_std(relative: &str) -> String {
    let path = std_source_dir().join(relative);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("read {path:?}: {error}"))
}

/// Every `.vl` file under `vilan/std/src`, repo-relative to that directory.
fn std_files() -> Vec<String> {
    let root = std_source_dir();
    let mut files = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).expect("read std source directory") {
            let path = entry.expect("std source entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension().is_some_and(|extension| extension == "vl") {
                let relative = path
                    .strip_prefix(&root)
                    .expect("a std path under the std root")
                    .to_string_lossy()
                    .replace('\\', "/");
                files.push(relative);
            }
        }
    }
    files.sort();
    files
}

fn construction_sites(source: &str) -> usize {
    source.matches("Shared::new(").count()
}

#[test]
fn the_shared_census_matches_the_committed_table() {
    let mut measured: Vec<(String, usize)> = Vec::new();
    for relative in std_files() {
        let count = construction_sites(&read_std(&relative));
        if count > 0 {
            measured.push((relative, count));
        }
    }

    let expected: Vec<(String, usize)> = CENSUS
        .iter()
        .map(|(file, count, _)| ((*file).to_string(), *count))
        .collect();

    assert_eq!(
        measured, expected,
        "the std `Shared` census moved. Every construction site belongs to a \
         class (proposal/signal-cell-representation.md §2): R root-scoped, O \
         owner-scoped, E escaping, F frame-scoped. A frame-scoped cell is a \
         `mut` local written the long way and does not get added; any other \
         class updates its row in CENSUS with the class named."
    );

    let total: usize = measured.iter().map(|(_, count)| count).sum();
    assert_eq!(
        total, 128,
        "the total number of `Shared` construction sites in std changed"
    );
}

#[test]
fn the_frame_scoped_class_stays_retired() {
    for (file, shape) in FRAME_SCOPED_RETIRED {
        let source = read_std(file);
        assert!(
            source.contains(shape),
            "{file} no longer spells the M71 frame-scoped binding `{shape}`. A \
             closure captures the BINDING (spec §6.9), so these are `mut` \
             locals and not cells; re-wrapping one in `Shared::new` buys an \
             allocation and an indirection for storage the frame already has."
        );
    }

    // The residue is counted, so a fix that removes one is a red row in the
    // census above rather than a silent pass here.
    for (file, blocked, reason) in FRAME_SCOPED_BLOCKED {
        let row = CENSUS
            .iter()
            .find(|(name, _, _)| name == file)
            .unwrap_or_else(|| panic!("{file} has no census row"));
        assert_eq!(
            row.1, *blocked,
            "{file}'s blocked frame-scoped residue moved ({reason})"
        );
    }
}
