
// `hmr_swap.rs`'s layer over the shared stub (`support/dom/stub.js`). Three
// host facts the swap protocol needs and no other suite does:
//
//   * `window === globalThis`, as in a browser. The shim installs its singleton
//     on `window` and its `__hmr_active` helper reads it back off `globalThis`,
//     so the two have to BE one object here. The shared stub's window is an
//     element — right for a suite that fires window events, wrong for one that
//     reads a property off it — so the install's window is replaced.
//   * `location.reload`, which the never-reload discipline is asserted against.
//     The stub's location carries only `pathname`; both are kept.
//   * `Blob` and `URL.createObjectURL`, because the swap loads the new bundle
//     through an object URL and node has no `blob:` loader — a `data:` URL is
//     the sanctioned S2b fallback.

const appRoot = installStubDocument({ rootTag: "div", rootId: "app" });

globalThis.window = globalThis;
globalThis.location = { pathname: "/", reload: () => { globalThis.__reloaded = true; } };
// No EventSource: the shim's connect() is skipped under the stub.
globalThis.Blob = class {
    constructor(parts) { this.__text = parts.join(""); }
};
// Extend (do NOT replace) the real URL so `new URL(...)` still works.
URL.createObjectURL = (blob) =>
    "data:text/javascript;base64," + Buffer.from(blob.__text).toString("base64");
URL.revokeObjectURL = () => {};
