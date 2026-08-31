//! The ignored set, gated from two sides.
//!
//! **The weekly leg** (backlog N27), held to the decisions that make it worth
//! having. Nothing else in the tree runs `.github/workflows/ignored-pins.yml` —
//! it fires on a cron, once a week, and a leg that has quietly stopped
//! excluding the 247-second pin, stopped being non-blocking, or started using
//! nextest's deprecated flag spelling would first be noticed by whoever waits
//! on it. So these pins read the workflow and hold the four decisions in it,
//! plus the one fact outside it the leg depends on: that the test it filters by
//! name is still a test.
//!
//! **The reasons themselves** (backlog N31). Two house rules meet at an
//! `#[ignore]`: a known-but-unfixed bug is pinned `#[ignore]`d, and an open
//! defect is not tracked unless it has an item. Nothing enforced the join, and
//! audit runs 2, 3 and 4 each found an ignored pin whose defect lived nowhere
//! but its reason string — a bug the tracker had never heard of, discoverable
//! only by reading test attributes. So every reason must LEAD with a tracker
//! item id, or be one of the few ignores that are deliberately not bugs.
//!
//! The leading id, and the scanner's fence around this file's own fixture, are
//! N33: run 5 found the first version of both halves too weak to be worth the
//! green tick. "Capitals then a digit" accepted `ARM64` and `UTF8`, and let a
//! reason satisfy the gate by mentioning any of the three ids in its prose
//! while the OPEN owner of the defect went unnamed; and the sweep read this
//! file's own string-literal fixture as a real attribute, passing by luck.

use std::path::PathBuf;
use std::process::Command;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn workflow() -> String {
    std::fs::read_to_string(repository_root().join(".github/workflows/ignored-pins.yml"))
        .expect("the ignored-pins workflow is committed")
}

/// The workflow with every comment line dropped — YAML's and the shell's alike.
/// This file's subject is what the leg DOES, and the workflow explains itself at
/// length: a `contains` over the whole text is satisfied by prose describing the
/// very decision that was deleted, which is a vacuous pin wearing a green tick.
fn workflow_code() -> String {
    workflow()
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The pin the leg excludes by name — 247 s of measurement whose `#[ignore]`
/// reason is cost, not a bug, so running it buys the leg nothing.
const EXCLUDED_PIN: &str = "perf_baseline_lsp_edit_latency";

// It runs the ignored set, and it runs it with the spelling nextest still
// documents. `ignored-only` is a deprecated alias that works today and is
// exactly the kind of thing a future nextest removes; `only` is the value its
// `--help` lists.
#[test]
fn the_leg_runs_the_ignored_set_with_nextests_current_flag_spelling() {
    let workflow = workflow_code();
    assert!(
        workflow.contains("--run-ignored only"),
        "the leg must run the ignored set: {workflow}"
    );
    assert!(
        !workflow.contains("--run-ignored ignored-only"),
        "`ignored-only` is nextest's DEPRECATED alias — the spelling is `only`"
    );
    assert!(
        workflow.contains("cargo nextest run --workspace"),
        "the leg runs the whole workspace's ignored set"
    );
}

// The exclusion, and the fact it depends on: a filter naming a test that no
// longer exists excludes nothing and costs the leg a quarter of its budget in
// silence.
#[test]
fn the_leg_excludes_the_expensive_pin_and_that_pin_still_exists() {
    assert!(
        workflow_code().contains(&format!("not test({EXCLUDED_PIN})")),
        "the leg must filter {EXCLUDED_PIN} out of the ignored set"
    );

    let document =
        std::fs::read_to_string(repository_root().join("crates/vilan-lsp/src/document.rs"))
            .expect("read the lsp document module");
    assert!(
        document.contains(&format!("fn {EXCLUDED_PIN}(")),
        "the leg filters {EXCLUDED_PIN} by name, and no test by that name exists any more — \
         the filter now excludes nothing"
    );
}

// Non-blocking, by construction. The bug pins in the ignored set are expected
// RED; a gate here would be red by design, and the signal it exists for is the
// opposite one (a pin that now passes).
#[test]
fn the_leg_is_advisory_and_reports_a_pin_that_now_passes() {
    let workflow = workflow_code();
    // The KEY, not the word: the header comment says `continue-on-error: true`
    // in prose, so a `contains` over the whole file passes on a workflow that
    // only talks about being advisory.
    assert!(
        workflow
            .lines()
            .any(|line| line.trim() == "continue-on-error: true"),
        "the leg must not block: its bug pins are expected red"
    );
    assert!(
        workflow.contains("GITHUB_STEP_SUMMARY"),
        "the leg's product is a job summary — that is the whole deliverable"
    );
    assert!(
        workflow.contains("now PASSES"),
        "the summary must call out a pin that PASSES: an expired `#[ignore]` reason is \
         the finding this leg exists to make"
    );
    assert!(
        workflow.contains("cron:"),
        "the leg is periodic, not per-push"
    );
}

// …and it is out of the required check's way. `ci / check` is the required
// status context (process.md §2.5): a required check that can be skipped, or
// that waits on an advisory job, strands a PR at "Expected" forever.
#[test]
fn the_required_check_does_not_wait_on_the_advisory_leg() {
    let ci = std::fs::read_to_string(repository_root().join(".github/workflows/ci.yml"))
        .expect("the ci workflow is committed");
    let needs = ci
        .lines()
        .find(|line| line.trim_start().starts_with("needs: [changes"))
        .expect("`check` declares its needed jobs on one line");
    assert_eq!(
        needs.trim(),
        "needs: [changes, test, wasm, fmt]",
        "`check` gained or lost a needed job — if the advisory ignored-pins leg is in \
         there, a weekly report can now block a PR"
    );
    assert!(
        !ci.contains("ignored-pins"),
        "the advisory leg lives in its own workflow so `ci / check` cannot wait on it"
    );
}

// ── N31: every `#[ignore]` reason names a tracker item ────────────────────────

/// The ignores that are deliberately NOT bugs, and so have no item to name.
/// Listed by their exact reason string rather than matched by a pattern: a
/// pattern is how "run deliberately" quietly becomes an escape hatch anyone can
/// spell their way into, and the whole point of this gate is that adding a
/// member is a decision somebody makes on purpose, in a diff.
///
/// The first three are cost and tooling, not defects — nothing is broken, and
/// running them in the ordinary suite would buy a wrong answer (a timing
/// measurement under suite load) or none at all (a build tool that is not
/// installed).
const DELIBERATE_NON_BUG_IGNORES: &[&str] = &[
    // The perf baseline, in both places it is measured (the CLI's corpus timing
    // and the LSP's edit latency): minutes of measurement whose number means
    // nothing on a machine running forty other test binaries.
    "the performance baseline: minutes of measurement, run deliberately \
     (proposal/perf-baseline.md §3)",
    // The leak soak: thousands of analyses per corpus, same reason.
    "the leak soak: thousands of analyses per corpus, run deliberately \
     (proposal/leak-soak.md §5)",
    // A tool gate: the book-heading differential needs a pinned `mdbook` on
    // PATH, which CI has and a contributor's machine may not.
    "needs the pinned `mdbook` v0.5.4 on PATH: builds the book and compares \
     every heading id to mdbook_heading_ids",
];

/// The tracker's family letters — the sections of its `INDEX.md`, one letter
/// each. A deliberate coupling to the other repo, and the smallest one
/// available: the roster of FAMILIES changes when a section is added, which is
/// rare and deliberate, where the roster of ITEMS changes several times a
/// cycle and could not be tracked from here at all.
///
/// The cost is honest and its direction is the safe one. A tracker that gains a
/// section reddens this gate on the first pin that names an item in it, and the
/// fix is one character in this list — a decision visible in a diff, exactly
/// like adding a member to [`DELIBERATE_NON_BUG_IGNORES`]. What it buys is the
/// refusal the shape alone could not make: `ARM64`, `UTF8`, `ES6` and `ISO8601`
/// are all "capitals then a digit", and none of them points anywhere.
const TRACKER_FAMILIES: &[char] = &['A', 'B', 'C', 'D', 'E', 'G', 'I', 'J', 'K', 'L', 'M', 'N'];

/// Whether `reason` LEADS with a tracker item id — a single family letter, one
/// to three digits, and then the end of the id (`B154: …`, `C13 — …`).
///
/// Both halves of that shape close a hole the audit found (N33), and neither
/// closes the other's:
///
/// - **The letter set.** "One or more capitals then a digit" is the shape of an
///   ACRONYM as much as of an id, so `ARM64`, `UTF8`, `ES6` and `ISO8601` all
///   satisfied it. An id is one family letter, and the run must stop there:
///   `ES6` fails because `S` is not a digit, `ISO8601` for the same reason, and
///   the id must end at a non-alphanumeric so `I18n` is not read as item 18.
/// - **The lead.** A reason that merely MENTIONS an id somewhere in its prose
///   has as many candidate owners as it has capitals. `borrows.rs` passed this
///   gate by naming `P4c` (a proposal slice label) and `C12` (closed) while the
///   OPEN owner of the defect, `C13`, went unnamed — three plausible ids, none
///   of them the answer. Leading with the id makes the reason POINT: there is
///   exactly one candidate, and it is the first thing a reader sees.
fn names_a_tracker_item(reason: &str) -> bool {
    let mut characters = reason.trim_start().chars();
    let Some(family) = characters.next() else {
        return false;
    };
    if !TRACKER_FAMILIES.contains(&family) {
        return false;
    }
    let mut digits = 0;
    for character in characters {
        if character.is_ascii_digit() {
            digits += 1;
            // Three digits is already 999 items in one family; a longer run is
            // a year or a version, not an id.
            if digits > 3 {
                return false;
            }
            continue;
        }
        // The id ends where the number does, and it must END: a letter or an
        // underscore straight after the digits makes this a word, not an id.
        return digits > 0 && !character.is_alphanumeric() && character != '_';
    }
    digits > 0
}

/// Every tracked `.rs` file, as `(repo-relative name, contents)`. `git
/// ls-files` is the enumerator on purpose: it is exactly the committed tree, so
/// the sweep can never wander into `target/` or into a sibling worktree under
/// `.claude/` (both ignored) and mistake somebody else's branch for this one.
fn tracked_rust_sources() -> Vec<(String, String)> {
    let repository_root = repository_root();
    let listing = Command::new("git")
        .args(["ls-files", "-z", "*.rs"])
        .current_dir(&repository_root)
        .output()
        .expect("git ls-files");
    assert!(listing.status.success(), "git ls-files failed");
    String::from_utf8_lossy(&listing.stdout)
        .split('\0')
        .filter(|name| !name.is_empty())
        .filter_map(|name| {
            let text = std::fs::read_to_string(repository_root.join(name)).ok()?;
            Some((name.to_string(), text))
        })
        .collect()
}

/// The fence a file writes around a block of Rust that is a FIXTURE rather than
/// code — a `#[ignore]` written inside a string literal, for a test about the
/// sweep itself. Spelled with `concat!` so the joined marker never appears in
/// this file except at the two places that really fence something: a scanner
/// looking for its own marker text would otherwise find these very lines.
const FIXTURE_FENCE_OPEN: &str = concat!("ignore-sweep-fixture", ":start");
const FIXTURE_FENCE_CLOSE: &str = concat!("ignore-sweep-fixture", ":end");

/// `text` with every fenced fixture region blanked to spaces, newlines kept so
/// every line number after one is unchanged.
///
/// Why a fence and not a Rust-literate scanner (N33): the sweep is line
/// oriented, so an `#[ignore` written at column 0 INSIDE a string literal reads
/// as an attribute — which is exactly what this file's own fixture is, and it
/// passed the gate only by coincidence, `read_string_literal` swallowing the
/// fixture and `read_attribute` picking up a later quoted string as the reason.
/// The general fix would be to track string state while scanning, and that is
/// the wrong trade here: this tree has 63 files using raw strings (`r#"…"#`)
/// and dozens containing a `'"'` char literal (`main.rs` alone has seven), so a
/// quote-toggling scanner desyncs, and the direction it fails in is a SILENT
/// gate — blind to every attribute after the desync. A fence fails the other
/// way: what is skipped is exactly what somebody wrote a marker around, and the
/// markers are inventoried by [`the_fixture_fence_is_used_once_and_only_here`],
/// so it cannot quietly become an escape hatch.
fn without_fixture_fences(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        if !fenced && line.contains(FIXTURE_FENCE_OPEN) {
            fenced = true;
        } else if fenced && line.contains(FIXTURE_FENCE_CLOSE) {
            fenced = false;
        }
        if fenced || line.contains(FIXTURE_FENCE_CLOSE) {
            result.extend(
                line.chars()
                    .map(|character| if character == '\n' { '\n' } else { ' ' }),
            );
        } else {
            result.push_str(line);
        }
    }
    result
}

/// Every `#[ignore]` attribute in `text`, as `(1-based line, reason)` — `None`
/// when the attribute carries no reason at all.
///
/// An attribute is recognized only where one is written: at the start of a
/// line, whitespace aside. That is what keeps the sweep off the many `#[ignore]`
/// mentions in this tree's prose — doc comments explaining the house rule,
/// including the ones in this very file — without needing to parse Rust. The
/// other place an `#[ignore` is not an attribute is inside a string literal,
/// and [`without_fixture_fences`] is how a file says so.
fn ignore_attributes(text: &str) -> Vec<(usize, Option<String>)> {
    const ATTRIBUTE: &str = "#[ignore";
    let text = &without_fixture_fences(text);
    let bytes = text.as_bytes();
    let mut attributes = Vec::new();
    let mut line = 1;
    let mut still_leading_whitespace = true;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' {
            line += 1;
            still_leading_whitespace = true;
            index += 1;
            continue;
        }
        if !bytes[index].is_ascii_whitespace() {
            if still_leading_whitespace && text[index..].starts_with(ATTRIBUTE) {
                let (reason, after) = read_attribute(text, index + ATTRIBUTE.len());
                attributes.push((line, reason));
                line += text[index..after].matches('\n').count();
                index = after;
                // Resume AT the character after the bracket, never past it: a
                // newline swallowed here loses a line for everything below.
                still_leading_whitespace = false;
                continue;
            }
            still_leading_whitespace = false;
        }
        index += 1;
    }
    attributes
}

/// Reads one attribute from just past `#[ignore` to its closing `]`, returning
/// the string literal inside it (decoded) and the offset after the bracket. The
/// scan understands string literals because a reason may span lines with `\`
/// continuations and may contain a bracket.
fn read_attribute(text: &str, from: usize) -> (Option<String>, usize) {
    let bytes = text.as_bytes();
    let mut reason = None;
    let mut index = from;
    while index < bytes.len() {
        match bytes[index] {
            b']' => return (reason, index + 1),
            b'"' => {
                let (literal, after) = read_string_literal(text, index);
                reason = Some(literal);
                index = after;
            }
            _ => index += 1,
        }
    }
    (reason, bytes.len())
}

/// The contents of the Rust string literal starting at the quote at `from`,
/// with its escapes resolved, and the offset just past its closing quote. A
/// backslash before a newline swallows the break and the next line's
/// indentation, exactly as rustc does — which is what lets a long reason be
/// written across lines and still compare equal to a one-line allowlist entry.
fn read_string_literal(text: &str, from: usize) -> (String, usize) {
    let bytes = text.as_bytes();
    let mut literal = String::new();
    let mut index = from + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => return (literal, index + 1),
            b'\\' => {
                index += 1;
                match bytes.get(index) {
                    Some(b'n') => literal.push('\n'),
                    Some(b't') => literal.push('\t'),
                    Some(b'\n') => {
                        index += 1;
                        while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
                            index += 1;
                        }
                        continue;
                    }
                    Some(_) => {
                        let rest = &text[index..];
                        let character = rest.chars().next().expect("a character follows `\\`");
                        literal.push(character);
                        index += character.len_utf8();
                        continue;
                    }
                    None => return (literal, bytes.len()),
                }
                index += 1;
            }
            _ => {
                let rest = &text[index..];
                let character = rest.chars().next().expect("a character at a boundary");
                literal.push(character);
                index += character.len_utf8();
            }
        }
    }
    (literal, bytes.len())
}

// The gate itself. A pin that says only what is wrong, and not WHERE that is
// written down, is how a defect ends up living in a test attribute — which is
// the shape three consecutive audit runs found and the reason this exists.
#[test]
fn every_ignored_pin_names_a_tracker_item_or_is_a_declared_non_bug() {
    let mut offenders = Vec::new();
    for (name, text) in tracked_rust_sources() {
        for (line, reason) in ignore_attributes(&text) {
            let Some(reason) = reason else {
                offenders.push(format!("{name}:{line}: `#[ignore]` with no reason at all"));
                continue;
            };
            if names_a_tracker_item(&reason) {
                continue;
            }
            if DELIBERATE_NON_BUG_IGNORES.contains(&reason.as_str()) {
                continue;
            }
            offenders.push(format!("{name}:{line}: {reason:?}"));
        }
    }
    assert!(
        offenders.is_empty(),
        "an `#[ignore]` reason must name its tracker item (`B154`, `E102`, …) so the \
         defect lives somewhere other than this attribute — or, for an ignore that is \
         deliberately not a bug, be added to `DELIBERATE_NON_BUG_IGNORES` here, on \
         purpose. Offending:\n  {}",
        offenders.join("\n  ")
    );
}

// The sweep is only worth what its predicate is worth, and a predicate that
// answered `true` to everything would make the gate above green forever.
#[test]
fn the_item_id_shape_accepts_an_id_and_refuses_prose() {
    for reason in [
        "B154 — the internal `NativeMap::insert` frees the caller's value",
        "E102 residue",
        "C11's predicate, narrowed",
        "N33: the id ends at the digits, and a colon is a fine terminator",
        "  B1: leading whitespace is not prose",
    ] {
        assert!(
            names_a_tracker_item(reason),
            "should name an item: {reason}"
        );
    }
    for reason in [
        "needs the pinned mdbook on PATH",
        "run deliberately: minutes of measurement",
        "OPEN, not yet filed",
        "b154 in lower case is not an id",
        "",
        // N33's first weakness: capitals-then-a-digit is the shape of an
        // ACRONYM too, and none of these points at anything.
        "ARM64 has no ignored pins",
        "UTF8 decoding is host business",
        "ES6 modules, not an item",
        "ISO8601 timestamps drift under load",
        "I18n is a word, not item 18 of the collections family",
        "A2026 is a year wearing a family letter",
        // N33's second weakness: a reason that MENTIONS an id has as many
        // candidate owners as it has capitals. `borrows.rs` named `P4c` and
        // `C12` while the open owner `C13` went unwritten.
        "waiting on N31",
        "not C12's hole — the capture here is a view parameter",
        "P4c: a proposal slice label is not a tracker item",
        "F14: an audit finding is not a tracker item either",
    ] {
        assert!(
            !names_a_tracker_item(reason),
            "should NOT count as naming an item: {reason}"
        );
    }
}

// The fence, and the inventory that keeps it from becoming an escape hatch.
// Skipping a region is the same kind of decision as allowlisting a reason, so
// it is held the same way: an exact list, edited on purpose, in a diff.
#[test]
fn the_fixture_fence_is_used_once_and_only_here() {
    let fenced: Vec<String> = tracked_rust_sources()
        .into_iter()
        .filter(|(_, text)| text.contains(FIXTURE_FENCE_OPEN))
        .map(|(name, _)| name)
        .collect();
    assert_eq!(
        fenced,
        vec!["crates/vilan-cli/tests/ci_ignored_pins.rs".to_string()],
        "the fixture fence exists for this file's own scanner fixture. A second \
         user is a decision, not a detail: say why here"
    );
    let text = tracked_rust_sources()
        .into_iter()
        .find(|(name, _)| name == "crates/vilan-cli/tests/ci_ignored_pins.rs")
        .expect("this file is tracked")
        .1;
    assert_eq!(
        text.matches(FIXTURE_FENCE_OPEN).count(),
        1,
        "one fenced region, not a fence anyone can reopen"
    );
    assert_eq!(text.matches(FIXTURE_FENCE_CLOSE).count(), 1);
    // And the point of it, stated against the real sweep rather than a
    // constructed source: this file holds no ignored PIN, so the whole-repo
    // scan must find nothing here. Without the fence it finds the fixture's,
    // and what it reads as the reason is whatever the string scan happens to
    // run into next — the coincidence audit run 5 caught.
    assert!(
        ignore_attributes(&text).is_empty(),
        "the sweep must see no attribute in the file that owns the scanner: {:?}",
        ignore_attributes(&text)
    );
}

// And the fence does what it says: an `#[ignore` written at column 0 inside a
// fenced block is invisible to the sweep, while one outside it is not, and the
// line numbers on the far side of a fence are unmoved.
#[test]
fn the_sweep_skips_a_fenced_fixture_and_keeps_its_line_numbers() {
    // Assembled rather than written out, markers included: an attribute or a
    // marker spelled at the head of a line in THIS file would be read by the
    // whole-repo sweep as the real thing, which is the exact confusion the
    // fence exists to end. The one place this file spells either is the fenced
    // region below.
    let attribute = |reason: &str| format!("#[ignore = \"{reason}\"]");
    let source = format!(
        "// {FIXTURE_FENCE_OPEN}\n\
         let fixture = \"…\n\
         {}\n\
         fn inside() {{}}\n\
         \";\n\
         // {FIXTURE_FENCE_CLOSE}\n\
         #[test]\n\
         {}\n\
         fn outside() {{}}\n",
        attribute("B1: a fixture, not a pin"),
        attribute("C13: a real one, after the fence"),
    );
    assert_eq!(
        ignore_attributes(&source),
        vec![(8, Some("C13: a real one, after the fence".to_string()))],
        "the fenced `#[ignore` is a fixture; the one after it is a pin, on line 8"
    );
}

// And the scanner: it must find the attribute where one is written, across the
// line break a long reason takes, and must NOT find the ones this tree's prose
// talks about — a false positive there would redden the gate over a comment.
#[test]
fn the_sweep_reads_attributes_and_not_prose_about_them() {
    // ignore-sweep-fixture:start
    let source = "\
/// A known-but-unfixed bug is pinned `#[ignore]`d and un-ignored when fixed.
#[test]
#[ignore = \"B1: a reason \\
            written across lines\"]
fn pinned() {}

#[test]
#[ignore]
fn bare() {}
";
    // ignore-sweep-fixture:end
    let found = ignore_attributes(source);
    assert_eq!(
        found,
        vec![
            (3, Some("B1: a reason written across lines".to_string())),
            (8, None),
        ],
        "the doc comment's `#[ignore]` is prose, not an attribute"
    );
}
