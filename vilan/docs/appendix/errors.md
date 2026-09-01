# Error index

You saw an error; this page says what it means and where to go. Messages
are quoted the way the compiler prints them, with `…` standing in for the
parts that vary. Find yours with a page search.

(Organized companion: the [gotchas checklist](gotchas.md) covers traps by
topic rather than by message.)

This index is a **curated subset**, not a catalogue: the compiler has some
three hundred message forms and this page carries around a hundred. An
entry is here when all three hold — a plausible program reaches the
message; reading it alone does not settle what to do next; and no entry
already covers its family (one entry per family, its arms behind the `…`).
The full list, with a verdict and pins per message, is the diagnostics
ledger, and `crates/vilan-cli/tests/diagnostics_ledger.rs` holds this page
against it: every quoted message here must still be one the compiler
prints, and every message the ledger marks as documented must still be
quoted here.

## Names and imports

**"cannot find '…' in this scope"** · **"cannot find type '…'"**
The name isn't visible here. Usually a missing `import` — though the
basics (`print`, `Option`/`Some`/`None`, `Result`/`Ok`/`Err`) are in the
prelude and need none. If you did import it, check for a typo or a
shadowing local.
→ [Hello Vilan](../tour/hello-vilan.md), [spec §4.7](../spec/names.md)

**"… is in the prelude of the web set — set `prelude = \"std::web\"`"**
The name (`Signal`, `view`, `View`) is one std's **web**
prelude makes ambient, and this package is on the base one. Either set
`prelude = "std::web"` in `vilan.toml`, or import the name explicitly —
both work — the steer fires only for names the web set carries as bare
members, never for its module-carried names (`style`, `ui`), where
switching preludes would leave a value-position miss unfixed — and it
only means the manifest line is usually what
you wanted.
→ [Projects](../tour/projects.md), [spec §4.7](../spec/names.md)

**"`std` is a namespace, not a value; import the module first …"**
You wrote a qualified path like `std::math::min(1, 2)` inline. That
spelling isn't supported. Import the module, then qualify through its
name: `import std::math;` and `math::min(1, 2)`.
→ [Hello Vilan](../tour/hello-vilan.md)

**"`…` requires the `…` layer of `std` and cannot run on `…`"**
Code reachable from this build's entry calls into a module the platform
doesn't have: `std::fs` from a browser build, `std::dom` from a Node
build. The error lists the call chain from `main` to the crossing.
Importing the module is not the problem (imports are free); reaching it
is. Move the call behind the right entry, or check the package's
`target`.
→ [Platforms](../tour/platforms.md)

**"`…` requires … and cannot run on `…` / reachable from `…`, fenced `[platform(…)]`"**
A function declared a platform fence and something it (transitively)
reaches requires a layer one of the fenced platforms doesn't serve. The
chain shows the path from the fence. Fences check on every compile.
Narrowing the fence, or moving the colored call out from behind it, are
the two fixes.
→ [Platforms](../tour/platforms.md)

**"`std::…` was removed: …"**
Sixteen aliases lived at the `std` root for one release — `std::print`,
`std::panic`, `std::Default`, the primitives — and the prelude serves the
same purpose better, so they were deleted. The message names the way
forward for the one you wrote: a primitive (`str`, `i32`, …) is always in
scope and needs no import at all, `print` is in the default prelude, and
anything else has its real module path (`std::io::panic`,
`std::default::Default`).
→ [Hello Vilan](../tour/hello-vilan.md), [spec §4.7](../spec/names.md)

**"`…` is a reserved package name: …"**
`std`, `pkg`, `macro_std` and `vilan` each already mean something as an
import root, so a `[package] name` or a dependency key cannot claim one —
a dependency named `std` used to REPLACE the standard library silently.
Rename the package; or, for a dependency, rename the key, which is only
the name you import it by and is free to differ from the library's own.
→ [Projects](../tour/projects.md)

**"cannot find module '…' to import"**
The path names a module file that doesn't exist. `pkg::routes` means
"`routes.vl` in this package's source root". Check the file name and
the package you're in.
→ [Hello Vilan](../tour/hello-vilan.md)

**"module '…' resolved to '…' on disk, but it is imported as '…'"**
Your filesystem ignores case (NTFS, and macOS by default) and answered
the import with a differently-cased file. Module names match byte for
byte (§4.2), so this would fail to build on a case-sensitive
filesystem. Rename the file or the import so the two agree.
→ [Names, modules, and packages](../spec/names.md)

## Types and generics

**"Expected …, but got … instead."**
The general type mismatch. One special case surprises people: an `i53`
mixed with a bare integer literal: the literal is `i32`, and there are
no implicit conversions. Suffix it (`stamp + 1000i53`).
→ [Values and types](../tour/values-and-types.md)

**"generic parameter '…' is missing the bound ': …' required by this call"**
You called something that needs a capability (say `PartialEq`) with a
generic parameter that doesn't declare it. Add the bound to *your*
signature: `fun caller<U: PartialEq>(…)`.
→ [Data and traits](../tour/data-and-traits.md)

**"cannot call method '…' on …"**
The value's type doesn't have that method. If the type is a generic
parameter, you probably need a bound. If it says something like
`|i32| i32`, you're calling a method on a closure, often a sign a
different value was passed than you think.
→ [Data and traits](../tour/data-and-traits.md)

**"'…' does not implement trait '…': missing '…'"**
An `impl … with Trait` doesn't provide every required method, or a bound
demands a trait the type never implemented.
→ [Data and traits](../tour/data-and-traits.md)

**"'…' is a trait, not a type: a trait is not a value type (vilan has no trait objects)"**
A trait's name was written where a type belongs — a parameter, a return
type, a struct field, or a generic argument like `List<Display>`. Traits
are **bounds**, not types, so no value can ever have that type: the impl
is fine, the signature is not. Write the generic the message spells out —
`fun show<T: A>(v: T)` — or, inside the trait's own declaration, write
`Self`, which is what a trait naming itself in a return position always
meant. The note points at the trait, which may live in another module.
For "one of several things at runtime", use an enum. A `let` binding's
own annotation is not this error: there a trait is a *constraint* on the
inferred type, see the next entry.
→ [Data and traits](../tour/data-and-traits.md)

**"'…' does not implement trait '…', required by the annotation on '…'"**
A `let` binding's annotation named a trait, which reads as a constraint
on the value's own type — the binding still has the concrete type its
initializer produced — and that type has no impl of the trait. Either
implement it, or annotate with the type you meant. Note that two `if`
arms of different types fail earlier, at the arms: the annotation is not
a widening, so there is nothing for two types to meet in even when both
implement the trait.
→ [Data and traits](../tour/data-and-traits.md)

**"'…::…' has no default body, so '…::…(..)' has nothing to call"**
An associated function (a trait `fun` with no `self`) was called on the
trait, but the trait only declares it — each impl supplies its own body,
and with no receiver there is nothing to pick between them. Call it
through an implementing type, `Type::func(..)`, or give the trait's
declaration a default body, which is what makes the trait's own spelling
callable.
→ [Data and traits](../tour/data-and-traits.md)

**"cannot call '…' on a value of bare trait type '…'"**
The same rule reached from the other side: a receiver whose type is a
bare trait has no concrete implementation to dispatch to. Reachable
inside an `impl` whose subject is itself a trait, where `self` is
abstract; elsewhere the annotation that produced the value is refused
first.
→ [Data and traits](../tour/data-and-traits.md)

**"'…' is already defined for '…'; remove or rename this one"**
Two impls declare the same name for the same type, and neither name
belongs to a trait. Nothing ranks them, so one of the two would simply
never run — the note points at the other declaration. Delete the copy
you don't want, or rename it. It is reported where it is defined, not
where it is called.

A type has one namespace, so **receiver position is not part of the
name**: a static `fun new()` and a method `fun new(self)` for the same
type collide with each other too. Give one of them a different name.
→ [Names, modules, and packages](../spec/names.md)

**"'…' is already implemented for '…'; remove or merge this impl"**
The same trait is implemented twice for the same type. A trait has one
implementation per type, so the second block would simply never run —
neither at `value.method()` nor through a `T: Trait` bound. Merge the two
bodies into one impl, or delete the one you don't want; the note points
at the first, and names its module when it lives in another file.

Only an exact repeat is refused. A parameterized trait may be
implemented once per set of arguments — `impl Bag with Into<Cup>` and
`impl Bag with Into<Mug>` are two implementations, not one written twice
— and an argument you leave to a `= Self` default counts as the one it
defaults to, so `with Combine` and `with Combine<Bag>` are the same
implementation of `Combine` for `Bag`.
→ [Data and traits](../tour/data-and-traits.md)

**"'…' is ambiguous on '…': both '…' and '…' provide it; call '…' to pick one"**
Two traits supply the same method name for this receiver (or, for a
generic receiver, two arms of its `T: A + B` bound), and the type has no
inherent method of its own to outrank them. Say which one you mean with
`Trait::method(receiver, …)` — the message spells both options out with
your own receiver already substituted in.
→ [Names, modules, and packages](../spec/names.md)

**"`next` is ambiguous on '…': both '…' and '…' provide it…, and a `for` loop has no spelling that names one"**
The loop's counterpart to the message above, for the iterator protocol
(`next`, or `next_mut` for `for x in &mut subject`). Two traits provide
the member — declaring it, or supplying it as an inherited default — and
no inherent member outranks them. A call can pick a provider with
`Trait::next(receiver)`; a `for` has no such spelling, so the fix is the
one the message names: declare `next` on the type itself, where it beats
every trait-provided one.
→ [Collections](../std/collections.md)

**"'…' is not an inherent member of '…': … provide… it; call … instead"**
`Type::method(receiver)` means the type's *own* method. This one comes
from a trait, so name the trait at the path head instead:
`Trait::method(receiver)`.
→ [Names, modules, and packages](../spec/names.md)

**"'…' does not implement '…', so '…::…' cannot be called on it"**
A `Trait::method(receiver)` call named a trait the receiver's type has no
`impl … with` for. Implement the trait, or call the method the receiver
does have.
→ [Data and traits](../tour/data-and-traits.md)

**"… match the receiver convention"** · **"… match the parameter convention"** · **"… match the declared type"** · **"… match the declared return type"** · **"… match the declared parameter list"** · **"… match the trait's type-parameter list"**
A method an `impl … with Trait` provides must match the trait's
declaration, not just its name: the receiver convention (`self` / `&self`
/ `&mut self` / `own self`), the parameter count and each parameter's
convention and type, the return type, and, for a generic method, the
type-parameter count. Types are compared with `Self` read as the impl's
subject and the trait's generic parameters read as the `with`-clause
arguments (`impl Meters with From<Feet>` expects `fun from(value: Feet):
Meters`). When the trait's parameter has a `Self` default and you supply
no argument, it reads as the subject: `impl Meters with Add` expects
`fun add(self, b: Meters): Meters`, while `impl Meters with Add<Feet>`
expects `fun add(self, b: Feet): Meters` — the argument changes, the
`Self` return does not. A generic method's own type parameters are held
to the trait's promise too: declaring `fun go<T>(&self, x: T)` and then
implementing `fun go<T>(&self, x: str)` narrows what the trait promised
to accept, and is rejected. Asyncness is not required to match: an async
impl of a synchronous trait method is allowed (dispatch is monomorphized,
so callers await it regardless).
→ [Data and traits](../tour/data-and-traits.md)

**"match is not exhaustive: missing …"** · **"match is not exhaustive: add a catch-all `_` leg"**
Some values have no arm. Handle them or add `_ => …`. This error is
the feature: it's what fires everywhere when you add a variant. A
**guarded** leg does not count towards it — a guard tests the value, and
the check reasons about the type — so `B if ready => …` leaves `B`
missing, and a note points at the guard to say so. That also means the
last leg may not be guarded: give it a `_ => …` after it, so the value
the guard rejects has somewhere to go.

The hole may be **below** the top level, and then the message names one
uncovered value as a pattern that would cover it —
*"missing `Pair::Of(Align::End)`"*, *"missing `Wrapped::Of(_)`"*,
*"missing `(Align::End, Align::Start)`"*. Coverage is judged over the
whole pattern tree: a payload or tuple element tested with a literal
proves nothing about the values it does not equal, and only a binder or
`_` covers an unbounded one. Where the hole is the subject's whole
domain the message asks for a catch-all instead, since naming a value
there would say nothing the `_` does not.
→ [Control flow](../tour/control-flow.md)

**"struct '…' has no field '…'"** · **"variant '…' does not belong to the matched enum"**
A field or variant name is off. For the variant case inside `match`,
patterns bind with `let`: a bare misspelled variant is an error here,
never a silent catch-all. When a real field is a close-enough edit away
the field case adds a note — *"did you mean `entries`?"* — and the editor
turns it into a "Change to `entries`" quickfix that rewrites the name.
Close enough is a real threshold: `"entires"` suggests `"entries"`, and
`"x"` suggests nothing at all.
→ [Control flow](../tour/control-flow.md)

**"`…` expects N arguments, but got M instead: `…` is missing."** ·
**"`…` expects N fields, but got M instead: `…` is not a field of `…`."**
A call or a struct literal is short of, or over, its declared list. The
message names the **callee or struct**, not just the counts — two calls
on one line no longer leave you working out which — and, when it is
short, the specific parameter or field that is missing (arguments bind
positionally, so which one is absent is unambiguous; too *many* gets no
such guess, since which extra to drop is not). A secondary note points at
the subject's own declaration: *"`distance` is declared here"*.
→ [Functions & closures](../tour/functions-and-closures.md), [Data and traits](../tour/data-and-traits.md)

**"Expected …, but got void instead: an `if` with no `else` produces void."** ·
**"…: the `;` discards this body's last value."** ·
**"…: this body ends without producing a value."**
Three shapes of "nothing came back", each naming its own cause instead of
reporting a bare void. The first is an `if`/`else if` chain in tail
position with no final `else` — add the branch. The second and third
anchor on the callable's **closing brace**, one character wide: the fix
is almost always the trailing `;` that discarded the value, and the
editor offers "Remove `;`" on it. A closure's own `: T` return annotation
is checked against its body directly, so these reach a closure literal as
readily as a `fun`. Passing a wrong-*typed* value is a different error and
still points at the value.
→ [Functions & closures](../tour/functions-and-closures.md)

**"`…` compares two values of the same type, but the operands are `…` and `…`"**
Comparisons follow the trait model (`==` is `PartialEq`, `<` is
`PartialOrd`): the right operand must be the left's type, and there are
no implicit conversions. An unsuffixed literal adapts to its peer
(`stamp < 3` is fine for an `i53` stamp); two differently-typed
*variables* need a suffix or an `as_*` conversion. Related:
**"`bool` has no ordering"** (compare with `==`/`!=`) and
**"`&&` takes `bool` operands"** (Vilan has no truthiness).
→ [Values and types](../tour/values-and-types.md)

**"`…` takes two values of the same type, but the operands are `…` and `…`:
`…` is wider than what `…` admits"**
The same rule reaching a **generic parameter** on the right of a native
operator — `total - value`, `total & value`, `total < value`,
`total == value` where `value: T`. An operator belongs to its LEFT
operand: the right one has to be a *member* of what the left admits, and
a bound can prove membership only where that set has a trait naming it.
`i32`'s does not — `i32` compares against `i32` and nothing else, and a
bound promises a trait's *methods*, never that the parameter **is** an
`i32` — so no bound rescues this, and adding one is not the fix.
Convert where the type is known and declare the operand `i32`. A native
left operand never dispatches either: an `impl i32 with Add` is not
consulted, because the host operator *is* the semantics there.
→ [Values and types](../tour/values-and-types.md),
[Data and traits](../tour/data-and-traits.md)

**"`+` on `str` concatenates, and `…` has no string form: concatenating it
renders the value's runtime shape …"**
A value with no string form was concatenated into one. Only `str`, the
numbers and `bool` render themselves; a struct lowers to a tuple, an
enum to a tagged array and a `List` to an array, so the host would have
printed `1,2` for a `Point { x = 1, y = 2 }`. Render it first —
`point.to_string()`, adding an `impl Point with Display` if the type has
none. **An interpolated string is this same concatenation** (`i"a{x}b"`
*is* `("" + "a" + x + "b")`), so a hole gets the identical error and the
identical fix; the same goes for a `css` block value that mixes text
with holes. A backed enum is included in the refusal on purpose: its
backing is a lowering detail, not a rendering the program chose.
A **generic parameter** gets the same error worded for its bounds — an
unbounded one promises nothing, and one bounded to something other than
a string form (`T: Add`) promises the wrong thing. Bound it with
`Display` and the bare operand concatenates: the implementation is
called at each instantiation, so `"v=" + value` and `i"v={value}"` both
render the value rather than its runtime shape.
→ [Values and types](../tour/values-and-types.md), [Strings](../std/strings.md)

**"`+` on `…` adds, and `str` is not a number: only a `str` LEFT operand
concatenates …"**
The concatenation is the right way round only when the string is on the
left, because the expression takes its type from its left operand:
`count + "!"` would have typed as `i32` while producing a string. Write
`"!" + count`, or convert with `count.to_string() + "!"`.
→ [Values and types](../tour/values-and-types.md)

**"`+` adds two values of the same type, but the operands are `…` and `…`"**
The `==`/`<` rule above, for addition: no implicit conversions between
numeric types. An unsuffixed literal still adapts to its peer
(`stamp + 1000` is fine for an `i53` stamp); two differently-typed
*variables* need a suffix or an `as_*` conversion (`ratio + count.as_f64()`).
A **generic parameter** on the right of a number's `+` is refused for a
reason of its own — *"`T` is wider than what `i32`'s `add` accepts"* —
and, unlike the concatenation above, **no bound fixes it**: `str`'s
admitted set has a trait that names it (`Display`), a number's has none,
so `T: Add` promises `T + T` and says nothing about `i32`. Convert where
the type is known and declare the operand `i32`.
→ [Values and types](../tour/values-and-types.md)

**"`…`'s `add` accepts `…`, but the right operand is `…`"**
The same membership rule where the left operand **dispatches**: your impl
says what its operator accepts, and this operand is not it. Three shapes
reach the message. A `Self` operator (`impl Counter with Add`) accepts
the subject and nothing else — a foreign struct there used to be read
through the declared type's fields, so `Counter { n = 1 } + Point { x =
1, y = 2 }` computed off the `Point`'s first slot. A **declared** `B`
(`impl Meters with Add<Feet>`) accepts *that* type, which means it does
not accept `Meters`. And an impl over its own parameter
(`impl Bag<type T> with Add<T>`) accepts whatever the left operand bound
`T` to. The routes out: convert the operand, give the left type a second
impl whose `B` is this operand's type, or — for a **generic** operand,
where no bound can prove membership — write the left operand's type over
that same parameter, so its `B` *is* the parameter. Every dispatched
operator reads this way, `eq` for `==`/`!=` and `lt`/`le`/`gt`/`ge` for
the orderings included.
→ [Data and traits](../tour/data-and-traits.md),
[Values and types](../tour/values-and-types.md)

**"`+` adds numbers and concatenates `str`, and `…` is neither: it has no
`Add` …"**
`bool` and backed enums are native for `==` and `<` without being
numbers, so `+` on one would have added its *lowering*: `true + true` is
`2`, typed as a `bool`, and two backings sum to something that is rarely
a variant. Match on the variant, or hold the number you mean.
→ [Values and types](../tour/values-and-types.md)

**"type '…' does not implement the `…` operator; add `impl … with …` providing `…`"**
An operator was used on a type without the matching trait impl: `+`
needs `Add`, `==` needs `PartialEq`, `<`/`<=`/`>`/`>=` need
`PartialOrd` (implement `partial_compare` once; the operators dispatch
through it, and `lt`/`le`/`gt`/`ge` come free as defaults).
→ [Data and traits](../tour/data-and-traits.md)

**"the literal `…` is out of range for `…` (…)"**
The number doesn't fit the type. For `i53`/`u53` the range is ±2^53,
JavaScript's exact-integer window. Bigger integers take `BigInt` (`7n`).
→ [Values and types](../tour/values-and-types.md)

**"unknown numeric suffix `…`"**
The letters after the number aren't a type. If it says `i64` or `u64`:
those were renamed to `i53`/`u53`.
→ [Values and types](../tour/values-and-types.md)

**"substring start … is negative"**, **"substring end … is negative"**,
**"substring end … is before its start …"**, **"substring end … is past the
length … of this string"** (each continuing "— the range must satisfy
`0 <= start <= end <= len`, and substring never clamps or swaps")
`substring(start, end)` was written with literal bounds outside its rule, so
it is refused here rather than at run time. The host's own `substring` would
have *corrected* the call — clamping a negative to `0`, swapping an inverted
pair — and returned a string that is not the one asked for; `s.substring(k, -1)`
in JavaScript is `s[0..k]`, the prefix, not the suffix. Write `s.len()` for
"to the end", and reach for `strip_prefix`/`strip_suffix` to drop a known
affix. Non-literal bounds are checked at run time (below).
→ [Strings](../std/strings.md)

**"type of … could not be resolved"**
Inference gave up somewhere upstream. This error is usually the *echo*
of another one, so fix the first error in the list. When it appears
alone, an annotation at the binding usually grounds it.
→ [gotchas](gotchas.md)

**"… have mismatched types: expected …, but got … instead."**
Every leg of a `match`, and both arms of a value `if`, produce the one
value the construct has, so they have to agree on a type. The refusal is
anchored at the arm that disagrees rather than at the whole construct, and
it names which construct it is ("match legs", "`if` arms"). An arm that
always leaves — a `ret`, a panic — contributes nothing to the merge and is
never the one blamed.
→ [Control flow](../tour/control-flow.md)

**"'…' is ambiguous on '…': both '…' and '…' provide it and neither impl subject is more specific than the other …"**
Two `impl` blocks match this receiver and neither is narrower than the
other, so no spelling at the call site picks one — both are the same trait
at the same instantiation. Vilan resolves overlap by specificity (a
constructor-headed impl outranks a blanket `impl type T`), so the fix is
at the definitions: narrow one subject until it is the more specific of
the two.
→ [Data and traits](../tour/data-and-traits.md)

## Memory and mutation

**"cannot mutate immutable '…'"**
The binding was declared with `let`. Declare it `mut`, or take
`&mut self` if you're inside a method.
→ [The memory model](../tour/memory-model.md)

**"a view cannot escape its scope: it may not be returned, stored in a field, placed in a collection, or carried in an enum payload. …"**
Views (`&x`, `&mut x`) are short-lived by design: lend, use, done. To
keep a reference around, store a plain value, a `Handle` into an
`Arena`, or a `Shared` cell.
→ [The memory model](../tour/memory-model.md)

**"cannot reassign '…' while a view into it is live (rule 4 …)"**
Replacing the whole value would detach the view from live storage. Finish
using the view first (its life ends with its block), or re-derive it after
the replacement. Views anchor wherever they come from: `&x`, a
view-returning call (`list.at(0)`, `arena.get(h)`), or a `Some(let v)`
capture of one. The rule is the same for all three.
→ [The memory model](../tour/memory-model.md)

**"cannot mutate '…' with '.…(..)' while a view into it is live (rule 4 …)"**
The call may advance the container's *geometry* (grow, shrink, reallocate,
swap an aggregate field) while a view points into it. Only
geometry-advancing callees trigger this: a method that writes fields or
elements through `&mut self` passes freely (the compiler infers which is
which). Do the mutation before taking the view, or after its block ends.
→ [The memory model](../tour/memory-model.md)

**"cannot hold a view across …: '…' is still live here. …"**
Your function suspends while a view is live, and whatever it points into
could change during the pause. Re-derive the view after the suspension
(`rows[i].field` again) instead of keeping it. The message names `await`,
but the question is whether the call **can suspend**: calling an async
function without the keyword is the sanctioned spelling and suspends
identically, so this fires on a line with no `await` on it — including
through a sync-looking function that reaches something async.
→ [The memory model](../tour/memory-model.md), [Async](../tour/async.md)

**"view binding '…' cannot be `mut`: a view cannot be rebound. …"**
`mut v = &mut x` doesn't mean what it would in Rust. Declare the view
with `let`; assigning through it (`v = …`) already writes the target.
→ [The memory model](../tour/memory-model.md)

**"a closure cannot capture the view '…': a view is second-class and the closure may outlive the place it views. …"**
A closure body named a view binding (`let v = &mut x`, a `for e in &mut
list` item, or the result of a view-returning call) declared outside it.
A closure captures the *binding*, and nothing says when the closure runs,
so the capture would outlive the place. The two fixes are the two ways it
stops being a capture: read the value out first (`let n = *v;`, then
capture `n`), or take the view as a **parameter** of the closure
(`|v: &mut i32| *v`), which is a per-call loan. A `&`/`&mut` parameter of
the *enclosing function* may be named inside a closure — it views the
caller's place — but that closure may not then escape. An async closure
gets the sharper message below instead.
→ [Functions and closures](../tour/functions-and-closures.md), [spec §6.9](../spec/memory.md)

**"an async closure cannot capture the view '…': the capture would be held across the closure's suspension points. …"**
The rule above, at a closure that suspends: on top of outliving the
place, the capture would be live across an `await`, where any turn may
invalidate it. Re-acquire the view inside the closure after the
suspension, or pass a value or a `Shared`/`Handle` in.
→ [The memory model](../tour/memory-model.md), [Async](../tour/async.md)

## Resources

A `resource` type has a single owner and moves rather than copies; a
struct, enum, or tuple holding one is a resource too, inferred by
containment (`Option<Database>` is a resource, `Option<i32>` is not). A
resource *moves* on binding (`let b = a`), on `own`-passing, on return, and
into a constructor; it is *loaned* (no ownership change) through `self`,
`&`, and `&mut`. The `Drop` destructor trait and its restrictions are below.
After a resource's last use the compiler runs its destructor; resources
whose last use is the same statement discharge in reverse declaration
order. Destruction goes through `try`/`finally`, so `ret`, `jump`, and a
thrown panic all run it on the way out; a resource without a `Drop` impl
still has its fields destroyed. A resource built inside one expression and
never bound is owned by its statement and destroyed at that statement's
end. A module-level resource lives for the process and never drops. A drop that panics while a panic is already
unwinding replaces the in-flight error (JS `finally` semantics). The tutorial
is [Resources](../tour/resources.md); the normative rules are spec
[§6.8](../spec/memory.md).

**"use of `…` after it was moved: a resource has a single owner"**
The binding was moved (bound to another name, passed to an `own`
parameter, returned, or matched by value) and then used again. The note
points at the move. Loan it instead (`&x` / `&mut x`, or a method call),
or, if you really need two owners, restructure with `Option` + `take`.
→ [Resources](../tour/resources.md)

**"cannot move a resource field out of a live aggregate: … no partial moves …"**
`let x = s.db`, or passing / returning `s.db` by value, would move a
resource out of a struct that is still alive: there are no partial moves.
Loan the field (`&s.db`, `&mut s.db`, `s.db.method(…)`), or make the field
an `Option<…>` and `take()` it out.
→ [Resources](../tour/resources.md)

**"`…` is moved on one path through this branch but not another: …"**
An `if`/`match` moves the binding on some paths and not others, so its
end-of-scope ownership isn't static (there are no runtime drop flags). Move it
on *every* path, on none, or hold it in an `Option` and `take()` on the
path that consumes it. A diverging leg (one that `ret`s or `jump`s out) is
exempt: it never reaches the merge.
→ [Resources](../tour/resources.md)

**"`…` is declared outside this loop and moved inside it: …"**
Moving a binding from a loop body would move it again on the next
iteration. Move a value declared *inside* the loop, or loan the outer one
(`&x` / `&mut x`).
→ [Resources](../tour/resources.md)

**"`…` is a module-level resource: it has process lifetime and cannot be moved …"**
A top-level `let` resource lives for the whole process and never drops (the
serve-forever server's `Database`). Consuming it (moving it into a local,
passing it to an `own` parameter, or `drop(x)`) would hand a
process-lifetime resource to a droppable owner and close the shared handle
out from under the rest of the program. Reach it by loan only: method calls,
`&x`, `&mut x`. To own a database that closes after its last use, open it
in a local instead.
→ [Resources](../tour/resources.md)

**"a closure cannot capture the resource `…`; …"**
A closure or `async`/spawn body referenced a *local* or *parameter* resource
from an enclosing scope; capturing it would give the closure a second owner.
Pass a loan into the call, give ownership to the struct that owns the
closure's lifetime, or hoist the resource to module level: a module
global is loan-only and process-lifetime, so a closure may reference it
without becoming an owner. (A closure's own *parameter* is per-call, not a
capture; injected `context`-clause bodies are unaffected.)
→ [Resources](../tour/resources.md)

**"`…` is not move-clean when instantiated with a resource: …"**
A generic function or method was called with a resource type argument
(`Option<Database>`, `wrap(db)`), and its body (checked with that type
parameter treated as a resource) breaks the affine rules in one of three
ways. It uses a value of the parameter's type **more than once** (moves it
on some paths but not all, or captures it in a closure); a resource has a
single owner. Or an **`own` parameter of resource type is never moved
out**: because the generic body is shared across every instantiation, it
cannot run a destructor, so an `own T` must be moved out on *every* path
(returned, or handed to another owner), or the function must take a
concrete type. Or it **passes such a value to `drop<T>`**: that erased
body has no concrete destructor either, so the resource would leak
(`drop(x)` on data is a fine no-op, which is why the data instantiation
stays accepted; destroy at a concrete type, or move the value out to the
caller). The error is spanned at the call (the instantiation), with a
note into the generic's body. A
clean generic moves each such value exactly once (as `Option::unwrap(self):
T` does), never copying, capturing, or forwarding it to the sink;
`drop(concrete)` on a concrete resource *is* the destructor. Instantiating
the same generic at a data type is unaffected.
→ [Resources](../tour/resources.md)

**"the resource `…` cannot be used where `any` is expected: …"**
`any` is a data sink, and a resource must keep its single owner: passing
one to `print`, binding it to `let x: any`, or returning it as `any`
would launder the discipline away. Debug-print the resource's fields
instead.
→ [Resources](../tour/resources.md)

**"`…` cannot hold the resource `…`…: … a native container's internals are host code …"**
`List`, `Map`, `Set`, and the external generics (`Shared`, `Task`,
`Promise`, `Context`) can't hold a resource: the move checker
can't see inside host storage. `Option` is the sanctioned resource
container; or keep the resource in a struct field.
→ [Resources](../tour/resources.md)

**"field `…` of `[derive(Wire)]` / `[derive(Json)]` / `[derive(Hashable)]` / `[derive(PartialEq)]` type `…` is the resource `…`: …"**
A resource is not plain data: it cannot be serialized, hashed by
value, or compared by copy. Drop it from the derived type, or carry a
plain-data handle (an id, a key) in its place. The check reaches a field
nested two structs deep and an enum variant's payload, not just a direct
field.
→ [Resources](../tour/resources.md)

**"`…` cannot be derived for the resource … `…`: …"**
The same rule with the resource in the other position — the derived type
*is* the resource. Serializing it copies a handle out of its owner, and the
reading half is worse: `Wire`'s `rebuild` and `Json`'s `from_json` build a
value out of bytes, which for a resource is a second handle nothing owns
and nothing will close. Send a plain-data name for the resource instead
(an id, an `Arena` handle) and keep the resource on the side that owns it.
The other derives are unaffected: `PartialEq` and `Debug` read a resource's
fields through the loan and stay available.
→ [Resources](../tour/resources.md), [Services](../guide/services.md)

**"`…` implements `Drop` but is not a resource: … declare it a `resource` …"**
`Drop` (the destruction hook) may be implemented only for a `resource`
type. A destructor without move discipline is the double-close bug:
copy the value and each copy would run `drop`. Declare the type `resource`
so it moves instead of being copied. (Plain-data, framework-driven teardown
uses the cooperative `Disposable` protocol, not `Drop`.)
→ [Resources](../tour/resources.md)

**"`drop` for `…` is async: teardown must be synchronous …"**
A `drop` body may not be `async`, nor await (call an async function): a
destructor runs synchronously. Cancel owned tasks through an
`OwnedNursery` (whose own `drop` cancels them) rather than awaiting them.
Awaited teardown is a future design.
→ [Resources](../tour/resources.md)

**"`drop` for `…` requires an ambient context: teardown must be context-free …"**
A `drop` body reached something that needs an ambient context, most often a
`Signal` write, which threads the current turn as a hidden argument. A
destructor's call sites are scope exits, which thread no context, so it
cannot receive one. Keep teardown context-free: hand turn-joining or
signal-writing work to an owner that runs inside a turn.
→ [Resources](../tour/resources.md)


## Async

**"`…` receives an async closure, but its type awaits nothing; declare it `async || T` (or return void for spawn semantics)"**
A closure that suspends was stored into a struct field typed as a
plain, value-returning closure (at the literal or a later assignment).
Either the field should be `async |…| T`, or, if fire-and-forget is
fine, its return type should be `void`. (A plain *parameter* no longer
produces this error. It adapts: the callee instantiates an async copy
that awaits the callback.)
→ [Async](../tour/async.md), [Functions & closures](../tour/functions-and-closures.md)

**"`…` requires a synchronous closure (`sync`): its completion is part of the declaring function's synchronous protocol …"**
The parameter is a `sync` contract position (`Signal::map`,
`set_with`, `turn`/`batch` bodies, the UI render callbacks) where the
callback must finish inside a synchronous protocol, so it cannot adapt.
Move the async work outside the callback: an explicit `turn(…)` whose
awaiting body holds one turn across its awaits, `Draft`/`optimistic`
for local-first commits, or a spawned `async { … }` block. The
transitive form ("this call passes an async closure that reaches `…`")
points at the call that made the closure async and notes where it was
forwarded.
→ [Async](../tour/async.md), [Reactivity](../guide/reactive.md)

**"`…` is a host (`external`) function: it cannot await a Vilan closure …"**
Host code can't await your callback, so an `external` function's
value-returning closure parameters only accept synchronous closures
(void-returning ones spawn, as everywhere). A parameter *declared*
`async |…| T` is exempt: that is the host's explicit contract to await
the closure itself.
→ [Async](../tour/async.md)

**"an async closure cannot adapt a trait/generic-dispatched call …"**
Adaptation instantiates a statically-known callee, and a
trait/generic-dispatched call doesn't have one: the concrete method
varies per instantiation. Bind the receiver concretely before the call,
or declare the trait method's parameter `async || T` so every impl
takes the typed channel.
→ [Async](../tour/async.md)

**"`…` returns an async closure, but its declared return type awaits nothing; declare it `async || T` (or return void for spawn semantics)"**
The function's declared return type is a plain, value-returning closure,
but a `ret` (or the tail) hands back a closure that suspends. Mark the
return type `async || T` so calls through the returned value await
(`make()()` and `let go = make(); go()` both do), or return a
`void`-returning closure for spawn semantics.
→ [Async](../tour/async.md)

**"the initializer of `…` calls `…`, which is async: a module-level binding cannot await"**
A top-level `let` runs when the module loads, and module initialization
is synchronous: there is no enclosing function to become async, so the
value would be a live promise wearing the wrong type. Wrap the work in
a function and call it from `main`. The variant "the initializer of
`…` runs a closure that awaits" is the same rule when the awaiting
thing has no name: an adopted async closure applied directly, a
`run(value, body)` whose body suspends, or a `nursery` at top level.
(Creating an async closure at top level is fine; it awaits nothing
until called.)
→ [Async](../tour/async.md)

**"the initializer of `…` awaits: a module-level binding cannot suspend"**
The same rule, reported at an explicit `await` whose operand is not an
async call — a `Task`-valued binding, a spawn (`await async f()`), or a
`Task` returned by a plain function. Module initialization is
synchronous by design, so every one of these is refused wherever the
`await` sits in the initializer's expression. The note names the fix:
*spawn* at module level (`let pending: Task<T> = async work();`), which
starts the work at load without suspending, and `await` the `Task` in
`main`. An `await` inside a closure the initializer merely *creates* is
not the initializer's own and stays legal.
→ [Async](../tour/async.md)

**"`…` form an initialization cycle: module-level bindings initialize in dependency order, and a cycle has no such order"**
Module-level bindings initialize in dependency order (spec §7.1): each
one runs after everything its initializer evaluates at load: the
bindings it reads, plus whatever is read inside anything it *calls* on
the way. A cycle among those evaluations has no valid order, so it is
refused at compile time; the message names the round trip (`via A → B
→ A`) and each participant's declaration. The self-referential form
("`…`'s initializer evaluates `…` itself, which has not initialized
yet") is the one-binding case of the same rule. Creating a closure
evaluates nothing, so two module-level closures may name each other
freely; moving one of the cycle's reads inside a closure is the usual
fix. If the chain runs through a dispatched call, every implementation
of that method participates, including one your program never
instantiates; the message says so when it applies.
→ [Execution](../spec/execution.md)

**"`!` requires the nearest enclosing function to declare an `Option`/`Result`-compatible return type …"**
`!` propagates the failure by *returning* it, so the surrounding
function must return an `Option`/`Result` that can carry it. Inside a
closure or a UI handler, `match` instead.
→ [Control flow](../tour/control-flow.md)

## Contexts and UI

**"context `owner_scope` is read here, but this code can be reached without an enclosing `run`"**
The most common first UI error: you built reactive state (an `effect`, a
binding) outside every ownership boundary. Wrap the entry point in
`mount_root`, or `run_with_owner` in a test. The error points at your
`effect`/`map`/`or` call; the note under it shows the read inside the
library — the standard library, or an external dependency package your
code calls — that your call reaches, and every call of yours on the
uncovered path above it is underlined too ("the context requirement
flows through this call") — follow the chain up to where the ownership
boundary belongs. Calls inside a covering `run` are clean and never
appear in the chain, and an external package's internal calls are never
underlined: the error always lands on code you wrote. (Your own
workspace is yours: a member package your project root's `packages`
declares reports exactly like your entry's modules — the read anchors
at itself, in the member's file.)
→ [Building UI](../guide/ui.md), [Reactive state](../guide/reactive.md)

**"`…` reads context `…`, so it can't be used as a value"**
A function that reads an ambient context (like the current owner) can't
be passed around as a plain closure: the context channel would be
severed. Wrap it in a closure literal at the use site instead.
→ [Functions & closures](../tour/functions-and-closures.md)

**"an injected (`context`-typed) closure can only be called, forwarded …, or passed to `run`"**
Injected closures (the ones with `context` clauses in their type) are
deliberately restricted so the ambient value can always be threaded to
them. Don't store them; call or forward them.
→ [Functions & closures](../tour/functions-and-closures.md)

**"unused result of a `[must_use]` call: bind it (e.g. `owner.take(…)`), or `let _ = …` to discard."**
The call returns something that stops working if you drop it (a
`Subscription`, typically). Keep it, hand it to an owner, or discard it
on purpose with `let _ = …`.
→ [Reactive state](../guide/reactive.md)

**"`…` is deprecated; use …"** *(a warning)*
The named function still works — this never fails a build — but it is
marked `[deprecated]` and scheduled for removal, no earlier than the
minor release *after* this warning first shipped. The message names the
replacement; switch to it at each warned use site. The removal itself,
when it comes, is announced under the CHANGELOG's Breaking entries with
migration notes.
→ [spec §3.3](../spec/grammar.md) for the attribute

## Wire and rpc

**"field `…` of `[derive(Wire)]` type `…` is `…`, which is not Wire: …"**
Something unserializable (a closure, a `Signal`) is inside a payload
type. Wire types carry data only: scalars, `str`, `bool`,
`List`/`Option` of Wire, and other Wire types.
→ [Services & RPC](../guide/services.md)

**`RpcError::Contract` at connect time**
Client and server were built from different versions of the service.
Rebuild both. During development, a *leaked old server* still holding
the port is the usual culprit: `ss -tlnp | grep <port>` and kill it.
→ [Services & RPC](../guide/services.md), [gotchas](gotchas.md)

**`RpcError::Transport("not connected")` / `("connection lost")`**
The connection is down (fail-fast) or dropped mid-call (in-flight
rejection). Nothing is retried automatically, because your rpc might
not be safe to repeat. Retry at the app level if that's correct; a
draft's next push already does.
→ [Services & RPC](../guide/services.md)

## Compile-time evaluation

**"`asset::emit` outside a `const` expression"**
Styles (and other build assets) are constructed at compile time. Build
the `Style` in a `const` (`let card = const style()…`); select and merge
already-built styles at runtime. The channel's other directions —
`asset::emit_keyed` and `asset::read` — are compile-time-only the same
way.
→ [Styling](../guide/styling.md), [Macros & const](../tour/macros-and-const.md)

**"`asset::emit_keyed` cannot order the `css` kind: the style sidecar is
ordered by the CSS cascade, not by a contribution's key"**
The stylesheet's order is decided by the cascade — base rules before
`@media` blocks, media blocks by ascending min-width — so a sort key
handed to it would have nowhere to apply. Write the rule with
`asset::emit("css", …)`, or let [`std::style`](../std/style.md) own the
sheet. `emit_keyed` is for a kind of the program's own.

**"… is compile-time-only; evaluate this call inside a `const` expression"**
The same rule, caught statically: some function on this call path reaches
`asset::emit`, `asset::emit_keyed` or `asset::read`, and the call itself
sits in runtime code.
The span is the outermost runtime crossing — the call that leaves ordinary
code and enters compile-time territory — so wrap *that* call in a `const`.
A crossing through trait dispatch counts too: a generic call is charged at
the entry whose concrete type selects an emitting impl (a clean impl of the
same trait member through the same generic stays legal), and a dispatch the
compiler cannot resolve — a shared default body's `self` call — is refused
for every receiver, conservatively, since letting one through would compile
clean and throw at run time.

**"cannot read `…` (resolved against the package root to `…`): …"**
A `const asset::read(path)` found no readable file. Paths are relative
to the **package root** — the directory imports resolve under, never the
directory the compiler happens to run from — and the message shows where
the resolution landed. An absolute path, or one that escapes the package
root (`../…`), is refused before any read: the file channel reads the
project, so the build can track every input it depends on. The refusal
is on the path as *written* — a symlink inside the package is ordinary
layout and is followed, so a name that resolves elsewhere is not an
escape ([Const evaluation](../spec/const.md#92-the-const-environment)).

**"… is compile-time-only; call it directly inside a `const` expression — a
compile-time-only function has no runtime value form"**
A compile-time-only function (or a closure that reaches one) was used as a
*value* — passed to a higher-order function, stored in a binding, built as a
closure literal — rather than called. The compiler cannot follow a call made
through a value, so it refuses the value instead. Call it directly inside the
`const`: `const apply(styled)` is fine, `apply(styled)` at runtime is not.

**"a `const` result must be plain data; this evaluates to …"**
The `const` expression produced something that can't be baked into the
output (a closure, a host object). Fold values, not behavior.
→ [Macros & const](../tour/macros-and-const.md)

**"const evaluation failed in `f`: …"**
The computation ran and something inside it went wrong — a panic, a
subscript past the end. The squiggle is on the `const` expression,
because that is the expression the compiler is refusing to fold; the
message names the function it failed *in*, and the note points at that
function's declaration with the call chain that reached it. (The
compiler cannot point inside the callee: the tree it evaluates is the
compiled output, which carries no source positions.)

**"const evaluation did not finish within the compile-time budget in
`f`: …"**
The same thing, but the computation never finished rather than failing:
it exhausted the interpreter's step budget (an unbounded `for`) or its
call-depth cap (unbounded recursion). The build fails rather than
hangs. Fix the termination condition, or move the work to runtime.

## Syntax

A syntax error no longer blanks out the rest of the file. The parser
recovers at statement and item boundaries — a statement it cannot read is
reported and skipped to the next `;`, `}`, or declaration keyword — so
the statements around it, the functions below it and the whole file tail
still reach the type checker, and the diagnostics they already had stay
where they were. `vilan check` type-checks that salvaged file too;
`vilan build` still stops, because a recovered file is not something to
emit from.

**"expected `;` to end this statement"**
A statement ran into the next one. The message is anchored at the gap
where the `;` goes — the last character before it, not the head of the
statement below — and the editor offers an "Insert `;`" quickfix there.
It answers a missing `;` after an `import` or a `use` as well.
→ [spec §3.2](../spec/grammar.md)

**"unclosed `(`: expected a matching `)`"**
A delimiter you opened and have not closed yet — the defining shape of
code mid-edit. It reports on the delimiter *you typed*, not on whatever
the parser tripped over several lines down. A closing delimiter that is
wrong *inside* a finished list keeps its own, more precise message
(`found ';' expected ',' or ')'`, on the exact character where the list
broke).
→ [spec §3](../spec/grammar.md)

**A struct literal in a condition parses as the block**
Struct literals are ordinary operator operands (`Point { … } == q`
compares), but condition positions exclude them: after `if Foo` or a
`match` subject, the `{` is the block/arms, by design. Written without
parentheses, `if p == Point { … } { … }` leaves a bare `Point` as the
condition's operand, which reports **"`Point` is a type, not a value"**.
Parenthesize the literal: `if p == (Point { x = 1 }) { … }`.
→ [spec §3.8](../spec/grammar.md)

**"`#` is not a vilan token …"** · **"`@` is not a vilan token …"**
Both turn up almost only inside a `css` block. A colour is written as a
hole that routes through the `Color` type — `color: {Color::hex("#333")};`
— which is what lets the type carry its own `:root` line. And a `css`
block has no at-rules of any kind: a media query is spelled as a
breakpoint combinator (`.md { … }`), and a declaration block under a
selector of your own is `std::style::declare`.
→ [Styling](../guide/styling.md)

**"`pub` is not a vilan keyword …"**
`pub` (and `public`) is an ordinary identifier here, so `pub fun helper()`
reads as the expression statement `pub` followed by an item — which used
to report a missing `;` three columns in, a true statement about a
program nobody wrote. Vilan has no visibility marker to reach for: a
module's items are importable as written, so the fix is to delete the
word. `export` is a different thing — it *re-exports* something this
module imported (`export import pkg::io::panic;`), so importers of this
module see the name as if it were declared here.
→ [spec §4.3](../spec/names.md)

**"a mutable binding is spelled `mut x = …` …"**
`let mut x = 1` is the Rust spelling. `let` and `mut` are vilan's two
binding *forms*, not a keyword and a modifier on it: `let` binds
immutably, `mut` binds mutably, and writing both is neither. The old
message ("found 'let' expected a statement") named the one token that was
right.
→ [Values and types](../tour/values-and-types.md), [spec §3.3](../spec/grammar.md)

**"a `{` inside an `i"…"` string opens an interpolation hole …"**
A trailing note on whatever the parser found inside the hole. `{` is the
hole opener, so a *literal* brace has to be escaped: write `\{` and `\}`.
Without it, `i"body { color: red }"` reports a failure about an
expression the author never wrote — and code that generates braces (a CSS
rule, a JS body, a JSON object) hits this on nearly every line.
→ [Values and types](../tour/values-and-types.md)

**"a string cannot span lines unless it is triple-quoted …"**
A `"…"` or `i"…"` ran into a line break before its closing quote. Either
the quote is missing (the common case; the error is reported on the
string's own line rather than wherever the next `"` happens to be), or
the text really is multi-line, in which case write it `"""…"""`
(`i"""…"""` with holes). A single line break inside a one-line string is
`\n`. Nothing escapes a line break: a trailing `\` does not continue the
literal onto the next line.
→ [Values and types](../tour/values-and-types.md), [spec §2.3](../spec/lexical.md)

**"`Name` is a type, not a value"** (also *"a trait / a type parameter /
a module, not a value"*)
A type, trait, type parameter, or module name was used where a value is
expected (`let q = Point;`). A type names a kind, not a runtime value:
construct it (`Point { … }`), name a variant (`Color::Red`), or call a
static (`Point::new(…)`).
→ [spec §4.2](../spec/names.md)

## Panics

These are not compile errors — they are what the program prints when it
stops at run time. Each exists because the alternative was a host-level
message naming nothing you wrote — or, worse, no message at all and a
quietly wrong answer.

**"… : … is not one of its values"** (e.g. `Align: "middle" is not one of its values`)
An **exhaustive** `match` over a backed enum met a value outside its
variant set. A backed enum lowers to a bare host string or number, so its
runtime domain is the host's, and exhaustiveness is a proof about the
*variant set* rather than about the value: an `external fun` return, a
host callback's argument, or a decoded payload can carry anything. The
last arm is tested like every other one and the `else` traps rather than
answering with whichever variant happened to be last. A `match` with a
`_` arm is unaffected — the out-of-set value takes the arm you wrote — and
`Enum::parse(text)` is the shape to reach for where an unrecognized value
is one of the answers you expect: it returns `Option`.
→ [Data and traits](../tour/data-and-traits.md), [spec §5.2](../spec/types.md)

**"substring out of range: the length is … but the range is …..…"**
`substring(start, end)` was called with bounds outside `0 <= start <= end <=
len`, computed rather than literal (a literal pair is refused at compile time
instead). The rule is absolute: no clamping, no swapping, and an `end` past the
length is an error rather than a truncation. This one is a panic *because* the
host would not have raised anything — JavaScript's `substring` clamps a
negative to `0` and swaps an inverted pair, so `s.substring(offset, -1)` there
quietly yields `s[0..offset]`, the complement of the intended cut. Pass
`s.len()` as the `end` to mean "the rest", and use
`strip_prefix`/`strip_suffix` (which return `Option<str>`) to drop an affix.
→ [Strings](../std/strings.md)

**"mount: no element with id '…'"**
`mount` or `mount_root` was given an id nothing on the page carries. The
host's `get_element_by_id` hands back `null` typed as an `Element`, so the
shared lookup checks for that first and names the id, instead of leaving a
`Cannot read properties of null` to speak for itself. On a server-served
page, `check_shell` catches the same mismatch at boot.
→ [Browser modules](../std/browser.md), [Building UI](../guide/ui.md)
