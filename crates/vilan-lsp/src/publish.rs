//! The publish planner (backlog E6): which diagnostics land at which URI,
//! computed as data before anything is sent. The server's `Client` only
//! transmits the returned actions, so the whole lifecycle — open, edit,
//! close, shared dependencies — is testable synchronously, without a
//! language-server connection.
//!
//! Each open document is an *owner*: its analysis produces diagnostic groups
//! for one or more *targets* (its own URI, plus each imported file with
//! diagnostics). A target's published list is the union of every owner's
//! group for it, so two open documents importing the same broken module
//! cannot overwrite each other's view — closing or fixing one leaves the
//! other's diagnostics standing.
//!
//! Identity and address are two different things here (`windows-support.md` §7).
//! Every owner and every target is *keyed* by its canonical form (`uri::normalize`),
//! so the client's spelling of a file and the server's own always land in one
//! slot — on Windows they are different strings (`file:///c%3A/…` vs
//! `file:///C:/…`) and a raw-`Url` key duplicated the diagnostics into two
//! entries that never cleared each other. What goes back *on the wire* is the
//! spelling the file was last named with — the client's own for an open document
//! (its key is authoritative for its buffer), the analysis-minted one otherwise
//! (which inherits the client's spelling of the workspace, symlinks and all, so
//! the squiggle lands on the file the user actually opened).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticRelatedInformation, DiagnosticSeverity, Location, Range, Url,
};

use crate::document::{Document, PublishedDiagnostic};
use crate::line_index::LineIndex;

/// The bookkeeping for everything published so far: each owner's last
/// diagnostic groups, keyed by owner then target — both in canonical form
/// (`uri::normalize`), so one file is one slot however it was spelled.
/// `BTreeMap` so merged unions list owners in a stable order
/// (diagnostics-standard.md C1 — republishing without a change must not
/// reorder).
pub struct PublishState {
    owned: BTreeMap<Url, Vec<(Url, Vec<Diagnostic>)>>,
    /// Key → the spelling the client used for that OPEN document. Authoritative
    /// while it is open: a notification about a buffer goes back in the client's
    /// own URI, never in a form the server invented for it.
    open_spellings: BTreeMap<Url, Url>,
    /// Key → the spelling the analysis last minted for a file that is not open.
    /// Kept because it inherits the client's spelling of the workspace (module
    /// paths are built from the entry path), which the canonical key does not —
    /// canonicalizing resolves symlinks, and publishing to the resolved path
    /// would put the squiggle on a file the user never opened. Not pruned: it is
    /// bounded by the files that have carried a diagnostic this session, and an
    /// entry is only ever read for a key that is publishing again.
    minted_addresses: BTreeMap<Url, Url>,
    /// Whether to apply the Windows drive-letter rule when keying. `cfg!(windows)`
    /// in production; a test can plan for the other platform (see `uri`).
    windows: bool,
}

impl PublishState {
    pub fn new() -> Self {
        PublishState::for_platform(cfg!(windows))
    }

    pub fn for_platform(windows: bool) -> Self {
        PublishState {
            owned: BTreeMap::new(),
            open_spellings: BTreeMap::new(),
            minted_addresses: BTreeMap::new(),
            windows,
        }
    }

    /// Re-plan after `owner` was (re)analyzed: recompute its groups and
    /// return one `(target, merged diagnostics)` action per target the
    /// change touches — including targets the owner dropped since last
    /// time, which get the remaining owners' merged view (possibly empty),
    /// so nothing goes stale.
    pub fn plan_publish(
        &mut self,
        owner: &Url,
        document: &Document,
    ) -> Vec<(Url, Vec<Diagnostic>)> {
        let owner_key = self.key(owner);
        // Clear before rebuild, deliberately (backlog E97). This planner is
        // reached under a poison-RECOVERING lock, and it is the one place here
        // that a caught panic could leave behind something worse than an absent
        // entry: `diagnostic_groups` does span-to-range arithmetic over the
        // analysis, so it is the panic-prone half, and it runs with the guard
        // held. Taking this owner's groups OUT first means an unwind leaves the
        // owner contributing NOTHING, which the next analysis of it rebuilds —
        // where leaving them in would keep `merged` folding one document's
        // superseded diagnostics into every OTHER document's republishes of a
        // shared module, with nothing to trigger a correction. Same result on
        // the happy path: the entry is reinserted below.
        let previous = self.owned.remove(&owner_key);
        // The groups come back addressed (the owner's own URI, each module's
        // minted one); key them, and remember the address each key was seen at.
        let groups: Vec<(Url, Vec<Diagnostic>)> = diagnostic_groups(document, owner)
            .into_iter()
            .map(|(address, group)| {
                let key = self.key(&address);
                self.minted_addresses.insert(key.clone(), address);
                (key, group)
            })
            .collect();
        self.open_spellings.insert(owner_key.clone(), owner.clone());
        let mut affected: Vec<Url> = groups.iter().map(|(target, _)| target.clone()).collect();
        if let Some(previous) = previous {
            for (target, _) in previous {
                if !affected.contains(&target) {
                    affected.push(target);
                }
            }
        }
        self.owned.insert(owner_key, groups);
        affected
            .into_iter()
            .map(|target| {
                let merged = self.merged(&target);
                (self.address(&target), merged)
            })
            .collect()
    }

    /// Remove `owner` (the document closed) and return the republish
    /// actions for every target it contributed to — each now the remaining
    /// owners' merged view, empty where it was the only contributor.
    pub fn plan_close(&mut self, owner: &Url) -> Vec<(Url, Vec<Diagnostic>)> {
        let owner_key = self.key(owner);
        let Some(previous) = self.owned.remove(&owner_key) else {
            self.open_spellings.remove(&owner_key);
            return Vec::new();
        };
        // Addressed BEFORE the spelling is dropped, so the closing document's own
        // clear still reaches the URI the client opened it under.
        let actions: Vec<(Url, Vec<Diagnostic>)> = previous
            .into_iter()
            .map(|(target, _)| {
                let merged = self.merged(&target);
                (self.address(&target), merged)
            })
            .collect();
        self.open_spellings.remove(&owner_key);
        actions
    }

    /// The canonical key for a URL, under this planner's platform rule.
    fn key(&self, url: &Url) -> Url {
        crate::uri::normalize(url, self.windows)
    }

    /// Where a notification for `key` is sent: the client's spelling while the
    /// file is open, else the one the analysis minted, else the key itself.
    fn address(&self, key: &Url) -> Url {
        self.open_spellings
            .get(key)
            .or_else(|| self.minted_addresses.get(key))
            .unwrap_or(key)
            .clone()
    }

    /// The union of every owner's group for `target`, deduplicated — two
    /// owners that see the same error in a shared module contribute it
    /// once.
    fn merged(&self, target: &Url) -> Vec<Diagnostic> {
        let mut merged: Vec<Diagnostic> = Vec::new();
        for groups in self.owned.values() {
            for (candidate, group) in groups {
                if candidate != target {
                    continue;
                }
                for diagnostic in group {
                    if !merged.contains(diagnostic) {
                        merged.push(diagnostic.clone());
                    }
                }
            }
        }
        merged
    }
}

/// A secondary location resolved to the wire: the file's URI and the span's
/// range in THAT file's text. `home` is the file the diagnostic itself was
/// published to, with its index: a location with no file of its own lives
/// there (the entry, or the module the diagnostic was attributed to). A
/// location in another file is read fresh, like the diagnostic's own file
/// is. An unreadable file answers `None` — the caller drops that entry
/// only, never the diagnostic.
fn locate_secondary(
    span: &vilan_core::Span,
    path: &Option<std::path::PathBuf>,
    home: &Url,
    home_index: &LineIndex,
) -> Option<Location> {
    match path {
        None => Some(Location {
            uri: home.clone(),
            range: home_index.range(span),
        }),
        Some(path) => vilan_core::util::read_source(path)
            .ok()
            .map(|text| LineIndex::new(&text).range(span))
            .and_then(|range| {
                Url::from_file_path(path)
                    .ok()
                    .map(|uri| Location { uri, range })
            }),
    }
}

/// Attaches a diagnostic's secondary locations as LSP related information
/// (backlog E17): the E78 requirement trace first — one entry per uncovered
/// upstream call, preserving the analyzer's entry → read order — then the C3
/// note, each a location plus a message, which is exactly what a chain hop
/// or a declaration note is.
fn attach_related(
    converted: &mut Diagnostic,
    item: &PublishedDiagnostic,
    home: &Url,
    home_index: &LineIndex,
) {
    let related: Vec<DiagnosticRelatedInformation> = item
        .trace
        .iter()
        .map(|hop| (&hop.span, &hop.message, &hop.path))
        .chain(
            item.note
                .iter()
                .map(|(span, message, path)| (span, message, path)),
        )
        .filter_map(|(span, message, path)| {
            locate_secondary(span, path, home, home_index).map(|location| {
                DiagnosticRelatedInformation {
                    location,
                    message: message.clone(),
                }
            })
        })
        .collect();
    if !related.is_empty() {
        converted.related_information = Some(related);
    }
}

/// The E81 hop diagnostics of one published item: each CALL hop of the item's
/// requirement trace, as a diagnostic AT THE CALL. Related information draws
/// no marker at its locations, so the trace alone (E78) squiggled the read
/// and left every call on the uncovered path bare — exactly the sites the
/// trace exists to point at. Each hop diagnostic carries the hop's own label
/// as its message (the primary's text would say "is read here" at a span that
/// is not the read) and the rest of the story as related information: the
/// other trace entries in entry → read order, then the read itself (the
/// primary), then the C3 note. The elision tail publishes no diagnostic of
/// its own — its span is the last kept hop's, already underlined by that
/// hop's — and a hop whose file cannot be read drops out entirely, exactly
/// as it drops out of the primary's related information.
fn trace_call_diagnostics(
    item: &PublishedDiagnostic,
    severity: DiagnosticSeverity,
    primary: Location,
    home: &Url,
    home_index: &LineIndex,
) -> Vec<(Url, Diagnostic)> {
    if !item.trace.iter().any(|hop| hop.call) {
        return Vec::new();
    }
    // The whole story, located once: every trace entry, then the read, then
    // the note.
    let located: Vec<(Option<Location>, &str, bool)> = item
        .trace
        .iter()
        .map(|hop| {
            (
                locate_secondary(&hop.span, &hop.path, home, home_index),
                hop.message.as_str(),
                hop.call,
            )
        })
        .chain(std::iter::once((
            Some(primary),
            item.message.as_str(),
            false,
        )))
        .chain(item.note.iter().map(|(span, message, path)| {
            (
                locate_secondary(span, path, home, home_index),
                message.as_str(),
                false,
            )
        }))
        .collect();
    located
        .iter()
        .enumerate()
        .filter_map(|(position, (location, message, call))| {
            if !call {
                return None;
            }
            let location = location.as_ref()?;
            let related: Vec<DiagnosticRelatedInformation> = located
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != position)
                .filter_map(|(_, (location, message, _))| {
                    location
                        .as_ref()
                        .map(|location| DiagnosticRelatedInformation {
                            location: location.clone(),
                            message: (*message).to_string(),
                        })
                })
                .collect();
            Some((
                location.uri.clone(),
                Diagnostic {
                    range: location.range,
                    severity: Some(severity),
                    source: Some("vilan".to_string()),
                    message: (*message).to_string(),
                    related_information: (!related.is_empty()).then_some(related),
                    ..Default::default()
                },
            ))
        })
        .collect()
}

/// One analyzed document's diagnostics as per-target groups: the entry's own
/// (always present, even when empty, so the owner's URI is always brought
/// current) plus each imported file's, with spans converted through a fresh
/// read of *that* file — `read_source` answers from the open-document overlay
/// when there is a buffer and the disk otherwise, which is exactly what the
/// analysis read, so they agree.
///
/// They did not always. This comment used to justify the agreement with "the
/// analysis read it from disk too", which stopped being true once the analyzer
/// grew the overlay: analysis indexed the buffer, publishing re-read the disk,
/// and every diagnostic in an edited-but-unsaved module landed off by the
/// buffer-versus-disk line delta. Routing both through one reader is what makes
/// the sentence above hold rather than merely assert.
fn diagnostic_groups(document: &Document, owner: &Url) -> Vec<(Url, Vec<Diagnostic>)> {
    let mut entry_group: Vec<Diagnostic> = Vec::new();
    let mut extra_groups: Vec<(Url, Vec<Diagnostic>)> = Vec::new();
    let mut extra_indices: HashMap<PathBuf, Option<Arc<LineIndex>>> = HashMap::new();
    // The E81 hop diagnostics, collected across items and routed to their
    // groups after the loop: a hop lives wherever its call does, which is
    // routinely not the file its primary published to.
    let mut hop_diagnostics: Vec<(Url, Diagnostic)> = Vec::new();
    for item in document.published_diagnostics() {
        let severity = if item.warning {
            DiagnosticSeverity::WARNING
        } else {
            DiagnosticSeverity::ERROR
        };
        let diagnostic = |range| Diagnostic {
            range,
            severity: Some(severity),
            source: Some("vilan".to_string()),
            message: item.message.clone(),
            ..Default::default()
        };
        match &item.path {
            None => {
                // The ANALYZED index: these are program spans, so they index
                // the text the analysis consumed (`document.rs`'s two-snapshot
                // law). Publishing runs right after an analysis lands, so the
                // two indices normally agree — the law is uniform anyway.
                let mut converted = diagnostic(document.analyzed_range(&item.span));
                // A secondary note becomes related information — "first
                // call here"-style anchors.
                attach_related(&mut converted, &item, owner, document.analyzed_index());
                hop_diagnostics.extend(trace_call_diagnostics(
                    &item,
                    severity,
                    Location {
                        uri: owner.clone(),
                        range: converted.range,
                    },
                    owner,
                    document.analyzed_index(),
                ));
                entry_group.push(converted);
            }
            Some(path) => {
                // A fresh (uncached) read: module files change across saves,
                // so a session-cached index would misplace ranges. The BOM is
                // dropped exactly as the analyzer's own read drops it
                // (windows-support.md §2), so the index and the spans agree on
                // line 0.
                let index = extra_indices
                    .entry(path.clone())
                    .or_insert_with(|| {
                        vilan_core::util::read_source(path)
                            .ok()
                            .map(|text| Arc::new(LineIndex::new(&text)))
                    })
                    .clone();
                match (index, Url::from_file_path(path)) {
                    (Some(index), Ok(target)) => {
                        let mut converted = diagnostic(index.range(&item.span));
                        // The note travels with the diagnostic here too
                        // (backlog E17): this branch used to publish
                        // module-attributed diagnostics stripped of their
                        // second location.
                        attach_related(&mut converted, &item, &target, &index);
                        hop_diagnostics.extend(trace_call_diagnostics(
                            &item,
                            severity,
                            Location {
                                uri: target.clone(),
                                range: converted.range,
                            },
                            &target,
                            &index,
                        ));
                        match extra_groups
                            .iter_mut()
                            .find(|(existing, _)| *existing == target)
                        {
                            Some((_, group)) => group.push(converted),
                            None => extra_groups.push((target, vec![converted])),
                        }
                    }
                    // Unreadable file: keep the error visible on the entry.
                    _ => entry_group.push(Diagnostic {
                        range: Range::default(),
                        severity: Some(severity),
                        source: Some("vilan".to_string()),
                        message: format!("(in {}) {}", path.display(), item.message),
                        ..Default::default()
                    }),
                }
            }
        }
    }
    // Two chains through one call — two reads sharing an upstream frame —
    // merge into ONE hop diagnostic there, their stories concatenated: an
    // identical squiggle stacked per read would report one call N times.
    let mut merged_hops: Vec<(Url, Diagnostic)> = Vec::new();
    for (target, diagnostic) in hop_diagnostics {
        match merged_hops.iter_mut().find(|(existing_target, existing)| {
            *existing_target == target
                && existing.range == diagnostic.range
                && existing.message == diagnostic.message
                && existing.severity == diagnostic.severity
        }) {
            Some((_, existing)) => {
                let combined = existing.related_information.get_or_insert_with(Vec::new);
                for entry in diagnostic.related_information.into_iter().flatten() {
                    if !combined.contains(&entry) {
                        combined.push(entry);
                    }
                }
            }
            None => merged_hops.push((target, diagnostic)),
        }
    }
    for (target, diagnostic) in merged_hops {
        if target == *owner {
            entry_group.push(diagnostic);
        } else {
            match extra_groups
                .iter_mut()
                .find(|(existing, _)| *existing == target)
            {
                Some((_, group)) => group.push(diagnostic),
                None => extra_groups.push((target, vec![diagnostic])),
            }
        }
    }
    let mut groups = vec![(owner.clone(), entry_group)];
    groups.extend(extra_groups);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tests::{analyze_workspace, std_root};
    use std::path::Path;
    use tower_lsp::lsp_types::Position;

    /// Analyze `relative` under `dir` as an open document (its own entry,
    /// like the server does for every open file).
    fn open(dir: &Path, relative: &str) -> (Url, Document) {
        let path = dir.join(relative);
        let text = std::fs::read_to_string(&path).unwrap();
        let document = Document::analyze(&text, &std_root(), &path);
        (Url::from_file_path(&path).unwrap(), document)
    }

    fn apply(editor: &mut BTreeMap<Url, Vec<Diagnostic>>, actions: Vec<(Url, Vec<Diagnostic>)>) {
        for (target, group) in actions {
            editor.insert(target, group);
        }
    }

    /// What the editor should show for exactly `open_documents`: a fresh
    /// planner replayed from scratch, empty targets dropped.
    fn fresh_view(open_documents: &[(&Url, &Document)]) -> BTreeMap<Url, Vec<Diagnostic>> {
        let mut state = PublishState::new();
        let mut editor: BTreeMap<Url, Vec<Diagnostic>> = BTreeMap::new();
        for (uri, document) in open_documents {
            apply(&mut editor, state.plan_publish(uri, document));
        }
        editor.retain(|_, group| !group.is_empty());
        editor
    }

    fn visible(editor: &BTreeMap<Url, Vec<Diagnostic>>) -> BTreeMap<Url, Vec<Diagnostic>> {
        let mut visible = editor.clone();
        visible.retain(|_, group| !group.is_empty());
        visible
    }

    /// The (line, start, end) of the `occurrence`th `snippet` in `text`,
    /// 0-based, so span expectations are computed from the fixture rather
    /// than hand-counted.
    fn position_of(text: &str, snippet: &str, occurrence: usize) -> (u32, u32, u32) {
        let mut from = 0;
        let mut at = None;
        for _ in 0..=occurrence {
            at = text[from..].find(snippet).map(|found| from + found);
            from = at.expect("the snippet occurs in the fixture") + 1;
        }
        let at = at.unwrap();
        let line = text[..at].matches('\n').count() as u32;
        let column = (at - text[..at].rfind('\n').map(|nl| nl + 1).unwrap_or(0)) as u32;
        (line, column, column + snippet.len() as u32)
    }

    /// The Range of the `occurrence`th `snippet` in `text`, from
    /// [`position_of`].
    fn range_of(text: &str, snippet: &str, occurrence: usize) -> Range {
        let (line, start, end) = position_of(text, snippet, occurrence);
        Range {
            start: Position {
                line,
                character: start,
            },
            end: Position {
                line,
                character: end,
            },
        }
    }

    /// One analysis of `text` as the open entry, published through the planner —
    /// byte-identical to what `Backend::publish_document` puts on the wire
    /// (`editing-dx.md` §1.3: the handler is a pure transmitter).
    fn published(text: &str) -> Vec<Diagnostic> {
        let path = std::env::temp_dir().join(format!("vilan_publish_{}.vl", std::process::id()));
        let uri = Url::from_file_path(&path).unwrap();
        let document = Document::analyze(text, &std_root(), &path);
        PublishState::new()
            .plan_publish(&uri, &document)
            .into_iter()
            .find(|(target, _)| *target == uri)
            .map(|(_, group)| group)
            .unwrap_or_default()
    }

    /// backlog E97: the server reaches this planner through a POISON-RECOVERING
    /// lock, because `fenced` catches a per-request panic and a propagated
    /// poison would wedge every request after it. Pinned at the shape the server
    /// uses — `Backend::publish_document`'s expression, over a mutex a caught
    /// panic has already poisoned.
    #[test]
    fn a_poisoned_publish_planner_still_plans() {
        let directory = std::env::temp_dir().join(format!("vilan_poison_{}", std::process::id()));
        std::fs::create_dir_all(&directory).expect("a scratch directory");
        let path = directory.join("entry.vl");
        std::fs::write(&path, "fun main() {\n\tlet wrong: i32 = \"text\";\n}\n")
            .expect("a writable scratch file");
        let uri = Url::from_file_path(&path).expect("a file URL");
        let document = Document::analyze(
            &std::fs::read_to_string(&path).expect("readable"),
            &std_root(),
            &path,
        );

        let state = std::sync::Mutex::new(PublishState::new());
        // Poison it exactly as a caught panic does: unwind out of a held guard,
        // catch, keep running.
        let previous_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = state.lock().expect("not poisoned yet");
            panic!("a request panicked inside the fence");
        }));
        std::panic::set_hook(previous_hook);
        assert!(outcome.is_err(), "the probe panic must have unwound");
        assert!(
            state.is_poisoned(),
            "the probe poisoned the planner's mutex"
        );

        let actions = state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .plan_publish(&uri, &document);
        assert!(
            actions
                .iter()
                .any(|(target, group)| *target == uri && !group.is_empty()),
            "the next request plans its diagnostics through the recovered guard: {actions:#?}"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    /// The BLACKOUT (`editing-dx.md` §2, the survey's headline finding), pinned at
    /// the wire: while a file did not parse, the editor lost every diagnostic the
    /// file already had and gained one anchored on a line the user was not editing.
    ///
    /// This is P30's keystroke table, stages 0 and 2. The standing type error on
    /// line 2 must be published UNCHANGED — same message, same range — while the
    /// call on line 3 is half typed; before the statement/item synchronizer (S1)
    /// stage 2 published exactly one diagnostic, `found '}' expected an
    /// expression`, on the function's closing brace, and the real error the user
    /// may well have opened the file to fix was gone.
    #[test]
    fn a_half_typed_statement_does_not_black_out_the_files_other_diagnostics() {
        let settled = published("fun main() {\n\tlet wrong: i32 = \"text\";\n\tprint(1);\n}\n");
        let standing: Vec<_> = settled
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .message
                    .contains("Expected i32, but got str instead.")
            })
            .collect();
        assert_eq!(
            standing.len(),
            1,
            "the premise: the settled buffer has exactly one standing type error: {settled:#?}"
        );
        let standing_range = standing[0].range;

        // The user goes back and starts retyping line 3. The buffer does not parse.
        let mid_edit = published("fun main() {\n\tlet wrong: i32 = \"text\";\n\tprint(\n}\n");
        assert!(
            mid_edit
                .iter()
                .any(|diagnostic| diagnostic.message.contains("unclosed `(`")),
            "the syntax error is reported, at the opener: {mid_edit:#?}"
        );
        let survivors: Vec<_> = mid_edit
            .iter()
            .filter(|diagnostic| {
                diagnostic
                    .message
                    .contains("Expected i32, but got str instead.")
            })
            .collect();
        assert_eq!(
            survivors.len(),
            1,
            "the standing diagnostic must survive a parse error in another \
             statement: {mid_edit:#?}"
        );
        assert_eq!(
            survivors[0].range, standing_range,
            "and must not move while the file below it is unparseable"
        );
    }

    /// The same law across FUNCTIONS, and in the direction that used to lose the
    /// most (`editing-dx.md` §2.2's P31 row B): an unclosed `(` ABOVE a type
    /// error. The parse stopped at the unclosed region, so everything below the
    /// cursor — the whole file tail — stopped being checked.
    #[test]
    fn an_unclosed_delimiter_does_not_black_out_the_file_tail() {
        let mid_edit =
            published("fun one() {\n\tprint(\n}\nfun two() {\n\tlet bad: i32 = \"text\";\n}\n");
        assert!(
            mid_edit
                .iter()
                .any(|diagnostic| diagnostic.message.contains("unclosed `(`")),
            "{mid_edit:#?}"
        );
        assert!(
            mid_edit.iter().any(|diagnostic| diagnostic
                .message
                .contains("Expected i32, but got str instead.")),
            "the diagnostic below the broken function must survive: {mid_edit:#?}"
        );
    }

    // A module-attributed diagnostic reaches the editor WITH its note (backlog
    // E17): the note is a second location plus a message, which is exactly LSP
    // related information. The publisher's module branch used to build the
    // diagnostic without one, so `Z` is declared here` — and every other C3
    // note on a non-entry file — was invisible in the editor.
    #[test]
    fn a_module_attributed_diagnostic_publishes_its_note_as_related_information() {
        let (dir, _) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::alpha::{ A };\nimport pkg::zeta::{ Z };\n\
                 fun main() { print(A); print(Z); }\n",
            ),
            (
                "alpha.vl",
                "import pkg::zeta::{ Z };\nlet A: i32 = Z + 1;\n",
            ),
            (
                "zeta.vl",
                "import pkg::alpha::{ A };\nlet Z: i32 = A + 2;\n",
            ),
        ]);
        let mut state = PublishState::new();
        let (main_uri, main_document) = open(&dir, "main.vl");
        let published = state.plan_publish(&main_uri, &main_document);
        let group = published
            .iter()
            .find(|(target, _)| target.path().ends_with("alpha.vl"))
            .map(|(_, group)| group)
            .expect("the cycle publishes at alpha.vl, where the read closes it");
        let diagnostic = group
            .iter()
            .find(|item| item.message.contains("initialization cycle"))
            .expect("the cycle diagnostic is published");
        let related = diagnostic
            .related_information
            .as_ref()
            .expect("the note travels with the diagnostic")
            .first()
            .expect("one note, one related-information entry");
        assert!(
            related.message.contains("`Z` is declared here"),
            "{}",
            related.message
        );
        assert!(
            related.location.uri.path().ends_with("zeta.vl"),
            "the note's location is in ITS file: {}",
            related.location.uri
        );
        assert_eq!(
            (
                related.location.range.start.line,
                related.location.range.start.character
            ),
            (1, 0),
            "`let Z` starts line 2 of zeta.vl"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E78 at the wire: a cross-file requirement chain publishes as related
    // information — one entry per uncovered hop, the analyzer's entry → read
    // order preserved, each located in ITS file (URI + range). The read is
    // user-written, so there is no C3 note: the related information is
    // exactly the chain.
    #[test]
    fn a_cross_file_requirement_chain_publishes_each_hop_as_related_information() {
        let main_text = "import std::print;\nimport pkg::lib::read_it;\nfun relay(): i32 {\n\tread_it()\n}\nfun main() {\n\tprint(relay());\n}\nmain();\n";
        let lib_text = "import std::context::Context;\nlet current: Context<i32> = Context::new();\nfun read_it(): i32 {\n\tcurrent.get()\n}\n";
        let (dir, _) = analyze_workspace(&[("main.vl", main_text), ("lib.vl", lib_text)]);
        let mut state = PublishState::new();
        let (main_uri, main_document) = open(&dir, "main.vl");
        let published = state.plan_publish(&main_uri, &main_document);
        let group = published
            .iter()
            .find(|(target, _)| target.path().ends_with("lib.vl"))
            .map(|(_, group)| group)
            .expect("the read is in lib.vl, so the diagnostic publishes there");
        let diagnostic = group
            .iter()
            .find(|item| {
                item.message
                    .contains("can be reached without an enclosing `run`")
            })
            .expect("the coverage diagnostic is published");
        let related = diagnostic
            .related_information
            .as_ref()
            .expect("the chain travels with the diagnostic");
        // Entry → read: the top-level call, main's `relay()`, relay's
        // `read_it()` — the occurrence skips a declaration where the
        // snippet also matches it.
        let expected = [
            position_of(main_text, "main()", 1),
            position_of(main_text, "relay()", 1),
            position_of(main_text, "read_it()", 0),
        ];
        assert_eq!(related.len(), 3, "{related:#?}");
        for (entry, (line, start, end)) in related.iter().zip(expected) {
            assert!(
                entry
                    .message
                    .contains("the context requirement flows through this call"),
                "{}",
                entry.message
            );
            assert!(
                entry.location.uri.path().ends_with("main.vl"),
                "each hop locates in ITS file: {}",
                entry.location.uri
            );
            assert_eq!(
                entry.location.range.start,
                Position {
                    line,
                    character: start
                }
            );
            assert_eq!(
                entry.location.range.end,
                Position {
                    line,
                    character: end
                }
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E81: related information draws no marker in the editor, so the E78
    // chain alone squiggled the read and left every uncovered call bare —
    // exactly the sites the trace exists to point at (the owner's report,
    // 2026-08-20). Each CALL hop publishes as its own diagnostic at the
    // call: the hop's own label as the message (rows 241/242), the rest of
    // the story — the other hops in entry → read order, then the read — as
    // its related information.
    #[test]
    fn a_trace_call_hop_publishes_its_own_diagnostic_at_the_call() {
        let text = "import std::context::Context;\nimport std::print;\nlet current: Context<i32> = Context::new();\nfun read_it(): i32 {\n\tcurrent.get()\n}\nfun relay(): i32 {\n\tread_it()\n}\nfun main() {\n\tprint(relay());\n}\nmain();\n";
        let group = published(text);
        let hops: Vec<&Diagnostic> = group
            .iter()
            .filter(|item| item.message == "the context requirement flows through this call")
            .collect();
        // The chain: the top-level `main();`, main's `relay()`, relay's
        // `read_it()` — each snippet's occurrence 1, past its declaration.
        let expected = [
            range_of(text, "main()", 1),
            range_of(text, "relay()", 1),
            range_of(text, "read_it()", 1),
        ];
        assert_eq!(hops.len(), 3, "{group:#?}");
        for expected_range in expected {
            let hop = hops
                .iter()
                .find(|hop| hop.range == expected_range)
                .expect("every uncovered call carries its own diagnostic");
            assert_eq!(hop.severity, Some(DiagnosticSeverity::ERROR));
            let related = hop
                .related_information
                .as_ref()
                .expect("the rest of the story travels with the hop");
            assert_eq!(
                related.len(),
                3,
                "the other two hops and the read: {related:#?}"
            );
            assert!(
                related
                    .last()
                    .unwrap()
                    .message
                    .contains("can be reached without an enclosing `run`"),
                "the story ends at the read: {related:#?}"
            );
        }
        // The primary keeps its own related chain unchanged (E78's law).
        let primary = group
            .iter()
            .find(|item| {
                item.message
                    .contains("can be reached without an enclosing `run`")
            })
            .expect("the coverage diagnostic is published");
        assert_eq!(
            primary.related_information.as_ref().map(Vec::len),
            Some(3),
            "{primary:#?}"
        );
    }

    // E81's merge law: two reads sharing an upstream frame — the owner's
    // todo client, where one `mount_root` closure feeds two `get()`s — put
    // ONE diagnostic on the shared call, its related information the union
    // of both stories, not a stacked pair of identical squiggles.
    #[test]
    fn two_reads_sharing_an_upstream_call_merge_into_one_hop_diagnostic() {
        let text = "import std::context::Context;\nimport std::print;\nlet current: Context<i32> = Context::new();\nfun first(): i32 {\n\tcurrent.get()\n}\nfun second(): i32 {\n\tcurrent.get() + first()\n}\nfun main() {\n\tprint(second());\n}\nmain();\n";
        let group = published(text);
        let hops: Vec<&Diagnostic> = group
            .iter()
            .filter(|item| item.message == "the context requirement flows through this call")
            .collect();
        // Three DISTINCT uncovered calls — `main();`, `second()`, `first()`
        // — though two chains traverse the first two.
        assert_eq!(hops.len(), 3, "{group:#?}");
        let shared: Vec<&&Diagnostic> = hops
            .iter()
            .filter(|hop| hop.range == range_of(text, "second()", 1))
            .collect();
        assert_eq!(shared.len(), 1, "one diagnostic on the shared call");
        let reads: Vec<_> = shared[0]
            .related_information
            .as_ref()
            .expect("the merged story travels with the hop")
            .iter()
            .filter(|entry| {
                entry
                    .message
                    .contains("can be reached without an enclosing `run`")
            })
            .collect();
        assert_eq!(
            reads.len(),
            2,
            "both reads in the merged story: {shared:#?}"
        );
    }

    // The elision tail (row 243) labels but never underlines: its span is
    // the last kept hop's, already underlined by that hop's own diagnostic,
    // so a diagnostic of its own would report one call twice. It still
    // rides every related-information chain.
    #[test]
    fn the_elision_tail_labels_but_never_underlines() {
        // Eight uncovered calls — two past TRACE_CAP.
        let text = "import std::context::Context;\nlet current: Context<i32> = Context::new();\nfun f8(): i32 {\n\tcurrent.get()\n}\nfun f7(): i32 {\n\tf8()\n}\nfun f6(): i32 {\n\tf7()\n}\nfun f5(): i32 {\n\tf6()\n}\nfun f4(): i32 {\n\tf5()\n}\nfun f3(): i32 {\n\tf4()\n}\nfun f2(): i32 {\n\tf3()\n}\nfun f1(): i32 {\n\tf2()\n}\nf1();\n";
        let group = published(text);
        assert!(
            !group
                .iter()
                .any(|item| item.message.contains("more uncovered call")),
            "the tail publishes no diagnostic of its own: {group:#?}"
        );
        let hops: Vec<&Diagnostic> = group
            .iter()
            .filter(|item| item.message == "the context requirement flows through this call")
            .collect();
        assert_eq!(hops.len(), 6, "the kept entry side underlines: {group:#?}");
        let primary = group
            .iter()
            .find(|item| {
                item.message
                    .contains("can be reached without an enclosing `run`")
            })
            .expect("the coverage diagnostic is published");
        let related = primary
            .related_information
            .as_ref()
            .expect("the chain travels with the diagnostic");
        assert_eq!(related.len(), 7, "six hops and the tail: {related:#?}");
        assert!(
            related
                .last()
                .unwrap()
                .message
                .contains("… 2 more uncovered calls on this path"),
            "{related:#?}"
        );
    }

    // E81 across files: a hop publishes in the file its CALL is in, not the
    // file its primary published to — the same fixture as the
    // related-information pin above, now asserting main.vl's own group.
    #[test]
    fn a_cross_file_chains_hop_diagnostics_publish_in_the_hops_file() {
        let main_text = "import std::print;\nimport pkg::lib::read_it;\nfun relay(): i32 {\n\tread_it()\n}\nfun main() {\n\tprint(relay());\n}\nmain();\n";
        let lib_text = "import std::context::Context;\nlet current: Context<i32> = Context::new();\nfun read_it(): i32 {\n\tcurrent.get()\n}\n";
        let (dir, _) = analyze_workspace(&[("main.vl", main_text), ("lib.vl", lib_text)]);
        let mut state = PublishState::new();
        let (main_uri, main_document) = open(&dir, "main.vl");
        let published = state.plan_publish(&main_uri, &main_document);
        let group_of = |suffix: &str| {
            published
                .iter()
                .find(|(target, _)| target.path().ends_with(suffix))
                .map(|(_, group)| group)
                .expect("both files publish")
        };
        let hops: Vec<&Diagnostic> = group_of("main.vl")
            .iter()
            .filter(|item| item.message == "the context requirement flows through this call")
            .collect();
        assert_eq!(hops.len(), 3, "every hop lives in main.vl: {published:#?}");
        for hop in &hops {
            let read = hop
                .related_information
                .as_ref()
                .expect("the story travels with the hop")
                .last()
                .unwrap();
            assert!(
                read.location.uri.path().ends_with("lib.vl"),
                "the story ends at the read, in ITS file: {read:#?}"
            );
        }
        assert!(
            !group_of("lib.vl")
                .iter()
                .any(|item| item.message == "the context requirement flows through this call"),
            "no hop publishes beside the primary: {published:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // E84 (diagnostics-standard.md C3a): the demotion is not std-specific.
    // A strict read inside a DEPENDENCY package (here a path dependency
    // resolved through the real manifest chain) publishes its primary at
    // the user's call, the package-internal read as related information
    // into the package's own file, hop diagnostics only at user-written
    // calls — and nothing at all into the dependency. Pre-widening (the
    // probe, 2026-08-24) the primary anchored inside `lib.vl` and the
    // package's file carried the diagnostic.
    #[test]
    fn a_dependency_reads_demotion_publishes_at_the_users_call() {
        let main_text = "import std::print;\nimport depctx::read_it;\nfun main() {\n\tprint(read_it());\n}\nmain();\n";
        let (dir, _) = analyze_workspace(&[
            ("app/src/main.vl", main_text),
            (
                "app/vilan.toml",
                "[package]\nname = \"app\"\n\n[package.dependencies]\ndepctx = { path = \"../depctx\" }\n",
            ),
            ("depctx/vilan.toml", "[library]\nname = \"depctx\"\n"),
            (
                "depctx/src/lib.vl",
                "import std::context::Context;\nlet current: Context<i32> = Context::new();\nfun read_it(): i32 {\n\tcurrent.get()\n}\n",
            ),
        ]);
        let mut state = PublishState::new();
        let (main_uri, main_document) = open(&dir, "app/src/main.vl");
        let published = state.plan_publish(&main_uri, &main_document);
        let main_group = published
            .iter()
            .find(|(target, _)| target.path().ends_with("main.vl"))
            .map(|(_, group)| group)
            .expect("main.vl publishes");
        let primary = main_group
            .iter()
            .find(|item| {
                item.message
                    .contains("can be reached without an enclosing `run`")
            })
            .expect("the coverage refusal publishes in main.vl");
        assert_eq!(
            primary.range,
            range_of(main_text, "read_it()", 0),
            "the primary sits at the USER call, never inside the package: {primary:#?}"
        );
        let related = primary
            .related_information
            .as_ref()
            .expect("trace + note travel with the primary");
        let note = related.last().unwrap();
        assert!(
            note.location.uri.path().ends_with("lib.vl"),
            "the demoted read is related information in the PACKAGE's file: {note:#?}"
        );
        assert_eq!(note.message, "the read is inside `read_it` here");
        let hops: Vec<&Diagnostic> = main_group
            .iter()
            .filter(|item| item.message == "the context requirement flows through this call")
            .collect();
        assert_eq!(
            hops.len(),
            1,
            "one user hop (the top-level `main();`): {main_group:#?}"
        );
        assert_eq!(hops[0].range, range_of(main_text, "main()", 1));
        assert!(
            !published
                .iter()
                .any(|(target, group)| target.path().ends_with("lib.vl") && !group.is_empty()),
            "nothing publishes into the dependency: {published:#?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A BOM'd module on disk publishes its diagnostic at the right COLUMN
    // (windows-support.md §2). Two reads have to agree: the analyzer's, which
    // produces the span, and the planner's, which builds the `LineIndex` that
    // converts it. Both now drop a leading BOM, so line 0's columns are counted
    // from the source proper — which is what VS Code (it strips the BOM before
    // sending a buffer) has always assumed. Disagreeing shifted every line-0
    // column, and before the strip the module did not lex at all.
    #[test]
    fn a_byte_order_marked_module_publishes_its_diagnostic_at_the_right_column() {
        // The whole module on ONE line, so the error sits on line 0 — the line
        // the BOM would shift.
        let stripped = "fun answer(): i32 { \"not a number\" }\n";
        let (dir, _) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::marked::answer;\nfun main() { print(answer()); }\n",
            ),
            ("marked.vl", &format!("\u{feff}{stripped}")),
        ]);
        let mut state = PublishState::new();
        let (main_uri, main_document) = open(&dir, "main.vl");
        let published = state.plan_publish(&main_uri, &main_document);
        let group = published
            .iter()
            .find(|(target, _)| target.path().ends_with("marked.vl"))
            .map(|(_, group)| group)
            .expect("the module error publishes at marked.vl");
        let diagnostic = group
            .iter()
            .find(|item| item.message.contains("Expected i32"))
            .expect("the module's type error is published");
        let column = stripped.find("\"not a number\"").unwrap() as u32;
        assert_eq!(
            (
                diagnostic.range.start.line,
                diagnostic.range.start.character
            ),
            (0, column),
            "line-0 columns must be counted past the BOM"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // THE Windows bug (`windows-support.md` §7): VS Code sends
    // `file:///c%3A/…` and the server mints `file:///C:/…` for one file, so a
    // raw-`Url` planner held TWO slots — the squiggle appeared twice and fixing
    // the code cleared only one of them. Both spellings are constructed from
    // strings here, which is exactly what arrives on the wire; the planner is
    // asked to key by the Windows rule (`for_platform`), the way the S3 helpers
    // make both platforms' path rules testable from either one.
    //
    // Asserted on the planner's slot rather than on a replayed editor map,
    // because a real client never spells ONE open buffer two ways — the two
    // producers in the wild are an open document and another owner's cross-file
    // group (pinned end-to-end by the next test).
    #[test]
    fn the_two_windows_spellings_of_one_file_share_one_planner_slot() {
        let (dir, _) =
            analyze_workspace(&[("solo.vl", "fun main() {\n\tlet wrong: i32 = \"text\";\n}\n")]);
        let (_, broken) = open(&dir, "solo.vl");
        std::fs::write(
            dir.join("solo.vl"),
            "fun main() {\n\tlet right: i32 = 1;\n}\n",
        )
        .unwrap();
        let (_, fixed) = open(&dir, "solo.vl");

        let minted = Url::parse("file:///C:/project/solo.vl").unwrap();
        let from_client = Url::parse("file:///c%3A/project/solo.vl").unwrap();
        assert_ne!(minted, from_client, "the fixture must start apart");

        for windows in [true, false] {
            let mut state = PublishState::for_platform(windows);
            let published = state.plan_publish(&minted, &broken);
            assert_eq!(published.len(), 1, "the entry is its own only target");
            assert_eq!(published[0].1.len(), 1, "the error publishes once");

            // The same file, arriving under the client's spelling, now analyzing
            // clean. One slot ⇒ the error is gone; two slots ⇒ the first
            // spelling keeps a squiggle nothing can ever clear.
            let republished = state.plan_publish(&from_client, &fixed);
            assert_eq!(
                republished[0].0, from_client,
                "an open document is addressed in the client's own spelling"
            );
            let key = state.key(&minted);
            let slots = (state.owned.len(), state.merged(&key).len());
            if windows {
                assert_eq!(
                    slots,
                    (1, 0),
                    "one file must occupy one slot under the Windows rule"
                );
            } else {
                // The non-vacuity half, host-dependent so pinned only where it
                // can be observed: `normalize`'s `to_file_path` → `from_file_path`
                // round trip is the OS's own, and on a Windows host it folds
                // `c%3A` → `C:` under EITHER rule — so the duplication the unix
                // rule leaves behind shows up on Linux only. The production claim
                // above runs on both hosts.
                #[cfg(not(windows))]
                assert_eq!(
                    slots,
                    (2, 1),
                    "the unix rule is what leaves one file duplicated"
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The same collision end to end, in the shape it actually takes: a module is
    // open under the client's spelling AND is a cross-file target of an open
    // importer, which addresses it by the spelling the analysis minted. Two keys
    // meant the one error rendered twice, in two Problems entries. The
    // percent-encoding divergence used here is the portable stand-in for the
    // drive-letter one — same mechanism, observable on every platform.
    #[test]
    fn an_open_module_and_its_importer_publish_one_merged_view() {
        let (dir, _) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
            ),
            ("broken.vl", "fun answer(): i32 {\n\t\"not a number\"\n}\n"),
        ]);
        let (minted_module_uri, module_document) = open(&dir, "broken.vl");
        let (main_uri, main_document) = open(&dir, "main.vl");
        // What the client sends for the module. `Url::from_file_path` never
        // writes this form; `Url::parse` is the wire.
        let module_from_client = Url::parse(
            &minted_module_uri
                .as_str()
                .replace("/broken.vl", "/%62roken.vl"),
        )
        .unwrap();
        assert_ne!(module_from_client, minted_module_uri, "spellings differ");

        let mut state = PublishState::new();
        let mut editor: BTreeMap<Url, Vec<Diagnostic>> = BTreeMap::new();
        apply(
            &mut editor,
            state.plan_publish(&module_from_client, &module_document),
        );
        apply(&mut editor, state.plan_publish(&main_uri, &main_document));

        let showing = visible(&editor);
        assert_eq!(
            showing.keys().collect::<Vec<_>>(),
            vec![&module_from_client],
            "one entry, at the URI the client opened the module with"
        );
        assert_eq!(
            showing[&module_from_client].len(),
            1,
            "the importer's view of the module error merges with the module's own"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The address rule: an open document's notifications carry the client's
    // exact URI, publish and close alike, even though the planner keys on the
    // canonical form. The client's key is authoritative for its own buffers
    // (the §2 principle for buffer text, applied to addressing).
    #[test]
    fn an_open_documents_notification_keeps_the_clients_spelling() {
        let (dir, _) =
            analyze_workspace(&[("solo.vl", "fun main() {\n\tlet wrong: i32 = \"text\";\n}\n")]);
        let (minted_uri, document) = open(&dir, "solo.vl");
        let from_client =
            Url::parse(&minted_uri.as_str().replace("/solo.vl", "/%73olo.vl")).unwrap();
        assert_ne!(from_client, minted_uri, "the fixture must start apart");
        // The claim is that the two spellings share ONE key — so both sides go
        // through the seam, against a file that exists. The minted spelling is
        // not itself the key: it is whatever `temp_dir()` handed the fixture, and
        // on a Windows runner that is an 8.3 short name (`…\RUNNER~1\…` for a
        // `runneradmin` profile) which `fs::canonicalize` expands inside the seam
        // and `Url::from_file_path` does not.
        assert_eq!(
            crate::uri::normalize(&from_client, cfg!(windows)),
            crate::uri::normalize(&minted_uri, cfg!(windows)),
            "…yet name one file"
        );

        let mut state = PublishState::new();
        let published = state.plan_publish(&from_client, &document);
        assert_eq!(published.len(), 1);
        assert_eq!(
            published[0].0, from_client,
            "published at the client's spelling, not the planner's key"
        );
        assert!(!published[0].1.is_empty(), "the error is there to publish");

        let closed = state.plan_close(&from_client);
        assert_eq!(closed.len(), 1);
        assert_eq!(
            closed[0].0, from_client,
            "the clear reaches the same URI the diagnostic went to"
        );
        assert!(closed[0].1.is_empty(), "closing clears it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // The E6 lifecycle property: after every open/edit/close, what the
    // editor shows (the applied actions) equals a fresh analysis of the
    // currently-open documents — nothing stale, nothing lost. The scenario
    // exercises the shared-dependency union: two open documents import the
    // same broken module, then one drops it, then each closes.
    #[test]
    fn published_equals_fresh_analysis_across_the_lifecycle() {
        let broken = "fun answer(): i32 {\n\t\"not a number\"\n}\n";
        let (dir, _) = analyze_workspace(&[
            (
                "main.vl",
                "import std::print;\nimport pkg::broken::answer;\nfun main() { print(answer()); }\n",
            ),
            (
                "other.vl",
                "import std::print;\nimport pkg::broken::answer;\nfun main() { print(answer() + 1); }\n",
            ),
            ("broken.vl", broken),
        ]);
        let mut state = PublishState::new();
        let mut editor: BTreeMap<Url, Vec<Diagnostic>> = BTreeMap::new();

        // Open main: the module error shows at broken.vl.
        let (main_uri, main_document) = open(&dir, "main.vl");
        apply(&mut editor, state.plan_publish(&main_uri, &main_document));
        assert_eq!(
            visible(&editor),
            fresh_view(&[(&main_uri, &main_document)]),
            "after opening main"
        );
        let broken_uri = visible(&editor)
            .keys()
            .find(|target| target.path().ends_with("broken.vl"))
            .cloned()
            .expect("the module error publishes at broken.vl");

        // Open other: both owners see the same module error — the union
        // holds ONE copy, not last-writer's.
        let (other_uri, other_document) = open(&dir, "other.vl");
        apply(&mut editor, state.plan_publish(&other_uri, &other_document));
        assert_eq!(
            visible(&editor),
            fresh_view(&[(&main_uri, &main_document), (&other_uri, &other_document)]),
            "after opening other"
        );
        assert_eq!(
            editor.get(&broken_uri).map(Vec::len),
            Some(1),
            "identical views of the shared module deduplicate"
        );

        // Edit main to drop the import: broken.vl must KEEP other's view —
        // the last-writer-wins case the union exists for.
        std::fs::write(
            dir.join("main.vl"),
            "import std::print;\nfun main() { print(1); }\n",
        )
        .unwrap();
        let (_, main_edited) = open(&dir, "main.vl");
        apply(&mut editor, state.plan_publish(&main_uri, &main_edited));
        assert_eq!(
            visible(&editor),
            fresh_view(&[(&main_uri, &main_edited), (&other_uri, &other_document)]),
            "after editing main"
        );
        assert_eq!(
            editor.get(&broken_uri).map(Vec::len),
            Some(1),
            "the remaining owner's view of the shared module survives"
        );

        // Close other: no owner sees broken.vl any more — explicit empty.
        apply(&mut editor, state.plan_close(&other_uri));
        assert_eq!(
            visible(&editor),
            fresh_view(&[(&main_uri, &main_edited)]),
            "after closing other"
        );
        assert_eq!(
            editor.get(&broken_uri).map(Vec::len),
            Some(0),
            "the dropped module target clears explicitly"
        );

        // Close main: everything clears.
        apply(&mut editor, state.plan_close(&main_uri));
        assert_eq!(visible(&editor), BTreeMap::new(), "after closing main");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // An entry's OWN diagnostics publish at its URI, update in place on an
    // edit that fixes them, and clear on close — the single-document
    // lifecycle (explicit empties included).
    #[test]
    fn own_diagnostics_update_and_clear_with_the_document() {
        let (dir, _) =
            analyze_workspace(&[("solo.vl", "fun main() {\n\tlet wrong: i32 = \"text\";\n}\n")]);
        let mut state = PublishState::new();
        let mut editor: BTreeMap<Url, Vec<Diagnostic>> = BTreeMap::new();

        let (uri, document) = open(&dir, "solo.vl");
        apply(&mut editor, state.plan_publish(&uri, &document));
        assert!(
            editor.get(&uri).is_some_and(|group| !group.is_empty()),
            "the type error publishes at the entry"
        );
        assert_eq!(visible(&editor), fresh_view(&[(&uri, &document)]));

        std::fs::write(
            dir.join("solo.vl"),
            "fun main() {\n\tlet right: i32 = 1;\n}\n",
        )
        .unwrap();
        let (_, fixed) = open(&dir, "solo.vl");
        apply(&mut editor, state.plan_publish(&uri, &fixed));
        assert_eq!(
            editor.get(&uri).map(Vec::len),
            Some(0),
            "fixing the error publishes an explicit empty"
        );

        apply(&mut editor, state.plan_close(&uri));
        assert_eq!(visible(&editor), BTreeMap::new());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // S3 (editing-dx.md §3.2): a missing return value used to publish a
    // ZERO-WIDTH range one byte PAST the closing brace — `start == end`, and
    // VS Code draws a caret for that, not an underline, so the diagnostic
    // was invisible in the editor even though the CLI rendered it
    // tolerably. It now publishes the brace itself, exactly one character
    // wide: `[2:0 .. 2:1]` on this source, where line 2 is the lone `}`.
    #[test]
    fn a_missing_return_value_publishes_a_one_character_range_not_a_zero_width_one() {
        let (dir, _) = analyze_workspace(&[(
            "main.vl",
            "fun total(a: i32, b: i32): i32 {\n\tlet sum: i32 = a + b;\n}\n\n\
             fun main() { total(1, 2); }\n",
        )]);
        let mut state = PublishState::new();
        let (uri, document) = open(&dir, "main.vl");
        let published = state.plan_publish(&uri, &document);
        let group = published
            .iter()
            .find(|(target, _)| *target == uri)
            .map(|(_, group)| group)
            .expect("main.vl publishes its own diagnostic");
        let diagnostic = group
            .iter()
            .find(|item| {
                item.message
                    .contains("this body ends without producing a value")
            })
            .expect("the missing-return-value diagnostic is published");
        assert_eq!(
            diagnostic.range.start,
            Position {
                line: 2,
                character: 0
            },
            "the `}}` starts line 2 (0-based)"
        );
        assert_eq!(
            diagnostic.range.end,
            Position {
                line: 2,
                character: 1
            },
            "one character wide — not the old start == end zero-width range"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
