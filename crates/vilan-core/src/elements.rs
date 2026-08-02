//! The element desugar (proposal/element-syntax.md §4).
//!
//! An element expression `<div …> … </div>` parses as a `Node::Element`; this
//! pass — pure Node → Node, run once per parsed tree immediately before
//! `lift::rewrite_items` at each of its call sites — rewrites every element
//! into the `view("tag")` method chain it sugars, building exactly the node
//! shapes a hand-written chain parses to (each link is
//! `MemberAccessor(subject, Call(Accessor(m), generics, args))`), so the
//! lowering IS the chain: identical trees, identical emitted JS. The
//! analyzer, transformer, and interpreter never see an element node.
//!
//! Every generated node carries the span of the markup segment it came from —
//! the tag span for `view("tag")`, the head item's span for its link, the
//! child's span for its `.child(…)` — so diagnostics land on what the user
//! wrote. Tag and attribute NAMES are stored as spans by the parser (keyword
//! and hyphenated names span tokens carrying no text) and sliced from the
//! source here, where it is in scope.
//!
//! The pass is the identity on element-free trees: a cheap `contains_element`
//! prefilter (riding `for_each_child`, like lift's mark detection) leaves
//! untouched nodes unrebuilt.

use crate::node::{Closure, ElementBody, ElementHeadItem, If, Node, NodeIfBranch, NodeList};
use crate::span::{Span, Spanned};

/// Rewrite every element in a parsed tree, in place. Called at each
/// `lift::rewrite_items` site, immediately before it — which covers the entry
/// file, every loaded module, and parsed macro-expansion output.
pub fn rewrite_items<'src>(items: &mut NodeList<'src>, source: &'src str) {
    for item in items.iter_mut() {
        if contains_element(&item.0) {
            take_and_desugar(item, source);
        }
    }
}

fn contains_element(node: &Node<'_>) -> bool {
    if matches!(node, Node::Element(_)) {
        return true;
    }
    let mut found = false;
    node.for_each_child(&mut |child| {
        if !found && contains_element(&child.0) {
            found = true;
        }
    });
    found
}

fn take_and_desugar<'src>(slot: &mut Spanned<Node<'src>>, source: &'src str) {
    let span = slot.1;
    let owned = std::mem::replace(&mut slot.0, Node::Error);
    *slot = desugar((owned, span), source);
}

fn desugar<'src>(node: Spanned<Node<'src>>, source: &'src str) -> Spanned<Node<'src>> {
    let (kind, span) = node;
    match kind {
        Node::Element(body) => {
            // Interior first: head links, handlers, attribute values, and
            // children may themselves contain elements.
            let body = desugar_interior(body, source);
            build_chain(body, span, source)
        }
        other => descend((other, span), source),
    }
}

fn desugar_interior<'src>(mut body: ElementBody<'src>, source: &'src str) -> ElementBody<'src> {
    for item in body.head.iter_mut() {
        match item {
            ElementHeadItem::Chain(link) => take_and_desugar(link, source),
            ElementHeadItem::Event(_, handler) => take_and_desugar(handler, source),
            ElementHeadItem::Attribute(_, value) => {
                if let Some(value) = value {
                    take_and_desugar(value, source);
                }
            }
        }
    }
    for child in body.children.iter_mut() {
        take_and_desugar(child.node_mut(), source);
    }
    body
}

/// The lowering table (proposal/element-syntax.md §4): `view("tag")`, then the
/// head items in written order, then a `.child(…)` per child in written order.
fn build_chain<'src>(
    body: ElementBody<'src>,
    span: Span,
    source: &'src str,
) -> Spanned<Node<'src>> {
    let ElementBody {
        tag,
        head,
        children,
        self_closing: _,
        close_tag: _,
    } = body;
    let tag_text = &source[tag.into_range()];
    // The generated `view` accessor spans `<tag` — an unresolved `view` (the
    // import is missing) then underlines the element head itself, which is
    // what the user wrote. The tailored import note rides S4 with the docs.
    let head_span: Span = (span.start..tag.end).into();
    let mut chain: Spanned<Node<'src>> = (
        Node::Call(
            Box::new((Node::Accessor("view"), head_span)),
            None,
            (vec![(Node::String(tag_text), tag)], tag),
        ),
        head_span,
    );
    for item in head {
        let member = match item {
            // A chain link splices verbatim — closure literals stay at the
            // call site, so the context model is untouched.
            ElementHeadItem::Chain(link) => link,
            ElementHeadItem::Event((event, event_span), handler) => {
                let handler = *handler;
                // Literal arity dispatches: `|| …` is `.on`, `|e| …` is
                // `.on_event`; anything else means `.on` (a named
                // one-parameter handler is written in chain form).
                let method = match &handler.0 {
                    Node::Closure(Closure { parameters, .. }) if parameters.0.len() == 1 => {
                        "on_event"
                    }
                    _ => "on",
                };
                let item_span: Span = (event_span.start..handler.1.end).into();
                // The scaffolding accessor takes a ZERO-WIDTH span: the event
                // name belongs to the markup (the LSP paints it as an
                // attribute), not to the generated method reference.
                let anchor: Span = (event_span.start..event_span.start).into();
                (
                    Node::Call(
                        Box::new((Node::Accessor(method), anchor)),
                        None,
                        (vec![(Node::String(event), event_span), handler], item_span),
                    ),
                    item_span,
                )
            }
            ElementHeadItem::Attribute(name, value) => {
                let name_text = &source[name.into_range()];
                let end = value.as_ref().map(|value| value.1.end).unwrap_or(name.end);
                let item_span: Span = (name.start..end).into();
                // A bare name is a boolean attribute — present, empty value.
                let value = value.unwrap_or((Node::String(""), name));
                // Zero-width scaffolding span, as for events: the attribute
                // name is markup, not a method reference.
                let anchor: Span = (name.start..name.start).into();
                (
                    Node::Call(
                        Box::new((Node::Accessor("attr"), anchor)),
                        None,
                        (vec![(Node::String(name_text), name), value], item_span),
                    ),
                    item_span,
                )
            }
        };
        chain = attach(chain, member);
    }
    for child in children {
        let child = child.into_node();
        let child_span = child.1;
        // Zero-width scaffolding span: a `.child` link generated for a hole
        // must not shadow the hole expression's own tokens (they share the
        // wide span otherwise, and the LSP's overlap sweep tie-breaks
        // nondeterministically).
        let anchor: Span = (child_span.start..child_span.start).into();
        let member = (
            Node::Call(
                Box::new((Node::Accessor("child"), anchor)),
                None,
                (vec![child], child_span),
            ),
            child_span,
        );
        chain = attach(chain, member);
    }
    (chain.0, span)
}

/// One chain link: `MemberAccessor(subject, member)` with the grown span,
/// exactly as `apply_postfix` builds it for a written `.m(…)`.
fn attach<'src>(subject: Spanned<Node<'src>>, member: Spanned<Node<'src>>) -> Spanned<Node<'src>> {
    let span: Span = (subject.1.start..member.1.end).into();
    (
        Node::MemberAccessor(Box::new(subject), Box::new(member)),
        span,
    )
}

/// Recurse into a node's interior expression positions, rebuilding it — the
/// same container coverage as `lift::descend`, with one divergence: a
/// `LiftGroup` is preserved (this pass runs BEFORE lift, which consumes it).
fn descend<'src>(node: Spanned<Node<'src>>, source: &'src str) -> Spanned<Node<'src>> {
    let (kind, span) = node;
    let kind = match kind {
        Node::LiftGroup(inner) => Node::LiftGroup(desugar_boxed(inner, source)),
        Node::Block(mut body) => {
            desugar_block(&mut body.0, source);
            Node::Block(body)
        }
        Node::If(branch) => Node::If(descend_if(branch, source)),
        Node::Match(subject, mut legs) => {
            let subject = desugar_boxed(subject, source);
            for (patterns, guard, body) in legs.0.iter_mut() {
                let _ = patterns;
                if let Some(guard) = guard {
                    take_and_desugar(guard, source);
                }
                take_and_desugar(body, source);
            }
            Node::Match(subject, legs)
        }
        Node::For(condition, mut body) => {
            desugar_block(&mut body.0, source);
            Node::For(condition.map(|inner| desugar_boxed(inner, source)), body)
        }
        Node::ForIn(binding, iterable, mut body) => {
            let iterable = desugar_boxed(iterable, source);
            desugar_block(&mut body.0, source);
            Node::ForIn(binding, iterable, body)
        }
        Node::Func(mut function) => {
            if let Some(body) = function.body.as_mut() {
                desugar_block(&mut body.0, source);
            }
            Node::Func(function)
        }
        Node::Closure(Closure {
            parameters,
            return_type,
            return_value,
        }) => Node::Closure(Closure {
            parameters,
            return_type,
            return_value: desugar_boxed(return_value, source),
        }),
        Node::Let(name, annotation, value, mutable) => {
            Node::Let(name, annotation, desugar_opt(value, source), mutable)
        }
        Node::LetDestructure(pattern, annotation, value, mutable) => {
            Node::LetDestructure(pattern, annotation, desugar_opt(value, source), mutable)
        }
        Node::Assign(target, op, value) => Node::Assign(
            desugar_boxed(target, source),
            op,
            desugar_boxed(value, source),
        ),
        Node::Call(subject, generics, mut arguments) => {
            let subject = desugar_boxed(subject, source);
            desugar_list(&mut arguments.0, source);
            Node::Call(subject, generics, arguments)
        }
        Node::Index(subject, index) => {
            Node::Index(desugar_boxed(subject, source), desugar_boxed(index, source))
        }
        Node::MemberAccessor(subject, member) => Node::MemberAccessor(
            desugar_boxed(subject, source),
            desugar_boxed(member, source),
        ),
        Node::List(mut items) => {
            desugar_list(&mut items, source);
            Node::List(items)
        }
        Node::Tuple(mut items) => {
            desugar_list(&mut items, source);
            Node::Tuple(items)
        }
        Node::Repeat(value, length) => {
            Node::Repeat(desugar_boxed(value, source), desugar_boxed(length, source))
        }
        Node::StructInitializer(name, generics, mut fields) => {
            for field in fields.0.iter_mut() {
                if let Some(value) = field.0.1.as_mut() {
                    take_and_desugar(value, source);
                }
            }
            Node::StructInitializer(name, generics, fields)
        }
        Node::Binary(op, left, right) => Node::Binary(
            op,
            desugar_boxed(left, source),
            desugar_boxed(right, source),
        ),
        Node::Unary(op, inner) => Node::Unary(op, desugar_boxed(inner, source)),
        Node::Reference(mutable, inner) => Node::Reference(mutable, desugar_boxed(inner, source)),
        Node::Dereference(inner) => Node::Dereference(desugar_boxed(inner, source)),
        Node::TryAssert(inner) => Node::TryAssert(desugar_boxed(inner, source)),
        Node::Await(inner) => Node::Await(desugar_boxed(inner, source)),
        Node::Async(inner) => Node::Async(desugar_boxed(inner, source)),
        Node::FuncReturn(value) => Node::FuncReturn(desugar_opt(value, source)),
        Node::Export(inner) => Node::Export(desugar_boxed(inner, source)),
        Node::Const(inner) => Node::Const(desugar_boxed(inner, source)),
        Node::Derive(names, inner) => Node::Derive(names, desugar_boxed(inner, source)),
        Node::Service(name, inner) => Node::Service(name, desugar_boxed(inner, source)),
        Node::MacroAttribute(name, name_span, arguments, inner) => {
            Node::MacroAttribute(name, name_span, arguments, desugar_boxed(inner, source))
        }
        Node::Module(name, mut items) => {
            desugar_list(&mut items.0, source);
            Node::Module(name, items)
        }
        Node::Impl(subject, traits, mut members) => {
            desugar_list(&mut members.0, source);
            Node::Impl(subject, traits, members)
        }
        Node::Trait(name, generics, supertraits, mut members) => {
            desugar_list(&mut members.0, source);
            Node::Trait(name, generics, supertraits, members)
        }
        Node::Lift(subject, continuation) => Node::Lift(
            desugar_boxed(subject, source),
            desugar_boxed(continuation, source),
        ),
        Node::Is(subject, pattern) => Node::Is(desugar_boxed(subject, source), pattern),
        Node::TupleComprehension {
            binder,
            binder_span,
            source: comprehension_source,
            body,
        } => Node::TupleComprehension {
            binder,
            binder_span,
            source: desugar_boxed(comprehension_source, source),
            body: desugar_boxed(body, source),
        },
        // Everything else cannot contain an expression, or the prefilter
        // already ruled it element-free.
        other => other,
    };
    (kind, span)
}

fn descend_if<'src>(branch: NodeIfBranch<'src>, source: &'src str) -> NodeIfBranch<'src> {
    match branch {
        NodeIfBranch::If(if_) => {
            let If {
                condition,
                mut then,
                else_,
            } = *if_;
            let condition = desugar_boxed(condition, source);
            desugar_block(&mut then.0, source);
            let else_ = else_.map(|(inner, span)| (descend_if(inner, source), span));
            NodeIfBranch::If(Box::new(If {
                condition,
                then,
                else_,
            }))
        }
        NodeIfBranch::Else(mut body) => {
            desugar_block(&mut body.0, source);
            NodeIfBranch::Else(body)
        }
    }
}

fn desugar_boxed<'src>(
    node: Box<Spanned<Node<'src>>>,
    source: &'src str,
) -> Box<Spanned<Node<'src>>> {
    Box::new(desugar(*node, source))
}

fn desugar_opt<'src>(
    node: Option<Box<Spanned<Node<'src>>>>,
    source: &'src str,
) -> Option<Box<Spanned<Node<'src>>>> {
    node.map(|node| desugar_boxed(node, source))
}

fn desugar_list<'src>(list: &mut NodeList<'src>, source: &'src str) {
    for item in list.iter_mut() {
        if contains_element(&item.0) {
            take_and_desugar(item, source);
        }
    }
}

fn desugar_block<'src>(body: &mut (NodeList<'src>, Box<Spanned<Node<'src>>>), source: &'src str) {
    desugar_list(&mut body.0, source);
    if contains_element(&body.1.0) {
        take_and_desugar(&mut body.1, source);
    }
}
