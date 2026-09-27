
// `ssr_fullstack.rs`'s layer over the shared stub (`support/dom/stub.js`): the
// boot suite reads `className` and `value` back off nodes the PROGRAM built, so
// its elements reflect both onto the attribute table the way the platform does.
// The container is built here, not by the install: the harness fills it with a
// mirror of the server-rendered page before the client bundle boots.

class ReflectingElement extends StubElement {
    set className(value) { this.attributes.class = value; }
    get className() { return this.attributes.class || ""; }
    set value(value) { this.attributes.value = value; }
    get value() { return this.attributes.value || ""; }
}

const newElement = (tag, namespace) => new ReflectingElement(tag, namespace);
const container = newElement("div");
