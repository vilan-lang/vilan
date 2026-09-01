use crate::span::{Span, Spanned};

pub type GenericParameters<'src> = Spanned<Vec<GenericParameter<'src>>>;

#[derive(Debug)]
pub struct GenericParameter<'src> {
    pub name: &'src str,
    /// The span of the parameter's name (for go-to-definition on a use of it).
    pub name_span: Span,
    // Declared with the `type` keyword (a binder, e.g. `impl Foo<type T>`).
    pub is_type: bool,
    // Trait bounds: `T: A + B` collects `[A, B]`.
    pub bounds: Vec<Spanned<Node<'src>>>,
    // A tuple bound: `T: (2..)` / `(..10)` / `(..: Display)` — the parameter is a
    // tuple of the given arity, optionally with a per-element trait bound. Mutually
    // exclusive with `bounds` (a tuple bound replaces the trait-bound list).
    pub tuple_bound: Option<TupleBound<'src>>,
    // A default, e.g. the `Self` in `<B = Self>`.
    pub default: Option<Box<Spanned<Node<'src>>>>,
}

// A tuple-arity bound on a generic parameter (`T: (lo..hi : Element)`). Either
// endpoint may be omitted (`(..)`, `(2..)`, `(..10)`); `element` is the optional
// per-element trait bound (`(..: Display)`).
#[derive(Debug)]
pub struct TupleBound<'src> {
    pub lo: Option<u32>,
    pub hi: Option<u32>,
    pub element: Option<Box<Spanned<Node<'src>>>>,
    pub span: Span,
}

pub type GenericArguments<'src> = Spanned<Vec<Spanned<Node<'src>>>>;

// How an `external` function is bound to the host (JS): a `[extern(..)]`
// attribute selects the form. The receiver of a method/property is the
// function's first parameter.
#[derive(Clone, Debug)]
pub enum ExternBinding<'src> {
    // `[extern("node:http", "createServer")]` — import `symbol` from `module`
    // (or, with no module, a global/verbatim symbol like `"console.log"`) and
    // call it: `symbol(args)`.
    Function {
        module: Option<&'src str>,
        symbol: &'src str,
    },
    // `[extern(method)]` / `[extern(method, "setHeader")]` — `receiver.symbol(rest)`
    // (the JS name defaults to the function's own name).
    Method {
        symbol: Option<&'src str>,
    },
    // `[extern(get, "statusCode")]` — `receiver.symbol` (a property read).
    Get {
        symbol: &'src str,
    },
    // `[extern(set, "statusCode")]` — `receiver.symbol = value` (a property write).
    Set {
        symbol: &'src str,
    },
    // `[extern(new, "TextDecoder")]` — `new symbol(args)`: construct a host class
    // instance (host constructors reject a plain call). With a module
    // (`[extern(new, "node:sqlite", "DatabaseSync")]`), the class is imported
    // first, like `Function`'s module form.
    New {
        module: Option<&'src str>,
        symbol: &'src str,
    },
}

#[derive(Debug)]
pub struct Func<'src> {
    pub name: Spanned<&'src str>,
    // Declared with the `async` keyword. For an `external` (a leaf with no body)
    // this is the only signal that it is async; for an ordinary function it is
    // usually inferred instead, but `async fun` forces it.
    pub is_async: bool,
    // Declared with the `external` keyword: an intrinsic with no Vilan body,
    // implemented by the runtime/compiler (e.g. `external fun print(..);`).
    pub external: bool,
    // Declared `[deprecated("use …")]`: every use of this function in code
    // outside std warns, non-fatally, carrying this replacement steer verbatim
    // after `` `{name}` is deprecated; `` (proposal/deprecation.md §1–§2). By
    // convention the steer reads `use …`. Honored wherever the attribute
    // appears — std and user code alike.
    pub deprecated: Option<&'src str>,
    // A `[extern(..)]` host binding, lowering this external to a JS import/call,
    // method, or property access. `None` for a plain `external` (compiler
    // intrinsic) or an ordinary function.
    pub extern_binding: Option<ExternBinding<'src>>,
    // Declared `[extern(…, retains)]` (`lifetimes.md` §6.4, RULED 2026-08-28):
    // the host KEEPS what it is handed and may read it after the call returns.
    // An extern loan is call-bounded unless this says otherwise, so an argument
    // to a retaining extern keeps its liveness to the binding's whole scope —
    // the conservative envelope. Meaningless (and inert) without an extern
    // binding.
    pub extern_retains: bool,
    // Declared `[must_use]`: dropping a call's result (a bare statement that
    // discards it) is a warning.
    pub must_use: bool,
    // Declared `[platform("…", …)]` — a platform FENCE: the function's
    // inferred requirement is checked against these patterns on every
    // compile (platform-coloring.md §3.7). Empty = no fence.
    pub platform_fence: Vec<Spanned<&'src str>>,
    // Declared `[rpc]`: callable over the wire as part of a service's surface.
    // Its parameters and return must be Wire types — checked by the analyzer
    // (`proposal/transport-rpc.md` §4.2).
    pub rpc: bool,
    // Declared `[trait_only]` (on a trait's method declaration): reachable only
    // through a trait bound, never on a concrete type's own surface
    // (`proposal/transport-rpc.md` §3.2).
    pub trait_only: bool,
    // Declared `[doc(hidden)]`: fully callable, but omitted from editor
    // completion (a tooling marker — no resolution change).
    pub doc_hidden: bool,
    pub generic_parameters: Option<GenericParameters<'src>>,
    pub parameters: Spanned<Vec<Parameter<'src>>>,
    pub return_type: Option<Box<Spanned<Node<'src>>>>,
    // The `borrows <param>` clause on a view-returning function
    // (`fun slot(&mut self): &mut i32 borrows self`): the returned view is a
    // projection of that parameter, so it may escape (rule 3's sanctioned case).
    pub borrows: Option<&'src str>,
    // `None` for a function signature without a body: a required trait method
    // declaration (`fun default(): Self;`) or an `external` intrinsic.
    pub body: Option<Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)>>,
}

/// A parsed parameter: binder, optional declared type, how it receives its
/// argument (rule 3 conventions), binder mutability, spread-ness, and the
/// binder's span (for go-to-definition / hover in the language server).
#[derive(Debug)]
pub struct Parameter<'src> {
    /// The binder: a plain name (`x`) or a tuple destructure (`(a, b)`).
    pub pattern: Pattern<'src>,
    pub declared_type: Option<Box<Spanned<Node<'src>>>>,
    pub convention: Convention,
    /// `mut x` — the body may rebind and field-write its copy
    /// (proposal/mut-parameters.md). Exclusive with `own`/`&`/`&mut`,
    /// never part of the signature.
    pub mutable: bool,
    /// `...items: T` — a SPREAD parameter (proposal/variadic-generics.md §S):
    /// the call site writes the pack's elements out flat and they are collected
    /// into this one tuple argument. `fun f(...items: T) {b}` is
    /// `fun f(items: T) {b}` with `f(a, b)` meaning `f((a, b))`, so the callee
    /// side is an ORDINARY tuple parameter — `...` is a call convention. Last
    /// parameter only, at most one, must declare its type, plain name binder,
    /// no rule-3 convention. Unlike `mut`, it IS part of the signature.
    pub spread: bool,
    pub span: Span,
}

/// How a parameter receives its argument (rule 3). `Bare` is the default (a
/// readonly view, once the default flip lands); `Ref` / `RefMut` are `&` / `&mut`
/// views. `Own` (owned value) is added with its keyword later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Convention {
    Bare,
    Own,
    Ref,
    RefMut,
}

#[derive(Debug)]
pub struct Closure<'src> {
    pub parameters: Spanned<Vec<Parameter<'src>>>,
    pub return_type: Option<Box<Spanned<Node<'src>>>>,
    pub return_value: Box<Spanned<Node<'src>>>,
}

/// A `css { … }` block's payload (proposal/css-block.md §4.3): the items, in
/// written order. Desugared to a `style()` method chain before analysis
/// (`css::rewrite_items`) — the analyzer, transformer, and interpreter never
/// see this node.
#[derive(Debug)]
pub struct CssBody<'src> {
    pub items: Vec<CssItem<'src>>,
    /// The `{ … }` span, so the desugar can anchor the generated `style()` on
    /// the block itself and the formatter can find the body's own extent.
    pub braces: Span,
}

/// One item of a `css` block. The dot is the whole disambiguator (§3):
/// undotted is a declaration, dotted is a condition combinator — so the
/// grammar never consults `Style`'s method list, and adding a method to
/// `Style` can never change what existing `css` means.
#[derive(Debug)]
pub enum CssItem<'src> {
    Declaration(CssDeclaration<'src>),
    Nested(CssNested<'src>),
}

impl CssItem<'_> {
    /// The item's own span — the whole `property: value;` or the whole
    /// `.name(..) { … }`. Carried per item from the first commit (§7.3) so the
    /// LSP slice has the anchor it needs without reshaping the tree.
    pub fn span(&self) -> Span {
        match self {
            CssItem::Declaration(declaration) => declaration.span,
            CssItem::Nested(nested) => nested.span,
        }
    }
}

/// `property: value;` — one declaration, lowering to exactly one
/// `.raw(property, value)` call (§5.2).
#[derive(Debug)]
pub struct CssDeclaration<'src> {
    /// The property name's SPAN, not a slice: a hyphenated or custom property
    /// (`flex-direction`, `--color-ink`) spans several tokens carrying no
    /// joined text, and the parser has no source access — the desugar slices
    /// it where the source is in scope, exactly as an element's tag name is.
    pub property: Span,
    pub value: Vec<CssValuePiece<'src>>,
    /// The value's whole extent, `:` exclusive and `;` exclusive. The slice
    /// the mixed-value row of §5.2's table renders, and the anchor a
    /// wrong-typed value reports at.
    pub value_span: Span,
    /// The declaration's own span, `;` inclusive.
    pub span: Span,
}

/// One piece of a declaration's value: a `{expr}` hole, or a run of source
/// text between holes. A value is a TOKEN RUN, not a typed grammar — typed
/// values arrive through holes, which is where the type system already lives
/// (§10).
#[derive(Debug)]
pub enum CssValuePiece<'src> {
    /// `{expression}` — the hole's expression, and the span of the whole
    /// `{…}` including its braces (what a reprint has to reproduce).
    Hole(Spanned<Node<'src>>, Span),
    /// A run of value text, verbatim from source.
    Text(Span),
}

/// `.name { … }` / `.name(a, b) { … }` — a condition combinator, lowering to
/// `.name(a, b, style() … )`: the block's own chain appended as the final
/// argument (§5.3).
#[derive(Debug)]
pub struct CssNested<'src> {
    pub name: Spanned<&'src str>,
    /// The head's arguments, ordinary vilan expressions; the inner chain is
    /// appended after them at desugar.
    pub arguments: Vec<Spanned<Node<'src>>>,
    pub body: CssBody<'src>,
    /// The head's span — `.name` or `.name(a, b)` — which is where a
    /// misnesting reports.
    pub head: Span,
    pub span: Span,
}

/// An element expression's payload (proposal/element-syntax.md §3): the tag,
/// the head items, and the children. Desugared to a `view("tag")` method
/// chain before analysis (`elements::rewrite_items`) — the analyzer,
/// transformer, and interpreter never see this node.
#[derive(Debug)]
pub struct ElementBody<'src> {
    /// The tag name's span (`div` in `<div …>`). A SPAN, not a slice: keyword
    /// tags (`<use>`) and hyphenated custom elements (`<my-widget>`) span
    /// several tokens, and the parser has no source access — the desugar pass
    /// slices the text where the source is in scope.
    pub tag: Span,
    pub head: Vec<ElementHeadItem<'src>>,
    pub children: Vec<ElementChild<'src>>,
    /// Whether the element was written self-closing (`<div />`). `<div></div>`
    /// parses to the same empty children but different TOKENS, and the
    /// formatter's re-lex net compares tokens — a reprint must keep the form.
    pub self_closing: bool,
    /// The closing tag name's span (`div` in `</div>`), when the element has
    /// one — the language server's matching-tag features read it. `None` for a
    /// self-closing element.
    pub close_tag: Option<Span>,
    /// Every angle-bracket span the element wrote, in source order: the opening
    /// `<`, the `>` or `/>` that closes the head, and — when the element has a
    /// close tag — its `</` and its `>`.
    ///
    /// Recorded because the editor's two highlight sources have to agree about
    /// this punctuation and only the parser knows where it is (E115). A
    /// TextMate rule is matched one line at a time, so a head whose attributes
    /// span lines puts its `>` on a line with no `<tag` on it, out of the
    /// rule's reach — the parser has no such limit, and the semantic-token pass
    /// paints from these spans whatever shape the head was written in.
    pub punctuation: Vec<Span>,
}

/// One child of an element. The distinction is TOKEN-carrying, not semantic —
/// both lower to `.child(…)` — but a reprint must know whether braces were
/// written: `{"x"}` and `"x"` parse to the same inner node and differ in
/// tokens, and the formatter's re-lex net compares tokens.
#[derive(Debug)]
pub enum ElementChild<'src> {
    /// `{expression}` — a braced hole.
    Hole(Spanned<Node<'src>>),
    /// A bare child: a nested element, a quoted string, or an i-string group.
    Bare(Spanned<Node<'src>>),
}

impl<'src> ElementChild<'src> {
    /// The child's expression, whichever form carried it.
    pub fn node(&self) -> &Spanned<Node<'src>> {
        match self {
            ElementChild::Hole(node) | ElementChild::Bare(node) => node,
        }
    }

    /// The child's expression, owned.
    pub fn into_node(self) -> Spanned<Node<'src>> {
        match self {
            ElementChild::Hole(node) | ElementChild::Bare(node) => node,
        }
    }

    /// The child's expression, mutably.
    pub fn node_mut(&mut self) -> &mut Spanned<Node<'src>> {
        match self {
            ElementChild::Hole(node) | ElementChild::Bare(node) => node,
        }
    }
}

/// One item in an element's head (proposal/element-syntax.md §2): undotted
/// names are attributes, a leading dot is the builder chain verbatim, `on:` is
/// the event form.
#[derive(Debug)]
pub enum ElementHeadItem<'src> {
    /// `.m(args)` — a chain link, spliced verbatim: the `parse_member_call`
    /// node (`Call(Accessor(m), generics, args)`, or a bare `Accessor`).
    Chain(Spanned<Node<'src>>),
    /// `on:evt(handler)` — the event name and the handler expression. The
    /// desugar dispatches `.on` vs `.on_event` on a literal handler's arity.
    Event(Spanned<&'src str>, Box<Spanned<Node<'src>>>),
    /// `name(value)` / bare `name` — the name's span (sliced at desugar, like
    /// the tag) and the optional value; a bare name is a boolean attribute.
    Attribute(Span, Option<Spanned<Node<'src>>>),
}

#[derive(Debug)]
pub struct If<'src> {
    pub condition: Box<Spanned<Node<'src>>>,
    pub then: Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)>,
    pub else_: Option<Spanned<NodeIfBranch<'src>>>,
}

#[derive(Debug)]
pub enum NodeIfBranch<'src> {
    If(Box<If<'src>>),
    Else(Spanned<(NodeList<'src>, Box<Spanned<Node<'src>>>)>),
}

#[derive(Debug)]
pub enum ImportBranch<'src> {
    // A path segment: its name, the span of that name, and an optional `::`
    // continuation. The span drives go-to-definition / hover on imports.
    Path(&'src str, Span, Option<Box<Self>>),
    Set(Vec<Self>),
}

pub type NodeList<'src> = Vec<Spanned<Node<'src>>>;

#[derive(Debug)]
pub enum Node<'src> {
    Accessor(&'src str),
    AccessorWithGenerics(&'src str, GenericArguments<'src>),
    // `async <block-or-expr>` — runs the body as a promise, evaluating to a
    // `Promise<T>` immediately (non-blocking). Lowers to an invoked async arrow.
    Async(Box<Spanned<Self>>),
    // `await <expr>` — suspends until the promise resolves, yielding `T`. Forces
    // its enclosing function to be async.
    Await(Box<Spanned<Self>>),
    // A `type X` generic binder appearing inside a type — the impl subject
    // pattern (`impl Option<(type T, type U)>`), including a bare blanket
    // (`impl type T`). The optional bounds are `T: A + B`.
    TypeBinder(&'src str, Vec<Spanned<Self>>),
    // `x = v` or a compound assignment like `x += v` (the operator is the
    // binary op the assignment applies, e.g. `Add` for `+=`). The target is an
    // lvalue: a local (`Accessor`) or a field place (`MemberAccessor`, e.g.
    // `self.n = v`).
    Assign(Box<Spanned<Self>>, Option<BinaryOp>, Box<Spanned<Self>>),
    Binary(BinaryOp, Box<Spanned<Self>>, Box<Spanned<Self>>),
    Block(Spanned<(NodeList<'src>, Box<Spanned<Self>>)>),
    Bool(bool),
    Call(
        Box<Spanned<Self>>,
        Option<GenericArguments<'src>>,
        Spanned<NodeList<'src>>,
    ),
    Closure(Closure<'src>),
    ClosureType(
        Spanned<Vec<(Option<&'src str>, Box<Spanned<Node<'src>>>)>>,
        Option<Box<Spanned<Node<'src>>>>,
    ),
    // `async || T` — a closure type whose calls suspend: calls through a
    // value of this type are implicitly awaited, like direct calls to an
    // async function (backlog J2). Wraps the closure type it marks.
    AsyncType(Box<Spanned<Node<'src>>>),
    // `sync |A| B` — the synchronous-contract marker on a closure type
    // (proposal/async-polymorphism.md A.2): the callback's completion is part
    // of the declaring function's synchronous protocol, so an async closure
    // argument is refused rather than adapted. `sync` is a CONTEXTUAL
    // keyword (lexes as an identifier; only means the contract directly
    // before a closure type). Wraps the closure type it marks.
    SyncType(Box<Spanned<Node<'src>>>),
    // `(|| void) context owner_scope` / `context (a, b)` — a closure type
    // carrying a context requirement (proposal/ambient-owner.md §5): the
    // closure defers those contexts' bindings to its CALL sites instead of
    // capturing at creation. The names (with spans) name context VALUES;
    // written order is the hidden-argument order.
    TypeWithContexts(Box<Spanned<Self>>, Vec<(&'src str, Span)>),
    // A mapped tuple type `(U in T: F<U>)`: bind each element of the source tuple
    // type `T` as `U`, and the corresponding result slot is the template `F<U>`.
    MappedType {
        binder: &'src str,
        binder_span: Span,
        source: Box<Spanned<Node<'src>>>,
        template: Box<Spanned<Node<'src>>>,
    },
    // A tuple comprehension `(x in xs = e)`: build a tuple by evaluating the body
    // `e` for each element of the source tuple `xs`, with the element bound as `x`.
    TupleComprehension {
        binder: &'src str,
        binder_span: Span,
        source: Box<Spanned<Node<'src>>>,
        body: Box<Spanned<Node<'src>>>,
    },
    // A `css { … }` block (proposal/css-block.md) — CSS-shaped sugar over the
    // `std::style` chain. Exists only between parse and the pre-analysis
    // desugar (`css::rewrite_items`); the formatter prints it from source
    // (S2's passthrough; S3 brings the canonical printer).
    Css(CssBody<'src>),
    // An element expression `<div …> … </div>` (proposal/element-syntax.md) —
    // markup sugar over the `std::ui` view chain. Exists only between parse
    // and the pre-analysis desugar (`elements::rewrite_items`); the formatter
    // prints it from source.
    Element(ElementBody<'src>),
    // An enum declaration: name, generics, the `resource` flag (the
    // owned-resource modifier, destruction.md §3 — SURFACE ONLY, carried but
    // not yet classified on), and the variants — each a name, the types of its
    // optional data, and an optional explicit discriminant (`Less = -1`).
    Enum(
        Spanned<&'src str>,
        Option<GenericParameters<'src>>,
        bool,
        Spanned<Vec<Spanned<EnumVariant<'src>>>>,
    ),
    Error,
    // A loop: `for { .. }` (infinite, condition `None`) or `for cond { .. }`
    // (while).
    For(
        Option<Box<Spanned<Self>>>,
        Spanned<(NodeList<'src>, Box<Spanned<Self>>)>,
    ),
    // `for item in iterable { .. }` — the binding name, the iterable, the body.
    ForIn(
        &'src str,
        Box<Spanned<Self>>,
        Spanned<(NodeList<'src>, Box<Spanned<Self>>)>,
    ),
    Func(Func<'src>),
    // `ret <expr>` / bare `ret` (an early return of void).
    FuncReturn(Option<Box<Spanned<Self>>>),
    // `expr!` — assert-or-return (proposal/try-and-lift.md): the good half of a
    // `Try` value, or an early return of the bad half from the nearest
    // enclosing function.
    TryAssert(Box<Spanned<Self>>),
    // `a?.b.c` — a lifted member chain (proposal/try-and-lift.md §3): the
    // subject, and the continuation built over `LiftBinder` (the segment from
    // this `?` to the next `?`/`!`/chain end). Maps, or flattens when the
    // continuation yields the subject's own container.
    Lift(Box<Spanned<Self>>, Box<Spanned<Self>>),
    // The continuation's hole: the lifted element inside a `Lift` chain.
    LiftBinder,
    // A bare postfix `?` (one NOT followed by `.`) — an expression-lifting mark
    // (proposal/expression-lifting.md). Exists only between parse and the
    // region rewrite at the analyzer's entry, which linearizes every marked
    // slot-root expression into a `LiftRegion`; the walk never sees it.
    Lifted(Box<Spanned<Self>>),
    // A RECORDED parenthesized expression — the parens were written, and the
    // node says so. The compiler's parse records exactly one case: a group
    // containing a `Lifted` mark, because parens delimit a lift region (§6.2);
    // a paren without a mark dissolves as always. The region rewrite then seals
    // the inner expression as its own region root and the wrapper vanishes, and
    // anything that does see one treats it as transparent (the analyzer's walk
    // forwards straight to the inner expression).
    //
    // The FORMATTER's parse (`parsing::parse_preserving_groups`) records every
    // group instead, so `vilan fmt` can reprint the parentheses a user wrote
    // rather than bailing on the file. That mode never reaches the analyzer.
    LiftGroup(Box<Spanned<Self>>),
    // A sealed lift region (rewrite output): the ordered evaluation steps and
    // the residual body skeleton. A step is (expression, is_split): an `Eval`
    // step (false) hoists effectful pre-`?` material so source evaluation
    // order holds; a `Split` step (true) is a `?` receiver — bad
    // short-circuits the region with the bad half as-is. The skeleton
    // references step results through `LiftHole(step_index)`.
    LiftRegion(Vec<(Spanned<Self>, bool)>, Box<Spanned<Self>>),
    // A hole in a region's body skeleton: the result of step `n` — the element
    // for a split step, the hoisted value for an eval step.
    LiftHole(usize),
    If(NodeIfBranch<'src>),
    // `subject is pattern` — a pattern test that yields a `bool` and binds the
    // pattern's captures into the surrounding scope.
    Is(Box<Spanned<Self>>, Box<Spanned<Pattern<'src>>>),
    // `jump break` / `jump continue` — the target keyword that follows `jump`.
    Jump(&'src str),
    Impl(
        // The subject type pattern. May contain `type X` binders anywhere
        // (`impl Option<(type T, type U)>`) or be a bare binder (`impl type T`);
        // those binders are the impl's generic parameters.
        Box<Spanned<Self>>,
        // The traits being implemented: the `A`, `B` in `impl Subject with A + B`.
        Vec<Spanned<Self>>,
        Spanned<NodeList<'src>>,
    ),
    Import(ImportBranch<'src>),
    // `export <item>` — re-export an import or expose a local declaration.
    Export(Box<Spanned<Self>>),
    // `macro fun name(..) { .. }` — a macro definition (macro-engine.md §3).
    // Its body is HERMETIC: never walked in the program world, compiled in the
    // per-file macro world instead (its imports resolve against `macro_std`
    // only), and executed by the expansion interpreter.
    MacroFun(Func<'src>),
    // `[name(args)] <item>` — a user macro attribute on a struct/enum/function:
    // the macro's name (with its span), the argument SPANS (their source text
    // is what `Arguments` carries — arguments are syntax), and the annotated
    // item. Expanded before analysis; the item itself is walked normally.
    MacroAttribute(&'src str, Span, Vec<Span>, Box<Spanned<Self>>),
    // `macro name(args)` — a macro invocation (macro-engine.md §2). At a
    // module's top level it is an ITEM invocation (the returned Source parses
    // as items, appended to the module); anywhere else it is an EXPRESSION
    // invocation (the returned Source parses as an expression and splices in
    // place). The name (with its span) and the argument SPANS.
    MacroInvocation(&'src str, Span, Vec<Span>),
    // `macro { .. }` — an anonymous, immediately-expanded macro (macro-engine.md
    // Phase 4): the body runs at expansion time (hermetic, like a `macro fun`
    // body) and its returned `Source` splices at this position — as items at a
    // module's top level, as one expression anywhere else. The body shape is a
    // function body; the world compiles it as a synthetic zero-argument
    // `fun __macro_block_<n>(): Source`.
    MacroBlock(Spanned<(NodeList<'src>, Box<Spanned<Self>>)>),
    // `[derive(A, B)] <struct|enum>` — the derive trait names and the item they
    // annotate. Transparent to analysis (the inner item is walked normally); a
    // pre-analysis pass generates the trait impls from the item's fields.
    Derive(Vec<(&'src str, Span)>, Box<Spanned<Self>>),
    // `[service(Client)] struct …` — a per-connection service struct
    // (`proposal/transport-rpc.md` §4.2). Transparent to analysis; a
    // pre-analysis pass generates its dispatcher, its client sibling (named by
    // the argument, defaulting to `<Struct>Client`), and the contract hash from
    // the struct's `[rpc]` impl methods and `[expose]`d fields.
    Service(Option<&'src str>, Box<Spanned<Self>>),
    // `let`/`mut` binding: name, type annotation, value, mutability.
    Let(
        Spanned<&'src str>,
        Option<Box<Spanned<Self>>>,
        Option<Box<Spanned<Self>>>,
        bool,
    ),
    // `let`/`mut` binding with a destructuring pattern: `let (a, b) = pair`. The
    // pattern is irrefutable (a tuple of names/sub-patterns); the rest mirrors
    // `Let` (type annotation, value, mutability).
    LetDestructure(
        Spanned<Pattern<'src>>,
        Option<Box<Spanned<Self>>>,
        Option<Box<Spanned<Self>>>,
        bool,
    ),
    List(NodeList<'src>),
    // `[value; n]` — a fixed-length array literal: `value` copied into each of
    // `n` slots (value semantics — independent copies). Value and length exprs.
    Repeat(Box<Spanned<Self>>, Box<Spanned<Self>>),
    // `[T; n]` — a fixed-length array TYPE (type position only): element-type
    // node and length expr (an integer literal in v1).
    ArrayType(Box<Spanned<Self>>, Box<Spanned<Self>>),
    // A match expression: subject and legs of `patterns (if guard)? => body`.
    Match(Box<Spanned<Self>>, Spanned<Vec<MatchLeg<'src>>>),
    MemberAccessor(Box<Spanned<Self>>, Box<Spanned<Self>>),
    // `subject[index]` — a subscript into a `List` (element access / assignment,
    // and `&mut list[i]` element views). Subject and index expressions.
    Index(Box<Spanned<Self>>, Box<Spanned<Self>>),
    Module(&'src str, Spanned<NodeList<'src>>),
    Null,
    // The whole part, an optional fractional part, and an optional type suffix.
    Number(&'src str, Option<&'src str>, Option<&'src str>),
    // `subject::member` — one step of a `::` path: the namespace to look
    // `member` up in, and the name. Paths of any depth nest to the left
    // (`a::b::C` is `StaticAccessor(StaticAccessor(a, "b"), "C")`).
    //
    // The generic arguments are the ones written directly ON `member`, which
    // only TYPE position offers: `std::reactive::SignalCell<i32>` names a
    // parameterized type, while in expression position a `<...>` belongs to the
    // CALL that follows (`math::min<i32>(a, b)` is a `Call`'s generics, folded
    // by `parse_call`), so an expression's path always carries `None`.
    StaticAccessor(
        Box<Spanned<Self>>,
        &'src str,
        Option<GenericArguments<'src>>,
    ),
    String(&'src str),
    // A triple-quoted string's raw inner text; trimmed to its content by
    // `util::trim_multiline_string` (validated in the analyzer, trimmed in the
    // transformer).
    MultilineString(&'src str),
    // `const expr` — evaluate at compile time (proposal/const-eval.md). The
    // analyzer marks the inner expression and FORWARDS to it (no wrapper
    // entity), so downstream passes see a plain subtree.
    Const(Box<Spanned<Node<'src>>>),
    // A struct declaration. The first `bool` marks an `external` (intrinsic)
    // struct; the second marks a `resource` — the owned-resource declaration
    // modifier (destruction.md §3), SURFACE ONLY for now: parsed, carried, and
    // formatted, with no classification or affine checking yet. In source the
    // modifiers read `resource external struct`; the node keeps `external` in
    // its original slot (so existing reads are undisturbed) and appends
    // `resource` after it. The body is `Some(fields)` for `{ .. }` and `None`
    // for a bodyless `;` declaration (only valid when `external`).
    Struct(
        Spanned<&'src str>,
        Option<GenericParameters<'src>>,
        bool,
        bool,
        Option<Spanned<Vec<Spanned<StructField<'src>>>>>,
    ),
    StructInitializer(
        &'src str,
        Option<GenericArguments<'src>>,
        Spanned<Vec<Spanned<StructInitializerField<'src>>>>,
    ),
    Trait(
        Spanned<&'src str>,
        Option<GenericParameters<'src>>,
        // Supertraits: the `A`, `B` in `trait T with A + B`.
        Vec<Spanned<Self>>,
        Spanned<NodeList<'src>>,
    ),
    Tuple(NodeList<'src>),
    // `..e` — a tuple-value SPREAD element (proposal/variadic-generics.md §T):
    // inside a tuple construction it contributes the ELEMENTS of `e`'s tuple
    // type rather than `e` itself, so the construction's type is the
    // concatenation. Written only where an element begins — a tuple
    // construction's element or a call argument (which a spread parameter
    // collects into a tuple construction) — so a `..` after an expression is
    // still the member-access dots it has always been.
    Spread(Box<Spanned<Self>>),
    // A prefix operator: `!x` or `-x`.
    Unary(char, Box<Spanned<Self>>),
    // `&x` / `&mut x` — take a (readonly / writable) view of a place. The bool is
    // whether the view is writable (`&mut`).
    Reference(bool, Box<Spanned<Self>>),
    // `*v` — read or write through a view.
    Dereference(Box<Spanned<Self>>),
    // `use Namespace::{ a, b };` — destructures items out of a namespace
    // (a module or an enum) into the current scope.
    Use(ImportBranch<'src>),
    Void,
}

impl<'src> Node<'src> {
    /// Whether this subtree contains a bare-`?` expression-lifting mark
    /// (`Node::Lifted`) anywhere. The parser uses it to decide whether a
    /// parenthesized expression must be recorded as a region-delimiting
    /// `LiftGroup`; the region rewrite uses it to skip unmarked trees.
    pub fn contains_lift_mark(&self) -> bool {
        if matches!(self, Node::Lifted(_)) {
            return true;
        }
        let mut found = false;
        self.for_each_child(&mut |child| {
            if !found && child.0.contains_lift_mark() {
                found = true;
            }
        });
        found
    }

    /// Visits every direct child node. Whole-tree scans that must see nodes at
    /// any nesting depth (`collect_module_refs` finding a block-scoped `import`
    /// inside a closure, the platform sniffer) recurse with this. The match is
    /// deliberately exhaustive with no catch-all: adding a `Node` variant must
    /// extend it or compilation fails here — a container variant silently
    /// missing from the scan is exactly the bug this prevents.
    pub fn for_each_child<'a>(&'a self, visit: &mut dyn FnMut(&'a Spanned<Node<'src>>)) {
        fn visit_generic_parameters<'a, 'src>(
            parameters: &'a Option<GenericParameters<'src>>,
            visit: &mut dyn FnMut(&'a Spanned<Node<'src>>),
        ) {
            for parameter in parameters.iter().flat_map(|parameters| &parameters.0) {
                for bound in &parameter.bounds {
                    visit(bound);
                }
                if let Some(element) = parameter
                    .tuple_bound
                    .as_ref()
                    .and_then(|bound| bound.element.as_deref())
                {
                    visit(element);
                }
                if let Some(default) = parameter.default.as_deref() {
                    visit(default);
                }
            }
        }
        fn visit_pattern<'a, 'src>(
            pattern: &'a Pattern<'src>,
            visit: &mut dyn FnMut(&'a Spanned<Node<'src>>),
        ) {
            match pattern {
                Pattern::Wildcard | Pattern::Binding(..) | Pattern::Variant(_, None) => {}
                Pattern::Variant(_, Some(payload)) => {
                    for (sub, _) in payload {
                        visit_pattern(sub, visit);
                    }
                }
                Pattern::Tuple(elements) | Pattern::Array(elements) => {
                    for (sub, _) in elements {
                        visit_pattern(sub, visit);
                    }
                }
                Pattern::Literal(literal) => visit(literal),
            }
        }
        fn visit_parameters<'a, 'src>(
            parameters: &'a Spanned<Vec<Parameter<'src>>>,
            visit: &mut dyn FnMut(&'a Spanned<Node<'src>>),
        ) {
            for parameter in &parameters.0 {
                visit_pattern(&parameter.pattern, visit);
                if let Some(type_) = parameter.declared_type.as_deref() {
                    visit(type_);
                }
            }
        }
        // A `(statements, tail)` body — blocks, loop bodies, function bodies.
        fn visit_body<'a, 'src>(
            body: &'a (NodeList<'src>, Box<Spanned<Node<'src>>>),
            visit: &mut dyn FnMut(&'a Spanned<Node<'src>>),
        ) {
            for statement in &body.0 {
                visit(statement);
            }
            visit(&body.1);
        }
        fn visit_if_branch<'a, 'src>(
            branch: &'a NodeIfBranch<'src>,
            visit: &mut dyn FnMut(&'a Spanned<Node<'src>>),
        ) {
            match branch {
                NodeIfBranch::If(if_) => {
                    visit(&if_.condition);
                    visit_body(&if_.then.0, visit);
                    if let Some(else_) = &if_.else_ {
                        visit_if_branch(&else_.0, visit);
                    }
                }
                NodeIfBranch::Else(body) => visit_body(&body.0, visit),
            }
        }

        match self {
            // Leaves.
            Node::Accessor(_)
            | Node::Bool(_)
            | Node::Error
            | Node::Import(_)
            | Node::Jump(_)
            | Node::LiftBinder
            | Node::LiftHole(_)
            | Node::MacroInvocation(..)
            | Node::Null
            | Node::Number(..)
            | Node::String(_)
            | Node::MultilineString(_)
            | Node::Use(_)
            | Node::Void => {}
            Node::AccessorWithGenerics(_, arguments) => {
                for argument in &arguments.0 {
                    visit(argument);
                }
            }
            Node::Css(body) => visit_css_body(body, visit),
            Node::Element(body) => {
                for item in &body.head {
                    match item {
                        ElementHeadItem::Chain(link) => visit(link),
                        ElementHeadItem::Event(_, handler) => visit(handler),
                        ElementHeadItem::Attribute(_, value) => {
                            if let Some(value) = value {
                                visit(value);
                            }
                        }
                    }
                }
                for child in &body.children {
                    visit(child.node());
                }
            }
            Node::Async(inner)
            | Node::Await(inner)
            | Node::Dereference(inner)
            | Node::Derive(_, inner)
            | Node::Export(inner)
            | Node::Reference(_, inner)
            | Node::Service(_, inner)
            | Node::TryAssert(inner)
            | Node::Lifted(inner)
            | Node::LiftGroup(inner)
            | Node::Spread(inner)
            | Node::Unary(_, inner) => visit(inner),
            Node::StaticAccessor(subject, _, generic_arguments) => {
                visit(subject);
                for argument in generic_arguments.iter().flat_map(|arguments| &arguments.0) {
                    visit(argument);
                }
            }
            Node::LiftRegion(steps, body) => {
                for (step, _) in steps {
                    visit(step);
                }
                visit(body);
            }
            Node::TypeBinder(_, bounds) => {
                for bound in bounds {
                    visit(bound);
                }
            }
            Node::Assign(target, _, value) => {
                visit(target);
                visit(value);
            }
            Node::Binary(_, left, right) => {
                visit(left);
                visit(right);
            }
            Node::Block(body) | Node::MacroBlock(body) => visit_body(&body.0, visit),
            Node::TypeWithContexts(inner, _) => visit(inner),
            Node::Call(subject, generic_arguments, arguments) => {
                visit(subject);
                for argument in generic_arguments.iter().flat_map(|arguments| &arguments.0) {
                    visit(argument);
                }
                for argument in &arguments.0 {
                    visit(argument);
                }
            }
            Node::Closure(closure) => {
                visit_parameters(&closure.parameters, visit);
                if let Some(return_type) = closure.return_type.as_deref() {
                    visit(return_type);
                }
                visit(&closure.return_value);
            }
            Node::ClosureType(parameters, return_type) => {
                for (_, type_) in &parameters.0 {
                    visit(type_);
                }
                if let Some(return_type) = return_type.as_deref() {
                    visit(return_type);
                }
            }
            Node::AsyncType(inner) => visit(inner),
            Node::SyncType(inner) => visit(inner),
            Node::Const(inner) => visit(inner),
            Node::MappedType {
                source, template, ..
            } => {
                visit(source);
                visit(template);
            }
            Node::TupleComprehension { source, body, .. } => {
                visit(source);
                visit(body);
            }
            Node::Enum(_, generic_parameters, _resource, variants) => {
                visit_generic_parameters(generic_parameters, visit);
                for (_, data, _) in variants.0.iter().map(|variant| &variant.0) {
                    for type_ in data {
                        visit(type_);
                    }
                }
            }
            Node::For(condition, body) => {
                if let Some(condition) = condition.as_deref() {
                    visit(condition);
                }
                visit_body(&body.0, visit);
            }
            Node::ForIn(_, iterable, body) => {
                visit(iterable);
                visit_body(&body.0, visit);
            }
            Node::Func(function) | Node::MacroFun(function) => {
                visit_generic_parameters(&function.generic_parameters, visit);
                visit_parameters(&function.parameters, visit);
                if let Some(return_type) = function.return_type.as_deref() {
                    visit(return_type);
                }
                if let Some(body) = &function.body {
                    visit_body(&body.0, visit);
                }
            }
            Node::MacroAttribute(_, _, _, item) => visit(item),
            Node::FuncReturn(value) => {
                if let Some(value) = value.as_deref() {
                    visit(value);
                }
            }
            Node::Lift(subject, continuation) => {
                visit(subject);
                visit(continuation);
            }
            Node::If(branch) => visit_if_branch(branch, visit),
            Node::Is(subject, pattern) => {
                visit(subject);
                visit_pattern(&pattern.0, visit);
            }
            Node::Impl(subject, traits, body) => {
                visit(subject);
                for trait_ in traits {
                    visit(trait_);
                }
                for member in &body.0 {
                    visit(member);
                }
            }
            Node::Let(_, type_, value, _) => {
                if let Some(type_) = type_.as_deref() {
                    visit(type_);
                }
                if let Some(value) = value.as_deref() {
                    visit(value);
                }
            }
            Node::LetDestructure(pattern, type_, value, _) => {
                visit_pattern(&pattern.0, visit);
                if let Some(type_) = type_.as_deref() {
                    visit(type_);
                }
                if let Some(value) = value.as_deref() {
                    visit(value);
                }
            }
            Node::List(items) | Node::Tuple(items) => {
                for item in items {
                    visit(item);
                }
            }
            Node::Repeat(value, length) | Node::ArrayType(value, length) => {
                visit(value);
                visit(length);
            }
            Node::Match(subject, legs) => {
                visit(subject);
                for (patterns, guard, body) in &legs.0 {
                    for (pattern, _) in patterns {
                        visit_pattern(pattern, visit);
                    }
                    if let Some(guard) = guard.as_deref() {
                        visit(guard);
                    }
                    visit(body);
                }
            }
            Node::MemberAccessor(subject, member) | Node::Index(subject, member) => {
                visit(subject);
                visit(member);
            }
            Node::Module(_, body) => {
                for statement in &body.0 {
                    visit(statement);
                }
            }
            Node::Struct(_, generic_parameters, _, _resource, fields) => {
                visit_generic_parameters(generic_parameters, visit);
                for (_, type_, _) in fields
                    .iter()
                    .flat_map(|fields| &fields.0)
                    .map(|field| &field.0)
                {
                    if let Some(type_) = type_ {
                        visit(type_);
                    }
                }
            }
            Node::StructInitializer(_, generic_arguments, fields) => {
                for argument in generic_arguments.iter().flat_map(|arguments| &arguments.0) {
                    visit(argument);
                }
                for (_, value) in fields.0.iter().map(|field| &field.0) {
                    if let Some(value) = value {
                        visit(value);
                    }
                }
            }
            Node::Trait(_, generic_parameters, supertraits, body) => {
                visit_generic_parameters(generic_parameters, visit);
                for supertrait in supertraits {
                    visit(supertrait);
                }
                for member in &body.0 {
                    visit(member);
                }
            }
        }
    }
}

// One enum variant: name, the types of its optional data, and an optional
// explicit backing value (`Less = -1`, `Start = "start"`).
pub type EnumVariant<'src> = (
    &'src str,
    Vec<Spanned<Node<'src>>>,
    Option<BackingLiteral<'src>>,
);

// An explicit enum backing value, `= ( (-)? NUMBER | STRING )`
// (proposal/backed-enums.md §3.1). The production GENERALIZES the integer
// discriminant that already existed rather than introducing a second kind of
// enum: a payload-free variant may carry a compile-time-constant scalar, and
// an enum whose variants carry one lowers to that scalar bare.
//
// Both arms are carried in the PARTS THE LEXER SAW rather than as a value
// (B79). For a number the grammar's production is an integer, but the token
// also admits a fraction (`1.5`) and a type suffix (`1u32`), and its whole
// part may not fit an `i64` — spellings the parser used to reduce with
// `unwrap_or(0)`, silently turning an overflow into the perfectly ordinary
// discriminant `0`. For a string the text is the raw source slice, escapes
// unprocessed, exactly as a `Node::String` carries it. Keeping the spelling
// lets the analyzer reject what the production does not mean and quote it back.
#[derive(Clone, Debug, PartialEq)]
pub enum BackingLiteral<'src> {
    Int {
        // A leading `-` on the magnitude.
        negative: bool,
        // The number token's whole part: decimal digits, or `0x` + hex digits.
        whole: &'src str,
        // The fractional part, when the literal was written `1.5`.
        fraction: Option<&'src str>,
        // A trailing type suffix (`1u32`) or unknown trailer (`1_000` lexes as
        // `1` with the suffix `_000`).
        suffix: Option<&'src str>,
        // The whole `(-)? NUMBER` span — what a diagnostic points at.
        span: Span,
    },
    Str {
        // The raw text between the quotes, escapes unprocessed.
        text: &'src str,
        // The whole `"…"` span, quotes included.
        span: Span,
    },
}

impl BackingLiteral<'_> {
    // What a diagnostic points at: the literal, not the `=`.
    pub fn span(&self) -> Span {
        match self {
            Self::Int { span, .. } | Self::Str { span, .. } => *span,
        }
    }
}

impl std::fmt::Display for BackingLiteral<'_> {
    // The literal exactly as written, so the formatter round-trips it and a
    // diagnostic quotes what the author typed — quotes included for a string,
    // which is what makes `1` and `"1"` distinguishable in a mixed-backing
    // message.
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Int {
                negative,
                whole,
                fraction,
                suffix,
                ..
            } => {
                if *negative {
                    out.write_str("-")?;
                }
                out.write_str(whole)?;
                if let Some(fraction) = fraction {
                    write!(out, ".{fraction}")?;
                }
                if let Some(suffix) = suffix {
                    out.write_str(suffix)?;
                }
                Ok(())
            }
            Self::Str { text, .. } => write!(out, "\"{text}\""),
        }
    }
}

// One struct field: its name (with the name's own span), optional type
// annotation, and whether it is `[expose]`d — observable by a service's client
// as a mirrored `Source` (`proposal/transport-rpc.md` §4.2).
pub type StructField<'src> = (Spanned<&'src str>, Option<Spanned<Node<'src>>>, bool);

// One field of a struct LITERAL: its name, and the value assigned to it —
// `None` for the shorthand form, where the name is also the value's binding.
pub type StructInitializerField<'src> = (&'src str, Option<Spanned<Node<'src>>>);

// A match-leg pattern.
#[derive(Debug)]
pub enum Pattern<'src> {
    // `_` — matches anything without binding it.
    Wildcard,
    // `let x` / `mut x` — matches anything, capturing the value. The third field
    // is the span of the NAME alone. It is carried rather than derived because a
    // binding's enclosing `Spanned` span is not uniform: a `match`/`is` capture's
    // span covers the `let `/`mut ` keyword too, while the same `Pattern::Binding`
    // reached as an element of a binder tuple/array (`Some(let (a, b))`) carries
    // the bare identifier. Consumers that paint or anchor the name — semantic
    // tokens, inlay hints — must read this field; reconstructing the name span by
    // arithmetic on the enclosing span cannot tell those two cases apart (E111).
    Binding(&'src str, bool, Span),
    // A path to an enum variant with optional payload patterns: a bare `Name`
    // (`["Name"]`) or a qualified `Enum::Variant` (`["Enum", "Variant"]`).
    Variant(Vec<&'src str>, Option<Vec<Spanned<Pattern<'src>>>>),
    // `(a, b, ...)` — a tuple pattern.
    Tuple(Vec<Spanned<Pattern<'src>>>),
    // `[a, b, c]` — a fixed-array binder pattern (`let [a, b, c] = arr`,
    // fixed-arrays.md §7): irrefutable, its element count must equal the
    // array type's length. Binder positions only (`let`, parameters) in v1.
    Array(Vec<Spanned<Pattern<'src>>>),
    // A literal value pattern (`"quit"`, `42`, `true`): matches by equality,
    // binding nothing. Holds the literal as its node.
    Literal(Box<Spanned<Node<'src>>>),
}

// One match leg: the patterns it matches (more than one is an or-pattern,
// `"y", "" => ..`), an optional `if` guard, and the body.
pub type MatchLeg<'src> = (
    Vec<Spanned<Pattern<'src>>>,
    Option<Box<Spanned<Node<'src>>>>,
    Spanned<Node<'src>>,
);

#[derive(Clone, Copy, Debug)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    // Truncated remainder (the dividend's sign) — Rust's and JS's shared
    // semantics. Exact for every integer type, so unlike `Div` it needs no
    // trunc wrap in emission.
    Rem,
    // Bitwise/shift operators (proposal/bits-and-bytes.md §2) — integer-typed,
    // overloadable via `std::operators` like the arithmetic four. Vilan
    // precedence (Rust's order, not C's): `<< >>` over `&` over `^` over `|`,
    // all over comparisons.
    Shl,
    Shr,
    // JS-only: the logical right shift `>>>`. The parser never produces it —
    // the transformer rewrites `Shr` to it when the operand type is `u32`
    // (JS `>>` is arithmetic, which is `i32`'s semantics, not `u32`'s).
    UShr,
    BitAnd,
    BitXor,
    BitOr,
    Eq,
    NotEq,
    Lt,
    Gt,
    LtEq,
    GtEq,
    // Logical AND (`&&`), also produced by the compiler for nested
    // match-pattern tests.
    And,
    // Logical OR (`||`). Binds looser than `&&`.
    Or,
}

/// Visits every expression position inside a `css` block, at any nesting
/// depth: each declaration value's holes and each nested rule's head
/// arguments. Free rather than a method so [`Node::for_each_child`]'s
/// borrow-shaped visitor can recurse through a `CssBody`, which is not a node.
fn visit_css_body<'a, 'src>(
    body: &'a CssBody<'src>,
    visit: &mut dyn FnMut(&'a Spanned<Node<'src>>),
) {
    for item in &body.items {
        match item {
            CssItem::Declaration(declaration) => {
                for piece in &declaration.value {
                    if let CssValuePiece::Hole(expression, _) = piece {
                        visit(expression);
                    }
                }
            }
            CssItem::Nested(nested) => {
                for argument in &nested.arguments {
                    visit(argument);
                }
                visit_css_body(&nested.body, visit);
            }
        }
    }
}
