//! E257 S5 (debugging.md §5.1): the JS backend's source maps.
//!
//! Two claims, and a gate for each.
//!
//! **The map costs the JavaScript nothing.** `transform_with_source_map`
//! walks with marks in the tree and prints a mark character at each, and the
//! strip pass takes the characters out again; what is left must be the bytes
//! `transform` writes, or a debug build (where maps are on by default) would
//! run a different program than the one the corpus goldens pin. Every corpus
//! program is built both ways and compared byte for byte — one test per
//! program, through the corpus roster the differentials share.
//!
//! **The map is a valid map that points into real text.** Each map is
//! decoded here by a decoder of this file's own (not the writer's), every
//! segment is held to its generated line and to its source's text, and the
//! decoded segments re-encode to the same `mappings` string. The construct
//! pins below then read the map the way a debugger does — a generated
//! position in, a `.vl` position out — for a function head and its name, a
//! statement, a call, a closure, both instances of a generic, a derive's
//! generated code and a std frame.

use std::path::{Path, PathBuf};

use vilan_core::options::BuildOptions;
use vilan_core::source_map::{RUNTIME_SOURCE, SOURCES_CONTENT_CAP, SourceMap};
use vilan_core::{
    PackageSpec, Platform, Workspace, analyze_source, transform, transform_with_source_map,
};

mod scratch;
#[macro_use]
mod corpus_harness;
use corpus_harness::{assert_the_declaration_is_the_corpus, corpus_dir};

fn std_spec() -> PackageSpec {
    vilan_core::manifest::resolve_std(
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vilan/std"),
    )
}

/// One program built without a map and with one, off the same analysis.
struct Built {
    plain: String,
    mapped: String,
    map: SourceMap,
}

fn build(source: String, root: PathBuf, entry: PathBuf) -> Result<Built, String> {
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &root,
                &entry,
                Some(Platform::default()),
                &Workspace::default(),
            );
            let program = match program {
                Some(program) if errors.is_empty() => program,
                _ => return Err(format!("compile failed: {errors:?}")),
            };
            let options = BuildOptions::default();
            assert!(options.source_maps, "the debug preset writes maps (Q9)");
            let plain = transform(&program, &options).map_err(|error| error.msg)?;
            let (mapped, map) =
                transform_with_source_map(&program, &options).map_err(|error| error.msg)?;
            let map = map.ok_or("the debug preset wrote no map")?;
            Ok(Built { plain, mapped, map })
        })
        .expect("spawn worker")
        .join()
        .unwrap_or_else(|_| Err("worker thread aborted".to_string()))
}

/// One decoded segment: a generated position and, when it maps, the original
/// one — all 0-based, columns in UTF-16 units.
#[derive(Clone, Debug, PartialEq)]
struct Segment {
    generated_line: usize,
    generated_column: i64,
    source: i64,
    line: i64,
    column: i64,
    name: Option<i64>,
}

fn decode_vlq(text: &str, at: &mut usize) -> i64 {
    const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = text.as_bytes();
    let mut value: i64 = 0;
    let mut shift = 0;
    loop {
        let digit = BASE64
            .iter()
            .position(|candidate| *candidate == bytes[*at])
            .unwrap_or_else(|| panic!("not a base-64 digit at {}: {:?}", *at, bytes[*at] as char))
            as i64;
        *at += 1;
        value += (digit & 31) << shift;
        shift += 5;
        if digit & 32 == 0 {
            break;
        }
    }
    if value & 1 == 1 {
        -(value >> 1)
    } else {
        value >> 1
    }
}

/// A plain v3 decoder: `;` ends a generated line, `,` a segment, and every
/// field but the generated column carries over from the previous segment.
fn decode(mappings: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let (mut source, mut line, mut column, mut name) = (0i64, 0i64, 0i64, 0i64);
    for (generated_line, text) in mappings.split(';').enumerate() {
        let mut generated_column = 0i64;
        for segment in text.split(',').filter(|segment| !segment.is_empty()) {
            let mut at = 0;
            let mut fields = Vec::new();
            while at < segment.len() {
                fields.push(decode_vlq(segment, &mut at));
            }
            assert!(
                fields.len() == 4 || fields.len() == 5,
                "a segment carries 4 or 5 fields (every mark maps): {segment:?} -> {fields:?}"
            );
            generated_column += fields[0];
            source += fields[1];
            line += fields[2];
            column += fields[3];
            let segment_name = fields.get(4).map(|delta| {
                name += delta;
                name
            });
            segments.push(Segment {
                generated_line,
                generated_column,
                source,
                line,
                column,
                name: segment_name,
            });
        }
    }
    segments
}

fn encode_vlq(out: &mut String, value: i64) {
    const BASE64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut rest = if value < 0 {
        ((-value) << 1) | 1
    } else {
        value << 1
    };
    loop {
        let mut digit = rest & 31;
        rest >>= 5;
        if rest > 0 {
            digit |= 32;
        }
        out.push(BASE64[digit as usize] as char);
        if rest == 0 {
            break;
        }
    }
}

/// The decoder's inverse, for the round trip.
fn encode(segments: &[Segment]) -> String {
    let mut out = String::new();
    let (mut source, mut line, mut column, mut name) = (0i64, 0i64, 0i64, 0i64);
    let mut current_line = 0;
    let mut generated_column = 0i64;
    let mut first_on_line = true;
    for segment in segments {
        while current_line < segment.generated_line {
            out.push(';');
            current_line += 1;
            generated_column = 0;
            first_on_line = true;
        }
        if !first_on_line {
            out.push(',');
        }
        first_on_line = false;
        encode_vlq(&mut out, segment.generated_column - generated_column);
        encode_vlq(&mut out, segment.source - source);
        encode_vlq(&mut out, segment.line - line);
        encode_vlq(&mut out, segment.column - column);
        if let Some(segment_name) = segment.name {
            encode_vlq(&mut out, segment_name - name);
            name = segment_name;
        }
        generated_column = segment.generated_column;
        source = segment.source;
        line = segment.line;
        column = segment.column;
    }
    out
}

fn utf16_length(text: &str) -> i64 {
    text.chars().map(char::len_utf16).sum::<usize>() as i64
}

/// The text of every source the map names: the map's own for a user file,
/// the file on disk for std (which the map names by path and does not carry).
fn source_texts(map: &SourceMap) -> Vec<String> {
    map.sources
        .iter()
        .map(|source| match &source.content {
            Some(content) => content.clone(),
            None => vilan_core::util::read_source(&source.path)
                .unwrap_or_else(|error| panic!("{}: {error}", source.path.display())),
        })
        .collect()
}

/// The validity gate one map is held to.
fn assert_valid(javascript: &str, map: &SourceMap) -> Vec<Segment> {
    let segments = decode(&map.mappings);
    assert_eq!(
        encode(&segments),
        map.mappings,
        "the mappings do not survive a decode/encode round trip"
    );
    let generated_lines: Vec<&str> = javascript.split('\n').collect();
    let texts = source_texts(map);
    let mut last: Option<(usize, i64)> = None;
    for segment in &segments {
        // Ordered, and never two at one place.
        if let Some((line, column)) = last
            && line == segment.generated_line
        {
            assert!(
                segment.generated_column > column,
                "segments out of order on generated line {line}: {segment:?}"
            );
        }
        last = Some((segment.generated_line, segment.generated_column));
        let generated = generated_lines
            .get(segment.generated_line)
            .unwrap_or_else(|| panic!("generated line past the output: {segment:?}"));
        assert!(
            segment.generated_column >= 0 && segment.generated_column < utf16_length(generated),
            "generated column past its line: {segment:?} on {generated:?}"
        );
        let text = texts
            .get(segment.source as usize)
            .unwrap_or_else(|| panic!("source index past `sources`: {segment:?}"));
        let original = text
            .split('\n')
            .nth(segment.line as usize)
            .unwrap_or_else(|| panic!("original line past its source: {segment:?}"));
        assert!(
            segment.column >= 0 && segment.column <= utf16_length(original),
            "original column past its line: {segment:?} on {original:?}"
        );
        if let Some(name) = segment.name {
            assert!(
                (name as usize) < map.names.len(),
                "name index past `names`: {segment:?}"
            );
        }
    }
    // The JSON a debugger reads: valid, and saying what the struct says.
    let json: serde_json::Value =
        serde_json::from_str(&map.to_json("out.mjs", None)).expect("the map is valid JSON");
    assert_eq!(json["version"], 3);
    assert_eq!(json["mappings"], map.mappings.as_str());
    let sources = json["sources"].as_array().expect("`sources`");
    let contents = json["sourcesContent"].as_array().expect("`sourcesContent`");
    assert_eq!(sources.len(), map.sources.len());
    assert_eq!(contents.len(), map.sources.len());
    let ignored: Vec<u64> = json["ignoreList"]
        .as_array()
        .expect("`ignoreList`")
        .iter()
        .map(|index| index.as_u64().expect("an index"))
        .collect();
    for (index, source) in map.sources.iter().enumerate() {
        assert_eq!(ignored.contains(&(index as u64)), source.ignored);
        assert_eq!(contents[index].is_null(), source.content.is_none());
        if let Some(url) = source.url {
            // The runtime helpers (S6): a source of their own, carried and
            // ignored, named by its synthetic URL.
            assert_eq!(url, RUNTIME_SOURCE);
            assert!(source.ignored && source.content.is_some(), "{source:?}");
            assert_eq!(sources[index], RUNTIME_SOURCE);
            continue;
        }
        if source.ignored {
            // std by its path, never its text.
            assert!(source.content.is_none(), "{source:?}");
        } else {
            // The user's files carry theirs, up to the cap (S6).
            let length = std::fs::metadata(&source.path).map_or(0, |meta| meta.len() as usize);
            assert_eq!(
                source.content.is_none(),
                length > SOURCES_CONTENT_CAP,
                "{}: {length} bytes",
                source.path.display()
            );
        }
        assert!(
            sources[index]
                .as_str()
                .is_some_and(|spelled| spelled.starts_with("file://")),
            "with no map directory every source is a file URL: {}",
            sources[index]
        );
    }
    segments
}

/// One corpus program: the same bytes either way, and a valid map.
fn the_map_is_neutral_and_valid_on(program: &str) {
    let corpus = corpus_dir();
    let path = corpus.join(program);
    let source = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    let built = build(source, corpus, path).unwrap_or_else(|error| panic!("{error}"));
    assert!(
        built.plain == built.mapped,
        "THE SOURCE MAP CHANGED THE JAVASCRIPT of {program}: the marked walk must print \
         the bytes the plain walk does"
    );
    let segments = assert_valid(&built.mapped, &built.map);
    assert!(!segments.is_empty(), "{program}: a program maps somewhere");
}

macro_rules! corpus_programs {
    ($($module:ident => $file:literal,)*) => {
        const DECLARED: &[(&str, &str)] = &[$((stringify!($module), $file),)*];

        $(
            mod $module {
                #[test]
                fn maps_without_changing_its_javascript() {
                    super::the_map_is_neutral_and_valid_on($file);
                }
            }
        )*
    };
}

corpus_manifest!(corpus_programs);

#[test]
fn every_corpus_program_has_a_map_test_of_its_own() {
    assert_the_declaration_is_the_corpus(DECLARED);
}

// --- The constructs, read the way a debugger reads them. ---------------------

/// A built program, written to its own file so the map names a real path.
fn build_program(name: &str, source: &str) -> (Built, String) {
    let directory = scratch::root().join(format!("source_maps_{name}_{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("scratch directory");
    let entry = directory.join("main.vl");
    std::fs::write(&entry, source).expect("write the program");
    let built = build(source.to_string(), directory.clone(), entry)
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(
        built.plain == built.mapped,
        "the map changed the JavaScript"
    );
    (built, source.to_string())
}

/// The generated (line, column) of the first occurrence of `needle` in the
/// output, or of its `occurrence`-th one.
fn generated_position(javascript: &str, needle: &str, occurrence: usize) -> (usize, i64) {
    let mut from = 0;
    let mut found = None;
    for _ in 0..=occurrence {
        let at = javascript[from..]
            .find(needle)
            .unwrap_or_else(|| panic!("`{needle}` not in the output:\n{javascript}"))
            + from;
        found = Some(at);
        from = at + needle.len();
    }
    let at = found.expect("an occurrence");
    let line = javascript[..at].matches('\n').count();
    let line_start = javascript[..at]
        .rfind('\n')
        .map_or(0, |newline| newline + 1);
    (line, utf16_length(&javascript[line_start..at]))
}

/// The 0-based (line, column) of `needle` in a source text.
fn source_position(text: &str, needle: &str) -> (i64, i64) {
    let at = text
        .find(needle)
        .unwrap_or_else(|| panic!("`{needle}` not in the source"));
    let line = text[..at].matches('\n').count() as i64;
    let line_start = text[..at].rfind('\n').map_or(0, |newline| newline + 1);
    (line, utf16_length(&text[line_start..at]))
}

/// The segment that starts exactly at a generated position.
fn segment_at(segments: &[Segment], (line, column): (usize, i64)) -> &Segment {
    segments
        .iter()
        .find(|segment| segment.generated_line == line && segment.generated_column == column)
        .unwrap_or_else(|| panic!("no segment at generated {line}:{column}: {segments:#?}"))
}

const CONSTRUCTS: &str = r#"
import std::io::print;

struct Point {
    x: i32,
    y: i32,
}

fun area(point: Point): i32 {
    point.x * point.y
}

fun total<T>(items: List<T>, measure: |T| i32): i32 {
    mut sum = 0;
    for item in items {
        let part = measure(item);
        sum = sum + part;
    }
    sum
}

fun main() {
    let points = [Point { x = 1, y = 2 }, Point { x = 3, y = 4 }];
    let result = total(points, |point| area(point));
    print(result);
}
"#;

#[test]
fn a_function_head_maps_to_its_name_and_carries_it() {
    let (built, source) = build_program("head", CONSTRUCTS);
    let segments = assert_valid(&built.mapped, &built.map);
    let head = segment_at(
        &segments,
        generated_position(&built.mapped, "function area(", 0),
    );
    assert_eq!(
        (head.line, head.column),
        source_position(&source, "area(point: Point)")
    );
    let name = head.name.expect("a function head carries its name");
    assert_eq!(built.map.names[name as usize], "area");
}

#[test]
fn a_statement_and_a_tail_map_to_their_lines() {
    let (built, source) = build_program("statement", CONSTRUCTS);
    let segments = assert_valid(&built.mapped, &built.map);
    let statement = segment_at(
        &segments,
        generated_position(&built.mapped, "const part = ", 0),
    );
    assert_eq!(
        (statement.line, statement.column),
        source_position(&source, "let part = measure(item);")
    );
    let tail = segment_at(
        &segments,
        generated_position(&built.mapped, "return point[0] * point[1];", 0),
    );
    assert_eq!(
        (tail.line, tail.column),
        source_position(&source, "point.x * point.y")
    );
    // And the source the segments point into is the user's file, carried.
    let source_entry = &built.map.sources[statement.source as usize];
    assert!(source_entry.path.ends_with("main.vl"), "{source_entry:?}");
    assert_eq!(source_entry.content.as_deref(), Some(source.as_str()));
}

#[test]
fn a_call_maps_to_its_callees_name() {
    let (built, source) = build_program("call", CONSTRUCTS);
    let segments = assert_valid(&built.mapped, &built.map);
    // `total(points, …)` is not where its statement starts, so its own
    // segment is what places it.
    let call = segment_at(
        &segments,
        generated_position(&built.mapped, "total(points", 0),
    );
    assert_eq!(
        (call.line, call.column),
        source_position(&source, "total(points, |point|")
    );
}

#[test]
fn a_closure_head_maps_to_its_bars() {
    let (built, source) = build_program("closure", CONSTRUCTS);
    let segments = assert_valid(&built.mapped, &built.map);
    let closure = segment_at(
        &segments,
        generated_position(&built.mapped, "(point) => {", 0),
    );
    assert_eq!(
        (closure.line, closure.column),
        source_position(&source, "|point| area(point)")
    );
}

#[test]
fn both_instances_of_a_generic_map_to_its_one_body() {
    let source = r#"
import std::io::print;

trait Shape {
    fun size(self): i32;
}

struct Square {
    side: i32,
}

struct Line {
    length: i32,
}

impl Square with Shape {
    fun size(self): i32 {
        self.side * self.side
    }
}

impl Line with Shape {
    fun size(self): i32 {
        self.length
    }
}

fun doubled<T: Shape>(shape: T): i32 {
    let size = shape.size();
    size * 2
}

fun main() {
    print(doubled(Square { side = 3 }));
    print(doubled(Line { length = 4 }));
}
"#;
    let (built, source) = build_program("instances", source);
    let segments = assert_valid(&built.mapped, &built.map);
    let expected = source_position(&source, "let size = shape.size();");
    let statements: Vec<&Segment> = segments
        .iter()
        .filter(|segment| (segment.line, segment.column) == expected)
        .collect();
    assert_eq!(
        statements.len(),
        2,
        "two emitted instances, each statement mapped to the one source line:\n{}",
        built.mapped
    );
    let heads: Vec<&Segment> = segments
        .iter()
        .filter(|segment| {
            segment
                .name
                .is_some_and(|name| built.map.names[name as usize] == "doubled")
        })
        .collect();
    assert_eq!(heads.len(), 2, "both instances name `doubled`");
}

#[test]
fn a_derives_generated_code_maps_to_its_attribute() {
    let source = r#"
import std::debug::Debug;
import std::io::print;

[derive(Debug)]
struct Point {
    x: i32,
    y: i32,
}

fun main() {
    let point = Point { x = 1, y = 2 };
    print(point.debug());
}
"#;
    let (built, source) = build_program("derive", source);
    let segments = assert_valid(&built.mapped, &built.map);
    let attribute = source_position(&source, "derive(Debug)");
    let user = built
        .map
        .sources
        .iter()
        .position(|entry| entry.path.ends_with("main.vl"))
        .expect("the user's file") as i64;
    // The attribute's line holds the generated impl's segments, and nothing
    // the derive generated lands anywhere else: there is no other text for it.
    assert!(
        segments
            .iter()
            .any(|segment| segment.source == user && segment.line == attribute.0),
        "the derive's code maps to the attribute's line:\n{}\n{segments:#?}",
        built.mapped
    );
}

#[test]
fn a_std_frame_names_std_by_its_path_and_ignores_it() {
    let source = r#"
import std::display::Display;
import std::io::print;

fun main() {
    let words = ["b", "a", "c"];
    print(words.join("-"));
}
"#;
    let (built, _) = build_program("std", source);
    let segments = assert_valid(&built.mapped, &built.map);
    let std_sources: Vec<usize> = built
        .map
        .sources
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.ignored)
        .map(|(index, _)| index)
        .collect();
    assert!(
        !std_sources.is_empty(),
        "a program that emits std code maps into std:\n{}",
        built.mapped
    );
    for index in &std_sources {
        let entry = &built.map.sources[*index];
        assert!(entry.content.is_none(), "std's text is not carried");
        assert!(entry.path.is_absolute() && entry.path.exists(), "{entry:?}");
        assert!(
            segments
                .iter()
                .any(|segment| segment.source == *index as i64),
            "an ignored source some segment points into"
        );
    }
}

#[test]
fn a_release_build_writes_no_map_unless_asked() {
    let corpus = corpus_dir();
    let path = corpus.join("bool.vl");
    let source = std::fs::read_to_string(&path).expect("bool.vl");
    let outcome = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || {
            let leaked: &'static str = Box::leak(source.into_boxed_str());
            let (program, errors) = analyze_source(
                leaked,
                &std_spec(),
                &corpus,
                &path,
                Some(Platform::default()),
                &Workspace::default(),
            );
            assert!(errors.is_empty(), "{errors:?}");
            let program = program.expect("a program");
            let mut options = BuildOptions::from_preset(vilan_core::options::Preset::Release);
            assert!(
                !options.source_maps,
                "release writes no map by default (Q9)"
            );
            let (release, map) = transform_with_source_map(&program, &options).expect("it builds");
            assert!(map.is_none());
            assert_eq!(release, transform(&program, &options).expect("it builds"));
            // `[build] source-maps = true` under release: the minified output
            // is still the same bytes, and it maps.
            options.source_maps = true;
            let (mapped, map) = transform_with_source_map(&program, &options).expect("it builds");
            assert_eq!(mapped, release);
            let map = map.expect("asked for");
            assert_valid(&mapped, &map);
        })
        .expect("spawn")
        .join();
    assert!(outcome.is_ok(), "the release leg panicked");
}

#[test]
fn a_map_directory_spells_the_users_files_relative_and_std_absolute() {
    let (built, _) = build_program("relative", CONSTRUCTS);
    let entry = built
        .map
        .sources
        .iter()
        .find(|entry| entry.path.ends_with("main.vl"))
        .expect("the user's file")
        .path
        .clone();
    let dist = entry.parent().expect("a directory").join("dist");
    let json: serde_json::Value =
        serde_json::from_str(&built.map.to_json("main.mjs", Some(Path::new(&dist))))
            .expect("valid JSON");
    assert_eq!(json["file"], "main.mjs");
    let sources: Vec<&str> = json["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .map(|source| source.as_str().expect("a string"))
        .collect();
    assert!(sources.contains(&"../main.vl"), "{sources:?}");
    for (index, source) in built.map.sources.iter().enumerate() {
        if source.ignored {
            assert!(sources[index].starts_with("file://"), "{sources:?}");
        }
    }
}

/// E257 S6: the runtime helpers the script opens with (`__at`, `__panic`)
/// map into a synthetic source of their own — carried, ignore-listed, each
/// generated line on the same line of the carried text — so a frame inside
/// one names it and a debugger's "step into" passes over it.
#[test]
fn the_runtime_helpers_map_into_an_ignored_source_of_their_own() {
    let source = r#"
import std::io::print;

fun pick(rows: List<i32>, at: usize): i32 {
    rows[at]
}

fun main() {
    print(pick([1, 2, 3], 1));
}
"#;
    let (built, _) = build_program("runtime", source);
    let segments = assert_valid(&built.mapped, &built.map);
    let (index, runtime) = built
        .map
        .sources
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.url == Some(RUNTIME_SOURCE))
        .unwrap_or_else(|| panic!("a runtime source:\n{}", built.mapped));
    assert!(runtime.ignored);
    let carried = runtime.content.as_deref().expect("the helpers' text");
    assert!(
        carried.starts_with("function __at(") && carried.contains("function __panic("),
        "{carried}"
    );
    // The carried text IS the script's opening lines.
    assert!(built.mapped.starts_with(carried), "{}", built.mapped);
    let generated: Vec<&str> = built.mapped.split('\n').collect();
    let original: Vec<&str> = carried.split('\n').collect();
    let into_runtime: Vec<&Segment> = segments
        .iter()
        .filter(|segment| segment.source == index as i64)
        .collect();
    assert_eq!(
        into_runtime.len(),
        original.iter().filter(|line| !line.is_empty()).count(),
        "one segment per helper line"
    );
    for segment in into_runtime {
        assert_eq!(segment.generated_column, 0);
        assert_eq!(segment.column, 0);
        assert_eq!(
            generated[segment.generated_line], original[segment.line as usize],
            "{segment:?}"
        );
    }
    // The user's code starts after them: `pick` still maps to `main.vl`.
    let head = generated_position(&built.mapped, "function pick", 0);
    let user = built
        .map
        .sources
        .iter()
        .position(|entry| entry.path.ends_with("main.vl"))
        .expect("the user's file") as i64;
    assert!(
        segments
            .iter()
            .any(|segment| segment.generated_line == head.0 && segment.source == user)
    );
}

/// E257 S6, debug-51's Q1 as ruled: a user file past
/// [`SOURCES_CONTENT_CAP`] goes by its path, relative to the map, and its
/// text is not carried — kolt's generated icon table made its client's map
/// 978 KB. The file still maps: a debugger reads it from disk.
#[test]
fn a_user_file_past_the_cap_goes_by_its_relative_path() {
    let mut source =
        String::from("import std::io::print;\n\nfun main() {\n    print(\"big\");\n}\n");
    let filler = format!("// {}\n", "x".repeat(96));
    while source.len() <= SOURCES_CONTENT_CAP {
        source.push_str(&filler);
    }
    let (built, _) = build_program("cap", &source);
    assert_valid(&built.mapped, &built.map);
    let entry = built
        .map
        .sources
        .iter()
        .find(|entry| entry.path.ends_with("main.vl"))
        .expect("the user's file");
    assert!(!entry.ignored, "a user file is never ignore-listed");
    assert!(
        entry.content.is_none(),
        "past the cap, the text stays on disk"
    );
    let dist = entry.path.parent().expect("a directory").join("dist");
    let json: serde_json::Value =
        serde_json::from_str(&built.map.to_json("main.mjs", Some(Path::new(&dist))))
            .expect("valid JSON");
    let spelled: Vec<&str> = json["sources"]
        .as_array()
        .expect("sources")
        .iter()
        .map(|source| source.as_str().expect("a string"))
        .collect();
    let at = spelled
        .iter()
        .position(|spelled| *spelled == "../main.vl")
        .unwrap_or_else(|| panic!("the relative path: {spelled:?}"));
    assert!(json["sourcesContent"][at].is_null());

    // At the cap exactly, it is still carried.
    let mut source =
        String::from("import std::io::print;\n\nfun main() {\n    print(\"small\");\n}\n");
    while source.len() + filler.len() <= SOURCES_CONTENT_CAP {
        source.push_str(&filler);
    }
    source.push_str(&"/".repeat(SOURCES_CONTENT_CAP - source.len()));
    assert_eq!(source.len(), SOURCES_CONTENT_CAP);
    let (built, _) = build_program("at_cap", &source);
    let entry = built
        .map
        .sources
        .iter()
        .find(|entry| entry.path.ends_with("main.vl"))
        .expect("the user's file");
    assert_eq!(entry.content.as_deref(), Some(source.as_str()));
}

/// The marks cost the JS tree no width: `js::Node` is as large as its
/// largest variant, `js::Function`, and a function head's `origin` is
/// packed into the padding beside `is_async`. Eight bytes more made every
/// node of every build — and every `vilan check`, which walks the same
/// tree — measurably slower (+0.06% instructions on reactive-ui).
#[test]
fn the_marks_leave_a_js_node_as_wide_as_it_was() {
    assert_eq!(
        std::mem::size_of::<vilan_core::transformer::js::Node<'static>>(),
        80
    );
}

// --- E257 S7: the `.vdbg.json` sidecar (debugging.md §5.3, door D1). --------

use vilan_core::debug_info::{BindingRead, DebugBinding, DebugFrame, DebugInfo, FrameKind, Layout};

/// The sidecar a built program's map carries.
fn sidecar(built: &Built) -> &DebugInfo {
    built
        .map
        .debug
        .as_deref()
        .expect("a map carries its sidecar (E257 S7)")
}

fn frame<'info>(info: &'info DebugInfo, name: &str) -> &'info DebugFrame {
    let found: Vec<&DebugFrame> = info
        .frames
        .iter()
        .filter(|frame| frame.name == name)
        .collect();
    assert_eq!(
        found.len(),
        1,
        "one frame named `{name}`: {:#?}",
        info.frames
    );
    found[0]
}

fn binding<'frame>(frame: &'frame DebugFrame, name: &str) -> &'frame DebugBinding {
    frame
        .bindings
        .iter()
        .find(|binding| binding.name == name)
        .unwrap_or_else(|| panic!("`{name}` in {frame:#?}"))
}

/// A binding's type, as the table spells it.
fn type_text(info: &DebugInfo, binding: &DebugBinding) -> String {
    let index = binding
        .type_index
        .unwrap_or_else(|| panic!("`{}` has a type: {binding:?}", binding.name));
    info.types[index as usize].text.clone()
}

fn layout_of<'info>(info: &'info DebugInfo, text: &str) -> &'info Layout {
    &info
        .types
        .iter()
        .find(|type_| type_.text == text)
        .unwrap_or_else(|| panic!("`{text}` in the table: {:#?}", info.types))
        .layout
}

/// The JS function a sidecar frame names, declared in the script.
fn declares(javascript: &str, name: &str) -> bool {
    javascript.contains(&format!("function {name}("))
}

const VALUES: &str = r#"
import std::hash_map::HashMap;
import std::io::print;
import std::reactive::{ Signal, SignalCell };

struct Point {
    x: i32,
    y: i32,
}

enum Shape {
    Circle(f64),
    Rect(Point, Point),
    Empty,
}

fun area(shape: Shape): f64 {
    let scale = 2.0;
    match shape {
        Shape::Circle(let radius) => radius * radius * scale,
        _ => 0.0,
    }
}

fun scaled(factor: i32): |i32| i32 {
    |value: i32| {
        let total = value * factor;
        total
    }
}

fun main() {
    let point = Point { x = 1, y = 2 };
    let numbers = [1, 2, 3];
    mut ages: HashMap<str, i32> = HashMap::new();
    ages.insert("ada", 36);
    let maybe: Option<i32> = Some(5);
    let outcome: Result<i32, str> = Err("no");
    let count: SignalCell<i32> = Signal::new(3);
    let pair = (1, "two");
    let triple = scaled(3);
    print(area(Shape::Circle(1.5)));
    print(triple(point.x));
    print(numbers[0]);
    print(ages.len());
    print(maybe);
    print(outcome);
    print(count.get());
    print(pair.0);
}
"#;

/// Every user function is a frame, keyed as a JavaScript frame finds it —
/// the emitted name, and the head's place in the map's coordinates — with its
/// parameters and locals by vilan name, the name each got in the script, and
/// a type whose layout is the printer's.
#[test]
fn s7_the_sidecar_lists_each_user_function_with_its_bindings_by_static_type() {
    let (built, source) = build_program("sidecar_values", VALUES);
    let info = sidecar(&built);

    let area = frame(info, "area");
    assert_eq!(area.kind, FrameKind::Function);
    let js = area
        .js
        .as_deref()
        .expect("a function frame names its function");
    assert!(
        declares(&built.mapped, js),
        "`{js}` is declared:\n{}",
        built.mapped
    );
    let (line, column) = source_position(&source, "area(shape: Shape)");
    let entry = built
        .map
        .sources
        .iter()
        .position(|entry| entry.path.ends_with("main.vl"))
        .expect("the user's file") as u32;
    assert_eq!(
        area.at,
        Some((entry, line as u32, column as u32)),
        "at the function's name"
    );
    let names: Vec<&str> = area
        .bindings
        .iter()
        .map(|binding| binding.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["shape", "scale", "radius"],
        "parameters, then locals in order"
    );
    assert!(binding(area, "shape").parameter && !binding(area, "scale").parameter);
    for row in &area.bindings {
        assert!(
            built.mapped.contains(&format!("{} ", row.js))
                || built.mapped.contains(&format!("{},", row.js))
                || built.mapped.contains(&format!("{})", row.js)),
            "`{}` is bound in the script as `{}`",
            row.name,
            row.js
        );
    }
    assert_eq!(type_text(info, binding(area, "shape")), "Shape");
    assert_eq!(type_text(info, binding(area, "scale")), "f64");

    // The closure is a frame of its own, at its bars; its captures are its
    // enclosing function's.
    let closure = frame(info, "a closure in scaled");
    assert_eq!(closure.kind, FrameKind::Closure);
    assert_eq!(closure.js, None);
    let (line, column) = source_position(&source, "|value: i32| {");
    assert_eq!(closure.at, Some((entry, line as u32, column as u32)));
    let names: Vec<&str> = closure
        .bindings
        .iter()
        .map(|binding| binding.name.as_str())
        .collect();
    assert_eq!(names, ["value", "total"]);
    let scaled = frame(info, "scaled");
    let names: Vec<&str> = scaled
        .bindings
        .iter()
        .map(|binding| binding.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["factor"],
        "the closure's bindings are not its enclosing function's"
    );

    // `main` runs at the top level: no name, no head.
    let main = frame(info, "main");
    assert_eq!(
        (main.kind, main.js.as_deref(), main.at),
        (FrameKind::Main, None, None)
    );
    for (name, text) in [
        ("point", "Point"),
        ("numbers", "List<i32>"),
        ("ages", "HashMap<str, i32>"),
        ("maybe", "Option<i32>"),
        ("outcome", "Result<i32, str>"),
        ("count", "SignalCell<i32>"),
        ("pair", "(i32, str)"),
        ("triple", "|i32| i32"),
    ] {
        assert_eq!(type_text(info, binding(main, name)), text, "`{name}`");
    }

    // The layouts are the printer's.
    let i32_index = info
        .types
        .iter()
        .position(|type_| type_.text == "i32")
        .expect("i32") as u32;
    let str_index = info
        .types
        .iter()
        .position(|type_| type_.text == "str")
        .expect("str") as u32;
    assert_eq!(
        layout_of(info, "Point"),
        &Layout::Struct {
            name: "Point".to_string(),
            fields: vec![("x".to_string(), i32_index), ("y".to_string(), i32_index)],
        }
    );
    let Layout::Enum { variants } = layout_of(info, "Shape") else {
        panic!("Shape is an enum layout");
    };
    let labels: Vec<&str> = variants.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(
        labels,
        ["Shape::Circle", "Shape::Rect", "Shape::Empty"],
        "qualified"
    );
    let Layout::Enum { variants } = layout_of(info, "Option<i32>") else {
        panic!("Option is an enum layout");
    };
    assert_eq!(
        variants,
        &vec![
            ("Some".to_string(), vec![i32_index]),
            ("None".to_string(), vec![])
        ]
    );
    assert_eq!(
        layout_of(info, "List<i32>"),
        &Layout::List {
            element: i32_index,
            fill: true
        }
    );
    assert_eq!(
        layout_of(info, "(i32, str)"),
        &Layout::Tuple(vec![
            (String::new(), i32_index, 1),
            (String::new(), str_index, 1)
        ])
    );
    assert!(matches!(
        layout_of(info, "HashMap<str, i32>"),
        Layout::Map { label, key, value, key_width: 1, value_width: 1, .. }
            if label == "HashMap" && *key == str_index && *value == i32_index
    ));
    assert!(matches!(
        layout_of(info, "SignalCell<i32>"),
        Layout::Cell { label, value, .. } if label == "SignalCell" && *value == i32_index
    ));
    assert_eq!(
        layout_of(info, "|i32| i32"),
        &Layout::Text("<closure |i32| i32>".to_string())
    );

    // Only the user's file: std's functions are ignore-listed, never a frame.
    assert!(
        info.frames
            .iter()
            .all(|frame| frame.at.is_none_or(|(source, _, _)| source == entry)),
        "{:#?}",
        info.frames
    );
}

/// Two instances whose bodies print the same share one JavaScript function
/// (M16): its frame keeps a binding's type only where the instances agree, so
/// a pane never prints a `str` as an `i32`.
#[test]
fn s7_a_shared_generic_body_keeps_only_the_types_its_instances_agree_on() {
    let source = r#"
import std::io::print;

fun first<T>(items: List<T>): Option<T> {
    let count = items.len();
    if count == 0 { None } else { Some(items[0]) }
}

fun main() {
    print(first([7, 8]));
    print(first(["a"]));
}
"#;
    let (built, _) = build_program("sidecar_shared", source);
    let info = sidecar(&built);
    let first = frame(info, "first");
    assert_eq!(
        built.mapped.matches("function first").count(),
        1,
        "the two instances share one body:\n{}",
        built.mapped
    );
    assert_eq!(
        binding(first, "items").type_index,
        None,
        "List<i32> and List<str> disagree"
    );
    assert_eq!(
        type_text(info, binding(first, "count")),
        "usize",
        "both agree on `usize`"
    );
}

/// Two instances whose bodies differ are two JavaScript functions, and each
/// frame carries its own instance's types.
#[test]
fn s7_each_distinct_instance_is_a_frame_of_its_own() {
    let source = r#"
import std::io::print;

trait Shape {
    fun size(self): i32;
}

struct Square {
    side: i32,
}

struct Line {
    length: i32,
}

impl Square with Shape {
    fun size(self): i32 {
        self.side * self.side
    }
}

impl Line with Shape {
    fun size(self): i32 {
        self.length
    }
}

fun doubled<T: Shape>(shape: T): i32 {
    let size = shape.size();
    size * 2
}

fun main() {
    print(doubled(Square { side = 3 }));
    print(doubled(Line { length = 4 }));
}
"#;
    let (built, _) = build_program("sidecar_instances", source);
    let info = sidecar(&built);
    let instances: Vec<&DebugFrame> = info
        .frames
        .iter()
        .filter(|frame| frame.name == "doubled")
        .collect();
    assert_eq!(instances.len(), 2, "{:#?}", info.frames);
    assert_ne!(instances[0].js, instances[1].js, "two emitted functions");
    assert_eq!(instances[0].at, instances[1].at, "one source head");
    let mut types: Vec<String> = instances
        .iter()
        .map(|instance| type_text(info, binding(instance, "shape")))
        .collect();
    types.sort();
    assert_eq!(types, ["Line", "Square"]);
    for instance in instances {
        let js = instance.js.as_deref().expect("named");
        assert!(declares(&built.mapped, js), "`{js}` is declared");
    }
}

/// A row says how its binding holds the value, so the debugger reads what
/// `dbg` prints and not the representation: a scalar view is a `(base, key)`
/// pair, and a `lazy` parameter a thunk that reading would force.
#[test]
fn s7_a_view_and_a_lazy_parameter_say_how_they_are_read() {
    let source = r#"
import std::io::print;

fun shout(lazy message: str) {
    print(message);
}

fun main() {
    mut rows = [Some(1), Some(2)];
    match &rows[0] {
        Some(let first) => {
            print(*first);
        },
        None => {},
    }
    shout("hey");
    rows.push(None);
}
"#;
    let (built, _) = build_program("sidecar_reads", source);
    let info = sidecar(&built);
    let main = frame(info, "main");
    let first = binding(main, "first");
    assert_eq!((first.read, first.view), (BindingRead::ViewPair, true));
    assert_eq!(type_text(info, first), "i32");
    assert_eq!(binding(main, "rows").read, BindingRead::Value);
    let shout = frame(info, "shout");
    assert_eq!(binding(shout, "message").read, BindingRead::Lazy);
}

/// A statement inside a block-bodied closure maps to its own line, and the
/// closure's `return` to its tail's — so a breakpoint inside the closure binds
/// inside it rather than sliding past its end.
#[test]
fn s7_a_block_bodied_closures_statements_and_tail_map_to_their_lines() {
    let (built, source) = build_program("sidecar_closure_lines", VALUES);
    let segments = assert_valid(&built.mapped, &built.map);
    let statement = segment_at(
        &segments,
        generated_position(&built.mapped, "const total = ", 0),
    );
    assert_eq!(
        (statement.line, statement.column),
        source_position(&source, "let total = value * factor;")
    );
    let tail = segment_at(
        &segments,
        generated_position(&built.mapped, "return total;", 0),
    );
    let (line, _) = source_position(&source, "        total\n");
    assert_eq!(tail.line, line, "the closure returns at its tail's line");
}

/// The sidecar's JSON: what `vilan-dap` reads (`editors/vscode/src/dap/sidecar.ts`).
#[test]
fn s7_the_sidecar_json_is_the_format_the_adapter_reads() {
    let source = r#"
import std::io::print;

struct Point {
    x: i32,
    y: i32,
}

fun main() {
    let point = Point { x = 1, y = 2 };
    print(point.x);
}
"#;
    let (built, _) = build_program("sidecar_json", source);
    let json = sidecar(&built).to_json("main.mjs");
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("the sidecar is JSON");
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["file"], "main.mjs");
    let main = parsed["frames"]
        .as_array()
        .expect("frames")
        .iter()
        .find(|frame| frame["kind"] == "main")
        .expect("main's frame");
    assert_eq!(main["name"], "main");
    assert_eq!(main["bindings"][0]["name"], "point");
    assert_eq!(main["bindings"][0]["js"], "point");
    let point = &parsed["types"][main["bindings"][0]["type"].as_u64().expect("typed") as usize];
    assert_eq!(point["text"], "Point");
    assert_eq!(point["shape"], "struct");
    assert_eq!(point["name"], "Point");
    let x = &point["fields"][0];
    assert_eq!(x[0], "x");
    assert_eq!(
        parsed["types"][x[1].as_u64().expect("an index") as usize]["shape"],
        "integer"
    );
}
