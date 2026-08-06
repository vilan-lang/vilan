# Canvas — a typed 2D drawing surface over `std::canvas` (backlog A17)

> Status: DEFERRED 2026-08-04 (owner call) — pending the bindgen probe on
> the global TypeScript declarations (lib.dom.d.ts). The owner's framing:
> if bindgen can chew the globals, EVERY host API comes for free and
> hand-maintaining canvas (and each next missing API) is the wrong path;
> canvas was only ever the tester-demand instance of that general problem.
> Revisit with the probe's result: autogen works → this proposal dissolves
> into generated bindings; autogen fails → this design stands ready.

Ground truth for this proposal was gathered from the shipped browser layer
(`std/src/browser/dom.vl`, `ui.vl`, `router.vl`, `dev.vl`), the external-struct
precedent (`std/src/fetch.vl`, `std/src/time.vl`), `std/src/reactive.vl`, the
platform-model layering rules (`proposal/platform-model.md`, `std/vilan.toml`),
the element-syntax grammar (`proposal/element-syntax.md`), the numeric-types
spec (`proposal/numeric-types.md`), and the actual browser-feature test
precedent (`crates/vilan-cli/tests/init.rs`, `tests/router.rs`). Every claim
about what exists today was checked in source, not assumed; two claims in
backlog A17's own wording turn out not to match what's shipped — flagged up
front in §1 and §6 rather than buried. Nothing here is implemented.

## 0. The problem and the thesis

`std::browser` today is `dom`/`ui`/`router`/`storage`/`dev` (verified: `ls
std/src/browser/`) — zero canvas anywhere under `vilan/std/src/` (verified:
zero `canvas`/`Canvas` hits outside this backlog entry itself). The ask (A17):
a canvas module reachable from the `view` chain, a typed 2D context, and a
resize/DPR story — with the central design fork named explicitly in the
entry: immediate-mode calls over the host API (the fetch/ws precedent) with a
`Signal`-driven redraw idiom, versus a retained display list. WebGL is
out of scope for v1 (the entry's words, unchanged here).

**Thesis: fork (a), immediate-mode.** `std::canvas` is a thin external-struct
wrapper over `CanvasRenderingContext2D` — one vilan method per host method,
`f64` throughout, no new abstraction — acquired off a view-built `<canvas>`
element and driven by ordinary `Signal.effect` closures that clear and
repaint. §2 makes the case in full; §5 shows that this choice buys
swap/disposal correctness *for free* through the existing `Owner`/
`Subscription` machinery, which is the sharpest argument for it beyond
"it's less work."

## 1. Module shape — and a naming correction

**The module is `std::canvas`, not `std::browser::canvas`.** The backlog
entry's own phrasing says `std::browser::canvas`, but that doesn't match how
the browser layer resolves today. `std/vilan.toml` declares
`[library.layer.browser] platform = ["browser"] root = "src/browser"` — a
**directory-form layer**: every file under `src/browser/` joins the layer
automatically, and the layer name (`browser`) never appears in the import
path. `ui.vl` imports `pkg::dom::Element`, not `pkg::browser::dom::Element`;
`router.vl` imports `pkg::ui::view`; `docs/std/browser.md` states it in
prose: "The browser layer of std: `std::dom`, `std::ui`, `std::router`,
`std::storage`." A `canvas.vl` dropped into `std/src/browser/` joins the same
layer the same way, with no per-file platform attribute needed — which is
also a second correction: the entry's "whether the context is a
`[platform(Browser)]` external struct" names an attribute that doesn't
exist. Platform coloring here is **file residence**, not a declaration on
the struct. §6 returns to this for the SSR/process-twin question.

So: **file `std/src/browser/canvas.vl`, imported as `std::canvas`** (matching
`std::dom`, `std::ui`, `std::router`, `std::storage`, `std::dev`).

### Exports

- `Context2D` — an `external struct` wrapping `CanvasRenderingContext2D`
  (§3 has the full method surface).
- `TextMetrics` — an `external struct` wrapping the `measureText()` return
  value, `.width(self): f64` in v1.
- `Element` gains two new inherent methods (`get_context`/`context_2d`,
  below), added from `canvas.vl` (a **cross-file** `impl Element { .. }` —
  `Element` is declared in `dom.vl` today with its one impl block there;
  this would be the first time a second file extends it). Verified this is
  legal: method resolution
  (`analyzer.rs`'s `method_member_impl_subject`) matches purely by subject
  type against a flat, file-agnostic `Vec<Implementation>` — no
  same-module requirement anywhere in the resolution path, and `fetch.vl`
  already proves multiple inherent-impl blocks for one external struct
  merge (`Response` has two, both in that file). Flagged as a **first**
  for this specific cross-file shape, worth the reviewer's eyes even
  though it compiles.
- `fun canvas(width: i32, height: i32): View` — a convenience builder,
  `view("canvas")` plus the two attrs stringified. Not load-bearing;
  `view("canvas").attr("width", "600").attr("height", "400")` or the
  markup form does the identical thing. Included because every other
  browser-facing std module ships the ergonomic wrapper next to the raw
  primitive (`fetch()` next to `fetch_with()`, `get`/`post` next to
  `Request.send`) and canvas's two required attributes are exactly the
  kind of boilerplate that precedent papers over:
  ```vilan
  fun canvas(width: i32, height: i32): View {
  	view("canvas").attr("width", i"{width}").attr("height", i"{height}")
  }
  ```

### Acquisition: `Element.context_2d()`, not a `View` method

The charter asks how user code gets from a view-built `<canvas>` to its
`CanvasRenderingContext2D`. Two candidates, both viable, one recommended:

- **`Element.context_2d(self): Context2D`** (recommended) — reached as
  `view.element.context_2d()`, exactly the spelling `guide/ui.md`'s
  "Escaping to the DOM" section already teaches for anything the chain
  doesn't cover (`view.element.set_attribute(..)` is the pattern's
  existing example). This is the honest fit: every `View` method today
  either returns `self` (the chain links) or, for `bind_each`/`when`/
  `swap`, still returns `View` — **zero** existing `View` method returns
  anything else. A `View.context_2d(): Context2D` would be the first
  chain method that breaks the chain, and `Context2D` doesn't have a
  `View`-shaped return to keep going with anyway. `Element` already
  carries this class of capability (`.value(): str`, `.set_value(..)`),
  so extending it is the smaller, more consistent move.
- **`View.context_2d(self): Context2D`** (considered, not recommended) —
  saves one `.element` hop at the cost of being the first non-chaining
  `View` method and blurring the "View is a thin wrapper, escape to
  `Element` for anything else" line the docs already draw.

```vilan
impl Element {
	/// The raw host call (`element.getContext(kind)`) — configurable, but
	/// only "2d" is typed honestly by `context_2d` below; a caller reaching
	/// for "webgl" here gets a `Context2D` handle whose methods will throw
	/// at runtime, the same class of tradeoff `.value()` already accepts
	/// on a non-input `Element`.
	[extern(method, "getContext")]
	external fun get_context(self, kind: str): Context2D;

	/// The element's 2D drawing context. Only meaningful on a `<canvas>`
	/// element built via `view("canvas")` / `canvas(w, h)` — `Element` is
	/// tag-erased like every `std::dom` handle, so nothing in the type
	/// system stops calling this on a `<div>`; it fails at runtime the way
	/// `.value()` on a non-input already does.
	fun context_2d(self): Context2D {
		self.get_context("2d")
	}
}
```

This mirrors `fetch.vl`'s own shape exactly: a raw, configurable extern
(`fetch_with`) sitting beside an ergonomic wrapper that supplies the
common-case default (`fetch`).

## 2. The central fork: immediate-mode + Signal-redraw, vs. a retained display list

### (a) Immediate-mode context + Signal-driven redraw — recommended

`Context2D` is a typed external struct, one vilan method per host method
(§3), called directly by user code inside a `Signal.effect` closure that
clears and repaints:

```vilan
import std::ui::{ view, View };
import std::canvas::{ canvas, Context2D };
import std::reactive::Signal;

fun sun_dial(): View {
	let angle = Signal::new(0.0);
	let surface = canvas(400, 400);
	let ctx = surface.element.context_2d();

	angle.effect(|radians| {
		ctx.clear_rect(0.0, 0.0, 400.0, 400.0);
		ctx.save();
		ctx.translate(200.0, 200.0);
		ctx.rotate(radians);
		ctx.set_fill_style("#e08a1e");
		ctx.begin_path();
		ctx.arc(0.0, 0.0, 120.0, 0.0, (360.0).to_radians());
		ctx.fill();
		ctx.restore();
	});

	surface
}
```

`angle.effect(..)` is `Source<T>`'s existing trait default
(`reactive.vl:358`) — `get_owner().take(self.sub(observer))`, the *exact*
mechanism `bind_text`/`bind_class`/`style_var` already use. No new reactive
primitive, no new lifecycle: an effect over a canvas repaint is structurally
identical to an effect over a DOM property write, just landing on
`fillRect`/`arc` instead of `set_text`.

**Why this over the alternative:**

- **Zero abstraction cost.** `arc(x, y, r, start, end)` compiles to
  `ctx.arc(x, y, r, start, end)` — no diffing, no intermediate
  representation, no runtime that has to reconcile what changed.
- **The reactive model already fits.** Vilan's whole story is
  fine-grained: a `Signal` change re-runs exactly the effect that reads
  it, nothing more, nothing traversed. A canvas repaint driven by
  `Signal.effect` is that story applied to pixels instead of DOM nodes —
  consistent, not a special case.
- **Free disposal correctness** (expanded in §5) — because the redraw is
  an ordinary `Subscription` under the ambient `Owner`, it already stops
  when the enclosing `swap`/`when`/`bind_each` boundary disposes. No new
  teardown code to write or forget.
- **Matches the fetch/ws precedent the entry itself names**: thin, typed,
  honest, no invented vocabulary between the user and the host API.

### (b) A retained display list — rejected for v1, recorded

The alternative: `Context2D` (or a `Canvas` value) holds a scene
description — shapes, styles, z-order — as data; drawing is "set the scene,"
and a diff pass decides what actually needs to be redrawn to the real
context. This is more "reactive-native" in the sense that it lets the
framework skip redundant work the way `bind_each`'s keyed reconciliation
skips redundant DOM writes.

Rejected for v1, for reasons worth recording rather than re-litigating
later:

- **It invents a scene graph vilan doesn't have anywhere else.** Every
  other reactive surface in std binds to a *host* representation (a DOM
  node, an attribute) that already has diffing built in for free (the
  browser's own DOM). Canvas has no such host-side incremental target —
  `CanvasRenderingContext2D` is genuinely immediate-mode at the API level
  — so a retained model here means vilan owns the diff algorithm, the
  scene data model, and its own bugs, none of which exist today.
- **No demonstrated need.** Canvas users (charts, small games, drawing
  surfaces) redraw cheaply in the common case — clearing and repainting a
  few hundred primitives per frame is not the DOM-diffing cost profile a
  retained model exists to avoid. The entry itself leans (a); nothing
  found in the codebase argues the other way.
- **It would fork the API in two**, the same mistake `element-syntax.md`
  §8 already litigated for markup: a scene-description DSL that some
  draw calls go through and some don't (immediate escape hatches would
  still be needed for anything the scene model doesn't cover) means two
  mental models for one canvas.
- **It's strictly addable later without breaking (a).** If a real
  consumer needs retained-mode performance (thousands of primitives,
  heavy per-frame diffing cost), it can be layered as a *separate*
  higher-level module built on top of `Context2D` — v1's immediate-mode
  surface is exactly the primitive such a module would need underneath
  it. Shipping (a) first forecloses nothing.

## 3. The typed 2D context surface, v1

**Numeric type: `f64` throughout the drawing surface.** Per
`proposal/numeric-types.md` §1, every sized numeric primitive lowers to a
plain JS number on the JS backend — so the *codegen* cost is identical
whether the vilan type is `i32` or `f64`. The choice is about what the API
actually needs: canvas coordinates are continuous (subpixel positioning
after a DPR scale, fractional radii, arbitrary transforms), and angles are
radians (irreducibly fractional). `f64` already carries the trig and
rounding surface canvas work wants (`number.vl`: `sin`/`cos`/`atan2`/
`to_radians`/`sqrt`), so `(360.0).to_radians()` reads naturally in a call
site. The one exception: the **canvas element's own pixel dimensions**
(`width`/`height` on `<canvas>`, the DOM's `unsigned long` IDL
attributes) stay `i32` on the `canvas(width, height)` helper — a pixel
count is a count, not a continuous quantity, and `i32` is what an author
types without a decimal point.

**Naming**: vilan has no default parameters and no overloading (verified
against the spec and against precedent throughout std — `on`/`on_event`,
`sleep`/`sleep_for`, `after`/`after_for`, `set_body`/`set_body_bytes` all
give a *distinct name* to each parameter shape rather than overloading one
name). Every host method with an optional argument or multiple call shapes
gets a distinct vilan name below, following that convention exactly.

```vilan
impl Context2D {
	// --- Paths --------------------------------------------------------
	[extern(method, "beginPath")]
	external fun begin_path(self): void;

	[extern(method, "moveTo")]
	external fun move_to(self, x: f64, y: f64): void;

	[extern(method, "lineTo")]
	external fun line_to(self, x: f64, y: f64): void;

	/// Clockwise arc (the host's default when its optional 6th argument
	/// is omitted).
	[extern(method, "arc")]
	external fun arc(self, x: f64, y: f64, radius: f64, start_angle: f64, end_angle: f64): void;

	[extern(method, "bezierCurveTo")]
	external fun bezier_curve_to(self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64): void;

	[extern(method, "closePath")]
	external fun close_path(self): void;

	// --- Fills & strokes ------------------------------------------------
	[extern(method, "fill")]
	external fun fill(self): void;

	[extern(method, "stroke")]
	external fun stroke(self): void;

	/// A plain CSS color string ("#e08a1e", "rgb(..)", a named color).
	/// Gradients/patterns are a v1.5 question — §8.
	[extern(set, "fillStyle")]
	external fun set_fill_style(self, color: str): void;

	[extern(set, "strokeStyle")]
	external fun set_stroke_style(self, color: str): void;

	[extern(set, "lineWidth")]
	external fun set_line_width(self, width: f64): void;

	[extern(set, "lineCap")]
	external fun set_line_cap(self, cap: str): void;   // "butt" | "round" | "square"

	[extern(set, "lineJoin")]
	external fun set_line_join(self, join: str): void; // "miter" | "round" | "bevel"

	[extern(set, "globalAlpha")]
	external fun set_global_alpha(self, alpha: f64): void;

	// --- Rects ----------------------------------------------------------
	[extern(method, "fillRect")]
	external fun fill_rect(self, x: f64, y: f64, width: f64, height: f64): void;

	[extern(method, "strokeRect")]
	external fun stroke_rect(self, x: f64, y: f64, width: f64, height: f64): void;

	// --- clearRect --------------------------------------------------------
	[extern(method, "clearRect")]
	external fun clear_rect(self, x: f64, y: f64, width: f64, height: f64): void;

	// --- Text -------------------------------------------------------------
	[extern(set, "font")]
	external fun set_font(self, font: str): void;       // CSS font shorthand

	[extern(set, "textAlign")]
	external fun set_text_align(self, align: str): void;

	[extern(set, "textBaseline")]
	external fun set_text_baseline(self, baseline: str): void;

	[extern(method, "fillText")]
	external fun fill_text(self, text: str, x: f64, y: f64): void;

	[extern(method, "strokeText")]
	external fun stroke_text(self, text: str, x: f64, y: f64): void;

	[extern(method, "measureText")]
	external fun measure_text(self, text: str): TextMetrics;

	// --- Images -------------------------------------------------------
	// `image` is a plain `std::dom::Element` — see below for why.
	[extern(method, "drawImage")]
	external fun draw_image(self, image: Element, dx: f64, dy: f64): void;

	[extern(method, "drawImage")]
	external fun draw_image_scaled(self, image: Element, dx: f64, dy: f64, dw: f64, dh: f64): void;

	[extern(method, "drawImage")]
	external fun draw_image_clip(
		self, image: Element,
		sx: f64, sy: f64, sw: f64, sh: f64,
		dx: f64, dy: f64, dw: f64, dh: f64,
	): void;

	// --- Transforms ---------------------------------------------------
	[extern(method, "translate")]
	external fun translate(self, x: f64, y: f64): void;

	[extern(method, "rotate")]
	external fun rotate(self, radians: f64): void;

	[extern(method, "scale")]
	external fun scale(self, x: f64, y: f64): void;

	[extern(method, "save")]
	external fun save(self): void;

	[extern(method, "restore")]
	external fun restore(self): void;
}

impl TextMetrics {
	[extern(get, "width")]
	external fun width(self): f64;
}
```

**Images: `drawImage` takes a plain `std::dom::Element`, not a new asset
type.** Checked `std/src/asset.vl` — it's a compile-time build-output
channel (`emit(kind, line)`, for the const-eval system), unrelated to
runtime image loading; std has **no** `Image`/asset-loading primitive
today. Rather than inventing one for canvas alone, `drawImage`'s source
reuses the same tag-erased `Element` handle `dom.vl` already returns for
any element — an `<img>` built with `view("img").attr("src", url)` (or
markup) is already an `Element`, decode-readiness already has a home
(`.on_event("load", ..)`, the existing `View`/`Element` event surface),
and `<canvas>`/`<video>` elements (also valid `drawImage` sources) are
`Element`s too. No new module, no new type, no gap to fill — the surface
that already exists happens to be exactly the right shape.

Each host method with several valid argument counts (`arc`'s optional
counterclockwise flag, `fillText`'s optional `maxWidth`, `drawImage`'s
3/5/9-argument forms) gets one vilan name per shape it actually needs in
v1; the omitted shapes (`arc`'s counterclockwise flag, bounded
`fillText`/`strokeText`, `quadraticCurveTo`, `setTransform`) are each a
one-line addition on the same pattern, deferred to when a real call site
wants them — `time.vl`'s own header states the std philosophy this
follows: "Grows from real call sites."

## 4. Resize + DPR

No existing story to build on: verified zero `devicePixelRatio`,
`ResizeObserver`, or `getBoundingClientRect` hits anywhere in `std/src`.
The DPR dance itself is standard and needs three pieces:

1. **Read the ratio.** `window.devicePixelRatio` is a bare, non-callable
   host property — the same shape `location.pathname` is in
   `router.vl:25`, which the codebase already solves with a two-step
   pattern: a tiny JS runtime helper plus a zero-arg
   `[extern("__name")]` wrapper (confirmed in `transformer.rs`: helpers
   are plain JS strings in a `helper_source` match table, wired in by
   adding one name to `EXTERN_HELPERS` and one match arm — a few lines,
   not new machinery). Canvas needs the identical shape:
   ```vilan
   [extern("__device_pixel_ratio")]
   external fun device_pixel_ratio(): f64;
   ```
   with `"function __device_pixel_ratio() {\n\treturn window.devicePixelRatio || 1;\n}"`
   added to `transformer.rs`'s helper table.
2. **Backing store vs. CSS size.** The canvas element's `width`/`height`
   attributes set the *backing store* resolution; the element's CSS box
   size is independent. `dom.vl`'s existing `set_style_property` already
   does general `element.style.setProperty(name, value)` — it's
   documented today as the CSS-custom-property channel, but the
   underlying host call is general-purpose, so `set_style_property("width",
   "400px")` sets a literal CSS property with no new binding needed. The
   v1 idiom:
   ```vilan
   let dpr = device_pixel_ratio();
   let backing_width = (css_width * dpr).as_i32();
   let backing_height = (css_height * dpr).as_i32();
   surface.element.set_attribute("width", i"{backing_width}");
   surface.element.set_attribute("height", i"{backing_height}");
   surface.element.set_style_property("width", i"{css_width.as_i32()}px");
   surface.element.set_style_property("height", i"{css_height.as_i32()}px");
   ctx.scale(dpr, dpr);
   ```
   After the one `scale(dpr, dpr)`, every subsequent draw call is
   authored in CSS-pixel units — the standard pattern, expressible today
   with the extern surface §3 already defines plus the one new
   `device_pixel_ratio` helper.
3. **Reacting to a resize.** No `ResizeObserver` binding exists, and
   building one is a bigger lift than the two items above: it needs a
   new `external struct` carrying a callback that receives a *host array*
   of entries and reads `contentRect.width`/`.height` off each — real new
   binding surface, not a one-line helper. **v1 recommendation: don't
   build it.** Two idioms cover the real cases without it: a fixed-size
   canvas (the common case — charts, games, drawing surfaces with a
   known layout slot), or reacting to the *window's* resize event via a
   `window.addEventListener("resize", ..)` binding (the same shape
   `router.vl:29`'s `window_listen` already is, just not currently
   exported outside routing) for the fluid-container case. True
   per-element `ResizeObserver` tracking is recorded as a v1.5+ item —
   §8.

## 5. swap/disposal interaction (A10)

`View.swap` (`ui.vl:385`) disposes the previous subtree's owner — and
everything that owner's cleanups were registered against — the moment the
driving signal changes, and `get_owner().defer(..)` at every boundary
(`bind_each`, `when`, `swap` all do this) ensures whatever's *live* when
the *enclosing* boundary disposes goes with it too. The question: what
happens to an animating canvas when its `swap`/`when` subtree is torn down
mid-animation?

**The Signal-redraw idiom (§2a) gets this for free.** `angle.effect(..)` is
`get_owner().take(self.sub(observer))` — an ordinary `Subscription` under
the ambient owner, registered the instant it's created, indistinguishable
from `bind_text`'s subscription. When `swap` disposes the subtree's owner,
this subscription disposes with everything else: the closure simply never
fires again. No canvas-specific teardown code exists to write, because
none is needed — this is the strongest structural argument for fork (a)
beyond "it's simpler."

**A `requestAnimationFrame` loop does *not* get this for free**, and this
is exactly why it's the harder half of the design. An rAF loop reschedules
itself via a raw host timer outside the `Signal`/`Owner` system —
structurally the same class of state `std::dev`'s `on_teardown` doc
comment names directly: *"the sanctioned patch for state the swap can't
see on its own (a raw interval, a bare task)."* Stopping it on disposal
needs an explicit hook, not automatic propagation:

```vilan
fun animate(ctx: Context2D): View {
	let running: Shared<bool> = Shared::new(true);
	get_owner().defer(|| { running.write() = false; });
	fun tick(timestamp: f64) {
		if running.read() {
			// .. repaint ..
			request_frame(tick);
		}
	}
	request_frame(tick);
	// ..
}
```

`request_frame`/`cancel_frame` bindings over `requestAnimationFrame`/
`cancelAnimationFrame` don't exist in std today (verified: zero hits) and
would follow `time.vl`'s `Timer` shape (a cancelable handle) if built —
but note the shapes genuinely differ: `Timer` settles once, an animation
frame *reschedules*, so it isn't a drop-in reuse of `Timer`, it's a new
type with the same spirit. **v1 recommendation: don't build it.** The
`Signal`-redraw idiom is disposal-safe for free and covers every
*state-driven* redraw (a chart updating when its data signal changes, a
cursor following a bound position) — the only case that needs a real
frame loop is *continuous* animation decoupled from any discrete signal
change (physics, an idle shimmer), which the entry doesn't cite a real
consumer for. Recorded for v1.5+ per §8, with the sketch above as the
shape it would take.

## 6. SSR / process-twin stance

Canvas is browser-only; there is no process twin, and none is proposed.
This is not a special case — it's the existing pattern for every module
under `std/src/browser/` that has no counterpart under `std/src/process/`.
Concretely:

- `std::dom` itself has no process twin (`process/` has no `dom.vl`), and
  `guide/ui.md`'s SSR section states the consequence in prose: *"`std::dom`
  stays browser-only, so a component reaching for raw DOM cannot SSR; the
  cross-platform error says so at the import."* `std::canvas` imports
  `std::dom::Element` and lives in the same `root = "src/browser"` layer,
  so it inherits exactly that fencing: a server build (`platform =
  "node:*"`/`@process`) importing `std::canvas` gets the recoverable
  cross-platform diagnostic platform-model.md §4.2 defines (the layer
  covers `["browser"]` only; no process-layer file provides the module),
  the same shape `std::ui`'s browser-only members already produce.
- Nothing needs to be *built* to get this — it falls out of file
  placement alone (§1's correction), not a per-struct attribute.
- `std::ui`'s process twin (`process/ui.vl`) renders `<canvas>` as a
  plain, contentless element tag on the server leg — the same as any
  other tag its string-tree `View` doesn't special-case — since SSR never
  calls `context_2d()` (that method lives on `std::dom::Element`, which
  the process twin doesn't have at all; a component that tried would fail
  to resolve the import, not silently no-op). A server-rendered page with
  a canvas ships an empty `<canvas>` tag and the client boots drawing
  into it — consistent with `guide/ui.md`'s stated SSR model ("Bindings
  read once... Event handlers accepted and discarded"): canvas drawing
  is neither a binding nor a handler, so it simply doesn't run
  server-side, exactly like `std::dom` calls in a component today would
  refuse to compile for the server leg if attempted directly.

No v1 work item follows from this section — it's the existing model,
applied.

## 7. Testing story

**There is no headless browser in the suite.** This is stated plainly in
the backlog's own record of E18 (`vilan init`, `proposal/backlog-2026-07-18.md`
around the browser-template item): *"the browser template is built and
inspected rather than executed — there is no headless browser in the
harness, and the emitted-bundle assertions stand in."* Verified directly
against `crates/vilan-cli/tests/init.rs::the_browser_template_builds_a_browser_bundle`:
it builds the scaffold and asserts on the **emitted JS text**
(`javascript.contains("document.")`, no `node:` import) — never executes
it. Three legs, all precedented, none requiring a real or headless
browser:

1. **Emitted-bundle assertions** (the E18 pattern) — build a small canvas
   program with the real CLI, assert the emitted JS contains the expected
   host calls (`getContext`, `arc`, `fillRect`, ...) and *doesn't* contain
   anything node-only. Cheap, matches the exact precedent already gating
   the browser template.
2. **Codegen pins for the external routing** — `inference.rs`-style pins
   per new `[extern(..)]` binding, the same shape `element-syntax.md`
   §10's S1 slice already used for a new dom.vl-style extern ("a browser
   `createTextNode` codegen pin"). One pin per `Context2D` method
   confirms the `extern` attribute lowers to the right host call shape
   (method name, arg order) without needing anything to *execute*.
3. **A DOM-stub-under-node behavioral leg** (extends the existing
   pattern, doesn't invent a new one) — `crates/vilan-cli/tests/router.rs`
   already runs a real built browser bundle under plain Node against a
   "~60-line DOM/history stub" (its own doc comment) that fakes
   `document`/`window`/`history`. The same shape extends naturally: the
   stub's `createElement("canvas")` result gets a `getContext("2d")` that
   returns a **recording** context object — every call
   (`moveTo`/`lineTo`/`fill`/...) appended to a plain JS array instead of
   touching real pixels. This lets *behavior*, not just presence, be
   pinned (`assert the call trace is [beginPath, arc(200,150,20,0,6.28),
   fill]`) with no new dependency (no `node-canvas`, no real or headless
   browser) — it's `router.rs`'s existing technique, generalized to a
   second host object.

`playground`/`web-playground.md`'s "verified end to end in a real browser
(headless Chrome...)" is a one-time **manual** verification pass before
shipping, not a suite-gated leg (no CDP/Puppeteer/Playwright dependency
exists anywhere in `Cargo.toml` or this repo) — the honest description of
today's "live check" is manual-in-a-real-browser, not an automated CDP
script. v1's testing story leans on legs 1–3, with a manual browser check
before release as the human backstop, matching how canvas's own sibling
features (routing, the browser template) actually ship today.

**WebGL is out of scope for v1** — no `getContext("webgl")` binding, no
`WebGLRenderingContext` surface. Unchanged from the entry.

## 8. Open questions, with recommendations

1. **Gradients/patterns — v1 or v1.5?** `set_fill_style`/`set_stroke_style`
   take a plain `str` in v1 (§3). A gradient needs `LinearGradient`/
   `RadialGradient` external structs (`create_linear_gradient(x0,y0,x1,y1)`,
   `.add_color_stop(offset, color)`) plus a *second* setter name per style
   property (`set_fill_gradient`, since vilan has no overloading — the
   same host property, a different vilan name by argument type, `fetch.vl`'s
   `set_body`/`set_body_bytes` shape). **Recommendation: v1.5.** It
   roughly doubles the style-setter surface for a feature nothing in the
   entry or the codebase shows a waiting consumer for; "grows from real
   call sites" argues waiting.
2. **An animation-frame helper in v1?** Covered in full in §5.
   **Recommendation: no** — the Signal-redraw idiom is disposal-safe for
   free and covers state-driven redraws; a real rAF wrapper is a new,
   non-trivial primitive (unlike `Timer`, it reschedules) that wants a
   demonstrated continuous-animation consumer before it's built. Record
   the §5 sketch as the shape to build against when one shows up.
3. **The canvas-in-markup attr story.** Checked against
   `element-syntax.md`'s grammar directly: `TAG = IDENT` with no
   special-casing, so `<canvas width(...) height(...) />` already parses
   and lowers today with **no change needed** to the grammar or the
   desugar — this was never actually gated on canvas existing. The real
   wrinkle: markup's undotted attribute form lowers to `.attr("name",
   value)`, and `View.attr<V: AttrValue>` only accepts `str` and
   `Signal<str>` (`ui.vl:484,491`) — so `<canvas width(400) />` **does
   not typecheck** (`400` is `i32`), and the honest markup spelling is
   `<canvas width("400") />` or, better, the `canvas(400, 300)` helper
   from §1 instead of raw markup for the numeric-dimension case.
   **Recommendation: don't solve this here.** Widening `AttrValue` to
   accept numeric types is a `std::ui`-wide ergonomics question (`width`/
   `height`/`tabindex`/`maxlength` are all numeric HTML attributes, not a
   canvas-specific gap) — out of A17's charter. The `canvas(width, height)`
   helper sidesteps it entirely for canvas's actual v1 need; record the
   broader `AttrValue`-widening idea as its own future backlog item if it
   turns out to matter beyond canvas.
4. **`context_2d()` on `Element` vs. `View`.** §1 recommends `Element`
   on return-type-consistency grounds (no existing `View` method returns
   anything but `View`). Flagged explicitly for sign-off since it's a
   real, opinionated call, not a forced one — the `View` placement is one
   line of counter-argument (`surface.context_2d()` reads slightly
   better than `surface.element.context_2d()`) if ergonomics should win
   over the consistency argument.
5. **Module path.** §1's `std::canvas` (not `std::browser::canvas`) is a
   correction against verified precedent, not a preference — flagged
   because the backlog entry's own wording says the latter, and the
   owner may have meant it as shorthand rather than a literal path.
6. **The cross-file `Element` extension itself.** Confirmed legal
   (§1), but it's a first for this exact shape in std (multiple inherent
   impl blocks for one external struct exist, but always same-file
   today). Worth an explicit yes rather than discovering it as a side
   effect of the canvas work — a same-file alternative (adding
   `context_2d`/`get_context` directly to `dom.vl` instead of `canvas.vl`)
   avoids the precedent question entirely at the cost of `dom.vl`
   growing a canvas-specific method it otherwise has no reason to know
   about. Recommendation: cross-file, in `canvas.vl` — keeps canvas's
   surface (and its future growth) in one file, and the resolution
   mechanism doesn't care.
