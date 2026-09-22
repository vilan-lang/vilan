// The DOM stub the browser-bundle suites share (N73).
//
// Eight suites each carried a near-copy of this file as a raw-string const,
// which is how A71's one `insertBefore` / `Text::remove` addition turned into
// seven edits, and how `router.rs`'s second copy came to call `indexOf` where
// every other copy calls `lastIndexOf`. The DOM lives here once; a suite layers
// ITS extras on top by concatenation, so nothing about how a suite writes a
// harness changes:
//
//     const DOM_STUB: &str = concat!(
//         include_str!("support/dom/stub.js"),
//         include_str!("support/dom/<suite>.js"),
//     );
//
// Three rules keep sharing behaviour-neutral, and a new addition follows them:
//
//   * Everything only a HARNESS calls — `find`, `findAll`, `click`, `fire`,
//     `render`, `flatten`, `describe` — belongs here even when one suite uses
//     it: a method no emitted program reaches cannot change what a program
//     does, and the suite that needs it next finds it already written.
//   * Anything a PROGRAM reaches whose shape differs between suites — the
//     reflection of `className` / `value` / `hidden` onto attributes, the fold
//     of inline styles into a `style` attribute, a serializer — stays with the
//     suite, as a subclass handed to `installStubDocument`.
//   * Every write to the tree calls `notifyMutation()`. It is a no-op until a
//     suite installs one (`split.rs` waits on renders), so the call costs the
//     others nothing and means none of them has to fork the write path.

let nextIdentity = 1;
const identities = new WeakMap();
/// A stable per-object id: what makes "the same node, updated" distinguishable
/// from "a new node in its place" in a flattened tree.
function identify(node) {
    if (!identities.has(node)) identities.set(node, nextIdentity++);
    return identities.get(node);
}

/// Called after every write to the tree. `installStubDocument({ onMutate })`
/// replaces it for a suite that waits on renders rather than sampling them.
let notifyMutation = () => {};

/// The document root, set by `installStubDocument` — also `global.documentRoot`.
let documentRoot = null;

/// An inline style declaration block, enough of one for the questions asked
/// here: `setProperty` with an empty value REMOVES the declaration, which is
/// what the CSSOM does and what `View::show` relies on to restore an element's
/// prior inline `display` (A60). Recorded, never dropped: `style_var`'s only
/// observable effect is this write, so a no-op stub cannot see a subscription
/// that outlived its boundary (A21).
class StubStyle {
    constructor(owner) {
        this.properties = {};
        this.owner = owner;
    }
    setProperty(name, value) {
        if (value === "" || value === null || value === undefined) delete this.properties[name];
        else this.properties[name] = value;
        this.owner.styleChanged();
    }
    getPropertyValue(name) { return this.properties[name] || ""; }
    removeProperty(name) { delete this.properties[name]; this.owner.styleChanged(); }
}

class StubElement {
    constructor(tag, namespace) {
        this.tagName = tag;
        this.namespaceURI = namespace || "http://www.w3.org/1999/xhtml";
        this.children = [];
        this.parent = null;
        this.listeners = {};
        // A59: capture-phase registrations are a SEPARATE table, because the
        // host matches on the phase as part of a listener's identity — a
        // capture listener is not removable by a bubble-phase `off_event`.
        this.captureListeners = {};
        this._text = "";
        // The reflecting properties are held in fields and reached through
        // accessors so a suite can override the PAIR without the constructor's
        // seeding write landing in its attribute table.
        this._value = "";
        this._className = "";
        this._hidden = false;
        this.attributes = {};
        this.focused = false;
        this.style = new StubStyle(this);
    }
    /// Hook: a suite whose serializer reads inline styles off an ATTRIBUTE
    /// (the SSR differential does — the process twin writes one) folds them in
    /// here. Nothing to do when the declarations are read where they are kept.
    styleChanged() {}
    /// The declarations written through `element.style`, by name.
    get styleProperties() { return this.style.properties; }
    set textContent(text) { this._text = text; this.children = []; notifyMutation(); }
    get textContent() { return this._text; }
    set value(value) { this._value = value; }
    get value() { return this._value; }
    set className(value) { this._className = value; }
    get className() { return this._className; }
    set hidden(on) { this._hidden = on; }
    get hidden() { return this._hidden; }
    get parentNode() { return this.parent; }
    // A121: the parent ELEMENT — the step `Element::tabbable`'s ancestor walk
    // takes. The stub has no `<body>`/`<html>` split, so the document root's
    // parent is null, which is the rule the walk terminates on.
    get parentElement() { return this.parent; }
    setAttribute(name, value) { this.attributes[name] = value; notifyMutation(); }
    removeAttribute(name) { delete this.attributes[name]; notifyMutation(); }
    // A121: presence, which is the whole meaning of a boolean attribute.
    hasAttribute(name) { return name in this.attributes; }
    // A121: `tabIndex` is the attribute reflected with a PER-TAG DEFAULT, and
    // the default is the half a walk cannot do without — `-1` for a bare
    // `<div>`, `0` for a natively focusable tag. The table is the HTML
    // specification's focusable set, reduced to the tags a test writes;
    // `<a>` counts only with an `href`, as in the platform.
    get tabIndex() {
        const written = this.attributes.tabindex;
        if (written !== undefined) {
            const parsed = Number.parseInt(written, 10);
            return Number.isNaN(parsed) ? -1 : parsed;
        }
        if ("contenteditable" in this.attributes) return 0;
        const tag = String(this.tagName).toLowerCase();
        if (tag === "a" || tag === "area") return "href" in this.attributes ? 0 : -1;
        if (tag === "audio" || tag === "video") return "controls" in this.attributes ? 0 : -1;
        return ["button", "input", "select", "textarea", "summary", "iframe", "object", "embed"]
            .includes(tag) ? 0 : -1;
    }
    appendChild(child) {
        // A DOM insertion SPLICES a document fragment: its children move and
        // the fragment is left empty. A91 builds a row inside one, so this is
        // the path every row of a keyed run takes.
        if (child.tagName === "#fragment") {
            for (const node of child.take()) this.appendChild(node);
            return child;
        }
        if (child.parent) child.parent.children = child.parent.children.filter(c => c !== child);
        child.parent = this;
        this.children.push(child);
        notifyMutation();
        return child;
    }
    // A71: `appendChild`'s positional counterpart. `std::ui`'s `Region`
    // plants an empty text node and inserts its content BEFORE it, so a
    // reactive run keeps its place among static siblings.
    insertBefore(child, anchor) {
        if (child.tagName === "#fragment") {
            for (const node of child.take()) this.insertBefore(node, anchor);
            return child;
        }
        if (child.parent) child.parent.children = child.parent.children.filter(c => c !== child);
        child.parent = this;
        const at = this.children.lastIndexOf(anchor);
        if (at < 0) this.children.push(child); else this.children.splice(at, 0, child);
        notifyMutation();
        return child;
    }
    remove() {
        if (this.parent) {
            this.parent.children = this.parent.children.filter(c => c !== this);
            this.parent = null;
        }
        notifyMutation();
    }
    replaceChildren() {
        for (const c of this.children) c.parent = null;
        this.children = [];
        notifyMutation();
    }
    addEventListener(event, handler, capture) {
        const table = capture ? this.captureListeners : this.listeners;
        (table[event] = table[event] || []).push(handler);
    }
    // Identity-matched, exactly as the DOM's is. A `dispose` that reconstructs
    // the handler instead of holding the registered one removes NOTHING here.
    removeEventListener(event, handler, capture) {
        const table = capture ? this.captureListeners : this.listeners;
        table[event] = (table[event] || []).filter(registered => registered !== handler);
    }
    /// How many bubble-phase listeners are registered for `event` — the shape
    /// a disposal claim is read in.
    count(event) { return (this.listeners[event] || []).length; }
    // Slice: a handler that disposes its own registration must not perturb the
    // iteration it is being dispatched from.
    fire(event, payload = {}) { for (const h of (this.listeners[event] || []).slice()) h(payload); }
    /// A click carrying the modifier keys a router reads, and recording whether
    /// the handler called `preventDefault` — the whole question a link asks.
    click(overrides = {}) {
        const event = {
            button: 0, metaKey: false, ctrlKey: false, shiftKey: false, altKey: false,
            prevented: false, preventDefault() { this.prevented = true; }, ...overrides,
        };
        for (const handler of (this.listeners.click || []).slice()) handler(event);
        return event;
    }
    // The walks skip TEXT nodes: a `Region`'s anchor (A71) is one, and these
    // ask element questions.
    find(predicate) {
        if (predicate(this)) return this;
        for (const child of this.children) {
            const hit = child.find && child.find(predicate);
            if (hit) return hit;
        }
        return null;
    }
    findAll(predicate, accumulator) {
        accumulator = accumulator || [];
        if (predicate(this)) accumulator.push(this);
        for (const child of this.children) if (child.findAll) child.findAll(predicate, accumulator);
        return accumulator;
    }
    // Tag-name selectors only — enough to ask "which of my descendants", which
    // is the whole question the scoped binding answers.
    querySelectorAll(selector) {
        const found = [];
        const walk = (node) => {
            for (const child of node.children) {
                // A121: `*` is the UNIVERSAL selector — every descendant
                // element, in document order. It is not a predicate, which is
                // why the walk `Element::tabbable` does may be written over it
                // while its predicates stay per-element reads (the `matches`
                // below still knows only `:focus`). Text nodes are not
                // elements and are skipped, as in the DOM.
                if (selector === "*" ? child.tagName[0] !== "#" : child.tagName === selector) {
                    found.push(child);
                }
                walk(child);
            }
        };
        walk(this);
        return found;
    }
    contains(other) {
        for (let walk = other; walk; walk = walk.parent) if (walk === this) return true;
        return false;
    }
    get isConnected() { return inDocument(this); }
    // A59: the stub has no layout engine, so a measured box comes from
    // `global.boxes` keyed by tag — a harness sets it before requiring the
    // bundle. The rule that IS modelled is the one the binding exists to let a
    // caller wait on: a DETACHED element measures 0x0, whatever the table says.
    getBoundingClientRect() {
        const zero = { left: 0, top: 0, width: 0, height: 0 };
        return inDocument(this) ? ((global.boxes || {})[this.tagName] || zero) : zero;
    }
    get offsetWidth() { return Math.round(this.getBoundingClientRect().width); }
    get offsetHeight() { return Math.round(this.getBoundingClientRect().height); }
    // A121: a focus that takes DISPATCHES `focusin` — the event a containment
    // guard hears, and without it the guard cannot fire at all. The host
    // dispatches `blur`/`focusout` on the old holder and `focus`/`focusin` on
    // the new one; the stub models the one a scope listens for, in the phase
    // it listens in (`dispatchEvent` runs the window's capture listeners
    // first). `relatedTarget` carries the element focus LEFT, as it does in
    // the platform, so an S3 `focusout` path reads the same shape.
    focus() {
        const previous = global.activeElement;
        this.focused = true;
        global.activeElement = this;
        global.focusLog.push(describe(this));
        // `global.window` is the dispatch's own precondition: a `focus()` before
        // `installStubDocument` has no window to run capture listeners on.
        if (previous !== this && global.window) {
            dispatchEvent(this, "focusin", { relatedTarget: previous || null });
        }
    }
    // `element.matches(":focus")` is how `View::autofocus` reads back whether
    // its request was honored (B271); the stub answers the one selector it is
    // ever asked for.
    matches(selector) {
        if (selector !== ":focus") throw new Error("the stub knows only :focus, got " + selector);
        return global.activeElement === this;
    }
    /// The subtree as markup, tags and text only — what a page-level assertion
    /// reads. A text child renders as its text, so a `Region` anchor renders
    /// as nothing.
    render() {
        const inner = this.children.map(c => (c.render ? c.render() : c.textContent)).join("");
        return `<${this.tagName}>${this._text}${inner}</${this.tagName}>`;
    }
}

/// A text node — a real sibling of the element children: what a `str` or a
/// `Source<str>` child rides, and what `std::ui`'s `Region` plants (empty) as
/// the anchor it inserts before (A71).
class StubText {
    constructor(text) { this.tagName = "#text"; this.children = []; this.parent = null; this._text = text; }
    set textContent(text) { this._text = text; notifyMutation(); }
    get textContent() { return this._text; }
    // A91: a `Region` reads its host off its anchor, because an anchor MOVES
    // when the row holding it is planted or re-ordered.
    get parentNode() { return this.parent; }
    remove() {
        if (this.parent) {
            this.parent.children = this.parent.children.filter(c => c !== this);
            this.parent = null;
        }
        notifyMutation();
    }
    render() { return this._text; }
}

/// A document fragment — a staging container whose CHILDREN move when it is
/// inserted (A91). `std::ui` builds a row in one of these and plants the whole
/// thing with a single `insertBefore`, which is how content of an unknown
/// shape reaches a position that is not the parent's end.
class StubFragment {
    constructor() { this.tagName = "#fragment"; this.children = []; this.parent = null; this._text = ""; }
    appendChild(child) {
        if (child.tagName === "#fragment") {
            for (const node of child.take()) this.appendChild(node);
            return child;
        }
        if (child.parent) child.parent.children = child.parent.children.filter(c => c !== child);
        child.parent = this;
        this.children.push(child);
        return child;
    }
    insertBefore(child, anchor) {
        if (child.tagName === "#fragment") {
            for (const node of child.take()) this.insertBefore(node, anchor);
            return child;
        }
        if (child.parent) child.parent.children = child.parent.children.filter(c => c !== child);
        child.parent = this;
        const at = this.children.lastIndexOf(anchor);
        if (at < 0) this.children.push(child); else this.children.splice(at, 0, child);
        return child;
    }
    /// Empties the fragment and hands back what was in it, in order.
    take() {
        const held = this.children;
        this.children = [];
        for (const node of held) node.parent = null;
        return held;
    }
    render() { return this.children.map(c => c.render()).join(""); }
}

/// A DOM range, reduced to the one thing `std::ui` asks of it: cut every node
/// between two sibling markers out of the document in one call (A91). A row's
/// node set is not fixed — a row that is itself a `when` or an `each` grows
/// after it was placed — so "move this row" is a span between markers rather
/// than a list of remembered nodes.
class StubRange {
    setStartAfter(node) { this.start = node; }
    setEndBefore(node) { this.end = node; }
    extractContents() {
        const fragment = new StubFragment();
        const parent = this.start.parent;
        if (!parent) return fragment;
        const from = parent.children.lastIndexOf(this.start) + 1;
        const to = parent.children.lastIndexOf(this.end);
        if (to < from) return fragment;
        for (const node of parent.children.slice(from, to)) fragment.appendChild(node);
        notifyMutation();
        return fragment;
    }
}

/// Whether `node` is reachable from the document root by parent links — the
/// question `on_mount` exists to answer.
function inDocument(node) {
    let walk = node;
    while (walk) {
        if (walk === documentRoot) return true;
        walk = walk.parent;
    }
    return false;
}

function describe(node) {
    return node.tagName + "#" + identify(node) + (inDocument(node) ? "@doc" : "@detached");
}

/// The whole tree as one flat line, each node as tag#identity'text'.
function flatten(node) {
    const own = node.tagName + "#" + identify(node) + (node._text ? "'" + node._text + "'" : "");
    return [own].concat(node.children.flatMap(flatten)).join(" ");
}

/// A59: dispatch the way the host does — the window's and every ancestor's
/// CAPTURE listeners on the way DOWN to the target, then the target's own and
/// every ancestor's on the way back UP. The order is the whole point of the
/// capture surface, so the stub has to get it right rather than fire a list.
function dispatchEvent(target, type, extra = {}) {
    const chain = [];
    for (let walk = target; walk; walk = walk.parent) chain.unshift(walk);
    const event = { target, type, preventDefault() { this.prevented = true; }, ...extra };
    const stubWindow = global.window;
    for (const handler of (stubWindow.captureListeners[type] || [])) handler(event);
    for (const node of chain) for (const handler of (node.captureListeners[type] || [])) handler(event);
    for (const node of chain.slice().reverse()) for (const handler of (node.listeners[type] || [])) handler(event);
    for (const handler of (stubWindow.listeners[type] || [])) handler(event);
    return event;
}

/// A59: the host `ResizeObserver`. `observe` fires the callback ONCE straight
/// away, as the real one does — that is what makes `observe_resize` a
/// first-layout hook — and `disconnect` forgets every target, so a disconnected
/// observer is silent for `resize(..)` below.
const resizeObservers = [];
global.ResizeObserver = class {
    constructor(callback) { this.callback = callback; this.targets = []; resizeObservers.push(this); }
    observe(target) { this.targets.push(target); this.callback(); }
    disconnect() { this.targets = []; }
};

/// Fire every observer still watching `element` — a size change.
global.resize = (element) => {
    for (const observer of resizeObservers) {
        if (observer.targets.includes(element)) observer.callback();
    }
};

// The frame clock. node has none, and every leg but B271's only needs it to
// EXIST — a macrotask is the honest stand-in for "after the rendering step".
// B271's own leg replaces this with a queue it drives, because "on the next
// frame" is a claim about order.
global.requestAnimationFrame = (callback) => setTimeout(callback, 0);

/// Install `document`, `window`, `location` and `history`, and return the root
/// element. Options:
///
///   * `rootTag`   — the root's tag name. It is printed by `flatten` and by a
///                   serializer, so a suite keeps the one its assertions read.
///   * `rootId`    — the id `getElementById` answers with the root, or `null`
///                   (the default) to answer every id with it. `mount_root`'s
///                   missing-id path needs the id to be able to MISS.
///   * `root`      — an already-built root to adopt, for a suite that has to
///                   put nodes in it before any program runs (a server-rendered
///                   page the boot replaces) or that counts what the program
///                   builds and must not count the root itself.
///   * `element`   — the element factory, for a suite whose nodes reflect more
///                   than the shared ones do.
///   * `text`      — the text-node factory, same reason.
///   * `onMutate`  — called after every write to the tree.
function installStubDocument(options = {}) {
    const settings = {
        rootTag: "root",
        rootId: null,
        root: null,
        element: (tag, namespace) => new StubElement(tag, namespace),
        text: (text) => new StubText(text),
        onMutate: null,
        ...options,
    };
    if (settings.onMutate) notifyMutation = settings.onMutate;
    documentRoot = settings.root || settings.element(settings.rootTag);
    global.focusLog = [];
    global.activeElement = null;
    global.document = {
        createElement: (tag) => settings.element(tag),
        createElementNS: (namespace, tag) => settings.element(tag, namespace),
        createTextNode: (text) => settings.text(text),
        createDocumentFragment: () => new StubFragment(),
        createRange: () => new StubRange(),
        getElementById: (id) => (settings.rootId === null || id === settings.rootId ? documentRoot : null),
        querySelector: () => null,
        querySelectorAll: () => [],
        get activeElement() { return global.activeElement; },
    };
    // The window is an element too: it takes listeners, and a harness fires
    // them the same way it fires an element's (`window.fire("resize")`).
    global.window = new StubElement("window");
    global.windowListeners = { bubble: global.window.listeners, capture: global.window.captureListeners };
    global.location = { pathname: "/" };
    // The history STACK, not just the current path: `pushState` grows it and
    // `replaceState` rewrites its top, which is the only difference between
    // `navigate` and `navigate_replace` (A72) and therefore the only thing a
    // pin can read. Nothing else has to look at it.
    global.historyEntries = ["/"];
    global.history = {
        pushState(state, title, path) {
            global.location.pathname = path;
            global.historyEntries.push(path);
        },
        replaceState(state, title, path) {
            global.location.pathname = path;
            global.historyEntries[global.historyEntries.length - 1] = path;
        },
    };
    // A121: the resolved style. The stub has no cascade, so a value comes from
    // the element's own `computedStyle` table when a harness set one, then
    // from its inline declarations, then from the defaults below — and
    // `visibility` INHERITS, which is the one cascade rule
    // `Element::computed_style` exists to read, so it walks up for that
    // property exactly as the engine's inheritance would.
    global.computedDefaults = { visibility: "visible", display: "block" };
    global.getComputedStyle = (element) => ({
        getPropertyValue(name) {
            for (let walk = element; walk; walk = walk.parent) {
                const table = walk.computedStyle;
                if (table && table[name] !== undefined) return table[name];
                const inline = walk.style ? walk.style.getPropertyValue(name) : "";
                if (inline !== "") return inline;
                if (name !== "visibility") break;
            }
            return global.computedDefaults[name] || "";
        },
    });
    global.documentRoot = documentRoot;
    global.inDocument = inDocument;
    global.describe = describe;
    global.flatten = flatten;
    global.identify = identify;
    global.dispatchEvent = dispatchEvent;
    return documentRoot;
}
