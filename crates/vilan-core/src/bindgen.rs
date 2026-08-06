//! `vilan bindgen` — generating `external` bindings from TypeScript headers
//! (`proposal/bindgen.md`, backlog E31).
//!
//! A `.d.ts` describes a JavaScript library's surface precisely enough that the
//! `external struct` + `[extern(…)]` `external fun` dialect std writes by hand
//! can be generated from it. This module is that generator: source-to-source,
//! no compiler feature involved. Its output is ordinary vilan the developer
//! reviews, edits, and commits (§1) — never a build-time artifact.
//!
//! # The invariant
//!
//! **An unmappable TypeScript construct never disappears silently.** Every
//! member bindgen cannot express becomes a `// TODO(bindgen): …` comment naming
//! the construct and why it did not map. A generated file with TODOs is
//! reviewable; a generated file with silent gaps is a landmine.
//!
//! # What crosses a host boundary (the correction that shapes this module)
//!
//! `proposal/bindgen.md`'s type table had three rows that mapped a host
//! aggregate onto a vilan aggregate. All three were verified against the
//! running compiler at take-up, and all three are wrong for one shared root
//! cause: **vilan's aggregate types have vilan-owned runtime representations
//! that do not match the host's.**
//!
//! - A plain `struct` lowers to a POSITIONAL ARRAY (`struct P { x: f64 }` is
//!   `[x]`, and `p.x` is `p[0]`). A host object `{x: 1}` read through it yields
//!   `undefined`, silently.
//! - `enum` lowers to `[tag, …payload]`. A TS discriminated union is a tagged
//!   *object* (`{kind: "circle", r: 2}`); matching one as an enum reads
//!   `value[0]`, misses every arm, and crashes.
//! - `std::map::Map<K, V>` is a plain struct wrapping a `NativeMap` keyed by
//!   `key.hash()` — nothing like a host `{a: 1}` object.
//! - `List<T>` is a native JS *array*. An array-LIKE object (`{[index: number]:
//!   T}` — NodeList-shaped: numeric keys and `length`, no `Symbol.iterator`)
//!   is not one: `for`-in over it throws `TypeError: … is not iterable`, and
//!   `map`/`filter`/`fold`/`reverse` are all built on `for`-in. A real array
//!   with HOLES fares no better — each hole arrives as `undefined` in a slot
//!   typed `T`.
//!
//! Only an `external struct` — an opaque handle whose fields are reached
//! through `[extern(get/set, …)]` — crosses the boundary intact. So every
//! object shape bindgen sees becomes an `external struct`, and the three
//! aggregate rows are TODOs rather than mappings. This turns §3.8's v1
//! recommendation ("`external struct` always") from an ergonomics judgment call
//! into a correctness requirement.
//!
//! Tuples are the exception that proves the rule: a vilan tuple `(A, B)` IS a
//! JS array `[a, b]`, so a TS tuple type maps across exactly.

pub mod dts;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;

use dts::{
    ClassDeclaration, Declaration, GenericParameter, IndexKey, InterfaceDeclaration, Member,
    MethodMember, Parameter, PropertyMember, Signature, TsType,
};

use crate::target::PlatformPattern;

/// How `generate` was invoked.
pub struct Options {
    /// The `[platform("…")]` fence stamped on every emitted `external fun`
    /// (§4). Required — there is no default, because a wrong inferred guess
    /// baked into checked-in source is worse than a flag a human chose once.
    pub platform: String,
    /// The input file's display name, for the generated header.
    pub source_name: String,
}

impl Options {
    /// Rejects a `--platform` value the language itself would not accept as a
    /// `[platform(…)]` pattern, so a typo fails at generation time rather than
    /// producing a file full of unparseable fences.
    pub fn validate(&self) -> Result<(), String> {
        match PlatformPattern::parse(&self.platform) {
            Some(_) => Ok(()),
            None => Err(format!(
                "unknown platform `{}` (expected `node`, `deno`, `bun`, `browser`, or `@process`)",
                self.platform
            )),
        }
    }
}

/// What one run produced.
pub struct Generated {
    /// The emitted vilan source, already passed through the formatter.
    pub source: String,
    pub coverage: Coverage,
}

/// Coverage accounting — how much of a `.d.ts` bound cleanly, and what did not.
///
/// This is not decoration: §6's "fourth, softer check" wants bindgen's coverage
/// claims measurable rather than asserted, and the E31 probe against
/// `lib.dom.d.ts` is exactly that measurement.
#[derive(Debug, Default)]
pub struct Coverage {
    /// Top-level declarations read from the file.
    pub declarations: usize,
    /// Declarations that produced at least one vilan item.
    pub declarations_bound: usize,
    /// Declarations that produced nothing, by construct class.
    pub declarations_skipped: BTreeMap<String, usize>,
    /// Members (properties, methods, constructors, index signatures) seen
    /// across every bound declaration, after inheritance flattening.
    pub members: usize,
    /// Members that produced at least one `external fun`.
    pub members_bound: usize,
    /// Every TODO emitted, by construct class.
    pub todos: BTreeMap<String, usize>,
    /// Values whose TypeScript type admitted `null`/`undefined`. Counted rather
    /// than TODO'd: the bare type IS bound, and vilan simply cannot say the
    /// value may be missing. Kept in the report so the honesty is measurable.
    pub absent_able: usize,
}

impl Coverage {
    pub fn total_todos(&self) -> usize {
        self.todos.values().sum()
    }

    fn note_todo(&mut self, construct: &str) {
        *self.todos.entry(construct.to_string()).or_default() += 1;
    }

    fn note_absence(&mut self) {
        self.absent_able += 1;
    }

    fn note_skipped(&mut self, construct: &str) {
        *self
            .declarations_skipped
            .entry(construct.to_string())
            .or_default() += 1;
    }

    /// A human-readable coverage summary (`vilan bindgen --stats`).
    pub fn report(&self) -> String {
        let mut out = String::new();
        let percentage = |part: usize, whole: usize| {
            if whole == 0 {
                100.0
            } else {
                part as f64 * 100.0 / whole as f64
            }
        };
        let _ = writeln!(
            out,
            "declarations: {}/{} bound ({:.1}%)",
            self.declarations_bound,
            self.declarations,
            percentage(self.declarations_bound, self.declarations)
        );
        let _ = writeln!(
            out,
            "members:      {}/{} bound ({:.1}%)",
            self.members_bound,
            self.members,
            percentage(self.members_bound, self.members)
        );
        let _ = writeln!(out, "TODOs:        {}", self.total_todos());
        let _ = writeln!(
            out,
            "absent-able:  {} value(s) whose TS type admits null/undefined",
            self.absent_able
        );
        if !self.declarations_skipped.is_empty() {
            let _ = writeln!(out, "\nskipped declarations, by construct:");
            let mut rows: Vec<_> = self.declarations_skipped.iter().collect();
            rows.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
            for (construct, count) in rows {
                let _ = writeln!(out, "  {count:>7}  {construct}");
            }
        }
        if !self.todos.is_empty() {
            let _ = writeln!(out, "\nTODOs, by construct:");
            let mut rows: Vec<_> = self.todos.iter().collect();
            rows.sort_by(|left, right| right.1.cmp(left.1).then(left.0.cmp(right.0)));
            for (construct, count) in rows {
                let _ = writeln!(out, "  {count:>7}  {construct}");
            }
        }
        out
    }
}

/// Parses `source` as a `.d.ts` and emits a vilan bindings module.
pub fn generate(source: &str, options: &Options) -> Generated {
    let file = dts::parse(source);
    let mut emitter = Emitter::new(options);
    emitter.collect(&file);
    emitter.emit(&file);
    let coverage = std::mem::take(&mut emitter.coverage);
    let raw = emitter.finish();
    // §1: everything goes through the same formatter `vilan fmt` uses, so
    // generated code is indistinguishable in style from hand-written std code.
    // `format` returns the input unchanged when it cannot print a construct,
    // which is why the emitter still emits tidy source of its own.
    Generated {
        source: crate::formatter::format(&raw),
        coverage,
    }
}

// --- Names -------------------------------------------------------------------

/// Vilan's reserved words plus the built-in type names a generated binding must
/// not shadow. A TS member landing on one of these is suffixed with `_`.
const RESERVED: &[&str] = &[
    "any", "async", "await", "bool", "borrows", "const", "else", "enum", "export", "external",
    "f32", "f64", "false", "for", "fun", "i16", "i32", "i53", "i8", "if", "impl", "import", "in",
    "is", "jump", "let", "macro", "match", "mod", "mut", "null", "own", "resource", "ret", "self",
    "str", "struct", "trait", "true", "type", "u16", "u32", "u53", "u8", "use", "void", "with",
    "BigInt", "List", "Map", "Option", "Set",
];

fn escape_reserved(name: &str) -> String {
    if RESERVED.contains(&name) {
        return format!("{name}_");
    }
    name.to_string()
}

/// `getElementById` → `get_element_by_id`, matching the hand-written std
/// dialect (`dom.vl`). A `_` is inserted before an uppercase letter that either
/// follows a lowercase letter or digit, or begins a word inside an acronym run
/// (`XMLHttpRequest` → `xml_http_request`, `Uint8Array` → `uint8_array`).
fn to_snake_case(name: &str) -> String {
    let characters: Vec<char> = name.chars().collect();
    let mut out = String::with_capacity(name.len() + 4);
    for (index, character) in characters.iter().enumerate() {
        if character.is_uppercase() && index > 0 {
            let previous = characters[index - 1];
            let next = characters.get(index + 1).copied();
            let starts_word = previous.is_lowercase()
                || previous.is_ascii_digit()
                || (previous.is_uppercase() && next.is_some_and(|next| next.is_lowercase()));
            if starts_word && !out.ends_with('_') {
                out.push('_');
            }
        }
        for lowered in character.to_lowercase() {
            out.push(lowered);
        }
    }
    // A member named in a string literal can carry anything (`"content-type"`).
    let cleaned: String = out
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches('_').to_string();
    if cleaned.is_empty() || cleaned.starts_with(|character: char| character.is_ascii_digit()) {
        return format!("_{cleaned}");
    }
    cleaned
}

/// `create-server` / `createServer` → `CreateServer`, for a synthesized type
/// name (§3.8.2). Derived only from the enclosing symbol and the member's own
/// name, never a traversal counter, so it is stable across runs (§6 gate 3).
fn to_pascal_case(name: &str) -> String {
    let joined: String = to_snake_case(name)
        .split('_')
        .filter(|segment| !segment.is_empty())
        .map(|segment| {
            let mut characters = segment.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect();
    // A TS string literal can be anything — `"2d"` is a real member of
    // `OffscreenRenderingContextId`. An identifier cannot start with a digit,
    // so one that would is prefixed rather than emitted broken.
    if joined.starts_with(|character: char| character.is_ascii_digit()) {
        return format!("_{joined}");
    }
    joined
}

// --- The emitter -------------------------------------------------------------

/// Where a type sits, which changes how `Promise` and absence map.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    Parameter,
    Return,
    /// Inside another type (a generic argument, a closure's own parameter).
    Nested,
}

/// A mapped type: the vilan text, plus whatever could not be mapped exactly.
struct Mapped {
    text: String,
    todos: Vec<String>,
    /// Set when a `Promise<T>` was unwrapped in return position — the enclosing
    /// binding must be `async` (§3.6).
    is_async: bool,
    /// Remarks about a mapping that is TOTAL but less precise than
    /// TypeScript's — rendered as `///` doc comments on the binding, not as
    /// TODOs. The distinction matters at scale: `lib.dom.d.ts` has 33k
    /// `T | null` returns, and burying 6.5k real TODOs under them would defeat
    /// the point of marking anything.
    notes: Vec<String>,
    /// The string-literal-union enum this parameter really wants, when the
    /// mapped text is the raw `str` standing in for it (§3.3).
    string_enum: Option<String>,
}

impl Mapped {
    fn plain(text: impl Into<String>) -> Self {
        Mapped {
            text: text.into(),
            todos: Vec::new(),
            is_async: false,
            notes: Vec::new(),
            string_enum: None,
        }
    }

    fn todo(text: impl Into<String>, todo: impl Into<String>) -> Self {
        Mapped {
            text: text.into(),
            todos: vec![todo.into()],
            is_async: false,
            notes: Vec::new(),
            string_enum: None,
        }
    }
}

/// A string-literal-union alias, and the enum bindgen emits for it.
struct StringEnum {
    /// Variant name in vilan, paired with the raw JS string it stands for.
    variants: Vec<(String, String)>,
}

struct Emitter<'options> {
    options: &'options Options,
    out: String,
    coverage: Coverage,
    /// Every type name declared in this file, so a reference to one resolves.
    declared: HashSet<String>,
    /// String-literal-union aliases, by name.
    string_enums: HashMap<String, StringEnum>,
    /// TRANSPARENT aliases — `type GLenum = number`, `type Float32List =
    /// Float32Array | number[]` — whose right-hand shape maps under the table
    /// without needing a nominal declaration of its own. vilan has no type
    /// alias (§5), so these are substituted at every reference rather than
    /// declared; without that, `lib.dom.d.ts` alone reports ~1500 references to
    /// types it plainly declares.
    transparent_aliases: HashMap<String, (Vec<GenericParameter>, TsType)>,
    /// The aliases currently being expanded, so a cyclic alias terminates.
    expanding: Vec<String>,
    /// Interfaces by name, for flattening `extends` chains.
    interfaces: HashMap<String, InterfaceDeclaration>,
    /// Type parameters in scope while mapping the current declaration.
    scope: Vec<String>,
    /// `external struct`s synthesized for anonymous object types, flushed
    /// before the declaration that needed them.
    pending_types: Vec<String>,
    /// Synthesized names already taken, so a second collision gets a suffix.
    synthesized: BTreeSet<String>,
}

impl<'options> Emitter<'options> {
    fn new(options: &'options Options) -> Self {
        Emitter {
            options,
            out: String::new(),
            coverage: Coverage::default(),
            declared: HashSet::new(),
            string_enums: HashMap::new(),
            transparent_aliases: HashMap::new(),
            expanding: Vec::new(),
            interfaces: HashMap::new(),
            scope: Vec::new(),
            pending_types: Vec::new(),
            synthesized: BTreeSet::new(),
        }
    }

    /// Pass one: learn every name the file declares, so a forward reference
    /// resolves and an unknown one can be honestly TODO'd.
    fn collect(&mut self, file: &dts::DeclarationFile) {
        for declaration in &file.declarations {
            match declaration {
                Declaration::Interface(interface) => {
                    self.declared.insert(interface.name.clone());
                }
                Declaration::Class(class) => {
                    self.declared.insert(class.name.clone());
                }
                Declaration::TypeAlias(alias) => {
                    // ONLY the alias shapes that actually produce a vilan
                    // declaration are declared: vilan has no type alias (§5), so
                    // an alias of, say, an open union emits a TODO and no type.
                    // Registering its name anyway would let other members
                    // reference a type that was never written, and the whole
                    // generated file would stop compiling.
                    if let Some(variants) = string_literal_variants(&alias.value) {
                        self.declared.insert(alias.name.clone());
                        self.string_enums
                            .insert(alias.name.clone(), StringEnum { variants });
                    } else if matches!(alias.value, TsType::Object(_)) {
                        self.declared.insert(alias.name.clone());
                    } else {
                        // Everything else is TRANSPARENT: `type GLenum = number`
                        // needs no declaration, it needs substituting.
                        self.transparent_aliases.insert(
                            alias.name.clone(),
                            (alias.generics.clone(), alias.value.clone()),
                        );
                    }
                }
                _ => {}
            }
        }
        // A second pass keeps the interface bodies available for flattening.
        for declaration in &file.declarations {
            if let Declaration::Interface(interface) = declaration {
                self.interfaces.insert(
                    interface.name.clone(),
                    InterfaceDeclaration {
                        name: interface.name.clone(),
                        generics: interface.generics.clone(),
                        extends: interface.extends.clone(),
                        members: interface.members.clone(),
                    },
                );
            }
        }
    }

    fn emit(&mut self, file: &dts::DeclarationFile) {
        // §3.10: vilan's `fun` grammar allows exactly one signature per name,
        // so an overload SET collapses to its first signature; the rest are
        // quoted verbatim in a TODO so nothing is dropped. Overloads must be
        // grouped before emission or the file would declare one name twice.
        let mut overloads: HashMap<&str, Vec<&Signature>> = HashMap::new();
        for declaration in &file.declarations {
            if let Declaration::Function(signature) = declaration {
                overloads
                    .entry(signature.name.as_str())
                    .or_default()
                    .push(signature);
            }
        }
        let mut emitted_functions: HashSet<&str> = HashSet::new();

        let mut body = String::new();
        for declaration in &file.declarations {
            self.coverage.declarations += 1;
            let chunk = match declaration {
                Declaration::Function(signature) => {
                    if !emitted_functions.insert(signature.name.as_str()) {
                        // A later overload of a name already emitted: the group
                        // it belongs to IS bound, through the first signature
                        // plus the TODO quoting this one.
                        self.coverage.declarations_bound += 1;
                        continue;
                    }
                    let group = &overloads[signature.name.as_str()];
                    let extra: Vec<Signature> = group[1..].iter().map(|s| (*s).clone()).collect();
                    self.emit_top_level_function(signature, &extra)
                }
                other => self.emit_declaration(other),
            };
            let pending = std::mem::take(&mut self.pending_types);
            for synthesized in pending {
                body.push_str(&synthesized);
            }
            body.push_str(&chunk);
        }
        self.emit_header();
        self.out.push_str(&body);
    }

    fn emit_header(&mut self) {
        let _ = writeln!(
            self.out,
            "// Generated by `vilan bindgen` from `{}` for the `{}` platform.",
            self.options.source_name, self.options.platform
        );
        self.out.push_str(
            "//\n\
             // This file is ordinary vilan source, not a build artifact: review it, edit\n\
             // it, and commit it. Nothing regenerates it behind your back.\n\
             //\n\
             // `// TODO(bindgen)` marks a TypeScript construct the generator could not\n\
             // express in vilan. Nothing was dropped silently — every one is named.\n\
             //\n\
             // ONE rule is stated here rather than repeated everywhere: vilan has no\n\
             // `null`, and `Option<T>` cannot cross a host boundary — it is a tagged\n\
             // array (`Some(v)` is `[0, v]`, `None` is `[1]`) that a third-party host\n\
             // neither produces nor reads. So a TypeScript type admitting `null` or\n\
             // `undefined` binds as the BARE type, and the absence is yours to guard.\n\
             // Each such binding carries a `///` note saying so.\n\n",
        );
    }

    fn finish(self) -> String {
        self.out
    }

    // --- Declarations -----------------------------------------------------

    fn emit_declaration(&mut self, declaration: &Declaration) -> String {
        match declaration {
            Declaration::Interface(interface) => {
                self.coverage.declarations_bound += 1;
                self.emit_interface(interface)
            }
            Declaration::Class(class) => self.emit_class(class),
            Declaration::Function(signature) => self.emit_top_level_function(signature, &[]),
            Declaration::TypeAlias(alias) => self.emit_type_alias(alias),
            Declaration::Variable(variable) => {
                // §3.7's extern forms bind a CALL (`[extern("f")]` is `f(args)`)
                // or a property of a receiver (`[extern(get, …)]`). There is no
                // form for reading a bare global as a value, so `declare const
                // document: Document` has no binding — though its members reach
                // vilan fine as dotted globals (`[extern("document.title")]`).
                self.coverage.note_skipped("global variable");
                self.coverage.note_todo("global variable");
                format!(
                    "{}//   {}\n\n",
                    todo_comment(&format!(
                        "`{}` is a global VALUE; vilan's `[extern(…)]` forms bind a call or a \
                         receiver's property, never a bare global read. Bind its members as \
                         dotted globals instead: `[extern(\"{}.member\")] external fun member(…)`",
                        variable.name, variable.name
                    )),
                    variable.raw
                )
            }
            Declaration::Unsupported(unsupported) => {
                self.coverage.note_skipped(unsupported.construct);
                self.coverage.note_todo(unsupported.construct);
                let name = if unsupported.name.is_empty() {
                    String::new()
                } else {
                    format!(" `{}`", unsupported.name)
                };
                format!(
                    "{}//   {}\n\n",
                    todo_comment(&format!(
                        "{}{} is out of bindgen v1's scope (proposal/bindgen.md §5)",
                        unsupported.construct, name
                    )),
                    unsupported.raw
                )
            }
        }
    }

    fn emit_type_alias(&mut self, alias: &dts::TypeAliasDeclaration) -> String {
        // A string-literal union is the one alias shape with a real vilan
        // target: a plain `enum`, plus a match-wrapper at every call site that
        // takes it (§3.3).
        if let Some(string_enum) = self.string_enums.get(&alias.name) {
            self.coverage.declarations_bound += 1;
            let mut out = String::new();
            let _ = writeln!(
                out,
                "/// `{}` — the closed string set `{}`.\n\
                 /// The host boundary still speaks the raw JS string, so every binding that\n\
                 /// takes one pairs a private raw extern with a match-wrapper.",
                alias.name,
                string_enum
                    .variants
                    .iter()
                    .map(|(_, raw)| format!("\"{raw}\""))
                    .collect::<Vec<_>>()
                    .join(" | ")
            );
            let _ = writeln!(out, "enum {} {{", escape_reserved(&alias.name));
            for (variant, _) in &string_enum.variants {
                let _ = writeln!(out, "\t{variant},");
            }
            out.push_str("}\n\n");
            return out;
        }

        // vilan has no type-alias declaration at all (§5), so an alias resolves
        // to whatever nominal declaration its right-hand shape maps to.
        match &alias.value {
            TsType::Object(members) => {
                let interface = InterfaceDeclaration {
                    name: alias.name.clone(),
                    generics: alias.generics.clone(),
                    extends: Vec::new(),
                    members: members.clone(),
                };
                self.emit_interface(&interface)
            }
            other if self.transparent_aliases.contains_key(&alias.name) => {
                // A transparent alias produces no declaration BY DESIGN — vilan
                // has none — but it is fully handled: every reference to it is
                // substituted. Recorded so a reader of the generated file can
                // see where a familiar TypeScript name went.
                self.coverage.declarations_bound += 1;
                format!(
                    "// `{}` is a transparent alias for `{}`; vilan has no type alias, so every\n\
                     // use of it below is written out directly.\n\n",
                    alias.name,
                    describe_type(other)
                )
            }
            other => {
                self.coverage.note_skipped("type alias");
                self.coverage.note_todo("unmappable type alias");
                format!(
                    "{}\n",
                    todo_comment(&format!(
                        "`type {} = …` ({}) has no vilan declaration form — vilan has no type \
                         alias, and this shape is not an object, a class, or a closed string \
                         set. Bind the underlying type where it is used instead",
                        alias.name,
                        other.construct()
                    ))
                )
            }
        }
    }

    fn emit_interface(&mut self, interface: &InterfaceDeclaration) -> String {
        self.scope = interface
            .generics
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect();

        let mut out = String::new();
        let mut todos = Vec::new();
        let members = self.flatten_members(
            &interface.name,
            &interface.extends,
            &interface.members,
            &mut todos,
        );

        let name = escape_reserved(&interface.name);
        let _ = writeln!(
            out,
            "/// `{}` — a host object, bound opaquely: its fields are reached through\n\
             /// `[extern(get/set, …)]` accessors, never as vilan struct fields (a vilan\n\
             /// `struct` is a positional array at runtime and would read the wrong slots).",
            interface.name
        );
        for todo in &todos {
            out.push_str(&todo_comment(todo));
        }
        let _ = writeln!(
            out,
            "external struct {}{};\n",
            name,
            self.generic_list(&interface.generics)
        );

        let mut block = String::new();
        // The `RequestInit` precedent (§3.2, `std/src/fetch.vl:109-110`): an
        // options bag is an opaque host object the caller fills in with setters,
        // which is also how the omitted-key-versus-explicit-null question stays
        // answerable — you only ever set the keys you mean.
        block.push_str(
            "\t/// A fresh empty host object (`{}`) to fill in with the setters below —\n\
             \t/// the `RequestInit` precedent (`std/src/fetch.vl`).\n\
             \t[extern(\"Object\")]\n\
             \texternal fun new(): ",
        );
        let _ = writeln!(
            block,
            "{}{};\n",
            name,
            self.generic_arguments(&interface.generics)
        );

        let mut names = HashSet::new();
        names.insert("new".to_string());
        for (member, overloads) in group_overloads(&members) {
            self.coverage.members += 1 + overloads.len();
            let rendered =
                self.emit_member(&interface.name, &member, false, &overloads, &mut names);
            if rendered.bound {
                self.coverage.members_bound += 1 + overloads.len();
            }
            block.push_str(&rendered.text);
        }

        let _ = writeln!(
            out,
            "impl {}{} {{\n{}}}\n",
            name,
            self.generic_binders(&interface.generics),
            block
        );
        self.scope.clear();
        out
    }

    fn emit_class(&mut self, class: &ClassDeclaration) -> String {
        self.coverage.declarations_bound += 1;
        self.scope = class
            .generics
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect();

        let mut out = String::new();
        let mut todos = Vec::new();
        let members = self.flatten_members(&class.name, &class.extends, &class.members, &mut todos);
        for implemented in &class.implements {
            if let TsType::Reference { name, .. } = implemented {
                self.coverage.note_todo("class implements clause");
                todos.push(format!(
                    "`implements {name}` — vilan has no structural subtyping, so this class is \
                     not assignable where `{name}` is expected; its members are not copied in"
                ));
            }
        }

        let name = escape_reserved(&class.name);
        let _ = writeln!(
            out,
            "/// `{}` — a host class, bound as an opaque handle.",
            class.name
        );
        for todo in &todos {
            out.push_str(&todo_comment(todo));
        }
        let _ = writeln!(
            out,
            "external struct {}{};\n",
            name,
            self.generic_list(&class.generics)
        );

        let mut block = String::new();
        let mut names = HashSet::new();
        for (member, overloads) in group_overloads(&members) {
            self.coverage.members += 1 + overloads.len();
            let rendered = self.emit_member(&class.name, &member, true, &overloads, &mut names);
            if rendered.bound {
                self.coverage.members_bound += 1 + overloads.len();
            }
            block.push_str(&rendered.text);
        }

        let _ = writeln!(
            out,
            "impl {}{} {{\n{}}}\n",
            name,
            self.generic_binders(&class.generics),
            block
        );
        self.scope.clear();
        out
    }

    fn emit_top_level_function(
        &mut self,
        signature: &Signature,
        overloads: &[Signature],
    ) -> String {
        self.coverage.declarations_bound += 1;
        self.scope = signature
            .generics
            .iter()
            .map(|parameter| parameter.name.clone())
            .collect();
        let binding = format!("[extern(\"{}\")]", signature.name);
        let mut extra = Vec::new();
        let mut text = self.emit_function(
            "",
            &to_snake_case(&signature.name),
            signature,
            &binding,
            None,
            overloads,
            &mut extra,
        );
        for line in extra {
            text.push_str(&line);
        }
        self.scope.clear();
        format!("{text}\n")
    }

    // --- Inheritance ------------------------------------------------------

    /// Flattens `extends` bases into `own`, derived-first (a derived member
    /// shadows a base member of the same name, as TS's own override rule says).
    ///
    /// vilan has no struct inheritance and no structural subtyping, so a base's
    /// members are simply not on the derived type unless they are copied there.
    /// Copying is what a human writing this binding by hand does, and it is the
    /// only mapping that leaves the derived type usable. What it cannot
    /// recover is ASSIGNABILITY: `Element` still is not accepted where `Node`
    /// is expected. That limit is noted on the emitted type, not papered over.
    fn flatten_members(
        &mut self,
        owner: &str,
        extends: &[TsType],
        own: &[Member],
        todos: &mut Vec<String>,
    ) -> Vec<Member> {
        let mut seen: HashSet<String> = own.iter().filter_map(member_key).collect();
        let mut flattened = own.to_vec();
        let mut visiting = HashSet::new();
        visiting.insert(owner.to_string());
        for base in extends {
            self.flatten_base(base, &mut flattened, &mut seen, &mut visiting, todos);
        }
        flattened
    }

    fn flatten_base(
        &mut self,
        base: &TsType,
        into: &mut Vec<Member>,
        seen: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        todos: &mut Vec<String>,
    ) {
        let TsType::Reference { name, arguments } = base else {
            self.coverage.note_todo("non-nominal base type");
            todos.push(format!(
                "`extends {}` is not a named type — its members are not copied in",
                base.construct()
            ));
            return;
        };
        if !visiting.insert(name.clone()) {
            return;
        }
        let Some(interface) = self.interfaces.get(name) else {
            self.coverage.note_todo("unresolved base type");
            todos.push(format!(
                "`extends {name}` — `{name}` is not declared in this file, so its members are \
                 not copied in (bindgen v1 does not resolve across files, §2)"
            ));
            return;
        };
        let substitution: HashMap<String, TsType> = interface
            .generics
            .iter()
            .zip(arguments.iter())
            .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
            .collect();
        let members = interface.members.clone();
        let bases = interface.extends.clone();
        for member in members {
            let member = substitute_member(&member, &substitution);
            if let Some(key) = member_key(&member) {
                if !seen.insert(key) {
                    continue;
                }
            }
            into.push(member);
        }
        for base in &bases {
            let base = substitute_type(base, &substitution);
            self.flatten_base(&base, into, seen, visiting, todos);
        }
    }

    // --- Members ----------------------------------------------------------

    fn emit_member(
        &mut self,
        owner: &str,
        member: &Member,
        is_class: bool,
        overloads: &[Signature],
        names: &mut HashSet<String>,
    ) -> Rendered {
        match member {
            Member::Property(property) => self.emit_property(owner, property, names),
            Member::Method(method) => self.emit_method(owner, method, overloads, names),
            Member::Construct(signature) => {
                if !is_class {
                    // `new (…): T` on an interface describes a constructor
                    // living on some OTHER value (`declare const Foo:
                    // FooConstructor`), which v1 has no way to name.
                    self.coverage.note_todo("interface construct signature");
                    return Rendered::todo(format!(
                        "{}\n",
                        indent(&todo_comment(&format!(
                            "construct signature `{}` — this describes a constructor reached \
                             through a global value, which v1 cannot bind (see the \
                             global-variable note)",
                            signature.raw
                        )))
                    ));
                }
                let binding = format!("[extern(new, \"{owner}\")]");
                let name = unique_name("new", names);
                let mut extra = Vec::new();
                // A TS `constructor(…)` declares no return type; `new Foo(…)`
                // obviously yields a `Foo<…>`, and the binding has to say so.
                // The class's own type parameters are in scope here.
                let signature = Signature {
                    return_type: Some(TsType::Reference {
                        name: owner.to_string(),
                        arguments: self
                            .scope
                            .clone()
                            .into_iter()
                            .map(|name| TsType::Reference {
                                name,
                                arguments: Vec::new(),
                            })
                            .collect(),
                    }),
                    ..signature.clone()
                };
                let mut text = self.emit_function(
                    owner, &name, &signature, &binding, None, overloads, &mut extra,
                );
                for line in extra {
                    text.push_str(&line);
                }
                Rendered::bound(indent(&text))
            }
            Member::Call(signature) => {
                self.coverage.note_todo("call signature");
                Rendered::todo(format!(
                    "{}\n",
                    indent(&todo_comment(&format!(
                        "call signature `{}` — a callable object has no vilan form (a value is \
                         not a function)",
                        signature.raw
                    )))
                ))
            }
            Member::Index(index) => {
                // VERIFIED against the running compiler, not assumed: `List<T>`
                // is a native JS array, and an array-LIKE object is not one.
                // `for`-in over `{0: "a", length: 1}` throws `TypeError: … is
                // not iterable`, and `map`/`filter`/`fold`/`reverse` all ride
                // `for`-in. `Map<str, T>` is worse still: it is a plain vilan
                // struct wrapping a `NativeMap` keyed by `key.hash()`, so a
                // host `{a: 1}` read through it crashes on `.has`.
                let (kind, note) = match index.key {
                    IndexKey::Number => (
                        "numeric index signature",
                        "this is an array-LIKE shape, NOT a JS array: `List<T>` needs \
                         `Symbol.iterator` (`for`-in, `map`, `filter`, `fold` all throw without \
                         it), and a real array with holes hands `undefined` to a `T`-typed slot. \
                         Convert at the boundary (`Array.from`) and bind the result as `List<T>`",
                    ),
                    IndexKey::String => (
                        "string index signature",
                        "vilan has no open keyed-object type at a host boundary: `Map<str, T>` \
                         is a vilan struct over a `NativeMap` keyed by `key.hash()`, not a plain \
                         host object. Bind the keys you need as `[extern(get, \"key\")]` \
                         accessors",
                    ),
                    IndexKey::Other => (
                        "index signature",
                        "only string and number index keys are recognized, and neither has a \
                         vilan form at a host boundary",
                    ),
                };
                self.coverage.note_todo(kind);
                Rendered::todo(format!(
                    "{}\n",
                    indent(&todo_comment(&format!("{kind} `{}` — {note}", index.raw)))
                ))
            }
            Member::Unsupported { construct, raw } => {
                self.coverage.note_todo(construct);
                Rendered::todo(format!(
                    "{}\n",
                    indent(&todo_comment(&format!(
                        "{construct} `{raw}` is out of bindgen v1's scope"
                    )))
                ))
            }
        }
    }

    fn emit_property(
        &mut self,
        owner: &str,
        property: &PropertyMember,
        names: &mut HashSet<String>,
    ) -> Rendered {
        let Some(declared) = &property.declared_type else {
            self.coverage.note_todo("untyped property");
            return Rendered::todo(format!(
                "{}\n",
                indent(&todo_comment(&format!(
                    "property `{}` has no declared type",
                    property.name
                )))
            ));
        };
        let context = format!("{owner}{}", to_pascal_case(&property.name));
        let mut mapped = self.map_type(declared, Position::Return, &context);
        if let Some(enum_name) = mapped.string_enum.take() {
            // A property is read AND written through separate externs; wrapping
            // both would double the generated surface for a case §3.3 only
            // specifies for parameters. The raw `str` is bound, named.
            self.coverage.note_todo("string-literal union property");
            mapped.todos.push(format!(
                "property `{}` is the closed string set `{enum_name}` — the raw `str` is bound; \
                 convert to and from `{enum_name}` at the call site",
                property.name
            ));
        }
        if property.optional {
            mapped = self.note_absence(mapped, &format!("Property `{}`", property.name));
        }

        let mut out = String::new();
        for note in &mapped.notes {
            out.push_str(&indent(&note_comment(note)));
        }
        for todo in &mapped.todos {
            out.push_str(&indent(&todo_comment(todo)));
        }
        if mapped.is_async {
            self.coverage.note_todo("Promise-typed property");
            out.push_str(&indent(&todo_comment(&format!(
                "property `{}` is a `Promise`; a property read cannot be awaited through an \
                 extern. Bind it as a method on the host side, or await it at the call site \
                 by hand",
                property.name
            ))));
        }
        let base = to_snake_case(&property.name);
        let mut bound = false;
        if property.readable {
            let name = unique_name(&escape_reserved(&base), names);
            if property.is_static {
                let _ = writeln!(out, "\t[extern(\"{owner}.{}\")]", property.name);
                let _ = writeln!(out, "\t[platform(\"{}\")]", self.options.platform);
                let _ = writeln!(out, "\texternal fun {name}(): {};\n", mapped.text);
            } else {
                let _ = writeln!(out, "\t[extern(get, \"{}\")]", property.name);
                let _ = writeln!(out, "\t[platform(\"{}\")]", self.options.platform);
                let _ = writeln!(out, "\texternal fun {name}(self): {};\n", mapped.text);
            }
            bound = true;
        }
        if property.writable && !property.is_static {
            let name = unique_name(&escape_reserved(&format!("set_{base}")), names);
            let _ = writeln!(out, "\t[extern(set, \"{}\")]", property.name);
            let _ = writeln!(out, "\t[platform(\"{}\")]", self.options.platform);
            let _ = writeln!(
                out,
                "\texternal fun {name}(self, value: {}): void;\n",
                mapped.text
            );
            bound = true;
        }
        if bound {
            Rendered::bound(out)
        } else {
            Rendered::todo(out)
        }
    }

    fn emit_method(
        &mut self,
        owner: &str,
        method: &MethodMember,
        overloads: &[Signature],
        names: &mut HashSet<String>,
    ) -> Rendered {
        let signature = &method.signature;
        let binding = if method.is_static {
            format!("[extern(\"{owner}.{}\")]", signature.name)
        } else {
            format!("[extern(method, \"{}\")]", signature.name)
        };
        let receiver = (!method.is_static).then_some("self");
        let name = unique_name(&escape_reserved(&to_snake_case(&signature.name)), names);
        let mut extra = Vec::new();
        let mut text = self.emit_function(
            owner, &name, signature, &binding, receiver, overloads, &mut extra,
        );
        for line in extra {
            text.push_str(&line);
        }
        Rendered::bound(indent(&text))
    }

    /// Emits one binding: the attributes, the `external fun`, and — when a
    /// parameter is a closed string set — the private raw extern plus the
    /// match-wrapper that speaks the enum (§3.3).
    #[allow(clippy::too_many_arguments)]
    fn emit_function(
        &mut self,
        owner: &str,
        name: &str,
        signature: &Signature,
        binding: &str,
        receiver: Option<&str>,
        overloads: &[Signature],
        extra: &mut Vec<String>,
    ) -> String {
        let outer_scope = self.scope.clone();
        for parameter in &signature.generics {
            self.scope.push(parameter.name.clone());
        }

        let mut todos = Vec::new();
        let mut notes = Vec::new();
        let context = format!("{owner}{}", to_pascal_case(&signature.name));
        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            let rendered = self.map_parameter(parameter, &context, &mut todos, &mut notes);
            parameters.push(rendered);
        }

        let (return_text, is_async) = match &signature.return_type {
            Some(declared) => {
                let mapped = self.map_type(declared, Position::Return, &context);
                todos.extend(mapped.todos.iter().cloned());
                notes.extend(mapped.notes.iter().cloned());
                if let Some(enum_name) = &mapped.string_enum {
                    self.coverage
                        .note_todo("string-literal union in return position");
                    todos.push(format!(
                        "returns the closed string set `{enum_name}` — the raw `str` is bound \
                         because the host may return a value outside the set; match it to \
                         `{enum_name}` by hand"
                    ));
                }
                (mapped.text, mapped.is_async)
            }
            None => ("void".to_string(), false),
        };

        let mut out = String::new();
        for note in &notes {
            out.push_str(&note_comment(note));
        }
        for todo in &todos {
            out.push_str(&todo_comment(todo));
        }
        // §3.10, first-signature-wins: vilan's `fun` grammar has no overload
        // form, so the remaining signatures are quoted verbatim. The raw TS is
        // enough for a human to hand-split them into distinct vilan names —
        // which std itself does (`append`/`append_text` are two bindings of one
        // `appendChild`).
        if !overloads.is_empty() {
            self.coverage.note_todo("function overload");
            out.push_str(&todo_comment(&format!(
                "{} additional overload(s) of `{}` not represented — vilan has one signature \
                 per name. Consider a differently-named binding per overload:",
                overloads.len(),
                signature.name
            )));
            for overload in overloads {
                let _ = writeln!(out, "//   {}", overload.raw);
            }
        }

        // A generic function declares its own parameters (`fun map<U>(…)`, as
        // `std/src/list.vl` writes them); without them the body's `T` resolves
        // to nothing and the generated file does not compile.
        let own_generics = self.generic_list(&signature.generics);

        // §3.2, corrected: an optional TS parameter cannot become `Option<T>`
        // (see `note_absence`), and it cannot simply be made required either —
        // that would force a caller to invent a value the host is meant never
        // to see. TS optionals are TRAILING, so the honest mapping is exact:
        // one binding per call ARITY. `getContext(id)` and
        // `getContext(id, options)` are two real host calls, so they become two
        // real bindings of the same symbol — which is what std already does by
        // hand (`append` and `append_text` both bind `appendChild`).
        let required = parameters
            .iter()
            .position(|parameter| parameter.optional)
            .unwrap_or(parameters.len());
        let optional_count = parameters.len() - required;

        if optional_count > 0 {
            // The bare name is the SIMPLE call — required arguments only.
            out.push_str(&self.emit_one_binding(
                name,
                binding,
                receiver,
                &parameters[..required],
                &return_text,
                is_async,
                &own_generics,
                extra,
            ));
            let suffix: Vec<String> = parameters[required..]
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect();
            let full_name = format!("{name}_with_{}", suffix.join("_and_"));
            if optional_count > 1 {
                self.coverage.note_todo("intermediate optional arity");
                out.push_str(&todo_comment(&format!(
                    "`{}` has {optional_count} optional parameters, so it has {} call arities; \
                     the shortest and the longest are bound below. Add a binding of the same \
                     symbol for an arity in between if you need one",
                    signature.name,
                    optional_count + 1
                )));
            }
            out.push_str(&self.emit_one_binding(
                &full_name,
                binding,
                receiver,
                &parameters,
                &return_text,
                is_async,
                &own_generics,
                extra,
            ));
        } else {
            out.push_str(&self.emit_one_binding(
                name,
                binding,
                receiver,
                &parameters,
                &return_text,
                is_async,
                &own_generics,
                extra,
            ));
        }

        self.scope = outer_scope;
        out
    }

    /// One `external fun`, plus the match-wrapper when a parameter is a closed
    /// string set (§3.3). Split out because an optional parameter produces more
    /// than one binding of the same host symbol.
    #[allow(clippy::too_many_arguments)]
    fn emit_one_binding(
        &mut self,
        name: &str,
        binding: &str,
        receiver: Option<&str>,
        parameters: &[RenderedParameter],
        return_text: &str,
        is_async: bool,
        own_generics: &str,
        extra: &mut Vec<String>,
    ) -> String {
        let has_wrapper = parameters
            .iter()
            .any(|parameter| parameter.string_enum.is_some());
        let raw_name = if has_wrapper {
            format!("{name}_raw")
        } else {
            name.to_string()
        };

        let mut out = String::new();
        let _ = writeln!(out, "{binding}");
        if has_wrapper {
            out.push_str("[doc(hidden)]\n");
        }
        let _ = writeln!(out, "[platform(\"{}\")]", self.options.platform);
        let signature_parameters = render_parameters(receiver, parameters, ParameterForm::Raw);
        let _ = writeln!(
            out,
            "{}external fun {raw_name}{own_generics}({signature_parameters}): {return_text};\n",
            if is_async { "async " } else { "" }
        );

        if has_wrapper {
            let wrapper_parameters = render_parameters(receiver, parameters, ParameterForm::Typed);
            let mut wrapper = String::new();
            let _ = writeln!(
                wrapper,
                "/// `{name}` — `{raw_name}` with its closed string sets spoken as enums."
            );
            let _ = writeln!(
                wrapper,
                "{}fun {name}{own_generics}({wrapper_parameters}): {return_text} {{",
                if is_async { "async " } else { "" }
            );
            let call_receiver = if receiver.is_some() { "self." } else { "" };
            let arguments: Vec<String> = parameters
                .iter()
                .map(|parameter| match &parameter.string_enum {
                    Some(enum_name) => {
                        let variants = &self.string_enums[enum_name].variants;
                        let arms: Vec<String> = variants
                            .iter()
                            .map(|(variant, raw)| {
                                format!(
                                    "\t\t{}::{variant} => \"{raw}\",",
                                    escape_reserved(enum_name)
                                )
                            })
                            .collect();
                        format!("match {} {{\n{}\n\t}}", parameter.name, arms.join("\n"))
                    }
                    None => parameter.name.clone(),
                })
                .collect();
            let _ = writeln!(
                wrapper,
                "\t{call_receiver}{raw_name}({})",
                arguments.join(", ")
            );
            wrapper.push_str("}\n\n");
            extra.push(wrapper);
        }
        out
    }

    fn map_parameter(
        &mut self,
        parameter: &Parameter,
        context: &str,
        todos: &mut Vec<String>,
        notes: &mut Vec<String>,
    ) -> RenderedParameter {
        let name = escape_reserved(&to_snake_case(&parameter.name));
        let Some(declared) = &parameter.declared_type else {
            self.coverage.note_todo("untyped parameter");
            todos.push(format!(
                "parameter `{}` has no declared type",
                parameter.name
            ));
            return RenderedParameter {
                name,
                text: "any".to_string(),
                optional: parameter.optional,
                string_enum: None,
            };
        };
        let context = format!("{context}{}", to_pascal_case(&parameter.name));
        let mapped = self.map_type(declared, Position::Parameter, &context);
        if parameter.rest {
            // A rest parameter's declared type is ALREADY the array
            // (`...parts: string[]`), so it maps to `List<str>` on its own —
            // what has no vilan form is the variadic call shape, not the type.
            self.coverage.note_todo("rest parameter");
            todos.push(format!(
                "rest parameter `...{}` — vilan has no variadic parameters, so the binding takes \
                 the whole `List` as one argument and the host receives it as a single array \
                 rather than spread across its own parameters",
                parameter.name
            ));
        }
        todos.extend(mapped.todos.iter().cloned());
        notes.extend(mapped.notes.iter().cloned());
        RenderedParameter {
            name,
            text: mapped.text,
            optional: parameter.optional,
            string_enum: mapped.string_enum,
        }
    }

    /// Records that a value may be ABSENT at the host boundary.
    ///
    /// `proposal/bindgen.md` §3.2 maps every absence onto `Option<T>`. VERIFIED
    /// broken, in both directions: `Option` is a vilan TAGGED ARRAY (`Some(v)`
    /// is `[0, v]`, `None` is `[1]`) that a third-party host neither produces
    /// nor understands.
    ///
    /// - Reading: a host that returns `"hello"` is matched as `value[0] === 0`,
    ///   which is `"h" === 0` — so a PRESENT value reads as `None`.
    /// - Writing: `None` arrives at the host as the array `[1]`, which for an
    ///   optional `boolean` argument is TRUTHY.
    ///
    /// std does use `Option` across `external` boundaries, but only ones it
    /// owns — compiler intrinsics and its own `__`-prefixed runtime helpers,
    /// which know the representation. A library's `.d.ts` does not.
    ///
    /// So the bare type is bound and the absence is named. The caller carries
    /// what the type system cannot.
    fn note_absence(&mut self, mapped: Mapped, what: &str) -> Mapped {
        if mapped.text == "void" {
            return mapped;
        }
        self.coverage.note_absence();
        let mut notes = mapped.notes;
        notes.push(format!(
            "{what} may be `null`/`undefined` at the host — the bare type is bound (see the \
             header); guard at the call site."
        ));
        Mapped {
            text: mapped.text,
            todos: mapped.todos,
            notes,
            is_async: mapped.is_async,
            string_enum: mapped.string_enum,
        }
    }

    // --- Types ------------------------------------------------------------

    fn map_type(&mut self, declared: &TsType, position: Position, context: &str) -> Mapped {
        match declared {
            TsType::Reference { name, arguments } => {
                self.map_reference(name, arguments, position, context)
            }
            TsType::Array(element) => {
                let element = self.map_type(element, Position::Nested, context);
                Mapped {
                    text: format!("List<{}>", element.text),
                    todos: element.todos,
                    is_async: false,
                    notes: Vec::new(),
                    string_enum: None,
                }
            }
            TsType::Tuple(elements) => {
                // A vilan tuple IS a JS array at runtime, so this row is exact.
                if elements.len() < 2 {
                    self.coverage.note_todo("tuple type");
                    return Mapped::todo(
                        "any",
                        format!(
                            "a {}-element tuple has no vilan form (vilan tuples start at two \
                             elements) — widened to `any`",
                            elements.len()
                        ),
                    );
                }
                let mut todos = Vec::new();
                let mut notes = Vec::new();
                let mut parts = Vec::new();
                for element in elements {
                    let mapped = self.map_type(element, Position::Nested, context);
                    todos.extend(mapped.todos);
                    notes.extend(mapped.notes);
                    parts.push(mapped.text);
                }
                Mapped {
                    text: format!("({})", parts.join(", ")),
                    todos,
                    notes,
                    is_async: false,
                    string_enum: None,
                }
            }
            TsType::Union(members) => self.map_union(members, position, context),
            TsType::Intersection(members) => {
                self.coverage.note_todo("intersection type");
                let rendered: Vec<String> =
                    members.iter().map(|member| describe_type(member)).collect();
                Mapped::todo(
                    "any",
                    format!(
                        "TS intersection `{}` has no vilan equivalent (no structural types) — \
                         widened to `any`",
                        rendered.join(" & ")
                    ),
                )
            }
            TsType::Function(signature) => self.map_closure(signature, context),
            TsType::Constructor(_) => {
                self.coverage.note_todo("constructor type");
                Mapped::todo(
                    "any",
                    "a constructor type (`new (…) => T`) is not a vilan value — widened to `any`",
                )
            }
            TsType::Object(members) => self.synthesize_object(members, context),
            TsType::StringLiteral(_) => Mapped::plain("str"),
            TsType::NumberLiteral(_) => Mapped::plain("f64"),
            TsType::BooleanLiteral(_) => Mapped::plain("bool"),
            TsType::Unsupported { construct, raw } => {
                self.coverage.note_todo(construct);
                Mapped::todo(
                    "any",
                    format!("{construct} `{raw}` is out of bindgen v1's scope — widened to `any`"),
                )
            }
        }
    }

    fn map_reference(
        &mut self,
        name: &str,
        arguments: &[TsType],
        position: Position,
        context: &str,
    ) -> Mapped {
        // §3.1's primitives.
        match name {
            "string" | "String" => return Mapped::plain("str"),
            "boolean" | "Boolean" => return Mapped::plain("bool"),
            // The single most consequential default in the table (§3.1): a
            // `.d.ts` cannot say whether a `number` means an integer, so every
            // one becomes `f64` — always lossless, never wrong, narrowing left
            // to a human edit.
            "number" | "Number" => return Mapped::plain("f64"),
            "bigint" | "BigInt" => return Mapped::plain("BigInt"),
            "void" => {
                return Mapped::plain(if position == Position::Parameter {
                    "any"
                } else {
                    "void"
                });
            }
            "any" | "unknown" | "object" | "Object" => return Mapped::plain("any"),
            "undefined" => {
                return if position == Position::Return {
                    Mapped::plain("void")
                } else {
                    Mapped::plain("any")
                };
            }
            "null" => return Mapped::plain("any"),
            "never" => {
                self.coverage.note_todo("never type");
                return Mapped::todo(
                    "void",
                    "TS `never` — vilan's `Never` is internal-only and cannot be written; this \
                     function may not return normally",
                );
            }
            "symbol" => {
                self.coverage.note_todo("symbol type");
                return Mapped::todo("any", "TS `symbol` has no vilan equivalent");
            }
            "Array" | "ReadonlyArray" => {
                let element = match arguments.first() {
                    Some(argument) => self.map_type(argument, Position::Nested, context),
                    None => Mapped::plain("any"),
                };
                return Mapped {
                    text: format!("List<{}>", element.text),
                    todos: element.todos,
                    is_async: false,
                    notes: Vec::new(),
                    string_enum: None,
                };
            }
            "Promise" | "PromiseLike" => {
                let inner = match arguments.first() {
                    Some(argument) => self.map_type(argument, position, context),
                    None => Mapped::plain("void"),
                };
                if position == Position::Return {
                    return Mapped {
                        is_async: true,
                        ..inner
                    };
                }
                self.coverage.note_todo("Promise outside return position");
                return Mapped::todo(
                    "any",
                    "a `Promise` outside return position is not an awaitable vilan value",
                );
            }
            "Function" => {
                self.coverage.note_todo("bare Function type");
                return Mapped::todo(
                    "any",
                    "the bare `Function` type says nothing about its signature",
                );
            }
            "Record" => {
                self.coverage.note_todo("Record type");
                return Mapped::todo(
                    "any",
                    "`Record<K, V>` is an open keyed host object; vilan's `Map` is a struct over \
                     a `NativeMap` keyed by `key.hash()`, not a host object. Bind the keys you \
                     need as `[extern(get, \"key\")]` accessors",
                );
            }
            // §3.5/§5: mapped utility types are out of v1 scope entirely.
            "Partial"
            | "Required"
            | "Readonly"
            | "Pick"
            | "Omit"
            | "Exclude"
            | "Extract"
            | "NonNullable"
            | "ReturnType"
            | "Parameters"
            | "InstanceType"
            | "ThisType"
            | "Awaited"
            | "ConstructorParameters"
            | "Uppercase"
            | "Lowercase"
            | "Capitalize"
            | "Uncapitalize" => {
                self.coverage.note_todo("mapped utility type");
                return Mapped::todo(
                    "any",
                    format!("`{name}<…>` is a mapped utility type, out of v1 scope (§3.11)"),
                );
            }
            _ => {}
        }

        if self.scope.iter().any(|parameter| parameter == name) {
            return Mapped::plain(escape_reserved(name));
        }
        if let Some(string_enum) = self.string_enums.get(name) {
            let _ = string_enum;
            // The host still speaks the raw string; the caller decides whether a
            // match-wrapper is possible in this position.
            return Mapped {
                text: "str".to_string(),
                todos: Vec::new(),
                is_async: false,
                notes: Vec::new(),
                string_enum: Some(name.to_string()),
            };
        }
        if self.declared.contains(name) {
            let mut todos = Vec::new();
            let mut notes = Vec::new();
            let mut parts = Vec::new();
            for argument in arguments {
                let mapped = self.map_type(argument, Position::Nested, context);
                todos.extend(mapped.todos);
                notes.extend(mapped.notes);
                parts.push(mapped.text);
            }
            let arguments = if parts.is_empty() {
                String::new()
            } else {
                format!("<{}>", parts.join(", "))
            };
            return Mapped {
                text: format!("{}{arguments}", escape_reserved(name)),
                todos,
                notes,
                is_async: false,
                string_enum: None,
            };
        }
        if let Some((parameters, value)) = self.transparent_aliases.get(name).cloned() {
            if self.expanding.iter().any(|entry| entry == name) {
                self.coverage.note_todo("cyclic type alias");
                return Mapped::todo(
                    "any",
                    format!("`{name}` is a cyclic type alias — widened to `any`"),
                );
            }
            let substitution: HashMap<String, TsType> = parameters
                .iter()
                .zip(arguments.iter())
                .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                .collect();
            let expanded = substitute_type(&value, &substitution);
            self.expanding.push(name.to_string());
            let mapped = self.map_type(&expanded, position, context);
            self.expanding.pop();
            return mapped;
        }
        self.coverage.note_todo("unresolved type reference");
        Mapped::todo(
            "any",
            format!(
                "`{name}` is not declared in this file — bindgen v1 does not resolve types \
                 across files (§2), so it is widened to `any`"
            ),
        )
    }

    /// §3.3, the hardest row: three different TS shapes go by "union".
    fn map_union(&mut self, members: &[TsType], position: Position, context: &str) -> Mapped {
        let absent = |member: &TsType| matches!(member, TsType::Reference { name, .. } if name == "null" || name == "undefined");
        let present: Vec<&TsType> = members.iter().filter(|member| !absent(member)).collect();
        let has_absence = present.len() != members.len();

        if present.is_empty() {
            return Mapped::plain("any");
        }
        if has_absence && present.len() == 1 {
            // `T | null`, `T | undefined`, `T | null | undefined` — one absence
            // case, which §3.2 maps to `Option<T>`. See `note_absence`.
            let inner = self.map_type(present[0], position, context);
            return self.note_absence(inner, "This value");
        }
        if has_absence {
            let inner = self.map_union(
                &present
                    .iter()
                    .map(|member| (*member).clone())
                    .collect::<Vec<_>>(),
                position,
                context,
            );
            return self.note_absence(inner, "This value");
        }

        // A closed string set written inline. Widening it to `str` is TOTAL and
        // SAFE — the host boundary speaks exactly that string — so this is an
        // informational note, not a TODO. Only a NAMED alias earns an enum plus
        // its match-wrapper (§3.3).
        if present
            .iter()
            .all(|member| matches!(member, TsType::StringLiteral(_)))
        {
            return Mapped::plain("str");
        }
        // A discriminated union. VERIFIED broken at a host boundary: vilan's
        // `enum` lowers to `[tag, …payload]` while the TS union is a tagged
        // OBJECT, so `match` reads `value[0]`, matches nothing, and crashes.
        let discriminated = present.len() > 1
            && present
                .iter()
                .all(|member| matches!(member, TsType::Object(_)))
            && discriminant_field(&present).is_some();
        if discriminated {
            let tag = discriminant_field(&present).unwrap_or_default();
            self.coverage.note_todo("discriminated union");
            return Mapped::todo(
                "any",
                format!(
                    "discriminated union on `{tag}` — a vilan `enum` lowers to `[tag, …payload]` \
                     while this host value is a tagged OBJECT, so the two representations do not \
                     meet at the boundary. Bind it as an opaque handle and dispatch on \
                     `[extern(get, \"{tag}\")]` by hand"
                ),
            );
        }

        // §3.3's third shape: an open primitive union with no vilan target.
        self.coverage.note_todo("open union");
        let rendered: Vec<String> = present.iter().map(|member| describe_type(member)).collect();
        Mapped::todo(
            "any",
            format!(
                "TS union `{}` widened to `any` — narrow by hand",
                rendered.join(" | ")
            ),
        )
    }

    /// §3.6: `(x: T) => void` is plain `|T| void` (vilan's divergence rule
    /// already lets an async closure fill a void slot), `(x: T) => Promise<U>`
    /// must be `async |T| U` (adaptation never crosses a host boundary), and
    /// `(x: T) => U` is plain `|T| U`.
    fn map_closure(&mut self, signature: &Signature, context: &str) -> Mapped {
        let mut todos = Vec::new();
        let mut notes = Vec::new();
        let mut parameters = Vec::new();
        for parameter in &signature.parameters {
            let mapped = match &parameter.declared_type {
                Some(declared) => self.map_type(declared, Position::Nested, context),
                None => Mapped::plain("any"),
            };
            todos.extend(mapped.todos);
            notes.extend(mapped.notes);
            parameters.push(mapped.text);
        }
        let (return_text, is_async) = match &signature.return_type {
            Some(declared) => {
                let mapped = self.map_type(declared, Position::Return, context);
                todos.extend(mapped.todos);
                notes.extend(mapped.notes);
                (mapped.text, mapped.is_async)
            }
            None => ("void".to_string(), false),
        };
        let prefix = if is_async { "async " } else { "" };
        Mapped {
            text: format!("{prefix}|{}| {return_text}", parameters.join(", ")),
            todos,
            notes,
            is_async: false,
            string_enum: None,
        }
    }

    /// §3.8.2: vilan has no anonymous struct types, so an inline object shape
    /// gets a synthesized name — derived from the enclosing symbol and the
    /// member's own name, never a traversal counter, so it is byte-stable
    /// across runs (§6 gate 3).
    fn synthesize_object(&mut self, members: &[Member], context: &str) -> Mapped {
        let base = if context.is_empty() {
            "Anonymous".to_string()
        } else {
            to_pascal_case(context)
        };
        let mut name = base.clone();
        let mut suffix = 2;
        while self.synthesized.contains(&name) || self.declared.contains(&name) {
            name = format!("{base}{suffix}");
            suffix += 1;
        }
        self.synthesized.insert(name.clone());
        self.declared.insert(name.clone());

        let interface = InterfaceDeclaration {
            name: name.clone(),
            generics: Vec::new(),
            extends: Vec::new(),
            members: members.to_vec(),
        };
        // Emitting the synthesized type recurses through the same path as a
        // named interface; the result is buffered and flushed ahead of the
        // declaration that referenced it.
        let outer_scope = std::mem::take(&mut self.scope);
        let outer_pending = std::mem::take(&mut self.pending_types);
        let text = self.emit_interface(&interface);
        let mut pending = std::mem::replace(&mut self.pending_types, outer_pending);
        pending.push(format!(
            "// Synthesized by bindgen for an anonymous TypeScript object type.\n{text}"
        ));
        self.pending_types.extend(pending);
        self.scope = outer_scope;
        Mapped::plain(name)
    }

    // --- Generics ---------------------------------------------------------

    /// `<T, U>` on a declaration.
    fn generic_list(&self, generics: &[GenericParameter]) -> String {
        if generics.is_empty() {
            return String::new();
        }
        let names: Vec<String> = generics
            .iter()
            .map(|parameter| escape_reserved(&parameter.name))
            .collect();
        format!("<{}>", names.join(", "))
    }

    /// `<type T, type U>` — the impl-subject binder form (`impl List<type T>`).
    fn generic_binders(&self, generics: &[GenericParameter]) -> String {
        if generics.is_empty() {
            return String::new();
        }
        let names: Vec<String> = generics
            .iter()
            .map(|parameter| format!("type {}", escape_reserved(&parameter.name)))
            .collect();
        format!("<{}>", names.join(", "))
    }

    fn generic_arguments(&self, generics: &[GenericParameter]) -> String {
        self.generic_list(generics)
    }
}

// --- Emission helpers --------------------------------------------------------

struct Rendered {
    text: String,
    bound: bool,
}

impl Rendered {
    fn bound(text: String) -> Self {
        Rendered { text, bound: true }
    }

    fn todo(text: String) -> Self {
        Rendered { text, bound: false }
    }
}

struct RenderedParameter {
    name: String,
    text: String,
    optional: bool,
    string_enum: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
enum ParameterForm {
    /// The extern's own signature: a closed string set is a raw `str`.
    Raw,
    /// The wrapper's signature: a closed string set is its enum.
    Typed,
}

fn render_parameters(
    receiver: Option<&str>,
    parameters: &[RenderedParameter],
    form: ParameterForm,
) -> String {
    let mut rendered: Vec<String> = receiver.map(str::to_string).into_iter().collect();
    for parameter in parameters {
        let text = match (&parameter.string_enum, form) {
            (Some(name), ParameterForm::Typed) => escape_reserved(name),
            _ => parameter.text.clone(),
        };
        rendered.push(format!("{}: {text}", parameter.name));
    }
    rendered.join(", ")
}

/// One `/// …` doc comment, hard-wrapped — an informational remark about a
/// mapping that is total but less precise than TypeScript's.
fn note_comment(text: &str) -> String {
    wrap_comment("///", text)
}

/// One `// TODO(bindgen): …` comment, hard-wrapped. A generated file is read by
/// a human, and a 300-column comment is not read at all.
fn todo_comment(text: &str) -> String {
    wrap_comment("// TODO(bindgen):", text)
}

/// Hard-wraps `text` into comment lines opened by `lead`. A generated file is
/// read by a human, and a 300-column comment is not read at all.
fn wrap_comment(lead: &str, text: &str) -> String {
    const WIDTH: usize = 76;
    let continuation = lead.split_whitespace().next().unwrap_or("//").to_string();
    let mut out = String::new();
    let mut line = lead.to_string();
    for word in text.split_whitespace() {
        if line.chars().count() + 1 + word.chars().count() > WIDTH && line != continuation {
            out.push_str(&line);
            out.push('\n');
            line = continuation.clone();
        }
        line.push(' ');
        line.push_str(word);
    }
    out.push_str(&line);
    out.push('\n');
    out
}

/// Indents a rendered binding into an `impl` block.
fn indent(text: &str) -> String {
    text.lines()
        .map(|line| {
            if line.is_empty() {
                String::new()
            } else {
                format!("\t{line}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// Reserves `name` in `taken`, suffixing `_2`, `_3`, … on a collision. Two TS
/// members can land on one vilan name (`align` and `setAlign` both want
/// `set_align`), and vilan allows exactly one function per name. The suffix is
/// assigned in emission order, which is source order, so it is deterministic.
fn unique_name(name: &str, taken: &mut HashSet<String>) -> String {
    if taken.insert(name.to_string()) {
        return name.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{name}_{suffix}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

/// Collapses method overload sets: each member keeps its position, and every
/// later signature of the same method name rides along with the first as an
/// overload to quote (§3.10). Order is source order throughout, so the result
/// is deterministic.
fn group_overloads(members: &[Member]) -> Vec<(Member, Vec<Signature>)> {
    let mut grouped: Vec<(Member, Vec<Signature>)> = Vec::new();
    let mut index_of: HashMap<String, usize> = HashMap::new();
    for member in members {
        match member {
            Member::Method(method) => {
                let key = method.signature.name.clone();
                match index_of.get(&key) {
                    Some(index) => grouped[*index].1.push(method.signature.clone()),
                    None => {
                        index_of.insert(key, grouped.len());
                        grouped.push((member.clone(), Vec::new()));
                    }
                }
            }
            Member::Construct(signature) => match index_of.get("\0constructor") {
                Some(index) => grouped[*index].1.push(signature.clone()),
                None => {
                    index_of.insert("\0constructor".to_string(), grouped.len());
                    grouped.push((member.clone(), Vec::new()));
                }
            },
            other => grouped.push((other.clone(), Vec::new())),
        }
    }
    grouped
}

/// A member's identity for override/dedup purposes while flattening.
fn member_key(member: &Member) -> Option<String> {
    match member {
        Member::Property(property) => Some(format!("property:{}", property.name)),
        Member::Method(method) => Some(format!("method:{}", method.signature.name)),
        Member::Construct(_) => Some("construct".to_string()),
        Member::Index(index) => Some(format!("index:{:?}", index.key)),
        Member::Call(_) | Member::Unsupported { .. } => None,
    }
}

/// The literal-typed field every member of a candidate discriminated union
/// shares, if there is one.
fn discriminant_field(members: &[&TsType]) -> Option<String> {
    let literal_fields = |member: &TsType| -> BTreeSet<String> {
        let TsType::Object(fields) = member else {
            return BTreeSet::new();
        };
        fields
            .iter()
            .filter_map(|field| match field {
                Member::Property(property)
                    if matches!(property.declared_type, Some(TsType::StringLiteral(_))) =>
                {
                    Some(property.name.clone())
                }
                _ => None,
            })
            .collect()
    };
    let mut shared = literal_fields(members.first()?);
    for member in &members[1..] {
        shared = shared
            .intersection(&literal_fields(member))
            .cloned()
            .collect();
    }
    shared.into_iter().next()
}

/// A union alias's variants, when every member is a string literal.
fn string_literal_variants(declared: &TsType) -> Option<Vec<(String, String)>> {
    let TsType::Union(members) = declared else {
        return None;
    };
    if members.len() < 2 {
        return None;
    }
    let mut variants = Vec::new();
    let mut taken = HashSet::new();
    for member in members {
        let TsType::StringLiteral(value) = member else {
            return None;
        };
        let base = to_pascal_case(value);
        let base = if base.is_empty() {
            "Empty".to_string()
        } else {
            base
        };
        let mut name = base.clone();
        let mut suffix = 2;
        while !taken.insert(name.clone()) {
            name = format!("{base}{suffix}");
            suffix += 1;
        }
        variants.push((name, value.clone()));
    }
    Some(variants)
}

/// A short rendering of a type for a TODO comment.
fn describe_type(declared: &TsType) -> String {
    match declared {
        TsType::Reference { name, arguments } if arguments.is_empty() => name.clone(),
        TsType::Reference { name, arguments } => format!(
            "{name}<{}>",
            arguments
                .iter()
                .map(describe_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TsType::Array(element) => format!("{}[]", describe_type(element)),
        TsType::Tuple(elements) => format!(
            "[{}]",
            elements
                .iter()
                .map(describe_type)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        TsType::Union(members) => members
            .iter()
            .map(describe_type)
            .collect::<Vec<_>>()
            .join(" | "),
        TsType::Intersection(members) => members
            .iter()
            .map(describe_type)
            .collect::<Vec<_>>()
            .join(" & "),
        TsType::Function(_) => "(…) => …".to_string(),
        TsType::Constructor(_) => "new (…) => …".to_string(),
        TsType::Object(_) => "{ … }".to_string(),
        TsType::StringLiteral(value) => format!("\"{value}\""),
        TsType::NumberLiteral(value) => value.clone(),
        TsType::BooleanLiteral(value) => value.to_string(),
        TsType::Unsupported { construct, .. } => construct.to_string(),
    }
}

// --- Type substitution (for `extends` flattening) ----------------------------

fn substitute_type(declared: &TsType, substitution: &HashMap<String, TsType>) -> TsType {
    if substitution.is_empty() {
        return declared.clone();
    }
    match declared {
        TsType::Reference { name, arguments } if arguments.is_empty() => substitution
            .get(name)
            .cloned()
            .unwrap_or_else(|| declared.clone()),
        TsType::Reference { name, arguments } => TsType::Reference {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_type(argument, substitution))
                .collect(),
        },
        TsType::Array(element) => TsType::Array(Box::new(substitute_type(element, substitution))),
        TsType::Tuple(elements) => TsType::Tuple(
            elements
                .iter()
                .map(|element| substitute_type(element, substitution))
                .collect(),
        ),
        TsType::Union(members) => TsType::Union(
            members
                .iter()
                .map(|member| substitute_type(member, substitution))
                .collect(),
        ),
        TsType::Intersection(members) => TsType::Intersection(
            members
                .iter()
                .map(|member| substitute_type(member, substitution))
                .collect(),
        ),
        TsType::Function(signature) => {
            TsType::Function(Box::new(substitute_signature(signature, substitution)))
        }
        TsType::Constructor(signature) => {
            TsType::Constructor(Box::new(substitute_signature(signature, substitution)))
        }
        TsType::Object(members) => TsType::Object(
            members
                .iter()
                .map(|member| substitute_member(member, substitution))
                .collect(),
        ),
        other => other.clone(),
    }
}

fn substitute_signature(
    signature: &Signature,
    substitution: &HashMap<String, TsType>,
) -> Signature {
    Signature {
        name: signature.name.clone(),
        generics: signature.generics.clone(),
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| Parameter {
                name: parameter.name.clone(),
                optional: parameter.optional,
                rest: parameter.rest,
                declared_type: parameter
                    .declared_type
                    .as_ref()
                    .map(|declared| substitute_type(declared, substitution)),
            })
            .collect(),
        return_type: signature
            .return_type
            .as_ref()
            .map(|declared| substitute_type(declared, substitution)),
        raw: signature.raw.clone(),
    }
}

fn substitute_member(member: &Member, substitution: &HashMap<String, TsType>) -> Member {
    match member {
        Member::Property(property) => Member::Property(PropertyMember {
            declared_type: property
                .declared_type
                .as_ref()
                .map(|declared| substitute_type(declared, substitution)),
            ..property.clone()
        }),
        Member::Method(method) => Member::Method(MethodMember {
            signature: substitute_signature(&method.signature, substitution),
            ..method.clone()
        }),
        Member::Construct(signature) => {
            Member::Construct(substitute_signature(signature, substitution))
        }
        Member::Call(signature) => Member::Call(substitute_signature(signature, substitution)),
        Member::Index(index) => Member::Index(dts::IndexMember {
            value: substitute_type(&index.value, substitution),
            ..index.clone()
        }),
        Member::Unsupported { construct, raw } => Member::Unsupported {
            construct,
            raw: raw.clone(),
        },
    }
}
