
// `router.rs`'s layer over the shared stub (`support/dom/stub.js`). Both of the
// suite's harnesses mount at `#app`, and both drive navigation through
// `location`/`history` and the window's `popstate`, all of which the shared
// stub installs.

const root = installStubDocument({ rootTag: "div", rootId: "app" });
