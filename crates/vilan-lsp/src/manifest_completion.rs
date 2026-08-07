//! Server-side `vilan.toml` completion (proposal `distribution.md` §5's second
//! rider): the keys each table accepts, and the values that come from a closed
//! set — offered by the language server, so every editor gets them and not just
//! the one with the JSON schema installed.
//!
//! **One listing, no drift.** [`TABLES`] below is the schema as the server
//! knows it, and `editors/vscode/schemas/vilan-toml.schema.json` is the schema
//! as the editor's TOML validator knows it. They are pinned to each other by
//! `schema_and_listing_agree` in this module's tests: a key added to one and
//! not the other fails the suite. The enumerable values are not written down
//! twice either — they are computed from the platform registry
//! (`vilan_core::target`) and the build presets, so adding a runtime updates the
//! completion and the pin at once.
//!
//! **Not the vilan pipeline.** A manifest is TOML; nothing here parses vilan or
//! runs analysis. The server routes a `vilan.toml` document to this module and
//! never hands it to `Document::analyze` — a manifest run through the vilan
//! lexer would publish a wall of nonsense.

use std::ops::Range;

use vilan_core::options::Preset;
use vilan_core::target::Platform;

/// One table of the manifest schema. `path` is the header as written, with `*`
/// standing for a user-chosen segment (`entry.*` matches `[entry.client]`).
pub struct Table {
    pub path: &'static str,
    pub documentation: &'static str,
    pub keys: &'static [Key],
}

/// One key of a table: how it is written, what it means, and — when the set of
/// values is closed — which values it takes.
pub struct Key {
    pub name: &'static str,
    pub documentation: &'static str,
    pub values: ValueSet,
}

/// What a key's value may be, when it is worth offering. `Open` is the honest
/// answer for a name, a path, or a number: there is nothing to enumerate.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ValueSet {
    Open,
    /// `true` / `false`, unquoted (TOML booleans).
    Boolean,
    /// A build platform, from the registry: `node`, `deno`, `bun`, `browser`,
    /// plus `none` (the host-less, check-only platform).
    Platform,
    /// A layer's platform pattern: the runtimes, their supported versions, the
    /// any-version `runtime:*` form, and the `@process` family.
    PlatformPattern,
    /// A `[build] preset`.
    Preset,
    /// Only `true` — `project = true` is the whole inherited-dependency form.
    OnlyTrue,
}

impl ValueSet {
    /// The values to offer, computed from the registries rather than listed —
    /// so a new runtime or preset appears here the day it is added.
    pub fn values(self) -> Vec<String> {
        match self {
            ValueSet::Open => Vec::new(),
            ValueSet::Boolean => vec!["true".to_string(), "false".to_string()],
            ValueSet::OnlyTrue => vec!["true".to_string()],
            ValueSet::Preset => Preset::all()
                .iter()
                .map(|preset| preset.name().to_string())
                .collect(),
            // A `target` is written bare (`node`), which the platform parser
            // reads as that runtime's supported version; `none` is not a host,
            // so it is not in `all_hosts`.
            ValueSet::Platform => {
                let mut values: Vec<String> = Platform::all_hosts()
                    .into_iter()
                    .map(runtime_name)
                    .collect();
                values.push("none".to_string());
                values
            }
            // A layer serves platforms, never `none`; it may name a family, a
            // runtime, that runtime's supported version, or any version of it.
            ValueSet::PlatformPattern => {
                let mut values = vec!["@process".to_string()];
                for host in Platform::all_hosts() {
                    let runtime = runtime_name(host);
                    let full = host.name();
                    values.push(runtime.clone());
                    if full != runtime {
                        values.push(full);
                        values.push(format!("{runtime}:*"));
                    }
                }
                values
            }
        }
    }
}

/// A platform's runtime name without its version (`node:24` → `node`).
fn runtime_name(platform: Platform) -> String {
    let name = platform.name();
    match name.split_once(':') {
        Some((runtime, _)) => runtime.to_string(),
        None => name,
    }
}

/// The keys of one dependency declaration — the inline table on the right of
/// `name = { … }`, and equally the body of a `[package.dependencies.<name>]`
/// table. Kept beside [`TABLES`] because it is the same schema seen from the
/// other side of an `=`.
pub const DEPENDENCY_KEYS: &[Key] = &[
    Key {
        name: "path",
        documentation: "A local directory, relative to this manifest.",
        values: ValueSet::Open,
    },
    Key {
        name: "git",
        documentation: "The repository URL of a git dependency. The checkout must be a \
                        `[library]` with its `vilan.toml` at the root.",
        values: ValueSet::Open,
    },
    Key {
        name: "tag",
        documentation: "The tag a git dependency pins (exactly one of `tag` / `rev`). A \
                        branch cannot be pinned; it moves.",
        values: ValueSet::Open,
    },
    Key {
        name: "rev",
        documentation: "The commit SHA a git dependency pins (exactly one of `tag` / `rev`).",
        values: ValueSet::Open,
    },
    Key {
        name: "branch",
        documentation: "Rejected: a branch moves, so it pins nothing. Use `tag` or `rev`.",
        values: ValueSet::Open,
    },
    Key {
        name: "project",
        documentation: "`project = true` takes this dependency's whole declaration from the \
                        workspace root's `[project.dependencies]` (paths there are relative \
                        to the project root). Combines with no other key.",
        values: ValueSet::OnlyTrue,
    },
    Key {
        name: "version",
        documentation: "A registry dependency's version requirement. Registry dependencies \
                        are not resolved yet.",
        values: ValueSet::Open,
    },
    Key {
        name: "registry",
        documentation: "The registry to fetch from. Not resolved yet.",
        values: ValueSet::Open,
    },
];

/// The tables a `vilan.toml` may declare, with the keys each accepts.
pub const TABLES: &[Table] = &[
    Table {
        path: "package",
        documentation: "A package: a buildable, importable unit.",
        keys: &[
            Key {
                name: "name",
                documentation: "How other packages import this one. A valid identifier.",
                values: ValueSet::Open,
            },
            Key {
                name: "description",
                documentation: "A short, free-text description of the package.",
                values: ValueSet::Open,
            },
            Key {
                name: "root",
                documentation: "The package source root, relative to this manifest. Default \
                                `src`.",
                values: ValueSet::Open,
            },
            Key {
                name: "entry",
                documentation: "The `build`/`run` entry file, resolved against `root`. \
                                Default `main.vl`. Replaced by `[entry.<name>]` sections.",
                values: ValueSet::Open,
            },
            Key {
                name: "target",
                documentation: "The default build platform. `none` is a pure library \
                                (type-checkable, not buildable).",
                values: ValueSet::Platform,
            },
            Key {
                name: "split",
                documentation: "Emit route chunks: an eager bundle plus one lazily imported \
                                file per route arm, fetched when a navigation reaches it. \
                                `browser` legs only. Replaced by `[entry.<name>] split` when \
                                entry sections are declared.",
                values: ValueSet::Boolean,
            },
            Key {
                name: "default-entry",
                documentation: "Which `[entry.<name>]` `vilan run` executes when several \
                                are runnable. `--entry` overrides it.",
                values: ValueSet::Open,
            },
            Key {
                name: "dependencies",
                documentation: "The packages this one may import, by the name it imports \
                                them under. Usually written as `[package.dependencies]`.",
                values: ValueSet::Open,
            },
        ],
    },
    Table {
        path: "library",
        documentation: "A library: an importable unit with a `lib.vl` surface, no entry, and \
                        per-platform layers instead of one target.",
        keys: &[
            Key {
                name: "name",
                documentation: "How dependents import this library. A valid identifier.",
                values: ValueSet::Open,
            },
            Key {
                name: "description",
                documentation: "A short, free-text description of the library.",
                values: ValueSet::Open,
            },
            Key {
                name: "root",
                documentation: "The base (shared) source root, relative to this manifest. \
                                Default `src`.",
                values: ValueSet::Open,
            },
            Key {
                name: "dependencies",
                documentation: "The packages this library may import. Usually written as \
                                `[library.dependencies]`.",
                values: ValueSet::Open,
            },
            Key {
                name: "layer",
                documentation: "The per-platform overlay layers, keyed by name. Usually \
                                written as `[library.layer.<name>]`.",
                values: ValueSet::Open,
            },
        ],
    },
    Table {
        path: "library.layer.*",
        documentation: "An overlay layer: a source root that shadows the base for the \
                        platforms it serves.",
        keys: &[
            Key {
                name: "root",
                documentation: "The layer's source root, relative to this manifest. Default \
                                `src/<layer name>`.",
                values: ValueSet::Open,
            },
            Key {
                name: "platform",
                documentation: "The platforms this layer serves (at least one). A runtime, a \
                                runtime version, or a family like `@process`.",
                values: ValueSet::PlatformPattern,
            },
        ],
    },
    Table {
        path: "project",
        documentation: "A workspace root: member packages, plus dependencies they can inherit.",
        keys: &[
            Key {
                name: "packages",
                documentation: "Paths to the member package directories: the build set of \
                                `vilan build .` at the root.",
                values: ValueSet::Open,
            },
            Key {
                name: "default-entry",
                documentation: "Which member package `vilan run` executes when several are \
                                runnable. `--entry` overrides it.",
                values: ValueSet::Open,
            },
            Key {
                name: "dependencies",
                documentation: "Dependencies declared once for the members. A member takes \
                                one with `dep = { project = true }`. Usually written as \
                                `[project.dependencies]`.",
                values: ValueSet::Open,
            },
        ],
    },
    Table {
        path: "entry.*",
        documentation: "One build entry of a multi-entry package; the name labels its \
                        `dist/<name>` output (`.mjs` on a process target, `.js` on the \
                        browser).",
        keys: &[
            Key {
                name: "path",
                documentation: "The entry file, resolved against the package `root`. Default \
                                `<name>.vl`.",
                values: ValueSet::Open,
            },
            Key {
                name: "target",
                documentation: "The entry's build platform. Must be a host: an entry is \
                                something to run.",
                values: ValueSet::Platform,
            },
            Key {
                name: "split",
                documentation: "Emit route chunks: an eager bundle plus one lazily imported \
                                file per route arm, fetched when a navigation reaches it. \
                                `browser` legs only.",
                values: ValueSet::Boolean,
            },
        ],
    },
    Table {
        path: "build",
        documentation: "Build settings: the `run` hooks, plus the code-generation knobs (a \
                        `preset` initializes every knob; individual keys then override it). \
                        The knobs never change program semantics, only the emitted text.",
        keys: &[
            Key {
                name: "run",
                documentation: "Commands to run through the platform shell BEFORE each \
                                build, in the manifest's directory. One bare, or several in \
                                a list. A failure fails the build.",
                values: ValueSet::Open,
            },
            Key {
                name: "preset",
                documentation: "A named starting point: `debug` is readable, `release` is \
                                minified and obfuscated.",
                values: ValueSet::Preset,
            },
            Key {
                name: "indent",
                documentation: "Lay the output across lines with indentation (vs one flat \
                                line).",
                values: ValueSet::Boolean,
            },
            Key {
                name: "spaces",
                documentation: "Pad tokens with spaces (`a + b`) vs tight (`a+b`).",
                values: ValueSet::Boolean,
            },
            Key {
                name: "readable-names",
                documentation: "Name generated identifiers after their source (most \
                                debuggable).",
                values: ValueSet::Boolean,
            },
            Key {
                name: "debug-names",
                documentation: "When `readable-names` is off, annotate obfuscated names with \
                                their source (`a/*count*/`).",
                values: ValueSet::Boolean,
            },
            Key {
                name: "infer-const",
                documentation: "Fold `let` initializers the compiler can evaluate, without the \
                                `const` keyword (on under `release`, off under `debug`).",
                values: ValueSet::Boolean,
            },
        ],
    },
    Table {
        path: "macro",
        documentation: "The compile-time interpreter's budget.",
        keys: &[
            Key {
                name: "fuel",
                documentation: "Interpreter steps per macro/const run (default 1000000).",
                values: ValueSet::Open,
            },
            Key {
                name: "depth",
                documentation: "Nested expansion rounds before the fixpoint gives up \
                                (default 16).",
                values: ValueSet::Open,
            },
        ],
    },
];

/// The headers that can be completed literally — every table whose path has no
/// user-chosen `*` segment, plus the dependency tables (which are written as a
/// header just as often as inline).
pub const HEADERS: &[&str] = &[
    "package",
    "package.dependencies",
    "library",
    "library.dependencies",
    "project",
    "project.dependencies",
    "build",
    "macro",
];

/// The table whose `path` matches `header`, where `*` matches one segment.
fn table_for(header: &str) -> Option<&'static Table> {
    TABLES.iter().find(|table| {
        let mut pattern = table.path.split('.');
        let mut actual = header.split('.');
        loop {
            match (pattern.next(), actual.next()) {
                (None, None) => return true,
                (Some(expected), Some(segment)) if expected == "*" || expected == segment => {}
                _ => return false,
            }
        }
    })
}

/// Whether `header` names a dependency table — `<kind>.dependencies`, whose keys
/// are the user's own import names.
fn is_dependency_table(header: &str) -> bool {
    matches!(
        header,
        "package.dependencies" | "library.dependencies" | "project.dependencies"
    )
}

/// Whether `header` names ONE dependency declared as a table
/// (`[package.dependencies.shapes]`), whose keys are [`DEPENDENCY_KEYS`].
fn is_dependency_declaration(header: &str) -> bool {
    match header.rsplit_once('.') {
        Some((prefix, _)) => is_dependency_table(prefix),
        None => false,
    }
}

/// Where the cursor sits in a manifest — the question completion answers.
#[derive(Debug, PartialEq, Eq)]
pub enum Context {
    /// Inside a `[header]`, completing the header itself.
    Header,
    /// A key position in the table named by the header (`""` at the top of the
    /// file, before any header).
    Key(String),
    /// A value position: the named key of the named table.
    Value { table: String, key: String },
    /// A key position inside a dependency declaration.
    DependencyKey,
    /// A value position inside a dependency declaration.
    DependencyValue { key: String },
    /// A comment, a string, or a place with nothing to say.
    Nothing,
}

/// One completion the server offers for a manifest: what to show, what to
/// insert, and exactly which bytes it replaces. The range is computed here
/// rather than left to the client's word rule, because a manifest value carries
/// its own quotes — replacing "the word" would leave `""node"`.
pub struct ManifestCompletion {
    pub label: String,
    pub documentation: Option<String>,
    /// True for a key/header, false for a value — the server maps this to the
    /// LSP item kind.
    pub is_key: bool,
    pub replace: Range<usize>,
    pub insert: String,
}

/// The completions for `text` at byte `offset`.
pub fn completions(text: &str, offset: usize) -> Vec<ManifestCompletion> {
    let offset = offset.min(text.len());
    let context = context_at(text, offset);
    match context {
        Context::Nothing => Vec::new(),
        Context::Header => {
            let replace = header_range(text, offset);
            HEADERS
                .iter()
                .map(|header| ManifestCompletion {
                    label: (*header).to_string(),
                    documentation: table_for(header).map(|table| table.documentation.to_string()),
                    is_key: true,
                    replace: replace.clone(),
                    insert: (*header).to_string(),
                })
                .collect()
        }
        Context::Key(header) => {
            // A dependency table's keys are the user's own import names, so
            // there is nothing to offer — and offering the schema's keys there
            // would be actively wrong.
            let keys = if is_dependency_declaration(&header) {
                DEPENDENCY_KEYS
            } else if is_dependency_table(&header) {
                &[]
            } else {
                match table_for(&header) {
                    Some(table) => table.keys,
                    None => &[],
                }
            };
            let replace = word_range(text, offset);
            keys.iter()
                .map(|key| ManifestCompletion {
                    label: key.name.to_string(),
                    documentation: Some(key.documentation.to_string()),
                    is_key: true,
                    replace: replace.clone(),
                    insert: key.name.to_string(),
                })
                .collect()
        }
        Context::DependencyKey => {
            let replace = word_range(text, offset);
            DEPENDENCY_KEYS
                .iter()
                .map(|key| ManifestCompletion {
                    label: key.name.to_string(),
                    documentation: Some(key.documentation.to_string()),
                    is_key: true,
                    replace: replace.clone(),
                    insert: key.name.to_string(),
                })
                .collect()
        }
        Context::Value { table, key } => {
            let keys = if is_dependency_declaration(&table) {
                DEPENDENCY_KEYS
            } else {
                match table_for(&table) {
                    Some(table) => table.keys,
                    None => return Vec::new(),
                }
            };
            match keys.iter().find(|candidate| candidate.name == key) {
                Some(key) => value_completions(text, offset, key),
                None => Vec::new(),
            }
        }
        Context::DependencyValue { key } => {
            match DEPENDENCY_KEYS
                .iter()
                .find(|candidate| candidate.name == key)
            {
                Some(key) => value_completions(text, offset, key),
                None => Vec::new(),
            }
        }
    }
}

/// The value candidates for `key` at the cursor, each replacing the whole value
/// token (quotes included) so the result is valid TOML however much of it the
/// user had typed.
fn value_completions(text: &str, offset: usize, key: &Key) -> Vec<ManifestCompletion> {
    let quoted = !matches!(key.values, ValueSet::Boolean | ValueSet::OnlyTrue);
    let replace = value_range(text, offset);
    key.values
        .values()
        .into_iter()
        .map(|value| ManifestCompletion {
            label: value.clone(),
            documentation: Some(key.documentation.to_string()),
            is_key: false,
            replace: replace.clone(),
            insert: if quoted {
                format!("\"{value}\"")
            } else {
                value.clone()
            },
        })
        .collect()
}

/// The start of the line containing `offset`.
fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map(|at| at + 1).unwrap_or(0)
}

/// The table header in effect at `offset`: the last `[header]` line above it,
/// with its brackets and any quoting stripped. `""` before the first header.
fn enclosing_header(text: &str, offset: usize) -> String {
    let mut header = String::new();
    for line in text[..line_start(text, offset)].lines() {
        let trimmed = line.trim();
        // `[[array.of.tables]]` is not part of the manifest schema, but reading
        // its inner name keeps the header tracking honest if one appears.
        let inner = trimmed
            .strip_prefix("[[")
            .and_then(|rest| rest.strip_suffix("]]"))
            .or_else(|| {
                trimmed
                    .strip_prefix('[')
                    .and_then(|rest| rest.strip_suffix(']'))
            });
        if let Some(inner) = inner {
            header = inner
                .split('.')
                .map(|segment| segment.trim().trim_matches(['"', '\'']))
                .collect::<Vec<_>>()
                .join(".");
        }
    }
    header
}

/// The scan of one line up to the cursor: what a completion needs to know that
/// only a left-to-right read can answer.
struct LineScan {
    /// Inside a string literal — nothing to complete.
    in_string: bool,
    /// Inside a `#` comment — likewise.
    in_comment: bool,
    /// Inline-table depth (`{`).
    depth: usize,
    /// The byte offset just after the `=` that opens the current value, at the
    /// current depth, if any.
    value_start: Option<usize>,
    /// The key text before that `=`.
    key: String,
}

/// Reads `text[from..to]` as one line's prefix, tracking strings, comments,
/// inline tables, and the `=` that opens a value.
fn scan_line(text: &str, from: usize, to: usize) -> LineScan {
    let mut scan = LineScan {
        in_string: false,
        in_comment: false,
        depth: 0,
        value_start: None,
        key: String::new(),
    };
    let mut quote = '"';
    let mut key_start = from;
    let mut index = from;
    for character in text[from..to].chars() {
        let width = character.len_utf8();
        if scan.in_comment {
            index += width;
            continue;
        }
        if scan.in_string {
            if character == quote {
                scan.in_string = false;
            }
            index += width;
            continue;
        }
        match character {
            '"' | '\'' => {
                scan.in_string = true;
                quote = character;
            }
            '#' => scan.in_comment = true,
            '{' => {
                scan.depth += 1;
                scan.value_start = None;
                key_start = index + width;
            }
            '}' => {
                scan.depth = scan.depth.saturating_sub(1);
                scan.value_start = None;
            }
            ',' if scan.depth > 0 => {
                scan.value_start = None;
                key_start = index + width;
            }
            '=' if scan.value_start.is_none() => {
                scan.key = text[key_start..index].trim().trim_matches('"').to_string();
                scan.value_start = Some(index + width);
            }
            _ => {}
        }
        index += width;
    }
    scan
}

/// Where the cursor sits (see [`Context`]).
pub fn context_at(text: &str, offset: usize) -> Context {
    let start = line_start(text, offset);
    let scan = scan_line(text, start, offset);
    // A comment says nothing, and neither does a string in KEY position (a
    // quoted key). A string in VALUE position is exactly where a value
    // completion belongs, so it is not excluded.
    if scan.in_comment || (scan.in_string && scan.value_start.is_none()) {
        return Context::Nothing;
    }
    let prefix = &text[start..offset];
    if scan.depth == 0 && prefix.trim_start().starts_with('[') {
        return Context::Header;
    }
    let header = enclosing_header(text, offset);
    if scan.depth > 0 {
        // An inline table. The only one the schema knows is a dependency's, so
        // anywhere else there is nothing honest to offer.
        if !is_dependency_table(&header) {
            return Context::Nothing;
        }
        return match &scan.value_start {
            Some(_) => Context::DependencyValue {
                key: scan.key.clone(),
            },
            None => Context::DependencyKey,
        };
    }
    match &scan.value_start {
        Some(_) => Context::Value {
            table: header,
            key: scan.key.clone(),
        },
        None => Context::Key(header),
    }
}

/// The identifier-ish run immediately before `offset` — what a key completion
/// replaces.
fn word_range(text: &str, offset: usize) -> Range<usize> {
    let start = text[..offset]
        .char_indices()
        .rev()
        .take_while(|(_, character)| {
            character.is_ascii_alphanumeric() || *character == '_' || *character == '-'
        })
        .last()
        .map(|(at, _)| at)
        .unwrap_or(offset);
    start..offset
}

/// What a header completion replaces: everything between the `[` and the
/// cursor. The insert is the bare header, so both brackets stay the user's —
/// completing inside `[pack|]` leaves exactly one `]`.
fn header_range(text: &str, offset: usize) -> Range<usize> {
    let start = line_start(text, offset);
    let open = text[start..offset]
        .rfind('[')
        .map(|at| start + at + 1)
        .unwrap_or(offset);
    open..offset
}

/// What a value completion replaces: the value token under the cursor — from
/// just after the `=` (or the array's `[` / `,`), through any opening quote and
/// partial text, and over the closing quote an editor may have auto-inserted.
fn value_range(text: &str, offset: usize) -> Range<usize> {
    let line = line_start(text, offset);
    let scan = scan_line(text, line, offset);
    let mut start = scan.value_start.unwrap_or(offset);
    // Inside an array (`platform = ["@process", …]`) each element is its own
    // value token.
    let region = &text[start..offset];
    if let Some(at) = region.rfind(['[', ',']) {
        start += at + 1;
    }
    // Leading whitespace is not part of the token.
    let leading = text[start..offset]
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map(|(at, _)| at)
        .unwrap_or(offset - start);
    start += leading;
    let quoted = text[start..offset].starts_with(['"', '\'']);
    let line_end = text[offset..]
        .find('\n')
        .map(|at| offset + at)
        .unwrap_or(text.len());
    let mut end = offset;
    if quoted && text[offset..line_end].starts_with(['"', '\'']) {
        end += 1;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The context at the `|` marker in `source` (which is removed first).
    fn at(source: &str) -> Context {
        let offset = source.find('|').expect("mark the cursor with `|`");
        let text = source.replace('|', "");
        context_at(&text, offset)
    }

    /// The completion labels offered at the `|` marker.
    fn labels(source: &str) -> Vec<String> {
        let offset = source.find('|').expect("mark the cursor with `|`");
        let text = source.replace('|', "");
        completions(&text, offset)
            .into_iter()
            .map(|item| item.label)
            .collect()
    }

    /// The text `source` becomes when the completion labeled `label` is applied.
    fn applied(source: &str, label: &str) -> String {
        let offset = source.find('|').expect("mark the cursor with `|`");
        let text = source.replace('|', "");
        let item = completions(&text, offset)
            .into_iter()
            .find(|item| item.label == label)
            .unwrap_or_else(|| panic!("no completion labeled `{label}`"));
        let mut applied = text.clone();
        applied.replace_range(item.replace, &item.insert);
        applied
    }

    // ── context detection ──

    #[test]
    fn a_key_position_knows_its_table() {
        assert_eq!(at("[package]\nna|\n"), Context::Key("package".to_string()));
        assert_eq!(at("[build]\n|"), Context::Key("build".to_string()));
        assert_eq!(
            at("[package]\nname = \"a\"\n\n[entry.client]\n|\n"),
            Context::Key("entry.client".to_string())
        );
        assert_eq!(
            at("[library]\n[library.layer.process]\nplat|\n"),
            Context::Key("library.layer.process".to_string())
        );
    }

    #[test]
    fn a_value_position_knows_its_key() {
        assert_eq!(
            at("[package]\ntarget = |\n"),
            Context::Value {
                table: "package".to_string(),
                key: "target".to_string()
            }
        );
        assert_eq!(
            at("[package]\ntarget = \"no|\"\n"),
            Context::Value {
                table: "package".to_string(),
                key: "target".to_string()
            }
        );
    }

    #[test]
    fn a_dependencys_inline_table_is_its_own_context() {
        assert_eq!(
            at("[package.dependencies]\nshapes = { |\n"),
            Context::DependencyKey
        );
        assert_eq!(
            at("[package.dependencies]\nshapes = { path = \"../s\", gi| }\n"),
            Context::DependencyKey
        );
        assert_eq!(
            at("[package.dependencies]\nshapes = { project = | }\n"),
            Context::DependencyValue {
                key: "project".to_string()
            }
        );
    }

    #[test]
    fn a_dependency_declared_as_a_table_completes_the_same_keys() {
        assert_eq!(
            at("[package.dependencies.shapes]\n|\n"),
            Context::Key("package.dependencies.shapes".to_string())
        );
        assert!(labels("[package.dependencies.shapes]\n|\n").contains(&"git".to_string()));
    }

    #[test]
    fn a_header_line_completes_headers() {
        assert_eq!(at("[pack|"), Context::Header);
        assert_eq!(at("[package]\nname = \"a\"\n[bui|]\n"), Context::Header);
        assert!(labels("[pack|").contains(&"package".to_string()));
        assert!(labels("[|").contains(&"project.dependencies".to_string()));
    }

    #[test]
    fn comments_and_strings_offer_nothing() {
        assert_eq!(at("[package]\n# na|\n"), Context::Nothing);
        assert_eq!(
            at("[package]\n# a comment with [brackets] |\n"),
            Context::Nothing
        );
        // A string in KEY position (a quoted key) is not a value — nothing yet.
        assert_eq!(at("[package]\n\"na|\n"), Context::Nothing);
    }

    #[test]
    fn a_dependency_tables_own_keys_are_the_users_names() {
        // The import names are the user's; offering schema keys there would be
        // wrong, so the honest answer is nothing.
        assert!(labels("[package.dependencies]\n|\n").is_empty());
    }

    // ── what gets inserted ──

    #[test]
    fn a_value_completion_writes_valid_toml_however_much_was_typed() {
        assert_eq!(
            applied("[package]\ntarget = |\n", "browser"),
            "[package]\ntarget = \"browser\"\n"
        );
        assert_eq!(
            applied("[package]\ntarget = \"|\n", "browser"),
            "[package]\ntarget = \"browser\"\n"
        );
        // The editor's auto-closed quote is consumed, not duplicated.
        assert_eq!(
            applied("[package]\ntarget = \"|\"\n", "browser"),
            "[package]\ntarget = \"browser\"\n"
        );
        assert_eq!(
            applied("[package]\ntarget = \"brow|\"\n", "browser"),
            "[package]\ntarget = \"browser\"\n"
        );
    }

    #[test]
    fn a_boolean_value_is_inserted_unquoted() {
        assert_eq!(
            applied("[build]\nindent = |\n", "true"),
            "[build]\nindent = true\n"
        );
        assert_eq!(
            applied("[package.dependencies]\nshapes = { project = | }\n", "true"),
            "[package.dependencies]\nshapes = { project = true }\n"
        );
    }

    #[test]
    fn an_array_element_replaces_only_that_element() {
        assert_eq!(
            applied(
                "[library.layer.process]\nplatform = [\"node\", \"|\"]\n",
                "@process"
            ),
            "[library.layer.process]\nplatform = [\"node\", \"@process\"]\n"
        );
    }

    #[test]
    fn a_key_completion_replaces_the_partial_word() {
        assert_eq!(applied("[package]\nnam|\n", "name"), "[package]\nname\n");
        assert_eq!(
            applied("[build]\nreadable-|\n", "readable-names"),
            "[build]\nreadable-names\n"
        );
    }

    #[test]
    fn a_header_completion_does_not_double_the_bracket() {
        assert_eq!(applied("[pack|]\n", "package"), "[package]\n");
        assert_eq!(applied("[pack|\n", "package"), "[package\n");
    }

    // ── the enumerable values come from the registries ──

    #[test]
    fn platform_values_are_the_registrys_hosts_plus_none() {
        let values = ValueSet::Platform.values();
        for host in Platform::all_hosts() {
            let runtime = super::runtime_name(host);
            assert!(
                values.contains(&runtime),
                "{runtime} missing from {values:?}"
            );
        }
        assert!(values.contains(&"none".to_string()));
    }

    #[test]
    fn layer_patterns_cover_the_family_and_the_versions() {
        let values = ValueSet::PlatformPattern.values();
        assert!(values.contains(&"@process".to_string()));
        assert!(values.contains(&"browser".to_string()));
        assert!(values.contains(&"node".to_string()));
        assert!(
            values
                .iter()
                .any(|value| value.starts_with("node:") && value != "node:*"),
            "the supported node version is offered: {values:?}"
        );
        assert!(values.contains(&"node:*".to_string()));
        // A layer serves platforms; `none` is not one.
        assert!(!values.contains(&"none".to_string()));
    }

    // ── the drift pin ──

    /// The schema the VS Code extension ships, parsed.
    fn schema() -> serde_json::Value {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../editors/vscode/schemas/vilan-toml.schema.json");
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        serde_json::from_str(&text).expect("the schema is valid JSON")
    }

    /// The property names of a schema object, sorted.
    fn properties(node: &serde_json::Value) -> Vec<String> {
        let mut names: Vec<String> = node
            .get("properties")
            .and_then(|properties| properties.as_object())
            .map(|properties| properties.keys().cloned().collect())
            .unwrap_or_default();
        names.sort();
        names
    }

    /// A listing table's key names, sorted.
    fn listed(path: &str) -> Vec<String> {
        let table = table_for(path).unwrap_or_else(|| panic!("no listed table `{path}`"));
        let mut names: Vec<String> = table.keys.iter().map(|key| key.name.to_string()).collect();
        names.sort();
        names
    }

    /// A schema enum's values, sorted.
    fn enumerated(node: &serde_json::Value) -> Vec<String> {
        let mut values: Vec<String> = node
            .get("enum")
            .and_then(|values| values.as_array())
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        values.sort();
        values
    }

    /// Sorted, for comparing against a schema enum.
    fn sorted(mut values: Vec<String>) -> Vec<String> {
        values.sort();
        values
    }

    // THE anti-drift pin: the server's listing and the extension's JSON schema
    // describe the same manifest, key for key and value for value. Adding a key
    // to `manifest.rs` means adding it in both places, and this fails until it
    // is in both.
    #[test]
    fn schema_and_listing_agree() {
        let schema = schema();
        let root = &schema["properties"];
        for table in ["package", "library", "project", "build", "macro"] {
            assert_eq!(
                properties(&root[table]),
                listed(table),
                "`[{table}]` differs between the schema and the server's listing"
            );
        }
        assert_eq!(
            properties(&root["entry"]["additionalProperties"]),
            listed("entry.*"),
            "`[entry.<name>]` differs"
        );
        assert_eq!(
            properties(&root["library"]["properties"]["layer"]["additionalProperties"]),
            listed("library.layer.*"),
            "`[library.layer.<name>]` differs"
        );
        let mut dependency_keys: Vec<String> = DEPENDENCY_KEYS
            .iter()
            .map(|key| key.name.to_string())
            .collect();
        dependency_keys.sort();
        assert_eq!(
            properties(&schema["definitions"]["dependency"]),
            dependency_keys,
            "a dependency declaration differs"
        );
        // The top-level tables are the manifest's own known sections (the
        // retired `[server]`/`[client]` pair is parsed only to reject it, so it
        // is neither completed nor advertised by the schema).
        let mut top_level: Vec<String> = root
            .as_object()
            .expect("the schema declares properties")
            .keys()
            .cloned()
            .collect();
        top_level.sort();
        let mut listed_top: Vec<String> = TABLES
            .iter()
            .filter_map(|table| table.path.split('.').next())
            .map(str::to_string)
            .collect();
        listed_top.sort();
        listed_top.dedup();
        assert_eq!(top_level, listed_top, "the top-level tables differ");
        for section in &listed_top {
            assert!(
                vilan_core::manifest::KNOWN_SECTIONS.contains(&section.as_str()),
                "`[{section}]` is not a section the manifest parses"
            );
        }
    }

    #[test]
    fn schema_and_listing_agree_on_the_enumerable_values() {
        let schema = schema();
        let root = &schema["properties"];
        assert_eq!(
            enumerated(&root["package"]["properties"]["target"]),
            sorted(ValueSet::Platform.values()),
            "`[package] target`"
        );
        assert_eq!(
            enumerated(&root["entry"]["additionalProperties"]["properties"]["target"]),
            sorted(ValueSet::Platform.values()),
            "`[entry.<name>] target`"
        );
        assert_eq!(
            enumerated(&root["build"]["properties"]["preset"]),
            sorted(ValueSet::Preset.values()),
            "`[build] preset`"
        );
        assert_eq!(
            enumerated(
                &root["library"]["properties"]["layer"]["additionalProperties"]["properties"]["platform"]
                    ["items"]
            ),
            sorted(ValueSet::PlatformPattern.values()),
            "`[library.layer.<name>] platform`"
        );
    }
}
