# Working on the Vilan compiler well

A working reference for people and agents changing the compiler, std, the
runtimes and the tools: the principles behind good decisions here, and the
cases they came from. It does not promise the best change every time. It
makes the usual mistakes rare and the remaining ones easy to see.

It has a sibling for people writing Vilan programs: [Writing Vilan programs
well](vilan/docs/guide/wisdom.md) in the book. That page's principles are
numbered 1 to 20; this page's are C1 onward, so the two never collide. Where
one of the book's principles applies here unchanged, this page points to it
rather than restating it.

`CLAUDE.md` is the contract and `AGENTS.md` the map and the scar tissue.
This page does not repeat their rules. It gives the ones with a history the
Ask / Usually / Why / Except / Check form, and the cases that taught them.
Where this page and `CLAUDE.md` seem to disagree, `CLAUDE.md` wins.

Solving a problem takes three kinds of reasoning, and this page has a part
for each:

- **Diagnosis** (abduction: guessing the explanation). A problem arrives as
  a symptom: a moved golden, a red leg, a slower check. This part maps
  symptoms to their likely causes, so the guess is a short list to check
  rather than an open question.
- **Principles** (deduction: working out what follows). The questions to
  ask, each with its usual answer, the reason for it, the conditions it
  holds under, and a way to check it.
- **Cases** (induction: learning from evidence). Real problems from this
  repository, each with its symptom, its root cause, the fix, and the
  principles it rests on, named by tracker item and commit.

## Terms

The same five qualities the book's page uses, read for a compiler.

- **Readability:** how well the code can be read *and understood* with
  little context. A pass whose name says what it decides, and whose
  comment says why, is readable; a `_ =>` arm that quietly handles a
  variant nobody thought about is not.
- **Strength** (robustness): how few bugs the code has, and in particular
  how few hidden ones. In a compiler the hidden ones are miscompiles: a
  program that checks clean and does the wrong thing.
- **Maintainability:** how few bugs a change introduces. A change to one
  pass shouldn't break another it didn't touch; the gates are how that is
  known rather than hoped.
- **Writability:** how strong code is when it's written quickly. The
  harness (`assert_compiles_and_runs`, the differentials, the generated
  tables) is what makes the quick way the correct way.
- **Consistency:** a pattern established in the codebase is reused
  everywhere it applies, and a better one replaces it everywhere, with
  every remaining place tracked until it does.

## How to use it

**Fixing something:**

1. **Find the symptom in the diagnosis tables.** Check its causes in order.
2. **Confirm the cause before fixing it.** Say why the bug happens in one
   sentence, and test that sentence: a five-line probe through a freshly
   built binary, a measurement, or the code that decides it (`AGENTS.md`,
   "Probe before you assert").
3. **Fix the general path** (C1), pin each case (C2), and run the cheap
   gate before the expensive one (`CLAUDE.md`, "Running the suite").

**Building something:**

1. **Settle the semantics on paper first** (`CLAUDE.md`, "Prove a feature
   before implementing it"). A proposal in the `proposals` repo, ruled,
   then code.
2. **Ask the principles' questions of each part.** Does both backends'
   answer follow from the design (C6)? Does every fact it computes survive
   the cache (C7)? What does the user read when it refuses (C9)?
3. **Write the pins before the code where you can.** A pin written after
   the fix has to be planted red to be trusted (C2).

**Either way, afterward:** add a case when the problem taught something,
and a row to a diagnosis table when a symptom led somewhere it doesn't list
yet.

The strongest form of a principle is one a tool enforces: a gate in the
suite, a CI leg, a generated table, a lint. Several of the rules in
`AGENTS.md` started as advice and became a test after they were broken
(`grammar_sync.rs`, `agents_map.rs`, the poisoned-lock scan, the
`shared_census`). When a principle here keeps being broken, make it a gate.

## Diagnosis

Symptoms, with their likely causes and where to read more. The order is a
starting guess from the tracker's history, not a count; reorder it as cases
accumulate.

### A golden moved

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| A corpus `.mjs` golden differs and you didn't touch codegen | A stale debug binary (rebuild before regenerating); a std change that renumbered temps | C10 |
| Goldens differ only in temp names or digits | A deliberate change whose output is identical; verify the program's output, then say so in the commit | C10; The formatter reformatted the estate |
| One golden differs in what the program does | A miscompile: stop and report, never regenerate | C10, C2 |
| The anchor golden differs | A book heading moved; every changed line is a changed URL | C10 |

### Green locally, red in CI

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| Red only on Windows | A platform default: `::` is IPv6-only, a test thread's stack runs out sooner, a slow runner's timer goes negative | C8; `::` on Windows, The walk overflowed on Windows, A negative delay |
| Red only on another node version | The test relied on host behavior nobody stated (how writes are framed) | C8; The probe relied on node's framing |
| Red on a tree nobody edited | A floating toolchain; a new security advisory | C8, C11; The ceiling moved with rustc, The audit leg caught a dependency |
| A perf row just past its ceiling | A real cost: fix it, or add a `[[bump]]` row with its reason in the same commit | C4 |
| A `#[cfg(windows)]` pin fails | It was only ever compiled here; CI is its first run | C3 |

### A fixed bug came back

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| The same failure, one function over | The instance was fixed, not the class; the check that found it runs too late | C1, C3; The playground crashed twice on one clock |
| A closed item regressed | Pinned per example, not per case; the pin was never seen red | C2 |
| A workaround in std outlived its bug | The bug was fixed and the workaround's comment named no item | C1; Instantiations shared a temp |

### Native and JS disagree

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| The same program prints different things | One backend decided a semantic alone: a host coercion, a host default, an evaluation order | C6; A NULL had two answers, A block ran before its siblings |
| rustc refuses the emitted Rust | The analyzer recorded a fact one emitter reads and the other ignores; a JS-only shape reached the native emitter | C6 |
| Native and node differ only under load or on one OS | A host default one side has and the other lacks (deadlines, binds, framing) | C8; A native server with no deadlines |

### Wrong only after an edit, or only in the editor

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| An incremental analysis answers differently from a clean one | A restored region lacks a fact another module's edit changed | C7; A keystroke kept a function async |
| A world or cache entry is served to the wrong program | The cache key misses an input the answer depends on | C7; A std world served to the wrong program |
| The language server degrades over a session | Retained state per open document, not the suspect you read in the code | C4; The prime suspect was refuted |

### Slow

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| `vilan check` or the editor is slower after a merge | A regression the gates didn't count; measure instructions, not wall time | C4; A 3.3× regression shipped green |
| A cost found by reading the code | A guess; often the wrong part | C4, book 16; A cost read off a loaded machine |
| A rewrite for speed changes output | It was checked by eye | book 17 |

### Refusals and messages

| Symptom | Likely causes, in order | See |
| --- | --- | --- |
| A message advises something the reader can't do | The diagnostic was written for the common case, not the one the reader is in | C9; A message advised the impossible |
| Legitimate std, corpus or docs code trips a new check | A semantics-level event: stop and report (`AGENTS.md`) | C9 |
| A checker answer is wrong for a closure written without annotations | A deferred check answered once against an unfilled type | C5; An unfilled parameter answered once |

When no row fits, the problem may be new. Reason it out from the
principles, then add the row.

## Principles

### C1. Fix the general path, not the input

**Ask:** Why is the general path wrong for this input, and which other
inputs take the same path?

**Usually:** Fix the path. Find the invariant the code assumed and the
place it stopped holding, and fix it there, for every input that reaches
it (`CLAUDE.md`, "Fix root causes, not symptoms").

**Why:** A compiler's inputs are every program. A special case handles the
one in front of you, and the next program that reaches the same path a
different way gets the same bug, usually as a miscompile rather than an
error.

**Except:** when the general fix is a refactor that can't land now. Then
the special case says so in a comment naming its tracker item, and the
item names the general fix.

**Check:** Say the root cause in one sentence that doesn't mention the
reporting program. If you can't, you have described the input, not the
cause.

### C2. A fixed bug gets a pin per case, seen red

**Ask:** Which distinct cases does this fix cover, and does each have a
test that fails without the fix?

**Usually:** One pin per case, including the edge cases (multi-parameter,
nested, mixed, ordering-sensitive), in the subject module that owns the
area. Plant the bug back, watch each pin go red, restore (`CLAUDE.md`,
"Testing" and "Running the suite").

**Why:** A pin that was never red may not test the fix at all. A class
closed on one representative program reopens on the next shape.

**Except:** a case that can't be pinned on this machine (a
`#[cfg(windows)]` pin) is compiled here, verified by CI, and the report
says so by name.

**Check:** The commit names each pin, and says it went red with the fix
removed.

### C3. A check that doesn't run before shipping is not a gate

**Ask:** Where does this check run, and when: on every push, at the seal,
at the cut, after publication? On which branches and platforms?

**Usually:** In CI, on the path every change takes, before anything is
published. A check that exists only in a script someone remembers, or
that runs after the release, will be skipped exactly when it matters.

**Why:** A bug a gate doesn't cover comes back. The playground's crash on
`Instant::now` came back eighteen releases later, one function over,
because the only check that compiled through the wasm glue ran after
publication. A 3.3× regression shipped while every gate was green, because
no gate counted time.

**Except:** checks too expensive for every push (the perf seal, the full
kolt exhibit) run at the seal and the cut, and the cut refuses without
them unless the owner overrides by name.

**Check:** For a new gate, name the CI leg or the release step that runs
it, and the branches it runs on. A patch branch runs CI too.

### C4. A cost claim needs a measurement on the real exhibit

**Ask:** What was measured, on what, with which counter, under what load?
Is the exhibit the shape that actually hurts?

**Usually:** Count instructions (`instructions:u`, or callgrind where there
is no counter), on the exhibit the claim is about, and record before and
after. Wall time is not a measurement on a loaded machine: one input read
3.19 s to 13.54 s across ten runs of the same binary
(`proposals/projects/vilan/proposal/performance-gates.md`). A change that
legitimately costs more adds its `[[bump]]` row in the same commit
(`perf/budgets.toml`'s header).

**Why:** A cost read off the code, or measured on a stand-in, is a guess.
The guesses here have been wrong in both directions: a "~100 ms" pass whose
fix cost more than it saved, a "0.18 s to 25 s" regression that held only
for a map shape the real program doesn't use. This is the book's principle
16 for the compiler, and 17 applies unchanged to a rewrite for speed.

**Except:** a cost plainly quadratic in review can be filed without
measuring. Measure the fix anyway.

**Check:** The commit or report gives the counter, the exhibit and both
numbers. A number from a different exhibit is labelled as such.

### C5. A shortcut in the checker is a soundness claim: prove it

**Ask:** This answers early, skips a copy, or resolves by a second route.
What program would make that answer wrong?

**Usually:** Write the program that would break it, as a pin, before
arguing the shortcut is safe. A check that defers must be asked again once
its input is known; a copy elided must be one no reader can observe; a
lookup must go through the rule the language states, not a parallel one.

**Why:** A checker that answers too early accepts programs it should
refuse, and the result is a miscompile with no diagnostic: a string stored
in a `SignalCell<bool>`. A narrowing that sounds obviously right ("copy only
where the cell is mutated in place") can be unsound in a way only a pin
finds.

**Except:** none for soundness. A shortcut that can't be proved waits.

**Check:** For each shortcut, name the pin that would go red if it were
wrong. If there isn't one, write it.

### C6. Both backends decide a semantic, or neither does

**Ask:** Where is this behavior decided: in the language and std, or in
whatever node or the Rust runtime happens to do?

**Usually:** In the language: the analyzer records the fact, std states
it, both emitters read it, and a `native_differential` pin holds the two
outputs byte-identical. When a host does something the program can see (a
NULL, an evaluation order, a default deadline), std decides it for both.

**Why:** A semantic one backend decides alone is two semantics. The
program prints different things on the two backends, and neither is wrong
by its own lights, so nothing fails until someone compares.

**Except:** a host difference std states as one (node stores an untyped
column's `7` as `7.0`; the native server writes no `Date`) is documented in
the native guide rather than hidden.

**Check:** A change to emission, to std's host bindings or to `vilan-rt`
names its `native_differential` pin, or says why the construct is refused
natively by name.

### C7. Every fact the analyzer computes survives the cache

**Ask:** If this module is restored from the cache and another module is
re-analyzed, is every fact this pass reads still true? Does the cache key
include everything the answer depends on?

**Usually:** A fact derived from one region's own text is stored with that
region; a fact that depends on other regions is recomputed from every
region's contribution on every analysis
(`crates/vilan-core/src/analyzer/held_tables.rs` states the split). A cache
key names every input the cached answer read.

**Why:** An incremental analysis that answers differently from a clean one
is wrong only after an edit, only in the editor, and only sometimes.
These bugs are latent for months, because a clean build never shows them.

**Except:** none. Where a fact can't be partitioned, its region files no
cached rows and stays live.

**Check:** A new analyzer fact or cache gets a fixture in the edit-replay
differential (`edit_replay_differential.rs`), which holds an incremental
analysis to a clean one's answer, the edit undone included.

### C8. A default you didn't set is not behavior

**Ask:** Which defaults does this rely on: the OS's socket options, the
test thread's stack, node's framing or timeouts, the toolchain CI resolves,
the awk on the runner?

**Usually:** Set what you rely on, or pin it: `rust-toolchain.toml` for
rustc, an explicit bind option for a socket, an explicit deadline for a
server, an explicit turn for a write a test counts. This is the book's
principle 20 for the compiler.

**Why:** Defaults differ between platforms and move between versions. A
test that passed on one machine fails on another with no change to the
tree, and the failure points at the code, not the default.

**Except:** a default whose change would break nothing can stay one.

**Check:** For a red that only one platform or version shows, name the
default that differs before touching the code.

### C9. A diagnostic is part of the feature

**Ask:** When this refuses a program, what does the reader see, and can
they act on it from where they are?

**Usually:** The message names the construct, the reason, and a fix the
reader can make, in the house shape (`AGENTS.md`: match existing messages;
the diagnostics ledger holds them). A message's fix is tested like
behavior: an `assert_fails` with the wording, and the fixed twin
compiling.

**Why:** A refusal with the wrong advice costs more than one with none:
the reader follows it, and it can't work. A warning that every reader has
to answer the same way is a default waiting to be made.

**Except:** an internal-error diagnostic from a fence says only that the
compiler failed and the details are on stderr; that is its job.

**Check:** For each new refusal, write the program a user would have
written and read the message as that user.

### C10. A moved golden is a finding until it is explained

**Ask:** Why did this golden move, and does the program still do exactly
what it did?

**Usually:** An existing golden that moves is a bug or a deliberate,
verified change; stop and find which (`CLAUDE.md`, "Testing"). Rebuild the
debug binary before regenerating, and say in the commit which goldens
moved and that their output is identical.

**Why:** Goldens are the byte-identical record of what the compiler
emits. A regenerated golden that wasn't read turns a miscompile into the
new expected output.

**Except:** a deliberate estate change (a formatter rule, a renaming)
moves many at once; one commit, every file named, the output check run.

**Check:** The commit lists the moved goldens and how their output was
compared.

### C11. A dependency change is a code change

**Ask:** What does this crate bring with it: advisories, licenses, API
changes, build cost?

**Usually:** A bump runs the notices gate, `cargo audit --deny unsound` and
clippy, as `CLAUDE.md` lists. The audit leg in CI reads the advisory
database as it is today, so a crate that was clean yesterday can turn a
tree red with no commit.

**Why:** A dependency is code shipped to every user, written by someone
else, on their schedule. Its advisories are this project's bugs.

**Except:** the runtime crates take no dependency without a ruling
(`vilan-rt` takes none, by rule; `AGENTS.md`'s map says why each sibling
took the one it has).

**Check:** A dependency commit names the gates it ran and their exit
codes.

### The book's principles that apply unchanged

- **1. One owner per fact.** A table generated from the compiler's own
  lists (`grammar_sync.rs`, the MIME table, `book_sync.rs`) instead of
  copied by hand.
- **15. One definition per rule.** The same, for logic: one predicate for
  scalar views (`is_scalar_view_pointee`), not three.
- **16. Measure before fixing a cost** (C4).
- **17. A rewrite for speed proves it gives the same answer.** The
  differentials (`release_differential`, `parse_differential`,
  `infer_differential`) are this rule as a gate.

## Cases

Each case: the symptom, the root cause, the fix, the principles involved.
Tracker ids are in the `proposals` repo's tracker; commits are this
repository's.

### Instantiations shared a temp

- **Symptom:** In a generic `bump<H>` called at two types,
  `table.write().cells[id.as_usize()].users += 1` emitted
  `const $b = as_usize(id);` in the first instantiation and
  `const $c = $b;` in the second, which threw
  `ReferenceError: $b is not defined` (B713).
- **Root cause:** The subscript hoist kept each temp in a map by
  expression id, and nothing took the entry out. One expression id is
  emitted again for every instantiation of a generic body, every closure
  copy inside one, and every impl inheriting a default method, so the next
  emission found the last one's temp.
- **Fix:** The hoist returns the ids it entered and the assignment removes
  them once written (53135cdb, merged a6dc370e, v0.55.0). Five
  `inference::generics::b713_*` pins, red with the removal planted out,
  and a corpus program. A workaround in std's lens had named the bug; it
  was dropped with the fix. Eleven lens goldens moved with identical
  output.
- **Principles:** C1 (the map's lifetime was wrong for every emission, not
  one), C2, C10.

### Tuples laid out once per generic body

- **Symptom:** `Map::entries()` on a `Map<K, (A, B)>` handed back a pair
  the caller couldn't read, silently: one field held both values, the
  other `undefined` (B310).
- **Root cause:** The analyzer laid a tuple out once per generic body, with
  an abstract parameter counted as one slot; emission runs per instance,
  where tuples are flat. They diverged three ways: a return, an argument,
  a generic struct field.
- **Fix:** Both halves use the instantiated layout through one index path
  (25ca2972). Eight `b310_*` pins; five goldens moved, output identical.
  The same paths later fixed a native tuple-index miscompile.
- **Principles:** C1, C10.

### The playground crashed twice on one clock

- **Symptom:** v0.41.0's playground wasm answered every compile with
  `RuntimeError: unreachable`; formatting and completion worked, so the
  page looked alive (B432).
- **Root cause:** A base-cache wait read `Instant::now()`, which aborts on
  `wasm32-unknown-unknown`. v0.23.0 had shipped the same crash from the
  phase-timing marks; that one was fenced, and the website's deploy-time
  smoke caught it before it went live. By v0.41.0 that smoke was still the
  only compile through the wasm glue, and it ran at the fold, after
  publication.
- **Fix:** The wait's clock is not read on wasm32 (47b0b6aa, v0.41.1). N134
  made the wasm leg run the compiler it built, a node smoke on both the
  local CI script and the release job (48ce5efb). N135 made a
  `release/**` branch run CI on push, since a patch branch's PR conflicts
  with `next` by construction (60c2c6e5).
- **Principles:** C3 (the check existed and ran too late), C1 (the fence
  covered one function, not the clock).

### A 3.3× regression shipped green

- **Symptom:** v0.42.0 shipped a 3.3× CPU and 3.9× memory regression on
  kolt with every gate green (`performance-gates.md` §0). Weeks later,
  `next` was 25–30% slower than v0.42.1 on every kolt edit in the language
  server, again with nothing red (M103).
- **Root cause:** The suite checked results, not time; the timing tests
  were ignored, and wall time on a shared machine was too noisy to gate on.
- **Fix:** M103 found the second regression and made three scans exact and
  cheap (kolt's check 24.4 G to 17.0 G instructions, 44d63c90). M105 built
  the gates: instruction-count
  ceilings in `perf/budgets.toml`, a CI perf job, and a cut that refuses
  without a green seal (8088b438). The paper measured why: instruction
  counts on one input varied by 1.2 parts per million where wall time
  varied 3×.
- **Principles:** C3, C4.

### The ceiling moved with rustc

- **Symptom:** The CI perf job's ceilings could turn red on a tree nobody
  edited (M126).
- **Root cause:** The job built with whatever stable rustc GitHub
  resolved, and the `ci` ceilings hold to 0.5%.
- **Fix:** The job resolves through `rust-toolchain.toml`, and every `ci`
  ceiling was re-adopted from the first pinned run (b4db6445).
- **Principles:** C8.

### A cost read off a loaded machine

- **Symptom:** A pass was filed as "~100 ms on kolt's client" (M38).
- **Root cause:** The figure was a wall reading at load average 31–166. The
  pass's memo already made its walk cost once per type, not per binding.
- **Fix:** The proposed pre-filter measured at +2,312,262 instructions on
  a full kolt check, and was withdrawn (92a7b697).
- **Principles:** C4.

### The prime suspect was refuted

- **Symptom:** The language server slowed down over a session; the
  owner's live server read 4.13 GB after 8 hours (E106).
- **Root cause:** Not the suspect. The deliberate per-analysis leak was
  reclaimed every window; what grew was one retained analysis per open
  document (about 48 MB per kolt file), and glibc not trimming what a
  closed file gave back.
- **Fix:** Measured over 2,000 keystrokes, with a gate that fails on growth
  across windows (5d48c85b); the retention fixes filed as their own items.
- **Principles:** C4.

### The copy that was right, and the narrowing that was not

- **Symptom:** Natively, kolt's dictionary copied a 123,702-entry map per
  word checked: 500 words in 13.2 s, against 4 ms on JS (M199).
- **Root cause:** A `match` or `if .. is` over `cell.read()` copied the
  payload even when its arms only looked at it.
- **Fix:** A pattern whose captures are only viewed reads the cell in place
  on both backends (f57ef700, merged 20b9fc49). On the way the lane found a
  JS miscompile: a bare `cell.read()` subject's captures aliased the cell.
  The memory model's own rule then made a capture handed out of the frame
  copy on JS, and the figure "0.18 s to 25 s" put the branch on hold.
- **What was checked:** The owner's narrowing ("copy only where a cell is
  mutated in place") was tried and found unsound: callers treat a call's
  result as owned and mutate it into the cell, and the branch's own pin
  `m199_a_capture_at_a_value_seam_copies` is that program. And the 25 s
  figure held only for a Vilan-built `HashMap`; kolt's real dictionary, an
  external struct, read 0.05 s both ways. The fix merged as it was, at
  about ×1.0025 instructions, with five example rows bumped.
- **Principles:** C4 (the exhibit decides), C5 (the narrowing had a pin
  against it).
- **Also learned:** dropping the return copy soundly needs caller-side
  alias tracking (M201, open). A related JS miscompile is open: a function
  returning a bare read of a module-level cell hands out the cell itself
  (B735).

### The lens refused twice by measurement

- **Symptom:** None; a design was tested before adoption. `ListCell`
  rebuilt on the reactive lens inside std (T4, `reactive-lens.md` §21).
- **What was measured:** The first try kept every semantic (the whole
  suite green, one golden moved) and was 2× on bare writes, 3.4× in heap
  per cell, and 13–18% more compile cost on every reactive program, ten
  perf rows over their ceilings. Rebuilt on the slimmed lens, it was ×1.48
  on writes, ×3.28 in heap, and worse on compile cost (×1.21–×1.29),
  because the module was dragged into every reactive program.
- **Decision:** Not passed, twice. The lens ships beside the cells, which
  are unchanged; the paper lists what landing it would take. The lab's
  earlier numbers had looked good because they measured the lens where it
  is strong (slots and parts, against `Store`), not on `ListCell`'s bare
  write path.
- **Principles:** C4. A design that holds can still be refused by its
  cost, and the gate is what refused it.

### An unfilled parameter answered once

- **Symptom:** An `Option<List<i53>>` was stored in a
  `SignalCell<List<i53>>`, and `channels.get().len()` answered 2, reading
  the `Option`'s representation as a list. A minimal repro stored a string
  in a `SignalCell<bool>` (B353, marked UNSOUND).
- **Root cause:** An unannotated closure parameter's type is unknown until
  its enclosing call fills it. The deferred method-argument check ran once
  against the unknown type, answered "resolved", and was never asked
  again. The free-function path never had the hole.
- **Fix:** The check defers on an unfilled closure parameter, and the retry
  sees the real type (dbea74f3); three pins, the repro, a kolt-shaped case
  and a control.
- **Principles:** C5, C1 (the method path now asks what the free-function
  path asked).

### A second resolution rule

- **Symptom:** `Cell { held = "abc" }.lend(|text| print(text.len()))`
  printed `TypeError: reading 'length' of undefined`, with no diagnostic
  (B715).
- **Root cause:** A supertrait's default method reached through a sub-trait
  impl ran with the supertrait's `T` unbound: the shared substitution only
  selected an impl block that names the declaring trait.
- **Fix:** The substitution falls back to the block that provides the trait
  through the chain (98b7334f). Two pins, no golden moved.
- **Principles:** C1 and where it stops: the tracker leaves the item open,
  because the fallback ignores the module's import surface and so is a
  second resolution rule beside the admitted one. The recommendation is to
  fold it into the admitted lookup. A fix that works can still be at the
  wrong layer.

### A NULL had two answers

- **Symptom:** A NULL column read through `Row::integer` answered `null`
  behind an `i32` on JS, and `0` natively (F149).
- **Root cause:** The JS binding returned whatever node handed back and let
  the signature supply the type; the native runtime coerced, and described
  its coercion as the JS reader's, which the JS reader never applied.
- **Fix:** A typed reader refuses a NULL with the same panic on both
  backends; the `optional_` readers are the nullable door (d053d9ae,
  breaking, unreleased at the time of writing).
- **Principles:** C6.

### A block ran before its siblings

- **Symptom:** `Pair { a = say("a"), b = { print("b-block"); say("b") } }`
  printed `b-block a b` on JS and `a b-block b` natively (B452).
- **Root cause:** The JS emitter hoisted a block expression at an argument,
  element or field position ahead of the siblings before it.
- **Fix:** The JS emitter keeps sibling order around a block (9c3fdfcc);
  three goldens moved, output identical.
- **Principles:** C6 (evaluation order is a semantic, stated in spec §7.2),
  C10.

### A native server with no deadlines

- **Symptom:** On a native server, a client that sent half a head, or
  declared a body and stalled, held its connection forever (F152).
- **Root cause:** Node bounds a request's head (60 s) and the whole request
  (300 s) by default; the native runtime had no timer on that path. The
  program never named a deadline, so it had one on one backend and none on
  the other.
- **Fix:** The native server keeps node's deadlines and defaults, set on
  both backends through `ServerBuilder` (bd5be545, breaking, unreleased at
  the time of writing).
- **Principles:** C6, C8.

### The probe relied on node's framing

- **Symptom:** The F152 differential failed on Windows CI.
- **Root cause:** The runner resolved a newer node 24.x, which corks a
  response's writes made in one tick into a single chunk; node 24.2.0 and
  the native server send one chunk per write. The probe's expectation
  depended on how node framed writes, which nothing stated.
- **Fix:** The probe sends each piece on its own turn (52081c5b); green on
  both node versions. The native behavior stands.
- **Principles:** C8.

### `::` on Windows

- **Symptom:** A dual-stack bind case failed on Windows CI (F164).
- **Root cause:** A `::` bind is IPv6-only on Windows by default
  (`IPV6_V6ONLY` is on), where it is dual-stack elsewhere. Separately, the
  native server with no host binds IPv4 only, where node binds `::`.
- **Fix:** Open. The planned fix clears `IPV6_V6ONLY` on every OS, which
  needs a socket option std's `TcpListener` doesn't expose. Until then the
  test skips the `::` case on Windows (09032e8f), and the skip goes with
  the fix.
- **Principles:** C8, C6.

### The walk overflowed on Windows

- **Symptom:** A nine-level module-cycle pin aborted the Windows shard with
  `0xc00000fd` at Order 36's seal; Linux passed (N97).
- **Root cause:** The analyzer's expression walk grew its frame as new
  arms landed in one function, while the recursion stayed the same, and
  the Windows test thread ran out first.
- **Fix:** The seal raised test threads to 8 MiB; N97 then measured the
  frame (42,464 bytes per level in debug) and planted a canary that runs a
  thirty-level chain on libtest's own 2 MiB thread, so growth reds on
  Linux (096cba99). `AGENTS.md` point 7 says not to raise the stack
  further.
- **Principles:** C8, C3 (the canary moved the check to every machine).

### A negative delay

- **Symptom:** A debounce pin that asserts an empty stderr went red on a
  slow Windows runner (N155).
- **Root cause:** std's debounce handed `Timer::after` a negative delay
  when the host was slow, and node printed `TimeoutNegativeWarning`.
- **Fix:** `sleep` and `Timer::after` clamp a negative delay at 0
  (d534bf6e).
- **Principles:** C8 (a slow runner is a platform too).

### A keystroke kept a function async

- **Symptom:** After an edit removed `hook = holder.run`, `call_hook` stayed
  `async` in the emitted JS, where a clean analysis made it synchronous
  (B650).
- **Root cause:** The cached seed for the async pass counted a value a
  re-analyzed module assigned into a cached module's binding as the cached
  world's own.
- **Fix:** The cached computation skips a held value outside the cached set
  (6740877f, merged 0279b560). Pinned by the edit-replay differential's
  `s3b_held` fixture, red with the filter reverted. The bug had been latent
  for four orders.
- **Principles:** C7.

### A std world served to the wrong program

- **Symptom:** `import std::web::dmo;` was refused as the moved web prelude
  ("`std::web` moved to `std::web::prelude`") instead of reported as a
  missing module (B651).
- **Root cause:** The std prefix cache's key didn't include the std
  namespaces a program's imports name, so a world built for a program that
  never named `std::web` was served to one that does.
- **Fix:** The namespace is part of the key (376c94b7), pinned by
  `a_std_namespace_the_import_names_is_part_of_the_prefix_key`.
- **Principles:** C7, C9 (the visible symptom was a wrong message).

### A message advised the impossible

- **Symptom:** A non-exported impl in a module with an explicit export
  surface, reached from a library's generic body through a bound, fails
  with advice to widen the library's own import: advice a library's user
  can't follow (B712).
- **Root cause:** The message was written for the case where the importing
  module is the reader's own. Here the fix is `export impl` on the
  caller's impl, which the message never names.
- **Fix:** Open. The recommendation names `export impl` when the impl lives
  in the module that instantiated the generic, pinned by the repro with the
  new wording and its fixed twin; whether a bound-dispatched impl needs the
  export at all is an open ruling.
- **Principles:** C9.

### A warning that became the default

- **Symptom:** A closure parameter that builds views, called inside
  `swap` or `when`, built them under the wrong owner; Vilan 0.53 warned
  (B677), and the fix the warning named was always the same clause,
  `context owner_scope`.
- **Fix:** In 0.54 a parameter whose closure answers a `View` or `Slot`
  carries `context owner_scope` by default (05f25ec2, breaking), and the
  warning remains only for closures answering anything else.
- **Principles:** C9 (a warning every reader answers the same way is a
  default waiting to be made), and the book's principle 2.

### The formatter reformatted the estate

- **Symptom:** A formatter rule change (long calls break their arguments
  one per line everywhere) touched 34 `.vl` files and six goldens, five in
  the corpus and the split fixture's script (E321).
- **Fix:** One change, every file named (std 18, native tests 13, examples
  2, the corpus 1), the goldens regenerated and checked digit-only, eleven
  new pins with eight red on the base (b5d136e5, merged 9716b72f). Programs
  outside the repository got their own patch rather than a surprise.
- **Principles:** C10 (a deliberate estate change, explained).

### The audit leg caught a dependency

- **Symptom:** The CI `audit` leg went red on `next` with no commit, the
  moment RUSTSEC-2026-0459 (bundled SQLite in `libsqlite3-sys`) entered the
  advisory database.
- **Fix:** rusqlite 0.37 to 0.41.1, with the notices regenerated and no
  Vilan code changed (84b304e9). Clippy then failed on the bump:
  `column_names` had become an iterator, so a conversion was useless
  (21b41db0).
- **Principles:** C11 (a bump changes API too; run the gates it owes).

### A keyword in one place of three

- **Symptom:** The `resource` keyword shipped in the lexer only, and the
  two editor grammars missed it; it was caught twice, days apart. An audit
  later found `i64`/`u64` still highlighted as valid types a release after
  they became a hard error (`AGENTS.md`).
- **Fix:** The grammars' word lists are generated from the compiler's own
  tables, and `grammar_sync.rs` holds them byte for byte on every suite run
  (E91).
- **Principles:** the book's 1 and 15: one list, generated, and a gate.

## Known gaps in this page

- The diagnosis tables' order is a guess. It should be reordered by how
  often each cause turns out to be the real one, which means counting the
  tracker's `kind` and root causes over time.
- Most cases are from the last few months of the tracker. Earlier history
  (the parser rewrite, the solver's first designs) is in the archive and
  the CHANGELOG and hasn't been mined.
- C5 has no case of a shortcut that was proved and kept; C7 has no case
  outside the async and prefix caches; C11 has one case.
- Several cases are open (B712, F164) or unreleased (F149, F152, M199) at
  the time of writing. Their fixes may change.
- Some rules in `CLAUDE.md` and `AGENTS.md` have no case here yet (never
  judge a gate by a filtered exit code; a pin that waits on a process is a
  race until the harness sequences it). They are stated there.

## Testing whether it works

Give a fresh agent the same compiler task twice: once with this page and
once without. Grade both versions blind, on the pins written, the root
cause named, and the gates run. If the version with the page isn't
consistently better, the entries are too vague to act on, and whatever
separates a good change from what was written is still unwritten.
