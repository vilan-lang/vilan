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
    (
        "browser/web/ui.vl",
        25,
        "O + R: per-boundary row/owner bookkeeping (+3 at Order 39: `when_some`'s \
         row, owner and payload cell — A119), plus A121's focus scopes at Order \
         40 — the scope STACK and its id source are R (module bindings, program \
         lifetime, because the document is global and an overlay is a portal: \
         the nesting cannot be read off the DOM), and a `FocusScope`'s `focused` \
         latch is O (it dies with the boundary that installed the scope). −1 at \
         A112 S3: `each`'s row list IS its region's (`region.rows`), not a \
         second cell kept in step with it; −1 at A112 S3b: `each_by`'s, the \
         same way",
    ),
    (
        "fs.vl",
        1,
        "F (blocked: `Reader::next` awaits, so no `&mut self`)",
    ),
    (
        "memo.vl",
        1,
        "E: the memo cache outlives every maker's scope",
    ),
    (
        "process/web/ui.vl",
        6,
        "O: the SSR request's view tree, plus A121's three inert focus-scope \
         twins at Order 40 (R by shape, never written): the twin surfaces are \
         held name for name by `std_twin_parity`, so the stack and its id \
         source are declared on the server side and nothing ever pushes to them",
    ),
    (
        "reactive.vl",
        50,
        "R + O + E: turns, owners, cells, drafts, the subscriber liveness flag \
         (A110 door 1 — one per OBSERVER, minted by `subscriber_of` and \
         shared with every handle to it, since A124 S2a split `observe` into \
         the mint and the attach; `Subscription::teardown`'s own; the \
         module-level `always_live` every deferral subscriber shares) and door 2's three: a \
         `Turn`'s second queue and its second dedup map (O, per turn) plus the \
         module-level `minting_derivation` mark (R — one bit for the program, \
         set at a derivation's attach and spent by the `observe` it reaches); \
         and A114's `scoped_runner` cell (O: the CURRENT run's owner, released \
         by the next run and by the enclosing boundary); and A123's two \
         (O: `switch`'s and `and_then`'s rolling inner subscription, the same \
         shape as the two `flatten`s', released by the ambient owner); and \
         A124 S2b's four (O: the cold `Switch` node's rolling inner \
         subscription, `switch`'s shape, released by the handle its attach \
         returns — the one cell the S1 probe carried, moved in with the \
         nodes; `Distinct`'s last-passed value, one per attach; a \
         `Resource`'s latest settled value and its load generation, the \
         `Draft` shape). `.cell()` adds NONE: its state is a `SignalCell`, \
         whose two cells are `SignalCell::new`'s. −2 at A124 S2c: the four \
         cell-returning joins (`switch`, both `flatten`s, `and_then`) each held \
         a rolling inner subscription; the total `flatten` is a `Switch` now and \
         the other two are the `FlattenOption`/`AndThen` nodes, one each +7 at A142 S1: the pipe stages keep their state in the INSTANCE a consumer starts (O, per instance, released with the consumer's handle): `Switch`, `SwitchSome` and `AndThen` each hold the flow they follow and its relay handle (2 each, where each cold node held one rolling subscription), `ThenSome` its flag and its handle (2), `Distinct` its last value (1, per instance now rather than per attach) and `DistinctBy` its last value and key (2). ±0 at A142 S2: an `Owner` is ONE cell now (its live epoch, cleanups and nursery; O) where it was two (the list and the disposed flag), its cleanup list is a cell of its own allocated at the epoch's first registration (O, the lazy owner), `owner_lists_allocated_count` is the module-level counter the lazy-owner pin reads (R), and A114's `scoped_runner` cell is gone with `scoped_runner` (an `effect`'s runs are epochs of one `Owner`). +1 at A142 S2's native follow-up: `no_cleanups`, the one never-pushed list every epoch that has registered nothing points at (R, module-level), since the native backend holds no `Option` of a closure list. +1 at A142 S4: `Owner::split_registrations`' fresh cell — an ELEMENT's owner, holding what one run of a collection operator's closure registered (O: released when the element leaves or re-runs). +6 at A142 S6 (tracked reads): a stage's `Tracker` is five cells (O, per stage instance, released with the consumer's handle) — where its runs stand (`TrackRuns`: epoch, open, dirty, and the connecting/missed pair written whole), the run in progress's reads, the last run's reads awaiting an attach, the live edges, and the subscriber a change wakes — and an `effect` keeps the value its input delivered last (O, per effect), which a tracked re-run runs with. +1 at M92: the module-level `run_nurseries_allocated_count` \
         (R: the lazy-nursery pin's instrument, beside `owner_lists_allocated_count`). −3 at M93: a `Tracker` is ONE cell made at `start` (where its runs stand, the target and, once something is tracked, its lists' cell) and the lists' ONE cell is made at the stage's first `track()` (O, per tracking stage) — where it was five cells per stage instance whether or not the body tracked; an `effect`'s latest input is a binding its two closures capture, not a cell; `trackers_allocated_count` is the lazy-tracker pin's instrument (R).",
    ),
    (
        "reactive/delta.vl",
        33,
        "E: the delta log's ops/version/base/cursors (twice — `new` and \
         `with_limit`) plus a cursor's own sequence. Every one of them is \
         minted by the CELL that holds the log and read by the CONSUMERS that \
         hold cursors into it, which is the E class exactly; they are A54's \
         five cells, lifted out of `rpc.vl` and spelled once per constructor \
         (A112 S1). +1 at M86: a `ListCell`'s own list, held in a cell of its \
         own rather than inside a `SignalCell` so a write can read its length \
         in place — E for the same reason (minted by the cell, read and \
         written through every copy of the handle). +12 at A142 S4, the \
         collection pipes' INSTANCE state (O, per instance: minted by `open`, \
         released with the consumer's handle): an element core's seven \
         (slot ids, inputs, values, the followed elements' holds, the dirty \
         slots, the next slot id, the consumer's subscriber), the kept mirror \
         of `filter`, `filter_map` and the tally (one each) and the Fenwick \
         index's tree; plus `element_holds_allocated_count`, the module-level \
         counter the per-element allocation pin reads (R). +2 at A142 S5: \
         `.coll()`'s and `.coll_by()`'s last list, the one each diffs the next \
         against (O, per instance). +9 at A138 S2, the map operators' INSTANCE \
         state (O, per instance, as the collection stages'): the rank index's \
         five (slots, live flags, a live key's slot, the Fenwick tree, the live \
         count) behind `values()`/`entries()`, the keyed core's two (rows by key, \
         a run's slot id to its key) behind `map_values`/`filter`/`sum_by`, and \
         `count()`'s counter and `sum_by`'s accumulator.",
    ),
    (
        "reactive/hash_map_cell.vl",
        4,
        "E (A138 S1): a `HashMapCell`'s own map, a `KeySlots` table, and each \
         watched key's subscription count; +1 at A138's wire reply, the table of \
         each asked key's identity (`HashMapEntry::identity`). Each is minted by the cell (or the \
         first subscription on a key) and read and written through every copy \
         of the handle and by the slot's release, which a disposal elsewhere \
         runs: the E class, as `ListCell`'s list is",
    ),
    (
        "reactive/hash_set_cell.vl",
        1,
        "E (A138 S1): a `HashSetCell`'s own set, as `HashMapCell`'s map (its slots are \
         a `KeySlots`, counted in `hash_map_cell.vl`)",
    ),
    (
        "reactive/store_core.vl",
        4,
        "E: a store's cells, all minted by the store and reached through every \
         copy of a handle (A142 S7, `proposal/store.md`) — the root's value \
         (`Store::new`), each node of its slot tree, and a slot's subscription \
         count (read by the subscription's release, Q12). The core moved out of \
         `store.vl` at A149 S3 (so `std::web::ui` stops loading the collection \
         layer); +1 there: a keyed node's table of children (a map's keys, a \
         set's members, a keyed list's keys), made by the first subscription \
         under a key and read by every write's diff. A collection flow's feed \
         keeps its state in `mut` bindings its closures capture, so `store.vl` \
         itself mints none",
    ),
    (
        "reactive/transient.vl",
        1,
        "E: a `Transient`'s generation claim (A142 S3) — made by `.transient()`, \
         read by each settling task to drop a superseded reply",
    ),
    (
        "rpc.vl",
        58,
        "R + O + E: sessions, wiring, the mirrors' leases. FIVE fewer since \
         A112 S1: `KeyedCell`'s own log, version, base and cursors, and its \
         cursor's sequence, are `DeltaLog`'s now (see `delta.vl`). +4 at A134 \
         (Order 43): a `ReactiveClient`'s origin-table enrolment list (E: \
         minted by the client, run by its `dispose`), each mirror's retire \
         hooks, plain and keyed (E: filled by the table that handed the \
         mirror out, run by the mirror's last release), and a `MirrorTable`'s \
         entries (R: a module binding the `[service]` expansion declares). +1 \
         at A137: a keyed mirror's join hook (E: filled once its deliverer \
         exists, run by its own `acquire`/`rebind`). +1 at A139: its per-KEY \
         join hook (E: the same shape, run after each per-key `Subscribe`). \
         +2 at A140: each mirror's revive hook, plain and keyed (E: filled by \
         the client that enlisted it, run by its `rebind`). +1 at A143: a \
         client's wire demand (R: one per `ReactiveClient`, every mirror's \
         `Subscribe`/`Unsubscribe` registered in it, cleared by a reconnect \
         and by `dispose`). +2 at A153 S4: a socket duplex's drop hooks (E: \
         `dispose_on_close` registers the client's, `handle_drop` runs them) \
         and a client's mirrored-store drop hooks (E: enlisted by a store \
         mint while it holds a grant, run by `connection_lost`).",
    ),
    (
        "rpc/mirror.vl",
        29,
        "R + E: the open mirror channels, the client's views and the failed \
         mints' last errors (three module bindings), and the cells every copy of \
         a record reaches — on the server per channel the grants, the slots, the \
         turn's dirty and seeding lists, its pending flush and the next base \
         slot, and each grant's and slot's hold count (A153 S1); on the client \
         per view its route cell, slots, retired readers, next slot, the turn's \
         subscribe and unsubscribe lists and its pending flush, each slot's hold \
         count, and per store mirror its binding, its call in flight, its holds, \
         each hold's release, its retire list, the boundaries it has heard and \
         the cells its handles' `states()` follow (A153 S2), and per view whether \
         its connection dropped (A153 S4)",
    ),
    (
        "rpc/server.vl",
        6,
        "R + O: the registry, the server's stats",
    ),
    (
        "shared.vl",
        2,
        "R: `fresh_identity`'s draw (A147) — a cell made only for the stamp \
         `Shared::identity` gives it, which is how every `identity()` in std \
         draws from ONE space; it is dropped at once — and the module-level \
         `debug_inside` stack `Shared`'s `Debug` keeps to print a cycle as \
         `<cycle>` (E283)",
    ),
    ("time.vl", 3, "O: the debouncer's pending/running/timer"),
    ("web/router.vl", 1, "R: the module-level `wired` latch"),
    ("ws.vl", 4, "O: the frame decoder's state"),
];

/// The frame-scoped cells M71 retired, as the shape that replaced each. A
/// `mut` local captured by the closures of its own frame IS the shared
/// storage, so each of these is the cell deleted and nothing put in its place.
const FRAME_SCOPED_RETIRED: &[(&str, &str)] = &[
    ("rpc/server.vl", "mut settled = false;"),
    ("rpc/server.vl", "mut expired = false;"),
    ("rpc/server.vl", "mut closed = false;"),
    ("rpc/server.vl", "mut greeted = false;"),
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
    ("binary.vl", "\tused: usize,"),
    ("binary.vl", "\tcursor: usize,"),
    ("binary.vl", "\terror: Option<str>,"),
];

/// The F residue, and what blocks it. Recorded rather than asserted away: the
/// count above already pins the number, and this is why it is not zero.
///
/// It is ONE file now. `json.vl`'s six and `binary.vl`'s four left with A108;
/// this one cannot follow them, and the reason is not the receiver.
const FRAME_SCOPED_BLOCKED: &[(&str, usize, &str)] = &[(
    "fs.vl",
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
        total, 229,
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
