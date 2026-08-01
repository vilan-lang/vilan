# A statement's split reaches its call's last argument

**Status:** implemented 2026-08-01 (backlog 43). Semantics settled below.

## Why

The width rule breaks the construct on the over-budget line. When a statement's
*only* breakable construct sits inside a call argument, there was nothing it
could do:

```vilan
list.push(Task { id = row.integer("id"), workspace_id = row.integer("workspace_id"), name = row.text("name") });
```

152 columns, and stable at 152 however wide it grew — the statement is not a
chain (one call link), so the split had nothing to break at statement level, and
it deliberately stopped at the call's arguments. Kolt has this shape; so does
`examples/walkthrough`'s `load_notes`, which is the same function. Wherever a
row is read into a record, it appears.

That boundary was `Split::Statement`'s v1 scope, recorded in the printer's own
comment. `Split::Tail` — armed one level in, on a split chain's link — already
descended through a call's LAST argument, which is how
`.child(footer_column(t, [..]))` reaches its list. The two permissions differed
in exactly this, and nothing motivated the difference except that v1 stopped
there.

## The rule

`Split::Statement` descends through a call's **last** argument, the way
`Split::Tail` already does. The two permissions now differ in nothing, and the
distinction survives only as a name for where each was armed.

Two mechanical parts, because the permission was lost in two places:

- `print_call_arguments` re-arms for the last argument under either permission,
  not `Tail` alone.
- The `MemberAccessor` arm **forwards** the permission to its member. Without
  this, `list.push(…)` drops it at the `.` — the callee is the member, so a
  method call's arguments were unreachable from statement level whatever
  `print_call_arguments` did. This is why the one-line version of this change
  measured as a no-op on the motivating case.

## What does NOT change

- **Only the last argument.** R5 stands: layout hangs off a call's final
  argument, so an earlier argument that is the over-budget cause still leaves a
  long line. Breaking there needs argument-list layout, which nothing motivates.
- **Nothing about what a construct's split looks like.** This changes only
  *reachability* — which constructs the permission can arrive at. Every split
  form renders exactly as before.

## Measured

Over `std` + `examples` + the corpus + the templates: **8 files, 63 lines**, and
the over-budget line count falls from 94 to 81. Idempotent. The item filed
alongside backlog 42 warned this would be "larger" than 42's 5-file delta and
that it "changes lines that have nothing to do with struct literals" — the first
is true and modest, and the second is the point: `std/rpc.vl`'s 221-column
`match_of(…).arm(…).arm(…)` and `std/hash.vl`'s 148-column
`source("…" + impl_of(…)…)` are chains in an argument, and they break now too.
