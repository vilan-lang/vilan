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

### Naming a type through a module

A nominal type is written as a **path** (grammar §3.9's `type-path`):
either a bare name, or the name qualified by the modules that declare it.
`Style`, `style::Style` and `std::style::Style` name the same type; the
segments before the last select namespaces and are resolved exactly as an
expression path's are (names §4.2), so a module in scope reaches its
types the way it reaches its values. This holds in **every** type
position — return type, `let` annotation, parameter, struct field, `impl`
subject, trait bound, generic argument, and nested inside another type
form:

```vilan
import std::reactive;
import std::style;

struct Card {
    style: style::Style,
    hits: reactive::SignalCell<i32>,
}

fun render(card: &Card): style::Style {
    card.style
}

fun main() {
    let card = Card {
        style = style::style(),
        hits = reactive::Signal::new(0),
    };
    print(render(&card).class_list());
}
```

Generic arguments attach to the last segment, the only one that names a
type (`reactive::SignalCell<i32>`); earlier segments are modules and take
none.

A path addresses exactly what its namespace declares, so a segment
naming a member that is not a type — a `fun`, a `let` — is refused where
it stands rather than resolving to that member's own type. The
unqualified form differs here: a bare name in type position may skip a
value binding and keep looking outward (names §4.5), which a qualified
one has nowhere to do.

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

An extern's `[extern(…)]` attribute takes one further trailing flag,
`retains`, declaring that the host keeps an argument past the call rather
than reading it only until the call returns; it composes with every
binding form. It is the one part of an extern's contract the signature
cannot express, and omitting it where it applies is a use-after-free the
compiler cannot warn about — §6.8's
[Externs and retention](memory.md#externs-and-retention) defines it.

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

### The trap arm

A backed enum lowers to a bare host value, so its runtime domain is the
*host's* and not the variant set. Exhaustiveness is checked over that
variant set, by name — which is a proof about the vilan side of the
boundary and never was one about the value. So an **exhaustive** `match`
over a backed enum tests every variant, including the last, and its
`else` traps:

```vilan,fragment
match align {
    Align::Start => "s",
    Align::End   => "e",       // tested, not assumed
}
// a value outside the set panics:
//   Align: "middle" is not one of its values
```

The trap follows the backed enum **wherever the pattern tests it**, not
only when it is the subject. A backed enum reached through a payload is
the same value on the same boundary:

```vilan,fragment
match pair {
    Pair::Of(Align::Start) => "s",
    Pair::Of(Align::End)   => "e",   // tested, not assumed
}
// an out-of-set payload panics, naming the payload's own value:
//   Align: "middle" is not one of its values
```

If one arm tests **more than one** backed enum, the panic names whichever
value actually left its set.

A backed test can also live in an EARLIER arm than the one that becomes the
`else` — a different variant's payload, tested across several arms with no
arm of its own left over for it:

```vilan,fragment
match pair {
    Pair::Of(Align::Start) => "s",
    Pair::Of(Align::End)   => "e",   // together, `Of`'s only handler
    Pair::Other            => "o",
}
// an out-of-set `Of` payload traps instead of silently answering `Other`:
//   Align: "middle" is not one of its values
```

`Other`'s own arm carries no backed test, so the exhaustiveness proof that
drops its condition is still a proof about `Pair`'s VARIANT set, not about
`Align`'s runtime domain — the same gap the payload form above closes, one
level up. The two `Of` arms above are the only place `Align` is ever tested,
so reaching the `else` with the subject's tag actually `Of` is possible only
when its payload left `Align`'s set; the trap fires there, naming `Align`
and the raw value, and `Other`'s own arm still answers for a genuine
`Pair::Other`.

Only the exhaustive form is affected. A `match` you gave a `_` arm keeps
it — an out-of-set value takes the arm you wrote, which is the answer you
asked for — and `is` and `==` compare against a literal, so they answer
`false` outside the set, as they always did. An enum with no backing
value keeps the tagged array form and no trap: the language itself writes
that tag, so there its exhaustiveness proof *is* a proof about the value.
That holds nested too — a `match` whose arms test only unbacked enums
emits exactly what it always did, at any depth.

An `external fun` may both **take and return** a backed enum, and so may
a callback it is handed. Nothing checks the boundary: a host value
outside the set enters unremarked, exactly as it does for an
`external fun f(): i32` that answers `"hello"`. What the trap arm buys is
that such a value can no longer become a *confident* variant — the first
exhaustive `match` to meet it panics with the raw value.

Which of the two shapes to write is a question about the value, not about
safety:

```vilan,fragment
[extern("getAlign")]
external fun get_align(): Align;              // out of set is a BUG — trap

[extern("getAlign")]
[doc(hidden)]
external fun get_align_raw(): str;            // out of set is an INPUT
fun read_align(): Option<Align> { Align::parse(get_align_raw()) }
```

Return the enum where the host's set is genuinely closed and a value
outside it means something is wrong; bind the backing type and `parse`
where an unrecognized value is one of the answers you expect.

## 5.4 Impls

`impl Subject { … }` adds **inherent** members to `Subject`;
`impl Subject with Trait { … }` provides `Trait` for `Subject`. The
subject is a type pattern whose `type X: Bounds` binders declare the
impl's generics:

```vilan,fragment
impl List<type T: PartialEq> { … }      // for every List<T> where T: PartialEq
impl SignalCell<SignalCell<type U>> { … }       // only for nested signals
impl type T: Display { … }              // blanket: every T that is Display
```

An impl applies to a concrete type when the pattern matches it and every
binder's bounds hold. Members may be functions (with or without
`self`). A function without `self` is a **static**, reached as
`Subject::name(…)`.

A binder may be written **inside another binder's bound**, and it declares
the impl's generics exactly like one written in the subject's own
arguments. Its scope is the whole head — the sibling bounds, the `with`
clause, and every member signature:

```vilan,fragment
impl type S: Source<type T> with MaybeSignal<T> { … }
```

reads "for every `S` that is a `Source` of some `T`", and binds that `T`
per receiver: a `Words: Source<str>` instantiates the impl at `T = str`, a
`Counts: Source<i32>` at `T = i32`. A name in a bound that no binder
declares is still unresolved, and is reported where it is written.

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

The rule refuses exact repeats, not OVERLAP: a blanket impl and a specific
one that both match a type — your own `impl type T with Show`
beside `impl Bag with Show`, or two conditional impls with different
bounds — both stand, and a call is answered by the **more specific** of
them, never by whichever was declared first. More specific means, in
order: the impl whose subject pattern the other's matches and not
conversely (`Box<i32>` and `Box<List<i32>>` over `Box<type T>`, and any
named type over a bare `type T`), then — for subjects of the same shape —
the impl whose binders carry the stronger bounds (`Box<type T: Display>`
over `Box<type T>`). The winner brings its own member, which may be the
trait's default body where it declares none.

Two impls that neither subsumes the other — `Box<type T: Display>` and
`Box<type U: Ord>` for a `Box<i32>` that satisfies both — are not ranked.
They are legal to write, and a call that reaches both is reported at the
call site; narrowing one subject is the fix. Two impls with the *same*
subject are refused at the second declaration instead, since no type
could ever separate them — so a second blanket for one trait
(`impl type T with Show` beside `impl type U with Show`) never reaches a
call site at all.

Which trait *instantiation* a call means is decided before specificity is
consulted: `impl Bag with Into<Cup>` and `impl Bag with Into<Mug>` are two
implementations, so `bag.into()` is answered by the one whose result fits
the type the call site expects (`let cup: Cup = bag.into()`). A call with
no such expectation, or one that fits both or neither, is reported rather
than resolved.

### Dispatch through a bound

A call on a generic parameter (`fun f<V: Show>(v: V) { v.show(); }`)
resolves through the parameter's bound and selects an implementation at
monomorphization, by the same order — applicability, then instantiation,
then specificity — with the bound supplying the instantiation the call
site would otherwise have to. That order is total wherever the program
compiles: every such call reaches one implementation's member, or the
trait's default where the winning impl declares none, or the program is
refused before it runs. A binding whose implementations the order cannot
rank is reported at the call that made it, naming both subjects.

A blanket implementation is reachable this way like any other — it is
simply the least specific tier — which is what makes one trait cover a
static value and a reactive one at once:

```vilan,fragment
trait MaybeSignal<T> { fun bind(self, react: |T| void); }

impl type T with MaybeSignal<T> {              // every type, statically
	fun bind(self, react: |T| void) { react(self); }
}
impl SignalCell<type T> with MaybeSignal<T> {      // signals, reactively
	fun bind(self, react: |T| void) { let _watching = self.sub(react); }
}

fun badge<V: MaybeSignal<str>>(label: V) { … }  // takes both, no ceremony
```

A blanket implementation is reachable from a **concrete** type this way,
and never from an **abstract** one. Whether a generic parameter satisfies
a bound is answered from that parameter's own **declared bounds alone** —
no impl is consulted, blanket or otherwise:

```vilan,fragment
trait Wrap<T> { fun unwrap(self): T; }
impl type T with Wrap<T> { fun unwrap(self): T { self } }

fun consume<T: Tag, W: Wrap<T>>(wrapped: W): str { wrapped.unwrap().tag() }

fun main() { consume(3); }                             // fine: `i32` is concrete
fun wrapper<T: Tag>(value: T): str { consume(value) }  // error: `T` lacks `: Wrap<T>`
fun ok<T: Tag + Wrap<T>>(value: T): str { consume(value) }   // declare it, and it holds
```

The blanket covers `T` at every instantiation, so reading it as satisfied
in the abstract body would be sound only until a *more specific* impl
appears — and the specificity order above ranks the blanket last, so the
body would have been checked against an implementation the call does not
reach. Monomorphization is where the question has a real answer, so the
concrete check is the one that counts and a declared bound is what an
abstract call may lean on. The refusal names the parameter and the bound
it lacks, which is the edit that fixes it.

Because the instantiation is decided first, a `SignalCell<str>` reaches the
`Signal` impl under a `MaybeSignal<str>` bound and the blanket under a
`MaybeSignal<SignalCell<str>>` bound, where it is a static value of that
type. The two impls never overlap at one bound: the blanket provides
`MaybeSignal<SignalCell<str>>` there and the `Signal` impl provides
`MaybeSignal<str>`, and `T = SignalCell<T>` has no solution at any finite
`T`. There is no way to write "every type *except* a signal" — a
negative bound would make adding an implementation a breaking change for
everyone who wrote one — and by this order there is no need to.

*Implementation note (tracked): derive-based checks (`Wire`, `Json`)
verify field trees syntactically rather than through trait bounds.*

## 5.5 Traits

A trait declares required methods (signature-only) and defaults (with
bodies). `trait X with Y` makes `Y` a supertrait: implementing `X`
requires `Y`. A supertrait's members are reachable through the
sub-trait — a `T: Ord` value may call `eq` — and they are typed at the
arguments the sub-trait passes the supertrait, not at the supertrait's
own parameters: under `trait Sig<T> with Src<T>`, a `S: Sig<u32>` bound
sees `Src`'s `get(): T` as `get(): u32`. A trait's generic parameters
may carry defaults
(`trait PartialEq<B = Self>`) and **bounds** (`trait Holder<T: Bound>`);
`Self` in a trait body denotes the implementing type. Traits are used as
**bounds**; a trait is not a type, and there are no trait objects: no
value ever has a trait as its type.

That rule is enforced **at the annotation**, in every value position — a
return type, a field, a generic argument (`List<Display>`) — and reported
where the trait's name is written, whether or not the
declaration is ever used. A trait's name stays legal in the positions
that name a bound or a namespace rather than a value's type: a generic
parameter's bound (`<T: Display>`), a supertrait, an `impl` subject
(`impl Iterator<type T>`, which blankets over a bound), and the head of a
qualified path (`Display::show(x)`). Inside a trait's own declaration,
write `Self` for "the implementing type"; a generic parameter defaulted
to it (`trait PartialEq<B = Self>`) is a parameter, not the trait, and is
unaffected.

### A trait annotation on a binding

A `let` binding's annotation is one of two exceptions — the other is a
parameter's, below — and neither is an exception to the rule above but an
application of it: a trait written here is a **checked constraint**, not
the binding's type.

```
let count: Signal<i32> = SignalCell::new(1);
```

`count`'s type is `SignalCell<i32>` — the type its initializer infers,
exactly as if nothing had been written. The annotation neither widens it
nor boxes it; it asserts that whatever type the initializer produces
implements `Signal<i32>`, and is a compile error when it does not. This
is the bounded-generic rule (§5.6) in binding position: **checked wide,
kept narrow**, one concrete type per binding. Reading `count`'s members
therefore reaches `SignalCell`'s own — its fields included — and a
reassignment must still be a `SignalCell<i32>`.

The reading is universal: any trait name in this position, for every
trait. It applies to the binding's OWN annotation only — a trait nested
inside one (`&Display`, `List<Display>`) is a value position like any
other and is refused, which is what keeps a heterogeneous container
impossible.

An `if` needs no rule of its own. Its arms unify first (§5.11), and the
constraint meets the one type that unification produced:

```
// legal — both arms are SignalCell<i32>
let cell: Signal<i32> = if c { SignalCell::new(1) } else { SignalCell::new(2) };
// refused at the ARMS, as an ordinary mismatch: two concrete types,
// each implementing Signal<i32>, still do not unify
let cell: Signal<i32> = if c { SignalCell::new(1) } else { OtherSignal::new(2) };
```

### A trait annotation on a parameter

A parameter's annotation is the other position that takes a trait name,
and it means something different from a binding's: an **implicit generic
parameter**.

```
fun render(cell: Signal<i32>): str { ... }   // == fun render<T: Signal<i32>>(cell: T)
```

The two readings differ because their quantification does. A binding has
one initializer and so one concrete type, which it keeps. A parameter has
one type per call site, so the function is checked **once, against the
bound**, and monomorphized per call like any generic — which means the
body reaches the trait's members and not the argument's own. Everything
else follows from the desugaring, and nothing about it is new:

- Each annotation is its **own** generic. `fun f(a: Show, b: Show)` has
  two type parameters and its two arguments may be different types; a
  function that needs them equal writes one generic and uses it twice.
- The trait's arguments are the bound's: `x: Signal<i32>` bounds the
  parameter by `Signal<i32>`, not by `Signal<T>` for a free `T`.
- The implicit parameter is **appended** after every generic the
  declaration writes, so explicit generic arguments keep their meaning.
- Written and implicit generics mix freely in one signature.
- A **closure** parameter takes no trait: a closure has no generic
  parameters, so the annotation is refused there like any other value
  position.

The reading is the binding's in one respect: it applies to the
parameter's OWN annotation, never to a trait nested inside one
(`List<Display>`), which stays refused. A `&` is not such a nesting — it
is a call convention, erased before the annotation is read — so
`&Display` is "a view of something implementing `Display`" at a parameter
and at a binding alike.

### Associated functions

A trait may declare **associated functions**: `fun`s with no `self`
receiver, with or without a default body. They are a namespace, not a
dispatch — there is no receiver to select an implementation with.

```
trait SignalCell<T> {
    fun new(initial: T): SignalCell<T> { SignalCell { value = initial } }
}
```

`Trait::func(..)` calls **the trait's own default body**, always. An impl
may override an associated function, and that override is reached through
the implementing type's path, `Type::func(..)` — never by re-pointing the
trait's spelling at it. (This is the opposite of `Trait::method(receiver)`
for a `self` method, §5.7, which exists precisely to name the trait a
receiver dispatches through: with a receiver there is something to select,
and without one there is not.)

Two consequences follow, and both are compile errors rather than
surprises. `Trait::func(..)` where the trait declares `func` without a
default body names a per-impl requirement with nothing behind it, and is
refused, naming both spellings. And a default body is not inherited onto
an implementing type's path: `Type::func(..)` reaches that type's own
declaration or nothing, and the refusal names the trait's spelling.

The trait's own generic parameters bind from the call, like any generic
function's: `Signal::new(7)` binds `T = i32`.

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

A written **type application supplies exactly the arity its declaration
declares**, in every position an annotation can occupy — a parameter, a
return type, a `let` annotation, a field, a generic argument, an `impl`
head subject, a trait bound's argument. Naming `Holder` for a
`struct Holder<S>` is an error, not a request to infer `S`: annotations
are checked, never inferred, so a missing argument has nothing to come
from. A parameter with a **default** (`<B = Self>`, §5.5) supplies
itself, so an application may omit exactly the trailing arguments whose
parameters default. Supplying too many is the same error. The head of a
qualified path (`Option::None`, `List::new()`) names a namespace rather
than a type and carries no arity of its own.

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
3. A generic mentioned only in a SIBLING parameter's parameterized bound
   (`fun f<T: PartialEq, S: Source<T>>(source: S)`) is bound once that
   sibling is: the bound's arguments are recovered from the
   implementation the sibling's type provides for the bound trait
   (`SignalCell<i32>: Source<i32>` binds `T := i32`). The arguments are read
   at that implementation's OWN binders, so an argument whose type is
   caller-generic comes through as the caller's parameter — bounds
   intact — exactly as a concrete one comes through as the concrete
   type: under `impl SignalCell<type Z> with Source<Z>`, a `SignalCell<T>`
   receiver binds `Z := T` and the bound's argument is the caller's `T`,
   never the impl's `Z`.
4. After binding, every bound's satisfaction is checked; an unsatisfied
   bound is an error naming the parameter and bound.
5. A call whose generics cannot all be grounded (no argument or
   expectation determines them) is an error at the call.

A generic parameter is **rigid inside its own body**. The caller chose it,
once, for this instantiation; nothing in the body may choose again. So a
parameter's type unifies with itself and with nothing else: assigning a
value of one parameter's type to a binding of another's is a mismatch,
and passing it where a concrete type is declared is a mismatch, exactly
as `str` and `i32` are. The parameters a site may bind are the ones it is
*inferring* — a callee's at a call, a struct's at a literal, an enum's at
a constructor, a declaration's own at an `impl` head — never one an
enclosing `fun`, `impl` or `trait` declares. A parameter that carries a
bound still **satisfies** a position declared as that trait (the bound is
what it promises); satisfying is not binding.

Bounds cut both ways. At a call they are an **obligation** (4 above); at
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

**Return-type inference.** A function with no declared return type takes
its type from its body's return positions — the tail, when the body can
reach it, and every `ret` — and they must agree. A tail the body cannot
reach (every path before it leaves by `ret`) is not a return position, so
`fun f(x: bool) { ret 1; }` is `i32`; a tail it can reach is one, so a
`ret 1` beside an `if` with no `else` disagrees with the void that path
produces. A bare `ret` is a void return; it agrees only with a void body.
A disagreeing `ret` is an error at that `ret`, naming both types and
where the inferred one came from — the function then has no type, so
the error is not repeated at its calls. A call the function makes to
itself contributes nothing (its type is the one being inferred); a
function whose only return positions are such calls is `Never`. Declaring
the return type replaces inference with checking (every position against
the declaration). A closure (and an `async` block) infers the same way:
its return type is the unification of its reachable tail and every
`ret`, so `|x| { ret x * 2; }` is `|i32| i32`, and a `ret` that
disagrees — with the tail, an earlier `ret`, or a body path that ends
without a value — is an error at that `ret`. When the closure's return
type is known ahead of the body (its own annotation, or the call site's
expectation), the `ret`s check against that type instead, exactly as a
declared function's do.

```vilan
fun sign(x: i32) {
	if x > 0 {
		ret 1;
	}
	if x < 0 {
		ret -1;
	}
	0
}

fun main() {
	let s: i32 = sign(-4);
	print(s);
}
```

```vilan,fragment
fun f(x: bool) { if x { ret "s"; } 2 }   // error at `ret "s"`: the tail is i32
fun g(x: bool) { if x { ret; } 2 }       // error at `ret`: a bare ret is void
fun h(x: bool) { if x { ret 1; } }       // error at `ret 1`: the if can produce void
```

## 5.7 Operator and method dispatch

The operators dispatch through lang-item traits (appendix §A.4):
`+ - * / %` and the bit/shift operators through `Add`/`Sub`/…;
`== !=` through `PartialEq`; `< <= > >=` through `PartialOrd`. The
left operand's type selects the impl; the trait's `B` parameter types the
right operand (default `Self`); the result type is the impl's (for the
arithmetic traits, `Self`). Compound assignment `x op= e` is exactly
`x = x op e` with `x`'s place evaluated once.

The left operand's *shape* does not enter into it. A tuple or an array
is an impl subject like any other (`impl (i32, i32) with PartialEq`),
and one without the impl is the same error a struct without it is — the
operators are never the host's. `void` is refused outright: an
expression that produces no value has no operand to be.

The primitives do not dispatch — native machine operators *are* their
semantics — so their admitted operand pairs are stated rather than read
off an impl. For `+` those pairs are exactly two:

- `str + x`, **concatenation**, where `x` is a `str`, a numeric
  primitive, or a `bool` — the types whose value is already their
  rendering — or a generic parameter whose bound provides
  `to_string(self): str`, which is that same rendering promised rather
  than built in. This is the rule an interpolated string is checked
  against, since `i"a{x}b"` *is* a `+` chain (§2.3).
- `T + T` for a numeric primitive `T`, ordinary addition, with no
  implicit conversions (§5.8).

Anything else is an error. A struct, an enum, a `List`, an `Option` or a
tuple has no string form, so it does not concatenate: render it first
with `to_string()`, implementing `Display` if it has none. The order
matters, because the expression takes its type from the left operand:
`"n=" + count` concatenates, `count + "n="` does not. And `bool` and
backed enums, native though they are for `==` and `<`, have no `Add` at
all — a backing value is a lowering detail, not a number to compute
with.

A **generic parameter** admits exactly what its bounds promise, and no
more: a declaration is checked once for all its instantiations, so the
bounds are the whole of what the operand is known to be. Whether that is
enough is decided by the LEFT operand, because what the right one has to
show is *membership* of the set the left admits — and a bound can show
membership only where a trait names that set.

`str`'s set has one. A bound providing `to_string(self): str` promises
the string form, so the parameter concatenates, and the concatenation
*calls* that implementation at each instantiation: the promise is kept,
not assumed. An unbounded parameter promises nothing and a bound
promising something else (`T: Add`) promises the wrong thing; both are
refused, for the same reason.

A number's set has no such trait. `i32 + i32` is the only admitted pair,
nothing names "is an `i32`", and a bound promises a trait's *methods*
rather than a type — `Add`'s `B` defaults to `Self`, which is the
parameter, not the left operand. So a parameter is never an admissible
right operand of a native operator over a number: not of `+`, and not of
any sibling (`-`, `*`, the bit and shift operators, `==`, `<`), whatever
it is bounded to. The same holds for `str`'s own comparisons, which want
a `str` and have no trait naming that either. Convert where the type is
known and declare the operand concretely.

A parameter on the LEFT is a different question, and there the bound is
**required**. The operator dispatches through its left operand, so the
bound is what selects the implementation to run; a parameter without one
has no implementation to dispatch to, and the operator is refused. What
admits it is a bound that **provides the operator's method** — the
parameter's own bound, or one reached through a supertrait: `+` needs
`T: Add`, `-` needs `T: Sub`, `==` and `!=` need `T: PartialEq`, the
four orderings need `T: PartialOrd`, and so on for every operator that
models a trait. Merely being bounded is not enough, for the same reason
it is not enough on the right: `T: Display` promises `to_string`, not
`add`.

Inside a trait's own default body the rule is unchanged, and the
parameter is then the **trait's**. The bound goes on the trait, so the
refusal names it: adding it there moves every `impl` of the trait and
every bound that mentions it, which is not a local edit the way a
function's own parameter is.

`&&` and `||` are the one family with no spelling that works. They admit
`bool`, they model no operator trait at all, and no trait names that set
— so a parameter is refused on **either** side, whatever it is bounded
to, and the fix is to change the type rather than to add a bound.

```vilan,fragment
"n=" + count                 // str + i32 — concatenation
"p=" + point                 // error: `Point` has no string form
"p=" + point.to_string()     // the fix the error names
count + "n="                 // error: only a `str` LEFT operand concatenates
1.5 + count                  // error: f64 and i32; no implicit conversions
```

```vilan,fragment
fun show<T>(value: T): str { "v=" + value }           // error: `T` is unbounded
fun show<T: Add>(value: T): str { "v=" + value }      // error: no bound provides `to_string`
fun show<T: Display>(value: T): str { "v=" + value }  // the impl is called
fun show<T: Display>(value: T): str { i"v={value}" }  // the hole is the same expression
```

```vilan,fragment
fun bump<T: Add>(total: i32, value: T): i32 { total + value }  // error: `T` is wider
fun bump<T: Add>(total: i32, value: T): bool { total < value } // error: same, for `<`
fun sum<T: Add>(first: T, second: T): T { first + second }     // the parameter is on the LEFT
```

```vilan,fragment
fun sum<T>(a: T, b: T): T { a + b }                 // error: `+` on `T` needs `T: Add`
fun sum<T: Display>(a: T, b: T): T { a + b }        // error: `Display` does not declare `add`
fun sum<T: Add>(a: T, b: T): T { a + b }            // the bound provides `add`
fun same<T: PartialEq>(a: T, b: T): bool { a == b } // one bound per operator
fun smaller<T: Ord>(a: T, b: T): bool { a <= b }    // a supertrait's `le` counts
fun both<T>(a: T, b: bool): bool { a && b }         // error: no bound can prove `bool`

trait Doubler<T> {                                  // error: the bound goes on the TRAIT
	fun once(self): T;
	fun twice(self): T { self.once() + self.once() }
}
trait Doubler<T: Add> { … }                         // the spelling that works
```

Where the left operand *does* dispatch, the same membership question is
answered by the impl instead of by this section: the right operand must
be admitted by the `B` **that impl declares**, checked against the
signature the dispatch will actually run. That `B` is whatever the impl
wrote — a type of its own (`impl Meters with Add<Feet>` admits a `Feet`
and nothing else, `Meters` included), one of the impl's own parameters
(`impl Bag<type T> with Add<T>`, whose admitted operand is whatever the
subject bound `T` to), or `Self`, spelled or reached through the
trait's `B = Self` default. A generic right operand refuses here for the
reason it refuses over a number, one level along: a bound promises a
trait's methods, never that the parameter *is* the declared `B`. The
spelling that works is the one where the impl's `B` **is** that same
parameter, which is what a generic subject's own impl gives.

```vilan,fragment
counter + point                  // error: `Counter`'s `add` accepts `Counter`
metres + feet                    // ok:    `impl Meters with Add<Feet>`
metres + metres                  // error: that impl's `B` is `Feet`
bag_of_i32 + "x"                 // error: `Bag<i32>`'s `add` accepts `i32`

fun bump<T: Add>(counter: Counter, value: T): Counter { counter + value }
// error: `Counter`'s `add` accepts `Counter`, and a bound cannot prove `T` is one
fun bump<T: Add>(bag: Bag<T>, value: T): Bag<T> { bag + value }
// ok: `impl Bag<type T> with Add<T>` declares the very parameter as its `B`
```

`is` (§3.7 level 10) tests a value against a match pattern and yields
`bool`, and a `let` binding inside the pattern **captures**. Its scope is
everywhere the test is known to have passed, and nowhere else: the
**then-branch**, and the rest of the condition **to the right of an
`&&`**. Not the `else` branch, where the test failed; not after the `if`,
where nothing is known; and not the other arm of a `||`, which runs
exactly when the test failed — nor, for a capture under a `||`, the
then-branch, since reaching it proves only that *some* arm was true.
Outside its scope the name is simply unbound, and reading it is the
ordinary "cannot find" error.

A **negation swaps the two branches**, and nothing else: `!(x is P)` is
true exactly where `P` failed, so the capture is *not* in that `if`'s
then-branch — it is in the **`else`** branch (and on down an `else if`
chain), which is reached precisely when the pattern matched. Two
negations cancel. The `&&` rule is unchanged *inside* the negation —
`!(x is P && …)` still binds the right operand, which the short-circuit
reached by matching — but a negated capture does not cross an `&&` it
sits to the left of, because `&&` carries only its left operand's true
side. A capture under a `||` stays unbound in both branches. Binding the
continuation after a diverging then-branch (`if !(x is P) { ret; }`) is
not part of this rule; the name is unbound after the `if` as usual.

```vilan,fragment
if slot is Some(let n) { use(n); }                // yes: the test passed
if slot is Some(let n) && n > 0 { use(n); }       // yes: `&&` short-circuits
if slot is Some(let n) { … } else { use(n); }     // error: unbound here
if slot is Some(let n) || n > 0 { … }             // error: unbound here
if slot is Some(let n) { … } use(n);              // error: the `if` ended

if !(slot is Some(let n)) { use(n); }             // error: the test failed
if !(slot is Some(let n)) { … } else { use(n); }  // yes: it matched
if !(!(slot is Some(let n))) { use(n); }          // yes: the negations cancel
if !(slot is Some(let n) && n > 0) { … }          // yes for `n > 0`, no for the branch
if !(slot is Some(let n)) && n > 0 { … }          // error: unbound here
if !(slot is Some(let n)) { ret; } use(n);        // error: the `if` ended
```

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
fun combine<T: (2..)>(sources: (U in T: SignalCell<U>)): SignalCell<T>
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
fun gather<T: (2..)>(...sources: (U in T: SignalCell<U>)): SignalCell<T>
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

- Using a trait as a type, in any value position — a parameter, a return
  type, a field, a generic argument (`fun f(v: Display)`, `fun make():
  Display`, `struct H { item: Display }`, `let xs: List<Display> = …`).
  Reported at the annotation (§5.5). A `let` binding's OWN annotation is
  not this case: there a trait is a checked constraint on the inferred
  type (§5.5).
- A `let` binding whose inferred type does not implement the trait its
  annotation names (`let x: Display = bag`, `Bag` having no `Display`
  impl). Reported at the annotation (§5.5).
- `Trait::func(..)` for an associated function the trait declares without
  a default body, and `Type::func(..)` for one the type's impl does not
  declare (§5.5).
- A type application whose argument count is not the arity its
  declaration declares — too few (`fun read(h: Holder)` for
  `struct Holder<S>`) or too many — in any annotation position.
  Reported at the annotation, naming the arity and the spelling that
  fixes it (§5.6). Trailing arguments whose parameters have defaults may
  be omitted.
- An enum-variant pattern matched against a generic parameter of an
  enclosing declaration (§5.7).
- An unsatisfied bound at a call (`generic parameter 'T' is missing the
  bound …`).
- A `match` whose VALUE legs' types don't unify, and — by the same rule
  — a value `if` (one with a final `else`, §5.1) whose arms' don't. An
  `else if` chain is one construct: every arm unifies with the rest, not
  just with its neighbour. Diverging legs and arms (`ret`, `panic`,
  `jump`) are `Never` and don't participate (§5.1).
- An `i53`/`i32` operand mix (no implicit widening; suffix the
  literal).
- A value written into a struct field it does not match, through either
  door: the literal `S { field = v }` and the assignment `s.field = v`
  are governed by ONE rule, and a place chain of any depth (`a.b.c = v`,
  `list[i].f = v`) is the same door. A compound `s.field op= v` is
  checked on what lands — the result of `op` (§5.7) — not on `v`. The
  error is reported at the value.

*Implementation note (tracked gaps): a closure bound to a local and
called directly does not infer its parameter types from the call, and
`effect`'s unannotated closure parameter can type against the impl's
abstract `T` (B23). Each has a pinned test; the workaround is an
annotation or a binding. (A closure passed to a method's own generic
parameter was a third such gap — its body reached the checker with the
parameter still abstract, so a pattern inside it was not checked at all.
Both call paths now bind from the non-closure arguments and defer before
typing any closure, so the substitution has landed by the time the body
is read.)*
