# The std surface audit — the ranked gap list (I4)

> **Status: v1 SHIPPED, 2026-08-03** (audit drafted 2026-08-03; user
> request 2026-08-03, `backlog-2026-07-18.md` §I.4). Every row §3's table
> marks v1 — rows 1–9 — is implemented, pinned, and documented. §7 below
> records what shipped where, what the implementation decided that the
> audit left open, and the two findings the implementation turned up.
> The audit body (§0–§6) is left as written: it is the record of what was
> true before the change, and §7 states every place the implementation
> diverged from it.
>
> Every method count in §1 was read from `vilan/std/src/*.vl` at audit
> time, not carried over from the backlog entry (which drifted on one
> point — see §1.1). Every demand citation is a `file:line` in the tree
> as of the audit; the diagnostic repro in §5 was run against a debug
> build of this worktree, not assumed from the bug report.

## 0. The thesis

The backlog's framing is right in outline (std's collection/string
surface is thinner than testers expect, and `42.to_string()` failing is
a discoverability bug wearing a missing-feature costume) but wrong on
one headline fact: `List` is not stuck at the eleven map/filter/fold
methods — `first`/`last`/`get` already shipped (§1.1) and are pinned in
`test/list-get-pop.vl`. The real gap list is narrower than advertised,
and the corpus sweep (§2) shows the demand concentrates hard on one
method (`join`, reinvented independently at least three times) and one
pattern (a predicate-based `find`/`contains`, reached for four separate
times across std and the examples) — not spread evenly across the
eight names the backlog lists. This changes the ranking, not the
verdict: build the ranked list below, land the eager forms now (none of
them wait on I3), fix the `to_string()` diagnostic first because it is
the cheapest, highest-leverage item on the whole list.

## 1. Current surface — verified against source

### 1.1 `List<T>` (`std/src/list.vl`, `std/src/option.vl:274-288`)

`list.vl` alone has the backlog's eleven:

```vilan,fragment
impl List<type T> {
	fun new(): List<T>
	fun push(&mut self, item: T): void
	fun pop(&mut self): Option<T>
	fun len(self): i32
	fun is_empty(self): bool
	fun map<U>(self, fn: |T| U): List<U>
	fun filter(self, predicate: |T| bool): List<T>
	fun fold<B>(self, init: B, fn: |B, T| B): B
	fun for_each(self, fn: |T| void): void
}
impl List<type T: Add + Default> { fun sum(self): T }
impl List<type T: Mul + Default> { fun product(self): T }
```

But `option.vl:274-288` adds a second `impl List<type T>` block — kept
out of `list.vl` deliberately, so the dependency-free core module stays
off the `Option` chain (the file's own comment says so) — that the
backlog entry missed:

```vilan,fragment
impl List<type T> {
	fun get(self, index: i32): Option<T>
	fun pop(&mut self): Option<T>       // redeclared, see note below
	fun first(self): Option<T>          // self.get(0)
	fun last(self): Option<T>           // self.get(len - 1)
}
```

**Fourteen distinct methods exist today, not eleven.** `get`/`first`/
`last` are real, `Option`-returning, and pinned:
`test/list-get-pop.vl:9-17` exercises all three plus the empty-list
`None` cases. **Correction to the backlog: "no first/last" is false.**
The entry's own semantics note ("first/last returning Option") is
already satisfied — nothing to design there.

One hygiene note, not a gap: `pop` is declared twice — once in
`list.vl:17` (documented as the compiler's native-`.pop` intrinsic),
once again in `option.vl:277`, identical signature. Both are `external`
so both lower to the same intrinsic; harmless, but redundant and worth
a one-line cleanup whenever this area is next touched. Not part of the
ranked list below.

**Confirmed absent** (checked by grep across `list.vl`/`option.vl`,
confirmed by attempting each in a repro build, §5's method): `reverse`,
`sort`/`sort_by`, `join`, `contains`, `index_of`, `find`, `insert`,
`remove`, slicing. `List` also does not implement `PartialEq` (no
`==` between two lists) or `Iterable` (the file's own `DEFERRED` comment
at `list.vl:97-100` — `for`/`map`/`filter`/`fold` work via native
`for...of`, but nothing implements the `Iterator<T>` protocol yet, which
matters for I3, see §4).

**Why `get`/`first`/`last` need no import today, and `to_string` does.**
This is the same mechanism behind the diagnostic in §5, worth stating
once: the analyzer always loads a small fixed set of core std modules
regardless of what the user imports —
`["boolean", "list", "null", "promise", "compare", "default", "debug",
"json", "hash"]` (`crates/vilan-core/src/analyzer.rs:24901-24903`, the
comment: *"`bool`, `List`, and `null` are core primitives, so their
(dependency-free) modules are always loaded even when not imported"*)
— plus everything transitively reachable from those files' own
`import pkg::...` lines. `list.vl:6` imports `pkg::option::Option`
(needed for its own `pop` signature), which pulls the *entire*
`option.vl` file in, including the unrelated `impl List<type T> { get,
first, last }` block riding along in the same file. `display.vl` is
reachable from none of the always-loaded set or their transitive
imports, so it loads only when a program imports something from it —
confirmed empirically in §5. The lesson generalizes: any std method's
discoverability today is an accident of which file it lives in and
what that file's neighbors import, not a designed boundary.

### 1.2 `Map<K, V>` (`std/src/map.vl`) and `Set<T>` (`std/src/set.vl`)

```vilan,fragment
impl Map<type K: Hashable, type V> {
	fun new(): Map<K, V>
	fun insert(&mut self, key: K, value: V): void
	fun get(self, key: K): Option<V>
	fun contains_key(self, key: K): bool
	fun remove(&mut self, key: K): void
	fun len(self): i32
	fun is_empty(self): bool
	fun keys(self): List<K>
	fun values(self): List<V>
}
impl Set<type T: Hashable> {
	fun new(): Set<T>
	fun insert(&mut self, value: T): void
	fun contains(self, value: T): bool
	fun remove(&mut self, value: T): void
	fun len(self): i32
	fun is_empty(self): bool
	fun values(self): List<T>
}
```

Nine methods on `Map`, seven on `Set` — matches `docs/std/collections.md`
exactly (the docs page is current for these two types). Absent:
`entries()` (paired key/value iteration — `keys()`/`values()` are two
separate `List` snapshots the caller has to zip by hand, and `List` has
no `zip`, see §4), `map`/`filter`/`for_each` on either, `contains_value`
on `Map`, `union`/`intersection`/`difference` on `Set`. **No corpus
evidence of demand for any of these** — see §2.4. They are real
structural gaps, not ranked gaps; §3 explains why they sit outside v1.

### 1.3 `str` (`std/src/string.vl`, `std/src/option.vl:259-269`,
`std/src/compare.vl:71-99`, `std/src/hash.vl:29-33`,
`std/src/default.vl:8-12`)

```vilan,fragment
impl str {
	fun len(self): i32
	fun is_empty(self): bool
	fun trim(self): str
	fun to_uppercase(self): str
	fun to_lowercase_ascii(self): str
	fun contains(self, needle: str): bool
	fun starts_with(self, prefix: str): bool
	fun ends_with(self, suffix: str): bool
	fun replace(self, from: str, to: str): str
	fun repeat(self, count: i32): str
	fun split(self, separator: str): List<str>
	fun substring(self, start: i32, end: i32): str
	fun code_at(self, index: i32): u32
	fun parse_i32(self): Option<i32>
	fun parse_f64(self): Option<f64>
}
```

Fifteen methods, plus `Add` (`+`), `Eq`/`PartialOrd`/`Ord` (lexicographic,
native `<`), `Default` (`""`), `Hashable`, `Display` (identity), `Json`.
**Every name the backlog's charter singled out for inventory —
`split`, `trim`, `starts_with`, `replace` — already exists.** The corpus
sweep (§2) found zero demand for `trim_start`/`trim_end`,
`to_lowercase` (full-Unicode; only the documented `_ascii` form exists),
`chars()`/iteration, or a substring-position `find`. **`str` is not a
ranked gap.** The one string-shaped want the sweep surfaced — joining a
`List<str>` back together — is a `List` method, not a `str` one (§2.1).

### 1.4 Numeric types (`std/src/number.vl`, `std/src/compare.vl`,
`std/src/math.vl`)

Every sized type (`i8 i16 i32 i53 u8 u16 u32 u53 f32 f64 BigInt`) has,
per type: `abs`, `pow`, `min`, `max` (all `external`, `Math.min`/
`Math.max`-backed on the integer types and `f32`/`f64`), `rem`
(the `%` operator's method form), the full `as_*` conversion family, and
`i32`/`u32` add `is_even`/`is_odd`, `i32` adds `diff`. Floats add
`sqrt floor ceil round trunc sin cos tan asin acos atan atan2 exp ln
log2 log10 cbrt hypot sign fract lerp to_radians to_degrees is_nan
is_finite is_infinite`.

**`clamp` exists — but only as `Ord`'s trait default**
(`compare.vl:58-60`, `self.min(max).max(min)`), and every *integer* type
implements `Ord` (`number.vl:207-475`). **`f64` and `f32` deliberately
do not** — `number.vl:477-517`'s comment states why (`NaN` breaks a
total order) — so **`f64`/`f32` have no `.clamp()`**, confirmed by
reading the impl blocks, not assumed. `math.vl`'s free functions
(`min`/`max`/`minmax<T: Ord>`) inherit the same floats-excluded
boundary. No "Compare" trait exists anywhere in std (grepped every
`trait` declaration in `std/src/*.vl`) — the ordering machinery is
`PartialEq`/`Eq`/`PartialOrd`/`Ord` (`compare.vl`), Rust-shaped, and
that is what any `sort` comparator bound should read against. Per-type
`MIN`/`MAX` constants are **not** part of this audit's scope — the
backlog already tracks them separately as a recorded proposal tail
("want a static-member design", `backlog-2026-07-18.md`'s closing
paragraph) blocked on a different mechanism.

## 2. What real code reaches for — demand evidence

Swept: `vilan/examples/**/*.vl` (browser, fullstack, math, reactive-ui,
router, rpc, ssr, todo, walkthrough), `vilan/docs/**/*.md` fenced
blocks, `vilan/test/**/*.vl`, `vilan/std/src/**/*.vl` (including
`browser/`, `process/`), and `vilan/macro_std/src/*.vl`. A first bare-call
sanity pass (`grep -rn '\.sort(\|\.reverse(\|\.join(\|\.index_of('`)
found **zero** hits anywhere — nothing in the corpus assumes these
methods exist, so there is no compile-broken example to flag; the
demand below is entirely hand-rolled-workaround evidence, not misuse.

### 2.1 `join` — the strongest signal, reinvented three times

- `macro_std/src/build.vl:25-37` — `export fun join(parts: List<str>,
  separator: str): str` with the `mut joined = ""; mut first = true;
  for part in parts { if first { first = false } else { joined = joined
  + separator } joined = joined + part } joined` shape. The project
  already wrote `List<str>.join` once — as a macro-expansion-only helper,
  unreachable from ordinary runtime code.
- `std/src/json.vl:34-45` (`impl List<type T: Json> with Json { fun
  to_json }`) — **inside std itself, at runtime**: `mut result = "[";
  mut first = true; for element in self { if !first { result = result +
  "," } result = result + element.to_json(); first = false } result +
  "]"`. Textbook `list.map(|e| e.to_json()).join(",")`.
- `std/src/style.vl:591-602` (`Style::class_list`) — space-joins a
  `Map.values()` list of class names with the same `out == ""` first-item
  guard: `if out == "" { out = class } else { out = out + " " + class }`.
- `std/src/rpc.vl:1474-1489` — the service-contract-hash builder
  comma-joins parameter type renderings by hand, without routing through
  `macro_std::build::join` even though that helper is reachable in that
  file's macro world.
- `macro_std/src/meta.vl:26-34` (`TypeExpr::render`) — a *third*
  independent comma-join, this time for rendering generic type arguments
  (`i"{self.name}<"` … `first`/`else` … `+ ">"`).
- `docs/tour/macros-and-const.md`'s `derive_display` example and its
  pinned twin `test/macro-derive.vl:21-30` build a `" + \", \" + "`
  field-joining expression by hand, the same shape once more.

Six independent sites, three of them inside std's own implementation,
all the identical `mut first = true` shape. This is the clearest single
piece of evidence in the whole sweep.

### 2.2 `find` / predicate-based `contains` — four sites, all predicate-shaped

- `examples/walkthrough/src/views.vl:191-198` (`note_page`) — `mut
  found: Option<Note> = None; for note in list { if note.id == note_id {
  found = Some(note) } } found`. No short-circuit. Exactly
  `list.find(|n| n.id == note_id)`.
- `examples/todo/src/store.vl:69-74` (`TodoStore::remove`, an `[rpc]`
  method) — `mut found = false; for todo in self.todos.get() { if
  todo.id == id { found = true } }`, used purely to compute the RPC's
  boolean return. Exactly `list.any(|t| t.id == id)` (or
  `list.find(..).is_some()`).
- `std/src/process/ui.vl:76-91` (`set_attribute`, the SSR `View` twin of
  `Element.setAttribute`) — a hand-rolled "find-by-name, replace in
  place, append if absent" upsert: `mut found = false; for attribute in
  attributes.read() { if attribute.name == name { updated.push(new);
  found = true } else { updated.push(attribute) } } if !found {
  updated.push(new) }`. The browser twin (`browser/ui.vl:130` and
  neighbors) sidesteps this by delegating to the real DOM, so the
  workaround is unique to the process/SSR leg, which has no such escape
  hatch.
- `std/src/reactive.vl:507-521` (`reconcile`, the engine behind
  `bind_each`'s keyed diff — runs on every reactive list update): a
  hand-rolled linear search for the first unclaimed index matching a key,
  with an extra "already claimed" side condition layered on top of the
  loop.

**None of these is element-equality `contains(x: T)`** — the charter's
named gap. Every real site needs a *predicate*, not a value. A
`PartialEq`-bound `contains`/`index_of` (the charter's ask) would satisfy
none of these four call sites as written; a predicate-based `find`
would satisfy three of the four directly (`views.vl`, `store.vl`,
`process/ui.vl`'s search half) and materially shrink the fourth. Both
are worth shipping — see §3's ranking, which puts `find` above the
charter's own `contains`/`index_of` for exactly this reason.

### 2.3 `reverse` — two sites, one file

- `std/src/json.vl:580-589` (`JsonReader::begin_list`) and
  `std/src/json.vl:614-619` (`begin_variant`, the arity>1 branch) — both
  read: `mut index = elements.len() - 1; for index >= 0 {
  self.stack.write().push(elements[index]); index -= 1; }`. The
  function's own doc comment (`json.vl:475-478`) names the intent:
  *"replace their aggregate with its elements (reversed, so pops come
  off in order)"*. Textbook `for element in elements.reverse() {
  stack.push(element) }`, hand-rolled twice in the same file because
  there is nothing to call.

### 2.4 `sort`, `insert`/`remove`, Map/Set extras, `clamp` — no corpus demand

- **`sort`**: zero hits — no comparison-swap loop, no bare `.sort(`
  call, nothing. The corpus has never needed to order a `List`. Real
  (§1.1 confirms the method is absent) but not corpus-evidenced; ranked
  on structural completeness, not observed pain.
- **`insert`/`remove` (positional)**: zero hits. Every "remove from a
  `List`" need in the corpus goes through the existing `filter`
  (`examples/reactive-ui/todos.vl:153-162`,
  `examples/walkthrough/src/store.vl:139-146`,
  `examples/todo/src/store.vl:76-83`); every "insert" need is a `push`
  at the end. No site wants a positional splice.
- **Map/Set extras** (`entries`, `map`/`filter`/`for_each`,
  `contains_value`, `union`/`intersection`/`difference`): zero hits.
  Every Map/Set use in the corpus is `insert`/`get`/`contains_key`/
  `remove`/a bare `for` loop.
- **`f64`/`f32.clamp()`**: zero hand-rolled if-chains. Every clamp-shaped
  need in the corpus (`test/number-math.vl:11-12,21-22`,
  `test/math.vl:50-51`) is satisfied by the existing `.min(hi).max(lo)`
  two-call spelling — which is also exactly what `Ord::clamp` itself
  does (`compare.vl:59`), so the float gap is real but cheap enough to
  not have forced anyone's hand yet.

### 2.5 Adjacent, not ranked: `enumerate`/`zip`

`std/src/browser/ui.vl:298-317` (`bind_each`) manually walks two lists
in lockstep with a hand-incremented `position` counter — the shape
`zip`/`enumerate` exist to remove. This is I3's territory by name (the
backlog's iterator-adapters entry lists `zip`/`enumerate` as adapter
candidates); flagged here as evidence, not claimed — see §4.

## 3. The ranked gap list

Per-method semantics settled against the precedent std already has
(§1), ordered by demand strength (§2) tempered by effort. **S** = pure
`.vl`, no compiler change, shaped exactly like `sum`/`product`
(`list.vl:65-95`) or `parse_i32`/`get` (`option.vl`) — a new `impl`
block, no transformer/interpreter/analyzer work. **M** = needs a
semantics decision this document makes plus either a new intrinsic
(transformer + interpreter, gated like `push`/`pop`) or an
analyzer-side mechanism (the diagnostic).

| # | Gap | Bound / signature | Size | Evidence |
|---|-----|--------------------|------|----------|
| 1 | **The `to_string()` steering diagnostic** | analyzer-only, §5 | M | The charter's own bug; reproduced live |
| 2 | **`List<T: Display>.join(self, separator: str): str`** | `Display` (str's own impl is identity, so `List<str>.join` costs nothing extra) | S | §2.1, 6 sites |
| 3 | **`List<T>.find(self, predicate: \|T\| bool): Option<T>`** | none | S | §2.2, 3 direct sites |
| 4 | **`List<T: PartialEq>.contains(self, value: T): bool`** | `PartialEq` | S | Charter-named; §2.2's 4th site is a near-fit once `find` exists (`list.find(p).is_some()`) |
| 5 | **`List<T: PartialEq>.index_of(self, value: T): Option<i32>`** | `PartialEq` | S | Charter-named; same family as `contains` |
| 6 | **`List<T>.reverse(self): List<T>`** | none | S | §2.3, 2 sites, one doc comment naming it |
| 7 | **`f64`/`f32.clamp(self, min, max)`** | none (no `Ord` needed — `self.min(max).max(min)`, same recipe `Ord::clamp` uses) | S | §2.4, no direct demand, trivial fix, charter-named |
| 8 | **`List<T: Ord>.sort(self): List<T>` + `List<T>.sort_by(self, compare: \|T, T\| Ordering): List<T>`** | `Ord` / none | M | §2.4, zero demand, charter-named, needs §3.1's decision |
| 9 | **`List<T>.insert(&mut self, index: i32, value: T): void` + `remove(&mut self, index: i32): T`** | none | M | §2.4, zero demand, charter-named, needs §3.2's decision |
| — | Slicing | wants a range type | deferred | Charter defers explicitly to I2 (`backlog-2026-07-18.md` §I.2: "slicing wants a range type") |
| — | `Map`/`Set` parity (`entries`, `map`/`filter`, `union`/`intersection`) | — | not ranked | §2.4, zero demand; revisit if a future sweep finds any |
| — | `enumerate`/`zip` on `List` | — | I3's territory | §2.5; flagged, not decided (§4) |

### 3.1 `sort`'s comparator form and stability

std's ordering machinery is `Ord::compare(self, b: Self): Ordering`
(§1.4) — no separate `Compare` trait, so the natural bound is `T: Ord`,
matching `math::min`/`max`/`minmax`'s existing `<T: Ord>` precedent
(`math.vl:30-45`). Proposed split, mirroring Rust's own `sort`/`sort_by`
pair:

- `sort_by(self, compare: |T, T| Ordering): List<T>` — the one real
  primitive, no trait bound (works on anything, including types with no
  `Ord` impl — e.g. sorting by a derived key). Should be `external`,
  lowering to `[...self].sort((a, b) => compare(a, b))` — a genuine new
  intrinsic (transformer + interpreter, gated like `pop`/`push`'s
  existing native lowering), not pure `.vl`: reimplementing a sort
  algorithm in `.vl` would both cost more code and, unless written very
  carefully, risk **not** being stable.
- `sort(self): List<T>` where `T: Ord` — a one-line pure-`.vl` wrapper:
  `self.sort_by(|a, b| a.compare(b))`. No second intrinsic needed.

**Stability is a hard requirement, and it is free**: ECMA-262 has
guaranteed `Array.prototype.sort` stable since ES2019, so lowering
directly to the native `.sort()` gets stability for the cost of a
regression test pinning it (two equal-key elements, distinct secondary
data, assert original relative order survives) rather than an algorithm
choice.

Eager, returns a new `List` (not mutate-in-place): consistent with
`map`/`filter`/`fold` (§1.1), which are all `self`-by-value already —
`sort`/`sort_by` following the same convention keeps `List`'s pure
methods uniformly pure and its `&mut self` methods (`push`/`pop`/
`insert`/`remove`) uniformly mutating, a clean line to hold.

**Naming sidesteps I3 cleanly**: I3's future lazy adapter is named
`rev` (`backlog-2026-07-18.md` §I.3: *"`rev` needs a double-ended story
… belongs in the same paper"*), not `reverse`. Rust's own `Vec` carries
both an in-place `.reverse()` and a lazy `Iterator::rev()` side by
side with zero conflict — the same split applies here for free. `sort`/
`sort_by` have no lazy-adapter twin named in I3 at all, so no tension to
flag.

### 3.2 `insert`/`remove`'s index semantics

Precedent already in std, from two different angles:

- The `[]` index operator **panics** out of bounds — verified live:
  `xs[10]` on a 3-element list throws `index out of bounds: the length
  is 3 but the index is 10` at runtime (`transformer.rs`/
  `interpreter.rs`'s shared message; reproduced with a debug build).
- `.get(index)` is the **safe, `Option`-returning** alternative
  (`option.vl:275`) for exactly the callers who don't want the panic.

`remove`/`insert` should follow the `[]`/panic convention, not `get`'s:
an out-of-range index passed to `remove`/`insert` is a caller bug, the
same class of bug `[]` already punishes, and `remove` in particular
returns `T` (the removed element), not `Option<T>` — unlike `pop`,
where "the list was empty" is a normal, expected condition worth an
`Option`, an arbitrary bad *index* is not. Proposed:

- `remove(&mut self, index: i32): T` — panics (`index out of bounds:
  the length is {len} but the index is {index}`, reusing `[]`'s exact
  wording) when `index >= len` or `index < 0`.
- `insert(&mut self, index: i32, value: T): void` — panics under the
  same rule, except `index == len` is legal (append via `insert`,
  mirroring Rust's `Vec::insert`).

Both are expressible as pure `.vl` without a new intrinsic — shift the
tail through the existing `push`/`pop`, as `List` already has both — but
that is an O(n) pop/push shuffle per call; whether that is acceptable or
this pair should also get a native `splice`-backed intrinsic (matching
`sort_by`'s reasoning) is exactly the kind of call the implementation
slice should make with a benchmark, not this audit. Recorded as the
open question the M-size tag reflects.

## 4. The v1 cut line and the I3 tension

**V1 (land now, none of it waits on I3's adapter paper):** everything in
§3's table rows 1–9. The charter's own constraint is explicit —
*"eager `List` forms land now and do not wait on I3's adapter paper (I3
subsumes them lazily later)"* — and nothing above needs the adapter
layer: `join`/`find`/`contains`/`index_of`/`reverse`/`sort`/`insert`/
`remove` are all terminal, eager operations on a concrete `List`, exactly
like the eleven (now fourteen) that already exist.

**Deferred, not decided — flagged for I3 to resolve:**

- **`enumerate`/`zip`** (§2.5) — real demand (`browser/ui.vl:298-317`),
  but these are named directly in I3's backlog entry as adapter
  candidates. Adding an eager `List.zip`/`List.enumerate` now would
  either collide with I3's naming or force I3 to pick different names
  later for what should be the obvious ones. Left to I3.
- **`take`/`skip`** — not evidenced anywhere in this sweep (no corpus
  site reaches for "first N elements of a `List`"), but I3 names them
  explicitly as lazy-adapter candidates. If eager demand for "first N"
  appears before I3 ships, the naming precedent set by `reverse`/`rev`
  in §3.1 generalizes: give the eager form a different name (e.g.
  `take_n`) rather than pre-empting `take` for the eager case. Recorded,
  not decided — no such demand exists yet to force the question.
- **`List` gaining `Iterable<T>`** — a prerequisite I3 needs regardless
  of adapter naming (`list.vl:97-100`'s own `DEFERRED` comment: adapters
  defined "ON `Iterable`" per the backlog's design direction can't reach
  `List` until `List` implements the protocol, which needs a concrete
  `ListIterator<T>` plus mutable iterator state — separate design work,
  outside I4's scope, called out here only because it blocks I3
  regardless of what I4 ships).

## 5. The steering diagnostic

### 5.1 The bug, reproduced

```vilan
fun main() {
    let x = 42;
    print(x.to_string());
}
```

Compiled against this worktree's debug binary:

```
Error: i32 has no method 'to_string'
   ╭─[ main.vl:3:13 ]
   │
 3 │     print(x.to_string());
   │             ────┬────
   │                 ╰────── i32 has no method 'to_string'
───╯
```

Adding `import std::display::Display;` (`impl i32 with Display` lives in
`display.vl:22-26`) makes it compile clean. §1.1 already explains the
mechanism: `display.vl` sits outside the always-loaded core set and
outside that set's transitive `import pkg::...` closure, so nothing
loads it — and therefore nothing registers `i32`'s `to_string` into
`self.implementations` — until a program names it directly.

### 5.2 The emission site

`crates/vilan-core/src/analyzer.rs`, `resolve_method_call`'s
`MethodLookup::NoMethod` arm (the general "no such method" path;
line numbers below are this snapshot's, they will drift):

```rust
MethodLookup::NoMethod => {
    let type_str = self.pretty_print_type(&subject_type, &HashMap::new());
    let trait_only_note = self
        .trait_only_provider(&subject_type, member_name)
        .map(|trait_name| format!(
            "; it is `[trait_only]` on trait `{trait_name}`: reach it through \
             a `{trait_name}` bound, not the concrete type"
        ))
        .unwrap_or_default();
    let field_steer = self
        .same_named_field_steer(&subject_type, member_name)
        .unwrap_or_default();
    self.diagnostics.push(Error {
        note: None,
        span: self.member_name_spans.get(&id).copied().unwrap_or(arguments_span),
        msg: format!("{} has no method '{}'{}{}", type_str, member_name, trait_only_note, field_steer),
    });
    ...
}
```

(`trait_only_provider` at `analyzer.rs:7721`, `same_named_field_steer`
at `analyzer.rs:7745` — this diagnostic already has two precedent
"append a steer if a helper finds one" hooks; ours is a third.) This is
also, per the diagnostics ledger, an already-audited, QUALIFIES-verdicted
site (`diagnostics-ledger.md` rows 72/74, "RE-ANCHORED to the method
name (batch 3)") — the anchor (A1: the method-name span) is settled and
correct; only the appended text changes, which is why this reads as a
`msg`-only addition, not a re-anchor.

There is a second, narrower "no method" site for fixed-length arrays
(`analyzer.rs:18026-18043`, `[T; n]` has exactly one method, `len`) —
out of scope: arrays are structural, not nominal, so "import the trait
that provides it" cannot apply there.

### 5.3 The mechanism: extend the lazy std-wide index to method names

The exact machinery this needs already exists for *names* — it just
doesn't look inside `impl` blocks yet. `import_steer`
(`analyzer.rs:19044-19076`) lazily builds `std_export_index: Option<
HashMap<String, String>>` by scanning every std file the program never
loaded (via `self.std_module_files`, populated eagerly as a cheap
directory walk at `analyzer.rs:24708-24732`) through
`load_package_module` + `collect_declared_names`
(`analyzer.rs:1593-1614`). `collect_declared_names` walks top-level
`Node::Func`/`Node::Struct`/`Node::Enum`/`Node::Trait`/`Node::Let` — it
has no `Node::Impl` arm, so a method declared *inside* an `impl Type
with Trait { fun name }` block is invisible to the existing index. That
is the one gap to close.

Proposed: a sibling index, built in the same lazy pass (same file scan,
no new I/O), keyed differently:

```
std_trait_method_index: Option<HashMap<(SubjectHead, MethodName), (TraitName, ModuleName)>>
```

built by a new walk alongside `collect_declared_names` that adds a
`Node::Impl(subject, traits, body)` arm: for each `fun` in `body`,
record `(subject_head(subject), method_name) -> (trait_name, module_name)`
for each trait the impl declares (skip bodyless/inherent impls — those
have no trait to name in the hint). `subject_head` is a light syntactic
reduction, not a type-system lookup (the module isn't loaded, so there
is no `Type` to compare against) — strip generic arguments and take the
leading identifier: `i32` → `i32`, `List<type T>` → `List`,
`Option<type T>` → `Option`. Every hand-written std impl subject is one
of these two shapes (a bare primitive name or one generic head), so this
covers the real surface without needing full unification.

At the `MethodLookup::NoMethod` site, a third helper alongside
`trait_only_provider`/`same_named_field_steer`:

```rust
fn unimported_trait_method_steer(&mut self, subject_type: &Type, member_name: &str) -> Option<String> {
    self.build_std_trait_method_index_if_needed();
    let head = self.subject_head(subject_type); // "i32", "List", ...
    let (trait_name, module_name) = self.std_trait_method_index.as_ref()?
        .get(&(head, member_name.to_string()))?;
    Some(format!(
        "; import std::{module_name}::{trait_name} to use it (`import std::{module_name}::{trait_name};`)"
    ))
}
```

appended as a fourth `{}` in the existing `format!("{} has no method
'{}'{}{}{}", ...)` — all independent steers concatenate today
(`trait_only_note` and `field_steer` already do), so this is additive,
not a branch.

### 5.4 The message text

```
i32 has no method 'to_string'; import std::display::Display to use it (`import std::display::Display;`)
```

Checked against `diagnostics-standard.md`:

- **B1** (user vocabulary only) — `trait_name`/`module_name`/
  `member_name` are all user-facing spellings; nothing internal leaks.
- **B4** (steer when the fix is unambiguous, code-shaped when short) —
  one action, the exact `import` line quoted verbatim, matching
  `import_steer_inner`'s own `"; import it first (\`import {root}::
  {module}::{name};\`)"` shape (`analyzer.rs:19110-19119`) precisely,
  so this reads as the same diagnostic family as the existing import
  steers, not a new dialect.
- **A2** (user code only — never anchor in std) — the *span* stays the
  method-name span in the user's file (unchanged from today's
  RE-ANCHORED verdict); the hint *names* `std::display` in prose but
  does not add a secondary span pointing into std source, so no C3 note
  is needed and no rule is broken.
- **C2** (the pin is the qualification) — needs an
  `assert_fails_spanning` pin exercising the exact repro in §5.1 (span:
  `to_string` in `x.to_string()`; message fragment: `import
  std::display::Display to use it`), plus a **negative** pin: the same
  call with `Display` already imported must NOT carry the hint (steers
  must not survive past the fix, matching B5's "no repetition" spirit).

This should be its own ledger row, added to `diagnostics-ledger.md` in
the same commit that implements it (per `diagnostics-standard.md §5`'s
running-ledger rule) — this audit does not touch that file, since it
records nothing not yet built.

## 6. Dependency order and size

Four independent tracks — nothing here blocks anything else in this
list, so ordering is about risk-clustering and quickest payoff, not a
hard dependency graph:

1. **The steering diagnostic (§5, M)** — first: self-contained, touches
   only `analyzer.rs` plus the ledger, fixes the exact complaint that
   opened I4, and its own machinery (the trait-method index) needs no
   List/Map/Set decision from the rest of this document.
2. **Pure-`.vl` `List`/number additions (§3 rows 2–7, S each)** — batch
   together: `join`, `find`, `contains`, `index_of`, `reverse`,
   `f64`/`f32.clamp`. Same shape as `sum`/`product`/`get`/`first`/`last`
   already in the tree — new `impl` blocks, docs updates
   (`docs/std/collections.md`, `docs/std/numbers.md`), corpus tests, no
   transformer/interpreter/analyzer work. Lowest risk in the whole list.
3. **`sort`/`sort_by` (§3.1, M)** — needs the new `sort_by` intrinsic
   (transformer + interpreter, gated like `push`/`pop`'s existing native
   lowering) plus the stability regression pin. Touches codegen, so it
   is its own suite-gated slice per `CLAUDE.md`'s "touched the
   transformer / codegen → `cargo test -p vilan-cli --test corpus`" rule.
4. **`insert`/`remove` (§3.2, M)** — last: needs the benchmark/intrinsic
   call §3.2 leaves open (pure-`.vl` pop/push shuffle vs. a native
   `splice`-backed intrinsic) resolved before implementation starts, and
   has zero corpus urgency (§2.4) backing it, so it can safely trail the
   higher-demand items above.

Each slice ships suite-gated per `CLAUDE.md` (docs-fence test for any
std change, corpus test for anything touching codegen, a pinned
regression per new method, the ledger update in the same commit as the
diagnostic).

## 7. What shipped (implementation record, 2026-08-03)

### 7.1 Placement — the visibility story, asserted

Placement was treated as part of the contract, not an afterthought: §1.1's
lesson is that a method's discoverability is an accident of which file it
lives in, so each method went into the module that makes the common case
visible from a plain `List` import, and the resulting behaviour is
pinned rather than assumed.

| Method | File | Block | Visible without an import? |
|---|---|---|---|
| `reverse`, `insert`, `remove` | `std/src/list.vl` | the main `impl List<type T>` | yes (always-loaded core) |
| `find` | `std/src/option.vl` | the existing second `impl List<type T>` | yes (reached transitively via `list.vl`'s own `import pkg::option::Option`) |
| `sort_by` | `std/src/compare.vl` | new `impl List<type T>` (`external`) | yes (`compare` is always loaded) |
| `sort` | `std/src/compare.vl` | new `impl List<type T: Ord>` | yes |
| `contains`, `index_of` | `std/src/compare.vl` | new `impl List<type T: PartialEq>` | yes |
| `join` | `std/src/display.vl` | new `impl List<type T: Display>` | **no** — the `Display` bound strands it |
| `clamp` | `std/src/number.vl` | the existing `impl f64` and `impl f32` | yes |

`compare.vl` was chosen over `list.vl` for the four bounded/ordering
methods for the reason §1.1 gives: `list.vl` must stay off the chains its
neighbours would drag in, and an `impl` block does not need to share a
module with the type it extends (the precedent `then_some`/`parse_i32`/
`get` already set). `compare` is in the always-loaded core set, so the
placement costs nothing in visibility.

`join` is the one exception, and it is the exception the audit predicted:
its bound is `Display`, `display.vl` is outside the always-loaded set, and
moving `Display` into that set (by importing it from `list.vl`) would make
the whole steering diagnostic below unreachable while permanently widening
the always-loaded core. The steer is the mitigation, and
`the_join_miss_steers_to_the_display_import` pins that a `List`-only
program calling `.join(..)` gets told exactly what to import.

`the_std_surface_batch_needs_no_import` pins the other half: a program
whose only import is `print` reaches `reverse`, `sort`, `sort_by`,
`contains`, `index_of`, `find`, `insert`, `remove` and `clamp`.

### 7.2 Decisions the audit left open

- **`insert`/`remove` are pure `.vl`, not a `splice` intrinsic** (§3.2
  left this to "the implementation slice … with a benchmark"). They shift
  the tail through the existing subscript, guarding the index and calling
  `io::panic` with `[]`'s exact wording. Rationale: §2.4 found **zero**
  corpus demand for either, so an O(n) shuffle is not on any hot path
  worth widening the intrinsic surface for; the sanctioned compiler work
  for this slice was the `sort_by` intrinsic and the diagnostic, and
  spending more of it on an unmeasured cost would be speculative. Should a
  profile ever show it, the swap is local: an `external` declaration plus
  a `__list_splice` helper, with the pins unchanged.
- **`sort_by` IS an intrinsic**, as §3.1 specified: `Intrinsic::ListSortBy`
  → `__list_sort_by(list, compare)` → `list.slice().sort(compare)`.
  Stability comes free from ECMA-262 (stable since ES2019) and is pinned
  independently (`list_sort_by_is_stable`, `vilan/test/list-sort.vl`).
  `Ordering` is a numeric enum lowering to `-1`/`0`/`1`, which is already
  the host comparator contract, so the vilan closure passes straight
  through with no adapter. The tree-walking interpreter cannot host the
  comparator in `Vec::sort_by` (it needs `&mut self` and can fail), so it
  implements the same helper as an explicit **bottom-up merge sort**,
  stable by construction — and the corpus interpreter differential
  (`list-sort.vl`) is what keeps the two agreeing.
- **The steer needs no "is that module already loaded?" guard.** One was
  written and then removed as unreachable: once the providing module is
  loaded its `impl` blocks are registered, so the call either resolves or
  — when the impl is bounded and the bound fails — reports the *bound*, at
  the bound's own site, never reaching the `NoMethod` arm. Pinned by
  `an_unsatisfied_bound_is_reported_as_a_bound_not_as_a_steered_miss`. The
  guard was also actively wrong for a *user* module that happens to share a
  std module's name, which would have silenced a legitimate steer.
- **The steer's index keys on trait names AND inherent bounds.** §5.3
  proposed indexing `impl X with Trait` only. That is not enough: `join`
  is an *inherent* `impl List<type T: Display>` with no `with` clause, and
  it is precisely the method that needs the steer. The walk therefore
  offers the impl's trait names first and falls back to the bounds on the
  subject's own `type X` binders — and, either way, only names something
  the containing module actually declares, so the steer can never suggest
  importing a name from a module that merely mentions it. Blanket
  (`impl type T`) subjects are skipped: they have no nominal head and
  would match every type in the program.

### 7.3 The hygiene nit (§1.1), cleaned

`pop` was declared twice, identically, in `list.vl:17` and `option.vl:277`.
The `option.vl` copy is gone; `list.vl`'s — the one carrying the doc
comment that explains the intrinsic — stays. Harmless as predicted: the
intrinsic table keys on the declaration id found in *any* `impl List` block,
so one declaration is enough, and the corpus goldens (including
`list-get-pop.js`, which exercises `pop` directly) are byte-identical
across the change.

### 7.4 Two findings

- **`sort_by` inherits `map`/`filter`'s element aliasing.** A struct
  element of the list `sort_by` returns is the *same* runtime value as the
  corresponding element of the receiver, so writing through one shows in
  the other. This is **not new**, and not specific to `sort_by`: `map` and
  `filter` behave identically today (verified on the pre-change tree —
  `xs.filter(|c| true)` then writing to the result's element 0 changes
  `xs[0]`). `sort_by` was left consistent with them rather than made
  unilaterally stricter, since the root cause is one shared hole in how
  `List`'s pure methods handle value semantics, and fixing it belongs in
  that arc, not in I4. (The new `reverse` happens *not* to alias, because
  it rebuilds through `push`.) **Recorded as a real gap, deferred with
  this note** — it wants its own slice.
- **`docs/std/collections.md` had never documented `get`/`first`/`last`.**
  §1.1 corrected the backlog on their existence; the docs page had the same
  blind spot. They are in the fragment now, alongside the new methods.

### 7.5 Deferred, unchanged

`enumerate`/`zip`, `take`/`skip`, `List: Iterable<T>` and slicing are all
exactly where §4 left them — out of I4, flagged for I3 (or I2, for
slicing). Nothing in this implementation touched or pre-empted any of
their names. The `Map`/`Set` parity gaps of §1.2 remain unranked: the
sweep found no demand, and none appeared while implementing.

### 7.6 Coverage

New pins: 24 in `crates/vilan-core/tests/inference.rs` (byte-exact stdout
via `assert_compiles_and_runs`, panic text via `assert_run_panics`, span +
message via `assert_fails_spanning`), plus four corpus fixtures with
goldens — `vilan/test/list-search.vl`, `list-sort.vl`, `list-splice.vl`,
`list-join.vl` — and `clamp` appended to `vilan/test/number-math.vl`. The
corpus fixtures also carry the intrinsic through the interpreter
differential. Docs: `docs/std/collections.md` (four new compiled examples)
and `docs/std/numbers.md` (one). Ledger: row 212.
