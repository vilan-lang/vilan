# Functions & closures

> Normative rules: spec [§3 Grammar](../spec/grammar.md), [§5.8 Coercions](../spec/types.md), [§7.4 Async closures](../spec/execution.md).

## Functions

`fun` declares a function. The last expression in the body is the return
value, and `ret` returns early:

```vilan
import std::print;

fun clamp(value: i32, low: i32, high: i32): i32 {
	if value < low {
		ret low;
	}
	if value > high {
		ret high;
	}
	value
}

fun main() {
	print(clamp(15, 0, 10));
}
```

Notice there is no `return` on the last line. A bare expression at the
end of a block is the block's value. You'll see this everywhere in Vilan.
`if`, `match`, and plain blocks all work the same way.

The `: i32` is optional. Leave it off and the return type is inferred
from the body — from the final expression and from every `ret`, which
have to agree. A body that only ever leaves by `ret` is typed by those
`ret`s:

```vilan
import std::print;

fun sign(x: i32) {
	if x < 0 {
		ret -1;
	}
	if x > 0 {
		ret 1;
	}
	ret 0;
}

fun main() {
	print(sign(-7));
}
```

If a `ret` disagrees with the rest of the body — a `ret "s"` next to a
final `2`, a bare `ret` in a body that ends in a value, a `ret 1` beside
an `if` with no `else` that can fall through with nothing — the compiler
reports it at that `ret`. Declaring the return type is always an option,
and turns inference into checking.

Generic functions take type parameters. Bounds say what the body is
allowed to do with them:

```vilan,fragment
fun largest<T: PartialOrd>(a: T, b: T): T { … }
```

Parameters are immutable by default, like `let` bindings. Write `mut` to
make one a scratch copy the body can rebind and mutate — the caller
never sees it, because parameters arrive by value (to change the
caller's value, take `&mut` instead). It works on `self` and closure
parameters too:

```vilan
import std::print;

fun bump(mut x: i32): i32 {
	x = x + 1;
	x
}

fun main() {
	let original = 7;
	print(bump(original)); // 8
	print(original);       // 7 — untouched
}
```

## Taking any number of arguments

Prefix the last parameter with `...` and callers write its elements out
flat instead of building a tuple. The parameter itself is an ordinary
tuple parameter — `fun f(...items: T)` is `fun f(items: T)`, and
`f(a, b)` is `f((a, b))` — so `T` is the whole **pack**, a tuple of
however many arguments arrived, and its bound decides how many are
allowed:

```vilan
import std::print;

// `(..)` admits any arity, including none at all.
fun how_many<T: (..)>(...items: T): i32 {
	1
}

// A fixed parameter first; the spread takes whatever is left.
fun first_of(head: i32, ...rest: (i32, i32)): i32 {
	head
}

fun main() {
	print(how_many());
	print(how_many("a", true, 3));
	print(first_of(7, 8, 9));
}
```

`T: (2..)` would reject `how_many(1)` with the arity bound's own error,
and `T: (..: Display)` would require every argument to be printable.
Since the arguments are collected into a value the *call site* builds,
`...` never combines with `own`, `&`, or `&mut`; `mut ...items` is fine,
and means what `mut` always means. `...` belongs to a plain `fun` — not
to a closure, a trait method, an `impl` member, or an `external fun`.

### Passing a pack on

Since `f(a, b)` means `f((a, b))`, a `..` spread at the call site means
`f((..pair))` — the arguments you already hold, written out flat. That is
how a pack is forwarded to another spread function, and it is not the
same as passing the tuple itself, which would arrive as a pack of one:

```vilan
import std::print;

fun how_many<T: (..)>(...items: T): i32 {
	1
}

fun forward<T: (..)>(...items: T): i32 {
	how_many(..items)
}

fun main() {
	let pair = (1, 2);
	print(how_many(..pair));       // a pack of 2
	print(how_many(..pair, 3));    // a pack of 3
	print(forward(1, 2, 3));       // a pack of 3, passed on
}
```

The bound is checked on what the spread actually contributes, so a
`T: (3..)` would reject `how_many(..pair)` for having only two. A spread
at a call to a function *without* a spread parameter builds no tuple and
is an error — write the tuple yourself (`takes_a_tuple((..pair))`).

## Closures

A closure is an inline function value. Where JavaScript writes
`x => x * 2`, Vilan writes `|x| x * 2`. Parameter types are usually
inferred from where the closure is used. Annotate them when they aren't:

```vilan
import std::print;

fun apply(seed: i32, transform: |i32| i32): i32 {
	transform(seed)
}

fun main() {
	print(apply(21, |n| n * 2));
	let label = |count: i32| i"{count} items";
	print(label(3));
}
```

Closure **types** are written `|T| U`. A closure with no parameters is
`|| U`, and one that returns nothing is `|| void`. These appear as
parameter types, in `let` annotations, and as struct fields.

`ret` inside a closure returns from the closure, and the closure's
return type is inferred exactly the way a function's is: from the final
expression when the body can reach it, and from every `ret`, which have
to agree — a body that only ever leaves by `ret` is typed by those
`ret`s. A `ret` that disagrees is reported at that `ret`.

A closure captures the **bindings** around it, not their values — the
one alias in Vilan you get without asking for one. The closure and its
creator share the binding, so a write on either side shows up on the
other, and even reassigning the whole binding is visible inside:

```vilan
import std::print;

fun main() {
	mut label = "before";
	let show = || label;
	label = "after";
	print(show());        // after — same binding, not a copy of it
}
```

That is usually what you want: it is how a click handler reads the
counter you just bumped. Three things it does not extend to. A
**resource** cannot be captured at all — a closure would be a second
owner of it. A **view** (`&x`, `&mut x`) cannot either: the closure may
outlive the place, so read the value out with `*` first, or take the
view as a parameter of the closure. And an ambient
[context](../spec/contexts.md) value is the one thing genuinely copied
when the closure is made, so a deferred body reads the context it was
written in.

When two closures need to share mutable state that outlives the frame
they were made in, they hold a `Shared` cell together. The
[memory model](memory-model.md) explains that pattern, and
[spec §6.9](../spec/memory.md) is the rule.

## Named functions as closures

When a function already does what your closure would do, pass the
function itself:

```vilan
import std::print;
import std::reactive::Signal;

fun exclaim(text: str): str {
	text + "!"
}

fun main() {
	let words = Signal::new("hello");
	let loud = words.map(exclaim);   // instead of .map(|w| exclaim(w))
	print(loud.get());
}
```

You can also just name one and call it later. A binding holding a
function is a function:

```vilan
import std::print;

fun exclaim(text: str): str {
	text + "!"
}

fun main() {
	let shout = exclaim;
	print(shout("hey"));
}
```

This works for plain Vilan functions. It does not work for generic
functions, methods, `async` functions, or externs — those have no value
form, so you can neither store one nor call it through a binding. For
those, write the small wrapping closure. The compiler will tell you when
you hit one, and which of the four you hit.

## Async closures

You can skip this section until you start storing callbacks that do
async work.

A closure type can carry an `async` marker: `async |T| U`. Calls through
a value of that type are awaited automatically, the same way direct
calls to async functions are (see [Async](async.md)). Write it anywhere
a closure type is declared (a parameter, a `let` annotation, a struct
field, a return type):

```vilan,fragment
fun draft<T: PartialEq>(initial: T, commit: async |T| Option<str>): Draft<T>   // a parameter

struct Poller {
	tick: async || i32,               // a struct field
}

let commit: async |T| Option<str> = stored;   // a let annotation
let outcome = commit(value);                  // …and this call awaits
```

An unannotated `let` needs no marker at all: a binding that holds an
async closure adopts its asyncness. [Async](async.md#async-closures)
covers the same seams from the async side.

There is one more rule, and it works in your favor. Passing an async
closure where a plain closure is expected is an error if the plain type
returns a value: you would receive a promise pretending to be the
value. But if the plain type returns `void`, it is allowed, and the call
becomes fire-and-forget. This is why UI event handlers can await things
freely without any ceremony.

## Context clauses

You will *use* this feature constantly without writing it. It is how the
UI framework passes things like "the current owner" invisibly. You only
write it yourself when building framework-level helpers, so feel free to
skim this on a first read.

A parameter's closure type can declare that the closure reads an ambient
**context**:

```vilan,fragment
fun mount_root(id: str, body: (sync || View) context owner_scope): Owner
fun batch<T>(body: (sync || T) context turn_scope): T
fun turn<T>(policy: FlushPolicy, body: (|| T) context turn_scope): T
```

The `sync` marker on the first two says the closure must stay
synchronous; [Async](async.md#higher-order-functions-adapt) explains it.
`turn` carries none, and the difference is the feature: its body is
asyncness-polymorphic, so an awaiting one holds every notification until
it completes, where a `batch` body must stay synchronous.
When you pass a closure literal into such a parameter, the ambient value
(the current `Owner`, the current `Turn`) is threaded to it at the call
site, through any depth of ordinary function calls in between. This is
the machinery behind "every `effect` registers with the nearest
boundary" in the UI layer. Your component functions never mention
owners, and ownership still flows to the right place.

> **Going deeper.** Two rules keep contexts sound. First, closures
> capture their contexts when they are *created*. A closure created
> outside a scope and called inside it would see nothing, so the
> compiler rejects that shape outright. Second, a function that reads a
> context can't be passed around as a plain value, because the context
> channel would be severed. Both produce clear errors when you hit them.

