# Data and traits

> Normative rules: spec [§5 The type system](../spec/types.md).

## Structs

A struct is a named record type:

```vilan
struct Task {
	id: i32,
	name: str,
}

fun main() {
	let task = Task { id = 1, name = "write docs" };
	print(task.name);
}
```

Literal fields use `=`, not `:`. When a field's value is a binding with
the same name, you can write it once: `Task { id, name }`. There are no
field defaults; `derive(Default)` (below) covers the all-defaults case.

If you're coming from TypeScript: a struct is like an interface plus an
object literal in one, except it's a real nominal type. Two structs with
identical fields are still different types.

## Enums

An enum is a type with a fixed set of variants, and variants can carry
data. If you've used discriminated unions in TypeScript, this is that
idea with language support:

```vilan
enum Shape {
	Circle(f64),
	Rectangle(f64, f64),
	Point,
}

fun area(shape: Shape): f64 {
	match shape {
		Shape::Circle(let radius) => 3.14159 * radius * radius,
		Shape::Rectangle(let width, let height) => width * height,
		Shape::Point => 0.0,
	}
}

fun main() {
	print(area(Shape::Rectangle(3.0, 4.0)));
}
```

`match` takes the value apart, and the compiler checks that you handled
every variant. Add a variant later and every `match` that misses it
becomes a compile error. That's the feature.

`Option` and `Result` are ordinary enums from std, with no special
cases.

### Backed enums

A variant with no payload can instead carry the value it stands for —
the number or string the outside world already speaks:

```vilan
import std::option::Option;

enum Align {
	Start = "flex-start",
	Center = "center",
	End = "flex-end",
}

fun main() {
	// The variant IS its backing value at runtime, so `.value()` is free.
	print(Align::Start.value());

	// And back, for a value you did not construct — `None` outside the set.
	print(match Align::parse("center") {
		Option::Some(let align) => align.value(),
		Option::None => "unknown",
	});
}
```

The backing value is written, never derived from the name: `Start` is
`"flex-start"` because that is what CSS calls it. Integers work the same
way (`enum Ordering { Less = -1, Equal = 0, Greater = 1 }`) and are the
older half of the same feature.

Two variants may not share a backing value, an enum may not mix strings
and integers, and a variant with a payload may not have one at all —
there is nowhere to put a payload in a bare string. `match` still matches
*variants*: `match align { "flex-start" => … }` is an error, because the
backing value is a representation, not a second spelling of the name.

Because the variant *is* the host's own value, an `external fun` can take
one and return one — no wrapper, no conversion. That also means a host
can hand you a value that is none of the variants, and an exhaustive
`match` says so rather than guessing: it tests every variant and panics
on anything else, with the value in the message
(`Align: "middle" is not one of its values`). Where an unrecognized value
is an answer you expect rather than a bug, bind the `str` and use
`parse`, which gives you `None` instead.

The same "the variant *is* the value" rule reaches the wire:
`[derive(Json)]` and `[derive(Wire)]` on a backed enum encode the
**backing value**, so `Align::Start` crosses as `"flex-start"`, not as
`"Start"`. That is usually the point — the string a backed enum carries
is generally the one the outside world already speaks — but it is worth
knowing before you name a backing value, because renaming one later is a
wire change.

## impl: methods and statics

Methods live in `impl` blocks, separate from the data:

```vilan
struct Counter {
	value: i32,
}

impl Counter {
	// A static: no self. Called as Counter::new().
	fun new(): Counter {
		Counter { value = 0 }
	}

	// A method: self is the receiver (a value; &mut self to mutate in place).
	fun doubled(self): i32 {
		self.value * 2
	}

	fun bump(&mut self) {
		self.value += 1;
	}
}

fun main() {
	mut counter = Counter::new();
	counter.bump();
	print(counter.doubled());
}
```

The `&mut self` on `bump` matters: it means "mutate the actual receiver,
not a copy". Plain `self` receives a copy, like every other value in
Vilan. The [memory model](memory-model.md) chapter makes this precise.

## Generics and bounds

Type parameters work on functions, structs, enums, and impls. A bound
constrains what the code may do with the parameter:

```vilan
import std::compare::PartialOrd;

struct Pair<T> {
	first: T,
	second: T,
}

impl Pair<type T: PartialOrd> {
	fun larger(self): T {
		if self.first > self.second {
			self.first
		} else {
			self.second
		}
	}
}

fun main() {
	let pair = Pair { first = 3, second = 8 };
	print(pair.larger());
}
```

The impl-side syntax is `impl Pair<type T: PartialOrd>`. The `type T`
declares the parameter at the impl, and the bound says these methods
exist only when `T` can be compared.

> **Going deeper.** Generics are monomorphized: each concrete use of a
> generic function or impl compiles to its own specialized code, so
> generic dispatch has no runtime cost. This is unlike TypeScript, where
> generics are erased. It also means the compiler checks bounds at each
> call site, not at the declaration alone.

## Traits

A trait declares a capability. `impl Type with Trait` provides it. Trait
methods can have default bodies written in terms of the required ones:

```vilan
trait Greet {
	fun name(self): str;

	// A default, in terms of the required method.
	fun greet(self): str {
		"hello, " + self.name()
	}
}

struct Robot {
	id: i32,
}

impl Robot with Greet {
	fun name(self): str {
		i"unit-{self.id}"
	}
}

fun main() {
	print(Robot { id = 7 }.greet());
}
```

A trait can take type parameters, and they can carry bounds of their own.
The bound is in scope inside the trait's default bodies — the same way a
bound on a function or an impl is inside theirs — so a default can call
the bound's methods on a value of that type:

```vilan
trait Label {
	fun label(self): str;
}

// `T: Label` — the bound is usable by the defaults below.
trait Holder<T: Label> {
	fun item(self): T;

	fun describe(self): str {
		"holding " + self.item().label()
	}
}

struct Dog {}
impl Dog with Label {
	fun label(self): str { "a dog" }
}

struct Cat {}
impl Cat with Label {
	fun label(self): str { "a cat" }
}

struct DogBox {}
impl DogBox with Holder<Dog> {
	fun item(self): Dog { Dog {} }
}

struct CatBox {}
impl CatBox with Holder<Cat> {
	fun item(self): Cat { Cat {} }
}

fun main() {
	print(DogBox {}.describe());
	print(CatBox {}.describe());
}
```

Each impl picks the argument (`Holder<Dog>`, `Holder<Cat>`) and is checked
against the bound there. The one `describe` body is then specialized per
implementing type, so `self.item().label()` reaches `Dog`'s `label` in one
and `Cat`'s in the other.

Each impl picks its own argument, but a type implements a given trait
**once**: a second `impl Robot with Greet` is a compile error at the
second block, because nothing would rank the two and one of them would
simply never run. Merge the bodies, or delete the copy you don't want.
Different arguments are different implementations, so `impl DogBox with
Holder<Dog>` and `impl DogBox with Holder<Cat>` would both be fine.

Traits are like interfaces, with two differences. They're implemented
explicitly (`impl Robot with Greet`), never structurally. And they
appear as *bounds* on generics (`T: Greet`) rather than as standalone
types: `fun f(v: Greet)`, `fun make(): Greet` and `struct H { item:
Greet }` are all compile errors. When you want "one of several things at
runtime", use an enum.

## A trait on a binding is a constraint

The one place a trait's name reads well in an annotation is a `let`, and
there it means something narrower than it looks: a **constraint** on the
value, not the value's type.

```vilan
trait Greet {
	fun greet(self): str;
}

struct Robot { id: i32 }
impl Robot with Greet {
	fun greet(self): str { "beep" }
}

fun main() {
	// `unit` is a `Robot`. The annotation only checks that `Robot`
	// implements `Greet` — it does not hide the type behind it.
	let unit: Greet = Robot { id = 1 };
	print(unit.id);      // a Robot's own field, still readable
	print(unit.greet());
}
```

Write it when you want the compiler to hold a value to a trait while you
keep working with its real type. Because the type stays concrete, two
`if` arms of *different* types are still an error, even when both
implement the trait — there is no widening for them to meet in. And a
trait nested inside the annotation (`List<Greet>`) is an ordinary value
position, so it is still refused: there are no heterogeneous containers.

## Associated functions

A trait `fun` with no `self` is an **associated function** — a name in
the trait's namespace rather than a method. Give it a default body and it
becomes callable on the trait itself:

```vilan
struct Counter { value: i32 }

trait Source {
	fun new(initial: i32): Counter { Counter { value = initial } }
}
impl Counter with Source {}

fun main() {
	print(Source::new(7).value);
}
```

`Source::new(..)` always runs the trait's own body. An impl may declare
its own `new` to override it, and that one is reached the way any of a
type's own statics is — `Counter::new(..)`. There is no receiver here for
the compiler to pick an implementation with, which is why the two
spellings mean two different bodies rather than one dispatch, and why a
declaration with *no* default body can only be called through a type.

Operators are traits too. `+` dispatches through `Add`, `==` through
`PartialEq`, `<` through `PartialOrd`, and so on. Implement the trait and
your type gets the operator. `std::time` does this so that
`instant + duration` works.

## Derives

`[derive(…)]` generates trait impls from a type's shape, so you don't
write the boilerplate:

| Derive | Gives you |
|---|---|
| `PartialEq` | structural `==` |
| `Debug` | `.debug()`: a developer-facing rendering |
| `Default` | `Default::default()` built from the fields' defaults |
| `Hashable` | usability as a `Map` key or `Set` member (`std::hash`) — a backed enum already has it |
| `Json` | JSON encode/decode (`std::json`) |
| `Wire` | serialization for rpc payloads (`std::wire`) |

```vilan
import std::debug::Debug;

[derive(PartialEq, Debug)]
struct Point {
	x: i32,
	y: i32,
}

fun main() {
	let a = Point { x = 1, y = 2 };
	let b = Point { x = 1, y = 2 };
	print(a == b);
	print(a.debug());
}
```

The standard shape for a type that crosses the wire is
`[derive(Wire, PartialEq, Debug)]`.

> **Going deeper.** Derives are ordinary macros, and you can write your
> own; see [Macros & const](macros-and-const.md). `Wire` and `Json`
> check that every field is itself serializable, recursively, and report
> at the derive site when one isn't.
