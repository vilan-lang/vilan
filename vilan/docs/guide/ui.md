# Building UI

`std::ui` is a declarative view layer with no virtual DOM. A `View`
describes a DOM element. Methods chain to build it. Where React re-runs
components and diffs the result, Vilan binds individual DOM properties to
signals: when a signal changes, exactly that text node or attribute
updates and nothing else runs.

Available in browser builds (`target = "browser"` in `vilan.toml`, or
`vilan build --target browser`).

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::reactive::{ Signal, SignalCell };

fun main() {
	let count = Signal::new(0);
	let _root = mount_root("app", || {
		view("div")
			.child(view("p").bind_text(count.map(|n: i32| i"clicked {n} times")))
			.child(view("button").text("+1").on("click", || count.set_with(|n| n + 1)))
	});
}
```

Read that top to bottom: make a `div`, give it a paragraph whose text
follows the counter, give it a button that bumps the counter. That's the
whole mental model.

## Views

`view(tag)` makes a fresh element. Methods chain, and each returns the
view so you can keep going:

- **Static content**: `.text(content)`, `.class(name)`,
  `.attr(name, value)`, `.styled(style)` (see [Styling](styling.md)).
- **Structure**: `.child(content)`, `.children(views)`.
- **Events**: `.on(event, handler)`, or `.on_event(event, |e| …)` when
  you need the DOM event itself (`prevent_default`, `key()`, modifiers,
  `pointer_x()`/`pointer_y()`). For window-level events, and for a listener
  you need to remove, drop to `std::dom` — [Escaping to the DOM](#escaping-to-the-dom).
- **Reactive bindings**: `.bind_text(source)`, `.bind_class(source)`,
  `.bind_attr(name, source)`, `.toggle_attr(name, flag)`,
  `.style_var(name, source)`.

Every `bind_*` sets the property now and re-sets it whenever the source
changes. There is no render loop to trigger.

A read-only binding asks for a
[`Source<T>`](../std/reactive.md#source), not the concrete `SignalCell<T>` — a
signal is one, and so is anything else you implement `get`/`sub` on. Only
the bindings that write back (`bind_value`, `bind_draft`) need a real
signal.

## Text children and mixed content

`child` takes more than a `View`. Anything that can fill a child
position works — the value's type decides what lands in the DOM. Three
static arms, and a reactive twin for each:

- a `View` appends as an element; a `Source<View>` appends the view it
  holds and **replaces** it whenever the source changes;
- a `str` appends as a **text node**; a `Source<str>` appends a text
  node kept in sync;
- a `List<View>` appends every view, in order; a `Source<List<View>>`
  appends the run and replaces the whole run on every change. `<>…</>`
  is the literal for one (see [Fragments](#fragments)).

That pairing is the whole contract: whatever may be a child statically
may be a child reactively, and `{expr}` in element syntax means the same
thing either way. The reactive arms register one subscription with the
nearest boundary, so a `{signal}` child inside an `each` row stops
replacing anything when the row is disposed — but the views themselves
arrive already built, so each one's own bindings belong to the scope
that *constructed* it. Reach for `swap(source, |value| …)` when every
subtree must be built and disposed per value; reach for a `Source<View>`
child when the views are values the app already holds.

**A reactive child keeps its place.** The replacement lands where the
`{expr}` is written, not at the end of the parent — and so do `when`'s
body, `swap`'s subtree and `each`'s rows. Each plants an empty text
node where it is called and inserts before it, so
`<nav>{brand}{when(..)}{footer}</nav>` puts the conditional between the
two, and it is still between them after it toggles off and on. No
wrapper element, and nothing to remember about ordering.

A `Source<List<View>>` is not a reconciler: it replaces the run rather
than moving surviving rows. `each` is the keyed form, and it is
what a list of *data* wants.

Text nodes make mixed content direct: prose around an inline element is
a run of siblings, not a pile of wrapper spans.

```vilan,browser
import std::ui::{ view, View, mount_root };

fun tip(): View {
	view("p")
		.child("Update any time with ")
		.child(view("code").text("vilan upgrade"))
		.child(".")
}

fun main() {
	let _root = mount_root("app", || tip());
}
```

`attr` is typed the same way: a `str` value sets once, a `SignalCell<str>`
re-sets whenever it changes — `attr("href", signal)` and
`bind_attr("href", signal)` are the same binding, chosen by type or by
name. (`text` is unchanged: it still replaces everything the element
contains, text nodes included, like the DOM's `textContent`.)

An attribute that comes and goes is a third case, and both forms take it
directly: over an `Option<str>` or a `Source<Option<str>>`, a `Some(text)`
sets the attribute and a `None` leaves it off — removing it, where the value
is reactive and the attribute was there a moment ago. `bind_attr` spells it by
name and element syntax by type, so `<div data-dragging(drag_status)>` is the
same binding as the chain below. Reach for it whenever a selector reads presence —
`[data-dragging="row"] *` matches an element that *has* the attribute, so
writing `""` between drags leaves the rule firing. `Some("")` is still a
present, empty attribute, which is why the absence has to be its own value
rather than a sentinel string.

```vilan,fragment
// `None` while nothing is being dragged; `Some("row")`/`Some("col")` while
// something is.
view("div").bind_attr("data-dragging", drag_status.map(|status| match status {
	DragStatus::Still => None,
	DragStatus::Vertical => Some("row"),
	DragStatus::Horizontal => Some("col"),
}))
```

A BOOLEAN attribute is a different thing and has its own binding:
`inert`, `disabled`, `hidden` and `open` mean *present*, so there is no
string that turns one off — `attr("disabled", "false")` is a disabled
control. `.toggle_attr(name, flag)` takes a `Source<bool>` and writes
the attribute when it is true, removes it when it is false:
`shell.toggle_attr("inert", modal_open)`.

`attr` and `child` dispatch through traits rather than a bound, and
their reactive arms are blanket impls over `Source`, so a derived
signal, a `RemoteSource` or a mirror of your own fills either position
exactly as a cell does.

## Element syntax

The chain has a markup coat. An **element expression** is HTML-shaped
sugar that lowers, before analysis, to exactly the chain you would have
written — the same methods, in the same order, emitting the same code:

```vilan,browser
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, mount_root, view };

fun counter(): View {
	let count = Signal::new(0);
	<div>
		<h2>"Counter"</h2>
		<button on:click(|| count.set_with(|n| n + 1))>"+1"</button>
		<p>{count.map(|n: i32| i"clicked {n} times")}</p>
	</div>
}

fun main() {
	let _root = mount_root("app", || counter());
}
```

One rule governs the head — everything between `<tag` and `>`:

- An **undotted** `name(value)` is an attribute: `.attr("name", value)`,
  the value's type deciding static vs tracked as always. A bare name
  (`disabled`) is a boolean attribute. Keyword and hyphenated names
  (`type`, `data-state`, `aria-label`) are ordinary attribute names —
  hyphens are ordinary attribute-name characters, exactly as in HTML, so
  every `data-*`/`aria-*` attribute is written in the same undotted form
  and emitted verbatim.
- A **leading dot** is the chain, verbatim: `.styled(card)`,
  `.bind_value(draft)`, `.show(flag)`, `.child(each(rows, |r| r.id,
  |r| row(r)))`. Every `View` method works in head position — the dot is
  what keeps attributes and methods from ever colliding, so a new
  method can never change what existing markup means.
- `on:click(handler)` is an event. A zero-parameter closure literal
  lowers to `.on`, a one-parameter literal to `.on_event`; a named
  one-parameter handler is written in chain form
  (`.on_event("click", handler)`).

Children — everything between `>` and `</tag>` — are nested elements,
**quoted** strings (`i"…"` interpolation included), and `{expression}`
holes; each lowers to `.child(…)` in written order. Text children are
quoted because Vilan's lexer is context-free and stays that way — and
the payoff is that interpolation, escapes, and expressions work in
markup exactly as they do everywhere else. Bare text is a parse error
that suggests the quoted form.

An element is an ordinary expression: it nests in holes, sits in match
arms, and takes postfix chains. The two forms mix freely —

```vilan,browser
import std::reactive::{ Signal, SignalCell };
import std::ui::{ View, each, mount_root, view };

fun panel(items: SignalCell<List<str>>, flag: SignalCell<bool>): View {
	<section class("panel")>
		<input placeholder("What needs doing?") />
		<ul>{each(items, |t| t, |t| <li>{t}</li>)}</ul>
		<p .show(flag)>"empty"</p>
	</section>
}

fun main() {
	let _root = mount_root("app", || panel(Signal::new(["alpha"]), Signal::new(false)));
}
```

### Fragments

A **fragment** groups several children under one hole, with no wrapper
element. `<>…</>` is the nameless head, and it lowers to a **list
literal** of its children — so its type is `List<View>`, the arm `child`
already places:

```vilan,browser
import std::ui::{ View, mount_root, view };

fun labelled(name: str, value: str): List<View> {
	<>
		<dt>{name}</dt>
		<dd>{value}</dd>
	</>
}

fun main() {
	let _root = mount_root("app", || {
		<dl>
			{labelled("host", "localhost")}
			{labelled("port", "8080")}
		</dl>
	});
}
```

`<>` and `</>` are written tight, like `/>` and `</`. A fragment takes
no attributes and no chain links — there is no element to put them on —
and it has no self-closing form; the empty fragment is `<></>`.

Its type is where its uses are, and where its limits are. A fragment is
a `List<View>`, so it fills a child position and every position a list
fills, and a `Source<List<View>>` of fragments keeps its place like any
other reactive child. It is **not** a `View`: a `fun …: View` return, a
`when` body, a `swap` render and an `each` row all want one view,
and a fragment there is a type error that says so. It also does not
flatten — a fragment written directly inside another is a list inside a
list, which the literal refuses; nest through a child position instead.

Components stay what they are — functions returning `View` — and are
called in holes: `{todo_row(items, todo)}`. Reactivity stays explicit:
an `if` or `match` inside a hole runs once at build, exactly as it does
in a chain; reactive structure is `.show` in head position and the
`when`/`swap`/`each` values in holes, with `Signal` values in slots. The sugar adds no
semantics: an element means `std::ui::view` whatever the file has
imported, so element syntax needs no `view` import of its own (a `View`
you write as a TYPE still needs one, and the editor offers it), and
everything this guide says about ownership, boundaries, and binding
types applies unchanged.

## Components are just functions

A "component" is a function that returns a `View`. There is no
registration, special types, or props system; the parameters are the
props:

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::reactive::{ Signal, SignalCell };

fun labelled_input(label: str, value: SignalCell<str>): View {
	view("label")
		.text(label)
		.child(view("input").bind_value(value))
}

fun main() {
	let name = Signal::new("");
	let _root = mount_root("app", || labelled_input("Name", name));
}
```

`mount_root(id, body)` builds the body and attaches it to the page
element with that id. It also establishes the root **owner**, which is
why you never think about cleanup: every binding you create, at any
depth of function calls, registers with the nearest owner automatically
(the [reactive guide](reactive.md) explains owners).

If you create a reactive binding — a `bind_*`, a `Signal` in a slot, a
`when`/`swap`/`each` — outside any root, you'll get a compile
error mentioning `owner_scope`. It means "wrap this in `mount_root`"
(or `run_with_owner` in a test). Purely static structure needs no
boundary: `mount("app", view("div").child(view("p").text("hi")))` is
fine, because nothing in it subscribes. That holds through your own
generic helpers too — a `fun card<T: Slot>(content: T): View` called
with static content needs no boundary, while the same helper called
with a `Signal` keeps the requirement: the compiler follows each call's
actual instantiation.

## Events run in turns

Each event dispatch runs your handler inside a fresh **turn**: all the
signal writes one click causes are batched, and watchers see the final
state once. Handlers die with their DOM node, so there is nothing to
unsubscribe.

```vilan,fragment
.on("click", || count.set_with(|n| n + 1))
.on_event("keydown", |pressed| {
	if pressed.key() == "Enter" { submit(); }
})
```

## Inputs

Two ways to wire an `<input>`, for two different situations:

**`bind_value(signal)`** is the simple two-way bind: the input shows the
signal, typing writes it back. Use it for local state: a search box, a
"new item" field.

**`bind_draft(draft)`** binds the input to a local-first
[draft](reactive.md#optimistic-writes-and-local-first-drafts) whose
commit is typically an rpc. Typing updates the input instantly and
commits in the background. A remote update folds in without re-sending.
An echo of your own edit never moves the caret. Use it for fields that
edit *server* state as you type:

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::reactive::{ draft, Draft, DraftState };
import std::option::Option::{ self, Some, None };

fun main() {
	let name = draft("initial", |value: str| {
		let _would_send = value; // an rpc call in a real app
		None
	});
	let _root = mount_root("app", || {
		view("div")
			.child(view("input").bind_draft(name))
			.child(view("span").bind_text(name.state.map(|state: DraftState| match state {
				DraftState::Synced => "",
				DraftState::Dirty => "saving…",
				DraftState::Failed(let reason) => i"failed: {reason}",
			})))
	});
}
```

## Lists: `each`

`each(source, key, render)` renders one row per element of any
`Source<List<T>>` — a signal, a derived one, a mirror, a type of your own.
It is a **value**: write it in a child hole, or hand it to `child` to place
the run at the parent's current end. Rows are **keyed**, like React's `key`
prop, and the key does real work here:

- A row whose key survives a change is reused. Its element moves to
  the new position with its state and subscriptions intact.
- A row whose key survives but whose *value* changed re-renders only
  that row (that's why `T: PartialEq`).
- Removed rows are disposed: each row is its own owner, so a
  row's bindings die with the row.

```vilan,browser
import std::ui::{ each, view, View, mount_root };
import std::reactive::{ Signal, SignalCell };

[derive(PartialEq)]
struct Todo {
	id: i32,
	title: str,
}

fun main() {
	let todos: SignalCell<List<Todo>> = Signal::new([
		Todo { id = 1, title = "write docs" },
	]);
	let _root = mount_root("app", || {
		view("ul").child(each(todos, |todo| todo.id, |todo| {
			view("li").text(todo.title)
		}))
	});
}
```

```vilan,fragment
fun each<T: PartialEq, K: PartialEq, S: Source<List<T>>, C: Slot>(
	source: S,
	key: sync |T| K,
	render: (sync |T| C) context owner_scope,
): Each<T, K, S, C>
```

### Three forms, one engine

`each` asks two things of your items — a key **and** an equality —
and most lists only have one of them to give. The other two forms each
drop one bound:

| | Signature | Key | Unchanged row | Asks of `T` |
|---|---|---|---|---|
| `each(source, key, render)` | `render: \|T\| C` | `key(item)` | reused; changed → rebuilt | `PartialEq` |
| `each_values(source, render)` | `render: \|T\| C` | the item itself | reused; changed → rebuilt | `PartialEq` |
| `each_by(source, key, render)` | `render: \|SignalCell<T>\| C` | `key(item)` | **always** reused; the row's cell is rewritten | nothing |

- **`each_values`** is `each(source, |x| x, render)` written
  once. Reach for it whenever the item *is* the identity — every
  `|x| x` key in the wild is this.
- **`each_by`** is Solid's `<Index>` beside `each`'s `<For>`.
  A row whose key survives keeps its element, its owner and its
  bindings, and std writes the new item into that row's own
  `SignalCell<T>`, so the row updates *through* the bindings `render`
  already made. That's what lets it take a `T` with no equality at all —
  a struct carrying a closure, a handle, anything you can't derive
  `PartialEq` for.

  The trade: `set` never compares, so **every** kept row's cell is
  written on every change of the list, and every binding in every row
  re-runs. Reach for `each` when `T` compares cheaply and rows are
  expensive to rebuild; reach for `each_by` when `T` can't compare,
  or when the row's own bindings are the natural update path.

```vilan,browser
import std::ui::{ each_by, each_values, view, View, mount_root };
import std::reactive::{ Signal, SignalCell };

struct Task {
	id: i32,
	title: str,
}

fun main() {
	let tasks: SignalCell<List<Task>> = Signal::new([
		Task { id = 1, title = "write docs" },
	]);
	let names: SignalCell<List<str>> = Signal::new(["ada", "grace"]);
	let _root = mount_root("app", || {
		view("div")
			// the item is the key
			.child(view("ul").child(each_values(names, |name| view("li").text(name))))
			// `Task` needs no PartialEq: the row updates through its cell
			.child(view("ol").child(each_by(tasks, |task: Task| task.id, |task: SignalCell<Task>| {
				view("li").bind_text(task.map(|current| current.title))
			})))
	});
}
```

## After the element lands: `on_mount`, `autofocus`

`view(..)` builds an element; it is not in the document until whatever
appends it does. `.on_mount(action)` runs `action` with the element once
it *is* — at every attachment site, including a `when` body or an
`each` row that appears in a later change.

```vilan,fragment
view("input").attr("type", "text").on_mount(|element| element.focus())
view("input").attr("type", "text").autofocus()          // and then some
```

`autofocus` is the reason the hook exists. HTML's `autofocus` attribute
fires only on a document's initial parse, so it does nothing for a modal
mounted later — and the workaround it forces (mint a uuid, set it as the
id, start a 1 ms timer, look the element back up) is three lines of
ceremony around a value you already had. There is nothing to look up:
the callback is handed the element.

`on_mount` promises the element is IN THE DOCUMENT, and that is all it
promises. A microtask runs before the frame's rendering step, so at that
moment the element is connected but not yet *rendered*: no layout, no
resolved style, and no `ResizeObserver` reaction to either. `focus()` has
a precondition — connected, rendered, visible, not inert, all of them at
the call — and the platform's answer to a target that is not is to do
nothing, silently. An overlay panel held at `visibility: hidden` until a
`ResizeObserver` places it and flips it visible is exactly the shape that
refuses.

So `autofocus` is not `on_mount(|e| e.focus())`: it attempts in the
microtask, and if the element did not take focus it attempts again on the
next animation frame and once more on the frame after, then stops. Three
attempts on the platform's own clock — no timer, no millisecond to tune.
Written out, with `std::dom::request_animation_frame` as the clock and
`matches(":focus")` as the read-back (`focus()` returns nothing):

```vilan,fragment
view("input").on_mount(|element| {
	element.focus();
	if !element.matches(":focus") {
		request_animation_frame(|| element.focus());
	}
})
```

Two things no retry fixes. iOS Safari ignores a programmatic `focus()`
outside a user gesture whatever frame it runs on; and the sound shape for
an overlay is that focus is a consequence of the SHOW — a hook the driver
runs when it flips visibility, or `<dialog>.showModal()`, whose focusing
steps run once the dialog is rendered.

The hook is a **microtask**, which is enough because the whole
synchronous build — and the `mount` that finishes it — runs to
completion before any microtask does. On the SSR twin both methods
accept and drop, like the event binders: there is no document to be in.

`autofocus` also writes the `autofocus` **attribute**. The attribute is
inert for an element inserted after the page parsed — that is *why* this
method exists — so it costs nothing at runtime and makes the choice
readable: to a focus scope, to devtools, and to a test that asserts
markup. The SSR twin deliberately does not write it, because a *served*
`autofocus` is honored by the browser's own initial parse.

## Focus scopes

An overlay usually wants more than one focused input: it wants Tab to
stay inside it while it is open, and it wants focus back where it was
when it closes. That is a **focus scope**.

```vilan,fragment
view("div")
	.focus_scope(FocusContainment::Wrap)
	.child(view("input").autofocus())
	.child(view("button").text("Close"))
```

`Wrap` is a menu: Tab cycles inside the subtree, and focus that leaves by
any other route may leave. `Contain` is a modal: it also pulls focus back
when it lands anywhere else. Neither writes anything outside the panel —
the rest of the page stays clickable and stays in the accessibility tree,
which is exactly what a menu needs and what `inert` would take away.
`inert` is still yours for a true modal (`set_attribute("inert", "")` and
`remove_attribute`), and it is a bigger hammer: it removes the subtree
from the accessibility tree and blocks pointer events.

`focus_scope` on a view is the ordinary case. A driver that owns its own
visibility flip — which is every overlay system that positions a panel
before showing it — installs the scope and takes the focus as **two
acts**, because focus has a precondition the mount cannot satisfy:

```vilan,fragment
// at mount, or wherever the panel element is in hand:
let scope = focus_scope(panel, FocusContainment::Contain);

// …in the pass that flips it visible:
let _took = scope.focus_initial();
```

`focus_initial` focuses the first `[autofocus]` descendant, else the
first tabbable one, else the panel itself at `tabindex="-1"` — and it
answers whether the focus was taken, so a show that fired too early is
simply asked again on the next pass. It is idempotent: once focus has
been taken, a later call leaves alone whatever the user has since moved
to.

Scopes NEST as a stack, not by DOM ancestry, because an overlay is a
portal: a submenu opened from inside a menu mounts beside its parent's
panel rather than inside it. The topmost `Contain` scope is the one that
guards. A scope lives as long as the boundary it was installed in — when
that boundary is disposed the scope pops, and focus goes back to whatever
held it when the scope opened, unless the app moved focus deliberately in
the meantime or the remembered element has since left the document.

`Element::tabbable()` is the query underneath, and it is public: a router
that moves focus to the new page's heading, or a menu that implements
arrow-key navigation, wants it too.

## Conditionals: `show`, `when`, `swap`

Three primitives. Pick by what should happen to the content while it's
not visible:

| | Content while off | State | Use for |
|---|---|---|---|
| `.show(condition)` | mounted, hidden | preserved | tabs, collapsibles, anything that should keep its input text |

`show` makes two writes: the `hidden` attribute, which selectors and
assistive technology read, and an inline `display: none`, which is what
actually hides it. The attribute alone would not — the preflight's
`[hidden]{display:none}` sits in `@layer vilan.preflight` and a compiled
`Style`'s rules are unlayered, so any `display` you set beats the reset
outright. Showing again puts back the element's own inline `display`,
captured before the first toggle. If you write this element's inline
`display` yourself after binding `show`, the next toggle takes it: style
through a `Style` and the two never meet.
| `when(condition, body)` | unmounted, disposed | dropped | content that shouldn't exist while off (an editor for a missing record) |
| `when_some(source, render)` | unmounted, disposed | dropped | the same, when the content NEEDS the value — an editor for the selected record |
| `swap(source, render)` | previous subtree disposed on change | per-value | pages on a route signal, any value-driven subtree |

`show` is a `View` method — it binds a property of the element it is written
on. `when`, `when_some` and `swap` are **values**: each fills a child
position, so it lands exactly where it is written.

```vilan,fragment
.show(open)                            // any Source<bool>
{when(present, || task_editor(…))}     // any Source<bool> + (sync || C: Slot)
{when_some(selected, |record|          // any Source<Option<T>>
	task_editor(record))}              //   + (sync |SignalCell<T>| C: Slot)
{swap(route, |current| match current { // any Source<T> + (sync |T| C: Slot)
	Route::Home => home_page(),
	Route::NotFound => not_found(),
})}
```

All three build their content under a fresh owner each time, so everything
inside cleans up when the content goes away. `swap` re-renders only when the
value *changes* (`T: PartialEq`), so navigating to the page you're already on
does nothing.

`when_some` is the one that binds the value, and it is the reason to reach for
it over `when(source.map(|value| value is Some(_)), ..)`: the body gets the
payload, and it gets it as a `SignalCell<T>` rather than as a `T`. What decides
structure is the PRESENCE — a `None` → `Some` builds, a `Some` → `None`
disposes — so a changed payload writes to the cell and the row stands, with
whatever bound to it updating in place. That is `each_by`'s row cell applied to
a single row, and it is why `T` needs no `PartialEq`: nothing is compared.
Read the cell inside a binding, exactly as an `each_by` row does:

```vilan,fragment
{when_some(selected, |account| <p>{account.map(|current| current.name)}</p>)}
```

### Position, and placing one at the end

A value fills a child position, so the conditional or the run sits exactly
among the siblings it is written between. `std::ui` exports six —
`when`, `when_some`, `swap`, `each`, `each_values`, `each_by` — and each
returns something that fills a child slot:

```vilan,fragment
<ul>
	{header_row()}
	{each_values(items, |item: str| <li>{item}</li>)}
	{when(more, || <li>"and more"</li>)}
	{footer_row()}
</ul>
```

To place one at the parent's current *end* — where the retired `View` methods
put it — hand it to `child`: `view("ul").child(each(rows, key, render))`. That
is the mechanical rewrite for old code, since `parent.swap(s, r)` was never
anything but `parent.child(swap(s, r))`. The run is called `each` rather than
`bind_each` because `bind_` means "one property kept in sync" everywhere else,
and a value that *is* a child has no property to bind.

A helper that returns one names its type, since vilan has no trait objects —
the last argument is the shape the closure yields (below):

```vilan,fragment
fun account_menu(signed_in: SignalCell<bool>): Conditional<SignalCell<bool>, View> {
	when(signed_in, || <nav>"Account"</nav>)
}
```

### A row, a body or a branch can be anything `Slot`

The render closures are not limited to `View`. A row may be a fragment, a bare
string, or another value form, and the run owns whatever it placed:

```vilan,fragment
<ul>
	{each_values(items, |item: str| <><li>{item}</li><li class("sep")/></>)}
</ul>
```

That is what makes a wrapper element unnecessary in the last place one was
still needed — a row that is several nodes. It costs one empty text marker per
row: a row's content can grow after it was placed (a `when` inside a row
toggles later), so the reconciler moves and removes a row by the SPAN between
its marker and the next one rather than by a list of nodes it remembered.

Ownership is the same either way: the body, the subtree and every row run
under a fresh owner established where the value is *placed*, and that owner is
disposed with the instantiation.

## The ownership picture

Here is the whole cleanup model in one picture. Owners exist at the
places marked `◆`: the boundaries where a subtree can die. Every
binding registers with the *nearest* boundary above it, no matter how
many plain function calls sit in between:

```text
◆ mount_root("app", …)                the root owner — lives forever
│
├── view("header")                     static: no boundary of its own
│     └─ .bind_text(title)             → registers with the ROOT
│
├── ◆ {swap(route, |page| …)}         one owner PER PAGE shown
│     └─ home_page()
│           └─ .bind_text(…)           → registers with the PAGE
│
└── ◆ {each(todos, key, |t| …)}        one owner PER ROW
      ├─ row(id = 1)
      │     └─ .bind_class(…)          → registers with ROW 1
      └─ row(id = 2)
            └─ .on("click", …)         → dies with ROW 2's DOM node
```

Navigate away, and the page's owner is disposed. Every binding the page
created dies with it. Delete row 2, and only row 2's bindings die. This
is why there is no unsubscribe code anywhere in a Vilan app: the tree of
boundaries *is* the cleanup logic, and the framework already placed
them where subtrees end.

**A boundary also removes what it placed.** Disposing it takes the
nodes out of the document: `when`'s body, `swap`'s subtree,
`each`'s rows, a `{signal}` child's view or run or text node, and
the invisible marker each of them keeps its position with. That is
usually invisible — the subtree was leaving with its parent anyway —
and it is the whole story for a **portal**, a container that outlives
the boundary filling it: an overlay, a tooltip layer, a modal host
mounted once at the top of the page. Fill one from a component's
boundary, dispose the component, and the container is empty; there is
nothing to remember to clean up by hand. The three **static** child
arms are untouched, deliberately: a `str`, a `View` or a `List<View>`
child belongs to the parent element it was appended to, not to a
boundary.

## Server-side rendering

The same component code runs on the server. On a Node build `std::ui`
builds an HTML string instead of live DOM, and `render(view)`
serializes it: first paint and SEO, before any JavaScript. A route
handler calls your own `app()` and splices the markup into its HTML
shell. The [server-side rendering guide](ssr.md) walks the whole loop.

```vilan
import std::ui::{ view, View, render };
import std::reactive::{ Signal, SignalCell };

fun greeting(name: SignalCell<str>): View {
	view("p").class("greeting").bind_text(name)
}

fun main() {
	let name = Signal::new("world");
	print(render(greeting(name)));
	// <p class="greeting">world</p>
}
```

Two rules make one component serve both legs:

- **Bindings read once.** `bind_text`, `bind_attr`, `each`, `when`, and
  `swap` embed the source's value *at render time*: no subscription is created,
  and nothing survives the request (create, serialize, discard). Build pure, bind
  reactive: a component that leans on effect side-channels at build time renders
  stale. Text and attribute values are escaped, so a hostile string is inert.
- **No `mount`/`mount_root` on the server.** Mounting is a client entry, not a
  renderable view, so the natural factoring is a shared `fun app(): View` with a
  per-leg `main`: `mount_root("app", app)` in the browser, `render(app())` on the
  server. Event handlers (`on`) are accepted and discarded; a server-rendered
  `<button>` is a plain button. `std::dom` stays browser-only, so a component
  reaching for raw DOM cannot SSR; the cross-platform error says so at the import.

## Escaping to the DOM

`View` is a thin wrapper over `std::dom::Element` (it's right there as
`view.element`). For anything the chain doesn't cover, use `std::dom`
directly: `get_element_by_id`, `query_selector`,
`element.set_attribute`, and so on. See the
[browser reference](../std/browser.md).

Two things live only down there, because they aren't about a view at all.
Events that no element receives — `resize`, `popstate`, `storage`, `message` —
hang off `window()`, which carries the same `on` / `on_event` verbs an element
does. And when a listener has to *stop* before its target does, `listen` is the
removable form: it hands back a `Subscription`, and disposing it unhooks the
listener.

A pointer drag needs both, which is why it can't be a `View` method: the
pointer leaves the element the moment the drag starts moving, so the stream has
to come from the window, and it has to stop on `pointerup`.

```vilan,fragment
element.on_event("pointerdown", |down| {
	let start = down.pointer_x();
	mut stop;
	let moves = window().listen("pointermove", |event| {
		width.set(start_width + event.pointer_x() - start);
	});
	let ups = window().listen("pointerup", |_| { stop(); });
	stop = || { moves.dispose(); ups.dispose(); };
})
```

Window handlers are raw, like `element.on`: they establish no turn, so wrap the
body in `turn(FlushPolicy::AtSuspension, …)` when a handler writes several
signals that should settle as one wave.

## Traps

- `show` keeps bindings live while hidden, and they keep firing. If the
  hidden content is expensive, use `when`.
- Inline SVG works: `view("svg").attr("viewBox", …).child(view("path")…)`
  creates real SVG-namespace elements, and the server render carries the
  `xmlns`. `show` works on an SVG subtree too — it writes the inline
  `display`, which SVG honours, not only the HTML-only `hidden`
  attribute that SVG ignores.
- `bind_value` fights remote updates (every keystroke overwrites). For
  server-backed fields, use `bind_draft`.
- The `owner_scope` compile error means you built UI outside every
  boundary. Wrap the entry point in `mount_root`.
- Don't create owners per element or per component function. Boundaries
  belong where subtrees can *die*: roots, rows, conditionals. The
  framework already puts them there.
