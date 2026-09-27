
// `dom_events.rs`'s layer over the shared stub (`support/dom/stub.js`). The
// suite builds targets by hand under the name its harnesses use; `window` is a
// stub element like any other, so `window.fire(..)` and `window.count(..)` are
// the shared methods.

const StubTarget = StubElement;

installStubDocument({ rootTag: "div" });
