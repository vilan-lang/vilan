//! `vilan-rust` — the emit-Rust backend (tracker F1, slice S1a).
//!
//! The same `Program` the JS emitter reads, a Rust source file out. It sits
//! BESIDE `transformer.rs` rather than inside it, because the two share an
//! input and nothing else: the JS emitter's whole job is a dynamically typed
//! target where a struct is an array and a closure is a reference, and this
//! one's is a statically typed target where a struct is a struct and a closure
//! has to say who counts it.
//!
//! # Scope, which is hard and deliberate
//!
//! S1a is the FIRST cut of `native-apps.md` §5-S1. What it emits is what the
//! paper's probe translated: structs, enums, `Option`/`Result`, `str`, `List`,
//! `Map`/`Set`, closures, `impl`s, `print`, `panic`, and the counted cell. What
//! it does not emit — async, UI, rpc, the filesystem, the platform surface,
//! generic functions, module-level bindings — it REFUSES, by name, with a
//! sentence saying which. A backend that silently emitted something else for a
//! construct it did not understand would fail the differential in a way nobody
//! could read; a backend that names the construct turns its own gaps into the
//! work list for S1b, and [`crates/vilan-cli/tests/native_differential.rs`]
//! prints exactly that list.
//!
//! # Why the walk is cheap
//!
//! Most of what makes `transformer.rs` twelve thousand lines is already done by
//! the time a program reaches here. Contexts are threaded into ordinary
//! parameters, `const` is folded, and — the one that matters most — a method
//! call `p.bump(2)` has already been RESOLVED by the analyzer into a call whose
//! subject is the member's own id and whose first argument is the receiver. So
//! this emitter needs no impl selection and no dispatch table for the concrete
//! case; it needs one for the GENERIC case, which is precisely the case it
//! refuses in S1a.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use vilan_core::analyzer::{Expr, ExprIfBranch, ExprMatchLeg, ExprPattern, Intrinsic, Program};
use vilan_core::error::Error;
use vilan_core::id::Id;
use vilan_core::node::{BinaryOp, Convention, ExternBinding};
use vilan_core::options::BuildOptions;
use vilan_core::span::Span;
use vilan_core::type_::{Type, TypeId};

/// What one emit produced: the Rust source, and the measurement R3 asked for.
pub struct Emitted {
    /// The whole program, as one `main.rs`.
    pub source: String,
    /// R3's measurement: how many bindings this program had to box into
    /// `vilan_rt::Captured<_>` (an `Rc<RefCell<_>>`) because a closure captures
    /// them and something writes them. C15's by-value capture optimisation is
    /// the later item this number pays for.
    pub boxed_bindings: usize,
}

/// Emits `program` as a single Rust source file.
///
/// `Err` carries the construct S1a does not reach, located at the expression
/// that wrote it, in exactly the shape a compiler diagnostic takes — so
/// `vilan build --backend rust` reports an unsupported program the way it
/// reports any other refusal, rather than producing Rust that will not build.
pub fn emit(program: &Program<'_>, _options: &BuildOptions) -> Result<Emitted, Error> {
    Emitter::new(program).run()
}

/// The prelude every emitted program carries.
const PRELUDE: &str = "\
#![allow(unused_imports, unused_parens, unused_variables, dead_code, clippy::all)]
use vilan_rt::Js as _;
";

fn unsupported(what: &str, span: Span) -> Error {
    Error {
        trace: Vec::new(),
        note: None,
        span,
        msg: format!(
            "the `rust` backend does not emit {what} yet — this is F1's slice S1a, \
             whose scope is structs, enums, `Option`/`Result`, `str`, `List`, `Map`/`Set`, \
             closures, `impl`s, `print`, `panic` and the reactive cell. Build this program \
             with `--backend js`."
        ),
    }
}

struct Emitter<'a, 'src> {
    program: &'a Program<'src>,
    /// Function bodies, keyed by id so the output is deterministic.
    functions: BTreeMap<u32, String>,
    /// Functions already emitted or in progress — recursion needs the mark
    /// BEFORE the body is walked, exactly as the JS emitter's does.
    started: HashSet<Id>,
    /// Nominal types the program reached, emitted once each.
    types: BTreeMap<u32, String>,
    started_types: HashSet<Id>,
    /// R3: the bindings boxed into a counted cell, and why they had to be.
    boxed: HashSet<Id>,
    /// Every module-level binding in the world, refused at the READ.
    module_bindings: HashSet<Id>,
}

impl<'a, 'src> Emitter<'a, 'src> {
    fn new(program: &'a Program<'src>) -> Self {
        Emitter {
            program,
            functions: BTreeMap::new(),
            started: HashSet::new(),
            types: BTreeMap::new(),
            started_types: HashSet::new(),
            boxed: HashSet::new(),
            module_bindings: HashSet::new(),
        }
    }

    fn run(mut self) -> Result<Emitted, Error> {
        let global_scope = self
            .program
            .scopes
            .get(&self.program.global_scope_id)
            .ok_or_else(|| unsupported("a program with no global scope", Span::new((), 0..0)))?;
        let main_id = *global_scope
            .name_to_id_map
            .get("main")
            .filter(|id| self.program.functions.contains_key(*id))
            .ok_or_else(|| Error {
                trace: Vec::new(),
                note: None,
                msg: "Cannot execute program without a main function".to_string(),
                span: Span::new((), 0..0),
            })?;

        // A module-level binding is a `static` with a constructor, which needs a
        // `thread_local!` and an initialization order this slice does not build.
        // Recorded now and refused where one is READ, not where one exists: std
        // declares several (`PI`, the reactive turn's registers) that a program
        // reaching none of them must not be refused for.
        self.module_bindings = self.program.module_level_bindings().into_iter().collect();
        self.compute_boxed_bindings();

        self.ensure_function(main_id)?;
        let main_body = self
            .functions
            .remove(&main_id.0)
            .expect("main was just emitted");

        let mut source = String::from(PRELUDE);
        for declaration in self.types.values() {
            source.push('\n');
            source.push_str(declaration);
        }
        for body in self.functions.values() {
            source.push('\n');
            source.push_str(body);
        }
        source.push('\n');
        source.push_str(&main_body);
        Ok(Emitted {
            source,
            boxed_bindings: self.boxed.len(),
        })
    }

    /// R3, as ruled: v1 boxes EVERY mutably-captured binding into a counted
    /// cell (`Rc<RefCell<_>>`), and the count over the exit corpus is the
    /// measurement C15's by-value capture optimisation has to beat.
    ///
    /// Spec §6.9 is why there is no choice here: a closure captures the
    /// BINDING, not the value, so a `mut` local a closure reads is a place two
    /// frames share. JavaScript boxes every place for free; natively the box is
    /// the emitter's to write.
    fn compute_boxed_bindings(&mut self) {
        let closures: Vec<Id> = self.program.closures.keys().copied().collect();
        for closure_id in closures {
            let Some(closure) = self.program.closures.get(&closure_id) else {
                continue;
            };
            let mut declared_inside = HashSet::new();
            let mut referenced = HashSet::new();
            let mut visited = HashSet::new();
            self.scan_closure(
                closure.return_,
                &mut declared_inside,
                &mut referenced,
                &mut visited,
            );
            for binding in referenced {
                if declared_inside.contains(&binding) {
                    continue;
                }
                if self
                    .program
                    .variables
                    .get(&binding)
                    .is_some_and(|variable| variable.mutable)
                {
                    self.boxed.insert(binding);
                }
            }
        }
    }

    /// Walks a closure body, collecting the bindings it DECLARES and the
    /// bindings it READS. The difference is what it captured.
    fn scan_closure(
        &self,
        expr_id: Id,
        declared: &mut HashSet<Id>,
        referenced: &mut HashSet<Id>,
        visited: &mut HashSet<Id>,
    ) {
        if !visited.insert(expr_id) {
            return;
        }
        match self.program.entity_map.get(&expr_id) {
            Some(Expr::Variable(binding)) => {
                declared.insert(*binding);
                if let Some(initial) = self
                    .program
                    .variables
                    .get(binding)
                    .and_then(|variable| variable.initial)
                {
                    self.scan_closure(initial, declared, referenced, visited);
                }
            }
            Some(Expr::Local(binding)) => {
                referenced.insert(*binding);
            }
            Some(_) => {
                for child in self.children_of(expr_id) {
                    self.scan_closure(child, declared, referenced, visited);
                }
            }
            None => {}
        }
    }

    /// Every sub-expression of `expr_id`, for the walks that only need to
    /// recurse. Written once rather than per walk: an arm this misses is a
    /// capture the box would not see, so there is exactly one list to keep.
    fn children_of(&self, expr_id: Id) -> Vec<Id> {
        let Some(expr) = self.program.entity_map.get(&expr_id) else {
            return Vec::new();
        };
        let mut children = Vec::new();
        match expr {
            Expr::Assignment(a, b) | Expr::Binary(_, a, b) | Expr::Index(a, b) => {
                children.push(*a);
                children.push(*b);
            }
            Expr::Async(a)
            | Expr::Await(a)
            | Expr::Unary(_, a)
            | Expr::Reference(a, _)
            | Expr::Dereference(a)
            | Expr::TryAssert(a)
            | Expr::Field(a, _, _)
            | Expr::TupleIndex(a, _, _)
            | Expr::ArrayLen(a, _)
            | Expr::Repeat(a, _) => children.push(*a),
            Expr::FunctionReturn(value) => children.extend(value.iter().copied()),
            Expr::Block((statements, tail)) => {
                children.extend(statements.iter().copied());
                children.push(*tail);
            }
            Expr::List(ids) | Expr::Tuple(ids) => children.extend(ids.iter().copied()),
            Expr::StructInitializer(_, fields) => children.extend(fields.values().copied()),
            Expr::For(condition, (statements, tail)) => {
                children.extend(condition.iter().copied());
                children.extend(statements.iter().copied());
                children.push(*tail);
            }
            Expr::ForEach(iterable, _, (statements, tail)) => {
                children.push(*iterable);
                children.extend(statements.iter().copied());
                children.push(*tail);
            }
            Expr::If(branch) => collect_if_children(branch, &mut children),
            Expr::Match(subject, legs) => {
                children.push(*subject);
                for leg in legs {
                    children.extend(leg.guard.iter().copied());
                    children.push(leg.body);
                }
            }
            Expr::Is(subject, _) | Expr::Destructure(subject, _) => children.push(*subject),
            Expr::Lift(subject, _, continuation) => {
                children.push(*subject);
                children.push(*continuation);
            }
            Expr::LiftRegion(steps, body) => {
                children.extend(steps.iter().map(|(step, _, _)| *step));
                children.push(*body);
            }
            Expr::TupleComprehension(a, b, c) => {
                children.push(*a);
                children.push(*b);
                children.push(*c);
            }
            Expr::Call(call_id) => {
                if let Some(function_call) = self.program.function_calls.get(call_id) {
                    children.push(function_call.subject_id);
                    children.extend(function_call.argument_ids.iter().copied());
                }
            }
            Expr::Closure(closure_id) => {
                if let Some(closure) = self.program.closures.get(closure_id) {
                    children.push(closure.return_);
                }
            }
            _ => {}
        }
        children
    }

    fn span_of(&self, id: Id) -> Span {
        self.program
            .span_map
            .get(&id)
            .map(|span| **span)
            .unwrap_or(Span::new((), 0..0))
    }

    // ------------------------------------------------------------- names ---

    /// Emitted names carry the vilan name AND the id: the id is what makes two
    /// same-named members of different impls distinct without a rename pass,
    /// and the name is what makes the emitted source readable when rustc
    /// reports something about it.
    fn function_name(&self, id: Id) -> String {
        let name = self
            .program
            .functions
            .get(&id)
            .map(|function| function.name)
            .unwrap_or("fun");
        format!("{}_{}", sanitize(name), id.0)
    }

    fn binding_name(&self, id: Id) -> String {
        let name = self
            .program
            .variables
            .get(&id)
            .map(|variable| variable.name)
            .or_else(|| {
                self.program
                    .parameters
                    .get(&id)
                    .map(|parameter| parameter.name)
            })
            .unwrap_or("x");
        if name == "self" {
            return "this".to_string();
        }
        format!("{}_{}", sanitize(name), id.0)
    }

    // ------------------------------------------------------------- types ---

    fn resolve(&self, type_id: TypeId) -> Option<&Type> {
        self.program.type_id_to_type_map.get(&type_id)
    }

    fn rust_type(&mut self, type_id: TypeId, span: Span) -> Result<String, Error> {
        let Some(resolved) = self.resolve(type_id).cloned() else {
            return Err(unsupported("a value whose type did not resolve", span));
        };
        match resolved {
            Type::Void => Ok("()".to_string()),
            Type::Tuple(elements) => {
                let mut parts = Vec::new();
                for element in &elements {
                    parts.push(self.rust_type(*element, span)?);
                }
                Ok(format!("({})", parts.join(", ")))
            }
            Type::Array(element, length) => {
                let element = self.rust_type(element, span)?;
                Ok(format!("[{element}; {length}]"))
            }
            Type::Closure(parameters, return_type, _) => {
                let mut parts = Vec::new();
                for parameter in &parameters {
                    parts.push(self.rust_type(*parameter, span)?);
                }
                let returned = self.rust_type(return_type, span)?;
                // F16, applied as ruled: a closure VALUE that can reach a
                // storing position is counted. A closure TYPE written in a
                // signature or a field is exactly such a position — the only
                // closure that is provably call-only is a parameter this
                // emitter can see the body of, and `parameter_type` decides
                // that one. Everything else is `Rc<dyn Fn>`.
                Ok(format!(
                    "std::rc::Rc<dyn Fn({}) -> {}>",
                    parts.join(", "),
                    returned
                ))
            }
            Type::Struct(id, arguments) => self.nominal_struct(id, &arguments, span),
            Type::Enum(id, arguments) => self.nominal_enum(id, &arguments, span),
            Type::Generic(_) => Err(unsupported("a generic type parameter", span)),
            other => Err(unsupported(
                &format!("a value of type `{}`", describe(&other)),
                span,
            )),
        }
    }

    fn nominal_struct(
        &mut self,
        id: Id,
        arguments: &[TypeId],
        span: Span,
    ) -> Result<String, Error> {
        let Some(declaration) = self.program.structs.get(&id) else {
            return Err(unsupported("an unresolved struct", span));
        };
        let name = declaration.name;
        if let Some(scalar) = scalar_type(name) {
            return Ok(scalar.to_string());
        }
        let mut rendered = Vec::new();
        for argument in arguments {
            rendered.push(self.rust_type(*argument, span)?);
        }
        match name {
            "List" => Ok(format!(
                "Vec<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            "Set" => Ok(format!(
                "vilan_rt::Set<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            "Map" | "NativeMap" => Ok(format!("vilan_rt::Map<{}>", rendered.join(", "))),
            "Shared" | "SignalCell" => Ok(format!(
                "vilan_rt::Shared<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            _ if declaration.external => Err(unsupported(&format!("the host type `{name}`"), span)),
            _ => {
                if !arguments.is_empty() {
                    return Err(unsupported(&format!("the generic struct `{name}`"), span));
                }
                self.ensure_struct(id, span)?;
                Ok(format!("{}_{}", sanitize(name), id.0))
            }
        }
    }

    fn nominal_enum(&mut self, id: Id, arguments: &[TypeId], span: Span) -> Result<String, Error> {
        let Some(declaration) = self.program.enums.get(&id) else {
            return Err(unsupported("an unresolved enum", span));
        };
        let name = declaration.name;
        let mut rendered = Vec::new();
        for argument in arguments {
            rendered.push(self.rust_type(*argument, span)?);
        }
        match name {
            "bool" => Ok("bool".to_string()),
            "Option" => Ok(format!(
                "Option<{}>",
                rendered
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "()".to_string())
            )),
            "Result" => Ok(format!("Result<{}>", rendered.join(", "))),
            _ => {
                if !arguments.is_empty() {
                    return Err(unsupported(&format!("the generic enum `{name}`"), span));
                }
                self.ensure_enum(id, span)?;
                Ok(format!("{}_{}", sanitize(name), id.0))
            }
        }
    }

    fn ensure_struct(&mut self, id: Id, span: Span) -> Result<(), Error> {
        let declaration = self.program.structs.get(&id).cloned().unwrap();
        // The refusal comes BEFORE the once-only mark, or a first call that
        // swallowed the error would let a second one through on the mark alone
        // and emit a reference to a type nothing declared.
        if declaration.resource {
            return Err(unsupported(
                &format!(
                    "the `resource` type `{}` (destruction.md's teardown is a later slice)",
                    declaration.name
                ),
                span,
            ));
        }
        if !self.started_types.insert(id) {
            return Ok(());
        }
        let mut fields = Vec::new();
        for field in &declaration.fields {
            let rendered = self.rust_type(field.type_id, span)?;
            fields.push(format!("    {}: {rendered},", sanitize(field.name)));
        }
        let mut out = String::new();
        let _ = writeln!(out, "#[derive(Clone, PartialEq)]");
        let _ = writeln!(out, "struct {}_{} {{", sanitize(declaration.name), id.0);
        for field in fields {
            let _ = writeln!(out, "{field}");
        }
        let _ = writeln!(out, "}}");
        // `print(value)` is `console.log`, and on the JS backend a struct value
        // IS its flat field array — so a struct prints as `[ a, b ]`. The
        // rendering is emitted beside the declaration rather than derived,
        // because `vilan_rt` cannot name a type the emitter just invented.
        let type_name = format!("{}_{}", sanitize(declaration.name), id.0);
        let parts: Vec<String> = declaration
            .fields
            .iter()
            .map(|field| format!("self.{}.js_nested()", sanitize(field.name)))
            .collect();
        let _ = writeln!(out, "impl vilan_rt::Js for {type_name} {{");
        let _ = writeln!(out, "    fn js(&self) -> String {{");
        let _ = writeln!(out, "        vilan_rt::js_tuple(&[{}])", parts.join(", "));
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "}}");
        self.types.insert(id.0, out);
        Ok(())
    }

    fn ensure_enum(&mut self, id: Id, span: Span) -> Result<(), Error> {
        let declaration = self.program.enums.get(&id).cloned().unwrap();
        if declaration.resource {
            return Err(unsupported(
                &format!(
                    "the `resource` enum `{}` (destruction.md's teardown is a later slice)",
                    declaration.name
                ),
                span,
            ));
        }
        if declaration.backing.is_some() {
            return Err(unsupported(
                &format!("the backed enum `{}`", declaration.name),
                span,
            ));
        }
        if !self.started_types.insert(id) {
            return Ok(());
        }
        let mut variants = Vec::new();
        for variant in &declaration.variants {
            let mut payload = Vec::new();
            for data_type_id in &variant.data_type_ids {
                payload.push(self.rust_type(*data_type_id, span)?);
            }
            if payload.is_empty() {
                variants.push(format!("    {},", sanitize(variant.name)));
            } else {
                variants.push(format!(
                    "    {}({}),",
                    sanitize(variant.name),
                    payload.join(", ")
                ));
            }
        }
        let mut out = String::new();
        let _ = writeln!(out, "#[derive(Clone, PartialEq)]");
        let _ = writeln!(out, "enum {}_{} {{", sanitize(declaration.name), id.0);
        for variant in variants {
            let _ = writeln!(out, "{variant}");
        }
        let _ = writeln!(out, "}}");
        let type_name = format!("{}_{}", sanitize(declaration.name), id.0);
        let _ = writeln!(out, "impl vilan_rt::Js for {type_name} {{");
        let _ = writeln!(out, "    fn js(&self) -> String {{");
        let _ = writeln!(out, "        match self {{");
        for (index, variant) in declaration.variants.iter().enumerate() {
            let name = sanitize(variant.name);
            if variant.data_type_ids.is_empty() {
                let _ = writeln!(
                    out,
                    "            {type_name}::{name} => vilan_rt::js_tuple(&[\"{index}\".to_string()]),"
                );
            } else {
                let binders: Vec<String> = (0..variant.data_type_ids.len())
                    .map(|slot| format!("p{slot}"))
                    .collect();
                let mut parts = vec![format!("\"{index}\".to_string()")];
                parts.extend(binders.iter().map(|binder| format!("{binder}.js_nested()")));
                let _ = writeln!(
                    out,
                    "            {type_name}::{name}({}) => vilan_rt::js_tuple(&[{}]),",
                    binders.join(", "),
                    parts.join(", ")
                );
            }
        }
        let _ = writeln!(out, "        }}");
        let _ = writeln!(out, "    }}");
        let _ = writeln!(out, "}}");
        self.types.insert(id.0, out);
        Ok(())
    }

    fn type_of(&self, expr_id: Id) -> Option<TypeId> {
        if let Some(type_id) = self.program.expr_type_ids.get(&expr_id) {
            return Some(*type_id);
        }
        match self.program.entity_map.get(&expr_id)? {
            Expr::Local(binding) | Expr::Variable(binding) => self
                .program
                .variables
                .get(binding)
                .map(|variable| variable.type_id)
                .or_else(|| {
                    self.program
                        .parameters
                        .get(binding)
                        .map(|parameter| parameter.type_id)
                }),
            Expr::Parameter(binding) => self
                .program
                .parameters
                .get(binding)
                .map(|parameter| parameter.type_id),
            Expr::Call(call_id) => self.program.inferred_return_types.get(call_id).copied(),
            _ => None,
        }
    }

    // --------------------------------------------------------- functions ---

    fn ensure_function(&mut self, id: Id) -> Result<(), Error> {
        if !self.started.insert(id) {
            return Ok(());
        }
        let function =
            self.program.functions.get(&id).cloned().ok_or_else(|| {
                unsupported("a call to a function with no body", self.span_of(id))
            })?;
        let span = function.name_span;
        if !function.has_body {
            return Err(unsupported(
                &format!("the body-less function `{}`", function.name),
                span,
            ));
        }
        if !function.generic_parameter_constraint_ids.is_empty() {
            return Err(unsupported(
                &format!(
                    "the generic function `{}` (monomorphisation is S1b's)",
                    function.name
                ),
                span,
            ));
        }
        if function.is_async {
            return Err(unsupported(
                &format!("the async function `{}`", function.name),
                span,
            ));
        }

        let is_main = self
            .program
            .scopes
            .get(&self.program.global_scope_id)
            .and_then(|scope| scope.name_to_id_map.get("main"))
            == Some(&id);

        let mut parameters = Vec::new();
        for parameter_id in &function.parameters {
            parameters.push(self.parameter_declaration(*parameter_id, span)?);
        }
        let returned = match function.return_type_id {
            Some(type_id) => {
                let rendered = self.rust_type(type_id, span)?;
                // The type system has no reference form — a `borrows` function's
                // return type IS its pointee's, and whether a view comes back is
                // recorded beside it. Without this the signature says `i32`
                // where the body hands back `&mut i32`.
                if function.returns_mut_view {
                    format!("&mut {rendered}")
                } else if function.returns_view {
                    format!("&{rendered}")
                } else {
                    rendered
                }
            }
            None => "()".to_string(),
        };

        let mut body = String::new();
        self.emit_block(&function.body.0, function.body.1, &mut body, 1)?;

        let mut out = String::new();
        if is_main {
            let _ = writeln!(out, "fn main() {{");
        } else {
            let _ = writeln!(
                out,
                "fn {}({}) -> {returned} {{",
                self.function_name(id),
                parameters.join(", ")
            );
        }
        out.push_str(&body);
        let _ = writeln!(out, "}}");
        self.functions.insert(id.0, out);
        Ok(())
    }

    fn parameter_declaration(&mut self, id: Id, span: Span) -> Result<String, Error> {
        let parameter = self
            .program
            .parameters
            .get(&id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved parameter", span))?;
        if parameter.spread {
            return Err(unsupported("a spread parameter", span));
        }
        if parameter.lazy {
            return Err(unsupported("a `lazy` parameter", span));
        }
        let rendered = self.rust_type(parameter.type_id, span)?;
        let declaration = match self.receiving_form(&parameter) {
            Receiving::ByValue => rendered,
            Receiving::Ref => format!("&{rendered}"),
            Receiving::RefMut => format!("&mut {rendered}"),
        };
        Ok(format!("{}: {declaration}", self.binding_name(id)))
    }

    /// How a parameter is RECEIVED natively.
    ///
    /// The three conventions map straight across, with one synthesis: a BARE
    /// `self` is a loan (spec §6.8 R3 — the receiver is not consumed), so it
    /// arrives as `&T` even though nothing in the source wrote an `&`. Every
    /// other bare parameter is a by-value copy, which rule 1 already paid for
    /// at the call site.
    fn receiving_form(&self, parameter: &vilan_core::analyzer::Parameter<'_>) -> Receiving {
        match parameter.convention {
            Convention::Ref => Receiving::Ref,
            Convention::RefMut => Receiving::RefMut,
            Convention::Bare if parameter.name == "self" => Receiving::Ref,
            Convention::Bare | Convention::Own => Receiving::ByValue,
        }
    }

    // ---------------------------------------------------------- the walk ---

    fn indent(depth: usize) -> String {
        "    ".repeat(depth)
    }

    fn emit_block(
        &mut self,
        statements: &[Id],
        tail: Id,
        out: &mut String,
        depth: usize,
    ) -> Result<(), Error> {
        let pad = Self::indent(depth);
        for statement in statements {
            let rendered = self.statement(*statement, depth)?;
            if !rendered.is_empty() {
                let _ = writeln!(out, "{pad}{rendered}");
            }
        }
        if !matches!(self.program.entity_map.get(&tail), Some(Expr::Void) | None) {
            // A block's trailing expression is a VALUE position — it is what the
            // block evaluates to — so rule 1's copy is owed here exactly as it
            // is at an argument. `fun to_string(self): str { self }` is the
            // shape: the tail reads a loan and the signature hands back a value.
            let rendered = self.value_of(tail, depth)?;
            let _ = writeln!(out, "{pad}{rendered}");
        }
        Ok(())
    }

    /// One statement, which is an expression plus a `;` for every form that
    /// needs one. `if`, `match` and the loops are statements in Rust already.
    fn statement(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        match self.program.entity_map.get(&id) {
            Some(Expr::Void) | None => Ok(String::new()),
            Some(Expr::If(_)) | Some(Expr::For(_, _)) | Some(Expr::ForEach(_, _, _)) => {
                self.expression(id, depth)
            }
            _ => Ok(format!("{};", self.expression(id, depth)?)),
        }
    }

    fn expression(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        let span = self.span_of(id);
        let Some(expr) = self.program.entity_map.get(&id).cloned() else {
            return Ok("()".to_string());
        };
        let rendered = match expr {
            Expr::Bool(value) => value.to_string(),
            Expr::Void | Expr::Null => "()".to_string(),
            Expr::Number(whole, fraction, suffix) => {
                self.number_literal(id, whole, fraction, suffix, span)?
            }
            Expr::String(text) => format!("vilan_rt::str_new({})", rust_string(text)),
            Expr::MultilineString(_) => {
                return Err(unsupported("a triple-quoted string", span));
            }
            Expr::Local(binding) => {
                if self.module_bindings.contains(&binding) {
                    let name = self
                        .program
                        .variables
                        .get(&binding)
                        .map(|variable| variable.name)
                        .unwrap_or("it");
                    return Err(unsupported(
                        &format!("a read of the module-level binding `{name}`"),
                        span,
                    ));
                }
                self.read_binding(binding)
            }
            Expr::Parameter(binding) => self.binding_name(binding),
            Expr::Variable(binding) => self.declaration(binding, depth)?,
            Expr::Block((statements, tail)) => {
                let mut body = String::new();
                self.emit_block(&statements, tail, &mut body, depth + 1)?;
                format!("{{\n{body}{}}}", Self::indent(depth))
            }
            Expr::Binary(op, left, right) => self.binary(id, op, left, right, depth, span)?,
            Expr::Unary(op, operand) => match op {
                '!' => format!("!({})", self.expression(operand, depth)?),
                '-' => format!("-({})", self.expression(operand, depth)?),
                other => {
                    return Err(unsupported(&format!("the unary operator `{other}`"), span));
                }
            },
            Expr::If(branch) => self.if_branch(&branch, depth)?,
            Expr::Match(subject, legs) => self.match_expr(subject, &legs, depth, span)?,
            Expr::For(condition, (statements, tail)) => {
                let mut body = String::new();
                self.emit_block(&statements, tail, &mut body, depth + 1)?;
                let pad = Self::indent(depth);
                match condition {
                    Some(condition) => {
                        let condition = self.expression(condition, depth)?;
                        format!("while {condition} {{\n{body}{pad}}}")
                    }
                    None => format!("loop {{\n{body}{pad}}}"),
                }
            }
            Expr::ForEach(iterable, item, (statements, tail)) => {
                self.for_each(id, iterable, item, &statements, tail, depth, span)?
            }
            Expr::Jump(keyword) => match keyword {
                "break" => "break".to_string(),
                "continue" => "continue".to_string(),
                other => return Err(unsupported(&format!("`jump {other}`"), span)),
            },
            Expr::FunctionReturn(value) => match value {
                Some(value) => format!("return {}", self.value_of(value, depth)?),
                None => "return".to_string(),
            },
            Expr::Assignment(target, value) => {
                let boxed_target = match self.program.entity_map.get(&target) {
                    Some(Expr::Local(binding)) => Some(*binding),
                    _ => None,
                }
                .filter(|binding| self.boxed.contains(binding));
                if let Some(binding) = boxed_target {
                    let value_text = self.value_of(value, depth)?;
                    format!("{}.set({value_text})", self.binding_name(binding))
                } else {
                    let target_text = self.expression(target, depth)?;
                    let value_text = self.value_of(value, depth)?;
                    format!("{target_text} = {value_text}")
                }
            }
            Expr::Field(subject, _, index) => {
                let subject_text = self.expression(subject, depth)?;
                let field = self.field_name(subject, index, span)?;
                format!("{subject_text}.{field}")
            }
            Expr::Index(subject, index) => {
                let subject_text = self.expression(subject, depth)?;
                let index_text = self.expression(index, depth)?;
                format!("{subject_text}[({index_text}) as usize]")
            }
            Expr::List(elements) => {
                let mut parts = Vec::new();
                for element in &elements {
                    parts.push(self.value_of(*element, depth)?);
                }
                format!("vec![{}]", parts.join(", "))
            }
            Expr::Tuple(elements) => {
                let mut parts = Vec::new();
                for element in &elements {
                    parts.push(self.value_of(*element, depth)?);
                }
                format!("({},)", parts.join(", "))
            }
            Expr::TupleIndex(subject, offset, width) => {
                if width != 1 {
                    return Err(unsupported("a multi-slot tuple element", span));
                }
                format!("{}.{offset}", self.expression(subject, depth)?)
            }
            Expr::StructInitializer(named, fields) => {
                let pairs: Vec<(usize, Id)> = fields
                    .iter()
                    .map(|(index, value)| (*index, *value))
                    .collect();
                // The initializer's own id carries the STRUCT it builds; the id
                // in the node is the name it was written under, which the JS
                // emitter ignores because an array needs no declaration. Here it
                // is the declaration or nothing, so the type is the answer and
                // the written name is the fallback.
                let struct_id = self
                    .type_of(id)
                    .and_then(|type_id| self.resolve(type_id))
                    .and_then(|resolved| match resolved {
                        Type::Struct(struct_id, _) => Some(*struct_id),
                        _ => None,
                    })
                    .filter(|struct_id| self.program.structs.contains_key(struct_id))
                    .unwrap_or(named);
                self.struct_literal(struct_id, &pairs, depth, span)?
            }
            Expr::Reference(operand, mutable) => {
                let operand_text = self.expression(operand, depth)?;
                if mutable {
                    format!("&mut {operand_text}")
                } else {
                    format!("&{operand_text}")
                }
            }
            Expr::Dereference(operand) => format!("(*{})", self.expression(operand, depth)?),
            Expr::Call(call_id) => self.call(id, call_id, depth, span)?,
            Expr::Closure(closure_id) => self.closure(closure_id, depth, span)?,
            Expr::Is(subject, pattern) => self.is_test(subject, &pattern, depth, span)?,
            Expr::EnumVariant(enum_id, index) => self.variant_path(enum_id, index, span)?,
            Expr::Function(_)
            | Expr::Struct(_)
            | Expr::Enum(_)
            | Expr::Trait(_)
            | Expr::Impl(_)
            | Expr::Module(_)
            | Expr::ExternalFunction(_) => String::new(),
            Expr::Error => return Err(unsupported("an expression that did not analyze", span)),
            other => {
                return Err(unsupported(
                    &format!("the expression form `{}`", form_name(&other)),
                    span,
                ));
            }
        };
        Ok(rendered)
    }

    /// An expression in a VALUE position — rule 1's copy applied where the
    /// analyzer already decided one is owed. `clone_sites` is the JS emitter's
    /// own `__clone` decision, read here so the two backends copy in exactly
    /// the same places rather than in two opinions of the same places.
    fn value_of(&mut self, id: Id, depth: usize) -> Result<String, Error> {
        let text = self.expression(id, depth)?;
        if self.program.clone_sites.contains_key(&id) {
            return Ok(format!("({text}).clone()"));
        }
        // F16 / the probe's R-1, from the other side: a closure value is a
        // COUNTED handle, so handing one on is a retain. JavaScript hides this
        // because a function value is a reference and using it twice costs
        // nothing; natively the second use is a use-after-move unless the
        // handle is bumped. The analyzer's `clone_sites` does not cover it —
        // rule 1 is about aggregates, and a closure is not one.
        if self.reads_a_closure_binding(id) {
            return Ok(format!("({text}).clone()"));
        }
        // A LOANED parameter (`&T` / `&mut T` natively) read where a value is
        // wanted is rule 1's copy: `fun to_string(self): str { self }` hands
        // back a `str`, and `self` is a loan of one.
        if self.reads_a_loaned_parameter(id) {
            return Ok(format!("({text}).clone()"));
        }
        Ok(text)
    }

    /// Whether `id` reads a parameter this emitter receives by reference.
    fn reads_a_loaned_parameter(&self, id: Id) -> bool {
        let binding = match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) | Some(Expr::Parameter(binding)) => *binding,
            _ => return false,
        };
        self.program
            .parameters
            .get(&binding)
            .is_some_and(|parameter| self.receiving_form(parameter) != Receiving::ByValue)
    }

    /// Whether `id` READS a binding whose type is a closure — the shape that
    /// owes a refcount bump. A closure LITERAL is a fresh value and owes
    /// nothing.
    fn reads_a_closure_binding(&self, id: Id) -> bool {
        if !matches!(
            self.program.entity_map.get(&id),
            Some(Expr::Local(_)) | Some(Expr::Parameter(_))
        ) {
            return false;
        }
        self.type_of(id)
            .and_then(|type_id| self.resolve(type_id))
            .is_some_and(|resolved| matches!(resolved, Type::Closure(_, _, _)))
    }

    fn read_binding(&self, binding: Id) -> String {
        let name = self.binding_name(binding);
        if self.boxed.contains(&binding) {
            return format!("{name}.get()");
        }
        name
    }

    fn declaration(&mut self, binding: Id, depth: usize) -> Result<String, Error> {
        let variable = self
            .program
            .variables
            .get(&binding)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved binding", self.span_of(binding)))?;
        if self.program.lazy_cells.contains(&binding) {
            return Err(unsupported("a `lazy` binding", self.span_of(binding)));
        }
        let name = self.binding_name(binding);
        let mutable = if variable.mutable { "mut " } else { "" };
        // An annotation is WRITTEN ONLY where the initializer cannot type the
        // binding — an empty collection literal, and nothing else.
        //
        // The temptation is to annotate everything, and it is wrong: a binding's
        // `type_id` is its POINTEE's whenever the binding is a view (the type
        // system has no reference form, views being tracked beside it), so
        // `let v = &mut n` would be annotated `i32` while holding `&mut i32`,
        // and so would a binding of a `borrows` call's result. Rust's own
        // inference has the right answer at every one of those sites, and every
        // numeric literal this emitter writes carries its own suffix — so the
        // annotation buys nothing except the chance to be wrong.
        let initializer_needs_a_type = variable.initial.is_some_and(|initial| {
            matches!(self.program.entity_map.get(&initial), Some(Expr::List(items)) if items.is_empty())
        });
        let annotation = if initializer_needs_a_type {
            match self.rust_type(variable.type_id, self.span_of(binding)) {
                Ok(rendered) => format!(": {rendered}"),
                Err(error) => return Err(error),
            }
        } else {
            String::new()
        };
        match variable.initial {
            Some(initial) => {
                let value = self.value_of(initial, depth)?;
                if self.boxed.contains(&binding) {
                    // R3: a mutably-captured binding is a counted cell, so the
                    // declaration builds one and every read and write below goes
                    // through it.
                    return Ok(format!("let {name} = vilan_rt::Captured::new({value})"));
                }
                Ok(format!("let {mutable}{name}{annotation} = {value}"))
            }
            None => Err(unsupported(
                "a binding with no initializer",
                self.span_of(binding),
            )),
        }
    }

    fn number_literal(
        &mut self,
        id: Id,
        whole: &str,
        fraction: Option<&str>,
        suffix: Option<&str>,
        span: Span,
    ) -> Result<String, Error> {
        // The literal arrives in THREE pieces (`Expr::Number(whole, fraction,
        // suffix)`), not two. Reading the fraction as the suffix turned `3.5f`
        // into `3i32` — caught here by rustc, which is luck rather than a
        // design, so the pieces are named.
        let cleaned = match fraction {
            Some(fraction) => format!("{whole}.{fraction}").replace('_', ""),
            None => whole.replace('_', ""),
        };
        let rendered = match self
            .type_of(id)
            .and_then(|type_id| self.rust_type(type_id, span).ok())
        {
            Some(rust) => rust,
            None => match suffix {
                Some(suffix) => scalar_type(suffix).unwrap_or("i32").to_string(),
                None => {
                    if cleaned.contains('.') {
                        "f64".to_string()
                    } else {
                        "i32".to_string()
                    }
                }
            },
        };
        // A literal with a fraction is a FLOAT, whatever a stale or absent type
        // entry says: `3.5i32` is not a Rust literal at all.
        let rendered = if (fraction.is_some()
            || matches!(suffix, Some("f") | Some("f32") | Some("f64")))
            && !matches!(rendered.as_str(), "f32" | "f64")
        {
            "f64".to_string()
        } else {
            rendered
        };
        if rendered == "f64" || rendered == "f32" {
            let body = if cleaned.contains('.') || cleaned.contains('e') || cleaned.contains('E') {
                cleaned
            } else {
                format!("{cleaned}.0")
            };
            return Ok(format!("({body}{rendered})"));
        }
        if !is_integer_type(&rendered) {
            return Err(unsupported(
                &format!("a numeric literal typed `{rendered}`"),
                span,
            ));
        }
        Ok(format!("({cleaned}{rendered})"))
    }

    fn binary(
        &mut self,
        id: Id,
        op: BinaryOp,
        left: Id,
        right: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        if self.program.binary_op_dispatch.contains_key(&id) {
            return Err(unsupported("an overloaded operator", span));
        }
        // `str + str` is a concatenation, which is a runtime call natively
        // rather than an operator — and an INTERPOLATION is a chain of them
        // whose right halves are whatever was interpolated, rendered. The
        // rendering of a scalar is its `console.log` form on both backends,
        // which is what makes the substitution sound; anything else goes
        // through a user `render` this slice does not monomorphise, so it is
        // refused rather than guessed at.
        if matches!(op, BinaryOp::Add) && self.is_str_expr(left) {
            let left_text = self.expression(left, depth)?;
            let right_text = self.expression(right, depth)?;
            if self.is_str_expr(right) {
                return Ok(format!("vilan_rt::str_concat(&{left_text}, &{right_text})"));
            }
            if self.is_scalar_expr(right) {
                return Ok(format!(
                    "vilan_rt::str_concat(&{left_text}, &vilan_rt::js_of(&({right_text})))"
                ));
            }
            return Err(unsupported(
                "an interpolation of a value with its own `render`",
                span,
            ));
        }
        let symbol = match op {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::Shl => "<<",
            BinaryOp::Shr => ">>",
            BinaryOp::BitAnd => "&",
            BinaryOp::BitXor => "^",
            BinaryOp::BitOr => "|",
            BinaryOp::Eq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::LtEq => "<=",
            BinaryOp::GtEq => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
            BinaryOp::UShr => return Err(unsupported("the `>>>` operator", span)),
        };
        let left_text = self.expression(left, depth)?;
        let right_text = self.expression(right, depth)?;
        Ok(format!("({left_text} {symbol} {right_text})"))
    }

    /// Whether an expression is a `str` — by its resolved type where it has
    /// one, and structurally where it does not (a literal, or a concatenation
    /// whose left half is one). The concatenation chain an interpolation
    /// desugars to carries a type on none of its joints.
    fn is_str_expr(&self, id: Id) -> bool {
        if self.is_str(id) {
            return true;
        }
        match self.program.entity_map.get(&id) {
            Some(Expr::String(_)) | Some(Expr::MultilineString(_)) => true,
            Some(Expr::Binary(BinaryOp::Add, left, _)) => self.is_str_expr(*left),
            _ => false,
        }
    }

    /// Whether an expression's type is a scalar primitive — the set whose
    /// `render` and whose `console.log` rendering are the same string.
    fn is_scalar_expr(&self, id: Id) -> bool {
        self.type_of(id)
            .and_then(|type_id| self.resolve(type_id))
            .and_then(|resolved| match resolved {
                Type::Struct(struct_id, _) => self.program.structs.get(struct_id),
                _ => None,
            })
            .is_some_and(|declaration| {
                scalar_type(declaration.name).is_some() && declaration.name != "str"
            })
            || self
                .type_of(id)
                .and_then(|type_id| self.resolve(type_id))
                .is_some_and(|resolved| {
                    matches!(resolved, Type::Enum(enum_id, _)
                    if self.program.bool_enum_id == Some(*enum_id))
                })
    }

    fn is_str(&self, id: Id) -> bool {
        self.type_of(id)
            .and_then(|type_id| self.resolve(type_id))
            .and_then(|resolved| match resolved {
                Type::Struct(struct_id, _) => self.program.structs.get(struct_id),
                _ => None,
            })
            .is_some_and(|declaration| declaration.name == "str")
    }

    fn if_branch(&mut self, branch: &ExprIfBranch, depth: usize) -> Result<String, Error> {
        let pad = Self::indent(depth);
        match branch {
            ExprIfBranch::If(condition, (statements, tail), next) => {
                let condition_text = self.expression(*condition, depth)?;
                let mut body = String::new();
                self.emit_block(statements, *tail, &mut body, depth + 1)?;
                let mut out = format!("if {condition_text} {{\n{body}{pad}}}");
                if let Some(next) = next {
                    let _ = write!(out, " else {}", self.if_branch(next, depth)?);
                }
                Ok(out)
            }
            ExprIfBranch::Else((statements, tail)) => {
                let mut body = String::new();
                self.emit_block(statements, *tail, &mut body, depth + 1)?;
                Ok(format!("{{\n{body}{pad}}}"))
            }
        }
    }

    fn match_expr(
        &mut self,
        subject: Id,
        legs: &[ExprMatchLeg],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let subject_text = self.expression(subject, depth)?;
        let pad = Self::indent(depth);
        let leg_pad = Self::indent(depth + 1);
        let mut out = format!("match {subject_text} {{\n");
        let mut has_catch_all = false;
        for leg in legs {
            if leg.guard.is_some() {
                return Err(unsupported("a guarded `match` leg", span));
            }
            let pattern = self.pattern(&leg.pattern, span)?;
            if matches!(leg.pattern, ExprPattern::Wildcard | ExprPattern::Binding(_))
                || self.pattern_is_bool(&leg.pattern)
            {
                // A `bool` match with both legs is exhaustive natively too, and
                // an extra arm after it is an `unreachable_patterns` lint rather
                // than a safety net.
                has_catch_all = true;
            }
            let body = self.expression(leg.body, depth + 1)?;
            let _ = writeln!(out, "{leg_pad}{pattern} => {body},");
        }
        // vilan's exhaustiveness is checked by vilan; rustc re-checks it over a
        // shape it cannot always see (a literal match on an integer, say), so a
        // match with no catch-all gets one that cannot be reached.
        if !has_catch_all {
            let _ = writeln!(
                out,
                "{leg_pad}_ => vilan_rt::panic_with(\"unreachable match leg\"),"
            );
        }
        let _ = write!(out, "{pad}}}");
        Ok(out)
    }

    fn pattern_is_bool(&self, pattern: &ExprPattern) -> bool {
        matches!(pattern, ExprPattern::Variant(enum_id, _, _)
            if self.program.bool_enum_id == Some(*enum_id))
    }

    fn pattern(&mut self, pattern: &ExprPattern, span: Span) -> Result<String, Error> {
        match pattern {
            ExprPattern::Wildcard => Ok("_".to_string()),
            ExprPattern::Binding(id) => Ok(self.binding_name(*id)),
            ExprPattern::Literal(id) => self.expression(*id, 0),
            ExprPattern::Variant(enum_id, index, payload) => {
                if let Some(declaration) = self.program.enums.get(enum_id)
                    && declaration.name == "bool"
                {
                    // `bool` is an enum in the source and a native boolean at
                    // runtime on both backends.
                    return Ok(if *index == 1 { "true" } else { "false" }.to_string());
                }
                let path = self.variant_path(*enum_id, *index, span)?;
                if payload.is_empty() {
                    return Ok(path);
                }
                let mut parts = Vec::new();
                for sub in payload {
                    parts.push(self.pattern(sub, span)?);
                }
                Ok(format!("{path}({})", parts.join(", ")))
            }
            ExprPattern::Tuple(elements) => {
                let mut parts = Vec::new();
                for (element, _) in elements {
                    parts.push(self.pattern(element, span)?);
                }
                Ok(format!("({},)", parts.join(", ")))
            }
            ExprPattern::Array(_) => Err(unsupported("an array pattern", span)),
        }
    }

    fn is_test(
        &mut self,
        subject: Id,
        pattern: &ExprPattern,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let subject_text = self.expression(subject, depth)?;
        let pattern_text = self.pattern(pattern, span)?;
        Ok(format!("matches!({subject_text}, {pattern_text})"))
    }

    fn variant_path(&mut self, enum_id: Id, index: usize, span: Span) -> Result<String, Error> {
        let declaration = self
            .program
            .enums
            .get(&enum_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved enum", span))?;
        let variant = declaration
            .variants
            .get(index)
            .ok_or_else(|| unsupported("an unresolved enum variant", span))?;
        match declaration.name {
            "bool" => Ok(if index == 1 { "true" } else { "false" }.to_string()),
            "Option" | "Result" => Ok(sanitize(variant.name)),
            _ => {
                self.ensure_enum(enum_id, span)?;
                Ok(format!(
                    "{}_{}::{}",
                    sanitize(declaration.name),
                    enum_id.0,
                    sanitize(variant.name)
                ))
            }
        }
    }

    fn field_name(&self, subject: Id, index: usize, span: Span) -> Result<String, Error> {
        let struct_id = self
            .type_of(subject)
            .and_then(|type_id| self.resolve(type_id))
            .and_then(|resolved| match resolved {
                Type::Struct(id, _) => Some(*id),
                _ => None,
            })
            .ok_or_else(|| unsupported("a field read of an unresolved subject", span))?;
        let declaration = self
            .program
            .structs
            .get(&struct_id)
            .ok_or_else(|| unsupported("a field read of an unresolved struct", span))?;
        declaration
            .fields
            .get(index)
            .map(|field| sanitize(field.name))
            .ok_or_else(|| unsupported("a field read past the struct's fields", span))
    }

    fn struct_literal(
        &mut self,
        struct_id: Id,
        fields: &[(usize, Id)],
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let declaration = self
            .program
            .structs
            .get(&struct_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved struct literal", span))?;
        if !declaration.generic_parameter_constraint_ids.is_empty() {
            return Err(unsupported(
                &format!("the generic struct `{}`", declaration.name),
                span,
            ));
        }
        self.ensure_struct(struct_id, span)?;
        let mut parts = Vec::new();
        for (index, value) in fields.iter() {
            let name = declaration
                .fields
                .get(*index)
                .map(|field| sanitize(field.name))
                .ok_or_else(|| unsupported("a struct field past the declaration", span))?;
            parts.push(format!("{name}: {}", self.value_of(*value, depth)?));
        }
        Ok(format!(
            "{}_{} {{ {} }}",
            sanitize(declaration.name),
            struct_id.0,
            parts.join(", ")
        ))
    }

    fn for_each(
        &mut self,
        _id: Id,
        iterable: Id,
        item: Option<Id>,
        statements: &[Id],
        tail: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        // Only a `List` and a fixed array iterate natively in S1a. Anything
        // else is an `Iterator` impl, which is a monomorphised generic — S1b's.
        // The iterable may be written `&mut xs`, whose own id carries no type —
        // ask about the place it views.
        let iterable_place = match self.program.entity_map.get(&iterable) {
            Some(Expr::Reference(operand, _)) => *operand,
            _ => iterable,
        };
        let iterates_something_else = self
            .type_of(iterable_place)
            .and_then(|type_id| self.resolve(type_id))
            .is_some_and(|resolved| match resolved {
                Type::Array(_, _) => false,
                Type::Struct(struct_id, _) => self
                    .program
                    .structs
                    .get(struct_id)
                    .is_none_or(|declaration| declaration.name != "List"),
                // An unresolved iterable is not a refusal: a list LITERAL
                // carries no type on its own id, and refusing on silence would
                // refuse the commonest loop in the corpus.
                _ => false,
            });
        if iterates_something_else {
            return Err(unsupported(
                concat!(
                    "a `for` over anything but a `List` ",
                    "(an `Iterator` impl is a monomorphised generic)"
                ),
                span,
            ));
        }
        let iterable_text = self.expression(iterable, depth)?;
        let binder = match item {
            Some(item) => self.binding_name(item),
            None => "_".to_string(),
        };
        let mut body = String::new();
        self.emit_block(statements, tail, &mut body, depth + 1)?;
        let pad = Self::indent(depth);
        // A `for` over a place the loop does not own only READS it (spec §6.1's
        // "a temporary that only reads"), so it iterates a borrow. A `&mut`
        // iteration is recorded on `for_each_views`.
        // A `for` over a place the loop does not own binds VALUES (rule 1: the
        // element is a copy), which is `.clone().into_iter()` natively — the
        // borrow `&container` would bind `&T` and every use of the binder would
        // then be a reference where the program wrote a value. A `&mut`
        // iteration is the one that really binds views, and it is recorded on
        // `for_each_views`.
        let iteration = match self.program.for_each_views.get(&item.unwrap_or(Id(0))) {
            Some(true) => format!("({iterable_text}).iter_mut()"),
            Some(false) => format!("({iterable_text}).iter()"),
            None => format!("({iterable_text}).clone().into_iter()"),
        };
        Ok(format!("for {binder} in {iteration} {{\n{body}{pad}}}"))
    }

    fn closure(&mut self, closure_id: Id, depth: usize, span: Span) -> Result<String, Error> {
        let closure = self
            .program
            .closures
            .get(&closure_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved closure", span))?;
        let mut parameters = Vec::new();
        for parameter_id in &closure.parameters {
            let parameter = self
                .program
                .parameters
                .get(parameter_id)
                .cloned()
                .ok_or_else(|| unsupported("an unresolved closure parameter", span))?;
            let rendered = self.rust_type(parameter.type_id, span)?;
            let declaration = match self.receiving_form(&parameter) {
                Receiving::ByValue => rendered,
                Receiving::Ref => format!("&{rendered}"),
                Receiving::RefMut => format!("&mut {rendered}"),
            };
            parameters.push(format!(
                "{}: {declaration}",
                self.binding_name(*parameter_id)
            ));
        }
        let body = self.expression(closure.return_, depth)?;
        // A `move` closure takes its captures by value, so a captured CELL has
        // to be a handle of its own — otherwise the binding outside is moved
        // into the closure and every later read of it is a use-after-move.
        let mut declared_inside = HashSet::new();
        let mut referenced = HashSet::new();
        let mut visited = HashSet::new();
        self.scan_closure(
            closure.return_,
            &mut declared_inside,
            &mut referenced,
            &mut visited,
        );
        let mut captures: Vec<Id> = referenced
            .into_iter()
            .filter(|binding| self.boxed.contains(binding) && !declared_inside.contains(binding))
            .collect();
        captures.sort_by_key(|binding| binding.0);
        let prelude: String = captures
            .iter()
            .map(|binding| {
                let name = self.binding_name(*binding);
                format!("let {name} = {name}.clone(); ")
            })
            .collect();
        // F16: a closure VALUE is counted, because the emitter cannot see from
        // here whether the position it lands in stores it. `Rc::new` is the
        // shape the probe's R-1 finding forced.
        Ok(format!(
            "{{ {prelude}std::rc::Rc::new(move |{}| {{ {body} }}) }}",
            parameters.join(", ")
        ))
    }

    // ----------------------------------------------------------- the call --

    fn call(
        &mut self,
        call_expr_id: Id,
        call_id: Id,
        depth: usize,
        span: Span,
    ) -> Result<String, Error> {
        let function_call = self
            .program
            .function_calls
            .get(&call_id)
            .cloned()
            .ok_or_else(|| unsupported("an unresolved call", span))?;
        if self.program.awaited_calls.contains(&call_expr_id)
            || self.program.awaited_calls.contains(&call_id)
        {
            return Err(unsupported("an `await`", span));
        }
        let Some(Expr::Local(target)) = self.program.entity_map.get(&function_call.subject_id)
        else {
            // A value call — `(h.f)()`. The subject is a counted closure.
            let subject = self.expression(function_call.subject_id, depth)?;
            let mut arguments = Vec::new();
            for argument in &function_call.argument_ids {
                arguments.push(self.value_of(*argument, depth)?);
            }
            return Ok(format!("({subject})({})", arguments.join(", ")));
        };
        let target = *target;

        if !function_call.generic_argument_ids.is_empty() {
            return Err(unsupported("a call with explicit generic arguments", span));
        }
        if self.program.generic_dispatch.contains_key(&call_id)
            || self
                .program
                .generic_dispatch
                .contains_key(&function_call.subject_id)
        {
            return Err(unsupported(
                "a call dispatched through a generic parameter (monomorphisation is S1b's)",
                span,
            ));
        }

        // A variant constructor builds the value directly.
        if let Some(Expr::EnumVariant(enum_id, index)) = self.program.entity_map.get(&target) {
            let (enum_id, index) = (*enum_id, *index);
            let path = self.variant_path(enum_id, index, span)?;
            if function_call.argument_ids.is_empty() {
                return Ok(path);
            }
            let mut arguments = Vec::new();
            for argument in &function_call.argument_ids {
                arguments.push(self.value_of(*argument, depth)?);
            }
            return Ok(format!("{path}({})", arguments.join(", ")));
        }

        let mut arguments = Vec::new();
        for argument in &function_call.argument_ids {
            arguments.push(self.value_of(*argument, depth)?);
        }

        if Some(target) == self.program.print_fn_id {
            let value = arguments
                .first()
                .cloned()
                .unwrap_or_else(|| "()".to_string());
            return Ok(format!("vilan_rt::print(&({value}))"));
        }
        if Some(target) == self.program.panic_fn_id {
            let value = arguments
                .first()
                .cloned()
                .unwrap_or_else(|| "()".to_string());
            return Ok(format!("vilan_rt::panic_with(&({value}))"));
        }
        if Some(target) == self.program.list_new_fn_id {
            return Ok("Vec::new()".to_string());
        }
        if Some(target) == self.program.list_push_fn_id {
            let mut parts = arguments.into_iter();
            let receiver = parts.next().unwrap_or_else(|| "()".to_string());
            let item = parts.next().unwrap_or_else(|| "()".to_string());
            return Ok(format!("{receiver}.push({item})"));
        }
        if let Some(intrinsic) = self.program.intrinsics.get(&target).copied() {
            return self.intrinsic(intrinsic, arguments, span);
        }
        if let Some(external) = self.program.external_functions.get(&target) {
            let name = external.name;
            let binding = external.extern_binding.clone();
            return Err(unsupported(
                &format!(
                    "the host binding `{name}`{}",
                    match binding {
                        Some(ExternBinding::Function { symbol, .. }) => format!(" (`{symbol}`)"),
                        _ => String::new(),
                    }
                ),
                span,
            ));
        }

        // A named BINDING that holds a closure — `g()` where `g` is a
        // parameter or a local. The subject is a place, not a definition, so it
        // is called as a value; an `Rc<dyn Fn>` derefs to the call.
        if self.program.parameters.contains_key(&target)
            || self.program.variables.contains_key(&target)
        {
            return Ok(format!(
                "({})({})",
                self.read_binding(target),
                arguments.join(", ")
            ));
        }

        // An ordinary call. The receiver of a loaned `self` arrives as a place
        // in the IR and as a reference natively, so the `&`/`&mut` the source
        // never wrote is synthesized here, from the callee's own conventions.
        self.ensure_function(target)?;
        let conventions: Vec<Receiving> = self
            .program
            .functions
            .get(&target)
            .map(|function| function.parameters.clone())
            .unwrap_or_default()
            .iter()
            .filter_map(|parameter_id| self.program.parameters.get(parameter_id).cloned())
            .map(|parameter| self.receiving_form(&parameter))
            .collect();
        let adjusted: Vec<String> = arguments
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                let already_a_reference = function_call
                    .argument_ids
                    .get(index)
                    .and_then(|argument| self.program.entity_map.get(argument))
                    .is_some_and(|expr| matches!(expr, Expr::Reference(_, _)));
                if already_a_reference {
                    return text;
                }
                match conventions.get(index) {
                    Some(Receiving::Ref) => format!("&{text}"),
                    Some(Receiving::RefMut) => format!("&mut {text}"),
                    _ => text,
                }
            })
            .collect();
        Ok(format!(
            "{}({})",
            self.function_name(target),
            adjusted.join(", ")
        ))
    }

    fn intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        arguments: Vec<String>,
        span: Span,
    ) -> Result<String, Error> {
        let mut parts = arguments.into_iter();
        let mut next = || parts.next().unwrap_or_else(|| "()".to_string());
        let rendered = match intrinsic {
            Intrinsic::StrLen => format!("vilan_rt::str_len(&{})", next()),
            Intrinsic::StrTrim => format!("vilan_rt::str_trim(&{})", next()),
            Intrinsic::StrToLowercase => format!("vilan_rt::str_to_lowercase(&{})", next()),
            Intrinsic::StrToUppercase => format!("vilan_rt::str_to_uppercase(&{})", next()),
            Intrinsic::StrContains => {
                format!("vilan_rt::str_contains(&{}, &{})", next(), next())
            }
            Intrinsic::StrStartsWith => {
                format!("vilan_rt::str_starts_with(&{}, &{})", next(), next())
            }
            Intrinsic::StrEndsWith => {
                format!("vilan_rt::str_ends_with(&{}, &{})", next(), next())
            }
            Intrinsic::StrReplace => {
                format!(
                    "vilan_rt::str_replace(&{}, &{}, &{})",
                    next(),
                    next(),
                    next()
                )
            }
            Intrinsic::StrRepeat => format!("vilan_rt::str_repeat(&{}, {})", next(), next()),
            Intrinsic::StrSplit => format!("vilan_rt::str_split(&{}, &{})", next(), next()),
            Intrinsic::StrSubstring => {
                format!(
                    "vilan_rt::str_substring(&{}, {}, {})",
                    next(),
                    next(),
                    next()
                )
            }
            Intrinsic::ParseI32 => format!("vilan_rt::parse_i32(&{})", next()),
            Intrinsic::ParseF64 => format!("vilan_rt::parse_f64(&{})", next()),
            Intrinsic::ListLen => format!("({}.len() as i32)", next()),
            Intrinsic::ListGet => format!("vilan_rt::list_get(&{}, ({}) as i64)", next(), next()),
            Intrinsic::ListPop => format!("vilan_rt::list_pop(&mut {})", next()),
            Intrinsic::ListRemove => {
                format!(
                    "vilan_rt::list_remove(&mut {}, ({}) as i64)",
                    next(),
                    next()
                )
            }
            Intrinsic::ListInsert => format!(
                "vilan_rt::list_insert(&mut {}, ({}) as i64, {})",
                next(),
                next(),
                next()
            ),
            Intrinsic::SharedNew => format!("vilan_rt::Shared::new({})", next()),
            Intrinsic::SharedClone => format!("({}).clone()", next()),
            Intrinsic::SharedValue => format!("({}).get()", next()),
            Intrinsic::SetNew => "vilan_rt::Set::new()".to_string(),
            Intrinsic::SetInsert => format!("{}.insert({})", next(), next()),
            Intrinsic::SetContains => format!("{}.contains(&{})", next(), next()),
            Intrinsic::SetRemove => format!("{}.remove(&{})", next(), next()),
            Intrinsic::SetLen => format!("({}.len() as i32)", next()),
            Intrinsic::MapNew => "vilan_rt::Map::new()".to_string(),
            Intrinsic::MapInsert => format!("{}.insert({}, {})", next(), next(), next()),
            Intrinsic::MapGet => format!("{}.get(&{}).cloned()", next(), next()),
            Intrinsic::MapContainsKey => format!("{}.contains_key(&{})", next(), next()),
            Intrinsic::MapRemove => format!("{}.remove(&{})", next(), next()),
            Intrinsic::MapLen => format!("({}.len() as i32)", next()),
            Intrinsic::MapKeys => format!("{}.keys()", next()),
            Intrinsic::MapValues => format!("{}.values()", next()),
            other => {
                return Err(unsupported(&format!("the intrinsic `{other:?}`"), span));
            }
        };
        Ok(rendered)
    }
}

/// How a parameter is received natively.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Receiving {
    ByValue,
    Ref,
    RefMut,
}

/// The native width of a vilan scalar primitive.
///
/// R6: `i53`/`u53` are distinct types with native widths `i64`/`u64` and the
/// documented note that their RANGE guarantee is the JS one — a program that
/// round-trips both backends behaves the same, and a native-only value outside
/// 2^53 is outside what the language promises either way.
fn scalar_type(name: &str) -> Option<&'static str> {
    Some(match name {
        "i8" => "i8",
        "u8" => "u8",
        "i16" => "i16",
        "u16" => "u16",
        "i32" => "i32",
        "u32" => "u32",
        "i53" => "i64",
        "u53" => "u64",
        "f32" => "f32",
        "f64" => "f64",
        "str" => "vilan_rt::Str",
        _ => return None,
    })
}

fn is_integer_type(rendered: &str) -> bool {
    matches!(
        rendered,
        "i8" | "u8" | "i16" | "u16" | "i32" | "u32" | "i64" | "u64"
    )
}

/// A vilan identifier as a Rust one. Vilan's identifier grammar is a subset of
/// Rust's already, so this only has to keep a vilan name that happens to be a
/// Rust keyword from becoming one.
fn sanitize(name: &str) -> String {
    const RUST_KEYWORDS: &[&str] = &[
        "as", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
        "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
        "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
        "unsafe", "use", "where", "while", "async", "await", "box", "final", "macro", "override",
        "priv", "try", "typeof", "unsized", "virtual", "yield", "abstract", "become", "do",
    ];
    if RUST_KEYWORDS.contains(&name) {
        return format!("r#{name}");
    }
    name.replace(|c: char| !c.is_alphanumeric() && c != '_', "_")
}

fn rust_string(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn describe(resolved: &Type) -> String {
    match resolved {
        Type::Any => "any".to_string(),
        Type::Never => "never".to_string(),
        Type::Mapped(_, _, _) => "a mapped tuple".to_string(),
        Type::Trait(_, _) => "a trait object".to_string(),
        Type::Function(_) => "a function value".to_string(),
        Type::Module(_) => "a module".to_string(),
        Type::Unknown | Type::Unresolved => "an unresolved type".to_string(),
        _ => "an unsupported type".to_string(),
    }
}

fn collect_if_children(branch: &ExprIfBranch, children: &mut Vec<Id>) {
    match branch {
        ExprIfBranch::If(condition, (statements, tail), next) => {
            children.push(*condition);
            children.extend(statements.iter().copied());
            children.push(*tail);
            if let Some(next) = next {
                collect_if_children(next, children);
            }
        }
        ExprIfBranch::Else((statements, tail)) => {
            children.extend(statements.iter().copied());
            children.push(*tail);
        }
    }
}

fn form_name(expr: &Expr<'_>) -> &'static str {
    match expr {
        Expr::Async(_) => "an `async` block",
        Expr::Await(_) => "an `await`",
        Expr::TryAssert(_) => "a `!` assertion",
        Expr::Lift(_, _, _) | Expr::LiftBinder | Expr::LiftRegion(_, _) => "a `?` lift",
        Expr::Destructure(_, _) => "a destructuring binding",
        Expr::TupleComprehension(_, _, _) => "a tuple comprehension",
        Expr::Repeat(_, _) => "a `[value; n]` literal",
        Expr::ArrayLen(_, _) => "a fixed-array `len()`",
        Expr::Generic(_) => "a generic type reference",
        Expr::Macro => "a macro name",
        _ => "an unsupported form",
    }
}

/// The `Cargo.toml` of the project the backend writes, pointing at the runtime
/// crate by path. `edition 2024` matches the workspace's own.
pub fn cargo_manifest(name: &str, runtime_path: &str) -> String {
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.0.0\"\n\
         edition = \"2024\"\n\
         \n\
         [[bin]]\n\
         name = \"{name}\"\n\
         path = \"src/main.rs\"\n\
         \n\
         [dependencies]\n\
         vilan-rt = {{ path = {runtime_path:?} }}\n\
         \n\
         [profile.release]\n\
         panic = \"unwind\"\n\
         \n\
         [workspace]\n"
    )
}
