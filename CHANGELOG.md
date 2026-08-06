# Changelog

Vilan is a fast-moving alpha. Minor versions (`0.X`) may change the
language, the standard library, and the wire protocol without a
deprecation period; patch versions are fixes. Each release below links
the highlights — the [book](https://vilan-lang.org/docs/) always
tracks the latest state.

## Unreleased

**Writing `impl List<type T>` in your own file no longer makes the compiler complain about std's `List`.** Roughly half the time — the same file, the same compiler, a different answer per run — a user impl block on a std container produced "the type of 'result' is never fully determined", pointing at a `mut result = List::new()` line inside `list.vl` that you did not write and cannot annotate. The other half of the runs were clean, which is the tell: the diagnostic was never about your program.

The residual-generic check asks whether a binding's leftover type parameter is declared in the binding's own file — legitimate inside a generic function's body, a real leak when it is some callee's parameter arriving from elsewhere. It answered that by recording one declaring file per parameter. But a parameter can honestly be declared in several: `impl Subject<type T>` deliberately *inherits* the subject type's own parameter identity, so that writing the binder means exactly what writing the subject's bound out means — and your `impl List<type T>` therefore **is** `list.vl`'s `T`. Recording one file for it collapsed a set into a coin flip, and the coin was a randomly seeded hash table's iteration order, so which file "declared" `T` changed run to run. When your file won, every `List<T>` inside `list.vl` read as a foreign leak.

The check now keeps the set of declaring files and asks whether the binding's own file is among them, which is the question the rule always meant to ask and does not depend on iteration order. The leak it exists to catch is untouched — an unannotated `Map::new()` or `Set::new()` still requires its annotation, including in a file that also writes `impl Map<type K, type V>`, because a static's own parameters are not the struct's. Only the first compile in a process could ever show this: every later one is served from the reuse cache, which always held the clean world. Pinned at 30 cold compiles with the cache cleared per attempt, in both directions.

**`for pair in [(1, "a")].iter()` gives you the pair.** It gave you "cannot access field '0' on type T" — the loop binding came out as the iterator's own generic parameter instead of the type it was instantiated at. The same iterator pulled by hand, `if it.next() is Some(let pair)`, always worked, and so did `for pair in [(1, "a")]` straight over the `List`; only the loop over an iterator lost it.

It is not a tuple problem. The loop reads its element off the declared return type of the subject's `next` — `impl ListIterator<type T> { fun next(..): Option<T> }` — and took that `T` verbatim, without instantiating it against the receiver's arguments the way the trait-bounded arm beside it always did. A bare `T` admits nothing, so **every** element shape refused: a struct's field, a nested `List`'s `len()`, an `Option`'s `unwrap_or`, and calling a closure element at all ("cannot call this as a function: it is T"). The `&mut` lending form (`next_mut`) over a generic container had it too, as did an enum-shaped iterator.

`enumerate` and `zip` hid it for the whole adapter arc, and now it is clear why: their payloads are written structurally, `Option<(i32, T)>` and `Option<(T, U)>`, so the loop saw a tuple whose *parts* were abstract — which projects fine — rather than a whole that was. Only an iterator whose element **is** the parameter could reach the gap, and `List::iter` is the first one std ever had.

## v0.30.0 — 2026-08-06

**Which method a `value.method()` call runs is now a rule, not an accident of where you typed the impl block.** Resolution scanned every impl of the receiver's type and took the first hit, in registration order — so an inherent method and a trait's could swap places by moving two blocks in one file, or by renaming a module so it sorted differently, with exit 0 both times and nothing said. The rule now: an **inherent** method — one declared by an impl of the type whose `with` clause does not declare that name — always beats a trait's, whatever the text order and whichever module loaded first. Otherwise the method a trait provides, whether the impl declares it or inherits the trait's default.

Folklore had this backwards where it mattered most. `iterator.vl` and `list.vl` are both std tier-0 modules tie-broken by module *name*, so `"iterator" < "list"`: a lazy adapter sharing a name with `List`'s eager `map`/`filter`/`fold` would have registered — and won — *before* `List`'s own inherent method. That is now impossible to reach by accident.

**Two declarations at the same level are an error instead of a silent pick.** Two *inherent* declarations of one name for one subject are rejected where they are written, before any call resolves them, with a note at the other declaration — Rust's E0592 shape. Two *traits* providing one name with no inherent method above them make each call ambiguous, and the message names both homes and both fixes with your own receiver already substituted: `'pick' is ambiguous on 'Bag': both 'A' and 'B' provide it; call 'A::pick(bag)' or 'B::pick(bag)' to pick one`. The same rule reaches the two places the identical bug was also hiding: two traits whose *defaults* share a name, and a `T: A + B` bound whose two arms both supply it (that one used to `break` on the first match with no check at all). Reaching one declaration twice — `Ord` and its supertrait `PartialEq` both offering `eq` — is one candidate, not an ambiguity.

**`Trait::method(receiver, args…)` is the disambiguator, and it needed no new syntax.** The grammar already parsed the qualified-path call; it just resolved through the same order-dependent scan spelled differently, and naming a *trait* at the head failed outright. It now means what it says: the named trait's version, whether the impl declares it or inherits the trait's default, and on a trait-bounded generic receiver as well as a concrete one. Its sibling `Type::method(receiver, args…)` is tightened to match — the type's own member or an error naming the trait that does have it — so there is no second, unreformed door into the ambiguity the rest of this closes.

**The one place this bit, it was already biting.** A sweep of std, every example, and the whole 111-program corpus found exactly one live collision: `vilan/test/gap-b.vl` redeclared `Option<(T, U)>::unzip`, which `std::option` already declares — dead code in a CI-gated file, undetected only because the two bodies happen to compute the same thing. It is now a compile error, and the file's copy is renamed so the gap it exists to pin stays pinned. Building every corpus program and every example with the old scan and the new resolver side by side found **no other site whose resolution changes** — the rule's entire blast radius is the bug it was built to catch. Twenty pins, each planted red and restored. Record: `proposal/method-resolution.md`.

---

**`(make(), 6).1` gives you the 6.** Building a tuple out of a value you did not first name lost that value's slots. `let pair = (4, 5); (pair, 6)` was fine; `(make(), 6)` — the same tuple, straight from the call — silently built the wrong thing, and every read past that element came back `undefined`. It compiled clean, it type-checked clean, and it has been wrong since tuples started storing flat: v0.28.0 and every release before it. No `..` was involved.

Tuples store **flat**, so an element that is itself a tuple has to splice its slots into the one being built rather than nest inside it — that is what makes `.1` resolve to the slot the type says it does. The compiler decided whether to splice by looking up the element's type in the cache the type solver fills, and that cache only holds a type where one is *produced*: a binding, a literal, a field or index read, a `match`. Everything else is typed on demand and the answer thrown away. So a **call**, an `if`, an `else if` chain, a `{ block }`, a method call, an associated call, a call through a closure value, an `await`, a `*view` read, a `const` element — and, least expected of all, a plain **parameter** — all read as untyped, all nested, and all broke the layout. Now the tuple's own type rule keeps the type it computes for each element, so coverage follows from the rule rather than from which forms happened to leave a trace. Sixteen pins, one per form plus the mixed, nested and spread-alongside cases, each planted red and restored.

An element whose type is still a generic parameter deliberately stays nested, and that is not a gap. A generic body is compiled once and emitted for every instantiation, and the flat offsets baked into that single pass count a generic element as one slot; splicing it at the instantiations that bind it to a tuple would move every offset past it. Pinned in both directions.

The spread form was never affected and still is not: `(..make(), 6)` has always been correct, because a `..` splices for having been *written* as one and asks no type question at all. Record: `proposal/variadic-generics.md` §T.8.

**A `fun` written inside another function is emitted once.** It came out twice — nested where you wrote it and again at module level, identical bodies, the inner one shadowing the outer. Nothing ran wrong, because the two were the same function, but the second copy was output no reader of the emitted JavaScript could account for and no change to which one a call resolves to would have stayed harmless. Emission is driven by demand — a call emits its callee — and the body walk was emitting the declaration a second time just for being written. It no longer does, which also means a nested `fun` nobody calls now emits nothing, like any other unreached function. The filed report said an `impl` method's body; it happened in a plain function's body too, since the double visit had nothing to do with `impl`.

**A tuple's arity is part of its type again.** `let t: (i32, str, bool) = (1, "x")` compiled clean, and so did passing a two-tuple where a three-tuple was declared, returning one, assigning one, putting both in a list literal, binding both to one generic, and writing an impl whose method returns a tuple of a different arity than the trait declares. The unifier compared tuples by walking the pair until the shorter one ran out — so it agreed on the common prefix and never noticed the rest — where the array unification beside it has always required the same length and the closure unification the same parameter count. It requires the same arity now, in both the unifier and the trait-conformance comparison that had the same hole. Nine pins, each planted red.

---

**`vilan bindgen leaflet.d.ts --platform browser` writes the bindings you would have written by hand.** A `.d.ts` describes a JavaScript library's surface precisely enough to generate the `external struct` + `[extern(…)]` `external fun` dialect `std` is written in, and now it does: interfaces, classes, functions, properties, methods, generics, `extends` chains, closures, and closed string sets all map. `getElementById` becomes `get_element_by_id` and the extern keeps the exact JS spelling; a `readonly` property gets a getter and no setter; a callback the host awaits is typed `async |T| U` because adaptation never crosses a host boundary, while a plain `(e: Event) => void` needs no annotation at all — the divergence rule already absorbs it. It is **not a build step**: nothing in `build`/`check`/`run` reaches it, and the `.vl` it writes is a file you review, edit, and commit, the way you own `vilan init`'s scaffold.

The parser is our own, and that is the one place this departs from the paper. `proposal/bindgen.md` §2 recommended `oxc`; running the gate the proposal itself said to run — *"whoever takes this item up should run `cargo about generate` against a real `Cargo.lock` with oxc added and read its own output"* — it fails. `dragonbox_ecma`, reached non-optionally through `oxc_syntax`, is `Apache-2.0 WITH LLVM-exception OR BSL-1.0`, and neither branch is on `about.toml`'s deliberately closed list. Two more costs came with it: **44** new crates against the ~12 the proposal predicted (a 32% larger lockfile), and — because the machinery lives in `vilan-core` — an entire JavaScript parser linked into the size-tuned wasm playground artifact. A `.d.ts` is a declaration-only grammar with no expressions, no statements, and no bodies, so it is written directly instead: **zero new dependencies, no license-policy change, `THIRD-PARTY-NOTICES.txt` untouched.**

**Nothing a `.d.ts` says is ever dropped silently.** Every construct that cannot be expressed becomes a `// TODO(bindgen)` naming it and saying why — overloads (the first signature wins, the rest are quoted verbatim), namespaces, `declare module`, conditional and mapped types, `keyof`, intersections, open unions, index signatures. A generated file with TODOs is reviewable; one with silent gaps is a landmine.

**Four rows of the ratified mapping table were wrong, and all four for one reason.** The owner flagged one as conditional — `{ [index: number]: T }` → `List<T>`, *"confirm whether `List` tolerates sparse keys before shipping that mapping"* — and running it found the rest. **A vilan aggregate has a vilan-owned runtime representation that a host does not speak.** A `struct` is a positional array (`struct P { x: f64 }` is `[x]`, `p.x` is `p[0]`), so a host object read through one yields `undefined`, silently. An `enum` is `[tag, …payload]`, so a TypeScript discriminated union — a tagged *object* — matches nothing and crashes. `Map<str, T>` is a struct over a hashed native map, not a plain object. And `List<T>` is a real JS array, which an array-*like* is not: `for`-in over `{0: "a", length: 1}` throws `TypeError: … is not iterable`, and a real array with holes hands `undefined` to a `T`-typed slot. So the three aggregate rows are diagnosed rather than mapped, and §3.8's "`external struct` always" recommendation stops being an ergonomic preference and becomes a correctness requirement. A tuple is the exception that proves it: `(A, B)` *is* a JS array, so `[A, B]` maps across exactly.

**The fourth is `Option`, and it is the one that would have hurt.** §3.2 maps every absence — `T | null`, `T | undefined`, `x?: T` — onto `Option<T>`. `Option` is a tagged array too: `Some(v)` is `[0, v]`, `None` is `[1]`. Reading, a host returning `"hello"` is tested as `value[0] === 0`, which is `"h" === 0`, so a **present value arrives as `None`**. Writing, `None` reaches the host as the array `[1]` — and for an optional `boolean` argument, `[1]` is truthy, so `arc(…)` quietly draws counterclockwise. `std` does use `Option` across `external` boundaries, but only ones it owns: compiler intrinsics and its own `__`-prefixed runtime helpers, which know the representation. A library's `.d.ts` does not. So nothing bindgen emits is ever an `Option`: a nullable type binds as the bare type with a `///` note, and an optional **parameter** becomes one binding per call arity — `getContext(id)` and `getContext(id, options)` are two real host calls, so they become two real bindings of the same symbol, which is exactly what `std` does by hand where `append` and `append_text` both bind `appendChild`.

**Whether a string-literal union should be a backed enum is a language question, not a bindgen one, and it stays open.** `type Align = "start" | "end"` generates a plain `enum` plus a private raw extern and a match-wrapper, because the host boundary still speaks the raw string. A **backed** enum — `enum Align { Start = "start" }` — would need no wrapper at all: a numeric enum already lowers to its bare discriminant, so a string-backed one would lower to its bare string. It does not parse; the discriminant grammar is `= (-)? integer`. That is recorded as its own question, with a test that goes red the day the language grows one.

**And the probe that was the point of doing this now.** Run against TypeScript's own `lib.dom.d.ts` — 39,429 lines, 2,415 declarations — bindgen emits **489,523 lines of vilan that type-check clean** through the real analyzer in eleven seconds. Members bind at **99.8%** (61,224 of 61,317, after `extends` flattening). Declarations bind at **65.8%**, and the entire shortfall is one construct: **824 `declare var` globals**, because every `[extern(…)]` form binds a *call* or a receiver's property and none reads a bare global as a value. 641 of those are the DOM's constructor idiom — `declare var HTMLCanvasElement: { new(): HTMLCanvasElement }` — so recognizing that one shape is the difference between an unusable and a usable binding. A working canvas program (fills, arcs, text, a click listener) compiles to exactly the JavaScript you would hand-write, on top of generated bindings plus **one** hand-written line for the entry point. The full findings are appended to `proposal/bindgen.md`; they are what the deferred canvas item (A17) was waiting on.

Sixty pins, each planted red and restored, including two that fix *language* facts rather than bindgen's own behavior — that a struct is a positional array, and that `Option` cannot cross a host boundary — so the day either changes, the mapping rows built on them go red instead of quietly rotting.

---

**`Iterator` is a trait you can actually implement.** It declared
`fun next(self): Option<T>` — by value — and a by-value receiver cannot advance
anything, so every stateful iterator (a cursor, a counter, anything holding a
position) hit the conformance error `match the receiver convention` and had to
give up on the trait. `Range`, the one real lazy iterator in the standard
library, was written that way: a bare inherent `next(&mut self)` and no `with
Iterator<i32>` clause, because the clause was not available to it. `next` now
takes `&mut self`, which is what advancing an iterator has always been, and
`Range` carries the clause. The documentation's claim that "`Range` is one such
type" is true for the first time.

Nothing about `for … in` changed: the loop resolves the protocol on the *method
name*, so a type with a `next(&mut self): Option<T>` has always driven a loop
whether or not it declared the trait, and still does. What the repair buys is
the trait as a **bound** — `fun total<I: Iterator<i32>>(mut source: I)` accepts a
`Range` now — and it is the enabling change for everything else in this arc,
since every adapter is stateful by construction. If you implemented `Iterator`
by mutating something outside the iterator (a module-level counter — the only
way that worked), the receiver is the one line to change.

**`List` has a cursor, and every iterator has adapters.** `xs.iter()` returns a
`ListIterator<T>`, and `map`, `filter`, `take`, `skip`, `enumerate`, `zip` and
`chain` arrive as trait *defaults* on `Iterator` — so implementing `next` gets
you all seven, on your own types as much as on std's. They are lazy: each one is
a small struct holding its upstream, nothing runs until something pulls, and a
chain makes **one** pass over the source with no intermediate lists.
`[1, 2, 3, 4, 5, 6].iter().filter(|n| n % 2 == 0).map(|n| n * 10).take(2)` walks
the six values once and touches four of them.

Laziness is what makes `take` more than shorthand: it never pulls past its
budget, so it bounds a source with no end. An iterator whose `next` always
answers `Some` is now a normal thing to write, and `.take(3)` terminates it.
`zip` stops with the shorter side; `enumerate` numbers what reaches *it*, so
after a `filter` you get positions in the output rather than in the source.

`xs.iter()` takes a **snapshot**. That is rule 1 rather than a policy — the
cursor stores the list in a slot that outlives the call, so the storage copies —
and it means a `push` after `iter()` is not walked, and that `iter()` itself
costs a copy of the list. The eager `List` methods that only need one pass still
take one.

The adapter *types* are past participles — `Mapped`, `Taken`, `Filtered` — while
the methods keep the plain names. `Map` is already a std type, and vilan's method
resolution picks by registration order rather than reporting a collision, so the
names are kept apart deliberately rather than arbitrated.

One rough edge is documented rather than hidden: if an iterator's element type is
its own generic parameter and you instantiate it at a *tuple*, the `for` binding
loses the tuple — `for pair in [(1, "a")].iter() { pair.0 }` is rejected with
"cannot access field '0' on type T". Pulling by hand works, iterating the `List`
works, and `enumerate`/`zip` are unaffected because they name their tuple element
structurally. It is a pre-existing defect in the loop's substitution that nothing
in std could reach until `List::iter` existed; it is pinned.

**A chain ends with a method that says what it builds.** `to_list`, `fold`,
`for_each`, `count`, `any`, `all` and `rev` consume the iterator and hand back an
ordinary value, and `to_list` is the primary one on purpose. A method that
*names* what it builds needs no type annotation, reads at the call site, and
works in the middle of an expression — `xs.iter().filter(f).to_list().len()` —
which is exactly the shape a pipeline invites and exactly where an
inference-driven `collect` gives up. **There is no `collect`, deliberately.** If
one is ever added it will sit beside this family, never replace it.

`any` and `all` short-circuit, so they answer over a source with no end;
`count`, `fold`, `for_each` and `to_list` pull everything, so bound such a source
with `take` first. `rev` is a **barrier** rather than a lazy adapter: it drains
its upstream into a `List`, reverses that, and hands back a `ListIterator`, so
the chain continues but the work has already happened. A lazy reverse wants a
double-ended protocol — every adapter deciding whether it can walk backwards —
which roughly doubles the surface of a layer that has not had its first user, and
is purely additive whenever a consumer needs it: `rev`'s signature would not
change, only its body.

For a `Set` or a `Map`, terminate and convert: `List` gains
`to_set(self): Set<T>` under `T: Hashable` and `to_map(self): Map<K, V>` over a
list of pairs, so a chain reads
`xs.iter().map(|w| (w, w.len())).to_list().to_map()`. Those two live on `List`
rather than on `Iterator`, and the reason is a real limit worth knowing: a trait
default may not require a bound its trait does not declare, and a method cannot
carry one of its own that ties back to the trait's parameter — so a `to_set`
written as a trait default is rejected at its own definition, before any call.
Putting a bounded method beside the bound it needs is the choice `join` already
makes with `Display`. That constraint is pinned as a compiler fact, so when
per-member bounds arrive the move onto the trait is additive and the record says
why it could not be there first.

**`List`'s own `map`/`filter`/`fold`/`for_each` are unchanged**, and the reason
is recorded rather than left implicit. The plan was to re-express them over the
adapters — `self.iter().map(fn).to_list()` — so that each name has exactly one
meaning. Built and measured, it turned out to remove something that works today:
an **async closure cannot adapt through an adapter chain**, because an adapter
stores the closure in a field and calls it from a trait-dispatched `next`, where
there is no single concrete callee to instantiate. `xs.map(async work)` stops
compiling, and the corpus program that exists to pin adaptation stops building.
It also cost about **5.5x** on a 20 000-element `map`→`filter`→`fold` — not from
the per-element calls, but from two O(n) deep copies the eager loop does not pay:
`iter()` snapshots the list, and the terminal copies the chain holding that
snapshot. So the eager four keep their bodies. Nothing is ambiguous as a result:
`List` does not implement `Iterator`, so the lazy `map` is reached only through
`.iter()` and the two are told apart by what they are called on.

**A method call is no longer colored async by a same-named *static*.** When the
compiler cannot pin which impl a dispatched `receiver.name()` will select, it
considers every member called `name` and takes the caller as async if any of them
is — sound, and deliberately over-approximate. Statics were in that set, and they
cannot be: a method call never selects a member with no receiver. It surfaced the
moment the standard library grew an `Iterator::all`, because `Promise::all` is an
`async` static in an always-loaded module, so `xs.iter().all(p)` colored its whole
caller async down to an `async` `main` — for a program with nothing async in it.
Compile-time evaluation then refused such a program outright, since macro and
`const` bodies are synchronous. The candidate scan now keeps only members that
take a receiver. Nothing else in the corpus moved a byte, and a genuinely async
dispatched member still colors its caller, which is pinned in both directions.

## v0.29.0 — 2026-08-04

**You can finally see an optimistic write happening.** `optimistic(signal, value, commit)` paints, awaits, and confirms or rolls back — and hands the outcome to whoever called it and to nobody else. So a button that should grey out while its write is in flight, or a banner that should say why one failed, needed a boolean you kept yourself, and a sweep of every app in the tree found not a single one. `Optimistic::over(signal)` wraps a signal you already have — no binding changes — and adds a `state` signal to bind: `Confirmed`, `Pending`, `Rejected(reason)`. `write` still returns the outcome; the state is an addition, not a replacement.

**It also fixes something you could not fix from outside.** Two writes in flight over one signal corrupted it. An older write that fails *after* a newer one succeeded rolled the newer value away — probe it through the free function and the screen ends up showing the value the cell started at while the server holds one from two writes later. The cell discards a superseded outcome (the newest write owns the cell, and the outcome still returns to its own caller), and a rollback lands on the last value the **server** confirmed rather than on whatever the signal happened to hold when the write began. Those are two different questions, so confirmations carry their own counter and an out-of-order reply cannot walk the recorded truth backwards.

There is deliberately **no re-send on reconnect**, which is the one place this cell and `Draft` part company. A re-send is at-least-once: safe for a draft's "set this field to this value", and not safe for a one-shot *action*, which is what this cell exists for. The rollback is the recovery and the user re-issues. A cell over a **mirrored** signal is out of scope for now — the mirror writes behind the cell's back — and the reason is written down rather than left to be discovered.

`optimistic` itself is untouched, byte for byte, and still pinned. Record: `proposal/optimistic-lifecycle.md`, which also carries two questions for the owner — whether the standard library should also grow the *paint-less* action-state cell the demand data actually points at, and whether the free function stays now that the cell exists.

**A `Draft` publishes its status and its text together.** Editing a draft writes two signals — `local` and `state` — and anything watching both could catch the pair mid-change: the new text still claiming `Synced`, then `Dirty` a moment later. Every shipped consumer went through a UI event handler, which already wraps a dispatch in a turn and coalesced them, so nothing tripped over it; a draft driven from a node program, from SSR, or from a test published the middle. Both `push` and `adopt` now publish their transition as one wave. Under a UI turn not a byte changes.

**Five small additions to `std::style`, and a headline that turned out not to need any.** `Length::zero()` writes a bare `0` — the spelling the `min-width: 0` flex idiom and the `inset: 0` fill-the-parent idiom are written in everywhere, where `space(0)` gives you `var(--space-0)` and `Length::px(0.0)` gives you `0px`. `Length::css("clamp(1100px, 100vw, 1920px)")` is the verbatim escape for a complete CSS value, `Color::hex`'s twin: reach for it for `clamp()`, `min()`, `max()`, `env()`, or for one named expression you want to reuse across several properties. `Length::calc` is unchanged and is still the one to write when you are writing *arithmetic* and want the wrapper supplied — it is now simply documented as the sugar it is, `Length::calc(e)` being `Length::css("calc(" + e + ")")`. `line_height_length(Length::px(24))` fills the hole where `line-height` was the one length-valued property whose typed method could not hold a unit; `line_height(1.5)` is untouched and remains the one to prefer, because a unitless number inherits as a *ratio* and re-computes against each descendant's own font size where a length does not. And `background_image(str)` with `background_size(str)` covers the background images a `Gradient` cannot hold — a `url()` or data URI, a multi-layer list, a positioned or `repeating-*` gradient. `background_image` writes the **same slot** `background_gradient` writes, deliberately, so the two override each other instead of racing at equal specificity. Every class name in every program is unchanged; the corpus stylesheet gains eight lines and changes none.

**The headline this slice was chartered around dissolved when it was measured, which is the more useful result.** The item was filed off a raw count — 36 `raw("background", ..)` calls surviving in the one real consumer — on the same day the previous slice's value types shipped, and was never checked against them. Reading the 36 sites one at a time: 20 write a plain hex or `rgba()` colour, which `background(Color::hex(..))` and `background(Color::rgba(..))` have held since that slice; 13 write a gradient, which `background_gradient(Gradient::radial(..))` and `Gradient::linear(..)` hold (the `to left` and `to right` keywords *are* 270deg and 90deg); and the remaining three are cuts the previous slice had already recorded and argued, two of which want live custom properties that no value type short of a CSS expression tree could carry. So the 36 sites are a conversion backlog, not a missing feature. Both shapes the charter proposed are declined on the same evidence: a composite `Background` value type models an arity **no site writes** (all 36 write exactly one value), and `background_position`, `background_repeat`, `background_attachment`, `background_origin` and `background_clip` have **zero sites each** across 2926 lines. What earned its place instead was the pair sitting beside them — `background-image` and `background-size`, which are always written together, so five sites and one unit rather than three sites and a fragment.

**A shorthand you write as two typed calls now provably resolves like the shorthand.** The four two-value `padding: 8px 20px` sites were the other candidate for a new method, and working the interaction through is what argued against one: `padding_y(v).padding_x(h)` writes all four `padding-*` longhands, which is exactly what the shorthand covers, so the composed form resolves the same way in every direction it can meet a `padding(..)` — after one, before one, over an edge, or across a `+`. It costs four atomic declarations where you wrote one, and buys no expressiveness, so the surface stays as it is and the guide says why.

Nine pins, each planted red and restored. Four of them exist to hold the measurement rather than the code: they assert that each shape the real sites write is expressible with what already shipped, and that converting a `raw("background", ..)` to a typed method — which moves the slot from the family shorthand to a longhand, and so stops the family reset — still resolves by authoring order, which is what makes that conversion safe to do incrementally. Record: `proposal/ui-styling.md` §0bis.5, which closes with the per-site table the conversion will read.

---

**`(..pair, 3)` — a tuple can be built out of another one.** A `..` entry **spreads** a tuple you already hold: its elements go into the tuple being built, so the type is the two **concatenated**. `let pair = (1, 2)` makes `(..pair, 3)` a `(i32, i32, i32)` — not the `((i32, i32), i32)` that writing `(pair, 3)` gives you. Spreads work in every position, in any number, mixed freely with ordinary entries — `(0, ..pair)`, `(0, ..pair, 9)`, `(..pair, ..pair)` — because concatenation does not care where its parts sit. A construction whose *only* entry is a spread is still a construction: `(..pair)` is the concatenation of one, and `(e)` is a parenthesized group exactly as before.

It concatenates **one level**. `..outer` contributes `outer`'s elements, so an element that is itself a tuple arrives as a tuple — the type system's nesting is a real boundary and a spread does not dissolve it, only the outermost layer. Spreading something that is not a tuple is an error that names the type.

**`f(..pair)` closes the circle the last release left open.** Variadics shipped a spread *parameter* — `f(a, b)` meaning `f((a, b))` — and refused the caller's `f(..existing)`, because under that same desugar it is `f((..existing))` and the tuple-value spread did not exist. It does now, and the call site needed nothing new: the arguments were already being collected into a tuple, so a spread argument simply lands inside it. `need2(..pair)` passes a pack of two, `need2(..pair, 7)` a pack of three, and the arity bound is checked on what the spread actually contributes — a `T: (3..)` rejects `need2(..pair)` with the bound's own error, naming the concatenation. Passing a spread to a function with **no** spread parameter builds no tuple and is refused with the steer that does work: write `f((..pair))`.

**This is what forwarding a pack has been missing.** From inside `fun outer<T: (..)>(...items: T)`, calling another spread function as `inner(items)` collects the pack into a *pack of one*; `inner(..items)` passes it on as itself. That works even though `T` has no known arity yet, because the concatenation of one is identity — there is nothing to concatenate it with, so nothing needs to be known. Mixing an abstract pack with anything else (`inner(..items, 9)`) is refused, and says why: the body is checked once, before any call fixes the arity, so there is no element sequence to join. A concrete tuple has no such restriction.

**`..` needed no disambiguation, which was worth checking rather than assuming.** Vilan has no range operator in any position — `0..3` is not an expression today — so the spelling was unclaimed. What did need a ruling is that a leading dot silently absorbs dots: `(1..3, x)` has always parsed, quietly, as a member chain over `1`. So `..` is a spread **only where an entry begins**, recognized before the entry's expression is parsed. `(1..3, x)` never reaches it and behaves exactly as it did, which is pinned. The cost is priced: an infix or postfix range stays available to a future operator; a *prefix* one is now spoken for inside a tuple and an argument list. Writing `...` where `..` belongs gets its own steer rather than a confusing failure — the two markers are one dot apart and one is the sibling feature.

Emission is the tuple form's, unchanged: tuples store flat, so `(..inner, 12)` and `(inner, 12)` both emit `[ ...inner, 12 ]` and differ only in type. **The work found one bug on the way, and it is in the tuple form, not the spread.** A tuple-typed element whose expression is a call or an `if` loses its splice, so the construction silently nests and every read past it is `undefined` — `(make(), 6).1` compiles clean and evaluates to nothing, on v0.28.0 and every release since flat lowering, no `..` required. Fixing it properly means typing every expression into the cache the splice test reads, which is its own change; it is recorded and pinned `#[ignore]`d. The spread is **not** affected, because a `..` splices for having been written as one rather than by a type lookup — which is the right source of truth anyway.

20 pins in the compiler-behaviour suite (19 live, 1 the `#[ignore]`d bug above), plus two parser unit tests, a formatter round-trip over nine spellings, six parse-differential fixtures and a corpus fixture with its golden and its node run. Every mechanism planted red and restored: the parser production takes 21 red, the type rule 11, the placement sweep 4, the mark-driven splice 2, and the disambiguation pin goes red on a parser that stops absorbing `..` — the plant for a pin whose whole claim is what the ruling leaves alone. The previous release's 32 spread-parameter pins are untouched; the one that pinned this as deferred is now the one that pins it working. The type-level spread `(..T, U)` is still future work. Record: `proposal/variadic-generics.md` §T.

**`preset = "release"` no longer miscompiles your program.** A release build renames every binding to the shortest identifier free in its scope, and that renaming collided: the emitted JavaScript declared one identifier twice. Seven of the corpus programs came out broken, in three shapes. Two module-level functions could end up sharing a name, and since the second declaration wins, the survivor called *itself* — a program that built cleanly and then died with `Maximum call stack size exceeded`. A binding could shadow the one its own initializer reads, giving `Cannot access 'x' before initialization`. Most often it was a plain `Identifier 'b' has already been declared`, which at least fails loudly. **This has been true of every release build the compiler has ever produced**, back through v0.27.0 and before, and nothing about it needed `const` inference — the sweep that shipped in v0.28.0 could shift which programs tripped it, but the folding was never the cause. Debug builds were never affected, and neither was the browser playground, which compiles with the debug options.

The cause was two name pools that were secretly one. Code generation gives every binding a unique name first, and a later pass re-allocates those names over the JavaScript scope tree so that disjoint scopes can share short ones. That pass was only told about the bindings the compiler had named after something in your source — every anonymous name it had minted for itself (each instance of a generic, a `match`'s subject and result temporaries, a loop's cursor) was invisible to it, and drawn from the identical `a, b, c, …` alphabet the pass hands out. So it reissued names that were already in use. Readable output escaped only because its temporaries are `$`-prefixed and no source-derived name contains a `$` — the two pools cannot meet there, which is exactly the property release lacked and now has: every generated name is either re-allocated against its scope or reserved in every scope.

**`print(3 - -2)` compiles again under `release`.** Tight output drops the padding around operators, and subtracting a negative printed as `3--(2)` — which JavaScript reads as one postfix `--` and refuses to parse, taking the whole file with it. Dropping padding is only sound while it leaves the token stream alone, which the printer now checks: a junction whose two characters would fuse keeps one space, and everything else stays as tight as before.

**The release path is gated now, for the first time.** Every corpus program is compiled under both presets and run under both, with identical output required. It is deliberately not a byte golden — release output differs from debug in nearly every byte by design, so bytes would pin the minifier's current letters instead of its correctness — and running it is the only instrument that works, since a renaming collision emits perfectly valid JavaScript. The gate found the `- -` bug on its first run. Until now the corpus built debug only, which is why none of this was ever noticed.

## v0.28.0 — 2026-08-04

**The phase-timing switch no longer kills warm analyses.** `VILAN_PHASE_TIMING=1` panicked on every base-cache hit (a refreshed start instant subtracted against a kept cold duration), and because the marks run inside the analysis fence, the panic surfaced as every analysis after a process's first silently producing no program. The instrument now reports honest per-phase numbers on cold and warm paths alike.

**Your own container can now lift a whole expression, not just a member chain.** `?.` has worked on any type that opts in with `impl .. with Lift` since the operator shipped; the bare `?` that lifts the surrounding expression — `price? + tax?`, `count? * 2` — only ever worked on `Option` and `Result`, and a user container at a bare `?` got a "recorded follow-up" error steering back to `?.`. It lifts now, through the container's own methods: one `?` calls its `map`, several nest its `and_then`s and end in a `map`, and a body that itself yields the container flattens instead of nesting — the same rules `Option` follows, and the same rules a `?.` chain already followed. The marker stays the only gate: a type with a `map` but no `Lift` is still refused, because having a `map` is not consent. Nothing about `Option` and `Result` changed — they keep their inline lowering and the corpus is byte-identical apart from the new examples.

Two things are worth knowing when you write the impl. `Lift` declares **no members** — it is consent, not a contract you fill in — so what the operators actually call is a pair of ordinary methods you supply, `map<U>(self, |T| U)` and `and_then<U>(self, |T| Self-of-U)`, with the element taken as the container's first type argument; a missing one is named in the error. And because the lift becomes *your* `and_then` calls rather than a tag check the compiler emits, short-circuiting and laziness in a multi-`?` expression are whatever your `and_then` does with the closure it is handed. Effects still order as written, including work hoisted between two `?`s. Record: `proposal/try-and-lift.md` §11.

The same pass wrote down what is left of that backlog item and why it is not code: `!` inside a closure waits on the `arg → Result` linkage design (three coupled decisions, none settled anywhere), and `Signal`/`Promise` opting into `Lift` waits on what `?` should *mean* for each — a signal's `?` reading versus subscribing is a real fork, not an implementation detail. Both are recorded as design questions in `proposal/try-and-lift.md` §12.

---

**An async callback can no longer be stashed in a field that says it is synchronous.** Passing an async closure where a plain one is expected *adapts* — the function is compiled a second time, awaiting the callback — and that has always been the point. But a function that **stored** its callback rather than calling it handed the async value on with its asyncness stripped: `fun install(f: || i32): Holder { Holder { hook = f } }` compiled, and a later `(holder.hook)()` returned a promise from a field typed `i32`. The type is what everything downstream trusts, so `let n = (holder.hook)(); print(n + 1)` printed `[object Promise]1` — integer addition silently became string concatenation, from a clean compile. The rule that a plain closure field refuses an async value was already written and already enforced; it just could not *see* a closure that was async only at one call site, because it ran without knowing which call site it was looking at. It runs per instantiation now, and refuses the store at the call that made the callback async, pointing at the field it would have escaped into. The fix it names is the one that works: declare the field `async || T`.

The same hole had two more faces. Returning the callback through a plain declared return (`fun pass(f: || i32): || i32 { f }`) leaked identically and is refused the same way. And `compose(f, g)` with an async `f` — the example the design notes had already ruled an error — turned out to compile and print a promise; it is now the error it was documented to be. **Void positions are untouched** and keep spawn semantics, a `sync` instance of the same function stores exactly as before, and passing a callback *onward* to another function still adapts through the whole chain — that was never an escape. Nothing in the standard library, the corpus, the examples or the docs relied on any of it. Record: `proposal/async-polymorphism.md` A.4b.

**`drop(open_database())` now closes the database.** Handing a value straight to the `drop` sink — a call's result, rather than something you had bound to a name first — destroyed nothing at all. `let handle = open(); drop(handle)` ran the destructor; `drop(open())` compiled clean, ran the call, and quietly leaked what it produced. The sink takes its argument by move, so an unbound value is owned by the `drop` expression and there is nothing else that could ever destroy it: no scope end to reach it, no binding to overwrite. The rewrite simply could not see a type on an expression that was not a name or a construction, and a sink with no type falls back to the no-op it uses for plain data — so a resource and an `i32` looked identical to it. Every value expression is typed now: a plain call, a method call, a nested call. `drop` on data stays the no-op consume that still evaluates its argument for effects, and every already-working form — a local, a parameter, a match capture, a construction — emits the same bytes it did before.

The same hole had a second face inside generics. `fun consume<T>(own x: T) { drop(x) }` is refused at a resource instantiation, because a body compiled once for every type at once has no destructor to run; routing the same `T` through a call first (`drop(identity(x))`) slipped past that check untyped and leaked instead. It is now refused for the same reason, with the same message. And the class is closed rather than the case: a `drop` whose argument type cannot be resolved is a hard compile error naming the call, instead of an emission that tears nothing down. Record: `proposal/affine-moves.md` §9.4.

**`log(1, "hi", true)` — a function can take its arguments out flat.** Variadics shipped as a *tuple*: `combine((items, filter))`, with the parentheses you had to remember and the extra pair you had to type. Prefix the last parameter with `...` and callers write the elements instead — `fun log<T: (..: Display)>(...items: T)` is called `log(1, "hi", true)`. **It is not a second mechanism**, and that is the whole design: a spread parameter *is* a tuple parameter, and `...` says only that the call site writes the pack flat. `fun f(...items: T) { … }` means `fun f(items: T) { … }`, and `f(a, b)` means `f((a, b))` — so the body, the type, the storage and the emitted JavaScript are the ones the tuple form always had, and everything the variadic machinery already does keeps working because it is working on the same parameter it always was. `combine` written this way is `gather(count, name)`, with `T` still recovered from the mapped signature `(U in T: Signal<U>)`.

**The arity rules are the tuple bound's, not new ones.** `T: (2..)` refuses a one-argument call with the bound's own error and its "the bound is declared here" note; `T: (..10)` refuses an eleventh argument; `T: (..: Display)` requires every argument to be printable, per element. `T: (..)` accepts a call with *no* arguments — the empty pack — which along with the one-argument pack is a tuple arity nothing in the language could previously produce, since `()` and `(x)` are not tuple *values* in source. Both work and both are pinned down to their emitted bytes. Fewer arguments than there are fixed parameters says "at least", because a variadic signature has no exact count to name.

**What `...` will not do, each with a reason rather than a syntax error.** It never combines with `own`, `&`, or `&mut`: the argument is a tuple the *call site* builds out of the collected arguments, so there is nothing to transfer and nothing to alias. It is refused on a **closure**, because a closure type (`sync |A, B| C`) has no variadic form — such a closure could not be annotated, stored, or passed anywhere, only called at its literal. It is refused on a **trait method and on any `impl` member**, because unlike `mut` it *is* part of the signature, and a method is reached by dispatch on three routes that would all have to agree; the steer is the tuple form. And it is refused on an `external fun`, whose calling convention belongs to the host. `mut ...items: T` is fine and means what `mut` always means. Two consequences are worth knowing: a spread function used as a **value** has its tuple type and is called with a tuple, and a tuple *written* at a spread call site is collected like any argument — into a one-element pack, which then fails its own bound.

**Spreading a tuple *into* a spread parameter is not here yet.** `f(..existing)` would desugar to `f((..existing))`, and that tuple-value spread — designed in the variadic proposal, never built — has to land first. Forwarding a pack today goes through the tuple form, which needs no new syntax.

32 pins in the compiler-behaviour suite, plus a parser unit test, three formatter round-trips, an LSP hover pin, three parse-differential fixtures and a corpus fixture with its golden — every mechanism planted red and restored. Two pre-existing limits the pack inherits are pinned beside their tuple-form twins rather than left to look like spread bugs: a pack that is still an abstract `T` cannot be indexed positionally, and a comprehension still needs a mapped source. Record: `proposal/variadic-generics.md` §S.

**Release builds fold the arithmetic you never marked `const`.** `let total = 1 + 2 * 3;` now ships as `7` — no keyword, no annotation — and so does a call whose arguments the compiler can work out, a chain through such a binding, and a small list built by a loop. The `const` keyword has not changed and is still worth writing: it *promises* the fold and reports an error when it cannot deliver one. Inference promises nothing. Anything it cannot settle — a free variable that is a parameter or a `mut`, a host call, a panic, a result that is not plain data, a budget — it silently leaves alone to run at runtime exactly as written, with **no diagnostic anywhere**. That asymmetry is the whole safety story: a release build can never reject a program a debug build accepts, and folding can never be the reason a build fails.

**Debug does not infer, and that is deliberate.** Folded computation vanishes from stack traces, so the readable preset keeps it; `release` folds. It is an ordinary `[build]` knob (`infer-const`), so either preset can be overridden. Nothing about a manifest-less `vilan build file.vl` changes — the default is still `debug`, which is why every corpus golden in the tree is byte-identical.

**A fold has to be invisible, not just correct**, and one rule here is stronger than the explicit form's. An explicit `const` discards what its evaluation printed — you asked for that computation to move to compile time, printing included. An inferred fold that printed would silently delete a line from a working program's output the day someone switched preset, so any evaluation that writes output or exits is refused and the binding left alone. The same closure applies to the asset channel: an inferred attempt runs with `asset::emit` unavailable, so inference folds values and never creates a compile-time context — whether a style compiles cannot depend on the optimizer. Bindings inside generic function bodies are not folded at all: their values depend on a monomorphization the const environment does not carry.

**Every fold is checked by running it.** Each corpus program is compiled twice — with the sweep and without — and every program whose output changed is executed both ways under node with identical results required. That gate is what caught the generic case above, on its first run. The pass is build-only: the language server never runs it, since silent fallback leaves nothing for an editor to surface. Record: `proposal/const-eval.md` §9.

**Known, unrelated, and not fixed here**: the `release` preset's short-name renaming emits colliding identifiers on seven corpus programs — two module-level `function b` that shadow into infinite recursion, a `const` shadowing the `for` binding it initializes from, and five plain redeclarations. All seven reproduce on v0.27.0 with no inference involved; folding can shift which programs trip it but does not cause it. Filed for a codegen arc of its own.

## v0.27.0 — 2026-08-04

**A task of a task is just a task.** `async { some_task }` typed as `Task<Task<T>>`, so reaching the value took two `await`s according to the type — but only ever one at runtime, because a task is a host thenable and a promise resolved with a promise *adopts* it rather than boxing it. The type sat one level deeper than the value it described, which is the divergence plain `Promise` always had, inherited when `Task` was typed. It does not any more: spawning a computation that produces a task yields `Task<T>`, one `await` reaches the value, and `let value: i32 = await outer` is what compiles. A chain collapses the same way, however deep — `async { async { async { 7 } } }` is a `Task<i32>` — because each spawn assimilates as it is built rather than trimming a layer at the end. **The sharp case is a generic that happens to land on a task**: `fun wrap<T>(value: T): Task<T>` called with a `Task<i32>` would have instantiated to `Task<Task<i32>>`, a type nothing can hold, so instantiation assimilates too — `wrap(task)` is a `Task<i32>`, and `Task::settle_all` over such tasks yields the values rather than a list of handles. The same wrapper at a non-task argument is untouched, and no other nesting type is affected: `Option<Option<i32>>` and `List<List<i32>>` mean exactly what they say. **What changes for existing code**: an annotation that spelled the old, deeper type is now an error naming the value type instead — nothing emitted a byte differently, and the whole corpus is byte-identical. One residual is recorded rather than fixed: an `async fun` whose *declared return* is itself a `Task` still types one level deep, because its calls are implicitly awaited and async-ness is a whole-program fixpoint computed after the types are. Record: `proposal/async-polymorphism.md` Part B.

**A dropped connection stops eating what someone was typing.** A `Draft` edited while the socket is down has always kept the user's text — that is the whole point of it, and why it doesn't roll back the way `optimistic` does — but nothing ever sent that text once the connection came back. It sat in the input looking saved until the user happened to type another character. `draft.repush()` re-sends it, and one line wires it to the transport: `client.transport.on_reconnect(|| title.repush())`. It sends **only if the remote is actually behind** (`local != synced`), so a screen full of untouched drafts costs nothing on reconnect, and it covers both an edit whose commit never left and one caught in flight by the drop. The same call is what a "retry" button in a failure banner should do.

Two things are stated in the open rather than buried. **Delivery is at-least-once**: a commit the server applied but could not acknowledge before the socket died is indistinguishable, at the client, from one that never arrived — so it gets sent twice. `Draft`'s own reconcile absorbs the duplicate (an echo is already a no-op, and the generation counter discards the superseded commit's outcome), which makes this a non-issue for the shape drafts exist for — "set this field to this value" — and a real one for a commit that *appends*. And **a failed re-push is not retried on a timer**: it rides the next reconnect, because a draft the server is permanently refusing must not spin.

`SocketTransport.on_reconnect(hook)` is the new surface underneath, and it is deliberately not a second connection signal. Hooks run after each successful re-dial, awaited in order, *after* the generated client's mirror re-attach — which is the one thing `connection_state` cannot tell you, since it flips to `Connected` a beat earlier by design (the re-attach's own rpc call needs a usable transport first). Bind the signal for a banner; use the hook when you need the mirrors to be current. Wiring is opt-in, and not merely out of caution: a `Draft` holds an opaque commit closure and has no reference to any transport, so it cannot subscribe to a reconnect it has no way to name. No existing app changes a byte on the wire until it writes the line. Record: `proposal/draft-reconnect.md`.

**`draft.debounce(300)` sends one commit per burst instead of one per keystroke.** Binding an input to a draft has always been per-keystroke *safe* — a slow older commit landing late is discarded rather than clobbering a newer one — without being per-keystroke *cheap*: every character was a frame on the wire. A debounced draft coalesces them, committing the value you ended on once the typing stops. **It does not slow the typing down**: `local` and the `Dirty` state still land synchronously inside `push`, so the input is exactly as immediate as before and only the commit waits. Trailing edge, and `commit()` — a blur handler, a Save button — cancels a pending window and sends now, so an explicit save costs one commit rather than yours plus the window's; a re-push does the same, since recovery is not typing. The window rides a real `std::time::Timer` rather than a counter, so cancelling it actually clears the host timeout — which matters, because a pending timer keeps node alive. `0` is the default and is byte-for-byte today's behaviour.

The end-to-end leg is a real one: a real socket, a server killed mid-session and restarted on a *different* value, an edit made while down, and the re-push that carries it back — including the ordering that makes it correct, where the mirror resyncs to the restarted server's value first, declines to clobber the dirty local, and is then knowingly overwritten by the re-push (`Draft`'s documented last-write-wins rule). Eight runtime pins cover the semantics, all four plants restored.

---

**The editor stopped evaluating your `const` expressions twice per keystroke.** Analysis already folds every `const` and keeps the results on the analyzed program; the language server was throwing that away and running the whole pass again, purely to have values to put in a hover. It reads the results analysis already produced. On a const-using file that is 12–17 ms off every debounced re-analysis — measured, not estimated — and nothing about what you see changes. Record: `proposal/const-eval.md` §8.3.

**A failed `const` now tells you which function failed.** "const evaluation failed: index out of bounds" pointed at the `const` expression and stopped there — which, when the expression is `const build_table()` and the subscript is three calls down, told you the build broke without telling you where. The diagnostic now names the function the failure happened *in*, and attaches a note at that function's declaration carrying the call chain that reached it (`level_one → level_two → level_three`), so the editor jumps straight to it. Running out of budget reads as budget rather than as breakage: an unbounded loop or runaway recursion inside a `const` says it "did not finish within the compile-time budget" and names the cap it hit, instead of borrowing the macro engine's internal wording. The squiggle itself stays on the `const` expression, and the appendix now says why — the tree being evaluated is compiled output and carries no source positions, so there is no inner expression to point at. Record: `proposal/const-eval.md` §8.2.

**A compile-time-only function can no longer escape into runtime code as a value.** Building a style — or calling anything else that reaches `std::asset::emit` — has always been compile-time-only, and calling one from runtime code has always been a clear compile error naming the crossing. Passing one *without* calling it was not: `apply(styled)`, `let f = styled;`, or a closure literal with an `emit` in its body all compiled clean, and the emitted JavaScript carried a live call to a helper that exists only inside the compiler. The program died on load with `ReferenceError: __emit_asset is not defined` — a compile-time rule failing at run time, which is the one outcome the rule exists to prevent. The compiler cannot follow a call made through a value (there is no statically known callee), so it now refuses the *value*, at the reference or the closure literal, and says why: a compile-time-only function has no runtime value form. Inside a `const` nothing changes and nothing is restricted — the interpreter makes the call itself, so `const apply(styled)` is legal, passes its function through as many hands as you like, and still emits its CSS. Record: `proposal/const-eval.md` §8.1.


**A shorthand and its longhands now resolve in the order you wrote them.** `style().padding(space(4)).padding_top(space(0))` is `1rem` on three edges and `0` on the top; `padding_top(space(0)).padding(space(4))` is `1rem` all round, because the later whole-box value replaces the edge outright. Until now which of the two won was decided by where their generated class names happened to fall in the stylesheet's sort — a coin flip, and one that had already landed wrong in real styling: a `margin(space(0))` was silently overriding the `margin_left(Length::auto())` written after it, so the flex-push idiom did nothing. Last-wins now covers a whole **family** — a property together with the properties it covers — and the families are `padding`, `margin`, `inset` (over `top`/`right`/`bottom`/`left`), `border` (over its parts and edges), `background`, and `flex`. It holds per condition, so a `dark` or `hover` variant of one family never disturbs the base; it holds across `+`, so `card + style().border_color(..)` recolours a border rather than racing it; and it holds for `raw`, since a family is a fact about the CSS property, not about which method wrote it.

**Class names are unchanged, but the stylesheet is not byte-identical.** A shorthand rule now renders `*.sX{..}` instead of `.sX{..}`, which is the same selector matching the same elements at the same specificity — the marker exists so the rule sorts ahead of its family's longhands. Every class keeps its name (across the corpus and the vilan-lang.org bundles, 913 classes: none renamed, no declaration changed), so nothing that references a class by name breaks; but a build that diffs its emitted CSS will see those lines move and gain a `*`. Record: `proposal/ui-styling.md` §0bis.4.

---

**Returning a different resource from each branch no longer loses one of them.** `fun pick(flag, own first, own second) { if flag { first } else { second } }` compiled, handed back `first`, and destroyed nothing at all — `second` was never torn down on the path that returned `first`, and vice versa. Each arm looked fully accounted for on its own, and the rule that a value must be moved on *every* path was not being read across the arms of a branch. It is now: producing a different binding from each arm is a conditional move, and reports as one — the same error, with the same wording, that you already get for `if flag { consume(handle) }`. Returning the *same* binding from every arm is unaffected and always was fine. There is one exemption, and it is what keeps `Option::or_else` and the `if slot is Some(_) { slot } else { make() }` idiom legal: a value that provably carries nothing on a path needs no move on it, because a `slot` reaching the `else` of `slot is Some(_)` is `None` and has no payload to destroy. The exemption is granted only on proof — for an enum whose every variant carries something, the divergent move is still an error. This was never a generics-only bug and the fix is not generics-only: the same shape at a concrete resource type leaked identically and is now caught too. Record: `proposal/affine-moves.md` §9.3.

**A generic that would have to destroy your resource now says so, instead of leaking it.** The rule that a generic body cannot destroy a `T` — it is compiled once, for every type at once, so it has no destructor to run — was only ever asked of `own` parameters. Everything else a generic body can end up holding went unasked: a `match` capture that took the payload, a `let` local it was moved into. `fun peek<T>(own o: Option<T>)` whose match consumed `o` passed the check and destroyed nothing at `T := Res`. The question is now asked of every value, and the answer is the same one it always was: move it out on every path, or take a concrete type. **This found real leaks in the standard library.** `Option::map`, `and_then`, `filter`, `is_some_and`, `map_or` and their neighbours hand the payload to your closure — and a closure only *borrows* what it is given, so nothing ever takes ownership and the payload dies unclosed. `Some(db).map(|d| d.handle)` compiled and silently leaked the database; it is now a compile error at the call, naming the value and pointing into the combinator. This is the same rule that already refused `or`, `xor` and `unwrap_or`, applied where it had been missed. **Only resource instantiations are affected** — `Option<i32>.map(..)` and every other data use is untouched, which is every use in std, the corpus and the examples. For a resource, `match option { Some(let handle) => .. }` at a concrete type is the spelling that works, and it destroys the payload correctly. Record: `proposal/affine-moves.md` §9.2.

**A resource you borrowed out of a pattern can no longer be given away.** `if option is Some(let handle) { consume(handle) }` compiled, and destroyed the handle twice. Testing with `is` — and matching `&option` — *inspects* without consuming: the value being matched keeps ownership and still tears the payload down at the end of its own scope. The capture is a view into it, not a second copy, so handing that capture to something taking `own` gave one payload two owners, and both destroyed it: the `drop` body ran twice, the file handle closed twice. The rule that a body may only give away what it owns already covered parameters; it now covers captures too, in all three loaning forms (`is`, `match &x`, `let (a, b) = &x`). **Reading a loaned capture is untouched** and stays the point of the idiom — test, read the payload, pass it on by loan; only *giving it away* is refused. The steer is the spelling that works: consume the subject instead (`match option` without the `&`), which makes the capture the owner and lets it be moved on freely, or reach for `Option` + `take`. Plain data is entirely unaffected. Record: `proposal/affine-moves.md` §9.1.

**Route chunks got their loading story.** A split browser leg (`split = true`) now starts the boot route's chunk downloading *before* it builds the page shell, so first paint waits on the network rather than on your own JavaScript. Navigating away from a page whose chunk is still in flight is safe: the **latest navigation wins**, whatever order the fetches finish in, so a slow chunk can no longer land on top of the page you moved to. And a failed fetch is finally something you can see — `std::router::chunk_error()` is a `Signal<Option<str>>` beside `pending()`, holding the reason the last chunk did not arrive. **This fixes a real bug**: a failed fetch used to leave `router::pending()` stuck true forever, so every spinner in the app stayed on. There is no retry API because a link already is one — the failed attempt is not remembered, so clicking through again refetches. Record: `proposal/bundle-splitting.md` §9.

**`vilan build` tells you when splitting cost you.** Splitting is not free — the route gate, the forwarders and the chunk map are a fixed cost of about 6 KB per split leg — so a leg whose pages are small ships *more* on first load than it would whole. The build now emits your entry both ways and compares: when the eager bundle came out no smaller, it warns with your leg's own numbers ("adds 1720 bytes to the first load and defers only 6802"). `vilan build --print-chunks` prints the same verdict without opting in, so a leg can be measured before it is split. Measured, not estimated: there is no threshold in the compiler to go stale.

**A leg's chunk files belong to its last build.** Renaming a route arm no longer leaves the old arm's `dist/<leg>.<Route>.js` beside the new one, dropping `split` takes `dist/<leg>.chunks.json` with it, and a `vilan run` clears whatever a previous `vilan build` left — so `dist/` never describes a build that is no longer there, and a server iterating the manifest can never be handed one that lies.

**`vilan run` emits whole bundles, watched or not — and now says so.** `split` is a `vilan build` optimization; the dev loop hot-swaps whole bundles, so `run` passes over the key and prints one line about it. This is also a fix: `vilan run` and `run --watch --no-hmr` used to honour `split` while an HMR-active watch round did not, so the same project built two different ways depending on a flag about hot reloading. Single-file emission remains the default and is first-class forever.

**A hand-written server can serve chunks without knowing a single route name.** `dist/<leg>.chunks.json` lists every artifact the build wrote; `examples/fullstack`'s server now reads it at boot and serves each file it names, so adding, renaming or removing a route arm needs no server change — and a leg that does not split writes no manifest, so the same server works either way. A static host still needs nothing: serve `dist/`. Record: `proposal/bundle-splitting.md` §10.

## v0.26.0 — 2026-08-04

**`std::style` grows the values people were escaping to `raw` for.** The 2026-08-04 sweep found two tails, and this closes the second: about 120 sites where the property already had a typed method but its *value type* could not hold what was wanted. Colours gain alpha two ways — `Color::rgba(27, 6, 13, 0.9)` for a literal, and `some_color.alpha(0.08)` for this colour at that alpha, which is the one to use on a ramp step because it keeps the token underneath (`var(--gray-900)` survives into the declaration, so a translucent themed colour still re-themes). Gradients arrive as their own value type rather than as a colour, because in CSS they are not one: `Gradient::linear(135.0)` or `Gradient::radial(RadialExtent::ClosestSide)`, `.stop(colour, percent)` per stop, painted by `background_gradient` onto the `background-image` slot — a *different* slot from `background`, so a style can set a colour and paint a gradient over it. `border_none()` removes a border, and does it by filling the same slot the `border` shorthand does, so it is an ordinary last-wins override rather than a second rule racing the first. The four `border_*` edges, the eight `padding_*`/`margin_*` edges, and `Display::InlineFlex`/`InlineGrid` fill in families that had holes. Channels, alphas and the two-stop gradient minimum are checked during const evaluation, so a bad value stops the build naming itself. **Class names are unchanged** — the corpus stylesheet golden gained thirteen lines and altered none.

What deliberately did *not* ship, and why: there is no multi-value `padding("8px 16px")` method, because every real two-value site is `padding_y(v).padding_x(h)` already; no `BorderStyle` enum, because the sweep found zero non-`solid` borders — alpha was what actually defeated `border`; and `box_shadow`/`transition` keep their `str` values, the latter with no `raw` sites at all. Five `raw` sites in the `todo` and `walkthrough` examples were converted to typed methods, and both examples' stylesheets came out byte-identical. Record: `proposal/ui-styling.md` §0bis.3.

---

**A bound on a trait's own type parameter reaches its default bodies.** `trait Holder<T: Bound>` has always been accepted, and the bound has always been checked at every impl — but the trait's own default methods could not actually use it: calling one of `Bound`'s methods on a `T`-typed value failed, so the bound was an obligation you paid for and never got to spend. A bound is two-way everywhere else in the language — an obligation at a call, an assumption inside the body that declares it — and it now is here too. A default body may call the bound's methods on its parameter, and the call reaches the implementation of whatever each impl supplies (`impl DogBox with Holder<Dog>`), specialized per implementing type. Multi-bounds (`T: A + B`) reach the members of both, calls inside a closure in the default work, and a generic impl (`impl Bag<type E: Bound> with Holder<E>`) grounds through its own binder. An *unbounded* trait parameter still refuses member access with the same error it always gave, and an impl that overrides a default still wins — this adds reach, it does not add inference. This is what stood between the iterator proposal's headline direction (adapters on `Iterable`, so `xs.take(3)` needs no `.iter()`) and the associated types Vilan does not have. Record: `proposal/iterator-adapters.md` (P4).


**A resource you take out of a pattern is destroyed.** `match option { Some(let handle) => .. }` is the sanctioned way to reach a resource payload — the previous release made it the *only* way, by rejecting `if (option.is_some()) { option.unwrap() }` as a conditional move — and the payload it handed you was never torn down. Matching by value consumes the value being matched, so its own end-of-scope teardown is correctly suppressed; the capture then became the payload's only owner, and nothing destroyed it. Every such payload leaked: no `drop` body ran, no file handle closed, no background task was cancelled. A capture that takes ownership of a resource now drops at the end of the leg it was bound in, exactly like a `let` in that leg — after the leg's own locals, in reverse order when a leg captures more than one, and on every way out including `ret`, `jump`, and a panic. The same closes for `let (handle, count) = pair`, which had the same hole. Moving the capture onward is unchanged and still costs nothing: hand it to `drop`, return it, pass it by `own`, or store it in a struct, and the destination owns it — exactly one destruction, wherever it ends up. Matching a *loan* (`match &option`, or `if option is Some(let handle)`) is unaffected: nothing is consumed, so the value being matched stays the owner and destroys the payload itself. Plain data is completely unaffected — a data pattern emits exactly the code it emitted before. Record: `proposal/affine-moves.md` §7.

---

**A browser leg can ship its routes separately, and you don't annotate a thing.** Add `split = true` beside a browser entry's `target` and `vilan build` stops writing one file: it writes an eager `dist/<leg>.js`, one `dist/<leg>.<Route>.js` per arm of your route `match`, and a `dist/<leg>.chunks.json` listing them. There is no `lazy()` to wrap a page in and no keyword to forget — the router `match` already marks the seams, so the compiler infers the split from whole-program reachability: a function only one arm can reach rides that arm's file, and anything two arms share stays eager, as does every module-level binding (so initialization order is exactly what it was). **What you see while a chunk loads is the page you were already on** — the route signal doesn't advance until the code arrives, so there is no blank frame and no placeholder tree to design; bind `router::pending()` for a spinner over it. First visit to a route pays one fetch, every later visit is instant, and a route you never visit is never downloaded. A failed fetch leaves the navigation undone and says so in the console. `vilan build --print-chunks` reports what *would* split without emitting anything, which is how to find out whether a leg has enough per-route code to bother. **Single-file emission is unchanged and stays the default forever**: without the flag every byte of every bundle is what it was, and `--watch` builds ignore `split` outright, since HMR swaps whole bundles. `split` on a Node leg is a manifest error rather than a line that quietly does nothing. Record: `proposal/bundle-splitting.md`.

## v0.25.0 — 2026-08-04

**`View.style_var` no longer keeps writing after its view is gone.** It was the one reactive `View` method built on a raw subscription parked in a local instead of the ambient `effect` every `bind_*` uses, so the subscription was never handed to the enclosing boundary: swap a view out, write the signal, and the detached element still got its custom property set — a leak that grew with every route change. It registers with the boundary now and dies with it. The server-rendering twin reads the signal once and was always correct.

**A closure that returns something it captured now hands back a copy.** `mut xs = [1, 2]; let get = || xs;` gave out `xs`'s live storage, so pushing to `get()`'s result grew `xs` — the same aliasing `fun identity(c) { c }` had before v0.25.0 fixed it, written inline instead. A closure over a parameter had it worse: `fun make(own items: List<i32>): || List<i32> { || items }` handed out the *same* list on every call. The rule that was already there — a body copies what it hands back unless it owns the storage — was reading "owns" off the convention, and a closure's frame is not the frame that owns a capture. It reads the frame now. A closure returning its own local is unchanged and still free, and nothing in the standard library or the corpus emits a byte differently.

**`Option`'s remaining combinators work on a resource — or say exactly why they cannot.** After the consuming-call change in v0.25.0, nine combinators refused an `Option<SomeResource>` outright. Six of them work now. `is_some_and`, `ok_or` and `unzip` were plain `own self` conversions. `inspect` and `or_else` are rewritten over `is` tests, which *loan* rather than consume, so `opt.inspect(|v| ..)` reads the payload and hands the option straight back. `==` between two options never actually needed a fix — it was on the refused list by mistake — and its rewrite drops the temporary pair it used to build for every comparison. The three that still refuse do so for one reason, now stated in one sentence each: `or`, `xor` and `unwrap_or` all have a path that *discards* a resource value they were handed, and a generic body has no way to destroy one. Each names the value it cannot handle — `or` names its alternative, `unwrap_or` its fallback — where before the first thing you were told was about the receiver, with a suggested fix that fixed nothing. The spellings that produce an alternative instead of taking one in, `or_else` and `unwrap_or_else`, work. **Plain data is untouched**: every one of these copies for a non-resource and behaves exactly as before, verified line by line.



**A resource cannot hide in a container behind a generic.** `Shared<Database>` was refused — the native containers' internals are host code the move checker cannot see — but `Signal<Database>` compiled clean, even though a `Signal`'s storage *is* a `Shared`. The rule was being read off the type you wrote rather than the one you got: `Signal`'s `Shared<T>` field holds nothing at its declaration, and only becomes a `Shared<Database>` at the point of use. It is now read per use, descending a generic type's fields as they stand at that use, so the rule covers any generic of your own with a `List`, `Map`, `Set`, `Shared`, `Task`, `Promise`, or `Context` field — not just `Signal`. The error names the route the resource took to get there (``Shared` cannot hold the resource `Database`, reached through `Signal.value``) and points at the field it landed in. `Signal<i32>`, `Signal<List<str>>`, and holding a resource in a struct field of your own are all unaffected.

**`sync` on a callback that returns nothing now means what it says.** A parameter declared `sync || void` accepted a closure that awaits, while the identical parameter returning `i32` refused it — so a `sync` marker on a void callback was a correct declaration that did not bite. Both checks were asking the wrong question: whether the callback returns a value decides whether an async closure can *adapt*, not whether a declared contract applies. The marker is the whole test now. The one place this changes in the standard library is `Signal::update`, whose `mutate` callback holds a writable view of the stored value: a view may not be live across an `await`, so an awaiting `update` body is refused rather than silently producing a view that outlives its guarantee.

**A `match` guard may need a temporary, and now gets one.** A guard containing an `is` test, a `?` lift, or a nested `match` compiles to statements as well as an expression — and an `else if` chain has no room for a statement before a leg's condition, so those statements were built and thrown away. The emitted condition then read a variable nothing had declared, and a program that type-checked cleanly died at startup with `ReferenceError`. A match with such a guard is now emitted as a sequence of tests instead of a chain, each leg holding a slot for what its guard needs; the copies a guarded leg's captures owe are made there too, ahead of the guard, so `Some(mut xs) if xs.pop() is Some(_)` pops from the capture rather than from the value being matched. A guard that needs nothing keeps the chain, byte for byte.

**A consuming call is a move.** `option.unwrap()` on an `Option<SomeResource>` hands you the payload — but the compiler recorded no move, so `option` stayed readable afterwards *and* was still torn down at scope end: one resource value destroyed twice. The cause was a mis-declared signature rather than a missing analysis. A bare `self` receiver is a **loan** in vilan (which is what lets `db.exec(..)` not consume `db`), and `unwrap` was declaring one while moving the payload out of it. Two things change. A body may now only consume what it **owns**: moving a loaned resource parameter out — returning it, passing it on by `own`, matching it by value — is an error naming the fix (`declare it \`own self\``), which closes the whole class rather than the one instance. And `Option`'s combinators that hand the payload onward (`unwrap`, `map`, `and_then`, `filter`, `flatten`, `zip`, `ok_or_else`, `unwrap_or_else`, `transpose`, …) now declare `own self`, so the call is a move: a later use of the source is a use-after-move error, and the source is not destroyed a second time. Everything downstream follows the rules that already governed `own` arguments, unchanged — a consuming call in one branch is a conditional move (R7), in a loop is a repeated move (R8), on a field is a partial move (R5), and re-initializing a `mut` binding afterwards is fine. **Plain data is completely unaffected**: `own` copies for a non-resource, so `o.is_some()` after `o.unwrap()` on an `Option<i32>` stays legal and stays correct. The one idiom that changes on a resource is `if (opt.is_some()) { opt.unwrap() }`, now rejected as a conditional move — reach the payload with `match opt { Some(let value) => .. }`, which consumes on every path. Record: `proposal/affine-moves.md`.

**A list built from another list no longer shares its elements.** `xs.map(f)`, `xs.filter(p)`, `xs.sort_by(c)` and `xs.reverse()` copied the spine but handed back the *same* element values, so writing through `xs.map(f)[0]` showed up in `xs[0]`. All four are independent now. The rule underneath is one line — a value stored in a slot that outlives the expression is copied, which is what value semantics has always said — and the standard library now declares it where it means it: `List::push` takes `own item`, `sort_by` takes `own self`. Passing something freshly built still costs nothing; only a value that has another owner is copied.

**Building a list, tuple, struct, or variant from an existing value copies it.** `[xs]`, `(xs, 1)`, `Holder { items = xs }`, and `Some(xs)` all filed the original's storage into the new value, so growing it through the new value grew the old one. Each of those positions is an initialization, and initializations copy. Constructing from a value that dies on the spot still moves rather than copies, so building a value up and handing it off is as cheap as it was.

**Returning something you were given hands back a copy.** `fun first(c: List<i32>): List<i32> { c }` returned the *caller's* storage, so the caller's own list grew when the result did — the same leak that made `map` share elements, since `|c| c` is that function written inline. A returned value reached through a by-value parameter now copies. A function's own local still moves out for free, as does an `own` parameter — which is how a fluent builder (`fun with(own self, …): Self`) stays copy-free — and a `&`/`&mut` projection is still a view by design.

**A `Shared<T>` is no longer copied when it is stored.** `Shared` is a cell, and sharing it is the point; the compiler nevertheless emitted a copy that copied nothing. Programs that build values out of `Shared` fields — every UI view — lose a pointless call per construction.

**`std::style` grows the properties people were escaping to `raw` for.** A sweep of the real styling written in vilan found 341 `raw(..)` calls against about 350 typed property calls — the escape hatch was carrying half the work. The methods the sweep ranked highest now exist: `top`, `right`, `bottom`, `left` and `inset`; `font_family`, `letter_spacing`, `text_decoration`, `white_space`, `user_select`, `transform`, `box_shadow`; `flex`, `flex_shrink`, `grid_template_columns`; `border_color` (its own slot, so a `hover` can recolour a border without restating its width); and `min_width`/`max_height`, which complete a quartet whose other halves already shipped. `Length` gains `em`, `vh`, `vw`, and `calc("100% - 2rem")` — you write the arithmetic, not the wrapper. 28 property methods become 46. This is deliberately the demanded head and not a CSS mirror: `clip_path`, `animation`, `z_index` and the rest of the thin tail stay with `raw` until they earn a place.

**The scaffolds link the stylesheet they emit.** `vilan build` has always written a `.css` sidecar beside the bundle for whatever your `const` styles compiled to, and nothing linked it: `vilan init`'s browser and fullstack templates shipped a page with an inline `<style>` block and no `<link>`, so adding `std::style` to a fresh project produced a stylesheet the page never loaded. Both templates now carry the link — and a small `const` style, so the sidecar exists from the first build — and the fullstack template also serves it (`/client.css`, guarded so an app with no styles still boots). The `reactive-ui` example, which had been emitting `app.css` and dropping it on the floor since the styling system landed, links it too. The examples gate now refuses any example that emits a stylesheet no page loads.

**Dark mode stacks with hover.** `style().dark(style().hover(..))` now compiles, emitting one rule under both conditions (`:root[data-theme="dark"] .sX:hover`), and a breakpoint can wrap the pair: `md(dark(hover(..)))`. Conditions nest outside-in in the order the selector nests them — media, then dark, then the pseudo-class — and writing them in any other order stops the build with a message naming the order it wanted, rather than the flat "dark cannot wrap an already pseudo-conditioned style" refusal that used to reject both directions. Class names are unchanged: the composition rides a grammar in the existing slot key, so every rule minted before this release keeps the exact name it had. Two long-standing nesting guards (a pseudo-class wrapping a pseudo-class, a breakpoint wrapping a breakpoint) gained their first tests on the way.

**A style can live in a signal.** `view.bind_styled(signal)` is the reactive twin of `styled`, exactly as `bind_class` is `class`'s: point it at a `Signal<Style>` and the element's classes follow the signal. Both styles are still built in `const`, so every rule is in the stylesheet before the page loads and the signal only chooses between class strings that already exist — the construct-in-const rule holds with a signal in the middle. Server-side it reads once, like every other `bind_*` on the SSR layer. Previously the only way to swap a whole style reactively was `bind_class(signal.map(|s| s.class_list()))`.

**A generic wrapper over an iterator no longer compiles to an empty function.** Writing the adapter shape — a struct holding an upstream behind a bound (`upstream: U` where `U: Iter<T>`) and pulling from it with `self.upstream.next()` — produced a program that compiled cleanly, reported nothing, and threw `TypeError` on the first element. The upstream's `next` was emitted with an **empty body**: reaching the adapter through a `for` loop, or constructing it from a trait default whose return type mentions `Self` inside a type argument (`Taken<Self, T>`), lost the binding for `U`, and the call fell back to the trait's signature-only `next` — which has no body to emit. Both paths now carry the binding, so the adapter dispatches to the concrete upstream. The compiler also refuses, loudly, to emit a body-less function as a call target at all: whatever fails to resolve, it can no longer leave as silently wrong JavaScript.

**`for v in self` inside a generic drives the iterator protocol.** A `for` loop whose subject was a trait-bounded generic (`it: I` where `I: Iter<T>`), or `self` inside a trait default, skipped the protocol entirely and emitted a native `for…of` over the receiver's **field array** — so a three-element source yielded the struct's two fields, with no diagnostic. Such a loop now calls `next()` and re-dispatches to the concrete type at each instantiation, exactly as a method call on the same value does. A `for` over a generic whose bounds provide no iterator is now a compile error naming the missing bound, rather than a native loop that throws at runtime.

---

## v0.24.0 — 2026-08-03

**Mutate a signal's collection in place.** `signal.update(|&mut list| { list.push(item); })` hands the closure a writable view of the *stored* value, so growing a `Signal<List<T>>` no longer means the copy-transform-return dance `set_with` required (`|mut list| { list.push(x); list }` — a whole-list copy per push, written as a transformation when you meant a mutation). Subscribers are notified once, after the closure returns, whatever it did; inside a `batch` that notification defers and coalesces like any other write. It is one method for every container — `List`, `Map`, `Set`, a struct's fields, anything a closure can mutate through a view — rather than a per-collection twin. `set_with` is unchanged and remains the right form when you are computing a new value rather than editing one.

**Closure parameters take the full parameter grammar.** `|&mut list|`, `|&view|`, and `|list: &mut List<i32>|` now mean in a closure literal exactly what they mean on a function: a closure can receive a view and mutate the caller's value. Previously every closure parameter was by-value regardless of what was written or declared, so a `&mut` callback could not be expressed at all — which is what `update` needed. The combinations that would mislead are still refused (`mut` with a convention), now in both positions.

**A `Shared<i32>`'s write view survives being passed.** `Shared::write()` over a scalar produced the slot's *value* rather than a view of it, so it worked as an assignment target (`cell.write() = x`) and over aggregates, but handing it to anything expecting `&mut i32` gave the callee a bare number and crashed at runtime with no diagnostic. It now lowers to a proper view in every position; `cell.write() = x` is byte-for-byte unchanged.

**`List` grows the methods everyone was hand-rolling.** `join` (reinvented at six separate sites, three of them inside the standard library itself), `find` (the predicate search four call sites wrote out longhand, now short-circuiting), `contains` and `index_of` for value search, `reverse`, `sort` and `sort_by`, and positional `insert`/`remove`. `sort`/`sort_by` are **stable** — elements the comparator calls equal keep their input order — and, like `map`/`filter`, every one of the pure methods returns a new list and leaves the receiver alone. `insert`/`remove` panic on an out-of-range index in exactly the words `list[i]` already uses: a bad index is a caller bug, and `get` remains the total, `Option`-returning way to ask. `f64` and `f32` also gain `clamp`, which the integers have always had from `Ord` and the floats — deliberately not `Ord`, because NaN — had not.

**"`i32` has no method `to_string`" now says what to import.** The standard library loads lazily, so an `impl` only registers once something pulls its file in — which made `42.to_string()` fail with a flat "no method" even though `std::display::Display` implements it. That error now names the fix: ``i32 has no method 'to_string'; import std::display::Display to use it (`import std::display::Display;`)``. It works for any method a std trait provides on the type you called it on, and stays silent when the method genuinely does not exist. `List.join` is the one new method with a bound that strands it outside the always-loaded core, so it gets the same steer.

**`is` tests and guarded `match` legs copy their captures too.** v0.23.6 made destructure and `match` captures true copies, but only along one of the two paths the compiler uses: a capture in an `if x is (let a, let b)` test, or in a `match` leg that carries a guard, still shared storage with the subject. Growing the source showed through the capture, and a `mut` capture wrote back into it. Both now copy, on the same rules — and the copy for a guarded leg is made when the leg is entered, so a guard that rejects leaves the subject exactly as it found it.

**A resource is never copied out of a generic.** `Option::unwrap` and its neighbours are written once and compiled per type they are used at. The capture inside them copied unconditionally, which for a resource meant two of them: independent state, and a destructor run for each. Resources now move out of a generic capture, as the memory rules always said they must, while every other type — numbers, lists, structs — keeps its copy. The same rule reaches through generic containers (`Wrap<T>`), not just a bare `T`.

**Two copies that cancelled out.** Sharing a read-only capture is free, and moving out of a value that dies immediately is free — but a value that was shared has nothing of its own to give away, and the two together handed a `mut` binding the original's storage: `let (xs, n) = pair; mut ys = xs; ys.push(9)` grew `pair.0`. Separately, a capture returned through a braced arm (`Some(let inner) => { inner }`) or a conditional tail (`if first { a } else { b }`) escaped the check that catches a returned capture, and leaked the alias out of the function. Both now copy.

**`mut [a, b]` means the same thing everywhere.** `mut` on an array binder marked its elements mutable in a `let`, but not in a `match` leg or an `is` test — where writing through them reported `cannot mutate immutable 'a'`. One keyword, one meaning: both forms now bind mutably.

---

## v0.23.6 — 2026-08-03

**Destructure and `match` captures are now true copies.** Binding a piece of a value — `let (xs, n) = pair`, `Some(let inner) => …` — used to share the underlying storage with the source: growing `pair.0` showed through `xs`, mutating a `mut` capture wrote back into the source, and a returned capture (`option.unwrap()`) handed the caller a live alias into the option's payload. All three now copy, per the value-semantics rule every other binding already followed. Two elisions keep the cost where it belongs: a read-only capture from an immutable source still shares (recursive walkers like SSR rendering stay linear), and destructuring a temporary that dies on the spot moves instead of copying.

**`mut` parameters.** `fun f(mut x: i32)`, `|mut list| { … }`, and `mut self` now parse and work: `mut` makes the parameter a scratch copy the body can rebind and mutate — field writes included — with nothing visible to the caller, exactly as if the body opened with `mut x = x`. It works in every parameter position, stays out of trait signatures, and refuses the combinations that would mislead (`mut own`, `mut` with a view, `mut` on an `external fun`, `mut` on a resource). The old error for assigning through a parameter steered everyone to `&mut`, which changes the caller contract; it now offers both spellings: `mut x` to mutate your copy, `&mut x` to mutate the caller's value.

---

## v0.23.5 — 2026-08-03

**See what bundle splitting would save, today.** `vilan build --print-chunks` reports the route-chunk plan for each entry: which `View.swap` route matches are splittable, which functions every path needs eagerly, and which pages (with their exclusive helpers, wherever they live — entry or module) would load lazily per route, with estimated sizes. Analysis only — the emitted JavaScript is unchanged. The instrument for the bundle-splitting arc (`proposal/bundle-splitting.md`): emission lands when a real app's report shows meaningful per-route mass.

---

## v0.23.4 — 2026-08-03

**Highlighting no longer goes blank below a typo.** When a stray token or an unterminated string at the top level breaks the parse mid-edit, everything below the break used to lose its colors until the text was whole again. The editor now keeps the previous highlighting for the part of the file that hasn't changed — byte-for-byte the same text, just shifted — and drops it the moment a fresh analysis reaches that region, so edited lines and re-analyzed code always show current information.

## v0.23.3 — 2026-08-03

**Programs with derives join the analysis cache.** The once-per-process standard-library analysis previously stood aside for any file containing a derive — which is most real UI programs, since reactive list rendering requires `[derive(PartialEq)]`. Derive expansion now runs over the cached world per analysis (the derive vocabulary itself rides the cache), so editor keystrokes and playground runs on derive-using files get the same skip everyone else got in v0.23. Files that define their own macros still build fresh.

## v0.23.2 — 2026-08-03

**A module path no longer reaches names the module never declared.** `math::helper()` compiled whenever the ENTRY file declared a top-level `helper` — any of your own globals resolved through any standard-library module path, in member position and in `import std::math::helper;` alike. The member lookup walked the module's scope chain out to the global scope, where your top-level items live; it now consults the module's own declarations and re-exports only (the rule `use` paths always followed), and a genuinely missing member reports exactly as before.

**The playground and the editor's std-editing flows join the analysis cache.** v0.23.0's once-per-process standard-library analysis skipped any session with an open buffer under std — which included the browser playground entirely, since it serves the standard library from in-memory buffers. The cache now revalidates buffered files by content exactly like disk files: a second playground Run with the same imports skips the std re-analysis, and editing the standard library itself in the editor evicts and rebuilds against the buffer instead of standing aside.

## v0.23.1 — 2026-08-03

**A static inherited default is no longer fenced by a stranger's name.** The owner fence's last blanket conservatism: a method call resolving to an inherited trait default was covered by *member name*, unioned across every trait in the program — so `5.verdict()`, whose inherited default reads nothing, was rejected whenever any unrelated impl anywhere spelled a subscribing method the same way. Such a call is now covered by its receiver: only the members the receiver's type actually selects demand a boundary. A `self` call inside a shared default body — whose receiver genuinely varies by impl — keeps the conservative treatment, as does everything the receiver's own type selects: a needy default you actually inherit still fences.

**The v0.23.0 playground crash is fixed.** The phase-timing instrument marked wall-clock times unconditionally, and `Instant::now()` aborts on the browser's wasm target — so the v0.23.0 compiler crashed on its first playground compile. The deploy pipeline's smoke gate caught it before anything went live (the playground kept serving v0.22.0); the marks now no-op on wasm and report zeros.

## v0.23.0 — 2026-08-03

**The compiler can show where an analysis spends its time.** Set `VILAN_PHASE_TIMING=1` and every analysis prints one stderr line splitting its wall clock between module loading, constraint solving, and the whole-program checks — the companion to `VILAN_LEAK_REPORT`'s what-was-retained line, and the instrument for the analysis-reuse work: on today's compiler, ~84% of a small program's compile is the standard library being re-solved and re-checked, which is the cost that arc exists to remove.

**The compiler stops re-analyzing the standard library on every compile.** The resolved standard-library world — loading, walking, and constraint-solving ~21 always-reachable modules, most of a small file's compile time — is now built once per process and reused: keyed by platform and the file's `std::` imports, revalidated by file CONTENT on every reuse (an edited std evicts instantly), and cloned per analysis so nothing ever leaks between files. Editor sessions benefit most: the language server's per-keystroke analysis hits the cache whenever the imports are unchanged. Entries that entangle the world — package siblings, services, macro or derive definitions, workspace dependencies — simply build fresh, exactly as before.

**A latent inference stall is fixed at its root.** The constraint solver's fixpoint could declare itself finished one round early: an attempt that typed a closure's parameters and then correctly deferred — waiting on exactly the types it had just written — made progress neither of the loop's two exit signals could see, and whether the solver granted the extra round it needed depended on unrelated constraint traffic. In practice the standard library's own constraints usually supplied that traffic, which is why the stall hid; the analysis-reuse work's two-phase probe removed the traffic and exposed it on an immediate-chained generic call (`items.map(|p| p.name).map(|s| s.len())`). The fixpoint now counts type writes as a third progress signal and only concludes when a full retry resolves nothing, wakes nothing, and writes nothing.

**The whole-program checks stop re-checking the standard library.** Thirteen definition-site check passes — mutability, views, must-use, trait conformance, and friends — now skip entities defined in std modules loaded from disk: their diagnostics depend only on std's own content, which a new permanent gate pins clean (every std module force-loaded and checked in full, per platform, plus the whole corpus compiled both ways and required to agree byte-for-byte on diagnostics, warnings, and emitted JS). Anything user-side keeps full checking — an open std buffer included — and every use-site, instantiation-driven, and data-producing pass still runs whole-program. A small step in wall clock (~6% off a small file's compile); the machinery and the gate are the foundation the frozen-std work builds on.

## v0.22.1 — 2026-08-02

**Two holes in the owner fence are closed.** Both were found by the requirement-polymorphism design recon and proven with red probes. First: a trait-bound method called on a generic value *inside a closure* contributed no coverage edges at all — a `Signal` slot placed through such a call compiled with no boundary anywhere and registered against an undefined owner at runtime (a v0.21.1 regression; v0.20.0's blanket conservatism fenced it). Second, and much older: one covered caller laundered any number of uncovered top-level calls to the same function — `covered(); needy();` at top level compiled whenever `covered` provided a boundary somewhere else, and the top-level path read an undefined value. An uncovered entry now fences regardless of what other callers provide.

**The owner fence follows instantiation chains.** v0.21.1 made the fence instantiation-aware one call deep; it now resolves the whole chain. A generic forwarding helper — `fun card<T: Slot>(content: T): View { view("div").child(content) }` — no longer demands a boundary for static content: the compiler chases each call site's recorded bindings through any number of forwarding levels (self- and mutual recursion included, resolved exactly), fences the calls that instantiate a subscribing arm, and leaves the rest free. Each call site is judged by its own instantiation, so one uncovered static call and one covered `Signal` call through the same helper both compile. Unresolvable chains — a helper taken as a value, or itself reached through dispatch — keep the conservative union, and concrete-receiver (`OnType`) dispatch is unchanged.

## v0.22.0 — 2026-08-02

**The editor sends keystrokes, not files.** Text sync is incremental: the client ships each edit as a ranged splice instead of re-sending the whole buffer per keystroke, and the server applies them in order — full-replacement events still work, and manifests fold the same contract. The recorded edit shapes also buy precision: the inlay-hint viewport filter now maps each hint's anchor through the edits since the last analysis, so a line inserted above the viewport no longer drops the hints near its edge for the beat until the refresh lands.

**Semantic highlighting refreshes ship as deltas.** Every token refresh re-encoded and re-sent the whole file's tokens; the server now answers `semanticTokens/full` with a `result_id`, implements the delta request against what it last sent — one minimal edit, or zero when nothing moved — and serves `semanticTokens/range` for viewport-sized asks. An unknown baseline (a restart, an answer the client never saw) re-synchronizes with a full stream, and a closed document drops its baseline.

## v0.21.1 — 2026-08-02

**`mount()` plus a static child works again.** v0.21.0's widened owner fence treated a trait-dispatched call as needing every implementation's contexts — so `view("div").child(view("span"))` demanded an owner boundary because the *Signal* arm of `child` subscribes, even though the resolved `View` arm reads nothing. Context coverage now follows the recorded instantiation: a call that binds the slot to a `View`, a `str`, or a `List<View>` needs no boundary, while a `Signal<str>` slot keeps the fence exactly as before, and a generic forwarding wrapper (`fun wrap<T: Slot>(…)`) conservatively keeps it too. The playground's styles example — the deploy casualty that surfaced this — compiles in its original attach-only form again. The value threading itself is unchanged; only the fence got precise.

**Typing in one file no longer re-analyzes every other open file.** On each typing pause the language server swept every other open document through a full analysis, serially, whether or not it could possibly care — with a handful of files open, most of the per-keystroke cost was other files' re-analyses. The sweep is now gated on the real dependency edge: a document re-analyzes only when its last analysis actually loaded the changed file (its imports, transitively, plus std). The conservative arms stay conservative — a document that failed to analyze has no recorded set and is swept as before.

## v0.21.0 — 2026-08-01

**The language server learns markup.** Tags — opening and closing — paint as their own semantic token, attribute and event names as properties, and the desugar's scaffolding no longer bleeds through: the `<div`-as-function token and the child-position method tokens are gone, so a hole's contents paint as themselves, deterministically. Editing a tag name renames its pair — the server implements linked editing ranges over a raw parse, the same cheap per-request pass keyword hover uses. (The closing tag's span now rides the AST for this; the parser had been dropping it after the match check.)

**The book and the compiler both teach element syntax.** The UI guide gains an element-syntax chapter with both forms side by side, the spec's grammar and lexical pages carry the productions and the span-adjacency rules, and JSX arrivals get three phrasebook rows in the tour. Two diagnostics land with the docs: an unresolved `view` at an element carries the import steer as a note (element syntax lowers to `std::ui::view`), and `<div text("hi")>` — the one str-typed method name the type system cannot catch undotted — warns toward the `.text(…)` content method while a hand-written `.attr("text", …)` stays silent. Editor highlighting learns markup in both grammars, VS Code's TextMate and the book's highlight.js.

**`vilan fmt` gives markup its canonical layout.** An element prints inline — `<h2>"Todos"</h2>` — when it has at most one non-element child and fits its line; otherwise children take one line each, one level in, with `</tag>` back at the element's indent. A head too wide for the tag line breaks one item per line, `>` or `/>` returning to the element's indent, the shape signatures already use. Self-closing tags space before the slash: `<div />`, never `<div/>`. Two shapes the formatter deliberately leaves alone, because the safety net compares tokens and these differ by tokens, not layout: `<div></div>` never becomes `<div />`, and a braced hole keeps its braces (`{label}` is structural, not inferred). A comment written inside markup attaches to the head item or child it precedes and keeps the element split — markup is the sixth construct the comment-attachment rule covers; only a comment after the last child, with nothing to attach to, relocates below the statement.

**Element syntax lands: markup as sugar over the view chain.** An element expression is an ordinary expression: `<section class("todos")> <h2>"Todos"</h2> <input placeholder("…") .bind_value(draft) /> </section>`. The head is the element's construction — undotted `name(value)` is an attribute (`.attr`), a bare name is a boolean attribute, a leading dot is the builder chain verbatim (`.bind_each(…)`, `.show(…)`), and `on:click(handler)` dispatches `.on`/`.on_event` on a literal handler's arity. Children — nested elements, quoted strings (i-strings included), and `{expression}` holes — lower to `.child`, so the value's type decides what lands, exactly as in the chain. Text children are quoted: the lexer is context-free and stays that way. The lowering runs before analysis and builds the very trees the chain parses to — byte-identical emitted JS, pinned — so diagnostics, hover, and both platform twins work unchanged, and elements compose like any expression (`<div/>.show(flag)` chains; markup in a match arm swaps). Canonical `vilan fmt` layout and the book's pages ship alongside (above).

**A generic method call no longer silently no-ops on a closure-typed argument.** A trait-bound generic method called on an unannotated closure parameter (`.bind_each(items, |t| t.id, |t| view("li").child(t))` — the row's `t`) resolved before the parameter's type landed, recorded no dispatch, and monomorphized to the trait's empty abstract member: the row rendered, the text vanished, and a type with no impl at all compiled cleanly. The method path now defers and retries like the free-function path, dispatch records the real impl, the bound audit rejects no-impl types, and a never-silent sweep turns any remaining unbound-bounded-generic call into a diagnostic instead of a misrender.

**Mixed content lands in `std::ui`.** `child` now takes anything that can fill a child position: a `View` appends as an element, a `str` as a real text node, a `Signal<str>` as a text node kept in sync (read once on the server), and a `List<View>` appends each view. Prose around an inline element is a run of siblings now, not a pile of wrapper spans. `attr` is typed the same way — a `str` value sets once, a `Signal<str>` tracks — so `attr("href", signal)` and `bind_attr("href", signal)` are the same binding, chosen by type or by name. Behind both stand two ordinary traits, `Slot` and `AttrValue`, on both platform twins, and `std::dom` gains `create_text_node` and the `Text` handle. One edge sharpened with the widening: on the browser twin, `child` and `attr` now sit behind the same `owner_scope` fence as the `bind_*` methods — a trait-dispatched call carries the union of its impls' context needs, and the signal arms subscribe. Building browser UI outside every boundary was already the documented compile error; it now fires on these two methods as well (the server twin is unchanged — its arms read once and need no boundary). This is the groundwork slice of the ratified element syntax (`proposal/element-syntax.md`, backlog H8): the sugar's child and attribute positions will lower onto exactly these two methods.

## v0.20.0 — 2026-08-01

**The playground compiler can check the server leg.** `vilan-wasm` exports `compile_for(source, platform)`: "node" analyzes the program as a process-leg build — platform coloring, twin resolution and all — so the playground's server mode can typecheck HTTP services in the browser without pretending to run them. The page feature-detects the export; older wasm builds hide the mode toggle.

**Semantic highlighting no longer paints a derive's generated code over your file.** `[derive(…)]` — and `[service]`/`[rpc]`, which expand the same way — produce items the compiler walks right after the file's own, and their spans are offsets into a generated template that no file holds. Those items were already filed away from the user's file as *entities*; their *references* were not, so every type name in an expansion arrived claiming to belong to the file being edited, at an offset into text that file does not have. The editor drew each one wherever its offset happened to land: through leading comments, mid-word, across punctuation. Worse, an editor drops overlapping tokens, so a wide bogus span took the real tokens behind it down with it — the highlighting did not just gain noise, it lost names. `examples/rpc/src/main.vl` reported 496 tokens and lit up its entire header comment. Hover and go-to-definition read the same records and were wrong the same way. A generated walk is now set up in one place that files both halves under the generated source, so a fourth caller cannot get half of it.

**A generic type application highlights its head and its arguments.** `Signal<List<str>>` recorded its reference at the whole application — head, arguments and closing `>` together — so it arrived as one token, and since overlapping tokens are dropped, `List` and `str` went dark behind it. The reference is now the head name alone, which is also what hover and go-to-definition on it should answer; the arguments keep the references they were already recording, and become visible for the first time. Types written with a closure argument (`Shared<|str, str| Outcome>`) reached furthest and gain the most.

**An imported `context` clause no longer highlights over your file.** A `context` clause (`|| void context owner_scope`) resolves after the import fixpoint, because it may name an imported binding — and by then the file being walked is no longer the file that wrote the clause. Its name spans were recorded against the ambient file anyway, so importing `std::reactive`, whose own source is full of such clauses, handed the editor references at reactive.vl's offsets labeled as belonging to the importing file. In a short file those landed past the end and were merely invisible; in a long one they drew over unrelated text, a comment two hundred lines from anything reactive among them. The clause now carries the file that wrote it, as every other deferred resolution already did, and its diagnostics are attributed there too rather than rendered against the importing file's text.

**A literal whose field or element spans lines breaks too.** `push(Subscriber { id, notify = || { … } });` closed on `} });` — three closings of three different things on one line — and every line was inside the budget, so the formatter left it, and put it back whenever its author broke it. A list or struct literal now splits when one of its elements renders across lines, the same complaint the chain rule answers one construct over. Unlike a chain, the *last* element counts: a chain that ends at its spanning link leaves a clean line and stays as written, but a composite's closing delimiter always follows its last element, so there is no such position. `std::json`'s codec — a two-field literal whose fields are both block-bodied closures, closing on a bare `} }` — is the case this most improves.

**A record built inside a call finally breaks.** `list.push(Task { id = …, workspace_id = …, name = … });` was 152 columns and stayed 152 however wide it grew: the statement is not a chain, so there was nothing to break at its own level, and the split stopped at the call's arguments. It now reaches a call's **last** argument — which is what a split chain's link already did, so the two permissions differed in nothing but where they were armed. The shape appears wherever a row is read into a record. A long *earlier* argument still leaves a long line; layout hangs off the final argument, and that boundary has not moved. Riding along, chains that live in an argument break too: `std::rpc`'s 221-column `match_of(…).arm(…).arm(…)` and `std::hash`'s 148-column `source("…" + impl_of(…)…)` are both that shape.

**A comment written inside a chain, list, struct literal, import or signature stays where you put it.** The comment machinery flushes at statement boundaries, so a comment written *inside* an expression had nowhere to go: it was re-emitted below the whole statement, never dropped but orphaned from the thing it explained — a note above one chain link would resurface before the enclosing function's closing brace. Such a comment now attaches to the element it precedes, on its own line at that element's indentation, in all five constructs the formatter can split. It also keeps the construct split even when the line would otherwise fit, because a collapsed construct has no line to keep the comment on; that is what makes the attachment possible rather than a preference. A comment *inside* an element — in a closure body one link carries — belongs to that body, prints where it was written, and changes no layout. Code without comments is unaffected: a hand-split construct that fits still collapses.

The examples were waiting on this. Seventeen of them reflow under the width rules shipped earlier in this release, and sweeping them before now would have moved their teaching comments away from the lines they teach; that sweep has landed with the comments untouched.

**A chain whose `})` is followed by more chain now breaks, however narrow it is.** Width was the formatter's only reason to split a chain, so a chain that read badly without being wide was left alone — and put back if you broke it yourself. `Server::builder().port(3000).on_request(|request| {` … `}).on_start(|server| {` … `}).build();` is inside the budget on every line and unreadable anyway, because `}).on_start(|server| {` is the end of one argument, the start of the next link and the start of its argument, all at once. A chain now splits regardless of width when a call link that is not its last renders across lines. A chain that *ends* at its spanning link keeps its shape: `self.cleanups.write().push(|| { … });` is the ordinary trailing-closure idiom, it has no seam, and breaking it would buy two lines and no clarity. Whether a link spans lines is decided by rendering it and looking, not by guessing from its shape — the same discipline the width rule follows.

## v0.19.0 — 2026-08-01

**The playground compiler can format.** `vilan-wasm` exports `format`, the CLI's `vilan fmt` rule exactly: canonical layout when the reprint round-trips, the original bytes untouched when the source does not parse or the printer declines. The playground page feature-detects the export, so its Format button lights up with the first release that carries this and older wasm builds simply do not show it.

**Editing a file that defines a macro no longer leaks a compiler per keystroke.** A macro's body compiles once into an isolated world the expander then runs. That world was cached by the layout of the whole defining file, so any edit that changed the file's length — typing almost anywhere — discarded the cached world and compiled and leaked a fresh one, in the editor, on every analysis. The world is now cached by the macro definitions themselves and survives edits around them. A macro definition that does not compile had it worse: its failure was never cached at all, so the world recompiled and leaked on every analysis even with the file untouched. Failed compiles now replay their diagnostics from cache, and the leak-measurement harness pins both behaviors so they cannot quietly return.

A sweep of every intentional leak site followed. The playground's compiler no longer leaks a copy of your program on every Run — identical source now reuses the first copy, and the leak shows up in the compiler's own accounting, which it previously escaped entirely. Two smaller per-keystroke leaks in the editor went the same way: a workspace dependency's display name, and the fallback path that generates derive implementations when no macro is in scope. And the compiler's leak accounting is no longer test-only: set `VILAN_LEAK_REPORT=1` and every analysis prints a cumulative per-site tally to stderr, so a long editor or watch session can show exactly what it has retained and why.

**`vilan fmt` formats five standard-library files it had been silently refusing.** The formatter's safety net returns a file unchanged rather than risk corrupting it, which is indistinguishable from a file that was already canonical — and five of std's own files had been in that state. Four printer gaps and one dropped node were behind it: a type's `context` clause (`(|| void) context turn_scope`), a mapped tuple type (`(U in T: Signal<U>)`), a tuple comprehension (`(source in sources => source.get())`), a tuple-arity bound (`T: (2..)`, dropped outright so `combine<T: (2..)>` reprinted as `combine<T>`), and a `void` written as a value, which lost its argument in `Verdict::Bad(void)`. All five now print, each pinned per shape.

The gate that should have caught this watched the regression corpus alone — the place bugs are deliberately planted, not the place the language's own source lives. It now watches std, the examples and the `vilan init` templates as well. The formatter's fixed-point pins were agreeing with the silence for the same reason: a file the formatter refuses is trivially a fixed point, so two of those twelve fixtures were proving nothing. They assert the formatter actually ran now, the way the per-construct pins already did.

**`vilan fmt` breaks up long signatures.** The width rule reaches a declaration for the first time (`proposal/signature-layout.md`): a `fun` whose signature line is over the budget renders one parameter per line, one level in, trailing comma on every one, and `)` back at the declaration's indent with the return type, a `borrows` clause and the body's `{` — or a bodyless `;` — glued after it. Signatures carrying closure types are wide by construction (`serve_connected` was 172 columns) and the author had no way to break them, because the formatter put them back. A list that fits stays inline without a trailing comma, an empty list never breaks, and a closure's parameters are never broken — they are an expression's own punctuation, not a declaration's contract. A call's *argument* list still does not wrap, which is a deliberate asymmetry rather than an oversight: an argument list sits inside an expression, where the builder convention decides layout, and the proposal says so outright.

**One block-bodied closure no longer exempts a whole statement from the width rule.** The formatter measured a statement's rendering only when that rendering was a single line, so a construct that opened a line and continued below it — a block-bodied closure, a `match`, a block — silently immunized everything printed before it. A `std::ui` tree ending in `.when(cond, || { … })` therefore stayed inline at any width, and the formatter had no way back out: `examples/reactive-ui/todos.vl`, hand-split by its author, reformatted into a single 707-column line. Width is now read from the rendering's FIRST line, which is the line the decision is about — the measured width and the line it describes stay the same thing, and body lines are measured where they are printed instead. Chains ending in a block closure split like any other; a statement whose opening line fits is still left alone however long its body runs. `std::rpc_server`'s `serve_connected` and `connected_response` were the two worst cases in std and are swept here.

**`vilan fmt` breaks up long imports.** An import's brace set is a list with braces, and over the budget it now breaks like one: one name per line, one level in, trailing comma on every one, `}` back at the opening line's indent — after the canonical sort, so a split run is the sorted run. A set that fits stays inline without a trailing comma. Organize Imports renders a split run identically, which it has to: the editor action promises byte-for-byte agreement with `fmt`, and if only one of them split the two would rewrite each other on every save.

**`vilan fmt` breaks up long struct literals.** A struct literal was the one composite the width rule did not reach: the printer joined its fields with `", "` no matter how many there were, so a hand-wrapped literal was *collapsed* onto a single line of whatever width it came to — and, having no layout of its own, could never be broken up again. A real one came out at 357 columns. A literal over the budget now renders one `field = value,` per line, one indentation level in, with `}` back at the opening line's indent, exactly as a list literal renders one element per line; the trailing comma follows the same rule, present on every field of a split literal (so adding a field is a one-line diff) and absent from one that fits. Generic arguments stay on the opening line with the name, shorthand fields take a line like any other, and an empty literal never breaks. Being the same rule, it composes with the existing ones in both directions at any depth: a field whose own line overflows splits in turn — as a nested literal, a chain, or a list — and a chain link's tail descent now reaches a struct literal sitting in its last argument, so `.child(Card { … })` breaks where `.child(column(…, [ … ]))` already did.

## v0.18.2 — 2026-07-29

**The npm packages are signed now.** Publishing to npm no longer goes through a stored token. The release workflow proves who it is to npm per run instead, so there is no long-lived credential behind the channel at all — nothing to expire, rotate, or leak. The visible half is provenance: each of the six packages carries a "Built and signed on GitHub Actions" badge linking to the exact workflow run that built it, so what npm serves can be traced back to this repository's tagged source. Installing and running are unchanged.

There are no language, standard-library, or toolchain changes in this release. If v0.18.1 is already installed, there is nothing here worth the download.

## v0.18.1 — 2026-07-29

**`npm install -g @vilan-lang/vilan` works.** v0.18.0 published its five platform packages and then stopped: the command that publishes the meta package passed a path with no trailing slash, so npm read `npm-dist/vilan` as its `<user>/<repo>` GitHub shorthand and went looking for a git remote instead of a directory. One character. The packages themselves are unchanged, and the platform packages published by v0.18.0 are fine — this release simply gives them the meta package that ties them together.

**The editor extension is on the VS Code Marketplace.** The last channel is live. Nothing in the extension changed; what was missing was permission for the release pipeline's identity to publish under the `vilan-lang` publisher. Installing from the release `.vsix` or from Open VSX still works and always will.

There are no language, standard-library, or toolchain changes in this release. If v0.18.0 installed for you, this one is not worth the download.

## v0.18.0 — 2026-07-29

**Breaking: a generic trait member is held to what the trait promised.** Trait conformance has checked receiver, arity, parameter types and return type since v0.12.0, but two positions slipped through, and both now diagnose. An impl that *fixes* a generic parameter to a concrete type is rejected: `trait Mapper { fun go<T>(&self, x: T): i32; }` is not implemented by `fun go<T>(&self, x: str)`, because the trait promised to accept any `T` and the impl accepts only strings. And a parameter whose type is a `Self`-defaulted trait generic is compared rather than skipped: `trait Add<B = Self>` resolves `B` to the same type as `Self`, which used to make the position ambiguous enough to go unchecked, so `impl Meters with Add { fun add(self, b: str): Meters }` compiled and only failed where it was used. Both now say "match the declared type" at the impl. With an explicit argument the two read differently, as they should: `impl Instant with Add<Duration>` expects `fun add(self, b: Duration): Instant` — the argument changes, the `Self` return does not.

**Install it with your own package manager.** The channels built in v0.14.0 are live for the first time: `npm install -g @vilan-lang/vilan` (the command stays `vilan`; the right platform binary comes down as an optional dependency, with no install-time script), `brew install vilan-lang/vilan/vilan`, and the editor extension on Open VSX for VSCodium, Cursor, and Theia. The curl and PowerShell installers keep working unchanged, and `vilan upgrade` still steers you to the right tool when it notices it is running from inside an npm or Homebrew tree. The VS Code Marketplace is not among them yet; until it is, install the extension from the release's `.vsix` or from Open VSX.

**The editor stops missing files you have not saved.** Two bugs, one cause: the open-document overlay reached exactly one reader, so everything else read the disk. A module that existed only as an unsaved buffer was invisible — you created `helper.vl`, typed into it, and the import diagnosed as missing while the file sat on screen — because the resolver asked the filesystem whether it existed and the filesystem said no. And for a module that *did* exist on disk with unsaved edits, the analysis read your buffer while diagnostics, hover docs and go-to-definition re-read the file, so every one of them landed off by the number of lines you had added. Reading is now one path: buffer if there is one, disk otherwise.

**The compiler builds for the browser.** A new `vilan-wasm` crate compiles Vilan to JavaScript entirely in WebAssembly, with the standard library carried inside it and no filesystem underneath. It ships as a release asset (2.2 MB, 0.64 MB compressed) and does nothing yet: it is the engine for the web playground, whose page comes next. Nothing in the toolchain you install depends on it.

## v0.17.0 — 2026-07-28

**Same-scope shadowing.** A `let` may redeclare a name in the same scope: the later binding shadows the earlier one from its own declaration point onward, and uses before that point keep the earlier binding — `let d = parse(d);` narrows a value under the same name, the way it reads. A binding becomes visible at the *end* of its declaring statement, so an initializer never sees the binding it declares: `let x = x + 1;` reads the previous `x`, and is an error when there is none. Parameters, loop items, and pattern captures are shadowable the same way; module-level bindings keep their order-independent, one-declaration-per-name rule. Two bugs died with the old behavior: `let x = x;` sent the analyzer into a stack-overflow abort (as did a module-level `let a = b; let b = a;`), and a same-scope redeclaration used to bind *every* use of the name — earlier ones included — to the last declaration, so a cleanly-compiling program crashed at startup with a `ReferenceError`.

**Breakpoint chains are mobile-first for real.** `std::style` emitted `@media (min-width: …)` rules in lexical order, and `'1' < '6'` put the 1024px rule before the 640px one — so with two breakpoints on the same property, `.sm(two_col).lg(three_col)`, a desktop viewport matched both, specificity tied, and the *narrow* value won the cascade. Media rules now emit ascending by min-width, so the widest matching breakpoint wins — the rule the docs now state outright. Everything else about the stylesheet is byte-identical.

**Inline SVG renders.** `view("svg")` used to build an HTML-namespace element — same serialization, renders nothing — because `document.createElement` knows only one namespace. `view` now recognizes the SVG vocabulary (exact case: `clipPath`, not `clippath`) and creates those elements through `createElementNS`; on the server, an `svg` root serializes with its `xmlns` attribute, and the SSR differential pins the two legs byte-for-byte. Tags that exist in both vocabularies (`a`, `title`, `style`, `script`) stay HTML. Riding along: `class`/`styled`/`bind_class` now set the `class` attribute instead of assigning `className` — identical for HTML, and the only form SVG accepts (its `className` is readonly; assigning it throws in module code). Icons can finally live in the view tree and inherit `currentColor` instead of shipping as pre-colored `<img>` files.

**The editor stops red-flagging shared files in two-entry packages.** A file shared between a browser entry and a node entry — the fullstack shape `vilan init` scaffolds — analyzes in the editor under a platform inferred from its imports, and the inference used to read *any* `std::ui` import as browser evidence. A shared file importing the process twin's `render` was therefore analyzed as a browser file and its import squiggled "cannot find `render`", while `vilan build` was clean on every entry. Inference is now name-aware: for a module both platforms serve, the *imported names* decide (`render` says process, `mount` says browser, `view` says nothing), so shared files resolve the twin they actually use.

**One bad request can no longer take down the language server.** A panic in any request handler used to unwind through the async runtime and abort the whole server — and after five crashes in three minutes the editor stops restarting it, so one poisoned hover locked out every feature until a manual restart. Handlers are now panic-fenced: a read-only query answers its empty default, rename and formatting refuse with an honest "this is a bug" error instead of pretending there was nothing to do, and the panic is logged to the output channel either way. The fences reach deeper too — a lexer or parser panic now degrades to a diagnostic like an analyzer one always has, and the caught panic can't poison the server's locks into failing every later request.

**Compiler messages punctuate like prose.** Diagnostics, the CLI's status and error lines, the language server's hovers and completions, and the HMR overlay all used " — " as their universal separator. They now punctuate like sentences: a colon before the rule, a semicolon before the fix. The words are unchanged, so anything matching message *text* still matches; anything matching the exact punctuation will see the difference. The book and every README received the same edit, and the pages that quote compiler output quote it verbatim again.

**Release artifacts carry their third-party notices.** The release archives, the npm packages, the Homebrew install, and `vilan upgrade` all ship `THIRD-PARTY-NOTICES.txt` alongside the licenses. The file is generated from the exact dependency lockfile and a suite gate fails the build when a new dependency is missing from it, so it cannot fall behind.

## v0.16.0 — 2026-07-28

**Breaking: single-quoted strings no longer span lines.** A raw line break inside `"…"` or `i"…"` is now a compile error, and so is a backslash before one — nothing escapes a line terminator. Multi-line text goes in the triple-quoted forms, `"""…"""` and `i"""…"""` (the interpolated `i"""…"""` form is new in this release; plain `"""…"""` arrived quietly in v0.10.0); a single line break inside a one-line string is written `\n`. The payoff is error locality: a string whose closing quote you forgot used to run on to the next `"` anywhere below it, so the compiler complained somewhere else entirely — often many lines away, about something unrelated. It is now reported on the literal's own line, pointing at the opening quote, and the rest of the file still compiles and still serves hovers and completions in the editor. The editor's syntax highlighting agrees: a broken string paints at most its own line. To migrate: a multi-line `"…"` becomes `"""…"""` (an `i"…"` becomes `i"""…"""` — the triple forms are raw and strip the closing delimiter's indentation), or collapse onto one line with `\n`.

**Breaking: `serve_service` and `serve_connected` hand their ready callback the `Server`.** `on_ready` is now `|Server| void`, matching `serve_rpc` — which is what makes `serve_service(0, …)` usable: the server you're handed knows the port it actually bound (`server.port()`, new this release).

**`vilan init` — install, init, run is the whole first minute.** Three templates ship embedded in the binary — `node`, `browser`, and `fullstack` in the one-package/two-entries shape — chosen with `--template <name>` or an interactive prompt on a TTY (and a clean error, not a hang, without one). It never overwrites: a file that already exists is an error. The templates are suite-gated, so a scaffold that stops compiling fails vilan's own build, not your first minute. The `vilan/examples` tree was reshaped to match — single-package/two-entries is now the default project shape in examples and docs alike, the multi-package workspace kept where it genuinely teaches workspaces, and every example carries a README saying what it demonstrates and how to run it.

**The manifest learned the dev loop's two missing keys.** `[build] run` — a command or a list of them — runs before each build and each `--watch` round, sequentially, from the manifest's directory: the Tailwind bridge, asset pipelines, codegen sidecars. A failing hook fails the build naming the command, and `vilan check` runs none of them. And `default-entry` names the entry `vilan run` should drive in a multi-entry package, in both manifest shapes, with the precedence you'd expect: `--entry` beats the manifest, the manifest beats the lone-leg default, and with none of the three the error names both ways to choose.

**Cancelable timers.** `std::time::Timer` is a delay you keep hold of — `setTimeout` and `clearTimeout` as one value. `Timer::after(ms)` (or `after_for(duration)`) starts the timer immediately; `timer.wait()` yields `true` when it fires and `false` when `timer.cancel()` got there first, and that verdict is remembered, so every waiter — one parked before it settled, one arriving long after — gets the same answer, and asking a settled timer returns at once. Cancelling twice, or cancelling a timer that already fired, does nothing. This is the shape a re-clickable button wants: keep the pending timer in hand and call it off before starting the next one, instead of leaving a stale sleeper to wake up and hopefully notice it's stale. A `Timer` is an ordinary value wrapping one host handle, the way a `Signal` wraps one cell, so copying it shares the same timer. And the two kinds of cancellation stay distinct: `cancel()` is a verdict, while a cancelling nursery tears down only the task that was awaiting — the timer itself is untouched and its other holders can still wait on it or call it off.

**Every diagnostic renders in its own file.** Every post-analyze pass — const, platform, async, context, drops — used to anchor its diagnostics to the *entry* file: the CLI rendered the entry file's text at another module's offsets, and the editor squiggled the wrong file entirely. Each diagnostic now renders in the file it belongs to; notes attached to a diagnostic reach the editor as locations in *their* files, so a note pointing across a module boundary finally lands where it points; and the HMR overlay names each diagnostic's own file instead of the entry. A rider closes the chained-element-access trap (`grid()[0][1]` and family), with six shapes pinned.

**Highlighting and inlay hints hold still while you type.** The language server keeps two views of an open file — the text you are editing and the text it last analyzed — and it used to mix them: an answer computed from the older analysis was converted to line/column through the *newer* text, so one character inserted anywhere above shifted every token and hint below it, and hints that shifted out of the visible range vanished outright. Every answer that comes from analysis — semantic tokens, inlay hints, hover, go-to-definition, find-references, the outline — is now expressed in the coordinates of the text it was actually computed from, which stays visually correct everywhere except the line you are on; and when the new analysis lands a moment later the server asks the editor to re-request both providers, so the catch-up happens immediately instead of whenever the editor next feels like asking. Three related fixes ride along: a completed analysis now *merges* into the buffer rather than replacing it (characters typed during the 80–190 ms it takes were being thrown away), an analysis finishing after you close a file no longer resurrects it, and semantic-token lengths are counted in UTF-16 units as the protocol specifies. Rename and Organize Imports — the two requests that hand back edits — answer "still analyzing, retry" for the fraction of a second while the buffer is ahead of the analysis, instead of returning edits computed against text you no longer have. Two hover fixes ride along: a constant whose preview carried multi-byte text crashed the server outright (a byte-budget cut landing inside a character), and comments or blank lines inside a function body no longer hover as the enclosing function.

**`vilan fmt` breaks up long method chains.** A statement wider than 100 columns whose expression is a chain of two or more `.method(…)` calls now renders with its subject on the statement's line and one link per line below it, indented one level — the shape a style builder or a fluent API wants, and the shape you probably wrote before the formatter collapsed it. The choice is purely width-driven and so is stable in both directions: a chain that fits stays on one line (a hand-split short chain still collapses), and a chain that doesn't always breaks the same way, so formatting is a fixed point. Non-call postfixes (`.field`, `[i]`, `!`) ride along with the link before them rather than taking a line each, and a chain that continues into an operator — `style()…margin(space(0)) + reveal` — puts the continuation on its own line at the links' indentation. Either side of an operator can be the chain that breaks: when it is the **right** side — `let tint = const (base + style()…)`, the shape a style module full of composed rules is written in — everything through the operator and the chain's subject stays on the statement's line, the links break below it, and the closing parenthesis and the `;` glue after the last one. The left side wins when both sides could break, and the right side then breaks only when the continuation line it landed on is itself over the budget. This is the formatter's first width-aware decision, and it is applied per line, recursively: when a link's *own* line still runs over the budget, the call on that line breaks its last argument too — a nested chain drops its links one level further in, and a list literal goes one element per line, indented past the line that opened the `[`, with a trailing comma after every element and the `]` back at that opening indent. A hand-nested `std::ui` view tree — `view("div").styled(…).child(view("div").styled(…).child(…))` — therefore comes back out the way its author wrote it, at any depth, in a single pass, while every subtree that fits stays on one line. A list that fits also stays on one line, *without* a trailing comma, so the comma marks a split list and nothing else. Two things deliberately do not move: layout hangs off a call's **last** argument, the builder convention every language's formatter follows, so when an earlier argument is what makes a line long the line stays long; and argument lists are still never wrapped.

**`vilan fmt` stopped skipping files with redundant parentheses.** A parenthesized group that the language did not strictly need — `let b = (1 + 2);`, `ret (x);`, `f((1 + 2))`, `(300).as_u8()`, and above all `const (chain + reveal)` — used to make the formatter give up on the **entire file** and return it byte for byte, with `fmt --check` then reporting that file as already clean. Those files now format, and the parentheses are kept exactly where you wrote them: the formatter preserves a group you wrote rather than judging it redundant, because a redundant group is usually there for clarity. A style module written as `let heading = const (style()…margin(space(0)) + reveal);` was exactly this case, so it is also what kept the new chain splitting from reaching real files.

**`vilan test` works in workspaces and `[library]` packages.** A manifest without `[package]` used to resolve to an empty workspace, so `vilan test` there compiled against nothing. Tests now see their `pkg::` siblings, path dependencies, and git dependencies (fetched on demand) in every project shape. The editor gained the same footing: a file in a `[library]`-rooted project resolves its own modules and dependencies, a manifest that fails to parse says so in the editor, and an inherited-declaration error is addressed to the manifest that declared it.

**Handles cross the wire.** `[derive(Wire)]` now accepts types carrying `Handle<T>`, and `Arena::branded()` starts an arena's generation counters at a random value instead of zero, so a handle issued by one branded arena is rejected by another — the shape a server handing session handles to clients wants.

**The entry file's case is checked.** A case-mismatched entry path — `Main.vl` on disk, `main.vl` in the manifest — is now a clean diagnostic on every path that names an entry, including `[entry.<name>]`, extending v0.14.0's case-exactness rule to the one file it missed.

**The book's canonical home is vilan-lang.org/docs.** Old deep links redirect.

**The VS Code extension requires VS Code 1.91 or newer.** The extension's language-client library moved to its current major (v10, clearing npm audit's outstanding advisories), and that library's floor is the extension's floor. The server's output channel became a log channel on the way: the Vilan Language Server output now carries timestamps and a per-level filter.

**Vilan has a new look.** The palette moved from indigo-and-lavender to blush on near-black (`#F9DFE7` on `#120004`), and every rendering of the brand moved with it: the repository header, the VS Code extension's icon and listing banner, the CLI's post-upgrade mark, and the website. The mark itself is unchanged.

## v0.15.0 — 2026-07-25

**Module bindings initialize in dependency order.** A top-level `let` now runs after every binding its initializer actually evaluates — the ones it reads, plus everything read inside whatever it calls on the way — so a binding may reference one declared below it, in the same file or in another module, exactly as a function may call one declared later. Creating a closure evaluates nothing, so two module-level closures may still name each other freely. This kills a real miscompile: declaration order used to follow the order names happened to be listed in your *imports*, so a constant that depended on another could be emitted before it and crash at load with `Cannot access 'X' before initialization` — with nothing at compile time to warn you. v0.12.0 made the emitted JavaScript independent of import *statement* order; this closes the other half, the names inside a `{ … }` brace set, so **no spelling of your imports can change what your program does or the bytes it compiles to** — `vilan fmt` can sort them freely. And a genuine cycle among initializers (including a binding that reads itself) is now a compile error that names the round trip (`via A → B → A`), anchored at the read that closes it and noting each participant's declaration, instead of a crash at load. The order is specified rather than incidental: spec §7.1 fixes dependency order first, then a canonical module order — the standard library first, then dependency packages, then your own, modules within a package by name, the entry file last — for bindings that depend on nothing from each other. One behavior note: a module initializer with *side effects* may now run in a different relative order than before — the old order was whatever your import listing happened to produce; the new one is the rule above. Bindings that actually depend on one another are unaffected: those were the broken case.

**Git dependencies.** A dependency can now come from a repository, pinned to an immutable point: `shapes = { git = "https://…", tag = "v1.2.0" }` (or `rev = "<commit sha>"` — exactly one; a `branch` is refused, because a branch moves and so cannot pin anything). The checkout is fetched shallowly, verified to be a vilan `[library]`, and cached content-addressed under `~/.vilan/` — after the first fetch, builds are fully offline, and the cache serves the *pinned* content even if the upstream moves or disappears. A dependency's own git dependencies resolve through the same cache. Fetching happens only when a build of a declaring project needs it — the toolchain still makes no passive network calls, and the editor never fetches at all: a not-yet-fetched dependency shows as a note to run `vilan build`. Workspaces got a matching quality-of-life: `[project.dependencies]` declares a dependency once at the workspace root, and a member opts in with `shapes = { project = true }` — explicit, so nothing is inherited by surprise.

**`vilan.toml` speaks in the editor.** The manifest now has completions — keys per table, values where they're enumerable (targets, presets), quotes placed for you — and its problems finally surface where you're looking: a manifest that doesn't parse, an invalid dependency, or a git dependency that isn't fetched yet publishes a diagnostic on `vilan.toml` itself and clears when you fix it.

**Installing vilan is becoming one command.** The Homebrew tap is live today: `brew install vilan-lang/vilan/vilan` (macOS and Linux, both architectures). The npm package (`@vilan-lang/vilan` — the command is still just `vilan`) and the VS Code Marketplace / Open VSX listings are built and ship with the next releases as their publishing credentials land. `vilan upgrade` learned to respect whoever installed it: an npm- or brew-managed vilan is steered to `npm update -g` / `brew upgrade` instead of overwriting files the package manager owns — the curl-script install keeps upgrading itself exactly as before.

**A save during the first watch build is never lost.** `vilan run --watch` took its file-change baseline *after* the initial build, so a save landing while that build ran was silently absorbed — the watcher never noticed, and your change sat unbuilt until you saved again. The baseline now precedes the build: a save at any moment after the watcher starts triggers a round. Found because a CI test kept "flaking" — it was right, four times, on three platforms.

## v0.14.0 — 2026-07-24

**Vilan runs on Windows.** Native, not WSL: install with one PowerShell line (`irm https://github.com/vilan-lang/vilan/releases/latest/download/install.ps1 | iex`), and the whole toolchain is there — `vilan.exe` and `vilan-lsp.exe`, the compiler, `run --watch` with hot reload, `fmt`, `test`, and `vilan upgrade` (which learned the Windows swap: a running executable can't be replaced in place, so the old one steps aside and is swept on the next run). The VS Code extension finds the server on Windows now, and the language server treats every spelling of a file — `C:` vs `c%3A`, even DOS-era `RUNNER~1` short names — as the one file it is, so diagnostics never duplicate or stick. Stopping a watch round kills the *whole* process tree (a forking dev server can't hold its port hostage), colors render in both Windows Terminal and classic conhost, and the entire test suite now runs green on Windows in CI as a required check on every change — this isn't a port that will quietly rot.

**Line endings became law.** A `\r\n` in source is one line terminator, and string literal values are built from the normalized text — a multi-line string contains `\n` regardless of how your editor or Git saved the file. This closes a real miscompile: the same program checked out with CRLF endings used to embed `\r` in its string values and emit different JavaScript than its LF twin. A leading byte-order mark is now stripped everywhere source is read, canonical Vilan is LF (`vilan fmt` converts), and a `.gitattributes` keeps every checkout byte-stable. The full corpus compiles byte-identically from LF, CRLF, and BOM'd copies — pinned, on both platforms.

**Module names are case-exact — everywhere.** New language rule (spec §4.2): an import must match the on-disk file name byte for byte. On a case-insensitive filesystem, `import foo` finding `Foo.vl` is now a clean diagnostic naming both spellings instead of a resolution — so a program that builds on Windows builds identically on Linux, with no case-sensitivity surprises waiting in CI.

**Errors print to stderr now, and every diagnostic respects `NO_COLOR`.** Compile errors joined warnings on stderr — `vilan build --stdout` can never again interleave a diagnostic into the JavaScript it pipes — and the ariadne-rendered reports finally obey the same terminal gate as the rest of the CLI: colored when you're looking, byte-plain when piped or `NO_COLOR` is set. If you were parsing errors from stdout in a script, read stderr instead.

## v0.13.0 — 2026-07-23

**Server-side rendering: render and replace.** A full-stack app can now serve its first paint as real HTML — for the crawler, and for the human who sees content before any JavaScript arrives. The model is deliberately simple: on the server, `import std::ui` resolves to a render-only implementation of the same API, so the *same component functions* build an HTML string instead of DOM — each `bind_*` embeds its signal's current value, event handlers are accepted and ignored — and `render(view)` hands your route handler the markup to splice into its shell. On the client, `mount` now replaces the container's contents when it boots, so the server HTML gives way to the live, bound tree in place. There is **no hydration** — no node adoption, no mismatch errors, no second set of rules — and that is a design decision, not a gap: the eventual path is resumability, which makes hydration's machinery obsolete anyway. The two `ui` implementations are held together by a differential test that renders one shared component through both and requires byte-for-byte agreement. The new [`examples/ssr`](vilan/examples/ssr/) is the working loop, and the [SSR guide](https://vilan-lang.org/docs/guide/ssr.html) explains the one rule that matters (build pure, bind reactive) and where v1 fits: self-contained and server-data-seeded apps — views that read a live rpc client while building are client-side by nature.

**Snippets in completion.** `for`, `fun`, `struct`, and `match` now complete as tab-through templates alongside their bare keywords — parameter names, field stubs, and match arms pre-placed — degrading to plain keywords in editors without snippet support.

**The CLI dresses for the terminal.** Build, watch, test, fmt, and upgrade output is colored when you're looking at it — green successes, bold red errors, cyan dev-loop lines — and byte-for-byte plain the moment it's piped or `NO_COLOR` is set.

## v0.12.0 — 2026-07-22

**The editor grew up.** Completing a function now inserts a real call — tab-through parameter placeholders by default (`greet(name, times)`), parens-only or plain-name via the new `vilan.completion.functionCall` setting — with the full signature and `///` doc shown right in the suggestion popup and parameter hints opening as you land in the parens (completing a callee you already parenthesized, or a function passed as a value, stays bare). Hover now answers everywhere: variables show their typed binding, parameters show their declared convention (`own x: T`, `x: &mut T`), and every keyword explains itself in one line with a deep link into the book. **Organize Imports** sorts and prunes: unused imports and brace-set branches are removed conservatively (never while the file has errors, never re-exports, never an import that only a derive's generated code uses — the compiler knows), with an opt-in `vilan.organizeImports.onSave`. Inlay hints and semantic tokens gained toggles, every setting applies live, and — pinned by a thirteen-test guarantee — the language server keeps working in files with errors: hover, navigation, completion, and the outline all serve the parsed remainder on both sides of a typo.

**`vilan fmt` sorts imports, and import order stopped mattering.** Top-level imports format into one canonical order (`std` first, then dependencies, then `pkg`; brace sets alphabetized; comments travel with their line; block-scoped imports deliberately untouched) — and underneath it, the compiler now walks modules in a canonical order too, so **the emitted JavaScript is byte-identical no matter how your imports are arranged**. Reordering an import can never again churn your build output.

**The extension ships its licenses** (MIT OR Apache-2.0) in the package, ready for the marketplace.

## v0.11.0 — 2026-07-22

**Hot module replacement — the dev loop closes.** `vilan run --watch` on a full-stack workspace now hot-reloads the browser: save a file and the app updates in place with module-level state carried across the swap (plain values by value, `Signal`/`Shared` by payload into fresh cells — keyed and fingerprinted by the compiler, so a changed shape fresh-inits instead of adopting stale data), while the server leg restarts behind the scenes and the client's rpc mirrors resync on their own. A CSS-only edit hot-swaps the stylesheet without a reload; a compile error shows an in-page overlay carrying the *actual* compiler diagnostics (file, line, message, note — the terminal's own rendering) and clears on the next good save; `std::dev` gives app code `on_teardown` and a type-checked `stash`/`take` carryover (only plain data may cross a swap — the compiler enforces what Vite leaves to convention). Watch rounds got structurally cheaper too: parse results are content-cached across rounds and a leg whose sources re-hash identically is skipped outright, its artifacts reused byte-for-byte. Multi-server workspaces pick their entry with `vilan run --entry <name>`. The [dev-loop guide](https://vilan-lang.org/docs/guide/dev-loop.html) walks the whole loop.

**The frontend is handwritten now — builds are ~2.7× faster.** The chumsky combinator frontend is gone, replaced by a hand-rolled lexer and recursive-descent parser proven byte-identical first (279/279 whole-file tree agreement, every corpus program compiled to identical output through the new code *before* it was wired in) and then measured: a release build of the todo client dropped from ~0.49 s to **0.18 s**, instruction counts fell 5.21 B → 2.01 B, and the frontend went from ~63% of a compile to under 4% — the debug binary gains the most. Parse errors improved with it: the 30-token "expected one of …" dumps are gone, a missing separator reports `found 'y' expected ',' or '}'` at the offending token, the `a!==b` spacing trap gets a first-class hint, and a syntax error no longer discards the whole file — the parsed prefix survives, so the language server keeps working on everything above the typo.

**Trait impls must now match their trait's signatures.** Previously an impl satisfied a trait by member *name* alone; receiver convention, parameter types, arity, and return type were never compared, so a wrong `fun drop(self)` compiled against `fun drop(&mut self)`. Every member is now checked under the trait's own generics (`Self` included), with the mismatch spelled per dimension. **This can reject code that previously compiled** — the fix is to make the impl say what the trait says. (A deliberate leniency: an `async` impl of a sync-declared method stays legal — dispatch is monomorphized, so the caller always knows the concrete callee.)

**Two real bugs died.** A module-level closure referenced *only* by calls (`let helper = || …;` used as `helper()`) was tree-shaken out of the bundle while its call sites remained — a runtime `ReferenceError`; calls now count as references, and six sibling shapes (calls through `?.`/`!`, transitive closure chains, nested modules) were quietly broken the same way and are fixed with it. And a typo'd name in value position no longer cascades — one unknown identifier is one error, not a fan of `Expected i32, but got void` noise at every use.

**`vilan fmt` formats everything.** The formatter silently returned files unchanged when they used newer constructs — destructuring, fixed arrays, `?.` chains, the macro forms, numeric suffixes. Every construct now has its printer, guarded by a standing zero-bail gate over the whole corpus, and two latent printer bugs found on the way (one would have reformatted `-(2 + 3)` into `-2 + 3`) are fixed. The standard library itself is freshly `vilan fmt`-formatted.

**Sharper diagnostics across the board.** Notes that pointed into `std` for user-caused conditions were audited (they are all genuinely declaration notes — "the trait declares it here" — and stay); one unresolved name suppresses its whole echo family; and the diagnostics ledger now runs as a living gate — every new compiler error message gets verdicted against the standard as it lands, not in batches after the fact.

## v0.10.0 — 2026-07-19

**Resources: values that clean up after themselves.** A `resource struct` (or `resource external struct`) is the new owned-resource class — a value with exactly one owner that **moves** on binding and `own`-passing instead of copying, is loaned through the ordinary view conventions, and runs its `Drop` at its owner's scope end, every exit included (`ret`, `jump`, panic unwinding — and a value-returning `main` now runs its drops *before* the process exits). Containment infers: a struct, enum, tuple, or fixed array holding a resource *is* one. `Option.take`/`replace` are the sanctioned partial move, std's `drop(value)` destroys early with no public `close()` anywhere, and the affine checker rejects the whole double-close family at compile time — use-after-move (with a note at the move), conditional moves, moves in loops, resource captures in closures and spawns, resources in native containers, coercions to `any`, and derives (`Wire`/`Hashable`/`PartialEq`) on resource-holding types. `Database` is the first real resource: it closes its `node:sqlite` handle deterministically, module-level handles keep process lifetime (the serve-forever idiom — now **loan-only**, and reachable from closures, which the checker previously miscounted as captures), and `OwnedNursery` owns background tasks whose real failures still reach the console with their spawn origin while cancellation echoes stay silent. The [resources tour](https://vilan-lang.org/docs/tour/resources.html) walks it; spec [§6.8](https://vilan-lang.org/docs/spec/memory.html) is the contract.

**One law now opens the memory model.** Spec [§6.0](https://vilan-lang.org/docs/spec/memory.html): every alias is a *claim* on an owner whose *epoch* advances on a fixed set of events — and a claim is valid while its owner's epoch is unchanged. Views are the statically-proven claims, handles the dynamically-checked ones, and every mechanism in the chapter (views, projections, `Arena`/`Handle`, `Shared`, resources) is presented as a cell in that one table.

**Rule 4 is now enforced everywhere views actually come from** — and it's smarter about what invalidates. Previously only a direct `&place` view was policed; a view returned through a call (`list.at(0)`, `arena.get(h)`) or bound by a `Some(let v)` match capture was invisible to the invalidation checks (and a *chained* projection didn't even lower as a view — a real miscompile, fixed). Now every view anchors at what it projects, multi-parameter projections anchor at all of them, and mutating a viewed container, reassigning its root, or holding any of these across `await` is the same compile error the direct form always raised. **This can reject code that previously compiled** — re-derive the view after the mutation or suspension, as ever. In exchange, the checker stopped over-rejecting: only calls that may change a container's *geometry* (grow, shrink, reallocate, swap an aggregate field — inferred per method as the new `bumps` effect, hover-visible beside `borrows`) conflict with a live view; a method that merely writes fields or elements through `&mut self` now passes freely.

**`Arena.get` hands back a live view** — `Option<&T> borrows self`, the shape the spec always described, instead of a copy; `set` remains the write path, and stale handles still answer `None`.

## v0.9.0 — 2026-07-18

**Higher-order functions adapt to async callbacks.** `map` is one function, not two: passing an async closure instantiates an async copy of the receiving function — its calls through the parameter are awaited, **sequentially** (each callback settles before the next begins) — while every sync call site keeps the untouched original. Adaptation follows the closure through plain parameters transitively (`helper(xs, f)` forwarding into `map` adapts end-to-end), an adapting function traverses a snapshot of its receiver so interleaved work can't tear the iteration, and it stops honestly at the boundaries: a parameter marked **`sync`** declares the synchronous contract (the reactive graph's recompute positions — `Signal::map`, `turn`, `batch`, the UI render callbacks — are `sync`), host (`external`) functions can't await your closure (unless a parameter is *declared* `async |…| T` — the typed channel), and trait/generic dispatch has no static callee to instantiate. When the elements are independent, opt into concurrency with the spawn-then-settle idiom: `.map(|x| async work(x))` then `Task::settle_all(tasks)`.

**Spawning grew a spine: `Task<T>`, and nurseries to own them.** `async expr` now yields a `Task<T>` — an eager, opaque handle; copying it refers to the same task. Every task absorbs its own failure at construction: a spawned panic can never crash the program as a host "unhandled rejection" — a later `await` receives it, and a task nobody observes reports the error to the console stamped with the function that spawned it, then execution continues. `Task::settle_all` joins many; `Task::race` yields the first to settle. Raw host promises stay `Promise<T>` at the extern seam, and `await` unwraps both.

**`nursery(body)` is structured concurrency** (`std::task`): every task spawned in the body's *dynamic extent* — by the body, by anything it calls, by the tasks themselves — is joined before the nursery returns the body's value. Failures follow the first-observed rule: a body throw wins, otherwise the earliest-settled task failure, re-raised from the `nursery` call with its spawn origin while every other task is absorbed. `n.cancel()` aborts the whole extent — the nursery's `AbortSignal` rides ambiently into `sleep` and `fetch`, so cancellation cuts in-flight IO short instead of waiting it out (a live e2e cancels a fetch against a hanging endpoint and joins in ~3s instead of 60), cancellation rejections are absorbed echoes rather than errors, nurseries chain so an outer cancel reaches nested extents, and `Task::race` + `n.cancel()` is the race idiom. The first real failure cancels the same way, so one task's error stops its siblings' work at settle time — not when the join happens to look. Spec [§7.7](https://vilan-lang.org/docs/spec/execution.html) is the contract; the [async tour](https://vilan-lang.org/docs/tour/async.html) walks it.

**Asyncness now rides every value channel.** `async |T| U` is accepted on struct fields and function return types (calls through a field read or a returned closure await implicitly), unannotated bindings adopt asyncness from any value they hold — including `mut` rebinds — and storing an async closure where a plain value-returning closure type is declared (a field, a return type) is a compile error instead of a promise wearing the wrong type. Void-returning positions keep spawn semantics, which is why UI handlers await freely with no ceremony. The standard library's own transport and draft plumbing was migrated off its workarounds in the process.

**Variadic tuple bounds are enforced.** `T: (2..)` and `(..: Display)` parsed since variadics landed but checked nothing; arity ranges and per-element trait bounds now hold at every call and construction site, with the note pointing at where the bound was declared.

**Editor and diagnostics tail.** Notes can point into another file (the "declared here" half of a cross-module error lands in the right source); inlay type hints for inferred `let` bindings; semantic tokens gained modifiers; parse errors name the split (`a! == b` vs `a != b`) instead of dumping the expected-token soup; `x.field()` on a closure-valued field steers to `(x.field)()`; and multi-file diagnostic publishing dedupes across dependents, so fixing a shared module clears its dependents' stale squiggles in one pass.

## v0.8.0 — 2026-07-16

**Diagnostics got a standard — and every one of the compiler's 180 diagnostics was audited against it.** The rules: anchor at the narrowest span that identifies the problem, in code you wrote; speak your vocabulary; name the fix when it's unambiguous; and never bury a root cause under its own consequences. What the audit shipped: "cannot find" errors now steer to the import when the name uniquely belongs to a module (`cannot find type 'JsonValue' — import it first (\`import std::json::JsonValue;\`)`); a conflict with an inferred type points at where the inference happened — the closure's first call, the variable's initializer — as a second label at that exact spot; "has no method" anchors at the method name instead of the argument list; an error inside macro-generated code anchors at the attribute that generated it, in your file; and the near-empty "could not be resolved" residuals only appear when they're the lone signal instead of trailing a real error.

**`///` is the doc-comment syntax.** Hover surfaces `///` blocks; a plain `//` comment is an implementation note and stays private. The standard library is documented with it — hovering `now()`, `format`, or any std function shows its docs in the editor.

**The editor understands the code, not just the text.** Semantic highlighting comes from the analyzer: a generic parameter at its use site, a macro name sharing a trait's name, a method call versus a field read, a module qualifier — each colors by what it *is*. Hover on a constant shows its evaluated value (`SIZE: i32 = 64`), signatures render their `context` clauses, and `[` before an item completes the registered macro names, derives included. Unsaved edits were already visible to dependent files as of v0.7.0; the editor now reads as precisely as it recompiles.

## v0.7.0 — 2026-07-16

**Expression lifting: a bare `?` lifts the whole expression.** Where `?.`
continues a member chain, `?` on its own lifts the rest of the surrounding
expression — `count? * 2`, `deadline? < now()`, and the two-receiver form
`price? + tax?`, which is good only when every receiver is and
short-circuits left to right (a receiver after a `None`/`Err` never runs,
like `&&`; on `Result`, the first error wins and every receiver shares one
error type). The lift stops at natural boundaries — call arguments, struct
fields, parentheses — and a `?` that lifts nothing, or would turn an `if`
condition into an `Option<bool>`, is an error with a steer. `?.` chains are
unchanged. `Option`/`Result` only for now; lowers to plain branches, no
closures.

**Fixed arrays round out: `.len()` and destructuring.** `arr.len()` folds to
the constant (the length lives in the type; a side-effectful subject still
evaluates, exactly once). `let [r, g, b] = rgb;` destructures — irrefutable,
element count checked against the type, nesting arrays and tuples freely,
and it works in parameter position (`fun sum([a, b]: [i32; 2])`). Elements
come out as value copies, like everything else.

**Conditions are type-checked now.** `if 5 { .. }` used to compile and
branch on JS truthiness — an `Option` condition always took the branch.
Every `if`/`for` condition must now be a `bool`, spanned at the condition.

**Two soundness holes closed.** An unannotated `Map::new()` never grounded
its key/value types, so mixed-typed inserts compiled and ran — a binding
whose type keeps a callee's parameters now demands an annotation. And a
derive's internal imports leaked into the deriving module (`JsonValue`
resolved with no import after `[derive(Json)]`) — expansion imports are
scoped to the expansion now.

**Editor and diagnostics.** Unsaved edits propagate to dependent files
immediately (analysis reads open buffers, not disk). A conflicting call on
an unannotated closure names the first call that fixed the parameter's
type. A heterogeneous list literal (`[1, "x"]`) is rejected instead of
silently typing by its first element.

## v0.6.2 — 2026-07-15

**Two generic miscompiles fixed.** A `&mut T` view resolving to `bool` through
a generic, and integer division / bitwise ops on `i32`/`u32` through a generic,
silently did the wrong thing: the boolean write-through was a no-op, `i32`/`u32`
division skipped its truncation (`7 / 2` came out `3.5`), and a `u32` shift used
the signed operator. Both were monomorphization-time classifications that dropped
their verdict for the native-JS types — concrete code and every other integer
width were already correct. Found by an audit after v0.6.1's `&mut bool` fix.

**`!` guides you to convert errors.** `!` returns a failure as-is, so the error
types must match; when they don't, the compiler now points at the fix instead of
calling it unsupported — `.map_err(…)` to change a `Result`'s error, `.ok_or(err)`
to turn an `Option`'s `None` into one. Conversion stays explicit (no hidden
`From` behind the operator), by design.

## v0.6.1 — 2026-07-15

**`&mut bool` write-through, fixed.** A writable view of a boolean *local* —
`let v = &mut flag`, or passing `&mut flag` to a function — silently did
nothing; the write never reached the original. v0.6.0 introduced `&mut bool`
views but boxed only number and string locals, so a boolean's backing cell
was missing and the write landed nowhere. Views of boolean *list elements*
and *struct fields* were already correct; this fixes the bare-local case,
the `v = !*v` toggle included.

## v0.6.0 — 2026-07-15

**Map and Set key by value.** A struct, enum, or `List` works as a key
once it derives `Hashable` (`[derive(Hashable)]`) — two equal values are
the same key, and a freshly-built equal key finds the entry a stored one
made. Scalar keys (`i32`, `str`) still work directly. Hand-write
`impl Hashable` to key by a subset of fields, or to build your own
hash-keyed structure on the `Hash` value the trait returns.

**Decoding validates.** A generated `from_json` returns
`Result<Self, str>` and checks the shape of what it is handed — a missing
field, a wrong JSON type, an absent required value — and returns an `Err`
with a reason instead of a struct half-built from garbage. Round-tripping
your own types across the wire or through a file is safe by construction.

**A view crosses to a value explicitly.** Reading a scalar view's value
requires the `*`. `print(v)` for a `&mut i32` used to leak the view's
internal `(base, key)` representation; it now tells you to write `*v`. The
language never silently converts a view to a value — storing one where it
would escape was already an error, and this closes the read half.

**`Option<&mut T>`, built inline.** `match Some(&mut a) { Some(let v) => … }`
constructs a mutable-view option on the spot and writes through it — the
direct form, the conditional `match if c { Some(&mut x) } else { None }`,
and forwarding a `&mut` parameter. It is a transient, so it may view a
local: it never outlives the `match`. Bind it to a `let` and it escapes,
rejected as before.

**`&mut bool` writes through — and toggles.** A writable view of a boolean
now lowers like any other scalar view, so `v = true` reaches the original;
and toggling reads naturally, `v = !*v`. (The toggle also needed a lexer
fix: adjacent prefix operators like `!*`, `!!`, and `-*` were fusing into
one bogus token and failing to parse — a space was the only workaround.)

## v0.5.1 — 2026-07-14

**A type name isn't a value.** `let q = Point;` used to compile, quietly
binding the constructor object; now it's an error that points you at the
fix — construct the type, name a variant, or call a static. This also
closes a trap the v0.5.0 grammar could spring: `if p == Point { … } { … }`
(a struct-literal comparison a user meant, written without parentheses)
parsed `p == Point` against the type object and ran. It now reports
`` `Point` is a type, not a value `` at the name instead of misbehaving at
runtime. Traits, type parameters, and module names get the same check.

## v0.5.0 — 2026-07-14

**Your types order themselves.** `<` `<=` `>` `>=` now dispatch through
`PartialOrd` — implement (or derive) `partial_compare` and the
operators just work, `started < deadline` on instants included. v0.4.0
steered you to calling `lt` by hand; that detour is over.

**Platform checking follows the instantiation.** A generic function is
checked with the types each call actually binds — `save(disk_store)` in
the server entry charges `std::fs` there and only there, while
`save(memory_store)` in the browser entry stays clean. Before, one
colored instantiation could taint every use of the generic.

**Boundaries you can declare: `[platform("browser")]`.** Inference
still colors everything; a fence turns intent into a checked promise —
verified on every compile, for every host the pattern names, libraries
included. Reach outside it and the error renders the chain from the
fenced function.

**Struct literals are operands.** `Point { x = 1, y = 2 } == p`
compares and `Rect { .. }.area()` chains — no more binding to a local
first. Conditions keep the brace for the block (`if Foo { … }` stays a
condition and a block), so a literal in a condition is parenthesized:
`if p == (Point { x = 1 }) { … }`.

**A local module may share a std name.** `pkg::ui` is always your
`ui.vl`; `std::ui` is always std's. Resolution is scoped by the import
root, so naming a module `ui`, `json`, or `io` no longer collides with
— or silently loses to — the standard library. (`pkg::` also no longer
accidentally aliases std modules you never wrote.)

**Hover tells the whole story.** The editor now renders the full
declaration — signature with parameter names, generics with their
bounds, struct and enum bodies, an `async` prefix when inference adds
one — plus the `//` doc block above the item, its `[platform]` fence,
and the inferred platform requirement with its via-chain.

Also fixed and improved:

- Impl binders: a `type T` binder impl declared before the subject's
  other impls no longer misresolves, and binders in trait-argument
  position (`impl X with Wire<type F>`) register and dispatch.

## v0.4.0 — 2026-07-14

**Platform checking moved from imports to reach.** A build may import
any module; what's checked is what your entry can actually *run into*.
Every function — and now every module-level `let` — carries an inferred
platform requirement, and a browser build that reaches `std::fs` fails
with the whole call chain (`main → boot → load → exists (std::fs)`),
anchored at your call site. Since imports stopped being the boundary, a
service can live next to its resources — the database, the filesystem —
and the client imports the generated stub from that very module; the
injected-closure ceremony is gone. The editor shows all of it live:
violations as you type, and hover tells you what a function requires
and via which path it got there.

**One package, many entries.** A client + server app no longer needs
three packages. Declare two entries in one `[package]` —

```toml
[entry.client]
target = "browser"

[entry.server]
```

— and `vilan build` compiles each for its own target into
`dist/<name>.js` (browser bundles first, so a serving entry finds them
fresh), `vilan run` starts the node entry, and `vilan check` checks
them all. Packages can also depend on each other by path, so the
multi-package shape still scales when you want it. The legacy
`[server]`/`[client]` manifest form is retired; the error names the
replacement. The docs walkthrough app is rewritten in the
single-package shape — its service holds its database directly.

**Module initializers are honest.** A top-level `let` runs iff
something reachable references it — the same rule emission uses — so a
dropped binding's callees (and their `import … from "node:…"` lines,
which previously leaked into every browser bundle and broke it at
module parse) never emit. And an initializer that calls an async
function is now a clean compile error instead of a value that is
secretly a promise.

**Comparisons type-check.** `true < 3`, `1 == "a"`, and mixed-width
typed operands used to compile into coercing JS comparisons; they are
errors now. A bare integer literal still adapts to its peer
(`stamp < 1000` stays fine on an `i53`). Ordering a user-defined type
errors honestly — `PartialOrd`'s operator dispatch isn't wired yet, so
the compiler steers you to its `lt`/`le`/`gt`/`ge` methods rather than
emitting a JS object comparison that is always `false`.

**Tuples have positional access.** `pair.0`, `pair.1`, chains like
`nested.0.1`, and assignment through `mut` bindings — all over the
tuple's flat storage, so a nested write mutates the tuple, never a
copy. Destructuring is no longer the only way in.

Also fixed and improved:

- Iterator-protocol `next()` calls, indexing subjects, destructuring
  subjects, and functions passed as values are now all visible to
  platform checking and async inference — each was a blind spot that
  could hide a platform requirement or an await.
- Two build units writing the same `dist/<name>.js` are rejected at
  build instead of silently overwriting each other.
- `vilan upgrade` prunes stale materialized-std cache directories after
  a successful swap.
- `[macro]` in a manifest no longer warns as an unknown key.
- `std::time`'s documented instant comparison was wrong at runtime
  (`started < deadline` always produced `false`); the docs now use
  `lt` and the compiler rejects the old form.

## v0.3.0 — 2026-07-13

**The toolchain updates itself.** `vilan upgrade` finds the newest
release, verifies its checksum, proves the downloaded binary runs, and
swaps `vilan` and `vilan-lsp` in place; `vilan upgrade --check` only
reports. This is the CLI's one network touchpoint, and it runs only
when you ask. (v0.2.0 installs predate the command — re-run the install
script once to pick it up; it updates in place.)

**Rpc handlers can await.** An `[rpc]` method body can now call
`sleep_for`, another service, or any async API. The reply is sent when
the body finishes, and the wire turn holds across the awaits — signal
writes before and after a suspension still reach every client as one
coalesced update beside the reply.

Also fixed and improved:

- No-argument `[rpc]` methods previously ran outside the wire turn, so
  each of their signal writes was broadcast as its own update. They now
  batch exactly like argument-taking methods.
- The VS Code extension finds the language server in `~/.vilan/bin`, so
  a `vilan upgrade` reaches the editor with no extra step.

## v0.2.0 — 2026-07-13

The first public release.

**The toolchain is self-contained.** The `vilan` binary carries the
standard library inside it and materializes it on first use — download
one file (plus `vilan-lsp` beside it) and `vilan run hello.vl` works
from any directory, with no checkout and no configuration.
`vilan --version` reports the exact build.

**What's in the box:**

- The language: value semantics (assignment copies), no `null` and no
  exceptions (`Option`/`Result` with `!` and `?.`), implicit `await`,
  second-class views with compile-time invalidation checks, generics,
  traits, enums with payloads, pattern matching, and a macro system.
- `std`: collections, strings, sized numerics (`i8`–`u53`, `f32`/`f64`),
  json, time, random, crypto/jwt/base64, fetch, fs/http/process (node),
  dom/storage (browser) — platform-layered, checked at compile time.
- Fine-grained reactive UI (`std::reactive`, `std::ui`): signals bind to
  individual DOM properties; no virtual DOM; automatic cleanup; a typed
  enum-based router; compile-time styling.
- The service layer: one struct is the client/server contract —
  `[expose]`d signals mirror live to every client, `[rpc]` methods are
  typed calls, the wire contract is hashed and checked at connect, and
  reconnects resync automatically.
- The tools: `vilan build / check / run / fmt / test` (all with
  `--watch`), a language server (diagnostics, hover, go-to-definition,
  references, rename — into `std` too), and a VS Code extension,
  prebuilt as a `.vsix` on every release.
- The book: a JS/TS-developer-first guide from
  [Coming from JavaScript](https://vilan-lang.org/docs/tour/coming-from-javascript.html)
  through a full-stack walkthrough app, plus a language spec — every
  example compiled by CI.

Install:

```sh
curl -fsSL https://github.com/vilan-lang/vilan/releases/latest/download/install.sh | sh
```
