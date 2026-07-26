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
        if let Some(previous) = self.owned.get(&owner_key) {
            for (target, _) in previous {
                if !affected.contains(target) {
                    affected.push(target.clone());
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

/// Attaches a diagnostic's C3 note as LSP related information (backlog E17) —
/// a location plus a message, which is exactly what a declaration note is.
/// `home` is the file the diagnostic itself was published to, with its index:
/// a note with no file of its own lives there (the entry, or the module the
/// diagnostic was attributed to). A note in another file is read fresh, like
/// the diagnostic's own file is. An unreadable note file drops the related
/// information only — never the diagnostic.
fn attach_note(
    converted: &mut Diagnostic,
    item: &PublishedDiagnostic,
    home: &Url,
    home_index: &LineIndex,
) {
    let Some((note_span, note_msg, note_path)) = &item.note else {
        return;
    };
    let located = match note_path {
        None => Some((home.clone(), home_index.range(note_span))),
        Some(path) => vilan_core::util::read_source(path)
            .ok()
            .map(|text| LineIndex::new(&text).range(note_span))
            .and_then(|range| Url::from_file_path(path).ok().map(|target| (target, range))),
    };
    if let Some((target, range)) = located {
        converted.related_information = Some(vec![DiagnosticRelatedInformation {
            location: Location { uri: target, range },
            message: note_msg.clone(),
        }]);
    }
}

/// One analyzed document's diagnostics as per-target groups: the entry's own
/// (always present, even when empty, so the owner's URI is always brought
/// current) plus each imported file's, with spans converted through a fresh
/// read of *that* file — the analysis read it from disk too, so they agree.
fn diagnostic_groups(document: &Document, owner: &Url) -> Vec<(Url, Vec<Diagnostic>)> {
    let mut entry_group: Vec<Diagnostic> = Vec::new();
    let mut extra_groups: Vec<(Url, Vec<Diagnostic>)> = Vec::new();
    let mut extra_indices: HashMap<PathBuf, Option<Arc<LineIndex>>> = HashMap::new();
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
                let mut converted = diagnostic(document.line_index.range(&item.span));
                // A secondary note becomes related information — "first
                // call here"-style anchors.
                attach_note(&mut converted, &item, owner, &document.line_index);
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
                        attach_note(&mut converted, &item, &target, &index);
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
    let mut groups = vec![(owner.clone(), entry_group)];
    groups.extend(extra_groups);
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::tests::{analyze_workspace, std_root};
    use std::path::Path;

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
}
