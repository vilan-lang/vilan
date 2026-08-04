//! The `const` pass (proposal/const-eval.md): evaluates `const`-marked
//! expressions post-analysis with the macro interpreter, in dependency order,
//! producing plain-data results the transformer serializes in place — plus
//! spanned diagnostics for everything that cannot evaluate. Free variables of
//! a const expression must be compile-time-known: an item (function, struct,
//! enum), or an immutable binding whose initializer is a literal or another
//! `const` expression.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::analyzer::{Expr, Program, SourceId};
use crate::call_graph::{Call, CallGraph, CallTarget, Node};
use crate::error::Error;
use crate::id::Id;
use crate::interpreter::{self, ConstValue, Limits};
use crate::options::BuildOptions;
use crate::span::Span;
use crate::transformer;

pub fn evaluate(
    program: &Program,
    options: &BuildOptions,
) -> (
    HashMap<Id, ConstValue>,
    Vec<(String, String)>,
    // Each failure with the file its span indexes into (backlog E16): the pass
    // walks the whole program, so a `const` in a module reports in that module.
    Vec<(Error, SourceId)>,
) {
    // A program that already failed analysis skips evaluation entirely: the
    // transformer's entity lookups (used to build const mini-programs) assume
    // a clean program, exactly as `transform` itself does.
    if !program.diagnostics.is_empty() {
        return (HashMap::new(), Vec::new(), Vec::new());
    }
    let mut state = State {
        program,
        options,
        const_set: program.const_exprs.iter().copied().collect(),
        results: HashMap::new(),
        assets: Vec::new(),
        failed: HashSet::new(),
        in_progress: HashSet::new(),
        errors: Vec::new(),
    };
    state.check_const_only();
    for &expr_id in &program.const_exprs {
        state.evaluate_one(expr_id);
    }
    (state.results, state.assets, state.errors)
}

/// Deduplicates and deterministically orders the collected `(kind, line)`
/// pairs into per-kind file contents (newline-terminated). Lines sort
/// lexically — which is SOUND for the CSS the styling system emits: `.class`
/// rules ('.' = 0x2E) sort before `:root` variables and `@media` blocks
/// ('@' = 0x40), so media rules take the later cascade position they need,
/// and pseudo-class rules don't compete with base rules on cascade order at
/// all (their classes are distinct and their specificity is higher) — EXCEPT
/// among `@media (min-width: …)` lines themselves, which sort by ascending
/// min-width, not by digit bytes. On a wide viewport every narrower
/// `min-width` rule also matches, specificity ties, and cascade order
/// decides — so the widest matching breakpoint must come last for a
/// mobile-first `.sm(x).lg(y)` chain to render `y`. The lexical digit sort
/// put `1024px` before `640px` and the narrow rule won (B35).
pub fn assemble_assets(assets: &[(String, String)]) -> BTreeMap<String, String> {
    let mut by_kind: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (kind, line) in assets {
        by_kind.entry(kind).or_default().insert(line);
    }
    by_kind
        .into_iter()
        .map(|(kind, lines)| {
            let mut lines = lines.into_iter().collect::<Vec<_>>();
            // Media lines as a group sort after everything else ('@' is the
            // highest first byte the styling system emits) — the key only has
            // to order them among themselves and keep the rest lexical.
            lines.sort_by_key(|line| (media_min_width(line).map(f64::to_bits), *line));
            let mut content = lines.join("\n");
            content.push('\n');
            (kind.to_string(), content)
        })
        .collect()
}

/// The numeric minimum width of an `@media (min-width: …)` line, in px —
/// `em`/`rem` normalized at the CSS-initial 16px — or `None` for a non-media
/// line, or a width in units the styling system doesn't emit (those keep
/// their lexical position). `f64::to_bits` in the sort key above is
/// order-preserving because widths are non-negative.
fn media_min_width(line: &str) -> Option<f64> {
    let rest = line.strip_prefix("@media (min-width: ")?;
    let close = rest.find(')')?;
    let number_end = rest
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(close);
    let number: f64 = rest[..number_end].parse().ok()?;
    match &rest[number_end..close] {
        "px" => Some(number),
        "em" | "rem" => Some(number * 16.0),
        _ => None,
    }
}

struct State<'p, 'src> {
    program: &'p Program<'src>,
    options: &'p BuildOptions,
    const_set: HashSet<Id>,
    results: HashMap<Id, ConstValue>,
    assets: Vec<(String, String)>,
    failed: HashSet<Id>,
    in_progress: HashSet<Id>,
    errors: Vec<(Error, SourceId)>,
}

/// How a const expression's free variable is (or isn't) compile-time-known.
enum Known<'src> {
    /// An item or a literal-initialized immutable binding — usable as-is.
    Ok,
    /// An immutable binding whose initializer is a `const` expression:
    /// evaluate that first.
    Const(Id),
    /// A runtime value — an error at the reference.
    Runtime(&'src str),
}

impl<'p, 'src> State<'p, 'src> {
    fn evaluate_one(&mut self, expr_id: Id) -> bool {
        if self.results.contains_key(&expr_id) {
            return true;
        }
        if self.failed.contains(&expr_id) {
            return false;
        }
        if !self.in_progress.insert(expr_id) {
            self.errors.push((
                Error {
                    note: None,
                    span: self.span_of(expr_id),
                    msg: "`const` expressions form a dependency cycle".to_string(),
                },
                self.source_of(expr_id),
            ));
            self.failed.insert(expr_id);
            return false;
        }
        let ok = self.evaluate_inner(expr_id);
        self.in_progress.remove(&expr_id);
        if !ok {
            self.failed.insert(expr_id);
        }
        ok
    }

    fn evaluate_inner(&mut self, expr_id: Id) -> bool {
        // The free-variable rule, with precise spans at each reference.
        let mut ok = true;
        let free = self.free_locals(expr_id);
        let external: HashSet<Id> = free.iter().map(|(_, binding)| *binding).collect();
        for (reference_id, binding) in free {
            match self.classify(binding) {
                Known::Ok => {}
                Known::Const(dependency) => {
                    if !self.evaluate_one(dependency) {
                        ok = false;
                    }
                }
                Known::Runtime(name) => {
                    self.errors.push((
                        Error {
                            note: None,
                            span: self.span_of(reference_id),
                            msg: format!(
                                "`{name}` is a runtime value; a `const` expression reads only \
                                 compile-time-known bindings"
                            ),
                        },
                        self.source_of(reference_id),
                    ));
                    ok = false;
                }
            }
        }
        if !ok {
            return false;
        }

        // Assemble the mini-program. Bindings reached through CALLED functions
        // surface as `unresolved` — const-initialized ones get evaluated and
        // the assembly retried; anything else is a diagnostic.
        let mut attempts = 0;
        loop {
            let (mini, unresolved) = transformer::transform_const_program(
                self.program,
                self.options,
                expr_id,
                &external,
                &self.results,
            );
            let mut retry = false;
            for binding in &unresolved {
                match self.classify(*binding) {
                    Known::Ok => {}
                    Known::Const(dependency) => {
                        if self.evaluate_one(dependency) {
                            retry = true;
                        } else {
                            ok = false;
                        }
                    }
                    Known::Runtime(name) => {
                        self.errors.push((
                            Error {
                                note: None,
                                span: self.span_of(expr_id),
                                msg: format!(
                                    "this `const` expression reaches `{name}`, whose value is not \
                                     compile-time-known"
                                ),
                            },
                            self.source_of(expr_id),
                        ));
                        ok = false;
                    }
                }
            }
            if !ok {
                return false;
            }
            if retry && attempts < 4 {
                attempts += 1;
                continue;
            }
            return match interpreter::eval_const(&mini, Limits::default()) {
                Ok((value, assets)) => {
                    self.results.insert(expr_id, value);
                    self.assets.extend(assets);
                    true
                }
                Err(failure) => {
                    self.errors.push((
                        Error {
                            note: None,
                            span: self.span_of(expr_id),
                            msg: format!("const evaluation failed: {}", failure.message),
                        },
                        self.source_of(expr_id),
                    ));
                    false
                }
            };
        }
    }

    /// The const-only capability check (const-eval.md §2): no RUNTIME call
    /// path may reach `asset::emit`. R = the functions/closures that reach it
    /// through call sites OUTSIDE `const` subtrees; roots (`main`, top-level
    /// initializers) never join R — a root's call into R is the offending
    /// boundary, reported at that call site.
    ///
    /// A call THROUGH a value resolves to `CallTarget::Indirect(Value)`, which
    /// carries no caller edge, so the fixpoint cannot follow it. §2's rule is
    /// therefore a refusal at the point the value is MADE: an R-member
    /// referenced as a function value, or an escaping R closure, outside every
    /// `const` subtree. Without it the escape is silent and the emitted JS
    /// carries a live `__emit_asset` call with no runtime binding.
    fn check_const_only(&mut self) {
        let Some(emit_id) = self.program.asset_emit_fn_id else {
            return;
        };
        let graph = CallGraph::build(self.program);
        let main_id = self
            .program
            .scopes
            .get(&self.program.global_scope_id)
            .and_then(|scope| scope.name_to_id_map.get("main").copied());

        // Seed: nodes calling `emit` directly through a non-const site.
        let mut in_r: HashSet<Id> = HashSet::new();
        let mut worklist: Vec<Id> = Vec::new();
        let mut boundary_errors: Vec<(Id, Id)> = Vec::new(); // (call site, callee)
        let mut owned_calls: HashSet<Id> = HashSet::new();
        for node in graph.nodes() {
            for call in graph.calls_of(node.id()) {
                owned_calls.insert(call.call_id);
                if !matches!(call.target, CallTarget::External(target) if target == emit_id) {
                    continue;
                }
                if self.in_const_subtree(call.call_id) {
                    continue;
                }
                if Some(node.id()) == main_id {
                    boundary_errors.push((call.call_id, emit_id));
                } else if in_r.insert(node.id()) {
                    worklist.push(node.id());
                }
            }
        }
        // Propagate to callers through non-const sites; roots never join.
        while let Some(member) = worklist.pop() {
            for caller in graph.callers_of(member) {
                let caller_id = caller.id();
                if in_r.contains(&caller_id) {
                    continue;
                }
                let sites: Vec<&Call> = graph
                    .calls_of(caller_id)
                    .iter()
                    .filter(|call| match call.target {
                        CallTarget::Function(target) | CallTarget::Closure(target) => {
                            target == member
                        }
                        _ => false,
                    })
                    .collect();
                for site in sites {
                    if self.in_const_subtree(site.call_id) {
                        continue;
                    }
                    if Some(caller_id) == main_id {
                        boundary_errors.push((site.call_id, member));
                    } else if in_r.insert(caller_id) {
                        worklist.push(caller_id);
                    }
                }
            }
        }
        // Top-level initializers own no graph node: a direct-call site outside
        // every node whose subject resolves to `emit` or an R-function is the
        // same boundary.
        for (call_id, function_call) in &self.program.function_calls {
            if owned_calls.contains(call_id) || self.in_const_subtree(*call_id) {
                continue;
            }
            let Some(Expr::Local(target)) = self.program.entity_map.get(&function_call.subject_id)
            else {
                continue;
            };
            if *target == emit_id || in_r.contains(target) {
                boundary_errors.push((*call_id, *target));
            }
        }
        boundary_errors.sort_by_key(|(site, _)| self.span_of(*site).start);
        boundary_errors.dedup();
        for (site, callee) in boundary_errors {
            let name = self.const_only_name(callee, emit_id);
            self.errors.push((
                Error {
                    note: None,
                    span: self.span_of(site),
                    msg: format!(
                        "{name} is compile-time-only; evaluate this call inside a `const` \
                         expression"
                    ),
                },
                self.source_of(site),
            ));
        }

        self.check_value_escapes(&graph, &in_r, emit_id);
    }

    /// The value-escape half of §2's rule. Two shapes make a runtime function
    /// value out of R, and both bypass the call-graph fixpoint:
    ///
    /// - an R-member NAMED as a value (fn-to-closure coercion) — the call graph
    ///   already separates these from call subjects in `function_references`;
    /// - an R closure that is never immediately applied — it joined R through
    ///   its own body, but nothing calls it by identity, so no boundary error
    ///   can fire for it.
    ///
    /// Both are refused at the site the value is made, which is also the
    /// narrowest span that identifies the problem (diagnostics-standard A1).
    /// A reference inside a `const` subtree is untouched: there the interpreter
    /// makes the call, which is the whole styling shape.
    fn check_value_escapes(&mut self, graph: &CallGraph, in_r: &HashSet<Id>, emit_id: Id) {
        let mut escapes: Vec<(Id, Option<Id>)> = Vec::new(); // (site, named function)

        // `function_references` is keyed by every function node, every closure
        // node, and every module-level binding's initializer — the same key set
        // the graph itself walks. A `const`-marked initializer is skipped at
        // build time, so module-level const chains never appear here at all.
        let reference_owners = graph
            .nodes()
            .iter()
            .map(|node| node.id())
            .chain(self.program.module_level_bindings());
        for owner in reference_owners {
            for &(reference_id, function_id) in graph.function_references_of(owner) {
                if !in_r.contains(&function_id) || self.in_const_subtree(reference_id) {
                    continue;
                }
                escapes.push((reference_id, Some(function_id)));
            }
        }

        // An immediately-applied closure literal is `CallTarget::Closure`; any
        // R closure that is NOT one of those exists only as a value.
        let mut applied: HashSet<Id> = HashSet::new();
        for node in graph.nodes() {
            for call in graph.calls_of(node.id()) {
                if let CallTarget::Closure(target) = call.target {
                    applied.insert(target);
                }
            }
        }
        for binding in self.program.module_level_bindings() {
            for call in graph.initializer_calls_of(binding) {
                if let CallTarget::Closure(target) = call.target {
                    applied.insert(target);
                }
            }
        }
        for node in graph.nodes() {
            let Node::Closure(closure_id) = *node else {
                continue;
            };
            if !in_r.contains(&closure_id)
                || applied.contains(&closure_id)
                || self.in_const_subtree(closure_id)
            {
                continue;
            }
            escapes.push((closure_id, None));
        }

        escapes.sort_by_key(|(site, _)| self.span_of(*site).start);
        escapes.dedup();
        for (site, function_id) in escapes {
            let subject = match function_id {
                Some(function_id) => self.const_only_name(function_id, emit_id),
                None => "this closure (it reaches `asset::emit`)".to_string(),
            };
            self.errors.push((
                Error {
                    note: None,
                    span: self.span_of(site),
                    msg: format!(
                        "{subject} is compile-time-only; call it directly inside a `const` \
                         expression — a compile-time-only function has no runtime value form"
                    ),
                },
                self.source_of(site),
            ));
        }
    }

    /// How a const-only callee names itself in a diagnostic: `asset::emit`
    /// itself, or the R-member that reaches it.
    fn const_only_name(&self, callee: Id, emit_id: Id) -> String {
        if callee == emit_id {
            return "`asset::emit`".to_string();
        }
        self.program
            .functions
            .get(&callee)
            .map(|function| format!("`{}` (it reaches `asset::emit`)", function.name))
            .unwrap_or_else(|| "this closure (it reaches `asset::emit`)".to_string())
    }

    /// The file an anchor entity's span indexes into — the file its diagnostic
    /// renders in (backlog E16); a synthetic entity falls back to the entry, and
    /// generated code to the file that wrote the attribute.
    fn source_of(&self, id: Id) -> SourceId {
        self.program.diagnostic_source_of(id)
    }

    /// Whether an entity sits inside any `const` expression's span (same
    /// source file) — the site test the capability check cuts edges by.
    fn in_const_subtree(&self, id: Id) -> bool {
        let Some(source) = self.program.source_of(id) else {
            return false;
        };
        let span = self.span_of(id);
        self.program.const_exprs.iter().any(|&root| {
            self.program.source_of(root) == Some(source) && {
                let root_span = self.span_of(root);
                span.start >= root_span.start && span.end <= root_span.end
            }
        })
    }

    /// The free local references of the const subtree: every `Expr::Local`
    /// whose span lies inside the expression's span (same source file), minus
    /// bindings DECLARED inside it (block `let`s, closure parameters — their
    /// references are internal, not free).
    fn free_locals(&self, root: Id) -> Vec<(Id, Id)> {
        let root_span = self.span_of(root);
        let Some(root_source) = self.program.source_of(root) else {
            return Vec::new();
        };
        let within = |id: Id| -> bool {
            self.program.source_of(id) == Some(root_source)
                && self
                    .program
                    .span_map
                    .get(&id)
                    .map(|span| span.start >= root_span.start && span.end <= root_span.end)
                    .unwrap_or(false)
        };
        let mut references = Vec::new();
        for (id, expr) in &self.program.entity_map {
            if let Expr::Local(binding) = expr
                && within(*id)
                && !within(*binding)
            {
                references.push((*id, *binding));
            }
        }
        // Deterministic diagnostic order.
        references.sort_by_key(|(id, _)| self.span_of(*id).start);
        references
    }

    fn classify(&self, binding: Id) -> Known<'src> {
        if let Some(parameter) = self.program.parameters.get(&binding) {
            return Known::Runtime(parameter.name);
        }
        if let Some(variable) = self.program.variables.get(&binding) {
            if variable.mutable {
                return Known::Runtime(variable.name);
            }
            let Some(initial) = variable.initial else {
                return Known::Runtime(variable.name);
            };
            if self.const_set.contains(&initial) {
                return Known::Const(initial);
            }
            let literal = matches!(
                self.program.entity_map.get(&initial),
                Some(
                    Expr::String(_)
                        | Expr::MultilineString(_)
                        | Expr::Number(..)
                        | Expr::Bool(_)
                        | Expr::Null
                )
            );
            if literal {
                return Known::Ok;
            }
            return Known::Runtime(variable.name);
        }
        // Items — functions, structs, enum constructors — are code, not
        // runtime state; the mini-program emits them.
        Known::Ok
    }

    fn span_of(&self, id: Id) -> Span {
        self.program
            .span_map
            .get(&id)
            .map(|span| **span)
            .unwrap_or((0..0).into())
    }
}
