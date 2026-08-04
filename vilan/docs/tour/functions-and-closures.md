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

Closures capture their surroundings **by value** at the moment they are
created. Vilan copies, remember. When a closure needs to share mutable
state with its creator, they hold a `Shared` cell together. The
[memory model](memory-model.md) explains that pattern.

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

This works for plain Vilan functions. It does not work for generic
functions, methods, `async` functions, or externs. For those, write the
small wrapping closure. The compiler will tell you when you hit one.

## Async closures

You can skip this section until you start storing callbacks that do
async work.

A closure type can carry an `async` marker: `async |T| U`. Calls through
a value of that type are awaited automatically, the same way direct
calls to async functions are (see [Async](async.md)). Write it anywhere
a closure type is declared (a parameter, a `let` annotation, a struct
field, a return type):

```vilan,fragment
fun draft<T>(initial: T, commit: async |T| Option<str>): Draft<T>   // a parameter

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
fun turn<T>(policy: FlushPolicy, body: (sync || T) context turn_scope): T
```

The `sync` marker in those signatures says the closure must stay
synchronous; [Async](async.md#higher-order-functions-adapt) explains it.
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

