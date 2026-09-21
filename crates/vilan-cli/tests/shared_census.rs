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
//! M71 retired the eight F cells that were reachable without a surface change,
//! and A108 retired the ten the Wire visitor's by-value receivers had forced
//! (see `FRAME_SCOPED_RETIRED`). This gate is what keeps the class at zero:
//! the per-file counts below are a committed census, so a new `Shared::new`
//! anywhere in std fails here until someone has classified it. A cell that is
//! genuinely R, O or E updates its file's row; a cell that is F does not get
//! written in the first place.
//!
//! **`json.vl` and `binary.vl` have no rows at all now**, which is the shape of
//! the answer: they held ten cells between them and hold none, because
//! `Serialize`/`Deserialize` take `&mut self` and `Wire::describe`/`rebuild`
//! take `&mut S`/`&mut D`, so the visitors' state lives in plain fields.
//!
//! The ONE F residue left is recorded in `FRAME_SCOPED_BLOCKED` with its
//! reason. It is not an oversight: it is the language telling the code what
//! shape it may have.

use std::path::{Path, PathBuf};

/// `Shared::new(` construction sites per std file, and the class those cells
/// belong to. The count is of OCCURRENCES, not lines — `reactive.vl` and
/// `rpc.vl` each construct two cells on one line in places.
const CENSUS: &[(&str, usize, &str)] = &[
    ("browser/router.vl", 1, "R: the module-level `wired` latch"),
    ("browser/ui.vl", 21, "O: per-boundary row/owner bookkeeping"),
    (
        "delta.vl",
        9,
        "E: the delta log's ops/version/base/cursors (twice — `new` and \
         `with_limit`) plus a cursor's own sequence. Every one of them is \
         minted by the CELL that holds the log and read by the CONSUMERS that \
         hold cursors into it, which is the E class exactly; they are A54's \
         five cells, lifted out of `rpc.vl` and spelled once per constructor \
         (A112 S1).",
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
    (
        "reactive.vl",
        29,
        "R + O + E: turns, owners, cells, drafts, and the subscriber liveness \
         flag (A110 door 1 — `observe`'s per-subscriber cell, shared with the \
         `Subscription`; `Subscription::teardown`'s own; the module-level \
         `always_live` every deferral subscriber shares)",
    ),
    (
        "rpc.vl",
        47,
        "R + O + E: sessions, wiring, the mirrors' leases. FIVE fewer since \
         A112 S1: `KeyedCell`'s own log, version, base and cursors, and its \
         cursor's sequence, are `DeltaLog`'s now (see `delta.vl`).",
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

/// The ten cells A108 retired, as the plain FIELD declaration that replaced
/// each. These were never "shared" with anything: they were a by-value `self`
/// working around itself, and the fix was the receiver.
///
/// The spellings are asserted, so a later edit that re-wraps one of these in a
/// cell fails here by name rather than only moving the count.
const VISITOR_STATE_UNBOXED: &[(&str, &str)] = &[
    ("json.vl", "\tout: str,"),
    ("json.vl", "\tpending_comma: bool,"),
    ("json.vl", "\tsaved_commas: List<bool>,"),
    ("json.vl", "\tvariant_arities: List<i32>,"),
    ("json.vl", "\tstack: List<JsonValue>,"),
    ("json.vl", "\terror: Option<str>,"),
    ("binary.vl", "\tbuffer: Bytes,"),
    ("binary.vl", "\tused: i32,"),
    ("binary.vl", "\tcursor: i32,"),
    ("binary.vl", "\terror: Option<str>,"),
];

/// The F residue, and what blocks it. Recorded rather than asserted away: the
/// count above already pins the number, and this is why it is not zero.
///
/// It is ONE file now. `json.vl`'s six and `binary.vl`'s four left with A108;
/// this one cannot follow them, and the reason is not the receiver.
const FRAME_SCOPED_BLOCKED: &[(&str, usize, &str)] = &[(
    "process/fs.vl",
    1,
    "`Reader.cursor` is FORCED rather than chosen, and the type's own \
     doc-comment says so: `Reader::next` awaits the read, and a `&mut` view \
     may not be held across a suspension (spec §6.3), so a plain field behind \
     `&mut self` is not a shape this method may have — which is exactly why \
     A108's receiver change reached the other ten and not this one.",
)];

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
        total, 125,
        "the total number of `Shared` construction sites in std changed"
    );

    // A108's headline, asserted as an absence rather than read off the table:
    // the two codec files hold no cells at all.
    for file in ["json.vl", "binary.vl"] {
        assert!(
            !read_std(file).contains("Shared::new("),
            "{file} constructs a `Shared` cell again. Its visitor state is \
             plain fields behind `&mut self` (A108); a cell here is the \
             by-value receiver coming back, and the ten cells with it."
        );
    }
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

    for (file, declaration) in VISITOR_STATE_UNBOXED {
        let source = read_std(file);
        assert!(
            source.contains(declaration),
            "{file} no longer declares the A108 plain field `{declaration}`. \
             `Serialize`/`Deserialize` take `&mut self` and \
             `Wire::describe`/`rebuild` take `&mut S`/`&mut D`, so the \
             visitor's state is a field and not a cell; re-wrapping one buys \
             an allocation and an indirection for storage the receiver \
             already reaches."
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
