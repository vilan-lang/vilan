# Spec §5 — The type system

## 5.1 Types

The type forms (grammar §3.9) denote:

- **Nominal types**: structs and enums, possibly generic
  (`Task`, `Option<i32>`, `Map<str, List<i32>>`). Two nominal types are
  equal iff they name the same declaration and their arguments are
  equal. There is no structural typing of nominals.
- **Primitives**: `bool`, `str`, `i8 i16 i32 i53 u8 u16 u32 u53`,
  `f32 f64`, `BigInt`. Declared in std as external structs; nominally
  distinct (no implicit numeric conversions, §5.8).
- **Tuples**: `(T, U, …)`; structural: equal iff element-wise equal.
  `()` and one-element tuples do not exist as distinct types (`(T)` is
  `T`; the unit is `void`).
- **Closure types**: `|T, U| R`, `|| R`, `|| void`; structural in their
  parameter and return types. An `async` closure type (§7.4) and a
  `context`-claused type (§8.5) are distinct from their plain
  counterparts.
- **View types**: `&T`, `&mut T` (§6). Views are second-class: these
  types appear in parameter and return positions and in short-lived
  locals only.
- **`void`**: the unit; one value, also written `void`.
- **`any`**: the dynamic top type, produced at host boundaries; it
  unifies with every type (absorbing).
- **`Never`**: the type of diverging expressions (`panic(..)`, `ret ..`,
  `jump break`/`continue`). Never unifies by *yielding*: a diverging
  match leg or if branch doesn't constrain the construct's type, and a
  `Never` value satisfies any expected type. Internal; not written in
  source.
- **Generics**: a bound type parameter in scope (`T`) is a type; it is
  abstract within its binder's body.

## 5.2 `null`

`null` is not a member of ordinary types. It exists for host
interoperability (an extern that may return JS null); std APIs flatten it
at the boundary (`Option`, or a documented sentinel like `storage::get`'s
`""`). A conforming program cannot assign `null` to a non-host type.

## 5.3 Declarations

A `struct` introduces a nominal product type; field types are mandatory
in non-external structs. An `enum` introduces a nominal sum type; each
variant is a constructor (with payload types) and a static member of the
enum. An `external struct` declares a host type: no fields, its surface
defined entirely by externs in impls.

### Enum representation

An enum has one of two runtime representations, and **which one is a
property of the whole declaration, not of a variant**. An enum is
*backed* when both of these hold:

1. every variant is data-less, **and**
2. at least one variant has an explicit backing value.

A backed enum lowers to the bare backing value: `Ordering::Greater` *is*
the value `1` and `Align::Start` *is* the string `"flex-start"`,
comparisons and equality are native scalar comparisons, and a `match`
tests `subject === 1` / `subject === "flex-start"`. Every other enum
lowers to the tagged form `[index, …payload]`, and a `match` tests the
tag slot.

The conjunction is the part worth stating outright, because both halves
are easy to trip over:

```vilan,fragment
enum Level { Low, Mid, High }           // tagged: no explicit backing value
enum Level { Low = 0, Mid, High }       // backed: ONE `= 0` converts all three
enum Level { Low = 0, Mid(i32) }        // rejected: a payload and a backing value
```

Adding `= 0` to a single variant changes the runtime shape of the entire
enum — including how its values cross a host boundary, since a backed
enum reaches an `external fun` as a plain number or string. Backing
values must be unique across the enum, counting the values implicitly
continued from the previous variant; see the grammar chapter for the full
rule.

An **integer** backing value must lie in `-9007199254740991 ..=
9007199254740991` (`i53`), because that is the widest integer a runtime
number holds exactly and the variant *is* that number: a discriminant
past the bound would cross a host boundary as a different value than the
source wrote. The continuation stops at the same edge.

### Backed-enum conversions

Every backed enum gets two members, synthesized by the compiler — no
`derive` marker, because writing `= "start"` is already the opt-in:

```vilan,fragment
enum Align { Start = "flex-start", End = "flex-end" }

Align::Start.value()            // "flex-start" — the backing type
Align::parse("flex-start")      // Some(Align::Start)
Align::parse("middle")          // None
```

`value()` returns the enum's backing type (`str`, or the narrowest of
`i32`/`i53` that holds every discriminant) and costs nothing: the
receiver already *is* that value, so the call lowers to the receiver.
`parse` is a static returning `Option<Self>` — the house form for a
fallible parse, matching `str::parse_i32`. Declaring your own `value` or
`parse` on a backed enum is a duplicate-member error.

Every variant takes part, including one that *continued* the sequence
instead of writing a value. There is one rule about what a variant is
worth, and the conversions read the same answer the lowering does:

```vilan
import std::print;
import std::option::Option::{ self, Some, None };

enum Level { Low = 0, Mid, High }

fun main() {
	print(Level::Mid.value());  // 1 — continued from Low
	print(match Level::parse(2) {
		Some(let level) => level.value(),
		None => -1,
	});                         // 2 — High
}
```

(A **string** backing is the exception, and not a new one: there is no
successor of `"start"`, so every variant must write its own.)

A backed enum is also **`Hashable`**, implemented by the compiler on the
same opt-in and for the same reason `value()` costs nothing: the enum IS
its backing value, and that value is already a key.

```vilan,fragment
mut widths: Map<Align, i32> = Map::new();
widths.insert(Align::Start, 1);          // keyed by "flex-start"
```

So `Map<Align, V>` and `Set<Align>` need no `[derive(Hashable)]`, and
`Align::Start.hash()` is `Align::Start.value().hash()`. Writing the derive
anyway is harmless and does nothing; a hand-written `impl Align with
Hashable` is a duplicate-impl error, because the compiler's is already
there. An **unbacked** enum is unaffected — it lowers to the tagged array,
so it is an aggregate and needs the derive like a struct.

A `resource` enum gets none of the three — no `value()`, no `parse`, no
`Hashable`. Its identity is not its copyable backing value.

Two rules follow from the backing value being a *representation* rather
than a second name for the variant. A `match` still matches variants, not
values — `match align { "flex-start" => … }` is an error, exactly as
`match ordering { 1 => … }` already was. And `<`, `<=`, `>`, `>=` are
rejected on a **string** backing: they would compare the strings
lexicographically (`Size::Large < Size::Small` because `"lg" < "sm"`),
and ordering by declaration index cannot be offered, because bare
lowering erases the index. Integer backings order as before.

An `external fun` may take a backed enum but may **not return one**: the
host can answer with a value outside the set, and an exhaustive `match`
compiles its last arm to a bare `else`, so a bogus value would silently
become whichever variant happened to be last. Bind the backing type and
convert with `parse`, which answers `None`.

## 5.4 Impls

`impl Subject { … }` adds **inherent** members to `Subject`;
`impl Subject with Trait { … }` provides `Trait` for `Subject`. The
subject is a type pattern whose `type X: Bounds` binders declare the
impl's generics:

```vilan,fragment
impl List<type T: PartialEq> { … }      // for every List<T> where T: PartialEq
impl Signal<Signal<type U>> { … }       // only for nested signals
impl type T: Display { … }              // blanket: every T that is Display
```

An impl applies to a concrete type when the pattern matches it and every
binder's bounds hold. Members may be functions (with or without
`self`). A function without `self` is a **static**, reached as
`Subject::name(…)`.

A trait has **one implementation per subject**: writing
`impl Bag with Show` twice is a compile error at the second one, since
nothing would rank them and the second would never run. Two impls are the
same implementation when they name the same trait with the same
arguments for the same subject, up to the naming of their own binders —
so `impl Pair<type T> with Show` and `impl Pair<type U> with Show` are one
impl written twice, and an argument left to a `= Self` default is the type
it defaults to (`with Combine` is `with Combine<Bag>` on subject `Bag`).
A trait parameterized differently is a different implementation:
`impl Bag with Into<Cup>` and `impl Bag with Into<Mug>` both stand.

*Implementation note (tracked): the rule refuses only exact repeats, not
OVERLAP. A blanket impl and a specific one that both match a type — the
`impl type T with Into<T>` of `std::into` beside a type's own `Into` impl,
or two conditional impls with different bounds — are accepted, and which
one a call selects is still decided by declaration order. A specificity
rule is owed.*

*Implementation note (soundness gap, tracked): a conditional impl's
bounds are not yet re-checked when the impl is selected through a
GENERIC bound: `List<B>` can satisfy a `Marker` bound via
`impl List<type T: Marker>` even when `B` lacks `Marker`. Derive-based
checks (`Wire`, `Json`) therefore verify field trees syntactically rather
than through trait bounds.*

## 5.5 Traits

A trait declares required methods (signature-only) and defaults (with
bodies). `trait X with Y` makes `Y` a supertrait: implementing `X`
requires `Y`. A trait's generic parameters may carry defaults
(`trait PartialEq<B = Self>`) and **bounds** (`trait Holder<T: Bound>`);
`Self` in a trait body denotes the implementing type. Traits are used as
**bounds**; a trait is not a type: `let x: Display` is a compile error
(no trait objects).

That rule is enforced **at the annotation**, in every value position — a
binding, a parameter, a return type, a field, a generic argument
(`List<Display>`) — and reported where the trait's name is written,
whether or not the declaration is ever used. A trait's name stays legal
in the positions that name a bound or a namespace rather than a value's
type: a generic parameter's bound (`<T: Display>`), a supertrait, an
`impl` subject (`impl Iterator<type T>`, which blankets over a bound),
and the head of a qualified path (`Display::show(x)`). Inside a trait's
own declaration, write `Self` for "the implementing type"; a generic
parameter defaulted to it (`trait PartialEq<B = Self>`) is a parameter,
not the trait, and is unaffected.

A trait parameter's bound is in scope inside the trait's own default
bodies, exactly as a function's or impl's is inside theirs (§5.6): a
default may call the bound trait's members on a value of that parameter's
type. Each impl supplies the parameter's argument (`impl DogBox with
Holder<Dog>`), whose conformance is checked there, and the default
monomorphizes per implementing type — so the call reaches that type's
argument's implementation, never the bound trait's abstract member.

Trait members resolve on a value when exactly one visible impl provides
the name; a trait default is inherited by impls that don't override it.

## 5.6 Generic binding and inference

Type checking is **expectation-directed**: every expression is checked
against an expected type (possibly unknown), and expectations flow inward
(a `let` annotation to its initializer, a parameter type to its argument,
a field's declared type to its initializer value).

For a call `f(a₁ … aₙ)` where `f` has generic parameters:

1. Each parameter type is unified with its argument's type; unification
   of a generic parameter with a concrete (or caller-generic) type
   **binds** it. Bindings are per-call.
2. A generic mentioned only in the return type is bound by unifying the
   declared return type against the call's expected type
   (`let c: Cell<i32> = Cell::fresh()` binds `T := i32`). Only binders
   that are **free at this call** participate: neither a caller-side
   generic introduced by substitution, nor a binder of a declaration
   enclosing the call — the latter is fixed by the enclosing
   instantiation, and callee and caller can share one outright (a trait's
   parameter, in a call between two of its own members).
3. After binding, every bound's satisfaction is checked; an unsatisfied
   bound is an error naming the parameter and bound.
4. A call whose generics cannot all be grounded (no argument or
   expectation determines them) is an error at the call.

Bounds cut both ways. At a call they are an **obligation** (3 above); at
a declaration they are an **assumption**: inside the body of whatever
declares the parameter — a function, an impl, or a trait — the bound
trait's members are in scope on that parameter's type, and a call to one
resolves through the bound and dispatches at monomorphization to the
implementation the parameter is bound to there.

Method calls additionally bind the receiver's impl binders from the
receiver's type before the parameters are considered. Closure arguments
participate: a closure's parameter types take the callee's expectations,
and its return type may ground the callee's generics; resolution defers
until the closure's body has typed.

Generic code is **monomorphized**: each distinct binding vector of a
generic function/impl produces its own specialization; dispatch is
static. A program that would require an unbounded set of specializations
(polymorphic recursion) is not required to compile.

## 5.7 Operator and method dispatch

The operators dispatch through lang-item traits (appendix §A.4):
`+ - * / %` and the bit/shift operators through `Add`/`Sub`/…;
`== !=` through `PartialEq`; `< <= > >=` through `PartialOrd`. The
left operand's type selects the impl; the trait's `B` parameter types the
right operand (default `Self`); the result type is the impl's (for the
arithmetic traits, `Self`). Compound assignment `x op= e` is exactly
`x = x op e` with `x`'s place evaluated once.

`is` (§3.7 level 10) tests a value against a match pattern and yields
`bool`; bindings inside an `is` pattern are scoped to nothing (use
`match` to bind).

A pattern is checked against the type of the value it matches, so an
enum-variant pattern requires that type to be that enum. A **generic
parameter of an enclosing declaration** is not: `T` is whatever each
instantiation binds it to, and the declaration is checked once for all
of them, so `value is Colour::Red` on a `T`-typed value is a compile
error — in a generic function's body, and in a trait default over the
trait's own parameter. Match a value of the enum's own type, or move the
match to where the parameter is concrete. A bound does not change this:
a trait bound cannot make a parameter be one particular enum.

## 5.8 Conversions and coercions

There are **no implicit conversions** between numeric types; use the
`as_*` methods (value-converting, Rust-`as` semantics). There is **one
implicit coercion**: a reference to a plain named function coerces to a
matching closure type:

```vilan,fragment
let transform: |str| i32 = measure;    // fun measure(text: str): i32
words.map(measure);
```

Eligibility: a non-generic, non-method, non-`async`, non-`external`
`fun` whose signature equals the target closure type. Everything else
(generics, methods, async functions, externs) requires an explicit
wrapping closure.

The same eligibility decides what a **function-typed binding** can do. A
binding that takes its type from the reference rather than an annotation
keeps the function's own type, and calling it calls that function:

```vilan,fragment
let f = measure;                       // fun measure(text: str): i32
let n = f("abc");                      // 3 — arity and argument types
                                       // are the declaration's
```

An ineligible `fun` has no value form at all, so it can be neither
stored nor called this way; the error names which rule it hit.

`any` unifies with every type in both directions (it is produced by
`panic` and host boundaries; it absorbs rather than converts).

## 5.9 Variadic tuples

A generic parameter with a **tuple bound** ranges over tuples:
`T: (2..)` (arity ≥ 2), `T: (..: Display)` (every element `Display`).
A **mapped type** `(U in T: F<U>)` denotes the tuple obtained by mapping
each element `U` of `T` to `F<U>`; `combine`'s signature is the
canonical use:

```vilan,fragment
fun combine<T: (2..)>(sources: (U in T: Signal<U>)): Signal<T>
```

A **tuple comprehension** `(x in xs => e)` is the value-level mapping
form.

Tuple bounds are **enforced** at every binding site, alongside trait
bounds: the bound value must be a tuple, its arity must fall inside the
declared range (endpoints inclusive), and every element must satisfy the
element bound. A generic parameter forwarded into a tuple-bounded
position satisfies it only through its own declared tuple bound: a
contained arity range whose element bound names the same trait or a
subtrait.

A **spread parameter** `...items: T` is a *call convention* over an
ordinary tuple parameter — the call site writes the pack's elements out
flat, and they are collected into that one tuple argument:

```vilan,fragment
fun log<T: (..: Display)>(...items: T)      //  log(1, "hi")  ==  log((1, "hi"))
fun gather<T: (2..)>(...sources: (U in T: Signal<U>)): Signal<T>
```

`T` is the **pack** — the tuple of the collected arguments' types — not
each element's type; a uniform requirement is spelled as the element
bound. The callee side is unchanged in every respect, so a spread
parameter's legal call arities are exactly the arities its declared type
admits, by the rule above: `(2..)` refuses a one-argument call, and `(..)`
accepts a zero-argument one (the empty pack `()`). A spread parameter is
what makes 0- and 1-arity tuple *values* reachable; tuple types already
admit them. Since the convention lives on the declaration, a spread
function used as a **value** has its tuple type, and is called with a
tuple.

Grammar and the positions where `...` is rejected: §3.3.

A **tuple-value spread** `..e` is an entry of a tuple construction that
contributes the *elements* of `e`'s tuple type rather than `e` itself, so
the construction's type is the **concatenation** of its entries, in
written order:

```vilan,fragment
let pair = (1, 2);
let lead = (..pair, 3);      //  (i32, i32, i32)
let mid  = (0, ..pair, 9);   //  (i32, i32, i32, i32)
let both = (..pair, ..pair); //  (i32, i32, i32, i32)
```

Spreads may appear in any position, in any number, mixed freely with
ordinary entries, and a construction whose only entry is a spread is
still a construction (`(..pair)` is the concatenation of one). The
operand must be a tuple; anything else is an error naming the type.
Concatenation is one level deep — a spread operand's own nested elements
keep their nesting, since it contributes its *elements*, not its slots.

A spread is also a **call argument**, which is what lets a pack be
forwarded: since `f(a, b)` means `f((a, b))`, `f(..pair)` means
`f((..pair))`, and the arity bound is checked on the concatenation.
Passing a spread to a function that has no spread parameter builds no
tuple and is an error; write the construction (`f((..pair))`) instead.

A tuple that is still an abstract generic pack has no known element
sequence while the body is checked, so it may be spread only where there
is nothing to concatenate it with — alone, as `inner(..items)`, which is
how a pack is forwarded to another spread function. *`keyof` and the
type-level spread `(..T, U)` are recorded future work.*

**Positional access** `t.0`, `t.1` (chaining as `t.0.1`) types as that
element and, through a `mut` binding, assigns it. Tuples store flat: a
tuple-typed element occupies its elements' slots, so accessing one
yields its region as a value (destructuring reads the same layout).

## 5.10 `!` and `?.`

Both dispatch through lang-item traits and desugar per expression:

- `e!`: **try-assert**. With `v = e` of a type implementing
  `Try<T, B>`: if `v.verdict()` is `Good(t)`, the value is `t`;
  if `Bad(b)`, the enclosing function returns
  `R::from_bad(b)` where `R` is its declared return type (which must
  implement `Try<_, B>` compatibly). `Option` and `Result` implement
  `Try` in std.
- `e?.m…`: **lift**. With `v = e` of a container type implementing
  `Lift` + `Try`: if `v` is good with value `t`, the continuation
  (`.m` and the following plain postfixes, §3.6) applies to `t`; the
  result re-wraps in the container, unless the continuation itself
  yields the container type, in which case it is returned as-is
  (flattening). If `v` is bad, the container passes through unchanged.

```vilan
import std::print;
import std::option::Option::{ self, Some, None };

fun main() {
	let word: Option<str> = Some("dune");
	print((word?.len()).unwrap_or(0));       // 4  — lift + rewrap
	let missing: Option<str> = None;
	print((missing?.len()).unwrap_or(0));    // 0  — bad passes through
}
```

## 5.11 Type errors of note

Normative rejection cases (each is a compile error):

- Using a trait as a type, in any value position — a binding, a
  parameter, a return type, a field, a generic argument (`let x: Display
  = …`, `fun f(v: Display)`, `fun make(): Display`, `struct H { item:
  Display }`, `List<Display>`). Reported at the annotation (§5.5).
- An enum-variant pattern matched against a generic parameter of an
  enclosing declaration (§5.7).
- An unsatisfied bound at a call (`generic parameter 'T' is missing the
  bound …`).
- A `match` whose VALUE legs' types don't unify. Diverging legs
  (`ret`, `panic`, `jump`) are `Never` and don't participate (§5.1).
- An `i53`/`i32` operand mix (no implicit widening; suffix the
  literal).

*Implementation note (tracked gaps): a closure bound to a local and
called directly does not infer its parameter types from the call;
`effect`'s unannotated closure parameter can type against the impl's
abstract `T` (B23); and a closure passed to a **method's** own generic
parameter reaches its body with that parameter still abstract, so a
pattern inside it is not checked at all — the free-function twin
substitutes and is. Each has a pinned test; the workaround is an
annotation or a binding.*
