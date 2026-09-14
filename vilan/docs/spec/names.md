# Spec §4 — Names, modules, and packages

## 4.1 Modules

A module is one source file. Its top-level statements form its body; its
declarations (`fun`, `struct`, `enum`, `trait`, `impl`, module-level
`let`, `mod` blocks) are its items. There is no separate module
declaration: a file `routes.vl` in a package's source root is the module
`routes` of that package.

Modules form a **tree**, and a directory under the source root is a
module path. `a.vl` **or** `a/lib.vl` is the body of module `a` (both
present is an ambiguity error, §4.2), and `a/b.vl` is the module `a::b`
whether or not `a` has a body of its own; nesting is unbounded, so
`a/b/c.vl` is `a::b::c`. A directory with no body is a **pure
namespace**: it holds modules but is not one, and an `import` naming it
is a diagnostic listing the modules it does hold.

`lib` is a body's **file name**, not a path segment: `a/lib.vl` is what
`pkg::a` resolves to, so `pkg::a::lib` would name one file under two
names and is a **diagnostic** steering to `pkg::a` (whose items are that
file's). The rule reads the DIRECTORY: a `lib.vl` at the source root is
the ordinary module `lib`, because the root is nobody's body, and a
directory literally named `lib` (`a/lib/x.vl`) is a path segment like any
other.

A module is reached by its **own** path. `import pkg::a` binds the items
of `a`'s body and nothing below it; `a::b` is reached by
`import pkg::a::b`. Item lookup through a module in expression position
(`a::item`) reads that module's items only, so what a path means never
depends on which other files a build happens to load.

**Loading is not lookup, and a child pulls its parent.** Importing
`pkg::a::b` LOADS `a`'s body too, when `a` has one — `a` is a node on the
path to `a::b`, and the tree is built by loading each segment that names a
file (Rust's model, where `a::b` is reached through `a`'s own `mod b;`).
Nothing of `a`'s becomes visible by it: the own-path rule above is
unchanged, and `a::item` after `import pkg::a::b` still does not resolve.
What is paid is the COMPILE: `a.vl` is parsed and analyzed, and its
diagnostics are the build's. This is stated because it is payable — a
directory whose `a.vl` does real work is a cost every one of its children
imposes on any build that reaches one, so a heavy parent beside light
children is worth splitting. A directory with no body (the pure namespace
above) has nothing to load and costs nothing. Emission is unaffected: it
follows reachability, so a parent body nothing references contributes no
output.

**What a parent DOES contribute is its `impl` blocks.** Items are reached
by a module's own path and nothing below it; implementations are not
items, and a plain `import pkg::a::b;` admits every block declared along
the path — `a`'s as well as `a::b`'s. So an extension impl written in a
directory's body arrives in any file that imports a module beneath it,
without that file naming `a`, and `a.vl` is the place to put one that
every child's importer should have. `only` declines them all and an impl
selector takes the blocks it names (§4.3); a block the declaring module
does not `export` is not admitted at all (§4.8).

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
`pkg::` never reaches a std module.) The root names themselves are
**reserved**: a manifest may not declare a dependency — or a
`[package] name` — as `std`, `pkg`, or `macro_std` (§11.4), so no
package can shadow a root or vanish behind one; `vilan`, the language's
own name, is reserved alongside them (§11.4). A module path that
resolves both as `path.vl` and `path/lib.vl` is an **ambiguity error**.

A path is resolved by its **longest module prefix**: the segments after
the root are matched against the tree longest-first, and what is left
over is looked up as items of the module that answered. So
`pkg::a::b::c` is the module `a/b/c.vl` when that file exists, and
otherwise the item `c` of the module `a/b.vl`, and otherwise the item
path `b::c` under `a.vl`.

A segment that names **both** — `a.vl` declares an item `b` and `a/b.vl`
exists — is an **ambiguity error**, the same answer `a.vl` beside
`a/lib.vl` gets (§4.1): one of the two has to be renamed. A module's own
items are matched before its directory's files, so the declaration is
what an import would land on and the file could not be reached at all;
a silent winner here is what makes a rename on either side change what
every existing `pkg::a::b` means. The refusal is raised at the
declaration, once, whether or not anything imports the path.

Every segment must match the on-disk directory entry **byte for byte**: a
case-insensitive filesystem that answers `import foo` with `Foo.vl` is a
**diagnostic naming both spellings**, not a resolution, so that a program
compiles identically on case-sensitive and case-insensitive filesystems
([design notes](https://github.com/vilan-lang/proposals/blob/main/proposal/windows-support.md) §5). Every component of the resolved path
carries the rule — directory components included — so `foo/lib.vl` is
reached by `import foo` only when the directory is spelled `foo`, and
`a/b.vl` by `import pkg::a::b` only when both are.

## 4.3 Imports

`import path` (§3.2) loads the target module (once per program: loading
is idempotent and cycle-tolerant) and binds the imported items in the
importing module's scope:

- `import std::io::print;` binds the item `print`.
- `import std::reactive::{ Signal, SignalCell, combine };` binds each set member.
- `import std::option::Option::{ self, Some, None };` is a path into a
  TYPE: `self` binds the type itself; variant names bind the variants for
  unqualified use.
- `import std::style::Length::rem;` is the same path into a type, reaching
  its **statics** — the functions its `impl` blocks declare that take no
  `self` (§4.6). A static is reached through the module whose file writes
  the block, which is the type's own module for a type whose impls sit
  beside it and the extending module for an extension impl; a `self`
  method is not importable under either form, and says so. The set and
  `as` forms reach statics like any other leaf, which is what lets an
  app's own prelude module re-export one bare
  (`export import std::style::Length::rem as rem;`).
- A **type segment replaces** the path's namespace. What follows
  `Length` is resolved in `Length`'s namespace and nowhere else — its
  variants, then its statics — so a module-level `rem` declared beside
  `Length` does not shadow `Length::rem`, and `style::Length::px` does
  not reach the module's `px`. One rule for both kinds: an enum segment
  has always worked this way, and a struct segment does too. A path
  reaching a module's item through a type-named prefix is a diagnostic
  naming the spelling that replaces it (`style::px`).
- `import std::json::Json as Document;` binds `Document`. `as` renames the
  leaf and changes nothing else: the path resolves exactly as it would
  without one, and the imported item is the same item under a second
  spelling — go-to-definition on `Document` lands on `Json`, and a rename
  of `Json` rewrites the alias with every other reference to it. The form
  reaches a set member too (`import a::{ b as x, c };`), and a re-export
  publishes the alias (`export import a::b as c;` publishes `c`).

- `import std::list only;` binds the statement's names and **no
  implementations**. A plain `import` brings every `impl` declared in the
  files on the path to its leaf — that is how an extension impl arrives —
  and `only` is the spelling that declines them.
- `import std::list::{ (impl List<i32>) };` is an **impl selector**: a
  brace-set element that binds no name and admits exactly the `impl`
  blocks of the module whose subject unifies with the written type. A set
  naming one means "these implementations only", so `only` is for
  statements without a selector. `_` is the placeholder — `(impl List<_>)`
  admits every `List` block whatever its element, and `(impl _)` admits
  every block the module declares — and no `type X` binders are written in
  a selector. `import std::list::{ (impl List<i32>)::first };` takes ONE
  member into the type's namespace for this file, and
  `::{ first, last }` takes several; a selector takes no `as`, because a
  method is called by name on a receiver. The subject resolves in the
  IMPORTING file's scope: `impl S` after `import item::Struct as S;`
  reaches `S`, and `impl item::Struct` is the qualified spelling.

`use path` binds names from an already-visible type's namespace without
loading (variants, statics) — the same two kinds `import` reaches by
path, and the difference is only that `use` loads nothing, the type being
in scope already. `export statement` re-exports: importers of this module
see the exported names as if declared here.

**Visibility** is §4.8. In an import path it shows up as one token: `#`
before a leaf is the **reach** marker, `import pkg::a::{ #hidden };`,
which takes an item the module does not export and says so.

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

One module declares a name **once**, whatever the sorts: two `struct N`,
two `fun n`, a `struct N` beside an `enum N` or a `trait N` are all an
error at the second declaration, naming the first. Order is not a rule a
reader can see, and without this it decided which declaration a name
meant. Two *modules* may each declare `N` freely — a module is a
namespace, and `shapes::N` and `colors::N` are two types the program can
name apart.

The full ladder, weakest first: the **prelude** (§4.7) → module items and
explicit **imports** → enclosing scopes → the innermost local binding.
Two rungs deserve stating outright, because both are silent:

- An **explicit import beats a same-file declaration.** `import
  std::io::print;` in a file that also declares `fun print` binds the
  import; the file's own function is unreachable by that name.
- **Everything beats the prelude.** It is the weakest rung by
  construction, so shadowing one of its names — by declaring it or by
  importing it — is never a diagnostic.

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
SignalCell<i32>` annotations in the same scope, but relying on this is poor
style.

A type may also be **qualified** by the modules that declare it —
`style::Style`, `std::reactive::SignalCell<i32>` — in any type position
(grammar §3.9, types §5.1). The namespace segments resolve as §4.2's
paths do, so an imported module reaches its types as it reaches its
values. The preference above does not apply to a qualified path: it
addresses exactly what that one namespace declares, so a member that is
not a type is an error there rather than a reason to keep looking.

## 4.6 Statics and members

`Type::member` (§3.6) resolves `member` in `Type`'s namespace: enum
variants, the static functions of the type's impls (those without `self`),
and the type's own `self`-methods. Generic statics take their arguments at
the path head: `List<str>::new()`.

A type has **one namespace per importing file**, and receiver position is
not part of a name. The namespace is the union of the `impl` blocks that
file's own import statements admit (§4.3): a plain `import a::b;` admits
every block declared in `a`'s file and in every file on the path to it,
`import a::b only;` admits none, and
`import a::{ (impl T) }` admits exactly the blocks whose subject unifies
with `T`.

Two impls declaring the same name for one subject — two statics, two
methods, or one of each — are refused, and *where* depends on how far
apart they are declared:

- **In one module**, at the declaration. Nothing ranks them and one would
  simply never be reachable, so the file that writes both is told so.
- **In two modules**, at the **import**, spanned on the second import
  statement and naming both blocks. Each module stays importable on its
  own; it is the file that takes both that has to choose. The fix is an
  impl selector, which is why the refusal is reported where selectors are
  written: `import pkg::z::{ (impl Thing)::tag };` takes one member from
  one block and leaves the other module's alone.

That is what lets two independent extension modules declare the same
method for the same type and both ship: a file that never imports both
never sees a conflict, and the refusal is a fact about the FILE's imports
rather than about the order modules happened to load in.

It is also the **whole** namespace of the path that names it: `Type::`
replaces what the rest of the path resolves against rather than adding to
it (§4.3), so the module the type is declared in does not answer through
it. A name that is not in the type's namespace is not in the path's.

That namespace is what `import` and `use` reach into (§4.3), and receiver
position IS the line they draw: a static binds to a bare name, and a
`self`-method does not — it is called on a value, so a bare binding would
name something uncallable. The refusal says so rather than reporting the
member as missing.

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
ambiguity error — as does a `T: A + B` bound whose two arms supply it. Two
impls of **one** trait for one subject are rejected at the definition site
too, by the coherence rule of §5.4: rule 2 above says "the method a trait
provides", and a trait provides one.

```vilan
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

The **impl selector** is the other disambiguator, and it answers a
different question: `Trait::member` picks between two providers both of
which the file can see, while a selector decides which blocks the file
sees at all. A file that would otherwise take one name from two modules
writes `import pkg::z::{ (impl Thing)::tag };` and the rest of `z`'s
blocks stay out of its namespace. Both spellings are per file, and
neither changes what any other file resolves.

## 4.7 The prelude

Two sets of names are in scope without imports.

**The built-in set**, always: the primitive types (`i32`, `str`, `bool`,
…), `List`, `void`, and the boolean/`null` literals' types. (The exact
set is the lang-item table, appendix §A.4.)

**The package's prelude**, chosen by its manifest. A prelude is a
**module**, and the names it makes ambient are exactly that module's
exports. `[package] prelude` (and `[library] prelude`) names it:

| Value | Ambient names |
|---|---|
| *omitted* — the default | std's base set: `print`, `Option`, `Some`, `None`, `Result`, `Ok`, `Err` |
| `"std::web"` | the base set, plus `Signal`, `SignalCell`, `view`, `View`, the five slot values (`when`, `swap`, `each`, `each_values`, `each_by`), and the **modules** `style` and `ui` |
| any module path (`pkg::…`, `std::…`, a dependency) | that module's exports |
| `false` | none — only the built-in set above |

```vilan
fun main() {
	// `Option`, `Some`, `None` and `print` with no import: the language
	// manufactures the `Option` here, so it can also name it.
	match [10, 20, 30].get(1) {
		Some(let n) => print("found one"),
		None => print("empty"),
	}
}
```

A prelude entry may name a **module** as well as a member. An ambient
module contributes exactly one name to the bare namespace — its own — and
its members are reached through it, so `style::Display` needs no import
while bare `Display` still means `std::display::Display`.

**The prelude is the weakest binding in the language.** A local
declaration or an explicit import of a prelude name wins, silently, with
no diagnostic (§4.4). A file that declares its own `fun print` gets its
own; a file that imports `std::io::print` gets that import, and the
import is simply redundant rather than an error.

**A prelude is per package and never inherited.** A dependency's files
resolve under the prelude *its* manifest declares — not its consumer's,
not a workspace root's, and not per platform layer. Two packages that
disagree about what `Signal` means both keep compiling in one build.

A shadowed prelude name has no qualified spelling at the use site:
`std::io::print(x)` written inline is refused (§4.6). Recover by importing
the module and qualifying through it — `import std::io;` then
`io::print(…)`.

The standard library itself declares `prelude = false`, so its own
resolution stays greppable.

## 4.8 Visibility

Every top-level item carries one bit: **exported**, or the module's own.
The default is the module's own — a declaration with no marker is
private.

**The four forms.**

| form | means |
|---|---|
| `export fun helper()` | this item is the module's surface. Any declaration takes it: `fun`, `struct`, `enum`, `trait`, `impl`, a module-level `let`, a `macro`, a `mod` block |
| `export *;` | every item of this module is exported. One bare statement at the module's top level; `vilan fmt` puts it just below the file's leading import run |
| `export(in PATH) item` | narrower than `export`. `export(in mod)` keeps one item private under an `export *;`; `export(in pkg)` publishes it to its own package and no further |
| `export import pkg::io::print;` | a **re-export**: importers of this module see `print` as if it were declared here. This is how a prelude module is written (§4.7) |

`export` on an `impl` block means what it means on every other
declaration. A block a consumer cannot see contributes **no methods** to
that consumer — not even as an ambient impl arriving along a path — so a
curated module writes `export impl Thing { … }` for the blocks it means
to publish. A module with no marker anywhere is **uncurated** and offers
everything, which is what keeps a package that has never thought about
visibility compiling exactly as it did.

**Visibility never blocks access.** The bit is consulted by completion,
by the add-import fix, by the "import it first" steer and by the
per-importer method namespace (§4.6) — and by nothing in name resolution.
An importer who needs a private item writes the **reach**:

```text
import pkg::util::{ #helper };     // "not exported, and I want it anyway"
```

A plain `import pkg::util::helper;` of an unexported item still compiles
and **warns**, naming the marked spelling; so does a qualified reach
through the module (`util::helper()` after `import pkg::util;`), which
has no leaf to mark. The reach marker on an item that IS exported warns
the other way — it says something about the author's belief that is not
true — and the fix is to delete one character.

Reaching a **dependency's** unexported item is no diagnostic at all, at
any release. Whether an item should be exported is its author's
judgement, and a consumer's need is real evidence against it; the marker
is available there too and says the same thing, but nothing asks for it.

**The second warning is about signatures.** An exported item whose
signature names an unexported type is reported once at the declaration:

> `S` is returned here. `my_fun` is exported, but `S` is not. A consumer
> can call `my_fun` but cannot name the return type.

Its reach is the signature positions and only those — a module-level
`let`'s type, a parameter type, a return type, an exported struct's field
types, an exported enum's variant payloads, a declared bound, and a
generic argument in any of them. A private type used inside an exported
function's **body** is exactly the encapsulation the bit exists to
permit, and is never reported. The fix is to export the type; when the
type belongs to a dependency there is no fix, and the message says so.

The standard library is curated: its modules export what they publish and
keep their own machinery private, which is why `vilan check` on a program
that reaches into std's internals tells you so.
