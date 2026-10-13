//! Order 52's editor review (the owner's of 2026-10-12/13): E313's span
//! sweep, the markup head's hover and hints, the where-does-this-live
//! lines, the tooltip upgrades and the inlay pair. The kolt probes are
//! `#[ignore]`d and read `VILAN_EDITOR_KOLT` (a copy of the owner's tree,
//! lucide carried); the in-repo pins stand alone.

use std::path::{Path, PathBuf};

use super::Document;
use super::tests::{on_big_stack, std_root};
use vilan_core::analyzer::SourceId;

/// The world a kolt module is served from: the client entry analyzed whole,
/// then the module's view of it — the shape the server answers an open
/// module with (`Document::view_of`).
fn kolt_view(root: &Path, relative: &str) -> Document {
    let entry = root.join("src/client.vl");
    let entry_text = std::fs::read_to_string(&entry).expect("kolt's client entry");
    let world = Document::analyze(&entry_text, &std_root(), &entry);
    if relative == "client.vl" {
        return world;
    }
    let path = root.join("src").join(relative);
    let text = std::fs::read_to_string(&path).expect("the kolt module");
    if std::env::var_os("VILAN_EDITOR_LONE").is_some() {
        return Document::analyze(&text, &std_root(), &path);
    }
    Document::view_of(&world, root, &path, &text, Vec::new())
        .unwrap_or_else(|| panic!("{relative} is in the client's world"))
}

/// A scratch browser package on disk — `vilan.toml`, a `client` entry and
/// the modules given — removed on drop. A module is served as a VIEW of the
/// client's world ([`Package::view`]), which is the server's answer once two
/// files of one world are open (M104).
pub(super) struct Package {
    directory: PathBuf,
}

const MANIFEST: &str = "[package]\nname = \"app\"\ndefault-entry = \"client\"\n\
     prelude = \"std::web::prelude\"\n\n[entry.client]\ntarget = \"browser\"\n";

impl Package {
    pub(super) fn new(tag: &str, files: &[(&str, &str)]) -> Package {
        let directory = std::env::temp_dir().join(format!(
            "vilan-editor52-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id(),
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("src")).expect("a scratch package");
        std::fs::write(directory.join("vilan.toml"), MANIFEST).expect("the manifest");
        for (name, text) in files {
            std::fs::write(directory.join("src").join(name), text).expect("a source file");
        }
        Package { directory }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.directory.join("src").join(name)
    }

    /// The client entry's own analysis — the world.
    pub(super) fn world(&self) -> Document {
        let entry = self.path("client.vl");
        let text = std::fs::read_to_string(&entry).expect("the entry");
        Document::analyze(&text, &std_root(), &entry)
    }

    /// `name` served from `world`, as the server serves an open module.
    pub(super) fn view(&self, world: &Document, name: &str) -> Document {
        let path = self.path(name);
        let text = std::fs::read_to_string(&path).expect("the module");
        Document::view_of(world, &self.path("client.vl"), &path, &text, Vec::new())
            .unwrap_or_else(|| panic!("{name} is in the client's world"))
    }
}

impl Drop for Package {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

/// The byte offset of the `occurrence`-th `needle` in `text`, plus `delta`.
pub(super) fn at(text: &str, needle: &str, delta: usize) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} in the text"))
        + delta
}

/// A module whose one long identifier COVERS byte `offset` — so a row the
/// entry minted at `offset` lands inside a word of this file if anything
/// reads it at this file's offsets.
fn module_with_a_word_across(offset: usize) -> (String, String) {
    let prefix = "import std::display::Display;\n\nexport fun widen(count: i32): str {\n\tlet ";
    assert!(
        offset > prefix.len() + 2,
        "the entry's row sits past the prefix"
    );
    let name = format!("{}_tail", "word".repeat((offset - prefix.len()) / 4 + 2));
    let text = format!("{prefix}{name} = count.to_string();\n\t{name}\n}}\n");
    (text, name)
}

/// `VILAN_EDITOR_PROBE="file:line:from-to;…"` (1-based line and columns):
/// every column's innermost entity, its span, its `Expr` and its hover.
#[test]
#[ignore = "E313: the kolt probe - needs VILAN_EDITOR_KOLT and VILAN_EDITOR_PROBE, run deliberately"]
fn kolt_probe() {
    let Some(root) = std::env::var_os("VILAN_EDITOR_KOLT").map(PathBuf::from) else {
        return;
    };
    let probes = std::env::var("VILAN_EDITOR_PROBE").unwrap_or_default();
    on_big_stack(move || {
        for probe in probes.split(';').filter(|probe| !probe.is_empty()) {
            let mut parts = probe.split(':');
            let file = parts.next().expect("a file");
            let line: usize = parts
                .next()
                .expect("a line")
                .parse()
                .expect("a line number");
            let (from, to) = parts
                .next()
                .expect("columns")
                .split_once('-')
                .expect("from-to");
            let (from, to): (usize, usize) = (from.parse().unwrap(), to.parse().unwrap());
            let document = kolt_view(&root, file);
            let text = document.analyzed_text().to_string();
            let line_start: usize = text
                .split_inclusive('\n')
                .take(line - 1)
                .map(str::len)
                .sum();
            let program = document.program.as_ref().expect("a program");
            println!(
                "== {file}:{line} {:?}",
                text[line_start..].lines().next().unwrap_or("")
            );
            for column in from..=to {
                let offset = line_start + column - 1;
                let character = text[offset..].chars().next().unwrap_or(' ');
                let entity = document.entity_at(offset);
                let row = entity.map(|id| {
                    let span = program.span_map.get(&id).map(|span| span.into_range());
                    let kind = program
                        .entity_map
                        .get(&id)
                        .map(|expr| {
                            let debug = format!("{expr:?}");
                            debug.chars().take(70).collect::<String>()
                        })
                        .unwrap_or_default();
                    (id, span, kind)
                });
                let hover = document.hover(offset).map(|hover| {
                    hover
                        .replace('\n', " / ")
                        .chars()
                        .take(160)
                        .collect::<String>()
                });
                println!("{column:>4} {character:?} {row:?}\n       hover: {hover:?}");
            }
        }
    });
}

/// E313's sweep: every span a hover answers by containment — the entity
/// table, and this document's pattern and type-position rows — starts on a
/// token boundary. A span that starts INSIDE an identifier wins
/// `entity_at`'s narrowest-containing test for the word's tail, which is how
/// a hover splits a word. Answers the offending rows, rendered for a report.
pub(super) fn spans_starting_inside_a_word(document: &Document) -> Vec<String> {
    let text = document.analyzed_text();
    let bytes = text.as_bytes();
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80;
    let inside = |start: usize| {
        start > 0 && start < bytes.len() && word(bytes[start]) && word(bytes[start - 1])
    };
    let mut offending = Vec::new();
    let Some(program) = document.program.as_ref() else {
        return offending;
    };
    let line_of = |offset: usize| text[..offset].matches('\n').count() + 1;
    for (start, end, id) in &document.entity_spans {
        if inside(*start) {
            offending.push(format!(
                "entity {id:?} {:?} at line {}: {:?}",
                program
                    .entity_map
                    .get(id)
                    .map(|expr| { format!("{expr:?}").chars().take(40).collect::<String>() }),
                line_of(*start),
                &text[*start..(*end).min(text.len())]
            ));
        }
    }
    for site in &program.pattern_labels {
        let range = site.name_span.into_range();
        if site.source == document.focus && inside(range.start) {
            offending.push(format!(
                "pattern {:?} at line {}",
                site.label,
                line_of(range.start)
            ));
        }
    }
    for hover in &program.reference_hovers {
        let range = hover.span.into_range();
        if hover.source == document.focus && inside(range.start) {
            offending.push(format!(
                "reference {:?} at line {}",
                hover.code,
                line_of(range.start)
            ));
        }
    }
    offending
}

/// E313's sweep over a kolt copy: every `src/*.vl` file, served as a view of
/// the client's world (the owner's session shape).
#[test]
#[ignore = "E313: the kolt sweep - needs VILAN_EDITOR_KOLT, run deliberately"]
fn kolt_no_hover_span_starts_inside_a_word() {
    let Some(root) = std::env::var_os("VILAN_EDITOR_KOLT").map(PathBuf::from) else {
        return;
    };
    on_big_stack(move || {
        let entry = root.join("src/client.vl");
        let entry_text = std::fs::read_to_string(&entry).expect("kolt's client entry");
        let world = Document::analyze(&entry_text, &std_root(), &entry);
        let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("src"))
            .expect("kolt's src")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "vl"))
            .collect();
        files.sort();
        let mut offending = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).expect("a kolt file");
            let document = if path == entry {
                Document::analyze(&entry_text, &std_root(), &entry)
            } else {
                match Document::view_of(&world, &root, &path, &text, Vec::new()) {
                    Some(view) => view,
                    None => continue,
                }
            };
            for row in spans_starting_inside_a_word(&document) {
                offending.push(format!("{}: {row}", path.display()));
            }
        }
        assert!(offending.is_empty(), "{}", offending.join("\n"));
    });
}

/// E313's invariant as the owner met it: every column of one identifier
/// hovers the same answer. Answers the words that split, with the columns'
/// answers, for a report.
pub(super) fn words_whose_hover_splits(document: &Document) -> Vec<String> {
    let text = document.analyzed_text();
    let bytes = text.as_bytes();
    let word = |byte: u8| byte.is_ascii_alphanumeric() || byte == b'_';
    let mut offending = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        if !word(bytes[start]) || (start > 0 && word(bytes[start - 1])) {
            start += 1;
            continue;
        }
        let mut end = start;
        while end < bytes.len() && word(bytes[end]) {
            end += 1;
        }
        if !bytes[start].is_ascii_digit() && end - start > 1 {
            let first = document.hover(start);
            if let Some(column) = (start + 1..end).find(|offset| document.hover(*offset) != first) {
                let line = text[..start].matches('\n').count() + 1;
                offending.push(format!(
                    "`{}` at line {line} splits at byte {}: {:?} then {:?}",
                    &text[start..end],
                    column - start,
                    first.map(|hover| hover.chars().take(80).collect::<String>()),
                    document
                        .hover(column)
                        .map(|hover| hover.chars().take(80).collect::<String>()),
                ));
            }
        }
        start = end;
    }
    offending
}

/// The same sweep, every column of every word of kolt's own files (the
/// generated lucide and search-dict modules excluded for their size).
#[test]
#[ignore = "E313: the kolt word sweep - needs VILAN_EDITOR_KOLT, run deliberately"]
fn kolt_no_hover_splits_a_word() {
    let Some(root) = std::env::var_os("VILAN_EDITOR_KOLT").map(PathBuf::from) else {
        return;
    };
    on_big_stack(move || {
        let entry = root.join("src/client.vl");
        let entry_text = std::fs::read_to_string(&entry).expect("kolt's client entry");
        let world = Document::analyze(&entry_text, &std_root(), &entry);
        let mut files: Vec<PathBuf> = std::fs::read_dir(root.join("src"))
            .expect("kolt's src")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "vl"))
            .collect();
        files.sort();
        let mut offending = Vec::new();
        for path in files {
            let text = std::fs::read_to_string(&path).expect("a kolt file");
            let view;
            let document = if path == entry {
                &world
            } else {
                match Document::view_of(&world, &root, &path, &text, Vec::new()) {
                    Some(served) => {
                        view = served;
                        &view
                    }
                    None => continue,
                }
            };
            for row in words_whose_hover_splits(document) {
                offending.push(format!("{}: {row}", path.display()));
            }
        }
        assert!(offending.is_empty(), "{}", offending.join("\n"));
    });
}

/// E313, the owner's shape at its cause: the entry's PATTERN rows (E241) —
/// a `None` leg and a `_` leg — were read by every view of its world at the
/// view's own offsets, so a word of a module that happened to cover those
/// bytes hovered as `Option<i32>::None` from its middle on (kolt's
/// `shortcut` in `channel.vl`, the `in` of `to_string` in `prefs.vl`).
#[test]
fn e313_a_view_never_answers_with_the_entrys_pattern_rows() {
    on_big_stack(|| {
        let client = "import pkg::util::widen;\n\n\
             fun pick(value: Option<i32>): i32 {\n\tmatch value {\n\t\tNone => 0,\n\t\tSome(let inner) => inner,\n\t}\n}\n\n\
             fun guess(value: i32): i32 {\n\tmatch value {\n\t\t1 => 1,\n\t\t_ => 2,\n\t}\n}\n\n\
             fun main() {\n\tlet _ = pick(Some(1)) + guess(3);\n\tlet _ = widen(1);\n}\n";
        for (leg, label) in [("None =>", "::None"), ("_ =>", "_: i32")] {
            let offset = at(client, leg, 1);
            let (module, word) = module_with_a_word_across(offset);
            let package = Package::new("e313", &[("client.vl", client), ("util.vl", &module)]);
            let world = package.world();
            // The entry's own row answers in the entry.
            let entry_answer = world.hover(at(client, leg, 0)).expect("the leg hovers");
            assert!(entry_answer.contains(label), "{entry_answer}");
            let view = package.view(&world, "util.vl");
            let start = at(&module, &word, 0);
            let first = view.hover(start).expect("the binding hovers");
            assert!(first.contains(&format!("let {word}: str")), "{first}");
            for column in start..start + word.len() {
                assert_eq!(
                    view.hover(column).as_deref(),
                    Some(first.as_str()),
                    "column {} of `{word}` (the entry's `{leg}` row sits at {offset})",
                    column - start
                );
            }
            assert!(words_whose_hover_splits(&view).is_empty());
            assert!(spans_starting_inside_a_word(&view).is_empty());
        }
    });
}

/// E313's three positions from the item, as a VIEW and as the entry: a
/// member read after an index in a markup hole (kolt `channel.vl:134`), the
/// same chain outside markup, the same chain without the index, and the
/// closure body in a struct initializer (`prefs.vl:61`) — no word splits.
#[test]
fn e313_no_word_splits_in_the_kolt_shapes() {
    on_big_stack(|| {
        let shapes = "import std::display::Display;\n\nexport struct Shortcut {\n\tkey: str,\n}\n\n\
             export struct Binding {\n\tshortcut: Shortcut,\n}\n\n\
             export struct Layer {\n\tbindings: List<Binding>,\n}\n\n\
             export struct Codec {\n\tencode: |bool| Option<str>,\n}\n\n\
             impl Shortcut {\n\tfun to_string(self): str {\n\t\tself.key\n\t}\n}\n\n\
             export fun layer(): Layer {\n\tLayer { bindings = [Binding { shortcut = Shortcut { key = \"k\" } }] }\n}\n\n\
             export fun hole(): View {\n\t<span>\n\t\t{layer().bindings[0].shortcut.to_string()}\n\t</span>\n}\n\n\
             export fun plain(): str {\n\tlet indexed = layer().bindings[0].shortcut.to_string();\n\t\
             let first = layer().bindings.first().unwrap().shortcut.to_string();\n\tindexed + first\n}\n\n\
             export fun codec(): Codec {\n\tCodec {\n\t\tencode = |x| Some(x.to_string()),\n\t}\n}\n";
        let client = "import pkg::shapes::{ codec, hole, plain };\n\n\
             fun main() {\n\tmatch Some(1) {\n\t\tNone => {}\n\t\tSome(let one) => {\n\t\t\tlet _ = one;\n\t\t}\n\t}\n\
             \tlet _ = hole();\n\tlet _ = plain();\n\tlet _ = codec();\n}\n";
        let package = Package::new(
            "e313-shapes",
            &[("client.vl", client), ("shapes.vl", shapes)],
        );
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let view = package.view(&world, "shapes.vl");
        for document in [&world, &view] {
            let splits = words_whose_hover_splits(document);
            assert!(splits.is_empty(), "{}", splits.join("\n"));
            let starts = spans_starting_inside_a_word(document);
            assert!(starts.is_empty(), "{}", starts.join("\n"));
        }
        // And the member read answers as itself on every column.
        let shortcut = at(shapes, "shortcut.to_string()}", 0);
        let answer = view.hover(shortcut).expect("the member read hovers");
        assert!(answer.contains("shortcut: Shortcut"), "{answer}");
        let to_string = at(shapes, "to_string())", 0);
        let answer = view.hover(to_string + 5).expect("the method hovers");
        assert!(answer.contains("fun to_string(self): str"), "{answer}");
    });
}

/// E313's other rows, which had the same entry-only reach: a VIEW now
/// answers its own E206 call line (kolt `command_palette.vl:220`'s `get`,
/// E308's premise), E237 definition, E240 trait block and E241 pattern.
#[test]
fn e313_a_view_answers_its_own_hover_rows() {
    on_big_stack(|| {
        let module = "export struct Boxed<T> {\n\tvalue: T,\n}\n\n\
             impl Boxed<type T> {\n\tfun get(self): T {\n\t\tself.value\n\t}\n}\n\n\
             export enum Shape {\n\tCircle(f64),\n\tEmpty,\n}\n\n\
             export trait Named {\n\tfun name(self): str;\n}\n\n\
             export fun describe<N: Named>(named: N): str {\n\tnamed.name()\n}\n\n\
             export fun area(shape: Shape): f64 {\n\tmatch shape {\n\t\tShape::Circle(let r) => r,\n\t\tShape::Empty => 0.0,\n\t}\n}\n\n\
             export fun run(): usize {\n\tlet boxed = Boxed { value = [1, 2] };\n\tlet shape = Shape::Empty;\n\tlet _ = area(shape);\n\tboxed.get().len()\n}\n";
        let client = "import pkg::shapes::run;\n\nfun main() {\n\tlet _ = run();\n}\n";
        let package = Package::new("e313-rows", &[("client.vl", client), ("shapes.vl", module)]);
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let view = package.view(&world, "shapes.vl");
        let get = view
            .hover(at(module, "get().len", 0))
            .expect("`get` hovers");
        assert!(
            get.contains("fun get(self): List<i32>"),
            "E206's line in a view: {get}"
        );
        let shape = view
            .hover(at(module, "shape = Shape", 0))
            .expect("`shape` hovers");
        assert!(
            shape.contains("enum Shape {"),
            "E237's chunk in a view: {shape}"
        );
        let named = view
            .hover(at(module, "Named>(", 0))
            .expect("`Named` hovers");
        assert!(
            named.contains("fun name(self): str;"),
            "E240's block in a view: {named}"
        );
        let empty = view
            .hover(at(module, "Empty => 0.0", 0))
            .expect("the leg hovers");
        assert!(
            empty.contains("Shape::Empty"),
            "E241's label in a view: {empty}"
        );
    });
}

/// E313: a module under the package's declared `generated` root renders no
/// per-file rows (kolt's lucide module is three quarters of its world's
/// generic call sites) — its words still hover, and the rows of its
/// neighbours are unaffected.
#[test]
fn e313_a_generated_module_renders_no_hover_rows() {
    on_big_stack(|| {
        let module = "export fun pick(value: Option<i32>): i32 {\n\tmatch value {\n\t\tNone => 0,\n\t\tSome(let inner) => inner,\n\t}\n}\n";
        let client = "import pkg::gen::icons::pick;\n\nfun main() {\n\tlet _ = pick(None);\n}\n";
        let package = Package::new("e313-generated", &[("client.vl", client)]);
        std::fs::create_dir_all(package.path("gen/icons.vl").parent().unwrap()).unwrap();
        std::fs::write(package.path("gen/icons.vl"), module).unwrap();
        let manifest = package.directory.join("vilan.toml");
        let declared = std::fs::read_to_string(&manifest).unwrap().replace(
            "prelude = \"std::web::prelude\"\n",
            "prelude = \"std::web::prelude\"\ngenerated = \"src/gen\"\n",
        );
        std::fs::write(&manifest, declared).unwrap();
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let program = world.program.as_ref().expect("a program");
        assert!(
            program
                .pattern_labels
                .iter()
                .all(|row| row.source == SourceId(0)),
            "a generated module banks no rendered rows"
        );
        let view = package.view(&world, "gen/icons.vl");
        let inner = view
            .hover(at(module, "inner) =>", 0))
            .expect("the capture hovers");
        assert!(inner.contains("let inner: i32"), "{inner}");
    });
}

/// E313's table, deduplicated by rendered type, met two IMPLICIT parameters
/// once it was every file's: `x: Show<i32>` and `y: Show<str>` both render
/// `Show` (B261's bare name), and the second hovered with the first's block.
#[test]
fn e313_two_implicit_parameters_keep_their_own_definitions() {
    on_big_stack(|| {
        let module = "export trait Show<T> {\n\tfun show(self, value: T): str;\n}\n\n\
             export fun numbers(shown: Show<i32>): str {\n\tshown.show(1)\n}\n\n\
             export fun words(spoken: Show<str>): str {\n\tspoken.show(\"a\")\n}\n";
        let client = "import pkg::show::{ numbers, words };\n\nfun main() {}\n";
        let package = Package::new(
            "e313-implicit",
            &[("client.vl", client), ("show.vl", module)],
        );
        let world = package.world();
        let view = package.view(&world, "show.vl");
        // As a view, and as the file's own analysis (the lone-document shape,
        // where the entry-only table met the collision first).
        let lone = Document::analyze(module, &std_root(), &package.path("show.vl"));
        for document in [&view, &lone] {
            let shown = document
                .hover(at(module, "shown:", 0))
                .expect("`shown` hovers");
            let spoken = document
                .hover(at(module, "spoken:", 0))
                .expect("`spoken` hovers");
            assert!(
                shown.contains("fun show(self, value: i32): str;"),
                "{shown}"
            );
            assert!(
                spoken.contains("fun show(self, value: str): str;"),
                "{spoken}"
            );
        }
    });
}

/// The E305 head: the kolt positions (`views.vl:113`'s `on:click`,
/// `views.vl:116`'s `.styled`), an attribute, a chain item over two lines
/// and an event with a named handler.
const E305_HEAD: &str = "import std::web::style::Style;\n\n\
     fun icon_style(): Style {\n\tstyle::style()\n}\n\nfun ping() {}\n\n\
     export fun bar(open: || void): View {\n\t<div>\n\t\t<button\n\t\t\ttitle(\"Search\")\n\
     \t\t\ton:click(|_| {\n\t\t\t\topen();\n\t\t\t})\n\
     \t\t\t.styled(icon_style())\n\
     \t\t\t.text(\n\t\t\t\t\"wide\",\n\t\t\t)\n\
     \t\t\ton:dblclick(ping)\n\t\t/>\n\t</div>\n}\n";

/// E305: the head's written tokens answer for themselves — `on:` and the
/// event name as the event hook the desugar lowers to, `.styled` as
/// `styled`'s own signature (as it hovers outside markup), an attribute's
/// name as `attr` — in the entry and in a view.
#[test]
fn e305_an_element_heads_written_tokens_answer_for_themselves() {
    on_big_stack(|| {
        let client = "import pkg::bar::bar;\n\nfun main() {\n\tlet _ = bar(|| {});\n}\n";
        let package = Package::new("e305", &[("client.vl", client), ("bar.vl", E305_HEAD)]);
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let view = package.view(&world, "bar.vl");
        let lone = Document::analyze(E305_HEAD, &std_root(), &package.path("bar.vl"));
        for document in [&view, &lone] {
            let hover = |needle: &str, delta: usize| {
                document
                    .hover(at(E305_HEAD, needle, delta))
                    .unwrap_or_else(|| panic!("{needle:?}+{delta} hovers"))
            };
            for (needle, delta) in [
                ("on:click", 0),
                ("on:click", 1),
                ("on:click", 3),
                ("on:click", 7),
            ] {
                let answer = hover(needle, delta);
                assert!(
                    answer.contains("fun on_event(self, event: str"),
                    "{needle}+{delta}: {answer}"
                );
            }
            for delta in [0, 3, 10] {
                let answer = hover("on:dblclick", delta);
                assert!(
                    answer.contains("fun on(self, event: str"),
                    "dblclick+{delta}: {answer}"
                );
            }
            for delta in [1, 4] {
                let answer = hover(".styled(icon_style())", delta);
                assert!(
                    answer.contains("fun styled(self, style: Style): View"),
                    "{answer}"
                );
                let answer = hover(".text(\n", delta);
                assert!(
                    answer.contains("fun text(self, content: str): View"),
                    "{answer}"
                );
            }
            let answer = hover("title(", 2);
            assert!(answer.contains("fun attr(self, name: str"), "{answer}");
            // The written string is still the reader's own: no hover.
            assert_eq!(document.hover(at(E305_HEAD, "\"Search\"", 2)), None);
        }
    });
}

/// `VILAN_EDITOR_HINTS="file:from-to;…"` (1-based lines): every inlay hint
/// the file's view lands on those lines, with its anchor.
#[test]
#[ignore = "E303: the kolt hint probe - needs VILAN_EDITOR_KOLT and VILAN_EDITOR_HINTS, run deliberately"]
fn kolt_hint_probe() {
    let Some(root) = std::env::var_os("VILAN_EDITOR_KOLT").map(PathBuf::from) else {
        return;
    };
    let probes = std::env::var("VILAN_EDITOR_HINTS").unwrap_or_default();
    on_big_stack(move || {
        for probe in probes.split(';').filter(|probe| !probe.is_empty()) {
            let (file, lines) = probe.split_once(':').expect("file:from-to");
            let (from, to) = lines.split_once('-').expect("from-to");
            let (from, to): (usize, usize) = (from.parse().unwrap(), to.parse().unwrap());
            let document = kolt_view(&root, file);
            let text = document.analyzed_text().to_string();
            for hint in document.landed_hints() {
                let line = text[..hint.name.end].matches('\n').count() + 1;
                if (from..=to).contains(&line) {
                    let start = hint.name.start;
                    let shown: String = text[start..hint.name.end].chars().take(50).collect();
                    println!(
                        "{file}:{line} after {shown:?} -> {:?} / {:?}",
                        hint.label, hint.abbreviated
                    );
                }
            }
        }
    });
}

/// The stage hints a document lands, as `(line, label)` (1-based lines).
fn stage_hint_lines(document: &Document) -> Vec<(usize, String)> {
    let text = document.analyzed_text();
    document
        .landed_hints()
        .into_iter()
        .filter(|hint| hint.label.starts_with(" as "))
        .map(|hint| (text[..hint.name.end].matches('\n').count() + 1, hint.label))
        .collect()
}

/// E303 (RULED: hide): kolt `views.vl:110`'s head — an event, a `.styled`
/// item and a `.child(swap(..))` whose argument is a chain split over lines —
/// inside a parent element. Neither the tag nor the head's items nor the
/// children's links hint (they DOUBLED at the element's last line, where the
/// parent's `.child` link and the element's own last link both end); the
/// chain inside the attribute value keeps E278's hints; a chain written after
/// the finished element is the reader's and hints.
#[test]
fn e303_an_element_head_mints_no_stage_hints() {
    on_big_stack(|| {
        let source = "import std::web::style::Style;\n\n\
             fun icon_style(): Style {\n\tstyle::style()\n}\n\n\
             fun label(open: bool): str {\n\tif open { \"a\" } else { \"b\" }\n}\n\n\
             export fun bar(): View {\n\t<div>\n\t\t<button\n\t\t\ton:contextmenu(|event| {\n\t\t\t\tevent.prevent_default();\n\t\t\t})\n\
             \t\t\t.styled(icon_style())\n\
             \t\t\t.child(Some(true)\n\t\t\t\t.map(|x| !x)\n\t\t\t\t.map(|x| label(x))\n\t\t\t\t.unwrap_or(\"c\"))\n\
             \t\t/>\n\t\t<span />\n\t</div>\n}\n\n\
             export fun after(): View {\n\t<div />\n\t\t.text(\"later\")\n\t\t.class(\"x\")\n}\n";
        let client = "import pkg::bar::{ after, bar };\n\nfun main() {\n\tlet _ = bar();\n\tlet _ = after();\n}\n";
        let package = Package::new("e303", &[("client.vl", client), ("bar.vl", source)]);
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let view = package.view(&world, "bar.vl");
        let lone = Document::analyze(source, &std_root(), &package.path("bar.vl"));
        let line = |needle: &str| source[..at(source, needle, 0)].matches('\n').count() + 1;
        for document in [&view, &lone] {
            let hints = stage_hint_lines(document);
            // The inner chain inside `.child(..)`'s argument: two stages,
            // `.map(|x| !x)` and `.map(|x| label(x))` (the last ends inside
            // the parentheses of the item, so it ends no line).
            assert!(
                hints.contains(&(line(".map(|x| !x)"), " as Option<bool>".to_string())),
                "{hints:?}"
            );
            assert!(
                hints.contains(&(line(".map(|x| label(x))"), " as Option<str>".to_string())),
                "{hints:?}"
            );
            // Nothing on the element's own lines: the tag, the items, the
            // closing `/>`, the parent's lines.
            for needle in [
                "<button",
                ".styled(",
                "\t\t/>",
                "<span />",
                "\t<div>",
                "on:contextmenu",
            ] {
                assert!(
                    !hints.iter().any(|(at_line, _)| *at_line == line(needle)),
                    "a hint on `{needle}`'s line: {hints:?}"
                );
            }
            // At most one hint per line anywhere.
            let mut lines: Vec<usize> = hints.iter().map(|(at_line, _)| *at_line).collect();
            lines.dedup();
            assert_eq!(lines.len(), hints.len(), "{hints:?}");
            // The chain written after the finished element is the reader's.
            assert!(
                hints.contains(&(line(".text(\"later\")"), " as View".to_string())),
                "{hints:?}"
            );
            assert!(
                hints.contains(&(line("\t<div />"), " as View".to_string())),
                "{hints:?}"
            );
        }
    });
}

/// `VILAN_EDITOR_TOKENS="file:from-to;…"` (1-based lines): every semantic
/// token on those lines, with its text and kind.
#[test]
#[ignore = "E311: the kolt token probe - needs VILAN_EDITOR_KOLT and VILAN_EDITOR_TOKENS, run deliberately"]
fn kolt_token_probe() {
    let Some(root) = std::env::var_os("VILAN_EDITOR_KOLT").map(PathBuf::from) else {
        return;
    };
    let probes = std::env::var("VILAN_EDITOR_TOKENS").unwrap_or_default();
    on_big_stack(move || {
        for probe in probes.split(';').filter(|probe| !probe.is_empty()) {
            let (file, lines) = probe.split_once(':').expect("file:from-to");
            let (from, to) = lines.split_once('-').expect("from-to");
            let (from, to): (usize, usize) = (from.parse().unwrap(), to.parse().unwrap());
            let document = kolt_view(&root, file);
            let text = document.analyzed_text().to_string();
            for (span, kind, modifiers) in document.semantic_tokens() {
                let range = span.into_range();
                let line = text[..range.start].matches('\n').count() + 1;
                if (from..=to).contains(&line) {
                    println!("{file}:{line} {:?} {kind:?} {modifiers}", &text[range]);
                }
            }
        }
    });
}

/// Every semantic token over the one occurrence of `needle` in `text`, as
/// `(token text, kind)`.
fn tokens_over(document: &Document, needle: &str) -> Vec<(String, String)> {
    let text = document.analyzed_text();
    let start = text.find(needle).unwrap_or_else(|| panic!("{needle:?}"));
    assert_eq!(text.matches(needle).count(), 1, "{needle:?} occurs once");
    let end = start + needle.len();
    let mut tokens: Vec<(usize, String, String)> = document
        .semantic_tokens()
        .into_iter()
        .filter(|(span, _, _)| span.start >= start && span.end <= end)
        .map(|(span, kind, _)| {
            (
                span.start,
                text[span.into_range()].to_string(),
                format!("{kind:?}"),
            )
        })
        .collect();
    tokens.sort();
    tokens
        .into_iter()
        .map(|(_, word, kind)| (word, kind))
        .collect()
}

/// E311: a function a PATH reaches paints as a Function — `stroke1` in
/// `.border(px(1), Theme::stroke1())` inside a `css` block's chain link (kolt
/// `login_page.vl:97`), and equally in a property's hole, a condition body,
/// plain code and a module-qualified call: the call's one entity spans the
/// whole path, and its token lost to the type's at the overlap.
#[test]
fn e311_a_function_reached_through_a_path_paints_as_a_function() {
    on_big_stack(|| {
        let source = "import std::web::style::{ Color, Style };\n\n\
             struct Theme {}\n\nimpl Theme {\n\tfun stroke1(): Color {\n\t\tColor::hex(\"#5e5e5e\")\n\t}\n\
             \tfun ink(): Color {\n\t\tColor::hex(\"#000000\")\n\t}\n}\n\n\
             fun plain(): Color {\n\tlet c = Theme::stroke1();\n\tc\n}\n\n\
             fun block(): View {\n\t<div .styled(css {\n\t\tpadding(style::Length::px(4));\n\t\t.border(px(1), Theme::stroke1());\n\
             \t\tcolor(Theme::ink());\n\t\t.hover {\n\t\t\t.border(px(2), Theme::ink());\n\t\t}\n\t}) />\n}\n\n\
             fun main() {\n\tlet _ = plain();\n\tlet _ = block();\n}\n";
        let package = Package::new("e311", &[("client.vl", source)]);
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let pair = |word: &str, kind: &str| (word.to_string(), kind.to_string());
        assert_eq!(
            tokens_over(&document, ".border(px(1), Theme::stroke1());"),
            vec![
                pair("border", "Method"),
                pair("px", "Function"),
                pair("Theme", "Struct"),
                pair("stroke1", "Function"),
            ]
        );
        assert_eq!(
            tokens_over(&document, "color(Theme::ink());"),
            vec![
                pair("color", "Property"),
                pair("Theme", "Struct"),
                pair("ink", "Function")
            ]
        );
        assert_eq!(
            tokens_over(&document, ".border(px(2), Theme::ink());"),
            vec![
                pair("border", "Method"),
                pair("px", "Function"),
                pair("Theme", "Struct"),
                pair("ink", "Function"),
            ]
        );
        assert_eq!(
            tokens_over(&document, "let c = Theme::stroke1();"),
            vec![
                pair("c", "Variable"),
                pair("Theme", "Struct"),
                pair("stroke1", "Function")
            ]
        );
        assert_eq!(
            tokens_over(&document, "padding(style::Length::px(4));"),
            vec![
                pair("padding", "Property"),
                pair("style", "Namespace"),
                pair("px", "Function")
            ]
        );
    });
}

/// E299's package: a nested module with a free function, a method, a trait
/// requirement and a generic call site; the client calls into it and std.
const E299_OVERLAY: &str = "/// The overlay.\nexport struct Overlay {\n\topen: bool,\n}\n\n\
     export impl Overlay {\n\tfun toggle(self): bool {\n\t\t!self.open\n\t}\n}\n\n\
     export trait Shows {\n\tfun show(self): str;\n}\n\n\
     export fun pick<T>(value: T): T {\n\tvalue\n}\n\n\
     export fun dismiss(): Overlay {\n\tOverlay { open = false }\n}\n";

/// E299: every function hover opens with its declaring module's `mod` line —
/// above E238's header, above E206's two lines.
#[test]
fn e299_a_function_hover_opens_with_its_mod_line() {
    on_big_stack(|| {
        let client = "import pkg::lib::overlay::{ Overlay, Shows, dismiss, pick };\n\n\
             fun local(): i32 {\n\t1\n}\n\n\
             fun main() {\n\tlet overlay = dismiss();\n\tlet _ = overlay.toggle();\n\tlet _ = pick(3);\n\tlet _ = local();\n\tlet _ = [1].len();\n}\n";
        let package = Package::new("e299", &[("client.vl", client)]);
        std::fs::create_dir_all(package.path("lib")).unwrap();
        std::fs::write(package.path("lib/overlay.vl"), E299_OVERLAY).unwrap();
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let hover = |document: &Document, text: &str, needle: &str| {
            document
                .hover(at(text, needle, 0))
                .unwrap_or_else(|| panic!("{needle:?} hovers"))
        };
        // A free function in a nested module, at its call.
        assert!(
            hover(&world, client, "dismiss();")
                .starts_with("```vilan\nmod pkg::lib::overlay\nfun dismiss(): Overlay\n```"),
            "{}",
            hover(&world, client, "dismiss();")
        );
        // A method: three lines, the mod line on top of E238's header.
        assert!(
            hover(&world, client, "toggle()").starts_with(
                "```vilan\nmod pkg::lib::overlay\nimpl Overlay\nfun toggle(self): bool\n```"
            ),
            "{}",
            hover(&world, client, "toggle()")
        );
        // E206's two-line site: the mod line once, on top.
        let generic = hover(&world, client, "pick(3)");
        assert!(
            generic.starts_with("```vilan\nmod pkg::lib::overlay\nfun pick<T>(value: T): T\n\nfun pick(value: i32): i32\n```"),
            "{generic}"
        );
        // A std function, and a function of the document's own module.
        assert!(
            hover(&world, client, "len()").starts_with("```vilan\nmod std::list\n"),
            "{}",
            hover(&world, client, "len()")
        );
        assert!(
            hover(&world, client, "local();")
                .starts_with("```vilan\nmod pkg::client\nfun local(): i32\n```"),
            "{}",
            hover(&world, client, "local();")
        );
        // A trait requirement, at its declaration in a view of the module.
        let view = package.view(&world, "lib/overlay.vl");
        assert!(
            hover(&view, E299_OVERLAY, "show(self)").starts_with(
                "```vilan\nmod pkg::lib::overlay\ntrait Shows\nfun show(self): str\n```"
            ),
            "{}",
            hover(&view, E299_OVERLAY, "show(self)")
        );
        // Out of scope: a type's hover keeps its shape.
        assert!(hover(&world, client, "Overlay,").starts_with("```vilan\nstruct Overlay"));
    });
}

/// E298: each segment of a module path answers for ITS module — in an
/// import and in value position — with its `mod` line, its `///`, what it
/// exports, its file and its platform; the package root with the package;
/// a std segment; E152's bodiless namespace unchanged.
#[test]
fn e298_a_module_segment_hovers_as_its_module() {
    on_big_stack(|| {
        let client = "import pkg::lib::overlay::{ Overlay, show };\nimport pkg::model;\n\
             import std::reactive::store::Store;\n\n\
             fun main() {\n\tshow();\n\tlet _ = model::global();\n}\n";
        let overlay = "/// What floats above the page.\n///\n/// Dismissed by a click outside.\n\n\
             export struct Overlay {\n\topen: bool,\n}\n\nexport fun show() {}\n\nfun hidden() {}\n";
        let model = "[platform(\"browser\")] mod self;\n\nexport fun global(): i32 {\n\t1\n}\n";
        let package = Package::new("e298", &[("client.vl", client), ("model.vl", model)]);
        std::fs::create_dir_all(package.path("lib")).unwrap();
        std::fs::write(package.path("lib/overlay.vl"), overlay).unwrap();
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let hover = |needle: &str, delta: usize| {
            world
                .hover(at(client, needle, delta))
                .unwrap_or_else(|| panic!("{needle:?}+{delta} hovers"))
        };
        let overlay_hover = hover("overlay::{", 2);
        assert!(
            overlay_hover.starts_with("```vilan\nmod pkg::lib::overlay\n```\n\nWhat floats above the page.\n\nDismissed by a click outside."),
            "{overlay_hover}"
        );
        assert!(
            overlay_hover.contains("Exports `Overlay`, `show`."),
            "{overlay_hover}"
        );
        assert!(
            overlay_hover.contains("`src/lib/overlay.vl`"),
            "{overlay_hover}"
        );
        // `lib` has no body: E152's namespace answer stands.
        assert_eq!(
            hover("lib::overlay", 1),
            "```vilan\nnamespace lib\n```\n\n---\n\nHolds `overlay`."
        );
        // The package root.
        let root = hover("pkg::lib", 1);
        assert!(
            root.starts_with("```vilan\nmod pkg\n```\n\nThe package `app`."),
            "{root}"
        );
        assert!(root.contains("Holds "), "{root}");
        // A std segment, and a platform-gated module in value position.
        let store = hover("store::Store", 2);
        assert!(
            store.starts_with("```vilan\nmod std::reactive::store\n```"),
            "{store}"
        );
        assert!(store.contains("`std/src/reactive/store.vl`"), "{store}");
        let std_root_hover = hover("std::reactive", 1);
        assert!(
            std_root_hover.starts_with("```vilan\nmod std\n```"),
            "{std_root_hover}"
        );
        let value = hover("model::global", 2);
        assert!(
            value.starts_with("```vilan\nmod pkg::model\n```"),
            "{value}"
        );
        assert!(
            value.contains("declares `[platform(\"browser\")] mod self;`"),
            "{value}"
        );
        assert_eq!(
            value,
            hover("import pkg::model", 13),
            "an import and a value path agree"
        );
    });
}
