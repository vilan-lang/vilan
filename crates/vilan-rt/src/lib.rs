//! `vilan-rt` — the runtime for programs the vilan compiler emits as Rust
//! (tracker F1, slice S1a; `proposal/native-apps.md` §2.3 enumerates it).
//!
//! Nothing here is speculative. Every item is something `native-apps.md`'s probe
//! either used or had to hand-write, and the scope is the one S1a was ruled to:
//! structs, enums, `Option`/`Result`, `str`, `List`, `Map`/`Set`, closures,
//! `impl`s, `print`, `panic`, and the counted cell. **No UI, no rpc, no
//! filesystem, no platform surface** — those are later slices.
//!
//! Order 38 added the one exception to that list: [`executor`], the
//! single-threaded executor `async` code runs on (tracker J6, designed in
//! `native-apps.md` §10 and built against `transformer.rs::helper_source` as
//! its contract). It is a module rather than a crate because it links the same
//! way the rest of this runtime does and shares [`Str`], [`panic_with`] and the
//! panic-payload reading with it.
//!
//! # The contract this crate actually has to keep
//!
//! S1a's exit test is a DIFFERENTIAL: the same program compiled by the JS
//! backend and by the Rust one must print byte-identical stdout. So this crate
//! is not free to format values the way Rust would. `print` is `console.log`,
//! and its output is JavaScript's, down to `Infinity` rather than `inf` and `0`
//! rather than `-0`. [`Js`] is where that lives, and every divergence it papers
//! over is written down at the impl that papers it.
//!
//! # Dependencies: none, deliberately
//!
//! Order 37's R8. See this crate's `Cargo.toml` for the reasoning.

#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc;
use std::rc::Rc;

pub mod executor;

// ---------------------------------------------------------------- strings ---

/// `str` — immutable and value-copied, so a rule-1 copy is a refcount bump
/// rather than a `String` clone. `native-apps.md` §2.3 (1) names the `String`
/// alternative "the single largest avoidable cost".
pub type Str = Rc<str>;

/// The emitter's spelling for a string literal.
pub fn str_new(text: &str) -> Str {
    Rc::from(text)
}

/// `a + b` on two strings.
pub fn str_concat(left: &str, right: &str) -> Str {
    let mut joined = String::with_capacity(left.len() + right.len());
    joined.push_str(left);
    joined.push_str(right);
    Rc::from(joined)
}

// ------------------------------------------------------- console.log-ness ---

/// How a value prints under `print(..)`.
///
/// This is JavaScript's `console.log`, not Rust's `Display`, and the difference
/// is the whole reason the trait exists: the differential compares BYTES. Two
/// renderings per type, because node prints a top-level string bare and a
/// nested one quoted (`print(s)` → `hello`, `print(xs)` → `[ 'hello' ]`).
pub trait Js {
    /// The rendering at the top level of a `print`.
    fn js(&self) -> String;
    /// The rendering INSIDE a container. Same as [`Js::js`] for everything
    /// except strings, which node quotes when nested.
    fn js_nested(&self) -> String {
        self.js()
    }
}

macro_rules! js_via_display {
    ($($type:ty),*) => {
        $(impl Js for $type {
            fn js(&self) -> String {
                self.to_string()
            }
        })*
    };
}

// The integer widths, including R6's: `i53`/`u53` are distinct vilan types with
// native widths `i64`/`u64` and the documented note that their RANGE guarantee
// is the JS one, so a program that round-trips both backends behaves the same.
js_via_display!(bool, i8, u8, i16, u16, i32, u32, i64, u64, usize, isize);

impl Js for f64 {
    fn js(&self) -> String {
        js_number(*self)
    }
}

impl Js for f32 {
    fn js(&self) -> String {
        js_number(*self as f64)
    }
}

impl Js for Str {
    fn js(&self) -> String {
        self.to_string()
    }
    fn js_nested(&self) -> String {
        format!("'{self}'")
    }
}

impl Js for str {
    fn js(&self) -> String {
        self.to_string()
    }
    fn js_nested(&self) -> String {
        format!("'{self}'")
    }
}

impl Js for () {
    fn js(&self) -> String {
        "undefined".to_string()
    }
}

/// A TUPLE renders as the array it is on the JS backend — a vilan tuple and a
/// vilan struct are both flat arrays there, so `print((a, b))` is `[ a, b ]`
/// with node's spacing. Written for the arities a program reaches; a wider one
/// is a refusal in the emitter rather than a silently different rendering.
macro_rules! js_for_tuple {
    ($($name:ident),+) => {
        impl<$($name: Js),+> Js for ($($name,)+) {
            fn js(&self) -> String {
                #[allow(non_snake_case, reason = "the binders are the type parameters' own names")]
                let ($($name,)+) = self;
                js_tuple(&[$($name.js_nested()),+])
            }
        }
    };
}

js_for_tuple!(A);
js_for_tuple!(A, B);
js_for_tuple!(A, B, C);
js_for_tuple!(A, B, C, D);
js_for_tuple!(A, B, C, D, E);
js_for_tuple!(A, B, C, D, E, F);

impl<T: Js> Js for Vec<T> {
    fn js(&self) -> String {
        if self.is_empty() {
            // node prints an empty array as `[]`, with no inner space — the one
            // place the `[ a, b ]` spacing does not apply.
            return "[]".to_string();
        }
        let mut out = String::from("[ ");
        for (index, item) in self.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&item.js_nested());
        }
        out.push_str(" ]");
        out
    }
}

impl<T: Js> Js for Option<T> {
    /// An `Option` is a vilan ENUM, and an enum's runtime value on the JS
    /// backend is `[index, ...data]` — so `Some(5)` prints `[ 0, 5 ]` and `None`
    /// prints `[ 1 ]`. The differential is about bytes, and these are the bytes.
    fn js(&self) -> String {
        match self {
            Some(value) => format!("[ 0, {} ]", value.js_nested()),
            None => "[ 1 ]".to_string(),
        }
    }
}

impl<T: Js, E: Js> Js for Result<T, E> {
    fn js(&self) -> String {
        match self {
            Ok(value) => format!("[ 0, {} ]", value.js_nested()),
            Err(error) => format!("[ 1, {} ]", error.js_nested()),
        }
    }
}

/// The rendering an emitted aggregate uses: the JS backend's `[a, b]` array,
/// with node's spacing. An emitted `impl Js` for a struct or an enum calls this
/// with its already-rendered parts.
pub fn js_tuple(parts: &[String]) -> String {
    if parts.is_empty() {
        return "[]".to_string();
    }
    format!("[ {} ]", parts.join(", "))
}

impl<T: Js> Js for &T {
    fn js(&self) -> String {
        (*self).js()
    }
    fn js_nested(&self) -> String {
        (*self).js_nested()
    }
}

/// A value's own rendering as a `Str` — what an interpolation's non-string
/// half becomes. For the scalars, this IS `std::display`'s `render`.
pub fn js_of<T: Js + ?Sized>(value: &T) -> Str {
    Rc::from(value.js().as_str())
}

/// `print(message)` — `std::io`'s one universal output, bound to
/// `console.log` on the JS backend and to this here.
pub fn print<T: Js + ?Sized>(value: &T) {
    println!("{}", value.js());
}

/// ECMA-262's `Number::toString` for the cases a compiled program reaches.
///
/// Rust's own `{}` is already the shortest round-tripping decimal, which is
/// what JavaScript specifies too, so the body below is only the four places the
/// two disagree — and each is a real difference a corpus program can print, not
/// a hypothetical.
pub fn js_number(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        // Rust writes `inf` / `-inf`.
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if value == 0.0 {
        // JavaScript's `String(-0)` is `"0"`; Rust's is `"-0"`. The sign is
        // observable through `1/x`, and nothing in scope prints that.
        return "0".to_string();
    }
    let magnitude = value.abs();
    // JavaScript switches to exponential notation outside 1e-6 ..< 1e21; Rust
    // never does. Below that boundary the two agree exactly.
    if !(1e-6..1e21).contains(&magnitude) {
        return js_exponential(value);
    }
    let mut text = format!("{value}");
    // Rust prints `2` for 2.0 as well, so there is no `.0` to strip — but a
    // value formatted through `{:?}` would carry one, and a future caller
    // reaching for that spelling would silently break the differential. Keep the
    // strip so the function is correct for both inputs.
    if let Some(stripped) = text.strip_suffix(".0") {
        text = stripped.to_string();
    }
    text
}

/// The exponential half of [`js_number`]: `1e+21`, `1.5e-7`.
fn js_exponential(value: f64) -> String {
    let formatted = format!("{value:e}");
    // Rust writes `1e21` / `1.5e-7`; JavaScript writes `1e+21` / `1.5e-7` — the
    // POSITIVE exponent carries an explicit sign and the negative one does not.
    match formatted.split_once('e') {
        Some((mantissa, exponent)) if !exponent.starts_with('-') => {
            let mut out = String::new();
            let _ = write!(out, "{mantissa}e+{exponent}");
            out
        }
        _ => formatted,
    }
}

// ------------------------------------------------------------ panic paths ---

/// `panic(message)` — `std::io::panic`. The payload is a `String` so
/// [`guarded`] can hand the message back the way the JS `catch` does.
pub fn panic_with(message: &str) -> ! {
    std::panic::panic_any(message.to_string())
}

/// `__guarded` — run `body`, answering `Err(message)` if it panicked.
///
/// `native-apps.md` §2.3 (5): the JS helper is a `try`/`catch` returning
/// `[1]` or `[0, message]`, and `catch_unwind` plus a payload downcast is the
/// native shape. The hook is silenced for the duration so a caught panic does
/// not also print a backtrace banner the JS leg has no counterpart for — which
/// would not reach stdout, but does reach a reader of the differential's logs.
pub fn guarded(body: impl FnOnce()) -> Result<(), Str> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    std::panic::set_hook(previous);
    match outcome {
        Ok(()) => Ok(()),
        Err(payload) => Err(describe_panic(&payload)),
    }
}

/// `__with_finally` — `after` runs whether or not `body` panicked, and a panic
/// keeps travelling.
pub fn with_finally(body: impl FnOnce(), after: impl FnOnce()) {
    struct Guard<F: FnOnce()>(Option<F>);
    impl<F: FnOnce()> Drop for Guard<F> {
        fn drop(&mut self) {
            if let Some(after) = self.0.take() {
                after();
            }
        }
    }
    let _guard = Guard(Some(after));
    body();
}

fn describe_panic(payload: &Box<dyn std::any::Any + Send>) -> Str {
    if let Some(message) = payload.downcast_ref::<String>() {
        Rc::from(message.as_str())
    } else if let Some(message) = payload.downcast_ref::<&str>() {
        Rc::from(*message)
    } else {
        Rc::from("panicked")
    }
}

// --------------------------------------------------------------- the cell ---

/// `Shared<T>` — the counted box the JS backend spells `{ v: value }`.
///
/// S1a's cell, per the brief: `Rc<RefCell<..>>`, with C14 S4's real counting a
/// later slice. Cloning is another handle to the SAME cell, which is what makes
/// it the box a `SignalCell` is built out of.
pub struct Shared<T> {
    inner: Rc<RefCell<T>>,
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Shared {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Shared {
            inner: Rc::new(RefCell::new(value)),
        }
    }

    /// Reads the cell. Rule 4 is checked by *vilan*, and rustc sees only the
    /// emitted code — so a violated invariant is this borrow panicking with a
    /// message rather than a compile error. That is R3's ruled residue, and it
    /// is honest: it stops the program at the aliasing read instead of reading
    /// through it.
    pub fn get(&self) -> T
    where
        T: Clone,
    {
        self.inner.borrow().clone()
    }

    pub fn set(&self, value: T) {
        *self.inner.borrow_mut() = value;
    }

    /// `Shared::write()` USED AS A PLACE — `cell.write().push(x)`,
    /// `cell.write().field = y` (F20).
    ///
    /// On the JS backend `read()` and `write()` are the same property access
    /// (`cell.v`) and the difference is only what the program does with it; here
    /// they are two different things, because a read COPIES out of the cell
    /// (`get`) and a write has to reach the slot. `cell.write() = value` is the
    /// assignment form and the emitter renders it as [`Shared::set`]; every other
    /// use of the view is this borrow.
    ///
    /// A `RefCell` borrow, so an aliasing write — rule 4, which *vilan* checks
    /// and rustc cannot see — panics at the read instead of reading through it.
    /// That is R3's ruled residue and the same stance [`Shared::get`] takes.
    pub fn borrow_mut(&self) -> std::cell::RefMut<'_, T> {
        self.inner.borrow_mut()
    }

    /// The count, for the measurement C14 S4 will want and for tests here.
    pub fn strong_count(&self) -> usize {
        Rc::strong_count(&self.inner)
    }

    /// This CELL's identity — the same number for every handle to one cell,
    /// different for every other cell, stable for a run (`shared.vl`'s
    /// `identity`). The JS backend mints a counter per cell; here the cell's
    /// own address IS its identity, and it is narrowed to the `i53` range the
    /// language promises.
    pub fn identity(&self) -> i64 {
        (Rc::as_ptr(&self.inner) as usize as u64 & 0x1f_ffff_ffff_ffff) as i64
    }

    /// The back-edge handle (`shared.vl`'s `downgrade`): it names the cell and
    /// does not keep it alive.
    pub fn downgrade(&self) -> Weak<T> {
        Weak {
            inner: Rc::downgrade(&self.inner),
        }
    }
}

/// Reference equality, which is what `==` between two cells means on the JS
/// backend: a `Shared` is an object there, and two of them compare `===`, so
/// the answer is whether they are the SAME cell. Cloning a handle keeps the
/// answer `true`, which is the property the reactive code relies on.
impl<T> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.inner, &other.inner)
    }
}

/// A cell prints as the object the JS backend spells it with: `{ v: value }`,
/// with node's spacing.
impl<T: Js> Js for Shared<T> {
    fn js(&self) -> String {
        format!("{{ v: {} }}", self.inner.borrow().js_nested())
    }
}

/// `Weak<T>` — a handle that names a [`Shared`] cell without keeping it alive.
///
/// On the JS backend `downgrade` is the identity and `upgrade` is always
/// `Some`, because nothing counts there; here the count is real, so `upgrade`
/// answers `None` once the last strong handle is gone. `shared.vl`'s own note
/// says that is the shape landing ahead of the guarantee, and a program written
/// against the `Option` keeps working either way.
pub struct Weak<T> {
    inner: rc::Weak<RefCell<T>>,
}

impl<T> Clone for Weak<T> {
    fn clone(&self) -> Self {
        Weak {
            inner: rc::Weak::clone(&self.inner),
        }
    }
}

impl<T> Weak<T> {
    /// A strong handle to the cell, or `None` if the cell is gone. The `Some`
    /// case RETAINS.
    pub fn upgrade(&self) -> Option<Shared<T>> {
        self.inner.upgrade().map(|inner| Shared { inner })
    }
}

impl<T> PartialEq for Weak<T> {
    fn eq(&self, other: &Self) -> bool {
        rc::Weak::ptr_eq(&self.inner, &other.inner)
    }
}

/// On the JS backend `downgrade` is the identity, so a weak handle prints as
/// the cell it names.
impl<T: Js> Js for Weak<T> {
    fn js(&self) -> String {
        match self.upgrade() {
            Some(cell) => cell.js(),
            None => "undefined".to_string(),
        }
    }
}

/// The box a MUTABLY-CAPTURED binding becomes (spec §6.9: a closure captures
/// the BINDING, so a binding two closures write is a shared cell).
///
/// R3 ruled v1 boxes every one of them and MEASURES; C15's capture-mode
/// analysis — by-value copies for bindings never written after capture — is the
/// later item that measurement pays for. This is a plain alias of [`Shared`],
/// named separately so the emitted source says which rule put it there.
pub type Captured<T> = Shared<T>;

// ------------------------------------------------------------------ lazy ---

/// `proposal/lazy.md` §5's memo cell — the ONE shape both lazy positions share
/// (a `lazy` parameter and a `lazy let` module binding), and the native twin of
/// the JS backend's `__lazy` / `__force` pair.
///
/// `{ name, state, value, thunk }` is the paper's shape and the states are its
/// four. `Running` IS the cycle trap: an initializer that transitively touches
/// its own binding re-enters [`Lazy::force`] and finds its own flag set, which
/// is a clear panic rather than a silent hang. A panicking thunk POISONS (§6a):
/// the failure propagates at the touching site and every later touch re-panics
/// naming the poison, because retrying would turn "at most once" into "at least
/// once per attempt".
///
/// The thunk is dropped after a successful force, so everything it captured is
/// released once the value exists — the JS helper's `cell.thunk = null`.
///
/// Counted, because a `lazy` argument FORWARDED into another lazy position
/// travels as the same cell however deep the chain: one memo, and the eventual
/// first read forces the original thunk.
pub struct Lazy<T> {
    inner: Rc<LazyCell<T>>,
}

struct LazyCell<T> {
    name: Str,
    state: std::cell::Cell<LazyState>,
    value: RefCell<Option<T>>,
    /// The poison's message, kept so every later touch can name it.
    poison: RefCell<Option<Str>>,
    thunk: RefCell<Option<Box<dyn FnOnce() -> T>>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LazyState {
    Pending,
    Running,
    Done,
    Poisoned,
}

impl<T> Clone for Lazy<T> {
    fn clone(&self) -> Self {
        Lazy {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T> Lazy<T> {
    /// `__lazy(name, thunk)`. The name is carried because the cycle and poison
    /// messages have to say WHICH binding, and the forcing site has no other way
    /// to know.
    pub fn new(name: &str, thunk: impl FnOnce() -> T + 'static) -> Self {
        Lazy {
            inner: Rc::new(LazyCell {
                name: str_new(name),
                state: std::cell::Cell::new(LazyState::Pending),
                value: RefCell::new(None),
                poison: RefCell::new(None),
                thunk: RefCell::new(Some(Box::new(thunk))),
            }),
        }
    }

    /// An already-evaluated cell — what M81's eager set would build if the
    /// emitter ever needed a cell for a value it had in hand.
    pub fn ready(value: T) -> Self {
        Lazy {
            inner: Rc::new(LazyCell {
                name: str_new(""),
                state: std::cell::Cell::new(LazyState::Done),
                value: RefCell::new(Some(value)),
                poison: RefCell::new(None),
                thunk: RefCell::new(None),
            }),
        }
    }

    /// `__force(cell)` — the read every use of a `lazy` binding goes through,
    /// which is the whole of "the parameter reads as a plain `T`".
    pub fn force(&self) -> T
    where
        T: Clone,
    {
        match self.inner.state.get() {
            LazyState::Done => {
                return self
                    .inner
                    .value
                    .borrow()
                    .clone()
                    .expect("a forced lazy cell holds its value");
            }
            LazyState::Running => {
                panic_with(&format!("lazy initialization cycle: `{}`", self.inner.name));
            }
            LazyState::Poisoned => {
                let reason = self.inner.poison.borrow().clone();
                panic_with(&format!(
                    "lazy `{}` is poisoned: its initializer panicked: {}",
                    self.inner.name,
                    reason.unwrap_or_else(|| str_new("panicked"))
                ));
            }
            LazyState::Pending => {}
        }
        let thunk = self
            .inner
            .thunk
            .borrow_mut()
            .take()
            .expect("a pending lazy cell holds its thunk");
        self.inner.state.set(LazyState::Running);
        // The JS helper's `try`/`catch`: the failure is stored, the state goes
        // to poisoned, and the panic keeps travelling from where it was thrown.
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {}));
        let produced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(thunk));
        std::panic::set_hook(previous);
        match produced {
            Ok(value) => {
                *self.inner.value.borrow_mut() = Some(value.clone());
                self.inner.state.set(LazyState::Done);
                value
            }
            Err(payload) => {
                *self.inner.poison.borrow_mut() = Some(describe_panic(&payload));
                self.inner.state.set(LazyState::Poisoned);
                std::panic::resume_unwind(payload);
            }
        }
    }
}

/// `__force` as a free function, so the emitter can spell it without naming the
/// type's own method on a value whose type it is writing inline.
pub fn force<T: Clone>(cell: &Lazy<T>) -> T {
    cell.force()
}

/// A cell is its VALUE wherever one is printed or compared: the cell is the
/// deferral, not a wrapper the program can see. Both force, which is what
/// "fully transparent" means.
impl<T: Js + Clone> Js for Lazy<T> {
    fn js(&self) -> String {
        self.force().js()
    }
    fn js_nested(&self) -> String {
        self.force().js_nested()
    }
}

impl<T: PartialEq + Clone> PartialEq for Lazy<T> {
    fn eq(&self, other: &Self) -> bool {
        self.force() == other.force()
    }
}

impl<T: Json + Clone> Json for Lazy<T> {
    fn json(&self) -> String {
        self.force().json()
    }
}

// ------------------------------------------------------------ collections ---

/// `Map<K, V>` — INSERTION-ORDERED, because a JS `Map` is and the differential
/// compares what a program prints when it walks one.
///
/// Keyed by the canonical hash the `CanonicalHash` intrinsic already computes
/// on the JS side (`JSON.stringify` for an object, the value itself for a
/// scalar); here the key's own `Hash`/`Eq` stand in, which is the same
/// equivalence for every key type in S1a's scope.
pub struct Map<K, V> {
    index: HashMap<K, usize>,
    entries: Vec<Option<(K, V)>>,
    live: usize,
}

impl<K: std::hash::Hash + Eq + Clone, V> Default for Map<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K: std::hash::Hash + Eq + Clone, V> Map<K, V> {
    pub fn new() -> Self {
        Map {
            index: HashMap::new(),
            entries: Vec::new(),
            live: 0,
        }
    }

    pub fn insert(&mut self, key: K, value: V) {
        match self.index.get(&key) {
            // A re-insert keeps the ORIGINAL position, exactly as a JS `Map`
            // does — the order is first-insertion order, not last-write order.
            Some(slot) => self.entries[*slot] = Some((key, value)),
            None => {
                self.index.insert(key.clone(), self.entries.len());
                self.entries.push(Some((key, value)));
                self.live += 1;
            }
        }
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.index
            .get(key)
            .and_then(|slot| self.entries[*slot].as_ref())
            .map(|(_, value)| value)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    pub fn remove(&mut self, key: &K) {
        if let Some(slot) = self.index.remove(key)
            && self.entries[slot].take().is_some()
        {
            self.live -= 1;
        }
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn keys(&self) -> Vec<K>
    where
        K: Clone,
    {
        self.iter().map(|(key, _)| key.clone()).collect()
    }

    pub fn values(&self) -> Vec<V>
    where
        V: Clone,
    {
        self.iter().map(|(_, value)| value.clone()).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.entries
            .iter()
            .filter_map(|entry| entry.as_ref().map(|(key, value)| (key, value)))
    }
}

impl<K: std::hash::Hash + Eq + Clone, V: Clone> Clone for Map<K, V> {
    fn clone(&self) -> Self {
        Map {
            index: self.index.clone(),
            entries: self.entries.clone(),
            live: self.live,
        }
    }
}

/// `Set<T>` — insertion-ordered for the same reason [`Map`] is.
pub struct Set<T> {
    entries: Map<T, ()>,
}

impl<T: std::hash::Hash + Eq + Clone> Default for Set<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: std::hash::Hash + Eq + Clone> Set<T> {
    pub fn new() -> Self {
        Set {
            entries: Map::new(),
        }
    }

    pub fn insert(&mut self, value: T) {
        self.entries.insert(value, ());
    }

    pub fn contains(&self, value: &T) -> bool {
        self.entries.contains_key(value)
    }

    pub fn remove(&mut self, value: &T) {
        self.entries.remove(value);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn values(&self) -> Vec<T> {
        self.entries.keys()
    }
}

impl<T: std::hash::Hash + Eq + Clone> Clone for Set<T> {
    fn clone(&self) -> Self {
        Set {
            entries: self.entries.clone(),
        }
    }
}

/// Entry-wise, in insertion order.
///
/// **This is not reachable from a vilan program**, and it is here because the
/// emitter derives `PartialEq` for every aggregate it writes: `std::map` gives
/// `Map` no `impl PartialEq`, so `a == b` on two maps does not type-check, and
/// `[derive(PartialEq)]` on a struct with a `Map` field is rejected by the
/// analyzer's all-fields-comparable check. What the derive would mean on the JS
/// backend is `===` — reference equality on two `Map` objects — and a value
/// struct has no reference to compare, which is the other half of why this stays
/// unreachable rather than becoming the answer to a question a program can ask.
impl<K: std::hash::Hash + Eq + Clone, V: PartialEq> PartialEq for Map<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.live == other.live && self.iter().eq(other.iter())
    }
}

impl<T: std::hash::Hash + Eq + Clone> PartialEq for Set<T> {
    fn eq(&self, other: &Self) -> bool {
        self.entries == other.entries
    }
}

/// Node prints a `Map` as `Map(2) { 'a' => 1, 'b' => 2 }` and an empty one as
/// `Map(0) {}` — NOT as an array, so this cannot go through [`js_tuple`]. The
/// emitter reaches it through the `Map`/`Set` wrapper structs std writes, whose
/// single field is the raw JS map (`NativeMap`), so a program that prints a
/// `Map` prints this inside `[ .. ]`.
impl<K: Js + std::hash::Hash + Eq + Clone, V: Js> Js for Map<K, V> {
    fn js(&self) -> String {
        if self.is_empty() {
            return format!("Map({}) {{}}", self.len());
        }
        let mut out = String::new();
        let _ = write!(out, "Map({}) {{ ", self.len());
        for (index, (key, value)) in self.iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            let _ = write!(out, "{} => {}", key.js_nested(), value.js_nested());
        }
        out.push_str(" }");
        out
    }
}

/// `Set(2) { 1, 2 }`, `Set(0) {}` — node's own rendering, [`Js for Map`]'s
/// sibling.
impl<T: Js + std::hash::Hash + Eq + Clone> Js for Set<T> {
    fn js(&self) -> String {
        if self.is_empty() {
            return format!("Set({}) {{}}", self.len());
        }
        let mut out = String::new();
        let _ = write!(out, "Set({}) {{ ", self.len());
        for (index, value) in self.values().iter().enumerate() {
            if index > 0 {
                out.push_str(", ");
            }
            out.push_str(&value.js_nested());
        }
        out.push_str(" }");
        out
    }
}

// --------------------------------------------------------- canonical keys ---

/// `std::hash::Hash` — the opaque canonical key `Hashable` answers (I1,
/// `proposal/hashable-keys.md`), as the JS backend's `__hash` actually computes
/// it:
///
/// ```js
/// function __hash(value) {
///     return (typeof value === "object" && value !== null) ? JSON.stringify(value) : value;
/// }
/// ```
///
/// So a vilan `Hash` is always a JS PRIMITIVE — a number, a string, a boolean
/// or `null` — and an aggregate arrives here already flattened into the string
/// `JSON.stringify` made of it. That is why this enum has four arms and not one
/// per vilan type, and why collapsing an aggregate into [`Hash::Text`] is
/// FAITHFUL rather than lossy: on the JS backend a `List<i32>` key and the
/// string `"[1,2]"` really are the same key.
///
/// # The two equalities, which are not the same equality
///
/// `NativeMap` is a JS `Map`, whose key comparison is SameValueZero: `NaN`
/// matches `NaN` and `-0` matches `0`. `hashes_equal` — the body of `impl Hash
/// with PartialEq` — is `===`, under which `NaN` matches nothing. Both are
/// observable, so both are here: [`PartialEq`]/[`Eq`]/[`std::hash::Hash`] below
/// are SameValueZero, because that is what keys the map, and
/// [`Hash::strict_eq`] is `===`.
#[derive(Clone, Debug)]
pub enum Hash {
    Number(f64),
    Text(Str),
    Bool(bool),
    /// `__hash(null)`: `typeof null` is `"object"` but the `value !== null`
    /// guard fails, so the helper answers `null` itself.
    Null,
}

impl Hash {
    /// A JS `Map`'s SameValueZero bits for the number arm: `-0` and `0` are one
    /// key, and every `NaN` is one key.
    fn number_bits(value: f64) -> u64 {
        if value.is_nan() {
            return u64::MAX;
        }
        if value == 0.0 {
            return 0;
        }
        value.to_bits()
    }

    /// `===` on the two canonical keys — `hashes_equal`, the body of `impl Hash
    /// with PartialEq` (`hashable-keys.md` §3.2). Distinct from the map's own
    /// key equality at exactly one value: `NaN === NaN` is false.
    pub fn strict_eq(&self, other: &Hash) -> bool {
        match (self, other) {
            // Rust's `f64: PartialEq` IS JavaScript's `===` for numbers — `NaN`
            // equals nothing and `-0.0 == 0.0`.
            (Hash::Number(left), Hash::Number(right)) => left == right,
            (Hash::Text(left), Hash::Text(right)) => left == right,
            (Hash::Bool(left), Hash::Bool(right)) => left == right,
            (Hash::Null, Hash::Null) => true,
            _ => false,
        }
    }
}

impl PartialEq for Hash {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Hash::Number(left), Hash::Number(right)) => {
                Hash::number_bits(*left) == Hash::number_bits(*right)
            }
            (Hash::Text(left), Hash::Text(right)) => left == right,
            (Hash::Bool(left), Hash::Bool(right)) => left == right,
            (Hash::Null, Hash::Null) => true,
            _ => false,
        }
    }
}

impl Eq for Hash {}

impl std::hash::Hash for Hash {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // The discriminant is hashed first, so the number `1` and the string
        // `"1"` land in different buckets the way two different JS primitives
        // do.
        match self {
            Hash::Number(value) => {
                state.write_u8(0);
                Hash::number_bits(*value).hash(state);
            }
            Hash::Text(text) => {
                state.write_u8(1);
                text.hash(state);
            }
            Hash::Bool(value) => {
                state.write_u8(2);
                value.hash(state);
            }
            Hash::Null => state.write_u8(3),
        }
    }
}

/// A `Hash` IS a JS primitive, so printing one prints that primitive — a string
/// bare at the top level and quoted inside a container, like every other string.
impl Js for Hash {
    fn js(&self) -> String {
        match self {
            Hash::Number(value) => js_number(*value),
            Hash::Text(text) => text.js(),
            Hash::Bool(value) => value.to_string(),
            Hash::Null => "null".to_string(),
        }
    }
    fn js_nested(&self) -> String {
        match self {
            Hash::Text(text) => text.js_nested(),
            other => other.js(),
        }
    }
}

/// `JSON.stringify`, which is NOT [`Js`].
///
/// `Js` is `console.log`: node's inspector, with `[ a, b ]` spacing, bare
/// top-level strings and `Map(1) { .. }`. `JSON.stringify` is the wire form:
/// `[a,b]`, always-quoted strings with JSON escapes, `{}` for a `Map`, `null`
/// for `Infinity`. The canonical hash of an aggregate is its `JSON.stringify`
/// text, so the two renderings have to exist side by side.
///
/// [`Json::canonical_hash`] is `__hash` itself, and its DEFAULT body is the
/// object branch: an aggregate keys as the string `JSON.stringify` made of it.
/// Every primitive impl below overrides it to key as itself, which is the other
/// branch.
pub trait Json {
    fn json(&self) -> String;
    fn canonical_hash(&self) -> Hash {
        Hash::Text(str_new(&self.json()))
    }
}

macro_rules! json_for_integer {
    ($($type:ty),*) => {
        $(impl Json for $type {
            fn json(&self) -> String {
                self.to_string()
            }
            fn canonical_hash(&self) -> Hash {
                Hash::Number(*self as f64)
            }
        })*
    };
}

json_for_integer!(i8, u8, i16, u16, i32, u32, i64, u64, usize, isize);

/// `JSON.stringify(Infinity)` and `JSON.stringify(NaN)` are both `null` — the
/// one place the JSON rendering of a number is not [`js_number`]'s.
impl Json for f64 {
    fn json(&self) -> String {
        if self.is_nan() || self.is_infinite() {
            return "null".to_string();
        }
        js_number(*self)
    }
    fn canonical_hash(&self) -> Hash {
        Hash::Number(*self)
    }
}

impl Json for f32 {
    fn json(&self) -> String {
        (*self as f64).json()
    }
    fn canonical_hash(&self) -> Hash {
        Hash::Number(*self as f64)
    }
}

impl Json for bool {
    fn json(&self) -> String {
        self.to_string()
    }
    fn canonical_hash(&self) -> Hash {
        Hash::Bool(*self)
    }
}

impl Json for Str {
    fn json(&self) -> String {
        json_string(self)
    }
    fn canonical_hash(&self) -> Hash {
        Hash::Text(Rc::clone(self))
    }
}

impl Json for str {
    fn json(&self) -> String {
        json_string(self)
    }
    fn canonical_hash(&self) -> Hash {
        Hash::Text(str_new(self))
    }
}

/// `JSON.stringify(undefined)` answers `undefined` rather than a string, which
/// no vilan program can observe: a `void` value is never a key and never a
/// field. `null` is the honest stand-in and it is what `JSON.stringify([void 0])`
/// writes for a void ELEMENT, which is the only place this arm is reachable.
impl Json for () {
    fn json(&self) -> String {
        "null".to_string()
    }
    fn canonical_hash(&self) -> Hash {
        Hash::Null
    }
}

impl<T: Json> Json for Vec<T> {
    fn json(&self) -> String {
        json_array(&self.iter().map(Json::json).collect::<Vec<String>>())
    }
}

impl<T: Json, const N: usize> Json for [T; N] {
    fn json(&self) -> String {
        json_array(&self.iter().map(Json::json).collect::<Vec<String>>())
    }
}

/// An `Option` is a vilan ENUM, whose JS value is `[index, ...data]` (the same
/// reason [`Js for Option`] prints `[ 0, 5 ]`) — so it is an OBJECT there and it
/// keys as the JSON text of that array, not as its payload.
impl<T: Json> Json for Option<T> {
    fn json(&self) -> String {
        match self {
            Some(value) => json_array(&["0".to_string(), value.json()]),
            None => json_array(&["1".to_string()]),
        }
    }
}

impl<T: Json, E: Json> Json for Result<T, E> {
    fn json(&self) -> String {
        match self {
            Ok(value) => json_array(&["0".to_string(), value.json()]),
            Err(error) => json_array(&["1".to_string(), error.json()]),
        }
    }
}

impl<T: Json> Json for &T {
    fn json(&self) -> String {
        (*self).json()
    }
    fn canonical_hash(&self) -> Hash {
        (*self).canonical_hash()
    }
}

/// A `Hash` is already a canonical key, so hashing one is the identity — which
/// is what `impl Hash with Hashable` says in vilan. Its JSON text is the text of
/// the primitive it holds.
impl Json for Hash {
    fn json(&self) -> String {
        match self {
            Hash::Number(value) => value.json(),
            Hash::Text(text) => text.json(),
            Hash::Bool(value) => value.json(),
            Hash::Null => "null".to_string(),
        }
    }
    fn canonical_hash(&self) -> Hash {
        self.clone()
    }
}

/// `JSON.stringify(new Map([["a", 1]]))` is `"{}"` — a `Map` has no own
/// enumerable properties. So is a `Set`'s.
impl<K, V> Json for Map<K, V> {
    fn json(&self) -> String {
        "{}".to_string()
    }
}

impl<T> Json for Set<T> {
    fn json(&self) -> String {
        "{}".to_string()
    }
}

/// A `Shared` is the object the JS backend spells `{ v: value }` (see
/// [`Js for Shared`]), so that is its JSON too.
impl<T: Json> Json for Shared<T> {
    fn json(&self) -> String {
        format!("{{\"v\":{}}}", self.inner.borrow().json())
    }
}

/// `downgrade` is the identity on the JS backend, so a weak handle's JSON is
/// the cell's — and a dead one is `undefined`, which `JSON.stringify` writes as
/// `null` in every position a vilan program can put it.
impl<T: Json> Json for Weak<T> {
    fn json(&self) -> String {
        match self.upgrade() {
            Some(cell) => cell.json(),
            None => "null".to_string(),
        }
    }
}

/// The four host types the executor IS are opaque objects on the JS backend — a
/// `Task` is a `Promise` — and `JSON.stringify` of an object with no own
/// enumerable properties is `{}`. They exist as `Json` only so that a struct
/// holding one can still carry the `impl Json` the emitter writes beside every
/// `impl Js`.
impl<T> Json for executor::Task<T> {
    fn json(&self) -> String {
        "{}".to_string()
    }
}

impl Json for executor::Nursery {
    fn json(&self) -> String {
        "{}".to_string()
    }
}

impl Json for executor::CancelSignal {
    fn json(&self) -> String {
        "{}".to_string()
    }
}

impl Json for executor::TimerHandle {
    fn json(&self) -> String {
        "{}".to_string()
    }
}

macro_rules! json_for_tuple {
    ($($name:ident),+) => {
        impl<$($name: Json),+> Json for ($($name,)+) {
            fn json(&self) -> String {
                #[allow(non_snake_case, reason = "the binders are the type parameters' own names")]
                let ($($name,)+) = self;
                json_array(&[$($name.json()),+])
            }
        }
    };
}

json_for_tuple!(A);
json_for_tuple!(A, B);
json_for_tuple!(A, B, C);
json_for_tuple!(A, B, C, D);
json_for_tuple!(A, B, C, D, E);
json_for_tuple!(A, B, C, D, E, F);

/// The JSON array an emitted aggregate's `impl Json` builds — `[a,b]`, with NO
/// spacing, which is where it differs from [`js_tuple`].
pub fn json_array(parts: &[String]) -> String {
    format!("[{}]", parts.join(","))
}

/// A JSON string literal, per ECMA-404: the two mandatory escapes, the five
/// short ones, and `\u00XX` for every other control character. `JSON.stringify`
/// leaves every other code point alone (a lone surrogate aside, which a vilan
/// `str` cannot hold).
pub fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            control if control < ' ' => {
                let _ = write!(out, "\\u{:04x}", control as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// `canonical_hash(value)` — the `CanonicalHash` intrinsic, as a free function
/// so the emitter can spell it without naming the trait.
pub fn canonical_hash<T: Json + ?Sized>(value: &T) -> Hash {
    value.canonical_hash()
}

/// `hashes_equal(a, b)` — the `HashEq` intrinsic. See [`Hash::strict_eq`].
pub fn hashes_equal(left: &Hash, right: &Hash) -> bool {
    left.strict_eq(right)
}

// ----------------------------------------------------------- list helpers ---

/// `List::get` — `__list_get`. Out of range is `None`, never a panic; the index
/// is signed because a vilan `i32` index can be negative and that is a miss,
/// not an overflow.
pub fn list_get<T: Clone>(list: &[T], index: i64) -> Option<T> {
    if index < 0 {
        return None;
    }
    list.get(index as usize).cloned()
}

/// `List::pop` — `__list_pop`.
pub fn list_pop<T>(list: &mut Vec<T>) -> Option<T> {
    list.pop()
}

/// `List::remove` — `remove(&mut self, index: i32): T`, so it answers the
/// ELEMENT, not an `Option`. An out-of-range index is a panic with a message
/// rather than a silent `undefined`.
pub fn list_remove<T>(list: &mut Vec<T>, index: i64) -> T {
    if index < 0 || index as usize >= list.len() {
        panic_with("List::remove: index out of range");
    }
    list.remove(index as usize)
}

/// `List::insert` — a past-the-end index appends, as the JS `splice` does.
pub fn list_insert<T>(list: &mut Vec<T>, index: i64, value: T) {
    let at = if index < 0 {
        0
    } else {
        (index as usize).min(list.len())
    };
    list.insert(at, value);
}

// ---------------------------------------------------------- str intrinsics --

pub fn str_len(text: &str) -> i32 {
    // JavaScript's `.length` counts UTF-16 code units, and the corpus's strings
    // are ASCII, where the two agree. A non-ASCII program is a KNOWN divergence
    // and the differential reports it rather than this pretending otherwise.
    text.chars().map(|c| c.len_utf16() as i32).sum()
}

pub fn str_trim(text: &str) -> Str {
    Rc::from(text.trim())
}

pub fn str_to_lowercase(text: &str) -> Str {
    Rc::from(text.to_lowercase().as_str())
}

pub fn str_to_uppercase(text: &str) -> Str {
    Rc::from(text.to_uppercase().as_str())
}

pub fn str_contains(text: &str, needle: &str) -> bool {
    text.contains(needle)
}

pub fn str_starts_with(text: &str, prefix: &str) -> bool {
    text.starts_with(prefix)
}

pub fn str_ends_with(text: &str, suffix: &str) -> bool {
    text.ends_with(suffix)
}

pub fn str_replace(text: &str, from: &str, to: &str) -> Str {
    Rc::from(text.replace(from, to).as_str())
}

pub fn str_repeat(text: &str, times: i32) -> Str {
    Rc::from(text.repeat(times.max(0) as usize).as_str())
}

pub fn str_split(text: &str, separator: &str) -> Vec<Str> {
    if separator.is_empty() {
        return text
            .chars()
            .map(|c| Rc::from(c.to_string().as_str()))
            .collect();
    }
    text.split(separator).map(Rc::from).collect()
}

/// `str::substring` — JavaScript's, which CLAMPS rather than panicking and
/// swaps a reversed pair.
pub fn str_substring(text: &str, start: i32, end: i32) -> Str {
    let characters: Vec<char> = text.chars().collect();
    let length = characters.len() as i64;
    let mut first = (start as i64).clamp(0, length);
    let mut last = (end as i64).clamp(0, length);
    if first > last {
        std::mem::swap(&mut first, &mut last);
    }
    Rc::from(
        characters[first as usize..last as usize]
            .iter()
            .collect::<String>()
            .as_str(),
    )
}

pub fn parse_i32(text: &str) -> Option<i32> {
    text.trim().parse::<i32>().ok()
}

pub fn parse_f64(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_print_the_way_javascript_prints_them() {
        assert_eq!(js_number(2.0), "2");
        assert_eq!(js_number(1.5), "1.5");
        assert_eq!(js_number(-0.0), "0");
        assert_eq!(js_number(f64::INFINITY), "Infinity");
        assert_eq!(js_number(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(js_number(f64::NAN), "NaN");
        assert_eq!(js_number(1e21), "1e+21");
        assert_eq!(js_number(1e-7), "1e-7");
    }

    #[test]
    fn a_list_prints_with_the_inner_spaces_node_writes() {
        assert_eq!(Vec::<i32>::new().js(), "[]");
        assert_eq!(vec![1, 2, 3].js(), "[ 1, 2, 3 ]");
    }

    #[test]
    fn a_string_is_bare_at_the_top_and_quoted_inside_a_list() {
        let text: Str = str_new("hi");
        assert_eq!(text.js(), "hi");
        assert_eq!(vec![text].js(), "[ 'hi' ]");
    }

    #[test]
    fn a_map_keeps_insertion_order_and_a_reinsert_keeps_its_slot() {
        let mut map: Map<i32, i32> = Map::new();
        map.insert(2, 20);
        map.insert(1, 10);
        map.insert(2, 22);
        assert_eq!(map.keys(), vec![2, 1]);
        assert_eq!(map.values(), vec![22, 10]);
        map.remove(&2);
        assert_eq!(map.keys(), vec![1]);
        assert_eq!(map.len(), 1);
    }

    #[test]
    fn guarded_catches_a_panic_and_hands_back_its_message() {
        let caught = guarded(|| panic_with("boom"));
        assert_eq!(
            caught.err().map(|m| m.to_string()),
            Some("boom".to_string())
        );
        assert!(guarded(|| {}).is_ok());
    }

    #[test]
    fn with_finally_runs_after_a_panic_too() {
        let ran = Rc::new(RefCell::new(false));
        let flag = Rc::clone(&ran);
        let caught =
            guarded(move || with_finally(|| panic_with("boom"), || *flag.borrow_mut() = true));
        assert!(caught.is_err());
        assert!(*ran.borrow());
    }

    #[test]
    fn substring_clamps_and_swaps_the_way_javascript_does() {
        assert_eq!(str_substring("abcdef", 1, 3).to_string(), "bc");
        assert_eq!(str_substring("abcdef", 3, 1).to_string(), "bc");
        assert_eq!(str_substring("abc", 0, 99).to_string(), "abc");
    }

    // -- F20: the canonical key --

    /// `__hash`'s two branches: a primitive keys as ITSELF, an aggregate as the
    /// string `JSON.stringify` made of it.
    #[test]
    fn a_primitive_keys_as_itself_and_an_aggregate_as_its_json() {
        assert_eq!(canonical_hash(&7i32), Hash::Number(7.0));
        assert_eq!(canonical_hash(&1.5f64), Hash::Number(1.5));
        assert_eq!(canonical_hash(&true), Hash::Bool(true));
        assert_eq!(canonical_hash(&str_new("a")), Hash::Text(str_new("a")));
        assert_eq!(canonical_hash(&vec![1i32, 2]), Hash::Text(str_new("[1,2]")));
        // The collapse JavaScript itself performs: a list key and the string of
        // its JSON really are one key there, so they are one key here.
        assert_eq!(
            canonical_hash(&vec![1i32, 2]),
            canonical_hash(&str_new("[1,2]"))
        );
    }

    /// The number `1` and the string `"1"` are two different JS primitives, so
    /// they are two different keys — the case a canonicalise-everything-to-text
    /// representation would silently merge.
    #[test]
    fn a_number_key_and_a_string_key_that_render_alike_stay_distinct() {
        assert_ne!(canonical_hash(&1i32), canonical_hash(&str_new("1")));
        let mut map: Map<Hash, i32> = Map::new();
        map.insert(canonical_hash(&1i32), 10);
        map.insert(canonical_hash(&str_new("1")), 20);
        assert_eq!(map.len(), 2);
    }

    /// The two equalities, which differ at exactly one value. A JS `Map` keys by
    /// SameValueZero (`NaN` matches `NaN`, `-0` matches `0`); `hashes_equal` is
    /// `===`, under which `NaN` matches nothing.
    #[test]
    fn the_maps_key_equality_and_strict_equality_part_company_at_nan() {
        let nan = canonical_hash(&f64::NAN);
        assert_eq!(nan, canonical_hash(&f64::NAN), "SameValueZero keys NaN");
        assert!(
            !hashes_equal(&nan, &canonical_hash(&f64::NAN)),
            "`===` does not"
        );
        let mut map: Map<Hash, i32> = Map::new();
        map.insert(nan, 1);
        map.insert(canonical_hash(&f64::NAN), 2);
        assert_eq!(map.len(), 1, "one NaN key, as a JS `Map` has");

        assert_eq!(canonical_hash(&0.0f64), canonical_hash(&-0.0f64));
        assert!(hashes_equal(
            &canonical_hash(&0.0f64),
            &canonical_hash(&-0.0f64)
        ));
    }

    /// `JSON.stringify` is not `console.log`: no spacing, always-quoted strings,
    /// `null` for a non-finite number, `{}` for a `Map`.
    #[test]
    fn json_is_not_the_console_log_rendering() {
        assert_eq!(vec![str_new("a"), str_new("b")].json(), "[\"a\",\"b\"]");
        assert_eq!(vec![str_new("a"), str_new("b")].js(), "[ 'a', 'b' ]");
        assert_eq!(f64::INFINITY.json(), "null");
        assert_eq!(f64::INFINITY.js(), "Infinity");
        assert_eq!(f64::NAN.json(), "null");
        let map: Map<Hash, i32> = Map::new();
        assert_eq!(map.json(), "{}");
        // An `Option` is a vilan ENUM, so its JS value is `[index, ...data]`.
        assert_eq!(Some(5i32).json(), "[0,5]");
        assert_eq!(None::<i32>.json(), "[1]");
        assert_eq!(str_new("q\"\n\t\u{1}").json(), "\"q\\\"\\n\\t\\u0001\"");
    }

    /// Node's own `Map` / `Set` rendering, which is not an array's.
    #[test]
    fn a_map_and_a_set_print_the_way_node_prints_them() {
        let mut map: Map<Hash, i32> = Map::new();
        assert_eq!(map.js(), "Map(0) {}");
        map.insert(canonical_hash(&str_new("a")), 1);
        map.insert(canonical_hash(&str_new("b")), 2);
        assert_eq!(map.js(), "Map(2) { 'a' => 1, 'b' => 2 }");
        let mut set: Set<i32> = Set::new();
        assert_eq!(set.js(), "Set(0) {}");
        set.insert(1);
        set.insert(2);
        assert_eq!(set.js(), "Set(2) { 1, 2 }");
    }

    /// `Shared::write()` used as a PLACE reaches the cell, where a read copies
    /// out of it. The two were one call on the JS backend (`cell.v`), which is
    /// how the place form came to be emitted as a write of `()`.
    #[test]
    fn a_shared_write_place_reaches_the_cell_where_a_read_copies() {
        let cell: Shared<Vec<i32>> = Shared::new(Vec::new());
        cell.get().push(1);
        assert_eq!(cell.get().len(), 0, "a read is a copy");
        cell.borrow_mut().push(1);
        assert_eq!(cell.get().len(), 1, "the place is the cell");
    }
}
