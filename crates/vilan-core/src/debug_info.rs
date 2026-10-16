//! The `.vdbg.json` debug-info sidecar (debugging.md §5.3 door D1, E257 S7).
//!
//! A source map says where a JavaScript position came from; it says nothing
//! about what a JavaScript VALUE is. `Point { x = 1, y = 2 }` is `[1, 2]` on
//! the JS backend, `Some(5)` is `[0, 5]`, a `SignalCell<i32>` is an array
//! holding a `Shared` and its subscribers — so a debugger that reads only the
//! map shows JS-shaped values (S6). The sidecar is the rest: written beside the
//! map under the same switches (the `debug` preset, `[build] source-maps`,
//! `--source-maps`), it carries, for every function the user's files declare,
//! each parameter's and local's vilan name, the name its JavaScript binding
//! got, and its static type; and, per type, the layout `dbg`'s printer reads
//! it by (debugging.md §2, [`crate::printer::shape_of`]). `vilan-dap` (the
//! extension's adapter) reads it to print the Variables and Watch panes the
//! way `dbg` prints a value.
//!
//! **How a frame is found.** A JavaScript frame names its function (`area`, or
//! `first` for the one body two instances of a generic share) and where the
//! function starts; through the map, the start is the vilan function's NAME
//! (a function head's segment, S5) or a closure's first character. A frame
//! record carries both keys: `js`, the function's emitted name (the map's
//! `names` hold the vilan ones), and `at`, the head's original position in the
//! map's own coordinates (a `sources` index, a 0-based line, a 0-based UTF-16
//! column). `main`'s body is inlined at the script's top level (B214), so its
//! record is marked `main` and matches the module's own frame. Its locals are
//! the module scope's `const`s. An async `main` runs in an arrow the top level
//! calls; that arrow's head is marked at `main`, and the record carries the
//! place, so the frame is found as a closure's is (E331).
//!
//! **What a binding's value reads as.** Most bindings hold their value. A
//! local a view was taken of is BOXED (`[value]`, read through `[0]`); a scalar
//! view is the `(base, key)` pair the transformer reads as `base[key]`; a
//! `lazy` parameter holds a thunk that reading would force. Each row says
//! which, so the adapter reads what `dbg` would print — never the
//! representation.
//!
//! **Instances.** A generic function is one JavaScript function per
//! instantiation, except where two instances' bodies print identically and
//! share one (M16). A shared body's binding whose type differs between the
//! instances it serves has no type in its row: the adapter shows that one as
//! JavaScript, which is the honest answer.
//!
//! Only the user's own files are described: std's functions are ignore-listed
//! in the map, and a debugger never stops in them by default.

use crate::analyzer::{BackingValue, DERIVED_SOURCE, Program, SourceId};
use crate::fx::FxHashMap as HashMap;
use crate::id::Id;
use crate::source_map::{SourceMap, json_string, line_breaks, utf16_length};

/// The sidecar's format version. A reader refuses a version it does not know.
pub const SIDECAR_VERSION: u32 = 1;

/// The sidecar beside `script` (`main.mjs` → `main.mjs.vdbg.json`), as the
/// map is `main.mjs.map`.
pub fn sidecar_file_name(script: &str) -> String {
    format!("{script}.vdbg.json")
}

/// One build's debug information: the frames and the type table their rows
/// point into.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DebugInfo {
    pub frames: Vec<DebugFrame>,
    pub types: Vec<DebugType>,
}

/// What kind of JavaScript frame a record describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    /// A function declaration (`function area(shape) { .. }`), including each
    /// emitted instance of a generic one.
    Function,
    /// A closure (`(a, b) => { .. }`).
    Closure,
    /// `main`, whose body is the script's top level.
    Main,
}

impl FrameKind {
    fn as_str(self) -> &'static str {
        match self {
            FrameKind::Function => "function",
            FrameKind::Closure => "closure",
            FrameKind::Main => "main",
        }
    }
}

/// One emitted function's bindings.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugFrame {
    pub kind: FrameKind,
    /// The vilan function's name; `main`; or `a closure in main`.
    pub name: String,
    /// The emitted function's name — `None` for a closure, an arrow with
    /// none of its own, and for `main`.
    pub js: Option<String>,
    /// Where the head maps to, in the map's coordinates: (`sources` index,
    /// 0-based line, 0-based UTF-16 column). `None` for a synchronous `main`; an
    /// async one runs in an arrow whose head is marked at `main` (E331).
    pub at: Option<(u32, u32, u32)>,
    pub bindings: Vec<DebugBinding>,
}

/// How a binding's JavaScript value is read to get the vilan value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingRead {
    /// The binding holds the value.
    Value,
    /// A one-slot cell (`[value]`): a local a view was taken of.
    Boxed,
    /// A scalar view's `(base, key)` pair: the value is `base[key]`.
    ViewPair,
    /// A `lazy` parameter's thunk: reading it would force it.
    Lazy,
}

impl BindingRead {
    fn as_str(self) -> &'static str {
        match self {
            BindingRead::Value => "value",
            BindingRead::Boxed => "boxed",
            BindingRead::ViewPair => "pair",
            BindingRead::Lazy => "lazy",
        }
    }
}

/// One parameter or local.
#[derive(Clone, Debug, PartialEq)]
pub struct DebugBinding {
    pub name: String,
    /// The JavaScript binding's name in the emitted script.
    pub js: String,
    /// Its type's index in [`DebugInfo::types`]; `None` where the frame's
    /// instances disagree on it, or the build never grounded it.
    pub type_index: Option<u32>,
    pub parameter: bool,
    /// A view (`view T`): a `&`/`&mut` parameter or a view binding.
    pub view: bool,
    pub read: BindingRead,
}

/// A type as the printer lays it out (debugging.md §2.1), with what the JS
/// backend stores it as. Every index is into [`DebugInfo::types`].
#[derive(Clone, Debug, PartialEq)]
pub struct DebugType {
    /// The type as vilan writes it: `List<Point>`, `|i32| i32`.
    pub text: String,
    pub layout: Layout,
}

/// [`crate::printer::Shape`], with its type ids grounded into the table and
/// the JS reading facts the generated printer bakes in (tuple widths, a list's
/// fill).
#[derive(Clone, Debug, PartialEq)]
pub enum Layout {
    Integer,
    Float,
    BigInt,
    Bool,
    Str,
    Void,
    /// Fixed text: a closure by its type, a pipe, an opaque host value.
    Text(String),
    /// A struct: its field array, in declaration order.
    Struct {
        name: String,
        fields: Vec<(String, u32)>,
    },
    /// `[tag, ...payload]`: each variant's label and payload types.
    Enum {
        variants: Vec<(String, Vec<u32>)>,
    },
    /// A backed enum: the value IS one of the literals.
    Backed {
        variants: Vec<(String, BackingValue)>,
    },
    /// Flat slots: each element's opening text, type, and slot width.
    Tuple(Vec<(String, u32, usize)>),
    /// `List<T>`, `[T; n]`; `fill` when the element prints as a scalar (E277).
    List {
        element: u32,
        fill: bool,
    },
    /// `Shared<T>`: `{ v }`.
    Shared(u32),
    /// std's `SignalCell<T>`: the `Shared` at field `field`.
    Cell {
        label: String,
        field: usize,
        value: u32,
    },
    /// std's `HashMap<K, V>`: the JS `Map` at field `field`, whose values are
    /// the flat `(K, V)` pairs.
    Map {
        label: String,
        field: usize,
        key: u32,
        value: u32,
        key_width: usize,
        value_width: usize,
    },
    /// std's `HashSet<T>`: the JS `Map` at field `field`.
    Set {
        label: String,
        field: usize,
        element: u32,
        fill: bool,
    },
    /// A trait object, `[value, table]`: printed by the table's `show` slot
    /// when the program's tables carry one.
    Object {
        label: String,
    },
}

/// A frame as the walk recorded it, before the scope rename and the map exist.
#[derive(Clone, Debug)]
pub(crate) struct RecordedFrame {
    pub kind: FrameKind,
    pub name: String,
    /// The name the generator minted for the function (renamed later).
    pub js: Option<String>,
    /// The head's place: its source and a byte offset into it.
    pub origin: Option<(SourceId, usize)>,
    pub bindings: Vec<RecordedBinding>,
    /// Merged into the instance whose body it turned out to share (M16).
    pub dropped: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct RecordedBinding {
    pub id: Id,
    pub name: String,
    /// The minted name (renamed later).
    pub js: String,
    pub type_index: Option<u32>,
    pub parameter: bool,
    pub view: bool,
    pub read: BindingRead,
}

/// Everything the transformer recorded for one script.
#[derive(Clone, Debug, Default)]
pub(crate) struct RecordedDebug {
    pub frames: Vec<RecordedFrame>,
    pub types: Vec<DebugType>,
}

impl RecordedDebug {
    /// The scope rename's answers (`rename_for_scopes`), applied: a binding's
    /// and a function's minted name become the names the script declares.
    pub fn rename(&mut self, rename: &HashMap<String, String>) {
        for frame in &mut self.frames {
            if let Some(js) = &mut frame.js
                && let Some(renamed) = rename.get(js)
            {
                *js = renamed.clone();
            }
            for binding in &mut frame.bindings {
                if let Some(renamed) = rename.get(&binding.js) {
                    binding.js = renamed.clone();
                }
            }
        }
    }
}

/// Merges `dropped`'s rows into `survivor`'s: the two instances share one
/// body, so a binding's type is kept only where both agree on it.
pub(crate) fn merge_shared_frame(survivor: &mut RecordedFrame, dropped: &RecordedFrame) {
    for binding in &mut survivor.bindings {
        let agrees = dropped
            .bindings
            .iter()
            .find(|other| other.id == binding.id)
            .is_some_and(|other| {
                other.type_index == binding.type_index && other.read == binding.read
            });
        if !agrees {
            binding.type_index = None;
        }
    }
}

/// The finished sidecar: each frame's head placed in the map's coordinates.
/// A frame whose head the map does not reach (its source is not among the
/// map's) keeps no `at` — for a function the adapter still finds it by name.
pub(crate) fn finish(program: &Program, recorded: RecordedDebug, map: &SourceMap) -> DebugInfo {
    let frames: Vec<RecordedFrame> = recorded
        .frames
        .into_iter()
        .filter(|frame| !frame.dropped)
        .collect();
    // Each source's index in the map, by its canonical path.
    let mut slots: HashMap<SourceId, Option<u32>> = HashMap::default();
    let mut slot_of = |source: SourceId| -> Option<u32> {
        *slots.entry(source).or_insert_with(|| {
            let index = source.0 as usize;
            let path = program
                .canonical_sources
                .get(index)
                .or_else(|| program.sources.get(index))?;
            map.sources
                .iter()
                .position(|candidate| candidate.url.is_none() && candidate.path == *path)
                .map(|position| position as u32)
        })
    };
    // One sweep per source in offset order, as the map's own `locate` does.
    let mut anchored: Vec<(SourceId, usize, usize)> = frames
        .iter()
        .enumerate()
        .filter_map(|(index, frame)| frame.origin.map(|(source, offset)| (source, offset, index)))
        .collect();
    anchored.sort_unstable();
    let mut placed: Vec<Option<(u32, u32, u32)>> = vec![None; frames.len()];
    let mut sweep: Option<(SourceId, usize, u32, usize)> = None;
    for (source, offset, index) in anchored {
        let Some(text) = program
            .source_texts
            .iter()
            .find(|(candidate, _)| *candidate == source)
            .map(|(_, text)| *text)
        else {
            continue;
        };
        if offset > text.len() || !text.is_char_boundary(offset) {
            continue;
        }
        let Some(slot) = slot_of(source) else {
            continue;
        };
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
        placed[index] = Some((slot, line, utf16_length(&text[line_start..offset])));
    }
    DebugInfo {
        frames: frames
            .into_iter()
            .zip(placed)
            .map(|(frame, at)| DebugFrame {
                kind: frame.kind,
                name: frame.name,
                js: frame.js,
                at,
                bindings: frame
                    .bindings
                    .into_iter()
                    .map(|binding| DebugBinding {
                        name: binding.name,
                        js: binding.js,
                        type_index: binding.type_index,
                        parameter: binding.parameter,
                        view: binding.view,
                        read: binding.read,
                    })
                    .collect(),
            })
            .collect(),
        types: recorded.types,
    }
}

/// Whether a function or closure written in `source` is the user's — not std's
/// and not a derive's generated code.
pub(crate) fn is_user_source(program: &Program, source: SourceId) -> bool {
    source != DERIVED_SOURCE && !program.std_sources.contains(&source)
}

impl DebugInfo {
    /// The sidecar's JSON for the generated file `script` (its name, as the
    /// map's `file` names it).
    pub fn to_json(&self, script: &str) -> String {
        let mut json = String::with_capacity(256 + self.frames.len() * 128 + self.types.len() * 64);
        json.push_str("{\"version\":");
        json.push_str(&SIDECAR_VERSION.to_string());
        json.push_str(",\"file\":");
        json_string(&mut json, script);
        json.push_str(",\"frames\":[");
        for (index, frame) in self.frames.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str("\n{\"kind\":");
            json_string(&mut json, frame.kind.as_str());
            json.push_str(",\"name\":");
            json_string(&mut json, &frame.name);
            if let Some(js) = &frame.js {
                json.push_str(",\"js\":");
                json_string(&mut json, js);
            }
            if let Some((source, line, column)) = frame.at {
                json.push_str(&format!(",\"at\":[{source},{line},{column}]"));
            }
            json.push_str(",\"bindings\":[");
            for (index, binding) in frame.bindings.iter().enumerate() {
                if index > 0 {
                    json.push(',');
                }
                json.push_str("{\"name\":");
                json_string(&mut json, &binding.name);
                json.push_str(",\"js\":");
                json_string(&mut json, &binding.js);
                if let Some(type_index) = binding.type_index {
                    json.push_str(&format!(",\"type\":{type_index}"));
                }
                if binding.parameter {
                    json.push_str(",\"parameter\":true");
                }
                if binding.view {
                    json.push_str(",\"view\":true");
                }
                if binding.read != BindingRead::Value {
                    json.push_str(",\"read\":");
                    json_string(&mut json, binding.read.as_str());
                }
                json.push('}');
            }
            json.push_str("]}");
        }
        json.push_str("\n],\"types\":[");
        for (index, type_) in self.types.iter().enumerate() {
            if index > 0 {
                json.push(',');
            }
            json.push_str("\n{\"text\":");
            json_string(&mut json, &type_.text);
            json.push(',');
            type_.layout.write_json(&mut json);
            json.push('}');
        }
        json.push_str("\n]}\n");
        json
    }
}

impl Layout {
    /// The layout's fields, `"shape":…` first.
    fn write_json(&self, json: &mut String) {
        let shape = |json: &mut String, name: &str| {
            json.push_str("\"shape\":");
            json_string(json, name);
        };
        let label = |json: &mut String, label: &str| {
            json.push_str(",\"label\":");
            json_string(json, label);
        };
        match self {
            Layout::Integer => shape(json, "integer"),
            Layout::Float => shape(json, "float"),
            Layout::BigInt => shape(json, "bigint"),
            Layout::Bool => shape(json, "bool"),
            Layout::Str => shape(json, "str"),
            Layout::Void => shape(json, "void"),
            Layout::Text(text) => {
                shape(json, "text");
                json.push_str(",\"value\":");
                json_string(json, text);
            }
            Layout::Struct { name, fields } => {
                shape(json, "struct");
                json.push_str(",\"name\":");
                json_string(json, name);
                json.push_str(",\"fields\":[");
                for (index, (field, type_index)) in fields.iter().enumerate() {
                    if index > 0 {
                        json.push(',');
                    }
                    json.push('[');
                    json_string(json, field);
                    json.push_str(&format!(",{type_index}]"));
                }
                json.push(']');
            }
            Layout::Enum { variants } => {
                shape(json, "enum");
                json.push_str(",\"variants\":[");
                for (index, (variant, payload)) in variants.iter().enumerate() {
                    if index > 0 {
                        json.push(',');
                    }
                    json.push('[');
                    json_string(json, variant);
                    let payload: Vec<String> = payload.iter().map(u32::to_string).collect();
                    json.push_str(&format!(",[{}]]", payload.join(",")));
                }
                json.push(']');
            }
            Layout::Backed { variants } => {
                shape(json, "backed");
                json.push_str(",\"variants\":[");
                for (index, (variant, backing)) in variants.iter().enumerate() {
                    if index > 0 {
                        json.push(',');
                    }
                    json.push('[');
                    json_string(json, variant);
                    json.push(',');
                    match backing {
                        BackingValue::Int(value) => json.push_str(&value.to_string()),
                        BackingValue::Str(raw) => {
                            json_string(json, &crate::transformer::unescape_string(raw))
                        }
                    }
                    json.push(']');
                }
                json.push(']');
            }
            Layout::Tuple(elements) => {
                shape(json, "tuple");
                json.push_str(",\"elements\":[");
                for (index, (opens, type_index, width)) in elements.iter().enumerate() {
                    if index > 0 {
                        json.push(',');
                    }
                    json.push('[');
                    json_string(json, opens);
                    json.push_str(&format!(",{type_index},{width}]"));
                }
                json.push(']');
            }
            Layout::List { element, fill } => {
                shape(json, "list");
                json.push_str(&format!(",\"element\":{element}"));
                if *fill {
                    json.push_str(",\"fill\":true");
                }
            }
            Layout::Shared(value) => {
                shape(json, "shared");
                json.push_str(&format!(",\"value\":{value}"));
            }
            Layout::Cell {
                label: text,
                field,
                value,
            } => {
                shape(json, "cell");
                label(json, text);
                json.push_str(&format!(",\"field\":{field},\"value\":{value}"));
            }
            Layout::Map {
                label: text,
                field,
                key,
                value,
                key_width,
                value_width,
            } => {
                shape(json, "map");
                label(json, text);
                json.push_str(&format!(
                    ",\"field\":{field},\"key\":{key},\"value\":{value},\"keyWidth\":{key_width},\"valueWidth\":{value_width}"
                ));
            }
            Layout::Set {
                label: text,
                field,
                element,
                fill,
            } => {
                shape(json, "set");
                label(json, text);
                json.push_str(&format!(",\"field\":{field},\"element\":{element}"));
                if *fill {
                    json.push_str(",\"fill\":true");
                }
            }
            Layout::Object { label: text } => {
                shape(json, "object");
                label(json, text);
            }
        }
    }
}
