
// `reactive_lifetimes.rs`'s layer over the shared stub (`support/dom/stub.js`).
// Nothing beyond the root's tag: the suite reads the tree through `find` and
// `fire`, both of which the shared stub carries.

installStubDocument({ rootTag: "div" });
