# Comment attachment inside split constructs

**Status:** implemented 2026-08-01 (backlog 41). Semantics settled below.
**Extended same day:** element expressions (`element-syntax.md`) are the sixth
construct — a comment between a markup head's items or children attaches and
forces the split, through the same `flush_element_comments` /
`comment_between_elements` pair; a comment after the last child relocates,
list parity. Pinned in `element_layout`.

## Why

The comment machinery flushes at *statement* boundaries, so a comment written
inside an expression has nowhere to go and lands below the whole statement:

```vilan
let short = one()
	// a note
	.two(2)
	.three(3);
```

reprints as

```vilan
let short = one().two(2).three(3);
// a note
```

Never dropped — E13's law holds — but orphaned from the link it explains, and
now attached to whatever follows instead. `reactive-ui/counter.vl`'s `bind_text`
note ends up dangling before a closing brace. The same happens between a struct
literal's fields, an import set's names, and a parameter list's parameters, for
the same reason.

Until the width arc (backlog 42/44/45/46/48) there was nowhere to put such a
comment: the constructs collapsed onto one line. Now every one of them has a
split form with one element per line, so there is a line to attach to.

## The rule

**A. A comment between elements forces the split.** A splittable construct whose
source carries a standalone comment *between* two of its elements renders in its
split form regardless of width. A collapsed construct has no line to hold the
comment, so this is what makes attachment possible at all rather than a
preference — it is the same reasoning as rustfmt's.

The trigger is deliberately the *gaps between* elements, not the construct's
whole span: a comment inside a closure body that a link happens to carry
(`.on("click", || { // note`) belongs to that body and already prints where it
was written. Only a comment in a between-elements gap forces the split.

**B. A comment attaches to the element it precedes.** In a split construct, a
standalone comment before an element prints on its own line above that element,
at the element's indentation. A trailing same-line comment after an element
stays on that element's line. This is exactly what `print_items` already does
for statements, applied one level in.

```vilan
let short = one()
	// a note
	.two(2)
	.three(3);
```

is now a fixed point: it forces the split (A) and keeps the note above `.two(2)`
(B).

## Scope

One mechanism, all five split forms, because the item asks for the fix to be
written against the split construct generally rather than the chain
specifically: postfix chains, list literals, struct literals, import brace sets,
and `fun` parameter lists. Each already loops over elements with a source span
per element; attachment is a flush before and a trailing flush after.

## Consequences

- **Width is no longer the only thing that splits a chain**, and this is now the
  third door alongside the seam rule (`chain-seam-split.md`). A hand-split
  construct carrying a comment stays split; one carrying none still collapses if
  it fits, so the comma/collapse rules are unchanged for comment-free code.
- **The examples fmt sweep unblocks.** It was queued behind this precisely
  because sweeping earlier would have moved 17 files' pedagogical comments out
  of place.
- **No comment moves relative to its code.** The rule only ever moves a comment
  from *below the statement* to *above the element it preceded*, which is where
  the author wrote it.
