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
use std::pin::Pin;
use std::rc::{Rc, Weak};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use crate::{Str, describe_panic};

/// A task's index in the runtime's slab. Slots are never REUSED (a settled
/// task's slot becomes `None` and stays `None`), so a wake list still holding
/// the id of a task that has since been reaped wakes nothing rather than waking
/// a stranger.
type TaskId = usize;

/// The type-erased driver of one task.
type Driver = Pin<Box<dyn Future<Output = ()>>>;

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
    /// A vilan `panic`, or any other Rust panic raised inside the task.
    Panic(Str),
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
        Failure::Panic(message) => crate::panic_with(&message),
    }
}

fn classify(payload: &Box<dyn std::any::Any + Send>) -> Failure {
    if payload.downcast_ref::<Cancellation>().is_some() {
        Failure::Cancelled
    } else {
        Failure::Panic(describe_panic(payload))
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
        if let Some(Failure::Panic(message)) = self.failure.borrow().as_ref() {
            eprintln!(
                "unhandled task error (spawned in {}): {message}",
                self.origin
            );
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
    inner: Pin<Box<dyn Future<Output = T>>>,
    done: bool,
}

impl<T> Catch<T> {
    fn new(inner: Pin<Box<dyn Future<Output = T>>>) -> Self {
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

    fn park(&self) {
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

/// `__nursery_run(n, body)` — the join, reproduced exactly.
///
/// Run the body, then drain the children; the child list may GROW while
/// draining (a running child spawns a grandchild), so the loop re-reads it every
/// turn rather than snapshotting its length. Failure reaction is at settle time:
/// a failing owned task has already latched itself through [`Nursery::fail`] and
/// aborted the signal, and the drain races every child against the fail-wake. On
/// failure every remaining child is absorbed, the body's error wins if there is
/// one, and a latched winner carries its spawn origin into the message.
pub async fn nursery_run<T>(nursery: Nursery, body: Pin<Box<dyn Future<Output = T>>>) -> T {
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
        Some(Failure::Panic(message)) => {
            crate::panic_with(&format!("{message} (in task spawned in {})", winner.origin))
        }
        // `__fail` is only ever called for a non-cancellation error, so a
        // cancelled winner cannot arise; raising the cancellation is the honest
        // answer if it ever did.
        Some(Failure::Cancelled) | None => std::panic::panic_any(Cancellation),
    }
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
pub async fn with_finally_async(body: Pin<Box<dyn Future<Output = ()>>>, after: impl FnOnce()) {
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
    tasks: RefCell<Vec<Option<Rc<TaskNode>>>>,
    microtasks: RefCell<VecDeque<TaskId>>,
    timers: RefCell<Vec<Rc<TimerSlot>>>,
    current: Cell<Option<TaskId>>,
    sequence: Cell<u64>,
}

thread_local! {
    static RUNTIME: Runtime = const {
        Runtime {
            tasks: RefCell::new(Vec::new()),
            microtasks: RefCell::new(VecDeque::new()),
            timers: RefCell::new(Vec::new()),
            current: Cell::new(None),
            sequence: Cell::new(0),
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

fn register_task(node: Rc<TaskNode>) -> TaskId {
    RUNTIME.with(|runtime| {
        let mut tasks = runtime.tasks.borrow_mut();
        tasks.push(Some(node));
        tasks.len() - 1
    })
}

fn task_at(id: TaskId) -> Option<Rc<TaskNode>> {
    RUNTIME.with(|runtime| runtime.tasks.borrow().get(id).cloned().flatten())
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
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        driver.as_mut().poll(&mut context)
    }));
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
    let nodes: Vec<Rc<TaskNode>> =
        RUNTIME.with(|runtime| runtime.tasks.borrow().iter().flatten().cloned().collect());
    for node in nodes {
        if node.settled.get() && !node.observed.get() && !node.owned {
            node.report_unobserved();
        }
    }
}

/// Settled tasks leave the slab. Their slots are NOT reused (see [`TaskId`]) and
/// every `Task` handle keeps its own `Rc`, so this drops the RUNTIME's share of
/// a finished task rather than growing the live set for the program's lifetime.
/// It runs after [`report_unobserved_failures`], so nothing is reaped before its
/// report was decided.
fn reap_settled() {
    RUNTIME.with(|runtime| {
        for slot in runtime.tasks.borrow_mut().iter_mut() {
            if slot.as_ref().is_some_and(|node| node.settled.get()) {
                *slot = None;
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
/// Answers whether anything fired — `false` means the deadline list is empty and
/// the loop is done.
fn advance_timers() -> bool {
    let earliest = RUNTIME.with(|runtime| {
        let mut timers = runtime.timers.borrow_mut();
        timers.retain(|slot| !slot.cancelled.get() && !slot.fired.get());
        timers
            .iter()
            .min_by_key(|slot| (slot.deadline, slot.sequence))
            .cloned()
    });
    let Some(slot) = earliest else {
        return false;
    };
    let now = Instant::now();
    if slot.deadline > now {
        std::thread::sleep(slot.deadline - now);
    }
    slot.fired.set(true);
    let action = slot.action.borrow_mut().take();
    if let Some(action) = action {
        action();
    }
    true
}

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

/// `async fun main` — run `body` as the root task and drive the loop until both
/// the microtask queue and the deadline list are empty (§10 (2)), the way node
/// exits when its event loop has nothing left to do.
///
/// A FAILING root stops the loop there and raises: `main()` at the top of an
/// emitted module is an unhandled rejection, and node takes the process down at
/// that point rather than running the timers that were still outstanding.
pub fn block_on<T: Clone + 'static>(body: impl Future<Output = T> + 'static) -> T {
    let root = spawn(body, "top level");
    loop {
        drain_microtasks();
        report_unobserved_failures();
        if let Some(failure) = root.node.failure.borrow().clone() {
            raise(failure);
        }
        reap_settled();
        if !advance_timers() {
            break;
        }
    }
    root.value
        .borrow()
        .clone()
        .expect("the root task settled without a value")
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
}
