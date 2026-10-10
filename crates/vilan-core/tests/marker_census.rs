//! The MARKER CENSUS (B485): every keyword and every attribute, the positions
//! each is accepted at, and what each one changes — generated from the
//! compiler's own tables and its own parser and analyzer, and byte-held to
//! `marker_census.golden.md` beside this file.
//!
//! B485 asks which words should be keywords and which bracket attributes. A
//! paper answering that needs the facts first, and a hand-written list of them
//! drifts: three copies of the attribute list (the grammar chapter, the
//! lexical chapter, the appendix) already disagree with each other about
//! `resource` and `hint`. So nothing here is a list of FACTS:
//!
//! - the MARKERS are the rows of `lexing::KEYWORDS`, `lexing::CONTEXTUAL_KEYWORDS`
//!   and `parsing::KNOWN_ATTRIBUTE_MARKERS`, plus one user macro attribute (any
//!   other name in `[…]`). A row added to any of the three tables reds this
//!   test until it is classified below;
//! - the POSITIONS each marker is accepted at are PROBED: each position is a
//!   small program with a hole, the marker is written into the hole, and the
//!   parser (then, if the parser took it, the analyzer) is asked. The program
//!   without the marker must parse clean, which keeps a probe from rotting
//!   into one that measures its own typo;
//! - the STACKING ORDER is probed the same way, pairwise, for every two
//!   markers a position takes one at a time.
//!
//! What IS hand-written is the classification column — what a marker changes
//! (typing, ownership class, visibility/linkage, tooling only, code
//! generation; or that it is a construct, a literal, a name, a clause) — and
//! the attribute spelling each probe writes. Both are tables keyed by the
//! compiler's rows and checked against them in both directions.
//!
//! Regenerate after a deliberate change:
//! `VILAN_REGENERATE_MARKER_CENSUS=1 cargo test -p vilan-core --test marker_census`.
//! Its own test binary because it analyzes a few hundred programs: under
//! nextest it is one process, and it never shares a cache with a pin.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use vilan_core::lexing::{CONTEXTUAL_KEYWORDS, KEYWORDS};
use vilan_core::parsing::{self, KNOWN_ATTRIBUTE_MARKERS, ParseErrorReason};
use vilan_core::{PackageSpec, Platform, Workspace, analyze_source};

const REGENERATE_ENV: &str = "VILAN_REGENERATE_MARKER_CENSUS";
const REGENERATE_COMMAND: &str =
    "VILAN_REGENERATE_MARKER_CENSUS=1 cargo test -p vilan-core --test marker_census";

/// The hole a position's program leaves for the marker.
const HOLE: &str = "MARK";

/// What a marker changes, in B485's five classes, or what kind of word it is
/// when it marks nothing.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum Class {
    /// Changes how the item is typed, called or evaluated.
    Typing,
    /// Changes the ownership class of a type or a value's convention.
    Ownership,
    /// Changes who can name the item, or what it links to.
    Visibility,
    /// Changes what tools say (a warning, an editor treatment); the program
    /// means the same without it.
    Tooling,
    /// Generates code (a macro expansion or a compiler-written surface).
    CodeGeneration,
    /// Introduces a construct (a declaration, a control form, an operator).
    Construct,
    /// A literal value.
    Literal,
    /// A name with a fixed meaning.
    Name,
    /// A connective inside another construct's syntax (`with`, `as`, `only`).
    Clause,
}

impl Class {
    fn label(self) -> &'static str {
        match self {
            Class::Typing => "typing/semantics",
            Class::Ownership => "ownership class",
            Class::Visibility => "visibility/linkage",
            Class::Tooling => "tooling only",
            Class::CodeGeneration => "code generation",
            Class::Construct => "construct",
            Class::Literal => "literal",
            Class::Name => "name",
            Class::Clause => "clause",
        }
    }

    fn is_marker(self) -> bool {
        matches!(
            self,
            Class::Typing
                | Class::Ownership
                | Class::Visibility
                | Class::Tooling
                | Class::CodeGeneration
        )
    }
}

/// The classification of every keyword (reserved and contextual), one row per
/// table row, with the one line on what it changes.
const KEYWORD_CLASSES: &[(&str, Class, &str)] = &[
    (
        "async",
        Class::Typing,
        "the function runs as a task: its calls are awaited, and the body may `await`",
    ),
    (
        "await",
        Class::Construct,
        "the prefix operator that waits on a task",
    ),
    (
        "const",
        Class::Typing,
        "compile-time evaluation: `const let`, `const fun`, `const mut`, a `const` expression",
    ),
    ("css", Class::Construct, "a css block or rule set"),
    (
        "else",
        Class::Construct,
        "the alternative branch of `if` and `then`",
    ),
    ("enum", Class::Construct, "declares an enum"),
    (
        "export",
        Class::Visibility,
        "puts the item on the module's surface (`export(in PATH)` narrows it; `export *;` marks every item)",
    ),
    (
        "external",
        Class::Visibility,
        "a host-provided declaration: an `external fun` or `external struct` has no vilan body",
    ),
    ("false", Class::Literal, "the bool literal"),
    ("for", Class::Construct, "the loop"),
    ("fun", Class::Construct, "declares a function"),
    ("if", Class::Construct, "the conditional"),
    ("impl", Class::Construct, "declares an implementation block"),
    ("import", Class::Construct, "brings module names into scope"),
    (
        "in",
        Class::Construct,
        "separates a `for` binder from its source; `export(in PATH)`",
    ),
    ("is", Class::Construct, "the pattern test operator"),
    ("let", Class::Construct, "declares a binding"),
    (
        "macro",
        Class::CodeGeneration,
        "a compile-time function (`macro fun`), its invocation (`macro m(..)`), or an expanded block",
    ),
    ("match", Class::Construct, "the pattern match"),
    (
        "mod",
        Class::Construct,
        "declares a nested module; `mod self;` hosts file attributes",
    ),
    (
        "mut",
        Class::Typing,
        "binder mutability (`mut x = …`, `mut` parameter) and the writable view `&mut`",
    ),
    ("null", Class::Literal, "the null literal (host interop)"),
    ("ret", Class::Construct, "returns from the function"),
    ("struct", Class::Construct, "declares a struct"),
    ("trait", Class::Construct, "declares a trait"),
    ("true", Class::Literal, "the bool literal"),
    (
        "type",
        Class::Construct,
        "a `type X` binder in an impl head or a generic list",
    ),
    (
        "use",
        Class::Construct,
        "brings a type's namespace (variants) into scope",
    ),
    (
        "as",
        Class::Clause,
        "the alias on an import leaf; the type ascription (B571)",
    ),
    (
        "borrows",
        Class::Typing,
        "names the parameter a returned view projects: part of the signature",
    ),
    (
        "context",
        Class::Typing,
        "the contexts a body or a closure type reads: part of the signature",
    ),
    ("dyn", Class::Typing, "a trait-object type"),
    (
        "jump",
        Class::Construct,
        "loop control: `jump break`, `jump continue`",
    ),
    (
        "lazy",
        Class::Typing,
        "deferred evaluation: a lazy parameter takes a thunk the call site builds; a lazy binding is forced on first read",
    ),
    (
        "only",
        Class::Clause,
        "an import that brings names and no implementations",
    ),
    (
        "own",
        Class::Ownership,
        "the parameter convention that takes the argument by move",
    ),
    (
        "self",
        Class::Name,
        "the receiver; the file's own module in `mod self;`",
    ),
    ("Self", Class::Name, "the implementing type"),
    (
        "sync",
        Class::Typing,
        "a synchronous closure type: `(sync || T)` cannot suspend",
    ),
    ("then", Class::Construct, "the infix conditional"),
    ("void", Class::Name, "the unit type and value"),
    (
        "with",
        Class::Clause,
        "the trait list of an impl or a trait head",
    ),
];

/// The name a user macro attribute is probed by: any name not in
/// `KNOWN_ATTRIBUTE_MARKERS` is one.
const USER_ATTRIBUTE: &str = "user_macro";

/// The classification of every attribute, with the spelling each probe writes
/// (the attribute's representative argument form).
const ATTRIBUTE_CLASSES: &[(&str, &str, Class, &str)] = &[
    (
        "derive",
        "[derive(Debug)]",
        Class::CodeGeneration,
        "runs the named derive macros over a struct or an enum",
    ),
    (
        "service",
        "[service]",
        Class::CodeGeneration,
        "generates a service's client, routes and wire surface from a struct",
    ),
    (
        "client_service",
        "[client_service]",
        Class::CodeGeneration,
        "generates the handler-side proxy a server calls back into",
    ),
    (
        "extern",
        "[extern(\"f\")]",
        Class::Visibility,
        "binds the declaration to a host symbol (function, method, property, constructor)",
    ),
    (
        "must_use",
        "[must_use]",
        Class::Tooling,
        "warns when the function's result is dropped",
    ),
    (
        "track_caller",
        "[track_caller]",
        Class::CodeGeneration,
        "passes the caller's location as a hidden parameter, so a panic inside names the call site",
    ),
    (
        "rpc",
        "[rpc]",
        Class::CodeGeneration,
        "puts a method on a service's wire surface (Wire-checked, read by the generation)",
    ),
    (
        "trait_only",
        "[trait_only]",
        Class::Typing,
        "a trait method reachable only through a bound, never on the concrete type's surface",
    ),
    (
        "doc",
        "[doc(hidden)]",
        Class::Tooling,
        "reserved: `[doc(hidden)]` is refused with a steer",
    ),
    (
        "expose",
        "[expose]",
        Class::CodeGeneration,
        "publishes a service field as a mirrored channel (`[expose(keyed)]` keyed)",
    ),
    (
        "platform",
        "[platform(\"browser\")]",
        Class::Visibility,
        "fences the item (or, on `mod self;`, the file) to the matching platforms",
    ),
    (
        "deprecated",
        "[deprecated(\"use g\")]",
        Class::Tooling,
        "warns at every use outside std, with the steer",
    ),
    (
        "internal",
        "[internal(\"why\")]",
        Class::Tooling,
        "the editor hides, dims and explains the item; never a diagnostic",
    ),
    (
        "resource",
        "[resource]",
        Class::Ownership,
        "the owned-resource class: moved, never copied, torn down at scope end",
    ),
    (
        "hint",
        "[hint(Show)]",
        Class::Tooling,
        "the trait application an inlay hint shows the type as",
    ),
    (
        "reactive",
        "[reactive(coarse)]",
        Class::CodeGeneration,
        "a field's store knobs, read by `[derive(Storable)]` alone: `coarse` makes it one compared slot, `name = \"x\"` names its projection",
    ),
    (
        USER_ATTRIBUTE,
        "[user_macro]",
        Class::CodeGeneration,
        "a user macro attribute: the named macro rewrites the item",
    ),
];

/// One probed position: an id, what it is, and the programs that hold it (the
/// first that the marker parses in answers for the position — a `;` body and
/// a block body, say, so a marker that wants one is not refused for the other).
struct Position {
    id: &'static str,
    label: &'static str,
    programs: &'static [&'static str],
}

/// The item heads: where a declaration begins.
const ITEM_POSITIONS: &[Position] = &[
    Position {
        id: "I1",
        label: "before `fun`",
        programs: &["MARK fun f(): i32 { 1 }", "MARK fun f(): i32;"],
    },
    Position {
        id: "I2",
        label: "before `export fun`",
        programs: &[
            "MARK export fun f(): i32 { 1 }",
            "MARK export fun f(): i32;",
        ],
    },
    Position {
        id: "I3",
        label: "between `export` and `fun`",
        programs: &[
            "export MARK fun f(): i32 { 1 }",
            "export MARK fun f(): i32;",
        ],
    },
    Position {
        id: "I4",
        label: "before `struct`",
        programs: &["MARK struct S { x: i32 }", "MARK struct S;"],
    },
    Position {
        id: "I5",
        label: "before `export struct`",
        programs: &["MARK export struct S { x: i32 }", "MARK export struct S;"],
    },
    Position {
        id: "I6",
        label: "between `export` and `struct`",
        programs: &["export MARK struct S { x: i32 }", "export MARK struct S;"],
    },
    Position {
        id: "I7",
        label: "before `enum`",
        programs: &["MARK enum E { A, B }"],
    },
    Position {
        id: "I8",
        label: "before `trait`",
        programs: &["MARK trait T { fun t(self): i32; }"],
    },
    Position {
        id: "I9",
        label: "before `impl`",
        programs: &[
            "trait Show { fun show(self): i32; }\nstruct S { x: i32 }\nMARK impl S with Show { fun show(self): i32 { 1 } }",
        ],
    },
    Position {
        id: "I10",
        label: "before `export impl`",
        programs: &[
            "trait Show { fun show(self): i32; }\nstruct S { x: i32 }\nMARK export impl S with Show { fun show(self): i32 { 1 } }",
        ],
    },
    Position {
        id: "I11",
        label: "between `export` and `impl`",
        programs: &[
            "trait Show { fun show(self): i32; }\nstruct S { x: i32 }\nexport MARK impl S with Show { fun show(self): i32 { 1 } }",
        ],
    },
    Position {
        id: "I12",
        label: "before a module `let`",
        programs: &["MARK let x = 1;"],
    },
    Position {
        id: "I13",
        label: "before `export let`",
        programs: &["MARK export let x = 1;"],
    },
    Position {
        id: "I14",
        label: "between `export` and `let`",
        programs: &["export MARK let x = 1;"],
    },
    Position {
        id: "I15",
        label: "before `mod`",
        programs: &["MARK mod m { fun g() {} }"],
    },
    Position {
        id: "I16",
        label: "before `import`",
        programs: &["MARK import std::shared::Shared;"],
    },
    Position {
        id: "I17",
        label: "between `export` and `import`",
        programs: &["export MARK import std::shared::Shared;"],
    },
    Position {
        id: "I18",
        label: "before `mod self;` (the file head)",
        programs: &["MARK mod self;"],
    },
];

/// The member heads: where a member of a type or a trait begins.
const MEMBER_POSITIONS: &[Position] = &[
    Position {
        id: "M1",
        label: "before a method in an `impl`",
        programs: &["struct S { x: i32 }\nimpl S { MARK fun m(self): i32 { 1 } }"],
    },
    Position {
        id: "M2",
        label: "before a required trait method",
        programs: &["trait T { MARK fun t(self): i32; }"],
    },
    Position {
        id: "M3",
        label: "before a default trait method",
        programs: &["trait T { MARK fun t(self): i32 { 1 } }"],
    },
    Position {
        id: "M4",
        label: "before a struct field",
        programs: &["struct S { MARK x: i32 }"],
    },
    Position {
        id: "M5",
        label: "before an enum variant",
        programs: &["enum E { MARK A, B }"],
    },
];

/// Binders, types and expressions.
const BINDER_POSITIONS: &[Position] = &[
    Position {
        id: "B1",
        label: "a parameter's head",
        programs: &["fun f(MARK x: i32): i32 { 1 }"],
    },
    Position {
        id: "B2",
        label: "the receiver's head",
        programs: &["struct S { x: i32 }\nimpl S { fun m(MARK self): i32 { 1 } }"],
    },
    Position {
        id: "B3",
        label: "a closure parameter's head",
        programs: &["fun f() { let g = |MARK x: i32| x; }"],
    },
    Position {
        id: "B4",
        label: "before a local `let`",
        programs: &["fun f() { MARK let x = 1; }"],
    },
    Position {
        id: "B5",
        label: "a binding statement's head, in place of `let`",
        programs: &["fun f() { MARK x = 1; }"],
    },
    Position {
        id: "B6",
        label: "a type's head",
        programs: &["trait Show { fun show(self): i32; }\nfun f(x: MARK Show) {}"],
    },
    Position {
        id: "B7",
        label: "a closure type's head, inside its parentheses",
        programs: &["fun f(g: (MARK || void)) {}"],
    },
    Position {
        id: "B8",
        label: "before a closure literal",
        programs: &["fun f() { let g = MARK |x: i32| x; }"],
    },
    Position {
        id: "B9",
        label: "an expression's head",
        programs: &["fun g(): i32 { 1 }\nfun f(): i32 { MARK g() }"],
    },
    Position {
        id: "B10",
        label: "an expression statement's head",
        programs: &["fun g() {}\nfun f() { MARK g(); }"],
    },
];

/// Where a word stands as a NAME (the reserved-vs-contextual question).
const NAME_POSITIONS: &[Position] = &[
    Position {
        id: "N1",
        label: "a `let` binding",
        programs: &["fun f() { let MARK = 1; }"],
    },
    Position {
        id: "N2",
        label: "a parameter",
        programs: &["fun f(MARK: i32) {}"],
    },
    Position {
        id: "N3",
        label: "a free function",
        programs: &["fun MARK() {}"],
    },
    Position {
        id: "N4",
        label: "a type",
        programs: &["struct MARK { x: i32 }"],
    },
    Position {
        id: "N5",
        label: "a struct field",
        programs: &["struct S { MARK: i32 }"],
    },
    Position {
        id: "N6",
        label: "a method",
        programs: &["struct S { x: i32 }\nimpl S { fun MARK(self) {} }"],
    },
    Position {
        id: "N7",
        label: "a member read after `.`",
        programs: &["fun f(s: i32) { let y = s.MARK; }"],
    },
    Position {
        id: "N8",
        label: "a module",
        programs: &["mod MARK { }"],
    },
];

/// The positions the stacking order is probed at: an item with the marker
/// pair written in its hole.
const ORDER_POSITIONS: &[&str] = &["I1", "I3", "I4", "I8", "I9", "I12", "M1", "M2", "M4", "B1"];

/// One marker under census.
struct Marker {
    /// The table row's word (`derive`, `own`, …).
    word: &'static str,
    /// Where the row lives.
    table: &'static str,
    /// What the probe writes into a hole.
    spelling: String,
    class: Class,
    effect: &'static str,
    /// The table's own sentence for the word's position (contextual keywords
    /// carry one).
    stated_position: Option<&'static str>,
}

fn markers() -> Vec<Marker> {
    let keyword_class = |word: &str| {
        KEYWORD_CLASSES
            .iter()
            .find(|(classified, _, _)| *classified == word)
            .unwrap_or_else(|| {
                panic!("keyword `{word}` is in the lexer's tables and not classified in KEYWORD_CLASSES")
            })
    };
    let mut markers = Vec::new();
    for (word, _) in KEYWORDS {
        let (_, class, effect) = keyword_class(word);
        markers.push(Marker {
            word,
            table: "reserved",
            spelling: word.to_string(),
            class: *class,
            effect,
            stated_position: None,
        });
    }
    for (word, sentence) in CONTEXTUAL_KEYWORDS {
        let (_, class, effect) = keyword_class(word);
        markers.push(Marker {
            word,
            table: "contextual",
            spelling: word.to_string(),
            class: *class,
            effect,
            stated_position: Some(sentence),
        });
    }
    for name in KNOWN_ATTRIBUTE_MARKERS
        .iter()
        .copied()
        .chain([USER_ATTRIBUTE])
    {
        let (_, spelling, class, effect) = ATTRIBUTE_CLASSES
            .iter()
            .find(|(classified, ..)| *classified == name)
            .unwrap_or_else(|| {
                panic!("attribute `{name}` is in KNOWN_ATTRIBUTE_MARKERS and not classified in ATTRIBUTE_CLASSES")
            });
        markers.push(Marker {
            word: name,
            table: if name == USER_ATTRIBUTE {
                "user macro"
            } else {
                "attribute"
            },
            spelling: spelling.to_string(),
            class: *class,
            effect,
            stated_position: None,
        });
    }
    markers
}

/// What the compiler made of one probe.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Outcome {
    /// The parser refused it with no rule of its own (a generic expectation).
    Refused,
    /// The parser refused it with a curated rule — a steer.
    Steered(String),
    /// The parser took it and the analyzer refused it.
    Analyzed(String),
    /// Parsed and analyzed clean.
    Accepted,
}

impl Outcome {
    fn rank(&self) -> u8 {
        match self {
            Outcome::Refused => 0,
            Outcome::Steered(_) => 1,
            Outcome::Analyzed(_) => 2,
            Outcome::Accepted => 3,
        }
    }
}

fn first_line_bounded(text: &str) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    const LIMIT: usize = 220;
    if line.chars().count() > LIMIT {
        let cut: String = line.chars().take(LIMIT).collect();
        format!("{cut}…")
    } else {
        line.to_string()
    }
}

/// The parser's verdict alone: `None` when it parsed clean.
fn parse_verdict(source: &str) -> Option<Outcome> {
    let (_, errors) = parsing::parse(source);
    if errors.is_empty() {
        return None;
    }
    // A STEER is a refusal with a sentence of its own about the shape — a
    // rule. A generic expectation, a missing terminator and an unclosed or
    // unbalanced delimiter are what any unparseable text gets.
    let rule = errors.iter().find(|error| {
        matches!(
            error.reason,
            ParseErrorReason::Rule(_) | ParseErrorReason::VisibilityMarker { .. }
        )
    });
    Some(match rule {
        Some(error) => Outcome::Steered(first_line_bounded(&parsing::render(error))),
        None => Outcome::Refused,
    })
}

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// The analyzer's verdict on a program the parser took.
fn analyze_verdict(source: &str, std: &PackageSpec) -> Outcome {
    let leaked: &'static str = Box::leak(format!("{source}\nfun main() {{}}\n").into_boxed_str());
    let (_, errors) = analyze_source(
        leaked,
        std,
        Path::new("."),
        Path::new("census.vl"),
        Some(Platform::default()),
        &Workspace::default(),
    );
    match errors.first() {
        None => Outcome::Accepted,
        Some(error) => Outcome::Analyzed(first_line_bounded(&error.msg)),
    }
}

fn fill(program: &str, spelling: &str) -> String {
    program.replacen(HOLE, spelling, 1)
}

fn unmarked(program: &str) -> String {
    program
        .replacen(&format!("{HOLE} "), "", 1)
        .replacen(HOLE, "", 1)
}

/// The best outcome the marker reaches over the position's programs.
fn probe(position: &Position, spelling: &str, std: Option<&PackageSpec>) -> Outcome {
    let mut best = Outcome::Refused;
    for program in position.programs {
        let source = fill(program, spelling);
        let outcome = match parse_verdict(&source) {
            Some(refusal) => refusal,
            None => match std {
                Some(std) => analyze_verdict(&source, std),
                None => Outcome::Accepted,
            },
        };
        // Strictly better only: between two refusals of one rank the FIRST
        // program's answers, so a note never depends on how two messages sort.
        if outcome.rank() > best.rank() {
            best = outcome;
        }
        if best == Outcome::Accepted {
            break;
        }
    }
    best
}

/// A topological order of `count` markers under the one-order `edges`
/// (before, after) — the stable one (Kahn's algorithm, lowest table index
/// first), so the golden does not move with hash order — or `None` on a
/// cycle. Markers in no edge keep their table place among the rest.
fn stacking_chain(count: usize, edges: &[(usize, usize)]) -> Option<Vec<usize>> {
    let mut incoming = vec![0usize; count];
    for (_, after) in edges {
        incoming[*after] += 1;
    }
    let mut placed = vec![false; count];
    let mut chain = Vec::with_capacity(count);
    while chain.len() < count {
        let next = (0..count).find(|index| !placed[*index] && incoming[*index] == 0)?;
        placed[next] = true;
        chain.push(next);
        for (before, after) in edges {
            if *before == next {
                incoming[*after] -= 1;
            }
        }
    }
    Some(chain)
}

/// Numbered footnotes for the messages cells cite, in first-cited order.
#[derive(Default)]
struct Notes {
    numbered: Vec<(char, String)>,
}

impl Notes {
    fn cite(&mut self, kind: char, message: &str) -> usize {
        if let Some(index) = self
            .numbered
            .iter()
            .position(|(noted_kind, noted)| *noted_kind == kind && noted == message)
        {
            return index + 1;
        }
        self.numbered.push((kind, message.to_string()));
        self.numbered.len()
    }
}

fn cell(outcome: &Outcome, notes: &mut Notes) -> String {
    match outcome {
        Outcome::Accepted => "✓".to_string(),
        Outcome::Analyzed(message) => format!("a{}", notes.cite('a', message)),
        Outcome::Steered(message) => format!("s{}", notes.cite('s', message)),
        Outcome::Refused => "·".to_string(),
    }
}

fn find_position(id: &str) -> &'static Position {
    ITEM_POSITIONS
        .iter()
        .chain(MEMBER_POSITIONS)
        .chain(BINDER_POSITIONS)
        .find(|position| position.id == id)
        .unwrap_or_else(|| panic!("no position {id}"))
}

/// Every (marker, position) probe of section 2, run across worker threads: a
/// few hundred analyses one after another are a minute of a debug build, and
/// each is independent of the others (the caches they share are the
/// process-global ones the language server shares across threads).
fn probe_all(
    markers: &[Marker],
    positions: &[&'static Position],
    std: &PackageSpec,
) -> BTreeMap<(&'static str, &'static str), Outcome> {
    let jobs: Vec<(&'static str, &str, &'static Position)> = markers
        .iter()
        .flat_map(|marker| {
            positions
                .iter()
                .map(move |position| (marker.word, marker.spelling.as_str(), *position))
        })
        .collect();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results = std::sync::Mutex::new(BTreeMap::new());
    let workers = std::thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(4)
        .min(8);
    std::thread::scope(|scope| {
        for _ in 0..workers {
            std::thread::Builder::new()
                .stack_size(256 * 1024 * 1024)
                .spawn_scoped(scope, || {
                    loop {
                        let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some((word, spelling, position)) = jobs.get(index) else {
                            break;
                        };
                        let outcome = probe(position, spelling, Some(std));
                        results
                            .lock()
                            .unwrap()
                            .insert((*word, position.id), outcome);
                    }
                })
                .expect("spawn a census worker");
        }
    });
    results.into_inner().unwrap()
}

fn matrix(
    out: &mut String,
    title: &str,
    positions: &[Position],
    markers: &[Marker],
    outcomes: &BTreeMap<(&'static str, &'static str), Outcome>,
    notes: &mut Notes,
    accepted: &mut BTreeMap<(&'static str, &'static str), bool>,
) {
    writeln!(out, "### {title}\n").unwrap();
    for position in positions {
        writeln!(
            out,
            "- **{}** — {}: `{}`",
            position.id,
            position.label,
            position.programs[0].replace('\n', " ⏎ ")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    write!(out, "| marker |").unwrap();
    for position in positions {
        write!(out, " {} |", position.id).unwrap();
    }
    writeln!(out).unwrap();
    write!(out, "|---|").unwrap();
    for _ in positions {
        write!(out, ":-:|").unwrap();
    }
    writeln!(out).unwrap();
    for marker in markers {
        write!(out, "| `{}` |", marker.spelling).unwrap();
        for position in positions {
            let outcome = &outcomes[&(marker.word, position.id)];
            accepted.insert(
                (marker.word, position.id),
                matches!(outcome, Outcome::Accepted | Outcome::Analyzed(_)),
            );
            write!(out, " {} |", cell(outcome, notes)).unwrap();
        }
        writeln!(out).unwrap();
    }
    writeln!(out).unwrap();
}

fn census() -> String {
    let std = std_spec();
    let markers = markers();
    // Every probe program must parse clean WITHOUT its marker, or a refusal
    // would be the program's and not the marker's.
    for position in ITEM_POSITIONS
        .iter()
        .chain(MEMBER_POSITIONS)
        .chain(BINDER_POSITIONS)
    {
        for program in position.programs {
            let bare = unmarked(program);
            assert!(
                parse_verdict(&bare).is_none(),
                "position {}'s program does not parse without a marker: {bare:?}",
                position.id
            );
        }
    }
    for position in NAME_POSITIONS {
        let plain = fill(position.programs[0], "plain");
        assert!(
            parse_verdict(&plain).is_none(),
            "name position {}'s program does not parse with a plain name: {plain:?}",
            position.id
        );
    }

    let mut out = String::new();
    writeln!(out, "# Marker census (B485)\n").unwrap();
    writeln!(
        out,
        "GENERATED by `crates/vilan-core/tests/marker_census.rs` from `lexing::KEYWORDS`, \
         `lexing::CONTEXTUAL_KEYWORDS` and `parsing::KNOWN_ATTRIBUTE_MARKERS`, with every \
         position PROBED through `parsing::parse` and `analyze_source` (node platform). \
         Regenerate: `{REGENERATE_COMMAND}`.\n"
    )
    .unwrap();

    // --- 1. The tables -----------------------------------------------------
    writeln!(
        out,
        "## 1. Every keyword and attribute, and what it changes\n"
    )
    .unwrap();
    writeln!(
        out,
        "`class` is B485's question — typing/semantics, ownership class, visibility/linkage, \
         tooling only, code generation — for a MARKER; a word that marks nothing is a \
         construct, a literal, a name or a clause. The class and the line are the census's \
         one hand-written column, keyed to the tables and checked against them.\n"
    )
    .unwrap();
    writeln!(
        out,
        "| word | table | class | what it changes | stated position |"
    )
    .unwrap();
    writeln!(out, "|---|---|---|---|---|").unwrap();
    for marker in &markers {
        let shown = match marker.table {
            "attribute" | "user macro" => marker.spelling.clone(),
            _ => marker.word.to_string(),
        };
        writeln!(
            out,
            "| `{shown}` | {} | {} | {} | {} |",
            marker.table,
            marker.class.label(),
            marker.effect,
            marker.stated_position.unwrap_or("")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    let mut by_class: BTreeMap<Class, Vec<String>> = BTreeMap::new();
    for marker in &markers {
        let shown = match marker.table {
            "attribute" | "user macro" => format!("`[{}]`", marker.word),
            _ => format!("`{}`", marker.word),
        };
        by_class.entry(marker.class).or_default().push(shown);
    }
    writeln!(out, "By class:\n").unwrap();
    for (class, words) in &by_class {
        writeln!(
            out,
            "- **{}** ({}): {}",
            class.label(),
            words.len(),
            words.join(", ")
        )
        .unwrap();
    }
    writeln!(out).unwrap();

    // --- 2. Positions ------------------------------------------------------
    let marker_rows: Vec<Marker> = markers
        .into_iter()
        .filter(|marker| marker.class.is_marker() || marker.table == "contextual")
        .collect();
    writeln!(out, "## 2. Where each marker is accepted\n").unwrap();
    writeln!(
        out,
        "Rows: every MARKER (the five classes) and every contextual keyword. Cells: `✓` parses \
         and analyzes clean; `aN` parses and the analyzer refuses (note aN); `sN` the parser \
         refuses with a rule of its own, a steer (note sN); `·` the parser refuses with no \
         rule (a generic \"found X expected Y\"). A position with two programs (a block body \
         and a `;` body) answers with the better.\n"
    )
    .unwrap();
    let probed: Vec<&'static Position> = ITEM_POSITIONS
        .iter()
        .chain(MEMBER_POSITIONS)
        .chain(BINDER_POSITIONS)
        .collect();
    // One analysis first, alone: it builds the base world every probe then
    // clones. Started cold, the workers all wait on that one build and the
    // census takes twice as long (measured: 51 s against 23 s, debug).
    assert_eq!(analyze_verdict("", &std), Outcome::Accepted);
    let outcomes = probe_all(&marker_rows, &probed, &std);
    let mut notes = Notes::default();
    let mut accepted = BTreeMap::new();
    matrix(
        &mut out,
        "2.1 Item heads",
        ITEM_POSITIONS,
        &marker_rows,
        &outcomes,
        &mut notes,
        &mut accepted,
    );
    matrix(
        &mut out,
        "2.2 Member heads",
        MEMBER_POSITIONS,
        &marker_rows,
        &outcomes,
        &mut notes,
        &mut accepted,
    );
    matrix(
        &mut out,
        "2.3 Binders, types and expressions",
        BINDER_POSITIONS,
        &marker_rows,
        &outcomes,
        &mut notes,
        &mut accepted,
    );

    // A summary line per marker: where it is taken at all.
    writeln!(out, "### 2.4 Accepted positions, per marker\n").unwrap();
    writeln!(
        out,
        "The positions whose cell is `✓` or `aN` (the parser takes the marker there).\n"
    )
    .unwrap();
    for marker in &marker_rows {
        let taken: Vec<&str> = ITEM_POSITIONS
            .iter()
            .chain(MEMBER_POSITIONS)
            .chain(BINDER_POSITIONS)
            .filter(|position| accepted[&(marker.word, position.id)])
            .map(|position| position.id)
            .collect();
        let taken = if taken.is_empty() {
            "none of the probed positions".to_string()
        } else {
            taken.join(", ")
        };
        writeln!(out, "- `{}`: {taken}", marker.spelling).unwrap();
    }
    writeln!(out).unwrap();

    // --- 3. Stacking order -------------------------------------------------
    writeln!(out, "## 3. Stacking order\n").unwrap();
    writeln!(
        out,
        "For each position, every two markers the PARSER takes there one at a time, written \
         both ways round. `A < B`: only A-then-B parses. `A <> B`: either order parses. \
         `A × B`: neither does (they do not stack). Parser only.\n"
    )
    .unwrap();
    for id in ORDER_POSITIONS {
        let position = find_position(id);
        let taken: Vec<&Marker> = marker_rows
            .iter()
            .filter(|marker| accepted[&(marker.word, position.id)])
            .collect();
        writeln!(out, "### {} — {}\n", position.id, position.label).unwrap();
        if taken.len() < 2 {
            writeln!(out, "Fewer than two markers are taken here.\n").unwrap();
            continue;
        }
        let parses = |first: &Marker, second: &Marker| {
            position.programs.iter().any(|program| {
                let pair = format!("{} {}", first.spelling, second.spelling);
                parse_verdict(&fill(program, &pair)).is_none()
            })
        };
        let (mut either, mut ordered, mut neither) = (Vec::new(), Vec::new(), Vec::new());
        // `edges` holds (before, after) indices into `taken` for every pair
        // that stacks in one order only.
        let mut edges: Vec<(usize, usize)> = Vec::new();
        for (index, first) in taken.iter().enumerate() {
            for (offset, second) in taken[index + 1..].iter().enumerate() {
                let other = index + 1 + offset;
                match (parses(first, second), parses(second, first)) {
                    (true, true) => {
                        either.push(format!("`{}` <> `{}`", first.spelling, second.spelling))
                    }
                    (true, false) => {
                        edges.push((index, other));
                        ordered.push(format!("`{}` < `{}`", first.spelling, second.spelling));
                    }
                    (false, true) => {
                        edges.push((other, index));
                        ordered.push(format!("`{}` < `{}`", second.spelling, first.spelling));
                    }
                    (false, false) => {
                        neither.push(format!("`{}` × `{}`", first.spelling, second.spelling))
                    }
                }
            }
        }
        let taken_list: Vec<String> = taken
            .iter()
            .map(|marker| format!("`{}`", marker.spelling))
            .collect();
        writeln!(out, "Taken alone: {}.\n", taken_list.join(", ")).unwrap();
        match stacking_chain(taken.len(), &edges) {
            Some(chain) if !edges.is_empty() => {
                let chain: Vec<String> = chain
                    .iter()
                    .map(|index| format!("`{}`", taken[*index].spelling))
                    .collect();
                writeln!(
                    out,
                    "- The order every one-order pair agrees with: {}",
                    chain.join(" < ")
                )
                .unwrap();
            }
            Some(_) => {}
            None => writeln!(
                out,
                "- The one-order pairs contain a CYCLE: no single order"
            )
            .unwrap(),
        }
        for (label, list) in [
            ("One order only", &ordered),
            ("Either order", &either),
            ("Do not stack", &neither),
        ] {
            if list.is_empty() {
                continue;
            }
            writeln!(out, "- {label}: {}", list.join("; ")).unwrap();
        }
        writeln!(out).unwrap();
    }

    // --- 4. As names ---------------------------------------------------------
    writeln!(out, "## 4. Each word as a NAME\n").unwrap();
    writeln!(
        out,
        "Whether the word may NAME something (parser only): a reserved keyword never can \
         outside the member tier (B414 S4); a contextual keyword and an attribute name are \
         ordinary names. `✓` parses, `sN` a steer, `·` refused.\n"
    )
    .unwrap();
    for position in NAME_POSITIONS {
        writeln!(
            out,
            "- **{}** — {}: `{}`",
            position.id,
            position.label,
            position.programs[0].replace('\n', " ⏎ ")
        )
        .unwrap();
    }
    writeln!(out).unwrap();
    write!(out, "| word |").unwrap();
    for position in NAME_POSITIONS {
        write!(out, " {} |", position.id).unwrap();
    }
    writeln!(out).unwrap();
    write!(out, "|---|").unwrap();
    for _ in NAME_POSITIONS {
        write!(out, ":-:|").unwrap();
    }
    writeln!(out).unwrap();
    let names: Vec<&str> = KEYWORDS
        .iter()
        .map(|(word, _)| *word)
        .chain(CONTEXTUAL_KEYWORDS.iter().map(|(word, _)| *word))
        .chain(KNOWN_ATTRIBUTE_MARKERS.iter().copied())
        .collect();
    for word in names {
        write!(out, "| `{word}` |").unwrap();
        for position in NAME_POSITIONS {
            let outcome = probe(position, word, None);
            write!(out, " {} |", cell(&outcome, &mut notes)).unwrap();
        }
        writeln!(out).unwrap();
    }
    writeln!(out).unwrap();

    // --- Notes ---------------------------------------------------------------
    writeln!(out, "## Notes\n").unwrap();
    for (index, (kind, message)) in notes.numbered.iter().enumerate() {
        let message = message.replace('|', "\\|");
        writeln!(out, "- **{kind}{}** {message}", index + 1).unwrap();
    }
    out
}

#[test]
fn every_table_row_is_classified_and_every_classification_is_a_row() {
    for (word, _, _) in KEYWORD_CLASSES {
        assert!(
            KEYWORDS.iter().any(|(reserved, _)| reserved == word)
                || CONTEXTUAL_KEYWORDS
                    .iter()
                    .any(|(contextual, _)| contextual == word),
            "KEYWORD_CLASSES classifies `{word}`, which is in neither keyword table"
        );
    }
    for (name, spelling, _, _) in ATTRIBUTE_CLASSES {
        assert!(
            *name == USER_ATTRIBUTE || KNOWN_ATTRIBUTE_MARKERS.contains(name),
            "ATTRIBUTE_CLASSES classifies `{name}`, which is not in KNOWN_ATTRIBUTE_MARKERS"
        );
        assert!(
            spelling.starts_with(&format!("[{name}")),
            "`{name}`'s probe spelling {spelling:?} spells another attribute"
        );
    }
    assert!(
        !KNOWN_ATTRIBUTE_MARKERS.contains(&USER_ATTRIBUTE),
        "the user-macro probe name must not be a built-in attribute"
    );
    // The lookups panic on a missing row.
    assert_eq!(
        markers().len(),
        KEYWORDS.len() + CONTEXTUAL_KEYWORDS.len() + KNOWN_ATTRIBUTE_MARKERS.len() + 1
    );
}

#[test]
fn the_census_is_current() {
    let generated = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(census)
        .expect("spawn the census worker")
        .join()
        .expect("the census worker panicked");
    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/marker_census.golden.md");
    if std::env::var_os(REGENERATE_ENV).is_some() {
        std::fs::write(&golden, &generated).expect("write the census golden");
        return;
    }
    let committed = std::fs::read_to_string(&golden).unwrap_or_default();
    assert!(
        committed == generated,
        "the marker census moved — a keyword, an attribute or a position changed. Read the \
         diff, then regenerate: {REGENERATE_COMMAND}"
    );
}
