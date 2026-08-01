# Chain seam split — a `})` that continues is not a line

**Status:** implemented 2026-08-01 (backlog 48). Semantics settled below.

## Why

The width rule breaks a chain when its line is too wide. It has nothing to say
about a chain that is *narrow* and still reads badly:

```vilan
let server = Server::builder().port(3000).on_request(|request| {
	…
}).on_start(|server| {
	print(server.url());
}).build();
```

Every line here is inside the budget, so the formatter leaves it — and puts it
back if the author breaks it. The problem is not width, it is the **seam**: a
link's closing `})` lands on a line that then continues with more chain, so
`}).on_start(|server| {` is simultaneously the end of one argument, the start of
the next link, and the start of *its* argument. Three unrelated things, one line.
The eye has nowhere to rest, and the chain's shape — the thing the split form
exists to show — is invisible.

This came out of backlog 47's residue: after five bailing std files began
formatting, some of what they produced read worse than what their authors wrote,
and "the opening line fits" was the rule saying so.

## The rule

A postfix chain the split form can break (two or more `.name(…)` call links)
splits **regardless of width** when a call link that is **not the chain's last**
renders across lines.

```vilan
let server = Server::builder()
	.port(3000)
	.on_request(|request| {
		…
	})
	.on_start(|server| {
		print(server.url());
	})
	.build();
```

Everything else about the split form is unchanged — it is the same
`print_split_chain`, reached by a second door.

### Why "not the last"

Because the last link has no seam. When the chain *ends* at its spanning link,
the closing `})` closes the statement and nothing follows it on that line:

```vilan
self.cleanups.write().push(|| {
	item.dispose();
});
```

That is the ordinary trailing-closure idiom, it is already readable, and
breaking it buys two lines of noise and no clarity:

```vilan
self.cleanups              // NOT what this rule does
	.write()
	.push(|| {
		item.dispose();
	});
```

The measured difference is not marginal. Over `std` + `examples` + the corpus,
counting a spanning link anywhere touches 8 files and 170 lines; counting only
non-final links touches 5 files and 121 lines, **none of them in std** — every
std case the broader reading would have changed is a trailing closure that
should stay put.

### Measured, not predicted

Whether a link "renders across lines" is decided by rendering it and looking,
not by inspecting its AST for block-shaped nodes. This is the same discipline
the width rule follows, and for the same reason: the printer is the only thing
that knows what the printer will do, so a predicate that guesses drifts from it.

The probe renders one link into the output buffer, checks for a newline, and
truncates back — restoring the comment cursor, the bail flag and the pending
split, so a probe is invisible. Probes do not nest: a probe already in progress
suppresses further seam checks inside it. That bounds the cost (a subtree is
rendered once per level rather than exponentially) and changes no answer,
because a nested chain only seam-splits when a body already spans lines, which
the probe sees either way.

## Interaction with the existing rules

- **Width still wins first.** An over-budget chain splits as it always did; this
  rule only adds a reason, never removes one.
- **The recursion is unchanged.** A split link's own line is measured as before,
  so a link that is still too wide breaks its last argument one level further.
- **Nothing else gains a second door.** List literals, struct literals, import
  sets and parameter lists split on width alone. A struct literal holding a
  multi-line closure — `push(Subscriber { id, notify = || { … } })` — therefore
  stays inline, which is 47's other residue and is NOT addressed here: fixing it
  needs the split to land on the construct that *holds* the spanning element,
  a mechanism the printer does not have. Recorded, deliberately unbuilt.

## What this does not do

It does not touch a chain of one call link (there is no shape to show), a `?.`
lift chain (no postfix spine), or any non-chain expression — notably a
triple-quoted string, which spans lines because of its *contents*. An earlier
draft of this rule triggered on "the statement's rendering spans lines" and
broke `let line = """…""" + "!";` at the operator for exactly that reason; the
chain restriction is what excludes it.
