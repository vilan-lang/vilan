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
fun request_animation_frame(callback: || void)             // requestAnimationFrame

struct DomRect { left: f64, top: f64, width: f64, height: f64 }   // a VALUE, not a handle
impl DomRect {
	fun right(self): f64                               // left + width
	fun bottom(self): f64                              // top + height
}

impl Element {
	fun set_text(self, text: str)                      // textContent =
	fun set_class(self, name: str)                     // className =
	fun set_attribute(self, name: str, value: str)
	fun remove_attribute(self, name: str)              // removeAttribute — a boolean attribute's off
	fun set_style_property(self, name: str, value: str) // style.setProperty (an empty value REMOVES)
	fun style_property(self, name: str): str           // style.getPropertyValue
	fun append(self, child: Element)
	fun append_text(self, child: Text)                 // appendChild, text-node overload
	fun remove(self)                                   // detach from the document
	fun clear(self)                                    // remove every child
	fun set_hidden(self, hidden: bool)
	fun bounding_rect(self): DomRect                   // getBoundingClientRect — forces layout
	fun offset_width(self): f64                        // offsetWidth — laid out, rounded
	fun offset_height(self): f64                       // offsetHeight
	fun is_connected(self): bool                       // attached to the document?
	fun contains(self, other: Element): bool           // other is this element or inside it
	fun query_selector_all(self, selector: str): List<Element>   // scoped to this subtree
	fun focus(self)                                    // move keyboard focus here
	fun matches(self, selector: str): bool             // element.matches(..)
	fun value(self): str                               // an input's current text
	fun set_value(self, value: str)
	fun on(self, event: str, handler: || void)
	fun on_event(self, event: str, handler: |Event| void)
	fun off_event(self, event: str, handler: |Event| void)   // listen's teardown
	fun listen(self, event: str, handler: |Event| void): Subscription   // must_use
	fun on_event_capture(self, event: str, handler: |Event| void, capture: bool)
	fun off_event_capture(self, event: str, handler: |Event| void, capture: bool)
	fun listen_capture(self, event: str, handler: |Event| void): Subscription   // must_use
	fun observe_resize(self, on_resize: || void): Subscription           // must_use
}

external struct ResizeObserver;      // the raw class behind observe_resize
impl ResizeObserver {
	fun new(callback: || void): ResizeObserver
	fun observe(self, target: Element)
	fun disconnect(self)
}

external struct Window;              // the window — a listen target, like Element
fun window(): Window
impl Window {
	fun on(self, event: str, handler: || void)
	fun on_event(self, event: str, handler: |Event| void)
	fun off_event(self, event: str, handler: |Event| void)
	fun listen(self, event: str, handler: |Event| void): Subscription   // must_use
	fun on_event_capture(self, event: str, handler: |Event| void, capture: bool)
	fun off_event_capture(self, event: str, handler: |Event| void, capture: bool)
	fun listen_capture(self, event: str, handler: |Event| void): Subscription   // must_use
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
	fun key(self): str           // "Enter", "Escape", "a", … — the CHARACTER
	fun code(self): str          // "KeyE", "Digit1", "Escape" — the PHYSICAL key
	fun target(self): Element    // the node the event was dispatched to
	fun current_target(self): Element  // the node whose listener is running
	fun target_value(self): str  // event.target.value — the input's text
	fun target(self): Element    // event.target — what contains() is asked about
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

**`key` is the character; `code` is the key.** A shortcut table wants `code`:
`"KeyE"` is the same physical key on QWERTY, AZERTY and Dvorak, where `key`
reads `"e"`, `"e"` and `"."` there — so a shortcut written against `key` moves
under the user's layout and one written against `code` does not. A text-entry
handler wants `key`, for the mirror-image reason. Both use the host's own value
spaces, so a binding table reads the same here as in the DOM docs it came from.

**`target` is where the event started; `current_target` is which handler is
speaking.** They coincide only when the event was dispatched straight at the
listening element. A click on the `<span>` inside a button has the span as
`target` and the button as `current_target` — so a row of buttons sharing one
handler reads `current_target` to learn which row it is in, and a
document-level dismiss listener reads `target` to ask "did this land inside
me?". `current_target` is only meaningful
DURING dispatch: the host clears it when the handler returns, so read it in the
handler rather than out of a captured event.

`target_value` is how a listener reads what the user typed **without holding
the element**. An element that reaches its own listener and back is a cycle
straddling the language/host boundary, where no disposal can see both halves,
so `bind_value`/`bind_draft` are built on this — and a hand-written input
handler should be too:

```vilan,fragment
view("input").on_event("input", |event| query.set(event.target_value()))
```

**Measurement is a value, and it forces layout.** `bounding_rect` returns a
`DomRect` — four numbers copied out of the host box, so it compares, parks in a
signal, and does not change under whoever is holding it. Reading it costs a
layout, so measure once and pass the value on. `offset_width`/`offset_height`
are the cheap, rounded pair for "how big is it" where the rect is the
fractional "where is it".

A **detached** element measures 0×0 whatever its styles say, which is why
`is_connected` exists: positioning that runs before the node is in the tree
positions against nothing, and reads as a layout bug rather than a timing one.

```vilan,fragment
if panel.is_connected() {
	let anchor = button.bounding_rect();
	panel.set_style_property("--x", i"{anchor.left}px");
	panel.set_style_property("--y", i"{anchor.bottom()}px");
}
```

**`listen_capture` is `listen` in the other phase.** Capture runs the
ancestors' listeners on the way *down* to the target, before the target's own;
bubble runs them on the way back up. Two things need the downward pass: events
that do not bubble at all (`scroll`, `focus`, `blur`) are heard from an
ancestor only in capture — which is how an anchored menu hears every scrolling
ancestor at once and stays on its element while the page moves under it — and a
listener that must run *before* the target's own (a dismiss guard, a modal's
key trap) has nowhere else to be. The phase is part of the identity the host
matches on, so a capture listener is not removable by the bubble-phase
`off_event`; that is why there is a second raw pair rather than a flag.

`observe_resize` is the wrapper over `ResizeObserver`, and the form to reach
for: it fires **once when observation starts** — so the first layout and every
later one run through the same callback — and its `Subscription` disconnects
the observer. The bare class is there for what the wrapper doesn't cover, and
leaks unless you disconnect it yourself.

```vilan,fragment
let sizing = get_owner().take(panel.observe_resize(|| reposition(panel)));
let scrolls = get_owner().take(window().listen_capture("scroll", |_| reposition(panel)));
let outside = get_owner().take(window().listen_capture("pointerdown", |event| {
	if !panel.contains(event.target()) {
		dismiss();
	}
}));
```

`remove_attribute` is `set_attribute`'s other half, and the half a **boolean**
attribute needs: `disabled`, `open`, `aria-hidden` and the rest are read by
their presence, so the false state is the attribute being gone, not
`="false"`.

```vilan,fragment
if is_open {
	panel.set_attribute("open", "");
} else {
	panel.remove_attribute("open");
}
```

## std::ui

```vilan,fragment
struct View { element: Element }
fun view(tag: str): View
fun mount(id: str, view: View)                                   // attach only
fun mount_root(id: str, body: (sync || View) context owner_scope): Owner

trait Slot { fun place(self, parent: View) }   // str | View | List<View>, and a Source of each
trait AttrValue { fun apply(self, parent: View, name: str) }   // str | SignalCell<str>
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
| `attr` | `(name: str, value: V): View`; `V: AttrValue` | `str` sets once, `SignalCell<str>` tracks |
| `style_var` | `(name: str, source: S): View`; `S: Source<str>` | reactive CSS custom property; registers with the enclosing boundary like every `bind_*` |
| `on` | `(event: str, handler: (\|\| void) context turn_scope): View` | handler runs in a fresh turn |
| `on_event` | `(event: str, handler: (\|Event\| void) context turn_scope): View` | same, with the DOM event |
| `child` | `(content: C): View`; `C: Slot` | the child contract: `str`, `View`, `List<View>`, and a `Source` of each — text re-set in place, an element or a run replaced |
| `children` | `(items: List<View>): View` | append several |
| `bind_text` | `(source: S): View`; `S: Source<str>` | reactive text |
| `bind_class` | `(source: S): View`; `S: Source<str>` | reactive class |
| `bind_styled` | `(source: S): View`; `S: Source<Style>` | reactive compiled style — `styled`'s reactive twin |
| `bind_attr` | `(name: str, source: S): View`; `S: Source<str>` | reactive attribute |
| `toggle_attr` | `(name: str, source: S): View`; `S: Source<bool>` | reactive BOOLEAN attribute — presence, not value (`inert`, `disabled`, `hidden`, `open`): present when true, removed when false |
| `bind_value` | `(signal: SignalCell<str>): View` | two-way input bind — **concrete `Signal`**: it writes back |
| `bind_draft` | `(draft: Draft<str>): View` | local-first input bind ([drafts](reactive.md#draft--local-first-cells)) |
| `bind_each` | `(source: S, key: sync \|T\| K, render: (sync \|T\| View) context owner_scope): View`; `T: PartialEq, K: PartialEq, S: Source<List<T>>` | keyed rows; each row is a disposal boundary |
| `bind_each_values` | `(source: S, render: (sync \|T\| View) context owner_scope): View`; `T: PartialEq, S: Source<List<T>>` | `bind_each` keyed by the item itself |
| `bind_each_by` | `(source: S, key: sync \|T\| K, render: (sync \|SignalCell<T>\| View) context owner_scope): View`; `K: PartialEq, S: Source<List<T>>` — **no bound on `T`** | keyed rows that UPDATE through the row's own cell instead of rebuilding |
| `when` | `(condition: S, body: (sync \|\| View) context owner_scope): View`; `S: Source<bool>` | state-DROPPING conditional |
| `swap` | `(source: S, render: (sync \|T\| View) context owner_scope): View`; `T: PartialEq, S: Source<T>` | dispose + rebuild per changed value |
| `swap_split` | same signature as `swap`; `T: PartialEq, S: Source<T>` | `swap` that holds the current page until the next route's chunk has loaded; identical to `swap` in a build with no chunk map |
| `show` | `(condition: S): View`; `S: Source<bool>` | state-PRESERVING visibility toggle — sets the `hidden` attribute AND an inline `display:none`, restoring the element's own inline `display` when it turns true |
| `on_mount` | `(action: sync \|Element\| void): View` | run `action` with this element once it is in the document |
| `autofocus` | `(): View` | focus this element once it is mounted AND rendered — the modal-input form HTML's `autofocus` cannot serve |

Semantics, choosing between `show`/`when`/`swap`, and examples: the
[UI guide](../guide/ui.md).

### A binding takes a `Source`, not a `Signal`

Every binding above that only READS its argument is generic over
[`Source<T>`](reactive.md#source), so a `Signal`, a derived signal, a
`RemoteSource` mirror or a type of your own all drive it:

```vilan,fragment
struct Stored<T> { inner: SignalCell<T> }

impl Stored<type T> with Source<T> {
	fun get(self): T { self.inner.get() }
	[must_use]
	fun on_change(self, observer: |T| void): Subscription { self.inner.on_change(observer) }
}
```

`Stored<str>` now feeds `bind_text`, `bind_class`, `bind_attr`,
`bind_styled`, `style_var`, `bind_each`, `bind_each_values`,
`bind_each_by`, `when`, `show`, `swap`, `swap_split` and `chunk_preload`
— on both the browser layer and the SSR twin.

Two things deliberately still ask for the concrete type:

- **`bind_value` and `bind_draft`**, because they WRITE BACK. `Source`
  declares `get` and `on_change` and no `set`, so there is nothing to widen to
  yet — the write side is its own design question.
- **`attr` and `child`**, whose reactive arms are the `AttrValue` and
  `Slot` traits — so `<div href(source)>` and `<p>{source}</p>` still want
  a `SignalCell<str>`. Widening a trait ARM is a blanket impl rather than a
  bound on a parameter, and that is a separate piece of machinery.

## std::router

```vilan,fragment
fun current_path(): SignalCell<str>       // location.pathname, live (navigate + back/forward)
fun navigate(path: str)               // pushState + update current_path
fun location_url(): str               // pathname + search + hash — the whole relative URL
fun segments(path: str): List<str>    // "/w/3/task/7" → ["w", "3", "task", "7"], RAW
fun percent_decode(text: str): str    // decodeURIComponent, total (a bad escape decodes to itself)
fun parse_query(query: str): Map<str, str>   // "a=1&flag" → { "a": "1", "flag": "" }

struct PathParts { segments: List<str>, query: Map<str, str>, fragment: Option<str> }
fun parse_path(path: str): PathParts  // cut, then decode — the READ direction

trait Routable { fun to_path(self): str }              // route → URL
trait FromPath { fun from_segments(parts: List<str>): Self }   // URL → route
fun from_path<R: FromPath>(path: str): R               // parse_path, then from_segments
fun link<R: Routable>(label: str, route: R): View   // a real <a>; intercepts plain left-clicks

impl View {
	fun link_to<R: Routable>(self, route: R): View  // `link`'s body on an anchor you built
}

// Route chunks (a `split = true` leg) — both are ordinary signals
fun pending(): SignalCell<bool>                 // a route chunk is in flight
fun chunk_error(): SignalCell<Option<str>>      // the last fetch failed, with the reason
```

`current_path()` is a singleton signal: every caller gets the same one, and
the `popstate` listener is wired on first use. `link` renders a real anchor
(middle-click, ctrl-click, and copy-link keep native behavior) and intercepts
only a plain left click, calling `prevent_default` + `navigate`. It also sets
`draggable="false"`, which is the one non-native thing about it — see the
[gotchas](../appendix/gotchas.md) note. `View::link_to(route)` is that same
body without the `<a>` and the label, for an app that builds and styles its
own anchor. Route modelling (`Routable`/`FromPath` over enums): the
[routing guide](../guide/routing.md).

**`parse_path` is the read direction, and it cuts before it decodes.** The
fragment is delimited first (a `?` after a `#` is fragment text), then the
query, then the path splits on `/` — and only then is every piece
percent-decoded, so an escaped separator (`%2F`, `%26`) stays inside the
segment or value it belongs to instead of becoming one. Empty segments do not
exist, so `"/w/acme"`, `"/w/acme/"` and `"//w//acme"` parse alike and no match
arm has to name the trailing-slash case. A query key with no `=` is present
with an empty value (`"?open"` and `"?open="` both give `{ "open": "" }`), a
repeated key keeps the last, and query keys and values additionally read `+`
as a space — a path segment's `+` is a literal plus. `fragment` is `None` when
the URL has no `#` at all and `Some("")` for a bare trailing one, because those
are different URLs.

`segments` stays **raw** — it is the splitter, not the reader — so
`segments("/a%20b")` is `["a%20b"]` where `parse_path("/a%20b").segments` is
`["a b"]`. Route matching wants the decoded form.

`FromPath` is `Routable`'s inverse, and the pair has a law:
`from_path(route.to_path())` is `route`, for every route the app can reach —
one round-trip test per route space instead of a drift nobody notices until a
link 404s. Its member takes segments rather than a path, because that is where
a route space is decided; `from_path` is a free generic function rather than a
trait default because an associated function has no receiver to inherit a
default through (the compiler says exactly that if you try). It is total: an
unrecognized URL is a route, not an `Option`.

`current_path()` is the **pathname only**, deliberately — a signal that
advanced on every `#anchor` would re-render the page for a scroll, and a route
whose `to_path()` prints a pathname would stop comparing equal to the location.
Reach for `location_url()` where the query matters (on load, and in a
`popstate` handler) and keep routing on the path.

`pending()` and `chunk_error()` describe a `split = true` leg's route-chunk
fetches, and are ordinary signals — bind them with `show`, `bind_text` or a
class. A failed fetch means the navigation simply did not happen and nothing
is remembered as in flight, so clicking the link again retries; there is no
retry API because a link is one. Worked example:
[the dev loop](../guide/dev-loop.md#shipping-routes-separately).

## std::storage

`localStorage` / `sessionStorage`, string-keyed strings, in **two forms**: six
free functions for the site that touches one key, and a `Storage` handle for
the site whose subject is the store.

```vilan,fragment
// The one-key form. A missing key reads as "".
fun get(key: str): str
fun set(key: str, value: str)
fun remove(key: str)
fun session_get(key: str): str
fun session_set(key: str, value: str)
fun session_remove(key: str)

// The handle.
impl Window {
	fun local_storage(self): Storage     // window.localStorage
	fun session_storage(self): Storage   // window.sessionStorage
}
external struct Storage;
impl Storage {
	fun len(self): i32
	fun key_at(self, index: i32): Option<str>   // None past the end
	fun get(self, key: str): Option<str>        // None when ABSENT
	fun has(self, key: str): bool
	fun set(self, key: str, value: str)
	fun remove(self, key: str)
	fun clear(self)                             // every key on the origin
}
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

**The free functions are not deprecated by the handle.** They are shorter where
only one key is in play, and the flattening `""` means nothing has to be
unwrapped. The handle is what a store-shaped job needs — and its `get` is the
one that can tell an ABSENT key from a key whose stored value is legitimately
`""`, which `!get(key).is_empty()` cannot. That difference is not academic: an
"initialize this key if it is unset" pass rewrites a legitimately-empty value on
every run when presence is spelled as non-emptiness.

Counting **down** is the shape an enumerating sweep wants, because removing a
key renumbers everything above it — and the host's key order is unspecified
anyway, so a pass that both reads and removes must not assume it is stable.

The two reader verbs live on `Window`, so a module that calls them imports
**both** `std::dom`'s `window` and `std::storage` — the `impl Window` block is
declared in `std::storage`, and an impl is in scope only where its module is.

```vilan,browser
import std::dom::window;
import std::io::print;
import std::option::Option::{ self, None, Some };
import std::storage;

fun main() {
	let store = window().local_storage();
	mut index = store.len();
	for index > 0 {
		match store.key_at(index - 1) {
			Some(let key) => {
				if !key.starts_with("app.") {
					store.remove(key);
				}
			}
			None => print("the store shrank under the walk"),
		}
		index -= 1;
	}
	if !store.has("app.theme") {
		store.set("app.theme", "");
	}
}
```

**Browser-only, with no process twin, deliberately.** An SSR leg has no
`window` and no per-user store, so a twin could only be a stub that touches
`window` (a crash on the server) or an in-memory map answering reads with
values the browser never wrote — and the second turns a layer mistake into a
silent divergence between the two renders of one component. Importing
`std::storage` from a process module is a cross-platform error at analysis
instead. Server-side persistence is [`std::db`](process.md); state that must
reach the browser rides the render or an rpc call.
