"use strict";
// A Tarjan SCC walk over a V8 heap snapshot — the reactive graph's standing
// no-cycle gate (proposal/lifetimes.md §5 and §11's S1 line, backlog A28/A29).
//
// The lifetime session measured the five reactive back edges by taking heap
// snapshots of RUNNING programs and looking for strongly-connected components;
// this is that method, small enough to keep and to read. It answers one
// question — "does anything in the app's object graph point back at itself?" —
// and it is the shape Tier B needs, because a reclaimer without a tracing
// collector cannot free a cycle at all.
//
// WHAT IS WALKED. Four edge kinds, and only four:
//   * `property`  — a named field (`{ v: … }`, the `Shared` cell)
//   * `element`   — an array index (every vilan struct emits as a JS array)
//   * `context`   — a scope SLOT (the variables a closure captured)
//   * `internal:context` — a closure's edge to its own scope. V8 files this one
//     under `internal`, and without it the walk cannot see a capture at all.
// Everything else (`hidden`, `weak`, `shortcut`, the rest of `internal`) is
// bookkeeping the language never emits.
//
// WHAT IS EXCLUDED, and why each exclusion is not a way of passing:
//   * Node kinds that are not `object` or `closure`: strings, numbers, code,
//     shapes, backing stores. None can close a cycle a vilan value opened.
//   * `system / …` nodes other than `system / Context`: V8's own records.
//   * The MODULE-SCOPE RECORDS. A module's scope holds its own top-level
//     function declarations, and those functions' closures point back at it, so
//     leaving it in makes one SCC of the whole bundle and says nothing about
//     anything. The rule is structural, not a name list: a scope is a
//     declaration record when it holds a closure whose own scope IS that scope.
//     That catches this bundle's module scope and node's, and it cannot catch a
//     per-invocation scope, which is where every reactive capture lives.
//   * `constructor` / `prototype` / `__proto__` property edges — the host's
//     class plumbing, which is cyclic by construction in every JS program.
//
// WHAT COUNTS AS A CYCLE. An SCC of more than one node containing at least one
// closure or one scope. Every reactive back edge runs through a capture — a
// subscriber's notify, an element's listener — so a component with neither is
// host data (the DOM stub's `parent` ↔ `children`, which a real document has
// too), not the reactive graph this gate is about.

const fs = require("fs");

/**
 * @param {string} snapshotPath a file written by `v8.writeHeapSnapshot`
 * @param {{rootEdgeName: string}} options `rootEdgeName` is the global the
 *   harness parked the app's roots under; the walk is scoped to what it reaches.
 */
function analyze(snapshotPath, options) {
    const snapshot = JSON.parse(fs.readFileSync(snapshotPath, "utf8"));
    const meta = snapshot.snapshot.meta;
    const nodeFieldCount = meta.node_fields.length;
    const edgeFieldCount = meta.edge_fields.length;
    const nodes = snapshot.nodes;
    const edges = snapshot.edges;
    const strings = snapshot.strings;
    const nodeCount = snapshot.snapshot.node_count;
    const nodeTypes = meta.node_types[0];
    const edgeTypes = meta.edge_types[0];

    // Edges are one flat array; a node's run starts where the previous ended.
    const edgeStart = new Int32Array(nodeCount + 1);
    let cursor = 0;
    for (let node = 0; node < nodeCount; node++) {
        edgeStart[node] = cursor;
        cursor += nodes[node * nodeFieldCount + 4];
    }
    edgeStart[nodeCount] = cursor;

    const nameOf = node => strings[nodes[node * nodeFieldCount + 1]];
    const typeOf = node => nodeTypes[nodes[node * nodeFieldCount]];
    const edgeTypeOf = edge => edgeTypes[edges[edge * edgeFieldCount]];
    const edgeNameOf = edge => strings[edges[edge * edgeFieldCount + 1]];
    const edgeTargetOf = edge => edges[edge * edgeFieldCount + 2] / nodeFieldCount;

    const scopeOfClosure = closure => {
        for (let edge = edgeStart[closure]; edge < edgeStart[closure + 1]; edge++) {
            if (edgeTypeOf(edge) === "internal" && edgeNameOf(edge) === "context") {
                return edgeTargetOf(edge);
            }
        }
        return -1;
    };

    // The declaration records: a scope holding a closure whose scope is itself.
    const declarationScopes = new Set();
    for (let node = 0; node < nodeCount; node++) {
        if (nameOf(node) !== "system / Context") continue;
        for (let edge = edgeStart[node]; edge < edgeStart[node + 1]; edge++) {
            if (edgeTypeOf(edge) !== "context") continue;
            const slot = edgeTargetOf(edge);
            if (typeOf(slot) === "closure" && scopeOfClosure(slot) === node) {
                declarationScopes.add(node);
                break;
            }
        }
    }

    const HOST_CLASS_LINKS = new Set(["constructor", "prototype", "__proto__"]);
    const included = node => {
        if (declarationScopes.has(node)) return false;
        const type = typeOf(node);
        if (type !== "object" && type !== "closure") return false;
        const name = nameOf(node);
        return !name.startsWith("system / ") || name === "system / Context";
    };

    const successors = node => {
        const out = [];
        for (let edge = edgeStart[node]; edge < edgeStart[node + 1]; edge++) {
            const type = edgeTypeOf(edge);
            if (type === "element") {
                const target = edgeTargetOf(edge);
                if (included(target)) out.push(target);
                continue;
            }
            const name = edgeNameOf(edge);
            const kept = type === "property" || type === "context"
                || (type === "internal" && name === "context");
            if (!kept) continue;
            if (type === "property" && HOST_CLASS_LINKS.has(name)) continue;
            const target = edgeTargetOf(edge);
            if (included(target)) out.push(target);
        }
        return out;
    };

    const roots = [];
    for (let node = 0; node < nodeCount; node++) {
        for (let edge = edgeStart[node]; edge < edgeStart[node + 1]; edge++) {
            if (edgeTypeOf(edge) === "element") continue;
            if (edgeNameOf(edge) !== options.rootEdgeName) continue;
            const target = edgeTargetOf(edge);
            if (included(target)) roots.push(target);
        }
    }
    if (roots.length === 0) {
        throw new Error(`no snapshot root reached through an edge named ${options.rootEdgeName}`);
    }

    const reachable = new Set();
    const pending = [...roots];
    while (pending.length > 0) {
        const node = pending.pop();
        if (reachable.has(node)) continue;
        reachable.add(node);
        for (const target of successors(node)) {
            if (!reachable.has(target)) pending.push(target);
        }
    }

    // Tarjan, iterative — the graphs are small but the recursion is not worth
    // betting a suite gate on.
    const index = new Map();
    const lowLink = new Map();
    const onStack = new Set();
    const componentStack = [];
    const components = [];
    let counter = 0;
    for (const entry of reachable) {
        if (index.has(entry)) continue;
        const work = [[entry, 0, null]];
        while (work.length > 0) {
            const frame = work[work.length - 1];
            const node = frame[0];
            if (frame[1] === 0) {
                index.set(node, counter);
                lowLink.set(node, counter);
                counter++;
                componentStack.push(node);
                onStack.add(node);
                frame[2] = successors(node).filter(target => reachable.has(target));
            }
            const children = frame[2];
            if (frame[1] < children.length) {
                const child = children[frame[1]];
                frame[1]++;
                if (!index.has(child)) work.push([child, 0, null]);
                else if (onStack.has(child)) {
                    lowLink.set(node, Math.min(lowLink.get(node), index.get(child)));
                }
                continue;
            }
            if (lowLink.get(node) === index.get(node)) {
                const component = [];
                for (;;) {
                    const member = componentStack.pop();
                    onStack.delete(member);
                    component.push(member);
                    if (member === node) break;
                }
                const isReactive = component.length > 1 && component.some(
                    member => typeOf(member) === "closure" || nameOf(member) === "system / Context",
                );
                if (isReactive) components.push(component);
            }
            work.pop();
            if (work.length > 0) {
                const parent = work[work.length - 1][0];
                lowLink.set(parent, Math.min(lowLink.get(parent), lowLink.get(node)));
            }
        }
    }

    const describe = node => `${typeOf(node)}/${nameOf(node) || "(anonymous)"}`;
    return {
        components,
        reachable: reachable.size,
        describe,
        report: components
            .map(component => `SCC of ${component.length}: `
                + component.slice(0, 12).map(describe).join(" | ")
                + (component.length > 12 ? " | ..." : ""))
            .join("\n"),
    };
}

module.exports = { analyze };
