
// `ui_rows.rs`'s layer over the shared stub (`support/dom/stub.js`).

/// `hidden` is a reflecting property in the DOM: the attribute is what CSS and
/// assistive technology see, and this suite reads the attribute table back, so
/// its elements reflect it. (A60's `show`/`hide` writes the PROPERTY; the pin
/// is that the attribute follows.)
class UiRowsElement extends StubElement {
    set hidden(on) { if (on) this.attributes.hidden = ""; else delete this.attributes.hidden; }
    get hidden() { return "hidden" in this.attributes; }
}

installStubDocument({
    rootTag: "root",
    element: (tag, namespace) => new UiRowsElement(tag, namespace),
});
