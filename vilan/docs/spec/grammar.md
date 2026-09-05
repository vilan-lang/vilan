# Spec §3 — Grammar

The full syntactic grammar, in the notation of §1.3. Token classes
(`IDENT`, `NUMBER`, `STRING`, …) are defined in §2. The start symbol is
`module`.

## 3.1 Modules and statements

```text
module    = { statement } ;

statement = derived-item
          | service-item
          | macro-attributed-item
          | macro-fun
          | macro-block [ ";" ]
          | macro-invocation [ ";" ]
          | "export" statement
          | expression ";"
          | if-expr        (* not before "}" — see below *)
          | for-expr       (* not before "}" *)
          | match-expr     (* not before "}" *)
          | function
          | struct
          | enum
          | impl
          | trait
          | "mod" IDENT "{" { statement } "}"
          | import ";"
          | use ";"
          | block          (* not before "}" *)
          ;
```

A block-like form (`if`/`for`/`match`/`{…}`) in statement position must
not be the last thing in its enclosing block: in that position it is
instead the block's **trailing expression** and supplies the block's value
(§3.5).

## 3.2 Imports and exports

```text
import  = "import" path-branch ;
use     = "use"    path-branch ;
path-branch = NAME [ "::" ( path-branch | path-set ) ] ;
path-set    = "{" path-branch { "," path-branch } [ "," ] "}" ;
NAME        = IDENT | "true" | "false" ;   (* variant re-exports *)
```

`import` brings names from another module into scope; `use` brings names
from a type's namespace (e.g. variants) into scope. In a set, `self` names
the item itself (`Option::{ self, Some, None }` imports the type and its
variants). Semantics: §4. `export statement` re-exports an import or
exposes a declaration to importers of the module.

## 3.3 Items

### Functions

```text
function = [ "[" "deprecated" "(" STRING ")" "]" ]
           [ extern-attr ] [ "[" "must_use" "]" ] [ "[" "rpc" "]" ]
           [ "[" "trait_only" "]" ] [ "[" "doc" "(" "hidden" ")" "]" ]
           [ "[" "platform" "(" STRING { "," STRING } [ "," ] ")" "]" ]
           [ "async" ] [ "external" ]
           "fun" IDENT [ generic-params ]
           "(" [ parameter { "," parameter } [ "," ] ] ")"
           [ ":" type ] [ "borrows" IDENT ]
           ( block | ";" ) ;

parameter  = [ "mut" | convention ] [ "..." ] binder [ ":" type ] ;
convention = "own" | "&" [ "mut" ] ;
binder     = IDENT | "(" binder "," binder { "," binder } [ "," ] ")" ;

extern-attr = "[" "extern" "(" extern-args [ "," "retains" ] [ "," ] ")" "]" ;
extern-args = STRING [ "," STRING ]              (* global, or module and symbol *)
            | "method" [ "," STRING ]            (* method on the receiver *)
            | ( "get" | "set" ) "," STRING       (* property read / write *)
            | "new" "," STRING [ "," STRING ] ;  (* construction *)
```

The four arms are the four host bindings. `"method"` is the only form
word that stands alone: `[extern(method)]` binds the receiver method of
the same name as the vilan function, and `[extern(method, "on")]` names a
different one. `get` and `set` always take their property name — a bare
`[extern(get)]` is not an accessor, it is a malformed attribute, and
lowers as one. `new` reads its strings exactly as the first arm does: one
is a global class (`[extern(new, "TextDecoder")]`), two are a module and
the class imported from it (`[extern(new, "node:sqlite", "DatabaseSync")]`),
and the call sites emit `new Symbol(…)` because a host constructor
rejects a plain call.

`retains` is a **flag**, not a binding form: it is recognized in trailing
position only — the one place it cannot displace a form word — and so
composes with every shape above rather than needing an arm per
combination. It declares that the host keeps an argument past the call;
§6.8's [Externs and retention](memory.md#externs-and-retention) gives the
semantics. Elsewhere in the argument list it is an unknown argument, and
the attribute lowers to the empty global symbol as any other malformed
extern attribute does.

A `;` body is a signature-only declaration: legal for `external`
functions and required trait methods. A parameter's **convention** may
come from the prefix (`own x`, `&mut self`) or from a view type
(`x: &mut T`); the prefix wins if both are present (§6.3). The `borrows`
clause names the parameter the returned view projects (§6.5). A **closure
literal's** parameters take this same rule, conventions included, so a
callback can receive a writable view: `signal.update(|&mut list| { … })`
against a `sync |&mut T| void` parameter.

A leading `mut` is **binder mutability**, not a convention: the body may
rebind and field-write its by-value copy, invisibly to the caller —
`fun f(mut x: T) { … }` is `fun f(x': T) { mut x = x'; … }`. It applies
to a plain name binder (including `self` and closure parameters), never
combines with a convention, is not part of the signature (trait
conformance ignores it), and is rejected on an `external fun` (no body).
A resource cannot be taken `mut` (a resource never copies; take it
`own`).

A leading `...` marks a **spread parameter**: a call convention over an
ordinary tuple parameter, where the call site writes the pack's elements
out flat — `fun f(...items: T) { … }` is `fun f(items: T) { … }` with
`f(a, b)` meaning `f((a, b))` (§5.9). It must be the **last** parameter
(so at most one per signature), must declare its type, and takes a plain
name binder. Unlike `mut` it **is** part of the signature, and it is
rejected outside a free `fun`: on a closure literal, on a trait
declaration or any `impl` member, and on an `external fun`. It never
combines with a convention — the argument is a tuple the *call site*
builds, so there is nothing to transfer or alias — but `mut` may precede
it (`mut ...items: T`).

The attribute prefix is **ordered** — each attribute is optional, but
they appear in exactly the production's order. `[deprecated("use …")]`
leads it: the function is **deprecated**, and every use in code outside
the standard library — a call, a method call, the function passed as a
value — still compiles but raises the non-fatal warning
`` `{name}` is deprecated; {steer} ``, anchored at the using name, once
per use site. The one required argument is the replacement steer,
carried into the warning verbatim; by convention it reads `use …`
(`[deprecated("use two()")]` warns `` `one` is deprecated; use two() ``).
Uses *inside* the standard library are silent — std migrates its own
callers in the release that deprecates. The attribute is honored
wherever it appears, in std and user code alike, and dies with its item:
when the function is removed, so is the mark. When the item goes away is
the CHANGELOG's fact, not the source's — the removal comes no earlier
than the minor release after the warning first shipped.

### Structs and enums

```text
struct = [ "resource" ] [ "external" ] "struct" (IDENT | "null") [ generic-params ]
         ( "{" [ field { "," field } [ "," ] ] "}" | ";" ) ;
field  = [ "[" "expose" "]" ] IDENT [ ":" type ] ;

enum          = [ "resource" ] "enum" IDENT [ generic-params ]
                "{" [ variant { "," variant } [ "," ] ] "}" ;
variant       = NAME [ "(" [ type { "," type } [ "," ] ] ")" ]
                [ "=" backing-value ] ;
backing-value = [ "-" ] INTEGER | STRING ;
INTEGER       = NUMBER without a fractional part and without a SUFFIX ;
```

A `;`-bodied struct is legal only for `external` structs (host types). An
explicit variant **backing value** — `= 0`, `= -1`, `= "start"` — fixes
what the variant *is* at runtime. An enum whose variants carry one is a
*backed enum* and lowers to that bare value; see §5.3 of the types
chapter.

The two backing types are the integers and `str`, and nothing else. A
float is rejected for the same reason its equality is: the lowering is
`===`, on which `0.1 + 0.2` is a footgun and `NaN` is not even equal to
itself. `bool` is rejected because `bool` is itself an enum that already
lowers to native `true`/`false`, so a two-variant bool-backed enum is
`bool` with extra steps.

An integer backing value is an **integer**, not a general `NUMBER`: a
fractional part (`= 1.5`) and a type suffix (`= 1u32`, and `= 1_000`,
which lexes as `1` with the trailer `_000`) are both errors rather than
being silently discarded. Hex is read as hex (`= 0xFF` is 255). The value
must lie in **`-9007199254740991 ..= 9007199254740991`** — `i53`, the
widest integer a runtime number holds exactly — because a backed enum *is*
that number at runtime, and a discriminant past the bound would reach the
host as a different value than the source wrote. The bound is symmetric.
The implicit continuation obeys it too: a variant with no backing value
takes the previous variant's plus one, starting at 0, and running past the
bound is an error rather than a wrap.

**A string backing must be written on every variant.** There is no
successor of `"start"` for the continuation rule to hand out, and the
string is deliberately not derived from the variant name — the two are
independent (`AlignItems::Start` is `"flex-start"`, `Display::Hidden` is
`"none"`), and a naming convention that is right most of the time would
be silently wrong the rest.

**One enum has one backing type.** The type is fixed by the first
explicit value in declaration order and every later value must agree:
`enum X { A = 1, B = "two" }` is rejected. An enum has one runtime
representation, and a value that is sometimes a number and sometimes a
string is not a vilan type.

**Two variants may not share a backing value**, whether written or
continued — `enum Dup { A = 1, B = 1 }`, `enum Walked { A = 1, B = 0,
C }`, and `enum Align { Start = "a", End = "a" }` are all rejected.
Sharing one would make two variants a single runtime value (see §5.3 of
the types chapter), leaving the second `match` arm unreachable in an
otherwise exhaustive match.

**A backing value is only legal when every variant is data-less.** A
variant carrying a payload may not carry one, and neither may its
data-less siblings: an enum with any payload variant uses the tagged
representation, in which a bare backing value has nowhere to put a
payload.

The leading `resource` modifier marks a type declaration as a *resource*:
the owned-resource class, whose semantics are specified in
[§6.8](memory.md). It
precedes `external`, so the full modifier order is `resource external
struct`, and it is accepted only on `struct` and `enum` declarations;
`resource` before any other item is a parse error.

### Impls and traits

```text
impl  = "impl" type [ "with" type { "+" type } ] "{" { statement } "}" ;
trait = "trait" IDENT [ generic-params ] [ "with" type { "+" type } ]
        "{" { function } "}" ;
```

An impl's subject is a **type pattern**: `type X [: bounds]` binders
anywhere inside it (`impl List<type T>`, `impl Option<(type T, type U)>`,
bare `impl type T`) declare the impl's generic parameters (§5.6). `with`
lists the implemented trait(s). An impl without `with` provides inherent
members. A trait's `with` lists supertraits.

### Generic parameters and arguments

```text
generic-params = "<" generic-param { "," generic-param } [ "," ] ">" ;
generic-param  = [ "type" ] IDENT [ ":" ( bound-list | tuple-bound ) ]
                 [ "=" type ] ;
bound-list  = type { "+" type } ;
tuple-bound = "(" [ NUMBER ] ".." [ NUMBER ] [ ":" type ] ")" ;
generic-args = "<" type { "," type } [ "," ] ">" ;
```

A tuple bound constrains a variadic tuple parameter's arity and,
optionally, each element (`T: (2..)`, `T: (..: Display)`); see §5.9.

### Attributes and macro items

```text
derived-item   = "[" "derive" "(" IDENT { "," IDENT } [ "," ] ")" "]"
                 ( struct | enum ) ;
service-item   = "[" "service" [ "(" IDENT ")" ] "]" struct ;
macro-attributed-item = "[" IDENT [ "(" [ expr-span { "," expr-span } ] ")" ] "]"
                        ( struct | enum | function ) ;
macro-fun        = "macro" function ;
macro-invocation = "macro" IDENT "(" [ expr-span { "," expr-span } ] ")" ;
macro-block      = "macro" block ;
```

A macro attribute's arguments are captured as **source spans**: the
macro receives their text, not their values (§10). The built-in
attribute names (`derive`, `service`, `extern`, `must_use`, `rpc`,
`trait_only`, `doc`, `expose`, `platform`, `deprecated`) are not
available as user macro-attribute names.

## 3.4 Bindings and assignment

```text
let        = ("let" | "mut") binder [ ":" type ] [ "=" expression ] ;
assignment = [ "*" ] place ( "=" | "+=" | "-=" | "*=" | "/=" | "%=" )
             expression ;
place      = chain ;                 (* an assignable location, §3.6 *)
ret        = "ret" [ expression ] ;
jump       = "jump" IDENT ;          (* break | continue *)
```

`let` binds immutably, `mut` mutably; a tuple binder destructures
(irrefutably: names and nested tuples only). Both the type and the
initializer are syntactically optional. A **place** is a chain expression
(§3.6) denoting a location: a local, a field chain, an index, or a place
reached through a call (`a.write().count`); the optional leading `*`
assigns through a view. `jump break` / `jump continue` control the
innermost enclosing loop.

## 3.5 Blocks and control expressions

```text
block      = "{" { statement } [ expression ] "}" ;
if-expr    = "if" condition-expr block [ "else" ( block | if-expr ) ] ;
for-expr   = "for" IDENT "in" condition-expr block   (* iteration *)
           | "for" condition-expr block              (* while *)
           | "for" block ;                           (* infinite *)
match-expr = "match" condition-expr "{" { match-leg [ "," ] } "}" ;
match-leg  = pattern { "," pattern } [ "if" expression ] "=>" expression ;
```

A block's value is its trailing expression, or `void` if none. Conditions
and `match`/`for` subjects are **condition expressions** (§3.8): struct
initializers and `css` blocks are excluded there, keeping `if Foo {`
unambiguous. A match leg's comma-separated patterns form an or-pattern;
the optional `if` guard applies to the whole leg; the trailing comma
after a leg is optional.

A `for IDENT in` subject must have **one element type**, which the binder
takes: a `List<T>`, a `Set<T>`, a `[T; n]`, a `str` (yielding characters),
or any type providing the iterator protocol `next(&mut self): Option<T>`
(§5.7 dispatches it; `for e in &mut c` drives `next_mut` and binds each
element as a view, §6). A **tuple is not iterable**, and the loop over one
is refused: a tuple is a fixed sequence of independently typed elements
rather than a container of one element type, so there is no single type
for the binder to take and one body cannot be checked for every element.
Read the elements positionally (`t.0`, `t.1`) or destructure the tuple
(§5.9). The same holds for a **mapped tuple** `(U in T: F<U>)`, whose
elements have no positions to read while `T` is abstract: its element-wise
form is the tuple comprehension. *Whether such a loop should instead be
UNROLLED — the body checked and emitted once per element, at that element's
own type — is recorded future work; the refusal is forward-compatible with
it.*

## 3.6 Chain expressions (postfix)

The tightest expression tier, `chain`:

```text
chain   = path { call-suffix | postfix } ;
path    = ( IDENT generic-args ␣"::"  (* generic static head *)
          | atom )
          { "::" IDENT } ;
call-suffix = [ generic-args ] "(" [ entry { "," entry } [ "," ] ] ")" ;
member  = NUMBER                          (* tuple index: .0 *)
        | IDENT [ call-suffix ] ;         (* field / ONE fused method call *)
postfix = "." member
        | "[" expression "]"             (* index *)
        | "!"                            (* try-assert, §5.10 *)
        | "(" [ entry { "," entry } [ "," ] ] ")"
                                          (* direct call on the chain result *)
        | "?." member                    (* lift link, §5.10 *)
        | "?" ;                           (* expression lift, §5.10 *)

atom    = literal | IDENT | IDENT generic-args | struct-init
        | "(" expression ")" | tuple | list
        | tuple-comprehension | macro-invocation | macro-block
        | element | css-block ;
literal = NUMBER | STRING | "true" | "false" | "null" ;
tuple   = "(" ( spread | expression "," entry { "," entry } [ "," ] ) ")" ;
entry   = spread | expression ;
spread  = ".." expression ;
list    = "[" [ expression { "," expression } [ "," ] ] "]" ;
tuple-comprehension = "(" IDENT "in" secondary-expr "=>" expression ")" ;

element      = "<" element-name { head-item }
               ( "/>" | ">" { child } "</" element-name ">" ) ;
head-item    = "." member                          (* a chain link, verbatim *)
             | "on" ":" IDENT "(" expression ")"   (* event form *)
             | element-name [ "(" expression ")" ] ;
                                          (* attribute; bare name = boolean *)
element-name = NAME { "-" NAME } ;   (* NAME: an identifier or any keyword *)
child        = element | STRING | ISTRING | "{" expression "}" ;

css-block    = "css" css-body ;        (* atom position; excluded in conditions *)
css-body     = "{" { css-item } "}" ;
css-item     = css-declaration | css-rule ;
css-declaration = css-property ":" css-value ";" ;
css-property = { "-" } element-name ;  (* span-adjacent, as an element name is *)
css-rule     = "." IDENT [ "(" [ expression { "," expression } [ "," ] ] ")" ]
               css-body ;
css-value    = css-piece { css-piece } ;   (* to the ";" at brace depth 0 *)
css-piece    = "{" expression "}"          (* a hole *)
             | TOKEN ;                     (* any token but ";", "{", "}" *)
```

`Name<Args>` is read as a generic path head only when `::` immediately
follows (`List<str>::new()`); otherwise `<` is a comparison. A member
fuses at most ONE call; a further `(args)` is a **direct call** on the
chain's result, calling a closure-typed value
(`self.hook.read()(a, b)`). A `?.` link's **continuation** extends
through the following plain postfixes up to the next `?.` or `!`:
`a?.b.c()!` lifts `b.c()` into the container, then try-asserts the
result (§5.10). A bare `?` — a `?` with no `.` after it — is the
**expression lift**: it takes no member and lifts its whole enclosing
SLOT rather than a continuation, so the slot receives the container
(§5.10). It has been in the language since 2026-07-16 and had no
production here until now.

A leading `..` marks a **tuple-value spread** (§5.9). It is recognized
only where an *entry* begins — a tuple construction's entry, or a call
argument — so `..` after an expression is unaffected and remains the
member-access dots it has always been (`(1..3, x)` is not a spread). A
tuple construction whose only entry is a spread is still a tuple, not a
parenthesized group: `(..a)` is the concatenation of one, and `(e)` is a
group as before. There is no type-level spread; `(..T, U)` does not
parse.

An **element** appears only in atom position, where `<` begins no other
expression; after an operand, `<` remains a comparison (`x < <div/>` is
a comparison whose right operand is an element). `/>`, the closing
marker `</`, the `on:` joint, and the `-` joints of a hyphenated name
are **span-adjacent** token pairs, the shift-operator discipline
(lexical spec §2.4). The closing tag's name must match the opening
tag's token for token. In a head item, an undotted name is an attribute
(a bare name is a boolean attribute) and a leading `.` is an ordinary
chain member — the grammar never consults any method list. Text
children are quoted strings; bare text is a parse error. An element is
an ordinary expression: it desugars before analysis to the `std::ui`
view chain (`view("tag")` with one method call per head item and a
`.child(…)` per child), and postfix suffixes apply to it
(`<div />.show(flag)`).

A **`css` block** is the same shape on the style side, and it appears in
atom position too — but where an element occupies grammar space nothing
else could want, a `css` block is **brace-initial**, so it is excluded
from condition operands exactly as a struct initializer is (§3.8).
`css` is a reserved word (lexical spec §2.2); the block is the keyword
followed immediately by `{`.

Inside the body the **dot decides, and decides alone**: an undotted item
is a *declaration* and a dotted one is a *condition rule*, so the
grammar never consults any method list and a method added to `Style`
cannot change what an existing block means. A property name is a
span-adjacent name-`-`-name run, the element-name rule (so
`flex-direction` is three tokens and `--color-ink` is five, while
`data - id` is arithmetic). The `;` is **required** after every
declaration, the last one included: the formatter may never invent a
token, and a required terminator makes value scanning decidable in one
pass. A value is a run of tokens and `{expression}` holes — there is no
typed value grammar, and typed values arrive through the holes. A
condition rule's parenthesized arguments are ordinary expressions
(`.within("data-theme", "dark") { … }`).

Like an element, a block is an ordinary expression that desugars before
analysis — to the `std::style` chain: `style()`, then `.raw(property,
value)` per declaration and `.name(args…, style() … )` per condition
rule, with the rule's own chain appended as the final argument, in
written order. `#` and `@` do not lex at all, so there are no hex
literals and no at-rules inside one (lexical spec §2.4).

## 3.7 Operator precedence

From tightest to loosest; every binary level is left-associative:

| Level | Operators | Notes |
|---|---|---|
| 1 | `::` paths, calls, `.` `[]` `!` `?.` `?` | §3.6 |
| 2 | prefix `!` `-` `await` `async` `&` `&mut` `*` | unary; `async` also takes a block |
| 3 | `*` `/` `%` | |
| 4 | `+` `-` | |
| 5 | `<<` `>>` | the two control tokens must be span-adjacent |
| 6 | `&` | bitwise and |
| 7 | `^` | bitwise xor |
| 8 | `\|` | bitwise or |
| 9 | `==` `!=` `<` `<=` `>` `>=` | one level; `a < b < c` parses as `(a < b) < c` (ill-typed, §5.7) |
| 10 | `is` pattern | at most one per operand (no chaining) |
| 11 | `&&` | |
| 12 | `\|\|` | |

Bitwise operators bind tighter than comparisons (`a & b == c` is
`(a & b) == c`).

## 3.8 The expression tiers

```text
expression     = "const" expression        (* weak prefix: captures to the end *)
               | secondary-expr ;
secondary-expr = closure | block | if-expr | for-expr | match-expr
               | jump | let | ret | assignment
               | operator-expr ;           (* §3.7 levels 1–12 *)
condition-expr = secondary-expr ;    (* struct-init and css-block excluded *)

struct-init   = type-path                      (* §3.9; qualified heads too *)
                "{" [ init-field { "," init-field } [ "," ] ] "}" ;
init-field    = IDENT [ "=" expression ] ;   (* shorthand: name alone *)
closure       = ( "||" | "|" [ closure-param { "," closure-param } [ "," ] ] "|" )
                [ ":" type ] expression ;
closure-param = parameter ;   (* the same rule as a function's, less "..." *)

operator-expr  = unary-expr { binary-op unary-expr } [ "is" pattern ] ;
                 (* SHAPE only: §3.7's table is normative for precedence and
                    associativity, and for the one-`is`-per-operand rule *)
unary-expr     = { "!" | "-" | "await" | "async" | "&" [ "mut" ] | "*" }
                 ( chain | block ) ;   (* "async" also takes a block *)
binary-op      = "*" | "/" | "%" | "+" | "-" | "<<" | ">>"
               | "&" | "^" | "|"
               | "==" | "!=" | "<" | "<=" | ">" | ">="
               | "&&" | "||" ;
```

Two consequences of the tier split are normative:

- A **struct initializer** is an operand of the operator/postfix chain
  (`Point { … } == q` compares; `Point { x = 1, y = 2 }.length()` folds
  the member chain), except in **condition positions**: an `if`/`for`
  condition, a `for … in` iterable, and a `match` subject parse
  `condition-expr`, whose operands exclude struct initializers, so the
  `{` after `if Foo` is the block. Parenthesize a literal to use it in a
  condition (`if p == (Point { x = 1 }) { … }`). The head is a
  `type-path`, so a **qualified** literal (`shapes::Dot { x = 1 }`) is the
  same production and takes the same exclusion — `if k ==
  shapes::Kind::Flat { … }` reads the `{` as the block. A **`css` block**
  (§3.6) is brace-initial for the same reason and is excluded in the
  same three places, with the same escape: `if (css { … }).class_list()
  != "" { … }`.
- `const` captures **weakly**: everything to the end of the expression
  (up to the enclosing bracket or comma) folds; parenthesize to narrow
  (§9).

A closure's body is one expression (commonly a block). `||` in operand
position always begins a zero-parameter closure; logical-or is only
recognized between two operands.

## 3.9 Types

```text
type = "&" [ "mut" ] type                       (* view type *)
     | "type" IDENT [ ":" bound-list ]          (* impl-subject binder *)
     | [ "async" | "sync" ] closure-type [ context-clause ]
     | type-path                                 (* nominal *)
     | "(" IDENT "in" type ":" type ")"          (* mapped tuple, §5.9 *)
     | "(" [ type { "," type } [ "," ] ] ")"     (* tuple type *)
     ;
type-path      = IDENT { "::" IDENT } [ generic-args ] ;
closure-type   = ( "||" | "|" [ [IDENT ":"] type { "," [IDENT ":"] type } "|" )
                 [ type ] ;
context-clause = "context" ( IDENT | "(" IDENT { "," IDENT } [ "," ] ")" ) ;
```

`context` here is the contextual keyword (§2.2); the clause is only valid
on closure types, checked semantically (§8.5). `sync` is likewise
contextual (§7.4: the synchronous contract; parameters only). A closure
type's parameters may carry documentation names (`|value: T| U`); only
the types are significant.

A `type-path` names a type, optionally through the modules that declare
it: `Style`, `List<T>`, `style::Style`, `std::reactive::SignalCell<i32>`.
Every segment but the last selects a namespace, and the resolution is the
one an expression path uses (§4.2, §4.6) — so a module in scope reaches
its types exactly as it reaches its values. Generic arguments belong to
the LAST segment, the only one that names a type; there is no
`List<str>::Item` in type position (a generic path head, §3.6, is an
expression form).

Every type position takes a `type-path`: a return type, a `let`
annotation, a parameter, a struct field, an `impl` subject, a trait
bound, a generic argument, and any of these nested inside another type
form.

## 3.10 Patterns (match)

```text
pattern = ("let" | "mut") binder                (* binding *)
        | "(" pattern "," pattern { "," pattern } [ "," ] ")"
        | STRING | MULTILINE_STRING | NUMBER    (* equality literal *)
        | "_"                                   (* wildcard *)
        | NAME { "::" IDENT }
          [ "(" [ pattern { "," pattern } [ "," ] ] ")" ] ;  (* variant *)
```

Bindings inside patterns are written explicitly (`Some(let x)`), so a
bare name is always a **variant** reference, never a fresh binding: the
classic mistyped-variant trap is a resolution error instead of a silent
catch-all. `bool` and `null` literals match as variants of their enums.
The `let`/parameter binder grammar (names and tuples, §3.3) is the
irrefutable subset; refutable forms (literals, variants) are match-only.
A tuple pattern is irrefutable only when its elements are: `(let a, let b)`
is a destructure and matches everything, `(1, 2)` is a test.

A `match` must be **exhaustive**, and exhaustiveness is proven by
**unguarded** legs only — a guard tests the value, which the check does
not reason about, so a guarded leg proves nothing about what the match
covers. What an unguarded leg proves is the value-space its whole pattern
**tree** covers, not its root, and the check descends accordingly:

- an **enum** position needs every variant named, and each named
  variant's payload positions covered in turn, so
  `Pair::Of(Align::Start)` alone leaves `Pair::Of(Align::End)` missing
  however few variants `Pair` has;
- a **tuple** position needs its elements covered — as a product, so
  `(1, 2)` and `(3, 4)` leave every other pair missing;
- an **open** position (`i32`, `str`, a struct, a still-abstract type
  parameter) is covered only by a binder or `_`. No number of literals
  exhausts it: `Wrapped::Of(1)`, `Wrapped::Of(2)`, … is never total.

A subject whose type is not yet known, or is `any`, `never`, or a generic
parameter, is exempt: the question is which values the subject can take,
and there is no answer to give. The diagnostic names one uncovered value
as a pattern that would cover it — *"missing `Pair::Of(Align::End)`"* —
or asks for a catch-all where the hole is the whole domain. The
consequence lands on the **last** leg, the one that answers for whatever
the legs above it did not take: a `match` whose final leg is guarded must
be exhaustive without it, so `match a { A => …, B if c => … }` is refused
whatever the subject's type ("match is not exhaustive"). Write the guard
with a catch-all after it — `B if c => …, _ => …` — and the value the
guard rejects has somewhere to go. A guard is tested wherever its leg
stands, the last one included; no leg is silently promoted to a
catch-all.
