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
    Expr, ExprIfBranch, ExprMatchLeg, ExprPattern, GenericDispatch, Intrinsic, Program,
};
use vilan_core::error::Error;
use vilan_core::fx::FxHashMap as HashMap;
use vilan_core::id::Id;
use vilan_core::impl_select;
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
    /// R3's measurement: how many bindings this program had to box into
    /// `vilan_rt::Captured<_>` (an `Rc<RefCell<_>>`) because a closure captures
    /// them and something writes them. C15's by-value capture optimisation is
    /// the later item this number pays for.
    pub boxed_bindings: usize,
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

/// The member a concatenation's render dispatch calls (B176).
///
/// `analyzer::RENDER_MEMBER` is the source of truth and is `pub(crate)`, so a
/// backend crate outside `vilan-core` cannot name it. Copied here rather than
/// widened, because the ownership map for this order gives `analyzer.rs` to
/// three other lanes and a one-token visibility change is not worth a merge
/// conflict — the lane's report asks for the widening instead.
const RENDER_MEMBER: &str = "to_string";

/// The name `async fun main`'s body takes, since `fn main` cannot be `async`
/// and the executor has to be entered from a synchronous frame.
const ASYNC_MAIN_BODY: &str = "vilan_async_main";

/// The prelude every emitted program carries.
const PRELUDE: &str = "\
#![allow(unused_imports, unused_parens, unused_variables, unused_mut, unused_braces)]
#![allow(dead_code)]
#![allow(unreachable_patterns, non_camel_case_types, non_snake_case, clippy::all)]
use vilan_rt::Js as _;
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
    /// J6: each context-threaded hidden parameter's flavour, as
    /// [`Emitter::compute_context_flavours`] reads it off the arguments its call
    /// sites pass.
    context_flavours: BTreeMap<u32, ContextFlavour>,
    /// J6: the name of the function whose body is being walked — a spawn's
    /// ORIGIN, which is what the unobserved-failure report names. The JS
    /// emitter keeps the same thing under the same name.
    current_origin: Option<&'src str>,
}

/// Whether a context-threaded hidden parameter carries the context's value or an
/// `Option` of it.
///
/// reactive-turns.md §5 (1): the hidden parameter for a `get_safe`-reachable
/// region carries `Option<T>` and a strict-`get` region keeps the bare flavour.
/// The context pass marks the parameter in `context_hidden_parameters` but
/// records no type for it — it is deliberately not source — so the flavour is
/// recovered from what the call sites PASS.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ContextFlavour {
    Bare,
    Optional,
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
            context_flavours: BTreeMap::new(),
            current_origin: None,
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
        self.compute_context_flavours();

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
            boxed_bindings: self.boxed_emitted.len(),
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

    /// J6: which flavour each context-threaded hidden parameter carries.
    ///
    /// `context.rs` gives every needs-context function a record-LESS parameter
    /// (no `parameters` entry, no span, no type) and appends the value as an
    /// argument at each call. So the parameter's type is not written down
    /// anywhere, but it is determined: the argument the callers pass is either
    /// a literal `None`, a `Some(..)` wrap, or a read of the CALLER's own
    /// hidden parameter. The first two settle a callee outright; the third
    /// propagates, which is why this is a worklist rather than one pass.
    ///
    /// An undetermined parameter stays out of the map and
    /// [`Emitter::parameter_declaration`] refuses at it — a guessed flavour
    /// would be a type error in the emitted Rust, and a refusal by name is the
    /// standing answer to a construct this backend cannot see through.
    fn compute_context_flavours(&mut self) {
        // Which closures a clause-typed PARAMETER can hold — every closure
        // literal any call site hands it. `nursery(|n| { .. })` is the shape:
        // the body's own hidden parameter is bound not at the `nursery(..)`
        // call but inside `nursery`, where the parameter is CALLED, so the two
        // have to be connected before the flavours can propagate.
        let mut closures_by_parameter: HashMap<Id, Vec<Id>> = HashMap::default();
        for call in self.program.function_calls.values() {
            let Some(receiving) = self.receiving_parameters(call.subject_id) else {
                continue;
            };
            for (position, parameter_id) in receiving.iter().enumerate() {
                if let Some(argument) = call.argument_ids.get(position)
                    && let Some(Expr::Closure(closure_id)) =
                        self.program.entity_map.get(argument).cloned()
                {
                    closures_by_parameter
                        .entry(*parameter_id)
                        .or_default()
                        .push(closure_id);
                }
            }
        }

        // (callee's hidden parameter, the argument expression a call passes it).
        let mut edges: Vec<(Id, Id)> = Vec::new();
        for call in self.program.function_calls.values() {
            // A call's subject is a named callee, or — after `Context::run`
            // lowered to `body(value)` — the closure itself, or a clause-typed
            // parameter, in which case every closure that can land there binds
            // its own hidden parameter at this position.
            let mut receiving_lists: Vec<Vec<Id>> = Vec::new();
            if let Some(receiving) = self.receiving_parameters(call.subject_id) {
                receiving_lists.push(receiving);
            }
            if let Some(Expr::Local(target)) = self.program.entity_map.get(&call.subject_id)
                && let Some(candidates) = closures_by_parameter.get(target)
            {
                for closure_id in candidates {
                    if let Some(closure) = self.program.closures.get(closure_id) {
                        receiving_lists.push(closure.parameters.clone());
                    }
                }
            }
            for receiving in receiving_lists {
                for (position, parameter_id) in receiving.iter().enumerate() {
                    if self
                        .program
                        .context_hidden_parameters
                        .contains_key(parameter_id)
                        && let Some(argument) = call.argument_ids.get(position)
                    {
                        edges.push((*parameter_id, *argument));
                    }
                }
            }
        }
        // The two determined shapes first, then propagate through the reads.
        for (parameter, argument) in &edges {
            if let Some(flavour) = self.flavour_of_argument(*argument) {
                self.context_flavours.insert(parameter.0, flavour);
            }
        }
        loop {
            let mut changed = false;
            for (parameter, argument) in &edges {
                if self.context_flavours.contains_key(&parameter.0) {
                    continue;
                }
                if let Some(Expr::Local(source)) = self.program.entity_map.get(argument)
                    && let Some(flavour) = self.context_flavours.get(&source.0).copied()
                {
                    self.context_flavours.insert(parameter.0, flavour);
                    changed = true;
                }
            }
            if !changed {
                return;
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
    /// a clause position has a hidden parameter for the same context, and
    /// [`Emitter::compute_context_flavours`] settled those. Two closures that
    /// disagree would need two types, which is refused rather than guessed.
    fn context_clause_type(&mut self, context: Id, span: Span) -> Result<String, Error> {
        let name = self
            .program
            .variables
            .get(&context)
            .map(|variable| variable.name)
            .unwrap_or("a context");
        let mut settled: Option<ContextFlavour> = None;
        for closure in self.program.closures.values() {
            for parameter_id in &closure.parameters {
                if self.program.context_hidden_parameters.get(parameter_id) != Some(&context) {
                    continue;
                }
                let Some(flavour) = self.context_flavours.get(&parameter_id.0).copied() else {
                    continue;
                };
                if settled.is_some_and(|already| already != flavour) {
                    return Err(unsupported(
                        &format!(
                            "a closure type carrying the context `{name}`, whose closures do \
                             not agree on whether the threaded value arrives as an `Option`"
                        ),
                        span,
                    ));
                }
                settled = Some(flavour);
            }
        }
        // Same default as the parameter's own: strict, so a wrong guess is a
        // type error rather than a wrong answer.
        let flavour = settled.unwrap_or(ContextFlavour::Bare);
        let value = self.context_value_type(context, span)?;
        Ok(match flavour {
            ContextFlavour::Bare => value,
            ContextFlavour::Optional => format!("Option<{value}>"),
        })
    }

    /// The parameter list a call's SUBJECT receives against — a named callee's,
    /// or a closure literal's where `Context::run` lowered `run(value, body)`
    /// into `body(value)`.
    fn receiving_parameters(&self, subject_id: Id) -> Option<Vec<Id>> {
        match self.program.entity_map.get(&subject_id)? {
            Expr::Local(target) => self
                .program
                .functions
                .get(target)
                .map(|function| function.parameters.clone()),
            Expr::Closure(closure_id) => self
                .program
                .closures
                .get(closure_id)
                .map(|closure| closure.parameters.clone()),
            _ => None,
        }
    }

    /// The flavour a context argument's own SHAPE settles: a bare `None` or a
    /// `Some(..)` wrap says `Option<T>`, and a read of an in-scope value says
    /// the bare flavour. A read of another hidden parameter settles nothing here
    /// — that is the propagating case.
    fn flavour_of_argument(&self, argument: Id) -> Option<ContextFlavour> {
        match self.program.entity_map.get(&argument) {
            Some(Expr::Local(binding)) => match self.program.entity_map.get(binding) {
                Some(Expr::EnumVariant(enum_id, _)) => self
                    .program
                    .enums
                    .get(enum_id)
                    .filter(|declaration| declaration.name == "Option")
                    .map(|_| ContextFlavour::Optional),
                _ if self.program.context_hidden_parameters.contains_key(binding) => None,
                _ => Some(ContextFlavour::Bare),
            },
            Some(Expr::Call(call_id)) => {
                let call = self.program.function_calls.get(call_id)?;
                let Some(Expr::Local(subject)) = self.program.entity_map.get(&call.subject_id)
                else {
                    return None;
                };
                match self.program.entity_map.get(subject) {
                    Some(Expr::EnumVariant(enum_id, _)) => self
                        .program
                        .enums
                        .get(enum_id)
                        .filter(|declaration| declaration.name == "Option")
                        .map(|_| ContextFlavour::Optional),
                    _ => None,
                }
            }
            _ => None,
        }
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
            Some(_) => {
                for child in self.children_of(expr_id) {
                    self.scan_closure(child, declared, referenced, visited);
                }
            }
            None => {}
        }
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
        let mut generics = Vec::new();
        for parameter_id in &function.parameters {
            if let Some(parameter) = self.program.parameters.get(parameter_id) {
                self.collect_type_generics(parameter.type_id, 0, &mut generics);
            }
        }
        if let Some(return_type_id) = function.return_type_id {
            self.collect_type_generics(return_type_id, 0, &mut generics);
        }
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

    /// The `Generic` constraint ids a type's structure mentions.
    fn collect_type_generics(&self, type_id: TypeId, depth: usize, out: &mut Vec<TypeId>) {
        if depth > 24 {
            return;
        }
        match self.program.type_id_to_type_map.get(&type_id) {
            Some(Type::Generic(constraint_id)) => {
                if !out.contains(constraint_id) {
                    out.push(*constraint_id);
                }
            }
            Some(
                Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Tuple(arguments),
            ) => {
                for argument in arguments.clone() {
                    self.collect_type_generics(argument, depth + 1, out);
                }
            }
            Some(Type::Closure(parameters, return_type_id, _)) => {
                let parameters = parameters.clone();
                let return_type_id = *return_type_id;
                for parameter in parameters {
                    self.collect_type_generics(parameter, depth + 1, out);
                }
                self.collect_type_generics(return_type_id, depth + 1, out);
            }
            Some(Type::Array(element_id, _)) => {
                self.collect_type_generics(*element_id, depth + 1, out);
            }
            _ => {}
        }
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
        let mut substitution = HashMap::default();
        let Some((trait_id, trait_)) = self
            .program
            .traits
            .iter()
            .find(|(_, trait_)| trait_.declarations.values().any(|id| *id == default_id))
        else {
            return substitution;
        };
        if trait_.generic_parameter_constraint_ids.is_empty() {
            return substitution;
        }
        let Some(implementation) =
            impl_select::select_implementation(self.program, None, type_id, *trait_id)
        else {
            return substitution;
        };
        impl_select::bind_subject(
            self.program,
            implementation.subject,
            type_id,
            &mut substitution,
        );
        let Some((_, arguments)) = implementation
            .trait_args
            .iter()
            .find(|(provided, _)| provided == trait_id)
        else {
            return substitution;
        };
        for (parameter_id, argument_id) in trait_
            .generic_parameter_constraint_ids
            .iter()
            .zip(arguments)
        {
            substitution.insert(*parameter_id, *argument_id);
        }
        substitution
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
                Ok(format!(
                    "std::rc::Rc<dyn Fn({}) -> {}>",
                    parts.join(", "),
                    returned
                ))
            }
            Type::Struct(id, arguments) => self.nominal_struct(id, &arguments, span),
            Type::Enum(id, arguments) => self.nominal_enum(id, &arguments, span),
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
        match name {
            "List" => Ok(format!(
                "Vec<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            "Set" => Ok(format!(
                "vilan_rt::Set<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            "Map" | "NativeMap" => Ok(format!("vilan_rt::Map<{}>", rendered.join(", "))),
            "Shared" | "SignalCell" => Ok(format!(
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
            _ if external => {
                let what = format!("the host type `{name}`");
                self.host_gap(what, span).map(|_| "()".to_string())
            }
            _ => Ok(self.ensure_struct(id, arguments, span)?.name),
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
        match name {
            "bool" => Ok("bool".to_string()),
            "Option" => Ok(format!(
                "Option<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
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
            .map(|field| self.rust_type(field.type_id, span))
            .collect();
        self.current_substitution = saved;
        let rendered_types = rendered_types?;
        let holds_a_closure = rendered_types
            .iter()
            .any(|rendered| is_closure_type(rendered));

        let mut out = String::new();
        // A CLOSURE field is why `PartialEq` cannot simply be derived:
        // `Rc<dyn Fn>` does not implement it. The answer is not to drop
        // equality — a struct holding a callback is compared in real reactive
        // code — but to write the impl JavaScript's `===` already gives, which
        // for a function value is REFERENCE equality and for an `Rc` is
        // `ptr_eq`.
        if holds_a_closure {
            let _ = writeln!(out, "#[derive(Clone)]");
        } else {
            let _ = writeln!(out, "#[derive(Clone, PartialEq)]");
        }
        let _ = writeln!(out, "struct {type_name} {{");
        for (field, rendered) in declaration.fields.iter().zip(rendered_types.iter()) {
            let _ = writeln!(out, "    {}: {rendered},", sanitize(field.name));
        }
        let _ = writeln!(out, "}}");
        if holds_a_closure {
            let comparisons: Vec<String> = declaration
                .fields
                .iter()
                .zip(rendered_types.iter())
                .map(|(field, rendered)| {
                    let name = sanitize(field.name);
                    if is_closure_type(rendered) {
                        format!("std::rc::Rc::ptr_eq(&self.{name}, &other.{name})")
                    } else {
                        format!("self.{name} == other.{name}")
                    }
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
        if holds_a_closure {
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
        self.types.insert(slot, out);
        Ok(Reserved {
            name: type_name,
            slot,
        })
    }

    fn ensure_enum(&mut self, id: Id, arguments: &[TypeId], span: Span) -> Result<Reserved, Error> {
        let declaration = self.program.enums.get(&id).cloned().unwrap();
        if declaration.resource {
            return Err(unsupported(
                &format!(
                    "the `resource` enum `{}` (destruction.md's teardown is a later slice)",
                    declaration.name
                ),
                span,
            ));
        }
        if declaration.backing.is_some() {
            return Err(unsupported(
                &format!("the backed enum `{}`", declaration.name),
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
        let mut rendered = Ok(Vec::new());
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
                }
                Ok(variants) => variants.push(format!(
                    "    {}({}),",
                    sanitize(variant.name),
                    payload.join(", ")
                )),
                Err(_) => break,
            }
        }
        self.current_substitution = saved;
        let variants = rendered?;

        let mut out = String::new();
        let _ = writeln!(out, "#[derive(Clone, PartialEq)]");
        let _ = writeln!(out, "enum {type_name} {{");
        for variant in variants {
            let _ = writeln!(out, "{variant}");
        }
        let _ = writeln!(out, "}}");
        let _ = writeln!(out, "impl vilan_rt::Js for {type_name} {{");
        let _ = writeln!(out, "    fn js(&self) -> String {{");
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
        self.types.insert(slot, out);
        Ok(Reserved {
            name: type_name,
            slot,
        })
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
        let key: Vec<String> = entries
            .iter()
            .map(|(_, type_id)| self.type_key(*type_id))
            .collect();
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
        let emitted = self.function_body(&function, span, is_main, &name);
        self.current_substitution = saved;
        let out = emitted?;
        self.functions.insert(slot, out);
        Ok(Reserved { name, slot })
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
        let is_async = self.program.async_functions.contains(&function.id);
        let mut parameters = Vec::new();
        for parameter_id in &function.parameters {
            parameters.push(self.parameter_declaration(*parameter_id, span)?);
        }
        let returned = match self.return_type_of(function) {
            Some(type_id) => {
                let rendered = self.rust_type(type_id, span)?;
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

        let mut body = String::new();
        let saved_view = std::mem::replace(
            &mut self.current_returns_view,
            function.returns_view || function.returns_mut_view,
        );
        let saved_origin = self.current_origin.replace(function.name);
        let walked = self.emit_block(&function.body.0, function.body.1, &mut body, 1);
        self.current_origin = saved_origin;
        self.current_returns_view = saved_view;
        walked?;

        let mut out = String::new();
        if is_main {
            if is_async {
                // `async fun main` — `main` itself cannot be async, so the real
                // body is its own `async fn` and `main` is the one call into the
                // executor. `block_on` drives the loop until both the microtask
                // queue and the deadline list are empty, which is where node
                // exits too.
                let _ = writeln!(out, "fn main() {{");
                let _ = writeln!(
                    out,
                    "    vilan_rt::executor::block_on({ASYNC_MAIN_BODY}());"
                );
                let _ = writeln!(out, "}}");
                let _ = writeln!(out, "async fn {ASYNC_MAIN_BODY}() {{");
            } else {
                let _ = writeln!(out, "fn main() {{");
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
        if parameter.lazy {
            return Err(unsupported("a `lazy` parameter", span));
        }
        let rendered = self.rust_type(parameter.type_id, span)?;
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
        // Nothing settled it: take the STRICT reading, which is the direction to
        // be wrong in. A parameter that is really the safe one then becomes a
        // rustc type error rather than a wrong answer — and the reactive path
        // (`turn_scope`, `owner_scope`) is threaded by `run` alone, where the
        // value is always present and the strict reading is the right one.
        let flavour = self
            .context_flavours
            .get(&id.0)
            .copied()
            .unwrap_or(ContextFlavour::Bare);
        let value = self.context_value_type(context, span)?;
        Ok(match flavour {
            ContextFlavour::Bare => value,
            ContextFlavour::Optional => format!("Option<{value}>"),
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
            let rendered = if returns_the_loan {
                self.expression(tail, depth)?
            } else {
                self.value_of(tail, depth)?
            };
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
                Some(value) => format!("return {}", self.value_of(value, depth)?),
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
                let index_text = self.expression(index, depth)?;
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
                let operand_text = self.expression(operand, depth)?;
                if mutable {
                    format!("&mut {operand_text}")
                } else {
                    format!("&{operand_text}")
                }
            }
            Expr::Dereference(operand) => format!("(*{})", self.expression(operand, depth)?),
            Expr::Call(call_id) => self.call(id, call_id, depth, span)?,
            Expr::Async(spawned) => self.async_spawn(id, spawned, depth, span)?,
            Expr::Await(awaited) => self.await_of(awaited, depth)?,
            Expr::Closure(closure_id) => self.closure(closure_id, depth, span)?,
            Expr::Is(subject, pattern) => self.is_test(subject, &pattern, depth, span)?,
            Expr::EnumVariant(enum_id, index) => {
                let arguments = self.enum_arguments_at(id, enum_id);
                self.variant_path(enum_id, index, &arguments, span)?
            }
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
        rendered
    }

    /// An expression in a VALUE position — rule 1's copy applied where the
    /// analyzer already decided one is owed. `clone_sites` is the JS emitter's
    /// own `__clone` decision, read here so the two backends copy in exactly
    /// the same places rather than in two opinions of the same places.
    fn value_of(&mut self, id: Id, depth: usize) -> Result<String, Error> {
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
        Ok(text)
    }

    /// Whether `id` reads a parameter this emitter receives by reference.
    fn reads_a_loaned_parameter(&self, id: Id) -> bool {
        let binding = match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) | Some(Expr::Parameter(binding)) => *binding,
            _ => return false,
        };
        self.program
            .parameters
            .get(&binding)
            .is_some_and(|parameter| self.receiving_form(parameter) != Receiving::ByValue)
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
                        "Task" | "Nursery" | "CancelSignal" | "TimerHandle"
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
        // A write THROUGH a counted cell — `*self.value = v`, which the
        // analyzer lowers to a deref of the cell's read intrinsic. The place
        // is not a Rust place at all (`Shared` hands out a value, not a
        // reference), so the write is the cell's own `set`; rendering the
        // target and assigning to it produced `(*(cell).set(())) = v`, which
        // rustc refused — the intrinsic's second argument had nowhere to come
        // from because the VALUE is the assignment's.
        if let Some(receiver) = self.cell_write_receiver(target) {
            let receiver_text = self.expression(receiver, depth)?;
            let value_text = self.value_of(value, depth)?;
            return Ok(format!("({receiver_text}).set({value_text})"));
        }
        let named = match self.program.entity_map.get(&target) {
            Some(Expr::Local(binding)) => Some(*binding),
            _ => None,
        };
        if let Some(binding) = named {
            if self.boxed.contains(&binding) {
                let value_text = self.value_of(value, depth)?;
                return Ok(format!("{}.set({value_text})", self.binding_name(binding)));
            }
            if self.module_bindings.contains(&binding) {
                let cell = self.ensure_module_binding(binding, span)?;
                let value_text = self.value_of(value, depth)?;
                return Ok(format!(
                    "{cell}.with(|cell| *cell.borrow_mut() = {value_text})"
                ));
            }
            if self.binding_holds_a_view(binding) {
                let value_text = self.value_of(value, depth)?;
                return Ok(format!("*{} = {value_text}", self.binding_name(binding)));
            }
        }
        let target_text = self.expression(target, depth)?;
        let value_text = self.value_of(value, depth)?;
        Ok(format!("{target_text} = {value_text}"))
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
        let rendered = paired.and_then(|_| {
            let target_text = self.expression(target, depth)?;
            let value_text = self.value_of(value, depth)?;
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
        match self.program.entity_map.get(&initial) {
            Some(Expr::Reference(_, _)) => true,
            Some(Expr::Call(call_id)) => self
                .program
                .function_calls
                .get(call_id)
                .and_then(|call| match self.program.entity_map.get(&call.subject_id) {
                    Some(Expr::Local(target)) => self.program.functions.get(target),
                    _ => None,
                })
                .is_some_and(|function| function.returns_view || function.returns_mut_view),
            _ => false,
        }
    }

    fn read_binding(&self, binding: Id) -> String {
        let name = self.binding_name(binding);
        if self.boxed.contains(&binding) {
            return format!("{name}.get()");
        }
        name
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
        Ok(format!("{cell}.with(|cell| cell.borrow().clone())"))
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
        let rendered = self
            .rust_type(variable.type_id, span)
            .and_then(|rendered| self.value_of(initial, 0).map(|value| (rendered, value)));
        self.current_substitution = saved;
        let (rendered, value) = rendered?;
        let mut out = String::new();
        let _ = writeln!(out, "thread_local! {{");
        let _ = writeln!(
            out,
            "    static {cell}: std::cell::RefCell<{rendered}> = \
             std::cell::RefCell::new({value});"
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
                let value = self.value_of_expecting(initial, Some(variable.type_id), depth)?;
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
        let rendered = match self
            .type_of(id)
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
        if self.program.binary_op_dispatch.contains_key(&id) {
            return Err(unsupported("an overloaded operator", span));
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
        let left_text = self.expression(left, depth)?;
        let right_text = self.expression(right, depth)?;
        Ok(format!("({left_text} {symbol} {right_text})"))
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
                        let subject_text = self.expression(*subject, depth)?;
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
        let subject_text = self.expression(subject, depth)?;
        let subject_type = self.type_of(subject);
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
            ExprPattern::Literal(id) => self.expression(*id, 0),
            ExprPattern::Variant(enum_id, index, payload) => {
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
    /// is not reusable here: it binds a `Type::Generic` node, and a nominal
    /// declaration's parameter can appear in its own body as the constraint id
    /// itself (whose `Type` is `Any`). This one is told which ids are
    /// parameters, so both spellings bind.
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
        if parameters.contains(&pattern) {
            out.entry(pattern).or_insert(concrete);
            return;
        }
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
        if let Some(Type::Struct(found, arguments)) = self
            .type_of(expr_id)
            .and_then(|type_id| self.resolve(type_id))
            && *found == struct_id
        {
            return arguments.clone();
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
            parts.push(format!(
                "{name}: {}",
                self.value_of_expecting(*value, Some(expecting), depth)?
            ));
        }
        Ok(format!("{} {{ {} }}", instance.name, parts.join(", ")))
    }

    fn for_each(
        &mut self,
        _id: Id,
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
            return Err(unsupported(
                concat!(
                    "a `for` over anything but a `List` ",
                    "(an `Iterator` impl is a monomorphised generic)"
                ),
                span,
            ));
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
        if self.program.async_functions.contains(&closure_id) {
            return self.host_gap(
                "an `async` closure as a VALUE (a callee taking one at one call site and a \
                 synchronous closure at another is an adapted instance)"
                    .to_string(),
                span,
            );
        }
        let mut parameters = Vec::new();
        for parameter_id in &closure.parameters {
            parameters.push(self.parameter_declaration(*parameter_id, span)?);
        }
        let body = self.expression(closure.return_, depth)?;
        // A `move` closure takes its captures by value, so a captured CELL has
        // to be a handle of its own — otherwise the binding outside is moved
        // into the closure and every later read of it is a use-after-move.
        let mut declared_inside = HashSet::new();
        let mut referenced = HashSet::new();
        let mut visited = HashSet::new();
        self.scan_closure(
            closure.return_,
            &mut declared_inside,
            &mut referenced,
            &mut visited,
        );
        let mut captures: Vec<Id> = referenced
            .into_iter()
            .filter(|binding| self.boxed.contains(binding) && !declared_inside.contains(binding))
            .collect();
        captures.sort_by_key(|binding| binding.0);
        let prelude: String = captures
            .iter()
            .map(|binding| {
                let name = self.binding_name(*binding);
                format!("let {name} = {name}.clone(); ")
            })
            .collect();
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
    fn await_of(&mut self, awaited: Id, depth: usize) -> Result<String, Error> {
        // `value_of` retains a handle read out of a binding (J6's rule for the
        // executor's handles), which is exactly what awaiting a task twice
        // needs: a task is a handle, and awaiting a settled one answers again.
        let operand = self.value_of(awaited, depth)?;
        Ok(format!("({operand}).await"))
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
        let mut declared_inside = HashSet::new();
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
            let value = self.value_of(*argument, depth)?;
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
            return Ok(format!("({rendered}).await"));
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
                return self.host_gap(
                    "a view inside an enum payload (`Option<&mut T>`: the payload's type is \
                     its pointee's, so the emitted variant takes a value)"
                        .to_string(),
                    span,
                );
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
            let receiver = self.place_argument(&function_call.argument_ids, 0, depth)?;
            let item = self.value_argument(&function_call.argument_ids, 1, depth)?;
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
            let what = format!(
                "the host binding `{name}`{}",
                match binding {
                    Some(ExternBinding::Function { symbol, .. }) => format!(" (`{symbol}`)"),
                    _ => String::new(),
                }
            );
            return self.host_gap(what, span);
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
        let name = self.ensure_function(target, &substitution)?.name;
        let arguments = self.call_arguments(target, &function_call.argument_ids, depth)?;
        Ok(format!("{name}({})", arguments.join(", ")))
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
    fn call_arguments(
        &mut self,
        target: Id,
        argument_ids: &[Id],
        depth: usize,
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
        let mut rendered = Vec::new();
        for (index, argument) in argument_ids.iter().enumerate() {
            let wants_a_place = !matches!(conventions.get(index), None | Some(Receiving::ByValue));
            let expecting = declared
                .get(index)
                .map(|parameter| self.concrete(parameter.type_id));
            let mut text = if wants_a_place {
                self.expression(*argument, depth)?
            } else {
                self.value_of_expecting(*argument, expecting, depth)?
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
            rendered.push(match conventions.get(index) {
                Some(Receiving::Ref) if !already_a_reference => format!("&{text}"),
                Some(Receiving::RefMut) if !already_a_reference => format!("&mut {text}"),
                _ => text,
            });
        }
        Ok(rendered)
    }

    /// Every argument as a VALUE — a closure call and a variant constructor,
    /// both of which consume what they are handed.
    fn value_arguments(&mut self, argument_ids: &[Id], depth: usize) -> Result<Vec<String>, Error> {
        let mut rendered = Vec::new();
        for argument in argument_ids {
            rendered.push(self.value_of(*argument, depth)?);
        }
        Ok(rendered)
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
    fn resolve_dispatch(
        &mut self,
        type_id: TypeId,
        member: &str,
        own_generic_values: &[TypeId],
        preferred_trait: Option<(Id, Vec<TypeId>)>,
        span: Span,
    ) -> Result<Option<NativeDispatch>, Error> {
        let type_id = self.concrete(type_id);
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
                    .dispatch_to_member(selected, type_id, own_generic_values, span)
                    .map(Some);
            }
            if let Some(default_id) = self.trait_default_member(trait_id, member) {
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
                .dispatch_to_member(selected, type_id, own_generic_values, span)
                .map(Some);
        }
        let Some(default_id) = self.resolve_inherited_default(type_id, member) else {
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

    /// Whether a receiver is a shape an impl can be written for — the nominal
    /// ones plus the two structural ones (spec §5.7). `select_member` admits
    /// impl SUBJECTS of every shape past this; the guard is about the RECEIVER,
    /// so a blanket subject cannot "apply" to something still abstract.
    fn selectable_receiver(&self, type_id: TypeId) -> bool {
        matches!(
            self.program.type_id_to_type_map.get(&type_id),
            Some(Type::Struct(..) | Type::Enum(..) | Type::Tuple(..) | Type::Array(..))
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
        span: Span,
    ) -> Result<NativeDispatch, Error> {
        let member_id = selected.member_id;
        if let Some(intrinsic) = self.program.intrinsics.get(&member_id).copied() {
            return Ok(NativeDispatch::Intrinsic(intrinsic));
        }
        if let Some(external) = self.program.external_functions.get(&member_id) {
            let what = format!("the host binding `{}`", external.name);
            return self
                .host_gap(what, span)
                .map(|_| NativeDispatch::Call("unimplemented!()".to_string()));
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
            NativeDispatch::Call(name) => {
                // The callee is known by id only inside `dispatch_to_member`;
                // what is known here is the NAME. The conventions come from the
                // member the name was minted for, so the lookup is by the
                // instance's own record.
                let target = self.instance_target(&name);
                let arguments = match target {
                    Some(target) => self.call_arguments(target, argument_ids, depth)?,
                    None => self.value_arguments(argument_ids, depth)?,
                };
                Ok(format!("{name}({})", arguments.join(", ")))
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

    /// `member` as an INHERITED trait default on a concrete type — a member no
    /// impl declares but a (super)trait it implements provides with a body.
    fn resolve_inherited_default(&self, type_id: TypeId, member: &str) -> Option<Id> {
        impl_select::applying_trait_ids(self.program, None, type_id)
            .into_iter()
            .find_map(|trait_id| self.trait_default_member(trait_id, member))
    }

    /// A trait and its supertraits, searched for a member WITH a body.
    fn trait_default_member(&self, trait_id: Id, member: &str) -> Option<Id> {
        let mut stack = vec![trait_id];
        let mut seen = HashSet::new();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(trait_) = self.program.traits.get(&id) else {
                continue;
            };
            if let Some(&member_id) = trait_.declarations.get(member)
                && self.function_has_body(member_id)
            {
                return Some(member_id);
            }
            for supertrait_type_id in &trait_.supertraits {
                if let Some(Type::Trait(super_id, _)) =
                    self.program.type_id_to_type_map.get(supertrait_type_id)
                {
                    stack.push(*super_id);
                }
            }
        }
        None
    }

    fn function_has_body(&self, member_id: Id) -> bool {
        match self.program.entity_map.get(&member_id) {
            Some(Expr::Function(function_id)) => self
                .program
                .functions
                .get(function_id)
                .is_some_and(|function| function.has_body),
            _ => false,
        }
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
        let mut arguments = Vec::new();
        for (index, argument) in argument_ids.iter().enumerate() {
            arguments.push(if index == 0 {
                self.expression(*argument, depth)?
            } else {
                self.value_of(*argument, depth)?
            });
        }
        self.intrinsic(intrinsic, arguments, span)
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
            Intrinsic::ListInsert => format!(
                "vilan_rt::list_insert(&mut {}, ({}) as i64, {})",
                next(),
                next(),
                next()
            ),
            Intrinsic::SharedNew => format!("vilan_rt::Shared::new({})", next()),
            Intrinsic::SharedClone => format!("({}).clone()", next()),
            Intrinsic::SharedValue => format!("({}).get()", next()),
            Intrinsic::SharedWrite => format!("({}).set({})", next(), next()),
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
        "f32" => "f32",
        "f64" => "f64",
        "str" => "vilan_rt::Str",
        _ => return None,
    })
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

/// Whether a rendered Rust type is a counted closure (F16: every closure type
/// is `Rc<dyn Fn(..) -> ..>`), which is neither `PartialEq` nor printable.
fn is_closure_type(rendered: &str) -> bool {
    rendered.contains("dyn Fn")
}

fn is_integer_type(rendered: &str) -> bool {
    matches!(
        rendered,
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64"
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

/// The VALUE of a vilan string literal's body — `transformer::unescape_string`,
/// which is where a literal's value is BUILT on the JS side.
fn unescape_string_value(raw: &str) -> String {
    let raw = vilan_core::util::normalize_newlines(raw);
    let mut result = String::with_capacity(raw.len());
    let mut characters = raw.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some('0') => result.push('\0'),
            // An unknown escape keeps both characters.
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    result
}

fn describe(resolved: &Type) -> String {
    match resolved {
        Type::Any => "any".to_string(),
        Type::Never => "never".to_string(),
        Type::Mapped(_, _, _) => "a mapped tuple".to_string(),
        Type::Trait(_, _) => "a trait object".to_string(),
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
pub fn cargo_manifest(name: &str, runtime_path: &str) -> String {
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
         \n\
         [profile.release]\n\
         panic = \"unwind\"\n\
         \n\
         [workspace]\n"
    )
}
