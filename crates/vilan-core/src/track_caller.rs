//! `[track_caller]` (debugging.md S0, Q11): a function that reports its
//! CALLER's location.
//!
//! A function marked `[track_caller]` takes a hidden trailing parameter of type
//! `std::debug::Location`. This pass, run once over the analyzed program right
//! after the context pass (whose hidden parameters it mirrors, `context.rs`'s
//! `apply`), does the whole of the threading:
//!
//!   1. mints the hidden parameter on every tracking function with a body;
//!   2. appends a location ARGUMENT at every static call of a tracking
//!      function (or external): the enclosing function's own hidden parameter
//!      when the call sits in a tracking function's own body — so `assert`
//!      reports where `assert` was called, not the `panic` inside it — and
//!      otherwise an [`Expr::CallerLocation`] naming the call site;
//!   3. records, for each `xs[i]` in a tracking function's own body, a read
//!      of the parameter its bounds panic reports (every other subscript
//!      reports its own site, which the emitters ask
//!      [`Program::site_location`] for);
//!   4. refuses a tracking function taken as a VALUE: a call through a value
//!      cannot know to pass a location, and silently passing nothing would be
//!      a wrong answer.
//!
//! **A trait member (B646).** `[track_caller]` declared on a trait's member is
//! part of the trait's contract: every implementation of the member takes the
//! hidden location whether or not it writes the attribute
//! ([`imply_trait_tracking`]), the trait's declaration takes it too (so a
//! `dyn` table's slot is typed with it on both backends), and every call that
//! names the member passes one — a static call, a call through a bound (the
//! analyzer resolved it to the trait's declaration, and emission re-dispatches
//! it to the implementation with the argument list as threaded here) and a
//! call through a `dyn` table. An implementation that marks a member its trait
//! does not is refused: a call through the trait would pass nothing.
//!
//! A closure is a boundary, as in Rust: a closure written inside a tracking
//! function is not itself tracking, so a panic inside it reports the closure's
//! own site.
//!
//! The two compiler-lowered tracking externals are `std::io::panic`, whose
//! location rides into the panic, and `std::debug::caller`, which lowers to its
//! location argument. Neither emitter needs any other knowledge of the
//! attribute: a hidden parameter is an ordinary `Location`-typed parameter
//! and a location argument an ordinary value.
//!
//! The pass is a no-op for a program with no tracking function, but std's own
//! `panic`, `assert`, `unwrap` and `expect` are tracking, so in practice it
//! always runs; it visits only the calls the graph already holds.

use std::path::{Component, Path};

use crate::analyzer::{Expr, ExprIfBranch, Implementation, Parameter, Program, SourceId};
use crate::call_graph::{Call, CallGraph, CallTarget, IndirectReason};
use crate::error::Error;
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::node::Convention;
use crate::span::Span;
use crate::type_::Type;

/// The hidden parameter's name. Neither backend can collide on it: both
/// suffix a binding's emitted name with its id.
const HIDDEN_PARAMETER_NAME: &str = "caller";

/// Thread `[track_caller]` locations through `program` (see the module docs).
pub fn thread_locations(program: &mut Program, graph: &CallGraph) {
    let Some(location_type) = program
        .caller_fn_id
        .and_then(|caller| program.external_functions.get(&caller))
        .map(|caller| caller.return_type_id)
    else {
        // `debug.vl` did not load (a program analyzed without std): there is
        // no `Location` type to give the parameter, and nothing tracks.
        return;
    };
    imply_trait_tracking(program);
    let tracking: HashSet<Id> = program
        .functions
        .iter()
        .filter(|(_, function)| function.track_caller)
        .map(|(id, _)| *id)
        .chain(
            program
                .external_functions
                .iter()
                .filter(|(_, external)| external.track_caller)
                .map(|(id, _)| *id),
        )
        .collect();
    if tracking.is_empty() {
        return;
    }
    let mut next_id = program.next_entity_id;
    let mut fresh = || {
        let id = Id(next_id);
        next_id += 1;
        id
    };

    // 1. The hidden parameters, in the program's own function order so the
    //    minted ids are deterministic. A trait's signature-only member takes
    //    one too (B646): it has no body to read it, but its parameter list is
    //    what a `dyn` table's slot is typed and sized by.
    let tracking_functions: Vec<(Id, bool)> = program
        .functions
        .iter()
        .filter(|(_, function)| function.track_caller)
        .map(|(id, function)| (*id, function.has_body))
        .collect();
    for (function_id, has_body) in tracking_functions {
        let parameter = fresh();
        program.parameters.insert(
            parameter,
            Parameter {
                id: parameter,
                function_id,
                name: HIDDEN_PARAMETER_NAME,
                type_id: location_type,
                convention: Convention::Bare,
                mutable: false,
                spread: false,
                lazy: false,
            },
        );
        program
            .entity_map
            .insert(parameter, Expr::Parameter(parameter));
        program.expr_type_ids.insert(parameter, location_type);
        if let Some(function) = program.functions.get_mut(&function_id) {
            function.parameters.push(parameter);
        }
        if has_body {
            program
                .track_caller_parameters
                .insert(function_id, parameter);
        }
    }

    // 2. The location argument at every call that names a tracking function:
    //    a static call, and (B646) a call through a bound or a `dyn` table,
    //    which the analyzer resolved to the trait's declaration — emission
    //    re-dispatches it with the argument list threaded here. A node's own
    //    hidden parameter forwards; anything else names the site. A call with
    //    no argument list — a `for` loop's `next` — cannot carry one.
    let mut threaded: Vec<(Id, Option<Id>)> = Vec::new();
    let mut argumentless: Vec<(Id, Id)> = Vec::new();
    let collect = |calls: &[Call],
                   forward: Option<Id>,
                   into: &mut Vec<(Id, Option<Id>)>,
                   argumentless: &mut Vec<(Id, Id)>| {
        for call in calls {
            let target = match call.target {
                CallTarget::Function(target) | CallTarget::External(target) => target,
                CallTarget::Indirect(
                    IndirectReason::GenericMember | IndirectReason::TraitDispatch,
                ) => match dispatched_declaration(program, call.call_id) {
                    Some(target) => target,
                    None => continue,
                },
                CallTarget::Closure(_)
                | CallTarget::Variant(_)
                | CallTarget::Indirect(IndirectReason::Value) => continue,
            };
            if !tracking.contains(&target) {
                continue;
            }
            if program.function_calls.contains_key(&call.call_id) {
                into.push((call.call_id, forward));
            } else {
                argumentless.push((call.call_id, target));
            }
        }
    };
    for node in graph.nodes() {
        let forward = program.track_caller_parameters.get(&node.id()).copied();
        collect(
            graph.calls_of(node.id()),
            forward,
            &mut threaded,
            &mut argumentless,
        );
    }
    let bindings = program.module_level_bindings();
    for &binding in &bindings {
        collect(
            graph.initializer_calls_of(binding),
            None,
            &mut threaded,
            &mut argumentless,
        );
        // A `const` initializer runs in the compile-time interpreter, which
        // calls the same functions with the same arity.
        collect(
            graph.const_initializer_calls_of(binding),
            None,
            &mut threaded,
            &mut argumentless,
        );
    }
    for &region in graph.const_regions() {
        collect(
            graph.const_initializer_calls_of(region),
            None,
            &mut threaded,
            &mut argumentless,
        );
    }
    let mut seen: HashSet<Id> = HashSet::default();
    for (call_id, forward) in threaded {
        // A call reachable from two collections (a const region inside a
        // binding's initializer) is threaded once.
        if !seen.insert(call_id) {
            continue;
        }
        let argument = fresh();
        match forward {
            Some(parameter) => {
                program.entity_map.insert(argument, Expr::Local(parameter));
            }
            None => {
                program
                    .entity_map
                    .insert(argument, Expr::CallerLocation(call_id));
            }
        }
        program.expr_type_ids.insert(argument, location_type);
        if let Some(call) = program.function_calls.get_mut(&call_id) {
            call.argument_ids.push(argument);
        }
    }

    // 3. Subscripts in a tracking function's own body forward its location.
    let forwarding: Vec<(Id, Id)> = program
        .track_caller_parameters
        .iter()
        .map(|(function, parameter)| (*function, *parameter))
        .collect();
    for (function_id, parameter) in forwarding {
        let Some(function) = program.functions.get(&function_id) else {
            continue;
        };
        let mut pending: Vec<Id> = function.body.0.clone();
        pending.push(function.body.1);
        let mut subscripts = Vec::new();
        while let Some(id) = pending.pop() {
            if matches!(program.entity_map.get(&id), Some(Expr::Index(_, _))) {
                subscripts.push(id);
            }
            own_body_children(program, id, &mut pending);
        }
        for subscript in subscripts {
            let argument = fresh();
            program.entity_map.insert(argument, Expr::Local(parameter));
            program.expr_type_ids.insert(argument, location_type);
            program.index_location_arguments.insert(subscript, argument);
        }
    }

    // 4. A tracking function taken as a value.
    let mut refusals: Vec<(Id, Id)> = Vec::new();
    for node in graph.nodes() {
        for &(reference, function) in graph.function_references_of(node.id()) {
            if tracking.contains(&function) {
                refusals.push((reference, function));
            }
        }
    }
    for &binding in &bindings {
        for &(reference, function) in graph.function_references_of(binding) {
            if tracking.contains(&function) {
                refusals.push((reference, function));
            }
        }
    }
    for (reference, function) in refusals {
        let name = program
            .functions
            .get(&function)
            .map(|function| function.name)
            .or_else(|| {
                program
                    .external_functions
                    .get(&function)
                    .map(|external| external.name)
            })
            .unwrap_or("this function");
        // A member with a receiver (B646: a trait's tracking method taken as
        // a value) is called on the closure's argument.
        let receives = program
            .functions
            .get(&function)
            .and_then(|function| function.parameters.first())
            .and_then(|parameter| program.parameters.get(parameter))
            .is_some_and(|parameter| parameter.name == "self");
        let steer = if receives {
            format!("`|value| value.{name}(..)`")
        } else {
            format!("`|value| {name}(value)`")
        };
        let span = program
            .span_map
            .get(&reference)
            .map(|span| **span)
            .unwrap_or(Span { start: 0, end: 0 });
        let source = program.source_of(reference).unwrap_or(SourceId(0));
        program.push_diagnostic(
            Error {
                trace: Vec::new(),
                note: None,
                span,
                msg: format!(
                    "`{name}` is `[track_caller]`: it takes its caller's location as a hidden \
                     argument, and a call through a value cannot pass one. Call it inside a \
                     closure instead: {steer}"
                ),
            },
            source,
        );
    }

    // A `for` loop calls its iterator's `next` through the protocol, with no
    // argument list to append a location to.
    for (loop_id, target) in argumentless {
        let name = program
            .functions
            .get(&target)
            .map(|function| function.name)
            .unwrap_or("next");
        // At the iterable: the loop's protocol call is made on it.
        let anchor = match program.entity_map.get(&loop_id) {
            Some(Expr::ForEach(iterable, _, _)) => *iterable,
            _ => loop_id,
        };
        let span = program
            .span_map
            .get(&anchor)
            .map(|span| **span)
            .unwrap_or(Span { start: 0, end: 0 });
        let source = program.source_of(loop_id).unwrap_or(SourceId(0));
        program.push_diagnostic(
            Error {
                trace: Vec::new(),
                note: None,
                span,
                msg: format!(
                    "`{name}` is `[track_caller]`, and a `for .. in` loop calls it through the \
                     iterator protocol, which cannot pass its caller's location. Call `{name}` \
                     yourself inside the loop, or drop `[track_caller]` from it"
                ),
            },
            source,
        );
    }

    program.next_entity_id = next_id;
}

/// B646: `[track_caller]` declared on a trait's member makes tracking part of
/// the trait's contract, so every implementation of that member tracks —
/// marked or not — and the flag is set on it here, where each
/// implementation's trait is known. An implementation that marks a member its
/// trait does NOT declare tracking is refused: a call through the trait (a
/// bound, a `dyn` table) passes no location, so the mark would promise a site
/// it cannot report.
pub fn imply_trait_tracking(program: &mut Program) {
    let mut implied: Vec<Id> = Vec::new();
    let mut refused: Vec<(Id, &str, Id)> = Vec::new();
    for implementation in &program.implementations {
        if implementation.trait_ids.is_empty() {
            continue;
        }
        for &(name, member) in &implementation.declared_members {
            let Some(function) = program.functions.get(&member) else {
                continue;
            };
            let Some(declaration) = home_declaration(program, implementation, name) else {
                continue;
            };
            let declared = program
                .functions
                .get(&declaration)
                .is_some_and(|declaration| declaration.track_caller);
            if declared {
                if !function.track_caller {
                    implied.push(member);
                }
            } else if function.track_caller {
                refused.push((member, name, declaration));
            }
        }
    }
    for member in implied {
        if let Some(function) = program.functions.get_mut(&member) {
            function.track_caller = true;
        }
    }
    for (member, name, declaration) in refused {
        let trait_name = program
            .traits
            .values()
            .find(|trait_| trait_.declarations.get(name) == Some(&declaration))
            .map(|trait_| trait_.name)
            .unwrap_or("its trait");
        let span = program
            .functions
            .get(&member)
            .map(|function| function.name_span)
            .unwrap_or(Span { start: 0, end: 0 });
        let source = program.source_of(member).unwrap_or(SourceId(0));
        program.push_diagnostic(
            Error {
                trace: Vec::new(),
                note: None,
                span,
                msg: format!(
                    "`[track_caller]` on `{name}`, which `{trait_name}` does not declare \
                     tracking: a call through `{trait_name}` (a bound, a `dyn`) would pass no \
                     location. Declare it on the trait's member (`[track_caller]` above \
                     `{trait_name}`'s `fun {name}`), where every implementation takes it"
                ),
            },
            source,
        );
    }
}

/// The trait member `name` implements in `implementation`: the declaration in
/// the first of its traits — or their supertraits, nearest first — that
/// declares the name.
fn home_declaration(program: &Program, implementation: &Implementation, name: &str) -> Option<Id> {
    let mut queue: Vec<Id> = implementation.trait_ids.clone();
    let mut seen: HashSet<Id> = HashSet::default();
    let mut next = 0;
    while let Some(&trait_id) = queue.get(next) {
        next += 1;
        if !seen.insert(trait_id) {
            continue;
        }
        let Some(trait_) = program.traits.get(&trait_id) else {
            continue;
        };
        if let Some(&member) = trait_.declarations.get(name) {
            return Some(member);
        }
        for supertrait in &trait_.supertraits {
            if let Some(Type::Trait(supertrait_id, _)) = program.type_id_to_type_map.get(supertrait)
            {
                queue.push(*supertrait_id);
            }
        }
    }
    None
}

/// The trait member a dispatched call names (B646): the analyzer resolved a
/// call through a bound or a `dyn` table to the member's DECLARATION, which
/// stays the call's subject while emission picks the implementation.
fn dispatched_declaration(program: &Program, call_id: Id) -> Option<Id> {
    let subject = match program.function_calls.get(&call_id) {
        Some(call) => call.subject_id,
        // A `for` loop's protocol call: the loop is the call.
        None => return program.for_each_next.get(&call_id).copied(),
    };
    match program.entity_map.get(&subject) {
        Some(Expr::Local(member)) if program.functions.contains_key(member) => Some(*member),
        _ => None,
    }
}

/// debugging.md §3.3 (Q4): every `dbg(..)` in a build whose policy is
/// [`DbgPolicy::Refuse`] — the release preset's default — is an error at the
/// call. A browser release prints to every user's console, and a silent
/// strip is how a debugging line survives for a year.
pub fn refuse_release_dbg(program: &mut Program, options: &crate::options::BuildOptions) {
    if options.dbg != crate::options::DbgPolicy::Refuse || program.dbg_calls.is_empty() {
        return;
    }
    for call_id in program.dbg_calls.clone() {
        let span = program
            .span_map
            .get(&call_id)
            .map(|span| **span)
            .unwrap_or(Span { start: 0, end: 0 });
        let source = program.source_of(call_id).unwrap_or(SourceId(0));
        program.push_diagnostic(
            Error {
                trace: Vec::new(),
                note: None,
                span,
                msg: "`dbg` left in a release build: a release build refuses it, so a \
                      debugging line cannot ship by accident. Remove the call and keep its \
                      argument, or set `[build] dbg = \"strip\"` (print nothing) or \
                      `\"keep\"` (print in release too) in `vilan.toml`"
                    .to_string(),
            },
            source,
        );
    }
    program.normalize_diagnostic_order();
}

/// The expressions one step below `id` that still belong to the SAME function
/// body: a closure, an `async` block and a nested declaration are other
/// bodies, and a tracking function's location does not reach into them.
fn own_body_children(program: &Program, id: Id, out: &mut Vec<Id>) {
    let Some(expr) = program.entity_map.get(&id) else {
        return;
    };
    match expr {
        Expr::Assignment(target, value) => out.extend([*target, *value]),
        Expr::Await(inner)
        | Expr::TryAssert(inner)
        | Expr::Ascribe(inner)
        | Expr::Unary(_, inner)
        | Expr::Reference(inner, _)
        | Expr::Dereference(inner)
        | Expr::FunctionReturn(Some(inner))
        | Expr::Repeat(inner, _)
        | Expr::ArrayLen(inner, _)
        | Expr::Field(inner, _, _)
        | Expr::TupleIndex(inner, _, _)
        | Expr::Destructure(inner, _)
        | Expr::Is(inner, _) => out.push(*inner),
        Expr::Binary(_, left, right) | Expr::Index(left, right) => out.extend([*left, *right]),
        Expr::Block((statements, tail)) => {
            out.extend(statements.iter().copied());
            out.push(*tail);
        }
        Expr::Call(call_id) => {
            if let Some(call) = program.function_calls.get(call_id) {
                out.push(call.subject_id);
                out.extend(call.argument_ids.iter().copied());
            }
        }
        Expr::TupleComprehension(bindings, body) => {
            out.extend(bindings.iter().map(|(_, source)| *source));
            out.push(*body);
        }
        Expr::For(condition, (statements, tail)) => {
            out.extend(condition.iter().copied());
            out.extend(statements.iter().copied());
            out.push(*tail);
        }
        Expr::ForEach(iterable, _, (statements, tail)) => {
            out.push(*iterable);
            out.extend(statements.iter().copied());
            out.push(*tail);
        }
        Expr::Lift(subject, _, continuation) => out.extend([*subject, *continuation]),
        Expr::LiftRegion(steps, body) => {
            out.extend(steps.iter().map(|(step, _, _)| *step));
            out.push(*body);
        }
        Expr::If(branch) => {
            let mut current = branch;
            loop {
                match current {
                    ExprIfBranch::If(condition, (statements, tail), otherwise) => {
                        out.push(*condition);
                        out.extend(statements.iter().copied());
                        out.push(*tail);
                        match otherwise {
                            Some(next) => current = next,
                            None => break,
                        }
                    }
                    ExprIfBranch::Else((statements, tail)) => {
                        out.extend(statements.iter().copied());
                        out.push(*tail);
                        break;
                    }
                }
            }
        }
        Expr::List(ids) | Expr::Tuple(ids) => out.extend(ids.iter().copied()),
        Expr::Match(subject, legs) => {
            out.push(*subject);
            for leg in legs {
                out.extend(leg.guard);
                out.push(leg.body);
            }
        }
        Expr::StructInitializer(_, fields) => out.extend(fields.values().copied()),
        Expr::Variable(variable) => {
            if let Some(initial) = program
                .variables
                .get(variable)
                .and_then(|variable| variable.initial)
            {
                out.push(initial);
            }
        }
        Expr::Closure(_)
        | Expr::Async(_)
        | Expr::FunctionReturn(None)
        | Expr::Bool(_)
        | Expr::Number(_, _, _)
        | Expr::LengthValue(_)
        | Expr::String(_)
        | Expr::MultilineString(_)
        | Expr::Null
        | Expr::Void
        | Expr::Error
        | Expr::Jump(_)
        | Expr::LiftBinder
        | Expr::EnumVariant(_, _)
        | Expr::Generic(_)
        | Expr::Struct(_)
        | Expr::Enum(_)
        | Expr::Impl(_)
        | Expr::Function(_)
        | Expr::Module(_)
        | Expr::Trait(_)
        | Expr::Macro
        | Expr::Local(_)
        | Expr::Parameter(_)
        | Expr::ExternalFunction(_)
        | Expr::CallerLocation(_) => {}
    }
}

/// What [`Program::site_location`] reads: each source's display path and
/// line starts, computed once per program on first ask.
#[derive(Debug, Default)]
pub struct SiteLocator {
    package_root: std::path::PathBuf,
    /// Per source, built on its first location: a program names sites in a
    /// handful of files, and scanning every std source for its line starts
    /// was a fixed cost on every build.
    paths: Vec<std::sync::OnceLock<String>>,
    line_starts: Vec<std::sync::OnceLock<Vec<usize>>>,
    texts: HashMap<SourceId, usize>,
}

impl SiteLocator {
    fn build(program: &Program) -> SiteLocator {
        // The PACKAGE root is the directory its `vilan.toml` sits in — the
        // source root (`pkg_root`, `src/` by default) is below it — so a
        // location reads `src/main.vl:12:5`. A file built with no manifest
        // above it is its own package, rooted where it sits.
        let source_root = crate::util::canonical_path(&program.pkg_root);
        let package_root = source_root
            .ancestors()
            .find(|directory| directory.join("vilan.toml").is_file())
            .map(Path::to_path_buf)
            .unwrap_or(source_root);
        let texts = program
            .source_texts
            .iter()
            .enumerate()
            .map(|(index, (source, _))| (*source, index))
            .collect();
        SiteLocator {
            package_root,
            paths: (0..program.sources.len())
                .map(|_| std::sync::OnceLock::new())
                .collect(),
            line_starts: (0..program.source_texts.len())
                .map(|_| std::sync::OnceLock::new())
                .collect(),
            texts,
        }
    }

    fn path(&self, program: &Program, source: SourceId) -> &str {
        let index = source.0 as usize;
        let Some(cell) = self.paths.get(index) else {
            return "";
        };
        cell.get_or_init(|| {
            let path = program
                .canonical_sources
                .get(index)
                .unwrap_or(&program.sources[index]);
            display_path(
                path,
                &self.package_root,
                program.std_sources.contains(&source),
            )
        })
    }

    fn line_starts(&self, text_index: usize, text: &str) -> &[usize] {
        self.line_starts[text_index].get_or_init(|| {
            let mut starts = vec![0];
            starts.extend(
                text.bytes()
                    .enumerate()
                    .filter(|(_, byte)| *byte == b'\n')
                    .map(|(at, _)| at + 1),
            );
            starts
        })
    }
}

/// A source's path as a location prints it: relative to the entry package's
/// root (`src/views.vl`), std's own as `std/src/..`, and a dependency's as
/// `<package>/src/..`. Always with `/`, so a location reads the same on every
/// host and a golden that holds one is portable.
fn display_path(path: &Path, package_root: &Path, is_std: bool) -> String {
    let joined = |components: &[Component<'_>]| {
        components
            .iter()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/")
    };
    let components: Vec<Component<'_>> = path.components().collect();
    // The LAST `<name>/src` pair: std's own (`std/src/..`, wherever the cache
    // or the checkout keeps it) and a dependency's (`<package>/src/..`).
    let package_relative = || {
        (1..components.len().saturating_sub(1))
            .rev()
            .find(|&at| components[at].as_os_str() == "src")
            .map(|at| joined(&components[at - 1..]))
    };
    if is_std && let Some(relative) = package_relative() {
        return relative;
    }
    if let Ok(relative) = path.strip_prefix(package_root) {
        let relative: Vec<Component<'_>> = relative.components().collect();
        if !relative.is_empty() {
            return joined(&relative);
        }
    }
    if let Some(relative) = package_relative() {
        return relative;
    }
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Program<'_> {
    /// The `file:line:column` of the site `anchor` names (debugging.md S0):
    /// the file relative to its package root, a 1-based line, a 1-based
    /// column in characters. A call anchors at its callee's NAME (`unwrap`
    /// in `rows.first().unwrap()`), as Rust's `#[track_caller]` does, so a
    /// chain's links read apart; any other anchor at its own start.
    pub fn site_location(&self, anchor: Id) -> String {
        let locator = self.site_locator.get_or_init(|| SiteLocator::build(self));
        let source = self.source_of(anchor).unwrap_or(SourceId(0));
        let path = locator.path(self, source);
        let Some((text_index, offset)) = locator.texts.get(&source).copied().and_then(|index| {
            let (_, text) = self.source_texts[index];
            let offset = self.site_offset(anchor, text)?;
            Some((index, offset.min(text.len())))
        }) else {
            return format!("{path}:1:1");
        };
        let (_, text) = self.source_texts[text_index];
        let starts = locator.line_starts(text_index, text);
        let line = starts.partition_point(|&start| start <= offset).max(1);
        let line_start = starts[line - 1];
        let column = text
            .get(line_start..offset)
            .map(|prefix| prefix.chars().count())
            .unwrap_or(0)
            + 1;
        format!("{path}:{line}:{column}")
    }

    /// The source text of `id`'s span, whitespace runs collapsed to one
    /// space — how `dbg` prints the expression it was handed (§3.2).
    pub fn source_text_of(&self, id: Id) -> String {
        let source = self.source_of(id).unwrap_or(SourceId(0));
        let Some(text) = self
            .source_texts
            .iter()
            .find(|(candidate, _)| *candidate == source)
            .map(|(_, text)| *text)
        else {
            return String::new();
        };
        let Some(written) = self
            .span_map
            .get(&id)
            .and_then(|span| text.get(span.start..span.end))
        else {
            return String::new();
        };
        written.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The byte offset [`Self::site_location`] reports for `anchor`.
    pub(crate) fn site_offset(&self, anchor: Id, text: &str) -> Option<usize> {
        let span = **self.span_map.get(&anchor)?;
        if let Some(call) = self.function_calls.get(&anchor) {
            // The callee's name is the identifier that ends where the
            // argument list opens (generic arguments aside).
            let open = call.arguments_span.start;
            let before = text.get(span.start.min(open)..open)?;
            let name_length = before
                .chars()
                .rev()
                .take_while(|character| character.is_alphanumeric() || *character == '_')
                .map(char::len_utf8)
                .sum::<usize>();
            if name_length > 0 {
                return Some(open - name_length);
            }
        }
        Some(span.start)
    }
}
