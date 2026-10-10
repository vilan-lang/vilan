//! `dbg_stack()` (debugging.md S2, E257): the call, and the two per-site
//! records its expansion needs (E281, E282).
//!
//! The call resolves like any zero-argument call. What it may print is the
//! scope at the call, and two facts no binding carries: whether a resource is
//! MOVED there (the move scan's `MoveFlow`), and whether a capture view past its
//! last use was INVALIDATED by a rule-4 event since (the invalidation scan's).
//! Reading either at the call would be refused, so the expansion must be told
//! both rather than mint a read. Neither check kept state per site, so each now
//! files what it holds at a `dbg_stack()` call on the analyzer
//! ([`Analyzer::record_dbg_stack_moves`], [`DbgStackViewState`]), keyed by the
//! call, and both reach the `Program`.
//!
//! Both scans run inside M19 T1's Class A window, which skips a REUSED module's
//! bodies, so a module holding a `dbg_stack()` call is never recorded for
//! reuse ([`Analyzer::mark_dbg_stack_modules_unrecordable`]): its scans, and
//! these records with them, run on every analysis (B575's rule).

use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use crate::span::Span;
use crate::type_::TypeId;

use super::{Analyzer, Expr, MoveState, Resolution, SourceId};

/// E281: a resource binding's move state at a `dbg_stack()` call, as the
/// resource move scan holds it there, with the span of a move that spent it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DbgStackMove {
    /// Moved on every path reaching the call.
    Moved(Span),
    /// Moved on some paths only — legal where the others provably hold no
    /// payload (B67), so the binding may or may not still own one.
    MovedOnSomePaths(Span),
}

/// E282: a capture view a `dbg_stack()` call must not read — past its last
/// use, with a rule-4 event on its root since.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DbgStackInvalidation {
    pub view: Id,
    /// The event's anchor: the mutating call, or the assignment's target.
    pub event: Id,
    /// The callee of a mutating call (`push`); `None` for an assignment.
    pub callee: Option<Id>,
}

/// E282: a capture view out of the live set, as the invalidation scan sees it.
#[derive(Clone, Copy)]
struct RetiredView {
    /// The loop depth the view's arm sits at.
    depth: u32,
    event: Option<Rule4Event>,
}

/// A rule-4 event: the anchor, and the callee for a mutating call.
#[derive(Clone, Copy)]
struct Rule4Event {
    anchor: Id,
    callee: Option<Id>,
}

/// E282: one `dbg_stack()` call's record while the scan is still running.
pub(super) struct DbgStackViewSite {
    call: Id,
    /// The deepest loop around the call that the scan is still inside: an
    /// event in it, later in the walk, comes BEFORE the call on the next
    /// iteration.
    open_depth: u32,
    views: Vec<(Id, RetiredView)>,
}

/// One branch point's paths (an `if` chain, a `match`).
struct RetiredFork {
    entry: HashMap<Id, RetiredView>,
    joined: HashMap<Id, RetiredView>,
}

/// E282: what the view-invalidation scan keeps for the `dbg_stack()` calls in
/// a body — the capture views past their last use (B509 Q3 retires a capture
/// view there, so a later push is legal while a READ there would make it a
/// rule-4 error) and the first event on each one's root since. Inert unless
/// `tracking` (a program with a `dbg_stack()` call): every method returns at
/// once, so the scan pays nothing for it otherwise.
#[derive(Default)]
pub(super) struct DbgStackViewState {
    pub(super) tracking: bool,
    retired: HashMap<Id, RetiredView>,
    loop_depth: u32,
    forks: Vec<RetiredFork>,
    pub(super) sites: Vec<DbgStackViewSite>,
}

impl DbgStackViewState {
    /// `view` left the live set at the scan's position (its last use, or an
    /// arm that never uses it).
    pub(super) fn retire(&mut self, view: Id) {
        if self.tracking {
            self.retired.insert(
                view,
                RetiredView {
                    depth: self.loop_depth,
                    event: None,
                },
            );
        }
    }

    /// The arm binding `views` ended: they are out of scope.
    pub(super) fn leave_scope(&mut self, views: &[Id]) {
        if self.tracking {
            for view in views {
                self.retired.remove(view);
            }
        }
    }

    /// A rule-4 event at `anchor`, invalidating every view `invalidates`
    /// answers for: a retired view's first event, and — retroactively — a view
    /// a call earlier in a loop still open here listed, when the view's arm
    /// encloses that loop (the event precedes the call on the next iteration).
    pub(super) fn note_event(
        &mut self,
        anchor: Id,
        callee: Option<Id>,
        invalidates: impl Fn(&Id) -> bool,
    ) {
        if !self.tracking {
            return;
        }
        let event = Rule4Event { anchor, callee };
        for (view, retired) in self.retired.iter_mut() {
            if retired.event.is_none() && invalidates(view) {
                retired.event = Some(event);
            }
        }
        for site in &mut self.sites {
            for (view, retired) in &mut site.views {
                if retired.event.is_none() && retired.depth < site.open_depth && invalidates(view) {
                    retired.event = Some(event);
                }
            }
        }
    }

    /// A `dbg_stack()` call: what it may not read, as of here.
    pub(super) fn note_site(&mut self, call: Id) {
        if !self.tracking {
            return;
        }
        let mut views: Vec<(Id, RetiredView)> = self
            .retired
            .iter()
            .map(|(view, retired)| (*view, *retired))
            .collect();
        views.sort_unstable_by_key(|(view, _)| view.0);
        self.sites.push(DbgStackViewSite {
            call,
            open_depth: self.loop_depth,
            views,
        });
    }

    pub(super) fn enter_loop(&mut self) {
        if self.tracking {
            self.loop_depth += 1;
        }
    }

    pub(super) fn leave_loop(&mut self) {
        if self.tracking {
            self.loop_depth = self.loop_depth.saturating_sub(1);
            for site in &mut self.sites {
                site.open_depth = site.open_depth.min(self.loop_depth);
            }
        }
    }

    /// A branch point: the paths that follow start from the state here.
    pub(super) fn fork(&mut self) {
        if self.tracking {
            self.forks.push(RetiredFork {
                entry: self.retired.clone(),
                joined: HashMap::default(),
            });
        }
    }

    /// The next path of the innermost branch point: the entry state, plus the
    /// views an earlier path retired (the scan's live set is shared between
    /// paths, so those stay out of it) without that path's events.
    pub(super) fn enter_path(&mut self) {
        if !self.tracking {
            return;
        }
        let Some(fork) = self.forks.last() else {
            return;
        };
        let mut start = fork.entry.clone();
        for (view, retired) in &fork.joined {
            start.entry(*view).or_insert(RetiredView {
                depth: retired.depth,
                event: None,
            });
        }
        self.retired = start;
    }

    /// A path ended: an event on it holds after the branch point.
    pub(super) fn leave_path(&mut self) {
        if !self.tracking {
            return;
        }
        let Some(fork) = self.forks.last_mut() else {
            return;
        };
        for (view, retired) in &self.retired {
            let slot = fork.joined.entry(*view).or_insert(RetiredView {
                depth: retired.depth,
                event: None,
            });
            if slot.event.is_none() {
                slot.event = retired.event;
            }
        }
    }

    /// The branch point's paths joined: a view is invalidated after it when
    /// it was on some path (an `if` without `else` keeps the entry's own).
    pub(super) fn join(&mut self) {
        if !self.tracking {
            return;
        }
        let Some(fork) = self.forks.pop() else {
            return;
        };
        let mut joined = fork.joined;
        for (view, retired) in fork.entry {
            joined.entry(view).or_insert(retired);
        }
        self.retired = joined;
    }
}

impl<'src> Analyzer<'src> {
    /// debugging.md S2: a `dbg_stack()` call. It takes no arguments — the
    /// expansion supplies them — and types as `void`.
    #[inline(never)]
    pub(super) fn resolve_dbg_stack_call(
        &mut self,
        call_id: Id,
        subject_id: Id,
        generic_argument_ids: &[TypeId],
        argument_ids: &[Id],
        arguments_span: Span,
    ) -> Resolution {
        if !argument_ids.is_empty() || !generic_argument_ids.is_empty() {
            self.diagnostics.push(crate::error::Error {
                trace: Vec::new(),
                note: None,
                span: self.clamp_span_to_first_line(arguments_span, call_id),
                msg: "`dbg_stack()` takes no arguments: it prints every binding in scope at \
                      the call. To print chosen values, use `dbg(..)`"
                    .to_string(),
            });
            return Resolution::Failed;
        }
        self.wire_call(call_id, subject_id, &[], &[], arguments_span);
        self.dbg_stack_calls.insert(subject_id, call_id);
        Resolution::Resolved
    }

    /// A module holding a `dbg_stack()` call keeps no Class A record (see the
    /// module docs): its two per-site records are re-derived every time.
    pub(super) fn mark_dbg_stack_modules_unrecordable(&mut self) {
        let sources: Vec<SourceId> = self
            .dbg_stack_calls
            .values()
            .filter_map(|call_id| self.source_of_id(*call_id))
            .collect();
        for source in sources {
            self.reuse_unrecordable.insert(source.0);
        }
    }

    /// E281: the move scan's state at each `dbg_stack()` call — the concrete
    /// scan's, and R11's for each instantiation of a generic body (the first
    /// verdict a binding gets stands).
    pub(super) fn record_dbg_stack_moves(&mut self, records: Vec<(Id, Vec<(Id, MoveState)>)>) {
        for (call_id, moved) in records {
            let entry = self.dbg_stack_moves.entry(call_id).or_default();
            for (binding, state) in moved {
                if entry.iter().any(|(known, _)| *known == binding) {
                    continue;
                }
                let state = match state {
                    MoveState::Moved(span) => DbgStackMove::Moved(span),
                    MoveState::MaybeMoved(span) => DbgStackMove::MovedOnSomePaths(span),
                };
                entry.push((binding, state));
            }
        }
    }

    /// E282: the views each `dbg_stack()` call may not read.
    pub(super) fn record_dbg_stack_invalidations(&mut self, sites: Vec<DbgStackViewSite>) {
        for site in sites {
            let invalidated: Vec<DbgStackInvalidation> = site
                .views
                .into_iter()
                .filter_map(|(view, retired)| {
                    let event = retired.event?;
                    Some(DbgStackInvalidation {
                        view,
                        event: event.anchor,
                        callee: event.callee,
                    })
                })
                .collect();
            if !invalidated.is_empty() {
                self.dbg_stack_invalidated
                    .entry(site.call)
                    .or_default()
                    .extend(invalidated);
            }
        }
    }

    /// E282: an assignment's rule-4 events — a whole reassignment of a root
    /// (E1) and a write to a part of one that holds a view (B529), judged as
    /// `scan_invalidation` judges a live view.
    pub(super) fn note_dbg_stack_assignment(
        &self,
        target_id: Id,
        scan: &super::InvalidationScan<'_>,
        state: &mut super::InvalidationScanState,
    ) {
        if let Some(Expr::Local(root_id)) = self.expr_id_to_expr_map.get(&target_id) {
            let root_id = *root_id;
            state.dbg_stack.note_event(target_id, None, |view| {
                scan.view_origins
                    .get(view)
                    .is_some_and(|roots| roots.contains(&root_id))
            });
        } else if let Some((root_id, written)) = self.place_path(target_id)
            && !written.is_empty()
            && !self.place_is_scalar(target_id)
        {
            state.dbg_stack.note_event(target_id, None, |view| {
                scan.view_anchors.get(view).is_some_and(|anchors| {
                    anchors
                        .iter()
                        .any(|anchor| anchor.overwritten_by(root_id, &written))
                })
            });
        }
    }
}
