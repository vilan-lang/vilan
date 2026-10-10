//! The `dbg` printer's SHAPES (debugging.md §2, S1): the one answer both
//! emitters build their generated printers from.
//!
//! A printer writes a value in vilan's own literal syntax — `Point { x = 1,
//! y = 2 }`, `Shape::Circle(1.5)`, `Some(5)`, `(1, "two", 3.0)`, `[1, 2]` — and
//! the two backends must write the same bytes. Each emitter generates one
//! printer per concrete type a `dbg` reaches (`__show_*` functions on JS,
//! `show_*` functions natively), and each asks THIS module what the type is
//! for printing: [`shape_of`] classifies it, and the labels it carries (the
//! qualified variant names, a closure's `<closure |i32| i32>`) are spelled
//! here, once. What the emitters add is only how a value of that shape is
//! READ on their backend: a JS struct is its field array, a native one has
//! named fields.
//!
//! The layout — one line when it fits 80 columns, else one entry per line
//! (a list or set of scalars filling each line instead, E277), two spaces
//! deeper, with a trailing comma — and the scalar spellings (a
//! float keeps its `.0`, a string is quoted and escaped as vilan writes it)
//! are the runtimes' (`__dbg_*` on JS, `vilan_rt::show` natively), written
//! twice and pinned against each other by the native differential.

use crate::analyzer::{BackingValue, DbgStackBinding, DbgStackSite, DbgStackValue, Program};
use crate::id::Id;
use crate::impl_select;
use crate::type_::{Type, TypeId};

/// What a value of one concrete type prints as.
#[derive(Clone, Debug)]
pub enum Shape {
    /// An integer of any width (`i32`, `u8`, `usize`, …): its decimal digits.
    Integer,
    /// `f32` / `f64`: the language's number text, with `.0` on an integral
    /// value (Q1).
    Float,
    /// `BigInt`: its digits.
    BigInt,
    Bool,
    /// `str`: quoted and escaped.
    Str,
    /// `void` / `()`.
    Void,
    /// A struct. `bindings` grounds its generic parameters for the field
    /// types; a field-less struct prints its bare name.
    Struct {
        name: String,
        fields: Vec<(String, TypeId)>,
        bindings: Vec<(TypeId, TypeId)>,
    },
    /// An enum over the array layout: each variant's printed label (`Some`,
    /// `Shape::Circle`) and payload types.
    Enum {
        variants: Vec<(String, Vec<TypeId>)>,
        bindings: Vec<(TypeId, TypeId)>,
    },
    /// A BACKED enum (backed-enums.md): a value IS its backing literal, so a
    /// variant is found by comparing against each one.
    Backed {
        variants: Vec<(String, BackingValue)>,
    },
    /// A tuple: its element types, in order, each with the text its entry
    /// opens with — `x = ` for a labelled slot (B569 S3: `dbg` prints the
    /// literal, `(x = 5.0, y = 7.0)`), empty for a positional one.
    Tuple(Vec<(String, TypeId)>),
    /// `List<T>` and `[T; n]`.
    List(TypeId),
    /// A value that prints as fixed text: a closure by its type, a pipe by
    /// its type (sampling one would run it), an opaque host handle by its
    /// name, a generic the build never grounded.
    Text(String),
    /// S1b: `Shared<T>` — `Shared(<value>)`, and `<cycle>` for a cell the
    /// print is already inside (only a `Shared` can close a cycle).
    Shared(TypeId),
    /// S1b: std's `SignalCell<T>` — `SignalCell(<value>)`, its current value
    /// read through the field at `field` (a `Shared<T>`) WITHOUT tracking.
    Cell {
        label: String,
        field: (usize, String),
        value: TypeId,
    },
    /// S1b: std's `HashMap<K, V>` — `HashMap { k => v, .. }`, its entries
    /// the `(K, V)` pairs of the table at `field`, in insertion order.
    Map {
        label: String,
        field: (usize, String),
        key: TypeId,
        value: TypeId,
    },
    /// S1b: std's `HashSet<T>` — `HashSet { a, b }`.
    Set {
        label: String,
        field: (usize, String),
        element: TypeId,
    },
    /// S1b: a trait object — `dyn Area(Square { side = 2 })`, its value
    /// printed by the `show` slot its table carries in a program that calls
    /// `dbg` (debugging.md §2.2). `label` is the object's type as written.
    Object {
        label: String,
    },
}

impl Shape {
    /// Whether a value of this shape prints as one short token — a number, a
    /// bool, a string, `()`, a backed enum or one whose variants carry nothing
    /// (`Color::Red`). A list or a set of scalars FILLS its broken lines up
    /// to the 80-column limit rather than taking a line per entry (E277); any
    /// other element keeps a line of its own.
    pub fn is_scalar(&self) -> bool {
        match self {
            Shape::Integer
            | Shape::Float
            | Shape::BigInt
            | Shape::Bool
            | Shape::Str
            | Shape::Void
            | Shape::Backed { .. } => true,
            Shape::Enum { variants, .. } => variants.iter().all(|(_, payload)| payload.is_empty()),
            _ => false,
        }
    }
}

/// The numeric scalars, by the name std declares them under.
fn is_integer_name(name: &str) -> bool {
    matches!(
        name,
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i53" | "u53" | "usize"
    )
}

/// S1b: whether a program's `dyn` tables carry the `show` slot — any program
/// whose build prints a `dbg` (a stripped build has no printers to point at).
/// The slot is one function reference per (trait, type) table, so a program
/// that never calls `dbg` pays nothing (§2.3).
pub fn tables_carry_show(program: &Program, policy: crate::options::DbgPolicy) -> bool {
    (policy != crate::options::DbgPolicy::Strip
        && (!program.dbg_calls.is_empty() || !program.dbg_stack_sites.is_empty()))
        || program
            .print_argument_types
            .values()
            .any(|type_id| may_hold_an_object(program, *type_id, &mut Vec::new()))
}

/// S3: whether a value of `type_id` may hold a trait object a `print` reaches —
/// the type names a `dyn`, or a generic an instance could bind to one.
fn may_hold_an_object(program: &Program, type_id: TypeId, seen: &mut Vec<TypeId>) -> bool {
    if seen.contains(&type_id) {
        return false;
    }
    seen.push(type_id);
    match program.type_id_to_type_map.get(&type_id) {
        Some(Type::Dyn(..) | Type::Generic(_)) => true,
        Some(Type::Tuple(elements, _)) => elements
            .iter()
            .any(|element| may_hold_an_object(program, *element, seen)),
        Some(Type::Array(element, _)) => may_hold_an_object(program, *element, seen),
        Some(Type::Struct(struct_id, arguments)) => {
            arguments
                .iter()
                .any(|argument| may_hold_an_object(program, *argument, seen))
                || program.structs.get(struct_id).is_some_and(|declaration| {
                    declaration
                        .fields
                        .iter()
                        .any(|field| may_hold_an_object(program, field.type_id, seen))
                })
        }
        Some(Type::Enum(enum_id, arguments)) => {
            arguments
                .iter()
                .any(|argument| may_hold_an_object(program, *argument, seen))
                || program.enums.get(enum_id).is_some_and(|declaration| {
                    declaration.variants.iter().any(|variant| {
                        variant
                            .data_type_ids
                            .iter()
                            .any(|payload| may_hold_an_object(program, *payload, seen))
                    })
                })
        }
        _ => false,
    }
}

/// debugging.md S3 (P2, the 2026-10-09 ruling): whether `print` writes a value
/// of this shape through the printer — on one line (`print` never breaks
/// lines), a float inside it keeping its `.0`, a backed enum by its NAME. A
/// number, a bool, a string and `()` keep `print`'s own rendering (`console.log`
/// on JS: `3.0` prints `3`, a string raw), and so does a host or opaque value
/// the printer could only name (`<Location>`), which `console.log` shows — a
/// bare closure among them, which the native backend refuses to print (F25).
/// A closure INSIDE an aggregate prints its type, as `dbg` prints it.
pub fn print_uses_the_printer(shape: &Shape) -> bool {
    match shape {
        Shape::Integer | Shape::Float | Shape::BigInt | Shape::Bool | Shape::Str | Shape::Void => {
            false
        }
        Shape::Text(text) => text.starts_with("<pipe "),
        Shape::Struct { .. }
        | Shape::Enum { .. }
        | Shape::Backed { .. }
        | Shape::Tuple(_)
        | Shape::List(_)
        | Shape::Shared(_)
        | Shape::Cell { .. }
        | Shape::Map { .. }
        | Shape::Set { .. }
        | Shape::Object { .. } => true,
    }
}

// --- `dbg_stack()` (debugging.md S2) ---
//
// A site prints a header line, then one line per listed binding:
//
//     [src/main.vl:12:5] dbg_stack() in main
//       rows: List<i32> = [1, 2]
//       first: view i32 = 1  (a view into rows)
//       guard: Guard = <moved at 9:10>
//       x (shadowed at 7:6): i32 = 1
//
// A value lays out from where it starts, its broken entries two spaces under
// the binding. The text around the values is spelled here, once, so the two
// emitters write the same bytes; each emitter only reads the value.

/// The header line's text after the location: `dbg_stack() in main`.
pub fn dbg_stack_title(site: &DbgStackSite) -> String {
    match &site.owner {
        Some(owner) => format!("dbg_stack() in {owner}"),
        None => "dbg_stack()".to_string(),
    }
}

/// A binding line's head, before ` = `: `x (shadowed at 7:6): view i32`. The
/// type is resolved under the emitter's active substitution.
pub fn dbg_stack_head(
    program: &Program,
    binding: &DbgStackBinding,
    resolve: &dyn Fn(TypeId) -> TypeId,
) -> String {
    let shadowed = binding
        .shadowed_at
        .as_ref()
        .map(|at| format!(" (shadowed at {at})"))
        .unwrap_or_default();
    let view = if binding.view { "view " } else { "" };
    format!(
        "{}{shadowed}: {view}{}",
        binding.name,
        type_text(program, binding.type_id, resolve)
    )
}

/// What a binding prints INSTEAD of its value, when it prints one: a moved
/// resource, an invalidated view, a pipe (sampling it would run it) and a
/// `lazy` parameter (reading it would force it) are never read.
pub fn dbg_stack_unread_value(
    program: &Program,
    binding: &DbgStackBinding,
    resolve: &dyn Fn(TypeId) -> TypeId,
) -> Option<String> {
    match &binding.value {
        DbgStackValue::Read(_) => is_pipe_type(program, binding.type_id, resolve)
            .then(|| "<pipe, not sampled>".to_string()),
        DbgStackValue::Moved(at) => Some(format!("<moved at {at}>")),
        DbgStackValue::MovedOnSomePaths => Some("<moved on some paths>".to_string()),
        DbgStackValue::Invalidated { by, at } => {
            Some(format!("<view, invalidated by {by} at {at}>"))
        }
        DbgStackValue::Pipe => Some("<pipe, not sampled>".to_string()),
        DbgStackValue::Lazy => Some("<lazy, not forced>".to_string()),
    }
}

/// What follows a binding's value: `  (a view into rows)`, `  (captured)`, a
/// cell's `  (read without tracking)` — or nothing.
pub fn dbg_stack_note(
    program: &Program,
    binding: &DbgStackBinding,
    resolve: &dyn Fn(TypeId) -> TypeId,
) -> String {
    let mut notes: Vec<String> = Vec::new();
    if !binding.view_into.is_empty() {
        notes.push(format!("a view into {}", binding.view_into.join(", ")));
    }
    if binding.captured {
        notes.push("captured".to_string());
    }
    if matches!(binding.value, DbgStackValue::Read(_))
        && matches!(
            shape_of(program, binding.type_id, resolve),
            Shape::Cell { .. }
        )
    {
        notes.push("read without tracking".to_string());
    }
    if notes.is_empty() {
        String::new()
    } else {
        format!("  ({})", notes.join(", "))
    }
}

/// Whether `type_id` (under `resolve`) is a std pipe, which prints by its type.
fn is_pipe_type(program: &Program, type_id: TypeId, resolve: &dyn Fn(TypeId) -> TypeId) -> bool {
    matches!(
        program.type_id_to_type_map.get(&resolve(type_id)),
        Some(Type::Struct(struct_id, _)) if is_a_pipe(program, *struct_id)
    )
}

/// E275: the trait a WRITTEN `Debug` impl for `type_id` is reached through —
/// `Some(Debug)` when `dbg` prints the type by calling that impl's `debug`
/// instead of by its structure.
///
/// The impl is the one B57's order selects for the concrete type, so a
/// written generic impl whose bound the type misses (`impl Boxed<type T:
/// Debug>` at `Boxed<Opaque>`) does not apply and the structure prints. Two
/// impls are NOT written ones and leave the structure in charge: std's own
/// (they spell what the printer spells, and the printer can lay a container
/// out over lines where a string cannot), and the one `[derive(Debug)]`
/// generates, which is the printer's spelling on one line by construction —
/// so a derived value still breaks past 80 columns.
pub fn written_debug(program: &Program, type_id: TypeId) -> Option<Id> {
    let concrete = program.type_id_to_type_map.get(&type_id)?;
    if !matches!(
        concrete,
        Type::Struct(..) | Type::Enum(..) | Type::Tuple(..) | Type::Array(..)
    ) || !impl_select::is_resolvable(concrete)
    {
        return None;
    }
    let trait_id = std_trait(program, "Debug")?;
    let implementation = impl_select::select_implementation(program, None, type_id, trait_id)?;
    if program.std_sources.contains(&implementation.source) {
        return None;
    }
    let member = *implementation.declarations.get("debug")?;
    if derived_by(program, member, "Debug") {
        return None;
    }
    Some(trait_id)
}

/// Whether `id` was generated by the attribute or derive NAMED `name` — the
/// expansion's origin is the name's span in the file that wrote it.
fn derived_by(program: &Program, id: Id, name: &str) -> bool {
    let Some((span, source)) = program.derived_origin(id) else {
        return false;
    };
    program
        .source_texts
        .iter()
        .find(|(candidate, _)| *candidate == source)
        .and_then(|(_, text)| text.get(span.start..span.end))
        == Some(name)
}

/// The trait std declares as `name` (a program's own trait of that name is
/// not it).
fn std_trait(program: &Program, name: &str) -> Option<Id> {
    program
        .traits
        .values()
        .filter(|declared| declared.name == name)
        .find(|declared| is_std(program, declared.id))
        .map(|declared| declared.id)
}

fn is_std(program: &Program, id: Id) -> bool {
    program
        .source_of(id)
        .is_some_and(|source| program.std_sources.contains(&source))
}

/// The LABELS a printer for `type_id` writes (B569 S3), spelled for a
/// printer cache's key: every tuple label the type reaches, in walk order,
/// and the empty string for a type that reaches none. The emitters key their
/// printers by the type's identity, which a label is no part of (labels
/// erase at mono), so a key that is only the identity would hand
/// `(x: i32, y: i32)` the printer `(i32, i32)` built first, or the reverse.
pub fn label_key(program: &Program, type_id: TypeId, resolve: &dyn Fn(TypeId) -> TypeId) -> String {
    fn walk(
        program: &Program,
        type_id: TypeId,
        resolve: &dyn Fn(TypeId) -> TypeId,
        out: &mut String,
    ) {
        let Some(_guard) = crate::util::RecursionGuard::enter() else {
            return;
        };
        let written = type_id;
        let type_id = resolve(type_id);
        let Some(resolved) = program.type_id_to_type_map.get(&type_id) else {
            return;
        };
        match resolved {
            Type::Tuple(elements, labels) => {
                // Erased through a substitution, as `shape_of` reads it.
                if written == type_id
                    && let Some(labels) = labels.labels()
                {
                    out.push('(');
                    out.push_str(&labels.join(","));
                    out.push(')');
                }
                for element in elements {
                    walk(program, *element, resolve, out);
                }
            }
            Type::Struct(_, arguments) | Type::Enum(_, arguments) => {
                for argument in arguments {
                    walk(program, *argument, resolve, out);
                }
            }
            Type::Array(element, _) => walk(program, *element, resolve, out),
            _ => {}
        }
    }
    let mut out = String::new();
    walk(program, type_id, resolve, &mut out);
    out
}

/// The printing shape of `type_id`. `resolve` grounds a generic under the
/// asking emitter's active substitution (and returns any other id as is);
/// the shape's own type ids are NOT resolved — an emitter recursing into a
/// field resolves it under the shape's `bindings`.
pub fn shape_of(program: &Program, type_id: TypeId, resolve: &dyn Fn(TypeId) -> TypeId) -> Shape {
    let written = type_id;
    let type_id = resolve(type_id);
    // B569 §5: labels print where the type was WRITTEN with them. A type that
    // reaches the printer through a generic's substitution reaches it erased:
    // label sets share one instance (§6.3), so the labels there are whichever
    // instantiation was emitted first, and printing them would be a guess.
    let erased = written != type_id;
    let Some(resolved) = program.type_id_to_type_map.get(&type_id) else {
        return Shape::Text("<unknown>".to_string());
    };
    match resolved {
        Type::Void => Shape::Void,
        Type::Tuple(elements, _) if elements.is_empty() => Shape::Void,
        Type::Tuple(elements, labels) => Shape::Tuple(
            elements
                .iter()
                .enumerate()
                .map(|(slot, element)| {
                    let opens = labels
                        .get(slot)
                        .filter(|_| !erased)
                        .map_or_else(String::new, |label| format!("{label} = "));
                    (opens, *element)
                })
                .collect(),
        ),
        Type::Array(element, _) => Shape::List(*element),
        Type::Closure(..) | Type::Function(_) => Shape::Text(format!(
            "<closure {}>",
            closure_text(program, type_id, resolve)
        )),
        Type::Struct(struct_id, _) if is_a_pipe(program, *struct_id) => {
            Shape::Text(format!("<pipe {}>", type_text(program, type_id, resolve)))
        }
        Type::Struct(struct_id, arguments) => struct_shape(program, *struct_id, arguments),
        Type::Enum(enum_id, arguments) => enum_shape(program, *enum_id, arguments),
        Type::Dyn(..) => Shape::Object {
            label: type_text(program, type_id, resolve),
        },
        Type::Generic(_) => Shape::Text(format!("<{}>", type_text(program, type_id, resolve))),
        _ => Shape::Text(format!("<{}>", type_text(program, type_id, resolve))),
    }
}

fn struct_shape(program: &Program, struct_id: Id, arguments: &[TypeId]) -> Shape {
    let Some(declaration) = program.structs.get(&struct_id) else {
        return Shape::Text("<struct>".to_string());
    };
    if declaration.external {
        if declaration.name == "Shared"
            && let Some(inner) = arguments.first()
        {
            return Shape::Shared(*inner);
        }
        return match declaration.name {
            name if is_integer_name(name) => Shape::Integer,
            "f32" | "f64" => Shape::Float,
            "BigInt" => Shape::BigInt,
            "str" => Shape::Str,
            "List" => match arguments.first() {
                Some(element) => Shape::List(*element),
                None => Shape::Text("[]".to_string()),
            },
            name => Shape::Text(format!("<{name}>")),
        };
    }
    if let Some(shape) = std_handle_shape(program, struct_id, arguments) {
        return shape;
    }
    let bindings = declaration
        .generic_parameter_constraint_ids
        .iter()
        .copied()
        .zip(arguments.iter().copied())
        .collect();
    Shape::Struct {
        name: declaration.name.to_string(),
        fields: declaration
            .fields
            .iter()
            .map(|field| (field.name.to_string(), field.type_id))
            .collect(),
        bindings,
    }
}

/// S1b: std's handles print as themselves (§2.2) rather than as their
/// internals: a cell by its value, a map and a set by their members, a pipe
/// by its type. Recognized by the declaration's NAME and its residence in std,
/// so a program's own `HashMap` is an ordinary struct.
fn std_handle_shape(program: &Program, struct_id: Id, arguments: &[TypeId]) -> Option<Shape> {
    let declaration = program.structs.get(&struct_id)?;
    if !is_std(program, struct_id) {
        return None;
    }
    let field = |name: &str| {
        declaration
            .fields
            .iter()
            .position(|field| field.name == name)
            .map(|index| (index, name.to_string()))
    };
    match declaration.name {
        "SignalCell" => Some(Shape::Cell {
            label: "SignalCell".to_string(),
            field: field("value")?,
            value: *arguments.first()?,
        }),
        "HashMap" => Some(Shape::Map {
            label: "HashMap".to_string(),
            field: field("table")?,
            key: *arguments.first()?,
            value: *arguments.get(1)?,
        }),
        "HashSet" => Some(Shape::Set {
            label: "HashSet".to_string(),
            field: field("table")?,
            element: *arguments.first()?,
        }),
        _ => None,
    }
}

/// Whether std's `Pipe`/`CollPipe` is implemented for the struct — a pipe
/// prints by its type alone, because sampling it would run its bodies.
fn is_a_pipe(program: &Program, struct_id: Id) -> bool {
    let pipe_traits: Vec<Id> = program
        .traits
        .values()
        .filter(|declared| matches!(declared.name, "Pipe" | "CollPipe"))
        .filter(|declared| is_std(program, declared.id))
        .map(|declared| declared.id)
        .collect();
    if pipe_traits.is_empty() {
        return false;
    }
    program.implementations.iter().any(|implementation| {
        implementation
            .trait_ids
            .iter()
            .any(|trait_id| pipe_traits.contains(trait_id))
            && matches!(
                program.type_id_to_type_map.get(&implementation.subject),
                Some(Type::Struct(subject, _)) if *subject == struct_id
            )
    })
}

fn enum_shape(program: &Program, enum_id: Id, arguments: &[TypeId]) -> Shape {
    if program.bool_enum_id == Some(enum_id) {
        return Shape::Bool;
    }
    let Some(declaration) = program.enums.get(&enum_id) else {
        return Shape::Text("<enum>".to_string());
    };
    // The prelude's four print bare, as they are written (§2.1) — std's
    // `Option` and `Result`, not a program's own enum of either name.
    let bare = matches!(declaration.name, "Option" | "Result") && is_std(program, enum_id);
    let label = |variant: &str| {
        if bare {
            variant.to_string()
        } else {
            format!("{}::{variant}", declaration.name)
        }
    };
    if declaration.backing.is_some() {
        return Shape::Backed {
            variants: declaration
                .variants
                .iter()
                .map(|variant| (label(variant.name), variant.backing_value.clone()))
                .collect(),
        };
    }
    let bindings = declaration
        .generic_parameter_constraint_ids
        .iter()
        .copied()
        .zip(arguments.iter().copied())
        .collect();
    Shape::Enum {
        variants: declaration
            .variants
            .iter()
            .map(|variant| (label(variant.name), variant.data_type_ids.clone()))
            .collect(),
        bindings,
    }
}

/// `|i32, str| bool`: a closure type as `dbg` prints it — vilan's own type
/// syntax, the result directly after the parameters.
fn closure_text(program: &Program, type_id: TypeId, resolve: &dyn Fn(TypeId) -> TypeId) -> String {
    match program.type_id_to_type_map.get(&resolve(type_id)) {
        Some(Type::Closure(parameters, return_type, _, _)) => {
            let parameters: Vec<String> = parameters
                .iter()
                .map(|parameter| type_text(program, *parameter, resolve))
                .collect();
            format!(
                "|{}| {}",
                parameters.join(", "),
                type_text(program, *return_type, resolve)
            )
        }
        Some(Type::Function(function_id)) => program
            .functions
            .get(function_id)
            .map(|function| format!("fun {}", function.name))
            .unwrap_or_else(|| "fun".to_string()),
        _ => "||".to_string(),
    }
}

/// A type as vilan writes it — `List<Point>`, `(i32, str)`, `|i32| bool`.
pub fn type_text(program: &Program, type_id: TypeId, resolve: &dyn Fn(TypeId) -> TypeId) -> String {
    let Some(_guard) = crate::util::RecursionGuard::enter() else {
        return "..".to_string();
    };
    let type_id = resolve(type_id);
    let arguments_text = |arguments: &[TypeId]| {
        if arguments.is_empty() {
            String::new()
        } else {
            let parts: Vec<String> = arguments
                .iter()
                .map(|argument| type_text(program, *argument, resolve))
                .collect();
            format!("<{}>", parts.join(", "))
        }
    };
    match program.type_id_to_type_map.get(&type_id) {
        Some(Type::Void) => "void".to_string(),
        Some(Type::Struct(id, arguments)) => format!(
            "{}{}",
            program
                .structs
                .get(id)
                .map(|declaration| declaration.name)
                .unwrap_or("?"),
            arguments_text(arguments)
        ),
        Some(Type::Enum(id, arguments)) => format!(
            "{}{}",
            program
                .enums
                .get(id)
                .map(|declaration| declaration.name)
                .unwrap_or("?"),
            arguments_text(arguments)
        ),
        Some(Type::Dyn(id, arguments)) => format!(
            "dyn {}{}",
            program
                .traits
                .get(id)
                .map(|declaration| declaration.name)
                .unwrap_or("?"),
            arguments_text(arguments)
        ),
        Some(Type::Tuple(elements, _)) => {
            let parts: Vec<String> = elements
                .iter()
                .map(|element| type_text(program, *element, resolve))
                .collect();
            format!("({})", parts.join(", "))
        }
        Some(Type::Array(element, length)) => {
            format!("[{}; {length}]", type_text(program, *element, resolve))
        }
        Some(Type::Closure(..)) | Some(Type::Function(_)) => {
            closure_text(program, type_id, resolve)
        }
        // A parameter the build never grounded (a generic body emitted
        // without an instantiation): its name is not on the program.
        Some(Type::Generic(_)) => "T".to_string(),
        Some(Type::Never) => "never".to_string(),
        Some(Type::Any) => "any".to_string(),
        _ => "?".to_string(),
    }
}
