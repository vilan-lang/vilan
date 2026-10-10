//! `dbg_stack()` (debugging.md S2, E257): every binding in scope at the call,
//! expanded statically from the scope it sits in.
//!
//! The call resolves like any zero-argument call; what it prints is decided
//! once the checks have judged the program as written, by
//! [`Analyzer::expand_dbg_stacks`], which runs before the last-use dataflow:
//!
//! - **Which bindings:** the parameters and locals visible at the call,
//!   innermost scope first and in declaration order within a scope, each
//!   followed by the same-name bindings it shadows (newest first). Inside a
//!   closure, its own parameters and locals, then the enclosing bindings it
//!   already CAPTURES — listing one it does not would capture it. Module-level
//!   bindings are not the stack and are not listed.
//! - **What it reads:** each listed binding it may read becomes a minted
//!   `Expr::Local` argument of the call, read in place (`Ref`), so liveness,
//!   copy elision and the drop extents count it as a use at the call. It reads
//!   nothing it must not: a resource the move scan holds as moved there
//!   (E281's record), a capture view a rule-4 event invalidated since its last
//!   use (E282's record), a pipe (looking would run it) and a `lazy` parameter
//!   (reading would force it) print without a read.
//! - **The records.** The two checks keep no state per site of their own, so
//!   each files what it holds at a `dbg_stack()` call on the analyzer
//!   ([`Analyzer::record_dbg_stack_moves`], [`DbgStackViewState`]). Both run
//!   inside M19 T1's Class A window, which skips a REUSED module's bodies, so a
//!   module holding a `dbg_stack()` call is never recorded for reuse
//!   ([`Analyzer::mark_dbg_stack_modules_unrecordable`]): its scans, and these
//!   records with them, run on every analysis (B575's rule).
//!
//! The emitters print each site from [`DbgStackSite`]; the line format is
//! `crate::printer`'s, so the two backends write the same bytes.

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::node::Convention;
use crate::span::Span;
use crate::type_::{Type, TypeId};

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

/// One `dbg_stack()` call's expansion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbgStackSite {
    /// What the header line names: `main`, `a closure in main`, or nothing at
    /// module level.
    pub owner: Option<String>,
    pub bindings: Vec<DbgStackBinding>,
}

/// One listed binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbgStackBinding {
    pub name: String,
    /// `L:C` of the declaration that hides this one, for a shadowed binding.
    pub shadowed_at: Option<String>,
    /// The binding's type as the analysis settled it — which may name the
    /// enclosing function's generics; an emitter resolves it under the
    /// instance it is emitting.
    pub type_id: TypeId,
    /// A view (`view T`): a view binding, a capture view or a `&`/`&mut`
    /// parameter.
    pub view: bool,
    /// The roots a view binding views, by name (`(a view into rows)`).
    pub view_into: Vec<String>,
    /// An enclosing binding a closure captures.
    pub captured: bool,
    pub value: DbgStackValue,
}

/// How a listed binding's value prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbgStackValue {
    /// Through the printer, from this minted read — an argument of the call.
    Read(Id),
    /// `<moved at L:C>`.
    Moved(String),
    /// `<moved on some paths>`.
    MovedOnSomePaths,
    /// `<view, invalidated by push at L:C>`: what invalidated it, and where.
    Invalidated { by: String, at: String },
    /// `<pipe, not sampled>`.
    Pipe,
    /// `<lazy, not forced>`.
    Lazy,
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

    /// debugging.md S2: every `dbg_stack()` call's expansion (module docs).
    pub(super) fn expand_dbg_stacks(&mut self) {
        if self.dbg_stack_calls.is_empty() {
            return;
        }
        let calls: Vec<Id> = self.dbg_stack_calls.values().copied().collect();
        let view_origins = self.compute_view_origins();
        let mut views = self.compute_view_bindings();
        views.extend(view_origins.keys().copied());
        let function_scopes: HashMap<Id, Id> = self
            .functions
            .values()
            .map(|function| (function.body.2, function.id))
            .collect();
        let closure_scopes: HashMap<Id, Id> = self
            .closures
            .values()
            .filter_map(|closure| {
                let scope = self.expr_id_to_scope_id_map.get(&closure.return_)?;
                Some((*scope, closure.id))
            })
            .collect();
        let context = ExpansionContext {
            views,
            view_origins,
            function_scopes,
            closure_scopes,
        };
        for call_id in calls {
            if let Some(site) = self.expand_dbg_stack(call_id, &context) {
                self.dbg_stack_sites.insert(call_id, site);
            }
        }
    }

    fn expand_dbg_stack(
        &mut self,
        call_id: Id,
        context: &ExpansionContext,
    ) -> Option<DbgStackSite> {
        let scope_id = *self.expr_id_to_scope_id_map.get(&call_id)?;
        let call_span: &'src Span = self.span_map.get(&call_id)?;
        let offset = call_span.start;
        let source = self.source_of_id(call_id).unwrap_or(SourceId(0));

        // The scope chain: the call's own body (up to its function's or
        // closure's scope), then — inside a closure — the enclosing scopes up
        // to the function, whose bindings are listed only when captured.
        let mut own_scopes: Vec<Id> = Vec::new();
        let mut capture_scopes: Vec<Id> = Vec::new();
        let mut closure_scope: Option<Id> = None;
        let mut owner_function: Option<Id> = None;
        let mut current = Some(scope_id);
        while let Some(id) = current {
            if self.module_scope_ids.contains(&id) {
                break;
            }
            if closure_scope.is_some() {
                capture_scopes.push(id);
            } else {
                own_scopes.push(id);
            }
            if let Some(function_id) = context.function_scopes.get(&id) {
                owner_function = Some(*function_id);
                break;
            }
            if closure_scope.is_none() && context.closure_scopes.contains_key(&id) {
                closure_scope = Some(id);
            }
            current = self.scopes.get(&id).and_then(|scope| scope.parent_id);
        }
        let owner = match (owner_function, closure_scope) {
            (Some(function_id), None) => self
                .functions
                .get(&function_id)
                .map(|function| function.name.to_string()),
            (Some(function_id), Some(_)) => self
                .functions
                .get(&function_id)
                .map(|function| format!("a closure in {}", function.name)),
            (None, Some(_)) => Some("a closure".to_string()),
            (None, None) => None,
        };
        // What the closure already captures: an enclosing binding it reads.
        let captured: HashSet<Id> = match closure_scope {
            Some(closure_scope) if !capture_scopes.is_empty() => self
                .expr_id_to_expr_map
                .iter()
                .filter_map(|(expr_id, expr)| match expr {
                    Expr::Local(binding) => Some((*expr_id, *binding)),
                    _ => None,
                })
                .filter(|(expr_id, _)| {
                    self.expr_id_to_scope_id_map
                        .get(expr_id)
                        .is_some_and(|scope| self.scope_encloses(closure_scope, *scope))
                })
                .map(|(_, binding)| binding)
                .collect(),
            _ => HashSet::default(),
        };

        // Every visible declaration in the chain: (rank, position, name, id).
        let mut declared: Vec<(usize, usize, &'src str, Id)> = Vec::new();
        for (rank, scope) in own_scopes.iter().chain(&capture_scopes).enumerate() {
            let in_capture_scope = rank >= own_scopes.len();
            let Some(scope) = self.scopes.get(scope) else {
                continue;
            };
            for (name, declarations) in &scope.local_value_declarations {
                for declaration in declarations {
                    if !(declaration.visible_from <= offset && offset < declaration.visible_until) {
                        continue;
                    }
                    if in_capture_scope && !captured.contains(&declaration.id) {
                        continue;
                    }
                    if !self.variables.contains_key(&declaration.id)
                        && !self.parameters.contains_key(&declaration.id)
                    {
                        continue;
                    }
                    declared.push((rank, declaration.visible_from, *name, declaration.id));
                }
            }
        }
        // The visible binding of each name is the innermost scope's latest;
        // the rest are the ones it shadows, newest first.
        declared.sort_by(|left, right| {
            (left.0, std::cmp::Reverse(left.1), left.3.0).cmp(&(
                right.0,
                std::cmp::Reverse(right.1),
                right.3.0,
            ))
        });
        let mut by_name: HashMap<&'src str, Vec<(usize, usize, Id)>> = HashMap::default();
        let mut visible: Vec<(usize, usize, &'src str)> = Vec::new();
        for (rank, position, name, id) in &declared {
            let entries = by_name.entry(name).or_default();
            if entries.is_empty() {
                visible.push((*rank, *position, name));
            }
            entries.push((*rank, *position, *id));
        }
        visible.sort_by_key(|(rank, position, _)| (*rank, *position));

        let mut bindings = Vec::new();
        for (_, _, name) in visible {
            let Some(entries) = by_name.get(name) else {
                continue;
            };
            let mut hider: Option<Id> = None;
            for (rank, _, id) in entries {
                let shadowed_at = hider.map(|hider| self.binding_line_column(hider, source));
                let binding = self.dbg_stack_binding(
                    call_id,
                    name,
                    *id,
                    shadowed_at,
                    *rank >= own_scopes.len(),
                    call_span,
                    scope_id,
                    context,
                );
                bindings.push(binding);
                hider = Some(*id);
            }
        }
        Some(DbgStackSite { owner, bindings })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "one binding's facts, gathered at one site"
    )]
    fn dbg_stack_binding(
        &mut self,
        call_id: Id,
        name: &str,
        binding: Id,
        shadowed_at: Option<String>,
        captured: bool,
        call_span: &'src Span,
        scope_id: Id,
        context: &ExpansionContext,
    ) -> DbgStackBinding {
        let (declared_type, lazy, view_parameter) = match self.parameters.get(&binding) {
            Some(parameter) => (
                parameter.type_id,
                parameter.lazy,
                matches!(parameter.convention, Convention::Ref | Convention::RefMut),
            ),
            None => (
                self.variables
                    .get(&binding)
                    .map(|variable| variable.type_id)
                    .unwrap_or_else(|| Type::Unknown.get_type_id(self)),
                false,
                false,
            ),
        };
        let type_ = declared_type.get_type(self);
        let pipe = self.is_std_pipe(&type_);
        let type_id = type_.get_type_id(self);
        let view = view_parameter || context.views.contains(&binding);
        let mut view_into: Vec<String> = context
            .view_origins
            .get(&binding)
            .map(|roots| {
                roots
                    .iter()
                    .filter_map(|root| {
                        self.variables
                            .get(root)
                            .map(|variable| variable.name)
                            .or_else(|| self.parameters.get(root).map(|parameter| parameter.name))
                    })
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        view_into.sort();
        view_into.dedup();
        let source = self.source_of_id(call_id).unwrap_or(SourceId(0));
        let moved = self
            .dbg_stack_moves
            .get(&call_id)
            .and_then(|moved| moved.iter().find(|(id, _)| *id == binding))
            .map(|(_, state)| *state);
        let invalidated = self
            .dbg_stack_invalidated
            .get(&call_id)
            .and_then(|views| views.iter().find(|entry| entry.view == binding))
            .copied();
        let value = if let Some(state) = moved {
            match state {
                DbgStackMove::Moved(span) => {
                    DbgStackValue::Moved(self.line_column(source, span.start))
                }
                DbgStackMove::MovedOnSomePaths(_) => DbgStackValue::MovedOnSomePaths,
            }
        } else if let Some(invalidation) = invalidated {
            let by = match invalidation.callee {
                Some(callee) => self
                    .functions
                    .get(&callee)
                    .map(|function| function.name)
                    .or_else(|| {
                        self.external_functions
                            .get(&callee)
                            .map(|external| external.name)
                    })
                    .unwrap_or("a call")
                    .to_string(),
                None => "assignment".to_string(),
            };
            let at = self
                .span_map
                .get(&invalidation.event)
                .map(|span| self.line_column(source, span.start))
                .unwrap_or_default();
            DbgStackValue::Invalidated { by, at }
        } else if lazy {
            DbgStackValue::Lazy
        } else if pipe {
            DbgStackValue::Pipe
        } else {
            DbgStackValue::Read(self.mint_dbg_stack_read(call_id, binding, call_span, scope_id))
        };
        DbgStackBinding {
            name: name.to_string(),
            shadowed_at,
            type_id,
            view,
            view_into,
            captured,
            value,
        }
    }

    /// A read of `binding` at the call: an `Expr::Local` the call takes as its
    /// next argument, spanned and scoped at the call.
    fn mint_dbg_stack_read(
        &mut self,
        call_id: Id,
        binding: Id,
        call_span: &'src Span,
        scope_id: Id,
    ) -> Id {
        let read = self.new_entity_id();
        self.expr_id_to_expr_map.insert(read, Expr::Local(binding));
        self.span_map.insert(read, call_span);
        self.expr_id_to_scope_id_map.insert(read, scope_id);
        *self.reference_count.entry(binding).or_insert(0) += 1;
        if let Some(call) = self.function_calls.get_mut(&call_id) {
            call.argument_ids.push(read);
        }
        read
    }

    /// Whether `type_` is a struct std's `Pipe`/`CollPipe` is implemented for
    /// (`crate::printer`'s rule, asked of the analyzer): a pipe prints by its
    /// type, since sampling it would run its bodies.
    fn is_std_pipe(&self, type_: &Type) -> bool {
        let Type::Struct(struct_id, _) = type_ else {
            return false;
        };
        let is_std = |id: Id| {
            self.source_of_id(id)
                .is_some_and(|source| self.std_sources.contains(&source))
        };
        let pipe_traits: Vec<Id> = self
            .traits
            .values()
            .filter(|declared| matches!(declared.name, "Pipe" | "CollPipe") && is_std(declared.id))
            .map(|declared| declared.id)
            .collect();
        !pipe_traits.is_empty()
            && self.implementations.iter().any(|implementation| {
                implementation
                    .trait_ids
                    .iter()
                    .any(|trait_id| pipe_traits.contains(trait_id))
                    && matches!(
                        self.type_id_to_type_map.get(&implementation.subject),
                        Some(Type::Struct(subject, _)) if subject == struct_id
                    )
            })
    }

    /// `L:C` of a binding's name.
    fn binding_line_column(&self, binding: Id, source: SourceId) -> String {
        let offset = self
            .variables
            .get(&binding)
            .map(|variable| variable.name_span.start)
            .or_else(|| self.span_map.get(&binding).map(|span| span.start))
            .unwrap_or(0);
        self.line_column(source, offset)
    }

    /// `L:C` of a byte offset in `source`: a 1-based line and a 1-based column
    /// in characters, as [`super::Program::site_location`] counts them.
    fn line_column(&self, source: SourceId, offset: usize) -> String {
        let Some(text) = self.source_text(source) else {
            return "1:1".to_string();
        };
        let offset = offset.min(text.len());
        let prefix = text.get(..offset).unwrap_or("");
        let line = prefix.matches('\n').count() + 1;
        let line_start = prefix.rfind('\n').map_or(0, |at| at + 1);
        let column = prefix
            .get(line_start..)
            .map_or(0, |line| line.chars().count())
            + 1;
        format!("{line}:{column}")
    }
}

/// What every site's expansion reads, computed once per analysis.
struct ExpansionContext {
    views: HashSet<Id>,
    view_origins: HashMap<Id, Vec<Id>>,
    /// A function's body scope → the function.
    function_scopes: HashMap<Id, Id>,
    /// A closure's body scope → the closure.
    closure_scopes: HashMap<Id, Id>,
}

/// debugging.md §3.3 (Q4), for `dbg_stack()` as for `dbg`: every call in a
/// build whose policy is [`crate::options::DbgPolicy::Refuse`] — the release
/// preset's default — is an error at the call.
pub fn refuse_release_dbg_stack(
    program: &mut super::Program,
    options: &crate::options::BuildOptions,
) {
    if options.dbg != crate::options::DbgPolicy::Refuse || program.dbg_stack_sites.is_empty() {
        return;
    }
    let mut calls: Vec<Id> = program.dbg_stack_sites.keys().copied().collect();
    calls.sort_unstable_by_key(|call| call.0);
    for call_id in calls {
        let span = program
            .span_map
            .get(&call_id)
            .map(|span| **span)
            .unwrap_or(Span { start: 0, end: 0 });
        let source = program.source_of(call_id).unwrap_or(SourceId(0));
        program.push_diagnostic(
            crate::error::Error {
                trace: Vec::new(),
                note: None,
                span,
                msg: "`dbg_stack()` left in a release build: a release build refuses it, so a \
                      debugging line cannot ship by accident. Remove the call, or set `[build] \
                      dbg = \"strip\"` (print nothing) or `\"keep\"` (print in release too) in \
                      `vilan.toml`"
                    .to_string(),
            },
            source,
        );
    }
    program.normalize_diagnostic_order();
}
