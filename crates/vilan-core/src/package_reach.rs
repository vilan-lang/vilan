//! M209: the package reach walk, remembered.
//!
//! Which entry of a package REACHES a file decides the platform the file is
//! analyzed under (E113, `platform_color::file_platform_choices_for`) and the
//! world it is served from (M104, the language server's `RootResolver`). The
//! answer is [`crate::analyzer::package_modules_reachable_from`]: a walk of the
//! entry's `pkg::` import graph that reads and parses every module it reaches
//! and probes the filesystem for each import path. The language server asked it
//! up to three times per keystroke, and on kolt that was 0.61 G instructions of
//! a 9.14 G leaf keystroke — for an answer no body edit can move.
//!
//! [`ReachMemo`] keeps each walk's answer together with a [`ReachRecord`] of
//! EVERY question the walk asked of the world:
//!
//! - the text each module was read as (open buffer or disk, through the one
//!   overlay-then-disk seam), and the `pkg::` module paths parsed out of it;
//! - each candidate file it probed for existence, and the answer
//!   ([`crate::analyzer::module_file_exists`]: on disk, or an open buffer);
//! - each path it canonicalized, and the canonical form.
//!
//! The walk is a pure function of those answers. So a later ask re-asks exactly
//! them — reading the texts again, probing the same candidates on disk,
//! canonicalizing the same paths — and keeps the answer only when every one
//! comes back the same; a module whose text changed is re-parsed, and the
//! answer is kept only when its import list did not move. Anything else walks
//! again. The one question not re-asked every time is the open-buffer half of
//! a probe whose candidate is not on disk: that answer can only move when a
//! buffer is opened or closed, which the overlay counts
//! (`analyzer::document_overlay_membership`), and when the count moved it is
//! re-asked too. Nothing about
//! the key is guessed: a body edit, a new file on disk, an import added in a
//! sibling, a symlink re-pointed, an editor buffer opened or closed — each
//! changes one of the recorded answers or it cannot change the walk.
//!
//! What it does not cover, because the walk does not read it: the manifest
//! (the caller names the entry and the root it read from the manifest, and
//! those are the memo's key) and the edited file's own `[platform(..)]`
//! declarations, which the callers read off the live buffer on every ask.
//!
//! The cold path (`vilan check`) never builds one: it asks the walk once, as
//! it always did.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::fx::FxHashMap as HashMap;
use crate::fx::FxHashSet as HashSet;

/// Every question one reach walk asked, with the answer it got.
pub struct ReachRecord {
    /// Each module the walk loaded, in load order.
    pub(crate) files: Vec<ReadModule>,
    /// Each candidate file probed: whether it was there, and whether the DISK
    /// said so (rather than an open buffer). Asked once per walk: the walk's
    /// later probes of the same candidate read this answer.
    pub(crate) probes: HashMap<PathBuf, Probe>,
    /// Each path canonicalized, with the result; asked once per walk too.
    pub(crate) canonicals: HashMap<PathBuf, PathBuf>,
    /// The overlay's membership count
    /// ([`crate::analyzer::document_overlay_membership`]) when the walk began.
    pub(crate) overlay_membership: u64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Probe {
    there: bool,
    on_disk: bool,
}

impl Default for ReachRecord {
    fn default() -> ReachRecord {
        ReachRecord {
            files: Vec::new(),
            probes: HashMap::default(),
            canonicals: HashMap::default(),
            // Read BEFORE the walk asks anything: a buffer opened or closed
            // while it runs moves the count past this, and the next ask
            // re-probes the overlay in full.
            overlay_membership: crate::analyzer::document_overlay_membership(),
        }
    }
}

/// One module the walk loaded: the path as the walk spelled it, the text it
/// read (`None`: unreadable, so the walk saw no imports), and the `pkg::`
/// module paths parsed out of that text.
pub(crate) struct ReadModule {
    pub(crate) path: PathBuf,
    pub(crate) text: Option<String>,
    pub(crate) siblings: Option<Vec<String>>,
}

impl ReachRecord {
    /// [`crate::analyzer::module_file_exists`], asked once per candidate —
    /// spelled out so the record knows which side answered.
    pub(crate) fn probe(&mut self, candidate: &Path) -> bool {
        if let Some(probe) = self.probes.get(candidate) {
            return probe.there;
        }
        let on_disk = candidate.exists();
        let there = on_disk || crate::analyzer::document_overlay_contains(candidate);
        self.probes
            .insert(candidate.to_path_buf(), Probe { there, on_disk });
        there
    }

    /// [`crate::util::canonical_path`], asked once per path.
    pub(crate) fn canonical(&mut self, path: &Path) -> PathBuf {
        if let Some(canonical) = self.canonicals.get(path) {
            return canonical.clone();
        }
        let canonical = crate::util::canonical_path(path);
        self.canonicals
            .insert(path.to_path_buf(), canonical.clone());
        canonical
    }
}

thread_local! {
    /// How many reach walks THIS THREAD has run through a [`ReachMemo`] —
    /// misses only. An instrumentation probe in the style of
    /// [`crate::util::source_read_count`]: read before and after, assert on the
    /// difference.
    static RECORDED_WALKS: Cell<u64> = const { Cell::new(0) };
}

/// The number of reach walks this thread has run on a [`ReachMemo`]'s behalf
/// (each one a miss: no remembered answer, or one whose record no longer
/// holds). Monotonic.
pub fn recorded_walk_count() -> u64 {
    RECORDED_WALKS.with(Cell::get)
}

struct Leg {
    reached: Arc<HashSet<PathBuf>>,
    record: ReachRecord,
}

/// Remembered reach walks, keyed by the entry's CANONICAL path and the package
/// root exactly as the caller spells it.
///
/// The entry is canonicalized because the walk reads nothing of its spelling:
/// it reads the file (one file, whichever spelling names it) and inserts its
/// canonical path, so the language server's two askers — the resolver, which
/// canonicalizes its entries, and platform colouring, which joins them onto the
/// root — share one walk. The ROOT is not: every import resolves against it by
/// joining, and an open buffer with no file on disk is matched by that joined
/// spelling, so two spellings of a root are two walks.
#[derive(Default)]
pub struct ReachMemo {
    legs: HashMap<(PathBuf, PathBuf), Leg>,
    /// The last text each module path was parsed from for its import list, and
    /// the list — shared by the legs, so a buffer two legs reach is parsed
    /// once per edit rather than once per leg.
    parsed: HashMap<PathBuf, (Option<String>, Option<Vec<String>>)>,
}

impl ReachMemo {
    pub fn new() -> ReachMemo {
        ReachMemo::default()
    }

    /// [`crate::analyzer::package_modules_reachable_from`]`(entry, pkg_root)`,
    /// walked only when no remembered walk's record still holds.
    ///
    /// Inside an analysis's collection scope it walks unrecorded and remembers
    /// nothing: a load there is served from the analysis's own memo, which is
    /// not a read this record could re-ask.
    pub fn reach(&mut self, entry: &Path, pkg_root: &Path) -> Arc<HashSet<PathBuf>> {
        if crate::owned_modules::collecting() {
            return Arc::new(crate::analyzer::package_modules_reachable_from(
                entry, pkg_root,
            ));
        }
        let entry = crate::util::canonical_path(entry);
        let key = (entry.clone(), pkg_root.to_path_buf());
        if let Some(leg) = self.legs.get_mut(&key)
            && still_holds(&mut leg.record, &mut self.parsed)
        {
            return Arc::clone(&leg.reached);
        }
        RECORDED_WALKS.with(|count| count.set(count.get() + 1));
        let mut record = ReachRecord::default();
        let reached = Arc::new(crate::analyzer::package_module_walk(
            &entry,
            pkg_root,
            Some(&mut record),
        ));
        self.legs.insert(
            key,
            Leg {
                reached: Arc::clone(&reached),
                record,
            },
        );
        reached
    }
}

/// Whether every question `record` holds still gets the answer it recorded.
/// A module whose text moved is re-parsed (through `parsed`, shared by the
/// legs); when its import list is unchanged the record adopts the new text, so
/// the next ask compares bytes again instead of parsing.
fn still_holds(
    record: &mut ReachRecord,
    parsed: &mut HashMap<PathBuf, (Option<String>, Option<Vec<String>>)>,
) -> bool {
    if record
        .canonicals
        .iter()
        .any(|(path, canonical)| crate::util::canonical_path(path) != *canonical)
    {
        return false;
    }
    // The probes. The disk is asked again for every one (a file created or
    // deleted since). The overlay is asked again only where it can have
    // changed its answer: a candidate the disk does not hold, once a buffer
    // was opened or closed since the walk. While the overlay's set stands
    // still, a path it did not hold it still does not, and one it held it
    // still holds — and a path missing from disk canonicalizes lexically, so
    // its key cannot move under it either.
    let membership = crate::analyzer::document_overlay_membership();
    let overlay_unchanged = membership == record.overlay_membership;
    for (candidate, recorded) in &record.probes {
        let there = if candidate.exists() {
            true
        } else if overlay_unchanged && !recorded.on_disk {
            recorded.there
        } else {
            crate::analyzer::document_overlay_contains(candidate)
        };
        if there != recorded.there {
            return false;
        }
    }
    for module in &mut record.files {
        let read = crate::util::read_source_traced(&module.path).ok();
        if read.as_ref().map(|(text, _)| text) == module.text.as_ref() {
            continue;
        }
        let text = read.as_ref().map(|(text, _)| text.clone());
        let siblings = match parsed.get(&module.path) {
            Some((parsed_text, siblings)) if *parsed_text == text => siblings.clone(),
            _ => {
                let siblings = read
                    .map(|(source, provenance)| parse_siblings(&module.path, source, provenance));
                parsed.insert(module.path.clone(), (text.clone(), siblings.clone()));
                siblings
            }
        };
        if siblings != module.siblings {
            return false;
        }
        module.text = text;
    }
    // Every answer held against the overlay as it is now, so the next ask
    // compares against today's set.
    record.overlay_membership = membership;
    true
}

/// The module paths `source` names, parsed as the walk parses a module —
/// inside a collection scope of its own, so an open buffer's tree is owned
/// here and given back rather than parked in the process-global parse cache
/// (`leak-soak.md` §7.5, the walk's own rule).
fn parse_siblings(
    path: &Path,
    source: String,
    provenance: crate::util::SourceProvenance,
) -> Vec<String> {
    let scope = crate::owned_modules::CollectionScope::activate();
    let siblings = crate::analyzer::pkg_module_paths(&crate::analyzer::load_package_module_source(
        path, source, provenance,
    ));
    // SAFETY: the module names were copied into owned `String`s above; nothing
    // derived from the parse outlives this line, and nothing global retains a
    // borrow of it (no world is stored, no macro expanded).
    unsafe { scope.drain().reclaim() };
    siblings
}
