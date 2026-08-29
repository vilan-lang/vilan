use crate::analyzer::{
    BackingValue, CopyDecision, DropExtent, Expr, ExprIfBranch, ExprPattern, Function,
    GenericDispatch, Intrinsic, LiftDispatch, Program, TransferForm, TryDispatch,
};
use crate::call_graph::{CallTarget, IndirectReason};
use crate::error::Error;
use crate::fx::{FxHashMap as HashMap, FxHashSet as HashSet};
use crate::id::Id;
use crate::interpreter::ConstValue;
use crate::node::{BinaryOp, Convention, ExternBinding};
use crate::options::BuildOptions;
use crate::span::Span;
use crate::type_::{SCALAR_PRIMITIVE_NAMES, Type, TypeId};
use indexmap::IndexMap;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

pub fn transform<'src>(program: &Program<'src>, options: &BuildOptions) -> Result<String, Error> {
    Transformer::new(program, options).transform_entry()
}

/// The transformed program one step before formatting: the whole JS AST plus
/// the text prelude it needs. `transform` formats this into the final source;
/// the macro engine's interpreter (`interpreter.rs`, macro-engine.md §5)
/// evaluates `nodes` directly — the two consumers share every lowering
/// decision down to this tree.
pub struct JsProgram<'src> {
    /// Host import lines (`import { a } from "m";`) from `[extern]` bindings.
    /// Non-empty means the program reaches host capabilities — the interpreter
    /// rejects it ("not available at expansion time").
    pub imports: Vec<String>,
    /// The names of the `__` runtime helpers the program uses (`__clone`, …),
    /// in emission order. The formatter prepends their JS sources; the
    /// interpreter implements them natively by name.
    pub helpers: Vec<&'static str>,
    pub nodes: Vec<js::Node<'src>>,
}

pub fn transform_to_ast<'src>(
    program: &'src Program<'src>,
    options: &BuildOptions,
) -> Result<JsProgram<'src>, Error> {
    Transformer::new(program, options).transform_entry_ast()
}

/// Transforms a program rooted at the given FUNCTIONS instead of `main` — the
/// macro world's shape (macro-engine.md §3): macro funs are entry points the
/// expansion interpreter calls directly, so emission is seeded from them (plus
/// module-level globals) and no `main` is required. Returns the program AST and
/// each root's emitted function name. Skips the cosmetic scope-renaming pass —
/// the interpreter doesn't read the output, and stable names keep the root map
/// trivially correct.
pub fn transform_functions<'src>(
    program: &'src Program<'src>,
    options: &BuildOptions,
    roots: &[Id],
) -> Result<(JsProgram<'src>, HashMap<Id, String>), Error> {
    let mut transformer = Transformer::new(program, options);

    // The macro world emits the same `const` declarations as a normal build, so
    // it needs the same initialization order (`b33-emission-order.md` §4).
    let global_variables = crate::init_order::initialization_order(program, program.call_graph());
    let t_global_variables = transformer.walk_list(&global_variables);

    let mut names = HashMap::default();
    for root in roots {
        transformer.ensure_function_emitted(*root);
        names.insert(*root, transformer.ng.name_for(*root));
    }

    let mut t_functions = transformer
        .required_functions
        .into_iter()
        .collect::<Vec<_>>();
    t_functions.sort_by(|a, b| (a.0.0).cmp(&b.0.0));
    let t_functions = t_functions.into_iter().map(|x| x.1);
    let t_instances = transformer.monomorphized.into_iter();

    let imports = transformer
        .used_imports
        .iter()
        .map(|(module, symbols)| {
            let names = symbols.iter().cloned().collect::<Vec<_>>().join(", ");
            format!("import {{ {} }} from \"{}\";", names, module)
        })
        .collect::<Vec<_>>();
    let helpers = transformer.used_helpers.into_iter().collect::<Vec<_>>();

    let nodes = t_functions
        .chain(t_instances)
        .chain(t_global_variables)
        .collect::<Vec<_>>();
    Ok((
        JsProgram {
            imports,
            helpers,
            nodes,
        },
        names,
    ))
}

/// One emitted route chunk: the file a first navigation to `arm` fetches.
pub struct EmittedChunk {
    /// The arm pattern this chunk serves, as `--print-chunks` names it.
    pub arm: String,
    /// The route value's variant tag — the key the embedded map is read by.
    pub tag: usize,
    /// The artifact's file name, beside the entry bundle.
    pub file: String,
    pub source: String,
}

/// A split entry's artifacts (`bundle-splitting.md` §3): the eager bundle plus
/// one file per route chunk. `chunks` is empty when the entry has nothing to
/// split, and `main` is then byte-identical to [`transform`]'s output.
pub struct SplitProgram {
    pub main: String,
    pub chunks: Vec<EmittedChunk>,
    /// The same entry emitted as ONE file — the denominator of the split's
    /// verdict (`bundle-splitting.md` §S3, item 5). Measured rather than
    /// estimated: the fixed cost of the gate is not a constant the emitter can
    /// be trusted to remember, so a split build emits both ways and compares.
    pub whole_bytes: usize,
}

impl SplitProgram {
    /// What this split cost, in emitted bytes.
    pub fn cost(&self) -> crate::chunks::SplitCost {
        crate::chunks::SplitCost {
            eager: self.main.len(),
            deferred: self.chunks.iter().map(|chunk| chunk.source.len()).sum(),
            whole: self.whole_bytes,
        }
    }
}

/// The registry the eager bundle and its chunks meet at. Not an ESM export:
/// the emitted entry exports nothing and renames whole-program, and a relative
/// `import()` cannot resolve in the playground's opaque-origin srcdoc frame —
/// so the boundary is a runtime object keyed by the (already allocated)
/// emitted names. `bundle-splitting.md` §3.
const CHUNK_REGISTRY: &str = "__vilan_chunks";

/// Emits `program` as an eager bundle plus one file per route chunk
/// (`bundle-splitting.md` S2), for a browser entry that declared
/// `split = true`. `leg` names the entry, and so its artifacts.
///
/// The walk, the name generator and the scope rename are the single-file
/// build's, unchanged — the split is a partition of the assembled nodes, taken
/// after the rename. That is what lets a chunk's declarations read the eager
/// scope by name through the registry, and what keeps the eager bundle's
/// module-binding block identical to the one a single-file build emits.
pub fn transform_split<'src>(
    program: &'src Program<'src>,
    options: &BuildOptions,
    leg: &str,
) -> Result<SplitProgram, Error> {
    let plan = crate::chunks::plan(program);
    if plan.chunks.is_empty() {
        // Nothing splittable: the entry is a single file, exactly as if the
        // flag were absent. Reported by `--print-chunks`, not by a failure.
        let main = transform(program, options)?;
        return Ok(SplitProgram {
            whole_bytes: main.len(),
            main,
            chunks: Vec::new(),
        });
    }
    // The denominator of the verdict below: the same program as one file. A
    // second walk over an already-analyzed program, paid only by a leg that
    // asked to split, and it buys an EXACT answer where a compiled-in threshold
    // would only ever be a remembered measurement.
    let whole_bytes = transform(program, options)?.len();

    let mut transformer = Transformer::new(program, options);
    transformer.chunk_members = plan.members();
    transformer.chunk_count = plan.chunks.len();
    transformer.chunk_gate = plan.gate.as_ref().map(|gate| ChunkGate {
        swap: gate.swap,
        swap_split: gate.swap_split,
        preload: gate.preload,
        calls: gate.calls.iter().copied().collect::<HashSet<Id>>(),
    });
    transformer.used_helpers.insert("__chunk_registry");
    let formatter = transformer.formatter.clone();
    let line_break = formatter.line_break;
    let mut assembled = transformer.assemble()?;

    // What each side can see of the other. The eager bundle's top level is the
    // one scope a chunk can read from; a chunk's top level is the one a
    // forwarder stands in for.
    let mut eager_names = top_level_names(&assembled.nodes);
    eager_names.extend(assembled.helpers.iter().map(|name| name.to_string()));
    eager_names.extend(imported_symbols(&assembled.imports));
    let chunk_names: Vec<BTreeSet<String>> = assembled
        .chunks
        .iter()
        .map(|nodes| top_level_names(nodes))
        .collect();

    // The eager bundle keeps a forwarder for every chunk function it names, so
    // every call site — the route match's arms — is emitted unchanged.
    let mut eager_references = BTreeSet::new();
    collect_references(&assembled.nodes, &mut eager_references);
    let mut forwarders: Vec<js::Node<'src>> = Vec::new();
    let mut registered_for_chunks: BTreeSet<String> = BTreeSet::new();
    for nodes in &assembled.chunks {
        for node in nodes {
            let js::Node::Function(function) = node else {
                continue;
            };
            if !eager_references.contains(&function.name) {
                continue;
            }
            forwarders.push(chunk_forwarder(function));
        }
    }
    // …and registers everything a chunk reads back out of it.
    let mut chunk_sources: Vec<String> = Vec::new();
    for (index, nodes) in assembled.chunks.iter().enumerate() {
        let mut references = BTreeSet::new();
        collect_references(nodes, &mut references);
        let needs: Vec<String> = references
            .into_iter()
            .filter(|name| eager_names.contains(name) && !chunk_names[index].contains(name))
            .collect();
        registered_for_chunks.extend(needs.iter().cloned());

        let mut chunk_nodes: Vec<js::Node<'src>> = vec![js::Node::ConstVariable(js::Variable {
            name: CHUNK_REGISTRY.to_string(),
            value: Box::new(js::Node::Property(
                Box::new(js::Node::Local("globalThis".to_string())),
                CHUNK_REGISTRY.to_string(),
            )),
        })];
        for name in &needs {
            chunk_nodes.push(js::Node::ConstVariable(js::Variable {
                name: name.clone(),
                value: Box::new(registry_slot(name)),
            }));
        }
        chunk_nodes.extend(nodes.iter().cloned());
        for name in &chunk_names[index] {
            chunk_nodes.push(js::Node::Assignment(
                Box::new(registry_slot(name)),
                Box::new(js::Node::Local(name.clone())),
            ));
        }
        chunk_sources.push(format!("{}{}", formatter.file(&chunk_nodes), line_break));
    }

    // The eager bundle's own glue: the registry handle first (nothing runs
    // before it), then — at the seam between the module bindings and `main` —
    // the embedded chunk map and the registrations. Every module binding has
    // initialized by then, and nothing can have navigated yet.
    let mut glue: Vec<js::Node<'src>> = Vec::new();
    for chunk in &plan.chunks {
        glue.push(js::Node::Assignment(
            Box::new(js::Node::PropertyIndex(
                Box::new(js::Node::Property(
                    Box::new(js::Node::Local(CHUNK_REGISTRY.to_string())),
                    "url".to_string(),
                )),
                Box::new(js::Node::Number(chunk.tag.to_string(), None)),
            )),
            Box::new(js::Node::String(Cow::Owned(
                crate::chunks::chunk_file_name(leg, &chunk.arm),
            ))),
        ));
    }
    for name in &registered_for_chunks {
        glue.push(js::Node::Assignment(
            Box::new(registry_slot(name)),
            Box::new(js::Node::Local(name.clone())),
        ));
    }
    assembled
        .nodes
        .splice(assembled.main_body_start..assembled.main_body_start, glue);
    assembled.nodes.splice(0..0, forwarders);
    assembled.nodes.insert(
        0,
        js::Node::ConstVariable(js::Variable {
            name: CHUNK_REGISTRY.to_string(),
            value: Box::new(js::Node::Call(
                Box::new(js::Node::Local("__chunk_registry".to_string())),
                Vec::new(),
            )),
        }),
    );

    let body = formatter.file(&assembled.nodes);
    let imports = assembled.imports.join("\n");
    let helpers = assembled
        .helpers
        .iter()
        .map(|name| helper_source(name))
        .collect::<Vec<_>>()
        .join("\n");
    let prelude = [imports, helpers]
        .into_iter()
        .filter(|section| !section.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    let main = if prelude.is_empty() {
        format!("{body}{line_break}")
    } else {
        format!("{prelude}\n{body}{line_break}")
    };

    Ok(SplitProgram {
        main,
        whole_bytes,
        chunks: plan
            .chunks
            .iter()
            .zip(chunk_sources)
            .map(|(chunk, source)| EmittedChunk {
                arm: chunk.arm.clone(),
                tag: chunk.tag,
                file: crate::chunks::chunk_file_name(leg, &chunk.arm),
                source,
            })
            .collect(),
    })
}

/// `__vilan_chunks.fn.<name>` — one function's slot in the registry.
fn registry_slot<'src>(name: &str) -> js::Node<'src> {
    js::Node::Property(
        Box::new(js::Node::Property(
            Box::new(js::Node::Local(CHUNK_REGISTRY.to_string())),
            "fn".to_string(),
        )),
        name.to_string(),
    )
}

/// The eager stand-in for a chunked function: same name, same parameters, one
/// hop through the registry. Every call site the route match emits is
/// unchanged, and a call made before the chunk landed fails loudly at the
/// forwarder instead of yielding `undefined`.
fn chunk_forwarder<'src>(function: &js::Function<'src>) -> js::Node<'src> {
    js::Node::Function(js::Function {
        name: function.name.clone(),
        parameters: function.parameters.clone(),
        body: vec![js::Node::Return(Box::new(js::Node::Call(
            Box::new(registry_slot(&function.name)),
            function
                .parameters
                .iter()
                .map(|parameter| js::Node::Local(parameter.name.clone()))
                .collect(),
        )))],
        is_async: function.is_async,
    })
}

/// Plants `__chunk_preload(<route signal>)` ahead of every statement that
/// mounts a recognized route swap (`bundle-splitting.md` §S3), and reports the
/// indices it inserted at in `body` itself so a caller holding a position into
/// that vector can adjust it.
///
/// The swap is the last call in its view chain, so its arguments — the shell
/// subtree among them — are all evaluated before the gate ever looks at the
/// route. A statement of its own, before that one, is the earliest point in the
/// program where the boot arm is known: the route value exists (it is the
/// statement's own argument), and nothing of the view has been built yet.
///
/// `gates` are the emitted names the gate's calls resolved to, and the preload's
/// argument is the swap's SOURCE argument — planted only when that argument is a
/// plain name, which is in scope at the statement by construction. Any other
/// shape (a route signal derived inline at the call) simply gets no preload,
/// which is the behaviour that shipped with S2.
fn plant_boot_preloads<'src>(
    body: &mut Vec<js::Node<'src>>,
    gates: &BTreeMap<String, String>,
    total: &mut usize,
) -> Vec<usize> {
    if gates.is_empty() {
        return Vec::new();
    }
    // Descend first, so a swap inside a nested body is planted beside the
    // statement in ITS list rather than the outer one.
    for node in body.iter_mut() {
        descend_for_preload(node, gates, total);
    }
    let mut planted = Vec::new();
    let mut index = 0;
    while index < body.len() {
        match gate_source_name(&body[index], gates) {
            Some((preload, source)) => {
                body.insert(
                    index,
                    js::Node::Call(
                        Box::new(js::Node::Local(preload)),
                        vec![js::Node::Local(source)],
                    ),
                );
                planted.push(index);
                *total += 1;
                index += 2;
            }
            None => index += 1,
        }
    }
    planted
}

/// Recurses into every statement list `node` contains — a function or closure
/// body wherever it sits, and the block forms — planting there.
fn descend_for_preload<'src>(
    node: &mut js::Node<'src>,
    gates: &BTreeMap<String, String>,
    total: &mut usize,
) {
    match node {
        js::Node::Function(function) => {
            plant_boot_preloads(&mut function.body, gates, total);
        }
        js::Node::Closure(closure) => {
            plant_boot_preloads(&mut closure.body, gates, total);
        }
        js::Node::ForOf(_, iterable, block) => {
            descend_for_preload(iterable, gates, total);
            plant_boot_preloads(block, gates, total);
        }
        js::Node::While(condition, block) => {
            descend_for_preload(condition, gates, total);
            plant_boot_preloads(block, gates, total);
        }
        js::Node::If(branch) => descend_if_for_preload(branch, gates, total),
        js::Node::Try(block, finally) => {
            plant_boot_preloads(block, gates, total);
            plant_boot_preloads(finally, gates, total);
        }
        js::Node::Call(subject, arguments) => {
            descend_for_preload(subject, gates, total);
            for argument in arguments {
                descend_for_preload(argument, gates, total);
            }
        }
        js::Node::ConstVariable(variable) | js::Node::LetVariable(variable) => {
            descend_for_preload(&mut variable.value, gates, total)
        }
        js::Node::Assignment(left, right)
        | js::Node::Binary(_, left, right)
        | js::Node::PropertyIndex(left, right) => {
            descend_for_preload(left, gates, total);
            descend_for_preload(right, gates, total);
        }
        js::Node::Await(inner)
        | js::Node::Unary(_, inner)
        | js::Node::Return(inner)
        | js::Node::Throw(inner)
        | js::Node::Spread(inner)
        | js::Node::Property(inner, _) => descend_for_preload(inner, gates, total),
        js::Node::Array(items) => {
            for item in items {
                descend_for_preload(item, gates, total);
            }
        }
        _ => {}
    }
}

fn descend_if_for_preload<'src>(
    branch: &mut js::IfBranch<'src>,
    gates: &BTreeMap<String, String>,
    total: &mut usize,
) {
    match branch {
        js::IfBranch::If(condition, block, else_branch) => {
            descend_for_preload(condition, gates, total);
            plant_boot_preloads(block, gates, total);
            if let Some(else_branch) = else_branch {
                descend_if_for_preload(else_branch, gates, total);
            }
        }
        js::IfBranch::Else(block) => {
            plant_boot_preloads(block, gates, total);
        }
    }
}

/// The route signal one statement's gate call reads, when the statement makes
/// such a call with a plainly-named source. Deliberately does NOT descend into
/// function or closure bodies or into block forms: those are statement lists of
/// their own, and [`plant_boot_preloads`] has already planted in them.
fn gate_source_name(node: &js::Node, gates: &BTreeMap<String, String>) -> Option<(String, String)> {
    match node {
        js::Node::Call(subject, arguments) => {
            if let js::Node::Local(name) = subject.as_ref()
                && let Some(preload) = gates.get(name)
                && let Some(js::Node::Local(source)) = arguments.get(1)
            {
                return Some((preload.clone(), source.clone()));
            }
            gate_source_name(subject, gates).or_else(|| {
                arguments
                    .iter()
                    .find_map(|node| gate_source_name(node, gates))
            })
        }
        js::Node::ConstVariable(variable) | js::Node::LetVariable(variable) => {
            gate_source_name(&variable.value, gates)
        }
        js::Node::Assignment(left, right)
        | js::Node::Binary(_, left, right)
        | js::Node::PropertyIndex(left, right) => {
            gate_source_name(left, gates).or_else(|| gate_source_name(right, gates))
        }
        js::Node::Await(inner)
        | js::Node::Unary(_, inner)
        | js::Node::Return(inner)
        | js::Node::Throw(inner)
        | js::Node::Spread(inner)
        | js::Node::Property(inner, _) => gate_source_name(inner, gates),
        js::Node::Array(items) => items.iter().find_map(|item| gate_source_name(item, gates)),
        _ => None,
    }
}

/// The names one file's top level declares — what the other side of the split
/// can address it by.
fn top_level_names(nodes: &[js::Node]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for node in nodes {
        match node {
            js::Node::Function(function) => {
                names.insert(function.name.clone());
            }
            js::Node::ConstVariable(variable) | js::Node::LetVariable(variable) => {
                names.insert(variable.name.clone());
            }
            _ => {}
        }
    }
    names
}

/// The symbols host `import` lines bind, so a chunk that names one can be
/// handed it through the registry rather than re-importing the module.
fn imported_symbols(imports: &[String]) -> Vec<String> {
    imports
        .iter()
        .filter_map(|line| {
            let start = line.find('{')? + 1;
            let end = line.find('}')?;
            Some(line.get(start..end)?.to_string())
        })
        .flat_map(|names| {
            names
                .split(',')
                .map(|name| name.trim().to_string())
                .collect::<Vec<_>>()
        })
        .filter(|name| !name.is_empty())
        .collect()
}

/// Every identifier a run of nodes mentions. A superset of its free variables
/// (a local shadowing a global is counted too), which is the safe direction:
/// the extra name is bound from the registry and then shadowed, costing one
/// declaration and never a missing one.
fn collect_references(nodes: &[js::Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        collect_reference(node, out);
    }
}

fn collect_reference(node: &js::Node, out: &mut BTreeSet<String>) {
    match node {
        js::Node::Local(name) => {
            out.insert(name.clone());
        }
        js::Node::Function(function) => collect_references(&function.body, out),
        js::Node::Closure(closure) => collect_references(&closure.body, out),
        js::Node::ConstVariable(variable) | js::Node::LetVariable(variable) => {
            collect_reference(&variable.value, out)
        }
        js::Node::ForOf(_, iterable, body) => {
            collect_reference(iterable, out);
            collect_references(body, out);
        }
        js::Node::While(condition, body) => {
            collect_reference(condition, out);
            collect_references(body, out);
        }
        js::Node::If(branch) => collect_reference_if(branch, out),
        js::Node::Try(body, finally) => {
            collect_references(body, out);
            collect_references(finally, out);
        }
        js::Node::Call(subject, arguments) => {
            collect_reference(subject, out);
            collect_references(arguments, out);
        }
        js::Node::Assignment(left, right)
        | js::Node::Binary(_, left, right)
        | js::Node::PropertyIndex(left, right) => {
            collect_reference(left, out);
            collect_reference(right, out);
        }
        js::Node::Await(inner)
        | js::Node::Unary(_, inner)
        | js::Node::Return(inner)
        | js::Node::Throw(inner)
        | js::Node::Spread(inner)
        | js::Node::Property(inner, _) => collect_reference(inner, out),
        js::Node::Array(items) => collect_references(items, out),
        js::Node::String(_)
        | js::Node::Number(_, _)
        | js::Node::Bool(_)
        | js::Node::Null
        | js::Node::Void
        | js::Node::Break
        | js::Node::Continue => {}
    }
}

fn collect_reference_if(branch: &js::IfBranch, out: &mut BTreeSet<String>) {
    match branch {
        js::IfBranch::If(condition, body, else_branch) => {
            collect_reference(condition, out);
            collect_references(body, out);
            if let Some(else_branch) = else_branch {
                collect_reference_if(else_branch, out);
            }
        }
        js::IfBranch::Else(body) => collect_references(body, out),
    }
}

/// Interprets a string literal's backslash escapes into the characters they
/// denote (`\n` -> newline, `\t`, `\r`, `\"`, `\\`, `\0`), so the value is the
/// real text — the JS formatter then re-escapes it for output. Borrows the slice
/// unchanged when it has no escapes. An unknown escape keeps both characters.
///
/// This is where a string literal's VALUE is built from source text — for a
/// plain `"…"` and for each literal fragment of an `i"…"` alike (the lexer
/// desugars an i-string into `String` tokens) — so it is where the
/// `\r\n`-is-one-line-terminator rule lands (`windows-support.md` §2,
/// specification §2.1): a multi-line literal's value carries `\n` per source
/// line break whatever the file's on-disk encoding, exactly as a triple-quoted
/// literal already does. An ESCAPED `\r` is unaffected — it is written, not read
/// from the line ending — and a lone `\r` in the text is preserved.
fn unescape_string(raw: &str) -> Cow<'_, str> {
    let raw = crate::util::normalize_newlines(raw);
    if !raw.contains('\\') {
        return raw;
    }
    let mut result = String::with_capacity(raw.len());
    let mut characters = raw.chars();
    while let Some(character) = characters.next() {
        if character != '\\' {
            result.push(character);
            continue;
        }
        match characters.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('r') => result.push('\r'),
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some('0') => result.push('\0'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => result.push('\\'),
        }
    }
    Cow::Owned(result)
}

/// The `__`-named free externs whose implementations live in the helper table:
/// a std module binds `[extern("__name")]`, and transforming a call through
/// one marks its helper for emission. Returns the canonical `'static` name.
fn extern_helper(symbol: &str) -> Option<&'static str> {
    const EXTERN_HELPERS: &[&str] = &[
        "__hmr_active",
        "__hmac_sha512",
        "__pbkdf2_sha512",
        "__sha256",
        "__sha384",
        "__sha512",
        "__random_bytes",
        "__db_run",
        "__db_all",
        "__db_get",
        "__db_column",
        "__db_is_null",
        "__db_close",
        "__db_exec_guarded",
        "__db_run_guarded",
        "__fs_close",
        "__fs_close_awaited",
        "__fs_stat",
        "__fs_read_dir_all",
        "__fs_watch",
        "__fs_watch_stop",
        "__local_get",
        "__session_get",
        "__router_path",
        "__nursery_new",
        "__nursery_new_detached",
        "__nursery_run",
        "__sleep",
        "__timer",
        "__chunk_arm",
        "__chunk_ready",
        "__chunk_load",
        "__chunk_preload",
        "__is_null",
    ];
    EXTERN_HELPERS.iter().find(|name| **name == symbol).copied()
}

/// The JS source for a runtime helper an intrinsic call needs. `__scan` reads
/// all of stdin once and hands out one line per call; `__parse_i32` returns the
/// `Option<i32>` array form (`[0, n]` = `Some`, `[1]` = `None`).
fn helper_source(name: &str) -> &'static str {
    match name {
        "__scan" => {
            "let __vilan_stdin = null, __vilan_stdin_index = 0;\n\
             function __scan() {\n\
             \tif (__vilan_stdin === null) {\n\
             \t\ttry {\n\
             \t\t\t__vilan_stdin = require(\"fs\").readFileSync(0, \"utf-8\").split(\"\\n\");\n\
             \t\t} catch (error) {\n\
             \t\t\t__vilan_stdin = [];\n\
             \t\t}\n\
             \t}\n\
             \treturn __vilan_stdin_index < __vilan_stdin.length ? __vilan_stdin[__vilan_stdin_index++] : \"\";\n\
             }"
        }
        // STRICT parses: the whole (trimmed) text must be the number — trailing
        // garbage, a fractional part on an integer, or an out-of-range value is
        // `None`, not a truncation (`parseInt`'s liberality said the wrong thing).
        "__parse_i32" => {
            "function __parse_i32(text) {\n\
             \tconst trimmed = text.trim();\n\
             \tconst value = Number(trimmed);\n\
             \treturn /^[+-]?[0-9]+$/.test(trimmed) && value >= -2147483648 && value <= 2147483647 ? [ 0, value ] : [ 1 ];\n\
             }"
        }
        "__parse_f64" => {
            "function __parse_f64(text) {\n\
             \tconst trimmed = text.trim();\n\
             \tconst value = Number(trimmed);\n\
             \treturn trimmed === \"\" || Number.isNaN(value) ? [ 1 ] : [ 0, value ];\n\
             }"
        }
        "__try_parse_json" => {
            "function __try_parse_json(text) {\n\
             \ttry {\n\
             \t\treturn [ 0, JSON.parse(text) ];\n\
             \t} catch (error) {\n\
             \t\treturn [ 1 ];\n\
             \t}\n\
             }"
        }
        // Route chunks (`bundle-splitting.md` §2/§3). The registry is created on
        // first touch and lives on `globalThis` because a chunk is a separate
        // module with no lexical view of the entry bundle. `base` is captured
        // from the entry's own <script> — a classic script's relative `import()`
        // resolves against the DOCUMENT's URL, which is the route the user is
        // on, so a bare `./chunk.js` would miss on every nested path. Node (and
        // any host without `document`) resolves relative to the importing file,
        // which is already right.
        "__chunk_registry" => {
            "function __chunk_registry() {\n\
             \tlet chunks = globalThis.__vilan_chunks;\n\
             \tif (chunks === undefined) {\n\
             \t\tlet base = \"\";\n\
             \t\tif (typeof document !== \"undefined\" && document.currentScript && document.currentScript.src) {\n\
             \t\t\tbase = document.currentScript.src;\n\
             \t\t}\n\
             \t\tchunks = { fn: Object.create(null), url: Object.create(null), loaded: Object.create(null), pending: Object.create(null), base: base };\n\
             \t\tglobalThis.__vilan_chunks = chunks;\n\
             \t}\n\
             \treturn chunks;\n\
             }"
        }
        // The route arm a value selects. An enum emits as `[tag, ..]`, and the
        // gate only ever sees the subject of a route `match`, so the tag is the
        // first slot; anything else is a build with no chunk map, where every
        // arm reports ready.
        "__chunk_arm" => {
            "function __chunk_arm(value) {\n\
             \treturn Array.isArray(value) ? value[0] : -1;\n\
             }"
        }
        // An arm with no entry in the map needs no chunk — which is every arm of
        // every single-file build, so `swap_split` is `swap` there.
        "__chunk_ready" => {
            "function __chunk_ready(arm) {\n\
             \tconst chunks = __chunk_registry();\n\
             \treturn chunks.url[arm] === undefined || chunks.loaded[arm] === true;\n\
             }"
        }
        // A failed fetch reports and does NOT continue: the route signal never
        // advances, so the previous view stays and the navigation simply did not
        // happen (`bundle-splitting.md` §2). `failed` carries the reason to
        // `std::router::chunk_error` (§S3) — without it a failure left
        // `pending()` stuck true forever, since only the success path cleared
        // it. The in-flight promise is dropped on failure so the next attempt
        // refetches.
        "__chunk_load" => {
            "function __chunk_load(arm, then, failed) {\n\
             \tconst chunks = __chunk_registry();\n\
             \tif (chunks.url[arm] === undefined || chunks.loaded[arm] === true) {\n\
             \t\tthen();\n\
             \t\treturn;\n\
             \t}\n\
             \tlet inflight = chunks.pending[arm];\n\
             \tif (inflight === undefined) {\n\
             \t\tconst url = chunks.url[arm];\n\
             \t\tconst specifier = chunks.base === \"\" ? \"./\" + url : new URL(url, chunks.base).href;\n\
             \t\tinflight = import(specifier).then(() => {\n\
             \t\t\tchunks.loaded[arm] = true;\n\
             \t\t\tdelete chunks.pending[arm];\n\
             \t\t}, (error) => {\n\
             \t\t\tdelete chunks.pending[arm];\n\
             \t\t\tconsole.error(\"[vilan] route chunk \" + url + \" failed to load\", error);\n\
             \t\t\tthrow error;\n\
             \t\t});\n\
             \t\tchunks.pending[arm] = inflight;\n\
             \t}\n\
             \tinflight.then(then, (error) => {\n\
             \t\tfailed(String(error));\n\
             \t});\n\
             }"
        }
        // The boot preload's fire-and-forget half (`bundle-splitting.md` §S3);
        // `std::ui::chunk_preload` computes the arm and calls this. Failure is
        // silent — `__chunk_load` has already reported it, and the gate's own
        // load surfaces it on `chunk_error()`.
        "__chunk_preload" => {
            "function __chunk_preload(arm) {\n\
             \t__chunk_load(arm, () => {}, () => {});\n\
             }"
        }
        // `std::ui::mount_target` (A24, fullstack-dx.md §9.5): the one peek at
        // whether a host value is JS `null`/`undefined` — `Element` (and any
        // other opaque `external struct` handle) has no vilan-visible way to
        // ask this itself.
        "__is_null" => {
            "function __is_null(value) {\n\treturn value === null || value === undefined;\n}"
        }
        "__random_int" => {
            "function __random_int(low, high) {\n\
             \treturn Math.floor(Math.random() * (high - low + 1)) + low;\n\
             }"
        }
        "__random_float" => {
            "function __random_float(low, high) {\n\
             \treturn Math.random() * (high - low) + low;\n\
             }"
        }
        // `process::args()` — the script's own arguments: `process.argv` is
        // `[node, script, ...args]`, so the tail past index 2 is what the program
        // was invoked with. `slice` returns a fresh array (no aliasing the live
        // `argv`), matching `List` value semantics.
        "__args" => {
            "function __args() {\n\
             \treturn process.argv.slice(2);\n\
             }"
        }
        // `Shared::new(value)` — a one-field object cell. An object (not an array)
        // is returned by reference from `__clone`, so the cell is shared, not
        // snapshotted — exactly the `Shared` semantics.
        "__shared_new" => {
            "function __shared_new(value) {\n\
             \treturn { v: value };\n\
             }"
        }
        // `process::env(key): Option<str>` — a missing variable reads back
        // `undefined`, which becomes `None`; otherwise `Some(value)`.
        "__env" => {
            "function __env(key) {\n\
             \tconst value = process.env[key];\n\
             \treturn value === undefined ? [ 1 ] : [ 0, value ];\n\
             }"
        }
        // `List.get(i): Option<T>` — bounds-checked, returning the `Option` array
        // form. Clones the element so the returned value can't alias the list
        // (value semantics; views are second-class and can't escape).
        "__list_get" => {
            "function __list_get(list, index) {\n\treturn index >= 0 && index < list.length ? [ 0, __clone(list[index]) ] : [ 1 ];\n}"
        }
        // WebCrypto glue (std::crypto): HMAC-SHA-512 over `crypto.subtle`.
        "__hmac_sha512" => {
            "async function __hmac_sha512(key, data) {\n\
             \tconst imported = await crypto.subtle.importKey(\"raw\", key, { name: \"HMAC\", hash: \"SHA-512\" }, false, [ \"sign\" ]);\n\
             \treturn new Uint8Array(await crypto.subtle.sign(\"HMAC\", imported, data));\n\
             }"
        }
        // PBKDF2-HMAC-SHA-512 via `crypto.subtle.deriveBits`.
        "__pbkdf2_sha512" => {
            "async function __pbkdf2_sha512(password, salt, iterations, bits) {\n\
             \tconst imported = await crypto.subtle.importKey(\"raw\", password, \"PBKDF2\", false, [ \"deriveBits\" ]);\n\
             \treturn new Uint8Array(await crypto.subtle.deriveBits({ name: \"PBKDF2\", salt, iterations, hash: \"SHA-512\" }, imported, bits));\n\
             }"
        }
        // Unkeyed content digests (kolt.local 024) via `crypto.subtle.digest`.
        // Async because WebCrypto is — the std::crypto stance (`std/misc.md`);
        // a path that cannot suspend binds the host primitive as an extern
        // instead, so there is deliberately no sync twin here.
        "__sha256" => {
            "async function __sha256(data) {\n\treturn new Uint8Array(await crypto.subtle.digest(\"SHA-256\", data));\n}"
        }
        "__sha384" => {
            "async function __sha384(data) {\n\treturn new Uint8Array(await crypto.subtle.digest(\"SHA-384\", data));\n}"
        }
        "__sha512" => {
            "async function __sha512(data) {\n\treturn new Uint8Array(await crypto.subtle.digest(\"SHA-512\", data));\n}"
        }
        // Web Storage glue (std::storage): a missing key reads null; flatten to "".
        "__local_get" => {
            "function __local_get(key) {\n\treturn localStorage.getItem(key) ?? \"\";\n}"
        }
        "__session_get" => {
            "function __session_get(key) {\n\treturn sessionStorage.getItem(key) ?? \"\";\n}"
        }
        // Router glue (std::router): `location.pathname` is a global property,
        // which the function-extern form can't address directly.
        "__router_path" => "function __router_path() {\n\treturn location.pathname;\n}",
        // HMR activity guard (std::dev, `hmr.md` §4/§5): true only when a `run
        // --watch` shim installed its `window.__VILAN_HMR__` singleton. A
        // self-contained `typeof` test (safe with no shim, in any host), so the
        // std hooks and `dev::*` calls that guard on it are inert in production
        // and under the interpreter (whose `__hmr_active` arm returns false).
        "__hmr_active" => {
            "function __hmr_active() {\n\treturn typeof globalThis.__VILAN_HMR__ !== \"undefined\";\n}"
        }
        // SQLite glue (std::db): parameter spreads and row/column reads the
        // extern binding forms can't express directly.
        "__db_run" => {
            "function __db_run(statement, parameters) {\n\
             \tconst result = statement.run(...parameters);\n\
             \treturn Number(result.lastInsertRowid ?? 0);\n\
             }"
        }
        "__db_all" => {
            "function __db_all(statement, parameters) {\n\treturn statement.all(...parameters);\n}"
        }
        "__db_get" => {
            "function __db_get(statement, parameters) {\n\
             \tconst row = statement.get(...parameters);\n\
             \treturn row === undefined ? [ 1 ] : [ 0, row ];\n\
             }"
        }
        "__db_column" => "function __db_column(row, name) {\n\treturn row[name];\n}",
        "__db_is_null" => {
            "function __db_is_null(row, name) {\n\treturn row[name] === null || row[name] === undefined;\n}"
        }
        // `Database`'s `Drop` closes the handle (destruction.md §9). No public
        // `close()` surfaces this — the destructor is the only caller.
        "__db_close" => "function __db_close(database) {\n\tdatabase.close();\n}",
        // The migrator's guard (`db-migrations.md` §6). `vilan` has no
        // `try`/`catch`, and `Database.migrate` has to name the step that
        // failed and roll its transaction back — so the catch lives here, in
        // the same `Option` array form `__db_get` and `__fs_stat` already use
        // at this boundary: `[ 1 ]` on success, `[ 0, message ]` on a throw.
        //
        // These are NOT public surface. Whether `std::db` should offer a
        // `Result`-returning `try_exec` is a real question about the module's
        // error posture — everything else in it throws — and it deserves its
        // own answer rather than arriving as a side effect of migrations.
        //
        // `error.message` is what `node:sqlite` puts the SQLite diagnosis in
        // (`no such table: taks`); the `String(error)` fallback covers a throw
        // of something that is not an Error at all.
        "__db_exec_guarded" => {
            "function __db_exec_guarded(database, sql) {\n\
             \ttry {\n\
             \t\tdatabase.exec(sql);\n\
             \t\treturn [ 1 ];\n\
             \t} catch (error) {\n\
             \t\treturn [ 0, error && error.message ? error.message : String(error) ];\n\
             \t}\n\
             }"
        }
        "__db_run_guarded" => {
            "function __db_run_guarded(statement, parameters) {\n\
             \ttry {\n\
             \t\tstatement.run(...parameters);\n\
             \t\treturn [ 1 ];\n\
             \t} catch (error) {\n\
             \t\treturn [ 0, error && error.message ? error.message : String(error) ];\n\
             \t}\n\
             }"
        }
        // `File`'s `Drop` (filesystem.md §5.1; kolt.local 031 Q1, ruled
        // (a)+(c) and scoped to `File` alone). `FileHandle.close()` returns a
        // promise and a destructor cannot await, so the drop INITIATES the
        // close without waiting: data already written is safe (a resolved
        // write was handed to the OS), and the process cannot exit under the
        // pending close (it holds the event loop). What is lost is the
        // close's own ERROR — reported here rather than left to take the
        // process down as an unhandled rejection. `with_file` is the spelling
        // in which the close is awaited and its failure observable.
        "__fs_close" => {
            "function __fs_close(file) {\n\
             \tfile.close().catch((error) => {\n\
             \t\tconsole.error(\"vilan: closing a dropped file failed:\", error);\n\
             \t});\n\
             }"
        }
        // `with_file`'s close — awaited, so a failure to close is a failure
        // of `with_file` itself. The handle's `Drop` still runs at scope end
        // and re-enters `close()` fire-and-forget; the host close is
        // idempotent (a second call resolves against the already-closed
        // handle), so the safety net stays benign behind the awaited path.
        "__fs_close_awaited" => {
            "async function __fs_close_awaited(file) {\n\tawait file.close();\n}"
        }
        // `std::fs::stat` (F13, fullstack-dx.md §9.3): `fs.promises.stat`
        // wrapped so a missing path reads back the `Option` array `None`
        // instead of throwing — vilan has no `try`/`catch`, so the ENOENT
        // catch has to live here. Every other failure re-throws, matching
        // `read_bytes`/`read_dir`/`read_file_to_str`'s posture. The dynamic
        // `import` is self-contained on purpose (no co-declared static
        // import to coordinate with, the same reason `__hmac_sha512` reaches
        // for the global `crypto` instead) and Node caches module resolution,
        // so a hot loop does not re-resolve the module per call.
        "__fs_stat" => {
            "async function __fs_stat(path) {\n\
             \tconst fsPromises = await import(\"node:fs/promises\");\n\
             \ttry {\n\
             \t\treturn [ 0, await fsPromises.stat(path) ];\n\
             \t} catch (error) {\n\
             \t\tif (error && error.code === \"ENOENT\") return [ 1 ];\n\
             \t\tthrow error;\n\
             \t}\n\
             }"
        }
        // `std::fs::read_dir_all` (kolt.local 019): `fs.promises.readdir`
        // under `{ recursive: true }` — an option-object argument the extern
        // binding forms cannot spell, so the call lives here (the same reason
        // `__fs_stat` does). No `try`/`catch`: a missing or unreadable
        // directory throws host-side, matching `read_dir`'s posture. The
        // dynamic `import` is self-contained on purpose, and Node caches
        // module resolution, so a hot loop does not re-resolve it per call.
        // Entries come back `/`-separated on every host. node's recursive
        // `readdir` joins with the PLATFORM separator, so on Windows a nested
        // entry arrived as `sub\\c.txt` — one component to `std::path`, which
        // is POSIX-shaped by ruling (kolt.local 017: a separator-aware path
        // module would make every derived cache key, asset url and golden
        // host-dependent). Normalizing here rather than in `read_dir_all`
        // keeps the whole language on one path shape, and this is the only
        // place in std where a host hands back a joined path.
        //
        // Gated on `path.sep`, NOT unconditional: a backslash is a LEGAL
        // filename byte on Linux, so rewriting it there would corrupt a real
        // name to fix a problem that platform does not have. On Windows a
        // filename cannot contain one, so splitting is unambiguous exactly
        // where it runs. (backlog N25)
        "__fs_read_dir_all" => {
            "async function __fs_read_dir_all(path) {\n\
             \tconst fsPromises = await import(\"node:fs/promises\");\n\
             \tconst nodePath = await import(\"node:path\");\n\
             \tconst entries = await fsPromises.readdir(path, { recursive: true });\n\
             \tif (nodePath.sep === \"/\") return entries;\n\
             \treturn entries.map((entry) => entry.split(nodePath.sep).join(\"/\"));\n\
             }"
        }
        // `std::fs::Watcher` (kolt.local 020, which owns the watch surface
        // whole). The mechanism is a stat-diffing POLL, not `node:fs`'s own
        // `watch`: that binding coalesces and duplicates events, folds
        // creation, deletion and renaming into one ambiguous `"rename"`,
        // varies by platform and node version on recursive watching, can
        // report a null filename on macOS, and throws outright on a path that
        // does not exist yet. Comparing two `stat`s promises strictly more —
        // Created / Modified / Removed, unambiguous everywhere — and it is the
        // choice the compiler made for its own `--watch` loop
        // (`proposals/proposal/watch-mode.md`).
        //
        // The loop lives here rather than in `fs.vl` for two reasons the
        // language decides: it must own a host timer and a wake queue, and a
        // walk of a live tree hits paths that vanish between the `readdir` and
        // the `stat`, which `vilan` has no `try`/`catch` to absorb. It calls
        // the same `fs.promises.stat` that `std::fs::stat` wraps and compares
        // the same `mtimeMs`/`size` that `Stat` exposes.
        //
        // A self-rescheduling `setTimeout`, deliberately NOT `setInterval`:
        // the scan is asynchronous, and an interval would stack overlapping
        // walks on a tree slower to read than the period. Entry keys are
        // `/`-separated on every host, the same normalization
        // `__fs_read_dir_all` applies and for the same ruling (kolt.local
        // 017), and gated on `path.sep` for the same reason — a backslash is a
        // legal filename byte on Unix.
        //
        // A directory's own mtime moves whenever its contents do, so a
        // directory reports Created and Removed but never Modified: that event
        // would restate an entry change the watcher has already reported
        // individually, and it does not fire uniformly across filesystems
        // anyway. A poll that fails for a reason other than absence reports to
        // stderr and keeps watching — a permissions error on one tick is not a
        // reason to silently stop observing.
        "__fs_watch" => {
            "class __Watcher {\n\
             \tconstructor(fsPromises, nodePath, root, recursive, intervalMs) {\n\
             \t\tthis.fs = fsPromises;\n\
             \t\tthis.nodePath = nodePath;\n\
             \t\tthis.root = nodePath.join(root, \".\");\n\
             \t\tthis.recursive = recursive;\n\
             \t\tthis.intervalMs = intervalMs;\n\
             \t\tthis.previous = new Map();\n\
             \t\tthis.queue = [];\n\
             \t\tthis.waiters = [];\n\
             \t\tthis.stopped = false;\n\
             \t\tthis.id = null;\n\
             \t}\n\
             \t__key(path) {\n\
             \t\treturn this.nodePath.sep === \"/\" ? path : path.split(this.nodePath.sep).join(\"/\");\n\
             \t}\n\
             \tasync __stat(path) {\n\
             \t\ttry {\n\
             \t\t\treturn await this.fs.stat(path);\n\
             \t\t} catch (error) {\n\
             \t\t\tif (error && (error.code === \"ENOENT\" || error.code === \"ENOTDIR\")) return null;\n\
             \t\t\tthrow error;\n\
             \t\t}\n\
             \t}\n\
             \tasync __snapshot() {\n\
             \t\tconst seen = new Map();\n\
             \t\tconst rootStat = await this.__stat(this.root);\n\
             \t\tif (rootStat === null) return seen;\n\
             \t\tseen.set(this.__key(this.root), { mtime: rootStat.mtimeMs, size: rootStat.size, dir: rootStat.isDirectory() });\n\
             \t\tif (!rootStat.isDirectory()) return seen;\n\
             \t\tlet names;\n\
             \t\ttry {\n\
             \t\t\tnames = await this.fs.readdir(this.root, { recursive: this.recursive });\n\
             \t\t} catch (error) {\n\
             \t\t\tif (error && error.code === \"ENOENT\") return seen;\n\
             \t\t\tthrow error;\n\
             \t\t}\n\
             \t\tfor (const name of names) {\n\
             \t\t\tconst full = this.nodePath.join(this.root, name);\n\
             \t\t\tconst entry = await this.__stat(full);\n\
             \t\t\tif (entry !== null) seen.set(this.__key(full), { mtime: entry.mtimeMs, size: entry.size, dir: entry.isDirectory() });\n\
             \t\t}\n\
             \t\treturn seen;\n\
             \t}\n\
             \t__diff(current) {\n\
             \t\tconst changes = [];\n\
             \t\tfor (const [ path, now ] of current) {\n\
             \t\t\tconst before = this.previous.get(path);\n\
             \t\t\tif (before === undefined) changes.push({ path: path, kind: [ 0 ] });\n\
             \t\t\telse if (!now.dir && (before.mtime !== now.mtime || before.size !== now.size)) changes.push({ path: path, kind: [ 1 ] });\n\
             \t\t}\n\
             \t\tfor (const path of this.previous.keys()) {\n\
             \t\t\tif (!current.has(path)) changes.push({ path: path, kind: [ 2 ] });\n\
             \t\t}\n\
             \t\tchanges.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);\n\
             \t\treturn changes;\n\
             \t}\n\
             \t__arm() {\n\
             \t\tif (this.stopped) return;\n\
             \t\tthis.id = setTimeout(() => this.__tick(), this.intervalMs);\n\
             \t}\n\
             \tasync __tick() {\n\
             \t\tthis.id = null;\n\
             \t\tif (this.stopped) return;\n\
             \t\tlet current;\n\
             \t\ttry {\n\
             \t\t\tcurrent = await this.__snapshot();\n\
             \t\t} catch (error) {\n\
             \t\t\tconsole.error(\"vilan: a filesystem watch poll failed:\", error);\n\
             \t\t\tthis.__arm();\n\
             \t\t\treturn;\n\
             \t\t}\n\
             \t\tif (this.stopped) return;\n\
             \t\tfor (const change of this.__diff(current)) this.queue.push(change);\n\
             \t\tthis.previous = current;\n\
             \t\twhile (this.queue.length > 0 && this.waiters.length > 0) this.waiters.shift().resolve(this.queue.shift());\n\
             \t\tthis.__arm();\n\
             \t}\n\
             \tnext_change(signal) {\n\
             \t\tif (this.queue.length > 0) return Promise.resolve(this.queue.shift());\n\
             \t\tconst sig = signal && signal[0] === 0 ? signal[1] : undefined;\n\
             \t\treturn new Promise((resolve, reject) => {\n\
             \t\t\tif (this.stopped) {\n\
             \t\t\t\treject(\"the watcher was dropped while a change was awaited\");\n\
             \t\t\t\treturn;\n\
             \t\t\t}\n\
             \t\t\tif (sig && sig.aborted) {\n\
             \t\t\t\treject(sig.reason);\n\
             \t\t\t\treturn;\n\
             \t\t\t}\n\
             \t\t\tconst waiter = { resolve: resolve, reject: reject };\n\
             \t\t\tthis.waiters.push(waiter);\n\
             \t\t\tif (sig) sig.addEventListener(\"abort\", () => {\n\
             \t\t\t\tconst parked = this.waiters.indexOf(waiter);\n\
             \t\t\t\tif (parked >= 0) this.waiters.splice(parked, 1);\n\
             \t\t\t\treject(sig.reason);\n\
             \t\t\t}, { once: true });\n\
             \t\t});\n\
             \t}\n\
             \tstop() {\n\
             \t\tif (this.stopped) return;\n\
             \t\tthis.stopped = true;\n\
             \t\tif (this.id !== null) clearTimeout(this.id);\n\
             \t\tthis.id = null;\n\
             \t\tconst waiters = this.waiters;\n\
             \t\tthis.waiters = [];\n\
             \t\tfor (const waiter of waiters) waiter.reject(\"the watcher was dropped while a change was awaited\");\n\
             \t}\n\
             }\n\
             async function __fs_watch(root, recursive, intervalMs) {\n\
             \tconst fsPromises = await import(\"node:fs/promises\");\n\
             \tconst nodePath = await import(\"node:path\");\n\
             \tconst watcher = new __Watcher(fsPromises, nodePath, root, recursive, intervalMs);\n\
             \twatcher.previous = await watcher.__snapshot();\n\
             \twatcher.__arm();\n\
             \treturn watcher;\n\
             }"
        }
        // `Watcher`'s `Drop` — genuinely synchronous, unlike `File`'s
        // (`clearTimeout` is not a promise), so this resource needs no part of
        // Q1's exception. Idempotent: a second stop is a no-op, so the
        // destructor stays benign behind any earlier `drop(watcher)`. No
        // public `stop()` surfaces it — the destructor is the only caller.
        "__fs_watch_stop" => "function __fs_watch_stop(watcher) {\n\twatcher.stop();\n}",
        // Cryptographically random bytes.
        "__random_bytes" => {
            "function __random_bytes(length) {\n\treturn crypto.getRandomValues(new Uint8Array(length));\n}"
        }
        // `list[i]` — the checked subscript read: out of bounds panics (`get`
        // is the total, Option-returning form above).
        "__at" => {
            "function __at(list, index) {\n\
             \tif (index >= 0 && index < list.length) return list[index];\n\
             \tthrow \"index out of bounds: the length is \" + list.length + \" but the index is \" + index;\n\
             }"
        }
        // `list[i] = v` — the checked subscript write: writing never creates a
        // slot (growth is `push`), so out of bounds panics.
        "__at_put" => {
            "function __at_put(list, index, value) {\n\
             \tif (index >= 0 && index < list.length) return list[index] = value;\n\
             \tthrow \"index out of bounds: the length is \" + list.length + \" but the index is \" + index;\n\
             }"
        }
        // `&mut list[i]` — the checked view mint: the scalar `(base, key)` pair
        // exists only for an in-bounds element.
        "__at_view" => {
            "function __at_view(list, index) {\n\
             \tif (index >= 0 && index < list.length) return [ list, index ];\n\
             \tthrow \"index out of bounds: the length is \" + list.length + \" but the index is \" + index;\n\
             }"
        }
        // `str.substring(start, end)` — the checked slice. The native JS method
        // clamps a negative to 0 and SWAPS an inverted pair, so `s.substring(k,
        // -1)` quietly returns `s[0..k]`, the complement of the request; and it
        // clamps `end` past the length rather than saying so. One rule replaces
        // all three guesses: `0 <= start <= end <= len`, refused otherwise.
        "__substring" => {
            "function __substring(text, start, end) {\n\
             \tif (0 <= start && start <= end && end <= text.length) return text.substring(start, end);\n\
             \tthrow \"substring out of range: the length is \" + text.length + \" but the range is \" + start + \"..\" + end + \" — substring requires 0 <= start <= end <= len and never clamps or swaps; to drop a known affix use strip_prefix/strip_suffix, and for the rest of the string pass s.len() as the end\";\n\
             }"
        }
        // `List.pop(): Option<T>` — removes and returns the last element (no clone:
        // the element leaves the list), or `None` when empty.
        "__list_pop" => {
            "function __list_pop(list) {\n\treturn list.length === 0 ? [ 1 ] : [ 0, list.pop() ];\n}"
        }
        // `List.sort_by(cmp): List<T>` — a new list, so the sort runs on a copy
        // and the receiver survives (`sort_by` takes `self` by value, like
        // `map`/`filter`). `Ordering` lowers to -1/0/1, which IS the comparator
        // contract, so the vilan closure is passed straight through. Stability
        // comes from the host: ECMA-262 has required a stable `sort` since ES2019.
        "__list_sort_by" => {
            "function __list_sort_by(list, compare) {\n\treturn list.slice().sort(compare);\n}"
        }
        // `Option.take(&mut self): Option<T>` — snapshot the slot (a structural
        // copy, not a deep clone: the payload MOVES out), then rewrite it to `None`
        // in place so the caller's binding sees the change. `[0, v]` -> the slot
        // becomes `[1]`, `[0, v]` is returned; `[1]` -> stays `[1]`.
        "__option_take" => {
            "function __option_take(slot) {\n\
             \tconst old = slot.slice();\n\
             \tslot.length = 1;\n\
             \tslot[0] = 1;\n\
             \treturn old;\n\
             }"
        }
        // `Option.replace(&mut self, value): Option<T>` — snapshot the slot, then
        // rewrite it to `Some(value)` in place; the old contents are returned.
        "__option_replace" => {
            "function __option_replace(slot, value) {\n\
             \tconst old = slot.slice();\n\
             \tslot[0] = 0;\n\
             \tslot[1] = value;\n\
             \tslot.length = 2;\n\
             \treturn old;\n\
             }"
        }
        // `Map.get(key): Option<V>` — returns the `Option` array form, cloning the
        // value so the result can't alias the map (value semantics).
        "__map_get" => {
            "function __map_get(map, key) {\n\treturn map.has(key) ? [ 0, __clone(map.get(key)) ] : [ 1 ];\n}"
        }
        // `Map.keys()`/`Map.values(): List<_>` — a fresh array snapshot (cloned, so
        // it can't alias the map's stored entries) in insertion order.
        "__map_keys" => "function __map_keys(map) {\n\treturn [ ...map.keys() ].map(__clone);\n}",
        "__map_values" => {
            "function __map_values(map) {\n\treturn [ ...map.values() ].map(__clone);\n}"
        }
        // `for x in set`: `Set` is a struct `[table]` over a `NativeMap`, so the
        // elements are the backing map's stored originals, in insertion order (I1).
        "__set_iter" => "function __set_iter(set) {\n\treturn [ ...set[0].values() ];\n}",
        // The trap arm of an exhaustive `match` over a BACKED enum
        // (backed-enums.md §9): the enum lowers to a bare host primitive, so its
        // runtime domain is the host's, not the variant set the analyzer proved
        // total over. Reaching this means the subject holds a value outside the
        // set — a panic naming the enum and the raw value, rather than the
        // confident wrong variant a bare `else` used to answer. `JSON.stringify`
        // so a string backing is quoted and a number is not.
        "__enum_trap" => {
            "function __enum_trap(name, value) {\n\tthrow name + \": \" + JSON.stringify(value) + \" is not one of its values\";\n}"
        }
        // The externally-tagged enum discriminator: a bare `"Variant"` is its own
        // tag, a `{"Variant":..}` object's tag is its single key.
        "__json_tag" => {
            "function __json_tag(value) {\n\treturn typeof value === \"string\" ? value : Object.keys(value)[0];\n}"
        }
        // The normalized JSON type of a parsed value: `typeof` buckets arrays and
        // `null` as `"object"`, so name them explicitly. Basis for the decode
        // type checks (`JsonValue.kind()` in json.vl).
        "__json_kind" => {
            "function __json_kind(value) {\n\tif (value === null) return \"null\";\n\tif (Array.isArray(value)) return \"array\";\n\treturn typeof value;\n}"
        }
        // The canonical key of a value: a primitive keys as itself (JS keys those
        // by value), an aggregate (an object/array) canonicalizes to its JSON
        // string. Basis of `Hashable` / value-keyed `Map`/`Set`.
        "__hash" => {
            "function __hash(value) {\n\treturn (typeof value === \"object\" && value !== null) ? JSON.stringify(value) : value;\n}"
        }
        // Value-semantics deep clone. Structs/lists/enums/tuples are arrays and a
        // `Set`/`Map` is a JS `Set`/`Map`, so recurse into them; everything else —
        // primitives and closures — is returned by reference (a closure is
        // immutable, so sharing it is a copy). Unlike `structuredClone`, this
        // doesn't throw on functions.
        // `[value; n]` — value evaluated once (the argument), then n slots. A
        // primitive fills directly (copies are trivial); an aggregate is cloned
        // per slot so the slots are independent (value semantics).
        "__repeat" => {
            "function __repeat(value, n) {\n\
             \treturn typeof value === \"object\" && value !== null\n\
             \t\t? Array.from({ length: n }, () => __clone(value))\n\
             \t\t: new Array(n).fill(value);\n\
             }"
        }
        // The `Task<T>` handle an `async` spawn yields (async-polymorphism.md
        // Part B). Eager: the body closure runs to its first suspension inside
        // the constructor. The rejection handler attached AT CONSTRUCTION
        // absorbs the failure (it can never surface as a host unhandled
        // rejection); if nothing observes the task — `await` and every other
        // consumer go through `then`, the thenable protocol — the failure is
        // reported one macrotask later with the spawn origin, and the program
        // continues. A class instance passes through `__clone` untouched, so
        // copying a task refers to the same task (handle semantics).
        // `globalThis.setTimeout`: a module importing `setTimeout` (e.g. from
        // "node:timers/promises") shadows the global in the whole module
        // scope, helper included. A spawn inside a nursery's dynamic extent
        // registers via the third argument; an OWNED task that fails with a
        // real (non-cancellation) error notifies its nursery AT SETTLE TIME
        // (`__fail`: latch the earliest, abort the signal, wake the drain),
        // and never default-reports — the nursery observes it.
        "__task" => {
            "class __Task {\n\
             \tconstructor(run, origin, nursery) {\n\
             \t\tthis.origin = origin;\n\
             \t\tthis.observed = false;\n\
             \t\tthis.nursery = nursery;\n\
             \t\tthis.owned = !!nursery;\n\
             \t\tthis.rejected = false;\n\
             \t\tthis.error = undefined;\n\
             \t\tthis.promise = run();\n\
             \t\tthis.promise.then(null, (error) => {\n\
             \t\t\tthis.rejected = true;\n\
             \t\t\tthis.error = error;\n\
             \t\t\tif (this.owned && !__nursery_is_cancel(error)) this.nursery.__fail(this);\n\
             \t\t\tif (!this.observed && !this.owned) {\n\
             \t\t\t\tglobalThis.setTimeout(() => {\n\
             \t\t\t\t\tif (!this.observed) console.error(\"unhandled task error (spawned in \" + this.origin + \"): \" + String(error));\n\
             \t\t\t\t}, 0);\n\
             \t\t\t}\n\
             \t\t});\n\
             \t\tif (nursery) nursery.children.push(this);\n\
             \t}\n\
             \tthen(onFulfilled, onRejected) {\n\
             \t\tthis.observed = true;\n\
             \t\treturn this.promise.then(onFulfilled, onRejected);\n\
             \t}\n\
             }\n\
             function __task(run, origin, nursery) {\n\
             \treturn new __Task(run, origin, nursery);\n\
             }"
        }
        // The nursery handle: children + an AbortController. `cancel()` (and
        // the join's first-error abort) fires the signal std IO listens on;
        // a child nursery chains to its parent's signal at creation, so an
        // outer cancel reaches every nested extent. The vilan-side methods
        // (`cancel`/`is_cancelled`/`signal_of`) bind via `[extern(method)]`.
        "__nursery_new" => {
            "class __Nursery {\n\
             \tconstructor(parent) {\n\
             \t\tthis.children = [];\n\
             \t\tthis.failedTask = undefined;\n\
             \t\tthis.failWake = undefined;\n\
             \t\tthis.controller = new AbortController();\n\
             \t\tif (parent) {\n\
             \t\t\tconst signal = parent.controller.signal;\n\
             \t\t\tif (signal.aborted) this.controller.abort(signal.reason);\n\
             \t\t\telse signal.addEventListener(\"abort\", () => this.controller.abort(signal.reason), { once: true });\n\
             \t\t}\n\
             \t}\n\
             \tcancel() {\n\
             \t\tthis.controller.abort();\n\
             \t}\n\
             \t__fail(task) {\n\
             \t\tif (this.failedTask === undefined) {\n\
             \t\t\tthis.failedTask = task;\n\
             \t\t\tthis.controller.abort();\n\
             \t\t\tif (this.failWake) this.failWake();\n\
             \t\t}\n\
             \t}\n\
             \tis_cancelled() {\n\
             \t\treturn this.controller.signal.aborted;\n\
             \t}\n\
             \tsignal_of() {\n\
             \t\treturn this.controller.signal;\n\
             \t}\n\
             }\n\
             function __nursery_new(parent) {\n\
             \treturn new __Nursery(parent && parent[0] === 0 ? parent[1] : undefined);\n\
             }"
        }
        // A DETACHED nursery — the one an `OwnedNursery` owns (destruction.md
        // §9). It is never joined, so it must not silently absorb a child's
        // failure the way the join does. `detached` marks the mode, and the
        // per-instance `__fail` override reopens the free-task reporting path:
        // a real (non-cancellation) child failure reports to the console with
        // its spawn origin and does NOT abort the controller, so siblings keep
        // running (ownership is lifetime, not fate-sharing). `__task` only
        // calls `__fail` for non-cancellation errors, so cancellation echoes
        // (an owner's `cancel`/`drop`) never reach here and stay silent. Reuses
        // the base `__Nursery` (co-emitted) untouched — so a plain `nursery`
        // program stays byte-identical.
        "__nursery_new_detached" => {
            "function __nursery_new_detached() {\n\
             \tconst n = __nursery_new(undefined);\n\
             \tn.detached = true;\n\
             \tn.__fail = function (task) {\n\
             \t\tif (!task.observed) {\n\
             \t\t\tglobalThis.setTimeout(() => {\n\
             \t\t\t\tif (!task.observed) console.error(\"unhandled task error (spawned in \" + task.origin + \"): \" + String(task.error));\n\
             \t\t\t}, 0);\n\
             \t\t}\n\
             \t};\n\
             \treturn n;\n\
             }"
        }
        // The nursery join (async-polymorphism.md Part B): run the body, then
        // drain the children — the list may grow while draining (children
        // spawn grandchildren). Failure reaction is AT SETTLE TIME: a failing
        // owned task latched itself via `__fail` (earliest-settled by
        // construction) and aborted the signal, and the drain races every
        // child against the wake so a fast failure behind a slow healthy
        // sibling is seen immediately. Cancellation-classified rejections
        // (`AbortError`) are echoes, never winners. On failure every child
        // is absorbed — observed, so no unobserved-failure reports; results
        // discarded — the body's throw wins (it always happens before the
        // join, and a cancellation interrupting the body propagates as the
        // nursery's outcome), and a string winner (a vilan panic) carries
        // its spawn origin into the message.
        "__nursery_run" => {
            "function __nursery_is_cancel(error) {\n\
             \treturn !!error && error.name === \"AbortError\";\n\
             }\n\
             async function __nursery_run(n, body) {\n\
             \tlet result;\n\
             \tlet bodyError;\n\
             \tlet bodyFailed = false;\n\
             \ttry {\n\
             \t\tresult = await body();\n\
             \t} catch (error) {\n\
             \t\tbodyFailed = true;\n\
             \t\tbodyError = error;\n\
             \t}\n\
             \tif (bodyFailed) n.controller.abort();\n\
             \tconst failed = new Promise((resolve) => {\n\
             \t\tn.failWake = resolve;\n\
             \t\tif (n.failedTask !== undefined) resolve();\n\
             \t});\n\
             \tlet index = 0;\n\
             \twhile (!bodyFailed && n.failedTask === undefined && index < n.children.length) {\n\
             \t\ttry {\n\
             \t\t\tawait Promise.race([n.children[index], failed]);\n\
             \t\t} catch (error) {}\n\
             \t\tif (n.failedTask === undefined) index += 1;\n\
             \t}\n\
             \tif (!bodyFailed && n.failedTask === undefined) return result;\n\
             \tfor (const task of n.children) task.then(null, () => {});\n\
             \tif (bodyFailed) throw bodyError;\n\
             \tconst winner = n.failedTask;\n\
             \tthrow typeof winner.error === \"string\" ? winner.error + \" (in task spawned in \" + winner.origin + \")\" : winner.error;\n\
             }"
        }
        // The abortable timer behind `std::time::sleep`: resolve after `ms`,
        // or reject with the abort reason (clearing the timer) when the
        // ambient cancel signal — an `Option<CancelSignal>` in the [0, s] /
        // [1] array form — fires first.
        "__sleep" => {
            "function __sleep(ms, signal) {\n\
             \tconst sig = signal && signal[0] === 0 ? signal[1] : undefined;\n\
             \treturn new Promise((resolve, reject) => {\n\
             \t\tif (sig && sig.aborted) {\n\
             \t\t\treject(sig.reason);\n\
             \t\t\treturn;\n\
             \t\t}\n\
             \t\tconst timer = setTimeout(() => resolve(), ms);\n\
             \t\tif (sig) sig.addEventListener(\"abort\", () => {\n\
             \t\t\tclearTimeout(timer);\n\
             \t\t\treject(sig.reason);\n\
             \t\t}, { once: true });\n\
             \t});\n\
             }"
        }
        // The cancelable host timer behind `std::time::Timer` — `setTimeout`
        // and `clearTimeout` as one value. The handle memoizes a VERDICT:
        // `true` once the timer fired, `false` once `cancel()` settled it
        // first, and the first settlement wins forever. Every waiter — the
        // ones already parked and the ones that arrive afterwards — observes
        // that same verdict, so `wait()` past settlement is an immediate
        // answer rather than a second timer.
        //
        // `wait` bridges an ambient cancel signal the way `__sleep` does, with
        // the one difference that is the point of the type: an abort rejects
        // THAT waiter (the structured teardown of the task awaiting) and
        // leaves the verdict unsettled and the host timer running. The timer
        // belongs to whoever holds the value, not to the nursery that happened
        // to await it, so its other holders can still wait or cancel. A
        // settled timer answers from the memo without consulting the signal —
        // there is nothing left to tear down.
        "__timer" => {
            "class __Timer {\n\
             \tconstructor(ms) {\n\
             \t\tthis.settled = false;\n\
             \t\tthis.verdict = false;\n\
             \t\tthis.waiters = [];\n\
             \t\tthis.id = setTimeout(() => this.__settle(true), ms);\n\
             \t}\n\
             \t__settle(verdict) {\n\
             \t\tif (this.settled) return;\n\
             \t\tthis.settled = true;\n\
             \t\tthis.verdict = verdict;\n\
             \t\tconst waiters = this.waiters;\n\
             \t\tthis.waiters = [];\n\
             \t\tfor (const wake of waiters) wake(verdict);\n\
             \t}\n\
             \tcancel() {\n\
             \t\tif (this.settled) return;\n\
             \t\tclearTimeout(this.id);\n\
             \t\tthis.__settle(false);\n\
             \t}\n\
             \twait(signal) {\n\
             \t\tif (this.settled) return Promise.resolve(this.verdict);\n\
             \t\tconst sig = signal && signal[0] === 0 ? signal[1] : undefined;\n\
             \t\treturn new Promise((resolve, reject) => {\n\
             \t\t\tif (sig && sig.aborted) {\n\
             \t\t\t\treject(sig.reason);\n\
             \t\t\t\treturn;\n\
             \t\t\t}\n\
             \t\t\tthis.waiters.push(resolve);\n\
             \t\t\tif (sig) sig.addEventListener(\"abort\", () => {\n\
             \t\t\t\tconst parked = this.waiters.indexOf(resolve);\n\
             \t\t\t\tif (parked >= 0) this.waiters.splice(parked, 1);\n\
             \t\t\t\treject(sig.reason);\n\
             \t\t\t}, { once: true });\n\
             \t\t});\n\
             \t}\n\
             }\n\
             function __timer(ms) {\n\
             \treturn new __Timer(ms);\n\
             }"
        }
        // Reads `__task`'s third argument out of a safe holder's
        // `Option<Nursery>` ([0, n] = Some, [1] = None).
        "__nursery_of" => {
            "function __nursery_of(option) {\n\treturn option[0] === 0 ? option[1] : undefined;\n}"
        }
        // Writing a whole aggregate through a view: REPLACE the pointee's
        // contents, keeping its identity so every alias sees the new value.
        // `Object.assign` alone is a merge — it never removes a slot the source
        // does not reach — so the length is set first (backlog B89). Both sides
        // are arrays for every aggregate the view machinery reaches (structs,
        // tuples, enums and `List` are arrays; a `Map` is a one-slot array); the
        // guard keeps an object-backed pointee on the plain merge.
        "__replace" => {
            "function __replace(target, value) {\n\
             \tif (Array.isArray(target) && Array.isArray(value)) target.length = value.length;\n\
             \treturn Object.assign(target, value);\n\
             }"
        }
        "__clone" => {
            "function __clone(value) {\n\
             \tif (Array.isArray(value)) return value.map(__clone);\n\
             \tif (value instanceof Set) return new Set([ ...value ].map(__clone));\n\
             \tif (value instanceof Map) return new Map([ ...value ].map(([ k, v ]) => [ __clone(k), __clone(v) ]));\n\
             \treturn value;\n\
             }"
        }
        _ => "",
    }
}

/// Whether two types name the same nominal struct/enum, ignoring type
/// arguments — so an `impl List<T>` (subject `List<Generic>`) matches a concrete
/// `List<i32>` value when resolving a member to emit.
fn nominal_matches(a: &Type, b: &Type) -> bool {
    match (a, b) {
        (Type::Struct(a_id, _), Type::Struct(b_id, _)) => a_id == b_id,
        (Type::Enum(a_id, _), Type::Enum(b_id, _)) => a_id == b_id,
        _ => a == b,
    }
}

/// Builds a binary expression, gluing adjacent string literals at compile time
/// so concatenations like `"" + "Hello, " + "!"` collapse to a single literal.
/// Because `+` is left-associative, folding here folds whole static runs.
fn binary<'src>(op: BinaryOp, lhs: js::Node<'src>, rhs: js::Node<'src>) -> js::Node<'src> {
    match (op, lhs, rhs) {
        (BinaryOp::Add, js::Node::String(left), js::Node::String(right)) => {
            let mut glued = left.into_owned();
            glued.push_str(&right);
            js::Node::String(Cow::Owned(glued))
        }
        (op, lhs, rhs) => js::Node::Binary(op, Box::new(lhs), Box::new(rhs)),
    }
}

/// How a dispatched trait-member call lowers, once resolved to a concrete type's
/// member. The member may be an intrinsic or an `[extern]` external (a host form),
/// not just a normal emitted function — so resolution is split from emission, and
/// `args` is consumed only once the form is known (see `resolve_dispatch`).
enum Dispatch<'src> {
    /// A built-in lowering (`str.len()` → `.length`, etc.).
    Intrinsic(Intrinsic),
    /// An `[extern]`-bound external: the external's id and its host binding.
    Extern(Id, ExternBinding<'src>),
    /// A normal emitted function: its JS name and whether it is async.
    Call(String, bool),
}

/// One lowered `match` leg, kept in pieces until the whole match is compiled:
/// the shape it is emitted in depends on whether ANY leg needs a statement slot
/// (see `Expr::Match`), which is only known once every guard has been walked.
struct MatchLeg<'src> {
    /// The variant/shape test, `None` for an irrefutable pattern.
    pattern_condition: Option<js::Node<'src>>,
    /// Statements that must run after the pattern test and before the guard:
    /// the guard's own temporaries and the copies its captures owe. Empty
    /// unless the guard needs them — an empty prelude is what keeps a plain
    /// guard in the else-if chain, byte for byte.
    prelude: Vec<js::Node<'src>>,
    /// The guard test, `None` for an unguarded leg.
    guard_condition: Option<js::Node<'src>>,
    body: Vec<js::Node<'src>>,
}

/// One BACKED-enum test inside a match leg's pattern, and where the value it
/// tests is read from (`backed_enum_name`'s enum, `compile_pattern`'s accessor).
/// The trap arm names both (backed-enums.md §9.4, §11.6).
struct BackedTest<'src> {
    enum_name: &'src str,
    enum_id: Id,
    value: js::Node<'src>,
}

struct Transformer<'src> {
    formatter: Formatter,
    ng: NameGenerator,
    print_fn_id: Id,
    list_new_fn_id: Option<Id>,
    list_push_fn_id: Option<Id>,
    panic_fn_id: Option<Id>,
    drop_fn_id: Option<Id>,
    program: &'src Program<'src>,
    required_functions: IndexMap<Id, js::Node<'src>>,
    // Functions whose body is currently being walked. A recursive (or mutually
    // recursive) call inside that body must not re-enter and re-emit it — the
    // call site only needs the function's name, which is available regardless.
    // Kept separate from `required_functions` (which records *finished* bodies)
    // so the callee-before-caller insertion order is preserved.
    emitting: HashSet<Id>,
    // The active generic-parameter substitution while emitting a monomorphized
    // function body (constraint id -> concrete type id).
    current_substitution: HashMap<TypeId, TypeId>,
    // The adapted-instance context while emitting a body
    // (async-polymorphism.md A.1): WHICH parameters are async in the
    // instance being emitted, and its await/instantiation decisions. Base
    // (un-adapted) bodies carry their base entry when they instantiate
    // adapted callees.
    current_adapted: Vec<Id>,
    current_instance: Option<crate::analyzer::AdaptedInstance>,
    // The source name of the function whose body is being emitted — the spawn
    // ORIGIN stamped into `__task` calls, so an unobserved task failure can
    // name where it was spawned. `None` at module level ("top level").
    current_origin: Option<&'src str>,
    // Every entity emitted as a VALUE reference (the `Expr::Local` arm) —
    // consulted at assembly to tree-shake module-level bindings (F6): a
    // binding emits only if something reachable referenced it.
    referenced_globals: HashSet<Id>,
    // Monomorphized function variants, keyed by (generic function, concrete
    // type arguments) so each distinct instantiation is emitted exactly once.
    instances: HashMap<(Id, Vec<String>, Vec<Id>), String>,
    // The concrete type a trait default method is currently being specialized
    // for, so `self.method()` calls in its body re-dispatch to that type's impl.
    current_self_type: Option<TypeId>,
    // Trait default methods specialized per concrete type, keyed by
    // (default function, concrete type) so each is emitted once.
    default_instances: HashMap<(Id, String), String>,
    // Per-type `__drop` helpers (destruction.md §7), keyed by `type_key`. `None`
    // records a type whose destruction is a complete no-op (no `Drop` impl, no
    // resource members) so callers skip it; `Some(name)` is the emitted helper.
    drop_helpers: HashMap<String, Option<String>>,
    monomorphized: Vec<js::Node<'src>>,
    // Captures introduced by an `is` test, aliased to the subject's payload
    // slots (e.g. `t[1]`) since they can't be JS bindings in expression position.
    is_bindings: HashMap<Id, js::Node<'src>>,
    // Expressions already evaluated into a temp, which every occurrence names
    // instead of re-evaluating: a compound assignment's INDEXED target subscript,
    // walked once for the write and once for the synthesized re-read (B105).
    hoisted_values: HashMap<Id, js::Node<'src>>,
    // While `Some`, every `is_bindings` lookup records the capture it resolved.
    // A match guard is walked with this on, so the leg's lowering can tell
    // whether the guard READS a capture whose copy has to be declared ahead of
    // it (B59) — the guard's reference is to the copy, not the subject's slot.
    is_binding_reads: Option<HashSet<Id>>,
    // Runtime helper functions (`__scan`, `__parse_i32`, `__random_int`) an
    // intrinsic call needs; emitted as a prelude only when used.
    used_helpers: BTreeSet<&'static str>,
    // Host imports an `[extern]` call needs, as module -> imported symbols;
    // emitted as `import { a, b } from "module";` lines at the top.
    used_imports: BTreeMap<String, BTreeSet<String>>,
    // Emit HMR instrumentation (`hmr.md` §5): wrap each transferable module-level
    // binding's initializer in an `__hmr_adopt` thunk and emit a matching
    // `__hmr_expose` getter at the module tail. Set only by an HMR-active `run
    // --watch`; false keeps output byte-identical.
    hmr: bool,
    // Never-silent guard (B55): functions emitted as a call target that have NO
    // source body — a trait's signature-only requirement. Such an emission is
    // always a resolution failure (a generic that never got bound), and its
    // output is `function f(self) {\n}`: a clean compile whose first use of the
    // result is a runtime `TypeError`. Collected here and turned into a hard
    // compile error at assembly, so the class cannot recur silently.
    bodyless_emissions: Vec<Id>,
    // B135: memo for `reaches_bare_requirement` — whether a function's body,
    // transitively through the program call graph, contains a dispatch that
    // would fall through to a bodyless trait requirement if emitted without a
    // substitution. Keyed by the queried root only (the walk's per-query
    // visited set keeps cycles finite without poisoning other roots).
    bare_requirement_memo: HashMap<Id, bool>,
    // Never-silent guard (B68, affine-moves.md §9.4): `drop(x)` sink calls whose
    // argument type did not resolve at the rewrite. Such a call lowers to the
    // bare argument — indistinguishable from the legitimate data no-op — so a
    // resource handed to it is destroyed nowhere, from a compile that reported
    // nothing. Collected here and turned into a hard compile error at assembly,
    // so the class cannot recur silently.
    unresolved_drop_sinks: Vec<Id>,
    // C11 (`temporary-drop.md`): the resource temporaries lifted out of the
    // statement currently being emitted, innermost last. Each records where its
    // `const` landed in the statement list, so the emitter can close a
    // `try`/`finally` around everything after it. Empty between statements.
    pending_temporaries: Vec<PendingTemporary>,
    // Route-chunk partition (`bundle-splitting.md` S2): function id -> chunk
    // index, plus how many chunks there are. Empty for every build that did not
    // ask to split, which is what keeps single-file emission byte-identical —
    // the partition is the ONLY thing this feature adds to the walk.
    chunk_members: HashMap<Id, usize>,
    chunk_count: usize,
    // The route gate. `None` for every build that is not splitting.
    chunk_gate: Option<ChunkGate>,
    // The emitted name each gate call resolved to, paired with the name of the
    // boot preload for the same route type (`bundle-splitting.md` §S3). Recorded
    // at emission, so these are PRE-rename names — which is what the planting
    // pass, which runs before the rename, matches against.
    gate_call_names: BTreeMap<String, String>,
    // The const pass's per-emission attribution (`const-eval.md` §10.6). `None`
    // for every other transform — an entry build records nothing, and the field
    // is what keeps the emission path it shares with the const pass unchanged.
    recorder: Option<EmissionRecorder>,
}

/// One thing the shared const world declares: a concrete function, or a KEYED
/// emission — a generic instance, a specialized trait default, or a per-type
/// drop helper, all of which land in [`Transformer::monomorphized`]. A keyed
/// emission's identity is reserved BEFORE its body is walked (its
/// `monomorphized` slot is not known until after), so a self- or mutually
/// recursive requirement inside that body still resolves to it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum EmissionId {
    Function(Id),
    Keyed(usize),
}

/// What one emission contributed DIRECTLY, recorded the first — and, in the
/// shared world, only — time it is lowered (`const-eval.md` §10.6).
///
/// A per-site mini-program used to re-walk every function its expression
/// reached, and that walk was where the site learned three things besides the
/// code: which module-level bindings it reads (its prelude, and the
/// `unresolved` diagnostics), which runtime helpers it needs, and which host
/// imports it reaches (what `check_capabilities` refuses on). Lowering each
/// function once would lose all three for every site after the first — so each
/// emission records them, together with what it directly required, and a site
/// recovers its exact set by closing over `requires` from its own walk.
#[derive(Default)]
struct EmissionRecord {
    globals: HashSet<Id>,
    helpers: BTreeSet<&'static str>,
    imports: BTreeMap<String, BTreeSet<String>>,
    /// Everything this body required directly, whether or not the requirement
    /// was already emitted — a memo hit contributes to the reaching site's set
    /// exactly as a fresh emission does.
    requires: Vec<EmissionId>,
}

/// The per-emission records, plus the open frames they are captured in. Present
/// only on the const pass's transformer.
#[derive(Default)]
struct EmissionRecorder {
    frames: Vec<EmissionRecord>,
    records: HashMap<EmissionId, EmissionRecord>,
    /// Where each keyed emission's node landed in `monomorphized`, by reserved
    /// index. `None` until the body is walked and pushed.
    keyed_slots: Vec<Option<usize>>,
    /// The reserved identity of each keyed emission, by the memo key its own
    /// emitter uses — consulted on a memo hit, the one place the identity is
    /// not already in hand.
    instances: HashMap<(Id, Vec<String>, Vec<Id>), EmissionId>,
    defaults: HashMap<(Id, String), EmissionId>,
    drops: HashMap<String, EmissionId>,
}

thread_local! {
    /// How many function bodies the const pass has LOWERED on this thread since
    /// [`reset_const_lowering_count`]. The instrument behind M4-A's pin
    /// (`const-eval.md` §10.6), on the same argument as the call-graph and
    /// name-seed counters beside it: a world lowered once per pass and a world
    /// lowered once per site produce IDENTICAL results — the sharing is
    /// behaviour-neutral by construction — so only a counter can tell them
    /// apart, and only a counter can catch the per-site rebuild creeping back.
    ///
    /// Bumped only under a recorder, so it counts the const pass and nothing
    /// else; one `Cell` bump against a whole function-body walk is
    /// unmeasurable.
    static CONST_LOWERING_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// How many function bodies the const pass has lowered on this thread since the
/// last [`reset_const_lowering_count`]. See [`CONST_LOWERING_COUNT`].
pub fn const_lowering_count() -> usize {
    CONST_LOWERING_COUNT.with(std::cell::Cell::get)
}

/// Zeroes this thread's [`const_lowering_count`].
pub fn reset_const_lowering_count() {
    CONST_LOWERING_COUNT.with(|count| count.set(0));
}

/// The three accumulating sets a frame borrows while it is open.
type FrameSets = (
    HashSet<Id>,
    BTreeSet<&'static str>,
    BTreeMap<String, BTreeSet<String>>,
);

/// What a split build's route gate rewires: `View.swap` becomes
/// `View.swap_split` at the recognized calls, and `std::ui::chunk_preload` is
/// planted ahead of the statement that mounts each one.
struct ChunkGate {
    swap: Id,
    swap_split: Id,
    preload: Id,
    calls: HashSet<Id>,
}

/// One entry's emission, partitioned. `chunks` is empty unless the transformer
/// was given a route-chunk partition; the eager `nodes` are then exactly the
/// nodes a single-file build would emit minus the chunked function
/// declarations, renamed identically (one walk, one rename, then the split).
struct Assembled<'src> {
    imports: Vec<String>,
    helpers: Vec<&'static str>,
    nodes: Vec<js::Node<'src>>,
    chunks: Vec<Vec<js::Node<'src>>>,
    /// Where `main`'s inlined body starts in `nodes` — the seam a split build
    /// files the chunk map and the registrations into, after every module
    /// binding has initialized and before anything can navigate.
    main_body_start: usize,
}

/// How a resource-owning scope's tail value is used when it is restructured into
/// `try`/`finally` (destruction.md §7).
#[derive(Clone)]
enum TailDisposition {
    /// A function body: the tail is `return`ed, flowing out through the finallys.
    Return,
    /// A statement-position scope (a `{}` block used as a statement, a loop body,
    /// a discarded branch): the tail runs for effect, its value dropped.
    Discard,
    /// A value-position `{}` block: the tail is assigned to this temp (declared
    /// before the tries), so the block's value survives the finallys.
    AssignTo(String),
    /// A value-position `if`/`match` arm: the tail assigns to the result temp, or
    /// — if it diverges (`ret`/`jump`) — runs as-is (`push_result_or_divergence`),
    /// inside the drop scope so teardown precedes the branch's result.
    ResultOrDivergence(String),
}

/// One resource temporary lifted out of the statement being emitted
/// (`temporary-drop.md` §7.1): the index of its minted `const` in that
/// statement's node list, the name it was given, and the type whose destructor
/// the closing `finally` calls.
struct PendingTemporary {
    at: usize,
    name: String,
    type_id: TypeId,
}

/// What a direct statement of a scope owes at the scope's end (destruction.md
/// §7). Classified with `&self` so the (`&mut self`) emission can borrow freely.
enum ScopeTeardown {
    /// Nothing — the statement declares no resource this scope destroys.
    None,
    /// A resource `let` still owned at the scope's end.
    Binding(Id),
    /// B62: the resource payloads a `let`-pattern captured out of a consumed
    /// subject, in declaration order.
    Captures(Vec<Id>),
}

impl<'src> Transformer<'src> {
    fn new(program: &'src Program<'src>, options: &BuildOptions) -> Self {
        Self::with_name_seed(program, options, Rc::new(NameSeed::build(program, options)))
    }

    /// [`Transformer::new`] over an ALREADY-BUILT name seed — the entry a caller
    /// that transforms the same program many times uses, so the seed is built
    /// once instead of once per transform. See [`NameSeed`] and
    /// [`ConstProgramSeed`].
    fn with_name_seed(
        program: &'src Program<'src>,
        options: &BuildOptions,
        names: Rc<NameSeed>,
    ) -> Self {
        let print_fn_id = {
            let std_module_id = *program
                .module_id_by_name
                .get("std")
                .expect("missing std module");
            let std_module = program.modules.get(&std_module_id).unwrap();
            let std_module_scope_id = std_module.body.1;
            let std_module_scope = program.scopes.get(&std_module_scope_id).unwrap();
            let print_fn_id = *std_module_scope
                .name_to_id_map
                .get("print")
                .expect("missing print function in the std module");
            print_fn_id
        };

        Self {
            formatter: Formatter::from_options(options.indent, options.spaces),
            ng: NameGenerator::new(names),
            print_fn_id,
            list_new_fn_id: program.list_new_fn_id,
            list_push_fn_id: program.list_push_fn_id,
            panic_fn_id: program.panic_fn_id,
            drop_fn_id: program.drop_fn_id,
            program,
            required_functions: IndexMap::new(),
            emitting: HashSet::default(),
            current_substitution: HashMap::default(),
            current_adapted: Vec::new(),
            current_instance: None,
            current_origin: None,
            referenced_globals: HashSet::default(),
            instances: HashMap::default(),
            current_self_type: None,
            default_instances: HashMap::default(),
            drop_helpers: HashMap::default(),
            monomorphized: Vec::new(),
            is_bindings: HashMap::default(),
            hoisted_values: HashMap::default(),
            is_binding_reads: None,
            used_helpers: BTreeSet::new(),
            used_imports: BTreeMap::new(),
            hmr: options.hmr,
            bodyless_emissions: Vec::new(),
            bare_requirement_memo: HashMap::default(),
            unresolved_drop_sinks: Vec::new(),
            pending_temporaries: Vec::new(),
            chunk_members: HashMap::default(),
            chunk_count: 0,
            chunk_gate: None,
            gate_call_names: BTreeMap::new(),
            recorder: None,
        }
    }

    fn transform_entry(self) -> Result<String, Error> {
        let formatter = self.formatter.clone();
        let line_break = formatter.line_break;
        let program = self.transform_entry_ast()?;
        let body = formatter.file(&program.nodes);
        let imports = program.imports.join("\n");
        let helpers = program
            .helpers
            .iter()
            .map(|name| helper_source(name))
            .collect::<Vec<_>>()
            .join("\n");
        let prelude = [imports, helpers]
            .into_iter()
            .filter(|section| !section.is_empty())
            .collect::<Vec<_>>()
            .join("\n");
        let output = if prelude.is_empty() {
            body
        } else {
            format!("{}\n{}", prelude, body)
        };
        Ok(format!("{}{}", output, line_break))
    }

    fn transform_entry_ast(self) -> Result<JsProgram<'src>, Error> {
        let assembled = self.assemble()?;
        debug_assert!(
            assembled.chunks.is_empty(),
            "a single-file build splits nothing"
        );
        Ok(JsProgram {
            imports: assembled.imports,
            helpers: assembled.helpers,
            nodes: assembled.nodes,
        })
    }

    fn assemble(mut self) -> Result<Assembled<'src>, Error> {
        let global_scope = self
            .program
            .scopes
            .get(&self.program.global_scope_id)
            .unwrap();

        let main_fn = global_scope
            .name_to_id_map
            .get("main")
            .and_then(|id| self.program.functions.get(id))
            .ok_or_else(|| Error {
                trace: Vec::new(),
                note: None,
                msg: "Cannot execute program without a main function".to_string(),
                span: Span::new((), 0..0),
            })?;
        let main_is_async = self.program.async_functions.contains(&main_fn.id);

        // Every module-level binding, in INITIALIZATION order — dependency
        // order over the load-time relation, ties broken by the canonical key
        // (`b33-emission-order.md`). A module-level `let` emits a non-hoisted
        // `const`, so declaration order IS initialization order: a binding
        // whose initializer evaluates another must be declared after it. The
        // call graph serves both this and the reachability filter below — and
        // it is the one the post-`analyze()` cycle check already built
        // ([`Program::call_graph`]), not a second build of the same thing.
        let graph = self.program.call_graph();
        let global_variables = crate::init_order::initialization_order(self.program, graph);

        // Walk the module-level bindings the entry can REACH, in initialization
        // order, keeping each binding's nodes separate (F6 — a binding emits
        // only if something reachable references it; the stated semantics: a
        // dropped binding's initializer does not run — module state exists
        // only if something reaches it; top-level side effects are not a
        // promise). Reachability comes from the call graph — the same edges
        // platform coloring admits over — so a dropped binding is never even
        // walked: its initializer can't retain callees, nor drag their host
        // `import ... from "node:..."` lines into a bundle that never runs
        // it. Assembly still keeps only bindings that emitted code actually
        // referenced (dispatch candidates over-approximate reachability, like
        // everywhere else — such a binding is walked but then dropped here,
        // and it was admission-checked by the same graph).
        // The route gate is a second root: nothing in source calls
        // `View.swap_split`, so its own module state (the pending signal) is
        // invisible to a walk from `main` alone.
        let gate_roots: Vec<Id> = self
            .chunk_gate
            .as_ref()
            .map(|gate| vec![gate.swap_split, gate.preload])
            .unwrap_or_default();
        let reachable_bindings = crate::platform_color::reachable_bindings(
            self.program,
            &graph,
            main_fn.id,
            &gate_roots,
        );
        let binding_nodes: Vec<(Id, Vec<js::Node<'src>>)> = global_variables
            .iter()
            .filter(|binding| reachable_bindings.contains(binding))
            .map(|&binding| (binding, self.walk_list(&vec![binding])))
            .collect();

        let saved_instance = self.enter_instance(main_fn.id, Vec::new());
        // A `main` owning resource locals is restructured into `try`/`finally`
        // teardown (destruction.md §7); one owning none emits exactly as before.
        // `process.exit` does NOT run pending `finally` blocks, so a non-void exit
        // code is captured to a temp *inside* the drop scope, the finallys (drops)
        // run, and only THEN does `process.exit` fire — teardown genuinely precedes
        // exit. A void tail (or a host without `process.exit`, e.g. the browser)
        // needs no capture: teardown runs and `main` falls through, the tail's side
        // effects preserved.
        let mut t_main_fn_body = if self.scope_needs_drops(&main_fn.body.0) {
            let tail_is_void = matches!(
                self.program.entity_map.get(&main_fn.body.1),
                Some(Expr::Void) | None
            );
            if tail_is_void || !self.program.platform.has_process_exit() {
                self.walk_scope_body(
                    &main_fn.body.0,
                    0,
                    main_fn.body.0.len(),
                    Some((main_fn.body.1, TailDisposition::Discard)),
                )
            } else {
                let exit_temp = self.ng.next_name();
                let mut body = vec![js::Node::LetVariable(js::Variable {
                    name: exit_temp.clone(),
                    value: Box::new(js::Node::Void),
                })];
                let wrapped = self.walk_scope_body(
                    &main_fn.body.0,
                    0,
                    main_fn.body.0.len(),
                    Some((main_fn.body.1, TailDisposition::AssignTo(exit_temp.clone()))),
                );
                body.extend(wrapped);
                body.push(js::Node::Call(
                    Box::new(js::Node::Property(
                        Box::new(js::Node::Local("process".to_string())),
                        "exit".to_string(),
                    )),
                    vec![js::Node::Local(exit_temp)],
                ));
                body
            }
        } else {
            let mut t_main_fn_body = self.walk_list(&main_fn.body.0);

            // Emit main's trailing expression (and any statements it expands to). On
            // Node a non-void result is forwarded to `process.exit` (the exit code); a
            // void tail (e.g. a block ending in a loop) exits normally. The browser has
            // no exit code, so the tail is emitted as a plain statement — its side
            // effects still run (a `main` that ends in `render()`), the value discarded.
            if let Some(value) = self.walk_entity(main_fn.body.1, &mut t_main_fn_body) {
                if !matches!(value, js::Node::Void) {
                    // A host with `process.exit` (Node) forwards `main`'s result as the
                    // exit code; the browser (and the host-less `none`, which the CLI
                    // refuses to *build*) has none, so the tail is a plain statement.
                    let statement = if self.program.platform.has_process_exit() {
                        js::Node::Call(
                            Box::new(js::Node::Property(
                                Box::new(js::Node::Local("process".to_string())),
                                "exit".to_string(),
                            )),
                            vec![value],
                        )
                    } else {
                        value
                    };
                    t_main_fn_body.push(statement);
                }
            }
            t_main_fn_body
        };

        self.restore_instance(saved_instance);

        // An async `main` (it awaits) runs inside an invoked async arrow, since
        // module initialization is synchronous and there is no top-level await
        // to lift it into (`execution.md` §7.1): `(async () => { .. })()`.
        //
        // That promise used to be DISCARDED (J6). A rejection then reached the
        // host only as an unhandled-rejection event, so what a failing `main`
        // did was the host's default policy rather than vilan's: Node ≥15
        // happens to rethrow and exit non-zero, but it buries the program's
        // error under `UnhandledPromiseRejection` and an engine-internal
        // stack, and a host configured otherwise (or an older Node) exits 0.
        // A *sync* `main` that panics has always terminated with the message
        // and a non-zero code, and async `main` is the substitute vilan steers
        // people to instead of top-level await — so the two must agree.
        //
        // `.catch` and not `await`: attaching a handler does not delay
        // anything, so a `main` that never settles (a listening server) is
        // untouched — it keeps running, and the handler simply never fires.
        // `process.exit` rather than `exitCode`, for the same reason in
        // reverse: a rejection while some other handle is still live (that
        // same listener) would otherwise set a code and then hang forever.
        // The unwind through `main` has already run its `finally` blocks by
        // the time the handler sees the error, so exiting here does not skip
        // teardown the way §7.1's exit-code path would.
        if main_is_async {
            let invocation = js::Node::Call(
                Box::new(js::Node::Closure(js::Closure {
                    parameters: Vec::new(),
                    body: t_main_fn_body,
                    is_async: true,
                })),
                Vec::new(),
            );
            // The browser has no exit code; its unhandled-rejection path
            // already reports to the console, so there is nothing to add.
            t_main_fn_body = if self.program.platform.has_process_exit() {
                let error_name = self.ng.next_name();
                vec![js::Node::Call(
                    Box::new(js::Node::Property(
                        Box::new(invocation),
                        "catch".to_string(),
                    )),
                    vec![js::Node::Closure(js::Closure {
                        parameters: vec![js::Parameter {
                            name: error_name.clone(),
                        }],
                        body: vec![
                            js::Node::Call(
                                Box::new(js::Node::Property(
                                    Box::new(js::Node::Local("console".to_string())),
                                    "error".to_string(),
                                )),
                                vec![js::Node::Call(
                                    Box::new(js::Node::Local("String".to_string())),
                                    vec![js::Node::Local(error_name)],
                                )],
                            ),
                            js::Node::Call(
                                Box::new(js::Node::Property(
                                    Box::new(js::Node::Local("process".to_string())),
                                    "exit".to_string(),
                                )),
                                vec![js::Node::Number("1".to_string(), None)],
                            ),
                        ],
                        is_async: false,
                    })],
                )]
            } else {
                vec![invocation]
            };
        }

        // Assembly-time tree-shake: keep a binding's declaration only when
        // something emitted referenced it.
        let emitted_bindings: Vec<Id> = binding_nodes
            .iter()
            .filter(|(binding, _)| self.referenced_globals.contains(binding))
            .map(|(binding, _)| *binding)
            .collect();
        let t_global_variables: Vec<js::Node<'src>> = binding_nodes
            .into_iter()
            .filter(|(binding, _)| self.referenced_globals.contains(binding))
            .flat_map(|(_, nodes)| nodes)
            .collect();

        // HMR (`hmr.md` §5): one `__hmr_expose` getter per non-excluded binding
        // that actually emitted, at the module tail (after the globals, before
        // main). The getter closes over the live binding, so capture at swap time
        // reads the current value; a payload getter reads the value cell
        // (`Signal` -> `[0].v`, `Shared` -> `.v`). Referenced by the binding's
        // emitted identifier, so `rename_for_scopes` rewrites both consistently.
        let mut hmr_expose: Vec<js::Node<'src>> = Vec::new();
        if self.hmr {
            for binding in &emitted_bindings {
                let Some(hmr_binding) = self.program.hmr_bindings.get(binding) else {
                    continue;
                };
                if hmr_binding.form == TransferForm::Excluded {
                    continue;
                }
                let key = hmr_binding.key.clone();
                let fingerprint = hmr_binding.fingerprint;
                let form = hmr_binding.form;
                let name = self.ng.name_for(*binding);
                let getter_body = match form {
                    TransferForm::Value => js::Node::Local(name),
                    TransferForm::SignalPayload => js::Node::Property(
                        Box::new(js::Node::PropertyIndex(
                            Box::new(js::Node::Local(name)),
                            Box::new(js::Node::Number("0".to_string(), None)),
                        )),
                        "v".to_string(),
                    ),
                    TransferForm::SharedPayload => {
                        js::Node::Property(Box::new(js::Node::Local(name)), "v".to_string())
                    }
                    TransferForm::Excluded => unreachable!("filtered above"),
                };
                let getter = js::Node::Closure(js::Closure {
                    parameters: Vec::new(),
                    body: vec![js::Node::Return(Box::new(getter_body))],
                    is_async: false,
                });
                hmr_expose.push(js::Node::Call(
                    Box::new(js::Node::Local("__hmr_expose".to_string())),
                    vec![
                        js::Node::String(Cow::Owned(key)),
                        js::Node::Number(fingerprint.to_string(), None),
                        getter,
                    ],
                ));
            }
        }

        // Never-silent (B55): refuse to ship a program that emitted a body-less
        // function as a call target. The emitted body is empty, so the call
        // yields `undefined` and the first use of the result is a runtime
        // `TypeError` — from a compile that reported nothing. Whatever failed to
        // resolve upstream, it must not leave here quietly.
        if let Some(&function_id) = self.bodyless_emissions.first() {
            let function = self.program.functions.get(&function_id);
            let name = function.map(|function| function.name).unwrap_or("?");
            let declaring_trait = self
                .program
                .traits
                .values()
                .find(|trait_| trait_.declarations.values().any(|id| *id == function_id))
                .map(|trait_| trait_.name);
            let source = match declaring_trait {
                Some(trait_name) => format!("`{trait_name}`'s requirement `{name}`"),
                None => format!("`{name}`"),
            };
            return Err(Error {
                trace: Vec::new(),
                note: None,
                span: function
                    .map(|function| function.name_span)
                    .unwrap_or_default(),
                msg: format!(
                    "internal: a call resolved to {source}, which has no body — \
                     emitting it would produce an empty function and a runtime \
                     `TypeError`. The receiver's type could not be resolved to a \
                     concrete implementation at this call; please report this \
                     program"
                ),
            });
        }

        // Never-silent (B68, affine-moves.md §9.4): refuse to ship a program with
        // a `drop(x)` whose argument type did not resolve. The rewrite decides
        // between a destructor and the data no-op purely by that type, so an
        // unresolved one silently picks the no-op — and a resource handed to the
        // sink is destroyed nowhere, from a compile that reported nothing. This
        // is the class `drop(f(x))` belonged to; it must not leave here quietly.
        if let Some(&call_id) = self.unresolved_drop_sinks.first() {
            return Err(Error {
                trace: Vec::new(),
                note: None,
                span: self
                    .program
                    .span_map
                    .get(&call_id)
                    .map(|span| **span)
                    .unwrap_or_default(),
                msg: "internal: the type of this `drop` argument could not be \
                      resolved, so the sink cannot tell a resource from data and \
                      would tear nothing down; please report this program"
                    .to_string(),
            });
        }

        let mut t_functions = self.required_functions.into_iter().collect::<Vec<_>>();
        t_functions.sort_by(|a, b| (a.0.0).cmp(&b.0.0));

        // The route-chunk partition (`bundle-splitting.md` §1): a function
        // reachable from exactly one route arm and nothing eager leaves the
        // entry bundle. Everything else — module bindings included, which is
        // what keeps B33's initialization order whole — stays. The buckets are
        // assembled into ONE node vector below and split apart only after the
        // rename, so a chunk's declarations are named exactly as they would
        // have been in the single-file build and can never collide with the
        // eager scope they read through the registry.
        let mut eager_functions: Vec<js::Node<'src>> = Vec::new();
        let mut chunked: Vec<Vec<js::Node<'src>>> = vec![Vec::new(); self.chunk_count];
        for (id, node) in t_functions {
            match self.chunk_members.get(&id) {
                Some(&chunk) => chunked[chunk].push(node),
                None => eager_functions.push(node),
            }
        }
        let t_functions = eager_functions.into_iter();

        // Monomorphized variants are plain function declarations too; ordering
        // among declarations is irrelevant since JS hoists them. They carry no
        // function id, so they are never chunked — a conservative eager
        // placement, correct at the cost of a chunk-exclusive instantiation
        // riding along.
        let t_instances = self.monomorphized.into_iter();

        let mut nodes = t_functions.collect::<Vec<_>>();
        // Each chunk's declarations occupy one contiguous run, recorded so the
        // rename below can see the whole program in one scope tree and the runs
        // can then be lifted out intact.
        let mut chunk_ranges: Vec<(usize, usize)> = Vec::with_capacity(chunked.len());
        for bucket in chunked {
            let start = nodes.len();
            nodes.extend(bucket);
            chunk_ranges.push((start, nodes.len()));
        }
        nodes.extend(t_instances);
        nodes.extend(t_global_variables);
        nodes.extend(hmr_expose);
        let mut main_body_start = nodes.len();
        nodes.extend(t_main_fn_body);

        // The boot preload (`bundle-splitting.md` §S3). The gate's chunk fetch
        // is issued when `swap_split` runs, and `swap_split` is the LAST call in
        // the view chain that mounts it — so today the whole shell subtree is
        // built before the boot route's chunk is even asked for. Planting
        // `__chunk_preload(<route signal>)` immediately before that statement
        // puts the fetch on the wire first and the shell build in its shadow.
        // Runs before the rename, so the planted names are renamed with every
        // other reference to the same binding.
        let mut preloads = 0usize;
        let planted = plant_boot_preloads(&mut nodes, &self.gate_call_names, &mut preloads);
        main_body_start += planted.iter().filter(|at| **at < main_body_start).count();

        // Host imports (`import { a, b } from "module";`) from `[extern]` calls,
        // then runtime helpers (`__scan`, ...) — both a prelude before the body.
        let imports = self
            .used_imports
            .iter()
            .map(|(module, symbols)| {
                let names = symbols.iter().cloned().collect::<Vec<_>>().join(", ");
                format!("import {{ {} }} from \"{}\";", names, module)
            })
            .collect::<Vec<_>>();
        let mut helpers = self.used_helpers.into_iter().collect::<Vec<_>>();
        // `__chunk_ready`/`__chunk_load` read the registry through
        // `__chunk_registry`, so the helper's own dependency travels with it —
        // including in a build that reached the gate without splitting, where
        // an empty registry makes every arm ready.
        if helpers
            .iter()
            .any(|helper| matches!(*helper, "__chunk_ready" | "__chunk_load"))
            && !helpers.contains(&"__chunk_registry")
        {
            helpers.push("__chunk_registry");
            helpers.sort();
        }

        // Re-allocate names over the JS scope tree so disjoint scopes share them
        // (readable: both sibling `value`s stay `value`; release: reuse short
        // names per function).
        rename_for_scopes(&self.ng, self.program, &mut nodes);
        // Lift the chunk runs out, last first so the earlier ranges stay valid.
        let mut chunks: Vec<Vec<js::Node<'src>>> = Vec::with_capacity(chunk_ranges.len());
        for (start, end) in chunk_ranges.iter().rev() {
            chunks.push(nodes.drain(start..end).collect());
            main_body_start -= end - start;
        }
        chunks.reverse();
        Ok(Assembled {
            imports,
            helpers,
            nodes,
            chunks,
            main_body_start,
        })
    }

    /// Push an expression-position result into `body`: normally an
    /// assignment into `result_name`, but a DIVERGING value (`return`,
    /// `break`, `continue` — a `Never`-typed match leg or if branch) is a
    /// statement of its own; `x = return e` is not JavaScript.
    fn push_result_or_divergence(
        &mut self,
        result_name: &str,
        value: js::Node<'src>,
        body: &mut Vec<js::Node<'src>>,
    ) {
        match value {
            value if value.is_divergent() => body.push(value),
            value => body.push(js::Node::Assignment(
                Box::new(js::Node::Local(result_name.to_string())),
                Box::new(value),
            )),
        }
    }

    fn walk_list(&mut self, list: &[Id]) -> Vec<js::Node<'src>> {
        let mut block = Vec::new();
        self.walk_entities(list, &mut block);
        block
    }

    fn walk_entities(&mut self, ids: &[Id], block: &mut Vec<js::Node<'src>>) {
        for id in ids {
            self.emit_statement(*id, block);
        }
    }

    /// Emit one statement into `block`, then close the `finally` of every
    /// resource temporary the statement lifted (C11) — the whole of what
    /// "a temporary is owned by its statement" means, in one place.
    ///
    /// A statement that lifts nothing takes exactly the path it always did,
    /// which is what keeps every resource-free program byte-identical.
    fn emit_statement(&mut self, id: Id, block: &mut Vec<js::Node<'src>>) {
        let mark = self.pending_temporaries.len();
        let base = block.len();
        if let Some(node) = self.walk_entity(id, block) {
            // A statement whose value is discarded and is `undefined` (e.g.
            // the trailing void of a block used as a statement) is a no-op.
            if !matches!(node, js::Node::Void) {
                block.push(node);
            }
        }
        self.close_temporaries(mark, base, block);
    }

    /// Give a resource temporary a name and remember where it landed.
    ///
    /// The `const` goes into the STATEMENT's node list rather than into the
    /// expression, because a `finally` needs a statement to sit after — and it
    /// goes there before the rest of the statement is emitted, so the value is
    /// acquired outside the `try` it will be destroyed by (`destruction.md`
    /// §7's mid-acquisition law, which a temporary obeys for the same reason a
    /// `let` does). A type that destroys nothing is not lifted at all.
    fn lift_resource_temporary(
        &mut self,
        type_id: TypeId,
        node: js::Node<'src>,
        block: &mut Vec<js::Node<'src>>,
    ) -> js::Node<'src> {
        if !self.type_drops_nontrivially(type_id) {
            return node;
        }
        let name = self.ng.next_name();
        block.push(js::Node::ConstVariable(js::Variable {
            name: name.clone(),
            value: Box::new(node),
        }));
        self.pending_temporaries.push(PendingTemporary {
            at: block.len() - 1,
            name: name.clone(),
            type_id,
        });
        js::Node::Local(name)
    }

    /// Close every temporary lifted since `mark`, innermost first, by wrapping
    /// everything after its `const` in a `try` whose `finally` destroys it.
    ///
    /// Popping in reverse is what gives §7.1's reverse construction order among
    /// the temporaries of one statement, and it nests them correctly: the last
    /// one born is the innermost `try` and the first one destroyed. Every exit
    /// — a throw mid-statement included (P8) — leaves through the `finally`.
    ///
    /// An entry whose `const` did not land in THIS list belongs to an emitter
    /// further out and is left for it.
    fn close_temporaries(&mut self, mark: usize, base: usize, block: &mut Vec<js::Node<'src>>) {
        if self.pending_temporaries.len() <= mark {
            return;
        }
        self.hoist_declaration_out_of(mark, base, block);
        while self.pending_temporaries.len() > mark {
            let Some(pending) = self.pending_temporaries.pop() else {
                return;
            };
            if pending.at < base || pending.at >= block.len() {
                continue;
            }
            let tail = block.split_off(pending.at + 1);
            let value = js::Node::Local(pending.name);
            match self.resource_drop_of(pending.type_id, value) {
                Some(drop) => block.push(js::Node::Try(tail, vec![drop])),
                None => block.extend(tail),
            }
        }
    }

    /// Wraps a call in `await` when its target is async (the implicit await), so
    /// the value flows as the resolved `T` rather than a promise.
    fn maybe_await(&self, target_id: Id, node: js::Node<'src>) -> js::Node<'src> {
        // `async_values`: a call through an `async || T`-typed parameter or
        // binding awaits like a direct async call (J2).
        if self.program.async_functions.contains(&target_id)
            || self.program.async_values.contains(&target_id)
        {
            js::Node::Await(Box::new(node))
        } else {
            node
        }
    }

    /// H9 (proposal/mut-parameters.md): the body-entry statements realizing
    /// `mut x = x'` — `x = __clone(x)` for an aggregate `mut` parameter
    /// (rule 1's copy), `x = [x]` for a scalar one some view roots in (the
    /// boxed cell its `(base, key)` views write through). Run before
    /// anything else in the body, including tuple-parameter destructures
    /// and resource teardown wrapping.
    fn parameter_entry_preludes(&mut self, parameter_ids: &[Id]) -> Vec<js::Node<'src>> {
        parameter_ids
            .iter()
            .filter_map(|parameter_id| {
                let name = self.ng.name_for(*parameter_id);
                if self.program.parameter_entry_clones.contains(parameter_id) {
                    self.used_helpers.insert("__clone");
                    Some(js::Node::Assignment(
                        Box::new(js::Node::Local(name.clone())),
                        Box::new(js::Node::Call(
                            Box::new(js::Node::Local("__clone".to_string())),
                            vec![js::Node::Local(name)],
                        )),
                    ))
                } else if self.program.boxed_locals.contains(parameter_id) {
                    Some(js::Node::Assignment(
                        Box::new(js::Node::Local(name.clone())),
                        Box::new(js::Node::Array(vec![js::Node::Local(name)])),
                    ))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Rule 1 (value semantics): wrap a value in `__clone(...)` when the analyzer
    /// marked this binding, assignment, or STORE as copying an aggregate place
    /// that would otherwise alias its source. `__clone` (not `structuredClone`)
    /// so a value holding closures can be copied.
    fn maybe_clone(&mut self, value_id: Id, node: js::Node<'src>) -> js::Node<'src> {
        if self.copy_applies(self.program.clone_sites.get(&value_id)) {
            self.used_helpers.insert("__clone");
            js::Node::Call(Box::new(js::Node::Local("__clone".to_string())), vec![node])
        } else {
            node
        }
    }

    /// Whether a recorded copy decision fires AT THIS EMISSION. A value whose
    /// type no instantiation can change always copies; a generic-dependent one
    /// is re-decided here, per monomorphization — `__clone` is identity on
    /// scalars and correct on aggregates, but on a RESOURCE it mints a second
    /// owner (divergent state, a second destructor run), and R11
    /// (`docs/spec/memory.md`) requires `Option::unwrap(self): T` to pass with
    /// no copies. A constraint this instance leaves unbound keeps the
    /// conservative copy.
    fn copy_applies(&self, decision: Option<&CopyDecision>) -> bool {
        match decision {
            None => false,
            Some(CopyDecision::Always) => true,
            Some(CopyDecision::UnlessResource(constraint_ids)) => {
                !constraint_ids.iter().any(|constraint_id| {
                    self.current_substitution
                        .get(constraint_id)
                        .map(|bound| self.resolve_type_id(*bound))
                        .is_some_and(|bound| self.program.resource_types.contains(&bound))
                })
            }
        }
    }

    /// §4's TRAIT path for a region: `s₁.and_then(|x₁| … sₙ.map(|xₙ| body))`
    /// — the user-`Lift` chain lowering, nested. Each split becomes a call on
    /// its own receiver with the rest of the region as a closure, so
    /// short-circuiting and laziness are the container's own `and_then`, not
    /// a tag branch; a hoisted eval binds where it sits, inside the enclosing
    /// continuation, which is what keeps effects in source order.
    ///
    /// Returns the region's value directly — unlike the std form there is no
    /// result temp to assign into.
    fn emit_lift_region_trait_steps(
        &mut self,
        steps: &[(Id, Id, bool)],
        body_id: Id,
        block: &mut Vec<js::Node<'src>>,
    ) -> js::Node<'src> {
        let Some(((step_id, binder_id, is_split), rest)) = steps.split_first() else {
            // The bottom: the body computes on the aliased elements. No
            // rewrap — the innermost `map` (or `and_then`, when the body
            // yields the container) does that.
            return self.walk_entity(body_id, block).unwrap_or(js::Node::Void);
        };
        let value = self.walk_entity(*step_id, block).unwrap_or(js::Node::Void);
        if !is_split {
            let step_name = self.ng.next_name();
            block.push(js::Node::ConstVariable(js::Variable {
                name: step_name.clone(),
                value: Box::new(value),
            }));
            self.is_bindings
                .insert(*binder_id, js::Node::Local(step_name));
            return self.emit_lift_region_trait_steps(rest, body_id, block);
        }
        // The analyzer records one dispatch per split, under its binder id.
        let Some(LiftDispatch::Trait {
            member_id,
            impl_subject,
            subject_type_id,
            own_generic_value,
        }) = self.program.lift_dispatch.get(binder_id).cloned()
        else {
            return js::Node::Void;
        };
        let dispatch = self.dispatch_to_member(
            member_id,
            impl_subject,
            subject_type_id,
            &[own_generic_value],
        );
        let Dispatch::Call(member_name, _) = dispatch else {
            // A Lift impl's members are ordinary vilan methods.
            return js::Node::Void;
        };
        let parameter = self.ng.next_name();
        self.is_bindings
            .insert(*binder_id, js::Node::Local(parameter.clone()));
        let mut closure_body = Vec::new();
        let continuation = self.emit_lift_region_trait_steps(rest, body_id, &mut closure_body);
        closure_body.push(js::Node::Return(Box::new(continuation)));
        js::Node::Call(
            Box::new(js::Node::Local(member_name)),
            vec![
                value,
                js::Node::Closure(js::Closure {
                    parameters: vec![js::Parameter { name: parameter }],
                    body: closure_body,
                    is_async: false,
                }),
            ],
        )
    }

    /// One step of an expression-lifting region, then the rest nested inside
    /// its good branch (a split) or plainly after it (an eval) —
    /// `expression-lifting.md` §4's std lowering. The recursion bottoms out by
    /// assigning the (map-wrapped or flattened) body to the result temp.
    fn emit_lift_region_steps(
        &mut self,
        region_id: Id,
        steps: &[(Id, Id, bool)],
        body_id: Id,
        result_name: &str,
        block: &mut Vec<js::Node<'src>>,
    ) {
        let Some(((step_id, binder_id, is_split), rest)) = steps.split_first() else {
            let value = self.walk_entity(body_id, block).unwrap_or(js::Node::Void);
            let wrapped = match self.program.lift_dispatch.get(&region_id) {
                Some(LiftDispatch::Std { flatten: true, .. }) | None => value,
                Some(LiftDispatch::Std {
                    flatten: false,
                    enum_id,
                }) => self.variant_value(*enum_id, 0, vec![value]),
                // A trait-path region never reaches the std emitter — the
                // `Expr::LiftRegion` arm routes it to
                // `emit_lift_region_trait_steps` before this runs.
                Some(LiftDispatch::Trait { .. } | LiftDispatch::TraitRegion) => unreachable!(),
            };
            block.push(js::Node::Assignment(
                Box::new(js::Node::Local(result_name.to_string())),
                Box::new(wrapped),
            ));
            return;
        };
        let value = self.walk_entity(*step_id, block).unwrap_or(js::Node::Void);
        let step_name = self.ng.next_name();
        block.push(js::Node::ConstVariable(js::Variable {
            name: step_name.clone(),
            value: Box::new(value),
        }));
        if !is_split {
            self.is_bindings
                .insert(*binder_id, js::Node::Local(step_name));
            self.emit_lift_region_steps(region_id, rest, body_id, result_name, block);
            return;
        }
        self.is_bindings.insert(
            *binder_id,
            js::Node::PropertyIndex(
                Box::new(js::Node::Local(step_name.clone())),
                Box::new(js::Node::Number("1".to_string(), None)),
            ),
        );
        let bad_body = vec![js::Node::Assignment(
            Box::new(js::Node::Local(result_name.to_string())),
            Box::new(js::Node::Local(step_name.clone())),
        )];
        let mut good_body = Vec::new();
        self.emit_lift_region_steps(region_id, rest, body_id, result_name, &mut good_body);
        block.push(js::Node::If(js::IfBranch::If(
            Box::new(js::Node::Binary(
                BinaryOp::Eq,
                Box::new(js::Node::PropertyIndex(
                    Box::new(js::Node::Local(step_name)),
                    Box::new(js::Node::Number("0".to_string(), None)),
                )),
                Box::new(js::Node::Number("1".to_string(), None)),
            )),
            bad_body,
            Some(Box::new(js::IfBranch::Else(good_body))),
        )));
    }

    /// The compound-assignment subscript hoist (B105). `x[i] op= v` desugars to
    /// `x[i] = x[i] op v`, and the two `x[i]`s are two independent walks of the
    /// same source place — so every effectful subscript in the target ran TWICE
    /// (`ys[bump()] += 1` emitted `__at_put(ys, bump(), __at(ys, bump()) + 1)`).
    /// Each is evaluated ONCE into a temp declared ahead of the statement, and
    /// both occurrences name the temp.
    ///
    /// The compound-ness comes from the analyzer's own record rather than from
    /// the shape: `ys[f()] = ys[g()] + 1` has the same shape and two genuinely
    /// different subscripts.
    fn hoist_compound_target(
        &mut self,
        target_id: Id,
        value_id: Id,
        block: &mut Vec<js::Node<'src>>,
    ) {
        // The re-read is the synthesized left operand of the desugared binary. An
        // OVERLOADED operator keeps this shape — its dispatch is a side map
        // (`binary_op_dispatch`), not a different expression — and a VIEW target
        // wraps both halves alike in R5's synthetic `Dereference`, under which the
        // analyzer's mark sits.
        let Some(&Expr::Binary(_, left_id, _)) = self.program.entity_map.get(&value_id) else {
            return;
        };
        let reread_id = match self.program.entity_map.get(&left_id) {
            Some(&Expr::Dereference(operand)) => operand,
            _ => left_id,
        };
        if !self.program.compound_rereads.contains(&reread_id) {
            return;
        }
        self.hoist_compound_place(target_id, reread_id, block);
    }

    /// The two place spines in lockstep — they are the same source place walked
    /// twice, so they match node for node, and pairing them is what lets the
    /// re-read name the write's temp.
    ///
    /// Descends to the ROOT first, so the temps land in source order (`grid[f()][g()]`
    /// evaluates `f()` before `g()`), which is also the order the un-hoisted
    /// emission ran them in — a JS call evaluates its arguments left to right, and
    /// the subscript is `__at_put`'s second argument while the read is its third.
    /// Nothing but the duplication changes.
    ///
    /// A **pure** subscript is deliberately left alone: evaluating `i` twice is
    /// not an observable difference, and a temp for it would churn goldens with
    /// nothing to show for it.
    fn hoist_compound_place(
        &mut self,
        target_id: Id,
        reread_id: Id,
        block: &mut Vec<js::Node<'src>>,
    ) {
        let matched = match (
            self.program.entity_map.get(&target_id),
            self.program.entity_map.get(&reread_id),
        ) {
            (
                Some(&Expr::Index(target_subject, target_index)),
                Some(&Expr::Index(reread_subject, reread_index)),
            ) => Some((
                target_subject,
                reread_subject,
                Some((target_index, reread_index)),
            )),
            (
                Some(&Expr::Field(target_subject, _, _)),
                Some(&Expr::Field(reread_subject, _, _)),
            )
            | (
                Some(&Expr::TupleIndex(target_subject, _, _)),
                Some(&Expr::TupleIndex(reread_subject, _, _)),
            )
            | (
                Some(&Expr::Dereference(target_subject)),
                Some(&Expr::Dereference(reread_subject)),
            ) => Some((target_subject, reread_subject, None)),
            _ => None,
        };
        let Some((target_subject, reread_subject, index)) = matched else {
            return;
        };
        self.hoist_compound_place(target_subject, reread_subject, block);
        if let Some((target_index, reread_index)) = index
            && self.expr_has_side_effects(target_index)
        {
            let value = self
                .walk_entity(target_index, block)
                .unwrap_or(js::Node::Void);
            let name = self.ng.next_name();
            block.push(js::Node::ConstVariable(js::Variable {
                name: name.clone(),
                value: Box::new(value),
            }));
            let temp = js::Node::Local(name);
            self.hoisted_values.insert(target_index, temp.clone());
            self.hoisted_values.insert(reread_index, temp);
        }
    }

    /// Whether an expression may have a side effect — a call, an `await`, or an
    /// assignment, or anything containing one. An unused `let` binding can be
    /// dropped only if its initializer is side-effect-free; a side-effecting one
    /// (e.g. a call that mutates through `&mut self`) must still run.
    fn expr_has_side_effects(&self, expr_id: Id) -> bool {
        match self.program.entity_map.get(&expr_id) {
            Some(Expr::Call(_)) | Some(Expr::Await(_)) | Some(Expr::Assignment(_, _)) => true,
            // An `async { .. }` block is an *invoked* async arrow — it starts
            // executing its body immediately, so it is effectful even when its
            // promise is discarded (`let _ = async { pump loop }`).
            Some(Expr::Async(_)) => true,
            Some(Expr::Binary(_, lhs, rhs)) => {
                self.expr_has_side_effects(*lhs) || self.expr_has_side_effects(*rhs)
            }
            Some(Expr::Unary(_, operand))
            | Some(Expr::Reference(operand, _))
            | Some(Expr::Dereference(operand)) => self.expr_has_side_effects(*operand),
            Some(Expr::Field(subject, _, _))
            | Some(Expr::TupleIndex(subject, _, _))
            | Some(Expr::ArrayLen(subject, _)) => self.expr_has_side_effects(*subject),
            // `[value; n]` evaluates its value expression once.
            Some(Expr::Repeat(value, _)) => self.expr_has_side_effects(*value),
            // A lift region runs its steps and (conditionally) its body.
            Some(Expr::LiftRegion(steps, body_id)) => {
                steps
                    .iter()
                    .any(|(step_id, _, _)| self.expr_has_side_effects(*step_id))
                    || self.expr_has_side_effects(*body_id)
            }
            // A checked subscript can panic, so an indexing expression is
            // effectful in itself: dropping it would drop its bounds check.
            Some(Expr::Index(_, _)) => true,
            Some(Expr::List(ids)) | Some(Expr::Tuple(ids)) => {
                ids.iter().any(|id| self.expr_has_side_effects(*id))
            }
            Some(Expr::StructInitializer(_, fields)) => {
                fields.values().any(|id| self.expr_has_side_effects(*id))
            }
            // A comprehension runs its body per element (`combine` subscribes each
            // source this way), so it inherits the body's side effects.
            Some(Expr::TupleComprehension(_, _, body_id)) => self.expr_has_side_effects(*body_id),
            _ => false,
        }
    }

    /// Whether a deref operand is a scalar `(base, key)` view — so `*operand`
    /// reads or writes `operand[0][operand[1]]`. True for a scalar-view binding /
    /// parameter, or `&place` of a scalar place directly.
    fn derefs_scalar_view(&self, operand: Id) -> bool {
        match self.program.entity_map.get(&operand) {
            Some(Expr::Reference(..)) => self.program.scalar_view_refs.contains(&operand),
            _ => self.emits_scalar_view_pair(operand),
        }
    }

    /// Whether an expression's own emission IS a scalar `(base, key)` pair — a
    /// scalar view binding / parameter, or a call returning a scalar view.
    ///
    /// Deliberately not the `&place` case, which `derefs_scalar_view` adds for
    /// its own question: at B108's return seam a `&place` leaf emits the PLACE
    /// READ, not a pair (the `Expr::Reference` arm decides that with the seam in
    /// hand), so asking "is this already a pair" must say no about it.
    fn emits_scalar_view_pair(&self, id: Id) -> bool {
        match self.program.entity_map.get(&id) {
            Some(Expr::Local(binding)) => {
                self.program.primitive_views.contains(binding)
                    || self.generic_ref_param_is_scalar(*binding)
            }
            // `*obj.slot()` — a `borrows` call returning a scalar view. A
            // scalar `Shared::write()` is one too: it lowers to the `(base,
            // key)` pair, and the analyzer cannot classify it (the pointee is
            // generic until this monomorphization).
            Some(Expr::Call(..)) => {
                self.program.scalar_view_calls.contains(&id) || self.call_is_scalar_shared_write(id)
            }
            _ => false,
        }
    }

    /// Read the value out of a scalar `(base, key)` view: `view[0][view[1]]`.
    /// A view produced by a CALL is bound to a temp first, so the two reads do
    /// not evaluate it twice; a plain binding or reference is cheap to repeat.
    fn emit_scalar_view_read(
        &mut self,
        view_id: Id,
        view: js::Node<'src>,
        block: &mut Vec<js::Node<'src>>,
    ) -> js::Node<'src> {
        let mut view = view;
        if matches!(self.program.entity_map.get(&view_id), Some(Expr::Call(..))) {
            let name = self.ng.next_name();
            block.push(js::Node::ConstVariable(js::Variable {
                name: name.clone(),
                value: Box::new(view),
            }));
            view = js::Node::Local(name);
        }
        let base = js::Node::PropertyIndex(
            Box::new(view.clone()),
            Box::new(js::Node::Number("0".to_string(), None)),
        );
        let key = js::Node::PropertyIndex(
            Box::new(view),
            Box::new(js::Node::Number("1".to_string(), None)),
        );
        js::Node::PropertyIndex(Box::new(base), Box::new(key))
    }

    /// Whether a call expression is a `Shared::write()` with a scalar pointee.
    fn call_is_scalar_shared_write(&self, call_expr_id: Id) -> bool {
        self.is_shared_write(call_expr_id) && self.shared_write_pointee_is_scalar(call_expr_id)
    }

    /// A `&`/`&mut` parameter whose declared pointee is a generic that resolves,
    /// at this monomorphization, to a scalar primitive. `compute_primitive_views`
    /// classifies a view by its pointee type, but a generic `&mut T` parameter's
    /// pointee is abstract there, so it cannot be added to `primitive_views`; the
    /// classification is re-made here against the concrete type, so a scalar
    /// pointee uses the `(base, key)` representation its (concrete) caller passed
    /// rather than the aggregate `__replace` path.
    fn generic_ref_param_is_scalar(&self, binding: Id) -> bool {
        self.program
            .parameters
            .get(&binding)
            .is_some_and(|parameter| {
                matches!(parameter.convention, Convention::Ref | Convention::RefMut)
                    && matches!(
                        self.program.type_id_to_type_map.get(&parameter.type_id),
                        Some(Type::Generic(_))
                    )
                    && self.resolves_to_scalar_view_pointee(parameter.type_id)
            })
    }

    /// Whether `type_id`, resolved under the active monomorphization substitution,
    /// is a scalar the **view** machinery lowers to a `(base, key)` pair — a scalar
    /// primitive (`SCALAR_PRIMITIVE_NAMES`), or `bool` (a numeric enum, so it is
    /// not in that struct set). The analyzer's `is_scalar_view_pointee` is the
    /// matching predicate; missing `bool` here routed a generic `&mut T` resolving
    /// to `bool` down the aggregate `Object.assign` path — a silent no-op write.
    fn resolves_to_scalar_view_pointee(&self, type_id: TypeId) -> bool {
        match self
            .program
            .type_id_to_type_map
            .get(&self.resolve_type_id(type_id))
        {
            Some(Type::Struct(id, _)) => self
                .program
                .structs
                .get(id)
                .is_some_and(|struct_| SCALAR_PRIMITIVE_NAMES.contains(&struct_.name)),
            Some(Type::Enum(id, _)) => Some(*id) == self.program.bool_enum_id,
            _ => false,
        }
    }

    /// Whether a bitwise/shift binary's operands are `u32` — the emission
    /// switch between JS's signed operators and the `>>>`-based unsigned forms.
    /// A concrete-`u32` verdict was recorded by the analyzer; a generic operand
    /// recorded its constraint, resolved here under the active
    /// monomorphization's substitution.
    fn binary_operands_are_u32(&self, binary_id: Id) -> bool {
        if self.program.bitwise_u32.contains(&binary_id) {
            return true;
        }
        let Some(constraint_id) = self.program.bitwise_generic_lhs.get(&binary_id) else {
            return false;
        };
        matches!(
            self.program
                .type_id_to_type_map
                .get(&self.resolve_constraint(*constraint_id)),
            Some(Type::Struct(id, _))
                if self.program.structs.get(id).is_some_and(|struct_| struct_.name == "u32")
        )
    }

    /// Resolve a generic's *constraint id* — as stored in `bitwise_generic_lhs` /
    /// `division_generic_lhs` — to its concrete type under the active
    /// monomorphization. The recorded id is the bound itself (`Trait(Div)`), NOT a
    /// `Generic(constraint)` wrapper, so `resolve_type_id` (which only unwraps a
    /// `Generic`) would leave it untouched; the substitution is keyed by exactly
    /// this id, so look it up directly, then resolve the binding in case it is
    /// itself generic (a composed instantiation). Missing `bool` was one drift
    /// bug; this untouched-constraint was another — a generic `i32`/`u32` division
    /// or bitwise op silently dropped its truncation / unsigned verdict.
    fn resolve_constraint(&self, constraint_id: TypeId) -> TypeId {
        self.current_substitution
            .get(&constraint_id)
            .map(|type_id| self.resolve_type_id(*type_id))
            .unwrap_or(constraint_id)
    }

    /// Whether a division's operands are an INTEGER primitive — the switch to
    /// the truncating `Math.trunc` emission (proposal/numeric-types.md §2).
    /// Concrete verdicts were recorded by the analyzer; a generic operand
    /// resolves under the active monomorphization's substitution.
    fn binary_operands_are_integer(&self, binary_id: Id) -> bool {
        const INTEGER_PRIMITIVES: &[&str] = &["i8", "u8", "i16", "u16", "i32", "u32", "i53", "u53"];
        if self.program.integer_division.contains(&binary_id) {
            return true;
        }
        let Some(constraint_id) = self.program.division_generic_lhs.get(&binary_id) else {
            return false;
        };
        matches!(
            self.program
                .type_id_to_type_map
                .get(&self.resolve_constraint(*constraint_id)),
            Some(Type::Struct(id, _))
                if self
                    .program
                    .structs
                    .get(id)
                    .is_some_and(|struct_| INTEGER_PRIMITIVES.contains(&struct_.name))
        )
    }

    /// Whether a local is boxed into a `[value]` cell at this monomorphization: a
    /// concrete scalar root (`boxed_locals`), or a generic-typed `&`-referenced
    /// root that resolves here to a scalar primitive (decided now, not in the
    /// analyzer, since its type was abstract there).
    fn local_is_boxed(&self, id: Id) -> bool {
        self.program.boxed_locals.contains(&id)
            || (self.program.generic_referenced_roots.contains(&id)
                && self
                    .program
                    .variables
                    .get(&id)
                    .is_some_and(|variable| self.resolves_to_scalar_view_pointee(variable.type_id)))
    }

    /// Whether `&[mut] operand` (the reference expr `ref_id`) lowers to a scalar
    /// `(base, key)` pair: a concrete scalar place (`scalar_view_refs`), or a
    /// reference whose place root is a generic local resolving here to a scalar.
    fn emits_scalar_view_ref(&self, ref_id: Id, operand: Id) -> bool {
        self.program.scalar_view_refs.contains(&ref_id)
            || self.place_root_local(operand).is_some_and(|root| {
                self.program.generic_referenced_roots.contains(&root)
                    && self.program.variables.get(&root).is_some_and(|variable| {
                        self.resolves_to_scalar_view_pointee(variable.type_id)
                    })
            })
    }

    /// The local a place expression bottoms out in (mirrors the analyzer's
    /// `place_root`) — for deciding a generic place's view representation.
    fn place_root_local(&self, expr_id: Id) -> Option<Id> {
        match self.program.entity_map.get(&expr_id)? {
            Expr::Local(binding) => Some(*binding),
            Expr::Field(subject, _, _) | Expr::TupleIndex(subject, _, _) => {
                self.place_root_local(*subject)
            }
            Expr::Index(subject, _) => self.place_root_local(*subject),
            Expr::Dereference(operand) => self.place_root_local(*operand),
            _ => None,
        }
    }

    /// Whether this intrinsic call is a `Shared::write()` whose pointee resolves
    /// (under the active monomorphization) to a scalar — the case that lowers to
    /// a `(base, key)` pair rather than to the bare `cell.v` slot access.
    fn emits_scalar_shared_write(&self, intrinsic: Intrinsic, call_expr_id: Id) -> bool {
        matches!(intrinsic, Intrinsic::SharedWrite)
            && self.shared_write_pointee_is_scalar(call_expr_id)
    }

    /// Whether a `Shared::write()` call's pointee resolves to a scalar under the
    /// active monomorphization. A call expression carries no recorded type of its
    /// own, so this reads the extern's declared return type (`&mut T`, erased to
    /// the generic `T`) and resolves it through the current substitution — the
    /// same channel `generic_ref_param_is_scalar` uses for a `&mut T` parameter.
    fn shared_write_pointee_is_scalar(&self, call_expr_id: Id) -> bool {
        let Some(Expr::Call(call_id)) = self.program.entity_map.get(&call_expr_id) else {
            return false;
        };
        let Some(function_call) = self.program.function_calls.get(call_id) else {
            return false;
        };
        let Some(Expr::Local(callee_id)) = self.program.entity_map.get(&function_call.subject_id)
        else {
            return false;
        };
        if !self.program.external_functions.contains_key(callee_id) {
            return false;
        }
        // The pointee is the RECEIVER's `Shared<T>` type argument. The extern's
        // own declared `&mut T` names the impl binder, which the caller's
        // monomorphization substitution does not bind — the receiver's resolved
        // type does carry the concrete argument.
        let Some(receiver_type_id) = function_call
            .argument_ids
            .first()
            .and_then(|receiver_id| self.expr_type_id(*receiver_id))
        else {
            return false;
        };
        let Some(Type::Struct(_, arguments)) = self
            .program
            .type_id_to_type_map
            .get(&self.resolve_type_id(receiver_type_id))
        else {
            return false;
        };
        arguments
            .first()
            .is_some_and(|pointee| self.resolves_to_scalar_view_pointee(*pointee))
    }

    /// The cell's `v` slot for a `Shared::write()` call — the form the pair
    /// lowering above is built from, and the one the assign-through path wants
    /// back (`cell.write() = x` is `cell.v = x` for every pointee).
    fn shared_write_slot(
        &mut self,
        call_expr_id: Id,
        block: &mut Vec<js::Node<'src>>,
    ) -> Option<js::Node<'src>> {
        let Some(Expr::Call(call_id)) = self.program.entity_map.get(&call_expr_id) else {
            return None;
        };
        let receiver_id = *self
            .program
            .function_calls
            .get(call_id)?
            .argument_ids
            .first()?;
        let receiver = self.walk_entity(receiver_id, block)?;
        Some(js::Node::Property(Box::new(receiver), "v".to_string()))
    }

    /// Whether an expression is a `Shared::write()` call — a single-slot view of
    /// the cell's `v` slot. Writing through it rebinds the slot (`cell.v = x`),
    /// distinct from both the `(base, key)` and aggregate-`__replace` views.
    fn is_shared_write(&self, operand: Id) -> bool {
        let Some(Expr::Call(call_id)) = self.program.entity_map.get(&operand) else {
            return false;
        };
        let Some(function_call) = self.program.function_calls.get(call_id) else {
            return false;
        };
        let Some(Expr::Local(function_id)) = self.program.entity_map.get(&function_call.subject_id)
        else {
            return false;
        };
        matches!(
            self.program.intrinsics.get(function_id),
            Some(Intrinsic::SharedWrite)
        )
    }

    fn walk_entity(&mut self, id: Id, block: &mut Vec<js::Node<'src>>) -> Option<js::Node<'src>> {
        let node = self.walk_entity_inner(id, block)?;
        // C11 (`temporary-drop.md`): a resource value that is neither bound nor
        // moved is owned by its STATEMENT. It has no name of its own, so it is
        // given one here — a minted `const` at the statement's position — and
        // the emitter closes a `finally` around the rest of the statement. This
        // runs before the copy seams below on purpose: a resource never copies
        // (R1), so it can take neither, and a lifted temporary must not be
        // wrapped by them either.
        if let Some(&type_id) = self.program.resource_temporaries.get(&id) {
            return Some(self.lift_resource_temporary(type_id, node, block));
        }
        // B108: the same seam for a leaf whose runtime representation is a
        // scalar `(base, key)` view — a `&mut i32` parameter forwarded straight
        // out, a scalar `borrows` call, a generic `&T` at a scalar
        // instantiation. `__clone` cannot collapse a pair (and the type filter
        // rightly kept scalars out of `return_clone_sites`); a scalar's copy IS
        // its read (B81), so the crossing emits the read.
        if self.program.return_view_reads.contains(&id) && self.emits_scalar_view_pair(id) {
            return Some(self.emit_scalar_view_read(id, node, block));
        }
        // Rule 1's return clause: a tail/`ret` leaf that hands back a place the
        // body does not own copies HERE, where the place itself is emitted, so
        // that a tail `if`/`match` copies only in the arms that owe it. Keyed by
        // a map of its own, so a leaf that is also a `clone_sites` entry (it
        // never is — one expression, one syntactic position) could not be
        // wrapped twice.
        if self.copy_applies(self.program.return_clone_sites.get(&id)) {
            self.used_helpers.insert("__clone");
            return Some(js::Node::Call(
                Box::new(js::Node::Local("__clone".to_string())),
                vec![node],
            ));
        }
        Some(node)
    }

    fn walk_entity_inner(
        &mut self,
        id: Id,
        block: &mut Vec<js::Node<'src>>,
    ) -> Option<js::Node<'src>> {
        // A `const` expression's computed value replaces the whole subtree —
        // in-place serialization (const-eval.md §1). The const world itself is
        // lowered with the results map still empty for the expression being
        // evaluated, so this arm never short-circuits an evaluation.
        if let Some(value) = self.program.const_results.get(&id) {
            return Some(const_value_to_js(value));
        }
        // An expression already evaluated into a temp (B105) names the temp: the
        // whole point is that the second occurrence does not run it again.
        if let Some(hoisted) = self.hoisted_values.get(&id) {
            return Some(hoisted.clone());
        }
        let entity = self.program.entity_map.get(&id).unwrap();

        Some(match entity {
            Expr::Error => unreachable!(),
            // A macro-name marker: never a value (the analyzer rejects value
            // uses); reached only as an inert statement — emit nothing.
            Expr::Macro => js::Node::Void,
            Expr::TupleComprehension(binder_id, source_id, body_id) => {
                // A flat tuple is a JS array, so the comprehension lowers to a
                // runtime `source.map((x) => body)` — arity-independent, no
                // monomorphization needed. The binder is the closure parameter.
                let (binder_id, source_id, body_id) = (*binder_id, *source_id, *body_id);
                let source = self.walk_entity(source_id, block).unwrap_or(js::Node::Void);
                let parameter_name = self.ng.name_for(binder_id);
                let mut body = Vec::new();
                if let Some(value) = self.walk_entity(body_id, &mut body) {
                    body.push(js::Node::Return(Box::new(value)));
                }
                let closure = js::Node::Closure(js::Closure {
                    parameters: vec![js::Parameter {
                        name: parameter_name,
                    }],
                    body,
                    is_async: false,
                });
                js::Node::Call(
                    Box::new(js::Node::Property(Box::new(source), "map".to_string())),
                    vec![closure],
                )
            }
            Expr::Void => js::Node::Void,
            Expr::Null => js::Node::Null,
            Expr::Bool(x) => js::Node::Bool(*x),
            Expr::Number(whole, fraction, suffix) => {
                // `n`-suffixed literals are JS BigInts (`5n`); other suffixes
                // only affect typing and are dropped in the output.
                let whole = if matches!(*suffix, Some("n")) {
                    format!("{whole}n")
                } else {
                    whole.to_string()
                };
                js::Node::Number(whole, fraction.map(|x| x.to_string()))
            }
            Expr::String(x) => js::Node::String(unescape_string(x)),
            // A triple-quoted string: RAW (no escape interpretation), trimmed
            // to its content; the analyzer already validated, so an error here
            // is unreachable and degrades to "".
            Expr::MultilineString(x) => js::Node::String(std::borrow::Cow::Owned(
                crate::util::trim_multiline_string(x).unwrap_or_default(),
            )),
            Expr::Struct(_) => {
                return None;
            }
            Expr::Enum(_) => {
                return None;
            }
            Expr::Trait(_) => {
                return None;
            }
            Expr::Impl(_) => {
                return None;
            }
            Expr::ExternalFunction(_) => {
                return None;
            }
            Expr::Generic(_) => {
                return None;
            }
            // A `fun` DECLARATION, like the `struct`/`enum`/`trait`/`impl`
            // declarations above it — including one nested in another function's
            // body, which is the only way a `fun` reaches this walk. Emission is
            // demand-driven from the roots: a call to it (or a reference to it
            // as a value, through `Expr::Local` below) emits it once, at module
            // level, keyed on its id. Emitting the body here too produced the
            // same function TWICE (B71) — nested and hoisted, identical bodies,
            // the inner shadowing the outer. A `fun` captures nothing, so where
            // it is written is a scoping question the name generator already
            // answers and not an emission one.
            Expr::Function(_) => {
                return None;
            }
            // An enum value is an array whose first element identifies the
            // variant; a bare (data-less) variant is just `[index]`. `bool` is
            // the exception — it lowers to a native boolean.
            Expr::EnumVariant(enum_id, variant_index) => {
                self.variant_value(*enum_id, *variant_index, Vec::new())
            }
            Expr::Local(id) => {
                self.referenced_globals.insert(*id);
                // A capture from an `is` test aliases the subject's payload slot.
                if let Some(accessor) = self.is_bindings.get(id) {
                    let accessor = accessor.clone();
                    if let Some(reads) = self.is_binding_reads.as_mut() {
                        reads.insert(*id);
                    }
                    return Some(accessor);
                }
                // A reference to a data-less variant (e.g. `None`) is the
                // variant value itself, not a named binding.
                if let Some(Expr::EnumVariant(enum_id, variant_index)) =
                    self.program.entity_map.get(id)
                {
                    return Some(self.variant_value(*enum_id, *variant_index, Vec::new()));
                }
                // A reference to a named function as a VALUE (backlog B20,
                // proposal/fn-coercion.md): the function object itself is the
                // value — ensure it's emitted and name it, exactly as a call
                // subject would.
                if let Some(Expr::Function(function_id)) = self.program.entity_map.get(id) {
                    let function_id = *function_id;
                    self.ensure_function_emitted(function_id);
                    return Some(js::Node::Local(self.ng.name_for(function_id)));
                }
                // A boxed scalar local reads through its cell's slot 0.
                if self.local_is_boxed(*id) {
                    return Some(js::Node::PropertyIndex(
                        Box::new(js::Node::Local(self.ng.name_for(*id))),
                        Box::new(js::Node::Number("0".to_string(), None)),
                    ));
                }
                js::Node::Local(self.ng.name_for(*id))
            }
            Expr::Field(subject_id, _struct_id, field_index) => {
                let subject = self
                    .walk_entity(*subject_id, block)
                    .unwrap_or(js::Node::Void);
                js::Node::PropertyIndex(
                    Box::new(subject),
                    Box::new(js::Node::Number(field_index.to_string(), None)),
                )
            }
            // `pair.0` — tuples store flat: a width-1 element reads its slot,
            // a tuple-typed element reslices its region (like destructuring).
            Expr::TupleIndex(subject_id, offset, width) => {
                let subject = self
                    .walk_entity(*subject_id, block)
                    .unwrap_or(js::Node::Void);
                if *width == 1 {
                    js::Node::PropertyIndex(
                        Box::new(subject),
                        Box::new(js::Node::Number(offset.to_string(), None)),
                    )
                } else {
                    js::Node::Call(
                        Box::new(js::Node::Property(Box::new(subject), "slice".to_string())),
                        vec![
                            js::Node::Number(offset.to_string(), None),
                            js::Node::Number((offset + width).to_string(), None),
                        ],
                    )
                }
            }
            // `list[i]` — the checked read (`__at`): an out-of-bounds subscript
            // panics; `get` is the total, Option-returning form.
            Expr::Index(subject_id, index_id) => {
                let subject = self
                    .walk_entity(*subject_id, block)
                    .unwrap_or(js::Node::Void);
                let index = self.walk_entity(*index_id, block).unwrap_or(js::Node::Void);
                self.used_helpers.insert("__at");
                js::Node::Call(
                    Box::new(js::Node::Local("__at".to_string())),
                    vec![subject, index],
                )
            }
            Expr::Call(id) => {
                let function_call = self.program.function_calls.get(id).unwrap().clone();
                let args = function_call
                    .argument_ids
                    .iter()
                    .filter_map(|arg| {
                        // An argument to an `own` parameter is copied (marked in
                        // `clone_sites`), like a binding copy.
                        self.walk_entity(*arg, block)
                            .map(|node| self.maybe_clone(*arg, node))
                    })
                    .collect::<Vec<_>>();

                // `T::member()` inside a monomorphized body: dispatch directly
                // to the concrete type's member that `T` is bound to here.
                if let Some(GenericDispatch::OnConstraint(constraint_id, member_name)) = self
                    .program
                    .generic_dispatch
                    .get(&function_call.subject_id)
                    .copied()
                {
                    if let Some(&concrete_type_id) = self.current_substitution.get(&constraint_id) {
                        let own_values = self
                            .program
                            .own_generic_call_bindings
                            .get(id)
                            .cloned()
                            .unwrap_or_default();
                        // A static's trait was recorded against the ACCESSOR
                        // (the call id wasn't known at resolution).
                        let preferred = self
                            .program
                            .bound_dispatch_traits
                            .get(id)
                            .or_else(|| {
                                self.program
                                    .bound_dispatch_traits
                                    .get(&function_call.subject_id)
                            })
                            .cloned();
                        if let Some(dispatch) = self.resolve_dispatch_with(
                            concrete_type_id,
                            member_name,
                            &own_values,
                            preferred,
                        ) {
                            return Some(self.emit_dispatch(dispatch, args, Some(*id)));
                        }
                    }
                }

                // `a.member()` where `a`'s type is a trait-bounded generic `T`:
                // dispatch to the member of the concrete type `T` is bound to at this
                // monomorphization (the instance analogue of the `T::member()` path
                // above). The trait member may be abstract (bodyless), so this can't
                // fall through to a normal emit.
                if let Some(GenericDispatch::OnConstraint(constraint_id, member_name)) =
                    self.program.generic_dispatch.get(id).copied()
                {
                    if let Some(&concrete_type_id) = self.current_substitution.get(&constraint_id) {
                        let own_values = self
                            .program
                            .own_generic_call_bindings
                            .get(id)
                            .cloned()
                            .unwrap_or_default();
                        let preferred = self.program.bound_dispatch_traits.get(id).cloned();
                        if let Some(dispatch) = self.resolve_dispatch_with(
                            concrete_type_id,
                            member_name,
                            &own_values,
                            preferred,
                        ) {
                            return Some(self.emit_dispatch(dispatch, args, Some(*id)));
                        }
                    }
                }

                // A trait method re-dispatched to the receiver's concrete type: an
                // inherited default called on a concrete value (Gap E, with the
                // type recorded), or a `self`-call inside a default body (no type,
                // dispatched on the type the default is being specialized for).
                if let Some(GenericDispatch::OnType(concrete_type, member_name)) =
                    self.program.generic_dispatch.get(id).copied()
                {
                    if let Some(type_id) = concrete_type.or(self.current_self_type) {
                        // A `Trait::member(receiver, ..)` call names the trait to
                        // dispatch on (B57 §3.1) — without it, two traits whose
                        // DEFAULTS share a name both resolve to whichever the
                        // by-name lookup reaches first.
                        let preferred = self.program.bound_dispatch_traits.get(id).cloned();
                        if let Some(dispatch) =
                            self.resolve_dispatch_with(type_id, member_name, &[], preferred)
                        {
                            return Some(self.emit_dispatch(dispatch, args, Some(*id)));
                        }
                    }
                }

                let subject = self
                    .program
                    .entity_map
                    .get(&function_call.subject_id)
                    .unwrap();
                match subject {
                    Expr::Local(target_id) => {
                        let target_id = *target_id;
                        // Calling a named binding REFERENCES it, exactly as
                        // reading it does (the `Expr::Local` value arm above):
                        // the tree-shake keeps a module-level binding only when
                        // `referenced_globals` holds it, and a call site emitted
                        // as `name(..)` needs the `name` to survive. This arm
                        // reads the subject directly instead of walking it, so
                        // it must record the reference itself — without this, a
                        // module closure reached ONLY by call (`f()`) is dropped
                        // while its call site remains (B31). Non-binding targets
                        // (intrinsics, externs, functions, variants) are filtered
                        // out at the consumption sites, so this is harmless for
                        // them, matching the value arm's unconditional insert.
                        self.referenced_globals.insert(target_id);
                        // A split build's route match: `swap` becomes
                        // `swap_split`, which waits for the arm's chunk before
                        // letting the view advance (`bundle-splitting.md` §2).
                        // Same shape, so the call's own type binding carries
                        // over by position; every argument is emitted unchanged.
                        if let Some((gate_target, preload)) = self.split_gate_target(*id, target_id)
                        {
                            let call_substitution = self.call_substitution(
                                *id,
                                target_id,
                                &function_call.generic_argument_ids,
                            );
                            let substitution = call_substitution
                                .as_ref()
                                .map(|substitution| {
                                    self.rebind_by_position(target_id, gate_target, substitution)
                                })
                                .unwrap_or_default();
                            // The boot preload takes the same route type, and
                            // `plant_boot_preloads` needs its emitted name — so
                            // it is instantiated here, beside the gate call it
                            // will be planted in front of.
                            let preload_substitution = call_substitution
                                .as_ref()
                                .map(|substitution| {
                                    self.rebind_by_position(target_id, preload, substitution)
                                })
                                .unwrap_or_default();
                            let preload_name = self.emit_instance(preload, &preload_substitution);
                            let name = self.emit_instance(gate_target, &substitution);
                            self.gate_call_names.insert(name.clone(), preload_name);
                            return Some(js::Node::Call(Box::new(js::Node::Local(name)), args));
                        }
                        // An external std intrinsic lowers to native JS or a
                        // runtime helper.
                        if let Some(intrinsic) = self.program.intrinsics.get(&target_id).copied() {
                            return Some(self.emit_intrinsic(intrinsic, args, Some(*id)));
                        }
                        // An `[extern]`-bound external lowers to its host (JS)
                        // import/call, method, or property access.
                        if let Some(binding) = self
                            .program
                            .external_functions
                            .get(&target_id)
                            .and_then(|external| external.extern_binding.clone())
                        {
                            let call = self.emit_extern(target_id, binding, args);
                            return Some(self.maybe_await(target_id, call));
                        }
                        // A variant constructor call builds the enum value
                        // directly: `[variant_index, ...data]` (or a native
                        // boolean for `bool`).
                        if let Some(Expr::EnumVariant(enum_id, variant_index)) =
                            self.program.entity_map.get(&target_id)
                        {
                            return Some(self.variant_value(*enum_id, *variant_index, args));
                        }
                        if target_id == self.print_fn_id {
                            return Some(js::Node::Call(
                                Box::new(js::Node::Property(
                                    Box::new(js::Node::Local("console".to_string())),
                                    "log".to_string(),
                                )),
                                args,
                            ));
                        }
                        // `List::new()` builds an empty JS array.
                        if Some(target_id) == self.list_new_fn_id {
                            return Some(js::Node::Array(Vec::new()));
                        }
                        // `list.push(x)` lowers to the native array method; the
                        // receiver is the method call's first (`self`) argument.
                        if Some(target_id) == self.list_push_fn_id {
                            let mut arguments = args.into_iter();
                            let receiver = arguments.next().unwrap_or(js::Node::Void);
                            return Some(js::Node::Call(
                                Box::new(js::Node::Property(
                                    Box::new(receiver),
                                    "push".to_string(),
                                )),
                                arguments.collect(),
                            ));
                        }
                        // `panic(msg)` lowers to a thrown error. It's wrapped in
                        // an immediately-invoked arrow so it stays valid in
                        // expression position (e.g. a match leg).
                        if Some(target_id) == self.panic_fn_id {
                            let message = args.into_iter().next().unwrap_or(js::Node::Void);
                            return Some(js::Node::Call(
                                Box::new(js::Node::Closure(js::Closure {
                                    parameters: Vec::new(),
                                    body: vec![js::Node::Throw(Box::new(message))],
                                    is_async: false,
                                })),
                                Vec::new(),
                            ));
                        }
                        // `drop(x)` — the std early-teardown sink (destruction.md
                        // §6), rewritten by the concrete argument type at THIS
                        // (possibly monomorphized) site: a resource lowers to its
                        // `__drop` helper (destructor, then fields, reverse order);
                        // data is a no-op that still evaluates the argument for its
                        // effects. Erasure forces the rewrite here — the generic
                        // sink body cannot drop instantiation-conditionally.
                        // `x.value()` on a backed enum IS `x` (backed-enums.md
                        // §3.8): the receiver already holds the backing value
                        // at runtime, so the conversion is the identity and
                        // emits nothing. The generated body — a `match` over
                        // the variants — stays the definition this fold has to
                        // agree with; folded away, it has no callers left and
                        // the tree-shake drops it.
                        if self.program.backed_value_members.contains(&target_id) {
                            return Some(args.into_iter().next().unwrap_or(js::Node::Void));
                        }
                        if Some(target_id) == self.drop_fn_id {
                            let arg_node = args.into_iter().next().unwrap_or(js::Node::Void);
                            let argument_id = function_call.argument_ids.first().copied();
                            let argument_type = argument_id.map(|argument_id| {
                                self.drop_argument_type_id(argument_id)
                                    .map(|type_id| self.resolve_type_id(type_id))
                            });
                            match argument_type {
                                // A resource: its `__drop` helper. Data (no glue
                                // for the type): the no-op consume, which still
                                // evaluates the argument for its effects.
                                Some(Some(type_id)) => match self.ensure_drop_helper(type_id) {
                                    Some(helper) => {
                                        let drop = js::Node::Call(
                                            Box::new(js::Node::Local(helper)),
                                            vec![arg_node.clone()],
                                        );
                                        // B150: the sink no longer takes the
                                        // binding's teardown away with it — the
                                        // scope's `finally` stays, so a panic
                                        // before this line still releases the
                                        // resource. What keeps the fall-through
                                        // path destroying exactly once is the
                                        // other half of the pair: the slot is
                                        // left EMPTY, which is the moved-out
                                        // state `Option.take` writes for the
                                        // same reason, and the `finally` tests
                                        // it. `arg_node` is the very node the
                                        // teardown reads (a local, or a leg's
                                        // payload accessor), so the two cannot
                                        // name different storage.
                                        let empties_slot = argument_id
                                            .and_then(|argument_id| {
                                                self.place_binding_of(argument_id)
                                            })
                                            .is_some_and(|binding| {
                                                self.slot_is_emptied_early(binding)
                                            });
                                        if empties_slot {
                                            block.push(drop);
                                            return Some(js::Node::Assignment(
                                                Box::new(arg_node),
                                                Box::new(js::Node::Null),
                                            ));
                                        }
                                        return Some(drop);
                                    }
                                    None => return Some(arg_node),
                                },
                                // Never-silent (B55's pattern, applied to the sink
                                // by B68): an argument whose type did not resolve
                                // cannot be told apart from data here, so emitting
                                // the bare argument is a leak from a clean compile
                                // — exactly how `drop(f(x))` destroyed nothing for
                                // as long as it did. Report it instead.
                                Some(None) => {
                                    self.unresolved_drop_sinks.push(*id);
                                    return Some(arg_node);
                                }
                                // `drop()` with no argument: an arity error the
                                // analyzer already reported.
                                None => return Some(arg_node),
                            }
                        }
                        // A call to a generic function/method is compiled to a
                        // specialized instance chosen by its concrete type arguments
                        // — no runtime dispatch. The binding comes from whichever
                        // channel carries it (see `call_substitution`); all feed the
                        // one `emit_instance` path. A non-generic call (no binding)
                        // is emitted as a plain function.
                        // Adaptation (async-polymorphism.md A.1): the
                        // analysis routed this call to an adapted instance
                        // (async closure arguments) — emit that instance,
                        // and await it when the instance is async.
                        let adapted_bits = self
                            .current_instance
                            .as_ref()
                            .and_then(|instance| instance.callee_bits.get(id))
                            .cloned()
                            .unwrap_or_default();
                        let name = match self.call_substitution(
                            *id,
                            target_id,
                            &function_call.generic_argument_ids,
                        ) {
                            Some(substitution) => self.emit_instance_with_bits(
                                target_id,
                                &substitution,
                                &adapted_bits,
                            ),
                            None if adapted_bits.is_empty() => {
                                self.ensure_function_emitted(target_id);
                                self.ng.name_for(target_id)
                            }
                            None => {
                                let inherited = self.inherited_substitution(target_id);
                                self.emit_instance_with_bits(target_id, &inherited, &adapted_bits)
                            }
                        };
                        let call = js::Node::Call(Box::new(js::Node::Local(name)), args);
                        if self
                            .current_instance
                            .as_ref()
                            .is_some_and(|instance| instance.awaited_calls.contains(id))
                        {
                            js::Node::Await(Box::new(call))
                        } else {
                            self.maybe_await(target_id, call)
                        }
                    }
                    _ => {
                        let t_subject = self.walk_entity(function_call.subject_id, block).unwrap();
                        let call = js::Node::Call(Box::new(t_subject), args);
                        // A call through an async field or an async-returning
                        // call awaits (J2), as does a call through an ADAPTED
                        // parameter or an instance-async held closure
                        // (async-polymorphism.md A.1).
                        if self.program.awaited_calls.contains(id)
                            || self
                                .current_instance
                                .as_ref()
                                .is_some_and(|instance| instance.awaited_calls.contains(id))
                        {
                            js::Node::Await(Box::new(call))
                        } else {
                            call
                        }
                    }
                }
            }
            Expr::Closure(closure_id) => {
                let closure = self.program.closures.get(closure_id).unwrap();
                let parameters = closure
                    .parameters
                    .iter()
                    .map(|parameter_id| js::Parameter {
                        name: self.ng.name_for(*parameter_id),
                    })
                    .collect::<Vec<_>>();
                let mark = self.pending_temporaries.len();
                let mut body = self.parameter_entry_preludes(&closure.parameters);
                // Tuple-parameter destructures run before the body proper.
                let parameter_destructures = closure.parameter_destructures.clone();
                for destructure_id in parameter_destructures {
                    self.walk_entity(destructure_id, &mut body);
                }
                let value = self.walk_entity(closure.return_, &mut body);
                if let Some(value) = value {
                    // Same seam as a function body's tail (B152): a divergent
                    // tail is the statement, never a value to `return`.
                    if value.is_divergent() {
                        body.push(value);
                    } else {
                        body.push(js::Node::Return(Box::new(value)));
                    }
                }
                self.seal_pending_temporaries(mark, &mut body);
                js::Node::Closure(js::Closure {
                    parameters,
                    body,
                    is_async: self.program.async_functions.contains(closure_id)
                        || self
                            .current_instance
                            .as_ref()
                            .is_some_and(|instance| instance.async_closures.contains(closure_id)),
                })
            }
            // `async <body>` — the spawn: `__task(async () => { <body> },
            // "<origin>")` constructs the `Task` handle
            // (async-polymorphism.md Part B). `__task` invokes the body
            // closure immediately (eager — it runs to its first suspension at
            // the spawn expression, as before), attaches the absorption
            // handler so the rejection can never surface as a host unhandled
            // rejection, and records the spawn origin for the
            // unobserved-failure report. A spawn the context pass connected
            // to an ambient nursery passes it as the third argument (read
            // from the threaded parameter — unwrapped when the holder is
            // safe-flavored and carries `Option<Nursery>`), registering the
            // task for the nursery's join.
            Expr::Async(closure_id) => {
                self.used_helpers.insert("__task");
                let closure = self
                    .walk_entity(*closure_id, block)
                    .unwrap_or(js::Node::Void);
                let mut arguments = vec![
                    closure,
                    js::Node::String(Cow::Borrowed(self.current_origin.unwrap_or("top level"))),
                ];
                if let Some(&(source_entity, is_option)) =
                    self.program.spawn_nursery_sources.get(&id)
                {
                    let source = self
                        .walk_entity(source_entity, block)
                        .unwrap_or(js::Node::Void);
                    arguments.push(if is_option {
                        self.used_helpers.insert("__nursery_of");
                        js::Node::Call(
                            Box::new(js::Node::Local("__nursery_of".to_string())),
                            vec![source],
                        )
                    } else {
                        source
                    });
                }
                js::Node::Call(Box::new(js::Node::Local("__task".to_string())), arguments)
            }
            // `await <inner>`.
            Expr::Await(inner) => {
                let inner = self.walk_entity(*inner, block).unwrap_or(js::Node::Void);
                js::Node::Await(Box::new(inner))
            }
            // A bare `ret` returns void — emitted as `return;` (the emitter
            // special-cases a `Void` child).
            Expr::FunctionReturn(value) => js::Node::Return(Box::new(
                value
                    .and_then(|value| self.walk_entity(value, block))
                    .unwrap_or(js::Node::Void),
            )),
            // `a?.b.c` (proposal/try-and-lift.md §3–4): evaluate the subject
            // once; a bad tag short-circuits AS-IS; otherwise the continuation
            // runs with the binder aliased to the element, and the result is
            // wrapped back (map) or passed through (flatten).
            Expr::Lift(subject_id, binder_id, continuation_id) => {
                let subject = self
                    .walk_entity(*subject_id, block)
                    .unwrap_or(js::Node::Void);
                // A user `Lift` container: `map_instance(subject, (x) => cont)`
                // — the continuation becomes a closure whose parameter aliases
                // the binder (proposal/try-and-lift.md §4's trait path).
                if let Some(LiftDispatch::Trait {
                    member_id,
                    impl_subject,
                    subject_type_id,
                    own_generic_value,
                }) = self.program.lift_dispatch.get(&id).cloned()
                {
                    let dispatch = self.dispatch_to_member(
                        member_id,
                        impl_subject,
                        subject_type_id,
                        &[own_generic_value],
                    );
                    let Dispatch::Call(member_name, _) = dispatch else {
                        // A Lift impl's members are ordinary vilan methods.
                        return Some(js::Node::Void);
                    };
                    let parameter = self.ng.next_name();
                    self.is_bindings
                        .insert(*binder_id, js::Node::Local(parameter.clone()));
                    let mut closure_body = Vec::new();
                    let value = self
                        .walk_entity(*continuation_id, &mut closure_body)
                        .unwrap_or(js::Node::Void);
                    self.is_bindings.remove(binder_id);
                    closure_body.push(js::Node::Return(Box::new(value)));
                    return Some(js::Node::Call(
                        Box::new(js::Node::Local(member_name)),
                        vec![
                            subject,
                            js::Node::Closure(js::Closure {
                                parameters: vec![js::Parameter { name: parameter }],
                                body: closure_body,
                                is_async: false,
                            }),
                        ],
                    ));
                }
                let subject_name = self.ng.next_name();
                block.push(js::Node::ConstVariable(js::Variable {
                    name: subject_name.clone(),
                    value: Box::new(subject),
                }));
                let result_name = self.ng.next_name();
                block.push(js::Node::LetVariable(js::Variable {
                    name: result_name.clone(),
                    value: Box::new(js::Node::Null),
                }));
                let bad_body = vec![js::Node::Assignment(
                    Box::new(js::Node::Local(result_name.clone())),
                    Box::new(js::Node::Local(subject_name.clone())),
                )];
                self.is_bindings.insert(
                    *binder_id,
                    js::Node::PropertyIndex(
                        Box::new(js::Node::Local(subject_name.clone())),
                        Box::new(js::Node::Number("1".to_string(), None)),
                    ),
                );
                let mut good_body = Vec::new();
                let value = self
                    .walk_entity(*continuation_id, &mut good_body)
                    .unwrap_or(js::Node::Void);
                self.is_bindings.remove(binder_id);
                let wrapped = match self.program.lift_dispatch.get(&id) {
                    Some(LiftDispatch::Std { flatten: true, .. }) | None => value,
                    Some(LiftDispatch::Std {
                        flatten: false,
                        enum_id,
                    }) => self.variant_value(*enum_id, 0, vec![value]),
                    // Handled by the early trait-path branch above; a region
                    // marker never lands on a chain node.
                    Some(LiftDispatch::Trait { .. } | LiftDispatch::TraitRegion) => unreachable!(),
                };
                good_body.push(js::Node::Assignment(
                    Box::new(js::Node::Local(result_name.clone())),
                    Box::new(wrapped),
                ));
                block.push(js::Node::If(js::IfBranch::If(
                    Box::new(js::Node::Binary(
                        BinaryOp::Eq,
                        Box::new(js::Node::PropertyIndex(
                            Box::new(js::Node::Local(subject_name)),
                            Box::new(js::Node::Number("0".to_string(), None)),
                        )),
                        Box::new(js::Node::Number("1".to_string(), None)),
                    )),
                    bad_body,
                    Some(Box::new(js::IfBranch::Else(good_body))),
                )));
                js::Node::Local(result_name)
            }
            // Only reachable through the `Local` alias inside a continuation;
            // standalone it has no value.
            Expr::LiftBinder => js::Node::Void,
            // An expression-lifting region (expression-lifting.md §4): the
            // steps emit as progressively nested guards — an eval step is a
            // temp binding (hoisted pre-`?` material, source order), a split
            // step branches on the bad tag and short-circuits the region with
            // the bad container as-is; the body computes in the innermost
            // good branch and wraps back into the container (map) or stands
            // as-is (flatten). No closures — cheaper than the `and_then`/
            // `map` nest it replaces.
            Expr::LiftRegion(steps, body_id) => {
                // A user `Lift` container takes §4's TRAIT path instead: the
                // nested `and_then`/`map` calls ARE the value — no result
                // temp, no tag branching, since short-circuiting is the
                // container's own `and_then`.
                if matches!(
                    self.program.lift_dispatch.get(&id),
                    Some(LiftDispatch::TraitRegion)
                ) {
                    let value = self.emit_lift_region_trait_steps(steps, *body_id, block);
                    for (_, binder_id, _) in steps {
                        self.is_bindings.remove(binder_id);
                    }
                    return Some(value);
                }
                let result_name = self.ng.next_name();
                block.push(js::Node::LetVariable(js::Variable {
                    name: result_name.clone(),
                    value: Box::new(js::Node::Null),
                }));
                self.emit_lift_region_steps(id, steps, *body_id, &result_name, block);
                for (_, binder_id, _) in steps {
                    self.is_bindings.remove(binder_id);
                }
                js::Node::Local(result_name)
            }
            // `expr!` (proposal/try-and-lift.md §4): evaluate the receiver once,
            // branch on the bad tag, return the bad half, yield the good half.
            Expr::TryAssert(receiver_id) => {
                let receiver = self
                    .walk_entity(*receiver_id, block)
                    .unwrap_or(js::Node::Void);
                let name = self.ng.next_name();
                block.push(js::Node::ConstVariable(js::Variable {
                    name: name.clone(),
                    value: Box::new(receiver),
                }));
                let tag_is_bad = |subject: js::Node<'src>| {
                    js::Node::Binary(
                        BinaryOp::Eq,
                        Box::new(js::Node::PropertyIndex(
                            Box::new(subject),
                            Box::new(js::Node::Number("0".to_string(), None)),
                        )),
                        Box::new(js::Node::Number("1".to_string(), None)),
                    )
                };
                match self.program.try_dispatch.get(&id).cloned() {
                    // Option/Result: the bad VALUE (`None`, `Err(e)`) is the
                    // receiver itself — return it as-is (byte-identical at any
                    // success type).
                    Some(TryDispatch::Std) | None => {
                        block.push(js::Node::If(js::IfBranch::If(
                            Box::new(tag_is_bad(js::Node::Local(name.clone()))),
                            vec![js::Node::Return(Box::new(js::Node::Local(name.clone())))],
                            None,
                        )));
                        js::Node::PropertyIndex(
                            Box::new(js::Node::Local(name)),
                            Box::new(js::Node::Number("1".to_string(), None)),
                        )
                    }
                    // A user `Try` impl: `verdict(receiver)`, branch on the
                    // Verdict tag (Good = 0, Bad = 1), return `from_bad(bad)`.
                    Some(TryDispatch::Trait {
                        verdict_id,
                        from_bad_id,
                        impl_subject,
                        receiver_type_id,
                    }) => {
                        let verdict = self.dispatch_to_member(
                            verdict_id,
                            impl_subject,
                            receiver_type_id,
                            &[],
                        );
                        let from_bad = self.dispatch_to_member(
                            from_bad_id,
                            impl_subject,
                            receiver_type_id,
                            &[],
                        );
                        let (Dispatch::Call(verdict_name, _), Dispatch::Call(from_bad_name, _)) =
                            (verdict, from_bad)
                        else {
                            // A Try impl's members are ordinary vilan methods —
                            // an intrinsic/extern here is unreachable.
                            return Some(js::Node::Void);
                        };
                        let verdict_value = self.ng.next_name();
                        block.push(js::Node::ConstVariable(js::Variable {
                            name: verdict_value.clone(),
                            value: Box::new(js::Node::Call(
                                Box::new(js::Node::Local(verdict_name)),
                                vec![js::Node::Local(name)],
                            )),
                        }));
                        block.push(js::Node::If(js::IfBranch::If(
                            Box::new(tag_is_bad(js::Node::Local(verdict_value.clone()))),
                            vec![js::Node::Return(Box::new(js::Node::Call(
                                Box::new(js::Node::Local(from_bad_name)),
                                vec![js::Node::PropertyIndex(
                                    Box::new(js::Node::Local(verdict_value.clone())),
                                    Box::new(js::Node::Number("1".to_string(), None)),
                                )],
                            )))],
                            None,
                        )));
                        js::Node::PropertyIndex(
                            Box::new(js::Node::Local(verdict_value)),
                            Box::new(js::Node::Number("1".to_string(), None)),
                        )
                    }
                }
            }
            Expr::Binary(op, lhs, rhs) => {
                let lhs = self.walk_entity(*lhs, block).unwrap_or(js::Node::Void);
                let rhs = self.walk_entity(*rhs, block).unwrap_or(js::Node::Void);
                // `x op y` where `x: T` is a trait-bounded generic: dispatch to T's
                // concrete operator impl at this monomorphization, re-resolved like
                // the instance-method generic dispatch. (`!=` negates `eq`, as below.)
                // A CONCRETE receiver whose operator method is an inherited trait
                // default (`instant < instant` over `PartialOrd`'s `lt`) records
                // `OnType` instead — same re-dispatch, the type known up front.
                if let Some(GenericDispatch::OnType(Some(receiver_type_id), member_name)) =
                    self.program.generic_dispatch.get(&id).copied()
                {
                    let concrete = self.resolve_type_id(receiver_type_id);
                    if !self.compares_natively(concrete) {
                        if let Some(dispatch) = self.resolve_dispatch(concrete, member_name) {
                            let substitution = self
                                .program
                                .method_call_substitution
                                .get(&id)
                                .cloned()
                                .unwrap_or_default();
                            let saved = self.current_substitution.clone();
                            self.current_substitution.extend(substitution);
                            let call =
                                self.emit_dispatch(dispatch, vec![lhs.clone(), rhs.clone()], None);
                            self.current_substitution = saved;
                            return Some(if matches!(*op, BinaryOp::NotEq) {
                                js::Node::Unary('!', Box::new(call))
                            } else {
                                call
                            });
                        }
                    }
                }
                if let Some(GenericDispatch::OnConstraint(constraint_id, member_name)) =
                    self.program.generic_dispatch.get(&id).copied()
                {
                    let concrete = self
                        .current_substitution
                        .get(&constraint_id)
                        .map(|type_id| self.resolve_type_id(*type_id));
                    // A native-equality concrete type (`Option<i32>`'s element) keeps
                    // native `===`/`!==`; only an aggregate (`Option<Point>`)
                    // dispatches to its `eq` impl.
                    if let Some(concrete_type_id) = concrete.filter(|t| !self.compares_natively(*t))
                    {
                        if let Some(dispatch) = self.resolve_dispatch(concrete_type_id, member_name)
                        {
                            let call = self.emit_dispatch(dispatch, vec![lhs, rhs], None);
                            return Some(if matches!(*op, BinaryOp::NotEq) {
                                js::Node::Unary('!', Box::new(call))
                            } else {
                                call
                            });
                        }
                    }
                }
                // An overloaded operator (`a + b` where `a`'s type implements
                // `Add`) compiles to the trait method call `add(a, b)`. On a generic
                // receiver (`Option<Point> ==`) the method is monomorphized against
                // the recorded type-arg substitution so its body specializes — when
                // the site requires it (`operator_instance_required`, B135): an
                // all-native binding whose body needs no substitution keeps the
                // shared generic emission.
                if let Some(&method_id) = self.program.binary_op_dispatch.get(&id) {
                    let substitution = self.program.method_call_substitution.get(&id).cloned();
                    let name = match substitution {
                        Some(substitution)
                            if self.operator_instance_required(method_id, &substitution) =>
                        {
                            self.emit_instance(method_id, &substitution)
                        }
                        _ => {
                            self.ensure_function_emitted(method_id);
                            self.ng.name_for(method_id)
                        }
                    };
                    let call = js::Node::Call(Box::new(js::Node::Local(name)), vec![lhs, rhs]);
                    // `a != b` dispatches to `eq` and negates — the impl provides
                    // `eq`, and `ne` is just its `!eq` default.
                    return Some(if matches!(*op, BinaryOp::NotEq) {
                        js::Node::Unary('!', Box::new(call))
                    } else {
                        call
                    });
                }
                // JS bitwise is signed: on `u32` operands `>>` must be the
                // logical `>>>`, and the value-producing ops re-wrap with
                // `>>> 0` so a set high bit stays a large unsigned value
                // instead of going negative. `i32` keeps the native ops (JS
                // ToInt32 IS i32 semantics), and `BigInt` must NOT wrap
                // (arbitrary precision). Proposal/bits-and-bytes.md §2.
                if matches!(
                    op,
                    BinaryOp::Shl
                        | BinaryOp::Shr
                        | BinaryOp::BitAnd
                        | BinaryOp::BitXor
                        | BinaryOp::BitOr
                ) && self.binary_operands_are_u32(id)
                {
                    if matches!(op, BinaryOp::Shr) {
                        return Some(binary(BinaryOp::UShr, lhs, rhs));
                    }
                    return Some(binary(
                        BinaryOp::UShr,
                        binary(*op, lhs, rhs),
                        js::Node::Number("0".to_string(), None),
                    ));
                }
                // Integer division truncates toward zero
                // (proposal/numeric-types.md §2): `Math.trunc(a / b)`.
                // Float and BigInt division stay native.
                if matches!(op, BinaryOp::Div) && self.binary_operands_are_integer(id) {
                    return Some(js::Node::Call(
                        Box::new(js::Node::Local("Math.trunc".to_string())),
                        vec![binary(*op, lhs, rhs)],
                    ));
                }
                binary(*op, lhs, rhs)
            }
            Expr::Unary(operator, operand) => {
                let operand = self.walk_entity(*operand, block).unwrap_or(js::Node::Void);
                js::Node::Unary(*operator, Box::new(operand))
            }
            // A view of a scalar place lowers to a `[base, key]` pair — a boxed
            // local's cell at slot 0, or a struct's field slot. A view of an
            // aggregate is the value's own JS reference (an aggregate is its own
            // view), so it passes through unchanged.
            Expr::Reference(operand, _) => {
                // B108/B109: at a by-value return seam the reference CROSSES to a
                // value, so what leaves is the place the reference names — its
                // read, never the pair. (An aggregate view is the value's own JS
                // reference, so it takes this path anyway; only a scalar's
                // representation differs, which is the whole of B108.)
                if self.emits_scalar_view_ref(id, *operand)
                    && !self.program.return_view_reads.contains(&id)
                {
                    let (base, key) = match self.program.entity_map.get(operand) {
                        Some(Expr::Field(subject, _, field_index)) => (
                            self.walk_entity(*subject, block).unwrap_or(js::Node::Void),
                            js::Node::Number(field_index.to_string(), None),
                        ),
                        // `&mut list[i]` — the checked mint (`__at_view`): the
                        // scalar `(base, key)` pair exists only for an in-bounds
                        // element, so a view of an absent element panics at the
                        // `&mut`, not at first use through it.
                        Some(Expr::Index(subject, index)) => {
                            let base = self.walk_entity(*subject, block).unwrap_or(js::Node::Void);
                            let key = self.walk_entity(*index, block).unwrap_or(js::Node::Void);
                            self.used_helpers.insert("__at_view");
                            return Some(js::Node::Call(
                                Box::new(js::Node::Local("__at_view".to_string())),
                                vec![base, key],
                            ));
                        }
                        // A boxed scalar local: the cell itself (slot 0 holds the
                        // value), not the `[0]` read `walk_entity` would produce.
                        Some(Expr::Local(root)) => (
                            js::Node::Local(self.ng.name_for(*root)),
                            js::Node::Number("0".to_string(), None),
                        ),
                        _ => (
                            self.walk_entity(*operand, block).unwrap_or(js::Node::Void),
                            js::Node::Number("0".to_string(), None),
                        ),
                    };
                    return Some(js::Node::Array(vec![base, key]));
                }
                return self.walk_entity(*operand, block);
            }
            // Deref of an aggregate view is the operand itself; deref of a scalar
            // `(base, key)` view reads/writes through `operand[0][operand[1]]`.
            Expr::Dereference(operand) => {
                let operand = *operand;
                let value = self.walk_entity(operand, block);
                if self.derefs_scalar_view(operand) {
                    let view = value.unwrap_or(js::Node::Void);
                    return Some(self.emit_scalar_view_read(operand, view, block));
                }
                return value;
            }
            Expr::Variable(id) => {
                // A dropped resource local must keep its binding even if otherwise
                // unused — the scope's `finally` reads it (destruction.md §7).
                if !self.program.dropped_bindings.contains(id)
                    && self
                        .program
                        .reference_count
                        .get(id)
                        .map(|x| *x < 1)
                        .unwrap_or(true)
                {
                    // An unused binding is dropped — but a side-effecting
                    // initializer (a call mutating through `&mut`, say) must still
                    // run; emit it as a bare statement, discarding the value.
                    let initial = self.program.variables.get(id).and_then(|v| v.initial);
                    if let Some(value_id) = initial
                        && self.expr_has_side_effects(value_id)
                    {
                        return self.walk_entity(value_id, block);
                    }
                    return None;
                }
                let name = self.ng.name_for(*id);
                let variable = self.program.variables.get(id).unwrap();
                let initial = variable.initial;
                let mutable = variable.mutable;
                // HMR (`hmr.md` §5): a transferable module-level binding's
                // initializer is wrapped in an `__hmr_adopt` thunk so it runs
                // lazily — only on a cache miss. Any prelude the initializer needs
                // is walked INTO the thunk body, so a cache hit runs none of it.
                let hmr_binding = if self.hmr {
                    self.program
                        .hmr_bindings
                        .get(id)
                        .filter(|binding| binding.form != TransferForm::Excluded)
                } else {
                    None
                };
                let value = if let Some(hmr_binding) = hmr_binding {
                    let mut thunk_block = Vec::new();
                    let inner = initial
                        .and_then(|value_id| {
                            self.walk_entity(value_id, &mut thunk_block)
                                .map(|node| self.maybe_clone(value_id, node))
                        })
                        .unwrap_or(js::Node::Void);
                    let inner = if self.local_is_boxed(*id) {
                        js::Node::Array(vec![inner])
                    } else {
                        inner
                    };
                    thunk_block.push(js::Node::Return(Box::new(inner)));
                    // SYNCHRONOUS, and it depends on an invariant held
                    // elsewhere: a module-level initializer cannot suspend
                    // (`execution.md` §7.1, enforced await-shaped in
                    // `async_infer`), so nothing walked into `thunk_block`
                    // above can be an `await`. When the check was merely
                    // call-shaped this was reachable, and the result was
                    // `return await (pending);` inside a non-async arrow — a
                    // dev bundle that did not parse at all
                    // (`top-level-await.md` §1.5).
                    //
                    // Do not "fix" that by flipping this to `is_async: true`.
                    // The thunk's value is written into a `const` that every
                    // later binding reads as a value, and
                    // `__hmr_adopt_signal`/`__hmr_adopt_shared` do
                    // `var cell = thunk(); cell[0].v = …` — a promise-shaped
                    // thunk poisons all three, and the call sites would each
                    // need an `await`, which is top-level await again on every
                    // transferable binding in the bundle. If the await rule is
                    // ever relaxed, this contract is a redesign, not a patch
                    // (`top-level-await.md` §4.2).
                    let thunk = js::Node::Closure(js::Closure {
                        parameters: Vec::new(),
                        body: thunk_block,
                        is_async: false,
                    });
                    let callee = match hmr_binding.form {
                        TransferForm::Value => "__hmr_adopt",
                        TransferForm::SignalPayload => "__hmr_adopt_signal",
                        TransferForm::SharedPayload => "__hmr_adopt_shared",
                        TransferForm::Excluded => unreachable!("filtered above"),
                    };
                    js::Node::Call(
                        Box::new(js::Node::Local(callee.to_string())),
                        vec![
                            js::Node::String(Cow::Owned(hmr_binding.key.clone())),
                            js::Node::Number(hmr_binding.fingerprint.to_string(), None),
                            thunk,
                        ],
                    )
                } else {
                    let value = initial
                        .and_then(|value_id| {
                            self.walk_entity(value_id, block)
                                .map(|node| self.maybe_clone(value_id, node))
                        })
                        .unwrap_or(js::Node::Void);
                    // A boxed scalar local is declared as a one-slot cell.
                    if self.local_is_boxed(*id) {
                        js::Node::Array(vec![value])
                    } else {
                        value
                    }
                };
                let js_variable = js::Variable {
                    name,
                    value: Box::new(value),
                };
                // B150: a slot an explicit `drop(x)` empties is rebound once,
                // so it is declared `let` even for an immutable vilan binding.
                // The affine rules make that invisible to the program — the
                // binding is dead after the move, so nothing may read it again.
                if mutable || self.slot_is_emptied_early(*id) {
                    js::Node::LetVariable(js_variable)
                } else {
                    js::Node::ConstVariable(js_variable)
                }
            }
            Expr::Assignment(target_id, value_id) => {
                // B105: a compound assignment desugars to `x = x op v`, which walks
                // the target place TWICE — so an effectful subscript in it ran
                // twice. Evaluate each one here, first, into a temp both walks
                // name; source order puts the subscript ahead of everything the
                // write does, the drop below included.
                self.hoist_compound_target(*target_id, *value_id, block);
                // R2 (destruction.md §5): assigning onto a place that still holds
                // a resource drops the OLD value first, then moves the new one in.
                // The analyzer flagged the assignment and resolved the overwritten
                // value's type; three target shapes reach here and each names the
                // same storage the write is about to clobber — a resource `Local`,
                // (B94) the synthetic `Dereference` of a writable view, whose
                // pointee is the caller's value, and (B99) a COMPONENT projection,
                // whose read is the drop's operand. The drop reads that storage
                // before the write clobbers it — and, on the view path, before
                // `__replace` truncates the payload out from under it (B89) — so
                // the target place is read HERE, in source order, ahead of the
                // new value's own effects.
                let overwrite_drop = match self.program.overwrite_drops.get(&id) {
                    Some(&type_id) => {
                        let target = *target_id;
                        let overwritten = match self.program.entity_map.get(&target) {
                            Some(Expr::Local(binding)) => {
                                Some(js::Node::Local(self.ng.name_for(*binding)))
                            }
                            Some(Expr::Dereference(operand)) => {
                                let operand = *operand;
                                self.walk_entity(operand, block)
                            }
                            Some(
                                Expr::Field(_, _, _)
                                | Expr::TupleIndex(_, _, _)
                                | Expr::Index(_, _),
                            ) => self.walk_entity(target, block),
                            _ => None,
                        };
                        overwritten
                            .and_then(|overwritten| self.resource_drop_of(type_id, overwritten))
                    }
                    None => None,
                };
                // B151: the new value is computed BEFORE the old one is
                // destroyed. Emitting the drop first left a window between the
                // destructor and the write that a throwing right-hand side
                // escaped through, and the scope-end `finally` then walked over
                // the corpse — a double `close()` on the JS backend and a double
                // free on a native one. R2's sentence survives the move: it
                // promises the old value drops before the new one is MOVED IN,
                // and the drop still sits between the two.
                let mut evaluated: Vec<js::Node<'src>> = Vec::new();
                let value = self
                    .walk_entity(*value_id, &mut evaluated)
                    .unwrap_or(js::Node::Void);
                let value = self.maybe_clone(*value_id, value);
                let value = match overwrite_drop {
                    None => {
                        block.append(&mut evaluated);
                        value
                    }
                    // An INERT right-hand side — a tree of literals and reads of
                    // locals, needing no prelude — can neither throw nor observe
                    // the destructor, so the two orders are indistinguishable and
                    // the drop stays where it was. This is what keeps every
                    // existing R2 golden byte-identical: the corpus writes
                    // constructor literals.
                    Some(drop) if evaluated.is_empty() && Self::node_is_inert(&value) => {
                        block.push(drop);
                        value
                    }
                    Some(drop) => {
                        block.append(&mut evaluated);
                        let name = self.ng.next_name();
                        block.push(js::Node::ConstVariable(js::Variable {
                            name: name.clone(),
                            value: Box::new(value),
                        }));
                        block.push(drop);
                        js::Node::Local(name)
                    }
                };
                // Writing a *whole value* through a view. A `Shared` write is a
                // single-slot view (`cell.v`): rebind the slot, so every handle to
                // the cell sees the new value (`cell.v = value`). An ordinary
                // aggregate view REPLACES the pointee's contents in place
                // (`__replace`), so the target and its aliases update rather than
                // rebinding a local.
                // A primitive view's `*c` is a `[0]` slot write — the normal path.
                if let Some(Expr::Dereference(operand)) = self.program.entity_map.get(target_id) {
                    if self.is_shared_write(*operand) {
                        // Take the `v` slot directly rather than walking the
                        // call: a SCALAR pointee now lowers the call to the
                        // `(base, key)` pair, and assigning to the pair would
                        // write the pair, not the cell.
                        let operand = *operand;
                        let slot = self
                            .shared_write_slot(operand, block)
                            .or_else(|| self.walk_entity(operand, block))
                            .unwrap_or(js::Node::Void);
                        return Some(js::Node::Assignment(Box::new(slot), Box::new(value)));
                    }
                    if !self.derefs_scalar_view(*operand) {
                        // `Object.assign` was the write for years and is a MERGE:
                        // it copies the value's slots over the pointee's and
                        // leaves any TRAILING slot the value does not reach
                        // standing (backlog B89). Every aggregate whose width can
                        // shrink is wrong under it — an enum reassigned to a
                        // shorter variant keeps the old payload in the tail
                        // (unreachable through the enum's API, but present), and a
                        // `&mut List<T>` written with a shorter list keeps its old
                        // elements outright (`len` still counts them). The write
                        // means REPLACE, so it emits replace.
                        let base = self.walk_entity(*operand, block).unwrap_or(js::Node::Void);
                        self.used_helpers.insert("__replace");
                        return Some(js::Node::Call(
                            Box::new(js::Node::Local("__replace".to_string())),
                            vec![base, value],
                        ));
                    }
                }
                // `pair.0 = v` on a multi-slot (tuple-typed) element: write
                // each slot of the region from the value (evaluated once).
                // Statically-known width keeps this plain slot assignments —
                // the const-eval interpreter runs them like any other write.
                if let Some(&Expr::TupleIndex(subject_id, offset, width)) =
                    self.program.entity_map.get(target_id)
                {
                    if width > 1 {
                        let subject = self
                            .walk_entity(subject_id, block)
                            .unwrap_or(js::Node::Void);
                        let value_name = self.ng.next_name();
                        block.push(js::Node::ConstVariable(js::Variable {
                            name: value_name.clone(),
                            value: Box::new(value),
                        }));
                        for slot in 0..width {
                            block.push(js::Node::Assignment(
                                Box::new(js::Node::PropertyIndex(
                                    Box::new(subject.clone()),
                                    Box::new(js::Node::Number((offset + slot).to_string(), None)),
                                )),
                                Box::new(js::Node::PropertyIndex(
                                    Box::new(js::Node::Local(value_name.clone())),
                                    Box::new(js::Node::Number(slot.to_string(), None)),
                                )),
                            ));
                        }
                        return None;
                    }
                }
                // `list[i] = v` — the checked write (`__at_put`): writing never
                // creates a slot (growth is `push`), so an out-of-bounds write
                // panics. The read side is `__at`; an assignment target can't
                // be a call, so the write gets its own helper.
                if let Some(&Expr::Index(subject_id, index_id)) =
                    self.program.entity_map.get(target_id)
                {
                    let subject = self
                        .walk_entity(subject_id, block)
                        .unwrap_or(js::Node::Void);
                    let index = self.walk_entity(index_id, block).unwrap_or(js::Node::Void);
                    self.used_helpers.insert("__at_put");
                    return Some(js::Node::Call(
                        Box::new(js::Node::Local("__at_put".to_string())),
                        vec![subject, index, value],
                    ));
                }
                let target = self
                    .walk_entity(*target_id, block)
                    .unwrap_or(js::Node::Void);
                js::Node::Assignment(Box::new(target), Box::new(value))
            }
            Expr::Parameter(_) => {
                return None;
            }
            Expr::Block(body) => {
                // A block owning resource locals is restructured into `try`/
                // `finally` (destruction.md §7). A value-position block captures
                // its tail into a temp declared before the tries, so the value
                // survives the finallys; a void-tail block just discards.
                if self.scope_needs_drops(&body.0) {
                    let tail_is_void = matches!(
                        self.program.entity_map.get(&body.1),
                        Some(Expr::Void) | None
                    );
                    if tail_is_void {
                        let wrapped = self.walk_scope_body(
                            &body.0,
                            0,
                            body.0.len(),
                            Some((body.1, TailDisposition::Discard)),
                        );
                        block.extend(wrapped);
                        return Some(js::Node::Void);
                    }
                    let temp = self.ng.next_name();
                    block.push(js::Node::LetVariable(js::Variable {
                        name: temp.clone(),
                        value: Box::new(js::Node::Void),
                    }));
                    let wrapped = self.walk_scope_body(
                        &body.0,
                        0,
                        body.0.len(),
                        Some((body.1, TailDisposition::AssignTo(temp.clone()))),
                    );
                    block.extend(wrapped);
                    return Some(js::Node::Local(temp));
                }
                for statement in &body.0 {
                    if let Some(node) = self.walk_entity(*statement, block) {
                        // A statement that lowered to nothing (a void tail, a
                        // self-emitting loop/`if`) leaves no stray `undefined`.
                        if !matches!(node, js::Node::Void) {
                            block.push(node);
                        }
                    }
                }
                // B152: a block whose tail LEAVES (`{ ret 1 }`, `{ jump break }`)
                // has no value — emit the statement here, where it is legal, and
                // report no value. Handing the `return` back would put it in
                // whatever value position the block sits in (`const y = return
                // 1;`, `return return 1;`), which does not parse. Everything
                // after it in that position is unreachable anyway.
                let tail = self.walk_entity(body.1, block)?;
                if tail.is_divergent() {
                    block.push(tail);
                    return None;
                }
                return Some(tail);
            }
            Expr::For(condition, body) => {
                // Every loop compiles to a `while`; an absent condition is an
                // infinite loop, i.e. `while (true)`.
                //
                // The condition is walked into its own prelude, NOT into the
                // enclosing block: a condition that needs statements — an `is`
                // subject temp and its materialized captures above all — must
                // run them on EVERY evaluation, and the enclosing block runs
                // them once, before the loop. That was B136's stale-subject
                // miscompile (`proposal/markdown.md` §10.7): a body
                // reassignment never reached the condition's `is` test, so the
                // loop read the wrong branch — or never ended.
                let mut prelude = Vec::new();
                let t_condition = condition
                    .and_then(|condition| self.walk_entity(condition, &mut prelude))
                    .unwrap_or(js::Node::Bool(true));
                // A loop body owning resource locals drops them each iteration
                // (destruction.md §7); `jump break`/`continue` leave through the
                // finally natively. A resource-free body emits as before.
                let t_body = self.walk_loop_body_nodes(&body.0, body.1);
                // A loop is a statement with no value: emit it into the block
                // and yield void, so a loop as a block's tail isn't treated as
                // the block's result.
                if prelude.is_empty() {
                    // A statement-free condition sits in the `while` head as
                    // before — the common case, byte-identical emission.
                    block.push(js::Node::While(Box::new(t_condition), t_body));
                } else {
                    // `while (true) { <prelude> if (!cond) break; <body> }`.
                    // The prelude re-runs per iteration (each evaluation still
                    // reads its subject exactly once, into a fresh per-
                    // iteration temp the captures alias), and a `continue` in
                    // the body re-enters at the prelude — `jump` has no labeled
                    // form, so no jump can skip past the test.
                    let mut loop_body = prelude;
                    loop_body.push(js::Node::If(js::IfBranch::If(
                        Box::new(js::Node::Unary('!', Box::new(t_condition))),
                        vec![js::Node::Break],
                        None,
                    )));
                    loop_body.extend(t_body);
                    block.push(js::Node::While(Box::new(js::Node::Bool(true)), loop_body));
                }
                js::Node::Void
            }
            Expr::ForEach(iterable_id, item_id, body) => {
                let t_iterable = self
                    .walk_entity(*iterable_id, block)
                    .unwrap_or(js::Node::Void);
                // `Set` is a vilan struct over a `NativeMap`; iterate the backing
                // map's stored originals (`set[0].values()`), in insertion order.
                //
                // The type comes from the analyzer's own record for this loop
                // (`for_each_iterable_types`) rather than from a lookup on the
                // iterable expression: only the analyzer knows it for the forms
                // that store no type on their own id — a parameter, and `self`
                // above all, but equally a call, an `if`, a block, an `await` or
                // a `*view`. Asking the expression left all of those looking
                // untyped, and the lowering below silently didn't fire (B85).
                let t_iterable = if self.for_each_iterates_a_set(id) {
                    self.used_helpers.insert("__set_iter");
                    js::Node::Call(
                        Box::new(js::Node::Local("__set_iter".to_string())),
                        vec![t_iterable],
                    )
                } else {
                    t_iterable
                };
                // Snapshot semantics (async-polymorphism.md A.5): inside an
                // ASYNC adapted instance, the loop's awaits admit arbitrary
                // interleaved code, so the traversal iterates a shallow copy
                // taken at entry — the receiver as of the call. Element
                // aliasing doesn't exist under value semantics, so a shallow
                // copy is a sound snapshot. Sync instances are untouched.
                let t_iterable = if !self.current_adapted.is_empty()
                    && self
                        .current_instance
                        .as_ref()
                        .is_some_and(|instance| instance.is_async)
                {
                    js::Node::Array(vec![js::Node::Spread(Box::new(t_iterable))])
                } else {
                    t_iterable
                };

                if let Some(&next_id) = self.program.for_each_next.get(&id) {
                    // Iterator protocol: evaluate the iterator once, then loop
                    // calling `next()` until it yields `None` (variant 1); the
                    // `Some` payload (slot 1) is the element.
                    let next_dispatch = self.for_each_next_dispatch(id, next_id);
                    let iterator_name = self.ng.next_name();
                    let next_value_name = self.ng.next_name();
                    let next_call = self.emit_dispatch(
                        next_dispatch,
                        vec![js::Node::Local(iterator_name.clone())],
                        None,
                    );
                    block.push(js::Node::ConstVariable(js::Variable {
                        name: iterator_name.clone(),
                        value: Box::new(t_iterable),
                    }));
                    let mut loop_body = vec![
                        js::Node::ConstVariable(js::Variable {
                            name: next_value_name.clone(),
                            value: Box::new(next_call),
                        }),
                        js::Node::If(js::IfBranch::If(
                            Box::new(js::Node::Binary(
                                BinaryOp::NotEq,
                                Box::new(js::Node::PropertyIndex(
                                    Box::new(js::Node::Local(next_value_name.clone())),
                                    Box::new(js::Node::Number("0".to_string(), None)),
                                )),
                                Box::new(js::Node::Number("0".to_string(), None)),
                            )),
                            vec![js::Node::Break],
                            None,
                        )),
                    ];
                    if let Some(item_id) = item_id {
                        loop_body.push(js::Node::ConstVariable(js::Variable {
                            name: self.ng.name_for(*item_id),
                            value: Box::new(js::Node::PropertyIndex(
                                Box::new(js::Node::Local(next_value_name)),
                                Box::new(js::Node::Number("1".to_string(), None)),
                            )),
                        }));
                    }
                    loop_body.extend(self.walk_loop_body_nodes(&body.0, body.1));
                    block.push(js::Node::While(Box::new(js::Node::Bool(true)), loop_body));
                    return Some(js::Node::Void);
                }

                // `for e in &mut list` / `&list` — an indexed loop binding each
                // element as a view: a scalar element pairs to `[list, i]`, an
                // aggregate is `list[i]` (its own reference). `list.keys()` yields
                // the indices. The list is bound to a temp so it's evaluated once.
                if let Some(item_id) = *item_id
                    && self.program.for_each_views.contains_key(&item_id)
                {
                    let list_name = self.ng.next_name();
                    block.push(js::Node::ConstVariable(js::Variable {
                        name: list_name.clone(),
                        value: Box::new(t_iterable),
                    }));
                    let index_name = self.ng.next_name();
                    let element = if self.program.primitive_views.contains(&item_id) {
                        js::Node::Array(vec![
                            js::Node::Local(list_name.clone()),
                            js::Node::Local(index_name.clone()),
                        ])
                    } else {
                        js::Node::PropertyIndex(
                            Box::new(js::Node::Local(list_name.clone())),
                            Box::new(js::Node::Local(index_name.clone())),
                        )
                    };
                    let mut loop_body = vec![js::Node::ConstVariable(js::Variable {
                        name: self.ng.name_for(item_id),
                        value: Box::new(element),
                    })];
                    loop_body.extend(self.walk_loop_body_nodes(&body.0, body.1));
                    let keys = js::Node::Call(
                        Box::new(js::Node::Property(
                            Box::new(js::Node::Local(list_name)),
                            "keys".to_string(),
                        )),
                        Vec::new(),
                    );
                    block.push(js::Node::ForOf(index_name, Box::new(keys), loop_body));
                    return Some(js::Node::Void);
                }

                // Otherwise a native `for...of` (a `List` is a JS array).
                let binding = item_id
                    .map(|item_id| self.ng.name_for(item_id))
                    .unwrap_or_else(|| "_".to_string());
                let t_body = self.walk_loop_body_nodes(&body.0, body.1);
                block.push(js::Node::ForOf(binding, Box::new(t_iterable), t_body));
                js::Node::Void
            }
            Expr::Jump(target) => match *target {
                "break" => js::Node::Break,
                "continue" => js::Node::Continue,
                _ => js::Node::Void,
            },
            Expr::If(branch) => {
                fn walk_branch<'src>(
                    t: &mut Transformer<'src>,
                    branch: &ExprIfBranch,
                    block: &mut Vec<js::Node<'src>>,
                    expr_variable_name: &mut Option<String>,
                ) -> js::IfBranch<'src> {
                    match branch {
                        ExprIfBranch::If(condition, body, else_) => {
                            let t_condition = t
                                .walk_entity(*condition, block)
                                .unwrap_or(js::Node::Bool(false));
                            let t_body = t.walk_branch_body(&body.0, body.1, expr_variable_name);
                            js::IfBranch::If(
                                Box::new(t_condition),
                                t_body,
                                else_.as_ref().map(|x| {
                                    Box::new(walk_branch(t, x, block, expr_variable_name))
                                }),
                            )
                        }
                        ExprIfBranch::Else(body) => {
                            let t_body = t.walk_branch_body(&body.0, body.1, expr_variable_name);
                            js::IfBranch::Else(t_body)
                        }
                    }
                }
                let mut expr_variable_name = None;
                let branch = walk_branch(self, branch, block, &mut expr_variable_name);
                match expr_variable_name {
                    Some(variable_name) => {
                        let expr_variable = js::Node::LetVariable(js::Variable {
                            name: variable_name.clone(),
                            value: Box::new(js::Node::Null),
                        });
                        block.push(expr_variable);
                        block.push(js::Node::If(branch));
                        js::Node::Local(variable_name)
                    }
                    // A value-less `if` (no branch produces a value) is a
                    // statement: emit it into the block and yield void, so a
                    // trailing `if` isn't mistaken for the block's/function's
                    // result (and wrapped in `return`/`process.exit`).
                    None => {
                        block.push(js::Node::If(branch));
                        js::Node::Void
                    }
                }
            }
            Expr::Is(subject_id, pattern) => {
                // Evaluate the subject once into a temp; the test reads from it,
                // and any captures alias its payload slots.
                let t_subject = self
                    .walk_entity(*subject_id, block)
                    .unwrap_or(js::Node::Void);
                let subject_name = self.ng.next_name();
                block.push(js::Node::ConstVariable(js::Variable {
                    name: subject_name.clone(),
                    value: Box::new(t_subject),
                }));
                let mut conditions = Vec::new();
                self.compile_is_pattern(pattern, js::Node::Local(subject_name), &mut conditions);
                // B53/B81: a capture that owes a copy, or that must be read at
                // the match, becomes a real binding here — beside the subject
                // temp and before the test. The test's outcome cannot change
                // what the binding holds (a failing test never reads the
                // capture), and the branch bodies are not reachable from this
                // arm.
                self.materialize_captures(pattern, block);
                // An irrefutable pattern (binding/wildcard/tuple) is always true.
                conditions
                    .into_iter()
                    .reduce(|a, b| js::Node::Binary(BinaryOp::And, Box::new(a), Box::new(b)))
                    .unwrap_or(js::Node::Bool(true))
            }
            Expr::Destructure(value_id, pattern) => {
                // `let (a, b) = value` -> bind the value to a temp (evaluated
                // once), then declare each binding from a positional slot:
                // `const __d = value; const a = __d[0]; const b = __d[1];`. An
                // irrefutable binder produces no conditions.
                let value = self.walk_entity(*value_id, block).unwrap_or(js::Node::Void);
                let temp_name = self.ng.next_name();
                block.push(js::Node::ConstVariable(js::Variable {
                    name: temp_name.clone(),
                    value: Box::new(value),
                }));
                let mut conditions = Vec::new();
                let mut bindings = Vec::new();
                self.compile_pattern(
                    pattern,
                    js::Node::Local(temp_name),
                    &mut conditions,
                    &mut bindings,
                );
                for binding in bindings {
                    block.push(binding);
                }
                return None;
            }
            Expr::Match(subject_id, legs) => {
                let t_subject = self
                    .walk_entity(*subject_id, block)
                    .unwrap_or(js::Node::Void);
                // Evaluate the subject once into a temporary; every variant
                // test and capture reads from it.
                let subject_name = self.ng.next_name();
                block.push(js::Node::ConstVariable(js::Variable {
                    name: subject_name.clone(),
                    value: Box::new(t_subject),
                }));
                let result_name = self.ng.next_name();
                block.push(js::Node::LetVariable(js::Variable {
                    name: result_name.clone(),
                    value: Box::new(js::Node::Null),
                }));
                // Each leg becomes an optional variant test plus a body that
                // declares its captures and assigns the leg's value.
                let mut compiled_legs: Vec<MatchLeg<'src>> = Vec::new();
                for leg in legs {
                    let mut body = Vec::new();
                    let mut prelude = Vec::new();
                    let mut guard_condition = None;
                    let subject = js::Node::Local(subject_name.clone());
                    let mut conditions = Vec::new();
                    match leg.guard {
                        // No guard: captures are declared as `const`s in the body.
                        None => {
                            self.compile_pattern(&leg.pattern, subject, &mut conditions, &mut body)
                        }
                        // Guarded: the guard reads the pattern's captures, so they
                        // can't be `const`s declared inside the matched body — alias
                        // them to the subject's slots (like an `is` test) for the
                        // guard and body, then clear the aliases after this leg.
                        Some(guard_id) => {
                            self.compile_is_pattern(&leg.pattern, subject, &mut conditions);
                            // B53/B81: a capture that owes a copy, or that must
                            // be read at the match, becomes a real declaration.
                            // WHERE it lands is decided below, once the guard
                            // has been walked.
                            let mut copies = Vec::new();
                            self.materialize_captures(&leg.pattern, &mut copies);
                            let mut guard_prelude = Vec::new();
                            let outer_reads = self.is_binding_reads.replace(HashSet::default());
                            guard_condition = self.walk_entity(guard_id, &mut guard_prelude);
                            let guard_reads =
                                std::mem::replace(&mut self.is_binding_reads, outer_reads)
                                    .unwrap_or_default();
                            // A guard nested inside another guard's expression
                            // still reads for the outer one.
                            if let Some(outer) = self.is_binding_reads.as_mut() {
                                outer.extend(guard_reads.iter().copied());
                            }
                            // B59: an else-if chain has no statement slot before a
                            // leg's condition, so a guard that needs statements —
                            // its own temporaries (an `is` test, a `?` lift, a
                            // nested `match`), or a capture's declaration it reads
                            // — gets the leg its own slot instead (see the
                            // emission below). B81 widens the second case from
                            // copies to every materialized capture: the guard is
                            // walked AFTER `materialize_captures` has re-pointed
                            // the alias table, so a guard reading one that was
                            // left in the body would name an undeclared binding.
                            let reads_a_declaration = Self::pattern_capture_ids(&leg.pattern)
                                .into_iter()
                                .any(|capture| {
                                    guard_reads.contains(&capture)
                                        && (self.capture_copies(capture)
                                            || self.capture_materializes(capture))
                                });
                            if guard_prelude.is_empty() && !reads_a_declaration {
                                // A plain guard stays an expression in the chain's
                                // condition, and its copies are made on ENTRY to
                                // the body, after the guard has already decided —
                                // a guard that rejects has copied nothing and left
                                // the subject exactly as it found it.
                                body = copies;
                            } else {
                                prelude = copies;
                                prelude.append(&mut guard_prelude);
                            }
                        }
                    }
                    let pattern_condition = conditions
                        .into_iter()
                        .reduce(|a, b| js::Node::Binary(BinaryOp::And, Box::new(a), Box::new(b)));
                    // B62: the resource payloads this leg captured are destroyed
                    // at its end. Read AFTER the pattern has been compiled, so a
                    // guarded leg's accessors (and any declaration `materialize_captures`
                    // re-pointed) are final. The teardown wraps the leg BODY only:
                    // a guard that rejects never enters it, so it destroys nothing
                    // and the next leg finds the subject exactly as it was.
                    let capture_drops = {
                        let captures = self.droppable_pattern_captures(&leg.pattern);
                        self.capture_drop_nodes(captures)
                    };
                    let mut leg_body = Vec::new();
                    let value = self.walk_entity(leg.body, &mut leg_body);
                    let value = value.unwrap_or(js::Node::Null);
                    self.push_result_or_divergence(&result_name, value, &mut leg_body);
                    // A leg owing nothing splices its body in unchanged, which is
                    // what keeps every data match byte-identical.
                    if capture_drops.is_empty() {
                        body.append(&mut leg_body);
                    } else {
                        body.push(js::Node::Try(leg_body, capture_drops));
                    }
                    if leg.guard.is_some() {
                        for capture in Self::pattern_capture_ids(&leg.pattern) {
                            self.is_bindings.remove(&capture);
                        }
                    }
                    let is_catch_all = pattern_condition.is_none() && guard_condition.is_none();
                    compiled_legs.push(MatchLeg {
                        pattern_condition,
                        prelude,
                        guard_condition,
                        body,
                    });
                    if is_catch_all {
                        // Later legs are unreachable.
                        break;
                    }
                }
                // backed-enums.md §9 (candidate (b), ratified): the one match
                // whose final leg does NOT become a bare `else`. A backed enum
                // lowers to a bare host primitive (§3.5), so §1.5's
                // exhaustiveness proof is over the vilan-side VARIANT SET and
                // never was a proof about the runtime value's domain — the
                // host's. The final leg keeps its variant test and the `else`
                // traps, so a value outside the set is named instead of
                // silently becoming whichever variant the analyzer ordered
                // last (P11). Asked of the leg's own pattern, never of where
                // the subject came from — that is the whole point of (b) over
                // (a).
                //
                // §11.6 / B114: asked of the whole pattern, not of its root. A
                // backed test nested in a payload (`Pair::Of(Align::Start)`) is
                // the same hazard, and dropping the leg's condition drops that
                // `===` with the rest. The trap keys on backed tests ONLY, so a
                // match carrying none emits exactly as it always did.
                let mut trap_tests = Vec::new();
                if let Some(final_leg) = compiled_legs
                    .len()
                    .checked_sub(1)
                    .and_then(|index| legs.get(index))
                {
                    self.backed_pattern_tests(
                        &final_leg.pattern,
                        js::Node::Local(subject_name.clone()),
                        &mut trap_tests,
                    );
                }
                // B121 (backed-enums.md §13): a backed test in an EARLIER leg
                // can ALSO be what the bare `else` needs to trap for — a
                // hazard §12.1 does not reach because it only asks the FINAL
                // leg's own pattern. `Pair::Of(Align::Start) => .., Pair::Of
                // (Align::End) => .., Pair::Other => ..` carries no backed
                // test in `Other`'s leg (so `trap_tests` above is empty, same
                // as an ordinary bare `else`), but every `Of` leg tests a
                // SPECIFIC `Align` literal — together they are `Of`'s only
                // handler, so reaching this point with the subject's tag
                // actually `Of` is possible only when the payload left
                // `Align`'s set. §12.1's message still applies (which VALUE
                // left its set, not which TEST failed) but its trap point
                // does not: the payload slot depends on which variant the
                // subject turns out to be, which the final leg's own pattern
                // cannot say. So this re-dispatches on the tag INSIDE the
                // dropped leg's body instead — one partitioned variant at a
                // time, in the order its legs first appear — and traps only
                // for a tag that never got an unconditional (irrefutable-
                // payload) leg of its own; a tag covered that way already
                // matches its own leg earlier in the chain and never reaches
                // here, so re-testing it would-be dead code, not a wrong
                // trap. The final leg's own tag is what is left once none of
                // the partitioned ones matched, and it keeps the author's own
                // body underneath, unchanged.
                let mut earlier_variant_traps: Vec<(usize, js::Node<'src>, Vec<BackedTest<'src>>)> =
                    Vec::new();
                if trap_tests.is_empty()
                    && let Some(final_leg_index) = compiled_legs.len().checked_sub(1)
                    && let Some((final_enum_id, final_variant_index)) = legs
                        .get(final_leg_index)
                        .and_then(|leg| match &leg.pattern {
                            ExprPattern::Variant(enum_id, variant_index, _) => {
                                Some((*enum_id, *variant_index))
                            }
                            _ => None,
                        })
                {
                    for leg in legs.iter().take(final_leg_index) {
                        let ExprPattern::Variant(enum_id, variant_index, _) = &leg.pattern else {
                            continue;
                        };
                        if *enum_id != final_enum_id || *variant_index == final_variant_index {
                            continue;
                        }
                        let mut tests = Vec::new();
                        self.backed_pattern_tests(
                            &leg.pattern,
                            js::Node::Local(subject_name.clone()),
                            &mut tests,
                        );
                        if tests.is_empty() {
                            continue;
                        }
                        match earlier_variant_traps
                            .iter_mut()
                            .find(|(index, _, _)| index == variant_index)
                        {
                            Some((_, _, existing)) => {
                                for test in tests {
                                    let already_seen = existing.iter().any(|seen: &BackedTest| {
                                        seen.enum_id == test.enum_id
                                            && Self::same_trap_accessor(&seen.value, &test.value)
                                    });
                                    if !already_seen {
                                        existing.push(test);
                                    }
                                }
                            }
                            None => {
                                let tag_test = self.variant_tag_test(
                                    *enum_id,
                                    *variant_index,
                                    &js::Node::Local(subject_name.clone()),
                                );
                                earlier_variant_traps.push((*variant_index, tag_test, tests));
                            }
                        }
                    }
                }
                // The analyzer verified exhaustiveness, so an UNGUARDED final
                // leg can always be the `else` branch — its whole test is
                // dropped. Backed tests are the exception (§9/§11.6): the leg
                // keeps its condition and the `else` traps.
                //
                // B115: a GUARDED final leg never carries that proof — the
                // analyzer's walk counts unguarded legs only, so the legs
                // BEFORE this one are what make the match exhaustive, and this
                // one keeps its test, its prelude and its guard. The trap
                // composes cleanly: an in-set value this guard rejects was
                // taken by an earlier leg, so only an out-of-set value reaches
                // the trap, exactly as when the final leg is unguarded.
                let last_leg_droppable = compiled_legs
                    .last()
                    .is_some_and(|leg| leg.guard_condition.is_none());
                if last_leg_droppable && trap_tests.is_empty() && !earlier_variant_traps.is_empty()
                {
                    let mut chain = js::IfBranch::Else(std::mem::take(
                        &mut compiled_legs.last_mut().expect("checked above").body,
                    ));
                    for (_, tag_test, tests) in earlier_variant_traps.into_iter().rev() {
                        let trap = self.trap_body(tests);
                        chain = js::IfBranch::If(Box::new(tag_test), trap, Some(Box::new(chain)));
                    }
                    let last_leg = compiled_legs.last_mut().expect("checked above");
                    last_leg.body = vec![js::Node::If(chain)];
                    last_leg.pattern_condition = None;
                    last_leg.prelude.clear();
                } else if let Some(last_leg) = compiled_legs.last_mut()
                    && last_leg.guard_condition.is_none()
                {
                    if trap_tests.is_empty() {
                        last_leg.pattern_condition = None;
                    }
                    last_leg.prelude.clear();
                }
                if !trap_tests.is_empty() {
                    // ONE backed test is the whole story: reaching the trap
                    // proves that value left its set, so it is named
                    // unconditionally — which is the shipped §9 emission when
                    // the test is the pattern's root (the accessor is the
                    // subject itself), byte for byte.
                    //
                    // SEVERAL backed tests in one leg (`Two::Of(Align::End,
                    // Display::Inline)`) is the case §11.6 said needed a message
                    // design §9 does not have, and it needs none: which value
                    // failed is not knowable from the leg's condition, but it IS
                    // knowable by asking each value whether it is in its enum's
                    // set at all. The first that is not is named; the last is
                    // what is left when none of the others answered.
                    let trap_body = self.trap_body(trap_tests);
                    compiled_legs.push(MatchLeg {
                        pattern_condition: None,
                        prelude: Vec::new(),
                        guard_condition: None,
                        body: trap_body,
                    });
                }
                if compiled_legs.iter().all(|leg| leg.prelude.is_empty()) {
                    self.emit_match_chain(compiled_legs, block);
                } else {
                    self.emit_match_sequence(compiled_legs, block);
                }
                js::Node::Local(result_name)
            }
            Expr::List(ids) => {
                // An element read from a place is a construction slot: it copies
                // (B54), unless the analyzer elided it.
                let items = ids
                    .iter()
                    .filter_map(|id| {
                        self.walk_entity(*id, block)
                            .map(|node| self.maybe_clone(*id, node))
                    })
                    .collect();
                js::Node::Array(items)
            }
            Expr::Repeat(value_id, length) => {
                // `[value; n]` -> `__repeat(value, n)`: the value is evaluated once
                // (the argument) and copied into each slot (a primitive fills, an
                // aggregate clones per slot — see the helper).
                self.used_helpers.insert("__repeat");
                self.used_helpers.insert("__clone");
                let value = self.walk_entity(*value_id, block).unwrap_or(js::Node::Void);
                js::Node::Call(
                    Box::new(js::Node::Local("__repeat".to_string())),
                    vec![value, js::Node::Number(length.to_string(), None)],
                )
            }
            Expr::ArrayLen(subject_id, length) => {
                // `arr.len()` — the length is in the type. A pure subject folds to
                // the literal; a side-effectful one (`make().len()`, `grid[i].len()`)
                // reads `subject.length` in place instead — same value (the array is
                // a plain JS array), evaluated exactly once in source order.
                if self.expr_has_side_effects(*subject_id) {
                    let subject = self
                        .walk_entity(*subject_id, block)
                        .unwrap_or(js::Node::Void);
                    js::Node::Property(Box::new(subject), "length".to_string())
                } else {
                    js::Node::Number(length.to_string(), None)
                }
            }
            Expr::Tuple(ids) => {
                // Tuples store flat: a tuple-typed element's value is itself a flat
                // array, so splice its slots in (`...elem`) rather than nesting it.
                // A `..e` element splices because it was WRITTEN as one — the type
                // rule already proved the operand a tuple (variadic-generics.md
                // §T.2), so it needs no type lookup and cannot lose the splice to a
                // missing one, which the type-driven test does for an element whose
                // expression caches no type of its own (a call, an `if`).
                let items = ids
                    .iter()
                    .filter_map(|id| {
                        let walked = self.walk_entity(*id, block)?;
                        let value = self.maybe_clone(*id, walked);
                        let splices =
                            self.program.spread_elements.contains(id) || self.is_tuple_typed(*id);
                        Some(if splices {
                            js::Node::Spread(Box::new(value))
                        } else {
                            value
                        })
                    })
                    .collect();
                js::Node::Array(items)
            }
            Expr::StructInitializer(_struct_id, assignments) => {
                // let struct_ = self.program.structs.get(struct_id).unwrap();
                // let mut properties_ng = NameGenerator::simple(debug_names);
                let mut properties = assignments
                    .iter()
                    .filter_map(|(i, id)| {
                        // let field = struct_.fields.get(*i).unwrap();
                        let value = self.walk_entity(*id, block);
                        value.map(|x| (i, self.maybe_clone(*id, x)))
                    })
                    .collect::<Vec<_>>();
                properties.sort_by(|a, b| a.0.cmp(b.0));
                let items = properties.into_iter().map(|x| x.1).collect::<Vec<_>>();
                js::Node::Array(items)
            }
            Expr::Module(_module_id) => {
                // println!("SEEN MODULE");
                // let module = self.program.modules.get(module_id).expect("failed to find module by id");
                // self.walk_entities(&module.body.0, block);
                return None;
            }
        })
    }

    /// The JS value for an enum variant. `bool` lowers to a native boolean
    /// (`false`/`true`), a BACKED enum to its bare backing value — a number or a
    /// string — and every other enum to an array `[index, ...data]`.
    fn variant_value(
        &self,
        enum_id: Id,
        variant_index: usize,
        data: Vec<js::Node<'src>>,
    ) -> js::Node<'src> {
        if Some(enum_id) == self.program.bool_enum_id {
            return js::Node::Bool(variant_index == 1);
        }
        if let Some(value) = self.backed_variant_value(enum_id, variant_index) {
            return value;
        }
        let mut items = vec![js::Node::Number(variant_index.to_string(), None)];
        items.extend(data);
        js::Node::Array(items)
    }

    /// The bare backing value of a variant if `enum_id` is a backed enum, else
    /// `None` (it uses the array representation). `Align::Start` is the JS
    /// string `"start"` exactly as `Ordering::Greater` is the JS number `1`:
    /// one rule, two literal kinds (backed-enums.md §3.5).
    fn backed_variant_value(&self, enum_id: Id, variant_index: usize) -> Option<js::Node<'src>> {
        let enum_ = self.program.enums.get(&enum_id)?;
        enum_.backing?;
        match &enum_.variants.get(variant_index)?.backing_value {
            BackingValue::Int(discriminant) => {
                Some(js::Node::Number(discriminant.to_string(), None))
            }
            // The declaration carries the RAW literal text, so it unescapes at
            // emission exactly like any other string literal.
            BackingValue::Str(text) => Some(js::Node::String(Cow::Owned(
                unescape_string(text).into_owned(),
            ))),
        }
    }

    /// The name of `enum_id` if it is a BACKED enum, else `None`. `bool` is
    /// excluded deliberately: it lowers to a native scalar through its own
    /// special case rather than through a backing value (§3.4 rejects a `bool`
    /// backing), and a `match` over it is not what §9's trap arm guards.
    fn backed_enum_name(&self, enum_id: Id) -> Option<&'src str> {
        if Some(enum_id) == self.program.bool_enum_id {
            return None;
        }
        let enum_ = self.program.enums.get(&enum_id)?;
        enum_.backing?;
        Some(enum_.name)
    }

    /// For a variant of an enum that lowers to a native scalar — `bool`
    /// (`subject === true`) or a backed enum (`subject === backing value`) — the
    /// equality test. `None` for array-form enums, which test the `[0]` slot.
    ///
    /// A string backing needs no new codegen path: this is the same `===` chain
    /// a `match` over a raw `str` already emits, with a `js::Node::String` where
    /// the `js::Node::Number` is (§1.4's probe P2).
    fn scalar_variant_test(
        &self,
        enum_id: Id,
        variant_index: usize,
        subject: &js::Node<'src>,
    ) -> Option<js::Node<'src>> {
        let value = if Some(enum_id) == self.program.bool_enum_id {
            js::Node::Bool(variant_index == 1)
        } else {
            self.backed_variant_value(enum_id, variant_index)?
        };
        Some(js::Node::Binary(
            BinaryOp::Eq,
            Box::new(subject.clone()),
            Box::new(value),
        ))
    }

    /// Every BACKED-enum test `pattern` carries, in source order, each paired
    /// with the accessor that reads the value it tests.
    ///
    /// This is §9's trap question asked of the pattern TREE rather than of its
    /// root (§11.6, B114). A backed enum reached through a payload —
    /// `Pair::Of(Align::Start)` — is the same hazard one level down: its `===`
    /// rides in the leg's condition, and the final leg drops that condition
    /// whole. The walk mirrors `compile_pattern`'s accessors exactly, so the
    /// value named at the trap is the value the dropped test compared.
    ///
    /// A backed enum has no payload variants (§3.3), so a backed test is always
    /// a LEAF — the walk never recurses through one, and a variant's payload is
    /// only descended for the array-form enums that have one.
    fn backed_pattern_tests(
        &self,
        pattern: &ExprPattern,
        subject: js::Node<'src>,
        out: &mut Vec<BackedTest<'src>>,
    ) {
        match pattern {
            // Irrefutable, or refutable against something that is not an enum
            // value: a literal pattern's domain is the primitive's own, which no
            // host value can leave.
            ExprPattern::Wildcard | ExprPattern::Binding(_) | ExprPattern::Literal(_) => {}
            ExprPattern::Variant(enum_id, _, payload) => {
                if let Some(enum_name) = self.backed_enum_name(*enum_id) {
                    out.push(BackedTest {
                        enum_name,
                        enum_id: *enum_id,
                        value: subject,
                    });
                    return;
                }
                // `bool` lowers to a native scalar too (and carries no payload),
                // but its two values ARE its domain — nothing to trap.
                if Some(*enum_id) == self.program.bool_enum_id {
                    return;
                }
                for (data_index, sub_pattern) in payload.iter().enumerate() {
                    let element = js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number((data_index + 1).to_string(), None)),
                    );
                    self.backed_pattern_tests(sub_pattern, element, out);
                }
            }
            ExprPattern::Tuple(elements) => {
                let mut leaves = Vec::new();
                Self::flatten_tuple_pattern(elements, &subject, 0, &mut leaves);
                for (sub_pattern, element) in leaves {
                    self.backed_pattern_tests(sub_pattern, element, out);
                }
            }
            ExprPattern::Array(elements) => {
                for (index, sub_pattern) in elements.iter().enumerate() {
                    let element = js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number(index.to_string(), None)),
                    );
                    self.backed_pattern_tests(sub_pattern, element, out);
                }
            }
        }
    }

    /// `value === v1 || value === v2 || …` over every variant of a backed enum:
    /// the runtime question "is this one of its values AT ALL", which is a
    /// different question from "did this leg's test match". Only the trap block
    /// asks it, and only when one leg carries more than one backed test — see
    /// `Expr::Match`.
    fn backed_value_membership(
        &self,
        enum_id: Id,
        value: &js::Node<'src>,
    ) -> Option<js::Node<'src>> {
        let enum_ = self.program.enums.get(&enum_id)?;
        (0..enum_.variants.len())
            .filter_map(|variant_index| self.scalar_variant_test(enum_id, variant_index, value))
            .reduce(|a, b| js::Node::Binary(BinaryOp::Or, Box::new(a), Box::new(b)))
    }

    /// The runtime test for "is this value variant `variant_index` of
    /// `enum_id`" — a native scalar comparison for `bool`/backed enums, else
    /// the array's own discriminant slot at index 0. `compile_pattern`'s
    /// `Variant` arm builds exactly this (inline, for the pattern it is
    /// walking); B121's earlier-leg re-dispatch needs the same test for a leg
    /// it is NOT walking through `compile_pattern` (it reads straight off the
    /// leg list), so it is pulled out here rather than duplicated ad hoc.
    fn variant_tag_test(
        &self,
        enum_id: Id,
        variant_index: usize,
        subject: &js::Node<'src>,
    ) -> js::Node<'src> {
        self.scalar_variant_test(enum_id, variant_index, subject)
            .unwrap_or_else(|| {
                js::Node::Binary(
                    BinaryOp::Eq,
                    Box::new(js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number("0".to_string(), None)),
                    )),
                    Box::new(js::Node::Number(variant_index.to_string(), None)),
                )
            })
    }

    /// Whether two trap accessors read the exact same slot. Restricted to the
    /// `Local`/`PropertyIndex`/`Number` shapes `backed_pattern_tests` ever
    /// builds (a chain of property reads off the match subject) — B121's
    /// per-variant grouping uses it to tell "the same `Align` slot, tested by
    /// two different legs of the same variant" from "two different slots that
    /// happen to share an enum", without a general `js::Node` equality that
    /// would have to answer for every other variant too. Anything outside
    /// that shape compares unequal, which only costs a redundant (harmless)
    /// trap test — never a wrong one.
    fn same_trap_accessor(a: &js::Node<'src>, b: &js::Node<'src>) -> bool {
        match (a, b) {
            (js::Node::Local(left), js::Node::Local(right)) => left == right,
            (js::Node::Number(left, _), js::Node::Number(right, _)) => left == right,
            (
                js::Node::PropertyIndex(left_object, left_index),
                js::Node::PropertyIndex(right_object, right_index),
            ) => {
                Self::same_trap_accessor(left_object, right_object)
                    && Self::same_trap_accessor(left_index, right_index)
            }
            _ => false,
        }
    }

    /// The `__enum_trap` sequence for a set of backed tests read while
    /// reaching one point of a match: `K` tests become `K − 1`
    /// membership-guarded traps and one bare one (backed-enums.md §12.1) —
    /// the last needs no guard because it is what is left once none of the
    /// others answered. Factored out of `Expr::Match`'s final-leg trap so
    /// B121's earlier-leg re-dispatch can build the identical shape for a
    /// DIFFERENT reason to be there (a variant whose own legs, not the final
    /// one, exhausted their literals) without a second implementation to
    /// drift out of step with the first.
    fn trap_body(&mut self, tests: Vec<BackedTest<'src>>) -> Vec<js::Node<'src>> {
        self.used_helpers.insert("__enum_trap");
        let final_test = tests.len() - 1;
        let mut body = Vec::new();
        for (index, test) in tests.into_iter().enumerate() {
            let trap = js::Node::Call(
                Box::new(js::Node::Local("__enum_trap".to_string())),
                vec![
                    js::Node::String(Cow::Borrowed(test.enum_name)),
                    test.value.clone(),
                ],
            );
            match self.backed_value_membership(test.enum_id, &test.value) {
                Some(membership) if index < final_test => {
                    body.push(js::Node::If(js::IfBranch::If(
                        Box::new(js::Node::Unary('!', Box::new(membership))),
                        vec![trap],
                        None,
                    )))
                }
                _ => body.push(trap),
            }
        }
        body
    }

    /// B53 (rule 1): whether this capture's slot read copies AT THIS EMISSION.
    fn capture_copies(&self, capture_id: Id) -> bool {
        self.copy_applies(self.program.capture_clone_sites.get(&capture_id))
    }

    /// B81/B88: whether this capture must become a real declaration on the
    /// alias path even when it owes no copy — see
    /// [`Program::materialized_captures`].
    fn capture_materializes(&self, capture_id: Id) -> bool {
        self.program.materialized_captures.contains(&capture_id)
    }

    /// Emits a match as an else-if chain, each leg's test the conjunction of its
    /// pattern and its guard. The shape every match had before B59, and the one
    /// every match without a statement slot still has.
    fn emit_match_chain(&mut self, legs: Vec<MatchLeg<'src>>, block: &mut Vec<js::Node<'src>>) {
        let mut chain: Option<js::IfBranch<'src>> = None;
        for leg in legs.into_iter().rev() {
            let condition = match (leg.pattern_condition, leg.guard_condition) {
                (Some(pattern), Some(guard)) => Some(js::Node::Binary(
                    BinaryOp::And,
                    Box::new(pattern),
                    Box::new(guard),
                )),
                (Some(condition), None) | (None, Some(condition)) => Some(condition),
                (None, None) => None,
            };
            chain = Some(match condition {
                None => js::IfBranch::Else(leg.body),
                Some(condition) => {
                    js::IfBranch::If(Box::new(condition), leg.body, chain.map(Box::new))
                }
            });
        }
        match chain {
            // A lone catch-all needs no branching at all.
            Some(js::IfBranch::Else(body)) => block.extend(body),
            Some(chain) => block.push(js::Node::If(chain)),
            None => {}
        }
    }

    /// Emits a match whose legs need statement slots (B59): an else-if chain has
    /// nowhere to put the statements a guard hoists — an `is` test, a `?` lift, a
    /// nested `match` — so the guard's temporaries were walked and dropped, and
    /// the emitted condition referenced a name that was never declared.
    ///
    /// The legs become a flat statement sequence instead. Each leg's slot is the
    /// body of its own pattern test, so its prelude runs only once the pattern
    /// has matched (a guard's temporary may read a payload slot that only exists
    /// on the matched variant), and a `matched` flag stands in for the `else`s.
    fn emit_match_sequence(&mut self, legs: Vec<MatchLeg<'src>>, block: &mut Vec<js::Node<'src>>) {
        let matched_name = self.ng.next_name();
        block.push(js::Node::LetVariable(js::Variable {
            name: matched_name.clone(),
            value: Box::new(js::Node::Bool(false)),
        }));
        let leg_count = legs.len();
        for (index, leg) in legs.into_iter().enumerate() {
            let mut body = leg.body;
            // Nothing follows the final leg to fall through to, so it has no
            // flag to set — whether it is the `else` (unguarded) or keeps a test
            // of its own (a guarded final leg, B115).
            if index + 1 < leg_count {
                // Record the match BEFORE the body runs, so a body that returns,
                // breaks, or continues cannot leave the flag behind.
                body.insert(
                    0,
                    js::Node::Assignment(
                        Box::new(js::Node::Local(matched_name.clone())),
                        Box::new(js::Node::Bool(true)),
                    ),
                );
            }
            let mut slot = leg.prelude;
            match leg.guard_condition {
                Some(guard) => {
                    slot.push(js::Node::If(js::IfBranch::If(Box::new(guard), body, None)))
                }
                None => slot.extend(body),
            }
            // Every leg but the first falls through only while nothing has
            // matched; the first has nothing before it to fall through from.
            let unmatched =
                || js::Node::Unary('!', Box::new(js::Node::Local(matched_name.clone())));
            let test = match (index == 0, leg.pattern_condition) {
                (true, pattern) => pattern,
                (false, None) => Some(unmatched()),
                (false, Some(pattern)) => Some(js::Node::Binary(
                    BinaryOp::And,
                    Box::new(unmatched()),
                    Box::new(pattern),
                )),
            };
            match test {
                None => block.extend(slot),
                Some(test) => {
                    block.push(js::Node::If(js::IfBranch::If(Box::new(test), slot, None)))
                }
            }
        }
    }

    /// Turn an ALIASED pattern's captures (`is`, a guarded match leg) into real
    /// declarations. That path binds nothing — each capture is recorded as an
    /// accessor into the subject and substituted at every reference — so a
    /// capture that owes anything gets a declaration here, and its alias
    /// re-points at the declared name.
    ///
    /// Two independent reasons to declare one, and they compose into one
    /// statement:
    ///
    /// - **B53 (rule 1) — it COPIES.** The declaration wraps the slot read in
    ///   `__clone`; the alias alone would hand the body the subject's own
    ///   storage. Captures that share or move own nothing to copy and keep
    ///   their accessor, which is what keeps the elisions free — and a RESOURCE
    ///   capture never copies (R1).
    /// - **B81/B88 — it must be READ at the match.** A subject whose storage a
    ///   write can reach IN PLACE — through a writable view, or through a
    ///   COMPONENT write / `&mut` / `&mut self` call on an owned place — is
    ///   mutated under the temp, so an accessor re-read later in the leg
    ///   returns post-write state. The declaration freezes the read without
    ///   touching the value: no `__clone`, so a SHARE stays a share and a
    ///   resource stays the loan B62's leg teardown destroys through
    ///   (`capture_drop_nodes` reads the alias table after this runs, so it
    ///   finds the declared name and destroys the very value the leg
    ///   captured).
    ///
    /// WHERE `out` goes decides WHEN that happens, and the callers answer
    /// differently: an `is` test emits into the statements before it (reading an
    /// unmatched payload yields `undefined`, so a failing test pays nothing),
    /// while a guarded leg picks between its prelude and its body once the guard
    /// has been walked (see `Expr::Match`).
    fn materialize_captures(&mut self, pattern: &ExprPattern, out: &mut Vec<js::Node<'src>>) {
        for capture_id in Self::pattern_capture_ids(pattern) {
            let copies = self.capture_copies(capture_id);
            if !copies && !self.capture_materializes(capture_id) {
                continue;
            }
            let Some(accessor) = self.is_bindings.get(&capture_id).cloned() else {
                continue;
            };
            let read = if copies {
                self.used_helpers.insert("__clone");
                js::Node::Call(
                    Box::new(js::Node::Local("__clone".to_string())),
                    vec![accessor],
                )
            } else {
                accessor
            };
            let name = self.ng.name_for(capture_id);
            let variable = js::Variable {
                name: name.clone(),
                value: Box::new(read),
            };
            let mutable = self
                .program
                .variables
                .get(&capture_id)
                .is_some_and(|variable| variable.mutable);
            // B150: a capture an explicit `drop(c)` empties is rebound once.
            out.push(if mutable || self.slot_is_emptied_early(capture_id) {
                js::Node::LetVariable(variable)
            } else {
                js::Node::ConstVariable(variable)
            });
            self.is_bindings.insert(capture_id, js::Node::Local(name));
        }
    }

    // Compiles a match pattern against the JS expression holding the value it
    // matches: variant tests are appended to `conditions` and capture
    // declarations to `bindings`.
    /// Compiles a pattern for an `is` test: collects the boolean test conditions
    /// and records each capture as an alias to the subject's payload slot (so
    /// references compile to `t[i]` rather than a binding statement). A capture
    /// that owes a copy is turned into a real binding afterwards by
    /// `materialize_captures` — the alias alone would hand the body the
    /// subject's own storage.
    fn compile_is_pattern(
        &mut self,
        pattern: &ExprPattern,
        subject: js::Node<'src>,
        conditions: &mut Vec<js::Node<'src>>,
    ) {
        match pattern {
            ExprPattern::Wildcard => {}
            ExprPattern::Binding(capture_id) => {
                self.is_bindings.insert(*capture_id, subject);
            }
            ExprPattern::Variant(enum_id, variant_index, payload) => {
                // `bool` and numeric enums lower to native values (see
                // `compile_pattern`), so they test by value, not array slot.
                if let Some(test) = self.scalar_variant_test(*enum_id, *variant_index, &subject) {
                    conditions.push(test);
                    return;
                }
                conditions.push(js::Node::Binary(
                    BinaryOp::Eq,
                    Box::new(js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number("0".to_string(), None)),
                    )),
                    Box::new(js::Node::Number(variant_index.to_string(), None)),
                ));
                for (data_index, sub_pattern) in payload.iter().enumerate() {
                    let element = js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number((data_index + 1).to_string(), None)),
                    );
                    self.compile_is_pattern(sub_pattern, element, conditions);
                }
            }
            ExprPattern::Tuple(elements) => {
                let mut leaves = Vec::new();
                Self::flatten_tuple_pattern(elements, &subject, 0, &mut leaves);
                for (sub_pattern, element) in leaves {
                    self.compile_is_pattern(sub_pattern, element, conditions);
                }
            }
            // Array binders are irrefutable and parse only in binder position
            // (`let`/parameters) in v1, so an `is`/match array pattern cannot
            // reach here — but sub-patterns recurse defensively, condition-free.
            ExprPattern::Array(elements) => {
                for (index, sub_pattern) in elements.iter().enumerate() {
                    let element = js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number(index.to_string(), None)),
                    );
                    self.compile_is_pattern(sub_pattern, element, conditions);
                }
            }
            ExprPattern::Literal(literal_id) => {
                conditions.push(self.literal_equality(*literal_id, subject));
            }
        }
    }

    /// Flattens a tuple pattern's elements to `(sub-pattern, subject-slot)` leaves
    /// for flat storage: a nested tuple pattern recurses (accumulating the flat
    /// offset), a width-1 element reads `subject[offset]`, and a multi-slot capture
    /// (a binding/wildcard of tuple type) reslices `subject.slice(offset, end)`.
    fn flatten_tuple_pattern<'a>(
        elements: &'a [(ExprPattern, usize)],
        subject: &js::Node<'src>,
        base: usize,
        out: &mut Vec<(&'a ExprPattern, js::Node<'src>)>,
    ) {
        let mut offset = base;
        for (sub_pattern, width) in elements {
            match sub_pattern {
                ExprPattern::Tuple(inner) => {
                    Self::flatten_tuple_pattern(inner, subject, offset, out);
                }
                _ if *width == 1 => out.push((
                    sub_pattern,
                    js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number(offset.to_string(), None)),
                    ),
                )),
                _ => out.push((
                    sub_pattern,
                    js::Node::Call(
                        Box::new(js::Node::Property(
                            Box::new(subject.clone()),
                            "slice".to_string(),
                        )),
                        vec![
                            js::Node::Number(offset.to_string(), None),
                            js::Node::Number((offset + width).to_string(), None),
                        ],
                    ),
                )),
            }
            offset += width;
        }
    }

    /// Lowers an `[extern]`-bound call to its host (JS) form. The first argument
    /// is the receiver for method/property bindings; a `Function` binding with a
    /// module records the import to emit.
    fn emit_extern(
        &mut self,
        target_id: Id,
        binding: ExternBinding<'src>,
        args: Vec<js::Node<'src>>,
    ) -> js::Node<'src> {
        match binding {
            ExternBinding::Function { module, symbol } => {
                if let Some(module) = module {
                    self.used_imports
                        .entry(module.to_string())
                        .or_default()
                        .insert(symbol.to_string());
                }
                // A `__`-named free extern is a runtime helper whose source
                // lives in the helper table (glue for shapes the extern binding
                // forms can't express — option-object arguments, `??`
                // flattening, global property reads).
                if let Some(helper) = extern_helper(symbol) {
                    self.used_helpers.insert(helper);
                    // `__nursery_new_detached` builds on the base `__Nursery`
                    // class + `__nursery_new` factory (it makes a detached
                    // nursery and overrides its failure path), and its owned
                    // tasks make `__task` reach `__nursery_is_cancel` — which
                    // lives in the `__nursery_run` helper (a free/awaited task
                    // short-circuits that call, so a plain spawn never needs
                    // it, but an OWNED task does). There is no transitive helper
                    // resolver, so co-emit both — the `__repeat` -> `__clone`
                    // precedent.
                    if helper == "__nursery_new_detached" {
                        self.used_helpers.insert("__nursery_new");
                        self.used_helpers.insert("__nursery_run");
                    }
                }
                js::Node::Call(Box::new(js::Node::Local(symbol.to_string())), args)
            }
            ExternBinding::Method { symbol } => {
                // The JS method name defaults to the external's source name.
                let method = symbol
                    .or_else(|| {
                        self.program
                            .external_functions
                            .get(&target_id)
                            .map(|e| e.name)
                    })
                    .unwrap_or("")
                    .to_string();
                let mut args = args.into_iter();
                let receiver = args.next().unwrap_or(js::Node::Void);
                js::Node::Call(
                    Box::new(js::Node::Property(Box::new(receiver), method)),
                    args.collect(),
                )
            }
            ExternBinding::Get { symbol } => {
                let receiver = args.into_iter().next().unwrap_or(js::Node::Void);
                js::Node::Property(Box::new(receiver), symbol.to_string())
            }
            ExternBinding::Set { symbol } => {
                let mut args = args.into_iter();
                let receiver = args.next().unwrap_or(js::Node::Void);
                let value = args.next().unwrap_or(js::Node::Void);
                js::Node::Assignment(
                    Box::new(js::Node::Property(Box::new(receiver), symbol.to_string())),
                    Box::new(value),
                )
            }
            // `new Symbol(args)` — the callee renders verbatim, and an extern is
            // only ever emitted as a direct call, so the textual form is exact.
            // A module-qualified class imports first (the `Function` rule).
            ExternBinding::New { module, symbol } => {
                if let Some(module) = module {
                    self.used_imports
                        .entry(module.to_string())
                        .or_default()
                        .insert(symbol.to_string());
                }
                js::Node::Call(Box::new(js::Node::Local(format!("new {symbol}"))), args)
            }
        }
    }

    /// Lowers an `external` std intrinsic call. Method intrinsics take the
    /// receiver as the first argument; helper-backed ones record the helper so
    /// it's emitted in the prelude.
    fn emit_intrinsic(
        &mut self,
        intrinsic: Intrinsic,
        args: Vec<js::Node<'src>>,
        call_expr_id: Option<Id>,
    ) -> js::Node<'src> {
        // `Shared::write()` over a SCALAR pointee is a view like any other, so
        // it lowers to the `(base, key)` pair the view machinery reads. `cell.v`
        // alone is the VALUE: passing it where a `&mut i32` is expected handed
        // the callee a number, and `slot[0][slot[1]]` on it is `undefined` — a
        // runtime crash, not a diagnostic. The assign-through and deref sites
        // take the `v` slot back off the pair. Whether the pointee is scalar is
        // decidable only here, per monomorphization.
        if call_expr_id.is_some_and(|id| self.emits_scalar_shared_write(intrinsic, id)) {
            let cell = args.into_iter().next().unwrap_or(js::Node::Void);
            return js::Node::Array(vec![
                cell,
                js::Node::String(std::borrow::Cow::Borrowed("v")),
            ]);
        }
        // A method that maps directly onto a native JS method (`str`, `Set`, `Map`):
        // the receiver is `self` (the first argument), the rest pass through as args.
        fn native_method<'a, I: Iterator<Item = js::Node<'a>>>(
            args: &mut I,
            native: &str,
        ) -> js::Node<'a> {
            let receiver = args.next().unwrap_or(js::Node::Void);
            js::Node::Call(
                Box::new(js::Node::Property(Box::new(receiver), native.to_string())),
                args.collect(),
            )
        }
        let mut args = args.into_iter();
        match intrinsic {
            Intrinsic::Scan => {
                self.used_helpers.insert("__scan");
                js::Node::Call(Box::new(js::Node::Local("__scan".to_string())), Vec::new())
            }
            Intrinsic::StrTrim => native_method(&mut args, "trim"),
            Intrinsic::StrToLowercaseAscii => native_method(&mut args, "toLowerCase"),
            Intrinsic::StrToUppercase => native_method(&mut args, "toUpperCase"),
            Intrinsic::StrContains => native_method(&mut args, "includes"),
            Intrinsic::StrStartsWith => native_method(&mut args, "startsWith"),
            Intrinsic::StrEndsWith => native_method(&mut args, "endsWith"),
            Intrinsic::StrReplace => native_method(&mut args, "replaceAll"),
            Intrinsic::StrRepeat => native_method(&mut args, "repeat"),
            Intrinsic::StrSplit => native_method(&mut args, "split"),
            // NOT `native_method`: JS `substring` clamps negatives to 0 and
            // SWAPS an inverted pair, so `s.substring(offset, -1)` silently
            // returns the prefix — the complement of what was asked for. The
            // checked helper refuses instead, the way `list[i]` goes through
            // `__at` rather than a bare subscript.
            Intrinsic::StrSubstring => {
                self.used_helpers.insert("__substring");
                js::Node::Call(
                    Box::new(js::Node::Local("__substring".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::StrLen | Intrinsic::ListLen => js::Node::Property(
                Box::new(args.next().unwrap_or(js::Node::Void)),
                "length".to_string(),
            ),
            Intrinsic::ListGet => {
                self.used_helpers.insert("__list_get");
                self.used_helpers.insert("__clone");
                js::Node::Call(
                    Box::new(js::Node::Local("__list_get".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::ListPop => {
                self.used_helpers.insert("__list_pop");
                js::Node::Call(
                    Box::new(js::Node::Local("__list_pop".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::ListSortBy => {
                self.used_helpers.insert("__list_sort_by");
                js::Node::Call(
                    Box::new(js::Node::Local("__list_sort_by".to_string())),
                    args.collect(),
                )
            }
            // `opt.take()` / `opt.replace(v)` (destruction.md §6): the receiver is
            // the `&mut self` slot (a JS array, mutated in place so the caller's
            // binding sees the change). Each snapshots the old contents, rewrites
            // the slot to `None` / `Some(value)`, and returns the snapshot.
            Intrinsic::OptionTake => {
                self.used_helpers.insert("__option_take");
                js::Node::Call(
                    Box::new(js::Node::Local("__option_take".to_string())),
                    vec![args.next().unwrap_or(js::Node::Void)],
                )
            }
            Intrinsic::OptionReplace => {
                self.used_helpers.insert("__option_replace");
                js::Node::Call(
                    Box::new(js::Node::Local("__option_replace".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::ParseI32 => {
                self.used_helpers.insert("__parse_i32");
                js::Node::Call(
                    Box::new(js::Node::Local("__parse_i32".to_string())),
                    vec![args.next().unwrap_or(js::Node::Void)],
                )
            }
            Intrinsic::TryParseJson => {
                self.used_helpers.insert("__try_parse_json");
                js::Node::Call(
                    Box::new(js::Node::Local("__try_parse_json".to_string())),
                    vec![args.next().unwrap_or(js::Node::Void)],
                )
            }
            Intrinsic::ParseF64 => {
                self.used_helpers.insert("__parse_f64");
                js::Node::Call(
                    Box::new(js::Node::Local("__parse_f64".to_string())),
                    vec![args.next().unwrap_or(js::Node::Void)],
                )
            }
            Intrinsic::RandomInt => {
                self.used_helpers.insert("__random_int");
                js::Node::Call(
                    Box::new(js::Node::Local("__random_int".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::RandomFloat => {
                self.used_helpers.insert("__random_float");
                js::Node::Call(
                    Box::new(js::Node::Local("__random_float".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::Args => {
                self.used_helpers.insert("__args");
                js::Node::Call(Box::new(js::Node::Local("__args".to_string())), Vec::new())
            }
            Intrinsic::Env => {
                self.used_helpers.insert("__env");
                js::Node::Call(
                    Box::new(js::Node::Local("__env".to_string())),
                    vec![args.next().unwrap_or(js::Node::Void)],
                )
            }
            // `Shared::new(value)` -> a `{ v: value }` cell (a JS object, so
            // `__clone` shares it by reference rather than deep-copying).
            Intrinsic::SharedNew => {
                self.used_helpers.insert("__shared_new");
                js::Node::Call(
                    Box::new(js::Node::Local("__shared_new".to_string())),
                    vec![args.next().unwrap_or(js::Node::Void)],
                )
            }
            // `shared.clone()` -> the same cell (the receiver, unchanged).
            Intrinsic::SharedClone => args.next().unwrap_or(js::Node::Void),
            // `shared.read()` / `shared.write()` -> the cell's value, `self.v`.
            // `write` returns a view of the slot; the write-*through* (rebind vs
            // merge) is handled where the assignment is lowered.
            Intrinsic::SharedValue | Intrinsic::SharedWrite => js::Node::Property(
                Box::new(args.next().unwrap_or(js::Node::Void)),
                "v".to_string(),
            ),
            // `Set::new()` -> `new Set()` (no constructor args).
            Intrinsic::SetNew => {
                js::Node::Call(Box::new(js::Node::Local("new Set".to_string())), Vec::new())
            }
            Intrinsic::SetInsert => native_method(&mut args, "add"),
            Intrinsic::SetContains => native_method(&mut args, "has"),
            Intrinsic::SetRemove => native_method(&mut args, "delete"),
            Intrinsic::SetLen => js::Node::Property(
                Box::new(args.next().unwrap_or(js::Node::Void)),
                "size".to_string(),
            ),
            // `Map::new()` -> `new Map()` (no constructor args).
            Intrinsic::MapNew => {
                js::Node::Call(Box::new(js::Node::Local("new Map".to_string())), Vec::new())
            }
            Intrinsic::MapInsert => native_method(&mut args, "set"),
            Intrinsic::MapGet => {
                self.used_helpers.insert("__map_get");
                self.used_helpers.insert("__clone");
                js::Node::Call(
                    Box::new(js::Node::Local("__map_get".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::MapContainsKey => native_method(&mut args, "has"),
            Intrinsic::MapRemove => native_method(&mut args, "delete"),
            Intrinsic::MapLen => js::Node::Property(
                Box::new(args.next().unwrap_or(js::Node::Void)),
                "size".to_string(),
            ),
            Intrinsic::MapKeys => {
                self.used_helpers.insert("__map_keys");
                self.used_helpers.insert("__clone");
                js::Node::Call(
                    Box::new(js::Node::Local("__map_keys".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::MapValues => {
                self.used_helpers.insert("__map_values");
                self.used_helpers.insert("__clone");
                js::Node::Call(
                    Box::new(js::Node::Local("__map_values".to_string())),
                    args.collect(),
                )
            }
            Intrinsic::JsonField => {
                let receiver = args.next().unwrap_or(js::Node::Void);
                let key = args.next().unwrap_or(js::Node::Void);
                js::Node::PropertyIndex(Box::new(receiver), Box::new(key))
            }
            Intrinsic::JsonTag => {
                self.used_helpers.insert("__json_tag");
                js::Node::Call(
                    Box::new(js::Node::Local("__json_tag".to_string())),
                    args.collect(),
                )
            }
            // A parsed JSON array already is a JS array, so `elements` is the
            // receiver itself (typed as `List<JsonValue>`).
            Intrinsic::JsonElements => args.next().unwrap_or(js::Node::Void),
            // `self === null` — the `Option::None` discriminator.
            Intrinsic::JsonIsNull => js::Node::Binary(
                BinaryOp::Eq,
                Box::new(args.next().unwrap_or(js::Node::Void)),
                Box::new(js::Node::Null),
            ),
            // The normalized JSON kind string, for the decode type checks.
            Intrinsic::JsonKind => {
                self.used_helpers.insert("__json_kind");
                js::Node::Call(
                    Box::new(js::Node::Local("__json_kind".to_string())),
                    args.collect(),
                )
            }
            // The canonical key of a value (Hashable / value-keyed Map/Set).
            Intrinsic::CanonicalHash => {
                self.used_helpers.insert("__hash");
                js::Node::Call(
                    Box::new(js::Node::Local("__hash".to_string())),
                    args.collect(),
                )
            }
            // `a === b` — the body of `impl Hash with PartialEq`. A `Hash` is
            // always a JS primitive, so native equality IS its equality
            // (hashable-keys.md §3.2); no helper, the comparison is the node.
            Intrinsic::HashEq => js::Node::Binary(
                BinaryOp::Eq,
                Box::new(args.next().unwrap_or(js::Node::Void)),
                Box::new(args.next().unwrap_or(js::Node::Void)),
            ),
            // `Array.from(document.querySelectorAll(selector))` — the NodeList as a
            // real array, so `List` operations (`map`/`push`/…) behave.
            Intrinsic::QuerySelectorAll => {
                let query = js::Node::Call(
                    Box::new(js::Node::Local("document.querySelectorAll".to_string())),
                    vec![args.next().unwrap_or(js::Node::Void)],
                );
                js::Node::Call(
                    Box::new(js::Node::Local("Array.from".to_string())),
                    vec![query],
                )
            }
        }
    }

    /// `subject === <literal>` — the test a literal pattern compiles to.
    fn literal_equality(&mut self, literal_id: Id, subject: js::Node<'src>) -> js::Node<'src> {
        let mut throwaway = Vec::new();
        let literal = self
            .walk_entity(literal_id, &mut throwaway)
            .unwrap_or(js::Node::Void);
        js::Node::Binary(BinaryOp::Eq, Box::new(subject), Box::new(literal))
    }

    /// The capture variable ids a pattern binds, in order — so a guarded leg can
    /// clear their subject-slot aliases after the leg is compiled.
    fn pattern_capture_ids(pattern: &ExprPattern) -> Vec<Id> {
        let mut ids = Vec::new();
        fn collect(pattern: &ExprPattern, ids: &mut Vec<Id>) {
            match pattern {
                ExprPattern::Binding(capture_id) => ids.push(*capture_id),
                ExprPattern::Variant(_, _, payload) => {
                    for sub_pattern in payload {
                        collect(sub_pattern, ids);
                    }
                }
                ExprPattern::Tuple(elements) => {
                    for (sub_pattern, _width) in elements {
                        collect(sub_pattern, ids);
                    }
                }
                ExprPattern::Array(elements) => {
                    for sub_pattern in elements {
                        collect(sub_pattern, ids);
                    }
                }
                ExprPattern::Wildcard | ExprPattern::Literal(_) => {}
            }
        }
        collect(pattern, &mut ids);
        ids
    }

    fn compile_pattern(
        &mut self,
        pattern: &ExprPattern,
        subject: js::Node<'src>,
        conditions: &mut Vec<js::Node<'src>>,
        bindings: &mut Vec<js::Node<'src>>,
    ) {
        match pattern {
            ExprPattern::Wildcard => {}
            ExprPattern::Binding(capture_id) => {
                let name = self.ng.name_for(*capture_id);
                let mutable = self
                    .program
                    .variables
                    .get(capture_id)
                    .map(|variable| variable.mutable)
                    .unwrap_or(false);
                // B53 (rule 1): an aggregate capture from a place subject is
                // a value copy, like any binding of an aggregate place. The
                // `Array` arm below cloned its element reads already — skip
                // the second wrap when this subject is one of those.
                let already_cloned = matches!(
                    &subject,
                    js::Node::Call(callee, _)
                        if matches!(callee.as_ref(), js::Node::Local(name) if name == "__clone")
                );
                let subject = if self.capture_copies(*capture_id) && !already_cloned {
                    self.used_helpers.insert("__clone");
                    js::Node::Call(
                        Box::new(js::Node::Local("__clone".to_string())),
                        vec![subject],
                    )
                } else {
                    subject
                };
                let variable = js::Variable {
                    name,
                    value: Box::new(subject),
                };
                // B150: a capture an explicit `drop(c)` empties is rebound once.
                bindings.push(if mutable || self.slot_is_emptied_early(*capture_id) {
                    js::Node::LetVariable(variable)
                } else {
                    js::Node::ConstVariable(variable)
                });
            }
            ExprPattern::Variant(enum_id, variant_index, payload) => {
                // `bool` and numeric (C-like) enums lower to native scalars, so
                // their variants test by value (`subject === true` / `=== -1`)
                // rather than by array discriminant slot.
                if let Some(test) = self.scalar_variant_test(*enum_id, *variant_index, &subject) {
                    conditions.push(test);
                    return;
                }
                conditions.push(js::Node::Binary(
                    BinaryOp::Eq,
                    Box::new(js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number("0".to_string(), None)),
                    )),
                    Box::new(js::Node::Number(variant_index.to_string(), None)),
                ));
                for (data_index, sub_pattern) in payload.iter().enumerate() {
                    // Variant data sits after the variant index.
                    let element = js::Node::PropertyIndex(
                        Box::new(subject.clone()),
                        Box::new(js::Node::Number((data_index + 1).to_string(), None)),
                    );
                    self.compile_pattern(sub_pattern, element, conditions, bindings);
                }
            }
            ExprPattern::Tuple(elements) => {
                // Tuples store flat: read each leaf at its flat offset, reslicing a
                // multi-slot (sub-tuple) capture.
                let mut leaves = Vec::new();
                Self::flatten_tuple_pattern(elements, &subject, 0, &mut leaves);
                for (sub_pattern, element) in leaves {
                    self.compile_pattern(sub_pattern, element, conditions, bindings);
                }
            }
            ExprPattern::Array(elements) => {
                // A fixed array stores as a plain JS array: element `i` reads
                // `subject[i]`, CLONED — a destructured binding is a value copy
                // (rule 1), and `__clone` is identity on scalars. Indices are
                // statically in range (count == length was checked at
                // resolution), so no bounds helper is needed.
                self.used_helpers.insert("__clone");
                for (index, sub_pattern) in elements.iter().enumerate() {
                    let element = js::Node::Call(
                        Box::new(js::Node::Local("__clone".to_string())),
                        vec![js::Node::PropertyIndex(
                            Box::new(subject.clone()),
                            Box::new(js::Node::Number(index.to_string(), None)),
                        )],
                    );
                    self.compile_pattern(sub_pattern, element, conditions, bindings);
                }
            }
            ExprPattern::Literal(literal_id) => {
                conditions.push(self.literal_equality(*literal_id, subject));
            }
        }
    }

    fn function(&mut self, function: &Function<'src>) -> js::Node<'src> {
        let name = self.ng.name_for(function.id);
        self.function_with_name(function, name)
    }

    fn function_with_name(&mut self, function: &Function<'src>, name: String) -> js::Node<'src> {
        // Never-silent (B55): every path that emits a callable funnels through
        // here, so this is where "a call resolved to NOTHING" becomes visible. A
        // function with no source body is a trait's signature-only requirement —
        // it is never the answer to a call, only what a call falls back to when
        // the receiver's generic never got bound. Record it; assembly refuses.
        if !function.has_body {
            self.bodyless_emissions.push(function.id);
        }
        let parameters = function
            .parameters
            .iter()
            .map(|parameter_id| js::Parameter {
                name: self.ng.name_for(*parameter_id),
            })
            .collect::<Vec<_>>();
        // A body owning resource locals is restructured into per-resource
        // `try`/`finally` teardown (destruction.md §7); one owning none emits
        // exactly as before (byte-identical corpus gate).
        let mut body = self.parameter_entry_preludes(&function.parameters);
        body.extend(self.walk_function_body(function));
        // An `own` resource parameter not moved out drops at its LAST USE — or,
        // when the dataflow refuses to say, at the body's scope end
        // (destruction.md §6). `walk_function_body` above has already emitted
        // the split form when the last use is short of the end; this wraps the
        // whole body otherwise, keeping parameters last in the reverse order.
        let body = self.wrap_own_param_drops(function, body);
        js::Node::Function(js::Function {
            name,
            parameters,
            body,
            is_async: self.program.async_functions.contains(&function.id)
                || self
                    .current_instance
                    .as_ref()
                    .is_some_and(|instance| instance.is_async),
        })
    }

    /// The function body's statements and tail, restructured for teardown.
    ///
    /// Three shapes, narrowest first, so a program that owns nothing keeps the
    /// bytes it had:
    ///
    /// 1. no resource declarations and no early-dropping `own` parameter — the
    ///    plain statement list;
    /// 2. resource declarations only — [`Self::walk_scope_body`] over the whole
    ///    body, which places each local's drop at its own last use;
    /// 3. an `own` resource parameter whose last use is short of the body's end
    ///    — the body SPLIT at that statement, the prefix wrapped in the
    ///    parameters' `try`/`finally` and the suffix (with the tail) emitted
    ///    after it. This is the parameter twin of a local's early drop, and it
    ///    is done here rather than in [`Self::wrap_own_param_drops`] because by
    ///    the time that runs the statement boundaries are gone.
    fn walk_function_body(&mut self, function: &Function<'src>) -> Vec<js::Node<'src>> {
        let mark = self.pending_temporaries.len();
        let mut body = self.walk_function_body_nodes(function);
        self.seal_pending_temporaries(mark, &mut body);
        body
    }

    fn walk_function_body_nodes(&mut self, function: &Function<'src>) -> Vec<js::Node<'src>> {
        let statements = &function.body.0;
        let tail = function.body.1;
        if let Some(split) = self.own_param_split(function) {
            let prefix = self.walk_scope_body(statements, 0, split, None);
            let finally = self.own_param_drop_nodes(function);
            let mut out = vec![js::Node::Try(prefix, finally)];
            out.extend(self.walk_scope_body(
                statements,
                split,
                statements.len(),
                Some((tail, TailDisposition::Return)),
            ));
            return out;
        }
        if self.scope_needs_drops(statements) {
            return self.walk_scope_body(
                statements,
                0,
                statements.len(),
                Some((tail, TailDisposition::Return)),
            );
        }
        let mut body = self.walk_list(statements);
        if let Some(return_expr) = self.walk_entity(tail, &mut body) {
            match return_expr {
                js::Node::Void => {}
                // A tail that already left the function — `fun a(): i32 { ret 1
                // }` — is the statement, not a value to return (B152: wrapping
                // it emitted `return return 1;`).
                node if node.is_divergent() => body.push(node),
                _ => {
                    body.push(js::Node::Return(Box::new(return_expr)));
                }
            }
        }
        body
    }

    /// The statement index (exclusive) at which this function's owned resource
    /// parameters stop being live, when that is short of the body's end —
    /// `None` when they run to the end (the shipped whole-body wrap), when the
    /// dataflow refuses to answer for any of them, or when the function owns no
    /// droppable parameter at all.
    ///
    /// A parameter is declared before every statement, so its region starts at
    /// index 0 and the group discharges together at the LAST of their last uses
    /// — which keeps the reverse-declaration order `own_param_drop_nodes`
    /// emits for a simultaneous discharge.
    fn own_param_split(&self, function: &Function<'src>) -> Option<usize> {
        let parameters = self.own_param_drops(function);
        if parameters.is_empty() {
            return None;
        }
        let statements = &function.body.0;
        let teardown = ScopeTeardown::Captures(parameters.iter().map(|(id, _)| *id).collect());
        // The parameters' region starts at the body's entry rather than after a
        // declaration statement, and — like any region — must cover every
        // teardown declared inside it.
        let end = statements.len();
        let own = self.own_teardown_extent(&teardown, statements, 0, end);
        let extent = self.widen_over_declarations(own, statements, 0, end);
        (extent < end).then_some(extent)
    }

    /// This function's `own` resource parameters that actually destroy
    /// something, in declaration order.
    fn own_param_drops(&self, function: &Function<'src>) -> Vec<(Id, TypeId)> {
        function
            .parameters
            .iter()
            .filter(|parameter_id| self.program.dropped_bindings.contains(parameter_id))
            .filter_map(|parameter_id| {
                let type_id = self.program.parameters.get(parameter_id)?.type_id;
                self.type_drops_nontrivially(type_id)
                    .then_some((*parameter_id, type_id))
            })
            .collect()
    }

    /// The `finally` nodes destroying this function's owned resource parameters,
    /// in reverse declaration order.
    fn own_param_drop_nodes(&mut self, function: &Function<'src>) -> Vec<js::Node<'src>> {
        let mut finally: Vec<js::Node<'src>> = Vec::new();
        for (parameter_id, type_id) in self.own_param_drops(function).iter().rev() {
            let value = js::Node::Local(self.ng.name_for(*parameter_id));
            if let Some(drop) = self.slot_drop_node(*parameter_id, *type_id, value) {
                finally.push(drop);
            }
        }
        finally
    }

    /// Wrap a function body in a `try`/`finally` that drops its owned resource
    /// parameters (destruction.md §6) in reverse declaration order, or return the
    /// body unchanged when it has none — which every resource-free program does,
    /// keeping its output byte-identical. The parameter type ids match the glue
    /// `build_drop_glue` seeded from the same `parameters` table.
    ///
    /// A function whose parameters drop EARLY has already been split by
    /// [`Self::walk_function_body`]; this returns its body untouched, because
    /// `own_param_split` answering `Some` is exactly the case that already
    /// emitted the `finally`.
    fn wrap_own_param_drops(
        &mut self,
        function: &Function<'src>,
        body: Vec<js::Node<'src>>,
    ) -> Vec<js::Node<'src>> {
        if self.own_param_split(function).is_some() {
            return body;
        }
        let param_drops: Vec<(Id, TypeId)> = self.own_param_drops(function);
        if param_drops.is_empty() {
            return body;
        }
        let mut finally: Vec<js::Node<'src>> = Vec::new();
        for (parameter_id, type_id) in param_drops.iter().rev() {
            let value = js::Node::Local(self.ng.name_for(*parameter_id));
            if let Some(drop) = self.slot_drop_node(*parameter_id, *type_id, value) {
                finally.push(drop);
            }
        }
        vec![js::Node::Try(body, finally)]
    }

    /// How a `for` loop's protocol call (`next` / `next_mut`) lowers. The loop
    /// is a call site like any other and takes the SAME dispatch precedence as
    /// `Expr::Call`: a generic subject re-dispatches to the concrete type its
    /// constraint is bound to here; `self` in a trait default re-dispatches to
    /// the type the default is being specialized for; a concrete receiver whose
    /// impl is generic monomorphizes against the loop's recorded bindings.
    /// Emitting by bare id — which is all this did — left a generic callee's own
    /// parameters unbound, so ITS bounded calls resolved to the trait's empty
    /// abstract member (B55).
    fn for_each_next_dispatch(&mut self, for_each_id: Id, next_id: Id) -> Dispatch<'src> {
        let member = self.program.generic_dispatch.get(&for_each_id).copied();
        let concrete = match member {
            Some(GenericDispatch::OnConstraint(constraint_id, _)) => {
                self.current_substitution.get(&constraint_id).copied()
            }
            Some(GenericDispatch::OnType(type_id, _)) => type_id.or(self.current_self_type),
            None => None,
        };
        if let Some((GenericDispatch::OnConstraint(_, member_name), type_id))
        | Some((GenericDispatch::OnType(_, member_name), type_id)) = member.zip(concrete)
        {
            let preferred = self
                .program
                .bound_dispatch_traits
                .get(&for_each_id)
                .cloned();
            if let Some(dispatch) = self.resolve_dispatch_with(type_id, member_name, &[], preferred)
            {
                return dispatch;
            }
        }
        let name = match self.call_substitution(for_each_id, next_id, &[]) {
            Some(substitution) => self.emit_instance(next_id, &substitution),
            None => {
                self.ensure_function_emitted(next_id);
                self.ng.name_for(next_id)
            }
        };
        Dispatch::Call(name, self.program.async_functions.contains(&next_id))
    }

    /// The gate this call is retargeted to, if it is one of the split build's
    /// recognized route matches. `None` for every other call in every other
    /// build — which is why a flagless build emits exactly what it always did.
    fn split_gate_target(&self, call_id: Id, target_id: Id) -> Option<(Id, Id)> {
        let gate = self.chunk_gate.as_ref()?;
        (target_id == gate.swap && gate.calls.contains(&call_id))
            .then_some((gate.swap_split, gate.preload))
    }

    /// Re-keys a type substitution from one function's generic parameters onto
    /// another's, by position. The two must declare the same generics in the
    /// same order — which is the standing requirement on `swap`/`swap_split`,
    /// pinned by the gate's own test.
    fn rebind_by_position(
        &self,
        from: Id,
        to: Id,
        substitution: &HashMap<TypeId, TypeId>,
    ) -> HashMap<TypeId, TypeId> {
        let (Some(from), Some(to)) = (
            self.program.functions.get(&from),
            self.program.functions.get(&to),
        ) else {
            return HashMap::default();
        };
        from.generic_parameter_constraint_ids
            .iter()
            .zip(to.generic_parameter_constraint_ids.iter())
            .filter_map(|(from, to)| substitution.get(from).map(|bound| (*to, *bound)))
            .collect()
    }

    /// Notes a requirement in the frame currently being recorded — called at
    /// every requirement, memo hit or fresh emission alike (`const-eval.md`
    /// §10.6). A no-op outside the const pass.
    fn record_require(&mut self, key: Option<EmissionId>) {
        if let (Some(recorder), Some(key)) = (&mut self.recorder, key) {
            if let Some(frame) = recorder.frames.last_mut() {
                frame.requires.push(key);
            }
        }
    }

    /// Registers a keyed emission whose body is about to be walked: reserves its
    /// identity, files it under the memo key its emitter looks it up by, and
    /// notes it as a requirement of the frame that asked for it.
    fn record_keyed(
        &mut self,
        remember: impl FnOnce(&mut EmissionRecorder, EmissionId),
    ) -> Option<EmissionId> {
        let key = {
            let recorder = self.recorder.as_mut()?;
            let key = EmissionId::Keyed(recorder.keyed_slots.len());
            recorder.keyed_slots.push(None);
            remember(recorder, key);
            key
        };
        CONST_LOWERING_COUNT.with(|count| count.set(count.get() + 1));
        self.record_require(Some(key));
        Some(key)
    }

    /// Notes a keyed emission the memo already held as a requirement of the
    /// asking frame — the path that makes a memo hit contribute exactly what a
    /// fresh emission would.
    fn record_hit(&mut self, lookup: impl FnOnce(&EmissionRecorder) -> Option<EmissionId>) {
        let key = self.recorder.as_ref().and_then(lookup);
        self.record_require(key);
    }

    /// Records where a keyed emission's node landed, once it is pushed.
    fn record_landed(&mut self, key: Option<EmissionId>) {
        let slot = self.monomorphized.len().checked_sub(1);
        if let (Some(recorder), Some(EmissionId::Keyed(index)), Some(slot)) =
            (self.recorder.as_mut(), key, slot)
        {
            recorder.keyed_slots[index] = Some(slot);
        }
    }

    /// Opens a recording frame: the three accumulating sets are lent to it, so
    /// what the body about to be walked adds is exactly what the frame holds.
    /// Returns `None` (and does nothing) outside the const pass.
    fn record_enter(&mut self) -> Option<FrameSets> {
        self.recorder.as_ref()?;
        let saved = (
            std::mem::take(&mut self.referenced_globals),
            std::mem::take(&mut self.used_helpers),
            std::mem::take(&mut self.used_imports),
        );
        if let Some(recorder) = &mut self.recorder {
            recorder.frames.push(EmissionRecord::default());
        }
        Some(saved)
    }

    /// Closes a frame, files it under `key`, and merges what it captured back
    /// into the enclosing accumulation — so an enclosing frame, and the
    /// assembly of a whole-program transform, see exactly what they saw before.
    /// Returns the closed frame when it is not filed under a key — the const
    /// world's site walk and prelude build both read their own frame back.
    fn record_leave(
        &mut self,
        saved: Option<FrameSets>,
        key: Option<EmissionId>,
    ) -> Option<EmissionRecord> {
        let (globals, helpers, imports) = saved?;
        let mut frame = self
            .recorder
            .as_mut()
            .and_then(|recorder| recorder.frames.pop())
            .unwrap_or_default();
        frame.globals = std::mem::replace(&mut self.referenced_globals, globals);
        frame.helpers = std::mem::replace(&mut self.used_helpers, helpers);
        frame.imports = std::mem::replace(&mut self.used_imports, imports);
        self.referenced_globals
            .extend(frame.globals.iter().copied());
        self.used_helpers.extend(frame.helpers.iter().copied());
        for (module, symbols) in &frame.imports {
            self.used_imports
                .entry(module.clone())
                .or_default()
                .extend(symbols.iter().cloned());
        }
        match (self.recorder.as_mut(), key) {
            (Some(recorder), Some(key)) => {
                recorder.records.insert(key, frame);
                None
            }
            _ => Some(frame),
        }
    }

    /// Emits a concrete (non-generic) function once, keyed by its id. Any active
    /// substitution and self-type are cleared while walking it, since its body
    /// has no generic parameters of its own and is not a default being
    /// specialized.
    fn ensure_function_emitted(&mut self, function_id: Id) {
        self.record_require(Some(EmissionId::Function(function_id)));
        if self.required_functions.contains_key(&function_id) {
            return;
        }
        // Already walking this body higher up the stack (a recursive call): the
        // call site just needs the name, so don't re-enter — otherwise a recursive
        // function would emit its body forever. The outer call records it below.
        if !self.emitting.insert(function_id) {
            return;
        }
        if let Some(function) = self.program.functions.get(&function_id) {
            if self.recorder.is_some() {
                CONST_LOWERING_COUNT.with(|count| count.set(count.get() + 1));
            }
            let saved = std::mem::take(&mut self.current_substitution);
            let saved_self = self.current_self_type.take();
            let saved_instance = self.enter_instance(function_id, Vec::new());
            let frame = self.record_enter();
            let js_function = self.function(function);
            self.record_leave(frame, Some(EmissionId::Function(function_id)));
            self.restore_instance(saved_instance);
            self.current_substitution = saved;
            self.current_self_type = saved_self;
            self.required_functions.insert(function_id, js_function);
        }
        self.emitting.remove(&function_id);
    }

    /// Re-dispatches a trait method call to the receiver's concrete `type_id`:
    /// resolves to the type's own impl member if it declares one, otherwise an
    /// inherited trait default specialized for the type (so the default's inner
    /// `self.method()` calls dispatch to this type too). The member may be an
    /// intrinsic or an `[extern]` external — which lower to a host form, not a
    /// call to an emitted function — so this returns a [`Dispatch`] describing
    /// how to emit it; `emit_dispatch` turns that into the actual call node. A
    /// generic dispatch resolving to an extern/intrinsic without this would mint
    /// a dangling name for a function that is never emitted.
    fn resolve_dispatch(&mut self, type_id: TypeId, member: &str) -> Option<Dispatch<'src>> {
        self.resolve_dispatch_with(type_id, member, &[], None)
    }

    /// Whether `implementation` provides `trait_id` at an instantiation the
    /// re-dispatch demonstrably does NOT want — B73's R1 on the emission side.
    ///
    /// The filter is deliberately one-sided: it excludes an impl only when a
    /// wanted argument and the impl's written one resolve to two *different
    /// nominal types* (`Conv<Bar>` against a bound written `Conv<Baz>`). A
    /// position still abstract on either side after `resolve_type_id` — the
    /// impl's own binder (`impl Signal<type T> with Readable<T>`), or a bound
    /// argument the current monomorphization has not fixed — proves nothing
    /// here and keeps the impl, so every program whose arguments already agreed
    /// emits exactly the bytes it emitted before.
    fn trait_instantiation_conflicts(
        &self,
        implementation: &crate::analyzer::Implementation<'src>,
        trait_id: Id,
        wanted_arguments: &[TypeId],
    ) -> bool {
        if wanted_arguments.is_empty() {
            return false;
        }
        let Some((_, written_arguments)) = implementation
            .trait_args
            .iter()
            .find(|(id, _)| *id == trait_id)
        else {
            return false;
        };
        if written_arguments.len() != wanted_arguments.len() {
            return false;
        }
        written_arguments
            .iter()
            .zip(wanted_arguments)
            .any(|(written, wanted)| self.nominally_different(*written, *wanted))
    }

    /// Whether two types are provably distinct nominal types — the only
    /// judgement [`trait_instantiation_conflicts`] is willing to act on. Type
    /// ids are minted, not interned, so identity is compared through the types
    /// they name and never through the ids themselves.
    fn nominally_different(&self, left: TypeId, right: TypeId) -> bool {
        let left = self.resolve_type_id(left);
        let right = self.resolve_type_id(right);
        if left == right {
            return false;
        }
        match (
            self.program.type_id_to_type_map.get(&left),
            self.program.type_id_to_type_map.get(&right),
        ) {
            (Some(Type::Struct(left_id, _)), Some(Type::Struct(right_id, _)))
            | (Some(Type::Enum(left_id, _)), Some(Type::Enum(right_id, _))) => left_id != right_id,
            (Some(Type::Struct(..)), Some(Type::Enum(..)))
            | (Some(Type::Enum(..)), Some(Type::Struct(..))) => true,
            _ => false,
        }
    }

    /// `resolve_dispatch`, additionally binding the target method's OWN generics
    /// from `own_generic_values` (the call's bindings in declaration order —
    /// recorded against the trait member the analyzer saw, whose ids differ from
    /// each concrete impl's, so only positional values cross the re-dispatch),
    /// and — when the analyzer resolved the call through a trait — dispatching on
    /// THAT trait's surface (`preferred_trait`): its impl's override, else its
    /// default, never an inherent method that happens to share the name.
    fn resolve_dispatch_with(
        &mut self,
        type_id: TypeId,
        member: &str,
        own_generic_values: &[TypeId],
        preferred_trait: Option<(Id, Vec<TypeId>)>,
    ) -> Option<Dispatch<'src>> {
        let type_id = self.resolve_type_id(type_id);
        if let Some((trait_id, trait_arguments)) = preferred_trait {
            // Resolve strictly within the trait AND its instantiation (B73 R1).
            // The impl's override first...
            if let Some((member_id, impl_subject)) =
                self.resolve_member_on_trait_impl(type_id, trait_id, &trait_arguments, member)
            {
                return Some(self.dispatch_to_member(
                    member_id,
                    impl_subject,
                    type_id,
                    own_generic_values,
                ));
            }
            // ...else the trait's own default, specialized for this type.
            if let Some(default_id) = self.trait_default_member(trait_id, member) {
                let is_async = self.program.async_functions.contains(&default_id);
                return Some(Dispatch::Call(
                    self.emit_default_instance(default_id, type_id),
                    is_async,
                ));
            }
            // The preference didn't materialize (shouldn't happen for a call the
            // analyzer resolved) — fall through to the general lookup.
        }
        if let Some((member_id, impl_subject)) = self.resolve_member_on_type(type_id, member) {
            return Some(self.dispatch_to_member(
                member_id,
                impl_subject,
                type_id,
                own_generic_values,
            ));
        }
        let default_id = self.resolve_inherited_default(type_id, member)?;
        let is_async = self.program.async_functions.contains(&default_id);
        Some(Dispatch::Call(
            self.emit_default_instance(default_id, type_id),
            is_async,
        ))
    }

    /// Lowers a resolved member to its dispatch: an intrinsic, an extern, or an
    /// emitted (possibly monomorphized) instance. Binds the impl's generics from
    /// the concrete receiver type — so a method whose body uses the impl's type
    /// parameter (`T::from_json_value` inside `List<T>::from_json_value`)
    /// resolves it concretely even when reached as a *nested* dispatch — plus
    /// the method's OWN generics from the call's ordered values (without which
    /// the instance emitted with them unbound — the silent no-op through a
    /// bound).
    fn dispatch_to_member(
        &mut self,
        member_id: Id,
        impl_subject: TypeId,
        type_id: TypeId,
        own_generic_values: &[TypeId],
    ) -> Dispatch<'src> {
        if let Some(intrinsic) = self.program.intrinsics.get(&member_id).copied() {
            return Dispatch::Intrinsic(intrinsic);
        }
        if let Some(binding) = self
            .program
            .external_functions
            .get(&member_id)
            .and_then(|external| external.extern_binding.clone())
        {
            return Dispatch::Extern(member_id, binding);
        }
        let mut substitution = HashMap::default();
        self.bind_generics(impl_subject, type_id, &mut substitution);
        if !own_generic_values.is_empty() {
            if let Some(function) = self.program.functions.get(&member_id) {
                for (constraint_id, value) in function
                    .generic_parameter_constraint_ids
                    .iter()
                    .zip(own_generic_values.iter())
                {
                    substitution.insert(*constraint_id, *value);
                }
            }
        }
        let name = if substitution.is_empty() {
            self.ensure_function_emitted(member_id);
            self.ng.name_for(member_id)
        } else {
            self.emit_instance(member_id, &substitution)
        };
        let is_async = self.program.async_functions.contains(&member_id);
        Dispatch::Call(name, is_async)
    }

    /// A member provided by `type_id`'s impl OF `trait_id` specifically — the
    /// trait-scoped counterpart of `resolve_member_on_type`, immune to inherent
    /// name collisions.
    fn resolve_member_on_trait_impl(
        &self,
        type_id: TypeId,
        trait_id: Id,
        trait_arguments: &[TypeId],
        member: &str,
    ) -> Option<(Id, TypeId)> {
        let type_ = self.program.type_id_to_type_map.get(&type_id)?;
        self.program
            .implementations
            .iter()
            .filter(|implementation| {
                implementation.trait_ids.contains(&trait_id)
                    && !self.trait_instantiation_conflicts(
                        implementation,
                        trait_id,
                        trait_arguments,
                    )
                    && self
                        .program
                        .type_id_to_type_map
                        .get(&implementation.subject)
                        .is_some_and(|subject| nominal_matches(subject, type_))
            })
            .find_map(|implementation| {
                implementation
                    .declarations
                    .get(member)
                    .map(|member_id| (*member_id, implementation.subject))
            })
    }

    /// Lowers a resolved [`Dispatch`] to its call node with `args` (the receiver
    /// is the first argument). An async member is awaited.
    fn emit_dispatch(
        &mut self,
        dispatch: Dispatch<'src>,
        args: Vec<js::Node<'src>>,
        call_expr_id: Option<Id>,
    ) -> js::Node<'src> {
        match dispatch {
            Dispatch::Intrinsic(intrinsic) => self.emit_intrinsic(intrinsic, args, call_expr_id),
            Dispatch::Extern(member_id, binding) => {
                let call = self.emit_extern(member_id, binding, args);
                self.maybe_await(member_id, call)
            }
            Dispatch::Call(name, is_async) => {
                let call = js::Node::Call(Box::new(js::Node::Local(name)), args);
                if is_async {
                    js::Node::Await(Box::new(call))
                } else {
                    call
                }
            }
        }
    }

    /// Emits a trait default method specialized for a concrete type, keyed by
    /// (default, type) so each pairing is emitted once. While walking the body,
    /// `current_self_type` is the concrete type so its `self.method()` calls
    /// re-dispatch there, and `current_substitution` binds the TRAIT's own
    /// generic parameters to the arguments this type implements it at (B58) —
    /// so a `T`-typed value's bound-member call grounds the same way it does
    /// in a generic function's body.
    fn emit_default_instance(&mut self, default_id: Id, type_id: TypeId) -> String {
        let key = (default_id, self.type_key(type_id));
        if let Some(name) = self.default_instances.get(&key) {
            let name = name.clone();
            self.record_hit(|recorder| recorder.defaults.get(&key).copied());
            return name;
        }
        let name = self.ng.next_name();
        self.default_instances.insert(key.clone(), name.clone());
        if let Some(function) = self.program.functions.get(&default_id) {
            let emission = self.record_keyed(|recorder, id| {
                recorder.defaults.insert(key, id);
            });
            let substitution = self.trait_parameter_substitution(default_id, type_id);
            let saved_self = std::mem::replace(&mut self.current_self_type, Some(type_id));
            let saved_substitution =
                std::mem::replace(&mut self.current_substitution, substitution);
            let frame = self.record_enter();
            let js_function = self.function_with_name(function, name.clone());
            self.current_self_type = saved_self;
            self.current_substitution = saved_substitution;
            self.monomorphized.push(js_function);
            self.record_landed(emission);
            self.record_leave(frame, emission);
        }
        name
    }

    /// B58: the substitution a trait default body is specialized under — the
    /// trait's own generic parameters bound to the arguments `type_id`
    /// implements the trait at (`impl Box with Holder<Dog>` binds `T = Dog`),
    /// plus the providing impl's own binders bound from the concrete receiver
    /// (`impl Bag<type E> with Holder<E>` against `Bag<Dog>` binds `E = Dog`,
    /// which is what grounds a trait argument written in the impl's terms).
    ///
    /// Without this the body ran under an EMPTY substitution, so a call the
    /// analyzer resolved through `T`'s declared bound
    /// (`GenericDispatch::OnConstraint`) had nothing to ground `T` to and fell
    /// through to the trait's abstract member — which the never-silent guard
    /// (B55) now reports rather than emitting an empty body.
    fn trait_parameter_substitution(
        &self,
        default_id: Id,
        type_id: TypeId,
    ) -> HashMap<TypeId, TypeId> {
        let mut substitution = HashMap::default();
        let Some(type_) = self.program.type_id_to_type_map.get(&type_id) else {
            return substitution;
        };
        // The default's own trait — the one whose declarations hold it. A
        // supertrait's default reached through a subtrait's impl keeps its own
        // parameters, so key on the declaring trait, not the implemented one.
        let Some((trait_id, trait_)) = self
            .program
            .traits
            .iter()
            .find(|(_, trait_)| trait_.declarations.values().any(|id| *id == default_id))
        else {
            return substitution;
        };
        if trait_.generic_parameter_constraint_ids.is_empty() {
            return substitution;
        }
        // The impl of THAT trait for this type, matched nominally like every
        // other dispatch lookup here (the impl subject is in its own generic
        // terms, the receiver in concrete ones).
        let Some(implementation) = self.program.implementations.iter().find(|implementation| {
            implementation.trait_ids.contains(trait_id)
                && self
                    .program
                    .type_id_to_type_map
                    .get(&implementation.subject)
                    .is_some_and(|subject| nominal_matches(subject, type_))
        }) else {
            return substitution;
        };
        self.bind_generics(implementation.subject, type_id, &mut substitution);
        let Some((_, arguments)) = implementation
            .trait_args
            .iter()
            .find(|(provided, _)| provided == trait_id)
        else {
            return substitution;
        };
        // A trait argument written in the impl's terms (`with Holder<E>`)
        // stays keyed to the binder above, so `resolve_type_id` composes the
        // two hops within this same map.
        for (parameter_id, argument_id) in trait_
            .generic_parameter_constraint_ids
            .iter()
            .zip(arguments)
        {
            substitution.insert(*parameter_id, *argument_id);
        }
        substitution
    }

    /// Whether a scope needs `try`/`finally` teardown: some direct statement
    /// declares a resource this scope drops (destruction.md §7) whose destruction
    /// is not a complete no-op. A resource-free program never hits this (empty
    /// `dropped_bindings`), so its output stays byte-identical.
    fn scope_needs_drops(&self, statements: &[Id]) -> bool {
        statements
            .iter()
            .any(|statement| !matches!(self.statement_teardown(*statement), ScopeTeardown::None))
    }

    /// What a direct statement of a scope owes at the scope's end: a resource
    /// `let`'s teardown, or — B62 — the resource payloads a `let`-pattern
    /// captured out of a consumed subject. Nothing for every other statement.
    fn statement_teardown(&self, statement: Id) -> ScopeTeardown {
        match self.program.entity_map.get(&statement) {
            Some(Expr::Variable(variable_id))
                if self.program.dropped_bindings.contains(variable_id)
                    && self.binding_drops_nontrivially(*variable_id) =>
            {
                ScopeTeardown::Binding(*variable_id)
            }
            Some(Expr::Destructure(_, pattern)) => match self.droppable_pattern_captures(pattern) {
                captures if captures.is_empty() => ScopeTeardown::None,
                captures => ScopeTeardown::Captures(captures),
            },
            _ => ScopeTeardown::None,
        }
    }

    /// The pattern's captures this scope must destroy, in declaration order: the
    /// ones the drop planner left owning a resource payload at their scope's end
    /// (B62). Empty for every data pattern, and for a capture moved onward.
    fn droppable_pattern_captures(&self, pattern: &ExprPattern) -> Vec<Id> {
        Self::pattern_capture_ids(pattern)
            .into_iter()
            .filter(|capture_id| {
                self.program.dropped_bindings.contains(capture_id)
                    && self.binding_drops_nontrivially(*capture_id)
            })
            .collect()
    }

    /// The `finally` nodes for a pattern's owned captures, in REVERSE declaration
    /// order — the order a scope's own locals drop in.
    ///
    /// The value destroyed is the capture's binding on the DECLARED path
    /// (`compile_pattern`) and its subject-slot accessor on the ALIASED path
    /// (`compile_is_pattern`, which a guarded leg uses): a resource never copies,
    /// so a guarded leg's capture is still an accessor here, and the slot it
    /// names has exactly one owner because the match consumed the subject.
    fn capture_drop_nodes(&mut self, captures: Vec<Id>) -> Vec<js::Node<'src>> {
        let mut drops = Vec::new();
        for capture_id in captures.into_iter().rev() {
            let Some(type_id) = self
                .program
                .variables
                .get(&capture_id)
                .map(|variable| variable.type_id)
            else {
                continue;
            };
            let accessor = self.is_bindings.get(&capture_id).cloned();
            let value = accessor.unwrap_or_else(|| js::Node::Local(self.ng.name_for(capture_id)));
            if let Some(drop) = self.slot_drop_node(capture_id, type_id, value) {
                drops.push(drop);
            }
        }
        drops
    }

    /// Whether a dropped binding's type actually destroys something (a `Drop` impl
    /// or a resource member) — as opposed to a bare `resource external` leaf with
    /// no destructor, whose scope-end drop is a no-op.
    fn binding_drops_nontrivially(&self, variable_id: Id) -> bool {
        self.program
            .variables
            .get(&variable_id)
            .is_some_and(|variable| self.type_drops_nontrivially(variable.type_id))
    }

    fn type_drops_nontrivially(&self, type_id: TypeId) -> bool {
        self.program
            .drop_glue
            .get(&type_id)
            .is_some_and(|glue| glue.drop_method.is_some() || !glue.members.is_empty())
    }

    /// Emit a scope body (statements + tail) with per-resource `try`/`finally`
    /// teardown (destruction.md §7, as amended by `lifetimes.md` §6). Each owned
    /// resource declaration is emitted, then the statements it stays live across
    /// are wrapped in a `try` whose `finally` drops it — declarations stay
    /// OUTSIDE their own `try` (a panic mid-acquisition never drops an
    /// unacquired value), and nested tries discharge in reverse declaration
    /// order. `ret` / `break` / `continue` / a thrown panic all leave through
    /// the finallys natively.
    ///
    /// **S3: the region ends at the LAST USE, not at the scope's end.**
    /// [`Self::teardown_extent`] answers where, as an exclusive statement index;
    /// the scope then continues *after* the `try` instead of being nested inside
    /// it. An extent of `end` reproduces the shipped scope-end shape exactly,
    /// which is what an opaque binding falls back to.
    ///
    /// `start`/`end` bound the statement range this call owns, and `tail` is
    /// `Some` only when the range reaches the scope's own end — a range cut
    /// short by an earlier drop point has no tail to emit.
    fn walk_scope_body(
        &mut self,
        statements: &[Id],
        start: usize,
        end: usize,
        tail: Option<(Id, TailDisposition)>,
    ) -> Vec<js::Node<'src>> {
        let mut out: Vec<js::Node<'src>> = Vec::new();
        let mut tail = tail;
        let mut index = start;
        while index < end {
            let statement = statements[index];
            let teardown = self.statement_teardown(statement);
            if !matches!(teardown, ScopeTeardown::None) {
                // Emit the declaration (outside its own `try`).
                self.emit_statement(statement, &mut out);
                let extent = self.teardown_extent(&teardown, statements, index, end);
                let inner_tail = if extent == end { tail.take() } else { None };
                // A binding nothing reads again drops right here, and an empty
                // `try` would be the only thing between the acquisition and the
                // drop: there is no window for a throw to escape through, so
                // the bare call is the same program, shorter.
                let region_is_empty = extent == index + 1 && inner_tail.is_none();
                // The region is walked BEFORE the teardown is built, in both
                // shapes: `capture_drop_nodes` reads the `is_bindings` alias
                // table the walk fills in, and every minted name is drawn from
                // the same generator, so building the drop first would rename
                // every helper after it.
                let inner = (!region_is_empty)
                    .then(|| self.walk_scope_body(statements, index + 1, extent, inner_tail));
                let teardown_nodes = match teardown {
                    ScopeTeardown::None => Vec::new(),
                    ScopeTeardown::Binding(variable_id) => {
                        let type_id = self.program.variables.get(&variable_id).unwrap().type_id;
                        let value = js::Node::Local(self.ng.name_for(variable_id));
                        self.slot_drop_node(variable_id, type_id, value)
                            .map(|node| vec![node])
                            .unwrap_or_default()
                    }
                    ScopeTeardown::Captures(captures) => self.capture_drop_nodes(captures),
                };
                match inner {
                    Some(inner) => out.push(js::Node::Try(inner, teardown_nodes)),
                    None => out.extend(teardown_nodes),
                }
                index = extent;
                continue;
            }
            self.emit_statement(statement, &mut out);
            index += 1;
        }
        if let Some((tail, disposition)) = tail {
            self.emit_scope_tail(tail, disposition, &mut out);
        }
        out
    }

    /// Where a declaration's teardown region ends — an EXCLUSIVE index into
    /// `statements`, never past `end` and never before `declaration + 1`.
    ///
    /// The analyzer answers per BINDING ([`DropExtent`], `lifetimes.md` §6) with
    /// the chain of statements enclosing the last read, outermost first; this
    /// picks the chain element that is a direct statement of the range being
    /// emitted. Three refusals all fall back to `end`, which is the scope-end
    /// law that shipped: no answer at all (an opaque binding — a capture, a
    /// cross-region read, an unfollowable loan), an explicit `ScopeEnd`, and a
    /// chain naming no statement of this range (the read is in the scope's tail
    /// or somewhere this walk does not emit).
    ///
    /// A last read *inside* a branch or a loop resolves to that branch or loop
    /// STATEMENT, so the drop lands at the join and every path through it —
    /// taken, not-taken, `ret`, `jump` — releases through the one `finally`.
    /// That is §6.3's drop specialization with no runtime flag anywhere.
    ///
    /// A `ScopeTeardown::Captures` group shares one region, so the group's
    /// extent is the LAST of its members' — simultaneous discharge, in the
    /// reverse declaration order `capture_drop_nodes` already emits.
    ///
    /// **Regions nest.** The region lowers to a JS block, so every `const` a
    /// statement inside it declares dies at its brace: the region is widened
    /// until it covers the last read of every name declared within it (a
    /// fixpoint — widening admits more declarations, which may widen again).
    /// Without that the emitted program reads a name out of scope, which is how
    /// this was found (`owner.enter(…)`'s result, read after the owner's drop
    /// point). The widening question is deliberately SYNTACTIC — see
    /// `liveness::LastUse::syntactic_extent`: block scope is about where a name
    /// may be written down, not about when a value may be destroyed.
    fn teardown_extent(
        &self,
        teardown: &ScopeTeardown,
        statements: &[Id],
        declaration: usize,
        end: usize,
    ) -> usize {
        let own = self.own_teardown_extent(teardown, statements, declaration + 1, end);
        self.widen_over_declarations(own, statements, declaration + 1, end)
    }

    /// Grow `extent` until every name declared in `statements[start..extent]`
    /// has its last read inside it. Monotone and bounded by `end`.
    fn widen_over_declarations(
        &self,
        mut extent: usize,
        statements: &[Id],
        start: usize,
        end: usize,
    ) -> usize {
        loop {
            let mut widened = extent;
            for index in start..extent {
                let Some(declared) = self
                    .program
                    .declared_binding_extents
                    .get(&statements[index])
                else {
                    continue;
                };
                for binding_extent in declared {
                    // Measured from the declaring statement itself, not after
                    // it: a `for` item or an `is` capture has its last read
                    // INSIDE the statement that declares it, and resolving from
                    // the next one would find no chain element and refuse.
                    widened =
                        widened.max(Self::resolve_extent(binding_extent, statements, index, end));
                }
            }
            if widened == extent {
                return extent;
            }
            extent = widened;
        }
    }

    /// One [`DropExtent`] resolved against a statement range: the exclusive
    /// index its last read sits at, `start` when nothing reads it, and `end`
    /// for every refusal (an explicit scope end, or a chain naming no statement
    /// of this range — the read is in the scope's tail).
    fn resolve_extent(extent: &DropExtent, statements: &[Id], start: usize, end: usize) -> usize {
        let start = start.min(end);
        match extent {
            DropExtent::ScopeEnd => end,
            DropExtent::Declaration => start,
            DropExtent::Statement(chain) => {
                let region = &statements[start..end];
                match chain
                    .iter()
                    .find_map(|holder| region.iter().position(|s| s == holder))
                {
                    Some(offset) => start + offset + 1,
                    None => end,
                }
            }
        }
    }

    /// One teardown's own extent, before nesting is taken into account: the
    /// exclusive statement index its last use sits at, `start` when nothing
    /// reads it, and `end` for every refusal.
    fn own_teardown_extent(
        &self,
        teardown: &ScopeTeardown,
        statements: &[Id],
        start: usize,
        end: usize,
    ) -> usize {
        let bindings: &[Id] = match teardown {
            ScopeTeardown::None => return end,
            ScopeTeardown::Binding(binding) => std::slice::from_ref(binding),
            ScopeTeardown::Captures(captures) => captures.as_slice(),
        };
        let mut extent = start.min(end);
        for binding in bindings {
            let Some(binding_extent) = self.program.drop_extents.get(binding) else {
                return end;
            };
            extent = extent.max(Self::resolve_extent(binding_extent, statements, start, end));
        }
        extent.min(end)
    }

    /// Emit a loop body's nodes (statements + discarded tail), with per-resource
    /// `try`/`finally` teardown when it owns resource locals (destruction.md §7)
    /// so they drop each iteration and `jump break`/`continue` leave through the
    /// finally; a resource-free body emits exactly as before.
    fn walk_loop_body_nodes(&mut self, statements: &[Id], tail: Id) -> Vec<js::Node<'src>> {
        if self.scope_needs_drops(statements) {
            self.walk_scope_body(
                statements,
                0,
                statements.len(),
                Some((tail, TailDisposition::Discard)),
            )
        } else {
            let mark = self.pending_temporaries.len();
            let mut body = self.walk_list(statements);
            match self.program.entity_map.get(&tail) {
                Some(Expr::Void) | None => {}
                Some(_) => {
                    if let Some(node) = self.walk_entity(tail, &mut body) {
                        if !matches!(node, js::Node::Void) {
                            body.push(node);
                        }
                    }
                }
            }
            self.seal_pending_temporaries(mark, &mut body);
            body
        }
    }

    /// Emit an `if`/`match` arm body (statements + value tail), with per-resource
    /// `try`/`finally` teardown when it owns resource locals (destruction.md §7).
    /// A value-producing arm assigns its result to the shared result temp inside
    /// the drop scope (so teardown precedes the branch value); a resource-free arm
    /// emits exactly as before.
    fn walk_branch_body(
        &mut self,
        statements: &[Id],
        tail: Id,
        result_name: &mut Option<String>,
    ) -> Vec<js::Node<'src>> {
        let has_value = !matches!(self.program.entity_map.get(&tail), None | Some(Expr::Void));
        if self.scope_needs_drops(statements) {
            if has_value {
                let name = result_name
                    .get_or_insert_with(|| self.ng.next_name())
                    .clone();
                self.walk_scope_body(
                    statements,
                    0,
                    statements.len(),
                    Some((tail, TailDisposition::ResultOrDivergence(name))),
                )
            } else {
                self.walk_scope_body(
                    statements,
                    0,
                    statements.len(),
                    Some((tail, TailDisposition::Discard)),
                )
            }
        } else {
            let mark = self.pending_temporaries.len();
            let mut body = self.walk_list(statements);
            if has_value {
                let value = self.walk_entity(tail, &mut body);
                // The result temp is named whether or not the tail yields one, so
                // the sibling arms and every later temp keep their names.
                let name = result_name
                    .get_or_insert_with(|| self.ng.next_name())
                    .clone();
                // A tail that emitted itself and reported no value (a block that
                // LEAVES — B152) has nothing to assign; the arm already diverged.
                if let Some(value) = value {
                    self.push_result_or_divergence(&name, value, &mut body);
                }
            }
            self.seal_pending_temporaries(mark, &mut body);
            body
        }
    }

    /// Emit a restructured scope's tail into `out` per its disposition — the same
    /// handling the un-restructured paths use, so a scope that ends up wrapping
    /// nothing stays byte-identical.
    fn emit_scope_tail(
        &mut self,
        tail: Id,
        disposition: TailDisposition,
        out: &mut Vec<js::Node<'src>>,
    ) {
        // A temporary in TAIL position closes around the `return` / assignment
        // the tail becomes, not before it: the value must be computed before any
        // teardown runs (P11), which is exactly what `finally` gives.
        let mark = self.pending_temporaries.len();
        let base = out.len();
        let emitted = self.emit_scope_tail_node(tail, disposition, out);
        if emitted {
            self.close_temporaries(mark, base, out);
        }
    }

    fn emit_scope_tail_node(
        &mut self,
        tail: Id,
        disposition: TailDisposition,
        out: &mut Vec<js::Node<'src>>,
    ) -> bool {
        let Some(node) = self.walk_entity(tail, out) else {
            return false;
        };
        if matches!(node, js::Node::Void) {
            return true;
        }
        // A tail that already leaves the scope (`ret` / `jump`) is emitted as the
        // statement it is under EVERY disposition — returning or assigning one
        // is B152's unparseable `return return 1;` / `t = return 1;`.
        if node.is_divergent() {
            out.push(node);
            return true;
        }
        match disposition {
            TailDisposition::Return => out.push(js::Node::Return(Box::new(node))),
            TailDisposition::Discard => out.push(node),
            TailDisposition::AssignTo(name) => out.push(js::Node::Assignment(
                Box::new(js::Node::Local(name)),
                Box::new(node),
            )),
            TailDisposition::ResultOrDivergence(name) => {
                self.push_result_or_divergence(&name, node, out)
            }
        }
        true
    }

    /// The net under [`Self::close_temporaries`]: seal a freshly built BODY, so
    /// a lifted `const` can never outlive the list it was declared in.
    ///
    /// Every statement list closes its own temporaries as it goes; this catches
    /// the ones lifted out of a TAIL expression, where there is no next
    /// statement to close them at. Sealing at the body's start gives the
    /// longest correct region — the value is held until the body's own value
    /// has been produced, which is what a tail-position temporary needs (P11)
    /// and what the `finally` around a `return` already does.
    fn seal_pending_temporaries(&mut self, mark: usize, body: &mut Vec<js::Node<'src>>) {
        self.close_temporaries(mark, 0, body);
    }

    /// Split a statement's own declaration in two when a temporary's `try` is
    /// about to close over it: `const n = f(t)` becomes `let n;` before the
    /// region and `n = f(t)` inside it.
    ///
    /// Without this, `let size = File::open(p).stat().size` puts `size` inside
    /// the block that destroys the handle and every later read of it is a
    /// `ReferenceError`. The declaration goes to the START of the statement's
    /// nodes — ahead of every lifted `const`, not just the innermost — so no
    /// enclosing region can swallow it either; the pending indices shift with
    /// it.
    fn hoist_declaration_out_of(
        &mut self,
        mark: usize,
        base: usize,
        block: &mut Vec<js::Node<'src>>,
    ) {
        let (name, value) = match block.last() {
            Some(js::Node::ConstVariable(variable) | js::Node::LetVariable(variable)) => {
                (variable.name.clone(), variable.value.clone())
            }
            _ => return,
        };
        block.pop();
        block.push(js::Node::Assignment(
            Box::new(js::Node::Local(name.clone())),
            value,
        ));
        block.insert(
            base,
            js::Node::LetVariable(js::Variable {
                name,
                value: Box::new(js::Node::Void),
            }),
        );
        for pending in &mut self.pending_temporaries[mark..] {
            pending.at += 1;
        }
    }

    /// The drop statement for a value of `type_id` (destruction.md §5/§7), or
    /// `None` when destruction is a no-op. A direct call to the type's `__drop`
    /// helper, which runs the impl's `drop` then destroys the fields.
    fn resource_drop_of(
        &mut self,
        type_id: TypeId,
        value: js::Node<'src>,
    ) -> Option<js::Node<'src>> {
        let helper = self.ensure_drop_helper(type_id)?;
        Some(js::Node::Call(
            Box::new(js::Node::Local(helper)),
            vec![value],
        ))
    }

    /// Whether an emitted expression can neither throw nor observe a
    /// destructor: a tree of literals and plain reads of already-declared
    /// locals. R2's overwrite drop may stay AHEAD of such a right-hand side
    /// (B151) — there is no window between the drop and the write for a throw
    /// to escape through, and nothing in the expression can read the value
    /// being destroyed — so both orders emit the same program and the shorter
    /// one is kept. Anything else (a call, a property read, an `await`) gets
    /// the temporary.
    fn node_is_inert(node: &js::Node<'src>) -> bool {
        match node {
            js::Node::Local(_)
            | js::Node::Number(_, _)
            | js::Node::String(_)
            | js::Node::Bool(_)
            | js::Node::Null
            | js::Node::Void => true,
            js::Node::Array(items) => items.iter().all(Self::node_is_inert),
            js::Node::Spread(inner) => Self::node_is_inert(inner),
            _ => false,
        }
    }

    /// The binding an expression NAMES, when it is a bare place: a local, a
    /// pattern capture, or a parameter (which reaches expression position as an
    /// `Expr::Local` of the parameter's id). `None` for a value expression,
    /// which owns no slot anyone else can read.
    fn place_binding_of(&self, expr_id: Id) -> Option<Id> {
        match self.program.entity_map.get(&expr_id) {
            Some(Expr::Local(binding) | Expr::Parameter(binding)) => Some(*binding),
            _ => None,
        }
    }

    /// Whether `binding`'s teardown is the GUARDED half of B150's pair: an
    /// explicit `drop(x)` reaches it, so the value may already be gone by the
    /// time the scope's `finally` runs. Every other binding keeps the bare,
    /// unconditional drop it always had, which is what keeps a program that
    /// never calls the sink byte-identical.
    fn slot_is_emptied_early(&self, binding: Id) -> bool {
        self.program.explicit_drop_bindings.contains(&binding)
            && self.program.dropped_bindings.contains(&binding)
    }

    /// The scope-end teardown statement for `binding`, reading `slot`.
    ///
    /// For an ordinary owner that is the bare destructor call. For a binding an
    /// explicit `drop(x)` may already have destroyed (B150) it is the same call
    /// under the emptiness test the sink's `slot = null` answers — the emitted
    /// discriminant test the enum drop glue already uses, applied to a whole
    /// slot rather than a payload. This is emission machinery, not a semantic
    /// drop flag: mR7 bans runtime flags for CONDITIONAL moves, and R7 rejects
    /// a conditional `drop(x)` outright, so the pair guards an UNCONDITIONAL
    /// early teardown against a path that never reached it.
    fn slot_drop_node(
        &mut self,
        binding: Id,
        type_id: TypeId,
        slot: js::Node<'src>,
    ) -> Option<js::Node<'src>> {
        let drop = self.resource_drop_of(type_id, slot.clone())?;
        if !self.slot_is_emptied_early(binding) {
            return Some(drop);
        }
        Some(js::Node::If(js::IfBranch::If(
            Box::new(js::Node::Binary(
                BinaryOp::NotEq,
                Box::new(slot),
                Box::new(js::Node::Null),
            )),
            vec![drop],
            None,
        )))
    }

    /// Emit (once) the per-type `__drop` helper for `type_id` and return its name,
    /// or `None` if the type destroys nothing. The helper runs the value's own
    /// `drop(&mut self)` first (so it cannot resurrect itself), then destroys each
    /// resource member in reverse order (destruction.md §5): struct/tuple/array
    /// slots directly, enum payloads under a tag test.
    fn ensure_drop_helper(&mut self, type_id: TypeId) -> Option<String> {
        let key = self.type_key(type_id);
        if let Some(existing) = self.drop_helpers.get(&key) {
            let existing = existing.clone();
            self.record_hit(|recorder| recorder.drops.get(&key).copied());
            return existing;
        }
        let Some(glue) = self.program.drop_glue.get(&type_id).cloned() else {
            self.drop_helpers.insert(key, None);
            return None;
        };
        if glue.drop_method.is_none() && glue.members.is_empty() {
            self.drop_helpers.insert(key, None);
            return None;
        }
        // Register the name BEFORE building the body, so a self-referential
        // resource (`struct Node { next: Option<Node> }`) terminates.
        let name = self.ng.next_name();
        self.drop_helpers.insert(key.clone(), Some(name.clone()));
        let emission = self.record_keyed(|recorder, id| {
            recorder.drops.insert(key, id);
        });
        let frame = self.record_enter();
        let value_name = self.ng.next_name();
        let value = || js::Node::Local(value_name.clone());
        let mut body: Vec<js::Node<'src>> = Vec::new();
        // 1. The value's own destructor, before its fields. The receiver is the
        //    only argument. LIMITATION (destruction.md §8 Turns): a `drop` body
        //    that requires an ambient context — writing a `Signal` threads the
        //    turn as a hidden context argument — needs that context forwarded
        //    here, which this generated helper does not do. Such a drop is
        //    unsupported in this slice; std's resource drops (Database close,
        //    OwnedNursery cancel) need no context, so nothing here hits it.
        if let Some(drop_method) = glue.drop_method {
            self.ensure_function_emitted(drop_method);
            body.push(js::Node::Call(
                Box::new(js::Node::Local(self.ng.name_for(drop_method))),
                vec![value()],
            ));
        }
        // 2. The resource members, in reverse declaration order.
        match &glue.members {
            crate::analyzer::DropMembers::Fields(fields) => {
                for (index, member_type) in fields.iter().rev() {
                    let slot = js::Node::PropertyIndex(
                        Box::new(value()),
                        Box::new(js::Node::Number(index.to_string(), None)),
                    );
                    if let Some(drop) = self.resource_drop_of(*member_type, slot) {
                        body.push(drop);
                    }
                }
            }
            crate::analyzer::DropMembers::Variants(variants) => {
                for (variant_index, slots) in variants {
                    let mut arm: Vec<js::Node<'src>> = Vec::new();
                    for (slot, member_type) in slots.iter().rev() {
                        let payload = js::Node::PropertyIndex(
                            Box::new(value()),
                            Box::new(js::Node::Number(slot.to_string(), None)),
                        );
                        if let Some(drop) = self.resource_drop_of(*member_type, payload) {
                            arm.push(drop);
                        }
                    }
                    if !arm.is_empty() {
                        let test = js::Node::Binary(
                            BinaryOp::Eq,
                            Box::new(js::Node::PropertyIndex(
                                Box::new(value()),
                                Box::new(js::Node::Number("0".to_string(), None)),
                            )),
                            Box::new(js::Node::Number(variant_index.to_string(), None)),
                        );
                        body.push(js::Node::If(js::IfBranch::If(Box::new(test), arm, None)));
                    }
                }
            }
        }
        self.monomorphized.push(js::Node::Function(js::Function {
            name: name.clone(),
            parameters: vec![js::Parameter { name: value_name }],
            body,
            is_async: false,
        }));
        self.record_landed(emission);
        self.record_leave(frame, emission);
        Some(name)
    }

    /// Resolves `member` as an inherited trait *default* on a concrete type — a
    /// member none of the type's impls declare, but a (super)trait it implements
    /// provides with a body. Mirrors the analyzer's Gap E resolution.
    fn resolve_inherited_default(&self, type_id: TypeId, member: &str) -> Option<Id> {
        let type_ = self.program.type_id_to_type_map.get(&type_id)?.clone();
        self.program
            .implementations
            .iter()
            .filter(|implementation| {
                // NOMINAL matching, like `resolve_member_on_type`: the impl
                // subject is written in its own generic terms (`Signal<T>`),
                // the receiver in concrete ones (`Signal<i32>`) — exact type
                // equality only ever matched non-generic subjects, silently
                // dropping inherited defaults on generic types (the emitted
                // call then bound to the trait's abstract member).
                self.program
                    .type_id_to_type_map
                    .get(&implementation.subject)
                    .is_some_and(|subject| nominal_matches(subject, &type_))
            })
            .flat_map(|implementation| implementation.trait_ids.iter().copied())
            .find_map(|trait_id| self.trait_default_member(trait_id, member))
    }

    /// Searches a trait and its supertraits for a default (bodied) member.
    fn trait_default_member(&self, trait_id: Id, member: &str) -> Option<Id> {
        let mut stack = vec![trait_id];
        let mut seen = HashSet::default();
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(trait_) = self.program.traits.get(&id) else {
                continue;
            };
            if let Some(&member_id) = trait_.declarations.get(member) {
                if self.function_has_body(member_id) {
                    return Some(member_id);
                }
            }
            for supertrait_type_id in &trait_.supertraits {
                if let Some(Type::Trait(super_id, _)) =
                    self.program.type_id_to_type_map.get(supertrait_type_id)
                {
                    stack.push(*super_id);
                }
            }
        }
        None
    }

    /// Whether `member_id` is a function with a source-provided body (a trait
    /// default, as opposed to a signature-only requirement).
    fn function_has_body(&self, member_id: Id) -> bool {
        match self.program.entity_map.get(&member_id) {
            Some(Expr::Function(function_id)) => self
                .program
                .functions
                .get(function_id)
                .map(|function| function.has_body)
                .unwrap_or(false),
            _ => false,
        }
    }

    /// The generic binding to monomorphize a call's callee with, drawn from
    /// whichever channel carries it — so the transformer reads a call's binding in
    /// one place and emits through the one [`Self::emit_instance`] path. In
    /// precedence order: a free generic call's positional type arguments
    /// (`id<i32>` -> `{T: i32}`); the receiver / own-generic substitution the
    /// analyzer recorded for a method or operator (`xs.sum()` on `List<i32>`); or,
    /// for a generic call nested in a monomorphized body whose arguments come only
    /// from the enclosing instantiation, the inherited slice of the active
    /// substitution. `None` means the callee is non-generic (or nothing binds it),
    /// so it is emitted as a plain function.
    fn call_substitution(
        &self,
        call_id: Id,
        target_id: Id,
        generic_argument_ids: &[TypeId],
    ) -> Option<HashMap<TypeId, TypeId>> {
        let function = self.program.functions.get(&target_id);
        let is_generic = function.is_some_and(|f| !f.generic_parameter_constraint_ids.is_empty());
        if is_generic && !generic_argument_ids.is_empty() {
            return Some(
                function
                    .unwrap()
                    .generic_parameter_constraint_ids
                    .iter()
                    .copied()
                    .zip(generic_argument_ids.iter().copied())
                    .collect(),
            );
        }
        if let Some(recorded) = self.program.method_call_substitution.get(&call_id) {
            return Some(recorded.clone());
        }
        let inherited = self.inherited_substitution(target_id);
        (!inherited.is_empty()).then_some(inherited)
    }

    /// Emits (or reuses) a monomorphized instance of `function_id` specialized by
    /// `substitution` (generic constraint id -> concrete type). This is the single
    /// monomorphization path for *every* generic instantiation — free function,
    /// impl/trait method, operator, nested call — so a binding flows through one
    /// place regardless of how it was recorded. Keyed by (function, bound types)
    /// so each instantiation is emitted once. While walking the body,
    /// `current_substitution` is the binding, so `T::default()` and `T`-typed
    /// values resolve concretely.
    fn emit_instance(&mut self, function_id: Id, substitution: &HashMap<TypeId, TypeId>) -> String {
        self.emit_instance_with_bits(function_id, substitution, &[])
    }

    /// Emits one monomorphized instance: a type substitution plus the
    /// adapted-asyncness bits (async-polymorphism.md A.1 — which closure
    /// parameters are async in this instance). The bits join the memo key,
    /// so `map` over a sync closure and over an async one are distinct
    /// emissions; they are independent of the type substitution.
    fn emit_instance_with_bits(
        &mut self,
        function_id: Id,
        substitution: &HashMap<TypeId, TypeId>,
        bits: &[Id],
    ) -> String {
        // Resolve each bound type under the active substitution (so a nested
        // instantiation composes) and order by constraint id for a stable key.
        let mut entries: Vec<(TypeId, TypeId)> = substitution
            .iter()
            .map(|(constraint_id, type_id)| (*constraint_id, self.resolve_type_id(*type_id)))
            .collect();
        entries.sort_by_key(|(constraint_id, _)| constraint_id.0);
        let key = (
            function_id,
            entries
                .iter()
                .map(|(_, type_id)| self.type_key(*type_id))
                .collect::<Vec<_>>(),
            bits.to_vec(),
        );
        if let Some(name) = self.instances.get(&key) {
            let name = name.clone();
            self.record_hit(|recorder| recorder.instances.get(&key).copied());
            return name;
        }
        let substitution: HashMap<TypeId, TypeId> = entries.into_iter().collect();
        let name = self.ng.next_name();
        self.instances.insert(key.clone(), name.clone());
        if let Some(function) = self.program.functions.get(&function_id) {
            let emission = self.record_keyed(|recorder, id| {
                recorder.instances.insert(key, id);
            });
            let saved = std::mem::replace(&mut self.current_substitution, substitution);
            let saved_instance = self.enter_instance(function_id, bits.to_vec());
            let frame = self.record_enter();
            let js_function = self.function_with_name(function, name.clone());
            self.restore_instance(saved_instance);
            self.current_substitution = saved;
            self.monomorphized.push(js_function);
            self.record_landed(emission);
            self.record_leave(frame, emission);
        }
        name
    }

    /// Swap in the adapted-instance context for a body about to be emitted;
    /// returns the previous context for `restore_instance`. Also tracks the
    /// function's source name as the spawn origin for `__task` calls.
    fn enter_instance(
        &mut self,
        function_id: Id,
        bits: Vec<Id>,
    ) -> (
        Vec<Id>,
        Option<crate::analyzer::AdaptedInstance>,
        Option<&'src str>,
    ) {
        let info = self
            .program
            .adapted_instances
            .get(&(function_id, bits.clone()))
            .cloned();
        let origin = self
            .program
            .functions
            .get(&function_id)
            .map(|function| function.name);
        (
            std::mem::replace(&mut self.current_adapted, bits),
            std::mem::replace(&mut self.current_instance, info),
            std::mem::replace(&mut self.current_origin, origin),
        )
    }

    fn restore_instance(
        &mut self,
        saved: (
            Vec<Id>,
            Option<crate::analyzer::AdaptedInstance>,
            Option<&'src str>,
        ),
    ) {
        self.current_adapted = saved.0;
        self.current_instance = saved.1;
        self.current_origin = saved.2;
    }

    /// The bindings the active substitution provides for the generics a callee's
    /// signature mentions — used to specialize a generic call whose type
    /// arguments come only from the enclosing monomorphization (so the analysis
    /// recorded no substitution of its own). Empty when nothing applies, so the
    /// caller falls back to a plain (generic) emission.
    fn inherited_substitution(&self, target_id: Id) -> HashMap<TypeId, TypeId> {
        if self.current_substitution.is_empty() {
            return HashMap::default();
        }
        let Some(function) = self.program.functions.get(&target_id) else {
            return HashMap::default();
        };
        let mut generics = Vec::new();
        for parameter_id in &function.parameters {
            if let Some(parameter) = self.program.parameters.get(parameter_id) {
                self.collect_type_generics(parameter.type_id, 0, &mut generics);
            }
        }
        if let Some(return_type_id) = function.return_type_id {
            self.collect_type_generics(return_type_id, 0, &mut generics);
        }
        generics
            .into_iter()
            .filter_map(|constraint_id| {
                self.current_substitution
                    .get(&constraint_id)
                    .map(|type_id| (constraint_id, *type_id))
            })
            .collect()
    }

    /// Collects the `Generic` constraint ids a type's structure mentions (its own
    /// id, or those nested in a struct/enum/tuple/closure's arguments).
    fn collect_type_generics(&self, type_id: TypeId, depth: usize, out: &mut Vec<TypeId>) {
        if depth > 24 {
            return;
        }
        match self.program.type_id_to_type_map.get(&type_id) {
            Some(Type::Generic(constraint_id)) => {
                if !out.contains(constraint_id) {
                    out.push(*constraint_id);
                }
            }
            Some(
                Type::Struct(_, arguments) | Type::Enum(_, arguments) | Type::Tuple(arguments),
            ) => {
                for argument in arguments.clone() {
                    self.collect_type_generics(argument, depth + 1, out);
                }
            }
            Some(Type::Closure(parameters, return_type_id)) => {
                let parameters = parameters.clone();
                let return_type_id = *return_type_id;
                for parameter in parameters {
                    self.collect_type_generics(parameter, depth + 1, out);
                }
                self.collect_type_generics(return_type_id, depth + 1, out);
            }
            Some(Type::Array(element_id, _)) => {
                self.collect_type_generics(*element_id, depth + 1, out);
            }
            _ => {}
        }
    }

    /// Resolves a type id to its concrete form under the active substitution,
    /// following generic parameters to the type they're currently bound to.
    /// The resolved type id of an expression, used for tuple flat-layout
    /// decisions. Falls back through a binding reference to the binding's type
    /// (a bare `Expr::Local`/`Parameter` use carries no type on its own id).
    fn expr_type_id(&self, expr_id: Id) -> Option<TypeId> {
        if let Some(type_id) = self.program.expr_type_ids.get(&expr_id) {
            return Some(*type_id);
        }
        match self.program.entity_map.get(&expr_id)? {
            Expr::Local(binding) | Expr::Variable(binding) => {
                self.program.variables.get(binding).map(|v| v.type_id)
            }
            Expr::Parameter(binding) => self.program.parameters.get(binding).map(|p| p.type_id),
            _ => None,
        }
    }

    /// The type of a `drop(x)` argument, for the early-teardown rewrite. Like
    /// `expr_type_id` but a bare `Expr::Local` of a PARAMETER id also resolves (a
    /// plain `drop(param)` would otherwise read as untyped and no-op, leaking the
    /// parameter), and a VALUE argument — a call result, which stores no type on
    /// its own id and names no binding — resolves through the analyzer's B68
    /// recording (`drop_sink_value_types`, affine-moves.md §9.4). Kept separate
    /// from `expr_type_id` so the tuple/set layout decisions that read it stay
    /// byte-identical.
    fn drop_argument_type_id(&self, expr_id: Id) -> Option<TypeId> {
        self.expr_type_id(expr_id)
            .or_else(|| match self.program.entity_map.get(&expr_id)? {
                Expr::Local(binding) => self.program.parameters.get(binding).map(|p| p.type_id),
                _ => None,
            })
            .or_else(|| self.program.drop_sink_value_types.get(&expr_id).copied())
    }

    /// Whether an expression's (monomorphized) type is a tuple — its value is a
    /// flat array whose slots splice into a constructed tuple. A tuple literal is
    /// recognized structurally (its own id carries no stored type); anything else
    /// is decided by its resolved type.
    ///
    /// B70 (variadic-generics.md §T.8): the general cache answers only for the
    /// forms that *store* a type. An element written as a call, an `if`, a
    /// block, an `await`, a method call, a `*view` or a plain parameter is typed
    /// on demand and stored nowhere, so this read came back silent and the
    /// element nested instead of splicing — flat storage broken, every read past
    /// it `undefined`. `tuple_element_types` is the type the tuple rule computed
    /// for that very element and it covers every form; it is consulted second so
    /// an expression that already answered keeps its answer byte for byte.
    ///
    /// The element entry is read UNRESOLVED, unlike the general one. The
    /// analyzer bakes a `.n` read's flat offset into the AST from
    /// `tuple_flat_width`, which counts a still-generic element as one slot
    /// because a generic body is walked once for every instantiation — so
    /// splicing one would move every offset past it. Reading the entry as
    /// written keeps emission and those offsets on the same layout.
    fn is_tuple_typed(&self, expr_id: Id) -> bool {
        if matches!(self.program.entity_map.get(&expr_id), Some(Expr::Tuple(_))) {
            return true;
        }
        if let Some(type_id) = self.expr_type_id(expr_id) {
            return self
                .program
                .type_id_to_type_map
                .get(&self.resolve_type_id(type_id))
                .is_some_and(|type_| matches!(type_, Type::Tuple(_)));
        }
        self.program
            .tuple_element_types
            .get(&expr_id)
            .and_then(|type_id| self.program.type_id_to_type_map.get(type_id))
            .is_some_and(|type_| matches!(type_, Type::Tuple(_)))
    }

    /// Whether a `for x in ...` loop's iterable is the built-in `Set` — a vilan
    /// struct wrapping a `NativeMap` (I1). Its elements are the backing map's
    /// stored originals, so such a loop iterates `set[0].values()`.
    ///
    /// Keyed by the LOOP, not by the iterable expression: the analyzer recorded
    /// the type it inferred there (`for_each_iterable_types`), which is the only
    /// total answer. Re-deriving it here from the iterable's own expr id was
    /// what B85 was — silent for every form that stores no type of its own, so
    /// `for x in self` inside `Set`'s own impl, `for x in make_set()` and `for
    /// x in *view` all walked the struct's one-element field array instead.
    fn for_each_iterates_a_set(&self, for_each_id: Id) -> bool {
        self.program
            .for_each_iterable_types
            .get(&for_each_id)
            .map(|type_id| self.resolve_type_id(*type_id))
            .and_then(|type_id| self.program.type_id_to_type_map.get(&type_id))
            .is_some_and(|type_| match type_ {
                Type::Struct(id, _) => self
                    .program
                    .structs
                    .get(id)
                    .is_some_and(|struct_| struct_.name == "Set"),
                _ => false,
            })
    }

    fn resolve_type_id(&self, type_id: TypeId) -> TypeId {
        let Some(_guard) = crate::util::RecursionGuard::enter() else {
            return type_id;
        };
        match self.program.type_id_to_type_map.get(&type_id) {
            Some(Type::Generic(constraint_id)) => {
                match self.current_substitution.get(constraint_id) {
                    // Guard a self-mapping (`T -> T`): the substitution binds the
                    // generic to itself (which reconciling an impl's own parameter
                    // records), so following it would loop forever — leave it abstract.
                    Some(bound)
                        if !matches!(
                            self.program.type_id_to_type_map.get(bound),
                            Some(Type::Generic(c)) if c == constraint_id
                        ) =>
                    {
                        self.resolve_type_id(*bound)
                    }
                    _ => type_id,
                }
            }
            _ => type_id,
        }
    }

    /// A type whose `==`/`!=` compares by value in native JS — the scalar
    /// primitives (`i32`/…/`str`), `bool`, and BACKED enums, all lowered to JS
    /// numbers/strings/booleans. A generic `==` monomorphized to one of these
    /// stays native rather than dispatching to a `PartialEq` impl (which for a
    /// primitive is native `===` anyway), keeping codegen identical to a direct `==`.
    ///
    /// A string-backed enum qualifies for exactly the reason `str` itself does,
    /// listed two lines up: both sides are JS string primitives (§3.5).
    fn compares_natively(&self, type_id: TypeId) -> bool {
        match self.program.type_id_to_type_map.get(&type_id) {
            Some(Type::Struct(id, _)) => self.program.structs.get(id).is_some_and(|struct_| {
                matches!(struct_.name, "i32" | "u32" | "f64" | "BigInt" | "str")
            }),
            Some(Type::Enum(id, _)) => {
                Some(*id) == self.program.bool_enum_id
                    || self
                        .program
                        .enums
                        .get(id)
                        .is_some_and(|enum_| enum_.backing.is_some())
            }
            _ => false,
        }
    }

    /// Whether the operator method at this site must be EMITTED AS AN
    /// INSTANCE against `substitution`, or may share the generic emission
    /// (B135). A non-native binding always specializes — the body's element
    /// comparisons must dispatch to the bound type's own impls. An all-native
    /// binding shares the generic emission (its operators on `T` lower to
    /// native JS — the std `Option`/`Result`/`List` idiom, and the shape the
    /// corpus goldens pin) UNLESS the body transitively contains a call that
    /// needs the substitution to resolve at all — an explicit `.eq()` on a
    /// `T`-typed value, which absent a substitution falls through to the
    /// trait's bodyless requirement and trips the never-silent check.
    fn operator_instance_required(
        &mut self,
        method_id: Id,
        substitution: &HashMap<TypeId, TypeId>,
    ) -> bool {
        for &bound in substitution.values() {
            let resolved = self.resolve_type_id(bound);
            if !self.compares_natively(resolved) {
                return true;
            }
        }
        self.reaches_bare_requirement(method_id)
    }

    /// Whether `function_id`'s body — transitively, through the calls and
    /// lexical closures the program's call graph records — contains a
    /// dispatch that, emitted WITHOUT a substitution, falls through to a
    /// BODYLESS trait requirement (`assemble`'s never-silent ICE, B135).
    /// The walk follows resolved calls into their bodies, descends into
    /// every lexical closure (a hidden `.eq()` in a closure invoked through
    /// a variable still counts — its call edge is `Indirect`), treats a
    /// dispatch falling back to a trait DEFAULT body as a walk into that
    /// body, and stops at externs and variant constructors. Operator uses of
    /// `T` contribute nothing here — a binary expression records no call
    /// edge, which is exactly what lets an operator-only body keep the
    /// generic emission. Memoized per queried root; the per-query visited
    /// set keeps cycles finite without poisoning other roots' answers.
    fn reaches_bare_requirement(&mut self, function_id: Id) -> bool {
        if let Some(&known) = self.bare_requirement_memo.get(&function_id) {
            return known;
        }
        let program = self.program;
        let graph = program.call_graph();
        let mut visited: HashSet<Id> = HashSet::default();
        let mut stack = vec![function_id];
        let mut reaches = false;
        'walk: while let Some(node) = stack.pop() {
            if !visited.insert(node) {
                continue;
            }
            if let Some(children) = graph.closure_children_of(node) {
                stack.extend(children.iter().copied());
            }
            for call in graph.calls_of(node) {
                match call.target {
                    CallTarget::Function(callee) | CallTarget::Closure(callee) => {
                        stack.push(callee);
                    }
                    CallTarget::External(_) | CallTarget::Variant(_) => {}
                    // A call through a function/closure VALUE resolves to
                    // whatever the value holds — never to a requirement.
                    CallTarget::Indirect(IndirectReason::Value) => {}
                    CallTarget::Indirect(
                        IndirectReason::GenericMember | IndirectReason::TraitDispatch,
                    ) => {
                        let Some(fallback) = self.dispatch_fallback(call.call_id) else {
                            continue;
                        };
                        if program
                            .functions
                            .get(&fallback)
                            .is_some_and(|function| !function.has_body)
                        {
                            reaches = true;
                            break 'walk;
                        }
                        stack.push(fallback);
                    }
                }
            }
        }
        self.bare_requirement_memo.insert(function_id, reaches);
        reaches
    }

    /// The function a dispatch-carrying call site emits when NO substitution
    /// resolves it: a `for` loop's recorded `next`, or the `Expr::Local`
    /// target its subject names — for `x.eq(y)` on a `T`-bounded receiver,
    /// the trait's requirement itself.
    fn dispatch_fallback(&self, call_id: Id) -> Option<Id> {
        if let Some(&next_id) = self.program.for_each_next.get(&call_id) {
            return Some(next_id);
        }
        let function_call = self.program.function_calls.get(&call_id)?;
        match self.program.entity_map.get(&function_call.subject_id) {
            Some(Expr::Local(target_id)) => Some(*target_id),
            _ => None,
        }
    }

    /// A stable key identifying a concrete type, used to deduplicate instances.
    ///
    /// STRUCTURAL, not id-keyed (B95). The obvious spelling — `format!("{:?}",
    /// type_)` — looks structural and is not: every nominal `Type` carries its
    /// arguments as raw [`TypeId`]s, so `Struct(list, [TypeId(42)])` and
    /// `Struct(list, [TypeId(99)])` key differently even when 42 and 99 both
    /// denote `i32`. Type ids are minted in inference order, so the same program
    /// can mint two ids for one type merely because an argument was re-inferred
    /// earlier — and the instance memo would then emit the SAME body twice under
    /// two names (a duplicate `Signal::new` was observed that way). Spelling the
    /// whole shape out makes the key depend on what the type IS.
    ///
    /// The recursion is strictly coarsening: equal `Debug` output implies equal
    /// `Type` values implies an equal structural key, so this can only ever MERGE
    /// what the old key separated — never split.
    ///
    /// Two positions stay id-keyed, deliberately: a `Generic` binder (distinct
    /// binders are distinct abstract types — following the id would be a lookup
    /// into itself) and a type id absent from the map (nothing to spell).
    fn type_key(&self, type_id: TypeId) -> String {
        let mut key = String::new();
        self.write_type_key(type_id, &mut key);
        key
    }

    /// Appends [`Self::type_key`]'s spelling of `type_id` to `out`. Every arm
    /// opens with a distinct sigil and closes its argument list, so the encoding
    /// stays injective over shapes.
    fn write_type_key(&self, type_id: TypeId, out: &mut String) {
        use std::fmt::Write;
        // Guards a type-argument cycle; the depth is shared with the rest of the
        // compiler's recursive walks, and reaching it means the key is truncated
        // (still sound — a truncated key only merges further).
        let Some(_guard) = crate::util::RecursionGuard::enter() else {
            out.push_str("...");
            return;
        };
        let Some(type_) = self.program.type_id_to_type_map.get(&type_id) else {
            let _ = write!(out, "?{}", type_id.0);
            return;
        };
        match type_ {
            Type::Struct(id, arguments) => {
                let _ = write!(out, "S{}", id.0);
                self.write_type_key_arguments(arguments, out);
            }
            Type::Enum(id, arguments) => {
                let _ = write!(out, "E{}", id.0);
                self.write_type_key_arguments(arguments, out);
            }
            Type::Trait(id, arguments) => {
                let _ = write!(out, "T{}", id.0);
                self.write_type_key_arguments(arguments, out);
            }
            Type::Tuple(elements) => {
                out.push_str("Tup");
                self.write_type_key_arguments(elements, out);
            }
            Type::Closure(parameters, return_type_id) => {
                out.push_str("Fn");
                self.write_type_key_arguments(parameters, out);
                out.push_str("->");
                self.write_type_key(*return_type_id, out);
            }
            Type::Array(element_type_id, length) => {
                out.push_str("Arr[");
                self.write_type_key(*element_type_id, out);
                let _ = write!(out, ";{length}]");
            }
            Type::Mapped(binder_id, source_type_id, template_type_id) => {
                let _ = write!(out, "Map(G{},", binder_id.0);
                self.write_type_key(*source_type_id, out);
                out.push(',');
                self.write_type_key(*template_type_id, out);
                out.push(')');
            }
            Type::Generic(constraint_id) => {
                let _ = write!(out, "G{}", constraint_id.0);
            }
            // No nested type ids to spell — `Any`, `Never`, `Function(Id)`,
            // `Module(Id)`, `Unknown`, `Unresolved`, `Void`. The `#` keeps their
            // `Debug` spelling from colliding with an arm above.
            other => {
                let _ = write!(out, "#{other:?}");
            }
        }
    }

    fn write_type_key_arguments(&self, arguments: &[TypeId], out: &mut String) {
        out.push('(');
        for (index, argument) in arguments.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            self.write_type_key(*argument, out);
        }
        out.push(')');
    }

    /// Finds the function implementing `member` for a concrete type, searching
    /// the implementations whose subject matches that type.
    /// Resolves `member` on a concrete type to its impl method, returning the
    /// member id *and the impl's subject* (in the impl's own generic terms, e.g.
    /// `List<Generic(T)>`) so the caller can bind the impl's generics from the
    /// concrete type's arguments.
    fn resolve_member_on_type(&self, type_id: TypeId, member: &str) -> Option<(Id, TypeId)> {
        let type_ = self.program.type_id_to_type_map.get(&type_id)?;
        match type_ {
            Type::Struct(_, _) | Type::Enum(_, _) => self
                .program
                .implementations
                .iter()
                .filter(|implementation| {
                    self.program
                        .type_id_to_type_map
                        .get(&implementation.subject)
                        .is_some_and(|subject| nominal_matches(subject, type_))
                })
                .find_map(|implementation| {
                    implementation
                        .declarations
                        .get(member)
                        .map(|member_id| (*member_id, implementation.subject))
                }),
            _ => None,
        }
    }

    /// Binds the generic parameters in `pattern` (an impl subject in its own
    /// generic terms, `List<Generic(T)>`) from the matching positions of the
    /// concrete `type_id` (`List<i32>`), accumulating `{T -> i32}`. Recurses
    /// through nominal arguments, tuples, and closures so a nested parameter
    /// (`List<List<T>>` -> `T = i32`) is reached.
    fn bind_generics(&self, pattern: TypeId, type_id: TypeId, out: &mut HashMap<TypeId, TypeId>) {
        let Some(pattern_type) = self.program.type_id_to_type_map.get(&pattern).cloned() else {
            return;
        };
        if let Type::Generic(constraint_id) = pattern_type {
            out.insert(constraint_id, type_id);
            return;
        }
        let Some(concrete_type) = self.program.type_id_to_type_map.get(&type_id).cloned() else {
            return;
        };
        let zip_args = |out: &mut HashMap<TypeId, TypeId>,
                        pattern_args: &[TypeId],
                        concrete_args: &[TypeId],
                        this: &Self| {
            for (pattern_arg, concrete_arg) in pattern_args.iter().zip(concrete_args.iter()) {
                this.bind_generics(*pattern_arg, *concrete_arg, out);
            }
        };
        match (pattern_type, concrete_type) {
            (Type::Struct(a, pattern_args), Type::Struct(b, concrete_args)) if a == b => {
                zip_args(out, &pattern_args, &concrete_args, self);
            }
            (Type::Enum(a, pattern_args), Type::Enum(b, concrete_args)) if a == b => {
                zip_args(out, &pattern_args, &concrete_args, self);
            }
            (Type::Tuple(pattern_args), Type::Tuple(concrete_args)) => {
                zip_args(out, &pattern_args, &concrete_args, self);
            }
            // `[T; n]` against `[i32; n]` binds `T = i32` through the element.
            (Type::Array(pattern_element, _), Type::Array(concrete_element, _)) => {
                self.bind_generics(pattern_element, concrete_element, out);
            }
            (
                Type::Closure(pattern_params, pattern_ret),
                Type::Closure(concrete_params, concrete_ret),
            ) => {
                zip_args(out, &pattern_params, &concrete_params, self);
                self.bind_generics(pattern_ret, concrete_ret, out);
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct Formatter {
    line_break: &'static str,
    indentation: &'static str,
    space: &'static str,
    array_surround: &'static str,
    // object_surround: &'static str,
}

impl Formatter {
    /// Builds the whitespace style from the two formatting options: `indent` gives
    /// line breaks + leading indentation, `spaces` gives inter-token padding. They
    /// are independent — `indent && !spaces` is multi-line but tight, for example.
    fn from_options(indent: bool, spaces: bool) -> Self {
        Self {
            line_break: if indent { "\n" } else { "" },
            indentation: if indent { "\t" } else { "" },
            space: if spaces { " " } else { "" },
            array_surround: if spaces { " " } else { "" },
        }
    }

    fn file(&self, list: &Vec<js::Node>) -> String {
        self.sequence(list, ";", 0)
    }

    /// The separator two adjacent output fragments need. Normally that is just
    /// `space`, but dropping the padding is only sound while it leaves the TOKEN
    /// STREAM alone, and at an operator junction it does not always: `3 - -(2)`
    /// printed tight is `3--(2)`, which JavaScript lexes as one postfix `--` and
    /// rejects. A pair of characters that would fuse into a longer token keeps a
    /// single space whatever the padding option says — the minimum separation,
    /// not the configured one.
    fn between(&self, left: &str, right: &str) -> &'static str {
        if !self.space.is_empty() {
            return self.space;
        }
        match (left.chars().next_back(), right.chars().next()) {
            // `+ +…` and `- -…` lex as the increment and decrement operators.
            (Some('+'), Some('+')) | (Some('-'), Some('-')) => " ",
            // `/ /…` opens a line comment and `/ *…` a block comment.
            (Some('/'), Some('/' | '*')) => " ",
            _ => "",
        }
    }

    /// Renders a sequence of statements, one per line, each indented to `level`.
    /// The per-statement indent lives here (not in `node`) so `node` can render a
    /// sub-expression inline — without a leading indent — while still passing the
    /// current `level` down, so a block nested inside an expression (a closure
    /// argument, a function-valued binding) indents to its true depth.
    fn sequence(&self, list: &[js::Node], terminator: &'static str, level: usize) -> String {
        let indent = self.indentation.repeat(level);
        list.iter()
            .map(|node| format!("{}{}", indent, self.node(node, terminator, level)))
            .collect::<Vec<_>>()
            .join(self.line_break)
    }

    /// A binary operator's JavaScript binding precedence (higher binds tighter),
    /// used to parenthesize operands. Note this is JS's C-style order — distinct
    /// from vilan's source precedence (bitwise binds tighter than comparison in
    /// vilan, looser in JS), which is exactly why emission must parenthesize by
    /// THIS table, not the parser's.
    fn js_binary_precedence(op: BinaryOp) -> u8 {
        match op {
            BinaryOp::Or => 0,
            BinaryOp::And => 1,
            // JS's C-style order: the bitwise ops bind LOOSER than comparison —
            // the opposite of vilan's source precedence, so a vilan
            // `(a & b) == c` tree emits with parentheses.
            BinaryOp::BitOr => 2,
            BinaryOp::BitXor => 3,
            BinaryOp::BitAnd => 4,
            BinaryOp::Eq | BinaryOp::NotEq => 5,
            BinaryOp::Lt | BinaryOp::Gt | BinaryOp::LtEq | BinaryOp::GtEq => 6,
            BinaryOp::Shl | BinaryOp::Shr | BinaryOp::UShr => 7,
            BinaryOp::Add | BinaryOp::Sub => 8,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem => 9,
        }
    }

    /// Renders a binary node's operand, parenthesizing when its own binding is
    /// too loose to survive unwrapped: a nested binary whose precedence fails
    /// `keeps(child)`, or an assignment (which JS parses greedily as an
    /// expression). Atoms, calls, and property accesses bind tighter than any
    /// binary operator and pass through bare.
    fn operand(&self, node: &js::Node, level: usize, keeps: impl Fn(u8) -> bool) -> String {
        let rendered = self.node(node, "", level);
        let wrap = match node {
            js::Node::Binary(op, _, _) => !keeps(Self::js_binary_precedence(*op)),
            js::Node::Assignment(_, _) => true,
            _ => false,
        };
        if wrap {
            format!("({rendered})")
        } else {
            rendered
        }
    }

    /// Renders the SUBJECT of a postfix — a call's callee, a `.member`, an
    /// `[index]` — parenthesizing when the subject binds looser than the postfix
    /// does. Member access and call are JS's tightest-binding forms, so a
    /// subject that is not an atom, a literal or another postfix gets its
    /// operand stolen unless it is wrapped: `await (f()).x` parses as
    /// `await ((f()).x)`, reading the member off the PROMISE rather than the
    /// value, which is a silent wrong answer (B141).
    ///
    /// This is `operand`'s counterpart on the other side of the precedence
    /// question. `operand` asks whether a child survives unwrapped in
    /// BINARY-OPERAND position, where the danger is a child that binds too
    /// loosely to hold together; this asks whether it survives in
    /// POSTFIX-SUBJECT position, where the danger is the same but the threshold
    /// is the highest one JS has, so the test is a flat "is this a postfix or an
    /// atom" rather than a numeric comparison.
    fn postfix_subject(&self, node: &js::Node, level: usize) -> String {
        let rendered = self.node(node, "", level);
        let wrap = matches!(
            node,
            // `await` is a unary prefix: every postfix binds tighter than it.
            js::Node::Await(_)
                | js::Node::Unary(_, _)
                | js::Node::Binary(_, _, _)
                // JS parses an assignment greedily, as `operand` also notes.
                | js::Node::Assignment(_, _)
                // A closure called directly must be parenthesised: `(() => …)()`.
                | js::Node::Closure(_)
        );
        if wrap {
            format!("({rendered})")
        } else {
            rendered
        }
    }

    /// Renders one JavaScript node at block-nesting `level` (used to indent the
    /// bodies of any nested blocks). It emits no leading indent of its own — a
    /// statement's indent is added by `sequence`, an expression is rendered inline
    /// — and passes `level` down to its sub-expressions, so a block nested inside
    /// an expression indents to its true depth.
    fn node(&self, node: &js::Node, terminator: &'static str, level: usize) -> String {
        match node {
            js::Node::Void => format!("undefined{}", terminator),
            js::Node::Null => format!("null{}", terminator),
            js::Node::String(x) => format!("\"{}\"{}", x.escape_default(), terminator),
            js::Node::Number(whole, fraction) => format!(
                "{}{}{}",
                whole,
                fraction
                    .clone()
                    .map(|x| format!(".{x}"))
                    .unwrap_or("".to_string()),
                terminator
            ),
            js::Node::Bool(x) => format!("{}{}", x, terminator),
            js::Node::Array(items) => {
                let s_items = items
                    .iter()
                    .map(|x| self.node(x, "", level))
                    .collect::<Vec<_>>()
                    .join(format!(",{}", self.space).as_str());
                format!(
                    "[{}{}{}]{}",
                    self.array_surround, s_items, self.array_surround, terminator
                )
            }
            js::Node::Spread(operand) => {
                format!("...{}{}", self.node(operand, "", level), terminator)
            }
            js::Node::Function(function) => {
                let name = function.name.as_str();
                let parameters = function
                    .parameters
                    .iter()
                    .map(|x| x.name.as_str())
                    .collect::<Vec<_>>()
                    .join(format!(",{}", self.space).as_str());
                let body = self.sequence(&function.body, ";", level + 1);
                format!(
                    "{}function {}({}){}{{{}{}{}{}}}{}",
                    if function.is_async { "async " } else { "" },
                    name,
                    parameters,
                    self.space,
                    self.line_break,
                    body,
                    self.line_break,
                    self.indentation.repeat(level),
                    match terminator {
                        ";" => "",
                        x => x,
                    }
                )
            }
            js::Node::Local(name) => format!("{}{}", name, terminator),
            js::Node::Assignment(subject, value) => format!(
                "{}{}={}{}{}",
                self.node(subject, "", level),
                self.space,
                self.space,
                self.node(value, "", level),
                terminator
            ),
            js::Node::Return(value) => match &**value {
                js::Node::Void => format!("return{}", terminator),
                x => format!("return {}{}", self.node(x, "", level), terminator),
            },
            js::Node::Throw(value) => {
                format!("throw {}{}", self.node(value, "", level), terminator)
            }
            js::Node::Call(subject, args) => {
                let s_subject = self.postfix_subject(subject, level);
                let s_args = args
                    .iter()
                    .map(|x| self.node(x, "", level))
                    .collect::<Vec<_>>()
                    .join(format!(",{}", self.space).as_str());
                format!("{}({}){}", s_subject, s_args, terminator)
            }
            js::Node::Binary(op, lhs, rhs) => {
                let s_op = match op {
                    BinaryOp::Add => "+",
                    BinaryOp::Sub => "-",
                    BinaryOp::Mul => "*",
                    BinaryOp::Div => "/",
                    BinaryOp::Rem => "%",
                    BinaryOp::Shl => "<<",
                    BinaryOp::Shr => ">>",
                    BinaryOp::UShr => ">>>",
                    BinaryOp::BitAnd => "&",
                    BinaryOp::BitXor => "^",
                    BinaryOp::BitOr => "|",
                    BinaryOp::Eq => "===",
                    BinaryOp::NotEq => "!==",
                    BinaryOp::Lt => "<",
                    BinaryOp::Gt => ">",
                    BinaryOp::LtEq => "<=",
                    BinaryOp::GtEq => ">=",
                    BinaryOp::And => "&&",
                    BinaryOp::Or => "||",
                };
                // Operands are parenthesized by JS precedence, or grouping is
                // lost — `(1 + 2) * 3` must not print as `1 + 2 * 3`. The left
                // operand needs parens when it binds looser than this node; the
                // right also at EQUAL precedence (`-`/`/` are non-associative,
                // and `+` mixes strings and numbers, so `1 + (2 + "x")` differs
                // from `1 + 2 + "x"`).
                let parent = Self::js_binary_precedence(*op);
                let s_lhs = self.operand(lhs, level, |child| child >= parent);
                let s_rhs = self.operand(rhs, level, |child| child > parent);
                // The one junction in the printer where two arbitrary fragments
                // meet across punctuation, so the one that needs `between`.
                format!(
                    "{}{}{}{}{}{}",
                    s_lhs,
                    self.between(&s_lhs, s_op),
                    s_op,
                    self.between(s_op, &s_rhs),
                    s_rhs,
                    terminator
                )
            }
            js::Node::Unary(operator, operand) => {
                // Parenthesise the operand so precedence is preserved — e.g.
                // `!(a < b)` must not render as `!a < b`.
                format!(
                    "{}({}){}",
                    operator,
                    self.node(operand, "", level),
                    terminator
                )
            }
            js::Node::LetVariable(variable) => {
                let value = self.node(&variable.value, "", level);
                format!(
                    "let {}{}={}{}{}",
                    variable.name, self.space, self.space, value, terminator
                )
            }
            js::Node::ConstVariable(variable) => {
                let value = self.node(&variable.value, "", level);
                format!(
                    "const {}{}={}{}{}",
                    variable.name, self.space, self.space, value, terminator
                )
            }
            js::Node::Property(subject, member) => {
                let s_subject = self.postfix_subject(subject, level);
                format!("{}.{}{}", s_subject, member, terminator)
            }
            js::Node::PropertyIndex(subject, member) => {
                let s_subject = self.postfix_subject(subject, level);
                let s_member = self.node(member, "", level);
                format!("{}[{}]{}", s_subject, s_member, terminator)
            }
            js::Node::If(branch) => {
                fn walk_branch(
                    f: &Formatter,
                    branch: &js::IfBranch,
                    level: usize,
                    else_depth: u32,
                ) -> String {
                    match branch {
                        js::IfBranch::If(condition, body, else_) => {
                            let s_prefix = if else_depth > 0 { "else " } else { "" };
                            let s_condition = f.node(condition, "", level);
                            let s_body = f.sequence(body, ";", level + 1);
                            let s_else = else_
                                .as_ref()
                                .map(|x| {
                                    format!(
                                        "{}{}",
                                        f.space,
                                        walk_branch(f, x, level, else_depth + 1)
                                    )
                                })
                                .unwrap_or("".to_string());
                            format!(
                                "{}if{}({}){}{{{}{}{}{}}}{}",
                                s_prefix,
                                f.space,
                                s_condition,
                                f.space,
                                f.line_break,
                                s_body,
                                f.line_break,
                                f.indentation.repeat(level),
                                s_else
                            )
                        }
                        js::IfBranch::Else(body) => {
                            let s_body = f.sequence(body, ";", level + 1);
                            format!(
                                "else{}{{{}{}{}{}}}",
                                f.space,
                                f.line_break,
                                s_body,
                                f.line_break,
                                f.indentation.repeat(level)
                            )
                        }
                    }
                }
                walk_branch(self, branch, level, 0)
            }
            js::Node::While(condition, body) => {
                let s_condition = self.node(condition, "", level);
                let s_body = self.sequence(body, ";", level + 1);
                format!(
                    "while{}({}){}{{{}{}{}{}}}",
                    self.space,
                    s_condition,
                    self.space,
                    self.line_break,
                    s_body,
                    self.line_break,
                    self.indentation.repeat(level),
                )
            }
            js::Node::ForOf(binding, iterable, body) => {
                let s_iterable = self.node(iterable, "", level);
                let s_body = self.sequence(body, ";", level + 1);
                format!(
                    "for{}(const {} of {}){}{{{}{}{}{}}}",
                    self.space,
                    binding,
                    s_iterable,
                    self.space,
                    self.line_break,
                    s_body,
                    self.line_break,
                    self.indentation.repeat(level),
                )
            }
            js::Node::Try(body, finally) => {
                let s_body = self.sequence(body, ";", level + 1);
                let s_finally = self.sequence(finally, ";", level + 1);
                format!(
                    "try{}{{{}{}{}{}}}{}finally{}{{{}{}{}{}}}",
                    self.space,
                    self.line_break,
                    s_body,
                    self.line_break,
                    self.indentation.repeat(level),
                    self.space,
                    self.space,
                    self.line_break,
                    s_finally,
                    self.line_break,
                    self.indentation.repeat(level),
                )
            }
            js::Node::Break => format!("break{}", terminator),
            js::Node::Continue => format!("continue{}", terminator),
            js::Node::Closure(closure) => {
                let s_parameters = closure
                    .parameters
                    .iter()
                    .map(|x| x.name.as_str())
                    .collect::<Vec<_>>()
                    .join(format!(",{}", self.space).as_str());
                let s_body = self.sequence(&closure.body, ";", level + 1);
                format!(
                    "{}({}){}=>{}{{{}{}{}{}}}{}",
                    if closure.is_async { "async " } else { "" },
                    s_parameters,
                    self.space,
                    self.space,
                    self.line_break,
                    s_body,
                    self.line_break,
                    self.indentation.repeat(level),
                    terminator
                )
            }
            js::Node::Await(operand) => {
                // Parenthesise so `await` doesn't bind too loosely (e.g.
                // `await (a + b)`), mirroring the unary `!` rendering.
                format!("await ({}){}", self.node(operand, "", level), terminator)
            }
        }
    }
}

/// Serializes a const result in place of its expression (const-eval.md §1),
/// producing the same runtime shapes emitted code builds itself: structs and
/// enums are already positional arrays at this level, so `ConstValue::Array`
/// covers them.
fn const_value_to_js<'src>(value: &ConstValue) -> js::Node<'src> {
    match value {
        ConstValue::Undefined => js::Node::Void,
        ConstValue::Null => js::Node::Null,
        ConstValue::Bool(value) => js::Node::Bool(*value),
        ConstValue::Number(n) => {
            if n.is_nan() {
                js::Node::Local("NaN".to_string())
            } else if n.is_infinite() {
                js::Node::Local(if *n > 0.0 { "Infinity" } else { "-Infinity" }.to_string())
            } else if *n == 0.0 && n.is_sign_negative() {
                // `js_number_to_string` collapses -0 to "0" (string coercion
                // semantics); the LITERAL must keep the sign.
                js::Node::Number("-0".to_string(), None)
            } else {
                js::Node::Number(crate::interpreter::js_number_to_string(*n), None)
            }
        }
        ConstValue::BigInt(n) => js::Node::Number(format!("{n}n"), None),
        ConstValue::Str(s) => js::Node::String(Cow::Owned(s.clone())),
        ConstValue::Array(items) => js::Node::Array(items.iter().map(const_value_to_js).collect()),
        ConstValue::Set(items) => js::Node::Call(
            Box::new(js::Node::Local("new Set".to_string())),
            vec![js::Node::Array(
                items.iter().map(const_value_to_js).collect(),
            )],
        ),
        ConstValue::Map(entries) => js::Node::Call(
            Box::new(js::Node::Local("new Map".to_string())),
            vec![js::Node::Array(
                entries
                    .iter()
                    .map(|(key, value)| {
                        js::Node::Array(vec![const_value_to_js(key), const_value_to_js(value)])
                    })
                    .collect(),
            )],
        ),
    }
}

/// Everything a const site's assembly needs that does NOT vary with the
/// expression being assembled: the [`NameSeed`] the world's transformer starts
/// from, and the module-level bindings each site's prelude is filtered against.
/// Both are functions of `(program, options)` alone — and both were rebuilt per
/// const site, which on the website's server entry meant 210 rebuilds of a
/// 4,184-entry source-name map and 210 walks of the module tree
/// (`const-eval.md` §10). The [`ConstWorld`] builds one of these per analysis.
struct ConstProgramSeed {
    names: Rc<NameSeed>,
    /// Module-level bindings as a set: the ids a site's prelude may have to
    /// declare. `module_level_bindings()` returns them in emission order, which
    /// this build does not need — the prelude sorts its own batches by entity
    /// id (`b33-emission-order.md` §4).
    module_level_bindings: HashSet<Id>,
}

impl ConstProgramSeed {
    fn build(program: &Program, options: &BuildOptions) -> Self {
        Self {
            names: Rc::new(NameSeed::build(program, options)),
            module_level_bindings: program.module_level_bindings().into_iter().collect(),
        }
    }
}

/// The shared const world (`const-eval.md` §10.6): ONE lowering per const pass
/// that every site is evaluated against, in place of the whole-closure
/// mini-program each site used to build for itself.
///
/// A site's expression is walked the first time the pass reaches it, and the
/// functions that walk requires are lowered into this world's single
/// [`Transformer`] — so a function every style chain enters through is emitted
/// once for the pass instead of once per site. (The website's client entry:
/// 3,873 function emissions across 188 sites, **106** of them distinct.)
///
/// **What is shared is the LOWERING, never the evaluation.** Each site still
/// gets its own prelude — the module-level bindings it reads, declared afresh —
/// and its own interpreter scope, so no site can observe another site's state.
/// And each site still carries its OWN reached set (functions, module-level
/// bindings, runtime helpers, host imports), reconstructed from the
/// [`EmissionRecord`]s the lowering left behind rather than from a re-walk, so
/// its prelude, its `unresolved` diagnostics and what `check_capabilities`
/// refuses on are exactly what a per-site mini-program produced.
pub struct ConstWorld<'src> {
    transformer: Transformer<'src>,
    seed: ConstProgramSeed,
    sites: HashMap<Id, SiteWalk<'src>>,
}

/// One site's own lowering, cached: the statements its expression emitted (the
/// last of them `const __const_result = …`), and what that walk required — the
/// seed of the site's reached set. Cached because the dependency-order retry
/// loop re-derives only the PRELUDE; the expression and the world it reaches do
/// not change when a dependency folds.
struct SiteWalk<'src> {
    body: Vec<js::Node<'src>>,
    record: EmissionRecord,
}

/// What one site reaches in the world — the per-site half of what used to be a
/// per-site mini-program.
pub struct SiteReach {
    /// The concrete functions it reaches, in entity-id order (the order a
    /// mini-program declared them in).
    functions: Vec<Id>,
    /// The `monomorphized` slots it reaches, in emission order.
    slots: Vec<usize>,
    /// Every module-level binding its code reads — the prelude's input, and
    /// what anything not compile-time-known is reported from.
    globals: HashSet<Id>,
    helpers: Vec<&'static str>,
    imports: Vec<String>,
}

/// One const site's program, evaluated against the pass's shared world.
pub struct ConstSite<'a> {
    /// The world declarations this site reaches, borrowed from the one lowering
    /// the pass made — hoisted into the site's own fresh scope.
    pub world: Vec<&'a js::Node<'a>>,
    /// The host imports and runtime helpers THIS site reaches, never the
    /// world's union: what the interpreter refuses on is a per-site fact.
    pub imports: Vec<String>,
    pub helpers: Vec<&'static str>,
    /// The module-level bindings this site reads, declared for this site alone.
    pub prelude: Vec<js::Node<'a>>,
    /// The site's expression, lowered once, ending in `const __const_result`.
    pub body: &'a [js::Node<'a>],
}

impl<'src> ConstWorld<'src> {
    pub fn new(program: &'src Program<'src>, options: &BuildOptions) -> Self {
        let seed = ConstProgramSeed::build(program, options);
        let mut transformer = Transformer::with_name_seed(program, options, seed.names.clone());
        transformer.recorder = Some(EmissionRecorder::default());
        Self {
            transformer,
            seed,
            sites: HashMap::default(),
        }
    }

    /// Lowers `expr_id`'s expression into the world if it is not there yet, then
    /// builds this site's prelude: declarations for the module-level bindings it
    /// reads — already-computed `const` values as literals, literal initializers
    /// walked. Returns the site's reach, its prelude, and the bindings that are
    /// NOT compile-time-known, which the caller turns into diagnostics or
    /// resolves and asks again.
    pub fn prepare(
        &mut self,
        expr_id: Id,
        external_bindings: &HashSet<Id>,
        const_values: &HashMap<Id, crate::interpreter::ConstValue>,
    ) -> (SiteReach, Vec<js::Node<'src>>, Vec<Id>) {
        self.walk_site(expr_id);
        let mut reach = self.reach_of(expr_id);

        let ConstWorld {
            transformer, seed, ..
        } = self;
        let program = transformer.program;
        // The bindings that may need a prelude declaration: the expression's own
        // free locals (checked by the caller) and module-level bindings reached
        // through called functions. Everything else referenced is declared inside
        // the emitted code itself (function-body and block locals). Asked as a
        // predicate over the two sets rather than built as their union, so the
        // module-level half stays the seed's one copy.
        let is_external =
            |id: &Id| seed.module_level_bindings.contains(id) || external_bindings.contains(id);

        // The fixpoint runs against the SITE's reached bindings, in a frame of
        // its own — emitting a binding's initializer can reference more bindings
        // (and, in principle, require more code), and both belong to this site.
        let frame = transformer.record_enter();
        transformer.referenced_globals = reach.globals.clone();
        let mut declared: HashSet<Id> = HashSet::default();
        let mut unresolved: Vec<Id> = Vec::new();
        let mut prelude: Vec<js::Node<'src>> = Vec::new();
        loop {
            // `referenced_globals` is a `HashSet`, so sort each round's batch by
            // entity id — the canonical key — or the prelude's declaration order
            // (and any diagnostic order derived from it) would vary run to run
            // (`b33-emission-order.md` §4).
            let mut pending: Vec<Id> = transformer
                .referenced_globals
                .iter()
                .copied()
                .filter(|id| is_external(id) && !declared.contains(id))
                .collect();
            if pending.is_empty() {
                break;
            }
            pending.sort_by_key(|id| id.0);
            for binding in pending {
                declared.insert(binding);
                // Non-variable references (functions, struct names) emit through
                // their own channels; only value bindings need declarations.
                let Some(variable) = program.variables.get(&binding) else {
                    continue;
                };
                let name = transformer.ng.name_for(binding);
                // A const-initialized binding's computed value, keyed by its
                // INITIAL expression id (how `const_eval` stores results).
                if let Some(value) = variable
                    .initial
                    .and_then(|initial| const_values.get(&initial))
                {
                    prelude.push(js::Node::ConstVariable(js::Variable {
                        name,
                        value: Box::new(const_value_to_js(value)),
                    }));
                    continue;
                }
                let initial = variable.initial;
                let literal_initial = initial
                    .and_then(|initial| program.entity_map.get(&initial))
                    .map(|entity| {
                        matches!(
                            entity,
                            Expr::String(_)
                                | Expr::MultilineString(_)
                                | Expr::Number(..)
                                | Expr::Bool(_)
                                | Expr::Null
                        )
                    })
                    .unwrap_or(false);
                if literal_initial && !variable.mutable {
                    let value = transformer
                        .walk_entity(initial.unwrap(), &mut prelude)
                        .unwrap_or(js::Node::Void);
                    prelude.push(js::Node::ConstVariable(js::Variable {
                        name,
                        value: Box::new(value),
                    }));
                } else {
                    unresolved.push(binding);
                }
            }
        }
        let closed = transformer.record_leave(frame, None).unwrap_or_default();
        // Whatever the prelude's own walks added belongs to this site too.
        if closed.requires.is_empty() {
            reach.globals = closed.globals;
        } else {
            reach = self.reach_from(&closed);
        }
        (reach, prelude, unresolved)
    }

    /// The site's program: the world declarations it reaches (borrowed), its own
    /// imports and helpers, its prelude, and its cached body.
    pub fn site<'world>(
        &'world self,
        expr_id: Id,
        reach: &SiteReach,
        prelude: Vec<js::Node<'src>>,
    ) -> ConstSite<'world> {
        let mut world: Vec<&'world js::Node<'world>> =
            Vec::with_capacity(reach.functions.len() + reach.slots.len());
        for function_id in &reach.functions {
            if let Some(node) = self.transformer.required_functions.get(function_id) {
                world.push(node);
            }
        }
        for slot in &reach.slots {
            if let Some(node) = self.transformer.monomorphized.get(*slot) {
                world.push(node);
            }
        }
        ConstSite {
            world,
            imports: reach.imports.clone(),
            helpers: reach.helpers.clone(),
            prelude,
            body: self
                .sites
                .get(&expr_id)
                .map(|site| site.body.as_slice())
                .unwrap_or_default(),
        }
    }

    /// Lowers one site's expression into the world, once per pass. The three
    /// per-walk scratch maps are cleared first, so the shared transformer starts
    /// each site's walk from exactly the state a fresh one gave it.
    fn walk_site(&mut self, expr_id: Id) {
        if self.sites.contains_key(&expr_id) {
            return;
        }
        self.transformer.is_bindings.clear();
        self.transformer.hoisted_values.clear();
        self.transformer.is_binding_reads = None;
        let frame = self.transformer.record_enter();
        let mut body = Vec::new();
        let result = self
            .transformer
            .walk_entity(expr_id, &mut body)
            .unwrap_or(js::Node::Void);
        body.push(js::Node::ConstVariable(js::Variable {
            name: "__const_result".to_string(),
            value: Box::new(result),
        }));
        let record = self
            .transformer
            .record_leave(frame, None)
            .unwrap_or_default();
        self.sites.insert(expr_id, SiteWalk { body, record });
    }

    /// Resolves an interpreter frame trace back to the FUNCTIONS whose emission
    /// minted those names (`const-eval.md` §10.6).
    ///
    /// The trace carries emitted names, and an emitted name is a generated
    /// artifact: one generator serves the whole pass, so two reached functions
    /// that share a source name cannot both be called by it — the second is
    /// `helper2`. Matching a frame by IDENTITY, which the generator's own map
    /// answers, is what keeps §8.2's attribution reading the source rather than
    /// the mint. It also drops the frames §8.2 never wanted at a user: an
    /// instance or a drop helper is named from the anonymous sequence and was
    /// never minted for an entity at all, so it resolves to `None`.
    pub fn resolve_trace(&self, trace: &[String]) -> Vec<Option<Id>> {
        let functions = &self.transformer.program.functions;
        let by_name: HashMap<&str, Id> = self
            .transformer
            .ng
            .names
            .iter()
            .filter(|(id, _)| functions.contains_key(*id))
            .map(|(id, name)| (name.as_str(), *id))
            .collect();
        trace
            .iter()
            .map(|frame| by_name.get(frame.as_str()).copied())
            .collect()
    }

    fn reach_of(&self, expr_id: Id) -> SiteReach {
        match self.sites.get(&expr_id) {
            Some(site) => self.reach_from(&site.record),
            None => self.reach_from(&EmissionRecord::default()),
        }
    }

    /// Closes over `requires` from one walk's record: the set a per-site
    /// re-walk of the whole closure would have produced, read off the records
    /// the single lowering left instead. The walk is a plain worklist, so a
    /// recursive or mutually recursive requirement terminates on `visited`.
    fn reach_from(&self, seed: &EmissionRecord) -> SiteReach {
        let mut functions: Vec<Id> = Vec::new();
        let mut slots: Vec<usize> = Vec::new();
        let mut globals = seed.globals.clone();
        let mut helpers = seed.helpers.clone();
        let mut imports = seed.imports.clone();
        if let Some(recorder) = self.transformer.recorder.as_ref() {
            let mut visited: HashSet<EmissionId> = HashSet::default();
            let mut worklist: Vec<EmissionId> = seed.requires.clone();
            while let Some(key) = worklist.pop() {
                if !visited.insert(key) {
                    continue;
                }
                match key {
                    EmissionId::Function(function_id) => {
                        if self
                            .transformer
                            .required_functions
                            .contains_key(&function_id)
                        {
                            functions.push(function_id);
                        }
                    }
                    EmissionId::Keyed(index) => {
                        if let Some(Some(slot)) = recorder.keyed_slots.get(index) {
                            slots.push(*slot);
                        }
                    }
                }
                let Some(record) = recorder.records.get(&key) else {
                    continue;
                };
                globals.extend(record.globals.iter().copied());
                helpers.extend(record.helpers.iter().copied());
                for (module, symbols) in &record.imports {
                    imports
                        .entry(module.clone())
                        .or_default()
                        .extend(symbols.iter().cloned());
                }
                worklist.extend(record.requires.iter().copied());
            }
        }
        functions.sort_by_key(|function_id| function_id.0);
        slots.sort_unstable();
        SiteReach {
            functions,
            slots,
            globals,
            helpers: helpers.into_iter().collect(),
            imports: imports
                .iter()
                .map(|(module, symbols)| {
                    let names = symbols.iter().cloned().collect::<Vec<_>>().join(", ");
                    format!("import {{ {} }} from \"{}\";", names, module)
                })
                .collect(),
        }
    }
}

pub mod js {
    use crate::node::BinaryOp;
    use std::borrow::Cow;

    #[derive(Clone, Debug)]
    pub enum Node<'src> {
        Array(Vec<Self>),
        // `...<operand>` — array spread, used to splice a tuple-typed element's
        // (already flat) slots into a constructed tuple.
        Spread(Box<Self>),
        Assignment(Box<Self>, Box<Self>),
        // `await <operand>`.
        Await(Box<Self>),
        Binary(BinaryOp, Box<Self>, Box<Self>),
        Unary(char, Box<Self>),
        Bool(bool),
        Break,
        Call(Box<Self>, Vec<Self>),
        Closure(Closure<'src>),
        ConstVariable(Variable<'src>),
        Continue,
        Function(Function<'src>),
        If(IfBranch<'src>),
        While(Box<Self>, Vec<Self>),
        // `for (const <binding> of <iterable>) { <body> }`. The binding name is
        // `_` for a discarded element.
        ForOf(String, Box<Self>, Vec<Self>),
        LetVariable(Variable<'src>),
        Local(String),
        Null,
        Number(String, Option<String>),
        // Object(Vec<(&'src str, Self)>),
        Property(Box<Self>, String),
        PropertyIndex(Box<Self>, Box<Self>),
        Return(Box<Self>),
        String(Cow<'src, str>),
        Throw(Box<Self>),
        // `try { <body> } finally { <finally> }` — scope-end destruction
        // (destruction.md §7): the `finally` drops the scope's still-owned
        // resources, so `ret` / `break` / `continue` / a thrown panic all run it
        // on the way out. A resource declaration stays OUTSIDE its own `try`, so a
        // panic mid-acquisition never drops an unacquired value.
        Try(Vec<Self>, Vec<Self>),
        Void,
    }

    impl Node<'_> {
        /// Whether this node is a JS *statement* that leaves its enclosing block
        /// rather than an expression with a value — `return` / `break` /
        /// `continue`. vilan's `ret` and `jump` are expressions (of the never
        /// type) that may sit in a tail position, so a walk can hand one of these
        /// back where a value was expected; every seam that would wrap or assign
        /// a tail must emit a divergent node AS-IS instead. Wrapping one produced
        /// B152's `return return 1;` — a bundle that does not parse.
        ///
        /// The set is every variant the emitter renders as a bare statement:
        /// `Throw` is in it for the same reason, though no walk hands one back
        /// today (the only `Throw` is built directly into a generated closure
        /// body) — a future one must not be wrapped either.
        pub fn is_divergent(&self) -> bool {
            matches!(
                self,
                Self::Return(_) | Self::Break | Self::Continue | Self::Throw(_)
            )
        }
    }

    #[derive(Clone, Debug)]
    pub enum IfBranch<'src> {
        If(Box<Node<'src>>, Vec<Node<'src>>, Option<Box<Self>>),
        Else(Vec<Node<'src>>),
    }

    #[derive(Clone, Debug)]
    pub struct Function<'src> {
        pub name: String,
        pub parameters: Vec<Parameter>,
        pub body: Vec<Node<'src>>,
        pub is_async: bool,
    }

    #[derive(Clone, Debug)]
    pub struct Parameter {
        pub name: String,
    }

    #[derive(Clone, Debug)]
    pub struct Variable<'src> {
        pub name: String,
        pub value: Box<Node<'src>>,
    }

    #[derive(Clone, Debug)]
    pub struct Closure<'src> {
        pub parameters: Vec<Parameter>,
        pub body: Vec<Node<'src>>,
        pub is_async: bool,
    }
}

/// JavaScript reserved words, the globals the runtime/codegen reference, and the
/// `__`-prefixed runtime helpers — names a readable identifier must avoid. Per-
/// program `[extern]` symbols are added on top (see `collect_reserved_names`).
const RESERVED_NAMES: &[&str] = &[
    // Reserved words (a binding can't use these).
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "new",
    "null",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "let",
    "static",
    "enum",
    "await",
    "async",
    "implements",
    "interface",
    "package",
    "private",
    "protected",
    "public",
    // Globals the runtime helpers / codegen reference as free identifiers.
    "console",
    "process",
    "Math",
    "JSON",
    "Number",
    "BigInt",
    "Boolean",
    "String",
    "Array",
    "Object",
    "Set",
    "Map",
    "Promise",
    "Symbol",
    "Date",
    "Error",
    "RegExp",
    "undefined",
    "NaN",
    "Infinity",
    "globalThis",
    "require",
    "module",
    "exports",
    "structuredClone",
    "setTimeout",
    "setInterval",
    "fetch",
    "document",
    "window",
    "Response",
    "Request",
    // Runtime helpers (emitted as `function __clone(..)`, etc.).
    "__clone",
    "__scan",
    "__parse_i32",
    "__parse_f64",
    "__random_int",
    "__random_float",
    "__args",
    "__env",
    "__shared_new",
    "__list_get",
    "__list_pop",
    "__list_sort_by",
    "__option_take",
    "__option_replace",
    "__map_get",
    "__map_keys",
    "__map_values",
    "__task",
    "__Task",
    "__nursery_new",
    "__nursery_new_detached",
    "__nursery_run",
    "__nursery_of",
    "__nursery_is_cancel",
    "__Nursery",
    "__sleep",
    "__timer",
    "__Timer",
    "__hmr_active",
    // Route chunks (`bundle-splitting.md` §3): the registry handle a split
    // bundle and its chunks meet at, and the helpers behind the gate.
    "__vilan_chunks",
    "__chunk_registry",
    "__chunk_arm",
    "__chunk_ready",
    "__chunk_load",
];

/// The free identifiers a program's `[extern]`s introduce — an imported symbol
/// (`createServer`) or a global root (`console` from `console.log`) — which a
/// readable name must not shadow.
fn collect_reserved_names(program: &Program) -> HashSet<String> {
    let mut reserved: HashSet<String> =
        RESERVED_NAMES.iter().map(|name| name.to_string()).collect();
    for external in program.external_functions.values() {
        if let Some(ExternBinding::Function { symbol, .. }) = &external.extern_binding {
            if let Some(root) = symbol.split('.').next() {
                reserved.insert(root.to_string());
            }
        }
    }
    reserved
}

/// Turns a source name into a valid JS identifier — Vilan identifiers already are
/// (besides reserved words, handled at disambiguation), so this only guards the
/// degenerate cases.
fn sanitize_identifier(name: &str) -> String {
    let mut result: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if result.is_empty() || result.starts_with(|c: char| c.is_ascii_digit()) {
        result.insert(0, '_');
    }
    result
}

/// How generated identifiers are named.
enum NameStyle {
    /// After the source (`greet`), disambiguated on collision — most debuggable.
    Readable,
    /// Obfuscated short name with a source annotation (`a/*greet*/`).
    Annotated,
    /// Obfuscated short name only (`a`).
    Plain,
}

/// What a [`NameGenerator`] starts from, and the only part of it that is a fact
/// about the PROGRAM rather than about one transform: the source names the
/// readable styles name identifiers after, the reserved set no style may hand
/// out, and which style is in force. All three are functions of
/// `(program, options)` alone, so a caller that transforms one program many
/// times — the `const` pass, which used to lower a world per site — builds this
/// once and shares it (`const-eval.md` §10). Behind an `Rc` because sharing it
/// is the point.
struct NameSeed {
    style: NameStyle,
    /// Source names by id (functions, variables, parameters) — empty for `Plain`.
    source_names: HashMap<Id, String>,
    /// Keywords, referenced globals, `__`-helpers and `[extern]` symbols: names
    /// no generated identifier may collide with. Seeded in EVERY style, not just
    /// the readable one — the obfuscated sequence walks `a, b, …, aa, ab, …` and
    /// so eventually spells `if`, `in`, `do`.
    reserved: HashSet<String>,
}

thread_local! {
    /// How many [`NameSeed`]s this thread has built since
    /// [`reset_name_seed_build_count`]. A test instrument for the
    /// one-seed-per-const-pass invariant (`const-eval.md` §10), on the same
    /// argument as `call_graph`'s build counter: a seed rebuilt per const site
    /// and a seed built once produce IDENTICAL output — the sharing is
    /// behaviour-neutral by construction — so only a counter can tell them
    /// apart, and only a counter can catch the rebuild creeping back.
    ///
    /// Thread-local for the same reason as that one: the suite runs analyses
    /// concurrently under plain `cargo test`, and an analysis is
    /// single-threaded. One `Cell` bump against a build that walks every
    /// variable, function and parameter in the program — unmeasurable.
    static NAME_SEED_BUILD_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// The number of [`NameSeed`] builds on this thread since the last
/// [`reset_name_seed_build_count`]. See [`NAME_SEED_BUILD_COUNT`].
pub fn name_seed_build_count() -> usize {
    NAME_SEED_BUILD_COUNT.with(std::cell::Cell::get)
}

/// Zeroes this thread's [`name_seed_build_count`].
pub fn reset_name_seed_build_count() {
    NAME_SEED_BUILD_COUNT.with(|count| count.set(0));
}

impl NameSeed {
    fn build(program: &Program, options: &BuildOptions) -> Self {
        NAME_SEED_BUILD_COUNT.with(|count| count.set(count.get() + 1));
        let style = if options.readable_names {
            NameStyle::Readable
        } else if options.debug_names {
            NameStyle::Annotated
        } else {
            NameStyle::Plain
        };
        // `Plain` names after nothing, so it needs no source names.
        let source_names = if matches!(style, NameStyle::Plain) {
            HashMap::default()
        } else {
            program
                .variables
                .iter()
                .map(|(id, variable)| (*id, variable.name.to_string()))
                .chain(
                    program
                        .functions
                        .iter()
                        .map(|(id, function)| (*id, function.name.to_string())),
                )
                .chain(
                    program
                        .parameters
                        .iter()
                        .map(|(id, parameter)| (*id, parameter.name.to_string())),
                )
                .collect::<HashMap<Id, String>>()
        };
        Self {
            style,
            source_names,
            reserved: collect_reserved_names(program),
        }
    }
}

struct NameGenerator {
    chars: Vec<char>,
    counter: u64,
    names: HashMap<Id, String>,
    /// The program-wide seed — see [`NameSeed`].
    seed: Rc<NameSeed>,
    /// Every name this generator has MINTED — the ones handed to an `Id` by
    /// `name_for` and the ones handed to an anonymous temp by `next_name` alike.
    /// `names` covers only the former, which is what made B69 possible: the
    /// scope re-allocator needs the CLOSED set, because a minted name it does
    /// not know about is one it will happily mint again out of its own
    /// identical alphabet. See `rename_for_scopes`.
    ///
    /// With the seed's reserved set it is also the generator's uniqueness
    /// invariant: [`NameGenerator::is_taken`] is the union of the two, and every
    /// mint consults it, so a generated name is never a reserved word and never
    /// repeats.
    minted: HashSet<String>,
}

impl NameGenerator {
    fn new(seed: Rc<NameSeed>) -> Self {
        Self {
            chars: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
                .chars()
                .collect(),
            counter: 0,
            names: HashMap::default(),
            seed,
            minted: HashSet::default(),
        }
    }

    /// Reserved, or already handed out by this generator — the two halves of
    /// "unavailable".
    fn is_taken(&self, name: &str) -> bool {
        self.seed.reserved.contains(name) || self.minted.contains(name)
    }

    fn name_for(&mut self, id: Id) -> String {
        if let Some(name) = self.names.get(&id) {
            return name.clone();
        }
        let name = match self.seed.style {
            // Name after the source; an entity with no source name (an anonymous
            // temp) gets a `$`-prefixed fresh name, which no source name can be.
            NameStyle::Readable => match self.seed.source_names.get(&id).cloned() {
                Some(source) => self.unique_readable(&source),
                None => self.next_name(),
            },
            NameStyle::Annotated => match self.seed.source_names.get(&id).cloned() {
                Some(source) => format!("{}/*{}*/", self.next_name(), source),
                None => self.next_name(),
            },
            NameStyle::Plain => self.next_name(),
        };
        self.names.insert(id, name.clone());
        name
    }

    /// A readable identifier from `source`, suffixed (`greet2`, `greet3`, ...) until
    /// it collides with neither a reserved name nor a previously assigned one.
    fn unique_readable(&mut self, source: &str) -> String {
        let base = sanitize_identifier(source);
        let mut candidate = base.clone();
        let mut suffix = 2;
        while self.is_taken(&candidate) {
            candidate = format!("{base}{suffix}");
            suffix += 1;
        }
        self.mint(candidate)
    }

    fn next_idx(&mut self) -> u64 {
        let c = self.counter;
        self.counter += 1;
        c
    }

    /// The next unused generated name. The obfuscated sequence walks the same
    /// `[a-zA-Z]` alphabet the scope re-allocator draws from, so it eventually
    /// spells reserved words (`if`, `in`, `do`, …) — [`NameGenerator::is_taken`]
    /// is consulted so it never hands one out.
    fn next_name(&mut self) -> String {
        loop {
            let index = self.next_idx();
            let short = self.name_from_idx(index);
            // In readable mode, temps are `$`-prefixed so they can't collide with a
            // readable (source-derived) name, which never contains `$`.
            let candidate = match self.seed.style {
                NameStyle::Readable => format!("${short}"),
                _ => short,
            };
            if !self.is_taken(&candidate) {
                return self.mint(candidate);
            }
        }
    }

    /// Records a name as handed out: unavailable for a later mint, and a member
    /// of the closed set `rename_for_scopes` re-allocates over.
    fn mint(&mut self, name: String) -> String {
        self.minted.insert(name.clone());
        name
    }

    fn name_from_idx(&self, n: u64) -> String {
        let mut s = String::new();
        let mut num = n;
        let base = self.chars.len() as u64;

        loop {
            let remainder = (num % base) as usize;
            s.push(self.chars[remainder]);
            num /= base;
            if num < 1 {
                break;
            }
            num -= 1;
        }

        s.chars().rev().collect()
    }
}

// --- Scope-aware name allocation --------------------------------------------
//
// The transform assigns each binding a globally-unique name. That's correct but
// not optimal: two locals named `value` in sibling functions become `value` and
// `value2`, and obfuscated names never reuse a letter across functions. This
// post-pass re-allocates names over the *JavaScript* scope tree so disjoint
// scopes share names: in readable mode both `value`s stay `value`; in release a
// short name is reused in every function.
//
// It runs on the assembled node tree, where the real lexical scopes are visible,
// so it's decoupled from any Vilan/JS scope mismatch. Scopes are function-grained
// (a block's `let`s belong to the enclosing function — safe, just less reuse).
// The collect walk may be incomplete (a missed binding just keeps its unique
// name); the rename walk must be exhaustive, so every node variant is handled.

/// The bindings declared directly in one JS function scope, plus its child
/// scopes (nested functions/closures). Names are the binding's current (unique)
/// output name, the key for the rename.
struct JsScope {
    declarations: Vec<String>,
    children: Vec<JsScope>,
}

/// `idx`th obfuscated short name (`a`, `b`, …, `aa`, …) — the release sequence.
fn short_name_from_idx(idx: u64) -> String {
    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let base = CHARS.len() as u64;
    let mut bytes = Vec::new();
    let mut num = idx;
    loop {
        bytes.push(CHARS[(num % base) as usize]);
        num /= base;
        if num < 1 {
            break;
        }
        num -= 1;
    }
    bytes.reverse();
    String::from_utf8(bytes).unwrap()
}

/// The shortest obfuscated name not already in `used` (release allocation).
fn shortest_available(used: &HashSet<String>) -> String {
    let mut idx = 0;
    loop {
        let name = short_name_from_idx(idx);
        if !used.contains(&name) {
            return name;
        }
        idx += 1;
    }
}

/// `base`, or `base2`/`base3`/… if taken (readable allocation).
fn disambiguated(base: &str, used: &HashSet<String>) -> String {
    let base = sanitize_identifier(base);
    if !used.contains(&base) {
        return base;
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}{suffix}");
        if !used.contains(&candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

/// The scope rooted at a function/closure: its parameters, then everything its
/// body declares.
fn function_scope(
    parameters: &[js::Parameter],
    body: &[js::Node],
    renameable: &HashSet<String>,
) -> JsScope {
    let mut declarations: Vec<String> = parameters
        .iter()
        .filter(|parameter| renameable.contains(&parameter.name))
        .map(|parameter| parameter.name.clone())
        .collect();
    let mut children = Vec::new();
    collect_declarations(body, renameable, &mut declarations, &mut children);
    JsScope {
        declarations,
        children,
    }
}

/// Collects, from a run of statements at one function level, the bindings
/// declared directly here (into `declarations`) and the nested function/closure
/// scopes (into `children`). Block bodies (`if`/`while`/`for`) are part of this
/// scope; functions and closures start child scopes.
fn collect_declarations(
    nodes: &[js::Node],
    renameable: &HashSet<String>,
    declarations: &mut Vec<String>,
    children: &mut Vec<JsScope>,
) {
    for node in nodes {
        collect_node(node, renameable, declarations, children);
    }
}

fn collect_node(
    node: &js::Node,
    renameable: &HashSet<String>,
    declarations: &mut Vec<String>,
    children: &mut Vec<JsScope>,
) {
    match node {
        js::Node::Function(function) => {
            if renameable.contains(&function.name) {
                declarations.push(function.name.clone());
            }
            children.push(function_scope(
                &function.parameters,
                &function.body,
                renameable,
            ));
        }
        js::Node::Closure(closure) => {
            children.push(function_scope(
                &closure.parameters,
                &closure.body,
                renameable,
            ));
        }
        js::Node::ConstVariable(variable) | js::Node::LetVariable(variable) => {
            if renameable.contains(&variable.name) {
                declarations.push(variable.name.clone());
            }
            collect_node(&variable.value, renameable, declarations, children);
        }
        js::Node::ForOf(binding, iterable, body) => {
            if renameable.contains(binding) {
                declarations.push(binding.clone());
            }
            collect_node(iterable, renameable, declarations, children);
            collect_declarations(body, renameable, declarations, children);
        }
        js::Node::While(condition, body) => {
            collect_node(condition, renameable, declarations, children);
            collect_declarations(body, renameable, declarations, children);
        }
        js::Node::If(branch) => collect_if(branch, renameable, declarations, children),
        js::Node::Try(body, finally) => {
            collect_declarations(body, renameable, declarations, children);
            collect_declarations(finally, renameable, declarations, children);
        }
        js::Node::Call(subject, arguments) => {
            collect_node(subject, renameable, declarations, children);
            collect_declarations(arguments, renameable, declarations, children);
        }
        js::Node::Assignment(left, right)
        | js::Node::Binary(_, left, right)
        | js::Node::PropertyIndex(left, right) => {
            collect_node(left, renameable, declarations, children);
            collect_node(right, renameable, declarations, children);
        }
        js::Node::Await(inner)
        | js::Node::Unary(_, inner)
        | js::Node::Return(inner)
        | js::Node::Throw(inner)
        | js::Node::Spread(inner)
        | js::Node::Property(inner, _) => collect_node(inner, renameable, declarations, children),
        js::Node::Array(items) => collect_declarations(items, renameable, declarations, children),
        js::Node::Local(_)
        | js::Node::String(_)
        | js::Node::Number(_, _)
        | js::Node::Bool(_)
        | js::Node::Null
        | js::Node::Void
        | js::Node::Break
        | js::Node::Continue => {}
    }
}

fn collect_if(
    branch: &js::IfBranch,
    renameable: &HashSet<String>,
    declarations: &mut Vec<String>,
    children: &mut Vec<JsScope>,
) {
    match branch {
        js::IfBranch::If(condition, body, else_branch) => {
            collect_node(condition, renameable, declarations, children);
            collect_declarations(body, renameable, declarations, children);
            if let Some(else_branch) = else_branch {
                collect_if(else_branch, renameable, declarations, children);
            }
        }
        js::IfBranch::Else(body) => collect_declarations(body, renameable, declarations, children),
    }
}

/// Allocates names over the scope tree, top-down. A scope's bindings get names
/// not used by an ancestor (no shadowing) or a same-scope sibling; disjoint
/// scopes (passed the same inherited set) reuse freely. `release` picks the
/// shortest obfuscated name; otherwise the binding's source name, disambiguated.
///
/// `holder` says which BINDING each name in `used` belongs to, for the names
/// this pass allocated (a reserved name belongs to nobody and is absent). It is
/// what separates a binding legitimately meeting itself from two bindings
/// meeting — see the duplicate-emission branch below.
fn allocate_scope(
    scope: &JsScope,
    inherited: &HashSet<String>,
    holder: &HashMap<String, String>,
    release: bool,
    source_of: &HashMap<String, String>,
    rename: &mut HashMap<String, String>,
) {
    let mut used = inherited.clone();
    let mut holder = holder.clone();
    for old in &scope.declarations {
        // One generated name is one binding, even where the emitter writes that
        // binding out more than once: every instance of a monomorphized generic
        // repeats its body's names. The rename map is keyed by NAME, so all of a
        // binding's emission sites must land on one answer: take the allocation
        // already made rather than minting a second, disagreeing one, which
        // would rewrite the earlier site to a name chosen against a scope it is
        // not in. (This branch also covered a nested free `fun`, which was
        // emitted both nested and at module level until B71 stopped the item
        // walk visiting it twice. The generic instances keep it live.)
        if let Some(allocated) = rename.get(old).cloned() {
            // Meeting the name again is expected when the binding meets ITSELF —
            // two instances of one generic body are the same bindings written
            // twice. Two DIFFERENT bindings under one name is the collision this
            // pass exists to prevent, and there is no name a name-keyed rename
            // could give them both.
            debug_assert!(
                !used.contains(&allocated)
                    || holder.get(&allocated).map(String::as_str) == Some(old.as_str()),
                "`{old}` is declared in a scope where `{allocated}`, the name allocated for it \
                 elsewhere, already belongs to a different binding"
            );
            used.insert(allocated.clone());
            holder.insert(allocated, old.clone());
            continue;
        }
        let new = if release {
            shortest_available(&used)
        } else {
            // Readable: `renameable` only holds source-named bindings, so this is
            // always present.
            disambiguated(source_of.get(old).unwrap_or(old), &used)
        };
        rename.insert(old.clone(), new.clone());
        used.insert(new.clone());
        holder.insert(new, old.clone());
    }
    for child in &scope.children {
        allocate_scope(child, &used, &holder, release, source_of, rename);
    }
}

/// Applies the rename map to every binding and reference in the tree. Property
/// names and untouched identifiers (externs, helpers — never in the map) are left
/// as-is. Must be exhaustive: a missed reference would dangle.
fn rename_nodes(nodes: &mut [js::Node], rename: &HashMap<String, String>) {
    for node in nodes {
        rename_node(node, rename);
    }
}

fn rename_one(name: &mut String, rename: &HashMap<String, String>) {
    if let Some(new) = rename.get(name) {
        *name = new.clone();
    }
}

fn rename_node(node: &mut js::Node, rename: &HashMap<String, String>) {
    match node {
        js::Node::Local(name) => rename_one(name, rename),
        js::Node::Function(function) => {
            rename_one(&mut function.name, rename);
            for parameter in &mut function.parameters {
                rename_one(&mut parameter.name, rename);
            }
            rename_nodes(&mut function.body, rename);
        }
        js::Node::Closure(closure) => {
            for parameter in &mut closure.parameters {
                rename_one(&mut parameter.name, rename);
            }
            rename_nodes(&mut closure.body, rename);
        }
        js::Node::ConstVariable(variable) | js::Node::LetVariable(variable) => {
            rename_one(&mut variable.name, rename);
            rename_node(&mut variable.value, rename);
        }
        js::Node::ForOf(binding, iterable, body) => {
            rename_one(binding, rename);
            rename_node(iterable, rename);
            rename_nodes(body, rename);
        }
        js::Node::While(condition, body) => {
            rename_node(condition, rename);
            rename_nodes(body, rename);
        }
        js::Node::If(branch) => rename_if(branch, rename),
        js::Node::Try(body, finally) => {
            rename_nodes(body, rename);
            rename_nodes(finally, rename);
        }
        js::Node::Call(subject, arguments) => {
            rename_node(subject, rename);
            rename_nodes(arguments, rename);
        }
        js::Node::Assignment(left, right)
        | js::Node::Binary(_, left, right)
        | js::Node::PropertyIndex(left, right) => {
            rename_node(left, rename);
            rename_node(right, rename);
        }
        js::Node::Await(inner)
        | js::Node::Unary(_, inner)
        | js::Node::Return(inner)
        | js::Node::Throw(inner)
        | js::Node::Spread(inner)
        // `Property`'s member is a property name, not a binding — recurse only the subject.
        | js::Node::Property(inner, _) => rename_node(inner, rename),
        js::Node::Array(items) => rename_nodes(items, rename),
        js::Node::String(_)
        | js::Node::Number(_, _)
        | js::Node::Bool(_)
        | js::Node::Null
        | js::Node::Void
        | js::Node::Break
        | js::Node::Continue => {}
    }
}

fn rename_if(branch: &mut js::IfBranch, rename: &HashMap<String, String>) {
    match branch {
        js::IfBranch::If(condition, body, else_branch) => {
            rename_node(condition, rename);
            rename_nodes(body, rename);
            if let Some(else_branch) = else_branch {
                rename_if(else_branch, rename);
            }
        }
        js::IfBranch::Else(body) => rename_nodes(body, rename),
    }
}

/// Re-allocates the program's binding names over its JS scope tree (see the
/// machinery above) and rewrites the node tree. A no-op for the annotated style
/// (its names carry `/*source*/` comments the rename can't cleanly reuse).
///
/// **The uniqueness invariant (B69).** This pass hands out names from a pool —
/// `a, b, c, …` under release, source names under readable — while leaving
/// alone every name it was not asked to re-allocate. That is sound if and only
/// if the two sets cannot meet, and there are exactly two ways for a generated
/// name to end up in the left-alone set: it is not `renameable`, or the collect
/// walk never reached its declaration. Both are closed here — the first by
/// re-allocating the generator's whole minted set under release, the second by
/// RESERVING whatever the walk did not reach. Every generated name is therefore
/// either re-allocated against a scope's `used` set or reserved in every scope,
/// and a binding the walk misses can only come out over-long, never colliding.
fn rename_for_scopes(ng: &NameGenerator, program: &Program, nodes: &mut Vec<js::Node>) {
    let release = match ng.seed.style {
        NameStyle::Annotated => return,
        NameStyle::Readable => false,
        NameStyle::Plain => true,
    };
    // Each renameable binding's current (unique) name -> its source name.
    let mut source_of: HashMap<String, String> = HashMap::default();
    for (id, name) in &ng.names {
        if let Some(source) = ng.seed.source_names.get(id) {
            source_of.insert(name.clone(), source.clone());
        }
    }
    // Release re-allocates EVERY name the generator minted — including the
    // anonymous temps (`ng.names` holds only the id-keyed ones), whose names come
    // out of the very `a, b, c, …` alphabet `shortest_available` draws from.
    // Readable re-allocates only the source-named bindings, and can leave the
    // temps alone safely because those are `$`-prefixed and no source-derived
    // name ever contains a `$`.
    let renameable: HashSet<String> = if release {
        ng.minted.clone()
    } else {
        source_of.keys().cloned().collect()
    };
    if renameable.is_empty() {
        return;
    }
    // The reserved set (keywords, referenced globals, `__`-helpers, the program's
    // `[extern]` symbols) counts as used in every scope, so nothing collides.
    let mut reserved = collect_reserved_names(program);
    let mut declarations = Vec::new();
    let mut children = Vec::new();
    collect_declarations(nodes, &renameable, &mut declarations, &mut children);
    let global = JsScope {
        declarations,
        children,
    };
    // The second half of the invariant. The collect walk is a hand-written walk
    // over the node tree and may be incomplete; a declaration it misses keeps
    // the name the generator minted, which is a name this pass can otherwise
    // mint again. Reserving the unreached names makes the allocator's output
    // disjoint from the kept names whatever the walk did or did not see.
    let mut reached = HashSet::default();
    collect_reached_names(&global, &mut reached);
    reserved.extend(
        renameable
            .iter()
            .filter(|name| !reached.contains(*name))
            .cloned(),
    );
    let mut rename = HashMap::default();
    allocate_scope(
        &global,
        &reserved,
        &HashMap::default(),
        release,
        &source_of,
        &mut rename,
    );
    debug_assert!(
        renameable
            .iter()
            .all(|name| rename.contains_key(name) || reserved.contains(name)),
        "a generated name was neither re-allocated nor reserved — it can collide"
    );
    rename_nodes(nodes, &rename);
}

/// Every binding name the scope tree accounts for — the walk's reach, which is
/// what `rename_for_scopes` reserves the complement of.
fn collect_reached_names(scope: &JsScope, reached: &mut HashSet<String>) {
    reached.extend(scope.declarations.iter().cloned());
    for child in &scope.children {
        collect_reached_names(child, reached);
    }
}

#[cfg(test)]
mod tests {
    use super::{Formatter, unescape_string};

    /// The junctions where dropping the padding would change the token stream.
    /// Only `- -` is reachable from Vilan source today (`-` is the only
    /// arithmetic unary the language has), so the rest of the rule is stated
    /// here or nowhere: the day a `+` unary or a regex literal arrives, the
    /// printer must already be right rather than newly wrong.
    #[test]
    fn tight_printing_separates_only_the_pairs_that_would_fuse() {
        let tight = Formatter::from_options(false, false);
        // `3 - -(2)` must not become `3--(2)`, a postfix decrement.
        assert_eq!(tight.between("3", "-"), "");
        assert_eq!(tight.between("-", "-(2)"), " ");
        // The same for the increment operator, and for both comment openers.
        assert_eq!(tight.between("+", "+(2)"), " ");
        assert_eq!(tight.between("/", "/(2)"), " ");
        assert_eq!(tight.between("/", "*(2)"), " ");
        // Everything else stays tight — a rule that always padded would be
        // correct and would also stop minifying.
        assert_eq!(tight.between("7", "-"), "");
        assert_eq!(tight.between("-", "9"), "");
        assert_eq!(tight.between("*", "-(2)"), "");
        assert_eq!(tight.between("-", "(2)"), "");
        assert_eq!(tight.between("===", "-(2)"), "");
        // An empty side has no last/first character to fuse with.
        assert_eq!(tight.between("", "-"), "");
        assert_eq!(tight.between("-", ""), "");
    }

    /// Padded output already separates every junction, so the rule costs it
    /// nothing and must not double a space.
    #[test]
    fn padded_printing_is_unchanged_by_the_fusing_rule() {
        let padded = Formatter::from_options(true, true);
        assert_eq!(padded.between("-", "-(2)"), " ");
        assert_eq!(padded.between("7", "-"), " ");
        assert_eq!(padded.between("", ""), " ");
    }

    /// A string literal's value is built from the NORMALIZED source text
    /// (windows-support.md §2, spec §2): a `\r\n` in the file is one line
    /// terminator, so a multi-line literal carries `\n` however the file was
    /// saved. Plain `"…"` and the literal fragments of an `i"…"` both arrive
    /// here, so both are covered.
    #[test]
    fn a_crlf_line_break_in_a_literal_becomes_one_newline() {
        assert_eq!(unescape_string("alpha\r\nbeta"), "alpha\nbeta");
        assert_eq!(unescape_string("a\r\nb\r\nc"), "a\nb\nc");
    }

    #[test]
    fn a_lone_carriage_return_in_a_literal_is_preserved() {
        // Classic-Mac endings are deliberately NOT blessed: a `\r` with no
        // following `\n` is an ordinary character of the value.
        assert_eq!(unescape_string("a\rb"), "a\rb");
    }

    #[test]
    fn an_escaped_carriage_return_survives_normalization() {
        // `\r` WRITTEN in the literal is a value the author asked for — only a
        // line ending read off the file normalizes. `\r\n` typed as escapes
        // stays a two-character CRLF.
        assert_eq!(unescape_string(r"a\r\nb"), "a\r\nb");
        assert_eq!(unescape_string(r"a\rb"), "a\rb");
    }

    #[test]
    fn a_literal_needing_neither_pass_is_borrowed() {
        let raw = "plain text";
        match unescape_string(raw) {
            std::borrow::Cow::Borrowed(borrowed) => assert!(std::ptr::eq(borrowed, raw)),
            std::borrow::Cow::Owned(_) => {
                panic!("an escape-free, CR-free literal must not allocate")
            }
        }
    }

    #[test]
    fn escapes_and_a_crlf_line_break_compose() {
        // Both passes on one literal: the CRLF folds, the escapes interpret.
        assert_eq!(unescape_string("a\\t\r\nb\\\"c"), "a\t\nb\"c");
    }
}
