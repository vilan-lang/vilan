
// `split.rs`'s layer over the shared stub (`support/dom/stub.js`): the module a
// split-bundle harness requires. It waits on RENDERS rather than sampling them,
// so every write to the tree is observed.

let mutations = 0;
const watchers = [];
// Every write to the tree bumps `mutations` and wakes whoever is waiting on the
// next render. This is the observable event a chunk's arrival ends in.
const touched = () => {
    mutations += 1;
    for (const watcher of watchers.splice(0)) watcher();
};

// Whether the FIRST element the program built was created with a chunk fetch
// still in flight — "did the shell render before the route chunk landed?" in
// the only form the stub can see.
let first_element_saw_a_fetch = null;
const fetching = () =>
    globalThis.__vilan_chunks !== undefined &&
    Object.keys(globalThis.__vilan_chunks.pending).length > 0;

// The root is built here rather than by the install so that it is not the
// element the count above sees first: it exists before any program runs.
const root = new StubElement("div");
installStubDocument({
    root,
    rootId: "app",
    onMutate: touched,
    // The namespace-less call is `document.createElement` — the door the
    // program's own elements come through.
    element: (tag, namespace) => {
        if (namespace === undefined && first_element_saw_a_fetch === null) {
            first_element_saw_a_fetch = fetching();
        }
        return new StubElement(tag, namespace);
    },
});

const page = () => root.children.map((child) => child.render()).join("");
// One turn of the loop: `setImmediate` runs after every microtask queued so
// far, and reactive's continuation segments settle on microtasks
// (`std/reactive.vl`), so a turn boundary drains the whole render a resolved
// chunk schedules. A turn is not a duration — this waits for the queue, not for
// the clock.
const turn = () => new Promise((resolve) => setImmediate(resolve));

module.exports = {
    page,
    // Waits for the render that puts `needle` on the page. Returns after the
    // mutation that lands it PLUS one turn, so the surrounding synchronous
    // render batch and any microtask that follows it are complete before the
    // page is sampled. The deadline is a failure mode, not the wait: nothing
    // here passes because it expired.
    rendered: (needle, deadline_ms = 30000) =>
        new Promise((resolve, reject) => {
            const done = () => turn().then(resolve);
            if (page().includes(needle)) return done();
            const timer = setTimeout(() => {
                reject(new Error(
                    `the render carrying ${JSON.stringify(needle)} never arrived within ` +
                    `${deadline_ms}ms; the page is ${JSON.stringify(page())}`,
                ));
            }, deadline_ms);
            const watcher = () => {
                if (!page().includes(needle)) return watchers.push(watcher);
                clearTimeout(timer);
                done();
            };
            watchers.push(watcher);
        }),
    // For an assertion that the page must NOT change: drains turns until one
    // passes with no mutation at all. Used only after the event whose effect is
    // being denied has already been observed (a chunk that landed by the
    // harness's own hand), so this closes a window that is already open rather
    // than standing in for the arrival itself.
    quiet: async (turns = 3) => {
        for (let index = 0; index < turns; index += 1) {
            const before = mutations;
            await turn();
            if (mutations === before) return;
        }
    },
    go: (path) => {
        global.location.pathname = path;
        global.window.fire("popstate", {});
    },
    first_element_saw_a_fetch: () => first_element_saw_a_fetch,
};
