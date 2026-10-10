//! A144 (ruled R-f, Order 45): a `[service]`'s contract hash is computed over
//! its RESOLVED types, not the types as written.
//!
//! The `service` macro (`std/src/rpc.vl`) runs before analysis and sees each
//! type as its spelling, so an alias and its target — the deprecated `Map<..>`
//! and `HashMap<..>`, or `import a::Row as Line` and `Row` — rendered apart and
//! hashed apart for one wire contract. The macro therefore no longer hashes. It
//! writes the contract surface as a TEMPLATE whose every type is a slot, `$N$`,
//! and hands it to `rpc::resolved_contract_hash(template, |_slot0: T0, ..| {})`
//! wherever the hash is needed; the closure's Nth parameter is typed with the
//! Nth slot's type, so analysis resolves the slots like any other annotation.
//!
//! This pass answers each such call in place: djb2 over the template with every
//! slot filled by its parameter's resolved type, as 8 hex digits, recorded in
//! `const_results` — which both backends serialize in place of the expression,
//! so the call (and its closure) never reaches either emitter. Every site of one
//! expansion — the service's `contract_hash`, the client's, the reconnect hook's
//! and the connect check — carries the same template and slots, so they agree.
//!
//! B525 (mirrored-store.md S0, Q14; ruled into v0.44.0): a slot renders a
//! type's SHAPE, not only its name. A Wire codec is the type itself — the binary
//! reader advances by position and ignores names, the JSON one reads by name —
//! so a struct that gained, lost, renamed or reordered a field, or an enum whose
//! variants moved, is a different contract under the same name. Every struct and
//! enum declared OUTSIDE std is therefore written, the first time one surface
//! reaches it, as its name followed by its fields in declaration order (name and
//! resolved type, recursively) or its variants in order (payload types, and the
//! backing value of a backed enum). std's types stand for themselves: they change
//! only with the toolchain both sides are built by, and the handle types among
//! them (`SignalCell`, `MemoCell`, …) are not encoded by their fields at all.

use crate::analyzer::{BackingValue, Expr, Program};
use crate::fx::FxHashSet as HashSet;
use crate::id::Id;
use crate::interpreter::ConstValue;
use crate::type_::{Type, TypeId};

/// Answer every `rpc::resolved_contract_hash(..)` call in the program.
pub fn resolve_contract_hashes(program: &mut Program) {
    let Some(resolver) = resolver_function(program) else {
        return;
    };
    let mut answers: Vec<(Id, String)> = Vec::new();
    for (&call_id, call) in &program.function_calls {
        let subject = program.entity_map.get(&call.subject_id);
        if !matches!(subject, Some(Expr::Local(target)) if *target == resolver) {
            continue;
        }
        let (Some(&template_id), Some(&slots_id)) =
            (call.argument_ids.first(), call.argument_ids.get(1))
        else {
            continue;
        };
        let Some(Expr::String(template)) = program.entity_map.get(&template_id) else {
            continue;
        };
        let Some(Expr::Closure(closure_id)) = program.entity_map.get(&slots_id) else {
            continue;
        };
        let Some(closure) = program.closures.get(closure_id) else {
            continue;
        };
        // One renderer per surface: a shape is written where the surface first
        // reaches its type, and by name after that.
        let mut renderer = Renderer::new(program);
        let slots: Vec<String> = closure
            .parameters
            .iter()
            .map(|parameter| match program.parameters.get(parameter) {
                Some(parameter) => renderer.render(parameter.type_id, &[]),
                None => "_".to_string(),
            })
            .collect();
        answers.push((call_id, hash_hex(&fill(template, &slots))));
    }
    // The call's VALUE is the hash: the entity a `const_results` entry replaces
    // is the expression, which for a call is the entity whose `Expr` is the
    // call record.
    let call_entities: Vec<(Id, String)> = program
        .entity_map
        .iter()
        .filter_map(|(&entity, expr)| match expr {
            Expr::Call(call_id) => answers
                .iter()
                .find(|(answered, _)| answered == call_id)
                .map(|(_, hash)| (entity, hash.clone())),
            _ => None,
        })
        .collect();
    for (entity, hash) in call_entities {
        program.const_results.insert(entity, ConstValue::Str(hash));
    }
}

/// `std::rpc::resolved_contract_hash`, if `rpc.vl` loaded.
fn resolver_function(program: &Program) -> Option<Id> {
    program.functions.values().find_map(|function| {
        if function.name != "resolved_contract_hash" {
            return None;
        }
        let source = program.source_of(function.id)?;
        let in_std_rpc = program.std_sources.contains(&source)
            && program
                .sources
                .get(source.0 as usize)
                .and_then(|path| path.file_name())
                .is_some_and(|file| file == "rpc.vl");
        in_std_rpc.then_some(function.id)
    })
}

/// The template with each `$N$` replaced by the Nth slot. Text that is not a
/// well-formed slot is kept as written.
fn fill(template: &str, slots: &[String]) -> String {
    let mut filled = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find('$') {
        filled.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let slot = after
            .find('$')
            .and_then(|end| after[..end].parse::<usize>().ok().map(|index| (index, end)))
            .and_then(|(index, end)| slots.get(index).map(|slot| (slot, end)));
        match slot {
            Some((slot, end)) => {
                filled.push_str(slot);
                rest = &after[end + 1..];
            }
            None => {
                filled.push('$');
                rest = after;
            }
        }
    }
    filled.push_str(rest);
    filled
}

/// djb2 over the filled surface, as 8 lowercase hex digits — the fingerprint
/// both generated sides compare at connect.
fn hash_hex(surface: &str) -> String {
    let mut hash: u32 = 5381;
    for byte in surface.bytes() {
        hash = hash.wrapping_mul(33) ^ (byte as u32);
    }
    format!("{hash:08x}")
}

/// A resolved type's canonical spelling, rendered for one surface.
struct Renderer<'program, 'src> {
    program: &'program Program<'src>,
    /// The instances whose shape this surface has written already, by their
    /// rendered name (`Page<i32>`): a second mention is the name alone, which is
    /// also what ends the walk through a recursive type.
    expanded: HashSet<String>,
}

impl<'program, 'src> Renderer<'program, 'src> {
    fn new(program: &'program Program<'src>) -> Self {
        Renderer {
            program,
            expanded: HashSet::default(),
        }
    }

    /// Each nominal type by its DECLARED name (an alias, a renaming import or a
    /// module path all name the declaration), its arguments in order,
    /// `, `-separated — the spelling a type written plainly already has — and,
    /// for a struct or enum declared outside std, its shape after the name the
    /// first time (B525). `substitution` maps the enclosing declaration's
    /// generic parameters (by constraint id) to their rendered arguments.
    fn render(&mut self, type_id: TypeId, substitution: &[(TypeId, String)]) -> String {
        let program = self.program;
        let Some(type_) = program.type_id_to_type_map.get(&type_id) else {
            return "_".to_string();
        };
        match type_ {
            Type::Struct(id, arguments) => {
                let name = self.nominal(
                    program.structs.get(id).map(|declared| declared.name),
                    arguments,
                    substitution,
                );
                self.with_struct_shape(*id, name, arguments, substitution)
            }
            Type::Enum(id, arguments) => {
                let name = self.nominal(
                    program.enums.get(id).map(|declared| declared.name),
                    arguments,
                    substitution,
                );
                self.with_enum_shape(*id, name, arguments, substitution)
            }
            Type::Trait(id, arguments) => self.nominal(
                program.traits.get(id).map(|declared| declared.name),
                arguments,
                substitution,
            ),
            Type::Dyn(id, arguments) => format!(
                "dyn {}",
                self.nominal(
                    program.traits.get(id).map(|declared| declared.name),
                    arguments,
                    substitution,
                )
            ),
            Type::Tuple(elements, _) => format!("({})", self.list(elements, substitution)),
            Type::Array(element, length) => {
                format!("[{}; {length}]", self.render(*element, substitution))
            }
            // B495: a closure type's parameter modes are part of the type, so
            // they are part of its spelling (`|&str| void`).
            Type::Closure(parameters, returned, _, modes) => {
                let parameters: Vec<String> = parameters
                    .iter()
                    .enumerate()
                    .map(|(index, parameter)| {
                        let prefix = modes
                            .get(index)
                            .map_or("", |mode| self.program.parameter_mode(*mode).prefix());
                        format!("{prefix}{}", self.render(*parameter, substitution))
                    })
                    .collect();
                format!(
                    "|{}| {}",
                    parameters.join(", "),
                    self.render(*returned, substitution)
                )
            }
            Type::Void => "void".to_string(),
            Type::Never => "never".to_string(),
            // A parameter of the declaration whose shape is being written: the
            // argument this instance gives it.
            Type::Generic(constraint) => substitution
                .iter()
                .find(|(parameter, _)| parameter == constraint)
                .map_or_else(|| "_".to_string(), |(_, argument)| argument.clone()),
            // A module, a function and the solver's holes do not reach a service
            // surface: services are not generic (B266), and a type that did not
            // resolve is already a diagnostic.
            _ => "_".to_string(),
        }
    }

    /// The declaration's parameters, each paired with its argument's rendering —
    /// the substitution its fields and payloads are written under.
    fn bind(
        &mut self,
        parameters: &[TypeId],
        arguments: &[TypeId],
        substitution: &[(TypeId, String)],
    ) -> Vec<(TypeId, String)> {
        parameters
            .iter()
            .zip(arguments)
            .map(|(&parameter, &argument)| (parameter, self.render(argument, substitution)))
            .collect()
    }

    /// Whether `id`'s shape is written: a declaration outside std, written once
    /// per surface and instance.
    fn first_user_mention(&mut self, id: Id, name: &str) -> bool {
        let program = self.program;
        let in_std = program
            .source_of(id)
            .is_some_and(|source| program.std_sources.contains(&source));
        !in_std && self.expanded.insert(name.to_string())
    }

    fn with_struct_shape(
        &mut self,
        id: Id,
        name: String,
        arguments: &[TypeId],
        substitution: &[(TypeId, String)],
    ) -> String {
        let program = self.program;
        let Some(declared) = program.structs.get(&id) else {
            return name;
        };
        // An `external` struct is a host type with no fields to write.
        if declared.external || !self.first_user_mention(id, &name) {
            return name;
        }
        let inner = self.bind(
            &declared.generic_parameter_constraint_ids,
            arguments,
            substitution,
        );
        let fields: Vec<String> = declared
            .fields
            .iter()
            .map(|field| format!("{}: {}", field.name, self.render(field.type_id, &inner)))
            .collect();
        format!("{name}{{{}}}", fields.join(", "))
    }

    fn with_enum_shape(
        &mut self,
        id: Id,
        name: String,
        arguments: &[TypeId],
        substitution: &[(TypeId, String)],
    ) -> String {
        let program = self.program;
        let Some(declared) = program.enums.get(&id) else {
            return name;
        };
        if !self.first_user_mention(id, &name) {
            return name;
        }
        let inner = self.bind(
            &declared.generic_parameter_constraint_ids,
            arguments,
            substitution,
        );
        let backed = declared.backing.is_some();
        let variants: Vec<String> = declared
            .variants
            .iter()
            .map(|variant| {
                let mut written = variant.name.to_string();
                if !variant.data_type_ids.is_empty() {
                    written.push_str(&format!("({})", self.list(&variant.data_type_ids, &inner)));
                }
                if backed {
                    match &variant.backing_value {
                        BackingValue::Int(value) => written.push_str(&format!(" = {value}")),
                        BackingValue::Str(value) => written.push_str(&format!(" = \"{value}\"")),
                    }
                }
                written
            })
            .collect();
        format!("{name}{{{}}}", variants.join(", "))
    }

    fn nominal(
        &mut self,
        name: Option<&str>,
        arguments: &[TypeId],
        substitution: &[(TypeId, String)],
    ) -> String {
        let name = name.unwrap_or("_");
        if arguments.is_empty() {
            name.to_string()
        } else {
            format!("{name}<{}>", self.list(arguments, substitution))
        }
    }

    fn list(&mut self, types: &[TypeId], substitution: &[(TypeId, String)]) -> String {
        types
            .iter()
            .map(|&type_id| self.render(type_id, substitution))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::{fill, hash_hex};

    #[test]
    fn fill_replaces_each_slot_and_keeps_stray_dollars() {
        let slots = vec!["HashMap<str, i32>".to_string(), "i32".to_string()];
        assert_eq!(
            fill("get($0$)->$1$;keyed:x:$1$:$0$;", &slots),
            "get(HashMap<str, i32>)->i32;keyed:x:i32:HashMap<str, i32>;"
        );
        assert_eq!(fill("a$b$9$c$", &slots), "a$b$9$c$");
        assert_eq!(fill("", &slots), "");
    }

    #[test]
    fn hash_hex_is_djb2_in_eight_hex_digits() {
        // djb2 of the empty surface is its seed.
        assert_eq!(hash_hex(""), "00001505");
        assert_eq!(hash_hex("a").len(), 8);
    }
}
