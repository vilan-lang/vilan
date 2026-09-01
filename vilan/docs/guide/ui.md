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
import std::reactive::Signal;

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
  `.bind_attr(name, source)`, `.style_var(name, source)`.

Every `bind_*` sets the property now and re-sets it whenever the source
changes. There is no render loop to trigger.

A read-only binding asks for a
[`Source<T>`](../std/reactive.md#source), not the concrete `Signal<T>` — a
signal is one, and so is anything else you implement `get`/`sub` on. Only
the bindings that write back (`bind_value`, `bind_draft`) need a real
signal.

## Text children and mixed content

`child` takes more than a `View`. Anything that can fill a child
position works — the value's type decides what lands in the DOM:

- a `View` appends as an element;
- a `str` appends as a **text node**;
- a `Signal<str>` appends as a text node kept in sync;
- a `List<View>` appends every view, in order.

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

`attr` is typed the same way: a `str` value sets once, a `Signal<str>`
re-sets whenever it changes — `attr("href", signal)` and
`bind_attr("href", signal)` are the same binding, chosen by type or by
name. (`text` is unchanged: it still replaces everything the element
contains, text nodes included, like the DOM's `textContent`.)

`attr` and `child` dispatch through traits rather than a bound, so their
reactive arms are `Signal<str>` specifically — a custom `Source` goes
through the named binding (`bind_attr`, `bind_text`) for now.

## Element syntax

The chain has a markup coat. An **element expression** is HTML-shaped
sugar that lowers, before analysis, to exactly the chain you would have
written — the same methods, in the same order, emitting the same code:

```vilan,browser
import std::reactive::Signal;
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
  `.bind_value(draft)`, `.show(flag)`, `.bind_each(rows, |r| r.id,
  |r| row(r))`. Every `View` method works in head position — the dot is
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
import std::reactive::Signal;
import std::ui::{ View, mount_root, view };

fun panel(items: Signal<List<str>>, flag: Signal<bool>): View {
	<section class("panel")>
		<input placeholder("What needs doing?") />
		<ul .bind_each(items, |t| t, |t| <li>{t}</li>) />
		<p .show(flag)>"empty"</p>
	</section>
}

fun main() {
	let _root = mount_root("app", || panel(Signal::new(["alpha"]), Signal::new(false)));
}
```

Components stay what they are — functions returning `View` — and are
called in holes: `{todo_row(items, todo)}`. Reactivity stays explicit:
an `if` or `match` inside a hole runs once at build, exactly as it does
in a chain; reactive structure is `.show`/`.when`/`.swap`/`.bind_each`
in head position, and `Signal` values in slots. The sugar adds no
semantics: `import std::ui::{ view, View }` is still required (the
compiler points the way if it is missing), and everything this guide
says about ownership, boundaries, and binding types applies unchanged.

## Components are just functions

A "component" is a function that returns a `View`. There is no
registration, special types, or props system; the parameters are the
props:

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::reactive::Signal;

fun labelled_input(label: str, value: Signal<str>): View {
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
`when`/`swap`/`bind_each` — outside any root, you'll get a compile
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

## Lists: `bind_each`

`bind_each(source, key, render)` renders one row per element of any
`Source<List<T>>` — a signal, a derived one, a mirror, a type of your own.
Rows are **keyed**, like React's `key` prop, and the key does real work
here:

- A row whose key survives a change is reused. Its element moves to
  the new position with its state and subscriptions intact.
- A row whose key survives but whose *value* changed re-renders only
  that row (that's why `T: PartialEq`).
- Removed rows are disposed: each row is its own owner, so a
  row's bindings die with the row.

```vilan,browser
import std::ui::{ view, View, mount_root };
import std::reactive::Signal;

[derive(PartialEq)]
struct Todo {
	id: i32,
	title: str,
}

fun main() {
	let todos: Signal<List<Todo>> = Signal::new([
		Todo { id = 1, title = "write docs" },
	]);
	let _root = mount_root("app", || {
		view("ul").bind_each(todos, |todo| todo.id, |todo| {
			view("li").text(todo.title)
		})
	});
}
```

```vilan,fragment
fun bind_each<T: PartialEq, K: PartialEq, S: Source<List<T>>>(
	self,
	source: S,
	key: sync |T| K,
	render: (sync |T| View) context owner_scope,
): View
```

## Conditionals: `show`, `when`, `swap`

Three primitives. Pick by what should happen to the content while it's
not visible:

| | Content while off | State | Use for |
|---|---|---|---|
| `.show(condition)` | mounted, hidden | preserved | tabs, collapsibles, anything that should keep its input text |
| `.when(condition, body)` | unmounted, disposed | dropped | content that shouldn't exist while off (an editor for a missing record) |
| `.swap(source, render)` | previous subtree disposed on change | per-value | pages on a route signal, any value-driven subtree |

```vilan,fragment
.show(open)                             // any Source<bool>
.when(present, || task_editor(…))       // any Source<bool> + (sync || View)
.swap(route, |current| match current {  // any Source<T> + (sync |T| View)
	Route::Home => home_page(),
	Route::NotFound => not_found(),
})
```

`when` and `swap` build their content under a fresh owner each time, so
everything inside cleans up when the content goes away. `swap` re-renders
only when the value *changes* (`T: PartialEq`), so navigating
to the page you're already on does nothing.

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
├── ◆ .swap(route, |page| …)           one owner PER PAGE shown
│     └─ home_page()
│           └─ .bind_text(…)           → registers with the PAGE
│
└── ◆ .bind_each(todos, key, |t| …)    one owner PER ROW
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

## Server-side rendering

The same component code runs on the server. On a Node build `std::ui`
builds an HTML string instead of live DOM, and `render(view)`
serializes it: first paint and SEO, before any JavaScript. A route
handler calls your own `app()` and splices the markup into its HTML
shell. The [server-side rendering guide](ssr.md) walks the whole loop.

```vilan
import std::ui::{ view, View, render };
import std::reactive::Signal;

fun greeting(name: Signal<str>): View {
	view("p").class("greeting").bind_text(name)
}

fun main() {
	let name = Signal::new("world");
	print(render(greeting(name)));
	// <p class="greeting">world</p>
}
```

Two rules make one component serve both legs:

- **Bindings read once.** `bind_text`, `bind_attr`, `bind_each`, `when`, and
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
  `xmlns`. But `show` drives the HTML-only `hidden` property, which SVG
  ignores: toggle an SVG subtree with `when` (or a class) instead.
- `bind_value` fights remote updates (every keystroke overwrites). For
  server-backed fields, use `bind_draft`.
- The `owner_scope` compile error means you built UI outside every
  boundary. Wrap the entry point in `mount_root`.
- Don't create owners per element or per component function. Boundaries
  belong where subtrees can *die*: roots, rows, conditionals. The
  framework already puts them there.
