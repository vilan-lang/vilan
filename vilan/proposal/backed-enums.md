# Backed enums — a variant carries the value the host speaks (B76)

> Status: RATIFIED 2026-08-08 (owner review) — as drafted, every settled
> recommendation standing, with §7.2 DEFERRED: an `external fun` may NOT
> return a backed enum in v1 — host boundaries keep the generated-wrapper
> / `parse()` path (where an out-of-set value honestly yields `None`)
> until backed enums grow a trap-arm story for the bare-`else` hazard
> §7.2 records. Implementation is the v0.35.0 backed-enums lane.
>
> Prior status: DRAFT (awaiting owner review)
>
> Origin: OWNER NOTE 1 on `bindgen.md` (§9.4), recorded 2026-08-06 during the
> E31 review and deliberately *not* settled inside bindgen — "record that as
> its own language question rather than deciding it inside bindgen". This is
> that question. Proposal-first per the house rules; nothing here is
> implemented, and the paper recommends nothing land in the compiler until it
> is ratified.
>
> Every claim below about what the compiler does today was checked against
> source **or run through the repo compiler** as a probe. The probes are
> called out inline (P1…P9), because four of them found defects that change
> the design — the discriminant grammar the feature would extend is not
> validated at all, and one of its holes is a live miscompile in shipping
> code (§1.7). Probes ran against `target/debug/vilan` built in this
> worktree from `next @c2b9c7c`. §7 is the open-questions set; everything
> before it is a recommendation, not a ratification.

## 0. The problem and the thesis

A vilan enum variant can carry an integer. It cannot carry a string:

```
enum Align { Start = "start", End = "end" }
        → Error: found '=' expected ',' or '}'
```

The grammar is `= (-)? integer` and nothing else (`parse_discriminant`,
`crates/vilan-core/src/parsing.rs:3307-3318`). That one gap is why
`std/src/style.vl` contains eleven hand-written functions whose entire job is
to turn an enum back into the string CSS wanted all along, why `bindgen`
emits a private `_raw` extern plus a generated match-wrapper for every
closed string set it meets, and why thirteen further `external` declarations
in std take a bare `str` for a vocabulary the host has closed.

**Thesis: this is not a new language feature. It is the removal of an
arbitrary restriction on a feature that already exists, already lowers
correctly, and already crosses a host boundary in exactly the shape the
string case needs.** A C-like enum today compiles to its **bare
discriminant** — `Ordering::Greater` is the JavaScript number `1`, not a
tagged array (P1) — and that bare value passes through an `external fun` to
the host unchanged (P4). A string-backed enum lowering to its bare string
would make the enum *be* the host's string. Every wrapper in the previous
paragraph is machinery for translating between two representations that
would then be the same representation.

The survey found the demand is real and concentrated: **eleven of the
fifteen payload-free enums in the whole standard library exist only to be
converted to a host string** (§2.1), all in one file, 63 lines of `match`
arms that delete outright. It also found the feature this extends is in
worse shape than anyone assumed: duplicate discriminants are accepted and
silently miscompile (P5), a discriminant on a payload variant is accepted
and silently ignored (P6), and `= 1.5` and `= 99999999999999999999` are both
accepted and silently become something else (P7). Those three want fixing
whether or not backed enums land, and §5 recommends they land first.

## 1. Ground truth — what the language does today

### 1.1 The grammar, and where it stops

```rust
/// `= (-)? integer` — an explicit enum discriminant, or `None` (backtracking)
/// when no `=` follows. The magnitude is parsed as `i64` (0 on overflow,
/// matching chumsky's `unwrap_or(0)`).
fn parse_discriminant(&mut self) -> Option<i64> {
    self.attempt(|parser| {
        parser.expect_op("=")?;
        let negative = parser.eat_op("-");
        let whole = parser.eat_integer()?;
        let magnitude = whole.parse::<i64>().unwrap_or(0);
        Some(if negative { -magnitude } else { magnitude })
    })
}
```
`crates/vilan-core/src/parsing.rs:3307-3318`

The spec agrees, and is unusually terse about it — `grammar.md:117-118`
gives `variant = NAME [ "(" … ")" ] [ "=" [ "-" ] NUMBER ]`, with one
sentence of prose at `grammar.md:122-123`: *"An explicit variant
discriminant (`= 0`, `= -1`) fixes the variant's integer tag."* The
production says `NUMBER`; the parser accepts only an integer. That
disagreement is the seed of §1.7's third hole.

The analyzer records one derived bit per enum:

```rust
is_numeric: all_data_less && any_explicit_discriminant,
```
`crates/vilan-core/src/analyzer.rs:15723`

Note the **conjunction**, which matters more than it looks: an enum is
bare-lowered only if every variant is data-less *and at least one carries an
explicit discriminant*. `enum Plain { A, B, C }` is not numeric.

### 1.2 The two lowerings (P1)

> **P1.** `enum Ordering { Less = -1, Equal = 0, Greater = 1 }` beside
> `enum Plain { A, B, C }`, one `match` over each, compiled with the worktree
> binary.

```js
const g = 1;              // Ordering::Greater — the bare discriminant
const p = [ 1 ];          // Plain::B          — the [index, ...data] array

const $a = g;             // match over the numeric enum
let $b = null;
if ($a === -1)      { $b = "less"; }
else if ($a === 0)  { $b = "equal"; }
else                { $b = "greater"; }

const $c = p;             // match over the array-form enum
let $d = null;
if ($c[0] === 0)      { $d = "a"; }
else if ($c[0] === 1) { $d = "b"; }
else                  { $d = "c"; }
```

Both programs run and print correctly. The machinery is
`variant_value` (`transformer.rs:4280-4299`), `numeric_enum_discriminant`
(`:4303-4314`) and `scalar_variant_test` (`:4317-4340`); `compares_natively`
(`:6246-6262`) is what lets `==` on a numeric enum stay native `===` instead
of dispatching to `PartialEq`.

The asymmetry deserves stating plainly, because a reader meeting backed
enums will trip on it: **adding `= 0` to one variant changes the runtime
representation of the entire enum.** That is true today, undocumented, and
§3.1 recommends keeping it.

### 1.3 The bare lowering already crosses a host boundary (P4)

This is the load-bearing evidence, and it is worth its own probe rather than
an inference from §1.2.

> **P4.** Does a numeric enum reach the host as its bare value, or does
> something re-wrap it at the boundary?
>
> ```vilan
> enum Code { Ok = 200, NotFound = 404 }
> [extern("String")]
> external fun to_host_string(value: Code): str;
> fun main() { print(to_host_string(Code::NotFound)); }
> ```
>
> Emits `console.log(String(404));`. Prints `404`.

Nothing wraps, nothing translates, no wrapper function is generated: the
enum *is* the number, all the way into the host call. A string-backed enum
would be the string, all the way into the host call — which is precisely
what every wrapper in §2 is hand-written to simulate.

### 1.4 What `match` compiles to, and the performance question

The lowering question B76 raises — "what does `match` compile to then, and
does the string comparison change performance shape?" — has a clean answer,
because the comparison target already exists in the language.

> **P2.** `match` over a raw `str`:
>
> ```vilan
> fun classify(s: str): i32 { match s { "start" => 0, "end" => 1, _ => 2 } }
> ```
> ```js
> const $a = s;
> let $b = null;
> if ($a === "start")    { $b = 0; }
> else if ($a === "end") { $b = 1; }
> else                   { $b = 2; }
> ```

Set that beside P1's numeric-enum chain and they are the same emission,
character for character apart from the compared constant. A string-backed
enum's `match` needs **no new codegen path at all** — it is `scalar_variant_test`
with a `js::Node::Str` where the `js::Node::Number` is.

On performance the honest answer is: the *shape* does not change, and the
constant barely does. Both forms are a linear `else if` chain over `===`
(neither is a jump table today, for numbers either). JS `===` on two string
primitives is a pointer comparison when both are interned, and every string
in the emitted chain is a source literal, so the compiler-side operand is
interned by construction; the subject may not be if it arrived from a host
call that built it dynamically, in which case the engine falls back to a
length check and a memcmp over a short keyword. That is a constant-factor
difference on an operation that is already not on any hot path this project
has measured. **This is not a performance argument in either direction, and
the proposal does not make one.** What it does buy is on the other side of
the ledger: today's wrapper runs the `===` chain *anyway* (§2.1's `match`
arms are exactly that chain) and then makes a call; a backed enum runs
nothing.

> **P8.** The scalar operators on a numeric enum, one program each:
>
> | source | emitted |
> |---|---|
> | `if c is Code::Ok` where `Code::Ok = 200` | `if ($a === 200)` |
> | `Level::Low == Level::High` | `0 === 1` |
> | `Level::Low < Level::High` | `0 < 1` |
>
> All three check clean. The third matters in §3.6.

So `is` and `==` fold to the same `===` a `match` arm produces — no separate
path to widen for strings.

### 1.5 Exhaustiveness is checked on variants, not on values (P3)

> **P3.** A `match` over `Ordering` missing the `Greater` arm:
> `Error: match is not exhaustive: missing 'Greater'`.

Exhaustiveness runs on the variant set, by name, in the analyzer, before
anything knows about lowering. Backing values do not enter it and would not
need to. This is the cheapest answer in the paper: **exhaustiveness needs no
change whatsoever.**

The one interaction worth recording is on the other side: a numeric enum's
`match` cannot be written against raw values —

```
match g { 1 => "one", _ => "other" }
    → Error: literal pattern of type i32 cannot match type Ordering
```

— which is correct and should stay correct for strings. `match align {
"start" => … }` must remain an error; the backing value is a
representation, not a second spelling of the variant.

### 1.6 What `Wire` does with an enum today (P9)

The derived `Wire` impls for an enum are generated as vilan source by
`enum_wire_visitor_impls` (`analyzer.rs:26813-26895`), and they key on the
**variant name**: `serializer.begin_variant("{name}", {arity})`,
`rebuild` matching `deserializer.variant_tag()` against `"{name}"`.
`is_numeric` is never consulted.

> **P9.** `[derive(Wire)]` on a numeric enum, a plain enum, and a payload
> enum, encoded with `std::json::encode_json`:
>
> | value | JSON |
> |---|---|
> | `Ordering::Greater` (discriminant `1`) | `"Greater"` |
> | `Plain::B` | `"B"` |
> | `Payload::Text("hi")` | `{"Text":"hi"}` |

So the discriminant is **already** invisible to serialization: `Ordering`
goes on the wire as `"Greater"`, not `1`. That is the fact §3.9 has to
reckon with, and it inverts the naive expectation — making a backed enum
serialize as its backing value is a *divergence* from current behavior, not
an extension of it.

Free of charge, the survey establishes that the divergence costs nothing
today: there is **no `[derive(Wire)]` enum anywhere in `vilan/std/src/`** —
the only derive sites are `arena.vl:22` (a struct) and doc-comment
references in `wire.vl:4` / `jwt.vl:4`. Nothing in std's wire format changes.

### 1.7 Three silent holes in the discriminant grammar as it stands

The survey went looking for how the existing feature validates its input.
It does not.

**(a) Duplicate discriminants silently miscompile (P5).** This is a live
bug, not a design gap.

> **P5.**
> ```vilan
> enum Dup { A = 1, B = 1, C = 2 }
> fun main() {
>     let d = Dup::B;
>     print(match d { Dup::A => "a", Dup::B => "b", Dup::C => "c" });
> }
> ```
> Compiles with **no diagnostic**. Emits:
> ```js
> const d = 1;
> if ($a === 1)      { $b = "a"; }
> else if ($a === 1) { $b = "b"; }   // unreachable
> else               { $b = "c"; }
> ```
> Prints **`a`**.

A `Dup::B` value matches the `Dup::A` arm. Two distinct variants are one
runtime value, the second arm is dead, and the program is exhaustively
matched and wrong. Nothing in the analyzer checks discriminant uniqueness.

**(b) A discriminant on a mixed enum is accepted and dropped (P6).**
`enum Mixed { A = 1, B(str) }` compiles clean. Because `is_numeric` requires
`all_data_less`, the enum takes the array form and the `= 1` is inert — it
parsed, it was stored in `EnumVariantDeclaration::discriminant`, and nothing
will ever read it. A user who writes it is expressing an intent the compiler
silently discards.

**(c) A non-integer discriminant is accepted and silently changed (P7).**

> **P7.** Two programs, both `no errors`:
>
> | source | actual discriminant | emitted |
> |---|---|---|
> | `enum A { X = 1.5, Y = 7 }` | `X` → `1` | `1 === 7` |
> | `enum B { X = 99999999999999999999, Y = 1 }` | `X` → `0` | `0 === 1` |

The float truncates; the overflow becomes `0` via the documented
`unwrap_or(0)` at `parsing.rs:3315` — a behavior the comment attributes to
matching chumsky, i.e. inherited from the pre-`frontend.md` parser and never
revisited. The overflow case is the worse of the two, because `0` is a
perfectly ordinary discriminant that a sibling variant may legitimately hold,
which routes it straight into hole (a).

These are not incidental to this proposal. A string backing makes (a)
dramatically more likely to be hit — two CSS keywords colliding is a typo,
not an exotic input, and std already writes `Display::Hidden => "none"` and
`UserSelect::Off => "none"` (in different enums, which is fine, but shows how
near the shape is). **Recommendation: all three get closed, as their own
slice, landing before the backed-enum work and independently valuable
without it.** They reject only programs that are already miscompiling.

#### SHIPPED as B79 (v0.32.0 cycle) — what the survey got right, and two more

All three closed, as their own slice, ahead of this proposal and without
prejudging it. The messages state the rule as it stands — "an enum
discriminant must be an integer", never "always will be" — so §3.1's
widening is not foreclosed.

Two corrections to the record above, both found by reading the token
rather than the value:

- **(c) is wider than "a fraction".** `unwrap_or(0)` sat over the number
  token's WHOLE part alone, so it also ate a type suffix and a hex
  literal. `= 1u32` became `1`; `= 1_000` became `1`, because the lexer
  reads `_000` as a suffix; and `= 0xFF` became **`0`**, since
  `parse::<i64>()` fails outright on `0xFF` — a second, undocumented route
  into hole (a). Hex is now read as hex (the analyzer's own range check
  has always read `0x` as radix 16); a fraction and a suffix are errors.
  A third route was a compiler PANIC rather than a wrong answer:
  `enum E { A = 9223372036854775807, B }` overflowed the debug build's
  `discriminant + 1` and wrapped the release build's.

- **(b) is not "a discriminant on a payload variant".** §1.7's own P6 is
  `enum Mixed { A = 1, B(str) }`, where the discriminant sits on the
  DATA-LESS variant and `B`'s payload is what makes it inert — so the
  narrow reading would have left the recorded hole open. The rule shipped
  is §3.3's: a payload variant may not carry a discriminant, and neither
  may its data-less siblings. Two messages, one for each shape.

The sweep confirms §5's premise: `std/src/compare.vl`'s `Ordering` is the
only enum in the tree using the feature (plus the corpus's own copy of
it), both legal under every rule, and corpus goldens are byte-identical.
So this really did reject only programs that were already miscompiling.

Also closed in the same arc, per the entry's second half: `grammar.md`
wrote the production as `NUMBER` where the parser means an integer, and
`types.md` §5.3 now carries the representation rule — `is_numeric` is a
CONJUNCTION, so one `= 0` changes the whole enum's runtime shape — which
was documented nowhere. That rule is what §3.5's lowering builds on, so
it is now a written premise rather than a read-the-compiler one.

## 2. The demand side

### 2.1 std — eleven of fifteen payload-free enums exist only to become a string

A sweep of `vilan/std/src/` found **27 `enum` declarations, 15 of them
payload-free**. Eleven of those fifteen are CSS keyword enums in
`std/src/style.vl`, and every one is paired with a hand-written function
whose whole body is a `match` from the variant to the string the host wanted:

```vilan
fun display(self, value: Display): Style {
    self.raw("display", match value {
        Display::Flex        => "flex",
        Display::Grid        => "grid",
        Display::Block       => "block",
        Display::Inline      => "inline",
        Display::InlineBlock => "inline-block",
        Display::InlineFlex  => "inline-flex",
        Display::InlineGrid  => "inline-grid",
        Display::Hidden      => "none",
    })
}
```
`std/src/style.vl:658-669`

With a backed enum that is:

```vilan
fun display(self, value: Display): Style {
    self.raw("display", value.value())
}
```

The full inventory (`match`-block line range, arms, lines that delete
outright — the `match value {` line survives as the rewritten call, the arms
and the closing `})` do not):

| enum | wrapper | `match` block | arms | deletes |
|---|---|---|---|---|
| `RadialExtent` | `Gradient::radial` | 326–331 | 4 | 5 |
| `Display` | `Style::display` | 659–668 | 8 | 9 |
| `Position` | `Style::position` | 672–678 | 5 | 6 |
| `FlexDirection` | `Style::flex_direction` | 682–687 | 4 | 5 |
| `AlignItems` | `Style::align_items` | 691–697 | 5 | 6 |
| `JustifyContent` | `Style::justify_content` | 701–708 | 6 | 7 |
| `Overflow` | `Style::overflow` | 911–916 | 4 | 5 |
| `WhiteSpace` | `Style::white_space` | 992–998 | 5 | 6 |
| `UserSelect` | `Style::user_select` | 1002–1007 | 4 | 5 |
| `TextAlign` | `Style::text_align` | 1026–1030 | 3 | 4 |
| `Cursor` | `Style::cursor` | 1034–1039 | 4 | 5 |
| | | **74 lines** | **52** | **63** |

All eleven in one file. `RadialExtent` is the only one that is not a `Style`
method — its `match` sits inline in a struct literal (`geometry = match
extent { … },`) and collapses to `geometry = extent.value(),`, which is
worth noting because it shows the conversion wants to be an *expression*,
not a method the wrapper pattern happens to admit.

**The strongest single fact in the sweep is a negative one.** These strings
are not derivable from the variant names:

| variant | backing value |
|---|---|
| `AlignItems::Start` | `"flex-start"` |
| `AlignItems::End` | `"flex-end"` |
| `JustifyContent::Between` | `"space-between"` |
| `Display::Hidden` | `"none"` |
| `UserSelect::Off` | `"none"` |

`Hidden` and `Off` are named as they are *deliberately*, to stay clear of
`Option::None` at use sites — the comments at `style.vl:375` and
`style.vl:443-444` say so. So the cheap alternative design — derive the
string from the variant name by a case convention, the `serde(rename_all)`
move — **is disqualified by the demand it would serve**. A rule that is
right for six of eleven std enums and wrong for five is worse than no rule,
because being wrong is silent. The backing value is arbitrary text and must
be written.

### 2.2 The reverse direction, in its degraded form

`std/src/json.vl:110-130`. `JsonValue::kind()` is an intrinsic returning a
closed set, and its doc comment says so:

```vilan
/// The normalized JSON type — `"number"`/`"string"`/`"boolean"`/`"array"`/
/// `"object"`/`"null"` (an intrinsic: `typeof` mis-buckets arrays and null).
external fun kind(self): str;

fun is_number(self): bool { self.kind() == "number" }
fun is_string(self): bool { self.kind() == "string" }
fun is_bool(self): bool   { self.kind() == "boolean" }
fun is_array(self): bool  { self.kind() == "array" }
```

Six members in the documented set; four predicates. `"object"` and `"null"`
never got one. That is the failure mode a closed type prevents and a doc
comment does not — **the set is closed in prose and open in the type
system**, so nothing noticed the two missing cases. 15 lines, 4 functions,
13 call sites across std.

This direction is also where the language has *nothing* to offer today: the
sweep found **not one function in `vilan/std/src/` that takes a host `str`
and returns an `Option<SomeEnum>`.** The conversion is one-directional
throughout std. That is not because nobody wants it; it is because writing
it by hand costs a `match` with a `_ =>` arm per enum and nobody paid.

### 2.3 The thirteen sites that stayed untyped

Where the wrapper was not worth writing, std simply passes `str` for a
vocabulary the host has closed. These delete nothing — they are the safety
the feature would buy rather than the lines it would save, and they are the
better measure of the cost:

`browser/dom.vl:94,101` (`on`/`on_event`, DOM event names),
`browser/dom.vl:143` (`key()`, `KeyboardEvent.key`),
`browser/dom.vl:25` (`create_element_ns`, XML namespace URIs),
`browser/router.vl:31,45` (window event names),
`fetch.vl:114` (`set_method`, HTTP verbs — with the verb literals loose at
`fetch.vl:145,150,157,176` and `Request.method: str` at `:138`),
`process/http.vl:49` (`method()`, inbound verbs),
`process/http.vl:31,71,75,112` (node stream event names),
`process/fs.vl:30` (node encodings — `read_file_to_str` at `:42-44` is a
three-line wrapper whose only job is passing `"utf8"`),
`process/rpc_server.vl:41,43` (digest algorithms and output encodings, with
`"sha1"`/`"base64"` hardcoded at `:47`),
`rpc.vl:337` (`set_binary_type` — the host accepts exactly `"blob"` |
`"arraybuffer"`),
`rpc.vl:325` (`host_kind`, `Object.prototype.toString` tags, string-compared
at `rpc.vl:610`),
`asset.vl:24` (`emit(kind, line)` — every std call site passes `"css"`).

### 2.4 bindgen's generated match-wrapper (P0)

> **P0.** `vilan bindgen` on a four-line `.d.ts` declaring
> `type Align = "start" | "end" | "center"` plus one method taking it, one
> returning it, and one free function taking two.

The generated file today (abridged — the emission is
`bindgen.rs:1259-1321`):

```vilan
enum Align { Start, End, Center }

[extern(method, "setAlign")]
[doc(hidden)]
[platform("browser")]
external fun set_align_raw(self, value: str): void;

/// `set_align` — `set_align_raw` with its closed string sets spoken as enums.
fun set_align(self, value: Align): void {
    self.set_align_raw(match value {
        Align::Start => "start",
        Align::End => "end",
        Align::Center => "center",
    })
}

// TODO(bindgen): returns the closed string set `Align` — the raw `str` is
// bound because the host may return a value outside the set; match it to
// `Align` by hand
[extern(method, "getAlign")]
[platform("browser")]
external fun get_align(self): str;
```

Three things to read out of that. The wrapper is real generated *logic* —
"new territory for bindgen relative to every other row in this table
(everything else emits signatures only, never bodies)", as `bindgen.md`
§3.3 puts it. It is emitted **per parameter, per binding**, so the free
function taking two `Align`s gets the arm block twice. And the **return
direction has no wrapper at all** — it is a TODO, because there is no
spelling for string → variant. On the `lib.dom.d.ts` probe that is
**375 TODOs** of construct class "string-literal union property"
(`bindgen.md` §10.1), every one of them a getter bindgen had to give up on.

## 3. The design

### 3.1 Syntax — a generalization of the discriminant, not a new kind of enum

**Recommendation: generalize.** The grammar becomes

```text
variant = NAME [ "(" [ type { "," type } [ "," ] ] ")" ]
          [ "=" ( [ "-" ] INTEGER | STRING ) ] ;
```

and the analyzer's `is_numeric: bool` becomes `backing: Option<Backing>` with
`Backing::Int` / `Backing::Str`, `EnumVariantDeclaration::discriminant: i64`
becoming a `BackingValue` of the same two shapes. No new keyword, no
`enum Align: str { … }` header form, no second concept in the type system.

The case for this is that the semantics are already identical and were
designed once: *a payload-free variant may carry a compile-time-constant
scalar, and an enum whose variants carry one lowers to that scalar bare.*
Everything in §1.2's machinery — `variant_value`, `numeric_enum_discriminant`,
`scalar_variant_test`, `compares_natively` — is written against that sentence
and needs its `i64` widened, not its structure changed. A separate "backed
enum" kind would fork all four and leave the language with two names for one
idea.

Two sub-rules fall out:

**(a) A string backing must be explicit on every variant.** C-style
auto-increment is meaningful for integers (`enum X { A = 5, B }` gives `B`
the value 6, and today's `next_discriminant` at `analyzer.rs:15697` does
exactly that). There is no successor of `"start"`. So: if any variant carries
a string, every variant must, and a missing one is an error naming the
variant. Deriving it from the name is rejected on §2.1's evidence.

**(b) The `all_data_less && any_explicit_discriminant` asymmetry stays.**
An enum is bare-lowered iff it is payload-free *and* at least one variant is
explicit; `enum Plain { A, B }` keeps its `[0]`/`[1]` array form (P1). This
is a wart — adding `= 0` to one variant silently changes the representation
of the whole type — but changing it would change the runtime representation
of every payload-free enum in every existing program, and the array form is
what `Wire`'s derive, pattern matching, and the `Hashable` story all
currently assume. **Recommendation: preserve it exactly, and document it in
`grammar.md`, which today says nothing about representation at all.**

### 3.2 Mixed backings in one enum — reject

`enum X { A = 1, B = "two" }`. **Recommendation: hard error.**

This is not a taste call. An enum has **one** runtime representation. A mixed
enum could only lower to the tagged-array form, which discards the entire
point of the feature, or to a JS value that is sometimes a number and
sometimes a string, which no vilan type can describe and which would make
`.value()` (§3.8) have no return type. The backing type is fixed by the first
explicit value in declaration order; every later value must agree, and a
disagreement is an error naming both variants and both spellings.

### 3.3 Payload variants in a backed enum — reject, and close the existing hole

`enum X { A = "a", B(str) }`. **Recommendation: hard error**, and the same
error for the integer case, which today is P6's silent drop.

The bindgen use case needs no payloads: `bindgen.md` §3.3 routes
discriminated unions to a *different*, unbacked enum with per-variant payload
structs, and closed string unions to a payload-free one. std's eleven are all
payload-free. Nothing in the demand asks for a hybrid, and a hybrid has no
coherent lowering — a bare backing value has nowhere to put a payload.

So: a variant carrying a payload may not carry a backing value, and an enum
containing any payload variant may not have any backing values. The
diagnostic should name the offending variant and say which of the two rules
it broke. **This rejects `enum Mixed { A = 1, B(str) }`, which compiles
today** — see §5 on why that is a fix rather than a break.

### 3.4 Which backing types — `str` and the existing integers, nothing else

**Recommendation: `str` plus the integer form that already exists. Not
floats, not `bool`.**

- **`str`** — the entire motivation (§2).
- **integers** — already shipped, already used (`compare.vl:13-17`), must
  keep working unchanged.
- **floats — reject.** No demand: the `lib.dom.d.ts` probe's TODO table
  (`bindgen.md` §10.1) shows 375 string-literal union properties and no
  numeric-literal union class at all. And the semantics are hostile — the
  lowering is `===`, so `0.1 + 0.2` is a footgun on a value the user never
  computes but the *host* might, and `NaN !== NaN` breaks both the duplicate
  check (§3.7) and any variant test. Revisit on a real driver application,
  not before. This also finally makes P7's `= 1.5` an error instead of a
  silent truncation.
- **`bool` — reject.** `bool` is itself an enum in std (`boolean.vl:6-9`)
  that already lowers to native `true`/`false` via the `bool_enum_id`
  special case (`transformer.rs:4290-4292`). A two-variant bool-backed enum
  is `bool` with extra steps and a worse `match`.

### 3.5 Lowering — the bare backing value, per the precedent

**Recommendation: `Align::Start` compiles to `"start"`.** Exactly as
`Ordering::Greater` compiles to `1` (P1) and reaches the host as `404`
(P4). `variant_value` gains a `js::Node::Str` arm beside its
`js::Node::Number` one; `scalar_variant_test` likewise; `compares_natively`
returns true for a string-backed enum for the same reason it does for a
numeric one, and for `str` itself (`transformer.rs:6249-6250` already lists
`"str"` among the natively-comparing struct names).

`match` needs no new path (§1.4). Performance shape is unchanged and the
paper makes no claim beyond that.

### 3.6 Ordering operators on a string backing — reject

Not asked in the charter, but the survey forced it. `<` works on a numeric
enum today — `Level::Low < Level::High` emits `0 < 1` and checks clean
(P8) — and std's `PartialOrd` **defaults depend on it**: `compare.vl:22-36`
implements `lt`/`le`/`gt`/`ge` as comparisons against `Ordering::Equal`.

On a string backing, `<` would lower to JavaScript's lexicographic string
comparison over the *backing value*, so `Size::Large < Size::Small` would be
true because `"lg" < "sm"`. That is essentially never what a reader means,
and the thing they *do* mean — order by declaration index — cannot be
provided, because bare lowering erases the index at runtime.

**Recommendation: `<`, `<=`, `>`, `>=` are rejected on a string-backed enum,
with a diagnostic that says the backing value is not an order and points at
writing an explicit `impl Ord` or using an integer backing.** `==` and `!=`
stay. The integer form is untouched.

### 3.7 Exhaustiveness (unchanged) and duplicate backing values (reject)

**Exhaustiveness: no change.** It is checked on the variant set by name,
before lowering (P3), and backing values are irrelevant to it. Matching a
backed enum against a raw literal stays an error, as it is for integers
today (§1.5).

**Duplicates: hard error, for both backings.** `enum Align { Start = "a", End
= "a" }` is rejected naming both variants and the shared value. So is
`enum Dup { A = 1, B = 1 }`, which today compiles and miscompiles (P5).

The argument is P5's output. Two variants sharing a backing value are one
runtime value: the second `match` arm is unreachable, `Dup::B == Dup::A` is
true, and an exhaustive `match` returns the wrong answer with exit 0. There
is no legitimate use — a variant that should be indistinguishable from
another is the same variant. This is the one recommendation in the paper
that fixes a bug rather than adding a capability, and §5 recommends it land
on its own regardless of what happens to the rest.

### 3.8 Conversions — `.value()` out, `Enum::parse` back

Two directions, both synthesized by the compiler on every backed enum.

**(a) Variant → backing value: an inherent method `value()`.**

```vilan
Align::Start.value()      // "start"
Ordering::Greater.value() // 1
```

Return type is the enum's backing type. It lowers to the **identity** — the
receiver already *is* the backing value at runtime — so it costs nothing and
emits nothing; `value.value()` in a rewritten `style.vl` compiles to `value`.

Naming. The runner-up was `.raw()`, which has the advantage that bindgen
already uses `_raw` for this exact concept. It is rejected because in this
codebase `raw` consistently means *the escape hatch that bypasses the typed
surface* — `Style::raw(property, value)` (`style.vl`), and bindgen's `_raw`
externs are `[doc(hidden)]` precisely because they are the thing you are not
supposed to call. `.value()` is total, safe, and first-class; nothing is
being bypassed, and it should not borrow the vocabulary of bypassing.
`.backing()` is accurate and reads like compiler-internals at a call site.

Collision. If a user declares their own `fun value(self)` on a backed enum,
**recommendation: hard error, naming the synthesized member.** Silently
preferring one is exactly the class of bug B57 was ratified to kill
(`method-resolution.md`: duplicate-inherent is a hard error), and a
synthesized member that quietly loses is worse than a user-visible name
clash.

**(b) Backing value → variant: a static `Enum::parse`, returning `Option`.**

```vilan
Align::parse("start")   // Some(Align::Start)
Align::parse("middle")  // None
```

Return type is `Option<Self>`. This matches the house form for a fallible
parse exactly — `str::parse_i32(): Option<i32>` and `str::parse_f64():
Option<f64>` (`option.vl:294,299`), `str::try_parse_json():
Option<JsonValue>` (`json.vl:91`) — all three of which return `Option`, not
`Result`. `Result` is rejected because there is exactly one failure mode and
its error string would carry nothing the caller does not already have;
`from_json` earns `Result` because decoding has many.

A static rather than a method on `str` is forced: a per-enum method would
pollute `str` with one name per backed enum in scope, and vilan has no
turbofish to disambiguate a generic `text.parse()`.

Lowering is the `===` chain in reverse. For a large variant set a
module-level lookup object (`{"start": …}`) is the better emission; that is
an implementation choice, not a semantic one, and the paper does not fix a
threshold.

Deliberately **not** recommended for v1: a compiler-derived `impl Align with
Into<str>` or `with Display`. Both are the natural-looking answer and both
are premature while backlog item 73 is open — `impl type T with Into<T>` in
`std/src/into.vl` is a blanket impl that matches every subject and wins by
declaration order, so a user's own `impl Align with Into<str>` already loses
to it today. Hanging a synthesized impl off that machinery is asking for a
resolution bug in a feature whose entire value proposition is that it is
simple. A plain inherent method sidesteps it. Layer the trait impls on
later, once B73 has a specificity rule.

### 3.9 `Wire` and JSON — serialize as the backing value

**Recommendation: a backed enum serializes as its backing value, for both
string and integer backings, and `rebuild` accepts that value.**

`Align::Start` encodes as `"start"`, not `"Start"`. `Ordering::Greater`
encodes as `1`, not `"Greater"`.

This is the point of the feature: the JSON on the wire is the value the host
speaks, and it round-trips through `parse`. But §1.6 established it is a
**divergence** — today's derive keys on the variant name and ignores the
discriminant entirely (P9), so this changes the meaning of `[derive(Wire)]`
on an enum that has explicit backing values.

What breaks, checked rather than assumed: **nothing on disk.** There is no
`[derive(Wire)]` enum anywhere in `vilan/std/src/`; the derive's only std
uses are on structs. `Ordering` — the one integer-backed enum in std — does
not derive `Wire`. So the divergence is free today and should be taken now,
while it is free, rather than after the first user ships a format.

Two consequences to write down rather than mechanize:

- **Adding a backing value to an existing `[derive(Wire)]` enum is a wire
  format break.** So is removing one. This deserves a sentence in
  `docs/std/encoding.md`, not a compiler mechanism.
- The `rebuild` unknown-tag path already exists and already does the right
  thing — `deserializer.fail(i"unknown variant '{tag}'")` plus a poisoned
  zero-construction (`analyzer.rs:26880-26886`) — so a host sending a value
  outside the set decodes to `Err`, not to garbage. No change needed there.

## 4. What this deletes

### 4.1 bindgen §3.3

The generated output for P0's input becomes:

```vilan
/// `Align` — the closed string set `"start" | "end" | "center"`.
enum Align { Start = "start", End = "end", Center = "center" }

[extern(method, "setAlign")]
[platform("browser")]
external fun set_align(self, value: Align): void;

/// The host may return a value outside the set — `parse` is the guard.
[extern(method, "getAlign")]
[doc(hidden)]
[platform("browser")]
external fun get_align_raw(self): str;
fun get_align(self): Option<Align> { Align::parse(self.get_align_raw()) }
```

The **parameter** direction loses its wrapper entirely — the extern takes the
enum, because the enum is the string. Deleted from
`crates/vilan-core/src/bindgen.rs`:

- the wrapper-emission block in `emit_one_binding` — `has_wrapper`,
  `raw_name`, the `[doc(hidden)]` line, the arm-rendering `match`, the
  assembled wrapper pushed to `extra` (**1259–1321, ~62 lines**);
- `ParameterForm` and its two variants, and `render_parameters`'s
  string-enum arm (**1875–1900, ~26 lines**);
- the `string_enum: Option<String>` field threaded through `Mapped`
  (`:317`) and `RenderedParameter` (`:1875`) and initialized at eleven
  further sites — **27 references** in all.

What **stays** is the part that was never the problem: the `StringEnum`
collection (`:343-345`, `:411-414`) and the alias→enum emission
(`:568-592`), now writing `Start = "start"` instead of `Start,`.

The **return** direction is the bigger win and it is not a deletion at all —
it is 375 TODOs on `lib.dom.d.ts` (`bindgen.md` §10.1, construct class
"string-literal union property") becoming real bindings, because `parse`
gives the generator a spelling it does not have today. §3.3's own summary of
the wrapper as "real generated logic beyond a bare declaration, which is new
territory for bindgen relative to every other row in this table" resolves the
right way: bindgen goes back to emitting signatures only, plus one
one-line `parse` forwarder in return position.

`crates/vilan-core/tests/bindgen.rs:866-885`
(`a_vilan_enum_cannot_carry_a_string_backing_value`) goes red on the day
this lands, by construction — it asserts the parse error. It should be
replaced, not deleted: invert it to assert the backed form compiles and the
generated output contains no `_raw` wrapper for a parameter.

### 4.2 std

| file | what | lines |
|---|---|---|
| `style.vl` | 11 enum→string wrappers, 52 `match` arms | **−63** |
| `json.vl:110-130` | `kind(): JsonKind` + 4 predicates deleted | **−15** |
| | | **−78, 15 functions** |

`style.vl` is mechanical: each of the eleven enums gains its strings and each
wrapper collapses to one line (§2.1). The strings move from the wrapper to
the declaration, so the *file* loses 63 lines and the *type* gains the
information — `enum AlignItems { Start = "flex-start", … }` says at the
declaration what today is only discoverable by reading a function 300 lines
away.

`json.vl` depends on a backed enum being legal as an `external fun`'s return
type (§7.2). If it is, `external fun kind(self): JsonKind` replaces the `str`
version, the four predicates delete, and their 13 call sites become
`v.kind() == JsonKind::Number` — with `"object"` and `"null"`, which never
got predicates, covered for free by exhaustiveness.

The thirteen `external` sites in §2.3 delete nothing. They are the reason to
do this anyway.

## 5. Migration and back-compat

**Existing integer-discriminant enums do not change meaning.** The grammar is
widened, not altered; `Backing::Int` behaves exactly as `is_numeric` does
today; lowering, `match`, `==`, and ordering are all untouched for integers.
`Ordering` and every user enum with discriminants compile identically and
emit identical JavaScript. The corpus goldens (`vilan/test/*.js`, a
byte-identical gate) should not move at all for the integer path, and that
is the check to run first.

Three of the recommendations **reject programs that compile today**:

| rejects | today | §5 verdict |
|---|---|---|
| duplicate backing values (§3.7) | compiles, **miscompiles** (P5) | fix |
| backing value on a payload variant (§3.3) | compiles, value silently dropped (P6) | fix |
| non-integer numeric backing (§3.4) | compiles, silently truncated / zeroed (P7) | fix |

All three currently produce code that does something other than what was
written. Rejecting them is a strict improvement, not a break, and none has
a legitimate use to preserve.

**Recommendation: land those three as their own slice, first and
independently.** They are small, they are valuable without backed enums, and
they make the backed-enum slice's validation a widening of an existing check
rather than a new one. The house rule that a fix needs a pin per case applies:
three pins, one per hole, each proven non-vacuous by planting the bug.

The one genuine format change is `Wire` (§3.9), and §1.6 checked that it
costs nothing in the tree today.

## 6. Slices

1. **Validate the discriminant that exists.** Duplicate check, payload-variant
   check, non-integer rejection. Three diagnostics, three pins in
   `inference.rs`. Independent of everything below; ships on its own.
2. **The grammar and the type.** `parse_discriminant` accepts a string;
   `is_numeric` → `backing: Option<Backing>`; `discriminant: i64` →
   `BackingValue`. §3.1's rules (a) and (b), §3.2's one-backing-per-enum,
   §3.4's type set. No codegen yet — an enum parses and checks, and lowering
   still refuses. `grammar.md` and `types.md` updated in the same commit.
3. **Lowering.** `variant_value`, `scalar_variant_test`, `compares_natively`.
   §3.6's ordering rejection. Corpus goldens verified unmoved for integers.
4. **Conversions.** Synthesized `value()` and `Enum::parse`, §3.8's collision
   error.
5. **`Wire`.** §3.9, plus the `docs/std/encoding.md` note.
6. **std adoption.** `style.vl`'s eleven; `json.vl`'s `JsonKind` if §7.2
   resolves in favor. Docs pages for `std::style` updated in the same commit.
7. **bindgen.** §4.1's deletions, the return-direction `parse` forwarder, and
   the inverted pin.

## 7. Open questions

### 7.1 Does a backed enum become `Hashable`? — recommend: out of scope, but raise it

> **P10.** `Map<Level, str>` where `enum Level { Low = 0, High = 1 }`:
> `Error: 'Level' does not implement trait 'Hashable', required by a generic
> bound of this call`.

A numeric enum lowers to a plain JS number and a string-backed one to a plain
JS string — both of which the host `Map` keys natively — and neither is
`Hashable` today. The feature makes this considerably more glaring, because
"the enum *is* the string" is the whole pitch and the first thing a user will
try is keying a map by it.

**Recommendation: do not solve it here.** It belongs to `hashable-keys.md`
(draft, backlog-tracked) and solving it for bare-lowered enums only would
create a rule that half the enums in a program satisfy for reasons invisible
at their declaration. But this proposal is the strongest case yet for a
compiler-derived `Hashable` on bare-lowered enums, and it should be recorded
against `hashable-keys.md` when this is reviewed. Left open because it is
genuinely a different paper's call, not because the answer is unclear.

### 7.2 May an `external fun` return a backed enum? — recommend: yes, with a caveat I am not fully comfortable with

The parameter direction is safe: vilan constructs the value, so it is always
in the set. The return direction is not — the host can return `"middle"` for
an `Align`, and nothing checks.

What happens then is the uncomfortable part, and it is a fact about today's
lowering rather than a new risk: an exhaustive `match` compiles its last arm
to a bare `else` (P1, P2 — there is no "impossible" trap arm), so a bogus
value silently takes whichever arm happens to be last. The value is not
detectably wrong; it is confidently the wrong variant.

**Recommendation: allow it.** `external` is already a trust boundary in
exactly this way — `external fun f(): i32` returning `"hello"` is equally
unchecked and equally silent, and the language has never pretended
otherwise. Adding a runtime guard here and nowhere else would be
inconsistent, and adding it everywhere is a different and much larger
proposal. But **bindgen must not generate it** (§4.1 generates the `parse`
form), and std should use it only where the host's set is genuinely closed by
the platform rather than by convention — `json.vl`'s `kind()` qualifies (the
intrinsic is std's own code), `fetch.vl`'s inbound HTTP verb does not.

Recorded as open rather than settled because the "confidently the wrong
variant" behavior is the one outcome in this paper I would want the owner to
look at directly. If the ruling is to forbid it, §4.2 loses `json.vl`'s 15
lines and nothing else in the paper changes.

### 7.3 Should `value()` and `parse` be synthesized, or written by a derive? — recommend: synthesized

`[derive(Backed)]` would make the two members opt-in and visible at the
declaration, matching how `Wire` works. Synthesizing them unconditionally
makes them always available, which is what a user who wrote `= "start"`
plainly wants, and avoids a second thing to remember.

**Recommendation: synthesize.** The backing value is already the opt-in — you
do not accidentally write `= "start"` — so a derive would be a second switch
for the same decision. Left in the open section only because it is the one
place the paper adds compiler-synthesized members to a user type without a
`derive` marker, and that is a precedent worth the owner seeing rather than
inheriting.

### 7.4 Does `str` remain the only string backing if sized string types ever land? — recommend: revisit then, not now

`numeric-types.md` shipped sized integers and left a native-width tail; there
is no analogous string story and no proposal for one. If one appears, the
backing set widens by the same argument that admits `str`. Nothing to decide
today. Noted only so a future reader does not mistake §3.4's list for a
closed set on principle rather than on demand.

## 8. Implementation notes (v0.35.0)

Shipped as recorded, every settled recommendation in §3 standing and §7.2
enforced as deferred. What follows is what the build found — the places
the paper's design met the code and needed a decision it had not made,
and the two claims that were re-checked rather than inherited.

### 8.1 What the paper got right, checked against the build

- **§1.4/P2's claim that `match` needs no new codegen is exact, not
  approximate.** The pin compares two WHOLE emissions — a `match` over a
  three-variant string-backed enum and a `match` over a raw `str` with a
  `_` arm — and asserts byte equality. They are identical.
  `scalar_variant_test` needed a `js::Node::String` where its
  `js::Node::Number` was, and nothing else.
- **§5's "the corpus goldens should not move at all for the integer
  path" held, and then some.** No corpus golden moved for the *string*
  path either, including `style.mjs` and `style.css` after §4.2's
  rewrite: styles construct inside `const`, so the eleven wrappers were
  folded at build time and never reached the emitted output. The wrapper
  really was pure compile-time translation between two representations
  that are now one.
- **§1.6's "the divergence costs nothing today" was re-verified, not
  assumed.** Still no `[derive(Wire)]` or `[derive(Json)]` enum anywhere
  in `vilan/std/src/`; `arena.vl:22` remains the only derive site and it
  is a struct.
- **§4.2's `−63` for `style.vl` is exactly right** (126 deletions, 63
  insertions). `RadialExtent`'s inline `match` inside a struct literal
  did collapse to `geometry = extent.value()`, which is the case §2.1
  flagged as showing the conversion wants to be an expression.

### 8.2 Decisions the paper left open, and how they were made

**(a) `value()`/`parse()` are generated as vilan SOURCE, not built into
the compiler.** §3.8 specifies the members and their lowering but not the
mechanism. Source generation — through the same channel `[derive(..)]`
uses, collected in its own pass because a backing value is not a derive
and must be reached however the enum is wrapped — was chosen for one
reason above the others: it makes §3.8's collision rule fall out of B57
rather than needing a rule of its own. A user's `fun value(self)` on a
backed enum meets the duplicate-inherent error, which is precisely "a
hard error, naming the synthesized member". The `Option` construction,
monomorphization, demand-driven emission and the docs gate come free.

`value()` still lowers to the identity, as §3.8 requires: the transformer
folds `x.value()` to `x` (`backed_value_members`), leaving the generated
body with no callers, so it emits nothing. The body is the semantics the
fold has to agree with rather than dead weight.

B57's duplicate message gained one case in the process: a
compiler-synthesized first declaration has no file to point at, so the
note says what it is instead of pointing the author's own declaration
back at itself, and a synthesized member now always sorts first so the
error lands on the declaration the author can edit.

**(b) `parse` is an `if`/`else if` chain, not a `match`.** A `match`
cannot be written against a NEGATIVE literal pattern, and
`Ordering { Less = -1 }` is the one backed enum std already shipped. The
emission is the same `===` chain either way. §3.8's note that a
module-level lookup object is the better emission for a large variant set
remains an open implementation choice; no threshold is fixed.

**(c) The integer backing type is `i32`, widening to `i53`.** §3.8 says
"the enum's backing type" without saying which integer. `i32` is the
language's default integer, so `Ordering::Greater.value() == 1` needs no
suffix; `i53` is the widest integer a JS number holds exactly, and a
backed enum IS a JS number. A discriminant outside `i53` gets NO
conversions at all — see §8.4.

**(d) A generic enum gets no conversions.** A backed generic enum's
parameter can only be phantom (§3.3 rejects payloads), and `Enum::parse`
on one would have no way to bind it. The `[derive(..)]` generators
already skip generic enums for the same reason. Recorded rather than
diagnosed: the declaration stays legal and lowers correctly.

**(e) `Wire`/`Json` decode delegates to `parse` rather than matching the
value inline.** `from_json_value` is
`Enum::parse(coerce_*(value)).ok_or(..)` and `to_json` is
`self.value().to_json()`, so neither direction re-implements JSON quoting
and the unknown-value path is §3.9's `Err` by construction. This required
the macro reflection surface to grow the two facts a generator needs:
`Variant.backing` (the literal AS WRITTEN, `""` for none — so an empty
string backing stays distinguishable) and `EnumItem.backing_type`. Both
generators are updated in step, the std macros in `json.vl` (what a real
std runs) and the Rust fallback in `analyzer.rs` (fixture stds and macro
worlds).

**(f) §7.2's refusal searches the whole return type.** `Option<Align>`
and `List<Align>` carry a host-supplied backing value in exactly the same
way as a bare `Align`, and the wrapper path is what each of them wants,
so the check follows generic arguments, tuple elements and array
elements. It does NOT follow a nominal struct's fields: a struct crossing
the boundary is a different hazard, and one the language already has. The
check runs after the walk, so the refusal does not depend on declaration
order. Both backings are refused — §7.2 is about the host boundary, not
about strings — and a sweep found nothing in the tree that flips.

**(g) The `= true` case is refused by the PRODUCTION.** §3.4 rejects
`bool`, and the parser now commits once it sees `=` rather than
backtracking, so the message reads "expected an integer, a string" at the
offending token instead of blaming the `=`.

### 8.3 §4.2's `json.vl` contingency: NOT taken

§4.2 makes `json.vl`'s 15 lines conditional on §7.2 resolving in favour,
and §7.2 was DEFERRED. The deletion would need `external fun kind(self):
JsonKind` — an extern returning a backed enum, which is exactly what the
deferral forbids, and the refusal added in this arc rejects it.

The honest re-read of §4.2's contingency says leave it, and it is left.
The intrinsic being std's own code does not change the shape of the
hazard: `kind()` is a `[extern]` binding to a host helper, and the
compiler cannot tell std's host code from anyone else's. §7.2's own
sentence — "if the ruling is to forbid it, §4.2 loses `json.vl`'s 15
lines and nothing else in the paper changes" — is what happened.

Worth recording for whoever takes §7.2 up again: the sweep found
`kind()` has **no caller outside `json.vl` itself**. Its four predicates
are called only from within the file (13 sites), so the conversion is a
one-file change whenever the deferral lifts, and the two members of the
documented set that never got a predicate (`"object"`, `"null"`) are
still uncovered. `docs/std/encoding.md:67` documents `value.tag(): str`
with `kind()`'s vocabulary in its comment — a pre-existing error, filed
rather than fixed here.

### 8.4 Two limits found, both pre-existing

- **A discriminant past 2^53 is not representable.** `enum E { A =
  9007199254740993 }` emits the JS number literal
  `9007199254740993`, which JavaScript reads as `9007199254740992`. The
  emission is self-consistent (the `match` compares the same literal), so
  nothing in-tree miscompiles — but a value crossing a host boundary
  would. This predates backed enums and is untouched by them; the arc's
  only concession is that `value()`/`parse()` are not synthesized there,
  rather than being synthesized with a return type that lies.
- **bindgen's PROPERTIES keep their TODO.** §4.1 specifies the parameter
  and return directions; a property is both, through separate externs, so
  one bound type cannot serve it. The TODO now names the spellings that
  exist (`Enum::parse(..)` to read, `Enum::Variant.value()` to write),
  which is what it could not say before. Widening the property emitter to
  a raw pair plus two forwarders is a bindgen question, not a language
  one.

### 8.5 §7.1 (`Hashable`) — still open, and now stronger

Unchanged and untouched, as §7.1 recommends. Recording the promised
observation against `hashable-keys.md`: a string-backed enum is a plain
JS string, which a host `Map` keys natively, and "the enum IS the string"
is now the shipped pitch — so `Map<Align, T>` failing on a missing
`Hashable` is the first thing a user will hit. The case for a
compiler-derived `Hashable` on bare-lowered enums is stronger than the
paper could state it.

## 9. The trap arm — a design note for lifting §7.2 (cycle 13)

> **DESIGN NOTE, not a ratification and not an implementation.** §7.2 is
> DEFERRED "until backed enums grow a trap-arm story for the bare-`else`
> hazard". This is that story, written for the owner's queue. Nothing here
> is built; §9.5 is a recommendation to accept or reject, and §9.6 is the
> worked example of what accepting buys.
>
> Probes P11–P16 ran against `target/debug/vilan` built in the
> `docs-trap-note` worktree from `next @92db7d2` — the shipped v0.35.0
> backed-enum implementation, not the paper's model of it. P16 found a live
> hole in the refusal §8.2(f) describes, which changes the weighing.

### 9.1 The hazard, re-measured — and it is narrower than §7.2 states

> **P11.** The shipped emission for an exhaustive three-variant string-backed
> `match`:
>
> ```vilan
> enum Align { Start = "flex-start", Center = "center", End = "flex-end" }
> fun label(align: Align): str {
>     match align { Align::Start => "s", Align::Center => "c", Align::End => "e" }
> }
> ```
> ```js
> if ($a === "flex-start")   { $b = "s"; }
> else if ($a === "center")  { $b = "c"; }
> else                       { $b = "e"; }
> ```
>
> Driving the emitted function directly: `label("middle") === "e"`. §7.2's
> "confidently the wrong variant" is exact, and still true of the build.

> **P12.** The other three ways a backed enum can be tested, same enum, one
> program:
>
> | source | emitted | on an out-of-set value |
> |---|---|---|
> | `a is Align::End` | `$a === "flex-end"` | `false` — honest |
> | `a == Align::End` | `a === "flex-end"` | `false` — honest |
> | `match a { Align::Start => .., _ => .. }` | `if ($a === "flex-start") .. else ..` | takes `_` — honest |

P12 is the useful narrowing and the paper does not currently say it: **the
hazard is confined to the last arm of an exhaustive `match`.** Everywhere
else the feature emits a `===` against a literal, and a `===` against a
literal answers `false` for a value outside the set, which is the correct
answer. There is exactly one construct in the language that converts an
out-of-set value into a confident lie, and it converts it into precisely one
variant: whichever the analyzer ordered last.

That matters for scope. A trap-arm design does not have to guard the
boundary, or the type, or the value. It has to guard one `else`.

### 9.2 A hole in the refusal itself (P16) — and why it re-ranks the candidates

> **P16.** §8.2(f) says the refusal "searches the whole return type". It does
> not search a function-typed **parameter's** parameters, where the host is
> the one constructing the value:
>
> ```vilan
> [extern("onAlignChange")]
> external fun on_align_change(handler: |Align| void): void;
> ```
>
> This compiles clean today — `vilan check` reports no errors — while
> `external fun host_align(): Align` and `external fun align(self): Align` on
> an `external struct` are both correctly refused. Run against a host that
> calls `handler("middle")`, the program prints `e`: `Align::End`,
> confidently, exit 0.

§7.2's premise for allowing the parameter direction is "vilan constructs the
value, so it is always in the set". That premise fails for a **callback**
parameter, which is a return position wearing a parameter's clothes. The
refusal inherited the premise rather than the position, so it enumerates
host-constructing positions and has already missed one.

This should be filed as a bug against the shipped refusal regardless of what
happens to §7.2 — it is a live instance of the exact hazard the deferral
exists to prevent. But it also carries a design argument: **any answer built
on "find the places the host supplies a value" has to be exhaustive over the
language's positions to be worth anything, and one attempt already was
not.** An answer built on "guard the one `else`" does not have to enumerate
anything.

### 9.3 The trap arm already exists in the emission (P13, P14)

> **P13.** A `_` arm on an ALREADY-EXHAUSTIVE backed-enum match is accepted
> today — no unreachable-arm diagnostic — and emits exactly the trap shape:
> every variant gets its own `===` and the `_` becomes the bare `else`.
>
> ```js
> if ($a === "flex-start")    { $b = "s"; }
> else if ($a === "center")   { $b = "c"; }
> else if ($a === "flex-end") { $b = "e"; }   // the last arm, now tested
> else                        { $b = "trap"; } // the trap
> ```

So no candidate below needs a new codegen path. `scalar_variant_test` already
produces both shapes; what a trap arm changes is only whether the compiler
emits the second shape when the author wrote the first. The difference
between the three candidates is **who writes the arm and when** — not what
it compiles to.

> **P14.** The byte delta, measured on three enums (whole emissions, this
> worktree's binary):
>
> | enum | backing | variants | today | with a trap arm | delta |
> |---|---|---|---|---|---|
> | `Align` | `str` | 3 | 211 | 259 | **+48** |
> | `Display` | `str` | 7 | 353 | 392 | **+39** |
> | `Ordering` (`vilan/test/enum-discriminant.vl:15`) | `i32` | 3 | 329 | 368 | **+39** |
>
> The delta is **per match, not per variant** — the 7-variant enum is the
> cheapest of the three. It decomposes as one added `===` test (12 bytes plus
> the last variant's literal as written) plus one `else` block (13 bytes plus
> the trap statement), so a real helper call lands around 50–55 bytes.

The runtime cost is the mirror image and just as small: matching the *last*
variant goes from N−1 comparisons to N. The trap branch itself runs never.

**Corpus-wide, "always trap" costs 39 bytes.** The tree contains exactly ONE
exhaustive match over a backed enum — `vilan/test/enum-discriminant.vl:15` —
across 112 corpus programs totalling 213,335 bytes of goldens, i.e. **0.018%**
of the corpus, in one file. `style.vl` contributes nothing, because §8.1's
rewrite collapsed all eleven wrappers to `.value()`; there is no `match` left
there to pay for. That is the measurement §6's slice 3 would need, and it is
not a cost worth an argument.

### 9.4 The three candidates

**(a) A trap arm at host-tainted values only.**

The value carries a provenance bit from an `external fun`'s return, and a
`match` on a tainted subject gains the arm; everything else emits as today.

On the sub-question of what the arm *does*: **panic, naming the enum and the
raw value** — not an `Option`-shaped result. An `Option`-shaped result would
make a `match` expression's type depend on where its subject came from: the
same three arms are `str` for one caller and `Option<str>` for another. That
is not a trap-arm design, it is an effect system, and it is a much larger
paper than this one. A panic reading `Align: host value "middle" is not one
of its values` is the honest report, and it should emit through the same
`throw` shape `panic()` already uses rather than inventing a second failure
path.

Costs: a taint analysis through the analyzer that survives assignment, field
reads, list elements, closure capture and calls — new machinery in exactly
the two places §1.4 and §1.5 were pleased to leave untouched. Its emitted
size today is **zero**, but only because the refusal makes the taint set
empty; zero is the cost of doing nothing. And per P16 it inherits the
liability that sank the refusal: it must be right about every position from
which a host value can enter, and it makes one `match`'s emission a function
of a fact established elsewhere in the program — the property that produces
bugs reproducing in one program and not another.

**(b) Exhaustive matches on backed enums always emit a trap `else`.**

Costs, all measured above: +39 bytes on the whole corpus; ~50 bytes per match
in user code; one extra comparison when the last variant matches.

What it breaks, concretely: `b76_a_match_on_a_string_backing_is_the_same_chain_a_raw_str_gets`
(`crates/vilan-core/tests/inference.rs:49612`) asserts **byte equality** of
two whole emissions — a backed-enum match against a raw-`str` match with a
`_` arm. Under (b) the backed side gains an arm the raw side does not, and
the pin fails by construction. It **rewrites rather than retires**: give the
raw side its own trap-shaped `_` arm and the equality holds again, since P13
shows the two are the same emission. The claim it protects — §1.4/P2's "a
string backing needs no new codegen path" — survives in a marginally weaker
form (the reference shape becomes "a raw `str` match with a trap arm"), and
that weakening should be written on the pin rather than absorbed silently.

The real objection is philosophical: the compiler proves the match total in
§1.5 and then emits code for the impossible case. The answer is that §1.5's
proof is over the vilan-side *variant set*, and was never a proof about the
runtime *value* — a backed enum lowers to a bare host primitive (§3.5), so
its runtime domain is the host's, not the language's. Rust faces the same
gap on a `repr` enum built from a transmuted byte and answers with `unsafe`;
vilan has no `unsafe`, so a trap is how it pays.

**(c) The boundary stays where §7.2 put it; `json.vl` uses `parse()` at its
own boundary.**

Two readings, and they measure differently.

*As status quo* — leave `json.vl` alone — this is §8.3, already taken.
Re-verified here rather than inherited: `kind()` and its four predicates have
**13 call sites, all inside `json.vl`, and zero callers anywhere in
`vilan/std/src`, `vilan/test`, `vilan/examples` or `vilan/docs`.** The
standing cost is §4.2's 15 lines and 4 functions, plus the two members of the
documented set that never got a predicate (`"object"`, `"null"`) staying
uncovered — `is_null()` does not close that gap, being a separate intrinsic
that tests the value against `null` rather than reading `kind()`.

*As written* — `kind()` actually routing through `parse()` — it is **worse
than doing nothing**:

> **P15.** `fun kind_of(value: JsonValue): Option<JsonKind> {
> JsonKind::parse(value.kind()) }` over the six-member set emits a **425-byte**
> six-arm `===` chain, and every call allocates an `Option` (`[ 0, "number" ]`)
> that each predicate must then unwrap — replacing today's single
> `__json_kind(value) === "number"`.

And neither reading closes P16, which is a hole in the language, not in
`json.vl`.

### 9.5 Recommendation: (b), always trap

**Recommend (b).** Three reasons, in the order they should be weighed:

1. **It is the only candidate that does not have to enumerate anything.** P16
   is the argument: the refusal already tried to name every host-constructing
   position and missed callback parameters. A trap arm asserts what §1.5
   already proved, at the one place P12 shows the proof can be violated — it
   never asks where the value came from. Adopting (b) also makes P16 *moot
   rather than fixed*, because lifting the refusal removes the incomplete
   check along with its hole.
2. **The measured cost does not support the argument against it.** +39 bytes
   on a 213 KB corpus, ~50 per match in user code, one extra `===` on the
   last-variant path, and no new analysis in the analyzer or the transformer
   — P13 shows the emission already exists.
3. **It changes §7.2's answer from "allow it and hope" to "allow it and
   find out".** §7.2 recommended allowing the return direction on consistency
   grounds — `external fun f(): i32` returning `"hello"` is equally unchecked
   — and the deferral was the owner declining that trade. (b) does not
   re-argue it; it removes it. Under (b) a bogus host value is not detected
   at the boundary (nothing is), but it can no longer become a *confident*
   variant: the first `match` that meets it says so, loudly, with the raw
   value in the message. Backed enums end up better checked than `i32`, which
   is an asymmetry worth naming out loud rather than discovering later.

Rejecting (a) is chiefly about the analyzer: it buys a strictly smaller
guarantee than (b) for a strictly larger implementation, and it makes the
emission of a `match` depend on a caller. Rejecting (c) is not a criticism of
§8.3 — leaving it was right while the deferral stood — but (c) is a decision
not to have a trap-arm story, and this note exists because one was asked for.

Slices, if (b) is accepted:

1. The trap arm in `scalar_variant_test`'s exhaustive path, plus the helper.
   One pin per backing (`str`, integer), each proven non-vacuous.
2. Rewrite `b76_a_match_on_a_string_backing_is_the_same_chain_a_raw_str_gets`
   to compare against a raw-`str` match with a trap arm, and record on the
   pin why the reference shape moved.
3. Lift §7.2's refusal — which deletes the check P16 found the hole in. A pin
   that the callback shape now traps rather than lying is the regression test
   for P16.
4. §4.2's `json.vl` deletion (§9.6), and `docs/std/encoding.md` in the same
   commit per the house rule.

Steps 1 and 2 are independent of 3 and 4 and ship on their own; the trap arm
is worth having whether or not the boundary ever opens.

### 9.6 Worked example — `json.vl`'s fifteen lines, and what they actually cost

§4.2's contingency is the right test of the winner because §8.3 already
established it has no external callers, so the whole change is one file.

Under (b), `external fun kind(self): JsonKind` becomes legal, the four
predicates delete, and the 13 in-file call sites become
`value.kind() == JsonKind::Number`. The result is better than §4.2 predicted
in one way and worse in another, and both are worth writing down before
anyone implements it:

- **The 13 sites pay nothing.** P12 measured `==` on a backed enum as
  `$a === "number"` — the same comparison against the same literal
  `is_number()`'s body compiles to today. The four predicate wrappers stop
  being emitted (emission is demand-driven, §8.2(a)), so the rewrite is a
  net *reduction* in emitted bytes as well as the promised −15 source lines.
- **The rewrite pays no trap cost either**, because `==` is not a `match`:
  there is no exhaustive match over `JsonKind` in the rewritten file, so (b)'s
  ~50-byte-per-match cost applies at zero sites in the worked example.
- **§4.2's claim that `"object"` and `"null"` are "covered for free by
  exhaustiveness" does not survive contact with this shape.** Exhaustiveness
  covers a `match`; the 13 sites are `==` comparisons and get no coverage
  from it. Buying that coverage means writing the decode checks *as* a match
  over `JsonKind` — which is a better file, and which is then exactly the
  site that pays (b)'s one added `===` and gains the trap. That is the trade
  §4.2 should have stated, and it is small in both directions.

So the winner's worked example costs `json.vl` nothing and returns 15 lines
and four functions — which is what §4.2 promised, arrived at for a slightly
different reason than §4.2 gave.
