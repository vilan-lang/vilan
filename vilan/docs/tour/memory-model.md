# The memory model

> Normative rules: spec [§6 The memory model](../spec/memory.md).

This is the chapter where Vilan differs most from JavaScript, so it goes
slowly. The one-sentence version: **values are copied, and sharing is
something you ask for explicitly.**

In JavaScript, objects and arrays are shared by reference. Passing one to
a function means the function can mutate your data. Storing one in two
places means a change shows up in both. That's convenient right up until
it isn't, and then it's a bug hunt.

Vilan flips the default. Everything copies. Then it gives you four tools,
each for one kind of sharing, so that when data *is* shared, the code
says so. Here's the decision tree you'll internalize quickly:

1. Default: plain **values**. Copies everywhere.
2. A function should mutate my thing in place: pass a **view** (`&mut`).
3. Two long-lived places need the same mutable state: a **`Shared<T>`**
   cell.
4. Things need to point at each other (graphs, cycles): an **`Arena`**
   with **`Handle`** ids.

## Values: the default

`let` binds immutably. `mut` allows reassignment and field mutation.
Either way, binding an existing value makes a copy:

```vilan
import std::print;

struct Counter { value: i32 }

fun main() {
	mut original = Counter { value = 10 };
	mut copy = original;
	copy.value = 99;
	print(original.value); // 10 — a binding copies
}
```

Passing to a function is the same. The callee gets its own value, and
nothing it does leaks back to you. If you've ever defensively
`structuredClone`d an object before handing it out, this is that,
everywhere, for free.

> **Going deeper.** "Copy" describes the *semantics*, not necessarily
> the machine code. The compiler skips copies whenever no program could
> tell the difference (for example, when the source is never used
> again). You reason in copies; it optimizes.

## Views: lending a value out

Sometimes you *want* a function to mutate your value. You lend it a
**view** with `&mut`:

```vilan
import std::print;

struct Counter { value: i32 }

fun bump(&mut c: Counter) {
	c.value = c.value + 10;
}

impl Counter {
	fun increment(&mut self) {
		self.value = self.value + 1;
	}
}

fun main() {
	mut c = Counter { value = 10 };
	c.increment();
	bump(&mut c);
	print(c.value); // 21 — both mutated the original through views
}
```

A view aliases the original instead of copying it. `&mut` views can
write; `&` views can only read. The `&mut self` receiver is the same
idea for methods: "this method changes the actual object".

The two behaviors side by side:

```text
mut b = a          — a COPY               let v = &mut a    — a VIEW

a ──▶ ┌─────────┐                         a ──▶ ┌─────────┐
      │ x=1 y=2 │                               │ x=1 y=2 │
      └─────────┘                          v ──▶└─────────┘
b ──▶ ┌─────────┐
      │ x=1 y=2 │                          two names, ONE box:
      └─────────┘                          writing through v changes a

two boxes: b.x = 9
never touches a
```

Views come with a deliberate restriction: **they don't outlive the
moment.** A view can be a parameter or a short-lived local. It cannot be
stored in a struct field, put in a list, returned into long-lived state,
or held across a suspension. Lend, use, done. That confinement is what
makes views safe without a whole lifetime system. If you've heard Rust
horror stories, this is the part Vilan deliberately keeps small.

Views also make loops that mutate in place. A plain `for x in list`
copies each element; `for e in &mut list` gives you writable views
instead:

```vilan
import std::print;

fun main() {
	mut xs: List<i32> = [1, 2, 3];
	for e in &mut xs {
		e = *e * 10;
	}
	print(xs[2]); // 30
}
```

Inside the loop, `*e` reads the element and assigning to `e` writes
through to the list.

> **Going deeper.** A method may return a view that projects its
> receiver, like `fun get(&mut self, i: i32): &mut T`. The compiler
> infers which parameter the view borrows from, and the returned view
> obeys the same short-lived rules at the call site, anchored to what
> it projects: a `list.push(..)` while a view from `list.at(0)` is live
> is the same compile error a direct `&mut list[0]` would raise.
> `Option<&T>` covers "a view, maybe" (map lookups). The compiler is
> precise about *which* mutations invalidate: only calls that may change
> a container's geometry (grow, shrink, reallocate; inferred per method)
> conflict with a live view; a method that only writes fields or elements
> through `&mut self` passes freely. Hover a function to see both
> inferred effects (`borrows`, `bumps`). Spec §6 has the precise rules.
>
> What decides whether you got a projection or a copy is the **return
> type**, never what the body happened to touch. `fun make(&self):
> (i32, i32) { self.pair }` returns a value — the caller's copy, so a
> later write to `h.pair` cannot reach it — and so does `fun grab(&self):
> Inner { &self.inner }`, one `&` away, because the return says `Inner`.
> Only `fun slot(&mut self): &mut T` hands back the alias, as it must.
> References are transparent in the type system, so inside the body
> `self` looks the same either way; the signature is the only thing that
> says which one you meant. Leave the return type off and it is inferred
> as a value type: an unannotated function always returns a value — the
> caller's copy — never a view, through its final expression and through
> every `ret` alike.

## `Shared<T>`: one cell, many holders

When two places need to see the same mutable state (a closure and its
creator, most commonly), reach for `Shared<T>`. It's a small
heap cell. Copying the `Shared` value copies the *handle*, and both
handles point at one cell. That's the point:

```vilan
import std::print;
import std::shared::Shared;

fun main() {
	let count = Shared::new(0);
	let bump = || {
		count.write() = count.read() + 1;
	};
	bump();
	bump();
	print(count.read()); // 2 — the closure and main share the cell
}
```

Two methods, two behaviors, one trap:

- `read()` gives you a **copy** of the contents.
- `write()` gives you a writable view of the contents, to assign to or
  mutate through, within the same statement.
- The trap: `shared.read().push(x)` mutates the copy and is lost. Write
  through the cell: `shared.write().push(x)`.

If you're reaching for `Shared` to "avoid a copy" on a hot path,
don't: values are cheap, and the compiler already elides copies it can
prove away.

## `Arena` + `Handle`: graphs and cycles

Values copy, and views can't be stored. So how do you build a tree with
parent pointers, or a graph where nodes reference each other? With an
**arena**: a container that owns all the nodes, handing you small
copyable `Handle` ids. Edges are handles, and a handle is a plain value
you can store anywhere:

```vilan,fragment
mut nodes: Arena<Node> = Arena::new();
let a: Handle = nodes.insert(Node { … });
let b: Handle = nodes.insert(Node { edges = [a] });
// `get` hands back a view; copy it out, add the edge, write it back.
match nodes.get(a) {
	Some(let node) => {
		mut updated = *node;
		updated.edges.push(b);      // a cycle, no aliasing
		nodes.set(a, updated);
	},
	None => {},
}
```

Deleting is safe too. Removing a node and reusing its slot bumps a
generation counter, so a stale handle comes back as `None` instead of
pointing at the new occupant. The full API is in the
[cells reference](../std/cells.md).

## The async rule

One rule ties this chapter to the [async model](async.md): **a view may
not be held across a suspension.** While your function is suspended,
other code runs and may change or replace whatever the view pointed
into. The compiler rejects the shape. The fix is always the same:
re-derive after the suspension.

```vilan,fragment
let row = &mut rows[i];
send(row.id);            // suspends —
row.text = "sent";       // ✗ rejected: view held across a suspension

send(rows[i].id);        // ✓ re-derive after the suspension
rows[i].text = "sent";
```

The question is whether the call **can suspend**, not whether you wrote
`await`. Calling an async function without the keyword is the sanctioned
spelling and it yields exactly the same way, so `send(row.id)` above is
caught with no `await` anywhere in it — and so is a sync-looking function
in between that reaches something async. The rule reads the declaration
of the function you *call*.

## Traps

- "Why didn't my mutation stick?" You mutated a copy. Either take `&mut`
  at the function boundary, or restructure around `Shared` or a signal's
  `update`.
- `shared.read().push(x)` is lost (it mutates a copy). Use
  `shared.write().push(x)`.
- Signals hold values too. Mutate a signal's list with
  `signal.update(|&mut list| …)` — which hands the closure a writable view
  of the stored value — never by mutating a `get()` result.
