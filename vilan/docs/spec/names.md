# Spec §4 — Names, modules, and packages

## 4.1 Modules

A module is one source file. Its top-level statements form its body; its
declarations (`fun`, `struct`, `enum`, `trait`, `impl`, module-level
`let`, `mod` blocks) are its items. There is no separate module
declaration: a file `routes.vl` in a package's source root is the module
`routes` of that package.

## 4.2 The three namespaces

A path's first segment selects a namespace:

- `std::name`: the standard library module `name`, resolved against the
  std package's layers for the current platform (§11).
- `pkg::name`: the module `name` of the **importing file's own
  package**.
- `depname::name`: the module `name` of the dependency declared as
  `depname` in the package manifest.

Within std itself, sibling modules are referenced as `pkg::…` (std is its
own package). The namespaces are disjoint: resolution is scoped by the
root segment, so a package is free to name a module `ui` or `json` even
though std has one: `pkg::ui` is always the package's own module,
`std::ui` always std's, and neither shadows the other. (Conversely,
`pkg::` never reaches a std module.) A module name that resolves both as
`name.vl` and `name/lib.vl` is an **ambiguity error**.

A module name must match the on-disk directory entry **byte for byte**: a
case-insensitive filesystem that answers `import foo` with `Foo.vl` is a
**diagnostic naming both spellings**, not a resolution, so that a program
compiles identically on case-sensitive and case-insensitive filesystems
([design notes](https://github.com/vilan-lang/vilan/blob/main/vilan/proposal/windows-support.md) §5). Every component of the resolved path
carries the rule, so `foo/lib.vl` is reached by `import foo` only when the
directory is spelled `foo`.

## 4.3 Imports

`import path` (§3.2) loads the target module (once per program: loading
is idempotent and cycle-tolerant) and binds the imported items in the
importing module's scope:

- `import std::print;` binds the item `print`.
- `import std::reactive::{ Signal, combine };` binds each set member.
- `import std::option::Option::{ self, Some, None };` is a path into a
  TYPE: `self` binds the type itself; variant names bind the variants for
  unqualified use.

`use path` binds names from an already-visible type's namespace without
loading (variants, statics). `export statement` re-exports: importers of
this module see the exported names as if declared here.

Platform gating is not checked at the import: a module outside the
current platform's layers (e.g. `std::ui` in a Node build) still loads,
so its items type-check. The error is reported where platform-colored
code becomes **reachable** from the build's entry (§11).

## 4.4 Scopes and shadowing

Scopes nest: module → function/impl → block → closure. Name lookup walks
outward from the use site to the innermost binding. A `let`/`mut` binding
**shadows** any outer binding of the same name from its point of
declaration onward, including imports and items:

```vilan
import std::print;

fun main() {
	let print_count = 2;
	mut label = "a";
	{
		let label = "inner";     // shadows the outer binding in this block
		print(label);
	}
	print(label);
	print(print_count);
}
```

Items within one module share the module scope and are visible
**throughout** the module regardless of declaration order (a function may
call one declared later). Local `let` bindings are visible only after
their declaration.

A `let` may also redeclare a name **within the same scope**: the later
binding shadows the earlier one from its own declaration point onward,
while uses before that point keep the earlier binding (parameters —
`mut`, spread, or plain — and loop/pattern bindings are shadowable the same way). A
spread parameter binds one name to the whole pack, like any other
parameter; it declares no per-element names. Visibility starts at
the **end** of the declaring statement, so an initializer never reads the
binding it declares: in `let x = x + 1;` the right-hand `x` is the
previous `x` (an enclosing or earlier same-scope binding) and an error
when none exists. Module-level bindings are the exception, as above: they
are order-independent, one declaration per name, and a genuine
initialization cycle is a compile error (§7 of the execution chapter).

## 4.5 Type position vs value position

A name is resolved differently by position:

- In **type position** (annotations, generic arguments, impl subjects),
  lookup prefers bindings that denote types; a value binding with the same
  name does not shadow a type there.
- In **value position**, lookup takes the nearest binding of any kind.

Consequently a local variable named `Signal` does not break `let s:
Signal<i32>` annotations in the same scope, but relying on this is poor
style.

## 4.6 Statics and members

`Type::member` (§3.6) resolves `member` in `Type`'s namespace: enum
variants, the static functions of the type's impls (those without `self`),
and the type's own `self`-methods. Generic statics take their arguments at
the path head: `List<str>::new()`.

A type has **one** namespace, and receiver position is not part of a
name. Two impls of one type declaring the same name — two statics, two
methods, or one of each — are a compile error at the declaration, since
nothing ranks them and one would simply never be reachable.

`value.member` resolves against the value's type: fields first, then
methods, by a **precedence rule** — not by the order the impl blocks
happen to be written or the modules happen to load:

1. An **inherent** method — one declared by an impl of the type whose
   `with` clause does not declare that name — always wins, whatever the
   text order.
2. Otherwise, the method a **trait** provides, whether the impl declares
   it or inherits the trait's default (§5.7).

Two declarations at the same level are an error rather than a silent
pick. Two *inherent* declarations of one name for one subject are rejected
at the definition site, before any call resolves them. Two *traits*
providing one name, with no inherent method above them, make each call an
ambiguity error — as does a `T: A + B` bound whose two arms supply it.

```vilan
import std::print;

struct Bag { x: i32 }
trait Iter { fun pick(self): str; }

impl Bag with Iter {
    fun pick(self): str { "the trait's" }
}

impl Bag {
    fun pick(self): str { "the type's own" }
}

fun main() {
    let bag = Bag { x = 1 };
    // The inherent method wins, though the trait impl is written first.
    print(bag.pick());
    // Naming the trait reaches its version; naming the type means the
    // inherent one, and never falls through to a trait's.
    print(Iter::pick(bag));
    print(Bag::pick(bag));
}
```

`Trait::member(receiver, args…)` is the disambiguator: it names which
provider to use, and works on a concrete receiver or a trait-bounded
generic one. `Type::member(receiver, args…)` means the type's own member
or nothing.

## 4.7 The prelude

A small set of names is in scope without imports: the primitive types
(`i32`, `str`, `bool`, …), `List`, `void`, and the boolean/`null`
literals' types. Everything else (including `Option`, `Result`, `print`)
must be imported. (The exact prelude is the lang-item table, appendix
§A.4.)
