//! The last-use liveness dataflow — `proposal/lifetimes.md` §6, slice **S2**.
//!
//! One notion, two consumers. This module answers a single question about a
//! program point:
//!
//! > **Is this read of `binding` its LAST use on every path out of here?**
//!
//! Copy elision (rule 2, [`Analyzer::is_elidable_copy`]) consumes it today: a
//! place whose owner is dead at the read donates its storage instead of being
//! deep-copied, because the aliasing that would follow can never be observed.
//! §6's last-use *disposal* (slice S3) consumes the same answer for a drop
//! point, and the loan-extension rule below is built to its strength rather
//! than to elision's, so S3 inherits it whole.
//!
//! **What it replaces.** `reference_count == 1` — a static, whole-program,
//! syntactic *count* of how many times a name was resolved — guarded by
//! `collect_repeatable_interiors`, a set of every id lexically inside a loop or
//! closure. The census (§3 fact 3) priced that test: **25.4% of entities fail
//! it, half of them with exactly two uses** — the read-then-move shape
//! (`f(&xs); mut ys = xs`) a real dataflow wins immediately. Both are deleted;
//! the loop rule below subsumes the interior set, because "live across the back
//! edge" is the question the interior set was approximating, and it asks it
//! *relative to the binding's declaration* — a binding declared INSIDE the loop
//! body is fresh on every iteration, so its last use in the body is a genuine
//! last use, which the lexical set could not see.
//!
//! **The shape.** A backward walk over the same scope-structured tree
//! [`Analyzer::scan_move`] walks forward, one region at a time:
//!
//! - **Regions.** A function body, a closure body, a module body. Each is
//!   walked alone, from an empty live set. A binding used in a region other
//!   than the one that declares it is `opaque` (below) — which is how the
//!   capture rule (§4: a closure captures BINDINGS, not values) and
//!   cross-module reads are both paid for, conservatively and in one rule.
//! - **Sequence.** Statements are walked in REVERSE evaluation order, so a
//!   read sees exactly the set of bindings some later read still wants. Getting
//!   the order wrong is not a precision bug but a soundness one: it would
//!   report the EARLIER of two reads as the last one.
//! - **Branches merge by union** — live if live on any successor path. An `if`
//!   with no `else` unions the fall-through path; a `match` unions every leg.
//! - **Loops carry.** A loop's body is walked from `live-after ∪ carry`, where
//!   `carry` is everything the body itself can still read on a later iteration
//!   (its own live-in, computed by a dry run of this same walk). A `jump`
//!   re-admits the carry, because its target is the loop head or the loop exit
//!   and both are covered by it.
//! - **Loans extend (§6.1).** A view keeps its owner alive to the VIEW's last
//!   use: a read of a view binding marks its origin ROOTS live, through
//!   [`Analyzer::compute_view_origins`] — the fixpoint §6.1 names as the one a
//!   last-use pass consumes unchanged. Elision alone would not need the full
//!   rule; S3's drop placement does, and this is where it lives.
//!
//! **`opaque` — the refusal set.** The walk answers optimistically and then
//! withdraws the answer for any binding it cannot stand behind. A binding is
//! opaque when it is read from more than one region, or from a region other
//! than its declaring one (captures, module globals read by functions); when
//! its declaration was never reached; when the walk did not reach every
//! `Expr::Local` node naming it that exists in the program (the completeness
//! net — a traversal gap becomes a refusal, never a wrong answer); or when it
//! is loaned somewhere the loan cannot be followed (below). Opacity is applied
//! at query time, so one pass suffices.
//!
//! **S3's second answer: [`DropExtent`].** Disposal asks a coarser question
//! than elision does — not "is THIS read the last one?" but "which STATEMENT of
//! the declaring scope holds the last read?", because a `finally` region has to
//! end at a statement boundary (`temporary-drop.md` §6.1's honest floor). The
//! walk therefore also records, for each binding's last read, the chain of
//! enclosing STATEMENTS from the outermost block inward
//! ([`Liveness::statement_stack`]); the transformer picks the element of that
//! chain that is a direct statement of the scope it is emitting, and closes the
//! teardown `finally` after it. Three answers, and the refusals fall back to the
//! law that shipped:
//!
//! - [`DropExtent::Declaration`] — the binding is never read; it drops right
//!   after its own declaration.
//! - [`DropExtent::Statement`] — the chain above.
//! - [`DropExtent::ScopeEnd`] — opaque, or last read in a scope's TAIL (which
//!   *is* the scope's end). Today's law, unchanged.
//!
//! Because a `finally` covers a lexical region, a last read inside a branch
//! yields the BRANCH statement, so the drop lands at the join and every path —
//! taken, not-taken, `ret`, `jump` — releases through the one `finally`
//! (`lifetimes.md` §6.3's drop specialization, flaglessly).
//!
//! **Parameters are walked too** (they reach expression position as
//! `Expr::Local` of the parameter's id), because an `own` resource parameter is
//! one of the three teardown classes §6 moves. They are declared at their
//! function's entry, which a backward walk reaches last. Elision does not read
//! their answers (`is_elidable_copy` gates on `variables`), and
//! `compute_view_origins` never keys a parameter, so nothing about the elision
//! answers moves when they join the walk.

use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;

use super::{Analyzer, Expr, ExprIfBranch, ExprPattern};

/// Where a binding's teardown region ENDS (`lifetimes.md` §6, slice S3).
///
/// The transformer turns this into the point at which the drop `finally`
/// closes. Every variant is a STATEMENT boundary, because that is what a
/// `try`/`finally` can be cut at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropExtent {
    /// The binding is never read: it drops immediately after its own
    /// declaration. This is the shape that fixes the serve-forever `main` —
    /// a handle nothing reads again is released now, not at a scope end that
    /// never arrives.
    Declaration,
    /// The last read sits inside this chain of enclosing statements, outermost
    /// first. The consumer picks the element that is a direct statement of the
    /// scope it is emitting and closes the region after it.
    Statement(Vec<Id>),
    /// The declaring scope's end — the law `destruction.md` §5 shipped, kept
    /// for an opaque binding (the refusal set) and for a last read in a scope's
    /// tail, where the two answers coincide anyway.
    ScopeEnd,
}

/// The dataflow's answer, keyed by USE SITE (an `Expr::Local` expression id).
///
/// Empty by default — an analyzer that has not run the pass elides nothing and
/// drops nothing early, which is the safe direction on both counts.
#[derive(Clone, Debug, Default)]
pub(super) struct LastUse {
    /// Every read that is the last use of its binding on every path out of it.
    last_uses: HashSet<Id>,
    /// Bindings the pass refuses to answer for (see the module doc).
    opaque: HashSet<Id>,
    /// Per binding, the enclosing-statement chain of its LAST read (outermost
    /// first). Absent = never read. Empty = read in a scope's tail.
    last_use_statements: HashMap<Id, Vec<Id>>,
    /// Per binding, the enclosing-statement chain of its DECLARATION. Every
    /// declaring form feeds it — `let`, a destructure, an `is` capture that
    /// binds into the surrounding scope, a `for` item — so the consumer can ask
    /// "which statement of this block brings this name into existence?" without
    /// enumerating the forms.
    declaration_statements: HashMap<Id, Vec<Id>>,
    /// Bindings with a read the walk never reached (the completeness net). The
    /// only refusal the SYNTACTIC answer honours: a chain it did not see every
    /// read of cannot be trusted to name the last one.
    unreached: HashSet<Id>,
}

impl LastUse {
    /// Whether the read at `use_id` is `binding_id`'s last use on every path —
    /// the question rule 2 and (later) §6's drop placement both ask.
    pub(super) fn is_last_use(&self, use_id: Id, binding_id: Id) -> bool {
        !self.opaque.contains(&binding_id) && self.last_uses.contains(&use_id)
    }

    /// Where `binding_id`'s teardown region ends — §6's disposal answer.
    /// An opaque binding falls back to the scope end it has always had; the
    /// pass never guesses a drop point it cannot stand behind.
    pub(super) fn drop_extent(&self, binding_id: Id) -> DropExtent {
        if self.opaque.contains(&binding_id) {
            return DropExtent::ScopeEnd;
        }
        self.syntactic_extent(binding_id)
    }

    /// The same coordinate asked SYNTACTICALLY — the last statement that
    /// mentions the name, with opacity ignored.
    ///
    /// This is the question emitted JS *scoping* asks, and it is a different
    /// question from disposal's. A `const` declared inside a `try` dies at that
    /// block's brace, so a teardown region may not close while a name declared
    /// inside it is still read afterwards — whether or not the dataflow can
    /// say anything about that name's liveness. Opacity is a claim about when a
    /// value may be destroyed; block scope is a claim about where a name can be
    /// written down, and only the completeness net (a read the walk never saw)
    /// can make the syntactic answer wrong.
    pub(super) fn syntactic_extent(&self, binding_id: Id) -> DropExtent {
        if self.unreached.contains(&binding_id) {
            return DropExtent::ScopeEnd;
        }
        match self.last_use_statements.get(&binding_id) {
            None => DropExtent::Declaration,
            Some(chain) if chain.is_empty() => DropExtent::ScopeEnd,
            Some(chain) => DropExtent::Statement(chain.clone()),
        }
    }

    /// Per declaring STATEMENT, the syntactic extents of the bindings it brings
    /// into existence — what a teardown region must cover before it may close,
    /// because those names live in the emitted block the region becomes.
    ///
    /// Keyed on the outermost enclosing statement, so a name declared in a
    /// nested block keys the statement that block belongs to; its own last read
    /// is inside that same statement, so it never widens anything by itself.
    /// A binding declared in a scope's tail, or at a function's entry (a
    /// parameter), keys nothing — neither can be read past a statement region.
    pub(super) fn declared_binding_extents(&self) -> HashMap<Id, Vec<DropExtent>> {
        let mut declared: HashMap<Id, Vec<DropExtent>> = HashMap::default();
        for (binding_id, chain) in &self.declaration_statements {
            let Some(statement) = chain.first().copied() else {
                continue;
            };
            declared
                .entry(statement)
                .or_default()
                .push(self.syntactic_extent(*binding_id));
        }
        declared
    }

    /// Run the pass over every region of the program.
    pub(super) fn compute(analyzer: &Analyzer<'_>) -> Self {
        let view_origins = analyzer.compute_view_origins();
        let mut walk = Liveness {
            analyzer,
            view_origins: &view_origins,
            live: HashSet::default(),
            repeat_carry: None,
            dry: false,
            region: Id(0),
            last_uses: HashSet::default(),
            statement_stack: Vec::new(),
            last_use_statements: HashMap::default(),
            declaration_statements: HashMap::default(),
            walked_uses: HashMap::default(),
            use_region: HashMap::default(),
            declaration_region: HashMap::default(),
            conflicted: HashSet::default(),
        };

        // Function bodies. A nested `fun` is a region of its own and is not
        // descended into from its enclosing body, exactly as the call graph's
        // traversal treats it.
        for function in analyzer.functions.values() {
            if function.has_body {
                walk.enter(function.id);
                walk.walk_block(&function.body.0, function.body.1);
                // Parameters come into existence at the body's entry, which a
                // BACKWARD walk reaches last — an `own` resource parameter is
                // one of §6's three teardown classes and needs the same
                // declaration kill every local gets.
                for parameter_id in &function.parameters {
                    walk.record_declaration(*parameter_id);
                }
            }
        }
        // Module bodies (module-level bindings live here; a function READING
        // one crosses regions and is refused below).
        for module in analyzer.modules.values() {
            walk.enter(module.id);
            walk.walk_block(&module.body.0, module.body.1);
        }
        // Closure bodies. Their own root, at their own loop depth — an
        // enclosing loop does not repeat a closure invocation — and their
        // parameter destructures run before the body, so they are walked after
        // it going backward.
        for closure in analyzer.closures.values() {
            walk.enter(closure.id);
            walk.walk(closure.return_);
            for destructure_id in closure.parameter_destructures.iter().rev() {
                walk.walk(*destructure_id);
            }
        }

        let mut opaque = walk.conflicted;
        // The completeness net's own half, kept separate: it is the ONE refusal
        // the syntactic answer honours (see [`LastUse::syntactic_extent`]).
        let mut unreached: HashSet<Id> = HashSet::default();
        // The completeness net: every `Expr::Local` naming a variable that
        // exists in the program must have been REACHED by the walk. A form the
        // traversal does not know about would otherwise hide a use and turn a
        // live binding into a reported last use — the one wrong answer this
        // pass must never give. A gap costs elisions, never correctness.
        for (expr_id, expr) in &analyzer.expr_id_to_expr_map {
            let Expr::Local(binding_id) = expr else {
                continue;
            };
            if !analyzer.variables.contains_key(binding_id)
                && !analyzer.parameters.contains_key(binding_id)
            {
                continue;
            }
            let reached = walk
                .walked_uses
                .get(binding_id)
                .is_some_and(|sites| sites.contains(expr_id));
            if !reached {
                opaque.insert(*binding_id);
                unreached.insert(*binding_id);
            }
        }
        // A binding whose declaration the walk never reached, or that is read
        // outside the region declaring it.
        for binding_id in walk.walked_uses.keys() {
            match (
                walk.declaration_region.get(binding_id),
                walk.use_region.get(binding_id),
            ) {
                (Some(declared), Some(used)) if declared == used => {}
                _ => {
                    opaque.insert(*binding_id);
                }
            }
        }
        analyzer.collect_unfollowable_loans(&view_origins, &mut opaque);
        // A view the pass cannot answer for drags its owners down with it: the
        // extension rule is only as good as the view's own liveness. Views copy
        // between locals, so this closes over the origin relation.
        loop {
            let mut changed = false;
            for (view_id, roots) in &view_origins {
                if !opaque.contains(view_id) {
                    continue;
                }
                for root in roots {
                    changed |= opaque.insert(*root);
                }
            }
            if !changed {
                break;
            }
        }

        LastUse {
            last_uses: walk.last_uses,
            opaque,
            last_use_statements: walk.last_use_statements,
            declaration_statements: walk.declaration_statements,
            unreached,
        }
    }
}

/// The backward walk's state for one region.
struct Liveness<'a, 'src> {
    analyzer: &'a Analyzer<'src>,
    view_origins: &'a HashMap<Id, Vec<Id>>,
    /// The bindings live at the point the walk has reached — i.e. read by
    /// something the walk has ALREADY visited, which is everything that runs
    /// after this point on some path.
    live: HashSet<Id>,
    /// The innermost enclosing loop's live set at its head, re-admitted by a
    /// `jump` (whose target is that head, or the loop's exit — covered by it).
    repeat_carry: Option<HashSet<Id>>,
    /// A dry run computing a loop's carry set: the `live` transfer is real,
    /// the ANSWERS are not recorded.
    dry: bool,
    /// The region root the walk is inside (a function, module or closure id).
    region: Id,
    last_uses: HashSet<Id>,
    /// The chain of block STATEMENTS the walk is inside, outermost first — the
    /// statement-boundary coordinate a `finally` region can be cut at.
    statement_stack: Vec<Id>,
    /// Per binding, the statement chain of its LAST read. The walk runs
    /// backward, so the FIRST chain recorded is the last one in program order
    /// and later writes are dropped.
    last_use_statements: HashMap<Id, Vec<Id>>,
    /// Per binding, the statement chain of its DECLARATION.
    declaration_statements: HashMap<Id, Vec<Id>>,
    /// Every read site the walk reached, per binding — the completeness net.
    walked_uses: HashMap<Id, HashSet<Id>>,
    use_region: HashMap<Id, Id>,
    declaration_region: HashMap<Id, Id>,
    /// Bindings read from two regions, or declared twice.
    conflicted: HashSet<Id>,
}

impl Liveness<'_, '_> {
    /// Start a fresh region: nothing is live at a body's end.
    fn enter(&mut self, region: Id) {
        self.region = region;
        self.live.clear();
        self.repeat_carry = None;
        self.statement_stack.clear();
    }

    /// Remember where `binding_id`'s last read sits, as a statement chain. The
    /// walk is backward, so the first answer recorded is the last read in
    /// program order — `or_insert_with` is the "latest wins" it looks like the
    /// opposite of.
    fn record_statement_chain(&mut self, binding_id: Id) {
        if self.dry {
            return;
        }
        self.last_use_statements
            .entry(binding_id)
            .or_insert_with(|| self.statement_stack.clone());
    }

    /// A read of `binding_id` at `use_id`. Records the answer, then marks the
    /// binding (and, for a view, its owners — §6.1) live for everything the
    /// walk reaches next, which is everything that runs BEFORE this point.
    fn record_use(&mut self, use_id: Id, binding_id: Id) {
        if !self.dry {
            if !self.live.contains(&binding_id) {
                self.last_uses.insert(use_id);
            }
            match self.use_region.get(&binding_id) {
                Some(region) if *region != self.region => {
                    self.conflicted.insert(binding_id);
                }
                Some(_) => {}
                None => {
                    self.use_region.insert(binding_id, self.region);
                }
            }
            self.walked_uses
                .entry(binding_id)
                .or_default()
                .insert(use_id);
        }
        self.record_statement_chain(binding_id);
        self.live.insert(binding_id);
        // §6.1, the loan-extension rule: a `borrows` projection extends its
        // owner's last use to the last use of any view rooted at it. Reading
        // the VIEW is therefore a read of every root it projects from — for the
        // drop point as much as for liveness, or the owner would be torn down
        // under a live projection (the one unsoundness shape §6.1 names).
        if let Some(roots) = self.view_origins.get(&binding_id).cloned() {
            for root in roots {
                self.record_statement_chain(root);
                self.live.insert(root);
            }
        }
    }

    /// A binding comes into existence here: it is dead everywhere before this
    /// point, which is what makes a use inside a loop body of a binding the
    /// body itself declares a genuine last use.
    fn record_declaration(&mut self, binding_id: Id) {
        if !self.dry {
            self.declaration_statements
                .entry(binding_id)
                .or_insert_with(|| self.statement_stack.clone());
            match self.declaration_region.get(&binding_id) {
                Some(_) => {
                    self.conflicted.insert(binding_id);
                }
                None => {
                    self.declaration_region.insert(binding_id, self.region);
                }
            }
        }
        self.live.remove(&binding_id);
    }

    /// Statements, then the tail — backward, so the tail first. A statement
    /// that IS a `ret` makes everything after it unreachable, which is what
    /// lets the guard-clause shape (`if bad { ret e }; …`) answer per path.
    fn walk_block(&mut self, statements: &[Id], tail: Id) {
        // The tail is not a statement: a read there is a read at the scope's
        // END, which is exactly `DropExtent::ScopeEnd` and is recorded as the
        // empty chain by leaving the stack alone.
        self.walk(tail);
        for statement_id in statements.iter().rev() {
            if matches!(
                self.analyzer.expr_id_to_expr_map.get(statement_id),
                Some(Expr::FunctionReturn(_))
            ) {
                self.live.clear();
            }
            self.statement_stack.push(*statement_id);
            self.walk(*statement_id);
            self.statement_stack.pop();
        }
    }

    /// Run `body` over a loop's interior with nothing live, yielding its own
    /// live-in — every binding the body can still read on a LATER iteration.
    /// The dry flag suppresses the answers; only the `live` transfer is real.
    fn carry_of(&mut self, condition: Option<Id>, statements: &[Id], tail: Id) -> HashSet<Id> {
        let saved_live = std::mem::take(&mut self.live);
        let saved_repeat = self.repeat_carry.take();
        let was_dry = std::mem::replace(&mut self.dry, true);
        self.walk_block(statements, tail);
        if let Some(condition) = condition {
            self.walk(condition);
        }
        self.dry = was_dry;
        self.repeat_carry = saved_repeat;
        std::mem::replace(&mut self.live, saved_live)
    }

    /// The body of a loop-shaped form: seeded with the back edge's carry, and
    /// publishing that seed for any `jump` inside it.
    fn walk_repeat(&mut self, condition: Option<Id>, statements: &[Id], tail: Id) {
        if self.dry {
            // Already computing an outer carry — a plain walk of the interior
            // is exactly the union this is asked for, and re-deriving the
            // inner carry here would cost a pass per nesting level.
            self.walk_block(statements, tail);
            if let Some(condition) = condition {
                self.walk(condition);
            }
            return;
        }
        let live_after = self.live.clone();
        let carry = self.carry_of(condition, statements, tail);
        self.live.extend(carry);
        let saved_repeat = self.repeat_carry.replace(self.live.clone());
        self.walk_block(statements, tail);
        if let Some(condition) = condition {
            self.walk(condition);
        }
        self.repeat_carry = saved_repeat;
        // A loop may run zero times, so everything live after it is live before.
        self.live.extend(live_after);
    }

    /// Declarations a pattern makes, and the literal expressions it tests
    /// against. Backward: the captures come into existence at the test, so they
    /// are killed here, and the literals are read here.
    fn walk_pattern(&mut self, pattern: &ExprPattern) {
        match pattern {
            ExprPattern::Wildcard => {}
            ExprPattern::Binding(capture_id) => {
                self.record_declaration(*capture_id);
            }
            ExprPattern::Variant(_, _, payload) => {
                for sub_pattern in payload {
                    self.walk_pattern(sub_pattern);
                }
            }
            ExprPattern::Tuple(elements) => {
                for (sub_pattern, _) in elements {
                    self.walk_pattern(sub_pattern);
                }
            }
            ExprPattern::Array(elements) => {
                for sub_pattern in elements {
                    self.walk_pattern(sub_pattern);
                }
            }
            ExprPattern::Literal(value_id) => {
                self.walk(*value_id);
            }
        }
    }

    /// One expression, backward. Sub-expressions are visited in REVERSE
    /// evaluation order throughout — see the module doc on why that is a
    /// soundness property and not a precision one.
    fn walk(&mut self, expr_id: Id) {
        let Some(expr) = self.analyzer.expr_id_to_expr_map.get(&expr_id).cloned() else {
            return;
        };
        match expr {
            // --- the leaves that matter ---
            Expr::Local(binding_id) => {
                // A `Local` also names functions, enums and modules; only a
                // value binding — a local or a PARAMETER, which reaches
                // expression position under this same node — has liveness.
                if self.analyzer.variables.contains_key(&binding_id)
                    || self.analyzer.parameters.contains_key(&binding_id)
                {
                    self.record_use(expr_id, binding_id);
                }
            }
            Expr::Variable(variable_id) => {
                self.record_declaration(variable_id);
                if let Some(initial) = self
                    .analyzer
                    .variables
                    .get(&variable_id)
                    .and_then(|variable| variable.initial)
                {
                    self.walk(initial);
                }
            }
            Expr::Destructure(value_id, pattern) => {
                self.walk_pattern(&pattern);
                self.walk(value_id);
            }
            Expr::Is(subject_id, pattern) => {
                // `x is Some(let v)` binds into the SURROUNDING scope, so the
                // capture is declared here and read by whatever follows.
                self.walk_pattern(&pattern);
                self.walk(subject_id);
            }

            // --- places ---
            Expr::Field(subject_id, _, _) | Expr::TupleIndex(subject_id, _, _) => {
                self.walk(subject_id);
            }
            Expr::Index(subject_id, index_id) => {
                self.walk(index_id);
                self.walk(subject_id);
            }
            Expr::Reference(operand_id, _) | Expr::Dereference(operand_id) => {
                self.walk(operand_id);
            }
            Expr::ArrayLen(subject_id, _) => {
                self.walk(subject_id);
            }

            // --- calls and constructions ---
            Expr::Call(call_id) => {
                let Some(function_call) = self.analyzer.function_calls.get(&call_id).cloned()
                else {
                    return;
                };
                for argument_id in function_call.argument_ids.iter().rev() {
                    self.walk(*argument_id);
                }
                self.walk(function_call.subject_id);
            }
            Expr::StructInitializer(_, fields) => {
                for value_id in fields.values().rev() {
                    self.walk(*value_id);
                }
            }
            Expr::List(element_ids) | Expr::Tuple(element_ids) => {
                for element_id in element_ids.iter().rev() {
                    self.walk(*element_id);
                }
            }
            // `[value; n]` writes the value into every slot, so a read there
            // repeats exactly as a loop body's does.
            Expr::Repeat(value_id, _length) => {
                self.walk_repeat(None, &[], value_id);
            }

            // --- assignment ---
            // A write to a binding is treated as a READ of it and does not
            // kill: the elision question is about the VALUE side, and a killed
            // target would have to prove the emitted write rebinds rather than
            // mutates through a box. Conservative in the refusing direction.
            Expr::Assignment(target_id, value_id) => {
                self.walk(target_id);
                self.walk(value_id);
            }

            // --- control flow ---
            Expr::Block((statements, tail)) => {
                self.walk_block(&statements, tail);
            }
            Expr::If(branch) => {
                self.walk_if(&branch);
            }
            Expr::Match(subject_id, legs) => {
                let live_after = std::mem::take(&mut self.live);
                let mut merged = live_after.clone();
                for leg in &legs {
                    self.live.clone_from(&live_after);
                    self.walk(leg.body);
                    if let Some(guard_id) = leg.guard {
                        self.walk(guard_id);
                    }
                    self.walk_pattern(&leg.pattern);
                    merged.extend(self.live.iter().copied());
                }
                self.live = merged;
                self.walk(subject_id);
            }
            Expr::For(condition, (statements, tail)) => {
                self.walk_repeat(condition, &statements, tail);
            }
            Expr::ForEach(iterable_id, item, (statements, tail)) => {
                self.walk_repeat(None, &statements, tail);
                // The element binding is fresh on every iteration, declared at
                // the top of the body — inside the repeat, before the iterable.
                if let Some(item_id) = item {
                    self.record_declaration(item_id);
                }
                self.walk(iterable_id);
            }
            // A tuple comprehension UNROLLS: one body per element, all from the
            // same expression ids, so a read there repeats like a loop's.
            Expr::TupleComprehension(binder_id, source_id, body_id) => {
                self.walk_repeat(None, &[], body_id);
                self.record_declaration(binder_id);
                self.walk(source_id);
            }
            Expr::FunctionReturn(Some(value_id)) => {
                self.walk(value_id);
            }
            Expr::FunctionReturn(None) => {}
            // `jump break` / `jump continue`: the successor is the loop's exit
            // or its head, and the carry covers both.
            Expr::Jump(_) => {
                if let Some(carry) = self.repeat_carry.clone() {
                    self.live.extend(carry);
                }
            }

            // --- pass-through ---
            Expr::Await(inner_id) | Expr::TryAssert(inner_id) => {
                self.walk(inner_id);
            }
            Expr::Unary(_, operand_id) => {
                self.walk(operand_id);
            }
            Expr::Binary(_, left_id, right_id) => {
                self.walk(right_id);
                self.walk(left_id);
            }
            Expr::Lift(subject_id, _binder, continuation_id) => {
                self.walk(continuation_id);
                self.walk(subject_id);
            }
            Expr::LiftRegion(steps, body_id) => {
                self.walk(body_id);
                for (step_id, _binder, _is_split) in steps.iter().rev() {
                    self.walk(*step_id);
                }
            }

            // A closure body is its OWN region (the capture rule, §4): the
            // bindings it reads are read from another region and are refused
            // wholesale, so descending here would double-count them.
            Expr::Closure(_)
            | Expr::Async(_)
            // Declarations and non-value leaves.
            | Expr::Bool(_)
            | Expr::Number(_, _, _)
            | Expr::String(_)
            | Expr::MultilineString(_)
            | Expr::Null
            | Expr::Void
            | Expr::Error
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
            | Expr::Parameter(_)
            | Expr::ExternalFunction(_) => {}
        }
    }

    /// An `if` chain, backward: every arm from the same live-out, merged by
    /// union, then the condition (which runs before all of them, and may itself
    /// declare — `if x is Some(let v)`).
    fn walk_if(&mut self, branch: &ExprIfBranch) {
        match branch {
            ExprIfBranch::If(condition_id, (statements, tail), else_branch) => {
                let live_after = std::mem::take(&mut self.live);
                self.live.clone_from(&live_after);
                self.walk_block(statements, *tail);
                let mut merged = std::mem::take(&mut self.live);
                match else_branch {
                    Some(next) => {
                        self.live.clone_from(&live_after);
                        self.walk_if(next);
                        merged.extend(self.live.iter().copied());
                    }
                    // No `else`: the fall-through path is a successor too.
                    None => merged.extend(live_after.iter().copied()),
                }
                self.live = merged;
                self.walk(*condition_id);
            }
            ExprIfBranch::Else((statements, tail)) => {
                self.walk_block(statements, *tail);
            }
        }
    }
}

impl Analyzer<'_> {
    /// The owners of loans this pass cannot follow to their end.
    ///
    /// The extension rule (§6.1) only holds where the view has a liveness of
    /// its own to extend to: a `&place` bound to a local, or projected by a
    /// `borrows` call whose result is bound. Everywhere else the loan escapes
    /// into a value whose lifetime this pass does not model — a construction
    /// slot (`Some(&x)`), a struct field, an assignment onto an existing view
    /// binding, a returned or tailed reference — and the owner is refused
    /// outright, which is what `reference_count` was already doing for every
    /// loaned binding.
    ///
    /// A loan handed to a RESOLVED callee is call-bounded and needs no
    /// refusal — §6.4's rule, whose declared-retention escape hatch is S4's
    /// business. A loan handed to a callee this analysis cannot resolve
    /// (dispatched, generic) is refused, because "call-bounded" is a claim
    /// about a signature nobody has read.
    fn collect_unfollowable_loans(
        &self,
        view_origins: &HashMap<Id, Vec<Id>>,
        opaque: &mut HashSet<Id>,
    ) {
        // The positions a loan may be created in and still be followable.
        let mut anchored: HashSet<Id> = HashSet::default();
        // The positions a `borrows` call's projected view may land in and still
        // be followable: bound to a local, or a `match` subject whose captures
        // `compute_view_origins` maps back to the same roots.
        let mut projected_anchors: HashSet<Id> = HashSet::default();
        for variable in self.variables.values() {
            if let Some(initial) = variable.initial {
                anchored.insert(initial);
                projected_anchors.insert(initial);
            }
        }
        for expr in self.expr_id_to_expr_map.values() {
            match expr {
                Expr::Call(call_id) => {
                    let Some(function_call) = self.function_calls.get(call_id) else {
                        continue;
                    };
                    if self.callee_conventions(function_call.subject_id).is_some() {
                        anchored.extend(function_call.argument_ids.iter().copied());
                    }
                }
                Expr::ForEach(iterable_id, _, _) => {
                    anchored.insert(*iterable_id);
                    projected_anchors.insert(*iterable_id);
                }
                Expr::Match(subject_id, _) => {
                    projected_anchors.insert(*subject_id);
                }
                _ => {}
            }
        }
        for (expr_id, expr) in &self.expr_id_to_expr_map {
            match expr {
                Expr::Reference(operand_id, _) if !anchored.contains(expr_id) => {
                    if let Some(root) = self.place_root(*operand_id) {
                        opaque.insert(root);
                    }
                }
                Expr::Call(call_id) if !projected_anchors.contains(expr_id) => {
                    for place_id in self.projected_argument_ids(*call_id) {
                        let roots = match self.expr_id_to_expr_map.get(&place_id) {
                            Some(Expr::Local(binding_id)) => view_origins.get(binding_id).cloned(),
                            _ => None,
                        }
                        .or_else(|| self.place_root(place_id).map(|root| vec![root]))
                        .unwrap_or_default();
                        opaque.extend(roots);
                    }
                }
                _ => {}
            }
        }
    }
}
