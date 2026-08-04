# Vilan Backlog — everything outstanding

> **Superseded 2026-07-18** — open work now lives in
> [`backlog-2026-07-18.md`](backlog-2026-07-18.md) (distilled, open items only, same
> stable ids). This file stays as the historical record: shipped items, their full
> context, and the lessons recorded alongside them. Don't add new items here.

A running capture of work that is *known but not done*, so nothing is lost to conversation. This
is the tactical companion to [`roadmap.md`](roadmap.md) (the ranked strategic view); items that
`roadmap.md` already tracks are cross-referenced by number rather than duplicated in full.

Per the project's engineering principles (see `CLAUDE.md`): each non-trivial item below should get
a **formal definition + unit tests + regression tests** before it is implemented, and should be
built to subsume special cases rather than patch them. Items carry a rough size (S/M/L) and known
dependencies. Unordered within a section.

Item numbers are **stable identifiers** (other documents cite them — `backlog F3`, `I2`):
completed items are removed and their numbers retired, so numbering within a section may
have gaps.

---

## A. Reactive core & UI (`std::reactive`, `std::ui`)

3. ~~**`bind_each` keyed reconciliation**~~ — **SHIPPED 2026-07-07**: rows move with their
   keys, a changed row re-renders (`T: PartialEq`), removed rows dispose + leave the DOM;
   the plan is `std::reactive::reconcile` (pure, node-tested — corpus `reactive-keyed.vl`
   + pins), applied by `bind_each` (appending a kept element MOVES it, so ordering is one
   append per row). `Owner.defer` added for non-`Disposable` teardown.

4. ~~**`flatten` reactive combinator**~~ — **SHIPPED 2026-07-07**: `outer.flatten()` on
   `Signal<Signal<U>>` (a nested-generic impl subject) follows the current inner and
   DETACHES a replaced one (corpus `reactive-flatten.vl` + pin). Internal subscriptions
   follow `map`/`combine`'s unowned precedent; the rolling inner subscription is disposed
   per switch.

5. **Ambient owner / `comp` ergonomic layer** (`proposal/ambient-owner.md`) — **COMPLETE
   2026-07-07** (basics + `comp` + B15 + the `std::ui` boundary-ownership integration:
   owner-less `View`, ambient `bind_*`, per-row owners, `when`, `mount_root`; remaining
   tails recorded with triggers: `get_safe`, fence-diagnostic anchoring). History: `owner_scope` (a `Context<Owner>`), `get_owner()`, and
   `Signal.effect` (the scope-tied `sub` — registers into the ambient owner, nothing to
   hold; misuse outside an extent is a COMPILE error via the context coverage fence). The
   substrate was proven against stored callbacks AND async first (probes: capture survives
   extent exit and `await`; interleaved extents each keep their value). Findings: a
   `run_with_owner(owner, body)` wrapper FUNCTION is impossible by the context model's own
   rules (`run` needs a closure literal; capture is at CREATION — a forwarded body is born
   outside the extent), so the extent is entered as `owner_scope.run(owner, || ..)`; macro
   sugar can restore the wrapper spelling later. The coverage check gained the DEAD-reader
   exemption (an uncalled, un-taken, non-top-level function cannot run uncovered) — without
   it every `std::reactive` importer failed. ~~`effect` on the `Source` trait~~ (shipped
   with B14's fix). ~~`comp` sugar~~ (shipped on B15 + value-returning `Context.run<U>`
   — the `batch` shape; `run_with_owner` yields too). ~~`std::ui` integration~~ (shipped:
   the boundary-ownership model — the fold-scope-into-View question resolved as
   `mount_root`/`comp` roots owning everything ambiently).

6. **Reactive turns — scoped flush + async turns** (M–L; **CORE SHIPPED
   2026-07-09** — `get_safe` + `Turn`/`turn_scope`/`flush`/`turn`/`batch` in
   `std::reactive` (injected bodies; drain-affinity stack for mid-settle
   cascades — the one runtime device; per-turn dedup + budget) + the server
   boundary (`[service]` routes wrap their bodies in per-dispatch `turn(AtEnd, ..)`;
   manual `dispatcher.on` handlers self-`batch`, as the coalescing benchmark now
   spells). The `std::ui` boundary shipped same-day
   (View.on/bind_value/mount_root wrap dispatches in turns via plain host-stored
   adapters), riding two B15 extensions: clauses on `let` annotations and clause
   ADOPTION (an unannotated closure-literal binding passed into a clause position
   adopts it — the idiomatic `let add = || ..; .on("click", add)` just works).
   Continuation settling shipped same-day too: a write landing after the turn
   settled schedules ONE microtask drain (`queue_microtask` extern), so each
   async continuation segment settles as a coalesced wave — no compiler
   insertion needed, the policies converge for async extents (a true
   held-across-await `AtEnd` = `turn_async`, recorded). `turn_async` +
   `optimistic` shipped same-day, closing the follow-ons: `turn_async(body)` =
   the TRUE transactional extent — every notification held until the body's
   whole async chain completes, one coalesced settle (spawn-then-await over
   the J2 gap); `optimistic(signal, value, commit)` = paint now, await the
   commit, reconcile to the confirmed value or roll back, returning the
   outcome. **A6 is COMPLETE**; the cadence split for directly-awaiting
   `turn` bodies is the one recorded refinement. Original
   design: **proposal: `reactive-turns.md`, 2026-07-09** — supersedes the original "auto-flush on the next
   microtask" sketch, which a review scenario killed: the scheduler's single global
   pending queue means one request's `flush` drains every interleaved request's
   notifications, and a global microtask hook makes that routine). The redesign: a
   `Turn` (queue + policy) established through `turn_scope: Context<Turn>` at
   boundaries — UI events/`mount_root` (`AtSuspension`: settle at each await +
   end, the optimistic-paint cadence), `serve_connected`/RPC dispatch (`AtEnd`:
   transactional) — with `set` routing via `get_safe` (no turn → inline, status
   quo), `flush` draining only the ambient turn, `batch` dissolving into
   join-or-create. Context capture-at-creation makes a request's turn follow its
   own awaits (the A5 probes). Prerequisite sub-slice: **`get_safe`** (the A5
   tail's first real consumer). Honest limit recorded: turns isolate NOTIFICATION
   waves, not value visibility on shared signals (eager commit; last-flush-wins).
   The optimistic-write → reconcile lifecycle remains the follow-on, riding turns.
   C3 shipped, so nothing blocks this.

7. **Server-side rendering (SSR) + hydration vs resumability** (L–XL; recorded 2026-07-08;
   proposal first) — render the initial UI as HTML on the server (first paint before any JS,
   SEO), then make it live on the client. Vilan's model is unusually well placed: the UI is
   fine-grained reactive (no VDOM — Solid's shape, where SSR is proven), the compiler already
   builds client AND server bundles from one program (the full-stack split), and value
   semantics make the state handoff mostly plain data (views are second-class and never
   stored — nothing dangling to serialize; `Shared` identity is the one careful spot).
   - **What server rendering needs regardless of strategy:** a render-only target for
     `std::ui` — `View` over an HTML string-builder (or DOM shim) instead of `document`,
     legal on `@process` where the platform gate today forbids the browser layer (a
     `_sys`-style seam: same interface, an HTML impl on the server — the platform model's
     §5 shape). Effects/subscriptions must NOT run server-side; server render is
     create-serialize-discard (A5's boundary owners just never get disposal work).
   - **Hydration** (the Solid/React lineage): the client re-runs the component tree, but a
     hydrating DOM adapter CLAIMS existing server nodes instead of creating them —
     `bind_text` adopts the server text rather than rewriting, listeners attach, signals
     re-create from serialized initial values. Needs deterministic node addressing
     (hydration markers) and a first-run-adopts discipline in the `bind_*` effects. The
     well-trodden path; maps 1:1 onto `std::ui`'s ambient bindings.
   - **Resumability** (the Qwik lineage): the server serializes enough that the client
     resumes WITHOUT re-executing components — event handlers become addressable entry
     points loaded on demand. JS frameworks contort to get this (every handler manually
     `$`-split); **vilan owns closure conversion in the compiler**, so lowering each
     handler to a top-level function + an explicit serialized environment record (Wire
     already exists) is a compiler pass, not a user convention — the language is genuinely
     better positioned than the JS ecosystem here. Still the research-grade option:
     `Shared` graph serialization, lazy chunk loading, event delegation before JS loads.
   - **Recommended shape:** v1 = server string-renderer + hydration (proven, incremental,
     every piece reusable later); resumability recorded as the ambitious follow-on riding
     the same render target and serialization format. Streaming SSR / suspense boundaries
     are beyond-v1 (interact with A6's async turns and J1). Dependencies met: platform
     model, P6 transport (data fetching), A5 boundary ownership.

8. **UI styling — typed atomic styles, compiled** (L; `proposal/ui-styling.md`,
   REVISED 2026-07-10 — expression-flavored, rides `const` (G2); **CORE SHIPPED
   same day**: `std::style` (builder chain ~30 properties, Color/Length/space
   tokens as `:root`-var-carrying values, pseudo/breakpoint/dark conditions,
   `raw`, pure-vilan djb2 class hashing — cross-program-deterministic, proven by
   the corpus and example minting identical names), `View.styled`/`style_var`,
   12 pins, corpus `style.vl` with js AND css goldens, styled reactive-ui
   counter emitting `app.css`. Remaining recorded in the proposal status:
   bind_styled, dark×pseudo, html `<link>` scaffold, fmt chain splitting,
   property long tail, A7 critical CSS, liveness dead-style elimination) —
   the last big hole in the UI model. Styles are typed values built by ordinary
   a const-evaluated builder chain (`let card = const style().display(Display::Flex)
   .padding(space(4)).hover(style().background(Color::gray(100)));` — one import,
   `.`-completion over the property surface, color tokens namespaced on the `Color`
   type; `+` combines named styles) lowered to
   deduplicated atomic CSS through the const-eval **asset channel**; merge is `+`
   (`impl Style with Add`) with per-property last-wins — record semantics, so
   specificity fights are structurally impossible; const merges fold, runtime merges
   are a map union, never string parsing. Variants are plain `match` over const
   styles (CVA dissolves), governed by the load-bearing **construct-in-const rule**:
   property functions bottom out in const-only `asset::emit`, so a runtime
   construction is a STATIC error and every variant's CSS exists at build time.
   Tokens: themeable ones (space/color/type) lower to CSS custom properties
   (re-theming and dark mode = property swaps; signal-driven values ride `var()`);
   structural ones (breakpoints) resolve to literals at const time — the first
   draft's config knob dissolved. The macro-DSL first draft is superseded (git
   history keeps it): the expression form gets hover/go-to-def/typed diagnostics
   for free and composes with functions/impls natively. Tailwind stays a documented
   SIDECAR bridge, not the foundation. Order: G2 slices 1–2, then std::ui::style
   (slices 3–5), A7-entangled tail (critical CSS, dead-style elimination) later.

9. **`vilan.toml [build] run` hooks** (S; spun off A8) — run external commands
   alongside `vilan build` / `--watch` (the Tailwind-bridge runner, asset pipelines,
   codegen sidecars). Useful independent of styling.

10. ~~**`std::ui` router**~~ — **SHIPPED 2026-07-11** (`proposal/router.md`; Kolt
    gap §2.3). The enum-route model: routes are (nested) ENUMS plus a
    hand-written `parse`/`href` inverse pair — nested layouts are nested
    functions, guards are `if`s, params are typed payloads, and pattern-string
    routing never enters the language. `std::router` (browser layer):
    `current_path()` (one signal driven by `pushState` AND `popstate`),
    `navigate`, `segments`, and `link(label, route)` over a `Routable` trait —
    a real `<a href>` intercepting only plain left-clicks. Plus the general
    machinery routing rode in on: `View.swap<T: PartialEq>` (the
    value-generalized `when` — dispose + re-render per changed value, equal
    values a no-op) and `View.on_event` / `std::dom::Event` (typed DOM
    events). Runtime semantics pinned HEADLESS in
    `crates/vilan-cli/tests/router.rs` (a ~60-line DOM/history stub under
    node: interception, dedupe, disposal, popstate); compile pins in
    `inference.rs`. Building it found B19 and B20.

11. ~~**Web storage externs**~~ — **SHIPPED 2026-07-11** as `std::storage`
    (browser layer): `get`/`set`/`remove` over `localStorage`, `session_*` over
    `sessionStorage`; a missing key reads "" (the `__local_get`/`__session_get`
    helpers flatten the host's null). The pilot's token home. Live-tested by the
    pilot (the node harness can't build browser layers).

12. ~~**`Draft<T>` — local-first cells (optimistic-local editing)**~~ —
    **SHIPPED 2026-07-12** (kolt-migration §4's crate-style refinement).
    `draft(initial, commit)`: edits land in `local` FIRST (`push` sets the
    signal + Dirty, then SPAWNS the commit via `async self.settle(..)` —
    the input path never rides the wire), a generation counter discards
    superseded completions (fast typing over a slow wire), and failure
    KEEPS the local value (unlike `optimistic`'s rollback — right for
    one-shot actions, hostile mid-typing; the next push retries).
    `adopt(remote)` folds in mirror updates: an echo of our own push is a
    no-op, a clean local takes the remote edit, a dirty local wins
    (last-write-wins — `synced` still records the remote so the eventual
    push knowingly overwrites). `ui.bind_draft` is the input seam: user
    input pushes; adoption writes `local` and bypasses the push path; the
    local-echo write is deduped (`element.value() != value`) so the caret
    never moves. The commit parameter is `async |T| Option<str>` (J2's
    channel — an rpc-calling closure flows in legally; a sync commit
    passes fine), stored plain in the struct field (the marker doesn't
    exist on fields yet) and re-marked at a `let` in `settle`. Seven pins
    (six runtime semantics under node + the browser ui compile). Building
    it found B22. **Deferred tail (recorded):** auto re-push of dirty
    drafts on reconnect (today the next keystroke retries); a debounced
    variant (today one commit per input event).

---

## B. Type system & the type solver

3. **Variadic-generics deferred tail** (M–L; `variadic-generics.md` §Deferred) — shipped:
   flat-tuple lowering, mapped tuple types `(U in T: F<U>)`, tuple comprehensions, `combine`.
   ~~Enforcement of arity/element bounds~~ — **SHIPPED 2026-07-17**: tuple bounds
   check wherever trait bounds do (`check_generic_bound_satisfaction`, both call
   substitutions and construction sites): non-tuple values, arity outside
   `lo..=hi` (inclusive), element-bound violations per element via
   `satisfies_trait_bound`, and forwarded generics (satisfied only by a
   contained own-bound: range containment + same-or-subtrait element bound).
   Diagnostics carry the B12 "the bound is declared here" note for free (same
   constraint-id channel); 11 pins; spec §5.9's "parsed but not enforced" note
   replaced. **Not done:** `keyof`; spread parameters (`...items: T`); elision
   of the flat-tuple construction copy; trait-typed-value dispatch (B4).

4. **Trait objects / dynamic dispatch** (L; own proposal when demanded) — a value typed as a bare
   trait (`let x: Display = …`) is a clean compile error today (the silent-miscompile half was
   fixed). Making it *work* by value needs a runtime representation (a `(value, vtable)` pair /
   `Box<dyn>`-style) — a real language feature; nothing uses it today.

6. ~~**Closure-return element inference gap**~~ — **CLOSED, pinned 2026-07-14**:
   found already-fixed when scheduled (the B19 defer machinery — method
   resolution defers while a closure argument is unresolved — plus the
   2026-07-14 binder-window solver work closed it incidentally). Every
   recorded shape now types and RUNS: unannotated `xs.map(|p| p.name)` with
   member dispatch on the element, immediate chaining, map-of-map, nested
   accessors, struct-element maps, and both slot-grounded combinations
   including the exact deadlock reproducer from the reverted general fix
   (`List::new()`+`push`+`map().sum()`). 8 pins hold the family — this area
   regressed before, so each case stands alone.

8. ~~**Trait-argument binders**~~ — **FIXED 2026-07-14** (v0.5.0 arc):
   `impl X with Trait<type S: Bound>` registers with-clause binders exactly
   like subject binders (bound-less ones inherit the trait's declared bound
   for the position, deferred retrofit included), and the CALL binds them:
   `bind_method_own_generics`'s adopt-filter grew the declaring impl's binder
   ids (`impl_binder_generics` — subject args + recorded trait_args), since a
   with-clause binder appears only in parameter types and an argument is its
   one binding channel. Without that second half the program COMPILED and
   monomorphized `sink.put` to the abstract no-op (printed 0). 3 pins
   (explicit bound, trait-declared-bound inheritance, subject+trait binders
   composing). Unblocks trait-shaped visitors (p6-followups #2/#4).

9. ~~**Impl-binder declaration order**~~ — **FIXED 2026-07-14** (v0.5.0 arc):
   a bound-less binder whose subject is declared LATER registers fresh and
   retrofits the subject's bounds just before solving (`generic_bounds` link
   by ID only — the bound types themselves resolve later in build()), placed
   after import/use resolution and before anything types. Declaration order
   no longer matters; multi-bounds and enum subjects pinned too.

11. **`!` / `?.` deferred tail** (M; `try-and-lift.md`) — the operators shipped 2026-07-04
    (both slices + the stabilization arc: bang-directed return-position generics, closure-`ret`
    participation, user-`Lift` lowering). ~~Error conversion at the `!` boundary~~ —
    resolved EXPLICIT + shipped 2026-07-15 (§9; 7796628). ~~Expression lifting +
    applicatives~~ — **v1 SHIPPED 2026-07-16** (`expression-lifting.md`: slot-root
    lift regions over the std pair, lazy-right applicative, source-order eval
    hoisting, paren delimiting, 15 pins + corpus + docs; chain absorption REJECTED
    for soundness — `a?.b == None` keeps its meaning). Remaining deferrals:
    the bare-`?` TRAIT path (user `Lift` containers — today a clean error steering
    to `?.`), closure `!` (the RPC-handler follow-up; needs the `arg → Result`
    linkage design), and `Signal`/`Promise` `Lift` opt-ins.

28. ~~**Conditions are not type-checked**~~ — **FIXED 2026-07-16** (found the same
    day building expression lifting): `if 5 { .. }` compiled and branched on JS
    truthiness — any non-empty aggregate (an Option is a tagged array) always took
    the branch. Now every `if`/`for` condition rides a post-solve `bool` check
    (`prepped_conditions`, the B24 `&&`/`||`-operand pattern: a grounded non-`bool`
    rejects, spanned at the condition; `Never`/`any` pass by their own rules;
    generics stay lenient like the operator checks; match guards already had their
    own check). A lifted condition keeps its earlier, targeted walk-time message —
    the general check skips `LiftRegion` conditions to avoid the double report.
    6 pins (i32/str/Option `if`, i32 `for`, the full legitimate-shape positive,
    the `any` leniency); corpus byte-identical — nothing depended on truthiness.

12. ~~**Missing-impl bound dispatch emits the abstract method**~~ — **FIXED 2026-07-08**:
    `check_generic_bound_satisfaction`, a post-solve pass over
    `method_call_substitution` (the one channel every instantiation shape records
    into — free functions incl. explicit `f<Cat>()` arguments, method own-generics,
    impl-subject and trait-parameter bindings): every binding of a bounded generic
    must SATISFY the bound — a concrete type through an impl of the trait or any
    SUBTRAIT of it, a generic argument through its own declared bounds
    (bound-to-bound flow; forwarding through an under-bounded wrapper is rejected
    at the inner call with "add `: Trait`" wording — bounds must be re-declared,
    which is also what closes the nested-call hole: the transformer's inherited
    substitutions never cross an unchecked edge). Spanned at the full call.
    Eleven pins (free fn, method, multi-bound naming the missing trait, static
    channel, trait-default-without-impl, subtrait satisfaction, generic impl
    subject, rebounded forward, under-bounded forward). **Conditional-impl DEPTH
    closed same day** (4 more pins): satisfaction reconciles the impl subject to
    bind its binders and recursively requires each binder bound — explicit
    (`impl Box2<type X: Greet> with Greet`) or inherited from the struct
    declaration — to hold at the argument (`Box2<Box2<Dog>>` greets,
    `Box2<Cat>` errors; depth-capped, lenient past the cap). **The family is
    CLOSED 2026-07-08** (three follow-on slices, 17 more pins): construction
    sites check DECLARED bounds (struct literals via the initializer's solved
    arguments; enum-variant calls by locally reconciling payload types against
    argument types — partial variants check exactly what they bind), and bound
    trait ARGUMENTS match (`Feed<str>` no longer satisfies `F: Feed<i32>`;
    required args ground through the call's substitution / the construction's
    own bindings / the conditional impl's binder bindings, and errors read
    "does not implement trait 'Feed<i32>'"). The unbounded-forward gap got its
    ROOT fix too (same day): the initializer's second-chance FIELD-first
    reconcile binds a declared parameter from a generic field value (the main
    loop reconciles value-first, which grounds a value's inference slots but
    never binds the struct's parameter from a generic), and the enum checker
    types identifier arguments via `infer_type` (an identifier's own expr id
    carries no type entry) — both forwards now reject, pin un-ignored, enum
    twins added. Remaining leniencies, each deliberate: an impl reached via a
    SUBTRAIT keeps trait-level argument matching; generic-value
    bound-to-bound flow stays trait-level.

14. ~~**Context threading misses trait-default dispatch edges**~~ — **FIXED 2026-07-07**:
    the context pass adds trait-dispatch edges locally (coverage, backward needs
    propagation, and argument threading through dispatch call sites; the shared call
    graph stays untouched — it is also async inference's). `effect` moved onto the
    `Source` trait as designed; pin un-ignored. The fix EXPOSED a latent miscompile:
    `resolve_inherited_default` matched impl subjects by exact type equality, so an
    inherited default on a GENERIC subject silently bound to the trait's abstract member
    (B12's shape) — now nominal matching, pinned
    (`an_inherited_default_on_a_generic_subject_dispatches`).

15. ~~**Context-typed closure parameters**~~ — **SHIPPED 2026-07-07**
    (`proposal/ambient-owner.md` §5): `body: (|| void) context owner_scope` (multi:
    `context (a, b)`), a contextual keyword on parameter closure types. Injected
    literals defer (own hidden parameter instead of creation capture); calls through
    the parameter are reads (fenced when uncovered) and thread the argument; values
    flow only where threading follows (call / same-clause forward / `run` body);
    `run` accepts a matching annotated value. `std::reactive::run_with_owner`
    shipped on it. Also fixed: unused `Context::new()` emitted a dangling call.
    Deferred: clauses on `let`/return types; superset-clause forwarding.

13. ~~**A direct call on a closure-typed local doesn't type its unannotated
    parameter**~~ — **FIXED 2026-07-12** (the gotchas sweep; pin un-ignored +
    multi-param and mixed-annotation pins). Two fills: the call-subject
    Closure arm writes an Unknown parameter's shared type slot from the
    argument (in place, so deferred body constraints retry against it), and
    an unknown closure parameter used as a call ARGUMENT adopts the callee's
    declared parameter type when it is concrete (breaking the
    body-waits-for-param-waits-for-body deadlock; a generic declared type
    still defers to the closure's owning call, preserving the
    `count.derive(|n| format(n))` channel). ~~Residual: the first
    call site wins; later conflicting calls diagnose against it~~ — the residual's
    DIAGNOSTIC half closed 2026-07-16: a later conflicting call now names the
    origin ("inferred from the closure's FIRST call") and steers to annotating
    the parameter; first-call-wins itself stands (it is the design).

18. ~~**Calling a method-call result directly doesn't parse**~~ — **FIXED
    2026-07-12** (the gotchas sweep). A member now fuses at most ONE call;
    a further `(args)` is a direct-call POSTFIX on the chain result
    (`self.hook.read()(a, b)`, `handlers[0](x)`), analyzer-side support
    already existed (closure-typed call subjects). Corpus byte-identical;
    5 pins incl. the un-ignored original; the walkthrough and kolt dropped
    their bind-first workarounds. Found while pinning: tuple member access
    (`.0`) is UNIMPLEMENTED entirely — recorded as item 19 below. Original
    entry follows. — (S; pinned
    `#[ignore]`d; found 2026-07-11 in the Kolt pilot) — `func()(args)` parses
    (call-on-call-result), but `x.method()(args)` does not ("expected a method
    name after `.`") — the postfix grammar accepts a call after a call, but not a
    call after a METHOD call. Surfaced storing server hooks as `Shared<|..| R>`
    and invoking them (`self.hook.read()(a, b)`); the bind-first workaround
    (`let hook = self.hook.read(); hook(a, b)`) is pinned as the working shape.
    Small postfix-parser fix.

19. ~~**A bound is checked against a chained generic result before substitution
    resolves it**~~ — **FIXED 2026-07-11** (found the same day building the A10
    pins). The symptom — `current_path().map(|p| parse(p))` into
    `swap<T: PartialEq>` erroring "generic parameter 'U' is missing the bound"
    — was NOT the bound check running early: the `map` call itself RESOLVED
    with `U` unbound. A method's own generic fixed only by a closure
    argument's RETURN binds on the second `bind_method_own_generics` pass,
    but when the closure's body hadn't typed yet (its parameters were supplied
    by that same resolution attempt) the pass read the closure as
    `Unresolved`, silently skipped it, and the resolution completed — freezing
    `Generic(U)` into the call's substitution and return type. Downstream,
    the bound check correctly rejected the abstract `U` (and monomorphization
    would have dispatched `==` abstractly — the check was guarding a real
    B12-shape miscompile, so the diagnostic was right; the resolution was
    wrong). Fix: the method resolution now DEFERS when an own generic is
    unbound and a closure argument is still unresolved — exactly the retry
    the non-closure arguments (and the free-function path, which never had
    the bug) always had; the closure's type lands between retries and `U`
    grounds. The receiver-form red herring: a literal receiver with `U == V`
    masked it (the bug needs `U ≠ V`). Pins: the un-ignored browser pin +
    runtime dispatch through a derived `PartialEq` (both outcomes), the
    still-failing unmet-bound gate, chained maps (inside-out convergence),
    and a method-bound consumer.

26. ~~**Diverging match legs and if branches poisoned or mismatched the
    construct's type**~~ — **FIXED 2026-07-12** (the gotchas sweep; 5
    pins). `Type::Never` added (internal): `panic(..)`, `ret ..`, and
    `jump break/continue` now type as Never, which YIELDS in
    reconciliation and satisfies any expectation in comparison — a
    `None => panic("missing")` leg no longer absorbs the match into
    `any`, and a `ret` leg no longer mismatches. The transformer emits
    diverging leg/branch results as statements (`return e`, never
    `x = return e`). Spec §5.1/§5.11 updated.

19. ~~**Tuple member access (`.0`/`.1`) is unimplemented**~~ — **FIXED
    2026-07-14** (v0.4.0 bundle): number members ride the same
    `FieldAccessorConstraint` as named ones (the no-deferral pre-solve
    shunt they used to take is deleted); `pair.0.1` (lexed as the float
    `0.1`) splits into chained accesses at the walk; `TupleIndex(subject,
    flat_offset, flat_width)` does slot arithmetic over the FLAT storage
    with chained accesses folding onto the root, so nested writes hit the
    storage, never a resliced copy; multi-slot regions read as reslices
    and assign slot-by-slot (the const-eval interpreter's equivalence gate
    rejected splice-with-spread). 12 pins + a corpus golden; docs updated
    (values-and-types, gotchas, spec §5.9).

25. ~~**A bare `std::…` path in expression position panicked the
    compiler**~~ — **FIXED 2026-07-12** (found building the docs
    walkthrough app; pins: bare fn path, bare variant path, alias-qualified
    positive). `std::math::min(1, 2)` inline crashed: the namespace root
    isn't a binding, the failed head resolution left the path's type id
    UNMAPPED (the type-static-accessor loop's `_ => {}` arm inserted
    nothing), and the first downstream `get_type` unwrap-panicked. Fix:
    every walked type id now resolves (Unknown on failure), a non-module
    path head gets a real diagnostic, and `std`/`pkg` heads get a guiding
    one ("`std` is a namespace, not a value — import the module first").
    Bare-namespace expression paths remain UNSUPPORTED by design (qualified
    access goes through an imported module alias: `import std::math;
    math::min(…)`); supporting them directly is a possible H-series
    follow-up.

24. ~~**Primitive comparisons skip operand-type checking**~~ — **FIXED
    2026-07-14** (v0.4.0 bundle): the native fast path now checks
    `B = Self` with the right operand inferred against the left's type
    (so an unsuffixed literal adapts, `1i53 < 3` stays legal — the
    original third pin asserted rejection on the strength of "the same
    mix errors under `+`", which literal adaptation had since made
    untrue); `bool` has no ordering; `&&`/`||` take `bool`; ordering a
    user-defined type errors (see 25). 8 pins.

25. ~~**Ordering operators don't dispatch to `PartialOrd` impls**~~ —
    **FIXED 2026-07-14** (v0.5.0 arc): `< <= > >=` resolve `PartialOrd`'s
    `lt`/`le`/`gt`/`ge` — usually the trait DEFAULTS over the impl's
    `partial_compare`, recorded as `GenericDispatch::OnType` with the
    receiver bindings (the Gap-E inherited-default path method calls use)
    and re-dispatched at emission; an impl-declared override takes the
    `binary_op_dispatch` path; `T: PartialOrd` bounds take `OnConstraint`.
    Natives NEVER dispatch (std's numeric impls have default bodies written
    WITH the operators — dispatching a native into one recurses; found
    live). 6 pins; `started < deadline` restored in `docs/std/time.md`.

27. ~~**A bare type name is accepted in value position**~~ — **FIXED
    2026-07-14** (found while pinning §H.1's condition negatives). `let q =
    Point;` compiled clean, binding the constructor object; it also armed the
    condition-position trap — `if p == Point { .. } { .. }` parsed `p ==
    Point` (against the type object) and ran, trapping at runtime. Fix: one
    guard in the `prepped_locals` (bare value-name) resolution loop —
    `bare_name_not_a_value(subject_id, name)` rejects a name resolving to a
    non-value entity (`Expr::Struct`/`Enum`/`Trait`/`Generic`/`Module`,
    primitives included as source `external struct`s; `Expr::Macro` folded in
    from its old inline check), with a per-kind steering message. Values
    (bindings, functions — B20 coercion — enum variants) pass through. With
    the type name rejected early (`Expr::Error`), the misparse becomes a clear
    error spanned at the name instead of a runtime trap. 8 live pins in
    `inference.rs` (struct/enum/trait/type-parameter/primitive/module + the
    realistic condition misparse + two value-form regression guards) via a new
    `assert_fails_with` helper; ignored-pin ledger now EMPTY. One test fixture
    repaired (`an_iterator_protocols_next_call_colors_the_loop` parenthesized
    its struct-literal iterable, the §H.1 migration). Corpus byte-identical.

23. ~~**An `effect` closure's unannotated parameter doesn't ground from a
    generic signal's payload**~~ — **FIXED 2026-07-12** (the gotchas
    sweep; pin un-ignored; kolt + walkthrough dropped the annotations).
    TWO root causes: the inherited-trait-default method path recorded NO
    receiver substitution (the direct-impl path did), so a default's
    `|T| void` parameter typed abstractly — it now binds the impl's
    generics from the receiver and the trait's parameters through the
    impl's written trait arguments; and `resolve_match` didn't defer on a
    not-yet-filled closure parameter, binding pattern captures against
    the enum's RAW declaration (the C′-family deferral now covers match
    subjects). Original entry follows. —
    `entry: Signal<Option<Task>>; entry.effect(|current| match current {
    Some(let task) => task.name, .. })` errors "cannot access field 'name'
    on type T": the parameter types against the IMPL's abstract `T`
    instead of the receiver's `Option<Task>`. A counterexample to item
    13's "closures passed to methods work via reconciliation" — `map` in
    the same shape DOES ground (the task-list pages ride it), so the gap
    is specific to how `effect`'s parameter reconciles against the
    receiver substitution when the body destructures and field-accesses
    mid-resolution. Workaround (pinned passing): annotate
    (`|current: Option<Task>|` — what the kolt editor ships with).

22. ~~**Return-expectation generic inference re-bound the CALLER's
    generics**~~ — **FIXED 2026-07-12** (found the same day building A12
    `Draft<T>`; pins:
    `a_bounded_caller_constructs_an_unbounded_struct_via_a_generic_static_new`
    plus two-generic, nested-argument, and return-type-only-regression
    variants). `fun draft<T: PartialEq>(initial: T): Draft<T> { Draft {
    synced = Shared::new(initial), .. } }` errored "generic parameter 'T'
    is missing the bound ': PartialEq' required by this call". The
    return-type-only inference (the `let c: Cell<i32> = Cell::fresh()`
    gap-filler) collected "the callee's generics still to infer" from the
    SUBSTITUTED return type — but when an abstract argument has already
    bound the callee's `T` to the caller's `T`, the substituted return
    type's generics ARE the caller's; unifying those against the
    expectation (here the struct field's RAW declared type) merged a
    caller-keyed entry (`draft`'s bounded `T` → the `Draft` struct's
    unbounded binder) into the call's substitution map, and the bound
    check — which demands every keyed constraint's bounds of its value —
    then required `PartialEq` of the raw struct binder. Fix: the filter
    now collects generics from the DECLARED (unsubstituted) return type,
    so only the callee's own binders ever merge; the expectation-driven
    binding still lands for genuinely return-only generics (regression
    pin). Only vilan-fn callees were affected (external fns return before
    the merge — `Shared::new` alone never reproduced it; a vilan `new` in
    the same shape did). Debugging lesson: the first trace interleaved the
    MACRO-WORLD analyzers' output with the main build's (each world has
    its own id/source-id space) and manufactured a phantom "type names
    resolve to shifted declarations" theory — tag per-world output (or
    dump each world's source table) before reading a cross-world trace.

21. ~~**A dependency-package `[service]` consumer without a direct `std::rpc`
    import mistypes the generated `connect`**~~ — **FIXED 2026-07-11** (pin
    un-ignored: `a_library_service_client_compiles_without_an_rpc_import`).
    The true mechanism was NOT solver order-sensitivity (the earlier
    characterization): the compiler carries a Rust FIXTURE generator for
    `[service]` (for test stds with no rpc module), selected silently when
    the `service` macro isn't in the expansion scope — and its baked
    template had gone stale (it still produced the pre-K6 `connect`, whose
    `socket.connection`/`.transport()` against the new
    Result-returning `connect_socket` produced the exact error wall). The
    macro was missing because the DEPENDENCY-SURFACE load path never scanned
    for `[service]` — the third of three seed sites — so `std::rpc` wasn't
    loaded when the once-only macro registry was built; a consumer-side
    `std::rpc` import re-ordered the load and masked it. Fix: the dependency
    surface now seeds the rpc load like the entry and the load loop, and a
    REAL std reaching the fixture fallback errors loudly instead of silently
    generating stale code. Debugging lesson for the record: four template
    edits "failing identically" meant the edited template was never the one
    running — when errors won't move with your changes, suspect a TWIN
    (a fallback, a cache, a second generator), not the code you're editing.

20. ~~**A named function doesn't coerce to a closure parameter**~~ —
    **SHIPPED 2026-07-11** (`proposal/fn-coercion.md`; found the same day
    building A10). `signal.map(parse)` now works: a reference to a plain
    vilan `fun` coerces to a matching closure type — eta-equivalent to the
    wrapping closure, and on JS the function IS the value. Eligibility: not
    `external` (dotted globals lose `this` detached), not generic (no single
    value — which instantiation?), not a method (`self` capture = closure
    creation, B18-adjacent), not `async` (a call through a plain closure
    value isn't awaited — the J2 gap — so the value would leak a promise);
    context-reading functions stay rejected by the context pass's value-use
    rule. The return type is the declared one, else the body's inferred
    type. Implementation: symmetric `Function`↔`Closure` arms in
    `reconcile_type`/`compare_type` converting the signature and recursing
    (binding the closure side's generics — `|str| U` binds `U = Route`), plus
    the transformer's value-reference arm (`ensure_function_emitted` + the
    emitted name, as a call subject would). Ineligible functions keep the
    old mismatch error. 8 pins (arg/method-arg/let/field/return/Shared
    round-trip/void-handler/cross-module import) + 4 guards (mismatch,
    generic, async, context value-use); the router surfaces now use
    `current_path().map(parse)`.

17. ~~**A generic call in an `else`/`match` branch loses its type argument**~~ —
    **FIXED 2026-07-11** (found building `std::jwt` for the Kolt migration). The
    discovering case looked async (a generic decode after a branch-nested await
    emitting the EMPTY `Wire::deserialize` — a silent miscompile), but the root
    cause was STRUCTURAL and sync: the `if` inference arm propagated the
    expected-type constraint only into the `then` branch, so a generic call
    reached only through an `else` never received its expected type and left its
    type parameter unbound (dispatch then fell through to the trait's abstract
    body). `match` had the twin gap — it reads its expectation from the
    `expected_types` channel, which the `constraint` parameter alone doesn't feed.
    Fix: the `if` arm now infers EVERY branch tail against the constraint and
    unifies; a `seed_expectation` helper populates `expected_types` on branch/block
    tails so `match`-leg propagation and generic-call binding see it. `std::jwt`'s
    verify was reverted from its split workaround to the natural inline form (the
    proof). Two pins (else-branch, match-arm) + the async-shape pin un-ignored;
    85/86 goldens byte-identical (only crypto.js moved, from the jwt revert).

16. ~~**Methods on an ungrounded generic receiver typecheck nothing — silently**~~ —
    **SHIPPED 2026-07-10** (the full (b) fix). The class was WIDER than the item: probing
    showed even `mut a: List<i32> = List::new(); a.push("text")` passed, as did
    `Holder<i32>.replace("text")` and `Map<str, i32>.insert("k", "not an int")` — the
    method argument check (`resolve_method_arg_check`) reconciled against the RAW
    parameter type, and `Type::Generic(T)` reconciles with anything, so EVERY
    generic-typed method parameter was vacuously checked, grounded receiver or not.
    Three coordinated fixes, one mechanism each: (1) `MethodArgCheck` now carries its
    call id and applies `method_call_substitution` to parameter types before checking
    (`List<i32>.push`'s `item: T` checks as `i32`) — fixes annotated receivers, user
    generics, Map; (2) an empty `[]` literal mints a STABLE element inference slot
    (`list_element_slots` keyed by the literal's expr id, exactly `List::new()`'s
    mechanism) instead of erasing to zero-argument `List`, so pushes ground it and
    `mut a = []; a.push(10); a[0] + 1` finally works; (3) `resolve_slot_unification`
    now VERIFIES against an already-filled slot instead of no-opping (the receiver's
    `Unknown` slot records no reconcile binding, so fix (1) can't see this case) —
    first push wins, the second mismatched push errors at its argument. Subscripts on a
    still-unknown slot DEFER, and the end-of-fixpoint sweep turns a never-grounded one
    into I4's never-determined error (now also for unannotated `List::new()`, an
    improvement); `len()`-style methods on never-grounded lists stay legal (pinned), and
    typing is fixpoint-order-independent (a later push types an earlier guarded read —
    pinned). 12 pins; all 84 corpus goldens BYTE-IDENTICAL (no emission change — the
    world already type-checked cleanly under real checking); the playground repro now
    errors at `"some text"`. ~~An unannotated `Map::new()` stays loose~~ — **CLOSED
    2026-07-16** (verified live first: mixed-typed inserts compiled AND ran, reads
    came back under any annotation). The post-solve sweep now rejects any REFERENCED
    binding whose final type keeps a generic declared in ANOTHER file (`Map::new`'s
    `K` can never ground in user code) — general over containers, with three
    recorded bounds: a zero-reference binding is exempt (no uses = no vacuous
    checks, and a pattern capture cannot take the annotation the fix asks for — the
    walkthrough's unused `Some(let _note)` capture types abstractly, a B23-adjacent
    latent case that now surfaces only if used); only STRUCT-headed types reject
    (an enum keeping a payload parameter — `Ok("done")` with `E` never named,
    `let x = None` — is commonplace, its residual sits in the never-constructed
    leg; a possible future tightening); and a leak whose generic is declared in
    the SAME file is not caught (rare; the cross-file std-container case is the
    class).
    4 pins. Still recorded: grounding an empty literal from a LATER annotated use
    (`let b: List<str> = a`) is not taken (pushes and annotations are the grounding
    channels).

---

## C. Memory model — Phase 6+ tail (deferred; see `memory-management-impl-plan.md`)

1. **`Weak<T>`** (M) — non-owning handle for breaking `Shared` cycles.

2. **Dynamic rule-4** (M; **re-scoped 2026-07-09 by `proposal/view-invalidation.md`**) — the
   STATIC half (a mutating call on the viewed root: `a.remove(i)`, `a.push(x)`,
   `free_fn(&mut a)` — constant or dynamic index alike, via `&mut` conventions) moves into
   the rule-4 scan as event **E2** of that proposal; what remains HERE is the genuinely
   dynamic remainder — writes through ALIASED paths (two `Shared` handles to one cell) —
   runtime-check territory (generation counters / poisoned views), to be sized only after
   E2/E3 have been in use.

3. ~~**No-view-across-`await`**~~ — **SHIPPED 2026-07-09** with E2, both as events of
   `view-invalidation.md`'s unified model (one lexical-liveness scan, three events: E1
   reassignment — previously shipped; E2 mutating call on the viewed root, scalar roots
   exempt; E3 `await` while ANY view is live). Includes the signature rule (a
   suspending function takes no `&`/`&mut` parameters — sync callees stay free, which
   keeps the analysis local), the async-closure capture rule, wrapped-match-leg capture
   liveness, and loop-binding origins (also fixing E1's `for e in &mut a { a = [] }`
   gap). Sub-question answered: `Shared` is NOT exempt — though `read()` returning a
   COPY means only `write()`'s view fences `await` (value semantics made reads safe by
   construction). ~25 pins. A6's ground rule is in place.

4. **Deterministic destruction** (L) — scope-end destructors / `Drop`-equivalent.

5. ~~**Transparent-references remainder**~~ — **ALL THREE SHIPPED 2026-07-14**
   (`transparent-references.md`; pins in `inference.rs`, corpus in
   `test/transparent-references.vl`):
   - ~~**Scalar views don't auto-deref in argument position**~~ (found 2026-07-09 probing
     `view-invalidation.md`): `print(b)` for `let b = &mut a[0]` printed the raw
     `(base, key)` pair (`[ [ 99 ], 0 ]`) instead of the element. **Resolved by rejecting
     it** (the user's call: the language never silently converts view→value, so require the
     explicit `*`): a scalar view read in value position — an argument, a binary operand, a
     value binding — is now a compile error (`check_view_value_reads`, keyed off a
     `is_scalar_view_pointee` classification that includes `bool`), so the representation can
     never leak. Compound write-through (`s += 5`) stays sanctioned (excluded via
     `compound_reread_ids`).
   - ~~**Inline `Option<&mut T>` transient**~~: `match Some(&mut a) { Some(let x) => … }` now
     binds the capture to the view and writes through. `inline_wrapped_view_shape` +
     `inline_subject_wrapped_view_shape` recognize the direct, conditional (`if c { Some(..) }
     else { None }`), and forwarded-bare-view-parameter (`Some(p)` for `p: &mut T`) shapes;
     `transient_wrapped_view_calls` adds the constructor to `check_view_escape`'s sanctioned
     set (the transient never outlives the `match`, so a view of a local is sound). A *stored*
     `let x = Some(&mut a)` still escapes and is rejected.
   - ~~**`&mut bool`**~~: `bool` is a numeric enum, so it was excluded from `is_scalar_primitive`
     and took the aggregate view path (`Object.assign` — a silent no-op write). The view
     machinery now classifies pointees through `is_scalar_view_pointee` (structs via
     `is_scalar_primitive`, **plus `bool` via `bool_enum_id`**), so `&mut bool` lowers to the
     scalar `(base, key)` view and writes through.

---

## D. Language specification & documentation

1. **Write a language specification** (L) — a single source-of-truth document for the grammar and
   semantics, so grammar changes/issues can be checked against a definition rather than inferred
   from the parser. Should cover: lexical grammar, the full expression/statement/item grammar
   (reconciled with the chumsky parser and the formatter), the type system and the memory model
   (value semantics, second-class views, `borrows`, conventions), and the evaluation/lowering
   model. Becomes the reference solver and parser work is checked against.

---

## E. LSP & tooling

2. **LSP semantic highlighting** (M; roadmap #10) — semantic tokens, precision over TextMate.

10. **`pkg::<name>` collides with `std::<name>` for a local module** (S; found
    2026-07-11 in the Kolt pilot) — **FIXED 2026-07-14**. A client module named
    `ui.vl` imported as `pkg::ui::screen` failed to resolve ("cannot find in the
    imported path") while `std::ui` was also imported. Root cause: std modules and
    the entry's own modules registered into one shared `pkg` namespace map — last
    writer won. Fix: std is itself a package (its modules register under `std`
    only; every std source maps to it, so std's internal `pkg::` imports resolve
    within std), `pkg` holds only the entry's modules, and the primitive/`panic`
    capture map (`module_scopes`) is std-fed only — so a local `string.vl`/`io.vl`
    can't displace the real one either. Deliberate behavior change: `pkg::` no
    longer *accidentally* aliases std modules (`import pkg::time::now` without a
    local `time.vl` now errors). 7 pins in `module_resolution.rs` (core-module
    collision, layered `std::ui` collision, primitive-host and `io` names, the
    negative alias case, with-dependencies variant, std-file-as-entry).

9. **Richer hover tooltips** (user request 2026-07-10) — **(a)–(d) SHIPPED
   2026-07-14**: `Program.declaration_labels` pre-renders full signatures
   (params with names+types, returns, generics with bounds; declared `async`
   in the label, INFERRED async prepended by the server post-inference) and
   struct/enum declaration blocks; the LSP fences them as ```vilan code,
   surfaces the declaration's leading `//` block as prose (a TEXT-side scan
   above the name span — entry buffer or the source file read on demand — no
   lexer trivia side-channel needed after all; attribute/modifier lines
   between docs and the name are skipped), and keeps the platform-requirement
   paragraph. 5 pins. REMAINING slices: (e) constants show their value; a
   `///` doc-comment convention decision; `context` clauses in signatures.

3. **Fix per-analysis `Box::leak` + incremental analysis** (L; roadmap #12, caching Tier 2/3) —
   the leak grows each keystroke/compile; true incremental is blocked by global
   `entity_id`/`type_id` counters. **MEASURED 2026-07-17** (the `#[ignore]`d
   `measure_per_analysis_leak` in `vilan-lsp/src/document.rs`; run with
   `cargo test -p vilan-lsp -- --ignored leak --nocapture`): a medium
   std-using document leaks **≈43 KiB per analysis** (debug build, 200
   fresh-text analyses ≈ simulated keystrokes) — ≈42 MiB per 1000
   keystrokes. With the debounce collapsing bursts, an hour of heavy
   editing lands in the low hundreds of MiB: real, worth fixing, not
   urgent. The fix (arena/incremental) stays L-sized and deferred; the
   measurement is the harness to re-run against it.

4. **LSP sub-file incremental parsing** (L; roadmap #13) — tree-sitter-style reuse; chumsky is a
   batch parser, so this is the largest, lowest-priority LSP item.

5. ~~**Migrate the codegen-snapshot corpus into `vilan test`**~~ — **DONE
   2026-07-17**: the byte gate is now `crates/vilan-cli/tests/corpus.rs` —
   every `vilan/test/*.vl` with a `.js` golden builds through the CURRENT
   `vilan` binary (`CARGO_BIN_EXE`, exactly the command that generated the
   goldens, `VILAN_STD` pinned to the repo std) in a temp copy, and the
   `.js`/`.css` outputs must be byte-identical. Runs in the ordinary
   `cargo test` suite (~27s, 8-way parallel), so the by-hand loop — rebuild
   the debug binary, regenerate, `git diff` — is no longer how the gate is
   *checked*; a stale binary can no longer check goldens. Regenerating after
   a deliberate output change stays manual (and still wants a fresh binary).

6. **Diagnostics remainder** (M; what E1 left open when it shipped 2026-07-04) —
   - ~~**Buffer overlay for unsaved dependencies**~~ — **FIXED 2026-07-16**: a
     process-global overlay in the loader (`set_document_overlay`, canonicalized
     path keys; `load_package_module` consults it before the disk read — the
     content-addressed parse cache is untouched, an overlay only changes WHICH
     content loads). The server registers buffers on open/change (pre-debounce)
     and clears on close; `on_change` already re-analyzed dependents, so unsaved
     edits now propagate live. Pinned (disk-vs-overlay dependent analysis).
   - ~~**Async lifecycle harness**~~ — **BUILT 2026-07-17**: the publish bookkeeping
     extracted from the server into a synchronous planner (`publish.rs`:
     `PublishState::plan_publish`/`plan_close` return `(target, diagnostics)`
     actions; the `Client` only transmits). The property test replays
     open/edit/close sequences and asserts *published == fresh analysis* after
     every step, including explicit empties and close-clears-extras.
   - ~~**Shared-dependency last-writer-wins**~~ — **FIXED 2026-07-17**, same
     refactor: a target's published list is the deduplicated union of every
     open owner's group for it (`BTreeMap` owner order, so republishing
     without a change is byte-stable — C1). Fixing or closing one importer
     leaves the other's view of the shared module standing; the last owner's
     close publishes the explicit empty. Pinned in the lifecycle property
     test.

7. **Diagnostic span precision — the long-tail audit** (SUPERSEDED by the
   diagnostics standard, 2026-07-16: `proposal/diagnostics-standard.md` +
   `diagnostics-ledger.md` — 129/180 sites verdicted in six audit batches;
   the remaining 51 blanks + the "could not be resolved" cascade check are
   batch 7) — original entry: the harness and the top user-visible classes landed: `assert_fails_spanning`
   (exact-range span pins in the inference harness), and re-anchors for match-leg mismatches
   (→ the offending leg's body), struct-initializer field mismatches (→ that field's value)
   and unknown-struct (→ the initializer incl. its name), import root/segment errors (→ the
   segment), and `use` root/segment errors (→ the segment) — six span pins. Remaining: the
   long tail of the ~150 `diagnostics.push` sites hasn't been audited — when a coarse span
   shows up in use, re-anchor it and pin with `assert_fails_spanning`. The standard: point at
   the narrowest expression that identifies the problem (call-argument mismatches are the
   model).

8. **Boundary-crossing diagnostics — anchor at the user's code** (**CLOSED
   STRUCTURALLY 2026-07-16**, audit batch 5: diagnostics anchored in generated
   code re-anchor at the generating attribute via ExpansionOutput origins +
   derived_origins + both redirect channels, message-prefixed "in code
   generated by this attribute:"; pinned. The cross-source NOTE — pointing
   INTO std for a user-caused condition — remains the recorded refinement on
   the C3 mechanism.) — original entry: when the offending code is macro-generated or lives in `std`, the
   diagnostic points there and speaks internal vocabulary. The motivating case: an `[rpc]`
   method that awaited (before 2f08699 made the dispatch spine async) produced "`body`
   receives an async closure, but its type awaits nothing" — `body` being a `std` turn
   parameter three layers below anything the user wrote, with no span in their file and no
   mention of which of their methods caused it. The standard to build toward: a diagnostic
   whose primary anchor lands in std/expansion output from a user-caused condition should
   walk back to the nearest **user-code** anchor — the macro attribute/item that generated
   the code, or the user construct whose inferred property (async-ness, a type) infected the
   plumbing — and lead with that ("the route generated for `[rpc] fun add` is async because
   `add` awaits `sleep_for`; …"), demoting the internal frame to a secondary note.
   Ingredients that already exist: expansions know their originating item (the macro engine's
   provenance), `async_infer` knows *why* a closure is async (the infecting call chain), and
   `SourceId`s distinguish user files from `std`. Audit trigger: any diagnostic observed
   pointing into `std/src/*` or generated source for conditions the user created.

---

8. **LSP + editor support for the macro engine** (M) — **core shipped 2026-07-07**: the
   TextMate grammar knows the `macro` keyword, `macro fun` definitions, `macro name(..)`
   invocations, and generic line-anchored `[name(args)]` attributes; hover on `[name]` /
   `[derive(Name)]` / `macro name(..)` shows the macro's `macro fun` signature; go-to-definition
   jumps to the defining `macro fun`, cross-file into `std` for prelude derives (derive names
   now carry per-name spans; macro names live in a separate scope namespace so trait/macro
   name sharing resolves both ways). Remaining: completion offering registered macro names at
   attribute sites, and semantic tokens classifying macro names distinctly (see #2 above).

## F. Backend & platform

2. **Numeric types `u8`…`i64`/`f32`** (S; roadmap #15) — **SHIPPED 2026-07-07**
   (`proposal/numeric-types.md`): `i8`/`u8`/`i16`/`u16`/`i64`/`u64`/`f32` as nominal
   primitives collapsing to plain JS numbers — the 64-bit lowering PROFILED
   (f64+`Math.trunc` beats BigInt 5.2–14.1× on speed, 4× on memory; `BigInt` stays the
   exact escape hatch). With it, two semantic repairs: **truncating integer division**
   (`7 / 2` is now `3` — `Math.trunc` on every integer type, generic dispatch included;
   one corpus golden regenerated run-verified) and **range-checked integer literals**
   (suffix/annotation-typed, `-128i8`-style minimums admitted at `2^(n-1)`; 64-bit bound
   = f64's ±2^53 window, error names `BigInt`). Explicit `as_*` conversions with
   Rust-`as` fold semantics; Json/Debug/operator families mirror `i32` (generated once
   by a macro, checked in — `number.vl` loads inside macro worlds, which expand with an
   empty scope, so world-loaded std files must not dispatch; the flagship
   `numeric_family` macro lives on as a pinned test). `vilan/outdated/` pruned.
   Remaining (recorded in the proposal §7): wrapping arithmetic + real widths on a
   non-JS backend, `f32` fround, Wire slots, parse family, numeric→`BigInt`.

6. ~~**Tree-shake module-level bindings**~~ — **SHIPPED 2026-07-10**: module-level
   bindings are walked per-binding (in order, names stable) and included at ASSEMBLY
   only when something emitted referenced them (one chokepoint — the `Expr::Local`
   value arm; declarations emit through a different arm, so a binding never retains
   itself). The stated semantics landed: a dropped binding's initializer does not
   run — module state exists only if something reaches it. The acceptance test:
   `number.vl` now imports `std::math::PI` for `to_radians`/`to_degrees` (workaround
   removed) and every non-math golden stays byte-identical; the reactive goldens
   dropped their vestigial `const turn_scope = null` / `owner_scope = null` (already
   rewritten away by the context pass). Known over-approximations, recorded: a
   reference made inside a DROPPED binding's initializer still retains its target,
   and a function required only by a dropped binding stays emitted. Worlds
   (macro compiles) are untouched — cached, and correctness-first. Original:
   module-level
   `let`s emit unconditionally whenever their module loads, unlike functions (which the
   transformer already emits reachability-only). Two observed consequences: `number.vl`
   cannot import `std::math::PI` — every program would gain a stray `const PI`, since
   `number.vl` is always loaded (K2 worked around it by inlining the literal, with a
   comment at the site; remove the workaround when this ships) — and a DROPPED unused
   binding with a call initializer degenerates to a bare side-effect statement
   (`Math.pow(2, 0 - 52);` appeared in every golden from `EPSILON`'s initializer — the
   same shape as the fixed dangling-`Context::new()`, which was handled for the
   news-only path specifically). Wanted: extend the existing function-reachability walk
   to module-level bindings — emit a binding only when a reachable item references it,
   and drop its initializer with it. One semantics decision to state: a truly-unreferenced
   module `let` with a SIDE-EFFECTING initializer (`Shared::new`, `Context::new`) —
   today's live ones (`scheduler`, `owner_scope`) are referenced by any program that
   loads them, so reachability keeps them; declare unused-initializer dropping as the
   defined behavior (module state exists only if something reaches it) rather than
   promising top-level side effects.

5. **Project-model deferrals from P1/P2** (M) — registry-dependency loading (only `path`
   dependencies resolve today), `[project.dependencies]` inheritance, and P1's server-side
   manifest completions. (Captured here when the shipped `project-model-p1/p2` proposals
   were pruned — their full context lives in git history.)

3. **WASM backend** (L; far future) — the second emitter on the platform model's `Backend` axis
   (`Js` is the only variant today; `platform-model.md` §7.1 reserves `Wasm`). Three parts, only
   one of which is "codegen":
   - **Emitter** — Vilan's lowered IR → WebAssembly (via a `wasm-encoder`-style crate, or emit
     WAT). Most language constructs (functions, structs, control flow) lower straightforwardly;
     closures and generics (already monomorphized) are the work.
   - **Host-import seam** (`platform-model.md` §5) — a WASM module imports host functions
     differently than JS, so an `[extern]` binding may gate on **backend**: `http_sys.wasm.vl`, or
     a layer with `backend = ["wasm"]`. The *shared interface* is unchanged — only the `_sys` impl
     differs. Needs **backend-gating on layers** (`LayerDecl` carries only `platform` today;
     `Layer.backend: Option<Backend>` per §7.1) — the one piece of platform-model scaffolding
     deferred from the stabilizing slice.
   - **Memory-model lowering** — the model is GC-free by design
     (`memory-management-rev-1.md`, goal #1): values are scope-owned copies, views are
     second-class (never outlive a frame), and `Arena` owns its slots outright with
     generational handles — none of these need collection. What a non-JS backend needs is a
     linear-memory allocator, **scope-end destruction (C4 — the linchpin**, deferred today
     precisely because the JS GC makes deferral free), and an **ARC lowering for `Shared`**
     (+ `Weak`, C1, for cycles). This is the heavy part and is **shared with F4**; do it
     once. Targets both `browser` and `@process` (WASM runs in each).

4. **Native backend — server performance** (XL; far future) — a third `Backend` emitting native
   machine code, motivated by server throughput (no V8/JS overhead). For comparison, **Rust**
   lowers `source → HIR → MIR → LLVM IR → machine code`, with **LLVM** the default backend and
   **Cranelift**/**GCC** as alternates. A Vilan native path wants the same shape — a typed
   mid-level IR to lower from — and faces two choices:
   - **Backend infra** (cheapest → fastest peak): **emit C** (portable, leans on the C compiler;
     simplest to maintain — Nim/V do this) ▸ **Cranelift** (Rust-native, fast compiles, solid
     codegen; the natural fit for a Rust project) ▸ **LLVM** (peak performance, heavy dependency,
     slow builds).
   - **Memory model** — the central challenge (bigger than codegen), but smaller than
     "build a GC": the model is deterministic by design, so the lowering is allocator +
     scope-end drops (C4) + ARC for `Shared` (+ `Weak`, C1). A bundled tracing GC would
     *contradict* rev-1's goal #1 (deterministic, GC-free memory) and is not on the table.
     Shares the F3 lowering work.
   - **Standing cost:** maintaining ≥3 backends is a real tax (each language feature must lower to
     each). Gate this behind a **stable backend abstraction + a shared lowered IR**, and prove the
     seam with a *single* non-JS backend (F3) before committing to a third. Far future — flagged
     here so the IR/abstraction work that unblocks it is designed with this in mind.

---

## G. Macros

1. **General macro engine** (L; roadmap #9; **proposal: `macro-engine.md`; Phases 0–1
   SHIPPED 2026-07-06**) — Phase 0: the interpreter over the transformer's `js::Node` AST
   (`transform_to_ast`), the 70/70 equivalence gate, `macro_std`. Phase 1: `macro fun`
   items, per-file hermetic worlds (blanked-file compile against a macro_std-only
   workspace), `[name(args)]`/`[derive(Name)]` dispatch through `run_entry`, output
   splicing with depth-16 fixpoint, world + expansion caches; library-defined macros work
   (the exit criterion). Phase 2 (also 2026-07-06):
   `macro name(..)` invocations — item + expression position, shape-checked dispatch from
   the signature, `fresh()` gensyms stamped per splice site (capture pinned as a clean
   error), output previews in errors. Phase 3 UNDERWAY (2026-07-06): the
   builtin-derive channel (`std/derives.vl`, names reserved, Rust fallback for
   unmigrated/fixture stds) with `PartialEq`/`Default`/`Debug` migrated byte-identically.
   Derives COMPLETE (2026-07-06):
   all five migrated (`Json`+`Wire` together — one Rust contract — via str-returning
   helper macro funs); `Arguments` typed accessors shipped (construction API step 1).
   `[service]` migrated same day (the
   stress test passed: `Item::Service`/`ServiceItem` reflection with compiler-gathered
   rpc surface, cache keyed on struct+methods text, in-macro djb2 via new `str.code_at`;
   byte-gated on todo/rpc bundles). Scoped names + dissolution SHIPPED
   (2026-07-06): macro names are module-scoped (leaf imports; std prelude ambient; markers
   in the analyzer; lazy per-file worlds), `derives.vl` dissolved into
   compare/default/debug/json/rpc, outputs self-carry imports. The
   **construction API** (macro-engine §3, user request 2026-07-06): ~~`Arguments`
   typed accessors~~ (step 1, shipped 2026-07-06), ~~macro_std output builders~~
   (step 2, **shipped 2026-07-07** as `macro_std::build` — `quote`/`join`/
   `indent` + `impl_of`/`fun_of`/`match_of`/`struct_of`/`init_of`; all five derives and
   `[service]` rewritten against them byte-identically; exact-bytes e2e pin);
   ~~tree interchange~~ (step 3, **measured 2026-07-07 and NOT taken**: 0.8% of the
   rpc example's build parses generated text; a 240-expansion synthetic hits 39% of a
   188ms first compile, erased by the caches on re-analysis — batching parses is the
   recorded cheap alternative if it ever matters). ~~Ambient meta vocabulary~~
   (**shipped 2026-07-07**: the meta types + `source`/`fresh` are ambient in macro
   bodies via the world prelude; explicit definitions shadow; std macros dropped the
   boilerplate imports). ~~`macro { .. }` blocks~~ (Phase 4, **shipped 2026-07-07**:
   item-position comptime families + expression-position constant folding; blocks
   survive world blanking verbatim and wrap into synthetic `__macro_block_<n>` entries
   — true spans; 9 pins + the `macro-block.vl` corpus program).
   **G1 is COMPLETE** — the engine's remaining tail is macro-engine.md §11's
   explicitly-beyond-v1 list (semantic queries, quasi-quotation, compiled host,
   on-disk caching, batched parsing), each recorded with its trigger, plus the
   derive-name registration decoupling (deferred to the first user derive needing it).

2. **The `macro_std` output contract** (M–L; `proposal/macros-post-parse.md`,
   design complete 2026-07-16, **DEFERRED by decision** — explored, not ready to
   build): `Output` values over text `Source` (value-returning item builders with
   bulk list forms, expansion-scoped `uses`, quoted-expressions-with-handle-splices,
   semantic handles v1 against loaded modules), six open questions drafted, full
   normalization recorded as the §7 horizon behind the API-churn problem (candidate:
   a small versioned stable IR + generated adapters). ~~The DERIVE-IMPORT LEAK~~ — **FIXED
   standalone 2026-07-16**: generated items walk under a CHILD scope (expansion
   imports bind there, invisible to the module; the expansion's own references
   resolve via parent fallthrough) and the expansion's DEFINITIONS hoist to the
   module scope by node-level name (Func/Struct/Enum+variants/Trait/MacroFun/Let,
   unwrapped through Export/Derive/Service/MacroAttribute — import-bound names
   and definitions are indistinguishable by Expr kind, so the NODE shapes are the
   whitelist). rpc.vl's old leaked-name dependence turned out already gone (full
   suite green untouched). 2 pins (JsonValue no longer resolves; derived impls
   stay module-visible alongside an explicit import of the same name).

2. **`const` — compile-time evaluation** (M–L; `proposal/const-eval.md`, 2026-07-10,
   revised same day to the EXPRESSION form; **SHIPPED same day, v1 complete** —
   slices 1–4 plus the asset channel + const-only bit (`std::asset::emit` live only
   under `eval_const`; R-fixpoint over the call graph with roots-never-join, so the
   error sits at the outermost runtime crossing while const-chained property
   functions stay legal; channel dedups + lexically orders — sound for CSS, '.' <
   '@' — and `vilan build` writes `<output>.<kind>`; 7 pins + an end-to-end CLI
   test; A8's prerequisite is MET). Recorded: indirect-call conservative gap,
   run/watch asset writes, liveness-tied emission, Tier-2 memoization, deep spans.
   Slices 1–4 were:
   keyword/weak-precedence grammar, mark-and-forward analysis + the const-known
   free-variable rule with precise reference spans, the evaluation pass over const
   mini-programs (functions + external bindings + `__const_result`, assembled per
   expression, computed dependencies substituted by initializer id), and in-place
   serialization (`const.vl`: a compile-time-only function VANISHES from the
   emitted JS; 21 pins; 84/84 prior goldens byte-identical; refugee hint moved to
   the analyzer since `const x = 3` parses as `const (x = 3)`). REMAINING: the
   asset channel + const-only bit (the A8 prerequisite), budgeted-LSP memoization
   (Tier-2), deep failure spans. The styling system A8 is the forcing use
   case, independently motivated) — `const` is a weak-precedence expression keyword
   (`let x = const 1 + 2;` — captures to the bracket/comma boundary; `let NAME =
   const expr` IS the constant declaration, so bindings stay ordinary `let`/`mut`
   with F6/clone-site machinery unchanged, and `mut cache = const initial()` works).
   Evaluates with THE macro interpreter (one evaluator, no second dialect) and
   serializes the plain-data result IN PLACE (never worse than the computation it
   replaces; sharing = bind it). Free variables must be const-known (imports,
   literals, immutable bindings whose initializers are const — chaining; `mut`
   disqualifies); runtime captures error at the reference. **No `const fn` coloring** — the
   interpreter is total over the pure language (the Zig-shaped design; Rust's
   annotation burden avoided); reaching an unavailable capability, panicking, or
   producing non-data (closure/view/Shared) is a spanned static error at the
   binding. One new capability bit: **const-only functions** (std-internal, v1),
   enforced by call-graph reachability — the first is `std::asset::emit(kind,
   line)`, the **asset channel**: compile-time-accumulated build outputs,
   line-deduplicated, deterministically ordered, written beside the `.js` (CSS for
   A8; critical CSS for A7; any codegen later). Recorded v1 bounds: no const
   generics, binding-form only, assets emitted regardless of F6 liveness
   (liveness-tied emission = dead-style elimination, recorded). General payoff:
   lookup tables, precomputed scales, wire hashes (`contract_hash` de-magicked),
   parsed static config — all zero-cost at runtime. **Tooling: the LSP evaluates
   EXPLICIT consts** (opt-in, bounded, debounced, fuel-capped — `space(37)`
   squiggles live in the editor) **but never runs G3's inference sweep**
   (silent-fallback optimization = nothing to surface; build-only). `vilan check`
   evaluates as `build` does. The invariant that keeps LSP evaluation cheap and
   deferrable: no downstream pass depends on const VALUES (types are
   value-independent — the asymmetry with macros, which create items; also a
   second strike against const generics).

3. **Inferred `const` — automatic compile-time folding** (M; v2 of G2, recorded
   2026-07-10; design constraints in `const-eval.md` §5's recorded-v2 note) —
   `let a = 1 + 2;` folds without the keyword. No fundamental blocker. The
   soundness rules, settled up front: inference falls back SILENTLY on any
   evaluation failure, panics included (a dynamically-dead `xs[5]` must not become
   a compile error — explicit `const` remains the erroring guarantee); eligibility
   is the explicit form's (const-known free variables, the capability world,
   plain-data result); const-only functions NEVER infer (an asset-emitting style
   must not compile-or-not by optimizer mood — inference folds values, never
   creates const contexts). The v2-sized work is budgets: evaluation fuel (a
   missed fold beats a hung compiler) and serialized-size caps (don't inline a
   10 KB table nobody asked for), plus the `[build]`-preset split (debug = no
   inference for honest stack traces, release = infer). The LSP never runs the
   sweep — silent fallback means nothing to surface; build-only.

---

## H. Parser & grammar

1. **Struct literal as an operator operand** (S) — **SHIPPED 2026-07-14**. The
   operator/postfix chain is built over two operand grammars (`operator_tower` +
   `chain_expr_parser` parameterized by operand): ordinary expression positions
   admit struct literals (`Point { .. } == x`, both sides, inside `&&`/`||`,
   generic literals too), and the general postfix chain subsumes the old
   dedicated literal member-fold (`Point { .. }.sum()` — the special case is
   deleted). Condition positions (`if`/`for` conditions, `for .. in` iterables,
   `match` subjects) use the struct-free chain via the new
   `condition_expression`, so `if Foo { .. }` keeps the brace for the block —
   parenthesize a literal to use it in a condition (à la Rust). Corpus
   byte-identical; 10 live pins + formatter reformat pin. Found §B.27 while
   pinning the condition negatives (2 `#[ignore]`d pins ride with it).

5. ~~**The `%` remainder operator**~~ — **SHIPPED 2026-07-10**: truncated remainder
   (the dividend's sign — Rust's and JS's shared semantics) at every numeric type, plus
   `%=`, binding with `*`/`/` (left-associative), overloadable through the new
   `std::operators::Rem` trait (`impl T with Rem { fun rem(..) }`). Emission is the bare
   JS `%` with NO wrap at any type — unlike `/`, an integer remainder is always
   representable (magnitude < |divisor|, sign of the dividend), so i32/u32/i64 need no
   `Math.trunc`/`>>> 0`; BigInt `%` is native (the macro interpreter mirrors with
   `checked_rem` + the division-by-zero throw). The promised cleanup landed: `f64.rem`/
   `f32.rem` bodies and `fold_unsigned` (the as_* conversion folding) now spell `%`
   directly — their "vilan has no `%` operator yet" comment removed; only the
   `math.js`/`numeric-types.js` goldens moved (one line each, parity-verified). 8 pins
   (signs, floats, i64-exact, u32, BigInt, precedence, `%=`, trait dispatch) + corpus
   `remainder.vl` + TextMate `%`/`%=`.

2. ~~Block-scoped imports~~ — **shipped 2026-07-05** (kept as the design record; macro-engine
   §3 consumes it for `macro_std` resolution). `import`/`use` are statements, legal in any
   block (function/closure/if/match-arm bodies, bare blocks, impl bodies — an impl-scope
   import serves its methods); a binding is visible throughout its enclosing block and a later
   same-name binding shadows by overwrite — both **exactly `let`'s semantics** (vilan scopes
   are flat per block; use-before-`let` already compiled, and imports have no TDZ hazard since
   they compile to nothing). Not re-exportable: `export` in a body is a spanned error. The
   compiler previously PANICKED on a body import (no `Expr` for the statement id → transformer
   `unwrap`; now `Expr::Void`), and the loader only scanned top-level nodes — `Node::for_each_child`
   (the new exhaustive structural visitor, no catch-all) drives `collect_module_refs` at every
   depth, which also carries the P3 cross-target gates, the L1 lib-surface check, the §4.2
   contract check, and the LSP platform sniffer for free. Pins: 12 in `inference.rs`, corpus
   `scoped-import.vl`, workspace body-import + §4.2-at-depth CLI tests.

---

4. ~~**Triple-quoted strings `\"\"\"text\"\"\"`**~~ — **SHIPPED 2026-07-10** (semantics
   settled by the user, = Swift's multiline rule): the whitespace before the CLOSING
   `\"\"\"` is the indentation prefix, stripped from every content line (exact-character
   match, so a tab never satisfies a space prefix; a whitespace-only line may fall short
   and becomes empty); the newlines adjoining the delimiters belong to the syntax; the
   opening `\"\"\"` takes nothing after it on its line and the closing sits alone on its —
   both compile errors with PRECISE sub-literal spans (the offending text/line, not the
   whole literal), as is insufficient indentation (named by line number). The closing
   delimiter governs — not the opening's column as this item originally sketched — because
   `let s = \"\"\"` puts the opener mid-line where its column is meaningless. The body is
   RAW: no escape processing at all (`\n` stays two characters; braces literal) — the
   paste-code-verbatim appeal; content runs to the FIRST `\"\"\"` (no way to embed one —
   recorded limitation, plain strings still have `\"`). One trim/validate helper
   (`util::trim_multiline_string`, 12 unit tests) is validated in the analyzer (a bad
   literal degrades to `\"\"` so its uses stay typed under one diagnostic) and trimmed in
   the transformer — the VALUE flows to JS emission and the macro interpreter alike, so
   macros compose (`source(\"\"\"..\"\"\"`)` pinned), patterns match, i-string holes accept
   them, and `vilan fmt` reprints them verbatim (inner whitespace is semantic). 7 pins +
   corpus `multiline-string.vl` + TextMate rule. **The one recorded follow-up:** the
   interpolated variant `i\"\"\"..\"\"\"` (the macro-authoring payoff) still needs its
   escape story — raw braces vs `{expr}` holes conflict; settle it as its own small item.

6. **Handwritten recursive-descent frontend — replace chumsky** (L; recorded 2026-07-08; take
   it when the combinator model gives trouble, not before) — after the 2026-07-08 perf arc
   (the lexer-trivia quadratic + cheap-first/rich-fallback parsing, commits 5752f76/7b026bc),
   a cold compile is *still* ~95% lex+parse (todo client: 2.43B instructions; type solver 2%,
   macro interpreter 0.09%). What remains is chumsky's **structural** overhead, not a fixable
   pathology: `choice()` finds the right branch by attempting alternatives in order where
   recursive descent switches on the lookahead token; tokens are wrapped and compared per
   attempted primitive (`to_maybe_ref` + `Token::eq` ≈ 17% of the whole build); the
   precedence tower is a `foldl` chain; recursion is boxed. A handwritten frontend is the one
   remaining big multiplier: expect 3–5× on parse — todo ~0.43s → ~0.10–0.15s release — with
   the **debug binary** gaining most (4.8s → likely under 1s; deep combinator towers are what
   unoptimized builds execute worst), and vilan-core's own rustc build gets cheaper too (the
   grammar has been instantiated twice — cheap + rich — since 7b026bc; both towers dissolve).
   - **Speed is the bonus; control is the driver.** The friction is already visible in the
     grammar: the split-shift `try_map` hack, `<`/`>` as control tokens, contextual keywords
     (`context`) via ident-guards. Diagnostics are generated expected-lists (a broken shift
     names 15 candidates) where a handwritten parser gives curated messages. And a handwritten
     parser is fast AND rich in one pass, so `parse_clean`, `CustomParseError`, and the
     cheap/rich double instantiation all dissolve. Mature frontends (rustc, TypeScript, swc)
     ended up handwritten for exactly these reasons.
   - **Do NOT do instead:** another combinator library (winnow/nom — ~2–3× at best, loses
     chumsky's recovery + rich errors, so the hard parts get hand-built anyway: worst of
     both); Tier-2 on-disk/embedded std ASTs (obsoleted — a 5× parser makes cold std parsing
     ~50ms with no owned-AST lifetime surgery and no invalidation story).
   - **Proof strategy:** the corpus byte-gate pins acceptance; scale
     `tests/parse_fast_path.rs`'s tree-equality pattern into a differential harness — both
     parsers over corpus + std + examples, identical trees required. The true cost center is
     **LSP-grade recovery** (the `nested_delimiters`-equivalent partial ASTs the language
     server depends on): hand-designed sync points typically end up *better*, but they — not
     the grammar — are the work.
   - **Triggers:** release builds creeping past ~1s on real projects; LSP latency on large
     files; the next grammar feature that fights the combinator model. Best taken after D1
     (the language spec) exists to check the new parser against, and with the grammar stable —
     a rewrite chasing a moving grammar pays twice. Unblocks E4 (sub-file incremental
     parsing), which is impractical over chumsky's batch model.

---

## I. Collections

1. ~~**Struct keys for `Map`/`Set`**~~ — **SHIPPED 2026-07-14**
   (`proposal/hashable-keys.md`). `Map`/`Set` now key **by value**: a
   struct/enum/`List` key works with `[derive(Hashable)]` (or a hand-written
   `impl Hashable`), and a fresh equal key hits. A new `std::hash` gives a
   `Hashable` trait producing an opaque `Hash` (canonical key: a primitive
   as-is, an aggregate as its `JSON.stringify` string, via the `canonical_hash`
   intrinsic). `Map`/`Set` are vilan wrappers over a raw `NativeMap` that
   dispatch `key.hash()` — so a custom impl (hash-by-subset) is honored inside
   std collections too, not just user-built tables (bound on `K: Hashable`, key
   a `Map<Hash, …>`). The derive's recursive all-fields check rejects a
   non-`Hashable` field. Deferred: tuple *keys* / tuple *fields* (structural
   bound satisfaction, like `Wire`), a real hash table on a native backend
   (the opaque `Hash` seam makes it user-invisible), identity (`Shared`) keys.
   Two pre-existing analyzer gaps noted en route (bound mis-propagation through
   a two-param generic's argument; struct-literal fields don't direct a generic
   call).

2. **`[T; n]` — fixed-length arrays, the deferred tail** (M; v1 **SHIPPED 2026-07-15**,
   4bd978a + review follow-ups — `Type::Array(TypeId, usize)`, repeat `[v; n]`,
   context-directed `[a, b, c]`, index/view/copy/iteration, literal-OOB compile error;
   `.len()` fold **SHIPPED 2026-07-16**, 94fdd8d — pure subject folds to the constant,
   a side-effectful subject reads `.length` in place; `proposal/fixed-arrays.md`).
   Remaining, per §7:
   - **Const-named / const-generic lengths** (`[u8; SIZE]`, `<const N>`) — **blocked on
     design, not code** (recorded 2026-07-16 in the proposal §7): the "values in
     generics" model is under-specified — what `const N` *means*, whether any constant
     value can be passed or only plain data, and how to constrain one to a number
     (`<const N: i32>`). Plus the mechanical fork: `const_eval` is strictly
     post-analysis while lengths are needed mid-fixpoint (staged analysis vs. the
     cheap literal-initialized-name subset). Proposal work first.
   - `List` ↔ `[T; n]` conversions (explicit methods, no coercion); destructuring
     `let [a, b, c] = arr`; slicing (wants a range type); generic `[T; N].len()` → `N`
     (rides const lengths).

3. ~~**Validating per-type `from_json`**~~ — **SHIPPED 2026-07-14**
   (`proposal/validating-from-json.md`). The per-type surface was the last
   trusting decoder: a missing/mistyped field decoded to `undefined` and flowed
   on as garbage, and `from_json(text)` crashed on malformed input via
   `JSON.parse`. Now both `FromJson` methods return `Result<Self, str>` (matching
   `decode_json`): scalars validate the JSON type before coercing (new
   `JsonValue.kind()` intrinsic + `is_number`/`is_string`/`is_bool`/`is_array`),
   the struct derive presence-checks each field (naming a missing one) and threads
   leaves with `!`, the enum derive validates the tag, and `from_json` parses
   non-crashingly through `try_parse_json`. Composes with B11 `!`/`?.`. 13 pins +
   8 migrated tests + 3 goldens; docs updated. Deferred: JSON-pointer error paths,
   number-range checks, deduplicating the two readers (validating-from-json.md §7).

4. ~~**Subscript absence semantics**~~ — **SHIPPED 2026-07-10**: panic, checked at use
   and at mint. `a[i]` — read, write, or `&mut a[i]` — requires `0 <= i < a.len()`; a
   violation panics with "index out of bounds: the length is L but the index is I".
   Writes never create slots (growth is `push`); `get(i)` stays the total,
   `Option`-returning form. Emission is three self-contained helpers
   (`__at`/`__at_put`/`__at_view` — an assignment target can't be a call, so the write
   has its own) throwing the same bare-string shape `panic` lowers to; the macro
   interpreter enforces identical bounds as `Thrown`, so a macro-time violation fails
   the expansion with the same message. An indexing expression now counts as effectful
   in itself (it can throw), so unused-binding elision can't drop a check. A deref
   through an already-minted stale view remains C2's dynamic-rule-4 remainder — the
   mint check plus E2's static fence cover the lexical cases. The circular
   empty-literal message now says what's missing ("its element type is never
   determined"). F3/F4 alignment comes free: panic is exactly what a bounds-checked
   native subscript must do. Corpus impact was 6 goldens (parity-verified); the rest
   of the corpus iterates via `for`/methods and never raw-indexes. Original design
   space, for the record: panic (taken) vs `undefined`-propagation (status quo,
   rejected) vs bare reads as a compile error in favor of `get()` (rejected — hostile
   to the common in-bounds case). Surfaced 2026-07-09 by
   `proposal/view-invalidation.md` §1's P1 case.

---

## J. Concurrency

1. **Async/await remaining phases** (L; see the `context-async-plan` memory) — `context` (scoped
   value) landed and threads as a hidden parameter; the shared call-graph (Phase 0) is in
   `call_graph.rs`. The async/await execution-model phases remain. **Direction set
   2026-07-17**: `proposal/async-polymorphism.md` Part B — structured `scope`
   blocks (dynamic extent via `context`, join-at-exit, first-error-wins with
   absorbed rejections, cooperative cancellation honest about JS); Part C
   records the parallelism spine (sendability via value semantics + Wire,
   workers, future native fork-join) that the scopes must not foreclose.

2. ~~**Indirect calls are not async-inferred — no implicit await through closure values**~~ —
   **SHIPPED 2026-07-10**: `async || T` closure types. The marker is written at contract
   positions and only there (parameters and `let` annotations — the same policy as types
   generally: written at signatures, inferred at literals); it composes with the B15
   clause (`(async || T) context turn_scope`). A call through an `async`-typed value is
   an await point (async inference) and emits the implicit await (`maybe_await` covers
   `async_values` — one side-channel set, the `parameter_contexts` pattern; the solver
   never sees asyncness). The divergence check kills the bug class: an async closure
   flowing into a PLAIN closure parameter with a non-void return errors, naming the fix —
   while void-returning parameters stay legal as SPAWN semantics (fire-and-forget; the
   turns machinery settles the continuations — UI handlers and turn bodies ride this,
   pinned). `turn_async` and `optimistic` dropped the spawn-then-flatten workaround for
   plain awaited calls. Six pins. **REMAINDER CLOSED 2026-07-17** — the value-flow
   channels: the marker is accepted on STRUCT FIELDS (`async_fields`, keyed
   (struct, index)) and FUNCTION RETURN TYPES (`async_returning`); calls
   through a field read or a returned value await (`awaited_calls` → the
   transformer's non-Local subjects); unannotated bindings ADOPT asyncness
   from any held value — initializer or `mut` rebind — including async field
   reads, async-returning calls, and binding chains (`held_values` in
   async_infer, depth-capped). The divergence check now covers all three
   boundaries: parameter, field (literal + assignment), and declared return
   type — refused when the closure returns a value, spawn-legal when void,
   skipped when the return is unresolved (no known lie). 9 pins + docs
   (tour/async.md rewritten — the "re-mark at a `let`" idiom is obsolete;
   errors appendix). The asyncness-polymorphic remainder **SHIPPED
   2026-07-17** (`proposal/async-polymorphism.md` Part A, four slices
   ending 176fe8a): plain non-void closure parameters adapt per
   instantiation (the async_infer instance worklist +
   Program::adapted_instances; bits join the monomorphization key;
   sequential contract pinned), `sync |T| U` contract marker (contextual
   keyword; 11 std positions marked), void keeps spawn, async adapted
   instances iterate snapshots (`[...iterable]`), transitive adaptation
   works, refusals hold at `sync`/extern/dispatch/initializer boundaries,
   spec §7.4 rewritten. Concurrency = the spawn-then-settle idiom (helper
   surface still open). Part B seeds J1's scopes; Part C records the
   parallelism spine. Original finding follows. — async inference infects through DIRECT calls
   (`f()` awaits when `f` is async), but a call THROUGH a closure value or parameter
   (`body()` where `body: || T`) has no static callee, so it is never inferred async: the
   call returns the host promise at runtime while typing as plain `T` — the static type
   and the runtime value diverge until something awaits. Probed: `turn(policy, || {
   status.set(..); tick(); .. })` published the pre-await write immediately because
   `run`'s rewritten `body(value)` call was not awaited. Workaround (used by `turn_async`
   and `optimistic`, documented at both): SPAWN the call then await it — `let pending =
   async body(); await pending` — the host flattens promise-of-promise, so the await
   covers the callee's whole chain, and it is harmless for sync callees. A real fix wants
   closure TYPES to carry asyncness (an `async || T` closure type, inferred at the
   literal and checked at the call) so indirect calls await implicitly like direct ones —
   which interacts with B15 clauses (a clause-typed async closure) and the async-model
   phases above.

3. **Async calls in module-level initializers** (found 2026-07-13 while making
   initializers platform-colored) — `let state = ready();` where `ready` is async
   type-checked as `i32` but held a live Promise at runtime (`state + 1` was garbage):
   initializers are not `nodes()`, so `async_infer` never saw the call and nothing
   awaited it. **The diagnostic HALF is FIXED 2026-07-14** (v0.4.0 bundle): after the
   async fixpoint, every module binding's `initializer_calls_of` is checked — a call to
   an (inferred-)async function/extern, an async dispatch candidate, or an
   `async ||`-typed value errors at the call span ("a module-level binding cannot await —
   module initialization is synchronous; wrap the work in a function"); creating async
   closures/blocks at top level stays legal (3 pins). REMAINING (the design half):
   actually *allowing* awaited initializers implies top-level await (the emitted bundles
   are ESM, so TLA is available on every emitted host) plus an ordering story for
   dependent bindings — design before implementing.

---

## K. Std runtime

1. ~~**`Server` streaming responses**~~ — **SHIPPED 2026-07-18**: `Response` grew a
   body KIND (`Text`/`Bytes`/`Stream`) and a headers LIST (`set_header` is
   repeatable; empty = the old text/plain default), `ResponseBuilder` gained
   `body_bytes` + `streaming(on_open)` — `on_open` receives a live
   `ResponseStream` (`send`/`close`/`on_close`) once status+headers are
   written, and a suspending `on_open` runs as spawned work.
   `ServerBuilder::on_upgrade` mounts the WebSocket handshake. `Request` now
   pre-reads BYTES (`bytes()` for binary POSTs; `body()` decodes text on
   demand), so the binary `/rpc` leg fits the abstract surface.
   `serve_connected` moved fully onto `Server` (`connected_response` routes
   /events as a streaming response); the raw `node:http` seam note is gone.
   E2e: `tests/streaming.rs` pins the public surface (chunks over a held
   response, read back through fetch's body stream); the realtime-SSE +
   socket-robustness suites gate the moved mount. Also fixed en route: the
   J2-era single-header limitation (a response can now carry several).

2. ~~**Expand the std math surface**~~ — **SHIPPED 2026-07-09**: `std::math` (constants
   `PI`/`TAU`/`E`/`EPSILON`/`INFINITY`/`NAN` — EPSILON computed, the lexer has no exponent
   literals — plus the `Ord` free functions `min`/`max`/`minmax` MOVED from `compare.vl`,
   which had zero users and were latent-broken: primitives had no `Ord`); the f64 method
   family (trig + `atan2`, `exp`/`ln`/`log2`/`log10`, `cbrt`/`hypot`, `sign`/`fract`/
   `lerp`/`to_radians`/`to_degrees` — pi inlined there, `number.vl` must not import
   `math` or its module-level constants emit into EVERY program — `is_nan`/`is_finite`/
   `is_infinite`); sized-type parity (`abs` signed-only, `pow`/`min`/`max` everywhere,
   f32 mirror incl. `sqrt`..`trunc`); truncated `rem` on every numeric type (exact for
   ints — `/` truncates; the H5 stopgap). En route, three real fixes: the comparable
   primitives gained `Eq`/`PartialOrd`/`Ord` impls (ints + `str` + `BigInt` total; floats
   `PartialOrd` ONLY — the stated NaN answer: `partial_compare` is None for unordered,
   no total-order lie; hand-written, `number.vl` is world-loaded so no macro dispatch);
   the CONFORMANCE checker now credits a supertrait member provided by a SEPARATE impl
   of the declaring trait on the same subject (`impl str with Eq {}` no longer demands
   `eq` be restated — same-named members from unrelated traits still rejected; 3 pins);
   and the macro interpreter's host table learned the `Math.*` set + `Number.isFinite`
   (the corpus-equivalence gate caught it). Corpus `math.vl` (run-verified golden) +
   7 pins; existing goldens byte-identical. Original wanted-list follows for the record —
   today `number.vl` gives
   `i32` only `abs/pow/min/max` and `f64` adds `sqrt/floor/ceil/round/trunc`; generic
   `min/max/clamp/minmax` live on `Ord` (`compare.vl`); `std::random` exists. Missing,
   roughly in demand order:
   Remaining tail (deliberately not taken): per-type `MIN`/`MAX` constants (want a
   static-member story or per-type modules — neither exists; revisit with F5/spec work).

3. ~~**`std::crypto` — auth primitives**~~ — **SHIPPED 2026-07-11** (the pilot's
   first blocker, cleared): `std::crypto` (`hmac_sha512`, `pbkdf2_sha512`,
   `random_bytes`, `random_uuid`, constant-time compare — `crypto.subtle` glue as
   three `__`-helpers), `std::base64` (pure-vilan base64url over `Bytes`, so it is
   also const-evaluable), `std::jwt` (HS512 sign/verify over any `[derive(Wire)]`
   claim — a tampered or wrong-key token is `None`, never garbage). Vectors are
   RFC-checked: HMAC = RFC 4231 #2, PBKDF2 byte-exact against node. 5 live pins +
   corpus `crypto.vl` (interpreter-excluded — host capability) + `Bytes::to_hex`.
   Building it FOUND the async×monomorphization bug (B17); `std::jwt` is structured
   around it (crypto in a non-generic async helper, decode a flat match). bcrypt/
   argon2 and passkeys stay beyond-v1.

4. ~~**SQLite bindings**~~ — **SHIPPED 2026-07-11** as `std::db` over the host's
   BUILT-IN `node:sqlite` (node 22.5+; deno 2 via node-compat) — zero package
   dependency, so the jsr/npm proving-ground goal moves to whenever a dep is
   truly unavoidable (recorded). One compiler addition: the module-qualified
   constructor binding `[extern(new, "node:sqlite", "DatabaseSync")]` (New grew a
   module field, imported like Function's). Surface: `Database::open/exec/prepare`,
   `Statement::run` (returns the rowid) `/first/all` with positional `List<any>`
   params via `__db_*` spread helpers, `Row::text/integer/real/is_null` (three
   typed views over one column helper — trust the schema, guard nullables).
   @process layer: the client physically cannot import it — Kolt's principle made
   structural. 2 pins + corpus `db.vl` (interpreter-excluded). Recorded: i64
   rowids, transactions, deno-native verification.

5. ~~**`std::time`**~~ — **SHIPPED 2026-07-11** (kolt-migration.md §2.5; this entry
   was stale until 2026-07-16): `std/src/time.vl` — `now()` (host clock, correctly
   not const-evaluable), `Instant`/`Duration` (both Wire; epoch-milli `i53`),
   constructors/truncating accessors, `Add`/`Sub`/`PartialOrd`, `to_iso`,
   `describe()` ("2h 3m"), `sleep`/`sleep_for`. Docs `std/time.md`; corpus
   `time.vl` (node-run, interpreter-excluded — host clock) + 5 pins. Kolt has no
   live date call sites yet — grow-from-real-call-sites stands.

6. ~~**Transport robustness — Railway parity**~~ — **SHIPPED 2026-07-11**
   (`proposal/transport-robustness.md`; this entry was stale until 2026-07-16):
   `ConnectionState` signal, doubling-backoff reconnect (dial included), typed
   pending-rejection (`RpcError::Transport`), mirror re-subscription via
   `reattach_mirrors`; SIGSTOP/SIGKILL e2e. Found B21 (stale Rust fixture
   generator behind a missing dependency-surface rpc seed) — fixed same day.

## Distilled-file ship records, moved 2026-08-03

Full bodies of items that were fully SHIPPED/CLOSED in
`backlog-2026-07-18.md` as of the 2026-08-03 restructure, moved here
verbatim per that file's distillation convention (a one-line tombstone
stays behind, pointing here). Ids and titles are preserved as written;
organized by originating section for lookup.

### A. Reactive core & UI (`std::reactive`, `std::ui`)

9. **`vilan.toml [build] run` hooks — SHIPPED 2026-07-25 (v0.16.0 grind 5,
   f4c9dd6)** — string-or-list, before each build/round, sequential, manifest-dir
   cwd, ManagedChild teardown, failure fails the build naming the command;
   `check` runs none. Original entry: (S) — run external commands alongside
   `vilan build` / `--watch` (the Tailwind bridge, asset pipelines, codegen sidecars).


13. **Hot module replacement (HMR) — SHIPPED 2026-07-21, arc complete** (`hmr.md`
    is the record: ratified design + amendments + per-slice status). Six commits
    c5c0954 → 8c8ffc3: asset writes on the run paths, the SSE dev channel
    (reload/css/overlay), identity + fingerprints + adopt/expose emission, the
    state-carrying swap + `std::dev`, the coordination matrix + kolt proof +
    docs, and the post-ship stale-heal fix (swap-not-reload, caught by the
    first live-browser session). Remaining threads live where they're filed:
    residues in `hmr.md` §11 + amendments; split-out finds = B31 (tree-shake
    miscompile), A15 (multi-node `run`), E12 (watch-round compile caching),
    D3 (docs-harness fences). A7 (SSR) now has its identity + transfer
    groundwork sitting ready.


15. **`run` in a multi-node workspace — SHIPPED 2026-07-22, flag-only v1**
    (`vilan run --entry <name>` on every run path incl. HMR rounds; the no-flag
    2+-leg error lists candidates; non-selected node legs compile but never run
    and drive no restart; kolt-shape e2e pinned). ~~Recorded follow-up~~ the
    follow-up SHIPPED 2026-07-25 (grind 5, f4c9dd6): `default-entry` in both
    manifest shapes; --entry > manifest > lone leg > an error naming both
    ways — kolt needs no flag.


### B. Type system & the type solver

34. **a self-referential local initializer overflows the analyzer — SHIPPED
    2026-07-28, with same-scope shadowing** (`local-shadowing.md` is the
    record) — the crash was THREE shapes, all `Expr::Local` cycles recursing
    `view_binding_mutability` unboundedly: local `let x = x;`, module-level
    bare `let a = a;`, and module-level `let a = b; let b = a;` (the module
    shapes overflowed BEFORE B33's cycle check could speak — B33's pin used
    `A + 1`, whose initial is a Binary, not a Local). Two independent legs:
    (a) the copy-chain walk is now iterative with a seen-set (no cycle
    origin can overflow again); (b) local name resolution became POSITIONAL
    (user call 2026-07-28): a binding is visible from the end of its
    declaring construct, a later same-name `let` shadows from its point on,
    an initializer never sees its own binding. That also fixed a live
    miscompile the old final-map resolution hid: `let d = 1; print(d);
    let d = 2; print(d);` bound BOTH prints to the second `d` — a TDZ
    `ReferenceError` at runtime from a cleanly-compiling program. The spec
    (names.md §4.4) already promised point-of-declaration visibility; the
    implementation now conforms, and §4.4 states the same-scope rule
    explicitly. 16 pins (self-ref local + module shapes, rebinding runs
    1-then-2, block/param/for-item/match-capture/destructure shadowing,
    use-before-declaration note, closure capture of a later local rejected,
    module order-independence kept, unterminated-string-at-EOF salvage
    path). Residuals in `local-shadowing.md` §6 (B33-grade message for the
    bare module cycles; positional LSP completions).


35. **`std::style` breakpoint rules emit in an order that lets narrow beat
    wide — SHIPPED 2026-07-28** — the cause was not `render_rule` (a pure
    string formatter) but `assemble_assets`' one ordering rule: a lexical
    `BTreeSet<&str>` sort of whole lines, where `'1' < '6'` put `1024px`
    before `640px` — deterministic, universal, and exactly the hole the
    ui-styling proposal's promised "kind-specific rule" was meant to cover.
    Fix: media lines sort as a group by ascending numeric min-width
    (`em`/`rem` normalized ×16, foreign units keep lexical position);
    everything else stays lexical, so base < `:root` < media holds and all
    pre-existing output is byte-identical. 3 pins (raw-emit ascending order
    with widest-first collection, the `.sm(x).lg(y)` same-property field
    case, the renamed cascade-order dedup test) + the corpus golden grew a
    two-breakpoint style pinning the order in bytes; docs state the
    min-widths and the widest-wins guarantee; the website's
    single-breakpoint workaround can be unwound.


36. **the LSP red-flags process-twin-only names in two-entry packages —
    SHIPPED 2026-07-28** — the recon corrected the premise: the shared file
    was not analyzed "under one entry's platform" but under a platform
    INFERRED from its own imports (`resolve_project_context` yields None
    for a non-entry file in a multi-entry package), and `infer_platform`
    counted any import of a browser-layer module as browser evidence — a
    module in BOTH layers (`std::ui`, a twin) read as one-sided. Neither
    module-level answer works (ignoring twins would mirror the bug onto
    browser-only names like `mount`), so inference became NAME-level: a
    browser-exclusive module (`std::dom`) stays module evidence; a twin is
    evidence only through the imported names — one declared by just one
    twin says its side, one both declare says nothing; browser evidence
    wins a contradiction (old bias, kept). Twin scans ride the process-wide
    clean-parse cache. Pinned both directions in the LSP (shared file
    importing `render` analyzes clean; importing `mount` still infers
    browser). The per-entry-intersect design was rejected on measured cost
    (~88 ms per analysis, std-dominated — N entries would multiply the
    whole floor). Residual: a bare `import std::ui;` plus a qualified
    `ui::mount(..)` use names nothing at import time → infers Node and
    red-flags `mount`; take up only if it bites (the idiomatic form
    imports names).


37. **`std::ui` cannot build inline SVG — SHIPPED 2026-07-28** — `view`
    routes an exact-case SVG-vocabulary allowlist (`is_svg_tag`, browser
    twin) through the new `std::dom::create_element_ns`; the HTML-ambiguous
    tags (`a`, `title`, `style`, `script`) deliberately stay HTML, and the
    camelCase names (`clipPath`, `linearGradient`, `fe*`) were unreachable
    via `createElement` anyway (it lowercases). The process twin seeds
    `xmlns` on the `svg` ROOT only (descendants inherit; a component's own
    `xmlns` replaces the seed). The recon's bigger find shipped with it:
    `class`/`styled`/`bind_class` assigned the `className` PROPERTY, which
    is a readonly `SVGAnimatedString` on SVG — styled icons would have
    thrown in module code while passing every stub test — so all three now
    set the `class` attribute (identical on HTML; the twins now share one
    mechanism). Pins: the SSR differential grew an `svg>path` subtree with
    a namespace-cause check in the stub (namespaceURI, not the backlog's
    "client rect" — no harness in the repo does layout), `ssr-render.vl`'s
    golden carries the xmlns bytes, and codegen pins assert the
    createElementNS routing. Residuals: `show` drives the HTML-only
    `hidden` property (SVG ignores it — documented trap, toggle with
    `when`); the website's `<img>` icons can now be unwound to inline
    views (separate website change).


38. **salvage tail retention for semantic tokens — SHIPPED 2026-08-03** —
    the blank tail below a transient parse break keeps its highlighting:
    at analysis adoption the previous stream's tokens for the
    byte-identical, LINE-ALIGNED common suffix of old and new analyzed
    texts are retained (shifted into the new coordinates), and served
    ONLY under the salvage signature — the fresh stream entirely silent
    within the suffix — so a complete parse (including one that
    legitimately re-classifies identical tail text; semantics flow
    downward) always wins, and an EDITED tail line is excluded by byte
    identity itself. Retention chains across successive truncated
    analyses. Snapshot-consistency's law holds: retained tokens never
    cover changed text. Three pins (headline with a self-validating
    truncation premise; the edited-line honesty half; complete-analysis
    suppression), plant-proven. Break-shape reconnaissance: body-level
    breaks RECOVER (salvage is better than the entry assumed); the
    truncating shapes are top-level — a stray token, an unterminated
    top-level triple-quote, an unclosed brace. Original entry: (M; recorded
    2026-07-28 by `lsp-snapshot-consistency.md`, which fixed the *other*
    half of "highlighting breaks while typing") — H6 salvage keeps only the
    parsed PREFIX on some breaks (an unterminated triple-quoted string, a
    stray top-level token), so everything below the break loses its tokens
    until the text is whole again — a blank tail that reads as a bug even
    though the diagnostics are right. Snapshot consistency deliberately does
    not paper over it: serving the previous analysis's tokens for text the
    user has since changed is exactly the bug class that proposal removes.
    So this needs its own design — most likely retaining the last GOOD
    analysis's tokens only for the region above the break (where the text is
    still byte-identical), and blanking from the break down. Pin: a document
    that breaks mid-file keeps its tokens above the break across the
    recoverable-error round trip, and gains none below it.


39. **the LSP's request path is doing too much work — leg (a) SHIPPED
    2026-08-02** — the dependents sweep is gated on the real dependency
    edge: `Document::depends_on` answers from `Program.canonical_sources`
    (the recorded set of every file the last analysis loaded) through
    `same_file`, and `reanalyze_dependents` filters on it, so a typing
    pause re-analyzes only actual importers. The recorded pin holds both
    ways (an importer IS swept; a stranger is NOT — one analysis per
    pause, not two; spelling cannot fake a miss), proven non-vacuous by
    planted inversion, and the conservative arms stay: no program or a
    non-file URL sweeps as before. **Leg (b) SHIPPED 2026-08-02** —
    `semanticTokens/full` answers with a `result_id` and remembers what it
    sent (per-document cache, evicted on close); `full/delta` diffs against
    that baseline into ONE minimal edit in flat-integer units (zero edits
    for an unchanged refresh; an unknown id re-synchronizes full);
    `semanticTokens/range` filters the absolute tokens to the asked lines
    before encoding. Five diff pins + three protocol pins (id round trip,
    resync, range subset), non-vacuity by planted suffix-trim removal; the
    snapshot-consistency pin now compares token DATA (the id is fresh per
    response by design). **Leg (c) SHIPPED 2026-08-02 — B39 is COMPLETE** —
    sync is `INCREMENTAL`: `Document::apply_change` splices ranged events
    in order at UTF-16 positions (full-replacement events and manifests
    keep working), and each splice is RECORDED (`live_edits`, identity on
    adoption since `land` only ever lands on the live text, unmappable
    after any whole-text set). `Document::live_offset` maps an
    analyzed-space anchor through the record, which makes the inlay-hint
    viewport filter EXACT — a hint pushed down by an edit above is answered
    at its live line, not its stale one — with the old approximation kept
    as the fallback wherever the map is broken. Ten pins across splice
    shapes (ordered events, astral columns, multi-line, full-reset),
    mapping (shift, clamp, unmappable, adoption reset) and the handler
    (documents, manifests, the sharpened filter case — proven non-vacuous
    by a planted no-op map after the first cut passed vacuously through a
    second hint). Original entry: (M; recorded
    2026-07-28 alongside `lsp-snapshot-consistency.md`, which measured the
    behavior but fixed only correctness) — three compounding costs, in the
    order they bite: (a) `reanalyze_dependents` re-analyzes EVERY other open
    file, serially, on every typing pause, whether or not it imports the
    edited one — it should be gated on an actual dependency edge (each
    document's analysis already knows the files it loaded); (b) the server
    offers neither `semanticTokens/range` nor delta/`resultId`, so every
    refresh re-encodes and re-sends the whole file's tokens (the
    `result_id: None` today is what forecloses the delta path); (c) sync is
    `FULL` — the client resends the entire buffer on every keystroke. FULL is
    also why the inlay-hint viewport filter compares an analyzed-space
    position against the client's live-space range (no edit deltas = no
    mapping between the two spaces; exact for same-line edits, off by the
    inserted/deleted lines near the viewport edge until the refresh lands) —
    incremental sync makes positions mappable and that filter exact.
    Sequence: (a) first (biggest win, no protocol surface), then (b), then
    (c). Pin: a two-open-file workspace with no import edge between them
    performs one analysis per pause, not two.


51. **NEW — Requirement polymorphism: the owner fence follows instantiation
    chains — SHIPPED 2026-08-02** (M; proposal `requirement-polymorphism.md`;
    the H8 residual's recorded follow-up refactor, taken as its own arc).
    S1 (soundness, red-probed during the design recon): a closure-owned
    `OnConstraint` site contributed NO coverage edges — a Signal placed
    through it slipped the fence entirely (an 8d6980e regression, shipped in
    v0.21.1 AND v0.22.0; v0.20.0's union fenced it); and one covered caller
    laundered any number of uncovered top-level calls (the else-arm never
    consulted `top_level_targets` — verbatim back to v0.20.0 and before).
    Outside entries now force uncovered ahead of the caller-edges arm;
    closure-owned sites resolved. S2 (the walk): coverage resolves each
    call chain's recorded bindings through generic forwarders recursively —
    per-entry edges attach to the OUTERMOST resolving caller (attaching to
    the forwarder would let an uncovered static call poison a covered
    Signal call through the same helper — pinned by plant); a revisited
    `(function, constraint)` pair is skipped EXACTLY, so recursion needs no
    cap; unresolvable chains and value-taken/dispatch-reachable levels keep
    the union; `OnType` narrowing SHIPPED post-arc same day (proposal §8):
    a recorded receiver narrows by its HEAD (substitution cannot change it) —
    an unrelated needy impl under the same member name no longer fences a
    concrete receiver's static inherited default (red-probed); receiver-less
    sites (shared default bodies) keep the union. Three pins, one red-first.
    A merged explicit-generic-args channel was implemented and proven dead
    by plant (`method_call_substitution` already records explicit-arg
    calls) — removed. Thirteen pins across the two slices, five proven by
    red probe or plant; needs/strict/threading stay union-based (arity is
    instantiation-invariant — the transformer copies one parameter list per
    function). Docs: spec §8.3 dispatch-coverage paragraph rewritten (it
    still described the pre-v0.21.1 blanket union — a drift 8d6980e left),
    guide fence paragraph gains the generic-helper sentence.


49. **NEW — a composite holding a spanning element — SHIPPED 2026-08-01**
    (S; `proposal/composite-spanning-split.md`; this is 47's recorded residue,
    left unbuilt then because the mechanism was unclear — it turned out to be
    the seam rule's own mechanism over the element list) — a list or struct
    literal whose ELEMENT renders across lines splits regardless of width.
    `push(Subscriber { id, notify = || { … } });` closed on `} });`.

    ANY element, where the chain rule needs a NON-FINAL link, and the asymmetry
    is pinned rather than left to prose: a chain that ENDS at its spanning link
    leaves a clean line (the trailing-closure idiom), while a composite's
    closing delimiter always follows its last element — and usually an enclosing
    `)` and `;` after that — so it has no equivalent position. 4 pins in
    `formatter::composite_spanning_layout`, the two splitting ones proven
    non-vacuous per door. Measured: 3 files, 72 lines (`binary.vl`, `json.vl`,
    `reactive.vl`), idempotent, every site recovering the shape its author
    wrote. Imports and parameter lists are deliberately excluded — neither can
    hold a spanning element, so the rule would be unreachable there.


48. **NEW — a chain's `})` seam splits regardless of width — SHIPPED 2026-08-01**
    (S; `proposal/chain-seam-split.md`; grew out of 47's residue) — width was
    the only reason a chain ever split, so a chain that read badly without being
    WIDE was left alone and re-collapsed if hand-broken. A chain now also splits
    when a call link that is NOT its last renders across lines: the `})`
    -then-more-chain seam, where one line is the end of one argument, the start
    of the next link and the start of its argument at once.

    "Not the last" is the whole rule and was settled by measurement, not taste.
    Counting a spanning link ANYWHERE touches 8 files / 170 lines across std and
    examples; counting only non-final links touches 5 / 121, **none in std** —
    every std case the broad reading would have changed is a trailing closure
    (`Owner::take`, `Signal::sub`) that should stay put. Two earlier drafts died
    on measurement and are recorded in the proposal so they are not retried: a
    statement-level "rendering spans lines" trigger broke
    `let line = """…""" + "!";` at the operator (a string spans lines by its
    CONTENTS), and it also split the wrong construct in `push(Subscriber { … })`
    — the chain instead of the literal — so it did not even fix the case that
    motivated it.

    Spanning is MEASURED by rendering a link and looking, per the width rule's
    own discipline; probes restore output/cursor/bail/split and do not nest, so
    the cost stays linear rather than exponential on nested view trees. 5 pins in
    `formatter::chain_seam_layout`, 2 non-vacuous (the other 3 pin
    NON-splitting and cannot respond). `chain_splitting`'s
    `a_statement_that_already_spans_lines_is_not_split` was itself a seam
    fixture; it is now `a_spanning_last_link_is_not_split_by_width`, testing what
    it claimed to. **Not swept**: the 5 affected files are all examples, which
    stay queued behind 41 — they reflow when that sweep happens.


47. **NEW — five std files silently BAIL the formatter, and two idempotency
    pins are vacuous because of it — SHIPPED 2026-08-01** (S–M; found
    2026-08-01 while measuring 46)

    Fixed at the root, five causes, each pinned per shape in
    `formatter::bailing_constructs`: `TypeWithContexts` (`(|| void) context
    turn_scope`; one name reprints bare or parenthesized AS WRITTEN, since
    rewriting `context (a)` to `context a` would itself be drift),
    `MappedType` (`(U in T: Signal<U>)`), `TupleComprehension` (`(source in
    sources => source.get())`), `GenericParameter::tuple_bound` (`T: (2..)` —
    never printed at all, so `combine<T: (2..)>` reprinted as `combine<T>` and
    the drift bailed `reactive.vl`), and `Node::Void` in value position
    (`Verdict::Bad(void)` lost its argument; told apart from the parser's
    SYNTHESIZED empty-block tail by span width — written text has a non-empty
    span). Bails across std + examples + corpus + templates: 5 of 187 → **0**.

    Both tripwires now watch `formattable_files()` — corpus, std, examples,
    templates — and are renamed off `_over_the_corpus`, which is no longer what
    they cover. `assert_fixed_point` asserts non-bail first: verified
    non-vacuous by re-planting two of the bugs and watching `option_vl` and
    `reactive_vl` go RED, which they could not do before.

    Two residues recorded, neither a bail: (a) sweeping the five newly
    formattable files COLLAPSED two hand-split struct literals in
    `reactive.vl` whose opening line fits — `push(Subscriber { id, notify = ||
    {` — which is the documented "what fits stays inline" rule meeting a
    literal that holds a multi-line closure; whether a composite containing a
    spanning element should be forced to split is a design question, not a bug,
    and is NOT filed as one. (b) The examples are watched for bails now but
    still not swept for layout — that stays behind 41.
    — E13's zero-bail gate (`formatter_never_silently_bails`)
    covers `vilan/test` only, so nothing watches std. Detected by appending
    blank lines and checking they survive — a bail returns the input bytes:
    `browser/ui.vl`, `option.vl`, `process/ui.vl`, `reactive.vl`, `task.vl`.
    `reactive.vl` is reachable from a `(sync || T) context owner_scope`
    parameter type, which the printer cannot round-trip; the others are
    unidentified. The sharp end: `formatter::idempotency` asserts only
    `format(x) == format(format(x))`, which a bailing file satisfies
    TRIVIALLY — `option_vl` and `reactive_vl` are two of its twelve fixtures,
    so those pins currently prove nothing, and `reactive_vl`'s own comment says
    it is there to catch a dropped `[must_use]` tripping the safety net into a
    silent no-op, which is exactly the state it is in. Fix in two parts:
    (a) widen the zero-bail gate to std (and ideally examples), (b) make
    `assert_fixed_point` assert non-bail the way `assert_construct` does, so a
    fixture cannot pass by being untouched.


46. **NEW — `fun` signatures were outside the width rule — SHIPPED 2026-08-01**
    (M; `proposal/signature-layout.md`; the largest addressable category left
    after 44/45 — 16 of the 111 over-budget lines) — the width rule's first
    DECLARATION site. A signature over the budget now breaks one parameter per
    line with the return type, `borrows` clause and body `{` (or bodyless `;`)
    on the closing `)`; empty lists never break, closure parameters never break.
    Signatures carrying closure types are wide by construction
    (`serve_connected` at 172) and the author could not break them, because the
    formatter put them back. Mechanically it is 44's measurement finally being
    SPENT: the statement rule already measured a `fun` item's first line — that
    is what leaked into bodies before 44's fix — and `print_parameters` now
    consumes the permission the signature earned. 6 pins in
    `formatter::signature_layout`, 4 proven non-vacuous (the survivors pin
    non-splitting). Two std files swept. The call-argument asymmetry (R5 / 43)
    is stated deliberately in the proposal, not left implicit. Residue: 97
    over-budget lines, and one of the 16 was never reachable — the
    `context owner_scope` signature sits in a file that BAILS (see 47).


45. **NEW — imports were outside the width rule — SHIPPED 2026-08-01** (S;
    found by the same survey as 44) — an import's brace set is a list with
    braces and now breaks like one: one name per line one level in, trailing
    comma on every one, `}` at the opening line's indent, after the canonical
    sort so a split run is the sorted run. Both printers go through the new
    `print_import_statement`, because `organize_run` promises byte-for-byte
    agreement with `fmt` — split in only one and the editor action and the
    formatter rewrite each other on every save (pinned as
    `organize_imports_renders_a_split_run_the_way_fmt_does`, including the
    read-backwards case: pruning leaves back under the budget collapses the set
    to inline). The trailing comma is legal on both sides — the language grammar
    takes it, and so does the token-level `parse_token_branch` behind Organize
    Imports, which is what keeps a split run sortable. 5 pins in
    `formatter::import_set_layout`, 4 proven non-vacuous. One std file swept
    (`rpc_server.vl`).


44. **NEW — a spanning rendering exempted its whole statement from the budget —
    SHIPPED 2026-08-01** (S; a BUG, not a missing feature; found by measuring
    the 117 over-budget lines left across std/examples/test after 42) —
    `over_line_budget` required `!rendered.contains('\n')`, so any construct
    that opened a line and continued below it — a block-bodied closure, a
    `match`, a block — made the whole statement unmeasurable and therefore
    unsplittable. A `std::ui` tree ending in `.when(cond, || { … })` stayed
    inline at ANY width: `examples/reactive-ui/todos.vl`, hand-split by its
    author, reformatted to a single 707-column line, and nothing could break it
    again. Now the FIRST line of the rendering is measured, which preserves the
    property the old guard was protecting (measured width and described line are
    the same thing) without the exemption; body lines are measured where they
    are printed. Two std files were being flattened this way
    (`rpc_server.vl`'s `serve_connected` and `connected_response`). 3 pins in
    `formatter::spanning_renderings`, 2 proven non-vacuous; no existing pin
    defended the old behavior, which is why it survived the chain-splitting arc.
    Residue: 111 over-budget lines remain, 45 of them a single string literal
    wider than the budget (unbreakable by design, correctly).


43. **NEW — a statement's split does not descend into a call's argument, and
    `push(T { … })` is the shape that wants it — SHIPPED 2026-08-01**
    (`proposal/argument-tail-descent.md`; recorded 2026-08-01 alongside 42)

    `Split::Statement` descends through a call's LAST argument now, the way
    `Split::Tail` already did; the two permissions allow the same thing and
    differ only in where each is armed. TWO mechanical parts, and the second is
    the one that matters: `print_call_arguments` re-arming under either
    permission is a no-op on the motivating shape, because the callee of
    `list.push(…)` IS the member and `MemberAccessor` DROPPED the permission at
    the `.`. Both halves pinned independently — reverting either turns both
    splitting pins red, which is how the no-op was caught rather than shipped.

    Measured before landing, as the item demanded: 8 files, 63 lines across std
    + examples + corpus + templates, over-budget lines 94 → 81, idempotent. The
    item feared a delta "larger" than 42's and it is, modestly; it also warned
    the change "changes lines that have nothing to do with struct literals",
    which is exactly right and is the point — `std/rpc.vl`'s 221-column
    `match_of(…).arm(…)` and `std/hash.vl`'s 148-column
    `source("…" + impl_of(…)…)` are chains in an argument and break now too.
    R5 is untouched and pinned beside the new behavior: an EARLIER argument that
    is the over-budget cause still leaves a long line. Two pins asserting the
    old v1 boundary flipped (`a_chain_nested_in_an_argument_splits_at_the_tail`,
    and the mixed fixed-point file's nested-chain assertion).

    Original entry: — `Split::Statement` reaches the
    statement's value position but stops at a call's arguments (the v1 boundary
    the printer's own comment records), so a statement whose only breakable
    construct sits in an argument stays long however wide it gets. Kolt's
    `list.push(Task { id = …, … })` is 217 columns after 42 landed, and the
    shape is everywhere: one call, one argument, a literal inside it. The
    narrow fix is to let `Split::Statement` descend through a call's LAST
    argument the way `Split::Tail` already does — which is a real widening, not
    a special case, because it also lets a chain in a last argument break, so
    it needs the boundary restated and the corpus/examples delta measured
    before it lands (42's delta was 5 std files; this one will be larger).
    Do NOT fold it into 42's rule quietly: the two are separate decisions, and
    this one changes lines that have nothing to do with struct literals.


42. **NEW — struct literals were outside the width rule — SHIPPED 2026-08-01**
    (S; found by the Kolt migration, which produced a 357-column
    `KoltStore { … }` the formatter had itself collapsed) — the printer joined
    a literal's fields with `", "` unconditionally, so a hand-wrapped literal
    was flattened onto one line of whatever width it came to and, having no
    layout of its own, could never be broken up again. `print_split_struct`
    gives it the list literal's rule exactly — one `field = value,` per line
    one level in, trailing comma on every field of a split literal and none on
    one that fits, `}` at the opening line's indent, empty never breaks — so it
    composes with what was already there at any depth in both directions: a
    field whose line overflows splits as a nested literal, a chain, or a list,
    and a link's `Split::Tail` descent now finds a literal in a call's last
    argument (`.child(Card { … })`). 11 pins in
    `formatter::struct_literal_layout`, 9 of them proven non-vacuous against
    the planted bug; `cli.md` states the rule. Riding along: the field pair got
    the name `StructInitializerField`, and inline and split forms now share one
    `print_struct_field`, which is what keeps the two spellings honest.


41. **NEW — mid-chain comments orphan below the statement; attach them into
    split chains — SHIPPED 2026-08-01** (S–M; recorded 2026-07-28 by the
    chain-splitting arc, cost measured on the examples sweep;
    `proposal/split-comment-attachment.md`)

    Two rules, one mechanism, all five split forms — the item's own instruction
    to write the fix against the split construct generally rather than the chain
    specifically. (A) A comment in a GAP between elements forces the construct
    into its split form, because collapsed there is no line to keep it on; the
    gaps and not the construct's whole span, so a comment inside a closure body a
    link carries belongs to that body and forces nothing. (B) In a split
    construct a comment attaches above the element it precedes, at that element's
    indent, and a trailing same-line comment stays on its element's line — which
    is `print_items`' statement rule applied one level in.

    Boundary detail worth keeping: a comment before the FIRST element is in no
    between-elements gap, so the trigger needs the construct's own opening
    offset. Chains need none (their subject is element 0); struct literals,
    lists and parameter lists carry a span; an `ImportBranch::Set` carries none,
    so its extent is recovered from the source (`import_set_extent`, bounded by
    the previous `;` so a `{` inside an earlier comment cannot be mistaken for
    the set's). The same offset seeds the blank-line heuristic, which otherwise
    measured from offset 0 and prefixed a spurious blank line.

    The three orphan pins flipped together as predicted
    (`a_mid_chain_comment_attaches_to_its_link`,
    `a_comment_inside_a_nested_chain_attaches_there`,
    `a_comment_between_fields_attaches_to_its_field`) plus 6 new ones in
    `formatter::split_comment_attachment`; 6 proven non-vacuous. The nested pin
    got sharper on the way: the comment sits in the INNER chain, so only that
    chain is forced now and the outer stays inline — the rule reaches exactly
    the construct the comment is inside of.

    **The examples sweep is DONE** — 18 files reflowed, and the aggregate check
    that unblocked it holds: not one comment line appears in the diff, so no
    teaching comment moved relative to the code it teaches (`counter.vl`'s
    `bind_text` note now sits above its own link). std, the templates and the
    zero-bail gate are unaffected; `vilan/test` stays unswept as before, its
    `.vl` files being ungated (only the `.js` goldens are).

    Original entry: — the comment machinery flushes at
    statement boundaries, so a comment written between chain links reprints
    below the whole statement (pinned as `a_mid_chain_comment_moves_below_
    the_statement`): legal under E13's never-drop law, but the comment lands
    orphaned from the link it explains — `reactive-ui/counter.vl`'s
    `bind_text` note ends up dangling before the closing brace. Now that
    chains SPLIT, every link has its own line, so there is finally a place
    to put them: attach a comment to the link it precedes and print it above
    that link at link indent. **Scope extended 2026-08-01 by item 42**: the
    same orphaning happens to a comment written between a struct literal's
    FIELDS, and for the same reason, so the fix should be written against the
    split construct generally rather than the chain specifically (pinned as
    `a_comment_between_fields_moves_below_the_statement`; the two pins flip
    together). Consider the rustfmt rule while there: a
    mid-chain comment FORCES the split form even under budget (a chain that
    collapses has no line to keep the comment on). **The examples fmt sweep
    is queued behind this** — 17 files reflow cleanly except for orphaned
    comments; sweeping them before this lands would damage pedagogical
    placement (std's 5 carried no mid-chain comments and are already
    reflowed).


40. **LSP request handlers are not panic-fenced — SHIPPED 2026-07-28** (the
    blast-radius fix behind the const-preview `truncate` crash) — three
    layers, because the survey found the fence alone was half a fix:
    (a) `Backend::fenced`, one `catch_unwind` seam inside every request
    handler + `did_change` + `did_open`'s sync prefix — sound because every
    query handler's body is `.await`-free work over a DashMap snapshot.
    Policy: read-only queries answer their empty default; edit-producing
    requests (rename, formatting) REFUSE in the inline no-toast `-32803`
    spelling (an empty answer reads as "nothing to do", which a failure is
    not); `initialize` errors. The panic is never swallowed: the default
    hook writes payload+location to stderr (the extension's output channel),
    an ERROR `log_message` names the handler. (b) The seven `std::sync` lock
    sites (config, publish_state) + core's document-overlay and error-cache
    mutexes are poison-tolerant (`PoisonError::into_inner`) — otherwise the
    FIRST caught panic converts into a panic on every later request, an
    infinite crash the fence would have laundered. (c) Root-cause widening:
    core's `analyze_source` fence now covers lexing/parsing and the lift
    rewrite (a parser panic used to escape it), and `Document::analyze`'s
    big-stack thread catches instead of re-raising on join — a panicked
    analysis degrades to a no-program document with one honest
    internal-error diagnostic instead of aborting whichever handler joined
    it (`did_open` ran it inline). 2 pins via a name-keyed injection seam in
    `fenced` (a panicked hover answers None AND the next hover runs the
    normal path; a panicked rename refuses with the pinned code/message AND
    the next rename succeeds — the keeps-serving half is the poison test).
    Residual: `did_save`/`did_close`'s few sync lines before their awaits
    stay unfenced (their risky work — analysis, planning — is fenced deeper
    or poison-tolerant); a futures-level fence would buy little for a new
    dependency.


29. **Trait conformance is name-only; signatures unchecked — SHIPPED
    2026-07-20** (`deb4a1d`, before v0.12.0; this entry was never closed and
    still read "ARC ACTIVE" at the 2026-07-29 reconciliation, which is how it
    got re-recommended as open work) — full per-member conformance runs
    post-build in `check_trait_conformance` / `check_one_conformance`
    (analyzer.rs): receiver presence and convention, arity, per-position
    conventions and substituted types, return type, and a generic member's
    type-parameter COUNT, each with its own "match the …" steer and a
    cross-file "the trait declares `x` here" note. Types compare under
    {`Self` -> the impl's subject, the trait's generics -> the `with`-clause
    arguments} via `substitute_member_type`. C4 S2b's targeted
    `check_drop_signature` was DELETED as redundant (one mistake, one
    diagnostic); its epitaph is the comment in `check_drop_impls`. Asyncness is
    deliberately NOT compared (monomorphized dispatch), pinned by
    `a_declared_async_impl_of_a_sync_trait_method_is_permitted` — std's
    `SplitDuplex::send` depends on it. ~25 pins in inference.rs's
    `// --- B29: full trait-signature conformance ---` block; documented at
    `docs/appendix/errors.md` §Types and generics.
    **Residue (a) CLOSED 2026-07-29** — deep alpha-equivalence over a member's
    OWN generics: an impl that FIXED a generic position to a concrete type
    (`fun go<T>(&self, x: str)` against `fun go<T>(&self, x: T)`) was accepted,
    because an unmapped generic fell back to its constraint and an unbounded
    constraint compares equal to anything. Fix: `compare_type_rigid`, a
    comparison mode carrying the impl member's own generic parameters as RIGID
    (each matches only the identically-aligned parameter, never a concrete
    type), threaded through the structural recursion AND through
    `compare_argument_types`' own generic-is-a-hole leniency, so `List<T>` is
    not satisfied by `List<str>` either. `compare_type` is now a thin forwarder
    passing an empty rigid set, leaving its other 21 call sites untouched. The
    rigid arms sit BELOW the unknown/diverging arms on purpose: a position that
    failed to resolve stays lenient, so already-broken code gains no extra
    conformance error. Pin un-ignored, renamed
    `a_generic_method_fixing_a_generic_parameter_to_a_concrete_type_is_rejected`.
    **Residue (b) CLOSED 2026-07-28** (`0c7f603`, "closes its two generic
    residues"; this entry was never updated and still read OPEN at the
    2026-08-01 reconciliation) — the ambiguous `= Self`-defaulted position is
    compared at the declaration level (the recorded fix: conformance recovers
    the written spelling via `prepped_type_locals`, where a written `Self` and
    a written `B` are still distinct); the pin is un-ignored and renamed
    `a_self_defaulted_generic_position_with_a_wrong_type_is_rejected`, with
    companion pins beside it (inference.rs ~25398). Original residue: a
    `= Self`-defaulted trait generic (`trait Add<B = Self>`) interned `B` to
    the same TypeId as `Self`, so the position was SKIPPED (`self_ambiguous`):
    `impl Meters with Add { fun add(self, b: str) }` slipped conformance and
    only errored at use sites.


31. **module-level closure binding tree-shaken but still called — FIXED
    2026-07-22** (found 2026-07-20 by A13 S2a's probes). Root cause was NOT the
    backlog's reachability hypothesis: the assembly-time tree-shake filter
    (`referenced_globals`) was bypassed because the transformer's `Call` arm
    reads a `Local` callee subject directly (for intrinsic special-casing) and
    never recorded the reference the value arm records. One-line general fix at
    the Call arm; 7 sibling shapes were broken and are now fixed + pinned
    (initializer-only calls, transitive closure chains, nested-mod, `?`-region,
    `!`, `?.`-chain heads); argument position already worked (regression-
    pinned); precision guarded (`a_genuinely_dead_module_closure_is_still_
    tree_shaken`). The B31 pin is un-ignored.


32. **unknown value name types as `void` and cascades — FIXED 2026-07-22**
    (found 2026-07-21 by E7's cascade probes). One arm at the one site:
    `Expr::Error => Type::Unresolved` before `infer_type_path`'s void
    catch-all. The backlog's "type it Unknown" wording was corrected by
    evidence: `Unknown` EMITS at field/call-subject positions and grounds
    bindings through `resolved_types`; `Unresolved` never grounds and every
    downstream check already defers on it — the unknown-call path's exact
    machinery, now shared. 8 pins (per-position silence, both-roots,
    sibling-inference, generic-arg, closure-capture, unknown-call re-pin,
    non-function-call guard); the un-ignored B32 pin holds at exactly one
    diagnostic. A leftover cascade echo on the call path ("cannot call — it
    is void") vanished with it, verified never independently load-bearing.


33. **module-level binding emission order — SHIPPED 2026-07-25, arc
    complete** (`b33-emission-order.md` is the record: ratified rule +
    premise corrections + per-slice status; f9dec2f the load-time relation
    and dependency-ordered emission, 3f82aa2 the cycle diagnostic, + the
    S3 spec/pin/perf close). Bindings initialize in dependency order over
    load-time evaluation (closure creation inert — EVEN/ODD legal),
    canonical tie-break, cycles = compile errors with the round trip
    named; import order can no longer change behavior OR bytes; math.vl
    reformatted (golden-neutral); spec §7.1/§7.6 amended. The original
    entry follows for history. — (M; found 2026-07-22 by WO-1b's probes) — imported
    module-level constants emit in the order names appear in a `{..}` brace
    set (`module_level_bindings` iterates the insertion-ordered scope map),
    so (a) reordering names inside an import's braces churns emitted JS —
    the one residual import-order sensitivity after WO-1b's canonical module
    walk (pinned OUT of scope in `emitted_js_is_independent_of_import_order`'s
    docstring; `math.vl` left unreformatted because of it), and (b) the
    PROBED latent hazard: a module-level binding whose initializer reads a
    binding that emits later hits a TDZ `ReferenceError` (JS `const` doesn't
    hoist) — constructible today at HEAD with an adversarial cross-module
    shape, though no std/corpus code triggers it. The naive fix (sort by id
    or name) MISCOMPILES the same way — probe-proven. Root fix: emit
    module-level bindings in dependency-respecting (topological) order over
    their initializer references, cycles diagnosed; byte-stability falls out.


52. **any entry global resolves through any std module path — FIXED
    2026-08-03** — member lookup in namespace scopes now consults the
    scope's OWN declarations and re-exports only (`member_in_namespace`,
    the shape the `use` loop always had), at all three chain-walk sites:
    the import segment walk and both module-member accessor arms. This
    also deletes the backwards memo write (an entry id cached into a std
    scope — the S3 inventory's one cross-generation write). Pinned
    red-first in inference.rs for both the member form and the import
    form; module_resolution 43/43, docs, and the full suite green — no
    legitimate path relied on the chain. Original entry: (S–M;
    found 2026-08-02 by S3's mutation sweep, repro'd live) — `fun helper()`
    in the entry, then `math::helper()` compiles: "no errors". Two
    ingredients: every std module scope's parent is the global scope, and
    the module static-accessor arm resolves members via the scope-CHAIN
    lookup (`try_get_expr_id_by_name`), which walks out of the std module
    scope into the global scope where entry names live; name-resolution
    memoization then caches the entry id INTO the std scope (the one
    backwards-generation write in the S3 inventory). Fix: module-member
    access resolves against the module's own declarations only, and the
    memo becomes generation-aware (or skips cross-scope hits). Pin the
    repro red-first; watch re-exports and lib.vl surface members as the
    edge cases. Must be fixed regardless of S3 — it is a live language
    bug, not an optimization concern.


### C. Memory model

4. **Deterministic destruction** (L; **SHIPPED 2026-07-19, Tier 1 complete S1–S5** —
   ledger in `destruction.md`'s status block; Tier 2 = the counted class, specified in
   §10, builds with the native arc — **was the keystone; the memory model is now
   CLOSED per `claims-and-epochs.md` §4**) — scope-end destructors. The accepted design: a two-world
   partition — data stays rule-1 copying, untouched; a small **affine resource class**
   (declared `resource` + recursive containment) moves on binding/`own`, loans through
   the existing view conventions, and drops at its owner's scope end via
   `Drop { fun drop(&mut self) }` (Tier 1, on JS). `Shared`/`Owner`/`Disposable`
   deliberately stay data in v1; the counted class (`Shared` retain/release, `Weak` +
   `get`, counted closure environments) is Tier 2 — specified in `destruction.md` §10,
   built with the native arc. The design docs supersede this entry's earlier sketch.
   Unblocks C1, J4's free-spawn lint, real teardown in std, and the memory-model half
   of F3/F4.


6. **Geometry-effect inference — `bumps`, the twin of `borrows`** (**SHIPPED
   2026-07-19** — `rule4-completion.md`, commits d595ed2 (the effect) + 9b5e3cf (E2
   keys off it; both recorded E2 conservatisms cleared); residues recorded in the
   plan: generic verdicts are instantiation-independent-conservative, view-binding
   arguments under-approximate — both wait on no driver) —
   infer per function whether each `&mut` parameter (receiver included) is
   *content-stable* (field/element writes only) or *bumping* (may resize / reassign /
   drop through it). E2 keys off "`&mut` convention" as a proxy for "may bump the
   root's epoch", and its two recorded conservatisms (scalar-field view under a
   `&mut s` call; generic-typed roots) are exactly where the proxy is coarse; the
   inferred effect makes rule 4 precise (relaxation only — accepts more programs,
   changes no runtime behavior) and is the event classifier Tier 2's trap law and C2's
   generations key off. Fifth verse of the inferred-effect worklist (async, platform,
   contexts, `borrows`).


7. **Wire-blessed handles — SHIPPED 2026-07-25 (v0.16.0 grind 5, f4c9dd6)**
   — all three ratified conditions: [derive(Wire)] on Handle<T> (compiler gap
   = is_wire_type's applied-accessor arm), per-session arenas documented as
   the default, the optional Arena::branded() confusion guard (the authorized
   arena.js golden: the brand's complete 4-site footprint, node-verified with
   an in-pin unbranded control). Original entry: (S–M; **decision made 2026-07-18:
   bless**, under `claims-and-epochs.md` §6's conditions) — `[derive(Wire)]` on
   `Handle<T>` + the documented idiom: a server-side arena whose handles flow to
   clients as stable entity references (`Draft` targets, router entities, "update node
   X" RPCs); stale-handle → `None` becomes the distributed staleness story. Conditions:
   per-session arenas as the blessed default, an optional per-arena random `brand` for
   anything cross-tenant, and the derive must tolerate `Handle`'s phantom type
   parameter. Std surface + docs; no semantic change.


8. **`Arena.get` view-form migration** (**SHIPPED 2026-07-19** — `get(&self, handle):
   Option<&T> borrows self`; `Slot.value` unwrapped to `T` because rev-1's literal
   pseudocode silently emitted a COPY (`Some(inner)` vs the recognized `Some(&place)`
   leaf); `set` stays; the arena.js golden change authorized + verified; docs updated,
   incl. a phantom `get_mut` in the tour. The migration EXPOSED item 10.)


10. **`borrows`-returned views are invisible to rule 4** (**SHIPPED 2026-07-19** —
    `rule4-completion.md`, commits 144d44e (root-set, + a chain-miscompile fix) +
    9b5e3cf (anchoring: call results + wrapped captures + the ForEach chain; ZERO
    std/corpus/docs/kolt fallout); the arena pin un-ignored; `Weak.get` (C1)
    inherits the anchoring by construction; was found 2026-07-19 by C8's rule-4
    pin as the biggest open static-model hole) — `compute_view_origins`
    tracks only direct `&place` bindings and view→view copies, and `Function.borrows`
    is a bare bool recording no projected root — so a call-returned view
    (`let v = list.at(0)`) or a wrapped-view `match` capture is never anchored, and
    E1/E2/E3 all miss invalidation under it (`list.push(x)` while `v` lives compiles;
    likewise across `await`). Pre-existing since the E-checks shipped (their pins only
    exercised direct views); semantically empty on JS, a hole in the single-conformance
    guarantee for native. Fix: the inferred `borrows` summary carries the projected
    parameter ROOT-SET; `compute_view_origins` seeds call results (mapping roots
    through the call's arguments) and wrapped-view captures; then E1/E2/E3 anchor as
    designed. Overlaps C6's `bumps` machinery (same summary). **C1's `Weak.get` ships
    the identical signature and inherits this until fixed.** `#[ignore]`d pin:
    `arena_mutation_under_a_live_get_view_is_rejected`.


### D. Documentation

2. **Docs book Phase 3 — SHIPPED 2026-07-12** (this entry was never closed
   and still read NEW at the 2026-08-01 reconciliation) — both halves are
   long since real: the guided walkthrough (`docs/guide/walkthrough.md`,
   backed by the tested `examples/walkthrough` app; `bb98564`) and the
   glossary (`docs/appendix/glossary.md`). `documentation.md`'s Phase 3 line
   records the same. Original entry: (S–M; split from D1 — the spec itself
   shipped, §1–§11; plan in `proposal/documentation.md`) — the guided
   walkthrough expansion + the glossary.


3. **docs-harness fence extraction — SHIPPED 2026-07-22** (CommonMark §4.5:
   same-indent close + `min(leading, N)` dedent, spaces-only documented; applied
   to BOTH extractor copies with reciprocal drift notes, and a latent copy
   divergence closed — the sweep's copy now enters fence-state for every fence
   so an embedded ```vilan-looking line can't split them; 10 unit pins + a real
   bullet-indented `map` example as the end-to-end proof, 86→87 compiled
   examples; all pre-existing content proven byte-identical under the new
   rule — no page had a silently-swallowed fence).


4. **README refresh — SHIPPED 2026-07-22** (`b1f42b8`, + `156166b`), **tail
   swept 2026-07-29** — every scoped element is present: the pitch, the dev
   loop (HMR, build speed, the full editor-feature list), the memory model's
   one-law story, install + quick start (curl/irm/from-source, `hello.vl`,
   `vilan init … && vilan run .`), and the book link. Pseudonym discipline
   held (no author name) and no roadmap promises (the status block disclaims
   stability outright). The 2026-07-29 reconciliation found and fixed three
   factual drifts the refresh had outlived: the "links (book, **changelog**)"
   half was never added; the repo-layout line still described a
   `.github/workflows/docs.yml` that `0a0bdd4` DELETED when the site repo took
   over the docs build; and the `cargo test` comment claimed the suite covers
   "examples" when only `ssr` and `walkthrough` are built by it (see E22).
   `upgrade` was also missing from the CLI list. Original entry: (S; user
   request 2026-07-22) — the repo README trails the project.


6. **Docs approachability walk-through — CLOSED 2026-08-03** (audit half
   2026-07-28; judgment tail done 2026-08-03): hello-vilan's Projects
   section shrank to the two basic manifests + the src layout, with the
   deep vocabulary (git dependencies, pre-build commands, `default-entry`,
   workspaces and root-dependency inheritance) moved to a new
   `tour/projects.md` sidebar page; the OwnedNursery density finding was
   resolved by the docs restructure that moved it into `tour/resources.md`
   (verified reading well — drift in our favor for once); the spec pages'
   unreachable `proposal/*.md` citations became absolute repository links
   (six sites, matching README.md's existing pattern); the example-README
   jargon item rode D7 (done 2026-08-03). Docs gate green (sidebar +
   fences). Original entry: (M; user request 2026-07-24;
   **audit half DONE 2026-07-28**, rode the D12 pass — the motivating ssr.md
   find was already fixed by earlier docs-audit work; this pass added: ssr +
   walkthrough "Try it" sections link the toolchain install, walkthrough's
   stale `load_notes(self.db)` corrected to `store::open_database()` (checked
   against the example source), a `sync`-marker forward reference in
   functions-and-closures.md, glossary alphabetization, a dev-loop duplicate
   sentence removed. What remains is the judgment-restructure tail, findings
   recorded: hello-vilan's Projects section is long for page one; misc.md's
   OwnedNursery paragraph is dense; spec pages cite repo-relative
   `proposal/*.md` paths a published-book reader cannot reach; example
   READMEs use internal jargon (backlog ids, § refs) and rpc/README reads as
   an engineering log — those last two ride D7) —
   read the whole book (`vilan/docs/`) as an *outside* junior developer who has
   never seen the repo, and fix what that reader trips on. The motivating find:
   `guide/ssr.md`'s "Try it" section says `vilan run examples/ssr` but never
   links to the repository or says where `examples/` lives — dead-on-arrival
   for anyone reading the published book (reedsyllas.github.io/vilan) rather
   than sitting in a checkout. Audit every "Try it" / run-this instruction for
   the same assumption; each needs a repo link or a from-scratch path. The
   general bar: the docs must be approachable to a junior developer unfamiliar
   with vilan — advanced content may appear, but understanding the page must
   never *require* it. Overlaps D2's walkthrough expansion; this is the audit
   pass, not new chapters.


7. **`vilan/examples` cleanup; single-package becomes the default shape —
   SHIPPED 2026-07-25** (`5c351a0`) — `playground` pruned (its motivating
   repro kept as a test); `ssr` and `todo` converted to one package / two
   entries, joining `walkthrough`; `fullstack` deliberately KEPT as the
   workspace teacher, its manifest header pointing at the other three as the
   default. All nine survivors carry a manifest header comment AND a README
   with a run recipe. The emitted-JS sweep item resolved the other way than
   "decide": root `.gitignore` now carries `vilan/examples/**/dist/`,
   `**/*.js`, `**/*.css`, and `git ls-files vilan/examples` returns only
   `.vl`, `README.md`, `vilan.toml`, three `.html` and two per-example
   `.gitignore` — zero checked-in emitted JS, `examples/rpc/src/main.js` gone.
   Docs agree: `tour/platforms.md` calls one-package "the default shape; reach
   past it only when you have a reason" with the workspace as "the advanced
   form", and `tour/hello-vilan.md` starts readers at
   `vilan init my-app --template fullstack`. **Tails handed on, not closed:**
   (a) example READMEs still speak internal jargon — **DONE 2026-08-03**:
   `rpc/README.md` rewritten reader-facing (292 → ~180 lines; the 126-line
   fixed-bug archaeology moved to `proposal/transport-rpc.md`'s appendix, the
   record home; quirk #2's parenthesized-receiver/struct-bound lesson kept as
   language teaching; the stale sample output corrected — it was missing the
   `count = 13` line the E22 run-pin proved real); `router`/`ssr`/`todo`
   de-jargonized surgically (proposal §-refs and backlog ids dropped, book
   links kept). All nine READMEs now carry zero internal references. (b) **E22 — CLOSED
   2026-07-29**: seven of the nine examples had no build gate; now all nine do,
   by `crates/vilan-cli/tests/examples.rs`. The list is DISCOVERED from the
   directory rather than written down, so a new example is gated the day it
   lands, and a second pin fails the suite on a subdirectory without a
   `vilan.toml` — the one way that enumeration could go vacuous. Each example
   is staged to a temp directory through `git ls-files` and built there, which
   makes the claim the useful one ("a fresh clone builds this") instead of the
   weak one ("it builds against whatever is in the working tree"), and keeps
   the suite off the in-tree walkthrough build. The two pre-existing gates keep
   their sharper claims: `ssr_fullstack.rs` builds and RUNS `ssr` end to end,
   and `workspace.rs::the_walkthrough_example_builds` pins the three files the
   book tells readers to expect. All nine built unchanged — this bought a
   regression net, not a fix. Both pins were proven non-vacuous by planting the
   failure each catches. Original entry: (S–M; user request
   2026-07-24) — the directory has accreted (browser,
   fullstack, math, playground, reactive-ui, router, rpc, ssr, todo,
   walkthrough) with two competing project shapes: the three-package
   `common`/`client`/`server` workspace (fullstack, ssr, todo) and the
   walkthrough's one-package/two-entries model (`[entry.<name>]` + platform
   coloring). The single-package approach becomes the DEFAULT — in the
   examples and in the docs that present project structure — with the
   multi-package workspace kept where it genuinely earns its keep (or as the
   one example that teaches workspaces). Prune or merge examples that no
   longer teach anything distinct; every survivor gets a manifest header
   comment and a README that says what it demonstrates and how to run it.
   *Sweep item found 2026-07-25 (B33 S2, incidental):*
   `examples/rpc/src/main.js` is a stale checked-in EMITTED artifact (last
   written at e91f6ca; a rebuild differs by +766 lines of newer runtime) —
   decide whether examples check in emitted JS at all (likely: no,
   gitignore `dist/` + stray `.js`) as part of this cleanup.


8. **`examples/walkthrough` README — SHIPPED 2026-07-25** (`5c351a0`, with D7)
   — all four scoped points covered by `vilan/examples/walkthrough/README.md`:
   what the app is (sign-in, live-syncing note list, save-as-you-type editor),
   the pairing with `docs/guide/walkthrough.md` (linked, with a note that the
   guide quotes these files), the one-package/two-entries shape (manifest
   excerpt + file tree + the platform-coloring argument), and how to build/run
   (`vilan run .` → localhost:4600, plus the build-only path and the
   `dist/`/`notes.db` note). Original entry: (S; user request 2026-07-24; ships
   with or before D6/D7) — the only multi-file example without one.


10. **Move the repo to a proper org — SHIPPED** (the owner-string sweep +
    never-again gate `5bb74b9`; the later book-host move `0a0bdd4`; see also
    F9, the same arc from the distribution side) — `vilan-lang/vilan` is live
    and every scoped link site carries the new identity: `DEFAULT_BASE` in
    upgrade.rs, `BOOK_BASE` in the LSP (now `vilan-lang.org/docs/`),
    install.sh/install.ps1, the extension's publisher/homepage/repository/bugs,
    README + CHANGELOG, the npm meta launcher and platform manifests, the brew
    tap remote in release.yml, and the book's own cross-links (`book.toml`
    hardcodes no base-url — links are relative, so nothing to rewrite there).
    The go-public tripwires survived and GREW one: `tests/hygiene.rs`'s
    `no_tracked_file_contains_a_pre_migration_owner_string`, needles assembled
    at runtime, case-insensitive, with a 3-file allowlist each carrying an
    inline reason; the commit records a planted-probe non-vacuity proof.
    **The Pages tombstone is LIVE, verified end to end 2026-07-29** (this was
    F9's (a) hazard and the one thing the checkout cannot prove): a v0.14.0
    binary's hover URL `reedsyllas.github.io/vilan/guide/dev-loop.html` serves
    a `noindex` forwarder that JS-redirects `/vilan/*` fragment-intact to
    `vilan-lang.github.io`, which 301s to the custom domain, where a SECOND
    forwarder maps `/vilan/*` -> `/docs/*` and lands on a **200**. Both
    forwarders carry comments explaining why they must answer indefinitely.
    (Both hops return HTTP 404 by Pages design — status alone is not the test;
    the body is.) **Tail mostly CLOSED (reconciled 2026-08-01):** (a) DONE
    2026-07-29 — `org-migration.md` opens on "MIGRATION COMPLETE — all
    slices done" with per-slice markers, S1–S4 each dated; (b) half done —
    `CODE_OF_CONDUCT.md` now names `conduct@vilan-lang.org` (`3ebb532`; the
    follow-up `02f663e` records which half of the alias delivers). `AI_STANCE.md`
    links `github.com/ReedSyllas` and sits outside the hygiene gate's
    needle by design — it was left undecided-and-unrecorded at the
    2026-08-01 reconciliation. **RESOLVED 2026-08-03** — owner's call: the
    personal-account link is intended, same as the byline it sits beside,
    and stays; `org-migration.md`'s tail section carries the same note.
    **Tail fully CLOSED.** Original entry:
    (S–M; user request 2026-07-25;
    **prerequisite of D5, public traction**) — the repo lives under the
    personal account; before anything is promoted publicly, transfer it to a
    proper GitHub org. The transfer itself is small; the tail is the links:
    GitHub redirects repo URLs after a transfer but **Pages URLs do not** —
    the docs book moves off `reedsyllas.github.io/vilan`, so every existing
    link must be replaced, not left to redirects: the book's own cross-links
    and base-url config, the repo README + example READMEs, the install
    script / release-asset URLs (`releases.md`'s curl flow, F8's
    `install.ps1`), the extension's marketplace-facing links, and any remote
    recorded in CI. Timing note (user, 2026-07-25): existing links have
    barely circulated, so rewriting them is safe today — no
    external-breakage worry, and the window only narrows once D5 starts
    spreading URLs; hence the sequencing. Also re-check the go-public
    tripwires (hygiene test, private hooks) survive the move, and keep the
    pseudonym discipline — org naming is the user's call.


9. **"Vilan" casing sweep in docs and READMEs — DONE 2026-07-28** (rode the
   D12 pass) — the book was already largely conformant (earlier docs-audit
   work); the stragglers are fixed: cli.md's prose possessives ("Vilan's own
   test suite"), book.toml's title, the branding README's H1 and license
   note. Manifest header comments checked clean. Artifact names stay
   lowercase per the rule below. The runtime-name question was decided the
   same day (user: "Node" proper) and swept: Node/Deno/Bun brand-cased in
   prose repo-wide, lowercase kept for commands, target values, backticked
   artifacts, and the DOM/data-structure sense of "node". Original entry: (S; user request
   2026-07-24) — the language name appears lowercased ("vilan") everywhere in
   prose; proper-case it: **"Vilan" when naming the language** in docs-book
   pages, READMEs, and manifest/header comments, while `vilan` stays lowercase
   where it names the *artifact* — the CLI (`vilan run`), the binary, package
   names, paths, and code spans. Sweep `vilan/docs/`, every README, and the
   repo README; sentence-initial and mid-sentence prose uses alike. Mechanical
   but judgment-per-site (prose vs. command is context, not regex); rides
   naturally with D6's approachability pass.


11. **Web playground — LIVE AND COMPLETE, NOTHING REMAINS BY CHOICE
    2026-08-02** (`web-playground.md` is the record: ratified design +
    amendments + per-slice status; **reconciled 2026-08-03** — this entry
    still read "Open: S4" with S4 unstarted, four ship commits and eight
    days stale, caught by `git log --grep D11`, none of which had touched
    this file) — S0 the size spike (GO), S1 the overlay completion
    (`3f387fe`), S2 `crates/vilan-wasm` + release wiring (the
    `vilan-playground-wasm.tar.gz` asset, live since v0.18.2), S3 the page
    itself in the WEBSITE repo (third entry `/playground`, vendored
    CodeMirror 6, worker + sandboxed-iframe runner, diagnostics pane with
    editor squiggles, seeded examples with a byte-compare freshness gate,
    deploy.yml wiring + smoke gate; real-browser e2e green; S3 record in the
    proposal), and **S4 — SHIPPED 2026-08-01/02**: share-via-fragment
    (website `4a229f9`), the fmt button (`6ed421a`, `vilan-wasm` exports
    `format`, canonical-or-original-bytes-on-bail, two pins), the upgrade
    round (website `2fd4dd9..6920d30`), and the version selector (website
    `c409854`: the manifest's `versions` inventory is the pages repo's own
    directories, a pin recycles the worker under `?v=` and re-checks,
    capabilities re-evaluate per version, a pinned share carries `&v` —
    doubles as the version-badge call). vilan-lang.org/playground serves
    the whole arc; promotion itself still rides D5/D10 (unlinked from the
    site nav by choice, not by gap). Original
    entry: (M–L; user request 2026-07-28) — run vilan code
    on the fly from the browser: an editor pane, a run button, output beside
    it — the try-it-without-installing on-ramp every language site has, and a
    natural D5 (traction) asset. Two candidate architectures, decide in the
    proposal: **(a) in-browser** — compile the compiler crates to WASM
    (`vilan-core` is the candidate; distinct from F3, which is a WASM *target*
    for vilan programs) and run the emitted JS in a sandboxed iframe — no
    server, no abuse surface, works offline, but node-leg programs (services,
    RPC) can only be typechecked, not run; **(b) server-side compile** — a
    compile service returns the emitted JS which still executes in the
    visitor's iframe; smaller build problem, but a hosted service to run,
    rate-limit, and keep patched (executing visitor code server-side is off
    the table either way). Either shape wants compiler error rendering in the
    pane (the diagnostics are the pitch — see the site's compiler showcase)
    and shareable snippets eventually. The `examples/playground` directory is
    the local-CLI cousin, not this. PROPOSAL FIRST per `CLAUDE.md`; sequencing
    interacts with D5/D10 (where it's hosted and promoted is the user's call).


12. **De-AI-ism sweep of the docs — DONE 2026-07-28** (same-day request; one
    combined pass with D9 + the D6 audit; a six-editor line edit over the 54
    book pages + 15 public READMEs against a rubric modeled on the Rust
    Book/Stripe/MDN register) — ~800 prose em-dash lines brought to zero
    outside fences (replacements varied per site: colons dominant, parens,
    sentence splits; semicolons kept rare), decorative "X, not Y" reversals
    dropped where not load-bearing (kept where they kill a real wrong
    assumption), filler cut ("simply"/"just"/"honestly"/"exactly"), rhythm
    bolds removed (term-at-definition and true warnings kept), staccato
    triads flattened. Fenced examples byte-identical (extraction-diffed;
    docs gate 8/8 green). **The compiler's own messages got the same pass
    (user call, 2026-07-28, same day):** ~200 production strings across
    vilan-core (analyzer, async_infer, manifest, parsing, lexing, macros,
    git_dep, init_order, const_eval), the CLI (init, watch, upgrade, HMR
    overlay + hmr_shim.js), and the LSP (hovers, completions, errors)
    de-dashed by rule (colon before rule statements, semicolon before
    imperative fixes, parens for asides; the two double-dash move-clean
    messages restructured; four in-message "vilan"→"Vilan" prose casings).
    All pinned tests assert the new text (incl. the "build failed; see the
    terminal" triplication across main.rs/hmr_shim.js and its NEGATIVE pin
    in tests/hmr.rs, and the lib.rs/document.rs internal-error duplicate);
    errors.md's 19 quotes, dev.md's and dev-loop.md's fenced quotes, and
    memory.md §418's paraphrase updated in lockstep, so errors.md now
    carries zero em-dashes. Full workspace suite green (47 result sets),
    fmt clean, production string sweep clean. Same session, same user
    turn: runtime names brand-cased in prose (Node/Deno/Bun; 52 sites, 23
    files; artifact/value positions stay lowercase) and the D12 anchor
    renames ratified. Docs-side deliberate survivors: spec's citable
    "§N — Title" H1s / memory.md rule headings (anchor-bearing) and
    std/reactive.md's "Draft — local-first cells" (linked anchor).
    Residuals: em-dash/"just" tells in ~30 fenced-example comments
    (compile-gated, safe to fix, each needs the docs gate re-run); ~13
    "X — Y" headings renamed to "X: Y" changed their published anchors
    (no internal links target them, verified; external links young per
    D10's timing note — revert is trivial if that matters).


### E. LSP & tooling

3. **Per-analysis leak + incremental analysis** (was L; **Phases 1–2 resolved
   2026-07-21**, `analysis-reuse.md` is the record) — Phase 1 SHIPPED (the
   leak reframed and closed: macro-expansion parse sites cached, `run_service`
   leak removed at root, 14-site `leak_tally`, harness un-`#[ignore]`d
   asserting on counters — 357 B/analysis named leak vs ~60 KiB allocator
   churn). Phase 2 (the prelude checkpoint) **CLOSED BY MEASUREMENT** — its
   own 30% gate fired at 18.5%: 82% of the warm ~88 ms floor is `build()` +
   whole-program checks re-resolving unchanged std, which no load-state
   snapshot captures. What remains is **Phase 3 only** (entry-delta fixpoint
   + entry-scoped checks over a frozen, generation-0 std base — XL; concrete
   blockers recorded in `analysis-reuse.md` §4); take it when the warm floor
   demonstrably hurts on real projects, not before. 2026-08-01: a Phase-1
   RESIDUAL is filed as E23 — macro-DEFINING buffers re-leak their world per
   length-changing edit; the harness's fixed-width dodge is now recorded in
   `analysis-reuse.md`. **2026-08-02: Phase 3 REOPENED as the std-tax arc**
   (`analysis-reuse.md` §6): the suite audit's E28+E30 measured to the same
   root (a fixed ~115 ms per-analysis std tax, ~84 % `build()`+checks over
   unchanged std) and folded in; `VILAN_PHASE_TIMING` shipped as S0; the
   slice plan S1 (entry-scoped checks) → S2 (resolution idempotence) → S3
   (frozen base) → S4 (consumer wiring) stands ready for take-up.
   **S1 SHIPPED 2026-08-02** (`analysis-reuse.md` §6.5): all 28 passes
   classified by four independent reads; 13 definition-site sweeps filter
   frozen (std-from-disk) entities via sealed binary-searchable ranges;
   the permanent differential gate landed (`check_scope_differential.rs`:
   std-clean invariant per platform + whole-corpus both-ways agreement on
   diagnostics/warnings/JS), both plant directions proven red. Measured:
   checks 30 → 23 ms, trivial floor ~113 → ~106 ms — the estimate's
   composition was wrong (most of the window is the unskippable
   instantiation-driven + data-producer core). S1's lasting value is the
   gate and the frozen-source machinery S2/S3 run on; the money stays in
   `build()` + load/walk = S3.
   **S2 SHIPPED 2026-08-02** (`analysis-reuse.md` §6.6): resolution is
   drain-once — FIVE cloned queues (not the recorded three; both accessor
   queues also re-incremented counts) now `mem::take`, with conformance's
   post-build read retained via `written_type_spellings` (accumulates
   across builds). Pinned by `build_idempotence.rs`: a `set_build_twice`
   switch + four-program battery asserting identical
   diagnostics/warnings/JS; plant-proven red through copy-elision's
   `reference_count == 1` JS diff. Finding: with the drains in, a full
   second `build()` is already observationally neutral — S3's remaining
   work is generation-scoped ids, not behavioral neutrality.
   **S3 DESIGN DONE + GROUNDWORK LANDED 2026-08-02** (`analysis-reuse.md`
   §6.7): mechanism settled as CLONE-the-base (the mutation inventory
   kills rollback: accumulating context-pass appends into std IR,
   in-place TypeId fills, one backwards scope-memo write); the dispatch
   candidate-set fear retired (all enumeration is post-build from
   Program); `build()` split into `resolve_world` + `finalize_build`
   (commit-once), `set_early_std_build` + `VILAN_EARLY_STD_BUILD` probe
   landed — the whole suite votes on two-phase neutrality: corpus clean,
   1215/1216 inference clean, and the ONE failure (immediate-chained
   `.map().map()`) is the first concrete instance of the §4 id blocker,
   minimal repro recorded with three falsified hypotheses. Next: S3b
   (freshen-not-fill base TypeIds, repro pinned red-first) → S3c (base
   cache + clone, measured) → S3d (wiring + standing differential).
   **S3b ADVANCED 2026-08-02** (`analysis-reuse.md` §6.8): the stall is
   localized by falsification — NOT below-mark writes (generation-mark
   tracing recorded zero), NOT slot machinery (fills byte-identical both
   modes); the missing event is the LATE fill of map#1's fresh per-call
   result element by the first closure's landed return, which monolithic
   ordering delivers and two-phase never triggers. Blocker pinned
   `#[ignore]`d as `two_phase_build_resolves_chained_generic_calls`
   (red when run). Next instrument: the ClosureReturns flow and the
   method-call result-instance construction; the fix lands at that root.
   **S3b KERNEL LANDED 2026-08-02** (`analysis-reuse.md` §6.9): the
   stall was a LATENT FIXPOINT BUG, not an id-space problem — the exit
   condition missed type writes made by deferring attempts (a chained
   call's closure-parameter fills), and std's constraint churn masked it
   monolithically. Fix: `type_map_writes` as the fixpoint's third
   progress signal; quiescence requires a fruitless retry AND an
   untouched type map. Pin un-ignored and green; plant-proven; the
   whole-workspace two-phase vote is the acceptance. S3c (base cache +
   measured clone) now has no known blocker.
   **S3c PART 1 LANDED 2026-08-02** (`analysis-reuse.md` §6.10):
   two-phase is the DEFAULT pipeline (probe switch retired; std-entry
   keeps monolithic order); the phase line grew a `base` bucket. Measured
   warm: ~19 ms load+walk + ~42 ms base + **3 ms entry build** + ~23 ms
   checks — the cacheable slice is ~61 ms, and the entry's own build is
   three milliseconds. Part 2 (the cache) is fully designed in §6.10:
   World split at the resolved-world boundary, interned seeds so the
   cached world is 'static by construction, (platform, seeds) key with
   E12 content validation, entry-slot patching, conservative syntactic
   bypasses, cached-vs-fresh differential + clone measurement as gates.
   **S3c PART 2 SHIPPED 2026-08-02** (`analysis-reuse.md` §6.11): the
   base cache is LIVE — (platform, std-ref names) key, E12 content
   revalidation, entry-slot patching, std-root-scoped overlay bypass so
   the LSP's normal open-buffer state still hits. Measured: inference
   19.2 → 13.4 s (−30 %) from in-process hits alone. Residuals: wasm
   bypasses (std-from-overlay), derive/macro entries bypass (expansion
   hoist would widen). Gates in base_cache.rs, all plant-proven.
   **S3d SHIPPED 2026-08-03** (`analysis-reuse.md` §6.12): the consumers
   wired BY DELETION — the std-overlay bypass was lost coverage
   (validation reads through overlays, so E12 already governs them):
   the wasm playground now caches (pinned: whole-std-from-overlays hits
   on the second analysis), and LSP-edits-std is content-governed
   (unchanged buffer hits, edited buffer evicts — pinned both ways).
   Leak harness green, RSS retention +0. Remaining residual: the
   derive/macro expansion hoist. THE STD-TAX ARC IS COMPLETE S0–S3d.
   **THE HOIST SHIPPED 2026-08-03** (`analysis-reuse.md` §6.14):
   register_file measured ~0.95 ms → fork (a), the World carries the
   registry; `expand_entry_over_world` runs entry expansion post-world on
   hit and miss alike; unloaded-module demands rebuild fresh (depth-one
   `allow_cache: false`); `derive` leaves the bypass, `macro` stays.
   Plant-proven; the residual list is EMPTY.


4. **Sub-file incremental parsing — CLOSED BY MEASUREMENT 2026-07-22** (user
   ratified) — the motivation was chumsky-era parse cost; post-H6, parsing is
   **3.7% of a compile** (callgrind, S5) and the per-keystroke LSP floor is
   82% `build()` + whole-program checks (the E3 Phase-2 measurements), so an
   L-sized tree-reuse engine would optimize a ~3% slice with no observable
   editor win. The LSP-latency lever, if ever needed, is E3 Phase 3 (the
   entry-delta fixpoint), not this. Reopen only if a real project's
   single-file parse time becomes observable in the editor.


7. **Diagnostics audit — batch 7 — SHIPPED 2026-07-21** (`diagnostics-ledger.md`
   batch-7 continuation): 23 post-snapshot sites verdicted (all QUALIFIES — each
   born in a proven, pinned arc), the five `could not be resolved` residual rows
   finalized DEMOTE behind the cascade guard, pinned by the multi-use-site
   `one_unresolved_name_does_not_cascade_across_many_use_sites`. Split-out find:
   B32 (the void-typed unknown-value cascade).


11. **Cross-source diagnostic notes — CLOSED as already-satisfied 2026-07-21**
    (survey, not code): every producer that can emit an into-`std` note for a
    user-caused condition emits a *declaration* note ("the bound/trait declares
    … here") — the class the refinement explicitly protects; re-anchoring those
    would make them lie. Recorded residual (reopen only on demand): pointing the
    bound-failure note at the offending *argument* needs the bound-check to keep
    per-argument expr ids (it flattens to `(site, constraints, type-ids)` today)
    — a targeted plumbing order if the declaration-note UX ever proves
    insufficient in practice.


13. **formatter completion — SHIPPED 2026-07-22, zero-bail gate LIVE**
    (`formatter_never_silently_bails`, un-ignored). Printer
    arms for every bailing construct (LetDestructure incl. arrays/nesting,
    `[T; n]`/`[v; n]`, macro fun/block/invocation/attribute with
    verbatim-span args, match tuple-pattern source-consulted spelling — both
    spellings live in corpus+std); TWO latent printer bugs the net had been
    absorbing fixed (prefix-operand precedence — `-(2+3)` would have
    reformatted to `-2 + 3` — and dropped parens around lift-chain
    subjects); 25 per-construct pins (token-equality, idempotence, canonical
    round-trip, not-a-bail perturbation). Residual DESIGN gap, recorded in
    the gate's comment: a redundant paren around a BARE ATOM
    (`(300).as_u8()`) is dissolved by the parser and unrecorded, so such a
    file bails safely; the corpus's five sites were canonicalized
    (emission byte-identical, probe-proven); future fix = AST-aware net or
    parser-recorded parens. Informational: `vilan fmt` would reformat 11
    std files (all token-equal — line-collapsing + trailing-blank trims);
    NOT applied, the user decides.


12. **watch-round compile caching — SHIPPED 2026-07-21** (premise corrected in
    flight: std/package modules were ALREADY content-cached across rounds by
    `load_package_module` — the only re-parsed file was each leg's entry, so
    half (b) was the real win). Shipped: (a) `parse_clean_cached` in
    vilan-core — ONE content-addressed clean-parse cache (post-lift trees)
    shared by the module loader and the CLI entry path, with a known-broken
    set so a broken entry leaks once per distinct content; (b) per-leg skip in
    the HMR round — `Program.source_hashes` records the content hash each
    source was COMPILED from, and a leg is reused only when every recorded
    source re-hashes identically now (`hmr::leg_is_current`; the review
    replaced the first cut's mtime-derived changed-set intersection — an
    mtime-preserving write or coarse-mtime filesystem could have served stale
    bytes; content now decides, mtime only triggers rounds). Round guards
    (first round / prior failure / any `vilan.toml` under the root changed)
    force full recompiles, pinned as pure logic. Measured (release,
    todo-workspace, client-only round): 128ms → 76ms compute. Recorded
    residues: `macro_std` files loaded only by macro *bodies* aren't in a
    leg's source set (toolchain-dev only); the plain non-HMR watch loop
    doesn't skip (no retained artifacts to reuse). H6's "release builds past
    ~1s" trigger remains unmet.


14. **LSP construct snippets — SHIPPED 2026-07-23** (`e29955e`; this entry
    was never closed and still read NEW at the 2026-08-01 reconciliation) —
    the data-driven `CONSTRUCT_SNIPPETS` table (document.rs) offers the four
    named shapes at scope positions, mapped to `InsertTextFormat::SNIPPET`
    with the bare-keyword fallback for clients without snippet support;
    pinned both ways (`construct_snippets_are_offered_at_a_scope_position`,
    `construct_snippet_without_snippet_support_falls_back_to_bare_keyword`).
    Original entry: (S; user request 2026-07-22) — completion
    offers snippet-kind templates for the shape-heavy constructs: `for` →
    `for ${1:item} in ${2:items} {\n\t$0\n}`, `fun` → name/params/return/body
    tabstops, `struct` → name + first field, `match` → subject + first arm.
    The WO-3 machinery (snippet capability check, plain-text fallback) already
    exists; these are keyword-adjacent completion items with
    `InsertTextFormat::SNIPPET`. Start with the four named; the table is
    data-driven for growth (`if`, `impl`, `trait` when wanted).


15. **pretty, colored CLI output — SHIPPED 2026-07-23** (`9ae208b`, "the CLI
    speaks in color — when spoken to directly"; this entry was never closed
    and still read NEW at the 2026-08-01 reconciliation) — `paint.rs` is the
    one mechanism, gated exactly as specified (`is_terminal && !NO_COLOR`,
    plus the Windows VT enable), covering the `Compiled …`/`[watch] …`/
    `hmr: …` lines; unit-pinned including the piped-output-stays-plain half.
    Original entry: (S–M; user request 2026-07-22) —
    diagnostics are already ariadne-colored; the plain status lines are not
    (`Compiled X -> Y`, `[watch] …`, `hmr: …`, `error:` prefixes, the test
    runner's pass/fail summary, fmt's file lines). One consistent scheme:
    success green, errors red+bold, watch/dev-channel lines dim or cyan,
    counts bold. MUST be TTY-gated (`std::io::IsTerminal`) and respect
    `NO_COLOR` — piped output stays byte-plain (the e2e tests parse stdout
    lines; coloring must never reach a pipe). Dependency-free ANSI helpers
    (or reuse ariadne's `Color` already in-tree — implementer's call, one
    mechanism only).


16. **cross-source diagnostic rendering — SHIPPED 2026-07-25 (v0.16.0 grind
    4)** — diagnostic_sources is authoritative (ONE writer push_diagnostic,
    ONE reader, CLI + LSP on the same channel; an Error field was rejected —
    254 literals that don't know their source); every post-analyze pass now
    attributes (they ALL defaulted to the entry — the editor squiggled the
    wrong file for const/platform/async/context diagnostics too); the CLI
    renders each diagnostic against ITS file with a snippet net (a span that
    doesn't index the text loses its snippet, never panics — the CRLF case
    exits 1 cleanly). RESIDUAL CLOSED 2026-07-26 (`f4c9dd6`, grind 5, "the
    overlay's last lie"; still read open at the 2026-08-01 reconciliation):
    `OverlayDiagnostic` carries a per-diagnostic `file` (hmr.rs), `located`
    resolves line/col against the diagnostic's own source, and the overlay
    renders `<file>:<line>:<col>`. Original entry: (S–M; found 2026-07-24 by the windows-arc S2 adversarial
    review, pre-existing) — a diagnostic whose span belongs to a *different*
    source than the one it is rendered against (repro: `macros.rs` ~381-393
    spans the macro_std-missing error at a macro definition that lives in
    std, then the CLI renders it against the entry text) prints wrongly but
    safely on LF; under CRLF the byte drift can land mid-codepoint and
    ariadne panics ("byte index N is not a char boundary"), taking the
    compiler thread down. Root fix: a diagnostic must be rendered against
    the source its span indexes (carry the SourceId through to the render
    site), not patched by clamping offsets. **Second sufferer (2026-07-25,
    B33 S2):** a cross-module initialization-cycle error's primary span
    renders against the entry file's text in the CLI (probed: a plain type
    error inside an imported module misrenders identically today); the LSP
    is fully correct via `diagnostic_sources`, and B33's message carries
    the true file names in its text — the CLI render is the remaining gap.


17. **LSP module-attributed notes — SHIPPED 2026-07-25 (v0.16.0 grind 4)** —
    notes arrive as relatedInformation with their own file's URI (a note
    with no file resolves against the diagnostic's published file); the
    note-drop pin flipped to assert arrival. Original entry: (S; found
    2026-07-25 by B33 S2's publish pins, pre-existing) —
    `published_diagnostics`' module-attributed branch publishes with
    `note: None` (document.rs ~843), so any diagnostic attributed to a
    non-entry file loses its C3 note in the editor (B33's "`Z` is declared
    here" among them; affects every module-attributed diagnostic). Pinned
    as current truth in B33 S2's publish pins; fix = thread the note
    through that branch (likely wants LSP related-information rather than
    message-appending — decide against the diagnostics standard).


20. **watch mode swallowed a save landing during the initial build — ROOT
    BUG FOUND AND FIXED 2026-07-25** (the test was RIGHT four times: the
    'flake' was a real product race. watch_loop took its baseline mtime
    snapshot AFTER the initial action; a save landing between the build's
    output appearing and the snapshot being taken was baked into the
    baseline and never detected — no round ever fired. The window is
    microseconds idle (isolation-green), wide under load (the four
    deadline-EXHAUSTED strikes: 20.25s/20.5s/20.97s were never
    'bare overshoots' — that misread produced a wrong raise-the-deadline
    mitigation, refuted by the fourth strike consuming all 120s). Fix:
    snapshot BEFORE the first action — an edit during the initial build now
    triggers one extra round, the correct behavior; a human saving during
    the initial watch build was silently losing that save too. One site;
    plain and HMR paths both flow through it. The 120s deadline stays (a
    hang net). LESSON: a deadline-shaped failure's duration tells you
    which HALF failed — deadline+ε = exhausted-never-happened, not
    finishing-late. Original entry:
    `assets.rs::watch_round_refreshes_the_sidecar` waits a fixed deadline
    for the mtime-polling watch loop to notice an edit; under full parallel
    suite load the round loses the race. Fix directions, decide at take-up:
    an adaptive/longer deadline (cheap, still probabilistic), serializing
    the watch-e2e group (a shared test mutex — deterministic, small
    wall-clock cost), or injecting the change event instead of racing the
    poller (best, needs a seam). Untouched by the slices that surfaced it
    both times.


19. **the `free_port` TOCTOU flake — SHIPPED 2026-07-26 via direction (a)
    (v0.16.0 grind 6, 1442586)** — std `Server.port()` (the bound port; the
    real API port-0 users need) + every fixed-port-probe fixture migrated to
    real port 0 with read-back; free_port deleted where unused;
    transport_robustness keeps its probe (same-port rebind is the lesson)
    with the reason recorded. The .staging sweep shipped in the same slice
    (materialize-time, 6h age gate). Original entry: (S–M; filed
    2026-07-25 per the three-strike rule — struck three times that day,
    all in `rpc_http.rs` under full parallel suite load) — the S1-windows
    port migration probes an ephemeral port by bind-then-release and
    substitutes the literal into the program source; the window between
    release and node's bind loses a race to a concurrent suite process
    (`EADDRINUSE` on an ephemeral port; always green in isolation). Two
    fix directions, decide at take-up: (a) substitute port 0 into the
    PROGRAM and read the actual bound port back — needs the fixture's
    server to report it (std `http::Server` may want a `port()` accessor
    after listen — an API addition useful in its own right), or (b) keep
    the probed-port pattern but retry the fixture with a fresh port when
    stderr shows exactly the EADDRINUSE signature (no std change, bounded
    retries). The `ssr_fullstack`/`hmr_overlay` tests already read output
    and could adopt (a) cheaply; `rpc_http`'s six are the hot spot.


18. **`vilan init` scaffold subcommand — SHIPPED 2026-07-25** (`69ad118`,
    v0.16.0 grind 2) — all three scoped templates live at
    `crates/vilan-cli/templates/{node,browser,fullstack}/`, embedded in the
    binary so an installed toolchain carries its scaffolds and never looks up a
    directory at runtime. `--template` AND a prompt (`choose_template`); a
    non-TTY without the flag is a clean error, never a hang, and the prompt is
    unit-testable through an injected `ask` closure. Manifests carry header
    comments with run recipes; `.gitignore` ships per template (fullstack
    `dist/`, node/browser `*.js`+`*.css`, since those emit beside the source).
    **The suite gates compile AND run, per template:** node builds, runs and
    asserts the greeting plus `vilan test` output; fullstack builds both
    entries, **spawns `node dist/server.js`, waits for the port and HTTP-GETs
    `/` and `/client.js`**, and separately diffs the scaffold's manifest field
    by field against the blessed examples (walkthrough/todo/ssr) so drift in
    either direction fails; browser builds and asserts the emitted bundle uses
    DOM globals and imports no `node:` module. Plus a gate that the embedded
    file set equals the on-disk template set, is `fmt --check` clean, and
    leaves no `{{name}}` token. Documented in `appendix/cli.md`,
    `tour/hello-vilan.md`, `guide/dev-loop.md`, `guide/ssr.md`, README, and the
    CHANGELOG. *One literal-reading nuance: the browser template is built and
    inspected rather than executed — there is no headless browser in the
    harness, and the emitted-bundle assertions stand in.* Original entry:
    (S–M; user request 2026-07-25;
    renumbered from 17 — E17 was filed earlier the same day) —
    `vilan init [name]` scaffolds a ready-to-run project so the first minute
    with the toolchain is `install → init → vilan run`, not hand-writing a
    manifest from the docs. Templates for the shapes that exist: a minimal
    node package, a browser app, and the full-stack single-package
    two-entries shape — the D7 default; the scaffold IS that default's
    delivery vehicle, so the two must agree on the blessed layout. Template
    selection via `--template` (or a short prompt when omitted); emitted
    files carry brief orienting comments, `.gitignore` included, `dist/`
    respected. Every template must compile and run green under the current
    binary — gate them in the suite like corpus programs so a language change
    can't silently rot the scaffold. Docs tie-in: D6's "from-scratch path"
    for Try-it sections gets to say `vilan init` instead of walking manifest
    authoring; the on-ramp pairs with F7 distribution (npm/brew install →
    init → run).


21. **test-suite speed audit — AUDIT DONE 2026-08-02**
    (`proposal/suite-speed.md` is the record: the measured profile and the
    slice list, filed here as E25–E30) — the headline numbers: a warm-tree
    suite is 131.3 s wall of which 130.7 s is 51 result sets run STRICTLY
    SERIALLY on a 16-core machine (gaps and compile check are 0.1 s each),
    plus a 16.0 s relink tax after any vilan-core edit at only ~490 % CPU.
    The heavy binaries decompose cleanly: the LSP unit tests (29.9 s) are
    ~81 real `Document::analyze` fixtures, inference (18.7 s) is ~1400
    full-pipeline compiles saturating all cores (first attributed to its
    node spawns — E26's measurement corrected that, `suite-speed.md` §2.1),
    docs (16.7 s) and interpreter (14.9 s) are serial loops over independent
    compiles, and corpus is already 8-way parallel — the shape to copy.
    Recommended sequence after the E26 correction:
    E27 → E25 → (E28/E29/E30 per E25's outcome). Original entry: (S–M investigation; user request
    2026-07-28) — every arc pays the full suite at least once, so its wall
    time is a tax on all work; find where the time actually goes and what
    can be reclaimed **without weakening any gate** — no pins dropped, no
    cases sampled, no goldens loosened; quality is the constraint, speed is
    the variable, and anything that changes what is *tested* is out of scope
    by definition. Measure first, per target: the ~40 separate
    integration-test binaries (each links the full crate; the default
    harness runs binaries serially), the corpus's ~100 per-program builds
    through the debug binary, the docs gate compiling every fence,
    `inference.rs`'s ~1200 cases with their run-and-check node spawns, the
    Linux leak harness's 200-analysis loops, and the e2e legs that bind
    ports and spawn servers. Then name the levers with evidence:
    cross-binary parallelism (cargo-nextest or equivalent — the E19/E20
    flake history marks exactly where timing pressure already bites, and
    stdout-parsing e2e legs plus port binding constrain it), consolidating
    test binaries to cut link jobs, amortizing process spawns (corpus and
    node runs through warmed processes), linker and profile choices for the
    test build itself, and CI-leg split/caching across the OS matrix.
    Output: a measured profile plus a slice list; each lever ships as its
    own suite-gated item.


22. **seven of the nine examples have no build gate — SHIPPED 2026-07-29
    (unmarked until 2026-08-03), COMPLETED 2026-08-03** — the build gate
    landed the same day the reconciliation filed this (af49f87,
    `crates/vilan-cli/tests/examples.rs`: discovery-based enumeration, a
    manifest check so enumeration cannot go vacuous, tracked-files-only
    staging) but the entry never got its marker — caught by the
    verify-before-acting rule at take-up. The take-up completed the
    decide-at-take-up answers: `math` and `rpc` RUN with byte-exact
    stdout pins (the corpus bar); `browser`/`reactive-ui`/`router`/`todo`
    assert their emitted bundles exist non-empty at documented paths;
    `fullstack` stays build-only deliberately (the fullstack TEMPLATE's
    spawn-and-fetch e2e already exercises the served shape). Unknown new
    examples default to build-only — gated the day they land. Both leg
    kinds plant-proven red. Original entry: (S; found
    2026-07-29 by the backlog reconciliation, handed on from D7) — only
    `examples/ssr` (`tests/ssr_fullstack.rs`) and `examples/walkthrough`
    (`tests/workspace.rs`) are compiled by the suite; `corpus.rs` walks
    `vilan/test/`, not `vilan/examples/`, and CI is a plain
    `cargo test --workspace`. So `browser`, `fullstack`, `math`,
    `reactive-ui`, `router`, `rpc` and `todo` can silently rot under a
    language or std change — exactly the failure mode E18 gated the *templates*
    against, and the reason the README's `cargo test` comment overclaimed
    until D4's tail sweep. The examples are the on-ramp D5 will point people
    at, so a rotted one is a first-impression bug. Cheapest shape: extend the
    existing template gate's pattern — build each example with the current
    binary, run the ones that terminate (`math`, `rpc`), assert the emitted
    bundle for the browser ones. Decide at take-up whether `fullstack`/`todo`
    earn a spawn-and-fetch leg like the fullstack template's or just a build.
    Watch E21: this ADDS suite time, so the two want sequencing (measure
    first, then add the gate onto a known profile).


23. **NEW — a macro-DEFINING buffer re-leaks its whole world per
    length-changing edit** (S; E3 Phase 1's recorded-but-unmeasured residual,
    found 2026-07-28 scoping D11's leak exposure; filed 2026-08-01, ARC
    ACTIVE) — the world cache (`compile_world`'s `WORLDS`, macros.rs) keys on
    the hash of the length-preserving blanked source: every byte outside the
    macro definitions becomes a space, so the key depends on the whole file's
    length and newline layout, and any length-changing edit outside the macro
    spans misses the cache, recompiles the world, and `Box::leak`s the full
    blanked text (`MacroWorldText`, ~file size) plus a whole world `Program`
    (`MacroWorldProgram`) — per analysis, unbounded, in any buffer defining a
    `macro fun` or `macro { .. }` block. The Phase-1 harness deliberately
    dodges it (`gensym_expansion_leak_plateaus` holds its edit tail at four
    digits so the blanked source stays byte-identical), so nothing in the
    suite measures it; `analysis-reuse.md` now records the dodge. D11 raised
    the exposure: vilan-wasm recycles its instance per Run partly because of
    this. Pin first (a length-changing-edit case in the leak harness
    asserting the macro counters plateau), then make the world key survive
    non-macro edits — the definitions' content, not the file's layout — with
    any world-surfaced span remapped if positions moved (design at take-up).
    **SHIPPED 2026-08-01** (`world_cache_keys`: the world keys on the
    definition segments' content; failures cache under content+offsets with
    spans clamped at replay — a second find, since a BROKEN definition
    re-leaked its world text every analysis even unedited; both pinned
    red-first in `leak_measurement`). Same-day sweep follow-through, each
    pinned red-first: the wasm front-end's entry text was leaked UNTALLIED
    per compile (now content-interned, `WasmEntryText`, 15th site);
    dependency display names re-leaked per analysis (now content-interned);
    `flush_rust_fallback` parsed uncached (now through `parse_cached`);
    `EntryAst`'s tally is now tree-proportional (node count × node size) so
    tree growth is visible to the counters. Accepted residuals: an edit that
    MOVES a still-broken definition recompiles once per layout, and
    `MacroWorldProgram`'s tally stays shallow (bounded by the world cache;
    magnitude is not what its assertion needs).


24. **NEW — `leak_tally` has no production surface** (S; suggested 2026-07-28
    alongside the E23 find; filed 2026-08-01) — the 14-site counters record
    on every analysis in production builds, but the readers
    (`bytes`/`total`/`reset`) are reachable only from the Linux-gated
    `leak_measurement` test module: a live LSP or CLI session cannot report
    what it has leaked, so leak claims from the field stay RSS-inferred —
    exactly what Phase 1's "honest instrumentation" was built to avoid.
    Smallest useful surface (decide at take-up): an env-var-gated stderr
    report on shutdown, an LSP custom request, or a `--leak-report` flag on
    `build`/`check`; whatever ships should print the same per-site split the
    harness asserts on. **SHIPPED 2026-08-01**: `VILAN_LEAK_REPORT` prints one
    cumulative `[vilan leak]` line to stderr per top-level analysis, from the
    end of `analyze` itself — the one chokepoint every front-end shares (the
    CLI calls `analyze` directly and never passes `analyze_source`, which the
    first cut wrongly assumed) — and NOT at shutdown, which was the filed
    idea's flaw: the counters are thread-local and analyses run on a
    dedicated thread, so only the analysis chokepoint can read them. One
    mechanism covers CLI, watch, and LSP (wasm hosts have no env, and the
    playground recycles instances); `leak_tally::report` is the formatter,
    unit-pinned, with the default-off + split e2e-pinned in diagnostics.rs.


25. **run the suite's binaries in parallel — SHIPPED 2026-08-02**
    (`suite-speed.md` §2 has the full numbers) — cargo-nextest 0.9.140:
    suite 112 s → **63.5 s** (131 s at the audit baseline; 2.1× with
    E27). The committed `.config/nextest.toml` turns fail-fast off and
    priority-starts the leak plateaus (the 32 s longest test is the
    critical path if scheduled last). Parity exact (2270 + 1 skipped =
    cargo test's 2271; doc-test sets all empty, CI grew a
    `cargo test --workspace --doc` guard leg); three runs, zero flakes —
    the e2e/node-storm risks did not bite; plant-proven red. CI's test
    job runs nextest on both OS legs; CLAUDE.md names nextest as the
    gate; release.yml's tag-time gate unchanged. Recorded costs: user CPU
    +70 % (per-test process tax), so the wall floor is ~57 s, not 33 s;
    and CI's windows leg 8m21s → 10m02s (ubuntu 6m37s → 6m13s) — accepted
    for instrument parity, one-line revert if it stops being worth it.
    Original entry: 130.7 s of serial execution vs a 29.9 s longest
    binary is a theoretical ~4.4×; cargo-nextest is the candidate
    instrument. Clear per-binary: stdout-parsing e2e legs, node-spawn
    storms under load (the E20 flake history), CI parity.


26. **batch inference's node runs — CLOSED NEGATIVE 2026-08-02**
    (`suite-speed.md` §2.1 is the record) — built in full (persistent node
    child, one `worker_threads` isolate per program, probe-pinned
    byte-identical to standalone `node file.js`; 1205 cases + 8 runner pins
    green) and the wall did not move: 19.39 s → 19.20 s. The filed premise
    (534 spawns × ~35 ms IS the 18.7 s) was arithmetic derived from the
    wall; the user-time check shows ~276 CPU-seconds of analyzer work is
    the actual bound and the spawns always overlapped it (~15 CPU-s
    aggregate). The runner would regress under E25's per-test processes,
    so it was withdrawn unlanded. Successor: E30. Original entry: 534
    `assert_compiles_and_runs` spawns × ~35 ms of node startup IS
    inference.rs's 18.7 s; execute the emitted programs through one (or a
    few) node processes with per-program output markers. Est: 18.7 s →
    ~3–4 s.


27. **parallelize the docs gate and interpreter cases — SHIPPED
    2026-08-02** (`suite-speed.md` §2 has the numbers) — user-time check
    first per the E26 lesson (docs 99 % CPU, interpreter 104 % — genuinely
    serial, unlike inference), then corpus.rs's 8-way `thread::scope`
    chunk shape applied to both big tests; chunks preserve item order so
    failure reports read identically. Measured: docs 16.78 s → 3.73 s,
    interpreter 15.09 s → 2.78 s (−25.4 s off the serial floor). Both
    gates plant-proven red under parallelism (broken README fence; db.vl
    unexcluded). Original entry: both are serial loops over independent
    compiles; corpus.rs's 8-way `thread::scope` chunks are the proven
    in-suite shape. Est: 16.7 s → ~4 s and 14.9 s → ~4 s.


28. **share the LSP's analyzed test fixtures — CLOSED INTO THE STD-TAX ARC
    2026-08-02** (`analysis-reuse.md` §6 is the continuation) — measured to
    the root: the fixture repetition IS the fixed ~115 ms per-analysis std
    tax (100 % of a trivial compile; ~84 % is `build()` + whole-program
    checks + post-passes over unchanged std; parse is already
    content-cached). No test-side shape survives nextest, so the item folds
    into E3 Phase 3, reopened as the std-tax arc with slices S1–S4 and the
    `VILAN_PHASE_TIMING` instrument (S0, shipped). Original entry: the
    unit binary's 29.9 s is ~81 `Document::analyze` fixture sites at
    ~150 ms each; a `OnceLock`-shared analysis per distinct source pays
    each once.


29. **cut the 16 s relink tax — CLOSED 2026-08-02, overtaken by events**
    (`suite-speed.md` §2's E29 entry is the record) — evidence-first, as
    filed, and the evidence dissolved the item: the 16 s figure does not
    reproduce (the identical touch-probe measures ~3–4 s; vilan-core's
    ~1.7 s no-op incremental recompile is the critical path, links well
    under a second each), and the "no fast linker installed" premise was
    stale at filing — rust-lld has been rustc's DEFAULT linker on
    x86_64-linux since 1.90, verified via `readelf -p .comment` on the
    test binaries. mold was probed anyway: its apparent ~1 s win was
    rebuild-freshness, not mold (the flag never took effect against
    rustc's self-contained lld — check the .comment, not the wall
    clock); its real ceiling over lld is a fraction of a second. The
    per-arc edit tax is ~3 s and recompile-bound; there is no linker
    item left. Original entry: after any vilan-core edit, 43+ test
    binaries relink at ~490 % of 1600 % CPU; sub-levers: a faster linker
    and consolidating same-subject test files.


30. **inference.rs is ~276 CPU-seconds of repeated std analysis — CLOSED
    INTO THE STD-TAX ARC 2026-08-02** (`analysis-reuse.md` §6 is the
    continuation) — profiled as the entry asked: the 170 ms is 100 % std
    tax (a trivial entry against an EMPTY std analyzes in 0.5 ms; against
    the real std, ~115 ms, invariant of entry size), split ~19 ms
    load+walk / ~44 ms `build()` / ~30 ms checks / ~22 ms post-passes.
    Folded with E28 into E3 Phase 3, reopened as the std-tax arc
    (S0 instrument shipped; S1 entry-scoped checks; S2 resolution
    idempotence; S3 frozen generation-0 std base; S4 consumer wiring).
    Original entry: a single ~10-line `assert_compiles` case costs
    ~170 ms of single-threaded pipeline; share the analyzed std across
    the harness's compiles.


### F. Backend & platform

5. **Project-model deferrals — SHIPPED 2026-07-25 (F5 S4+S5, distribution.md
   is the record)** — git dependencies (9c7567e: {git, tag|rev} pinned,
   content-addressed cache, offline-faithful, LSP never fetches),
   `[project.dependencies]` inheritance (dcf32eb: explicit opt-in
   `{ project = true }`), server-side manifest completions + the vilan.toml
   LSP diagnostic channel (same commit). Registry-dependency loading stays
   demand-gated per ratified (e) — the `registry` field parses and errors
   "not yet supported"; a true registry is a D5-era decision.


7. **Distribution Phase 2 — BUILT 2026-07-25, all channels (F7 S1-S3;
   distribution.md is the record)** — npm (@vilan-lang/vilan + 5 platform
   packages, 826a76f), VS Code Marketplace + Open VSX (publish jobs + brand
   icon, c38c23e), the Homebrew tap (formula + CI updater, 08ce573;
   vilan-lang/homebrew-vilan LIVE with real v0.14.0 checksums — brew install
   works today). Publish jobs are disabled-until-secret; the next release cut
   after provisioning publishes everywhere at once. README install lines flip
   when channels go live (§7). winget = §10 recorded follow-up.
   **PROVISIONED 2026-07-29** — and three of the four credentials are not
   what this entry originally named, because each channel's token story moved
   underneath it:
   - **npm** — was `NPM_TOKEN`, now trusted publishing (OIDC). The token was
     only ever a BRIDGE: npm is retiring 2FA-bypass tokens (account changes
     Aug 2026, direct publishing ~Jan 2027), and OIDC cannot perform a
     package's FIRST publish, so the token existed to create the six packages.
     It did, at v0.18.1; `publish-npm` was rewritten the same day and no
     longer reads a secret. **PROVEN 2026-07-29 by v0.18.2**, a release cut
     for exactly that purpose — npm offers no way to read a trusted-publisher
     config back, so a release was the only instrument that could confirm the
     six were right. All six published by OIDC with signed provenance;
     `NPM_TOKEN` is revoked and gone from repo secrets, with no fallback
     credential by design. `distribution.md` §2 + §7 carry the mechanics.
   - **VS Code Marketplace** — `AZURE_CLIENT_ID` / `AZURE_TENANT_ID`, NOT
     `VSCE_PAT`. Azure DevOps retires global PATs (the "all accessible
     organizations" scope `vsce` requires) on 2026-12-01, so a PAT was worth
     four months. Entra workload identity federation instead, bound to the
     repo's `marketplace` environment — no stored credential. The last step
     cost hours and the answer is one sentence: **a service principal has no
     Marketplace identity until it first authenticates**, so every identifier
     the Azure portal can show you is rejected, and the way to obtain the real
     one is to let a publish FAIL and read the identity out of the error.
     Full account in `distribution.md`.
   - **Open VSX** — `OVSX_TOKEN`, set. The real gate turned out to be the
     Eclipse **Publisher Agreement**, not the namespace: a valid token cannot
     publish without it. An unverified namespace publishes fine (warning icon
     instead of the shield); ownership is claimed separately and can follow.
   - **Homebrew** — `TAP_APP_ID` / `TAP_APP_PRIVATE_KEY`, NOT `TAP_TOKEN`. A
     GitHub App, because a release-time-only PAT expires unobserved and the
     disabled-until-secret gate cannot catch it (an expired token is still a
     non-empty string).
   The through-line: a credential used only at release time must not be one
   that expires, and two of the four ecosystems were mid-migration away from
   long-lived tokens when we arrived.


8. **NEW — Windows support** (L; **ARC ACTIVE — proposal `windows-support.md`
   RATIFIED 2026-07-24**, all §10 calls settled) — first-class native Windows
   for the toolchain: the CRLF string-literal miscompile (the one correctness
   bug), `.gitattributes` + honest corpus diagnostics, suite compile-clean +
   green on Windows, PR CI born (ubuntu + windows), path/case semantics
   (case-exact module resolution), Job-object teardown + VT + ariadne color
   gating (+ errors→stderr, cross-platform), LSP URI keys + extension `.exe`
   discovery, msvc release leg + `install.ps1` + upgrade's rename-aside dance.
   Slices S0–S6 in the proposal; supersedes `releases.md`'s WSL-only record.
   Prerequisite for F7 distribution (user call 2026-07-24). **ARC FULLY
   COMPLETE 2026-07-24** — v0.14.0 LIVE (f6171a7: the whole arc as a release,
   msvc leg green first run), windows CI leg REQUIRED, user's live-host pass
   clean. Residuals in `windows-support.md` §12; upgrade's rename-aside dance
   self-verifies at the next release.


9. **repo → GitHub org — SHIPPED** (`5bb74b9` the sweep + gate, `0a0bdd4` the
   book-host move; D10 carries the full evidence and the open tail — read that
   one, this entry is the distribution-side framing of the same arc) — the org
   is `vilan-lang`, so the npm scope (`@vilan-lang/vilan`), the marketplace
   publisher id, and the tap owner (`vilan-lang/homebrew-vilan`) all match the
   repo, which was the prize. Both named non-redirecting hazards are handled:
   (a) the Pages tombstone chain is live and verified end to end 2026-07-29
   (a ≤v0.14.0 binary's hover deep link lands on a 200; see D10), and (b) the
   hardcoded owner strings are swept with a hygiene gate that keeps them swept.
   The sequencing worked as planned: this landed BEFORE F7's channel accounts
   were provisioned, so no channel identity needs migrating — though as of
   2026-07-29 those accounts are still unprovisioned and every publish job
   skips (F7). Original entry: (S–M execution, but PLAN FIRST; user request
   2026-07-25, raised at F7 ratification) — with publishing spreading to
   npm / marketplace / brew (+ winget later), the owner identity stops being
   one repo's cosmetic detail: every channel bakes it in (the `@vilan-lang`
   npm scope, the marketplace publisher id, the tap repo's owner, winget's
   package identifier, and the URLs embedded in released binaries and docs).
   Moving `ReedSyllas/vilan` to an org (e.g. `vilan-lang/vilan`) is
   well-supported by GitHub transfer — git redirects cover clones AND
   `releases/download/…`, so **existing installed binaries' `vilan upgrade`
   keeps working** as long as the old name is never reused — but two things
   do NOT redirect and need a plan: (a) **GitHub Pages**: the book moves
   `reedsyllas.github.io/vilan` → `<org>.github.io/vilan` with no redirect —
   and released binaries deep-link the book from editor hovers, so old
   binaries' links would 404 unless the old Pages site is kept as a
   tombstone/redirect page; (b) hardcoded owner strings in the repo
   (`DEFAULT_BASE` in upgrade.rs, install scripts, README, extension
   `repository`, docs) need one sweep — trivial, but versioned: binaries
   released BEFORE the sweep carry the old URLs forever (the redirect saves
   upgrade; the Pages tombstone saves hovers). Decisions for the plan: org
   name (interacts with the npm scope — same name everywhere is the prize),
   pseudonym discipline (an org has members/visibility surface — see
   `going-public`), timing (**before F7's account provisioning** — creating
   channel accounts under the org from day one avoids every migration), and
   whether kolt/other repos move too. SEQUENCING (user, 2026-07-25): decide
   F9 before F7's §7 accounts are created; F7's CODE (S1–S3) is
   identity-independent and does not wait.


10. **Third-party notices for the distributed binaries — SHIPPED 2026-07-28**
    — the vsix precedent (a CHECKED-IN notices file) turned out to be the
    right shape here too: `THIRD-PARTY-NOTICES.txt` is generated by
    `cargo-about` (`about.toml` + `about.hbs`, whole-lockfile superset — no
    per-target filtering, so the completeness gate stays an exact
    `Cargo.lock` walk) and committed, with a suite test
    (`third_party_notices.rs`) that fails when the lock gains a crate the
    file doesn't cover — no CI tooling, no release-time generation. Wired
    everywhere the recon found (the backlog's "one cp" undercounted):
    release.yml's unix cp + explicit tar member list and the windows
    Copy-Item; npm-package.sh (platform packages from the unpacked archive
    with a repo-root fallback, meta from the root) + the `files` allowlist
    in all six package.json manifests (npm silently drops unlisted files);
    the brew formula generator + checked-in formula + the pinned test
    (`prefix.install`, conditional so old archives still install); and the
    recon's real find — `vilan upgrade` installed only the two binaries, so
    upgraders would never have received the file — now copies the licenses
    and notices best-effort. install.sh/install.ps1 extract whole archives
    and needed nothing.


### H. Parser & grammar

6. **Handwritten recursive-descent frontend — SHIPPED 2026-07-22, arc complete**
   (`frontend.md` is the record; taken deliberately as structural investment,
   trigger preempted by user call 2026-07-21) — chumsky deleted (8 files,
   4,601 lines + the dependency); measured: `build` 466–509→181 ms (~2.7×),
   5.21 B→2.01 B Ir, frontend share of a compile ~63%→3.7%; six
   differentially-gated slices, 279/279 whole-file agreement + 97/97 corpus
   byte-identity through the new frontend BEFORE wiring; improved parse
   diagnostics live (ledger rows 204–208; missing-separator typos now locate);
   the prefix-salvage behavior change shipped (KEEP, nine-row inventory in
   frontend.md). E4 now unlocked; E13 (formatter bails) still open; the
   grammar freeze is lifted.


7. **Interpolated triple-quoted strings — SHIPPED 2026-07-26 (v0.16.0 grind
   6, 1442586)** — the ratified escape rule (exactly `\{`/`\}`, holes,
   near-rawness; trimming×holes stated as spec text); multiline_layout shared
   with plain `"""` (2M-input differential-proven); the review's
   compound-truncation fmt block fixed (recovery only ever EXTENDS a span).
   Original entry: (S; split from shipped H4) —
   `i"""…"""` needs its escape story (raw braces vs `{expr}` holes conflict); settle
   it, then ship. The macro-authoring payoff. **Recorded revisit (2026-07-24,
   from `windows-support.md` §2):** once this ships, revisit disallowing
   multi-line single-quoted strings (plain and `i"…"`) — today multi-line
   i-strings are the macro-authoring idiom with no replacement, so v1 of the
   newline rule normalizes `\r\n`→`\n` in them instead of banning them.


8. **NEW — Element syntax — ARC COMPLETE 2026-08-01, all five slices
   SHIPPED** (L; proposal `element-syntax.md` RATIFIED same day; branch
   `element-syntax`, unmerged) — HTML-flavored sugar lowering to the `view` chain in the
   pre-analysis lift slot; zero lexer changes (`<` is free in atom position;
   text children are QUOTED — the context-free lexer is load-bearing). Head
   items: undotted `name(value)` is an attribute → `.attr("name", value)`;
   dotted `.method(…)` is the chain, verbatim; `on:click(h)` dispatches
   on/on_event by literal arity. Children (elements, strings, `{expr}` holes)
   lower to `.child`; static-vs-reactive rides the value's type via new
   `Slot`/`AttrValue` traits with `child`/`attr` WIDENED (probe green
   2026-08-01: bounded generics monomorphize to direct calls; corpus churn is
   symbol renames only). Self-closing tags space before `/>` (formatter
   normalizes). Slices: S1 std text-node children + traits — **SHIPPED
   2026-08-01** (standalone payoff: kills the website's pt()/t()
   span-fabrication idiom; proposal §10 has the record). ~~Recorded residual
   (S1): trait-dispatched context requirements union across impls~~ — CLOSED
   2026-08-01 post-arc (v0.21.0's deploy surfaced the first casualty, the
   playground styles example): coverage follows the recorded instantiation at
   `OnConstraint` sites; static slots unfenced, Signal arms fenced, generic
   forwarders conservatively fenced (requirement polymorphism = the recorded
   refactor — **SHIPPED 2026-08-02 as B51**, forwarders now resolve per call
   site); threading stays union-based. Five pins, two proven red first.
   → S2 grammar + desugar — **SHIPPED 2026-08-01** (proposal §10: lowering
   proven byte-identical to the chain; fmt prints markup verbatim until S3;
   bycatch fixed root-cause: bound-generic method calls on unannotated
   closure params froze abstract and silently misrendered — the method path
   now defers like the free path, plus a never-silent bound audit)
   → S3 formatter — **SHIPPED 2026-08-01** (canonical element layout: inline
   ≤1 non-element child, children one-per-line, signature-shaped head split;
   `ElementChild::Hole`/`self_closing` added for token fidelity; ten
   assert_construct pins; proposal §10) → S4 spec/book/TextMate/highlight.js +
   both deferred diagnostics — **SHIPPED 2026-08-01** (per-source-text
   plumbing `Analyzer.source_texts`; the view import note + the `text(…)`
   attribute warning; guide chapter, spec productions, JSX phrasebook rows,
   markup highlighting in both editor grammars; proposal §10) → S5 LSP tail +
   the exhibit — **SHIPPED 2026-08-01** (Tag semantic tokens from a raw
   parse, zero-width scaffolding spans, `close_tag` in the AST,
   linkedEditingRange end-to-end; website branch `element-syntax-exhibit`
   d4ee524 rewrites art.vl's diagram(), ships after the next release). Deferred entirely: component tags, fragments, bare text, the
   `=` attr spelling.


9. **`mut` parameters — SHIPPED 2026-08-03** (proposal
   `mut-parameters.md`, semantics = the desugar `fun f(mut x: T) {b}` ≡
   `fun f(x': T) { mut x = x'; b }`; parser+analyzer+formatter+spec+tour;
   14 pins incl. mut self, closure forms, conformance-ignores, resource
   and convention rejections, three mechanisms planted; corpus fixture
   `mut-parameters.vl`; the parameter immutability diagnostic now offers
   BOTH `mut x` and `&mut x`. Ship record incl. two discovered
   pre-existing gaps in `mut-parameters.md` §6 — one filed as B53).
   Original entry (S–M; tester report 2026-08-03, verified) —
   `|mut v| { ... }` fails to parse,
   and so does `fun f(mut x: i32)`: the convention grammar is `"own" | "&"
   ["mut"]` (spec grammar.md:70) — there is no plain-`mut` parameter form
   anywhere, while `mut` BINDINGS (`let = ("let"|"mut") binder`,
   grammar.md:158) and `mut` PATTERNS (grammar.md:335) both exist. So this
   is a consistent grammar gap, not a closure-specific bug, and the
   workaround is a noise line: `|temp| { mut v = temp; ... }` (the field
   case was mutating a `Signal<List<T>>` via `set_with` — see A18). The
   fix is principled under value semantics: `mut x` on a parameter is
   LOCAL rebindability of the callee's copy, no caller-visible effect, no
   interaction with `own`/`&`/`&mut` conventions (which stay
   prefix-exclusive with it). Touches lexer-adjacent nothing: parser
   (parameter rule in both positions), analyzer (parameter binding
   mutability — reuse the `mut`-binder path), spec grammar.md + names.md,
   docs tour. Pin the edge cases per CLAUDE.md: `mut` + type annotation,
   `mut` in multi-parameter lists, closures with and without annotation,
   and the rejection of `mut` combined with a view convention if that is
   the settled rule.

### Moved at the v0.24.0 cut (2026-08-03)

#### A18. signals of collections: the mutate-in-place gap — SHIPPED 2026-08-03

(design (a) `Signal::update` per owner call, same day the entry was filed;
full record in `signal-update.md`. Original entry:) (M; user
report 2026-08-03; design first, sized without it) — mutating a
`Signal<List<T>>` today is `signal.set_with(|list| { ... })`, a
copy-transform-return dance (`reactive.vl:419`: `set_with(self,
transform: sync |T| T)`), made worse by the missing `mut`-parameter
form (H9) — the tester's real code needed `|temp| { mut list = temp;
list.push(5); list }`. `Shared` already has the right shape for the
STORAGE half: `write(self): &mut T borrows self` (`shared.vl:28`), but
a signal write must also NOTIFY, so a bare view is not enough. Two
candidate designs, one to be picked on paper: (a) **`update(self,
mutate: sync |&mut T| void)`** — the closure receives a writable view
of the stored value, the runtime notifies once after it returns;
composes with `batch` coalescing for free; smallest surface. (b)
**`write(self): SignalWrite<T>`** — a guard whose view mutates storage
and whose `drop` notifies (rides C4 Tier-1 deterministic destruction);
reads best (`signal.write().push(5)`) but hangs semantics on
temporary-drop timing and needs a rule-4 story for a held guard across
a re-entrant read. Either generalizes over EVERY collection — a
dedicated `ListSignal` was considered and rejected as non-general
(Set/Map/user types would each need a twin; the user's own framing).
Note this is the same gap Solid-family frameworks carry; the memory
model's views are exactly the tool they lack.

#### B53. Pattern captures alias their source — SHIPPED 2026-08-03, COMPLETE

(`0835c7d` in v0.23.6 was the first half — destructure + unguarded match
legs via `compute_capture_clone_sites`/`compile_pattern`, the SHARE and
MOVE elisions, 5 pins, corpus fixture `capture-clones.vl`, 12 goldens
runtime-verified. An adversarial review the same day found it HALF-fixed:
`is` captures and GUARDED match legs compile through the sibling
`compile_is_pattern` path and still aliased; the conservative
generic-capture clone deep-copied RESOURCES (violating `memory.md` R11,
e.g. through `Option::unwrap()`); the SHARE elision composed unsoundly
with rule 2's move elision; and seam detection missed non-place tails
(braced blocks, `if`-expression value positions). The follow-up arc
closed all four the same day — `materialize_capture_clones` on the
`is`-path with guard-ordering decided (copy on leg entry; a rejecting
guard copies nothing), resources MOVE out of generic captures via
per-instantiation `resource_triggering_constraints` (reaching through
`Wrap<T>`, wider than the review filed), the capture pass stratified
before `compute_clone_sites` so SHARE is never an elidable-copy source,
tail-leaf seam collection, and `mut [a, b]` stamps its elements in
`match`/`is` exactly as in `let`. 14 pins, 10 red-first against v0.23.6;
only the rewritten fixture's golden changed — every other corpus golden
byte-identical, which is the SHARE elision's survival proof. Two open
holes found on the way are filed as B59 (guard-hoisted temporary) and
B60 (self-by-value is not a move); full record in `capture-clones.md`.
Original entry:) (S–M;
found 2026-08-03 while building H9, pinned as an `#[ignore]`d test
`a_mut_destructure_capture_does_not_alias_its_source`) — rule 1 says
a binding copies its aggregate place, and `mut copy = single` does
(`compute_clone_sites` reads `Variable.initial`), but a DESTRUCTURE
capture does not: `mut (xs, n) = pair; xs.push(9)` grows `pair.0`
too (v0.23.5 prints "3 3" where rule 1 says "3 2"). The transformer's
`Expr::Destructure` arm binds captures from positional slots of a
temp (`const __d = value; const xs = __d[0]`) with no `__clone` on
any slot, and `compute_clone_sites` has no Destructure arm. Mostly
masked because binder-element captures parse immutable (even under
`mut (a, b)` — `apply_binding_mutability` leaves tuple/array
elements untouched, itself a quirk worth folding into the fix) and
an immutable alias is unobservable; `set_pattern_bindings_mutable`
(`mut` destructuring let) opens the observable path. Fix at the root:
clone captures per rule 1 (with the same elision rules), not just
mutable ones — then decide whether `mut (a, b)` should stamp its
elements mutable while in there.



### Moved at the v0.25.0 cut (2026-08-04)

#### A19. `Signal<resource>` slips R10 — SHIPPED 2026-08-04

(S; found 2026-08-03 by the A18 arc) — `Shared<Database>` is refused by R10,
but `Signal<Database>` compiles clean: the check keys on the WRITTEN type's
head, so a resource reaching `Shared` through a generic struct field is
invisible to it. Fix direction: R10 consults the instantiated field types
(the same per-instantiation resolution B53's follow-up used for capture
clones). Record: `signal-update.md` §6.

#### A20. `map`/`filter`/`sort_by` element aliasing — SHIPPED 2026-08-04

(M; found 2026-08-03 by the I4 arc, pre-existing — verified on the pre-I4
tree for `map`/`filter`) — writing through an element of the RETURNED list
shows in the receiver: the list-producing methods copy the spine, not the
elements. One shared value-semantics hole, the same family as B53/B54
(rule 1 at a list-producing seam); `sort_by` was deliberately kept
consistent with its siblings rather than made unilaterally stricter, and
new `reverse` happens not to alias (it rebuilds through `push`). Wants one
slice with B53's SHARE/MOVE elision reasoning applied across all three.
Record: `std-surface.md` §7. [Ship note: the "reverse does not alias"
claim was WRONG — `self[index]` hands elements over uncopied; all four
methods aliased. See `element-clones.md`.]

#### B54. place-into-construction sharing — SHIPPED 2026-08-04

(S–M; split from B53's ship record, `0835c7d`, v0.23.6, filed 2026-08-03 by
the reconciliation sweep — the commit's own message named this as a
backlog-recorded open edge and it had not actually been filed anywhere) — a
place flowing into a CONSTRUCTION (a list/struct/variant literal payload:
`List { a, b }` or `[x, y]` where `a`/`b`/`x`/`y` are existing places) is a
separate, pre-existing sharing question from B53's capture aliasing — not
capture-specific. Undesigned: whether rule 1's per-binding copy applies
uniformly to a place read into a literal's field/element position, and what
elision (if any) is sound there by the same SHARE/MOVE reasoning B53 used
for pattern captures. B53's follow-up arc closed 2026-08-03 WITHOUT
absorbing this (its `capture-clones.md` §5 records different holes) — this
entry stands on its own. No repro is recorded yet; needs one before design.
Related family: A20 (list-producing methods share elements).

#### B55. a bounded-generic call through a re-dispatched callee emits an empty function body — SHIPPED 2026-08-04

(M–L; found 2026-08-03 by I3's design probes — SILENT MISCOMPILE) — a
generic function whose bound-satisfying call goes through a re-dispatching
callee (`self.upstream.next()` — the adapter shape) compiles clean and
emits an EMPTY body: exit 0 at compile time, `TypeError` at runtime. Two
verified triggers: the `for`-loop protocol edge and a trait-default
constructor. Bites any user writing a generic wrapper over an iterator
today; the long pole for I3's adapter layer. Repros in
`iterator-adapters.md` (P2). [Ship note: TWO root causes, not one — the
loop arm's bare-id emission clearing the substitution, and whole-type
`Self`-return specialization; plus the never-silent guard.]

#### B56. `for v in self` over a generic lowers to a native `for...of` — SHIPPED 2026-08-04

(M; same probes — SILENT MISCOMPILE) — inside a generic method,
`for v in self` skips the iterator protocol and emits a native `for...of`
over the struct's flat field array: wrong elements, wrong count, no
diagnostic (a probe `to_list` returned 2 elements of a 3-element source).
Repro in `iterator-adapters.md` (P3). [Ship note: the genuinely-missed
subjects were `Type::Trait` and `Type::Generic`; a bound-less generic
subject is now a clean compile error.]

#### B59. a guard needing a hoisted statement emits a dangling reference — SHIPPED 2026-08-04

(S–M; found 2026-08-03 by the B53 follow-up arc, pre-existing on v0.23.6) —
`compile_is_pattern`'s guarded-leg arm walks the guard expression into a
`guard_block` that is never emitted (an else-if chain has no statement slot
before a leg's condition), so any `is`/`?`/nested `match` INSIDE a guard
drops its temporary: `if ($c[0] === 0)` with no `$c` — a runtime
`ReferenceError` from a cleanly-compiling program. Pinned `#[ignore]`d as
`a_guard_that_needs_a_temporary_emits_it`. Fix direction: give guard
lowering a statement slot (hoist the leg chain out of expression position
when any guard needs one). Record: `capture-clones.md` §5.

#### B60. a self-by-value call is not a move to the affine checker — SHIPPED 2026-08-04

(M; found 2026-08-03 by the B53 follow-up arc, pre-existing in both
directions) — `o.unwrap()` consumes `self`, but the checker records no
move: `o.is_some()` afterwards compiles clean and `o`'s scope-end teardown
still fires, so one resource value is destroyed twice (the payload the
caller now owns AND the source's copy). The follow-up arc made the capture
itself MOVE correctly; this is the remaining source-side hole. Pinned
honestly as `a_moved_resource_instantiation_destroys_one_value` (asserts
the current double output so the hole stays visible). Fix: R11's
move-out-of-`self` calls join the affine use-once accounting. Record:
`capture-clones.md` §5. [Ship note: the premise was inverted — bare `self`
is a LOAN per R3; what shipped is "a body may only consume what it owns"
plus `own self` on Option's consuming combinators. See `affine-moves.md`.]

#### B61. `sync` is unenforced for a void-returning closure parameter — SHIPPED 2026-08-04

(S; found 2026-08-03 by the A18 arc, pre-existing, no std involved) —
`sync || void` accepts an awaiting closure; the identical signature
returning `i32` refuses it. So a `sync` declaration on a void callback is a
correct declaration that does not yet bite. Pinned `#[ignore]`d in the
signal-update arc's tests. Record: `signal-update.md` §6.

### Moved at the v0.26.0 cut (2026-08-04)

#### B58. a bound on a trait's own generic parameter does not reach its default bodies — SHIPPED 2026-08-04

(M; found 2026-08-03 by I3's design probes) — a `trait X<T: Bound>`'s default
member bodies cannot use `Bound`'s members on `T`-typed values: the bound is
enforced at impl sites but is not IN SCOPE inside defaults. This — not
associated types — is what blocks I3's headline blanket-adapter direction
(`xs.filter(f)` via `Iterable` defaults). Evidence in `iterator-adapters.md`
(P4). Probed 2026-08-04 by the B55/B56 arc: NOT the same mechanism
(bound-to-bound flow at `satisfies_trait_bound`), behavior byte-identical
after those fixes — stands on its own. [Ship note: the probe's steer was
wrong on both counts — the real causes were empty-substitution default
emission (transformer) and return-type inference re-binding a shared trait
parameter (analyzer); the spec gained the bound-as-assumption dual rule.]

#### B62. a match arm capture of a resource payload is never destroyed — SHIPPED 2026-08-04

(M; found 2026-08-04 by the B60 arc, verified pre-existing on v0.24.0) —
`match o { Some(let r) => .. }` on an `Option<resource>` prints NO drop for
`r`: the capture enters no owner's scope-end plan. Extra urgency: `match` is
exactly the idiom B60's new conditional-move rejection steers users toward,
so the recommended path leaks the resource the rejected path double-dropped.
Fix direction: a pattern capture of a resource joins the leg scope's owned
set (the same plan_expr accounting bindings get). Pin red-first per payload
shape (match leg, `is` capture, nested). Record: `affine-moves.md` §6.
[Ship note: the shipped rule keys on the SUBJECT — a capture of a consumed
subject owns like a `let`, a capture of a loaned subject enrolls nothing;
the twin `let`-destructure hole was fixed in the same arc; the loan-side
consume hole is filed as B65.]

#### B63. Option's remaining combinators at resource instantiations — SHIPPED 2026-08-04

(S–M; from the B60 arc, 2026-08-04) — after B60, `is_some_and`, `ok_or`,
`unzip` REJECT at a resource instantiation rather than silently
double-destroying (convertible to `own self`; blocked on separating the
elision predicate from `readonly_root`, a diagnostic helper reused for a
semantic decision), and `or`/`or_else`/`xor`/`inspect`/`eq`/`unwrap_or`
CANNOT be move-clean under R6 (each reads `self` twice or duplicates the
payload) — rewriting them over `is` tests is a std slice. Also reconcile
spec §6.3's bare-parameter table with R3: they disagree for resources (the
implementation enforces R3's loan reading). Record: `affine-moves.md` §6.
[Ship note: the filed analysis was corrected in three places — `eq` never
rejected (nothing was pinned), `unwrap_or` was a discard case not a
two-read case, and the real law is "a generic body cannot destroy a T";
`inspect`/`or_else` work after the `is`-test rewrites; only
`or`/`xor`/`unwrap_or` reject, correctly, one error each.]

#### B64. a closure returning a captured local still aliases — SHIPPED 2026-08-04

(S–M; found 2026-08-04 by the A20/B54 arc) — the return-clause copy keys on
by-value PARAMETERS; a closure that returns a local it captured from the
enclosing frame hands back live storage the same way `fun identity(c) { c }`
did before the fix. Same store-rule reasoning applies; needs the
capture-root walk. Record: `element-clones.md` §7 (which also records two
deliberate non-takes: the caller-side escape summary that would free builder
chains without the `own self` opt-out, and `Set::insert`'s undeclared `own`
— unobservable today). [Ship note: reframed on FRAMES — the returning frame
owns what it DECLARED; the fix also covers an `own`-parameter capture
handed out repeatedly, which the "capture like a bare parameter" reading
would have missed; bycatch fixed a pre-existing R9 false positive.]

#### A21. View.style_var leaks its subscription — SHIPPED 2026-08-04

(S; found 2026-08-04 by the A8 arc) — the only reactive `View` method built
on `source.sub(..)` + `let _sub` instead of `effect`, so its subscription
outlives the view's boundary: a swapped-out view keeps reacting. A
reactive-ownership bug, not a styling one. Fix: route it through `effect`
like every `bind_*`; pin the disposal (swap out, fire the signal, assert no
write). Record: `ui-styling.md` §0bis. [Ship note: the ssr.md residue about
the DOM stub no-oping style.setProperty proved FALSE — the stub has folded
the property into the style attribute since 309e2bb; style_var now lives in
the shared component, both twins byte-identical.]

#### E32. the cancellation test family is wall-clock-bounded — SHIPPED 2026-08-04

(S; observed independently by four lanes, 2026-08-04; verified on unmodified
base commits) — four tests
(`a_fast_failure_behind_a_slow_sibling_reacts_at_settle_time`,
`cancel_cuts_a_sleeping_child_short_and_keeps_the_value`,
`outer_cancel_chains_into_nested_nurseries`,
`nested_nurseries_join_inside_out`, inference.rs ~21829 and neighbours)
assert `started.elapsed() < 4s` where elapsed INCLUDES a ~4.3s debug-build
compile plus a node spawn — under nextest's full parallelism the compile
alone eats the budget, and which test fails varies per run (the flake
signature). Fix: measure the program's own runtime (split the compile out of
the timed window), not the harness wall clock. All four pass in isolation in
~2.3s. [Ship note: only THREE of the four ever carried the assertion —
`nested_nurseries_join_inside_out` was drift in this very entry; the fix
split compile()/run_js() with budgets unchanged, plant-proven per test,
stress-validated under two concurrent full suites.]

#### E33. the benchmarks e2e binds a fixed port — SHIPPED 2026-08-04

(S; found 2026-08-04) — `benchmarks_run_and_report_the_deterministic_counts`
binds `:48231` and collides with concurrent e2e legs under nextest
(`EADDRINUSE`). Fix: port 0 + read-back, the E19 precedent
(`Server.port()`). [Ship note: FOUR fixed ports existed, not one —
48231–48234 across throughput.vl and realtime.vl; all migrated; the literal
collision was reproduced pre-fix against held ports and the fixed tests
survived the same scenario.]

### Moved at the v0.27.0 cut (2026-08-04)

#### A16. Bundle splitting — ARC COMPLETE 2026-08-04

(Full lineage: filed as M–L user request 2026-07-24, proposal-first; the
proposal drafted 2026-08-03 from two source sweeps; S1 --print-chunks shipped
2026-08-03 as the measure-first gate; S2 emission shipped 2026-08-04 —
`[entry.<name>] split = true`, partition after the single rename so chunk
bodies are byte-identical to the single-file build, the runtime registry,
`View.swap_split` degrading to `swap` with no chunk map, four plant-proven
gates; S3 shipped same day — initial-route preload planted by the emitter
before the swap mount, `chunk_error()` + the stuck-`pending()` fix, the
generation guard on Draft::push's shape, the namespace sweep for stray
chunks, the per-leg split-cost warning; S4 decisions — `run` ignores `split`
in every form by the single-file-dev doctrine (fixing a three-way
inconsistency), fullstack's server reads chunks.json, the playground is
guarded at the source level because its `std_sources` is empty. Break-even
remeasured at ≈6KB fixed cost per split leg — more than double S2's figure
once the gate machinery rides along — so no in-tree example declares
`split = true` and the take-up instrument is `--print-chunks`' verdict going
positive on a real app. Records: bundle-splitting.md, complete.)

#### A22. same-family style rules resolve by stylesheet order — SHIPPED 2026-08-04

(S–M; found 2026-08-04 by the A8 value-type slice, pre-existing) — atomic
longhand and shorthand rules of one family were equally specific, resolving
by the lexical sort over content-hashed class names. The measurement changed
the design: the hazard was LIVE in two production sites (the website's
`margin-left: auto` flex-push was dead), and one sat on the runtime-legal `+`
merge path, which killed the resolve-at-build candidate (a split cannot emit
from `add`). Shipped: an object-level family drop (a later shorthand clears
its family's slots — a map removal, so `+` does it too) plus the `*.sX`
selector marker riding B35's existing lexical sort so shorthands order ahead
of longhands at identical specificity. Zero classes renamed — the re-mint
permission went unspent. Family inventory grew inset/background/flex; `raw`
participates by property name (border_none() IS raw). One documented hole:
two longhands covering different parts of one family tie, zero instances.
Record: ui-styling.md §0bis.4.

#### B65. a capture of a LOANED subject can be consumed — SHIPPED 2026-08-04

(S–M; found 2026-08-04 by the B62 arc, pre-existing) — B60's "a body may
only consume what it owns" had no CAPTURE twin: `if o is Some(let r) {
sink(r) }` and `match &o { Some(let r) => sink(r) }` both compiled and
double-destroyed. Shipped: collect_loaned_pattern_captures maps every
capture bound by a loaned subject to that subject (all three loan forms),
riding MoveScan so R11's per-instantiation scan covers it free; its own
diagnostic steers to consuming the subject — "copy the payload" proved an
impossible steer (no Clone/Copy exists; copying is implicit for data and
forbidden for resources). Record: affine-moves.md §9.

#### B66. a generic body's capture leaks at a resource instantiation — SHIPPED 2026-08-04, WIDENED

(S–M; found 2026-08-04 by the B62 arc, pre-existing) — the filed capture
case generalized to the honest rule: plan_scope's `dropped` set is exactly
the teardowns a generic body cannot run, so the check asks about all of it —
captures, let-locals, and the R2 overwrite position (`mut held = a; held =
b`) found and closed in the same arc rather than filed. The compat sweep
also proved B63 §8.2's claim that `map`/`is_some_and` were clean FALSE —
closure-valued callees loan their arguments, so both leaked at a resource;
pins corrected to assert rejection. Record: affine-moves.md §9.

#### B67. branch-divergent moves leak — SHIPPED 2026-08-04

(M; found 2026-08-04 by the B63 arc, pre-existing) — `pick(true, Some(a),
Some(b))` with two `own` parameters moved on different branches compiled and
destroyed nothing. The filed diagnosis (intersection merge wrong for
leak-finding) was half wrong: intersection is exact for both questions GIVEN
R7 — the defect was R7's reach cut by an over-broad R4 tail exemption.
Shipped: the exemption replaced by `is`-refinement, with `or_else` and `or`
proven legal in the design note before implementation (removing the
exemption alone broke exactly those 2 of 1442 pins); the diagnostic reuses
R7's ConditionalMove verbatim. Record: affine-moves.md §9.

#### E34. the std::ui twin-surface parity gate — SHIPPED 2026-08-04

(S; found 2026-08-04 by the A16 S2 arc) — a surface added to browser
`std::ui` had to be manually mirrored or process builds broke at analysis.
Shipped: each twin analyzed on its platform, surfaces read off the Program
(module-scope bindings by source + declared types' members), divergences
allowlisted with per-name reasons under the distilled rule — emitter-selected
names are safe browser-only, user-bindable ones are the hazard. `ui` is the
only twin today, and the twin inventory itself is gated so std cannot grow an
ungated one. Fired correctly the same day on the A16 S3 surfaces — its
designed collision, resolved by allowlisting with reasons. Record: the gate
file's own header (crates/vilan-core/tests/std_twin_parity.rs).
