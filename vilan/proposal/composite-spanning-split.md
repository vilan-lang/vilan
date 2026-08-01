# A composite holding a spanning element breaks

**Status:** implemented 2026-08-01 (backlog 49). Semantics settled below.

## Why

`chain-seam-split.md` broke chains whose `})` is followed by more chain. The same
complaint survives one construct over, in the composites a chain link carries:

```vilan
self.subscribers.write().push(Subscriber { id, notify = || {
	observer(self.get());
} });
```

Every line is inside the budget, so the width rule leaves it — and put it back
when its author broke it, which is how `reactive.vl` came to look like this. The
`} });` is three closings of three different things on one line: the closure's
body, the literal, and the call. `json.vl`'s codec is worse, closing a two-field
literal whose fields are both block-bodied closures on a bare `} }`.

This is backlog 47's recorded residue. It was left unbuilt then because the
mechanism was unclear; it is the seam rule's own mechanism, applied to the
element list instead of the link list.

## The rule

A list literal or struct literal whose **element** renders across lines splits,
regardless of width — one element per line, the form those constructs already
have.

## Why ANY element, where a chain needs a non-final link

The chain rule excludes the last link deliberately: a chain that *ends* at its
spanning link has no seam, because the `})` closes the statement and nothing
follows it on that line. That is the trailing-closure idiom, and breaking it
buys nothing.

A composite has no such case. Its closing delimiter always follows the last
element — and in practice so does the enclosing call's `)` and the statement's
`;`. There is no position in which a spanning element leaves a clean line, so
the predicate is every element rather than every non-final one. The two rules
differ because the constructs differ, not because one is a refinement of the
other.

## Measured

Over `std` + `examples` + the corpus + the templates: **3 files, 72 lines**
(`binary.vl`, `json.vl`, `reactive.vl`), idempotent. Every changed site recovers
the shape its author wrote before the formatter collapsed it.

## Interaction with the existing rules

- **Width still wins first**, and the recursion is unchanged: a split element's
  own line is measured as before.
- **Spanning is MEASURED**, by rendering an element and looking, with the same
  non-nesting probe discipline the seam rule uses.
- **Imports and parameter lists are not included.** Neither can hold an element
  that spans lines — an imported name is a name, and a parameter is
  `name: Type`. The rule would be unreachable there, so it is not written.
