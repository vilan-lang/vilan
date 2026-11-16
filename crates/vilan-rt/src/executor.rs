//! `vilan-rt::executor` — the single-threaded executor emitted `async` code
//! runs on (tracker J6; `proposal/native-apps.md` §10 "§8 Q4 — the executor,
//! designed" is the design, and `transformer.rs::helper_source` is the
//! CONTRACT).
//!
//! # What this has to be identical to
//!
//! Nothing here is a new concurrency design. The JS backend's `__task`,
//! `__nursery_new`, `__nursery_new_detached`, `__nursery_run`, `__sleep`,
//! `__timer` and `__with_finally_async` helpers already define what an `async`
//! vilan program MEANS, and `native_differential` compares the two backends'
//! stdout byte for byte — so every rule below is read off one of those helpers,
//! and the comment at each rule names the one it came from. Where the two
//! genuinely cannot agree the divergence is written down at the site rather
//! than papered over.
//!
//! # The shape, in one paragraph
//!
//! One thread, no work stealing. A slab of tasks; a **microtask queue** of task
//! ids to re-poll, drained to EXHAUSTION; then, and only then, the single
//! earliest entry of a deadline-ordered timer list — one macrotask, after which
//! the microtask queue drains again. Exit when both are empty. Every leaf
//! future (a `sleep`, a `Timer::wait`, a task join, a nursery's drain) parks by
//! writing the CURRENT task's id into the wake list of the thing it waits on,
//! and that thing enqueues the id when it settles — which is why the executor
//! needs no `RawWaker` and this crate can keep `#![forbid(unsafe_code)]`
//! (`Waker::noop()` is handed to every poll; nothing ever consults it).
//!
//! # `Pin` and `Send` never surface
//!
//! §10's rule for the emitter: emitted Rust says `async fn`, `.await`,
//! `spawn(..)`, `sleep(..)` and nothing else. Every `Pin<Box<..>>` below is
//! behind one of those spellings. Nothing here is `Send`, deliberately: a `Task`
//! is an `Rc` handle and the runtime is a thread-local.

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::future::{Future, IntoFuture};
use std::net::{Ipv4Addr, UdpSocket};
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use crate::{Str, describe_panic};

/// A handle into the runtime's slab: a slot index plus the GENERATION that slot
/// carried when the handle was minted (tracker F24).
///
/// Slots are reused — a settled task's slot goes back on the free list — so an
/// index alone is not an identity: a wake list still holding the id of a task
/// that has since been reaped would wake whatever took its slot. The generation
/// is what keeps the guarantee that the index alone used to give by never being
/// reused: [`reap_settled`] bumps it when it frees the slot, so every handle
/// minted before that point compares unequal for the rest of the program and
/// [`task_at`] answers `None`.
///
/// Before F24 this was a bare `usize` and the slab grew one slot per spawn for
/// the program's lifetime. That is fine for a CLI program and unbounded for a
/// server spawning per request, which is what F18 makes this crate do.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct TaskId {
    index: usize,
    generation: u64,
}

/// One slot of the task slab: the live task, or nothing, and the generation
/// that says which handles still name it.
struct TaskSlot {
    node: Option<Rc<TaskNode>>,
    generation: u64,
}

/// The type-erased driver of one task.
type Driver = Pin<Box<dyn Future<Output = ()>>>;

/// A future the runtime can HOLD — what the nursery join and
/// [`with_finally_async`] take where the JS helpers take a closure they invoke.
///
/// Named, and paired with [`pin_future`], for §10's rule: emitted Rust says
/// `vilan_rt::executor::pin_future(async move { .. })` and never `Pin` or
/// `Box::pin`.
pub type Boxed<T> = Pin<Box<dyn Future<Output = T>>>;

/// Boxes and pins `body`, so the emitter never has to spell either.
pub fn pin_future<T>(body: impl Future<Output = T> + 'static) -> Boxed<T> {
    Box::pin(body)
}

// -------------------------------------------------------------- failure ---

/// Why a task did not produce a value.
///
/// The JS side distinguishes these by `error.name === "AbortError"`
/// (`__nursery_is_cancel`), and the distinction is load-bearing three times: an
/// owned task that fails by CANCELLATION does not latch its nursery's failure,
/// does not take the unobserved-report path, and can never win the join.
/// Natively the classification is the payload's own TYPE, so it cannot be
/// spoofed by a vilan `panic("AbortError")`.
#[derive(Clone, Debug, PartialEq)]
pub enum Failure {
    /// A cancel signal fired under a suspended operation — the abort a
    /// nursery's `cancel` (or its first error) delivers.
    Cancelled,
    /// A vilan `panic`, or any other Rust panic raised inside the task: the
    /// message, and the vilan site a `panic` names (debugging.md S0).
    Panic(Str, Option<crate::Location>),
}

/// The payload a cancellation panics with. A distinct type rather than a magic
/// string, so [`classify`] cannot mistake a vilan `panic("…")` for one.
#[derive(Debug)]
struct Cancellation;

/// Raises `failure` out of the current poll — the native spelling of a rejected
/// promise propagating out of an `await`.
fn raise(failure: Failure) -> ! {
    match failure {
        Failure::Cancelled => std::panic::panic_any(Cancellation),
        Failure::Panic(message, Some(location)) => crate::panic_at(&message, location),
        Failure::Panic(message, None) => crate::panic_with(&message),
    }
}

fn classify(payload: &Box<dyn std::any::Any + Send>) -> Failure {
    if payload.downcast_ref::<Cancellation>().is_some() {
        Failure::Cancelled
    } else {
        Failure::Panic(describe_panic(payload), crate::panic_location(payload))
    }
}

// ---------------------------------------------------------------- tasks ---

/// The record behind a `Task<T>` — everything about a task that does not
/// mention `T`, so a nursery's child list and the runtime's slab can hold one
/// without knowing what the task computes.
///
/// `__task`'s own fields, one for one: `origin`, `observed`, `nursery`,
/// `owned`, the `rejected`/`error` pair that is the settled/failure pair here,
/// and the continuation list the thenable protocol gives JS for free.
struct TaskNode {
    origin: Str,
    driver: RefCell<Option<Driver>>,
    settled: Cell<bool>,
    failure: RefCell<Option<Failure>>,
    observed: Cell<bool>,
    owned: bool,
    nursery: Option<Nursery>,
    /// Task ids to enqueue when this task settles.
    waiters: RefCell<Vec<TaskId>>,
    /// Whether the unobserved-failure report has already been decided — the
    /// report happens at most once, as `__task`'s `setTimeout` guard ensures.
    reported: Cell<bool>,
}

impl TaskNode {
    fn settle(self: &Rc<Self>, failure: Option<Failure>) {
        if self.settled.get() {
            return;
        }
        self.settled.set(true);
        // The driver is dropped at settle, so a task's captured state (and any
        // `Task` handles it held) is released rather than living as long as
        // somebody's handle to the finished task does.
        *self.driver.borrow_mut() = None;
        if let Some(failure) = failure {
            *self.failure.borrow_mut() = Some(failure.clone());
            // `__task`'s rejection handler: an OWNED task that failed with a
            // real error notifies its nursery at settle time. A cancellation is
            // an echo of a teardown somebody already knows about.
            if self.owned
                && failure != Failure::Cancelled
                && let Some(nursery) = &self.nursery
            {
                nursery.fail(self);
            }
        }
        for waiter in self.waiters.borrow_mut().drain(..) {
            enqueue(waiter);
        }
    }

    /// The `unhandled task error (spawned in …)` line, at most once. It goes to
    /// stderr because `console.error` does, which is also why it cannot move
    /// the differential (stdout).
    fn report_unobserved(&self) {
        if self.reported.replace(true) {
            return;
        }
        // `String(error)` on the JS leg, which for a vilan panic is its
        // `panicked at <location>: <message>` line.
        match self.failure.borrow().as_ref() {
            Some(Failure::Panic(message, Some(location))) => eprintln!(
                "unhandled task error (spawned in {}): panicked at {}: {message}",
                self.origin, location.0
            ),
            Some(Failure::Panic(message, None)) => eprintln!(
                "unhandled task error (spawned in {}): {message}",
                self.origin
            ),
            _ => {}
        }
    }
}

/// `Task<T>` — the handle an `async` expression yields.
///
/// A slab handle, and a COPY of one refers to the same task: `__clone` passes a
/// JS class instance through untouched, so handle semantics are what the
/// language already promises. The value lives in its own cell rather than in
/// the node, because the node is the type-erased half.
pub struct Task<T> {
    node: Rc<TaskNode>,
    value: Rc<RefCell<Option<T>>>,
}

impl<T> Clone for Task<T> {
    fn clone(&self) -> Self {
        Task {
            node: Rc::clone(&self.node),
            value: Rc::clone(&self.value),
        }
    }
}

impl<T> Task<T> {
    /// Whether the task has settled, without observing it.
    pub fn is_settled(&self) -> bool {
        self.node.settled.get()
    }
}

/// Two handles are equal when they are handles to the SAME task — which is what
/// `===` on the JS backend's class instance answers, and the only equality a
/// handle has (a task is not a value).
impl<T> PartialEq for Task<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.node, &other.node)
    }
}

/// The future `task.await` becomes. Holds only `Rc` handles, so it is `Unpin`
/// and needs no projection.
pub struct TaskJoin<T> {
    task: Task<T>,
}

impl<T: Clone> Future for TaskJoin<T> {
    type Output = T;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<T> {
        let task = &self.as_mut().get_mut().task;
        // `then` sets `observed` on the way IN, whatever the outcome — which is
        // what keeps an awaited failure off the unobserved-report path.
        task.node.observed.set(true);
        if !task.node.settled.get() {
            park_on(&task.node.waiters);
            return Poll::Pending;
        }
        if let Some(failure) = task.node.failure.borrow().clone() {
            raise(failure);
        }
        // Read rather than TAKE: `await task` twice yields the value twice, as
        // awaiting a settled promise twice does.
        let value = task
            .value
            .borrow()
            .clone()
            .expect("a settled task that did not fail has its value");
        Poll::Ready(value)
    }
}

impl<T: Clone> IntoFuture for Task<T> {
    type Output = T;
    type IntoFuture = TaskJoin<T>;

    fn into_future(self) -> TaskJoin<T> {
        TaskJoin { task: self }
    }
}

/// A join over the TYPE-ERASED node — what a nursery's drain awaits, since it
/// discards its children's values.
struct NodeJoin {
    node: Rc<TaskNode>,
    /// The nursery whose fail latch also ends this wait. `__nursery_run` races
    /// every child against the fail-wake, so a fast failure behind a slow
    /// healthy sibling is seen immediately.
    nursery: Nursery,
}

impl Future for NodeJoin {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<()> {
        let this = self.as_mut().get_mut();
        // The drain ABSORBS its children: observed, results discarded, so a
        // child's failure is never also reported as unobserved.
        this.node.observed.set(true);
        if this.node.settled.get() || this.nursery.failed().is_some() {
            return Poll::Ready(());
        }
        park_on(&this.nursery.0.fail_waiters);
        park_on(&this.node.waiters);
        Poll::Pending
    }
}

/// Catches a panic raised anywhere inside `inner`, including across an `await`.
///
/// std has no `FutureExt::catch_unwind` and this crate has no futures crate, so
/// the fence is written where it belongs anyway — at the poll boundary. Holding
/// the inner future as `Pin<Box<..>>` is what makes the projection safe: a
/// `Pin<Box<F>>` is itself `Unpin`, so this future reaches its own field with no
/// `unsafe`.
struct Catch<T> {
    inner: Boxed<T>,
    done: bool,
}

impl<T> Catch<T> {
    fn new(inner: Boxed<T>) -> Self {
        Catch { inner, done: false }
    }
}

impl<T> Future for Catch<T> {
    type Output = Result<T, Failure>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Result<T, Failure>> {
        let this = self.as_mut().get_mut();
        if this.done {
            // Polling a completed future is the caller's defect; answering
            // `Pending` is the inert response, and nothing in this file does it.
            return Poll::Pending;
        }
        let inner = &mut this.inner;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inner.as_mut().poll(context)
        }));
        match outcome {
            Ok(Poll::Pending) => Poll::Pending,
            Ok(Poll::Ready(value)) => {
                this.done = true;
                Poll::Ready(Ok(value))
            }
            Err(payload) => {
                this.done = true;
                Poll::Ready(Err(classify(&payload)))
            }
        }
    }
}

/// `body` to its end, answering its FAILURE instead of raising it — the seam
/// `vilan_rt::http::respond` answers a request at before [`raise_failure`] puts
/// the failure back on its way (M179).
pub(crate) async fn caught<T>(body: Boxed<T>) -> Result<T, Failure> {
    Catch::new(body).await
}

/// Raises `failure` again, location and all — a caught failure continuing as if
/// it had never been caught.
pub(crate) fn raise_failure(failure: Failure) -> ! {
    raise(failure)
}

/// `__guarded_async` — run `body` to its end, answering `Err(message)` if it
/// panicked: `guarded`'s async twin, the shape `std::rpc`'s `guarded_async`
/// names (M165: an `[rpc]` body that throws answers its call with the error).
/// The JS helper is a `try`/`await`/`catch` returning `[0, value]` or
/// `[1, message]`; natively [`Catch`] catches at every poll, as a task's own
/// failure latch does.
///
/// **A cancellation is caught too**, because the JS `catch` catches it: an
/// aborted `__sleep` rejects with the signal's reason — the `AbortError` whose
/// message is `This operation was aborted` — and the helper answers it as
/// `[1, message]` like any rejection. `std::rpc`'s dial reads exactly that:
/// its backoff wait is `guarded_async(|| backoff_wait(..))`, and an `Err` there
/// is how a cancel during the wait ends the dial with `Cancelled` (A179).
/// Re-raising it here instead (as this did until F1 S1b gave the dial a
/// native client) carried the abort out through the dial's task and failed
/// the nursery that was only asking the dial to stop.
pub async fn guarded_async<T>(body: Boxed<T>) -> Result<T, Str> {
    match Catch::new(body).await {
        Ok(value) => Ok(value),
        Err(Failure::Cancelled) => Err(crate::str_new("This operation was aborted")),
        Err(Failure::Panic(message, _location)) => Err(message),
    }
}

// ------------------------------------------------------------ nurseries ---

/// A nursery's body — `__Nursery`'s fields, plus the two the JS version gets
/// from `AbortController` for free.
///
/// §10 (3): `Rc<NurseryBody>` with `children`, `cancelled`, a fail latch, a
/// wake list and `parent: Option<Weak<..>>`. The parent link is a `Weak`
/// because a child nursery outliving its parent must not keep it alive — and
/// because `CancelSignal` IS that `Weak`: a signal handed to a host operation
/// cannot be what keeps the nursery from being dropped.
struct NurseryBody {
    children: RefCell<Vec<Rc<TaskNode>>>,
    cancelled: Cell<bool>,
    /// The first owned task to fail with a real error — the join's winner.
    failed: RefCell<Option<Rc<TaskNode>>>,
    /// Tasks to wake when the fail latch is set (the join's `failWake`).
    fail_waiters: RefCell<Vec<TaskId>>,
    /// Tasks to wake when this nursery — or an ancestor — cancels: the native
    /// `signal.addEventListener("abort", ..)`.
    cancel_waiters: RefCell<Vec<TaskId>>,
    /// Nurseries created under this one. `__nursery_new` chains a child's
    /// controller to its parent's signal, and the chain has to be walked
    /// DOWNWARD at cancel time so a descendant's parked `sleep` is woken —
    /// which is exactly what the JS listener does.
    descendants: RefCell<Vec<Weak<NurseryBody>>>,
    /// Callbacks to RUN when this nursery cancels (F1 S1b) — the native
    /// `signal.addEventListener("abort", listener)` that `std::rpc`'s dial
    /// registers through `__dial_on_abort`, beside the park-only
    /// `cancel_waiters` a leaf future uses. Keyed by a registration number so
    /// an [`AbortHook`] can take its own back out.
    abort_hooks: RefCell<Vec<(u64, Rc<dyn Fn()>)>>,
    parent: Option<Weak<NurseryBody>>,
    /// A DETACHED nursery is never joined, so it must not silently absorb a
    /// child's failure: this flag is `__nursery_new_detached`'s `__fail`
    /// override, and it reopens the free-task reporting path.
    detached: bool,
}

/// `Nursery` — the handle `std::task` binds `cancel` / `is_cancelled` /
/// `signal_of` against.
#[derive(Clone)]
pub struct Nursery(Rc<NurseryBody>);

/// `CancelSignal` — the `Weak` link to a nursery, handed to abortable
/// operations.
#[derive(Clone)]
pub struct CancelSignal(Weak<NurseryBody>);

impl CancelSignal {
    /// Whether the nursery behind this signal has cancelled. A signal whose
    /// nursery is GONE is not aborted: the JS signal would simply never fire
    /// again.
    pub fn aborted(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|body| Nursery(body).is_cancelled())
    }

    /// Parks the current task on this signal's nursery, so a `cancel` wakes it
    /// to observe the abort. `pub(crate)` for the leaf futures of sibling
    /// modules ([`crate::process`]'s stdin read).
    pub(crate) fn park(&self) {
        if let Some(body) = self.0.upgrade() {
            park_on(&body.cancel_waiters);
        }
    }
}

impl Nursery {
    fn body(parent: Option<Weak<NurseryBody>>, detached: bool) -> Nursery {
        Nursery(Rc::new(NurseryBody {
            children: RefCell::new(Vec::new()),
            cancelled: Cell::new(false),
            failed: RefCell::new(None),
            fail_waiters: RefCell::new(Vec::new()),
            cancel_waiters: RefCell::new(Vec::new()),
            descendants: RefCell::new(Vec::new()),
            abort_hooks: RefCell::new(Vec::new()),
            parent,
            detached,
        }))
    }

    /// `cancel()` — abort this nursery's signal and every descendant's, waking
    /// whatever is parked on them. Idempotent, as a second
    /// `AbortController::abort` is.
    pub fn cancel(&self) {
        if self.0.cancelled.replace(true) {
            return;
        }
        for waiter in self.0.cancel_waiters.borrow_mut().drain(..) {
            enqueue(waiter);
        }
        // The abort listeners run HERE, synchronously and in registration
        // order, as `AbortController::abort` dispatches its `abort` event.
        // Taken out of the cell first: a hook may register or detach another.
        let hooks: Vec<Rc<dyn Fn()>> = self
            .0
            .abort_hooks
            .borrow_mut()
            .drain(..)
            .map(|(_, hook)| hook)
            .collect();
        for hook in hooks {
            hook();
        }
        let descendants: Vec<Weak<NurseryBody>> = self.0.descendants.borrow().clone();
        for descendant in descendants {
            if let Some(body) = descendant.upgrade() {
                Nursery(body).cancel();
            }
        }
    }

    /// Whether this nursery has been cancelled — its own `cancel`, a parent's,
    /// or the first error's abort. The parent is consulted on the way UP as
    /// well, because a nursery created under an already-cancelled parent is
    /// cancelled at birth and `cancel` cannot have reached it.
    pub fn is_cancelled(&self) -> bool {
        if self.0.cancelled.get() {
            return true;
        }
        self.0
            .parent
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|parent| Nursery(parent).is_cancelled())
    }

    /// `signal_of()`.
    pub fn signal(&self) -> CancelSignal {
        CancelSignal(Rc::downgrade(&self.0))
    }

    /// `__fail` — latch the EARLIEST failing owned task, abort the signal, wake
    /// the drain. On a detached nursery it is the override instead: report the
    /// failure with its spawn origin and leave the siblings running, because
    /// ownership is lifetime, not fate-sharing.
    fn fail(&self, task: &Rc<TaskNode>) {
        if self.0.detached {
            if !task.observed.get() {
                task.report_unobserved();
            }
            return;
        }
        if self.0.failed.borrow().is_some() {
            return;
        }
        *self.0.failed.borrow_mut() = Some(Rc::clone(task));
        self.cancel();
        for waiter in self.0.fail_waiters.borrow_mut().drain(..) {
            enqueue(waiter);
        }
    }

    fn failed(&self) -> Option<Rc<TaskNode>> {
        self.0.failed.borrow().clone()
    }
}

/// `__nursery_new(parent)`.
pub fn nursery_new(parent: Option<Nursery>) -> Nursery {
    let nursery = Nursery::body(
        parent.as_ref().map(|parent| Rc::downgrade(&parent.0)),
        false,
    );
    if let Some(parent) = parent {
        // `if (signal.aborted) abort(reason); else addEventListener("abort", ..)`.
        if parent.is_cancelled() {
            nursery.cancel();
        } else {
            parent
                .0
                .descendants
                .borrow_mut()
                .push(Rc::downgrade(&nursery.0));
        }
    }
    nursery
}

/// `__nursery_new_detached()` — `OwnedNursery`'s never-joined nursery.
pub fn nursery_new_detached() -> Nursery {
    Nursery::body(None, true)
}

/// `__nursery_has_spawned(n)` — whether any task ever registered with the
/// nursery, a settled one included (M92: a pipe stage carries a nursery no task
/// registered with into its next run).
pub fn nursery_has_spawned(nursery: &Nursery) -> bool {
    !nursery.0.children.borrow().is_empty()
}

/// `__nursery_run(n, body)` — the join, reproduced exactly.
///
/// Run the body, then drain the children; the child list may GROW while
/// draining (a running child spawns a grandchild), so the loop re-reads it every
/// turn rather than snapshotting its length. Failure reaction is at settle time:
/// a failing owned task has already latched itself through [`Nursery::fail`] and
/// aborted the signal, and the drain races every child against the fail-wake. On
/// failure every remaining child is absorbed, the body's error wins if there is
/// one, and a latched winner carries its spawn origin into the message.
pub async fn nursery_run<T>(nursery: Nursery, body: Boxed<T>) -> T {
    let (result, body_failure) = match Catch::new(body).await {
        Ok(value) => (Some(value), None),
        Err(failure) => (None, Some(failure)),
    };
    if body_failure.is_some() {
        nursery.cancel();
    }
    let mut index = 0;
    while body_failure.is_none() && nursery.failed().is_none() {
        let child = nursery.0.children.borrow().get(index).cloned();
        let Some(child) = child else { break };
        NodeJoin {
            node: child,
            nursery: nursery.clone(),
        }
        .await;
        if nursery.failed().is_none() {
            index += 1;
        }
    }
    if body_failure.is_none() && nursery.failed().is_none() {
        return result.expect("a body that did not fail produced its value");
    }
    let children: Vec<Rc<TaskNode>> = nursery.0.children.borrow().clone();
    for child in children {
        child.observed.set(true);
    }
    if let Some(failure) = body_failure {
        raise(failure);
    }
    let winner = nursery.failed().expect("the latch was just read as set");
    let failure = winner.failure.borrow().clone();
    match failure {
        Some(Failure::Panic(message, location)) => {
            let message = format!("{message} (in task spawned in {})", winner.origin);
            match location {
                Some(location) => crate::panic_at(&message, location),
                None => crate::panic_with(&message),
            }
        }
        // `__fail` is only ever called for a non-cancellation error, so a
        // cancelled winner cannot arise; raising the cancellation is the honest
        // answer if it ever did.
        Some(Failure::Cancelled) | None => std::panic::panic_any(Cancellation),
    }
}

// --------------------------------------------------------- host one-shots ---

/// The registration counter behind [`AbortHook`]'s identity.
static NEXT_ABORT_HOOK: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// `__dial_on_abort(signal, handler)` (F1 S1b) — run `handler` once when
/// `signal` aborts, or at once when it already has; the hook it answers
/// removes it again.
///
/// The JS helper is `addEventListener("abort", listener, { once: true })`
/// behind an `aborted` check, and this is the same two arms: an aborted signal
/// runs the handler inside this call and answers a hook that detaches nothing,
/// and a live one registers it with the nursery, whose [`Nursery::cancel`]
/// runs it. A signal whose nursery is GONE never aborts (see
/// [`CancelSignal::aborted`]), so its handler never runs.
pub fn on_abort(signal: &CancelSignal, handler: Rc<dyn Fn()>) -> AbortHook {
    if signal.aborted() {
        handler();
        return AbortHook {
            signal: Weak::new(),
            id: 0,
        };
    }
    let id = NEXT_ABORT_HOOK.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Some(body) = signal.0.upgrade() {
        body.abort_hooks.borrow_mut().push((id, handler));
    }
    AbortHook {
        signal: Weak::clone(&signal.0),
        id,
    }
}

/// `AbortHook` — one abort listener, as a value its registrant can take back
/// (`std::rpc`'s `external struct AbortHook`).
#[derive(Clone)]
pub struct AbortHook {
    signal: Weak<NurseryBody>,
    id: u64,
}

impl AbortHook {
    /// `hook.detach()` — `removeEventListener`: an abort after this reaches
    /// nothing of the registrant's. Idempotent, and inert after the hook ran.
    pub fn detach(&self) {
        if let Some(body) = self.signal.upgrade() {
            body.abort_hooks
                .borrow_mut()
                .retain(|(registered, _)| *registered != self.id);
        }
    }
}

impl PartialEq for AbortHook {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.signal.as_ptr(), other.signal.as_ptr()) && self.id == other.id
    }
}

impl crate::Js for AbortHook {
    fn js(&self) -> String {
        crate::panic_with(
            "printing an `AbortHook` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

/// `new Promise(executor)` (F1 S1b) — a task settled by the RESOLVER its
/// executor is handed rather than by a body of its own.
///
/// `std::rpc` binds the host `Promise` constructor twice — `dial_promise` and
/// `reply_promise` — as a first-call-wins one-shot that a socket event
/// completes. The executor runs inside this call, as the constructor runs it;
/// the first call of the resolver settles the task with its value and wakes
/// whatever awaits it, and every later call is inert, as resolving a settled
/// promise is. The node has no driver and is never in the slab: nothing polls
/// it, the resolver settles it, and a join parks on it like any task's.
pub fn promise<T: 'static>(executor: Rc<dyn Fn(Rc<dyn Fn(T)>)>) -> Task<T> {
    let node = Rc::new(TaskNode {
        origin: crate::str_new("Promise"),
        driver: RefCell::new(None),
        settled: Cell::new(false),
        failure: RefCell::new(None),
        observed: Cell::new(false),
        owned: false,
        nursery: None,
        waiters: RefCell::new(Vec::new()),
        reported: Cell::new(false),
    });
    let value: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let resolve: Rc<dyn Fn(T)> = {
        let node = Rc::clone(&node);
        let value = Rc::clone(&value);
        Rc::new(move |resolved: T| {
            if node.settled.get() {
                return;
            }
            *value.borrow_mut() = Some(resolved);
            node.settle(None);
        })
    };
    executor(resolve);
    Task { node, value }
}

// ----------------------------------------------------------------- time ---

/// One registration in the deadline list.
struct TimerSlot {
    deadline: Instant,
    /// Registration order, so two timers that come due together fire in the
    /// order they were registered — `setTimeout`'s own rule.
    sequence: u64,
    fired: Cell<bool>,
    cancelled: Cell<bool>,
    /// What firing does. A `sleep` wakes the task that registered it; a `Timer`
    /// settles its verdict and wakes every waiter.
    action: RefCell<Option<Rc<dyn Fn()>>>,
}

/// `__sleep(ms, signal)` — resolve after `ms`, or reject with the abort when the
/// ambient cancel signal fires first (clearing the timer).
pub fn sleep(milliseconds: i32, signal: Option<CancelSignal>) -> Sleep {
    Sleep {
        milliseconds,
        signal,
        slot: None,
    }
}

/// The future [`sleep`] returns. Public because emitted code's TYPE inference
/// names it, never its text.
pub struct Sleep {
    milliseconds: i32,
    signal: Option<CancelSignal>,
    slot: Option<Rc<TimerSlot>>,
}

impl Future for Sleep {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<()> {
        let this = self.as_mut().get_mut();
        // The abort is checked BEFORE the timer is registered, exactly as the
        // promise constructor's first line does: an already-aborted signal never
        // starts a timer, and an abort arriving later clears the one it started.
        if this.signal.as_ref().is_some_and(CancelSignal::aborted) {
            if let Some(slot) = &this.slot {
                slot.cancelled.set(true);
            }
            raise(Failure::Cancelled);
        }
        if let Some(slot) = &this.slot {
            if slot.fired.get() {
                return Poll::Ready(());
            }
        } else {
            // The wake is captured HERE, at registration: the timer's action
            // enqueues the task that first polled this sleep. Nothing in
            // emitted code moves a half-polled future between tasks.
            let task = current_task();
            this.slot = Some(register_timer(this.milliseconds, move || {
                if let Some(task) = task {
                    enqueue(task);
                }
            }));
        }
        if let Some(signal) = &this.signal {
            signal.park();
        }
        Poll::Pending
    }
}

/// `__timer`'s body: the memoized verdict plus the waiter list.
struct TimerBody {
    settled: Cell<bool>,
    verdict: Cell<bool>,
    waiters: RefCell<Vec<TaskId>>,
    slot: RefCell<Option<Rc<TimerSlot>>>,
}

impl TimerBody {
    /// The first settlement wins forever, and every waiter — the ones already
    /// parked and the ones that arrive afterwards — observes that same verdict.
    fn settle(&self, verdict: bool) {
        if self.settled.replace(true) {
            return;
        }
        self.verdict.set(verdict);
        for waiter in self.waiters.borrow_mut().drain(..) {
            enqueue(waiter);
        }
    }
}

/// `TimerHandle` — `setTimeout` and `clearTimeout` as one value.
#[derive(Clone)]
pub struct TimerHandle(Rc<TimerBody>);

/// `__timer(ms)`.
pub fn timer(milliseconds: i32) -> TimerHandle {
    let body = Rc::new(TimerBody {
        settled: Cell::new(false),
        verdict: Cell::new(false),
        waiters: RefCell::new(Vec::new()),
        slot: RefCell::new(None),
    });
    let fired = Rc::clone(&body);
    let slot = register_timer(milliseconds, move || fired.settle(true));
    *body.slot.borrow_mut() = Some(slot);
    TimerHandle(body)
}

impl TimerHandle {
    /// `cancel()` — `clearTimeout` plus a `false` verdict, if nothing settled it
    /// first.
    pub fn cancel(&self) {
        if self.0.settled.get() {
            return;
        }
        if let Some(slot) = self.0.slot.borrow().as_ref() {
            slot.cancelled.set(true);
        }
        self.0.settle(false);
    }

    /// `wait(signal)` — the verdict, or the abort of THIS waiter.
    ///
    /// The one difference from [`sleep`] is the point of the type: an abort
    /// rejects the waiter and leaves the verdict unsettled and the host timer
    /// running, because the timer belongs to whoever holds the value rather than
    /// to the nursery that happened to await it. A settled timer answers from
    /// the memo without consulting the signal at all — there is nothing left to
    /// tear down.
    pub fn wait(&self, signal: Option<CancelSignal>) -> TimerWait {
        TimerWait {
            body: Rc::clone(&self.0),
            signal,
            parked: false,
        }
    }
}

/// The future [`TimerHandle::wait`] returns.
pub struct TimerWait {
    body: Rc<TimerBody>,
    signal: Option<CancelSignal>,
    parked: bool,
}

impl Future for TimerWait {
    type Output = bool;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<bool> {
        let this = self.as_mut().get_mut();
        if this.body.settled.get() {
            return Poll::Ready(this.body.verdict.get());
        }
        if this.signal.as_ref().is_some_and(CancelSignal::aborted) {
            if this.parked {
                unpark_from(&this.body.waiters);
            }
            raise(Failure::Cancelled);
        }
        park_on(&this.body.waiters);
        if let Some(signal) = &this.signal {
            signal.park();
        }
        this.parked = true;
        Poll::Pending
    }
}

/// `__with_finally_async(body, after)` — `after` runs whether or not `body`
/// failed, and the failure keeps travelling.
pub async fn with_finally_async(body: Boxed<()>, after: impl FnOnce()) {
    let outcome = Catch::new(body).await;
    after();
    if let Err(failure) = outcome {
        raise(failure);
    }
}

// ------------------------------------------------------------- the joins ---

/// `Promise.all` — every task's value, in order; the first failure raises.
///
/// "Ready when all have settled OR any has failed" rather than "await them one
/// at a time", because `Promise.all` rejects as soon as ANY task rejects: a
/// program whose second task fails while the first is still pending must see
/// the second's failure rather than wait for the first.
pub async fn settle_all<T: Clone>(tasks: Vec<Task<T>>) -> Vec<T> {
    let nodes: Vec<Rc<TaskNode>> = tasks.iter().map(|task| Rc::clone(&task.node)).collect();
    AllSettledOrAnyFailed { nodes }.await;
    for task in &tasks {
        if let Some(failure) = task.node.failure.borrow().clone() {
            raise(failure);
        }
    }
    tasks
        .iter()
        .map(|task| {
            task.value
                .borrow()
                .clone()
                .expect("a settled task that did not fail has its value")
        })
        .collect()
}

struct AllSettledOrAnyFailed {
    nodes: Vec<Rc<TaskNode>>,
}

impl Future for AllSettledOrAnyFailed {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<()> {
        let this = self.as_mut().get_mut();
        // `Promise.all` attaches a handler to every task at once, so every task
        // in the list is observed from the first poll — a later failure in a
        // task the raise walked past is absorbed here, not reported.
        for node in &this.nodes {
            node.observed.set(true);
        }
        let mut pending = false;
        for node in &this.nodes {
            if node.settled.get() {
                if node.failure.borrow().is_some() {
                    return Poll::Ready(());
                }
            } else {
                pending = true;
            }
        }
        if !pending {
            return Poll::Ready(());
        }
        for node in &this.nodes {
            if !node.settled.get() {
                park_on(&node.waiters);
            }
        }
        Poll::Pending
    }
}

/// `Promise.race` — the first task to settle, its value or its failure. The
/// losers keep running; inside a nursery, `cancel()` is what stops them.
pub async fn race<T: Clone>(tasks: Vec<Task<T>>) -> T {
    let nodes: Vec<Rc<TaskNode>> = tasks.iter().map(|task| Rc::clone(&task.node)).collect();
    AnySettled { nodes }.await;
    for task in &tasks {
        if task.node.settled.get() {
            if let Some(failure) = task.node.failure.borrow().clone() {
                raise(failure);
            }
            return task
                .value
                .borrow()
                .clone()
                .expect("a settled task that did not fail has its value");
        }
    }
    crate::panic_with("Promise.race on an empty task list never settles")
}

struct AnySettled {
    nodes: Vec<Rc<TaskNode>>,
}

impl Future for AnySettled {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<()> {
        let this = self.as_mut().get_mut();
        for node in &this.nodes {
            node.observed.set(true);
        }
        if this.nodes.iter().any(|node| node.settled.get()) {
            return Poll::Ready(());
        }
        for node in &this.nodes {
            park_on(&node.waiters);
        }
        Poll::Pending
    }
}

// ----------------------------------------------------- handles as values ---

/// Every one of the four handles a vilan program can HOLD (`Task`, `Nursery`,
/// `CancelSignal`, `TimerHandle`) is an `external struct`, which means two
/// things for the emitter: a vilan struct with a field of that type derives
/// `PartialEq` and gets an emitted `Js` rendering, so the handle owes both.
///
/// Equality is handle identity, as it is on the JS backend (a class instance
/// compares by reference). `console.log` of a host handle prints the host's own
/// object inspection — `__Timer { settled: false, … }` under node — and there is
/// nothing to reproduce there: the shape is the JS runtime's, not the
/// language's. So `print` of a handle PANICS with that sentence rather than
/// inventing a rendering the differential would then have to believe. `Js` is
/// implemented at all because the struct that HOLDS the handle needs it to
/// compile, and such a struct is printable exactly as long as nothing reaches
/// the handle's own slot.
impl PartialEq for Nursery {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl crate::Js for Nursery {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `Nursery` is a host object's own inspection, which the native backend \
             does not reproduce",
        )
    }
}

impl PartialEq for TimerHandle {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl crate::Js for TimerHandle {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `Timer` is a host object's own inspection, which the native backend does \
             not reproduce",
        )
    }
}

impl PartialEq for CancelSignal {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self.0.as_ptr(), other.0.as_ptr())
    }
}

impl crate::Js for CancelSignal {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `CancelSignal` is a host object's own inspection, which the native \
             backend does not reproduce",
        )
    }
}

impl<T> crate::Js for Task<T> {
    fn js(&self) -> String {
        crate::panic_with(
            "printing a `Task` is a host object's own inspection, which the native backend does \
             not reproduce",
        )
    }
}

// --------------------------------------------------------------- runtime ---

/// The whole runtime: a task slab, the microtask queue, the deadline list, the
/// id of the task currently being polled, and the registration counter.
///
/// A thread-local rather than a parameter, for the reason every `async` runtime
/// has one: a leaf future deep inside user code has no channel through which the
/// executor could have been handed to it, and the alternative — threading a
/// runtime handle through every emitted signature — would put the executor into
/// the emitted source, which §10 rules out.
struct Runtime {
    tasks: RefCell<Vec<TaskSlot>>,
    /// F24: slab indices whose task has been reaped, newest first. A spawn takes
    /// one from here before it grows the slab, which is what keeps a server's
    /// per-request spawns from growing it without bound.
    free_slots: RefCell<Vec<usize>>,
    /// F18: the external readiness sources the loop polls once per turn — a
    /// listening socket and its connections. Empty for every program that does
    /// no I/O, which is what keeps the turn model below EXACTLY J6's for all of
    /// them.
    io: RefCell<Vec<Rc<dyn IoSource>>>,
    microtasks: RefCell<VecDeque<TaskId>>,
    timers: RefCell<Vec<Rc<TimerSlot>>>,
    current: Cell<Option<TaskId>>,
    sequence: Cell<u64>,
    /// F160: the socket another thread wakes this loop's readiness wait
    /// through, opened by the first wait that needs it.
    bell: RefCell<BellState>,
    /// How many macrotask steps the loop has taken ([`turns`]).
    turns: Cell<u64>,
    /// F160: what the last readiness wait reported, sorted by socket, for the
    /// poll right after it ([`reported_quiet`]); empty otherwise.
    reported: RefCell<Vec<Watch>>,
    /// F160: the last turn ended in a readiness wait whose report is still
    /// unread, so the next poll needs no look of its own.
    fresh: Cell<bool>,
    /// What wakes this loop from another thread ([`LoopSignal`]), made by the
    /// first waker, wait or host that needs it.
    signal: RefCell<Option<Arc<LoopSignal>>>,
    /// R8: the helper a hosted loop's sockets are waited on by
    /// ([`install_socket_watcher`]).
    watcher: RefCell<Option<Box<dyn SocketWatcher>>>,
}

thread_local! {
    static RUNTIME: Runtime = const {
        Runtime {
            tasks: RefCell::new(Vec::new()),
            free_slots: RefCell::new(Vec::new()),
            io: RefCell::new(Vec::new()),
            microtasks: RefCell::new(VecDeque::new()),
            timers: RefCell::new(Vec::new()),
            current: Cell::new(None),
            sequence: Cell::new(0),
            bell: RefCell::new(BellState::Unopened),
            turns: Cell::new(0),
            reported: RefCell::new(Vec::new()),
            fresh: Cell::new(false),
            signal: RefCell::new(None),
            watcher: RefCell::new(None),
        }
    };
}

fn current_task() -> Option<TaskId> {
    RUNTIME.with(|runtime| runtime.current.get())
}

/// Writes the CURRENT task's id into `waiters` — how every leaf future in this
/// file parks. Registering twice is harmless: a duplicate wake is one extra poll
/// of a future that re-checks its own condition.
fn park_on(waiters: &RefCell<Vec<TaskId>>) {
    if let Some(task) = current_task() {
        waiters.borrow_mut().push(task);
    }
}

/// The task polling right now, held by a leaf future in a SIBLING module so
/// whatever it waits on can wake it later — the cross-module spelling of
/// [`park_on`], without opening the task slab's ids (A173 S1: `vilan_rt::process`'s
/// stdin read is woken by its I/O source, not by anything in this file).
pub(crate) struct Parked(TaskId);

impl Parked {
    /// The current task, if a task is being polled.
    pub(crate) fn current() -> Option<Parked> {
        current_task().map(Parked)
    }

    /// Re-polls the parked task on the next microtask drain.
    pub(crate) fn wake(&self) {
        enqueue(self.0);
    }
}

/// Raises the cancellation abort out of the current poll — a sibling module's
/// leaf future observing its signal, as [`Sleep`] does here.
pub(crate) fn raise_cancelled() -> ! {
    raise(Failure::Cancelled)
}

fn unpark_from(waiters: &RefCell<Vec<TaskId>>) {
    if let Some(task) = current_task() {
        waiters.borrow_mut().retain(|parked| *parked != task);
    }
}

fn enqueue(task: TaskId) {
    RUNTIME.with(|runtime| runtime.microtasks.borrow_mut().push_back(task));
}

fn register_timer(milliseconds: i32, action: impl Fn() + 'static) -> Rc<TimerSlot> {
    RUNTIME.with(|runtime| {
        let sequence = runtime.sequence.get();
        runtime.sequence.set(sequence + 1);
        let slot = Rc::new(TimerSlot {
            deadline: Instant::now() + Duration::from_millis(milliseconds.max(0) as u64),
            sequence,
            fired: Cell::new(false),
            cancelled: Cell::new(false),
            action: RefCell::new(Some(Rc::new(action))),
        });
        runtime.timers.borrow_mut().push(Rc::clone(&slot));
        slot
    })
}

/// Puts `node` in a slab slot and answers the handle to it (F24): a freed slot
/// if there is one, a fresh one otherwise.
fn register_task(node: Rc<TaskNode>) -> TaskId {
    RUNTIME.with(|runtime| {
        let reused = runtime.free_slots.borrow_mut().pop();
        let mut tasks = runtime.tasks.borrow_mut();
        match reused {
            Some(index) => {
                let slot = &mut tasks[index];
                slot.node = Some(node);
                TaskId {
                    index,
                    generation: slot.generation,
                }
            }
            None => {
                tasks.push(TaskSlot {
                    node: Some(node),
                    generation: 0,
                });
                TaskId {
                    index: tasks.len() - 1,
                    generation: 0,
                }
            }
        }
    })
}

/// The task `id` names, or `None` if the slot was reaped — including the case
/// where it has since been handed to a different task, which is what the
/// generation check rules out (F24).
fn task_at(id: TaskId) -> Option<Rc<TaskNode>> {
    RUNTIME.with(|runtime| {
        let tasks = runtime.tasks.borrow();
        let slot = tasks.get(id.index)?;
        if slot.generation != id.generation {
            return None;
        }
        slot.node.clone()
    })
}

/// How many slots the task slab holds — F24's measurement, and the only reason
/// anything outside the runtime asks.
#[cfg(test)]
fn slab_slots() -> usize {
    RUNTIME.with(|runtime| runtime.tasks.borrow().len())
}

/// Polls one task once, with the panic fence that latches its failure.
fn poll_task(id: TaskId) {
    let Some(node) = task_at(id) else { return };
    if node.settled.get() {
        return;
    }
    let Some(mut driver) = node.driver.borrow_mut().take() else {
        // A re-entrant poll of a task already on the stack. Nothing in this file
        // does it; the guard makes a future that somehow did a no-op rather than
        // a `RefCell` panic.
        return;
    };
    let previous = RUNTIME.with(|runtime| runtime.current.replace(Some(id)));
    let mut context = Context::from_waker(Waker::noop());
    // F25: a panic caught HERE is a task's failure latching, not the program
    // failing, so the process hook stays quiet for it — `report_unobserved`
    // and the raise out of `block_on` are what say something.
    crate::enter_caught();
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        driver.as_mut().poll(&mut context)
    }));
    crate::leave_caught();
    RUNTIME.with(|runtime| runtime.current.set(previous));
    match outcome {
        Ok(Poll::Pending) => {
            *node.driver.borrow_mut() = Some(driver);
        }
        Ok(Poll::Ready(())) => node.settle(None),
        Err(payload) => node.settle(Some(classify(&payload))),
    }
}

/// The microtask queue, drained to EXHAUSTION — §10 (2), and observable: a write
/// landing after a turn settled schedules one microtask drain, and
/// `reactive-turns` is the pin on the order that produces.
fn drain_microtasks() {
    loop {
        let next = RUNTIME.with(|runtime| runtime.microtasks.borrow_mut().pop_front());
        let Some(id) = next else { return };
        poll_task(id);
    }
}

/// The macrotask boundary: `__task` reports an unowned, unobserved failure one
/// macrotask after it settled, which is "after the current microtask cascade has
/// finished". Reporting here IS that timing.
fn report_unobserved_failures() {
    let nodes: Vec<Rc<TaskNode>> = RUNTIME.with(|runtime| {
        runtime
            .tasks
            .borrow()
            .iter()
            .filter_map(|slot| slot.node.clone())
            .collect()
    });
    for node in nodes {
        if node.settled.get() && !node.observed.get() && !node.owned {
            node.report_unobserved();
        }
    }
}

/// Settled tasks leave the slab, and their SLOTS go back on the free list (F24).
/// Every `Task` handle keeps its own `Rc`, so this drops the RUNTIME's share of
/// a finished task rather than growing the live set for the program's lifetime.
/// It runs after [`report_unobserved_failures`], so nothing is reaped before its
/// report was decided.
///
/// The generation is bumped as the slot is freed rather than as it is reused, so
/// every handle minted for the task that just left stops naming the slot AT the
/// reap — whether or not anything takes it afterwards. A wake list holding one
/// wakes nothing, which is the guarantee the never-reused slab gave for free and
/// the only thing the free list could have taken away.
fn reap_settled() {
    RUNTIME.with(|runtime| {
        let mut tasks = runtime.tasks.borrow_mut();
        let mut free = runtime.free_slots.borrow_mut();
        for (index, slot) in tasks.iter_mut().enumerate() {
            if slot.node.as_ref().is_some_and(|node| node.settled.get()) {
                slot.node = None;
                slot.generation += 1;
                free.push(index);
            }
        }
    });
}

/// Fires the SINGLE earliest due timer, sleeping the thread until its deadline.
///
/// One timer per turn, not every timer that has come due: two `setTimeout(_, 0)`
/// callbacks are two macrotasks, and the microtask queue drains to exhaustion
/// BETWEEN them. Firing both in one pass would put the second callback ahead of
/// the first one's continuations, which is a different program.
///
/// Answers [`Step::Ended`] when the deadline list is empty (or a termination
/// stopped the loop). An OWNED loop sleeps until the earliest deadline and
/// fires it; a HOSTED one (R8) fires it only when it is due and otherwise
/// answers [`Step::Idle`] with the deadline, which the host waits out instead.
fn advance_timers(mode: Mode) -> Step {
    if stopped() {
        return Step::Ended;
    }
    let Some(slot) = earliest_timer() else {
        return Step::Ended;
    };
    match mode {
        Mode::Owned => wait_until(slot.deadline),
        Mode::Hosted => {
            note_program_loop();
            if slot.deadline > Instant::now() {
                return Step::Idle(Some(slot.deadline));
            }
        }
    }
    if stopped() {
        return Step::Ended;
    }
    fire_timer(&slot);
    Step::Ran
}

/// Sleeps the program's thread until `deadline`, or until a termination
/// request wakes it (F157): an hour-long `sleep` in a detached task must not
/// stand between a SIGTERM and the program's end. Parked rather than slept,
/// so [`request_termination`]'s unpark reaches it.
fn wait_until(deadline: Instant) {
    note_program_loop();
    loop {
        let now = Instant::now();
        if now >= deadline || termination_requested() {
            return;
        }
        std::thread::park_timeout(deadline - now);
    }
}

/// The earliest live entry of the deadline list, with the dead ones swept.
fn earliest_timer() -> Option<Rc<TimerSlot>> {
    RUNTIME.with(|runtime| {
        let mut timers = runtime.timers.borrow_mut();
        timers.retain(|slot| !slot.cancelled.get() && !slot.fired.get());
        timers
            .iter()
            .min_by_key(|slot| (slot.deadline, slot.sequence))
            .cloned()
    })
}

/// Marks one timer fired and runs its action — the macrotask itself, split out
/// so the I/O turn (F18) can fire a DUE timer without the wait.
fn fire_timer(slot: &Rc<TimerSlot>) {
    slot.fired.set(true);
    let action = slot.action.borrow_mut().take();
    if let Some(action) = action {
        action();
    }
}

// ------------------------------------------------------------------- I/O ---

/// A source of external readiness the loop looks at once per turn (F18).
///
/// The turn model is a POLL, and stays one: each turn asks every source "did
/// anything happen" — a listening socket ACCEPTS without blocking, its
/// connections read and write without blocking — and a `true` answer is one
/// macrotask, exactly as a fired timer is. That is what keeps the order in
/// which tasks wake identical with and without I/O.
///
/// What changes with F160 is what the loop does when NOTHING happened. Before,
/// it slept [`IO_POLL_INTERVAL`] and asked again, so an idle server woke a
/// thousand times a second and every request waited up to a millisecond for
/// the next look. Now every source DESCRIBES what it waits for
/// ([`IoSource::interest`]) — sockets that must become readable or writable,
/// a deadline of its own, or a wake from another thread through a
/// [`LoopWaker`] — and the loop blocks in the readiness wait a native program
/// installs ([`install_readiness`]) until one of them, or the next timer, is
/// due. The wait is `poll(2)` / `WSAPoll`, which is FFI, and this crate has no
/// dependency and no `unsafe` by rule (Order 37 R8, Order 39 R1), so the wait
/// is `vilan-rt-poll`'s and everything else — which sockets, which deadline,
/// what happens after — is this file's.
///
/// The tick remains as the FALLBACK, for a program that installed no wait
/// (this crate's own unit tests; a build that did not link `vilan-rt-poll`),
/// for a source that cannot describe itself (the trait's default), and for a
/// wait that failed.
pub trait IoSource {
    /// Look for work, without blocking. `true` if anything progressed — a
    /// connection accepted, bytes read, bytes written, a request dispatched.
    /// A `true` answer sends the loop back to the microtask queue, exactly as a
    /// fired timer does.
    fn poll(&self) -> bool;

    /// F160: what this source waits for, asked right after a [`IoSource::poll`]
    /// of every source found nothing to do, before the loop blocks — every
    /// socket whose readiness would give the next poll work
    /// ([`Interest::watch`]), and the earliest moment its next poll has work
    /// whatever the sockets do ([`Interest::wake_at`]: a request deadline, a
    /// linger). Work that arrives from ANOTHER thread is not described here:
    /// that thread wakes the loop through a [`LoopWaker`] it was handed.
    ///
    /// `true` if the description is complete — the loop may then block until
    /// one of those, or the next timer, is due. `false`, the default, if this
    /// source cannot say, and the loop falls back to the tick. A description
    /// may be generous (a deadline that turns out to have nothing behind it
    /// costs one extra turn), never short: a socket or a moment left out is a
    /// request that waits for something else to wake the loop.
    fn interest(&self, interest: &mut Interest) -> bool {
        let _ = interest;
        false
    }

    /// F146: when this is the ONLY live source and no readiness wait is in
    /// use, the loop asks it to WAIT for work itself — at most `limit` (the
    /// next timer's deadline), or as long as it takes with no timer — instead
    /// of sleeping [`IO_POLL_INTERVAL`] between polls. `true` if it waited
    /// (whatever it received is the next [`IoSource::poll`]'s to answer);
    /// `false`, the default, if it cannot block, and the loop ticks as before.
    /// Only a source whose work arrives on a channel can say yes: nothing else
    /// can wake the loop while it blocks, which is why a second live source
    /// turns this off.
    fn block(&self, limit: Option<Duration>) -> bool {
        let _ = limit;
        false
    }

    /// Whether this source can still produce work. A closed listener with no
    /// live connections answers `false`, is dropped from the registry, and the
    /// loop goes back to exiting when the deadline list empties.
    fn is_live(&self) -> bool;

    /// F157: whether this source holds a termination's graceful stop open —
    /// a server still answering what it accepted. Once a termination request
    /// is in and no registered source answers `true`, the loop ENDS, whatever
    /// timers and tasks are still pending; a source that would only wait for
    /// more input (stdin) holds nothing, which is the default.
    fn holds_the_stop(&self) -> bool {
        false
    }
}

/// How long the loop waits between polls when it cannot wait for readiness
/// (see [`IoSource`]): no wait installed, a source that cannot describe
/// itself, or a wait that failed.
const IO_POLL_INTERVAL: Duration = Duration::from_millis(1);

/// The operating system's name for a socket, as the readiness wait takes it:
/// a file descriptor on unix, a `SOCKET` on Windows.
#[cfg(unix)]
pub type RawSocket = std::os::fd::RawFd;
/// The operating system's name for a socket, as the readiness wait takes it:
/// a file descriptor on unix, a `SOCKET` on Windows.
#[cfg(windows)]
pub type RawSocket = std::os::windows::io::RawSocket;

/// A socket [`Interest::watch`] can name — every `std::net` socket. Reading
/// the raw handle is safe; only waiting on it is not, and that is
/// `vilan-rt-poll`'s.
pub trait Pollable {
    /// The handle the readiness wait names this socket by.
    fn raw_socket(&self) -> RawSocket;
}

#[cfg(unix)]
impl<T: std::os::fd::AsRawFd> Pollable for T {
    fn raw_socket(&self) -> RawSocket {
        self.as_raw_fd()
    }
}

#[cfg(windows)]
impl<T: std::os::windows::io::AsRawSocket> Pollable for T {
    fn raw_socket(&self) -> RawSocket {
        self.as_raw_socket()
    }
}

/// One socket the readiness wait watches, and for what.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Watch {
    pub socket: RawSocket,
    /// Wake when it can be read without blocking — bytes, an end of stream, a
    /// connection to accept, or an error.
    pub readable: bool,
    /// Wake when it can be written without blocking, or has failed.
    pub writable: bool,
    /// Set by the wait: whether the socket reported anything — ready for
    /// what it was watched for, an error, or a hang-up.
    pub ready: bool,
}

/// What every live source waits for, gathered before the loop blocks (F160).
pub struct Interest {
    /// When this turn began, before its poll. A deadline at or before it has
    /// already been seen by the poll — it fired, or it means nothing now — so
    /// it is left out rather than turning the wait into a spin.
    turn: Instant,
    watches: Vec<Watch>,
    deadline: Option<Instant>,
}

impl Interest {
    fn new(turn: Instant) -> Interest {
        Interest {
            turn,
            watches: Vec::new(),
            deadline: None,
        }
    }

    /// Wake when `socket` can be read (`readable`) or written (`writable`)
    /// without blocking. Watching for neither is watching nothing: a socket a
    /// source does not mean to touch is left out, because an error or hang-up
    /// on it would wake the wait to a poll that does not look.
    pub fn watch(&mut self, socket: &impl Pollable, readable: bool, writable: bool) {
        if readable || writable {
            self.watches.push(Watch {
                socket: socket.raw_socket(),
                readable,
                writable,
                ready: false,
            });
        }
    }

    /// Wake by `deadline` at the latest. A moment at or before the start of
    /// this turn is ignored (see [`Interest::turn`]).
    pub fn wake_at(&mut self, deadline: Instant) {
        if deadline > self.turn {
            self.deadline = Some(self.deadline.map_or(deadline, |held| held.min(deadline)));
        }
    }

    /// The sockets watched so far — for a source's own tests.
    #[cfg(test)]
    pub(crate) fn watches(&self) -> &[Watch] {
        &self.watches
    }

    /// The deadline gathered so far — for a source's own tests.
    #[cfg(test)]
    pub(crate) fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// An interest for a source's own tests, as of `turn`.
    #[cfg(test)]
    pub(crate) fn starting(turn: Instant) -> Interest {
        Interest::new(turn)
    }
}

/// The readiness wait (F160): block until one of the watched sockets is ready
/// for what it is watched for (or has an error or hang-up to report), or
/// `timeout` passes — `None` is no limit — and set each [`Watch::ready`]. An
/// early return is allowed (an interrupted wait, a spurious wake): the loop
/// only polls again. A late one is not. An `Err` makes the loop tick for that
/// turn instead.
pub type ReadinessWait = fn(&mut [Watch], Option<Duration>) -> std::io::Result<()>;

/// The wait [`install_readiness`] installed, process-wide.
static READINESS: OnceLock<ReadinessWait> = OnceLock::new();

/// Installs the readiness wait the loop blocks in — `vilan_rt_poll::install`,
/// called from the emitted `main` of a program that does I/O. The first
/// install wins; a program that never calls this keeps the tick.
pub fn install_readiness(wait: ReadinessWait) {
    let _ = READINESS.set(wait);
}

/// The sending end of a loop's bell: a UDP socket connected to the loop's own.
struct BellRope {
    sender: UdpSocket,
}

/// The loop's bell (F160): a loopback UDP socket the readiness wait always
/// watches, so ANOTHER thread can end the wait by sending it a datagram. Plain
/// `std::net`, so it needs nothing the rule forbids, and it is a socket, so it
/// waits in the same `poll` / `WSAPoll` as everything else on every platform
/// (a pipe or an eventfd would not, on Windows). Its sending end lives in the
/// loop's [`LoopSignal`], where every waker finds it.
struct Bell {
    receiver: UdpSocket,
}

impl Bell {
    fn open() -> std::io::Result<(Bell, BellRope)> {
        let receiver = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        receiver.set_nonblocking(true)?;
        let sender = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0))?;
        sender.connect(receiver.local_addr()?)?;
        sender.set_nonblocking(true)?;
        Ok((Bell { receiver }, BellRope { sender }))
    }

    /// Takes every datagram rung since the last drain. Each one only meant
    /// "look again", and the poll that follows looks.
    fn drain(&self) {
        let mut datagram = [0u8; 8];
        while self.receiver.recv(&mut datagram).is_ok() {}
    }
}

/// Where a loop's bell stands.
enum BellState {
    /// Not needed yet: no readiness wait has run on this thread.
    Unopened,
    Open(Bell),
    /// The loopback socket could not be opened: this loop ticks.
    Unavailable,
}

/// The program's loop — the first loop in the process to wait (on a timer or
/// for readiness), open its bell, or take a host ([`install_host`]) — which a
/// termination request wakes from the signal thread (F45/F157): its bell
/// rings, its host's wake runs, and its thread is unparked.
static PROGRAM_LOOP: OnceLock<Arc<LoopSignal>> = OnceLock::new();

/// Records the current thread's loop as the program's, if none is yet.
fn note_program_loop() {
    PROGRAM_LOOP.get_or_init(this_loop_signal);
}

/// Runs `with` on this thread's bell, opening it on first use. `None` when no
/// readiness wait is installed, the bell cannot be opened, or the loop has a
/// host (R8): a hosted loop never blocks in the readiness wait, so nothing
/// would ever hear the bell.
fn with_bell<R>(with: impl FnOnce(&Bell) -> R) -> Option<R> {
    READINESS.get()?;
    RUNTIME.with(|runtime| {
        let mut state = runtime.bell.borrow_mut();
        if matches!(*state, BellState::Unopened) {
            let signal = this_loop_signal();
            if signal.host_wake.get().is_some() {
                return None;
            }
            *state = match Bell::open() {
                Ok((bell, rope)) => {
                    let _ = signal.rope.set(rope);
                    PROGRAM_LOOP.get_or_init(|| Arc::clone(&signal));
                    BellState::Open(bell)
                }
                Err(_) => BellState::Unavailable,
            };
        }
        match &*state {
            BellState::Open(bell) => Some(with(bell)),
            _ => None,
        }
    })
}

/// A host's wake (R8): the function an event loop that is not this crate's
/// hands the executor, called from ANY thread whenever the executor's loop
/// has something to do that the host would otherwise not know about.
///
/// It must be cheap, must not block, and must not run the executor itself —
/// it only asks the host, from whatever thread it is called on, to call
/// [`turn`] (or [`Program::turn`]) on the loop's thread soon. A wake that
/// arrives WHILE a turn runs must still lead to another turn: the host
/// clears its "woken" note before each turn, never after. Under `winit` it is
/// `EventLoopProxy::send_event`.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

/// Everything that ends one loop's wait from another thread (F160; R8): its
/// bell's sending end once the bell is open, the host's wake once a host is
/// installed, and the loop's thread, unparked for a loop asleep on a timer or
/// ticking. One per loop thread, shared by every [`LoopWaker`] taken there, so
/// a bell opened or a host installed after a waker was taken still reaches it.
struct LoopSignal {
    thread: std::thread::Thread,
    rope: OnceLock<BellRope>,
    host_wake: OnceLock<Wake>,
}

impl LoopSignal {
    fn wake(&self) {
        if let Some(rope) = self.rope.get() {
            // A full socket buffer refuses the send, and is itself a ring.
            let _ = rope.sender.send(&[1]);
        }
        if let Some(host_wake) = self.host_wake.get() {
            host_wake();
        }
        self.thread.unpark();
    }
}

/// The current thread's [`LoopSignal`], made on first use.
fn this_loop_signal() -> Arc<LoopSignal> {
    RUNTIME.with(|runtime| {
        Arc::clone(runtime.signal.borrow_mut().get_or_insert_with(|| {
            Arc::new(LoopSignal {
                thread: std::thread::current(),
                rope: OnceLock::new(),
                host_wake: OnceLock::new(),
            })
        }))
    })
}

/// A handle another thread wakes the loop with (F160) — taken ON the loop's
/// thread, moved to the thread whose work the loop waits for (stdin's reader,
/// a WebSocket's connect), and woken AFTER that work is where the loop's next
/// poll finds it. A wake rings the loop's bell when it has one, calls its
/// host's wake when it has a host (R8), and unparks its thread either way,
/// which ends a tick or a timer sleep early.
#[derive(Clone)]
pub struct LoopWaker {
    signal: Arc<LoopSignal>,
}

impl LoopWaker {
    /// Ends the loop's current wait, or the next one if it is not waiting:
    /// the datagram stays in the bell until the loop drains it, and a host
    /// keeps its own note. Cheap, never blocks, safe from any thread.
    pub fn wake(&self) {
        self.signal.wake();
    }
}

/// The waker for the loop of the CURRENT thread.
pub fn loop_waker() -> LoopWaker {
    // Opened here, as before the signal was shared, so a wake that comes
    // before the loop's first readiness wait still finds a bell to ring.
    with_bell(|_| ());
    LoopWaker {
        signal: this_loop_signal(),
    }
}

// ----------------------------------------------------------------- hosts ---

/// Waits on sockets for a HOSTED loop (R8), off the loop's thread: the loop's
/// thread belongs to its host and never blocks in the readiness wait, so a
/// socket's readiness reaches the host through a helper that waits for it
/// and then calls the host's [`Wake`]. `vilan_rt_poll::install_hosted` gives
/// a loop one; a hosted loop without one ticks while it watches a socket.
///
/// Every method is called on the loop's thread, and the contract is the
/// handoff that keeps a socket from being closed while the helper waits on
/// it: the helper waits ONLY between [`SocketWatcher::arm`] (the end of a turn
/// that found nothing to do) and [`SocketWatcher::disarm`] (the start of the
/// next turn, before any program code runs), and program code runs only
/// inside turns — so no source can close or replace a socket in a watch list
/// the helper still waits on.
pub trait SocketWatcher {
    /// Starts waiting, on the helper's thread, until one of `watches` is
    /// ready for what it is watched for (or reports an error or hang-up);
    /// then marks each [`Watch::ready`], keeps the report for
    /// [`SocketWatcher::disarm`], calls the host's wake once, and waits for
    /// the NEXT `arm` rather than looking again — the readiness is level, so
    /// looking again before the loop has acted on it would only spin.
    /// `watches` is never empty. `false` if it cannot wait (the loop ticks
    /// instead).
    fn arm(&mut self, watches: Vec<Watch>) -> bool;

    /// Ends the wait the last `arm` started, and answers its report if it
    /// completed — the watches, each [`Watch::ready`] set — or `None` if it
    /// did not (it was interrupted, or nothing was armed). MUST NOT RETURN
    /// while the helper may still be inside a wait on any of those sockets.
    fn disarm(&mut self) -> Option<Vec<Watch>>;

    /// Nothing is watched any more: the helper may end its thread. A later
    /// `arm` starts one again. Called with the watcher disarmed.
    fn release(&mut self);
}

/// Makes the CURRENT thread's loop a hosted one (R8): `wake` is called, from
/// any thread, by everything that wakes the loop — a [`LoopWaker`], a
/// termination request ([`request_termination`]), a [`SocketWatcher`]'s
/// report. The host then drives the loop with [`turn`] / [`Program::turn`] on
/// this thread. Installed once per thread; a second install is ignored.
///
/// A host does not have to install a wake to call [`turn`] — a loop is hosted
/// by being driven through it — but without one nothing that happens on
/// another thread can reach it, only its deadlines.
pub fn install_host(wake: Wake) {
    let signal = this_loop_signal();
    let _ = signal.host_wake.set(wake);
    PROGRAM_LOOP.get_or_init(|| signal);
}

/// Gives the CURRENT thread's hosted loop its [`SocketWatcher`] (R8). The
/// first install on a thread wins.
pub fn install_socket_watcher(watcher: Box<dyn SocketWatcher>) {
    RUNTIME.with(|runtime| {
        let mut slot = runtime.watcher.borrow_mut();
        if slot.is_none() {
            *slot = Some(watcher);
        }
    });
}

/// What a hosted turn leaves its host to do (R8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    /// Nothing can run now. Call [`turn`] again when the host's wake is
    /// called, or at `deadline` (the next timer, a source's own deadline, or
    /// — for a socket the loop cannot have watched — the 1 ms tick),
    /// whichever comes first; with no `deadline`, only the wake will.
    /// `live_sources` says whether an I/O source is still registered — a
    /// server, a dial, stdin, a window — the reason the loop is waiting at all
    /// when there is no deadline.
    Idle {
        deadline: Option<Instant>,
        live_sources: bool,
    },
    /// The loop is over: stop calling [`turn`], and (for a [`Program`]) take
    /// its value with [`Program::finish`].
    Finished(Finished),
}

/// Why a hosted loop is over.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Finished {
    /// Nothing is left: no microtask, no timer, no live source — where node
    /// exits.
    Drained,
    /// A termination request's graceful stop completed (F45/F157), whatever
    /// else was pending.
    Stopped,
}

/// Runs everything runnable NOW on the current thread's loop and answers what
/// the host must wait for — the hosted form of [`run_pending`] (R8), for a
/// program whose `main` is synchronous.
///
/// A hosted turn is the owned loop's turns, the same function, up to the
/// point where the owned loop would block: microtasks drain to exhaustion,
/// then one macrotask (a due timer, or a poll of the I/O sources), then the
/// microtasks again, and so on — and where the owned loop would sleep until a
/// deadline or wait for readiness, the turn answers [`Turn::Idle`] instead.
/// It never blocks, so it is the host's to call from inside its own event
/// handler. Everything that runs program code must run inside a turn: a
/// window event, for one, is queued for the window's [`IoSource`] and then a
/// turn is run, never delivered into the program from the handler directly
/// (see [`SocketWatcher`] for why).
pub fn turn() -> Turn {
    hosted_turn(None)
}

/// An `async fun main` run by a host (R8) — [`block_on`]'s hosted form: the
/// root task, spawned (and run to its first suspension) by [`Program::start`],
/// driven by [`Program::turn`], and its value taken by [`Program::finish`].
pub struct Program<T> {
    root: Task<T>,
}

impl<T: Clone + 'static> Program<T> {
    /// Spawns `body` as the root task — eagerly, as every spawn is.
    pub fn start(body: impl Future<Output = T> + 'static) -> Program<T> {
        let root = spawn(body, "top level");
        // The root is OBSERVED by definition: the loop reads its failure and
        // raises it. Without this it also took the unobserved-report path, so
        // a failing `async fun main` printed `unhandled task error (spawned in
        // top level)` and then the message — two lines where node prints one
        // (F25).
        root.node.observed.set(true);
        Program { root }
    }

    /// [`turn`] with this program's root: a root that FAILS raises its
    /// failure out of this call, as it raises out of [`block_on`] — node
    /// takes the process down there — and the host lets it travel to the
    /// emitted `main`'s guard.
    pub fn turn(&self) -> Turn {
        hosted_turn(Some(&self.root.node))
    }

    /// The root's value, once a turn answered [`Turn::Finished`]. A
    /// termination that ended the loop with `main` still suspended unwinds
    /// with [`TerminationStop`], as [`block_on`] does.
    pub fn finish(self) -> T {
        let value = self.root.value.borrow().clone();
        match value {
            Some(value) => value,
            // F157: the loop ended on a termination with `main` still
            // suspended. `resume_unwind` runs no panic hook, so nothing is
            // printed.
            None if termination_requested() => std::panic::resume_unwind(Box::new(TerminationStop)),
            None => panic!("the root task settled without a value"),
        }
    }
}

/// How a loop waits (R8): the OWNED loop ([`block_on`], [`run_pending`])
/// blocks on its own thread; a HOSTED one ([`turn`]) answers what it would
/// have waited for and returns to its host.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Owned,
    Hosted,
}

/// What one macrotask step did.
enum Step {
    /// Ran a macrotask, or waited (owned): go back to the microtasks.
    Ran,
    /// The loop is over.
    Ended,
    /// Hosted only: nothing to run before this deadline or a wake.
    Idle(Option<Instant>),
}

/// The loop itself, shared by every way of running it: drain the microtasks,
/// report and reap at the macrotask boundary, take one macrotask step — until
/// the loop ends, or (hosted) a step has nothing to do but wait. With a root,
/// its failure raises at the boundary.
fn drive(root: Option<&Rc<TaskNode>>, mode: Mode) -> Step {
    loop {
        drain_microtasks();
        report_unobserved_failures();
        if let Some(root) = root
            && let Some(failure) = root.failure.borrow().clone()
        {
            raise(failure);
        }
        reap_settled();
        match advance_macrotasks(mode) {
            Step::Ran => {}
            ended_or_idle => return ended_or_idle,
        }
    }
}

/// One hosted turn: take what the socket watcher reported (and end its wait
/// before any program code runs), then drive the loop until it ends or idles.
fn hosted_turn(root: Option<&Rc<TaskNode>>) -> Turn {
    let report = RUNTIME.with(|runtime| {
        runtime
            .watcher
            .borrow_mut()
            .as_mut()
            .and_then(|watcher| watcher.disarm())
    });
    if let Some(mut watches) = report {
        // The report is this turn's, as a blocking wait's is the owned
        // loop's next turn's: the poll reads it without a look of its own.
        watches.sort_unstable_by_key(|watch| watch.socket);
        RUNTIME.with(|runtime| {
            *runtime.reported.borrow_mut() = watches;
            runtime.fresh.set(true);
        });
    }
    match drive(root, Mode::Hosted) {
        Step::Idle(deadline) => Turn::Idle {
            deadline,
            live_sources: io_registered(),
        },
        Step::Ran | Step::Ended => {
            release_watcher();
            Turn::Finished(if termination_requested() {
                Finished::Stopped
            } else {
                Finished::Drained
            })
        }
    }
}

/// Lets the socket watcher end its thread: nothing is watched.
fn release_watcher() {
    RUNTIME.with(|runtime| {
        if let Some(watcher) = runtime.watcher.borrow_mut().as_mut() {
            watcher.release();
        }
    });
}

/// R8: where a hosted turn whose poll found nothing would have blocked. Every
/// source describes what it waits for; with a complete description the
/// sockets go to the [`SocketWatcher`] (none at all: no helper thread) and the
/// host is told the nearest deadline. A source that cannot describe itself,
/// or sockets with no watcher to wait on them, leave the host the tick.
fn hosted_idle(turn: Instant, requests: usize, next_timer: Option<Instant>) -> Step {
    note_program_loop();
    // As the owned wait does: a request that arrived during this turn is
    // acted on now rather than after a wait. (Its wake has reached the host
    // too, so this only saves the host a round trip.)
    if TERMINATION_REQUESTS.load(Ordering::SeqCst) != requests {
        return Step::Ran;
    }
    let watched = gather_interest(turn).and_then(|interest| {
        let own = interest.deadline;
        let watches = interest.watches;
        let armed = if watches.is_empty() {
            release_watcher();
            true
        } else {
            RUNTIME.with(|runtime| {
                runtime
                    .watcher
                    .borrow_mut()
                    .as_mut()
                    .is_some_and(|watcher| watcher.arm(watches))
            })
        };
        armed.then_some(own)
    });
    match watched {
        Some(own) => Step::Idle(match (next_timer, own) {
            (Some(timer), Some(own)) => Some(timer.min(own)),
            (timer, own) => timer.or(own),
        }),
        None => {
            release_watcher();
            let now = Instant::now();
            let remaining = next_timer.map(|deadline| deadline.saturating_duration_since(now));
            Step::Idle(Some(
                now + remaining.map_or(IO_POLL_INTERVAL, |remaining| {
                    remaining.min(IO_POLL_INTERVAL)
                }),
            ))
        }
    }
}

/// R8: the look a hosted turn takes before its poll when no watcher report is
/// fresh — the owned loop's [`Wait::Look`] on the loop's own thread, without
/// the bell, which a hosted loop does not have.
fn hosted_look(turn: Instant) {
    let Some(wait) = READINESS.get().copied() else {
        return;
    };
    let Some(interest) = gather_interest(turn) else {
        return;
    };
    let mut watches = interest.watches;
    if watches.is_empty() || wait(&mut watches, Some(Duration::ZERO)).is_err() {
        watches.clear();
    } else {
        watches.sort_unstable_by_key(|watch| watch.socket);
    }
    RUNTIME.with(|runtime| *runtime.reported.borrow_mut() = watches);
}

/// Asks every source what it waits for (F160), into the watch list the last
/// wait gave back, so a server's list is not reallocated every turn. `None`
/// if a source cannot describe itself; the list is given back either way the
/// caller does not keep it.
fn gather_interest(turn: Instant) -> Option<Interest> {
    let mut interest = Interest::new(turn);
    interest.watches = RUNTIME.with(|runtime| std::mem::take(&mut *runtime.reported.borrow_mut()));
    interest.watches.clear();
    for source in io_sources() {
        if !source.interest(&mut interest) {
            give_back(interest.watches);
            return None;
        }
    }
    Some(interest)
}

/// Returns a watch list's allocation to the runtime, empty, so nothing is
/// reported that a wait did not report.
fn give_back(mut watches: Vec<Watch>) {
    watches.clear();
    RUNTIME.with(|runtime| *runtime.reported.borrow_mut() = watches);
}

/// How many turns of the loop — macrotask steps — this thread has taken
/// (F160's measurement): an idle program that waits for readiness takes
/// almost none, where the tick took a thousand a second.
pub fn turns() -> u64 {
    RUNTIME.with(|runtime| runtime.turns.get())
}

/// Registers `source` with the loop. A server calls this when it binds.
pub fn register_io(source: Rc<dyn IoSource>) {
    RUNTIME.with(|runtime| runtime.io.borrow_mut().push(source));
}

fn io_registered() -> bool {
    RUNTIME.with(|runtime| !runtime.io.borrow().is_empty())
}

fn io_sources() -> Vec<Rc<dyn IoSource>> {
    RUNTIME.with(|runtime| runtime.io.borrow().iter().map(Rc::clone).collect())
}

/// Polls every registered source once, dropping the dead ones. `true` if any of
/// them progressed.
fn poll_io() -> bool {
    let sources = io_sources();
    let mut progressed = false;
    for source in &sources {
        if source.poll() {
            progressed = true;
        }
    }
    RUNTIME.with(|runtime| runtime.io.borrow_mut().retain(|source| source.is_live()));
    progressed
}

/// Whether a microtask is waiting — work the loop must not block in front of.
fn microtasks_pending() -> bool {
    RUNTIME.with(|runtime| !runtime.microtasks.borrow().is_empty())
}

/// The non-microtask half of one turn.
///
/// With no I/O registered this IS [`advance_timers`], unchanged — which is what
/// keeps every ordering rule J6 pinned true for every program that does no I/O.
/// With a live source it is the same one-macrotask-per-turn rule extended by one
/// kind of macrotask: a poll of the sources, then the single earliest DUE timer
/// (due, so the wait cannot be spent blind to a socket), then a wait — for
/// readiness (F160), or the tick when the loop cannot wait for readiness.
///
/// A HOSTED step (R8) is the same step up to the wait, which it hands to its
/// host instead ([`hosted_idle`]), and its readiness report comes from the
/// host's [`SocketWatcher`] rather than a wait of its own.
fn advance_macrotasks(mode: Mode) -> Step {
    RUNTIME.with(|runtime| runtime.turns.set(runtime.turns.get() + 1));
    if stopped() {
        return Step::Ended;
    }
    if !io_registered() {
        return advance_timers(mode);
    }
    let turn = Instant::now();
    let requests = TERMINATION_REQUESTS.load(Ordering::SeqCst);
    // F160: the poll reads only what a wait reported, so it needs a report
    // as of NOW — the blocking wait's when this turn follows one, else a look
    // that does not block. Every turn's report is fresh, so a socket whose
    // bytes arrive while the loop is busy is read at the next turn, never
    // starved behind its busier neighbours.
    if !RUNTIME.with(|runtime| runtime.fresh.replace(false)) {
        match mode {
            Mode::Owned => {
                readiness_wait(turn, Wait::Look);
            }
            Mode::Hosted => hosted_look(turn),
        }
    }
    let progressed = poll_io();
    forget_reported();
    if progressed {
        return Step::Ran;
    }
    // F1 M2: a termination's graceful stop can COMPLETE inside this poll — a
    // server whose connections were only waiting closes them and its listener
    // and leaves the registry — while a live source that does not hold the
    // stop (a desktop window, stdin's reader) stays. The check at the top of
    // the turn ran before that poll and saw the server still holding, so
    // without asking again the wait below blocks with nothing left that will
    // ever wake it, and the stop never ends the program.
    if stopped() {
        return Step::Ended;
    }
    if let Some(slot) = earliest_timer()
        && slot.deadline <= Instant::now()
    {
        fire_timer(&slot);
        return Step::Ran;
    }
    if !io_registered() {
        // Every source died in this turn's poll: back to the plain timer step,
        // which is allowed to sleep and lets the program exit.
        return advance_timers(mode);
    }
    if microtasks_pending() {
        // A poll that queued work without calling it progress: run it before
        // anything waits.
        return Step::Ran;
    }
    let next_timer = earliest_timer().map(|slot| slot.deadline);
    if mode == Mode::Hosted {
        return hosted_idle(turn, requests, next_timer);
    }
    if readiness_wait(
        turn,
        Wait::Block {
            requests,
            next_timer,
        },
    ) {
        RUNTIME.with(|runtime| runtime.fresh.set(true));
        return Step::Ran;
    }
    let remaining = next_timer.map(|deadline| deadline.saturating_duration_since(Instant::now()));
    // F146: a SOLE source that can block waits for its own work until the next
    // deadline — stdin's reader, parked on a silent terminal or pipe, sleeps in
    // the kernel rather than waking the loop a thousand times a second.
    if remaining.is_none_or(|remaining| !remaining.is_zero())
        && let Some(source) = sole_io_source()
        && source.block(remaining)
    {
        return Step::Ran;
    }
    tick(remaining);
    Step::Ran
}

/// F18's wait, now the fallback: at most [`IO_POLL_INTERVAL`], less when the
/// next timer is nearer. Parked rather than slept, so a [`LoopWaker`] ends it.
fn tick(remaining: Option<Duration>) {
    let wait = remaining.map_or(IO_POLL_INTERVAL, |remaining| {
        remaining.min(IO_POLL_INTERVAL)
    });
    if !wait.is_zero() {
        std::thread::park_timeout(wait);
    }
}

/// The two ways a turn asks the readiness wait (F160).
enum Wait {
    /// Before the poll: what is ready right now, without blocking.
    Look,
    /// After a poll that found nothing: until a watched socket is ready, the
    /// nearest deadline — the next timer's or a source's own — passes, or the
    /// bell rings. `requests` is the termination count when the turn began.
    Block {
        requests: usize,
        next_timer: Option<Instant>,
    },
}

/// F160: asks the readiness wait, and keeps what it reported for the poll
/// ([`reported_quiet`]). `false` if the loop cannot wait for readiness this
/// turn (see [`IoSource`]) — nothing is then reported, and nothing happened.
fn readiness_wait(turn: Instant, mode: Wait) -> bool {
    let Some(wait) = READINESS.get().copied() else {
        return false;
    };
    // The watch list is cleared on every road out, so nothing is reported
    // unless this wait reports it.
    let Some(interest) = gather_interest(turn) else {
        return false;
    };
    let Some(bell) = with_bell(|bell| bell.receiver.raw_socket()) else {
        give_back(interest.watches);
        return false;
    };
    let timeout = match mode {
        Wait::Look => Some(Duration::ZERO),
        Wait::Block {
            requests,
            next_timer,
        } => {
            note_program_loop();
            // A termination request rings the program's bell — but one that
            // arrived after this turn began and before the bell first opened
            // rang nothing. The bell is open now, so a later one will ring it.
            if TERMINATION_REQUESTS.load(Ordering::SeqCst) != requests {
                give_back(interest.watches);
                return true;
            }
            let deadline = match (next_timer, interest.deadline) {
                (Some(timer), Some(own)) => Some(timer.min(own)),
                (timer, own) => timer.or(own),
            };
            deadline.map(|deadline| deadline.saturating_duration_since(Instant::now()))
        }
    };
    let mut watches = interest.watches;
    watches.push(Watch {
        socket: bell,
        readable: true,
        writable: false,
        ready: false,
    });
    if wait(&mut watches, timeout).is_err() {
        // A wait that fails must not become a loop that spins: this turn
        // ticks, and what it reported is not known.
        if matches!(mode, Wait::Block { .. }) {
            tick(timeout);
        }
        with_bell(Bell::drain);
        give_back(watches);
        return true;
    }
    if watches.last().is_some_and(|watch| watch.ready) {
        with_bell(Bell::drain);
    }
    watches.pop();
    watches.sort_unstable_by_key(|watch| watch.socket);
    RUNTIME.with(|runtime| *runtime.reported.borrow_mut() = watches);
    true
}

/// Whether the wait that reported for this turn's poll WATCHED `socket` for
/// reading (`readable`) or for writing (`!readable`) and it reported nothing
/// at all — no readiness, no error, no hang-up (F160). A source then skips
/// the read or the write it would only have had refused: whatever arrives
/// after the report is the next turn's, whose report is fresh. `false`
/// whenever nothing is known — no wait reported, or the socket was not
/// watched that way.
pub fn reported_quiet(socket: &impl Pollable, readable: bool) -> bool {
    let socket = socket.raw_socket();
    RUNTIME.with(|runtime| {
        let reported = runtime.reported.borrow();
        reported
            .binary_search_by_key(&socket, |watch| watch.socket)
            .is_ok_and(|index| {
                let watch = &reported[index];
                !watch.ready
                    && if readable {
                        watch.readable
                    } else {
                        watch.writable
                    }
            })
    })
}

/// Forgets what the last wait reported: only the poll right after it may
/// read it.
fn forget_reported() {
    RUNTIME.with(|runtime| runtime.reported.borrow_mut().clear());
}

/// The registry's one source, when it holds exactly one (F146).
fn sole_io_source() -> Option<Rc<dyn IoSource>> {
    RUNTIME.with(|runtime| {
        let io = runtime.io.borrow();
        (io.len() == 1).then(|| Rc::clone(&io[0]))
    })
}

// ----------------------------------------------------------- termination ---

/// How many termination requests the process has had (F45). A COUNT, not a
/// flag, because the second one means something the first does not.
///
/// Process-wide and atomic because the request arrives from outside the
/// program's thread: `vilan-rt-signal` routes SIGTERM/SIGINT/SIGHUP here from
/// the thread its signal crate runs handlers on.
static TERMINATION_REQUESTS: AtomicUsize = AtomicUsize::new(0);

/// How many sources held the graceful stop open at the loop's last look —
/// what the SECOND request's sentence reads, on the signal thread.
static HOLDING_THE_STOP: AtomicUsize = AtomicUsize::new(0);

/// A request to stop the program GRACEFULLY — what a termination signal means
/// to a native program that serves (F45; Order 43's R-i).
///
/// The first request is the graceful stop: at its next poll every server
/// stops accepting, closes the connections that are only waiting, and lets a
/// request already being answered finish; once no server holds the stop open
/// ([`IoSource::holds_the_stop`]) the event loop ends and what runs at process
/// end runs — the leak census among it. It ends WHATEVER ELSE is pending
/// (F157): a detached `async { for { ..; sleep(3600000) } }`, a timer, a task
/// still awaiting — node, with no handler, dies of the signal and abandons
/// them all, so a native program that kept running for them would be one that
/// SIGTERM no longer stops. A `main` still suspended ends without its value
/// ([`block_on`]). The SECOND request ends the process at once, with exit code
/// 1 and a sentence on stderr — for a server whose open response never ends
/// (a stream, a long poll), or a program busy in a synchronous stretch that
/// never reaches the loop.
///
/// Safe to call from any thread, which is the point: a signal handler's
/// thread is not the program's.
pub fn request_termination() {
    if TERMINATION_REQUESTS.fetch_add(1, Ordering::SeqCst) > 0 {
        if HOLDING_THE_STOP.load(Ordering::SeqCst) > 0 {
            eprintln!(
                "the program was stopped by a second termination request before its open \
                 connections ended"
            );
        } else {
            eprintln!(
                "the program was stopped by a second termination request before it reached its \
                 event loop to stop"
            );
        }
        std::process::exit(1);
    }
    // F160: a loop blocked in its readiness wait hears the bell; one asleep
    // on a timer, or ticking, is unparked; a hosted one's host is woken (R8).
    if let Some(program_loop) = PROGRAM_LOOP.get() {
        program_loop.wake();
    }
}

/// Whether a termination request has arrived.
pub fn termination_requested() -> bool {
    TERMINATION_REQUESTS.load(Ordering::SeqCst) > 0
}

/// Whether the loop must end now: a termination request is in and no
/// registered source holds the stop open. Publishes how many still do.
fn stopped() -> bool {
    if !termination_requested() {
        return false;
    }
    let holding = RUNTIME.with(|runtime| {
        runtime
            .io
            .borrow()
            .iter()
            .filter(|source| source.holds_the_stop())
            .count()
    });
    HOLDING_THE_STOP.store(holding, Ordering::SeqCst);
    holding == 0
}

/// The unwind payload a `main` still suspended at a termination's end leaves
/// [`block_on`] with: there is no value to answer, and the stop is not a
/// failure. [`crate::main_guard`] reads it as an ordinary end.
pub struct TerminationStop;

/// `async <body>` — the spawn. Eager: the body runs to its first suspension
/// inside this call, as `this.promise = run()` does.
pub fn spawn<T: 'static>(body: impl Future<Output = T> + 'static, origin: &str) -> Task<T> {
    spawn_in(body, origin, None)
}

/// The spawn a nursery's dynamic extent produces: the task registers with the
/// nursery for its join.
pub fn spawn_in<T: 'static>(
    body: impl Future<Output = T> + 'static,
    origin: &str,
    nursery: Option<Nursery>,
) -> Task<T> {
    let value: Rc<RefCell<Option<T>>> = Rc::new(RefCell::new(None));
    let slot = Rc::clone(&value);
    let node = Rc::new(TaskNode {
        origin: crate::str_new(origin),
        driver: RefCell::new(Some(Box::pin(async move {
            let produced = body.await;
            *slot.borrow_mut() = Some(produced);
        }))),
        settled: Cell::new(false),
        failure: RefCell::new(None),
        observed: Cell::new(false),
        owned: nursery.is_some(),
        nursery: nursery.clone(),
        waiters: RefCell::new(Vec::new()),
        reported: Cell::new(false),
    });
    // The child is registered BEFORE the first poll, so a task that fails at its
    // very first suspension already has a nursery to notify.
    if let Some(nursery) = &nursery {
        nursery.0.children.borrow_mut().push(Rc::clone(&node));
    }
    let id = register_task(Rc::clone(&node));
    poll_task(id);
    Task { node, value }
}

/// `queueMicrotask(callback)` — `std::reactive`'s continuation-settling
/// primitive (F20).
///
/// A DEFERRED callback, which is the whole point of it: `enqueue`'s settled path
/// schedules one drain per segment and the callback must not run inside the
/// write that scheduled it. The microtask queue holds task ids rather than
/// closures, so the callback becomes a one-suspension task: the eager first poll
/// parks it on the queue, and the drain runs the body. A panicking callback
/// settles that task as a failure, which is reported once with its origin —
/// node's uncaught-exception path for the same callback.
pub fn queue_microtask(callback: impl Fn() + 'static) {
    spawn(
        async move {
            YieldOnce { yielded: false }.await;
            callback();
        },
        "queueMicrotask",
    );
}

/// One microtask hop — what JavaScript's `await` always costs and Rust's
/// `.await` never does (F20).
///
/// `await p` in JS queues the continuation on the microtask queue even when `p`
/// is already resolved; `future.await` in Rust continues in the same poll when
/// the future is ready. So an `async fun` that suspends nowhere ran to
/// completion INSIDE its spawn natively and after the enclosing sync body on the
/// JS backend — `reactive-turns.vl` prints `a -> 5` on the wrong side of
/// `end-sync` without this. The emitter spends one of these at every `.await`
/// it writes, which is the hop the JS backend spends there too.
pub async fn yield_now() {
    YieldOnce { yielded: false }.await
}

/// Pending exactly once, re-enqueueing the polling task — the "be a microtask"
/// future. Nothing else in this file needs it, because every other leaf parks on
/// an object that owns a wake list.
struct YieldOnce {
    yielded: bool,
}

impl Future for YieldOnce {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<()> {
        if self.yielded {
            return Poll::Ready(());
        }
        self.yielded = true;
        if let Some(task) = current_task() {
            enqueue(task);
        }
        Poll::Pending
    }
}

/// The event loop a program runs AFTER its synchronous body returns (F20).
///
/// node does not exit when the module's top level finishes; it exits when the
/// loop has nothing left. A SYNCHRONOUS `fun main` can still leave work behind —
/// `std::reactive`'s late-write path calls [`queue_microtask`] — and dropping it
/// would be a native program that prints less than the JS one. So an emitted
/// sync `main` ends with this, which is [`block_on`]'s loop without a root task
/// and a no-op for a program that queued nothing.
///
/// The loop is [`drive`]'s, the one a hosted [`turn`] runs too, so it takes
/// [`advance_macrotasks`]' step rather than only the deadline list's (F18): a
/// synchronous `fun main` that bound a SERVER left an I/O source behind, not a
/// timer, and a loop that only asked the deadline list saw nothing to do and
/// let the process exit with the listener still open — the socket announced
/// its port and then refused every connection.
pub fn run_pending() {
    run_owned(None);
}

/// The owned loop: [`drive`] in the mode that blocks, to the loop's end.
fn run_owned(root: Option<&Rc<TaskNode>>) {
    match drive(root, Mode::Owned) {
        Step::Ended => {}
        Step::Ran | Step::Idle(_) => unreachable!("an owned loop blocks rather than idling"),
    }
}

/// `async fun main` — run `body` as the root task and drive the loop until both
/// the microtask queue and the deadline list are empty (§10 (2)), the way node
/// exits when its event loop has nothing left to do.
///
/// A FAILING root stops the loop there and raises: `main()` at the top of an
/// emitted module is an unhandled rejection, and node takes the process down at
/// that point rather than running the timers that were still outstanding.
///
/// The owned host of the same root a [`Program`] gives a hosted loop (R8).
pub fn block_on<T: Clone + 'static>(body: impl Future<Output = T> + 'static) -> T {
    let program = Program::start(body);
    run_owned(Some(&program.root.node));
    program.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A recorder the ordering tests write into, so each one asserts on a
    /// SEQUENCE rather than on a final value.
    #[derive(Clone, Default)]
    struct Log(Rc<RefCell<Vec<String>>>);

    impl Log {
        fn push(&self, entry: &str) {
            self.0.borrow_mut().push(entry.to_string());
        }
        fn taken(&self) -> Vec<String> {
            self.0.borrow().clone()
        }
    }

    // -- rule 1: a spawn is EAGER (`this.promise = run()`) --

    #[test]
    fn a_spawn_runs_to_its_first_suspension_inside_the_spawn_expression() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            let inner = outer.clone();
            let task = spawn(
                async move {
                    inner.push("before the suspension");
                    sleep(1, None).await;
                    inner.push("after the suspension");
                },
                "test",
            );
            outer.push("after the spawn expression");
            task.await;
            outer.push("after the join");
        });
        assert_eq!(
            log.taken(),
            vec![
                "before the suspension",
                "after the spawn expression",
                "after the suspension",
                "after the join",
            ]
        );
    }

    // -- rule 2: microtasks drain to EXHAUSTION before the next timer --

    #[test]
    fn a_timers_continuation_chain_runs_whole_before_the_next_timer() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            // The head's timer is registered FIRST and the tail's second, both
            // at 0 ms, so the head fires first. Between the two macrotasks the
            // microtask queue must drain to exhaustion — which is the whole
            // three-link chain hanging off the head.
            let head_log = outer.clone();
            let head = spawn(
                async move {
                    sleep(0, None).await;
                    head_log.push("head");
                },
                "test",
            );
            let tail_log = outer.clone();
            let tail = spawn(
                async move {
                    sleep(0, None).await;
                    tail_log.push("tail");
                },
                "test",
            );
            let mut previous = head;
            for step in 1..4 {
                let chain_log = outer.clone();
                let waited = previous.clone();
                previous = spawn(
                    async move {
                        waited.await;
                        chain_log.push(&format!("chain {step}"));
                    },
                    "test",
                );
            }
            previous.await;
            tail.await;
        });
        assert_eq!(
            log.taken(),
            vec!["head", "chain 1", "chain 2", "chain 3", "tail"]
        );
    }

    // -- rule 3: the deadline list is ordered by deadline, then registration --

    #[test]
    fn timers_fire_by_deadline_and_two_of_one_delay_fire_in_registration_order() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            let mut tasks = Vec::new();
            for (label, delay) in [("late", 12), ("early", 1), ("same-a", 6), ("same-b", 6)] {
                let recorder = outer.clone();
                tasks.push(spawn(
                    async move {
                        sleep(delay, None).await;
                        recorder.push(label);
                    },
                    "test",
                ));
            }
            for task in tasks {
                task.await;
            }
        });
        assert_eq!(log.taken(), vec!["early", "same-a", "same-b", "late"]);
    }

    // -- rule 4: the join drains a list that GROWS mid-drain --

    #[test]
    fn the_join_waits_for_a_grandchild_spawned_while_the_drain_was_running() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            let nursery = nursery_new(None);
            let joined = nursery.clone();
            let body_log = outer.clone();
            let value = nursery_run(
                nursery.clone(),
                Box::pin(async move {
                    let child_log = body_log.clone();
                    let grandparent = joined.clone();
                    spawn_in(
                        async move {
                            sleep(2, None).await;
                            let grandchild_log = child_log.clone();
                            spawn_in(
                                async move {
                                    sleep(2, None).await;
                                    grandchild_log.push("grandchild");
                                },
                                "test",
                                Some(grandparent),
                            );
                            child_log.push("child");
                        },
                        "test",
                        Some(joined),
                    );
                    body_log.push("body");
                    7
                }),
            )
            .await;
            assert_eq!(value, 7);
            outer.push("joined");
        });
        assert_eq!(log.taken(), vec!["body", "child", "grandchild", "joined"]);
    }

    // -- rule 5: the join's failure reaction --

    #[test]
    fn a_failing_child_aborts_the_extent_and_the_winner_carries_its_origin() {
        let log = Log::default();
        let outer = log.clone();
        let outcome = crate::guarded(move || {
            block_on(async move {
                let nursery = nursery_new(None);
                let joined = nursery.clone();
                let body_log = outer.clone();
                nursery_run(
                    nursery.clone(),
                    Box::pin(async move {
                        let slow_log = body_log.clone();
                        let slow_signal = joined.signal();
                        spawn_in(
                            async move {
                                sleep(60, Some(slow_signal)).await;
                                slow_log.push("the slow sibling finished");
                            },
                            "the slow sibling",
                            Some(joined.clone()),
                        );
                        let quick_log = body_log.clone();
                        spawn_in(
                            async move {
                                sleep(1, None).await;
                                quick_log.push("the quick sibling failed");
                                crate::panic_with("boom");
                            },
                            "line 12",
                            Some(joined),
                        );
                    }),
                )
                .await;
            })
        });
        assert_eq!(
            outcome.err().map(|message| message.to_string()),
            Some("boom (in task spawned in line 12)".to_string())
        );
        // The slow sibling was cancelled by the failure's abort, so its own line
        // never ran: the 60 ms timer was cleared rather than waited out.
        assert_eq!(log.taken(), vec!["the quick sibling failed"]);
    }

    #[test]
    fn the_bodys_own_failure_wins_over_a_latched_child() {
        let outcome = crate::guarded(|| {
            block_on(async {
                let nursery = nursery_new(None);
                let joined = nursery.clone();
                nursery_run(
                    nursery.clone(),
                    Box::pin(async move {
                        spawn_in(
                            async {
                                sleep(1, None).await;
                                crate::panic_with("the child")
                            },
                            "a child",
                            Some(joined),
                        );
                        crate::panic_with("the body")
                    }),
                )
                .await;
            })
        });
        assert_eq!(
            outcome.err().map(|message| message.to_string()),
            Some("the body".to_string())
        );
    }

    // -- rule 6: a cancellation is an ECHO, never a winner --

    #[test]
    fn a_cancelled_sleep_never_becomes_the_nurserys_failure() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            let nursery = nursery_new(None);
            let joined = nursery.clone();
            let body_log = outer.clone();
            let value = nursery_run(
                nursery.clone(),
                Box::pin(async move {
                    let cancelled_log = body_log.clone();
                    let signal = joined.signal();
                    spawn_in(
                        async move {
                            sleep(60, Some(signal)).await;
                            cancelled_log.push("the sleep finished");
                        },
                        "a cancelled child",
                        Some(joined.clone()),
                    );
                    joined.cancel();
                    body_log.push("body");
                    crate::str_new("the body's value")
                }),
            )
            .await;
            assert_eq!(value.to_string(), "the body's value");
            outer.push("joined");
        });
        assert_eq!(log.taken(), vec!["body", "joined"]);
    }

    #[test]
    fn a_child_nursery_cancels_when_its_parent_does() {
        let parent = nursery_new(None);
        let child = nursery_new(Some(parent.clone()));
        let grandchild = nursery_new(Some(child.clone()));
        assert!(!grandchild.is_cancelled());
        parent.cancel();
        assert!(child.is_cancelled());
        assert!(grandchild.is_cancelled());
        // A nursery created UNDER an already-cancelled parent is cancelled at
        // birth — `if (signal.aborted) this.controller.abort(..)`.
        assert!(nursery_new(Some(parent)).is_cancelled());
    }

    // -- rule 7: a detached nursery reports rather than latching --

    #[test]
    fn a_detached_nursery_does_not_let_one_child_cancel_another() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            let owner = nursery_new_detached();
            let failing_log = outer.clone();
            spawn_in(
                async move {
                    sleep(1, None).await;
                    failing_log.push("the failing child");
                    crate::panic_with("boom")
                },
                "a detached child",
                Some(owner.clone()),
            );
            let surviving_log = outer.clone();
            let sibling = spawn_in(
                async move {
                    sleep(4, None).await;
                    surviving_log.push("the sibling survived");
                },
                "a detached sibling",
                Some(owner.clone()),
            );
            sibling.await;
            assert!(!owner.is_cancelled());
        });
        assert_eq!(
            log.taken(),
            vec!["the failing child", "the sibling survived"]
        );
    }

    // -- rule 8: `Timer`'s memoized verdict --

    #[test]
    fn a_timers_verdict_is_memoized_and_the_first_settlement_wins() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            let fired = timer(1);
            assert!(fired.wait(None).await);
            // Past settlement the answer comes from the memo rather than from a
            // second timer, so this resolves with no turn of the loop.
            assert!(fired.wait(None).await);
            outer.push("fired");

            let cancelled = timer(60);
            cancelled.cancel();
            assert!(!cancelled.wait(None).await);
            // A cancel after the verdict settled changes nothing.
            cancelled.cancel();
            assert!(!cancelled.wait(None).await);
            outer.push("cancelled");
        });
        assert_eq!(log.taken(), vec!["fired", "cancelled"]);
    }

    #[test]
    fn an_aborted_wait_leaves_the_timer_running_for_its_other_holders() {
        block_on(async {
            let handle = timer(3);
            let nursery = nursery_new(None);
            let waiting = handle.clone();
            let signal = nursery.signal();
            let aborted = spawn(
                async move {
                    waiting.wait(Some(signal)).await;
                },
                "an aborted waiter",
            );
            nursery.cancel();
            // The abort rejected THAT waiter and left the verdict unsettled, so
            // another holder still sees the timer fire.
            assert!(handle.wait(None).await);
            assert!(aborted.is_settled());
        });
    }

    // -- rule 9: an unowned unobserved failure is reported ONCE --

    #[test]
    fn an_unowned_unobserved_failure_is_reported_once_with_its_origin() {
        block_on(async {
            let dropped = spawn(
                async {
                    sleep(1, None).await;
                    crate::panic_with("nobody is listening")
                },
                "line 40",
            );
            // Nothing awaits it. The report goes to stderr (as `console.error`
            // does), so stdout — what the differential compares — is untouched.
            sleep(8, None).await;
            assert!(dropped.is_settled());
            assert!(dropped.node.reported.get());
        });
    }

    #[test]
    fn an_awaited_failure_is_never_reported_as_unobserved() {
        let outcome = crate::guarded(|| {
            block_on(async {
                let task: Task<()> = spawn(
                    async {
                        sleep(1, None).await;
                        crate::panic_with("observed")
                    },
                    "line 9",
                );
                assert!(!task.node.reported.get());
                task.await;
            })
        });
        assert_eq!(
            outcome.err().map(|message| message.to_string()),
            Some("observed".to_string())
        );
    }

    // -- rule 10: the loop exits when BOTH queues are empty --

    #[test]
    fn the_loop_runs_a_pending_timer_the_root_task_never_awaited() {
        let log = Log::default();
        let outer = log.clone();
        block_on(async move {
            let background_log = outer.clone();
            spawn(
                async move {
                    sleep(3, None).await;
                    background_log.push("the background task finished");
                },
                "test",
            );
            outer.push("the root task returned");
        });
        assert_eq!(
            log.taken(),
            vec!["the root task returned", "the background task finished"]
        );
    }

    // -- rule 11: the two joins --

    #[test]
    fn settle_all_preserves_order_whatever_the_delays_are() {
        let values = block_on(async {
            let tasks = vec![
                spawn(
                    async {
                        sleep(9, None).await;
                        crate::str_new("a")
                    },
                    "test",
                ),
                spawn(
                    async {
                        sleep(1, None).await;
                        crate::str_new("b")
                    },
                    "test",
                ),
                spawn(
                    async {
                        sleep(5, None).await;
                        crate::str_new("c")
                    },
                    "test",
                ),
            ];
            settle_all(tasks).await
        });
        assert_eq!(
            values
                .iter()
                .map(|value| value.to_string())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn settle_all_raises_the_failure_of_a_task_that_is_not_the_first() {
        let outcome = crate::guarded(|| {
            block_on(async {
                let tasks: Vec<Task<Str>> = vec![
                    spawn(
                        async {
                            sleep(60, None).await;
                            crate::str_new("the slow one")
                        },
                        "test",
                    ),
                    spawn(
                        async {
                            sleep(1, None).await;
                            crate::panic_with("the quick one failed")
                        },
                        "test",
                    ),
                ];
                settle_all(tasks).await;
            })
        });
        assert_eq!(
            outcome.err().map(|message| message.to_string()),
            Some("the quick one failed".to_string())
        );
    }

    #[test]
    fn race_answers_the_first_task_to_settle_and_leaves_the_losers_running() {
        let log = Log::default();
        let outer = log.clone();
        let winner = block_on(async move {
            let loser_log = outer.clone();
            let loser = spawn(
                async move {
                    sleep(8, None).await;
                    loser_log.push("the loser kept running");
                    crate::str_new("slow")
                },
                "test",
            );
            let quick = spawn(
                async {
                    sleep(1, None).await;
                    crate::str_new("quick")
                },
                "test",
            );
            let answer = race(vec![quick, loser.clone()]).await;
            // The loser is neither cancelled nor abandoned: the loop keeps
            // draining it, which is what `Promise.race` leaves behind.
            loser.await;
            answer
        });
        assert_eq!(winner.to_string(), "quick");
        assert_eq!(log.taken(), vec!["the loser kept running"]);
    }

    // -- rule 12: `with_finally_async` --

    #[test]
    fn with_finally_async_runs_after_a_failure_and_keeps_it_travelling() {
        let log = Log::default();
        let outer = log.clone();
        let recorded = outer.clone();
        let outcome = crate::guarded(move || {
            block_on(async move {
                let body_log = recorded.clone();
                let after_log = recorded.clone();
                with_finally_async(
                    Box::pin(async move {
                        sleep(1, None).await;
                        body_log.push("body");
                        crate::panic_with("boom")
                    }),
                    move || after_log.push("after"),
                )
                .await;
            })
        });
        assert_eq!(
            outcome.err().map(|message| message.to_string()),
            Some("boom".to_string())
        );
        assert_eq!(log.taken(), vec!["body", "after"]);
    }

    // -- rule 13: the slab's free list and its generation counter (F24) --

    /// A node that is already settled, for the slab tests: they are about the
    /// SLOT bookkeeping, not about driving a body.
    fn settled_node(origin: &str) -> Rc<TaskNode> {
        Rc::new(TaskNode {
            origin: crate::str_new(origin),
            driver: RefCell::new(None),
            settled: Cell::new(true),
            failure: RefCell::new(None),
            observed: Cell::new(true),
            owned: false,
            nursery: None,
            waiters: RefCell::new(Vec::new()),
            reported: Cell::new(false),
        })
    }

    /// F24's claim: a settled task's slot is REUSED, so a program that spawns
    /// per unit of work does not grow the slab one word per spawn for its
    /// lifetime.
    ///
    /// Two hundred rounds, each spawning one task, awaiting it and reading the
    /// slab's length. The high-water mark is what the assertion is about: it was
    /// `rounds + 1` before the free list existed, because a reaped slot stayed
    /// `None` forever. The bound is deliberately loose (there is one round of
    /// lag — a task settles inside a microtask drain and its slot is freed at
    /// the top of the NEXT turn), and it is two orders of magnitude below the
    /// unbounded number, which is what makes it non-vacuous.
    #[test]
    fn a_settled_tasks_slot_is_reused_so_repeated_spawns_keep_the_slab_bounded() {
        let rounds = 200;
        let high_water = Rc::new(Cell::new(0usize));
        let observed = Rc::clone(&high_water);
        block_on(async move {
            for _ in 0..rounds {
                let task = spawn(
                    async {
                        sleep(0, None).await;
                    },
                    "test",
                );
                task.await;
                observed.set(observed.get().max(slab_slots()));
            }
        });
        assert!(
            high_water.get() <= 8,
            "{rounds} spawn/settle rounds left the slab at {} slots; the free list is not \
             reclaiming them",
            high_water.get()
        );
    }

    /// The guarantee the never-reused slab gave for free, and the one thing a
    /// free list could have taken away: a handle to a task that has been reaped
    /// must not name whatever task takes its slot next.
    ///
    /// The slot really IS reused (the index is the same one), and the
    /// generation is what makes the old handle stop naming it — checked both
    /// before and after the reuse, because a check only after it would pass on
    /// an implementation that merely left the slot empty.
    #[test]
    fn a_reaped_handle_does_not_name_the_task_that_reuses_its_slot() {
        let first = settled_node("first");
        let first_id = register_task(Rc::clone(&first));
        assert!(task_at(first_id).is_some(), "a fresh handle names its task");
        reap_settled();
        assert!(
            task_at(first_id).is_none(),
            "a reaped handle must name nothing"
        );

        let second = settled_node("second");
        let second_id = register_task(Rc::clone(&second));
        assert_eq!(
            second_id.index, first_id.index,
            "the freed slot must be the one the next spawn takes"
        );
        assert_ne!(
            second_id.generation, first_id.generation,
            "reuse must move the generation on"
        );
        assert!(
            task_at(first_id).is_none(),
            "the stale handle named the stranger that took its slot"
        );
        assert!(Rc::ptr_eq(
            &task_at(second_id).expect("the new handle names the new task"),
            &second
        ));
    }

    /// And a WAKE through a stale handle is a no-op rather than a poll of the
    /// stranger in that slot: `enqueue` takes handles off wake lists that
    /// outlive their tasks, and `poll_task` is where the check has to hold.
    #[test]
    fn a_wake_through_a_stale_handle_polls_nothing() {
        let first = settled_node("first");
        let first_id = register_task(Rc::clone(&first));
        reap_settled();
        let polled = Rc::new(Cell::new(false));
        let flag = Rc::clone(&polled);
        // A live task in the freed slot whose body records that it ran.
        let second = Rc::new(TaskNode {
            origin: crate::str_new("second"),
            driver: RefCell::new(Some(Box::pin(async move {
                flag.set(true);
            }))),
            settled: Cell::new(false),
            failure: RefCell::new(None),
            observed: Cell::new(true),
            owned: false,
            nursery: None,
            waiters: RefCell::new(Vec::new()),
            reported: Cell::new(false),
        });
        let second_id = register_task(Rc::clone(&second));
        assert_eq!(second_id.index, first_id.index);
        enqueue(first_id);
        drain_microtasks();
        assert!(
            !polled.get(),
            "a wake through a reaped task's handle polled the task that took its slot"
        );
        // The same wake through the RIGHT handle does run it, so the no-op above
        // is the generation check and not a dead queue.
        enqueue(second_id);
        drain_microtasks();
        assert!(polled.get(), "the live handle must still wake its task");
    }

    // -- F146: a SOLE source that can block waits for its work itself --

    /// A source that records the limit every `block` was asked for.
    struct Recorder {
        limits: Rc<RefCell<Vec<Option<Duration>>>>,
        blocks: bool,
    }

    impl IoSource for Recorder {
        fn poll(&self) -> bool {
            false
        }

        fn block(&self, limit: Option<Duration>) -> bool {
            self.limits.borrow_mut().push(limit);
            self.blocks
        }

        fn is_live(&self) -> bool {
            true
        }
    }

    fn recorder(blocks: bool) -> (Rc<Recorder>, Rc<RefCell<Vec<Option<Duration>>>>) {
        let limits = Rc::new(RefCell::new(Vec::new()));
        let source = Rc::new(Recorder {
            limits: Rc::clone(&limits),
            blocks,
        });
        (source, limits)
    }

    #[test]
    fn f146_a_sole_blocking_source_waits_until_the_next_timer_deadline() {
        let _timer = register_timer(40, || {});
        let (source, limits) = recorder(true);
        register_io(source);
        assert!(
            matches!(advance_macrotasks(Mode::Owned), Step::Ran),
            "a turn that waited is a turn"
        );
        let limits = limits.borrow();
        assert_eq!(limits.len(), 1, "the sole source was asked to wait once");
        let limit = limits[0].expect("a timer is due, so the wait is bounded by it");
        assert!(
            limit <= Duration::from_millis(40),
            "the wait ends by the deadline: {limit:?}"
        );
    }

    #[test]
    fn f146_a_sole_blocking_source_with_no_timer_waits_unbounded() {
        let (source, limits) = recorder(true);
        register_io(source);
        assert!(matches!(advance_macrotasks(Mode::Owned), Step::Ran));
        assert_eq!(*limits.borrow(), [None], "no deadline: as long as it takes");
    }

    #[test]
    fn f146_two_live_sources_never_block_on_either() {
        // Neither could wake the loop for the other's work, so the turn
        // sleeps the poll interval as before.
        let (first, first_limits) = recorder(true);
        let (second, second_limits) = recorder(true);
        register_io(first);
        register_io(second);
        assert!(matches!(advance_macrotasks(Mode::Owned), Step::Ran));
        assert!(first_limits.borrow().is_empty());
        assert!(second_limits.borrow().is_empty());
    }

    #[test]
    fn f146_a_source_that_cannot_block_keeps_the_poll_interval() {
        // `block`'s default answer: the loop falls back to its sleep.
        let (source, limits) = recorder(false);
        register_io(source);
        assert!(matches!(advance_macrotasks(Mode::Owned), Step::Ran));
        assert_eq!(limits.borrow().len(), 1, "asked once, declined");
    }

    // -- F160: the interest a readiness wait is built from --

    #[test]
    fn f160_an_interest_keeps_the_nearest_deadline_after_the_turn_began() {
        let turn = Instant::now();
        let mut interest = Interest::new(turn);
        interest.wake_at(turn);
        assert_eq!(
            interest.deadline, None,
            "the turn's own poll saw that moment"
        );
        interest.wake_at(turn + Duration::from_millis(50));
        interest.wake_at(turn + Duration::from_millis(20));
        interest.wake_at(turn + Duration::from_millis(80));
        assert_eq!(interest.deadline, Some(turn + Duration::from_millis(20)));
    }

    #[test]
    fn f160_a_socket_watched_for_nothing_is_left_out() {
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).expect("bind");
        let mut interest = Interest::new(Instant::now());
        interest.watch(&socket, false, false);
        assert!(interest.watches.is_empty());
        interest.watch(&socket, true, false);
        assert_eq!(
            interest.watches,
            [Watch {
                socket: socket.raw_socket(),
                readable: true,
                writable: false,
                ready: false,
            }]
        );
    }

    /// No wait installed — this crate's own tests never install one — so a
    /// source that describes itself still ticks, and a waker made here only
    /// unparks: the fallback the readiness wait stands on.
    #[test]
    fn f160_without_a_readiness_wait_the_loop_ticks_and_a_waker_unparks() {
        struct Describing;
        impl IoSource for Describing {
            fn poll(&self) -> bool {
                false
            }
            fn interest(&self, _interest: &mut Interest) -> bool {
                true
            }
            fn is_live(&self) -> bool {
                true
            }
        }
        assert!(READINESS.get().is_none());
        assert!(
            loop_waker().signal.rope.get().is_none(),
            "no bell without a wait"
        );
        register_io(Rc::new(Describing));
        let before = turns();
        let started = Instant::now();
        assert!(
            matches!(advance_macrotasks(Mode::Owned), Step::Ran),
            "a tick is a turn"
        );
        assert_eq!(turns(), before + 1);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "one tick, not a block: {:?}",
            started.elapsed()
        );
    }
}
