
// `source_bindings.rs`'s layer over the shared stub (`support/dom/stub.js`):
// one serializer, dumped on demand, which is how a binding's effect on the
// tree is read here.

function serialize(node) {
    // A text node serializes as its text, which is nothing for a `Region`
    // anchor (A71) — the same nothing the process twin serves.
    if (node instanceof StubText) return node.textContent;
    let out = "<" + node.tagName;
    for (const [name, value] of Object.entries(node.attributes)) out += ` ${name}="${value}"`;
    for (const [name, value] of Object.entries(node.styleProperties)) out += ` ${name}="${value}"`;
    if (node.hidden) out += " hidden";
    out += ">" + node.textContent;
    for (const child of node.children) out += serialize(child);
    return out + "</" + node.tagName + ">";
}

installStubDocument({ rootTag: "body" });
global.__dump = (tag) => console.log(tag + " " + serialize(documentRoot));
