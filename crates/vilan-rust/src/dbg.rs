//! `dbg(..)` on the native backend (debugging.md S1): the call's lowering and
//! the generated printers — the twin of `vilan-core`'s `transformer/dbg.rs`.
//!
//! A printer is a `fn show_<n>(value: &T) -> vilan_rt::show::Doc` per
//! concrete type a `dbg` reaches. [`vilan_core::printer::shape_of`] says what
//! the type prints as, so both backends build the same document; this file
//! says how a Rust value of that shape is read — named fields, minted enum
//! variants, nested tuples, `Vec`s.

use std::fmt::Write as _;

use vilan_core::analyzer::BackingValue;
use vilan_core::error::Error;
use vilan_core::id::Id;
use vilan_core::options::DbgPolicy;
use vilan_core::printer::{Shape, shape_of};
use vilan_core::span::Span;
use vilan_core::type_::{Type, TypeId};

use crate::{Emitter, NativeDispatch, Receiving, rust_literal, rust_string, sanitize, unsupported};

impl<'a, 'src> Emitter<'a, 'src> {
    /// `dbg(a, b, ..)`: prints `[file:line:col] expr = value` per argument to
    /// stderr and answers the argument, a tuple of them for several, `()` for
    /// none (Q2). A statement reads its arguments in place (a borrow, so
    /// `dbg(guard);` moves nothing); under `[build] dbg = "strip"` the call is
    /// its arguments alone.
    pub(crate) fn dbg_call(
        &mut self,
        call_id: Id,
        argument_ids: &[Id],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let statement = self.program.dbg_statement_calls.contains(&call_id);
        if self.dbg_policy == DbgPolicy::Strip {
            let mut values = Vec::with_capacity(argument_ids.len());
            for argument in argument_ids {
                values.push(self.consumed_value_of(*argument, depth)?);
            }
            return Ok(match values.len() {
                0 => "()".to_string(),
                1 => values.pop().unwrap_or_default(),
                _ => format!("({})", values.join(", ")),
            });
        }
        let location = format!(
            "vilan_rt::Location({})",
            rust_literal(&self.program.site_location(call_id))
        );
        let mut printers = Vec::with_capacity(argument_ids.len());
        for argument in argument_ids {
            let printer = match self.dbg_argument_type(*argument) {
                Some(type_id) => self.native_printer_for(type_id, span)?,
                None => self.native_printer_for_text("<?>", "()"),
            };
            printers.push(printer);
        }
        let texts: Vec<String> = argument_ids
            .iter()
            .map(|argument| rust_literal(&self.program.source_text_of(*argument)))
            .collect();
        if statement || argument_ids.is_empty() {
            let mut entries = Vec::with_capacity(argument_ids.len());
            for (index, (text, printer)) in texts.iter().zip(&printers).enumerate() {
                let place = self.place_argument(argument_ids, index, depth)?;
                entries.push(format!("({text}, {printer}(&({place})))"));
            }
            return Ok(format!(
                "vilan_rt::show::dbg({location}, vec![{}])",
                entries.join(", ")
            ));
        }
        let mut out = String::from("{ ");
        let mut entries = Vec::with_capacity(argument_ids.len());
        let mut names = Vec::with_capacity(argument_ids.len());
        for (index, argument) in argument_ids.iter().enumerate() {
            let value = self.consumed_value_of(*argument, depth)?;
            let name = format!("dbg_{index}");
            let _ = write!(out, "let {name} = {value}; ");
            entries.push(format!("({}, {}(&{name}))", texts[index], printers[index]));
            names.push(name);
        }
        let _ = write!(
            out,
            "vilan_rt::show::dbg({location}, vec![{}]); ",
            entries.join(", ")
        );
        if names.len() == 1 {
            let _ = write!(out, "{} }}", names[0]);
        } else {
            let _ = write!(out, "({}) }}", names.join(", "));
        }
        Ok(out)
    }

    fn dbg_argument_type(&self, argument: Id) -> Option<TypeId> {
        self.program
            .dbg_argument_types
            .get(&argument)
            .copied()
            .or_else(|| self.type_of(argument))
    }

    /// A printer answering fixed text for a value of `rust_type`.
    fn native_printer_for_text(&mut self, fixed: &str, rust_type: &str) -> String {
        let key = format!("text:{fixed}:{rust_type}");
        if let Some(name) = self.printers.get(&key) {
            return name.clone();
        }
        let name = format!("show_{}", self.printers.len());
        self.printers.insert(key, name.clone());
        self.printer_bodies.push(format!(
            "fn {name}(_value: &{rust_type}) -> vilan_rt::show::Doc {{\n    vilan_rt::show::Doc::text({})\n}}\n",
            rust_literal(fixed)
        ));
        name
    }

    /// The printer for `type_id` under the active substitution, generated
    /// (with every printer it calls) on first ask.
    pub(crate) fn native_printer_for(
        &mut self,
        type_id: TypeId,
        span: Span,
    ) -> Result<String, Error> {
        let type_id = self.concrete(type_id);
        let key = self.type_key(type_id);
        if let Some(name) = self.printers.get(&key) {
            return Ok(name.clone());
        }
        let rust_type = self.rust_type(type_id, span)?;
        let program = self.program;
        let shape = {
            let resolve = |type_id| self.concrete(type_id);
            shape_of(program, type_id, &resolve)
        };
        let name = format!("show_{}", self.printers.len());
        // Recorded BEFORE the body is built: a recursive type's printer calls
        // itself.
        self.printers.insert(key, name.clone());
        if let Some(body) = self.written_debug_body(type_id, span)? {
            self.printer_bodies.push(format!(
                "fn {name}(value: &{rust_type}) -> vilan_rt::show::Doc {{\n    {body}\n}}\n"
            ));
            return Ok(name);
        }
        let body = match shape {
            Shape::Integer | Shape::BigInt => "vilan_rt::show::integer(value)".to_string(),
            Shape::Float => "vilan_rt::show::float(*value as f64)".to_string(),
            Shape::Bool => "vilan_rt::show::Doc::text(value.to_string())".to_string(),
            Shape::Str => "vilan_rt::show::string(value)".to_string(),
            Shape::Void => "vilan_rt::show::Doc::text(\"()\")".to_string(),
            Shape::Text(fixed) => format!("vilan_rt::show::Doc::text({})", rust_literal(&fixed)),
            Shape::Struct {
                name: struct_name,
                fields,
                bindings,
            } => {
                if fields.is_empty() {
                    format!("vilan_rt::show::Doc::text({})", rust_literal(&struct_name))
                } else {
                    let entries = self.with_bindings(bindings, |emitter| {
                        let mut entries = Vec::with_capacity(fields.len());
                        for (field, field_type) in &fields {
                            let printer = emitter.native_printer_for(*field_type, span)?;
                            entries.push(format!(
                                "({}.to_string(), {printer}(&value.{}))",
                                rust_literal(&format!("{field} = ")),
                                sanitize(field)
                            ));
                        }
                        Ok(entries)
                    })?;
                    format!(
                        "vilan_rt::show::Doc::group({}, \"}}\", true, vec![{}])",
                        rust_literal(&format!("{struct_name} {{")),
                        entries.join(", ")
                    )
                }
            }
            Shape::Tuple(elements) => {
                let mut entries = Vec::with_capacity(elements.len());
                for (index, element) in elements.iter().enumerate() {
                    let printer = self.native_printer_for(*element, span)?;
                    entries.push(format!("(String::new(), {printer}(&value.{index}))"));
                }
                format!(
                    "vilan_rt::show::Doc::group(\"(\", \")\", false, vec![{}])",
                    entries.join(", ")
                )
            }
            Shape::List(element) => {
                let printer = self.native_printer_for(element, span)?;
                let fill = self.prints_as_a_scalar(element);
                format!("vilan_rt::show::list(&value[..], {fill}, |item| {printer}(item))")
            }
            Shape::Shared(inner) => {
                let printer = self.native_printer_for(inner, span)?;
                format!("vilan_rt::show::shared(value, |inner| {printer}(inner))")
            }
            Shape::Cell {
                label,
                field: (_, field),
                value: inner,
            } => {
                // `get()` is the cell's plain read: nothing subscribes.
                let printer = self.native_printer_for(inner, span)?;
                format!(
                    "vilan_rt::show::Doc::group({}, \")\", false, vec![(String::new(), {printer}(&value.{}.get()))])",
                    rust_literal(&format!("{label}(")),
                    sanitize(&field)
                )
            }
            Shape::Map {
                label,
                field: (_, field),
                key,
                value: entry_value,
            } => {
                let key_printer = self.native_printer_for(key, span)?;
                let value_printer = self.native_printer_for(entry_value, span)?;
                format!(
                    "vilan_rt::show::members({}, &value.{}.values(), false, |entry| (format!(\"{{}} => \", {key_printer}(&entry.0).flat()), {value_printer}(&entry.1)))",
                    rust_literal(&format!("{label} {{")),
                    sanitize(&field)
                )
            }
            Shape::Set {
                label,
                field: (_, field),
                element,
            } => {
                let printer = self.native_printer_for(element, span)?;
                let fill = self.prints_as_a_scalar(element);
                format!(
                    "vilan_rt::show::members({}, &value.{}.values(), {fill}, |item| (String::new(), {printer}(item)))",
                    rust_literal(&format!("{label} {{")),
                    sanitize(&field)
                )
            }
            Shape::Object { label } => {
                let Some(Type::Dyn(trait_id, arguments)) = self.resolve(type_id).cloned() else {
                    return Err(unsupported("printing an object that did not resolve", span));
                };
                let object = self.ensure_object_trait(trait_id, &arguments, span)?;
                match object.show {
                    Some(show) => format!(
                        "vilan_rt::show::Doc::group({}, \")\", false, vec![(String::new(), {}::{show}(value.object()))])",
                        rust_literal(&format!("{label}(")),
                        object.name
                    ),
                    None => format!(
                        "vilan_rt::show::Doc::text({})",
                        rust_literal(&format!("<{label}>"))
                    ),
                }
            }
            Shape::Enum { variants, bindings } => {
                let Some(Type::Enum(enum_id, arguments)) = self.resolve(type_id).cloned() else {
                    return Err(unsupported("printing an enum that did not resolve", span));
                };
                let legs = self.with_bindings(bindings, |emitter| {
                    let mut legs = Vec::with_capacity(variants.len());
                    for (index, (label, payload)) in variants.iter().enumerate() {
                        let path = emitter.variant_path(enum_id, index, &arguments, span)?;
                        if payload.is_empty() {
                            legs.push(format!(
                                "        {path} => vilan_rt::show::Doc::text({}),",
                                rust_literal(label)
                            ));
                            continue;
                        }
                        let mut binders = Vec::with_capacity(payload.len());
                        let mut entries = Vec::with_capacity(payload.len());
                        for (slot, payload_type) in payload.iter().enumerate() {
                            let printer = emitter.native_printer_for(*payload_type, span)?;
                            binders.push(format!("p{slot}"));
                            entries.push(format!("(String::new(), {printer}(p{slot}))"));
                        }
                        legs.push(format!(
                            "        {path}({}) => vilan_rt::show::Doc::group({}, \")\", false, vec![{}]),",
                            binders.join(", "),
                            rust_literal(&format!("{label}(")),
                            entries.join(", ")
                        ));
                    }
                    Ok(legs)
                })?;
                format!("match value {{\n{}\n    }}", legs.join("\n"))
            }
            Shape::Backed { variants } => {
                let mut legs = Vec::with_capacity(variants.len() + 1);
                for (label, backing) in &variants {
                    let test = match backing {
                        BackingValue::Int(value) => format!("*value == {value}"),
                        BackingValue::Str(raw) => format!("&**value == {}", rust_string(raw)),
                    };
                    legs.push(format!(
                        "if {test} {{ return vilan_rt::show::Doc::text({}); }}",
                        rust_literal(label)
                    ));
                }
                legs.push("vilan_rt::show::Doc::text(\"<?>\")".to_string());
                legs.join("\n    ")
            }
        };
        self.printer_bodies.push(format!(
            "fn {name}(value: &{rust_type}) -> vilan_rt::show::Doc {{\n    {body}\n}}\n"
        ));
        Ok(name)
    }

    /// E275: a printer body that answers the text of the type's WRITTEN
    /// `Debug` impl, or `None` when the structure prints
    /// ([`vilan_core::printer::written_debug`] says which) — the twin of the
    /// JS emitter's. The impl's `debug` takes its receiver as a loan or a
    /// copy, by its own convention.
    fn written_debug_body(&mut self, type_id: TypeId, span: Span) -> Result<Option<String>, Error> {
        let Some(trait_id) = vilan_core::printer::written_debug(self.program, type_id) else {
            return Ok(None);
        };
        let Some(NativeDispatch::Call(function_name)) =
            self.resolve_dispatch(type_id, "debug", &[], Some((trait_id, Vec::new())), span)?
        else {
            return Ok(None);
        };
        let receiver = self
            .instance_target(&function_name)
            .and_then(|target| self.program.functions.get(&target))
            .and_then(|function| function.parameters.first())
            .and_then(|parameter| self.program.parameters.get(parameter))
            .map(|parameter| self.receiving_form(parameter));
        let argument = match receiver {
            Some(Receiving::Ref) => "value",
            Some(Receiving::ByValue) => "value.clone()",
            _ => return Ok(None),
        };
        Ok(Some(format!(
            "vilan_rt::show::Doc::text({function_name}({argument}).to_string())"
        )))
    }

    /// Whether an element of `type_id` prints as one short token, so its list
    /// or set fills its broken lines (E277) — the JS emitter asks the same.
    fn prints_as_a_scalar(&self, type_id: TypeId) -> bool {
        let resolve = |type_id| self.concrete(type_id);
        shape_of(self.program, type_id, &resolve).is_scalar()
    }

    /// Runs `body` with a nominal declaration's generic parameters bound to
    /// its instance's arguments, resolved first under the substitution in
    /// force.
    fn with_bindings<T>(
        &mut self,
        bindings: Vec<(TypeId, TypeId)>,
        body: impl FnOnce(&mut Self) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let entries = bindings
            .into_iter()
            .map(|(parameter, argument)| (parameter, self.concrete(argument)))
            .collect();
        let saved = self.enter_substitution(entries);
        let outcome = body(self);
        self.current_substitution = saved;
        outcome
    }
}
