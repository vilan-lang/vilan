# Element syntax — markup as sugar over the `view` chain

Status: **RATIFIED 2026-08-01** — backlog **H8** filed the same day. Prior status: DRAFT 2026-08-01, open calls all settled same day (Reed took every recommendation); the §9.1 widening probe ran green against the repo compiler. Revised 2026-08-01 after first review (Reed): attributes are paren-form (`placeholder("…")` — the first draft's `=`/hole spelling is gone), chain links in the head wear their dot (`.bind_value(draft)`), self-closing tags space before `/>` by convention, component tags deferred entirely, the `on:` handler call settled. The `view` chain (`guide/ui.md`) is not replaced, deprecated, or changed by this proposal — the sugar lowers to it, and the two forms mix freely in one file, one function, one expression.

## 0. Motivation

Vilan courts web developers, and the first thing a web developer writes is markup. What they meet instead is `view("div").child(view("h2").text("Todos"))` — a good API (uniform, typed, no VDOM, every tool works on it) wearing an unfamiliar coat. The coat has real costs beyond familiarity:

- **Nesting reads inside-out.** `vilan-website/src/art.vl` `diagram()` is ~75 lines of one expression, six levels of `.child(view(…))` deep. The structure of the page is there, but you reconstruct it instead of seeing it.
- **Mixed content doesn't exist.** `.text()` is all-or-nothing per element, so prose with an inline `<code>` span is built from fabricated span vocabularies — `pt(…)`, `t(…)`, `leaf(…)` in `vilan-website/src/code.vl` — and the DOM grows wrapper spans that exist only because the API can't put a text node next to an element.
- **The tag is a string argument**, so the one thing every HTML author knows by heart (`div`, `button`, `input`) arrives quoted and parenthesized.

At the same time, the chain is strictly more capable than markup: `bind_each`, `when`, `swap`, `show`, `bind_value`, `bind_draft`, `styled` have no HTML spelling. A sugar that hid them behind a new template dialect would fork the API in two. So the design brief is a meld, not a port: HTML shape where HTML has a shape, the chain — verbatim, dot included — where it doesn't.

## 1. What it looks like

The todos example (`vilan/examples/reactive-ui/todos.vl`), today:

```vilan
view("section")
	.class("todos")
	.child(view("h2").text("Todos"))
	.child(
		view("div")
			.class("row")
			.child(view("input").attr("placeholder", "What needs doing?").bind_value(draft))
			.child(view("button").text("Add").on("click", add))
	)
	.child(view("p").bind_text(remaining.map(|n| format(n) + " remaining")))
	.child(view("ul").bind_each(visible, |todo| todo.id, |todo| todo_row(items, todo)))
	.child(view("p").class("empty").text("Nothing here 🎉").show(visible.map(|list| list.len() == 0)))
```

with element syntax:

```vilan
<section class("todos")>
	<h2>"Todos"</h2>
	<div class("row")>
		<input placeholder("What needs doing?") .bind_value(draft) />
		<button on:click(add)>"Add"</button>
	</div>
	<p>{remaining.map(|n| i"{n} remaining")}</p>
	<ul .bind_each(visible, |todo| todo.id, |todo| todo_row(items, todo)) />
	<p class("empty") .show(visible.map(|list| list.len() == 0))>"Nothing here 🎉"</p>
</section>
```

Mixed content — today's span-fabrication workaround (`vilan-website/src/page.vl`), gone:

```vilan
<p .styled(lead)>
	"The compiler, dev server with hot reload, formatter, test runner, and language
	server live in one small binary. Update any time with "
	<code .styled(leaf)>"vilan upgrade"</code>
	"."
</p>
```

The counter, whole:

```vilan
fun main() {
	mount_root("app", || {
		let count = Signal::new(0);
		<div>
			<h2>"Counter"</h2>
			<button on:click(|| count.set(count.get() + 1))>"+1"</button>
			<p>{count.map(|n| i"clicked {n} times")}</p>
		</div>
	});
}
```

An element is an ordinary expression of type `View`. It goes anywhere a `view(…)` chain goes today: returned from a component function, passed as a prop, matched over, stored in a `List<View>` — and postfix chains hang off it (`<div />.show(flag)` parses and means what it says).

## 2. The one rule

**The head constructs the element; everything between `>` and `</tag>` is a `.child(…)` call.** Inside the head, the dot draws the line Reed's reading draws: attributes are part of what the element *is*; dotted links are the chain — behavior and bindings *attached to it*.

- **Attribute form** — undotted: `name(value)`, or a bare `name` for a boolean attribute. Lowers to `.attr("name", value)` (bare → empty value), nothing more. The value is an ordinary expression in ordinary parens — a string, an `i"…"`, a signal (§5). Hyphenated names (`aria-label`, `data-id`) and keyword names (`type`, `for`) are ordinary attribute names.
- **Chain form** — dotted: any `View` method, written exactly as it appears in a chain: `.styled(card + column)`, `.bind_value(draft)`, `.bind_each(rows, |r| r.id, |r| row(r))`, `.show(flag)`, `.when(cond, || …)`, `.swap(route, |r| match r { … })`, `.style_var("--w", width)`. Nothing is renamed, nothing is wrapped: the closure literal the context model requires at the call site (`reactive.vl`, the injected-closure rule) is *at the call site*. Every future `View` method works in head position on the day it ships, with no grammar change.
- **Event form** — `on:click(handler)`, lowering to `.on("click", …)` / `.on_event("click", …)` (§4).

The dot is load-bearing twice over. Semantically it marks the construction/attachment boundary. Mechanically it is the disambiguator that keeps the desugar name-blind: undotted *always* means attribute, dotted *always* means the chain, so the grammar never consults `View`'s method list — and adding a method to `View` can never change what existing markup means.

Children are elements, string literals (including `i"…"` interpolation), and `{expression}` holes. Each lowers to `.child(…)` in written order. What may fill a child position or an attribute value is decided by the *type system*, not the grammar — §5.

Reactivity stays explicit, and the model stays visible through the sugar: an `if` or a `match` inside a hole runs **once, at build** — exactly as it does in a chain today. Reactive structure is what it has always been: `.show`/`.when`/`.swap`/`.bind_each` in head position, and `Signal` values in slots. The sugar adds no reactive semantics whatsoever; there is nothing to learn about *when this updates* beyond what `guide/ui.md` already teaches.

## 3. Grammar & lexing

**The lexer does not change.** Every token an element form produces already exists: `<` and `>` are `Ctrl` tokens, `/` and `.` are ordinary tokens, tag and attribute names are idents/keywords, values are ordinary expressions inside parens. Lexing stays context-free (`docs/spec/lexical.md` §7) — which is precisely why text children are quoted (§8, alternative 2).

The parser gains one atom. `<` cannot begin an expression today — `parse_atom` has no arm for it — so the form occupies empty grammar space, the same argument `expression-lifting.md` §1 made for bare `?`. Nothing existing reparses: infix `<` requires a left operand, `f<T>(x)` generic-argument backtracking fires only after an ident, and the `no_struct` condition mode is untouched (an element is not a struct initializer).

```text
element    = "<" TAG { head-item } ( "/>" | ">" { child } "</" TAG ">" ) ;
TAG        = IDENT ;                       (* exact case; the SVG-namespace rules are view()'s *)
head-item  = "." IDENT [ generic-args ] "(" [ expression { "," expression } [ "," ] ] ")"
                                           (* chain form *)
           | "on" ":" IDENT "(" expression ")"
                                           (* event form *)
           | attr-name "(" expression ")"  (* attribute *)
           | attr-name                     (* boolean attribute *)
           ;
attr-name  = NAME { "-" NAME } ;           (* NAME = IDENT or any keyword; parts span-adjacent *)
child      = element | STRING | "{" expression "}" ;
```

Disambiguation in the head is one token of lookahead: a leading `.` is chain form; `on` followed by span-adjacent `:` is event form; ident followed by `(` is an attribute with a value; a bare ident is a boolean attribute. An undotted item with more than one argument is an error whose note teaches the rule: *attributes take one value; chain links start with `.`*. `/>` and `</` are reassembled from span-adjacent pairs exactly as `<<`/`>>` are (`eat_shift_operator`). The closing tag must match the opening tag; the mismatch diagnostic points at both spans.

Wrinkles, recorded rather than hidden:

- `hidden.show(f)` — written without a space — parses as the boolean attribute `hidden` followed by the chain link `.show(f)`; the head is positional, not an expression, so no member access is ever intended there. The formatter spaces head items apart.
- A parenthesized expression parses in CHILD position — an i-string child arrives from the lexer as its paren group, and the parser cannot tell `("a" + b)` from one. Spec'd as the quoted-string child forms; the formatter reprints what the spans say. (The first draft's attribute-value wrinkle, resurfaced in the one position that still delimits with something other than parens.)
- An attribute *named* `on` is only reachable as `on(expr)` (one argument, no colon); HTML has no such attribute, and the event form owns `on:`. Consistent, if theoretical.
- The `class`/`styled` clobber (both set the `class` attribute, last write wins — `guide/styling.md`) carries through the sugar unchanged. Not new, not worsened; a lint is out of scope (§7).

## 4. Lowering

Lowering is a pure `Node → Node` desugar in the pre-analysis slot where `lift::rewrite_items` runs — after macro expansion, before the analyzer. The analyzer, transformer, and interpreter never see an element node, so the codegen/interpreter equivalence gate is not exposed at all. Every generated node carries the span of the markup segment it came from, so diagnostics land on what the user wrote.

| Written | Lowers to |
|---|---|
| `<tag …>` / `<tag … />` | `view("tag")` followed by the head items in written order |
| chain form `.m(a, b)` | `.m(a, b)` — verbatim, including generic args and closure literals |
| attribute `name(e)` | `.attr("name", e)` |
| bare `name` | `.attr("name", "")` |
| `on:evt(\|\| …)` (zero-parameter closure literal) | `.on("evt", \|\| …)` |
| `on:evt(\|e\| …)` (one-parameter closure literal) | `.on_event("evt", \|e\| …)` |
| `on:evt(expr)` (not a closure literal) | `.on("evt", expr)` — *(settled 2026-08-01)* a named one-parameter handler is chain form, `.on_event("evt", h)` |
| any child (element, string, `{expr}`) | `.child(…)`, in written order |

Two properties fall out. Attribute order is written order, so SSR serialization (insertion-ordered, byte-stable — `process/ui.vl`) is exactly as deterministic as the chain. And the desugar emits a bare `view` identifier: the file needs `import std::ui::{ view, View }` in scope, same as today. An element form with `view` unresolved gets a tailored note on the normal unknown-name diagnostic ("element syntax lowers to std::ui::view — import it"). No auto-import, no hidden resolution: the sugar is a spelling, not a prelude.

Order sensitivity is likewise inherited deliberately: head items and children apply in written order, exactly as chain links do. The sugar does not reorder, dedupe, or merge — what you write is the chain you get.

One risk, named: an undotted spelling of a *method* — `<div styled(card)>` for `.styled(card)` — is an attribute by the rule, so it lowers to `.attr("styled", card)`. For nearly every method the type system catches it at once (`Style` is no `AttrValue`); the exposed corner is str-typed methods (`text("hi")` as a head item would silently set a `text` attribute). A note diagnostic on the handful of str-typed `View` method names in attribute position is cheap and lives in the analyzer, where knowing std names is legitimate — recorded for S2.

## 5. The std groundwork — text nodes and type-carried slots

Two std changes, valuable independently of the grammar, ship first (S1). Both twins, kept in step.

**Text-node children.** The process twin's `View` gains a two-armed child (element | text); `render` emits escaped text nodes in order. The browser twin gains `create_text_node` in `std::dom` and appends it like any child. `.text()` keeps its existing replace-everything semantics on both twins — untouched, still the right call for the leaf-with-only-text case. This alone dissolves the `pt(…)`/`t(…)` span-fabrication idiom: mixed content becomes expressible in the chain (`.child("Take ").child(view("code").text("vilan upgrade")).child(".")`) before any new syntax exists.

**Slots.** Two traits make child position and attribute values type-directed:

```vilan
/// Something that can fill a child position.
trait Slot {
	fun place(self, parent: View);
}

impl View with Slot { … }         // append the element
impl str with Slot { … }          // append a text node
impl Signal<str> with Slot { … }  // append a text node, re-set on change (process: read once)
impl List<View> with Slot { … }   // append each

/// Something that can fill an attribute value.
trait AttrValue {
	fun apply(self, parent: View, name: str);
}

impl str with AttrValue { … }          // set once
impl Signal<str> with AttrValue { … }  // set and track (process: read once)
```

and the two existing methods widen over them — call-compatible, since every existing call site passes the types the old signatures took:

```vilan
fun child<C: Slot>(self, content: C): View
fun attr<V: AttrValue>(self, name: str, value: V): View
```

This is where the static/reactive distinction lives in the sugar: **the value's type carries it.** `src("hero.png")` sets once; `src(icon)` with `icon: Signal<str>` tracks — and both are honest, because the signal is right there in the source. `<p>{status}</p>` is reactive text for the same reason `bind_text(status)` is: `status` is a `Signal<str>`. No sigils, no compiler magic, no VDOM diffing — ordinary trait dispatch, resolved by the analyzer, visible in hover. `bind_text`/`bind_attr`/`bind_class` remain, unchanged, as the narrow-typed spellings; the traits are strictly additive surface.

The precedents for every mechanism here already ship: trait impls on primitives (`impl str with Display`, display.vl), shaped generic impls (`impl List<type T: Wire> with Wire`, wire.vl), generic bounded methods (`bind_each<T: PartialEq, K: PartialEq>`). ~~One verification item: a concrete-instantiation impl~~ — verified 2026-08-01 (probe, repo compiler): concrete-instantiation trait impls compile and dispatch directly in all three shapes needed here — `impl List<str> with X` on the std generic, the bounded-generic `impl List<type T: X> with X` (wire.vl's own shape), and a concrete instantiation of a local generic struct (`Wrap<str>`).

## 6. Interactions with what already shipped

- **Macros** expand before the desugar, so macro-*generated* markup re-parses and lowers normally. Markup inside a macro *body* fails name resolution on `view` (macro bodies are hermetic against `macro_std`) — the §4 note diagnostic names the reason.
- **The formatter** parses with element nodes intact (`parse_preserving_groups` is orthogonal) and needs a real `print_expr` arm plus split rules: head items space-separated inline until the line budget, then one per line; children indented one per line, a single string child allowed inline (`<h2>"Todos"</h2>`). **Self-closing tags print with a space before the slash — `<div />`, never `<div/>`** — a convention, not a parse rule: both forms parse, the formatter normalizes, and every example and book page follows it. The E13 catch-all (`_ => self.bailed = true`) means a missing arm silently disables `fmt` for any file using the syntax — the S3 gate is that `KNOWN_FORMATTER_BAILS` does not grow.
- **The LSP** consumes the analyzed program, and the desugared nodes carry markup spans, so hover, go-to-def, and typed diagnostics on chain-form head items, holes, and handler closures work from day one. Tag/attribute semantic-token classification is a tail slice (S5). The TextMate grammar and the book's highlight.js theme need markup rules in the same change — the three-places rule from `AGENTS.md` applies even with zero new keywords.
- **SSR** — the lowering is twin-agnostic method calls; S1's text-node change touches `render`, so the `test/ssr-render` byte goldens are re-blessed once, in S1, with the escaping pins of §10.
- **`node.rs::for_each_child`** gains the element variant's arms (hard compile gate, by design); `lift.rs` walks it like any container until the desugar retires it from the tree.
- **The context fence × trait dispatch** *(found by S1's suite run, adopted deliberately)* — a trait-dispatched call carries the union of its impls' context requirements, so the widened browser `child`/`attr` sit behind the `owner_scope` fence even when the instantiation is a static slot: `view("a").child(view("b"))` outside every boundary is now the documented compile error. That was the documented model already (`guide/ui.md`: build under a root); the fence now reaches these two methods. The process twin's arms read once and stay unfenced. Per-instantiation context precision — the analyzer already monomorphizes; the coverage pass does not yet follow — is recorded in H8 as a follow-up, not drifted into S1.
- **`vilan fmt`'s import organizer, HMR, the router, `const` styling** — no interaction; the sugar is gone before any of them look.

## 7. What v1 explicitly does not do

- **Bare text children.** `<h2>Todos</h2>` is a parse error suggesting `<h2>"Todos"</h2>`. §8 records why this is architecture, not taste.
- **Component tags.** *(settled 2026-08-01 — deferred entirely.)* `<todo_row … />` is not a form; calling the component function in a hole — `{todo_row(items, todo)}` — is the escape hatch. Vilan has no named arguments, so a component tag would be a function call wearing angle brackets, with attribute-looking props that positional parameters cannot honestly wear.
- **The `=` attribute spelling.** `name="value"` / `name={expr}` is not a form in v1 — §8 records the first draft's version and why paren form replaced it. The grammar space stays free; it could return later as a pure alternate spelling if familiarity demands it.
- **Fragments** (`<>…</>`). `List<View>` and `children(…)` already cover multiple roots.
- **Control-flow blocks** (`{#if}`/`{#each}` dialects). `.show`/`.when`/`.swap`/`.bind_each` in head position are the one way; a second reactive spelling would fork the model.
- **Spread attributes**, **auto-import of `view`**, and a **`class`/`styled` clobber lint** — each recorded here so declining them was a decision, not an omission.

## 8. Alternatives rejected

- **A macro DSL** (`macro html(…)`) — rejected on the grounds `ui-styling.md` §8 already litigated for styling: every consumer pays the DSL toll (no hover, no go-to-def, no typed diagnostics inside the block, macro-grade error spans, custom highlighting). It is also mechanically unavailable: macro arguments must parse as Vilan expressions before the macro sees them (`parse_argument_span`), and markup does not. Core grammar, lowered before analysis, gets the whole toolchain free.
- **Bare text children via a lexer markup mode.** The i-string scanner proves lexer-side content handling is possible — but an i-string announces itself with `i"`. Markup mode would have to begin at `<`, and the lexer cannot know `a < b` from `<div>` without parse state; lexing is context-free by spec (`lexical.md` §7) and by architecture (`tokenize()` completes before the parser exists, `frontend.md`). Quoted text also buys interpolation (`i"…"`), escaping rules already specified, and zero whitespace-significance questions (what would leading indentation inside `<p>` mean?). This is the one deliberate divergence from JSX, and it is load-bearing.
- **The `=`-and-hole attribute spelling** (`name="value"` / `name={expr}` — this proposal's own first draft) — replaced 2026-08-01 on review (Reed): two value grammars (`"…"` vs `{…}`) where parens already delimit; with it gone the head has no `=`, no holes, and no ISTRING wrinkle (an i-string value is just an expression in the parens). Paren form also made the construction/attachment split expressible (§2). Recorded here since it is the JSX-familiar spelling.
- **Undotted chain form resolved by name lookup** (`styled(card)` meaning `.styled(card)` when `styled` is a method, an attribute otherwise) — rejected: the desugar is pre-analysis and name-blind, a method list in the grammar would couple the parser to std, and — worse — adding a method to `View` would silently flip the meaning of any markup already using that name as an attribute. The dot keeps the rule decidable on one token forever.
- **Binding sigils** (`bind:value(…)`, `class:bind(…)` — the Svelte spelling). The type already says it (§5); a sigil would say it twice, and could *disagree* with the type. Events keep `on:` because there the name (`click`) is data, not a method.
- **Type-directed `on`/`on_event` unification.** Dispatching handler shape on the closure's type instead of literal arity would need the desugar to see types; it runs before the analyzer by design. Arity-of-literal is syntactic, covers the real corpus (every handler in the website and examples is a literal), and chain form remains for the rest.
- **Lowering single string children to `.text(…)`.** Saves nothing, and `.text()`'s replace-children semantics (process twin, `textContent` on the browser) would make `<p>"a" {x}</p>` and `<p>{x} "a"</p>` differ in kind, not just order. Uniform `.child` keeps the lowering one rule.
- **Capitalized-tag components resolved as functions** (the JSX rule) — Vilan components are snake_case functions; a casing convention would import a foreign idiom to solve a problem holes already solve.

## 9. Open calls — all settled 2026-08-01 (recommendations inline, taken)

1. ~~Widen `child`/`attr` vs. additive `slot`/`attr_slot` methods.~~ The traits are identical either way; the call was which method names carry them.
   *Widen* — `child` becomes `child<C: Slot>`, `attr` becomes `attr<V: AttrValue>`; every existing call site type-checks unchanged (`View`/`str` satisfy the bounds). The sugar lowers to the two names everyone already reads, chain code gains mixed content (`.child("take ")`) with no new vocabulary, and the "markup *is* the chain" story stays one sentence. Costs: the two hottest methods in std change public signature; emitted JS shape may change for every existing call — corpus goldens re-blessed; wrong-type diagnostics become trait-bound errors rather than "expected View".
   *Additive* — new `slot(content)` / `attr_slot(name, value)`; `child`/`attr` untouched, zero golden churn, `child` keeps its beginner-simple signature. Costs: two names for one idea forever, the sugar lowers to vocabulary chain authors never write, and if the widening happens later anyway the extra names become permanent residue.
   Settled: **widen** — and the gate probe already ran (2026-08-01, repo compiler, single-file builds, probe sources scratchpad-only). Findings: bounded-generic methods are **fully monomorphized** — one JS function per instantiation, trait calls emitted as **direct calls** to the concrete impl function, no dispatch tables, no adapter arguments; widening a str-taking method left the call site's shape byte-identical and changed only the emitted symbol name (`put` → `$a`), so the corpus churn is symbol renaming, nothing structural — re-blessed once in S1 as planned. The additive fallback is dead; `slot`/`attr_slot` will not ship.
2. ~~Trait names.~~ — settled: `Slot` / `AttrValue` as drafted.
3. ~~`class(…)` lowering.~~ — settled: `.attr("class", …)` like every other attribute (identical behavior since B37 made `class()` attribute-based) — no special-cased names in the lowering table, ever. (Dotted `.class(…)` remains available and identical.)
4. ~~The `on:` non-literal rule~~ — settled: literal arity dispatches (§4), non-literals mean `.on`, and a named one-parameter handler is chain form, `.on_event("evt", h)`.
5. ~~Feature name in docs.~~ — settled: *element syntax* for the feature, "an element expression" for one occurrence.

## 10. Slices (suite-gated, docs same commit, per-case pins)

- **S1 — std groundwork. SHIPPED 2026-08-01** (branch `element-syntax`) — `std::dom` gained `create_text_node` + the `Text` handle (with an `append_text` overload of `append`); the process twin's children are two-armed (`Child::Element | Child::Text`) and `render` emits escaped text nodes in order; both twins carry `Slot` (`View | str | Signal<str> | List<View>`) and `AttrValue` (`str | Signal<str>`) with `child`/`attr` widened over them. Pins: five runtime SSR cases + a browser `createTextNode` codegen pin + two trait-naming failure pins (inference.rs), three mixed-content stanzas in the `ssr-render` corpus program. The corpus churn was confined to `ssr-render` alone — the corpus' only `std::ui` user — and its old/new runtime output was proven byte-identical before the re-bless. The remaining verification item closed well: a wrong-typed slot reports `'i32' does not implement trait 'Slot', required by a generic bound of this call`, with a secondary span at the bound. `guide/ui.md` (new "Text children and mixed content" section) + `std/browser.md` + CHANGELOG in the same commit. One sharpened edge, found by the suite run and adopted deliberately: browser `child`/`attr` joined the binders behind the owner fence (§6 — the trait-dispatch union); the one pre-existing pin building boundary-less UI gained a `run_with_owner`, and the fence is pinned in its own right. *Original scope:* text-node children on both twins; `Slot`/`AttrValue`; widened `child`/`attr` (§9.1, settled); useful standalone — mixed content becomes expressible in the chain today.
- **S2 — grammar. SHIPPED 2026-08-01** (branch `element-syntax`) — `Node::Element` (`ElementBody`/`ElementHeadItem`), the `parse_atom` arm (one-token head disambiguation; `/>`, `</`, `on:` by span adjacency — the shift-operator mechanism; keyword and hyphenated names stored as SPANS and sliced at desugar, since the parser holds no source), `elements::rewrite_items` in the pre-lift slot at all five lift sites (macro-generated markup covered, pinned), the formatter printing elements from source verbatim (the i-string mechanism — and the fmt reality diverged from this plan's "ledger" idea: `KNOWN_FORMATTER_BAILS` exists only as prose, the real gate asserts an EMPTY bail set, so the passthrough arm ships in S2 and canonical layout is S3's job), and corpus program `element-syntax.vl`. The lowering is the chain at the strongest level: byte-identical emitted JS on both twins, pinned. Deviations, recorded: the tailored unresolved-`view` NOTE became a SPAN — the generated `view` accessor underlines `<tag`, so the error lands on what the user wrote; the note text needs per-source text the analyzer does not hold and rides S4 with the docs, as does the str-typed-method-name-as-attribute note; the close-tag mismatch diagnostic names the expected `</tag>` at the close site (parse errors carry no secondary span). Bycatch, fixed root-cause first in its own right: the S2 probe surfaced a GENERAL pre-existing solver bug — a bound-generic method call on an unannotated closure parameter froze abstract and monomorphized to the trait's empty member (silent misrender; no-impl types compiled clean). The method path now defers like the free path and the bound audit gained a never-silent sweep; six pins, three proven red first. *Original scope:* the `Node` element variant, `parse_atom` arm, span adjacency, keyword/hyphen names, the desugar pass, the two notes, corpus + fixtures + recovery.
- **S3 — formatter.** Print arm + split rules; the ` />` spacing convention; idempotence; the safety net (re-lex compare) holds; `KNOWN_FORMATTER_BAILS` does not grow.
- **S4 — docs & editors.** `docs/spec/grammar.md`, a book page teaching both forms side by side (`guide/ui.md` gains the sugar, keeps the chain as the ground truth), the tour, TextMate grammar, highlight.js theme. Every fenced example compile-gated as always.
- **S5 — tails.** LSP semantic tokens for tags/attributes; matching-tag edit niceties; rewriting one real website page (`art.vl`'s `diagram()` is the stress case) as the proof and the before/after exhibit.

## 11. Test plan (per case, as always)

- **Parser** — fixtures per form: each head-item spelling; chain form with generic args; keyword attribute names (`type`, `for`); hyphenated names with and without span adjacency; bare boolean attributes; `hidden.show(f)` (no-space attr-then-chain); an attribute named `on` (paren form, no colon); `/>` and `</` adjacency; both `<div/>` and `<div />` parse; nested elements; i-string attribute values; postfix chain off a closed element (`<div />.show(f)`); element in condition position (`no_struct` non-interaction); `f<T>(x)` unchanged. Errors: mismatched close (both spans), unclosed element, bare text child (with the quoted-string suggestion), undotted multi-argument item (with the attributes-take-one-value note), stray `<` in operand position, chain form with a missing paren. Recovery fixtures in `parser_recovery.rs`.
- **Lowering** — span-inclusive snapshots per table row of §4; written-order preservation for attrs and children; `on:` arity dispatch all three ways.
- **Inference** — pins for each `Slot`/`AttrValue` impl dispatching in holes and attribute values; a hole of an unimplemented type fails with the trait named; an attribute value of an unimplemented type (`styled(card)` undotted) fails with the trait named; handler context clauses (`turn_scope`) satisfied through the sugar; `.bind_each`/`.when`/`.swap` closure literals in head position satisfy `owner_scope`.
- **Corpus** — the todos example rewritten in element syntax alongside the chain original, byte-identical JS goldens proving the lowering is the chain; a mixed-content case; an SVG case (`<svg>` head → namespace rules).
- **SSR** — byte goldens for text-node order and escaping, mixed content, boolean attributes, void elements.
- **Formatter** — idempotence per form; `<div/>` normalized to `<div />`; the long-head split; the single-string-child inline rule; a file mixing chain and markup.
- **Docs gate** — every example in this proposal's final book page compiles.
