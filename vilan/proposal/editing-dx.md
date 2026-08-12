# In-progress-change diagnostics — the DX survey (E49)

> Status: RATIFIED 2026-08-11 ("The recommendations for editing-dx.md … look
> good") — slices S1–S6 are free for lanes, S1 (the statement synchronizer,
> the blackout's fix) first among them. Filed from backlog E49, the owner's
> 2026-08-09 charter.
>
> Origin: backlog E49 (`backlog-2026-07-18.md:591-608`). The charter's premise:
> "code MID-EDIT is the diagnostic path's most common input, and several shapes
> serve it badly". It names five shapes — (a) a missing semicolon, (b) a missing
> opening or closing parenthesis, (c) mismatched call-argument count, (d)
> mismatched struct-initializer property count, (e) a missing return value, the
> one known-bad — and asks the survey to settle, per shape, **where** the
> diagnostic anchors, **what** it says, and **how the steer resolves it**,
> measured against what the LSP renders live and not only batch `vilan check`.
> The charter explicitly allows "already fine" as a verdict, with evidence.
>
> Every claim below about what the compiler does today was checked against
> source **and** run through the repo compiler as a probe. The probes are called
> out inline (P1…P31); they ran against `target/debug/vilan` built in this
> worktree from `next @868c109`, and against the language server driven directly
> through its own publish planner (§1.3). **They found that the five shapes are
> not the top of the list.** Above all five sits a sixth thing the charter did
> not name: while a file does not parse, the editor loses *every* diagnostic the
> file already had, and gains one that points at a line the user is not editing
> (§2). That is the mid-edit path's actual worst behavior, and it is measured
> keystroke by keystroke in P30.
>
> §13 is the open-questions set; everything before it is a recommendation, not a
> ratification. Two recommendations here are **re-verdicts of already-QUALIFIED
> ledger rows**, not bug fixes, and are argued as such (§7.5, §10.2).

## 0. The problem and the thesis

A compiler's diagnostics are written for finished code. Its users read them on
unfinished code. Every keystroke between `let total: i32 = distance(` and
`let total: i32 = distance(origin.x, origin.y);` is a state the compiler is
asked about, and the states in between outnumber the finished one by an order
of magnitude — a 150 ms debounce (`crates/vilan-lsp/src/main.rs:33`) means a
typing user asks the analyzer a question roughly twice a second, and almost
none of those questions are about a program that parses.

The survey probed all five named shapes in both instruments and graded them:

| Shape | Grade | One line |
|---|---|---|
| (a) missing semicolon | **BAD** | The parser cannot say `;` — the token is not in any expected-set — so it blames the *next* statement's first token and demands `}` (P2–P4). |
| (b) missing `)` | **FINE** in a committed list | Tight span on the offending token, correct expectation set: `found ';' expected ',' or ')'` (P8–P10). |
| (b) missing `(`, or a genuinely-unclosed delimiter | **BAD** | Openers are silent by design; an unclosed region defeats recovery entirely and the anchor lands on a distant `}` or on EOF (P11–P13). |
| (c) call-argument count | **FINE, with one flaw** | Right anchor, right house form; the span inflates to a multi-line rectangle when the list wraps, and the message names neither the callee nor which parameter (P15–P17). |
| (d) struct-initializer count | **BAD** | The count message names neither the struct nor the missing field; the misspelled-field message anchors on the field's **value**, not its name (P18–P20). |
| (e) missing return value | **BAD — three regimes, not one** | Named function: a **zero-width** anchor past the closing brace, invisible in an editor. Void-typed tail: a multi-line rectangle. Closure: the whole closure, or the whole *call* containing it (P21–P28). |
| — the blackout (unnamed by the charter) | **WORST** | While the file does not parse, the batch checker analyzes nothing at all, and the language server loses every diagnostic in the enclosing body — often in the whole file tail (P29–P31). |

**Thesis: the mid-edit path is not short of good messages. It is short of a
parser that keeps going, and of spans that point at the gap instead of at
whatever expression happened to carry the wrong type.** Those are two different
work streams — one in the H-section parser, one in the analyzer — and they
should be sliced apart (§11), because the parser half unblocks the other half's
value: a re-anchored return-value diagnostic that the user never sees, because
a missing `)` three lines up blacked the file out, is worth nothing.

The single most valuable slice is the one the project has already ratified and
never built: `frontend.md:137-140` specifies statement/item synchronization as
part of the H6 recovery design. It was not implemented. §10.1.

## 1. How the survey was run

### 1.1 The sample program

Every span in this paper is drawn over one program, so the ranges are
comparable. It compiles clean (**P1**: `vilan check base.vl` → `no errors`,
exit 0):

```vilan
import std::print;

struct Point { x: i32, y: i32 }

fun distance(x: i32, y: i32): i32 {
    x + y
}

fun main() {
    let origin: Point = Point { x = 3, y = 4 };
    let total: i32 = distance(origin.x, origin.y);
    print(total);
}
```

Shape (e) needs a closure, so it uses a second sample, likewise clean:

```vilan
import std::print;

struct Point { x: i32, y: i32 }

fun main() {
    mut points: List<Point> = List::new();
    points.push(Point { x = 1, y = 10 });
    let widths: List<i32> = points.map(|point| {
        point.x * 2
    });
    print(widths.len());
}
```

### 1.2 The batch instrument

`target/debug/vilan check <file>`, built in this worktree. It renders through
ariadne; the excerpts below are verbatim. Its line:column pairs are 1-based.

### 1.3 The live instrument — and why it is not `vilan check` twice

The charter asks for what the LSP *publishes*, not what the CLI *prints*, and
they are not the same program. `crates/vilan-lsp` is a **bin-only crate** — no
`[lib]` target, so there is no `tests/` directory and cannot be one; every test
in it is an inline `#[cfg(test)] mod`. The probes here were run the same way: a
throwaway `#[cfg(test)] mod` appended to `crates/vilan-lsp/src/publish.rs`,
deleted before commit.

The payload it captures is exact. `Backend::publish_document`
(`crates/vilan-lsp/src/main.rs:1221`) is a **pure transmitter**: it calls
`PublishState::plan_publish` (`publish.rs:80`) and loops
`client.publish_diagnostics(target, group, None)`. Calling `plan_publish`
directly therefore yields byte-identical `Vec<Diagnostic>` values to what goes
on the wire. (The alternative — driving `LspService` over real JSON-RPC — is
strictly worse for this purpose: `tower_lsp` suppresses every notification
until the service reaches `State::Initialized` through its *layers*, so calling
`backend.did_open(..)` on `service.inner()` publishes nothing at all. That is
presumably why no existing test in the crate reads the client socket.)

Two facts about the live path matter for reading the probes:

- **The two-snapshot law.** A `Document` keeps a *live* text/index and an
  *analyzed* text/index (`document.rs:756-812`). `did_change` advances only the
  live side (`apply_change`, `document.rs:1257`); `published_diagnostics()`
  reads the analyzed side (`document.rs:1108`). So during the 150 ms debounce
  the editor still shows the *previous* analysis, unchanged. Probes confirm
  this: the mid-keystroke stage of P30's predecessor published exactly what the
  settled stage did. This is good behavior and the survey has no complaint
  about it — but it means the interesting question is what the *next* analysis
  publishes, and that is what every probe below reports.
- **No suppression, no cap, no filtering.** `diagnostic_groups`
  (`publish.rs:217-233`) maps every item from `published_diagnostics()` to a
  `Diagnostic` with a severity and nothing else — `code`, `tags`, `data` are
  never set, there is no cap, and parse errors are not dropped. Whatever the
  compiler produces, the editor gets. The LSP is not the problem in any shape
  this survey found; it is a faithful window onto the compiler.

LSP ranges below are printed as `[line:char .. line:char]`, **0-based**, as the
protocol carries them — so a CLI `5:2` and an LSP `[4:1 .. 4:1]` are the same
place.

## 2. The finding that outranks the five shapes — the blackout

### 2.1 Keystroke by keystroke (P30)

**P30** — a file with one standing, correct type error, while the user types
`print(1);` on the line below it. Each stage is a full analysis of that exact
buffer, published through the planner:

```
fun main() {
    let wrong: i32 = "text";     ← the standing error
    <the user types here>
}
```

| Stage | Buffer's new line | What the editor shows |
|---|---|---|
| 0 | *(empty)* | `[3:21 .. 3:27] Expected i32, but got str instead.` |
| 1 | `p` | `[3:21 .. 3:27] Expected i32, but got str instead.`<br>`[4:4 .. 4:5] cannot find 'p' in this scope` |
| 2 | `print(` | `[5:0 .. 5:1] found '}' expected an expression` |
| 3 | `print(1` | `[5:0 .. 5:1] found '}' expected ',' or ')'` |
| 4 | `print(1)` | `[3:21 .. 3:27] Expected i32, but got str instead.` |
| 5 | `print(1);` | `[3:21 .. 3:27] Expected i32, but got str instead.` |

Read stages 2 and 3. The user's real error — the one they may well be in the
file to fix — **disappears from the editor**, and the diagnostic that replaces
it is anchored on the closing brace of the function, a line the user is not
editing and did not change. For two of the six states in an ordinary
nine-character typing burst, the editor is lying about the file.

Stage 1 is a smaller version of the same thing: `cannot find 'p' in this scope`
is true of a program nobody meant to write. That one is arguably unavoidable
(a lone `p` really is a complete, wrong program) and the survey does not
propose suppressing it; it is recorded because it is what makes stage 2's
regression visible — the editor went from *two* diagnostics, one of them
useful, to *one*, useless.

### 2.2 Three separate mechanisms produce it

The blackout is not one gate. It is three, and they need three different fixes.

**Mechanism 1 — the batch checker throws the tree away.** `vilan check` does
not analyze a file whose parse was not clean. The code says so in as many
words, `crates/vilan-cli/src/main.rs:2511-2521`:

```rust
// A batch compile does not analyze a file that failed to parse cleanly — its
// parse errors are reported and the build fails — so the freshly parsed tree
// is taken only when the parse produced no diagnostics.
let (tree, errors) = vilan_core::parsing::parse(src.as_str());
let clean = errors.is_empty();
parse_errors = errors;
tree.filter(|_| clean).map(|(mut items, span)| { … })
```

**P29** — a file with one parse error in `fun broken` and two genuine analyzer
errors in `fun main` reports **only the parse error**. Neither analyzer error
appears, in either order (a type error *before* the parse error is dropped just
the same). One missing `;` anywhere in the file blinds `vilan check` to
everything else in it.

This is a deliberate, defensible choice for `vilan build` — you cannot emit
from a recovered tree. It is not defensible for `vilan check`, whose whole job
is to answer questions about a file the user is still writing.

**Mechanism 2 — block recovery eats the enclosing body.** The LSP path does
*not* have mechanism 1: `analyze_source` (`crates/vilan-core/src/lib.rs:331-341`)
analyzes the salvaged tree and its comment says so explicitly ("Analysis below
runs on the salvaged tree, so a mid-edit source still yields a partial program
rather than nothing"). But the salvage is coarser than it sounds. When
`parse_block_clean` declines, `parse_block` recovers by *skipping the entire
balanced `{…}` region* (`parsing.rs:2066-2074`) and substituting an **empty**
block. Every statement in that function body ceases to exist, so every
diagnostic those statements would have produced ceases to exist with them.

That is exactly what P30 stage 2 shows: the standing `Expected i32, but got str
instead.` lived in the same body as the half-typed `print(`.

**Mechanism 3 — an unclosed delimiter defeats recovery, and takes the file tail
with it.** `recover_delimited` depends on `scan_balanced`
(`parsing.rs:816-865`), which returns `None` if the region never closes. A
half-typed `print(` is precisely a region that never closes. Recovery therefore
does not fire at all, the statement loop `break`s (`parsing.rs:2089` in a block,
`:912` at file scope), and the parse stops there — everything after is dropped.

**P31** measures the reach, at the wire:

| Probe | File shape | Published |
|---|---|---|
| A | type error in `fun one`, then unclosed `(` in `fun two` | **both** — the parse error, and the type error |
| B | unclosed `(` in `fun one`, then type error in `fun two` | **only the parse error** |
| C | type error and unclosed `(` in the *same* fun | **only the parse error** |
| D | type error in `main`, then `BROKEN nonsense` at file scope | **both** |

B is the shape a typing user is in constantly, and it is the one that loses the
most: everything *below* the cursor stops being checked. A and D show the
salvage working as designed — the parsed *prefix* survives, which is exactly
what `frontend.md:32-41` records as the deliberate KEEP over chumsky's
all-or-nothing behavior. The gap is not that salvage does not exist; it is that
salvage is prefix-only, because there is no synchronizer to restart it (§4.3).

### 2.3 Grade and recommendation

**Grade: the blackout is the worst thing in the mid-edit path, and it is not
one of the five named shapes.** No amount of span or message work on (a)–(e)
reaches a user who cannot see any diagnostic at all.

**Recommend**, in this order:

1. **Build the statement/item synchronizer** `frontend.md:137-140` already
   ratified (S1, §11). It fixes mechanism 3 by letting the parse restart at the
   next `;`/`}`/item keyword instead of stopping, which restores diagnostics for
   the whole file tail — the B row above.
2. **Make `scan_balanced` tolerant of a never-closing region** (S1): treat
   end-of-input, or the enclosing closer, as the region's end for recovery
   purposes. This narrows mechanism 2's blast radius from "the whole body" to
   "the one statement being typed".
3. **Let `vilan check` analyze the salvaged tree**, as the LSP already does
   (S6, §11) — mechanism 1. Scoped to `check`, never to `build`. Raised as
   §13.1 because it changes `check`'s output contract on broken files.

## 3. Shape (e) — the missing return value

The charter names this the one known-bad instance, and asks for it first and
deepest. The probes agree it is bad, and disagree with the report about *how*.

### 3.1 What the report says, and what the probes found

The charter: "it underlines the ENTIRE closure rather than the gap". **P21**
reproduces that, on the closure sample with its tail turned into a statement:

```
Error: Expected List<i32>, but got List<void> instead.
    ╭─[ e1.vl:8:29 ]
    │
  8 │ ╭─▶     let widths: List<i32> = points.map(|point| {
    ┆ ┆
 10 │ ├─▶     });
    │ │
    │ ╰───────────── Expected List<i32>, but got List<void> instead.
```

Live, the same file publishes `[7:28 .. 9:6]` — a three-line rectangle in the
editor.

Two corrections to the report, both in the direction of "worse":

- **It is not the closure. It is the whole *call*.** Column 29 on line 8 is the
  `p` of `points`, not the `|` of `|point|`. The span is
  `points.map(|point| { … })` — receiver, method name, argument list and all.
  The closure is a *proper subset* of what gets underlined.
- **There are three regimes, not one.** Depending on how the value goes
  missing, the same conceptual mistake produces three structurally different
  anchors: a zero-width point, a multi-line rectangle over a sub-expression,
  and a multi-line rectangle over an enclosing expression. §3.2–§3.4.

### 3.2 Regime 1 — the declared-return function: a zero-width anchor

**P22** — a named `fun` with a declared return type whose body ends in a
statement:

```
fun total(a: i32, b: i32): i32 {
    let sum: i32 = a + b;
}
```
```
Error: Expected i32, but got void instead.
   ╭─[ e2.vl:5:2 ]
   │
 5 │ }
   │  │
   │  ╰─ Expected i32, but got void instead.
```

An empty body (`fun total(…): i32 { }`) and a body ending in a void call
(`print(doubled);`) both land in the same place.

At the CLI this reads as tolerably good — it points at the closing brace, which
is roughly the gap. **Live, it is invisible.** The published range is:

```
e2. named fun, body ends in a let
  [4:1 .. 4:1] Expected i32, but got void instead.
```

`start == end`. A zero-width LSP range. `line_index.rs:81-87` converts start and
end independently with **no widening**, and `document.rs:1122-1174` publishes
`error.span` verbatim, so nothing downstream fixes it. VS Code draws a
caret-width marker for a zero-width range, not an underline. The regime the CLI
renders best is the regime the editor renders worst — and the editor is where
mid-edit code lives.

Worse, the point is one byte off from where the comment claims it is. It sits
*after* the `}`, not on it: the closing brace occupies characters 0..1 of line
4, and the diagnostic is at 1..1.

### 3.3 Regime 2 — a void-typed tail expression

**P25** — the body's tail *is* an expression; it just has type `void`:

```
fun classify(n: i32): str {
    if n > 0 {
        "positive"
    }
}
```
```
Error: Expected str, but got void instead.       (CLI 4:5 → 6:5)
  [3:4 .. 5:5] Expected str, but got void instead.   (LSP)
```

Three lines underlined. The `if` really is the expression whose type is wrong,
so this is A1-compliant on its face — but the *reason* it is `void` is one
missing `else`, which is a point, not a rectangle.

### 3.4 Regime 3 — the closure: the known-bad, and its root cause

Every closure spelling produces a whole-value type mismatch:

**P23** — bound to an annotated local, and passed as an argument to a
non-generic function:

```
let scale: |i32| i32 = |value| {
    let doubled: i32 = value * 2;
};
   → Expected |i32| i32, but got |i32| void instead.    span: the whole closure
```

**P24** — the one-line form, where the whole mistake is a single character:

```
let scale: |i32| i32 = |value| { value * 2; };
                       ───────────┬──────────
                                  ╰──────────── Expected |i32| i32, but got |i32| void instead.
```
Live: `[3:27 .. 3:49]` — **22 characters underlined to tell the user to delete
one**. The `;` at 3:46 is the entire fix, and it is not distinguished from the
21 characters around it that are correct.

**P26 — the finding that settles the root cause.** vilan's grammar *has* a
closure return-type annotation. Writing it changes nothing:

```
let scale: |i32| i32 = |value: i32|: i32 { print(value); };
                       ─────────────────┬─────────────────
                                        ╰─── Expected |i32| i32, but got |i32| void instead.
```

The closure declares `: i32`. A named function with the same declaration gets
regime 1's return-position check. The closure does not. `Closure::return_type`
(`crates/vilan-core/src/node.rs:150-154`) is read by the **formatter**
(`formatter.rs:3225`) and the generic AST visitor (`node.rs:713`) and by nothing
else. The analyzer's `Node::Closure` walk (`analyzer.rs:18498-18541`) reads
`closure.parameters` and `closure.return_value` and never touches
`return_type` — its own comment states the premise: "A closure is a `ret`
boundary with an INFERRED return type". An annotated closure return type is
parsed, re-printed, and completely ignored by type checking.

**P27 — the contrast that bounds the fix.** When the *parameter* type is what
differs, the whole-closure anchor is correct:

```
let scale: |i32| i32 = |value: str| { 1 };
                       ─────────┬────────
                                ╰────────── Expected |i32| i32, but got |str| i32 instead.
```
Nothing narrower would be honest here — the closure as written is the wrong
value. Any re-anchor must preserve this.

**P28 — a B5 violation in the family.** A bare `ret` in a value-returning
function reports the same root cause twice:

```
fun total(a: i32): i32 {
    ret;
}
```
```
  [3:4 .. 3:7] Expected i32, but got void instead.     ← the ret
  [4:1 .. 4:1] Expected i32, but got void instead.     ← the synthesized tail
```
One mistake, two identical messages, one of them zero-width. `diagnostics-standard.md:57`
B5 — "One diagnostic per root cause… A second diagnostic must add information,
not repetition" — is not satisfied.

### 3.5 Where the span is chosen, and why it widens

The dissection has two halves, one in the parser and one in the analyzer.

**The parser half — where `void` and its span are manufactured.**
`parse_block_clean`, `crates/vilan-core/src/parsing.rs:2096-2102`:

```rust
self.expect_ctrl('}')?;
let span = self.span_from(start);
let tail = tail
    .map(Box::new)
    .unwrap_or_else(|| Box::new((Node::Void, (span.end..span.end).into())));
```

A block with no trailing expression synthesizes a `Node::Void` at
`(span.end..span.end)` — zero width, and because `span_from`
(`parsing.rs:590-603`) ends at the *end of the last consumed token* and `}` has
already been consumed, that point is immediately **past** the brace. The
recovery twin at `parsing.rs:2070-2074` does the same. The doc comment says
"the value is `void` at the closing brace" (`parsing.rs:2061-2065`), pinned as
`empty_block_value_is_void_at_the_closing_brace` (`parsing.rs:4470-4476`) — the
pin asserts the value, not the offset, and the offset is one byte off from the
prose.

That single line is the whole of regime 1's anchor.

**The analyzer half — why a closure never reaches that path.** The rule is
`ret-checking.md`'s, ratified 2026-07-04:

> **Rule 2.** In a function with a declared return type `R`: the tail and every
> `ret v` check `typeof(v)` against `R` through the same constraint.
>
> **Rule 4.** In closures and `async` blocks … their return types are
> *inferred*, so a closure's `ret`s collect on its frame and check against the
> inferred tail type once it resolves (`Constraint::ClosureReturns`).

Rule 2 is regime 1: `resolve_return_type` (`analyzer.rs:24403-24430`) anchors at
`span_map[body_id]`, where `body_id` is the **tail expression**
(`analyzer.rs:17566`, `:17595-17604`) — i.e. the synthesized zero-width `Void`.
That anchor is already as narrow as the parser makes available.

Rule 4 is regime 3, and it is the root cause. A closure has **no declared
return type to check the body against** — so there is no return-position
constraint at all. What happens instead: `infer_type_path`'s `Expr::Closure`
arm (`analyzer.rs:21261-21314`) types the closure as
`Type::Closure(params, typeof(body))`, the body's type is its tail's type, the
tail is `void`, and the closure's type becomes `|i32| void`. The mismatch then
surfaces wherever that *value* is used, through an ordinary value-position
check, each of which anchors at the argument or value expression's span:

| Site | Route |
|---|---|
| `analyzer.rs:24312` (`resolve_variable`, first value) | `let f: \|i32\| i32 = \|x\| { … ; };` (P23) |
| `analyzer.rs:23431` (plain `fun` call arguments) | `apply(\|x\| { … ; }, 3)` (P23) |
| `analyzer.rs:24186` (`resolve_method_arg_check`) | `points.map(\|x\| { … ; })` — but see below |
| `analyzer.rs:25742` (`resolve_struct_initializer`) | a closure-typed field's value |

The `map` case (P21) does not even stop there: the closure's `void` return
propagates through the generic binding into `List<void>`, and the mismatch is
detected one level *out*, on the initializer, so the anchor is the whole call.
That is why the underline is wider than the closure the charter reported.

Regime 2 sits between them: the tail *is* a real expression, so `span_map` has
its real span, and A1 is satisfied by construction.

**Summary of the dissection.** The span does not "widen to the closure". The
closure is simply the innermost expression whose *type* is wrong, and the
compiler has no notion that a missing return value is a distinct kind of
mistake with a distinct location. Regime 1 has that notion (rule 2) and its
anchor is correspondingly tight. Regime 3 does not (rule 4), so it falls back
on the generic value-mismatch machinery.

### 3.6 What the right anchor is

The charter asks: the closing brace? the last expression? the signature's
return type?

**Recommend: the closing brace, width one, plus a C3 note at the return-type
annotation.** Reasoning, against the alternatives:

- **Not the signature's return type.** `diagnostics-standard.md:37-38` A4 ends
  with "annotation conflict → the value (the annotation is the contract, the
  value broke it)". The declared `: i32` is right; the body is wrong. Anchoring
  primary at the annotation inverts that. It is the correct place for the
  *secondary* note (C3), and that is what §3.7 proposes.
- **Not the last expression.** In P24 the last expression (`value * 2`) is
  correct — it is the `;` after it that is wrong. In P22 there is no last
  expression at all. The last expression is the right anchor in neither.
- **Not the zero-width point past `}`.** It renders as nothing in an editor
  (§3.2), and it is not even on the brace.
- **The closing brace itself** — `block_span.end - 1 .. block_span.end`, exact
  because `}` is one ASCII byte — is the one location that exists in all three
  regimes, is always one character wide, is always visible, and is always
  adjacent to where the value would go. A user who sees it squiggled and reads
  "this body must produce an `i32`" knows both what is wrong and where to type.

For regime 2, the same anchor applies with one refinement: an `if` with no
`else` in tail position should anchor at its closing brace too, with the note
naming the missing `else` (§3.7) — it is a distinguishable sub-case and the
steer differs.

Drawn on the sample, today versus recommended:

```
    fun total(a: i32, b: i32): i32 {
                               ───   ← proposed C3 note ("declared here")
        let sum: i32 = a + b;
    }
    ^                                ← proposed primary span (width 1)
     ^                               ← today's span (width 0, past the brace)
```

```
    let scale: |i32| i32 = |value| { value * 2; };
               ─────────                                ← proposed C3 note
                           ─────────────────────        ← today's span (22 chars)
                                              ^         ← proposed primary span
```

The second drawing is the one that shows the change's value. Today the user is
told the whole closure is wrong. The proposal points at one character and tells
them to remove it.

### 3.7 What it says

The house form is mandatory (`diagnostics-standard.md:45-47` B2: "Expected X,
but got Y instead." — a diagnostic that names only one side fails the rule), and
B3 requires naming an inferred decision the user never wrote. `void` is exactly
such a decision. Recommended messages, per regime:

**Regime 1 — declared return type, no tail expression:**

```
Expected i32, but got void instead: this body ends without producing a value.
  note (at the return type): `i32` declared here
```

**Regime 1′ — the last statement is an expression with a trailing `;`** (P24's
shape; detectable because the block has statements and its last statement is an
`Expr` statement whose type reconciles with the expectation):

```
Expected i32, but got void instead: the `;` discards this body's last value.
  note (at the return type): `i32` declared here
```

**Regime 2 — an `if` with no `else` in tail position:**

```
Expected str, but got void instead: an `if` with no `else` produces void.
  note (at the return type): `str` declared here
```

**Regime 3 — a closure whose expected return type is known:** the same three
messages, with the note anchored at the closure's own return-type annotation
when it has one, and at the *expected type*'s written form when it does not
(the `|i32| i32` in `let scale: |i32| i32 = …`, which is what the user wrote and
what the compiler is holding them to).

Note what is *not* proposed: no message says "closure" or "function" or names
the callable. The anchor already names it — the squiggle is on its brace.

### 3.8 The resolution steer

`diagnostics-standard.md:52-55` B4: one concrete action, code-shaped when short,
and no steer is better than a speculative one. Per regime, what the user types
next:

| Regime | Steer | Rationale |
|---|---|---|
| 1 (no tail) | none | The value to return is not knowable; a speculative steer would be guessing. The anchor and message are the whole help. |
| 1′ (trailing `;`) | **"remove the `;`"** — and this one is unambiguous enough to *want* to be a quick fix | See §3.8.1: the server has no quick-fix surface today, so this ships as message text now and as an action later. |
| 2 (`if` with no `else`) | **"add an `else` branch"** | Unambiguous: the construct is void precisely because a branch is missing. |
| 3 | as regime 1/1′ | The closure case reduces to the function case once the return-position check exists. |

Regime 1′ is the single highest-value item in shape (e): it turns the most
common spelling of the mistake from a 22-character underline into a
one-character one, with a message that names the character.

#### 3.8.1 There is no quick-fix surface — an unbudgeted prerequisite

Two of this survey's best steers (remove the `;` here, insert the `;` in §4.4)
want to be `CodeActionKind::QUICKFIX` actions rather than prose. **They cannot
be, today.** The server advertises exactly one code-action kind
(`main.rs:1553-1558`):

```rust
code_action_provider: Some(CodeActionProviderCapability::Options(
    CodeActionOptions {
        code_action_kinds: Some(vec![CodeActionKind::SOURCE_ORGANIZE_IMPORTS]),
        ..Default::default()
    },
)),
```

and the handler short-circuits anything else (`main.rs:2124-2128`): "The only
source action we offer is Organize Imports; skip the work entirely when the
client asked for a different kind (e.g. quickfix)" → `Ok(None)`.

So a quick fix is not a free rider on either slice. It needs the capability
widened, a diagnostic-to-action router (match a published `Diagnostic` by range
and produce a `TextEdit`), and — because a `Diagnostic`'s `code` field is
**never set** today (`publish.rs:227-233`) — a stable machine-readable code on
the diagnostics that carry fixes, so the router matches on identity rather than
on message text.

**Recommend: file the quick-fix surface as its own item, not inside E49.** The
message-level steers in §3.7 and §4.4 stand on their own and are what S2/S3
ship; the actions are a strictly-later multiplier on them. Raised as §13.7.

### 3.9 The implementation shape, and what it costs

The fix is **not** a span tweak at four `diagnostics.push` sites. It is one
structural change: **when a closure's expected type is known and only its
return type differs, check the closure's body in return position — the same
`Constraint::ReturnType` path rule 2 gives a named function — instead of
comparing the closure value's type as a whole.** Fall through to today's
value-position comparison when the parameter types differ (P27), which keeps
that case's correct anchor.

Two supporting changes:

- The synthesized `Node::Void` should carry the **closing brace's** span
  (`span.end - 1 .. span.end`) rather than a zero-width point past it
  (`parsing.rs:2101` and the recovery twin at `:2072`). This is a one-line
  change that fixes regime 1's invisibility everywhere at once, including the
  P28 duplicate.
- `Closure::return_type` should be read by the analyzer, so an annotated
  closure gets rule 2 directly. Today it is dead weight (§3.4).

`Span::to_end()` (`span.rs:41-47`) — "a zero-width span at this span's end" —
**exists and has zero callers anywhere in the tree.** It is the helper this
family would want if zero-width anchors were the answer. They are not (§3.2), so
the recommendation is to leave it unused, or delete it in the same slice.

### 3.10 Grade

**(e): BAD.** Three regimes, none of which anchors where the fix goes; one of
them invisible in an editor; a declared closure return type ignored entirely;
and a duplicate diagnostic in the `ret` case. This is the survey's largest
analyzer-side finding.

## 4. Shape (a) — the missing semicolon

### 4.1 Today

**P2** — the sample program, `;` removed from the end of line 10:

```
Error: found 'let' expected '}'
    ╭─[ a1.vl:11:5 ]
    │
 11 │     let total: i32 = distance(origin.x, origin.y);
    │     ─┬─
    │      ╰─── found 'let' expected '}'
```
Live: `[10:4 .. 10:7] found 'let' expected '}'`.

Three things are wrong and they compound:

1. **The location is the next line.** The gap is at the end of line 10. The
   squiggle is on line 11, on a statement that is entirely correct.
2. **The expectation is `}`.** The parser genuinely wants `}` there — but the
   user does not want to close the function, they want to end a statement.
3. **The word `;` does not appear.** Nowhere in the message.

**P4** — the same shape after a call statement: `found 'print' expected '}'`,
anchored on the next statement's `print`.

**P3** — after an `import`, at file scope, it is worse still:

```
import std::print          ← the `;` is missing here
```
```
Error: found 'import' expected an expression
   ╭─[ a2.vl:1:1 ]
   │
 1 │ import std::print
   │ ───┬──
   │    ╰──── found 'import' expected an expression
```

The squiggle is on the `import` keyword itself, and the message asks for an
expression. Neither is comprehensible.

**P5** — one case where nothing is reported at all: a missing `;` on the **last**
statement of a **void** body is silent, because the statement legally becomes
the block's tail expression and a void tail in a void function is fine
(`ret-checking.md` rule 3: "In a function with no declared return type (void):
nothing is checked"). `vilan check` → `no errors`. This is correct language
semantics and the survey proposes no change; it is recorded so the eventual
pins do not assume a diagnostic exists here.

### 4.2 Why the message can never say `;` today

`expect_ctrl`, `crates/vilan-core/src/parsing.rs:516-525`:

```rust
fn expect_ctrl(&mut self, character: char) -> Option<()> {
    if self.eat_ctrl(character) {
        Some(())
    } else {
        if matches!(character, ')' | ']' | '}' | '>') {
            self.note_expected(&format!("'{character}'"));
        }
        None
    }
}
```

`;` is not in that set, so **every** demand for a semicolon is silent — and the
statement terminator is not even an `expect_ctrl`: it is a bare `eat_ctrl(';')`
at `parsing.rs:962` (top-level and block statements) and at `:1678`, `:2974`,
`:3241`, `:3766`, `:3795`.

With nothing recording `;`, the "farthest failure" heuristic falls back on the
deepest expectation anyone *did* record. Inside a body that is
`parse_block_clean`'s own `self.expect_ctrl('}')` (`parsing.rs:2097`) — hence
`expected '}'`. At file scope there is no note at all, so
`emit_leftover_error` uses its hard-coded fallback (`parsing.rs:687-691`,
rendering as `found 'X' expected an item or end of input`) — and in P3's case
even that is displaced, because the import's own parse backtracked.

Worth recording separately: the string `"expected an item"` appears in **zero
tests anywhere in the repo**. That fallback path — the one every file-scope
missing semicolon lands on — is entirely unpinned.

The silence is deliberate and defended. `frontend.md:24-31` records the S5
review's ratified rule:

> **closing delimiters and committed operators note their expectations;
> speculative opener head-checks stay silent** (blanket noting re-imports the
> expected-dump noise).

That rule is right, and the recommendation below does not overturn it. The
statement terminator is not a speculative opener head-check — by the time the
parser has parsed a complete statement and is looking for `;`, it is fully
committed. `;` belongs on the *committed* side of the rule the review drew; it
was simply never put there.

### 4.3 The cascade — measured

The charter asks how many spurious diagnostics one missing token produces. The
answer is a pleasant surprise.

| Probe | File | Diagnostics |
|---|---|---|
| **P6** | one missing `;`, four correct statements after it | **1** |
| **P7a** | two missing `;`, in two different function bodies | **2** |
| **P7b** | three missing `;`, in three different function bodies | **3** |
| **P14** | one unclosed `(`, three correct statements after it | **1** |

**There is no cascade.** One missing token produces exactly one diagnostic, and
N independent errors in N separate function bodies produce exactly N. That is
already the bar the charter asks the survey to design, and today's parser meets
it.

The mechanism, and its price: recovery is entirely **balanced-delimiter-region
skipping** at ten fixed sites, via `recover_delimited` (`parsing.rs:769-807`).
When a body fails to parse cleanly, the whole `{…}` region is skipped and *one*
error is surfaced from inside it. There is no token-skipping synchronizer
anywhere — no `synchronize`, no `skip_until`, no `expect_or_recover`. The
statement loops simply `break` on the first statement that declines
(`parsing.rs:912` at file scope, `:2089` in a block).

So the count of 1 is not the product of good recovery. It is the product of the
parser **stopping** — and that is the same mechanism that produces §2's
blackout. The recovery bar is met by the wrong means.

### 4.4 Recommendation

**Where it anchors: the gap — a zero-width span at the end of the previous
token, rendered as a one-character caret at that position.** This is the one
place in the survey where a zero-width anchor is right, because there is
genuinely nothing there yet; the recommended rendering is the character
*before* the gap (the `}` of the struct literal in P2), one character wide, so
the editor has something to draw.

`Span::to_end()` (`span.rs:41-47`) is exactly this helper and has zero callers.
`emit_failure` (`parsing.rs:700-710`) unconditionally takes `token_span(position)`
— the *found* token's span — so the change is: when the expectation set contains
a statement terminator, anchor at the previous token's end rather than the found
token's start.

**What it says:**

```
expected `;` to end this statement
```

and, at file scope, the same. Both replace a message that names the wrong token
and the wrong expectation.

**The steer:** none needed — the message *is* the steer, and `;` is short
enough to be code-shaped in place, per B4. It is also the natural quick fix
(insert `;` at a known offset), which the server cannot serve today — §3.8.1.

Drawn on the sample:

```
        let origin: Point = Point { x = 3, y = 4 }
                                                 ^     ← proposed span
        let total: i32 = distance(origin.x, origin.y);
        ───                                              ← today's span
```

### 4.5 Grade

**(a): BAD.** Right *class* of construct identified, wrong line, wrong
expectation, and the word `;` is structurally unable to appear. The cascade
behavior is fine (1 per error), but by the wrong mechanism.

## 5. Shape (b) — the missing parenthesis

This shape splits cleanly into a part that is already good and a part that is
not.

### 5.1 A missing closing `)` inside a committed list — already fine

Three positions probed, all on the sample program.

**P8** — a call's argument list:

```
    let total: i32 = distance(1, 2;
                                  ┬
                                  ╰── found ';' expected ',' or ')'
```

**P9** — a parenthesized condition:

```
    if (n > 0 {
              ┬
              ╰── found '{' expected ')'
```

**P10** — a parameter list:

```
fun distance(x: i32, y: i32: i32 {
                           ┬
                           ╰── found ':' expected ',' or ')'
```

All three: **one diagnostic, one token wide, on the exact character where the
parser's expectation broke, naming the right expectation set.** `,` appears
alongside `)` in the list cases because `comma_list` notes it
(`parsing.rs:893`), which is correct — either would fix it.

**These are the best diagnostics in the survey.** Grade: **FINE.** The survey
recommends no change. They work because a closing delimiter *is* in
`expect_ctrl`'s noting set (`parsing.rs:520`) — which is precisely the fix
§4.4 asks for on `;`, already shipped for `)`. The existing pins
(`parsing.rs:5175-5215`) cover the parameter-comma and list-separator forms.

### 5.2 A missing *opening* `(` — silent by design, and bad

**P11**:

```
    print 1);
          ┬
          ╰── found '1' expected '}'
```

`(` is never noted (`parsing.rs:520`, rationale at `:506-515`). The message that
comes out is the block's fallback again — same failure mode as (a), same cause.

Unlike `;`, this one is genuinely contestable. `frontend.md:24-31`'s rule
("speculative opener head-checks stay silent") was drawn to stop blanket noting
from re-importing chumsky's expected-dump noise, and an opener check often *is*
speculative — the parser is choosing between alternatives and `(` is only one
of them. **Recommend: leave `(` silent, and let §4.4's `;` work carry this
case.** With a statement terminator in the expected set, `print 1);` reports
`expected ';' to end this statement` at the gap after `print` — which is wrong
about the user's intent but right about the parser's state, and strictly better
than `expected '}'`. Anything more requires guessing, and B4 says no steer beats
a speculative one. Recorded as §13.2 for the owner.

### 5.3 A genuinely-unclosed delimiter — the mid-edit case, and the worst one

This is the shape a typing user is actually in: the opener is typed, the closer
is not yet.

**P12** — `print(1` with the block's `}` on the next line:

```
 5 │ }
   │ ┬
   │ ╰── found '}' expected ',' or ')'
```

The anchor is a brace on a line the user has not touched.

**P13** — an unclosed `{`:

```
 8 │ }
   │   │
   │   ╰─ found end of input expected an expression
```

The anchor is **end of file**. The unclosed `{` is five lines up and is never
mentioned.

Both are downstream of the same mechanism: `scan_balanced` (`parsing.rs:816-865`)
returns `None` when the region never closes, so `recover_delimited` never fires,
no `Unbalanced` error is produced, and what surfaces is whichever
farthest-failure the enclosing statement recorded before giving up. The
`unclosed \`{\` in struct body: expected a matching \`}\`` message
(`parsing.rs:141-145`) — which is the *right* message for this case — fires
only when the region **did** close and merely declined at its first token. It
is unreachable in the case that needs it.

**Recommend:**

- **`scan_balanced` gains an unclosed mode**: when the region does not close
  before end-of-input or before the enclosing closer, treat that boundary as the
  region's end. This makes `recover_delimited` fire, which makes the
  `unclosed \`(\`` message reachable, and (per §2.2 mechanism 3) restores
  diagnostics for the rest of the file.
- **The anchor is the *opener*, not the found token.** `unclosed \`(\`` pointing
  at EOF helps nobody; pointing at the `(` the user typed is the whole content
  of the message. `recover_delimited` already has `start` in hand
  (`parsing.rs:775`).
- **What it says**, replacing P12 and P13 respectively:
  ```
  unclosed `(`: expected a matching `)`
  unclosed `{`: expected a matching `}`
  ```
  This is the *existing* message shape (`parsing.rs:141-145`), re-anchored and
  made reachable — not a new one.

Drawn:

```
        print(1
             ^          ← proposed span (the opener)
    }
    ^                   ← today's span
```

**The steer:** none. The message names the delimiter and its match; what the
user types next is `)`, and saying so adds nothing the message has not already
said. Per B4, no steer beats a redundant one.

### 5.4 Grade

**(b): SPLIT.** A missing closing `)` in a committed list is **FINE** and is the
model the rest of the parser's diagnostics should be measured against. A
missing opening `(` is **BAD** but is best fixed as a side effect of (a). A
genuinely-unclosed delimiter — the defining mid-edit shape — is **BAD**, anchors
at EOF or at an untouched brace, and is the direct cause of §2's worst blackout
row.

## 6. Shape (c) — call-argument count

### 6.1 Today

**P15** — three counts, one call:

```
    print(distance(3));            → Expected 2 arguments, but got 1 instead.   span: `3`, i.e. the argument list
    print(distance(3, 4, 5));      → Expected 2 arguments, but got 3 instead.   span: `3, 4, 5`
    print(distance());             → Expected 2 arguments, but got 0 instead.   span: `()`
```

**P16** — a method call behaves identically: `origin.shift(1)` →
`Expected 2 arguments, but got 1 instead.`, anchored on `1`.

**P17** — the flaw. When the argument list wraps, the span wraps with it:

```
    print(distance(
        3,
    ));
```
```
  [5:18 .. 7:5] Expected 2 arguments, but got 1 instead.
```

A three-line rectangle for a count mismatch. The formatter splits argument
lists routinely (`signature-layout.md`, `composite-spanning-split.md`), so this
is not an exotic case.

### 6.2 Assessment

The anchor is right in principle and is already ratified as such:
`diagnostics-ledger.md:112,114,117,155` verdict these rows
**"QUALIFIES — arity anchors at the arguments (they ARE the problem)"**, and
`diagnostics-standard.md:14-17` names call-argument mismatches as *the model*
for the whole anchoring standard. The message satisfies B2 (both sides
rendered). The survey does not propose moving the anchor.

Two things it does propose:

- **Name the callee.** `Expected 2 arguments, but got 1 instead.` does not say
  *whose* two arguments. In a chained or nested expression — `print(distance(3))`
  has two calls on one line — the reader has to work out which. The callee's
  name is in hand at every site (`method_signature` at `analyzer.rs:24128`,
  the resolved subject at `:23431`), and `member_name_spans`
  (`analyzer.rs:1896`) already exists for exactly this kind of narrowing, used
  today to anchor `cannot call method '{}' on {}` at the method name.
- **Name what is missing.** For the too-few case the parameter names are in
  hand. `distance` takes `(x: i32, y: i32)`; the user supplied `x`. The message
  can say so.

Recommended message, too few:

```
`distance` expects 2 arguments, but got 1 instead: `y: i32` is missing.
```

too many:

```
`distance` expects 2 arguments, but got 3 instead.
```

The asymmetry is deliberate: with too many arguments there is no principled way
to say *which* is extra (the user may have meant to replace one), and B4 forbids
a speculative steer. With too few, the missing parameter is unambiguous.

**Span, when the list wraps:** clamp to the first line of the argument list —
`arguments_span.start .. min(arguments_span.end, end_of_that_line)`. A count is
a property of the list as a whole; a rectangle spanning a screen is not more
informative than its first line, and is considerably more disruptive. Recorded
as §13.3 because it is a rendering-policy question that would apply to other
multi-line spans too (regime 2 of shape (e) has the same shape of problem).

**The steer:** none. The message names the missing parameter and its type;
what to pass is the user's business.

### 6.3 Grade

**(c): FINE, with one flaw.** The anchor is correct and already ratified; the
house form is satisfied; the cascade is clean (one diagnostic, and
`Resolution::Failed` stops the argument type-checks that would otherwise pile on
— B5 satisfied). The flaw is span inflation on wrapped lists, plus a message
that could name its subject and does not.

## 7. Shape (d) — struct-initializer properties

### 7.1 Count mismatches

**P18**:

```
    let origin: Point = Point { x = 3 };
                              ────┬────
                                  ╰────── Expected 2 fields, but got 1 instead.

    let origin: Point = Point { x = 3, y = 4, z = 5 };
                              ───────────┬───────────
                                         ╰───────────── Expected 2 fields, but got 3 instead.
```

Live: `[9:30 .. 9:39] Expected 2 fields, but got 1 instead.`

The span is the `{ … }` region (`constraint.fields_span`,
`analyzer.rs:25689`). One diagnostic per literal.

The message is where this shape falls down, and it falls further than (c) does.
`Expected 2 fields, but got 1 instead.` names neither the struct nor the
missing field. The reader is told a number and left to go look up `Point`. For
a struct with eight fields and seven supplied, the message is close to useless.

Both pieces of information are in hand at the site: `struct_fields` is cloned
into scope at `analyzer.rs:25687` and `constraint.fields` holds what was
written. The set difference is two lines of code.

**Recommend:**

```
`Point` expects 2 fields, but got 1 instead: `y` is missing.
```
```
`Point` expects 2 fields, but got 3 instead: `z` is not a field of `Point`.
```

Note the second form: an *extra* field in a struct literal, unlike an extra
call argument, **is** identifiable — struct fields are named. So both directions
get a steer here where (c) only gets one.

Anchor: **unchanged** for the missing case (the literal's brace region is the
right place — the gap is inside it and has no narrower home), and **moved to
the offending field** for the extra case, which is a specific identifiable
token:

```
    let origin: Point = Point { x = 3, y = 4, z = 5 };
                              ───────────────────────    ← today's span, both directions
                                              ─          ← proposed span, EXTRA field only
```

**The steer:** the message carries it. For the missing case, naming `y` tells
the user exactly what to type; for the extra case, naming `z` tells them exactly
what to delete. No separate steer clause is warranted, and B4 forbids inventing
one.

### 7.2 The misspelled field — a real span bug

**P19**:

```
    let origin: Point = Point { x = 3, yy = 4 };
                                            ┬
                                            ╰── struct 'Point' has no field 'yy'
```

Look at where the caret is. Column 45 is `4` — **the value**. The message is
about `yy`, which is at columns 40–41, and `yy` is not underlined.

**P20** confirms it is not a rendering artifact. Widen the value and the
underline widens with it:

```
    let origin: Point = Point { x = 3, yy = 40000 };
                                            ──┬──
                                              ╰──── struct 'Point' has no field 'yy'
```
Live: `[5:44 .. 5:49]` — five characters, exactly `40000`.

The diagnostic underlines a perfectly correct integer literal and tells the user
about a field name three columns to its left.

The cause, `analyzer.rs:25707-25720`:

```rust
for (field_name, field_value, field_value_span) in &constraint.fields {
    let field = struct_fields.iter().enumerate()
        .find(|(_, field)| *field.name == **field_name);
    let (struct_field_index, struct_field) = match field {
        Some(field) => field,
        None => {
            self.diagnostics.push(Error {
                note: None,
                span: *field_value_span,
                msg: format!("struct '{}' has no field '{}'", struct_name, field_name),
            });
```

`constraint.fields` is a `(field_name, field_value, field_value_span)` triple.
**There is no field-*name* span recorded** — so `field_value_span` is the only
span available, and it is used whether or not the value is what is wrong.

This is a genuine A1 violation, not a debatable one:
`diagnostics-standard.md:21-24` — "the span covers the smallest expression that
identifies the problem". The problem is `yy`. The value is not implicated at
all.

```
    let origin: Point = Point { x = 3, yy = 40000 };
                                       ──              ← proposed span (the name)
                                            ─────      ← today's span (the value)
```

**Recommend:** thread a `field_name_span` into the struct-initializer
constraint, and anchor this diagnostic there. It is a mechanical change: the
parser has the name's span at the point it builds the field list, and the value
span already rides alongside for the *type*-mismatch case, which correctly keeps
it (A4: "struct-field mismatch → the field's value" — that rule is about a
field whose *value* has the wrong type, and stays as it is).

**And say more.** vilan has essentially no did-you-mean machinery — one
occurrence exists in the entire tree (`analyzer.rs:11071`, the
field-vs-method-call steer). A misspelled field is the canonical case for it,
and the candidate set is tiny and in hand:

```
struct `Point` has no field `yy` — did you mean `y`?
```

Guarded by an edit-distance threshold so it stays inside B4 ("no steer is
better than a speculative one"): suggest only on a single closest match within
distance 2. With no close match, the message stops after the first clause.

Recorded as §13.4: a general did-you-mean helper is worth more than this one
site (unknown method names, unknown imports, unknown variants all want it), and
is arguably its own item rather than part of E49.

### 7.3 What the misspelling *also* hides

In P19 the struct has two fields and the literal supplies two, so the count
check passes and only the name error fires. The user is not told that `y` is
missing — which it is. This is defensible under B5 (one root cause: the
misspelling), and the recommended did-you-mean makes the connection explicit
without a second diagnostic. No change proposed beyond §7.2.

### 7.4 Grade

**(d): BAD.** The count message names neither the struct nor the field, when
both are trivially in hand; the misspelled-field diagnostic underlines the
wrong token outright. This shape has the cheapest fixes in the survey and some
of the largest per-line gains.

### 7.5 A note on what changing (c) and (d) costs

The `Expected {} {}, but got {} instead.` family is **already verdicted
QUALIFIES** in the living ledger, on the arity rows
(`diagnostics-ledger.md:112,114,117,155`) and the value-anchored rows
(`:115,116,125,126,129,130,157`). Rewording them is therefore a **re-verdict**,
not a bug fix, and per the standing rule (`diagnostics-standard.md:117-122`) the
ledger owes updated rows in the same commit. Only §7.2's misspelled-field span
is a straightforward A1 violation; everything else in §6 and §7 is a
deliberate improvement over something already judged acceptable, and should be
argued that way rather than filed as a defect.

## 8. The recovery bar

The charter asks the survey to design the bar. Measured (§4.3), today's parser
already meets the obvious formulation, so the bar must be written to capture
what today's parser gets *wrong* rather than what it gets right.

**Recommended bar, three clauses:**

1. **One missing token produces exactly one diagnostic.** Met today (P6, P14).
   Pin it so it stays met — nothing pins it now (`parser_recovery.rs:19-22`
   states outright that "the pins assert 'at least one error', never an exact
   count", and no test anywhere pins a missing semicolon or a missing paren).
2. **N independent errors produce N diagnostics — including when they are in the
   same body, and including when the first one is an unclosed delimiter.**
   *Not* met today. P7 passes only because the errors are in separate function
   bodies; §2.2's mechanism 3 shows an unclosed delimiter drops everything after
   it in the file.
3. **A parse error never removes a diagnostic from a region it does not
   contain.** *Not* met today. This is §2's blackout, stated as a rule. It is
   the clause that gives the statement synchronizer its acceptance test:
   analysis-level diagnostics in a sibling statement, a sibling function, and
   the file tail all survive one broken statement.

Clauses 2 and 3 are what S1 buys. Clause 1 is what S1 must not break — and
because it is unpinned, S1 owes it a pin before it starts.

## 9. The cascade counts, collected

Every count below is from the batch instrument on this worktree's binary; the
live instrument agrees on every one (P31 spot-checks four of them).

| Probe | Input | Diagnostics | Verdict |
|---|---|---|---|
| P6 | 1 missing `;`, 4 statements after | 1 | meets the bar |
| P7a | 2 missing `;`, 2 function bodies | 2 | meets the bar |
| P7b | 3 missing `;`, 3 function bodies | 3 | meets the bar |
| P14 | 1 unclosed `(`, 3 statements after | 1 | meets the bar by stopping (§4.3) |
| P12 | 1 unclosed `(`, block closer on the next line | 1 | meets the count, wrong anchor |
| P13 | 1 unclosed `{` | 1 | meets the count, anchor at EOF |
| P29 | 1 parse error + 2 analyzer errors, batch | 1 | **fails clause 3** — the 2 are erased |
| P31-B | 1 unclosed `(` in `fun one`, type error in `fun two` | 1 | **fails clause 3** — the tail is erased |
| P31-A | type error in `fun one`, 1 unclosed `(` in `fun two` | 2 | prefix salvage works |
| P28 | 1 bare `ret` in a value-returning fun | 2 | **fails B5** — same message twice |

**There is no cascade of spurious diagnostics anywhere in the survey.** The
failure mode is the exact opposite: diagnostics *disappear*. That is worth
stating plainly, because "one missing semicolon produces N spurious errors" is
the failure this bar is usually written against, and it is not vilan's.

## 10. Record-versus-compiler drift

Four places where a ratified paper and the compiler disagree. The first is
load-bearing for this whole survey.

### 10.1 `frontend.md` specifies a statement synchronizer that was never built

`frontend.md:137-140`, in the ratified H6 architecture:

> recovery is explicit sync points: delimiter matching reproduces the 10
> `nested_delimiters` behaviors, **statement/item boundaries synchronize on
> `;`/`}`/item keywords**, and the trailing-`.`/`?.` member recovery is a
> first-class case in the postfix loop.

The delimiter half shipped (`recover_delimited`, ten sites). The member-recovery
half shipped. **The statement/item clause did not.** Both statement loops
`break` on the first decline — `parse_program` at `parsing.rs:912`,
`parse_block_clean` at `:2089` — and there is no `synchronize`, `skip_until`, or
`expect_or_recover` anywhere in the 5,235-line parser.

This matters twice over: it is the direct cause of §2.2's mechanism 3 and of
§8's failed clauses, **and** it means S1 is not a new proposal. The design is
ratified; only the code is missing. That should lower, not raise, the bar for
scheduling it.

### 10.2 `diagnostics-standard.md` §4 describes a parser that no longer exists

`diagnostics-standard.md:79-85`:

> **Lexer/parser errors** are chumsky-generated expected-lists ("found X,
> expected one of …15 tokens"). Curated parse messages are genuinely the
> handwritten parser's territory (H6); this arc records the worst offenders …
> with targeted `labelled` improvements only. **Open question 2.**

and the sign-off at `:143-145`: "parser errors stay generated until H6".

H6 shipped 2026-07-22 (`backlog-2026-07-18.md:666`). chumsky is deleted — eight
files, 4,601 lines, gone from the lockfile (`frontend.md:8-11`). **The deferral
has come due and nobody collected.** Parser messages are now hand-written and in
scope for the standard like any other diagnostic — which is the premise §4 and
§5 of this paper argue from, and it should be recorded as such rather than
assumed.

### 10.3 The charter's own framing of (e)

E49 places the missing-return-value span problem in "the E38 family's
territory". E38 (`backlog-2026-07-18.md:569`) is about diagnostic
**determinism and attribution** — one canonical sort at one seam, notes made a
field, parallel vectors co-permuted — not about anchoring. Its later leg (item
112, shipped 2026-08-09) is about a diagnostic carrying its span's *file*.

The relationship is real but narrower than the framing suggests: E38's canonical
sort key is `(source, span, msg, note)`, so **any re-anchor in §3 changes the
sort key and can therefore reorder neighbouring diagnostics.** E38's 30/30
byte-identical harness is the gate S3 must pass, and golden movement should be
expected rather than treated as a regression. That is a constraint E49 inherits
from E38, not a shared subject matter.

### 10.4 Two smaller ones

- `parsing.rs:2061-2065` says a value-less block's void is "at the closing
  brace". It is at `span.end..span.end`, which is one byte **past** the brace
  (§3.5). The pin `empty_block_value_is_void_at_the_closing_brace`
  (`parsing.rs:4470-4476`) asserts the value, not the offset, so the prose has
  been wrong without a test noticing.
- `Span::to_end()` (`span.rs:41-47`) has **zero callers** in the tree. Either
  §4.4 gives it its first one, or it should go.

## 11. Slices

Sized for future lanes, and split along the line the charter draws: parser
recovery on one side, analyzer spans and messages on the other. **S1/S2 and
S3/S4/S5 are independent and can run in parallel lanes**; within each column the
order is a real dependency.

### The parser-recovery column (H-section)

**S1 — the statement/item synchronizer, and unclosed-region tolerance.** (L)
Build `frontend.md:137-140`'s ratified clause: on a declined statement,
synchronize forward to the next `;`, `}`, or item keyword and resume, at both
loops (`parsing.rs:912`, `:2089`). Give `scan_balanced` (`:816-865`) an unclosed
mode so `recover_delimited` fires on a region that never closes, and anchor the
resulting `unclosed \`X\`` at the **opener**. Gated by §8's three clauses.
Prerequisite for everything else in this column, and the single highest-value
slice in the paper — it is what §2's blackout costs.

**S2 — the missing-token vocabulary and the gap anchor.** (M)
Put `;` on the committed side of `frontend.md:24-31`'s noting rule (`expect_ctrl`,
`parsing.rs:516-525`, plus the six bare `eat_ctrl(';')` statement sites). Anchor
a missing-terminator failure at the gap — the previous token's end — via
`Span::to_end()`, rather than at the found token (`emit_failure`,
`parsing.rs:700-710`). Message: `expected \`;\` to end this statement`. Depends
on S1: with the parser stopping, the message improves but the blackout still
hides it. The insert-`;` quick fix is **not** in this slice — §3.8.1.

### The analyzer span/message column

**S3 — the missing return value.** (M)
Three parts, in order: (i) the synthesized `Node::Void` carries the closing
brace (`parsing.rs:2101` and `:2072`) — a one-line change that fixes regime 1's
invisibility and P28's duplicate at once; (ii) a closure whose *expected* type is
known and differs only in its return type is checked in **return position**, via
the same path `ret-checking.md` rule 2 gives a named function, falling through
to today's value comparison when the parameter types differ (P27); (iii)
`Closure::return_type` is read by the analyzer, so an annotated closure gets
rule 2 directly. Ships the regime-1/1′/2 messages; the remove-`;` action is
**not** in this slice (§3.8.1). Owes an **A4 catalog addition** and a **ledger
re-verdict** (§7.5, §13.5). Gated by E38's determinism harness (§10.3).

**S4 — the count messages name their subject.** (S)
`Expected 2 arguments…` → `` `distance` expects 2 arguments, but got 1 instead:
`y: i32` is missing. ``; the same for struct fields, both directions. Clamp a
wrapped argument list's span to its first line (§13.3). Ledger rows updated in
the same commit.

**S5 — the field-name span and did-you-mean.** (S)
Thread a `field_name_span` into the struct-initializer constraint and anchor
`struct '{}' has no field '{}'` there (`analyzer.rs:25718`) — the survey's one
clear-cut A1 violation. Add the guarded suggestion. Cheapest slice with the
clearest verdict; buildable independently of everything else.

### The tail

**S6 — `vilan check` analyzes the salvaged tree.** (S)
Drop `tree.filter(|_| clean)` (`crates/vilan-cli/src/main.rs:2521`) on the
`check` path only, matching what `analyze_source` already does for the LSP.
`build` keeps today's behavior. Best done after S1, when the salvaged tree is
worth analyzing. Raised as §13.1 because it changes `check`'s contract.

## 12. The pins each slice owes

`diagnostics-standard.md:69-72` C2: "Every diagnostic class carries an
`assert_fails_spanning` pin: exact span text + a message fragment. **A site with
no pin is unaudited by definition.**" By that rule, most of this survey's
subject matter is currently unaudited:

- No test anywhere pins a missing semicolon, a missing `(`, or a missing `)`
  outside the two existing separator forms (`parsing.rs:5175-5215`).
- No test pins **two** syntax errors from one file. `parser_recovery.rs:19-22`
  says so outright: "The pins assert 'at least one error', never an exact
  count." §8's bar cannot be defended without exact counts.
- The `"expected an item"` fallback (`parsing.rs:687-691`) — every file-scope
  missing semicolon — appears in **zero** tests.
- The function-tail and `ret` return checks are pinned with bare `assert_fails`
  — no span, no message: `ret_value_is_checked_against_the_declared_return_type`
  (`inference.rs:10599`), `bare_ret_in_a_value_returning_function_is_rejected`
  (`:10617`), `function_tail_is_checked_against_the_declared_return_type`
  (`:10681`), `a_void_call_tail_is_not_a_value_return` (`:10697`),
  `a_void_call_ret_is_not_a_value_return` (`:10715`). The closure-as-value
  mismatch is pinned by **message only** — `assert_fails_with` on
  `"Expected |Route| Route, but got |Route| Other instead."`
  (`inference.rs:49041`). **The entire missing-return-value span surface is
  unpinned.** The `ret`-participation messages are the family's one exception:
  they *are* span-pinned — `bare_ret_in_a_value_yielding_closure_is_rejected`
  (`inference.rs:11746`), `value_ret_in_a_void_closure_is_rejected` (`:11770`),
  `async_block_rets_check_against_the_tail` (`:11814`).

Per slice:

| Slice | Owes |
|---|---|
| S1 | §8's three clauses as exact-count pins, **written before the slice starts** (clause 1 is met today — pin it first so the change is proven not to break it); the P31-B shape (a diagnostic in the file tail survives an unclosed `(` above it); the P29 shape once S6 lands |
| S2 | `assert_fails_spanning` on the gap anchor for each of P2/P3/P4; the P5 silence pinned as a *non*-diagnostic (a missing `;` on a void body's last statement stays clean) |
| S3 | `assert_fails_spanning` per regime (P22, P25, P24, P21, P23) — five cases, five pins, per CLAUDE.md's "per case, not per example"; P27 pinned as unchanged; P26 as newly-checked; P28 as a single diagnostic |
| S4 | Message pins on all four forms (too few / too many arguments, missing / extra field); a span pin on the clamped wrapped list |
| S5 | `assert_fails_spanning` on the field **name** (P19 and P20, since P20 is what proves the anchor moved); the suggestion pinned present on a close match and **absent** on a distant one |
| S6 | The P29 shape: a parse error plus two analyzer errors reports all three |

Every span pin should be written against `assert_fails_spanning`
(`inference.rs:800-822`) or `assert_fails_spanning_nth` (`:779-798`) — the
nth-occurrence helper the standard's §5 already records.

## 13. Open questions

### 13.1 Should `vilan check` analyze a file that did not parse cleanly? — recommend: yes, `check` only

Today it does not, by an explicit decision (`crates/vilan-cli/src/main.rs:2511-2514`),
and that costs §2's mechanism 1. The LSP already does the opposite on the same
tree with no known ill effects. The risk is spurious analyzer diagnostics from a
recovered region — mitigable by suppressing diagnostics whose span falls inside
one. `build` should not change: you cannot emit from a recovered tree. **Draft:
change `check`, leave `build`.** Flagged because it changes an output contract
users may script against.

### 13.2 Should a missing *opening* `(` get a targeted message? — recommend: no

`frontend.md:24-31` deliberately keeps opener head-checks silent, and the reason
(expected-dump noise) is sound. §5.2 argues the `;` work covers the case well
enough. **Draft: leave it.** Raised because the charter names "missing opening
… parenthesis" explicitly and a null answer to half a named shape should be the
owner's to accept.

### 13.3 Should a multi-line span clamp to its first line? — recommend: raise, do not decide here

It would improve (c)'s wrapped-list case (P17) and (e)'s regime 2 (P25), and
probably a dozen sites this survey did not look at. It is a **rendering policy**
that would apply compiler-wide, not an E49 decision, and it interacts with
ariadne's multi-line rendering (which is genuinely good when the span really is
the construct). **Draft: file separately; S4 clamps only the arity case, where
a count is provably not a property of the lines.**

### 13.4 Should did-you-mean be a general helper? — recommend: yes, separately

§7.2 needs it for one site. Unknown method names, unknown imports, unknown enum
variants and unknown local names all want the same thing, and the tree has
exactly one hand-rolled instance today (`analyzer.rs:11071`). **Draft: S5 ships
the struct-field case with a local helper written to be lifted; a general
sweep is its own backlog item.**

### 13.5 The A4 catalog addition — the owner's call

This is the one place the survey asks to change ratified policy, and it should
not be slipped in.

`diagnostics-standard.md:32-38` A4's catalog says **"argument mismatch → that
argument"**, and the ledger has already verdicted the closure case
**QUALIFIES — value-anchored (A4)** on seven rows. Under today's rules, the
whole-closure underline in P21/P23/P24 is *correct*. §3's recommendation is
therefore not a bug fix — it needs A4 to gain a row that takes precedence:

> **a missing return value → the callable's closing brace, with the declared or
> expected return type as the secondary note** — because the argument's *type*
> is wrong only as a consequence of a value that was never produced, and the
> place the fix goes is inside the argument, not at it.

**Draft: add the row.** The evidence is P24 — 22 characters underlined to ask
for one to be deleted — and P26, where the user wrote the return type and the
compiler ignored it. But the survey flags this as the owner's, because it is a
policy amendment with reach beyond E49: the same reasoning would apply to any
"the value is wrong because of something inside it" diagnostic, and the standard
deliberately resisted that generality once already.

### 13.6 Is the blackout in E49's scope at all? — flagged

§2 is the survey's biggest finding and the charter does not name it. It is
arguably E49's subject (it is *the* in-progress-change diagnostic behavior) and
arguably an H-section item of its own (S1 is pure parser work against a
ratified-but-unbuilt design). **Draft: keep S1/S6 filed under E49, since the
survey is what found them and the pins are shared, and cross-reference from H.**

### 13.7 Where does the quick-fix surface get filed? — recommend: separately

§3.8.1 found that the language server offers exactly one code action
(Organize Imports) and refuses every other kind outright. Two of this survey's
steers — remove the `;`, insert the `;` — are textbook quick fixes and cannot
be served. Building the surface means widening the advertised kinds, adding a
diagnostic-to-action router, and putting a stable `code` on the diagnostics that
carry fixes (the field is never set today, `publish.rs:227-233`). **Draft: its
own E-section item, downstream of S2/S3.** Recorded here because E49's charter
asks for "the resolution steer" and the strongest form of a steer turned out to
be unavailable — that is a survey finding, not an implementation detail.

## 14. The recommendations, collected

1. **Build the ratified statement/item synchronizer** (`frontend.md:137-140`,
   never implemented) and make `scan_balanced` tolerant of an unclosed region.
   This is the highest-value change in the paper: it ends the file-tail blackout
   (§2.2 mechanism 3), makes the correct `unclosed \`X\`` message reachable, and
   is the acceptance test for §8's recovery bar. **S1.**
2. **Put `;` in the parser's expected-set** and anchor a missing terminator at
   the gap, not at the next statement's head. Message:
   `expected \`;\` to end this statement`. **S2.**
3. **Give the missing return value its own anchor** — the callable's closing
   brace, one character wide, with the return type as a C3 note — and reach it
   for closures by **checking a closure's body in return position** when only
   the return type differs. Fix the synthesized `Void`'s off-by-one span, and
   make the analyzer read `Closure::return_type`, which it ignores today. **S3.**
4. **Let the count messages name their subject** — the callee, the struct, and
   the missing or extra field. **S4.**
5. **Anchor `has no field` at the field name**, not at its value (the survey's
   one clear A1 violation), with a guarded did-you-mean. **S5.**
6. **Let `vilan check` analyze the salvaged tree**, as the LSP already does.
   **S6.**
7. **Leave alone**: a missing closing `)` in a committed list (§5.1 — the best
   diagnostics in the survey, and the model for the rest); the arity *anchor*
   (§6.2 — already ratified as the standard's own model); the LSP's two-snapshot
   law and its debounce (§1.3 — a faithful window, not a source of problems);
   the silence on a missing opening `(` (§5.2, §13.2); the silence on a missing
   `;` at the end of a void body (§4.1 P5 — correct language semantics).
8. **File separately**: the quick-fix surface (§3.8.1, §13.7 — the server offers
   Organize Imports and nothing else, so the strongest form of two of this
   survey's steers is unavailable today); a general did-you-mean helper
   (§13.4); a compiler-wide multi-line-span clamp policy (§13.3).
9. **Record the drift**: `diagnostics-standard.md` §4's chumsky-era premise has
   been stale since H6 shipped, and parser messages are now in scope for the
   standard (§10.2); `parsing.rs`'s "at the closing brace" comment is one byte
   off (§10.4); `Span::to_end()` has no callers (§10.4); E49's "E38 family"
   framing describes an inherited *gate*, not a shared subject (§10.3).

## 15. Implementation notes — the recovery lane (S1, S2, S6)

Shipped as the `recovery` lane of cycle 18, in three commits: the synchronizer
with the `;` vocabulary (S1+S2, which §11 pairs), the item-keyword reach the
first commit's scan turned out to need, and `check`'s goal split (S6). §8's
three clauses are pins in `crates/vilan-core/tests/parser_recovery.rs`, the
blackout itself is pinned at the wire in `crates/vilan-lsp/src/publish.rs` and
at the analyzer in `inference.rs`, and the corpus goldens did not move a byte.

### 15.1 The synchronizer, and the two loops

`Parser::recover_statement` is the whole of S1: a declined statement is
reported once and the cursor moves to the next boundary, at both loops
(`parse_program`, `parse_block_clean`). §10.1 was right that this is not a new
proposal — the design was `frontend.md`'s, ratified and unbuilt — and right
that it is the file's load-bearing change. Three things followed from it that
§11's sizing did not name:

**`parse_block_clean` had to try the TAIL before recovering.** A block's
trailing expression is a statement that legitimately declines — `x + y` before
`}` is not a statement, it is the block's value — so the loop tries the tail on
every decline and only recovers when that fails too. Without this the parser
would report a diagnostic on every clean value-returning function in the
corpus.

**The block's `nested_delimiters` site retired.** Past its `{`,
`parse_block_clean` can no longer decline: a broken statement is recovered
individually, and a body that runs out of input reports ``unclosed `{` `` at the
opener and keeps what it parsed. That made `recover_delimited("block", …)`
unreachable and it is deleted — nine sites remain, not ten. This is §2.2
mechanism 2 fixed at the root rather than narrowed: the survey proposed
narrowing the blast radius by making `scan_balanced` tolerant, and the honest
fix was for the block never to reach region-skipping at all.

**`emit_leftover_error` retired with it.** `parse_program` now consumes the
whole token stream by construction — there are no leftovers to report — and a
`debug_assert` in `parse_with` holds that invariant. Its farthest-failure logic
lives on in `emit_statement_failure`, which is the same choice made per
statement instead of once at the end.

### 15.2 Where the boundaries are, and the two that reach inside a region

`;` at statement depth (resume after it), the enclosing `}` (resume on it), an
item keyword, and — added beyond `frontend.md`'s list — a statement-head
keyword. The last one is what keeps §8 clause 3 honest for shape (a): syncing
from a missing `;` to the *next* `;` would swallow the following statement
whole, and the following statement is exactly the one whose diagnostics clause
3 promises to keep. `let`/`mut`/`ret`/`jump`/`if`/`for`/`match`/`const`/`async`
cannot continue the broken statement, so stopping there costs nothing.

Identifiers and literals are deliberately not boundaries. They begin an
expression statement, but they also appear all through a broken one, and
resuming mid-garbage reports again — the cascade §9 records vilan as not
having, and must keep not having.

Two boundaries reach INSIDE a region the scan is skipping, each because the
token cannot be part of one:

- **a `;` whose innermost opener is `(`.** A call's arguments and a
  parenthesized expression admit no semicolon (the constructs that do,
  `[value; length]` and a block's statements, sit under `[` or `{`), so it
  terminates a statement written below the unfinished region. Without this,
  `print(` followed by three statements swallowed all three.
- **an item keyword with no `{` open above it.** `fun broken( {` leaves the
  parameter list open, and the scan ran to end of input — the file-tail
  blackout again, one layer above the statement loop. The pre-fix tree for a
  four-item file was `([], 0..74)`. A `{` on the stack means a block or closure
  body, where a nested `fun` is ordinary code; the pin for that shape stays
  green with the reach removed, which is what makes it a guard rather than a
  restatement.

Statement heads get no such reach: `if`/`match`/`async` are perfectly good
arguments.

### 15.3 Insertion, where skipping would manufacture a diagnostic

Synchronizing is the wrong recovery for shape (a), and the survey could not have
seen it because it measured what the compiler *said*, not what a fixed compiler
would then say. A missing `;` is not a statement the parser failed to read — it
read the statement perfectly and wants one more token. Skipping it is honest
about the syntax and wrong about the program: dropping `let origin: Point = …`
unbinds `origin` at every use below it, and dropping `import std::print` unbinds
`print` in the whole file. P2 and P3 measured that way came back with the right
diagnostic and a screenful of "cannot find" beneath it, on lines that were
correct.

So a statement whose body parsed to completion, and whose next token can only
BEGIN a fresh statement or item, is kept: `recover_missing_terminator` reports
the gap and pushes the statement. §8 clause 3 in the direction the survey did
not measure — a parse error that must not remove a diagnostic must not
manufacture one either.

The keyword-head boundary is what keeps this from cascading, and it is the same
token class the synchronizer syncs on. An identifier or a literal does not
qualify, so `print 1);` still takes the skipping path and reports once (§5.2's
accepted outcome for a missing opening paren) rather than accepting `print`,
resuming at `1`, and reporting again. Pinned both ways: two pins that need
insertion, and one that stays green with insertion removed.

### 15.4 Which diagnostic a recovered statement reports

Three-way, in the order the grades ask for. A committed demand that failed
strictly inside an unfinished region wins — that is §5.1's `found ';' expected
',' or ')'`, the best diagnostic in the survey, and P8 keeps it. Otherwise, an
opener still unclosed at the boundary reports ``unclosed `X` `` at the opener
(§5.3). Otherwise the farthest failure, or the position's own fallback.

The failure-within rule is `recover_delimited`'s own, reused: it is the same
question ("did anything inside the region locate a real error?") and deserved
the same answer rather than a second one.

Note what this means for P14 and P8, which the survey grades separately: they
are the SAME shape to the parser — an argument list that reached a `;` — and
they now produce the same message. P14's `let b: i32 = 2` reads as an argument,
a committed demand fails at its `;`, and the located message wins. The survey
recorded a count for P14 and no message; the count is 1, as it was.

### 15.5 §4.4's anchor: one character, not zero

§4.4 asks for two things that cannot both be built: "a zero-width span at the
end of the previous token", and "rendered as a one-character caret at that
position". There is no rendering layer between them — `line_index.rs` converts
both ends verbatim (§3.2 measures exactly that) — so a zero-width span is a
zero-width LSP range, which VS Code draws as nothing. The implementation takes
the rendered intent: the **last character of the previous token**, which is
what §4.4's own drawing puts the caret under (the `}` of `… y = 4 }`), computed
on a char boundary so a token ending in a multi-byte character still slices.

`Span::to_end()` therefore still has zero callers (§10.4). It is left alone
rather than deleted: `frontend.md` §2 lists it in the span API the cutover
committed to, and removing it is not this lane's call.

### 15.6 S2's scope: three sites, not `expect_ctrl`'s set

§11 asks for `;` on the committed side of the noting rule "(`expect_ctrl`, plus
the six bare `eat_ctrl(';')` statement sites)". Widening `expect_ctrl`'s match
would have reached two `;` demands that are not statement terminators at all:
the `[T; length]` array type and the `[value; length]` repeat fork, both of
which spell `;` as a fork between two readings. A failure there would have
reported "expected `;` to end this statement" in type position.

So the terminator is its own demand (`note_terminator`) at the three sites that
genuinely have one: the expression statement (which serves both loops), the
import statement, and the use statement. The `fun`-body and `struct`-body forms
(`;` or `{ … }`) are forks too and are left silent. P2, P3 and P4 are all
covered by those three; P5's silence is pinned as a non-diagnostic.

### 15.7 S6: nothing to suppress, and what a salvaged tree does produce

§13.1's mitigation — "suppressing diagnostics whose span falls inside a
recovered region" — needs no code, and the reason is worth recording: a
salvaged tree holds nothing inside a skipped region to diagnose. A dropped
statement is not in the tree; a garbled region's `Node::Error` placeholder
types as nothing and reports nothing. Pinned as
`a_recovered_region_produces_no_analyzer_diagnostics_of_its_own`.

What a salvaged tree does produce is a different class the survey did not
name — **consequence** diagnostics, from what recovery removed rather than from
what it kept: a body that lost its tail reads as `void` against a declared
return type, a declaration the parser could not read at all leaves its name
unbound at every call site. (The commonest source of those — a statement
dropped over its missing `;` — is gone, §15.3.) These are reported, beside the parse error that explains them. They
are not suppressed, for two reasons: the anchors lane's S3 owns the void-tail
diagnostic and would be fighting a suppression written here, and a rule broad
enough to catch them ("drop analyzer diagnostics when the file has a parse
error") is the blackout with extra steps.

`build` is untouched, per §13.1's draft. The two goals now differ at one
`CompileGoal` parameter through `compile_unit`/`compile_to_js`; emission gained
a parse-clean gate of its own, so no goal can reach codegen with a recovered
tree.

### 15.8 §8's bar, measured after

| Clause | Before | After |
|---|---|---|
| 1 — one missing token, one diagnostic | met (P6/P14) | met, and pinned with exact counts |
| 2 — N errors, N diagnostics, same body or after an unclosed delimiter | not met | met (two per body, two across an unclosed `(`) |
| 3 — a parse error never removes a diagnostic from a region it does not contain | not met | met at statement, body, item and file-tail scope |

Clause 3's one residual is textual containment: a statement swallowed INTO an
unfinished `(` — `print(` on one line, `let b = 2;` on the next — is read as an
argument and is lost with the region. There is no reading in which it is both
an argument and a statement. Everything past that statement's `;` resumes, and
the pin says so rather than leaving it to be discovered.

### 15.9 Drift found while building

- **§3.8.1 is stale.** The server advertises `QUICKFIX` today, alongside
  Organize Imports and a fix-all kind, and ships quickfixes for the
  add-missing-import and misspelled-field diagnostics. The survey's "there is
  no quick-fix surface" was true when it was written and is not now; the
  insert-`;` and remove-`;` actions §3.8.1 defers are ordinary work on an
  existing surface, not a prerequisite.
- **The B38 salvage-tail pins needed a new break shape.** They keyed on a stray
  top-level token truncating the parse, which S1 no longer does; their own
  premise assertion caught it, as it was written to. They now use an
  unterminated `i"""`, which truncates at the LEXER and is beyond a parser's
  recovery — the honest remaining shape for that feature.
- **`a_top_level_error_salvages_the_prefix_and_drops_the_tail`** pinned the
  blackout as a contract ("the tail after a top-level stray token is not
  recovered"). It is now
  `a_top_level_error_keeps_the_items_on_both_sides`.

## 16. What shipped — the anchors lane (S3/S4/S5, implementation record, 2026-08-11)

The analyzer-span/message column, built against `next` in the `anchors`
worktree while the recovery lane built S1/S2/S6 concurrently. Diff scoped to
spans and messages in `analyzer.rs`/`parsing.rs`/`tests/inference.rs`, plus
one LSP-side hover fix span/rendering forced (§15.1). Zero corpus golden
movement (`cargo test -p vilan-cli --test corpus`) — diagnostics don't
change emitted JS.

### 15.1 S3 — the return-value re-anchor

**Shipped**, with two deliberate scope cuts recorded below.

- **The brace anchor** (§3.9's one-line fix): `parse_block_clean`'s
  synthesized `Node::Void` now carries `span.end - 1..span.end` — the
  closing brace itself — instead of the zero-width point past it. This
  alone re-anchors regime 1 everywhere the tail is checked (named functions
  today; closures per below) and closes the §10.4 drift: the doc comment's
  "the value is void at the closing brace" is now literally true, not one
  byte off. **Not touched**: `parse_block`'s *recovery* twin
  (`recover_delimited`'s fallback) — that function is S1's territory
  (`frontend.md:137-140`'s unclosed-region tolerance), left to the recovery
  lane to avoid an append conflict on code it is actively rebuilding.
- **Regime 1 vs 1' message.** `Constraint::ReturnType` gained an optional
  `last_statement_id`, populated at the tail-construction site (walk time,
  from the raw statement list) and left `None` at the `ret`-construction
  site. `resolve_return_type` (renamed internals: the check now lives in
  shared `check_return_position`, plus `missing_return_value_message`)
  distinguishes "this body ends without producing a value" (regime 1) from
  "the `;` discards this body's last value" (regime 1') by whether that
  statement, inferred bare, reconciles with the declared type — excluding
  `Type::Void`/`Type::Never` (a genuinely void statement, or a diverging one
  like a bare `ret`) **and** `self.variables` (a `let`'s own id types as its
  *binding*, e.g. `i32` for `let sum: i32 = a + b;`, which reconciles by
  coincidence — `let` isn't an expression, and this is the bug the dev-time
  probes P22 caught before the guard was added).
- **Regime 3 — closures, both routes.** `analyzer::Closure` gained
  `return_type_id: Option<TypeId>`, read from the closure's own annotation
  (S3-iii, "gets rule 2 directly" — P26). `infer_type_path`'s `Expr::Closure`
  arm additionally routes through `check_return_position` when the
  *context's* expected closure type is known and every parameter already
  reconciles (S3-ii — P23/P24), replacing the whole-value comparison at the
  consuming site rather than adding to it: on a mismatch the closure's
  *reported* type becomes the target it was held to, so the caller's own
  reconcile trivially agrees and never double-reports (guarded further by an
  exact span+message de-dup check, since `infer_type` has no memoization and
  the same closure can in principle be inferred more than once per constraint
  attempt). **The gate that makes this safe**: `type_is_ground` — a target is
  only used when, after substitution, it carries no `Generic`/`Unknown`/
  `Unresolved` anywhere in its structure. Without it, routing through a
  still-abstract target (e.g. `Iterator::from_fn<T>`'s `Option<T>`) silently
  swallowed the binding `reconcile_type` would otherwise have produced at the
  caller, and four *unrelated* iterator tests broke during development before
  the gate was added — the regression is what the gate's own doc comment
  points at.
- **LSP side-effect, fixed.** The wider `Expr::Void` span is now real code
  for the first time, which meant it started winning hover's smallest-span
  lookup at the closing brace (`entity_at`'s `min_by_key`), regressing
  `a_body_brace_still_hovers_the_enclosing_function`. Fixed at the root: a
  synthesized `Expr::Void` is not something the user wrote and is now
  excluded from `entity_spans` entirely (hover, go-to-definition, semantic
  tokens, document symbols all read that one list), so a cursor on the brace
  still finds the next-smallest *real* entity around it. A new pin,
  `a_missing_return_value_publishes_a_one_character_range_not_a_zero_width_one`
  (`crates/vilan-lsp/src/publish.rs`), proves the exact editor-visible
  regression the paper's §3.2 measured (`start == end`) no longer holds.

**Not shipped, and why:**

- **Regime 2's `if`-no-`else` refinement** (§3.6's "one refinement"). Left
  on today's already-A1-compliant anchor (`missing_return_value_regime_2_
  if_with_no_else_is_unchanged` pins it explicitly unchanged). The refinement
  needs a provenance channel from `infer_type_path`'s `Expr::If` arm (which
  drops straight to a bare `Type::Void` on a missing final else, §3.6 of the
  exploration) through to whichever diagnostic reports the mismatch — new
  plumbing, not a span/message edit, and out of this slice's scope per the
  cross-lane instruction to keep the diff to spans and messages.
- **P21 — the generic-propagation case.** `.map<U>`'s `U` is exactly the
  shape `type_is_ground` is built to decline (§ above); the closure's void
  return still surfaces one level out, on `List<void>`, unchanged. Pinned
  `#[ignore]`d as a known residue
  (`missing_return_value_regime_3_through_a_generic_binding_is_not_yet_fixed`),
  per CLAUDE.md's convention for a known-but-unfixed bug — tracing the root
  cause back through a generic binding is a materially different mechanism
  from the direct-target case this slice builds.
- **P28 — the bare-`ret` duplicate.** The brace fix makes *both* of P28's
  diagnostics correctly anchored and visible (previously one was an invisible
  zero-width point); it does not deduplicate them — that is a B5 fix
  requiring the tail-construction site to know a preceding `ret` already
  diverged, which needs its own design. Pinned as today's true (still
  doubled) behavior, not ignored, since nothing asserted is wrong on its own:
  `a_bare_ret_still_duplicates_the_synthesized_tail_diagnostic`.
- **The C3 "declared here" note** (§3.6's drawn example, recommendation 3).
  Not built: it needs the declared/expected return type's own *span* threaded
  to the checking site (`Function`/`Closure` carry a `TypeId` today, not a
  span), a second field-and-plumbing exercise on top of `last_statement_id`.
  The primary anchor and message — the load-bearing half of §3 — ship without
  it.

Ledger consequence (§13.5, the owner's call, left open): this slice's
messages are what a hypothetical A4 amendment ("a missing return value → the
callable's closing brace") would describe. No catalog row was added — that
remains the owner's to rule on, unchanged by this record.

### 15.2 S4 — the count messages name their subject

**Shipped** exactly as specified, at the three sites the survey probed
(P15/P16 function and method calls; P18 struct fields) — the closure-typed-
value call and enum-variant-constructor arity sites (structurally identical
`Expected N arguments…` pushes, not in the survey's probe set) were left
untouched, in keeping with the scoped diff.

- `` `distance` expects 2 arguments, but got 1 instead: `y: i32` is missing. ``
  and the too-many form with no steer (B4), for both a plain function call
  (`resolve_call_subject`) and a method call (`resolve_method_arg_check`) —
  `callable_name` resolves the callee's declared name uniformly for both
  (a `Function`/`ExternalFunction` id directly, or through a method's
  `member_id` via `expr_id_to_expr_map`, mirroring `method_signature`'s own
  indirection).
- `` `Point` expects 2 fields, but got 1 instead: `y` is missing. `` and, the
  asymmetric extra-field case struct fields get that call arguments don't
  (fields are named): `` `Point` expects 2 fields, but got 3 instead: `z` is
  not a field of `Point`. ``, re-anchored at the offending field's *name*
  span (E58's `field_name_span`, reused) — a duplicate-named field with no
  single unmatched name falls back to the bare count, never a guess.
- The wrapped-argument-list clamp (§13.3, scoped here to the arity case only,
  per the paper's own instruction not to build the general policy):
  `clamp_span_to_first_line` trims `arguments_span` to its first line for
  both call sites; struct-field spans are untouched (§7.1: the brace region
  is already the right size for a missing-field gap).

Two existing pins were **re-verdicted** to the new wording in the same
commit (`a_fn_typed_binding_checks_its_arity`,
`initializer_field_count_mismatch_is_unaffected_by_the_closest_name_scan`),
per §7.5's standing rule — this is a re-verdict of an already-QUALIFIED
message family, not a bug fix.

### 15.3 S5 — the field-name span: VERIFY-FIRST VERDICT

**RE-VERDICT — E58 (cycle 17) already fully satisfies S5 as specified.**
Reproduced against today's compiler before writing anything: `Point { x = 3,
yy = 40000 }` anchors `struct 'Point' has no field 'yy'` on `yy` (five
columns left of where it anchored pre-E58), with a guarded `did you mean
'entries'?`-style note present on a close typo and absent on a distant one.
`StructInitializerConstraint::fields` already carries the `field_name_span`
S5 asked to thread; `analyzer.rs`'s unknown-field arm already anchors there
(not at `field_value_span`). **No fix shipped for S5** — the slice's own
text scopes it to exactly this one diagnostic (the struct-initializer
unknown-field case), and every part of that is already true.

The one gap: the pin table (§12) names **both** P19 and P20 as owed pins —
P20 specifically because a probe that reproduces the OLD bug's exact shape
(a *wide* value next to a *short* name) is what proves the anchor moved,
not merely that it currently sits somewhere plausible. Only a P19-shaped
pin (`unknown_initializer_field_spans_the_name_not_the_value`, a one-
character value) existed. Added
`unknown_initializer_field_with_a_wide_value_still_spans_the_name` (P20's
exact reproduction, `yy = 40000`) to close that gap — the only new test S5
owes, per "add any pin the paper specifies that doesn't exist yet, and
write no fix."

### 15.4 Files touched

`crates/vilan-core/src/parsing.rs` (the brace-span one-liner, main path
only), `crates/vilan-core/src/analyzer.rs` (S3's `Constraint::ReturnType`
field, `check_return_position`/`missing_return_value_message`/
`type_is_ground`/`closure_block_tail`, `Closure::return_type_id`,
`Expr::Closure`'s return-position routing; S4's `callable_name`/
`argument_count_message`/`clamp_span_to_first_line`/
`struct_field_count_message`), `crates/vilan-core/src/span.rs`
(`Span::to_end()` deleted — confirmed zero callers, per §3.9's own
recommendation), `crates/vilan-core/tests/inference.rs` (new S3/S4/S5 pins,
two S4 re-verdicts, one stale-comment fix), `crates/vilan-lsp/src/
document.rs` (`entity_spans` excludes synthesized `Expr::Void`),
`crates/vilan-lsp/src/publish.rs` (the one-character-range pin). No docs
page under `vilan/docs/` quoted the old message text for any changed
diagnostic (checked by grep before closing the docs gate), so none needed
updating in this commit.
