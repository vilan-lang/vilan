//! Source maps for the JS backend (debugging.md §5.1, E257 S5).
//!
//! The emitter builds a JS tree and prints it with a string printer that
//! knows no positions, so a position is carried the cheap way: the
//! transformer leaves a MARK in the tree at each place a debugger stops or a
//! stack frame lands — a statement ([`js::Node::Mark`]), a call
//! ([`js::Node::At`]), a function or closure head (`origin` on
//! [`js::Function`] / [`js::Closure`]) — and the printer writes one
//! [`MARK`] character at each, recording which entity it stood for in
//! TEXTUAL order. [`strip_marks`] then removes the characters in one pass,
//! and the generated line and column of each is where the map's segment
//! goes. The map's original side is the entity's span, read off the
//! [`Program`] the way a panic location is (`site_location`, S0).
//!
//! Marks exist only on the map's own entry
//! ([`crate::transformer::transform_with_source_map`]): `transform`, the
//! check's `diagnose`, the const world and the macro interpreter never see
//! one, so a build without a map — and every check — pays nothing. With one,
//! the JS is byte-identical to the build without it; the corpus pins that
//! (`crates/vilan-core/tests/source_maps.rs`).
//!
//! The level is the owner's (debug-50's measurement, ruled for Order 51):
//! statements, calls, closures and function heads — where a breakpoint
//! binds and a frame reports — and not every expression. `sourcesContent`
//! carries the user's files only, each up to [`SOURCES_CONTENT_CAP`] (a
//! larger one — kolt's generated icon table — goes by its path, debug-51's
//! Q1); a std source is named by its path in the std cache and listed in
//! `ignoreList`, so "step into" skips it. The runtime helpers the script
//! opens with (`__at`, `__panic`, …) are one more source, synthetic
//! ([`RUNTIME_SOURCE`]): their own text, ignore-listed, so a frame inside
//! one names it and "step into" passes over it (E257 S6).
//!
//! [`js::Node::Mark`]: crate::transformer::js::Node::Mark
//! [`js::Node::At`]: crate::transformer::js::Node::At
//! [`js::Function`]: crate::transformer::js::Function
//! [`js::Closure`]: crate::transformer::js::Closure

use crate::analyzer::{DERIVED_SOURCE, Program, SourceId};
use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use std::path::{Component, Path, PathBuf};

/// The character the printer writes at a mark: a Unicode NONCHARACTER
/// (U+FDD0), reserved for exactly this — internal use, never interchanged.
/// It cannot reach the output by another road: a string literal escapes
/// every non-ASCII character (`escape_default`), and identifiers are ASCII.
pub(crate) const MARK: char = '\u{FDD0}';

/// [`MARK`] as the text the printer writes.
pub(crate) const MARK_TEXT: &str = "\u{FDD0}";

/// The largest user file a map carries in `sourcesContent` (debug-51's Q1,
/// ruled as recommended): past it the file goes by its path, relative to the
/// map, which a local debugger reads from disk. A hand-written module is far
/// below it; what crosses it is generated (kolt's `src/lucide/lib.vl`, 636 KB,
/// made its client's map 978 KB against a 590 KB script).
pub const SOURCES_CONTENT_CAP: usize = 256 * 1024;

/// The synthetic source the runtime helpers map into (E257 S6): the helper
/// text the script opens with, carried in `sourcesContent` line for line and
/// listed in `ignoreList`. Not a file — a debugger shows the carried text, and
/// node's `--enable-source-maps` names a helper's frame by it
/// (`at __panic (vilan:///runtime.js:3:8)`).
pub const RUNTIME_SOURCE: &str = "vilan:///runtime.js";

/// What a mark stands for, which decides where in the entity's span the
/// map points.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkKind {
    /// A statement, or a body's tail: the span's start.
    Statement,
    /// A call: the callee's NAME, as a panic location reads it
    /// (`rows.first().unwrap()` points at `unwrap`).
    Call,
    /// A function head: the function's name, carried into `names`.
    Function,
    /// A closure head: the span's start.
    Closure,
}

/// One mark the printer wrote: what it stood for, in textual order.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SourceMark {
    pub kind: MarkKind,
    pub id: Id,
}

/// A version 3 source map (ECMA-426) for one emitted file, its sources still
/// ABSOLUTE: where the map is written decides how a source is spelled
/// ([`SourceMap::to_json`]).
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    pub sources: Vec<MapSource>,
    pub names: Vec<String>,
    /// The `mappings` string: VLQ segments, `;` between generated lines.
    pub mappings: String,
    /// The `.vdbg.json` sidecar that rides with this map (E257 S7): the
    /// script's frames and their bindings by static type, written beside the
    /// map wherever the map is. Not part of the map's own JSON.
    pub debug: Option<Box<crate::debug_info::DebugInfo>>,
}

/// One source the map points into.
#[derive(Clone, Debug)]
pub struct MapSource {
    /// The file, as the build read it (canonical where it exists); empty for
    /// a synthetic source, which `url` names instead.
    pub path: PathBuf,
    /// A synthetic source's URL, written as it is: [`RUNTIME_SOURCE`].
    pub url: Option<&'static str>,
    /// The file's text — the user's own files up to [`SOURCES_CONTENT_CAP`],
    /// and the runtime's. `None` for std, which a debugger reads from `path`
    /// in the std cache (the owner's ruling on debug-50's measurement: std is
    /// 1.85 MB against kolt's 184 KB), and for a user file past the cap.
    pub content: Option<String>,
    /// std and the runtime: listed in `ignoreList`, so a debugger's "step
    /// into" stays in the user's code.
    pub ignored: bool,
}

/// Strips the [`MARK`]s out of `text` and builds the map they describe.
///
/// `marks` are the printer's records in textual order, one per [`MARK`]
/// character. Three passes: the strip, which places each mark at its
/// generated line and column; the location of every mark's original place,
/// one sweep per source in offset order (a source map reaches into whole std
/// files, 600 KB of them for a small browser app, and a line table per file
/// was most of what the map cost a build); and the encoding, in generated
/// order. A mark whose entity has no place in a real file — a synthetic
/// node, a span into a macro's generated text — maps nothing, and its
/// character is removed all the same. Two marks at one generated position
/// keep the first that maps: the statement that starts there, ahead of the
/// call it starts with.
///
/// The count of characters and records must agree; a printer that wrote a
/// mark it did not record (or the reverse) is a compiler bug, and the map
/// then stops at the first unpaired character rather than pairing the rest
/// wrongly.
///
/// `runtime` is the byte range of the runtime helpers' text in `text`, when
/// the script carries any: the prelude, ahead of every mark. Each of its
/// lines maps to the same line of [`RUNTIME_SOURCE`].
pub(crate) fn strip_marks(
    program: &Program,
    text: &str,
    marks: &[SourceMark],
    runtime: Option<std::ops::Range<usize>>,
) -> (String, SourceMap) {
    let mut clean = String::with_capacity(text.len());
    let mut placed: Vec<(Position, SourceMark)> = Vec::with_capacity(marks.len());
    let mut position = Position::default();
    let mut records = marks.iter();
    let mut paired = true;
    let mut rest = text;
    while let Some(at) = rest.find(MARK) {
        let (before, after) = rest.split_at(at);
        position.advance(before);
        clean.push_str(before);
        rest = &after[MARK.len_utf8()..];
        match records.next() {
            Some(mark) if paired => placed.push((position, *mark)),
            _ => paired = false,
        }
    }
    clean.push_str(rest);
    debug_assert!(
        paired && records.next().is_none(),
        "the printer's marks and its records disagree (E257 S5)"
    );
    let originals = locate(program, &placed);
    let mut writer = MapWriter::new(program);
    // The prelude comes before the first mark, so its segments come first in
    // generated order, as `mappings` must list them.
    if let Some(range) = runtime
        && let Some(helpers) = text.get(range.clone())
        && !helpers.is_empty()
    {
        debug_assert!(
            !text[..range.end].contains(MARK),
            "the runtime prelude precedes every mark (E257 S6)"
        );
        let (first_line, _) = line_breaks(&text.as_bytes()[..range.start]);
        writer.runtime(first_line, helpers);
    }
    for ((generated, mark), original) in placed.iter().zip(originals) {
        if let Some(original) = original {
            writer.segment(*generated, *mark, original);
        }
    }
    (clean, writer.finish())
}

/// A generated position: a 0-based line and a 0-based column in UTF-16 code
/// units, the unit a JS engine counts in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Position {
    line: u32,
    column: u32,
}

impl Position {
    /// Moves past `text`: its line breaks, then the UTF-16 units after the
    /// last of them.
    fn advance(&mut self, text: &str) {
        match line_breaks(text.as_bytes()) {
            (0, _) | (_, None) => self.column += utf16_length(text),
            (breaks, Some(last)) => {
                self.line += breaks;
                self.column = utf16_length(&text[last..]);
            }
        }
    }
}

pub(crate) fn utf16_length(text: &str) -> u32 {
    if text.is_ascii() {
        text.len() as u32
    } else {
        text.chars().map(char::len_utf16).sum::<usize>() as u32
    }
}

/// The line breaks in `bytes`: how many, and the offset just past the last
/// one. Eight bytes at a time — this is the sweep over std's text — and the
/// last break found by a scan back from the end, which stops within a line.
pub(crate) fn line_breaks(bytes: &[u8]) -> (u32, Option<usize>) {
    const LOW_SEVEN: u64 = 0x7f7f_7f7f_7f7f_7f7f;
    const HIGH: u64 = 0x8080_8080_8080_8080;
    const NEWLINES: u64 = 0x0a0a_0a0a_0a0a_0a0a;
    const ONES: u64 = 0x0101_0101_0101_0101;
    let mut count: u64 = 0;
    let (words, remainder) = bytes.as_chunks::<8>();
    for word in words {
        let word = u64::from_le_bytes(*word);
        // A byte of `differs` is zero exactly where the text has a `\n`; the
        // high bit of each byte of `found` is set exactly there (the sum of
        // two seven-bit halves cannot carry into the next byte). The
        // multiply sums the eight 0/1 bytes into the top one: a population
        // count with no `popcnt` instruction to ask for.
        let differs = word ^ NEWLINES;
        let found = !(((differs & LOW_SEVEN).wrapping_add(LOW_SEVEN)) | differs) & HIGH;
        count += ((found >> 7).wrapping_mul(ONES)) >> 56;
    }
    count += remainder.iter().filter(|byte| **byte == b'\n').count() as u64;
    if count == 0 {
        return (0, None);
    }
    let last = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map(|at| at + 1);
    (count as u32, last)
}

/// The original side of one segment, before the map numbers its source.
#[derive(Clone, Copy)]
struct Original<'src> {
    source: SourceId,
    text: &'src str,
    line: u32,
    column: u32,
}

/// Every placed mark's original place, or `None` where it has none: each
/// mark is anchored to a byte offset in its source, and each source is then
/// swept ONCE, in offset order, counting lines up to each.
fn locate<'src>(
    program: &Program<'src>,
    placed: &[(Position, SourceMark)],
) -> Vec<Option<Original<'src>>> {
    let mut texts: HashMap<SourceId, Option<&'src str>> = HashMap::default();
    let mut text_of = |source: SourceId| {
        *texts.entry(source).or_insert_with(|| {
            program
                .source_texts
                .iter()
                .find(|(candidate, _)| *candidate == source)
                .map(|(_, text)| *text)
        })
    };
    // (source, byte offset, index into `placed`).
    let mut anchored: Vec<(SourceId, usize, usize)> = Vec::with_capacity(placed.len());
    for (index, (_, mark)) in placed.iter().enumerate() {
        let Some((source, anchor)) = anchor(program, *mark) else {
            continue;
        };
        let Some(text) = text_of(source) else {
            continue;
        };
        let offset = match anchor {
            Anchor::Offset(offset) => offset,
            Anchor::Callee(call) => match program.site_offset(call, text) {
                Some(offset) => offset,
                None => continue,
            },
        };
        if offset <= text.len() && text.is_char_boundary(offset) {
            anchored.push((source, offset, index));
        }
    }
    anchored.sort_unstable();
    let mut originals = vec![None; placed.len()];
    let mut sweep: Option<(SourceId, usize, u32, usize)> = None;
    for (source, offset, index) in anchored {
        let text = text_of(source).expect("anchored in a source with text");
        // (source, swept to, lines before it, where its line starts)
        let (_, swept, mut line, mut line_start) = match sweep {
            Some(state) if state.0 == source => state,
            _ => (source, 0, 0, 0),
        };
        let (breaks, last) = line_breaks(&text.as_bytes()[swept..offset]);
        line += breaks;
        if let Some(last) = last {
            line_start = swept + last;
        }
        sweep = Some((source, offset, line, line_start));
        originals[index] = Some(Original {
            source,
            text,
            line,
            column: utf16_length(&text[line_start..offset]),
        });
    }
    originals
}

/// Builds `mappings` segment by segment, each field relative to the one
/// before it as v3 encodes them.
struct MapWriter<'program, 'src> {
    program: &'program Program<'src>,
    map: SourceMap,
    /// A program source's index in `map.sources`, assigned on first use —
    /// so `sources` lists files in the order the script first reaches them.
    source_slots: HashMap<SourceId, Option<u32>>,
    names: HashMap<&'src str, u32>,
    last_generated: Option<Position>,
    previous_generated_column: u32,
    previous_source: u32,
    previous_line: u32,
    previous_column: u32,
    previous_name: u32,
}

impl<'program, 'src> MapWriter<'program, 'src> {
    fn new(program: &'program Program<'src>) -> Self {
        Self {
            program,
            map: SourceMap::default(),
            source_slots: HashMap::default(),
            names: HashMap::default(),
            last_generated: None,
            previous_generated_column: 0,
            previous_source: 0,
            previous_line: 0,
            previous_column: 0,
            previous_name: 0,
        }
    }

    fn segment(&mut self, generated: Position, mark: SourceMark, original: Original<'src>) {
        if self.last_generated == Some(generated) {
            return;
        }
        let Some(source) = self.source_slot(original.source, original.text) else {
            return;
        };
        let program = self.program;
        let name = match mark.kind {
            MarkKind::Function => program
                .functions
                .get(&mark.id)
                .map(|function| self.name_slot(function.name)),
            _ => None,
        };
        self.write(generated, source, original.line, original.column, name);
    }

    /// The runtime helpers' text, starting on generated line `first_line`:
    /// one source of its own, and each non-empty line mapped at its start to
    /// the same line of it.
    fn runtime(&mut self, first_line: u32, helpers: &str) {
        self.map.sources.push(MapSource {
            path: PathBuf::new(),
            url: Some(RUNTIME_SOURCE),
            content: Some(helpers.to_string()),
            ignored: true,
        });
        let source = (self.map.sources.len() - 1) as u32;
        for (index, line) in helpers.split('\n').enumerate() {
            if line.is_empty() {
                continue;
            }
            let generated = Position {
                line: first_line + index as u32,
                column: 0,
            };
            self.write(generated, source, index as u32, 0, None);
        }
    }

    /// One segment, each field relative to the one before it.
    fn write(
        &mut self,
        generated: Position,
        source: u32,
        original_line: u32,
        original_column: u32,
        name: Option<u32>,
    ) {
        let line_changed = match self.last_generated {
            Some(last) => last.line != generated.line,
            None => true,
        };
        let previous_line = self.last_generated.map_or(0, |last| last.line);
        if line_changed {
            for _ in previous_line..generated.line {
                self.map.mappings.push(';');
            }
            self.previous_generated_column = 0;
        } else {
            self.map.mappings.push(',');
        }
        let mappings = &mut self.map.mappings;
        vlq(
            mappings,
            i64::from(generated.column) - i64::from(self.previous_generated_column),
        );
        vlq(
            mappings,
            i64::from(source) - i64::from(self.previous_source),
        );
        vlq(
            mappings,
            i64::from(original_line) - i64::from(self.previous_line),
        );
        vlq(
            mappings,
            i64::from(original_column) - i64::from(self.previous_column),
        );
        if let Some(name) = name {
            vlq(mappings, i64::from(name) - i64::from(self.previous_name));
            self.previous_name = name;
        }
        self.previous_generated_column = generated.column;
        self.previous_source = source;
        self.previous_line = original_line;
        self.previous_column = original_column;
        self.last_generated = Some(generated);
    }

    fn source_slot(&mut self, source: SourceId, text: &'src str) -> Option<u32> {
        if let Some(slot) = self.source_slots.get(&source) {
            return *slot;
        }
        let program = self.program;
        let index = source.0 as usize;
        let slot = program.sources.get(index).map(|spelled| {
            let path = program
                .canonical_sources
                .get(index)
                .cloned()
                .unwrap_or_else(|| spelled.clone());
            let is_std = program.std_sources.contains(&source);
            self.map.sources.push(MapSource {
                path,
                url: None,
                content: (!is_std && text.len() <= SOURCES_CONTENT_CAP).then(|| text.to_string()),
                ignored: is_std,
            });
            (self.map.sources.len() - 1) as u32
        });
        self.source_slots.insert(source, slot);
        slot
    }

    fn name_slot(&mut self, name: &'src str) -> u32 {
        if let Some(slot) = self.names.get(name) {
            return *slot;
        }
        self.map.names.push(name.to_string());
        let slot = (self.map.names.len() - 1) as u32;
        self.names.insert(name, slot);
        slot
    }

    fn finish(self) -> SourceMap {
        self.map
    }
}

/// Where in its source a mark points: a byte offset, or a call whose
/// callee's name the source text locates (`site_offset`, S0's rule).
enum Anchor {
    Offset(usize),
    Callee(Id),
}

/// The source and place a mark points at. Generated code — a derive's impl —
/// points at the attribute that produced it, as its diagnostics do.
fn anchor(program: &Program, mark: SourceMark) -> Option<(SourceId, Anchor)> {
    let source = program.source_of(mark.id)?;
    if source == DERIVED_SOURCE {
        let (span, origin) = program.derived_origin(mark.id)?;
        return Some((origin, Anchor::Offset(span.start)));
    }
    if mark.kind == MarkKind::Function {
        let function = program.functions.get(&mark.id)?;
        if function.name_span.start == function.name_span.end {
            return None;
        }
        return Some((source, Anchor::Offset(function.name_span.start)));
    }
    let span = **program.span_map.get(&mark.id)?;
    // A synthetic entity carries an empty span (`self` parameters, desugared
    // nodes): it has no text to point at.
    if span.start == span.end {
        return None;
    }
    if mark.kind == MarkKind::Call {
        return Some((source, Anchor::Callee(mark.id)));
    }
    Some((source, Anchor::Offset(span.start)))
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// One base-64 VLQ field: the sign in the lowest bit, five bits per digit,
/// the sixth bit a continuation.
fn vlq(out: &mut String, value: i64) {
    let mut rest = if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    };
    loop {
        let mut digit = (rest & 31) as usize;
        rest >>= 5;
        if rest > 0 {
            digit |= 32;
        }
        out.push(BASE64[digit] as char);
        if rest == 0 {
            break;
        }
    }
}

impl SourceMap {
    /// This map for its script with `lines` lines written in front of it —
    /// the HMR shim before a browser bundle (E257 S6): every segment moves down
    /// by that many lines and nothing else changes, since a segment's
    /// generated column is relative only within its own line.
    pub fn shift_lines(&mut self, lines: usize) {
        if lines == 0 || self.mappings.is_empty() {
            return;
        }
        let mut shifted = String::with_capacity(self.mappings.len() + lines);
        shifted.extend(std::iter::repeat_n(';', lines));
        shifted.push_str(&self.mappings);
        self.mappings = shifted;
    }

    /// The map's JSON for the generated file `file` (its name, as the
    /// `sourceMappingURL` comment beside it names the map).
    ///
    /// `map_directory` is where the map will be written. A user source is
    /// then spelled relative to it (`../src/main.vl`), so a built `dist/`
    /// keeps working when the project moves; with `None` — a script `vilan
    /// run` writes to a temp directory — or when no relative spelling exists
    /// (another drive), it is an absolute `file://` URL. A std source is
    /// always its absolute path in the std cache, which is where a debugger
    /// finds the text this map does not carry.
    pub fn to_json(&self, file: &str, map_directory: Option<&Path>) -> String {
        // A script written to the working directory names an empty parent.
        let directory = map_directory.map(|directory| {
            if directory.as_os_str().is_empty() {
                crate::util::canonical_path(".")
            } else {
                crate::util::canonical_path(directory)
            }
        });
        let mut json = String::with_capacity(
            self.mappings.len()
                + self
                    .sources
                    .iter()
                    .map(|source| source.content.as_ref().map_or(0, String::len) + 64)
                    .sum::<usize>()
                + 128,
        );
        json.push_str("{\"version\":3,\"file\":");
        json_string(&mut json, file);
        json.push_str(",\"sources\":[");
        for (index, source) in self.sources.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            let spelled = match (&directory, source.ignored, source.url) {
                (_, _, Some(url)) => url.to_string(),
                (Some(directory), false, None) => {
                    relative_url(directory, &source.path).unwrap_or_else(|| file_url(&source.path))
                }
                _ => file_url(&source.path),
            };
            json_string(&mut json, &spelled);
        }
        json.push_str("],\"sourcesContent\":[");
        for (index, source) in self.sources.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            match &source.content {
                Some(content) => json_string(&mut json, content),
                None => json.push_str("null"),
            }
        }
        json.push_str("],\"names\":[");
        for (index, name) in self.names.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json_string(&mut json, name);
        }
        // Base-64 digits, `,` and `;`: nothing in `mappings` needs escaping.
        json.push_str("],\"mappings\":\"");
        json.push_str(&self.mappings);
        json.push('"');
        let ignored: Vec<String> = self
            .sources
            .iter()
            .enumerate()
            .filter(|(_, source)| source.ignored)
            .map(|(index, _)| index.to_string())
            .collect();
        json.push_str(",\"ignoreList\":[");
        json.push_str(&ignored.join(","));
        json.push_str("]}");
        json
    }
}

/// The comment that names a map from the end of its generated file.
pub fn mapping_url_comment(map_file_name: &str) -> String {
    format!("//# sourceMappingURL={}\n", percent_encode(map_file_name))
}

pub(crate) fn json_string(out: &mut String, text: &str) {
    out.push('"');
    // Runs that need no escape are copied whole; every byte that does is
    // ASCII, so each cut falls on a character boundary.
    let mut copied = 0;
    for (at, byte) in text.bytes().enumerate() {
        let escaped = match byte {
            b'"' => "\\\"",
            b'\\' => "\\\\",
            b'\n' => "\\n",
            b'\r' => "\\r",
            b'\t' => "\\t",
            0x00..=0x1f => "",
            _ => continue,
        };
        out.push_str(&text[copied..at]);
        if escaped.is_empty() {
            out.push_str(&format!("\\u{byte:04x}"));
        } else {
            out.push_str(escaped);
        }
        copied = at + 1;
    }
    out.push_str(&text[copied..]);
    out.push('"');
}

/// `path` as a `file://` URL, with `/` separators on every host.
fn file_url(path: &Path) -> String {
    let joined = path_text(path);
    if joined.starts_with('/') {
        format!("file://{}", percent_encode(&joined))
    } else {
        // A Windows drive path (`C:/…`) takes the third slash.
        format!("file:///{}", percent_encode(&joined))
    }
}

/// `target` relative to `directory`, as a relative URL, or `None` when they
/// share no root.
fn relative_url(directory: &Path, target: &Path) -> Option<String> {
    let from: Vec<Component<'_>> = directory.components().collect();
    let to: Vec<Component<'_>> = target.components().collect();
    let shared = from
        .iter()
        .zip(to.iter())
        .take_while(|(left, right)| left == right)
        .count();
    // Nothing in common, or only a Windows prefix: there is no relative
    // spelling that survives.
    if shared == 0
        || !matches!(
            from.first(),
            Some(Component::RootDir) | Some(Component::Prefix(_))
        )
    {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    for _ in shared..from.len() {
        parts.push("..".to_string());
    }
    for component in &to[shared..] {
        parts.push(component.as_os_str().to_string_lossy().into_owned());
    }
    Some(percent_encode(&parts.join("/")))
}

fn path_text(path: &Path) -> String {
    let text = path.to_string_lossy();
    if std::path::MAIN_SEPARATOR == '\\' {
        text.replace('\\', "/")
    } else {
        text.into_owned()
    }
}

/// Escapes the characters a URL path cannot carry bare.
fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '%' => out.push_str("%25"),
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '?' => out.push_str("%3F"),
            character => out.push(character),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marks_text_is_the_mark() {
        assert!(MARK_TEXT.chars().eq([MARK]));
    }

    #[test]
    fn vlq_encodes_the_spec_examples() {
        let encode = |value| {
            let mut out = String::new();
            vlq(&mut out, value);
            out
        };
        assert_eq!(encode(0), "A");
        assert_eq!(encode(1), "C");
        assert_eq!(encode(-1), "D");
        assert_eq!(encode(15), "e");
        assert_eq!(encode(16), "gB");
        assert_eq!(encode(-16), "hB");
        assert_eq!(encode(123456), "gkxH");
    }

    #[test]
    fn a_generated_position_counts_utf16_units_and_resets_at_a_line_break() {
        let mut position = Position::default();
        position.advance("ab");
        assert_eq!((position.line, position.column), (0, 2));
        position.advance("\u{1F600}");
        assert_eq!((position.line, position.column), (0, 4));
        position.advance("x\n\nyz");
        assert_eq!((position.line, position.column), (2, 2));
        position.advance("é");
        assert_eq!((position.line, position.column), (2, 3));
    }

    #[test]
    fn line_breaks_counts_what_a_byte_scan_counts() {
        let naive = |bytes: &[u8]| {
            let count = bytes.iter().filter(|byte| **byte == b'\n').count() as u32;
            let last = bytes
                .iter()
                .rposition(|byte| *byte == b'\n')
                .map(|at| at + 1);
            (count, last)
        };
        // Breaks at every offset modulo the word, adjacent breaks, a break
        // last, the remainder, the bytes one above and below `\n`, and
        // multi-byte characters.
        let texts = [
            String::new(),
            "\n".to_string(),
            "abcdefg\nhijklmno\n".to_string(),
            "\n\n\n\n\n\n\n\n\n\n".to_string(),
            "a\tb\x0bc\nd".to_string(),
            "é\n😀\nabc\n".repeat(7),
            (0..40)
                .map(|length| format!("{}\n", "x".repeat(length)))
                .collect(),
        ];
        for text in &texts {
            for start in 0..text.len().min(9) {
                let bytes = &text.as_bytes()[start..];
                assert_eq!(line_breaks(bytes), naive(bytes), "{text:?} from {start}");
            }
        }
    }

    #[test]
    fn a_relative_source_climbs_out_of_the_map_directory() {
        assert_eq!(
            relative_url(Path::new("/p/dist"), Path::new("/p/src/main.vl")).as_deref(),
            Some("../src/main.vl")
        );
        assert_eq!(
            relative_url(Path::new("/p/src"), Path::new("/p/src/a b.vl")).as_deref(),
            Some("a%20b.vl")
        );
        assert_eq!(
            file_url(Path::new("/opt/vilan/std/src/io.vl")),
            "file:///opt/vilan/std/src/io.vl"
        );
    }

    #[test]
    fn shifting_a_map_moves_every_segment_down_and_nothing_else() {
        let mut map = SourceMap {
            mappings: "AAAA,EAAE;AACA".to_string(),
            ..SourceMap::default()
        };
        map.shift_lines(3);
        assert_eq!(map.mappings, ";;;AAAA,EAAE;AACA");
        map.shift_lines(0);
        assert_eq!(map.mappings, ";;;AAAA,EAAE;AACA");
        // A map with no segments stays empty: there is nothing to move.
        let mut empty = SourceMap::default();
        empty.shift_lines(5);
        assert!(empty.mappings.is_empty());
    }

    #[test]
    fn a_synthetic_source_is_spelled_by_its_url() {
        let map = SourceMap {
            sources: vec![MapSource {
                path: PathBuf::new(),
                url: Some(RUNTIME_SOURCE),
                content: Some("function __at() {}".to_string()),
                ignored: true,
            }],
            ..SourceMap::default()
        };
        for directory in [None, Some(Path::new("/p/dist"))] {
            let json = map.to_json("main.mjs", directory);
            assert!(
                json.contains("\"sources\":[\"vilan:///runtime.js\"]")
                    && json.contains("\"ignoreList\":[0]"),
                "{json}"
            );
        }
    }

    #[test]
    fn json_strings_escape_what_json_requires() {
        let mut out = String::new();
        json_string(&mut out, "a\"b\\c\nd\u{1}");
        assert_eq!(out, "\"a\\\"b\\\\c\\nd\\u0001\"");
    }
}
