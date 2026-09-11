//! Every listing of a std enum in the book names every arm std declares
//! (tracker E159).
//!
//! `--test docs` compiles every fenced example, which is what makes a doc
//! example that no longer typechecks impossible to ship. A stale enum LISTING
//! is invisible to it, because a listing that is missing an arm still compiles
//! perfectly: A52 added `RpcError::Unavailable` and `Reject::Unavailable`, and
//! the book went on counting five variants, listing five in the reference and
//! naming two status codes in the mapping — three separate readers, each of
//! them wrong, and every gate green.
//!
//! This is the `style_table_sync` shape (Order 12): the source of truth is read
//! out of the real artefact (`vilan/std/src/**/*.vl`), the hand-written copy is
//! held to it, and the SAME reader parses both — so "what the book shows" and
//! "what std declares" cannot be parsed apart.
//!
//! # The three listing forms, because the defect wore all three
//!
//! 1. **A fence** that declares the enum (`std/rpc.md`'s `enum RpcError { … }`).
//!    Its arms must be exactly std's, in order — a reference listing is a copy
//!    of the declaration and there is no reason for it to differ.
//! 2. **A table** whose first header cell is the enum's name
//!    (`guide/services.md`'s `| `Reject` | Status | … |`). One row per arm, in
//!    order: `Reject` has no fence in the book at all, so the fence gate alone
//!    would not have caught its half of A52.
//! 3. **A prose listing** — a paragraph that names the enum and COUNTS its arms
//!    ("`RpcError` tells you what went wrong, in six variants: …"). The count
//!    word is the claim, and it is exactly the sentence that went stale, so the
//!    count is checked against the declaration and every arm must be named in
//!    the paragraph. Order is not checked here: prose reads in whatever order
//!    it explains best.
//!
//! A doc example that happens to DECLARE its own enum sharing a std name would
//! be compared against std's arms and would be wrong to be. There is no such
//! example today; [`NOT_THE_STD_ENUM`] is where one would be excused, one entry
//! with its reason, which is a decision visible in a diff.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Listings the gate must NOT read as a std enum's, each with its reason — a
/// doc example that declares its own type sharing a std name. Empty: every
/// listing in the book today is genuinely a mirror of std's declaration.
const NOT_THE_STD_ENUM: &[(&str, &str)] = &[];

/// The number words a prose listing counts with, `one` at index 1.
const NUMBER_WORDS: &[&str] = &[
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
    "twenty",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repo root is two levels above the crate")
}

fn markdown_files() -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(&repo_root().join("vilan/docs"), "md", &mut found);
    found.sort();
    found
}

fn vilan_sources() -> Vec<PathBuf> {
    let mut found = Vec::new();
    collect(&repo_root().join("vilan/std/src"), "vl", &mut found);
    found.sort();
    found
}

fn collect(directory: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, extension, found);
        } else if path.extension().is_some_and(|found| found == extension) {
            found.push(path);
        }
    }
}

/// A path as the failure messages spell it — repo-relative, so a message can be
/// pasted into an editor.
fn relative(path: &Path) -> String {
    path.strip_prefix(repo_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

// --- The reader, used on BOTH sides ------------------------------------------

/// Every `enum Name { … }` in `text`, as `(name, arms)` in declaration order.
///
/// One reader for std's declarations and for the book's fences, which is the
/// whole point: a difference this gate reports is a difference in the SOURCE
/// and never a difference in how the two were read.
fn enum_declarations(text: &str) -> Vec<(String, Vec<String>)> {
    let mut found = Vec::new();
    let bytes = text.as_bytes();
    let mut at = 0;
    while let Some(offset) = text[at..].find("enum ") {
        let start = at + offset;
        at = start + "enum ".len();
        // A declaration, not the word inside a sentence or a type position:
        // `enum` begins its line (modifiers and attributes sit on their own
        // lines in both the std sources and the book's fences).
        let line_start = text[..start].rfind('\n').map(|at| at + 1).unwrap_or(0);
        if !text[line_start..start].trim().is_empty() {
            continue;
        }
        let name: String = text[at..]
            .chars()
            .take_while(|character| character.is_alphanumeric() || *character == '_')
            .collect();
        if name.is_empty() {
            continue;
        }
        let Some(open) = text[at..].find('{').map(|offset| at + offset) else {
            continue;
        };
        // The generic parameter list may not contain a `{`, so the first one
        // after the name opens the body.
        let mut depth = 0usize;
        let mut close = None;
        for (index, byte) in bytes.iter().enumerate().skip(open) {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(index);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(close) = close else { continue };
        found.push((name, arm_names(&text[open + 1..close])));
        at = close;
    }
    found
}

/// The arm names of an enum body: the comma-separated entries at the body's own
/// depth, each read down to its leading capitalized identifier.
///
/// Comments come off FIRST and that is not a detail — a `///` arm doc is prose,
/// prose has commas in it, and splitting before stripping cut `Reject` into one
/// arm and eleven fragments.
fn arm_names(body: &str) -> Vec<String> {
    let mut stripped = String::with_capacity(body.len());
    for line in body.lines() {
        stripped.push_str(line.split("//").next().unwrap_or(line));
        stripped.push('\n');
    }
    let mut arms = Vec::new();
    let mut depth = 0usize;
    let mut entry = String::new();
    for character in stripped.chars() {
        match character {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                push_arm(&entry, &mut arms);
                entry.clear();
                continue;
            }
            _ => {}
        }
        entry.push(character);
    }
    push_arm(&entry, &mut arms);
    arms
}

fn push_arm(entry: &str, arms: &mut Vec<String>) {
    let trimmed = entry.trim_start();
    let name: String = trimmed
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    if name.chars().next().is_some_and(char::is_uppercase) {
        arms.push(name);
    }
}

/// Every std enum by name, with the file it is declared in.
fn std_enums() -> BTreeMap<String, (String, Vec<String>)> {
    let mut declared = BTreeMap::new();
    for path in vilan_sources() {
        let text = std::fs::read_to_string(&path).expect("read a std source");
        for (name, arms) in enum_declarations(&text) {
            declared.insert(name, (relative(&path), arms));
        }
    }
    assert!(
        declared.len() > 20,
        "the std enum reader found only {} enums — it has stopped reading the \
         sources it is supposed to be derived from",
        declared.len()
    );
    declared
}

/// `text` with every fenced block replaced by blank lines of the same shape, so
/// the prose scans below cannot read a code fence as a paragraph and the line
/// numbers still line up.
fn without_fences(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut inside = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            inside = !inside;
            out.push('\n');
            continue;
        }
        if !inside {
            out.push_str(line);
        }
        out.push('\n');
    }
    out
}

fn excused(page: &str, name: &str) -> bool {
    NOT_THE_STD_ENUM
        .iter()
        .any(|(excused_page, excused_name)| *excused_page == page && *excused_name == name)
}

// --- Form 1: a fence that declares the enum ----------------------------------

#[test]
fn every_doc_fence_of_a_std_enum_lists_every_arm() {
    let declared = std_enums();
    let mut stale = Vec::new();
    let mut seen = 0;
    for path in markdown_files() {
        let text = std::fs::read_to_string(&path).expect("read a doc page");
        let page = relative(&path);
        let mut inside = false;
        let mut fence = String::new();
        for line in text.lines() {
            if line.trim_start().starts_with("```") {
                if inside {
                    for (name, arms) in enum_declarations(&fence) {
                        let Some((source, std_arms)) = declared.get(&name) else {
                            continue;
                        };
                        if excused(&page, &name) {
                            continue;
                        }
                        seen += 1;
                        if &arms != std_arms {
                            stale.push(format!(
                                "  {page}: `enum {name}` shows {arms:?}, {source} declares \
                                 {std_arms:?}"
                            ));
                        }
                    }
                    fence.clear();
                }
                inside = !inside;
                continue;
            }
            if inside {
                fence.push_str(line);
                fence.push('\n');
            }
        }
    }
    assert!(
        seen > 0,
        "the fence scan found no std enum listed anywhere in the book — it has \
         stopped looking, and this gate would pass over any drift"
    );
    assert!(
        stale.is_empty(),
        "these doc fences no longer show what std declares. A reference listing \
         is a COPY of the declaration, so the fix is the declaration's own arms, \
         in its own order:\n{}",
        stale.join("\n")
    );
}

// --- Form 2: a table headed by the enum's name -------------------------------

#[test]
fn every_doc_table_of_a_std_enum_lists_every_arm() {
    let declared = std_enums();
    let mut stale = Vec::new();
    let mut seen = 0;
    for path in markdown_files() {
        let text = without_fences(&std::fs::read_to_string(&path).expect("read a doc page"));
        let page = relative(&path);
        let lines: Vec<&str> = text.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            let Some(name) = table_head_enum(line) else {
                continue;
            };
            let Some((source, std_arms)) = declared.get(&name) else {
                continue;
            };
            if excused(&page, &name) {
                continue;
            }
            // A table is its header, a `| --- |` rule, then its rows.
            if !lines.get(index + 1).is_some_and(|rule| is_table_rule(rule)) {
                continue;
            }
            seen += 1;
            let shown: Vec<String> = lines[index + 2..]
                .iter()
                .take_while(|row| row.starts_with('|'))
                .filter_map(|row| first_cell_name(row))
                .collect();
            if &shown != std_arms {
                stale.push(format!(
                    "  {page}:{}: the `{name}` table shows {shown:?}, {source} declares \
                     {std_arms:?}",
                    index + 1
                ));
            }
        }
    }
    assert!(
        seen > 0,
        "the table scan found no std enum's table in the book — `guide/services.md`'s \
         `Reject` table is the one this form exists for, and it is no longer being read"
    );
    assert!(
        stale.is_empty(),
        "these doc tables no longer have one row per arm. A table headed by an \
         enum's name is a listing of that enum:\n{}",
        stale.join("\n")
    );
}

/// The enum a table header declares itself to be about: `| `Reject` | … |`.
fn table_head_enum(line: &str) -> Option<String> {
    let cell = line.strip_prefix('|')?.split('|').next()?.trim();
    let name = cell.strip_prefix('`')?.strip_suffix('`')?;
    (!name.is_empty()
        && name.starts_with(|character: char| character.is_ascii_uppercase())
        && name
            .chars()
            .all(|character| character.is_alphanumeric() || character == '_'))
    .then(|| name.to_string())
}

fn is_table_rule(line: &str) -> bool {
    line.starts_with('|')
        && line
            .chars()
            .all(|character| matches!(character, '|' | '-' | ':' | ' '))
        && line.contains('-')
}

/// The capitalized name a table row's first cell leads with, backticks and any
/// payload spelling ignored.
fn first_cell_name(row: &str) -> Option<String> {
    let cell = row.strip_prefix('|')?.split('|').next()?.trim();
    let cell = cell.trim_start_matches('`');
    let name: String = cell
        .chars()
        .take_while(|character| character.is_alphanumeric() || *character == '_')
        .collect();
    name.chars()
        .next()
        .is_some_and(char::is_uppercase)
        .then_some(name)
}

// --- Form 3: a prose paragraph that COUNTS the arms --------------------------

#[test]
fn every_prose_count_of_a_std_enums_arms_is_the_declared_count() {
    let declared = std_enums();
    let mut stale = Vec::new();
    let mut seen = 0;
    for path in markdown_files() {
        let text = without_fences(&std::fs::read_to_string(&path).expect("read a doc page"));
        let page = relative(&path);
        for (line_number, block) in prose_blocks(&text) {
            let Some(counted) = counted_arms(&block) else {
                continue;
            };
            // The claim is about an enum only where the block NAMES one. "two
            // arms of its `T: A + B` bound" counts something else entirely, and
            // the book says that in four places.
            let Some(name) = declared
                .keys()
                .find(|name| block.contains(&format!("`{name}`")))
            else {
                continue;
            };
            let (source, arms) = &declared[name];
            if excused(&page, name) {
                continue;
            }
            seen += 1;
            if counted != arms.len() {
                stale.push(format!(
                    "  {page}:{line_number}: counts {counted} arm(s) of `{name}`, {source} \
                     declares {} ({arms:?})",
                    arms.len()
                ));
                continue;
            }
            let missing: Vec<&String> =
                arms.iter().filter(|arm| !names_word(&block, arm)).collect();
            if !missing.is_empty() {
                stale.push(format!(
                    "  {page}:{line_number}: counts `{name}`'s arms but never names \
                     {missing:?} ({source})"
                ));
            }
        }
    }
    assert!(
        seen > 0,
        "the prose scan found no counted listing of a std enum — `guide/services.md`'s \
         \"in six variants\" is the sentence this form exists for, and it is no longer \
         being read"
    );
    assert!(
        stale.is_empty(),
        "these sentences count a std enum's arms and the count (or the list) has \
         gone stale — the exact shape A52 shipped:\n{}",
        stale.join("\n")
    );
}

/// The book's prose in blocks: a run of non-blank lines, and each list item its
/// own block. A bullet list is written with no blank line between its items, so
/// without the second rule one item's count word would pair with another item's
/// enum name.
fn prose_blocks(text: &str) -> Vec<(usize, String)> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut start = 0;
    let mut flush = |start: usize, current: &mut String| {
        if !current.trim().is_empty() {
            blocks.push((start, std::mem::take(current)));
        } else {
            current.clear();
        }
    };
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let bullet = trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
            || trimmed.split_once(". ").is_some_and(|(head, _)| {
                !head.is_empty() && head.bytes().all(|b| b.is_ascii_digit())
            });
        if line.trim().is_empty() || bullet {
            flush(start, &mut current);
            start = index + 1;
        }
        if line.trim().is_empty() {
            continue;
        }
        if current.is_empty() {
            start = index + 1;
        }
        current.push_str(line);
        current.push('\n');
    }
    flush(start, &mut current);
    blocks
}

/// The arm count a block CLAIMS: `<number word> arms` / `<number word> variants`.
fn counted_arms(block: &str) -> Option<usize> {
    let lowered = block.to_lowercase();
    for (count, word) in NUMBER_WORDS.iter().enumerate() {
        for noun in ["arms", "variants"] {
            if lowered.contains(&format!("{word} {noun}")) {
                return Some(count);
            }
        }
    }
    None
}

/// Whether `block` names `word` as a whole word — `Text` is not `Texture`.
fn names_word(block: &str, word: &str) -> bool {
    let boundary = |character: Option<char>| {
        character.is_none_or(|character| !character.is_alphanumeric() && character != '_')
    };
    let mut at = 0;
    while let Some(offset) = block[at..].find(word) {
        let found = at + offset;
        let before = block[..found].chars().next_back();
        let after = block[found + word.len()..].chars().next();
        if boundary(before) && boundary(after) {
            return true;
        }
        at = found + word.len();
    }
    false
}

// --- The two exhibits, named ------------------------------------------------

/// E159's own two, asserted by NAME rather than left to the scans' counters: a
/// gate that stops finding its exhibit passes silently, and these are the two
/// listings A52's arm actually went missing from.
#[test]
fn the_rpc_error_and_reject_listings_are_reached_by_the_gate() {
    let declared = std_enums();
    for (name, arms) in [
        (
            "RpcError",
            vec![
                "Transport",
                "Decode",
                "Remote",
                "Contract",
                "Unauthorized",
                "Unavailable",
            ],
        ),
        (
            "Reject",
            vec!["Unauthorized", "Forbidden", "TooMany", "Unavailable"],
        ),
    ] {
        let (source, declared_arms) = declared
            .get(name)
            .unwrap_or_else(|| panic!("std no longer declares `{name}`"));
        assert_eq!(
            declared_arms, &arms,
            "`{name}`'s arms moved in {source}; the exhibit list here is what E159 \
             was filed against and wants re-reading, not silently updating"
        );
    }
    // `RpcError` has a fence in the reference and a counted sentence in the
    // guide; `Reject` has a table and no fence at all, which is why the table
    // form exists.
    let reference = std::fs::read_to_string(repo_root().join("vilan/docs/std/rpc.md"))
        .expect("read std/rpc.md");
    assert!(
        enum_declarations(&reference)
            .iter()
            .any(|(name, _)| name == "RpcError"),
        "std/rpc.md no longer declares `RpcError` in a fence — the fence gate has \
         nothing to hold"
    );
    let guide = std::fs::read_to_string(repo_root().join("vilan/docs/guide/services.md"))
        .expect("read guide/services.md");
    assert!(
        guide
            .lines()
            .any(|line| table_head_enum(line).is_some_and(|name| name == "Reject")),
        "guide/services.md no longer carries the `Reject` table — the table gate has \
         nothing to hold"
    );
    assert!(
        prose_blocks(&without_fences(&guide))
            .iter()
            .any(|(_, block)| counted_arms(block).is_some() && block.contains("`RpcError`")),
        "guide/services.md no longer counts `RpcError`'s variants — the prose gate \
         has nothing to hold"
    );
}

/// The reader itself, proven on a body it cannot get from the tree: a `///` arm
/// doc full of commas, a payload arm, a backed arm, and a trailing comma.
#[test]
fn the_arm_reader_survives_docs_payloads_and_backings() {
    let source = "[derive(Debug)]\nenum Probe {\n\
         \t/// One, two, three — a doc with commas in it.\n\
         \tFirst(str),\n\
         \tSecond = -1,\n\
         \tThird(Map<str, i32>),\n\
         }\n";
    assert_eq!(
        enum_declarations(source),
        vec![(
            "Probe".to_string(),
            vec![
                "First".to_string(),
                "Second".to_string(),
                "Third".to_string()
            ]
        )]
    );
    // A one-line spelling, which is how the book writes a small enum.
    assert_eq!(
        enum_declarations("enum Small<T> { Yes(T), No }\n"),
        vec![(
            "Small".to_string(),
            vec!["Yes".to_string(), "No".to_string()]
        )]
    );
    // The word `enum` inside a sentence declares nothing.
    assert!(enum_declarations("a backed enum Foo { lowers to").is_empty());
}
