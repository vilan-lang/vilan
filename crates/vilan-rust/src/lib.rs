//! `vilan-rust` — the emit-Rust backend (tracker F1, slice S1a).
//!
//! The same `Program` the JS emitter reads, a Rust source file out. It sits
//! BESIDE `transformer.rs` rather than inside it, because the two share an
//! input and nothing else: the JS emitter's whole job is a dynamically typed
//! target where a struct is an array and a closure is a reference, and this
//! one's is a statically typed target where a struct is a struct and a closure
//! has to say who counts it.
//!
//! # Scope, which is hard and deliberate
//!
//! S1a is the FIRST cut of `native-apps.md` §5-S1. What it emits is what the
//! paper's probe translated: structs, enums, `Option`/`Result`, `str`, `List`,
//! `Map`/`Set`, closures, `impl`s, `print`, `panic`, and the counted cell. What
//! it does not emit — async, UI, rpc, the filesystem, the platform surface,
//! generic functions, module-level bindings — it REFUSES, by name, with a
//! sentence saying which. A backend that silently emitted something else for a
//! construct it did not understand would fail the differential in a way nobody
//! could read; a backend that names the construct turns its own gaps into the
//! work list for S1b, and [`crates/vilan-cli/tests/native_differential.rs`]
//! prints exactly that list.
//!
//! # Why the walk is cheap
//!
//! Most of what makes `transformer.rs` twelve thousand lines is already done by
//! the time a program reaches here. Contexts are threaded into ordinary
//! parameters, `const` is folded, and — the one that matters most — a method
//! call `p.bump(2)` has already been RESOLVED by the analyzer into a call whose
//! subject is the member's own id and whose first argument is the receiver. So
//! this emitter needs no impl selection and no dispatch table for the concrete
//! case.
//!
//! # S1b: monomorphisation, and where its parts come from
//!
//! 51 of S1a's 98 refusals were generics, and closing them is this slice. The
//! JS emitter has done monomorphisation since long before either backend
//! existed, and **its instance set is not a data structure this one can read**:
//! `Transformer::instances` is minted DURING the JS walk, keyed by
//! `(function, type keys, adapted-asyncness bits)`, and its values are JS
//! function nodes. There is no pre-computed set on `Program` to consume (see the
//! lane's report — this was the slice's first open question).
//!
//! What IS shared, and is what this emitter reads, is the backend-independent
//! half: [`vilan_core::impl_select`] (`select_member`, `bind_subject`,
//! `applying_trait_ids` — selecting an impl for a concrete receiver, and
//! recovering the bindings that selection implies) plus the analyzer's own
//! records, `method_call_substitution`, `generic_dispatch`,
//! `own_generic_call_bindings` and `bound_dispatch_traits`. The mechanism around
//! them — a substitution threaded through the walk, a `resolve_type_id` that
//! follows a `Generic` to its binding, a structural instance key, one memo per
//! instance — is reproduced here rather than lifted out of `transformer.rs`,
//! because lifting it is a refactor of a file three other lanes are editing this
//! order. That the two mechanisms must agree is exactly what the differential
//! measures.
//!
//! One thing is genuinely different, and it is the reason the reproduction is
//! not a copy: **JS needs no concrete types and Rust needs nothing else.** The
//! JS emitter resolves a bare `Generic` and stops, because `List<T>` and
//! `List<i32>` are the same array. Here `List<T>` must come out `Vec<i32>`, and
//! `struct Pair<T>` must come out as one Rust `struct` PER instantiation — so
//! type rendering resolves structurally, and a nominal declaration is emitted
//! once per concrete argument list.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use vilan_core::analyzer::{
    AdaptedInstance, Backing, BackingValue, Expr, ExprIfBranch, ExprMatchLeg, ExprPattern,
    GenericDispatch, Intrinsic, Program, RENDER_MEMBER, TryDispatch,
};
use vilan_core::error::Error;
use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::impl_select;
use vilan_core::mono;
use vilan_core::node::{BinaryOp, Convention, ExternBinding};
use vilan_core::options::BuildOptions;
use vilan_core::span::Span;
use vilan_core::type_::{Type, TypeId};

/// What one emit produced: the Rust source, and the measurement R3 asked for.
pub struct Emitted {
    /// The whole program, as one `main.rs`.
    pub source: String,
    /// The HOST surface the program reached, in name order — the platform
    /// bindings, intrinsics and external types this backend has no native body
    /// for (tracker F18's work list).
    ///
    /// Filled only under `VILAN_NATIVE_HOST_CENSUS=1`, which makes a host gap a
    /// RECORDED `unimplemented!()` instead of a refusal so one emit can walk a
    /// whole program and answer with the list rather than with its first
    /// sentence. The source such an emit produces is a census, not a build:
    /// nothing compiles it, and the mode exists because "what does this program
    /// still need" is a question one run should answer.
    pub host_gaps: Vec<String>,
    /// Whether the program reached `std::db`, so the cargo project written for
    /// it depends on `vilan-rt-sqlite` (F18 slice 2; Order 39's R1). A program
    /// that does not names the crate nowhere and never builds it.
    pub reaches_sqlite: bool,
    /// R3's measurement: how many bindings this program had to box into
    /// `vilan_rt::Captured<_>` (an `Rc<RefCell<_>>`) because a closure captures
    /// them and something writes them. C15's by-value capture optimisation is
    /// the later item this number pays for.
    pub boxed_bindings: usize,
    /// F31's measurement, the native twin of `copy-elision-census.tsv`: how
    /// many CONSUMED place reads this emit had to copy, and how many it moved
    /// because the read was the binding's last use.
    ///
    /// These are the copies rule 1's own marking never reached — a closure
    /// call's arguments, a variant constructor's, a destructure's — so they are
    /// exactly the number the native liveness pass is answerable for, and not a
    /// count of every `.clone()` in the emitted source (a refcount bump on a
    /// handle is one of those and is not a copy).
    pub consumed_copies: usize,
    pub consumed_copies_elided: usize,
}

/// Emits `program` as a single Rust source file.
///
/// `Err` carries the construct S1a does not reach, located at the expression
/// that wrote it, in exactly the shape a compiler diagnostic takes — so
/// `vilan build --backend rust` reports an unsupported program the way it
/// reports any other refusal, rather than producing Rust that will not build.
pub fn emit(program: &Program<'_>, _options: &BuildOptions) -> Result<Emitted, Error> {
    Emitter::new(program).run()
}

/// The name `async fun main`'s body takes, since `fn main` cannot be `async`
/// and the executor has to be entered from a synchronous frame.
const ASYNC_MAIN_BODY: &str = "vilan_async_main";

/// The prelude every emitted program carries.
const PRELUDE: &str = "\
#![allow(unused_imports, unused_parens, unused_variables, unused_mut, unused_braces)]
#![allow(dead_code)]
#![allow(unreachable_patterns, non_camel_case_types, non_snake_case, clippy::all)]
use vilan_rt::Js as _;
use vilan_rt::Json as _;
";

fn unsupported(what: &str, span: Span) -> Error {
    Error {
        trace: Vec::new(),
        note: None,
        span,
        msg: format!(
            "the `rust` backend does not emit {what} yet — this is F1's slice S1b, \
             whose scope is structs, enums, generics, `Option`/`Result`, `str`, `List`, \
             `Map`/`Set`, closures, `impl`s, traits, operators, `?`, `print`, `panic` and \
             the reactive cell. Build this program with `--backend js`."
        ),
    }
}

/// Where in the emitted file one reserved slot's text goes, and under what name.
struct Reserved {
    name: String,
    slot: usize,
}

struct Emitter<'a, 'src> {
    program: &'a Program<'src>,
    /// Function bodies, keyed by the slot reserved for them — so the emitted
    /// order is discovery order and a body written during a nested walk cannot
    /// reorder the file.
    functions: BTreeMap<usize, String>,
    /// Instances emitted or in progress, keyed by (function, the structural keys
    /// of the concrete types its generic parameters are bound to). An empty key
    /// list is the non-generic case, which keeps its plain `{name}_{id}`.
    ///
    /// The memo is written BEFORE the body is walked, exactly as the JS
    /// emitter's is — a self-recursive body has to be able to call itself.
    instances: HashMap<(Id, Vec<String>), Reserved>,
    /// Trait DEFAULT bodies, specialized per concrete receiver type: (default,
    /// that type's structural key). Keyed separately from [`Self::instances`]
    /// because what varies is `Self`, not a written generic parameter.
    default_instances: HashMap<(Id, String), Reserved>,
    /// Nominal type declarations, keyed by (declaration, its concrete
    /// arguments). `Pair<i32>` and `Pair<str>` are two Rust structs.
    types: BTreeMap<usize, String>,
    type_instances: HashMap<(Id, Vec<String>), Reserved>,
    /// The next reserved slot. Functions and types number independently
    /// (they are two sections of the file).
    next_function_slot: usize,
    next_type_slot: usize,
    /// The generic binding in force while a body is walked: generic constraint
    /// id -> the type it is bound to. [`Emitter::concrete`] follows it.
    current_substitution: HashMap<TypeId, TypeId>,
    /// The concrete type a trait default is being specialized for, so a
    /// `self.method()` inside it re-dispatches there (the JS emitter's
    /// `current_self_type`).
    current_self_type: Option<TypeId>,
    /// The trait whose default is being specialized, plus its supertraits.
    ///
    /// Inside a default body `self` is typed as the TRAIT, not as the type the
    /// default is being specialized for — so `fun shout(self): str {
    /// self.describe() + "!" }` asks this emitter to render a parameter of
    /// trait type, which natively is nothing. The set is what makes the
    /// rewrite to `current_self_type` narrow: only the traits this body's
    /// `Self` actually satisfies are rewritten, so a genuinely trait-typed
    /// value elsewhere keeps its refusal rather than quietly becoming `Self`.
    current_self_traits: HashSet<Id>,
    /// Whether the function being emitted hands back a VIEW (`borrows`). Its
    /// return positions then pass the loan on instead of copying out of it —
    /// `fun same(x: &mut i32): &mut i32 { x }` returns `x`, and rule 1's copy
    /// there is a type error, not a copy.
    current_returns_view: bool,
    /// The type the position an expression is being emitted INTO declares, when
    /// that position declares one — a binding's annotation, a call argument's
    /// parameter, a struct field, a `ret`'s return type.
    ///
    /// It exists for one job the JS emitter has no use for: a generic variant
    /// CONSTRUCTOR records an open type at its own site (`Tree::Leaf(7)` is
    /// `Tree<any>` until the annotation beside it closes the hole), and a Rust
    /// enum cannot be minted over a hole. Consulted ONLY where the recorded
    /// type is not grounded, and only when it names the same declaration, so it
    /// can narrow an answer and never change one.
    expected_type: Option<TypeId>,
    /// R3: the bindings boxed into a counted cell, and why they had to be.
    /// Computed over EVERY closure the program loaded, std's unreached ones
    /// included — it is a lookup the walk consults, not a measurement.
    boxed: HashSet<Id>,
    /// The subset of [`Emitter::boxed`] whose declaration this walk actually
    /// EMITTED as a `vilan_rt::Captured` — R3's measurement. The two differ by
    /// every mutably-captured binding in a function the program never reaches:
    /// A108 gave `json_codec` and `binary_codec` one each, and a program that
    /// merely loads `std::json` would otherwise report two boxes its binary
    /// does not contain.
    boxed_emitted: HashSet<Id>,
    /// Every module-level binding in the world, lowered to a `thread_local!`.
    module_bindings: HashSet<Id>,
    /// The module-level bindings this program actually READ, in reach order,
    /// with the `thread_local!` block each lowered to.
    module_binding_cells: BTreeMap<u32, String>,
    /// Module-level bindings whose cell is being built, so a binding whose
    /// initializer reads another one does not recurse forever.
    module_bindings_started: HashSet<Id>,
    /// B105: expression ids standing for a temp the compound-assignment hoist
    /// declared ahead of the write. Live only while one assignment is rendered.
    hoisted: HashMap<Id, String>,
    /// Whether this emit is a HOST CENSUS rather than a build (see
    /// [`Emitted::host_gaps`]).
    census: bool,
    /// The host surface a census emit reached, deduplicated and ordered.
    host_gaps: std::collections::BTreeSet<String>,
    /// `is`-test captures currently bound by an enclosing `if let` — the only
    /// scope in which a capture has a native name at all (it has no `variables`
    /// record; the JS emitter substitutes the payload accessor instead).
    is_captures: HashSet<Id>,
    /// J6: the name of the function whose body is being walked — a spawn's
    /// ORIGIN, which is what the unobserved-failure report names. The JS
    /// emitter keeps the same thing under the same name.
    current_origin: Option<&'src str>,
    /// Whether the value being emitted initializes a binding that HOLDS a view
    /// (F20) — `let v = &mut n;`, `let items = holder.items_view();`.
    ///
    /// Such a binding is the one value position that wants the REFERENCE rather
    /// than what it points at: the type system has no reference form, so the
    /// binding's recorded type is its pointee's and only the initializer's own
    /// shape says otherwise. Without this, B109's read-through copy fired here
    /// too and `let v = &mut n` bound an `i32`.
    declaring_a_view: bool,
    /// Whether the position whose TYPE is being rendered, or whose VALUE is
    /// being emitted, is a DECLARED-async closure one (F20).
    ///
    /// Asyncness is not part of `Type::Closure` — the analyzer records it per
    /// SITE (`async_fields`, `async_values`, `async_returning`), because
    /// `async |T| U` and `|T| U` are the same TYPE with different call
    /// obligations. Natively they are two different types: an async closure
    /// answers a future, so it is `Rc<dyn Fn(T) -> Boxed<U>>` and a literal
    /// landing there is `pin_future(async move { .. })`.
    ///
    /// A flag rather than a parameter on `rust_type` because the three sites
    /// that know (a struct field, a parameter, a return type) are far from the
    /// one arm that needs it; it is cleared as soon as that arm reads it, so
    /// only the OUTERMOST closure of a position is affected.
    expects_async: bool,
    /// Whether the VALUE position being emitted is a declared-async closure one
    /// — the twin of [`Emitter::expects_async`] on the value side, so a SYNC
    /// closure literal landing in an `async |T| U` field or parameter is wrapped
    /// into a future rather than handed over with the wrong type.
    /// `Server::builder()`'s default handler is the shape.
    expects_async_value: bool,
    /// The DECLARED return type of the function being emitted, threaded to its
    /// return positions so a generic aggregate built there instantiates at the
    /// signature's arguments rather than at the ones its own site recorded.
    ///
    /// The signature is what the body has to satisfy; a literal's own record
    /// can be open (`Taken<Self, T>` inside a trait default) and a Rust struct
    /// cannot be minted over a hole. Consulted only where the recorded
    /// arguments are NOT grounded and only when the two name the same
    /// declaration, which is `Emitter::expected_type`'s standing rule.
    current_return_type: Option<TypeId>,
    /// Whether this program reached `std::db` (F18 slice 2, Order 39's R1).
    ///
    /// SQLite is the one runtime surface with a crates.io dependency, so it is
    /// a crate of its own and the emitted cargo project names it only when the
    /// program needs it — a program that does not pays neither the lockfile
    /// entry nor the C compile of SQLite's amalgamation. The flag is set where
    /// one of the three host TYPES is rendered, which is the narrowest point
    /// every reach passes through: a binding takes or answers one.
    reaches_sqlite: bool,
    /// Whether the function being emitted DECLARES an `async |T| U` return
    /// type (J2's `async_returning`) — so the closure literal it hands back is
    /// a future-answering one.
    ///
    /// [`Emitter::expects_async`] is the same fact on the TYPE side and was
    /// already read; the value side was not, so `fold_service_requests` —
    /// `std::rpc_server`'s fold, which every `Server::builder()` reaches —
    /// returned a closure whose body calls an `async` one and was refused as an
    /// adapted instance. It is not one: the position DECLARES the asyncness,
    /// which is precisely the case F20 lifted the refusal for at a field and at
    /// an argument.
    ///
    /// Saved and restored around a nested body, because a synchronous closure
    /// inside such a function still returns a value.
    returns_an_async_closure: bool,
    /// The bindings each enclosing closure CAPTURES, innermost last (F20).
    ///
    /// A `move` closure owns its captures, so a body that hands one on by value
    /// moves out of the closure — which makes it `FnOnce`, and no closure-typed
    /// position natively takes one: every closure type is `Rc<dyn Fn>` (F16).
    /// JavaScript never had to ask, because a capture there is a binding two
    /// frames share. So a read of a captured binding in a VALUE position copies,
    /// which is rule 1's answer anyway — the analyzer's own `clone_sites` elides
    /// it at a LAST use, and a last use inside a closure body is not a last use
    /// of the capture.
    closure_captures: Vec<HashSet<Id>>,
    /// F21: whether the TYPE being rendered is an `Option` whose payload is a
    /// VIEW, and whether that view is writable.
    ///
    /// The type system has no reference form — a payload's type is its
    /// pointee's and viewness is recorded beside it — so `Option<&mut i32>` and
    /// `Option<i32>` are one type id and only the position says which. A flag
    /// rather than a parameter for the same reason
    /// [`Emitter::expects_async`] is one: the site that knows (a signature's
    /// return position) is far from the arm that needs it, and it is TAKEN by
    /// that arm so only the outermost `Option` of a position is affected.
    expects_payload_view: Option<bool>,
    /// F21: whether the expression being rendered is a `match` SUBJECT — the
    /// one position an `Option` with a view payload is carried through today.
    /// Taken by the call arm that reads it, so only the outermost call of the
    /// subject is affected.
    matching_the_subject: bool,
    /// F22 (async-polymorphism.md A.1): the ADAPTED INSTANCE being emitted —
    /// which of the callee's closure parameters arrive async at this instance,
    /// and the emission decisions the analyzer already made for that pairing.
    ///
    /// This emitter monomorphises on TYPES, and asyncness is not one: a callee
    /// handed an async closure at one call site and a synchronous one at
    /// another is two functions natively, exactly as it is two on the JS
    /// backend. The bits are independent of the type substitution, so one
    /// `AdaptedInstance` serves every type instantiation and the instance key
    /// carries both.
    current_adapted_bits: Vec<Id>,
    current_instance: Option<AdaptedInstance>,
    /// F31: the READS that are their binding's last use in the body they sit
    /// in, so the conservative copy [`Emitter::copy_a_consumed_place_read`]
    /// takes can be downgraded to a move.
    ///
    /// The JS emitter gets this from the analyzer (`clone_sites`' rule-2
    /// elision, `lifetimes.md` §6); the positions this backend copies at are
    /// the ones rule 1's marking never reached, so the answer is computed here
    /// over the SAME bodies the emitter walks.
    last_uses: HashSet<Id>,
    /// The function bodies whose liveness has been computed, so a function
    /// emitted at three instantiations is walked once. Keyed on the
    /// FUNCTION, because the expression ids under it are the same at every
    /// instantiation.
    liveness_walked: HashSet<Id>,
    /// F31's census, the native twin of `copy-elision-census.tsv`: how many
    /// consumed place reads this emit COPIED and how many it moved.
    copies_taken: usize,
    copies_elided: usize,
    /// A124 R3: the Rust trait each OBJECT type lowers to — one per vilan trait
    /// at its concrete arguments (`dyn Source<i32>` and `dyn Source<str>` are
    /// two), keyed like a nominal instance. See [`Emitter::ensure_object_trait`].
    object_traits: HashMap<(Id, Vec<String>), ObjectTrait>,
    /// The `impl ObjectX for Concrete` blocks already written, by the object
    /// trait's name and the RENDERED concrete type — rendered, because two
    /// vilan types that lower to one Rust type must share one impl.
    object_impls: HashSet<(String, String)>,
}

/// One object type's Rust trait: its name and its slots, each slot's
/// signature rendered ONCE, at the object's own arguments, so an impl written
/// later — under another body's substitution — repeats it verbatim.
#[derive(Clone)]
struct ObjectTrait {
    name: String,
    slots: Vec<ObjectSlot>,
}

#[derive(Clone)]
struct ObjectSlot {
    /// The vilan member's name, which is also the slot method's.
    member: String,
    /// The member as the trait (or the supertrait that declares it) declares
    /// it — whose parameters' conventions a call through the object renders
    /// its arguments against.
    declaration: Id,
    /// `fn get(&self, observer: T) -> R`, without the trailing `;` or body.
    signature: String,
    /// The non-receiver parameters' names, in order — what an impl forwards.
    forwarded: Vec<String>,
}

/// F31's walk state: where each binding was declared, and the last read of it
/// that is a candidate for a move.
#[derive(Default)]
struct Liveness {
    /// The region depth a binding was declared at. A read deeper than that is
    /// inside a loop, a closure or a spawn RELATIVE to the declaration, so it
    /// may run more than once for one declaration and is never a last use;
    /// a binding the loop body itself declares is fresh on every iteration and
    /// elides at its last use, which is the distinction a lexical
    /// "inside a loop" set cannot make (`lifetimes.md` §6 makes the same one).
    declared_at: HashMap<Id, usize>,
    /// The candidate read per binding: `Some(id)` for the latest read that may
    /// move, `None` once a read that may repeat has been seen. Overwritten by
    /// every later read, so what survives the walk is the last one.
    candidate: HashMap<Id, Option<Id>>,
}

impl<'a, 'src> Emitter<'a, 'src> {
    fn new(program: &'a Program<'src>) -> Self {
        Emitter {
            program,
            functions: BTreeMap::new(),
            instances: HashMap::default(),
            default_instances: HashMap::default(),
            types: BTreeMap::new(),
            type_instances: HashMap::default(),
            next_function_slot: 0,
            next_type_slot: 0,
            current_substitution: HashMap::default(),
            current_self_type: None,
            current_self_traits: HashSet::new(),
            current_returns_view: false,
            expected_type: None,
            boxed: HashSet::new(),
            boxed_emitted: HashSet::new(),
            module_bindings: HashSet::new(),
            module_binding_cells: BTreeMap::new(),
            module_bindings_started: HashSet::new(),
            hoisted: HashMap::default(),
            is_captures: HashSet::new(),
            census: std::env::var_os("VILAN_NATIVE_HOST_CENSUS").is_some(),
            host_gaps: std::collections::BTreeSet::new(),
            current_origin: None,
            declaring_a_view: false,
            expects_async: false,
            expects_async_value: false,
            current_return_type: None,
            reaches_sqlite: false,
            returns_an_async_closure: false,
            closure_captures: Vec::new(),
            expects_payload_view: None,
            matching_the_subject: false,
            current_adapted_bits: Vec::new(),
            current_instance: None,
            last_uses: HashSet::new(),
            liveness_walked: HashSet::new(),
            copies_taken: 0,
            copies_elided: 0,
            object_traits: HashMap::default(),
            object_impls: HashSet::new(),
        }
    }

    fn run(mut self) -> Result<Emitted, Error> {
        let global_scope = self
            .program
            .scopes
            .get(&self.program.global_scope_id)
            .ok_or_else(|| unsupported("a program with no global scope", Span::new((), 0..0)))?;
        let main_id = *global_scope
            .name_to_id_map
            .get("main")
            .filter(|id| self.program.functions.contains_key(*id))
            .ok_or_else(|| Error {
                trace: Vec::new(),
                note: None,
                msg: "Cannot execute program without a main function".to_string(),
                span: Span::new((), 0..0),
            })?;

        // A module-level binding is a `static` with a constructor. It is
        // lowered where one is READ, not where one exists: std declares several
        // (`PI`, the reactive turn's registers) and a program reaching none of
        // them must carry none of them.
        self.module_bindings = self.program.module_level_bindings().into_iter().collect();
        self.compute_boxed_bindings();

        let main = self.ensure_function(main_id, &HashMap::default())?;
        let main_body = self
            .functions
            .remove(&main.slot)
            .expect("main was just emitted");

        let mut source = String::from(PRELUDE);
        for declaration in self.types.values() {
            source.push('\n');
            source.push_str(declaration);
        }
        for cell in self.module_binding_cells.values() {
            source.push('\n');
            source.push_str(cell);
        }
        for body in self.functions.values() {
            source.push('\n');
            source.push_str(body);
        }
        source.push('\n');
        source.push_str(&main_body);
        Ok(Emitted {
            source,
            host_gaps: self.host_gaps.iter().cloned().collect(),
            reaches_sqlite: self.reaches_sqlite,
            boxed_bindings: self.boxed_emitted.len(),
            consumed_copies: self.copies_taken,
            consumed_copies_elided: self.copies_elided,
        })
    }

    /// R3, as ruled: v1 boxes EVERY mutably-captured binding into a counted
    /// cell (`Rc<RefCell<_>>`), and the count over the exit corpus is the
    /// measurement C15's by-value capture optimisation has to beat.
    ///
    /// Spec §6.9 is why there is no choice here: a closure captures the
    /// BINDING, not the value, so a `mut` local a closure reads is a place two
    /// frames share. JavaScript boxes every place for free; natively the box is
    /// the emitter's to write.
    fn compute_boxed_bindings(&mut self) {
        let closures: Vec<Id> = self.program.closures.keys().copied().collect();
        for closure_id in closures {
            let Some(closure) = self.program.closures.get(&closure_id) else {
                continue;
            };
            let mut declared_inside = HashSet::new();
            let mut referenced = HashSet::new();
            let mut visited = HashSet::new();
            self.scan_closure(
                closure.return_,
                &mut declared_inside,
                &mut referenced,
                &mut visited,
            );
            for binding in referenced {
                if declared_inside.contains(&binding) {
                    continue;
                }
                if self
                    .program
                    .variables
                    .get(&binding)
                    .is_some_and(|variable| variable.mutable)
                {
                    self.boxed.insert(binding);
                }
            }
        }
    }

    /// The native type a context's THREADED VALUE has — `let ambient_nursery:
    /// Context<Nursery>` carries it as the declared type's one argument.
    fn context_value_type(&mut self, context: Id, span: Span) -> Result<String, Error> {
        let name = self
            .program
            .variables
            .get(&context)
            .map(|variable| variable.name)
            .unwrap_or("a context");
        let declared = self
            .program
            .variables
            .get(&context)
            .map(|variable| variable.type_id)
            .ok_or_else(|| {
                unsupported(&format!("the context `{name}`, which has no type"), span)
            })?;
        let argument = match self.resolve(declared).cloned() {
            Some(Type::Struct(_, arguments)) => arguments.first().copied(),
            _ => None,
        };
        let Some(argument) = argument else {
            return Err(unsupported(
                &format!("the context `{name}`, whose value type did not resolve"),
                span,
            ));
        };
        self.rust_type(argument, span)
    }

    /// The native type one entry of a `context` clause contributes to a closure
    /// type (J6).
    ///
    /// A clause entry is a context BINDING id, and the parameter the pass
    /// appends for it carries that binding's value type — under an `Option` for
    /// the safe flavour. There is no parameter id to key the flavour on at the
    /// type level, so it is taken from the CLOSURES: every closure that lands in
    /// a clause position has a hidden parameter for the same context, and F23
    /// records each one's flavour where the pass minted it. Two closures that
    /// disagree would need two types, which is refused rather than guessed.
    fn context_clause_type(&mut self, context: Id, span: Span) -> Result<String, Error> {
        let name = self
            .program
            .variables
            .get(&context)
            .map(|variable| variable.name)
            .unwrap_or("a context");
        let mut settled: Option<bool> = None;
        for closure in self.program.closures.values() {
            for parameter_id in &closure.parameters {
                if self.program.context_hidden_parameters.get(parameter_id) != Some(&context) {
                    continue;
                }
                let optional = self
                    .program
                    .context_optional_hidden_parameters
                    .contains(parameter_id);
                if settled.is_some_and(|already| already != optional) {
                    return Err(unsupported(
                        &format!(
                            "a closure type carrying the context `{name}`, whose closures do \
                             not agree on whether the threaded value arrives as an `Option`"
                        ),
                        span,
                    ));
                }
                settled = Some(optional);
            }
        }
        // No closure in the program carries this context: nothing to disagree
        // with, and the clause names a value the callee will be handed bare.
        let optional = settled.unwrap_or(false);
        let value = self.context_value_type(context, span)?;
        Ok(if optional {
            format!("Option<{value}>")
        } else {
            value
        })
    }

    /// F25: `print` of a HOST HANDLE or of a value holding a function is
    /// refused where it is written, not where it runs.
    ///
    /// What node prints for either is its own object inspection —
    /// `Promise { <pending> }`, `[Function (anonymous)]`, `[Function: name]`
    /// depending on how the function was WRITTEN — and none of it is anything
    /// the language defines. Order 38 answered it with a runtime panic carrying
    /// the reason, which is honest and one release too late: the program that
    /// does it cannot work, so it is a compile-time refusal (the standing
    /// preference, F25's own recommendation).
    ///
    /// The test is the RENDERED type, which is where the two facts already
    /// live: every closure type is `Rc<dyn Fn(..) -> ..>` (F16) and every host
    /// handle is a `vilan_rt::executor::` or `vilan_rt::http::` path. A value
    /// that holds one NESTED — a `List` of structs each holding a closure —
    /// renders as neither, and the runtime panic in the generated `impl Js`
    /// stays as the backstop for it rather than being replaced by a walk that
    /// would have to chase every type this emitter can mint.
    fn refuse_unprintable(&mut self, argument_ids: &[Id], span: Span) -> Result<(), Error> {
        let Some(&argument) = argument_ids.first() else {
            return Ok(());
        };
        let Some(type_id) = self.type_of(argument) else {
            return Ok(());
        };
        // A type this emitter cannot render is refused by the render itself,
        // where the diagnosis is better; nothing to add here.
        let Ok(rendered) = self.rust_type(type_id, span) else {
            return Ok(());
        };
        if is_closure_type(&rendered) {
            return Err(unsupported("`print` of a value holding a function", span));
        }
        if let Some(handle) = host_handle_name(&rendered) {
            return Err(unsupported(
                &format!("`print` of the host handle `{handle}`"),
                span,
            ));
        }
        Ok(())
    }

    /// A closure's body, with the destructures a TUPLE PARAMETER owes in front
    /// of it (F18).
    ///
    /// `|(value, factor)| value * factor` has one parameter — an unnamed tuple —
    /// and the analyzer records a destructure per pattern in
    /// `Closure::parameter_destructures`, to run before the body. The emitter
    /// rendered the parameter and the body and dropped the destructures, so the
    /// body referred to bindings nothing declared; `destructuring.vl` was
    /// refused for the `let` form before this slice and became a rustc refusal
    /// the moment that form was admitted, which is how it was found.
    fn closure_body(
        &mut self,
        closure: &vilan_core::analyzer::Closure,
        depth: usize,
    ) -> Result<String, Error> {
        let body = self.expression(closure.return_, depth)?;
        if closure.parameter_destructures.is_empty() {
            return Ok(body);
        }
        let mut prefix = String::new();
        for destructure in &closure.parameter_destructures {
            let rendered = self.expression(*destructure, depth)?;
            let _ = write!(prefix, "{rendered}; ");
        }
        Ok(format!("{{ {prefix}{body} }}"))
    }

    /// Every name a closure's PARAMETER LIST introduces: the parameters
    /// themselves, plus the binders a tuple parameter's destructures bind.
    ///
    /// The second half is why this is a function. `|(value, factor)| ..` has one
    /// parameter — an unnamed tuple — and `value` and `factor` are declared by
    /// `Closure::parameter_destructures`, which the body walk never reaches, so
    /// every seed built from `closure.parameters` alone reads them as captures.
    /// Whether a closure's body evaluates to nothing — the test that says a
    /// dropped future loses no value. See the floating arm in
    /// [`Emitter::closure`].
    ///
    /// **Silence is a NO.** The first spelling of this asked the body tail's
    /// recorded type and read `None` as void, which is what every BLOCK-bodied
    /// closure answers — `adapt.vl`'s `|url| { …; length }` among them — so
    /// three closures whose callers read their value were floated and rustc
    /// refused the emission with `expected i32, found ()`. A future may only be
    /// dropped where there is provably nothing to drop, so every arm here has
    /// to say void POSITIVELY: a written annotation that is void, no `ret`
    /// carrying a value, and a tail that is either absent or typed void.
    fn closure_answers_void(&self, closure: &vilan_core::analyzer::Closure) -> bool {
        if let Some(type_id) = closure.return_type_id {
            return matches!(self.resolve(type_id), Some(Type::Void));
        }
        if closure.rets.iter().any(|(_, value)| value.is_some()) {
            return false;
        }
        let tail = match self.program.entity_map.get(&closure.return_) {
            Some(Expr::Block((_, tail))) => *tail,
            _ => closure.return_,
        };
        match self.program.entity_map.get(&tail) {
            // A block whose last statement carried a `;` has no tail at all.
            Some(Expr::Void) | None => true,
            _ => matches!(
                self.type_of(tail).and_then(|type_id| self.resolve(type_id)),
                Some(Type::Void)
            ),
        }
    }

    fn closure_parameter_bindings(&self, closure: &vilan_core::analyzer::Closure) -> HashSet<Id> {
        let mut bindings: HashSet<Id> = closure.parameters.iter().copied().collect();
        for destructure in &closure.parameter_destructures {
            if let Some(Expr::Destructure(_, pattern)) = self.program.entity_map.get(destructure) {
                collect_pattern_bindings_into(pattern, &mut bindings);
            }
        }
        bindings
    }

    /// Walks a closure body, collecting the bindings it DECLARES and the
    /// bindings it READS. The difference is what it captured.
    fn scan_closure(
        &self,
        expr_id: Id,
        declared: &mut HashSet<Id>,
        referenced: &mut HashSet<Id>,
        visited: &mut HashSet<Id>,
    ) {
        if !visited.insert(expr_id) {
            return;
        }
        match self.program.entity_map.get(&expr_id) {
            Some(Expr::Variable(binding)) => {
                declared.insert(*binding);
                if let Some(initial) = self
                    .program
                    .variables
                    .get(binding)
                    .and_then(|variable| variable.initial)
                {
                    self.scan_closure(initial, declared, referenced, visited);
                }
            }
            Some(Expr::Local(binding)) => {
                referenced.insert(*binding);
            }
            Some(other) => {
                // A `let` is not the only way a body introduces a name (F20, and
                // F18's destructuring `let`). A match leg's pattern, an `is`
                // test's capture, a destructure's pattern, a `for` binder and a
                // nested closure's parameters all declare INSIDE, and a walk
                // that missed them called them captures: the capture prelude
                // then emitted `let live = live.clone();` for a binding that
                // only exists inside the leg it is bound in, and
                // `future_closure_argument` — which emits one clone per capture
                // — did the same for the three `match` captures and the two
                // destructured names in `std::http`'s response loop.
                match other {
                    Expr::Match(_, legs) => {
                        for leg in legs {
                            collect_pattern_bindings_into(&leg.pattern, declared);
                        }
                    }
                    Expr::Is(_, pattern) | Expr::Destructure(_, pattern) => {
                        collect_pattern_bindings_into(pattern, declared);
                    }
                    Expr::ForEach(_, item, _) => declared.extend(item.iter().copied()),
                    Expr::Closure(closure_id) => {
                        if let Some(closure) = self.program.closures.get(closure_id) {
                            declared.extend(closure.parameters.iter().copied());
                        }
                    }
                    _ => {}
                }
                for child in self.children_of(expr_id) {
                    self.scan_closure(child, declared, referenced, visited);
                }
            }
            None => {}
        }
    }

    // ------------------------------------------------------- F31 liveness ---

    /// F31: computes, once per FUNCTION, which reads are their binding's last
    /// use — the answer [`Emitter::copy_a_consumed_place_read`] needs to
    /// downgrade a conservative copy to a move.
    ///
    /// The JS backend does not need this: rule 1's marking is made against a
    /// NAMED callee's parameter modes, so the positions this backend copies at
    /// (a closure call's arguments, a variant constructor's, a destructure's)
    /// carry no `clone_sites` decision at all and the JS emitter moves there for
    /// free. Natively the same read is a real move, so it was taken
    /// conservatively as a copy — which is right, and four copies a second
    /// backward look would not take.
    ///
    /// **The rule.** A read is a last use when no later read of the same
    /// binding follows it AND it is not deeper in a repeatable region than the
    /// declaration. The walk is forward and the candidate is overwritten, which
    /// is the same answer a backward walk gives for structured control flow and
    /// is written forward because `children_of` already enumerates a node's
    /// children in evaluation order.
    ///
    /// **Why it is sound where it is not complete.** A read in one branch of an
    /// `if` is "earlier" than one in the other, so the first keeps its copy and
    /// the second moves — each path moves once, which is what rustc asks. A
    /// read whose statement is unreachable on the path that took an early
    /// `return` is later in the walk, so the `return`'s own read keeps its copy.
    /// And a read inside a loop, a closure or a spawn is never a candidate
    /// unless its binding was declared inside the same region, because the
    /// region may run again for one declaration — the classic trap, and the one
    /// shape a "no later read" rule alone gets wrong.
    fn compute_liveness(&mut self, function_id: Id, statements: &[Id], tail: Id) {
        if !self.liveness_walked.insert(function_id) {
            return;
        }
        let mut state = Liveness::default();
        for statement in statements {
            self.walk_liveness(*statement, 0, &mut state);
        }
        self.walk_liveness(tail, 0, &mut state);
        for candidate in state.candidate.values().flatten() {
            self.last_uses.insert(*candidate);
        }
    }

    fn walk_liveness(&self, expr_id: Id, depth: usize, state: &mut Liveness) {
        match self.program.entity_map.get(&expr_id).cloned() {
            Some(Expr::Variable(binding)) => {
                // The INITIALIZER is read before the binding exists, so it is
                // walked first and the declaration recorded after it.
                if let Some(initial) = self
                    .program
                    .variables
                    .get(&binding)
                    .and_then(|variable| variable.initial)
                {
                    self.walk_liveness(initial, depth, state);
                }
                state.declared_at.insert(binding, depth);
            }
            Some(Expr::Local(binding) | Expr::Parameter(binding)) => {
                let declared = state.declared_at.get(&binding).copied().unwrap_or(0);
                let candidate = (depth <= declared).then_some(expr_id);
                state.candidate.insert(binding, candidate);
            }
            Some(other) => {
                // The binders a node introduces are declared at the depth its
                // BODY is walked at, which for a loop or a closure is one
                // deeper: a `for` binder is fresh on every iteration.
                let inner = depth + Self::repeats_its_body(&other) as usize;
                match &other {
                    Expr::ForEach(_, item, _) => {
                        for binding in item.iter() {
                            state.declared_at.insert(*binding, inner);
                        }
                    }
                    Expr::Closure(closure_id) => {
                        if let Some(closure) = self.program.closures.get(closure_id) {
                            for parameter in &closure.parameters {
                                state.declared_at.insert(*parameter, inner);
                            }
                        }
                    }
                    Expr::Match(_, legs) => {
                        let mut bound = HashSet::new();
                        for leg in legs {
                            collect_pattern_bindings_into(&leg.pattern, &mut bound);
                        }
                        for binding in bound {
                            state.declared_at.insert(binding, inner);
                        }
                    }
                    Expr::Is(_, pattern) | Expr::Destructure(_, pattern) => {
                        let mut bound = HashSet::new();
                        collect_pattern_bindings_into(pattern, &mut bound);
                        for binding in bound {
                            state.declared_at.insert(binding, inner);
                        }
                    }
                    _ => {}
                }
                // The ITERABLE of a `for .. in` is evaluated once, outside the
                // body — every other child of a repeating node is inside it.
                let iterable = match &other {
                    Expr::ForEach(iterable, _, _) => Some(*iterable),
                    _ => None,
                };
                for child in self.children_of(expr_id) {
                    let child_depth = if Some(child) == iterable {
                        depth
                    } else {
                        inner
                    };
                    self.walk_liveness(child, child_depth, state);
                }
            }
            None => {}
        }
    }

    /// Whether a node's body may run more than once for one entry — a loop, a
    /// closure (called as often as its holder likes), a spawn (which runs after
    /// the statement that made it), or a comprehension.
    fn repeats_its_body(expr: &Expr<'_>) -> bool {
        matches!(
            expr,
            Expr::For(_, _)
                | Expr::ForEach(_, _, _)
                | Expr::Closure(_)
                | Expr::Async(_)
                | Expr::TupleComprehension(_, _, _)
        )
    }

    /// Every sub-expression of `expr_id`, for the walks that only need to
    /// recurse. Written once rather than per walk: an arm this misses is a
    /// capture the box would not see, so there is exactly one list to keep.
    fn children_of(&self, expr_id: Id) -> Vec<Id> {
        let Some(expr) = self.program.entity_map.get(&expr_id) else {
            return Vec::new();
        };
        let mut children = Vec::new();
        match expr {
            Expr::Assignment(a, b) | Expr::Binary(_, a, b) | Expr::Index(a, b) => {
                children.push(*a);
                children.push(*b);
            }
            Expr::Async(a)
            | Expr::Await(a)
            | Expr::Unary(_, a)
            | Expr::Reference(a, _)
            | Expr::Dereference(a)
            | Expr::TryAssert(a)
            | Expr::Field(a, _, _)
            | Expr::TupleIndex(a, _, _)
            | Expr::ArrayLen(a, _)
            | Expr::Repeat(a, _) => children.push(*a),
            Expr::FunctionReturn(value) => children.extend(value.iter().copied()),
            Expr::Block((statements, tail)) => {
                children.extend(statements.iter().copied());
                children.push(*tail);
            }
            Expr::List(ids) | Expr::Tuple(ids) => children.extend(ids.iter().copied()),
            Expr::StructInitializer(_, fields) => children.extend(fields.values().copied()),
            Expr::For(condition, (statements, tail)) => {
                children.extend(condition.iter().copied());
                children.extend(statements.iter().copied());
                children.push(*tail);
            }
            Expr::ForEach(iterable, _, (statements, tail)) => {
                children.push(*iterable);
                children.extend(statements.iter().copied());
                children.push(*tail);
            }
            Expr::If(branch) => collect_if_children(branch, &mut children),
            Expr::Match(subject, legs) => {
                children.push(*subject);
                for leg in legs {
                    children.extend(leg.guard.iter().copied());
                    children.push(leg.body);
                }
            }
            Expr::Is(subject, _) | Expr::Destructure(subject, _) => children.push(*subject),
            Expr::Lift(subject, _, continuation) => {
                children.push(*subject);
                children.push(*continuation);
            }
            Expr::LiftRegion(steps, body) => {
                children.extend(steps.iter().map(|(step, _, _)| *step));
                children.push(*body);
            }
            Expr::TupleComprehension(a, b, c) => {
                children.push(*a);
                children.push(*b);
                children.push(*c);
            }
            Expr::Call(call_id) => {
                if let Some(function_call) = self.program.function_calls.get(call_id) {
                    children.push(function_call.subject_id);
                    children.extend(function_call.argument_ids.iter().copied());
                }
            }
            Expr::Closure(closure_id) => {
                if let Some(closure) = self.program.closures.get(closure_id) {
                    children.push(closure.return_);
                }
            }
            _ => {}
        }
        children
    }

    fn span_of(&self, id: Id) -> Span {
        self.program
            .span_map
            .get(&id)
            .map(|span| **span)
            .unwrap_or(Span::new((), 0..0))
    }

    // ----------------------------------------------- the substitution ---

    /// A type id under the active substitution: a bare `Generic` followed to
    /// what it is bound to, anything else left alone. The JS emitter's
    /// `resolve_type_id`, guard included — a substitution that binds a generic
    /// to ITSELF (which reconciling an impl's own parameter records) would
    /// otherwise loop forever.
    ///
    /// Only the HEAD is resolved here. A constructor-headed type is resolved
    /// structurally as it is rendered ([`Self::rust_type`] recurses into the
    /// arguments) and as it is keyed ([`Self::type_key`] does the same), which
    /// is the part the JS emitter has no use for and this one cannot do without.
    fn concrete(&self, type_id: TypeId) -> TypeId {
        let Some(_guard) = vilan_core::util::RecursionGuard::enter() else {
            return type_id;
        };
        // The substitution is keyed by CONSTRAINT id, and a type id is looked up
        // directly before its `Type` is consulted. The direct hop is not an
        // optimisation: a nominal declaration's parameter can arrive as the
        // constraint id ITSELF — `enum Tree<T>`'s `Leaf(T)` records T's
        // constraint id, whose `Type` is `Any`, where `struct Pair<A, B>`'s
        // fields record `Generic(..)` nodes — so a `Generic`-only lookup
        // resolves a generic struct's field and leaves a generic enum's payload
        // abstract. Type ids are deliberately NOT interned
        // (`type_id_for_type`), so a key can only ever be the parameter it was
        // minted for.
        if let Some(bound) = self.current_substitution.get(&type_id) {
            // A binding to ITSELF, which reconciling an impl's own parameter
            // records: following it would loop forever, so it stays abstract.
            if *bound != type_id {
                return self.concrete(*bound);
            }
            return type_id;
        }
        match self.program.type_id_to_type_map.get(&type_id) {
            Some(Type::Generic(constraint_id)) => {
                match self.current_substitution.get(constraint_id) {
                    Some(bound)
                        if !matches!(
                            self.program.type_id_to_type_map.get(bound),
                            Some(Type::Generic(other)) if other == constraint_id
                        ) =>
                    {
                        self.concrete(*bound)
                    }
                    _ => type_id,
                }
            }
            // `Self` inside a trait default, which the analyzer types as the
            // trait itself. Rewritten only to the traits THIS body's `Self`
            // satisfies ([`Self::current_self_traits`]).
            Some(Type::Trait(trait_id, _)) if self.current_self_traits.contains(trait_id) => {
                match self.current_self_type {
                    Some(self_type) if self_type != type_id => self.concrete(self_type),
                    _ => type_id,
                }
            }
            _ => type_id,
        }
    }

    /// The trait declaring `default_id`, closed over its supertraits — the set
    /// a default body's `Self` satisfies.
    fn self_traits_of(&self, default_id: Id) -> HashSet<Id> {
        let mut out = HashSet::new();
        let Some((trait_id, _)) = self
            .program
            .traits
            .iter()
            .find(|(_, trait_)| trait_.declarations.values().any(|id| *id == default_id))
        else {
            return out;
        };
        let mut stack = vec![*trait_id];
        while let Some(id) = stack.pop() {
            if !out.insert(id) {
                continue;
            }
            if let Some(trait_) = self.program.traits.get(&id) {
                for supertrait_type_id in &trait_.supertraits {
                    if let Some(Type::Trait(super_id, _)) =
                        self.program.type_id_to_type_map.get(supertrait_type_id)
                    {
                        stack.push(*super_id);
                    }
                }
            }
        }
        out
    }

    /// A stable key identifying a type under the active substitution — the
    /// instance memo's key, and the discriminator between two Rust types minted
    /// from one declaration.
    ///
    /// STRUCTURAL rather than id-keyed, for the reason `transformer.rs`'s
    /// `type_key` spells out (B95): a nominal `Type` carries its arguments as
    /// raw `TypeId`s, and inference can mint two ids for one type, so a key that
    /// followed the ids would emit one body twice. It is written here rather
    /// than borrowed because it must resolve THROUGH the substitution at every
    /// level — the JS one resolves only where it is asked to.
    ///
    /// It never fails. A type this emitter cannot RENDER still has to be keyed,
    /// or two instances it will refuse would collide and the refusal would name
    /// the wrong one.
    fn type_key(&self, type_id: TypeId) -> String {
        let mut key = String::new();
        self.write_type_key(type_id, &mut key);
        key
    }

    fn write_type_key(&self, type_id: TypeId, out: &mut String) {
        let Some(_guard) = vilan_core::util::RecursionGuard::enter() else {
            out.push_str("...");
            return;
        };
        let type_id = self.concrete(type_id);
        let Some(type_) = self.program.type_id_to_type_map.get(&type_id) else {
            let _ = write!(out, "?{}", type_id.0);
            return;
        };
        match type_ {
            Type::Struct(id, arguments) => {
                let _ = write!(out, "S{}", id.0);
                self.write_key_arguments(arguments, out);
            }
            Type::Enum(id, arguments) => {
                let _ = write!(out, "E{}", id.0);
                self.write_key_arguments(arguments, out);
            }
            Type::Trait(id, arguments) => {
                let _ = write!(out, "T{}", id.0);
                self.write_key_arguments(arguments, out);
            }
            // A124 R3: a trait OBJECT keys apart from the trait it erases and
            // from every other instantiation of it. The backend refuses to
            // EMIT one (see `rust_type_inner`), but a key is read before a
            // type is rendered, and two keys that collided would make the
            // refusal name the wrong instance.
            Type::Dyn(id, arguments) => {
                let _ = write!(out, "D{}", id.0);
                self.write_key_arguments(arguments, out);
            }
            Type::Tuple(elements) => {
                out.push_str("Tup");
                self.write_key_arguments(elements, out);
            }
            Type::Closure(parameters, return_type_id, _) => {
                out.push_str("Fn");
                self.write_key_arguments(parameters, out);
                out.push_str("->");
                self.write_type_key(*return_type_id, out);
            }
            Type::Array(element_type_id, length) => {
                out.push_str("Arr[");
                self.write_type_key(*element_type_id, out);
                let _ = write!(out, ";{length}]");
            }
            // A generic the substitution did not reach is an ABSTRACT type, and
            // two distinct binders are two distinct abstract types — so this one
            // position stays id-keyed, exactly as the JS emitter's does.
            Type::Generic(constraint_id) => {
                let _ = write!(out, "G{}", constraint_id.0);
            }
            Type::Void => out.push_str("void"),
            other => {
                let _ = write!(out, "X{}", describe(other));
            }
        }
    }

    fn write_key_arguments(&self, arguments: &[TypeId], out: &mut String) {
        out.push('<');
        for argument in arguments {
            self.write_type_key(*argument, out);
            out.push(',');
        }
        out.push('>');
    }

    /// The bindings the active substitution provides for the generics a
    /// callee's signature mentions — for a generic call inside a monomorphized
    /// body whose type arguments come only from the enclosing instantiation, so
    /// the analyzer recorded no substitution of its own. The JS emitter's
    /// `inherited_substitution`.
    fn inherited_substitution(&self, target_id: Id) -> HashMap<TypeId, TypeId> {
        if self.current_substitution.is_empty() {
            return HashMap::default();
        }
        let Some(function) = self.program.functions.get(&target_id) else {
            return HashMap::default();
        };
        let mut generics = mono::signature_generics(self.program, target_id);
        // A generic parameter the SIGNATURE does not mention still has to be
        // bound when the enclosing instantiation can bind it: `fun make<T>():
        // T` is covered by the return type, but `T::describe()` inside a body
        // whose `T` is the caller's own is not reachable from either list.
        for constraint_id in &function.generic_parameter_constraint_ids {
            if !generics.contains(constraint_id) {
                generics.push(*constraint_id);
            }
        }
        generics
            .into_iter()
            .filter_map(|constraint_id| {
                self.current_substitution
                    .get(&constraint_id)
                    .map(|type_id| (constraint_id, *type_id))
            })
            .collect()
    }

    /// The generic binding to monomorphize a call's callee with, from whichever
    /// channel carries it — the written type arguments (`id<i32>`), the
    /// receiver/own-generic substitution the analyzer recorded, or the inherited
    /// slice of the active one. The JS emitter's `call_substitution`, B192's
    /// merge included: a written list is a PREFIX laid OVER the recorded
    /// substitution, never a replacement for it.
    fn call_substitution(
        &self,
        call_id: Id,
        target_id: Id,
        generic_argument_ids: &[TypeId],
    ) -> HashMap<TypeId, TypeId> {
        let function = self.program.functions.get(&target_id);
        let is_generic =
            function.is_some_and(|function| !function.generic_parameter_constraint_ids.is_empty());
        let mut substitution = match self.program.method_call_substitution.get(&call_id) {
            Some(recorded) => recorded.clone(),
            None => self.inherited_substitution(target_id),
        };
        if is_generic && !generic_argument_ids.is_empty() {
            for (constraint_id, argument_id) in function
                .expect("a generic callee is a function")
                .generic_parameter_constraint_ids
                .iter()
                .copied()
                .zip(generic_argument_ids.iter().copied())
            {
                substitution.insert(constraint_id, argument_id);
            }
        }
        // The call's OWN generic values, recorded positionally by the analyzer
        // where the written-argument channel carries nothing (a bound method
        // reached through a trait surface).
        if let Some(values) = self.program.own_generic_call_bindings.get(&call_id)
            && let Some(function) = function
        {
            for (constraint_id, value) in function
                .generic_parameter_constraint_ids
                .iter()
                .zip(values.iter())
            {
                substitution.insert(*constraint_id, *value);
            }
        }
        substitution
    }

    /// The substitution a trait DEFAULT body is specialized under: the trait's
    /// own generic parameters bound to the arguments `type_id` implements the
    /// trait at, plus the providing impl's binders bound from the concrete
    /// receiver. The JS emitter's `trait_parameter_substitution` (B58).
    fn trait_parameter_substitution(
        &self,
        default_id: Id,
        type_id: TypeId,
    ) -> HashMap<TypeId, TypeId> {
        mono::trait_parameter_substitution(self.program, None, default_id, type_id)
    }

    /// Composes `entries` onto the substitution in force and installs the
    /// result, answering the old one for the caller to restore.
    ///
    /// COMPOSES rather than replaces, for B244's reason: a bound type that is
    /// constructor-headed with a generic inside (`Option<T>`) cannot be grounded
    /// at the point of binding, so dropping the outer entries strands that inner
    /// `T`. Keeping the ones the inner entries do not shadow leaves the chain
    /// walkable, and [`Self::concrete`] already follows a binding to a binding.
    fn enter_substitution(&mut self, entries: Vec<(TypeId, TypeId)>) -> HashMap<TypeId, TypeId> {
        let mut composed = self.current_substitution.clone();
        composed.extend(entries);
        std::mem::replace(&mut self.current_substitution, composed)
    }

    /// `substitution`'s entries with every bound type resolved under the
    /// substitution in force, ordered by constraint id — the memo key's input,
    /// and the composition's.
    fn resolved_entries(&self, substitution: &HashMap<TypeId, TypeId>) -> Vec<(TypeId, TypeId)> {
        let mut entries: Vec<(TypeId, TypeId)> = substitution
            .iter()
            .map(|(constraint_id, type_id)| (*constraint_id, self.concrete(*type_id)))
            .collect();
        entries.sort_by_key(|(constraint_id, _)| constraint_id.0);
        entries
    }

    // ------------------------------------------------------------- names ---

    fn binding_name(&self, id: Id) -> String {
        // J6: a context-threaded hidden parameter has no `parameters` record to
        // take a name from — it is not source. Its name says what it is, so an
        // emitted signature carrying one reads honestly.
        if self.program.context_hidden_parameters.contains_key(&id) {
            return format!("context_{}", id.0);
        }
        let name = self
            .program
            .variables
            .get(&id)
            .map(|variable| variable.name)
            .or_else(|| {
                self.program
                    .parameters
                    .get(&id)
                    .map(|parameter| parameter.name)
            })
            .unwrap_or("x");
        if name == "self" {
            return "this".to_string();
        }
        format!("{}_{}", sanitize(name), id.0)
    }

    // ------------------------------------------------------------- types ---

    /// What a type IS, under the substitution in force. Every "what shape is
    /// this" question in the walk goes through here, so a generic parameter is
    /// followed to its binding in ONE place — a second spelling that forgot to
    /// is how a monomorphized body comes to ask about `T` instead of `i32`.
    fn resolve(&self, type_id: TypeId) -> Option<&Type> {
        self.program
            .type_id_to_type_map
            .get(&self.concrete(type_id))
    }

    fn rust_type(&mut self, type_id: TypeId, span: Span) -> Result<String, Error> {
        let rendered = self.rust_type_inner(type_id, span);
        match rendered {
            Err(error) if self.census => {
                self.host_gaps.insert(census_entry(&error));
                Ok("()".to_string())
            }
            other => other,
        }
    }

    fn rust_type_inner(&mut self, type_id: TypeId, span: Span) -> Result<String, Error> {
        let Some(_guard) = vilan_core::util::RecursionGuard::enter() else {
            return Err(unsupported("a type nested past the recursion guard", span));
        };
        let type_id = self.concrete(type_id);
        let Some(resolved) = self.resolve(type_id).cloned() else {
            return Err(unsupported("a value whose type did not resolve", span));
        };
        match resolved {
            Type::Void => Ok("()".to_string()),
            Type::Tuple(elements) => {
                let mut parts = Vec::new();
                for element in &elements {
                    parts.push(self.rust_type(*element, span)?);
                }
                Ok(format!("({})", parts.join(", ")))
            }
            Type::Array(element, length) => {
                let element = self.rust_type(element, span)?;
                Ok(format!("[{element}; {length}]"))
            }
            Type::Closure(parameters, return_type, contexts) => {
                // Read and CLEAR: the flag belongs to this position's outermost
                // closure, not to a closure nested inside its own signature.
                let is_async = std::mem::take(&mut self.expects_async);
                let mut parts = Vec::new();
                for parameter in &parameters {
                    parts.push(self.rust_type(*parameter, span)?);
                }
                // B309: the `context` clause is part of the TYPE, and
                // `context::thread_contexts` appends one hidden parameter per
                // clause entry to every closure that lands in this position —
                // so the type has to name them or the emitted `Fn` has the
                // wrong arity. The value type is the context binding's, under
                // an `Option` for the safe flavour, and the flavour is the one
                // the closures themselves settled.
                for context in &contexts {
                    parts.push(self.context_clause_type(*context, span)?);
                }
                let returned = self.rust_type(return_type, span)?;
                // F16, applied as ruled: a closure VALUE that can reach a
                // storing position is counted. A closure TYPE written in a
                // signature or a field is exactly such a position — the only
                // closure that is provably call-only is a parameter this
                // emitter can see the body of, and `parameter_type` decides
                // that one. Everything else is `Rc<dyn Fn>`.
                // An `async |T| U` answers a FUTURE, and the call site awaits
                // it — which is what `Boxed<U>` is for. `std::http`'s
                // `Server.request_handler` is the shape, and without this the
                // emitted `.await` met a plain `Response`.
                let returned = if is_async {
                    format!("vilan_rt::executor::Boxed<{returned}>")
                } else {
                    returned
                };
                Ok(format!(
                    "std::rc::Rc<dyn Fn({}) -> {}>",
                    parts.join(", "),
                    returned
                ))
            }
            Type::Struct(id, arguments) => self.nominal_struct(id, &arguments, span),
            Type::Enum(id, arguments) => self.nominal_enum(id, &arguments, span),
            // F18 slice 2: `any`. It exists for `std::db`'s bind list, and
            // `vilan_rt::Any` says what its scope is — not a dynamic type
            // system, just the value type a heterogeneous list needs where the
            // JS backend has a bare array.
            Type::Any => Ok("vilan_rt::Any".to_string()),
            // A124 R3 / F1: a `dyn` lowers natively to a FAT POINTER —
            // `vilan_rt::Dyn<dyn ObjectX>`, the counted value pointer beside
            // Rust's own vtable for the per-object trait this emitter writes,
            // the shape a closure already takes (`Rc<dyn Fn(..)>`).
            Type::Dyn(trait_id, arguments) => {
                let object = self.ensure_object_trait(trait_id, &arguments, span)?;
                Ok(format!("vilan_rt::Dyn<dyn {}>", object.name))
            }
            // A generic the substitution did not reach. The refusal names the
            // PARAMETER, because which one went unbound is the whole diagnosis
            // when it happens.
            Type::Generic(constraint_id) => Err(unsupported(
                &format!(
                    "a value of an unbound generic type parameter ({})",
                    self.generic_name(constraint_id)
                ),
                span,
            )),
            other => Err(unsupported(
                &format!("a value of type `{}`", describe(&other)),
                span,
            )),
        }
    }

    /// Which generic parameter went unbound, for a refusal whose whole
    /// diagnosis is that. Only a `trait` records its parameters' written NAMES,
    /// so everything else is named by its declaration and position — which is
    /// what a reader needs either way.
    fn generic_name(&self, constraint_id: TypeId) -> String {
        for trait_ in self.program.traits.values() {
            if let Some(position) = trait_
                .generic_parameter_constraint_ids
                .iter()
                .position(|id| *id == constraint_id)
            {
                return match trait_.generic_parameter_names.get(position) {
                    Some(name) => format!("`{name}` of trait `{}`", trait_.name),
                    None => format!("parameter {} of trait `{}`", position + 1, trait_.name),
                };
            }
        }
        let position_in = |constraints: &[TypeId]| {
            constraints
                .iter()
                .position(|id| *id == constraint_id)
                .map(|position| position + 1)
        };
        for function in self.program.functions.values() {
            if let Some(position) = position_in(&function.generic_parameter_constraint_ids) {
                return format!("parameter {position} of `{}`", function.name);
            }
        }
        for declaration in self.program.structs.values() {
            if let Some(position) = position_in(&declaration.generic_parameter_constraint_ids) {
                return format!("parameter {position} of struct `{}`", declaration.name);
            }
        }
        for declaration in self.program.enums.values() {
            if let Some(position) = position_in(&declaration.generic_parameter_constraint_ids) {
                return format!("parameter {position} of enum `{}`", declaration.name);
            }
        }
        format!("#{}", constraint_id.0)
    }

    /// Whether `any` appears ANYWHERE inside a type.
    ///
    /// Nested, because the openness that mints a second Rust type need not be
    /// at the top: `delta-law.vl` instantiates `SeqOp<List<any>>` beside
    /// `SeqOp<List<i32>>`, and a check that looked only at the outermost
    /// argument saw a grounded `List` and minted both.
    fn mentions_any(&self, type_id: TypeId) -> bool {
        let Some(_guard) = vilan_core::util::RecursionGuard::enter() else {
            return false;
        };
        match self.resolve(type_id) {
            Some(Type::Any) => true,
            Some(
                Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Tuple(arguments),
            ) => arguments
                .clone()
                .iter()
                .any(|inner| self.mentions_any(*inner)),
            Some(Type::Array(element, _)) => {
                let element = *element;
                self.mentions_any(element)
            }
            Some(Type::Closure(parameters, returns, _)) => {
                let (parameters, returns) = (parameters.clone(), *returns);
                parameters.iter().any(|inner| self.mentions_any(*inner))
                    || self.mentions_any(returns)
            }
            _ => false,
        }
    }

    /// A nominal type instantiated at `any` is REFUSED, even though `any`
    /// itself now renders (F18 slice 2).
    ///
    /// `Type::Any` is the analyzer's "open at this position", and a generic
    /// whose argument is open is the B357 shape: the site records `SeqOp<any>`
    /// while the binding beside it is `SeqOp<i32>`, and grounding the open one
    /// mints a SECOND Rust type where the JS backend has one array. That is
    /// exactly what `delta-law.vl` and `generic-adapter-dispatch.vl` did the
    /// moment `any` stopped being a refusal — `Reset(Vec<vilan_rt::Any>)`
    /// handed a `Vec<i32>`.
    ///
    /// The check sits at the two MINT points themselves (`ensure_struct` and
    /// `ensure_enum`) rather than at `nominal_struct`/`nominal_enum`, because
    /// a variant CONSTRUCTOR reaches the mint through `variant_path` and would
    /// otherwise walk past it — which is exactly the site `delta-law.vl`
    /// failed at.
    ///
    /// Asking only where a type is about to be minted is what
    /// is what makes `List<any>` — the whole reason `any` renders — untouched:
    /// `List`, `Option` and `Result` are answered by name as `Vec<_>`,
    /// `Option<_>` and `Result<_, _>` and mint nothing, so `std::db`'s bind
    /// list works while a user aggregate at an open argument keeps the refusal
    /// it had.
    fn refuse_an_any_argument(&self, arguments: &[TypeId], span: Span) -> Result<(), Error> {
        if arguments
            .iter()
            .any(|argument| self.mentions_any(*argument))
        {
            return Err(unsupported(
                "a generic type instantiated at `any` (the site's argument is still open)",
                span,
            ));
        }
        Ok(())
    }

    fn nominal_struct(
        &mut self,
        id: Id,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<String, Error> {
        let Some(declaration) = self.program.structs.get(&id) else {
            return Err(unsupported("an unresolved struct", span));
        };
        let name = declaration.name;
        if let Some(scalar) = scalar_type(name) {
            return Ok(scalar.to_string());
        }
        let external = declaration.external;
        let mut rendered = Vec::new();
        for argument in arguments {
            rendered.push(self.rust_type(*argument, span)?);
        }
        // A name is only a RUNTIME type when std declared it `external`. The
        // shortcuts below used to be keyed on the name alone, which quietly
        // claimed three vilan STRUCTS that merely share a name with a runtime
        // one: `Map<K, V>` and `Set<T>` are I1's wrappers over the raw
        // `NativeMap` (they hold the original key beside the value so `keys()`
        // answers real `K`s), and `SignalCell<T>` is a pair of `Shared`s with
        // its own `subscribers` list. Emitting `vilan_rt::Map` for the wrapper
        // dropped the wrapper's field and its methods' bodies then read a field
        // of a type that has none. No program reached it because every one of
        // the three needs `Hash` first, which was refused above — F20's whole
        // subject.
        if !external {
            return Ok(self.ensure_struct(id, arguments, span)?.name);
        }
        match name {
            "List" => Ok(format!(
                "Vec<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            // The raw JS `Map` (`native_map.vl`), whose key type is FIXED as
            // `Hash` rather than generic, so its one written argument is the
            // VALUE. The arity is why this cannot share `List`'s shape: a
            // `NativeMap<V>` is a `vilan_rt::Map<Hash, V>`.
            "NativeMap" => Ok(format!(
                "vilan_rt::Map<vilan_rt::Hash, {}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            // F20: the opaque canonical key (`hash.vl`'s `external struct
            // Hash`). `vilan_rt::Hash` documents why it has four arms.
            "Hash" => Ok("vilan_rt::Hash".to_string()),
            "Shared" => Ok(format!(
                "vilan_rt::Shared<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            // C14's back edge (`shared.vl`): `Weak<T>` names a cell without
            // keeping it alive. On JS it is the identity because nothing
            // counts; natively the count is real, which is the whole reason the
            // std surface answers an `Option`.
            "Weak" => Ok(format!(
                "vilan_rt::Weak<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            // J6: the four host types the executor IS.
            "Task" => Ok(format!(
                "vilan_rt::executor::Task<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            "Nursery" => Ok("vilan_rt::executor::Nursery".to_string()),
            "CancelSignal" => Ok("vilan_rt::executor::CancelSignal".to_string()),
            "TimerHandle" => Ok("vilan_rt::executor::TimerHandle".to_string()),
            // F18: the host types `vilan_rt::http` IS. `std::http` declares its
            // node handles as `external struct`s, exactly as `std::task`
            // declares `Task`, so they arrive here for the same reason and are
            // answered the same way. The `external` gate is above, so this arm
            // cannot claim a vilan struct that merely shares one of the names.
            _ if let Some(native) = http_host_type(name) => Ok(native.to_string()),
            // F18 slice 2: `std::db`'s three host types, which are
            // `vilan-rt-sqlite`'s — a SEPARATE crate (Order 39's R1), linked
            // only when a program reaches one of them. Naming one is what
            // records that reach: see [`Emitter::reaches_sqlite`].
            "Database" | "Statement" | "Row" => {
                self.reaches_sqlite = true;
                Ok(format!("vilan_rt_sqlite::{name}"))
            }
            // F32 (RULED (b), Order 40): `BigInt` is an `i128` natively — the
            // documented LIMIT. `vilan_rt::BigInt` says what that buys and
            // what it costs; it is a newtype and not a bare `i128` because
            // node prints a `BigInt` with its `n`.
            "BigInt" => Ok("vilan_rt::BigInt".to_string()),
            // F18 slice 2: `std::json`'s opaque host value. It stands apart
            // from the HTTP table because it is a different module's host
            // surface and because two of the HTTP bindings (`headers`,
            // `remoteAddress`) ANSWER one — the dependency runs this way and
            // not the other.
            "JsonValue" => Ok("vilan_rt::json::JsonValue".to_string()),
            _ => {
                let what = format!("the host type `{name}`");
                self.host_gap(what, span).map(|_| "()".to_string())
            }
        }
    }

    fn nominal_enum(&mut self, id: Id, arguments: &[TypeId], span: Span) -> Result<String, Error> {
        let Some(declaration) = self.program.enums.get(&id) else {
            return Err(unsupported("an unresolved enum", span));
        };
        let name = declaration.name;
        let mut rendered = Vec::new();
        for argument in arguments {
            rendered.push(self.rust_type(*argument, span)?);
        }
        // F20: a BACKED enum IS its backing value at runtime — a number or a
        // string, on both backends (`backed-enums.md` §3.5). So the TYPE is that
        // scalar, a variant is its literal, and a pattern is the same literal;
        // there is no Rust `enum` to declare, which is also why exhaustiveness
        // lands on the catch-all arm `match_expr` already writes (the JS side's
        // `__enum_trap`).
        if let Some(backing) = declaration.backing {
            return Ok(match backing {
                Backing::Int => "i32".to_string(),
                Backing::Str => "vilan_rt::Str".to_string(),
            });
        }
        match name {
            "bool" => Ok("bool".to_string()),
            "Option" => {
                // F21: a VIEW inside the payload. Rust's own `Option` takes a
                // reference payload without a declaration of its own, and the
                // lifetime a signature needs is elided from the single input
                // loan the projection came through — which is why this reaches
                // `Option` and not the minted enums, whose payload would need a
                // lifetime parameter written on the declaration.
                let view = match self.expects_payload_view.take() {
                    Some(true) => "&mut ",
                    Some(false) => "&",
                    None => "",
                };
                Ok(format!(
                    "Option<{view}{}>",
                    rendered
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "()".to_string())
                ))
            }
            "Result" => Ok(format!("Result<{}>", rendered.join(", "))),
            _ => Ok(self.ensure_enum(id, arguments, span)?.name),
        }
    }

    /// The substitution a nominal declaration's own body is written under: its
    /// generic parameters bound to the arguments this instantiation supplies.
    ///
    /// The arguments are resolved through the substitution in force FIRST, so a
    /// `Pair<T>` reached from inside a `T = i32` instance instantiates
    /// `Pair<i32>` rather than re-binding `T` to itself.
    fn nominal_entries(
        &self,
        parameters: &[TypeId],
        arguments: &[TypeId],
    ) -> Vec<(TypeId, TypeId)> {
        parameters
            .iter()
            .zip(arguments.iter())
            .map(|(parameter, argument)| (*parameter, self.concrete(*argument)))
            .collect()
    }

    /// Emits (or reuses) the Rust `struct` for one instantiation of `id`.
    ///
    /// `Pair<i32>` and `Pair<str>` are TWO Rust structs, so the memo is keyed by
    /// the concrete arguments and each instantiation carries its own name and
    /// its own `impl Js`. A non-generic declaration keys on an empty argument
    /// list and keeps its plain `{Name}_{id}`, which is what leaves every
    /// program S1a already emitted byte-identical.
    fn ensure_struct(
        &mut self,
        id: Id,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<Reserved, Error> {
        let declaration = self.program.structs.get(&id).cloned().unwrap();
        self.refuse_an_any_argument(arguments, span)?;
        // The refusal comes BEFORE the once-only mark, or a first call that
        // swallowed the error would let a second one through on the mark alone
        // and emit a reference to a type nothing declared.
        if declaration.resource {
            return Err(unsupported(
                &format!(
                    "the `resource` type `{}` (destruction.md's teardown is a later slice)",
                    declaration.name
                ),
                span,
            ));
        }
        let entries =
            self.nominal_entries(&declaration.generic_parameter_constraint_ids, arguments);
        let key: Vec<String> = entries
            .iter()
            .map(|(_, type_id)| self.type_key(*type_id))
            .collect();
        if let Some(reserved) = self.type_instances.get(&(id, key.clone())) {
            return Ok(Reserved {
                name: reserved.name.clone(),
                slot: reserved.slot,
            });
        }
        let type_name = self.mint_type_name(declaration.name, id, &key);
        let slot = self.next_type_slot;
        self.next_type_slot += 1;
        self.type_instances.insert(
            (id, key),
            Reserved {
                name: type_name.clone(),
                slot,
            },
        );

        let saved = self.enter_substitution(entries);
        let rendered_types: Result<Vec<String>, Error> = declaration
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| {
                // A field declared `async |T| U` (J2's `async_fields`).
                self.expects_async = self.program.async_fields.contains(&(id, index));
                let rendered = self.rust_type(field.type_id, span);
                self.expects_async = false;
                rendered
            })
            .collect();
        self.current_substitution = saved;
        let rendered_types = rendered_types?;
        // Two questions, not one. `PartialEq` needs to know which FIELDS are
        // themselves closures (those compare by `ptr_eq`); `Js` and `Json` need
        // to know whether a closure is reachable at all, since a container of
        // them has no rendering either.
        // The question `PartialEq` asks is whether a closure is reachable in a
        // field AT ALL, not whether the field IS one: `Rc<dyn Fn>` implements no
        // `PartialEq` at any depth, so `Option<|| void>` — `std::http`'s
        // `Server.upgrade_handler` — defeats the derive exactly as a bare
        // closure field does. `Js` and `Json` ask the same question, for the
        // same reason.
        let reaches_a_closure = rendered_types
            .iter()
            .any(|rendered| mentions_a_closure(rendered));
        for rendered in &rendered_types {
            Self::check_reference_equality(rendered, declaration.name, span)?;
        }

        let mut out = String::new();
        // A CLOSURE field is why `PartialEq` cannot simply be derived:
        // `Rc<dyn Fn>` does not implement it. The answer is not to drop
        // equality — a struct holding a callback is compared in real reactive
        // code — but to write the impl JavaScript's `===` already gives, which
        // for a function value is REFERENCE equality and for an `Rc` is
        // `ptr_eq`.
        if reaches_a_closure {
            let _ = writeln!(out, "#[derive(Clone)]");
        } else {
            let _ = writeln!(out, "#[derive(Clone, PartialEq)]");
        }
        let _ = writeln!(out, "struct {type_name} {{");
        for (field, rendered) in declaration.fields.iter().zip(rendered_types.iter()) {
            let _ = writeln!(out, "    {}: {rendered},", sanitize(field.name));
        }
        let _ = writeln!(out, "}}");
        if reaches_a_closure {
            let comparisons: Vec<String> = declaration
                .fields
                .iter()
                .zip(rendered_types.iter())
                .map(|(field, rendered)| {
                    let name = sanitize(field.name);
                    Self::compare_fields(
                        rendered,
                        &format!("self.{name}"),
                        &format!("other.{name}"),
                    )
                })
                .collect();
            let body = if comparisons.is_empty() {
                "true".to_string()
            } else {
                comparisons.join(" && ")
            };
            let _ = writeln!(out, "impl PartialEq for {type_name} {{");
            let _ = writeln!(out, "    fn eq(&self, other: &Self) -> bool {{");
            let _ = writeln!(out, "        {body}");
            let _ = writeln!(out, "    }}");
            let _ = writeln!(out, "}}");
        }
        // `print(value)` is `console.log`, and on the JS backend a struct value
        // IS its flat field array — so a struct prints as `[ a, b ]`. The
        // rendering is emitted beside the declaration rather than derived,
        // because `vilan_rt` cannot name a type the emitter just invented.
        let _ = writeln!(out, "impl vilan_rt::Js for {type_name} {{");
        let _ = writeln!(out, "    fn js(&self) -> String {{");
        if reaches_a_closure {
            // Node prints a function value as `[Function (anonymous)]` or
            // `[Function: <name>]` depending on how it was WRITTEN, and the
            // emitted name of a gensym'd JS function is not a thing this
            // backend can reproduce. So printing such a value is refused — at
            // run time, loudly, by name, rather than by guessing a string the
            // differential would then disagree about. No corpus program does
            // it; a program that starts to gets this message.
            let _ = writeln!(
                out,
                "        vilan_rt::panic_with(\"the rust backend cannot print a value holding \\
                 a function\")"
            );
        } else {
            let parts: Vec<String> = declaration
                .fields
                .iter()
                .map(|field| format!("self.{}.js_nested()", sanitize(field.name)))
                .collect();
            let _ = writeln!(out, "        vilan_rt::js_tuple(&[{}])", parts.join(", "));
        }
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "}}");
        // F20: `JSON.stringify` is a SECOND rendering, not `Js` with different
        // spacing, and the canonical hash of an aggregate IS its
        // `JSON.stringify` text — so `[derive(Hashable)]` on a struct needs it.
        // It is written beside `impl Js` for the same reason `impl Js` is
        // written here at all: `vilan_rt` cannot name a type the emitter just
        // invented.
        let field_json: Vec<String> = declaration
            .fields
            .iter()
            .map(|field| format!("self.{}.json()", sanitize(field.name)))
            .collect();
        out.push_str(&Self::json_impl(
            &type_name,
            reaches_a_closure,
            &format!("vilan_rt::json_array(&[{}])", field_json.join(", ")),
        ));
        self.types.insert(slot, out);
        Ok(Reserved {
            name: type_name,
            slot,
        })
    }

    fn ensure_enum(&mut self, id: Id, arguments: &[TypeId], span: Span) -> Result<Reserved, Error> {
        let declaration = self.program.enums.get(&id).cloned().unwrap();
        self.refuse_an_any_argument(arguments, span)?;
        if declaration.resource {
            return Err(unsupported(
                &format!(
                    "the `resource` enum `{}` (destruction.md's teardown is a later slice)",
                    declaration.name
                ),
                span,
            ));
        }
        let entries =
            self.nominal_entries(&declaration.generic_parameter_constraint_ids, arguments);
        let key: Vec<String> = entries
            .iter()
            .map(|(_, type_id)| self.type_key(*type_id))
            .collect();
        if let Some(reserved) = self.type_instances.get(&(id, key.clone())) {
            return Ok(Reserved {
                name: reserved.name.clone(),
                slot: reserved.slot,
            });
        }
        let type_name = self.mint_type_name(declaration.name, id, &key);
        let slot = self.next_type_slot;
        self.next_type_slot += 1;
        self.type_instances.insert(
            (id, key),
            Reserved {
                name: type_name.clone(),
                slot,
            },
        );

        let saved = self.enter_substitution(entries);
        let mut rendered: Result<Vec<String>, Error> = Ok(Vec::new());
        // The payload types per variant, kept beside the rendered declaration
        // because `PartialEq`, `Js` and `Json` all have to ask the same question
        // of them that a struct asks of its fields: is a closure reachable?
        let mut payload_types: Vec<Vec<String>> = Vec::new();
        for variant in &declaration.variants {
            let mut payload = Vec::new();
            for data_type_id in &variant.data_type_ids {
                match self.rust_type(*data_type_id, span) {
                    Ok(one) => payload.push(one),
                    Err(error) => {
                        rendered = Err(error);
                        break;
                    }
                }
            }
            match &mut rendered {
                Ok(variants) if payload.is_empty() => {
                    variants.push(format!("    {},", sanitize(variant.name)));
                    payload_types.push(payload);
                }
                Ok(variants) => {
                    variants.push(format!(
                        "    {}({}),",
                        sanitize(variant.name),
                        payload.join(", ")
                    ));
                    payload_types.push(payload);
                }
                Err(_) => break,
            }
        }
        self.current_substitution = saved;
        let variants = rendered?;
        // A closure in a PAYLOAD is the struct's closure-holding field, one
        // level along: `std::http`'s `ResponseBody::Stream(|| Bytes)` defeats
        // the derive and has no rendering, and `ensure_struct` has guarded both
        // since S1a while this one guarded neither.
        let reaches_a_closure = payload_types
            .iter()
            .flatten()
            .any(|rendered| mentions_a_closure(rendered));
        for rendered in payload_types.iter().flatten() {
            Self::check_reference_equality(rendered, declaration.name, span)?;
        }

        let mut out = String::new();
        if reaches_a_closure {
            let _ = writeln!(out, "#[derive(Clone)]");
        } else {
            let _ = writeln!(out, "#[derive(Clone, PartialEq)]");
        }
        let _ = writeln!(out, "enum {type_name} {{");
        for variant in variants {
            let _ = writeln!(out, "{variant}");
        }
        let _ = writeln!(out, "}}");
        if reaches_a_closure {
            let mut legs = String::new();
            for (index, variant) in declaration.variants.iter().enumerate() {
                let name = sanitize(variant.name);
                let slots = payload_types.get(index).map(Vec::len).unwrap_or(0);
                if slots == 0 {
                    let _ = writeln!(
                        legs,
                        "            ({type_name}::{name}, {type_name}::{name}) => true,"
                    );
                    continue;
                }
                let left: Vec<String> = (0..slots).map(|slot| format!("a{slot}")).collect();
                let right: Vec<String> = (0..slots).map(|slot| format!("b{slot}")).collect();
                let comparisons: Vec<String> = (0..slots)
                    .map(|slot| {
                        Self::compare_fields(&payload_types[index][slot], &left[slot], &right[slot])
                    })
                    .collect();
                let _ = writeln!(
                    legs,
                    "            ({type_name}::{name}({}), {type_name}::{name}({})) => {},",
                    left.join(", "),
                    right.join(", "),
                    comparisons.join(" && ")
                );
            }
            let _ = writeln!(out, "impl PartialEq for {type_name} {{");
            let _ = writeln!(out, "    fn eq(&self, other: &Self) -> bool {{");
            let _ = writeln!(out, "        match (self, other) {{");
            out.push_str(&legs);
            let _ = writeln!(out, "            _ => false,");
            let _ = writeln!(out, "        }}");
            let _ = writeln!(out, "    }}");
            let _ = writeln!(out, "}}");
        }
        let _ = writeln!(out, "impl vilan_rt::Js for {type_name} {{");
        let _ = writeln!(out, "    fn js(&self) -> String {{");
        if reaches_a_closure {
            // A struct holding a callback refuses to PRINT for the reason
            // written at `ensure_struct`: node prints a function value off a
            // name this backend cannot reproduce. An enum payload is the same
            // value in a different place.
            let _ = writeln!(
                out,
                "        vilan_rt::panic_with(\"the rust backend cannot print a value holding \\
                 a function\")"
            );
            let _ = writeln!(out, "    }}");
            let _ = writeln!(out, "}}");
            out.push_str(&Self::json_impl(&type_name, true, ""));
            self.types.insert(slot, out);
            return Ok(Reserved {
                name: type_name,
                slot,
            });
        }
        let _ = writeln!(out, "        match self {{");
        for (index, variant) in declaration.variants.iter().enumerate() {
            let name = sanitize(variant.name);
            if variant.data_type_ids.is_empty() {
                let _ = writeln!(
                    out,
                    "            {type_name}::{name} => vilan_rt::js_tuple(&[\"{index}\".to_string()]),"
                );
            } else {
                let binders: Vec<String> = (0..variant.data_type_ids.len())
                    .map(|slot| format!("p{slot}"))
                    .collect();
                let mut parts = vec![format!("\"{index}\".to_string()")];
                parts.extend(binders.iter().map(|binder| format!("{binder}.js_nested()")));
                let _ = writeln!(
                    out,
                    "            {type_name}::{name}({}) => vilan_rt::js_tuple(&[{}]),",
                    binders.join(", "),
                    parts.join(", ")
                );
            }
        }
        let _ = writeln!(out, "        }}");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "}}");
        // F20, the enum half: an enum's JS value is `[index, ...data]`, so its
        // `JSON.stringify` text is that array's — `[0,7]` for `Leaf(7)`.
        let mut body = String::from("match self {\n");
        for (index, variant) in declaration.variants.iter().enumerate() {
            let name = sanitize(variant.name);
            let binders: Vec<String> = (0..variant.data_type_ids.len())
                .map(|slot| format!("p{slot}"))
                .collect();
            let mut parts = vec![format!("\"{index}\".to_string()")];
            parts.extend(binders.iter().map(|binder| format!("{binder}.json()")));
            let pattern = if binders.is_empty() {
                format!("{type_name}::{name}")
            } else {
                format!("{type_name}::{name}({})", binders.join(", "))
            };
            let _ = writeln!(
                &mut body,
                "            {pattern} => vilan_rt::json_array(&[{}]),",
                parts.join(", ")
            );
        }
        body.push_str("        }");
        out.push_str(&Self::json_impl(&type_name, false, &body));
        self.types.insert(slot, out);
        Ok(Reserved {
            name: type_name,
            slot,
        })
    }

    /// How one field or payload slot of a closure-reaching aggregate compares.
    ///
    /// A field that IS a closure is `Rc::ptr_eq` — JavaScript's `===` on a
    /// function value. A field that merely CONTAINS one goes through
    /// `vilan_rt::reference_eq`, which is the same answer through an `Option` or
    /// a `Vec`. Everything else is ordinary equality, because a struct holding
    /// one callback still compares its other fields by value.
    fn compare_fields(rendered: &str, left: &str, right: &str) -> String {
        if is_closure_type(rendered) {
            return format!("std::rc::Rc::ptr_eq(&{left}, &{right})");
        }
        if mentions_a_closure(rendered) && !compares_by_cell_identity(rendered) {
            return format!("vilan_rt::reference_eq(&{left}, &{right})");
        }
        format!("{left} == {right}")
    }

    /// Refuses a closure-reaching field whose shape `vilan_rt::ReferenceEq` has
    /// no impl for, BY NAME, rather than emitting Rust that rustc will refuse.
    ///
    /// The trait covers an `Rc` at any depth under an `Option` or a `Vec`, which
    /// is every shape std writes. A `Map<K, || void>` is not one of them and
    /// says so here.
    fn check_reference_equality(rendered: &str, owner: &str, span: Span) -> Result<(), Error> {
        if !mentions_a_closure(rendered)
            || compares_by_cell_identity(rendered)
            || Self::reference_equality_reaches(rendered)
        {
            return Ok(());
        }
        Err(unsupported(
            &format!(
                "a closure held inside `{rendered}` (in `{owner}`), which has no reference \
                 equality — `Option` and `List` of one do"
            ),
            span,
        ))
    }

    fn reference_equality_reaches(rendered: &str) -> bool {
        if rendered.starts_with("std::rc::Rc<") {
            return true;
        }
        for wrapper in ["Option<", "Vec<"] {
            if let Some(inner) = rendered.strip_prefix(wrapper)
                && let Some(inner) = inner.strip_suffix('>')
            {
                return Self::reference_equality_reaches(inner);
            }
        }
        false
    }

    /// One emitted aggregate's `impl vilan_rt::Json`.
    ///
    /// `holds_a_closure` is the same refusal `impl Js` takes for the same
    /// reason: `JSON.stringify` of a function is `undefined` in a field position
    /// and OMITS the key, which is not a shape this backend reproduces by
    /// guessing. A program that hashes a value holding a callback says so at run
    /// time rather than keying on a string the differential would then disagree
    /// about. No corpus program does it.
    fn json_impl(type_name: &str, holds_a_closure: bool, body: &str) -> String {
        let mut out = String::new();
        let _ = writeln!(out, "impl vilan_rt::Json for {type_name} {{");
        let _ = writeln!(out, "    fn json(&self) -> String {{");
        if holds_a_closure {
            let _ = writeln!(
                out,
                "        vilan_rt::panic_with(\"the rust backend cannot hash or serialize a \\
                 value holding a function\")"
            );
        } else {
            let _ = writeln!(out, "        {body}");
        }
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "}}");
        out
    }

    /// The Rust name for one instantiation of a nominal declaration.
    ///
    /// A non-generic one is `{Name}_{id}` — unchanged from S1a, so every
    /// program the previous slice emitted still emits the same bytes. An
    /// instantiation appends a per-emitter sequence number rather than the
    /// structural key, because the key is long, contains `<`/`,` and would make
    /// rustc's own messages unreadable.
    fn mint_type_name(&self, name: &str, id: Id, key: &[String]) -> String {
        if key.is_empty() {
            return format!("{}_{}", sanitize(name), id.0);
        }
        let mut sequence = 0;
        for (existing, _) in self.type_instances.keys() {
            if *existing == id {
                sequence += 1;
            }
        }
        format!("{}_{}_{}", sanitize(name), id.0, sequence)
    }

    fn type_of(&self, expr_id: Id) -> Option<TypeId> {
        if let Some(type_id) = self.program.expr_type_ids.get(&expr_id) {
            return Some(*type_id);
        }
        match self.program.entity_map.get(&expr_id)? {
            Expr::Local(binding) | Expr::Variable(binding) => self
                .program
                .variables
                .get(binding)
                .map(|variable| variable.type_id)
                .or_else(|| {
                    self.program
                        .parameters
                        .get(binding)
                        .map(|parameter| parameter.type_id)
                }),
            Expr::Parameter(binding) => self
                .program
                .parameters
                .get(binding)
                .map(|parameter| parameter.type_id),
            Expr::Call(call_id) => self
                .program
                .inferred_return_types
                .get(call_id)
                .copied()
                .or_else(|| self.declared_return_type(*call_id)),
            Expr::Await(awaited) => self.awaited_type(*awaited),
            _ => None,
        }
    }

    /// The type an `await` produces (J6): a `Task<T>`'s payload.
    ///
    /// `(await pending).id` reads a field off the await, and the await
    /// expression carries no type of its own — `pending` carries `Task<Row>`
    /// and the field is `Row`'s. An operand that is already the payload (an
    /// implicitly-awaited call, whose recorded type is its declared return
    /// type) passes through unchanged.
    fn awaited_type(&self, awaited: Id) -> Option<TypeId> {
        let type_id = self.type_of(awaited)?;
        match self.resolve(type_id)? {
            Type::Struct(struct_id, arguments)
                if self
                    .program
                    .structs
                    .get(struct_id)
                    .is_some_and(|declaration| declaration.name == "Task") =>
            {
                arguments.first().copied()
            }
            _ => Some(type_id),
        }
    }

    /// The DECLARED return type of a call's callee — what
    /// [`Emitter::type_of`] falls back to when the solver banked no inferred
    /// return for the call site.
    ///
    /// `inferred_return_types` is keyed by call and filled where inference had
    /// something to add; a monomorphic callee with a written return type adds
    /// nothing, so a field read straight off such a call (`fetch_row().id`) had
    /// no subject type at all and was refused. The declaration is the answer at
    /// exactly those sites.
    fn declared_return_type(&self, call_id: Id) -> Option<TypeId> {
        let call = self.program.function_calls.get(&call_id)?;
        let Some(Expr::Local(target)) = self.program.entity_map.get(&call.subject_id) else {
            return None;
        };
        self.program.functions.get(target)?.return_type_id
    }

    // --------------------------------------------------------- functions ---

    /// Emits (or reuses) ONE instance of `id`, specialized by `substitution`.
    ///
    /// This is the single monomorphisation path — a free function, an impl
    /// member, an operator's method, a nested generic call all come through
    /// here, because a binding recorded in one channel and read in another is
    /// how the two halves of a monomorphisation come to disagree. The memo key
    /// is (function, the structural keys of the bound types); the reservation is
    /// made BEFORE the body is walked, so a recursive body can call itself.
    ///
    /// A non-generic function reached with an empty substitution keys on an
    /// empty list and keeps its plain `{name}_{id}`, which is what leaves S1a's
    /// emitted programs byte-identical.
    fn ensure_function(
        &mut self,
        id: Id,
        substitution: &HashMap<TypeId, TypeId>,
    ) -> Result<Reserved, Error> {
        self.ensure_function_with_bits(id, substitution, &[])
    }

    /// [`Self::ensure_function`] for an ADAPTED instance (F22): the same
    /// monomorphisation path, keyed on the async bits as well as the types.
    ///
    /// The bits join the type key rather than sitting beside it, so one lookup
    /// still answers "have I emitted this instance", and a function reached
    /// with no bits keys exactly as it did before — which is what leaves every
    /// program that writes no async closure byte-identical.
    fn ensure_function_with_bits(
        &mut self,
        id: Id,
        substitution: &HashMap<TypeId, TypeId>,
        bits: &[Id],
    ) -> Result<Reserved, Error> {
        let function =
            self.program.functions.get(&id).cloned().ok_or_else(|| {
                unsupported("a call to a function with no body", self.span_of(id))
            })?;
        let span = function.name_span;
        // Every entry of the binding THIS call carries belongs in the key — the
        // function's own parameters and the impl binders the analyzer recorded
        // beside them, both of which the body can read. The caller's unrelated
        // bindings are not in it: composition keeps them REACHABLE
        // ([`Self::enter_substitution`]) without keying on them, which is what
        // stops one body per caller.
        let entries: Vec<(TypeId, TypeId)> = self.resolved_entries(substitution);
        let mut key: Vec<String> = entries
            .iter()
            .map(|(_, type_id)| self.type_key(*type_id))
            .collect();
        if !bits.is_empty() {
            key.push(format!(
                "async({})",
                bits.iter()
                    .map(|bit| bit.0.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
        if let Some(reserved) = self.instances.get(&(id, key.clone())) {
            return Ok(Reserved {
                name: reserved.name.clone(),
                slot: reserved.slot,
            });
        }

        if !function.has_body {
            return Err(unsupported(
                &format!("the body-less function `{}`", function.name),
                span,
            ));
        }

        let is_main = self
            .program
            .scopes
            .get(&self.program.global_scope_id)
            .and_then(|scope| scope.name_to_id_map.get("main"))
            == Some(&id);

        let name = if is_main {
            "main".to_string()
        } else {
            self.mint_function_name(function.name, id, &key)
        };
        let slot = self.next_function_slot;
        self.next_function_slot += 1;
        self.instances.insert(
            (id, key),
            Reserved {
                name: name.clone(),
                slot,
            },
        );

        let saved = self.enter_substitution(entries);
        let saved_instance = self.enter_instance(id, bits.to_vec());
        let emitted = self.function_body(&function, span, is_main, &name);
        self.restore_instance(saved_instance);
        self.current_substitution = saved;
        let out = emitted?;
        self.functions.insert(slot, out);
        Ok(Reserved { name, slot })
    }

    /// F22: swaps in the adapted-instance context a body is about to be
    /// emitted under, and answers the one it displaced.
    fn enter_instance(
        &mut self,
        function_id: Id,
        bits: Vec<Id>,
    ) -> (Vec<Id>, Option<AdaptedInstance>) {
        let info = self
            .program
            .adapted_instances
            .get(&(function_id, bits.clone()))
            .cloned();
        (
            std::mem::replace(&mut self.current_adapted_bits, bits),
            std::mem::replace(&mut self.current_instance, info),
        )
    }

    fn restore_instance(&mut self, saved: (Vec<Id>, Option<AdaptedInstance>)) {
        self.current_adapted_bits = saved.0;
        self.current_instance = saved.1;
    }

    /// F22: the async bits the CALLEE of `call_expr_id` must be emitted at, as
    /// this instance recorded them.
    fn callee_bits(&self, call_expr_id: Id) -> Vec<Id> {
        self.current_instance
            .as_ref()
            .and_then(|instance| instance.callee_bits.get(&call_expr_id))
            .cloned()
            .unwrap_or_default()
    }

    /// F21: whether this function returns a view inside an enum PAYLOAD, and
    /// whether that view is writable — the `Option<&mut T>` shape a
    /// view-returning `Arena::get` needs.
    ///
    /// Two records answer the first half between them, and neither alone.
    /// `borrows` is the projected parameter set — non-empty exactly when what
    /// comes back ALIASES a parameter — and `returns_view` is the signature's
    /// own statement that the RETURN TYPE is a view. A function with a
    /// projection and no view return type is projecting through something else,
    /// and the payload is the only thing it can be: `fun get_mut(&mut self):
    /// Option<&mut i32>` records `borrows = {0}` with both view flags false,
    /// where `fun same(x: &mut i32): &mut i32 borrows x` records `borrows = {0}`
    /// with both true.
    ///
    /// The second half — `&` or `&mut` — is read off the CONSTRUCTION, because
    /// nothing in the signature's records distinguishes `Option<&i32>` from
    /// `Option<&mut i32>` (`get` and `get_mut` have identical flags) and the
    /// receiver's own convention is the wrong answer for a `&mut self` method
    /// that hands back a read-only projection. A body constructs its return
    /// type's payload at one permission, since it has one return type.
    fn payload_view_of(&self, function: &vilan_core::analyzer::Function<'src>) -> Option<bool> {
        if function.borrows.is_empty() || function.returns_view || function.returns_mut_view {
            return None;
        }
        let mut found = None;
        let mut visited = HashSet::new();
        for statement in function
            .body
            .0
            .iter()
            .copied()
            .chain(std::iter::once(function.body.1))
        {
            self.find_payload_view(statement, &mut found, &mut visited);
        }
        found
    }

    /// The first variant construction in a body whose payload is a `&`/`&mut`,
    /// and which of the two it is.
    fn find_payload_view(&self, expr_id: Id, found: &mut Option<bool>, visited: &mut HashSet<Id>) {
        if found.is_some() || !visited.insert(expr_id) {
            return;
        }
        // The subject of a variant construction is a LOCAL that resolves to the
        // variant, not the variant itself — the same two hops
        // [`Emitter::call_expression`] takes.
        if let Some(Expr::Call(call_id)) = self.program.entity_map.get(&expr_id)
            && let Some(call) = self.program.function_calls.get(call_id)
            && let Some(Expr::Local(target)) = self.program.entity_map.get(&call.subject_id)
            && let Some(Expr::EnumVariant(_, _)) = self.program.entity_map.get(target)
        {
            for argument in &call.argument_ids {
                if let Some(Expr::Reference(_, mutable)) = self.program.entity_map.get(argument) {
                    *found = Some(*mutable);
                    return;
                }
            }
        }
        for child in self.children_of(expr_id) {
            self.find_payload_view(child, found, visited);
        }
    }

    /// One instance's signature and body, under the substitution already
    /// installed. Split out so `ensure_function` restores the substitution on
    /// the refusal path as well as the success one.
    fn function_body(
        &mut self,
        function: &vilan_core::analyzer::Function<'src>,
        span: Span,
        is_main: bool,
        name: &str,
    ) -> Result<String, Error> {
        // J6: asyncness is the INFERRED set, not the declared keyword — a
        // function whose body awaits is async whether or not it says so, and
        // `async_functions` is what the JS emitter reads for the same reason.
        // F22: an ADAPTED instance is async because an async closure reached
        // it, whether or not the declaration says so — the same question the
        // JS emitter asks of `AdaptedInstance::is_async`.
        let is_async = self.program.async_functions.contains(&function.id)
            || self
                .current_instance
                .as_ref()
                .is_some_and(|instance| instance.is_async);
        let mut parameters = Vec::new();
        for parameter_id in &function.parameters {
            parameters.push(self.parameter_declaration(*parameter_id, span)?);
        }
        let returned = match self.return_type_of(function) {
            Some(type_id) => {
                // A function whose DECLARED return type is `async |T| U`
                // (J2's `async_returning`).
                self.expects_async = self.program.async_returning.contains(&function.id);
                // F21: a view inside the returned PAYLOAD (`Option<&mut T>`),
                // which `returns_view` does not cover — see
                // [`Emitter::payload_view_of`].
                self.expects_payload_view = self.payload_view_of(function);
                let rendered = self.rust_type(type_id, span);
                self.expects_async = false;
                self.expects_payload_view = None;
                let rendered = rendered?;
                // The type system has no reference form — a `borrows` function's
                // return type IS its pointee's, and whether a view comes back is
                // recorded beside it. Without this the signature says `i32`
                // where the body hands back `&mut i32`.
                if function.returns_mut_view {
                    format!("&mut {rendered}")
                } else if function.returns_view {
                    format!("&{rendered}")
                } else {
                    rendered
                }
            }
            None => "()".to_string(),
        };

        // F31: the body's liveness, computed before it is walked and once per
        // function however many instantiations it is emitted at.
        self.compute_liveness(function.id, &function.body.0, function.body.1);
        let mut body = String::new();
        let saved_view = std::mem::replace(
            &mut self.current_returns_view,
            function.returns_view || function.returns_mut_view,
        );
        let saved_returns_async = std::mem::replace(
            &mut self.returns_an_async_closure,
            self.program.async_returning.contains(&function.id),
        );
        let declared_return = self
            .return_type_of(function)
            .map(|type_id| self.concrete(type_id));
        let saved_return_type = std::mem::replace(&mut self.current_return_type, declared_return);
        let saved_origin = self.current_origin.replace(function.name);
        let walked = self.emit_block(&function.body.0, function.body.1, &mut body, 1);
        self.current_origin = saved_origin;
        self.current_return_type = saved_return_type;
        self.returns_an_async_closure = saved_returns_async;
        self.current_returns_view = saved_view;
        walked?;

        let mut out = String::new();
        // F25: whether `main`'s body was opened inside a `main_guard` closure
        // that has to be closed after it.
        let mut closes_a_guard = false;
        if is_main {
            if is_async {
                // `async fun main` — `main` itself cannot be async, so the real
                // body is its own `async fn` and `main` is the one call into the
                // executor. `block_on` drives the loop until both the microtask
                // queue and the deadline list are empty, which is where node
                // exits too.
                // F25: `main_guard` is what makes an uncaught failure answer
                // node's shape — the message alone on stderr, exit code 1 —
                // rather than Rust's panic banner and 101.
                let _ = writeln!(out, "fn main() {{");
                let _ = writeln!(
                    out,
                    "    vilan_rt::main_guard(|| vilan_rt::executor::block_on({ASYNC_MAIN_BODY}()));"
                );
                let _ = writeln!(out, "}}");
                let _ = writeln!(out, "async fn {ASYNC_MAIN_BODY}() {{");
            } else {
                let _ = writeln!(out, "fn main() {{");
                let _ = writeln!(out, "    vilan_rt::main_guard(|| {{");
                closes_a_guard = true;
            }
        } else {
            let _ = writeln!(
                out,
                "{}fn {name}({}) -> {returned} {{",
                if is_async { "async " } else { "" },
                parameters.join(", ")
            );
        }
        out.push_str(&body);
        // F20: node runs its event loop AFTER the module's top level returns, so
        // a synchronous `main` that left a microtask behind — which
        // `std::reactive`'s late-write path does — must still let it run. The
        // async `main` above reaches the same loop through `block_on`, and a
        // program that queued nothing pays one empty loop turn.
        //
        // It runs INSIDE F25's `main_guard`, and that is the point of the
        // order: a panic raised by a microtask this turn is the program
        // failing, and it owes node's exit code and node's stderr like any
        // other.
        if is_main && !is_async {
            let _ = writeln!(out, "    vilan_rt::executor::run_pending();");
        }
        if closes_a_guard {
            let _ = writeln!(out, "    }});");
        }
        let _ = writeln!(out, "}}");
        Ok(out)
    }

    /// What a function hands back — the DECLARED return type where one was
    /// written, else the type of its body's tail, else its first `ret`'s.
    ///
    /// The fallback is not a nicety. `impl Id with Default { fun default() {
    /// Id::new(0) } }` writes no return type because the trait declared
    /// `Self`, and reading only the declaration emitted `-> ()` for a body that
    /// hands back an `Id` — rustc's `expected Id_8605, found ()`. JavaScript
    /// never had to ask, which is why the gap survived S1a.
    fn return_type_of(&self, function: &vilan_core::analyzer::Function<'src>) -> Option<TypeId> {
        if let Some(type_id) = function.return_type_id {
            return Some(type_id);
        }
        let from_tail = self.value_type_of(function.body.1);
        if from_tail.is_some() {
            return from_tail;
        }
        function
            .rets
            .iter()
            .find_map(|(_, value)| value.and_then(|value| self.value_type_of(value)))
    }

    /// The type an expression EVALUATES to, for the return-type inference above
    /// — `type_of`, and, where that is silent about a call, the callee's own
    /// answer.
    ///
    /// The recursion is what `default.vl` needs: the impl's `fun default() {
    /// Id::new(0) }` writes no return type and its tail is a call to
    /// `Id::new`, which writes none either, so one hop of inference answers
    /// `()` for a chain the analyzer had fully typed.
    fn value_type_of(&self, expr_id: Id) -> Option<TypeId> {
        let _guard = vilan_core::util::RecursionGuard::enter()?;
        if let Some(type_id) = self
            .type_of(expr_id)
            .filter(|type_id| !matches!(self.resolve(*type_id), Some(Type::Void) | None))
        {
            return Some(type_id);
        }
        let Some(Expr::Call(call_id)) = self.program.entity_map.get(&expr_id) else {
            return None;
        };
        let subject_id = self.program.function_calls.get(call_id)?.subject_id;
        match self.program.entity_map.get(&subject_id) {
            Some(Expr::Local(target)) => {
                let callee = self.program.functions.get(target)?;
                self.return_type_of(callee)
            }
            // A VALUE call — `(self.fn)()`. The subject's own type is a
            // closure, and a closure type carries its return type: `fun
            // next(self): T { (self.fn)() }` writes no return type (the trait
            // declared one), and without this hop the instance was emitted
            // `-> ()` over a body handing back an `i32`.
            _ => match self.resolve(self.type_of(subject_id)?)? {
                Type::Closure(_, return_type_id, _) => Some(*return_type_id),
                _ => None,
            },
        }
    }

    /// The Rust name for one instance. `{name}_{id}` for the non-generic case
    /// (S1a's spelling, kept so its output does not move) and a per-function
    /// sequence number after it for an instantiation.
    fn mint_function_name(&self, name: &str, id: Id, key: &[String]) -> String {
        if key.is_empty() {
            return format!("{}_{}", sanitize(name), id.0);
        }
        let mut sequence = 0;
        for (existing, _) in self.instances.keys() {
            if *existing == id {
                sequence += 1;
            }
        }
        format!("{}_{}_{}", sanitize(name), id.0, sequence)
    }

    fn parameter_declaration(&mut self, id: Id, span: Span) -> Result<String, Error> {
        if self.program.context_hidden_parameters.contains_key(&id) {
            return self.context_parameter_declaration(id, span);
        }
        let parameter = self
            .program
            .parameters
            .get(&id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved parameter", span))?;
        if parameter.spread {
            return Err(unsupported("a spread parameter", span));
        }
        // F20: a `lazy` parameter carries the memo cell, by value — one handle
        // per call, and a FORWARD passes the same cell on so a chain memoizes
        // once. M81's eager set is the exception: a parameter every call site
        // filled inertly holds the plain value and its reads do not force, so it
        // is an ordinary parameter here too.
        if parameter.lazy && !self.program.lazy_eager_parameters.contains(&id) {
            let rendered = self.rust_type(parameter.type_id, span)?;
            return Ok(format!(
                "{}: vilan_rt::Lazy<{rendered}>",
                self.binding_name(id)
            ));
        }
        // A parameter declared `async |T| U` (J2's `async_values`, which the
        // inference also fills for an unannotated binding that holds one) —
        // or, F22, one THIS INSTANCE adapts: `fun run(f: || i32)` is declared
        // synchronous and its adapted instance takes a future-answering
        // closure, which is the whole of what an adapted instance is.
        self.expects_async =
            self.program.async_values.contains(&id) || self.current_adapted_bits.contains(&id);
        let rendered = self.rust_type(parameter.type_id, span);
        self.expects_async = false;
        let rendered = rendered?;
        let form = self.receiving_form(&parameter);
        let declaration = match form {
            Receiving::ByValue => rendered,
            Receiving::Ref => format!("&{rendered}"),
            Receiving::RefMut => format!("&mut {rendered}"),
        };
        // H9 (`mut-parameters.md`): `mut x` is BINDER mutability of the
        // callee's by-value copy, which is exactly Rust's `mut x: T`. Written
        // on every by-value parameter rather than only the `mut` ones, because
        // a by-value parameter IS a local copy and a `mut` binder on one
        // changes nothing a caller can observe — while getting the set wrong
        // is a rustc refusal (`cannot assign to immutable argument`). A
        // reference parameter keeps no `mut`: there the binder's mutability
        // would be the POINTER's, which is a different claim.
        let binder = match form {
            Receiving::ByValue => "mut ",
            _ => "",
        };
        Ok(format!("{binder}{}: {declaration}", self.binding_name(id)))
    }

    /// A HIDDEN CONTEXT parameter's declaration (J6).
    ///
    /// `context::thread_contexts` rewrites every ambient read into a parameter
    /// and every call into one that passes the value — so by the time a program
    /// reaches a backend, a `context` clause is ordinary plumbing. What it is
    /// NOT is an ordinary `parameters` record: the pass keeps those out of the
    /// map deliberately (editing-dx.md §19.3 — a fabricated entry would dress
    /// compiler plumbing as source in completion and every other consumer), and
    /// leaves a marker naming the context binding the parameter threads.
    ///
    /// So the type comes from that binding: it is declared `Context<T>`, and
    /// the value threaded through is its `T` — under an `Option` for the SAFE
    /// flavour (`ambient-owner.md` §2.1), which is what
    /// [`Emitter::context_parameter_type`] decides. The binder is `mut` because
    /// nothing in the IR says whether the plumbing writes it, and an unused
    /// `mut` is in `PRELUDE`'s allow list.
    fn context_parameter_declaration(&mut self, id: Id, span: Span) -> Result<String, Error> {
        let rendered = self.context_parameter_type(id, span)?;
        Ok(format!("mut {}: {rendered}", self.binding_name(id)))
    }

    /// The native type a context-threaded hidden parameter carries — the
    /// context's value type, under an `Option` for the safe flavour.
    fn context_parameter_type(&mut self, id: Id, span: Span) -> Result<String, Error> {
        let Some(context) = self.program.context_hidden_parameters.get(&id).copied() else {
            return Err(unsupported(
                "a parameter that is not context-threaded",
                span,
            ));
        };
        // F23: a LOOKUP, not an inference. `context.rs` knows the flavour where
        // it mints the parameter (the node's provider settles it) and records
        // it; the emitter used to re-derive it from the arguments the call
        // sites pass, which needed a worklist, could not see through a
        // clause-typed parameter without first connecting it to every closure
        // literal that can land there, and defaulted to the strict reading when
        // nothing settled — a guess that showed up as a rustc type error.
        let optional = self
            .program
            .context_optional_hidden_parameters
            .contains(&id);
        let value = self.context_value_type(context, span)?;
        Ok(if optional {
            format!("Option<{value}>")
        } else {
            value
        })
    }

    /// How a parameter is RECEIVED natively.
    ///
    /// The three conventions map straight across, with one synthesis: a BARE
    /// `self` is a loan (spec §6.8 R3 — the receiver is not consumed), so it
    /// arrives as `&T` even though nothing in the source wrote an `&`. Every
    /// other bare parameter is a by-value copy, which rule 1 already paid for
    /// at the call site.
    ///
    /// `mut self` is NOT that loan: H9 makes it the callee's own by-value copy,
    /// and `mut-parameters.vl` is the pin — `original.with_x(9)` leaves
    /// `original.x` at 0, so the receiver was copied. Reading it as a loan
    /// emitted `&Point` where the body assigns a field, which rustc refuses.
    fn receiving_form(&self, parameter: &vilan_core::analyzer::Parameter<'_>) -> Receiving {
        match parameter.convention {
            Convention::Ref => Receiving::Ref,
            Convention::RefMut => Receiving::RefMut,
            Convention::Bare if parameter.name == "self" && !parameter.mutable => Receiving::Ref,
            Convention::Bare | Convention::Own => Receiving::ByValue,
        }
    }

    // ---------------------------------------------------------- the walk ---

    fn indent(depth: usize) -> String {
        "    ".repeat(depth)
    }

    fn emit_block(
        &mut self,
        statements: &[Id],
        tail: Id,
        out: &mut String,
        depth: usize,
    ) -> Result<(), Error> {
        let pad = Self::indent(depth);
        for statement in statements {
            let rendered = self.statement(*statement, depth)?;
            if !rendered.is_empty() {
                let _ = writeln!(out, "{pad}{rendered}");
            }
        }
        if !matches!(self.program.entity_map.get(&tail), Some(Expr::Void) | None) {
            // A block's trailing expression is a VALUE position — it is what the
            // block evaluates to — so rule 1's copy is owed here exactly as it
            // is at an argument. `fun to_string(self): str { self }` is the
            // shape: the tail reads a loan and the signature hands back a value.
            //
            // UNLESS this is the body tail of a `borrows` function, whose
            // return type IS a reference: `fun same(x: &mut i32): &mut i32 { x }`
            // passes the loan on, and copying out of it answers `i32` where the
            // signature promised `&mut i32`.
            let returns_the_loan = depth == 1 && self.current_returns_view;
            // The body TAIL of a function declared `-> async |T| U` is a
            // return position, so a closure literal landing there answers a
            // future. See [`Emitter::returns_an_async_closure`].
            self.expects_async_value = depth == 1 && self.returns_an_async_closure;
            let saved_expected = if depth == 1 {
                std::mem::replace(&mut self.expected_type, self.current_return_type)
            } else {
                self.expected_type
            };
            let rendered = if returns_the_loan {
                self.expression(tail, depth)
            } else {
                self.value_of(tail, depth)
            };
            self.expected_type = saved_expected;
            self.expects_async_value = false;
            let rendered = rendered?;
            let _ = writeln!(out, "{pad}{rendered}");
        }
        Ok(())
    }

    /// One statement, which is an expression plus a `;` for every form that
    /// needs one. `if`, `match` and the loops are statements in Rust already.
    fn statement(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        match self.program.entity_map.get(&id) {
            Some(Expr::Void) | None => Ok(String::new()),
            Some(Expr::If(_)) | Some(Expr::For(_, _)) | Some(Expr::ForEach(_, _, _)) => {
                self.expression(id, depth)
            }
            _ => Ok(format!("{};", self.expression(id, depth)?)),
        }
    }

    fn expression(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        let rendered = self.expression_inner(id, depth);
        // The census (see [`Emitted::host_gaps`]) records a refusal and carries
        // on, so ONE emit answers "what does this program still need" instead
        // of answering with its first sentence. Only here: an expression
        // position accepts `unimplemented!()`, which types as anything.
        match rendered {
            Err(error) if self.census => {
                self.host_gaps.insert(census_entry(&error));
                Ok("unimplemented!()".to_string())
            }
            other => other,
        }
    }

    fn expression_inner(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        // B105's hoist: this expression was evaluated once into a temp ahead of
        // the write, and both walks of the place name that temp.
        if let Some(name) = self.hoisted.get(&id) {
            return Ok(name.clone());
        }
        let span = self.span_of(id);
        let Some(expr) = self.program.entity_map.get(&id).cloned() else {
            return Ok("()".to_string());
        };
        let rendered = match expr {
            Expr::Bool(value) => value.to_string(),
            Expr::Void | Expr::Null => "()".to_string(),
            Expr::Number(whole, fraction, suffix) => {
                self.number_literal(id, whole, fraction, suffix, span)?
            }
            Expr::String(text) => format!("vilan_rt::str_new({})", rust_string(text)),
            Expr::MultilineString(_) => {
                return Err(unsupported("a triple-quoted string", span));
            }
            // J6: a `None` or a variant the context pass synthesized as an
            // argument names the VARIANT, not a place — `Expr::Local` of the
            // variant's own declaration id.
            Expr::Local(binding)
                if matches!(
                    self.program.entity_map.get(&binding),
                    Some(Expr::EnumVariant(_, _))
                ) =>
            {
                let Some(Expr::EnumVariant(enum_id, index)) =
                    self.program.entity_map.get(&binding).cloned()
                else {
                    unreachable!("the guard just matched an enum variant");
                };
                self.variant_path(enum_id, index, &[], span)?
            }
            Expr::Local(binding) => self.read_module_binding_or_local(binding, span)?,
            Expr::Parameter(binding) => self.binding_name(binding),
            Expr::Variable(binding) => self.declaration(binding, depth)?,
            Expr::Block((statements, tail)) => {
                let mut body = String::new();
                self.emit_block(&statements, tail, &mut body, depth + 1)?;
                format!("{{\n{body}{}}}", Self::indent(depth))
            }
            Expr::Binary(op, left, right) => self.binary(id, op, left, right, depth, span)?,
            Expr::Unary(op, operand) => match op {
                '!' => format!("!({})", self.expression(operand, depth)?),
                '-' => format!("-({})", self.expression(operand, depth)?),
                other => {
                    return Err(unsupported(&format!("the unary operator `{other}`"), span));
                }
            },
            Expr::If(branch) => self.if_branch(&branch, depth)?,
            Expr::Match(subject, legs) => self.match_expr(subject, &legs, depth, span)?,
            Expr::For(condition, (statements, tail)) => {
                let mut body = String::new();
                self.emit_block(&statements, tail, &mut body, depth + 1)?;
                let pad = Self::indent(depth);
                match condition {
                    Some(condition) => {
                        let condition = self.expression(condition, depth)?;
                        format!("while {condition} {{\n{body}{pad}}}")
                    }
                    None => format!("loop {{\n{body}{pad}}}"),
                }
            }
            Expr::ForEach(iterable, item, (statements, tail)) => {
                self.for_each(id, iterable, item, &statements, tail, depth, span)?
            }
            Expr::Jump(keyword) => match keyword {
                "break" => "break".to_string(),
                "continue" => "continue".to_string(),
                other => return Err(unsupported(&format!("`jump {other}`"), span)),
            },
            Expr::FunctionReturn(value) => match value {
                Some(value) if self.current_returns_view => {
                    format!("return {}", self.expression(value, depth)?)
                }
                Some(value) => {
                    self.expects_async_value = self.returns_an_async_closure;
                    let saved_expected =
                        std::mem::replace(&mut self.expected_type, self.current_return_type);
                    let rendered = self.value_of(value, depth);
                    self.expected_type = saved_expected;
                    self.expects_async_value = false;
                    format!("return {}", rendered?)
                }
                None => "return".to_string(),
            },
            Expr::Assignment(target, value) => self.assignment(target, value, depth, span)?,
            Expr::Field(subject, _, index) => {
                let subject_text = self.expression(subject, depth)?;
                let field = self.field_name(subject, index, span)?;
                format!("{subject_text}.{field}")
            }
            Expr::Index(subject, index) => {
                let subject_text = self.expression(subject, depth)?;
                // An index is an index, whatever the surrounding position
                // expects: `xs[1] = xs[1] + 2` on a `List<u53>` expects `u53`
                // of the VALUE, and a literal subscript that inherited it came
                // out `1u64`.
                let index_text =
                    self.expecting_nothing(|emitter| emitter.expression(index, depth))?;
                format!("{subject_text}[({index_text}) as usize]")
            }
            Expr::List(elements) => {
                // An element's position declares the list's ELEMENT type, not
                // the list's — `mut xs: List<u32> = [0]` needs `0u32`.
                let element_type = self
                    .type_of(id)
                    .or(self.expected_type)
                    .and_then(|type_id| self.resolve(type_id))
                    .and_then(|resolved| match resolved {
                        Type::Struct(_, arguments) => arguments.first().copied(),
                        Type::Array(element, _) => Some(*element),
                        _ => None,
                    });
                let mut parts = Vec::new();
                for element in &elements {
                    parts.push(self.value_of_expecting(*element, element_type, depth)?);
                }
                format!("vec![{}]", parts.join(", "))
            }
            Expr::Tuple(elements) => {
                let mut parts = Vec::new();
                for element in &elements {
                    parts.push(self.value_of(*element, depth)?);
                }
                format!("({},)", parts.join(", "))
            }
            Expr::TupleIndex(subject, offset, width) => {
                if width != 1 {
                    return Err(unsupported("a multi-slot tuple element", span));
                }
                format!("{}.{offset}", self.expression(subject, depth)?)
            }
            Expr::StructInitializer(named, fields) => {
                let pairs: Vec<(usize, Id)> = fields
                    .iter()
                    .map(|(index, value)| (*index, *value))
                    .collect();
                // The initializer's own id carries the STRUCT it builds AND the
                // arguments it builds it at; the id in the node is the name it
                // was written under, which the JS emitter ignores because an
                // array needs no declaration. Here it is the declaration or
                // nothing, so the type is the answer and the written name is the
                // fallback.
                let struct_id = self
                    .type_of(id)
                    .or(self.expected_type)
                    .and_then(|type_id| self.resolve(type_id))
                    .and_then(|resolved| match resolved {
                        Type::Struct(struct_id, _) => Some(*struct_id),
                        _ => None,
                    })
                    .filter(|struct_id| self.program.structs.contains_key(struct_id))
                    .unwrap_or(named);
                let arguments = self.struct_arguments_at(id, struct_id);
                self.struct_literal(struct_id, &arguments, &pairs, depth, span)?
            }
            Expr::Reference(operand, mutable) => {
                if mutable {
                    // A `&mut` the source wrote names a place the holder will
                    // WRITE, so a cell-resident binding reaches its cell — see
                    // [`Emitter::mutable_place`].
                    format!("&mut {}", self.mutable_place(operand, depth)?)
                } else {
                    format!("&{}", self.expression(operand, depth)?)
                }
            }
            Expr::Dereference(operand) => format!("(*{})", self.expression(operand, depth)?),
            Expr::Call(call_id) => self.call(id, call_id, depth, span)?,
            Expr::Async(spawned) => self.async_spawn(id, spawned, depth, span)?,
            Expr::Await(awaited) => self.await_of(awaited, depth)?,
            Expr::Closure(closure_id) => self.closure(closure_id, depth, span)?,
            Expr::Is(subject, pattern) => self.is_test(subject, &pattern, depth, span)?,
            // A destructuring `let` — `let (name, value) = pair;`. The pattern
            // renderer is `match`'s: a destructure's pattern is IRREFUTABLE by
            // construction (spec §3.10 — a refutable one is a `match` or an
            // `is` test), so there is nothing to test and nothing to fall
            // through to, which is exactly what makes a `let` pattern legal
            // Rust too. `std::http`'s response loop is the customer:
            // `for header in response.headers { let (name, value) = header; .. }`.
            Expr::Destructure(subject, pattern) => {
                let subject_type = self.type_of(subject);
                let bound = self.pattern(&pattern, subject_type, span)?;
                // A destructure CONSUMES what it binds, so a destructure of a
                // PLACE is a copy by rule 1 and has to be written as one:
                // `clone_sites` does not mark the read (on the JS backend the
                // pattern reads the elements out of the array and moves
                // nothing), and two destructures of the same binding were a
                // use-after-move. `copy_a_consumed_place_read` is the rule a
                // by-value ARGUMENT takes, so there is one of it — and it knows
                // not to copy a binding that holds a view.
                let value = self.expression(subject, depth)?;
                let value = self.copy_a_consumed_place_read(subject, value);
                format!("let {bound} = {value}")
            }
            Expr::EnumVariant(enum_id, index) => {
                let arguments = self.enum_arguments_at(id, enum_id);
                self.variant_path(enum_id, index, &arguments, span)?
            }
            Expr::TryAssert(receiver) => self.try_assert(id, receiver, depth, span)?,
            Expr::Function(_)
            | Expr::Struct(_)
            | Expr::Enum(_)
            | Expr::Trait(_)
            | Expr::Impl(_)
            | Expr::Module(_)
            | Expr::ExternalFunction(_) => String::new(),
            Expr::Error => return Err(unsupported("an expression that did not analyze", span)),
            other => {
                return Err(unsupported(
                    &format!("the expression form `{}`", form_name(&other)),
                    span,
                ));
            }
        };
        Ok(rendered)
    }

    /// [`Self::value_of`] under a declared expectation — the type the position
    /// being filled asks for, which is how a generic constructor whose own site
    /// records an open type finds its arguments.
    fn value_of_expecting(
        &mut self,
        id: Id,
        expecting: Option<TypeId>,
        depth: usize,
    ) -> Result<String, Error> {
        let saved = std::mem::replace(&mut self.expected_type, expecting);
        let rendered = self.value_of(id, depth);
        self.expected_type = saved;
        let rendered = rendered?;
        // F18 slice 2: a value landing in an `any` position is WRAPPED. On the
        // JS backend there is nothing to do — every value is already a JS
        // value — so the conversion has no counterpart there and lives at the
        // one seam that knows both the position's type and the value's.
        // Already-`any` values pass through, which is what makes a bind list
        // read out of another one idempotent.
        if matches!(
            expecting.and_then(|type_id| self.resolve(type_id)),
            Some(Type::Any)
        ) && !matches!(
            self.type_of(id).and_then(|type_id| self.resolve(type_id)),
            Some(Type::Any)
        ) {
            // The wrap CONSUMES what it is given (`Any::from` takes the value),
            // and `clone_sites` marks nothing here — on the JS backend there is
            // no conversion at all — so rule 1's copy is owed at this read the
            // way it is owed at any other consuming position. Without it,
            // `run([username, hashed])` moved `username` out of a binding the
            // next line still reads.
            let rendered = self.copy_a_consumed_place_read(id, rendered);
            return Ok(format!("vilan_rt::Any::from({rendered})"));
        }
        Ok(rendered)
    }

    /// An expression in a VALUE position — rule 1's copy applied where the
    /// analyzer already decided one is owed. `clone_sites` is the JS emitter's
    /// own `__clone` decision, read here so the two backends copy in exactly
    /// the same places rather than in two opinions of the same places.
    fn value_of(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        let text = self.value_of_unerased(id, depth)?;
        self.erase_into_object(id, text)
    }

    /// A124 R3: a concrete value landing in a `dyn`-typed position becomes the
    /// object here — AFTER every copy the position owes has been applied to
    /// the value, so what the pointer takes is the copy rather than a moved
    /// original (the JS emitter builds its pair at the same point for the same
    /// reason). The `Rc<Concrete>` coerces to `Rc<dyn ObjectX>` at
    /// `Dyn::new`'s argument, which is the whole erasure.
    fn erase_into_object(&mut self, id: Id, text: String) -> Result<String, Error> {
        let Some((subject, trait_id, arguments)) = self.program.dyn_coercions.get(&id).cloned()
        else {
            return Ok(text);
        };
        let span = self.span_of(id);
        let object = self.ensure_object_impl(subject, trait_id, &arguments, span)?;
        Ok(format!(
            "vilan_rt::Dyn::<dyn {object}>::new(std::rc::Rc::new({text}))"
        ))
    }

    fn value_of_unerased(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        // B109: a `&place` and a `borrows` CALL are leaves that name storage
        // without being places, and in a VALUE position both are read THROUGH —
        // which is rule 1's copy. `element-clones.vl` states the claim in its
        // own comment ("read through the view and the three spellings are
        // indistinguishable") and is the pin: `fun reference_of(holder:
        // &Holder): List<i32> { &holder.items }` emitted `&holder.items` against
        // a signature promising a value.
        if !self.current_returns_view && !self.declaring_a_view && self.reads_through_a_view(id) {
            if let Some(&Expr::Reference(operand, _)) = self.program.entity_map.get(&id) {
                let operand_text = self.expression(operand, depth)?;
                return Ok(format!("({operand_text}).clone()"));
            }
            let text = self.expression(id, depth)?;
            return Ok(format!("({text}).clone()"));
        }
        let text = self.expression(id, depth)?;
        if self.program.clone_sites.contains_key(&id) {
            return Ok(format!("({text}).clone()"));
        }
        // F16 / the probe's R-1, from the other side: a closure value is a
        // COUNTED handle, so handing one on is a retain. JavaScript hides this
        // because a function value is a reference and using it twice costs
        // nothing; natively the second use is a use-after-move unless the
        // handle is bumped. The analyzer's `clone_sites` does not cover it —
        // rule 1 is about aggregates, and a closure is not one.
        if self.reads_a_closure_binding(id) {
            return Ok(format!("({text}).clone()"));
        }
        // J6, the same rule for the executor's HANDLES. A `Task`, a `Nursery`,
        // a `CancelSignal` and a `TimerHandle` are counted handles, and a copy
        // of one is the same task/nursery/timer — which is why `clone_sites`
        // never marks a read of one: on the JS backend a handle is a class
        // instance and `__clone` passes it through untouched, so no copy is
        // owed there. Natively the second read is a use-after-move unless the
        // count is bumped.
        if self.reads_a_handle_binding(id) {
            return Ok(format!("({text}).clone()"));
        }
        // A LOANED parameter (`&T` / `&mut T` natively) read where a value is
        // wanted is rule 1's copy: `fun to_string(self): str { self }` hands
        // back a `str`, and `self` is a loan of one.
        if self.reads_a_loaned_parameter(id) {
            return Ok(format!("({text}).clone()"));
        }
        // F20: a read of an enclosing closure's CAPTURE, handed on by value.
        // See [`Emitter::closure_captures`] for why the copy is owed here and
        // not on the JS side.
        if self.reads_a_captured_binding(id) {
            return Ok(format!("({text}).clone()"));
        }
        Ok(text)
    }

    /// Whether `id` is a leaf that names STORAGE without being a place — a
    /// written `&place`, or a call to a `borrows` function. Both answer a
    /// reference natively and a value is what the position wants.
    fn reads_through_a_view(&self, id: Id) -> bool {
        match self.program.entity_map.get(&id) {
            Some(Expr::Reference(_, _)) => true,
            Some(&Expr::Call(call_id)) => self
                .program
                .function_calls
                .get(&call_id)
                .and_then(|call| match self.program.entity_map.get(&call.subject_id) {
                    Some(Expr::Local(target)) => self.program.functions.get(target),
                    _ => None,
                })
                .is_some_and(|function| function.returns_view || function.returns_mut_view),
            _ => false,
        }
    }

    /// Whether `id` reads a binding some enclosing closure captures.
    ///
    /// Every frame is consulted, not only the innermost: a closure nested two
    /// deep reads the OUTER one's capture through the inner one's, and both
    /// moves are the same move.
    fn reads_a_captured_binding(&self, id: Id) -> bool {
        if self.closure_captures.is_empty() {
            return false;
        }
        let binding = match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) | Some(Expr::Parameter(binding)) => *binding,
            _ => return false,
        };
        self.closure_captures
            .iter()
            .any(|frame| frame.contains(&binding))
    }

    /// Whether `id` reads a parameter this emitter receives by reference —
    /// THROUGH a place spine, because a field of a loan is behind the same
    /// reference the loan is.
    ///
    /// `fun build(self): Response { Response { body: self.body, .. } }` is the
    /// shape (`std::http`'s builder): `self` arrives as `&Request` and
    /// `self.body` is a `str`, so reading it into the literal is a move out of a
    /// shared reference. `clone_sites` marks the copy where rule 1 needs one and
    /// ELIDES it at a last use, which is sound on a backend where nothing moves;
    /// natively the elision is the defect. The copy is rule 1's own answer for a
    /// value read, so taking it is the conservative direction.
    fn reads_a_loaned_parameter(&self, id: Id) -> bool {
        let Some(_guard) = vilan_core::util::RecursionGuard::enter() else {
            return false;
        };
        match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) | Some(Expr::Parameter(binding)) => self
                .program
                .parameters
                .get(binding)
                .is_some_and(|parameter| self.receiving_form(parameter) != Receiving::ByValue),
            // The place spine: a field, a tuple slot or a subscript of a loan is
            // itself behind the loan's reference.
            Some(&Expr::Field(subject, _, _))
            | Some(&Expr::TupleIndex(subject, _, _))
            | Some(&Expr::Index(subject, _)) => self.reads_a_loaned_parameter(subject),
            _ => false,
        }
    }

    /// Whether `id` READS a binding whose type is a closure — the shape that
    /// owes a refcount bump. A closure LITERAL is a fresh value and owes
    /// nothing.
    fn reads_a_closure_binding(&self, id: Id) -> bool {
        if !matches!(
            self.program.entity_map.get(&id),
            Some(Expr::Local(_)) | Some(Expr::Parameter(_))
        ) {
            return false;
        }
        self.type_of(id)
            .and_then(|type_id| self.resolve(type_id))
            .is_some_and(|resolved| matches!(resolved, Type::Closure(_, _, _)))
    }

    /// Whether `id` READS a binding holding one of the executor's host handles
    /// (J6) — the shape that owes a refcount bump, exactly as a closure
    /// binding does.
    fn reads_a_handle_binding(&self, id: Id) -> bool {
        let binding = match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) | Some(Expr::Parameter(binding)) => *binding,
            _ => return false,
        };
        // A context-threaded hidden parameter carries no recorded type at all,
        // so the type test below cannot see it — and what it carries IS a
        // handle wherever the executor is involved (`ambient_nursery`). It is
        // retained unconditionally: a context value is `Clone` by
        // construction, and the alternative is a use-after-move on the second
        // read of a parameter the pass appended.
        if self
            .program
            .context_hidden_parameters
            .contains_key(&binding)
        {
            return true;
        }
        self.type_of(id)
            .and_then(|type_id| self.resolve(type_id))
            .and_then(|resolved| match resolved {
                Type::Struct(struct_id, _) => self.program.structs.get(struct_id),
                _ => None,
            })
            .is_some_and(|declaration| {
                declaration.external
                    && matches!(
                        declaration.name,
                        // The counted CELL is a handle in exactly this sense
                        // (F20): `Shared` is shared, not copied, so the JS
                        // backend's `__clone` passes one through untouched and
                        // `clone_sites` marks no copy — and natively the second
                        // read of a `Shared` binding is a use-after-move.
                        // `std::reactive`'s `sub` puts one `Shared<bool>` into a
                        // `Subscriber` and the same one into the `Subscription`.
                        "Shared" | "Weak" | "Task" | "Nursery" | "CancelSignal" | "TimerHandle"
                    )
            })
    }

    /// An assignment, which has FOUR shapes natively where JS has one.
    ///
    /// A plain place assigns. A mutably-captured binding is a counted cell and
    /// assigns through it (R3). A module-level binding is a `thread_local!` and
    /// assigns through its `RefCell`. And a binding that HOLDS A VIEW assigns
    /// through the view — `let cell = &mut x; cell = 5;` writes `x`, which is
    /// `*cell = 5` and not `cell = 5` (the latter reseats the reference, which
    /// rustc refuses outright for a non-`mut` binder and would silently do the
    /// wrong thing for a `mut` one).
    fn assignment(
        &mut self,
        target: Id,
        value: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        if let Some(hoisted) = self.hoist_compound_target(target, value, depth)? {
            return Ok(hoisted);
        }
        // The TARGET's type is the value position's expectation (B370's law on
        // the assignment path): a numeric literal takes its width from the
        // position it lands in, and an assignment is a position. Without it
        // `mut i: u53 = 5; i -= 1;` emitted `i - (1i32)` against a `u64`, which
        // rustc refused — a `let` was right only because `declaration` was
        // already passing the binding's type down.
        let expecting = self.type_of(target);
        // A write THROUGH a counted cell — `*self.value = v`, which the
        // analyzer lowers to a deref of the cell's read intrinsic. The place
        // is not a Rust place at all (`Shared` hands out a value, not a
        // reference), so the write is the cell's own `set`; rendering the
        // target and assigning to it produced `(*(cell).set(())) = v`, which
        // rustc refused — the intrinsic's second argument had nowhere to come
        // from because the VALUE is the assignment's.
        if let Some(receiver) = self.cell_write_receiver(target) {
            // The cell's ELEMENT type is the position's expectation: the target
            // is a deref of the cell's own read intrinsic and carries no type of
            // its own, so `cell.write() = cell.read() + 1` on a `Shared<u53>`
            // wrote `(1i32)` against a `u64`.
            let expecting = expecting.or_else(|| self.cell_element_type(receiver));
            let receiver_text = self.expression(receiver, depth)?;
            let value_text = self.value_of_expecting(value, expecting, depth)?;
            return Ok(format!("({receiver_text}).set({value_text})"));
        }
        let named = match self.program.entity_map.get(&target) {
            Some(Expr::Local(binding)) => Some(*binding),
            _ => None,
        };
        if let Some(binding) = named {
            if self.boxed.contains(&binding) {
                let value_text = self.value_of_expecting(value, expecting, depth)?;
                return Ok(format!("{}.set({value_text})", self.binding_name(binding)));
            }
            if self.module_bindings.contains(&binding) {
                let cell = self.ensure_module_binding(binding, span)?;
                let value_text = self.value_of_expecting(value, expecting, depth)?;
                return Ok(format!("{cell}.with(|cell| cell.set({value_text}))"));
            }
            if self.binding_holds_a_view(binding) {
                let value_text = self.value_of_expecting(value, expecting, depth)?;
                return Ok(format!("*{} = {value_text}", self.binding_name(binding)));
            }
        }
        let target_text = self.mutable_place(target, depth)?;
        let value_text = self.value_of_expecting(value, expecting, depth)?;
        Ok(format!("{target_text} = {value_text}"))
    }

    /// Renders with NO expected type — the positions a surrounding
    /// expectation must not reach (a subscript, whose type is an index's and
    /// not the indexed value's).
    fn expecting_nothing<T>(
        &mut self,
        render: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let saved = self.expected_type.take();
        let rendered = render(self);
        self.expected_type = saved;
        rendered
    }

    /// The element type of a counted cell a write goes through — the single
    /// argument of the `Shared` the receiver names.
    fn cell_element_type(&self, receiver: Id) -> Option<TypeId> {
        match self.resolve(self.type_of(receiver)?)? {
            Type::Struct(_, arguments) => arguments.first().copied(),
            _ => None,
        }
    }

    /// The CELL an assignment writes through, when its target is a deref of a
    /// `Shared`'s own read — the shape `*cell = value` lowers to. The receiver
    /// is the intrinsic call's first argument, which is the cell itself.
    fn cell_write_receiver(&self, target: Id) -> Option<Id> {
        let Some(Expr::Dereference(inner)) = self.program.entity_map.get(&target) else {
            return None;
        };
        let Some(Expr::Call(call_id)) = self.program.entity_map.get(inner) else {
            return None;
        };
        let call = self.program.function_calls.get(call_id)?;
        let Some(Expr::Local(subject)) = self.program.entity_map.get(&call.subject_id) else {
            return None;
        };
        matches!(
            self.program.intrinsics.get(subject),
            Some(Intrinsic::SharedValue | Intrinsic::SharedWrite)
        )
        .then(|| call.argument_ids.first().copied())
        .flatten()
    }

    /// The compound-assignment subscript hoist (B105), natively.
    ///
    /// `x[i] op= v` desugars to `x[i] = x[i] op v`, and the two `x[i]`s are two
    /// independent walks of the same source place — so an effectful subscript
    /// ran TWICE. `compound-index.vl` is the pin and it caught this backend:
    /// `ys[bump()] += 1` incremented the counter twice, printing `2` where the
    /// JS backend printed `1`.
    ///
    /// Each subscript is evaluated ONCE into a `let` ahead of the write, and
    /// both spines name it. The compound-ness comes from the analyzer's own
    /// record ([`Program::compound_rereads`]) rather than from the shape:
    /// `ys[f()] = ys[g()] + 1` looks identical and has two genuinely different
    /// subscripts. Unlike the JS emitter this hoists a PURE subscript too —
    /// there is no golden to churn here, and "evaluate the place once" is the
    /// rule with no exception to keep track of.
    fn hoist_compound_target(
        &mut self,
        target: Id,
        value: Id,
        depth: usize,
    ) -> Result<Option<String>, Error> {
        let Some(&Expr::Binary(_, left, _)) = self.program.entity_map.get(&value) else {
            return Ok(None);
        };
        // A VIEW target wraps both halves in a synthetic `Dereference`, under
        // which the analyzer's mark sits.
        let reread = match self.program.entity_map.get(&left) {
            Some(&Expr::Dereference(operand)) => operand,
            _ => left,
        };
        if !self.program.compound_rereads.contains(&reread) {
            return Ok(None);
        }
        let mut prelude = String::new();
        let saved = std::mem::take(&mut self.hoisted);
        let paired = self.pair_places(target, reread, &mut prelude, depth);
        let expecting = self.type_of(target);
        let rendered = paired.and_then(|_| {
            let target_text = self.mutable_place(target, depth)?;
            let value_text = self.value_of_expecting(value, expecting, depth)?;
            Ok((target_text, value_text))
        });
        self.hoisted = saved;
        let (target_text, value_text) = rendered?;
        if prelude.is_empty() {
            return Ok(None);
        }
        Ok(Some(format!(
            "{{ {prelude}{target_text} = {value_text}; }}"
        )))
    }

    /// The two place spines in lockstep — they are the same source place walked
    /// twice, so they match node for node, and pairing them is what lets the
    /// re-read name the write's temp. Descends to the ROOT first, so the temps
    /// land in source order (`grid[f()][g()]` evaluates `f()` before `g()`).
    fn pair_places(
        &mut self,
        target: Id,
        reread: Id,
        prelude: &mut String,
        depth: usize,
    ) -> Result<(), Error> {
        match (
            self.program.entity_map.get(&target).cloned(),
            self.program.entity_map.get(&reread).cloned(),
        ) {
            (
                Some(Expr::Index(target_subject, target_index)),
                Some(Expr::Index(reread_subject, reread_index)),
            ) => {
                self.pair_places(target_subject, reread_subject, prelude, depth)?;
                let name = format!("__hoist{}", self.hoisted.len());
                let rendered = self.expression(target_index, depth)?;
                let _ = write!(prelude, "let {name} = {rendered}; ");
                self.hoisted.insert(target_index, name.clone());
                self.hoisted.insert(reread_index, name);
                Ok(())
            }
            (Some(Expr::Field(target_subject, _, _)), Some(Expr::Field(reread_subject, _, _)))
            | (Some(Expr::Dereference(target_subject)), Some(Expr::Dereference(reread_subject)))
            | (
                Some(Expr::TupleIndex(target_subject, _, _)),
                Some(Expr::TupleIndex(reread_subject, _, _)),
            ) => self.pair_places(target_subject, reread_subject, prelude, depth),
            _ => Ok(()),
        }
    }

    /// Whether a local binding holds a VIEW rather than a value — its
    /// initializer is a `&`/`&mut`, or a call to a `borrows` function.
    ///
    /// The type system has no reference form (a view's `type_id` is its
    /// pointee's), so this is read off the initializer, which is where the
    /// `&` was written.
    fn binding_holds_a_view(&self, binding: Id) -> bool {
        let Some(initial) = self
            .program
            .variables
            .get(&binding)
            .and_then(|variable| variable.initial)
        else {
            return false;
        };
        self.reads_through_a_view(initial)
    }

    fn read_binding(&self, binding: Id) -> String {
        let name = self.binding_name(binding);
        // lazy.md §1: the binding holds a memo cell, so a READ of it is a force
        // — the whole of "the parameter reads as a plain `T`, fully
        // transparent". Only a PARAMETER's cell is forced here: a `lazy` module
        // binding is a `thread_local!`, which is already initialized on first
        // access, and the two positions that must not force (a forward into
        // another lazy position, the cell's own declaration) never reach this.
        if self.program.parameters.contains_key(&binding)
            && self.program.lazy_cells.contains(&binding)
            && !self.program.lazy_eager_parameters.contains(&binding)
        {
            return format!("vilan_rt::force(&{name})");
        }
        if self.boxed.contains(&binding) {
            return format!("{name}.get()");
        }
        name
    }

    /// What a call site emits for an argument standing in a `lazy` parameter
    /// (F20; lazy.md §1). `None` for every other argument, which is every
    /// argument in a program that writes no `lazy`.
    ///
    /// The analyzer decided which of the two shapes this is, and the decision is
    /// read rather than re-derived: a FORWARD is a bare reference to a binding
    /// that already holds a cell, so the cell travels as-is — one memo however
    /// deep the chain — and everything else is a THUNK, whose expression is
    /// walked INSIDE the closure, because walking it into the enclosing block
    /// would evaluate at the call site the very thing the feature defers.
    fn lazy_argument(&mut self, argument_id: Id, depth: usize) -> Option<Result<String, Error>> {
        if self.program.lazy_argument_forwards.contains(&argument_id) {
            let Some(&Expr::Local(binding)) = self.program.entity_map.get(&argument_id) else {
                return None;
            };
            // The cell is counted, so forwarding is a handle bump and the
            // forwarding frame keeps its own.
            return Some(Ok(format!("({}).clone()", self.binding_name(binding))));
        }
        let name = (*self.program.lazy_argument_thunks.get(&argument_id)?).to_string();
        Some(self.lazy_thunk(argument_id, &name, depth))
    }

    fn lazy_thunk(&mut self, argument_id: Id, name: &str, depth: usize) -> Result<String, Error> {
        // The thunk is `move`, so it takes every binding it mentions and the
        // enclosing frame goes on reading the same ones — the capture handling a
        // closure literal takes, for the same reason.
        let mut declared_inside = HashSet::new();
        let mut referenced = HashSet::new();
        let mut visited = HashSet::new();
        self.scan_closure(
            argument_id,
            &mut declared_inside,
            &mut referenced,
            &mut visited,
        );
        let captured: HashSet<Id> = referenced
            .iter()
            .filter(|binding| !declared_inside.contains(binding))
            .copied()
            .collect();
        let prelude = self.async_capture_prelude(argument_id);
        self.closure_captures.push(captured);
        let value = self.value_of(argument_id, depth);
        self.closure_captures.pop();
        let value = value?;
        Ok(format!(
            "{{ {prelude}vilan_rt::Lazy::new({}, move || {{ {value} }}) }}",
            rust_string(name)
        ))
    }

    /// A read of a binding that MIGHT be a module-level one.
    ///
    /// A module-level `let` is a program-lifetime value with a constructor, and
    /// natively that is a `thread_local!` — the single-threaded executor J6
    /// designs means a thread local is exactly as visible as a JS module
    /// binding, and unlike a `static` it can run arbitrary vilan code to
    /// initialize itself. Lowered at the READ rather than declared up front:
    /// std declares several (`PI`, the reactive turn's registers) and a program
    /// reaching none of them must carry none of them.
    ///
    /// The read answers a VALUE (`cell.borrow().clone()`) — a counted one
    /// (`str`, a closure, a `Shared`) copies as a refcount bump, so nothing
    /// observes the difference between the cell and a copy of it. The cell is a
    /// `RefCell` because a module-level binding can be `mut`
    /// (`compound-index.vl` counts calls in one), and a `thread_local!`
    /// `static` is otherwise immutable; an immutable binding pays one
    /// `RefCell`, which is the same thing S1a's ruling already pays everywhere
    /// else (R3).
    fn read_module_binding_or_local(&mut self, binding: Id, span: Span) -> Result<String, Error> {
        if !self.module_bindings.contains(&binding) {
            // A binding in neither table is not a binding at all: it is an
            // `is`-test CAPTURE, which the analyzer records as a value aliased
            // to the subject's payload slot with no declaration of its own (the
            // JS emitter substitutes the accessor at every reference through
            // `is_bindings`). Natively the shape is `if let Some(x) = subject`,
            // a restructuring of the `if` and not of the read — so it is named
            // rather than emitted, and it is named HERE because the emitted
            // alternative was a reference to a name nothing declares.
            // J6: a context-threaded hidden parameter is in neither table
            // EITHER, and for the same reason it has no type — `context.rs`
            // keeps it out of `parameters` deliberately. It is a real
            // parameter of the emitted signature, so it reads as its own name;
            // without this exclusion every program reaching a `sleep` was
            // refused as an `is` capture it has nothing to do with.
            if !self.program.variables.contains_key(&binding)
                && !self.program.parameters.contains_key(&binding)
                && !self.is_captures.contains(&binding)
                && !self
                    .program
                    .context_hidden_parameters
                    .contains_key(&binding)
            {
                return self.host_gap(
                    "a value captured by an `is` test outside an `if` condition (only \
                     `if subject is Pattern(let x)` restructures into an `if let`)"
                        .to_string(),
                    span,
                );
            }
            return Ok(self.read_binding(binding));
        }
        let cell = self.ensure_module_binding(binding, span)?;
        Ok(format!("{cell}.with(|cell| cell.get())"))
    }

    /// Emits the `thread_local!` for one module-level binding, once, and
    /// answers the cell's name.
    fn ensure_module_binding(&mut self, binding: Id, span: Span) -> Result<String, Error> {
        let variable = self
            .program
            .variables
            .get(&binding)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved module-level binding", span))?;
        let cell = format!(
            "MODULE_{}_{}",
            sanitize(variable.name).to_uppercase(),
            binding.0
        );
        if !self.module_bindings_started.insert(binding) {
            // Either already emitted, or being emitted right now — a module
            // binding whose initializer reads another one. The cycle is the
            // analyzer's to refuse (`init_order`), so reaching here twice means
            // the first one will land.
            return Ok(cell);
        }
        if self.program.lazy_cells.contains(&binding) {
            // A `lazy` module binding is a memoized thunk (A100), and a
            // `thread_local!` is ALREADY lazily initialized on first access —
            // so the two agree by construction and the only thing that would
            // differ is a read that never happens, which is unobservable.
            // Nothing extra to build; the eager form below is the lazy one.
        }
        let initial = variable
            .initial
            .ok_or_else(|| unsupported("a module-level binding with no initializer", span))?;
        // The initializer is emitted under NO substitution: a module-level
        // binding is outside every instantiation, and a generic it could not
        // ground would be a refusal there rather than a wrong grounding here.
        let saved = std::mem::take(&mut self.current_substitution);
        // The BINDING's type is the initializer's expectation, as it is for a
        // local (B370's law): without it `mut level: u32 = 10` declared a
        // `Shared<u32>` and handed it `(10i32)`.
        let expecting = Some(variable.type_id);
        let rendered = self.rust_type(variable.type_id, span).and_then(|rendered| {
            self.value_of_expecting(initial, expecting, 0)
                .map(|value| (rendered, value))
        });
        self.current_substitution = saved;
        let (rendered, value) = rendered?;
        let mut out = String::new();
        let _ = writeln!(out, "thread_local! {{");
        let _ = writeln!(
            out,
            "    static {cell}: vilan_rt::Shared<{rendered}> = \
             vilan_rt::Shared::new({value});"
        );
        let _ = writeln!(out, "}}");
        self.module_binding_cells.insert(binding.0, out);
        Ok(cell)
    }

    fn declaration(&mut self, binding: Id, depth: usize) -> Result<String, Error> {
        let variable = self
            .program
            .variables
            .get(&binding)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved binding", self.span_of(binding)))?;
        if self.program.lazy_cells.contains(&binding) {
            return Err(unsupported("a `lazy` binding", self.span_of(binding)));
        }
        let name = self.binding_name(binding);
        let mutable = if variable.mutable { "mut " } else { "" };
        // An annotation is WRITTEN ONLY where the initializer cannot type the
        // binding — an empty collection literal, and nothing else.
        //
        // The temptation is to annotate everything, and it is wrong: a binding's
        // `type_id` is its POINTEE's whenever the binding is a view (the type
        // system has no reference form, views being tracked beside it), so
        // `let v = &mut n` would be annotated `i32` while holding `&mut i32`,
        // and so would a binding of a `borrows` call's result. Rust's own
        // inference has the right answer at every one of those sites, and every
        // numeric literal this emitter writes carries its own suffix — so the
        // annotation buys nothing except the chance to be wrong.
        let initializer_needs_a_type = variable.initial.is_some_and(|initial| {
            matches!(self.program.entity_map.get(&initial), Some(Expr::List(items)) if items.is_empty())
        });
        let annotation = if initializer_needs_a_type {
            let rendered = self.rust_type(variable.type_id, self.span_of(binding))?;
            format!(": {rendered}")
        } else {
            String::new()
        };
        match variable.initial {
            Some(initial) => {
                // A binding whose initializer is a `&place` or a `borrows` call
                // HOLDS the view, so the initializer is not read through — see
                // [`Emitter::declaring_a_view`].
                let holds_a_view = self.reads_through_a_view(initial);
                // F21: a view binding initialized from ANOTHER view binding —
                // `let c: &mut i32 = b;` — is a second live loan of one place,
                // and natively the two cannot both be read. Named rather than
                // emitted, and named CONSERVATIVELY: rustc accepts the pair
                // where only one of them is used afterwards, and this refuses
                // the shape. The general answer is a model of aliasing views
                // that the emitter does not have — `transparent-references.vl`
                // is what wants it, and this is the gap that program names.
                if let Some(Expr::Local(source)) = self.program.entity_map.get(&initial)
                    && self.binding_holds_a_view(*source)
                {
                    return Err(unsupported(
                        "a view binding that ALIASES another view binding (`let c = b;` where \
                         `b` is a view: two live loans of one place, which needs a model of \
                         aliasing views this backend has not got)",
                        self.span_of(binding),
                    ));
                }
                let saved = std::mem::replace(&mut self.declaring_a_view, holds_a_view);
                let value = self.value_of_expecting(initial, Some(variable.type_id), depth);
                self.declaring_a_view = saved;
                let value = value?;
                if self.boxed.contains(&binding) {
                    // R3: a mutably-captured binding is a counted cell, so the
                    // declaration builds one and every read and write below goes
                    // through it.
                    self.boxed_emitted.insert(binding);
                    return Ok(format!("let {name} = vilan_rt::Captured::new({value})"));
                }
                Ok(format!("let {mutable}{name}{annotation} = {value}"))
            }
            None => Err(unsupported(
                "a binding with no initializer",
                self.span_of(binding),
            )),
        }
    }

    /// Whether a type is one of the numeric scalar primitives — the set
    /// [`Emitter::number_literal`] lets a declared POSITION override a
    /// literal's own record for.
    fn is_numeric_scalar(&self, type_id: TypeId) -> bool {
        self.resolve(type_id)
            .and_then(|resolved| match resolved {
                Type::Struct(struct_id, _) => self.program.structs.get(struct_id),
                _ => None,
            })
            .and_then(|declaration| scalar_type(declaration.name))
            .is_some_and(|scalar| scalar != "vilan_rt::Str")
    }

    /// Whether two types render to the same Rust type — asked of two numeric
    /// scalars, where it is the question "do these two records agree".
    fn rust_type_key_matches(&self, left: TypeId, right: TypeId) -> bool {
        self.type_key(left) == self.type_key(right)
    }

    fn number_literal(
        &mut self,
        id: Id,
        whole: &str,
        fraction: Option<&str>,
        suffix: Option<&str>,
        span: Span,
    ) -> Result<String, Error> {
        // The literal arrives in THREE pieces (`Expr::Number(whole, fraction,
        // suffix)`), not two. Reading the fraction as the suffix turned `3.5f`
        // into `3i32` — caught here by rustc, which is luck rather than a
        // design, so the pieces are named.
        // F20: the `n` suffix is a `BigInt` literal — ARBITRARY precision, and
        // node prints one with the `n` back on (`9007199254740993n % 4n` is
        // `1n`, not `1`). This runtime is dependency-free by rule, so there is
        // no bignum to lower it to and no width that is the same value AND the
        // same bytes; `i64` is neither. Refused by name rather than narrowed.
        // `remainder.vl` is the corpus's only one and it was refused for its
        // overloaded `%` until now, which is why this had never been asked.
        // F32, RULED (b) 2026-09-22: a `BigInt` is an `i128` natively — a
        // documented LIMIT rather than a bignum this dependency-free runtime
        // has no room for. A literal INSIDE the range is that value; one past
        // it is refused here, at the only place the whole number is still
        // written down, rather than silently narrowed (which is what this
        // emitted before Order 39 made it a refusal: `9007199254740993n` came
        // out as `…993i32`).
        if suffix == Some("n") {
            let digits = whole.replace('_', "");
            let Ok(value) = digits.parse::<i128>() else {
                return Err(Error {
                    trace: Vec::new(),
                    note: None,
                    span,
                    msg: format!(
                        "the `BigInt` literal `{digits}n` is outside the native backend's range: \
                         a `BigInt` is an `i128` there (±1.7e38), which is the documented limit \
                         — the JS backend's is arbitrary precision. Build this program with \
                         `--backend js`, or keep the value inside the limit"
                    ),
                });
            };
            if fraction.is_some() {
                return Err(unsupported("a `BigInt` literal with a fraction", span));
            }
            return Ok(format!("vilan_rt::BigInt({value}i128)"));
        }
        let cleaned = match fraction {
            Some(fraction) => format!("{whole}.{fraction}").replace('_', ""),
            None => whole.replace('_', ""),
        };
        // The literal's own recorded type, else the type of the POSITION it
        // fills. Without the second, `Id::new(0)` against `n: u32` emitted
        // `(0i32)` and rustc refused the argument — JavaScript has one numeric
        // type and never had to ask.
        // The expectation is consulted only when it names a NUMERIC scalar: a
        // literal inside a `List<i32>` initializer inherits the list's own
        // expectation otherwise, and `(1Vec<i32>)` is not a Rust literal.
        let expected_scalar = self.expected_type.filter(|type_id| {
            self.resolve(*type_id)
                .and_then(|resolved| match resolved {
                    Type::Struct(struct_id, _) => self.program.structs.get(struct_id),
                    _ => None,
                })
                .and_then(|declaration| scalar_type(declaration.name))
                .is_some_and(|scalar| scalar != "vilan_rt::Str")
        });
        // Where the POSITION declares a numeric scalar and the literal's own
        // record says a DIFFERENT one, the position wins. The record for a
        // literal is advisory — JavaScript has one numeric type, so nothing on
        // that backend ever had to agree — and the position is a Rust
        // signature the emission has to satisfy. `number-math.vl`'s
        // `16f.as_f32().clamp(0f.as_f32(), 4f.as_f32())` is the shape: `0f` is
        // recorded `f32` (from the clamp argument it eventually fills) while
        // the `as_f32` instance selected for it takes `f64`, and emitting the
        // record gave rustc `expected &f64, found &f32`.
        let own = self.type_of(id).filter(|type_id| {
            expected_scalar.is_none_or(|expected| {
                self.rust_type_key_matches(*type_id, expected) || !self.is_numeric_scalar(*type_id)
            })
        });
        let rendered = match own
            .or(expected_scalar)
            .and_then(|type_id| self.rust_type(type_id, span).ok())
        {
            Some(rust) => rust,
            None => match suffix {
                Some(suffix) => scalar_type(suffix).unwrap_or("i32").to_string(),
                None => {
                    if cleaned.contains('.') {
                        "f64".to_string()
                    } else {
                        "i32".to_string()
                    }
                }
            },
        };
        // A literal with a fraction is a FLOAT, whatever a stale or absent type
        // entry says: `3.5i32` is not a Rust literal at all.
        let rendered = if (fraction.is_some()
            || matches!(suffix, Some("f") | Some("f32") | Some("f64")))
            && !matches!(rendered.as_str(), "f32" | "f64")
        {
            "f64".to_string()
        } else {
            rendered
        };
        if rendered == "f64" || rendered == "f32" {
            let body = if cleaned.contains('.') || cleaned.contains('e') || cleaned.contains('E') {
                cleaned
            } else {
                format!("{cleaned}.0")
            };
            return Ok(format!("({body}{rendered})"));
        }
        if !is_integer_type(&rendered) {
            return Err(unsupported(
                &format!("a numeric literal typed `{rendered}`"),
                span,
            ));
        }
        Ok(format!("({cleaned}{rendered})"))
    }

    fn binary(
        &mut self,
        id: Id,
        op: BinaryOp,
        left: Id,
        right: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        // F20: an OVERLOADED operator is a call to the impl's member, which is
        // what the analyzer already resolved it to — the same record the JS
        // emitter reads, so the two backends dispatch to one answer rather than
        // to two opinions of it. The monomorphisation channel is
        // `call_substitution`'s, keyed on the BINARY expression's own id, which
        // is where a method call's substitution is recorded.
        if let Some(&member_id) = self.program.binary_op_dispatch.get(&id) {
            let substitution = self.call_substitution(id, member_id, &[]);
            let instance = self.ensure_function(member_id, &substitution)?;
            let mut prelude = String::new();
            let arguments = self.call_arguments(member_id, &[left, right], depth, &mut prelude)?;
            let call = Self::with_argument_prelude(
                prelude,
                format!("{}({})", instance.name, arguments.join(", ")),
            );
            // `a != b` dispatches to `eq` and negates: an impl provides `eq`,
            // and `ne` is its `!eq` default.
            return Ok(if matches!(op, BinaryOp::NotEq) {
                format!("!({call})")
            } else {
                call
            });
        }
        // `str + str` is a concatenation, which is a runtime call natively
        // rather than an operator — and an INTERPOLATION is a chain of them
        // whose right halves are whatever was interpolated, rendered. The
        // rendering of a scalar is its `console.log` form on both backends,
        // which is what makes the substitution sound; anything else goes
        // through a user `render` this slice does not monomorphise, so it is
        // refused rather than guessed at.
        // `str + str` is a concatenation, which is a runtime call natively
        // rather than an operator. EITHER side answering `str` makes it one:
        // inside a trait default `self.describe() + "!"` has a left operand
        // whose callee is only known once the default is specialized, so the
        // left side carries no type at all and the literal on the right is the
        // whole evidence. The other side is then rendered AS a string — its
        // `console.log` form, which is what makes the substitution sound for a
        // scalar; anything else goes through a user `render`, which is refused
        // rather than guessed at.
        // An `is`-test that CAPTURES, to the left of `&&`. See
        // [`Emitter::conjunction`].
        if matches!(op, BinaryOp::And) {
            let mut conjuncts = Vec::new();
            self.flatten_conjunction(id, &mut conjuncts);
            if conjuncts
                .iter()
                .any(|conjunct| self.is_condition_captures(*conjunct).is_some())
            {
                return self.conjunction(&conjuncts, depth);
            }
        }
        let concatenates = matches!(op, BinaryOp::Add)
            && (self.is_str_expr(left) || self.is_str_expr(right) || self.is_str(id));
        if concatenates {
            let left_text = self.as_string(left, None, depth, span)?;
            let right_text = self.as_string(right, Some(id), depth, span)?;
            return Ok(format!("vilan_rt::str_concat(&{left_text}, &{right_text})"));
        }
        let symbol = match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitXor => "^",
            BinaryOp::BitOr => "|",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::LtEq => "<=",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::UShr => return Err(unsupported("the `>>>` operator", span)),
        };
        // An operand that is a LOANED parameter is a `&T` natively, and Rust's
        // `PartialEq`/`PartialOrd` are not implemented across the reference
        // (`&i32 == i32`). `value_of` is the rule already written for this —
        // rule 1's copy at a loaned read — so both operands go through it, and
        // `std::compare`'s `impl i32 with PartialEq { fun eq(self, b: i32) {
        // self == b } }` compiles for every scalar it is specialized at.
        let left_text = self.value_of(left, depth)?;
        let right_text = self.value_of(right, depth)?;
        Ok(format!("({left_text} {symbol} {right_text})"))
    }

    /// The conjuncts of a `&&` chain, left to right.
    ///
    /// `&&` is left-associative, so `a && b && c` is `((a && b) && c)` and the
    /// conjuncts are the leaves of that spine.
    fn flatten_conjunction(&self, id: Id, out: &mut Vec<Id>) {
        match self.program.entity_map.get(&id) {
            Some(Expr::Binary(BinaryOp::And, left, right))
                if !self.program.binary_op_dispatch.contains_key(&id) =>
            {
                let (left, right) = (*left, *right);
                self.flatten_conjunction(left, out);
                self.flatten_conjunction(right, out);
            }
            _ => out.push(id),
        }
    }

    /// A `&&` chain in which some conjunct is an `is`-test that CAPTURES.
    ///
    /// `matches!` binds nothing, so the emission `is_test` writes dropped every
    /// capture on the floor and the reads to its right named a binding rustc
    /// had never seen — an emitted program that does not build, which is worse
    /// than a refusal (`derive-json.vl` and `json-roundtrip.vl` both write the
    /// shape, and both were held behind an earlier refusal until `std::json`
    /// went native). `if` had the same problem and answers it with `if let`;
    /// this is the same answer one level down:
    ///
    /// ```text
    /// `p is Ok(let v) && f(v)`  →  `match p { Ok(v) => f(v), _ => false }`
    /// ```
    ///
    /// which is exactly what `&&` means when its left operand binds — the
    /// short-circuit IS the `_` arm, and B215's rule ("a capture binds nothing
    /// after the test that made it") is the scope of the arm.
    ///
    /// Everything to the RIGHT of a capturing conjunct goes inside that arm,
    /// which is why this takes the whole flattened chain rather than one
    /// `Binary` node: `a is P(let x) && b && x.f` is `((a is P && b) && x.f)`
    /// as a tree, and nesting per node would have left `x.f` outside.
    fn conjunction(&mut self, conjuncts: &[Id], depth: usize) -> Result<String, Error> {
        let Some((&first, rest)) = conjuncts.split_first() else {
            return Ok("true".to_string());
        };
        if rest.is_empty() {
            return self.expression(first, depth);
        }
        let Some((subject, pattern, bindings)) = self.is_condition_captures(first) else {
            let left = self.expression(first, depth)?;
            let right = self.conjunction(rest, depth)?;
            return Ok(format!("({left} && {right})"));
        };
        // The same copy `if let` takes (F20): a pattern over a PLACE binds its
        // captures by reference under Rust's default binding modes, and a
        // capture is a copy by rule 1.
        let mut subject_text = self.expression(subject, depth)?;
        if matches!(
            self.program.entity_map.get(&subject),
            Some(Expr::Local(_) | Expr::Parameter(_) | Expr::Field(_, _, _))
        ) {
            subject_text = format!("({subject_text}).clone()");
        }
        let subject_type = self.type_of(subject);
        let pattern_text = self.pattern(&pattern, subject_type, self.span_of(first))?;
        for binding in &bindings {
            self.is_captures.insert(*binding);
        }
        let rest_text = self.conjunction(rest, depth);
        for binding in &bindings {
            self.is_captures.remove(binding);
        }
        Ok(format!(
            "match {subject_text} {{ {pattern_text} => {}, _ => false }}",
            rest_text?
        ))
    }

    /// One operand of a concatenation, rendered as a `str`.
    ///
    /// `concat` names the enclosing binary expression for the RIGHT operand,
    /// which is where the analyzer records a render dispatch (B176): `"v=" +
    /// value` with `value: T` bounded to a trait providing `to_string` is
    /// ADMITTED, so the emission owes the operand that impl's answer and not
    /// the value's runtime shape.
    fn as_string(
        &mut self,
        id: Id,
        concat: Option<Id>,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        if let Some(&(constraint_id, trait_id)) =
            concat.and_then(|concat| self.program.concat_render_dispatch.get(&concat))
            && let Some(&bound) = self.current_substitution.get(&constraint_id)
        {
            let concrete = self.concrete(bound);
            // Never-silent (B55's pattern): falling through here would emit the
            // very raw rendering this channel exists to replace, and the
            // program would print the wrong string.
            let Some(dispatch) = self.resolve_dispatch(
                concrete,
                RENDER_MEMBER,
                &[],
                Some((trait_id, Vec::new())),
                span,
            )?
            else {
                return Err(unsupported(
                    "a concatenation whose operand's `to_string` bound resolves to no impl",
                    span,
                ));
            };
            return self.emit_dispatch(dispatch, &[id], depth, span);
        }
        let text = self.expression(id, depth)?;
        if self.is_str_expr(id) {
            return Ok(text);
        }
        // A scalar renders as its `console.log` form, which is the same string
        // on both backends. So does an operand whose type this emitter cannot
        // see at all — which happens for exactly one shape, a call inside a
        // trait default whose callee the specialization picks, and whose real
        // type is the `str` the analyzer already checked. `js_of` of a `str` is
        // that `str`, so the two cases share an answer.
        if self.is_scalar_expr(id) || self.type_of(id).is_none() {
            return Ok(format!("vilan_rt::js_of(&({text}))"));
        }
        Err(unsupported(
            "an interpolation of a value with its own `render`",
            span,
        ))
    }

    /// Whether an expression is a `str` — by its resolved type where it has
    /// one, and structurally where it does not (a literal, or a concatenation
    /// whose left half is one). The concatenation chain an interpolation
    /// desugars to carries a type on none of its joints.
    fn is_str_expr(&self, id: Id) -> bool {
        if self.is_str(id) {
            return true;
        }
        match self.program.entity_map.get(&id) {
            Some(Expr::String(_)) | Some(Expr::MultilineString(_)) => true,
            Some(Expr::Binary(BinaryOp::Add, left, _)) => self.is_str_expr(*left),
            _ => false,
        }
    }

    /// Whether an expression's type is a scalar primitive — the set whose
    /// `render` and whose `console.log` rendering are the same string.
    fn is_scalar_expr(&self, id: Id) -> bool {
        self.type_of(id)
            .and_then(|type_id| self.resolve(type_id))
            .and_then(|resolved| match resolved {
                Type::Struct(struct_id, _) => self.program.structs.get(struct_id),
                _ => None,
            })
            .is_some_and(|declaration| {
                scalar_type(declaration.name).is_some() && declaration.name != "str"
            })
            || self
                .type_of(id)
                .and_then(|type_id| self.resolve(type_id))
                .is_some_and(|resolved| {
                    matches!(resolved, Type::Enum(enum_id, _)
                    if self.program.bool_enum_id == Some(*enum_id))
                })
    }

    fn is_str(&self, id: Id) -> bool {
        self.type_of(id)
            .and_then(|type_id| self.resolve(type_id))
            .and_then(|resolved| match resolved {
                Type::Struct(struct_id, _) => self.program.structs.get(struct_id),
                _ => None,
            })
            .is_some_and(|declaration| declaration.name == "str")
    }

    fn if_branch(&mut self, branch: &ExprIfBranch, depth: usize) -> Result<String, Error> {
        let pad = Self::indent(depth);
        match branch {
            ExprIfBranch::If(condition, (statements, tail), next) => {
                // `if subject is Some(let x) { .. }` is Rust's `if let`, and it
                // is the ONLY place an `is`-test capture has a name natively: a
                // capture has no declaration of its own (the JS emitter
                // substitutes the payload accessor at every reference), so
                // `matches!` — which binds nothing — cannot serve a pattern
                // that captures. Every other position keeps the refusal.
                let captured = self.is_condition_captures(*condition);
                let head = match &captured {
                    Some((subject, pattern, bindings)) => {
                        let mut subject_text = self.expression(*subject, depth)?;
                        // The same copy a destructuring `match` takes (F20): an
                        // `if let` over a PLACE binds its captures by REFERENCE
                        // under Rust's default binding modes, so
                        // `std::compare`'s `if this is Some(let x)` on a
                        // `&Option<i32>` bound `x: &i32` and `x == y` had no
                        // `PartialEq` across the reference. A capture is a copy
                        // (rule 1), and copying the subject is how it binds one.
                        if matches!(
                            self.program.entity_map.get(subject),
                            Some(Expr::Local(_) | Expr::Parameter(_) | Expr::Field(_, _, _))
                        ) {
                            subject_text = format!("({subject_text}).clone()");
                        }
                        let subject_type = self.type_of(*subject);
                        let pattern_text =
                            self.pattern(pattern, subject_type, self.span_of(*condition))?;
                        for binding in bindings {
                            self.is_captures.insert(*binding);
                        }
                        format!("if let {pattern_text} = {subject_text}")
                    }
                    None => format!("if {}", self.expression(*condition, depth)?),
                };
                let mut body = String::new();
                let walked = self.emit_block(statements, *tail, &mut body, depth + 1);
                if let Some((_, _, bindings)) = &captured {
                    for binding in bindings {
                        self.is_captures.remove(binding);
                    }
                }
                walked?;
                let mut out = format!("{head} {{\n{body}{pad}}}");
                if let Some(next) = next {
                    let _ = write!(out, " else {}", self.if_branch(next, depth)?);
                }
                Ok(out)
            }
            ExprIfBranch::Else((statements, tail)) => {
                let mut body = String::new();
                self.emit_block(statements, *tail, &mut body, depth + 1)?;
                Ok(format!("{{\n{body}{pad}}}"))
            }
        }
    }

    fn match_expr(
        &mut self,
        subject: Id,
        legs: &[ExprMatchLeg],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let saved_matching = std::mem::replace(&mut self.matching_the_subject, true);
        let rendered_subject = self.expression(subject, depth);
        self.matching_the_subject = saved_matching;
        let mut subject_text = rendered_subject?;
        let subject_type = self.type_of(subject);
        // A leg that DESTRUCTURES moves the payload out of the subject, so a
        // subject that is a PLACE has to be copied first (F20). On the JS
        // backend a capture is an accessor into the value the subject names and
        // the place is still readable afterwards; `std::reactive`'s `dispose`
        // matches its ambient turn and then publishes the same binding on
        // `releasing_turns`, which rustc read as a use after a partial move.
        //
        // The copy is rule 1's, and `clone_sites` marks none here because the
        // JS backend owes none. It is taken only when a leg really binds, so a
        // `match` over payload-less variants keeps the bytes S1a emitted.
        let destructures = legs.iter().any(|leg| {
            let mut bindings = Vec::new();
            collect_pattern_bindings(&leg.pattern, &mut bindings);
            !bindings.is_empty()
        });
        if destructures
            && matches!(
                self.program.entity_map.get(&subject),
                Some(Expr::Local(_) | Expr::Parameter(_) | Expr::Field(_, _, _))
            )
        {
            subject_text = format!("({subject_text}).clone()");
        }
        // A `str` subject is matched as a `&str`, which is the only form a
        // string LITERAL pattern has natively — see [`Emitter::pattern`]. The
        // question is asked of the PATTERNS rather than of the subject's type
        // because a subject that is a call (`s.trim()`) carries no type on its
        // own id, and a string literal in a leg is the whole evidence.
        let matches_a_string = legs.iter().any(|leg| match &leg.pattern {
            ExprPattern::Literal(id) => Self::string_pattern_text(self.program, *id).is_some(),
            // A `str`-BACKED enum's variant is a string literal too.
            ExprPattern::Variant(enum_id, _, _) => self
                .program
                .enums
                .get(enum_id)
                .is_some_and(|declaration| declaration.backing == Some(Backing::Str)),
            _ => false,
        });
        if matches_a_string {
            subject_text = format!("&*({subject_text})");
        }
        let pad = Self::indent(depth);
        let leg_pad = Self::indent(depth + 1);
        let mut out = format!("match {subject_text} {{\n");
        let mut has_catch_all = false;
        for leg in legs {
            if leg.guard.is_some() {
                return Err(unsupported("a guarded `match` leg", span));
            }
            let pattern = self.pattern(&leg.pattern, subject_type, span)?;
            if matches!(leg.pattern, ExprPattern::Wildcard | ExprPattern::Binding(_))
                || self.pattern_is_bool(&leg.pattern)
            {
                // A `bool` match with both legs is exhaustive natively too, and
                // an extra arm after it is an `unreachable_patterns` lint rather
                // than a safety net.
                has_catch_all = true;
            }
            let body = self.expression(leg.body, depth + 1)?;
            let _ = writeln!(out, "{leg_pad}{pattern} => {body},");
        }
        // vilan's exhaustiveness is checked by vilan; rustc re-checks it over a
        // shape it cannot always see (a literal match on an integer, say), so a
        // match with no catch-all gets one that cannot be reached.
        if !has_catch_all {
            let _ = writeln!(
                out,
                "{leg_pad}_ => vilan_rt::panic_with(\"unreachable match leg\"),"
            );
        }
        let _ = write!(out, "{pad}}}");
        Ok(out)
    }

    /// An `if` condition that is an `is`-test WITH captures: the subject, the
    /// pattern, and the binding ids it introduces. `None` for every other
    /// condition, including an `is` test that binds nothing (which stays a
    /// `matches!`, so `if x is None` reads as it did).
    fn is_condition_captures(&self, condition: Id) -> Option<(Id, ExprPattern, Vec<Id>)> {
        let Some(Expr::Is(subject, pattern)) = self.program.entity_map.get(&condition).cloned()
        else {
            return None;
        };
        let mut bindings = Vec::new();
        collect_pattern_bindings(&pattern, &mut bindings);
        (!bindings.is_empty()).then_some((subject, pattern, bindings))
    }

    /// The source text of a string-LITERAL pattern, or `None` for every other
    /// literal. A vilan `str` is an `Rc<str>` whose literal emits
    /// `vilan_rt::str_new("..")` — a function call, and no pattern at all — so
    /// this is what tells [`Emitter::pattern`] and [`Emitter::match_expr`] that
    /// the leg is a `&str` one.
    fn string_pattern_text(program: &'a Program<'src>, id: Id) -> Option<&'src str> {
        match program.entity_map.get(&id) {
            Some(Expr::String(text)) => Some(text),
            _ => None,
        }
    }

    fn pattern_is_bool(&self, pattern: &ExprPattern) -> bool {
        matches!(pattern, ExprPattern::Variant(enum_id, _, _)
            if self.program.bool_enum_id == Some(*enum_id))
    }

    /// One pattern, against the type of the value it matches.
    ///
    /// The subject's type is threaded because a variant pattern on a GENERIC
    /// enum names one instance's variant: `Tree<i32>::Leaf` and
    /// `Tree<str>::Leaf` are two Rust paths, and a pattern carries no type of
    /// its own to read them off.
    fn pattern(
        &mut self,
        pattern: &ExprPattern,
        subject_type: Option<TypeId>,
        span: Span,
    ) -> Result<String, Error> {
        match pattern {
            ExprPattern::Wildcard => Ok("_".to_string()),
            ExprPattern::Binding(id) => Ok(self.binding_name(*id)),
            // A `str` LITERAL pattern (F20). A vilan `str` is an `Rc<str>` and
            // its literal emits `vilan_rt::str_new("..")`, which is a function
            // CALL and no pattern at all — `parse-bool.vl` matches `"true"` /
            // `"false"` and rustc refused both legs. The subject is matched as a
            // `&str` ([`Emitter::match_expr`]), so the pattern is the bare Rust
            // literal, which is what a `&str` pattern is.
            ExprPattern::Literal(id) if Self::string_pattern_text(self.program, *id).is_some() => {
                Ok(rust_string(
                    Self::string_pattern_text(self.program, *id).expect("just tested"),
                ))
            }
            ExprPattern::Literal(id) => self.expression(*id, 0),
            ExprPattern::Variant(enum_id, index, payload) => {
                // A BACKED variant is its literal, which is the same pattern a
                // `match` over a raw number or a raw `str` already takes.
                if let Some(declaration) = self.program.enums.get(enum_id).cloned()
                    && let Some(value) = Self::backed_variant_value(&declaration, *index)
                {
                    let _ = payload;
                    return Ok(match declaration.backing {
                        // A `str`-backed variant matches as a `&str` literal;
                        // the subject is deref'd by `match_expr`.
                        Some(Backing::Str) => match &declaration.variants[*index].backing_value {
                            BackingValue::Str(text) => rust_string(text),
                            BackingValue::Int(discriminant) => format!("{discriminant}"),
                        },
                        // An integer pattern carries no suffix: the subject's
                        // own type decides the width.
                        _ => value
                            .trim_start_matches('(')
                            .trim_end_matches(')')
                            .trim_end_matches("i32")
                            .to_string(),
                    });
                }
                if let Some(declaration) = self.program.enums.get(enum_id)
                    && declaration.name == "bool"
                {
                    // `bool` is an enum in the source and a native boolean at
                    // runtime on both backends.
                    return Ok(if *index == 1 { "true" } else { "false" }.to_string());
                }
                let arguments = self.enum_arguments_of(subject_type, *enum_id);
                let path = self.variant_path(*enum_id, *index, &arguments, span)?;
                if payload.is_empty() {
                    return Ok(path);
                }
                let payload_types = self.variant_payload_types(*enum_id, *index, &arguments);
                let mut parts = Vec::new();
                for (slot, sub) in payload.iter().enumerate() {
                    parts.push(self.pattern(sub, payload_types.get(slot).copied(), span)?);
                }
                Ok(format!("{path}({})", parts.join(", ")))
            }
            ExprPattern::Tuple(elements) => {
                let element_types = match subject_type.and_then(|type_id| self.resolve(type_id)) {
                    Some(Type::Tuple(types)) => types.clone(),
                    _ => Vec::new(),
                };
                let mut parts = Vec::new();
                for (slot, (element, _)) in elements.iter().enumerate() {
                    parts.push(self.pattern(element, element_types.get(slot).copied(), span)?);
                }
                Ok(format!("({},)", parts.join(", ")))
            }
            ExprPattern::Array(_) => Err(unsupported("an array pattern", span)),
        }
    }

    /// The arguments `enum_id` is instantiated at, from a known subject type.
    fn enum_arguments_of(&self, subject_type: Option<TypeId>, enum_id: Id) -> Vec<TypeId> {
        if let Some(Type::Enum(found, arguments)) =
            subject_type.and_then(|type_id| self.resolve(type_id))
            && *found == enum_id
        {
            return arguments.clone();
        }
        self.program
            .enums
            .get(&enum_id)
            .map(|declaration| declaration.generic_parameter_constraint_ids.clone())
            .unwrap_or_default()
    }

    /// One variant's payload types, resolved under the instantiation — the
    /// subject types a nested pattern needs.
    fn variant_payload_types(
        &mut self,
        enum_id: Id,
        index: usize,
        arguments: &[TypeId],
    ) -> Vec<TypeId> {
        let Some(declaration) = self.program.enums.get(&enum_id).cloned() else {
            return Vec::new();
        };
        let Some(variant) = declaration.variants.get(index) else {
            return Vec::new();
        };
        let entries =
            self.nominal_entries(&declaration.generic_parameter_constraint_ids, arguments);
        let saved = self.enter_substitution(entries);
        let resolved = variant
            .data_type_ids
            .iter()
            .map(|type_id| self.concrete(*type_id))
            .collect();
        self.current_substitution = saved;
        resolved
    }

    fn is_test(
        &mut self,
        subject: Id,
        pattern: &ExprPattern,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let subject_text = self.expression(subject, depth)?;
        let subject_type = self.type_of(subject);
        let pattern_text = self.pattern(pattern, subject_type, span)?;
        Ok(format!("matches!({subject_text}, {pattern_text})"))
    }

    /// One variant's Rust path, in the INSTANCE `arguments` name.
    ///
    /// `Tree<i32>::Leaf` and `Tree<str>::Leaf` are variants of two Rust enums,
    /// so the path cannot be derived from the declaration alone — which is why
    /// every caller has to say which instantiation it is in.
    fn variant_path(
        &mut self,
        enum_id: Id,
        index: usize,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<String, Error> {
        let declaration = self
            .program
            .enums
            .get(&enum_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved enum", span))?;
        let variant = declaration
            .variants
            .get(index)
            .ok_or_else(|| unsupported("an unresolved enum variant", span))?;
        if let Some(value) = Self::backed_variant_value(&declaration, index) {
            return Ok(value);
        }
        match declaration.name {
            "bool" => Ok(if index == 1 { "true" } else { "false" }.to_string()),
            "Option" | "Result" => Ok(sanitize(variant.name)),
            _ => {
                let name = sanitize(variant.name);
                let instance = self.ensure_enum(enum_id, arguments, span)?;
                Ok(format!("{}::{name}", instance.name))
            }
        }
    }

    /// The bare backing value of a variant, if its enum is BACKED — `Align::Start`
    /// is the string `"start"` exactly as `Ordering::Greater` is the number `1`
    /// (backed-enums.md §3.5). `None` for every array-form enum, and for `bool`,
    /// which lowers to a native scalar through its own special case.
    fn backed_variant_value(
        declaration: &vilan_core::analyzer::Enum<'src>,
        index: usize,
    ) -> Option<String> {
        declaration.backing?;
        match &declaration.variants.get(index)?.backing_value {
            BackingValue::Int(discriminant) => Some(format!("({discriminant}i32)")),
            // The declaration carries the RAW literal text, so it unescapes at
            // emission exactly like any other string literal.
            BackingValue::Str(text) => {
                Some(format!("vilan_rt::str_new({})", rust_string(text.as_str())))
            }
        }
    }

    /// The type arguments an enum is instantiated at, at one expression — from
    /// the type the analyzer recorded there, else the declaration's own
    /// parameters (which for a non-generic enum is the empty list, and for a
    /// generic one leaves the parameters unbound so the refusal names them).
    fn enum_arguments_at(&self, expr_id: Id, enum_id: Id) -> Vec<TypeId> {
        if let Some(Type::Enum(found, arguments)) = self
            .type_of(expr_id)
            .and_then(|type_id| self.resolve(type_id))
            && *found == enum_id
        {
            return arguments.clone();
        }
        self.program
            .enums
            .get(&enum_id)
            .map(|declaration| declaration.generic_parameter_constraint_ids.clone())
            .unwrap_or_default()
    }

    /// The arguments a VARIANT CONSTRUCTOR instantiates its enum at.
    ///
    /// Two sources, in order, because neither is total. The type recorded at the
    /// call site is the general answer, but for a generic enum it can be
    /// OPEN — `Tree::Leaf(7)` records `Tree<any>`, its parameter still a hole
    /// the surrounding `let`'s annotation closes later (this is B357's shape
    /// from the emitter's side). So an open argument list falls back to the one
    /// place the answer is certainly present: the constructor's own arguments,
    /// bound against the variant's declared payload types by the same walk that
    /// binds an impl subject.
    ///
    /// A NULLARY variant of a generic enum has neither — nothing to read the
    /// parameter off — and the refusal names it rather than instantiating a
    /// second Rust enum over a hole.
    fn variant_arguments(
        &mut self,
        expr_id: Id,
        enum_id: Id,
        index: usize,
        argument_ids: &[Id],
    ) -> Vec<TypeId> {
        let Some(declaration) = self.program.enums.get(&enum_id).cloned() else {
            return Vec::new();
        };
        let parameters = &declaration.generic_parameter_constraint_ids;
        if parameters.is_empty() {
            return Vec::new();
        }
        let recorded = self.enum_arguments_at(expr_id, enum_id);
        if recorded.len() == parameters.len()
            && recorded.iter().all(|argument| self.is_grounded(*argument))
        {
            return recorded;
        }
        // The position this constructor is being emitted into, when it declares
        // the same enum with its arguments closed.
        if let Some(Type::Enum(found, expected)) =
            self.expected_type.and_then(|type_id| self.resolve(type_id))
            && *found == enum_id
            && expected.len() == parameters.len()
        {
            let expected = expected.clone();
            if expected.iter().all(|argument| self.is_grounded(*argument)) {
                return expected;
            }
        }
        let mut bound: HashMap<TypeId, TypeId> = HashMap::default();
        if let Some(variant) = declaration.variants.get(index) {
            for (slot, data_type_id) in variant.data_type_ids.iter().enumerate() {
                if let Some(argument_type) = argument_ids.get(slot).and_then(|id| self.type_of(*id))
                {
                    let concrete = self.concrete(argument_type);
                    self.bind_parameters(parameters, *data_type_id, concrete, &mut bound);
                }
            }
        }
        parameters
            .iter()
            .map(|parameter| bound.get(parameter).copied().unwrap_or(*parameter))
            .collect()
    }

    /// Binds a declaration's own generic `parameters` from the matching
    /// positions of a concrete type — `Leaf(T)` against `Leaf(i32)` giving
    /// `{T -> i32}`.
    ///
    /// `impl_select::bind_subject` is the same walk for an impl SUBJECT, and it
    /// is not reusable here: it is not told which ids are the DECLARATION's own
    /// parameters, which is what lets this one bind a nominal's body.
    ///
    /// B366: it used to carry a second branch for a parameter appearing in its
    /// own body as the bare CONSTRAINT ID rather than as a `Generic(..)` node.
    /// hygiene-39 measured the whole loaded world — 44 declared generic
    /// parameters, 131 `Generic(..)` nodes, ZERO bare constraint ids — and
    /// instrumented this function over a `--backend rust` build of all 131
    /// corpus programs: entered twice, both times through the `Generic(..)` arm,
    /// never the bare one. Planting the bare spelling makes the ANALYZER panic
    /// before any gate sees it, so the branch was unreachable rather than
    /// rarely reached. `vilan-core`'s `nominal_generic_spelling` holds the
    /// invariant now.
    fn bind_parameters(
        &self,
        parameters: &[TypeId],
        pattern: TypeId,
        concrete: TypeId,
        out: &mut HashMap<TypeId, TypeId>,
    ) {
        let Some(_guard) = vilan_core::util::RecursionGuard::enter() else {
            return;
        };
        match self.program.type_id_to_type_map.get(&pattern) {
            Some(Type::Generic(constraint_id)) if parameters.contains(constraint_id) => {
                out.entry(*constraint_id).or_insert(concrete);
            }
            Some(
                Type::Struct(_, pattern_arguments)
                | Type::Enum(_, pattern_arguments)
                | Type::Tuple(pattern_arguments),
            ) => {
                let pattern_arguments = pattern_arguments.clone();
                let concrete_arguments = match self.program.type_id_to_type_map.get(&concrete) {
                    Some(
                        Type::Struct(_, arguments)
                        | Type::Enum(_, arguments)
                        | Type::Tuple(arguments),
                    ) => arguments.clone(),
                    _ => return,
                };
                for (inner_pattern, inner_concrete) in
                    pattern_arguments.iter().zip(concrete_arguments.iter())
                {
                    self.bind_parameters(parameters, *inner_pattern, *inner_concrete, out);
                }
            }
            _ => {}
        }
    }

    /// Whether a type argument is CLOSED — something a Rust type can be minted
    /// from. `any`, an unresolved hole and a still-abstract generic are not.
    fn is_grounded(&self, type_id: TypeId) -> bool {
        match self.resolve(type_id) {
            Some(Type::Any | Type::Unknown | Type::Unresolved | Type::Generic(_)) | None => false,
            Some(
                Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Tuple(arguments),
            ) => arguments
                .clone()
                .iter()
                .all(|inner| self.is_grounded(*inner)),
            Some(_) => true,
        }
    }

    /// The same, for a struct.
    fn struct_arguments_at(&self, expr_id: Id, struct_id: Id) -> Vec<TypeId> {
        // The POSITION's type, where the recorded one names the same struct but
        // could not ground it. `struct Handle<T> { index: i32, generation: i32 }`
        // has a PHANTOM parameter, so `Handle { index, generation }` inside
        // `impl Arena<T>` gives the analyzer nothing to infer `T` from and the
        // literal keeps the open argument — the emitter minted the open
        // instantiation while the signature said the concrete one, and rustc
        // saw two structs. It can narrow an answer and never change one: the
        // expectation is consulted only when it names the SAME declaration.
        if let Some(Type::Struct(expected, expected_arguments)) =
            self.expected_type.and_then(|type_id| self.resolve(type_id))
            && *expected == struct_id
            && expected_arguments.iter().all(|argument| {
                !matches!(
                    self.resolve(self.concrete(*argument)),
                    Some(Type::Generic(_))
                )
            })
        {
            let grounded: Vec<TypeId> = expected_arguments
                .iter()
                .map(|argument| self.concrete(*argument))
                .collect();
            if !grounded.is_empty() {
                return grounded;
            }
        }
        if let Some(Type::Struct(found, arguments)) = self
            .type_of(expr_id)
            .and_then(|type_id| self.resolve(type_id))
            && *found == struct_id
            && arguments.iter().all(|argument| self.is_grounded(*argument))
        {
            return arguments.clone();
        }
        // The position this literal is being emitted into, when it declares the
        // same struct with its arguments CLOSED — `variant_arguments`' fallback
        // for a variant constructor, which a struct literal needs for the same
        // reason: a trait default's `Taken { upstream = self, remaining = count }`
        // records `Taken<Self, T>` in the DEFAULT's own context, so the literal
        // and the declared return type minted two Rust structs and the body
        // handed back the wrong one (`generic-adapter-dispatch.vl`).
        if let Some(Type::Struct(found, expected)) =
            self.expected_type.and_then(|type_id| self.resolve(type_id))
            && *found == struct_id
            && expected.iter().all(|argument| self.is_grounded(*argument))
        {
            return expected.clone();
        }
        if let Some(Type::Struct(found, arguments)) = self
            .type_of(expr_id)
            .and_then(|type_id| self.resolve(type_id))
            && *found == struct_id
        {
            // Through the SUBSTITUTION in force: a literal written inside
            // `impl Arena<T>` records `Handle<T>`, and an instance emitted at
            // `T = i32` must build `Handle<i32>` rather than the open one — the
            // body said `Handle { .. }` and the signature said `Handle<i32>`,
            // so rustc saw two different structs.
            return arguments
                .iter()
                .map(|argument| self.concrete(*argument))
                .collect();
        }
        self.program
            .structs
            .get(&struct_id)
            .map(|declaration| declaration.generic_parameter_constraint_ids.clone())
            .unwrap_or_default()
    }

    fn field_name(&self, subject: Id, index: usize, span: Span) -> Result<String, Error> {
        let struct_id = self
            .type_of(subject)
            .and_then(|type_id| self.resolve(type_id))
            .and_then(|resolved| match resolved {
                Type::Struct(id, _) => Some(*id),
                _ => None,
            })
            .ok_or_else(|| unsupported("a field read of an unresolved subject", span))?;
        let declaration = self
            .program
            .structs
            .get(&struct_id)
            .ok_or_else(|| unsupported("a field read of an unresolved struct", span))?;
        declaration
            .fields
            .get(index)
            .map(|field| sanitize(field.name))
            .ok_or_else(|| unsupported("a field read past the struct's fields", span))
    }

    fn struct_literal(
        &mut self,
        struct_id: Id,
        arguments: &[TypeId],
        fields: &[(usize, Id)],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let declaration = self
            .program
            .structs
            .get(&struct_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved struct literal", span))?;
        let entries =
            self.nominal_entries(&declaration.generic_parameter_constraint_ids, arguments);
        let instance = self.ensure_struct(struct_id, arguments, span)?;
        let mut parts = Vec::new();
        for (index, value) in fields.iter() {
            let field = declaration
                .fields
                .get(*index)
                .ok_or_else(|| unsupported("a struct field past the declaration", span))?;
            let name = sanitize(field.name);
            // The field's declared type under THIS instantiation, so a field
            // holding a generic enum can ground it.
            let saved = self.enter_substitution(entries.clone());
            let expecting = self.concrete(field.type_id);
            self.current_substitution = saved;
            // A field declared `async |T| U` takes a future-answering closure.
            self.expects_async_value = self.program.async_fields.contains(&(struct_id, *index));
            let rendered = self.value_of_expecting(*value, Some(expecting), depth);
            self.expects_async_value = false;
            parts.push(format!("{name}: {}", rendered?));
        }
        Ok(format!("{} {{ {} }}", instance.name, parts.join(", ")))
    }

    fn for_each(
        &mut self,
        id: Id,
        iterable: Id,
        item: Option<Id>,
        statements: &[Id],
        tail: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        // Only a `List` and a fixed array iterate natively in S1a. Anything
        // else is an `Iterator` impl, which is a monomorphised generic — S1b's.
        // The iterable may be written `&mut xs`, whose own id carries no type —
        // ask about the place it views.
        let iterable_place = match self.program.entity_map.get(&iterable) {
            Some(Expr::Reference(operand, _)) => *operand,
            _ => iterable,
        };
        let iterates_something_else = self
            .type_of(iterable_place)
            .and_then(|type_id| self.resolve(type_id))
            .is_some_and(|resolved| match resolved {
                Type::Array(_, _) => false,
                Type::Struct(struct_id, _) => self
                    .program
                    .structs
                    .get(struct_id)
                    .is_none_or(|declaration| declaration.name != "List"),
                // An unresolved iterable is not a refusal: a list LITERAL
                // carries no type on its own id, and refusing on silence would
                // refuse the commonest loop in the corpus.
                _ => false,
            });
        if iterates_something_else {
            return self.for_each_iterator(id, iterable_place, item, statements, tail, depth, span);
        }
        let iterable_text = self.expression(iterable, depth)?;
        let binder = match item {
            Some(item) => self.binding_name(item),
            None => "_".to_string(),
        };
        let mut body = String::new();
        self.emit_block(statements, tail, &mut body, depth + 1)?;
        let pad = Self::indent(depth);
        // A `for` over a place the loop does not own only READS it (spec §6.1's
        // "a temporary that only reads"), so it iterates a borrow. A `&mut`
        // iteration is recorded on `for_each_views`.
        // A `for` over a place the loop does not own binds VALUES (rule 1: the
        // element is a copy), which is `.clone().into_iter()` natively — the
        // borrow `&container` would bind `&T` and every use of the binder would
        // then be a reference where the program wrote a value. A `&mut`
        // iteration is the one that really binds views, and it is recorded on
        // `for_each_views`.
        let iteration = match self.program.for_each_views.get(&item.unwrap_or(Id(0))) {
            Some(true) => format!("({iterable_text}).iter_mut()"),
            Some(false) => format!("({iterable_text}).iter()"),
            None => format!("({iterable_text}).clone().into_iter()"),
        };
        Ok(format!("for {binder} in {iteration} {{\n{body}{pad}}}"))
    }

    /// A `for` over an **`Iterator` impl** — the largest refusal class this
    /// backend had (F18 slice 2; `std::range`'s `Range` is the one every
    /// counted loop in std goes through, and `Bytes::to_hex` is why the exit
    /// program needed it).
    ///
    /// The lowering is the JS emitter's, expressed in Rust's own `while let`:
    /// the analyzer records the loop's `next` member on `for_each_next`, so the
    /// protocol is not re-derived here — it is READ, from the same record the
    /// other backend reads, which is what keeps the two from having two
    /// opinions about which `next` a loop calls.
    ///
    /// ```text
    /// `for x in r { .. }`  →  `{ let mut it = r; while let Some(x) = next(&mut it) { .. } }`
    /// ```
    ///
    /// `next` takes `&mut self`, so the iterator is a `mut` binding of its own
    /// and the receiver is a borrow of it: the loop ADVANCES the iterator, and
    /// an iterator advanced through a copy would not terminate. The binding is
    /// scoped to a block so the name cannot collide with the body's.
    ///
    /// An iterator whose `next` resolves to an intrinsic or to a host binding
    /// is refused by name rather than guessed at — no `Iterator` impl in std is
    /// either, and one that were would need its own arm.
    fn for_each_iterator(
        &mut self,
        id: Id,
        iterable: Id,
        item: Option<Id>,
        statements: &[Id],
        tail: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let Some(_next_id) = self.program.for_each_next.get(&id).copied() else {
            return Err(unsupported(
                concat!(
                    "a `for` over anything but a `List` ",
                    "(an `Iterator` impl is a monomorphised generic)"
                ),
                span,
            ));
        };
        let Some(subject) = self.type_of(iterable).map(|type_id| self.concrete(type_id)) else {
            return Err(unsupported(
                "a `for` over an iterator of unresolved type",
                span,
            ));
        };
        let preferred = self.program.bound_dispatch_traits.get(&id).cloned();
        let Some(NativeDispatch::Call(next)) =
            self.resolve_dispatch(subject, "next", &[], preferred, span)?
        else {
            return Err(unsupported(
                "a `for` over an iterator whose `next` is not an ordinary member",
                span,
            ));
        };
        // The iterable is CONSUMED by the loop (the iterator is advanced), so a
        // read of a place copies — rule 1's answer at a consuming position.
        let iterable_text = self.consumed_value_of(iterable, depth)?;
        let binder = match item {
            Some(item) => self.binding_name(item),
            None => "_".to_string(),
        };
        let mut body = String::new();
        self.emit_block(statements, tail, &mut body, depth + 2)?;
        let pad = Self::indent(depth);
        let inner = Self::indent(depth + 1);
        Ok(format!(
            "{{\n{inner}let mut iterator = {iterable_text};\n\
             {inner}while let Some({binder}) = {next}(&mut iterator) {{\n{body}{inner}}}\n{pad}}}"
        ))
    }

    fn closure(&mut self, closure_id: Id, depth: usize, span: Span) -> Result<String, Error> {
        let closure = self
            .program
            .closures
            .get(&closure_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved closure", span))?;
        // J6: an `async` closure VALUE. Natively its type is different in kind
        // — a closure that answers a future, not a value — and a callee handed
        // one at one site and a synchronous closure at another is compiled TWICE
        // on the JS backend (async-polymorphism.md A.1's adapted instances,
        // which this emitter does not model: it monomorphizes on types, and
        // asyncness is not one). So it is named rather than emitted. The spawn
        // (`async <body>`) and the runtime helpers that take a body do NOT come
        // through here — they read the body directly and wrap it in a future.
        // F20: a DECLARED-async position takes a future-answering closure, and
        // that is exactly what this is — so the refusal below is lifted there.
        // What stays refused is the UNDECLARED case, which is the hard half
        // (F22's adapted instances): a callee taking an async closure at one
        // call site and a synchronous one at another is compiled twice on the JS
        // backend, and this emitter monomorphizes on types, of which asyncness
        // is not one.
        let wants_a_future = std::mem::take(&mut self.expects_async_value);
        // F22: a closure the analyzer marked async UNDER THIS INSTANCE's bits
        // — `|url| { sleep(1); url.len() }` handed to `map` is one, and the
        // instance it lands in is the async one.
        let adapted_async = self
            .current_instance
            .as_ref()
            .is_some_and(|instance| instance.async_closures.contains(&closure_id));
        let wants_a_future = wants_a_future || adapted_async;
        // F18 slice 2: an inferred-async closure at a position declared
        // SYNCHRONOUS and answering `void` is a FLOATING body, and that is not
        // an adapted instance — it is the shape node has when it drops the
        // promise a void callback returned. `std::http`'s `upgrade_handler` is
        // declared `|NodeRequest, NodeSocket, Bytes| void` while A40's
        // `authorize` hook inside it may await, which is exactly the case:
        // nothing observes the result on either backend, so the body is
        // SPAWNED and the closure answers `()`. An unhandled failure reports
        // through the unobserved-task path, which is what node does with a
        // rejected floating promise.
        //
        // The `void` test is the whole of the licence. A closure answering a
        // VALUE cannot have its future dropped — its caller reads the value —
        // and that one really is F22's adapted instance.
        let is_async_closure = self.program.async_functions.contains(&closure_id);
        let floats = is_async_closure && !wants_a_future && self.closure_answers_void(&closure);
        if is_async_closure && !wants_a_future && !floats {
            let rendered = self.host_gap(
                "an `async` closure as a VALUE (a callee taking one at one call site and a \
                 synchronous closure at another is an adapted instance)"
                    .to_string(),
                span,
            );
            // F29: a refused closure's BODY is the other half of what a census
            // loses at a refusal — `createServer`'s handler is one of these.
            self.census_walk(&[closure.return_], depth);
            return rendered;
        }
        let mut parameters = Vec::new();
        for parameter_id in &closure.parameters {
            parameters.push(self.parameter_declaration(*parameter_id, span)?);
        }
        // A `move` closure takes its captures by value, so a captured CELL has
        // to be a handle of its own — otherwise the binding outside is moved
        // into the closure and every later read of it is a use-after-move.
        //
        // The scan runs BEFORE the body is walked, because the body's own reads
        // of a capture need the set: see [`Emitter::closure_captures`].
        // The scan is given the BODY, so this closure's own parameters are
        // declared-inside by seeding rather than by the walk — and so are the
        // names a TUPLE PARAMETER's destructures bind (F18): `|(value, factor)|
        // value * factor` has one parameter, an unnamed tuple, and `value` and
        // `factor` are declared by `parameter_destructures`, which the body walk
        // never visits. Without them in the seed both read as captures and the
        // prelude cloned them before either existed.
        let declared_inside_seed: HashSet<Id> = self.closure_parameter_bindings(&closure);
        let mut declared_inside: HashSet<Id> = declared_inside_seed.clone();
        let mut referenced = HashSet::new();
        let mut visited = HashSet::new();
        self.scan_closure(
            closure.return_,
            &mut declared_inside,
            &mut referenced,
            &mut visited,
        );
        let captured: HashSet<Id> = referenced
            .iter()
            .filter(|binding| !declared_inside.contains(binding))
            .copied()
            .collect();
        // Every capture gets a handle of its own, not only the boxed ones: a
        // `move` closure takes the whole binding whatever the body does with it
        // (Rust 2021 captures the PATH, and `&item` inside a `move` closure
        // still captures `item` by value), so the enclosing frame loses it —
        // `Owner::take` hands `item` to a cleanup closure and then RETURNS it.
        //
        // Two shapes are skipped because `.clone()` would change their type
        // rather than copy them: a parameter received by reference (a `&T` is
        // `Copy`, so the frame keeps it, and `(&T).clone()` derefs to `T`), and
        // a binding that holds a view, for the same reason.
        let mut captures: Vec<Id> = captured
            .iter()
            .copied()
            .filter(|binding| {
                // A module-level binding is read through its own `thread_local!`
                // cell and has no local name to shadow; a `Local` naming an enum
                // VARIANT is not a place at all.
                if self.module_bindings.contains(binding) {
                    return false;
                }
                if self.boxed.contains(binding) {
                    return true;
                }
                if self.binding_holds_a_view(*binding) {
                    return false;
                }
                if self.program.context_hidden_parameters.contains_key(binding) {
                    return true;
                }
                match self.program.parameters.get(binding) {
                    Some(parameter) => self.receiving_form(parameter) == Receiving::ByValue,
                    None => self.program.variables.contains_key(binding),
                }
            })
            .collect();
        captures.sort_by_key(|binding| binding.0);
        self.closure_captures.push(captured);
        let body = self.closure_body(&closure, depth);
        self.closure_captures.pop();
        let body = body?;
        let prelude: String = captures
            .iter()
            .map(|binding| {
                let name = self.binding_name(*binding);
                format!("let {name} = {name}.clone(); ")
            })
            .collect();
        // A future-answering closure answers `Boxed<R>` on every call, so its
        // body is an `async move` block behind `pin_future`. The captures are
        // cloned a SECOND time inside the closure: the block is `move` and takes
        // them, which would make the closure itself `FnOnce` and no
        // `Rc<dyn Fn>` at all.
        if wants_a_future {
            let inner =
                self.async_capture_prelude_declaring(closure.return_, &declared_inside_seed);
            return Ok(format!(
                "{{ {prelude}std::rc::Rc::new(move |{}| {{ {inner}vilan_rt::executor::pin_future(async move {{ {body} }}) }}) }}",
                parameters.join(", ")
            ));
        }
        // The floating body: the same `async move` block, spawned rather than
        // handed back, so the closure's own type is the synchronous one the
        // position declares. The origin is the enclosing function's, as it is
        // for a written spawn.
        if floats {
            let inner =
                self.async_capture_prelude_declaring(closure.return_, &declared_inside_seed);
            let origin = rust_string(self.current_origin.unwrap_or("a floating handler"));
            return Ok(format!(
                "{{ {prelude}std::rc::Rc::new(move |{}| {{ {inner}vilan_rt::executor::spawn(async move {{ {body} }}, {origin}); }}) }}",
                parameters.join(", ")
            ));
        }
        // F16: a closure VALUE is counted, because the emitter cannot see from
        // here whether the position it lands in stores it. `Rc::new` is the
        // shape the probe's R-1 finding forced.
        Ok(format!(
            "{{ {prelude}std::rc::Rc::new(move |{}| {{ {body} }}) }}",
            parameters.join(", ")
        ))
    }

    // ----------------------------------------------------------- async ----

    /// `async <body>` — the spawn (J6; `__task` is the contract).
    ///
    /// The JS helper takes the body as a CLOSURE and invokes it inside the
    /// constructor, which is how it is eager; natively the body is an `async`
    /// block and `spawn` polls it once before it answers, which is the same
    /// thing without an `Rc<dyn Fn>` in the middle. The origin is the enclosing
    /// function's name, as it is on the JS side, and the ambient nursery — when
    /// the context pass connected this spawn to one — is the third argument.
    fn async_spawn(
        &mut self,
        spawn_id: Id,
        spawned: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let Some(Expr::Closure(closure_id)) = self.program.entity_map.get(&spawned).cloned() else {
            return Err(unsupported(
                "an `async` spawn of something but a body",
                span,
            ));
        };
        let closure = self
            .program
            .closures
            .get(&closure_id)
            .cloned()
            .ok_or_else(|| unsupported("an `async` spawn whose body did not resolve", span))?;
        if !closure.parameters.is_empty() {
            return Err(unsupported(
                "an `async` spawn whose body takes a parameter",
                span,
            ));
        }
        let body = self.expression(closure.return_, depth)?;
        let origin = rust_string(self.current_origin.unwrap_or("top level"));
        let prelude = self.async_capture_prelude(closure.return_);
        let Some(&(source_entity, is_option)) = self.program.spawn_nursery_sources.get(&spawn_id)
        else {
            return Ok(format!(
                "{{ {prelude}vilan_rt::executor::spawn(async move {{ {body} }}, {origin}) }}"
            ));
        };
        // A safe holder carries `Option<Nursery>` and a covered one the nursery
        // itself; `spawn_in` takes the `Option`, so a bare source is wrapped
        // here exactly as `__nursery_of` unwraps the other way on the JS side.
        let source = self.expression(source_entity, depth)?;
        let nursery = if is_option {
            format!("({source}).clone()")
        } else {
            format!("Some(({source}).clone())")
        };
        // The handle is taken BEFORE the block, because the block is `async
        // move` and the body usually reads the very same threaded parameter —
        // `sleep` does, through `ambient_signal` — so taking it afterwards is a
        // read of a place the block has moved. A handle is counted; a copy of
        // one is the same nursery.
        let holder = format!("nursery_{}", spawn_id.0);
        Ok(format!(
            "{{ {prelude}let {holder} = {nursery}; vilan_rt::executor::spawn_in(async move {{ {body} }}, {origin}, {holder}) }}"
        ))
    }

    /// `await <operand>` — `.await` (J6).
    ///
    /// A `Task` read out of a BINDING is retained rather than moved: awaiting it
    /// twice is legal vilan (a task is a handle, and awaiting a settled one
    /// answers again), and `.await` on the binding itself would move out of a
    /// place the program may read later.
    /// One `.await`, plus the microtask hop JavaScript's `await` always costs
    /// (F20; `vilan_rt::executor::yield_now` says why).
    ///
    /// Rust continues in the same poll when the awaited future is already ready,
    /// so an `async fun` that suspends nowhere finished inside its own spawn
    /// natively and one turn later on the JS backend. Every `.await` this
    /// emitter writes goes through here, the written `await` and the one the
    /// async inference implies alike — both are `await` on the JS side.
    fn awaited(operand: &str) -> String {
        format!(
            "{{ let awaited = ({operand}).await; vilan_rt::executor::yield_now().await; awaited }}"
        )
    }

    fn await_of(&mut self, awaited: Id, depth: usize) -> Result<String, Error> {
        // `value_of` retains a handle read out of a binding (J6's rule for the
        // executor's handles), which is exactly what awaiting a task twice
        // needs: a task is a handle, and awaiting a settled one answers again.
        let operand = self.value_of(awaited, depth)?;
        // A call to an async callee is awaited by the CALL path already
        // ([`Emitter::call_awaits`] reads the two channels the JS emitter
        // reads), so `await tick()` on an `async fun tick()` was rendered
        // `((tick()).await).await` — and `()` is not a future.
        // `reactive-turns.vl` is the pin: it is the corpus's only program that
        // writes the prefix `await` over a call the inference had already
        // marked, and it did not reach rustc until F20 built `Hash`.
        if let Some(&Expr::Call(call_id)) = self.program.entity_map.get(&awaited)
            && self.call_awaits(awaited, call_id)
        {
            return Ok(operand);
        }
        Ok(Self::awaited(&operand))
    }

    /// The host bindings `vilan-rt`'s executor answers (J6).
    ///
    /// `std::task` and `std::time` reach the event loop through named runtime
    /// helpers (`__sleep`, `__timer`, the three nursery helpers) and
    /// `[extern(method, ..)]` methods on the handles those return, plus
    /// `Promise.all`/`Promise.race` on `Task<T>`. Each one below has a body in
    /// `vilan_rt::executor` written against the JS helper of the same name, so
    /// the mapping is a rename rather than a reimplementation. `Ok(None)` means
    /// "not one of ours", and the caller refuses by name — which is what every
    /// other host binding still gets.
    ///
    /// The four method symbols (`signal_of`, `cancel`, `is_cancelled`, `wait`)
    /// are unique in std: the only other `[extern(method)]` bindings are
    /// `encode`/`decode` on the text codecs and `exec`/`prepare` on the sqlite
    /// handle, so a name here cannot capture a stranger's method.
    ///
    /// Still NOT here, deliberately: `__with_finally_async`, which only
    /// `Debounce` reaches and which no program in the census does.
    fn runtime_host_binding(
        &mut self,
        name: &str,
        binding: Option<&ExternBinding<'src>>,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<Option<String>, Error> {
        let Some(binding) = binding else {
            return Ok(None);
        };
        let rendered = match binding {
            ExternBinding::Function {
                module: None,
                symbol: "__sleep",
            } => format!(
                "vilan_rt::executor::sleep({}, {})",
                self.value_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ExternBinding::Function {
                module: None,
                symbol: "__timer",
            } => format!(
                "vilan_rt::executor::timer({})",
                self.value_argument(argument_ids, 0, depth)?
            ),
            ExternBinding::Function {
                module: None,
                symbol: "__nursery_new",
            } => format!(
                "vilan_rt::executor::nursery_new({})",
                self.value_argument(argument_ids, 0, depth)?
            ),
            ExternBinding::Function {
                module: None,
                symbol: "__nursery_new_detached",
            } => "vilan_rt::executor::nursery_new_detached()".to_string(),
            // The join takes the body as a FUTURE where the JS helper takes a
            // closure it invokes: `await body()` there is `body.await` here, and
            // a `Pin<Box<dyn Future>>` is what lets the join hold it across the
            // drain without the emitted source ever naming `Pin`.
            ExternBinding::Function {
                module: None,
                symbol: "__nursery_run",
            } => format!(
                "vilan_rt::executor::nursery_run({}, {})",
                self.value_argument(argument_ids, 0, depth)?,
                self.pinned_body_argument(argument_ids, 1, depth, span)?
            ),
            // F20: `std::reactive`'s three glue bindings. They stand with the
            // executor's because the runtime already had bodies for two of them
            // (`guarded`, `with_finally`, written for S1a's `resource`) and the
            // third is one call into the microtask queue J6 built — and because
            // the reactive scheduler reaches all three on the way to any `set`,
            // which is what kept `board.vl` refused behind `Hash`.
            //
            // Each takes a `|| void` — an `Rc<dyn Fn() -> ()>` once emitted,
            // which is not itself `Fn()` (`Rc` implements no `Fn` trait), so the
            // handle is bound and CALLED inside a closure the runtime can take.
            ExternBinding::Function {
                module: None,
                symbol: "__guarded",
            } => format!(
                "{{ let body = {}; vilan_rt::guarded(move || body()).err() }}",
                self.value_argument(argument_ids, 0, depth)?
            ),
            ExternBinding::Function {
                module: None,
                symbol: "__with_finally",
            } => format!(
                "{{ let body = {}; let after = {}; \
                 vilan_rt::with_finally(move || body(), move || after()) }}",
                self.value_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ExternBinding::Function {
                module: None,
                symbol: "queueMicrotask",
            } => format!(
                "{{ let callback = {}; \
                 vilan_rt::executor::queue_microtask(move || callback()) }}",
                self.value_argument(argument_ids, 0, depth)?
            ),
            ExternBinding::Function {
                module: None,
                symbol: "Promise.all",
            } => format!(
                "vilan_rt::executor::settle_all({})",
                self.value_argument(argument_ids, 0, depth)?
            ),
            ExternBinding::Function {
                module: None,
                symbol: "Promise.race",
            } => format!(
                "vilan_rt::executor::race({})",
                self.value_argument(argument_ids, 0, depth)?
            ),
            // The methods on the handles. `[extern(method)]` defaults the host
            // name to the function's own, so the vilan name is consulted where
            // the attribute wrote none. A receiver is a PLACE, never a copy:
            // every one of these handles is counted, and a copy of a handle is
            // the same handle anyway.
            ExternBinding::Method { symbol } => match symbol.unwrap_or(name) {
                // `Nursery::signal_of` — `ambient_signal()` reaches this one for
                // every `sleep` inside a nursery's extent.
                "signal_of" => format!(
                    "({}).signal()",
                    self.place_argument(argument_ids, 0, depth)?
                ),
                "cancel" => format!(
                    "({}).cancel()",
                    self.place_argument(argument_ids, 0, depth)?
                ),
                "is_cancelled" => format!(
                    "({}).is_cancelled()",
                    self.place_argument(argument_ids, 0, depth)?
                ),
                // `TimerHandle::wait(self, signal)`.
                "wait" => format!(
                    "({}).wait({})",
                    self.place_argument(argument_ids, 0, depth)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        Ok(Some(rendered))
    }

    /// The host bindings `vilan-rt`'s HTTP server answers (F18 slice 1).
    ///
    /// `std::http`'s raw `node:http` layer is nineteen bindings over five
    /// `external struct`s, and `vilan_rt::http` was written against those
    /// declarations, so — as with the executor — the mapping is a rename rather
    /// than a reimplementation.
    ///
    /// **The dispatch is keyed on the RECEIVER's host type, not on the symbol.**
    /// The executor's four method symbols were unique in std and could be
    /// matched by name alone; this surface is not remotely unique — `write`,
    /// `end`, `on`, `close`, `destroy`, `url`, `method`, `listen` and `port` are
    /// all names an unrelated `[extern(method)]` somewhere could carry, and two
    /// of these bindings (`write_text`/`write_bytes`, `end`/`end_bytes`) SHARE a
    /// host symbol and differ only in their vilan name. So the key is the pair
    /// (the host type the `self` parameter is declared at, the vilan name), both
    /// of which are static facts about the declaration.
    ///
    /// `Ok(None)` means "not one of ours" and the caller refuses by name, which
    /// is what the two bindings answering a `JsonValue` still get: the request's
    /// `headers` and the socket's `remoteAddress` need `std::json`'s host type,
    /// which is Order 40's.
    fn http_host_binding(
        &mut self,
        target: Id,
        binding: Option<&ExternBinding<'src>>,
        argument_ids: &[Id],
        depth: usize,
    ) -> Result<Option<String>, Error> {
        let Some(binding) = binding else {
            return Ok(None);
        };
        let Some(external) = self.program.external_functions.get(&target) else {
            return Ok(None);
        };
        let name = external.name;
        // The module-level entry points first: they have no receiver.
        match binding {
            ExternBinding::Function {
                module: Some("node:http"),
                symbol: "createServer",
            } => {
                // The handler answers a FUTURE: `std::http` declares this
                // parameter `|NodeRequest, NodeResponse| void`, synchronously,
                // and hands it a closure whose body awaits — it reads the
                // request body and then the application's `async` handler. On
                // the JS backend that is free (node ignores the promise its
                // callback returns); natively the closure has to answer one, so
                // the expectation is set HERE, at the one binding that takes
                // one, rather than read off a declared type that does not say
                // so. Cleared before the `?`, exactly as a call argument and a
                // struct field set it.
                self.expects_async_value = true;
                let handler = self.value_argument(argument_ids, 0, depth);
                self.expects_async_value = false;
                return Ok(Some(format!("vilan_rt::http::create_server({})", handler?)));
            }
            // The two body reads CONSUME the request handle, and the callback
            // reads the same handle again afterwards — `std::http`'s own
            // `Server::start` awaits the body and then builds
            // `Request { node = node_request, .. }` out of it. A handle is
            // RETAINED per use, exactly as J6 retains a `Task` read out of a
            // binding: `clone_sites` marks nothing, because on the JS backend a
            // handle is a class instance that `__clone` passes through.
            ExternBinding::Function {
                module: Some("node:stream/consumers"),
                symbol: "buffer",
            } => {
                let request = self.place_argument(argument_ids, 0, depth)?;
                return Ok(Some(format!(
                    "vilan_rt::http::read_request_bytes(({request}).clone())"
                )));
            }
            // `new TextDecoder()` / `new TextEncoder()` — both stateless for
            // the one encoding vilan has.
            ExternBinding::New {
                module: None,
                symbol: "TextDecoder",
            } => return Ok(Some("vilan_rt::http::TextDecoder".to_string())),
            ExternBinding::New {
                module: None,
                symbol: "TextEncoder",
            } => return Ok(Some("vilan_rt::http::TextEncoder".to_string())),
            ExternBinding::Function {
                module: Some("node:stream/consumers"),
                symbol: "text",
            } => {
                let request = self.place_argument(argument_ids, 0, depth)?;
                return Ok(Some(format!(
                    "vilan_rt::http::read_request_text(({request}).clone())"
                )));
            }
            _ => {}
        }
        let Some(receiver) = self.host_receiver_type(target) else {
            return Ok(None);
        };
        let rendered = match (receiver, name) {
            // --- NodeServer ---
            ("NodeServer", "listen") => format!(
                "({}).listen({}, {})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?,
                self.value_argument(argument_ids, 2, depth)?
            ),
            ("NodeServer", "address") => {
                format!(
                    "({}).address()",
                    self.place_argument(argument_ids, 0, depth)?
                )
            }
            ("NodeServer", "on_upgrade") => format!(
                "({}).on_upgrade(&{}, {})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?,
                self.value_argument(argument_ids, 2, depth)?
            ),
            ("NodeServer", "close") => format!(
                "({}).close({})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            // --- NodeAddress ---
            ("NodeAddress", "port") => {
                format!("({}).port()", self.place_argument(argument_ids, 0, depth)?)
            }
            // --- NodeRequest ---
            ("NodeRequest", "url") => {
                format!("({}).url()", self.place_argument(argument_ids, 0, depth)?)
            }
            ("NodeRequest", "method") => {
                format!(
                    "({}).method()",
                    self.place_argument(argument_ids, 0, depth)?
                )
            }
            // F18 slice 2: the two bindings that ANSWER a `JsonValue`, which is
            // why they waited for `vilan_rt::json`. `Request::header` reads
            // named entries out of the first with `std::json`'s accessors, and
            // `Socket::remote_address` flattens the second's `undefined`.
            ("NodeRequest", "headers") => {
                format!(
                    "({}).headers()",
                    self.place_argument(argument_ids, 0, depth)?
                )
            }
            ("NodeSocket", "remote_address_raw") => format!(
                "({}).remote_address_raw()",
                self.place_argument(argument_ids, 0, depth)?
            ),
            // --- NodeResponse ---
            ("NodeResponse", "set_status_code") => format!(
                "({}).set_status_code({})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeResponse", "set_header_raw") => format!(
                "({}).set_header(&{}, &{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?,
                self.value_argument(argument_ids, 2, depth)?
            ),
            ("NodeResponse", "end") => format!(
                "({}).end(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeResponse", "end_bytes") => format!(
                "({}).end_bytes(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeResponse", "write") => format!(
                "({}).write(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeResponse", "on_event") => format!(
                "({}).on_event(&{}, {})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?,
                self.value_argument(argument_ids, 2, depth)?
            ),
            // --- NodeSocket ---
            ("NodeSocket", "write_text") => format!(
                "({}).write_text(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeSocket", "write_bytes") => format!(
                "({}).write_bytes(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeSocket", "on_bytes") => format!(
                "({}).on_bytes(&{}, {})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?,
                self.value_argument(argument_ids, 2, depth)?
            ),
            ("NodeSocket", "on_signal") => format!(
                "({}).on_signal(&{}, {})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?,
                self.value_argument(argument_ids, 2, depth)?
            ),
            ("NodeSocket", "set_timeout") => format!(
                "({}).set_timeout({})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeSocket", "destroy") => {
                format!(
                    "({}).destroy()",
                    self.place_argument(argument_ids, 0, depth)?
                )
            }
            // F18 slice 2: `std::bytes`'s READ accessors. The three that
            // MUTATE (`alloc`, `fill`, `copy_into`) are deliberately absent:
            // a `Uint8Array` is a mutable reference type there and `Bytes` is
            // an immutable refcounted buffer here, so admitting them wants a
            // decision about which `Bytes` is — its own item, and a wrong
            // answer would be a silent miscompile rather than a refusal.
            ("Bytes", "len") => {
                format!("({}).len()", self.place_argument(argument_ids, 0, depth)?)
            }
            ("Bytes", "get") => format!(
                "({}).at({})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("Bytes", "get_u32") => format!(
                "(({}).at({}) as u32)",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("Bytes", "slice") => format!(
                "({}).slice({}, {})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?,
                self.value_argument(argument_ids, 2, depth)?
            ),
            ("TextDecoder", "decode") => format!(
                "({}).decode(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("TextEncoder", "encode") => format!(
                "({}).encode(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("NodeSocket", "destroyed") => format!(
                "({}).destroyed()",
                self.place_argument(argument_ids, 0, depth)?
            ),
            _ => return Ok(None),
        };
        Ok(Some(rendered))
    }

    /// `expr!` — try-and-lift's assertion (`proposal/try-and-lift.md` §4), which
    /// `std::json`'s derived decoders are written in and which therefore stood
    /// between the JSON surface and a program that decodes anything.
    ///
    /// The JS emitter hoists the receiver, tests its tag and RETURNS THE
    /// RECEIVER ITSELF for the bad half — byte-identical at any success type,
    /// because a vilan enum there is `[tag, ..payload]` and `None` is `None`
    /// whatever the `Option` was over. Natively the two halves are two types, so
    /// the bad half is REBUILT (`return None`, `return Err(error)`) rather than
    /// passed through. The `match` evaluates its subject once, which is what the
    /// JS temp is for.
    ///
    /// Only the `Option`/`Result` dispatch is emitted. A user `Try` impl
    /// (`TryDispatch::Trait`) is refused by name: its `verdict`/`from_bad` pair
    /// is two more dispatches and no program on the native path writes one.
    fn try_assert(
        &mut self,
        id: Id,
        receiver: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        if !matches!(
            self.program.try_dispatch.get(&id),
            Some(TryDispatch::Std) | None
        ) {
            return Err(unsupported(
                "a `!` assertion through a user `Try` impl (the `Option`/`Result` form is emitted)",
                span,
            ));
        }
        let Some(Type::Enum(enum_id, _)) = self
            .type_of(receiver)
            .map(|type_id| self.concrete(type_id))
            .and_then(|type_id| self.resolve(type_id))
            .cloned()
        else {
            return Err(unsupported(
                "a `!` assertion on an unresolved receiver",
                span,
            ));
        };
        let name = self
            .program
            .enums
            .get(&enum_id)
            .map(|declaration| declaration.name);
        // The binder carries the expression's own id, so a `!` inside the
        // receiver of another `!` cannot shadow the outer one's payload.
        let good = format!("try_good_{}", id.0);
        let bad = format!("try_bad_{}", id.0);
        let subject = self.consumed_value_of(receiver, depth)?;
        match name {
            Some("Option") => Ok(format!(
                "match {subject} {{ Some({good}) => {good}, None => return None }}"
            )),
            Some("Result") => Ok(format!(
                "match {subject} {{ Ok({good}) => {good}, Err({bad}) => return Err({bad}) }}"
            )),
            _ => Err(unsupported(
                "a `!` assertion on something that is neither an `Option` nor a `Result`",
                span,
            )),
        }
    }

    /// `std::db`'s ten host seams (F18 slice 2; Order 39's R1).
    ///
    /// Keyed on the pair (host symbol, vilan name) for
    /// [`Emitter::json_host_binding`]'s reason, and here the second half does
    /// real work twice over: the three column readers SHARE the host helper
    /// `__db_column` and differ only in the vilan type they read the column
    /// AT, and `exec`/`prepare` are bare `[extern(method)]`s whose host name
    /// defaults to their own.
    ///
    /// Every arm names `vilan_rt_sqlite`, whose module header says what each
    /// one is a twin of and where the two genuinely differ (a statement is
    /// prepared at its first USE, through the connection's own cache).
    fn db_host_binding(
        &mut self,
        name: &str,
        binding: Option<&ExternBinding<'src>>,
        argument_ids: &[Id],
        depth: usize,
    ) -> Result<Option<String>, Error> {
        let receiver = |emitter: &mut Self| emitter.place_argument(argument_ids, 0, depth);
        let rendered = match binding {
            // `new DatabaseSync(path)`.
            Some(ExternBinding::New {
                module: Some("node:sqlite"),
                symbol: "DatabaseSync",
            }) => format!(
                "vilan_rt_sqlite::Database::open({})",
                self.value_argument(argument_ids, 0, depth)?
            ),
            Some(ExternBinding::Method { symbol }) => match symbol.unwrap_or(name) {
                "exec" => format!(
                    "({}).exec({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                "prepare" => format!(
                    "({}).prepare({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                _ => return Ok(None),
            },
            Some(ExternBinding::Function {
                module: None,
                symbol,
            }) => match (*symbol, name) {
                ("__db_close", _) => format!("({}).close()", receiver(self)?),
                ("__db_run", _) => format!(
                    "({}).run({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_all", _) => format!(
                    "({}).all({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_get", _) => format!(
                    "({}).first({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_is_null", _) => format!(
                    "({}).is_null({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_exec_guarded", _) => format!(
                    "({}).exec_guarded({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_run_guarded", _) => format!(
                    "({}).run_guarded({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                // The three readers, told apart by the vilan name because the
                // host helper cannot tell them apart at all.
                ("__db_column", "column_text") => format!(
                    "({}).text({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_column", "column_integer") => format!(
                    "({}).integer({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_column", "column_big_integer") => format!(
                    "({}).big_integer({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                ("__db_column", "column_real") => format!(
                    "({}).real({})",
                    receiver(self)?,
                    self.value_argument(argument_ids, 1, depth)?
                ),
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        self.reaches_sqlite = true;
        Ok(Some(rendered))
    }

    /// The plain `node:fs/promises` set and `std::crypto`'s digest (F18
    /// slice 2).
    ///
    /// Keyed on the pair (module + host symbol, vilan name) for
    /// [`Emitter::json_host_binding`]'s reason, and here it is load-bearing in
    /// a second way: `readFile` and `writeFile` each back TWO declarations that
    /// differ only in whether the payload is text or bytes
    /// (`read_file_encoded`/`read_bytes`, `write_file`/`write_bytes`), exactly
    /// as `std::http`'s socket writes do.
    ///
    /// The `FsOptions` forms are deliberately absent — `vilan_rt::fs` says why
    /// — and so are `sha384`/`sha512`, which are the SHA-512 block function
    /// this slice did not need. Both stay refused BY NAME.
    fn host_module_binding(
        &mut self,
        name: &str,
        binding: Option<&ExternBinding<'src>>,
        argument_ids: &[Id],
        depth: usize,
    ) -> Result<Option<String>, Error> {
        let Some(ExternBinding::Function { module, symbol }) = binding else {
            return Ok(None);
        };
        let one = |emitter: &mut Self, path: &str| {
            Ok(Some(format!(
                "vilan_rt::{path}({})",
                emitter.value_argument(argument_ids, 0, depth)?
            )))
        };
        let two = |emitter: &mut Self, path: &str| {
            let first = emitter.value_argument(argument_ids, 0, depth)?;
            let second = emitter.value_argument(argument_ids, 1, depth)?;
            Ok(Some(format!("vilan_rt::{path}({first}, {second})")))
        };
        match (*module, *symbol, name) {
            (None, "__sha256", _) => one(self, "crypto::sha256_bytes"),
            (Some("node:fs/promises"), "readFile", "read_bytes") => one(self, "fs::read_bytes"),
            (Some("node:fs/promises"), "readFile", "read_file_encoded") => {
                two(self, "fs::read_text")
            }
            (Some("node:fs/promises"), "writeFile", "write_file") => two(self, "fs::write_text"),
            (Some("node:fs/promises"), "writeFile", "write_bytes") => two(self, "fs::write_bytes"),
            (Some("node:fs/promises"), "appendFile", _) => two(self, "fs::append"),
            (Some("node:fs/promises"), "copyFile", _) => two(self, "fs::copy"),
            (Some("node:fs/promises"), "rename", _) => two(self, "fs::rename"),
            (Some("node:fs/promises"), "unlink", _) => one(self, "fs::remove"),
            (Some("node:fs/promises"), "readdir", "read_dir") => one(self, "fs::read_dir"),
            (Some("node:fs/promises"), "mkdir", "create_dir") => one(self, "fs::create_dir"),
            (Some("node:fs/promises"), "rmdir", _) => one(self, "fs::remove_dir"),
            _ => Ok(None),
        }
    }

    /// The Rust scalar one `[extern("Number")]` binding of `std::number` labels
    /// its argument with — read off the DECLARED RETURN TYPE, not off the
    /// binding's name.
    ///
    /// The name is a hint and it lies: `label_i64(value: f64): i53` is the
    /// vilan width `i53`, whose native width is `i64`, and a read of the name
    /// answered "there is no `i64`" and refused `numeric-types.vl`. The
    /// declaration is where the vilan type is said, which is the same reason
    /// [`Emitter::math_host_binding`] casts from it.
    ///
    /// `str` is excluded — `Number` never answers one — and so is anything that
    /// is not a scalar primitive, which is how a `BigInt` receiver falls
    /// through to the arm above it.
    fn scalar_label_target(
        &mut self,
        target: Id,
        span: Span,
    ) -> Result<Option<&'static str>, Error> {
        let Some(external) = self.program.external_functions.get(&target) else {
            return Ok(None);
        };
        let returns = external.return_type_id;
        let Some(Type::Struct(struct_id, _)) = self.resolve(returns) else {
            return Ok(None);
        };
        let Some(declaration) = self.program.structs.get(struct_id) else {
            return Ok(None);
        };
        let _ = span;
        Ok(scalar_type(declaration.name).filter(|rendered| *rendered != "vilan_rt::Str"))
    }

    /// Whether a call's receiver is a `BigInt` — the one width
    /// [`Emitter::scalar_host_binding`]'s `Number` family cannot cast.
    fn receiver_is_bigint(&self, argument_ids: &[Id]) -> bool {
        let Some(&receiver) = argument_ids.first() else {
            return false;
        };
        let Some(Type::Struct(struct_id, _)) = self
            .type_of(receiver)
            .and_then(|type_id| self.resolve(type_id))
        else {
            return false;
        };
        self.program
            .structs
            .get(struct_id)
            .is_some_and(|declaration| declaration.external && declaration.name == "BigInt")
    }

    /// `std::number`'s `Math.*` family (F18 slice 2).
    ///
    /// Almost all of it is the `f64` method of the same name, so the table is
    /// the exceptions plus a list. The three that are NOT a rename:
    ///
    /// - `Math.round` rounds a half UP where Rust's rounds away from zero
    ///   (`Math.round(-2.5)` is `-2`), and `Math.sign` passes both zeros and
    ///   `NaN` through where `signum` answers `±1` for them. Both go to
    ///   `vilan_rt`, which states the divergence at each.
    /// - `Math.pow` computes in DOUBLE even where the declaration labels the
    ///   result an integer, so the integer widths go through `f64` and cast
    ///   back rather than through `i32::pow`, which panics on an overflow the
    ///   other backend simply widens through.
    ///
    /// The result is cast to the DECLARED return type, which is the whole of
    /// what makes one table serve `f64`, `f32` and the six integer widths: JS
    /// has one number type and the declaration is where the vilan one is said.
    fn math_host_binding(
        &mut self,
        target: Id,
        binding: Option<&ExternBinding<'src>>,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<Option<String>, Error> {
        let Some(ExternBinding::Function {
            module: None,
            symbol,
        }) = binding
        else {
            return Ok(None);
        };
        let Some(method) = symbol.strip_prefix("Math.") else {
            return Ok(None);
        };
        let Some(external) = self.program.external_functions.get(&target) else {
            return Ok(None);
        };
        let returns = self.rust_type(external.return_type_id, span)?;
        if scalar_type(&returns).is_none() && !is_integer_type(&returns) && returns != "f64" {
            return Ok(None);
        }
        let receiver = self.value_argument(argument_ids, 0, depth)?;
        // Every arm computes in `f64` and casts back, because that is what the
        // other backend does: a JS number IS an `f64`, and the vilan width is a
        // label on the result. The cast is a no-op where the declaration says
        // `f64`, which is most of this family.
        let subject = format!("(({receiver}) as f64)");
        let unary = |body: String| Ok(Some(format!("(({body}) as {returns})")));
        match method {
            "abs" => unary(format!("{subject}.abs()")),
            "sqrt" => unary(format!("{subject}.sqrt()")),
            "cbrt" => unary(format!("{subject}.cbrt()")),
            "floor" => unary(format!("{subject}.floor()")),
            "ceil" => unary(format!("{subject}.ceil()")),
            "trunc" => unary(format!("{subject}.trunc()")),
            "exp" => unary(format!("{subject}.exp()")),
            "log" => unary(format!("{subject}.ln()")),
            "log10" => unary(format!("{subject}.log10()")),
            "log2" => unary(format!("{subject}.log2()")),
            "sin" => unary(format!("{subject}.sin()")),
            "cos" => unary(format!("{subject}.cos()")),
            "tan" => unary(format!("{subject}.tan()")),
            "asin" => unary(format!("{subject}.asin()")),
            "acos" => unary(format!("{subject}.acos()")),
            "atan" => unary(format!("{subject}.atan()")),
            "round" => unary(format!("vilan_rt::js_math_round({subject})")),
            "sign" => unary(format!("vilan_rt::js_math_sign({subject})")),
            "pow" | "min" | "max" | "atan2" | "hypot" => {
                let other = self.value_argument(argument_ids, 1, depth)?;
                let other = format!("(({other}) as f64)");
                let body = match method {
                    "pow" => format!("{subject}.powf({other})"),
                    "min" => format!("{subject}.min({other})"),
                    "max" => format!("{subject}.max({other})"),
                    "atan2" => format!("{subject}.atan2({other})"),
                    _ => format!("{subject}.hypot({other})"),
                };
                unary(body)
            }
            _ => Ok(None),
        }
    }

    /// The host bindings `std::number` and `std::string` declare over
    /// JavaScript's global coercions (F18 slice 2).
    ///
    /// Three shapes, and none of them is a conversion natively:
    ///
    /// - `label_i32(value: f64): i32` and its eleven siblings are `Number(v)`
    ///   over an `f64` that `fold_signed`/`fold_unsigned` has ALREADY folded
    ///   into the target's range. `Number` is the identity there; the vilan
    ///   type is the whole of what the call says. So the emission is the cast,
    ///   and it is exact because the value is already in range and integral.
    /// - `as_f64(self): f64` on each integer width, on `f32` and on `BigInt` is
    ///   `Number(x)`, which is again the identity: a JS number IS an `f64`.
    /// - `code_at` is `charCodeAt`, the one binding here that is a real
    ///   function — `vilan_rt::str_code_at` indexes the same UTF-16 code units
    ///   `str_len` counts.
    ///
    /// Keyed on the pair (host symbol, vilan name), for
    /// [`Emitter::json_host_binding`]'s reason: `Number` is `std::json`'s
    /// symbol too, and there the receiver is a `JsonValue` and the emission is
    /// a coercion rather than a cast.
    fn scalar_host_binding(
        &mut self,
        target: Id,
        name: &str,
        binding: Option<&ExternBinding<'src>>,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<Option<String>, Error> {
        let rendered = match binding {
            // `BigInt::as_f64` is `Number(big)`, and a `BigInt` is a NEWTYPE
            // natively (F32), so it converts through its own method where
            // every other width is an `as` cast.
            Some(ExternBinding::Function {
                module: None,
                symbol: "Number",
            }) if name == "as_f64" && self.receiver_is_bigint(argument_ids) => format!(
                "({}).to_f64()",
                self.value_argument(argument_ids, 0, depth)?
            ),
            Some(ExternBinding::Function {
                module: None,
                symbol: "Number",
            }) if let Some(scalar) = self.scalar_label_target(target, span)? => format!(
                "(({}) as {scalar})",
                self.value_argument(argument_ids, 0, depth)?
            ),
            Some(ExternBinding::Method {
                symbol: Some("charCodeAt"),
            }) if name == "code_at" => format!(
                "vilan_rt::str_code_at(&{}, {})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            _ => return Ok(None),
        };
        Ok(Some(rendered))
    }

    /// The host bindings `vilan-rt`'s JSON value answers (F18 slice 2).
    ///
    /// **The key is the pair (host symbol, vilan name)**, for the reason
    /// [`Emitter::http_host_binding`] keys on its own pair and more sharply:
    /// three of these symbols are `String`, `Boolean` and `Number`, which
    /// `std::number` also binds — `label_i32(value: f64)` is `[extern("Number")]`
    /// and is not a JSON coercion at all. A symbol alone would claim it and
    /// emit a method no `f64` has. Both halves of the key are static facts
    /// about the declaration, so nothing at a call site can confuse them.
    ///
    /// `Ok(None)` means "not one of ours" and the caller refuses by name.
    fn json_host_binding(
        &mut self,
        name: &str,
        binding: Option<&ExternBinding<'src>>,
        argument_ids: &[Id],
        depth: usize,
    ) -> Result<Option<String>, Error> {
        let Some(ExternBinding::Function {
            module: None,
            symbol,
        }) = binding
        else {
            return Ok(None);
        };
        let rendered = match (*symbol, name) {
            // TRUSTING, as `std::json` documents it: malformed text is a host
            // exception, which natively is the abort every host throw takes.
            // The guarded parse is the `TryParseJson` INTRINSIC, not this.
            ("JSON.parse", "parse_json_value") => {
                format!(
                    "vilan_rt::json::parse(&{})",
                    self.value_argument(argument_ids, 0, depth)?
                )
            }
            // `JSON.stringify(value)` under whatever name declared it:
            // `impl i32 with Json`'s `to_json`, `std::debug`'s `debug`, and a
            // DERIVED struct's `to_json` calling the scalar one per field.
            // `vilan_rt::Json` is that rendering and already existed — it is
            // what `canonical_hash` keys on (F20) — so this arm is a rename
            // rather than a second stringifier, and keying it on the SYMBOL
            // rather than on one vilan name is the point: every declaration of
            // it means the same function.
            ("JSON.stringify", _) if argument_ids.len() == 1 => format!(
                "vilan_rt::str_new(&vilan_rt::Json::json(&{}))",
                self.place_argument(argument_ids, 0, depth)?
            ),
            ("Object.hasOwn", "has_json_field") => format!(
                "({}).has_field(&{})",
                self.place_argument(argument_ids, 0, depth)?,
                self.value_argument(argument_ids, 1, depth)?
            ),
            ("String", "coerce_str") => format!(
                "({}).coerce_str()",
                self.place_argument(argument_ids, 0, depth)?
            ),
            ("Boolean", "coerce_bool") => format!(
                "({}).coerce_bool()",
                self.place_argument(argument_ids, 0, depth)?
            ),
            // The twelve `Number` coercions differ only in the vilan type they
            // label the result with, and `json.vl` reaches every one of them
            // only past a `kind() == Number` check — so the cast is over a
            // number that is already a number.
            ("Number", _) if let Some(scalar) = json_coercion_target(name) => format!(
                "(({}).coerce_number() as {scalar})",
                self.place_argument(argument_ids, 0, depth)?
            ),
            _ => return Ok(None),
        };
        Ok(Some(rendered))
    }

    /// The host type an `[extern(method|get|set)]`'s RECEIVER is declared at —
    /// the discriminator [`Emitter::http_host_binding`] keys on.
    ///
    /// Read off the declaration's own `self` parameter rather than off the
    /// receiver EXPRESSION at the call site, so it is a static fact about the
    /// binding and cannot be confused by a generic call or by an inference that
    /// has not landed.
    fn host_receiver_type(&self, target: Id) -> Option<&'src str> {
        let external = self.program.external_functions.get(&target)?;
        let first = external.parameters.first()?;
        let parameter = self.program.parameters.get(first)?;
        match self.resolve(parameter.type_id)? {
            Type::Struct(id, _) => self
                .program
                .structs
                .get(id)
                .filter(|declaration| declaration.external)
                .map(|declaration| declaration.name),
            _ => None,
        }
    }

    /// An `async || T` argument as a pinned future — the shape the executor's
    /// join takes.
    ///
    /// The argument is a closure LITERAL at every call site std writes (the
    /// helper exists to be handed one), and what the future needs is its BODY,
    /// not an `Rc<dyn Fn>` wrapping it. Anything else is refused rather than
    /// wrapped, because a body this cannot see through is a body the join cannot
    /// await.
    fn pinned_body_argument(
        &mut self,
        argument_ids: &[Id],
        index: usize,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let Some(&argument) = argument_ids.get(index) else {
            return Err(unsupported("a runtime helper called with no body", span));
        };
        let Some(Expr::Closure(closure_id)) = self.program.entity_map.get(&argument).cloned()
        else {
            return Err(unsupported(
                "a runtime helper whose body is not written as a closure at the call site",
                span,
            ));
        };
        let closure = self
            .program
            .closures
            .get(&closure_id)
            .cloned()
            .ok_or_else(|| unsupported("a runtime helper whose body did not resolve", span))?;
        if !closure.parameters.is_empty() {
            return Err(unsupported(
                "a runtime helper whose body takes a parameter",
                span,
            ));
        }
        let body = self.expression(closure.return_, depth)?;
        let prelude = self.async_capture_prelude(closure.return_);
        Ok(format!(
            "{{ {prelude}vilan_rt::executor::pin_future(async move {{ {body} }}) }}"
        ))
    }

    /// The `let x = x.clone();` run that goes BEFORE an `async move` block the
    /// emitter writes (J6).
    ///
    /// The block is `move`, so it takes every binding it mentions — and the
    /// enclosing frame almost always reads the same ones afterwards (`sleep`
    /// reads the threaded nursery the spawn then hands to `spawn_in`, and
    /// `nursery`'s join reads the nursery it also passes as an argument). The
    /// clones are bound in a fresh scope OUTSIDE the block and shadow the
    /// originals, so the block moves copies and the frame keeps its own. Every
    /// value a vilan program can capture is `Clone` by construction: rule 1
    /// already says a capture is a copy, and a handle's copy is the same
    /// handle.
    fn async_capture_prelude(&mut self, body: Id) -> String {
        self.async_capture_prelude_declaring(body, &HashSet::new())
    }

    /// [`Emitter::async_capture_prelude`] with names the CALLER knows are
    /// declared inside and the body walk cannot see — a tuple parameter's
    /// destructured binders (F18).
    fn async_capture_prelude_declaring(&mut self, body: Id, seed: &HashSet<Id>) -> String {
        let mut declared_inside = seed.clone();
        let mut referenced = HashSet::new();
        let mut visited = HashSet::new();
        self.scan_closure(body, &mut declared_inside, &mut referenced, &mut visited);
        let mut captures: Vec<Id> = referenced
            .into_iter()
            .filter(|binding| {
                !declared_inside.contains(binding)
                    // A `Local` naming an enum VARIANT is not a place, and a
                    // module-level binding is read through its own cell.
                    && !self.module_bindings.contains(binding)
                    && (self.program.variables.contains_key(binding)
                        || self.program.parameters.contains_key(binding)
                        || self
                            .program
                            .context_hidden_parameters
                            .contains_key(binding))
            })
            .collect();
        captures.sort_by_key(|binding| binding.0);
        captures
            .iter()
            .map(|binding| {
                let name = self.binding_name(*binding);
                format!("let {name} = {name}.clone(); ")
            })
            .collect()
    }

    /// A closure literal applied at its own call site, as a block binding the
    /// parameters (J6).
    ///
    /// `Some` when the shape is one this can take, `None` when the caller should
    /// fall back to building the closure and calling it — which is the honest
    /// answer for a body that `ret`urns, since a `return` inside the block would
    /// leave the ENCLOSING function rather than the closure.
    fn applied_closure(
        &mut self,
        closure_id: Id,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<Option<String>, Error> {
        let closure = self
            .program
            .closures
            .get(&closure_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved closure", span))?;
        if closure.parameters.len() != argument_ids.len()
            || !closure.parameter_destructures.is_empty()
            || self.body_returns(closure.return_)
        {
            return Ok(None);
        }
        let mut bindings = String::new();
        for (parameter_id, argument) in closure.parameters.iter().zip(argument_ids) {
            // The parameter binding CONSUMES the argument, exactly as a real
            // closure call would — see [`Emitter::copy_a_consumed_place_read`].
            let value = self.consumed_value_of(*argument, depth)?;
            let _ = write!(
                bindings,
                "let {} = {value}; ",
                self.binding_name(*parameter_id)
            );
        }
        let body = self.expression(closure.return_, depth)?;
        Ok(Some(format!("{{ {bindings}{body} }}")))
    }

    /// Whether an expression tree contains a `ret` — the one thing that makes
    /// inlining a closure body into its caller's frame observable.
    fn body_returns(&self, expr_id: Id) -> bool {
        let Some(_guard) = vilan_core::util::RecursionGuard::enter() else {
            return true;
        };
        if matches!(
            self.program.entity_map.get(&expr_id),
            Some(Expr::FunctionReturn(_))
        ) {
            return true;
        }
        self.children_of(expr_id)
            .into_iter()
            .any(|child| self.body_returns(child))
    }

    /// Whether a call site awaits — the union of the two channels the JS emitter
    /// reads (J2, async-polymorphism.md A.1): a call to an async callee, and a
    /// call the async inference recorded as awaiting because its subject is not
    /// a plain binding (an async field, an async-returning call, an adapted
    /// parameter).
    fn call_awaits(&self, call_expr_id: Id, call_id: Id) -> bool {
        if self.program.awaited_calls.contains(&call_expr_id)
            || self.program.awaited_calls.contains(&call_id)
        {
            return true;
        }
        // F22: this instance's own await set — a call that is awaited BECAUSE
        // the callee was emitted as an async adapted instance, which the
        // declaration cannot say.
        if self
            .current_instance
            .as_ref()
            .is_some_and(|instance| instance.awaited_calls.contains(&call_expr_id))
        {
            return true;
        }
        let Some(call) = self.program.function_calls.get(&call_id) else {
            return false;
        };
        let Some(Expr::Local(target)) = self.program.entity_map.get(&call.subject_id) else {
            return false;
        };
        self.program.async_functions.contains(target) || self.program.async_values.contains(target)
    }

    // ----------------------------------------------------------- the call --

    /// One call, plus the `.await` an async callee owes (J6).
    fn call(
        &mut self,
        call_expr_id: Id,
        call_id: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        // A closure LITERAL applied right here — which is what
        // `Context::run(value, body)` lowers to (`body(value)`, the literal as
        // the call's own subject). It becomes a BLOCK binding the parameters
        // rather than an `Rc<dyn Fn>` built and called in one breath: one fewer
        // allocation, and — the reason it is not just a tidy-up — an `await` in
        // the body then sits inside the enclosing `async fn` instead of inside a
        // non-async closure, which is the only way `nursery`'s own body
        // compiles. It takes no `.await` of its own: the awaits are already in
        // the body where the closure wrote them.
        if let Some(call) = self.program.function_calls.get(&call_id).cloned()
            && let Some(Expr::Closure(closure_id)) =
                self.program.entity_map.get(&call.subject_id).cloned()
            && let Some(rendered) =
                self.applied_closure(closure_id, &call.argument_ids, depth, span)?
        {
            return Ok(rendered);
        }
        let rendered = self.call_expression(call_expr_id, call_id, depth, span)?;
        if self.call_awaits(call_expr_id, call_id) {
            return Ok(Self::awaited(&rendered));
        }
        Ok(rendered)
    }

    fn call_expression(
        &mut self,
        call_expr_id: Id,
        call_id: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let function_call = self
            .program
            .function_calls
            .get(&call_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved call", span))?;
        let Some(Expr::Local(target)) = self.program.entity_map.get(&function_call.subject_id)
        else {
            // A value call — `(h.f)()`. The subject is a counted closure.
            let subject = self.expression(function_call.subject_id, depth)?;
            let arguments = self.value_arguments(&function_call.argument_ids, depth)?;
            return Ok(format!("({subject})({})", arguments.join(", ")));
        };
        let target = *target;

        // A variant constructor builds the value directly.
        if let Some(Expr::EnumVariant(enum_id, index)) = self.program.entity_map.get(&target) {
            let (enum_id, index) = (*enum_id, *index);
            // A VIEW inside an enum payload (`Option<&mut T>`, the shape behind
            // a view-returning `Arena::get`). The type system has no reference
            // form — a payload's type is its POINTEE's, and viewness is
            // recorded beside it — so the emitted variant takes a value and
            // `Some(&mut x)` does not typecheck. Named rather than emitted:
            // carrying the viewness into the payload type is its own slice.
            if function_call.argument_ids.iter().any(|argument| {
                matches!(
                    self.program.entity_map.get(argument),
                    Some(Expr::Reference(_, _))
                )
            }) {
                let arguments = self.variant_arguments(
                    call_expr_id,
                    enum_id,
                    index,
                    &function_call.argument_ids,
                );
                let path = self.variant_path(enum_id, index, &arguments, span)?;
                let mut rendered = Vec::new();
                for argument in &function_call.argument_ids {
                    rendered.push(self.expression(*argument, depth)?);
                }
                return Ok(format!("{path}({})", rendered.join(", ")));
            }
            let arguments =
                self.variant_arguments(call_expr_id, enum_id, index, &function_call.argument_ids);
            let path = self.variant_path(enum_id, index, &arguments, span)?;
            if function_call.argument_ids.is_empty() {
                return Ok(path);
            }
            let arguments = self.value_arguments(&function_call.argument_ids, depth)?;
            return Ok(format!("{path}({})", arguments.join(", ")));
        }

        // A124 R3: `o.member(..)` where `o` is a trait OBJECT — a slot call.
        if let Some(member) = self.program.dyn_method_calls.get(&call_id).copied() {
            let receiver_type = function_call
                .argument_ids
                .first()
                .and_then(|receiver| self.type_of(*receiver))
                .ok_or_else(|| {
                    unsupported("a call through an object whose receiver has no type", span)
                })?;
            return self.object_call(
                receiver_type,
                member,
                &function_call.argument_ids,
                depth,
                span,
            );
        }

        // `T::member(..)` inside a monomorphized body, and `value.method()` on
        // a value whose type IS a bounded `T`: the analyzer could not pin the
        // callee, so it recorded how to re-resolve it at each instantiation.
        // Both are keyed by the CONSTRAINT, one against the accessor and one
        // against the call (the JS emitter reads the same two places).
        for keyed_on in [function_call.subject_id, call_id] {
            if let Some(GenericDispatch::OnConstraint(constraint_id, member)) =
                self.program.generic_dispatch.get(&keyed_on).copied()
                && let Some(&bound) = self.current_substitution.get(&constraint_id)
            {
                let concrete = self.concrete(bound);
                let own_values = self
                    .program
                    .own_generic_call_bindings
                    .get(&call_id)
                    .cloned()
                    .unwrap_or_default();
                let preferred = self
                    .program
                    .bound_dispatch_traits
                    .get(&call_id)
                    .or_else(|| {
                        self.program
                            .bound_dispatch_traits
                            .get(&function_call.subject_id)
                    })
                    .cloned();
                if let Some(dispatch) =
                    self.resolve_dispatch(concrete, member, &own_values, preferred, span)?
                {
                    return self.emit_dispatch(dispatch, &function_call.argument_ids, depth, span);
                }
                return Err(unsupported(
                    &format!(
                        "a call to `{member}` through a generic parameter that resolves to no \
                         member of the concrete type"
                    ),
                    span,
                ));
            }
        }
        // A trait method re-dispatched to the receiver's concrete type: an
        // inherited default called on a concrete value, or a `self`-call inside
        // a default body (no type recorded — it dispatches on the type the
        // default is being specialized for).
        if let Some(GenericDispatch::OnType(recorded, member)) =
            self.program.generic_dispatch.get(&call_id).copied()
            && let Some(type_id) = recorded.or(self.current_self_type)
        {
            let concrete = self.concrete(type_id);
            let preferred = self.program.bound_dispatch_traits.get(&call_id).cloned();
            if let Some(dispatch) = self.resolve_dispatch(concrete, member, &[], preferred, span)? {
                return self.emit_dispatch(dispatch, &function_call.argument_ids, depth, span);
            }
            return Err(unsupported(
                &format!("a trait call to `{member}` that resolves to no member of the receiver"),
                span,
            ));
        }

        if Some(target) == self.program.print_fn_id {
            self.refuse_unprintable(&function_call.argument_ids, span)?;
            let value = self.place_argument(&function_call.argument_ids, 0, depth)?;
            return Ok(format!("vilan_rt::print(&({value}))"));
        }
        if Some(target) == self.program.panic_fn_id {
            let value = self.place_argument(&function_call.argument_ids, 0, depth)?;
            return Ok(format!("vilan_rt::panic_with(&({value}))"));
        }
        if Some(target) == self.program.list_new_fn_id {
            return Ok("Vec::new()".to_string());
        }
        if Some(target) == self.program.list_push_fn_id {
            // A MUTATING receiver, so a boxed binding reaches its cell — see
            // [`Emitter::mutable_place`]. The ITEM is rendered first when the
            // receiver lives in a cell, for the reason
            // [`Emitter::emit_intrinsic`] states: the borrow outlives the call
            // and a read of the same binding inside the item would meet it.
            let item = self.value_argument(&function_call.argument_ids, 1, depth)?;
            let receiver = match function_call.argument_ids.first() {
                Some(argument) => self.mutable_place(*argument, depth)?,
                None => "()".to_string(),
            };
            if function_call
                .argument_ids
                .first()
                .is_some_and(|receiver| self.place_lives_in_a_cell(*receiver))
            {
                return Ok(format!(
                    "{{ let __borrowed1 = {item}; {receiver}.push(__borrowed1) }}"
                ));
            }
            return Ok(format!("{receiver}.push({item})"));
        }
        if let Some(intrinsic) = self.program.intrinsics.get(&target).copied() {
            return self.emit_intrinsic(intrinsic, &function_call.argument_ids, depth, span);
        }
        if let Some(external) = self.program.external_functions.get(&target) {
            let name = external.name;
            let binding = external.extern_binding.clone();
            // J6: the concurrency helpers have native bodies in
            // `vilan_rt::executor`. Everything else is still a host binding
            // this backend has nothing to put behind it.
            if let Some(rendered) = self.runtime_host_binding(
                name,
                binding.as_ref(),
                &function_call.argument_ids,
                depth,
                span,
            )? {
                return Ok(rendered);
            }
            // F18: and the HTTP surface has native bodies in `vilan_rt::http`.
            if let Some(rendered) = self.http_host_binding(
                target,
                binding.as_ref(),
                &function_call.argument_ids,
                depth,
            )? {
                return Ok(rendered);
            }
            // F18 slice 2: and `std::json`'s host seams in `vilan_rt::json`,
            // and `std::number`/`std::string`'s in the runtime's scalar half.
            if let Some(rendered) =
                self.json_host_binding(name, binding.as_ref(), &function_call.argument_ids, depth)?
            {
                return Ok(rendered);
            }
            if let Some(rendered) = self.scalar_host_binding(
                target,
                name,
                binding.as_ref(),
                &function_call.argument_ids,
                depth,
                span,
            )? {
                return Ok(rendered);
            }
            if let Some(rendered) = self.math_host_binding(
                target,
                binding.as_ref(),
                &function_call.argument_ids,
                depth,
                span,
            )? {
                return Ok(rendered);
            }
            if let Some(rendered) = self.host_module_binding(
                name,
                binding.as_ref(),
                &function_call.argument_ids,
                depth,
            )? {
                return Ok(rendered);
            }
            if let Some(rendered) =
                self.db_host_binding(name, binding.as_ref(), &function_call.argument_ids, depth)?
            {
                return Ok(rendered);
            }
            let what = format!(
                "the host binding `{name}`{}",
                match binding {
                    Some(ExternBinding::Function { symbol, .. }) => format!(" (`{symbol}`)"),
                    _ => String::new(),
                }
            );
            let rendered = self.host_gap(what, span);
            // F29: the census goes on into the arguments — this is the call
            // that hid nine bindings inside `createServer`'s handler.
            self.census_walk(&function_call.argument_ids, depth);
            return rendered;
        }

        // A named BINDING that holds a closure — `g()` where `g` is a
        // parameter or a local. The subject is a place, not a definition, so it
        // is called as a value; an `Rc<dyn Fn>` derefs to the call.
        if self.program.parameters.contains_key(&target)
            || self.program.variables.contains_key(&target)
        {
            let arguments = self.value_arguments(&function_call.argument_ids, depth)?;
            let callee = self.read_module_binding_or_local(target, span)?;
            return Ok(format!("({callee})({})", arguments.join(", ")));
        }

        // An ordinary call, monomorphized against whatever binds it.
        let substitution =
            self.call_substitution(call_id, target, &function_call.generic_argument_ids);
        // F21: an `Option` whose payload is a VIEW is a `&`/`&mut` natively,
        // and the only position that carries it today is a `match` subject,
        // where the leg binds the reference and reads through it. Anywhere else
        // it meets code written against the payload's POINTEE — `arena.vl`
        // hands one to `unwrap_or`, a generic monomorphised at `Option<i32>` —
        // so it is named rather than emitted. The general answer is a
        // monomorphisation keyed on viewness, which is its own slice.
        let carries_a_payload_view = self
            .program
            .functions
            .get(&target)
            .is_some_and(|function| self.payload_view_of(function).is_some());
        if carries_a_payload_view && !std::mem::take(&mut self.matching_the_subject) {
            return self.host_gap(
                "an `Option` with a VIEW payload read anywhere but as a `match` subject (the \
                 payload is a reference natively, and a generic over it monomorphises at the \
                 pointee)"
                    .to_string(),
                span,
            );
        }
        // F22: the instance this call must reach — the async bits this
        // instance recorded for it, which are empty for every call in a
        // program that writes no async closure.
        let bits = self.callee_bits(call_expr_id);
        let name = self
            .ensure_function_with_bits(target, &substitution, &bits)?
            .name;
        let mut prelude = String::new();
        let arguments = self.call_arguments_adapting(
            target,
            &function_call.argument_ids,
            depth,
            &mut prelude,
            &bits,
        )?;
        Ok(Self::with_argument_prelude(
            prelude,
            format!("{name}({})", arguments.join(", ")),
        ))
    }

    // ------------------------------------------------- arguments by position --

    /// One call's arguments, rendered per POSITION against the callee's own
    /// conventions: a by-value parameter takes a VALUE (rule 1's copy where the
    /// analyzer recorded one), a `&`/`&mut` parameter takes a PLACE.
    ///
    /// The distinction is not cosmetic, and getting it wrong was a live
    /// miscompile: every argument used to be rendered as a value, so a loaned
    /// parameter handed on to a `&mut` position became `&mut (xs).clone()` —
    /// `xs.push(1)` inside `fun bump(xs: &mut List<i32>)` pushed into a
    /// temporary and `side-effect-let.vl` printed `0 0` where the JS backend
    /// printed `1 2`. A borrow is by definition not a copy, so a reference
    /// position never takes one.
    /// With the `let`s a cell-resident `&mut` argument owes written into
    /// `prelude` (F30).
    ///
    /// A `&mut` argument naming a boxed or module-level binding BORROWS its
    /// cell, and the borrow lives to the end of the statement — so a read of
    /// the same binding in a LATER argument meets it and panics, where the JS
    /// backend (whose `&mut` is a plain reference) prints an answer. The
    /// by-value arguments are hoisted ahead of the borrow, which is the order
    /// JS has for free. Reference arguments are left where they are: hoisting
    /// one would name a borrow rather than a value, and two loans of ONE
    /// binding where either is mutable is what rule 4 refuses anyway.
    fn call_arguments(
        &mut self,
        target: Id,
        argument_ids: &[Id],
        depth: usize,
        prelude: &mut String,
    ) -> Result<Vec<String>, Error> {
        self.call_arguments_adapting(target, argument_ids, depth, prelude, &[])
    }

    /// [`Self::call_arguments`], told which of the callee's parameters this
    /// call hands an ASYNC closure (F22).
    fn call_arguments_adapting(
        &mut self,
        target: Id,
        argument_ids: &[Id],
        depth: usize,
        prelude: &mut String,
        callee_bits: &[Id],
    ) -> Result<Vec<String>, Error> {
        let declared: Vec<vilan_core::analyzer::Parameter<'src>> = self
            .program
            .functions
            .get(&target)
            .map(|function| function.parameters.clone())
            .unwrap_or_default()
            .iter()
            .filter_map(|parameter_id| self.program.parameters.get(parameter_id).cloned())
            .collect();
        let conventions: Vec<Receiving> = declared
            .iter()
            .map(|parameter| self.receiving_form(parameter))
            .collect();
        // Whether any argument will take a borrow of a cell that the arguments
        // after it must not be evaluated under.
        let borrows_a_cell = argument_ids.iter().enumerate().any(|(index, argument)| {
            matches!(conventions.get(index), Some(Receiving::RefMut))
                && self.place_lives_in_a_cell(*argument)
        });
        let mut rendered = Vec::new();
        for (index, argument) in argument_ids.iter().enumerate() {
            // F20: an argument standing in a `lazy` parameter is a cell, not a
            // value — see [`Emitter::lazy_argument`].
            if let Some(cell) = self.lazy_argument(*argument, depth) {
                rendered.push(cell?);
                continue;
            }
            let wants_a_place = !matches!(conventions.get(index), None | Some(Receiving::ByValue));
            // A `&mut` parameter (a `&mut self` receiver among them) takes a
            // place the callee WRITES, so a binding that lives in a cell has to
            // reach the cell — see [`Emitter::mutable_place`]. A `&` parameter
            // deliberately does NOT: a shared loan cannot write, so the value
            // read is indistinguishable from the place and costs no borrow that
            // could collide with another read in the same statement.
            let wants_a_mutable_place = matches!(conventions.get(index), Some(Receiving::RefMut));
            let expecting = declared
                .get(index)
                .map(|parameter| self.concrete(parameter.type_id));
            // A parameter declared `async |T| U` takes a future-answering
            // closure, so a sync literal at the call site is wrapped.
            // F22: an argument standing in a parameter this INSTANCE adapts is
            // a future-answering closure too, which the declaration does not
            // say (`fun run(f: || i32)` is declared sync and reached with an
            // async closure at one of its two call sites).
            self.expects_async_value = declared.get(index).is_some_and(|parameter| {
                self.program.async_values.contains(&parameter.id)
                    || callee_bits.contains(&parameter.id)
            });
            let mut text = if wants_a_place {
                // The declared type is threaded even for a PLACE, because a
                // numeric LITERAL at a `&`/`&mut` position still has to be
                // written at the width the signature names — `as_f32(self)`
                // takes `&f64` and `number-math.vl` hands it `0f`, whose own
                // record is `f32` (see [`Emitter::number_literal`]).
                let saved = std::mem::replace(&mut self.expected_type, expecting);
                let place = if wants_a_mutable_place {
                    self.mutable_place(*argument, depth)
                } else {
                    self.expression(*argument, depth)
                };
                self.expected_type = saved;
                self.expects_async_value = false;
                place?
            } else {
                // A by-value parameter CONSUMES its argument, so a plain read
                // of a place copies — see [`Emitter::copy_a_consumed_place_read`].
                let value = self.value_of_expecting(*argument, expecting, depth);
                self.expects_async_value = false;
                self.copy_a_consumed_place_read(*argument, value?)
            };
            // H9: a `mut` parameter of aggregate type is copied at BODY ENTRY
            // on the JS backend (`parameter_entry_clones`), because there the
            // callee and the caller share one array and the copy has to happen
            // somewhere. Natively the argument is MOVED into the callee, so the
            // one place the copy can happen is here — `grow(list)` followed by
            // `list.len()` was a borrow-after-move on a program the JS backend
            // runs. The same copy, at the only seam Rust offers for it.
            if declared.get(index).is_some_and(|parameter| {
                self.program.parameter_entry_clones.contains(&parameter.id)
            }) {
                text = format!("({text}).clone()");
            }
            // The receiver of a loaned `self` arrives as a place in the IR and
            // as a reference natively, so the `&`/`&mut` the source never wrote
            // is synthesized here — unless the source DID write one.
            let already_a_reference = self
                .program
                .entity_map
                .get(argument)
                .is_some_and(|expr| matches!(expr, Expr::Reference(_, _)));
            let text = match conventions.get(index) {
                Some(Receiving::Ref) if !already_a_reference => format!("&{text}"),
                Some(Receiving::RefMut) if !already_a_reference => format!("&mut {text}"),
                _ => text,
            };
            if borrows_a_cell
                && matches!(conventions.get(index), None | Some(Receiving::ByValue))
                && !already_a_reference
            {
                let name = format!("__borrowed{index}");
                let _ = write!(prelude, "let {name} = {text}; ");
                rendered.push(name);
                continue;
            }
            rendered.push(text);
        }
        Ok(rendered)
    }

    /// A rendered call, wrapped in the block its argument prelude needs (F30).
    fn with_argument_prelude(prelude: String, rendered: String) -> String {
        if prelude.is_empty() {
            return rendered;
        }
        format!("{{ {prelude}{rendered} }}")
    }

    /// Every argument as a VALUE — a closure call and a variant constructor,
    /// both of which consume what they are handed.
    ///
    /// Neither has a `clone_sites` decision behind it (F20): rule 1's marking is
    /// made against a NAMED callee's parameter modes, and a closure's parameter
    /// has none recorded — so a read of a binding handed to one was a move, and
    /// `Context::run(fresh, body)` lowers to exactly that (`body(fresh)`, with
    /// `fresh` read again by the two statements after it). A consumed argument
    /// that reads a place copies, which is what rule 1 says a value read into a
    /// call or an aggregate does anyway.
    fn value_arguments(&mut self, argument_ids: &[Id], depth: usize) -> Result<Vec<String>, Error> {
        let mut rendered = Vec::new();
        for argument in argument_ids {
            rendered.push(self.consumed_value_of(*argument, depth)?);
        }
        Ok(rendered)
    }

    /// [`Self::value_of`], plus rule 1's copy for a plain read of a place that
    /// the position CONSUMES. See [`Self::value_arguments`].
    fn consumed_value_of(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        let rendered = self.value_of(id, depth)?;
        Ok(self.copy_a_consumed_place_read(id, rendered))
    }

    /// Rule 1's copy at a position that CONSUMES its value, for the reads
    /// `clone_sites` deliberately elides (F20).
    ///
    /// The analyzer's last-use elision is correct for the JS backend and not
    /// transferable: `turn(body)` in `std::reactive` writes `drain(fresh)` and
    /// then `fresh.settled.write() = true`, and the elision is sound there
    /// because a copy of a `Turn` shares the very `Shared` cell the next line
    /// reads — so nothing can observe whether the copy happened. Natively the
    /// elided copy is a MOVE and the next line is a borrow after it. The copy is
    /// what rule 1 says the read means, so taking it always is the conservative
    /// direction; the elision is an optimisation this backend cannot take
    /// without a liveness pass of its own (a candidate item, C15's neighbour).
    ///
    /// Two shapes are skipped: a read already copied, and a binding that holds a
    /// VIEW — `(&mut T).clone()` derefs rather than copies.
    fn copy_a_consumed_place_read(&mut self, id: Id, rendered: String) -> String {
        if rendered.ends_with(".clone()") {
            return rendered;
        }
        let reads_a_place = match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) | Some(Expr::Parameter(binding)) => {
                let binding = *binding;
                (self.program.variables.contains_key(&binding)
                    || self.program.parameters.contains_key(&binding))
                    && !self.binding_holds_a_view(binding)
            }
            // An INDEX read is a place read too — `xs[i]` names storage, and a
            // consumed read of it is rule 1's copy. On the JS backend the
            // element is read out of the array and nothing moves, so
            // `clone_sites` marks nothing; natively `xs[i]` handed to a callee
            // is a move out of a `Vec`, which rustc refuses (`reconcile-index.vl`
            // is the pin, and it was behind the `for`-over-an-`Iterator` wall
            // until this slice). This is the widening native-b-39's find asked
            // for, taken at the position the find named: the CALL-ARGUMENT
            // path, where the position consumes by definition.
            Some(Expr::Index(_, _)) => true,
            _ => false,
        };
        if reads_a_place {
            // F31: a read that is the binding's LAST use donates its storage
            // instead of copying it — nothing can observe the difference,
            // which is exactly what rule 2's elision says. A read inside a
            // closure the emitter is walking is never one: the closure owns its
            // captures and handing one on by value would make it `FnOnce`,
            // which no closure-typed position natively takes.
            if self.last_uses.contains(&id) && !self.reads_a_captured_binding(id) {
                self.copies_elided += 1;
                return rendered;
            }
            self.copies_taken += 1;
            return format!("({rendered}).clone()");
        }
        rendered
    }

    fn value_argument(
        &mut self,
        argument_ids: &[Id],
        index: usize,
        depth: usize,
    ) -> Result<String, Error> {
        match argument_ids.get(index) {
            Some(argument) => self.value_of(*argument, depth),
            None => Ok("()".to_string()),
        }
    }

    /// One argument as a PLACE: the receiver of an intrinsic (`xs.push(..)`,
    /// `list_pop(&mut xs)`) and the argument of `print`/`panic`, which take a
    /// reference. Copying either would be wrong in the first case and wasteful
    /// in the second.
    fn place_argument(
        &mut self,
        argument_ids: &[Id],
        index: usize,
        depth: usize,
    ) -> Result<String, Error> {
        match argument_ids.get(index) {
            Some(argument) => self.expression(*argument, depth),
            None => Ok("()".to_string()),
        }
    }

    // ------------------------------------------------- generic dispatch --

    /// The trait-scoped / inherent / inherited-default resolution of `member`
    /// on a concrete receiver — [`vilan_core::impl_select`]'s selection, then
    /// the bindings that selection implies. `Ok(None)` means the type provides
    /// no such member, which the caller reports with the name it was looking
    /// for.
    ///
    /// The file scope is `None` on purpose: B318's admission is a
    /// FRONT-END rule, and a program that reaches emission has already been
    /// held to it — re-asking it here with no `current_admitting_file` to
    /// supply would refuse a member the analyzer admitted.
    /// A124 R3: the Rust trait an object type `dyn Trait<args>` lowers to,
    /// written once per concrete argument list.
    ///
    /// ```rust,ignore
    /// trait ObjectSource_12_0: vilan_rt::Js + vilan_rt::Json {
    ///     fn get(&self) -> i32;
    ///     fn on_change(&self, observer: std::rc::Rc<dyn Fn(i32)>) -> Subscription_40;
    /// }
    /// ```
    ///
    /// The slots are the members a call reaches THROUGH an object of this
    /// trait (`dyn_dispatched_members`, the set the JS table is built from),
    /// so a read-only `dyn Source<i32>` carries `get` alone. `Js` and `Json`
    /// are supertraits so the object prints and serializes as the JS pair
    /// does — `[ value, {} ]` — through the value it erased.
    ///
    /// Three member shapes are refused BY NAME rather than lowered: a `&mut
    /// self` slot (the pointer is counted, so a write through one copy would
    /// reach every copy — the JS backend copies the pair instead, and this
    /// backend has no copy-on-write for an unsized value yet), an async
    /// member (a slot answering a future is the executor's `Boxed`, not built
    /// for objects), and a view-returning one.
    fn ensure_object_trait(
        &mut self,
        trait_id: Id,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<ObjectTrait, Error> {
        let arguments: Vec<TypeId> = arguments
            .iter()
            .map(|argument| self.concrete(*argument))
            .collect();
        let key: Vec<String> = arguments
            .iter()
            .map(|argument| self.type_key(*argument))
            .collect();
        if let Some(object) = self.object_traits.get(&(trait_id, key.clone())) {
            return Ok(object.clone());
        }
        let trait_ = self
            .program
            .traits
            .get(&trait_id)
            .cloned()
            .ok_or_else(|| unsupported("an object over an unresolved trait", span))?;
        let sequence = self
            .object_traits
            .keys()
            .filter(|(existing, _)| *existing == trait_id)
            .count();
        let name = format!("Object{}_{}_{sequence}", sanitize(trait_.name), trait_id.0);
        let slot = self.next_type_slot;
        self.next_type_slot += 1;
        // Recorded BEFORE the slots render: a slot may name this very object
        // type (`fun next(self): Option<dyn Src>`), and the recursion has to
        // find the name rather than mint a second trait.
        self.object_traits.insert(
            (trait_id, key.clone()),
            ObjectTrait {
                name: name.clone(),
                slots: Vec::new(),
            },
        );
        let mut members: Vec<&'src str> = self
            .program
            .dyn_dispatched_members
            .iter()
            .filter(|(object_trait, _)| *object_trait == trait_id)
            .map(|(_, member)| *member)
            .collect();
        members.sort_unstable();
        members.dedup();
        let entries = self.nominal_entries(&trait_.generic_parameter_constraint_ids, &arguments);
        let saved = self.enter_substitution(entries);
        let mut slots = Vec::new();
        let mut failure = None;
        for member in members {
            match self.object_slot(trait_id, member, span) {
                Ok(Some(slot)) => slots.push(slot),
                Ok(None) => {}
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        self.current_substitution = saved;
        if let Some(error) = failure {
            self.object_traits.remove(&(trait_id, key));
            return Err(error);
        }
        let mut out = String::new();
        let _ = writeln!(out, "trait {name}: vilan_rt::Js + vilan_rt::Json {{");
        for slot in &slots {
            let _ = writeln!(out, "    {};", slot.signature);
        }
        let _ = writeln!(out, "}}");
        self.types.insert(slot, out);
        let object = ObjectTrait { name, slots };
        self.object_traits.insert((trait_id, key), object.clone());
        Ok(object)
    }

    /// One slot of an object trait, rendered under the object's substitution
    /// (already installed), or `None` for a member no table can hold — a
    /// generic one or one naming `Self`, which the analyzer refuses at every
    /// call, so a bound's over-approximation never makes it a slot.
    fn object_slot(
        &mut self,
        trait_id: Id,
        member: &str,
        span: Span,
    ) -> Result<Option<ObjectSlot>, Error> {
        let Some((declaration, declaring_trait, chain)) =
            object_member_declaration(self.program, trait_id, member)
        else {
            return Ok(None);
        };
        let Some(function) = self.program.functions.get(&declaration).cloned() else {
            return Ok(None);
        };
        if !function.generic_parameter_constraint_ids.is_empty() {
            return Ok(None);
        }
        let names_self = function.return_type_id.is_some_and(|type_id| {
            matches!(
                self.program.type_id_to_type_map.get(&type_id),
                Some(Type::Trait(mentioned, _)) if *mentioned == declaring_trait
            )
        });
        if names_self {
            return Ok(None);
        }
        let Some((receiver, rest)) = function.parameters.split_first() else {
            return Ok(None);
        };
        let Some(receiver) = self.program.parameters.get(receiver).cloned() else {
            return Ok(None);
        };
        if receiver.name != "self" {
            return Ok(None);
        }
        let trait_name = self
            .program
            .traits
            .get(&declaring_trait)
            .map(|trait_| trait_.name)
            .unwrap_or("the trait");
        if self.receiving_form(&receiver) == Receiving::RefMut {
            return Err(unsupported(
                &format!(
                    "`{trait_name}::{member}` through a `dyn` object (a `&mut self` slot: the \
                     object's pointer is counted, and a write through one copy would reach \
                     every copy)"
                ),
                span,
            ));
        }
        if self.program.async_functions.contains(&declaration) {
            return Err(unsupported(
                &format!("the async member `{trait_name}::{member}` through a `dyn` object"),
                span,
            ));
        }
        if function.returns_view || function.returns_mut_view {
            return Err(unsupported(
                &format!(
                    "the view-returning member `{trait_name}::{member}` through a `dyn` object"
                ),
                span,
            ));
        }
        // A member a SUPERTRAIT declares is written in that trait's terms, and
        // the chain's `with` clauses say what its parameters are here.
        let saved = self.enter_substitution(chain);
        let rendered = self.object_slot_signature(member, rest, &function, span);
        self.current_substitution = saved;
        let (signature, forwarded) = rendered?;
        Ok(Some(ObjectSlot {
            member: member.to_string(),
            declaration,
            signature,
            forwarded,
        }))
    }

    fn object_slot_signature(
        &mut self,
        member: &str,
        parameters: &[Id],
        function: &vilan_core::analyzer::Function<'src>,
        span: Span,
    ) -> Result<(String, Vec<String>), Error> {
        let mut rendered = vec!["&self".to_string()];
        let mut forwarded = Vec::new();
        for parameter in parameters {
            // A trait method with no body takes no binding patterns, so the
            // by-value binder's `mut` (H9's local copy) is dropped here; the
            // impl forwards the value to a function that declares its own.
            let declaration = self.parameter_declaration(*parameter, span)?;
            let declaration = declaration
                .strip_prefix("mut ")
                .unwrap_or(&declaration)
                .to_string();
            forwarded.push(self.binding_name(*parameter));
            rendered.push(declaration);
        }
        let returned = match self.return_type_of(function) {
            Some(type_id) => {
                self.expects_async = self.program.async_returning.contains(&function.id);
                let returned = self.rust_type(type_id, span);
                self.expects_async = false;
                returned?
            }
            None => "()".to_string(),
        };
        Ok((
            format!(
                "fn {}({}) -> {returned}",
                sanitize(member),
                rendered.join(", ")
            ),
            forwarded,
        ))
    }

    /// A124 R3: `impl ObjectX for Concrete`, written the first time a value of
    /// `Concrete` is erased into the object — each slot calls the member B57's
    /// ranking selects for that type under the object's trait (trait-objects.md
    /// §9.1: the table carries the WINNERS, so an impl's override of a default
    /// is what the slot runs).
    fn ensure_object_impl(
        &mut self,
        subject: TypeId,
        trait_id: Id,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<String, Error> {
        let object = self.ensure_object_trait(trait_id, arguments, span)?;
        let subject = self.concrete(subject);
        let rendered_subject = self.rust_type(subject, span)?;
        if !self
            .object_impls
            .insert((object.name.clone(), rendered_subject.clone()))
        {
            return Ok(object.name);
        }
        let arguments: Vec<TypeId> = arguments
            .iter()
            .map(|argument| self.concrete(*argument))
            .collect();
        let mut out = String::new();
        let _ = writeln!(out, "impl {} for {rendered_subject} {{", object.name);
        for slot in &object.slots {
            let preferred = Some((trait_id, arguments.clone()));
            let dispatch = self.resolve_dispatch(subject, &slot.member, &[], preferred, span)?;
            let Some(NativeDispatch::Call(function_name)) = dispatch else {
                self.object_impls
                    .remove(&(object.name.clone(), rendered_subject.clone()));
                return Err(unsupported(
                    &format!(
                        "`{}` through a `dyn` object on `{rendered_subject}` (its member is not an \
                         emitted function — an intrinsic or a host binding)",
                        slot.member
                    ),
                    span,
                ));
            };
            let target = self.instance_target(&function_name);
            let target_parameters: Vec<Id> = target
                .and_then(|target| self.program.functions.get(&target))
                .map(|function| function.parameters.clone())
                .unwrap_or_default();
            let receiver = match target_parameters
                .first()
                .and_then(|parameter| self.program.parameters.get(parameter))
                .map(|parameter| self.receiving_form(parameter))
            {
                Some(Receiving::Ref) => "self".to_string(),
                Some(Receiving::ByValue) => "self.clone()".to_string(),
                _ => {
                    self.object_impls
                        .remove(&(object.name.clone(), rendered_subject.clone()));
                    return Err(unsupported(
                        &format!(
                            "`{}` through a `dyn` object on `{rendered_subject}` (the member's \
                             receiver is neither a loan nor a copy)",
                            slot.member
                        ),
                        span,
                    ));
                }
            };
            if target_parameters.len() != slot.forwarded.len() + 1 {
                self.object_impls
                    .remove(&(object.name.clone(), rendered_subject.clone()));
                return Err(unsupported(
                    &format!(
                        "`{}` through a `dyn` object on `{rendered_subject}` (the implementation \
                         takes {} parameters where the declaration takes {})",
                        slot.member,
                        target_parameters.len(),
                        slot.forwarded.len() + 1
                    ),
                    span,
                ));
            }
            let mut forwarded = vec![receiver];
            forwarded.extend(slot.forwarded.iter().cloned());
            let _ = writeln!(out, "    {} {{", slot.signature);
            let _ = writeln!(out, "        {function_name}({})", forwarded.join(", "));
            let _ = writeln!(out, "    }}");
        }
        let _ = writeln!(out, "}}");
        let slot = self.next_type_slot;
        self.next_type_slot += 1;
        self.types.insert(slot, out);
        Ok(object.name)
    }

    /// A call through an object's table: `ObjectX::member(o.object(), ..)`.
    ///
    /// The receiver is rendered as the PLACE it is — the object is read, never
    /// copied, to make a call — and the rest of the arguments against the
    /// DECLARATION's conventions, which are the slot's.
    fn object_call(
        &mut self,
        receiver_type: TypeId,
        member: &str,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let receiver_type = self.concrete(receiver_type);
        let Some(Type::Dyn(trait_id, arguments)) = self.resolve(receiver_type).cloned() else {
            return Err(unsupported(
                "a call through an object whose receiver is not one",
                span,
            ));
        };
        let object = self.ensure_object_trait(trait_id, &arguments, span)?;
        let Some(slot) = object
            .slots
            .iter()
            .find(|slot| slot.member == member)
            .cloned()
        else {
            return Err(unsupported(
                &format!(
                    "`{member}` through a `dyn` object (no call to it through an object was \
                     recorded, so its table has no slot for it)"
                ),
                span,
            ));
        };
        let Some((receiver_id, _)) = argument_ids.split_first() else {
            return Err(unsupported(
                "a call through an object with no receiver",
                span,
            ));
        };
        let receiver = self.expression(*receiver_id, depth)?;
        let mut prelude = String::new();
        let mut rendered =
            self.call_arguments(slot.declaration, argument_ids, depth, &mut prelude)?;
        if !rendered.is_empty() {
            rendered.remove(0);
        }
        let mut parts = vec![format!("({receiver}).object()")];
        parts.extend(rendered);
        Ok(Self::with_argument_prelude(
            prelude,
            format!(
                "{}::{}({})",
                object.name,
                sanitize(member),
                parts.join(", ")
            ),
        ))
    }

    fn resolve_dispatch(
        &mut self,
        type_id: TypeId,
        member: &str,
        own_generic_values: &[TypeId],
        preferred_trait: Option<(Id, Vec<TypeId>)>,
        span: Span,
    ) -> Result<Option<NativeDispatch>, Error> {
        let type_id = self.concrete(type_id);
        // A124 R3, the blanket as a dispatch rule: a generic body whose
        // parameter bound to an OBJECT reaches the object's own trait members
        // through its table. A member some BLANKET provides is not a slot and
        // falls through to the selection below, where the blanket applies with
        // the object as its subject.
        if let Some(Type::Dyn(trait_id, _)) = self.resolve(type_id).cloned()
            && object_member_declaration(self.program, trait_id, member).is_some()
        {
            return Ok(Some(NativeDispatch::Object(type_id, member.to_string())));
        }
        if let Some((trait_id, written)) = preferred_trait {
            let arguments: Vec<TypeId> = written
                .iter()
                .map(|argument| self.concrete(*argument))
                .collect();
            let wanted = impl_select::WantedTrait {
                trait_id,
                arguments: &arguments,
            };
            if let Some(selected) =
                impl_select::select_member(self.program, None, type_id, member, Some(wanted))
            {
                return self
                    .dispatch_to_member(selected, type_id, own_generic_values)
                    .map(Some);
            }
            if let Some(default_id) = mono::trait_default_member(self.program, trait_id, member) {
                let name = self.default_instance(default_id, type_id, span)?;
                return Ok(Some(NativeDispatch::Call(name)));
            }
            // The preference did not materialize (it should not, for a call the
            // analyzer resolved) — fall through to the general lookup.
        }
        if self.selectable_receiver(type_id)
            && let Some(selected) =
                impl_select::select_member(self.program, None, type_id, member, None)
        {
            return self
                .dispatch_to_member(selected, type_id, own_generic_values)
                .map(Some);
        }
        let Some(default_id) = mono::resolve_inherited_default(self.program, None, type_id, member)
        else {
            return Ok(None);
        };
        let name = self.default_instance(default_id, type_id, span)?;
        Ok(Some(NativeDispatch::Call(name)))
    }

    /// One HOST gap: a refusal in an ordinary build, a recorded
    /// `unimplemented!()` under `VILAN_NATIVE_HOST_CENSUS=1`.
    ///
    /// The census answers the question a whole slice is sized from — "what host
    /// surface does this program still need" — in one emit instead of one emit
    /// per gap. A build never takes this path: the mode is off unless the
    /// variable is set, and the source a census produces is never compiled.
    fn host_gap(&mut self, what: String, span: Span) -> Result<String, Error> {
        if !self.census {
            return Err(unsupported(&what, span));
        }
        self.host_gaps.insert(what);
        Ok("unimplemented!()".to_string())
    }

    /// F29: what a REFUSAL owes the census — a walk of the subtrees the
    /// refusal stopped at.
    ///
    /// A gap answers `unimplemented!()` and the walk returns, so everything
    /// UNDER the refused construct went unrecorded. `createServer(handler)` is
    /// the shape that made it visible: one gap was reported and the handler's
    /// whole body — nine more bindings — was never walked, so the census that
    /// sized F18 was low by them. The program is refused whatever this walk
    /// finds; what the census owes is the whole list, which is the question it
    /// exists to answer ("what host surface does this program still need").
    ///
    /// The rendered text is thrown away, and so are the walk's ERRORS: a
    /// subtree that cannot be emitted is not a second refusal to report, it is
    /// a subtree whose gaps are recorded as far as the walk reached. Off unless
    /// the census is on, so a build pays nothing and takes exactly the first
    /// refusal it took before.
    fn census_walk(&mut self, argument_ids: &[Id], depth: usize) {
        if !self.census {
            return;
        }
        for argument in argument_ids {
            let _ = self.value_of(*argument, depth);
        }
    }

    /// Whether a receiver is a shape an impl can be written for — the nominal
    /// ones plus the two structural ones (spec §5.7). `select_member` admits
    /// impl SUBJECTS of every shape past this; the guard is about the RECEIVER,
    /// so a blanket subject cannot "apply" to something still abstract.
    fn selectable_receiver(&self, type_id: TypeId) -> bool {
        matches!(
            self.program.type_id_to_type_map.get(&type_id),
            Some(
                Type::Struct(..)
                    | Type::Enum(..)
                    | Type::Tuple(..)
                    | Type::Array(..)
                    | Type::Dyn(..)
            )
        )
    }

    /// Lowers a selected member to its dispatch, binding the impl's own
    /// generics from the concrete receiver (so a method whose body uses the
    /// impl's parameter grounds it) plus the method's own from the call's
    /// ordered values.
    fn dispatch_to_member(
        &mut self,
        selected: impl_select::SelectedMember,
        type_id: TypeId,
        own_generic_values: &[TypeId],
    ) -> Result<NativeDispatch, Error> {
        let member_id = selected.member_id;
        if let Some(intrinsic) = self.program.intrinsics.get(&member_id).copied() {
            return Ok(NativeDispatch::Intrinsic(intrinsic));
        }
        // An EXTERNAL member reached generically — `element.to_json()` inside
        // `impl List<type T: Json> with Json`, where `T` binds to `str` and the
        // selected member is `impl str with Json`'s `[extern("JSON.stringify")]`
        // one. The id is carried rather than refused here: the host tables are
        // keyed on the declaration and take the call's ARGUMENTS, neither of
        // which this function has, so the decision belongs at
        // [`Emitter::emit_dispatch`]. Refusing here refused every host binding
        // a blanket impl can reach, which is what stood between `std::json`'s
        // scalar impls and any generic that walks them.
        if self.program.external_functions.contains_key(&member_id) {
            return Ok(NativeDispatch::Host(member_id));
        }
        let mut substitution = HashMap::default();
        impl_select::bind_subject(
            self.program,
            selected.impl_subject,
            type_id,
            &mut substitution,
        );
        if !own_generic_values.is_empty()
            && let Some(function) = self.program.functions.get(&member_id)
        {
            for (constraint_id, value) in function
                .generic_parameter_constraint_ids
                .iter()
                .zip(own_generic_values.iter())
            {
                substitution.insert(*constraint_id, *value);
            }
        }
        let name = self.ensure_function(member_id, &substitution)?.name;
        Ok(NativeDispatch::Call(name))
    }

    /// Emits a resolved dispatch's call, with the receiver as the first
    /// argument — the shape the analyzer already put a method call in.
    fn emit_dispatch(
        &mut self,
        dispatch: NativeDispatch,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        match dispatch {
            NativeDispatch::Intrinsic(intrinsic) => {
                self.emit_intrinsic(intrinsic, argument_ids, depth, span)
            }
            NativeDispatch::Object(receiver_type, member) => {
                self.object_call(receiver_type, &member, argument_ids, depth, span)
            }
            NativeDispatch::Call(name) => {
                // The callee is known by id only inside `dispatch_to_member`;
                // what is known here is the NAME. The conventions come from the
                // member the name was minted for, so the lookup is by the
                // instance's own record.
                let target = self.instance_target(&name);
                let mut prelude = String::new();
                let arguments = match target {
                    Some(target) => {
                        self.call_arguments(target, argument_ids, depth, &mut prelude)?
                    }
                    None => self.value_arguments(argument_ids, depth)?,
                };
                Ok(Self::with_argument_prelude(
                    prelude,
                    format!("{name}({})", arguments.join(", ")),
                ))
            }
            // The external member a blanket impl selected. Same three tables an
            // ordinary external call goes through, in the same order, so a
            // binding reached generically and one reached directly emit the
            // same text — and one the tables do not answer is refused by the
            // same name it would be refused by at a direct call site.
            NativeDispatch::Host(member_id) => {
                let Some(external) = self.program.external_functions.get(&member_id) else {
                    return Err(unsupported("an unresolved host binding", span));
                };
                let name = external.name;
                let binding = external.extern_binding.clone();
                if let Some(rendered) =
                    self.runtime_host_binding(name, binding.as_ref(), argument_ids, depth, span)?
                {
                    return Ok(rendered);
                }
                if let Some(rendered) =
                    self.http_host_binding(member_id, binding.as_ref(), argument_ids, depth)?
                {
                    return Ok(rendered);
                }
                if let Some(rendered) =
                    self.json_host_binding(name, binding.as_ref(), argument_ids, depth)?
                {
                    return Ok(rendered);
                }
                if let Some(rendered) = self.scalar_host_binding(
                    member_id,
                    name,
                    binding.as_ref(),
                    argument_ids,
                    depth,
                    span,
                )? {
                    return Ok(rendered);
                }
                if let Some(rendered) =
                    self.math_host_binding(member_id, binding.as_ref(), argument_ids, depth, span)?
                {
                    return Ok(rendered);
                }
                if let Some(rendered) =
                    self.host_module_binding(name, binding.as_ref(), argument_ids, depth)?
                {
                    return Ok(rendered);
                }
                if let Some(rendered) =
                    self.db_host_binding(name, binding.as_ref(), argument_ids, depth)?
                {
                    return Ok(rendered);
                }
                let what = format!(
                    "the host binding `{name}`{}",
                    match binding {
                        Some(ExternBinding::Function { symbol, .. }) => format!(" (`{symbol}`)"),
                        _ => String::new(),
                    }
                );
                let rendered = self.host_gap(what, span);
                // F29: as at a direct host call, the census goes on into the
                // arguments of one reached through a blanket impl.
                self.census_walk(argument_ids, depth);
                rendered
            }
        }
    }

    /// The function id one emitted instance name belongs to, so a dispatched
    /// call can render its arguments against that member's conventions.
    fn instance_target(&self, name: &str) -> Option<Id> {
        self.instances
            .iter()
            .find(|(_, reserved)| reserved.name == name)
            .map(|((id, _), _)| *id)
            .or_else(|| {
                self.default_instances
                    .iter()
                    .find(|(_, reserved)| reserved.name == name)
                    .map(|((id, _), _)| *id)
            })
    }

    /// Emits a trait DEFAULT body specialized for one concrete type, keyed by
    /// (default, that type) so each pairing is emitted once. While the body is
    /// walked `current_self_type` is the concrete type, so its `self.method()`
    /// calls re-dispatch there, and the substitution binds the TRAIT's own
    /// generic parameters to the arguments this type implements it at (B58).
    fn default_instance(
        &mut self,
        default_id: Id,
        type_id: TypeId,
        span: Span,
    ) -> Result<String, Error> {
        let type_id = self.concrete(type_id);
        let key = (default_id, self.type_key(type_id));
        if let Some(reserved) = self.default_instances.get(&key) {
            return Ok(reserved.name.clone());
        }
        let function = self
            .program
            .functions
            .get(&default_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved trait default", span))?;
        let sequence = self
            .default_instances
            .keys()
            .filter(|(id, _)| *id == default_id)
            .count();
        let name = format!("{}_{}_d{sequence}", sanitize(function.name), default_id.0);
        let slot = self.next_function_slot;
        self.next_function_slot += 1;
        self.default_instances.insert(
            key,
            Reserved {
                name: name.clone(),
                slot,
            },
        );
        // REPLACED rather than composed, exactly as the JS emitter does it: a
        // default body has no generic parameters of its own, and the trait's
        // arguments for THIS type are the whole binding it runs under.
        let substitution = self.trait_parameter_substitution(default_id, type_id);
        let saved_self = self.current_self_type.replace(type_id);
        let self_traits = self.self_traits_of(default_id);
        let saved_traits = std::mem::replace(&mut self.current_self_traits, self_traits);
        let saved = std::mem::replace(&mut self.current_substitution, substitution);
        let emitted = self.function_body(&function, function.name_span, false, &name);
        self.current_substitution = saved;
        self.current_self_traits = saved_traits;
        self.current_self_type = saved_self;
        self.functions.insert(slot, emitted?);
        Ok(name)
    }

    fn emit_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        // Argument 0 of an intrinsic is its RECEIVER, and every arm below
        // renders it as a place (`&`, `&mut`, or a method receiver). The rest
        // are values.
        let mutating = mutates_its_receiver(intrinsic);
        // F30: a mutating receiver that lives in a CELL holds a borrow of it
        // for the whole call, and Rust evaluates the receiver BEFORE the
        // arguments — so `counts.push(counts.len())` met its own live borrow
        // and panicked where the JS backend prints an answer (JS has no borrow
        // to collide with: its receiver is a reference). The arguments are
        // hoisted into `let`s, which puts every read of the cell before the
        // borrow is taken; the block is an expression, so the call site is
        // unchanged.
        if mutating
            && argument_ids.len() > 1
            && argument_ids
                .first()
                .is_some_and(|receiver| self.place_lives_in_a_cell(*receiver))
        {
            let mut prelude = String::new();
            let mut arguments = Vec::new();
            for (index, argument) in argument_ids.iter().enumerate().skip(1) {
                let value = self.value_of(*argument, depth)?;
                let name = format!("__borrowed{index}");
                let _ = write!(prelude, "let {name} = {value}; ");
                arguments.push(name);
            }
            let receiver = self.mutable_place(argument_ids[0], depth)?;
            arguments.insert(0, receiver);
            let rendered = self.intrinsic(intrinsic, arguments, span)?;
            return Ok(format!("{{ {prelude}{rendered} }}"));
        }
        let mut arguments = Vec::new();
        for (index, argument) in argument_ids.iter().enumerate() {
            arguments.push(if index == 0 {
                if mutating {
                    self.mutable_place(*argument, depth)?
                } else {
                    self.expression(*argument, depth)?
                }
            } else {
                self.value_of(*argument, depth)?
            });
        }
        self.intrinsic(intrinsic, arguments, span)
    }

    /// Whether a place's ROOT is a binding that lives in a cell — a boxed
    /// binding (R3's `Captured`) or a module-level one (F30's `thread_local!`).
    /// Mutating one takes a borrow that lives to the end of the statement, so a
    /// second read of the same binding in the same statement has to happen
    /// first.
    fn place_lives_in_a_cell(&self, id: Id) -> bool {
        match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) => {
                self.boxed.contains(binding) || self.module_bindings.contains(binding)
            }
            Some(
                Expr::Field(subject, _, _)
                | Expr::Index(subject, _)
                | Expr::TupleIndex(subject, _, _)
                | Expr::Reference(subject, _),
            ) => self.place_lives_in_a_cell(*subject),
            _ => false,
        }
    }

    /// A place the program is about to MUTATE — through an intrinsic's
    /// receiver, through a `&mut` parameter, through an assignment, or through
    /// a `&mut` the source wrote.
    ///
    /// Two binding shapes differ from [`Self::expression`], and for one reason:
    /// both live in a CELL, and a read of a cell answers a VALUE. That is right
    /// for a value and silently wrong for a place, because the mutation then
    /// lands in a temporary copy that is dropped at the end of the statement.
    ///
    /// * a BOXED binding (R3's `Captured` cell): `board.vl`'s
    ///   `mut seen: List<i32> = []` is captured by a subscriber, and
    ///   `seen.push(value)` pushed into a COPY, so the program printed `0 0`
    ///   where the JS backend printed `2 2`.
    /// * a MODULE-LEVEL binding (F30, the same class at module scope): the
    ///   `thread_local!` read answers `cell.get()`, so `counts.push(x)` on a
    ///   module-level `let mut counts: List<i32>` pushed into a temporary and
    ///   the program printed `0` where the JS backend printed `2`. It was
    ///   invisible only because every mutated module binding in the estate held
    ///   a `Shared`, whose copy is the same cell.
    ///
    /// The spine is walked rather than only its root: `counter.n = 7` and
    /// `counts[0] = 5` name the cell through a field and an index, and a place
    /// is only a place if every node between the root and the write is one.
    /// `RefMut` derefs both ways, so a field, an index and a `&mut` all reach
    /// through it the way they reach through the value itself.
    fn mutable_place(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        // `boxed_emitted` is deliberately NOT written here: C15's count measures
        // what the walk EMITTED as a `Captured` cell, which is the DECLARATION's
        // record, and a use cannot precede one.
        match self.program.entity_map.get(&id).cloned() {
            Some(Expr::Local(binding)) if self.boxed.contains(&binding) => {
                Ok(format!("{}.borrow_mut()", self.binding_name(binding)))
            }
            Some(Expr::Local(binding)) if self.module_bindings.contains(&binding) => {
                // The handle is COPIED out of the `thread_local!` and borrowed
                // through the copy, because a borrow of the static itself
                // cannot outlive the `with` closure it is taken in. The copy is
                // a refcount bump naming the same cell, and it lives to the end
                // of the statement, which is exactly as long as the place is
                // used for.
                let cell = self.ensure_module_binding(binding, self.span_of(id))?;
                Ok(format!("{cell}.with(|cell| cell.clone()).borrow_mut()"))
            }
            Some(Expr::Field(subject, _, index)) => {
                let subject_text = self.mutable_place(subject, depth)?;
                let field = self.field_name(subject, index, self.span_of(id))?;
                Ok(format!("{subject_text}.{field}"))
            }
            Some(Expr::Index(subject, index)) => {
                let subject_text = self.mutable_place(subject, depth)?;
                let index_text =
                    self.expecting_nothing(|emitter| emitter.expression(index, depth))?;
                Ok(format!("{subject_text}[({index_text}) as usize]"))
            }
            Some(Expr::TupleIndex(subject, offset, 1)) => {
                let subject_text = self.mutable_place(subject, depth)?;
                Ok(format!("{subject_text}.{offset}"))
            }
            Some(Expr::Reference(operand, true)) => {
                let operand_text = self.mutable_place(operand, depth)?;
                Ok(format!("&mut {operand_text}"))
            }
            _ => self.expression(id, depth),
        }
    }

    fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: Vec<String>,
        span: Span,
    ) -> Result<String, Error> {
        let mut parts = arguments.into_iter();
        let mut next = || parts.next().unwrap_or_else(|| "()".to_string());
        let rendered = match intrinsic {
            Intrinsic::StrLen => format!("vilan_rt::str_len(&{})", next()),
            Intrinsic::StrTrim => format!("vilan_rt::str_trim(&{})", next()),
            Intrinsic::StrToLowercase => format!("vilan_rt::str_to_lowercase(&{})", next()),
            Intrinsic::StrToUppercase => format!("vilan_rt::str_to_uppercase(&{})", next()),
            Intrinsic::StrContains => {
                format!("vilan_rt::str_contains(&{}, &{})", next(), next())
            }
            Intrinsic::StrStartsWith => {
                format!("vilan_rt::str_starts_with(&{}, &{})", next(), next())
            }
            Intrinsic::StrEndsWith => {
                format!("vilan_rt::str_ends_with(&{}, &{})", next(), next())
            }
            Intrinsic::StrReplace => {
                format!(
                    "vilan_rt::str_replace(&{}, &{}, &{})",
                    next(),
                    next(),
                    next()
                )
            }
            Intrinsic::StrRepeat => format!("vilan_rt::str_repeat(&{}, {})", next(), next()),
            Intrinsic::StrSplit => format!("vilan_rt::str_split(&{}, &{})", next(), next()),
            Intrinsic::StrSubstring => {
                format!(
                    "vilan_rt::str_substring(&{}, {}, {})",
                    next(),
                    next(),
                    next()
                )
            }
            Intrinsic::ParseI32 => format!("vilan_rt::parse_i32(&{})", next()),
            Intrinsic::ParseF64 => format!("vilan_rt::parse_f64(&{})", next()),
            Intrinsic::ListLen => format!("({}.len() as i32)", next()),
            Intrinsic::ListGet => format!("vilan_rt::list_get(&{}, ({}) as i64)", next(), next()),
            Intrinsic::ListPop => format!("vilan_rt::list_pop(&mut {})", next()),
            Intrinsic::ListRemove => {
                format!(
                    "vilan_rt::list_remove(&mut {}, ({}) as i64)",
                    next(),
                    next()
                )
            }
            // `sort_by` answers a COPY (`own self`), and its comparator arrives
            // as an `Rc<dyn Fn>` — which implements no `Fn` trait itself, so it
            // is bound and CALLED inside a closure the runtime can take.
            Intrinsic::ListSortBy => {
                let receiver = next();
                let compare = next();
                format!(
                    "{{ let compare = {compare}; \
                     vilan_rt::list_sort_by(&{receiver}, move |a, b| compare(a, b)) }}"
                )
            }
            Intrinsic::ListInsert => format!(
                "vilan_rt::list_insert(&mut {}, ({}) as i64, {})",
                next(),
                next(),
                next()
            ),
            Intrinsic::SharedNew => format!("vilan_rt::Shared::new({})", next()),
            Intrinsic::SharedClone => format!("({}).clone()", next()),
            Intrinsic::SharedValue => format!("({}).get()", next()),
            // `cell.write()` reached HERE is the view used as a place —
            // `cell.write().push(x)`. The assignment form `cell.write() = v` is
            // [`Emitter::cell_write_receiver`]'s and never arrives here, which is
            // why this arm took a second argument it had nowhere to get: it
            // rendered `set(())` and wiped the cell.
            Intrinsic::SharedWrite => format!("({}).borrow_mut()", next()),
            Intrinsic::SharedIdentity => format!("({}).identity()", next()),
            Intrinsic::SharedDowngrade => format!("({}).downgrade()", next()),
            Intrinsic::WeakUpgrade => format!("({}).upgrade()", next()),
            Intrinsic::OptionTake => format!("({}).take()", next()),
            Intrinsic::OptionReplace => format!("({}).replace({})", next(), next()),
            Intrinsic::SetNew => "vilan_rt::Set::new()".to_string(),
            Intrinsic::SetInsert => format!("{}.insert({})", next(), next()),
            Intrinsic::SetContains => format!("{}.contains(&{})", next(), next()),
            Intrinsic::SetRemove => format!("{}.remove(&{})", next(), next()),
            Intrinsic::SetLen => format!("({}.len() as i32)", next()),
            Intrinsic::MapNew => "vilan_rt::Map::new()".to_string(),
            Intrinsic::MapInsert => format!("{}.insert({}, {})", next(), next(), next()),
            Intrinsic::MapGet => format!("{}.get(&{}).cloned()", next(), next()),
            Intrinsic::MapContainsKey => format!("{}.contains_key(&{})", next(), next()),
            Intrinsic::MapRemove => format!("{}.remove(&{})", next(), next()),
            Intrinsic::MapLen => format!("({}.len() as i32)", next()),
            Intrinsic::MapKeys => format!("{}.keys()", next()),
            Intrinsic::MapValues => format!("{}.values()", next()),
            // F20: `canonical_hash(value)` — `__hash` on the JS side. The
            // receiver is the VALUE, not a place, so it is borrowed rather than
            // moved: a `Map::insert` hashes the key and then stores it.
            Intrinsic::CanonicalHash => format!("vilan_rt::canonical_hash(&{})", next()),
            // `hashes_equal(a, b)` — `===` on the two canonical keys, which is
            // NOT the map's own key equality. `vilan_rt::Hash` says why.
            Intrinsic::HashEq => {
                format!("vilan_rt::hashes_equal(&{}, &{})", next(), next())
            }
            // F18 slice 2: `std::json`'s six intrinsics — the four walkers, the
            // normalized kind, and the guarded parse. Each is one method on
            // `vilan_rt::json::JsonValue`, whose doc comment names the JS
            // helper it is the twin of. `kind` answers a `str` because
            // `JsonKind` is a BACKED enum and a backed enum IS its backing
            // value on both backends.
            Intrinsic::JsonField => format!("({}).field(&{})", next(), next()),
            Intrinsic::JsonTag => format!("({}).tag()", next()),
            Intrinsic::JsonElements => format!("({}).elements()", next()),
            Intrinsic::JsonIsNull => format!("({}).is_null()", next()),
            Intrinsic::JsonKind => format!("({}).kind()", next()),
            Intrinsic::TryParseJson => format!("vilan_rt::json::try_parse(&{})", next()),
            // `std::process::env(key)` — `process.env[key]`, whose absent case
            // is `undefined` and reads back as `None`. `std::env::var` answers
            // `Err` for both "not set" and "not UTF-8"; the second is
            // unreachable from a vilan `str`, which is UTF-8 by construction.
            Intrinsic::Env => format!(
                "std::env::var(&*{}).ok().map(|value| vilan_rt::str_new(&value))",
                next()
            ),
            other => return self.host_gap(format!("the intrinsic `{other:?}`"), span),
        };
        Ok(rendered)
    }
}

/// How a generically dispatched member is CALLED, once resolved: the member may
/// be an intrinsic (a runtime form, not a call to an emitted function) or an
/// emitted instance. Without the distinction a generic dispatch landing on an
/// intrinsic would mint a name nothing defines.
enum NativeDispatch {
    Intrinsic(Intrinsic),
    Call(String),
    /// An `external fun` member: the DECLARATION's id, resolved against the
    /// host tables at [`Emitter::emit_dispatch`] where the call's arguments are
    /// in hand.
    Host(Id),
    /// A124 R3: a call through an OBJECT's table — the receiver's `dyn` type
    /// and the member.
    Object(TypeId, String),
}

/// A124 R3: `member` as the object's trait or one of its supertraits DECLARES
/// it — the declaration, the declaring trait, and the substitution the chain's
/// `with` clauses make for that trait's own parameters (`trait Signal<T> with
/// Source<T>` reaches `Source`'s `T` at `Signal`'s).
fn object_member_declaration(
    program: &Program<'_>,
    trait_id: Id,
    member: &str,
) -> Option<(Id, Id, Vec<(TypeId, TypeId)>)> {
    let mut stack: Vec<(Id, Vec<(TypeId, TypeId)>)> = vec![(trait_id, Vec::new())];
    let mut seen: HashSet<Id> = HashSet::new();
    while let Some((id, chain)) = stack.pop() {
        if !seen.insert(id) {
            continue;
        }
        let trait_ = program.traits.get(&id)?;
        if let Some(declaration) = trait_.declarations.get(member) {
            return Some((*declaration, id, chain));
        }
        for supertrait_type_id in &trait_.supertraits {
            if let Some(Type::Trait(super_id, super_arguments)) =
                program.type_id_to_type_map.get(supertrait_type_id)
            {
                let mut extended = chain.clone();
                if let Some(supertrait) = program.traits.get(super_id) {
                    extended.extend(
                        supertrait
                            .generic_parameter_constraint_ids
                            .iter()
                            .copied()
                            .zip(super_arguments.iter().copied()),
                    );
                }
                stack.push((*super_id, extended));
            }
        }
    }
    None
}

/// Whether an intrinsic MUTATES the value its receiver names — the set whose
/// receiver has to reach a boxed binding's cell rather than a copy of its value
/// (see [`Emitter::mutable_place`]).
///
/// `Shared`'s own intrinsics are deliberately NOT here: their receiver is a
/// handle, and a copy of a handle is the same cell, so a `get()` of a boxed
/// binding holding one reaches the same place either way.
fn mutates_its_receiver(intrinsic: Intrinsic) -> bool {
    matches!(
        intrinsic,
        Intrinsic::ListPop
            | Intrinsic::ListRemove
            | Intrinsic::ListInsert
            | Intrinsic::ListSortBy
            | Intrinsic::MapInsert
            | Intrinsic::MapRemove
            | Intrinsic::SetInsert
            | Intrinsic::SetRemove
    )
}

/// How a parameter is received natively.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiving {
    ByValue,
    Ref,
    RefMut,
}

/// The native width of a vilan scalar primitive.
///
/// R6: `i53`/`u53` are distinct types with native widths `i64`/`u64` and the
/// documented note that their RANGE guarantee is the JS one — a program that
/// round-trips both backends behaves the same, and a native-only value outside
/// 2^53 is outside what the language promises either way.
fn scalar_type(name: &str) -> Option<&'static str> {
    Some(match name {
        "i8" => "i8",
        "u8" => "u8",
        "i16" => "i16",
        "u16" => "u16",
        "i32" => "i32",
        "u32" => "u32",
        "i53" => "i64",
        "u53" => "u64",
        // I5: an index is the platform word natively (ruling 1). Its RANGE
        // guarantee is still the JS one, like `u53`'s (§11 Q3).
        "usize" => "usize",
        "f32" => "f32",
        "f64" => "f64",
        "str" => "vilan_rt::Str",
        _ => return None,
    })
}

/// F18: the `external struct`s `std::http` declares over `node:http`, and the
/// `vilan_rt::http` type that IS each one.
///
/// A name table rather than a per-declaration marker, for the reason the
/// executor's four handles are one: these are host types, so nothing in the
/// source says what they are made of, and the mapping is the whole of what the
/// backend knows about them. `NodeAddress` is `std::http`'s private
/// `address()` result; `Bytes` is `std::bytes`'s, and it is here because the
/// HTTP surface is the first thing that needs one.
fn http_host_type(name: &str) -> Option<&'static str> {
    Some(match name {
        "NodeServer" => "vilan_rt::http::Server",
        "NodeAddress" => "vilan_rt::http::Address",
        "NodeRequest" => "vilan_rt::http::Request",
        "NodeResponse" => "vilan_rt::http::Response",
        "NodeSocket" => "vilan_rt::http::Socket",
        "Bytes" => "vilan_rt::http::Bytes",
        // F18 slice 2: `std::bytes`'s two codec classes, which live beside
        // `Bytes` in the runtime for the reason `Bytes` does — the HTTP
        // surface is what first needs them (`Request::body` decodes the
        // collected body).
        "TextDecoder" => "vilan_rt::http::TextDecoder",
        "TextEncoder" => "vilan_rt::http::TextEncoder",
        _ => return None,
    })
}

/// F18 slice 2: the vilan type one `[extern("Number")]` JSON coercion labels
/// its result with, as a Rust scalar.
///
/// A table rather than a parse of the name's suffix, because `i53`/`u53` are
/// `i64`/`u64` natively and a suffix read would mint a type that does not
/// exist. The names are `std::json`'s own; [`scalar_type`] is the same mapping
/// for a type POSITION and this is it for a coercion's RESULT.
fn json_coercion_target(name: &str) -> Option<&'static str> {
    let labelled = name.strip_prefix("coerce_")?;
    scalar_type(labelled).filter(|rendered| *rendered != "vilan_rt::Str")
}

/// One census row from a refusal: the construct, without the boilerplate
/// sentence every refusal carries.
fn census_entry(error: &Error) -> String {
    let message = error.msg.as_str();
    let start = message
        .find("does not emit ")
        .map(|offset| offset + "does not emit ".len())
        .unwrap_or(0);
    let rest = &message[start..];
    let end = rest.find(" yet — this is").unwrap_or(rest.len());
    rest[..end].to_string()
}

/// Whether a rendered Rust type IS a counted closure (F16: every closure type is
/// `Rc<dyn Fn(..) -> ..>`), which is not `PartialEq` — so a field of this type
/// compares by `Rc::ptr_eq`, which is what `===` on a function value means.
///
/// The `contains` spelling this replaces claimed every type that MENTIONS a
/// closure: `std::reactive`'s `Owner` holds `Shared<List<|| void>>`, and its
/// emitted `PartialEq` compared two `Shared`s with `Rc::ptr_eq` — a type error,
/// where the cell's own reference equality was right there.
fn is_closure_type(rendered: &str) -> bool {
    rendered.starts_with("std::rc::Rc<dyn Fn")
}

/// Whether a rendered type mentions a closure ANYWHERE inside it, which is the
/// question `Js` and `Json` ask: there is no rendering for a function value on
/// either side (node prints `[Function: <name>]` off a name this backend cannot
/// reproduce, and `JSON.stringify` omits the key), so an aggregate that reaches
/// one refuses at run time by name rather than guessing bytes.
fn mentions_a_closure(rendered: &str) -> bool {
    rendered.contains("dyn Fn")
}

/// Whether a rendered type's own `PartialEq` is CELL IDENTITY, which does not
/// ask anything of what the cell holds.
///
/// `Shared<T>` and `Weak<T>` compare by `ptr_eq` for every `T` — the reference
/// equality two `Shared`s have on the JS backend, where a cell is an object —
/// so a field of one compares with `==` however deep a closure sits inside it.
/// `std::reactive`'s `Owner.cleanups: Shared<List<|| void>>` and
/// `Subscription.release: Shared<Option<|| void>>` are the two, and reading them
/// as ordinary closure-reaching fields refused eleven programs that had been
/// byte-identical.
fn compares_by_cell_identity(rendered: &str) -> bool {
    rendered.starts_with("vilan_rt::Shared<") || rendered.starts_with("vilan_rt::Weak<")
}

/// The host handle a rendered type IS, if it is one — F25's other unprintable.
///
/// A host handle has no rendering the language defines (what node prints is its
/// own object inspection), and every one of them is a path into this runtime's
/// two host modules, so the rendered type is where the fact already lives.
fn host_handle_name(rendered: &str) -> Option<&'static str> {
    const HANDLES: &[(&str, &str)] = &[
        ("vilan_rt::executor::Task", "Task"),
        ("vilan_rt::executor::Nursery", "Nursery"),
        ("vilan_rt::executor::CancelSignal", "CancelSignal"),
        ("vilan_rt::executor::TimerHandle", "TimerHandle"),
        ("vilan_rt::http::Server", "NodeServer"),
        ("vilan_rt::http::Address", "NodeAddress"),
        ("vilan_rt::http::Request", "NodeRequest"),
        ("vilan_rt::http::Response", "NodeResponse"),
        ("vilan_rt::http::Socket", "NodeSocket"),
        ("vilan_rt::http::Bytes", "Bytes"),
    ];
    HANDLES
        .iter()
        .find(|(path, _)| rendered.starts_with(path))
        .map(|(_, name)| *name)
}

fn is_integer_type(rendered: &str) -> bool {
    matches!(
        rendered,
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64" | "usize"
    )
}

/// A vilan identifier as a Rust one. Vilan's identifier grammar is a subset of
/// Rust's already, so this only has to keep a vilan name that happens to be a
/// Rust keyword from becoming one.
fn sanitize(name: &str) -> String {
    const RUST_KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while", "async", "await", "box", "final", "macro", "override",
        "priv", "try", "typeof", "unsized", "virtual", "yield", "abstract", "become", "do",
    ];
    if RUST_KEYWORDS.contains(&name) {
        return format!("r#{name}");
    }
    name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
}

/// A string literal's source text, as a Rust literal.
///
/// **The text is not the value.** An `Expr::String`'s text is the SOURCE body
/// of the literal, escapes unprocessed, and the JS emitter turns it into a
/// value with `transformer::unescape_string` before its formatter re-escapes it
/// for JavaScript. S1a wrote the text straight into a Rust literal and escaped
/// its backslashes, so `print("a\nb")` printed `a\nb` natively against the JS
/// backend's two lines — and no program in the accepted corpus carried an
/// escape, so the differential never saw it.
///
/// The escape set is `unescape_string`'s, exactly: `\n`, `\t`, `\r`, `\"`,
/// `\\`, `\0`, an unknown escape keeping BOTH characters (which is what makes
/// the lexer's `RAW_BACKSLASH` doubling round-trip), and a CRLF folded to one
/// `\n` (`windows-support.md` §2). It is reproduced rather than called because
/// the function is private to `transformer.rs`; the lane's report asks for it to
/// be shared.
fn rust_string(text: &str) -> String {
    let value = unescape_string_value(text);
    let mut out = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            other if (other as u32) < 0x20 || other as u32 == 0x7f => {
                let _ = write!(out, "\\u{{{:x}}}", other as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The VALUE of a vilan string literal's body.
///
/// F26: `transformer::unescape_string` IS this function and is now `pub`, so
/// the backend calls it rather than carrying a copy that drifts — a literal's
/// value must be the same value on both backends, and two implementations of
/// "what does `\\n` mean" is exactly the shape the differential can only catch
/// after it has shipped.
fn unescape_string_value(raw: &str) -> String {
    vilan_core::transformer::unescape_string(raw).into_owned()
}

fn describe(resolved: &Type) -> String {
    match resolved {
        Type::Any => "any".to_string(),
        Type::Never => "never".to_string(),
        Type::Mapped(_, _, _) => "a mapped tuple".to_string(),
        Type::Trait(_, _) => "a trait object".to_string(),
        Type::Dyn(_, _) => "a `dyn` trait object".to_string(),
        Type::Function(_) => "a function value".to_string(),
        Type::Module(_) => "a module".to_string(),
        Type::Unknown | Type::Unresolved => "an unresolved type".to_string(),
        _ => "an unsupported type".to_string(),
    }
}

/// Every binding a pattern introduces, in source order.
fn collect_pattern_bindings(pattern: &ExprPattern, out: &mut Vec<Id>) {
    match pattern {
        ExprPattern::Binding(id) => out.push(*id),
        ExprPattern::Variant(_, _, payload) => {
            for sub in payload {
                collect_pattern_bindings(sub, out);
            }
        }
        ExprPattern::Tuple(elements) => {
            for (element, _) in elements {
                collect_pattern_bindings(element, out);
            }
        }
        ExprPattern::Wildcard | ExprPattern::Literal(_) | ExprPattern::Array(_) => {}
    }
}

/// [`collect_pattern_bindings`] into a set — what the closure scan's
/// declared-inside side wants.
fn collect_pattern_bindings_into(pattern: &ExprPattern, out: &mut HashSet<Id>) {
    let mut bindings = Vec::new();
    collect_pattern_bindings(pattern, &mut bindings);
    out.extend(bindings);
}

fn collect_if_children(branch: &ExprIfBranch, children: &mut Vec<Id>) {
    match branch {
        ExprIfBranch::If(condition, (statements, tail), next) => {
            children.push(*condition);
            children.extend(statements.iter().copied());
            children.push(*tail);
            if let Some(next) = next {
                collect_if_children(next, children);
            }
        }
        ExprIfBranch::Else((statements, tail)) => {
            children.extend(statements.iter().copied());
            children.push(*tail);
        }
    }
}

fn form_name(expr: &Expr<'_>) -> &'static str {
    match expr {
        Expr::Async(_) => "an `async` block",
        Expr::Await(_) => "an `await`",
        Expr::TryAssert(_) => "a `!` assertion",
        Expr::Lift(_, _, _) | Expr::LiftBinder | Expr::LiftRegion(_, _) => "a `?` lift",
        Expr::Destructure(_, _) => "a destructuring binding",
        Expr::TupleComprehension(_, _, _) => "a tuple comprehension",
        Expr::Repeat(_, _) => "a `[value; n]` literal",
        Expr::ArrayLen(_, _) => "a fixed-array `len()`",
        Expr::Generic(_) => "a generic type reference",
        Expr::Macro => "a macro name",
        _ => "an unsupported form",
    }
}

/// The `Cargo.toml` of the project the backend writes, pointing at the runtime
/// crate by path. `edition 2024` matches the workspace's own.
pub fn cargo_manifest(name: &str, runtime_path: &str, reaches_sqlite: bool) -> String {
    // F18 slice 2, Order 39's R1: `vilan-rt-sqlite` is named only when the
    // program reached `std::db`. A program that did not pays neither the
    // lockfile entry nor the C compile of SQLite's amalgamation, which is the
    // whole reason the surface is a crate apart from the dependency-free
    // runtime. The path is a SIBLING of the runtime's, because that is how the
    // two sit in the repository and in the materialized cache alike.
    let sqlite = if reaches_sqlite {
        let beside = std::path::Path::new(runtime_path)
            .parent()
            .map(|parent| parent.join("vilan-rt-sqlite"))
            .unwrap_or_else(|| std::path::PathBuf::from("vilan-rt-sqlite"));
        format!(
            "vilan-rt-sqlite = {{ path = {:?} }}\n",
            beside.to_string_lossy()
        )
    } else {
        String::new()
    };
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         \n\
         [[bin]]\n\
         name = \"{name}\"\n\
         path = \"src/main.rs\"\n\
         \n\
         [dependencies]\n\
         vilan-rt = {{ path = {runtime_path:?} }}\n\
         {sqlite}\
         \n\
         [profile.release]\n\
         panic = \"unwind\"\n\
         \n\
         [workspace]\n"
    )
}
