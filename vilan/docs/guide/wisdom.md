# Writing Vilan programs well

A working reference: the principles behind good decisions in a Vilan
program, and the cases they came from. It does not promise the best code
every time. It makes the usual mistakes rare and the remaining ones easy to
see.

Most of it was first written for Kolt, a large Vilan app (channels, tasks
and documents for a team, with a browser client and a server), and its
cases are Kolt's. They are kept as evidence: each one is a real problem,
with what caused it and what fixed it. The principles are stated for any
Vilan program. Principles 18 to 20 are newer, and their cases come from the
Vilan tracker. The numbers are stable: code and commits cite a principle as
"WISDOM 12", so a principle keeps its number when others are added.

Most "new" problems are old problems combined to form a new one. A feature
nobody has built before still breaks down into questions that have been
answered: where a fact lives, who releases a subscription, what a change
costs, what happens before the data arrives. Answer those, and the new
feature is mostly assembled from known parts.

Solving a problem takes three kinds of reasoning, and this page has a part
for each:

- **Diagnosis** (abduction: guessing the explanation). A problem arrives as
  a symptom. This part maps symptoms to their likely causes, ranked, so the
  guess is a short list to check rather than an open question.
- **Principles** (deduction: working out what follows). The questions to
  ask, each with its usual answer, the reason for it, the conditions it
  holds under, and a way to check it.
- **Cases** (induction: learning from evidence). Real problems, each with
  its symptom, its root cause, the fix, and the principles it rests on.
  When a new problem looks like one of these, start from its fix. Cases
  where a principle turned out wrong belong here too: they are how the
  principles get better.

Experience makes the first kind cheap. A beginner facing a lag has endless
possible explanations; someone who has seen many lags has five, in order.
The diagnosis tables are an attempt to write that list down.

## Terms

Five qualities this page uses to judge code. Each is measured by what goes
wrong, so two people can disagree about a piece of code and settle it by
looking.

- **Readability:** how well the code can be read *and understood* with
  little context about the program's purpose. `3 + 5` and
  `add(three, five)` are readable; `{ let g = |x| |y| f(x, y); g(foo)(five) }`
  is not, because a reader has to run it in their head to learn what it
  does. Naming declarations well and using them well goes a long way.
- **Strength** (robustness): how few bugs the code has. Strong code has
  few, and in particular few hidden ones: cases nobody has hit yet but that
  are wrong.
- **Maintainability:** how few bugs or limitations a change to the code
  introduces. Maintainable code can be changed by someone who doesn't know
  all of it, without breaking something they didn't touch. Robust code is
  usually maintainable code.
- **Writability:** how strong code is when it's written quickly.
  Specialized abstractions make the *first* version fast, but every change
  *after* risky. Good writability means fast *and* strong: the easy way to
  write it is also the correct way.
- **Consistency:** a pattern established in a codebase is reused everywhere
  it applies. Consistency is not an argument against improvement: better
  patterns should be made often, and replace the old ones. How quickly the
  old ones are replaced is each team's call. The cost of not replacing them
  is that readers must learn both, and copy whichever they meet first.

The principles below serve these five. Most of them trade a little writing
speed for a lot of strength and maintainability; the language's job (and
the reason to move principles into Vilan) is to remove that trade, so that
the fast way and the right way are the same way.

**Inconsistency is allowed while it is tracked.** An old pattern may live
beside its replacement for a while, as long as every place it remains is
written down in the team's tracker, so it is certain to be replaced and
can't be forgotten. When the old pattern exists because of a gap in Vilan,
the tracker entry names the Vilan item. An inconsistency that isn't tracked
is a bug in the codebase's documentation.

## How to use it

**Fixing something:**

1. **Find the symptom in the diagnosis tables.** Check its causes in order,
   most likely first.
2. **Confirm the cause before fixing it.** Say why the bug happens in one
   sentence, and test that sentence: reproduce it, measure it, or read the
   code that decides it.
3. **Fix at the root.** Find the layer the behavior is decided in and fix
   it there, even if a local patch would be shorter. A local patch for a
   global cause reappears somewhere else.

**Building something:**

1. **Say what the problem is in terms of facts and flows.** What is true,
   who owns it, who needs to know, and when. A request like "show my
   assigned tasks in the switcher" becomes "a fact the server owns
   (assignments), needed by one menu, changing when tasks are assigned,
   moved or renamed."
2. **Break it into parts until each part is a known one.** Match each part
   against the cases below.
3. **Ask the principles' questions of each part before writing code.** The
   expensive mistakes are design mistakes; they are cheap to catch now and
   costly to catch later.

**Either way, afterward:** add a case when the problem taught something,
and a row to the diagnosis tables when a symptom led somewhere they don't
list yet. Prefer cases that show a principle being violated and what it
cost. A team can keep its own copy of this page for its own cases; the
principles and their numbers stay shared.

The strongest form of a principle is one the language enforces: an API that
makes the wrong thing impossible, or a warning. When a principle keeps
being broken, propose that to Vilan (the vilan-lang tracker) rather than
repeating the advice. Several entries below already moved that way.

## Diagnosis

Symptoms, with their likely causes in order and where to read more. The
order is a starting guess from Kolt's history; reorder it as cases
accumulate.

### Slow

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| Slow to open or close, worse as the data grows | Per-item subscriptions built and torn down for one list; a whole collection loaded to show a few items | 3; My work lagged |
| Slow on every keystroke or change | A derivation over the whole collection with no `memo`; an effect that rewrites a list, waking everything that reads it | 3, 4 |
| Fine in development, slow in use | A cost that grows with data the developer doesn't have: a loop over all tasks, members or messages | 3 |
| Slow, and the obvious suspect didn't help | The suspect was a guess: the time is somewhere else (a lookup inside a loop, a query per row) | 16; Every key re-read the document |
| Faster after a rewrite, but something is off | The rewrite was checked by eye, not against the old code's output | 17; An organization took seconds to load |
| Fast on one backend, slow on the other | A value copied out of a `Shared` cell on every read; the copy costs what the value holds | 18; A dictionary copied per word |

### Wrong

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| Wrong only in the first moment, then right | "Not yet" treated as "none": a decision made before the data arrived | 5; The wrong tab on re-entry |
| Right after a reload but wrong live (or the reverse) | Two owners of one fact, one not kept current; a cache with no way to go stale | 1 |
| Right here, wrong after saving or on another device | An id that only means something locally was saved or sent | 6; Image links pointed nowhere |
| One device's change undoes another's | State stored coarser than it changes: a blob replaced whole | 1; Tabs from two devices |
| A setting's new default doesn't apply | Old defaults were stored as if they were choices | 12; A new default didn't reach anyone |
| Breaks after a dependency upgrade, with no code change | The code relied on a default or a limit it never stated | 10, 20; Uploads over 1 MiB |
| A value from the database or the network is `null`, `0` or `""` where the program never wrote one | Absence read through a reader that has no word for it | 19; A NULL read as a number |
| A form does nothing, and the server's log has an error | A value the boundary can't carry, refused at run time | 19; An `Option` bound as a parameter |

### Looks wrong

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| Draws over or under something unrelated | A global system changed locally: stacking (`z-index`, `position`), a new stacking context | 10; The divider over the modal |
| Two scrollbars, or something won't shrink | A flex child's minimum size defaults to its content's: missing `min-height: 0` / `min-width: 0` | 10; Two scrollbars |
| Stray space in a row or button | An empty element still taking a flex gap | 11; Stray gaps |
| Spills out of its container | The placement doesn't clamp; or the container isn't the box you think it is | 9; Overlays weren't clamped |

### Leaks and recurrences

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| Things keep happening after their view is gone | A subscription built under the wrong owner; a closure building views called under an owner it doesn't take | 2; Pages stayed alive |
| Memory or work grows the longer the app is open | Same as above; listeners added on each open and never removed | 2 |
| A bug comes back after being fixed | Fixed at the wrong layer; or a workaround for a std gap that was copied rather than filed | 10, 8 |
| The same workaround appears in several files | A gap in std or in the app's own library | 8 |
| Someone who lost access still has data flowing to them | Access checked when asked for, never revoked afterward | 13; A removed member kept streaming |
| Something happens on a page that has nothing to do with it | A global holding a request or callback from a page that's gone | 14; A shortcut recording outlived its row |
| The client shows an action the server refuses (or the reverse) | The same rule written twice, and the copies drifted | 15 |
| Someone can reach what belongs to a group they aren't in | A lookup by an id alone, where ids are numbered across groups: the group was checked, but not that the thing is in it | 6; A session was found by its subject alone |
| A connection stays open long after its client went quiet | No deadline on the request's head, its body or the idle time between requests | 20; A stalled client held its connection |

When no row fits, the problem may be new. Reason it out from the
principles, then add the row.

## Principles

### 1. One owner per fact

**Ask:** Where does this fact live? Who is allowed to change it?

**Usually:** In one place. The server owns what's shared or persisted; the
client derives what it shows from it. Any other copy is a cache, and should
be named and treated as one: it knows where it came from and when it goes
stale.

**Why:** Two places that each think they own a fact will disagree, and the
disagreement shows up far from its cause, as a flicker, a stale row or a
conflict between devices.

**Except:** state that only one view uses and nobody else needs to know
about (a menu's open flag, a draft in progress) can live in that view. It
still has one owner: the view.

**Check:** For every piece of state you add, name its owner in one
sentence. If you can't, or there are two, stop.

### 2. Every subscription has an owner that releases it

**Ask:** When this view, row or overlay goes away, what releases the things
it started (effects, listeners, remote subscriptions)?

**Usually:** The reactive owner it was built under. Build things under the
owner whose lifetime matches theirs: a row's effects under the row, a page
under its panel, an overlay's contents under the open.

**Why:** A subscription with the wrong owner outlives what it serves. It
keeps doing work, keeps data alive, and keeps writing to things that are
gone. Nothing fails at first; it just piles up.

**Except:** things meant to last as long as the page (a global key
listener, the app's own stores) belong to the program's lifetime. Say so
where they're made.

**Check:** A closure parameter that builds a view takes the owner at the
call: since Vilan 0.54 a parameter whose closure answers a `View` (or a
`Slot`) carries `context owner_scope` without writing it. A parameter that
answers anything else (`|| void`, a value) keeps the owner it was made
under; when a body that establishes an owner (`swap`, `when`, `each`,
`run_with_owner`) calls one, the compiler warns (B677) and the fix is to
type it `(sync || void) context owner_scope`. See [the context
clause](../spec/contexts.md#85-injected-closures-the-context-clause) and
[ownership](reactive.md#ownership-who-cleans-up).

### 3. A change costs what changed

**Ask:** When one item changes, how much work happens? When the list has
10,000 items, how much?

**Usually:** Work proportional to what changed. Opening a menu should not
touch every task in the organization; filtering should not rebuild every
row.

**Why:** Costs that grow with the size of the data are invisible in
development and fatal in use. They are also hard to remove later, because
other code comes to rely on everything being loaded.

**Except:** a collection known to stay small (a board's columns, an
account's organizations) can be read and rebuilt whole. Say so in a comment
where the field is declared, so the next reader doesn't "fix" it.

**Check:** Look for a loop or `each` over a whole collection in code that
runs on every change, and for per-item subscriptions that exist only to
rebuild one list. Ask the server for the answer instead, or keep it current
where the change happens.

### 4. Derive, don't copy

**Ask:** Is this signal computed from others? Then why is it written to?

**Usually:** A value that depends on other values is a `derive`/`memo` of
them. An effect is for leaving the program: the DOM, the network, storage.

**Why:** An effect that copies one signal into another runs late, can run
in the wrong order, and can loop. It hides the dependency from everyone
reading the code.

**Except:** when the source isn't reactive (a callback API, a server push)
an effect or handler has to write the first signal. That's an edge of the
program, not a derivation.

**Check:** An effect whose body only calls `.set(..)` on another signal is
a derivation written the hard way. See [derived
state](reactive.md#derived-state-derive-pipes-and-memo).

### 5. "Not yet" is not "none"

**Ask:** What happens before the data arrives? Can the code tell "still
loading" from "there isn't any"?

**Usually:** Keep the two apart (std's `TransientState` has `Pending` for
"nothing yet" and `Absent` for "there is no such source"; or an `arrived`
flag), and make decisions that depend on the data wait for it.

**Why:** Code that treats "not yet" as "none" acts on the empty answer: it
redirects, deletes, or shows "not found" for something that exists.

**Except:** where acting on "none" early is harmless and undone when the
data arrives, such as showing an empty list for a moment. Redirects,
deletes and "not found" are not harmless.

**Check:** For every decision that reads remote data, ask what it does in
the first 100 ms. See [`TransientState`](../std/transient.md#transientstate).

### 6. Identity that crosses a boundary must mean something on both sides

**Ask:** Will this id, link or key be read somewhere else: saved, sent,
shown on another device, read after a reload?

**Usually:** Use an identity that's stable where it will be read: a
database id, an upload's id, a route. A position or a counter that's local
to one editor or one page stays inside it.

**Why:** A local id that leaks out works in the moment and points at
nothing later, with no error to say so.

**Except:** ids that never leave their scope, like an editor's block keys,
can be counters. The work is noticing when one starts to leave.

**Check:** Anything persisted or sent: could another process, another
device or next week's reload resolve it?

### 7. Types, not conventions

**Ask:** Does this string have a grammar, or a fixed set of values?

**Usually:** Then it's an enum or a struct. Strings are for text people
read and for crossing into JavaScript, CSS or HTML, where they're parsed
once at the edge.

**Why:** A convention like `"area:3"` or `"find_tab"` has to be
rediscovered by every reader, and a typo is a silent bug rather than a
compile error.

**Except:** at the edges, where the value is read by HTML, CSS or
JavaScript (a `data-` attribute), it has to be a string. Parse it into a
type as it comes back in.

**Check:** `starts_with`, `replace` and `split` on a value the program
itself produced.

### 8. Std before inventing

**Ask:** Does Vilan already provide this?

**Usually:** Look in the [std reference](../std/collections.md), and in the
std source your compiler uses (`~/.vilan/std-cache/*/std/src/`), before
writing a helper. If it's genuinely missing, write the smallest workaround,
comment it with a tracker item, and file the gap.

**Why:** A homegrown version drifts from std's behavior, isn't maintained
with it, and gets copied. Copying the nearest existing pattern spreads a
workaround as if it were a design.

**Except:** when std's version has the wrong cost or behavior for this
case. Then the workaround's comment says what std's version lacks, and the
lack is filed.

**Check:** A helper that wraps a browser API or a common data operation.
Search std for its name and for what it does.

### 9. Pure core, thin shell

**Ask:** Which part of this is a calculation, and which part touches the
world?

**Usually:** Put the calculation in a module with no browser or server
imports, and test it there. Keep the DOM, network and storage code thin
around it.

**Why:** `vilan test` runs tests on Node whatever the package's target
says, so browser-bound code can't be tested there. Logic tangled with the
DOM is untested by construction.

**Except:** code that is almost all edge (an extern wrapping a browser API)
has no core worth separating. Test it in the browser instead.

**Check:** Could this function's tests run without a browser? If it
decides something (placement, ordering, parsing), they should.

### 10. Fix at the layer that owns the behavior

**Ask:** Which layer is this behavior actually decided in: data, layout,
stacking, ownership, the language?

**Usually:** Fix it there. A rendering bug may be a layout constraint; a
flicker may be an ownership bug; a repeated workaround may be a missing std
feature.

**Why:** A fix at the wrong layer works for the case in front of you and
breaks another. CSS stacking, flex sizing and reactive ownership are all
global systems: a local change affects everything sharing them.

**Except:** when the right layer can't change now (a Vilan bug, a browser
bug), patch locally, comment the patch with the real cause, and file it
where it belongs.

**Check:** Before committing a fix, say why the bug happened in one
sentence. If the sentence names a different layer than the fix touches,
keep looking.

### 11. Nothing is nothing

**Ask:** When this shows nothing, does it leave anything behind?

**Usually:** `None` in an `Option<View>` slot, `ui::when`, `View::show`, or
`ui::nothing()` where a `View` must be returned. Rows kept only for their
effects use `ui::each_owned`, which places nothing.

**Why:** An empty element still takes a flex gap, a focus stop or a screen
reader's attention. These bugs return again and again because each looks
trivial.

**Except:** an element kept on purpose for layout (a spacer that pushes
the next item to the end) isn't "nothing"; give it a comment saying so.

**Check:** Search the views for an empty element placed only to fill a
position (`<span />`, a `div` with no children). A check that renders the
app's pages and reports any empty element inside a flex container turns
this into a gate; Kolt keeps one. See
[conditionals](ui.md#conditionals-show-when-swap).

### 12. Persisted choices and defaults are different things

**Ask:** When the default changes, what happens to people who never chose?

**Usually:** Store only what the person chose. If a stored value can't tell
a choice from an old default, a new default needs a new key.

**Why:** Otherwise a default someone never picked is frozen for them
forever, and the new default only reaches new installs.

**Except:** values that have no default worth keeping, like a window's
last size, can be stored whenever they change.

**Check:** Every setting that writes to storage: does it write when the
value equals the default? It shouldn't; it should remove the key. One
helper that names the default once and stores nothing for it, used by
every setting, makes this hold everywhere.

### 13. When access ends, take back what was handed out

**Ask:** When someone loses the right to see something (removed from an
organization, signed out, a channel made private), what are they still
holding: an open connection, a live store, a cached copy?

**Usually:** Revoke at the moment the right ends: close the connections or
subscriptions that carry it, and let the client ask again, so every check
runs afresh. A check at the door only guards new requests; it does nothing
about what is already streaming.

**Why:** Permission checks are usually written at the point of asking. Live
data is handed out once and then flows, so a check that passes once keeps
passing in effect until the connection ends.

**Except:** data that is public to everyone who could have held it (an
organization's name shown on an invite page) needs no revoking.

**Check:** For every place that removes a right, name what was handed out
under it and what takes it back.

### 14. A global holds what a view owns only with a release

**Ask:** Is this module-level (`lazy let`) state holding something that
belongs to a page, a row or a session: a callback, an element, a pending
request, a listener?

**Usually:** Keep it in the view instead. If it has to be global (it's
reached from outside the view, like "react to the hovered message"), the
view that writes it clears it when it goes (`get_owner().defer(..)`).

**Why:** A global outlives every view. A pending request fires on some
later, unrelated page; a stored closure keeps a whole closed page alive; a
listener keeps running after sign-out. Nothing fails at first, which is
why these survive review.

**Except:** state that genuinely belongs to the whole program (the app's
panels while signed in, the stores) is global by design; say so where it's
made, and still release it with the session.

**Check:** Every `lazy let` and every `Shared` written from inside a view.
For each, name what clears it.

### 15. One definition per rule

**Ask:** Is this rule (who may do what, what an address looks like, how a
link is spelled, what counts as valid) written in more than one place?

**Usually:** Define it once, where both sides can import it (a pure module
shared by client and server), and use that everywhere:
`may_remove(role, ..)` rather than six copies of
`role == "owner" || role == "admin"`.

**Why:** Copies of a rule drift. The server enforces one version and the
client shows another, so a button appears that the server refuses, or a
link the server writes points at an address the client renamed. Principle
1 is the same idea for data; this is it for logic.

**Except:** a check repeated on purpose at a trust boundary (the client
hides a button, the server refuses the call) is fine, as long as both sides
call the same definition.

**Check:** Search for the literal values a rule turns on (`"owner"`, a
link scheme, a route prefix): more than one file defining or comparing
them is a rule with several owners.

### 16. Measure before fixing a cost

**Ask:** Where does the time actually go, at the size where it hurts?

**Usually:** Build the large case (a grown copy of the data, a long
document), time it end to end, and profile it before changing anything.
Fix what the profile names, then time it again. Record both numbers with
the change.

**Why:** A cost found by reading code is a guess about where the time goes,
and the guess is often about the wrong part. In Kolt, an audit blamed
per-region derivations in a 300-block document. The profile showed that a
document is one region, and that the time went to spelling's lookups per
word.

**Except:** a cost that's plainly quadratic in review (a query per row, a
list copied per push) can be filed without measuring. Measure before and
after the fix, though, so the fix is known to matter.

**Check:** A performance commit without before and after numbers, or with
numbers from data far smaller than real use, hasn't been checked yet.

### 17. A rewrite for speed proves it gives the same answer

**Ask:** Does the new code produce exactly what the old code did, on real
data and on data large enough to hit every branch?

**Usually:** Run the old and the new code side by side for a while and
compare everything they produce, sorted into a canonical text. Then break
the new code on purpose once, to see the comparison catch it. Remove the
old code only after that.

**Why:** A rewrite that batches or reorders work (one query per table
instead of per row, a cache instead of a recomputation) changes how the
answer is built while claiming not to change the answer. Ordering,
duplicates and empty groups are where such rewrites go quietly wrong, and
nobody looks at those by eye.

**Except:** a change too small to hide a difference (one query given an
index) needs only the timing.

**Check:** The commit says what was compared, on what data, and that the
comparison could fail.

### 18. Know where a value is copied

*New in this book; the case is from Kolt, through the Vilan tracker.*

**Ask:** When this code reads a value out of a `Shared` cell (or a store
built on one), does it get its own copy? How big is what it copies, and how
often does it run?

**Usually:** Look at the value inside the read, and hand out only what you
need. Values are copied where they cross: a capture handed out of the
frame, a value returned, a value stored into a construction. A pattern
over `cell.read()` whose captures are only looked at reads the cell in
place, on both backends (M199).

**Why:** Vilan has value semantics (rule 1 of [the memory
model](../spec/memory.md#61-rule-1--values-are-copied-copies-are-semantic)):
a copy is the language keeping a promise, not waste the compiler failed to
remove. A function that returns a cell's map hands every caller a map of
its own, and that is cheap for ten entries and ruinous for a hundred
thousand. The cost also differs by backend, so code that is fine on one
can be slow on the other.

```vilan
import std::shared::Shared;

let loaded: Shared<Option<List<str>>> = Shared::new(None);

// Copies: the list leaves the read, so the caller gets its own.
fun word_list(): Option<List<str>> {
	if loaded.read() is Some(let words) {
		ret Some(words);
	}
	None
}

// Reads in place: the list is only looked at, inside the read.
fun knows(word: str): bool {
	if loaded.read() is Some(let words) {
		ret words.contains(word);
	}
	false
}

fun main() {
	loaded.write() = Some(["ada", "bob"]);
	print(knows("bob"));               // true
	print(word_list().is_some());      // true
}
```

**Except:** a small value, or one read rarely, can be copied freely; a
copy is simpler to reason about than a view. And a `Shared` held only to
"avoid a copy" on a hot path is the wrong tool ([`Shared<T>`: one cell,
many holders](../tour/memory-model.md#sharedt-one-cell-many-holders)).

**Check:** A function that returns what it read from a cell, called in a
loop or per keystroke: measure it at real size, on the backend you ship
(principle 16). Some forms still copy today even when they only look (a
guard clause `cell.read() is Some(let x) else ret;`, a subscript
`cell.read()[i]`); see Known gaps.

### 19. Absence and failure at a boundary have a type

*New in this book; the cases are from the Vilan tracker, one of them met
in Kolt.*

**Ask:** What can the outside world hand this code that the program never
wrote: a NULL column, a missing field, a value the database or the wire
can't carry? Does the type say so?

**Usually:** Read what may be absent as an `Option`, and send only what
the boundary's type admits. A nullable column reads through its
`optional_` reader (`optional_text`, `optional_integer`, …), which answers
`None` for NULL; a parameter list is a `List<dyn SqlValue>`, so an
`Option` binds NULL and anything else the database can't hold is a compile
error at the element. See [std::db](../std/process.md#stddb-sqlite).

```vilan,norun
import std::db::Database;

fun main() {
	let db = Database::open(":memory:");
	db.exec("CREATE TABLE task (id INTEGER PRIMARY KEY, edited_at INTEGER)");
	let never_edited: Option<i53> = None;
	let id = db.prepare("INSERT INTO task (edited_at) VALUES (?)").run([never_edited]);
	match db.prepare("SELECT edited_at FROM task WHERE id = ?").first([id]) {
		Some(let row) => match row.optional_big_integer("edited_at") {
			Some(let at) => print(i"edited at {at}"),
			None => print("never edited"),
		},
		None => print("missing"),
	}
}
```

**Why:** A boundary that has no word for "absent" invents one, and each
host invents a different one: JavaScript's `null`, a native `0` or `""`.
The program then carries a value nobody wrote, and the failure shows up
later, far from the read. A value the boundary can't carry fails at run
time, on the path nobody tested.

**Except:** a column the schema declares `NOT NULL` reads through the
typed reader; if it ever meets a NULL, the panic names the column, which is
what you want.

**Check:** Every column the schema leaves nullable is read through an
`optional_` reader, and every value from JSON or the network is decoded
into a type before the program uses it (principle 7).

### 20. State the limits you rely on

*New in this book; the cases are Kolt's and the Vilan tracker's.*

**Ask:** Which limits does this feature depend on: the largest body it
accepts, how long it waits, how many it holds? Where are they written?

**Usually:** In your code, at the place that relies on them: a route that
takes uploads sets its own body cap (`Request::max_body_bytes`, or
`max_body_bytes_under` for a mount); a server that serves slow clients or
long polls sets its deadlines (`headers_timeout`, `request_timeout`,
`keep_alive_timeout`). See [the server](../std/process.md#stdhttp-the-server).

**Why:** A limit inherited from a default is a limit nobody decided. When
the default changes, or the program moves to a host whose default differs
(Node and a native binary, one operating system and another), the program
changes behavior with no change in its code. And a limit nobody set may
not exist at all: a server with no deadline holds a stalled connection
forever.

**Except:** a default the program truly doesn't care about can stay a
default. The test is whether a feature would break if it changed.

**Check:** For each route and each server, write down the largest input
and the longest wait it is built for, and find each in the code.

## Cases

Each case: the symptom, the root cause, the fix, and the principles
involved. The cases up to "Two copies by effect that were left alone" are
Kolt's; the rest are from the Vilan tracker, named by item.

### The switcher's My work lagged on close

- **Symptom:** Opening and closing the panel switcher's My work menu was
  slow, and got slower with every task.
- **Root cause:** It set up a hidden effect per open task to watch its
  assignees and title, and each effect rewrote the whole result list.
  Building or tearing down the menu did work proportional to the number of
  tasks squared.
- **Fix:** The server keeps an `assigned` list on the account store,
  updated where tasks are assigned, moved, renamed or deleted. The menu
  reads one short list.
- **Principles:** 1, 3, 4.

### Overlays weren't clamped to their container

- **Symptom:** Offsetting the overlay container showed menus spilling
  outside it.
- **Root cause:** One placement strategy never clamped, and the placement
  math lived inside browser-only code, so it had never been tested.
- **Fix:** The placement math moved to a pure module with tests, and every
  strategy clamps.
- **Principles:** 9, 10.

### A resize divider drew over the settings modal

- **Symptom:** With resize handles on "While Shift is held", panel dividers
  showed through modals.
- **Root cause:** The divider got `z-index: 1` to sit above its neighbor's
  content. That ranked it above the overlay layer, which comes later in the
  page with no z-index of its own. Stacking is global.
- **Fix:** Drop the z-index; `position: relative` alone already raises it
  above the neighbor's ordinary content.
- **Principles:** 10.

### Image links in sent messages pointed nowhere

- **Symptom:** In a sent message, a link to one of its images did nothing
  on hover.
- **Root cause:** The editor names images by its own counter. The links
  were saved with those numbers, which mean nothing once the editor is
  gone.
- **Fix:** Sending rewrites image links to the upload's id; editing a sent
  message rewrites them back to the editor's own.
- **Principles:** 6.

### Re-entering an organization showed the wrong tab

- **Symptom:** Returning to an organization showed its first tab instead of
  the one last used.
- **Root cause:** The organization page redirected to its channel directory
  before the tabs had arrived, treating "not loaded" as "no tabs".
- **Fix:** Redirect only after the tabs have arrived, and only if the
  address is still the organization's root. A tab that was just opened is
  likewise not treated as closed until the server's list has included it.
- **Principles:** 5.

### Tabs from two devices could overwrite each other

- **Decision:** Tabs are stored as one row each, not as one JSON blob per
  account.
- **Why:** A blob is replaced whole, so two devices saving at once lose one
  side's changes. Rows change one at a time. Store at the same granularity
  the data changes at.
- **Principles:** 1.

### Pages swapped out of a panel stayed alive

- **Symptom:** None visible; Vilan 0.53 warned about it (B677).
- **Root cause:** The function that builds a panel's page was typed as a
  plain closure, so pages were built under the shell's owner and lived
  until sign-out.
- **Fix:** Type it `(sync |str| View) context (owner_scope, page_focus)`,
  so each page belongs to its panel.
- **Principles:** 2, and the language taking over a rule: since Vilan 0.54
  a closure parameter answering a `View` carries `owner_scope` by default,
  so `(|str| View) context page_focus` now means the same thing and the
  plain closure type is no longer a trap.

### Stray gaps in buttons and menus

- **Symptom:** Extra space in buttons, dropdowns and menu rows, reported
  many times.
- **Root cause:** Empty placeholder elements (`<span />`) still take a flex
  gap.
- **Fix:** Use std's forms of nothing; std gained `ui::nothing()` and
  `ui::each_owned` in Vilan 0.53 (M170), and Kolt's hidden-span helper was
  removed.
- **Principles:** 8, 11, and the language taking over a rule.

### Back and forward relied on timestamp marks

- **Root cause:** std's router had no way to tell a back/forward visit from
  a navigation, so Kolt stamped each history entry and compared stamps.
- **Fix:** Once std added `current_entry().popped` (M167, Vilan 0.53), Kolt
  dropped its stamps.
- **Principles:** 8. The workaround was right while the gap existed; the
  case is that it was filed, so it could be removed.

### Two scrollbars in a shrinking modal

- **Symptom:** The command palette showed two scrollbars when the window
  was short, and shrank instead of sliding up first.
- **Root cause:** A flex child's minimum height defaults to its content's,
  so the inner list never shrank and the panel scrolled as well.
  Separately, the placement shrank a pinned panel before using the room
  above it.
- **Fix:** `min-height: 0` on the overlay panel's children and the lists;
  the placement slides before it shrinks.
- **Principles:** 10 (the layout constraint, not the scrollbar, was the
  bug).

### A new default didn't reach anyone

- **Root cause:** "Open links" stored whatever the setting showed, so
  everyone who had opened Settings had the old default saved as their
  choice.
- **Fix:** The setting moved to a new storage key when its default changed.
- **Principles:** 12.
- **Later:** the cause was removed for every setting. A setting's key is
  made by one helper, which names the default once and stores nothing for
  it, so going back to a default forgets the choice and no default can be
  kept as one. Before, about ten settings stored whatever they showed, and
  each default was written twice (reading and writing), free to drift. A
  value an older build stored stays until that setting is next written.

### Uploads over 1 MiB failed after a Vilan upgrade

- **Root cause:** Vilan 0.53 caps request bodies at 1 MiB unless the app
  raises the cap (M164). Kolt relied on the old unlimited default without
  saying so.
- **Fix:** The upload handler and another mount that takes large bodies set
  their own caps.
- **Principles:** 10, 20. A limit your feature depends on belongs in your
  code, stated, not inherited from a default.

### Private channels saw an empty organization

- **Symptom:** In private channels and DMs, a task link opened the wrong
  task, and hover cards said tasks and documents "no longer exist".
- **Root cause:** A private channel's page was built from that channel's
  own store, which holds its messages and people but none of the
  organization's tasks, documents or members. Two facts (the channel's
  messages, the organization's things) were read from one place that owned
  only the first.
- **Fix:** The page takes both stores and reads each fact from its owner.
  Later, a private channel's store got a type of its own, with no
  organization-wide fields, so reading them there can't compile.
- **Principles:** 1, 7. The first fix corrected the reads; the second made
  the wrong read impossible to write.

### A removed member kept streaming the organization

- **Symptom:** None visible: the app hid the organization once the
  account's list of organizations changed.
- **Root cause:** The organization's live store kept streaming to the
  removed member's open tabs. Membership was checked when the store was
  asked for, and never again.
- **Fix:** Removing a member, leaving and signing out close the account's
  connections; its apps reconnect and are checked afresh.
- **Principles:** 13.

### An outage signed people out

- **Symptom:** If the server couldn't be reached at startup, the app went to
  the login page and lost the address it was opening.
- **Root cause:** "No client" stood for both "signed out" and "couldn't
  connect". The redirect treated the second as the first.
- **Fix:** Connecting is a state of its own (signed out, connecting,
  unreachable, ready). Only "signed out" goes to the login page;
  "unreachable" says so and retries.
- **Principles:** 5, 7 (a state with three meanings deserved a type).
- **Also learned:** retries started from `main` held the whole app back
  until the server answered; they now start from the mounted app. The
  explanation first recorded was that an `async` block is waited for by the
  `async` function that starts it. That is not the language's rule: a spawn
  does not suspend, and only a `nursery` waits for the tasks started inside
  it ([§7.3](../spec/execution.md#73-the-async-model),
  [§7.7](../spec/execution.md#77-nurseries)). The lesson that holds: work
  that should not delay the first paint does not belong in anything `main`
  waits for.

### A shortcut recording outlived its settings row

- **Symptom:** Close Settings while recording a shortcut, and the next key
  pressed anywhere was swallowed and bound to that action.
- **Root cause:** The recording's key listener was held in a `Shared` and a
  fresh `Owner` that nothing disposed, instead of the row's owner.
- **Fix:** The row stops the recording when it goes.
- **Principles:** 2, 14.

### The username stayed old in private channels

- **Root cause:** Each private channel's store keeps its own copy of its
  people. Renaming updated the organization stores, but not those copies.
- **Fix:** Renaming also updates every live private store that names the
  account.
- **Principles:** 1. The copies are a cache; the lasting fix is one place
  that answers "a user's record changed" for every store.

### An organization took seconds to load

- **Symptom:** On a copy of an organization grown to 2,072 tasks and about
  24,000 messages, the first connection to the organization waited 2.7 s on
  the server.
- **Root cause:** The loader ran about ten queries per task, and per channel
  one for its messages, one for each message's reactions, and a full scan
  for its thread source (the thread column had no index).
- **Fix:** One query per table for the whole organization, grouped in
  memory, plus the two missing indexes: 130 ms. The new state was compared
  with the old loader's on that copy and on both real organizations, and
  the comparison caught a deliberately reordered query.
- **Principles:** 3, 17. Grouping hit a std gap at the time (M182: a map's
  value couldn't be changed in place), so the rows were ordered by key in
  SQL instead. The gap is closed since Vilan 0.54: `HashMap::slot(key)`
  hands out the entry itself, so grouping is
  `groups.slot(key).get_or_insert([]).push(row)`, one step per row (see
  [`HashMap`](../std/collections.md#hashmapk-v)).

### A document's comments showed as channel messages

- **Symptom:** In "Linked from", a link inside a comment on a document
  showed as `#` with no name, and opened as a channel.
- **Root cause:** Backlink kinds and channel kinds were strings matched
  with a catch-all. Nobody had written a branch for `"note"` (a document
  comment's channel), so it fell into the channel case.
- **Fix:** Both are enums now, matched exhaustively. The compiler pointed
  at the missing case, which became "Comment on" and the document's name.
- **Principles:** 7.

### Every key re-read the document

- **Symptom:** Typing in a 300-paragraph document took 71–85 ms per key.
- **Root cause:** An audit guessed per-region derivations. A profile showed
  that the document is a single region, and that spelling looked up each
  word's code and link status from the document's start: O(words × runs)
  on every key.
- **Fix:** Code and link spans read in one pass and walked beside the
  words, plus per-region change counts so other regions keep their work:
  24–28 ms.
- **Principles:** 3, 16.

### A client's address could be anything it said

- **Symptom:** None yet; caught while adding sign-in throttling for the
  first hosted server. Limits per address would have been worthless.
- **Root cause:** Behind a TLS proxy every request comes from the proxy, so
  the client's address has to come from `X-Forwarded-For`. But a client can
  send that header itself, and the first entry is whatever it wrote. Only
  the entry the proxy appended (the last) means anything, and only when a
  proxy is really in front.
- **Fix:** The socket's address, unless a setting says a proxy is in
  front, and then only the last entry. The deployment turns the setting on
  because the proxy is the only way in. A test with the setting off forged
  eleven addresses and was still refused on the eleventh failure.
- **Principles:** 6 (an identity is only as good as whoever wrote it), 7.
- **Also learned:** Node as a container's PID 1 ignores `SIGTERM`, so
  `docker stop` waits out its timeout and then kills the server. Run it
  with an init process (`--init`, or `init: true` in compose). std now
  answers the socket's own address as `Request::peer_address()` (F163,
  unreleased at the time of writing).

### Two selections on the page at once

- **Symptom:** Selecting text in one sent message, then in another, left
  both selected, and nothing could select from one message into the next.
- **Root cause:** Every editor (each message, the draft, each title) had a
  text controller of its own, each with its own selection. The page has
  one selection, and there were dozens of copies of it. A first fix made
  each controller drop its selection when another took one; that patched
  the symptom and kept the copies.
- **Fix:** One core for the page: one selection, one caret, one textarea,
  one drag. Each editor is a host on it with its own hooks and undo
  history. The core writes each host's view of the selection directly and
  pushes each region's share only when it changes, so a keystroke costs the
  same with 3 messages on the page as with 150 (0.70 ms, from 0.95 ms).
- **Principles:** 1 (the selection is one fact), 3.
- **Also learned:** `.memo()` doesn't compare: every upstream write
  notifies. Use `.distinct().memo()` to stop a view from waking its
  readers ([sealing and
  `distinct`](../std/reactive.md#sealing-memo-cell-sample--and-distinct-the-stage-that-compares)).
  A `lazy let` can't read an ambient context such as the owner (the
  compiler refuses it), so build such a global from the first caller,
  under an `Owner::new()` of its own.

### A word's aliases included a function's source

- **Symptom:** None reported. In the command palette, searching "native" or
  "object" could match anything written with the word "constructor".
- **Root cause:** The search dictionary's generator, then a JavaScript
  script, looked irregular plurals up in a plain JavaScript object
  (`IRREGULAR_PLURALS[word] ?? ..`). For the word `constructor` that finds
  `Object`'s own constructor, so its alias became the text
  `function Object() { [native code] }`.
- **Fix:** Found when the build steps were rewritten in Vilan, where a
  lookup in a list of pairs has no inherited entries. The new steps'
  outputs were compared with the old ones': the spelling list and all
  1,791 icons were identical, and the dictionary's 702 files differed in
  that one line.
- **Principles:** 17 (the comparison is what showed it; nobody would have
  read 69,832 lines), 7 (a table with a fixed set of keys is not a host
  object).

### The client offered what the server refuses

- **Symptom:** Every message's menu had Delete; a member choosing it on
  someone else's message got the server's refusal as a toast. And a long
  organization name could suggest an address ending in `-`, which the
  server turned down.
- **Root cause:** Each rule was written once per side. The server's "own,
  or an admin" for messages was spelled out by hand next to the shared
  `may_remove` the task pages already used. The address rule lived in the
  server (`valid_slug`) and its suggestion in a page file, cutting at 40
  characters without looking at what the cut left.
- **Fix:** The message menu, the templates list and the server all call
  `may_remove`. `valid_slug` and `suggest_slug` sit together in a module
  both sides import, with a test that whatever is suggested is valid.
- **Principles:** 15, 9 (the suggestion was pure code in a view file, so it
  had no test to fail).
- **Also learned:** a rule's refusal belongs with the rule. The validators
  answered only "no", so each caller restated the limit in its own message
  (twelve times, for six limits). A rule type that holds the limit and says
  the refusal replaced them, and the other rules followed the same way: an
  upload's address, which pictures are pictures, whose a channel is to
  manage, what a body may hold (which the editor now asks before sending).

### A session was found by its subject alone

- **Symptom:** None reported; found while giving editing sessions'
  subjects a type. While a document, a task or a section was being edited,
  a member of any other organization who named it (ids are numbered across
  organizations, so they can be guessed) was handed its live session: the
  text, and the key that lets a change in.
- **Root cause:** Sessions were kept in a map by subject (`"doc:12"`).
  Every call checked that the caller was a member of the organization
  *they named*, then looked the session up by the subject alone. Only
  starting a session read the subject from that organization; finding a
  running one didn't.
- **Fix:** A session is kept under its organization and its subject
  together, so the organization that was checked is part of what is looked
  up. Resolving a comment's region checks that the document is the
  organization's too.
- **Principles:** 6 (an id means something only together with what it is
  an id *in*), 13.
- **Also learned:** the subjects were `"kind:id"` strings built in fourteen
  places and split apart in one. As a type, every place that takes one
  apart had to be visited, which is how the lookup was read at all. Giving
  a convention a type is also an audit of everything that relied on it.

### Two copies by effect that were left alone

- **Symptom:** None. An audit against principle 4 named two places as
  likely bugs: the task number maps (two maps an effect copies from a list,
  read by derivations of that list), and the theme picker (an effect resets
  the selected row, a second one copies the row's theme).
- **What was checked:** Both suspicions were about order, so the order was
  tested in a small program rather than argued. Effects run in the order
  they were made. Setting a cell to the value it already has still
  notifies. So the maps are current for every reader (the effect is made
  in the constructor, before anything can hold the struct), and the
  picker's second effect does run when the filter narrows.
- **Decision:** Left as they are. A derived map would be copied on every
  lookup (principle 3), and the picker's second effect is an edge: it
  previews the theme after a delay. The maps' constructor now says what
  makes it safe.
- **Principles:** 4, and where it stops: an effect that copies is a bug
  when a reader can run before it. Where construction rules that out, it is
  a cache with a stated owner (principle 1), and the comment is the fix.
- **Also learned:** an audit's "plausible" is a hypothesis. Two of five
  were wrong, and testing the mechanism took less time than the rewrite
  would have.

### A dictionary copied per word

*Kolt's, filed and fixed in Vilan as M199.*

- **Symptom:** Kolt's spell-check dictionary, built natively, took 13.2 s
  of release CPU for 500 words, against 4 ms on JS.
- **Root cause:** A `match` or `if .. is` over `cell.read()` copied the
  payload even when its arms only looked at it, and the native backend's
  copy of a 123,702-entry map is a deep one. Every word checked copied the
  whole dictionary.
- **Fix:** In Vilan: a pattern over a `Shared` read whose captures are only
  viewed now reads the cell in place on both backends (native release
  0.465 s to 0.028 s on the tracker's 100,000-entry measure; unreleased at
  the time of writing). In the
  program: check the word inside the read
  (`if loaded.read() is Some(let found) { found.has(word) }`), which copies
  nothing on either backend. A function that returns the captured map still
  copies it, as the memory model requires; on JS that shape went from
  0.18 s to 25 s for a 100,000-entry Vilan `HashMap` once the copy was
  made correctly.
- **Principles:** 18, 16 (measure on the backend you ship), 10 (the fix
  belonged in the compiler, and was filed there).

### A NULL read as a number

*From the Vilan tracker: F149.*

- **Symptom:** `row.integer("edited_at")` over a NULL column answered
  `null` behind an `i32` on JS, and `0` in a native build. The same program
  printed different things on the two backends, and neither was a value the
  program wrote.
- **Root cause:** The typed readers had no word for "absent", so each host
  supplied its own.
- **Fix:** A typed reader now refuses a NULL with a panic naming the column
  and its `optional_` twin, the same on both backends; the `optional_`
  readers answer `None` for NULL. A program migrates by reading each
  nullable column through its `optional_` reader. (Unreleased at the time
  of writing; breaking.)
- **Principles:** 19, 5 (absent is not zero).

### An `Option` bound as a parameter

*Kolt's, through the Vilan tracker: M165 and M175.*

- **Symptom:** A form did nothing. The server's `[rpc]` body had thrown
  node's `TypeError: Provided value cannot be bound to SQLite parameter 8`
  on an `Option` bound as a statement parameter, and the client never
  heard: its call waited forever.
- **Root cause:** Two layers. The parameter list took any value, so a
  value SQLite can't bind was found only at run time. And a throwing
  `[rpc]` body killed its task without answering the call.
- **Fix:** M165: a throwing `[rpc]` body answers its call with a `Remote`
  error naming the method, and the server logs it. M175: a parameter list
  is a `List<dyn SqlValue>`, so an `Option` binds NULL and a value that
  can't be bound is a compile error at the element (unreleased at the time
  of writing; breaking).
- **Principles:** 19, and the language taking over a rule: the run-time
  refusal became a type.

### A stalled client held its connection

*From the Vilan tracker: F152.*

- **Symptom:** A client that sent half a request head, or declared a body
  and stalled, held its connection on a native server forever, and a
  handler waiting on that body waited with it.
- **Root cause:** The native HTTP server had no deadline on a request's
  head or body. Node has them by default (60 s for the head, 300 s for the
  request), so a program that never named a deadline had one on one backend
  and none on the other.
- **Fix:** The native server keeps Node's deadlines and defaults, and
  `ServerBuilder`'s `headers_timeout`, `request_timeout` and
  `keep_alive_timeout` set them on both backends; a missed deadline is
  answered `408 Request Timeout` (unreleased at the time of writing).
- **Principles:** 20. A program that relies on a deadline (or on its
  absence, for a long poll) says so; see [native
  binaries](native.md#what-it-reaches-and-what-it-refuses-by-name).

## Known gaps in this page

- The diagnosis tables' order is a guess from a few weeks of Kolt's
  history. It should be reordered by how often each cause turns out to be
  the real one.
- One case shows where a principle stops (two copies by effect that were
  left alone), and one shows a case's recorded explanation being wrong (an
  outage signed people out); none yet shows a principle wrong outright.
- Most cases come from a few weeks of Kolt work, so they lean toward UI,
  reactivity and the client/server split. Server-side data modeling,
  security and concurrency need cases of their own.
- Principle 7 (types, not conventions) has one case of its own (a
  document's comments showed as channel messages); the rest of its support
  is in cases filed under other principles.
- Principle 18 has open edges in the compiler. Some read-only forms still
  copy (a guard clause, a subscript in the read, a call through a closure
  value), and on JS a function that returns a bare read of a module-level
  cell hands out the cell itself rather than a copy, so mutating the result
  changes the cell (B735, a miscompile, open). Until B735 is fixed, don't
  mutate what such a function returns; copy it first.
- Principles 19 and 20 rest on cases from the Vilan tracker more than from
  an app. They need cases from a program's own history.
- The principles are ordered roughly by how expensive their mistakes are,
  but the order hasn't been tested.

## Testing whether it works

Give a fresh agent, or a new programmer, the same task twice: once with
this page and once without. Grade both versions blind. If the version with
the page isn't consistently better, the entries are too vague to act on,
and whatever separates good code from what was written is still unwritten.
