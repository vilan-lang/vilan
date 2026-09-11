
// `ssr_differential.rs`'s layer over the shared stub (`support/dom/stub.js`).
// This suite compares the browser tree's SERIALIZATION to the bytes the process
// twin writes, so everything the twin puts in markup has to land in the
// attribute table: `class`, `value`, `hidden`, and the inline declarations,
// folded into one `style` attribute the way the CSSOM's `cssText` reads.

const VOID = new Set(["area","base","br","col","embed","hr","img","input","link","meta","source","track","wbr"]);
const escapeText = s => s.replaceAll("&","&amp;").replaceAll("<","&lt;").replaceAll(">","&gt;");
const escapeAttr = s => s.replaceAll("&","&amp;").replaceAll('"',"&quot;");

const SVG_NS = "http://www.w3.org/2000/svg";

class SerializedElement extends StubElement {
    constructor(tag, namespace) {
        super(tag, namespace);
        // A real createElementNS records no xmlns ATTRIBUTE; the canonical
        // form folds the namespace into the one the process twin seeds on the
        // svg root, so the namespace decision lands in the byte comparison.
        if (namespace === SVG_NS && tag === "svg") this.attributes.xmlns = namespace;
    }
    // A repeated declaration updates the `style` attribute in place rather than
    // appending a second one, and the last one removed takes the attribute with
    // it — `View::show` restores an element's prior inline `display` by writing
    // back what it read, which is the empty string when there was none.
    styleChanged() {
        const declarations = Object.entries(this.styleProperties);
        if (declarations.length === 0) { delete this.attributes.style; return; }
        this.attributes.style = declarations.map(([name, value]) => name + ":" + value).join(";");
    }
    set className(value) { this.attributes.class = value; }
    get className() { const value = this.attributes.class; return value === undefined ? "" : value; }
    set hidden(on) { if (on) this.attributes.hidden = ""; else delete this.attributes.hidden; }
    get hidden() { return "hidden" in this.attributes; }
    set value(value) { this.attributes.value = value; }
    get value() { const value = this.attributes.value; return value === undefined ? "" : value; }
}

function serialize(element) {
    // A71: a text node serializes as its text. A `Region`'s anchor is the
    // EMPTY one, so it serializes to nothing — which is exactly what the
    // process twin, which plants no anchor at all, writes there. That is the
    // whole reason the anchor is an empty text node and not a comment: a
    // `<!---->` would break this byte equality.
    if (element instanceof StubText) return escapeText(element.textContent);
    let out = "<" + element.tagName;
    for (const [name, value] of Object.entries(element.attributes)) out += ` ${name}="${escapeAttr(value)}"`;
    out += ">";
    if (VOID.has(element.tagName)) return out;
    out += escapeText(element.textContent);
    for (const child of element.children) out += serialize(child);
    return out + "</" + element.tagName + ">";
}

const root = installStubDocument({
    rootTag: "app-root",
    rootId: "app",
    element: (tag, namespace) => new SerializedElement(tag, namespace),
});
