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
                        .take(4000)
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

/// E307: a field hovers under the struct it belongs to — an initializer key
/// with a closure-typed field (kolt `views.vl:79`), its declaration, a
/// generic struct, a member read, and an internal field (E213's lead first).
#[test]
fn e307_a_field_hover_names_its_struct() {
    on_big_stack(|| {
        let source = "struct AppInputLayerData {\n\t/// Toggles the sidebar.\n\ton_toggle_sidebar: || void,\n}\n\n\
             struct Ring<T> {\n\titems: List<T>,\n\t[internal(\"the cursor\")]\n\tcursor: usize,\n}\n\n\
             fun main() {\n\tlet data = AppInputLayerData {\n\t\ton_toggle_sidebar = || {},\n\t};\n\
             \tlet ring = Ring { items = [1], cursor = 0 };\n\tlet _ = ring.items;\n\tlet _ = data;\n\tlet _ = ring.cursor;\n}\n";
        let package = Package::new("e307", &[("client.vl", source)]);
        let document = package.world();
        let hover = |needle: &str, delta: usize| {
            document
                .hover(at(source, needle, delta))
                .unwrap_or_else(|| panic!("{needle:?} hovers"))
        };
        let key = hover("on_toggle_sidebar = ", 2);
        assert!(
            key.starts_with("```vilan\nstruct AppInputLayerData\non_toggle_sidebar: || void\n```\n\nToggles the sidebar."),
            "{key}"
        );
        assert_eq!(
            hover("on_toggle_sidebar: ||", 2),
            key,
            "the declaration and the key agree"
        );
        let read = hover("ring.items", 6);
        assert!(
            read.starts_with("```vilan\nstruct Ring<T>\nitems: List<i32>\n```"),
            "{read}"
        );
        let declared = hover("items: List<T>", 1);
        assert!(
            declared.starts_with("```vilan\nstruct Ring<T>\nitems: List<T>\n```"),
            "{declared}"
        );
        let internal = hover("ring.cursor", 6);
        assert!(
            internal.starts_with(
                "**internal** — the cursor\n\n```vilan\nstruct Ring<T>\ncursor: usize\n```"
            ),
            "{internal}"
        );
    });
}

/// E304: the function label always carries a return type — `: void` where
/// none is written and nothing is returned, the inferred type where the body
/// returns one, the written one untouched; a method and a trait default
/// alike; a closure-typed parameter's label unchanged.
#[test]
fn e304_a_function_label_always_states_its_return_type() {
    on_big_stack(|| {
        let source = "struct Counter {\n\tcount: i32,\n}\n\n\
             impl Counter {\n\tfun bump(self) {\n\t\tlet _ = self.count + 1;\n\t}\n\tfun doubled(self) {\n\t\tself.count * 2\n\t}\n}\n\n\
             trait Greets {\n\tfun greet(self): str;\n\tfun twice(self) {\n\t\tlet _ = self.greet();\n\t}\n}\n\n\
             fun quiet() {\n\tlet _ = 1;\n}\n\nfun written(): void {}\n\nfun three() {\n\t3\n}\n\n\
             fun apply(f: |i32| i32): i32 {\n\tf(1)\n}\n\n\
             fun main() {\n\tquiet();\n\twritten();\n\tlet _ = three();\n\tlet counter = Counter { count = 1 };\n\
             \tcounter.bump();\n\tlet _ = counter.doubled();\n\tlet _ = apply(|x| x);\n}\n";
        let package = Package::new("e304", &[("client.vl", source)]);
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let signature = |needle: &str, delta: usize| {
            let hover = document
                .hover(at(source, needle, delta))
                .unwrap_or_else(|| panic!("{needle:?} hovers"));
            hover
                .lines()
                .find(|line| line.starts_with("fun "))
                .unwrap_or_else(|| panic!("a `fun` line in {hover}"))
                .to_string()
        };
        assert_eq!(signature("quiet();", 0), "fun quiet(): void");
        assert_eq!(signature("written();", 0), "fun written(): void");
        assert_eq!(signature("three();", 0), "fun three(): i32");
        assert_eq!(signature("bump();", 0), "fun bump(self): void");
        assert_eq!(signature("doubled();", 0), "fun doubled(self): i32");
        assert_eq!(signature("fun twice", 4), "fun twice(self): void");
        assert_eq!(signature("fun greet", 4), "fun greet(self): str");
        assert_eq!(signature("apply(|x|", 0), "fun apply(f: |i32| i32): i32");
    });
}

/// E308: a member of a GENERIC impl or trait renders under the impl's
/// binders as the receiver settled them — kolt `command_palette.vl:220`'s
/// `filtered_commands.get()` reads `fun get(self): List<Hit<Command>>` under
/// the declared `fun get(self): T` — merged with the function's own
/// substitution into one line; a non-generic member shows one line.
#[test]
fn e308_a_generic_impls_member_renders_under_the_receivers_binders() {
    on_big_stack(|| {
        let source = "struct Hit<C> {\n\tcommand: C,\n}\n\nstruct Command {\n\tname: str,\n}\n\n\
             struct Boxed<T> {\n\tvalue: T,\n}\n\n\
             impl Boxed<type T> {\n\tfun get(self): T {\n\t\tself.value\n\t}\n\
             \tfun map<U>(self, f: |T| U): Boxed<U> {\n\t\tBoxed { value = f(self.value) }\n\t}\n}\n\n\
             trait Read<T> {\n\tfun read(self): T;\n}\n\n\
             impl Command with Read<str> {\n\tfun read(self): str {\n\t\tself.name\n\t}\n}\n\n\
             fun through<R: Read<str>>(reader: R): str {\n\treader.read()\n}\n\n\
             fun plain(command: Command): str {\n\tcommand.name\n}\n\n\
             fun main() {\n\tlet hits = Boxed { value = [Hit { command = Command { name = \"a\" } }] };\n\
             \tlet _ = hits.get().len();\n\tlet _ = hits.map(|list| list.len()).get();\n\
             \tlet _ = through(Command { name = \"b\" });\n\tlet _ = plain(Command { name = \"c\" });\n}\n";
        let package = Package::new("e308", &[("client.vl", source)]);
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let fence = |needle: &str| {
            let hover = document
                .hover(at(source, needle, 0))
                .unwrap_or_else(|| panic!("{needle:?} hovers"));
            hover
                .split("```")
                .nth(1)
                .expect("a fence")
                .trim_start_matches("vilan\n")
                .trim_end()
                .to_string()
        };
        assert_eq!(
            fence("get().len()"),
            "mod pkg::client\nimpl Boxed<type T>\nfun get(self): T\n\nfun get(self): List<Hit<Command>>"
        );
        // The impl's `T` and the function's own `U`, one merged line.
        assert_eq!(
            fence("map(|list|"),
            "mod pkg::client\nimpl Boxed<type T>\nfun map<U>(self, f: |T| U): Boxed<U>\n\n\
             fun map(self, f: |List<Hit<Command>>| usize): Boxed<usize>"
        );
        // A trait member through a bound: the bound's arguments.
        assert_eq!(
            fence("read()"),
            "mod pkg::client\ntrait Read<T>\nfun read(self): T\n\nfun read(self): str"
        );
        // A non-generic function: one line.
        assert_eq!(
            fence("plain(Command"),
            "mod pkg::client\nfun plain(command: Command): str"
        );
    });
}

/// E310 (RULED shape): a trait named in a parameter's type position (kolt
/// `login_page.vl:72`'s `Signal<str>`) hovers with where it lives, its head
/// and required members at the written arguments, its `///`, and what can
/// be passed there — the implementors in reach at those arguments. A bound
/// and a `with` clause answer the same; a trait with no impl keeps E240's.
#[test]
fn e310_a_trait_in_a_parameter_type_hovers_with_its_implementors() {
    on_big_stack(|| {
        let shapes = "/// Something that shows a value.\nexport trait Shows<T> {\n\tfun show(self, value: T): str;\n}\n\n\
             export trait Lonely {\n\tfun alone(self): i32;\n}\n\n\
             export struct Plain {}\n\nexport impl Plain with Shows<str> {\n\tfun show(self, value: str): str {\n\t\tvalue\n\t}\n}\n\n\
             export struct Cell<T> {\n\tvalue: T,\n}\n\n\
             export impl Cell<type T> with Shows<T> {\n\tfun show(self, value: T): str {\n\t\t\"cell\"\n\t}\n}\n\n\
             export struct Numbers {}\n\nexport impl Numbers with Shows<i32> {\n\tfun show(self, value: i32): str {\n\t\t\"n\"\n\t}\n}\n";
        let client = "import pkg::shapes::{ Lonely, Plain, Shows };\n\n\
             fun login(username: Shows<str>, lonely: Lonely): str {\n\tusername.show(\"a\")\n}\n\n\
             fun bound<S: Shows<str>>(shown: S): str {\n\tshown.show(\"b\")\n}\n\n\
             fun main() {\n\tlet _ = bound(Plain {});\n}\n";
        let package = Package::new("e310", &[("client.vl", client), ("shapes.vl", shapes)]);
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let parameter = world
            .hover(at(client, "Shows<str>, lonely", 0))
            .expect("`Shows` hovers");
        assert!(
            parameter.starts_with("```vilan\nmod pkg::shapes\ntrait Shows<str> {\n\tfun show(self, value: str): str;\n}\n```"),
            "{parameter}"
        );
        assert!(
            parameter.contains("Something that shows a value."),
            "{parameter}"
        );
        assert!(
            parameter.ends_with("Implemented by `Cell<str>`, `Plain` (2 in reach)."),
            "{parameter}"
        );
        let bound = world
            .hover(at(client, "Shows<str>>(", 0))
            .expect("the bound hovers");
        assert_eq!(bound, parameter, "a bound answers as a parameter does");
        let lonely = world
            .hover(at(client, "Lonely,", 0))
            .expect("`Lonely` in the import");
        let lonely_parameter = world
            .hover(at(client, "Lonely)", 0))
            .expect("`Lonely` hovers");
        assert!(
            !lonely_parameter.contains("Implemented by"),
            "{lonely_parameter}"
        );
        assert!(
            lonely_parameter.contains("fun alone(self): i32;"),
            "{lonely_parameter} / {lonely}"
        );
    });
}

/// E312: a derive answers what it writes for THIS item (kolt `account.vl:9`'s
/// `[derive(Storable)]`), the trait it implements and its own facet; a
/// prelude derive likewise; the import leaf naming the trait AND the derive
/// says both (`account.vl:2`); a user macro attribute keeps its signature,
/// now under its `mod` line.
#[test]
fn e312_a_derive_answers_what_it_writes() {
    on_big_stack(|| {
        let client = "import std::reactive::store::{ Storable, Store };\n\n\
             /// An account.\n[derive(Storable, PartialEq)]\nstruct Account {\n\tname: str,\n}\n\n\
             macro fun derive_tag(item: Item): Source {\n\timport macro_std::source;\n\timport macro_std::meta::{ Item, Source };\n\tsource(\"\")\n}\n\n\
             [derive_tag]\nstruct Point {\n\tx: i32,\n}\n\n\
             fun main() {\n\tlet _ = Account { name = \"a\" };\n\tlet _: Option<Store<Account>> = None;\n}\n";
        let package = Package::new("e312", &[("client.vl", client)]);
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let hover = |needle: &str| {
            world
                .hover(at(client, needle, 1))
                .unwrap_or_else(|| panic!("{needle:?} hovers"))
        };
        let storable = hover("Storable, PartialEq");
        assert!(
            storable.starts_with("```vilan\nmod std::reactive::store\nimpl Account with Storable\nimpl Store<Account>\n"),
            "{storable}"
        );
        assert!(
            storable.contains("derive macro: `macro fun Storable(item: Item): Source`"),
            "{storable}"
        );
        assert!(
            storable.contains("A type a store diffs by its shape."),
            "{storable}"
        );
        let partial = hover("PartialEq)]");
        assert!(
            partial.starts_with("```vilan\nmod std::compare\nimpl Account with PartialEq\n```"),
            "{partial}"
        );
        // The two-facet import leaf: the trait first, then the macro.
        let leaf = hover("Storable, Store }");
        assert!(
            leaf.starts_with("```vilan\nmod std::reactive::store_core\ntrait Storable {"),
            "{leaf}"
        );
        assert!(
            leaf.contains("Also a derive macro: `[derive(Storable)]`."),
            "{leaf}"
        );
        // A user macro attribute: its signature, under its `mod` line.
        let tag = hover("derive_tag]");
        assert_eq!(
            tag,
            "```vilan\nmod pkg::client\nmacro fun derive_tag(item: Item): Source\n```"
        );
    });
}

/// E312: a derive whose expansion is REFUSED leads with the refusal.
#[test]
fn e312_a_refused_derive_leads_with_the_refusal() {
    on_big_stack(|| {
        let client = "[derive(PartialEq)]\nstruct Hook {\n\trun: || void,\n}\n\nfun main() {}\n";
        let package = Package::new("e312-refused", &[("client.vl", client)]);
        let world = package.world();
        let refusal = world
            .diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.span.start <= at(client, "PartialEq", 0)
                    && at(client, "PartialEq", 0) < diagnostic.span.end
            })
            .expect("the derive is refused at its name")
            .msg
            .clone();
        let hover = world
            .hover(at(client, "PartialEq", 2))
            .expect("the derive hovers");
        assert!(hover.starts_with(&refusal), "{hover}");
        assert!(
            hover.contains("macro fun PartialEq(item: Item): Source"),
            "{hover}"
        );
        assert!(hover.contains("impl Hook with PartialEq"), "{hover}");
    });
}

/// E306: an enum variant in VALUE position — kolt `views.vl:218:32`'s
/// `style::Overflow::Hidden` inside a `const` chain is the first pin —
/// hovers with its variant line and the enum's chunk; a payload variant and a
/// generic enum's variant likewise; and the enum-typed bindings E237's table
/// missed in a view (a call's result, a parameter, a capture, a `mut`)
/// carry the chunk.
#[test]
fn e306_a_variant_in_value_position_hovers_with_its_enum() {
    on_big_stack(|| {
        let shapes = "import std::web::style::Style;\n\n\
             /// How a shape is drawn.\nexport enum Shape {\n\t/// A round one.\n\tCircle(f64),\n\tEmpty,\n}\n\n\
             export fun styled(): Style {\n\tconst style::style()\n\t\t.overflow(style::Overflow::Hidden)\n}\n\n\
             export fun made(): Shape {\n\tShape::Circle(1.0)\n}\n\n\
             export fun area(shape: Shape): f64 {\n\tmut current = made();\n\tcurrent = shape;\n\
             \tlet maybe = Some(current);\n\tmatch maybe {\n\t\tSome(let inner) => {\n\t\t\tlet _ = inner;\n\t\t\t1.0\n\t\t}\n\t\tNone => 0.0,\n\t}\n}\n\n\
             export fun nothing(): Option<i32> {\n\tNone\n}\n";
        let client = "import pkg::shapes::{ area, made, nothing, styled };\n\n\
             fun main() {\n\tlet _ = styled();\n\tlet _ = area(made());\n\tlet _ = nothing();\n}\n";
        let package = Package::new("e306", &[("client.vl", client), ("shapes.vl", shapes)]);
        let world = package.world();
        assert!(world.diagnostics.is_empty(), "{:?}", world.diagnostics);
        let view = package.view(&world, "shapes.vl");
        let hover = |needle: &str, delta: usize| {
            view.hover(at(shapes, needle, delta))
                .unwrap_or_else(|| panic!("{needle:?}+{delta} hovers"))
        };
        let hidden = hover("Hidden)", 0);
        assert!(
            hidden.starts_with("```vilan\nOverflow::Hidden\n```"),
            "{hidden}"
        );
        assert!(
            hidden.contains("enum Overflow {\n\tVisible,\n\tHidden,"),
            "{hidden}"
        );
        let circle = hover("Circle(1.0)", 0);
        assert!(
            circle.starts_with("```vilan\nShape::Circle(f64)\n```\n\nA round one."),
            "{circle}"
        );
        assert!(circle.contains("enum Shape {"), "{circle}");
        let none = hover("None\n}", 0);
        // A generic enum's variant: its instantiation where the value's type
        // was tabled, the bare enum where not (a returned `None`), and the
        // enum's chunk either way.
        assert!(
            none.starts_with("```vilan\nOption") && none.contains("::None\n```"),
            "{none}"
        );
        assert!(none.contains("enum Option<T> {"), "{none}");
        // The bindings: a parameter, a `mut` from a call, a capture.
        for (needle, delta) in [("shape: Shape", 0), ("current = made", 0), ("inner) =>", 0)] {
            let binding = hover(needle, delta);
            assert!(binding.contains("enum Shape {"), "{needle}: {binding}");
        }
    });
}

/// E306's kolt sweep: every enum-typed binding and parameter of kolt's own
/// files, served as views, hovers with its enum's chunk.
#[test]
#[ignore = "E306: the kolt enum-binding sweep - needs VILAN_EDITOR_KOLT, run deliberately"]
fn kolt_every_enum_binding_hovers_with_its_chunk() {
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
        let (mut checked, mut missing) = (0, Vec::new());
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
            let program = document.program.as_ref().expect("a program");
            let source_of = program.source_lookup();
            let bindings = program
                .variables
                .iter()
                .map(|(id, variable)| (*id, variable.name_span, variable.type_id))
                .chain(program.parameters.iter().filter_map(|(id, parameter)| {
                    program
                        .span_map
                        .get(id)
                        .map(|span| (*id, **span, parameter.type_id))
                }));
            for (id, span, type_id) in bindings {
                if source_of.of(id) != Some(document.focus) || span.start >= span.end {
                    continue;
                }
                let Some(vilan_core::type_::Type::Enum(enum_id, _)) =
                    program.type_id_to_type_map.get(&type_id)
                else {
                    continue;
                };
                // `bool` is an enum underneath, and hovers as the primitive.
                if program.bool_enum_id == Some(*enum_id)
                    || program
                        .enums
                        .get(enum_id)
                        .is_none_or(|enumeration| enumeration.variants.is_empty())
                {
                    continue;
                }
                checked += 1;
                let hover = document.hover(span.start).unwrap_or_default();
                if !hover.contains("enum ") {
                    let line = text[..span.start].matches('\n').count() + 1;
                    missing.push(format!(
                        "{}:{line} {:?}: {hover:?}",
                        path.display(),
                        &text[span.into_range()]
                    ));
                }
            }
        }
        println!("E306 sweep: {checked} enum-typed bindings checked");
        assert!(missing.is_empty(), "{}", missing.join("\n"));
    });
}

/// E300 (RULED shape): a match arm's variant hovers with what the match is
/// over, the variant as the leg binds it, and the captures as declarations —
/// kolt `views.vl:187`'s `Some(let root)` over an `Option` of a nominal with
/// arguments; a tuple payload with two captures; `_`; an `is`; a variant
/// with a `///`.
#[test]
fn e300_a_match_arm_hovers_with_its_subject_and_captures() {
    on_big_stack(|| {
        let source = "struct Remote<P> {\n\tvalue: P,\n}\n\n\
             enum Shape {\n\t/// Two sides.\n\tRect(f64, f64),\n\tEmpty,\n}\n\n\
             fun global(): Option<Remote<str>> {\n\tSome(Remote { value = \"g\" })\n}\n\n\
             fun main() {\n\tmatch global() {\n\t\tSome(let root) => {\n\t\t\tlet _ = root;\n\t\t}\n\t\tNone => {}\n\t}\n\
             \tmatch Shape::Rect(1.0, 2.0) {\n\t\tShape::Rect(let w, mut h) => {\n\t\t\th = h + w;\n\t\t\tlet _ = h;\n\t\t}\n\t\t_ => {}\n\t}\n\
             \tif global() is Some(let found) {\n\t\tlet _ = found;\n\t}\n}\n";
        let package = Package::new("e300", &[("client.vl", source)]);
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let hover = |needle: &str, delta: usize| {
            document
                .hover(at(source, needle, delta))
                .unwrap_or_else(|| panic!("{needle:?} hovers"))
        };
        assert_eq!(
            hover("Some(let root)", 1),
            "```vilan\nmatch on Option<Remote<str>>\nOption<Remote<str>>::Some(Remote<str>)\nlet root: Remote<str>\n```"
        );
        assert_eq!(
            hover("Rect(let w", 1),
            "```vilan\nmatch on Shape\nShape::Rect(f64, f64)\nlet w: f64\nmut h: f64\n```\n\nTwo sides."
        );
        assert_eq!(
            hover("_ => {}", 0),
            "```vilan\nmatch on Shape\n_: Shape\n```"
        );
        assert_eq!(
            hover("Some(let found)", 1),
            "```vilan\nmatch on Option<Remote<str>>\nOption<Remote<str>>::Some(Remote<str>)\nlet found: Remote<str>\n```"
        );
        assert_eq!(
            hover("None => {}", 0),
            "```vilan\nmatch on Option<Remote<str>>\nOption<Remote<str>>::None\n```"
        );
    });
}

/// E301: an inlay hint's tooltip is what its binding's name hovers, as
/// markdown — kolt `views.vl:187`'s `Some(let root)` capture hint — or a
/// stage's `… as T` reading; the type it names resolves to its definition.
#[test]
fn e301_an_inlay_hint_resolves_to_its_bindings_hover_and_its_type() {
    on_big_stack(|| {
        let source = "/// A remote root.\nstruct Remote<P> {\n\tvalue: P,\n}\n\n\
             fun global(): Option<Remote<str>> {\n\tSome(Remote { value = \"g\" })\n}\n\n\
             fun main() {\n\tmatch global() {\n\t\tSome(let root) => {\n\t\t\tlet _ = root;\n\t\t}\n\t\tNone => {}\n\t}\n\
             \tlet doubled = Some(5)\n\t\t.map(|x| x * 2)\n\t\t.is_some();\n\tlet _ = doubled;\n}\n";
        let package = Package::new("e301", &[("client.vl", source)]);
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let root_end = at(source, "root) =>", 4);
        let (markdown, location) = document.inlay_hint_detail(root_end, ": Remote<str>", None);
        assert!(
            markdown.starts_with("```vilan\nlet root: Remote<str>\n```"),
            "{markdown}"
        );
        assert!(markdown.contains("A remote root."), "{markdown}");
        let (source_id, span) = location.expect("the type's definition");
        assert_eq!(source_id, document.focus);
        assert_eq!(&source[span.into_range()], "Remote");
        // A stage hint: the stage's reading, then what the type is.
        let stage_end = at(source, ".map(|x| x * 2)", ".map(|x| x * 2)".len());
        let (stage, _) = document.inlay_hint_detail(stage_end, " as Option<i32>", None);
        assert!(
            stage.starts_with("```vilan\n… as Option<i32>\n```"),
            "{stage}"
        );
    });
}

/// E302: the owner's four-line example under `vilan.inlayHints.chains =
/// "changes"` — the stages whose printed type moves are hinted, the one that
/// keeps `Option<i32>` is not; an all-same chain shows its first hint only;
/// `"every"` (the default) is E278's answer unchanged.
#[test]
fn e302_changes_hints_only_a_stage_whose_type_moves() {
    use crate::keystroke::{ChainHints, chain_hints};
    on_big_stack(|| {
        let source = "fun main() {\n\tlet some = Some(5)\n\t\t.map(|x| x * 2)\n\t\t.and_then(|x| Some(x + 1))\n\t\t.is_some();\n\
             \tlet same = Some(1)\n\t\t.map(|x| x + 1)\n\t\t.map(|x| x + 2)\n\t\t.map(|x| x + 3);\n\
             \tlet _ = some;\n\tlet _ = same;\n}\n";
        let document = Document::analyze(source, &std_root(), Path::new("/e302/main.vl"));
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let line = |offset: usize| source[..offset].matches('\n').count() + 1;
        let stages = |mode: ChainHints| -> Vec<(usize, String)> {
            chain_hints(document.keystroke_hints_served(false, true), mode)
                .into_iter()
                .filter(|hint| hint.label.starts_with(" as "))
                .map(|hint| (line(hint.offset), hint.label))
                .collect()
        };
        let every = stages(ChainHints::Every);
        let changes = stages(ChainHints::Changes);
        assert!(
            every.contains(&(3, " as Option<i32>".to_string())),
            "{every:?}"
        );
        assert!(
            every.contains(&(4, " as Option<i32>".to_string())),
            "{every:?}"
        );
        assert!(every.contains(&(5, " as bool".to_string())), "{every:?}");
        // The first chain: the and_then keeps `Option<i32>` and is hidden.
        assert!(
            changes.contains(&(3, " as Option<i32>".to_string())),
            "{changes:?}"
        );
        assert!(
            !changes.iter().any(|(at_line, _)| *at_line == 4),
            "{changes:?}"
        );
        assert!(
            changes.contains(&(5, " as bool".to_string())),
            "{changes:?}"
        );
        // The all-same chain: its first stage only.
        let same: Vec<&(usize, String)> = changes
            .iter()
            .filter(|(at_line, _)| (7..=9).contains(at_line))
            .collect();
        assert_eq!(same.len(), 1, "{changes:?}");
    });
}

/// B620's editor half: inside an opening tag, completion offers the methods
/// that hand the element back (and attributes and `on:` hooks); a method that
/// does not appears only to a typed prefix of three characters that it
/// extends — E213's rule, the same function — with the steer as its detail.
/// After the finished element (`<div />.`) every method is offered.
#[test]
fn b620_an_element_head_offers_a_non_view_method_only_to_three_typed_characters() {
    on_big_stack(|| {
        let base = "impl View {\n\tfun measured(self): usize {\n\t\t0\n\t}\n}\n\n\
             fun main() {\n\tlet _ = <div ¦ />;\n}\n";
        let labels = |typed: &str| -> Vec<(String, Option<String>)> {
            let text = base.replace('¦', typed);
            let offset = text.find(" />").expect("the tag's end");
            let package = Package::new("b620", &[("client.vl", &text)]);
            let document = package.world();
            document
                .completion(offset)
                .into_iter()
                .map(|candidate| (candidate.label, candidate.admission))
                .collect()
        };
        let view_returning = |items: &[(String, Option<String>)], name: &str| {
            items
                .iter()
                .any(|(label, admission)| label == name && admission.is_none())
        };
        // Nothing typed: `.styled` (returns View) offered, `.measured`
        // (returns usize) not; attributes and hooks are.
        let bare = labels("");
        assert!(view_returning(&bare, ".styled"), "{bare:?}");
        assert!(
            !bare.iter().any(|(label, _)| label == ".measured"),
            "{bare:?}"
        );
        assert!(bare.iter().any(|(label, _)| label == "title"), "{bare:?}");
        assert!(
            bare.iter().any(|(label, _)| label.starts_with("on:")),
            "{bare:?}"
        );
        // Two characters: still hidden.
        let two = labels(".me");
        assert!(!two.iter().any(|(label, _)| label == "measured"), "{two:?}");
        // Three: offered, with the steer.
        let three = labels(".mea");
        let offered = three
            .iter()
            .find(|(label, _)| label == "measured")
            .unwrap_or_else(|| panic!("offered at three characters: {three:?}"));
        assert_eq!(
            offered.1.as_deref(),
            Some(
                "returns `usize`, not the element: call it on the element instead, `<div />.measured()`"
            )
        );
        // After the finished element, the ordinary member list: offered.
        let after = "impl View {\n\tfun measured(self): usize {\n\t\t0\n\t}\n}\n\n\
             fun main() {\n\tlet _ = <div />.;\n}\n";
        let package = Package::new("b620-after", &[("client.vl", after)]);
        let document = package.world();
        let members: Vec<String> = document
            .completion(after.find("/>.").unwrap() + 3)
            .into_iter()
            .map(|candidate| candidate.label)
            .collect();
        assert!(
            members.iter().any(|label| label == "measured"),
            "{members:?}"
        );
    });
}

/// E313 under the incremental path (S1's hot set, S7b's served items): after
/// a keystroke in ONE module re-analyzes the world with that module hot, a
/// COLD module served as a view still answers its own pattern, call-label
/// and definition rows.
#[test]
fn e313_a_cold_view_keeps_its_rows_on_a_served_keystroke() {
    on_big_stack(|| {
        let cold = "export struct Boxed<T> {\n\tvalue: T,\n}\n\n\
             impl Boxed<type T> {\n\tfun get(self): T {\n\t\tself.value\n\t}\n}\n\n\
             export fun pick(value: Option<i32>): usize {\n\tlet boxed = Boxed { value = [1] };\n\
             \tmatch value {\n\t\tNone => boxed.get().len(),\n\t\tSome(let inner) => {\n\t\t\tlet _ = inner;\n\t\t\t1\n\t\t}\n\t}\n}\n";
        let hot = "export fun count(): i32 {\n\t1\n}\n";
        let client = "import pkg::cold::pick;\nimport pkg::hot::count;\n\n\
             fun main() {\n\tlet _ = pick(None);\n\tlet _ = count();\n}\n";
        let package = Package::new(
            "e313-served",
            &[("client.vl", client), ("cold.vl", cold), ("hot.vl", hot)],
        );
        let entry = package.path("client.vl");
        let seeds = vec![vilan_core::util::canonical_path(package.path("hot.vl"))];
        let analyze = || {
            Document::analyze_cancellable_editing(
                client,
                &std_root(),
                &entry,
                seeds.clone(),
                &vilan_core::cancel::CancelToken::new(),
            )
            .expect("an analysis")
        };
        // The first stores the world; the keystroke's is served from it.
        let _stored = analyze();
        std::fs::write(
            package.path("hot.vl"),
            "export fun count(): i32 {\n\t2\n}\n",
        )
        .unwrap();
        let served = analyze();
        assert!(served.diagnostics.is_empty(), "{:?}", served.diagnostics);
        let view = package.view(&served, "cold.vl");
        let none = view.hover(at(cold, "None =>", 1)).expect("the leg hovers");
        assert!(none.contains("Option<i32>::None"), "{none}");
        let get = view.hover(at(cold, "get().len", 0)).expect("`get` hovers");
        assert!(get.contains("fun get(self): List<i32>"), "{get}");
        let boxed = view.hover(at(cold, "boxed = ", 0)).expect("`boxed` hovers");
        assert!(
            boxed.contains("struct Boxed<List<i32>> {") || boxed.contains("value: List<i32>"),
            "{boxed}"
        );
    });
}

/// E315: a TYPE in the MIDDLE of a module-qualified value path — `Length` in
/// `style::Length::px(4)`, `Overflow` in `style::Overflow::Hidden` (kolt
/// `views.vl:218`), `Overlay` in a three-module `lib::deep::Overlay::fresh()`
/// — records the type-reference row a head (`Theme::stroke1()`) records: the
/// segment paints Struct / Enum and hovers with the declaration.
#[test]
fn e315_a_type_in_the_middle_of_a_module_path_paints_and_hovers() {
    on_big_stack(|| {
        let client = "import std::web::style::Style;\n\
             import pkg::lib;\n\n\
             struct Theme {}\n\nimpl Theme {\n\tfun width(): i32 {\n\t\t4\n\t}\n}\n\n\
             fun main() {\n\
             \tlet a = style::Length::px(4);\n\
             \tlet b = style::Overflow::Hidden;\n\
             \tlet c = lib::deep::Overlay::fresh();\n\
             \tlet d = lib::deep::Mode::On;\n\
                          \tlet e = Theme::width();\n\
             \tlet _ = (a, b, c, d, e);\n}\n";
        let deep = "/// An overlay.\nexport struct Overlay {\n\topen: bool,\n}\n\n\
             export impl Overlay {\n\tfun fresh(): Overlay {\n\t\tOverlay { open = false }\n\t}\n}\n\n\
             /// How it shows.\nexport enum Mode {\n\tOn,\n\tOff,\n}\n";
        let package = Package::new("e315", &[("client.vl", client)]);
        std::fs::create_dir_all(package.path("inner")).unwrap();
        std::fs::write(package.path("inner/deep.vl"), deep).unwrap();
        std::fs::write(package.path("lib.vl"), "export import pkg::inner::deep;\n").unwrap();
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let pair = |word: &str, kind: &str| (word.to_string(), kind.to_string());
        assert_eq!(
            tokens_over(&document, "style::Length::px(4)"),
            vec![
                pair("style", "Namespace"),
                pair("Length", "Struct"),
                pair("px", "Function")
            ]
        );
        assert_eq!(
            tokens_over(&document, "style::Overflow::Hidden"),
            vec![
                pair("style", "Namespace"),
                pair("Overflow", "Enum"),
                pair("Hidden", "EnumMember")
            ]
        );
        assert_eq!(
            tokens_over(&document, "lib::deep::Overlay::fresh()"),
            vec![
                pair("lib", "Namespace"),
                pair("deep", "Namespace"),
                pair("Overlay", "Struct"),
                pair("fresh", "Function")
            ]
        );
        assert_eq!(
            tokens_over(&document, "lib::deep::Mode::On"),
            vec![
                pair("lib", "Namespace"),
                pair("deep", "Namespace"),
                pair("Mode", "Enum"),
                pair("On", "EnumMember")
            ]
        );
        // A head type, as before.
        assert_eq!(
            tokens_over(&document, "Theme::width()"),
            vec![pair("Theme", "Struct"), pair("width", "Function")]
        );
        // The segment's hover is the declaration.
        let hover = |needle: &str| {
            document
                .hover(at(client, needle, 0))
                .unwrap_or_else(|| panic!("{needle:?} hovers"))
        };
        let length = hover("Length::px");
        assert!(length.contains("struct Length"), "{length}");
        let overflow = hover("Overflow::Hidden");
        assert!(overflow.contains("enum Overflow {"), "{overflow}");
        let overlay = hover("Overlay::fresh");
        assert!(overlay.contains("struct Overlay {"), "{overlay}");
        assert!(overlay.contains("An overlay."), "{overlay}");
        let mode = hover("Mode::On");
        assert!(mode.contains("enum Mode {"), "{mode}");
        // The leaf keeps its own answer.
        assert!(hover("px(4)").contains("fun px("), "{}", hover("px(4)"));
        assert!(
            hover("Hidden;").contains("Overflow::Hidden"),
            "{}",
            hover("Hidden;")
        );
    });
}

/// E315's neighbours: the same segment in a TYPE annotation
/// (`x: style::Length`) paints too, and find-references from the segment
/// reaches every spelling of the type — the head, the middle of a value path
/// and an annotation — with rename editing exactly the identifier.
#[test]
fn e315_a_middle_segment_is_a_reference_of_its_type() {
    on_big_stack(|| {
        let client = "import std::web::style::Style;\n\n\
             struct Theme {}\n\nimpl Theme {\n\tfun width(): i32 {\n\t\t4\n\t}\n}\n\n\
             fun measured(x: style::Length): style::Length {\n\tx\n}\n\n\
             fun main() {\n\
             \tlet a = style::Length::px(4);\n\
             \tlet b = Theme::width();\n\
             \tlet _ = (measured(a), b);\n}\n";
        let package = Package::new("e315b", &[("client.vl", client)]);
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let pair = |word: &str, kind: &str| (word.to_string(), kind.to_string());
        assert_eq!(
            tokens_over(&document, "fun measured(x: style::Length): style::Length"),
            vec![
                pair("measured", "Function"),
                pair("x", "Parameter"),
                pair("style", "Namespace"),
                pair("Length", "Struct"),
                pair("style", "Namespace"),
                pair("Length", "Struct"),
            ]
        );
        // Find-references from the middle segment of the value path: the
        // two annotations and the path itself, each exactly `Length`.
        let text = document.analyzed_text().to_string();
        let found: Vec<String> = document
            .references(at(client, "Length::px", 0))
            .into_iter()
            .filter(|(source, _)| *source == document.focus)
            .map(|(_, span)| text[span.into_range()].to_string())
            .collect();
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(found.iter().all(|word| word == "Length"), "{found:?}");
    });
}

/// E316: a function's signature label prints an IMPLICIT parameter's bound -
/// `username: Signal<str>` - not the binder's bare name (`Signal`). Two
/// parameters of one trait at different arguments print apart; an explicit
/// `<S: Shows<str>>` keeps its `S`.
#[test]
fn e316_an_implicit_parameter_prints_its_trait_arguments() {
    on_big_stack(|| {
        let client = "trait Shows<T> {\n\tfun show(self): T;\n}\n\n\
             struct Name {}\n\nstruct Count {}\n\n\
             impl Name with Shows<str> {\n\tfun show(self): str {\n\t\t\"n\"\n\t}\n}\n\n\
             impl Count with Shows<i32> {\n\tfun show(self): i32 {\n\t\t1\n\t}\n}\n\n\
             fun login(username: Shows<str>, tries: Shows<i32>): i32 {\n\ttries.show()\n}\n\n\
             fun single(username: Shows<str>): str {\n\tusername.show()\n}\n\n\
             fun explicit<S: Shows<str>>(value: S, other: Shows<i32>): str {\n\tvalue.show()\n}\n\n\
             fun main() {\n\tlet _ = login(Name {}, Count {});\n\tlet _ = single(Name {});\n\
             \tlet _ = explicit(Name {}, Count {});\n}\n";
        let package = Package::new("e316", &[("client.vl", client)]);
        let document = package.world();
        assert!(
            document.diagnostics.is_empty(),
            "{:?}",
            document.diagnostics
        );
        let hover = |needle: &str| {
            document
                .hover(at(client, needle, 0))
                .unwrap_or_else(|| panic!("{needle:?} hovers"))
        };
        for needle in ["login(username", "login(Name"] {
            let label = hover(needle);
            assert!(
                label.contains("fun login(username: Shows<str>, tries: Shows<i32>): i32"),
                "{needle}: {label}"
            );
        }
        let single = hover("single(username");
        assert!(
            single.contains("fun single(username: Shows<str>): str"),
            "{single}"
        );
        let explicit = hover("explicit<S");
        assert!(
            explicit.contains("fun explicit<S: Shows<str>>(value: S, other: Shows<i32>): str"),
            "{explicit}"
        );
    });
}
