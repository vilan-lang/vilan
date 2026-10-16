//! E257 S7: what the walk records for the `.vdbg.json` sidecar
//! ([`crate::debug_info`]) — each emitted user function's bindings, typed
//! under the substitution the body is emitted with, and the printer layouts of
//! those types.
//!
//! Recording happens only on the map's own entry
//! ([`super::transform_with_source_map`]), which sets
//! [`Transformer::debug_recorder`]. It READS the program and the walk's
//! state and never emits: a type's layout is asked of
//! [`crate::printer::shape_of`] as the generated printers ask it, but no
//! printer is generated, so the script is the same bytes with or without a
//! sidecar (`tests/source_maps.rs` pins the identity over the corpus).

use super::Transformer;
use crate::analyzer::{Function, SourceId};
use crate::debug_info::{
    BindingRead, DebugType, FrameKind, Layout, RecordedBinding, RecordedDebug, RecordedFrame,
    is_user_source, merge_shared_frame,
};
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::node::Convention;
use crate::printer::{Shape, shape_of};
use crate::type_::{Type, TypeId};

/// The recording's state, lent out of the transformer while a frame is read.
#[derive(Default)]
pub(super) struct DebugRecorder {
    pub(super) recorded: RecordedDebug,
    /// A recorded function frame by its MINTED name — what a shared body (M16)
    /// is merged by.
    by_function: HashMap<String, usize>,
    /// A type's index by the printer's own key (the grounded type's identity
    /// plus the labels it prints).
    type_keys: HashMap<String, u32>,
    /// Built on the first frame: each scope's children, and the scopes a
    /// closure's body opens (a frame of its own, so a function's walk stops
    /// there).
    scope_children: Option<HashMap<Id, Vec<Id>>>,
    closure_scopes: HashSet<Id>,
}

impl DebugRecorder {
    /// Every scope by its parent, and the scopes closures' bodies open —
    /// built on the first frame, once per script. The whole table, std's
    /// included: a file's scopes are not all inside its id range (filtering
    /// by the user's ranges lost a match arm's capture, `Some(let view)`), so
    /// the user's cannot be picked out by id.
    fn index_scopes(&mut self, program: &crate::analyzer::Program) {
        if self.scope_children.is_some() {
            return;
        }
        let mut children: HashMap<Id, Vec<Id>> = HashMap::default();
        for scope in program.scopes.values() {
            if let Some(parent) = scope.parent_id {
                children.entry(parent).or_default().push(scope.id);
            }
        }
        self.scope_children = Some(children);
        self.closure_scopes = program
            .closures
            .values()
            .filter_map(|closure| program.entity_scope_map.get(&closure.return_).copied())
            .collect();
    }
}

impl<'src> Transformer<'src> {
    /// Records the function `function` just emitted as `js_name` — one
    /// instance of a generic one, under the active substitution.
    pub(super) fn record_function_frame(&mut self, function: &Function<'src>, js_name: &str) {
        let Some(mut recorder) = self.debug_recorder.take() else {
            return;
        };
        if let Some(source) = self.program.source_of(function.id)
            && is_user_source(self.program, source)
        {
            let origin = (function.name_span.start != function.name_span.end)
                .then_some((source, function.name_span.start));
            let extent = self.debug_extent(source, function.id);
            let bindings =
                self.frame_bindings(&mut recorder, &function.parameters, function.body.2);
            let index = recorder.recorded.frames.len();
            recorder.recorded.frames.push(RecordedFrame {
                kind: FrameKind::Function,
                name: function.name.to_string(),
                js: Some(js_name.to_string()),
                origin,
                extent,
                bindings,
                dropped: false,
            });
            recorder.by_function.insert(js_name.to_string(), index);
        }
        self.debug_recorder = Some(recorder);
    }

    /// Records `main`, whose body the script's top level runs (B214).
    pub(super) fn record_main_frame(&mut self, main: &Function<'src>) {
        let Some(mut recorder) = self.debug_recorder.take() else {
            return;
        };
        if let Some(source) = self
            .program
            .source_of(main.id)
            .filter(|source| is_user_source(self.program, *source))
        {
            let bindings = self.frame_bindings(&mut recorder, &main.parameters, main.body.2);
            // An async `main` runs in an arrow the script calls at its top
            // level, whose head is a mark at `main`'s span (E331): that
            // frame finds this record by its place, as a closure's does.
            let origin = self
                .program
                .async_functions
                .contains(&main.id)
                .then(|| {
                    let span = self.program.span_map.get(&main.id)?;
                    (span.start != span.end).then_some((source, span.start))
                })
                .flatten();
            let extent = self.debug_extent(source, main.id);
            recorder.recorded.frames.push(RecordedFrame {
                kind: FrameKind::Main,
                name: main.name.to_string(),
                js: None,
                origin,
                extent,
                bindings,
                dropped: false,
            });
        }
        self.debug_recorder = Some(recorder);
    }

    /// Records the closure `closure_id` just emitted.
    pub(super) fn record_closure_frame(&mut self, closure_id: Id) {
        let Some(mut recorder) = self.debug_recorder.take() else {
            return;
        };
        let program = self.program;
        if let Some(closure) = program.closures.get(&closure_id)
            && let Some(source) = program.source_of(closure_id)
            && is_user_source(program, source)
            && let Some(span) = program.span_map.get(&closure_id)
            && span.start != span.end
            && let Some(scope) = program.entity_scope_map.get(&closure.return_).copied()
        {
            let bindings = self.frame_bindings(&mut recorder, &closure.parameters, scope);
            let name = match self.current_origin {
                Some(owner) => format!("a closure in {owner}"),
                None => "a closure".to_string(),
            };
            recorder.recorded.frames.push(RecordedFrame {
                kind: FrameKind::Closure,
                name,
                js: None,
                origin: Some((source, span.start)),
                extent: Some((source, span.start, span.end)),
                bindings,
                dropped: false,
            });
        }
        self.debug_recorder = Some(recorder);
    }

    /// `id`'s whole declaration as the sidecar's `span` holds it: where an
    /// awaiting frame, which names only the place it stands, finds its record
    /// (E326).
    fn debug_extent(&self, source: SourceId, id: Id) -> Option<(SourceId, usize, usize)> {
        let span = self.program.span_map.get(&id)?;
        (span.start < span.end).then_some((source, span.start, span.end))
    }

    /// The instance minted as `dropped` shares `survivor`'s body (M16): its
    /// rows merge into the survivor's, a type kept only where both agree.
    pub(super) fn record_shared_frame(&mut self, dropped: &str, survivor: &str) {
        let Some(recorder) = self.debug_recorder.as_mut() else {
            return;
        };
        let (Some(dropped_index), Some(survivor_index)) = (
            recorder.by_function.remove(dropped),
            recorder.by_function.get(survivor).copied(),
        ) else {
            return;
        };
        let frames = &mut recorder.recorded.frames;
        frames[dropped_index].dropped = true;
        let dropped_frame = frames[dropped_index].clone();
        merge_shared_frame(&mut frames[survivor_index], &dropped_frame);
    }

    /// A frame's bindings: its parameters, then every local its scopes declare
    /// — not a nested closure's, which is a frame of its own — in declaration
    /// order. A binding the walk never named was never emitted, and has no
    /// row.
    fn frame_bindings(
        &mut self,
        recorder: &mut DebugRecorder,
        parameters: &[Id],
        root: Id,
    ) -> Vec<RecordedBinding> {
        let program = self.program;
        recorder.index_scopes(program);
        let mut declared: Vec<(Id, &'src str, bool)> = parameters
            .iter()
            .filter_map(|id| {
                program
                    .parameters
                    .get(id)
                    .map(|parameter| (*id, parameter.name, true))
            })
            .collect();
        let mut listed: HashSet<Id> = declared.iter().map(|(id, _, _)| *id).collect();
        let mut locals: Vec<(usize, Id, &'src str)> = Vec::new();
        let mut pending = vec![root];
        while let Some(scope_id) = pending.pop() {
            if let Some(scope) = program.scopes.get(&scope_id) {
                for (name, declarations) in &scope.local_value_declarations {
                    for declaration in declarations {
                        if (program.variables.contains_key(&declaration.id)
                            || program.parameters.contains_key(&declaration.id))
                            && listed.insert(declaration.id)
                        {
                            locals.push((declaration.visible_from, declaration.id, *name));
                        }
                    }
                }
            }
            if let Some(children) = recorder
                .scope_children
                .as_ref()
                .and_then(|children| children.get(&scope_id))
            {
                pending.extend(
                    children.iter().filter(|child| {
                        **child == root || !recorder.closure_scopes.contains(child)
                    }),
                );
            }
        }
        locals.sort_by_key(|(position, id, _)| (*position, id.0));
        declared.extend(locals.into_iter().map(|(_, id, name)| (id, name, false)));

        let mut bindings = Vec::with_capacity(declared.len());
        for (id, name, parameter) in declared {
            let Some(minted) = self.ng.names.get(&id).cloned() else {
                continue;
            };
            let declared_parameter = program.parameters.get(&id);
            let lazy = declared_parameter.is_some_and(|parameter| parameter.lazy);
            let pair = self.binding_holds_a_scalar_view_pair(id);
            let read = if lazy {
                BindingRead::Lazy
            } else if pair {
                BindingRead::ViewPair
            } else if self.local_is_boxed(id) {
                BindingRead::Boxed
            } else {
                BindingRead::Value
            };
            let view =
                pair || declared_parameter.is_some_and(|parameter| {
                    matches!(parameter.convention, Convention::Ref | Convention::RefMut)
                }) || program.payload_view_captures.contains_key(&id)
                    || program.for_each_views.get(&id) == Some(&true);
            let type_index = self
                .binding_type_id(id)
                .and_then(|type_id| self.debug_type(recorder, type_id));
            bindings.push(RecordedBinding {
                id,
                name: name.to_string(),
                js: minted,
                type_index,
                parameter,
                view,
                read,
            });
        }
        bindings
    }

    /// `type_id`'s index in the sidecar's type table, under the active
    /// substitution, adding it (and every type its layout reaches) on first
    /// ask — the walk `printer_for` makes, keyed the same way, minus the
    /// printer. `None` for a type the build never grounded.
    fn debug_type(&mut self, recorder: &mut DebugRecorder, type_id: TypeId) -> Option<u32> {
        let _guard = crate::util::RecursionGuard::enter()?;
        let written = type_id;
        let grounded = self.ground_printer_type(type_id);
        let program = self.program;
        if matches!(
            program.type_id_to_type_map.get(&grounded),
            None | Some(Type::Generic(_) | Type::Any | Type::Unknown)
        ) {
            return None;
        }
        let key = format!(
            "{}{}",
            self.type_key(grounded),
            crate::printer::label_key(program, written, &|type_id| self
                .ground_printer_type(type_id))
        );
        if let Some(index) = recorder.type_keys.get(&key) {
            return Some(*index);
        }
        let (shape, text) = {
            let resolve = |type_id| self.ground_printer_type(type_id);
            (
                shape_of(program, written, &resolve),
                crate::printer::type_text(program, grounded, &resolve),
            )
        };
        // Reserved BEFORE the layout is built: a recursive type's layout
        // points at itself.
        let index = recorder.recorded.types.len() as u32;
        recorder.type_keys.insert(key, index);
        recorder.recorded.types.push(DebugType {
            text,
            layout: Layout::Void,
        });
        let element = |transformer: &mut Self, recorder: &mut DebugRecorder, type_id| {
            transformer
                .debug_type(recorder, type_id)
                .unwrap_or_else(|| unknown_type(recorder))
        };
        let layout = match shape {
            Shape::Integer => Layout::Integer,
            Shape::Float => Layout::Float,
            Shape::BigInt => Layout::BigInt,
            Shape::Bool => Layout::Bool,
            Shape::Str => Layout::Str,
            Shape::Void => Layout::Void,
            Shape::Text(text) => Layout::Text(text),
            Shape::Struct {
                name,
                fields,
                bindings,
            } => {
                let saved = self.current_substitution.clone();
                self.current_substitution.extend(bindings);
                let fields = fields
                    .into_iter()
                    .map(|(field, field_type)| (field, element(self, recorder, field_type)))
                    .collect();
                self.current_substitution = saved;
                Layout::Struct { name, fields }
            }
            Shape::Enum { variants, bindings } => {
                let saved = self.current_substitution.clone();
                self.current_substitution.extend(bindings);
                let variants = variants
                    .into_iter()
                    .map(|(variant, payload)| {
                        let payload = payload
                            .into_iter()
                            .map(|payload_type| element(self, recorder, payload_type))
                            .collect();
                        (variant, payload)
                    })
                    .collect();
                self.current_substitution = saved;
                Layout::Enum { variants }
            }
            Shape::Backed { variants } => Layout::Backed { variants },
            Shape::Tuple(elements) => Layout::Tuple(
                elements
                    .into_iter()
                    .map(|(opens, element_type)| {
                        let width = self.flat_width(element_type);
                        (opens, element(self, recorder, element_type), width)
                    })
                    .collect(),
            ),
            Shape::List(element_type) => {
                let fill = self.prints_as_a_scalar(element_type);
                Layout::List {
                    element: element(self, recorder, element_type),
                    fill,
                }
            }
            Shape::Shared(inner) => Layout::Shared(element(self, recorder, inner)),
            Shape::Cell {
                label,
                field: (field, _),
                value,
            } => Layout::Cell {
                label,
                field,
                value: element(self, recorder, value),
            },
            Shape::Map {
                label,
                field: (field, _),
                key,
                value,
            } => {
                let key_width = self.flat_width(key);
                let value_width = self.flat_width(value);
                Layout::Map {
                    label,
                    field,
                    key: element(self, recorder, key),
                    value: element(self, recorder, value),
                    key_width,
                    value_width,
                }
            }
            Shape::Set {
                label,
                field: (field, _),
                element: element_type,
            } => {
                let fill = self.prints_as_a_scalar(element_type);
                Layout::Set {
                    label,
                    field,
                    element: element(self, recorder, element_type),
                    fill,
                }
            }
            Shape::Object { label } => Layout::Object { label },
        };
        recorder.recorded.types[index as usize].layout = layout;
        Some(index)
    }
}

/// The table's entry for a type the build never grounded — a generic a
/// shared body leaves open: it prints `<?>`, as the printer's own fallback.
fn unknown_type(recorder: &mut DebugRecorder) -> u32 {
    if let Some(index) = recorder.type_keys.get("<?>") {
        return *index;
    }
    let index = recorder.recorded.types.len() as u32;
    recorder.type_keys.insert("<?>".to_string(), index);
    recorder.recorded.types.push(DebugType {
        text: "?".to_string(),
        layout: Layout::Text("<?>".to_string()),
    });
    index
}
