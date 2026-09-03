//! M26 (`proposal/editor-latency.md` §4.2): which analysis each open document
//! owes, and how to stop the ones it no longer does.
//!
//! Before this, the server's whole scheduling state was one number per
//! document — the edit generation a debounced pause compares itself against —
//! and the only thing that ever happened to a superseded analysis was that its
//! result got dropped at `land`. The analysis itself ran to the end on its
//! 128 MiB thread. Two consequences the item names:
//!
//! - a keystroke burst pays one WHOLE analysis per debounce window, all but the
//!   last of them for answers nobody will see;
//! - `did_open` (E123) schedules through the same path but registered no
//!   generation at all, so an edit arriving right after an open did not
//!   supersede the open's analysis — the two raced, and which one landed last
//!   was decided by E117's revision stamp rather than by anyone's intent.
//!
//! So the generation gains a companion: the [`CancelToken`] of every analysis
//! in flight for that document. Bumping the generation cancels the older ones
//! in the same breath, which is what makes "supersede" mean something before
//! the analysis finishes rather than only after.
//!
//! **The decisions live here, the effects live in `main.rs`.** Nothing in this
//! file spawns, awaits, or publishes; every method is a synchronous map
//! operation, which is what lets the pins below exercise the scheduler without
//! a server, a runtime or a filesystem — the same separation `pause_action` and
//! `recolored_package` have.
//!
//! **It is never what correctness rests on.** `land` and `plan_publish` keep
//! their E117 revision-stamp checks exactly as they were. A superseded analysis
//! that outruns its own cancellation still returns a result, and that result is
//! still dropped. Everything here only decides how much CPU was spent first.

use dashmap::DashMap;
use tower_lsp::lsp_types::Url;
use vilan_core::cancel::CancelToken;

/// One analysis in flight: the ticket that retires it, the generation of its
/// document it was started for, and the token it runs under.
///
/// The ticket is what `finish` retires, rather than the generation: the
/// dependency sweep and a document's own debounce can both be scheduling for
/// it, so two registrations can share a generation and retiring by generation
/// would let one analysis retire another's.
struct Running {
    ticket: u64,
    generation: u64,
    token: CancelToken,
}

/// One document's scheduling state.
#[derive(Default)]
struct DocumentSchedule {
    /// The latest edit generation. Bumped by every `did_change`, and — new in
    /// M26 — by `did_open`, so an open's analysis is superseded by a following
    /// edit exactly as an edit's is.
    generation: u64,
    /// The analyses currently in flight for this document. Normally zero or
    /// one; briefly two while a cancelled analysis is between its last
    /// checkpoint and its thread's exit, which is precisely the window this
    /// whole mechanism shortens.
    running: Vec<Running>,
    /// The ticket of the next analysis registered for this document.
    next_ticket: u64,
    /// Whether a module this document imports has been edited since this
    /// document's last landed analysis (§2.1.2's case 4). Set when the
    /// dependency sweep schedules this document, cleared when its analysis
    /// lands — the window in which the keystroke path's verdict degrades to
    /// `Stale`: whole-file syntax-only tokens, hints still served (Q1/Q4).
    dependency_moved: bool,
}

/// What [`Schedule::start`] hands back: the token the analysis runs under, and
/// the ticket that retires it.
///
/// Ticket `0` means "not registered" — the generation was already stale, or the
/// document had closed — and no real registration ever carries it, so
/// [`Schedule::finish`] on such a `Started` matches nothing and is the no-op it
/// should be. The token in that case is already cancelled.
pub struct Started {
    ticket: u64,
    pub token: CancelToken,
}

/// The server's analysis scheduler state, keyed by document.
///
/// An entry exists from the document's open until its close. `DashMap` for the
/// same reason `documents` is one: every access is synchronous, taken and
/// dropped without crossing an `await`.
#[derive(Default)]
pub struct Schedule {
    documents: DashMap<Url, DocumentSchedule>,
}

impl Schedule {
    /// Record a new edit (or open) of `uri`: bump its generation, and CANCEL
    /// every analysis in flight for an older one. Returns the new generation,
    /// which the caller carries into [`start`](Schedule::start).
    ///
    /// This is the whole of "supersede". The generation half is what the
    /// debounced pause has always compared against; the cancel half is what
    /// stops the superseded analysis from finishing work for an answer that
    /// `land` would drop.
    pub fn supersede(&self, uri: &Url) -> u64 {
        let mut entry = self.documents.entry(uri.clone()).or_default();
        entry.generation += 1;
        let generation = entry.generation;
        cancel_older_than(&entry, generation);
        generation
    }

    /// The document's current edit generation, for the debounced pause's
    /// [`crate::pause_action`]. `None` when the document has no schedule —
    /// closed, or never opened — which supersedes any pause waiting on it.
    pub fn generation(&self, uri: &Url) -> Option<u64> {
        self.documents.get(uri).map(|entry| entry.generation)
    }

    /// Register an analysis about to start for `generation` of `uri`, and hand
    /// back the token it runs under.
    ///
    /// The token comes back ALREADY CANCELLED when `generation` is no longer
    /// the document's latest (or the document has closed) — the caller's edit
    /// won the race between scheduling this analysis and superseding it — so
    /// the analysis stops at its first checkpoint instead of running a whole
    /// program's worth of work for a result that cannot land. That is the one
    /// ordering hazard the design has, and it is closed here rather than left
    /// to a caller to remember.
    pub fn start(&self, uri: &Url, generation: u64) -> Started {
        let token = CancelToken::new();
        let Some(mut entry) = self.documents.get_mut(uri) else {
            token.cancel();
            return Started { ticket: 0, token };
        };
        if entry.generation != generation {
            token.cancel();
            return Started { ticket: 0, token };
        }
        entry.next_ticket += 1;
        let ticket = entry.next_ticket;
        entry.running.push(Running {
            ticket,
            generation,
            token: token.clone(),
        });
        Started { ticket, token }
    }

    /// Retire the analysis `started` registered — it landed, was dropped, or
    /// stopped at a checkpoint. Idempotent, and silent about a document that
    /// has closed under it.
    pub fn finish(&self, uri: &Url, started: &Started) {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry
                .running
                .retain(|running| running.ticket != started.ticket);
        }
    }

    /// Mark that a module `uri` imports has been edited and this document has
    /// not re-analyzed yet — the window in which its keystroke-path verdict is
    /// `Stale` (§2.1.2's case 4).
    pub fn mark_dependency_moved(&self, uri: &Url) {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry.dependency_moved = true;
        }
    }

    /// Clear the mark: this document's analysis has landed over the edited
    /// dependency, so its landed snapshot is current again.
    pub fn clear_dependency_moved(&self, uri: &Url) {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry.dependency_moved = false;
        }
    }

    /// What the keystroke path's `dependency_moved` argument is for `uri` right
    /// now. `false` for a document with no schedule: nothing has been said
    /// about it, and the landed answer (if any) stands.
    pub fn dependency_moved(&self, uri: &Url) -> bool {
        self.documents
            .get(uri)
            .is_some_and(|entry| entry.dependency_moved)
    }

    /// Forget `uri` and cancel whatever it had running — the close. Dropping
    /// the generation is what makes an in-flight debounced pause bail; the
    /// cancel is what stops an analysis that is already past the pause.
    pub fn close(&self, uri: &Url) {
        if let Some((_, entry)) = self.documents.remove(uri) {
            for running in &entry.running {
                running.token.cancel();
            }
        }
    }

    /// How many documents have a schedule — the session trace's `pending`
    /// cardinality, unchanged in meaning from the bare generation map this
    /// replaced.
    pub fn len(&self) -> usize {
        self.documents.len()
    }
}

/// Cancel every analysis registered for a generation older than `generation`.
///
/// The registrations STAY: each analysis retires its own ticket when its task
/// returns, which is the one place that knows the analysis is really over —
/// dropping it here would lose the token before the thread had read it.
/// Cancelling is idempotent, so a second supersede over the same list costs a
/// store.
fn cancel_older_than(entry: &DocumentSchedule, generation: u64) {
    for running in &entry.running {
        if running.generation < generation {
            running.token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(name: &str) -> Url {
        Url::parse(&format!("file:///tmp/{name}.vl")).expect("a file url")
    }

    #[test]
    fn a_fresh_document_has_no_generation() {
        let schedule = Schedule::default();
        assert_eq!(schedule.generation(&uri("a")), None);
        assert_eq!(schedule.len(), 0);
    }

    #[test]
    fn superseding_counts_up_from_one() {
        let schedule = Schedule::default();
        assert_eq!(schedule.supersede(&uri("a")), 1);
        assert_eq!(schedule.supersede(&uri("a")), 2);
        assert_eq!(schedule.generation(&uri("a")), Some(2));
        assert_eq!(schedule.len(), 1);
    }

    /// The core of it: the analysis started for generation 1 is CANCELLED by
    /// the edit that makes generation 2, rather than left to run and be dropped
    /// at `land`.
    #[test]
    fn a_newer_generation_cancels_the_older_analysis() {
        let schedule = Schedule::default();
        let document = uri("a");
        let first = schedule.supersede(&document);
        let started = schedule.start(&document, first);
        assert!(!started.token.is_cancelled(), "it has only just started");
        schedule.supersede(&document);
        assert!(
            started.token.is_cancelled(),
            "the newer edit stopped the analysis the older one scheduled",
        );
    }

    /// The ordering hazard: an edit that lands between the pause's decision to
    /// analyze and the registration of its token. The analysis is born
    /// cancelled rather than running unwatched to the end.
    #[test]
    fn starting_a_superseded_generation_yields_a_cancelled_token() {
        let schedule = Schedule::default();
        let document = uri("a");
        let stale = schedule.supersede(&document);
        schedule.supersede(&document);
        assert!(
            schedule.start(&document, stale).token.is_cancelled(),
            "generation {stale} is no longer the document's latest",
        );
    }

    #[test]
    fn starting_an_analysis_of_a_closed_document_yields_a_cancelled_token() {
        let schedule = Schedule::default();
        let document = uri("a");
        let generation = schedule.supersede(&document);
        schedule.close(&document);
        assert!(schedule.start(&document, generation).token.is_cancelled());
        assert_eq!(schedule.generation(&document), None);
        assert_eq!(schedule.len(), 0);
    }

    #[test]
    fn closing_cancels_what_was_running() {
        let schedule = Schedule::default();
        let document = uri("a");
        let generation = schedule.supersede(&document);
        let started = schedule.start(&document, generation);
        schedule.close(&document);
        assert!(
            started.token.is_cancelled(),
            "the close stopped the analysis"
        );
    }

    /// A finished analysis is retired, so a later supersede has nothing to
    /// cancel — the list does not grow with the session.
    #[test]
    fn finishing_retires_the_registration() {
        let schedule = Schedule::default();
        let document = uri("a");
        let generation = schedule.supersede(&document);
        let started = schedule.start(&document, generation);
        schedule.finish(&document, &started);
        schedule.supersede(&document);
        assert!(
            !started.token.is_cancelled(),
            "a retired analysis is not cancelled retroactively",
        );
    }

    #[test]
    fn the_dependency_mark_is_set_and_cleared() {
        let schedule = Schedule::default();
        let document = uri("a");
        assert!(
            !schedule.dependency_moved(&document),
            "an unknown document says nothing",
        );
        schedule.supersede(&document);
        assert!(!schedule.dependency_moved(&document));
        schedule.mark_dependency_moved(&document);
        assert!(schedule.dependency_moved(&document));
        schedule.clear_dependency_moved(&document);
        assert!(!schedule.dependency_moved(&document));
    }

    /// Two documents keep separate schedules: superseding one does not cancel
    /// the other's analysis.
    #[test]
    fn documents_do_not_share_a_schedule() {
        let schedule = Schedule::default();
        let (first, second) = (uri("a"), uri("b"));
        let first_generation = schedule.supersede(&first);
        let first_started = schedule.start(&first, first_generation);
        let second_generation = schedule.supersede(&second);
        let second_started = schedule.start(&second, second_generation);
        schedule.supersede(&first);
        assert!(first_started.token.is_cancelled());
        assert!(!second_started.token.is_cancelled());
        assert_eq!(schedule.len(), 2);
    }
}
