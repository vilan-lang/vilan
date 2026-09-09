//! The `css` block desugar (proposal/css-block.md §5).
//!
//! A `css { … }` block parses as a `Node::Css`; this pass — pure Node → Node,
//! run once per parsed tree immediately before `elements::rewrite_items` at
//! each of its call sites — rewrites every block into the `style()` method
//! chain it sugars, building exactly the node shapes a hand-written chain
//! parses to (each link is
//! `MemberAccessor(subject, Call(Accessor(m), generics, args))`), so the
//! lowering IS the chain: identical trees, identical emitted CSS, identical
//! emitted JS. The analyzer, transformer, and interpreter never see a css node,
//! and the emitter is not touched — `Style::rule` stays the one chokepoint
//! (§5.1).
//!
//! The lowering table (§5.2), complete:
//!
//! | Written | Lowers to |
//! |---|---|
//! | `css { … }` | `style()` followed by the items in written order |
//! | `prop: <one hole>;` | `.raw("prop", <the hole's expression>)` |
//! | `prop: <anything else>;` | `.raw("prop", <the value's source slice as a str, each hole through `std::style::piece`>)` |
//! | `.name { … }` | `.name(style() … )` |
//! | `.name(a, b) { … }` | `.name(a, b, style() … )` |
//! | `.name(a, b);` | `.name(a, b)` — a chain link, verbatim (A69) |
//!
//! Name-blind on every row: an undotted item is ALWAYS `.raw`, a dotted one is
//! ALWAYS a method call — with a `{ … }` body it takes the body's chain as its
//! last argument, with a `;` it takes nothing — so the pass never consults
//! `Style`'s method list and a method added to `Style` can never change what
//! existing `css` means.
//!
//! **Spans are cut here, in the first slice, on purpose** (§7.3). Element
//! syntax reached its LSP slice and found that its desugar's wide generated
//! spans had painted `<div` as a function and attribute names as methods, and
//! that a generated `.child` link sharing its hole's exact span tie-broke
//! nondeterministically — repair work in S5 for a decision made in S2. So every
//! generated SCAFFOLDING accessor here takes a ZERO-WIDTH anchor: `.raw` and
//! each condition combinator's method reference, and the `style()` that seeds a
//! nested rule's inner chain. The one seed with a real span is the outer
//! `style()`, which takes the `css` keyword's own span — a diagnostic about
//! the block's own value then underlines the word that asked for one.
//!
//! **The seed is HYGIENIC** (B270): it is a `Node::StdItem("style", "style")`,
//! a scope-independent reference to `std::style::style`, not a bare `style`
//! accessor resolved at the site. A bare accessor made the whole form unusable
//! under `prelude = "std::web"`, where `style` is the ambient MODULE — every
//! block in an application package failed with "`style` is a module, not a
//! value" — and it let any local `style` capture a desugar nobody had written.
//! A block means std's `style()` under the web prelude, under a local
//! `let style = 1;`, under `import std::style::style as s;`, and with no
//! prelude at all; the loader seeds `std::style` off the reference itself, so
//! the form needs no import to work.
//!
//! Property names and value text are stored as SPANS by the parser (a
//! hyphenated or custom property spans tokens carrying no joined text) and
//! sliced from the source here, where it is in scope — the same mechanism the
//! element desugar uses for tag and attribute names.
//!
//! The pass is the identity on css-free trees: a cheap `contains_css`
//! prefilter (riding `for_each_child`, like lift's mark detection) leaves
//! untouched nodes unrebuilt.

use crate::node::{
    BinaryOp, Closure, CssBody, CssDeclaration, CssItem, CssLink, CssNested, CssValuePiece,
    ElementHeadItem, If, Node, NodeIfBranch, NodeList,
};
use crate::span::{Span, Spanned};

/// The keyword's own length, which is also the outer `style()` accessor's
/// span: a `Node::Css` span starts exactly at the `css` token.
const KEYWORD: &str = "css";

/// Rewrite every `css` block in a parsed tree, in place. Called at each
/// `lift::rewrite_items` site, immediately before `elements::rewrite_items` —
/// which covers the entry file, every loaded module, and parsed
/// macro-expansion output.
pub fn rewrite_items<'src>(items: &mut NodeList<'src>, source: &'src str) {
    for item in items.iter_mut() {
        if contains_css(&item.0) {
            take_and_desugar(item, source);
        }
    }
}

fn contains_css(node: &Node<'_>) -> bool {
    if matches!(node, Node::Css(_)) {
        return true;
    }
    let mut found = false;
    node.for_each_child(&mut |child| {
        if !found && contains_css(&child.0) {
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
        Node::Css(body) => {
            // The outer `style()` takes the keyword's own span; every generated
            // accessor below takes a zero-width anchor.
            let head: Span = (span.start..span.start + KEYWORD.len()).into();
            let chain = build_chain(body, head, source);
            // A68: the chain is wrapped in `const`. A block is a compile-time
            // asset BY CONSTRUCTION — `Style::raw` calls `emit`, the
            // compile-time channel — so every block needed the word written in
            // front of it, and `let b = css { padding: 1rem; };` was refused
            // with "`raw` … is compile-time-only; evaluate this call inside a
            // `const` expression". The desugar writes it instead.
            //
            // The wrapper takes the block's OWN span, and `const` forwards to
            // its inner expression in the analyzer, so a written
            // `const css { … }` nests two markers and means exactly what one
            // means: the form stays idempotent, and every block already written
            // with `const` is untouched down to the emitted byte. A hole that
            // reads a runtime binding is refused by const-eval AT THE HOLE,
            // which is the diagnostic that was always wanted here.
            // A70: the chain is wrapped in a `CssScope` too, INSIDE the
            // `const` — the mark that says "these expressions were written
            // inside a block", which is where `std::style::prelude` is
            // ambient. Both wrappers forward to their inner expression, so the
            // tree the analyzer types is still the chain.
            let scoped = (Node::CssScope(Box::new((chain.0, span))), span);
            (Node::Const(Box::new(scoped)), span)
        }
        other => descend((other, span), source),
    }
}

/// `style()` seeded at `head`, then one link per item in WRITTEN ORDER.
fn build_chain<'src>(body: CssBody<'src>, head: Span, source: &'src str) -> Spanned<Node<'src>> {
    let mut chain: Spanned<Node<'src>> = (
        Node::Call(
            Box::new((Node::StdItem("style", "style"), head)),
            None,
            (Vec::new(), head),
        ),
        head,
    );
    for item in body.items {
        let member = match item {
            CssItem::Declaration(declaration) => declaration_link(declaration, source),
            CssItem::Nested(nested) => nested_link(nested, source),
            CssItem::Link(link) => chain_link(link, source),
        };
        chain = attach(chain, member);
    }
    chain
}

/// `prop: value;` → `.raw("prop", value)` — one row of the table, total: there
/// is no CSS property the block cannot express, because `raw` is the model's
/// escape hatch and the block inherits it whole (§5.2).
fn declaration_link<'src>(
    declaration: CssDeclaration<'src>,
    source: &'src str,
) -> Spanned<Node<'src>> {
    let CssDeclaration {
        property,
        value,
        value_span,
        span,
    } = declaration;
    let property_text = &source[property.into_range()];
    let value = build_value(value, value_span, source);
    // Zero-width scaffolding span: the property name is CSS, not a method
    // reference, and the LSP paints it as a property.
    let anchor: Span = (property.start..property.start).into();
    (
        Node::Call(
            Box::new((Node::Accessor("raw"), anchor)),
            None,
            (vec![(Node::String(property_text), property), value], span),
        ),
        span,
    )
}

/// The two value rows of §5.2's table are one rule with two spellings of the
/// argument. A value that is EXACTLY one hole and nothing else passes its
/// expression through untouched, so `gap: {space(4)};` keeps a `Length` and its
/// `:root` line; anything else becomes a `str` — and when it contains holes,
/// the same parenthesized concatenation the lexer builds for an i-string, so
/// `padding: {a} {b};` and `.raw("padding", i"{a} {b}")` are the same tree.
/// Both paths call the same method, and the TYPE SYSTEM decides what the value
/// means.
fn build_value<'src>(
    pieces: Vec<CssValuePiece<'src>>,
    value_span: Span,
    source: &'src str,
) -> Spanned<Node<'src>> {
    if let [CssValuePiece::Hole(..)] = pieces.as_slice() {
        let Some(CssValuePiece::Hole(expression, _)) = pieces.into_iter().next() else {
            unreachable!("just matched a single hole");
        };
        return expression;
    }
    if let [CssValuePiece::Text(text)] = pieces.as_slice() {
        // A hole-free value is its own source slice, read as a string body:
        // exactly the node `.raw("prop", "text")` parses to.
        return (Node::String(&source[text.into_range()]), *text);
    }
    // Mixed: the i-string's own shape — `("" + part + part + …)`, left
    // associated, seeded with the empty string (`lexing::emit_interpolated`)
    // — with every HOLE wrapped in `std::style::piece` (A34), which returns
    // the hole's text and puts its `:root` line on the sheet on the way past.
    //
    // That wrapper is the whole of A34. Without it a mixed value was a plain
    // string concatenation, so `border: 1px solid {Color::gray(500)};` had no
    // correct spelling: `+` refuses a struct outright, and reaching for
    // `.text` emits `var(--gray-500)` with nothing declaring it — the exact
    // hazard the single-hole path exists to close. The single-hole path is
    // untouched: a value that is EXACTLY one hole still passes its expression
    // through, keeps its type, and reaches `Style::raw`, which does the same
    // job for a whole value.
    let mut concatenation: Spanned<Node<'src>> = (Node::String(""), value_span);
    for piece in pieces {
        let part = match piece {
            CssValuePiece::Hole(expression, _) => wrap_piece(expression),
            CssValuePiece::Text(text) => (Node::String(&source[text.into_range()]), text),
        };
        let span: Span = (concatenation.1.start..part.1.end).into();
        concatenation = (
            Node::Binary(BinaryOp::Add, Box::new(concatenation), Box::new(part)),
            span,
        );
    }
    (concatenation.0, value_span)
}

/// One hole of a MIXED value, wrapped in `std::style::piece` (A34): the call
/// returns the hole's text and emits its `:root` line, so a typed style token
/// mid-value carries its token exactly as a whole-value one does.
///
/// The callee is a `StdItem`, so it means std's `piece` whatever the site
/// binds — the same hygiene B270 gave the seed, and for the same reason: this
/// is a call nobody wrote. Its span is the hole's own expression span, which
/// is where a `CssPiece` failure should underline.
fn wrap_piece<'src>(expression: Spanned<Node<'src>>) -> Spanned<Node<'src>> {
    let span = expression.1;
    (
        Node::Call(
            Box::new((Node::StdItem("style", "piece"), span)),
            None,
            (vec![expression], span),
        ),
        span,
    )
}

/// `.name(a, b) { … }` → `.name(a, b, style() … )`: a dotted head lowers to a
/// method call with the block's own chain appended as the FINAL argument.
/// Inner-last is universal across the combinators that exist — `hover`,
/// `focus`, `active`, `disabled`, `first`, `last`, `sm`/`md`/`lg`/`xl` take
/// `(self, inner)`; `within` takes `(self, name, value, inner)`; `children`,
/// `divide` take `(self, inner)`; `attribute` `(self, name, value, inner)`;
/// `pseudo` `(self, name, inner)` — and the rule needs no list of them (§5.3).
fn nested_link<'src>(nested: CssNested<'src>, source: &'src str) -> Spanned<Node<'src>> {
    let CssNested {
        name,
        mut arguments,
        body,
        head,
        span,
    } = nested;
    for argument in arguments.iter_mut() {
        take_and_desugar(argument, source);
    }
    // The inner chain's own `style()` anchors zero-width at the body's `{`:
    // the combinator's name is CSS-side syntax, and a generated accessor
    // sharing it would paint the head as a method reference (§7.3).
    let inner_head: Span = (body.braces.start..body.braces.start).into();
    arguments.push(build_chain(body, inner_head, source));
    // Zero-width scaffolding span, as for `raw`.
    let anchor: Span = (head.start..head.start).into();
    (
        Node::Call(
            Box::new((Node::Accessor(name.0), anchor)),
            None,
            (arguments, span),
        ),
        span,
    )
}

/// `.name(a, b);` → `.name(a, b)` (A69): a chain link is the method call it
/// reads as, spliced at its written position with nothing added and nothing
/// consulted. `.ghost;` and `.ghost();` are the same call — a bare member and
/// a zero-argument call are one thing on a `Style`.
///
/// The accessor takes the NAME's own span, unlike every other generated
/// accessor here. A link is not scaffolding: the author wrote a method call
/// and means one, so hover, go-to-definition and the semantic-token painter
/// should all treat it as the method reference it is. A condition rule's head
/// stays zero-width because there the name is CSS-side syntax that happens to
/// be spelled like a method.
fn chain_link<'src>(link: CssLink<'src>, source: &'src str) -> Spanned<Node<'src>> {
    let CssLink {
        name,
        mut arguments,
        // The two spellings are one call; only the formatter cares which was
        // written.
        parenthesized: _,
        span,
    } = link;
    for argument in arguments.iter_mut() {
        take_and_desugar(argument, source);
    }
    (
        Node::Call(
            Box::new((Node::Accessor(name.0), name.1)),
            None,
            (arguments, span),
        ),
        span,
    )
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
/// same container coverage as `elements::descend`, with one addition: a
/// `Node::Element` is descended into rather than left alone, because this pass
/// runs BEFORE the element desugar and a block can sit in an element's head or
/// a child hole (`<div .styled(const css { … })>`).
fn descend<'src>(node: Spanned<Node<'src>>, source: &'src str) -> Spanned<Node<'src>> {
    let (kind, span) = node;
    let kind = match kind {
        Node::LiftGroup(inner) => Node::LiftGroup(desugar_boxed(inner, source)),
        Node::Element(mut body) => {
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
            Node::Element(body)
        }
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
        Node::StructInitializer(namespace, name, generics, mut fields) => {
            for field in fields.0.iter_mut() {
                if let Some(value) = field.0.1.as_mut() {
                    take_and_desugar(value, source);
                }
            }
            Node::StructInitializer(namespace, name, generics, fields)
        }
        Node::Binary(op, left, right) => Node::Binary(
            op,
            desugar_boxed(left, source),
            desugar_boxed(right, source),
        ),
        Node::Unary(op, inner) => Node::Unary(op, desugar_boxed(inner, source)),
        Node::Reference(mutable, inner) => Node::Reference(mutable, desugar_boxed(inner, source)),
        Node::Dereference(inner) => Node::Dereference(desugar_boxed(inner, source)),
        Node::Spread(inner) => Node::Spread(desugar_boxed(inner, source)),
        Node::TryAssert(inner) => Node::TryAssert(desugar_boxed(inner, source)),
        Node::Await(inner) => Node::Await(desugar_boxed(inner, source)),
        Node::Async(inner) => Node::Async(desugar_boxed(inner, source)),
        Node::FuncReturn(value) => Node::FuncReturn(desugar_opt(value, source)),
        Node::Export(inner) => Node::Export(desugar_boxed(inner, source)),
        Node::Const(inner) => Node::Const(desugar_boxed(inner, source)),
        Node::Derive(names, inner) => Node::Derive(names, desugar_boxed(inner, source)),
        Node::Service(attribute, inner) => Node::Service(attribute, desugar_boxed(inner, source)),
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
        // already ruled it css-free.
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

// Rewrites IN the box the tree already owns rather than unboxing, desugaring
// and boxing the result again: `take_and_desugar` is the same replace-in-place
// the slot walkers use, and reusing the allocation is what keeps this off
// clippy's `boxed_local` — which started firing once M32 narrowed `Node`
// enough for the lint to notice the round trip.
fn desugar_boxed<'src>(
    mut node: Box<Spanned<Node<'src>>>,
    source: &'src str,
) -> Box<Spanned<Node<'src>>> {
    take_and_desugar(&mut node, source);
    node
}

fn desugar_opt<'src>(
    node: Option<Box<Spanned<Node<'src>>>>,
    source: &'src str,
) -> Option<Box<Spanned<Node<'src>>>> {
    node.map(|node| desugar_boxed(node, source))
}

fn desugar_list<'src>(list: &mut NodeList<'src>, source: &'src str) {
    for item in list.iter_mut() {
        if contains_css(&item.0) {
            take_and_desugar(item, source);
        }
    }
}

fn desugar_block<'src>(body: &mut (NodeList<'src>, Box<Spanned<Node<'src>>>), source: &'src str) {
    desugar_list(&mut body.0, source);
    if contains_css(&body.1.0) {
        take_and_desugar(&mut body.1, source);
    }
}

#[cfg(test)]
mod tests {
    use crate::node::{Node, NodeList};
    use crate::parsing;
    use crate::span::Spanned;

    /// The `Debug` of `let probe = <source>;`'s initializer after the desugar —
    /// span-inclusive, so the anchors below are asserted as text rather than
    /// argued about.
    fn lowered(source: &str) -> String {
        let wrapped = format!("let probe = {source};");
        let leaked: &'static str = Box::leak(wrapped.into_boxed_str());
        let (tree, errors) = parsing::parse(leaked);
        assert!(
            errors.is_empty(),
            "{source} did not parse cleanly: {errors:?}"
        );
        let mut items: Spanned<NodeList<'static>> = tree.expect("a tree");
        super::rewrite_items(&mut items.0, leaked);
        let Node::Let(_, _, Some(value), _) = &items.0[0].0 else {
            panic!("expected a `let` with a value");
        };
        format!("{value:?}")
    }

    /// The same source parsed and lowered, beside the CHAIN it sugars, both as
    /// `Debug` trees with every span erased. The lowering IS the chain — the
    /// arc's headline claim (§5.1) — and this is the claim at tree granularity,
    /// where the emitted-bytes gate in `inference::styling` is the same claim at
    /// the other end of the pipeline.
    ///
    /// Two nodes are peeled from the block's side before the comparison, both
    /// of them MARKS on the chain rather than parts of it: A68's `const`
    /// (a block is a compile-time asset, and the desugar writes the word) and
    /// A70's `CssScope` (which expressions were written inside a block, so the
    /// style prelude can be ambient in them). Both forward to their inner
    /// expression in the analyzer, and each has its own pin below; what is
    /// compared here is what they wrap.
    ///
    /// The SEED is normalized for the same reason: B270 made it hygienic, so
    /// the block's is `std::style::style` where a hand-written chain's is
    /// whatever `style` means at the site.
    fn shapes_match(block: &str, chain: &str) -> (String, String) {
        (strip_spans(&peeled(block)), strip_spans(&peeled(chain)))
    }

    /// [`lowered`] with the marks off and the seed read as the accessor it
    /// means.
    fn peeled(source: &str) -> String {
        let wrapped = format!("let probe = {source};");
        let leaked: &'static str = Box::leak(wrapped.into_boxed_str());
        let (tree, errors) = parsing::parse(leaked);
        assert!(
            errors.is_empty(),
            "{source} did not parse cleanly: {errors:?}"
        );
        let mut items: Spanned<NodeList<'static>> = tree.expect("a tree");
        super::rewrite_items(&mut items.0, leaked);
        let Node::Let(_, _, Some(value), _) = &items.0[0].0 else {
            panic!("expected a `let` with a value");
        };
        let mut node: &Spanned<Node<'static>> = value;
        while let Node::Const(inner) | Node::CssScope(inner) = &node.0 {
            node = inner;
        }
        format!("{node:?}")
            .replace("StdItem(\"style\", \"style\")", "Accessor(\"style\")")
            .replace("StdItem(\"ui\", \"view\")", "Accessor(\"view\")")
    }

    /// A `Debug` tree with every span (`Span` renders as `start..end`) replaced
    /// by `_`. The two spellings are written at different offsets in their own
    /// sources, so the SHAPE is what is being compared; the spans have pins of
    /// their own below.
    fn strip_spans(debug: &str) -> String {
        let bytes = debug.as_bytes();
        let mut out = String::with_capacity(debug.len());
        let mut index = 0;
        while index < bytes.len() {
            let digits = bytes[index..]
                .iter()
                .take_while(|byte| byte.is_ascii_digit())
                .count();
            if digits > 0 && debug[index + digits..].starts_with("..") {
                let after = index + digits + 2;
                let tail = bytes[after..]
                    .iter()
                    .take_while(|byte| byte.is_ascii_digit())
                    .count();
                if tail > 0 {
                    out.push('_');
                    index = after + tail;
                    continue;
                }
            }
            out.push(debug[index..].chars().next().expect("a character"));
            index += debug[index..]
                .chars()
                .next()
                .expect("a character")
                .len_utf8();
        }
        out
    }

    #[test]
    fn a_declaration_lowers_to_raw() {
        let (block, chain) = shapes_match(
            "css { display: flex; }",
            r#"style().raw("display", "flex")"#,
        );
        assert_eq!(block, chain);
    }

    #[test]
    fn a_one_hole_value_passes_its_expression_through() {
        // The row that keeps a `Length` a `Length`: exactly one hole and
        // nothing else is the expression itself, never a string.
        let (block, chain) = shapes_match(
            "css { gap: {space(4)}; }",
            r#"style().raw("gap", space(4))"#,
        );
        assert_eq!(block, chain);
    }

    #[test]
    fn a_mixed_value_lowers_to_the_i_string_shape_with_each_hole_through_piece() {
        // Text, hole, text — the same parenthesized concatenation
        // `lexing::emit_interpolated` builds, whitespace included (the space
        // before `+` belongs to the text run, not to the hole), with every
        // HOLE wrapped in `std::style::piece` (A34), which returns the hole's
        // text and puts its `:root` line on the sheet.
        //
        // The chain side spells the wrapper out, so the two sides are the same
        // tree and the claim stays "the lowering IS the chain". `piece` is a
        // hygienic reference in the block, and the normalization below reads
        // it as the name a hand-written chain would import.
        let block = strip_spans(&peeled("css { padding: calc({a} + 2px); }"))
            .replace("StdItem(\"style\", \"piece\")", "Accessor(\"piece\")");
        let chain = strip_spans(&peeled(
            r#"style().raw("padding", "" + "calc(" + piece(a) + " + 2px)")"#,
        ));
        assert_eq!(block, chain);
    }

    #[test]
    fn a_single_hole_value_never_goes_through_piece() {
        // The control A34 rests on: exactly one hole and nothing else still
        // passes its expression through untouched, so the value keeps its TYPE
        // and reaches `Style::raw`, which carries the `:root` line itself.
        let tree = lowered("css { gap: {space(4)}; }");
        assert!(!tree.contains("piece"), "{tree}");
    }

    #[test]
    fn a_nested_rule_lowers_to_a_combinator_with_the_chain_last() {
        let (block, chain) = shapes_match(
            "css { .hover { color: red; } }",
            r#"style().hover(style().raw("color", "red"))"#,
        );
        assert_eq!(block, chain);
    }

    #[test]
    fn a_nested_head_with_arguments_keeps_them_before_the_chain() {
        let (block, chain) = shapes_match(
            r#"css { .within("data-theme", "dark") { color: red; } }"#,
            r#"style().within("data-theme", "dark", style().raw("color", "red"))"#,
        );
        assert_eq!(block, chain);
    }

    #[test]
    fn written_order_is_preserved() {
        // Nothing is reordered, deduped or merged at lowering (§3): the chain
        // is the items in the order they were written, a nested rule in the
        // middle included.
        let (block, chain) = shapes_match(
            "css { color: red; .hover { color: blue; } padding: 1rem; }",
            r#"style().raw("color", "red").hover(style().raw("color", "blue")).raw("padding", "1rem")"#,
        );
        assert_eq!(block, chain);
    }

    #[test]
    fn an_empty_block_is_a_bare_style_call() {
        let (block, chain) = shapes_match("css { }", "style()");
        assert_eq!(block, chain);
    }

    // --- A69: chain links -----------------------------------------------------

    #[test]
    fn a_bare_chain_link_is_the_method_call_it_reads_as() {
        let (block, chain) = shapes_match("css { .ghost(); }", "style().ghost()");
        assert_eq!(block, chain);
    }

    #[test]
    fn a_chain_link_carries_its_arguments_verbatim() {
        let (block, chain) =
            shapes_match(r#"css { .custom(1, "x"); }"#, r#"style().custom(1, "x")"#);
        assert_eq!(block, chain);
    }

    #[test]
    fn a_chain_link_is_emitted_at_its_written_position() {
        // Between declarations, in written order — the lowering reorders
        // nothing, and a link's position is what it means (an opaque method
        // may write any property at all).
        let (block, chain) = shapes_match(
            "css { color: red; .ghost(); padding: 1rem; }",
            r#"style().raw("color", "red").ghost().raw("padding", "1rem")"#,
        );
        assert_eq!(block, chain);
    }

    #[test]
    fn a_bare_member_and_an_empty_call_are_one_link() {
        assert_eq!(
            strip_spans(&peeled("css { .ghost; }")),
            strip_spans(&peeled("css { .ghost(); }"))
        );
    }

    #[test]
    fn a_dotted_item_with_a_body_is_still_a_condition_rule() {
        // The control: A69 splits the dotted half by what FOLLOWS the head, so
        // the condition rule is untouched — its inner chain still rides in as
        // the last argument.
        let (block, chain) = shapes_match(
            "css { .hover { color: red; } }",
            r#"style().hover(style().raw("color", "red"))"#,
        );
        assert_eq!(block, chain);
    }

    #[test]
    fn a_chain_links_accessor_keeps_the_names_own_span() {
        // Unlike every other generated accessor here, which is zero-width
        // scaffolding: a link IS a method call the author wrote, so hover,
        // go-to-definition and the token painter should all see the method.
        // `let probe = css { .ghost(); }` — `ghost` starts at 19.
        let tree = lowered("css { .ghost(); }");
        assert!(tree.contains("(Accessor(\"ghost\"), 19..24)"), "{tree}");
    }

    // --- Spans (§7.3) ---------------------------------------------------------
    // Cut in THIS slice, not repaired in S5. Element syntax reached its LSP
    // slice and found its desugar's wide generated spans had painted `<div` as
    // a function and attribute names as methods, and that a `.child` link
    // sharing its hole's exact span tie-broke nondeterministically. So every
    // generated SCAFFOLDING accessor here is zero-width, and the assertions are
    // on the `Debug` text so they cannot be argued with.

    #[test]
    fn the_outer_style_accessor_spans_the_css_keyword() {
        // The one generated seed with a real span, so a diagnostic about the
        // block's value underlines the word that asked for one. `let probe = `
        // is 12 bytes, so the keyword is 12..15.
        let tree = lowered("css { display: flex; }");
        assert!(
            tree.contains("(StdItem(\"style\", \"style\"), 12..15)"),
            "{tree}"
        );
    }

    // --- The hygienic seed (B270) ---------------------------------------------

    #[test]
    fn the_seed_is_a_scope_independent_std_reference() {
        // Not `Accessor("style")`: the block means `std::style::style`, and no
        // binding at the site — a local `let style`, `std::web`'s ambient
        // `style` MODULE — can be what it reaches. The resolution half is
        // pinned in `inference::styling` and `module_resolution`; this is the
        // TREE half, which is where the bare accessor used to be.
        let tree = lowered("css { display: flex; }");
        assert!(tree.contains("StdItem(\"style\", \"style\")"), "{tree}");
        assert!(!tree.contains("Accessor(\"style\")"), "{tree}");
    }

    #[test]
    fn a_nested_rules_seed_is_the_same_std_reference() {
        // Every seed is hygienic, not just the outer one: a condition rule's
        // inner chain is a `style()` too, and a site binding must not capture it
        // there either.
        let tree = lowered("css { .hover { color: red; } }");
        assert_eq!(
            tree.matches("StdItem(\"style\", \"style\")").count(),
            2,
            "{tree}"
        );
    }

    #[test]
    fn the_raw_accessor_is_zero_width_at_the_property() {
        // `let probe = css { display: flex; }` — `display` starts at 18.
        let tree = lowered("css { display: flex; }");
        assert!(tree.contains("(Accessor(\"raw\"), 18..18)"), "{tree}");
        // …and the property NAME keeps its own real span, which is what a
        // property-position diagnostic and the semantic-token painter need.
        assert!(tree.contains("(String(\"display\"), 18..25)"), "{tree}");
    }

    #[test]
    fn a_combinator_accessor_is_zero_width_at_its_head() {
        // `let probe = css { .hover { color: red; } }` — the `.` is at 18.
        let tree = lowered("css { .hover { color: red; } }");
        assert!(tree.contains("(Accessor(\"hover\"), 18..18)"), "{tree}");
    }

    #[test]
    fn a_nested_rules_inner_style_is_zero_width_at_its_brace() {
        // The inner `style()` anchors on the body's `{` (at 24), NOT on the
        // combinator head — a generated accessor sharing the head would paint
        // `.hover` as a method reference.
        let tree = lowered("css { .hover { color: red; } }");
        assert!(
            tree.contains("(StdItem(\"style\", \"style\"), 25..25)"),
            "{tree}"
        );
    }

    #[test]
    fn a_hole_keeps_its_own_expression_span() {
        // The generated `.raw` link must not shadow the hole's own tokens: the
        // hole's expression carries the span it was written at, and the link's
        // accessor is zero-width elsewhere.
        let tree = lowered("css { gap: {space(4)}; }");
        assert!(tree.contains("(Accessor(\"space\"), 24..29)"), "{tree}");
    }

    #[test]
    fn the_pass_is_the_identity_on_a_css_free_tree() {
        let plain = r#"style().raw("display", "flex")"#;
        assert_eq!(lowered(plain), lowered(plain));
        assert!(!lowered(plain).contains("Css"), "no css node survives");
    }

    #[test]
    fn a_block_inside_an_element_hole_is_lowered() {
        // The pass runs BEFORE the element desugar, so it descends into an
        // element's head items and children itself — otherwise a block written
        // inside markup would reach the analyzer as a `Node::Css`.
        let tree = lowered("<div .styled(const css { display: flex; }) />");
        assert!(
            !tree.contains("Css("),
            "a block inside markup survived: {tree}"
        );
        assert!(tree.contains("Accessor(\"raw\")"), "{tree}");
    }
}
