# Browser modules reference

The browser layer of std: `std::dom`, `std::ui`, `std::router`,
`std::storage`. Available only for browser builds. Concepts:
[Building UI](../guide/ui.md), [Routing](../guide/routing.md).

## std::dom

Opaque handles over real DOM objects.

```vilan,fragment
external struct Element;
fun get_element_by_id(id: str): Element
fun create_element(tag: str): Element
fun create_element_ns(namespace: str, tag: str): Element   // createElementNS
fun create_text_node(content: str): Text                   // a fresh text node
fun query_selector(selector: str): Element
fun query_selector_all(selector: str): List<Element>

impl Element {
	fun set_text(self, text: str)                      // textContent =
	fun set_class(self, name: str)                     // className =
	fun set_attribute(self, name: str, value: str)
	fun set_style_property(self, name: str, value: str) // style.setProperty (CSS custom props)
	fun append(self, child: Element)
	fun append_text(self, child: Text)                 // appendChild, text-node overload
	fun remove(self)                                   // detach from the document
	fun clear(self)                                    // remove every child
	fun set_hidden(self, hidden: bool)
	fun value(self): str                               // an input's current text
	fun set_value(self, value: str)
	fun on(self, event: str, handler: || void)
	fun on_event(self, event: str, handler: |Event| void)
	fun off_event(self, event: str, handler: |Event| void)   // listen's teardown
	fun listen(self, event: str, handler: |Event| void): Subscription   // must_use
}

external struct Window;              // the window — a listen target, like Element
fun window(): Window
impl Window {
	fun on(self, event: str, handler: || void)
	fun on_event(self, event: str, handler: |Event| void)
	fun off_event(self, event: str, handler: |Event| void)
	fun listen(self, event: str, handler: |Event| void): Subscription   // must_use
}

external struct Text;                // a text node — text only, no attributes
impl Text {
	fun set_text(self, text: str)                      // textContent =
}

external struct Event;
impl Event {
	fun prevent_default(self)
	fun button(self): i32        // 0 = main button
	fun meta_key(self): bool
	fun ctrl_key(self): bool
	fun shift_key(self): bool
	fun alt_key(self): bool
	fun key(self): str           // "Enter", "Escape", "a", …
	fun target_value(self): str  // event.target.value — the input's text
	fun pointer_x(self): f64     // clientX — where the pointer is, in the viewport
	fun pointer_y(self): f64     // clientY
}
```

Raw `element.on` handlers do **not** establish a turn: that's `View.on`'s
job — and the `Window` verbs are raw in the same way, so a window handler that
writes signals should wrap its body in `turn(FlushPolicy::AtSuspension, …)`
itself. Prefer the `View` layer; drop to `dom` for what it doesn't cover.

**`window` is a listen target.** Events that aren't delivered to any element —
`resize`, `popstate`, `storage`, `message`, and the pointer stream a drag needs
once the pointer has left the element it started on — hang off `window()`,
which carries exactly the verbs `Element` does.

**`on`/`on_event` are fire-and-forget; `listen` is the removable form.** An
element listener dies with its element, which is usually the whole answer.
Nothing about the window ever dies, so a window listener you can't remove is a
leak by construction. `listen` hands back a
[`Subscription`](reactive.md#subscription-disposable) — the same handle
`Source::sub` returns, `[must_use]` for the same reason — and disposing it
unhooks the listener. Later events then deliver nothing. Ownership is yours,
as `sub`'s is: `get_owner().take(window().listen(…))` ties it to a scope, or
call `dispose()` by hand. Disposing twice is safe.

```vilan,fragment
let moves = window().listen("pointermove", |event| {
	track(event.pointer_x(), event.pointer_y());
});
// … later, or when the enclosing scope ends:
moves.dispose();
```

`off_event` is what `listen` is built on, and removal is **identity-matched**:
the handler you pass must be the same value the host was handed, so a freshly
written closure removes nothing. `listen` exists so you don't have to hold that
pairing right.

`target_value` is how a listener reads what the user typed **without holding
the element**. An element that reaches its own listener and back is a cycle
straddling the language/host boundary, where no disposal can see both halves,
so `bind_value`/`bind_draft` are built on this — and a hand-written input
handler should be too:

```vilan,fragment
view("input").on_event("input", |event| query.set(event.target_value()))
```

## std::ui

```vilan,fragment
struct View { element: Element }
fun view(tag: str): View
fun mount(id: str, view: View)                                   // attach only
fun mount_root(id: str, body: (sync || View) context owner_scope): Owner

trait Slot { fun place(self, parent: View) }          // View | str | Signal<str> | List<View>
trait AttrValue { fun apply(self, parent: View, name: str) }   // str | Signal<str>
```

`mount_root` = fresh owner + turn boundary + attach; it returns the root
owner (most apps let it live forever). `mount` is the attach half alone.
Use it only when you already hold a boundary.

Both **panic naming the id** when nothing on the page carries it —
`mount: no element with id 'app'`. The lookup they share hands back the
host's `null` typed as an `Element`, so the alternative was a
`Cannot read properties of null` from somewhere inside the attach, with
the one thing you got wrong appearing nowhere in the message. On the
server side that mismatch is caught before it can happen: the id is what
[`check_shell`](process.md#stddocument) holds the document against.

`view` knows the SVG vocabulary: an SVG tag name (`svg`, `path`, `rect`,
`clipPath`, …; exact case) creates its element in the SVG namespace, so
inline icons and diagrams render; on the server the `svg` root serializes
with its `xmlns`. Tags that exist in both vocabularies (`a`, `title`,
`style`, `script`) resolve to HTML. `class`/`styled` set the `class`
attribute (not the `className` property), so styling works on SVG nodes
too.

### View methods

| Method | Signature (self elided) | Notes |
|---|---|---|
| `text` | `(content: str): View` | static text |
| `class` | `(name: str): View` | static class |
| `styled` | `(style: Style): View` | classes from a compiled style |
| `attr` | `(name: str, value: V): View`; `V: AttrValue` | `str` sets once, `Signal<str>` tracks |
| `style_var` | `(name: str, source: S): View`; `S: Source<str>` | reactive CSS custom property; registers with the enclosing boundary like every `bind_*` |
| `on` | `(event: str, handler: (\|\| void) context turn_scope): View` | handler runs in a fresh turn |
| `on_event` | `(event: str, handler: (\|Event\| void) context turn_scope): View` | same, with the DOM event |
| `child` | `(content: C): View`; `C: Slot` | element, text node (`str`/`Signal<str>`), or `List<View>` |
| `children` | `(items: List<View>): View` | append several |
| `bind_text` | `(source: S): View`; `S: Source<str>` | reactive text |
| `bind_class` | `(source: S): View`; `S: Source<str>` | reactive class |
| `bind_styled` | `(source: S): View`; `S: Source<Style>` | reactive compiled style — `styled`'s reactive twin |
| `bind_attr` | `(name: str, source: S): View`; `S: Source<str>` | reactive attribute |
| `bind_value` | `(signal: Signal<str>): View` | two-way input bind — **concrete `Signal`**: it writes back |
| `bind_draft` | `(draft: Draft<str>): View` | local-first input bind ([drafts](reactive.md#draft--local-first-cells)) |
| `bind_each` | `(source: S, key: sync \|T\| K, render: (sync \|T\| View) context owner_scope): View`; `T: PartialEq, K: PartialEq, S: Source<List<T>>` | keyed rows; each row is a disposal boundary |
| `when` | `(condition: S, body: (sync \|\| View) context owner_scope): View`; `S: Source<bool>` | state-DROPPING conditional |
| `swap` | `(source: S, render: (sync \|T\| View) context owner_scope): View`; `T: PartialEq, S: Source<T>` | dispose + rebuild per changed value |
| `swap_split` | same signature as `swap`; `T: PartialEq, S: Source<T>` | `swap` that holds the current page until the next route's chunk has loaded; identical to `swap` in a build with no chunk map |
| `show` | `(condition: S): View`; `S: Source<bool>` | state-PRESERVING visibility toggle |

Semantics, choosing between `show`/`when`/`swap`, and examples: the
[UI guide](../guide/ui.md).

### A binding takes a `Source`, not a `Signal`

Every binding above that only READS its argument is generic over
[`Source<T>`](reactive.md#source), so a `Signal`, a derived signal, a
`RemoteSource` mirror or a type of your own all drive it:

```vilan,fragment
struct Stored<T> { inner: Signal<T> }

impl Stored<type T> with Source<T> {
	fun get(self): T { self.inner.get() }
	[must_use]
	fun sub(self, observer: |T| void): Subscription { self.inner.sub(observer) }
}
```

`Stored<str>` now feeds `bind_text`, `bind_class`, `bind_attr`,
`bind_styled`, `style_var`, `bind_each`, `when`, `show`, `swap`,
`swap_split` and `chunk_preload` — on both the browser layer and the SSR
twin.

Two things deliberately still ask for the concrete type:

- **`bind_value` and `bind_draft`**, because they WRITE BACK. `Source`
  declares `get` and `sub` and no `set`, so there is nothing to widen to
  yet — the write side is its own design question.
- **`attr` and `child`**, whose reactive arms are the `AttrValue` and
  `Slot` traits — so `<div href(source)>` and `<p>{source}</p>` still want
  a `Signal<str>`. Widening a trait ARM is a blanket impl rather than a
  bound on a parameter, and that is a separate piece of machinery.

## std::router

```vilan,fragment
fun current_path(): Signal<str>       // location.pathname, live (navigate + back/forward)
fun navigate(path: str)               // pushState + update current_path
fun segments(path: str): List<str>    // "/w/3/task/7" → ["w", "3", "task", "7"]

trait Routable { fun to_path(self): str }
fun link<R: Routable>(label: str, route: R): View   // a real <a>; intercepts plain left-clicks

// Route chunks (a `split = true` leg) — both are ordinary signals
fun pending(): Signal<bool>                 // a route chunk is in flight
fun chunk_error(): Signal<Option<str>>      // the last fetch failed, with the reason
```

`current_path()` is a singleton signal: every caller gets the same one, and
the `popstate` listener is wired on first use. `link` renders a real anchor
(middle-click, ctrl-click, and copy-link keep native behavior) and intercepts
only a plain left click, calling `prevent_default` + `navigate`. Route
modelling (`parse`/`href` over enums): the [routing guide](../guide/routing.md).

`pending()` and `chunk_error()` describe a `split = true` leg's route-chunk
fetches, and are ordinary signals — bind them with `show`, `bind_text` or a
class. A failed fetch means the navigation simply did not happen and nothing
is remembered as in flight, so clicking the link again retries; there is no
retry API because a link is one. Worked example:
[the dev loop](../guide/dev-loop.md#shipping-routes-separately).

## std::storage

`localStorage` / `sessionStorage`, string-keyed strings. A missing key reads
as `""`.

```vilan,fragment
fun get(key: str): str
fun set(key: str, value: str)
fun remove(key: str)
fun session_get(key: str): str
fun session_set(key: str, value: str)
fun session_remove(key: str)
```

```vilan,browser
import std::storage;

fun main() {
	storage::set("token", "abc");
	let token = storage::get("token");
	if token != "" {
		storage::remove("token");
	}
}
```
