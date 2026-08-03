//! Route-chunk planning — bundle splitting's S1, analysis only
//! (proposal/bundle-splitting.md). Finds the splittable route matches (a
//! `match` on a `View.swap` render closure's parameter), attributes each
//! arm's calls by SPAN NESTING (a call belongs to the arm whose body span
//! contains it — no expression walker needed), and partitions the call
//! graph: reachable from the eager root (`main` + module bindings, with
//! arm calls held out) → eager; from exactly one arm → that arm's chunk;
//! from two or more arms → shared, which v1 sends eager. Nothing here
//! changes emission; the plan is a report.

use std::collections::HashSet;

use crate::analyzer::{Expr, Program, SourceId};
use crate::id::Id;

/// One splittable arm's would-be chunk.
pub struct Chunk {
    /// The arm's pattern, as written (sliced from the entry source).
    pub arm: String,
    /// The chunk's functions, by name, sorted.
    pub functions: Vec<String>,
    /// The summed source-span bytes of those functions — an estimate.
    pub bytes: usize,
}

/// The whole plan for one entry.
pub struct ChunkPlan {
    /// Splittable route matches found (v1 recognizes the `swap` shape only).
    pub sites: usize,
    /// Functions every path needs (or that two or more arms share).
    pub eager_functions: usize,
    /// Functions shared by two or more arms (a subset of the eager count —
    /// v2's shared-chunk candidates).
    pub shared_functions: usize,
    pub chunks: Vec<Chunk>,
}

/// Computes the plan.
pub fn plan(program: &Program<'_>) -> ChunkPlan {
    let empty = ChunkPlan {
        sites: 0,
        eager_functions: 0,
        shared_functions: 0,
        chunks: Vec::new(),
    };
    let Some(swap_fn) = swap_function(program) else {
        return empty;
    };
    let sites = splittable_sites(program, swap_fn);
    if sites.is_empty() {
        return empty;
    }

    let graph = program.call_graph();
    let held_out: Vec<(Id, Vec<(SourceId, usize, usize)>)> = sites
        .iter()
        .map(|site| (site.closure, site.arm_spans(program)))
        .collect();

    // Eager reach: main + every module binding, with arm-attributed edges
    // held out at each recognized render closure.
    let mut eager = HashSet::new();
    let mut queue: Vec<Id> = Vec::new();
    if let Some(main) = program
        .functions
        .iter()
        .find(|(id, function)| {
            function.name == "main" && program.source_of(**id) == Some(SourceId(0))
        })
        .map(|(id, _)| *id)
    {
        queue.push(main);
    }
    queue.extend(program.module_level_bindings());
    while let Some(node) = queue.pop() {
        if !eager.insert(node) {
            continue;
        }
        for next in expand(program, &graph, node, &held_out) {
            queue.push(next);
        }
    }

    // Per-arm reach, full expansion.
    let mut arm_reach: Vec<(String, HashSet<Id>)> = Vec::new();
    for site in &sites {
        for arm in &site.arms {
            let mut reach = HashSet::new();
            let mut queue = arm.seeds(program);
            while let Some(node) = queue.pop() {
                if !reach.insert(node) {
                    continue;
                }
                for next in expand(program, &graph, node, &[]) {
                    queue.push(next);
                }
            }
            arm_reach.push((arm.name.clone(), reach));
        }
    }

    // Membership.
    let mut shared = 0usize;
    let mut eager_functions = 0usize;
    let mut chunks: Vec<Chunk> = arm_reach
        .iter()
        .map(|(pattern, _)| Chunk {
            arm: pattern.clone(),
            functions: Vec::new(),
            bytes: 0,
        })
        .collect();
    for (id, function) in &program.functions {
        if program
            .source_of(*id)
            .is_none_or(|source| program.std_sources.contains(&source))
        {
            // Std is never chunked — it is the shared runtime, eager by
            // residence (and mostly tree-shaken anyway). App code is
            // chunkable wherever it lives: entry-only would plan ZERO chunks
            // for the common real shape (pages in a `views` module — the
            // walkthrough example), which S1's sweep caught.
            continue;
        }
        if eager.contains(id) {
            eager_functions += 1;
            continue;
        }
        let owners: Vec<usize> = arm_reach
            .iter()
            .enumerate()
            .filter(|(_, (_, reach))| reach.contains(id))
            .map(|(index, _)| index)
            .collect();
        match owners.as_slice() {
            [] => {}
            [only] => {
                let bytes = program
                    .span_map
                    .get(id)
                    .map(|span| span.end.saturating_sub(span.start))
                    .unwrap_or(0);
                chunks[*only].functions.push(function.name.to_string());
                chunks[*only].bytes += bytes;
            }
            _ => {
                shared += 1;
                eager_functions += 1;
            }
        }
    }
    for chunk in &mut chunks {
        chunk.functions.sort();
    }
    chunks.retain(|chunk| !chunk.functions.is_empty());

    ChunkPlan {
        sites: sites.len(),
        eager_functions,
        shared_functions: shared,
        chunks,
    }
}

/// The std `View.swap` method, when the browser layer is loaded.
fn swap_function(program: &Program<'_>) -> Option<Id> {
    let view_struct = program.structs.iter().find_map(|(id, struct_)| {
        (struct_.name == "View"
            && program
                .source_of(*id)
                .is_some_and(|source| program.std_sources.contains(&source)))
        .then_some(*id)
    })?;
    program.implementations.iter().find_map(|implementation| {
        matches!(
            program.type_id_to_type_map.get(&implementation.subject),
            Some(crate::type_::Type::Struct(id, _)) if *id == view_struct
        )
        .then(|| implementation.declarations.get("swap").copied())
        .flatten()
    })
}

/// One recognized `.swap(signal, |current| match current { .. })` site.
struct Site {
    closure: Id,
    arms: Vec<Arm>,
}

struct Arm {
    name: String,
    body: Id,
}

impl Site {
    /// The arm bodies' (source, span) ranges. Span offsets are file-local,
    /// so a range is meaningless without its source — a raw-offset match
    /// would confuse entities from different files that happen to overlap.
    fn arm_spans(&self, program: &Program<'_>) -> Vec<(SourceId, usize, usize)> {
        self.arms
            .iter()
            .filter_map(|arm| {
                let source = program.source_of(arm.body)?;
                program
                    .span_map
                    .get(&arm.body)
                    .map(|span| (source, span.start, span.end))
            })
            .collect()
    }
}

impl Arm {
    /// The arm's direct roots: every function or closure whose defining call
    /// or body sits inside the arm body's span.
    fn seeds(&self, program: &Program<'_>) -> Vec<Id> {
        let Some(body_span) = program.span_map.get(&self.body) else {
            return Vec::new();
        };
        let body_source = program.source_of(self.body);
        // Same source AND span-nested: offsets are file-local, so nesting
        // only means containment within the arm's own file.
        let inside = |id: Id| {
            program.source_of(id) == body_source
                && program
                    .span_map
                    .get(&id)
                    .is_some_and(|span| span.start >= body_span.start && span.end <= body_span.end)
        };
        let mut seeds = Vec::new();
        for (call_id, call) in &program.function_calls {
            if !inside(*call_id) {
                continue;
            }
            if let Some(Expr::Local(target)) = program.entity_map.get(&call.subject_id)
                && (program.functions.contains_key(target)
                    || program.external_functions.contains_key(target))
            {
                seeds.push(*target);
            }
        }
        for closure_id in program.closures.keys() {
            if inside(*closure_id) {
                seeds.push(*closure_id);
            }
        }
        seeds
    }
}

/// The recognized splittable sites: calls to `swap` whose last closure
/// argument's body is a `match` on that closure's parameter.
fn splittable_sites(program: &Program<'_>, swap_fn: Id) -> Vec<Site> {
    let mut sites = Vec::new();
    for call in program.function_calls.values() {
        let Some(Expr::Local(target)) = program.entity_map.get(&call.subject_id) else {
            continue;
        };
        if *target != swap_fn {
            continue;
        }
        let Some(closure_id) = call.argument_ids.iter().rev().find_map(|argument| {
            match program.entity_map.get(argument) {
                Some(Expr::Closure(closure_id)) | Some(Expr::Async(closure_id)) => {
                    Some(*closure_id)
                }
                _ => None,
            }
        }) else {
            continue;
        };
        let Some(closure) = program.closures.get(&closure_id) else {
            continue;
        };
        let match_id = match program.entity_map.get(&closure.return_) {
            Some(Expr::Match(..)) => closure.return_,
            Some(Expr::Block((_, tail))) => match program.entity_map.get(tail) {
                Some(Expr::Match(..)) => *tail,
                _ => continue,
            },
            _ => continue,
        };
        let Some(Expr::Match(subject, legs)) = program.entity_map.get(&match_id) else {
            continue;
        };
        // The subject must be the render closure's own parameter.
        let subject_is_parameter = match program.entity_map.get(subject) {
            Some(Expr::Local(local)) => {
                closure.parameters.contains(local)
                    || matches!(
                        program.entity_map.get(local),
                        Some(Expr::Parameter(parameter)) if closure.parameters.contains(parameter)
                    )
            }
            Some(Expr::Parameter(parameter)) => closure.parameters.contains(parameter),
            _ => false,
        };
        if !subject_is_parameter {
            continue;
        }
        sites.push(Site {
            closure: closure_id,
            arms: legs
                .iter()
                .map(|leg| Arm {
                    name: pattern_name(program, &leg.pattern),
                    body: leg.body,
                })
                .collect(),
        });
    }
    sites
}

/// Renders an arm's pattern from the resolved program — enum names, not
/// source slices, so the report reads `Route::Items(..)`.
fn pattern_name(program: &Program<'_>, pattern: &crate::analyzer::ExprPattern) -> String {
    use crate::analyzer::ExprPattern;
    match pattern {
        ExprPattern::Wildcard => "_".to_string(),
        ExprPattern::Binding(id) => program
            .variables
            .get(id)
            .map(|variable| format!("let {}", variable.name))
            .unwrap_or_else(|| "(binding)".to_string()),
        ExprPattern::Variant(enum_id, index, subs) => {
            let rendered = program
                .enums
                .get(enum_id)
                .and_then(|enum_| {
                    enum_
                        .variants
                        .get(*index)
                        .map(|variant| format!("{}::{}", enum_.name, variant.name))
                })
                .unwrap_or_else(|| "(variant)".to_string());
            if subs.is_empty() {
                rendered
            } else {
                format!("{rendered}(..)")
            }
        }
        _ => "(pattern)".to_string(),
    }
}

/// One BFS expansion step: graph successors plus closure descent, with the
/// recognized render closures' arm-attributed edges held out (matched by
/// call-site span inside an arm body span).
fn expand(
    program: &Program<'_>,
    graph: &crate::call_graph::CallGraph,
    node: Id,
    held_out: &[(Id, Vec<(SourceId, usize, usize)>)],
) -> Vec<Id> {
    let holds: Option<&Vec<(SourceId, usize, usize)>> = held_out
        .iter()
        .find(|(closure, _)| *closure == node)
        .map(|(_, spans)| spans);
    let site_held = |site: Option<Id>| {
        let (Some(spans), Some(site)) = (holds, site) else {
            return false;
        };
        let site_source = program.source_of(site);
        program.span_map.get(&site).is_some_and(|span| {
            spans.iter().any(|(source, start, end)| {
                site_source == Some(*source) && span.start >= *start && span.end <= *end
            })
        })
    };
    let mut next: Vec<Id> = Vec::new();
    for (callee, site) in graph.successors(program, node) {
        if site_held(site) {
            continue;
        }
        next.push(callee);
    }
    for child in graph.closure_children_of(node).unwrap_or(&[]) {
        if site_held(Some(*child)) {
            continue;
        }
        next.push(*child);
    }
    next.extend(graph.initializer_closures_of(node).iter().copied());
    next
}

/// Renders the plan as the `--print-chunks` report.
pub fn render(plan: &ChunkPlan, entry_name: &str) -> String {
    let mut out = String::new();
    if plan.sites == 0 {
        out.push_str(&format!(
            "[vilan chunks] {entry_name}: no splittable route matches\n"
        ));
        return out;
    }
    out.push_str(&format!(
        "[vilan chunks] {entry_name}: {} splittable match{}, {} route chunk{} (estimate; node-level reachability)\n",
        plan.sites,
        if plan.sites == 1 { "" } else { "es" },
        plan.chunks.len(),
        if plan.chunks.len() == 1 { "" } else { "s" },
    ));
    out.push_str(&format!(
        "  eager: {} entry function{} ({} shared by 2+ arms)\n",
        plan.eager_functions,
        if plan.eager_functions == 1 { "" } else { "s" },
        plan.shared_functions,
    ));
    for chunk in &plan.chunks {
        out.push_str(&format!(
            "  chunk `{}`: {} function{}, ~{} bytes ({})\n",
            chunk.arm,
            chunk.functions.len(),
            if chunk.functions.len() == 1 { "" } else { "s" },
            chunk.bytes,
            chunk.functions.join(", "),
        ));
    }
    out
}
