//! `dbg(..)` on the JS backend (debugging.md S1): the call's lowering and the
//! generated printers.
//!
//! A printer is a top-level `function __show_<Type>(value)` per concrete type
//! a `dbg` reaches, answering the type's DOCUMENT: a string, or a group
//! `{ o, c, p, e }` (open text, close text, padded, `[label, document]`
//! entries) that `__dbg_layout` lays out one line or one entry per line.
//! [`crate::printer::shape_of`] says what each type prints as; this file only
//! says how a JS value of that shape is read — a struct is its field array, an
//! enum `[tag, ...payload]`, a tuple its flat slots.
//!
//! A generic body is emitted per instantiation already, so a `dbg(x)` with
//! `x: T` resolves `T` under the instance's substitution (Q6): the printer is
//! the concrete type's, and two instances calling different printers are two
//! bodies.

use std::borrow::Cow;

use super::{Transformer, js};
use crate::analyzer::DbgStackValue;
use crate::id::Id;
use crate::node::BinaryOp;
use crate::options::DbgPolicy;
use crate::printer::{Shape, shape_of};
use crate::type_::TypeId;

/// A `js::Node` string literal.
fn text<'src>(value: impl Into<String>) -> js::Node<'src> {
    js::Node::String(Cow::Owned(value.into()))
}

fn call<'src>(name: &str, arguments: Vec<js::Node<'src>>) -> js::Node<'src> {
    js::Node::Call(Box::new(js::Node::Local(name.to_string())), arguments)
}

/// `value[index]`.
fn slot<'src>(index: usize) -> js::Node<'src> {
    js::Node::PropertyIndex(
        Box::new(js::Node::Local("value".to_string())),
        Box::new(js::Node::Number(index.to_string(), None)),
    )
}

/// `__dbg_group(open, close, padded, [[label, document], ..])`.
fn group<'src>(
    open: &str,
    close: &str,
    padded: bool,
    entries: Vec<(String, js::Node<'src>)>,
) -> js::Node<'src> {
    call(
        "__dbg_group",
        vec![
            text(open),
            text(close),
            js::Node::Bool(padded),
            js::Node::Array(
                entries
                    .into_iter()
                    .map(|(label, document)| js::Node::Array(vec![text(label), document]))
                    .collect(),
            ),
        ],
    )
}

impl<'src> Transformer<'src> {
    /// `dbg(a, b, ..)` (debugging.md §3.2): prints `[file:line:col] expr =
    /// value` per argument to stderr (`console.log` in the browser, Q3) and
    /// answers the argument, a tuple of them for several, `()` for none (Q2).
    /// A statement reads its arguments in place and answers nothing; under
    /// `[build] dbg = "strip"` the call is its arguments alone.
    pub(super) fn dbg_call(
        &mut self,
        call_id: Id,
        argument_ids: &[Id],
        arguments: Vec<js::Node<'src>>,
    ) -> js::Node<'src> {
        let statement = self.program.dbg_statement_calls.contains(&call_id);
        let spread: Vec<bool> = argument_ids
            .iter()
            .map(|argument| {
                self.dbg_argument_type(*argument)
                    .is_some_and(|type_id| self.flat_width(type_id) > 1)
            })
            .collect();
        if self.dbg_policy == DbgPolicy::Strip {
            return match arguments.len() {
                0 => js::Node::Void,
                1 => arguments.into_iter().next().unwrap_or(js::Node::Void),
                _ => js::Node::Array(
                    arguments
                        .into_iter()
                        .zip(&spread)
                        .map(|(argument, spread)| {
                            if *spread {
                                js::Node::Spread(Box::new(argument))
                            } else {
                                argument
                            }
                        })
                        .collect(),
                ),
            };
        }
        self.used_helpers.insert("__dbg");
        let write = js::Node::Property(
            Box::new(js::Node::Local("console".to_string())),
            if self.program.platform.has_process_exit() {
                "error".to_string()
            } else {
                "log".to_string()
            },
        );
        let location = text(self.program.site_location(call_id));
        let texts: Vec<String> = argument_ids
            .iter()
            .map(|argument| self.program.source_text_of(*argument))
            .collect();
        let printers: Vec<String> = argument_ids
            .iter()
            .map(|argument| match self.dbg_argument_type(*argument) {
                Some(type_id) => self.printer_for(type_id),
                None => self.printer_for_text("<?>"),
            })
            .collect();
        if statement || arguments.is_empty() {
            let arguments: Vec<js::Node<'src>> = argument_ids
                .iter()
                .zip(arguments)
                .map(|(argument_id, argument)| {
                    self.read_through_scalar_view(*argument_id, argument)
                })
                .collect();
            let entries = texts
                .into_iter()
                .zip(printers)
                .zip(arguments)
                .map(|((source, printer), argument)| {
                    js::Node::Array(vec![text(source), call(&printer, vec![argument])])
                })
                .collect();
            return call("__dbg", vec![write, location, js::Node::Array(entries)]);
        }
        if arguments.len() == 1 {
            let argument = arguments.into_iter().next().unwrap_or(js::Node::Void);
            return call(
                "__dbg_value",
                vec![
                    write,
                    location,
                    text(texts.into_iter().next().unwrap_or_default()),
                    js::Node::Local(printers.into_iter().next().unwrap_or_default()),
                    argument,
                ],
            );
        }
        let mut parameters = vec![
            write,
            location,
            js::Node::Array(texts.into_iter().map(text).collect()),
            js::Node::Array(printers.into_iter().map(js::Node::Local).collect()),
            js::Node::Array(arguments),
        ];
        // A tuple-typed argument's slots splice into the answer: tuples store
        // flat, so `dbg(pair, 3)` answers `[a, b, 3]`, not `[[a, b], 3]`.
        if spread.iter().any(|spread| *spread) {
            parameters.push(js::Node::Array(
                spread.into_iter().map(js::Node::Bool).collect(),
            ));
        }
        call("__dbg_values", parameters)
    }

    /// `dbg_stack()` (debugging.md §4): the header line and one line per
    /// binding the analyzer listed, to the stream `dbg` writes to. A binding
    /// it may read prints through its type's printer from the read the
    /// expansion minted (an argument of the call); the rest print their fixed
    /// text. Under `[build] dbg = "strip"` the call is nothing.
    pub(super) fn dbg_stack_call(
        &mut self,
        call_id: Id,
        argument_ids: &[Id],
        arguments: Vec<js::Node<'src>>,
    ) -> js::Node<'src> {
        let program = self.program;
        let Some(site) = program.dbg_stack_sites.get(&call_id) else {
            return js::Node::Void;
        };
        if self.dbg_policy == DbgPolicy::Strip {
            return js::Node::Void;
        }
        self.used_helpers.insert("__dbg_stack");
        let write = js::Node::Property(
            Box::new(js::Node::Local("console".to_string())),
            if program.platform.has_process_exit() {
                "error".to_string()
            } else {
                "log".to_string()
            },
        );
        let mut arguments: Vec<Option<js::Node<'src>>> = arguments.into_iter().map(Some).collect();
        let mut entries = Vec::with_capacity(site.bindings.len());
        for binding in &site.bindings {
            let (head, unread, note) = {
                let resolve = |type_id| self.ground_printer_type(type_id);
                (
                    crate::printer::dbg_stack_head(program, binding, &resolve),
                    crate::printer::dbg_stack_unread_value(program, binding, &resolve),
                    crate::printer::dbg_stack_note(program, binding, &resolve),
                )
            };
            let document = match (&binding.value, unread) {
                (_, Some(fixed)) => text(fixed),
                (DbgStackValue::Read(read), None) => {
                    let value = argument_ids
                        .iter()
                        .position(|argument| argument == read)
                        .and_then(|index| arguments.get_mut(index))
                        .and_then(Option::take);
                    match value {
                        Some(value) => {
                            let value = self.read_through_scalar_view(*read, value);
                            let printer = self.printer_for(binding.type_id);
                            call(&printer, vec![value])
                        }
                        None => text("<?>"),
                    }
                }
                (_, None) => text("<?>"),
            };
            entries.push(js::Node::Array(vec![text(head), document, text(note)]));
        }
        call(
            "__dbg_stack",
            vec![
                write,
                text(program.site_location(call_id)),
                text(crate::printer::dbg_stack_title(site)),
                js::Node::Array(entries),
            ],
        )
    }

    /// debugging.md S3: a `print` of an aggregate writes the printer's document
    /// on one line (`__dbg_flat`), so a struct prints `Point { x = 1, y = 2 }`
    /// rather than its field array; every other argument is left as it is.
    pub(super) fn printed_aggregates(
        &mut self,
        target_id: Id,
        argument_ids: &[Id],
        args: Vec<js::Node<'src>>,
    ) -> (Vec<js::Node<'src>>, bool) {
        if target_id != self.print_fn_id {
            return (args, false);
        }
        let mut printed = false;
        let args = argument_ids
            .iter()
            .zip(args)
            .map(|(argument, value)| {
                let Some(type_id) = self.printed_type(*argument) else {
                    return value;
                };
                printed = true;
                self.used_helpers.insert("__dbg");
                let value = self.read_through_scalar_view(*argument, value);
                let printer = self.printer_for(type_id);
                call("__dbg_flat", vec![call(&printer, vec![value])])
            })
            .collect();
        (args, printed)
    }

    /// S3: the type a `print` argument prints through the printer at, under
    /// the active substitution — `None` for a value `print` keeps rendering as
    /// it always has (a number, a string, a host handle).
    fn printed_type(&self, argument: Id) -> Option<TypeId> {
        if self.program.number_print_arguments.contains(&argument) {
            return None;
        }
        let type_id = self
            .program
            .print_argument_types
            .get(&argument)
            .copied()
            .or_else(|| self.expr_type_id(argument))?;
        let resolve = |type_id| self.ground_printer_type(type_id);
        let shape = shape_of(self.program, resolve(type_id), &resolve);
        crate::printer::print_uses_the_printer(&shape).then_some(type_id)
    }

    /// A value read in place for printing: a binding that holds a scalar view
    /// emits its `(base, key)` pair, so the printer is handed `base[key]` —
    /// the value, as the native backend's borrow reads it (a view of an
    /// aggregate is the aggregate's own reference and reads as itself).
    fn read_through_scalar_view(&mut self, argument: Id, value: js::Node<'src>) -> js::Node<'src> {
        match self.program.entity_map.get(&argument) {
            Some(crate::analyzer::Expr::Local(binding))
                if self.binding_holds_a_scalar_view_pair(*binding) =>
            {
                // A binding's pair is read twice by name, so nothing is hoisted.
                self.emit_scalar_view_read(argument, value, &mut Vec::new())
            }
            _ => value,
        }
    }

    /// A `dbg` argument's type as the analyzer settled it, which may name the
    /// enclosing instance's generics — every reader resolves it under the
    /// active substitution.
    fn dbg_argument_type(&self, argument: Id) -> Option<TypeId> {
        self.program
            .dbg_argument_types
            .get(&argument)
            .copied()
            .or_else(|| self.expr_type_id(argument))
    }

    /// A printer that answers fixed text, whatever the value.
    fn printer_for_text(&mut self, fixed: &str) -> String {
        let key = format!("text:{fixed}");
        if let Some(name) = self.printers.get(&key) {
            return name.clone();
        }
        let name = self.printer_name("text");
        self.printers.insert(key, name.clone());
        self.push_printer(&name, vec![js::Node::Return(Box::new(text(fixed)))]);
        name
    }

    /// The name of the printer for `type_id` under the active substitution,
    /// generating it (and every printer it calls) on first ask.
    fn printer_for(&mut self, type_id: TypeId) -> String {
        let type_id = self.ground_printer_type(type_id);
        let key = self.type_key(type_id);
        if let Some(name) = self.printers.get(&key) {
            return name.clone();
        }
        let program = self.program;
        let resolve = |type_id| self.ground_printer_type(type_id);
        let shape = shape_of(program, type_id, &resolve);
        let label = crate::printer::type_text(program, type_id, &resolve);
        let name = self.printer_name(&label);
        // Recorded BEFORE the body is built: a recursive type's printer
        // calls itself.
        self.printers.insert(key, name.clone());
        if let Some(body) = self.written_debug_body(type_id) {
            self.push_printer(&name, body);
            return name;
        }
        let document = match shape {
            Shape::Integer | Shape::BigInt | Shape::Bool => js::Node::Binary(
                BinaryOp::Add,
                Box::new(text("")),
                Box::new(js::Node::Local("value".to_string())),
            ),
            Shape::Float => call("__dbg_float", vec![js::Node::Local("value".to_string())]),
            Shape::Str => call("__dbg_str", vec![js::Node::Local("value".to_string())]),
            Shape::Void => text("()"),
            Shape::Text(fixed) => text(fixed),
            Shape::Struct {
                name: struct_name,
                fields,
                bindings,
            } => {
                if fields.is_empty() {
                    text(struct_name)
                } else {
                    let saved = self.current_substitution.clone();
                    self.current_substitution.extend(bindings);
                    let entries = fields
                        .iter()
                        .enumerate()
                        .map(|(index, (field, field_type))| {
                            let printer = self.printer_for(*field_type);
                            (format!("{field} = "), call(&printer, vec![slot(index)]))
                        })
                        .collect();
                    self.current_substitution = saved;
                    group(&format!("{struct_name} {{"), "}", true, entries)
                }
            }
            Shape::Tuple(elements) => {
                let mut offset = 0;
                let mut entries = Vec::with_capacity(elements.len());
                for element in elements {
                    let width = self.flat_width(element);
                    let printer = self.printer_for(element);
                    let read = if width == 1 {
                        slot(offset)
                    } else {
                        js::Node::Call(
                            Box::new(js::Node::Property(
                                Box::new(js::Node::Local("value".to_string())),
                                "slice".to_string(),
                            )),
                            vec![
                                js::Node::Number(offset.to_string(), None),
                                js::Node::Number((offset + width).to_string(), None),
                            ],
                        )
                    };
                    entries.push((String::new(), call(&printer, vec![read])));
                    offset += width;
                }
                group("(", ")", false, entries)
            }
            Shape::List(element) => {
                let printer = self.printer_for(element);
                let mut arguments = vec![
                    js::Node::Local("value".to_string()),
                    js::Node::Local(printer),
                ];
                if self.prints_as_a_scalar(element) {
                    arguments.push(js::Node::Bool(true));
                }
                call("__dbg_list", arguments)
            }
            Shape::Shared(inner) => {
                let printer = self.printer_for(inner);
                call(
                    "__dbg_shared",
                    vec![
                        js::Node::Local("value".to_string()),
                        js::Node::Local(printer),
                    ],
                )
            }
            Shape::Cell {
                label,
                field: (index, _),
                value: inner,
            } => {
                // The cell's `Shared` holds the value as `.v`; reading it is
                // the plain read — nothing subscribes.
                let printer = self.printer_for(inner);
                group(
                    &format!("{label}("),
                    ")",
                    false,
                    vec![(
                        String::new(),
                        call(
                            &printer,
                            vec![js::Node::Property(Box::new(slot(index)), "v".to_string())],
                        ),
                    )],
                )
            }
            Shape::Map {
                label,
                field: (index, _),
                key,
                value: entry_value,
            } => {
                let key_width = self.flat_width(key);
                let value_width = self.flat_width(entry_value);
                let key_printer = self.printer_for(key);
                let value_printer = self.printer_for(entry_value);
                call(
                    "__dbg_map",
                    vec![
                        text(format!("{label} {{")),
                        slot(index),
                        js::Node::Local(key_printer),
                        js::Node::Local(value_printer),
                        js::Node::Number(key_width.to_string(), None),
                        js::Node::Number(value_width.to_string(), None),
                    ],
                )
            }
            Shape::Set {
                label,
                field: (index, _),
                element,
            } => {
                let printer = self.printer_for(element);
                let mut arguments = vec![
                    text(format!("{label} {{")),
                    slot(index),
                    js::Node::Local(printer),
                ];
                if self.prints_as_a_scalar(element) {
                    arguments.push(js::Node::Bool(true));
                }
                call("__dbg_set", arguments)
            }
            Shape::Object { label } => {
                // `value[1].$show(value[0])`: the pair's table prints the
                // value it erased (S1b).
                let show = js::Node::Call(
                    Box::new(js::Node::Property(Box::new(slot(1)), "$show".to_string())),
                    vec![slot(0)],
                );
                group(
                    &format!("{label}("),
                    ")",
                    false,
                    vec![(String::new(), show)],
                )
            }
            Shape::Enum { variants, bindings } => {
                let saved = self.current_substitution.clone();
                self.current_substitution.extend(bindings);
                let mut legs = Vec::with_capacity(variants.len());
                for (variant, payload) in &variants {
                    let document = if payload.is_empty() {
                        text(variant.as_str())
                    } else {
                        let entries = payload
                            .iter()
                            .enumerate()
                            .map(|(index, payload_type)| {
                                let printer = self.printer_for(*payload_type);
                                (String::new(), call(&printer, vec![slot(index + 1)]))
                            })
                            .collect();
                        group(&format!("{variant}("), ")", false, entries)
                    };
                    legs.push(document);
                }
                self.current_substitution = saved;
                let body = Self::tagged_legs(legs, |index| {
                    js::Node::Binary(
                        BinaryOp::Eq,
                        Box::new(slot(0)),
                        Box::new(js::Node::Number(index.to_string(), None)),
                    )
                });
                self.push_printer(&name, body);
                return name;
            }
            Shape::Backed { variants } => {
                let legs: Vec<(js::Node<'src>, js::Node<'src>)> = variants
                    .iter()
                    .map(|(variant, backing)| {
                        let literal = match backing {
                            crate::analyzer::BackingValue::Int(value) => {
                                js::Node::Number(value.to_string(), None)
                            }
                            crate::analyzer::BackingValue::Str(raw) => js::Node::String(
                                Cow::Owned(super::unescape_string(raw).into_owned()),
                            ),
                        };
                        (literal, text(variant.as_str()))
                    })
                    .collect();
                let mut body = Vec::with_capacity(legs.len() + 1);
                for (literal, document) in legs {
                    body.push(js::Node::If(js::IfBranch::If(
                        Box::new(js::Node::Binary(
                            BinaryOp::Eq,
                            Box::new(js::Node::Local("value".to_string())),
                            Box::new(literal),
                        )),
                        vec![js::Node::Return(Box::new(document))],
                        None,
                    )));
                }
                body.push(js::Node::Return(Box::new(text("<?>"))));
                self.push_printer(&name, body);
                return name;
            }
        };
        self.push_printer(&name, vec![js::Node::Return(Box::new(document))]);
        name
    }

    /// E275: a printer body that answers the text of the type's WRITTEN
    /// `Debug` impl (`return debug(value);`), or `None` when the structure
    /// prints ([`crate::printer::written_debug`] says which). An async
    /// `debug` answers a promise, not text, so it leaves the structure in
    /// charge.
    fn written_debug_body(&mut self, type_id: TypeId) -> Option<Vec<js::Node<'src>>> {
        let trait_id = crate::printer::written_debug(self.program, type_id)?;
        let dispatch =
            self.resolve_dispatch_with(type_id, "debug", &[], Some((trait_id, Vec::new())))?;
        if matches!(dispatch, super::Dispatch::Call(_, true)) {
            return None;
        }
        let call = self.emit_dispatch(dispatch, vec![js::Node::Local("value".to_string())], None);
        Some(vec![js::Node::Return(Box::new(call))])
    }

    /// S1b: the `show` slot of the table for one `(type, trait)` pair — the
    /// type's printer — when the program's tables carry one, else `None`.
    pub(super) fn object_show_slot(&mut self, type_id: TypeId) -> Option<js::Node<'src>> {
        if !crate::printer::tables_carry_show(self.program, self.dbg_policy) {
            return None;
        }
        Some(js::Node::Local(self.printer_for(type_id)))
    }

    /// N136/N149: whether `argument` is a number of the language's own (an
    /// integer of any width, `f32`, `f64`; not `BigInt`) under the active
    /// substitution — what `print` formats by `String(x)`.
    pub(super) fn prints_a_number(&self, argument: Id) -> bool {
        let Some(type_id) = self.expr_type_id(argument) else {
            return false;
        };
        let resolve = |type_id| self.ground_printer_type(type_id);
        matches!(
            shape_of(self.program, type_id, &resolve),
            Shape::Integer | Shape::Float
        )
    }

    /// Whether an element of `type_id` prints as one short token, so its list
    /// or set fills its broken lines (E277).
    fn prints_as_a_scalar(&self, type_id: TypeId) -> bool {
        let resolve = |type_id| self.ground_printer_type(type_id);
        shape_of(self.program, type_id, &resolve).is_scalar()
    }

    /// `type_id` under the active substitution. A generic enum's payload can
    /// name its parameter by the CONSTRAINT id itself (whose own type is
    /// `Any`), not through a `Generic` node, so the substitution is asked
    /// for the id directly first — the native emitter's `concrete` does the
    /// same, for the same reason.
    fn ground_printer_type(&self, type_id: TypeId) -> TypeId {
        let Some(_guard) = crate::util::RecursionGuard::enter() else {
            return type_id;
        };
        match self.current_substitution.get(&type_id) {
            Some(bound) if *bound != type_id => self.ground_printer_type(*bound),
            _ => {
                let resolved = self.resolve_type_id(type_id);
                if resolved != type_id {
                    self.ground_printer_type(resolved)
                } else {
                    type_id
                }
            }
        }
    }

    /// An enum's legs as `if (<tag test>) return <document>;` with the last
    /// leg unconditional.
    fn tagged_legs(
        legs: Vec<js::Node<'src>>,
        test: impl Fn(usize) -> js::Node<'src>,
    ) -> Vec<js::Node<'src>> {
        let count = legs.len();
        let mut body = Vec::with_capacity(count);
        for (index, document) in legs.into_iter().enumerate() {
            if index + 1 == count {
                body.push(js::Node::Return(Box::new(document)));
            } else {
                body.push(js::Node::If(js::IfBranch::If(
                    Box::new(test(index)),
                    vec![js::Node::Return(Box::new(document))],
                    None,
                )));
            }
        }
        if body.is_empty() {
            body.push(js::Node::Return(Box::new(text("<?>"))));
        }
        body
    }

    /// `__show_<Type>`, unique: the type as vilan writes it, its non-identifier
    /// characters folded to `_`, and a counter on a second type that folds to
    /// the same name.
    fn printer_name(&mut self, label: &str) -> String {
        let mut base = String::from("__show_");
        let mut last_underscore = true;
        for character in label.chars() {
            if character.is_ascii_alphanumeric() {
                base.push(character);
                last_underscore = false;
            } else if !last_underscore {
                base.push('_');
                last_underscore = true;
            }
        }
        while base.ends_with('_') && base.len() > "__show_".len() {
            base.pop();
        }
        let mut name = base.clone();
        let mut counter = 2;
        while !self.printer_names.insert(name.clone()) {
            name = format!("{base}{counter}");
            counter += 1;
        }
        name
    }

    fn push_printer(&mut self, name: &str, body: Vec<js::Node<'src>>) {
        self.printer_functions
            .push(js::Node::Function(js::Function {
                name: name.to_string(),
                parameters: vec![js::Parameter {
                    name: "value".to_string(),
                }],
                body,
                is_async: false,
            }));
    }
}
